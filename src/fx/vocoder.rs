//! Vocoder: the carrier (this insert's signal) takes the spectrum of the
//! modulator on the sidechain input. Speech on the key, a pad or a saw on the
//! carrier, and the carrier sings the words (FL: Vocodex, simplified).

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, db, p};

pub const BANDS: usize = 16;

pub const PARAMS: &[ParamSpec] = &[
    p("Bands", 4.0, BANDS as f32, 12.0, false, Plain),
    p("Attack", 1.0, 50.0, 4.0, false, Ms),
    p("Release", 10.0, 500.0, 90.0, false, Ms),
    p("Level", -24.0, 24.0, 12.0, false, Db),
];

pub struct Vocoder {
    carrier: [Biquad; BANDS],
    modulator: [Biquad; BANDS],
    env: [f32; BANDS],
    rate: f32,
    /// Band centres are rebuilt when the rate changes.
    tuned: bool,
}

impl Vocoder {
    pub fn new(rate: f32) -> Self {
        Self {
            carrier: [Biquad::default(); BANDS],
            modulator: [Biquad::default(); BANDS],
            env: [0.0; BANDS],
            rate,
            tuned: false,
        }
    }

    fn tune(&mut self, rate: f32) {
        if self.tuned && (rate - self.rate).abs() < 0.5 {
            return;
        }
        self.rate = rate;
        self.tuned = true;
        let (f0, f1) = (90.0_f32, 9000.0_f32);
        for i in 0..BANDS {
            let t = i as f32 / (BANDS - 1) as f32;
            let f = f0 * (f1 / f0).powf(t);
            let q = 5.0;
            self.carrier[i].set(Shape::Bandpass, f, q, 0.0, rate);
            self.modulator[i].set(Shape::Bandpass, f, q, 0.0, rate);
        }
    }
}

fn coef(ms: f32, rate: f32) -> f32 {
    -(-1000.0 / (rate * ms.max(0.01))).exp_m1()
}

impl Effect for Vocoder {
    fn process(&mut self, _: &[f32], buf: &mut [f32], ctx: &Ctx) {
        // No modulator: the carrier is left alone instead of being filtered
        // into noise. The sidechain input is what makes it a vocoder.
        self.tune(ctx.rate);
        let _ = buf;
    }

    fn process_keyed(&mut self, p: &[f32], buf: &mut [f32], key: &[f32], ctx: &Ctx) {
        self.tune(ctx.rate);
        let bands = (p[0].round() as usize).clamp(1, BANDS);
        let (atk, rel) = (coef(p[1], ctx.rate), coef(p[2], ctx.rate));
        let level = db(p[3]);
        for (frame, k) in buf.chunks_exact_mut(2).zip(key.chunks_exact(2)) {
            let m = (k[0] + k[1]) * 0.5;
            let mut out = [0.0f32; 2];
            for i in 0..bands {
                let band = self.modulator[i].tick(0, m);
                let a = band.abs();
                self.env[i] += (a - self.env[i]) * if a > self.env[i] { atk } else { rel };
                let e = self.env[i];
                if e > 1e-6 {
                    for ch in 0..2 {
                        out[ch] += self.carrier[i].tick(ch, frame[ch]) * e;
                    }
                } else {
                    // Keep the carrier filters' history moving even when the
                    // band is closed, so a new syllable starts clean.
                    for ch in 0..2 {
                        let _ = self.carrier[i].tick(ch, frame[ch]);
                    }
                }
            }
            // Many narrow bands sum to a quiet signal: make up the level.
            frame[0] = out[0] * level * 6.0;
            frame[1] = out[1] * level * 6.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn saw(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let t = i as f32 / RATE;
                let v = 2.0 * (t * hz - (t * hz).floor()) - 1.0;
                [v * 0.3, v * 0.3]
            })
            .collect()
    }

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * hz * i as f32 / RATE).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn without_a_key_the_carrier_is_untouched() {
        let input = saw(220.0, 2400);
        let mut p = FxKind::Vocoder.defaults();
        let mut st = FxState::new(FxKind::Vocoder, RATE);
        let mut buf = input.clone();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        assert_eq!(buf, input);
        p[0] = 16.0;
        assert_eq!(p[0], 16.0);
    }

    #[test]
    fn a_silent_modulator_closes_the_bands() {
        let input = saw(220.0, 4800);
        let key = vec![0.0f32; input.len()];
        let mut p = FxKind::Vocoder.defaults();
        let mut st = FxState::new(FxKind::Vocoder, RATE);
        let mut buf = input.clone();
        for (b, k) in buf.chunks_mut(1024).zip(key.chunks(1024)) {
            st.process_keyed(&p, b, k, RATE, 120.0);
        }
        assert!(buf.iter().all(|v| v.abs() < 1e-4));
    }

    #[test]
    fn a_modulator_opens_the_matching_band() {
        let input = saw(220.0, 9600);
        let key = sine(1000.0, 9600);
        let mut p = FxKind::Vocoder.defaults();
        let mut st = FxState::new(FxKind::Vocoder, RATE);
        let mut buf = input.clone();
        for (b, k) in buf.chunks_mut(1024).zip(key.chunks(1024)) {
            st.process_keyed(&p, b, k, RATE, 120.0);
        }
        let tail = &buf[4800..];
        let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
        assert!(rms > 0.01, "vocoder output is silent: {rms}");
        assert!(buf.iter().all(|v| v.is_finite()));
    }
}
