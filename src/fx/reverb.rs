//! Freeverb-style reverb with size, damping, width and low-cut controls.

use super::Unit::*;
use super::dsp::{Allpass, Biquad, Comb, Shape};
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Size", 0.0, 100.0, 60.0, false, Pct),
    p("Damping", 0.0, 100.0, 50.0, false, Pct),
    p("Width", 0.0, 100.0, 100.0, false, Pct),
    p("Low cut", 20.0, 1000.0, 120.0, true, Hz),
    p("Wet", 0.0, 100.0, 25.0, false, Pct),
    p("Dry", 0.0, 100.0, 100.0, false, Pct),
];

pub const ROOM_PARAMS: &[ParamSpec] = &[
    PARAMS[0],
    PARAMS[1],
    PARAMS[2],
    p("Low cut", 0.0, 3000.0, 75.0, false, Hz),
    p("Wet", 0.0, 125.0, 50.0, false, Pct),
    p("Dry", 0.0, 125.0, 100.0, false, Pct),
    p("Decay", 100.0, 20_000.0, 1500.0, true, Ms),
    p("High cut", 100.0, 22_100.0, 4000.0, true, Hz),
    p("Predelay", 0.0, 1500.0, 0.0, false, Ms),
    p("Early reflections", 0.0, 125.0, 50.0, false, Pct),
];

/// Freeverb tunings at 44.1 kHz, scaled to the engine rate.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

pub struct Reverb {
    combs: Vec<[Comb; 2]>,
    allpasses: Vec<[Allpass; 2]>,
    low: Biquad,
    last_cut: f32,
    high: Biquad,
    last_high: f32,
    room_input: Vec<[f32; 2]>,
    room_at: usize,
}

impl Reverb {
    pub fn new(rate: f32) -> Self {
        let scale = |n: usize| ((n as f32 * rate / 44_100.0) as usize).max(1);
        Self {
            combs: COMBS
                .iter()
                .map(|n| [Comb::new(scale(*n)), Comb::new(scale(*n + SPREAD))])
                .collect(),
            allpasses: ALLPASSES
                .iter()
                .map(|n| [Allpass::new(scale(*n)), Allpass::new(scale(*n + SPREAD))])
                .collect(),
            low: Biquad::default(),
            last_cut: -1.0,
            high: Biquad::default(),
            last_high: -1.0,
            room_input: Vec::new(),
            room_at: 0,
        }
    }

    pub fn room(rate: f32) -> Self {
        Self {
            room_input: vec![[0.0; 2]; (rate * 1.56) as usize + 1],
            ..Self::new(rate)
        }
    }
}

impl Effect for Reverb {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let Self {
            combs,
            allpasses,
            low,
            last_cut,
            high,
            last_high,
            room_input,
            room_at,
        } = self;
        let room_model = !room_input.is_empty();
        let room = 0.7 + p[0] / 100.0 * 0.28;
        let damp = p[1] / 100.0 * 0.4;
        let width = p[2] / 100.0;
        if *last_cut != p[3] {
            low.set(Shape::Highpass, p[3], 0.707, 0.0, rate);
            *last_cut = p[3];
        }
        let (predelay, early_gain, feedback) = if room_model {
            if *last_high != p[7] {
                high.set(Shape::Lowpass, p[7], 0.707, 0.0, rate);
                *last_high = p[7];
            }
            let predelay = (p[8] * rate / 1000.0) as usize;
            let feedback = COMBS.map(|n| 10f32.powf(-3.0 * (n as f32 / 44_100.0) / (p[6] / 800.0)));
            (predelay, p[9] / 100.0, feedback)
        } else {
            (0, 0.0, [room; COMBS.len()])
        };
        let reflections: [usize; 4] = [0.0075, 0.0100, 0.0146, 0.0193]
            .map(|s| predelay + (s * rate * (0.5 + p[0] / 100.0)) as usize);
        let wet_gain = if room_model { 2.0 } else { 3.0 };
        let (wet, dry) = (p[4] / 100.0 * wet_gain, p[5] / 100.0);
        let (w1, w2) = (wet * (width / 2.0 + 0.5), wet * ((1.0 - width) / 2.0));
        for f in buf.as_chunks_mut::<2>().0 {
            let mut early = [0.0; 2];
            let x = if room_model {
                let len = room_input.len();
                let input = |ch, sample| {
                    let sample = if p[3] > 0.0 {
                        low.tick(ch, sample)
                    } else {
                        sample
                    };
                    high.tick(ch, sample)
                };
                let mut input = input;
                room_input[*room_at] = [input(0, f[0]), input(1, f[1])];
                let tap = |offset: usize| room_input[(*room_at + len - offset.min(len - 1)) % len];
                let direct = tap(predelay);
                let [a, b, c, d] = reflections.map(tap);
                early = [
                    (0.028 * a[0] + 0.022 * b[1] - 0.018 * c[0] + 0.024 * d[1]) * early_gain,
                    (0.028 * b[1] + 0.022 * a[0] - 0.018 * d[1] + 0.024 * c[0]) * early_gain,
                ];
                *room_at = (*room_at + 1) % len;
                (direct[0] + direct[1]) * 0.015
            } else {
                low.tick(0, (f[0] + f[1]) * 0.015)
            };
            let mut out = [0.0f32; 2];
            for (c, feedback) in combs.iter_mut().zip(feedback) {
                out[0] += c[0].tick(x, feedback, damp);
                out[1] += c[1].tick(x, feedback, damp);
            }
            for a in allpasses.iter_mut() {
                out[0] = a[0].tick(out[0]);
                out[1] = a[1].tick(out[1]);
            }
            let (l, r) = (f[0], f[1]);
            f[0] = l * dry + out[0] * w1 + out[1] * w2 + early[0];
            f[1] = r * dry + out[1] * w1 + out[0] * w2 + early[1];
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fx::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }
    fn run(kind: FxKind, set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = kind.defaults();
        set(&mut p);
        let mut st = FxState::new(kind, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn reverb_rings_and_decays() {
        let mut imp = vec![0.0f32; 2 * (RATE as usize * 4)];
        imp[0] = 1.0;
        imp[1] = 1.0;
        let out = run(FxKind::Reverb, |p| p[5] = 0.0, &imp);
        let s = RATE as usize * 2;
        let early = rms(&out[s / 4..s / 2]);
        let late = rms(&out[3 * s..4 * s - 2]);
        assert!(early > 1e-4, "tail exists");
        assert!(late < early * 0.1, "and decays: {early} -> {late}");
    }
}
