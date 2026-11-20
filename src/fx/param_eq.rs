//! Seven-band parametric EQ (FL: Parametric EQ 2): low shelf, five peaks, high shelf.

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("1 Low freq", 20.0, 1000.0, 50.0, true, Hz),
    p("1 Gain", -18.0, 18.0, 0.0, false, Db),
    p("1 Q", 0.2, 4.0, 0.7, true, Plain),
    p("2 Freq", 20.0, 20000.0, 120.0, true, Hz),
    p("2 Gain", -18.0, 18.0, 0.0, false, Db),
    p("2 Q", 0.2, 10.0, 1.0, true, Plain),
    p("3 Freq", 20.0, 20000.0, 300.0, true, Hz),
    p("3 Gain", -18.0, 18.0, 0.0, false, Db),
    p("3 Q", 0.2, 10.0, 1.0, true, Plain),
    p("4 Freq", 20.0, 20000.0, 800.0, true, Hz),
    p("4 Gain", -18.0, 18.0, 0.0, false, Db),
    p("4 Q", 0.2, 10.0, 1.0, true, Plain),
    p("5 Freq", 20.0, 20000.0, 2000.0, true, Hz),
    p("5 Gain", -18.0, 18.0, 0.0, false, Db),
    p("5 Q", 0.2, 10.0, 1.0, true, Plain),
    p("6 Freq", 20.0, 20000.0, 5000.0, true, Hz),
    p("6 Gain", -18.0, 18.0, 0.0, false, Db),
    p("6 Q", 0.2, 10.0, 1.0, true, Plain),
    p("7 High freq", 1000.0, 20000.0, 12000.0, true, Hz),
    p("7 Gain", -18.0, 18.0, 0.0, false, Db),
    p("7 Q", 0.2, 4.0, 0.7, true, Plain),
];

pub struct ParamEq {
    bands: [Biquad; 7],
    last: [f32; 21],
}

impl ParamEq {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            bands: [Biquad::default(); 7],
            last: [f32::NAN; 21],
        }
    }
}

impl Effect for ParamEq {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let Self { bands, last } = self;
        if last[..] != p[..21] {
            for (b, band) in bands.iter_mut().enumerate() {
                let shape = match b {
                    0 => Shape::LowShelf,
                    6 => Shape::HighShelf,
                    _ => Shape::Peak,
                };
                band.set(shape, p[b * 3], p[b * 3 + 2], p[b * 3 + 1], rate);
            }
            last.copy_from_slice(&p[..21]);
        }
        // Bands at 0 dB pass the signal untouched.
        for (b, band) in bands.iter_mut().enumerate() {
            if p[b * 3 + 1] == 0.0 {
                continue;
            }
            for f in buf.as_chunks_mut::<2>().0 {
                f[0] = band.tick(0, f[0]);
                f[1] = band.tick(1, f[1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fx::{FxKind, FxState};
    use std::f32::consts::PI;
    const RATE: f32 = 48_000.0;

    fn tone(hz: f32, n: usize, right: f32) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * PI * hz * i as f32 / RATE).sin() * 0.5;
                [v, v * right]
            })
            .collect()
    }
    fn rms(v: &[f32], ch: usize) -> f32 {
        let s: Vec<f32> = v
            .as_chunks::<2>()
            .0
            .iter()
            .skip(2000)
            .map(|f| f[ch])
            .collect();
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }
    fn run(kind: FxKind, p: &[f32], buf: Vec<f32>) -> Vec<f32> {
        let mut st = FxState::new(kind, RATE);
        let mut buf = buf;
        for c in buf.chunks_mut(512) {
            st.process(p, c, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn param_eq_is_transparent_flat_and_each_band_works_alone() {
        let input = tone(1000.0, 9600, 1.0);
        let flat = FxKind::ParamEq.defaults();
        assert_eq!(
            run(FxKind::ParamEq, &flat, input.clone()),
            input,
            "all bands at 0 dB: bit-identical"
        );
        // Band 4 (800 Hz default) moved onto 1 kHz, +12 dB.
        let mut p = flat.clone();
        p[9] = 1000.0;
        p[10] = 12.0;
        let boosted = run(FxKind::ParamEq, &p, input.clone());
        let gain = 20.0 * (rms(&boosted, 0) / rms(&input, 0)).log10();
        assert!((gain - 12.0).abs() < 0.5, "peak band gain {gain}");
        // A low shelf cut leaves 1 kHz nearly alone.
        let mut p = flat;
        p[0] = 100.0;
        p[1] = -18.0;
        let cut = run(FxKind::ParamEq, &p, input.clone());
        assert!((rms(&cut, 0) / rms(&input, 0) - 1.0).abs() < 0.05);
    }
}
