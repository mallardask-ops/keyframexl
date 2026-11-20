//! Fruity Squeeze: bit-depth loss, the Puncher (samples replaced by a fixed
//! level) and a low-pass, in FL's order.

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Squarize", 0.0, 100.0, 40.0, false, Pct),
    p("Preserve", 0.0, 100.0, 30.0, false, Pct),
    p("Impact", 0.0, 100.0, 30.0, false, Pct),
    p("Relation", 0.0, 3.0, 0.0, false, Plain),
    p("Amount", 0.0, 100.0, 0.0, false, Pct),
    p("Mix", 0.0, 100.0, 100.0, false, Pct),
    p("Gain", 0.0, 200.0, 100.0, false, Pct),
    p("Filter freq", 20.0, 20000.0, 12000.0, true, Hz),
    p("Filter reso", 0.0, 100.0, 10.0, false, Pct),
    p("Pre/Post", 0.0, 1.0, 0.0, false, Plain),
];

pub struct Squeeze {
    filter: Biquad,
    /// Samples left in the current Puncher phase.
    left: usize,
    punching: bool,
}

impl Squeeze {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            filter: Biquad::default(),
            left: 0,
            punching: false,
        }
    }
}

impl Effect for Squeeze {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate.max(1000.0);
        let q = 0.5 + p[8] / 100.0 * 9.5;
        self.filter
            .set(Shape::Lowpass, p[7].min(rate * 0.45), q, 0.0, rate);
        // Squarize: 16 bits down to 1.
        let bits = 16.0 - (p[0] / 100.0).clamp(0.0, 1.0) * 15.0;
        let levels = 2f32.powf(bits - 1.0);
        let preserve = ((p[1] / 100.0) * 0.05 * rate) as usize;
        let mut impact = ((p[2] / 100.0) * 0.05 * rate) as usize;
        impact = match p[3].round() as usize {
            1 => preserve,
            2 => preserve * 2,
            3 => preserve / 2,
            _ => impact,
        };
        let amount = (p[4] / 100.0).clamp(0.0, 1.0);
        let pre = p[9] >= 0.5;
        let mix = p[5] / 100.0;
        let gain = p[6] / 100.0;
        for f in buf.as_chunks_mut::<2>().0 {
            let dry = [f[0], f[1]];
            if self.left == 0 {
                // No impact: the Puncher is off (FL's default), not one
                // sample in every cycle.
                self.punching = !self.punching && impact > 0;
                // A zero-length phase would stick; one sample keeps it moving.
                self.left = if self.punching { impact } else { preserve }.max(1);
            }
            for ch in 0..2 {
                let mut x = dry[ch];
                if pre {
                    x = self.filter.tick(ch, x);
                }
                // Puncher: the impacted samples become a fixed level (with the
                // input's sign, so no DC).
                if self.punching {
                    x = x.signum() * amount;
                } else {
                    x = (x * levels).round() / levels;
                }
                if !pre {
                    x = self.filter.tick(ch, x);
                }
                f[ch] = (dry[ch] * (1.0 - mix) + x * mix) * gain;
            }
            self.left -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::Squeeze.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::Squeeze, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn full_squarize_collapses_the_levels() {
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| {
                let v = i as f32 / 4800.0 * 2.0 - 1.0;
                [v, v]
            })
            .collect();
        let out = run(
            |p| {
                p[0] = 100.0;
                p[9] = 1.0; // filter before the quantizer, so the levels stay steps
                p[5] = 100.0;
            },
            &input,
        );
        let mut levels: Vec<i32> = out.chunks(2).map(|f| (f[0] * 1000.0) as i32).collect();
        levels.sort_unstable();
        levels.dedup();
        assert!(levels.len() <= 4, "{} levels", levels.len());
    }

    #[test]
    fn the_puncher_silences_impacted_samples() {
        let input = vec![0.5f32; 4800];
        let out = run(
            |p| {
                p[1] = 30.0; // preserve
                p[2] = 50.0; // impact
                p[3] = 0.0;
                p[4] = 0.0; // amount 0
                p[7] = 20000.0;
                p[5] = 100.0;
            },
            &input,
        );
        assert!(out.iter().any(|v| v.abs() < 1e-6), "nothing punched");
        assert!(out.iter().any(|v| (*v - 0.5).abs() < 1e-3), "all punched");
    }

    #[test]
    fn mix_zero_is_dry() {
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| [(i as f32 * 0.03).sin(), (i as f32 * 0.02).cos()])
            .collect();
        let out = run(|p| p[5] = 0.0, &input);
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
