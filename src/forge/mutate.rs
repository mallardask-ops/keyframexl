//! Variations and mutations: the same sound nudged at random, so a game's
//! footsteps never repeat like a robot, and new ideas grow from one you like.

use super::{ForgePatch, NoiseColor, PITCH_RANGE, Rng, Source, Wave};

/// How far "Make variations" moves a sound: same sound, a little different.
pub const VARIATION: f32 = 0.15;
/// How far "Mutate" moves it: a relative, not a copy.
pub const MUTATION: f32 = 0.55;

/// `p` nudged by `amount` (0..1) with `seed`. Small amounts keep the sound's
/// character (lengths, sources, curve shapes); large ones also swap waves and
/// noise colours and turn the character knobs.
pub fn mutate(p: &ForgePatch, seed: u64, amount: f32) -> ForgePatch {
    let a = amount.clamp(0.0, 1.0);
    let mut rng = Rng::new(seed ^ 0xF0F0_1234);
    let mut out = p.clone();
    out.seed = rng.next_u64() | 1;
    out.transpose += rng.signed() * a * 3.0;
    for l in &mut out.layers {
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
        if a >= 0.3 {
            for pt in &mut l.amp.points {
                pt.tension = (pt.tension + rng.signed() * a * 0.4).clamp(-1.0, 1.0);
            }
            l.pan = (l.pan + rng.signed() * a * 0.3).clamp(-1.0, 1.0);
            match &mut l.source {
                Source::Osc { wave, hz, .. } => {
                    if rng.unit() < a * 0.3 {
                        *wave = Wave::ALL[(rng.next_u64() % Wave::ALL.len() as u64) as usize];
                    }
                    *hz *= 2f32.powf(rng.signed() * a * 0.25);
                }
                Source::Noise { color, hz } => {
                    if rng.unit() < a * 0.2 {
                        *color = NoiseColor::ALL
                            [(rng.next_u64() % NoiseColor::ALL.len() as u64) as usize];
                    }
                    *hz *= 2f32.powf(rng.signed() * a * 0.5);
                }
                _ => {}
            }
        }
    }
    if a >= 0.3 {
        for i in 0..4 {
            let v = out.macros.get(i) + rng.signed() * a * 0.25;
            out.macros.set(i, v);
        }
    }
    out.tidy();
    out
}

/// `n` variations of `p`, named "Name 01", "Name 02"…
pub fn variations(p: &ForgePatch, n: usize, seed: u64, amount: f32) -> Vec<ForgePatch> {
    (0..n)
        .map(|i| {
            let mut v = mutate(p, seed.wrapping_add(i as u64 * 0x9E37), amount);
            v.name = format!("{} {:02}", p.name, i + 1);
            v
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::recipes::Recipe;
    use super::super::render::render;
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
}
