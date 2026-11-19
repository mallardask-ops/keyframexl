use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Raw bytes as base64 text. `Vec<u8>` serializes to a JSON integer array by
/// default (~4x overhead for a DWP); base64 is ~1.37x and much faster to
/// parse. Reads both forms so old projects keep opening.
mod b64_bytes {
    use super::*;

    pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        // Projects leave the original DWP out: the engine never reads it and
        // it doubled the file size (the decoded samples are saved anyway).
        if crate::project_io::blobs::writing() {
            return s.serialize_str("");
        }
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v.as_slice()))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Compact(String),
        Legacy(Vec<u8>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        use base64::Engine as _;
        match Repr::deserialize(d)? {
            Repr::Compact(s) => Ok(Arc::new(
                base64::engine::general_purpose::STANDARD
                    .decode(s.as_bytes())
                    .map_err(serde::de::Error::custom)?,
            )),
            Repr::Legacy(v) => Ok(Arc::new(v)),
        }
    }
}

/// Stereo f32 frames as base64 of little-endian bytes. The default (an array
/// of JSON numbers) costs ~10 bytes per float — minutes of samples turn a
/// project into hundreds of megabytes and seconds of saving. Base64 of the
/// raw bytes is ~5.4 chars per float and encodes in milliseconds. Reads both
/// forms so old projects keep opening.
pub(crate) mod b64_frames {
    use super::*;

    pub fn serialize<S: Serializer>(v: &Arc<Vec<[f32; 2]>>, s: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        // Inside a project file: a reference to a binary blob.
        if let Some(r) = crate::project_io::blobs::store(v) {
            return s.serialize_str(&r);
        }
        let mut bytes = Vec::with_capacity(v.len() * 8);
        for f in v.iter() {
            bytes.extend_from_slice(&f[0].to_le_bytes());
            bytes.extend_from_slice(&f[1].to_le_bytes());
        }
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&bytes))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Compact(String),
        Legacy(Vec<[f32; 2]>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<[f32; 2]>>, D::Error> {
        use base64::Engine as _;
        match Repr::deserialize(d)? {
            Repr::Legacy(v) => Ok(Arc::new(v)),
            Repr::Compact(s) if s.starts_with('@') => match crate::project_io::blobs::load(&s) {
                Some(r) => r.map_err(serde::de::Error::custom),
                None => Err(serde::de::Error::custom(
                    "sample data missing from the project",
                )),
            },
            Repr::Compact(s) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(s.as_bytes())
                    .map_err(serde::de::Error::custom)?;
                if bytes.len() % 8 != 0 {
                    return Err(serde::de::Error::custom("bad frame data length"));
                }
                let mut out = Vec::with_capacity(bytes.len() / 8);
                for c in bytes.chunks_exact(8) {
                    let l = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let r = f32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                    if !l.is_finite() || !r.is_finite() {
                        return Err(serde::de::Error::custom("bad sample data"));
                    }
                    out.push([l, r]);
                }
                Ok(Arc::new(out))
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Adsr {
    /// DAHDSR: delay and hold default to 0, so older ADSR documents load unchanged.
    pub delay: f64,
    pub attack: f64,
    pub hold: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
    /// Segment bends −1..1, 0 straight: + moves fast first, − slow first.
    #[serde(skip_serializing_if = "is_zero")]
    pub attack_curve: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub decay_curve: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub release_curve: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// How far along a segment the level is at progress `p` (0..1), bent by
/// `c` (−1..1): 0 straight, + fast first (exponential-like), − slow first.
pub fn bend(p: f64, c: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    if c == 0.0 {
        p
    } else {
        p.powf(2f64.powf(-3.0 * c.clamp(-1.0, 1.0)))
    }
}
impl Default for Adsr {
    fn default() -> Self {
        Self {
            delay: 0.0,
            attack: 0.005,
            hold: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.015,
            attack_curve: 0.0,
            decay_curve: 0.0,
            release_curve: 0.0,
        }
    }
}
impl Adsr {
    /// Envelope for modulation (filter, pitch): instant attack, full sustain.
    pub fn flat() -> Self {
        Self {
            attack: 0.0,
            release: 0.0,
            ..Self::default()
        }
    }
    fn held(&self, t: f64) -> f64 {
        let t = t - self.delay;
        if t < 0.0 {
            0.0
        } else if self.attack > 0.0 && t < self.attack {
            bend(t / self.attack, self.attack_curve)
        } else if t < self.attack + self.hold {
            1.0
        } else if self.decay > 0.0 && t < self.attack + self.hold + self.decay {
            1.0 + (self.sustain - 1.0)
                * bend((t - self.attack - self.hold) / self.decay, self.decay_curve)
        } else {
            self.sustain
        }
    }
    pub(crate) fn valid(&self) -> bool {
        [self.delay, self.attack, self.hold, self.decay, self.release]
            .iter()
            .all(|v| v.is_finite() && (0.0..=60.0).contains(v))
            && self.sustain.is_finite()
            && (0.0..=1.0).contains(&self.sustain)
            && [self.attack_curve, self.decay_curve, self.release_curve]
                .iter()
                .all(|c| (-1.0..=1.0).contains(c))
    }
    pub fn value(&self, t: f64, gate: f64) -> f64 {
        if t < gate {
            self.held(t)
        } else if self.release > 0.0 {
            self.held(gate) * (1.0 - bend((t - gate) / self.release, self.release_curve))
        } else {
            0.0
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    #[default]
    None,
    Continuous,
    Sustain,
    PingPong,
}
// ---------------------------------------------------------------- DirectWave-style zone parameters
// Names follow DirectWave (research_notes/directwave-params.md). Ranges are ours
// where the manual is silent; they are documented next to each field.

macro_rules! labelled_enum {
    ($name:ident { $($v:ident => $l:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            #[default]
            $($v),+
        }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];
            pub fn label(self) -> &'static str {
                match self { $($name::$v => $l),+ }
            }
        }
    };
}

labelled_enum!(FilterKind { Off => "Off", Lowpass => "Lowpass", Highpass => "Highpass", Bandpass => "Bandpass", Notch => "Notch", Allpass => "Allpass" });
labelled_enum!(FilterRouting { Serial => "Serial", Parallel => "Parallel" });
labelled_enum!(LfoShape { Sine => "Sine", AbsSine => "Abs Sine", Triangle => "Triangle", Square => "Square", Saw => "Saw", InvSaw => "Inv Saw", Random => "Random", LpRandom => "LP Random" });
labelled_enum!(ModSource { None => "—", Lfo1 => "LFO 1", Lfo2 => "LFO 2", Env1 => "ENV 1", Env2 => "ENV 2", Velocity => "Velocity", Key => "Key", Random => "Random" });
labelled_enum!(ModTarget { Pitch => "Pitch", Gain => "Gain", Pan => "Pan", Cutoff1 => "Filter 1 freq", Emphasis1 => "Filter 1 emph", Cutoff2 => "Filter 2 freq", Emphasis2 => "Filter 2 emph", Lfo1Rate => "LFO 1 rate", Lfo2Rate => "LFO 2 rate" });
labelled_enum!(PlayMode { Poly => "Poly", Mono => "Mono", Legato => "Legato" });
labelled_enum!(GlideMode { Fixed => "Fixed", Auto => "Auto" });

/// What 100 % of a matrix slot means for each target.
impl ModTarget {
    pub fn full_scale(self) -> f64 {
        match self {
            ModTarget::Pitch => 12.0,                       // semitones
            ModTarget::Gain => 1.0,                         // ±100 % gain
            ModTarget::Pan => 1.0,                          // full left/right
            ModTarget::Cutoff1 | ModTarget::Cutoff2 => 6.0, // octaves
            ModTarget::Emphasis1 | ModTarget::Emphasis2 => 1.0,
            ModTarget::Lfo1Rate | ModTarget::Lfo2Rate => 3.0, // octaves of rate
        }
    }
}

fn hash01(seed: u64) -> f64 {
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

impl LfoShape {
    /// Value in -1..1 (Abs Sine: 0..1) at a phase in cycles. Random shapes are
    /// deterministic per cycle so renders are reproducible.
    pub fn value(self, phase: f64) -> f64 {
        let x = phase.rem_euclid(1.0);
        let cycle = phase.floor() as i64 as u64;
        match self {
            LfoShape::Sine => (x * std::f64::consts::TAU).sin(),
            LfoShape::AbsSine => (x * std::f64::consts::PI).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (x - 0.5).abs(),
            LfoShape::Square => {
                if x < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::Saw => 2.0 * x - 1.0,
            LfoShape::InvSaw => 1.0 - 2.0 * x,
            LfoShape::Random => hash01(cycle) * 2.0 - 1.0,
            LfoShape::LpRandom => {
                // Smoothstep between successive random values.
                let (a, b) = (
                    hash01(cycle) * 2.0 - 1.0,
                    hash01(cycle.wrapping_add(1)) * 2.0 - 1.0,
                );
                let k = x * x * (3.0 - 2.0 * x);
                a + (b - a) * k
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Filter {
    pub kind: FilterKind,
    /// Frequency, Hz 20..20000.
    pub cutoff: f64,
    /// Emphasis (resonance), 0..1.
    pub emphasis: f64,
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            kind: FilterKind::Off,
            cutoff: 20000.0,
            emphasis: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Lfo {
    pub shape: LfoShape,
    /// Rate, Hz 0.01..50.
    pub rate: f64,
    /// Attack: seconds for the modulation to fade in, 0..10.
    pub attack: f64,
    /// Start phase, 0..1 of a cycle.
    pub phase: f64,
    /// Free Run: the LFO ignores note-on and runs on the song clock.
    pub free_run: bool,
}
impl Default for Lfo {
    fn default() -> Self {
        Self {
            shape: LfoShape::Sine,
            rate: 5.0,
            attack: 0.0,
            phase: 0.0,
            free_run: false,
        }
    }
}
impl Lfo {
    /// Output at voice time `t` (s) with `rate_mul` from the matrix; `clock` is
    /// song time (s) at the voice start, used by Free Run.
    pub fn at(&self, t: f64, clock: f64, rate_mul: f64) -> f64 {
        let fade = if self.attack > 0.0 {
            (t / self.attack).min(1.0)
        } else {
            1.0
        };
        let base = if self.free_run { clock + t } else { t };
        self.shape.value(self.phase + base * self.rate * rate_mul) * fade
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ModSlot {
    pub source: ModSource,
    pub target: ModTarget,
    /// -1..1 (DirectWave: -100 %..+100 %).
    pub amount: f64,
}

pub const MOD_SLOTS: usize = 16;

/// Sound shaping of one zone beyond its mapping (DirectWave Zone tab). Defaults change nothing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ZoneParams {
    pub filters: [Filter; 2],
    pub routing: FilterRouting,
    /// ENV 1 and ENV 2: free envelopes, routed through the matrix.
    pub env: [Adsr; 2],
    pub lfo: [Lfo; 2],
    pub matrix: [ModSlot; MOD_SLOTS],
    /// Keyboard Track (KTRK), -1..1: 1 = a semitone per key.
    pub key_track: f64,
    pub reverse: bool,
    /// Loop Type "One Shot": plays to the end whatever the note length.
    pub one_shot: bool,
    /// Mute group: a new voice silences voices of the same non-zero group.
    pub mute_group: u8,
}
impl Default for ZoneParams {
    fn default() -> Self {
        Self {
            filters: [Filter::default(), Filter::default()],
            routing: FilterRouting::Serial,
            env: [Adsr::flat(), Adsr::flat()],
            lfo: [Lfo::default(), Lfo::default()],
            matrix: [ModSlot::default(); MOD_SLOTS],
            key_track: 1.0,
            reverse: false,
            one_shot: false,
            mute_group: 0,
        }
    }
}
impl ZoneParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        let filters = self.filters.iter().all(|f| {
            f.cutoff.is_finite()
                && (20.0..=20000.0).contains(&f.cutoff)
                && f.emphasis.is_finite()
                && (0.0..=1.0).contains(&f.emphasis)
        });
        let lfos = self.lfo.iter().all(|l| {
            [l.rate, l.attack, l.phase].iter().all(|v| v.is_finite())
                && (0.01..=50.0).contains(&l.rate)
                && (0.0..=10.0).contains(&l.attack)
                && (0.0..=1.0).contains(&l.phase)
        });
        let matrix = self
            .matrix
            .iter()
            .all(|m| m.amount.is_finite() && (-1.0..=1.0).contains(&m.amount));
        if filters
            && lfos
            && matrix
            && self.env.iter().all(Adsr::valid)
            && self.key_track.is_finite()
            && (-1.0..=1.0).contains(&self.key_track)
        {
            Ok(())
        } else {
            Err("Invalid zone sound parameters")
        }
    }
    /// First empty matrix slot, for importers.
    pub fn route(&mut self, source: ModSource, target: ModTarget, amount: f64) -> bool {
        match self.matrix.iter_mut().find(|m| m.source == ModSource::None) {
            Some(slot) => {
                *slot = ModSlot {
                    source,
                    target,
                    amount: amount.clamp(-1.0, 1.0),
                };
                true
            }
            None => false,
        }
    }
}

/// Program-level settings (DirectWave Program tab): apply to every zone.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Program {
    /// Volume, dB -60..+12.
    pub volume_db: f64,
    /// -1..1, added to each zone's panning.
    pub pan: f64,
    /// Semitones, -48..48.
    pub transpose: i8,
    pub play_mode: PlayMode,
    /// Poly mode voice limit per note, 1..64.
    pub polyphony: u8,
    /// Glide time, seconds 0..10 (0 = off).
    pub glide: f64,
    pub glide_mode: GlideMode,
    /// PB Rng, cents. Stored for MIDI input.
    pub bend_range: f64,
}
impl Default for Program {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            transpose: 0,
            play_mode: PlayMode::Poly,
            polyphony: 32,
            glide: 0.0,
            glide_mode: GlideMode::Fixed,
            bend_range: 200.0,
        }
    }
}
impl Program {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(self.volume_db.is_finite() && (-60.0..=12.0).contains(&self.volume_db))
            || !(self.pan.is_finite() && (-1.0..=1.0).contains(&self.pan))
            || !(-48..=48).contains(&self.transpose)
            || !(1..=64).contains(&self.polyphony)
            || !(self.glide.is_finite() && (0.0..=10.0).contains(&self.glide))
            || !(self.bend_range.is_finite() && (0.0..=4800.0).contains(&self.bend_range))
        {
            return Err("Invalid program parameters");
        }
        Ok(())
    }
    pub fn gain(&self) -> f64 {
        10f64.powf(self.volume_db / 20.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub root: u8,
    pub low: u8,
    pub high: u8,
    pub low_velocity: u8,
    pub high_velocity: u8,
    pub sample_rate: u32,
    #[serde(with = "b64_frames")]
    pub frames: Arc<Vec<[f32; 2]>>,
    pub offset: usize,
    pub end: usize,
    pub loop_mode: LoopMode,
    pub loop_start: usize,
    pub loop_end: usize,
    pub adsr: Adsr,
    pub gain: f64,
    pub pan: f64,
    pub tune_cents: f64,
    pub velocity_tracking: f64,
    #[serde(default)]
    pub params: ZoneParams,
}
impl Zone {
    pub fn matches(&self, key: u8, velocity: u8) -> bool {
        key >= self.low
            && key <= self.high
            && velocity >= self.low_velocity
            && velocity <= self.high_velocity
            && velocity > 0
    }
    fn wrap(&self, raw: f64) -> f64 {
        let start = self.loop_start as f64;
        let end = self.loop_end as f64;
        if raw < start {
            return raw;
        }
        if self.loop_mode == LoopMode::PingPong {
            let span = (end - start - 1.0).max(1.0);
            let x = (raw - start) % (2.0 * span);
            start + if x <= span { x } else { 2.0 * span - x }
        } else if raw < end {
            raw
        } else {
            start + (raw - start) % (end - start)
        }
    }
    pub fn frame(
        &self,
        key: u8,
        velocity: u8,
        t: f64,
        gate: f64,
        phase_time: f64,
        gate_phase: f64,
    ) -> [f32; 2] {
        if !self.matches(key, velocity) || t < 0.0 || t >= gate + self.adsr.release {
            return [0.0; 2];
        }
        let ratio = 2.0_f64.powf((key as f64 - self.root as f64 + self.tune_cents / 100.0) / 12.0)
            * self.sample_rate as f64;
        let raw = self.offset as f64 + phase_time * ratio;
        let looping = self.loop_mode != LoopMode::None && self.loop_end > self.loop_start + 1;
        let index = if looping && self.loop_mode == LoopMode::Sustain && t >= gate {
            self.wrap(self.offset as f64 + gate_phase * ratio) + (phase_time - gate_phase) * ratio
        } else if looping {
            self.wrap(raw)
        } else {
            raw
        };
        if index < self.offset as f64 || index >= self.end as f64 {
            return [0.0; 2];
        }
        let i = index.floor() as usize;
        let fraction = (index - i as f64) as f32;
        let a = self.frames[i];
        let next = if looping
            && (t < gate || self.loop_mode != LoopMode::Sustain)
            && i + 1 >= self.loop_end
        {
            if self.loop_mode == LoopMode::PingPong {
                self.loop_end - 1
            } else {
                self.loop_start
            }
        } else {
            (i + 1).min(self.end - 1)
        };
        let b = self.frames[next];
        let velocity_gain =
            (1.0 - self.velocity_tracking) + self.velocity_tracking * (velocity as f64 / 127.0);
        let gain = self.adsr.value(t, gate) * self.gain * velocity_gain;
        [
            (a[0] + (b[0] - a[0]) * fraction) * (gain * (1.0 - self.pan.max(0.0))) as f32,
            (a[1] + (b[1] - a[1]) * fraction) * (gain * (1.0 + self.pan.min(0.0))) as f32,
        ]
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        self.validate_params()?;
        // The per-sample scan is linear in the audio data: loads and saves
        // only, never the per-frame knob path (knobs cannot touch samples).
        if self.frames.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Invalid sample parameters");
        }
        Ok(())
    }
    /// Everything `validate` checks except the per-sample scan: cheap enough
    /// to run on every knob tick while editing.
    pub fn validate_params(&self) -> Result<(), &'static str> {
        if self.low > self.high
            || self.high > 127
            || self.root > 127
            || self.low_velocity > self.high_velocity
            || self.high_velocity > 127
            || self.sample_rate == 0
            || self.frames.is_empty()
            || self.offset >= self.end
            || self.end > self.frames.len()
        {
            return Err("Invalid sample zone");
        }
        if self.loop_mode != LoopMode::None
            && (self.loop_start < self.offset
                || self.loop_end > self.end
                || self.loop_end <= self.loop_start + 1)
        {
            return Err("Invalid sample loop");
        }
        if !self.adsr.valid()
            || !self.gain.is_finite()
            || !(0.0..=16.0).contains(&self.gain)
            || !self.pan.is_finite()
            || !(-1.0..=1.0).contains(&self.pan)
            || !self.tune_cents.is_finite()
            || self.tune_cents.abs() > 9600.0
            || !self.velocity_tracking.is_finite()
            || !(0.0..=1.0).contains(&self.velocity_tracking)
        {
            return Err("Invalid sample parameters");
        }
        self.params.validate()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "PresetDocument")]
pub struct SamplePreset {
    pub name: String,
    pub program: Program,
    pub zones: Vec<Zone>,
    /// The original DWP bytes (save/portability only — the engine never reads
    /// them). Shared, so editing a knob does not copy megabytes per frame.
    #[serde(with = "b64_bytes")]
    pub original_dwp: Arc<Vec<u8>>,
    pub report: String,
}
#[derive(Deserialize)]
struct PresetDocument {
    name: String,
    #[serde(default)]
    program: Program,
    #[serde(default)]
    zones: Option<Vec<Zone>>,
    #[serde(default, with = "b64_bytes")]
    original_dwp: Arc<Vec<u8>>,
    #[serde(default)]
    report: String,
    root: Option<u8>,
    low: Option<u8>,
    high: Option<u8>,
    sample_rate: Option<u32>,
    samples: Option<Vec<f32>>,
}
impl TryFrom<PresetDocument> for SamplePreset {
    type Error = String;
    fn try_from(d: PresetDocument) -> Result<Self, String> {
        let zones = if let Some(z) = d.zones {
            z
        } else {
            let samples = d.samples.ok_or("Missing sample zones")?;
            let end = samples.len();
            vec![Zone {
                name: d.name.clone(),
                root: d.root.unwrap_or(60),
                low: d.low.unwrap_or(0),
                high: d.high.unwrap_or(127),
                low_velocity: 1,
                high_velocity: 127,
                sample_rate: d.sample_rate.unwrap_or(48000),
                frames: Arc::new(samples.into_iter().map(|v| [v, v]).collect()),
                offset: 0,
                end,
                loop_mode: LoopMode::None,
                loop_start: 0,
                loop_end: end,
                adsr: Adsr::default(),
                gain: 0.5,
                pan: 0.0,
                tune_cents: 0.0,
                velocity_tracking: 0.0,
                params: ZoneParams::default(),
            }]
        };
        let p = Self {
            name: d.name,
            program: d.program,
            zones,
            original_dwp: d.original_dwp,
            report: d.report,
        };
        p.validate().map_err(str::to_owned)?;
        Ok(p)
    }
}
impl SamplePreset {
    /// A valid one-zone preset with a short silent sample, for tests.
    #[doc(hidden)]
    pub fn test_preset(name: &str) -> Self {
        Self::from_sample(name, 48_000, vec![[0.0f32; 2]; 64])
    }

    /// One sample over the whole keyboard, root C5 (FL: the channel sampler's
    /// default), played to its end without a loop.
    pub fn from_sample(name: &str, sample_rate: u32, frames: Vec<[f32; 2]>) -> Self {
        let end = frames.len();
        Self {
            name: name.into(),
            program: Program::default(),
            zones: vec![Zone {
                name: "z".into(),
                root: 60,
                low: 0,
                high: 127,
                low_velocity: 1,
                high_velocity: 127,
                sample_rate,
                frames: std::sync::Arc::new(frames),
                offset: 0,
                end,
                loop_mode: LoopMode::None,
                loop_start: 0,
                loop_end: end,
                adsr: Adsr::default(),
                gain: 1.0,
                pan: 0.0,
                tune_cents: 0.0,
                velocity_tracking: 1.0,
                params: ZoneParams::default(),
            }],
            original_dwp: Arc::new(vec![]),
            report: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.zones.is_empty() {
            return Err("Preset has no zones");
        }
        self.program.validate()?;
        for z in &self.zones {
            z.validate()?
        }
        Ok(())
    }
    /// Everything `validate` checks except the per-sample scan: cheap enough
    /// to run on every knob tick while editing (knobs cannot touch samples).
    pub fn validate_params(&self) -> Result<(), &'static str> {
        if self.zones.is_empty() {
            return Err("Preset has no zones");
        }
        self.program.validate()?;
        for z in &self.zones {
            z.validate_params()?
        }
        Ok(())
    }
    pub fn release(&self) -> f64 {
        self.zones
            .iter()
            .map(|z| z.adsr.release)
            .fold(0.0, f64::max)
    }
    /// An empty-sample preset is not valid; this builds the program shell for UI defaults.
    pub fn with_program(mut self, program: Program) -> Self {
        self.program = program;
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_release_starts_from_current_level() {
        let a = Adsr {
            attack: 1.0,
            decay: 1.0,
            sustain: 0.25,
            release: 0.5,
            ..Adsr::default()
        };
        assert_eq!(a.value(0.25, 0.5), 0.25);
        assert_eq!(a.value(0.5, 0.5), 0.5);
        assert_eq!(a.value(0.75, 0.5), 0.25);
        assert_eq!(a.value(1.0, 0.5), 0.0);
        assert_eq!(a.value(1.5, 3.0), 0.625);
        // Delay and hold shift the shape.
        let d = Adsr {
            delay: 0.5,
            hold: 0.5,
            ..a.clone()
        };
        assert_eq!(d.value(0.25, 9.0), 0.0);
        assert_eq!(d.value(1.75, 9.0), 1.0);
        assert_eq!(d.value(2.5, 9.0), 0.625);
    }

    #[test]
    fn bent_segments_keep_their_ends_and_lean_the_way_asked() {
        let a = Adsr {
            attack: 1.0,
            decay: 1.0,
            sustain: 0.0,
            release: 1.0,
            attack_curve: 0.8,
            decay_curve: 0.8,
            release_curve: -0.8,
            ..Adsr::default()
        };
        assert!(
            (a.value(1.0, 9.0) - 1.0).abs() < 1e-12,
            "the attack still peaks at 1"
        );
        assert!(
            a.value(0.25, 9.0) > 0.5,
            "+ attack rises fast first: {}",
            a.value(0.25, 9.0)
        );
        assert!(
            a.value(1.25, 9.0) < 0.5,
            "+ decay falls fast first: {}",
            a.value(1.25, 9.0)
        );
        let r = Adsr {
            release_curve: -0.8,
            ..Adsr::default()
        };
        let r = Adsr { release: 1.0, ..r };
        assert!(
            r.value(9.25, 9.0) > 0.75,
            "− release falls slowly first: {}",
            r.value(9.25, 9.0)
        );
        assert_eq!(r.value(10.0, 9.0), 0.0);
        assert!(
            !serde_json::to_string(&Adsr::default())
                .unwrap()
                .contains("curve"),
            "straight segments are not written"
        );
    }
    #[test]
    fn legacy_single_zone_project_migrates() {
        let p:SamplePreset=serde_json::from_str(r#"{"name":"old","root":60,"low":48,"high":72,"sample_rate":48000,"samples":[0.0,0.5]}"#).unwrap();
        assert_eq!(p.zones.len(), 1);
        assert_eq!(p.zones[0].frames[1], [0.5, 0.5]);
    }
    #[test]
    fn samples_and_dwp_store_compact_but_old_arrays_still_load() {
        // 1 s of stereo detuned saw: 48k frames would be ~1 MB as JSON numbers.
        let frames: Vec<[f32; 2]> = (0..48_000)
            .map(|i| [(i as f32 * 0.01).sin(), (i as f32 * 0.013).cos()])
            .collect();
        let mut p = SamplePreset::from_sample("big", 48_000, frames);
        p.original_dwp = Arc::new(vec![7u8; 100_000]);
        let json = serde_json::to_string(&p).unwrap();
        // Base64 of 384 KB frames (~525 KB) + 100 KB DWP (~137 KB): well under 1 MB.
        assert!(
            json.len() < 900_000,
            "compact save, got {} bytes",
            json.len()
        );
        let back: SamplePreset = serde_json::from_slice(json.as_bytes()).unwrap();
        assert_eq!(back.zones[0].frames.as_ref(), p.zones[0].frames.as_ref());
        assert_eq!(back.original_dwp, p.original_dwp);
        // Legacy form (arrays of numbers) still opens.
        let legacy = serde_json::json!({
            "name": "old", "program": Program::default(),
            "zones": [{"name": "z", "root": 60, "low": 0, "high": 127,
                "low_velocity": 1, "high_velocity": 127, "sample_rate": 48000,
                "frames": [[0.25, -0.5], [0.0, 1.0]], "offset": 0, "end": 2,
                "loop_mode": "none", "loop_start": 0, "loop_end": 2,
                "adsr": Adsr::default(), "gain": 1.0, "pan": 0.0,
                "tune_cents": 0.0, "velocity_tracking": 1.0, "params": ZoneParams::default()}],
            "original_dwp": [1, 2, 3], "report": String::new(),
        });
        let old: SamplePreset = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            old.zones[0].frames.as_ref(),
            &vec![[0.25, -0.5], [0.0, 1.0]]
        );
        assert_eq!(old.original_dwp.as_slice(), &[1, 2, 3]);
    }
    #[test]
    fn forward_sustain_and_ping_pong_loops_use_frame_boundaries() {
        let mut z = Zone {
            name: "ramp".into(),
            root: 60,
            low: 0,
            high: 127,
            low_velocity: 1,
            high_velocity: 127,
            sample_rate: 8,
            frames: Arc::new((0..8).map(|i| [i as f32 / 10.0; 2]).collect()),
            offset: 0,
            end: 8,
            loop_mode: LoopMode::Continuous,
            loop_start: 2,
            loop_end: 6,
            adsr: Adsr {
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                release: 2.0,
                ..Adsr::default()
            },
            gain: 1.0,
            pan: 0.0,
            tune_cents: 0.0,
            velocity_tracking: 0.0,
            params: ZoneParams::default(),
        };
        z.validate().unwrap();
        assert!((z.frame(60, 100, 0.75, 2.0, 0.75, 2.0)[0] - 0.2).abs() < 1e-6);
        z.loop_mode = LoopMode::PingPong;
        assert!((z.frame(60, 100, 5.5 / 8.0, 2.0, 5.5 / 8.0, 2.0)[0] - 0.45).abs() < 1e-6);
        assert!((z.frame(60, 100, 6.0 / 8.0, 2.0, 6.0 / 8.0, 2.0)[0] - 0.4).abs() < 1e-6);
        z.loop_mode = LoopMode::Sustain;
        let v = z.frame(60, 100, 1.0, 0.75, 1.0, 0.75)[0];
        assert!((v - 0.4 * 0.875).abs() < 1e-6);
        assert_eq!(z.frame(60, 100, 1.5, 0.75, 1.5, 0.75), [0.0; 2]);
    }
}
