use crate::sampler::Adsr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Table {
    #[default]
    SineSaw,
    SquarePulse,
    Organ,
    Vowel,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Warp {
    #[default]
    Bend,
    Sync,
    PwmMirror,
    FmSub,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    #[default]
    LowPass,
    HighPass,
    BandPass,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Param {
    #[default]
    Position,
    Warp,
    Cutoff,
    Resonance,
    Sub,
    Noise,
    Detune,
    Spread,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModSource {
    Lfo1,
    Lfo2,
    FilterEnv,
    Velocity,
    Macro(u8),
    ModWheel,
    Pressure,
    Slide,
}
impl ModSource {
    pub const ALL: [Self; 11] = [
        Self::Lfo1,
        Self::Lfo2,
        Self::FilterEnv,
        Self::Velocity,
        Self::Macro(0),
        Self::Macro(1),
        Self::Macro(2),
        Self::Macro(3),
        Self::ModWheel,
        Self::Pressure,
        Self::Slide,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Lfo1 => "LFO 1",
            Self::Lfo2 => "LFO 2",
            Self::FilterEnv => "Filter Env",
            Self::Velocity => "Velocity",
            Self::Macro(0) => "Macro 1",
            Self::Macro(1) => "Macro 2",
            Self::Macro(2) => "Macro 3",
            Self::Macro(3) => "Macro 4",
            Self::Macro(_) => "Invalid Macro",
            Self::ModWheel => "Mod wheel",
            Self::Pressure => "Pressure",
            Self::Slide => "Slide (CC74)",
        }
    }
}

/// How many values `SynthPatch::apply_mods` takes (one per source).
pub const MOD_SOURCES: usize = 11;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModTarget {
    Position,
    Warp,
    Cutoff,
    Resonance,
    Sub,
    Noise,
    Detune,
    Spread,
    PositionB,
    WarpB,
    LevelB,
    PitchA,
    PitchB,
    LevelC,
    TiltC,
    OddC,
}
impl ModTarget {
    pub const ALL: [Self; 16] = [
        Self::Position,
        Self::Warp,
        Self::Cutoff,
        Self::Resonance,
        Self::Sub,
        Self::Noise,
        Self::Detune,
        Self::Spread,
        Self::PositionB,
        Self::WarpB,
        Self::LevelB,
        Self::PitchA,
        Self::PitchB,
        Self::LevelC,
        Self::TiltC,
        Self::OddC,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Position => "Position",
            Self::Warp => "Warp",
            Self::Cutoff => "Cutoff",
            Self::Resonance => "Resonance",
            Self::Sub => "Sub",
            Self::Noise => "Noise",
            Self::Detune => "Detune",
            Self::Spread => "Spread",
            Self::PositionB => "Position B",
            Self::WarpB => "Warp B",
            Self::LevelB => "Level B",
            Self::PitchA => "Pitch A",
            Self::PitchB => "Pitch B",
            Self::LevelC => "Level C",
            Self::TiltC => "Tilt C",
            Self::OddC => "Odd/even C",
        }
    }
    pub fn from_param(p: Param) -> Self {
        match p {
            Param::Position => Self::Position,
            Param::Warp => Self::Warp,
            Param::Cutoff => Self::Cutoff,
            Param::Resonance => Self::Resonance,
            Param::Sub => Self::Sub,
            Param::Noise => Self::Noise,
            Param::Detune => Self::Detune,
            Param::Spread => Self::Spread,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModRoute {
    pub source: ModSource,
    pub target: ModTarget,
    pub depth: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LfoShape {
    #[default]
    Off,
    Sine,
    Triangle,
    Saw,
    Square,
    SampleAndHold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LfoDivision {
    Whole,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl LfoDivision {
    pub fn cycles_per_beat(self) -> f64 {
        match self {
            Self::Whole => 0.25,
            Self::Half => 0.5,
            Self::Quarter => 1.0,
            Self::Eighth => 2.0,
            Self::Sixteenth => 4.0,
            Self::ThirtySecond => 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lfo {
    pub shape: LfoShape,
    pub rate: f64,
    pub sync: bool,
    pub division: LfoDivision,
    pub retrigger: bool,
}

impl Lfo {
    pub fn value_at(self, phase: f64) -> f64 {
        let phase = phase.rem_euclid(1.0);
        match self.shape {
            LfoShape::Off | LfoShape::SampleAndHold => 0.0,
            LfoShape::Sine => (phase * std::f64::consts::TAU).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            LfoShape::Saw => phase * 2.0 - 1.0,
            LfoShape::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
    /// The LFO at `t` seconds into a note: retriggered LFOs start each note at
    /// phase 0, free ones follow the song clock (`clock`: when the note began).
    /// Sample-and-hold takes a fixed random value per cycle (`seed` per voice).
    pub fn at(self, t: f64, clock: f64, tempo: f64, seed: u64) -> f64 {
        if self.shape == LfoShape::Off {
            return 0.0;
        }
        let phase = self.frequency(tempo) * if self.retrigger { t } else { clock + t };
        if self.shape == LfoShape::SampleAndHold {
            let mut h = (phase.floor() as i64 as u64 ^ seed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= h >> 29;
            return (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
        }
        self.value_at(phase)
    }

    pub fn frequency(self, tempo: f64) -> f64 {
        if self.shape == LfoShape::Off {
            0.0
        } else if self.sync {
            tempo.max(0.0) / 60.0 * self.division.cycles_per_beat()
        } else {
            self.rate.clamp(0.01, 30.0)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LfoState {
    phase: f64,
    random: u32,
    held: f64,
}

impl Default for LfoState {
    fn default() -> Self {
        Self {
            phase: 0.0,
            random: 0x6d2b79f5,
            held: 0.0,
        }
    }
}

impl LfoState {
    pub fn tick(&mut self, lfo: Lfo, tempo: f64, sample_rate: f64) -> f64 {
        let value = match lfo.shape {
            LfoShape::Off => 0.0,
            LfoShape::Sine => (self.phase * std::f64::consts::TAU).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
            LfoShape::Saw => self.phase * 2.0 - 1.0,
            LfoShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => self.held,
        };
        let phase = self.phase + lfo.frequency(tempo) / sample_rate;
        if phase >= 1.0 {
            self.random ^= self.random << 13;
            self.random ^= self.random >> 17;
            self.random ^= self.random << 5;
            self.held = self.random as f64 / u32::MAX as f64 * 2.0 - 1.0;
        }
        self.phase = phase.fract();
        value
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Route {
    pub target: Param,
    pub depth: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Macro {
    pub value: f64,
    pub routes: [Route; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Oscillator {
    pub table: Table,
    pub position: f64,
    pub warp: Warp,
    pub warp_amount: f64,
    pub level: f64,
    pub coarse: f64,
    pub fine: f64,
}
impl Default for Oscillator {
    fn default() -> Self {
        Self {
            table: Table::default(),
            position: 0.0,
            warp: Warp::default(),
            warp_amount: 0.0,
            level: 0.0,
            coarse: 0.0,
            fine: 0.0,
        }
    }
}
/// Most partials the spectral oscillator sums.
pub const MAX_PARTIALS: usize = 32;

/// Oscillator C: additive partials (harmonics of the note) shaped by a few
/// controls instead of a stored table. Off at level 0 (older patches).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spectral {
    pub level: f64,
    /// Semitones from the note.
    pub coarse: f64,
    /// How many harmonics, 1..=32 (fewer above the band limit).
    pub partials: u8,
    /// −1 dark (1/k²) · 0 a saw's 1/k · +1 every partial equal.
    pub tilt: f64,
    /// −1 even harmonics only · 0 all · +1 odd only (hollow, square-like).
    pub odd: f64,
    /// 0..1: each partial swells and fades on its own slow cycle.
    pub shimmer: f64,
}

impl Default for Spectral {
    fn default() -> Self {
        Self {
            level: 0.0,
            coarse: 0.0,
            partials: 16,
            tilt: 0.0,
            odd: 0.0,
            shimmer: 0.0,
        }
    }
}

impl Spectral {
    /// Partial amplitudes for a note at `freq` (only those under 0.45 × `rate`),
    /// `t` seconds into it, normalized to an even loudness. Returns how many
    /// of `out` are used. No allocation (the audio thread calls it).
    pub fn amplitudes(&self, freq: f64, rate: f64, t: f64, out: &mut [f64; MAX_PARTIALS]) -> usize {
        let below = if freq > 0.0 {
            (0.45 * rate / freq) as usize
        } else {
            MAX_PARTIALS
        };
        let n = (self.partials as usize)
            .clamp(1, MAX_PARTIALS)
            .min(below.max(1));
        let slope = 1.0 - self.tilt.clamp(-1.0, 1.0);
        let mut power = 0.0;
        for (i, a) in out.iter_mut().enumerate().take(n) {
            let k = (i + 1) as f64;
            let mut v = k.powf(-slope);
            if i > 0 {
                let even = (i + 1) % 2 == 0;
                v *= if even {
                    1.0 - self.odd.max(0.0)
                } else {
                    1.0 + self.odd.min(0.0)
                };
            }
            if self.shimmer > 0.0 {
                // A slow, different cycle per partial (0.13 Hz upward, golden steps).
                let rate_k = 0.13 + 0.618_034 * k % 1.7;
                let swell = 0.5 + 0.5 * (std::f64::consts::TAU * rate_k * t + k * 2.4).sin();
                v *= 1.0 - self.shimmer.clamp(0.0, 1.0) * swell;
            }
            *a = v;
            power += v * v;
        }
        // An even RMS (a saw's) whatever the shape.
        let gain = if power > 1e-12 {
            0.45 / (power / 2.0).sqrt()
        } else {
            0.0
        };
        for a in out.iter_mut().take(n) {
            *a *= gain;
        }
        n
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthPatch {
    pub table: Table,
    pub level: f64,
    pub coarse: f64,
    pub fine: f64,
    pub osc_b: Oscillator,
    /// Oscillator C, additive (off at level 0).
    pub spectral: Spectral,
    pub position: f64,
    pub warp: Warp,
    pub warp_amount: f64,
    pub sub: f64,
    pub noise: f64,
    pub unison: u8,
    pub detune: f64,
    pub spread: f64,
    pub filter: Filter,
    pub cutoff: f64,
    pub resonance: f64,
    pub keytrack: f64,
    pub env_amount: f64,
    pub amp_env: Adsr,
    pub filter_env: Adsr,
    pub glide: f64,
    /// Pitch bend range in semitones (MIDI pitch wheel), 0..=48.
    pub bend_range: f64,
    /// MPE: channel 1 bends every note, channels 2-16 each their own note
    /// (±48 semitones), with pressure and slide per note.
    pub mpe: bool,
    pub macros: [Macro; 4],
    pub lfos: [Lfo; 2],
    pub mods: Vec<ModRoute>,
    /// The synth's own effects after the voices, in `FX_KINDS` order
    /// (FL natives have them too). Empty: none, and no extra work.
    pub fx: Vec<crate::fx::FxSlot>,
}

/// A built-in effect at its defaults with some parameters set (factory presets).
fn fx(kind: crate::fx::FxKind, set: &[(usize, f32)]) -> crate::fx::FxSlot {
    let mut slot = crate::fx::FxSlot::new(kind);
    for &(i, v) in set {
        slot.params[i] = v;
    }
    slot
}

/// The effects a synth patch may carry, in chain order.
pub const FX_KINDS: [crate::fx::FxKind; 4] = [
    crate::fx::FxKind::Drive,
    crate::fx::FxKind::Chorus,
    crate::fx::FxKind::Delay,
    crate::fx::FxKind::Reverb,
];

impl Default for SynthPatch {
    fn default() -> Self {
        Self {
            table: Table::SineSaw,
            level: 1.0,
            coarse: 0.0,
            fine: 0.0,
            osc_b: Oscillator::default(),
            spectral: Spectral::default(),
            position: 0.0,
            warp: Warp::Bend,
            warp_amount: 0.0,
            sub: 0.0,
            noise: 0.0,
            unison: 1,
            detune: 0.0,
            spread: 0.0,
            filter: Filter::LowPass,
            cutoff: 18000.0,
            resonance: 0.0,
            keytrack: 0.0,
            env_amount: 0.0,
            amp_env: Adsr::default(),
            filter_env: Adsr::default(),
            glide: 0.0,
            bend_range: 2.0,
            mpe: false,
            macros: [Macro::default(); 4],
            lfos: [Lfo::default(); 2],
            mods: Vec::new(),
            fx: Vec::new(),
        }
    }
}
impl SynthPatch {
    pub fn routes_to(&self, target: ModTarget) -> impl Iterator<Item = &ModRoute> {
        self.mods.iter().filter(move |r| r.target == target)
    }
    pub const PRESETS: [&'static str; 18] = [
        "Init",
        "Soft Pad",
        "Pluck",
        "Bass",
        "Lead",
        "Keys",
        "Wobble-free Sub",
        "Bell",
        "Wide Pad",
        "Echo Pluck",
        "Grit Bass",
        "Glass Keys",
        "Wobble Bass",
        "Space Lead",
        "Shimmer Bell",
        "Drive Organ",
        "Spectral Organ",
        "Hollow Choir",
    ];
    pub fn factory(index: usize) -> Self {
        use crate::fx::FxKind;
        let mut p = Self::default();
        match index {
            1 => {
                p.table = Table::Organ;
                p.position = 0.55;
                p.unison = 5;
                p.detune = 0.12;
                p.spread = 0.8;
                p.amp_env.attack = 0.6;
                p.amp_env.release = 1.8;
                p.cutoff = 4500.0;
            }
            2 => {
                p.position = 0.8;
                p.amp_env.decay = 0.25;
                p.amp_env.sustain = 0.0;
                p.amp_env.release = 0.15;
                p.cutoff = 4800.0;
            }
            3 => {
                p.position = 0.7;
                p.sub = 0.7;
                p.cutoff = 800.0;
                p.filter_env.decay = 0.25;
                p.filter_env.sustain = 0.0;
                p.env_amount = 0.5;
            }
            4 => {
                p.position = 0.9;
                p.unison = 3;
                p.detune = 0.08;
                p.spread = 0.65;
                p.cutoff = 8500.0;
            }
            5 => {
                p.table = Table::Organ;
                p.position = 0.8;
                p.amp_env.decay = 0.5;
                p.amp_env.sustain = 0.45;
                p.amp_env.release = 0.35;
            }
            6 => {
                p.sub = 0.9;
                p.cutoff = 350.0;
            }
            7 => {
                p.table = Table::Vowel;
                p.position = 0.75;
                p.amp_env.decay = 1.5;
                p.amp_env.sustain = 0.0;
                p.amp_env.release = 1.0;
            }
            8 => {
                p = Self::factory(1);
                p.osc_b = Oscillator {
                    table: Table::SineSaw,
                    position: 0.6,
                    level: 0.5,
                    fine: 7.0,
                    ..p.osc_b
                };
                p.lfos[0] = Lfo {
                    shape: LfoShape::Triangle,
                    rate: 0.15,
                    ..Lfo::default()
                };
                p.mods.push(ModRoute {
                    source: ModSource::Lfo1,
                    target: ModTarget::Position,
                    depth: 0.2,
                });
                p.fx = vec![
                    fx(FxKind::Chorus, &[(1, 60.0), (3, 50.0)]),
                    fx(FxKind::Reverb, &[(0, 85.0), (4, 35.0)]),
                ];
            }
            9 => {
                p = Self::factory(2);
                p.fx = vec![
                    fx(FxKind::Delay, &[(0, 3.0), (1, 45.0), (4, 35.0)]),
                    fx(FxKind::Reverb, &[(0, 40.0), (4, 15.0)]),
                ];
            }
            10 => {
                p = Self::factory(3);
                p.resonance = 0.35;
                p.fx = vec![fx(FxKind::Drive, &[(0, 18.0), (1, 3500.0), (2, -8.0)])];
            }
            11 => {
                p = Self::factory(5);
                p.table = Table::Vowel;
                p.position = 0.35;
                p.fx = vec![
                    fx(FxKind::Chorus, &[(1, 35.0), (3, 40.0)]),
                    fx(FxKind::Reverb, &[(0, 55.0), (4, 22.0)]),
                ];
            }
            12 => {
                p = Self::factory(3);
                p.cutoff = 600.0;
                p.resonance = 0.45;
                p.env_amount = 0.0;
                p.lfos[0] = Lfo {
                    shape: LfoShape::Sine,
                    sync: true,
                    division: LfoDivision::Eighth,
                    retrigger: true,
                    ..Lfo::default()
                };
                p.mods.push(ModRoute {
                    source: ModSource::Lfo1,
                    target: ModTarget::Cutoff,
                    depth: 0.6,
                });
                p.fx = vec![fx(FxKind::Drive, &[(0, 10.0), (2, -4.0)])];
            }
            13 => {
                p = Self::factory(4);
                p.glide = 0.06;
                p.fx = vec![
                    fx(FxKind::Delay, &[(0, 6.0), (1, 55.0), (4, 30.0)]),
                    fx(FxKind::Reverb, &[(0, 75.0), (4, 25.0)]),
                ];
            }
            14 => {
                p = Self::factory(7);
                p.osc_b = Oscillator {
                    table: Table::SineSaw,
                    position: 0.0,
                    level: 0.35,
                    coarse: 12.0,
                    ..p.osc_b
                };
                p.fx = vec![
                    fx(FxKind::Chorus, &[(1, 30.0), (3, 30.0)]),
                    fx(FxKind::Delay, &[(0, 4.0), (1, 50.0), (4, 30.0)]),
                    fx(FxKind::Reverb, &[(0, 95.0), (4, 40.0)]),
                ];
            }
            16 => {
                // Additive drawbars: the spectral oscillator alone, odd-heavy, a slow shimmer.
                p.level = 0.0;
                p.spectral = Spectral {
                    level: 1.0,
                    coarse: 0.0,
                    partials: 12,
                    tilt: 0.35,
                    odd: 0.4,
                    shimmer: 0.25,
                };
                p.amp_env.attack = 0.01;
                p.amp_env.sustain = 0.95;
                p.amp_env.release = 0.12;
                p.fx = vec![
                    fx(FxKind::Chorus, &[(1, 40.0), (3, 35.0)]),
                    fx(FxKind::Reverb, &[(0, 50.0), (4, 20.0)]),
                ];
            }
            17 => {
                // Even partials over a soft pad, the tilt breathing with LFO 1.
                p.table = Table::Vowel;
                p.position = 0.4;
                p.level = 0.5;
                p.spectral = Spectral {
                    level: 0.8,
                    coarse: 12.0,
                    partials: 24,
                    tilt: -0.2,
                    odd: -0.6,
                    shimmer: 0.6,
                };
                p.cutoff = 6000.0;
                p.amp_env.attack = 0.5;
                p.amp_env.release = 1.2;
                p.lfos[0] = Lfo {
                    shape: LfoShape::Sine,
                    rate: 0.2,
                    ..Lfo::default()
                };
                p.mods.push(ModRoute {
                    source: ModSource::Lfo1,
                    target: ModTarget::TiltC,
                    depth: 0.4,
                });
                p.fx = vec![
                    fx(FxKind::Chorus, &[(1, 35.0), (3, 40.0)]),
                    fx(FxKind::Reverb, &[(0, 90.0), (4, 40.0)]),
                ];
            }
            15 => {
                p = Self::factory(5);
                p.amp_env.sustain = 0.9;
                p.fx = vec![
                    fx(
                        FxKind::Drive,
                        &[(0, 14.0), (1, 5000.0), (2, -7.0), (3, 70.0)],
                    ),
                    fx(FxKind::Chorus, &[(1, 50.0), (2, 0.9), (3, 45.0)]),
                ];
            }
            _ => {}
        }
        p
    }
    pub fn modulated(&self) -> Self {
        let mut p = self.clone();
        self.apply_macros(&mut p);
        p
    }
    pub(crate) fn modulated_for_render(&self) -> Self {
        let mut p = Self {
            table: self.table,
            level: self.level,
            coarse: self.coarse,
            fine: self.fine,
            osc_b: self.osc_b,
            spectral: self.spectral,
            position: self.position,
            warp: self.warp,
            warp_amount: self.warp_amount,
            sub: self.sub,
            noise: self.noise,
            unison: self.unison,
            detune: self.detune,
            spread: self.spread,
            filter: self.filter,
            cutoff: self.cutoff,
            resonance: self.resonance,
            keytrack: self.keytrack,
            env_amount: self.env_amount,
            amp_env: self.amp_env.clone(),
            filter_env: self.filter_env.clone(),
            glide: self.glide,
            bend_range: self.bend_range,
            mpe: self.mpe,
            macros: self.macros,
            lfos: self.lfos,
            mods: Vec::new(),
            fx: Vec::new(),
        };
        self.apply_macros(&mut p);
        p
    }
    /// Index of a matrix source in the values `apply_mods` takes.
    pub fn source_index(s: ModSource) -> usize {
        match s {
            ModSource::Lfo1 => 0,
            ModSource::Lfo2 => 1,
            ModSource::FilterEnv => 2,
            ModSource::Velocity => 3,
            ModSource::Macro(m) => 4 + m.min(3) as usize,
            ModSource::ModWheel => 8,
            ModSource::Pressure => 9,
            ModSource::Slide => 10,
        }
    }

    /// The modulation matrix on top of `p` (this patch after macros), given the
    /// source values in `source_index` order. No allocation.
    pub fn apply_mods(&self, p: &mut Self, src: &[f64; MOD_SOURCES]) {
        for r in &self.mods {
            let d = src[Self::source_index(r.source)] * r.depth;
            match r.target {
                ModTarget::Position => p.position = (p.position + d).clamp(0.0, 1.0),
                ModTarget::Warp => p.warp_amount = (p.warp_amount + d).clamp(-1.0, 1.0),
                ModTarget::Cutoff => {
                    p.cutoff = (p.cutoff * 2.0_f64.powf(d * 8.0)).clamp(20.0, 20000.0)
                }
                ModTarget::Resonance => p.resonance = (p.resonance + d).clamp(0.0, 1.0),
                ModTarget::Sub => p.sub = (p.sub + d).clamp(0.0, 1.0),
                ModTarget::Noise => p.noise = (p.noise + d).clamp(0.0, 1.0),
                ModTarget::Detune => p.detune = (p.detune + d).clamp(0.0, 1.0),
                ModTarget::Spread => p.spread = (p.spread + d).clamp(0.0, 1.0),
                ModTarget::PositionB => p.osc_b.position = (p.osc_b.position + d).clamp(0.0, 1.0),
                ModTarget::WarpB => {
                    p.osc_b.warp_amount = (p.osc_b.warp_amount + d).clamp(-1.0, 1.0)
                }
                ModTarget::LevelB => p.osc_b.level = (p.osc_b.level + d).clamp(0.0, 1.0),
                ModTarget::PitchA => p.coarse += d * 24.0,
                ModTarget::PitchB => p.osc_b.coarse += d * 24.0,
                ModTarget::LevelC => p.spectral.level = (p.spectral.level + d).clamp(0.0, 1.0),
                ModTarget::TiltC => p.spectral.tilt = (p.spectral.tilt + d).clamp(-1.0, 1.0),
                ModTarget::OddC => p.spectral.odd = (p.spectral.odd + d).clamp(-1.0, 1.0),
            }
        }
    }

    fn apply_macros(&self, p: &mut Self) {
        for m in &self.macros {
            for r in &m.routes {
                let d = m.value * r.depth;
                match r.target {
                    Param::Position => p.position = (p.position + d).clamp(0.0, 1.0),
                    Param::Warp => p.warp_amount = (p.warp_amount + d).clamp(-1.0, 1.0),
                    Param::Cutoff => {
                        p.cutoff = (p.cutoff * 2.0_f64.powf(d * 8.0)).clamp(20.0, 20000.0)
                    }
                    Param::Resonance => p.resonance = (p.resonance + d).clamp(0.0, 1.0),
                    Param::Sub => p.sub = (p.sub + d).clamp(0.0, 1.0),
                    Param::Noise => p.noise = (p.noise + d).clamp(0.0, 1.0),
                    Param::Detune => p.detune = (p.detune + d).clamp(0.0, 1.0),
                    Param::Spread => p.spread = (p.spread + d).clamp(0.0, 1.0),
                }
            }
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let unit = |x: f64| (0.0..=1.0).contains(&x);
        let bipolar = |x: f64| (-1.0..=1.0).contains(&x);
        let env = |e: &Adsr| e.valid();
        if !unit(self.level)
            || !(-24.0..=24.0).contains(&self.coarse)
            || !(-100.0..=100.0).contains(&self.fine)
            || !unit(self.osc_b.level)
            || !unit(self.osc_b.position)
            || !bipolar(self.osc_b.warp_amount)
            || !(-24.0..=24.0).contains(&self.osc_b.coarse)
            || !(-100.0..=100.0).contains(&self.osc_b.fine)
            || !unit(self.position)
            || !unit(self.spectral.level)
            || !(-24.0..=24.0).contains(&self.spectral.coarse)
            || !(1..=MAX_PARTIALS as u8).contains(&self.spectral.partials)
            || !bipolar(self.spectral.tilt)
            || !bipolar(self.spectral.odd)
            || !unit(self.spectral.shimmer)
            || !bipolar(self.warp_amount)
            || !unit(self.sub)
            || !unit(self.noise)
            || !(1..=7).contains(&self.unison)
            || !unit(self.detune)
            || !unit(self.spread)
            || !(20.0..=20000.0).contains(&self.cutoff)
            || !unit(self.resonance)
            || !bipolar(self.keytrack)
            || !bipolar(self.env_amount)
            || !(0.0..=10.0).contains(&self.glide)
            || !(0.0..=48.0).contains(&self.bend_range)
            || self.lfos.iter().any(|lfo| {
                !lfo.rate.is_finite()
                    || !(0.0..=30.0).contains(&lfo.rate)
                    || (lfo.rate != 0.0 && lfo.rate < 0.01)
            })
            || self.mods.len() > 16
            || self
                .mods
                .iter()
                .any(|r| !bipolar(r.depth) || matches!(r.source, ModSource::Macro(n) if n >= 4))
            || !env(&self.amp_env)
            || !env(&self.filter_env)
            || self
                .macros
                .iter()
                .any(|m| !unit(m.value) || m.routes.iter().any(|r| !bipolar(r.depth)))
        {
            return Err("Bad synth patch");
        }
        if self.fx.len() > FX_KINDS.len()
            || self
                .fx
                .iter()
                .any(|s| !FX_KINDS.contains(&s.kind) || s.plugin.is_some())
        {
            return Err("Bad synth effect");
        }
        for slot in &self.fx {
            slot.validate()?;
        }
        Ok(())
    }
}

const SIZE: usize = 256;
const FRAMES: usize = 8;
const LEVELS: usize = 7;
/// Construct once outside the audio callback; lookups allocate nothing.
pub struct TableBank {
    frames: Box<[[[[f32; SIZE]; LEVELS]; FRAMES]; 4]>,
}
impl Default for TableBank {
    fn default() -> Self {
        Self::new()
    }
}
impl TableBank {
    pub fn new() -> Self {
        let mut frames = Box::new([[[[0.0; SIZE]; LEVELS]; FRAMES]; 4]);
        for (table, tables) in frames.iter_mut().enumerate() {
            for (frame, levels) in tables.iter_mut().enumerate() {
                let p = frame as f64 / (FRAMES - 1) as f64;
                for (level, wave) in levels.iter_mut().enumerate() {
                    let max_harmonic = 64 >> level;
                    for (i, sample) in wave.iter_mut().enumerate() {
                        let phase = i as f64 / SIZE as f64 * std::f64::consts::TAU;
                        let mut sum = 0.0;
                        for h in 1..=max_harmonic {
                            let n = h as f64;
                            let a = match table {
                                0 => {
                                    if h == 1 {
                                        1.0
                                    } else {
                                        p / n
                                    }
                                }
                                1 => {
                                    if h % 2 == 1 {
                                        (1.0 - p) / n
                                            + p * (std::f64::consts::PI * n * 0.25).sin() / n
                                    } else {
                                        p * (std::f64::consts::PI * n * 0.25).sin() / n
                                    }
                                }
                                2 => {
                                    if [1, 2, 3, 4, 6, 8].contains(&h) {
                                        (1.0 - p * 0.7) / n.sqrt() + p / n
                                    } else {
                                        0.0
                                    }
                                }
                                _ => {
                                    let center = 2.0 + 7.0 * p;
                                    (-(n - center).powi(2) / 5.0).exp() / n.sqrt()
                                        + if h == 1 { 0.35 } else { 0.0 }
                                }
                            };
                            sum += a * (phase * n).sin();
                        }
                        *sample = (sum * 0.5) as f32;
                    }
                }
            }
        }
        Self { frames }
    }
    pub fn sample(
        &self,
        table: Table,
        position: f64,
        phase: f64,
        frequency: f64,
        rate: f64,
    ) -> f64 {
        self.sample_at(table, position, phase, Self::level(frequency, rate))
    }

    /// The band-limited mip level for a note at `frequency` (cache it: it
    /// only changes with the pitch).
    pub fn level(frequency: f64, rate: f64) -> usize {
        let allowed = (rate * 0.5 / frequency.max(1.0)).max(1.0);
        (64.0_f64 / allowed)
            .log2()
            .ceil()
            .max(0.0)
            .min((LEVELS - 1) as f64) as usize
    }

    /// `sample` with the mip level already worked out.
    pub fn sample_at(&self, table: Table, position: f64, phase: f64, level: usize) -> f64 {
        let t = table as usize;
        let frame = position.clamp(0.0, 1.0) * (FRAMES - 1) as f64;
        let lo = frame as usize;
        let hi = (lo + 1).min(FRAMES - 1);
        let level = level.min(LEVELS - 1);
        // Wrap into 0..1 (floor is much cheaper than rem_euclid here).
        let x = (phase - phase.floor()) * SIZE as f64;
        // A hair below 0 can round up to exactly 1.0: stay inside the table.
        let i = (x as usize).min(SIZE - 1);
        let j = (i + 1) % SIZE;
        let frac = x - i as f64;
        let read = |f: usize| {
            let w = &self.frames[t][f][level];
            w[i] as f64 * (1.0 - frac) + w[j] as f64 * frac
        };
        read(lo) * (1.0 - frame.fract()) + read(hi) * frame.fract()
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn modulation_routes_validate() {
        let mut patch = SynthPatch::default();
        let route = ModRoute {
            source: ModSource::Lfo1,
            target: ModTarget::Cutoff,
            depth: 0.5,
        };
        patch.mods = vec![route; 16];
        assert_eq!(patch.routes_to(ModTarget::Cutoff).count(), 16);
        assert!(patch.validate().is_ok());
        patch.mods.push(route);
        assert!(patch.validate().is_err());
        patch.mods.pop();
        patch.mods[0].source = ModSource::Macro(4);
        assert!(patch.validate().is_err());
    }
    #[test]
    fn lfo_sync_and_legacy_defaults() {
        use super::*;
        let old: SynthPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(old.lfos, [Lfo::default(); 2]);
        let lfo = Lfo {
            shape: LfoShape::Sine,
            sync: true,
            division: LfoDivision::Quarter,
            ..Lfo::default()
        };
        assert_eq!(lfo.frequency(120.0), 2.0);
        let mut state = LfoState::default();
        assert_eq!(state.tick(lfo, 120.0, 8.0), 0.0);
        assert!((state.tick(lfo, 120.0, 8.0) - 1.0).abs() < 1e-12);
        assert!(old.validate().is_ok());
    }
    use super::*;
    #[test]
    fn patch_round_trip_and_bounds() {
        let p = SynthPatch::default();
        let q: SynthPatch = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, q);
        assert!(q.validate().is_ok());
        let mut bad = q;
        bad.unison = 8;
        assert!(bad.validate().is_err());
    }
    #[test]
    fn tables_are_periodic_and_nonzero() {
        let b = TableBank::new();
        for t in [
            Table::SineSaw,
            Table::SquarePulse,
            Table::Organ,
            Table::Vowel,
        ] {
            let a = b.sample(t, 0.4, 0.3, 440.0, 48000.0);
            assert!(a.abs() > 0.001);
            assert!((a - b.sample(t, 0.4, 1.3, 440.0, 48000.0)).abs() < 1e-6);
        }
    }
    #[test]
    fn older_song_and_synth_channel_round_trip() {
        let old = crate::sequence::Song::default();
        let data = serde_json::to_string(&old).unwrap();
        let mut loaded: crate::sequence::Song = serde_json::from_str(&data).unwrap();
        loaded.validate().unwrap();
        loaded.instruments[0].kind = crate::sequence::InstrumentKind::Synth {
            patch: SynthPatch::factory(1),
        };
        let data = serde_json::to_string(&loaded).unwrap();
        let loaded: crate::sequence::Song = serde_json::from_str(&data).unwrap();
        loaded.validate().unwrap();
        assert!(
            matches!(&loaded.instruments[0].kind, crate::sequence::InstrumentKind::Synth { patch } if *patch == SynthPatch::factory(1))
        );
    }
}
