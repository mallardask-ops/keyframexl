//! Mixer effects. Each slot stores its kind and parameter values in the
//! project (`FxSlot`); the engine keeps the DSP state (`FxState`) per slot and
//! rebuilds it when the kind changes. Parameters are natural units (Hz, dB,
//! ms, %) listed by [`FxKind::params`]; see research_notes/mixer-research.md §6.

use serde::{Deserialize, Serialize};

mod balance;
mod bloom;
mod chorus;
mod color;
mod compressor;
mod convolution;
mod delay;
mod drive;
mod dsp;
mod eq;
mod fft;
mod filter;
mod hyper_chorus;
mod limiter;
mod mid_side;
mod motion;
mod multiband;
mod param_eq;
mod reverb;
mod squeeze;
mod stereo;
mod stereo_shaper;
mod time;
mod transient;
mod vocoder;
pub use convolution::{cached as ir_cached, install as ir_install};
pub mod presets;
pub use time::{HINTS as TIME_HINTS, MODES as TIME_MODES, STEPS as TIME_STEPS};

pub use dsp::Biquad;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FxKind {
    Eq,
    Compressor,
    Limiter,
    Delay,
    Reverb,
    Chorus,
    Drive,
    Filter,
    /// Seven bands (FL: Parametric EQ 2): low shelf, five peaks, high shelf.
    ParamEq,
    /// Width, balance and a Haas offset (FL: Stereo Shaper / Enhancer).
    Stereo,
    /// A hosted plugin (`FxSlot::plugin` says which); the engine runs it.
    Plugin,
    Bloom,
    AutoPan,
    Bitcrusher,
    NoiseGate,
    Phaser,
    RingMod,
    TiltEq,
    Tremolo,
    Wavefolder,
    /// Tempo-synced beat buffer (FL: Gross Beat, time side).
    Time,
    /// Attack/sustain shaping.
    Transient,
    /// Width, mono bass, mid and side levels.
    MidSide,
    /// Carrier takes the spectrum of the sidechain modulator.
    Vocoder,
    /// Three bands, each with its own threshold and ratio.
    Multiband,
    /// Convolution with a .wav impulse response (or the built-in studio IR).
    Convolution,
    /// Volume and a left/right balance (FL: Fruity Balance).
    Balance,
    /// Left/right matrix, delay and phase (FL: Fruity Stereo Shaper).
    StereoShaper,
    /// Bit loss, Puncher and a low-pass (FL: Fruity Squeeze).
    Squeeze,
    /// Four to eight modulated delay lines (FL: Hyper Chorus).
    HyperChorus,
    RoomReverb,
    StereoDelay,
    SoftClipper,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Hz,
    Db,
    Ms,
    Pct,
    Ratio,
    Steps,
    Plain,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub log: bool,
    pub unit: Unit,
}

pub(crate) const fn p(
    name: &'static str,
    min: f32,
    max: f32,
    default: f32,
    log: bool,
    unit: Unit,
) -> ParamSpec {
    ParamSpec {
        name,
        min,
        max,
        default,
        log,
        unit,
    }
}

impl FxKind {
    pub const ALL: [FxKind; BUILTINS.len()] = {
        let mut kinds = [Self::Eq; BUILTINS.len()];
        let mut i = 0;
        while i < kinds.len() {
            kinds[i] = BUILTINS[i].kind;
            i += 1;
        }
        kinds
    };

    /// Effects that can listen to another insert (FL: sidechain): the
    /// compressor's detector, or a hosted plugin's sidechain input.
    pub fn takes_sidechain(self) -> bool {
        matches!(self, Self::Compressor | Self::Plugin | Self::Vocoder)
    }

    pub fn label(self) -> &'static str {
        self.builtin().name
    }

    pub fn params(self) -> &'static [ParamSpec] {
        self.builtin().params
    }

    pub fn defaults(self) -> Vec<f32> {
        self.params().iter().map(|p| p.default).collect()
    }

    pub fn builtin(self) -> &'static Builtin {
        match self {
            Self::ParamEq => &BUILTINS[0],
            Self::Eq => &BUILTINS[1],
            Self::Compressor => &BUILTINS[2],
            Self::Limiter => &BUILTINS[3],
            Self::Delay => &BUILTINS[4],
            Self::Reverb => &BUILTINS[5],
            Self::Chorus => &BUILTINS[6],
            Self::Drive => &BUILTINS[7],
            Self::Filter => &BUILTINS[8],
            Self::Stereo => &BUILTINS[9],
            Self::Bloom => &BUILTINS[10],
            Self::AutoPan => &BUILTINS[11],
            Self::Bitcrusher => &BUILTINS[12],
            Self::NoiseGate => &BUILTINS[13],
            Self::Phaser => &BUILTINS[14],
            Self::RingMod => &BUILTINS[15],
            Self::TiltEq => &BUILTINS[16],
            Self::Tremolo => &BUILTINS[17],
            Self::Wavefolder => &BUILTINS[18],
            Self::Time => &BUILTINS[19],
            Self::Transient => &BUILTINS[20],
            Self::MidSide => &BUILTINS[21],
            Self::Vocoder => &BUILTINS[22],
            Self::Multiband => &BUILTINS[23],
            Self::Convolution => &BUILTINS[24],
            Self::Balance => &BUILTINS[25],
            Self::StereoShaper => &BUILTINS[26],
            Self::Squeeze => &BUILTINS[27],
            Self::HyperChorus => &BUILTINS[28],
            Self::RoomReverb => &BUILTINS[29],
            Self::StereoDelay => &BUILTINS[30],
            Self::SoftClipper => &BUILTINS[31],
            Self::Plugin => &HOSTED,
        }
    }
}

/// Format a parameter value for display.
pub fn format(spec: &ParamSpec, v: f32) -> String {
    match spec.unit {
        Unit::Hz if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
        Unit::Hz if v < 10.0 => format!("{v:.2} Hz"),
        Unit::Hz => format!("{v:.0} Hz"),
        Unit::Db => format!("{v:+.1} dB"),
        Unit::Ms => format!("{v:.1} ms"),
        Unit::Pct => format!("{v:.0}%"),
        Unit::Ratio => format!("{v:.1}:1"),
        Unit::Steps => format!(
            "{v:.0} step{}",
            if (v - 1.0).abs() < 0.5 { "" } else { "s" }
        ),
        Unit::Plain if spec.name == "Bits" => format!("{:.0} bit", v.round()),
        Unit::Plain if spec.name == "Mode" => {
            ["A", "B", "C", "D"][(v.round() as usize).min(3)].into()
        }
        Unit::Plain if spec.name == "Type" => {
            ["Lowpass", "Bandpass", "Highpass"][(v.round() as usize).min(2)].into()
        }
        Unit::Plain if spec.name.starts_with("Step ") => {
            time::MODES[(v.round() as usize).min(time::MODES.len() - 1)].into()
        }
        Unit::Plain if spec.name == "Bands" => format!("{:.0}", v.round()),
        Unit::Plain if spec.name == "Solo" => {
            ["Off", "Mid", "Side"][(v.round() as usize).min(2)].into()
        }
        Unit::Plain if spec.name == "Dephase" => format!("{v:.0}°"),
        Unit::Plain if spec.name == "Lines" => format!("{:.0}", v.round()),
        Unit::Plain if spec.name == "Relation" => {
            ["None", "Equal", "Double", "Half"][(v.round() as usize).min(3)].into()
        }
        Unit::Plain if spec.name == "Pre/Post" => {
            ["Pre", "Post"][(v.round() as usize).min(1)].into()
        }
        Unit::Plain => format!("{v:.2}"),
    }
}

/// One effect in an insert's chain (FL: an FX slot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FxSlot {
    /// The hosted plugin of a `Plugin` slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<crate::sequence::PluginRef>,
    pub kind: FxKind,
    pub params: Vec<f32>,
    /// The slot LED: off = bypassed.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Slot mix 0..1 (FL: the knob beside each slot).
    #[serde(default = "one")]
    pub mix: f32,
    /// Mixer insert whose output drives the detector (FL: sidechain input).
    /// Only effects with `FxKind::takes_sidechain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<u16>,
    /// The impulse response of a `Convolution` slot (a .wav file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<std::path::PathBuf>,
}

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

impl FxSlot {
    pub fn new(kind: FxKind) -> Self {
        Self {
            plugin: None,
            kind,
            params: kind.defaults(),
            enabled: true,
            mix: 1.0,
            sidechain: None,
            ir: None,
        }
    }
    /// A slot that runs a hosted plugin.
    pub fn plugin(plugin: crate::sequence::PluginRef) -> Self {
        Self {
            plugin: Some(plugin),
            kind: FxKind::Plugin,
            params: vec![],
            enabled: true,
            mix: 1.0,
            sidechain: None,
            ir: None,
        }
    }
    /// What the slot shows: the plugin's name or the built-in's.
    pub fn label(&self) -> &str {
        self.plugin
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or(self.kind.label())
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if (self.kind == FxKind::Plugin) != self.plugin.is_some() {
            return Err("Plugin slot without a plugin");
        }
        let spec = self.kind.params();
        if self.params.len() != spec.len() || !(0.0..=1.0).contains(&self.mix) {
            return Err("Bad effect slot");
        }
        if self
            .params
            .iter()
            .zip(spec)
            .any(|(v, s)| !v.is_finite() || *v < s.min || *v > s.max)
        {
            return Err("Effect parameter out of range");
        }
        if self.sidechain.is_some() && !self.kind.takes_sidechain() {
            return Err("This effect has no sidechain input");
        }
        if self.ir.is_some() && self.kind != FxKind::Convolution {
            return Err("Only the convolution reverb has an impulse response");
        }
        Ok(())
    }
}

pub(crate) fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Run context for one block.
pub struct Ctx {
    pub rate: f32,
    pub tempo: f32,
}

/// A built-in effect: the same shape a hosted plugin has (parameters in, stereo in place).
pub trait Effect: Send {
    /// Process interleaved stereo in place; `p` are the slot's parameter values in natural units.
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx);

    /// The most gain reduction of the last block in dB (≥ 0), for a meter.
    fn reduction(&self) -> f32 {
        0.0
    }

    /// Process with an outside signal (same layout as `buf`) driving the
    /// detector. Effects without a sidechain input ignore it.
    fn process_keyed(&mut self, p: &[f32], buf: &mut [f32], key: &[f32], ctx: &Ctx) {
        let _ = key;
        self.process(p, buf, ctx);
    }
}

/// Static description of a built-in effect (what a plugin browser lists).
pub struct Builtin {
    pub kind: FxKind,
    pub name: &'static str,
    pub vendor: &'static str,
    pub category: &'static str,
    pub params: &'static [ParamSpec],
    pub build: fn(f32) -> Box<dyn Effect>,
}

fn build_param_eq(rate: f32) -> Box<dyn Effect> {
    Box::new(param_eq::ParamEq::new(rate))
}
fn build_eq(rate: f32) -> Box<dyn Effect> {
    Box::new(eq::Eq::new(rate))
}
fn build_compressor(rate: f32) -> Box<dyn Effect> {
    Box::new(compressor::Compressor::new(rate))
}
fn build_limiter(rate: f32) -> Box<dyn Effect> {
    Box::new(limiter::Limiter::new(rate))
}
fn build_delay(rate: f32) -> Box<dyn Effect> {
    Box::new(delay::Delay::new(rate))
}
fn build_reverb(rate: f32) -> Box<dyn Effect> {
    Box::new(reverb::Reverb::new(rate))
}
fn build_chorus(rate: f32) -> Box<dyn Effect> {
    Box::new(chorus::Chorus::new(rate))
}
fn build_drive(rate: f32) -> Box<dyn Effect> {
    Box::new(drive::Drive::new(rate))
}
fn build_filter(rate: f32) -> Box<dyn Effect> {
    Box::new(filter::Filter::new(rate))
}
fn build_stereo(rate: f32) -> Box<dyn Effect> {
    Box::new(stereo::Stereo::new(rate))
}

pub const BUILTINS: [Builtin; 32] = [
    Builtin {
        kind: FxKind::ParamEq,
        name: "Parametric EQ 2",
        vendor: "KeyFrameXL",
        category: "EQ & filters",
        params: param_eq::PARAMS,
        build: build_param_eq,
    },
    Builtin {
        kind: FxKind::Eq,
        name: "3-band EQ",
        vendor: "KeyFrameXL",
        category: "EQ & filters",
        params: eq::PARAMS,
        build: build_eq,
    },
    Builtin {
        kind: FxKind::Compressor,
        name: "Compressor",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: compressor::PARAMS,
        build: build_compressor,
    },
    Builtin {
        kind: FxKind::Limiter,
        name: "Limiter",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: limiter::PARAMS,
        build: build_limiter,
    },
    Builtin {
        kind: FxKind::Delay,
        name: "Delay",
        vendor: "KeyFrameXL",
        category: "Delay & reverb",
        params: delay::PARAMS,
        build: build_delay,
    },
    Builtin {
        kind: FxKind::Reverb,
        name: "Reverb",
        vendor: "KeyFrameXL",
        category: "Delay & reverb",
        params: reverb::PARAMS,
        build: build_reverb,
    },
    Builtin {
        kind: FxKind::Chorus,
        name: "Chorus",
        vendor: "KeyFrameXL",
        category: "Modulation",
        params: chorus::PARAMS,
        build: build_chorus,
    },
    Builtin {
        kind: FxKind::Drive,
        name: "Drive",
        vendor: "KeyFrameXL",
        category: "Distortion",
        params: drive::PARAMS,
        build: build_drive,
    },
    Builtin {
        kind: FxKind::Filter,
        name: "Filter",
        vendor: "KeyFrameXL",
        category: "EQ & filters",
        params: filter::PARAMS,
        build: build_filter,
    },
    Builtin {
        kind: FxKind::Stereo,
        name: "Stereo",
        vendor: "KeyFrameXL",
        category: "Stereo",
        params: stereo::PARAMS,
        build: build_stereo,
    },
    Builtin {
        kind: FxKind::Bloom,
        name: "Bloom",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: bloom::PARAMS,
        build: |_| Box::new(bloom::Bloom::default()),
    },
    Builtin {
        kind: FxKind::AutoPan,
        name: "Auto Pan",
        vendor: "KeyFrameXL",
        category: "Stereo",
        params: motion::AUTOPAN,
        build: |_| Box::new(motion::AutoPan::default()),
    },
    Builtin {
        kind: FxKind::Bitcrusher,
        name: "Bitcrusher",
        vendor: "KeyFrameXL",
        category: "Distortion",
        params: color::CRUSH,
        build: |_| Box::new(color::Bitcrusher::default()),
    },
    Builtin {
        kind: FxKind::NoiseGate,
        name: "Noise Gate",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: color::GATE,
        build: |_| Box::new(color::NoiseGate::default()),
    },
    Builtin {
        kind: FxKind::Phaser,
        name: "Phaser",
        vendor: "KeyFrameXL",
        category: "Modulation",
        params: motion::PHASER,
        build: |_| Box::new(motion::Phaser::default()),
    },
    Builtin {
        kind: FxKind::RingMod,
        name: "Ring Modulator",
        vendor: "KeyFrameXL",
        category: "Modulation",
        params: motion::RING,
        build: |_| Box::new(motion::RingMod::default()),
    },
    Builtin {
        kind: FxKind::TiltEq,
        name: "Tilt EQ",
        vendor: "KeyFrameXL",
        category: "EQ & filters",
        params: color::TILT,
        build: |_| Box::new(color::TiltEq::default()),
    },
    Builtin {
        kind: FxKind::Tremolo,
        name: "Tremolo",
        vendor: "KeyFrameXL",
        category: "Modulation",
        params: motion::TREMOLO,
        build: |_| Box::new(motion::Tremolo::default()),
    },
    Builtin {
        kind: FxKind::Wavefolder,
        name: "Wavefolder",
        vendor: "KeyFrameXL",
        category: "Distortion",
        params: color::FOLD,
        build: |_| Box::new(color::Wavefolder::default()),
    },
    Builtin {
        kind: FxKind::Time,
        name: "Time",
        vendor: "KeyFrameXL",
        category: "Time",
        params: time::PARAMS,
        build: |rate| Box::new(time::Time::new(rate)),
    },
    Builtin {
        kind: FxKind::Transient,
        name: "Transient Shaper",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: transient::PARAMS,
        build: |rate| Box::new(transient::Transient::new(rate)),
    },
    Builtin {
        kind: FxKind::MidSide,
        name: "Mid / Side",
        vendor: "KeyFrameXL",
        category: "Stereo",
        params: mid_side::PARAMS,
        build: |rate| Box::new(mid_side::MidSide::new(rate)),
    },
    Builtin {
        kind: FxKind::Vocoder,
        name: "Vocoder",
        vendor: "KeyFrameXL",
        category: "Vocal",
        params: vocoder::PARAMS,
        build: |rate| Box::new(vocoder::Vocoder::new(rate)),
    },
    Builtin {
        kind: FxKind::Multiband,
        name: "Multiband Compressor",
        vendor: "KeyFrameXL",
        category: "Dynamics",
        params: multiband::PARAMS,
        build: |rate| Box::new(multiband::Multiband::new(rate)),
    },
    Builtin {
        kind: FxKind::Convolution,
        name: "Convolution Reverb",
        vendor: "KeyFrameXL",
        category: "Delay & reverb",
        params: convolution::PARAMS,
        build: |rate| Box::new(convolution::Convolution::new(rate, None)),
    },
    Builtin {
        kind: FxKind::Balance,
        name: "Balance",
        vendor: "KeyFrameXL",
        category: "Stereo",
        params: balance::PARAMS,
        build: |_| Box::new(balance::Balance),
    },
    Builtin {
        kind: FxKind::StereoShaper,
        name: "Stereo Shaper",
        vendor: "KeyFrameXL",
        category: "Stereo",
        params: stereo_shaper::PARAMS,
        build: |rate| Box::new(stereo_shaper::StereoShaper::new(rate)),
    },
    Builtin {
        kind: FxKind::Squeeze,
        name: "Squeeze",
        vendor: "KeyFrameXL",
        category: "Distortion",
        params: squeeze::PARAMS,
        build: |rate| Box::new(squeeze::Squeeze::new(rate)),
    },
    Builtin {
        kind: FxKind::HyperChorus,
        name: "Hyper Chorus",
        vendor: "KeyFrameXL",
        category: "Modulation",
        params: hyper_chorus::PARAMS,
        build: |rate| Box::new(hyper_chorus::HyperChorus::new(rate)),
    },
    Builtin {
        kind: FxKind::RoomReverb,
        name: "Room Reverb",
        vendor: "KeyFrameXL",
        category: "Delay & reverb",
        params: reverb::ROOM_PARAMS,
        build: |rate| Box::new(reverb::Reverb::room(rate)),
    },
    Builtin {
        kind: FxKind::StereoDelay,
        name: "Stereo Delay",
        vendor: "KeyFrameXL",
        category: "Delay & reverb",
        params: delay::STEREO_PARAMS,
        build: |rate| Box::new(delay::Delay::stereo(rate)),
    },
    Builtin {
        kind: FxKind::SoftClipper,
        name: "Soft Clipper",
        vendor: "KeyFrameXL",
        category: "Distortion",
        params: drive::CLIPPER_PARAMS,
        build: |_| Box::new(drive::SoftClipper),
    },
];

fn build_thru(_: f32) -> Box<dyn Effect> {
    Box::new(Thru)
}

/// Stand-in state for a plugin slot: the engine runs the hosted instance instead.
struct Thru;
impl Effect for Thru {
    fn process(&mut self, _: &[f32], _: &mut [f32], _: &Ctx) {}
}

/// What `FxKind::Plugin` describes (the slot's plugin carries the real name).
pub const HOSTED: Builtin = Builtin {
    kind: FxKind::Plugin,
    name: "Plugin",
    vendor: "",
    category: "Plugin",
    params: &[],
    build: build_thru,
};

/// Engine-side state of one slot.
pub struct FxState {
    kind: FxKind,
    fx: Box<dyn Effect>,
}

impl FxState {
    /// Build the state for `kind`. Allocates (delay lines); call outside tight loops.
    pub fn new(kind: FxKind, rate: f32) -> Self {
        let fx = (kind.builtin().build)(rate);
        Self { kind, fx }
    }

    /// Build the state for a slot: the convolution reverb takes the IR the UI
    /// decoded for it (a missing file falls back to the built-in studio IR).
    pub fn for_slot(slot: &FxSlot, rate: f32) -> Self {
        if slot.kind == FxKind::Convolution {
            let ir = slot.ir.as_deref().and_then(convolution::cached);
            Self {
                kind: slot.kind,
                fx: Box::new(convolution::Convolution::new(rate, ir)),
            }
        } else {
            Self::new(slot.kind, rate)
        }
    }

    pub fn kind(&self) -> FxKind {
        self.kind
    }

    /// Process interleaved stereo in place. `tempo` in BPM (tempo-synced delay).
    pub fn process(&mut self, p: &[f32], buf: &mut [f32], rate: f32, tempo: f32) {
        let ctx = Ctx { rate, tempo };
        self.fx.process(p, buf, &ctx);
    }

    /// `process`, with `key` driving the detector (see `Effect::process_keyed`).
    pub fn process_keyed(
        &mut self,
        p: &[f32],
        buf: &mut [f32],
        key: &[f32],
        rate: f32,
        tempo: f32,
    ) {
        let ctx = Ctx { rate, tempo };
        self.fx.process_keyed(p, buf, key, &ctx);
    }

    /// The most gain reduction of the last block in dB (dynamics only, else 0).
    pub fn reduction(&self) -> f32 {
        self.fx.reduction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * PI * hz * i as f32 / RATE).sin() * amp;
                [v, v]
            })
            .collect()
    }
    #[test]
    fn dynamics_report_their_gain_reduction() {
        let loud = sine(440.0, 1.0, 24_000);
        let quiet = sine(440.0, 0.01, 24_000);
        let reduction = |kind: FxKind, set: &dyn Fn(&mut Vec<f32>), input: &[f32]| {
            let mut p = kind.defaults();
            set(&mut p);
            let mut st = FxState::new(kind, RATE);
            let mut buf = input.to_vec();
            for chunk in buf.chunks_mut(512) {
                st.process(&p, chunk, RATE, 120.0);
            }
            st.reduction()
        };
        // Compressor: 0 dBFS into −24 dB at 8:1 settles near 21 dB of reduction.
        let comp = |p: &mut Vec<f32>| {
            p[0] = -24.0;
            p[1] = 8.0;
        };
        let gr = reduction(FxKind::Compressor, &comp, &loud);
        assert!((19.0..22.0).contains(&gr), "compressor GR {gr}");
        assert_eq!(
            reduction(FxKind::Compressor, &comp, &quiet),
            0.0,
            "below threshold: none"
        );
        // Limiter: +6 dB input into a −6 dB ceiling takes 12 dB off.
        let lim = |p: &mut Vec<f32>| {
            p[0] = 6.0;
            p[1] = -6.0;
        };
        let gr = reduction(FxKind::Limiter, &lim, &loud);
        assert!((11.5..12.5).contains(&gr), "limiter GR {gr}");
        assert_eq!(
            reduction(FxKind::Reverb, &|_| {}, &loud),
            0.0,
            "others report none"
        );
    }

    fn run(kind: FxKind, set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = kind.defaults();
        set(&mut p);
        let mut st = FxState::new(kind, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn a_convolution_slot_keeps_its_ir() {
        let mut slot = FxSlot::new(FxKind::Convolution);
        slot.ir = Some(std::path::PathBuf::from("/tmp/space.wav"));
        let json = serde_json::to_string(&slot).unwrap();
        let back: FxSlot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ir, slot.ir);
        back.validate().unwrap();
        let mut wrong = FxSlot::new(FxKind::Reverb);
        wrong.ir = slot.ir.clone();
        assert!(wrong.validate().is_err());
    }

    #[test]
    fn every_kind_has_valid_defaults() {
        for k in FxKind::ALL {
            FxSlot::new(k).validate().unwrap();
            let out = run(k, |_| {}, &sine(440.0, 0.5, 4800));
            assert!(out.iter().all(|x| x.is_finite()), "{k:?}");
        }
    }
}
