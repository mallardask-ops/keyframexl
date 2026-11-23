//! Variations and mutations: the same sound nudged at random, so a game's
//! footsteps never repeat like a robot, and new ideas grow from one you like.
//! Locked layers stay untouched so key transients or sub layers remain consistent.

use super::{ForgePatch, NoiseColor, PITCH_RANGE, Rng, Source, Wave};

/// How far "Make variations" moves a sound: same sound, a little different.
pub const VARIATION: f32 = 0.15;
/// How far "Mutate" moves it: a relative, not a copy.
pub const MUTATION: f32 = 0.55;

/// `p` nudged by `amount` (0..1) with `seed`. Small amounts keep the sound's
/// character (lengths, sources, curve shapes); large ones also swap waves and
/// noise colours and turn the character knobs. Locked layers are never modified.
pub fn mutate(p: &ForgePatch, seed: u64, amount: f32) -> ForgePatch {
    let a = amount.clamp(0.0, 1.0);
    let mut rng = Rng::new(seed ^ 0xF0F0_1234);
    let mut out = p.clone();
    let any_locked = out.layers.iter().any(|l| l.locked);
    let any_unlocked = out.layers.iter().any(|l| !l.locked);

    if !any_unlocked {
        return out;
    }

    out.seed = rng.next_u64() | 1;
    // Only transpose globally when no layer is locked; otherwise keep locked layers pitch-exact.
    if !any_locked {
        out.transpose += rng.signed() * a * 3.0;
    }
    for l in &mut out.layers {
        if l.locked {
            continue;
        }
        l.gain = (l.gain * 2f32.powf(rng.signed() * a * 0.5)).clamp(0.0, 2.0);
        l.length *= 1.0 + rng.signed() * a * 0.25;
        if l.start > 0.0 {
            l.start = (l.start * (1.0 + rng.signed() * a * 0.3)).max(0.0);
        }
        let shift = rng.signed() * a * 3.0;
        for pt in &mut l.pitch.points {
            pt.v = (pt.v + shift + rng.signed() * a * 2.0).clamp(-PITCH_RANGE, PITCH_RANGE);
        }
        for pt in &mut l.cutoff.points {
            pt.v = (pt.v + rng.signed() * a * 0.12).clamp(0.0, 1.0);
        }
        match &mut l.source {
            Source::Osc {
                wave,
                hz,
                fm_index,
                detune,
                shape,
                ..
            } => {
                if a >= 0.3 {
                    if rng.unit() < a * 0.3 {
                        *wave = Wave::ALL[(rng.next_u64() % Wave::ALL.len() as u64) as usize];
                    }
                    *hz *= 2f32.powf(rng.signed() * a * 0.25);
                }
                if *fm_index > 0.0 {
                    *fm_index = (*fm_index * (1.0 + rng.signed() * a * 0.35)).clamp(0.0, 8.0);
                }
                if *detune > 0.0 {
                    *detune = (*detune + rng.signed() * a * 0.15).clamp(0.0, 1.0);
                }
                if *shape > 0.0 {
                    *shape = (*shape + rng.signed() * a * 0.2).clamp(0.0, 1.0);
                }
            }
            Source::Noise {
                color,
                hz,
                stereo_width,
                burst,
            } => {
                if a >= 0.3 {
                    if rng.unit() < a * 0.2 {
                        *color = NoiseColor::ALL
                            [(rng.next_u64() % NoiseColor::ALL.len() as u64) as usize];
                    }
                    *hz *= 2f32.powf(rng.signed() * a * 0.5);
                }
                if *stereo_width > 0.0 {
                    *stereo_width = (*stereo_width + rng.signed() * a * 0.15).clamp(0.0, 1.0);
                }
                if *burst > 0.0 {
                    *burst = (*burst + rng.signed() * a * 0.2).clamp(0.0, 1.0);
                }
            }
            Source::Modal {
                hz,
                damping,
                inharmonicity,
                strike,
                ..
            } => {
                *hz *= 2f32.powf(rng.signed() * a * 0.2);
                *damping = (*damping + rng.signed() * a * 0.2).clamp(0.0, 1.0);
                *inharmonicity = (*inharmonicity + rng.signed() * a * 0.2).clamp(0.0, 1.0);
                *strike = (*strike + rng.signed() * a * 0.25).clamp(0.0, 1.0);
            }
            Source::Grain {
                hz,
                density,
                grain_ms,
                scatter,
                spread,
                spray,
                ..
            } => {
                *hz *= 2f32.powf(rng.signed() * a * 0.25);
                *density = (*density * (1.0 + rng.signed() * a * 0.3)).clamp(1.0, 200.0);
                *grain_ms = (*grain_ms * (1.0 + rng.signed() * a * 0.25)).clamp(4.0, 250.0);
                *scatter = (*scatter + rng.signed() * a * 0.18).clamp(0.0, 1.0);
                *spread = (*spread + rng.signed() * a * 0.15).clamp(0.0, 1.0);
                *spray = (*spray + rng.signed() * a * 0.2).clamp(0.0, 1.0);
            }
            _ => {}
        }
        if a >= 0.3 {
            for pt in &mut l.amp.points {
                pt.tension = (pt.tension + rng.signed() * a * 0.4).clamp(-1.0, 1.0);
            }
            l.pan = (l.pan + rng.signed() * a * 0.3).clamp(-1.0, 1.0);
        }
    }
    if a >= 0.3 && !any_locked {
        for i in 0..4 {
            let v = out.macros.get(i) + rng.signed() * a * 0.25;
            out.macros.set(i, v);
        }
    }
    out.tidy();
    out
}

/// Deterministic variation for slot `index`, allowing single-card rerolling.
pub fn variation_at(p: &ForgePatch, index: usize, seed: u64, amount: f32) -> ForgePatch {
    let mut v = mutate(p, seed.wrapping_add(index as u64 * 0x9E37), amount);
    v.name = format!("{} {:02}", p.name, index + 1);
    v
}

/// `n` variations of `p`, named "Name 01", "Name 02"…
pub fn variations(p: &ForgePatch, n: usize, seed: u64, amount: f32) -> Vec<ForgePatch> {
    (0..n).map(|i| variation_at(p, i, seed, amount)).collect()
}

#[cfg(test)]
mod tests {
    use super::super::recipes::Recipe;
    use super::super::render::render;
    use super::super::{Layer, ModalMaterial};
    use super::*;

    #[test]
    fn variations_differ_but_stay_close() {
        let p = Recipe::Step.patch();
        let base = render(&p);
        let vs = variations(&p, 5, 42, VARIATION);
        assert_eq!(vs.len(), 5);
        assert_eq!(vs[0].name, "Step 01");
        let outs: Vec<_> = vs.iter().map(render).collect();
        for (i, o) in outs.iter().enumerate() {
            assert!(o.peak > 0.05);
            assert!((o.seconds() - base.seconds()).abs() < base.seconds() * 0.5);
            for other in &outs[..i] {
                assert_ne!(o.frames, other.frames, "two variations are identical");
            }
        }
    }

    #[test]
    fn mutation_is_repeatable_and_safe() {
        for r in Recipe::ALL {
            let p = r.patch();
            assert_eq!(mutate(&p, 7, MUTATION), mutate(&p, 7, MUTATION));
            assert_ne!(mutate(&p, 7, MUTATION), mutate(&p, 8, MUTATION));
            for seed in 0..4 {
                let m = mutate(&p, seed, 1.0);
                let mut t = m.clone();
                t.tidy();
                assert_eq!(t, m);
                let o = render(&m);
                assert!(o.peak <= 1.0 && o.frames.iter().all(|f| f[0].is_finite()));
            }
        }
    }

    #[test]
    fn locked_layers_stay_exact_while_unlocked_vary() {
        let mut p = ForgePatch::blank();
        p.layers.push(
            Layer::new("Click", Source::modal(ModalMaterial::Metal, 800.0), 0.15).with_locked(true),
        );
        p.layers
            .push(Layer::new("Tail", Source::grain(600.0, 40.0, 20.0), 0.4));

        let m = mutate(&p, 99, 0.7);
        assert_eq!(
            m.transpose, p.transpose,
            "global transpose must not move locked layers"
        );
        assert_eq!(
            m.layers[0], p.layers[0],
            "locked layer must remain bit-exact"
        );
        assert_ne!(m.layers[1], p.layers[1], "unlocked grain layer should vary");

        // All layers locked => returns identical patch.
        p.layers[1].locked = true;
        assert_eq!(mutate(&p, 99, 0.9), p);
    }

    #[test]
    fn variation_at_rerolls_one_slot_deterministically() {
        let p = Recipe::Hit.patch();
        let a = variation_at(&p, 2, 10, 0.25);
        let b = variation_at(&p, 2, 11, 0.25);
        assert_eq!(a.name, "Hit 03");
        assert_eq!(b.name, "Hit 03");
        assert_ne!(a, b);
        assert_eq!(a, variation_at(&p, 2, 10, 0.25));
    }
}
