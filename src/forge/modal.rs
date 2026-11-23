//! Physical modal resonator source: excites a bank of damped resonant modes
//! tuned to real-world materials (metal, wood, glass, stone, membrane).
//! Ideal for tactile UI clicks, footsteps, weapon impacts, clashes, and bells
//! without requiring external samples.

use super::{ModalMaterial, RATE, Rng};

const MODES: usize = 8;
/// Update mode coefficients every N frames when curves move (1.5 kHz control rate).
const CTRL_BLOCK: usize = 32;

/// Relative partial frequency ratios, mode gains, and decay-rate multipliers
/// for each physical material.
struct MaterialProfile {
    ratios: [f64; MODES],
    amps: [f64; MODES],
    /// Higher values decay faster relative to the fundamental.
    decay_mul: [f64; MODES],
    /// Base fundamental decay time in seconds at damping = 0.5.
    base_t60: f64,
    /// Stiffness scaling applied when `inharmonicity` increases.
    stiffness: f64,
}

fn profile(m: ModalMaterial) -> MaterialProfile {
    match m {
        ModalMaterial::Metal => MaterialProfile {
            ratios: [1.0, 2.756, 5.404, 8.933, 13.344, 18.64, 24.8, 31.9],
            amps: [1.0, 0.82, 0.66, 0.52, 0.40, 0.30, 0.22, 0.16],
            decay_mul: [1.0, 1.15, 1.35, 1.6, 1.9, 2.2, 2.6, 3.0],
            base_t60: 0.75,
            stiffness: 0.08,
        },
        ModalMaterial::Wood => MaterialProfile {
            ratios: [1.0, 2.58, 3.92, 5.14, 6.81, 8.45, 10.3, 12.1],
            amps: [1.0, 0.65, 0.42, 0.26, 0.18, 0.12, 0.08, 0.05],
            decay_mul: [1.0, 1.9, 2.9, 4.2, 5.8, 7.5, 9.5, 12.0],
            base_t60: 0.18,
            stiffness: 0.05,
        },
        ModalMaterial::Glass => MaterialProfile {
            ratios: [1.0, 2.32, 4.25, 6.63, 9.38, 12.4, 15.8, 19.6],
            amps: [1.0, 0.78, 0.62, 0.48, 0.36, 0.26, 0.18, 0.12],
            decay_mul: [1.0, 1.05, 1.2, 1.4, 1.65, 1.95, 2.3, 2.7],
            base_t60: 0.90,
            stiffness: 0.04,
        },
        ModalMaterial::Stone => MaterialProfile {
            ratios: [1.0, 1.71, 2.63, 3.84, 5.29, 7.12, 9.18, 11.4],
            amps: [1.0, 0.74, 0.55, 0.38, 0.25, 0.17, 0.11, 0.07],
            decay_mul: [1.0, 2.2, 3.6, 5.4, 7.2, 9.0, 11.5, 14.0],
            base_t60: 0.14,
            stiffness: 0.10,
        },
        ModalMaterial::Membrane => MaterialProfile {
            // Circular drumhead Bessel zeros relative to J_{0,1}.
            ratios: [1.0, 1.594, 2.136, 2.296, 2.653, 2.918, 3.156, 3.501],
            amps: [1.0, 0.68, 0.50, 0.40, 0.30, 0.22, 0.16, 0.11],
            decay_mul: [1.0, 1.45, 1.9, 2.4, 3.0, 3.7, 4.5, 5.4],
            base_t60: 0.32,
            stiffness: 0.06,
        },
    }
}

/// Damped 2-pole modal resonator using coupled-form rotation (unconditionally
/// stable even under fast per-sample pitch modulation).
#[derive(Clone, Copy, Default)]
struct Mode {
    re: f64,
    im: f64,
}

#[derive(Clone, Copy, Default)]
struct Coeff {
    active: bool,
    c: f64,
    s: f64,
    out_l: f64,
    out_r: f64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    material: ModalMaterial,
    base_hz: f32,
    damping: f32,
    inharmonicity: f32,
    strike: f32,
    frames: usize,
    pitch: &super::Curve,
    morph: &super::Curve,
    transpose: f32,
    seed: u64,
) -> Vec<[f64; 2]> {
    let rate = RATE as f64;
    let prof = profile(material);
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A);
    let mut modes = [Mode::default(); MODES];
    let mut out = vec![[0.0f64; 2]; frames];

    let damp = damping.clamp(0.0, 1.0) as f64;
    let inharm = inharmonicity.clamp(0.0, 1.0) as f64;
    let strike_v = strike.clamp(0.0, 1.0) as f64;

    // Precompute mode frequency multipliers and subtle stereo spread.
    let mut mode_ratios = [1.0f64; MODES];
    let mut pan_l = [0.707f64; MODES];
    let mut pan_r = [0.707f64; MODES];
    for k in 0..MODES {
        let base_r = prof.ratios[k];
        let warped =
            base_r.powf(0.85 + inharm * 0.35) * (1.0 + inharm * prof.stiffness * (k * k) as f64);
        let jitter = 1.0 + rng.signed() as f64 * 0.004 * (0.2 + inharm);
        mode_ratios[k] = (warped * jitter).max(0.5);

        let pan = if k == 0 {
            0.0
        } else {
            let side = if k % 2 == 1 { -1.0 } else { 1.0 };
            side * (0.12 + 0.04 * k as f64).min(0.42)
        };
        let angle = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
        pan_l[k] = angle.cos();
        pan_r[k] = angle.sin();
    }

    let exciter_ms = 1.5 + (1.0 - strike_v) * 20.0;
    let exciter_frames = ((exciter_ms * 0.001 * rate).round() as usize).max(4);
    let mut exc_lp = 0.0f64;
    let exc_alpha = (0.04 + strike_v * 0.92).clamp(0.02, 0.98);

    let base_t60 = prof.base_t60 * 0.08f64.powf(damp - 0.5).clamp(0.06, 4.0);
    let max_hz = rate * 0.44;
    let static_curves = pitch.is_flat_at(pitch.at(0.0)) && morph.is_flat_at(morph.at(0.0));

    let compute_coeffs = |x: f32| -> ([Coeff; MODES], f64) {
        let semis = (pitch.at(x) + transpose) as f64;
        let ratio = 2f64.powf(semis / 12.0);
        let morph_v = morph.at(x).clamp(0.0, 1.0) as f64;
        let morph_delta = (morph_v - 0.5) * 2.0;
        let brightness = (1.0 + morph_delta * 0.65).clamp(0.3, 1.8);
        let t60_scale = 2.0f64.powf(morph_delta * 0.8);
        let mut coeffs = [Coeff::default(); MODES];
        for k in 0..MODES {
            let hz = (base_hz as f64 * ratio * mode_ratios[k]).min(max_hz);
            if hz < 20.0 || hz >= max_hz * 0.995 {
                continue;
            }
            let w = std::f64::consts::TAU * hz / rate;
            let t60 = (base_t60 * t60_scale / prof.decay_mul[k]).max(0.004);
            let radius = (-6.907755278982137 / (t60 * rate)).exp().min(0.99995);
            let mode_tilt =
                (1.0 / (1.0 + k as f64 * (0.55 - 0.42 * strike_v))).powf(1.0 / brightness);
            coeffs[k] = Coeff {
                active: true,
                c: w.cos() * radius,
                s: w.sin() * radius,
                out_l: mode_tilt * pan_l[k],
                out_r: mode_tilt * pan_r[k],
            };
        }
        (coeffs, morph_delta)
    };

    let (mut coeffs, mut morph_delta) = compute_coeffs(0.0);
    let exc_direct = 0.28 + 0.22 * strike_v;

    for (i, frame) in out.iter_mut().enumerate() {
        if !static_curves && i > 0 && i % CTRL_BLOCK == 0 {
            let x = i as f32 / frames.max(1) as f32;
            (coeffs, morph_delta) = compute_coeffs(x);
        }

        let mut exc = 0.0;
        if i < exciter_frames {
            let env = (std::f64::consts::PI * i as f64 / exciter_frames as f64).sin();
            let raw = rng.signed() as f64;
            exc_lp += exc_alpha * (raw - exc_lp);
            let impulse = if i == 0 { 0.85 * strike_v } else { 0.0 };
            exc = (impulse + exc_lp * env * (0.45 + 0.55 * strike_v)) * 0.42;
        } else if morph_delta > 0.15 {
            let raw = rng.signed() as f64;
            exc_lp += exc_alpha * (raw - exc_lp);
            exc = exc_lp * (morph_delta - 0.15) * 0.025;
        }

        let mut l = exc * exc_direct;
        let mut r = l;

        for k in 0..MODES {
            let cf = coeffs[k];
            if !cf.active {
                continue;
            }
            let m = &mut modes[k];
            let next_re = m.re * cf.c - m.im * cf.s + exc * prof.amps[k];
            let next_im = m.re * cf.s + m.im * cf.c;
            m.re = next_re;
            m.im = next_im;
            l += next_im * cf.out_l;
            r += next_im * cf.out_r;
        }

        frame[0] = (l * 0.45).tanh();
        frame[1] = (r * 0.45).tanh();
    }

    out
}
