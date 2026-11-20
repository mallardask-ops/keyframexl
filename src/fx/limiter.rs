//! Limiter: input gain, ceiling and release with instant attack.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Input", -12.0, 24.0, 0.0, false, Db),
    p("Ceiling", -24.0, 0.0, -0.3, false, Db),
    p("Release", 1.0, 500.0, 50.0, true, Ms),
];

pub struct Limiter {
    gain: f32,
    /// Lowest gain of the last block.
    least: f32,
}

impl Limiter {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            gain: 1.0,
            least: 1.0,
        }
    }
}

impl Effect for Limiter {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let gain = &mut self.gain;
        let input = db(p[0]);
        let ceil = db(p[1]);
        let rel = 1.0 - (-1.0 / (p[2] * 0.001 * rate)).exp();
        let mut least = 1.0f32;
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0] * input, f[1] * input);
            let peak = l.abs().max(r.abs());
            let need = if peak > ceil { ceil / peak } else { 1.0 };
            // Instant attack, smooth release: never above the ceiling.
            *gain = (*gain + rel * (1.0 - *gain)).min(need);
            least = least.min(*gain);
            f[0] = l * *gain;
            f[1] = r * *gain;
        }
        self.least = least;
    }

    fn reduction(&self) -> f32 {
        -20.0 * self.least.max(1e-6).log10()
    }
}

#[cfg(test)]
mod tests {
    use crate::fx::{FxKind, FxState};
    use std::f32::consts::PI;
    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * PI * hz * i as f32 / RATE).sin() * amp;
                [v, v]
            })
            .collect()
    }
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
    fn limiter_never_exceeds_the_ceiling() {
        let hot = sine(220.0, 1.0, 48_000);
        let out = run(
            FxKind::Limiter,
            |p| {
                p[0] = 6.0;
                p[1] = -6.0;
            },
            &hot,
        );
        let ceil = 10f32.powf(-6.0 / 20.0);
        assert!(out.iter().all(|x| x.abs() <= ceil + 1e-6));
        assert!(rms(&out[24_000..]) > ceil * 0.5, "still loud");
    }
}
