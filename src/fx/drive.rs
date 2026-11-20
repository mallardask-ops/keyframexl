//! Drive / saturation with tone control and dry/wet mix.

use super::Unit::*;
use super::dsp::{OnePole, Oversample2};
use super::{Ctx, Effect, ParamSpec, db, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Drive", 0.0, 48.0, 12.0, false, Db),
    p("Tone", 500.0, 20000.0, 6000.0, true, Hz),
    p("Output", -24.0, 6.0, -6.0, false, Db),
    p("Mix", 0.0, 100.0, 100.0, false, Pct),
];

pub const CLIPPER_PARAMS: &[ParamSpec] = &[
    p("Threshold", -100.0, 0.0, -2.0, false, Db),
    p("Output", -100.0, 6.0, 0.0, false, Db),
];

pub struct SoftClipper;

impl Effect for SoftClipper {
    fn process(&mut self, p: &[f32], buf: &mut [f32], _: &Ctx) {
        let threshold = db(p[0]);
        let output = if p[1] <= -100.0 { 0.0 } else { db(p[1]) };
        for sample in buf {
            let magnitude = sample.abs();
            let over = (magnitude - threshold).max(0.0);
            let clipped = magnitude.min(threshold) + over / (1.0 + 0.54 * over);
            *sample = clipped.copysign(*sample) * output;
        }
    }
}

pub struct Drive {
    tone: OnePole,
    os: Oversample2,
}

impl Drive {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self {
            tone: OnePole::default(),
            os: Oversample2::default(),
        }
    }
}

impl Effect for Drive {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let tone = &mut self.tone;
        let pre = db(p[0]);
        // Twice the rate: the harmonics tanh makes stay below Nyquist and are
        // filtered by the halfband before decimation instead of aliasing.
        tone.set(p[1], rate * 2.0);
        let post = db(p[2]);
        let mix = p[3] / 100.0;
        for f in buf.as_chunks_mut::<2>().0 {
            let up_l = self.os.up(0, f[0]);
            let up_r = self.os.up(1, f[1]);
            let mut wet = [[0.0f32; 2]; 2];
            for s in 0..2 {
                let l = tone.tick(0, (up_l[s] * pre).tanh()) * post;
                let r = tone.tick(1, (up_r[s] * pre).tanh()) * post;
                wet[s] = [up_l[s] + (l - up_l[s]) * mix, up_r[s] + (r - up_r[s]) * mix];
            }
            f[0] = self.os.down(0, wet[0][0], wet[1][0]);
            f[1] = self.os.down(1, wet[0][1], wet[1][1]);
        }
    }
}

#[cfg(test)]
mod alias_tests {
    use super::super::{FxKind, FxState};
    use std::f32::consts::TAU;
    const RATE: f32 = 48_000.0;

    fn bin(y: &[f32], hz: f32) -> f32 {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, v) in y.iter().enumerate() {
            let w = TAU * hz * i as f32 / RATE;
            re += v * w.cos();
            im -= v * w.sin();
        }
        (re * re + im * im).sqrt() / y.len() as f32
    }

    #[test]
    fn oversampling_keeps_the_15k_alias_down() {
        let n = 4800;
        let input: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = (TAU * 15_000.0 * i as f32 / RATE).sin();
                [v, v]
            })
            .collect();
        let mut p = FxKind::Drive.defaults();
        p[0] = 24.0;
        p[1] = 20_000.0;
        p[2] = 0.0;
        p[3] = 100.0;
        let mut st = FxState::new(FxKind::Drive, RATE);
        let mut buf = input;
        for chunk in buf.chunks_mut(1024) {
            st.process(&p, chunk, RATE, 120.0);
        }
        let left: Vec<f32> = buf.chunks(2).map(|f| f[0]).collect();
        let fundamental = bin(&left, 15_000.0);
        let alias = bin(&left, 3_000.0);
        assert!(fundamental > 0.01, "no signal: {fundamental}");
        assert!(
            alias < fundamental * 0.05,
            "alias at 3 kHz is {} vs {fundamental}",
            alias
        );
    }
}
