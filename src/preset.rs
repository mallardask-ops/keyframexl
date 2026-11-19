//! Presets as files (FL: channel presets, mixer track states, effect presets).
//! JSON in an envelope like projects: `{ app, preset, version, data }`.
//! Hosted plugins inside a preset get fresh ids when applied, so the project
//! loads a new instance with the saved state.

use crate::{
    fx::FxSlot,
    project_io::APP_ID,
    sequence::{Instrument, InstrumentKind, MixerTrack, Song},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A channel: its instrument (sampler preset or plugin state), volume, pan.
    Channel,
    /// A mixer track: effect slots, EQ, volume, pan, separation (no routing).
    Mixer,
    /// One effect slot.
    Fx,
    /// A KeyFrame Tide synth patch.
    Synth,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Channel, Kind::Mixer, Kind::Fx, Kind::Synth];
    pub fn ext(self) -> &'static str {
        match self {
            Kind::Channel => "kfxchannel",
            Kind::Mixer => "kfxmixer",
            Kind::Fx => "kfxfx",
            Kind::Synth => "kfxsynth",
        }
    }
    /// Folder under `Presets/` (the browser lists the same ones).
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Channel => "Channels",
            Kind::Mixer => "Mixer",
            Kind::Fx => "Effects",
            Kind::Synth => "Synth",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Channel => "Channel preset",
            Kind::Mixer => "Mixer track preset",
            Kind::Fx => "Effect preset",
            Kind::Synth => "Synth preset",
        }
    }
    fn tag(self) -> &'static str {
        match self {
            Kind::Channel => "channel",
            Kind::Mixer => "mixer",
            Kind::Fx => "fx",
            Kind::Synth => "synth",
        }
    }
    pub fn of_path(path: &Path) -> Option<Kind> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Kind::ALL.into_iter().find(|k| k.ext() == ext)
    }
}

/// `~/Music/KeyFrameXL/Presets/<folder>` (falls back to the home folder).
pub fn dir(kind: Kind) -> Option<PathBuf> {
    Some(
        dirs::audio_dir()
            .or_else(dirs::home_dir)?
            .join("KeyFrameXL")
            .join("Presets")
            .join(kind.folder()),
    )
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    app: String,
    preset: String,
    version: u32,
    data: T,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelPreset {
    pub name: String,
    pub kind: InstrumentKind,
    pub volume: f32,
    pub pan: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixerPreset {
    pub track: MixerTrack,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxPreset {
    pub slot: FxSlot,
}

/// A KeyFrame Tide patch with its name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthPreset {
    pub name: String,
    pub patch: crate::synth::SynthPatch,
}

impl ChannelPreset {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(0.0..=crate::sequence::FADER_MAX).contains(&self.volume)
            || !(-1.0..=1.0).contains(&self.pan)
        {
            return Err("Bad channel volume or pan");
        }
        if let InstrumentKind::Synth { patch } = &self.kind {
            patch.validate()?;
        }
        Ok(())
    }
    /// The hosted plugin's path when this is a plugin channel.
    pub fn kind_plugin_path(&self) -> Option<String> {
        match &self.kind {
            InstrumentKind::Plugin { plugin } => Some(plugin.path.clone()),
            _ => None,
        }
    }

    pub fn of(i: &Instrument) -> Self {
        Self {
            name: i.name.clone(),
            kind: i.kind.clone(),
            volume: i.volume,
            pan: i.pan,
        }
    }

    /// Replace channel `id`'s instrument (name, sound, volume, pan); its notes,
    /// insert and mute stay. `false` when there is no such channel.
    pub fn apply(&self, song: &mut Song, id: u64) -> bool {
        let mut kind = self.kind.clone();
        fresh_instrument(&mut kind, &mut song.next_plugin_id());
        let Some(i) = song.instruments.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        i.name = self.name.clone();
        i.kind = kind;
        i.volume = self.volume;
        i.pan = self.pan;
        true
    }

    /// A new channel from the preset; returns its id.
    pub fn add(&self, song: &mut Song) -> u64 {
        let mut kind = self.kind.clone();
        fresh_instrument(&mut kind, &mut song.next_plugin_id());
        let name = song.free_name(&self.name);
        let id = song.add_channel(&name, kind);
        if let Some(i) = song.instruments.iter_mut().find(|i| i.id == id) {
            i.volume = self.volume;
            i.pan = self.pan;
        }
        id
    }
}

impl MixerPreset {
    /// Sends point at other tracks of the source project, so they stay out (FL does the same).
    pub fn of(t: &MixerTrack) -> Self {
        let mut track = t.clone();
        track.sends.clear();
        Self { track }
    }

    /// Effects, EQ and levels onto track `t`; its name, routing and mute stay.
    pub fn apply(&self, song: &mut Song, t: usize) -> bool {
        let mut fx = self.track.fx.clone();
        let mut next = song.next_plugin_id();
        for s in &mut fx {
            fresh_slot(s, &mut next);
        }
        let Some(m) = song.mixer.get_mut(t) else {
            return false;
        };
        m.fx = fx;
        m.volume = self.track.volume;
        m.pan = self.track.pan;
        m.eq = self.track.eq;
        m.sep = self.track.sep;
        true
    }
}

impl FxPreset {
    pub fn of(s: &FxSlot) -> Self {
        Self { slot: s.clone() }
    }

    /// Into slot `k` of track `t` (replacing it), or appended when `k` is past the end.
    pub fn apply(&self, song: &mut Song, t: usize, k: usize) -> bool {
        let mut slot = self.slot.clone();
        fresh_slot(&mut slot, &mut song.next_plugin_id());
        let Some(m) = song.mixer.get_mut(t) else {
            return false;
        };
        if k < m.fx.len() {
            m.fx[k] = slot;
        } else {
            m.fx.push(slot);
        }
        true
    }
}

fn fresh_instrument(kind: &mut InstrumentKind, next: &mut u64) {
    if let InstrumentKind::Plugin { plugin } = kind {
        plugin.id = *next;
        *next += 1;
    }
}

fn fresh_slot(slot: &mut FxSlot, next: &mut u64) {
    if let Some(p) = &mut slot.plugin {
        p.id = *next;
        *next += 1;
    }
}

pub fn save<T: Serialize>(kind: Kind, data: &T, path: &Path) -> Result<(), String> {
    let env = Envelope {
        app: APP_ID.to_string(),
        preset: kind.tag().to_string(),
        version: VERSION,
        data,
    };
    let bytes = serde_json::to_vec_pretty(&env).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn parse<T: DeserializeOwned>(kind: Kind, bytes: &[u8]) -> Result<T, String> {
    let env: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| format!("Not a preset file: {e}"))?;
    if env.app != APP_ID {
        return Err("Not a KeyFrameXL preset".into());
    }
    if env.preset != kind.tag() {
        return Err(format!("This is not a {}", kind.label().to_lowercase()));
    }
    if env.version > VERSION {
        return Err("Preset is from a newer KeyFrameXL".into());
    }
    serde_json::from_value(env.data).map_err(|e| e.to_string())
}

pub fn load<T: DeserializeOwned>(kind: Kind, path: &Path) -> Result<T, String> {
    parse(kind, &std::fs::read(path).map_err(|e| e.to_string())?)
}

/// A file name from a preset name: no path separators or control characters.
pub fn file_name(name: &str, kind: Kind) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim();
    format!(
        "{}.{}",
        if clean.is_empty() { "Preset" } else { clean },
        kind.ext()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::PluginRef;

    fn plugin(id: u64) -> PluginRef {
        PluginRef {
            id,
            format: "vst3".into(),
            path: "/x/Synth.vst3".into(),
            name: "Synth".into(),
            vendor: String::new(),
            state: "AAEC".into(),
            outs: vec![],
        }
    }

    #[test]
    fn channel_preset_round_trips_and_gets_a_fresh_plugin_id() {
        let mut song = Song::default();
        let id = song.add_channel("Lead", InstrumentKind::Plugin { plugin: plugin(1) });
        song.instruments
            .iter_mut()
            .find(|i| i.id == id)
            .unwrap()
            .volume = 0.5;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(file_name("Lead / bright", Kind::Channel));
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".kfxchannel")
        );
        save(
            Kind::Channel,
            &ChannelPreset::of(song.instruments.iter().find(|i| i.id == id).unwrap()),
            &path,
        )
        .unwrap();
        let p: ChannelPreset = load(Kind::Channel, &path).unwrap();
        let new = p.add(&mut song);
        let i = song.instruments.iter().find(|i| i.id == new).unwrap();
        assert_eq!(i.volume, 0.5);
        let r = i.plugin().unwrap();
        assert_eq!(r.state, "AAEC", "plugin state travels");
        assert_ne!(r.id, 1, "a second instance, not the first one again");
        assert_ne!(i.name, "Lead", "names stay unique");
        // Applying onto an existing channel keeps its id.
        let kick = song.instruments[0].id;
        assert!(p.apply(&mut song, kick));
        assert_eq!(song.instruments[0].id, kick);
        assert!(song.instruments[0].plugin().is_some());
        song.validate().unwrap();
    }

    #[test]
    fn mixer_preset_drops_sends_and_wrong_kinds_are_refused() {
        let mut song = Song::default();
        song.mixer[1].fx.push(FxSlot::plugin(plugin(7)));
        song.mixer[1]
            .sends
            .push(crate::sequence::Send { to: 2, level: 0.5 });
        song.mixer[1].eq = [3.0, 0.0, -2.0];
        let p = MixerPreset::of(&song.mixer[1]);
        assert!(p.track.sends.is_empty());
        let bytes = serde_json::to_vec(&Envelope {
            app: APP_ID.into(),
            preset: "mixer".into(),
            version: VERSION,
            data: &p,
        })
        .unwrap();
        assert!(parse::<ChannelPreset>(Kind::Channel, &bytes).is_err());
        let back: MixerPreset = parse(Kind::Mixer, &bytes).unwrap();
        assert!(back.apply(&mut song, 2));
        assert_eq!(song.mixer[2].eq, [3.0, 0.0, -2.0]);
        let ids: Vec<u64> = song.plugin_refs().iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(
            parse::<MixerPreset>(
                Kind::Mixer,
                b"{\"app\":\"other\",\"preset\":\"mixer\",\"version\":1,\"data\":{}}"
            )
            .is_err()
        );
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(Kind::of_path(Path::new("a/b.KFXFX")), Some(Kind::Fx));
        assert_eq!(Kind::of_path(Path::new("a/b.wav")), None);
    }
}
