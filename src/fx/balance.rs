//! Balance: volume and a left/right balance, FL's Fruity Balance.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Volume", 0.0, 200.0, 100.0, false, Pct),
    p("Pan", -100.0, 100.0, 0.0, false, Pct),
];

pub struct Balance;

impl Effect for Balance {
    fn process(&mut self, p: &[f32], buf: &mut [f32], _: &Ctx) {
        let gain = p[0] / 100.0;
        let pan = (p[1] / 100.0).clamp(-1.0, 1.0);
        // A balance control: the centre keeps both sides, the far ends mute one.
        let l = gain * if pan < 0.0 { 1.0 } else { 1.0 - pan };
        let r = gain * if pan > 0.0 { 1.0 } else { 1.0 + pan };
        for f in buf.as_chunks_mut::<2>().0 {
            f[0] *= l;
            f[1] *= r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(volume: f32, pan: f32) -> Vec<f32> {
        let mut p = FxKind::Balance.defaults();
        p[0] = volume;
        p[1] = pan;
        let mut st = FxState::new(FxKind::Balance, RATE);
        let mut buf = vec![1.0f32; 8];
        st.process(&p, &mut buf, RATE, 120.0);
        buf
    }

    #[test]
    fn centre_is_unity_and_the_ends_mute_a_side() {
        assert_eq!(run(100.0, 0.0), vec![1.0; 8]);
        let left = run(100.0, -100.0);
        assert!(
            left.chunks(2)
                .all(|f| (f[0] - 1.0).abs() < 1e-6 && f[1] == 0.0)
        );
        let right = run(100.0, 100.0);
        assert!(
            right
                .chunks(2)
                .all(|f| f[0] == 0.0 && (f[1] - 1.0).abs() < 1e-6)
        );
    }

    #[test]
    fn volume_scales_both_channels() {
        let out = run(50.0, 0.0);
        assert!(out.iter().all(|v| (v - 0.5).abs() < 1e-6));
    }
}
