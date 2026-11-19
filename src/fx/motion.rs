//! Oscillator-driven gain, panning and phase effects.
use super::{Ctx, Effect, ParamSpec, Unit::*, p};
use std::f32::consts::{PI, TAU};

const DEPTH: ParamSpec = p("Depth", 0., 100., 70., false, Pct);
const RATE: ParamSpec = p("Rate", 0.05, 16., 2., true, Hz);
pub const TREMOLO: &[ParamSpec] = &[RATE, DEPTH, p("Shape", 0., 100., 15., false, Pct)];
pub const AUTOPAN: &[ParamSpec] = &[p("Rate", 0.05, 12., 0.65, true, Hz), DEPTH];
pub const RING: &[ParamSpec] = &[
    p("Carrier", 10., 3200., 138., true, Hz),
    p("Mix", 0., 100., 68., false, Pct),
];
pub const PHASER: &[ParamSpec] = &[
    p("Rate", 0.03, 6., 0.65, true, Hz),
    DEPTH,
    p("Feedback", 0., 85., 35., false, Pct),
];

#[derive(Default)]
struct Oscillator(f32);
impl Oscillator {
    fn next(&mut self, step: f32) -> f32 {
        let y = (TAU * self.0).sin();
        self.0 = (self.0 + step).fract();
        y
    }
}
#[derive(Default)]
pub struct Tremolo(Oscillator);
impl Effect for Tremolo {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let step = p[0] / ctx.rate;
        let shape = 1. + 12. * p[2] * 0.01;
        for f in buf.chunks_exact_mut(2) {
            let osc = self.0.next(step);
            let wave = osc * shape / (1. + (shape - 1.) * osc.abs());
            let gain = 1. - p[1] * 0.005 * (1. - wave);
            f[0] *= gain;
            f[1] *= gain;
        }
    }
}
#[derive(Default)]
pub struct AutoPan(Oscillator);
impl Effect for AutoPan {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let step = p[0] / ctx.rate;
        for f in buf.chunks_exact_mut(2) {
            let pan = self.0.next(step) * p[1] * 0.01;
            f[0] *= (1. - pan).sqrt();
            f[1] *= (1. + pan).sqrt();
        }
    }
}
#[derive(Default)]
pub struct RingMod(Oscillator);
impl Effect for RingMod {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let step = p[0] / ctx.rate;
        for f in buf.chunks_exact_mut(2) {
            let gain = 1. + p[1] * 0.01 * (self.0.next(step) - 1.);
            f[0] *= gain;
            f[1] *= gain;
        }
    }
}
#[derive(Default)]
pub struct Phaser {
    osc: Oscillator,
    stages: [[f32; 4]; 2],
    feedback: [f32; 2],
}
impl Effect for Phaser {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let step = p[0] / ctx.rate;
        for f in buf.chunks_exact_mut(2) {
            let sweep = self.osc.next(step) * p[1] * 0.01;
            let hz = 760. * (2.2 * sweep).exp2();
            let g = (PI * (hz / ctx.rate).min(0.45)).tan();
            let a = (g - 1.) / (g + 1.);
            for (ch, x) in f.iter_mut().enumerate() {
                let mut y = *x + self.feedback[ch] * p[2] * 0.01;
                for z in &mut self.stages[ch] {
                    let out = a * y + *z;
                    *z = y - a * out;
                    y = out;
                }
                self.feedback[ch] = y;
                *x = 0.5 * (*x + y * (1. - p[2] * 0.01));
            }
        }
    }
}
