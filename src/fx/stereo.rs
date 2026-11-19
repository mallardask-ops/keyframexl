//! Stereo width, balance and a Haas offset (FL: Stereo Shaper / Enhancer).

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Width", 0.0, 200.0, 100.0, false, Pct),
    p("Balance", -100.0, 100.0, 0.0, false, Pct),
    p("Offset", -20.0, 20.0, 0.0, false, Ms),
];

const MAX_HAAS_S: f32 = 0.021;

pub struct Stereo {
    line: Vec<[f32; 2]>,
    i: usize,
}

impl Stereo {
    pub fn new(rate: f32) -> Self {
        Self {
            line: vec![[0.0; 2]; (MAX_HAAS_S * rate) as usize + 2],
            i: 0,
        }
    }
}

impl Effect for Stereo {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let Self { line, i } = self;
        let width = p[0] / 100.0;
        let bal = p[1] / 100.0;
        let (gl, gr) = ((1.0 - bal).min(1.0), (1.0 + bal).min(1.0));
        let offset = ((p[2].abs() * 0.001 * rate) as usize).min(line.len() - 1);
        let n = line.len();
        for f in buf.chunks_exact_mut(2) {
            let (m, s) = ((f[0] + f[1]) * 0.5, (f[0] - f[1]) * 0.5 * width);
            let (l, r) = ((m + s) * gl, (m - s) * gr);
            line[*i] = [l, r];
            let late = line[(*i + n - offset) % n];
            // Positive offset delays the right side, negative the left.
            (f[0], f[1]) = if p[2] > 0.0 {
                (l, late[1])
            } else if p[2] < 0.0 {
                (late[0], r)
            } else {
                (l, r)
            };
            *i = (*i + 1) % n;
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
        let s: Vec<f32> = v.chunks_exact(2).skip(2000).map(|f| f[ch]).collect();
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
    fn stereo_width_balance_and_offset() {
        let wide = tone(440.0, 9600, -1.0); // pure side signal
        let mono = run(FxKind::Stereo, &[0.0, 0.0, 0.0], wide.clone());
        assert!(
            rms(&mono, 0) < 1e-6 && rms(&mono, 1) < 1e-6,
            "width 0 folds side to silence"
        );
        let same = run(FxKind::Stereo, &FxKind::Stereo.defaults(), wide.clone());
        assert_eq!(same, wide, "defaults pass through");
        let centre = tone(440.0, 9600, 1.0);
        let left = run(FxKind::Stereo, &[100.0, -100.0, 0.0], centre.clone());
        assert!(
            rms(&left, 1) < 1e-6 && (rms(&left, 0) - rms(&centre, 0)).abs() < 1e-4,
            "full left balance"
        );
        let haas = run(FxKind::Stereo, &[100.0, 0.0, 10.0], centre.clone());
        let lag = (0.010 * RATE) as usize;
        let (l, r): (Vec<f32>, Vec<f32>) = haas.chunks_exact(2).map(|f| (f[0], f[1])).unzip();
        assert!(
            (r[3000] - l[3000 - lag]).abs() < 1e-6,
            "right is the left delayed by 10 ms"
        );
    }
}
