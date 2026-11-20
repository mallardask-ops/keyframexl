//! Standard MIDI File import and export (midly).
//!
//! Export builds a format-1 SMF at 480 PPQ without touching the song;
//! import parses a file into a brand-new pattern plus new channels,
//! leaving the song untouched when anything fails.

use std::collections::{BTreeMap, VecDeque};

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u15, u24, u28},
};

use crate::sequence::{BAR, InstrumentKind, Note, PPQ, STEP, STEP_PITCH, Song, Tick};

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
}

fn midi_channel(song: &Song, inst: usize) -> u8 {
    match song.instruments[inst].kind {
        InstrumentKind::Drum { .. } => DRUM_CHANNEL,
        _ => (inst % 16) as u8,
    }
}

fn export_pitch(song: &Song, inst: usize, pitch: u8) -> u8 {
    match song.instruments[inst].kind {
        InstrumentKind::Drum { voice } => DRUM_GM.get(voice as usize).copied().unwrap_or(42),
        _ => pitch.min(127),
    }
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
            });
        }
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
                    });
                }
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

pub fn export_smf(song: &Song, source: &MidiSource, opts: &ExportOptions) -> Vec<u8> {
    let mut notes = match source {
        MidiSource::Pattern(id) => pattern_notes(song, *id),
        MidiSource::Song => song_notes(song),
    };
    repair_overlaps(&mut notes);

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

    // Per-track event lists as (absolute tick, event), delta-encoded later.
    let mut conductor: Vec<(u64, TrackEventKind)> = vec![];
    push_track_name(&mut conductor, &owned[0]);
    push_conductor_tempo_meters(&mut conductor, song);

    // Track per instrument that has notes, in instrument order.
    let mut per_track: Vec<Vec<(u64, TrackEventKind)>> = vec![Vec::new(); song.instruments.len()];
    let mut has_notes = vec![false; song.instruments.len()];
    let mut merged: Vec<(u64, TrackEventKind)> = vec![];
    if !opts.one_track_per_channel {
        push_track_name(&mut merged, &owned[merged_name]);
    }
    for n in &notes {
        let channel = u4::new(n.channel);
        let on = TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOn {
                key: u7::new(n.pitch),
                vel: u7::new(n.vel),
            },
        };
        let off = TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOff {
                key: u7::new(n.pitch),
                vel: u7::new(64),
            },
        };
        if opts.one_track_per_channel {
            per_track[n.inst].push((n.start, on));
            per_track[n.inst].push((n.end, off));
            has_notes[n.inst] = true;
        } else {
            merged.push((n.start, on));
            merged.push((n.end, off));
        }
    }

    let mut smf = Smf::new(Header::new(
        Format::Parallel,
        Timing::Metrical(u15::new(PPQ as u16)),
    ));
    fn finish<'a>(evs: &mut Vec<(u64, TrackEventKind<'a>)>) -> midly::Track<'a> {
        evs.sort_by_key(|(t, k)| {
            (
                *t,
                matches!(
                    k,
                    TrackEventKind::Midi {
                        message: MidiMessage::NoteOn { .. },
                        ..
                    }
                ) as u8,
            )
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
            if !has_notes[i] {
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
                        // wins, FL does the same); other controllers are counted.
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
            color: None,
            loop_range: None,
            lyrics: Default::default(),
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
                    inst.volume = (g.volume.unwrap_or(100) as f32 / 127.0).powi(2);
                    inst.pan = g
                        .pan
                        .map(|p| ((p as f32 - 64.0) / 63.0).clamp(-1.0, 1.0))
                        .unwrap_or(0.0);
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
                    inst.volume = (g.volume.unwrap_or(100) as f32 / 127.0).powi(2);
                    inst.pan = g
                        .pan
                        .map(|p| ((p as f32 - 64.0) / 63.0).clamp(-1.0, 1.0))
                        .unwrap_or(0.0);
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
        assert_eq!(
            summary(&rep),
            "Imported 15 notes on 4 new channels into Pattern 2 at 120 BPM; 1 tempo change read, 1 meter change read, 0 pitch bends ignored."
        );
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
