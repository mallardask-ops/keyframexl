//! State-variable filter: lowpass, bandpass and highpass with resonance.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};
use std::f32::consts::PI;

pub const PARAMS: &[ParamSpec] = &[
    p("Type", 0.0, 2.0, 0.0, false, Plain),
    p("Cutoff", 20.0, 20000.0, 2000.0, true, Hz),
    p("Resonance", 0.0, 100.0, 20.0, false, Pct),
    p("Mix", 0.0, 100.0, 100.0, false, Pct),
];

pub struct Filter {
    svf: [[f32; 2]; 2],
}

impl Filter {
    pub fn new(rate: f32) -> Self {
        let _ = rate;
        Self { svf: [[0.0; 2]; 2] }
    }
}

impl Effect for Filter {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let svf = &mut self.svf;
        let g = (PI * (p[1] / rate).min(0.49)).tan();
        let k = 2.0 - 1.96 * p[2] / 100.0;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let (a2, a3) = (g * a1, g * g * a1);
        let mode = p[0].round() as usize;
        let mix = p[3] / 100.0;
        for f in buf.as_chunks_mut::<2>().0 {
            for (ch, s) in f.iter_mut().enumerate() {
                let [ic1, ic2] = &mut svf[ch];
                let v3 = *s - *ic2;
                let v1 = a1 * *ic1 + a2 * v3;
                let v2 = *ic2 + a2 * *ic1 + a3 * v3;
                *ic1 = 2.0 * v1 - *ic1;
                *ic2 = 2.0 * v2 - *ic2;
                let y = match mode {
                    0 => v2,
                    1 => v1,
                    _ => *s - k * v1 - v2,
                };
                *s = *s * (1.0 - mix) + y * mix;
            }
        }
    }
}
