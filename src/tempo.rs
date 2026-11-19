//! Song tempo map and time-signature changes. Pure data + math only;
//! the engine, audio, and UI still use constant tempo (tempo plan steps 1-2).
//!
//! Time is integer ticks (`Tick`, `PPQ` per quarter). `TempoMap` is the song
//! tempo track: points sorted by tick, empty = flat `Song.tempo`.
//! `TempoTable` is the precomputed segment table built from a map.
//! Lookups use `partition_point` and never allocate.

use serde::{Deserialize, Serialize};

use crate::sequence::{PPQ, STEP, Tick, seconds_per_step};

/// Slowest allowed tempo, same range as `Song::validate`.
pub const TEMPO_MIN: f64 = 40.0;
/// Fastest allowed tempo, same range as `Song::validate`.
pub const TEMPO_MAX: f64 = 300.0;

/// How the tempo moves from this point to the next one: jump or slide (FL).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum TempoShape {
    #[default]
    Step,
    Linear,
}

/// One tempo event on the song tempo track.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoPoint {
    pub tick: Tick,
    pub bpm: f64,
    /// Segment shape from this point to the next; ignored on the last point.
    #[serde(default)]
    pub shape: TempoShape,
}

/// The song tempo track. Empty = flat `Song.tempo` (old songs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TempoMap {
    pub points: Vec<TempoPoint>,
}

impl TempoMap {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// A time-signature change starting at bar `bar` (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    pub bar: u32,
    pub num: u8,
    pub den: u8,
}

/// Clamp to the tempo range; non-finite falls back to `fallback`.
fn clean_bpm(bpm: f64, fallback: f64) -> f64 {
    let fb = if fallback.is_finite() {
        fallback.clamp(TEMPO_MIN, TEMPO_MAX)
    } else {
        crate::sequence::default_tempo()
    };
    if bpm.is_finite() {
        bpm.clamp(TEMPO_MIN, TEMPO_MAX)
    } else {
        fb
    }
}

/// Check a tempo map is in normalized form: ticks strictly increasing and
/// non-negative, every bpm finite and in range. Empty is valid (flat tempo).
/// A missing tick-0 point is valid: the fallback tempo plays until the first
/// point (see `TempoTable::build`).
pub fn validate_tempo_map(map: &TempoMap) -> Result<(), &'static str> {
    let mut prev: Option<Tick> = None;
    for p in &map.points {
        if p.tick < 0 {
            return Err("Bad tempo map");
        }
        if prev.is_some_and(|q| p.tick <= q) {
            return Err("Bad tempo map");
        }
        prev = Some(p.tick);
        if !p.bpm.is_finite() || !(TEMPO_MIN..=TEMPO_MAX).contains(&p.bpm) {
            return Err("Bad tempo map");
        }
    }
    Ok(())
}

/// Check meters are sorted with unique bars and legal signatures.
pub fn validate_meters(meters: &[Meter]) -> Result<(), &'static str> {
    let mut prev: Option<u32> = None;
    for m in meters {
        if prev.is_some_and(|q| m.bar <= q) {
            return Err("Bad time signature");
        }
        prev = Some(m.bar);
        if !(1..=16).contains(&m.num) {
            return Err("Bad time signature");
        }
        if !matches!(m.den, 1 | 2 | 4 | 8 | 16) {
            return Err("Bad time signature");
        }
        if bar_len_ticks(m.num, m.den) < crate::sequence::STEP {
            return Err("Bad time signature");
        }
    }
    Ok(())
}

/// Sort by tick, drop negative ticks, dedupe same ticks (last wins, FL),
/// prepend a tick-0 point at `fallback_bpm` when missing.
pub fn normalize_tempo_map(map: &mut TempoMap, fallback_bpm: f64) {
    map.points.sort_by_key(|p| p.tick);
    let mut out = Vec::with_capacity(map.points.len());
    for p in map.points.drain(..) {
        if p.tick < 0 {
            continue;
        }
        if out.last().is_some_and(|l: &TempoPoint| l.tick == p.tick) {
            *out.last_mut().expect("checked") = p;
        } else {
            out.push(p);
        }
    }
    map.points = out;
    if map.points.first().is_none_or(|p| p.tick != 0) {
        map.points.insert(
            0,
            TempoPoint {
                tick: 0,
                bpm: clean_bpm(fallback_bpm, fallback_bpm),
                shape: TempoShape::Step,
            },
        );
    }
}

/// Sort by bar, dedupe same bars (last wins), insert 4/4 at bar 0 when missing.
pub fn normalize_meters(meters: &mut Vec<Meter>) {
    meters.sort_by_key(|m| m.bar);
    let mut out = Vec::with_capacity(meters.len());
    for m in meters.drain(..) {
        if out.last().is_some_and(|l: &Meter| l.bar == m.bar) {
            *out.last_mut().expect("checked") = m;
        } else {
            out.push(m);
        }
    }
    *meters = out;
    if meters.first().is_none_or(|m| m.bar != 0) {
        meters.insert(
            0,
            Meter {
                bar: 0,
                num: 4,
                den: 4,
            },
        );
    }
}

/// Ticks in one bar of `num/den`. Inputs outside `validate_meters` still
/// return something sane (never panics); validation rejects them.
pub fn bar_len_ticks(num: u8, den: u8) -> Tick {
    num as Tick * PPQ * 4 / den.max(1) as Tick
}

/// Signature in force at `bar`: the last change at or before it, else 4/4.
/// `meters` must be sorted by bar (see `validate_meters`).
fn meter_at(meters: &[Meter], bar: u32) -> (u8, u8) {
    let i = meters.partition_point(|m| m.bar <= bar);
    if i == 0 {
        (4, 4)
    } else {
        (meters[i - 1].num, meters[i - 1].den)
    }
}

/// Ticks in bar `bar` under `meters` (sorted by bar).
pub fn bar_len(meters: &[Meter], bar: u32) -> Tick {
    let (num, den) = meter_at(meters, bar);
    bar_len_ticks(num, den)
}

/// First tick of bar `bar` under `meters` (sorted by bar).
pub fn bar_start(meters: &[Meter], bar: u32) -> Tick {
    let mut tick: Tick = 0;
    let mut b = 0u32;
    let mut mi = 0usize;
    let mut len = bar_len_ticks(4, 4);
    while b < bar {
        while mi < meters.len() && meters[mi].bar <= b {
            len = bar_len_ticks(meters[mi].num, meters[mi].den).max(1);
            mi += 1;
        }
        let next = meters.get(mi).map(|m| m.bar).unwrap_or(u32::MAX);
        let run = next.saturating_sub(b).min(bar - b).max(1);
        tick += run as Tick * len;
        b += run;
    }
    tick
}

/// The bar holding tick `t` under `meters` (sorted by bar):
/// (bar index, its first tick, its length, numerator, denominator).
pub fn bar_at(meters: &[Meter], t: Tick) -> (u32, Tick, Tick, u8, u8) {
    let t = t.max(0);
    let mut tick: Tick = 0;
    let mut b = 0u32;
    let mut mi = 0usize;
    let (mut num, mut den) = (4u8, 4u8);
    loop {
        while mi < meters.len() && meters[mi].bar <= b {
            (num, den) = (meters[mi].num, meters[mi].den);
            mi += 1;
        }
        let len = bar_len_ticks(num, den).max(1);
        if let Some(next) = meters.get(mi) {
            let run = (next.bar - b) as Tick;
            if t >= tick + run * len {
                tick += run * len;
                b = next.bar;
                continue;
            }
        }
        let k = (t - tick) / len;
        return (b + k as u32, tick + k * len, len, num, den);
    }
}

/// How many bars cover `end` ticks under `meters` (at least one).
pub fn bars(meters: &[Meter], end: Tick) -> u32 {
    if end <= 0 {
        return 1;
    }
    let mut tick: Tick = 0;
    let mut b = 0u32;
    let mut mi = 0usize;
    let mut len = bar_len_ticks(4, 4).max(1);
    loop {
        while mi < meters.len() && meters[mi].bar <= b {
            len = bar_len_ticks(meters[mi].num, meters[mi].den).max(1);
            mi += 1;
        }
        let next = meters.get(mi).map(|m| m.bar).unwrap_or(u32::MAX);
        let run = next.saturating_sub(b).max(1) as Tick;
        if tick.saturating_add(run * len) >= end {
            let rest = end - tick;
            return b.saturating_add(((rest - 1) / len) as u32 + 1);
        }
        tick += run * len;
        b = b.saturating_add(run as u32);
    }
}

/// One tempo segment: `[t0, next t0)`. `m` is bpm per tick; 0 = constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CachedSeg {
    pub t0: Tick,
    pub b0: f64,
    pub m: f64,
    pub f0: f64,
}

/// Precomputed tempo segments. Build once per edit (UI/load thread);
/// lookups are O(log n) and never allocate.
#[derive(Clone, Debug)]
pub struct TempoTable {
    pub segs: Vec<CachedSeg>,
    pub rate: u32,
}

impl TempoTable {
    /// Build from a map; `fallback_bpm` (`Song.tempo`) is the tick-0 tempo
    /// and fills any gap before the first point. Bad values are clamped, so
    /// the table is always usable even without prior validation.
    /// Always holds at least one segment starting at tick 0.
    pub fn build(map: &TempoMap, fallback_bpm: f64, rate: u32) -> Self {
        let mut norm = TempoMap {
            points: map.points.clone(),
        };
        normalize_tempo_map(&mut norm, fallback_bpm);
        let rate_f = rate.max(1) as f64;
        let mut segs = Vec::with_capacity(norm.points.len());
        let mut f0 = 0.0;
        for (i, p) in norm.points.iter().enumerate() {
            let bpm = clean_bpm(p.bpm, fallback_bpm);
            let mut m = 0.0;
            let mut span = 0.0;
            if let Some(q) = norm.points.get(i + 1) {
                let d = (q.tick - p.tick).max(1) as f64;
                let b1 = clean_bpm(q.bpm, fallback_bpm);
                if p.shape == TempoShape::Linear && (b1 - bpm).abs() > 1e-9 * bpm {
                    m = (b1 - bpm) / d;
                    span = rate_f * 60.0 / (PPQ as f64 * m) * (b1 / bpm).ln();
                } else {
                    // Same op order as `frames_at_float` so segments join up.
                    span = d / STEP as f64 * seconds_per_step(bpm) * rate_f;
                }
            }
            segs.push(CachedSeg {
                t0: p.tick,
                b0: bpm,
                m,
                f0,
            });
            f0 += span;
        }
        Self {
            segs,
            rate: rate.max(1),
        }
    }

    /// One segment: covers the whole song, so no tempo events.
    pub fn is_flat(&self) -> bool {
        self.segs.len() <= 1
    }

    fn seg_for_tick(&self, tick: f64) -> &CachedSeg {
        let i = self.segs.partition_point(|s| s.t0 as f64 <= tick);
        &self.segs[i.saturating_sub(1)]
    }

    /// Exact frames from song start to fractional `tick`.
    pub fn frames_at_float(&self, tick: f64) -> f64 {
        let t = tick.max(0.0);
        let s = self.seg_for_tick(t);
        let dt = t - s.t0 as f64;
        if s.m == 0.0 {
            // Same op order as the engine's constant-tempo `note_frame`
            // (steps * sps * rate), so a single segment matches it bit-for-bit.
            s.f0 + dt / STEP as f64 * seconds_per_step(s.b0) * self.rate as f64
        } else {
            let per = self.rate as f64 * 60.0 / (PPQ as f64 * s.m);
            s.f0 + per * ((s.b0 + s.m * dt) / s.b0).ln()
        }
    }

    /// Exact frames from song start to `tick`.
    pub fn frames_at(&self, tick: Tick) -> f64 {
        self.frames_at_float(tick as f64)
    }

    /// Whole frames to `tick`, rounded up. This `.ceil()` matches the engine's
    /// `note_frame`/`clip_frame` policy for constant tempo (see tests).
    pub fn frames_at_ceil(&self, tick: Tick) -> u64 {
        self.frames_at(tick).ceil() as u64
    }

    /// Fractional song tick at `frame` (the inverse of `frames_at_float`).
    pub fn tick_at(&self, frame: f64) -> f64 {
        if frame <= 0.0 {
            return 0.0;
        }
        let i = self
            .segs
            .partition_point(|s| s.f0 <= frame)
            .saturating_sub(1);
        let s = &self.segs[i];
        let df = frame - s.f0;
        if s.m == 0.0 {
            // Same op order as the engine's `song_tick` rest term.
            s.t0 as f64 + df / (seconds_per_step(s.b0) * self.rate as f64) * STEP as f64
        } else {
            let k = s.m * PPQ as f64 * df / (self.rate as f64 * 60.0);
            s.t0 as f64 + s.b0 / s.m * (k.exp() - 1.0)
        }
    }

    /// Whole song tick at `frame`, truncated like the engine's `song_tick`.
    pub fn tick_at_frame(&self, frame: u64) -> Tick {
        self.tick_at(frame as f64).floor().max(0.0) as Tick
    }

    /// Tempo in force at fractional `tick` (ramps interpolate linearly).
    pub fn bpm_at_float(&self, tick: f64) -> f64 {
        let t = tick.max(0.0);
        let s = self.seg_for_tick(t);
        if s.m == 0.0 {
            s.b0
        } else {
            s.b0 + s.m * (t - s.t0 as f64)
        }
    }

    /// Tempo in force at `tick` (ramps interpolate linearly).
    pub fn bpm_at(&self, tick: Tick) -> f64 {
        self.bpm_at_float(tick as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::{BAR, STEP, Song};

    fn map(points: &[(Tick, f64, TempoShape)]) -> TempoMap {
        TempoMap {
            points: points
                .iter()
                .map(|&(tick, bpm, shape)| TempoPoint { tick, bpm, shape })
                .collect(),
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 + 1e-9 * b.abs()
    }

    fn lcg(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *state >> 11
    }

    #[test]
    fn empty_map_is_flat_fallback_tempo() {
        let t = TempoTable::build(&TempoMap::default(), 120.0, 48_000);
        assert!(t.is_flat());
        assert_eq!(t.bpm_at(0), 120.0);
        assert_eq!(t.bpm_at(10 * BAR), 120.0);
        // 120 BPM: a step is exactly 6000 frames at 48 kHz.
        assert_eq!(t.frames_at_ceil(STEP), 6000);
        assert_eq!(t.frames_at_ceil(BAR), 96_000);
        assert_eq!(t.tick_at_frame(6000), STEP);
    }

    #[test]
    fn round_trip_mixed_step_and_ramp() {
        let maps = [
            map(&[(0, 120.0, TempoShape::Step)]),
            map(&[(0, 120.0, TempoShape::Step), (BAR, 140.0, TempoShape::Step)]),
            map(&[
                (0, 100.0, TempoShape::Linear),
                (4 * BAR, 160.0, TempoShape::Step),
            ]),
            map(&[
                (0, 120.0, TempoShape::Step),
                (BAR, 120.0, TempoShape::Linear),
                (3 * BAR, 90.0, TempoShape::Linear),
                (4 * BAR, 150.0, TempoShape::Step),
            ]),
        ];
        let end = 8 * BAR;
        for m in &maps {
            let t = TempoTable::build(m, 120.0, 48_000);
            let f_end = t.frames_at(end);
            let mut rng = 0x1234_5678_9abc_def1u64;
            for _ in 0..200 {
                let tick = (lcg(&mut rng) % end as u64) as Tick;
                assert!(
                    close(t.tick_at(t.frames_at(tick)), tick as f64),
                    "tick {tick}"
                );
                let frame = (lcg(&mut rng) % f_end as u64) as f64;
                assert!(
                    close(t.frames_at_float(t.tick_at(frame)), frame),
                    "frame {frame}"
                );
            }
            // Exact hits on boundaries too.
            for tick in [0, 1, STEP - 1, STEP, BAR, 3 * BAR + 7, end] {
                assert!(close(t.tick_at(t.frames_at(tick)), tick as f64));
            }
        }
    }

    #[test]
    fn ramp_midpoint_and_bpm_track() {
        let t = TempoTable::build(
            &map(&[
                (0, 100.0, TempoShape::Linear),
                (4 * BAR, 200.0, TempoShape::Step),
            ]),
            120.0,
            48_000,
        );
        assert_eq!(t.bpm_at(0), 100.0);
        assert!(close(t.bpm_at(2 * BAR), 150.0));
        assert_eq!(t.bpm_at(4 * BAR), 200.0);
        assert_eq!(t.bpm_at(9 * BAR), 200.0);
        // Ramp-down slope is negative but frames still advance.
        let d = TempoTable::build(
            &map(&[
                (0, 160.0, TempoShape::Linear),
                (2 * BAR, 80.0, TempoShape::Step),
            ]),
            120.0,
            48_000,
        );
        assert!(close(d.bpm_at(BAR), 120.0));
        assert!(d.frames_at(2 * BAR) > d.frames_at(BAR));
        assert!(d.frames_at(BAR) > d.frames_at(0));
    }

    // Read-only copies of the engine's constant-tempo formulas
    // (src/engine.rs `sps`, `note_frame`, `clip_frame`, `song_tick`).
    // The engine is untouched; these pin the equivalence below.
    fn old_sps(bpm: f64) -> f64 {
        crate::sequence::seconds_per_step(bpm)
    }
    fn old_note_frame(bpm: f64, rate: u32, t: Tick) -> u64 {
        // Swing is 0 here; the map never includes swing either.
        ((t as f64 / STEP as f64 * old_sps(bpm)) * rate as f64).ceil() as u64
    }
    fn old_clip_frame(bpm: f64, rate: u32, t: Tick) -> u64 {
        let bar = (16.0 * old_sps(bpm) * rate as f64) as u64;
        let rest = t.rem_euclid(BAR);
        (t / BAR) as u64 * bar
            + if rest == 0 {
                0
            } else {
                (rest as f64 / STEP as f64 * old_sps(bpm) * rate as f64).ceil() as u64
            }
    }
    fn old_song_tick(bpm: f64, rate: u32, f: u64) -> Tick {
        let bar = ((16.0 * old_sps(bpm) * rate as f64) as u64).max(1);
        let rest = (f % bar) as f64 / (old_sps(bpm) * rate as f64) * STEP as f64;
        (f / bar) as Tick * BAR + rest as Tick
    }

    #[test]
    fn single_segment_matches_old_constant_tempo_math() {
        // Ceil policy: the map keeps exact f64 frames and rounds up only at
        // the end, exactly like `note_frame`'s `.ceil()`. `clip_frame` instead
        // truncates whole-bar frames (`as u64`) and ceils the rest, so the two
        // agree only when a bar is a whole number of frames (as here).
        let rate = 48_000;
        for bpm in [96.0, 100.0, 120.0, 125.0, 150.0] {
            let t = TempoTable::build(&TempoMap::default(), bpm, rate);
            assert!(t.is_flat());
            for tick in [
                0,
                1,
                STEP - 1,
                STEP,
                BAR - 1,
                BAR,
                BAR + 1,
                7 * BAR + 13,
                100 * BAR,
            ] {
                assert_eq!(
                    t.frames_at_ceil(tick),
                    old_note_frame(bpm, rate, tick),
                    "note {bpm} t={tick}"
                );
                assert_eq!(
                    t.frames_at_ceil(tick),
                    old_clip_frame(bpm, rate, tick),
                    "clip {bpm} t={tick}"
                );
            }
            for frame in [0, 1, 5999, 6000, 95_999, 96_000, 96_001, 5_000_000] {
                assert_eq!(
                    t.tick_at_frame(frame),
                    old_song_tick(bpm, rate, frame),
                    "tick {bpm} f={frame}"
                );
            }
        }
    }

    #[test]
    fn clip_frame_bar_truncation_quirk_is_documented() {
        // At 140 BPM a bar is 82285.714... frames. The old `clip_frame`
        // truncates that to 82285 per bar, while the map keeps the exact
        // cumulative value and ceils once. The map policy wins for step 3:
        // it is exact, and matches `note_frame` everywhere.
        let rate = 48_000;
        let t = TempoTable::build(&TempoMap::default(), 140.0, rate);
        assert_eq!(t.frames_at_ceil(BAR), old_note_frame(140.0, rate, BAR));
        assert_ne!(t.frames_at_ceil(BAR), old_clip_frame(140.0, rate, BAR));
        assert_eq!(old_clip_frame(140.0, rate, BAR), 82_285);
        assert_eq!(t.frames_at_ceil(BAR), 82_286);
    }

    #[test]
    fn validate_rejects_bad_maps_and_meters() {
        // bpm range (mirrors the Song.tempo range 40..=300).
        for bpm in [39.9, 300.1, f64::NAN, f64::INFINITY] {
            assert!(
                validate_tempo_map(&map(&[(0, bpm, TempoShape::Step)])).is_err(),
                "{bpm}"
            );
        }
        for bpm in [40.0, 120.0, 300.0] {
            assert!(
                validate_tempo_map(&map(&[(0, bpm, TempoShape::Step)])).is_ok(),
                "{bpm}"
            );
        }
        // Unsorted and duplicate ticks (normalize first; see below).
        assert!(
            validate_tempo_map(&map(&[
                (BAR, 120.0, TempoShape::Step),
                (0, 120.0, TempoShape::Step)
            ]))
            .is_err()
        );
        assert!(
            validate_tempo_map(&map(&[
                (0, 120.0, TempoShape::Step),
                (0, 140.0, TempoShape::Step)
            ]))
            .is_err()
        );
        assert!(validate_tempo_map(&map(&[(-1, 120.0, TempoShape::Step)])).is_err());
        // Empty and tick-0-less maps are valid (flat / fallback semantics).
        assert!(validate_tempo_map(&TempoMap::default()).is_ok());
        assert!(validate_tempo_map(&map(&[(BAR, 140.0, TempoShape::Step)])).is_ok());
        // Meters: dup bar, bad den, bad num.
        let m = |bar, num, den| vec![Meter { bar, num, den }];
        assert!(validate_meters(&[]).is_ok());
        assert!(validate_meters(&m(0, 4, 4)).is_ok());
        assert!(
            validate_meters(&[
                Meter {
                    bar: 0,
                    num: 4,
                    den: 4
                },
                Meter {
                    bar: 0,
                    num: 3,
                    den: 4
                }
            ])
            .is_err()
        );
        assert!(validate_meters(&m(0, 4, 3)).is_err());
        assert!(validate_meters(&m(0, 0, 4)).is_err());
        assert!(validate_meters(&m(0, 17, 4)).is_err());
        assert!(validate_meters(&m(0, 4, 32)).is_err());
        assert!(
            validate_meters(&[
                Meter {
                    bar: 1,
                    num: 3,
                    den: 4
                },
                Meter {
                    bar: 0,
                    num: 4,
                    den: 4
                }
            ])
            .is_err()
        );
    }

    #[test]
    fn normalize_sorts_dedupes_and_prepends() {
        let mut m = map(&[
            (BAR, 140.0, TempoShape::Step),
            (0, 110.0, TempoShape::Step),
            (BAR, 150.0, TempoShape::Step),
            (-5, 100.0, TempoShape::Step),
        ]);
        normalize_tempo_map(&mut m, 120.0);
        assert_eq!(
            m.points,
            vec![
                TempoPoint {
                    tick: 0,
                    bpm: 110.0,
                    shape: TempoShape::Step
                },
                TempoPoint {
                    tick: BAR,
                    bpm: 150.0,
                    shape: TempoShape::Step
                },
            ]
        );
        // Empty map gains the fallback point.
        let mut e = TempoMap::default();
        normalize_tempo_map(&mut e, 128.0);
        assert_eq!(e.points.len(), 1);
        assert_eq!((e.points[0].tick, e.points[0].bpm), (0, 128.0));

        let mut meters = vec![
            Meter {
                bar: 4,
                num: 3,
                den: 4,
            },
            Meter {
                bar: 1,
                num: 6,
                den: 8,
            },
            Meter {
                bar: 1,
                num: 2,
                den: 4,
            },
        ];
        normalize_meters(&mut meters);
        assert_eq!(
            meters,
            vec![
                Meter {
                    bar: 0,
                    num: 4,
                    den: 4
                },
                Meter {
                    bar: 1,
                    num: 2,
                    den: 4
                },
                Meter {
                    bar: 4,
                    num: 3,
                    den: 4
                },
            ]
        );
    }

    #[test]
    fn old_json_without_new_fields_loads_with_defaults() {
        let mut v = serde_json::to_value(Song::default()).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("tempo_map");
        o.remove("meters");
        let back: Song = serde_json::from_value(v).unwrap();
        assert!(back.tempo_map.is_empty());
        assert!(back.meters.is_empty());
        back.validate().unwrap();
        // A point without `shape` reads as a Step (FL default).
        let p: TempoPoint = serde_json::from_str(r#"{"tick":0,"bpm":120.0}"#).unwrap();
        assert_eq!(p.shape, TempoShape::Step);
    }

    #[test]
    fn meter_bar_math_with_a_3_4_bar() {
        assert_eq!(bar_len_ticks(4, 4), BAR);
        assert_eq!(bar_len_ticks(3, 4), 12 * STEP);
        let meters = vec![
            Meter {
                bar: 0,
                num: 4,
                den: 4,
            },
            Meter {
                bar: 1,
                num: 3,
                den: 4,
            },
            Meter {
                bar: 2,
                num: 4,
                den: 4,
            },
        ];
        assert_eq!(bar_len(&meters, 0), BAR);
        assert_eq!(bar_len(&meters, 1), 12 * STEP);
        assert_eq!(bar_len(&[], 7), BAR);
        assert_eq!(bar_start(&meters, 0), 0);
        assert_eq!(bar_start(&meters, 1), BAR);
        assert_eq!(bar_start(&meters, 2), BAR + 12 * STEP);
        assert_eq!(bar_start(&meters, 3), 2 * BAR + 12 * STEP);
        assert_eq!(bar_start(&[], 5), 5 * BAR);
        assert_eq!(bars(&meters, 0), 1);
        assert_eq!(bars(&meters, BAR + 12 * STEP), 2);
        assert_eq!(bars(&meters, BAR + 12 * STEP + 1), 3);
        assert_eq!(bars(&[], 2 * BAR + 1), 3);
    }

    #[test]
    fn table_uses_fallback_before_first_point() {
        let t = TempoTable::build(&map(&[(BAR, 140.0, TempoShape::Step)]), 120.0, 48_000);
        assert!(!t.is_flat());
        assert_eq!(t.bpm_at(0), 120.0);
        assert_eq!(t.bpm_at(BAR - 1), 120.0);
        assert_eq!(t.bpm_at(BAR), 140.0);
        assert_eq!(t.frames_at_ceil(STEP), 6000);
    }
}
