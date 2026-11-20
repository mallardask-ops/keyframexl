//! Stereo Shaper: a 2×2 matrix (left/right and their crossfeeds), a delay and
//! a phase shift on the right channel (FL: Fruity Stereo Shaper, simplified).

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Left", -100.0, 100.0, 100.0, false, Pct),
    p("Right", -100.0, 100.0, 100.0, false, Pct),
    p("L → R", -100.0, 100.0, 0.0, false, Pct),
    p("R → L", -100.0, 100.0, 0.0, false, Pct),
    p("Delay", 0.0, 100.0, 0.0, false, Ms),
    p("Dephase", -180.0, 180.0, 0.0, false, Plain),
];

const MAX_DELAY_S: f32 = 0.101;

pub struct StereoShaper {
    ring: Vec<f32>,
    at: usize,
    /// One-pole allpass state for the right channel.
    z: f32,
}

impl StereoShaper {
    pub fn new(rate: f32) -> Self {
        Self {
            ring: vec![0.0; (MAX_DELAY_S * rate.max(1000.0)) as usize],
            at: 0,
            z: 0.0,
        }
    }
}

impl Effect for StereoShaper {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate.max(1000.0);
        let delay = ((p[4] / 1000.0) * rate).round() as usize;
        let delay = delay.min(self.ring.len().saturating_sub(1));
        // 0° is a plain wire; 180° is a one-sample delay, like FL's dephase.
        let a = (1.0 - (p[5].abs() / 180.0)).clamp(0.0, 1.0);
        let (l2l, r2r, l2r, r2l) = (p[0] / 100.0, p[1] / 100.0, p[2] / 100.0, p[3] / 100.0);
        let n = self.ring.len();
        for f in buf.as_chunks_mut::<2>().0 {
            let (l, r) = (f[0], f[1]);
            let delayed = if delay == 0 {
                r
            } else {
                let idx = (self.at + n - delay) % n;
                self.ring[idx]
            };
            self.ring[self.at] = r;
            self.at = (self.at + 1) % n;
            let y = a * delayed + self.z;
            self.z = delayed - a * y;
            f[0] = l * l2l + y * r2l;
            f[1] = y * r2r + l * l2r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::StereoShaper.defaults();
        set(&mut p);
        let mut st = FxState::new(FxKind::StereoShaper, RATE);
        let mut buf = input.to_vec();
        st.process(&p, &mut buf, RATE, 120.0);
        buf
    }

    #[test]
    fn the_default_matrix_passes_the_signal() {
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| [(i as f32 * 0.01).sin(), (i as f32 * 0.02).cos()])
            .collect();
        let out = run(|_| {}, &input);
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn the_crossfeed_moves_the_left_into_the_right() {
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| [(i as f32 * 0.01).sin(), 0.0])
            .collect();
        let out = run(
            |p| {
                p[0] = 0.0;
                p[1] = 100.0;
                p[2] = 100.0;
            },
            &input,
        );
        for (a, f) in input.chunks(2).zip(out.chunks(2)) {
            assert!(f[0].abs() < 1e-5, "left leaked: {}", f[0]);
            assert!((f[1] - a[0]).abs() < 1e-4, "right {} vs {}", f[1], a[0]);
        }
    }

    #[test]
    fn the_delay_shifts_the_right_channel() {
        let mut input = vec![0.0f32; 9600];
        input[1] = 1.0; // right only: the delay sits on the right channel
        let out = run(
            |p| {
                p[1] = 0.0; // no direct right
                p[3] = 100.0; // R -> L
                p[4] = 10.0; // 10 ms
            },
            &input,
        );
        let at = (0.010 * RATE) as usize;
        assert!(out[2 * at].abs() > 0.3, "no delayed left: {}", out[2 * at]);
        assert!(out[..2 * at].iter().all(|v| v.abs() < 0.01));
    }
}
