//! Singing voice channels: UTAU voicebanks (Kasane Teto and any other) sung
//! from notes with lyrics, rendered with WORLD, played by the engine.
//!
//! Flow: the song's vocal notes are grouped into phrases ([`plan`]); a
//! background worker renders each phrase to audio ([`host`]) and publishes a
//! [`VocalMap`]; the engine starts a phrase at its first note. Editing one
//! phrase renders only that phrase again.

pub mod analysis;
pub mod dsp;
pub mod host;
pub mod install;
pub mod lyric;
pub mod oto;
pub mod plan;
pub mod render;

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub use render::RNote;

/// Singing settings of one vocal channel.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Params {
    /// Vibrato depth in cents (peak), rate in Hz, onset delay and fade-in in seconds.
    #[serde(default = "d_vib_depth")]
    pub vibrato_depth: f64,
    #[serde(default = "d_vib_rate")]
    pub vibrato_rate: f64,
    #[serde(default = "d_vib_delay")]
    pub vibrato_delay: f64,
    #[serde(default = "d_vib_fade")]
    pub vibrato_fade: f64,
    /// Seconds a pitch change between touching notes takes.
    #[serde(default = "d_transition")]
    pub transition: f64,
    /// Formant shift, -1 (darker, bigger) .. 1 (brighter, smaller).
    #[serde(default)]
    pub gender: f64,
    /// Breathiness 0..1.
    #[serde(default)]
    pub breath: f64,
}

fn d_vib_depth() -> f64 {
    35.0
}
fn d_vib_rate() -> f64 {
    5.5
}
fn d_vib_delay() -> f64 {
    0.30
}
fn d_vib_fade() -> f64 {
    0.25
}
fn d_transition() -> f64 {
    0.07
}

impl Default for Params {
    fn default() -> Self {
        Self {
            vibrato_depth: d_vib_depth(),
            vibrato_rate: d_vib_rate(),
            vibrato_delay: d_vib_delay(),
            vibrato_fade: d_vib_fade(),
            transition: d_transition(),
            gender: 0.0,
            breath: 0.0,
        }
    }
}

/// A vocal channel's voice: which bank, how it sings. The bank itself lives
/// on disk (it can be large and is licensed by its author), the project keeps
/// only its path.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Voice {
    /// Voicebank folder.
    pub bank: String,
    /// Shown name (from `character.txt`).
    pub name: String,
    /// What a note sings when it has no lyric.
    #[serde(default = "d_lyric")]
    pub default_lyric: String,
    #[serde(default)]
    pub params: Params,
}

fn d_lyric() -> String {
    "あ".into()
}

impl Voice {
    pub fn new(bank: &str, name: &str) -> Self {
        Self {
            bank: bank.into(),
            name: name.into(),
            default_lyric: d_lyric(),
            params: Params::default(),
        }
    }
}

/// A loaded voicebank with its analysis cache.
pub struct Bank {
    pub vb: oto::Voicebank,
    #[allow(clippy::type_complexity)]
    an: Mutex<HashMap<PathBuf, Arc<OnceLock<Result<Arc<analysis::Analysis>, String>>>>>,
}

impl Bank {
    pub fn open(dir: &Path) -> Result<Arc<Bank>, String> {
        Ok(Arc::new(Bank {
            vb: oto::Voicebank::load(dir)?,
            an: Mutex::new(HashMap::new()),
        }))
    }
    /// The WORLD analysis of an entry's wav (computed once, shared by threads).
    pub fn analysis(&self, e: &oto::Entry) -> Result<Arc<analysis::Analysis>, String> {
        let cell = {
            let mut m = self.an.lock().map_err(|_| "poisoned".to_string())?;
            m.entry(e.wav.clone()).or_default().clone()
        };
        cell.get_or_init(|| analysis::load(&e.wav).map(Arc::new))
            .clone()
    }
}

/// One rendered phrase, mono at the engine's rate.
pub struct Phrase {
    /// Frames that sound before its first note's start.
    pub lead: usize,
    pub data: Vec<f32>,
}

/// Phrases placed in one transport mode, keyed by (note id, tick) as the
/// engine's hits carry them: the playlist tick in song mode, the pattern tick
/// in pattern mode. A pattern in two clips is two placements.
#[derive(Default)]
pub struct Placed {
    /// Phrases by their first note.
    pub starts: HashMap<(u64, crate::sequence::Tick), Arc<Phrase>>,
    /// The notes phrases cover after their first (those start nothing).
    pub members: HashSet<(u64, crate::sequence::Tick)>,
}

/// What the engine plays.
#[derive(Default)]
pub struct VocalMap {
    pub song: Placed,
    pub pattern: Placed,
    /// The longest `lead` of any placed phrase: a clip's first phrase may
    /// start this many frames before the clip.
    pub max_lead: usize,
    /// A held key's note: the channel's default lyric at each pitch of the
    /// keyboard range, by (channel, pitch).
    /// (channel, pitch, lyric hash); hash 0 is the channel's default lyric.
    pub previews: HashMap<(u64, u8, u64), Arc<Phrase>>,
    pub rate: u32,
}

/// Keys that can be played live on a singing channel (C2 .. C6).
pub const PREVIEW_PITCHES: std::ops::RangeInclusive<u8> = 36..=84;
/// Hash of a lyric as the live-note hint and preview keys use it (never 0).
pub fn lyric_hash(text: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in text.trim().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h | 1
}
/// How long a held key sings (seconds).
pub const PREVIEW_LEN: f64 = 2.5;

impl VocalMap {
    pub fn is_empty(&self) -> bool {
        self.song.starts.is_empty() && self.pattern.starts.is_empty()
    }

    pub fn placed(&self, song_mode: bool) -> &Placed {
        if song_mode { &self.song } else { &self.pattern }
    }
}
