//! Transient shaper: raises or cuts the attack, and shapes the body after it.
//! Two envelopes tell an attack from the sustain: when the fast one is above
//! the slow one, the gain comes from Attack; otherwise from Sustain.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Attack", -100.0, 100.0, 0.0, false, Pct),
    p("Sustain", -100.0, 100.0, 0.0, false, Pct),
    p("Output", -12.0, 12.0, 0.0, false, Db),
];

pub struct Transient {
    fast: f32,
    slow: f32,
    gain: f32,
    rate: f32,
}

impl Transient {
    pub fn new(rate: f32) -> Self {
        Self {
            fast: 0.0,
            slow: 0.0,
            gain: 1.0,
            rate,
        }
    }

    fn set_rate(&mut self, rate: f32) {
        if (rate - self.rate).abs() > 0.5 {
            self.rate = rate;
        }
    }
}

fn coef(ms: f32, rate: f32) -> f32 {
    -(-1000.0 / (rate * ms.max(0.01))).exp_m1()
}

impl Effect for Transient {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        self.set_rate(ctx.rate);
        let rate = self.rate;
        let (fa, fr) = (coef(0.3, rate), coef(20.0, rate));
        // The reference follows the peak envelope slowly: a click leaves it
        // behind, a steady tone lets it catch up.
        let (sa, sr) = (coef(30.0, rate), coef(300.0, rate));
        let smooth = coef(2.0, rate);
        let attack = p[0] / 100.0;
        let sustain = p[1] / 100.0;
        for f in buf.chunks_exact_mut(2) {
            let x = (f[0] + f[1]) * 0.5;
            let a = x.abs();
            self.fast += (a - self.fast) * if a > self.fast { fa } else { fr };
            self.slow += (self.fast - self.slow) * if self.fast > self.slow { sa } else { sr };
            let tr = if self.fast > self.slow {
                (self.fast - self.slow) / self.fast.max(1e-5)
            } else {
                0.0
            };
            // Attacks take the Attack gain; the body takes the Sustain one.
            let target = db(attack * 12.0 * tr + sustain * 12.0 * (1.0 - tr) + p[2]);
            self.gain += (target - self.gain) * smooth;
            let g = self.gain.clamp(0.0, 4.0);
            f[0] *= g;
            f[1] *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::Transient.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::Transient, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    /// A click, then a steady tone: the attack and the body are separate.
    fn struck(n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = if i < 48 {
                    0.9
                } else {
                    0.2 * (i as f32 * 0.05).sin()
                };
                [v, v]
            })
            .collect()
    }

    fn rms(b: &[f32], a: usize, z: usize) -> f32 {
        let s: f32 = b[a..z].iter().map(|v| v * v).sum::<f32>() / (z - a) as f32;
        s.sqrt()
    }

    #[test]
    fn attack_raises_the_peak_not_the_body() {
        let input = struck(9600);
        let out = run(|p| p[0] = 100.0, &input);
        let peak = |b: &[f32]| b[..96].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak(&out) > peak(&input) * 1.5);
        // The steady body after the attack keeps its level (within 1 dB).
        let (a, z) = (4800, 9600);
        assert!((rms(&out, a, z) / rms(&input, a, z) - 1.0).abs() < 0.12);
    }

    #[test]
    fn sustain_cuts_the_body_not_the_peak() {
        let input = struck(9600);
        let out = run(|p| p[1] = -100.0, &input);
        let peak = |b: &[f32]| b[..96].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak(&out) - peak(&input)).abs() / peak(&input) < 0.15);
        let (a, z) = (4800, 9600);
        assert!(rms(&out, a, z) < rms(&input, a, z) * 0.6);
    }

    #[test]
    fn zero_settings_pass_the_signal() {
        let input = struck(4800);
        let out = run(|_| {}, &input);
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 0.02);
        }
    }
}
