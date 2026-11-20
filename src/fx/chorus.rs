//! Chorus with quadrature LFOs and interpolated delay line.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};
use std::f32::consts::PI;

pub const PARAMS: &[ParamSpec] = &[
    p("Delay", 1.0, 30.0, 7.0, false, Ms),
    p("Depth", 0.0, 100.0, 40.0, false, Pct),
    p("Rate", 0.05, 5.0, 0.6, true, Hz),
    p("Mix", 0.0, 100.0, 50.0, false, Pct),
];

pub struct Chorus {
    buf: Vec<[f32; 2]>,
    i: usize,
    phase: f32,
}

impl Chorus {
    pub fn new(rate: f32) -> Self {
        Self {
            buf: vec![[0.0; 2]; (0.05 * rate) as usize],
            i: 0,
            phase: 0.0,
        }
    }
}

impl Effect for Chorus {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let Self {
            buf: line,
            i,
            phase,
        } = self;
        let base = p[0] * 0.001 * rate;
        let depth = p[1] / 100.0 * base * 0.9;
        let inc = p[2] / rate;
        let mix = p[3] / 100.0;
        let n = line.len();
        for f in buf.as_chunks_mut::<2>().0 {
            line[*i] = [f[0], f[1]];
            let mut wet = [0.0f32; 2];
            for (ch, w) in wet.iter_mut().enumerate() {
                // Left and right LFOs a quarter turn apart.
                let lfo = (2.0 * PI * (*phase + ch as f32 * 0.25)).sin();
                let d = (base + depth * lfo).clamp(1.0, n as f32 - 2.0);
                let pos = *i as f32 - d + n as f32;
                let k = pos.floor() as usize;
                let fr = pos - pos.floor();
                let a = line[k % n][ch];
                let b = line[(k + 1) % n][ch];
                *w = a + (b - a) * fr;
            }
            *phase = (*phase + inc).fract();
            *i = (*i + 1) % n;
            f[0] = f[0] * (1.0 - mix * 0.5) + wet[0] * mix;
            f[1] = f[1] * (1.0 - mix * 0.5) + wet[1] * mix;
        }
    }
}
