//! Automatable & MIDI-linkable parameter enums for built-in instruments
//! (Sampler, Tide Synth, Vocal, Forge), plus zero-allocation audio-thread
//! override resolvers.

use crate::automation::{Target, ValueAt};
use crate::engine::LiveParam;
use crate::sampler::{Adsr, Program, SamplePreset, Zone, ZoneParams};
use crate::sequence::{InstrumentKind, Song};
use crate::synth::SynthPatch;
use serde::{Deserialize, Serialize};

/// Every automatable knob on a Sampler channel (program or zone level).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamplerParam {
    // Program tab
    ProgVolume,
    ProgPan,
    ProgTranspose,
    ProgBendRange,
    ProgPolyphony,
    ProgGlide,
    // Zone pitch & amp
    ZoneSemi,
    ZoneFine,
    ZoneKeyTrack,
    ZoneGain,
    ZonePan,
    ZoneVelTrack,
    ZoneMuteGroup,
    ZoneStretchAmount,
    ZoneStretchBeats,
    // Zone Amp DAHDSR
    ZoneAmpDelay,
    ZoneAmpAttack,
    ZoneAmpHold,
    ZoneAmpDecay,
    ZoneAmpSustain,
    ZoneAmpRelease,
    // Zone Filters (0 = F1, 1 = F2)
    ZoneFilterCutoff(u8),
    ZoneFilterEmph(u8),
    // Zone Free Envelopes (0 = ENV 1, 1 = ENV 2)
    ZoneEnvDelay(u8),
    ZoneEnvAttack(u8),
    ZoneEnvHold(u8),
    ZoneEnvDecay(u8),
    ZoneEnvSustain(u8),
    ZoneEnvRelease(u8),
    // Zone LFOs (0 = LFO 1, 1 = LFO 2)
    ZoneLfoRate(u8),
    ZoneLfoAttack(u8),
    ZoneLfoPhase(u8),
}

impl SamplerParam {
    pub fn is_program(self) -> bool {
        matches!(
            self,
            Self::ProgVolume
                | Self::ProgPan
                | Self::ProgTranspose
                | Self::ProgBendRange
                | Self::ProgPolyphony
                | Self::ProgGlide
        )
    }

    pub fn is_log(self) -> bool {
        matches!(
            self,
            Self::ZoneFilterCutoff(_)
                | Self::ZoneLfoRate(_)
                | Self::ZoneStretchAmount
                | Self::ZoneStretchBeats
        )
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Self::ProgVolume => (-60.0, 12.0),
            Self::ProgPan | Self::ZonePan | Self::ZoneKeyTrack => (-1.0, 1.0),
            Self::ProgTranspose => (-48.0, 48.0),
            Self::ProgBendRange => (0.0, 4800.0),
            Self::ProgPolyphony => (1.0, 64.0),
            Self::ProgGlide
            | Self::ZoneAmpDelay
            | Self::ZoneAmpAttack
            | Self::ZoneAmpHold
            | Self::ZoneEnvDelay(_)
            | Self::ZoneEnvAttack(_)
            | Self::ZoneEnvHold(_)
            | Self::ZoneLfoAttack(_) => (0.0, 10.0),
            Self::ZoneSemi => (-36.0, 36.0),
            Self::ZoneFine => (-50.0, 50.0),
            Self::ZoneGain => (-60.0, 24.0),
            Self::ZoneVelTrack
            | Self::ZoneAmpSustain
            | Self::ZoneFilterEmph(_)
            | Self::ZoneEnvSustain(_)
            | Self::ZoneLfoPhase(_) => (0.0, 1.0),
            Self::ZoneMuteGroup => (0.0, 16.0),
            Self::ZoneStretchAmount => (0.25, 4.0),
            Self::ZoneStretchBeats => (0.25, 64.0),
            Self::ZoneAmpDecay
            | Self::ZoneAmpRelease
            | Self::ZoneEnvDecay(_)
            | Self::ZoneEnvRelease(_) => (0.0, 20.0),
            Self::ZoneFilterCutoff(_) => (20.0, 20000.0),
            Self::ZoneLfoRate(_) => (0.01, 50.0),
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::ProgVolume
            | Self::ProgPan
            | Self::ProgTranspose
            | Self::ProgGlide
            | Self::ZoneSemi
            | Self::ZoneFine
            | Self::ZoneGain
            | Self::ZonePan
            | Self::ZoneMuteGroup
            | Self::ZoneAmpDelay
            | Self::ZoneAmpHold
            | Self::ZoneAmpDecay
            | Self::ZoneFilterEmph(_)
            | Self::ZoneEnvDelay(_)
            | Self::ZoneEnvAttack(_)
            | Self::ZoneEnvHold(_)
            | Self::ZoneEnvDecay(_)
            | Self::ZoneEnvRelease(_)
            | Self::ZoneLfoAttack(_)
            | Self::ZoneLfoPhase(_) => 0.0,
            Self::ProgBendRange => 200.0,
            Self::ProgPolyphony => 32.0,
            Self::ZoneKeyTrack
            | Self::ZoneVelTrack
            | Self::ZoneStretchAmount
            | Self::ZoneAmpSustain
            | Self::ZoneEnvSustain(_) => 1.0,
            Self::ZoneStretchBeats => 4.0,
            Self::ZoneAmpAttack => 0.005,
            Self::ZoneAmpRelease => 0.015,
            Self::ZoneFilterCutoff(_) => 20000.0,
            Self::ZoneLfoRate(_) => 5.0,
        }
    }

    pub fn get(self, preset: &SamplePreset, zone: Option<u16>) -> Option<f32> {
        if self.is_program() {
            let pr = &preset.program;
            return Some(match self {
                Self::ProgVolume => pr.volume_db as f32,
                Self::ProgPan => pr.pan as f32,
                Self::ProgTranspose => pr.transpose as f32,
                Self::ProgBendRange => pr.bend_range as f32,
                Self::ProgPolyphony => pr.polyphony as f32,
                Self::ProgGlide => pr.glide as f32,
                _ => unreachable!(),
            });
        }
        let z = preset.zones.get(zone.unwrap_or(0) as usize)?;
        Some(self.get_zone(z))
    }

    fn get_zone(self, z: &Zone) -> f32 {
        match self {
            Self::ZoneSemi => (z.tune_cents / 100.0).round() as f32,
            Self::ZoneFine => (z.tune_cents - (z.tune_cents / 100.0).round() * 100.0) as f32,
            Self::ZoneKeyTrack => z.params.key_track as f32,
            Self::ZoneGain => {
                if z.gain <= 1e-4 {
                    -60.0
                } else {
                    (20.0 * z.gain.log10()).clamp(-60.0, 24.0) as f32
                }
            }
            Self::ZonePan => z.pan as f32,
            Self::ZoneVelTrack => z.velocity_tracking as f32,
            Self::ZoneMuteGroup => z.params.mute_group as f32,
            Self::ZoneStretchAmount => z.stretch.amount as f32,
            Self::ZoneStretchBeats => z.stretch.beats as f32,
            Self::ZoneAmpDelay => z.adsr.delay as f32,
            Self::ZoneAmpAttack => z.adsr.attack as f32,
            Self::ZoneAmpHold => z.adsr.hold as f32,
            Self::ZoneAmpDecay => z.adsr.decay as f32,
            Self::ZoneAmpSustain => z.adsr.sustain as f32,
            Self::ZoneAmpRelease => z.adsr.release as f32,
            Self::ZoneFilterCutoff(fi) => z.params.filters[(fi as usize).min(1)].cutoff as f32,
            Self::ZoneFilterEmph(fi) => z.params.filters[(fi as usize).min(1)].emphasis as f32,
            Self::ZoneEnvDelay(ei) => z.params.env[(ei as usize).min(1)].delay as f32,
            Self::ZoneEnvAttack(ei) => z.params.env[(ei as usize).min(1)].attack as f32,
            Self::ZoneEnvHold(ei) => z.params.env[(ei as usize).min(1)].hold as f32,
            Self::ZoneEnvDecay(ei) => z.params.env[(ei as usize).min(1)].decay as f32,
            Self::ZoneEnvSustain(ei) => z.params.env[(ei as usize).min(1)].sustain as f32,
            Self::ZoneEnvRelease(ei) => z.params.env[(ei as usize).min(1)].release as f32,
            Self::ZoneLfoRate(li) => z.params.lfo[(li as usize).min(1)].rate as f32,
            Self::ZoneLfoAttack(li) => z.params.lfo[(li as usize).min(1)].attack as f32,
            Self::ZoneLfoPhase(li) => z.params.lfo[(li as usize).min(1)].phase as f32,
            _ => 0.0,
        }
    }

    pub fn set(self, preset: &mut SamplePreset, zone: Option<u16>, value: f32) -> bool {
        let (lo, hi) = self.range();
        let v = value.clamp(lo, hi) as f64;
        if self.is_program() {
            self.apply_program(&mut preset.program, v);
            return true;
        }
        match zone {
            Some(zi) => {
                let Some(z) = preset.zones.get_mut(zi as usize) else {
                    return false;
                };
                self.apply_zone(z, v);
                true
            }
            None => {
                if preset.zones.is_empty() {
                    return false;
                }
                for z in &mut preset.zones {
                    self.apply_zone(z, v);
                }
                true
            }
        }
    }

    pub fn apply_program(self, pr: &mut Program, v: f64) {
        match self {
            Self::ProgVolume => pr.volume_db = v.clamp(-60.0, 12.0),
            Self::ProgPan => pr.pan = v.clamp(-1.0, 1.0),
            Self::ProgTranspose => pr.transpose = v.round().clamp(-48.0, 48.0) as i8,
            Self::ProgBendRange => pr.bend_range = v.clamp(0.0, 4800.0),
            Self::ProgPolyphony => pr.polyphony = v.round().clamp(1.0, 64.0) as u8,
            Self::ProgGlide => pr.glide = if v < 0.0011 { 0.0 } else { v.clamp(0.0, 10.0) },
            _ => {}
        }
    }

    fn apply_zone(self, z: &mut Zone, v: f64) {
        let mut rt = ZoneRuntime::from_zone(z);
        self.apply_zone_runtime(&mut rt, v);
        z.params = rt.params;
        z.adsr = rt.adsr;
        z.gain = rt.gain;
        z.pan = rt.pan;
        z.tune_cents = rt.tune_cents;
        z.velocity_tracking = rt.velocity_tracking;
        match self {
            Self::ZoneStretchAmount => z.stretch.amount = v.clamp(0.25, 4.0),
            Self::ZoneStretchBeats => {
                z.stretch.beats = ((v * 4.0).round() / 4.0).clamp(0.25, 64.0);
            }
            _ => {}
        }
    }

    fn apply_zone_runtime(self, rt: &mut ZoneRuntime, v: f64) {
        let snap_time = |x: f64, hi: f64| if x < 0.0011 { 0.0 } else { x.clamp(0.0, hi) };
        match self {
            Self::ZoneSemi => {
                let fine = rt.tune_cents - (rt.tune_cents / 100.0).round() * 100.0;
                rt.tune_cents = (v.round() * 100.0 + fine).clamp(-3650.0, 3650.0);
            }
            Self::ZoneFine => {
                let semi = (rt.tune_cents / 100.0).round();
                rt.tune_cents = (semi * 100.0 + v.clamp(-50.0, 50.0)).clamp(-3650.0, 3650.0);
            }
            Self::ZoneKeyTrack => rt.params.key_track = v.clamp(-1.0, 1.0),
            Self::ZoneGain => rt.gain = 10f64.powf(v.clamp(-60.0, 24.0) / 20.0).min(16.0),
            Self::ZonePan => rt.pan = v.clamp(-1.0, 1.0),
            Self::ZoneVelTrack => rt.velocity_tracking = v.clamp(0.0, 1.0),
            Self::ZoneMuteGroup => rt.params.mute_group = v.round().clamp(0.0, 16.0) as u8,
            Self::ZoneAmpDelay => rt.adsr.delay = snap_time(v, 10.0),
            Self::ZoneAmpAttack => rt.adsr.attack = snap_time(v, 10.0),
            Self::ZoneAmpHold => rt.adsr.hold = snap_time(v, 10.0),
            Self::ZoneAmpDecay => rt.adsr.decay = snap_time(v, 20.0),
            Self::ZoneAmpSustain => rt.adsr.sustain = v.clamp(0.0, 1.0),
            Self::ZoneAmpRelease => rt.adsr.release = snap_time(v, 20.0),
            Self::ZoneFilterCutoff(fi) => {
                rt.params.filters[(fi as usize).min(1)].cutoff = v.clamp(20.0, 20000.0);
            }
            Self::ZoneFilterEmph(fi) => {
                rt.params.filters[(fi as usize).min(1)].emphasis = v.clamp(0.0, 1.0);
            }
            Self::ZoneEnvDelay(ei) => {
                rt.params.env[(ei as usize).min(1)].delay = snap_time(v, 10.0)
            }
            Self::ZoneEnvAttack(ei) => {
                rt.params.env[(ei as usize).min(1)].attack = snap_time(v, 10.0);
            }
            Self::ZoneEnvHold(ei) => rt.params.env[(ei as usize).min(1)].hold = snap_time(v, 10.0),
            Self::ZoneEnvDecay(ei) => {
                rt.params.env[(ei as usize).min(1)].decay = snap_time(v, 20.0)
            }
            Self::ZoneEnvSustain(ei) => {
                rt.params.env[(ei as usize).min(1)].sustain = v.clamp(0.0, 1.0);
            }
            Self::ZoneEnvRelease(ei) => {
                rt.params.env[(ei as usize).min(1)].release = snap_time(v, 20.0);
            }
            Self::ZoneLfoRate(li) => {
                rt.params.lfo[(li as usize).min(1)].rate = v.clamp(0.01, 50.0);
            }
            Self::ZoneLfoAttack(li) => {
                rt.params.lfo[(li as usize).min(1)].attack = v.clamp(0.0, 10.0);
            }
            Self::ZoneLfoPhase(li) => {
                rt.params.lfo[(li as usize).min(1)].phase = v.clamp(0.0, 1.0);
            }
            _ => {}
        }
    }
}

/// Stack-only mutable view of a sampler zone's real-time sound parameters
/// (no `String` or `Vec` clones — zero allocations on the audio thread).
#[derive(Clone)]
pub struct ZoneRuntime {
    pub params: ZoneParams,
    pub adsr: Adsr,
    pub gain: f64,
    pub pan: f64,
    pub tune_cents: f64,
    pub velocity_tracking: f64,
}

impl ZoneRuntime {
    pub fn from_zone(z: &Zone) -> Self {
        Self {
            params: z.params.clone(),
            adsr: z.adsr.clone(),
            gain: z.gain,
            pan: z.pan,
            tune_cents: z.tune_cents,
            velocity_tracking: z.velocity_tracking,
        }
    }
}

/// Fast check: are there any automated or live Sampler overrides for `inst`?
pub fn has_sampler_overrides(inst: u64, auto: &[ValueAt], live: &[LiveParam]) -> bool {
    auto.iter()
        .any(|a| matches!(a.target, Target::SamplerParam { inst: i, .. } if i == inst))
        || live
            .iter()
            .any(|l| matches!(l.target, Target::SamplerParam { inst: i, .. } if i == inst))
}

/// Resolve Sampler overrides for `(inst, zone_idx)` into stack copies with zero allocations.
/// Automation wins over live overrides for the same target.
pub fn resolve_sampler_runtime(
    inst: u64,
    zone_idx: usize,
    z: &Zone,
    program: &Program,
    auto: &[ValueAt],
    live: &[LiveParam],
) -> (ZoneRuntime, Program) {
    let mut rt = ZoneRuntime::from_zone(z);
    let mut prog = program.clone();
    let applies = |zone: Option<u16>| zone.is_none() || zone == Some(zone_idx as u16);
    // Apply live overrides first, then automation clips so automation wins.
    for l in live {
        if let Target::SamplerParam {
            inst: i,
            zone,
            param,
        } = l.target
            && i == inst
        {
            if param.is_program() {
                param.apply_program(&mut prog, l.value as f64);
            } else if applies(zone) {
                param.apply_zone_runtime(&mut rt, l.value as f64);
            }
        }
    }
    for a in auto {
        if let Target::SamplerParam {
            inst: i,
            zone,
            param,
        } = a.target
            && i == inst
        {
            if param.is_program() {
                param.apply_program(&mut prog, a.value as f64);
            } else if applies(zone) {
                param.apply_zone_runtime(&mut rt, a.value as f64);
            }
        }
    }
    (rt, prog)
}

/// Every automatable knob on Tide Synth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthParam {
    Position,
    Warp,
    Level,
    Coarse,
    Fine,
    Sub,
    Noise,
    PosB,
    WarpB,
    LevelB,
    CoarseB,
    FineB,
    LevelC,
    CoarseC,
    PartialsC,
    TiltC,
    OddC,
    ShimmerC,
    Cutoff,
    Resonance,
    Keytrack,
    EnvAmount,
    AmpAttack,
    AmpDecay,
    AmpSustain,
    AmpRelease,
    FilterAttack,
    FilterDecay,
    FilterSustain,
    FilterRelease,
    LfoRate(u8),
    Unison,
    Detune,
    Spread,
    Glide,
    BendRange,
    Macro(u8),
}

impl SynthParam {
    pub fn is_log(self) -> bool {
        matches!(self, Self::Cutoff | Self::LfoRate(_))
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Position
            | Self::Level
            | Self::Sub
            | Self::Noise
            | Self::PosB
            | Self::LevelB
            | Self::LevelC
            | Self::ShimmerC
            | Self::Resonance
            | Self::AmpSustain
            | Self::FilterSustain
            | Self::Detune
            | Self::Spread
            | Self::Macro(_) => (0.0, 1.0),
            Self::Warp
            | Self::WarpB
            | Self::TiltC
            | Self::OddC
            | Self::Keytrack
            | Self::EnvAmount => (-1.0, 1.0),
            Self::Coarse | Self::CoarseB | Self::CoarseC => (-24.0, 24.0),
            Self::Fine | Self::FineB => (-100.0, 100.0),
            Self::PartialsC => (1.0, crate::synth::MAX_PARTIALS as f32),
            Self::Cutoff => (20.0, 20000.0),
            Self::AmpAttack | Self::FilterAttack | Self::Glide => (0.0, 10.0),
            Self::AmpDecay | Self::AmpRelease | Self::FilterDecay | Self::FilterRelease => {
                (0.0, 20.0)
            }
            Self::LfoRate(_) => (0.01, 30.0),
            Self::Unison => (1.0, 7.0),
            Self::BendRange => (0.0, 48.0),
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::Level | Self::AmpSustain | Self::FilterSustain | Self::Unison => 1.0,
            Self::PartialsC => 16.0,
            Self::Cutoff => 18000.0,
            Self::AmpAttack | Self::FilterAttack => 0.005,
            Self::AmpRelease | Self::FilterRelease => 0.015,
            Self::LfoRate(_) => 1.0,
            Self::BendRange => 2.0,
            _ => 0.0,
        }
    }

    pub fn get(self, p: &SynthPatch) -> f32 {
        match self {
            Self::Position => p.position as f32,
            Self::Warp => p.warp_amount as f32,
            Self::Level => p.level as f32,
            Self::Coarse => p.coarse as f32,
            Self::Fine => p.fine as f32,
            Self::Sub => p.sub as f32,
            Self::Noise => p.noise as f32,
            Self::PosB => p.osc_b.position as f32,
            Self::WarpB => p.osc_b.warp_amount as f32,
            Self::LevelB => p.osc_b.level as f32,
            Self::CoarseB => p.osc_b.coarse as f32,
            Self::FineB => p.osc_b.fine as f32,
            Self::LevelC => p.spectral.level as f32,
            Self::CoarseC => p.spectral.coarse as f32,
            Self::PartialsC => p.spectral.partials as f32,
            Self::TiltC => p.spectral.tilt as f32,
            Self::OddC => p.spectral.odd as f32,
            Self::ShimmerC => p.spectral.shimmer as f32,
            Self::Cutoff => p.cutoff as f32,
            Self::Resonance => p.resonance as f32,
            Self::Keytrack => p.keytrack as f32,
            Self::EnvAmount => p.env_amount as f32,
            Self::AmpAttack => p.amp_env.attack as f32,
            Self::AmpDecay => p.amp_env.decay as f32,
            Self::AmpSustain => p.amp_env.sustain as f32,
            Self::AmpRelease => p.amp_env.release as f32,
            Self::FilterAttack => p.filter_env.attack as f32,
            Self::FilterDecay => p.filter_env.decay as f32,
            Self::FilterSustain => p.filter_env.sustain as f32,
            Self::FilterRelease => p.filter_env.release as f32,
            Self::LfoRate(i) => p.lfos[(i as usize).min(1)].rate as f32,
            Self::Unison => p.unison as f32,
            Self::Detune => p.detune as f32,
            Self::Spread => p.spread as f32,
            Self::Glide => p.glide as f32,
            Self::BendRange => p.bend_range as f32,
            Self::Macro(i) => p.macros[(i as usize).min(3)].value as f32,
        }
    }

    pub fn set(self, p: &mut SynthPatch, value: f32) {
        let (lo, hi) = self.range();
        let v = value.clamp(lo, hi) as f64;
        let snap = |x: f64| if x < 0.0011 { 0.0 } else { x };
        match self {
            Self::Position => p.position = v,
            Self::Warp => p.warp_amount = v,
            Self::Level => p.level = v,
            Self::Coarse => p.coarse = v,
            Self::Fine => p.fine = v,
            Self::Sub => p.sub = v,
            Self::Noise => p.noise = v,
            Self::PosB => p.osc_b.position = v,
            Self::WarpB => p.osc_b.warp_amount = v,
            Self::LevelB => p.osc_b.level = v,
            Self::CoarseB => p.osc_b.coarse = v,
            Self::FineB => p.osc_b.fine = v,
            Self::LevelC => p.spectral.level = v,
            Self::CoarseC => p.spectral.coarse = v,
            Self::PartialsC => {
                p.spectral.partials = v.round().clamp(1.0, crate::synth::MAX_PARTIALS as f64) as u8;
            }
            Self::TiltC => p.spectral.tilt = v,
            Self::OddC => p.spectral.odd = v,
            Self::ShimmerC => p.spectral.shimmer = v,
            Self::Cutoff => p.cutoff = v,
            Self::Resonance => p.resonance = v,
            Self::Keytrack => p.keytrack = v,
            Self::EnvAmount => p.env_amount = v,
            Self::AmpAttack => p.amp_env.attack = snap(v),
            Self::AmpDecay => p.amp_env.decay = snap(v),
            Self::AmpSustain => p.amp_env.sustain = v,
            Self::AmpRelease => p.amp_env.release = snap(v),
            Self::FilterAttack => p.filter_env.attack = snap(v),
            Self::FilterDecay => p.filter_env.decay = snap(v),
            Self::FilterSustain => p.filter_env.sustain = v,
            Self::FilterRelease => p.filter_env.release = snap(v),
            Self::LfoRate(i) => p.lfos[(i as usize).min(1)].rate = v,
            Self::Unison => p.unison = v.round().clamp(1.0, 7.0) as u8,
            Self::Detune => p.detune = v,
            Self::Spread => p.spread = v,
            Self::Glide => p.glide = snap(v),
            Self::BendRange => p.bend_range = v,
            Self::Macro(i) => p.macros[(i as usize).min(3)].value = v,
        }
    }
}

/// Fast check: are there any automated or live Synth parameter overrides for `inst`?
pub fn has_synth_overrides(inst: u64, auto: &[ValueAt], live: &[LiveParam]) -> bool {
    auto.iter()
        .any(|a| matches!(a.target, Target::SynthParam { inst: i, .. } if i == inst))
        || live
            .iter()
            .any(|l| matches!(l.target, Target::SynthParam { inst: i, .. } if i == inst))
}

/// Apply live & automated `SynthParam` overrides to `p` (zero allocations).
pub fn apply_synth_overrides(inst: u64, p: &mut SynthPatch, auto: &[ValueAt], live: &[LiveParam]) {
    for l in live {
        if let Target::SynthParam { inst: i, param } = l.target
            && i == inst
        {
            param.set(p, l.value);
        }
    }
    for a in auto {
        if let Target::SynthParam { inst: i, param } = a.target
            && i == inst
        {
            param.set(p, a.value);
        }
    }
}

/// Every automatable knob on a Vocal channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VocalParam {
    VibratoDepth,
    VibratoRate,
    VibratoDelay,
    VibratoFade,
    Transition,
    Gender,
    Breath,
}

impl VocalParam {
    pub fn is_log(self) -> bool {
        matches!(self, Self::Transition)
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Self::VibratoDepth => (0.0, 150.0),
            Self::VibratoRate => (2.0, 9.0),
            Self::VibratoDelay | Self::VibratoFade => (0.0, 1.5),
            Self::Transition => (0.01, 0.3),
            Self::Gender => (-1.0, 1.0),
            Self::Breath => (0.0, 1.0),
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::VibratoDepth => 35.0,
            Self::VibratoRate => 5.5,
            Self::VibratoDelay => 0.3,
            Self::VibratoFade => 0.25,
            Self::Transition => 0.07,
            Self::Gender | Self::Breath => 0.0,
        }
    }

    pub fn get(self, v: &crate::vocal::Voice) -> f32 {
        let p = &v.params;
        match self {
            Self::VibratoDepth => p.vibrato_depth as f32,
            Self::VibratoRate => p.vibrato_rate as f32,
            Self::VibratoDelay => p.vibrato_delay as f32,
            Self::VibratoFade => p.vibrato_fade as f32,
            Self::Transition => p.transition as f32,
            Self::Gender => p.gender as f32,
            Self::Breath => p.breath as f32,
        }
    }

    pub fn set(self, v: &mut crate::vocal::Voice, value: f32) {
        let (lo, hi) = self.range();
        let x = value.clamp(lo, hi) as f64;
        let p = &mut v.params;
        match self {
            Self::VibratoDepth => p.vibrato_depth = x,
            Self::VibratoRate => p.vibrato_rate = x,
            Self::VibratoDelay => p.vibrato_delay = x,
            Self::VibratoFade => p.vibrato_fade = x,
            Self::Transition => p.transition = x,
            Self::Gender => p.gender = x,
            Self::Breath => p.breath = x,
        }
    }
}

/// Every automatable / MIDI-linkable master knob on a Forge channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForgeParam {
    Size,
    Energy,
    Dirt,
    Space,
    Transpose,
    GainDb,
}

impl ForgeParam {
    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Size | Self::Energy | Self::Dirt | Self::Space => (0.0, 1.0),
            Self::Transpose => (-24.0, 24.0),
            Self::GainDb => (-36.0, 12.0),
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::Size | Self::Energy => 0.5,
            Self::Dirt | Self::Space | Self::Transpose | Self::GainDb => 0.0,
        }
    }

    pub fn get(self, p: &crate::forge::ForgePatch) -> f32 {
        match self {
            Self::Size => p.macros.size,
            Self::Energy => p.macros.energy,
            Self::Dirt => p.macros.dirt,
            Self::Space => p.macros.space,
            Self::Transpose => p.transpose,
            Self::GainDb => p.gain_db,
        }
    }

    pub fn set(self, p: &mut crate::forge::ForgePatch, value: f32) {
        let (lo, hi) = self.range();
        let v = value.clamp(lo, hi);
        match self {
            Self::Size => p.macros.size = v,
            Self::Energy => p.macros.energy = v,
            Self::Dirt => p.macros.dirt = v,
            Self::Space => p.macros.space = v,
            Self::Transpose => p.transpose = v,
            Self::GainDb => p.gain_db = v,
        }
    }
}

/// Helper: resolve `SynthFxParam` spec from `FX_KINDS[fx]`.
pub fn synth_fx_spec(fx: u8, param: u16) -> Option<&'static crate::fx::ParamSpec> {
    let kind = *crate::synth::FX_KINDS.get(fx as usize)?;
    kind.params().get(param as usize)
}

/// Helper: read `SynthFxParam` current value from `song`.
pub fn synth_fx_current(song: &Song, inst: u64, fx: u8, param: u16) -> Option<f32> {
    let kind = *crate::synth::FX_KINDS.get(fx as usize)?;
    let spec = kind.params().get(param as usize)?;
    let InstrumentKind::Synth { patch } = &song.instrument(inst)?.kind else {
        return None;
    };
    Some(
        patch
            .fx
            .iter()
            .find(|s| s.kind == kind)
            .and_then(|s| s.params.get(param as usize).copied())
            .unwrap_or(spec.default),
    )
}

/// Helper: write `SynthFxParam` value into `song` (ensuring the FX slot exists when edited).
pub fn synth_fx_set(song: &mut Song, inst: u64, fx: u8, param: u16, value: f32) -> bool {
    let Some(&kind) = crate::synth::FX_KINDS.get(fx as usize) else {
        return false;
    };
    let Some(spec) = kind.params().get(param as usize) else {
        return false;
    };
    let Some(InstrumentKind::Synth { patch }) = song
        .instruments
        .iter_mut()
        .find(|i| i.id == inst)
        .map(|i| &mut i.kind)
    else {
        return false;
    };
    let v = value.clamp(spec.min, spec.max);
    if let Some(slot) = patch.fx.iter_mut().find(|s| s.kind == kind) {
        if let Some(p) = slot.params.get_mut(param as usize) {
            *p = v;
            return true;
        }
        return false;
    }
    let mut slot = crate::fx::FxSlot::new(kind);
    if let Some(p) = slot.params.get_mut(param as usize) {
        *p = v;
    }
    patch.fx.push(slot);
    patch
        .fx
        .sort_by_key(|s| crate::synth::FX_KINDS.iter().position(|k| *k == s.kind));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{Envelope, Playback, Point};
    use crate::engine::Engine;
    use crate::forge::ForgeSound;
    use crate::sampler::{FilterKind, SamplePreset};
    use crate::sequence::{BAR, Clip, Note, Pattern, STEP};
    use crate::synth::SynthPatch;

    fn sample_song() -> (Song, u64, u64, u64, u64) {
        let mut song = Song::default();
        let smp = SamplePreset::from_sample("Test", 48_000, vec![[0.5, 0.5]; 4800]);
        let smp_id = song.add_channel(
            "Smp",
            InstrumentKind::Sampler {
                preset: Some(std::sync::Arc::new(smp)),
            },
        );
        let syn_id = song.add_channel(
            "Tide",
            InstrumentKind::Synth {
                patch: SynthPatch::factory(0),
            },
        );
        let voc_id = song.add_channel(
            "Voc",
            InstrumentKind::Vocal {
                voice: crate::vocal::Voice::new("", "Test"),
            },
        );
        let frg_id = song.add_channel(
            "Forge",
            InstrumentKind::Forge {
                sound: ForgeSound::new(crate::forge::recipes::Recipe::Jump.patch()),
            },
        );
        (song, smp_id, syn_id, voc_id, frg_id)
    }

    #[test]
    fn all_new_targets_roundtrip_range_current_and_set() {
        let (mut song, smp, syn, voc, frg) = sample_song();
        let cases: &[(Target, f32)] = &[
            (Target::Tempo, 145.5),
            (Target::Swing, 0.35),
            (Target::MixerSep { track: 1 }, -0.6),
            (Target::MixerEq { track: 1, band: 0 }, 6.5),
            (Target::MixerEq { track: 1, band: 2 }, -4.0),
            (
                Target::SamplerParam {
                    inst: smp,
                    zone: None,
                    param: SamplerParam::ProgVolume,
                },
                -6.0,
            ),
            (
                Target::SamplerParam {
                    inst: smp,
                    zone: Some(0),
                    param: SamplerParam::ZoneGain,
                },
                -12.0,
            ),
            (
                Target::SamplerParam {
                    inst: smp,
                    zone: Some(0),
                    param: SamplerParam::ZoneFilterCutoff(0),
                },
                1200.0,
            ),
            (
                Target::SynthParam {
                    inst: syn,
                    param: SynthParam::Cutoff,
                },
                850.0,
            ),
            (
                Target::SynthParam {
                    inst: syn,
                    param: SynthParam::Macro(0),
                },
                0.75,
            ),
            (
                Target::SynthFxParam {
                    inst: syn,
                    fx: 0,
                    param: 0,
                },
                12.0,
            ),
            (
                Target::VocalParam {
                    inst: voc,
                    param: VocalParam::Gender,
                },
                0.4,
            ),
            (
                Target::ForgeParam {
                    inst: frg,
                    param: ForgeParam::Size,
                },
                0.8,
            ),
        ];
        for &(target, val) in cases {
            assert!(target.exists(&song), "{target:?} should exist");
            let (lo, hi) = target.range(&song);
            assert!(hi > lo, "{target:?} range");
            // Log/linear unit round-trip.
            let u = target.to_unit(val, lo, hi);
            let back = target.from_unit(u, lo, hi);
            assert!(
                (back - val).abs() < 1e-2 * (hi - lo).max(1.0),
                "{target:?}: unit roundtrip {val} -> {u} -> {back}"
            );
            assert!(target.set(&mut song, val), "{target:?} set");
            let cur = target.current(&song).expect("current");
            assert!(
                (cur - val).abs() < 0.05,
                "{target:?}: got {cur}, want {val}"
            );
        }
    }

    #[test]
    fn tempo_automation_clip_drives_effective_tempo_map_and_song_length() {
        let mut song = Song::default();
        song.tempo = 120.0;
        let flat_frames = crate::sequence::loop_frames(&song, 1, true, 48_000);
        // Add a 1-bar clip automating Tempo to 240 BPM (twice as fast -> half the frames).
        let mut p = Pattern::new(50, "BPM");
        p.length = BAR;
        p.automation = Some(Envelope::flat(Target::Tempo, 240.0, BAR));
        song.patterns.push(p.into());
        song.clips.push(Clip::new(50, 0, 1));
        let fast_frames = crate::sequence::loop_frames(&song, 1, true, 48_000);
        assert!(
            (fast_frames as i64 - (flat_frames / 2) as i64).abs() < 256,
            "240 BPM clip should halve bar frames: flat={flat_frames}, fast={fast_frames}"
        );
        let map = crate::tempo::effective_tempo_map(&song).expect("tempo map");
        let table = crate::tempo::TempoTable::build(&map, song.tempo, 48_000);
        assert!((table.bpm_at(0) - 240.0).abs() < 0.1);
        assert!((table.bpm_at(BAR / 2) - 240.0).abs() < 0.1);
    }

    #[test]
    fn engine_applies_live_sampler_synth_and_mixer_sep_overrides() {
        let (mut song, smp, syn, _, _) = sample_song();
        // Put a note on the sampler and test ZoneGain override (-60 dB vs 0 dB).
        song.add_note(1, Note::new(smp, 0, BAR, 60, 127));
        let mut e_loud = Engine::new(48_000);
        let mut out_loud = vec![0.0f32; 512 * 2];
        e_loud.process(&song, 1, false, 0, &mut out_loud, true);
        let peak_loud = out_loud.iter().fold(0.0f32, |a, b| a.max(b.abs()));

        let mut e_quiet = Engine::new(48_000);
        e_quiet.apply_live_param(
            Target::SamplerParam {
                inst: smp,
                zone: Some(0),
                param: SamplerParam::ZoneGain,
            },
            -24.0,
        );
        let mut out_quiet = vec![0.0f32; 512 * 2];
        e_quiet.process(&song, 1, false, 0, &mut out_quiet, true);
        let peak_quiet = out_quiet.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            peak_loud > 0.1 && peak_quiet < peak_loud * 0.15,
            "ZoneGain override should attenuate sampler: loud={peak_loud}, quiet={peak_quiet}"
        );

        // Verify Sampler filter cutoff live override updates SVF filter cleanly.
        if let Some(InstrumentKind::Sampler { preset: Some(arc) }) = song
            .instruments
            .iter_mut()
            .find(|i| i.id == smp)
            .map(|i| &mut i.kind)
        {
            let p = std::sync::Arc::make_mut(arc);
            p.zones[0].params.filters[0].kind = FilterKind::Lowpass;
            p.zones[0].params.filters[0].cutoff = 20000.0;
        }
        let mut e_filt = Engine::new(48_000);
        e_filt.apply_live_param(
            Target::SamplerParam {
                inst: smp,
                zone: Some(0),
                param: SamplerParam::ZoneFilterCutoff(0),
            },
            40.0,
        );
        let mut out_filt = vec![0.0f32; 512 * 2];
        e_filt.process(&song, 1, false, 0, &mut out_filt, true);
        assert!(out_filt.iter().all(|s| s.is_finite()));

        // Verify SynthParam::Level live override mutes synth cleanly.
        let mut song2 = Song::default();
        let syn2 = song2.add_channel(
            "Tide",
            InstrumentKind::Synth {
                patch: SynthPatch::factory(0),
            },
        );
        song2.add_note(1, Note::new(syn2, 0, BAR, 60, 127));
        let mut es_on = Engine::new(48_000);
        let mut buf_on = vec![0.0f32; 512 * 2];
        es_on.process(&song2, 1, false, 0, &mut buf_on, true);
        let p_on = buf_on.iter().fold(0.0f32, |a, b| a.max(b.abs()));

        let mut es_off = Engine::new(48_000);
        es_off.apply_live_param(
            Target::SynthParam {
                inst: syn2,
                param: SynthParam::Level,
            },
            0.0,
        );
        let mut buf_off = vec![0.0f32; 512 * 2];
        es_off.process(&song2, 1, false, 0, &mut buf_off, true);
        let p_off = buf_off.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            p_on > 0.05 && p_off < p_on * 0.25,
            "SynthParam::Level override should lower synth output: on={p_on}, off={p_off}"
        );
        let _ = syn;
    }

    #[test]
    fn mixer_sep_and_eq_automation_tracks_shift_on_insert_and_remove() {
        let mut song = Song::default();
        let mut p1 = Pattern::new(60, "Sep");
        p1.length = BAR;
        p1.automation = Some(Envelope {
            target: Target::MixerSep { track: 2 },
            points: vec![Point {
                tick: 0,
                value: -1.0,
                tension: 0.0,
            }],
            playback: Playback::Clip,
        });
        let mut p2 = Pattern::new(61, "EQ");
        p2.length = BAR;
        p2.automation = Some(Envelope::flat(
            Target::MixerEq { track: 2, band: 1 },
            6.0,
            STEP,
        ));
        song.patterns.push(p1.into());
        song.patterns.push(p2.into());
        song.insert_track(2);
        assert_eq!(
            song.pattern(60)
                .unwrap()
                .automation
                .as_ref()
                .unwrap()
                .target,
            Target::MixerSep { track: 3 }
        );
        assert_eq!(
            song.pattern(61)
                .unwrap()
                .automation
                .as_ref()
                .unwrap()
                .target,
            Target::MixerEq { track: 3, band: 1 }
        );
        song.remove_track(3);
        assert!(song.pattern(60).unwrap().automation.is_none());
        assert!(song.pattern(61).unwrap().automation.is_none());
    }
}
