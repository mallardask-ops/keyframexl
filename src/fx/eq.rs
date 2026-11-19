//! 3-band EQ (low shelf, peak, high shelf).

use super::Unit::*;
use super::dsp::{Biquad, Shape};
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Low freq", 20.0, 1000.0, 100.0, true, Hz),
    p("Low gain", -18.0, 18.0, 0.0, false, Db),
    p("Mid freq", 100.0, 10000.0, 1000.0, true, Hz),
    p("Mid gain", -18.0, 18.0, 0.0, false, Db),
    p("Mid Q", 0.2, 10.0, 1.0, true, Plain),
    p("High freq", 1000.0, 20000.0, 8000.0, true, Hz),
    p("High gain", -18.0, 18.0, 0.0, false, Db),
];

pub struct Eq {
    bands: [Biquad; 3],
    last: Vec<f32>,
}

impl Eq {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            bands: [Biquad::default(); 3],
            last: Vec::with_capacity(PARAMS.len()),
        }
    }
}

impl Effect for Eq {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let Self { bands, last } = self;
        if last.as_slice() != p {
            bands[0].set(Shape::LowShelf, p[0], 0.707, p[1], rate);
            bands[1].set(Shape::Peak, p[2], p[4], p[3], rate);
            bands[2].set(Shape::HighShelf, p[5], 0.707, p[6], rate);
            // Reuses the buffer: no allocation in the audio thread.
            last.clear();
            last.extend_from_slice(p);
        }
        let active = [p[1] != 0.0, p[3] != 0.0, p[6] != 0.0];
        for f in buf.chunks_exact_mut(2) {
            for (ch, s) in f.iter_mut().enumerate() {
                for (b, on) in bands.iter_mut().zip(active) {
                    if on {
                        *s = b.tick(ch, *s);
                    }
                }
            }
        }
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
    fn eq_high_shelf_lifts_highs_and_mid_notch_cuts() {
        let hf = sine(12_000.0, 0.1, 24_000);
        let up = run(FxKind::Eq, |p| p[6] = 12.0, &hf);
        assert!(
            rms(&up[4800..]) / rms(&hf[4800..]) > 2.0,
            "high shelf +12 dB"
        );
        let mid = sine(1000.0, 0.3, 24_000);
        let cut = run(
            FxKind::Eq,
            |p| {
                p[3] = -18.0;
                p[4] = 4.0;
            },
            &mid,
        );
        assert!(
            rms(&cut[4800..]) / rms(&mid[4800..]) < 0.2,
            "mid -18 dB at 1 kHz"
        );
        // Flat EQ passes the signal untouched.
        assert_eq!(run(FxKind::Eq, |_| {}, &mid), mid);
    }
}
