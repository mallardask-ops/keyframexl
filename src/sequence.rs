//! Project model, format 2. Musical time is integer ticks at 480 per quarter
//! note (FL: PPQ). Every channel plays notes; a step in the channel rack is a
//! note one step long. Snap is a view setting, never a property of the data.

use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

pub use crate::sampler::SamplePreset;

/// Musical time in ticks.
pub type Tick = i64;
/// Ticks per quarter note. Divisible by 2, 3, 4, 5, 6 and 8: exact triplets.
pub const PPQ: Tick = 480;
/// One 1/16 step of the channel rack.
pub const STEP: Tick = PPQ / 4;
/// One 4/4 bar.
pub const BAR: Tick = PPQ * 4;
/// Longest pattern and latest clip start, in bars.
pub const MAX_BARS: Tick = 1024;
/// Pitch a new step gets (FL: C5).
pub const STEP_PITCH: u8 = 60;

pub const RATE: u32 = 48_000;
pub fn default_tempo() -> f64 {
    120.0
}
pub fn seconds_per_step(bpm: f64) -> f64 {
    60.0 / bpm.clamp(40.0, 300.0) / 4.0
}
/// Ticks as (fractional) steps. Exact for whole steps, which keeps format-1
/// projects rendering bit for bit.
pub fn steps(t: Tick) -> f64 {
    t as f64 / STEP as f64
}

fn half() -> f32 {
    0.5
}
fn one() -> f32 {
    1.0
}

/// One note. Per-note properties follow FL's piano roll (Note properties):
/// pan, release, fine pitch, mod X / mod Y, colour.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Note {
    /// Stable within the project; editors select by id.
    #[serde(default)]
    pub id: u64,
    pub instrument: u64,
    pub start: Tick,
    pub length: Tick,
    pub pitch: u8,
    pub velocity: u8,
    /// -1 (left) .. 1 (right).
    #[serde(default)]
    pub pan: f32,
    /// Release velocity 0..1.
    #[serde(default = "half")]
    pub release: f32,
    /// Fine pitch in cents, -100..100.
    #[serde(default)]
    pub fine: f32,
    /// Free controllers 0..1 (FL Mod X / Mod Y).
    #[serde(default = "half")]
    pub mod_x: f32,
    #[serde(default = "half")]
    pub mod_y: f32,
    #[serde(default)]
    pub muted: bool,
    /// Colour group 0..15 (FL note colour / MIDI channel).
    #[serde(default)]
    pub color: u8,
    /// Slide group: a glide bends the notes of its group.
    #[serde(default)]
    pub group: u8,
    /// Portamento (FL: porta note): glides in from the channel's previous
    /// note over the channel's glide time, or one step when it has none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub porta: bool,
}

impl Note {
    pub fn new(instrument: u64, start: Tick, length: Tick, pitch: u8, velocity: u8) -> Self {
        Self {
            id: 0,
            instrument,
            start,
            length,
            pitch,
            velocity,
            pan: 0.0,
            release: 0.5,
            fine: 0.0,
            mod_x: 0.5,
            mod_y: 0.5,
            muted: false,
            color: 0,
            group: 0,
            porta: false,
        }
    }
    pub fn end(&self) -> Tick {
        self.start + self.length
    }
}

/// Pitch glide: notes of `group` on `instrument` that sound at `start` bend to `target`.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Glide {
    pub start: Tick,
    pub length: Tick,
    pub target: u8,
    pub group: u8,
    pub instrument: u64,
}

/// The shape of a pitch curve segment (UTAU / OpenUtau names in brackets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveShape {
    /// Sine in and out (io).
    #[default]
    Smooth,
    /// Straight (l).
    Linear,
    /// Slow start (i, UST "j").
    EaseIn,
    /// Slow end (o, UST "r").
    EaseOut,
}

impl CurveShape {
    pub const ALL: [Self; 4] = [Self::Smooth, Self::Linear, Self::EaseIn, Self::EaseOut];
    pub fn label(self) -> &'static str {
        match self {
            Self::Smooth => "Smooth",
            Self::Linear => "Linear",
            Self::EaseIn => "Ease in",
            Self::EaseOut => "Ease out",
        }
    }
    /// 0..1 → 0..1 along the shape, from the segment's start.
    pub fn at(self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let pi = std::f64::consts::PI;
        match self {
            Self::Smooth => 0.5 - 0.5 * (pi * x).cos(),
            Self::Linear => x,
            Self::EaseIn => 1.0 - (pi * x / 2.0).cos(),
            Self::EaseOut => (pi * x / 2.0).sin(),
        }
    }
}

/// A point of a sung note's pitch curve: ticks from the note's start, cents
/// from the note. The segment from this point to the next has `shape`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchPoint {
    pub at: Tick,
    pub cents: f32,
    #[serde(default)]
    pub shape: CurveShape,
}

/// A sung note's own vibrato, over the channel's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteVibrato {
    /// Share of the note at its end that shakes, 0..1.
    pub len: f32,
    /// Peak depth in cents.
    pub depth: f32,
    /// Hz.
    pub rate: f32,
    /// Seconds.
    pub fade_in: f32,
    pub fade_out: f32,
}

impl Default for NoteVibrato {
    fn default() -> Self {
        Self {
            len: 0.6,
            depth: 40.0,
            rate: 5.5,
            fade_in: 0.15,
            fade_out: 0.05,
        }
    }
}

/// A sung note's pitch beyond its key: drawn points (they replace the glide
/// into the note) and its own vibrato. Kept beside the note, as lyrics are,
/// so `Note` stays `Copy`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotePitch {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<PitchPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vibrato: Option<NoteVibrato>,
}

impl NotePitch {
    /// Most points a note keeps (undo copies the song).
    pub const MAX_POINTS: usize = 64;

    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.vibrato.is_none()
    }

    /// Cents from the note at `at` ticks into a note `length` long: the
    /// points joined by their shapes, 0 at the note's ends unless a point
    /// sits there.
    pub fn cents_at(&self, at: f64, length: Tick) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        let mut pts: Vec<(f64, f64, CurveShape)> = Vec::with_capacity(self.points.len() + 2);
        if self.points[0].at > 0 {
            pts.push((0.0, 0.0, CurveShape::Smooth));
        }
        pts.extend(
            self.points
                .iter()
                .map(|p| (p.at as f64, p.cents as f64, p.shape)),
        );
        if self.points.last().is_some_and(|p| p.at < length) {
            pts.push((length as f64, 0.0, CurveShape::Smooth));
        }
        if at <= pts[0].0 {
            return pts[0].1;
        }
        for w in pts.windows(2) {
            let ((t0, c0, shape), (t1, c1, _)) = (w[0], w[1]);
            if at <= t1 {
                let x = if t1 > t0 { (at - t0) / (t1 - t0) } else { 1.0 };
                return c0 + (c1 - c0) * shape.at(x);
            }
        }
        pts.last().map_or(0.0, |p| p.1)
    }

    /// Points in time order, one per tick, within the note and range.
    pub fn tidy(&mut self, length: Tick) {
        for p in &mut self.points {
            p.at = p.at.clamp(0, length.max(0));
            p.cents = p.cents.clamp(-2400.0, 2400.0);
        }
        self.points.sort_by_key(|p| p.at);
        self.points.dedup_by_key(|p| p.at);
        self.points.truncate(Self::MAX_POINTS);
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Pattern {
    pub id: u64,
    pub name: String,
    /// Length in ticks; a whole number of steps.
    pub length: Tick,
    pub notes: Vec<Note>,
    pub slides: Vec<Glide>,
    /// An automation clip: this envelope instead of notes (FL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<crate::automation::Envelope>,
    /// An audio clip (FL): plays this sampler channel's sample directly across
    /// the timeline with full seeking, slicing, trimming and waveform preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_clip: Option<u64>,
    /// Playlist / pattern-picker colour as 0xRRGGBB; `None` = by id (FL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// A time selection inside the pattern (ticks, start < end): Pattern mode
    /// loops it instead of the whole pattern (FL: select on the roll's ruler).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_range: Option<(Tick, Tick)>,
    /// Lyrics of vocal channels' notes, by note id.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub lyrics: std::collections::BTreeMap<u64, String>,
    /// Pitch curves and vibrato of vocal channels' notes, by note id.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub pitch: std::collections::BTreeMap<u64, NotePitch>,
}

impl Pattern {
    /// Give the notes fresh ids from `first` on; lyrics and pitch curves
    /// follow their notes.
    pub fn renumber_notes(&mut self, first: u64) {
        let old = std::mem::take(&mut self.lyrics);
        let old_pitch = std::mem::take(&mut self.pitch);
        for (i, n) in self.notes.iter_mut().enumerate() {
            let id = first + i as u64;
            if let Some(l) = old.get(&n.id) {
                self.lyrics.insert(id, l.clone());
            }
            if let Some(c) = old_pitch.get(&n.id) {
                self.pitch.insert(id, c.clone());
            }
            n.id = id;
        }
    }
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            length: BAR,
            notes: vec![],
            slides: vec![],
            automation: None,
            audio_clip: None,
            color: None,
            loop_range: None,
            lyrics: Default::default(),
            pitch: Default::default(),
        }
    }
    pub fn step_count(&self) -> usize {
        (self.length / STEP) as usize
    }
    /// The note that is step `step` of `instrument`: starts exactly on it.
    pub fn step_note(&self, instrument: u64, step: usize) -> Option<usize> {
        let at = step as Tick * STEP;
        self.notes
            .iter()
            .position(|n| n.instrument == instrument && n.start == at)
    }
    /// A channel shows step pads while all its notes look like steps:
    /// on the step grid and one step long. Otherwise the rack shows a
    /// note preview and editing happens in the piano roll (FL behaviour).
    pub fn is_step_channel(&self, instrument: u64) -> bool {
        self.notes
            .iter()
            .filter(|n| n.instrument == instrument)
            .all(|n| n.start % STEP == 0 && n.length == STEP)
            && !self.slides.iter().any(|s| s.instrument == instrument)
    }
}

/// A pattern placed in the playlist. The clip plays the window
/// `offset .. offset + length` of its pattern, starting at `start`.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Clip {
    pub pattern: u64,
    pub start: Tick,
    /// Playlist track (row), 0-based.
    #[serde(default)]
    pub track: u32,
    /// Ticks cut from the pattern start (FL: slip / left-edge resize).
    #[serde(default)]
    pub offset: Tick,
    /// Ticks played; `None` follows the pattern length.
    #[serde(default)]
    pub length: Option<Tick>,
    #[serde(default)]
    pub muted: bool,
}

impl Clip {
    pub fn new(pattern: u64, start: Tick, track: u32) -> Self {
        Self {
            pattern,
            start,
            track,
            offset: 0,
            length: None,
            muted: false,
        }
    }
}

/// Inserts besides Master in a new project (FL shows 16 in its default view).
pub const INSERTS: usize = 16;
/// Effect slots per mixer track (FL: 10).
pub const FX_SLOTS: usize = 10;
/// Fader range: linear gain, 1.0 = 0 dB, max ≈ +5.6 dB like FL.
pub const FADER_MAX: f32 = 1.9;

/// Gain at `pos` (0..1) of a volume control's travel. Every volume control
/// (mixer faders, channel knobs, their automation and MIDI controllers) uses
/// this cubic taper, the same law as PulseAudio's sliders: loudness changes
/// evenly along the travel, 0 dB sits at 81 % and half travel is −12.5 dB.
/// A linear-gain knob put most of the change into the bottom fifth, and a
/// linear-dB fader made half travel −27 dB.
pub fn volume_gain(pos: f32) -> f32 {
    FADER_MAX * pos.clamp(0.0, 1.0).powi(3)
}

/// Where `gain` sits on a volume control's travel (inverse of `volume_gain`).
pub fn volume_pos(gain: f32) -> f32 {
    (gain.max(0.0) / FADER_MAX).cbrt().min(1.0)
}

/// A gain as dB text for volume controls (`−∞ dB` at silence).
pub fn gain_db_text(gain: f32) -> String {
    if gain <= 1e-5 {
        "−∞ dB".into()
    } else {
        format!("{:+.1} dB", 20.0 * gain.log10())
    }
}

/// A mixer track: Master (index 0) or an insert. Channels feed one via
/// `Instrument::mixer`; inserts feed Master and may send to later inserts.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MixerTrack {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fl_slots: Vec<FlMixerSlot>,
    #[serde(default)]
    pub name: String,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub fx: Vec<crate::fx::FxSlot>,
    /// Inserts only: feed Master (FL: the route arrow to Master).
    #[serde(default = "yes")]
    pub to_master: bool,
    /// Post-fader sends to later inserts.
    #[serde(default)]
    pub sends: Vec<Send>,
    /// Built-in 3-band EQ after the effect slots (FL: the strip EQ):
    /// low shelf, mid peak, high shelf gains in dB. All 0 = off.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub eq: [f32; 3],
    /// Stereo separation −1..1 (FL: SEP): −1 mono, 0 as is, +1 twice as wide.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sep: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

fn is_flat(eq: &[f32; 3]) -> bool {
    *eq == [0.0; 3]
}

/// Built-in EQ band frequencies (Hz) and gain range (dB).
pub const EQ_FREQS: [f32; 3] = [90.0, 1400.0, 6200.0];
pub const EQ_RANGE: f32 = 18.0;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Send {
    pub to: u16,
    pub level: f32,
}

fn yes() -> bool {
    true
}

impl MixerTrack {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            fx: vec![],
            fl_slots: vec![],
            to_master: true,
            sends: vec![],
            eq: [0.0; 3],
            sep: 0.0,
        }
    }
    /// Route only into `to` (0 = Master): clears the other destinations,
    /// keeping the send level when one already exists.
    pub fn route_only(&mut self, to: u16) {
        if to == 0 {
            self.to_master = true;
            self.sends.clear();
        } else {
            self.to_master = false;
            let level = self
                .sends
                .iter()
                .find(|s| s.to == to)
                .map(|s| s.level)
                .unwrap_or(1.0);
            self.sends.clear();
            self.sends.push(Send { to, level });
        }
    }
    /// Add a send into `to` (0 = the Master route), keeping the level of one
    /// that already exists. Returns true when a send was added.
    pub fn add_send(&mut self, to: u16) -> bool {
        if to == 0 {
            let was = self.to_master;
            self.to_master = true;
            return !was;
        }
        if self.sends.iter().any(|s| s.to == to) {
            return false;
        }
        self.sends.push(Send { to, level: 1.0 });
        true
    }
    /// Drop every send; the Master route stays as it is.
    pub fn clear_sends(&mut self) {
        self.sends.clear();
    }
    /// The built-in EQ as parameters of the Parametric EQ effect.
    pub fn eq_params(&self) -> [f32; 7] {
        let [low, mid, high] = self.eq;
        [EQ_FREQS[0], low, EQ_FREQS[1], mid, 0.8, EQ_FREQS[2], high]
    }
    pub fn gains(&self) -> [f32; 2] {
        [
            self.volume * (1.0 - self.pan.max(0.0)),
            self.volume * (1.0 + self.pan.min(0.0)),
        ]
    }
}

pub fn default_mixer() -> Vec<MixerTrack> {
    std::iter::once(MixerTrack::new("Master"))
        .chain((1..=INSERTS).map(|i| MixerTrack::new(&format!("Insert {i}"))))
        .collect()
}

/// Outgoing route edges of insert `v`: Master (as 0) when routed there, then sends.
fn route_edges(mixer: &[MixerTrack], v: usize) -> Vec<usize> {
    let mut out = Vec::new();
    if v > 0 && mixer[v].to_master {
        out.push(0);
    }
    out.extend(
        mixer[v]
            .sends
            .iter()
            .map(|s| s.to as usize)
            .filter(|t| *t < mixer.len()),
    );
    // A sidechain key feeds the insert that listens to it: that insert mixes after.
    out.extend(
        (1..mixer.len())
            .filter(|&t| t != v && mixer[t].fx.iter().any(|f| f.sidechain == Some(v as u16))),
    );
    out
}

/// Routing `from` into `to` would close a loop: itself, or `to` already feeds `from`.
pub fn route_would_loop(mixer: &[MixerTrack], from: usize, to: usize) -> bool {
    if from == 0 || from >= mixer.len() || to >= mixer.len() || from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = vec![false; mixer.len()];
    while let Some(v) = stack.pop() {
        if v == from {
            return true;
        }
        if seen[v] {
            continue;
        }
        seen[v] = true;
        stack.extend(route_edges(mixer, v));
    }
    false
}

/// True when any route path leaves an insert and returns to it.
pub fn mixer_has_loop(mixer: &[MixerTrack]) -> bool {
    for start in 0..mixer.len() {
        for t in route_edges(mixer, start) {
            let mut stack = vec![t];
            let mut seen = vec![false; mixer.len()];
            while let Some(v) = stack.pop() {
                if v == start {
                    return true;
                }
                if seen[v] {
                    continue;
                }
                seen[v] = true;
                stack.extend(route_edges(mixer, v));
            }
        }
    }
    false
}

/// A playlist track (row). Rows without an entry use the defaults.
#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Track {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub muted: bool,
    /// Header colour as 0xRRGGBB; `None` = default (FL: track colour).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

/// A channel (FL: Channel rack row) and its instrument.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Instrument {
    pub id: u64,
    pub name: String,
    pub kind: InstrumentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fl_plugin: Option<FlPluginState>,
    /// Linear gain 0..FADER_MAX (1.0 = 0 dB); the knob moves along `volume_gain`.
    #[serde(default = "one")]
    pub volume: f32,
    /// -1 (left) .. 1 (right).
    #[serde(default)]
    pub pan: f32,
    /// Mixer insert; 0 = Master until the mixer exists.
    #[serde(default)]
    pub mixer: u16,
    /// FL: the green LED. Solo is "mute every other channel".
    #[serde(default)]
    pub muted: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum InstrumentKind {
    /// Built-in drum synth voice: 0 kick, 1 snare, 2 hat.
    Drum {
        #[serde(alias = "row")]
        voice: u8,
    },
    /// Our sampler (DirectWave-like). The preset travels with the project.
    Sampler {
        #[serde(default)]
        preset: Option<std::sync::Arc<SamplePreset>>,
    },
    Synth {
        #[serde(default)]
        patch: crate::synth::SynthPatch,
    },
    /// A singing voice: notes with lyrics sung from a UTAU voicebank.
    Vocal { voice: crate::vocal::Voice },
    /// A hosted instrument plugin (VST3); its state travels with the project.
    Plugin { plugin: PluginRef },
    /// A game sound built in the Forge: layers, curves and effects rendered
    /// into one one-shot that every note plays (C5 at its own pitch).
    Forge { sound: crate::forge::ForgeSound },
}

/// A hosted plugin in the project: what to load and the state to give it.
/// A plugin that is missing on this computer keeps its state untouched and
/// stays silent.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PluginRef {
    /// Unique among the project's plugins (the engine finds the instance by it).
    pub id: u64,
    /// "vst3".
    pub format: String,
    /// Bundle path (e.g. ~/.vst3/yabridge/Name.vst3).
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    /// The plugin's own state (base64), refreshed on save.
    #[serde(default)]
    pub state: String,
    /// Mixer track of each output pair past the main one (FL: the plugin's
    /// output routing). 0 or no entry: into the channel's own insert.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outs: Vec<u16>,
}

/// Settings from an FL Studio plugin's event 213 in the source project.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct FlPluginState {
    pub name: String,
    pub state: std::sync::Arc<str>,
}

/// Playback-only FLP with the original native instruments and their presets.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct NativeFlProject {
    pub project: std::sync::Arc<str>,
    pub channels: Vec<u32>,
}

impl NativeFlProject {
    pub fn new(project: &[u8], channels: Vec<u32>) -> Self {
        use base64::Engine as _;
        Self {
            project: base64::engine::general_purpose::STANDARD
                .encode(project)
                .into(),
            channels,
        }
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(&*self.project)
            .map_err(|e| format!("Invalid FL bridge project: {e}"))
    }
}

impl FlPluginState {
    pub fn new(name: &str, data: &[u8]) -> Self {
        use base64::Engine as _;
        Self {
            name: name.into(),
            state: base64::engine::general_purpose::STANDARD
                .encode(data)
                .into(),
        }
    }

    pub fn data(&self) -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(self.state.as_bytes())
            .unwrap_or_default()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct FlMixerSlot {
    pub index: u32,
    pub enabled: bool,
    pub mix: f32,
    pub plugin: FlPluginState,
}

impl PluginRef {
    pub fn state_bytes(&self) -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(&self.state)
            .unwrap_or_default()
    }
    pub fn set_state(&mut self, bytes: &[u8]) {
        use base64::Engine as _;
        self.state = base64::engine::general_purpose::STANDARD.encode(bytes);
    }
}

pub const DRUM_VOICES: [&str; 3] = ["Kick", "Snare", "Hat"];

impl Instrument {
    pub fn new(id: u64, name: &str, kind: InstrumentKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            fl_plugin: None,
            volume: 1.0,
            pan: 0.0,
            mixer: 0,
            muted: false,
        }
    }
    pub fn sampler_preset(&self) -> Option<&SamplePreset> {
        match &self.kind {
            InstrumentKind::Sampler { preset } => preset.as_deref(),
            _ => None,
        }
    }
    pub fn is_sampler(&self) -> bool {
        matches!(self.kind, InstrumentKind::Sampler { .. })
    }
    pub fn plugin(&self) -> Option<&PluginRef> {
        match &self.kind {
            InstrumentKind::Plugin { plugin } => Some(plugin),
            _ => None,
        }
    }
    pub fn drum_voice(&self) -> Option<u8> {
        match self.kind {
            InstrumentKind::Drum { voice } => Some(voice),
            _ => None,
        }
    }
    /// Left/right gain from volume and pan (balance law: centre = unity).
    pub fn gains(&self) -> [f64; 2] {
        let v = self.volume as f64;
        let p = self.pan as f64;
        [v * (1.0 - p.max(0.0)), v * (1.0 + p.min(0.0))]
    }
}

pub fn default_swing() -> f64 {
    0.0
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Song {
    pub version: u8,
    #[serde(default = "default_tempo")]
    pub tempo: f64,
    /// Swing 0.0..=0.6: notes on odd 16th steps play late by this fraction of a step.
    #[serde(default = "default_swing")]
    pub swing: f64,
    pub instruments: Vec<Instrument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_fl: Option<std::sync::Arc<NativeFlProject>>,
    pub patterns: Vec<std::sync::Arc<Pattern>>,
    pub clips: Vec<Clip>,
    /// Named or muted playlist tracks; index = `Clip::track`.
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Master + inserts.
    #[serde(default = "default_mixer")]
    pub mixer: Vec<MixerTrack>,
    /// Song tempo track (FL): empty = flat `tempo`.
    #[serde(default, skip_serializing_if = "crate::tempo::TempoMap::is_empty")]
    pub tempo_map: crate::tempo::TempoMap,
    /// Time-signature changes by bar; empty = all 4/4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meters: Vec<crate::tempo::Meter>,
    /// Song-mode loop (FL: the playlist time selection), start..end in ticks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_range: Option<(Tick, Tick)>,
    /// Song-mode start marker (FL: the playback start position): Stop goes
    /// back here and Play starts here, until it is moved or removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_start: Option<Tick>,
    /// Named playlist positions (FL: markers), sorted by tick.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    /// Title, author, genre, notes (FL: Project info); also the export's tags.
    #[serde(default, skip_serializing_if = "ProjectInfo::is_empty")]
    pub info: ProjectInfo,
    /// The family this game's sounds belong to: Forge recipes start in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_style: Option<crate::forge::recipes::Style>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// What the project is (FL: Options → Project info).
#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct ProjectInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub genre: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub year: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub copyright: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments: String,
    /// Open Project info automatically when this project file is loaded.
    #[serde(default, skip_serializing_if = "is_false")]
    pub show_on_open: bool,
    /// Project musical key/scale; synced with piano roll scale highlighting on load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<crate::scale::Scale>,
}

impl ProjectInfo {
    pub const MAX_FIELD: usize = 200;
    pub const MAX_YEAR: usize = 32;
    pub const MAX_URL: usize = 500;
    pub const MAX_COMMENTS: usize = 20_000;

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A named song position on the playlist ruler.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Marker {
    pub tick: Tick,
    pub name: String,
}

impl Default for Song {
    fn default() -> Self {
        Self {
            version: 2,
            tempo: default_tempo(),
            swing: default_swing(),
            instruments: vec![
                Instrument::new(1, "Kick", InstrumentKind::Drum { voice: 0 }),
                Instrument::new(2, "Snare", InstrumentKind::Drum { voice: 1 }),
                Instrument::new(3, "Hat", InstrumentKind::Drum { voice: 2 }),
                Instrument::new(4, "Bass", InstrumentKind::Sampler { preset: None }),
            ],
            native_fl: None,
            patterns: vec![Pattern::new(1, "Pattern 1").into()],
            clips: vec![],
            tracks: vec![],
            mixer: default_mixer(),
            tempo_map: crate::tempo::TempoMap::default(),
            meters: vec![],
            loop_range: None,
            play_start: None,
            markers: vec![],
            info: ProjectInfo::default(),
            game_style: None,
        }
    }
}

impl Song {
    /// Insert an empty mixer track, returning its index (or the length at capacity).
    pub fn insert_track(&mut self, at: usize) -> usize {
        if self.mixer.len() >= 128 {
            return self.mixer.len();
        }
        let at = at.clamp(1, self.mixer.len());
        for inst in &mut self.instruments {
            if inst.mixer as usize >= at {
                inst.mixer += 1;
            }
            if let InstrumentKind::Plugin { plugin } = &mut inst.kind {
                for o in plugin.outs.iter_mut().filter(|o| **o as usize >= at) {
                    *o += 1;
                }
            }
        }
        for track in &mut self.mixer {
            for send in &mut track.sends {
                if send.to as usize >= at {
                    send.to += 1;
                }
            }
            for k in track.fx.iter_mut().filter_map(|f| f.sidechain.as_mut()) {
                if *k as usize >= at {
                    *k += 1;
                }
            }
        }
        for pattern in self
            .patterns
            .iter_mut()
            .filter(|p| p.automation.is_some())
            .map(std::sync::Arc::make_mut)
        {
            if let Some(env) = &mut pattern.automation {
                use crate::automation::Target::*;
                match &mut env.target {
                    MixerVolume { track }
                    | MixerPan { track }
                    | MixerSep { track }
                    | MixerEq { track, .. }
                    | FxParam { track, .. }
                    | FxMix { track, .. }
                    | FxEnabled { track, .. }
                        if *track as usize >= at =>
                    {
                        *track += 1
                    }
                    SendLevel { track, to } => {
                        if *track as usize >= at {
                            *track += 1;
                        }
                        if *to as usize >= at {
                            *to += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        self.mixer
            .insert(at, MixerTrack::new(&format!("Insert {at}")));
        at
    }

    /// FL's "auto map outputs": a multi-out plugin channel's main pair (when it
    /// plays straight into Master) and its `extra` pairs each get a free insert,
    /// named after the channel and the outputs ("SSD5 3-4"). Free means unused
    /// by any channel or plugin output, still named "Insert N", with no effects
    /// or sends; inserts are added when too few are free. Returns how many
    /// pairs were mapped.
    pub fn map_plugin_outputs(&mut self, inst: u64, extra: usize) -> usize {
        let Some(i) = self
            .instruments
            .iter()
            .position(|i| i.id == inst && i.plugin().is_some())
        else {
            return 0;
        };
        let extra = extra.min(crate::plugin::MAX_EXTRA_PAIRS);
        let name = self.instruments[i].name.clone();
        let mut used: Vec<bool> = vec![false; self.mixer.len()];
        for x in &self.instruments {
            if let Some(u) = used.get_mut(x.mixer as usize) {
                *u = true;
            }
            for &o in x.plugin().map(|p| p.outs.as_slice()).unwrap_or(&[]) {
                if let Some(u) = used.get_mut(o as usize) {
                    *u = true;
                }
            }
        }
        let start = self.instruments[i].mixer as usize;
        let free = |song: &Song, t: usize, used: &[bool]| {
            let m = &song.mixer[t];
            t > 0
                && !used[t]
                && m.name == format!("Insert {t}")
                && m.fx.is_empty()
                && m.sends.is_empty()
                && m.to_master
        };
        let next = |song: &mut Song, used: &mut Vec<bool>| -> Option<usize> {
            let n = song.mixer.len();
            let t = (start + 1..n)
                .chain(1..=start.min(n - 1))
                .find(|&t| free(song, t, used))
                .or_else(|| {
                    let t = song.insert_track(song.mixer.len());
                    (t < song.mixer.len()).then_some(t)
                })?;
            if t >= used.len() {
                used.resize(t + 1, false);
            }
            used[t] = true;
            Some(t)
        };
        let mut mapped = 0;
        if self.instruments[i].mixer == 0
            && let Some(t) = next(self, &mut used)
        {
            self.instruments[i].mixer = t as u16;
            self.mixer[t].name = format!("{name} 1-2");
            mapped += 1;
        }
        let mut outs = Vec::with_capacity(extra);
        for k in 0..extra {
            let Some(t) = next(self, &mut used) else {
                break;
            };
            self.mixer[t].name = format!("{name} {}-{}", 2 * k + 3, 2 * k + 4);
            outs.push(t as u16);
            mapped += 1;
        }
        if let InstrumentKind::Plugin { plugin } = &mut self.instruments[i].kind {
            plugin.outs = outs;
        }
        mapped
    }

    /// Move effect slot `from` to `to` on mixer insert `track` (FL: drag a
    /// slot in the list); the slots between shift by one. Automation of every
    /// moved slot follows it. False when there is nothing to move.
    pub fn move_fx_slot(&mut self, track: usize, from: usize, to: usize) -> bool {
        let Some(m) = self.mixer.get_mut(track) else {
            return false;
        };
        let n = m.fx.len();
        if from >= n || to >= n || from == to {
            return false;
        }
        let slot = m.fx.remove(from);
        m.fx.insert(to, slot);
        for pattern in self
            .patterns
            .iter_mut()
            .filter(|p| p.automation.is_some())
            .map(std::sync::Arc::make_mut)
        {
            if let Some(env) = &mut pattern.automation {
                use crate::automation::Target::*;
                if let FxParam { track: t, slot, .. }
                | FxMix { track: t, slot }
                | FxEnabled { track: t, slot } = &mut env.target
                    && *t as usize == track
                {
                    *slot = moved_slot_index(*slot as usize, from, to) as u8;
                }
            }
        }
        true
    }

    /// Remove an insert and its automation, keeping other routes attached.
    pub fn remove_track(&mut self, at: usize) {
        if at == 0 || at >= self.mixer.len() {
            return;
        }
        self.mixer.remove(at);
        for inst in &mut self.instruments {
            if inst.mixer as usize == at {
                inst.mixer = 0;
            } else if inst.mixer as usize > at {
                inst.mixer -= 1;
            }
            if let InstrumentKind::Plugin { plugin } = &mut inst.kind {
                for o in &mut plugin.outs {
                    if *o as usize == at {
                        *o = 0;
                    } else if *o as usize > at {
                        *o -= 1;
                    }
                }
            }
        }
        for track in &mut self.mixer {
            track.sends.retain(|s| s.to as usize != at);
            for send in &mut track.sends {
                if send.to as usize > at {
                    send.to -= 1;
                }
            }
            for f in &mut track.fx {
                f.sidechain = match f.sidechain {
                    Some(k) if k as usize == at => None,
                    Some(k) if k as usize > at => Some(k - 1),
                    k => k,
                };
            }
        }
        for pattern in self
            .patterns
            .iter_mut()
            .filter(|p| p.automation.is_some())
            .map(std::sync::Arc::make_mut)
        {
            if let Some(env) = &mut pattern.automation {
                use crate::automation::Target::*;
                match &mut env.target {
                    MixerVolume { track }
                    | MixerPan { track }
                    | MixerSep { track }
                    | MixerEq { track, .. }
                    | FxParam { track, .. }
                    | FxMix { track, .. }
                    | FxEnabled { track, .. } => {
                        if *track as usize == at {
                            pattern.automation = None;
                        } else if *track as usize > at {
                            *track -= 1;
                        }
                    }
                    SendLevel { track, to } => {
                        if *track as usize == at || *to as usize == at {
                            pattern.automation = None;
                        } else {
                            if *track as usize > at {
                                *track -= 1;
                            }
                            if *to as usize > at {
                                *to -= 1;
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
    }

    pub fn demo() -> Self {
        let mut song = Self::default();
        let mut add = |inst: u64, steps: &[usize], vel: u8| {
            for &s in steps {
                song.add_note(1, Note::new(inst, s as Tick * STEP, STEP, STEP_PITCH, vel));
            }
        };
        add(1, &[0, 4, 8, 12], 100);
        add(2, &[4, 12], 100);
        add(3, &[0, 2, 4, 6, 8, 10, 12], 100);
        add(3, &[14], 127);
        song.add_note(1, Note::new(4, 0, BAR, 48, 100));
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .slides
            .push(Glide {
                start: 6 * STEP,
                length: 4 * STEP,
                target: 60,
                group: 0,
                instrument: 4,
            });
        song
    }

    pub fn pattern(&self, id: u64) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id).map(|p| &**p)
    }

    /// Mutable pattern by id. Copies it first only while another snapshot (the
    /// audio thread, the undo history) still shares it.
    pub fn pattern_mut(&mut self, id: u64) -> Option<&mut Pattern> {
        self.patterns
            .iter_mut()
            .find(|p| p.id == id)
            .map(std::sync::Arc::make_mut)
    }

    pub fn instrument(&self, id: u64) -> Option<&Instrument> {
        self.instruments.iter().find(|i| i.id == id)
    }

    /// Next unused note id across the project.
    pub fn next_note_id(&self) -> u64 {
        // Lyrics of deleted notes stay (a cut can be pasted back), so their ids stay taken.
        let notes = self
            .patterns
            .iter()
            .flat_map(|p| p.notes.iter().map(|n| n.id));
        let lyrics = self.patterns.iter().flat_map(|p| p.lyrics.keys().copied());
        let curves = self.patterns.iter().flat_map(|p| p.pitch.keys().copied());
        notes.chain(lyrics).chain(curves).max().unwrap_or(0) + 1
    }

    /// Add a note to pattern `pattern` with a fresh id. Returns the id.
    pub fn add_note(&mut self, pattern: u64, mut note: Note) -> u64 {
        // A copy of a note (its old id still on it) keeps the original's lyric.
        let lyric = (note.id != 0)
            .then(|| {
                self.patterns
                    .iter()
                    .find_map(|p| p.lyrics.get(&note.id).cloned())
            })
            .flatten();
        let curve = (note.id != 0)
            .then(|| {
                self.patterns
                    .iter()
                    .find_map(|p| p.pitch.get(&note.id).cloned())
            })
            .flatten();
        note.id = self.next_note_id();
        let id = note.id;
        if let Some(p) = self.pattern_mut(pattern) {
            if let Some(l) = lyric {
                p.lyrics.insert(id, l);
            }
            if let Some(c) = curve {
                p.pitch.insert(id, c);
            }
            p.notes.push(note);
        }
        id
    }

    /// Give every note a unique id (files written by hand or older builds).
    pub fn ensure_note_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_note_id();
        for p in &mut self.patterns {
            let mut own = std::collections::HashSet::new();
            if p.notes
                .iter()
                .all(|n| n.id != 0 && !seen.contains(&n.id) && own.insert(n.id))
            {
                seen.extend(own);
                continue;
            }
            let p = std::sync::Arc::make_mut(p);
            for n in &mut p.notes {
                if n.id == 0 || !seen.insert(n.id) {
                    n.id = next;
                    seen.insert(next);
                    next += 1;
                }
            }
        }
    }

    /// Longest release among sampler instruments (loop seam + render tail).
    pub fn max_release(&self) -> f64 {
        self.instruments
            .iter()
            .filter_map(|i| i.sampler_preset())
            .map(|p| p.release())
            .fold(0.015, f64::max)
    }

    /// Detach a clip: copy its pattern and point only this clip at the copy.
    pub fn unique(&mut self, index: usize) -> Result<u64, &'static str> {
        let id = self.clips.get(index).ok_or("Clip missing")?.pattern;
        let mut p = self.pattern(id).ok_or("Pattern missing")?.clone();
        p.id = self
            .patterns
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("No pattern IDs left")?;
        p.name = format!("Pattern {}", p.id);
        p.renumber_notes(self.next_note_id());
        let new = p.id;
        self.patterns.push(p.into());
        self.clips[index].pattern = new;
        Ok(new)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 2 || self.patterns.is_empty() {
            return Err("Unsupported or empty project");
        }
        if self.loop_range.is_some_and(|(a, b)| a < 0 || b <= a) {
            return Err("Bad loop range");
        }
        let i = &self.info;
        if [&i.title, &i.author, &i.album, &i.genre, &i.copyright]
            .iter()
            .any(|s| s.chars().count() > ProjectInfo::MAX_FIELD)
            || i.year.chars().count() > ProjectInfo::MAX_YEAR
            || i.url.chars().count() > ProjectInfo::MAX_URL
            || i.comments.chars().count() > ProjectInfo::MAX_COMMENTS
        {
            return Err("Project info too long");
        }
        if i.scale.is_some_and(|sc| sc.root >= 12) {
            return Err("Bad project scale");
        }
        if self
            .markers
            .iter()
            .any(|m| m.tick < 0 || m.name.chars().count() > 64)
            || !self.markers.is_sorted_by_key(|m| m.tick)
        {
            return Err("Bad marker");
        }
        if !(40.0..=300.0).contains(&self.tempo) || !self.tempo.is_finite() {
            return Err("Bad tempo");
        }
        if !(0.0..=0.6).contains(&self.swing) || !self.swing.is_finite() {
            return Err("Bad swing");
        }
        crate::tempo::validate_tempo_map(&self.tempo_map)?;
        crate::tempo::validate_meters(&self.meters)?;
        for (i, inst) in self.instruments.iter().enumerate() {
            if self.instruments[..i].iter().any(|x| x.id == inst.id) {
                return Err("Duplicate channel ID");
            }
            if !(0.0..=FADER_MAX).contains(&inst.volume) || !(-1.0..=1.0).contains(&inst.pan) {
                return Err("Bad channel volume or pan");
            }
            if let Some(p) = inst.plugin()
                && (p.outs.len() > crate::plugin::MAX_EXTRA_PAIRS
                    || p.outs.iter().any(|&o| o as usize >= self.mixer.len()))
            {
                return Err("Plugin output routed to a missing mixer track");
            }
            if let Some(v) = inst.drum_voice()
                && v as usize >= DRUM_VOICES.len()
            {
                return Err("Unknown drum voice");
            }
            if let Some(preset) = inst.sampler_preset() {
                preset.validate()?;
            }
            if let InstrumentKind::Synth { patch } = &inst.kind {
                patch.validate()?;
            }
        }
        let known = |id: u64| self.instruments.iter().any(|i| i.id == id);
        for (i, p) in self.patterns.iter().enumerate() {
            if self.patterns[..i].iter().any(|x| x.id == p.id) {
                return Err("Duplicate pattern ID");
            }
            if p.length <= 0 || p.length % STEP != 0 || p.length > MAX_BARS * BAR {
                return Err("Bad pattern length");
            }
            if p.loop_range
                .is_some_and(|(a, b)| a < 0 || b <= a || b > MAX_BARS * BAR)
            {
                return Err("Bad pattern loop");
            }
            if p.notes.iter().any(|n| !known(n.instrument))
                || p.slides.iter().any(|s| !known(s.instrument))
                || p.audio_clip.is_some_and(|id| !known(id))
            {
                return Err("Unknown instrument");
            }
            let bad_note = |n: &Note| {
                n.start < 0
                    || n.length <= 0
                    || n.end() > p.length
                    || n.pitch > 127
                    || !(1..=127).contains(&n.velocity)
                    || !(-1.0..=1.0).contains(&n.pan)
                    || !(0.0..=1.0).contains(&n.release)
                    || !(-100.0..=100.0).contains(&n.fine)
                    || !(0.0..=1.0).contains(&n.mod_x)
                    || !(0.0..=1.0).contains(&n.mod_y)
                    || n.color > 15
            };
            let bad_slide = |s: &Glide| {
                s.start < 0 || s.length <= 0 || s.start + s.length > p.length || s.target > 127
            };
            if p.notes.iter().any(bad_note) || p.slides.iter().any(bad_slide) {
                return Err("Invalid note or slide");
            }
        }
        if self.clips.iter().any(|c| {
            c.start < 0
                || c.start >= MAX_BARS * BAR
                || c.track > 999
                || c.offset < 0
                || c.length.is_some_and(|l| l <= 0 || l > MAX_BARS * BAR)
                || self.pattern(c.pattern).is_none_or(|p| c.offset >= p.length)
        }) {
            return Err("Invalid clip");
        }
        if self.tracks.len() > 1000 {
            return Err("Too many tracks");
        }
        if self.mixer.is_empty() || self.mixer.len() > 128 {
            return Err("Bad mixer");
        }
        for (i, m) in self.mixer.iter().enumerate() {
            if !(0.0..=FADER_MAX).contains(&m.volume)
                || !(-1.0..=1.0).contains(&m.pan)
                || m.fx.len() > FX_SLOTS
                || m.eq.iter().any(|g| !g.is_finite() || g.abs() > EQ_RANGE)
                || !(-1.0..=1.0).contains(&m.sep)
            {
                return Err("Bad mixer track");
            }
            for slot in &m.fx {
                slot.validate()?;
                if slot
                    .sidechain
                    .is_some_and(|k| k == 0 || k as usize == i || k as usize >= self.mixer.len())
                {
                    return Err("Bad sidechain source");
                }
            }
            if m.sends.iter().any(|s| {
                (s.to as usize) == i
                    || s.to as usize >= self.mixer.len()
                    || !(0.0..=1.0).contains(&s.level)
            }) {
                return Err("Bad mixer send");
            }
        }
        if mixer_has_loop(&self.mixer) {
            return Err("Bad mixer send");
        }
        if self
            .instruments
            .iter()
            .any(|i| i.mixer as usize >= self.mixer.len())
        {
            return Err("Channel routed to a missing insert");
        }
        Ok(())
    }

    /// Add a channel. Returns its id.
    pub fn add_channel(&mut self, name: &str, kind: InstrumentKind) -> u64 {
        let id = self.instruments.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        self.instruments.push(Instrument::new(id, name, kind));
        id
    }

    /// Every hosted plugin in the project: channels, then mixer slots.
    pub fn plugin_refs(&self) -> Vec<&PluginRef> {
        let mut v: Vec<&PluginRef> = self.instruments.iter().filter_map(|i| i.plugin()).collect();
        v.extend(
            self.mixer
                .iter()
                .flat_map(|t| t.fx.iter().filter_map(|f| f.plugin.as_ref())),
        );
        v
    }

    pub fn plugin_ref_mut(&mut self, id: u64) -> Option<&mut PluginRef> {
        for i in &mut self.instruments {
            if let InstrumentKind::Plugin { plugin } = &mut i.kind
                && plugin.id == id
            {
                return Some(plugin);
            }
        }
        self.mixer
            .iter_mut()
            .flat_map(|t| t.fx.iter_mut().filter_map(|f| f.plugin.as_mut()))
            .find(|p| p.id == id)
    }

    pub fn next_plugin_id(&self) -> u64 {
        self.plugin_refs().iter().map(|p| p.id).max().unwrap_or(0) + 1
    }

    /// Reassign duplicate (or zero) hosted plugin instance IDs across channels
    /// and mixer effect slots so each channel/slot owns an independent instance.
    pub fn ensure_unique_plugin_ids(&mut self) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_plugin_id();
        let mut fixed = 0;
        for inst in &mut self.instruments {
            if let InstrumentKind::Plugin { plugin } = &mut inst.kind
                && (plugin.id == 0 || !seen.insert(plugin.id))
            {
                plugin.id = next;
                seen.insert(next);
                next += 1;
                fixed += 1;
            }
        }
        for track in &mut self.mixer {
            for slot in &mut track.fx {
                if let Some(plugin) = &mut slot.plugin
                    && (plugin.id == 0 || !seen.insert(plugin.id))
                {
                    plugin.id = next;
                    seen.insert(next);
                    next += 1;
                    fixed += 1;
                }
            }
        }
        fixed
    }

    pub fn add_sampler(&mut self, name: &str) -> u64 {
        self.add_channel(name, InstrumentKind::Sampler { preset: None })
    }

    /// `base`, or `base 2`, `base 3`… whichever is free.
    pub fn free_name(&self, base: &str) -> String {
        std::iter::once(base.to_string())
            .chain((2..).map(|n| format!("{base} {n}")))
            .find(|name| !self.instruments.iter().any(|i| &i.name == name))
            .expect("unbounded")
    }

    /// Next free "Sampler N" name.
    pub fn next_sampler_name(&self) -> String {
        (1..)
            .map(|n| format!("Sampler {n}"))
            .find(|name| !self.instruments.iter().any(|i| &i.name == name))
            .expect("unbounded")
    }

    /// Load a preset into a sampler channel. Errors if the id is not a sampler.
    pub fn set_sampler_preset(
        &mut self,
        id: u64,
        preset: SamplePreset,
    ) -> Result<(), &'static str> {
        match self
            .instruments
            .iter_mut()
            .find(|i| i.id == id)
            .map(|i| &mut i.kind)
        {
            Some(InstrumentKind::Sampler { preset: slot }) => {
                *slot = Some(std::sync::Arc::new(preset));
                Ok(())
            }
            Some(_) => Err("Only a sampler channel takes a preset"),
            None => Err("Channel missing"),
        }
    }

    /// Remove a channel with every note, slide and audio-clip pattern it plays.
    pub fn remove_instrument(&mut self, id: u64) -> Result<(), &'static str> {
        if !self.instruments.iter().any(|i| i.id == id) {
            return Err("Channel missing");
        }
        self.instruments.retain(|i| i.id != id);
        let dead_pats: Vec<u64> = self
            .patterns
            .iter()
            .filter(|p| p.audio_clip == Some(id))
            .map(|p| p.id)
            .collect();
        if !dead_pats.is_empty() {
            self.clips.retain(|c| !dead_pats.contains(&c.pattern));
            self.patterns.retain(|p| !dead_pats.contains(&p.id));
            if self.patterns.is_empty() {
                self.patterns.push(Pattern::new(1, "Pattern 1").into());
            }
        }
        for p in self.patterns.iter_mut().filter(|p| {
            p.notes.iter().any(|n| n.instrument == id)
                || p.slides.iter().any(|s| s.instrument == id)
        }) {
            let p = std::sync::Arc::make_mut(p);
            p.notes.retain(|n| n.instrument != id);
            p.slides.retain(|s| s.instrument != id);
        }
        Ok(())
    }

    /// Whole-step pattern length covering `seconds` at the song's tempo,
    /// clamped to `STEP ..= MAX_BARS * BAR`.
    pub fn ticks_for_seconds(&self, seconds: f64) -> Tick {
        let sps = seconds_per_step(self.tempo);
        let steps_needed = (seconds.max(0.0) / sps).ceil() as Tick;
        (steps_needed.max(1) * STEP).min(MAX_BARS * BAR)
    }

    /// Create an audio clip (FL-style): a sampler channel holding `preset`,
    /// a dedicated audio-clip pattern sized to the audio duration, and a
    /// playlist clip at `(start, track)`. Returns `(inst_id, pattern_id, clip_index)`.
    pub fn add_audio_clip(
        &mut self,
        name: &str,
        preset: SamplePreset,
        start: Tick,
        track: u32,
    ) -> Result<(u64, u64, usize), &'static str> {
        let seconds = preset.zones.first().map_or(0.0, |z| {
            z.end.saturating_sub(z.offset) as f64 / z.sample_rate.max(1) as f64
        });
        let base_name = if name.trim().is_empty() {
            "Audio clip"
        } else {
            name.trim()
        };
        let chan_name = self.free_name(base_name);
        let inst_id = self.add_sampler(&chan_name);
        self.set_sampler_preset(inst_id, preset)?;
        let pat_id = self.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut pat = Pattern::new(pat_id, &chan_name);
        pat.length = self.ticks_for_seconds(seconds);
        pat.audio_clip = Some(inst_id);
        self.patterns.push(std::sync::Arc::new(pat));
        let start = start.clamp(0, MAX_BARS * BAR - STEP);
        let track = track.min(999);
        self.clips.push(Clip::new(pat_id, start, track));
        Ok((inst_id, pat_id, self.clips.len() - 1))
    }

    /// FL solo: hear only this channel; soloing it again unmutes everything.
    pub fn solo(&mut self, id: u64) {
        let alone = self.instruments.iter().all(|i| i.muted == (i.id != id));
        for i in &mut self.instruments {
            i.muted = !alone && i.id != id;
        }
    }

    pub fn is_solo(&self, id: u64) -> bool {
        self.instruments.len() > 1 && self.instruments.iter().all(|i| i.muted == (i.id != id))
    }

    /// Ticks a clip plays.
    pub fn clip_len(&self, c: &Clip) -> Tick {
        c.length
            .unwrap_or_else(|| {
                self.pattern(c.pattern)
                    .map(|p| p.length - c.offset)
                    .unwrap_or(BAR)
            })
            .max(1)
    }

    pub fn track_muted(&self, track: u32) -> bool {
        self.tracks.get(track as usize).is_some_and(|t| t.muted)
    }

    /// Track name as shown: its own, or "Track N".
    pub fn track_name(&self, track: u32) -> String {
        match self.tracks.get(track as usize) {
            Some(t) if !t.name.is_empty() => t.name.clone(),
            _ => format!("Track {}", track + 1),
        }
    }

    /// Mutable track entry, created on demand.
    pub fn track_mut(&mut self, track: u32) -> &mut Track {
        if self.tracks.len() <= track as usize {
            self.tracks.resize(track as usize + 1, Track::default());
        }
        &mut self.tracks[track as usize]
    }

    /// End of the last clip in ticks.
    pub fn end(&self) -> Tick {
        self.clips
            .iter()
            .map(|c| c.start + self.clip_len(c))
            .max()
            .unwrap_or(0)
    }

    /// Song length in whole bars (at least one).
    pub fn bars(&self) -> u32 {
        if self.meters.is_empty() {
            ((self.end() + BAR - 1) / BAR).max(1) as u32
        } else {
            crate::tempo::bars(&self.meters, self.end())
        }
    }

    /// First tick of bar `bar`, time signatures counted.
    pub fn bar_tick(&self, bar: u32) -> Tick {
        if self.meters.is_empty() {
            bar as Tick * BAR
        } else {
            crate::tempo::bar_start(&self.meters, bar)
        }
    }

    /// The song's end, rounded up to a whole bar (its playing length).
    pub fn end_bar_tick(&self) -> Tick {
        self.bar_tick(self.bars())
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    fn chain() -> Song {
        let mut song = Song::default();
        song.mixer[1].route_only(2);
        song.mixer[2].route_only(3);
        song
    }

    #[test]
    fn loop_check_follows_sends_and_master() {
        let song = chain();
        assert!(
            route_would_loop(&song.mixer, 3, 1),
            "3 -> 1 closes 1 -> 2 -> 3"
        );
        assert!(
            route_would_loop(&song.mixer, 2, 2),
            "a track cannot feed itself"
        );
        assert!(
            route_would_loop(&song.mixer, 0, 1),
            "Master has no output to route"
        );
        assert!(
            !route_would_loop(&song.mixer, 1, 3),
            "1 -> 3 keeps the flow forward"
        );
        assert!(
            !route_would_loop(&song.mixer, 3, 0),
            "Master ends every chain"
        );
    }

    #[test]
    fn replace_keeps_an_existing_send_level() {
        let mut song = Song::default();
        song.mixer[1].sends = vec![Send { to: 2, level: 0.4 }, Send { to: 4, level: 0.2 }];
        song.mixer[1].route_only(2);
        assert_eq!(song.mixer[1].sends, vec![Send { to: 2, level: 0.4 }]);
        assert!(!song.mixer[1].to_master);
        song.mixer[1].route_only(0);
        assert!(song.mixer[1].to_master && song.mixer[1].sends.is_empty());
    }

    #[test]
    fn add_send_keeps_the_old_level_and_master_means_the_route() {
        let mut song = Song::default();
        song.mixer[1].to_master = false;
        assert!(song.mixer[1].add_send(3));
        assert!(
            !song.mixer[1].add_send(3),
            "the same send twice changes nothing"
        );
        assert_eq!(song.mixer[1].sends, vec![Send { to: 3, level: 1.0 }]);
        assert!(song.mixer[1].add_send(0));
        assert!(song.mixer[1].to_master && song.mixer[1].sends.len() == 1);
        song.mixer[1].clear_sends();
        assert!(song.mixer[1].sends.is_empty() && song.mixer[1].to_master);
    }

    #[test]
    fn backwards_sends_pass_and_loops_fail_validation() {
        let mut song = chain();
        song.mixer[3].sends.push(Send { to: 1, level: 1.0 });
        assert!(mixer_has_loop(&song.mixer));
        assert!(song.validate().is_err(), "1 -> 2 -> 3 -> 1 loops");
        song.mixer[3].clear_sends();
        song.mixer[3].sends.push(Send { to: 1, level: 0.5 });
        song.mixer[2].route_only(0);
        assert!(!mixer_has_loop(&song.mixer));
        assert!(
            song.validate().is_ok(),
            "3 -> 1 alone flows backwards but never back"
        );
    }
}

#[cfg(test)]
mod mixer_edit_tests {
    use super::*;
    use crate::automation::{Envelope, Target};

    #[test]
    fn insert_and_remove_remap_routes_and_automation() {
        let mut song = Song::default();
        song.instruments[0].mixer = 2;
        song.instruments[1].mixer = 4;
        song.mixer[1].sends = vec![Send { to: 2, level: 0.5 }, Send { to: 4, level: 1.0 }];
        for target in [
            Target::MixerVolume { track: 2 },
            Target::MixerPan { track: 4 },
            Target::FxParam {
                track: 4,
                slot: 0,
                param: 0,
            },
        ] {
            let mut pattern = Pattern::new(100 + song.patterns.len() as u64, "Automation");
            pattern.automation = Some(Envelope::flat(target, 0.5, BAR));
            song.patterns.push(pattern.into());
        }
        let first = song.patterns.len() - 3;
        assert_eq!(song.insert_track(2), 2);
        assert_eq!(
            (song.instruments[0].mixer, song.instruments[1].mixer),
            (3, 5)
        );
        assert_eq!(
            song.mixer[1].sends.iter().map(|s| s.to).collect::<Vec<_>>(),
            [3, 5]
        );
        assert_eq!(
            song.patterns[first].automation.as_ref().unwrap().target,
            Target::MixerVolume { track: 3 }
        );
        assert_eq!(
            song.patterns[first + 1].automation.as_ref().unwrap().target,
            Target::MixerPan { track: 5 }
        );
        assert_eq!(
            song.patterns[first + 2].automation.as_ref().unwrap().target,
            Target::FxParam {
                track: 5,
                slot: 0,
                param: 0
            }
        );
        song.remove_track(3);
        assert_eq!(
            (song.instruments[0].mixer, song.instruments[1].mixer),
            (0, 4)
        );
        assert_eq!(
            song.mixer[1].sends.iter().map(|s| s.to).collect::<Vec<_>>(),
            [4]
        );
        assert!(song.patterns[first].automation.is_none());
        assert_eq!(
            song.patterns[first + 1].automation.as_ref().unwrap().target,
            Target::MixerPan { track: 4 }
        );
        assert_eq!(
            song.patterns[first + 2].automation.as_ref().unwrap().target,
            Target::FxParam {
                track: 4,
                slot: 0,
                param: 0
            }
        );
        let count = song.mixer.len();
        song.remove_track(0);
        assert_eq!(song.mixer.len(), count);
    }
}

pub(crate) fn drum(voice: usize, t: f64) -> f64 {
    if !(0.0..0.25).contains(&t) {
        return 0.0;
    }
    match voice {
        0 => {
            (TAU * (48.0 * t + 70.0 * (1.0 - (-t * 22.0).exp()) / 22.0)).sin()
                * (-t * 24.0).exp()
                * 0.36
        }
        1 => {
            ((TAU * 180.0 * t).sin() * 0.3
                + (TAU * 7311.0 * t + 3.0 * (TAU * 2333.0 * t).sin()).sin() * 0.7)
                * (-t * 24.0).exp()
                * 0.20
        }
        _ => (TAU * 9231.0 * t + 5.0 * (TAU * 3991.0 * t).sin()).sin() * (-t * 65.0).exp() * 0.09,
    }
}

/// Late offset (seconds) for a note starting exactly on an odd 16th step.
pub fn swing_offset(t: Tick, sps: f64, swing: f64) -> f64 {
    if t.rem_euclid(2 * STEP) == STEP {
        swing.clamp(0.0, 0.6) * sps
    } else {
        0.0
    }
}

/// One audible note occurrence placed on the song (or pattern) timeline,
/// matching [`crate::engine::Engine`]'s clip window, repeat, and mute rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipNote {
    /// Index in `song.clips` (`usize::MAX` in Pattern mode).
    pub clip: usize,
    /// Source pattern id (for looking up lyrics and pitch curves).
    pub pattern: u64,
    /// Original note data.
    pub note: Note,
    /// Timeline start tick (playlist tick in Song mode, pattern tick in Pattern mode).
    pub start: Tick,
    /// Timeline end tick: full note length for synths/samplers, clipped to clip
    /// window and next vocal note on the same channel for vocals.
    pub end: Tick,
    /// Local tick within the clip, used for swing alignment matching the engine.
    pub local_tick: Tick,
    /// Clip window end on the timeline.
    pub window_end: Tick,
}

/// Longest note length across the song's patterns, capped at 16 bars for fast lookback.
pub fn song_max_note_len(song: &Song) -> Tick {
    let mut best: Tick = BAR;
    for p in &song.patterns {
        for n in &p.notes {
            if n.length > best {
                best = n.length;
            }
        }
    }
    best.min(16 * BAR)
}

/// Collect audible note occurrences overlapping `[from, to)` in Song mode.
///
/// Mirrors `Engine::collect_hits`: clip offset/length/repeats decide note starts,
/// mute is checked across clip, track, note, and instrument, synth/sampler notes
/// ring for their full length past clip end, and vocal notes clip to the clip
/// window and monophonic phrase boundaries.
pub fn clip_notes(song: &Song, from: Tick, to: Tick, max_lookback: Tick) -> Vec<ClipNote> {
    if to <= from {
        return Vec::new();
    }
    let lookback = max_lookback.clamp(0, 16 * BAR);
    let mut out = Vec::new();
    for (ci, c) in song.clips.iter().enumerate() {
        if c.muted || song.track_muted(c.track) {
            continue;
        }
        let Some(pat) = song.pattern(c.pattern) else {
            continue;
        };
        if pat.notes.is_empty() {
            continue;
        }
        let len = song.clip_len(c);
        let clip_end = c.start.saturating_add(len);
        if c.start >= to || clip_end.saturating_add(lookback) <= from {
            continue;
        }
        let plen = pat.length.max(1);
        let w0 = c.offset;
        let w1 = w0.saturating_add(len);
        if w1 <= w0 {
            continue;
        }
        let clip_first = w0.div_euclid(plen);
        let clip_last = (w1 - 1).div_euclid(plen);
        let q_lo = (from.saturating_sub(lookback) - c.start).max(0);
        let q_hi = (to - c.start).min(len);
        if q_hi <= q_lo {
            continue;
        }
        let t_lo = w0.saturating_add(q_lo);
        let t_hi = w0.saturating_add(q_hi);
        let pass_lo = (t_lo - plen + 1).div_euclid(plen).max(clip_first);
        let pass_hi = (t_hi - 1).div_euclid(plen).min(clip_last);
        if pass_hi < pass_lo {
            continue;
        }
        // Guard against degenerate 1-tick patterns stretched over hundreds of bars.
        let pass_hi = pass_hi.min(pass_lo.saturating_add(4096));

        for n in &pat.notes {
            if n.muted {
                continue;
            }
            let Some(inst) = song.instrument(n.instrument) else {
                continue;
            };
            if inst.muted {
                continue;
            }
            let is_vocal = matches!(inst.kind, InstrumentKind::Vocal { .. });
            for k in pass_lo..=pass_hi {
                let t = n.start.saturating_add(k.saturating_mul(plen));
                if t < w0 || t >= w1 {
                    continue;
                }
                let local = t - w0;
                let start = c.start.saturating_add(local);
                let raw_end = start.saturating_add(n.length.max(1));
                let end = if is_vocal {
                    raw_end.min(clip_end)
                } else {
                    raw_end
                };
                if end <= from || start >= to {
                    continue;
                }
                out.push(ClipNote {
                    clip: ci,
                    pattern: c.pattern,
                    note: *n,
                    start,
                    end,
                    local_tick: local,
                    window_end: clip_end,
                });
            }
        }
    }
    finalize_clip_notes(song, &mut out);
    out
}

/// Collect audible note occurrences overlapping `[from, to)` for a single pattern
/// (Pattern mode fallback when the song has no playlist clips yet).
pub fn pattern_notes(song: &Song, pattern_id: u64, from: Tick, to: Tick) -> Vec<ClipNote> {
    if to <= from {
        return Vec::new();
    }
    let Some(pat) = song.pattern(pattern_id) else {
        return Vec::new();
    };
    let plen = pat.length.max(1);
    let mut out = Vec::new();
    for n in &pat.notes {
        if n.muted {
            continue;
        }
        let Some(inst) = song.instrument(n.instrument) else {
            continue;
        };
        if inst.muted {
            continue;
        }
        let start = n.start;
        let end = start.saturating_add(n.length.max(1)).min(plen);
        if end <= from || start >= to {
            continue;
        }
        out.push(ClipNote {
            clip: usize::MAX,
            pattern: pattern_id,
            note: *n,
            start,
            end,
            local_tick: start,
            window_end: plen,
        });
    }
    finalize_clip_notes(song, &mut out);
    out
}

fn finalize_clip_notes(song: &Song, out: &mut [ClipNote]) {
    out.sort_by_key(|cn| (cn.start, cn.note.instrument, cn.note.pitch, cn.clip));
    // Monophonic vocal clipping: each vocal note ends no later than the next note
    // on the same vocal channel, matching `vocal::plan`.
    let mut last_vocal: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for i in 0..out.len() {
        let inst_id = out[i].note.instrument;
        let is_vocal = song
            .instrument(inst_id)
            .is_some_and(|ins| matches!(ins.kind, InstrumentKind::Vocal { .. }));
        if !is_vocal {
            continue;
        }
        if let Some(&prev) = last_vocal.get(&inst_id) {
            let next_start = out[i].start;
            if out[prev].end > next_start {
                out[prev].end = next_start.max(out[prev].start + 1);
            }
        }
        last_vocal.insert(inst_id, i);
    }
}

/// Offline render through the voice engine (same code as live playback).
pub fn render(song: &Song, pattern_id: u64, song_mode: bool) -> Vec<[f32; 2]> {
    crate::engine::render(song, pattern_id, song_mode, RATE)
}

impl Song {
    /// Add a marker at `tick` (kept in order), named "Marker N" by count.
    /// Returns its index; an existing marker there is returned unchanged.
    pub fn add_marker(&mut self, tick: Tick) -> usize {
        if let Some(i) = self.markers.iter().position(|m| m.tick == tick) {
            return i;
        }
        let at = self.markers.partition_point(|m| m.tick < tick);
        let n = (1..)
            .find(|n| !self.markers.iter().any(|m| m.name == format!("Marker {n}")))
            .unwrap_or(1);
        self.markers.insert(
            at,
            Marker {
                tick: tick.max(0),
                name: format!("Marker {n}"),
            },
        );
        at
    }

    /// The span from marker `i` to the next one, or to the song's end.
    pub fn marker_span(&self, i: usize) -> Option<(Tick, Tick)> {
        let a = self.markers.get(i)?.tick;
        let b = self
            .markers
            .get(i + 1)
            .map(|m| m.tick)
            .unwrap_or_else(|| self.end_bar_tick().max(a + BAR));
        (b > a).then_some((a, b))
    }
}

/// Tick → frames at the song's flat tempo, or through `table`.
pub fn tick_frames(
    song: &Song,
    table: Option<&crate::tempo::TempoTable>,
    t: Tick,
    rate: u32,
) -> u64 {
    match table {
        Some(m) => m.frames_at(t) as u64,
        None => (steps(t) * seconds_per_step(song.tempo) * rate as f64) as u64,
    }
}

/// Where slot `k` ends up when slot `from` moves to `to` (the others shift).
pub fn moved_slot_index(k: usize, from: usize, to: usize) -> usize {
    if k == from {
        to
    } else if from < to && k > from && k <= to {
        k - 1
    } else if to < from && k >= to && k < from {
        k + 1
    } else {
        k
    }
}

/// The loop the transport plays as frames `start..end`: the song's time
/// selection in song mode when it has one, else the whole pattern or song.
pub fn loop_bounds(
    song: &Song,
    table: Option<&crate::tempo::TempoTable>,
    pattern_id: u64,
    song_mode: bool,
    rate: u32,
) -> (u64, u64) {
    if song_mode && let Some((a, b)) = song.loop_range {
        let (s, e) = (
            tick_frames(song, table, a, rate),
            tick_frames(song, table, b, rate),
        );
        if e > s {
            return (s, e);
        }
    }
    // Pattern mode: the pattern's own time selection, kept inside the pattern.
    if !song_mode
        && let Some(p) = song.pattern(pattern_id)
        && let Some((a, b)) = p.loop_range
    {
        let b = b.min(p.length);
        if b > a {
            return (
                tick_frames(song, None, a, rate),
                tick_frames(song, None, b, rate),
            );
        }
    }
    let len = match table {
        Some(m) if song_mode => m.frames_at(song.end_bar_tick()) as u64,
        _ => loop_frames(song, pattern_id, song_mode, rate),
    };
    (0, len.max(1))
}

/// A monotonic transport position folded into the loop `start..end`. Before
/// `start` the song plays through to it (a lead-in), then repeats the loop.
pub fn wrap_frame(pos: u64, (start, end): (u64, u64)) -> u64 {
    if pos < end {
        pos
    } else {
        start + (pos - start) % (end - start).max(1)
    }
}

/// Rebase a monotonic transport position when song loop bounds change mid-playback
/// (e.g. adding, moving, or deleting a clip at the end of the playlist) so the
/// wrapped playhead stays on the exact same frame instead of jumping via modulo.
pub fn rebase_loop_pos(pos: u64, prev: (u64, u64), next: (u64, u64)) -> u64 {
    if prev == next {
        return pos;
    }
    let cur = wrap_frame(pos, prev);
    if cur < next.1 {
        cur
    } else {
        wrap_frame(cur, next)
    }
}

/// Frames in one pass of the pattern, or of the playlist in song mode.
pub fn loop_frames(song: &Song, pattern_id: u64, song_mode: bool, rate: u32) -> u64 {
    if song_mode && let Some(map) = crate::tempo::effective_tempo_map(song) {
        let table = crate::tempo::TempoTable::build(&map, song.tempo, rate);
        return table.frames_at(song.end_bar_tick()) as u64;
    }
    let st = if song_mode {
        steps(song.end_bar_tick())
    } else {
        steps(song.pattern(pattern_id).map(|p| p.length).unwrap_or(BAR))
    };
    (st * seconds_per_step(song.tempo) * rate as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_add_load_and_remove_with_their_notes() {
        let mut s = Song::demo();
        let id = s.add_sampler(&s.next_sampler_name());
        assert_eq!(s.instruments.last().unwrap().name, "Sampler 1");
        s.add_note(1, Note::new(id, 2 * STEP, 2 * STEP, 60, 90));
        let preset = SamplePreset::test_preset("Keys");
        assert!(s.set_sampler_preset(id, preset.clone()).is_ok());
        assert_eq!(
            s.set_sampler_preset(1, preset),
            Err("Only a sampler channel takes a preset")
        );
        assert!(s.validate().is_ok());
        s.remove_instrument(id).unwrap();
        assert!(s.patterns[0].notes.iter().all(|n| n.instrument != id));
        // Drum channels are ordinary channels now.
        s.remove_instrument(1).unwrap();
        assert!(s.patterns[0].notes.iter().all(|n| n.instrument != 1));
        assert!(s.validate().is_ok());
    }

    #[test]
    fn unique_detaches_with_fresh_note_ids() {
        let mut s = Song {
            clips: vec![Clip::new(1, 0, 0), Clip::new(1, BAR, 1)],
            ..Song::demo()
        };
        let id = s.unique(1).unwrap();
        std::sync::Arc::make_mut(&mut s.patterns[1]).notes.clear();
        assert!(!s.patterns[0].notes.is_empty());
        assert_eq!(s.clips[1].pattern, id);
        assert!(s.validate().is_ok());
        let mut s = Song {
            clips: vec![Clip::new(1, 0, 0)],
            ..Song::demo()
        };
        s.unique(0).unwrap();
        let ids: std::collections::HashSet<u64> = s
            .patterns
            .iter()
            .flat_map(|p| p.notes.iter().map(|n| n.id))
            .collect();
        assert_eq!(
            ids.len(),
            s.patterns.iter().map(|p| p.notes.len()).sum::<usize>()
        );
    }

    #[test]
    fn muted_channel_renders_silence_and_solo_isolates() {
        let mut song = Song::default();
        song.add_note(1, Note::new(1, 0, STEP, 60, 100));
        let energy = |song: &Song| -> f64 {
            render(song, 1, false)
                .iter()
                .map(|v| (v[0].abs() + v[1].abs()) as f64)
                .sum()
        };
        assert!(energy(&song) > 0.0);
        song.instruments[0].muted = true;
        assert_eq!(energy(&song), 0.0);
        song.instruments[0].muted = false;
        song.add_note(1, Note::new(2, 4 * STEP, STEP, 60, 100));
        song.solo(2);
        assert!(song.is_solo(2) && song.instruments[0].muted);
        let soloed = energy(&song);
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .retain(|n| n.instrument != 1);
        assert_eq!(energy(&song), soloed);
        song.solo(2);
        assert!(
            song.instruments.iter().all(|i| !i.muted),
            "second solo unmutes all"
        );
    }

    #[test]
    fn off_grid_notes_and_long_patterns_are_valid() {
        let mut song = Song::default();
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 2 * BAR;
        // A triplet 8th (160 ticks) and a note 37 ticks late: representable now.
        song.add_note(1, Note::new(4, 160, 160, 60, 100));
        song.add_note(1, Note::new(4, BAR + 37, 300, 64, 100));
        song.validate().unwrap();
        assert!(!song.patterns[0].is_step_channel(4));
        assert_eq!(loop_frames(&song, 1, false, RATE), RATE as u64 * 4);
        let json = serde_json::to_string(&song).unwrap();
        let back: Song = serde_json::from_str(&json).unwrap();
        assert_eq!(back.patterns[0].notes, song.patterns[0].notes);
        // Past the pattern end is rejected.
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes[1].length = BAR;
        assert!(song.validate().is_err());
    }

    #[test]
    fn swing_moves_odd_steps_only() {
        let mut song = Song::default();
        song.add_note(1, Note::new(1, STEP, STEP, 60, 100));
        let dry = render(&song, 1, false);
        let mut swung = song.clone();
        swung.swing = 0.3;
        assert!(swung.validate().is_ok());
        assert_ne!(render(&swung, 1, false), dry);
        // An even step does not move.
        let mut even = Song::default();
        even.add_note(1, Note::new(1, 2 * STEP, STEP, 60, 100));
        let a = render(&even, 1, false);
        even.swing = 0.3;
        assert_eq!(render(&even, 1, false), a);
        swung.swing = 0.9;
        assert!(swung.validate().is_err());
    }

    #[test]
    fn auto_map_gives_each_output_pair_a_free_named_insert() {
        let mut song = Song::default();
        let r = PluginRef {
            id: 5,
            format: "vst".into(),
            path: "/x.so".into(),
            name: "SSD5".into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        let drums = song.add_channel("SSD5", InstrumentKind::Plugin { plugin: r });
        let taken: Vec<u16> = song.instruments.iter().map(|i| i.mixer).collect();
        song.mixer[3].name = "Vocals".into();
        assert_eq!(
            song.map_plugin_outputs(drums, 20),
            21,
            "main pair plus 20 extra"
        );
        let inst = song.instrument(drums).unwrap();
        let outs = inst.plugin().unwrap().outs.clone();
        assert_eq!(outs.len(), 20);
        assert_eq!(song.mixer[inst.mixer as usize].name, "SSD5 1-2");
        assert_eq!(song.mixer[outs[0] as usize].name, "SSD5 3-4");
        assert_eq!(song.mixer[outs[19] as usize].name, "SSD5 41-42");
        assert!(!outs.contains(&3), "a renamed insert is not free");
        for &o in &outs {
            assert!(
                !taken.contains(&o) || o == 0,
                "another channel's insert is not free"
            );
        }
        let mut all = outs.clone();
        all.push(inst.mixer);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 21, "every pair on its own insert");
        assert!(
            song.mixer.len() > INSERTS + 1,
            "inserts were added when too few were free"
        );
        song.validate().unwrap();
    }

    #[test]
    fn the_volume_taper_spreads_loudness_evenly_along_the_travel() {
        let db = |pos: f32| 20.0 * volume_gain(pos).log10();
        assert!(
            (volume_gain(volume_pos(1.0)) - 1.0).abs() < 1e-5,
            "unity round-trips"
        );
        assert!(
            (volume_pos(1.0) - 0.807).abs() < 0.002,
            "0 dB at 81% of the travel"
        );
        assert!((db(1.0) - 5.58).abs() < 0.01, "top is +5.6 dB like FL");
        assert!(
            (db(0.5) - -12.5).abs() < 0.1,
            "half travel is −12.5 dB: {}",
            db(0.5)
        );
        assert_eq!(volume_gain(0.0), 0.0);
        // Each quarter of the travel below unity takes a comparable bite in dB,
        // not the 50 dB a linear-gain knob put into its bottom quarter.
        let steps = [
            db(0.25) - db(0.0001),
            db(0.5) - db(0.25),
            db(0.75) - db(0.5),
        ];
        assert!(steps[1] > 15.0 && steps[2] > 8.0, "{steps:?}");
        assert_eq!(gain_db_text(0.0), "−∞ dB");
        assert_eq!(gain_db_text(0.5), "-6.0 dB");
    }

    #[test]
    fn channel_volume_and_pan_shape_the_output() {
        let mut song = Song::default();
        song.add_note(1, Note::new(1, 0, STEP, 60, 100));
        let full = render(&song, 1, false);
        song.instruments[0].pan = 1.0;
        let right = render(&song, 1, false);
        assert!(right.iter().all(|f| f[0] == 0.0));
        assert!(right.iter().zip(&full).all(|(r, f)| r[1] == f[1]));
        song.instruments[0].pan = 0.0;
        song.instruments[0].volume = 0.5;
        let half = render(&song, 1, false);
        let peak = |v: &[[f32; 2]]| v.iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);
        assert!((peak(&half) / peak(&full) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_time_selection_folds_the_transport_after_a_lead_in() {
        assert_eq!(
            wrap_frame(50, (100, 200)),
            50,
            "lead-in before the selection"
        );
        assert_eq!(wrap_frame(150, (100, 200)), 150);
        assert_eq!(wrap_frame(200, (100, 200)), 100);
        assert_eq!(wrap_frame(345, (100, 200)), 145);
        let mut song = Song::demo();
        song.loop_range = Some((BAR, 3 * BAR));
        let bar = loop_frames(&song, 1, false, RATE);
        assert_eq!(loop_bounds(&song, None, 1, true, RATE), (bar, 3 * bar));
        assert_eq!(
            loop_bounds(&song, None, 1, false, RATE),
            (0, bar),
            "pattern mode ignores it"
        );
        assert!(song.validate().is_ok());
        song.loop_range = Some((BAR, BAR));
        assert!(song.validate().is_err());
    }

    #[test]
    fn a_patterns_own_selection_loops_in_pattern_mode() {
        let mut song = Song::demo();
        let bar = loop_frames(&song, 1, false, RATE);
        let step = bar / 16;
        std::sync::Arc::make_mut(&mut song.patterns[0]).loop_range = Some((4 * STEP, 8 * STEP));
        assert_eq!(
            loop_bounds(&song, None, 1, false, RATE),
            (4 * step, 8 * step),
            "steps 5-8 loop"
        );
        assert_eq!(
            loop_bounds(&song, None, 1, true, RATE).0,
            0,
            "song mode ignores it"
        );
        assert!(song.validate().is_ok());
        // A span past the pattern's end stops at the end.
        std::sync::Arc::make_mut(&mut song.patterns[0]).loop_range = Some((8 * STEP, 40 * STEP));
        assert_eq!(loop_bounds(&song, None, 1, false, RATE), (8 * step, bar));
        std::sync::Arc::make_mut(&mut song.patterns[0]).loop_range = Some((8 * STEP, 8 * STEP));
        assert!(song.validate().is_err(), "an empty span is refused");
    }

    #[test]
    fn markers_stay_in_order_with_free_names_and_loop_to_the_next() {
        let mut song = Song::demo();
        song.clips = vec![Clip::new(1, 0, 0), Clip::new(1, 7 * BAR, 0)];
        assert_eq!(song.add_marker(4 * BAR), 0);
        assert_eq!(song.add_marker(BAR), 0);
        assert_eq!(song.add_marker(4 * BAR), 1, "one marker per position");
        let names: Vec<_> = song
            .markers
            .iter()
            .map(|m| (m.tick, m.name.as_str()))
            .collect();
        assert_eq!(names, [(BAR, "Marker 2"), (4 * BAR, "Marker 1")]);
        assert_eq!(song.marker_span(0), Some((BAR, 4 * BAR)));
        assert_eq!(
            song.marker_span(1),
            Some((4 * BAR, 8 * BAR)),
            "the last one runs to the song's end"
        );
        assert!(song.validate().is_ok());
        let back: Song = serde_json::from_str(&serde_json::to_string(&song).unwrap()).unwrap();
        assert_eq!(back.markers, song.markers);
        song.markers.swap(0, 1);
        assert!(song.validate().is_err(), "out of order");
    }

    #[test]
    fn project_info_is_saved_only_when_filled_in() {
        let mut song = Song::demo();
        assert!(
            !serde_json::to_string(&song).unwrap().contains("\"info\""),
            "an empty info is not written"
        );
        song.info = ProjectInfo {
            title: "Нічне місто".into(),
            author: "Me".into(),
            album: "Night Sessions".into(),
            genre: "House".into(),
            year: "2026".into(),
            copyright: "CC0".into(),
            url: "https://example.com/night".into(),
            comments: "808 from kit 3".into(),
            show_on_open: true,
            scale: Some(crate::scale::Scale {
                root: 9,
                kind: crate::scale::Kind::Minor,
            }),
        };
        let back: Song = serde_json::from_str(&serde_json::to_string(&song).unwrap()).unwrap();
        assert_eq!(back.info, song.info);
        assert!(back.validate().is_ok());

        // Older 4-field JSON still deserializes cleanly with defaults for new fields.
        let legacy: ProjectInfo = serde_json::from_str(
            r#"{"title":"Old","author":"A","genre":"Ambient","comments":"Note"}"#,
        )
        .unwrap();
        assert_eq!(legacy.title, "Old");
        assert!(legacy.album.is_empty());
        assert!(!legacy.show_on_open);
        assert_eq!(legacy.scale, None);

        song.info.title = "x".repeat(ProjectInfo::MAX_FIELD + 1);
        assert!(song.validate().is_err());
        song.info.title = "ok".into();
        song.info.year = "2".repeat(ProjectInfo::MAX_YEAR + 1);
        assert!(song.validate().is_err());
        song.info.year = "2026".into();
        song.info.url = "h".repeat(ProjectInfo::MAX_URL + 1);
        assert!(song.validate().is_err());
        song.info.url.clear();
        song.info.scale = Some(crate::scale::Scale {
            root: 12,
            kind: crate::scale::Kind::Major,
        });
        assert!(song.validate().is_err());
    }

    #[test]
    fn tempo_scales_loop_length() {
        let slow = Song::default();
        assert_eq!(loop_frames(&slow, 1, false, RATE), RATE as u64 * 2);
        let mut fast = Song::default();
        fast.tempo = 140.0;
        assert!(fast.validate().is_ok());
        assert_eq!(
            loop_frames(&fast, 1, false, RATE),
            (2.0 * 120.0 / 140.0 * RATE as f64) as u64
        );
        let mut bad = Song::default();
        bad.tempo = f64::NAN;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn edited_variation_survives_roundtrip_and_changes_render() {
        let mut song = Song::demo();
        song.clips = vec![Clip::new(1, 0, 0), Clip::new(1, BAR, 1)];
        song.unique(1).unwrap();
        song.add_note(2, Note::new(2, 4 * STEP, STEP, 60, 100));
        song.add_note(2, Note::new(2, 5 * STEP, STEP, 60, 100));
        let restored: Song = serde_json::from_slice(&serde_json::to_vec(&song).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_ne!(restored.clips[0].pattern, restored.clips[1].pattern);
        assert_eq!(loop_frames(&restored, 1, true, RATE), RATE as u64 * 4);
        let frames = render(&restored, 1, true);
        // Bar 2 (the edited copy) differs from bar 1 at step 6.
        let t = (0.70 * RATE as f64) as usize;
        assert_ne!(frames[t], frames[t + 2 * RATE as usize]);
        let file = tempfile::NamedTempFile::new().unwrap();
        crate::write_wav(file.path(), &frames).unwrap();
        let wav = hound::WavReader::open(file.path()).unwrap();
        assert_eq!(wav.duration() as usize, frames.len());
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    fn energy_in(v: &[[f32; 2]], from: f64, to: f64) -> f64 {
        let (a, b) = (
            (from * RATE as f64) as usize,
            ((to * RATE as f64) as usize).min(v.len()),
        );
        v[a..b]
            .iter()
            .map(|f| (f[0].abs() + f[1].abs()) as f64)
            .sum()
    }

    #[test]
    fn clip_window_plays_only_its_part_of_the_pattern() {
        // Kick on beats 1 and 3; a clip that starts at beat 3 and lasts one beat.
        let mut song = Song::default();
        song.add_note(1, Note::new(1, 0, STEP, 60, 100));
        song.add_note(1, Note::new(1, 8 * STEP, STEP, 60, 100));
        song.clips.push(Clip {
            offset: 8 * STEP,
            length: Some(4 * STEP),
            ..Clip::new(1, 0, 0)
        });
        song.validate().unwrap();
        assert_eq!(song.end(), 4 * STEP);
        let out = render(&song, 1, true);
        // 120 BPM: one beat = 0.5 s. The beat-3 kick now sounds at 0 s; nothing at 1 s.
        assert!(energy_in(&out, 0.0, 0.2) > 1.0);
        assert_eq!(energy_in(&out, 0.3, 2.0), 0.0);
    }

    #[test]
    fn muted_clips_and_tracks_are_silent() {
        let mut song = Song::default();
        song.add_note(1, Note::new(1, 0, STEP, 60, 100));
        song.clips.push(Clip::new(1, 0, 2));
        let loud = render(&song, 1, true);
        assert!(energy_in(&loud, 0.0, 0.2) > 1.0);
        song.track_mut(2).muted = true;
        assert_eq!(energy_in(&render(&song, 1, true), 0.0, 2.0), 0.0);
        song.tracks.clear();
        song.clips[0].muted = true;
        assert_eq!(energy_in(&render(&song, 1, true), 0.0, 2.0), 0.0);
        assert_eq!(song.track_name(2), "Track 3");
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    use std::sync::Arc;

    fn big_song(patterns: u64, notes: i64) -> Song {
        let mut song = Song::default();
        song.patterns.clear();
        for id in 1..=patterns {
            let mut p = Pattern::new(id, &format!("P{id}"));
            for k in 0..notes {
                p.notes.push(Note::new(1, k * STEP, STEP, 60, 100));
            }
            song.patterns.push(p.into());
        }
        song
    }

    #[test]
    fn editing_one_pattern_copies_only_that_pattern() {
        let mut song = big_song(5, 50);
        let snapshot = song.clone(); // what the audio thread / undo holds
        song.pattern_mut(3).unwrap().notes[0].pitch = 72;
        for (old, new) in snapshot.patterns.iter().zip(&song.patterns) {
            assert_eq!(Arc::ptr_eq(old, new), old.id != 3, "pattern {}", old.id);
        }
        assert_eq!(snapshot.pattern(3).unwrap().notes[0].pitch, 60);
        assert_eq!(song.pattern(3).unwrap().notes[0].pitch, 72);
        // Unshared: edited in place, no copy.
        let before = Arc::as_ptr(&song.patterns[2]);
        drop(snapshot);
        song.pattern_mut(3).unwrap().notes[0].pitch = 74;
        assert_eq!(Arc::as_ptr(&song.patterns[2]), before);
    }

    #[test]
    fn removing_a_channel_leaves_unrelated_patterns_shared() {
        let mut song = big_song(3, 10);
        let other = song.add_sampler("Other");
        song.add_note(2, Note::new(other, 0, STEP, 60, 100));
        let snapshot = song.clone();
        song.remove_instrument(other).unwrap();
        assert!(Arc::ptr_eq(&snapshot.patterns[0], &song.patterns[0]));
        assert!(!Arc::ptr_eq(&snapshot.patterns[1], &song.patterns[1]));
        assert!(song.patterns[1].notes.iter().all(|n| n.instrument != other));
    }

    #[test]
    fn rebase_loop_pos_keeps_playhead_steady_when_song_length_changes() {
        let bar = 96_000u64;
        // Loop #3, bar 2 inside an 8-bar song: monotonic pos = 18 bars -> wrapped = 2 bars.
        let pos = 18 * bar + 1234;
        let prev = (0, 8 * bar);
        let next = (0, 9 * bar);
        let rebased = rebase_loop_pos(pos, prev, next);
        assert_eq!(wrap_frame(rebased, next), 2 * bar + 1234);

        // Shrinking the song past the current playhead wraps cleanly into the new length.
        let shrunk = (0, 4 * bar);
        let pos_late = 6 * bar + 500;
        assert_eq!(
            wrap_frame(rebase_loop_pos(pos_late, prev, shrunk), shrunk),
            2 * bar + 500
        );
    }

    #[test]
    fn ensure_unique_plugin_ids_repairs_duplicates_across_channels_and_fx() {
        let mut song = Song::default();
        let make = |id: u64, name: &str| PluginRef {
            id,
            format: "vst3".into(),
            path: "/plugins/GrandPiano.vst3".into(),
            name: name.into(),
            vendor: String::new(),
            state: "abc".into(),
            outs: vec![],
        };
        let a = song.add_channel(
            "Grand Piano",
            InstrumentKind::Plugin {
                plugin: make(7, "Grand Piano"),
            },
        );
        let b = song.add_channel(
            "Grand Piano 2",
            InstrumentKind::Plugin {
                plugin: make(7, "Grand Piano"),
            },
        );
        song.mixer[1]
            .fx
            .push(crate::fx::FxSlot::plugin(make(7, "Grand Piano FX")));

        assert_eq!(song.ensure_unique_plugin_ids(), 2);
        let id_a = song.instrument(a).unwrap().plugin().unwrap().id;
        let id_b = song.instrument(b).unwrap().plugin().unwrap().id;
        let id_fx = song.mixer[1].fx[0].plugin.as_ref().unwrap().id;
        assert_eq!(id_a, 7);
        assert_ne!(id_a, id_b);
        assert_ne!(id_a, id_fx);
        assert_ne!(id_b, id_fx);
        assert_eq!(song.ensure_unique_plugin_ids(), 0);
    }

    /// `cargo test --release --lib -- --ignored --nocapture edit_cost`
    #[test]
    #[ignore]
    fn edit_cost() {
        let mut song = big_song(100, 2000);
        let t = std::time::Instant::now();
        let n = 200;
        let mut held = Vec::new();
        for i in 0..n {
            held.push(song.clone());
            song.pattern_mut(1 + i % 100).unwrap().notes[0].pitch ^= 1;
        }
        eprintln!(
            "clone + one-note edit, 100×2000 notes: {:.1} µs",
            t.elapsed().as_secs_f64() * 1e6 / n as f64
        );
    }
}
