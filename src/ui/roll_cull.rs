//! Viewport culling, sorted range queries, and optional frame profiling for the piano roll.
//!
//! `Pattern::notes` is not required to stay sorted (z-order follows insertion order and drags
//! mutate `start` in place), so `RollIndex` maintains a compact sorted index with per-block
//! maximum end ticks. Queries are O(log N + K) for visible ranges, point hits, and event-lane
//! stems while never culling long notes that start off-screen to the left.

use super::roll::Geom;
use eframe::egui::Pos2;
use keyframexl::sequence::{Note, STEP, Tick};
use std::collections::HashSet;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const BLOCK_SIZE: usize = 64;

/// Compact entry in the start-sorted index.
#[derive(Clone, Copy, Debug)]
struct Entry {
    start: Tick,
    end: Tick,
    orig_idx: u32,
}

/// Fast selection membership check: linear scan for small selections (zero allocation),
/// `HashSet` for large selections so `paint_notes` / `event_lane` never degrade to O(N * S).
pub(crate) enum SelLookup<'a> {
    Slice(&'a [u64]),
    Set(HashSet<u64>),
}

impl<'a> SelLookup<'a> {
    pub(crate) fn new(sel: &'a [u64]) -> Self {
        if sel.len() <= 16 {
            Self::Slice(sel)
        } else {
            Self::Set(sel.iter().copied().collect())
        }
    }

    #[inline]
    pub(crate) fn contains(&self, id: u64) -> bool {
        match self {
            Self::Slice(s) => s.contains(&id),
            Self::Set(s) => s.contains(&id),
        }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Slice(s) => s.len(),
            Self::Set(s) => s.len(),
        }
    }
}

/// Cached spatial index for the currently open pattern's notes.
#[derive(Default, Debug)]
pub(crate) struct RollIndex {
    pattern_id: u64,
    notes_len: usize,
    fingerprint: u64,
    entries: Vec<Entry>,
    /// Maximum `end` tick within each `BLOCK_SIZE`-entry chunk of `entries`.
    block_max_end: Vec<Tick>,
    /// Longest note length in the pattern (ticks).
    max_len: Tick,
}

/// Fast non-cryptographic fingerprint over the fields that affect culling and hit-testing.
#[inline]
fn fingerprint_notes(notes: &[Note]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ (notes.len() as u64);
    for n in notes {
        h = h.wrapping_mul(0x100000001b3) ^ n.id;
        h = h.wrapping_mul(0x100000001b3) ^ (n.start as u64);
        h = h.wrapping_mul(0x100000001b3) ^ (n.length as u64);
        h = h.wrapping_mul(0x100000001b3) ^ ((n.pitch as u64) | (n.instrument << 8));
    }
    h
}

impl RollIndex {
    /// Ensure the index matches `notes` of `pattern_id`, rebuilding only when changed.
    pub(crate) fn sync(&mut self, pattern_id: u64, notes: &[Note]) {
        let fp = fingerprint_notes(notes);
        if self.pattern_id == pattern_id && self.notes_len == notes.len() && self.fingerprint == fp
        {
            return;
        }
        self.rebuild(pattern_id, notes, fp);
    }

    /// Build a fresh index (used by tests).
    #[cfg(test)]
    pub(crate) fn build(pattern_id: u64, notes: &[Note]) -> Self {
        let mut idx = Self::default();
        let fp = fingerprint_notes(notes);
        idx.rebuild(pattern_id, notes, fp);
        idx
    }

    /// Whether the cached index currently matches `pattern_id` and `notes.len()`.
    #[inline]
    pub(crate) fn matches(&self, pattern_id: u64, notes: &[Note]) -> bool {
        self.pattern_id == pattern_id
            && self.notes_len == notes.len()
            && self.entries.len() == notes.len()
    }

    fn rebuild(&mut self, pattern_id: u64, notes: &[Note], fp: u64) {
        self.pattern_id = pattern_id;
        self.notes_len = notes.len();
        self.fingerprint = fp;
        self.entries.clear();
        self.entries.reserve(notes.len());
        let mut max_len: Tick = 0;
        for (i, n) in notes.iter().enumerate() {
            if n.length > max_len {
                max_len = n.length;
            }
            self.entries.push(Entry {
                start: n.start,
                end: n.end(),
                orig_idx: i as u32,
            });
        }
        self.max_len = max_len;
        self.entries.sort_unstable_by_key(|e| (e.start, e.orig_idx));
        self.block_max_end.clear();
        for chunk in self.entries.chunks(BLOCK_SIZE) {
            let m = chunk.iter().map(|e| e.end).max().unwrap_or(Tick::MIN);
            self.block_max_end.push(m);
        }
    }

    /// Longest note length currently indexed.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn max_len(&self) -> Tick {
        self.max_len
    }

    /// Visit every note whose `[start, end]` overlaps `[vis_start, vis_end]`
    /// (inclusive: `n.end() >= vis_start && n.start <= vis_end`).
    ///
    /// Uses binary search on `start <= vis_end` and skips entire 64-note blocks whose
    /// `block_max_end < vis_start`, so long notes starting off-screen to the left are
    /// never culled.
    pub(crate) fn for_each_overlapping<F>(
        &self,
        notes: &[Note],
        vis_start: Tick,
        vis_end: Tick,
        mut f: F,
    ) where
        F: FnMut(usize, &Note),
    {
        if self.entries.is_empty() || vis_end < vis_start {
            return;
        }
        let last = self.entries.partition_point(|e| e.start <= vis_end);
        if last == 0 {
            return;
        }
        let n_blocks = last.div_ceil(BLOCK_SIZE);
        for b in 0..n_blocks {
            if self.block_max_end[b] < vis_start {
                continue;
            }
            let lo = b * BLOCK_SIZE;
            let hi = (lo + BLOCK_SIZE).min(last);
            for e in &self.entries[lo..hi] {
                if e.end >= vis_start {
                    let idx = e.orig_idx as usize;
                    if let Some(n) = notes.get(idx) {
                        f(idx, n);
                    }
                }
            }
        }
    }

    /// Collect indices of overlapping notes in `[vis_start, vis_end]`, sorted by original
    /// `Pattern::notes` order so z-order / overdraw matches full iteration.
    pub(crate) fn visible_indices_ordered(
        &self,
        notes: &[Note],
        vis_start: Tick,
        vis_end: Tick,
        out: &mut Vec<usize>,
    ) {
        out.clear();
        self.for_each_overlapping(notes, vis_start, vis_end, |idx, _| out.push(idx));
        out.sort_unstable();
    }

    /// Visit every note whose `start` lies in `[vis_start, vis_end]` (inclusive).
    /// Pure O(log N + K) binary search, ideal for event-lane stems drawn at `n.start`.
    pub(crate) fn for_each_starting_in<F>(
        &self,
        notes: &[Note],
        vis_start: Tick,
        vis_end: Tick,
        mut f: F,
    ) where
        F: FnMut(usize, &Note),
    {
        if self.entries.is_empty() || vis_end < vis_start {
            return;
        }
        let first = self.entries.partition_point(|e| e.start < vis_start);
        let last = self.entries.partition_point(|e| e.start <= vis_end);
        for e in &self.entries[first..last] {
            let idx = e.orig_idx as usize;
            if let Some(n) = notes.get(idx) {
                f(idx, n);
            }
        }
    }

    /// Topmost note (highest original index) of `inst` at `(at, pitch)` with `start <= at < end`.
    pub(crate) fn note_at(&self, notes: &[Note], inst: u64, at: Tick, pitch: u8) -> Option<Note> {
        let mut best: Option<(usize, Note)> = None;
        self.for_each_overlapping(notes, at, at, |idx, n| {
            if n.instrument == inst
                && n.pitch == pitch
                && at >= n.start
                && at < n.end()
                && best.is_none_or(|(bi, _)| idx > bi)
            {
                best = Some((idx, *n));
            }
        });
        best.map(|(_, n)| n)
    }

    /// Topmost ghost note's instrument under screen point `q`.
    pub(crate) fn ghost_at(
        &self,
        notes: &[Note],
        exclude_inst: u64,
        g: &Geom,
        q: Pos2,
    ) -> Option<u64> {
        // `note_rect` enforces a 3 px minimum width, so pad the tick query by ~4 px.
        let pad = ((4.0 / g.cell.max(1.0)) * STEP as f32).ceil() as Tick + 1;
        let at = g.tick_f(q.x);
        let vis_start = (at - pad).max(0);
        let vis_end = at + pad;
        let mut best: Option<(usize, u64)> = None;
        self.for_each_overlapping(notes, vis_start, vis_end, |idx, n| {
            if n.instrument != exclude_inst
                && g.visible(n.pitch)
                && g.note_rect(n.start, n.length, n.pitch).contains(q)
                && best.is_none_or(|(bi, _)| idx > bi)
            {
                best = Some((idx, n.instrument));
            }
        });
        best.map(|(_, inst)| inst)
    }

    /// Highest-pitched note of `(inst, group)` sounding at `slide_start` (`start <= s < end`).
    pub(crate) fn slide_source(
        &self,
        notes: &[Note],
        inst: u64,
        group: u8,
        slide_start: Tick,
    ) -> Option<Note> {
        let mut best: Option<Note> = None;
        self.for_each_overlapping(notes, slide_start, slide_start, |_, n| {
            if n.group == group
                && n.instrument == inst
                && n.start <= slide_start
                && n.end() > slide_start
                && best.is_none_or(|b| n.pitch >= b.pitch)
            {
                best = Some(*n);
            }
        });
        best
    }

    /// Nearest event-lane stem of `inst` within 10 px of `x`.
    /// When `sel.len() > 1`, selected stems take priority over unselected stems.
    pub(crate) fn lane_note_near(
        &self,
        notes: &[Note],
        inst: u64,
        sel: &SelLookup<'_>,
        g: &Geom,
        x: f32,
    ) -> Option<Note> {
        // Stem sits at `g.x(n.start) + 3.0`, hit radius is 10 px.
        let margin_ticks = ((14.0 / g.cell.max(0.5)) * STEP as f32).ceil() as Tick + 1;
        let center_tick = g.tick_f(x - 3.0);
        let t_lo = (center_tick - margin_ticks).max(0);
        let t_hi = center_tick + margin_ticks;

        let prefer_sel = sel.len() > 1;
        let mut best_sel: Option<(f32, Note)> = None;
        let mut best_any: Option<(f32, Note)> = None;

        self.for_each_starting_in(notes, t_lo, t_hi, |_, n| {
            if n.instrument != inst {
                return;
            }
            let d = (g.x(n.start) + 3.0 - x).abs();
            if d >= 10.0 {
                return;
            }
            if prefer_sel && sel.contains(n.id) && best_sel.is_none_or(|(bd, _)| d < bd) {
                best_sel = Some((d, *n));
            }
            if best_any.is_none_or(|(bd, _)| d < bd) {
                best_any = Some((d, *n));
            }
        });

        best_sel.or(best_any).map(|(_, n)| n)
    }

    /// Notes of `inst` intersecting a musical marquee box `[t0, t1] x [p0, p1]`.
    /// Works across the entire pattern (including off-screen notes).
    pub(crate) fn marquee_hits(
        &self,
        notes: &[Note],
        inst: u64,
        t0: Tick,
        t1: Tick,
        p0: f32,
        p1: f32,
    ) -> Vec<u64> {
        let mut hits = Vec::new();
        self.for_each_overlapping(notes, t0, t1, |_, n| {
            if n.instrument == inst
                && n.end() > t0
                && n.start <= t1
                && (n.pitch as f32 + 1.0) > p0
                && (n.pitch as f32) < p1
            {
                hits.push(n.id);
            }
        });
        hits
    }
}

/// Optional frame-time profiler enabled via `KFXL_PROFILE_ROLL=1`.
/// Prints rolling average and peak `roll_body` time every 60 frames to stderr.
pub(crate) struct RollProfileScope {
    start: Option<Instant>,
}

impl RollProfileScope {
    #[inline]
    pub(crate) fn begin() -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let on = *ENABLED.get_or_init(|| {
            std::env::var("KFXL_PROFILE_ROLL")
                .is_ok_and(|v| !v.is_empty() && v != "0" && v != "false")
        });
        Self {
            start: on.then(Instant::now),
        }
    }

    #[inline]
    pub(crate) fn finish(self, total_notes: usize) {
        let Some(t0) = self.start else {
            return;
        };
        let us = t0.elapsed().as_micros() as u64;
        static FRAMES: AtomicU64 = AtomicU64::new(0);
        static SUM_US: AtomicU64 = AtomicU64::new(0);
        static MAX_US: AtomicU64 = AtomicU64::new(0);

        let f = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
        let sum = SUM_US.fetch_add(us, Ordering::Relaxed) + us;
        MAX_US.fetch_max(us, Ordering::Relaxed);
        if f.is_multiple_of(60) {
            let avg_ms = (sum as f64 / 60.0) / 1000.0;
            let max_ms = MAX_US.swap(0, Ordering::Relaxed) as f64 / 1000.0;
            SUM_US.store(0, Ordering::Relaxed);
            eprintln!(
                "[roll-profile] frames={f} avg={avg_ms:.3}ms max={max_ms:.3}ms notes={total_notes}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Rect, pos2};
    use keyframexl::sequence::{BAR, STEP};

    fn make_note(id: u64, inst: u64, start: Tick, len: Tick, pitch: u8) -> Note {
        Note {
            id,
            ..Note::new(inst, start, len, pitch, 100)
        }
    }

    fn make_geom(x0: Tick, cell: f32, width: f32, top: u8, bottom: u8) -> Geom {
        let row = 14.0;
        let h = (top - bottom + 1) as f32 * row;
        Geom {
            grid: Rect::from_min_max(pos2(50.0, 24.0), pos2(50.0 + width, 24.0 + h)),
            cell,
            row,
            lane: Rect::NOTHING,
            top,
            bottom,
            len: 64 * BAR,
            x0,
        }
    }

    #[test]
    fn long_note_spanning_viewport_is_never_culled() {
        // Build 300 short notes plus one 32-bar pad starting at tick 0.
        let mut notes = Vec::new();
        notes.push(make_note(1, 4, 0, 32 * BAR, 60));
        for i in 0..300i64 {
            notes.push(make_note(
                (i + 2) as u64,
                4,
                i * STEP,
                STEP,
                62 + (i % 12) as u8,
            ));
        }
        let idx = RollIndex::build(1, &notes);
        assert_eq!(idx.max_len(), 32 * BAR);

        // Viewport is way out at bars 20..22 (past all 300 short notes, inside the 32-bar pad).
        let vis_start = 20 * BAR;
        let vis_end = 22 * BAR;
        let mut vis = Vec::new();
        idx.visible_indices_ordered(&notes, vis_start, vis_end, &mut vis);
        assert_eq!(
            vis,
            vec![0],
            "32-bar pad starting at 0 must stay visible at bars 20..22"
        );
    }

    #[test]
    fn edge_boundary_notes_match_full_scan() {
        let notes = vec![
            make_note(1, 4, 0, BAR, 60),            // ends right at BAR
            make_note(2, 4, BAR - 1, 1, 61),        // ends at BAR
            make_note(3, 4, BAR, STEP, 62),         // starts at BAR
            make_note(4, 4, 2 * BAR, STEP, 63),     // starts at 2*BAR (vis_end)
            make_note(5, 4, 2 * BAR + 1, STEP, 64), // strictly after vis_end
            make_note(6, 4, 0, BAR - 1, 65),        // ends strictly before BAR
        ];
        let idx = RollIndex::build(1, &notes);
        let vis_start = BAR;
        let vis_end = 2 * BAR;

        let mut got = Vec::new();
        idx.visible_indices_ordered(&notes, vis_start, vis_end, &mut got);
        let expected: Vec<usize> = notes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.end() >= vis_start && n.start <= vis_end)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn note_at_and_ghost_at_preserve_topmost_z_order_on_unsorted_notes() {
        // Deliberately unsorted notes with overlapping ranges on the same pitch.
        let notes = vec![
            make_note(10, 4, 2 * BAR, BAR, 60),
            make_note(11, 4, 0, BAR, 60),
            make_note(12, 4, STEP, BAR, 60), // later in Vec -> topmost at STEP..BAR
            make_note(20, 7, 0, BAR, 64),
            make_note(21, 9, STEP, BAR, 64), // ghost on channel 9 overlaps channel 7
        ];
        let idx = RollIndex::build(1, &notes);
        let hit = idx.note_at(&notes, 4, 2 * STEP, 60).unwrap();
        assert_eq!(hit.id, 12, "topmost overlapping note must win");

        let g = make_geom(0, 20.0, 600.0, 72, 48);
        let q = pos2(g.x(2 * STEP) + 2.0, g.y(64) + g.row / 2.0);
        let ghost = idx.ghost_at(&notes, 4, &g, q);
        assert_eq!(ghost, Some(9), "topmost overlapping ghost must win");
    }

    #[test]
    fn marquee_hits_selects_offscreen_notes() {
        let mut notes = Vec::new();
        // Put 2000 notes across 128 bars and pitches 36..84.
        for i in 0..2000i64 {
            let start = (i * STEP) % (128 * BAR);
            let pitch = 36 + (i % 48) as u8;
            notes.push(make_note((i + 1) as u64, 4, start, STEP * 2, pitch));
        }
        let idx = RollIndex::build(1, &notes);
        // Marquee spans bars 0..64, pitches 40..70 (far wider than a typical 4-bar viewport).
        let (t0, t1) = (0, 64 * BAR);
        let (p0, p1) = (40.0, 70.0);
        let mut got = idx.marquee_hits(&notes, 4, t0, t1, p0, p1);
        got.sort_unstable();
        let mut expected: Vec<u64> = notes
            .iter()
            .filter(|n| {
                n.instrument == 4
                    && n.end() > t0
                    && n.start <= t1
                    && (n.pitch as f32 + 1.0) > p0
                    && (n.pitch as f32) < p1
            })
            .map(|n| n.id)
            .collect();
        expected.sort_unstable();
        assert_eq!(got, expected);
        assert!(
            got.len() > 500,
            "marquee should capture many off-screen notes"
        );
    }

    #[test]
    fn lane_note_near_matches_linear_scan_with_and_without_selection() {
        let mut notes = Vec::new();
        for i in 0..200i64 {
            notes.push(make_note((i + 1) as u64, 4, i * STEP, STEP, 60));
        }
        let idx = RollIndex::build(1, &notes);
        let g = make_geom(4 * BAR, 24.0, 600.0, 72, 48);

        // Probe near note at 5*BAR (index 80).
        let target_x = g.x(5 * BAR) + 3.0 + 2.0;
        let sel_empty = SelLookup::new(&[]);
        let hit = idx.lane_note_near(&notes, 4, &sel_empty, &g, target_x);
        assert_eq!(hit.map(|n| n.start), Some(5 * BAR));

        // Multi-selection prefers a selected note within 10 px even if slightly further.
        let sel_ids = vec![80u64, 82u64]; // note 82 is at 5*BAR + STEP
        let sel_multi = SelLookup::new(&sel_ids);
        let near_81_x = g.x(5 * BAR + STEP) + 3.0 - 6.0;
        let hit_sel = idx.lane_note_near(&notes, 4, &sel_multi, &g, near_81_x);
        assert_eq!(hit_sel.map(|n| n.id), Some(82));
    }

    #[test]
    fn sync_rebuilds_on_inplace_mutation() {
        let mut notes = vec![make_note(1, 4, 0, STEP, 60)];
        let mut idx = RollIndex::default();
        idx.sync(1, &notes);
        assert_eq!(idx.note_at(&notes, 4, 0, 60).map(|n| n.id), Some(1));

        // Mutate start in place without changing Vec length.
        notes[0].start = 10 * BAR;
        idx.sync(1, &notes);
        assert_eq!(idx.note_at(&notes, 4, 0, 60), None);
        assert_eq!(idx.note_at(&notes, 4, 10 * BAR, 60).map(|n| n.id), Some(1));
    }

    /// Performance benchmark on a 2000-note pattern (zoomed-in + full zoom-out).
    /// Run with: `KFXL_NO_UPDATE_CHECK=1 cargo test --locked --release roll_cull_bench_2000_notes -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn roll_cull_bench_2000_notes() {
        use crate::ui::{App, BASS};
        use eframe::egui;
        use keyframexl::sequence::Song;

        let mut app = App::with_song(Song::default(), None);
        let pid = app.selected;
        {
            let pat = app.song.pattern_mut(pid).unwrap();
            pat.notes.clear();
            pat.length = 64 * BAR;
            for i in 0..2000i64 {
                let start = (i * (STEP / 2)) % (64 * BAR);
                let pitch = 36 + ((i * 7) % 48) as u8;
                let len = if i % 100 == 0 { 4 * BAR } else { STEP };
                pat.notes.push(Note {
                    id: (i + 1) as u64,
                    ..Note::new(BASS, start, len, pitch, 100)
                });
            }
        }
        // Select half the notes to also stress selection lookup.
        app.roll_sel = (1..=1000).collect();

        let ctx = egui::Context::default();
        let mut inst = BASS;
        let mut run_frames = |app: &mut App, cell: f32, frames: usize| -> f64 {
            app.roll_view.insert(
                BASS,
                crate::ui::roll::RollView {
                    top: 76.0,
                    row: 12.0,
                    x0: 8.0 * BAR as f32,
                    cell,
                },
            );
            // Warmup frame.
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.roll(ui, &mut inst));
            });
            let t0 = Instant::now();
            for _ in 0..frames {
                let out = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.roll(ui, &mut inst));
                });
                let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
            }
            t0.elapsed().as_secs_f64() * 1000.0 / frames as f64
        };

        let zoomed_in_ms = run_frames(&mut app, 24.0, 60);
        let zoomed_out_ms = run_frames(&mut app, 4.0, 60);
        println!(
            "roll_cull_bench_2000_notes: zoomed_in={zoomed_in_ms:.3}ms/frame, zoomed_out={zoomed_out_ms:.3}ms/frame"
        );
        if cfg!(not(debug_assertions)) {
            assert!(
                zoomed_in_ms < 16.6,
                "zoomed-in 2000-note roll exceeded 60fps budget: {zoomed_in_ms:.3}ms"
            );
        }
    }
}
