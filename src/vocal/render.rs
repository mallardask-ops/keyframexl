//! Singing a phrase: UTAU timing rules (offset / consonant / preutterance /
//! overlap), pitch and vibrato from the score, WORLD resynthesis.
//!
//! The whole phrase is built in the WORLD frame domain (spectral envelope,
//! aperiodicity, pitch per 5 ms) and synthesised once, so joins between
//! syllables are crossfades of envelopes, not of waveforms.

use super::{
    Bank, Params,
    analysis::{self, Analysis, FRAME_MS, FS},
    lyric::{self, Token},
    oto::Entry,
};
use crate::sequence::{CurveShape, NoteVibrato};
use std::sync::Arc;

/// One note of a phrase, in seconds from the pattern's start.
#[derive(Clone, Debug, PartialEq)]
pub struct RNote {
    pub id: u64,
    pub start: f64,
    pub end: f64,
    /// MIDI pitch with the note's fine tuning folded in.
    pub pitch: f64,
    pub velocity: u8,
    pub lyric: String,
    /// Vibrato depth multiplier (the note's Mod X: 0.5 = as set on the channel).
    pub vibrato: f64,
    /// Added to the channel's gender (the note's Mod Y: 0.5 = none), -1..1.
    pub gender: f64,
    /// Drawn pitch: (seconds as `start`, cents from `pitch`, shape to the
    /// next point), the note's ends included. Empty: the glide and nothing else.
    pub curve: Vec<(f64, f64, CurveShape)>,
    /// The note's own vibrato instead of the channel's.
    pub own_vibrato: Option<NoteVibrato>,
}

pub struct Rendered {
    /// Frames (at the output rate) that sound before the first note's start.
    pub lead: usize,
    pub data: Vec<f32>,
    /// Lyrics the bank had no syllable for.
    pub missing: Vec<String>,
}

struct Unit {
    entry: Entry,
    an: Arc<Analysis>,
    /// Note start / end in seconds.
    t0: f64,
    t1: f64,
    /// Consonant speed factor (velocity).
    cs: f64,
    a_start: f64,
    a_end: f64,
    gain: f64,
    /// Used part of the wav, ms.
    off: f64,
    ve: f64,
    gender: f64,
}

fn semis_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// A pitch segment: one note's span and pitch, for the contour.
struct Seg {
    start: f64,
    end: f64,
    pitch: f64,
    vibrato: f64,
    curve: Vec<(f64, f64, CurveShape)>,
    own_vibrato: Option<NoteVibrato>,
}

impl Seg {
    fn of(n: &RNote) -> Self {
        Self {
            start: n.start,
            end: n.end,
            pitch: n.pitch,
            vibrato: n.vibrato,
            curve: n.curve.clone(),
            own_vibrato: n.own_vibrato,
        }
    }
}

/// Cents along a drawn curve at `tau` seconds.
fn curve_at(curve: &[(f64, f64, CurveShape)], tau: f64) -> f64 {
    let Some(first) = curve.first() else {
        return 0.0;
    };
    if tau <= first.0 {
        return first.1;
    }
    for w in curve.windows(2) {
        let ((t0, c0, shape), (t1, c1, _)) = (w[0], w[1]);
        if tau <= t1 {
            let x = if t1 > t0 { (tau - t0) / (t1 - t0) } else { 1.0 };
            return c0 + (c1 - c0) * shape.at(x);
        }
    }
    curve.last().map_or(0.0, |c| c.1)
}

fn ease(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f64::consts::PI * x).cos()
}

fn contour(segs: &[Seg], tau: f64, p: &Params) -> f64 {
    if segs.is_empty() {
        return 60.0;
    }
    // The segment sounding at tau: the last one that started (the first for the lead-in).
    let mut k = 0;
    for (i, s) in segs.iter().enumerate() {
        if s.start <= tau + p.transition * 0.5 {
            k = i;
        }
    }
    let seg = &segs[k];
    let mut pitch = seg.pitch;
    if !seg.curve.is_empty() {
        // A drawn curve replaces the glide into the note.
        pitch += curve_at(&seg.curve, tau) / 100.0;
    } else if k > 0 {
        let prev = &segs[k - 1];
        // Glide only between touching notes.
        if seg.start - prev.end < 0.06 {
            let x = (tau - (seg.start - p.transition * 0.5)) / p.transition.max(1e-3);
            pitch = prev.pitch + (seg.pitch - prev.pitch) * ease(x);
        }
    }
    if let Some(v) = seg.own_vibrato {
        // The note's own: over the last `len` of it, faded in and out.
        let len = seg.end - seg.start;
        let from = seg.end - len * v.len.clamp(0.0, 1.0) as f64;
        let t = tau - from;
        if v.depth > 0.0 && t > 0.0 && tau < seg.end {
            let fade_in = (t / (v.fade_in as f64).max(1e-3)).min(1.0);
            let fade_out = ((seg.end - tau) / (v.fade_out as f64).max(1e-3)).min(1.0);
            pitch += v.depth as f64 / 100.0
                * fade_in
                * fade_out
                * (std::f64::consts::TAU * v.rate as f64 * t).sin();
        }
        return pitch;
    }
    // Vibrato, after a delay, on notes that are long enough.
    let t = tau - seg.start - p.vibrato_delay;
    if p.vibrato_depth > 0.0 && t > 0.0 && seg.end - seg.start > p.vibrato_delay + 0.2 {
        let fade = (t / p.vibrato_fade.max(1e-3)).min(1.0);
        pitch += p.vibrato_depth * seg.vibrato / 100.0
            * fade
            * (std::f64::consts::TAU * p.vibrato_rate * t).sin();
    }
    pitch
}

/// Source position (ms in the wav) for `u` seconds into a unit's audio.
fn source_ms(u: &Unit, at: f64) -> f64 {
    let e = &u.entry;
    let cons_out = e.consonant / 1000.0 * u.cs;
    if at < cons_out {
        return u.off + at * 1000.0 / u.cs.max(1e-3);
    }
    let vs = u.off + e.consonant;
    let v_ms = (u.ve - vs).max(1.0);
    let v_s = v_ms / 1000.0;
    let r_s = (u.a_end - u.a_start - cons_out).max(1e-3);
    let v = at - cons_out;
    if r_s <= v_s * 1.001 || v_ms < 80.0 {
        // Squeezed or stretched evenly (UTAU's own behaviour).
        return vs + v * 1000.0 * (v_s / r_s);
    }
    // Longer than the vowel: play its head, ping-pong the steady middle, play its tail.
    let ls = vs + 0.30 * v_ms;
    let le = u.ve - 0.25 * v_ms;
    let l_ms = (le - ls).max(20.0);
    let l_s = l_ms / 1000.0;
    let head = (ls - vs) / 1000.0;
    let tail = (u.ve - le) / 1000.0;
    let mid = (r_s - head - tail).max(1e-3);
    if v < head {
        return vs + v * 1000.0;
    }
    if v >= head + mid {
        return le + (v - head - mid) * 1000.0;
    }
    // An odd number of passes over the middle, so it ends where the tail starts.
    let passes = (2.0 * ((mid / l_s - 1.0) / 2.0).round() + 1.0).max(1.0);
    let x = (v - head) * (passes * l_s / mid);
    let k = (x / l_s).floor();
    let r = x - k * l_s;
    ls + 1000.0 * if (k as i64) % 2 == 0 { r } else { l_s - r }
}

/// Weight of a unit at `tau` (seconds in the phrase): fade in over its
/// overlap, fade out over the next unit's.
fn unit_weights(units: &[Unit], tau: f64, out: &mut Vec<(usize, f64)>) {
    out.clear();
    for (i, u) in units.iter().enumerate() {
        if tau < u.a_start || tau >= u.a_end {
            continue;
        }
        let mut w = 1.0;
        // In: crossfade with the unit before (or a short ramp at a phrase start).
        let fade_in =
            if i > 0 && units[i - 1].a_end > u.a_start && units[i - 1].a_end <= u.a_end + 1.0 {
                (units[i - 1].a_end - u.a_start).max(0.004)
            } else {
                0.004
            };
        w *= ease((tau - u.a_start) / fade_in);
        // Out: the next unit's overlap, or a short release.
        let fade_out = match units.get(i + 1) {
            Some(n) if n.a_start < u.a_end => (u.a_end - n.a_start).max(0.004),
            _ => 0.012,
        };
        w *= ease((u.a_end - tau) / fade_out);
        if w > 1e-6 {
            out.push((i, w));
        }
    }
}

pub fn render_phrase(
    bank: &Bank,
    notes: &[RNote],
    p: &Params,
    rate: u32,
) -> Result<Rendered, String> {
    let mut missing: Vec<String> = Vec::new();
    // 1. Notes → syllables, with their notes' spans.
    struct Syl {
        text: String,
        /// The lyric as typed, tried first: a bank may use its own spelling (`ka`, `a ka`).
        raw: Option<String>,
        t0: f64,
        t1: f64,
        pitch: f64,
        vel: u8,
        gender: f64,
        joined: bool,
    }
    let mut sylls: Vec<Syl> = Vec::new();
    let mut segs: Vec<Seg> = Vec::new();
    let mut prev_end = f64::NEG_INFINITY;
    for n in notes {
        if n.end <= n.start {
            continue;
        }
        let joined = n.start - prev_end <= 0.03;
        prev_end = n.end;
        match lyric::parse(&n.lyric) {
            Token::Rest => {
                prev_end = f64::NEG_INFINITY;
                continue;
            }
            Token::Hold => {
                if let Some(last) = sylls.last_mut() {
                    last.t1 = n.end;
                }
                segs.push(Seg::of(n));
            }
            Token::Sing(list) => {
                segs.push(Seg::of(n));
                let count = list.len();
                let span = (n.end - n.start) / count as f64;
                for (k, s) in list.into_iter().enumerate() {
                    let raw = (count == 1).then(|| n.lyric.trim().to_string());
                    sylls.push(Syl {
                        text: s,
                        raw,
                        t0: n.start + span * k as f64,
                        t1: n.start + span * (k + 1) as f64,
                        pitch: n.pitch,
                        vel: n.velocity,
                        gender: n.gender,
                        joined: joined || k > 0,
                    });
                }
            }
        }
    }
    if sylls.is_empty() {
        return Ok(Rendered {
            lead: 0,
            data: Vec::new(),
            missing,
        });
    }
    // 2. Syllables → bank entries.
    let mut units: Vec<Unit> = Vec::new();
    let mut prev_vowel: Option<&'static str> = None;
    for s in &sylls {
        let mut kana: Vec<String> = s.raw.iter().cloned().collect();
        for k in lyric::spellings(&s.text) {
            if !kana.contains(&k) {
                kana.push(k);
            }
        }
        let pitch = s.pitch.round().clamp(0.0, 127.0) as u8;
        let mut aliases: Vec<String> = Vec::new();
        if s.joined
            && let Some(v) = prev_vowel
        {
            aliases.extend(kana.iter().map(|k| format!("{v} {k}")));
            aliases.extend(kana.iter().map(|k| format!("* {k}")));
        } else {
            aliases.extend(kana.iter().map(|k| format!("- {k}")));
        }
        aliases.extend(kana.iter().cloned());
        let Some(entry) = bank.vb.find(&aliases, pitch).cloned() else {
            if !missing.contains(&s.text) {
                missing.push(s.text.clone());
            }
            prev_vowel = None;
            continue;
        };
        prev_vowel = lyric::vowel_of(&s.text);
        let an = bank.analysis(&entry)?;
        let cs = 2f64.powf(1.0 - s.vel.max(1) as f64 / 100.0).clamp(0.4, 2.5);
        let ve = if entry.cutoff < 0.0 {
            entry.offset - entry.cutoff
        } else {
            an.wav_ms - entry.cutoff
        }
        .clamp(
            entry.offset + entry.consonant + 10.0,
            an.wav_ms.max(entry.offset + entry.consonant + 10.0),
        );
        let a_start = s.t0 - entry.preutter / 1000.0 * cs;
        units.push(Unit {
            gain: (s.vel as f64 / 100.0).powf(0.6).clamp(0.2, 1.5),
            off: entry.offset,
            gender: s.gender,
            ve,
            entry,
            an,
            t0: s.t0,
            t1: s.t1,
            cs,
            a_start,
            a_end: s.t1,
        });
    }
    if units.is_empty() {
        return Ok(Rendered {
            lead: 0,
            data: Vec::new(),
            missing,
        });
    }
    // 3. Where each unit's audio ends: the next one's start plus its overlap.
    for i in 0..units.len() {
        let end = match units.get(i + 1) {
            Some(n) if n.t0 - units[i].t1 <= 0.03 => n.a_start + n.entry.overlap / 1000.0 * n.cs,
            _ => units[i].t1 + 0.04,
        };
        // Never shorter than the unit's consonant, never before it starts.
        let min_end =
            units[i].a_start + units[i].entry.consonant / 1000.0 * units[i].cs * 0.6 + 0.01;
        units[i].a_end = end.max(min_end);
    }
    // 4. Frame domain.
    let origin = units[0].a_start;
    let last_end = units.iter().map(|u| u.a_end).fold(f64::MIN, f64::max);
    let total = (last_end - origin).max(0.01);
    let frames = (total * 1000.0 / FRAME_MS).ceil() as usize + 2;
    let bins = units[0].an.bins;
    let mut f0 = vec![0.0f64; frames];
    let mut sp = vec![0.0f64; frames * bins];
    let mut ap = vec![0.0f64; frames * bins];
    let shift_units: Vec<Unit> = units
        .into_iter()
        .map(|mut u| {
            u.a_start -= origin;
            u.a_end -= origin;
            u.t0 -= origin;
            u.t1 -= origin;
            u
        })
        .collect();
    for s in &mut segs {
        s.start -= origin;
        s.end -= origin;
    }
    let units = shift_units;
    let mut act: Vec<(usize, f64)> = Vec::with_capacity(4);
    let mut ln = vec![0.0f64; bins];
    let mut apv = vec![0.0f64; bins];
    for fi in 0..frames {
        let tau = fi as f64 * FRAME_MS / 1000.0;
        unit_weights(&units, tau, &mut act);
        let wsum: f64 = act.iter().map(|a| a.1).sum();
        if wsum < 1e-6 {
            // Silence: a flat, very quiet unvoiced frame.
            for b in 0..bins {
                sp[fi * bins + b] = 1e-30;
                ap[fi * bins + b] = 1.0;
            }
            continue;
        }
        ln.fill(0.0);
        apv.fill(0.0);
        let mut voiced = 0.0;
        let mut gender = 0.0;
        for &(i, w) in &act {
            let u = &units[i];
            let w = w / wsum;
            let pos = source_ms(u, tau - u.a_start) / FRAME_MS;
            let last = u.an.frames.saturating_sub(1);
            let pos = pos.clamp(0.0, last as f64);
            let (i0, fr) = (pos.floor() as usize, pos - pos.floor());
            let i1 = (i0 + 1).min(last);
            let (l0, l1) = (u.an.lnsp_row(i0), u.an.lnsp_row(i1));
            let (a0, a1) = (u.an.ap_row(i0), u.an.ap_row(i1));
            gender += w * u.gender;
            let g = 2.0 * u.gain.ln();
            for b in 0..bins {
                ln[b] += w * ((l0[b] as f64) * (1.0 - fr) + (l1[b] as f64) * fr + g);
                apv[b] += w * ((a0[b] as f64) * (1.0 - fr) + (a1[b] as f64) * fr);
            }
            let near = if fr < 0.5 { i0 } else { i1 };
            if u.an.f0[near] > 0.0 {
                voiced += w;
            }
        }
        let row = fi * bins;
        let ratio = 2f64.powf((p.gender + gender).clamp(-1.0, 1.0) * 3.0 / 12.0);
        for b in 0..bins {
            // Formant shift: read the envelope from a stretched axis.
            let v = if (ratio - 1.0).abs() > 1e-6 {
                let src = (b as f64 / ratio).clamp(0.0, (bins - 1) as f64);
                let (j, fr) = (src.floor() as usize, src - src.floor());
                let j1 = (j + 1).min(bins - 1);
                ln[j] * (1.0 - fr) + ln[j1] * fr
            } else {
                ln[b]
            };
            sp[row + b] = v.exp();
            let a = apv[b].clamp(0.0, 1.0);
            ap[row + b] = (a + (1.0 - a) * p.breath * 0.7).clamp(0.0, 0.999_999);
        }
        if voiced >= 0.5 {
            // WORLD wants a sane pitch: notes far outside a voice's range stay inside it.
            f0[fi] = semis_to_hz(contour(&segs, tau, p)).clamp(55.0, 1500.0);
        }
    }
    // 5. Synthesis, then to the output rate.
    let y = analysis::synthesise(&f0, &mut sp, &mut ap, bins);
    let y = super::dsp::resample(&y, FS as u32, rate);
    let lead = ((units[0].t0 - units[0].a_start).max(0.0) * rate as f64).round() as usize;
    let mut data: Vec<f32> = y.iter().map(|v| *v as f32).collect();
    let fade = (0.003 * rate as f64) as usize;
    let n = data.len();
    for i in 0..fade.min(n) {
        let g = i as f32 / fade as f32;
        data[i] *= g;
        data[n - 1 - i] *= g;
    }
    Ok(Rendered {
        lead,
        data,
        missing,
    })
}

#[cfg(test)]
mod contour_tests {
    use super::*;
    use crate::sequence::{BAR, NotePitch, PitchPoint};

    fn note(start: f64, end: f64, pitch: f64) -> RNote {
        RNote {
            id: 1,
            start,
            end,
            pitch,
            velocity: 100,
            lyric: "a".into(),
            vibrato: 1.0,
            gender: 0.0,
            curve: Vec::new(),
            own_vibrato: None,
        }
    }

    #[test]
    fn a_drawn_curve_bends_the_note_and_replaces_the_glide() {
        let p = Params {
            vibrato_depth: 0.0,
            ..Params::default()
        };
        let a = note(0.0, 1.0, 60.0);
        let mut b = note(1.0, 2.0, 64.0);
        // From 2 semitones under, up to the note by a quarter, held.
        b.curve = vec![
            (1.0, -200.0, CurveShape::Linear),
            (1.25, 0.0, CurveShape::Smooth),
            (2.0, 0.0, CurveShape::Smooth),
        ];
        let segs = [Seg::of(&a), Seg::of(&b)];
        assert!((contour(&segs, 1.0, &p) - 62.0).abs() < 1e-9);
        assert!(
            (contour(&segs, 1.125, &p) - 63.0).abs() < 1e-9,
            "linear halfway"
        );
        assert!((contour(&segs, 1.6, &p) - 64.0).abs() < 1e-9);
        // Without the curve the glide starts from the note before.
        let plain = [Seg::of(&a), Seg::of(&note(1.0, 2.0, 64.0))];
        assert!(contour(&plain, 1.0, &p) < 64.0 && contour(&plain, 1.0, &p) > 60.0);
    }

    #[test]
    fn a_notes_own_vibrato_replaces_the_channels() {
        let p = Params::default();
        let mut n = note(0.0, 2.0, 60.0);
        n.own_vibrato = Some(NoteVibrato {
            len: 0.5,
            depth: 100.0,
            rate: 5.0,
            fade_in: 0.001,
            fade_out: 0.001,
        });
        let segs = [Seg::of(&n)];
        // Still before the last half; then up to ±1 semitone.
        assert_eq!(contour(&segs, 0.9, &p), 60.0);
        let peak = (0..200)
            .map(|i| (contour(&segs, 1.0 + i as f64 * 0.005, &p) - 60.0).abs())
            .fold(0.0, f64::max);
        assert!((peak - 1.0).abs() < 0.05, "{peak}");
    }

    #[test]
    fn note_pitch_points_join_by_their_shapes() {
        let c = NotePitch {
            points: vec![PitchPoint {
                at: BAR / 2,
                cents: 100.0,
                shape: CurveShape::EaseOut,
            }],
            vibrato: None,
        };
        assert_eq!(c.cents_at(0.0, BAR), 0.0);
        assert_eq!(c.cents_at((BAR / 2) as f64, BAR), 100.0);
        assert_eq!(c.cents_at(BAR as f64, BAR), 0.0);
        // Smooth from the start: a quarter of the way is under the line.
        assert!(c.cents_at((BAR / 8) as f64, BAR) < 25.0);
        let mut messy = NotePitch {
            points: vec![
                PitchPoint {
                    at: 50,
                    cents: 9000.0,
                    shape: CurveShape::Linear,
                },
                PitchPoint {
                    at: -5,
                    cents: 0.0,
                    shape: CurveShape::Linear,
                },
                PitchPoint {
                    at: 50,
                    cents: 1.0,
                    shape: CurveShape::Linear,
                },
            ],
            vibrato: None,
        };
        messy.tidy(40);
        assert_eq!(messy.points.len(), 2);
        assert_eq!(messy.points[0].at, 0);
        assert_eq!(messy.points[1].at, 40);
        assert_eq!(messy.points[1].cents, 2400.0);
    }
}
