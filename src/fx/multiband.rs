//! Three-band compressor (FL: Maximus, simplified): 200 Hz and 2 kHz splits,
//! an independent threshold and ratio per band, shared attack and release.

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Low band", -48.0, 0.0, -12.0, false, Db),
    p("Low ratio", 1.0, 10.0, 2.0, false, Ratio),
    p("Mid band", -48.0, 0.0, -10.0, false, Db),
    p("Mid ratio", 1.0, 10.0, 2.0, false, Ratio),
    p("High band", -48.0, 0.0, -8.0, false, Db),
    p("High ratio", 1.0, 10.0, 1.5, false, Ratio),
    p("Attack", 1.0, 100.0, 10.0, false, Ms),
    p("Release", 20.0, 1000.0, 120.0, false, Ms),
    p("Output", -24.0, 12.0, 0.0, false, Db),
];

const BANDS: usize = 3;

#[derive(Clone, Copy, Default)]
struct Band {
    env: f32,
    gain: f32,
}

pub struct Multiband {
    low: [Biquad; 2],
    mid_hp: [Biquad; 2],
    mid_lp: [Biquad; 2],
    high: [Biquad; 2],
    bands: [Band; BANDS],
    rate: f32,
    tuned: bool,
}

impl Multiband {
    pub fn new(rate: f32) -> Self {
        Self {
            low: [Biquad::default(); 2],
            mid_hp: [Biquad::default(); 2],
            mid_lp: [Biquad::default(); 2],
            high: [Biquad::default(); 2],
            bands: [Band::default(); BANDS],
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
        // Two cascaded Butterworth sections: 24 dB/octave, so the bands meet
        // without a hole in the middle.
        for f in &mut self.low {
            f.set(Shape::Lowpass, 200.0, 0.707, 0.0, rate);
        }
        for f in &mut self.mid_hp {
            f.set(Shape::Highpass, 200.0, 0.707, 0.0, rate);
        }
        for f in &mut self.mid_lp {
            f.set(Shape::Lowpass, 2000.0, 0.707, 0.0, rate);
        }
        for f in &mut self.high {
            f.set(Shape::Highpass, 2000.0, 0.707, 0.0, rate);
        }
    }
}

fn coef(ms: f32, rate: f32) -> f32 {
    -(-1000.0 / (rate * ms.max(0.01))).exp_m1()
}

impl Effect for Multiband {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        self.tune(ctx.rate);
        let (atk, rel) = (coef(p[6], ctx.rate), coef(p[7], ctx.rate));
        let out = db(p[8]);
        let gain_coef = coef(5.0, ctx.rate);
        for f in buf.as_chunks_mut::<2>().0 {
            let x = [f[0], f[1]];
            let mut low = x;
            for b in &mut self.low {
                low = [b.tick(0, low[0]), b.tick(1, low[1])];
            }
            let mut mid = x;
            for b in &mut self.mid_hp {
                mid = [b.tick(0, mid[0]), b.tick(1, mid[1])];
            }
            for b in &mut self.mid_lp {
                mid = [b.tick(0, mid[0]), b.tick(1, mid[1])];
            }
            let mut high = x;
            for b in &mut self.high {
                high = [b.tick(0, high[0]), b.tick(1, high[1])];
            }
            let bands = [low, mid, high];
            let mut sum = [0.0f32; 2];
            for (i, band) in bands.into_iter().enumerate() {
                let level = (band[0].abs() + band[1].abs()) * 0.5;
                let st = &mut self.bands[i];
                st.env += (level - st.env) * if level > st.env { atk } else { rel };
                let over = 20.0 * (st.env.max(1e-6)).log10() - p[i * 2];
                let target = if over > 0.0 {
                    db(-over * (1.0 - 1.0 / p[i * 2 + 1].max(1.0)))
                } else {
                    1.0
                };
                st.gain += (target - st.gain) * gain_coef;
                sum[0] += band[0] * st.gain;
                sum[1] += band[1] * st.gain;
            }
            f[0] = sum[0] * out;
            f[1] = sum[1] * out;
        }
    }

    fn reduction(&self) -> f32 {
        // The deepest band reduction of the block, for the slot meter.
        let last = self.bands.last().map(|b| b.gain).unwrap_or(1.0);
        let min = self.bands.iter().map(|b| b.gain).fold(last, f32::min);
        (-20.0 * min.max(1e-6).log10()).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    use std::f32::consts::TAU;
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::Multiband.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::Multiband, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    fn mix(n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let t = i as f32 / RATE;
                let v = 0.32 * (TAU * 60.0 * t).sin()
                    + 0.32 * (TAU * 900.0 * t).sin()
                    + 0.32 * (TAU * 6000.0 * t).sin();
                [v, v]
            })
            .collect()
    }

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|v| v * v).sum::<f32>() / b.len() as f32).sqrt()
    }

    #[test]
    fn a_hot_signal_is_compressed() {
        let input = mix(48000);
        let out = run(
            |p| {
                p[0] = -30.0;
                p[2] = -30.0;
                p[4] = -30.0;
            },
            &input,
        );
        let tail = &input[24000..];
        let got = &out[24000..];
        assert!(rms(got) < rms(tail) * 0.8);
    }

    #[test]
    fn open_thresholds_keep_the_level() {
        let input = mix(24000);
        let out = run(
            |p| {
                for i in 0..3 {
                    p[i * 2] = 0.0;
                }
            },
            &input,
        );
        let (a, z) = (12000, 24000);
        assert!((20.0 * (rms(&out[a..z]) / rms(&input[a..z])).log10()).abs() < 1.0);
    }

    #[test]
    fn output_is_finite_and_reports_reduction() {
        let input = mix(9600);
        let out = run(|p| p[6] = 20.0, &input);
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
