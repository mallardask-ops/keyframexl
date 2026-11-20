//! Forge: a sound designer for short game sounds (jumps, coins, hits,
//! lasers). A sound is up to six layers, each a source (oscillator, noise, a
//! sample, a Tide synth note) with drawn pitch, volume and filter curves and
//! its own effects; the mix goes through master effects and four character
//! knobs (Size, Energy, Dirt, Space). The whole sound renders offline into
//! one buffer that a Forge channel plays like a one-shot sample.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod curve;
pub mod export;
mod layer;
pub mod library;
pub mod mutate;
pub mod recipes;
pub mod render;

pub use curve::{Curve, CurvePoint};
pub use render::Rendered;

/// Forge renders at this rate; the engine resamples to its own.
pub const RATE: u32 = 48_000;
pub const MAX_LAYERS: usize = 6;
/// Longest layer, in seconds (start + length).
pub const MAX_SECONDS: f32 = 8.0;
/// Effects ring on at most this long after the last layer ends.
pub const MAX_TAIL: f32 = 4.0;
/// Pitch curves span ±48 semitones.
pub const PITCH_RANGE: f32 = 48.0;
/// The note that plays a sound at its own pitch (FL: C5).
pub const ROOT_KEY: u8 = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wave {
    #[default]
    Square,
    /// 25% pulse: the thinner chip voice.
    Pulse25,
    /// 12.5% pulse: the nasal one.
    Pulse12,
    Triangle,
    Saw,
    Sine,
}

impl Wave {
    pub const ALL: [Wave; 6] = [
        Wave::Square,
        Wave::Pulse25,
        Wave::Pulse12,
        Wave::Triangle,
        Wave::Saw,
        Wave::Sine,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Wave::Square => "Square",
            Wave::Pulse25 => "Pulse 25%",
            Wave::Pulse12 => "Pulse 12%",
            Wave::Triangle => "Triangle",
            Wave::Saw => "Saw",
            Wave::Sine => "Sine",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseColor {
    #[default]
    White,
    Pink,
    /// Deep rumble (integrated white noise).
    Brown,
    /// Sparse clicks and pops; the pitch curve sets how dense.
    Crackle,
    /// A chip noise channel (15-bit shift register); the pitch curve sets its clock.
    Digital,
}

impl NoiseColor {
    pub const ALL: [NoiseColor; 5] = [
        NoiseColor::White,
        NoiseColor::Pink,
        NoiseColor::Brown,
        NoiseColor::Crackle,
        NoiseColor::Digital,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NoiseColor::White => "White",
            NoiseColor::Pink => "Pink",
            NoiseColor::Brown => "Brown",
            NoiseColor::Crackle => "Crackle",
            NoiseColor::Digital => "Chip noise",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    #[default]
    Off,
    LowPass,
    HighPass,
    BandPass,
}

impl FilterMode {
    pub const ALL: [FilterMode; 4] = [
        FilterMode::Off,
        FilterMode::LowPass,
        FilterMode::HighPass,
        FilterMode::BandPass,
    ];
    pub fn label(self) -> &'static str {
        match self {
            FilterMode::Off => "Off",
            FilterMode::LowPass => "Low-pass",
            FilterMode::HighPass => "High-pass",
            FilterMode::BandPass => "Band-pass",
        }
    }
}

/// What a layer plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Osc {
        wave: Wave,
        /// Frequency in Hz at pitch 0.
        hz: f32,
        /// Vibrato: rate in Hz and depth in semitones.
        #[serde(default)]
        vibrato_hz: f32,
        #[serde(default)]
        vibrato: f32,
    },
    Noise {
        color: NoiseColor,
        /// Clock of chip noise and crackle density at pitch 0, in Hz.
        hz: f32,
    },
    /// A sound file, decoded into the project.
    Sample {
        name: String,
        rate: u32,
        #[serde(with = "crate::sampler::b64_frames")]
        frames: Arc<Vec<[f32; 2]>>,
        #[serde(default)]
        reverse: bool,
        /// The channel note it was bounced from, to bounce again.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Bounce>,
    },
    /// One note of a Tide synth patch, played like a sample.
    Synth {
        patch: Box<crate::synth::SynthPatch>,
        key: u8,
    },
}

/// One note of a project channel (a VST plugin, the sampler, a synth),
/// rendered with its mixer insert and held this long.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounce {
    pub channel: u64,
    pub key: u8,
    /// Seconds the key is held; the release rings on after it.
    pub hold: f32,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::Osc { .. } => "Oscillator",
            Source::Noise { .. } => "Noise",
            Source::Sample { .. } => "Sample",
            Source::Synth { .. } => "Synth",
        }
    }
    pub fn osc(wave: Wave, hz: f32) -> Self {
        Source::Osc {
            wave,
            hz,
            vibrato_hz: 0.0,
            vibrato: 0.0,
        }
    }
    pub fn noise(color: NoiseColor, hz: f32) -> Self {
        Source::Noise { color, hz }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub source: Source,
    /// The layer LED: off = muted.
    pub enabled: bool,
    pub solo: bool,
    /// Linear gain 0..2.
    pub gain: f32,
    /// −1..1.
    pub pan: f32,
    /// Seconds after the sound starts.
    pub start: f32,
    /// Seconds the layer lasts; its curves stretch across it.
    pub length: f32,
    /// Semitones, ±`PITCH_RANGE`.
    pub pitch: Curve,
    /// Volume 0..1.
    pub amp: Curve,
    pub filter: FilterMode,
    /// Cutoff 0..1 (20 Hz .. 20 kHz on a log scale).
    pub cutoff: Curve,
    /// 0..1.
    pub resonance: f32,
    /// The layer's own effects, before the mix.
    pub fx: Vec<crate::fx::FxSlot>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".into(),
            source: Source::osc(Wave::Square, 440.0),
            enabled: true,
            solo: false,
            gain: 1.0,
            pan: 0.0,
            start: 0.0,
            length: 0.3,
            pitch: Curve::flat(0.0),
            amp: Curve::new(&[(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)]),
            filter: FilterMode::Off,
            cutoff: Curve::flat(1.0),
            resonance: 0.0,
            fx: Vec::new(),
        }
    }
}

impl Layer {
    pub fn new(name: &str, source: Source, length: f32) -> Self {
        Self {
            name: name.into(),
            source,
            length,
            ..Self::default()
        }
    }
    pub fn with_pitch(mut self, c: Curve) -> Self {
        self.pitch = c;
        self
    }
    pub fn with_amp(mut self, c: Curve) -> Self {
        self.amp = c;
        self
    }
    pub fn with_filter(mut self, mode: FilterMode, cutoff: Curve, resonance: f32) -> Self {
        self.filter = mode;
        self.cutoff = cutoff;
        self.resonance = resonance;
        self
    }
    pub fn with_gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }
    pub fn with_start(mut self, start: f32) -> Self {
        self.start = start;
        self
    }
    pub fn with_pan(mut self, pan: f32) -> Self {
        self.pan = pan;
        self
    }
    pub fn with_fx(mut self, slot: crate::fx::FxSlot) -> Self {
        self.fx.push(slot);
        self
    }
}

/// The four character knobs, each 0..1. They move many settings at once.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Macros {
    /// Small (high, short) .. huge (low, long); 0.5 leaves the sound as drawn.
    pub size: f32,
    /// Lazy (soft, shallow sweeps) .. wild (loud, deep sweeps); 0.5 as drawn.
    pub energy: f32,
    /// Clean .. crunchy (drive, then fewer bits).
    pub dirt: f32,
    /// Dry .. a cave (reverb).
    pub space: f32,
}

impl Default for Macros {
    fn default() -> Self {
        Self {
            size: 0.5,
            energy: 0.5,
            dirt: 0.0,
            space: 0.0,
        }
    }
}

impl Macros {
    pub const NAMES: [&'static str; 4] = ["Size", "Energy", "Dirt", "Space"];
    pub fn get(&self, i: usize) -> f32 {
        [self.size, self.energy, self.dirt, self.space][i.min(3)]
    }
    pub fn set(&mut self, i: usize, v: f32) {
        let v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.5
        };
        match i {
            0 => self.size = v,
            1 => self.energy = v,
            2 => self.dirt = v,
            _ => self.space = v,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default = "ForgePatch::blank")]
pub struct ForgePatch {
    pub name: String,
    pub layers: Vec<Layer>,
    /// Effects on the whole sound, after the layers are mixed.
    pub fx: Vec<crate::fx::FxSlot>,
    pub macros: Macros,
    /// Semitones for the whole sound (variations nudge it).
    pub transpose: f32,
    /// Output level in dB before the safety limiter.
    pub gain_db: f32,
    /// Seeds the noise, so the same patch always renders the same.
    pub seed: u64,
}

impl Default for ForgePatch {
    fn default() -> Self {
        recipes::Recipe::Jump.patch()
    }
}

impl ForgePatch {
    /// No layers, neutral knobs.
    pub fn blank() -> Self {
        Self {
            name: "Sound".into(),
            layers: Vec::new(),
            fx: Vec::new(),
            macros: Macros::default(),
            transpose: 0.0,
            gain_db: 0.0,
            seed: 1,
        }
    }

    /// Bring every value into range (after an edit or an import) so a
    /// render never meets a NaN or an out-of-range effect parameter.
    pub fn tidy(&mut self) {
        self.layers.truncate(MAX_LAYERS);
        let fin = |v: f32, lo: f32, hi: f32, def: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { def }
        };
        for l in &mut self.layers {
            l.gain = fin(l.gain, 0.0, 2.0, 1.0);
            l.pan = fin(l.pan, -1.0, 1.0, 0.0);
            l.start = fin(l.start, 0.0, MAX_SECONDS - 0.01, 0.0);
            l.length = fin(l.length, 0.01, MAX_SECONDS - l.start, 0.3);
            l.resonance = fin(l.resonance, 0.0, 1.0, 0.0);
            l.pitch.tidy(-PITCH_RANGE, PITCH_RANGE);
            l.amp.tidy(0.0, 1.0);
            l.cutoff.tidy(0.0, 1.0);
            match &mut l.source {
                Source::Osc {
                    hz,
                    vibrato_hz,
                    vibrato,
                    ..
                } => {
                    *hz = fin(*hz, 10.0, 12_000.0, 440.0);
                    *vibrato_hz = fin(*vibrato_hz, 0.0, 40.0, 0.0);
                    *vibrato = fin(*vibrato, 0.0, 12.0, 0.0);
                }
                Source::Noise { hz, .. } => *hz = fin(*hz, 10.0, 48_000.0, 4000.0),
                Source::Sample { rate, .. } => *rate = (*rate).clamp(1000, 384_000),
                Source::Synth { key, .. } => *key = (*key).min(127),
            }
            tidy_fx(&mut l.fx);
        }
        tidy_fx(&mut self.fx);
        for i in 0..4 {
            let v = self.macros.get(i);
            self.macros.set(i, v);
        }
        self.transpose = fin(self.transpose, -24.0, 24.0, 0.0);
        self.gain_db = fin(self.gain_db, -36.0, 12.0, 0.0);
    }

    /// Layers that sound: soloed ones if any are, otherwise every enabled one.
    pub fn audible(&self) -> impl Iterator<Item = (usize, &Layer)> {
        let solo = self.layers.iter().any(|l| l.solo);
        self.layers
            .iter()
            .enumerate()
            .filter(move |(_, l)| if solo { l.solo } else { l.enabled })
    }
}

/// Built-in slots only (a hosted plugin cannot run offline here), with
/// parameters inside their ranges.
fn tidy_fx(fx: &mut Vec<crate::fx::FxSlot>) {
    fx.retain(|s| s.kind != crate::fx::FxKind::Plugin);
    for s in fx.iter_mut() {
        let spec = s.kind.params();
        s.params.truncate(spec.len());
        while s.params.len() < spec.len() {
            s.params.push(spec[s.params.len()].default);
        }
        for (v, p) in s.params.iter_mut().zip(spec) {
            *v = if v.is_finite() {
                v.clamp(p.min, p.max)
            } else {
                p.default
            };
        }
        s.mix = if s.mix.is_finite() {
            s.mix.clamp(0.0, 1.0)
        } else {
            1.0
        };
        s.sidechain = None;
    }
}

/// A Forge channel's sound: the patch and its rendered audio. Saved as the
/// patch alone; the audio is rendered again when a project opens.
///
/// An edit (`edit`) changes the patch at once and marks the audio stale; the
/// UI renders it on a worker thread and `install`s the result, while the
/// engine keeps playing the previous audio. Each version of the patch has
/// its own `version`, so a late render of an older patch is never installed.
#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "ForgePatch", into = "ForgePatch")]
pub struct ForgeSound {
    pub patch: ForgePatch,
    pub rendered: Arc<Rendered>,
    version: u64,
    stale: bool,
}

fn next_version() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl ForgeSound {
    /// The patch, rendered now.
    pub fn new(mut patch: ForgePatch) -> Self {
        patch.tidy();
        let rendered = Arc::new(render::render(&patch));
        Self {
            patch,
            rendered,
            version: next_version(),
            stale: false,
        }
    }

    /// Replace the patch and render it now.
    pub fn set(&mut self, patch: ForgePatch) {
        *self = Self::new(patch);
    }

    /// Replace the patch; the audio stays the old one until `install`.
    pub fn edit(&mut self, mut patch: ForgePatch) {
        patch.tidy();
        self.patch = patch;
        self.version = next_version();
        self.stale = true;
    }

    /// The audio does not match the patch yet.
    pub fn stale(&self) -> bool {
        self.stale
    }

    /// Which version of the patch this is (what a render job carries back).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Take a render of version `version`; false (and nothing changes) when
    /// the patch has moved on since.
    pub fn install(&mut self, version: u64, rendered: Arc<Rendered>) -> bool {
        if version != self.version {
            return false;
        }
        self.rendered = rendered;
        self.stale = false;
        true
    }

    /// Render now if stale (before an export that must hear the latest edit).
    pub fn refresh(&mut self) {
        if self.stale {
            self.rendered = Arc::new(render::render(&self.patch));
            self.stale = false;
        }
    }
}

/// Bring every Forge channel's audio up to date, rendering where stale.
pub fn refresh_all(song: &mut crate::sequence::Song) {
    for i in &mut song.instruments {
        if let crate::sequence::InstrumentKind::Forge { sound } = &mut i.kind {
            sound.refresh();
        }
    }
}

impl Default for ForgeSound {
    fn default() -> Self {
        Self::new(ForgePatch::default())
    }
}

impl From<ForgePatch> for ForgeSound {
    fn from(p: ForgePatch) -> Self {
        Self::new(p)
    }
}

impl From<ForgeSound> for ForgePatch {
    fn from(s: ForgeSound) -> Self {
        s.patch
    }
}

impl std::fmt::Debug for ForgeSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeSound")
            .field("patch", &self.patch.name)
            .field("frames", &self.rendered.frames.len())
            .finish()
    }
}

/// Saved sounds: `~/Music/KeyFrameXL/Presets/Forge`, next to the other presets.
pub fn user_dir() -> Option<std::path::PathBuf> {
    Some(
        dirs::audio_dir()
            .or_else(dirs::home_dir)?
            .join("KeyFrameXL")
            .join("Presets")
            .join("Forge"),
    )
}

pub const USER_EXT: &str = "kfxforge";

/// Save `p` as one of "my sounds" under its name (a same-named file is replaced).
pub fn save_user(p: &ForgePatch) -> Result<std::path::PathBuf, String> {
    let dir = user_dir().ok_or("No home folder to save into")?;
    save_user_in(&dir, p)
}

pub fn save_user_in(dir: &std::path::Path, p: &ForgePatch) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.{USER_EXT}", export::stem_of(&p.name)));
    let json = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// "My sounds", sorted by name; unreadable files are skipped.
pub fn user_sounds() -> Vec<ForgePatch> {
    user_dir().map(|d| user_sounds_in(&d)).unwrap_or_default()
}

pub fn user_sounds_in(dir: &std::path::Path) -> Vec<ForgePatch> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<ForgePatch> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == USER_EXT))
        .filter_map(|p| std::fs::read(&p).ok())
        .filter_map(|b| serde_json::from_slice::<ForgePatch>(&b).ok())
        .map(|mut p| {
            p.tidy();
            p
        })
        .collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    v
}

/// A small deterministic random source (xorshift64*).
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// 0..1
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// −1..1
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::recipes::Recipe;
    use super::*;
    use crate::sequence::{InstrumentKind, Song};

    fn song_with(p: ForgePatch) -> (Song, u64) {
        let mut song = Song::default();
        let id = song.add_channel(
            "Forge",
            InstrumentKind::Forge {
                sound: ForgeSound::new(p),
            },
        );
        (song, id)
    }

    #[test]
    fn a_project_keeps_the_patch_and_renders_it_again() {
        let (song, id) = song_with(Recipe::Coin.patch());
        let json = serde_json::to_string(&song).unwrap();
        // Only the patch is saved, never the audio.
        assert!(!json.contains("rendered"));
        let back: Song = serde_json::from_str(&json).unwrap();
        let (InstrumentKind::Forge { sound: a }, InstrumentKind::Forge { sound: b }) = (
            &song.instrument(id).unwrap().kind,
            &back.instrument(id).unwrap().kind,
        ) else {
            panic!("not a forge channel");
        };
        assert_eq!(a.patch, b.patch);
        assert_eq!(a.rendered, b.rendered);
    }

    #[test]
    fn my_sounds_save_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Recipe::Magic.patch();
        p.name = "Boss spell".into();
        let path = save_user_in(dir.path(), &p).unwrap();
        assert!(path.ends_with("boss_spell.kfxforge"));
        std::fs::write(dir.path().join("broken.kfxforge"), "{nope").unwrap();
        let list = user_sounds_in(dir.path());
        assert_eq!(list, vec![p]);
    }

    #[test]
    fn an_edit_waits_for_its_own_render() {
        let mut s = ForgeSound::new(Recipe::Coin.patch());
        let old = s.rendered.clone();
        s.edit(Recipe::Laser.patch());
        assert!(s.stale());
        assert!(
            Arc::ptr_eq(&s.rendered, &old),
            "the old audio plays meanwhile"
        );
        let v1 = s.version();
        let r1 = Arc::new(render::render(&s.patch));
        s.edit(Recipe::Hit.patch());
        // A render of the Laser patch arrives after the Hit edit: refused.
        assert!(!s.install(v1, r1));
        assert!(s.stale());
        let r2 = Arc::new(render::render(&s.patch));
        assert!(s.install(s.version(), r2.clone()));
        assert!(!s.stale() && s.rendered == r2);

        let (mut song, id) = song_with(Recipe::Coin.patch());
        if let Some(InstrumentKind::Forge { sound }) = song
            .instruments
            .iter_mut()
            .find(|i| i.id == id)
            .map(|i| &mut i.kind)
        {
            sound.edit(Recipe::Step.patch());
        }
        refresh_all(&mut song);
        let InstrumentKind::Forge { sound } = &song.instrument(id).unwrap().kind else {
            unreachable!()
        };
        assert!(!sound.stale());
        assert_eq!(*sound.rendered, render::render(&Recipe::Step.patch()));
    }

    #[test]
    fn a_patch_with_missing_fields_opens() {
        let p: ForgePatch = serde_json::from_str(
            r#"{"name":"Old","layers":[{"name":"A","source":{"Osc":{"wave":"Saw","hz":220.0}}}]}"#,
        )
        .unwrap();
        assert_eq!(p.layers.len(), 1);
        assert_eq!(p.macros, Macros::default());
        assert!(ForgeSound::new(p).rendered.peak > 0.05);
    }

    /// A note at C5 plays the rendered sound as it is; an octave up, twice as fast.
    #[test]
    fn the_engine_plays_the_rendered_sound() {
        let (song, id) = song_with(Recipe::Laser.patch());
        let InstrumentKind::Forge { sound } = &song.instrument(id).unwrap().kind else {
            unreachable!()
        };
        let want = sound.rendered.frames.clone();
        let play = |key: u8| {
            let mut e = crate::engine::Engine::new(RATE);
            e.live_on(&song, 1, id, key, 100);
            let mut out = Vec::new();
            let mut block = vec![0.0f32; 1024];
            while out.len() < want.len() + 4800 {
                block.fill(0.0);
                e.process_live(&song, &mut block);
                out.extend(block.as_chunks::<2>().0.iter().map(|f| [f[0], f[1]]));
            }
            out
        };
        let out = play(ROOT_KEY);
        let err = want
            .iter()
            .zip(&out)
            .map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs()))
            .fold(0.0, f32::max);
        assert!(err < 1e-4, "differs by {err}");
        let sounding = |o: &[[f32; 2]]| o.iter().rposition(|f| f[0].abs() > 1e-3).unwrap_or(0);
        let up = play(ROOT_KEY + 12);
        let ratio = sounding(&out) as f64 / sounding(&up) as f64;
        assert!((1.8..2.2).contains(&ratio), "octave up ratio {ratio}");
    }

    #[test]
    fn the_offline_render_plays_forge_notes() {
        let (mut song, id) = song_with(Recipe::Hit.patch());
        let pat = song.patterns[0].id;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(crate::sequence::Note::new(id, 0, 120, ROOT_KEY, 100));
        let out = crate::engine::render(&song, pat, false, RATE);
        assert!(out.iter().any(|f| f[0].abs() > 0.05));
    }
}
