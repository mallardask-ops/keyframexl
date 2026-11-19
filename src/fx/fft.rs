//! A small radix-2 FFT for convolution reverb (no external crate).

use std::f32::consts::TAU;
use std::ops::{Add, AddAssign, Mul, Sub};
use std::sync::OnceLock;

/// Twiddles are built once for this many points; smaller transforms stride
/// the table, so the hot loop never calls sin/cos.
const TABLE: usize = 1 << 16;

fn twiddles() -> &'static [Complex] {
    static TW: OnceLock<Vec<Complex>> = OnceLock::new();
    TW.get_or_init(|| {
        (0..TABLE / 2)
            .map(|i| Complex::from_angle(-TAU * i as f32 / TABLE as f32))
            .collect()
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }
    fn from_angle(a: f32) -> Self {
        Self {
            re: a.cos(),
            im: a.sin(),
        }
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
}
impl AddAssign for Complex {
    fn add_assign(&mut self, o: Self) {
        self.re += o.re;
        self.im += o.im;
    }
}
impl Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
}
impl Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

/// In-place FFT of a power-of-two length; `inverse` divides by the length.
pub fn fft(buf: &mut [Complex], inverse: bool) {
    let n = buf.len();
    if n <= 1 {
        return;
    }
    debug_assert!(n.is_power_of_two(), "FFT length must be a power of two");
    debug_assert!(n <= TABLE, "FFT longer than the twiddle table");
    let tw = twiddles();
    // Bit reversal permutation.
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            buf.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let stride = TABLE / len;
        let half = len / 2;
        for start in (0..n).step_by(len) {
            for k in 0..half {
                let mut w = tw[k * stride];
                if inverse {
                    w.im = -w.im;
                }
                let a = buf[start + k];
                let b = buf[start + k + half] * w;
                buf[start + k] = a + b;
                buf[start + k + half] = a - b;
            }
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / n as f32;
        for c in buf {
            c.re *= scale;
            c.im *= scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_impulse_is_flat() {
        let mut b = vec![Complex::default(); 16];
        b[0] = Complex::new(1.0, 0.0);
        fft(&mut b, false);
        for c in &b {
            assert!((c.re - 1.0).abs() < 1e-5 && c.im.abs() < 1e-5, "{c:?}");
        }
    }

    #[test]
    fn a_sine_lands_in_two_bins() {
        let n = 64;
        let mut b = (0..n)
            .map(|i| Complex::new((TAU * 4.0 * i as f32 / n as f32).sin(), 0.0))
            .collect::<Vec<_>>();
        fft(&mut b, false);
        let mag = |k: usize| (b[k].re * b[k].re + b[k].im * b[k].im).sqrt();
        assert!(mag(4) > 20.0 && mag(n - 4) > 20.0);
        assert!(mag(8) < 1e-3);
    }

    #[test]
    fn forward_then_inverse_returns_the_signal() {
        let mut b = (0..32)
            .map(|i| Complex::new((i as f32 * 0.37).sin(), (i as f32 * 0.11).cos()))
            .collect::<Vec<_>>();
        let want = b.clone();
        fft(&mut b, false);
        fft(&mut b, true);
        for (a, b) in want.iter().zip(&b) {
            assert!((a.re - b.re).abs() < 1e-4 && (a.im - b.im).abs() < 1e-4);
        }
    }
}

#[cfg(test)]
mod conv_tests {
    use super::*;

    #[test]
    fn a_delta_convolved_with_an_ir_is_the_ir() {
        let h = [1.0f32, 0.5, 0.25, -0.1];
        let mut spec: Vec<Complex> = h
            .iter()
            .map(|v| Complex::new(*v, 0.0))
            .chain(std::iter::repeat(Complex::default()))
            .take(16)
            .collect();
        fft(&mut spec, false);
        let mut x = vec![Complex::default(); 16];
        x[0] = Complex::new(1.0, 0.0);
        fft(&mut x, false);
        for (a, b) in x.iter_mut().zip(&spec) {
            *a = *a * *b;
        }
        fft(&mut x, true);
        for (i, want) in h.iter().enumerate() {
            assert!((x[i].re - want).abs() < 1e-5, "tap {i}: {}", x[i].re);
        }
    }
}
