//! Standard MIDI File import and export (midly).
//!
//! Export builds a format-1 SMF at 480 PPQ without touching the song;
//! import parses a file into a brand-new pattern plus new channels,
//! leaving the song untouched when anything fails.

use std::collections::{BTreeMap, VecDeque};

use midly::{
    Format, Header, MetaMessage, MidiMessage, PitchBend, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u14, u15, u24, u28},
};

use crate::automation::{self, Target, ValueAt};
use crate::sequence::{
    BAR, Instrument, InstrumentKind, Note, PPQ, Pattern, STEP, STEP_PITCH, Song, Tick,
};

/// Sampling step (ticks) for continuous MIDI CC automation and pitch-bend slides (1/64 note).
const CC_STEP: u64 = (STEP / 4) as u64;

/// GM drum pitch per [`InstrumentKind::Drum`] voice (kick, snare, hat).
const DRUM_GM: [u8; 3] = [36, 38, 42];
/// MIDI channel index (0-based) reserved for drums.
const DRUM_CHANNEL: u8 = 9;
/// Conductor track label; [`Song`] carries no project name.
const CONDUCTOR_NAME: &str = "KeyFrameXL";

/// What the UI asked to export.
pub enum MidiSource {
    Pattern(u64),
    Song,
}

/// Export switches.
pub struct ExportOptions {
    pub one_track_per_channel: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            one_track_per_channel: true,
        }
    }
}

/// Import switches.
pub struct ImportOptions {
    pub drums_on_channel_10: bool,
    pub one_channel_per_track: bool,
    /// Channel 10 becomes one channel playing a whole drum kit (its notes
    /// keep their GM keys) instead of separate kick / snare / hat channels.
    /// The program app sets it when it has a kit to load.
    pub drum_kit: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            drums_on_channel_10: true,
            one_channel_per_track: true,
            drum_kit: false,
        }
    }
}

/// Prefix of a channel made from MIDI channel 10 as a whole kit.
pub const DRUM_KIT_PREFIX: &str = "Drums · ";

/// The GM / GS drum kit a channel-10 program change names.
pub fn gm_kit_name(program: Option<u8>) -> &'static str {
    const KITS: [(u8, &str); 13] = [
        (0, "Standard"),
        (1, "Standard 2"),
        (2, "Standard 3"),
        (8, "Room"),
        (16, "Power"),
        (24, "Electronic"),
        (25, "TR-808"),
        (26, "TR-909"),
        (27, "CR-78"),
        (32, "Jazz"),
        (40, "Brush"),
        (48, "Orchestra"),
        (56, "SFX"),
    ];
    let p = program.unwrap_or(0);
    // Variations (e.g. 9-15 rooms) take the kit they belong to.
    KITS.iter()
        .rev()
        .find(|(k, _)| *k <= p)
        .map(|(_, n)| *n)
        .unwrap_or("Standard")
}

/// What an import created, for the status line and the details popup.
#[derive(Debug)]
pub struct ImportReport {
    pub pattern: u64,
    pub channels: Vec<u64>,
    pub notes: usize,
    pub tracks: usize,
    pub tempo: Option<f64>,
    pub tempos: usize,
    pub meters: usize,
    pub ignored_pitch_bends: usize,
    pub ignored_controllers: usize,
    pub repaired_notes: usize,
    pub names: Vec<String>,
}

/// One flattened export note before overlap repair.
struct OutNote {
    inst: usize,
    channel: u8,
    start: u64,
    end: u64,
    pitch: u8,
    vel: u8,
    pan: f32,
    fine: f32,
    mod_x: f32,
    mod_y: f32,
}

/// One flattened pitch slide on an instrument.
struct OutSlide {
    inst: usize,
    start: u64,
    end: u64,
    semitones: f64,
}

fn midi_channel(song: &Song, inst: usize) -> u8 {
    match song.instruments[inst].kind {
        InstrumentKind::Drum { .. } => DRUM_CHANNEL,
        _ => {
            let c = (inst % 15) as u8;
            if c >= DRUM_CHANNEL { c + 1 } else { c }
        }
    }
}

fn export_pitch(song: &Song, inst: usize, pitch: u8) -> u8 {
    match song.instruments[inst].kind {
        InstrumentKind::Drum { voice } => DRUM_GM.get(voice as usize).copied().unwrap_or(42),
        _ => pitch.min(127),
    }
}

/// Balance-law stereo gain pair `[L, R]` from linear gain and pan (`-1..=1`).
fn balance_gains(vol: f32, pan: f32) -> [f32; 2] {
    let v = vol.max(0.0);
    let p = pan.clamp(-1.0, 1.0);
    [v * (1.0 - p.max(0.0)), v * (1.0 + p.min(0.0))]
}

/// Collapse a stereo gain pair back into `(volume, pan)` under the balance law.
fn gains_to_vol_pan(g: [f32; 2]) -> (f32, f32) {
    let vol = g[0].max(g[1]);
    if vol <= 1e-6 {
        (0.0, 0.0)
    } else {
        (vol, ((g[1] - g[0]) / vol).clamp(-1.0, 1.0))
    }
}

/// General MIDI volume law: `gain = (cc7 / 127)²`.
fn cc7_to_gain(cc7: u8) -> f32 {
    (cc7 as f32 / 127.0).powi(2)
}

/// Linear gain -> General MIDI CC 7 volume (inverse of `cc7_to_gain`).
fn gain_to_cc7(gain: f32) -> u8 {
    (gain.clamp(0.0, 1.0).sqrt() * 127.0)
        .round()
        .clamp(0.0, 127.0) as u8
}

/// MIDI CC 10 pan (`0` left, `64` centre, `127` right) -> `-1..=1`.
fn cc10_to_pan(cc10: u8) -> f32 {
    if cc10 < 64 {
        ((cc10 as f32 - 64.0) / 64.0).clamp(-1.0, 0.0)
    } else {
        ((cc10 as f32 - 64.0) / 63.0).clamp(0.0, 1.0)
    }
}

/// Pan `-1..=1` -> MIDI CC 10 (`0` left, `64` centre, `127` right).
fn pan_to_cc10(pan: f32) -> u8 {
    let p = pan.clamp(-1.0, 1.0);
    if p < 0.0 {
        (64.0 + p * 64.0).round().clamp(0.0, 64.0) as u8
    } else {
        (64.0 + p * 63.0).round().clamp(64.0, 127.0) as u8
    }
}

/// Effective `(volume, pan)` of `inst` through its channel strip, mixer insert,
/// Master, and an optional per-note pan offset. Computing pan on unit gains keeps
/// pan intact even when a volume fader or fade-in/out reaches zero.
fn effective_mix(song: &Song, inst: &Instrument, auto: &[ValueAt], note_pan: f32) -> (f32, f32) {
    let c_vol =
        automation::find(auto, Target::ChannelVolume { inst: inst.id }).unwrap_or(inst.volume);
    let c_pan = automation::find(auto, Target::ChannelPan { inst: inst.id }).unwrap_or(inst.pan);
    let mut vol = c_vol.max(0.0);
    let mut pan_g = balance_gains(1.0, c_pan);
    if inst.mixer > 0
        && let Some(track) = song.mixer.get(inst.mixer as usize)
    {
        let m_vol = automation::find(auto, Target::MixerVolume { track: inst.mixer })
            .unwrap_or(track.volume);
        let m_pan =
            automation::find(auto, Target::MixerPan { track: inst.mixer }).unwrap_or(track.pan);
        vol *= m_vol.max(0.0);
        let mg = balance_gains(1.0, m_pan);
        pan_g = [pan_g[0] * mg[0], pan_g[1] * mg[1]];
    }
    if let Some(master) = song.mixer.first() {
        let m_vol =
            automation::find(auto, Target::MixerVolume { track: 0 }).unwrap_or(master.volume);
        let m_pan = automation::find(auto, Target::MixerPan { track: 0 }).unwrap_or(master.pan);
        vol *= m_vol.max(0.0);
        let mg = balance_gains(1.0, m_pan);
        pan_g = [pan_g[0] * mg[0], pan_g[1] * mg[1]];
    }
    if note_pan.abs() > 1e-4 {
        let np = balance_gains(1.0, note_pan);
        pan_g = [pan_g[0] * np[0], pan_g[1] * np[1]];
    }
    let (attenuation, pan) = gains_to_vol_pan(pan_g);
    (vol * attenuation, pan)
}

fn target_affects_inst_mix(target: Target, inst: &Instrument) -> bool {
    match target {
        Target::ChannelVolume { inst: id } | Target::ChannelPan { inst: id } => id == inst.id,
        Target::MixerVolume { track } | Target::MixerPan { track } => {
            track == 0 || (inst.mixer > 0 && track == inst.mixer)
        }
        _ => false,
    }
}

/// Highest sounding note pitch in `p` on the slide's instrument and group at `slide.start`.
fn slide_semitones(p: &Pattern, slide: &crate::sequence::Glide) -> Option<f64> {
    let highest = p
        .notes
        .iter()
        .filter(|n| {
            !n.muted
                && n.instrument == slide.instrument
                && n.group == slide.group
                && n.start <= slide.start
                && n.end() > slide.start
        })
        .map(|n| n.pitch)
        .max()?;
    Some(slide.target as f64 - highest as f64)
}

/// Notes of one pattern, filtered the way the engine plays them.
fn pattern_notes(song: &Song, pattern: u64) -> Vec<OutNote> {
    let Some(p) = song.pattern(pattern) else {
        return vec![];
    };
    let mut out = vec![];
    for (inst, _) in song.instruments.iter().enumerate() {
        if song.instruments[inst].muted {
            continue;
        }
        for n in p.notes.iter().filter(|n| !n.muted) {
            let Some(ni) = song.instruments.iter().position(|i| i.id == n.instrument) else {
                continue;
            };
            if ni != inst || n.length <= 0 || n.start < 0 {
                continue;
            }
            out.push(OutNote {
                inst,
                channel: midi_channel(song, inst),
                start: n.start as u64,
                end: (n.start + n.length) as u64,
                pitch: export_pitch(song, inst, n.pitch),
                vel: n.velocity.clamp(1, 127),
                pan: n.pan,
                fine: n.fine,
                mod_x: n.mod_x,
                mod_y: n.mod_y,
            });
        }
    }
    out
}

/// Slides of one pattern, filtered the way the engine plays them.
fn pattern_slides(song: &Song, pattern: u64) -> Vec<OutSlide> {
    let Some(p) = song.pattern(pattern) else {
        return vec![];
    };
    let mut out = vec![];
    for s in &p.slides {
        if s.start < 0 {
            continue;
        }
        let Some(inst) = song.instruments.iter().position(|i| i.id == s.instrument) else {
            continue;
        };
        if song.instruments[inst].muted
            || matches!(song.instruments[inst].kind, InstrumentKind::Drum { .. })
        {
            continue;
        }
        let Some(semitones) = slide_semitones(p, s) else {
            continue;
        };
        out.push(OutSlide {
            inst,
            start: s.start as u64,
            end: (s.start + s.length.max(0)) as u64,
            semitones,
        });
    }
    out
}

/// Pattern notes flattened through every sounding clip: the window
/// `offset .. offset + len` of the (repeating) pattern at `clip.start`.
fn song_notes(song: &Song) -> Vec<OutNote> {
    let mut out = vec![];
    for c in &song.clips {
        if c.muted || song.track_muted(c.track) {
            continue;
        }
        let Some(p) = song.pattern(c.pattern) else {
            continue;
        };
        let plen = p.length.max(1);
        let window = (c.offset, c.offset + song.clip_len(c));
        if window.1 <= window.0 {
            continue;
        }
        let first = window.0.div_euclid(plen);
        let last = (window.1 - 1).div_euclid(plen);
        for (inst, _) in song.instruments.iter().enumerate() {
            if song.instruments[inst].muted {
                continue;
            }
            for n in p.notes.iter().filter(|n| !n.muted) {
                let Some(ni) = song.instruments.iter().position(|i| i.id == n.instrument) else {
                    continue;
                };
                if ni != inst || n.length <= 0 || n.start < 0 {
                    continue;
                }
                for k in first..=last {
                    let t = n.start + k * plen;
                    if t < window.0 || t >= window.1 {
                        continue;
                    }
                    let abs = c.start + (t - window.0);
                    if abs < 0 {
                        continue;
                    }
                    out.push(OutNote {
                        inst,
                        channel: midi_channel(song, inst),
                        start: abs as u64,
                        end: (abs + n.length) as u64,
                        pitch: export_pitch(song, inst, n.pitch),
                        vel: n.velocity.clamp(1, 127),
                        pan: n.pan,
                        fine: n.fine,
                        mod_x: n.mod_x,
                        mod_y: n.mod_y,
                    });
                }
            }
        }
    }
    out
}

/// Pattern slides flattened through every sounding clip, matching `song_notes`.
fn song_slides(song: &Song) -> Vec<OutSlide> {
    let mut out = vec![];
    for c in &song.clips {
        if c.muted || song.track_muted(c.track) {
            continue;
        }
        let Some(p) = song.pattern(c.pattern) else {
            continue;
        };
        if p.slides.is_empty() {
            continue;
        }
        let plen = p.length.max(1);
        let window = (c.offset, c.offset + song.clip_len(c));
        if window.1 <= window.0 {
            continue;
        }
        let first = window.0.div_euclid(plen);
        let last = (window.1 - 1).div_euclid(plen);
        for s in &p.slides {
            if s.start < 0 {
                continue;
            }
            let Some(inst) = song.instruments.iter().position(|i| i.id == s.instrument) else {
                continue;
            };
            if song.instruments[inst].muted
                || matches!(song.instruments[inst].kind, InstrumentKind::Drum { .. })
            {
                continue;
            }
            let Some(semitones) = slide_semitones(p, s) else {
                continue;
            };
            for k in first..=last {
                let t = s.start + k * plen;
                if t < window.0 || t >= window.1 {
                    continue;
                }
                let abs = c.start + (t - window.0);
                if abs < 0 {
                    continue;
                }
                out.push(OutSlide {
                    inst,
                    start: abs as u64,
                    end: (abs + s.length.max(0)) as u64,
                    semitones,
                });
            }
        }
    }
    out
}

/// Same key overlapping itself on one channel would read back as a broken
/// pair, so the earlier note ends where the later one starts.
fn repair_overlaps(notes: &mut [OutNote]) {
    let mut by_key: BTreeMap<(u8, u8), Vec<usize>> = BTreeMap::new();
    for (i, n) in notes.iter().enumerate() {
        by_key.entry((n.channel, n.pitch)).or_default().push(i);
    }
    for group in by_key.values() {
        let mut order = group.clone();
        order.sort_by_key(|&i| (notes[i].start, notes[i].end));
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            if notes[b].start < notes[a].end {
                notes[a].end = notes[b].start;
            }
        }
    }
    for n in notes.iter_mut() {
        if n.end <= n.start {
            n.end = n.start + 1;
        }
    }
}

fn push_track_name<'a>(events: &mut Vec<(u64, TrackEventKind<'a>)>, name: &'a [u8]) {
    events.push((0, TrackEventKind::Meta(MetaMessage::TrackName(name))));
}

fn tempo_micros(bpm: f64) -> u32 {
    (60_000_000.0 / bpm.clamp(40.0, 300.0)).round() as u32
}

/// Conductor tempo + time-signature metas. Flat songs keep the single tempo
/// and 4/4 header exactly as before (byte-identical output); mapped songs get
/// one Set Tempo per point and one Time Signature per meter change. MIDI has
/// no ramp meta, so Linear ramps are approximated by tempo events every 1/16
/// note (STEP ticks) interpolated along the ramp.
fn push_conductor_tempo_meters<'a>(conductor: &mut Vec<(u64, TrackEventKind<'a>)>, song: &Song) {
    if song.tempo_map.is_empty() {
        conductor.push((
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_micros(song.tempo)))),
        ));
        conductor.push((
            0,
            TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
        ));
        return;
    }
    let mut points = song.tempo_map.points.clone();
    points.sort_by_key(|p| p.tick);
    if points.first().is_none_or(|p| p.tick != 0) {
        conductor.push((
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_micros(song.tempo)))),
        ));
    }
    for (i, p) in points.iter().enumerate() {
        let tick = p.tick.max(0) as u64;
        conductor.push((
            tick,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_micros(p.bpm)))),
        ));
        // Intermediate steps along a Linear ramp to the next point.
        if p.shape == crate::tempo::TempoShape::Linear
            && let Some(q) = points.get(i + 1)
        {
            let (t0, t1) = (p.tick.max(0), q.tick.max(0));
            let span = t1 - t0;
            if span > 0 {
                let mut t = t0 + STEP;
                while t < t1 {
                    let f = (t - t0) as f64 / span as f64;
                    let bpm = p.bpm + (q.bpm - p.bpm) * f;
                    conductor.push((
                        t as u64,
                        TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_micros(bpm)))),
                    ));
                    t += STEP;
                }
            }
        }
    }
    let mut meters = song.meters.clone();
    meters.sort_by_key(|m| m.bar);
    if meters.is_empty() {
        conductor.push((
            0,
            TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
        ));
        return;
    }
    for m in &meters {
        if !matches!(m.den, 1 | 2 | 4 | 8 | 16) {
            continue;
        }
        let tick = crate::tempo::bar_start(&meters, m.bar).max(0) as u64;
        conductor.push((
            tick,
            TrackEventKind::Meta(MetaMessage::TimeSignature(
                m.num,
                m.den.trailing_zeros() as u8,
                24,
                8,
            )),
        ));
    }
}

fn cc_ev<'a>(channel: u4, cc: u8, val: u8) -> TrackEventKind<'a> {
    TrackEventKind::Midi {
        channel,
        message: MidiMessage::Controller {
            controller: u7::new(cc),
            value: u7::new(val.min(127)),
        },
    }
}

/// Emit CC 7 (volume) and CC 10 (pan) for `inst`, including song-mode automation
/// clips (channel, mixer insert, Master) and per-note pan offsets.
fn push_mix_controllers<'a>(
    evs: &mut Vec<(u64, TrackEventKind<'a>)>,
    song: &Song,
    inst: &Instrument,
    channel: u4,
    track_notes: &[&OutNote],
    song_mode: bool,
) {
    let track_end = track_notes.iter().map(|n| n.end).max().unwrap_or(0);
    let has_note_pan = track_notes.iter().any(|n| n.pan.abs() > 1e-4);

    let mut sample_ticks: Vec<u64> = vec![0];
    let mut has_mix_auto = false;

    if song_mode {
        for clip in song
            .clips
            .iter()
            .filter(|c| !c.muted && !song.track_muted(c.track))
        {
            let Some(p) = song.pattern(clip.pattern) else {
                continue;
            };
            let Some(env) = &p.automation else { continue };
            if !target_affects_inst_mix(env.target, inst) {
                continue;
            }
            let len = clip.length.unwrap_or(p.length - clip.offset).max(1);
            let start = clip.start.max(0) as u64;
            if start > track_end {
                continue;
            }
            has_mix_auto = true;
            let end = (clip.start + len).max(0).min(track_end as Tick) as u64;
            sample_ticks.push(start);
            sample_ticks.push(end);
            let mut t = start + CC_STEP;
            while t < end {
                sample_ticks.push(t);
                t += CC_STEP;
            }
            // Exact envelope vertices inside the clip window preserve sharp corners and Events steps.
            let plen = p.length.max(1);
            let window = (clip.offset, clip.offset + len);
            let first = window.0.div_euclid(plen);
            let last = (window.1 - 1).div_euclid(plen);
            for pt in &env.points {
                for k in first..=last {
                    let local = pt.tick + k * plen;
                    if local < window.0 || local >= window.1 {
                        continue;
                    }
                    let abs = clip.start + (local - window.0);
                    if abs >= 0 && (abs as u64) <= track_end {
                        sample_ticks.push(abs as u64);
                    }
                }
            }
        }
    }

    if has_note_pan {
        for n in track_notes {
            sample_ticks.push(n.start);
        }
    }

    sample_ticks.sort_unstable();
    sample_ticks.dedup();

    let mut note_idx = 0usize;
    let mut active_note_pan = 0.0f32;
    let mut auto_vals: Vec<ValueAt> = Vec::new();
    let mut last_cc7: Option<u8> = None;
    let mut last_cc10: Option<u8> = None;

    for t in sample_ticks {
        if has_note_pan {
            while note_idx < track_notes.len() && track_notes[note_idx].start <= t {
                active_note_pan = track_notes[note_idx].pan;
                note_idx += 1;
            }
        }
        if has_mix_auto {
            automation::values_at(song, t as Tick, &mut auto_vals);
        }
        let note_pan = if has_note_pan { active_note_pan } else { 0.0 };
        let (vol, pan) = effective_mix(song, inst, &auto_vals, note_pan);
        let cc7 = gain_to_cc7(vol);
        let cc10 = pan_to_cc10(pan);
        if t == 0 || last_cc7 != Some(cc7) {
            evs.push((t, cc_ev(channel, 7, cc7)));
            last_cc7 = Some(cc7);
        }
        if t == 0 || last_cc10 != Some(cc10) {
            evs.push((t, cc_ev(channel, 10, cc10)));
            last_cc10 = Some(cc10);
        }
    }
}

/// Emit per-note Mod X (CC 74 Brightness) and Mod Y (CC 71 Resonance) when non-default.
fn push_note_mod_controllers<'a>(
    evs: &mut Vec<(u64, TrackEventKind<'a>)>,
    channel: u4,
    track_notes: &[&OutNote],
) {
    let has_mod_x = track_notes.iter().any(|n| (n.mod_x - 0.5).abs() > 1e-4);
    let has_mod_y = track_notes.iter().any(|n| (n.mod_y - 0.5).abs() > 1e-4);
    if !has_mod_x && !has_mod_y {
        return;
    }
    let mut last_x: Option<u8> = None;
    let mut last_y: Option<u8> = None;
    for n in track_notes {
        if has_mod_x {
            let v = (n.mod_x.clamp(0.0, 1.0) * 127.0).round().clamp(0.0, 127.0) as u8;
            if last_x != Some(v) {
                evs.push((n.start, cc_ev(channel, 74, v)));
                last_x = Some(v);
            }
        }
        if has_mod_y {
            let v = (n.mod_y.clamp(0.0, 1.0) * 127.0).round().clamp(0.0, 127.0) as u8;
            if last_y != Some(v) {
                evs.push((n.start, cc_ev(channel, 71, v)));
                last_y = Some(v);
            }
        }
    }
}

/// Emit PitchBend events (and RPN 0,0 Pitch Bend Sensitivity when > 2 semitones)
/// for piano-roll slides and per-note fine pitch.
fn push_pitch_bends<'a>(
    evs: &mut Vec<(u64, TrackEventKind<'a>)>,
    channel: u4,
    track_notes: &[&OutNote],
    track_slides: &[&OutSlide],
) {
    let has_fine = track_notes.iter().any(|n| n.fine.abs() > 1e-3);
    if track_slides.is_empty() && !has_fine {
        return;
    }

    // Group notes by start tick so chords share one pitch window.
    let mut starts: Vec<(u64, u64, f64)> = Vec::new();
    for n in track_notes {
        let fine_semi = n.fine as f64 / 100.0;
        if let Some(last) = starts.last_mut()
            && last.0 == n.start
        {
            last.1 = last.1.max(n.end);
            if fine_semi.abs() > last.2.abs() {
                last.2 = fine_semi;
            }
        } else {
            starts.push((n.start, n.end, fine_semi));
        }
    }

    let mut points: Vec<(u64, f64)> = Vec::new();
    let mut active_end = 0u64;
    for (idx, &(t_on, t_off, fine_semi)) in starts.iter().enumerate() {
        active_end = active_end.max(t_off);
        let next_on = starts.get(idx + 1).map_or(u64::MAX, |s| s.0);
        let window_end = active_end.min(next_on);
        let mut cur = fine_semi;
        points.push((t_on, cur));

        let active_slides: Vec<&OutSlide> = track_slides
            .iter()
            .copied()
            .filter(|s| s.start >= t_on && s.start < window_end)
            .collect();

        for (si, s) in active_slides.iter().enumerate() {
            let next_slide = active_slides.get(si + 1).map_or(u64::MAX, |n| n.start);
            let s_end = s.end.min(next_slide).min(window_end);
            if s.end <= s.start {
                cur = s.semitones;
                points.push((s.start, cur));
                continue;
            }
            let from = cur;
            let span = (s.end - s.start) as f64;
            let mut t = s.start;
            while t < s_end {
                let frac = ((t - s.start) as f64 / span).clamp(0.0, 1.0);
                points.push((t, from + (s.semitones - from) * frac));
                t = t.saturating_add(CC_STEP);
            }
            let end_frac = ((s_end - s.start) as f64 / span).clamp(0.0, 1.0);
            cur = from + (s.semitones - from) * end_frac;
            points.push((s_end, cur));
        }
    }

    let max_semi = points.iter().map(|(_, s)| s.abs()).fold(0.0f64, f64::max);
    if max_semi <= 1e-4 {
        return;
    }
    let range: u8 = if max_semi <= 2.0 {
        2
    } else {
        max_semi.ceil().clamp(2.0, 127.0) as u8
    };
    if range != 2 {
        // RPN 0,0: Pitch Bend Sensitivity in semitones, then RPN Null.
        evs.push((0, cc_ev(channel, 101, 0)));
        evs.push((0, cc_ev(channel, 100, 0)));
        evs.push((0, cc_ev(channel, 6, range)));
        evs.push((0, cc_ev(channel, 38, 0)));
        evs.push((0, cc_ev(channel, 101, 127)));
        evs.push((0, cc_ev(channel, 100, 127)));
    }

    let mut last_raw: u16 = 8192;
    for (t, semi) in points {
        let norm = (semi / range as f64).clamp(-1.0, 1.0);
        let raw = if norm >= 0.0 {
            8192.0 + norm * 8191.0
        } else {
            8192.0 + norm * 8192.0
        }
        .round()
        .clamp(0.0, 16383.0) as u16;
        if raw != last_raw {
            evs.push((
                t,
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::PitchBend {
                        bend: PitchBend(u14::new(raw)),
                    },
                },
            ));
            last_raw = raw;
        }
    }
}

pub fn export_smf(song: &Song, source: &MidiSource, opts: &ExportOptions) -> Vec<u8> {
    let song_mode = matches!(source, MidiSource::Song);
    let mut notes = match source {
        MidiSource::Pattern(id) => pattern_notes(song, *id),
        MidiSource::Song => song_notes(song),
    };
    let mut slides = match source {
        MidiSource::Pattern(id) => pattern_slides(song, *id),
        MidiSource::Song => song_slides(song),
    };
    repair_overlaps(&mut notes);
    slides.sort_by_key(|s| (s.start, s.end));

    // Owned text must outlive the Smf that borrows it.
    let mut owned: Vec<Vec<u8>> = vec![CONDUCTOR_NAME.as_bytes().to_vec()];
    let mut track_names: Vec<usize> = vec![];
    for inst in &song.instruments {
        track_names.push(owned.len());
        owned.push(inst.name.clone().into_bytes());
    }
    if !opts.one_track_per_channel {
        track_names.push(owned.len());
        owned.push(CONDUCTOR_NAME.as_bytes().to_vec());
    }
    let merged_name = owned.len() - 1;
    let mut marker_entries: Vec<(u64, usize)> = vec![];
    if song_mode {
        let mut markers = song.markers.clone();
        markers.sort_by_key(|m| m.tick);
        for m in markers {
            if m.tick < 0 || m.name.is_empty() {
                continue;
            }
            let idx = owned.len();
            owned.push(m.name.into_bytes());
            marker_entries.push((m.tick as u64, idx));
        }
    }

    // Per-track event lists as (absolute tick, event), delta-encoded later.
    let mut conductor: Vec<(u64, TrackEventKind)> = vec![];
    push_track_name(&mut conductor, &owned[0]);
    push_conductor_tempo_meters(&mut conductor, song);
    for (tick, idx) in marker_entries {
        conductor.push((tick, TrackEventKind::Meta(MetaMessage::Marker(&owned[idx]))));
    }

    // Track per instrument that has notes, in instrument order.
    let mut per_track: Vec<Vec<(u64, TrackEventKind)>> = vec![Vec::new(); song.instruments.len()];
    let mut notes_by_inst: Vec<Vec<&OutNote>> = vec![Vec::new(); song.instruments.len()];
    let mut slides_by_inst: Vec<Vec<&OutSlide>> = vec![Vec::new(); song.instruments.len()];
    for n in &notes {
        notes_by_inst[n.inst].push(n);
    }
    for list in &mut notes_by_inst {
        list.sort_by_key(|n| (n.start, n.end));
    }
    for s in &slides {
        slides_by_inst[s.inst].push(s);
    }

    let mut merged: Vec<(u64, TrackEventKind)> = vec![];
    let mut merged_channels_setup = [false; 16];
    if !opts.one_track_per_channel {
        push_track_name(&mut merged, &owned[merged_name]);
    }

    for (i, inst) in song.instruments.iter().enumerate() {
        let inst_notes = &notes_by_inst[i];
        if inst_notes.is_empty() {
            continue;
        }
        let ch_num = midi_channel(song, i);
        let channel = u4::new(ch_num);
        let target_evs = if opts.one_track_per_channel {
            &mut per_track[i]
        } else {
            &mut merged
        };
        let emit_channel_setup = if opts.one_track_per_channel {
            true
        } else if !merged_channels_setup[ch_num as usize] {
            merged_channels_setup[ch_num as usize] = true;
            true
        } else {
            false
        };
        if emit_channel_setup {
            push_mix_controllers(target_evs, song, inst, channel, inst_notes, song_mode);
            push_note_mod_controllers(target_evs, channel, inst_notes);
            push_pitch_bends(target_evs, channel, inst_notes, &slides_by_inst[i]);
        }
        for n in inst_notes {
            target_evs.push((
                n.start,
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOn {
                        key: u7::new(n.pitch),
                        vel: u7::new(n.vel),
                    },
                },
            ));
            target_evs.push((
                n.end,
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOff {
                        key: u7::new(n.pitch),
                        vel: u7::new(64),
                    },
                },
            ));
        }
    }

    let mut smf = Smf::new(Header::new(
        Format::Parallel,
        Timing::Metrical(u15::new(PPQ as u16)),
    ));
    fn finish<'a>(evs: &mut Vec<(u64, TrackEventKind<'a>)>) -> midly::Track<'a> {
        evs.sort_by_key(|(t, k)| {
            let order: u8 = match k {
                TrackEventKind::Meta(_) => 0,
                TrackEventKind::Midi {
                    message: MidiMessage::NoteOff { .. },
                    ..
                } => 1,
                TrackEventKind::Midi {
                    message: MidiMessage::ProgramChange { .. } | MidiMessage::Controller { .. },
                    ..
                } => 2,
                TrackEventKind::Midi {
                    message: MidiMessage::PitchBend { .. },
                    ..
                } => 3,
                TrackEventKind::Midi {
                    message: MidiMessage::NoteOn { .. },
                    ..
                } => 4,
                _ => 2,
            };
            (*t, order)
        });
        let mut track = midly::Track::new();
        let mut prev = 0u64;
        for (t, kind) in evs.drain(..) {
            track.push(TrackEvent {
                delta: u28::new((t - prev) as u32),
                kind,
            });
            prev = t;
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        track
    }
    smf.tracks.push(finish(&mut conductor));
    if !opts.one_track_per_channel {
        if !notes.is_empty() {
            smf.tracks.push(finish(&mut merged));
        }
    } else {
        for (i, evs) in per_track.iter_mut().enumerate() {
            if notes_by_inst[i].is_empty() {
                continue;
            }
            let mut evs2: Vec<(u64, TrackEventKind)> = vec![];
            push_track_name(&mut evs2, &owned[track_names[i]]);
            evs2.append(evs);
            smf.tracks.push(finish(&mut evs2));
        }
    }
    let mut out = Vec::new();
    smf.write_std(&mut out).expect("writing SMF to memory");
    out
}

// --- import ---------------------------------------------------------------

/// General MIDI program names for channel labels.
const GM_PROGRAMS: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bag pipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

const DRUM_NAMES: [&str; 3] = ["Kick (MIDI)", "Snare (MIDI)", "Hat (MIDI)"];

/// GM drum key to drum-voice index. Unlisted keys fall back to the nearest
/// of kick 36 / snare 38 / hat 42, ties going to the lower voice.
fn drum_voice(key: u8) -> usize {
    match key {
        35 | 36 => 0,
        37..=40 => 1,
        42 | 44 | 46 | 49 | 51 | 52 | 53 | 55 | 57 | 59 => 2,
        _ => {
            let d36 = (36i16 - key as i16).abs();
            let d38 = (38i16 - key as i16).abs();
            let d42 = (42i16 - key as i16).abs();
            if d36 <= d38 && d36 <= d42 {
                0
            } else if d38 <= d42 {
                1
            } else {
                2
            }
        }
    }
}

struct DoneNote {
    start: Tick,
    end: Tick,
    pitch: u8,
    vel: u8,
}

struct GroupNotes {
    notes: Vec<DoneNote>,
    program: Option<u8>,
    /// First CC7 (volume) and CC10 (pan) on this channel: the track's mix.
    volume: Option<u8>,
    pan: Option<u8>,
}

fn scale_tick(t: u64, ppq: u64) -> Tick {
    ((t * PPQ as u64 + ppq / 2) / ppq).min(i64::MAX as u64) as Tick
}

fn decode_name(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(bytes).into_owned();
    if s.is_empty() { None } else { Some(s) }
}

pub fn import_smf(
    bytes: &[u8],
    song: &mut Song,
    opts: &ImportOptions,
) -> Result<ImportReport, String> {
    let smf = Smf::parse(bytes).map_err(|e| format!("Not a MIDI file: {e}"))?;
    let ppq_src = match smf.header.timing {
        Timing::Metrical(p) => p.as_int() as u64,
        Timing::Timecode(..) => return Err("SMPTE-timing MIDI not supported".to_string()),
    };
    if ppq_src == 0 {
        return Err("MIDI file has no ticks per beat".to_string());
    }

    // First pass over the borrowed events: scaled notes per (track, channel).
    let mut groups: BTreeMap<(usize, u8), GroupNotes> = BTreeMap::new();
    let mut track_names: Vec<(Option<String>, Option<String>)> = vec![];
    let mut tempos: Vec<(u64, u32)> = vec![];
    let mut sigs: Vec<(u64, u8, u8)> = vec![];
    let mut pitch_bends = 0usize;
    let mut controllers = 0usize;
    let mut repaired = 0usize;

    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut abs: u64 = 0;
        let mut opens: BTreeMap<(u8, u8), VecDeque<(u64, u8)>> = BTreeMap::new();
        let mut tname: Option<String> = None;
        let mut iname: Option<String> = None;
        let mut programs: BTreeMap<u8, u8> = BTreeMap::new();
        let mut mix: BTreeMap<u8, (Option<u8>, Option<u8>)> = BTreeMap::new();
        let mut rpn: BTreeMap<u8, (u8, u8)> = BTreeMap::new();
        let mut done: Vec<(usize, u8, DoneNote)> = vec![];
        // Pop the oldest open note on a key and end it at `end`.
        // Returns the finished note plus whether it needed a repair.
        let shut = |q: &mut VecDeque<(u64, u8)>,
                    key: u8,
                    end: u64,
                    unclosed: bool|
         -> Option<(DoneNote, bool)> {
            let (start, vel) = q.pop_front()?;
            let s = scale_tick(start, ppq_src);
            let raw = scale_tick(end, ppq_src);
            let fixed = raw <= s;
            let e = if fixed { s + 1 } else { raw };
            Some((
                DoneNote {
                    start: s,
                    end: e,
                    pitch: key,
                    vel,
                },
                unclosed || fixed,
            ))
        };
        for ev in track {
            abs += ev.delta.as_int() as u64;
            match &ev.kind {
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match *message {
                        MidiMessage::NoteOn { key, vel } => {
                            let (key, vel) = (key.as_int(), vel.as_int());
                            // A new opener ends any ringing note on the same key first (FIFO).
                            if let Some(mut q) = opens.remove(&(ch, key)) {
                                while let Some((dn, fix)) = shut(&mut q, key, abs, false) {
                                    repaired += fix as usize;
                                    done.push((ti, ch, dn));
                                }
                            }
                            if vel == 0 {
                                continue;
                            }
                            opens.entry((ch, key)).or_default().push_back((abs, vel));
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            let key = key.as_int();
                            let mut finished: Option<(usize, u8, DoneNote, bool)> = None;
                            if let Some(q) = opens.get_mut(&(ch, key)) {
                                if let Some((dn, fix)) = shut(q, key, abs, false) {
                                    finished = Some((ti, ch, dn, fix));
                                }
                                let empty = q.is_empty();
                                if empty {
                                    opens.remove(&(ch, key));
                                }
                            }
                            if let Some((ti, ch, dn, fix)) = finished {
                                repaired += fix as usize;
                                done.push((ti, ch, dn));
                            }
                        }
                        MidiMessage::PitchBend { .. } => pitch_bends += 1,
                        // CC7 volume and CC10 pan set the channel's mix (first value
                        // wins, FL does the same); RPN 0,0 (pitch bend range) is
                        // recognised as setup; other controllers are counted.
                        MidiMessage::Controller { controller, value }
                            if controller.as_int() == 7 =>
                        {
                            mix.entry(ch).or_default().0.get_or_insert(value.as_int());
                        }
                        MidiMessage::Controller { controller, value }
                            if controller.as_int() == 10 =>
                        {
                            mix.entry(ch).or_default().1.get_or_insert(value.as_int());
                        }
                        MidiMessage::Controller { controller, value }
                            if controller.as_int() == 101 =>
                        {
                            rpn.entry(ch).or_insert((127, 127)).0 = value.as_int();
                        }
                        MidiMessage::Controller { controller, value }
                            if controller.as_int() == 100 =>
                        {
                            rpn.entry(ch).or_insert((127, 127)).1 = value.as_int();
                        }
                        MidiMessage::Controller { controller, .. }
                            if matches!(controller.as_int(), 6 | 38)
                                && rpn.get(&ch) == Some(&(0, 0)) => {}
                        MidiMessage::Controller { .. } => controllers += 1,
                        MidiMessage::ProgramChange { program } => {
                            programs.entry(ch).or_insert(program.as_int());
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(meta) => match meta {
                    MetaMessage::Tempo(mpqn) => {
                        let us = mpqn.as_int();
                        if us > 0 {
                            tempos.push((abs, us));
                        }
                    }
                    MetaMessage::TimeSignature(num, den_e, _, _) => {
                        sigs.push((abs, *num, *den_e));
                    }
                    MetaMessage::TrackName(b) => {
                        if tname.is_none() {
                            tname = decode_name(b);
                        }
                    }
                    MetaMessage::InstrumentName(b) if iname.is_none() => {
                        iname = decode_name(b);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let track_end = abs;
        // Anything still ringing stops at the end of its track.
        for ((ch, key), mut q) in opens {
            while let Some((dn, _)) = shut(&mut q, key, track_end, true) {
                repaired += 1;
                done.push((ti, ch, dn));
            }
        }
        for (ti2, ch, n) in done {
            groups
                .entry((ti2, ch))
                .or_insert(GroupNotes {
                    notes: vec![],
                    program: None,
                    volume: None,
                    pan: None,
                })
                .notes
                .push(n);
        }
        for (ch, (volume, pan)) in mix {
            let g = groups.entry((ti, ch)).or_insert(GroupNotes {
                notes: vec![],
                program: None,
                volume: None,
                pan: None,
            });
            g.volume = volume;
            g.pan = pan;
        }
        for (ch, program) in programs {
            groups
                .entry((ti, ch))
                .or_insert(GroupNotes {
                    notes: vec![],
                    program: None,
                    volume: None,
                    pan: None,
                })
                .program = Some(program);
        }
        track_names.push((tname, iname));
    }

    tempos.sort();
    sigs.sort();
    let tempo_events = tempos.len();
    let meter_events = sigs.len();
    // Every tempo event becomes a Step point (a file cannot mark ramps, so
    // an exported ramp reads back as its 1/16-note steps).
    let mut tempo_map = crate::tempo::TempoMap::default();
    for &(abs, us) in &tempos {
        let tick = scale_tick(abs, ppq_src);
        if tick < 0 {
            continue;
        }
        let bpm = (60_000_000.0 / us as f64).clamp(40.0, 300.0);
        if tempo_map.points.last().is_some_and(|p| p.tick == tick) {
            *tempo_map.points.last_mut().expect("checked") = crate::tempo::TempoPoint {
                tick,
                bpm,
                shape: crate::tempo::TempoShape::Step,
            };
        } else {
            tempo_map.points.push(crate::tempo::TempoPoint {
                tick,
                bpm,
                shape: crate::tempo::TempoShape::Step,
            });
        }
    }
    let tempo = tempo_map.points.first().map(|p| p.bpm);
    // One event, or all events at one bpm, is just the song tempo: no map.
    if tempo_map.points.len() <= 1
        || tempo_map
            .points
            .iter()
            .all(|p| p.bpm == tempo_map.points[0].bpm)
    {
        tempo_map.points.clear();
    } else {
        crate::tempo::normalize_tempo_map(&mut tempo_map, tempo.unwrap_or(120.0));
    }
    // Time signatures need bar indices: walk bars under the sigs so far.
    let mut meters: Vec<crate::tempo::Meter> = vec![];
    for &(abs, num, den_e) in &sigs {
        if !(1..=16).contains(&num) || den_e > 4 {
            continue;
        }
        let (num, den) = (num, 1u8 << den_e);
        if crate::tempo::bar_len_ticks(num, den) < STEP {
            continue;
        }
        let tick = scale_tick(abs, ppq_src).max(0);
        let mut bar = 0u32;
        while crate::tempo::bar_start(&meters, bar + 1) <= tick {
            bar += 1;
        }
        if meters.last().is_some_and(|m| m.bar == bar) {
            *meters.last_mut().expect("checked") = crate::tempo::Meter { bar, num, den };
        } else {
            meters.push(crate::tempo::Meter { bar, num, den });
        }
    }
    crate::tempo::normalize_meters(&mut meters);
    // All 4/4 is the default: no meters (old behaviour).
    if meters.iter().all(|m| m.num == 4 && m.den == 4) {
        meters.clear();
    }

    // Build on a clone so failures leave the caller's song untouched.
    let mut work = song.clone();

    let max_end = groups
        .values()
        .flat_map(|g| g.notes.iter().map(|n| n.end))
        .max();
    let length = max_end
        .map(|e| ((e + BAR - 1) / BAR) * BAR)
        .unwrap_or(BAR)
        .max(BAR);
    let pid = work.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    let mut n = 1u32;
    let pname = loop {
        let cand = format!("MIDI {n}");
        if !work.patterns.iter().any(|p| p.name == cand) {
            break cand;
        }
        n += 1;
    };
    work.patterns.push(
        crate::sequence::Pattern {
            id: pid,
            name: pname,
            length,
            notes: vec![],
            slides: vec![],
            automation: None,
            audio_clip: None,
            color: None,
            loop_range: None,
            lyrics: Default::default(),
            pitch: Default::default(),
        }
        .into(),
    );

    let group_name = |ti: usize, ch: u8, program: Option<u8>| -> String {
        let (tname, iname) = &track_names[ti];
        if let Some(s) = tname.as_ref().or(iname.as_ref()) {
            return s.clone();
        }
        if let Some(p) = program {
            return GM_PROGRAMS[p as usize].to_string();
        }
        format!("MIDI Ch {}", ch + 1)
    };

    let mut report = ImportReport {
        pattern: pid,
        channels: vec![],
        notes: 0,
        tracks: smf.tracks.len(),
        tempo,
        tempos: tempo_events,
        meters: meter_events,
        ignored_pitch_bends: pitch_bends,
        ignored_controllers: controllers,
        repaired_notes: repaired,
        names: vec![],
    };

    if let Some(t) = tempo {
        work.tempo = t;
    }
    work.tempo_map = tempo_map;
    work.meters = meters;

    let add_notes =
        |work: &mut Song, inst: u64, notes: Vec<DoneNote>, report: &mut ImportReport| {
            for dn in notes {
                let len = (dn.end - dn.start).max(1);
                work.add_note(pid, Note::new(inst, dn.start, len, dn.pitch, dn.vel));
                report.notes += 1;
            }
        };

    if opts.one_channel_per_track {
        for ((ti, ch), g) in groups {
            if g.notes.is_empty() {
                continue;
            }
            if ch == DRUM_CHANNEL && opts.drums_on_channel_10 && opts.drum_kit {
                // One channel for the kit: every key keeps its drum (GM map).
                let name = format!("{DRUM_KIT_PREFIX}{}", gm_kit_name(g.program));
                let id = work.add_sampler(&name);
                if let Some(inst) = work.instruments.iter_mut().find(|i| i.id == id) {
                    inst.volume = cc7_to_gain(g.volume.unwrap_or(100));
                    inst.pan = g.pan.map(cc10_to_pan).unwrap_or(0.0);
                }
                report.channels.push(id);
                report.names.push(name);
                let mut notes = g.notes;
                notes.sort_by_key(|n| (n.start, n.end));
                add_notes(&mut work, id, notes, &mut report);
            } else if ch == DRUM_CHANNEL && opts.drums_on_channel_10 {
                let mut by_voice: BTreeMap<usize, Vec<DoneNote>> = BTreeMap::new();
                for dn in g.notes {
                    by_voice.entry(drum_voice(dn.pitch)).or_default().push(dn);
                }
                for (voice, mut notes) in by_voice {
                    notes.sort_by_key(|n| (n.start, n.end));
                    let id = work.add_channel(
                        DRUM_NAMES[voice],
                        InstrumentKind::Drum { voice: voice as u8 },
                    );
                    if let Some(inst) = work.instruments.iter_mut().find(|i| i.id == id) {
                        inst.volume = cc7_to_gain(g.volume.unwrap_or(100));
                        inst.pan = g.pan.map(cc10_to_pan).unwrap_or(0.0);
                    }
                    for dn in &mut notes {
                        dn.pitch = STEP_PITCH;
                    }
                    report.channels.push(id);
                    report.names.push(DRUM_NAMES[voice].to_string());
                    add_notes(&mut work, id, notes, &mut report);
                }
            } else {
                let name = group_name(ti, ch, g.program);
                let id = work.add_sampler(&name);
                if let Some(inst) = work.instruments.iter_mut().find(|i| i.id == id) {
                    // General MIDI volume law: gain = (CC7 / 127)², so 100
                    // (the value when a file sends none) is −4.2 dB.
                    inst.volume = cc7_to_gain(g.volume.unwrap_or(100));
                    inst.pan = g.pan.map(cc10_to_pan).unwrap_or(0.0);
                }
                report.channels.push(id);
                report.names.push(name);
                let mut notes = g.notes;
                notes.sort_by_key(|n| (n.start, n.end));
                add_notes(&mut work, id, notes, &mut report);
            }
        }
    } else {
        let mut all: Vec<DoneNote> = groups.into_values().flat_map(|g| g.notes).collect();
        if !all.is_empty() {
            all.sort_by_key(|n| (n.start, n.end));
            let name = track_names
                .iter()
                .flat_map(|(t, i)| [t, i])
                .flatten()
                .next()
                .cloned()
                .unwrap_or_else(|| "MIDI".to_string());
            let id = work.add_sampler(&name);
            report.channels.push(id);
            report.names.push(name);
            add_notes(&mut work, id, all, &mut report);
        }
    }

    work.validate().map_err(|e| e.to_string())?;
    *song = work;
    Ok(report)
}

fn grouped(mut n: usize) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut parts: Vec<String> = vec![];
    while n >= 1000 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    let mut s = n.to_string();
    for p in parts.iter().rev() {
        s.push(' ');
        s.push_str(p);
    }
    s
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{} {many}", grouped(n))
    }
}

/// One status-bar line about an import, e.g. "Imported 1 204 notes on
/// 3 new channels into Pattern 4 at 128 BPM; 2 tempo changes read,
/// 1 meter change read, 31 pitch bends ignored."
/// Every note of a MIDI file as one list (all tracks and channels), keys as
/// written and starting at the file's first note: for a clip dragged out of a
/// plugin (a drum groove, a riff) onto one channel. (start, length, key, velocity).
pub fn smf_notes(bytes: &[u8]) -> Result<Vec<(Tick, Tick, u8, u8)>, String> {
    let mut scratch = Song::default();
    let opts = ImportOptions {
        drums_on_channel_10: false,
        one_channel_per_track: false,
        drum_kit: false,
    };
    let report = import_smf(bytes, &mut scratch, &opts)?;
    let pattern = scratch
        .patterns
        .iter()
        .find(|p| p.id == report.pattern)
        .ok_or("MIDI file has no notes")?;
    let mut notes: Vec<(Tick, Tick, u8, u8)> = pattern
        .notes
        .iter()
        .map(|n| (n.start, n.length, n.pitch, n.velocity))
        .collect();
    notes.sort_unstable();
    // A dragged clip starts on its first bar, wherever the plugin put it in time.
    if let Some(first) = notes.first().map(|n| n.0) {
        let shift = (first / crate::sequence::BAR) * crate::sequence::BAR;
        for n in &mut notes {
            n.0 -= shift;
        }
    }
    Ok(notes)
}

pub fn summary(report: &ImportReport) -> String {
    let mut s = format!(
        "Imported {} on {} into Pattern {}",
        plural(report.notes, "note", "notes"),
        plural(report.channels.len(), "new channel", "new channels"),
        report.pattern,
    );
    if let Some(t) = report.tempo {
        let bpm = if t.fract() == 0.0 {
            format!("{}", t as i64)
        } else {
            format!("{t:.1}")
        };
        s.push_str(&format!(" at {bpm} BPM"));
    }
    s.push_str(&format!(
        "; {} read, {} read, {} ignored",
        plural(report.tempos, "tempo change", "tempo changes"),
        plural(report.meters, "meter change", "meter changes"),
        plural(report.ignored_pitch_bends, "pitch bend", "pitch bends"),
    ));
    if report.ignored_controllers > 0 {
        s.push_str(&format!(
            ", {} ignored",
            plural(report.ignored_controllers, "controller", "controllers")
        ));
    }
    if report.repaired_notes > 0 {
        s.push_str(&format!(
            ", {} repaired",
            plural(report.repaired_notes, "note", "notes")
        ));
    }
    s.push('.');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragged_clip_keeps_keys_and_starts_on_its_first_bar() {
        // A groove written from bar 3, on MIDI channel 10, like a drum plugin's export.
        let mut song = Song::default();
        let pid = song.patterns[0].id;
        let ch = song.instruments[3].id; // Bass: keys go out as they are
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        for (k, key) in [36u8, 38, 42, 46].iter().enumerate() {
            song.add_note(
                pid,
                crate::sequence::Note::new(
                    ch,
                    2 * crate::sequence::BAR + k as Tick * 240 + 120,
                    100,
                    *key,
                    90,
                ),
            );
        }
        let bytes = export_smf(&song, &MidiSource::Pattern(pid), &ExportOptions::default());
        let notes = smf_notes(&bytes).unwrap();
        assert_eq!(notes.len(), 4);
        assert_eq!(
            notes[0].0, 120,
            "moved to the first bar, offset inside the bar kept"
        );
        let keys: Vec<u8> = notes.iter().map(|n| n.2).collect();
        assert_eq!(keys, vec![36, 38, 42, 46], "keys as written");
        assert!(notes.iter().all(|n| n.3 > 0 && n.1 > 0));
        assert!(smf_notes(b"not midi").is_err());
    }
    use crate::sequence::{Clip, STEP};
    use midly::Fps;

    fn ev(delta: u32, kind: TrackEventKind<'static>) -> TrackEvent<'static> {
        TrackEvent {
            delta: u28::new(delta),
            kind,
        }
    }

    fn on(ch: u8, key: u8, vel: u8) -> TrackEventKind<'static> {
        TrackEventKind::Midi {
            channel: u4::new(ch),
            message: MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(vel),
            },
        }
    }

    fn off(ch: u8, key: u8) -> TrackEventKind<'static> {
        TrackEventKind::Midi {
            channel: u4::new(ch),
            message: MidiMessage::NoteOff {
                key: u7::new(key),
                vel: u7::new(64),
            },
        }
    }

    fn end() -> TrackEventKind<'static> {
        TrackEventKind::Meta(MetaMessage::EndOfTrack)
    }

    fn tempo_ev(mpqn: u32) -> TrackEventKind<'static> {
        TrackEventKind::Meta(MetaMessage::Tempo(u24::new(mpqn)))
    }

    fn smf_bytes(timing: Timing, tracks: Vec<midly::Track<'static>>) -> Vec<u8> {
        let mut smf = Smf::new(Header::new(Format::Parallel, timing));
        smf.tracks = tracks;
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }

    fn ppq480(tracks: Vec<midly::Track<'static>>) -> Vec<u8> {
        smf_bytes(Timing::Metrical(u15::new(480)), tracks)
    }

    /// NoteOn (tick, key) pairs of an SMF, sorted.
    fn note_ons(bytes: &[u8]) -> Vec<(u64, u8)> {
        let smf = Smf::parse(bytes).unwrap();
        let mut ons = vec![];
        for t in &smf.tracks {
            let mut abs = 0u64;
            for e in t {
                abs += e.delta.as_int() as u64;
                if let TrackEventKind::Midi {
                    message: MidiMessage::NoteOn { key, vel },
                    ..
                } = e.kind
                    && vel.as_int() > 0
                {
                    ons.push((abs, key.as_int()));
                }
            }
        }
        ons.sort();
        ons
    }

    /// Multiset of (start, length, pitch, velocity) per group key.
    fn multiset(song: &Song, pattern: u64) -> BTreeMap<String, Vec<(Tick, Tick, u8, u8)>> {
        let mut map: BTreeMap<String, Vec<(Tick, Tick, u8, u8)>> = BTreeMap::new();
        for n in &song.pattern(pattern).unwrap().notes {
            let inst = song.instrument(n.instrument).unwrap();
            let key = match inst.drum_voice() {
                Some(v) => format!("drum{v}"),
                None => inst.name.clone(),
            };
            map.entry(key)
                .or_default()
                .push((n.start, n.length, n.pitch, n.velocity));
        }
        for v in map.values_mut() {
            v.sort();
        }
        map
    }

    #[test]
    fn round_trip_demo_pattern() {
        let song = Song::demo();
        let bytes = export_smf(&song, &MidiSource::Pattern(1), &ExportOptions::default());
        let mut into = Song::default();
        let rep = import_smf(&bytes, &mut into, &ImportOptions::default()).unwrap();
        assert_eq!(rep.tracks, 5);
        assert_eq!(rep.channels.len(), 4);
        assert_eq!(
            rep.names,
            vec!["Kick (MIDI)", "Snare (MIDI)", "Hat (MIDI)", "Bass"]
        );
        assert_eq!(rep.notes, 15);
        assert_eq!(rep.tempo, Some(120.0));
        assert_eq!(rep.tempos, 1);
        assert_eq!(rep.meters, 1);
        assert!(into.tempo_map.is_empty());
        assert!(into.meters.is_empty());
        assert_eq!(rep.repaired_notes, 0);
        assert!(into.validate().is_ok());
        assert_eq!(multiset(&song, 1), multiset(&into, rep.pattern));
        // Drum steps come back as pitch-60 notes on the right voices.
        for n in &into.pattern(rep.pattern).unwrap().notes {
            if into
                .instrument(n.instrument)
                .unwrap()
                .drum_voice()
                .is_some()
            {
                assert_eq!(n.pitch, STEP_PITCH);
            }
        }
        assert_eq!(rep.ignored_controllers, 0);
        assert!(
            rep.ignored_pitch_bends > 0,
            "demo Bass slide exports as pitch bends"
        );
        assert_eq!(
            summary(&rep),
            format!(
                "Imported 15 notes on 4 new channels into Pattern 2 at 120 BPM; 1 tempo change read, 1 meter change read, {} pitch bends ignored.",
                rep.ignored_pitch_bends
            )
        );
    }

    #[test]
    fn channel_and_mixer_volume_pan_export_and_round_trip() {
        let mut song = Song::default();
        // Kick (drum): channel volume 0.5, pan -0.5.
        song.instruments[0].volume = 0.5;
        song.instruments[0].pan = -0.5;
        // Bass (melodic) routed to Insert 1: channel vol 0.8, pan 0.0; insert vol 0.5, pan +0.5.
        // Combined balance gains: L = 0.8 * 0.5 * 0.5 = 0.2, R = 0.8 * 0.5 * 1.0 = 0.4
        // -> effective vol = 0.4, pan = +0.5.
        song.instruments[3].volume = 0.8;
        song.instruments[3].pan = 0.0;
        song.instruments[3].mixer = 1;
        song.mixer[1].volume = 0.5;
        song.mixer[1].pan = 0.5;
        song.add_note(1, Note::new(1, 0, STEP, STEP_PITCH, 100));
        song.add_note(1, Note::new(4, 0, BAR, 48, 100));

        let bytes = export_smf(&song, &MidiSource::Pattern(1), &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();

        // Check CC 7 / CC 10 arrive before NoteOn at tick 0 on every track.
        for track in &smf.tracks[1..] {
            let mut seen_note = false;
            let mut seen_cc7 = false;
            let mut seen_cc10 = false;
            for ev in track {
                if let TrackEventKind::Midi { message, .. } = ev.kind {
                    match message {
                        MidiMessage::Controller { controller, .. } if controller.as_int() == 7 => {
                            assert!(!seen_note, "CC 7 must precede NoteOn");
                            seen_cc7 = true;
                        }
                        MidiMessage::Controller { controller, .. } if controller.as_int() == 10 => {
                            assert!(!seen_note, "CC 10 must precede NoteOn");
                            seen_cc10 = true;
                        }
                        MidiMessage::NoteOn { .. } => seen_note = true,
                        _ => {}
                    }
                }
            }
            assert!(seen_cc7 && seen_cc10, "each track writes CC 7 and CC 10");
        }

        // Round-trip through import_smf recovers both drum and melodic mix levels.
        let mut into = Song::default();
        let rep = import_smf(&bytes, &mut into, &ImportOptions::default()).unwrap();
        assert_eq!(rep.ignored_controllers, 0);
        assert_eq!(rep.ignored_pitch_bends, 0);
        assert_eq!(
            summary(&rep),
            "Imported 2 notes on 2 new channels into Pattern 2 at 120 BPM; 1 tempo change read, 1 meter change read, 0 pitch bends ignored."
        );
        let kick = into.instrument(rep.channels[0]).unwrap();
        let bass = into.instrument(rep.channels[1]).unwrap();
        assert!((kick.volume - 0.5).abs() < 0.02, "kick vol {}", kick.volume);
        assert!((kick.pan - (-0.5)).abs() < 0.02, "kick pan {}", kick.pan);
        assert!((bass.volume - 0.4).abs() < 0.02, "bass vol {}", bass.volume);
        assert!((bass.pan - 0.5).abs() < 0.02, "bass pan {}", bass.pan);
    }

    #[test]
    fn volume_and_pan_automation_export_as_cc_streams() {
        use crate::automation::{Envelope, Playback, Point, Target};
        let mut song = Song::default();
        song.instruments[3].mixer = 1;
        song.add_note(1, Note::new(4, 0, 2 * BAR, 48, 100));
        song.clips.push(Clip {
            length: Some(2 * BAR),
            ..Clip::new(1, 0, 0)
        });

        // Mixer insert 1 volume fades 1.0 -> 0.0 over bar 1 and holds.
        let mut vol_pat = Pattern::new(2, "Vol");
        vol_pat.length = BAR;
        vol_pat.automation = Some(Envelope {
            target: Target::MixerVolume { track: 1 },
            playback: Playback::Hold,
            points: vec![
                Point {
                    tick: 0,
                    value: 1.0,
                    tension: 0.0,
                },
                Point {
                    tick: BAR,
                    value: 0.0,
                    tension: 0.0,
                },
            ],
        });
        song.patterns.push(vol_pat.into());
        song.clips.push(Clip::new(2, 0, 1));

        // Channel pan sweeps -1.0 -> +1.0 over bar 1..2.
        let mut pan_pat = Pattern::new(3, "Pan");
        pan_pat.length = BAR;
        pan_pat.automation = Some(Envelope {
            target: Target::ChannelPan { inst: 4 },
            playback: Playback::Clip,
            points: vec![
                Point {
                    tick: 0,
                    value: -1.0,
                    tension: 0.0,
                },
                Point {
                    tick: BAR,
                    value: 1.0,
                    tension: 0.0,
                },
            ],
        });
        song.patterns.push(pan_pat.into());
        song.clips.push(Clip::new(3, BAR, 2));

        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();
        let mut cc7s: Vec<(u64, u8)> = vec![];
        let mut cc10s: Vec<(u64, u8)> = vec![];
        let mut abs = 0u64;
        for ev in &smf.tracks[1] {
            abs += ev.delta.as_int() as u64;
            if let TrackEventKind::Midi {
                message: MidiMessage::Controller { controller, value },
                ..
            } = ev.kind
            {
                match controller.as_int() {
                    7 => cc7s.push((abs, value.as_int())),
                    10 => cc10s.push((abs, value.as_int())),
                    _ => {}
                }
            }
        }
        assert!(cc7s.len() > 10, "volume fade sampled smoothly: {cc7s:?}");
        assert_eq!(cc7s.first(), Some(&(0, 127)));
        assert_eq!(cc7s.last().map(|&(_, v)| v), Some(0));
        assert!(cc7s.last().unwrap().0 <= BAR as u64);
        assert!(
            cc7s.windows(2).all(|w| w[0].1 > w[1].1),
            "monotonic deduplicated fade"
        );
        assert!(cc10s.len() > 10, "pan sweep sampled smoothly: {cc10s:?}");
        assert_eq!(cc10s.first(), Some(&(0, 64)));
        assert!(cc10s.iter().any(|&(t, v)| t == BAR as u64 && v == 0));
        assert!(cc10s.iter().any(|&(_, v)| v >= 124));
    }

    #[test]
    fn slides_and_fine_pitch_export_with_rpn_and_reset_on_next_note() {
        let mut song = Song::default();
        song.add_note(1, Note::new(4, 0, BAR, 60, 100));
        song.add_note(1, Note::new(4, BAR, BAR, 60, 100));
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 2 * BAR;
        std::sync::Arc::make_mut(&mut song.patterns[0]).slides = vec![
            crate::sequence::Glide {
                start: 0,
                length: 4 * STEP,
                target: 72, // +12 semitones
                group: 0,
                instrument: 4,
            },
            crate::sequence::Glide {
                start: 8 * STEP,
                length: 4 * STEP,
                target: 65, // down to +5 semitones
                group: 0,
                instrument: 4,
            },
        ];

        let bytes = export_smf(&song, &MidiSource::Pattern(1), &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();
        let mut rpn_range = None;
        let mut bends: Vec<(u64, u16)> = vec![];
        let mut abs = 0u64;
        for ev in &smf.tracks[1] {
            abs += ev.delta.as_int() as u64;
            if let TrackEventKind::Midi { message, .. } = ev.kind {
                match message {
                    MidiMessage::Controller { controller, value } if controller.as_int() == 6 => {
                        rpn_range = Some(value.as_int());
                    }
                    MidiMessage::PitchBend { bend } => bends.push((abs, bend.0.as_int())),
                    _ => {}
                }
            }
        }
        assert_eq!(rpn_range, Some(12), "RPN 0,0 declares 12-semitone range");
        assert!(
            bends
                .iter()
                .any(|&(t, v)| t == (4 * STEP) as u64 && v == 16383)
        );
        // Second slide finishes at +5/12 of range.
        let expect_5 = (8192.0_f64 + (5.0 / 12.0) * 8191.0).round() as u16;
        assert!(
            bends
                .iter()
                .any(|&(t, v)| t == (12 * STEP) as u64 && (v as i32 - expect_5 as i32).abs() <= 1),
            "second slide hits +5 st: {bends:?}"
        );
        // Next note at BAR resets pitch bend to centre (8192).
        assert_eq!(bends.last(), Some(&(BAR as u64, 8192)));
    }

    #[test]
    fn per_note_pan_mod_markers_and_channel_10_skip() {
        let mut song = Song::default();
        // Add enough melodic instruments so index 9 is melodic (must skip GM drum channel 9).
        for n in 0..7 {
            song.add_sampler(&format!("Lead {n}"));
        }
        let tenth_id = song.instruments[9].id;
        let mut n1 = Note::new(tenth_id, 0, STEP, 60, 100);
        n1.pan = -0.5;
        n1.mod_x = 1.0;
        n1.mod_y = 0.0;
        let n2 = Note::new(tenth_id, STEP, STEP, 64, 100);
        song.add_note(1, n1);
        song.add_note(1, n2);
        song.clips.push(Clip::new(1, 0, 0));
        song.markers.push(crate::sequence::Marker {
            tick: 0,
            name: "Intro".into(),
        });
        song.markers.push(crate::sequence::Marker {
            tick: BAR,
            name: "Drop".into(),
        });

        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();
        // Conductor markers.
        let mut markers = vec![];
        let mut abs = 0u64;
        for ev in &smf.tracks[0] {
            abs += ev.delta.as_int() as u64;
            if let TrackEventKind::Meta(MetaMessage::Marker(b)) = ev.kind {
                markers.push((abs, String::from_utf8_lossy(b).into_owned()));
            }
        }
        assert_eq!(
            markers,
            vec![(0, "Intro".to_string()), (BAR as u64, "Drop".to_string())]
        );

        // Melodic instrument at index 9 must NOT use MIDI channel 9 (drums).
        let mut chs = vec![];
        let mut cc74 = vec![];
        let mut cc71 = vec![];
        let mut cc10 = vec![];
        let mut abs = 0u64;
        for ev in &smf.tracks[1] {
            abs += ev.delta.as_int() as u64;
            if let TrackEventKind::Midi { channel, message } = ev.kind {
                chs.push(channel.as_int());
                if let MidiMessage::Controller { controller, value } = message {
                    match controller.as_int() {
                        10 => cc10.push((abs, value.as_int())),
                        71 => cc71.push((abs, value.as_int())),
                        74 => cc74.push((abs, value.as_int())),
                        _ => {}
                    }
                }
            }
        }
        assert!(chs.iter().all(|&c| c == 10), "index 9 maps to MIDI ch 10");
        assert_eq!(cc10, vec![(0, 32), (STEP as u64, 64)]);
        assert_eq!(cc74, vec![(0, 127), (STEP as u64, 64)]);
        assert_eq!(cc71, vec![(0, 0), (STEP as u64, 64)]);
    }

    #[test]
    fn showcase_demo_song_exports_valid_smf_with_mix_and_markers() {
        let song = crate::demo::build(None);
        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();
        assert!(smf.tracks.len() > 2);
        let has_markers = smf.tracks[0]
            .iter()
            .any(|e| matches!(e.kind, TrackEventKind::Meta(MetaMessage::Marker(_))));
        assert!(has_markers, "demo song markers exported");
        let cc7_count: usize = smf.tracks[1..]
            .iter()
            .flat_map(|t| t.iter())
            .filter(|e| {
                matches!(
                    e.kind,
                    TrackEventKind::Midi {
                        message: MidiMessage::Controller { controller, .. },
                        ..
                    } if controller.as_int() == 7
                )
            })
            .count();
        assert!(cc7_count >= smf.tracks.len() - 1);
    }

    #[test]
    fn song_export_repeats_clips_and_skips_muted() {
        let mut song = Song::default();
        song.add_note(1, Note::new(4, 0, STEP, 60, 100));
        song.add_note(1, Note::new(4, STEP, STEP, 62, 100));
        song.add_note(1, Note::new(4, 4 * STEP, STEP, 64, 100));
        // Longer than its pattern: the pattern repeats.
        song.clips.push(Clip {
            length: Some(2 * BAR),
            ..Clip::new(1, 0, 0)
        });
        // Offset/length window: only the STEP note sounds, at the clip start.
        song.clips.push(Clip {
            offset: STEP,
            length: Some(2 * STEP),
            ..Clip::new(1, 4 * BAR, 0)
        });
        // Muted clip and muted track contribute nothing.
        song.clips.push(Clip {
            muted: true,
            ..Clip::new(1, 8 * BAR, 0)
        });
        song.track_mut(1).muted = true;
        song.clips.push(Clip::new(1, 8 * BAR, 1));
        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let bar = BAR as u64;
        let mut expect = vec![
            (0, 60),
            (120, 62),
            (480, 64),
            (1920, 60),
            (2040, 62),
            (2400, 64),
            (4 * bar, 62),
        ];
        expect.sort();
        assert_eq!(note_ons(&bytes), expect);
    }

    #[test]
    fn ppq_96_scales_exactly() {
        // Absolute: on60@0 off@96; on64@1 off@2; on62@32 off@64 (triplet 8ths).
        let track = vec![
            ev(0, on(0, 60, 100)),
            ev(1, on(0, 64, 100)),
            ev(1, off(0, 64)),
            ev(30, on(0, 62, 90)),
            ev(32, off(0, 62)),
            ev(32, off(0, 60)),
            ev(0, end()),
        ];
        let bytes = smf_bytes(Timing::Metrical(u15::new(96)), vec![track]);
        let mut song = Song::default();
        let rep = import_smf(&bytes, &mut song, &ImportOptions::default()).unwrap();
        assert_eq!(rep.notes, 3);
        assert_eq!(rep.repaired_notes, 0);
        assert_eq!(rep.names, vec!["MIDI Ch 1"]);
        let mut got: Vec<(Tick, Tick, u8)> = song
            .pattern(rep.pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| (n.start, n.length, n.pitch))
            .collect();
        got.sort();
        assert_eq!(got, vec![(0, 480, 60), (5, 5, 64), (160, 160, 62)]);
        assert!(song.validate().is_ok());
    }

    #[test]
    fn note_off_vel0_fifo_and_repairs() {
        // Track 1: vel-0 NoteOn closes; trailing note never closes.
        let t1 = vec![
            ev(0, on(0, 60, 100)),
            ev(480, on(0, 60, 0)),
            ev(0, on(0, 62, 100)),
            ev(0, end()),
        ];
        // Track 2: overlapping same-key pair closes FIFO at the later start.
        let t2 = vec![
            ev(0, on(1, 60, 90)),
            ev(480, on(1, 60, 80)),
            ev(480, off(1, 60)),
            ev(0, end()),
        ];
        // Track 3: zero-length note grows to length 1.
        let t3 = vec![ev(100, on(2, 63, 100)), ev(0, off(2, 63)), ev(0, end())];
        let bytes = ppq480(vec![t1, t2, t3]);
        let mut song = Song::default();
        let rep = import_smf(&bytes, &mut song, &ImportOptions::default()).unwrap();
        assert_eq!(rep.notes, 5);
        assert_eq!(rep.channels.len(), 3);
        assert_eq!(rep.repaired_notes, 2);
        let p = song.pattern(rep.pattern).unwrap();
        let on_channel = |id: u64| {
            let mut v: Vec<(Tick, Tick, u8, u8)> = p
                .notes
                .iter()
                .filter(|n| n.instrument == id)
                .map(|n| (n.start, n.length, n.pitch, n.velocity))
                .collect();
            v.sort();
            v
        };
        assert_eq!(
            on_channel(rep.channels[0]),
            vec![(0, 480, 60, 100), (480, 1, 62, 100)]
        );
        assert_eq!(
            on_channel(rep.channels[1]),
            vec![(0, 480, 60, 90), (480, 480, 60, 80)]
        );
        assert_eq!(on_channel(rep.channels[2]), vec![(100, 1, 63, 100)]);
        assert!(song.validate().is_ok());
    }

    #[test]
    fn tempo_first_wins_and_missing_stays() {
        let two = ppq480(vec![vec![
            ev(0, tempo_ev(500_000)),
            ev(480, tempo_ev(468_750)),
            ev(0, end()),
        ]]);
        let mut song = Song::default();
        let rep = import_smf(&two, &mut song, &ImportOptions::default()).unwrap();
        assert_eq!(rep.tempo, Some(120.0));
        assert_eq!(rep.tempos, 2);
        assert_eq!(rep.meters, 0);
        assert_eq!(song.tempo, 120.0);

        let none = ppq480(vec![vec![
            ev(0, on(0, 60, 100)),
            ev(120, off(0, 60)),
            ev(0, end()),
        ]]);
        let mut song = Song::default();
        let rep = import_smf(&none, &mut song, &ImportOptions::default()).unwrap();
        assert_eq!(rep.tempo, None);
        assert_eq!(rep.tempos, 0);
        assert_eq!(rep.meters, 0);
        assert_eq!(song.tempo, 120.0);
        assert!(song.validate().is_ok());
    }

    #[test]
    fn tempo_map_and_meters_round_trip() {
        use crate::tempo::{Meter, TempoMap, TempoPoint, TempoShape};
        let mut song = Song::default();
        song.tempo = 120.0;
        song.tempo_map = TempoMap {
            points: vec![
                TempoPoint {
                    tick: 0,
                    bpm: 120.0,
                    shape: TempoShape::Step,
                },
                TempoPoint {
                    tick: BAR,
                    bpm: 140.0,
                    shape: TempoShape::Step,
                },
                TempoPoint {
                    tick: 2 * BAR,
                    bpm: 100.0,
                    shape: TempoShape::Step,
                },
            ],
        };
        song.meters = vec![
            Meter {
                bar: 0,
                num: 4,
                den: 4,
            },
            Meter {
                bar: 2,
                num: 3,
                den: 4,
            },
            Meter {
                bar: 3,
                num: 7,
                den: 8,
            },
        ];
        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let mut into = Song::default();
        let rep = import_smf(&bytes, &mut into, &ImportOptions::default()).unwrap();
        assert_eq!(rep.tempos, 3);
        assert_eq!(rep.meters, 3);
        assert_eq!(into.tempo, 120.0);
        assert_eq!(into.tempo_map.points.len(), 3);
        let want = [(0, 120.0), (BAR, 140.0), (2 * BAR, 100.0)];
        for (got, (tick, bpm)) in into.tempo_map.points.iter().zip(want) {
            assert_eq!(got.tick, tick);
            assert_eq!(got.shape, TempoShape::Step);
            assert!((got.bpm - bpm).abs() < 0.01, "{} vs {bpm}", got.bpm);
        }
        assert_eq!(into.meters, song.meters);
        assert!(into.validate().is_ok());
    }

    #[test]
    fn flat_song_conductor_matches_legacy_layout() {
        // Flat songs export exactly one tempo + one 4/4 meta at tick 0,
        // the same conductor bytes as before the tempo map existed.
        let song = Song::default();
        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let smf = Smf::parse(&bytes).unwrap();
        let mut tempos = vec![];
        let mut sigs = vec![];
        for t in &smf.tracks {
            let mut abs = 0u64;
            for e in t {
                abs += e.delta.as_int() as u64;
                match &e.kind {
                    TrackEventKind::Meta(MetaMessage::Tempo(u)) => tempos.push((abs, u.as_int())),
                    TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => {
                        sigs.push((abs, *n, *d))
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(tempos, vec![(0, 500_000)]);
        assert_eq!(sigs, vec![(0, 4, 2)]);
    }

    #[test]
    fn ramp_exports_steps_and_imports_as_step_points() {
        use crate::tempo::{TempoMap, TempoPoint, TempoShape};
        let mut song = Song::default();
        song.tempo = 100.0;
        song.tempo_map = TempoMap {
            points: vec![
                TempoPoint {
                    tick: 0,
                    bpm: 100.0,
                    shape: TempoShape::Linear,
                },
                TempoPoint {
                    tick: BAR,
                    bpm: 140.0,
                    shape: TempoShape::Step,
                },
            ],
        };
        let bytes = export_smf(&song, &MidiSource::Song, &ExportOptions::default());
        let mut into = Song::default();
        let rep = import_smf(&bytes, &mut into, &ImportOptions::default()).unwrap();
        assert!(rep.tempos > 2, "ramp approximated, got {}", rep.tempos);
        assert!(!into.tempo_map.is_empty());
        assert!(
            into.tempo_map
                .points
                .iter()
                .all(|p| p.shape == TempoShape::Step)
        );
        assert_eq!(into.tempo, 100.0);
        assert_eq!(into.tempo_map.points.first().unwrap().tick, 0);
        assert_eq!(into.tempo_map.points.last().unwrap().tick, BAR);
        assert!((into.tempo_map.points.first().unwrap().bpm - 100.0).abs() < 0.01);
        assert!((into.tempo_map.points.last().unwrap().bpm - 140.0).abs() < 0.01);
        assert!(into.validate().is_ok());
    }

    #[test]
    fn smpte_and_garbage_fail_without_mutation() {
        let smpte = smf_bytes(Timing::Timecode(Fps::Fps24, 8), vec![vec![ev(0, end())]]);
        let mut song = Song::demo();
        let before = serde_json::to_string(&song).unwrap();
        let err = import_smf(&smpte, &mut song, &ImportOptions::default()).unwrap_err();
        assert!(err.contains("SMPTE"), "{err}");
        assert_eq!(serde_json::to_string(&song).unwrap(), before);
        let err = import_smf(
            b"definitely not a midi file",
            &mut song,
            &ImportOptions::default(),
        )
        .unwrap_err();
        assert_eq!(serde_json::to_string(&song).unwrap(), before);
        assert!(!err.is_empty());
    }

    #[test]
    fn channel_10_maps_to_drum_voices() {
        let track = vec![
            ev(0, on(9, 36, 100)),
            ev(120, off(9, 36)),
            ev(0, on(9, 38, 90)),
            ev(120, off(9, 38)),
            ev(0, on(9, 42, 80)),
            ev(120, off(9, 42)),
            ev(0, on(9, 46, 70)),
            ev(120, off(9, 46)),
            ev(0, end()),
        ];
        let bytes = ppq480(vec![track]);
        let mut song = Song::default();
        let rep = import_smf(&bytes, &mut song, &ImportOptions::default()).unwrap();
        assert_eq!(rep.names, vec!["Kick (MIDI)", "Snare (MIDI)", "Hat (MIDI)"]);
        let voices: Vec<u8> = rep
            .channels
            .iter()
            .map(|id| song.instrument(*id).unwrap().drum_voice().unwrap())
            .collect();
        assert_eq!(voices, vec![0, 1, 2]);
        let p = song.pattern(rep.pattern).unwrap();
        assert!(p.notes.iter().all(|n| n.pitch == STEP_PITCH));
        let hat = p
            .notes
            .iter()
            .filter(|n| n.instrument == rep.channels[2])
            .count();
        assert_eq!(hat, 2);
        assert!(song.validate().is_ok());

        // With drums off, channel 10 stays a pitched sampler channel.
        let mut plain = Song::default();
        let rep = import_smf(
            &bytes,
            &mut plain,
            &ImportOptions {
                drums_on_channel_10: false,
                ..ImportOptions::default()
            },
        )
        .unwrap();
        assert_eq!(rep.channels.len(), 1);
        assert!(plain.instrument(rep.channels[0]).unwrap().is_sampler());
        let mut pitches: Vec<u8> = plain
            .pattern(rep.pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.pitch)
            .collect();
        pitches.sort();
        assert_eq!(pitches, vec![36, 38, 42, 46]);

        // With a kit to load: one channel, every drum on its own GM key.
        let mut kit = Song::default();
        let rep = import_smf(
            &bytes,
            &mut kit,
            &ImportOptions {
                drum_kit: true,
                ..ImportOptions::default()
            },
        )
        .unwrap();
        assert_eq!(rep.names, vec!["Drums · Standard"]);
        let mut keys: Vec<u8> = kit
            .pattern(rep.pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.pitch)
            .collect();
        keys.sort();
        assert_eq!(keys, vec![36, 38, 42, 46]);
        assert_eq!(gm_kit_name(Some(25)), "TR-808");
        assert_eq!(gm_kit_name(Some(10)), "Room", "a room variation");
    }

    #[test]
    fn export_is_deterministic_and_pure() {
        let song = Song::demo();
        let before = serde_json::to_string(&song).unwrap();
        let opts = ExportOptions::default();
        let a = export_smf(&song, &MidiSource::Song, &opts);
        let b = export_smf(&song, &MidiSource::Song, &opts);
        let c = export_smf(
            &song,
            &MidiSource::Pattern(1),
            &ExportOptions {
                one_track_per_channel: false,
            },
        );
        let d = export_smf(
            &song,
            &MidiSource::Pattern(1),
            &ExportOptions {
                one_track_per_channel: false,
            },
        );
        assert_eq!(a, b);
        assert_eq!(c, d);
        assert_eq!(serde_json::to_string(&song).unwrap(), before);
    }
}

#[cfg(test)]
mod bench {
    /// KEYFRAMEXL_BENCH_MIDI=file.mid: realtime factor of the imported song,
    /// with every note sounding and with every note muted (search cost only).
    #[test]
    fn imported_song_render_speed() {
        let Some(path) = std::env::var_os("KEYFRAMEXL_BENCH_MIDI") else {
            return;
        };
        let mut song = crate::sequence::Song::default();
        let r = super::import_smf(
            &std::fs::read(path).unwrap(),
            &mut song,
            &Default::default(),
        )
        .unwrap();
        let mut e = crate::engine::Engine::new(48_000);
        e.reserve(song.mixer.len(), 512);
        let mut buf = vec![0.0f32; 1024];
        let secs = 10.0;
        let blocks = (secs * 48_000.0 / 512.0) as u64;
        let t = std::time::Instant::now();
        let mut at = 0;
        for _ in 0..blocks {
            at = e.process(&song, r.pattern, false, at, &mut buf, true);
        }
        let all = t.elapsed().as_secs_f64();
        for n in &mut song.pattern_mut(r.pattern).unwrap().notes {
            n.muted = true;
        }
        let mut e = crate::engine::Engine::new(48_000);
        e.reserve(song.mixer.len(), 512);
        let t = std::time::Instant::now();
        let mut at = 0;
        for _ in 0..blocks {
            at = e.process(&song, r.pattern, false, at, &mut buf, true);
        }
        let search = t.elapsed().as_secs_f64();
        eprintln!(
            "10 s of audio: {:.3} s with notes ({:.1}% of realtime), {:.3} s search only ({:.1}%)",
            all,
            all / secs * 100.0,
            search,
            search / secs * 100.0
        );
    }
}
