//! Hyper Chorus: four to eight modulated delay lines, each with its own LFO
//! phase and auto-pan, plus feedback and input/output filters.

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, p};
use std::f32::consts::TAU;

pub const PARAMS: &[ParamSpec] = &[
    p("Delay", 1.0, 30.0, 12.0, false, Ms),
    p("Feedback", 0.0, 90.0, 20.0, false, Pct),
    p("Modulation", 0.0, 100.0, 40.0, false, Pct),
    p("Rate", 0.05, 5.0, 0.6, true, Hz),
    p("Movement", 0.0, 100.0, 40.0, false, Pct),
    p("Move rate", 0.05, 5.0, 0.2, true, Hz),
    p("Lines", 4.0, 8.0, 8.0, false, Plain),
    p("High pass", 20.0, 2000.0, 80.0, true, Hz),
    p("Low pass", 2000.0, 20000.0, 12000.0, true, Hz),
    p("Phase diff", 0.0, 100.0, 50.0, false, Pct),
    p("Mix", 0.0, 100.0, 50.0, false, Pct),
];

const MAX_S: f32 = 0.1;

pub struct HyperChorus {
    ring: Vec<[f32; 2]>,
    at: usize,
    phase: f32,
    pan_phase: f32,
    hp: Biquad,
    lp: Biquad,
    last: [f32; 2],
}

impl HyperChorus {
    pub fn new(rate: f32) -> Self {
        Self {
            ring: vec![[0.0; 2]; (MAX_S * rate.max(1000.0)) as usize],
            at: 0,
            phase: 0.0,
            pan_phase: 0.0,
            hp: Biquad::default(),
            lp: Biquad::default(),
            last: [0.0; 2],
        }
    }

    fn read(&self, ch: usize, delay: f32) -> f32 {
        let n = self.ring.len();
        let d = delay.rem_euclid(n as f32);
        let pos = (self.at as f32 - 1.0 - d).rem_euclid(n as f32);
        let i0 = pos.floor() as usize % n;
        let i1 = (i0 + 1) % n;
        let t = pos - pos.floor();
        self.ring[i0][ch] + (self.ring[i1][ch] - self.ring[i0][ch]) * t
    }
}

impl Effect for HyperChorus {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate.max(1000.0);
        let lines = (p[6].round() as usize).clamp(4, 8);
        let base = p[0] / 1000.0 * rate;
        let depth = p[2] / 100.0 * 0.004 * rate;
        let fb = p[1] / 100.0;
        let pan_amt = p[4] / 100.0;
        let diff = p[9] / 100.0 * TAU;
        let mix = p[10] / 100.0;
        let hp = p[7].min(p[8] * 0.8);
        self.hp.set(Shape::Highpass, hp, 0.707, 0.0, rate);
        self.lp
            .set(Shape::Lowpass, p[8].min(rate * 0.45), 0.707, 0.0, rate);
        let norm = std::f32::consts::SQRT_2 / lines as f32;
        let n = self.ring.len();
        for f in buf.as_chunks_mut::<2>().0 {
            let dry = [f[0], f[1]];
            let (mut wl, mut wr) = (0.0f32, 0.0f32);
            for i in 0..lines {
                let off = diff * i as f32 / lines as f32;
                let d = base + depth * (self.phase + off).sin();
                let l = self.read(0, d);
                let r = self.read(1, d);
                // Each line drifts across the panorama at its own pace.
                let pan = pan_amt * (self.pan_phase + off).sin();
                let lg = ((1.0 - pan) * 0.5).sqrt();
                let rg = ((1.0 + pan) * 0.5).sqrt();
                wl += l * lg;
                wr += r * rg;
            }
            let mut wet = [wl * norm, wr * norm];
            wet[0] = self.hp.tick(0, wet[0]);
            wet[1] = self.hp.tick(1, wet[1]);
            wet[0] = self.lp.tick(0, wet[0]);
            wet[1] = self.lp.tick(1, wet[1]);
            // The feedback taps the wet sum, not the dry.
            self.ring[self.at] = [dry[0] + self.last[0] * fb, dry[1] + self.last[1] * fb];
            self.at = (self.at + 1) % n;
            self.last = wet;
            f[0] = dry[0] + (wet[0] - dry[0]) * mix;
            f[1] = dry[1] + (wet[1] - dry[1]) * mix;
            self.phase = (self.phase + TAU * p[3] / rate) % TAU;
            self.pan_phase = (self.pan_phase + TAU * p[5] / rate) % TAU;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    use std::f32::consts::TAU;
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::HyperChorus.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::HyperChorus, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn mix_zero_is_dry() {
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| [(i as f32 * 0.03).sin(), (i as f32 * 0.01).cos()])
            .collect();
        let out = run(|p| p[10] = 0.0, &input);
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn the_lines_delay_the_signal() {
        let mut input = vec![0.0f32; 9600];
        input[0] = 1.0;
        input[1] = 1.0;
        let out = run(
            |p| {
                p[0] = 5.0; // 5 ms
                p[1] = 0.0; // no feedback
                p[2] = 0.0; // no modulation
                p[4] = 0.0; // no movement
                p[7] = 20.0;
                p[8] = 20000.0;
                p[10] = 100.0;
            },
            &input,
        );
        let at = (0.005 * RATE) as usize;
        let peak = |ch: usize| {
            out[2 * (at - 3)..2 * (at + 4)]
                .chunks(2)
                .map(|f| f[ch].abs())
                .fold(0.0f32, f32::max)
        };
        assert!(peak(0) > 0.3, "no delayed left: {}", peak(0));
        assert!(peak(1) > 0.3);
    }

    #[test]
    fn movement_makes_the_channels_differ() {
        let input: Vec<f32> = (0..9600)
            .map(|i| (TAU * 440.0 * i as f32 / RATE).sin())
            .flat_map(|v| [v, v])
            .collect();
        let out = run(
            |p| {
                p[4] = 100.0;
                p[5] = 1.5;
                p[10] = 100.0;
            },
            &input,
        );
        let diff: f32 = out.chunks(2).skip(4800).map(|f| (f[0] - f[1]).abs()).sum();
        assert!(diff > 1.0, "the stereo image did not move: {diff}");
    }
}
