//! One layer's dry signal: the source through its pitch, filter, morph, pan
//! and volume curves, before the layer's effects.

use super::{FilterMode, Layer, NoiseColor, RATE, Rng, Source, Wave, granular, modal};

/// What the patch's knobs change in every layer.
#[derive(Clone, Copy, Debug)]
pub(super) struct LayerCtx {
    /// Semitones added to the pitch curve.
    pub transpose: f32,
    /// How far pitch and filter sweeps reach (1 = as drawn).
    pub sweep: f32,
    /// Stretch of every layer's length and start (1 = as drawn).
    pub time: f32,
    pub seed: u64,
}

/// Interleaved stereo at `RATE`, `layer.length * ctx.time` seconds long.
pub(super) fn dry(layer: &Layer, ctx: &LayerCtx) -> Vec<f32> {
    let frames = ((layer.length * ctx.time).max(0.001) * RATE as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    let rate = RATE as f64;
    let mut rng = Rng::new(ctx.seed);
    let mut rng_r = Rng::new(ctx.seed ^ 0xD1B5_4A32_D192_ED03);
    let mut burst_rng = Rng::new(ctx.seed ^ 0x94D0_49BB_1331_11EB);
    let pitch = layer.pitch.scaled(0.0, ctx.sweep);
    // A filter sweep deepens around its starting point.
    let pivot = layer.cutoff.points.first().map_or(1.0, |p| p.v);
    let cutoff = layer.cutoff.scaled(pivot, ctx.sweep);
    let has_pan_curve = !layer.pan_curve.is_flat_at(0.0);
    let morph_flat = layer.morph.is_flat_at(0.5);

    let synth = match &layer.source {
        Source::Synth { patch, key } => synth_note(patch, *key, frames),
        _ => Vec::new(),
    };
    let pre_rendered = match &layer.source {
        Source::Modal {
            material,
            hz,
            damping,
            inharmonicity,
            strike,
        } => modal::render(
            *material,
            *hz,
            *damping,
            *inharmonicity,
            *strike,
            frames,
            &pitch,
            &layer.morph,
            ctx.transpose,
            ctx.seed,
        ),
        Source::Grain {
            hz,
            density,
            grain_ms,
            scatter,
            spread,
            spray,
            sample,
        } => granular::render(
            *hz,
            *density,
            *grain_ms,
            *scatter,
            *spread,
            *spray,
            sample.as_ref(),
            frames,
            &pitch,
            &layer.morph,
            ctx.transpose,
            ctx.seed,
        ),
        _ => Vec::new(),
    };

    let mut st = State::default();
    let mut st_r = State::default();
    let mut svf = [Svf::default(); 2];
    // Deterministic staggered initial phases for unison voices 1..=3.
    let mut uni_ph = [0.0f64, 0.17, 0.43, 0.71];
    let mut uni_mod = [0.0f64; 4];
    let mut burst_env = 1.0f64;
    // The last 3 ms fade out, so a layer cut off mid-sound does not click.
    let declick = (0.003 * rate) as usize;
    for i in 0..frames {
        let x = i as f32 / frames as f32;
        let t = i as f64 / rate;
        let semis = (pitch.at(x) + ctx.transpose) as f64;
        let ratio = 2f64.powf(semis / 12.0);
        let morph_v = if morph_flat {
            0.5
        } else {
            layer.morph.at(x).clamp(0.0, 1.0) as f64
        };
        let s = match &layer.source {
            Source::Osc {
                wave,
                hz,
                vibrato_hz,
                vibrato,
                fm_ratio,
                fm_index,
                unison,
                detune,
                shape,
            } => {
                let vib = if *vibrato > 0.0 {
                    let v =
                        (t * *vibrato_hz as f64 * std::f64::consts::TAU).sin() * *vibrato as f64;
                    2f64.powf(v / 12.0)
                } else {
                    1.0
                };
                let ucount = (*unison).clamp(1, 4) as usize;
                let base_fm = (*fm_index).max(0.0) as f64;
                let base_shape = (*shape).clamp(0.0, 1.0) as f64;
                let morph_delta = (morph_v - 0.5) * 2.0;

                if ucount == 1 && base_fm == 0.0 && base_shape == 0.0 && morph_flat {
                    // Exact fast path for classic patches.
                    let dt = (*hz as f64 * ratio * vib / rate).min(0.49);
                    let v = osc(*wave, st.phase, dt);
                    st.phase = (st.phase + dt).fract();
                    [v, v]
                } else {
                    // Morph modulates FM depth (if active), shape (if active), or adds subtle fold.
                    let eff_fm = if base_fm > 0.0 {
                        (base_fm * 2.0f64.powf(morph_delta * 1.5)).clamp(0.0, 12.0)
                    } else if base_shape == 0.0 && morph_delta > 0.0 {
                        morph_delta * 1.5
                    } else {
                        0.0
                    };
                    let eff_shape = if base_shape > 0.0 || base_fm == 0.0 {
                        (base_shape + morph_delta * 0.45).clamp(0.0, 1.0)
                    } else {
                        base_shape
                    };
                    let fmr = (*fm_ratio).clamp(0.125, 16.0) as f64;
                    let det = (*detune).clamp(0.0, 1.0) as f64;
                    let gain = 1.0 / (ucount as f64).sqrt();
                    let mut acc = [0.0f64; 2];
                    for v_idx in 0..ucount {
                        let spread = if ucount == 1 {
                            0.0
                        } else {
                            (v_idx as f64 / (ucount - 1) as f64) * 2.0 - 1.0
                        };
                        let voice_det = 2.0f64.powf(spread * det * 0.45 / 12.0);
                        let dt = (*hz as f64 * ratio * vib * voice_det / rate).min(0.49);
                        let fm_offset = if eff_fm > 0.0 {
                            let mdt = (dt * fmr).min(0.49);
                            let m = (uni_mod[v_idx] * std::f64::consts::TAU).sin() * eff_fm * 0.35;
                            uni_mod[v_idx] = (uni_mod[v_idx] + mdt).fract();
                            m
                        } else {
                            0.0
                        };
                        let ph = (uni_ph[v_idx] + fm_offset).rem_euclid(1.0);
                        let sample = osc_shaped(*wave, ph, dt, eff_shape);
                        uni_ph[v_idx] = (uni_ph[v_idx] + dt).fract();
                        let pan = spread * (0.75 * det.max(0.25));
                        let gl = (1.0 - pan.max(0.0)) * gain;
                        let gr = (1.0 + pan.min(0.0)) * gain;
                        acc[0] += sample * gl;
                        acc[1] += sample * gr;
                    }
                    acc
                }
            }
            Source::Noise {
                color,
                hz,
                stereo_width,
                burst,
            } => {
                let vl = noise(*color, *hz as f64 * ratio, &mut st, &mut rng);
                let width = (*stereo_width).clamp(0.0, 1.0) as f64;
                let mut pair = if width <= 1e-4 {
                    [vl, vl]
                } else {
                    let vr = noise(*color, *hz as f64 * ratio, &mut st_r, &mut rng_r);
                    [vl, vl * (1.0 - width) + vr * width]
                };
                let eff_burst = (*burst as f64 + (morph_v - 0.5) * 0.7).clamp(0.0, 1.0);
                if eff_burst > 1e-3 {
                    // Stochastic spark / debris burst gate.
                    let trigger_p = (4.0 + (1.0 - eff_burst) * 35.0) / rate;
                    if (burst_rng.unit() as f64) < trigger_p {
                        burst_env = 0.55 + 0.45 * burst_rng.unit() as f64;
                    }
                    let decay = 0.9965 - eff_burst * 0.008;
                    burst_env = (burst_env * decay).max(1.0 - eff_burst * 0.96);
                    pair[0] *= burst_env;
                    pair[1] *= burst_env;
                }
                pair
            }
            Source::Sample {
                rate: sr,
                frames: data,
                reverse,
                trim_start,
                trim_end,
                ..
            } => {
                let n = data.len();
                let lo = ((*trim_start).clamp(0.0, 0.99) * n as f32).floor() as usize;
                let hi = ((*trim_end).clamp(0.01, 1.0) * n as f32)
                    .ceil()
                    .clamp((lo + 1) as f32, n as f32) as usize;
                let slice = if lo < hi && hi <= n {
                    &data[lo..hi]
                } else {
                    data.as_slice()
                };
                let v = play(slice, st.phase, *reverse);
                st.phase += ratio * *sr as f64 / rate;
                v
            }
            Source::Synth { .. } => {
                let v = play(&synth, st.phase, false);
                st.phase += ratio;
                v
            }
            Source::Modal { .. } | Source::Grain { .. } => {
                pre_rendered.get(i).copied().unwrap_or([0.0; 2])
            }
        };
        let mut s = s;
        if layer.filter != FilterMode::Off {
            let hz = (20.0 * 1000f64.powf(cutoff.at(x).clamp(0.0, 1.0) as f64)).min(rate * 0.45);
            let g = (std::f64::consts::PI * hz / rate).tan();
            let k = 2.0 - 1.96 * layer.resonance as f64;
            for c in 0..2 {
                s[c] = svf[c].run(s[c], g, k, layer.filter);
            }
        }
        let mut amp = layer.amp.at(x) as f64;
        let left = frames - i;
        if left < declick {
            amp *= left as f64 / declick as f64;
        }
        let (pl, pr) = if has_pan_curve {
            let p = layer.pan_curve.at(x).clamp(-1.0, 1.0) as f64;
            (1.0 - p.max(0.0), 1.0 + p.min(0.0))
        } else {
            (1.0, 1.0)
        };
        out[i * 2] = (s[0] * amp * pl) as f32;
        out[i * 2 + 1] = (s[1] * amp * pr) as f32;
    }
    out
}

#[derive(Default)]
struct State {
    /// Oscillator phase in cycles, or sample position in frames.
    phase: f64,
    /// Pink filter, brown integrator, chip shift register and clock,
    /// crackle decay.
    pink: [f64; 7],
    brown: f64,
    lfsr: u16,
    clock: f64,
    held: f64,
    pop: f64,
}

/// Band-limited (polyBLEP) oscillators at roughly equal loudness.
fn osc(wave: Wave, ph: f64, dt: f64) -> f64 {
    let pulse = |w: f64| {
        let mut v = if ph < w { 1.0 } else { -1.0 };
        v += blep(ph, dt);
        v -= blep((ph - w).rem_euclid(1.0), dt);
        v
    };
    match wave {
        Wave::Square => pulse(0.5) * 0.42,
        Wave::Pulse25 => pulse(0.25) * 0.45,
        Wave::Pulse12 => pulse(0.125) * 0.5,
        Wave::Saw => (2.0 * ph - 1.0 - blep(ph, dt)) * 0.5,
        Wave::Triangle => (4.0 * (ph - 0.5).abs() - 1.0) * 0.7,
        Wave::Sine => (ph * std::f64::consts::TAU).sin() * 0.7,
    }
}

/// Shaped oscillator: PWM for pulse waves, smooth wavefold for Sine/Tri/Saw.
fn osc_shaped(wave: Wave, ph: f64, dt: f64, shape: f64) -> f64 {
    if shape <= 1e-4 {
        return osc(wave, ph, dt);
    }
    let pulse = |w: f64| {
        let w = w.clamp(0.04, 0.96);
        let mut v = if ph < w { 1.0 } else { -1.0 };
        v += blep(ph, dt);
        v -= blep((ph - w).rem_euclid(1.0), dt);
        v
    };
    match wave {
        Wave::Square => pulse(0.5 - shape * 0.42) * 0.44,
        Wave::Pulse25 => pulse(0.25 + shape * 0.55) * 0.46,
        Wave::Pulse12 => pulse(0.125 + shape * 0.72) * 0.48,
        Wave::Sine | Wave::Triangle | Wave::Saw => {
            let base = osc(wave, ph, dt);
            let drive = 1.0 + shape * 3.5;
            (base * drive).sin() * 0.68
        }
    }
}

fn blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

fn noise(color: NoiseColor, hz: f64, st: &mut State, rng: &mut Rng) -> f64 {
    let w = rng.signed() as f64;
    match color {
        NoiseColor::White => w * 0.5,
        NoiseColor::Pink => {
            // Paul Kellet's refined pink filter.
            let b = &mut st.pink;
            b[0] = 0.99886 * b[0] + w * 0.0555179;
            b[1] = 0.99332 * b[1] + w * 0.0750759;
            b[2] = 0.96900 * b[2] + w * 0.1538520;
            b[3] = 0.86650 * b[3] + w * 0.3104856;
            b[4] = 0.55000 * b[4] + w * 0.5329522;
            b[5] = -0.7616 * b[5] - w * 0.0168980;
            let v = b.iter().sum::<f64>() + w * 0.5362;
            b[6] = w * 0.115926;
            v * 0.16
        }
        NoiseColor::Brown => {
            st.brown = (st.brown + 0.02 * w) / 1.02;
            st.brown * 3.0
        }
        NoiseColor::Crackle => {
            // `hz` pops per second on average, each a short decaying click.
            if (rng.unit() as f64) < hz / RATE as f64 {
                st.pop = rng.signed() as f64;
            }
            let v = st.pop;
            st.pop *= 0.93;
            v * 0.8
        }
        NoiseColor::Digital => {
            if st.lfsr == 0 {
                st.lfsr = 1;
            }
            st.clock += hz / RATE as f64;
            while st.clock >= 1.0 {
                st.clock -= 1.0;
                let bit = (st.lfsr ^ (st.lfsr >> 1)) & 1;
                st.lfsr = (st.lfsr >> 1) | (bit << 14);
                st.held = if st.lfsr & 1 == 1 { 1.0 } else { -1.0 };
            }
            st.held * 0.35
        }
    }
}

/// Linear interpolation at fractional frame `pos`; silence past the end.
fn play(data: &[[f32; 2]], pos: f64, reverse: bool) -> [f64; 2] {
    let n = data.len();
    if n == 0 || pos < 0.0 || pos >= (n - 1) as f64 {
        return [0.0; 2];
    }
    let i = pos as usize;
    let f = pos - i as f64;
    let (a, b) = if reverse {
        (data[n - 1 - i], data[n - 2 - i])
    } else {
        (data[i], data[i + 1])
    };
    [
        a[0] as f64 + (b[0] as f64 - a[0] as f64) * f,
        a[1] as f64 + (b[1] as f64 - a[1] as f64) * f,
    ]
}

/// The synth patch playing `key` for `frames` (released for the last fifth),
/// through the same engine a Tide channel uses.
fn synth_note(patch: &crate::synth::SynthPatch, key: u8, frames: usize) -> Vec<[f32; 2]> {
    use crate::sequence::{InstrumentKind, Song};
    let mut song = Song::default();
    let id = song.add_channel(
        "Forge",
        InstrumentKind::Synth {
            patch: patch.clone(),
        },
    );
    let mut engine = crate::engine::Engine::new(RATE);
    engine.live_on(&song, 1, id, key, 100);
    let release_at = frames * 4 / 5;
    let mut out = Vec::with_capacity(frames + 512);
    let mut block = vec![0.0f32; 1024];
    let mut released = false;
    while out.len() < frames {
        if !released && out.len() >= release_at {
            engine.live_off(1);
            released = true;
        }
        block.fill(0.0);
        engine.process_live(&song, &mut block);
        out.extend(block.as_chunks::<2>().0.iter().map(|f| [f[0], f[1]]));
    }
    out.truncate(frames);
    out
}

/// Topology-preserving state-variable filter (Cytomic).
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f64,
    ic2: f64,
}

impl Svf {
    fn run(&mut self, x: f64, g: f64, k: f64, mode: FilterMode) -> f64 {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match mode {
            FilterMode::Off => x,
            FilterMode::LowPass => v2,
            FilterMode::BandPass => v1,
            FilterMode::HighPass => x - k * v1 - v2,
        }
    }
}
