//! Shared DSP blocks for the built-in mixer effects.

use std::f32::consts::PI;

/// RBJ biquad (Audio EQ Cookbook), transposed direct form II, per channel.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [[f32; 2]; 2],
}

#[derive(Clone, Copy)]
pub(crate) enum Shape {
    LowShelf,
    Peak,
    HighShelf,
    Highpass,
    /// Constant 0 dB peak gain (RBJ cookbook).
    Bandpass,
    Lowpass,
}

impl Biquad {
    pub(crate) fn set(&mut self, shape: Shape, f: f32, q: f32, gain_db: f32, rate: f32) {
        let w = 2.0 * PI * (f / rate).min(0.49);
        let (s, c) = w.sin_cos();
        let a = 10f32.powf(gain_db / 40.0);
        let alpha = s / (2.0 * q.max(0.05));
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Peak => (
                1.0 + alpha * a,
                -2.0 * c,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * c,
                1.0 - alpha / a,
            ),
            Shape::LowShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * c + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - k),
                    (a + 1.0) + (a - 1.0) * c + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - k,
                )
            }
            Shape::HighShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * c + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - k),
                    (a + 1.0) - (a - 1.0) * c + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - k,
                )
            }
            Shape::Highpass => (
                (1.0 + c) / 2.0,
                -(1.0 + c),
                (1.0 + c) / 2.0,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Shape::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            Shape::Lowpass => (
                (1.0 - c) / 2.0,
                1.0 - c,
                (1.0 - c) / 2.0,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
        };
        self.b = [b0 / a0, b1 / a0, b2 / a0];
        self.a = [a1 / a0, a2 / a0];
    }

    #[inline]
    pub(crate) fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// One-pole lowpass for tone controls.
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    k: f32,
    z: [f32; 2],
}
impl OnePole {
    pub(crate) fn set(&mut self, f: f32, rate: f32) {
        self.k = 1.0 - (-2.0 * PI * f / rate).exp();
    }
    #[inline]
    pub(crate) fn tick(&mut self, ch: usize, x: f32) -> f32 {
        self.z[ch] += self.k * (x - self.z[ch]);
        self.z[ch]
    }
}

/// Freeverb comb with damping.
pub struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}
impl Comb {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n],
            i: 0,
            store: 0.0,
        }
    }
    #[inline]
    pub(crate) fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}

pub struct Allpass {
    buf: Vec<f32>,
    i: usize,
}
impl Allpass {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n],
            i: 0,
        }
    }
    #[inline]
    pub(crate) fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// 15-tap halfband FIR (Blackman window): passband to 0.22, stopband from
/// 0.4 of the 2x rate. Used to run nonlinear effects at twice the rate.
const HB: [f32; 15] = [
    0.0, 0.0, 0.005758, 0.0, -0.048721, 0.0, 0.292962, 0.500002, 0.292962, 0.0, -0.048721, 0.0,
    0.005758, 0.0, 0.0,
];

fn hb_tick(hist: &mut [f32; 15], at: &mut usize, x: f32, gain: f32) -> f32 {
    hist[*at] = x;
    let mut y = 0.0;
    let mut i = *at;
    for h in &HB {
        y += h * hist[i];
        i = if i == 0 { 14 } else { i - 1 };
    }
    *at = (*at + 1) % 15;
    y * gain
}

/// 2x oversampling for the distortion effects: feed one sample, get the two
/// half-rate samples; feed the two processed ones back, get one sample.
#[derive(Clone, Copy)]
pub(crate) struct Oversample2 {
    up: [[f32; 15]; 2],
    up_at: [usize; 2],
    down: [[f32; 15]; 2],
    down_at: [usize; 2],
}

impl Default for Oversample2 {
    fn default() -> Self {
        Self {
            up: [[0.0; 15]; 2],
            up_at: [0; 2],
            down: [[0.0; 15]; 2],
            down_at: [0; 2],
        }
    }
}

impl Oversample2 {
    pub(crate) fn up(&mut self, ch: usize, x: f32) -> [f32; 2] {
        let a = hb_tick(&mut self.up[ch], &mut self.up_at[ch], x, 2.0);
        let b = hb_tick(&mut self.up[ch], &mut self.up_at[ch], 0.0, 2.0);
        [a, b]
    }

    pub(crate) fn down(&mut self, ch: usize, a: f32, b: f32) -> f32 {
        let _ = hb_tick(&mut self.down[ch], &mut self.down_at[ch], a, 1.0);
        hb_tick(&mut self.down[ch], &mut self.down_at[ch], b, 1.0)
    }
}

#[cfg(test)]
mod oversample_tests {
    use super::*;

    #[test]
    fn a_constant_passes_at_unity() {
        let mut os = Oversample2::default();
        let (mut up0, mut up1, mut down) = (0.0, 0.0, 0.0);
        for _ in 0..64 {
            let u = os.up(0, 1.0);
            up0 = u[0];
            up1 = u[1];
            down = os.down(0, u[0], u[1]);
        }
        assert!((up0 - 1.0).abs() < 1e-3, "up even {up0}");
        assert!((up1 - 1.0).abs() < 1e-3, "up odd {up1}");
        assert!((down - 1.0).abs() < 1e-3, "down {down}");
    }

    #[test]
    fn a_sine_keeps_its_amplitude_and_phase_is_stable() {
        use std::f32::consts::TAU;
        let mut os = Oversample2::default();
        let mut peak = 0.0f32;
        for i in 0..4800 {
            let x = (TAU * 1000.0 * i as f32 / 48_000.0).sin();
            let u = os.up(0, x);
            let y = os.down(0, u[0], u[1]);
            if i > 100 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - 1.0).abs() < 0.02, "peak {peak}");
    }
}
