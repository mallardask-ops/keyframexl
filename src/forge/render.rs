//! Offline render of a whole Forge patch: layers, their effects, the mix,
//! master effects, the character knobs, seamless loop splicing, loudness
//! metering, and a safety limiter.

use super::layer::{self, LayerCtx};
use super::{ForgePatch, MAX_SECONDS, MAX_TAIL, RATE};
use crate::fx::{FxKind, FxSlot, FxState};

/// A finished sound at `rate`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
    /// Highest absolute sample.
    pub peak: f32,
    /// Approximate integrated loudness in LUFS (K-weighted; −120 for silence).
    pub lufs: f32,
    /// True when rendered in seamless-loop mode.
    pub seamless_loop: bool,
}

impl Rendered {
    pub fn seconds(&self) -> f64 {
        self.frames.len() as f64 / self.rate.max(1) as f64
    }
}

/// Silence below this (−80 dB) ends the sound.
const SILENT: f32 = 1e-4;
/// Effects run in blocks of this many frames, like the mixer.
const BLOCK: usize = 512;
const TEMPO: f32 = 120.0;

pub fn render(p: &ForgePatch) -> Rendered {
    let (mix, end) = mix_pre_gain(p);
    finish_mix(mix, end, p)
}

/// Level `p.gain_db` to `peak` (rounded to 0.1 dB) and return the final rendered
/// audio, running the expensive layer/FX synthesis only once.
pub(super) fn level_and_render(p: &mut ForgePatch, peak: f32) -> Rendered {
    let (pre_gain, end) = mix_pre_gain(p);
    let mut rendered = finish_mix(pre_gain.clone(), end, p);
    for _ in 0..4 {
        if rendered.peak <= 1e-4 || (rendered.peak - peak).abs() < 0.02 * peak {
            return rendered;
        }
        let db = p.gain_db + 20.0 * (peak / rendered.peak).log10();
        let next = ((db * 10.0).round() / 10.0).clamp(-36.0, 12.0);
        if (next - p.gain_db).abs() < 0.05 {
            return rendered;
        }
        p.gain_db = next;
        rendered = finish_mix(pre_gain.clone(), end, p);
    }
    rendered
}

/// Synthesize every layer, its layer FX, master FX, and character chain —
/// everything upstream of `p.gain_db`.
fn mix_pre_gain(p: &ForgePatch) -> (Vec<f32>, f32) {
    crate::denormal::flush_to_zero();
    let m = p.macros;
    let size = m.size - 0.5;
    let base = LayerCtx {
        // Bigger is lower (up to an octave) and longer (up to 1.7×).
        transpose: p.transpose - size * 24.0,
        // Energy 0 halves a sweep, 1 makes it 1.6× as deep.
        sweep: 0.4 + m.energy * 1.2,
        time: 2f32.powf(size * 1.6),
        seed: p.seed,
    };
    let rate = RATE as usize;
    let end = p
        .audible()
        .map(|(_, l)| (l.start + l.length) * base.time)
        .fold(0.0f32, f32::max)
        .min(MAX_SECONDS * 1.8);
    let ringing = |fx: &[FxSlot]| fx.iter().any(|s| s.enabled);
    let any_fx = ringing(&p.fx) || p.audible().any(|(_, l)| ringing(&l.fx));
    let tail = if any_fx || m.space > 0.01 {
        MAX_TAIL
    } else {
        0.0
    };
    let total = ((end + tail) * rate as f32) as usize + 1;
    let mut mix = vec![0.0f32; total * 2];
    for (i, l) in p.audible() {
        let ctx = LayerCtx {
            seed: p.seed.wrapping_mul(31).wrapping_add(i as u64 * 7919 + 1),
            ..base
        };
        let mut buf = layer::dry(l, &ctx);
        if ringing(&l.fx) {
            buf.resize(buf.len() + (MAX_TAIL * rate as f32) as usize * 2, 0.0);
            run_chain(&l.fx, &mut buf);
        }
        let at = (l.start * base.time * rate as f32) as usize;
        let (gl, gr) = (
            l.gain * (1.0 - l.pan.max(0.0)),
            l.gain * (1.0 + l.pan.min(0.0)),
        );
        for (f, s) in buf.as_chunks::<2>().0.iter().enumerate() {
            let Some(o) = mix.get_mut((at + f) * 2..(at + f) * 2 + 2) else {
                break;
            };
            o[0] += s[0] * gl;
            o[1] += s[1] * gr;
        }
    }
    run_chain(&p.fx, &mut mix);
    run_chain(&character(m), &mut mix);
    (mix, end)
}

/// Apply output gain, seamless-loop splice or limiter + tail trim, and meter peak/LUFS.
fn finish_mix(mut mix: Vec<f32>, end: f32, p: &ForgePatch) -> Rendered {
    let rate = RATE as usize;
    let gain = crate::fx::db(p.gain_db + (p.macros.energy - 0.5) * 6.0);
    for s in &mut mix {
        *s *= gain;
    }

    let frames = if p.seamless_loop {
        render_seamless_loop(&mix, end, rate)
    } else {
        let mut limit = FxSlot::new(FxKind::Limiter);
        limit.params = vec![0.0, -1.0, 60.0];
        run_chain(&[limit], &mut mix);
        let mut frames = sanitize_stereo(&mix);
        // Drop the silent tail, keep 10 ms after the last sound, fade 5 ms.
        let last = frames
            .iter()
            .rposition(|f| f[0].abs().max(f[1].abs()) > SILENT)
            .map_or(0, |i| i + 1);
        frames.truncate((last + rate / 100).min(frames.len()));
        let fade = (rate / 200).min(frames.len());
        let n = frames.len();
        for (k, f) in frames[n - fade..].iter_mut().enumerate() {
            let g = 1.0 - (k + 1) as f32 / fade as f32;
            f[0] *= g;
            f[1] *= g;
        }
        frames
    };

    let peak = frames
        .iter()
        .map(|f| f[0].abs().max(f[1].abs()))
        .fold(0.0, f32::max);
    let lufs = integrated_lufs(&frames, RATE);
    Rendered {
        rate: RATE,
        frames,
        peak,
        lufs,
        seamless_loop: p.seamless_loop,
    }
}

fn sanitize_stereo(mix: &[f32]) -> Vec<[f32; 2]> {
    mix.as_chunks::<2>()
        .0
        .iter()
        .map(|c| {
            let ok = |v: f32| {
                if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            };
            [ok(c[0]), ok(c[1])]
        })
        .collect()
}

/// Fold FX tails back into the loop window, apply an equal-power overlap-add
/// loop splice so frame `N-1` meets frame `0` with zero discontinuity, then
/// run a warmed-up limiter so gain reduction is also continuous across the seam.
fn render_seamless_loop(mix: &[f32], end: f32, rate: usize) -> Vec<[f32; 2]> {
    let total_frames = mix.len() / 2;
    let loop_frames = ((end * rate as f32).round() as usize)
        .max(rate / 20)
        .min(total_frames);
    if loop_frames < 8 {
        return Vec::new();
    }

    // 1. Fold all tail spillover (reverbs, delays, long grains) modulo loop_frames.
    let mut folded = vec![0.0f32; loop_frames * 2];
    for (i, s) in mix.as_chunks::<2>().0.iter().enumerate() {
        let dst = (i % loop_frames) * 2;
        folded[dst] += s[0];
        folded[dst + 1] += s[1];
    }

    // 2. Equal-power splice over `xfade` frames:
    //    Y[0..X] = folded[0..X]*sin + folded[N..N+X]*cos
    //    Y[X..N] = folded[X..N], where N = loop_frames - X.
    //    Thus Y[N-1] = folded[N-1] connects directly to Y[0] = folded[N]!
    let xfade = (loop_frames / 8).min(rate / 20).max(16);
    let out_frames = loop_frames.saturating_sub(xfade).max(16);
    let mut spliced = vec![0.0f32; out_frames * 2];
    for i in 0..out_frames {
        if i < xfade && out_frames + i < loop_frames {
            let theta = (i as f32 / xfade as f32) * std::f32::consts::FRAC_PI_2;
            let fin = theta.sin();
            let fout = theta.cos();
            let a = i * 2;
            let b = (out_frames + i) * 2;
            spliced[a] = folded[a] * fin + folded[b] * fout;
            spliced[a + 1] = folded[a + 1] * fin + folded[b + 1] * fout;
        } else {
            spliced[i * 2] = folded[i * 2];
            spliced[i * 2 + 1] = folded[i * 2 + 1];
        }
    }

    // 3. Limiter with seamless warmup prefix from the end of the loop.
    let warmup = out_frames.min(rate / 10);
    let mut limited = Vec::with_capacity((warmup + out_frames) * 2);
    limited.extend_from_slice(&spliced[(out_frames - warmup) * 2..]);
    limited.extend_from_slice(&spliced);
    let mut limit = FxSlot::new(FxKind::Limiter);
    limit.params = vec![0.0, -1.0, 60.0];
    run_chain(&[limit], &mut limited);

    sanitize_stereo(&limited[warmup * 2..])
}

/// Approximate integrated loudness in LUFS using ITU-R BS.1770 K-weighting
/// (high-shelf + high-pass filter) over the rendered audio.
pub fn integrated_lufs(frames: &[[f32; 2]], rate: u32) -> f32 {
    if frames.is_empty() {
        return -120.0;
    }
    // Pre-filter (high-shelf +4 dB around 1681.97 Hz) and RLB high-pass (~38 Hz)
    // computed cleanly for the active sample rate.
    let fs = rate.max(8_000) as f64;
    let shelf = Biquad::high_shelf(fs, 1681.974, 0.707175, 4.0);
    let hp = Biquad::high_pass(fs, 38.135, 0.500327);
    let mut s_l = shelf;
    let mut s_r = shelf;
    let mut h_l = hp;
    let mut h_r = hp;
    let mut energy = 0.0f64;
    for f in frames {
        let l = h_l.run(s_l.run(f[0] as f64));
        let r = h_r.run(s_r.run(f[1] as f64));
        energy += l * l + r * r;
    }
    let ms = energy / frames.len() as f64;
    if ms <= 1e-12 {
        -120.0
    } else {
        (-0.691 + 10.0 * ms.log10()).clamp(-120.0, 6.0) as f32
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn high_shelf(fs: f64, f0: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = std::f64::consts::TAU * (f0 / fs).min(0.45);
        let cw = w0.cos();
        let sw = w0.sin();
        let alpha = sw / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * cw + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) + (a - 1.0) * cw + two_sqrt_a_alpha)) / a0,
            b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cw)) / a0,
            b2: (a * ((a + 1.0) + (a - 1.0) * cw - two_sqrt_a_alpha)) / a0,
            a1: (2.0 * ((a - 1.0) - (a + 1.0) * cw)) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cw - two_sqrt_a_alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn high_pass(fs: f64, f0: f64, q: f64) -> Self {
        let w0 = std::f64::consts::TAU * (f0 / fs).min(0.45);
        let cw = w0.cos();
        let sw = w0.sin();
        let alpha = sw / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cw) * 0.5) / a0,
            b1: -(1.0 + cw) / a0,
            b2: ((1.0 + cw) * 0.5) / a0,
            a1: (-2.0 * cw) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// What Dirt and Space add: drive, then fewer bits past half way; reverb.
fn character(m: super::Macros) -> Vec<FxSlot> {
    let mut v = Vec::new();
    if m.dirt > 0.01 {
        let mut drive = FxSlot::new(FxKind::Drive);
        // Drive, Tone, Output, Mix.
        drive.params = vec![
            m.dirt * 30.0,
            14_000.0 - m.dirt * 8_000.0,
            -m.dirt * 9.0,
            100.0,
        ];
        v.push(drive);
    }
    if m.dirt > 0.5 {
        let x = (m.dirt - 0.5) * 2.0;
        let mut crush = FxSlot::new(FxKind::Bitcrusher);
        // Bits, Clock, Mix.
        crush.params = vec![12.0 - x * 8.0, 24_000.0 - x * 18_000.0, 100.0];
        v.push(crush);
    }
    if m.space > 0.01 {
        let mut rev = FxSlot::new(FxKind::Reverb);
        // Size, Damping, Width, Low cut, Wet, Dry.
        rev.params = vec![
            35.0 + m.space * 65.0,
            60.0 - m.space * 30.0,
            100.0,
            120.0,
            m.space * 55.0,
            100.0 - m.space * 25.0,
        ];
        v.push(rev);
    }
    v
}

/// Run enabled slots over interleaved stereo in blocks, honouring each
/// slot's mix. Past the end of its input a slot stops once its output has
/// been silent for a while (a reverb tail died out): the rest stays silent.
fn run_chain(fx: &[FxSlot], buf: &mut [f32]) {
    let mut dry = Vec::new();
    let mut end = sound_end(buf);
    for slot in fx.iter().filter(|s| s.enabled && s.kind != FxKind::Plugin) {
        let mut st = FxState::for_slot(slot, RATE as f32);
        let mut quiet = 0;
        let mut last = 0;
        // Echoes come back after a gap (up to 16 steps, 2 s at 120 BPM).
        let wait = match slot.kind {
            FxKind::Delay | FxKind::StereoDelay | FxKind::Time => ECHO_BLOCKS,
            _ => QUIET_BLOCKS,
        };
        for (b, chunk) in buf.chunks_mut(BLOCK * 2).enumerate() {
            let at = b * BLOCK;
            if at > end && quiet >= wait {
                break;
            }
            if slot.mix < 1.0 {
                dry.clear();
                dry.extend_from_slice(chunk);
            }
            st.process(&slot.params, chunk, RATE as f32, TEMPO);
            if slot.mix < 1.0 {
                for (w, d) in chunk.iter_mut().zip(&dry) {
                    *w = d + (*w - d) * slot.mix;
                }
            }
            if chunk.iter().all(|v| v.abs() <= SILENT) {
                quiet += 1;
            } else {
                quiet = 0;
                last = at + chunk.len() / 2;
            }
        }
        end = last;
    }
}

/// Blocks of silence (about 0.2 s) after which a tail counts as over.
const QUIET_BLOCKS: usize = 20;
/// The same for echoes: a little over 2 s.
const ECHO_BLOCKS: usize = 200;

/// The frame after the last audible one.
fn sound_end(buf: &[f32]) -> usize {
    buf.as_chunks::<2>()
        .0
        .iter()
        .rposition(|f| f[0].abs().max(f[1].abs()) > SILENT)
        .map_or(0, |i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::super::recipes::Recipe;
    use super::super::*;
    use super::*;

    fn rms(r: &Rendered) -> f32 {
        let s: f32 = r.frames.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum();
        (s / (r.frames.len().max(1) as f32 * 2.0)).sqrt()
    }

    #[test]
    fn every_recipe_renders_sound_without_clipping() {
        for r in Recipe::ALL {
            let out = render(&r.patch());
            assert!(out.frames.len() > RATE as usize / 100, "{r:?} too short");
            assert!(
                out.seconds() < f64::from(MAX_SECONDS + MAX_TAIL + 0.5),
                "{r:?} too long"
            );
            assert!(out.peak > 0.05, "{r:?} silent: peak {}", out.peak);
            assert!(out.peak <= 1.0, "{r:?} clips");
            assert!(rms(&out) > 0.003, "{r:?} too quiet");
            assert!(
                out.lufs > -60.0 && out.lufs < 3.0,
                "{r:?} lufs {}",
                out.lufs
            );
            assert!(
                out.frames
                    .iter()
                    .all(|f| f[0].is_finite() && f[1].is_finite()),
                "{r:?} NaN"
            );
            // One-shots fade cleanly to silence; seamless loops join smoothly across the seam.
            let first = out.frames.first().unwrap();
            let last = out.frames.last().unwrap();
            if out.seamless_loop {
                let jump = (first[0] - last[0]).abs().max((first[1] - last[1]).abs());
                assert!(jump < 0.18, "{r:?} loop seam jump {jump}");
            } else {
                assert!(
                    last[0].abs() < 0.01 && last[1].abs() < 0.01,
                    "{r:?} ends loud"
                );
            }
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        for r in Recipe::ALL {
            let p = r.patch();
            assert_eq!(render(&p), render(&p), "{r:?}");
        }
    }

    #[test]
    fn the_seed_changes_noise_only() {
        let mut p = Recipe::Explosion.patch();
        let a = render(&p);
        p.seed += 1;
        let b = render(&p);
        assert_ne!(a.frames, b.frames);
        assert!((a.seconds() - b.seconds()).abs() < 0.3);
    }

    #[test]
    fn knobs_do_what_they_say() {
        let mut p = Recipe::Hit.patch();
        // Without ringing effects the length is the layers' own.
        p.fx.clear();
        p.macros.space = 0.0;
        let mut big = p.clone();
        big.macros.size = 1.0;
        let mut small = p.clone();
        small.macros.size = 0.0;
        assert!(render(&big).seconds() > render(&small).seconds());

        let mut wet = p.clone();
        wet.macros.space = 1.0;
        assert!(render(&wet).seconds() > render(&p).seconds() + 0.2);

        let mut loud = p.clone();
        loud.macros.energy = 1.0;
        let mut soft = p.clone();
        soft.macros.energy = 0.0;
        assert!(rms(&render(&loud)) > rms(&render(&soft)));
    }

    #[test]
    fn mute_and_solo_pick_layers() {
        let mut p = Recipe::Coin.patch();
        for l in &mut p.layers {
            l.enabled = false;
        }
        assert!(render(&p).peak < SILENT);
        p.layers[1].solo = true;
        assert!(render(&p).peak > 0.05, "solo sounds even when muted");
    }

    #[test]
    fn layer_effects_and_samples_play() {
        let mut p = ForgePatch::blank();
        let tone: Vec<[f32; 2]> = (0..4800)
            .map(|i| {
                let v = (i as f32 * 0.05).sin() * 0.5;
                [v, v]
            })
            .collect();
        p.layers.push(
            Layer::new(
                "Sample",
                Source::Sample {
                    name: "tone".into(),
                    rate: 48_000,
                    frames: std::sync::Arc::new(tone),
                    reverse: true,
                    from: None,
                    trim_start: 0.0,
                    trim_end: 1.0,
                },
                0.2,
            )
            .with_amp(Curve::flat(1.0))
            .with_fx(FxSlot::new(FxKind::Delay)),
        );
        let out = render(&p);
        assert!(out.peak > 0.1);
        // The delay rings past the layer's 0.2 s.
        assert!(out.seconds() > 0.3, "{}", out.seconds());
    }

    #[test]
    fn a_synth_layer_plays_its_patch() {
        let mut p = ForgePatch::blank();
        p.layers.push(
            Layer::new(
                "Tide",
                Source::Synth {
                    patch: Box::default(),
                    key: 60,
                },
                0.3,
            )
            .with_amp(Curve::flat(1.0)),
        );
        assert!(render(&p).peak > 0.02);
    }

    #[test]
    fn a_broken_patch_is_tidied_not_rendered_wild() {
        let mut p = Recipe::Laser.patch();
        p.layers[0].gain = f32::NAN;
        p.layers[0].length = 1e9;
        p.macros.size = 7.0;
        p.fx.push(FxSlot {
            params: vec![f32::INFINITY; 2],
            ..FxSlot::new(FxKind::Reverb)
        });
        let s = ForgeSound::new(p);
        assert!(s.rendered.peak <= 1.0 && s.rendered.peak > 0.05);
        assert!(s.rendered.seconds() < 20.0);
    }

    #[test]
    fn modal_materials_render_distinct_clean_impacts() {
        let mut peaks = Vec::new();
        let mut first_buf = None;
        for mat in ModalMaterial::ALL {
            let mut p = ForgePatch::blank();
            p.layers.push(
                Layer::new("Strike", Source::modal(mat, 380.0), 0.45).with_amp(Curve::flat(1.0)),
            );
            let r = render(&p);
            assert!(r.peak > 0.08 && r.peak <= 1.0, "{mat:?} peak {}", r.peak);
            assert!(
                r.frames
                    .iter()
                    .all(|f| f[0].is_finite() && f[1].is_finite())
            );
            // Deterministic.
            assert_eq!(r, render(&p));
            if let Some(prev) = &first_buf {
                assert_ne!(prev, &r.frames, "{mat:?} should differ from Metal");
            } else {
                first_buf = Some(r.frames.clone());
            }
            peaks.push(r.peak);
        }
        assert_eq!(peaks.len(), 5);
    }

    #[test]
    fn granular_procedural_and_sample_clouds_render_stereo() {
        let mut proc_patch = ForgePatch::blank();
        proc_patch.layers.push(
            Layer::new("Debris", Source::grain(900.0, 55.0, 22.0), 0.5)
                .with_amp(Curve::flat(1.0))
                .with_morph(CurvePreset::Bell.build(0.0, 1.0, 0.5)),
        );
        let r = render(&proc_patch);
        assert!(r.peak > 0.05 && r.peak <= 1.0, "proc grain peak {}", r.peak);
        assert!(
            r.frames.iter().any(|f| (f[0] - f[1]).abs() > 0.02),
            "should have stereo spread"
        );
        assert_eq!(r, render(&proc_patch), "granular must be deterministic");

        // Sample-based granular cloud.
        let tone: Vec<[f32; 2]> = (0..12_000)
            .map(|i| {
                let v = (i as f32 * 0.04).sin() * 0.6;
                [v, v * 0.8]
            })
            .collect();
        let mut samp_patch = ForgePatch::blank();
        samp_patch.layers.push(
            Layer::new(
                "Cloud",
                Source::Grain {
                    hz: 440.0,
                    density: 40.0,
                    grain_ms: 35.0,
                    scatter: 0.25,
                    spread: 0.8,
                    spray: 0.6,
                    sample: Some(GrainSample {
                        name: "tone".into(),
                        rate: RATE,
                        frames: Arc::new(tone),
                    }),
                },
                0.4,
            )
            .with_amp(Curve::flat(1.0)),
        );
        let rs = render(&samp_patch);
        assert!(
            rs.peak > 0.05 && rs.peak <= 1.0,
            "sample grain peak {}",
            rs.peak
        );
    }

    #[test]
    fn osc_plus_fm_unison_shape_and_pan_curve_work() {
        let mut base = ForgePatch::blank();
        base.layers.push(
            Layer::new("Base", Source::osc(Wave::Sine, 220.0), 0.25).with_amp(Curve::flat(0.9)),
        );
        let r_base = render(&base);

        let mut fm = base.clone();
        if let Source::Osc {
            fm_ratio,
            fm_index,
            unison,
            detune,
            shape,
            ..
        } = &mut fm.layers[0].source
        {
            *fm_ratio = 2.5;
            *fm_index = 2.2;
            *unison = 3;
            *detune = 0.6;
            *shape = 0.4;
        }
        fm.layers[0].pan_curve = Curve::new(&[(0.0, -1.0, 0.0), (1.0, 1.0, 0.0)]);
        let r_fm = render(&fm);
        assert!(r_fm.peak > 0.1 && r_fm.peak <= 1.0);
        assert_ne!(r_base.frames, r_fm.frames);
        // Pan sweeps from left-heavy at start to right-heavy at end.
        let n = r_fm.frames.len();
        let head_l: f32 = r_fm.frames[..n / 5].iter().map(|f| f[0].abs()).sum();
        let head_r: f32 = r_fm.frames[..n / 5].iter().map(|f| f[1].abs()).sum();
        let tail_l: f32 = r_fm.frames[n * 4 / 5..].iter().map(|f| f[0].abs()).sum();
        let tail_r: f32 = r_fm.frames[n * 4 / 5..].iter().map(|f| f[1].abs()).sum();
        assert!(
            head_l > head_r * 1.5,
            "start should pan left: {head_l} vs {head_r}"
        );
        assert!(
            tail_r > tail_l * 1.5,
            "end should pan right: {tail_r} vs {tail_l}"
        );
    }

    #[test]
    fn sample_trim_slices_the_playback_region() {
        // First half is silent, second half is a loud sine.
        let mut data = vec![[0.0f32; 2]; 9600];
        for (i, f) in data[4800..].iter_mut().enumerate() {
            let v = (i as f32 * 0.06).sin() * 0.7;
            *f = [v, v];
        }
        let mut p = ForgePatch::blank();
        p.layers.push(
            Layer::new(
                "Trimmed",
                Source::Sample {
                    name: "half".into(),
                    rate: RATE,
                    frames: Arc::new(data),
                    reverse: false,
                    from: None,
                    trim_start: 0.5,
                    trim_end: 1.0,
                },
                0.1,
            )
            .with_amp(Curve::flat(1.0)),
        );
        let r = render(&p);
        assert!(r.peak > 0.3, "trimmed region should immediately sound");
        let early: f32 = r.frames[..200]
            .iter()
            .map(|f| f[0].abs())
            .fold(0.0, f32::max);
        assert!(early > 0.3, "should skip the silent first half");
    }

    #[test]
    fn seamless_loop_has_continuous_seam_and_steady_energy() {
        let mut p = ForgePatch::blank();
        p.seamless_loop = true;
        p.macros.space = 0.35;
        p.layers.push(
            Layer::new("Drone", Source::osc(Wave::Sine, 137.0), 0.6).with_amp(Curve::flat(0.75)),
        );
        let r = render(&p);
        assert!(r.seamless_loop);
        assert!(r.peak > 0.2 && r.peak <= 1.0);
        // Check seam continuity: jump from last frame to first frame must be small (no click).
        let first = r.frames.first().unwrap();
        let last = r.frames.last().unwrap();
        let seam_jump = (first[0] - last[0]).abs().max((first[1] - last[1]).abs());
        assert!(seam_jump < 0.08, "loop seam clicked: jump = {seam_jump}");
        // Must not fade to silence at the end like a one-shot.
        let end_rms: f32 = (r.frames[r.frames.len() - 400..]
            .iter()
            .map(|f| f[0] * f[0] + f[1] * f[1])
            .sum::<f32>()
            / 800.0)
            .sqrt();
        assert!(end_rms > 0.1, "seamless loop faded out at end: {end_rms}");
    }
}
