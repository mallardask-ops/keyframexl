use super::{Ctx, Effect, ParamSpec, Unit, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Amount", 0.0, 100.0, 40.0, false, Unit::Pct),
    p("Mode", 0.0, 3.0, 0.0, false, Unit::Plain),
];

struct Profile {
    lift: [f32; 3],
    knee: [f32; 3],
    attack_ms: f32,
    release_ms: f32,
}

const PROFILES: [Profile; 4] = [
    Profile {
        lift: [1.75, 1.55, 1.8],
        knee: [0.36, 0.33, 0.22],
        attack_ms: 8.0,
        release_ms: 110.0,
    },
    Profile {
        lift: [2.5, 1.55, 1.15],
        knee: [0.30, 0.38, 0.35],
        attack_ms: 20.0,
        release_ms: 155.0,
    },
    Profile {
        lift: [1.1, 1.85, 2.35],
        knee: [0.45, 0.31, 0.24],
        attack_ms: 5.0,
        release_ms: 85.0,
    },
    Profile {
        lift: [2.35, 2.45, 2.0],
        knee: [0.24, 0.22, 0.18],
        attack_ms: 2.0,
        release_ms: 65.0,
    },
];

#[derive(Default)]
pub struct Bloom {
    bass: [f32; 2],
    body: [f32; 2],
    peak: [f32; 3],
    gain: Option<[f32; 3]>,
    mix: Option<f32>,
}

impl Effect for Bloom {
    fn process(&mut self, params: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let profile = &PROFILES[(params[1].round() as usize).min(3)];
        let target = params[0] * 0.01;
        let mix = self.mix.get_or_insert(target);
        let gain = self.gain.get_or_insert(profile.lift);
        let pole = |hz: f32| -(-std::f32::consts::TAU * hz / ctx.rate).exp_m1();
        let (low, high) = (pole(180.0), pole(2800.0));
        let smooth = |ms: f32| -(-1000.0 / (ctx.rate * ms)).exp_m1();
        let (attack, release, glide) = (
            smooth(profile.attack_ms),
            smooth(profile.release_ms),
            smooth(5.0),
        );
        for frame in buf.as_chunks_mut::<2>().0 {
            let mut bands = [[0.0; 2]; 3];
            for ch in 0..2 {
                self.bass[ch] += low * (frame[ch] - self.bass[ch]);
                let upper = frame[ch] - self.bass[ch];
                self.body[ch] += high * (upper - self.body[ch]);
                bands[0][ch] = self.bass[ch];
                bands[1][ch] = self.body[ch];
                bands[2][ch] = upper - self.body[ch];
            }
            let mut wet = [0.0; 2];
            for (i, band) in bands.iter().enumerate() {
                let level = band[0].abs().max(band[1].abs());
                let env = &mut self.peak[i];
                *env += (level - *env) * if level > *env { attack } else { release };
                let desired =
                    profile.lift[i] / (1.0 + (*env / profile.knee[i]).powi(2)).sqrt().sqrt();
                gain[i] += glide * (desired - gain[i]);
                for ch in 0..2 {
                    wet[ch] += gain[i] * band[ch];
                }
            }
            *mix += glide * (target - *mix);
            if (*mix - target).abs() < 1e-6 {
                *mix = target;
            }
            for ch in 0..2 {
                frame[ch] += *mix * (wet[ch].tanh() - frame[ch]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_identity_and_distinct_stereo_linked_profiles() {
        let ctx = Ctx {
            rate: 48000.0,
            tempo: 120.0,
        };
        let input: Vec<_> = (0..12000)
            .flat_map(|i| {
                let t = i as f32 * std::f32::consts::TAU / ctx.rate;
                let x =
                    0.32 * (83.0 * t).sin() + 0.22 * (930.0 * t).sin() + 0.13 * (6100.0 * t).sin();
                [x, -x]
            })
            .collect();
        let render = |amount, mode| {
            let mut signal = input.clone();
            let mut fx = Bloom::default();
            for block in signal.chunks_mut(256) {
                fx.process(&[amount, mode], block, &ctx);
            }
            signal
        };
        assert_eq!(render(0.0, 0.0), input);
        let mut outputs = vec![input.clone()];
        for i in 0..4 {
            let wet = render(100.0, i as f32);
            assert!(
                wet.as_chunks::<2>()
                    .0
                    .iter()
                    .all(|f| f[0] == -f[1] && f[0].abs() <= 1.0)
            );
            for other in &outputs {
                let mse = wet
                    .iter()
                    .zip(other)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    / wet.len() as f32;
                assert!(mse > 0.0001);
            }
            outputs.push(wet);
        }
    }
}
