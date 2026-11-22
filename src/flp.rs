//! FL Studio projects (`.flp`), read-only: tempo, meter, channels, patterns
//! with their notes, and the playlist become a KeyFrameXL song.
//!
//! The format is an event stream ("FLhd" header, then "FLdt"): each event is
//! an id byte and a value whose size the id gives (0-63 byte, 64-127 word,
//! 128-191 dword, 192-255 varint-length data). Ids and record layouts follow
//! FL Studio 2025 files and the open PyFLP project; older playlist records
//! are told apart by size.
//!
//! The report lists settings that need FL Studio or an approximation.

use crate::fx::{FxKind, FxSlot};
use crate::sequence::{
    BAR, Clip, FX_SLOTS, FlMixerSlot, FlPluginState, Glide, Instrument, InstrumentKind, MixerTrack,
    Note, PPQ, Pattern, PluginRef, Send, Song, Tick, Track,
};
use std::collections::BTreeMap;

/// One event: id and value (a number, or data for ids 192 and up).
#[derive(Debug, Clone, Copy)]
enum Value<'a> {
    Num(u32),
    Data(&'a [u8]),
}

fn events(bytes: &[u8]) -> Result<(u16, Vec<(u8, Value<'_>)>), String> {
    let (ppq, evs, _) = events_at(bytes)?;
    Ok((ppq, evs.into_iter().map(|(id, v, _)| (id, v)).collect()))
}

/// `events`, with each event's byte span in the file and the data chunk's
/// start (where the first event begins).
fn events_at(
    bytes: &[u8],
) -> Result<(u16, Vec<(u8, Value<'_>, std::ops::Range<usize>)>, usize), String> {
    let le32 = |at: usize| -> Option<u32> {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if bytes.get(..4) != Some(b"FLhd") {
        return Err("Not an FL Studio project (no FLhd header)".into());
    }
    let head = le32(4).ok_or("Truncated FLP header")? as usize;
    let ppq = bytes
        .get(12..14)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or("Truncated FLP header")?;
    let mut p = 8 + head;
    if bytes.get(p..p + 4) != Some(b"FLdt") {
        return Err("FLP data chunk missing".into());
    }
    let end = (p + 8 + le32(p + 4).ok_or("Truncated FLP")? as usize).min(bytes.len());
    p += 8;
    let data_start = p;
    let mut out = Vec::new();
    while p < end {
        let at = p;
        let id = bytes[p];
        p += 1;
        let v = match id {
            0..=63 => {
                let v = *bytes.get(p).ok_or("Truncated FLP event")? as u32;
                p += 1;
                Value::Num(v)
            }
            64..=127 => {
                let b = bytes.get(p..p + 2).ok_or("Truncated FLP event")?;
                p += 2;
                Value::Num(u16::from_le_bytes([b[0], b[1]]) as u32)
            }
            128..=191 => {
                let v = le32(p).ok_or("Truncated FLP event")?;
                p += 4;
                Value::Num(v)
            }
            _ => {
                let (mut n, mut shift) = (0usize, 0);
                loop {
                    let b = *bytes.get(p).ok_or("Truncated FLP event")?;
                    p += 1;
                    n |= ((b & 0x7f) as usize) << shift;
                    shift += 7;
                    if b < 0x80 || shift > 28 {
                        break;
                    }
                }
                let d = bytes.get(p..p + n).ok_or("Truncated FLP event data")?;
                p += n;
                Value::Data(d)
            }
        };
        out.push((id, v, at..p));
    }
    Ok((ppq.max(1), out, data_start))
}

/// FL Studio VSTi's state: a 737-byte header of its own, then a whole FL
/// project. The header is the one the plugin saves for its default project.
pub const FL_VSTI_HEADER: &[u8] = include_bytes!("../assets/fl-vsti-header.bin");

/// A project holding only channel `index` of `bytes` (as channel 0, so it
/// is the selected one and plays FL Studio VSTi's MIDI input): the project's
/// own settings before the first pattern or channel, then that channel's
/// events up to the next channel, pattern, arrangement or the mixer.
pub fn channel_flp(bytes: &[u8], index: u32) -> Result<Vec<u8>, String> {
    let (_, evs, data_start) = events_at(bytes)?;
    let first = evs
        .iter()
        .position(|(id, _, _)| *id == CH_NEW || *id == PAT_NEW)
        .ok_or("No channels in the project")?;
    let start = evs
        .iter()
        .position(|(id, v, _)| *id == CH_NEW && matches!(v, Value::Num(i) if *i == index))
        .ok_or("No such channel")?;
    let stop = evs[start + 1..]
        .iter()
        .position(|(id, _, _)| [CH_NEW, PAT_NEW, ARR_NEW].contains(id) || MIXER_START.contains(id))
        .map_or(evs.len(), |n| start + 1 + n);
    let mut body = Vec::new();
    for (_, _, r) in &evs[..first] {
        body.extend_from_slice(&bytes[r.clone()]);
    }
    body.extend_from_slice(&[CH_NEW, 0, 0]);
    for (_, _, r) in &evs[start + 1..stop] {
        body.extend_from_slice(&bytes[r.clone()]);
    }
    let mut out = bytes[..data_start - 8].to_vec();
    // The header's channel count (bytes 10..12): one.
    if out.len() >= 12 {
        out[10..12].copy_from_slice(&1u16.to_le_bytes());
    }
    out.extend_from_slice(b"FLdt");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

/// The project with only channel `index` sounding, dry: every other channel
/// switched off, this one played straight into Master, and Master's effects,
/// EQ and fader neutral. FL Studio renders it for a channel that cannot play
/// here (FLEX): the insert effects then come from KeyFrameXL's own mixer.
pub fn solo_channel_flp(bytes: &[u8], index: u32) -> Result<Vec<u8>, String> {
    let (_, evs, data_start) = events_at(bytes)?;
    if !evs
        .iter()
        .any(|(id, v, _)| *id == CH_NEW && matches!(v, Value::Num(i) if *i == index))
    {
        return Err("No such channel".into());
    }
    let mut body = Vec::with_capacity(bytes.len());
    let mut channel: Option<u32> = None;
    let mut directwave = false;
    let mut mixer = false;
    for (id, v, r) in &evs {
        match (*id, v) {
            (CH_NEW, Value::Num(i)) => {
                channel = Some(*i);
                directwave = false;
            }
            (PAT_NEW, _) | (ARR_NEW, _) => channel = None,
            (i, _) if MIXER_START.contains(&i) => {
                channel = None;
                mixer = true;
            }
            (PLUGIN_INTERNAL, Value::Data(d)) if channel.is_some() => {
                directwave = text(d) == "DirectWave";
            }
            _ => {}
        }
        // The dry render routes straight to Master and needs no mixer plugins.
        if mixer
            && matches!(
                *id,
                PLUGIN_INTERNAL | PLUGIN_NAME | PLUGIN_WRAPPER | PLUGIN_DATA
            )
        {
            continue;
        }
        match (*id, v, channel) {
            // A muted DirectWave still searches for every missing sample at load.
            (PLUGIN_DATA, _, Some(c)) if c != index && directwave => continue,
            (CH_ENABLED, _, Some(c)) => {
                body.extend_from_slice(&[CH_ENABLED, (c == index) as u8]);
                continue;
            }
            (CH_INSERT, _, Some(c)) if c == index => {
                body.push(CH_INSERT);
                body.extend_from_slice(&0u16.to_le_bytes());
                continue;
            }
            (MIXER_PARAMS, Value::Data(d), _) => {
                let mut d = d.to_vec();
                for rec in d.as_chunks_mut::<12>().0 {
                    let (pid, packed) = (rec[4], u16::from_le_bytes([rec[6], rec[7]]));
                    if packed == 0x4000 && pid == 0 && rec[5] == 0 {
                        rec[8..12].copy_from_slice(&12800i32.to_le_bytes());
                        continue;
                    }
                    if mixer_address(packed).map(|(insert, _)| insert) != Some(0) {
                        continue;
                    }
                    let value: Option<i32> = match pid {
                        0 => Some(0),
                        192 => Some(12800),
                        193 | 194 | 208..=210 => Some(0),
                        _ => None,
                    };
                    if let Some(v) = value {
                        rec[8..12].copy_from_slice(&v.to_le_bytes());
                    }
                }
                body.push(MIXER_PARAMS);
                let mut n = d.len();
                loop {
                    let b = (n & 0x7f) as u8;
                    n >>= 7;
                    body.push(if n > 0 { b | 0x80 } else { b });
                    if n == 0 {
                        break;
                    }
                }
                body.extend_from_slice(&d);
                continue;
            }
            _ => {}
        }
        body.extend_from_slice(&bytes[r.clone()]);
        // Channels without the switch are on: say it right away.
        if let (CH_NEW, Value::Num(i)) = (*id, v) {
            body.extend_from_slice(&[CH_ENABLED, (*i == index) as u8]);
        }
    }
    let mut out = bytes[..data_start - 8].to_vec();
    out.extend_from_slice(b"FLdt");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

/// FL Studio VSTi state (KeyFrameXL's VST2 format) playing channel `index`.
pub fn fl_vsti_state(bytes: &[u8], index: u32) -> Result<Vec<u8>, String> {
    Ok([
        b"KFV2C".as_slice(),
        FL_VSTI_HEADER,
        &channel_flp(bytes, index)?,
    ]
    .concat())
}

/// A playback-only project for the standalone FL bridge. Keep channel
/// indices stable for FL's MIDI scripting API; only the requested generators
/// retain their plugin state. Each one goes to its own stereo ASIO output.
pub fn native_host_flp(bytes: &[u8], channels: &[u32]) -> Result<(Vec<u8>, Vec<u32>), String> {
    if channels.is_empty() || channels.len() > 16 {
        return Err("The FL bridge needs between one and sixteen channels".into());
    }
    let (_, evs, data_start) = events_at(bytes)?;
    let mut initial = vec![CH_NEW, 0, 0, CH_TYPE, 0, CH_ENABLED, 0, CH_INSERT, 0, 0];
    // The first FLEX instance can fail during a cold Wine start. Initialize
    // it on an unused channel before loading any of the routed presets.
    for (first, (id, value, _)) in evs.iter().enumerate() {
        if *id != CH_NEW || !matches!(value, Value::Num(i) if channels.contains(i)) {
            continue;
        }
        let events = evs[first + 1..].iter().take_while(|(id, _, _)| {
            !matches!(*id, PAT_NEW | CH_NEW | ARR_NEW) && !MIXER_START.contains(id)
        });
        if events.clone().any(|(id, value, _)| {
            *id == PLUGIN_INTERNAL
                && matches!(value, Value::Data(d) if text(d).eq_ignore_ascii_case("FLEX"))
        }) {
            initial[4] = 2;
            for (id, value, range) in events {
                match (*id, value) {
                    (PLUGIN_INTERNAL | PLUGIN_DATA, _) => {
                        initial.extend_from_slice(&bytes[range.clone()])
                    }
                    (PLUGIN_WRAPPER, Value::Data(data)) if data.len() >= 20 => {
                        write_hidden_wrapper(&mut initial, data);
                    }
                    _ => {}
                }
            }
            break;
        }
    }
    let mut body = Vec::new();
    let mut channel = None;
    let mut insert = None;
    let mut found = 0;
    let mut outputs = 0;
    let mut ordinal = 0u16;
    let mut midi = vec![0; channels.len()];
    for (id, value, range) in evs {
        match (id, value) {
            (CH_NEW, Value::Num(index)) => {
                let slot = channels.iter().position(|&c| c == index);
                channel = Some(slot);
                if ordinal == 0 {
                    body.extend_from_slice(&initial);
                }
                ordinal = ordinal
                    .checked_add(1)
                    .ok_or("Too many channels for the FL bridge")?;
                body.push(CH_NEW);
                body.extend_from_slice(&ordinal.to_le_bytes());
                if let Some(slot) = slot {
                    found += 1;
                    midi[slot] = u32::from(ordinal);
                    body.extend_from_slice(&[CH_ENABLED, 1, CH_INSERT, slot as u8 + 1, 0]);
                } else {
                    body.extend_from_slice(&[CH_TYPE, 0, CH_ENABLED, 0]);
                }
                continue;
            }
            (PAT_NEW | ARR_NEW, _) => channel = None,
            (i, _) if MIXER_START.contains(&i) => {
                channel = None;
                if i == INSERT_FLAGS {
                    insert = Some(insert.map_or(0, |n| n + 1));
                }
            }
            _ => {}
        }
        if matches!(
            id,
            PAT_EVENTS | INITIALIZED_CONTROLS | AUTO_LINK | PAT_NOTES | PLAYLIST
        ) || matches!(channel, Some(None))
        {
            continue;
        }
        if matches!(channel, Some(Some(_))) {
            match (id, value) {
                (CH_ENABLED | CH_INSERT, _) => continue,
                (PLUGIN_WRAPPER, Value::Data(data)) if data.len() >= 20 => {
                    write_hidden_wrapper(&mut body, data);
                    continue;
                }
                (CH_LEVELS, Value::Data(data)) if data.len() >= 8 => {
                    let mut levels = data.to_vec();
                    levels[..4].copy_from_slice(&6400u32.to_le_bytes());
                    levels[4..8].copy_from_slice(&10000u32.to_le_bytes());
                    write_event(&mut body, id, &levels);
                    continue;
                }
                _ => {}
            }
        }
        if let Some(track) = insert {
            match (id, value) {
                (PLUGIN_INTERNAL | PLUGIN_NAME | PLUGIN_WRAPPER | PLUGIN_DATA, _) => continue,
                (INSERT_FLAGS, Value::Data(data)) if track <= channels.len() && data.len() >= 8 => {
                    let mut flags = data.to_vec();
                    flags[4..8].copy_from_slice(&0x0cu32.to_le_bytes());
                    write_event(&mut body, id, &flags);
                    continue;
                }
                (INSERT_ROUTING, Value::Data(data)) if track <= channels.len() => {
                    write_event(&mut body, id, &vec![0; data.len()]);
                    continue;
                }
                (INSERT_OUTPUT, _) if (1..=channels.len()).contains(&track) => {
                    body.push(id);
                    body.extend_from_slice(&(track as u32).to_le_bytes());
                    outputs += 1;
                    continue;
                }
                (MIXER_PARAMS, Value::Data(data)) => {
                    let mut params = data.to_vec();
                    for r in params.as_chunks_mut::<12>().0 {
                        let addr = le16(r, 6);
                        let gain = if addr == 0x4000 && r[4..6] == [0, 0] {
                            Some(12800i32)
                        } else if mixer_address(addr).is_some_and(|(i, _)| i <= channels.len()) {
                            match r[4] {
                                192 => Some(12800),
                                193 | 194 | 208..=210 => Some(0),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(gain) = gain {
                            r[8..12].copy_from_slice(&gain.to_le_bytes());
                        }
                    }
                    write_event(&mut body, id, &params);
                    continue;
                }
                _ => {}
            }
        }
        body.extend_from_slice(&bytes[range]);
    }
    if found != channels.len() || outputs != channels.len() {
        return Err("The source project does not have the channels or mixer outputs needed by the FL bridge".into());
    }
    let mut project = bytes[..data_start - 4].to_vec();
    let count = ordinal
        .checked_add(1)
        .ok_or("Too many channels for the FL bridge")?;
    project[10..12].copy_from_slice(&count.to_le_bytes());
    project.extend_from_slice(&(body.len() as u32).to_le_bytes());
    project.extend(body);
    Ok((project, midi))
}

fn write_hidden_wrapper(body: &mut Vec<u8>, data: &[u8]) {
    let mut wrapper = data.to_vec();
    let flags = le32(data, 16) & !0x205; // visible, detached, editor window state
    wrapper[16..20].copy_from_slice(&flags.to_le_bytes());
    write_event(body, PLUGIN_WRAPPER, &wrapper);
}

fn write_event(body: &mut Vec<u8>, id: u8, data: &[u8]) {
    body.push(id);
    let mut n = data.len();
    while n >= 128 {
        body.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    body.push(n as u8);
    body.extend_from_slice(data);
}

/// FL keeps names as UTF-16 (FL 11.5 and later) or 8-bit text, NUL-ended.
fn text(d: &[u8]) -> String {
    let utf16 = d.len() >= 2
        && d.len().is_multiple_of(2)
        && d.iter().skip(1).step_by(2).filter(|b| **b == 0).count() * 2 >= d.len() / 2;
    let s = if utf16 {
        String::from_utf16_lossy(
            &d.as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(d).into_owned()
    };
    s.trim_end_matches('\0').trim().to_string()
}

/// Windows COLORREF (0x00BBGGRR) → 0xRRGGBB.
fn rgb(c: u32) -> u32 {
    ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff)
}

fn le16(d: &[u8], at: usize) -> u16 {
    d.get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}
fn le32(d: &[u8], at: usize) -> u32 {
    d.get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

// Event ids (FL Studio 2025; names as in PyFLP).
const CH_ENABLED: u8 = 0;
const METER_NUM: u8 = 17;
const METER_DEN: u8 = 18;
const CH_TYPE: u8 = 21;
const CH_NEW: u8 = 64;
const PAT_NEW: u8 = 65;
const ARR_NEW: u8 = 99;
const CH_COLOR: u8 = 128;
const PAT_COLOR: u8 = 150;
const TEMPO: u8 = 156;
const PAT_NAME: u8 = 193;
const SAMPLE_PATH: u8 = 196;
const FL_VERSION: u8 = 199;
const PLUGIN_INTERNAL: u8 = 201;
const PLUGIN_NAME: u8 = 203;
const PLUGIN_WRAPPER: u8 = 212;
const PLUGIN_DATA: u8 = 213;
const CH_INSERT: u8 = 104;
const SLOT_INDEX: u8 = 98;
const INSERT_NAME: u8 = 204;
/// Ends an insert's events; its name and colour come after, before the next
/// insert's flags.
const INSERT_OUTPUT: u8 = 147;
const INITIALIZED_CONTROLS: u8 = 216;
const MIXER_PARAMS: u8 = 225;
const INSERT_ROUTING: u8 = 235;
const INSERT_FLAGS: u8 = 236;
const CH_LEVELS: u8 = 219;
const PAT_NOTES: u8 = 224;
const PLAYLIST: u8 = 233;
const TRACK_DATA: u8 = 238;
const TRACK_NAME: u8 = 239;
/// Playlist record layout flags (FL 2025+): bit 0x2 clear = 60-byte
/// records, else bit 0x8 clear = 80, else 88.
const PLAYLIST_LAYOUT: u8 = 169;
/// A channel's sample parameters: stretch pitch/multiplier/mode at 100..112.
const CH_PARAMS: u8 = 215;
/// A pattern's automation events (recorded knob moves): 12-byte records.
const PAT_EVENTS: u8 = 223;
/// An automation channel's points.
const AUTO_POINTS: u8 = 234;
/// What an automation channel drives (FL: internal remote control).
const AUTO_LINK: u8 = 227;
/// Events that start the mixer: plugins named after them are effects.
const MIXER_START: [u8; 4] = [98, 147, 225, 236];

fn mixer_address(packed: u16) -> Option<(usize, u32)> {
    // Older projects start at 0x2000. In the FL 25 layout starting at
    // 0x7000, 0x8000 is insert 64 and 0xe000 is the Current strip.
    let address = match packed {
        0x0000..=0x1fff => packed,
        0x2000..=0x3fff => packed - 0x2000,
        0x7000..=0x8fff => packed - 0x7000,
        _ => return None,
    };
    Some(((address >> 6) as usize, (address & 0x3f) as u32))
}

/// A channel of the FL project as read.
#[derive(Debug, Clone, Default)]
pub struct FlpChannel {
    pub index: u32,
    pub name: String,
    /// FL's own plugin name ("FLEX", "Sytrus", "Fruity Wrapper" for VST), empty for samplers.
    pub plugin: String,
    pub kind: ChannelKind,
    pub sample: Option<String>,
    pub enabled: bool,
    pub data: Vec<u8>,
    pub color: Option<u32>,
    /// 0..12800 as FL keeps them (pan 6400 = centre, volume 10000 = default).
    pub pan: Option<u32>,
    pub volume: Option<u32>,
    /// The VST plugin of a "Fruity Wrapper" channel.
    pub vst: Option<FlpVst>,
    /// The instrument made from this channel, if any.
    pub instrument: Option<u64>,
    /// DirectWave: the program file (.dwp) it plays, by name.
    pub dwp: Option<String>,
    /// The mixer insert it plays into (FL: the channel's FX number).
    pub insert: Option<u32>,
    /// The channel's Pitch knob in cents (FL: ±range, default ±200).
    pub pitch: i32,
    /// Sampler time stretching (FL: the channel's sample settings).
    pub stretch: Option<FlpStretch>,
    /// An automation channel's points: beats since the previous point,
    /// value 0..1, tension of the curve into this point.
    pub points: Vec<(f64, f32, f32)>,
}

/// A sampler channel's stretch settings (event 215).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlpStretch {
    /// Pitch shift in cents.
    pub pitch: i32,
    /// Time multiplier as `10000 · log2(ratio)`.
    pub multiplier: i32,
    /// -1 Stretch, 0 Resample, 1.. the élastique/slice/auto modes.
    pub mode: i32,
}

impl FlpStretch {
    /// The pitch a sampler plays it at, in cents. Resample changes speed and
    /// pitch together (the multiplier is a pitch too); the other modes keep
    /// time, which a sampler cannot, so only their pitch is kept.
    pub fn cents(&self) -> f64 {
        if self.mode == 0 {
            self.pitch as f64 + 0.12 * self.multiplier as f64
        } else {
            self.pitch as f64
        }
    }
}

/// The part of a sample an audio clip plays, in seconds into the sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioSlice {
    pub start: f64,
    /// Where the clip's window ends; kept only for a fade-out to end there.
    pub end: Option<f64>,
    pub fade_in: f64,
    pub fade_out: f64,
}

impl AudioSlice {
    pub fn at(start: f64) -> Self {
        Self {
            start,
            ..Default::default()
        }
    }

    fn same(&self, o: &Self) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
        near(self.start, o.start)
            && near(self.end.unwrap_or(-1.0), o.end.unwrap_or(-1.0))
            && near(self.fade_in, o.fade_in)
            && near(self.fade_out, o.fade_out)
    }
}

/// How a sampler's audio clips cut its sample, and its pitch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioSlices {
    /// Index 0 is the whole sample (key 60); index k plays slice k on key
    /// 60 + k.
    pub slices: Vec<AudioSlice>,
    /// FL's stretch pitch, in cents.
    pub tune_cents: f64,
}

impl FlpChannel {
    pub fn in_native_bridge(&self) -> bool {
        self.kind == ChannelKind::Generator
            && self.vst.is_none()
            && (self.plugin.eq_ignore_ascii_case("FLEX")
                || self.plugin.eq_ignore_ascii_case("Sakura"))
    }

    /// A generator that plays inside FL Studio VSTi, which reads the
    /// channel's own settings (its pitch knob among them).
    pub fn in_fl_vsti(&self) -> bool {
        self.kind == ChannelKind::Generator
            && self.vst.is_none()
            && self.dwp.is_none()
            && !self.plugin.eq_ignore_ascii_case("FLEX")
            && !matches!(self.plugin.as_str(), "MIDI out" | "Fruity Wrapper")
    }
}

/// A mixer insert of the FL project (0 = Master).
#[derive(Debug, Clone, Default)]
pub struct FlpInsert {
    pub index: u32,
    pub name: String,
    pub enabled: bool,
    pub bypass_effects: bool,
    /// FL's fader 0..16000 (12800 = 0 dB) and pan -6400..6400.
    pub volume: Option<i32>,
    pub pan: Option<i32>,
    /// Byte `t` = 1: this insert feeds insert `t` (0 = Master).
    pub routes: Vec<u8>,
    /// Send level to insert `t`, 0..12800 (12800 = unity), when not unity.
    pub levels: BTreeMap<u32, i32>,
    pub slots: Vec<FlpSlot>,
    /// The insert's own EQ (raw): gains low/mid/high (0.01 dB), frequencies
    /// (0..65536), bandwidths. `None`: FL's defaults.
    pub eq: Option<[i32; 9]>,
    /// Stereo separation -64..64 (+64 = mono).
    pub sep: Option<i32>,
}

/// An effect in an insert slot.
#[derive(Debug, Clone, Default)]
pub struct FlpSlot {
    pub slot: u32,
    /// FL's own plugin name ("Fruity Reeverb 2", "Fruity Wrapper" for VST).
    pub plugin: String,
    pub name: String,
    pub vst: Option<FlpVst>,
    pub enabled: bool,
    pub mix: f32,
    /// The plugin's saved settings.
    pub data: Vec<u8>,
}

/// The built-in effect nearest to one of FL's own; `None` when there is none.
pub fn native_effect(fl: &str) -> Option<FxKind> {
    let n = fl.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    Some(if has(&["parametric eq"]) {
        FxKind::ParamEq
    } else if has(&["7 band eq", "fruity eq"]) {
        FxKind::Eq
    } else if has(&["soundgoodizer"]) {
        // The same four profiles and one big knob as Bloom.
        FxKind::Bloom
    } else if has(&["multiband comp", "maximus"]) {
        FxKind::Multiband
    } else if has(&["limiter"]) {
        FxKind::Limiter
    } else if has(&["compressor"]) {
        FxKind::Compressor
    } else if has(&["convolver"]) {
        FxKind::Convolution
    } else if n == "fruity reeverb 2" {
        FxKind::RoomReverb
    } else if has(&["reeverb", "reverb", "luxeverb"]) {
        FxKind::Reverb
    } else if matches!(n.as_str(), "fruity delay 2" | "fruity delay bank") {
        FxKind::StereoDelay
    } else if has(&["delay"]) {
        FxKind::Delay
    } else if has(&["hyper chorus"]) {
        FxKind::HyperChorus
    } else if has(&["chorus", "flang"]) {
        FxKind::Chorus
    } else if has(&["phaser"]) {
        FxKind::Phaser
    } else if has(&["vocodex", "vocoder"]) {
        FxKind::Vocoder
    } else if has(&["squeeze"]) {
        FxKind::Squeeze
    } else if has(&["gross beat"]) {
        FxKind::Time
    } else if has(&["stereo shaper"]) {
        FxKind::StereoShaper
    } else if has(&["balance", "panomatic"]) {
        FxKind::Balance
    } else if has(&["stereo", "spreader"]) {
        FxKind::Stereo
    } else if has(&["transient processor"]) {
        FxKind::Transient
    } else if has(&["waveshaper"]) {
        FxKind::Wavefolder
    } else if n == "fruity soft clipper" {
        FxKind::SoftClipper
    } else if has(&["overdrive", "fast dist", "soft clipper", "distructor"]) {
        FxKind::Drive
    } else if has(&["filter", "philter", "fast lp"]) {
        FxKind::Filter
    } else {
        return None;
    })
}

/// The first ".dwp" file name in a DirectWave channel's plugin data.
fn dwp_name(d: &[u8]) -> Option<String> {
    let at = d.windows(4).position(|w| w.eq_ignore_ascii_case(b".dwp"))?;
    let start = d[..at]
        .iter()
        .rposition(|b| !(b.is_ascii_graphic() || *b == b' ') || *b == b'\\' || *b == b'/')
        .map_or(0, |i| i + 1);
    let name = String::from_utf8_lossy(&d[start..at + 4])
        .trim()
        .to_string();
    (name.len() > 4).then_some(name)
}

/// A file named `name` under `root` (any depth, case ignored).
pub fn find_file(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let want = name.to_lowercase();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if e.file_name().to_string_lossy().to_lowercase() == want {
                return Some(p);
            }
        }
    }
    None
}

/// A VST hosted in FL's wrapper, with its state in KeyFrameXL's format.
#[derive(Debug, Clone, Default)]
pub struct FlpVst {
    pub name: String,
    pub vendor: String,
    /// The plugin file as FL saw it (Windows path).
    pub path: String,
    pub vst3: bool,
    /// VST2: `KFV2C` + the plugin's chunk (as the program stores VST2
    /// state); VST3: the component state (what the VST3 host restores).
    pub state: Vec<u8>,
}

/// Records of FL's VST wrapper data: `u32 id, u64 length, data`.
fn wrapper_records(d: &[u8], from: usize) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let mut p = from;
    while p + 12 <= d.len() {
        let id = le32(d, p);
        let n = d
            .get(p + 4..p + 12)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            as usize;
        let Some(data) = d.get(p + 12..p + 12 + n) else {
            break;
        };
        out.push((id, data));
        p += 12 + n;
    }
    out
}

/// The VST in a "Fruity Wrapper" channel's plugin data (event 213): 54 name,
/// 55 path, 56 vendor, 50 kind (8 = VST3), 53 state. FL wraps the state:
/// VST2 as 21 header bytes (the chunk's length at byte 9), then the chunk;
/// VST3 as a version and records of their own, 3 being the component state.
fn wrapper_vst(d: &[u8]) -> Option<FlpVst> {
    let recs = wrapper_records(d, 4);
    let get = |id: u32| recs.iter().find(|(i, _)| *i == id).map(|(_, v)| *v);
    let latin = |b: &[u8]| {
        b.iter()
            .map(|&c| c as char)
            .collect::<String>()
            .trim_end_matches('\0')
            .to_string()
    };
    let name = latin(get(54)?);
    let vst3 = get(50).and_then(|k| k.first()) == Some(&8);
    let raw = get(53).unwrap_or_default();
    let state = if vst3 {
        wrapper_records(raw, 4)
            .into_iter()
            .find(|(i, _)| *i == 3)
            .map(|(_, v)| v.to_vec())
            .unwrap_or_default()
    } else {
        let n = raw
            .get(9..17)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            as usize;
        raw.get(21..21 + n)
            .map(|chunk| [b"KFV2C".as_slice(), chunk].concat())
            .unwrap_or_default()
    };
    Some(FlpVst {
        name,
        vendor: get(56).map(latin).unwrap_or_default(),
        path: get(55).map(latin).unwrap_or_default(),
        vst3,
        state,
    })
}

/// Resolve an FL Studio path natively (without a Wine prefix): expands
/// `%FLStudioFactoryData%` and `%FLStudioUserData%`, normalizes separators,
/// and returns the path if it exists on disk.
pub fn resolve_native_path(
    fl_path: &str,
    fl_install: Option<&std::path::Path>,
    user_data: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    let lower = fl_path.to_ascii_lowercase();
    let candidate = if lower.starts_with("%flstudiofactorydata%") {
        let mut p = fl_install?.to_path_buf();
        let rest = &fl_path["%FLStudioFactoryData%".len()..];
        for seg in rest.split(['\\', '/']).filter(|s| !s.is_empty()) {
            p.push(seg);
        }
        p
    } else if lower.starts_with("%flstudiouserdata%") {
        let mut p = user_data?.to_path_buf();
        let rest = &fl_path["%FLStudioUserData%".len()..];
        for seg in rest.split(['\\', '/']).filter(|s| !s.is_empty()) {
            p.push(seg);
        }
        p
    } else if cfg!(target_os = "windows") {
        std::path::PathBuf::from(fl_path.replace('/', "\\"))
    } else {
        std::path::PathBuf::from(fl_path.replace('\\', "/"))
    };
    candidate.exists().then_some(candidate)
}

/// Where a path FL saved lives on this computer: FL's variables
/// (`%FLStudioFactoryData%` = the FL install, `%FLStudioUserData%` = the user's
/// Image-Line folder) and native Windows drives (or Wine `dosdevices` on Linux).
pub fn resolve_path(
    fl_path: &str,
    prefix: &std::path::Path,
    fl_install: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let _ = prefix;
        let user_data = dirs::document_dir().map(|d| d.join("Image-Line"));
        resolve_native_path(fl_path, fl_install, user_data.as_deref())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let user = std::env::var("USER").unwrap_or_default();
        let mut s = fl_path.replace('/', "\\");
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("%flstudiofactorydata%") {
            let install = fl_install?.to_string_lossy().into_owned();
            s = format!("{install}{}", &s["%FLStudioFactoryData%".len()..]);
        } else if lower.starts_with("%flstudiouserdata%") {
            s = format!(
                "C:\\users\\{user}\\Documents\\Image-Line{}",
                &s["%FLStudioUserData%".len()..]
            );
        }
        let unix = if s.starts_with('/') {
            std::path::PathBuf::from(s.replace('\\', "/"))
        } else {
            let (drive, rest) = s.split_once(':')?;
            let root = match drive.to_ascii_lowercase().as_str() {
                "z" => std::path::PathBuf::from("/"),
                d => prefix.join("dosdevices").join(format!("{d}:")),
            };
            root.join(rest.trim_start_matches('\\').replace('\\', "/"))
        };
        if unix.exists() {
            return Some(unix);
        }
        // Wine paths are case-insensitive: find each part ignoring case.
        let mut found = std::path::PathBuf::from("/");
        for part in unix.components().skip(1) {
            let want = part.as_os_str().to_string_lossy().to_lowercase();
            let next = std::fs::read_dir(&found)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().to_lowercase() == want)?;
            found = next.path();
        }
        Some(found)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ChannelKind {
    #[default]
    Sampler,
    Generator,
    Layer,
    Automation,
    Other(u32),
}

/// What the import made and what it left behind.
#[derive(Debug, Default)]
pub struct FlpReport {
    pub fl_version: String,
    pub tempo: f64,
    pub channels: Vec<FlpChannel>,
    pub patterns: usize,
    pub notes: usize,
    /// FL slide notes that became glides, and slides with no note under them.
    pub slides: usize,
    pub slides_dropped: usize,
    pub clips: usize,
    pub audio_clips: usize,
    /// Audio clips placed (as one-note patterns of their sampler channel).
    pub audio_imported: usize,
    /// Sampler instrument → where its clips start in the sample and its
    /// pitch. The program applies it with `slice_preset` once the sample is
    /// loaded.
    pub audio_slices: Vec<(u64, AudioSlices)>,
    /// Audio clips whose stretch mode keeps time (a sampler cannot): their
    /// pitch is kept, their length follows the pitch.
    pub audio_stretched: usize,
    /// Audio clips with fades (each becomes its own slice).
    pub audio_faded: usize,
    pub automation_clips: usize,
    pub arrangements: usize,
    pub meter: (u32, u32),
    /// Notes of channels that did not become instruments (automation, layers).
    pub dropped_notes: usize,
    pub inserts: Vec<FlpInsert>,
    /// FL effects with no built-in counterpart, by name.
    pub skipped_effects: Vec<String>,
    /// FL effects that became built-ins with their settings carried over.
    pub converted_effects: usize,
    /// Pattern automation events whose control has no known scale here.
    pub pattern_events_skipped: usize,
    /// Points of the song tempo track made from FL's tempo automation.
    pub tempo_points: usize,
    /// Inserts whose own EQ became a built-in EQ slot.
    pub insert_eqs: usize,
    /// FL effects that became built-ins at our defaults (added switched off).
    pub stand_in_effects: usize,
    /// Their FL names.
    pub stand_in_names: Vec<String>,
    /// What did not carry over in converted effects.
    pub effect_notes: Vec<String>,
    /// Automation clips made (one per controlled target).
    pub automations: usize,
    /// Automation that could not be moved, with why.
    pub automation_notes: Vec<String>,
    /// FL generators played by a Tide stand-in (until FL Studio VSTi takes
    /// over), one line each.
    pub generator_stand_ins: Vec<String>,
}

impl FlpReport {
    /// Lines for the import report popup.
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!(
            "FL Studio {} · {:.3} BPM · {}/{} · {} channels · {} patterns · {} notes · {} playlist clips",
            self.fl_version,
            self.tempo,
            self.meter.0,
            self.meter.1,
            self.channels.len(),
            self.patterns,
            self.notes,
            self.clips
        )];
        if self.slides > 0 || self.slides_dropped > 0 {
            v.push(format!(
                "{} slide(s) became pitch glides{}",
                self.slides,
                if self.slides_dropped > 0 {
                    format!(
                        ", {} left out: no note sounding under their start",
                        self.slides_dropped
                    )
                } else {
                    String::new()
                }
            ));
        }
        for c in &self.channels {
            let what = match c.kind {
                ChannelKind::Sampler => match &c.sample {
                    Some(s) => format!("sampler, sample {s}"),
                    None => "sampler without a sample".into(),
                },
                ChannelKind::Generator => match (&c.vst, &c.dwp) {
                    (Some(v), _) => format!("VST{} {}", if v.vst3 { "3" } else { "2" }, v.name),
                    (None, Some(d)) => format!("DirectWave {d} — in the KeyFrameXL sampler"),
                    (None, None) if c.in_native_bridge() => {
                        format!("{} — standalone FL Studio bridge", c.plugin)
                    }
                    (None, None) => format!("{} — in FL Studio VSTi", c.plugin),
                },
                ChannelKind::Layer => "layer channel — skipped".into(),
                ChannelKind::Automation => "automation clip".into(),
                ChannelKind::Other(t) => format!("channel type {t} — skipped"),
            };
            let pitch = if c.pitch != 0 && c.instrument.is_some() {
                if c.in_native_bridge() {
                    format!(" · pitch {:+} cents (kept by FL Studio)", c.pitch)
                } else if c.in_fl_vsti() {
                    format!(" · pitch {:+} cents (kept by FL Studio VSTi)", c.pitch)
                } else if c.vst.is_some() && c.pitch % 100 != 0 {
                    format!(
                        " · pitch {:+} cents: whole semitones moved the notes, the {:+} cent rest is not sent to plugins",
                        c.pitch,
                        c.pitch % 100
                    )
                } else {
                    format!(" · pitch {:+} cents moved into the notes", c.pitch)
                }
            } else {
                String::new()
            };
            v.push(format!("{}: {what}{pitch}", c.name));
        }
        v.extend(self.generator_stand_ins.iter().cloned());
        if self.automations > 0 {
            v.push(format!("{} automation clip(s) imported", self.automations));
        }
        v.extend(
            self.automation_notes
                .iter()
                .map(|n| format!("Automation not imported: {n}")),
        );
        if self.automation_clips > 0 {
            v.push(format!(
                "{} automation clip(s) in the playlist not imported",
                self.automation_clips
            ));
        }
        if self.audio_imported > 0 {
            v.push(format!("{} audio clip(s) imported (one note of their sample channel each; fades kept, FL's clip volume is not read)", self.audio_imported));
        }
        if self.audio_stretched > 0 {
            v.push(format!("{} audio clip(s) use a time-keeping stretch mode: their pitch is kept, their timing follows it", self.audio_stretched));
        }
        if self.audio_clips > 0 {
            v.push(format!(
                "{} audio clip(s) in the playlist not imported",
                self.audio_clips
            ));
        }
        if self.arrangements > 1 {
            v.push(format!(
                "{} arrangements: only the first one imported",
                self.arrangements
            ));
        }
        if self.dropped_notes > 0 {
            v.push(format!(
                "{} note(s) of skipped channels left out",
                self.dropped_notes
            ));
        }
        if self.converted_effects > 0 {
            v.push(format!("{} FL effect(s) became the nearest built-in effect with their settings carried over (close, not identical: the algorithms differ)", self.converted_effects));
        }
        v.extend(self.effect_notes.iter().cloned());
        if self.stand_in_effects > 0 {
            let mut names = self.stand_in_names.clone();
            names.sort();
            names.dedup();
            v.push(format!("{} FL effect(s) became the nearest built-in effect, switched off and at default settings (their settings are not read yet): {} — switch them on to compare", self.stand_in_effects, names.join(", ")));
        }
        if !self.skipped_effects.is_empty() {
            v.push(format!(
                "Effects with no counterpart here, left out: {}",
                self.skipped_effects.join(", ")
            ));
        }
        if self.insert_eqs > 0 {
            v.push(format!(
                "{} insert EQ(s) carried over (the strip EQ, or an EQ slot when its bands were moved)",
                self.insert_eqs
            ));
        }
        if self.tempo_points > 0 {
            v.push(format!(
                "Tempo automation became the song's tempo track ({} points)",
                self.tempo_points
            ));
        }
        v.push(
            "Not imported yet: FL audio clip volume, fade curves (tension) and time stretching."
                .into(),
        );
        v
    }
}

/// A playlist record of a pattern or audio clip.
struct Item {
    position: u32,
    index: u16,
    length: u32,
    track: u32,
    muted: bool,
    /// Pattern clips: ticks into the pattern (-1 = none).
    start_offset: i32,
    /// Automation clips: the same field as a float, in beats. Audio clips:
    /// milliseconds into the sample (FL 21-25.1).
    start_beats: f32,
    /// Audio clips: where the sample window ends (as `start_beats`/`start_offset`).
    end_raw: u32,
    /// Audio clip fades in ms (records of 60 bytes and more).
    fade_in_ms: f32,
    fade_out_ms: f32,
    /// 88-byte records (FL 25.2+): the sample window as playlist ticks.
    window_ticks: bool,
}

impl Item {
    fn window_secs(&self, raw: u32, seconds_per_tick: f64) -> f64 {
        let s = if self.window_ticks {
            raw as i32 as f64 * seconds_per_tick
        } else {
            let ms = f32::from_bits(raw);
            if ms.is_finite() {
                ms as f64 / 1000.0
            } else {
                0.0
            }
        };
        s.max(0.0)
    }

    /// An audio clip's start in its sample, in seconds.
    fn audio_start(&self, seconds_per_tick: f64) -> f64 {
        self.window_secs(self.start_offset as u32, seconds_per_tick)
    }

    /// The part of its sample an audio clip plays.
    fn audio_slice(&self, seconds_per_tick: f64) -> AudioSlice {
        let start = self.audio_start(seconds_per_tick);
        let end = self.window_secs(self.end_raw, seconds_per_tick);
        let ms = |v: f32| {
            if v.is_finite() {
                (v as f64 / 1000.0).max(0.0)
            } else {
                0.0
            }
        };
        let (fade_in, fade_out) = (ms(self.fade_in_ms), ms(self.fade_out_ms));
        AudioSlice {
            start,
            end: (fade_out > 0.0 && end > start).then_some(end),
            fade_in,
            fade_out,
        }
    }
}

/// Playlist record size: from the layout event when the file has one (FL
/// 2025+), else the first size whose records all carry FL's pattern base:
/// 88 (FL 25.2+), 80 (FL 24.2-25.1), 60 (FL 20.99-24.1), 32 (FL 12.3-20).
fn playlist_items(d: &[u8], layout: Option<u32>) -> Vec<Item> {
    let fits = |s: usize| {
        s > 0 && d.len().is_multiple_of(s) && d.chunks_exact(s).all(|it| le16(it, 4) == 20480)
    };
    let declared = layout.map(|l| match l {
        l if l & 0x2 == 0 => 60,
        l if l & 0x8 == 0 => 80,
        _ => 88,
    });
    let Some(size) = declared
        .filter(|s| fits(*s))
        .or_else(|| [88usize, 80, 60, 32].into_iter().find(|s| fits(*s)))
    else {
        return vec![];
    };
    d.chunks_exact(size)
        .map(|it| Item {
            position: le32(it, 0),
            index: le16(it, 6),
            length: le32(it, 8),
            track: 499u32.saturating_sub(le16(it, 12) as u32),
            // 0x2000 is FL's mute (performance-mode clips carry it); 0x1000
            // rides along with it on some clips and is something else.
            muted: le16(it, 18) & 0x2000 != 0,
            start_offset: le32(it, 24) as i32,
            start_beats: f32::from_bits(le32(it, 24)),
            end_raw: le32(it, 28),
            fade_in_ms: if size >= 60 {
                f32::from_bits(le32(it, 36))
            } else {
                0.0
            },
            fade_out_ms: if size >= 60 {
                f32::from_bits(le32(it, 44))
            } else {
                0.0
            },
            window_ticks: size == 88,
        })
        .collect()
}

/// Read an FL project into a new song.
pub fn read_flp(bytes: &[u8]) -> Result<(Song, FlpReport), String> {
    let (ppq, evs) = events(bytes)?;
    let scale = |t: u32| -> Tick { (t as u64 * PPQ as u64 / ppq as u64) as Tick };
    let mut report = FlpReport {
        tempo: 120.0,
        meter: (4, 4),
        ..Default::default()
    };
    let mut channels: Vec<FlpChannel> = Vec::new();
    // Pattern number → (name, colour, raw notes).
    let mut patterns: BTreeMap<u32, (String, Option<u32>, Vec<u8>)> = BTreeMap::new();
    // Pattern number → its automation events (event 223).
    let mut pattern_events: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    // Read after all events: the layout event may come after the records.
    let mut playlist: Vec<&[u8]> = Vec::new();
    let mut playlist_layout: Option<u32> = None;
    let mut tracks: BTreeMap<u32, Track> = BTreeMap::new();
    #[derive(PartialEq)]
    enum Ctx {
        None,
        Channel,
        Pattern(u32),
        Mixer,
    }
    let mut ctx = Ctx::None;
    let mut track: Option<u32> = None;
    let mut inserts: Vec<FlpInsert> = Vec::new();
    // The slot being described (its plugin events come before its index).
    let mut slot = FlpSlot::default();
    // An insert's name is written after its last event, ahead of the next.
    let (mut insert_open, mut next_name) = (false, None::<String>);
    let mut params: &[u8] = &[];
    let mut initialized: &[u8] = &[];
    // Automation links: (source channel, FL parameter, destination).
    let mut links: Vec<(u32, u16, u16)> = Vec::new();
    for (id, v) in evs {
        if MIXER_START.contains(&id) {
            ctx = Ctx::Mixer;
        }
        match (id, v) {
            (CH_NEW, Value::Num(i)) => {
                channels.push(FlpChannel {
                    index: i,
                    enabled: true,
                    ..Default::default()
                });
                ctx = Ctx::Channel;
            }
            (PAT_NEW, Value::Num(i)) => {
                patterns.entry(i).or_default();
                ctx = Ctx::Pattern(i);
            }
            (ARR_NEW, _) => report.arrangements += 1,
            (TEMPO, Value::Num(t)) => report.tempo = t as f64 / 1000.0,
            (METER_NUM, Value::Num(n)) if n > 0 => report.meter.0 = n,
            (METER_DEN, Value::Num(n)) if n > 0 => report.meter.1 = n,
            (FL_VERSION, Value::Data(d)) => {
                report.fl_version = String::from_utf8_lossy(d)
                    .trim_end_matches('\0')
                    .to_string()
            }
            (PLAYLIST_LAYOUT, Value::Num(l)) => playlist_layout = Some(l),
            (PLAYLIST, Value::Data(d)) if report.arrangements <= 1 => playlist.push(d),
            (TRACK_DATA, Value::Data(d)) if report.arrangements <= 1 => {
                let i = le32(d, 0);
                track = Some(i);
                let t = Track {
                    name: String::new(),
                    muted: d.get(12) == Some(&0),
                    color: Some(rgb(le32(d, 4))),
                };
                tracks.insert(i, t);
            }
            (MIXER_PARAMS, Value::Data(d)) => params = d,
            (INITIALIZED_CONTROLS, Value::Data(d)) => initialized = d,
            (AUTO_LINK, Value::Data(d)) if d.len() >= 12 => {
                links.push((le16(d, 2) as u32, le16(d, 8), le16(d, 10)))
            }
            (INSERT_FLAGS, Value::Data(d)) => {
                let flags = le32(d, 4);
                inserts.push(FlpInsert {
                    index: inserts.len() as u32,
                    enabled: flags & 8 != 0,
                    bypass_effects: flags & 4 == 0,
                    name: next_name.take().unwrap_or_default(),
                    ..Default::default()
                });
                slot = FlpSlot::default();
                insert_open = true;
            }
            (INSERT_OUTPUT, _) => insert_open = false,
            (INSERT_NAME, Value::Data(d)) if ctx == Ctx::Mixer => match inserts.last_mut() {
                Some(ins) if insert_open => ins.name = text(d),
                _ => next_name = Some(text(d)),
            },
            (TRACK_NAME, Value::Data(d)) => {
                if let Some(t) = track.and_then(|i| tracks.get_mut(&i)) {
                    t.name = text(d);
                }
            }
            _ => {}
        }
        match ctx {
            Ctx::Channel => {
                let Some(c) = channels.last_mut() else {
                    continue;
                };
                match (id, v) {
                    (CH_TYPE, Value::Num(t)) => {
                        c.kind = match t {
                            0 => ChannelKind::Sampler,
                            2 | 4 => ChannelKind::Generator,
                            3 => ChannelKind::Layer,
                            5 => ChannelKind::Automation,
                            t => ChannelKind::Other(t),
                        }
                    }
                    (CH_ENABLED, Value::Num(e)) => c.enabled = e != 0,
                    (CH_INSERT, Value::Num(i)) => c.insert = Some(i),
                    (PLUGIN_INTERNAL, Value::Data(d)) => c.plugin = text(d),
                    (PLUGIN_DATA, Value::Data(d)) => {
                        if c.plugin == "Fruity Wrapper" {
                            c.vst = wrapper_vst(d);
                        } else if c.plugin == "DirectWave" {
                            c.dwp = dwp_name(d);
                        }
                        c.data = d.to_vec();
                    }
                    (PLUGIN_NAME, Value::Data(d)) => c.name = text(d),
                    (SAMPLE_PATH, Value::Data(d)) => {
                        c.sample = Some(text(d)).filter(|s| !s.is_empty())
                    }
                    (CH_COLOR, Value::Num(col)) => c.color = Some(rgb(col)),
                    (AUTO_POINTS, Value::Data(d)) => c.points = auto_points(d),
                    (CH_PARAMS, Value::Data(d)) if d.len() >= 112 => {
                        c.stretch = Some(FlpStretch {
                            pitch: le32(d, 100) as i32,
                            multiplier: le32(d, 104) as i32,
                            mode: le32(d, 108) as i32,
                        })
                        .filter(|s| s.pitch != 0 || s.multiplier != 0);
                    }
                    (CH_LEVELS, Value::Data(d)) if d.len() >= 8 => {
                        c.pan = Some(le32(d, 0));
                        c.volume = Some(le32(d, 4));
                        c.pitch = le32(d, 8) as i32;
                    }
                    _ => {}
                }
            }
            Ctx::Pattern(i) => {
                let p = patterns.entry(i).or_default();
                match (id, v) {
                    (PAT_NAME, Value::Data(d)) => p.0 = text(d),
                    (PAT_COLOR, Value::Num(c)) => p.1 = Some(rgb(c)),
                    (PAT_NOTES, Value::Data(d)) => p.2.extend_from_slice(d),
                    (PAT_EVENTS, Value::Data(d)) => {
                        pattern_events.entry(i).or_default().extend_from_slice(d)
                    }
                    _ => {}
                }
            }
            Ctx::Mixer => {
                let Some(ins) = inserts.last_mut() else {
                    continue;
                };
                match (id, v) {
                    (PLUGIN_INTERNAL, Value::Data(d)) => slot.plugin = text(d),
                    (PLUGIN_NAME, Value::Data(d)) => slot.name = text(d),
                    (PLUGIN_DATA, Value::Data(d)) => {
                        if slot.plugin == "Fruity Wrapper" {
                            slot.vst = wrapper_vst(d);
                        }
                        slot.data = d.to_vec();
                    }
                    (SLOT_INDEX, Value::Num(i)) => {
                        let done = std::mem::take(&mut slot);
                        if !done.plugin.is_empty() {
                            ins.slots.push(FlpSlot {
                                slot: i,
                                enabled: true,
                                mix: 1.0,
                                ..done
                            });
                        }
                    }
                    (INSERT_ROUTING, Value::Data(d)) => ins.routes = d.to_vec(),
                    _ => {}
                }
            }
            Ctx::None => {}
        }
    }

    // Mixer parameters: 12-byte records, the insert and slot packed in one
    // word (insert << 6 | slot), the value last.
    let initial_mixer_values = initialized
        .as_chunks::<12>()
        .0
        .iter()
        .filter(|r| r[5] == 0x1f);
    for r in params
        .as_chunks::<12>()
        .0
        .iter()
        .chain(initial_mixer_values)
    {
        let (pid, packed, value) = (r[4], le16(r, 6), le32(r, 8) as i32);
        let Some((insert, at)) = mixer_address(packed) else {
            continue;
        };
        let Some(ins) = inserts.get_mut(insert) else {
            continue;
        };
        match pid {
            0 => ins
                .slots
                .iter_mut()
                .filter(|s| s.slot == at)
                .for_each(|s| s.enabled = value != 0),
            1 => ins
                .slots
                .iter_mut()
                .filter(|s| s.slot == at)
                .for_each(|s| s.mix = (value as f32 / 12800.0).clamp(0.0, 1.0)),
            64..=191 => {
                ins.levels.insert(pid as u32 - 64, value);
            }
            192 => ins.volume = Some(value),
            193 => ins.pan = Some(value),
            194 => ins.sep = Some(value),
            208..=210 | 216..=218 | 224..=226 => {
                let eq = ins
                    .eq
                    .get_or_insert([0, 0, 0, 5777, 33145, 55825, 17500, 17500, 17500]);
                eq[(pid as usize - 208) / 8 * 3 + (pid as usize - 208) % 8] = value;
            }
            _ => {}
        }
    }

    let mut song = Song::default();
    song.instruments.clear();
    song.patterns.clear();
    song.clips.clear();
    song.tempo = report.tempo.clamp(10.0, 999.0);
    // FL channel index → instrument id.
    let mut by_channel: BTreeMap<u32, u64> = BTreeMap::new();
    for (n, c) in channels.iter_mut().enumerate() {
        // Audio clip channels: type 4 with no plugin, just a sample.
        if c.kind == ChannelKind::Generator && c.plugin.is_empty() {
            c.kind = ChannelKind::Sampler;
        }
        if c.name.is_empty() {
            c.name = if c.plugin.is_empty() {
                format!("Channel {}", n + 1)
            } else {
                c.plugin.clone()
            };
        }
        // FL's own generators FL Studio VSTi would play: a Tide stand-in
        // where there is one, so the channel is not silent without it.
        let stand_in = (c.in_fl_vsti() || c.plugin.eq_ignore_ascii_case("FLEX"))
            .then(|| crate::flp_gen::stand_in(&c.plugin, &c.data))
            .flatten();
        let kind = match (c.kind, stand_in) {
            (ChannelKind::Generator, Some((patch, line))) => {
                report
                    .generator_stand_ins
                    .push(format!("{}: {line}", c.name));
                InstrumentKind::Synth { patch }
            }
            (ChannelKind::Sampler | ChannelKind::Generator, _) => {
                InstrumentKind::Sampler { preset: None }
            }
            _ => continue,
        };
        let id = song.instruments.len() as u64 + 1;
        let mut inst = Instrument::new(id, &c.name, kind);
        if !c.plugin.is_empty() {
            inst.fl_plugin = Some(FlPluginState::new(&c.plugin, &c.data));
        }
        // FL's channel volume runs 0..12800 with 10000 at 0 dB, roughly as
        // amplitude squared; pan 0..12800 with the centre at 6400.
        if let Some(v) = c.volume {
            inst.volume = ((v as f32 / 10000.0).powi(2)).min(crate::sequence::FADER_MAX);
        }
        if let Some(p) = c.pan {
            inst.pan = ((p as f32 - 6400.0) / 6400.0).clamp(-1.0, 1.0);
        }
        inst.muted = !c.enabled;
        song.instruments.push(inst);
        by_channel.insert(c.index, id);
        c.instrument = Some(id);
    }
    // Channel index → its pitch knob in cents, where KeyFrameXL has to apply
    // it (hosted FL channels apply their own).
    let channel_pitch: BTreeMap<u32, i32> = channels
        .iter()
        .filter(|c| c.pitch != 0 && !c.in_fl_vsti() && !c.in_native_bridge())
        .map(|c| (c.index, c.pitch))
        .collect();
    let native: std::collections::BTreeSet<_> = channels
        .iter()
        .filter(|c| c.in_native_bridge())
        .filter_map(|c| c.instrument)
        .collect();
    for (number, (name, color, raw)) in &patterns {
        let title = if name.is_empty() {
            format!("Pattern {number}")
        } else {
            name.clone()
        };
        let mut p = Pattern::new(*number as u64, &title);
        p.color = *color;
        // FL slide notes draw as slides, not notes: they become glides after
        // the pattern is read, once the note they bend is known.
        let mut sliding = Vec::new();
        for r in raw.as_chunks::<24>().0 {
            let channel = le16(r, 6) as u32;
            let Some(&inst) = by_channel.get(&channel) else {
                report.dropped_notes += 1;
                continue;
            };
            // FL's note fine pitch: 0..240, 120 = in tune, one step = 10 cents
            // (±1200). With the channel's pitch knob, whole semitones move
            // the key, the rest (±50 cents) stays as the note's fine pitch.
            let cents =
                (r[16] as i32 - 120) * 10 + channel_pitch.get(&channel).copied().unwrap_or(0);
            let semis = (cents as f32 / 100.0).round() as i32;
            let key = (le16(r, 12) as i32 + semis).clamp(0, 127) as u8;
            if le16(r, 4) & 0x08 != 0 {
                sliding.push((
                    inst,
                    scale(le32(r, 0)),
                    scale(le32(r, 8)).max(1),
                    le16(r, 14) as u8,
                    key,
                ));
                continue;
            }
            // FL velocity is 0..128 with MIDI's scale (100 = MIDI 100).
            let mut note = Note::new(
                inst,
                scale(le32(r, 0)),
                scale(le32(r, 8)).max(1),
                key,
                r[21].clamp(1, 127),
            );
            note.pan = ((r[20] as f32 - 64.0) / 64.0).clamp(-1.0, 1.0);
            note.release = r[18] as f32 / 128.0;
            note.fine = (cents - semis * 100) as f32;
            note.mod_x = r[22] as f32 / 128.0;
            note.mod_y = r[23] as f32 / 128.0;
            note.group = le16(r, 14).min(255) as u8;
            if native.contains(&inst) {
                note.color = note.group;
            }
            p.notes.push(note);
        }
        // A slide bends the note sounding under its start: FL's group when
        // both carry one, else the top sounding note takes a group of its own
        // (a slide's group is its target channel here).
        let mut used = p.notes.iter().map(|n| n.group).max().unwrap_or(0);
        for (inst, start, length, group, target) in sliding {
            let mut active: Vec<usize> = p
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    n.instrument == inst
                        && n.start <= start
                        && n.end() > start
                        && (!native.contains(&inst) || n.group == group)
                })
                .map(|(i, _)| i)
                .collect();
            active.sort_by_key(|&i| std::cmp::Reverse(p.notes[i].pitch));
            let Some(&top) = active.first() else {
                report.slides_dropped += 1;
                continue;
            };
            // A slide names its group: only notes in it can bend. Unnamed
            // notes (group 0) take the slide's group when nothing else can.
            let in_group: Vec<usize> = active
                .iter()
                .copied()
                .filter(|&i| p.notes[i].group == group)
                .collect();
            let g = if native.contains(&inst) || (group != 0 && !in_group.is_empty()) {
                group
            } else if group == 0 && p.notes[top].group != 0 {
                p.notes[top].group
            } else if group != 0 {
                for &i in &active {
                    p.notes[i].group = group;
                }
                group
            } else {
                used = used.saturating_add(1).max(1);
                for &i in &active {
                    p.notes[i].group = used;
                }
                used
            };
            p.slides.push(Glide {
                start,
                length,
                target,
                group: g,
                instrument: inst,
            });
        }
        let end = p.notes.iter().map(|n| n.end()).max().unwrap_or(0);
        p.length = (end.max(1) + BAR - 1) / BAR * BAR;
        report.notes += p.notes.len();
        report.slides += p.slides.len();
        song.patterns.push(p.into());
    }
    report.patterns = song.patterns.len();
    // Inserts that only automation names still have to exist.
    let automated = links
        .iter()
        .filter(|l| l.2 & 0x2000 != 0)
        .map(|l| ((l.2 & 0x0fff) >> 6) as u32)
        .max()
        .unwrap_or(0);
    // Inserts whose EQ is automated get one even when it is flat.
    let eq_automated: Vec<u32> = links
        .iter()
        .filter(|l| {
            l.2 & 0x2000 != 0
                && matches!(l.1 & 0xff, 208..=210 | 216..=218)
                && l.1 & 0xff00 == 0x1f00
        })
        .map(|l| ((l.2 & 0x0fff) >> 6) as u32)
        .collect();
    let placed = build_mixer(
        &mut song,
        &channels,
        &inserts,
        automated,
        &eq_automated,
        &mut report,
    );
    // Automation recorded into patterns (FL's event editor; a tempo
    // ritardando, knob moves): each pattern's events per control become an
    // automation channel of their own, placed wherever the pattern is.
    let mut auto_channels = channels.clone();
    let mut auto_links = links.clone();
    let mut pattern_autos: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut pattern_tempos: BTreeMap<u32, Vec<(Tick, f64)>> = BTreeMap::new();
    for (number, raw) in &pattern_events {
        let mut by_control: BTreeMap<(u16, u16), Vec<(u32, i32)>> = BTreeMap::new();
        for r in raw.as_chunks::<12>().0 {
            by_control
                .entry((le16(r, 4), le16(r, 6)))
                .or_default()
                .push((le32(r, 0), le32(r, 8) as i32));
        }
        let (pname, pcolor) = patterns
            .get(number)
            .map(|p| (p.0.clone(), p.1))
            .unwrap_or_default();
        for ((param, dest), mut evs) in by_control {
            if (param, dest) == (5, 0x4000) {
                evs.sort_by_key(|e| e.0);
                let points = evs
                    .into_iter()
                    .map(|(pos, value)| (scale(pos), value as f64 / 1000.0));
                pattern_tempos.entry(*number).or_default().extend(points);
                continue;
            }
            let Some(norm) = event_scale(param, dest) else {
                report.pattern_events_skipped += evs.len();
                continue;
            };
            evs.sort_by_key(|e| e.0);
            let mut points = Vec::with_capacity(evs.len());
            let mut at = 0u32;
            for (pos, v) in evs {
                points.push(((pos - at) as f64 / ppq as f64, norm(v), 0.0));
                at = pos;
            }
            let index = 0x1_0000 + auto_channels.len() as u32;
            auto_channels.push(FlpChannel {
                index,
                name: if pname.is_empty() {
                    format!("Pattern {number} events")
                } else {
                    format!("{pname} (events)")
                },
                kind: ChannelKind::Automation,
                color: pcolor,
                enabled: true,
                points,
                ..Default::default()
            });
            auto_links.push((index, param, dest));
            pattern_autos.entry(*number).or_default().push(index);
        }
    }
    let (autos, tempo) = build_automation(
        &mut song,
        &auto_channels,
        &inserts,
        &auto_links,
        &placed,
        &mut report,
    );
    let mut tempo_clips: Vec<(Tick, Tick, Tick, &[(Tick, f64)])> = Vec::new();
    let mut tempo_events = Vec::new();
    // Audio clips: per sampler channel, the start points used (in beats) and
    // a one-note pattern for each.
    let mut slices: BTreeMap<u64, AudioSlices> = BTreeMap::new();
    let mut audio_patterns: BTreeMap<(u64, usize), u64> = BTreeMap::new();
    let mut next_id = song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    let items: Vec<Item> = playlist
        .iter()
        .flat_map(|d| playlist_items(d, playlist_layout))
        .collect();
    let seconds_per_tick = 60.0 / report.tempo.max(1.0) / ppq.max(1) as f64;
    for it in &items {
        let active = !it.muted && !tracks.get(&(it.track + 1)).is_some_and(|t| t.muted);
        // A tempo automation clip (it may drive other controls too).
        if it.index < 20480
            && let Some(pts) = tempo.get(&(it.index as u32))
        {
            let offset = if it.start_beats.is_finite() && it.start_beats > 0.0 {
                (it.start_beats as f64 * PPQ as f64).round() as Tick
            } else {
                0
            };
            if active {
                tempo_clips.push((
                    scale(it.position),
                    offset,
                    scale(it.length).max(1),
                    pts.as_slice(),
                ));
            }
            if !autos.contains_key(&(it.index as u32)) {
                continue;
            }
        }
        if it.index > 20480 {
            let pattern = (it.index - 20480) as u64;
            if !song.patterns.iter().any(|p| p.id == pattern) {
                continue;
            }
            let offset = if it.start_offset > 0 {
                scale(it.start_offset as u32)
            } else {
                0
            };
            song.clips.push(Clip {
                pattern,
                start: scale(it.position),
                track: it.track,
                offset,
                length: Some(scale(it.length).max(1)),
                muted: it.muted,
            });
            if active && let Some(points) = pattern_tempos.get(&(pattern as u32)) {
                let end = offset + scale(it.length).max(1);
                let start = scale(it.position);
                let first = points.partition_point(|p| p.0 < offset);
                if let Some(previous) = first.checked_sub(1) {
                    tempo_events.push((start, points[previous].1));
                }
                tempo_events.extend(
                    points[first..]
                        .iter()
                        .take_while(|p| p.0 < end)
                        .map(|&(tick, bpm)| (start + tick - offset, bpm)),
                );
            }
            // The pattern's own automation plays along.
            for ch in pattern_autos.get(&(pattern as u32)).into_iter().flatten() {
                if active && let Some(pts) = tempo.get(ch) {
                    tempo_clips.push((
                        scale(it.position),
                        offset,
                        scale(it.length).max(1),
                        pts.as_slice(),
                    ));
                }
                for &a in autos.get(ch).into_iter().flatten() {
                    song.clips.push(Clip {
                        pattern: a,
                        start: scale(it.position),
                        track: it.track,
                        offset,
                        length: Some(scale(it.length).max(1)),
                        muted: it.muted,
                    });
                }
            }
        } else if let Some(made) = autos.get(&(it.index as u32)) {
            for &pattern in made {
                song.clips.push(Clip {
                    pattern,
                    start: scale(it.position),
                    track: it.track,
                    offset: if it.start_beats.is_finite() && it.start_beats > 0.0 {
                        (it.start_beats as f64 * PPQ as f64).round() as Tick
                    } else {
                        0
                    },
                    length: Some(scale(it.length).max(1)),
                    muted: it.muted,
                });
            }
        } else if channels
            .iter()
            .any(|c| c.index == it.index as u32 && c.kind == ChannelKind::Automation)
        {
            report.automation_clips += 1;
        } else if let Some(c) = channels.iter().find(|c| {
            c.index == it.index as u32 && c.kind == ChannelKind::Sampler && c.sample.is_some()
        }) && let Some(inst) = c.instrument
        {
            // FL's audio clip: its channel's sample from the clip's start
            // offset, for the clip's length. Each start point is a key of
            // the sampler (60 = the beginning, 61.. = later points).
            let at = it.audio_slice(seconds_per_tick);
            if c.stretch.is_some_and(|s| s.mode != 0) {
                report.audio_stretched += 1;
            }
            if at.fade_in > 0.0 || at.fade_out > 0.0 {
                report.audio_faded += 1;
            }
            let entry = slices.entry(inst).or_insert_with(|| AudioSlices {
                slices: vec![AudioSlice::at(0.0)],
                tune_cents: c.stretch.map_or(0.0, |s| s.cents()),
            });
            let starts = &mut entry.slices;
            let k = match starts.iter().position(|b| b.same(&at)) {
                Some(k) => k,
                None if starts.len() < 68 => {
                    starts.push(at);
                    starts.len() - 1
                }
                None => 0,
            };
            let len = scale(it.length).max(1);
            let pattern = *audio_patterns.entry((inst, k)).or_insert_with(|| {
                let mut p = Pattern::new(next_id, &c.name);
                p.color = c.color;
                p.notes.push(Note::new(inst, 0, len, 60 + k as u8, 127));
                song.patterns.push(p.into());
                next_id += 1;
                next_id - 1
            });
            if let Some(p) = song.pattern_mut(pattern) {
                let n = &mut p.notes[0];
                n.length = n.length.max(len);
                p.length = p.length.max(len);
            }
            song.clips.push(Clip {
                pattern,
                start: scale(it.position),
                track: it.track,
                offset: 0,
                length: Some(len),
                muted: it.muted,
            });
            report.audio_imported += 1;
        } else {
            report.audio_clips += 1;
        }
    }
    if !tempo_clips.is_empty() || !tempo_events.is_empty() {
        song.tempo_map = tempo_map(song.tempo, &mut tempo_clips, tempo_events);
        report.tempo_points = song.tempo_map.points.len();
    }
    report.audio_slices = slices
        .into_iter()
        .filter(|(_, s)| s.slices.len() > 1 || s.tune_cents != 0.0)
        .collect();
    report.clips = song.clips.len();
    let last = song.clips.iter().map(|c| c.track).max();
    if let Some(last) = last {
        song.tracks = (0..=last)
            .map(|t| tracks.get(&(t + 1)).cloned().unwrap_or_default())
            .collect();
    }
    if song.patterns.is_empty() {
        song.patterns.push(Pattern::new(1, "Pattern 1").into());
    }
    fit_limits(&mut song);
    song.ensure_note_ids();
    report.channels = channels;
    report.inserts = inserts;
    Ok((song, report))
}

/// FL's inserts become the mixer: names, faders, pans, routing, effects. The
/// mixer grows to the last insert in use (FL has 125; KeyFrameXL keeps 16
/// unless more are needed). Channels play into the insert FL gave them.
type MixerSlotMap = BTreeMap<(u32, u32), (usize, f32)>;

fn build_mixer(
    song: &mut Song,
    channels: &[FlpChannel],
    inserts: &[FlpInsert],
    automated: u32,
    eq_automated: &[u32],
    report: &mut FlpReport,
) -> MixerSlotMap {
    // (insert, FL slot) → our slot index and its internal wet level.
    let mut placed = BTreeMap::new();
    if inserts.is_empty() {
        return placed;
    }
    let used = |i: &FlpInsert| {
        !i.name.is_empty()
            || !i.slots.is_empty()
            || i.volume.is_some_and(|v| v != 12800)
            || i.pan.is_some_and(|p| p != 0)
            || i.routes.iter().skip(1).any(|r| *r != 0)
    };
    let last_insert = inserts
        .iter()
        .filter(|i| i.index > 0 && i.index < 126 && used(i))
        .map(|i| i.index)
        .max()
        .unwrap_or(0);
    let last_channel = channels
        .iter()
        .filter_map(|c| c.insert)
        .filter(|i| *i < 126)
        .max()
        .unwrap_or(0);
    // Inserts that only receive a send count too.
    let last_target = inserts
        .iter()
        .filter(|i| used(i))
        .flat_map(|i| {
            i.routes
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, r)| **r != 0)
                .map(|(t, _)| t as u32)
        })
        .filter(|t| *t < 126)
        .max()
        .unwrap_or(0);
    let len = (last_insert
        .max(last_channel)
        .max(last_target)
        .max(automated.min(125)) as usize
        + 1)
    .clamp(crate::sequence::INSERTS + 1, 126);
    song.mixer = crate::sequence::default_mixer();
    while song.mixer.len() < len {
        let n = song.mixer.len();
        song.mixer.push(MixerTrack::new(&format!("Insert {n}")));
    }
    for ins in inserts.iter().filter(|i| (i.index as usize) < len) {
        let t = &mut song.mixer[ins.index as usize];
        if !ins.name.is_empty() {
            t.name = ins.name.clone();
        }
        if let Some(v) = ins.volume {
            t.volume = fl_fader(v as f32);
        }
        if let Some(p) = ins.pan {
            t.pan = (p as f32 / 6400.0).clamp(-1.0, 1.0);
        }
        t.muted = !ins.enabled;
        t.fl_slots = ins
            .slots
            .iter()
            .map(|s| FlMixerSlot {
                index: s.slot,
                enabled: s.enabled,
                mix: s.mix,
                plugin: FlPluginState::new(&s.plugin, &s.data),
            })
            .collect();
        for s in &ins.slots {
            if s.slot as usize >= FX_SLOTS || t.fx.len() >= FX_SLOTS {
                continue;
            }
            let mut after = Vec::new();
            let fx = match (&s.vst, native_effect(&s.plugin)) {
                (Some(v), _) => {
                    // Resolved against the scanned plugins by the program (it
                    // knows them); the Windows path marks it as not yet found.
                    let mut plugin = PluginRef {
                        id: mixer_key(ins.index, t.fx.len()),
                        format: if v.vst3 { "vst3" } else { "vst2" }.into(),
                        path: v.path.clone(),
                        name: v.name.clone(),
                        vendor: v.vendor.clone(),
                        state: String::new(),
                        outs: vec![],
                    };
                    plugin.set_state(&v.state);
                    let mut fx = FxSlot::plugin(plugin);
                    fx.enabled = s.enabled && !ins.bypass_effects;
                    fx
                }
                (None, Some(_)) => {
                    let Some((mut fx, converted, extra)) =
                        crate::flp_fx::stand_in(&s.plugin, &s.data, &mut report.effect_notes)
                    else {
                        continue;
                    };
                    after = extra;
                    // Settings carried over: plays as in FL; otherwise off,
                    // our defaults would colour the sound.
                    if converted {
                        report.converted_effects += 1;
                        fx.enabled = s.enabled && !ins.bypass_effects;
                    } else {
                        report.stand_in_effects += 1;
                        report.stand_in_names.push(s.plugin.clone());
                        fx.enabled = false;
                    }
                    fx
                }
                (None, None) => {
                    report.skipped_effects.push(s.plugin.clone());
                    continue;
                }
            };
            placed.insert((ins.index, s.slot), (t.fx.len(), fx.mix));
            let on = fx.enabled;
            t.fx.push(FxSlot {
                mix: s.mix * fx.mix,
                ..fx
            });
            for f in after.drain(..) {
                if t.fx.len() >= FX_SLOTS {
                    report.effect_notes.push(format!(
                        "{}: no free slot for {}'s pass filter",
                        t.name, s.plugin
                    ));
                    break;
                }
                t.fx.push(FxSlot {
                    enabled: on,
                    mix: s.mix,
                    ..f
                });
            }
        }
        // FL's insert EQ (after the effects, before the fader): our
        // three-band EQ, when FL's has any gain.
        let eq = ins
            .eq
            .unwrap_or([0, 0, 0, 5777, 33145, 55825, 17500, 17500, 17500]);
        let default_freqs = eq[3..6] == [5777, 33145, 55825];
        if let Some(sep) = ins.sep {
            // FL: fully right (+64) = mono; ours: -1 = mono.
            t.sep = (-(sep as f32) / 64.0).clamp(-1.0, 1.0);
        }
        if default_freqs && !eq_automated.contains(&ins.index) {
            // FL's strip EQ at its usual bands: the strip EQ here.
            for b in 0..3 {
                t.eq[b] = (eq[b] as f32 / 100.0)
                    .clamp(-crate::sequence::EQ_RANGE, crate::sequence::EQ_RANGE);
            }
            if eq[..3].iter().any(|g| *g != 0) {
                report.insert_eqs += 1;
            }
        } else if eq[..3].iter().any(|g| *g != 0) || eq_automated.contains(&ins.index) {
            if t.fx.len() < FX_SLOTS {
                placed.insert((ins.index, INSERT_EQ_SLOT), (t.fx.len(), 1.0));
                t.fx.push(insert_eq(&eq));
                report.insert_eqs += 1;
            } else {
                report
                    .effect_notes
                    .push(format!("{}: no free slot for its insert EQ", t.name));
            }
        }
    }
    // Routing after every insert exists: master, then sends (no loops).
    for ins in inserts
        .iter()
        .filter(|i| i.index > 0 && (i.index as usize) < len && i.routes.len() > 1)
    {
        let i = ins.index as usize;
        song.mixer[i].to_master = ins.routes[0] != 0;
        song.mixer[i].sends.clear();
        for (to, _) in ins
            .routes
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(to, r)| **r != 0 && *to < len && *to != i)
        {
            if crate::sequence::route_would_loop(&song.mixer, i, to) {
                continue;
            }
            let level = ins
                .levels
                .get(&(to as u32))
                .map_or(1.0, |l| (*l as f32 / 12800.0).clamp(0.0, 1.0));
            song.mixer[i].sends.push(Send {
                to: to as u16,
                level,
            });
        }
    }
    for c in channels {
        if let (Some(id), Some(ins)) = (c.instrument, c.insert)
            && (ins as usize) < len
            && let Some(inst) = song.instruments.iter_mut().find(|i| i.id == id)
        {
            inst.mixer = ins as u16;
        }
    }
    placed
}

/// An imported audio clip channel's sample, cut at the clip start points
/// (from `FlpReport::audio_slices`): key 60 plays it from the beginning, key
/// 60 + k from start point k, all at the channel's stretch pitch.
pub fn slice_preset(
    mut preset: crate::sequence::SamplePreset,
    slices: &AudioSlices,
) -> crate::sequence::SamplePreset {
    let whole = [AudioSlice::at(0.0)];
    let cuts: &[AudioSlice] = if slices.slices.is_empty() {
        &whole
    } else {
        &slices.slices
    };
    let Some(first) = preset.zones.first().cloned() else {
        return preset;
    };
    if preset.zones.len() != 1 {
        return preset;
    }
    preset.zones.clear();
    for (k, cut) in cuts.iter().enumerate().take(68) {
        let mut z = first.clone();
        let key = 60 + k as u8;
        (z.root, z.low, z.high) = (key, key, key);
        let frames = |s: f64| (s * z.sample_rate as f64) as usize;
        z.offset = frames(cut.start).min(z.end.saturating_sub(1));
        if let Some(end) = cut.end {
            z.end = frames(end).clamp(z.offset + 1, z.end);
            z.loop_end = z.loop_end.min(z.end);
        }
        z.loop_start = z.loop_start.max(z.offset).min(z.loop_end);
        z.fade_in = frames(cut.fade_in);
        z.fade_out = frames(cut.fade_out);
        z.tune_cents += slices.tune_cents;
        preset.zones.push(z);
    }
    preset
}

/// Keep what FL allows inside KeyFrameXL's limits (`Song::validate`): pattern
/// lengths on the step grid and at most `MAX_BARS`, notes inside their
/// pattern, clips inside theirs. Automation patterns grow to their furthest
/// clip (FL holds the last value).
fn fit_limits(song: &mut Song) {
    use crate::sequence::{MAX_BARS, STEP};
    let max = MAX_BARS * BAR;
    song.clips.retain(|c| c.start >= 0 && c.start < max);
    for c in &mut song.clips {
        c.length = c.length.map(|l| l.clamp(1, max));
        if let Some(p) = song.patterns.iter_mut().find(|p| p.id == c.pattern)
            && p.automation.is_some()
        {
            let p = std::sync::Arc::make_mut(p);
            p.length = p.length.max(c.offset + c.length.unwrap_or(1)).min(max);
        }
    }
    for p in song.patterns.iter_mut().map(std::sync::Arc::make_mut) {
        p.length = ((p.length.max(1) + STEP - 1) / STEP * STEP).min(max);
        let len = p.length;
        p.notes.retain(|n| n.start >= 0 && n.start < len);
        for n in &mut p.notes {
            n.length = n.length.clamp(1, len - n.start);
            n.velocity = n.velocity.clamp(1, 127);
            n.pan = n.pan.clamp(-1.0, 1.0);
            n.release = n.release.clamp(0.0, 1.0);
            n.fine = n.fine.clamp(-100.0, 100.0);
            n.mod_x = n.mod_x.clamp(0.0, 1.0);
            n.mod_y = n.mod_y.clamp(0.0, 1.0);
        }
        p.slides.retain(|s| {
            s.start >= 0 && s.length > 0 && s.start + s.length <= len && s.target <= 127
        });
        if let Some(env) = &mut p.automation {
            env.points.retain(|q| q.tick <= len);
        }
    }
    let lens: BTreeMap<u64, Tick> = song.patterns.iter().map(|p| (p.id, p.length)).collect();
    for c in &mut song.clips {
        if let Some(&len) = lens.get(&c.pattern)
            && c.offset >= len
        {
            c.offset %= len;
        }
    }
}

/// `placed` key of an insert's own EQ.
const INSERT_EQ_SLOT: u32 = 1000;

/// FL's mixer EQ frequency (0..65536) in Hz: 10 · 1600^√x (DawVert);
/// FL's defaults land on 90 Hz, 1.9 kHz and 9.1 kHz.
fn insert_eq_freq(raw: f32) -> f32 {
    10.0 * 1600f32.powf((raw / 65536.0).clamp(0.0, 1.0).sqrt())
}

/// Our EQ for FL's insert EQ (raw values as `FlpInsert::eq`). FL's band
/// width has no known scale: the mid band's Q follows it around 1 at FL's
/// default.
fn insert_eq(eq: &[i32; 9]) -> FxSlot {
    let mut fx = FxSlot::new(FxKind::Eq);
    let spec = FxKind::Eq.params();
    let mut set = |i: usize, v: f32| fx.params[i] = v.clamp(spec[i].min, spec[i].max);
    set(0, insert_eq_freq(eq[3] as f32));
    set(1, eq[0] as f32 / 100.0);
    set(2, insert_eq_freq(eq[4] as f32));
    set(3, eq[1] as f32 / 100.0);
    set(4, 17500.0 / eq[7].max(1) as f32);
    set(5, insert_eq_freq(eq[5] as f32));
    set(6, eq[2] as f32 / 100.0);
    fx
}

/// Plugin ids the import gives hosted plugins before the program has found
/// them; `resolve_plugin_keys` swaps in the real ids.
pub const PLUGIN_KEY: u64 = 1 << 62;

/// The placeholder id of a VST effect: insert and its place in our list.
pub fn mixer_key(insert: u32, index: usize) -> u64 {
    PLUGIN_KEY | 1 << 40 | (insert as u64) << 8 | index as u64
}

/// The placeholder id of a VST channel's plugin (FL channel index).
pub fn channel_key(channel: u32) -> u64 {
    PLUGIN_KEY | channel as u64
}

/// Point automation of hosted plugins at their real ids (`ids`: placeholder →
/// id). Automation of plugins that were not found is removed with its clips;
/// the names of those clips come back.
pub fn resolve_plugin_keys(song: &mut Song, ids: &BTreeMap<u64, u64>) -> Vec<String> {
    use crate::automation::Target;
    let mut gone = Vec::new();
    for p in song
        .patterns
        .iter_mut()
        .filter(|p| p.automation.is_some())
        .map(std::sync::Arc::make_mut)
    {
        if let Some(env) = &mut p.automation
            && let Target::PluginParam { plugin, .. } = &mut env.target
            && *plugin & PLUGIN_KEY != 0
        {
            match ids.get(plugin) {
                Some(id) => *plugin = *id,
                None => gone.push((p.id, p.name.clone())),
            }
        }
    }
    song.patterns
        .retain(|p| !gone.iter().any(|(id, _)| *id == p.id));
    song.clips
        .retain(|c| !gone.iter().any(|(id, _)| *id == c.pattern));
    gone.into_iter().map(|(_, n)| n).collect()
}

/// Automation points (event 234): a 21-byte header with the count at 17,
/// then 24-byte points: f64 beats since the previous point, f64 value 0..1,
/// f32 tension.
fn auto_points(d: &[u8]) -> Vec<(f64, f32, f32)> {
    let n = le32(d, 17) as usize;
    if d.len() < 21 + n * 24 {
        return vec![];
    }
    d[21..21 + n * 24]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|r| {
            let f64_at = |i: usize| f64::from_le_bytes(r[i..i + 8].try_into().expect("8 bytes"));
            let tension = f32::from_le_bytes(r[16..20].try_into().expect("4 bytes"));
            (
                f64_at(0),
                f64_at(8) as f32,
                if tension.is_finite() { tension } else { 0.0 },
            )
        })
        .collect()
}

/// A pattern automation event's raw value as the 0..1 an automation clip
/// would hold for that control; `None` where the scale is unknown.
fn event_scale(param: u16, dest: u16) -> Option<fn(i32) -> f32> {
    Some(if dest & 0x2000 != 0 && param & 0xff00 == 0x1f00 {
        match param & 0xff {
            // The slot switch is 0 or 1, not a 0..12800 level.
            0 => |v| v.clamp(0, 1) as f32,
            192 => |v| v as f32 / 16000.0,
            193 => |v| (v as f32 + 6400.0) / 12800.0,
            208..=210 => |v| (v as f32 + 1800.0) / 3600.0,
            216..=218 => |v| v as f32 / 65536.0,
            _ => |v| v as f32 / 12800.0,
        }
    } else if dest & 0x6000 == 0 && param <= 1 {
        |v| v as f32 / 12800.0
    } else {
        return None;
    })
}

/// FL's mixer fader (raw 0..16000, 12800 = 0 dB) as our gain.
fn fl_fader(raw: f32) -> f32 {
    let k = crate::sequence::FADER_MAX.ln() / 1.25f32.ln();
    (raw.max(0.0) / 12800.0)
        .powf(k)
        .min(crate::sequence::FADER_MAX)
}

/// FL's automation clips: one pattern per controlled target (an FL clip that
/// drives two controls becomes two), with the clip's points. Returns FL
/// channel → the patterns made from it.
fn build_automation(
    song: &mut Song,
    channels: &[FlpChannel],
    inserts: &[FlpInsert],
    links: &[(u32, u16, u16)],
    placed: &MixerSlotMap,
    report: &mut FlpReport,
) -> (BTreeMap<u32, Vec<u64>>, BTreeMap<u32, Vec<(Tick, f64)>>) {
    use crate::automation::{Envelope, Playback, Point, Target};
    let mut made: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    // Tempo automation: channel → (tick in the clip, BPM), placed by its clips.
    let mut tempo: BTreeMap<u32, Vec<(Tick, f64)>> = BTreeMap::new();
    let mut next = song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    for c in channels
        .iter()
        .filter(|c| c.kind == ChannelKind::Automation)
    {
        let mine: Vec<_> = links.iter().filter(|l| l.0 == c.index).collect();
        if c.points.is_empty() || mine.is_empty() {
            continue;
        }
        for &&(_, param, dest) in &mine {
            // The project tempo (0..1 = FL's default automation range,
            // 60..180 BPM): it becomes the song's tempo track.
            if dest == 0x4000 && param == 5 {
                let mut beats = 0.0;
                let pts = c
                    .points
                    .iter()
                    .map(|&(delta, v, _)| {
                        beats += delta.max(0.0);
                        (
                            (beats * PPQ as f64).round() as Tick,
                            60.0 + v as f64 * 120.0,
                        )
                    })
                    .collect();
                tempo.insert(c.index, pts);
                continue;
            }
            // Destination: 0x2000 = mixer (insert << 6 | slot), 0x4000 alone =
            // the project (tempo, main volume), else a channel. Parameter:
            // 0x8000 | n = plugin parameter n; 0x1F00 | p = mixer parameter p
            // (as in event 225: 192 volume, 193 pan); else a channel
            // parameter (0 volume, 1 pan, ...).
            let why = |w: &str| format!("{} — {w}", c.name);
            let target: Result<(Target, Box<dyn Fn(f32) -> f32>), String> = if dest & 0x2000 != 0 {
                // 0x1000 and 0x4000 are flags here (seen on FL 2024+ links).
                let insert = ((dest & 0x0fff) >> 6) as u32;
                let slot = (dest & 0x3f) as u32;
                let track = insert as u16;
                if (insert as usize) >= song.mixer.len() {
                    Err(why("insert out of range"))
                } else if param & 0x8000 != 0 {
                    let n = param & 0x7fff;
                    let fx = placed
                        .get(&(insert, slot))
                        .and_then(|&(k, _)| song.mixer[insert as usize].fx.get(k).map(|f| (k, f)));
                    let fl = inserts
                        .get(insert as usize)
                        .and_then(|i| i.slots.iter().find(|s| s.slot == slot));
                    match (fx, fl) {
                        (Some((_, f)), _) if f.plugin.is_some() => {
                            let id = f.plugin.as_ref().map(|p| p.id).unwrap_or(0);
                            Ok((
                                Target::PluginParam {
                                    plugin: id,
                                    param: n as u32,
                                },
                                Box::new(|u| u),
                            ))
                        }
                        // Balance / PanOMatic volume (0..320, 256 = 0 dB): the
                        // insert's fader, from where it stands.
                        (Some(_), Some(fl))
                            if n == 1
                                && matches!(
                                    fl.plugin.as_str(),
                                    "Fruity Balance" | "Fruity PanOMatic"
                                ) =>
                        {
                            let fader = song.mixer[insert as usize].volume;
                            Ok((
                                Target::MixerVolume { track },
                                Box::new(move |u: f32| {
                                    (u * 320.0 / 256.0 * fader).min(crate::sequence::FADER_MAX)
                                }),
                            ))
                        }
                        (Some((k, _)), Some(fl))
                            if let Some((extra, conv)) =
                                crate::flp_fx::pass_band(&fl.plugin, &fl.data, n) =>
                        {
                            let at = k + 1 + extra;
                            if song.mixer[insert as usize]
                                .fx
                                .get(at)
                                .is_some_and(|f| f.kind == FxKind::Filter)
                            {
                                Ok((
                                    Target::FxParam {
                                        track,
                                        slot: at as u8,
                                        param: 1,
                                    },
                                    Box::new(conv),
                                ))
                            } else {
                                Err(why("its pass filter found no free slot"))
                            }
                        }
                        (Some((k, _)), Some(fl)) => {
                            match crate::flp_fx::automation(&fl.plugin, n) {
                                Some((ours, conv)) => Ok((
                                    Target::FxParam {
                                        track,
                                        slot: k as u8,
                                        param: ours as u16,
                                    },
                                    Box::new(conv),
                                )),
                                _ => Err(why(&format!(
                                    "{} parameter {n} has no counterpart in the stand-in",
                                    fl.plugin
                                ))),
                            }
                        }
                        (None, Some(fl)) => {
                            Err(why(&format!("its effect {} was not imported", fl.plugin)))
                        }
                        _ => Err(why("its effect slot is empty")),
                    }
                } else if param & 0xff00 == 0x1f00 {
                    match param & 0xff {
                        192 => Ok((
                            Target::MixerVolume { track },
                            Box::new(|u| fl_fader(u * 16000.0)),
                        )),
                        193 => Ok((Target::MixerPan { track }, Box::new(|u| u * 2.0 - 1.0))),
                        1 => match placed.get(&(insert, slot)) {
                            Some(&(k, wet)) => Ok((
                                Target::FxMix {
                                    track,
                                    slot: k as u8,
                                },
                                Box::new(move |u| u * wet),
                            )),
                            None => Err(why("its effect slot was not imported")),
                        },
                        // The insert EQ's gains and frequencies.
                        p @ (208..=210 | 216..=218) => {
                            match placed.get(&(insert, INSERT_EQ_SLOT)) {
                                Some(&(k, _)) => {
                                    let band = (p as usize - 208) % 8;
                                    let gain = p < 216;
                                    let ours = [[1, 3, 6], [0, 2, 5]][!gain as usize][band];
                                    let spec = FxKind::Eq.params()[ours];
                                    let conv: Box<dyn Fn(f32) -> f32> = if gain {
                                        Box::new(move |u| {
                                            ((u * 3600.0 - 1800.0) / 100.0)
                                                .clamp(spec.min, spec.max)
                                        })
                                    } else {
                                        Box::new(move |u| {
                                            insert_eq_freq(u * 65536.0).clamp(spec.min, spec.max)
                                        })
                                    };
                                    Ok((
                                        Target::FxParam {
                                            track,
                                            slot: k as u8,
                                            param: ours as u16,
                                        },
                                        conv,
                                    ))
                                }
                                None => Err(why("the insert EQ is flat here (no slot for it)")),
                            }
                        }
                        0 => match placed.get(&(insert, slot)) {
                            Some(&(k, _)) => Ok((
                                Target::FxEnabled {
                                    track,
                                    slot: k as u8,
                                },
                                Box::new(|u| u),
                            )),
                            None => Err(why("its effect slot was not imported")),
                        },
                        // Send levels to insert p - 64.
                        p @ 64..=191 => {
                            let to = p - 64;
                            if song
                                .mixer
                                .get(track as usize)
                                .is_some_and(|m| m.sends.iter().any(|s| s.to == to))
                            {
                                Ok((Target::SendLevel { track, to }, Box::new(|u| u)))
                            } else if to == 0 {
                                Err(why("the level into Master is not automatable here"))
                            } else {
                                Err(why("the send it moves is not routed here"))
                            }
                        }
                        _ => Err(why("this mixer control is not automatable here")),
                    }
                } else {
                    Err(why("unknown mixer control"))
                }
            } else if dest == 0x4000 && param == 0 {
                // FL's main volume (0.8 = 0 dB, the fader's scale) on top of
                // the Master fader as it stands.
                let master = song.mixer.first().map_or(1.0, |m| m.volume);
                Ok((
                    Target::MixerVolume { track: 0 },
                    Box::new(move |u: f32| {
                        (fl_fader(u * 16000.0) * master).min(crate::sequence::FADER_MAX)
                    }),
                ))
            } else if dest & 0x4000 != 0 {
                Err(why(
                    "this project control (main pitch, ...) is not automatable here",
                ))
            } else {
                let ch = channels.iter().find(|x| x.index == dest as u32);
                match ch.and_then(|x| x.instrument.map(|i| (x, i))) {
                    None => Err(why("its channel was not imported")),
                    Some((x, inst)) => {
                        if param & 0x8000 != 0 {
                            if x.vst.is_some() {
                                Ok((
                                    Target::PluginParam {
                                        plugin: channel_key(x.index),
                                        param: (param & 0x7fff) as u32,
                                    },
                                    Box::new(|u| u),
                                ))
                            } else {
                                Err(why(&format!(
                                    "{} parameters are not automatable from here",
                                    x.plugin
                                )))
                            }
                        } else {
                            match param {
                                0 => Ok((
                                    Target::ChannelVolume { inst },
                                    Box::new(|u| {
                                        (u * 12800.0 / 10000.0)
                                            .powi(2)
                                            .min(crate::sequence::FADER_MAX)
                                    }),
                                )),
                                1 => Ok((Target::ChannelPan { inst }, Box::new(|u| u * 2.0 - 1.0))),
                                _ => Err(why(
                                    "this channel control (filter, pitch, ...) is not automatable here",
                                )),
                            }
                        }
                    }
                }
            };
            let (target, conv) = match target {
                Ok(t) => t,
                Err(e) => {
                    report.automation_notes.push(e);
                    continue;
                }
            };
            // Positions add up; FL's tension bends the curve into a point,
            // ours the curve out of one.
            let mut beats = 0.0;
            let mut points: Vec<Point> = Vec::with_capacity(c.points.len());
            for &(delta, v, tension) in &c.points {
                beats += delta.max(0.0);
                if let Some(prev) = points.last_mut() {
                    prev.tension = tension.clamp(-1.0, 1.0);
                }
                points.push(Point {
                    tick: (beats * PPQ as f64).round() as Tick,
                    value: conv(v.clamp(0.0, 1.0)),
                    tension: 0.0,
                });
            }
            let length = points.last().map_or(1, |p| p.tick).max(1);
            let mut p = Pattern::new(next, &c.name);
            p.color = c.color;
            p.length = length;
            let playback = if c.index >= 0x1_0000 {
                Playback::Events
            } else {
                Playback::Hold
            };
            p.automation = Some(Envelope {
                target,
                points,
                playback,
            });
            song.patterns.push(p.into());
            made.entry(c.index).or_default().push(next);
            report.automations += 1;
            next += 1;
        }
    }
    (made, tempo)
}

/// The song's tempo track from FL's tempo automation clips: each clip's points
/// in playlist time, (clip start, clip-local offset, length, points). The
/// tempo holds between clips (FL keeps the last value) and starts at `base`.
fn tempo_map(
    base: f64,
    clips: &mut [(Tick, Tick, Tick, &[(Tick, f64)])],
    events: Vec<(Tick, f64)>,
) -> crate::tempo::TempoMap {
    use crate::tempo::{TEMPO_MAX, TEMPO_MIN, TempoMap, TempoPoint, TempoShape};
    clips.sort_by_key(|c| c.0);
    // (tick, bpm, the curve keeps going to the next point)
    let mut raw: Vec<(Tick, f64, bool)> = vec![(0, base, false)];
    let value = |pts: &[(Tick, f64)], t: Tick| -> f64 {
        let i = pts.partition_point(|p| p.0 <= t);
        match (i.checked_sub(1).map(|i| pts[i]), pts.get(i)) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                a.1 + (b.1 - a.1) * (t - a.0) as f64 / (b.0 - a.0) as f64
            }
            (Some(a), _) => a.1,
            (None, Some(b)) => b.1,
            (None, None) => base,
        }
    };
    for &mut (start, offset, len, pts) in clips.iter_mut() {
        let end = offset + len;
        raw.push((start, value(pts, offset), true));
        for &(t, bpm) in pts.iter().filter(|p| p.0 > offset && p.0 < end) {
            raw.push((start + t - offset, bpm, true));
        }
        // The clip's last value holds after it.
        if let Some(last) = raw.last_mut() {
            last.2 = false;
        }
    }
    raw.extend(events.into_iter().map(|(tick, bpm)| (tick, bpm, false)));
    raw.sort_by_key(|p| p.0);
    let mut map = TempoMap::default();
    for (tick, bpm, ramp) in raw {
        let bpm = bpm.clamp(TEMPO_MIN, TEMPO_MAX);
        let shape = if ramp {
            TempoShape::Linear
        } else {
            TempoShape::Step
        };
        match map.points.last_mut() {
            // Two points on one tick: FL's jump. The later value wins; the
            // segment before it holds if it was flat.
            Some(p) if p.tick >= tick => {
                p.bpm = bpm;
                p.shape = shape;
            }
            _ => map.points.push(TempoPoint { tick, bpm, shape }),
        }
    }
    // A ramp into a jump: the curve before it is flat or close enough.
    for i in 1..map.points.len() {
        if map.points[i - 1].bpm == map.points[i].bpm {
            map.points[i - 1].shape = TempoShape::Step;
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev_num(out: &mut Vec<u8>, id: u8, v: u32) {
        out.push(id);
        match id {
            0..=63 => out.push(v as u8),
            64..=127 => out.extend_from_slice(&(v as u16).to_le_bytes()),
            _ => out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn ev_data(out: &mut Vec<u8>, id: u8, d: &[u8]) {
        out.push(id);
        let mut n = d.len();
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            out.push(if n > 0 { b | 0x80 } else { b });
            if n == 0 {
                break;
            }
        }
        out.extend_from_slice(d);
    }
    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }
    fn note(pos: u32, ch: u16, len: u32, key: u16, vel: u8) -> Vec<u8> {
        let mut r = vec![0u8; 24];
        r[0..4].copy_from_slice(&pos.to_le_bytes());
        r[6..8].copy_from_slice(&ch.to_le_bytes());
        r[8..12].copy_from_slice(&len.to_le_bytes());
        r[12..14].copy_from_slice(&key.to_le_bytes());
        r[16] = 120;
        r[18] = 64;
        r[20] = 64;
        r[21] = vel;
        r[22] = 128;
        r[23] = 128;
        r
    }

    #[test]
    fn native_host_keeps_generator_state_pitch_and_separate_outputs() {
        let mut data = Vec::new();
        for (index, plugin, state) in [
            (0, "FLEX", b"preset A"),
            (1, "Synth", b"preset B"),
            (2, "Sakura", b"preset C"),
        ] {
            ev_num(&mut data, CH_NEW, index);
            ev_num(&mut data, CH_TYPE, 2);
            ev_num(&mut data, CH_ENABLED, 0);
            ev_num(&mut data, CH_INSERT, 9);
            ev_data(&mut data, PLUGIN_INTERNAL, &utf16(plugin));
            ev_data(&mut data, PLUGIN_DATA, state);
            let mut levels = [0; 12];
            levels[8..].copy_from_slice(&30i32.to_le_bytes());
            ev_data(&mut data, CH_LEVELS, &levels);
        }
        ev_num(&mut data, PAT_NEW, 1);
        ev_data(&mut data, PAT_NOTES, &note(0, 0, 96, 60, 100));
        ev_num(&mut data, ARR_NEW, 0);
        ev_data(&mut data, PLAYLIST, &item(0, 1, 96, 0));
        for _ in 0..3 {
            ev_data(&mut data, INSERT_FLAGS, &[0; 12]);
            ev_data(&mut data, INSERT_ROUTING, &[1]);
            ev_num(&mut data, INSERT_OUTPUT, u32::MAX);
        }
        let mut input = b"FLhd\x06\0\0\0\0\0\x03\0\x60\0FLdt".to_vec();
        input.extend_from_slice(&(data.len() as u32).to_le_bytes());
        input.extend(data);
        let (hosted, midi) = native_host_flp(&input, &[0, 2]).unwrap();
        let (_, report) = read_flp(&hosted).unwrap();
        assert_eq!(midi, vec![1, 3]);
        assert_eq!(report.channels.len(), 4);
        assert!(!report.channels[0].enabled);
        assert_eq!(report.channels[0].data, b"preset A");
        for (index, insert, data) in [(1, 1, b"preset A"), (3, 2, b"preset C")] {
            let channel = &report.channels[index];
            assert!(channel.enabled);
            assert_eq!(channel.insert, Some(insert));
            assert_eq!(channel.data, data);
            assert_eq!(channel.pitch, 30);
        }
        assert!(!report.channels[2].enabled);
        assert!(report.channels[2].data.is_empty());
        let (_, evs) = events(&hosted).unwrap();
        assert!(
            !evs.iter()
                .any(|(id, _)| matches!(*id, PAT_NOTES | PLAYLIST))
        );
        assert_eq!(
            evs.into_iter()
                .filter_map(|(id, v)| match (id, v) {
                    (INSERT_OUTPUT, Value::Num(n)) => Some(n),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![u32::MAX, 1, 2],
        );
    }
    fn item(pos: u32, pattern: u16, len: u32, track: u16) -> Vec<u8> {
        let mut r = vec![0u8; 80];
        r[0..4].copy_from_slice(&pos.to_le_bytes());
        r[4..6].copy_from_slice(&20480u16.to_le_bytes());
        r[6..8].copy_from_slice(&(20480 + pattern).to_le_bytes());
        r[8..12].copy_from_slice(&len.to_le_bytes());
        r[12..14].copy_from_slice(&(499 - track).to_le_bytes());
        r[24..28].copy_from_slice(&(-1i32).to_le_bytes());
        r
    }

    /// A small FL 2025-style project: two channels, one pattern, two clips.
    fn sample_flp() -> Vec<u8> {
        let mut d = Vec::new();
        ev_data(&mut d, FL_VERSION, b"25.1.6.4997\0");
        ev_num(&mut d, TEMPO, 140_000);
        ev_num(&mut d, METER_NUM, 3);
        ev_num(&mut d, METER_DEN, 4);
        ev_num(&mut d, CH_NEW, 0);
        ev_num(&mut d, CH_TYPE, 5);
        ev_data(&mut d, PLUGIN_NAME, &utf16("Automation Clip"));
        ev_num(&mut d, CH_NEW, 1);
        ev_num(&mut d, CH_TYPE, 2);
        ev_data(&mut d, PLUGIN_INTERNAL, &utf16("FLEX"));
        ev_data(&mut d, PLUGIN_NAME, &utf16("Lead"));
        let levels: Vec<u8> = [9600u32, 10000, 30, 256, 0, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        ev_data(&mut d, CH_LEVELS, &levels);
        ev_num(&mut d, PAT_NEW, 1);
        ev_data(&mut d, PAT_NAME, &utf16("Verse"));
        ev_num(&mut d, PAT_COLOR, 0x0000_33ff);
        let mut notes = note(0, 1, 96, 60, 100);
        let mut fine = note(192, 1, 48, 64, 128);
        fine[16] = 133;
        notes.extend(fine);
        notes.extend(note(96, 0, 24, 60, 100));
        // A slide note on the lead, under the second bar of its C note.
        let mut slide = note(48, 1, 24, 67, 100);
        slide[4..6].copy_from_slice(&0x0008u16.to_le_bytes());
        notes.extend(slide);
        ev_data(&mut d, PAT_NOTES, &notes);
        ev_num(&mut d, ARR_NEW, 0);
        let mut pl = item(0, 1, 384, 0);
        pl.extend(item(384, 1, 384, 2));
        ev_data(&mut d, PLAYLIST, &pl);
        let mut t = vec![0u8; 70];
        t[0..4].copy_from_slice(&3u32.to_le_bytes());
        t[4..8].copy_from_slice(&0x0011_2233u32.to_le_bytes());
        t[12] = 1;
        ev_data(&mut d, TRACK_DATA, &t);
        ev_data(&mut d, TRACK_NAME, &utf16("Drums"));
        let mut out = b"FLhd".to_vec();
        out.extend_from_slice(&6u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&96u16.to_le_bytes());
        out.extend_from_slice(b"FLdt");
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend(d);
        out
    }

    #[test]
    fn reads_tempo_channels_patterns_notes_and_playlist() {
        let (song, report) = read_flp(&sample_flp()).unwrap();
        assert_eq!(song.tempo, 140.0);
        assert_eq!(report.meter, (3, 4));
        assert_eq!(report.fl_version, "25.1.6.4997");
        // The automation channel is not an instrument; its note is left out.
        assert_eq!(song.instruments.len(), 1);
        assert_eq!(song.instruments[0].name, "Lead");
        assert!((song.instruments[0].pan - 0.5).abs() < 1e-6);
        assert!((song.instruments[0].volume - 1.0).abs() < 1e-6);
        assert_eq!(report.dropped_notes, 1);
        let p = &song.patterns[0];
        assert_eq!(
            (p.id, p.name.as_str(), p.color),
            (1, "Verse", Some(0xff3300))
        );
        // 96 PPQ → 480: a quarter note at 0, an eighth at beat 3.
        assert_eq!(p.notes.len(), 2);
        assert_eq!(p.slides.len(), 1);
        assert_eq!(
            (
                p.slides[0].start,
                p.slides[0].length,
                p.slides[0].target,
                p.slides[0].instrument
            ),
            (PPQ / 2, PPQ / 4, 67, 1)
        );
        assert_eq!((p.slides[0].group, p.notes[0].group), (0, 0));
        assert_eq!(report.slides, 1);
        assert_eq!(
            (
                p.notes[0].start,
                p.notes[0].length,
                p.notes[0].pitch,
                p.notes[0].velocity
            ),
            (0, PPQ, 60, 100)
        );
        assert_eq!(
            (p.notes[1].start, p.notes[1].length, p.notes[1].velocity),
            (2 * PPQ, PPQ / 2, 127)
        );
        // Standalone FL applies the channel's +30 cent knob.
        assert_eq!(report.channels[1].pitch, 30);
        assert_eq!((p.notes[0].pitch, p.notes[0].fine), (60, 0.0));
        assert_eq!((p.notes[1].pitch, p.notes[1].fine), (65, 30.0));
        assert_eq!(p.length, BAR);
        assert_ne!(p.notes[0].id, p.notes[1].id);
        assert_eq!(song.clips.len(), 2);
        assert_eq!(
            (
                song.clips[1].start,
                song.clips[1].track,
                song.clips[1].length
            ),
            (BAR, 2, Some(BAR))
        );
        assert_eq!(song.tracks[2].name, "Drums");
        assert_eq!(song.tracks[2].color, Some(0x332211));
        assert!(
            report
                .lines()
                .iter()
                .any(|l| l.starts_with("Automation Clip: automation clip"))
        );
    }

    #[test]
    fn one_channel_project_keeps_settings_and_that_channel_only() {
        let full = sample_flp();
        let one = channel_flp(&full, 1).unwrap();
        let (song, report) = read_flp(&one).unwrap();
        assert_eq!(song.tempo, 140.0);
        assert_eq!(report.channels.len(), 1);
        assert_eq!(
            (
                report.channels[0].index,
                report.channels[0].name.as_str(),
                report.channels[0].plugin.as_str()
            ),
            (0, "Lead", "FLEX")
        );
        assert_eq!(report.patterns, 0);
        assert!(song.clips.is_empty());
        let st = fl_vsti_state(&full, 1).unwrap();
        assert!(st.starts_with(b"KFV2C") && st[5 + FL_VSTI_HEADER.len()..].starts_with(b"FLhd"));
        assert!(channel_flp(&full, 9).is_err());
    }

    #[test]
    fn directwave_program_name_is_found() {
        let d = b"\x01\x00junk\x00Piano 2.dwph\x00more %FLStudioFactoryData%\\Piano 2\\x.wav";
        assert_eq!(dwp_name(d).as_deref(), Some("Piano 2.dwp"));
        assert_eq!(dwp_name(b"nothing here"), None);
    }

    #[test]
    fn mixer_inserts_routing_and_effects() {
        let mut d = Vec::new();
        ev_num(&mut d, CH_NEW, 0);
        ev_num(&mut d, CH_TYPE, 2);
        ev_data(&mut d, PLUGIN_INTERNAL, &utf16("Sytrus"));
        ev_num(&mut d, CH_INSERT, 20);
        // Channel 1 automates insert 20's fader and its reverb's room size.
        let link = |param: u16, dest: u16| -> Vec<u8> {
            let mut r = vec![0u8; 20];
            r[2..4].copy_from_slice(&1u16.to_le_bytes());
            r[8..10].copy_from_slice(&param.to_le_bytes());
            r[10..12].copy_from_slice(&dest.to_le_bytes());
            r
        };
        ev_data(&mut d, AUTO_LINK, &link(0x1fc0, 0x2000 | 20 << 6));
        ev_data(&mut d, AUTO_LINK, &link(0x8003, 0x2000 | 20 << 6));
        ev_data(&mut d, AUTO_LINK, &link(0x0005, 0x4000));
        ev_num(&mut d, CH_NEW, 1);
        ev_num(&mut d, CH_TYPE, 5);
        ev_data(&mut d, PLUGIN_NAME, &utf16("Swell"));
        let mut pts = [1u32.to_le_bytes(), 64u32.to_le_bytes()].concat();
        pts.push(0);
        pts.extend([4u32, 3, 3].iter().flat_map(|v| v.to_le_bytes()));
        for (pos, v, t) in [(0.0f64, 0.8f64, 0.0f32), (4.0, 0.0, 0.0), (4.0, 1.0, -0.5)] {
            pts.extend(pos.to_le_bytes());
            pts.extend(v.to_le_bytes());
            pts.extend(t.to_le_bytes());
            pts.extend([0, 0, 0, 2]);
        }
        ev_data(&mut d, AUTO_POINTS, &pts);
        let mut clip = item(96 * 4, 0, 96 * 8, 1);
        clip[6..8].copy_from_slice(&1u16.to_le_bytes());
        clip[24..28].copy_from_slice(&2.0f32.to_le_bytes());
        ev_num(&mut d, ARR_NEW, 0);
        ev_data(&mut d, PLAYLIST, &clip);
        let flags = |on: bool| {
            [0u32, if on { 0x0c } else { 0x04 }, 0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>()
        };
        let mut routes = vec![0u8; 127];
        for i in 0..22u32 {
            ev_data(&mut d, INSERT_FLAGS, &flags(i != 3));
            if i == 20 {
                ev_data(&mut d, PLUGIN_INTERNAL, &utf16("Fruity Reeverb 2"));
                let verb: Vec<u8> = [
                    10001i32, 75, 40, 0, 30, 100, 40, 68, 300, 2000, 0, 128, 64, 64, 684, 0,
                ]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
                ev_data(&mut d, PLUGIN_DATA, &verb);
                ev_num(&mut d, SLOT_INDEX, 0);
                ev_data(&mut d, PLUGIN_INTERNAL, &utf16("Edison"));
                ev_num(&mut d, SLOT_INDEX, 1);
                ev_num(&mut d, SLOT_INDEX, 2);
                routes.fill(0);
                routes[21] = 1;
            } else {
                routes.fill(0);
                routes[0] = 1;
            }
            ev_data(&mut d, INSERT_ROUTING, &routes);
            // FL writes the next insert's name after this one's output.
            ev_num(&mut d, INSERT_OUTPUT, 0);
            if i == 19 {
                ev_data(&mut d, INSERT_NAME, &utf16("Lead bus"));
            }
        }
        let rec = |pid: u8, ins: u16, slot: u16, v: i32| -> Vec<u8> {
            [
                vec![0, 0, 0, 0, pid, 0],
                ((ins << 6) | slot).to_le_bytes().to_vec(),
                v.to_le_bytes().to_vec(),
            ]
            .concat()
        };
        let params = [
            rec(192, 20, 0, 16000),
            rec(193, 20, 0, -3200),
            rec(64 + 21, 20, 0, 6400),
            rec(1, 20, 0, 6400),
            rec(208, 20, 0, -600),
            rec(217, 20, 0, 40000),
            rec(194, 20, 0, 32),
            rec(210, 21, 0, 300),
        ]
        .concat();
        ev_data(&mut d, MIXER_PARAMS, &params);
        let mut f = b"FLhd".to_vec();
        f.extend_from_slice(&6u32.to_le_bytes());
        f.extend_from_slice(&[0, 0, 1, 0, 96, 0]);
        f.extend_from_slice(b"FLdt");
        f.extend_from_slice(&(d.len() as u32).to_le_bytes());
        f.extend(d);
        let (song, report) = read_flp(&f).unwrap();
        assert_eq!(song.mixer.len(), 22);
        assert_eq!(song.instruments[0].mixer, 20);
        let t = &song.mixer[20];
        assert_eq!(t.name, "Lead bus");
        assert!((t.volume - crate::sequence::FADER_MAX).abs() < 1e-6);
        assert!((t.pan + 0.5).abs() < 1e-6);
        assert!(!t.to_master);
        assert_eq!(t.sends.len(), 1);
        assert_eq!((t.sends[0].to, t.sends[0].level), (21, 0.5));
        // The reverb, then FL's insert EQ: low band -6 dB at FL's 90 Hz.
        assert_eq!(t.fx.len(), 2);
        assert_eq!(t.fx[1].kind, FxKind::Eq);
        assert_eq!(t.fx[1].params[1], -6.0);
        assert!(
            (t.fx[1].params[0] - 89.5).abs() < 0.5,
            "{}",
            t.fx[1].params[0]
        );
        // A moved mid band: an EQ slot (FL's 40000 ≈ 3.2 kHz).
        assert!(
            (t.fx[1].params[2] - 3186.0).abs() < 2.0,
            "{}",
            t.fx[1].params[2]
        );
        assert_eq!(t.sep, -0.5, "half way to mono");
        // FL's EQ on its usual bands: the strip EQ.
        assert_eq!(song.mixer[21].eq, [0.0, 0.0, 3.0]);
        assert_eq!(report.insert_eqs, 2);
        assert_eq!(t.fx[0].kind, FxKind::RoomReverb);
        // Its settings carried over, so it plays: room size 30, low cut 75 Hz,
        // wet 64/128.
        assert!(t.fx[0].enabled && (t.fx[0].mix - 0.5).abs() < 1e-6);
        assert_eq!(
            (t.fx[0].params[0], t.fx[0].params[3], t.fx[0].params[4]),
            (30.0, 75.0, 50.0)
        );
        assert!(song.mixer[3].muted && !song.mixer[4].muted);
        assert!(song.mixer[5].to_master);
        assert_eq!(report.skipped_effects, ["Edison"]);
        assert_eq!((report.converted_effects, report.stand_in_effects), (1, 0));
        // Automation: two targets → two patterns, each with a clip at bar 2;
        // tempo automation is reported, not moved.
        use crate::automation::Target;
        let autos: Vec<_> = song
            .patterns
            .iter()
            .filter_map(|p| p.automation.as_ref().map(|e| (p, e)))
            .collect();
        assert_eq!(autos.len(), 2);
        let (p, vol) = autos[0];
        assert_eq!(vol.target, Target::MixerVolume { track: 20 });
        assert_eq!(p.name, "Swell");
        assert_eq!(
            vol.points.iter().map(|p| p.tick).collect::<Vec<_>>(),
            [0, 4 * PPQ, 8 * PPQ]
        );
        // 0.8 of FL's fader = 0 dB; tension -0.5 bends the curve into the
        // last point, ours sits on the point before.
        assert!((vol.points[0].value - 1.0).abs() < 1e-5 && vol.points[1].value == 0.0);
        assert_eq!((vol.points[1].tension, vol.points[2].tension), (-0.5, 0.0));
        assert_eq!(
            autos[1].1.target,
            Target::FxParam {
                track: 20,
                slot: 0,
                param: 0
            }
        );
        assert_eq!(autos[1].1.points[2].value, 100.0);
        let clips: Vec<_> = song
            .clips
            .iter()
            .filter(|c| autos.iter().any(|(p, _)| p.id == c.pattern))
            .collect();
        assert_eq!(clips.len(), 2);
        assert_eq!(
            (
                clips[0].start,
                clips[0].length,
                clips[0].track,
                clips[0].offset
            ),
            (BAR, Some(2 * BAR), 1, 2 * PPQ)
        );
        assert_eq!(report.automations, 2);
        // The same clip drives the tempo (0.8 → 156, 0 → 60, 1 → 180 BPM over
        // its 2 bars from 2 beats in): the song's tempo track.
        use crate::tempo::TempoShape::*;
        let t: Vec<_> = song
            .tempo_map
            .points
            .iter()
            .map(|p| (p.tick, p.bpm.round(), p.shape))
            .collect();
        assert_eq!(
            t,
            [
                (0, 120.0, Step),
                (BAR, 108.0, Linear),
                (BAR + 2 * PPQ, 60.0, Linear),
                (BAR + 6 * PPQ, 180.0, Step)
            ]
        );
    }

    #[test]
    fn audio_clips_play_their_sample_from_their_start() {
        let mut d = Vec::new();
        ev_num(&mut d, TEMPO, 120_000);
        ev_num(&mut d, CH_NEW, 0);
        // FL 2025 writes audio clip channels as type 4 without a plugin.
        ev_num(&mut d, CH_TYPE, 4);
        ev_data(&mut d, PLUGIN_NAME, &utf16("Vox"));
        ev_data(&mut d, SAMPLE_PATH, &utf16("C:\\vox.wav"));
        ev_num(&mut d, ARR_NEW, 0);
        let clip = |bar: u32, ms: f32, len_bars: u32| {
            let mut r = item(96 * 4 * bar, 0, 96 * 4 * len_bars, 0);
            r[6..8].copy_from_slice(&0u16.to_le_bytes());
            r[24..28].copy_from_slice(&ms.to_le_bytes());
            r
        };
        // Audio clip offsets are milliseconds into the sample.
        let pl = [clip(0, -1.0, 2), clip(4, 0.0, 1), clip(8, 3000.0, 1)].concat();
        ev_data(&mut d, PLAYLIST, &pl);
        let mut f = b"FLhd".to_vec();
        f.extend_from_slice(&6u32.to_le_bytes());
        f.extend_from_slice(&[0, 0, 1, 0, 96, 0]);
        f.extend_from_slice(b"FLdt");
        f.extend_from_slice(&(d.len() as u32).to_le_bytes());
        f.extend(d);
        let (song, report) = read_flp(&f).unwrap();
        assert_eq!(report.audio_imported, 3);
        assert_eq!(song.clips.len(), 3);
        // Two start points → two one-note patterns (keys 60 and 61); the
        // whole-sample one is as long as its longest clip.
        let key = |c: &Clip| {
            song.patterns
                .iter()
                .find(|p| p.id == c.pattern)
                .map(|p| (p.notes[0].pitch, p.notes[0].length))
                .unwrap()
        };
        assert_eq!(key(&song.clips[0]), (60, 2 * BAR));
        assert_eq!(song.clips[0].pattern, song.clips[1].pattern);
        assert_eq!(key(&song.clips[2]), (61, BAR));
        assert_eq!(
            (song.clips[2].start, song.clips[2].length),
            (8 * BAR, Some(BAR))
        );
        assert_eq!(
            report.audio_slices,
            [(
                song.instruments[0].id,
                AudioSlices {
                    slices: vec![AudioSlice::at(0.0), AudioSlice::at(3.0)],
                    tune_cents: 0.0
                }
            )]
        );
        let preset = crate::sequence::SamplePreset::from_sample("v", 1000, vec![[0.0; 2]; 5000]);
        let cut = slice_preset(
            preset,
            &AudioSlices {
                slices: vec![AudioSlice::at(0.0), AudioSlice::at(3.0)],
                tune_cents: 0.0,
            },
        );
        assert_eq!(cut.zones.len(), 2);
        assert_eq!(
            (
                cut.zones[1].root,
                cut.zones[1].low,
                cut.zones[1].high,
                cut.zones[1].offset
            ),
            (61, 61, 61, 3000)
        );
        assert_eq!(
            (cut.zones[0].low, cut.zones[0].high, cut.zones[0].offset),
            (60, 60, 0)
        );

        // Played by the engine: a sample quiet for 3 s, loud after. The clip
        // at bar 0 sounds quiet, the one starting 3 s in sounds loud.
        let rate = 8000;
        let frames: Vec<[f32; 2]> = (0..rate * 6)
            .map(|i| {
                let a = if i < rate * 3 { 0.1 } else { 0.6 };
                let v = a * (i as f32 * 0.25).sin();
                [v, v]
            })
            .collect();
        let preset = crate::sequence::SamplePreset::from_sample("v", rate as u32, frames);
        let mut song = song;
        let id = song.instruments[0].id;
        song.set_sampler_preset(id, slice_preset(preset, &report.audio_slices[0].1))
            .unwrap();
        let out = crate::render::render_audio(
            &song,
            0,
            true,
            crate::render::Tail::Cut,
            &Default::default(),
            &mut |_| {},
        )
        .unwrap();
        let bar = 2 * crate::render::RATE as usize; // 120 BPM, 4/4
        let peak = |from: usize| crate::render::peak(&out[from..from + bar / 4]);
        let (quiet, loud) = (peak(bar / 4), peak(8 * bar + bar / 4));
        assert!(
            quiet > 0.01 && loud > quiet * 3.0,
            "quiet {quiet} loud {loud}"
        );
    }

    #[test]
    fn solo_project_keeps_one_channel_dry() {
        let full = sample_flp();
        let solo = solo_channel_flp(&full, 1).unwrap();
        let (song, report) = read_flp(&solo).unwrap();
        assert!(report.channels.iter().all(|c| c.enabled == (c.index == 1)));
        assert_eq!(song.patterns[0].notes.len(), 2);
        assert!(solo_channel_flp(&full, 7).is_err());
    }

    #[test]
    fn mixer_addresses_keep_old_and_new_inserts_in_order() {
        assert_eq!(mixer_address(0x2001), Some((0, 1)));
        assert_eq!(mixer_address(0x3f80), Some((126, 0)));
        assert_eq!(mixer_address(0x7001), Some((0, 1)));
        assert_eq!(mixer_address(0x7349), Some((13, 9)));
        assert_eq!(mixer_address(0x8001), Some((64, 1)));
        assert_eq!(mixer_address(0x8f80), Some((126, 0)));
        assert_eq!(mixer_address(0x4000), None);
        assert_eq!(mixer_address(0xe000), None);
    }

    #[test]
    fn rejects_other_files() {
        assert!(read_flp(b"RIFF....").is_err());
        let mut bad = sample_flp();
        bad.truncate(40);
        // A cut file reads what it can or reports truncation, never panics.
        let _ = read_flp(&bad);
    }

    #[test]
    fn playlist_layout_mute_bit_and_stretch_pitch() {
        // 88-byte records (FL 25.2+): the layout event says so; the sample
        // window is in ticks.
        let mut r = item(0, 0, 96, 0);
        r.resize(88, 0);
        r[18..20].copy_from_slice(&0x2040u16.to_le_bytes());
        r[24..28].copy_from_slice(&48i32.to_le_bytes());
        let two = [r.clone(), r].concat();
        let items = playlist_items(&two, Some(0xF));
        assert_eq!(items.len(), 2);
        assert!(items[0].muted && items[0].window_ticks);
        // 48 ticks at 120 BPM, PPQ 96 = a quarter second.
        assert!((items[0].audio_start(60.0 / 120.0 / 96.0) - 0.25).abs() < 1e-9);
        // FL 2025.1 (layout 7): 80 bytes, offset in ms; 0x1000 alone is not mute.
        let mut r = item(0, 0, 96, 0);
        r[18..20].copy_from_slice(&0x1040u16.to_le_bytes());
        r[24..28].copy_from_slice(&967.7419f32.to_le_bytes());
        let items = playlist_items(&r, Some(7));
        assert!(!items[0].muted && !items[0].window_ticks);
        assert!((items[0].audio_start(1.0) - 0.9677419).abs() < 1e-6);
        // Without the event the size is found from the records.
        assert_eq!(playlist_items(&[r.clone(), r].concat(), None).len(), 2);

        let resample = FlpStretch {
            pitch: 300,
            multiplier: 10000,
            mode: 0,
        };
        assert_eq!(resample.cents(), 1500.0);
        let auto = FlpStretch {
            mode: 5,
            ..resample
        };
        assert_eq!(auto.cents(), 300.0);
        let preset = crate::sequence::SamplePreset::from_sample("v", 1000, vec![[0.0; 2]; 100]);
        let one = slice_preset(
            preset,
            &AudioSlices {
                slices: vec![AudioSlice::at(0.0)],
                tune_cents: -400.0,
            },
        );
        assert_eq!(one.zones.len(), 1);
        assert_eq!(one.zones[0].tune_cents, -400.0);

        // Fades: a clip window 0.01..0.05 s fading 10 ms in and 20 ms out.
        let mut r = item(0, 0, 96, 0);
        r[6..8].copy_from_slice(&0u16.to_le_bytes());
        r[24..28].copy_from_slice(&10f32.to_le_bytes());
        r[28..32].copy_from_slice(&50f32.to_le_bytes());
        r[36..40].copy_from_slice(&10f32.to_le_bytes());
        r[44..48].copy_from_slice(&20f32.to_le_bytes());
        let cut = playlist_items(&r, Some(7))[0].audio_slice(1.0);
        assert_eq!(cut.end, Some(0.05));
        let preset = crate::sequence::SamplePreset::from_sample("v", 1000, vec![[1.0; 2]; 100]);
        let z = &slice_preset(
            preset,
            &AudioSlices {
                slices: vec![AudioSlice::at(0.0), cut],
                tune_cents: 0.0,
            },
        )
        .zones[1];
        assert_eq!((z.offset, z.end, z.fade_in, z.fade_out), (10, 50, 10, 20));
        assert_eq!(z.fade_gain(10.0), 0.0);
        assert_eq!(z.fade_gain(15.0), 0.5);
        assert_eq!(z.fade_gain(25.0), 1.0);
        assert_eq!(z.fade_gain(40.0), 0.5);
    }

    /// The FL 21 demo with vocal audio clips: offsets are milliseconds.
    #[test]
    fn demo_audio_clips_start_where_fl_starts_them() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home).join(
            ".local/share/keyframexl/wine32-prefix/drive_c/Program Files/Image-Line/FL Studio 21/Data/Demo projects/NewStuff.flp",
        );
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let (_, report) = read_flp(&bytes).unwrap();
        let all: Vec<f64> = report
            .audio_slices
            .iter()
            .flat_map(|(_, s)| s.slices.iter().map(|x| x.start))
            .collect();
        assert!(all.iter().any(|s| (s - 0.9677419).abs() < 1e-4), "{all:?}");
        // The longest sample there (KARRA LEAD VOCAL.ogg) is 226 s.
        assert!(
            all.iter().all(|s| *s < 226.0),
            "offsets inside the samples: {all:?}"
        );
        assert!(report.audio_slices.iter().any(|(_, s)| s.tune_cents != 0.0));
        // Its first vocal clip fades in 25.2 ms and out 10.08 ms, ending at 3871 ms.
        assert!(report.audio_faded > 0);
        let faded = report
            .audio_slices
            .iter()
            .flat_map(|(_, s)| &s.slices)
            .find(|x| (x.start - 0.9677419).abs() < 1e-4 && x.fade_in > 0.0)
            .copied()
            .unwrap();
        assert!((faded.fade_in - 0.0252).abs() < 1e-4, "{faded:?}");
        assert!((faded.end.unwrap() - 3.871).abs() < 1e-3, "{faded:?}");
    }

    #[test]
    fn user_projects_when_present() {
        // Real FL 2025 projects on the developer machine; skipped elsewhere.
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home)
            .join("Документи/Image-Line/FL Studio/Projects/Dear Abonent/Dear Abonent-backip.flp");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let (song, report) = read_flp(&bytes).unwrap();
        assert!((song.tempo - 94.661).abs() < 1e-3);
        assert_eq!(report.patterns, 18);
        assert!(report.notes > 100);
        assert!(report.clips > 20);
        assert!(song.instruments.iter().any(|i| i.name == "Good 80 Super"));
        // Channels keep FL's inserts; inserts keep their faders and effects.
        assert!(song.instruments.iter().any(|i| i.mixer == 12));
        assert!(song.mixer.iter().map(|t| t.fx.len()).sum::<usize>() > 10);
        assert_eq!(report.inserts[0].volume, Some(12170));
        assert_eq!(report.inserts[4].volume, Some(14423));
        assert_eq!(report.inserts[7].volume, Some(10059));
        assert!(song.mixer[0].volume < 1.0);
        assert!(song.mixer[4].volume > 1.0);
        assert_eq!(report.inserts[0].slots[1].mix, 0.0);
        let phone_eq = song.mixer[0]
            .fx
            .iter()
            .position(|fx| fx.kind == FxKind::ParamEq)
            .unwrap();
        let mix = crate::automation::Target::FxMix {
            track: 0,
            slot: phone_eq as u8,
        };
        assert_eq!(mix.current(&song), Some(0.0));
        let mut values = Vec::with_capacity(16);
        let master_mix = |raw_tick, values: &mut Vec<_>| {
            crate::automation::values_at(&song, raw_tick * PPQ / 96, values);
            crate::automation::find(values, mix)
        };
        assert_eq!(master_mix(2200, &mut values), None);
        assert!(master_mix(5800, &mut values).is_some_and(|v| v > 0.0 && v < 1.0));
        assert_eq!(master_mix(6044, &mut values), master_mix(6070, &mut values));
        assert!(report.inserts[0].slots.iter().any(|s| !s.enabled));
        assert!(song.tempo_map.points.iter().any(|p| {
            (p.bpm - 58.378).abs() < 1e-6 && p.shape == crate::tempo::TempoShape::Step
        }));
        assert!(
            song.patterns
                .iter()
                .any(|p| p.automation.as_ref().is_some_and(|env| {
                    matches!(
                        env.target,
                        crate::automation::Target::FxMix { track: 8, .. }
                    ) && env.points.len() > 2
                }))
        );
        song.validate().unwrap();
    }

    #[test]
    fn user_project_plugins_carry_their_state() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home)
            .join("Документи/Image-Line/FL Studio/Projects/Dear Abonent/Dear Abonent-backip.flp");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let (song, report) = read_flp(&bytes).unwrap();
        let vst = |n: &str| {
            report
                .channels
                .iter()
                .find_map(|c| c.vst.clone().filter(|v| v.name == n))
                .unwrap()
        };
        let ssd = vst("SSDSampler5");
        assert!(!ssd.vst3 && ssd.state.starts_with(b"KFV2CVC2!"));
        let piano = vst("Grand Piano");
        assert!(piano.vst3 && piano.path.ends_with("Grand Piano.vst3") && piano.state.len() > 1000);
        use base64::Engine as _;
        let decode = |state: &FlPluginState| {
            base64::engine::general_purpose::STANDARD
                .decode(state.state.as_bytes())
                .unwrap()
        };
        for (name, plugin) in [("Bass", "Sakura"), ("Good 80 Super", "FLEX")] {
            let state = song
                .instruments
                .iter()
                .find(|i| i.name == name)
                .unwrap()
                .fl_plugin
                .as_ref()
                .unwrap();
            assert_eq!(state.name, plugin);
            assert_eq!(
                decode(state),
                report
                    .channels
                    .iter()
                    .find(|c| c.name == name)
                    .unwrap()
                    .data
            );
        }
        let eq = &song.mixer[0].fl_slots[1];
        assert_eq!(eq.plugin.name, "Fruity Parametric EQ 2");
        assert_eq!(decode(&eq.plugin), report.inserts[0].slots[1].data);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("import.kfxl");
        crate::project_io::save(&song, &path).unwrap();
        let reopened = crate::project_io::open(&path).unwrap();
        assert_eq!(reopened.mixer[0].fl_slots, song.mixer[0].fl_slots);
        assert_eq!(
            reopened.instruments[9].fl_plugin,
            song.instruments[9].fl_plugin
        );
    }

    #[test]
    fn vst_wrapper_data_gives_name_kind_and_state() {
        fn rec(id: u32, d: &[u8]) -> Vec<u8> {
            [
                id.to_le_bytes().as_slice(),
                &(d.len() as u64).to_le_bytes(),
                d,
            ]
            .concat()
        }
        let chunk = b"VC2!state";
        let mut v2 = vec![0xf7, 0xff, 0xff, 0xff, 0x0d, 0xfe, 0xff, 0xff, 0xff];
        v2.extend((chunk.len() as u64).to_le_bytes());
        v2.extend([0, 0, 0, 0]);
        v2.extend(chunk);
        let d = [
            vec![12, 0, 0, 0],
            rec(50, &[4, 0, 0, 0]),
            rec(54, b"Synth X"),
            rec(56, b"Vendor"),
            rec(53, &v2),
        ]
        .concat();
        let v = wrapper_vst(&d).unwrap();
        assert_eq!(
            (v.name.as_str(), v.vendor.as_str(), v.vst3),
            ("Synth X", "Vendor", false)
        );
        assert_eq!(v.state, b"KFV2CVC2!state");
        let v3state = [
            vec![1, 0, 0, 0],
            rec(1, &[0; 64]),
            rec(3, b"component"),
            rec(4, &[0; 8]),
        ]
        .concat();
        let d = [
            vec![12, 0, 0, 0],
            rec(50, &[8, 0, 0, 0]),
            rec(54, b"Piano"),
            rec(53, &v3state),
        ]
        .concat();
        let v = wrapper_vst(&d).unwrap();
        assert!(v.vst3);
        assert_eq!(v.state, b"component");
    }

    #[test]
    #[cfg(unix)]
    fn fl_paths_resolve_into_the_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let pfx = dir.path();
        std::fs::create_dir_all(pfx.join("drive_c/Program Files/Image-Line/FL Studio 2025/Data"))
            .unwrap();
        std::fs::write(
            pfx.join("drive_c/Program Files/Image-Line/FL Studio 2025/Data/Kick.wav"),
            b"",
        )
        .unwrap();
        std::fs::create_dir_all(pfx.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink("../drive_c", pfx.join("dosdevices/c:")).unwrap();
        let install = pfx.join("dosdevices/c:/Program Files/Image-Line/FL Studio 2025");
        let got =
            resolve_path("%FLStudioFactoryData%\\Data\\Kick.wav", pfx, Some(&install)).unwrap();
        assert!(got.ends_with("Data/Kick.wav") && got.exists());
        // Different case than on disk, as Windows paths often are.
        let got = resolve_path(
            "C:\\program files\\image-line\\FL STUDIO 2025\\data\\kick.WAV",
            pfx,
            None,
        )
        .unwrap();
        assert!(got.exists());
        assert!(resolve_path("C:\\nothing\\here.wav", pfx, None).is_none());
    }
}

#[cfg(test)]
mod corpus_dump {
    /// `KFXL_FLP_DUMP="hyper chorus" cargo test --lib corpus_dump -- --ignored --nocapture`:
    /// the plugin data of every slot or channel with that FL plugin, over every .flp under $HOME.
    #[test]
    #[ignore]
    fn dump_plugin_data() {
        let want = std::env::var("KFXL_FLP_DUMP")
            .unwrap_or_default()
            .to_lowercase();
        let out = std::process::Command::new("find")
            .args([
                std::env::var("HOME").unwrap().as_str(),
                "-name",
                "*.flp",
                "-not",
                "-path",
                "*/.cache/*",
            ])
            .output()
            .unwrap();
        let mut seen = 0;
        for path in String::from_utf8_lossy(&out.stdout).lines() {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Ok((song, _)) = super::read_flp(&bytes) else {
                continue;
            };
            for i in &song.instruments {
                if let Some(fp) = &i.fl_plugin
                    && fp.name.to_lowercase() == want
                    && seen < 40
                {
                    seen += 1;
                    let d = fp.data();
                    if want == "flex" {
                        let text = |a: usize| {
                            d.get(a..a + 1024)
                                .map(|b| {
                                    String::from_utf8_lossy(
                                        &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())],
                                    )
                                    .into_owned()
                                })
                                .unwrap_or_default()
                        };
                        println!(
                            "FLEX len {} pack {:?} preset {:?}",
                            d.len(),
                            text(0x10),
                            text(0x411)
                        );
                        continue;
                    }
                    let ints: Vec<i32> = d
                        .chunks_exact(4)
                        .take(60)
                        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    println!(
                        "{} [ch {}] len {} {:?}",
                        path.rsplit('/').next().unwrap(),
                        i.name,
                        d.len(),
                        ints
                    );
                }
            }
            for m in &song.mixer {
                for s in &m.fl_slots {
                    if s.plugin.name.to_lowercase() == want && seen < 40 {
                        seen += 1;
                        let d = s.plugin.data();
                        let ints: Vec<i32> = d
                            .chunks_exact(4)
                            .take(60)
                            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        println!(
                            "{} [{}] len {} {:?}",
                            path.rsplit('/').next().unwrap(),
                            m.name,
                            d.len(),
                            ints
                        );
                    }
                }
            }
        }
    }

    /// Mixer automation over the corpus: what became targets, what did not.
    #[test]
    #[ignore]
    fn mixer_automation_over_the_corpus() {
        let out = std::process::Command::new("find")
            .args([
                std::env::var("HOME").unwrap().as_str(),
                "-name",
                "*.flp",
                "-not",
                "-path",
                "*/.cache/*",
            ])
            .output()
            .unwrap();
        let (mut enabled, mut sends) = (0, 0);
        let mut notes = std::collections::BTreeMap::<String, usize>::new();
        for path in String::from_utf8_lossy(&out.stdout).lines() {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Ok((song, report)) = super::read_flp(&bytes) else {
                continue;
            };
            song.validate().unwrap();
            for p in &song.patterns {
                match p.automation.as_ref().map(|a| a.target) {
                    Some(crate::automation::Target::FxEnabled { .. }) => enabled += 1,
                    Some(crate::automation::Target::SendLevel { .. }) => sends += 1,
                    _ => {}
                }
            }
            for n in &report.automation_notes {
                let key = n.split(": ").last().unwrap_or(n).to_string();
                *notes.entry(key).or_default() += 1;
            }
        }
        println!("slot switches {enabled}, send levels {sends}");
        for (n, c) in notes {
            println!("{c:5}  {n}");
        }
    }

    #[test]
    fn resolve_native_path_expands_fl_variables_and_normalizes_separators() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("FL Studio 2025");
        let user = dir.path().join("Image-Line");
        let kick = install
            .join("Data")
            .join("Patches")
            .join("Packs")
            .join("Kick.wav");
        let rec = user.join("FL Studio").join("Audio").join("Take1.wav");
        std::fs::create_dir_all(kick.parent().unwrap()).unwrap();
        std::fs::create_dir_all(rec.parent().unwrap()).unwrap();
        std::fs::write(&kick, b"wav").unwrap();
        std::fs::write(&rec, b"wav").unwrap();

        assert_eq!(
            super::resolve_native_path(
                r"%FLStudioFactoryData%\Data\Patches\Packs\Kick.wav",
                Some(&install),
                Some(&user),
            ),
            Some(kick.clone()),
        );
        // Case-insensitive variable prefix and forward slashes.
        assert_eq!(
            super::resolve_native_path(
                "%flstudiofactorydata%/Data/Patches/Packs/Kick.wav",
                Some(&install),
                Some(&user),
            ),
            Some(kick),
        );
        assert_eq!(
            super::resolve_native_path(
                r"%FLStudioUserData%\FL Studio\Audio\Take1.wav",
                Some(&install),
                Some(&user),
            ),
            Some(rec.clone()),
        );
        // Direct native path.
        assert_eq!(
            super::resolve_native_path(&rec.to_string_lossy(), Some(&install), Some(&user),),
            Some(rec),
        );
        // Missing file returns None.
        assert_eq!(
            super::resolve_native_path(
                r"%FLStudioFactoryData%\Data\Patches\Packs\Missing.wav",
                Some(&install),
                Some(&user),
            ),
            None,
        );
    }
}
