//! The song's vocal notes grouped into phrases.

use super::{Params, RNote};
use crate::sequence::{InstrumentKind, Pattern, Song, Tick, seconds_per_step, steps, swing_offset};
use crate::tempo::TempoTable;
use std::hash::Hasher;

/// One phrase to render, with what it depends on hashed into `key`.
#[derive(Clone, Debug)]
pub struct Spec {
    pub inst: u64,
    pub bank: String,
    pub params: Params,
    pub notes: Vec<RNote>,
    pub key: u64,
    /// Song mode (placed by a playlist clip) or pattern mode.
    pub song: bool,
    /// Each note's tick as the engine's hit sees it: the playlist tick in
    /// song mode, the pattern tick in pattern mode.
    pub ticks: Vec<Tick>,
}

struct Fnv(u64);
impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
}

fn hash_f(h: &mut Fnv, v: f64) {
    h.write(&v.to_bits().to_le_bytes());
}

/// Phrases are cut where a rest is longer than this (seconds).
pub const GAP: f64 = 0.03;

/// Where a pattern plays: the pattern ticks `window` heard from playlist tick
/// `start`, timed like the engine times the notes there.
struct Place<'a> {
    window: (Tick, Tick),
    start: Tick,
    /// Song mode: notes are cut at the clip's end.
    song: bool,
    table: Option<&'a TempoTable>,
}

impl Place<'_> {
    /// Seconds from the place's first audible tick to pattern tick `t`, the
    /// engine's way (`Engine::collect_hits`): through the tempo map when there
    /// is one, swing included.
    fn secs(&self, song: &Song, rate: u32, t: Tick) -> f64 {
        let swing = song.swing.clamp(0.0, 0.6);
        let local = t - self.window.0;
        match self.table {
            Some(table) => {
                let sps = seconds_per_step(table.bpm_at(self.start + local));
                (table.frames_at(self.start + local) - table.frames_at(self.start)) / rate as f64
                    + swing_offset(local, sps, swing)
            }
            None => {
                let sps = seconds_per_step(song.tempo);
                steps(local) * sps + swing_offset(local, sps, swing)
            }
        }
    }
}

pub fn plan(song: &Song, rate: u32) -> Vec<Spec> {
    let table =
        crate::tempo::effective_tempo_map(song).map(|m| TempoTable::build(&m, song.tempo, rate));
    let mut out = Vec::new();
    for inst in &song.instruments {
        let InstrumentKind::Vocal { voice } = &inst.kind else {
            continue;
        };
        if inst.muted {
            continue;
        }
        let sings = |p: &Pattern| {
            p.automation.is_none() && p.notes.iter().any(|n| n.instrument == inst.id && !n.muted)
        };
        // Pattern mode: every pattern as a loop, no tempo map (as the engine).
        for pat in song.patterns.iter().filter(|p| sings(p)) {
            let place = Place {
                window: (0, pat.length),
                start: 0,
                song: false,
                table: None,
            };
            phrases(song, rate, inst.id, voice, pat, &place, &mut out);
        }
        // Song mode: each clip, timed where it sits.
        for c in song
            .clips
            .iter()
            .filter(|c| !c.muted && !song.track_muted(c.track))
        {
            let Some(pat) = song.pattern(c.pattern).filter(|p| sings(p)) else {
                continue;
            };
            let place = Place {
                window: (c.offset, c.offset + song.clip_len(c)),
                start: c.start,
                song: true,
                table: table.as_ref(),
            };
            phrases(song, rate, inst.id, voice, pat, &place, &mut out);
        }
    }
    out
}

/// A note's drawn pitch points in the phrase's seconds (through the tempo
/// map, as its notes), with its ends.
fn curve_secs(
    song: &Song,
    rate: u32,
    place: &Place,
    pitch: Option<&crate::sequence::NotePitch>,
    t: Tick,
    length: Tick,
) -> Vec<(f64, f64, crate::sequence::CurveShape)> {
    use crate::sequence::CurveShape;
    let Some(c) = pitch.filter(|c| !c.points.is_empty()) else {
        return Vec::new();
    };
    let mut pts: Vec<(Tick, f64, CurveShape)> = Vec::with_capacity(c.points.len() + 2);
    if c.points[0].at > 0 {
        pts.push((0, 0.0, CurveShape::Smooth));
    }
    pts.extend(c.points.iter().map(|p| (p.at, p.cents as f64, p.shape)));
    if c.points.last().is_some_and(|p| p.at < length) {
        pts.push((length, 0.0, CurveShape::Smooth));
    }
    pts.into_iter()
        .map(|(at, cents, shape)| (place.secs(song, rate, t + at), cents, shape))
        .collect()
}

fn phrases(
    song: &Song,
    rate: u32,
    inst: u64,
    voice: &super::Voice,
    pat: &Pattern,
    place: &Place,
    out: &mut Vec<Spec>,
) {
    let (w0, w1) = place.window;
    let plen = pat.length.max(1);
    // Every note start the place plays (a long clip repeats its pattern), the
    // engine's rule: a note sounds when its start falls inside the window.
    let mut hits: Vec<(Tick, &crate::sequence::Note)> = Vec::new();
    for n in pat
        .notes
        .iter()
        .filter(|n| n.instrument == inst && !n.muted)
    {
        for k in w0.div_euclid(plen)..=(w1 - 1).div_euclid(plen) {
            let t = n.start + k * plen;
            if (w0..w1).contains(&t) {
                hits.push((t, n));
            }
        }
    }
    hits.sort_by_key(|(t, n)| (*t, n.id));
    // One voice: a note ends where the next begins.
    let mut r: Vec<RNote> = Vec::new();
    let mut ticks: Vec<Tick> = Vec::new();
    for (i, &(t, n)) in hits.iter().enumerate() {
        if i > 0 && hits[i - 1].0 == t {
            continue;
        }
        let mut end = hits
            .iter()
            .skip(i + 1)
            .find(|(m, _)| *m > t)
            .map(|(m, _)| (*m).min(t + n.length))
            .unwrap_or(t + n.length);
        if place.song {
            end = end.min(w1);
        }
        r.push(RNote {
            id: n.id,
            start: place.secs(song, rate, t),
            end: place.secs(song, rate, end),
            pitch: n.pitch as f64 + n.fine as f64 / 100.0,
            velocity: n.velocity,
            lyric: pat
                .lyrics
                .get(&n.id)
                .cloned()
                .unwrap_or_else(|| voice.default_lyric.clone()),
            vibrato: (n.mod_x as f64 * 2.0).clamp(0.0, 2.0),
            gender: ((n.mod_y as f64 - 0.5) * 2.0).clamp(-1.0, 1.0),
            curve: curve_secs(song, rate, place, pat.pitch.get(&n.id), t, n.length),
            own_vibrato: pat.pitch.get(&n.id).and_then(|c| c.vibrato),
        });
        ticks.push(place.start + (t - w0));
    }
    let mut start = 0;
    for i in 1..=r.len() {
        if i == r.len() || r[i].start - r[i - 1].end > GAP {
            let notes = r[start..i].to_vec();
            let note_ticks = ticks[start..i].to_vec();
            start = i;
            if notes.is_empty() {
                continue;
            }
            let mut h = Fnv(0xcbf2_9ce4_8422_2325);
            h.write(voice.bank.as_bytes());
            h.write(&rate.to_le_bytes());
            let p = &voice.params;
            for v in [
                p.vibrato_depth,
                p.vibrato_rate,
                p.vibrato_delay,
                p.vibrato_fade,
                p.transition,
                p.gender,
                p.breath,
            ] {
                hash_f(&mut h, v);
            }
            // Times relative to the first note: moving a phrase as a whole
            // renders nothing again; a tempo change inside it does.
            let base = notes[0].start;
            for n in &notes {
                hash_f(&mut h, n.start - base);
                hash_f(&mut h, n.end - base);
                hash_f(&mut h, n.pitch);
                h.write(&[n.velocity]);
                h.write(n.lyric.as_bytes());
                hash_f(&mut h, n.vibrato);
                hash_f(&mut h, n.gender);
                for &(at, cents, shape) in &n.curve {
                    hash_f(&mut h, at - base);
                    hash_f(&mut h, cents);
                    h.write(&[shape as u8]);
                }
                if let Some(v) = n.own_vibrato {
                    for x in [v.len, v.depth, v.rate, v.fade_in, v.fade_out] {
                        hash_f(&mut h, x as f64);
                    }
                }
            }
            out.push(Spec {
                inst,
                bank: voice.bank.clone(),
                params: p.clone(),
                notes,
                key: h.finish(),
                song: place.song,
                ticks: note_ticks,
            });
        }
    }
}

/// One preview per key for every singing channel: the default lyric held on that pitch.
pub fn previews(song: &Song, rate: u32) -> Vec<(u64, u8, u64, Spec)> {
    let mut out = Vec::new();
    for inst in &song.instruments {
        let InstrumentKind::Vocal { voice } = &inst.kind else {
            continue;
        };
        if voice.bank.is_empty() {
            continue;
        }
        for pitch in super::PREVIEW_PITCHES {
            let note = RNote {
                id: 0,
                start: 0.0,
                end: super::PREVIEW_LEN,
                pitch: pitch as f64,
                velocity: 100,
                lyric: voice.default_lyric.clone(),
                vibrato: 1.0,
                gender: 0.0,
                curve: Vec::new(),
                own_vibrato: None,
            };
            let mut h = Fnv(0x9e37_79b9_7f4a_7c15);
            h.write(voice.bank.as_bytes());
            h.write(&rate.to_le_bytes());
            h.write(voice.default_lyric.as_bytes());
            h.write(&[pitch]);
            let p = &voice.params;
            for v in [
                p.vibrato_depth,
                p.vibrato_rate,
                p.vibrato_delay,
                p.vibrato_fade,
                p.transition,
                p.gender,
                p.breath,
            ] {
                hash_f(&mut h, v);
            }
            out.push((
                inst.id,
                pitch,
                0,
                Spec {
                    inst: inst.id,
                    bank: voice.bank.clone(),
                    params: p.clone(),
                    notes: vec![note],
                    key: h.finish(),
                    song: false,
                    ticks: vec![0],
                },
            ));
        }
        // Each written note also previews its own syllable at its own pitch (a click on it sings that).
        let mut seen = std::collections::HashSet::new();
        for pat in &song.patterns {
            for n in pat.notes.iter().filter(|n| n.instrument == inst.id) {
                let Some(l) = pat.lyrics.get(&n.id) else {
                    continue;
                };
                let lh = super::lyric_hash(l);
                if out.len() > 600
                    || !seen.insert((n.pitch, lh))
                    || (super::PREVIEW_PITCHES.contains(&n.pitch) && *l == voice.default_lyric)
                {
                    continue;
                }
                let note = RNote {
                    id: 0,
                    start: 0.0,
                    end: super::PREVIEW_LEN,
                    pitch: n.pitch as f64,
                    velocity: 100,
                    lyric: l.clone(),
                    vibrato: 1.0,
                    gender: 0.0,
                    curve: Vec::new(),
                    own_vibrato: None,
                };
                let mut h = Fnv(0x9e37_79b9_7f4a_7c15);
                h.write(voice.bank.as_bytes());
                h.write(&rate.to_le_bytes());
                h.write(l.as_bytes());
                h.write(&[n.pitch]);
                let p = &voice.params;
                for v in [
                    p.vibrato_depth,
                    p.vibrato_rate,
                    p.vibrato_delay,
                    p.vibrato_fade,
                    p.transition,
                    p.gender,
                    p.breath,
                ] {
                    hash_f(&mut h, v);
                }
                out.push((
                    inst.id,
                    n.pitch,
                    lh,
                    Spec {
                        inst: inst.id,
                        bank: voice.bank.clone(),
                        params: p.clone(),
                        notes: vec![note],
                        key: h.finish(),
                        song: false,
                        ticks: vec![0],
                    },
                ));
            }
        }
    }
    out
}
