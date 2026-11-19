//! Mid/side shaping: stereo width, a mono bass, and independent mid and side
//! levels. Solo hears one side of the signal (FL: Stereo Shaper's mid/side).

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Width", 0.0, 200.0, 100.0, false, Pct),
    p("Mono below", 0.0, 1000.0, 0.0, false, Hz),
    p("Mid", -24.0, 24.0, 0.0, false, Db),
    p("Side", -24.0, 24.0, 0.0, false, Db),
    p("Solo", 0.0, 2.0, 0.0, false, Plain),
];

pub struct MidSide {
    /// Two one-pole low passes, subtracted from the side (a 12 dB/octave
    /// high pass).
    lp: [f32; 2],
    rate: f32,
}

impl MidSide {
    pub fn new(rate: f32) -> Self {
        Self { lp: [0.0; 2], rate }
    }
}

impl Effect for MidSide {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        self.rate = ctx.rate.max(1000.0);
        let width = p[0] / 100.0;
        let (mid_gain, side_gain) = (db(p[2]), db(p[3]));
        let solo = p[4].round() as usize;
        // Two one-pole high passes on the side: a gentle 12 dB/octave mono
        // maker, so the bass never fights between the speakers.
        let coef = if p[1] > 0.0 {
            (-std::f32::consts::TAU * p[1].min(self.rate * 0.45) / self.rate).exp()
        } else {
            0.0
        };
        for f in buf.chunks_exact_mut(2) {
            let mid = (f[0] + f[1]) * 0.5;
            let mut side = (f[0] - f[1]) * 0.5 * width;
            if coef > 0.0 {
                for stage in &mut self.lp {
                    *stage = coef * *stage + (1.0 - coef) * side;
                    side -= *stage;
                }
            }
            let mut m = mid * mid_gain;
            let mut s = side * side_gain;
            if solo == 1 {
                s = 0.0;
            } else if solo == 2 {
                m = 0.0;
            }
            f[0] = m + s;
            f[1] = m - s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::MidSide.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::MidSide, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    fn antiphase(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * hz * i as f32 / RATE).sin() * 0.5;
                [v, -v]
            })
            .collect()
    }

    #[test]
    fn width_zero_folds_the_signal_to_mono() {
        let input = antiphase(200.0, 4800);
        let out = run(|p| p[0] = 0.0, &input);
        for f in out.chunks_exact(2) {
            assert!((f[0] - f[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn mono_below_removes_low_side_energy() {
        let input = antiphase(80.0, 9600);
        let out = run(|p| p[1] = 300.0, &input);
        let rms = |b: &[f32]| {
            let s: f32 = b[4800..].iter().map(|v| v * v).sum::<f32>() / (b.len() - 4800) as f32;
            s.sqrt()
        };
        assert!(rms(&out) < rms(&input) * 0.25);
    }

    #[test]
    fn solo_mid_leaves_one_centre_signal() {
        let mut input = antiphase(200.0, 4800);
        for (i, f) in input.chunks_exact_mut(2).enumerate() {
            f[1] += (i as f32 * 0.01).sin() * 0.1;
        }
        let out = run(|p| p[4] = 1.0, &input);
        for f in out.chunks_exact(2) {
            assert!((f[0] - f[1]).abs() < 1e-6);
        }
    }
}
