//! Indexed hit collection for [`crate::engine::Engine`].
//!
//! Replaces the per-block `O(clips * notes)` linear scan with:
//! - an `O(log P)` / `O(1)` pattern-id -> index lookup built once per `Song`
//!   snapshot (incrementally reusing unchanged `Arc<Pattern>` indexes), and
//! - per-pattern sorted `(note.start, note_index)` tables queried via binary
//!   search over the conservative tick window overlapping the current audio block.
//!
//! Hit ordering matches the original linear scan + stable `sort_by_key(|h| h.at)`
//! bit-for-bit by sorting with the total tie-break key `(at, clip, note, rep)`
//! using `sort_unstable_by_key` (which also avoids heap allocations on the RT
//! thread).

use std::sync::Arc;

use crate::engine::Hit;
use crate::sequence::{
    BAR, Clip, InstrumentKind, Pattern, STEP, Song, Tick, seconds_per_step, steps, swing_offset,
};
use crate::tempo::TempoTable;
use crate::vocal::VocalMap;

/// Sorted note index for a single `Pattern`.
///
/// Holding `Arc<Pattern>` guarantees that `Arc::make_mut` on the `Song` side
/// cannot mutate the pattern in-place while this index exists, making
/// `Arc::ptr_eq` a sound cache key for incremental rebuilds.
#[derive(Clone, Debug)]
struct PatternIndex {
    pattern: Arc<Pattern>,
    /// `(note.start, note_index)` sorted by `(start, note_index)`.
    notes: Vec<(Tick, u32)>,
}

impl PatternIndex {
    fn build(pattern: &Arc<Pattern>) -> Self {
        let mut notes: Vec<(Tick, u32)> = pattern
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.start, i as u32))
            .collect();
        notes.sort_unstable_by_key(|&(start, ni)| (start, ni));
        Self {
            pattern: Arc::clone(pattern),
            notes,
        }
    }
}

/// Precomputed pattern and clip lookup tables for a [`Song`] snapshot.
///
/// Built off the audio thread in [`crate::link::SongSender`] for live playback
/// (so the RT audio callback never allocates, frees, or sorts on snapshot
/// swaps), or synced lazily inside [`crate::engine::Engine`] for offline renders.
#[derive(Clone, Debug, Default)]
pub struct HitIndex {
    /// `(pattern_id, pattern_index)` sorted by `pattern_id`, keeping the first
    /// index if duplicate ids exist (matching `patterns.iter().position`).
    pattern_by_id: Vec<(u64, usize)>,
    /// Per-pattern sorted note indexes, aligned with `song.patterns`.
    patterns: Vec<Arc<PatternIndex>>,
    /// Resolved `song.patterns` index for each clip in `song.clips`.
    clip_patterns: Vec<Option<usize>>,
    /// Snapshot of `song.clips` used to detect in-place clip edits in offline
    /// `Engine::process` calls. Empty when built for an immutable `Arc<Song>`.
    clips_snapshot: Vec<Clip>,
}

impl HitIndex {
    /// Build a fresh index for `song`, reusing unchanged `Arc<Pattern>` indexes
    /// from `prev` when available.
    pub fn build_incremental(song: &Song, prev: Option<&HitIndex>) -> Self {
        let mut index = Self::default();
        index.rebuild_from(song, prev, false);
        index
    }

    /// Lazily synchronize this index with `song` (for offline `Engine::process`
    /// where no prebuilt `HitIndex` was supplied by `SongReceiver`).
    pub(crate) fn sync(&mut self, song: &Song) {
        let patterns_match = self.patterns.len() == song.patterns.len()
            && self
                .patterns
                .iter()
                .zip(song.patterns.iter())
                .all(|(cached, pat)| Arc::ptr_eq(&cached.pattern, pat));
        if patterns_match && self.clips_snapshot == song.clips {
            return;
        }
        let prev = std::mem::take(self);
        self.rebuild_from(song, Some(&prev), true);
    }

    fn rebuild_from(&mut self, song: &Song, prev: Option<&HitIndex>, keep_clips_snapshot: bool) {
        self.pattern_by_id.clear();
        self.pattern_by_id.reserve(song.patterns.len());
        for (pi, p) in song.patterns.iter().enumerate() {
            self.pattern_by_id.push((p.id, pi));
        }
        self.pattern_by_id
            .sort_unstable_by_key(|&(id, pi)| (id, pi));
        self.pattern_by_id.dedup_by_key(|&mut (id, _)| id);

        self.patterns.clear();
        self.patterns.reserve(song.patterns.len());
        for (pi, pat) in song.patterns.iter().enumerate() {
            let reused = prev.and_then(|old| {
                // Fast path: pattern stayed at the same index.
                if let Some(entry) = old.patterns.get(pi)
                    && Arc::ptr_eq(&entry.pattern, pat)
                {
                    return Some(Arc::clone(entry));
                }
                // Fallback: pattern moved to another index (e.g. insert/delete).
                old.pattern_index(pat.id).and_then(|old_pi| {
                    let entry = &old.patterns[old_pi];
                    Arc::ptr_eq(&entry.pattern, pat).then(|| Arc::clone(entry))
                })
            });
            self.patterns
                .push(reused.unwrap_or_else(|| Arc::new(PatternIndex::build(pat))));
        }

        self.clip_patterns.clear();
        self.clip_patterns.reserve(song.clips.len());
        for c in &song.clips {
            self.clip_patterns.push(self.pattern_index(c.pattern));
        }

        self.clips_snapshot.clear();
        if keep_clips_snapshot {
            self.clips_snapshot.extend_from_slice(&song.clips);
        }
    }

    #[inline]
    pub fn pattern_index(&self, id: u64) -> Option<usize> {
        self.pattern_by_id
            .binary_search_by_key(&id, |&(pid, _)| pid)
            .ok()
            .map(|i| self.pattern_by_id[i].1)
    }

    /// Collect all hits in `[from, to)` using the prebuilt pattern/note index.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn collect_hits(
        &self,
        song: &Song,
        tempo: Option<&TempoTable>,
        rate: f64,
        vocal: &VocalMap,
        pattern: u64,
        song_mode: bool,
        from: u64,
        to: u64,
        hits: &mut Vec<Hit>,
    ) {
        hits.clear();
        if from >= to {
            return;
        }
        let max_lead = vocal.max_lead as u64;
        let max_to = to.saturating_add(max_lead);
        if song_mode {
            for (ci, c) in song.clips.iter().enumerate() {
                if c.muted || song.track_muted(c.track) {
                    continue;
                }
                let pi = self
                    .clip_patterns
                    .get(ci)
                    .copied()
                    .flatten()
                    .or_else(|| self.pattern_index(c.pattern));
                let Some(pi) = pi else { continue };
                let p = &song.patterns[pi];
                let offset = clip_frame(song, tempo, rate, c.start);
                let clip_len = c.length.unwrap_or(p.length - c.offset).max(1);
                let window = (c.offset, c.offset + clip_len);
                let span = clip_frame(song, tempo, rate, window.1 - window.0);
                // A phrase may start before its clip by its preutterance.
                if offset < max_to && offset.saturating_add(span) > from {
                    add_indexed(
                        hits,
                        song,
                        tempo,
                        rate,
                        vocal,
                        song_mode,
                        from,
                        to,
                        offset,
                        c.start,
                        pi,
                        p,
                        &self.patterns[pi],
                        window,
                        ci as u32,
                    );
                }
            }
        } else if let Some(pi) = self.pattern_index(pattern) {
            let p = &song.patterns[pi];
            add_indexed(
                hits,
                song,
                tempo,
                rate,
                vocal,
                song_mode,
                from,
                to,
                0,
                0,
                pi,
                p,
                &self.patterns[pi],
                (0, p.length),
                0,
            );
        }
        hits.sort_unstable_by_key(|h| (h.at, h.clip, h.note, h.rep));
    }
}

/// Zero-allocation fallback used only when `Engine::reserve` was called (RT
/// mode) and the caller invoked `Engine::process` without a prebuilt
/// [`HitIndex`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_hits_unindexed(
    song: &Song,
    tempo: Option<&TempoTable>,
    rate: f64,
    vocal: &VocalMap,
    pattern: u64,
    song_mode: bool,
    from: u64,
    to: u64,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    if from >= to {
        return;
    }
    let max_to = to.saturating_add(vocal.max_lead as u64);
    if song_mode {
        for (ci, c) in song.clips.iter().enumerate() {
            if c.muted || song.track_muted(c.track) {
                continue;
            }
            let Some(pi) = song.patterns.iter().position(|p| p.id == c.pattern) else {
                continue;
            };
            let p = &song.patterns[pi];
            let offset = clip_frame(song, tempo, rate, c.start);
            let clip_len = c.length.unwrap_or(p.length - c.offset).max(1);
            let window = (c.offset, c.offset + clip_len);
            let span = clip_frame(song, tempo, rate, window.1 - window.0);
            if offset < max_to && offset.saturating_add(span) > from {
                add_scan(
                    hits, song, tempo, rate, vocal, song_mode, from, to, offset, c.start, pi, p,
                    window, ci as u32,
                );
            }
        }
    } else if let Some(pi) = song.patterns.iter().position(|p| p.id == pattern) {
        let p = &song.patterns[pi];
        add_scan(
            hits,
            song,
            tempo,
            rate,
            vocal,
            song_mode,
            from,
            to,
            0,
            0,
            pi,
            p,
            (0, p.length),
            0,
        );
    }
    hits.sort_unstable_by_key(|h| (h.at, h.clip, h.note, h.rep));
}

/// Frames from the start of a pattern to tick `t`, swing included.
#[inline]
pub(crate) fn note_frame(song: &Song, rate: f64, t: Tick) -> u64 {
    let sps = seconds_per_step(song.tempo);
    ((steps(t) * sps + swing_offset(t, sps, song.swing.clamp(0.0, 0.6))) * rate).ceil() as u64
}

/// Frames from the song start to playlist tick `t`. Whole bars count in
/// truncated bar frames, as format 1 did, so old songs keep their timing.
#[inline]
pub(crate) fn clip_frame(song: &Song, tempo: Option<&TempoTable>, rate: f64, t: Tick) -> u64 {
    if let Some(table) = tempo {
        return table.frames_at_ceil(t);
    }
    let sps = seconds_per_step(song.tempo);
    let bar = (16.0 * sps * rate) as u64;
    let rest = t.rem_euclid(BAR);
    (t / BAR) as u64 * bar
        + if rest == 0 {
            0
        } else {
            (steps(rest) * sps * rate).ceil() as u64
        }
}

#[allow(clippy::too_many_arguments)]
fn add_indexed(
    hits: &mut Vec<Hit>,
    song: &Song,
    tempo: Option<&TempoTable>,
    rate: f64,
    vocal: &VocalMap,
    song_mode: bool,
    from: u64,
    to: u64,
    offset: u64,
    start: Tick,
    pi: usize,
    p: &Pattern,
    pat_idx: &PatternIndex,
    window: (Tick, Tick),
    clip_order: u32,
) {
    if window.1 <= window.0 || pat_idx.notes.is_empty() {
        return;
    }
    let max_lead = vocal.max_lead as u64;
    let swing = song.swing.clamp(0.0, 0.6);

    // Conservative `[local_lo, local_hi)` tick range inside `window` that could
    // produce a hit in `[from, to)` after accounting for swing, rounding, and
    // vocal preutterance lead.
    let (local_lo, local_hi) = match tempo {
        Some(table) => {
            let max_swing_frames =
                (swing * seconds_per_step(crate::tempo::TEMPO_MIN) * rate).ceil() as u64;
            let tick_lo_song =
                table.tick_at_frame(from.saturating_sub(max_swing_frames.saturating_add(1)));
            let tick_hi_song = table.tick_at_frame(to.saturating_add(max_lead));
            (
                (tick_lo_song - start - 2).max(0),
                (tick_hi_song - start + 3).max(0),
            )
        }
        None => {
            let max_to = to.saturating_add(max_lead);
            if offset >= max_to {
                return;
            }
            let frames_per_step = seconds_per_step(song.tempo) * rate;
            let frame_lo = from.saturating_sub(offset);
            let frame_hi = max_to - offset;
            let lo = ((((frame_lo as f64 - 1.0) / frames_per_step) - swing) * STEP as f64).floor()
                as Tick
                - 1;
            let hi = ((frame_hi as f64 / frames_per_step) * STEP as f64).ceil() as Tick + 1;
            (lo.max(0), hi.max(0))
        }
    };

    let t_lo = window.0.saturating_add(local_lo);
    let t_hi = window.0.saturating_add(local_hi).min(window.1);
    if t_lo >= t_hi {
        return;
    }

    let plen = p.length.max(1);
    let first = window.0.div_euclid(plen);
    let last = (window.1 - 1).div_euclid(plen);
    if first > last {
        return;
    }

    let min_start = pat_idx.notes.first().expect("non-empty").0;
    let max_start = pat_idx.notes.last().expect("non-empty").0;
    let plen128 = plen as i128;
    let k_min = (t_lo as i128 - max_start as i128).div_euclid(plen128);
    let k_max = (t_hi as i128 - 1 - min_start as i128).div_euclid(plen128);
    let k_first = (first as i128).max(k_min);
    let k_last = (last as i128).min(k_max);
    if k_first > k_last {
        return;
    }

    for k in (k_first as i64)..=(k_last as i64) {
        let shift = k as i128 * plen128;
        let lo_start = (t_lo as i128 - shift).clamp(Tick::MIN as i128, Tick::MAX as i128) as Tick;
        let hi_start = (t_hi as i128 - shift).clamp(Tick::MIN as i128, Tick::MAX as i128) as Tick;
        if lo_start >= hi_start {
            continue;
        }
        let start_pos = pat_idx.notes.partition_point(|&(s, _)| s < lo_start);
        for &(note_start, ni_u32) in &pat_idx.notes[start_pos..] {
            if note_start >= hi_start {
                break;
            }
            let ni = ni_u32 as usize;
            let n = &p.notes[ni];
            let Some(inst) = song.instrument(n.instrument) else {
                continue;
            };
            if n.muted || inst.muted {
                continue;
            }
            let sings = matches!(inst.kind, InstrumentKind::Vocal { .. });
            let t = n.start + k * plen;
            if t < window.0 || t >= window.1 {
                continue;
            }
            push_if_in_block(
                hits,
                song,
                tempo,
                rate,
                vocal,
                song_mode,
                from,
                to,
                offset,
                start,
                pi,
                ni,
                n.id,
                sings,
                t - window.0,
                clip_order,
                k,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_scan(
    hits: &mut Vec<Hit>,
    song: &Song,
    tempo: Option<&TempoTable>,
    rate: f64,
    vocal: &VocalMap,
    song_mode: bool,
    from: u64,
    to: u64,
    offset: u64,
    start: Tick,
    pi: usize,
    p: &Pattern,
    window: (Tick, Tick),
    clip_order: u32,
) {
    if window.1 <= window.0 {
        return;
    }
    let plen = p.length.max(1);
    let (first, last) = (window.0.div_euclid(plen), (window.1 - 1).div_euclid(plen));
    for (ni, n) in p.notes.iter().enumerate() {
        let Some(inst) = song.instrument(n.instrument) else {
            continue;
        };
        if n.muted || inst.muted {
            continue;
        }
        let sings = matches!(inst.kind, InstrumentKind::Vocal { .. });
        for k in first..=last {
            let t = n.start + k * plen;
            if t < window.0 || t >= window.1 {
                continue;
            }
            push_if_in_block(
                hits,
                song,
                tempo,
                rate,
                vocal,
                song_mode,
                from,
                to,
                offset,
                start,
                pi,
                ni,
                n.id,
                sings,
                t - window.0,
                clip_order,
                k,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn push_if_in_block(
    hits: &mut Vec<Hit>,
    song: &Song,
    tempo: Option<&TempoTable>,
    rate: f64,
    vocal: &VocalMap,
    song_mode: bool,
    from: u64,
    to: u64,
    offset: u64,
    start: Tick,
    pi: usize,
    ni: usize,
    note_id: u64,
    sings: bool,
    local: Tick,
    clip_order: u32,
    rep: i64,
) {
    // A vocal phrase starts at its first note, early by the syllable's
    // preutterance; the notes after it are inside the phrase's audio.
    let mut lead = 0usize;
    if sings {
        let key = (note_id, start + local);
        let placed = vocal.placed(song_mode);
        if placed.members.contains(&key) {
            return;
        }
        match placed.starts.get(&key) {
            Some(ph) => lead = ph.lead,
            None => return,
        }
    }
    let at = match tempo {
        // With tempo changes a clip's notes follow the map, not the clip's first tempo.
        Some(table) => {
            let sps = seconds_per_step(table.bpm_at(start + local));
            (table.frames_at(start + local)
                + swing_offset(local, sps, song.swing.clamp(0.0, 0.6)) * rate)
                .ceil() as u64
        }
        None => offset + note_frame(song, rate, local),
    };
    let (at, skip) = (
        at.saturating_sub(lead as u64),
        (lead as u64).saturating_sub(at) as usize,
    );
    if (from..to).contains(&at) {
        hits.push(Hit {
            at,
            tick: start + local,
            pattern: pi,
            note: ni,
            skip,
            clip: clip_order,
            rep,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::Note;
    use crate::tempo::{TempoMap, TempoPoint, TempoShape};

    fn assert_hits_match_scan(
        song: &Song,
        tempo: Option<&TempoTable>,
        rate: f64,
        pattern: u64,
        song_mode: bool,
        total_frames: u64,
        block: u64,
    ) {
        let vocal = VocalMap::default();
        let index = HitIndex::build_incremental(song, None);
        let mut got = Vec::new();
        let mut expected = Vec::new();
        let mut from = 0;
        while from < total_frames {
            let to = from + block;
            index.collect_hits(
                song, tempo, rate, &vocal, pattern, song_mode, from, to, &mut got,
            );
            collect_hits_unindexed(
                song,
                tempo,
                rate,
                &vocal,
                pattern,
                song_mode,
                from,
                to,
                &mut expected,
            );
            assert_eq!(
                got.len(),
                expected.len(),
                "hit count mismatch at [{from}..{to})"
            );
            for (g, e) in got.iter().zip(expected.iter()) {
                assert_eq!(
                    (g.at, g.tick, g.pattern, g.note, g.skip),
                    (e.at, e.tick, e.pattern, e.note, e.skip),
                    "hit mismatch at [{from}..{to})"
                );
            }
            from = to;
        }
    }

    #[test]
    fn indexed_matches_scan_with_swing_unsorted_notes_repeats_and_offsets() {
        let mut song = Song::default();
        song.tempo = 128.0;
        song.swing = 0.55;
        let inst = song.instruments[0].id;

        // Pattern 1: unsorted notes, chords on same tick, odd steps (swung).
        let p1 = song.pattern_mut(1).unwrap();
        p1.length = BAR;
        for &(start, pitch) in &[
            (STEP * 7, 67),
            (0, 60),
            (STEP, 62),
            (0, 64),
            (STEP * 3, 65),
            (STEP * 15, 72),
            (STEP, 63),
            (STEP * 5 + 3, 66),
        ] {
            p1.notes.push(Note::new(inst, start, STEP, pitch, 100));
        }

        // Pattern 2: short pattern that repeats many times inside a clip.
        let mut p2 = Pattern::new(2, "Short");
        p2.length = STEP * 3;
        p2.notes.push(Note::new(inst, STEP * 2, STEP, 48, 90));
        p2.notes.push(Note::new(inst, 0, STEP, 50, 90));
        song.patterns.push(Arc::new(p2));

        // Clips: normal, sliced offset, repeated beyond pattern length, muted.
        song.clips.push(Clip::new(1, 0, 0));
        song.clips.push(Clip {
            pattern: 1,
            start: BAR / 2,
            track: 1,
            offset: STEP * 3,
            length: Some(BAR * 2),
            muted: false,
        });
        song.clips.push(Clip {
            pattern: 2,
            start: STEP * 2,
            track: 2,
            offset: STEP,
            length: Some(BAR * 2),
            muted: false,
        });
        song.clips.push(Clip {
            pattern: 1,
            start: 0,
            track: 3,
            offset: 0,
            length: None,
            muted: true,
        });

        let rate = 48_000.0;
        assert_hits_match_scan(&song, None, rate, 1, true, 48_000 * 6, 128);
        assert_hits_match_scan(&song, None, rate, 1, false, 48_000 * 3, 128);

        // Also test with a tempo map (step + linear ramp).
        song.tempo_map = TempoMap {
            points: vec![
                TempoPoint {
                    tick: 0,
                    bpm: 90.0,
                    shape: TempoShape::Linear,
                },
                TempoPoint {
                    tick: BAR * 2,
                    bpm: 180.0,
                    shape: TempoShape::Step,
                },
                TempoPoint {
                    tick: BAR * 3,
                    bpm: 60.0,
                    shape: TempoShape::Step,
                },
            ],
        };
        let table = TempoTable::build(&song.tempo_map, song.tempo, 48_000);
        assert_hits_match_scan(&song, Some(&table), rate, 1, true, 48_000 * 6, 128);
    }

    #[test]
    fn incremental_rebuild_reuses_unchanged_patterns() {
        let mut song = Song::default();
        let inst = song.instruments[0].id;
        for id in 2..=10 {
            let mut p = Pattern::new(id, &format!("P{id}"));
            p.notes
                .push(Note::new(inst, STEP * (id as Tick), STEP, 60, 100));
            song.patterns.push(Arc::new(p));
        }
        let first = HitIndex::build_incremental(&song, None);

        // Edit only pattern 5 via COW.
        song.pattern_mut(5)
            .unwrap()
            .notes
            .push(Note::new(inst, 0, STEP, 64, 100));
        let second = HitIndex::build_incremental(&song, Some(&first));

        for (i, (a, b)) in first
            .patterns
            .iter()
            .zip(second.patterns.iter())
            .enumerate()
        {
            let id = song.patterns[i].id;
            assert_eq!(Arc::ptr_eq(a, b), id != 5, "pattern {id} reuse mismatch");
        }
    }

    #[test]
    #[ignore]
    fn bench_collect_hits_500_clips_2000_notes() {
        let mut song = Song::default();
        song.tempo = 120.0;
        let inst = song.instruments[0].id;
        song.patterns.clear();
        let pat_count = 100u64;
        let notes_per_pat = 2000usize;
        let pat_len = BAR * 16;
        for pid in 1..=pat_count {
            let mut p = Pattern::new(pid, &format!("P{pid}"));
            p.length = pat_len;
            for i in 0..notes_per_pat {
                // Intentionally unsorted order to stress indexing.
                let start = ((i * 7919) % (pat_len as usize)) as Tick;
                p.notes
                    .push(Note::new(inst, start, STEP, 48 + (i % 36) as u8, 100));
            }
            song.patterns.push(Arc::new(p));
        }
        // 500 clips stacked across the first 4 bars so many overlap every block.
        for ci in 0..500usize {
            let pid = (ci as u64 % pat_count) + 1;
            let start = (ci % 4) as Tick * BAR;
            song.clips.push(Clip::new(pid, start, (ci % 32) as u32));
        }

        let index = HitIndex::build_incremental(&song, None);
        let vocal = VocalMap::default();
        let rate = 48_000.0;
        let block = 128u64;
        let blocks = 500u64;
        let mut hits_idx = Vec::with_capacity(256);
        let mut hits_scan = Vec::with_capacity(256);

        // Verify exact equivalence over the benchmark window first.
        for b in 0..20 {
            let from = b * block;
            let to = from + block;
            index.collect_hits(&song, None, rate, &vocal, 1, true, from, to, &mut hits_idx);
            collect_hits_unindexed(&song, None, rate, &vocal, 1, true, from, to, &mut hits_scan);
            assert_eq!(hits_idx.len(), hits_scan.len());
        }

        let t0 = std::time::Instant::now();
        let mut total_idx = 0usize;
        for b in 0..blocks {
            let from = b * block;
            let to = from + block;
            index.collect_hits(&song, None, rate, &vocal, 1, true, from, to, &mut hits_idx);
            total_idx += hits_idx.len();
        }
        let idx_us = t0.elapsed().as_secs_f64() * 1e6 / blocks as f64;

        let t1 = std::time::Instant::now();
        let mut total_scan = 0usize;
        for b in 0..blocks {
            let from = b * block;
            let to = from + block;
            collect_hits_unindexed(&song, None, rate, &vocal, 1, true, from, to, &mut hits_scan);
            total_scan += hits_scan.len();
        }
        let scan_us = t1.elapsed().as_secs_f64() * 1e6 / blocks as f64;

        assert_eq!(total_idx, total_scan);
        let speedup = scan_us / idx_us.max(1e-9);
        eprintln!(
            "collect_hits (500 clips x 2000 notes, 128-frame blocks): indexed = {idx_us:.2} us/block, scan = {scan_us:.2} us/block, speedup = {speedup:.1}x (hits = {total_idx})"
        );
        // Soft threshold: indexed block must stay well below 200 us (2.67 ms budget at 128 frames).
        assert!(
            idx_us < 200.0,
            "indexed collect_hits too slow: {idx_us:.2} us/block"
        );
    }
}
