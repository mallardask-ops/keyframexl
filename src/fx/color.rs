use super::{
    Ctx, Effect, ParamSpec,
    Unit::*,
    db,
    dsp::{Biquad, Shape},
    p,
};

pub const CRUSH: &[ParamSpec] = &[
    p("Bits", 2., 16., 6., false, Plain),
    p("Clock", 400., 48000., 7200., true, Hz),
    p("Mix", 0., 100., 85., false, Pct),
];
pub const FOLD: &[ParamSpec] = &[
    p("Drive", 0., 24., 10., false, Db),
    p("Mix", 0., 100., 70., false, Pct),
];
pub const GATE: &[ParamSpec] = &[
    p("Threshold", -80., -6., -38., false, Db),
    p("Attack", 0.1, 30., 1.5, true, Ms),
    p("Release", 10., 800., 90., true, Ms),
];
pub const TILT: &[ParamSpec] = &[
    p("Tilt", -12., 12., 3., false, Db),
    p("Pivot", 120., 4800., 850., true, Hz),
];

pub struct Bitcrusher {
    phase: f32,
    held: [f32; 2],
}
impl Default for Bitcrusher {
    fn default() -> Self {
        Self {
            phase: 1.,
            held: [0.; 2],
        }
    }
}
impl Effect for Bitcrusher {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let step = (p[1] / ctx.rate).min(1.);
        let scale = (p[0].round() - 1.).exp2();
        for f in buf.as_chunks_mut::<2>().0 {
            if self.phase >= 1. {
                for ch in 0..2 {
                    self.held[ch] = (f[ch] * scale).round().clamp(-scale, scale - 1.) / scale;
                }
                self.phase -= 1.;
            }
            self.phase += step;
            for ch in 0..2 {
                f[ch] += p[2] * 0.01 * (self.held[ch] - f[ch]);
            }
        }
    }
}

#[derive(Default)]
pub struct Wavefolder {
    os: super::dsp::Oversample2,
}
impl Effect for Wavefolder {
    fn process(&mut self, p: &[f32], buf: &mut [f32], _: &Ctx) {
        let gain = db(p[0]);
        let mix = p[1] * 0.01;
        // The fold generates harmonics above Nyquist; run it at 2x and filter
        // before decimating.
        for f in buf.as_chunks_mut::<2>().0 {
            let up_l = self.os.up(0, f[0]);
            let up_r = self.os.up(1, f[1]);
            let mut wet = [[0.0f32; 2]; 2];
            for s in 0..2 {
                let fold = |x: f32| {
                    let y = 1. - ((x * gain + 1.).rem_euclid(4.) - 2.).abs();
                    x + mix * (y - x)
                };
                wet[s] = [fold(up_l[s]), fold(up_r[s])];
            }
            f[0] = self.os.down(0, wet[0][0], wet[1][0]);
            f[1] = self.os.down(1, wet[0][1], wet[1][1]);
        }
    }
}

#[derive(Default)]
pub struct NoiseGate {
    open: bool,
    gain: f32,
    hold: u32,
}
impl Effect for NoiseGate {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let threshold = db(p[0]);
        let attack = -(-1000. / (p[1] * ctx.rate)).exp_m1();
        let release = -(-1000. / (p[2] * ctx.rate)).exp_m1();
        for f in buf.as_chunks_mut::<2>().0 {
            let level = f[0].abs().max(f[1].abs());
            if level >= threshold {
                self.open = true;
                self.hold = (0.015 * ctx.rate) as u32;
            } else if self.hold > 0 {
                self.hold -= 1;
            } else if level < threshold * 0.7 {
                self.open = false;
            }
            if self.open {
                self.gain += attack * (1. - self.gain);
            } else {
                self.gain -= release * self.gain;
            }
            for x in f {
                *x *= self.gain;
            }
        }
    }
}

#[derive(Default)]
pub struct TiltEq {
    low: Biquad,
    high: Biquad,
}
impl Effect for TiltEq {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        self.low.set(Shape::LowShelf, p[1], 0.707, -p[0], ctx.rate);
        self.high.set(Shape::HighShelf, p[1], 0.707, p[0], ctx.rate);
        for f in buf.as_chunks_mut::<2>().0 {
            for (ch, x) in f.iter_mut().enumerate() {
                *x = self.high.tick(ch, self.low.tick(ch, *x));
            }
        }
    }
}
