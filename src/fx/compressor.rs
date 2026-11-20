//! Compressor: threshold, ratio, attack, release and makeup gain.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Threshold", -60.0, 0.0, -18.0, false, Db),
    p("Ratio", 1.0, 20.0, 4.0, true, Ratio),
    p("Attack", 0.1, 100.0, 10.0, true, Ms),
    p("Release", 10.0, 1000.0, 120.0, true, Ms),
    p("Makeup", 0.0, 24.0, 0.0, false, Db),
];

pub struct Compressor {
    env: f32,
    /// Lowest gain of the last block (the meter shows it as dB).
    least: f32,
}

impl Compressor {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            env: 0.0,
            least: 1.0,
        }
    }
}

impl Compressor {
    /// Compress `buf`; the detector follows `key` when given, else `buf` itself.
    fn run(&mut self, p: &[f32], buf: &mut [f32], key: Option<&[f32]>, ctx: &Ctx) {
        let rate = ctx.rate;
        let env = &mut self.env;
        let (thr, ratio) = (p[0], p[1]);
        let att = (-1.0 / (p[2] * 0.001 * rate)).exp();
        let rel = (-1.0 / (p[3] * 0.001 * rate)).exp();
        let makeup = db(p[4]);
        let mut least = 1.0f32;
        for (i, f) in buf.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let d = match key {
                Some(k) => [
                    k.get(2 * i).copied().unwrap_or(0.0),
                    k.get(2 * i + 1).copied().unwrap_or(0.0),
                ],
                None => [f[0], f[1]],
            };
            let peak = d[0].abs().max(d[1].abs());
            let k = if peak > *env { att } else { rel };
            *env = peak + k * (*env - peak);
            let level = 20.0 * env.max(1e-9).log10();
            let over = level - thr;
            let gain = if over > 0.0 {
                db(-over * (1.0 - 1.0 / ratio))
            } else {
                1.0
            };
            least = least.min(gain);
            f[0] *= gain * makeup;
            f[1] *= gain * makeup;
        }
        self.least = least;
    }
}

impl Effect for Compressor {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        self.run(p, buf, None, ctx);
    }

    fn process_keyed(&mut self, p: &[f32], buf: &mut [f32], key: &[f32], ctx: &Ctx) {
        self.run(p, buf, Some(key), ctx);
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
    fn compressor_reduces_loud_parts() {
        let loud = sine(220.0, 0.9, 48_000);
        let out = run(
            FxKind::Compressor,
            |p| {
                p[0] = -24.0;
                p[1] = 8.0;
            },
            &loud,
        );
        assert!(rms(&out[24_000..]) < rms(&loud[24_000..]) * 0.4);
    }

    #[test]
    fn a_keyed_compressor_ducks_on_the_key_not_its_own_level() {
        let pad = sine(220.0, 0.3, 48_000);
        let mut key = vec![0.0f32; pad.len()];
        // A loud hit in the second half only.
        for v in &mut key[48_000..] {
            *v = 0.9;
        }
        let mut p = FxKind::Compressor.defaults();
        p[0] = -24.0;
        p[1] = 10.0;
        let mut st = FxState::new(FxKind::Compressor, RATE);
        let mut buf = pad.clone();
        for (b, k) in buf.chunks_mut(512).zip(key.chunks(512)) {
            st.process_keyed(&p, b, k, RATE, 120.0);
        }
        let quiet = rms(&buf[..40_000]) / rms(&pad[..40_000]);
        let ducked = rms(&buf[60_000..]) / rms(&pad[60_000..]);
        assert!(quiet > 0.98, "no key, no ducking: {quiet}");
        assert!(ducked < 0.25, "the key ducks the pad: {ducked}");
    }
}
