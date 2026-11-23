//! Granular / texture cloud source: spawns Hann-windowed micro-grains over
//! either procedural bursts (fire, rain, gravel, debris, wind, plasma) or an
//! embedded audio sample (granular cloud / shimmer / creature texture).

use super::{GrainSample, RATE, Rng};

const MAX_GRAINS: usize = 64;

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    age: usize,
    len: usize,
    /// Oscillator phase (procedural) or sample position in frames (sample cloud).
    pos: f64,
    /// Phase increment per output frame.
    step: f64,
    /// Noise vs tonal blend for procedural grains (0..1).
    tonal: f64,
    gain_l: f64,
    gain_r: f64,
    /// Per-grain low-pass state for warm procedural debris/crackles.
    lp: f64,
    lp_alpha: f64,
    rng: Rng,
}

impl Default for Grain {
    fn default() -> Self {
        Self {
            active: false,
            age: 0,
            len: 1,
            pos: 0.0,
            step: 0.0,
            tonal: 0.5,
            gain_l: 0.5,
            gain_r: 0.5,
            lp: 0.0,
            lp_alpha: 0.5,
            rng: Rng::new(1),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    base_hz: f32,
    density: f32,
    grain_ms: f32,
    scatter: f32,
    spread: f32,
    spray: f32,
    sample: Option<&GrainSample>,
    frames: usize,
    pitch: &super::Curve,
    morph: &super::Curve,
    transpose: f32,
    seed: u64,
) -> Vec<[f64; 2]> {
    let rate = RATE as f64;
    let mut rng = Rng::new(seed ^ 0x517C_C1B7);
    let mut pool = [Grain::default(); MAX_GRAINS];
    let mut out = vec![[0.0f64; 2]; frames];

    let base_density = density.clamp(1.0, 200.0) as f64;
    let base_grain_ms = grain_ms.clamp(4.0, 250.0) as f64;
    let scatter_v = scatter.clamp(0.0, 1.0) as f64;
    let spread_v = spread.clamp(0.0, 1.0) as f64;
    let spray_v = spray.clamp(0.0, 1.0) as f64;

    // Trigger first grain immediately at frame 0 so short layers never start silent.
    let mut next_spawn = 0.0f64;
    let mut cursor = 0usize;

    // Gain compensation keeps perceived loudness steady across different densities & grain sizes.
    let expected_overlap = (base_density * base_grain_ms * 0.001).max(1.0);
    let norm = 0.72 / expected_overlap.sqrt();

    for (i, frame) in out.iter_mut().enumerate() {
        let x = i as f32 / frames.max(1) as f32;
        let morph_v = morph.at(x).clamp(0.0, 1.0) as f64;
        // Morph 0.5 = 1.0x base density; 0.0 = 0.25x sparse; 1.0 = 4.0x dense.
        let eff_density = (base_density * 4.0f64.powf((morph_v - 0.5) * 2.0)).clamp(1.0, 240.0);

        if next_spawn <= 0.0 {
            let semis = (pitch.at(x) + transpose) as f64 + rng.signed() as f64 * scatter_v * 18.0;
            let ratio = 2f64.powf(semis / 12.0);

            let size_jitter = 1.0 + rng.signed() as f64 * (0.15 + spray_v * 0.35);
            let glen = ((base_grain_ms * size_jitter * 0.001 * rate).round() as usize)
                .clamp(32, (rate * 0.35) as usize);

            let pan = (rng.signed() as f64 * spread_v).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
            let amp = (0.65 + 0.35 * rng.unit() as f64) * norm;

            let (pos, step, tonal, lp_alpha) = if let Some(s) = sample
                && s.frames.len() > 2
            {
                let base_pos = x as f64 * (s.frames.len().saturating_sub(2)) as f64;
                let jitter = rng.signed() as f64 * spray_v * (s.frames.len() as f64 * 0.5);
                let start = (base_pos + jitter).rem_euclid((s.frames.len() - 1) as f64);
                let sr_ratio = s.rate.max(1000) as f64 / rate;
                (start, ratio * sr_ratio, 0.0, 1.0)
            } else {
                let hz = (base_hz as f64 * ratio).clamp(30.0, rate * 0.42);
                // Procedural grains blend resonant tone + filtered noise burst.
                let tonal = (0.15 + (1.0 - spray_v) * 0.55).clamp(0.1, 0.75);
                let alpha = (hz * 2.2 / rate).clamp(0.03, 0.95);
                (rng.unit() as f64, hz / rate, tonal, alpha)
            };

            let slot = (0..MAX_GRAINS)
                .find(|&k| !pool[k].active)
                .unwrap_or_else(|| {
                    cursor = (cursor + 1) % MAX_GRAINS;
                    cursor
                });
            pool[slot] = Grain {
                active: true,
                age: 0,
                len: glen,
                pos,
                step,
                tonal,
                gain_l: angle.cos() * amp,
                gain_r: angle.sin() * amp,
                lp: 0.0,
                lp_alpha,
                rng: Rng::new(rng.next_u64()),
            };

            // Schedule next grain with jitter controlled by `spray`.
            let interval = rate / eff_density;
            let jitter = 1.0 + rng.signed() as f64 * (0.2 + spray_v * 0.7);
            next_spawn = (interval * jitter.max(0.1)).max(1.0);
        }
        next_spawn -= 1.0;

        let mut acc_l = 0.0;
        let mut acc_r = 0.0;
        for g in &mut pool {
            if !g.active {
                continue;
            }
            let u = g.age as f64 / g.len as f64;
            // Raised-cosine Hann window: zero clicks at grain edges.
            let win = 0.5 * (1.0 - (std::f64::consts::TAU * u).cos());

            let [sl, sr] = if let Some(s) = sample
                && s.frames.len() > 2
            {
                let n = s.frames.len();
                let p = g.pos.rem_euclid((n - 1) as f64);
                let idx = p as usize;
                let frac = p - idx as f64;
                let a = s.frames[idx];
                let b = s.frames[idx + 1];
                g.pos += g.step;
                [
                    a[0] as f64 + (b[0] as f64 - a[0] as f64) * frac,
                    a[1] as f64 + (b[1] as f64 - a[1] as f64) * frac,
                ]
            } else {
                let tone = (g.pos * std::f64::consts::TAU).sin();
                g.pos = (g.pos + g.step).fract();
                let raw = g.rng.signed() as f64;
                g.lp += g.lp_alpha * (raw - g.lp);
                let v = tone * g.tonal + g.lp * (1.0 - g.tonal) * 1.6;
                [v, v]
            };

            acc_l += sl * win * g.gain_l;
            acc_r += sr * win * g.gain_r;

            g.age += 1;
            if g.age >= g.len {
                g.active = false;
            }
        }

        frame[0] = acc_l.tanh();
        frame[1] = acc_r.tanh();
    }

    out
}
