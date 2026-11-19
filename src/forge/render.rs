//! Offline render of a whole Forge patch: layers, their effects, the mix,
//! master effects, the character knobs and a safety limiter.

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
    let tail = if any_fx || m.space > 0.01 { MAX_TAIL } else { 0.0 };
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
        for (f, s) in buf.chunks_exact(2).enumerate() {
            let Some(o) = mix.get_mut((at + f) * 2..(at + f) * 2 + 2) else {
                break;
            };
            o[0] += s[0] * gl;
            o[1] += s[1] * gr;
        }
    }
    run_chain(&p.fx, &mut mix);
    run_chain(&character(m), &mut mix);
    let gain = crate::fx::db(p.gain_db + (m.energy - 0.5) * 6.0);
    for s in &mut mix {
        *s *= gain;
    }
    let mut limit = FxSlot::new(FxKind::Limiter);
    limit.params = vec![0.0, -1.0, 60.0];
    run_chain(&[limit], &mut mix);
    let mut frames: Vec<[f32; 2]> = mix
        .chunks_exact(2)
        .map(|c| {
            let ok = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
            [ok(c[0]), ok(c[1])]
        })
        .collect();
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
    let peak = frames
        .iter()
        .map(|f| f[0].abs().max(f[1].abs()))
        .fold(0.0, f32::max);
    Rendered {
        rate: RATE,
        frames,
        peak,
    }
}

/// What Dirt and Space add: drive, then fewer bits past half way; reverb.
fn character(m: super::Macros) -> Vec<FxSlot> {
    let mut v = Vec::new();
    if m.dirt > 0.01 {
        let mut drive = FxSlot::new(FxKind::Drive);
        // Drive, Tone, Output, Mix.
        drive.params = vec![m.dirt * 30.0, 14_000.0 - m.dirt * 8_000.0, -m.dirt * 9.0, 100.0];
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
    buf.chunks_exact(2)
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
            assert!(out.seconds() < 12.0, "{r:?} too long");
            assert!(out.peak > 0.05, "{r:?} silent: peak {}", out.peak);
            assert!(out.peak <= 1.0, "{r:?} clips");
            assert!(rms(&out) > 0.003, "{r:?} too quiet");
            assert!(
                out.frames.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
                "{r:?} NaN"
            );
            // The sound ends quietly (faded), never on a click.
            let last = out.frames.last().unwrap();
            assert!(last[0].abs() < 0.01 && last[1].abs() < 0.01, "{r:?} ends loud");
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
}
