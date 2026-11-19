//! Native DirectWave `.dwp` reader.
//!
//! Parses the binary program format directly (no ConvertWithMoss round trip),
//! so everything the SFZ conversion drops survives: ENV 1/2, Filter 1 and 2,
//! Zone LFO 1/2, the 16-slot modulation matrix, and monolithic samples
//! (planar PCM or FLAC) as well as external WAV/FLAC/OGG files.
//!
//! Layout (little endian). A 36-byte header (`DwPr`, version, ...) is
//! followed by a stream of blocks, each `tag: u32, len: u32, reserved: u32`
//! plus `len` payload bytes. Modern files wrap the program in a `tag 1`
//! container; older ones put the program blocks straight after the header.
//!
//! * `102` program name, `3` zone container, `2` end of program.
//! * Zone container: `500` mapping, `501` name, `502` stored sample path,
//!   `503` audio format and loop, `507` filter (twice), `509` amp envelope,
//!   `514` ENV 1/2, `515` LFO 1/2, `516` matrix slot (x16), `517` planar PCM,
//!   `518` FLAC, `4` end of zone.

use keyframexl::sampler::{
    Adsr, Filter, FilterKind, Lfo, LfoShape, LoopMode, ModSource, ModTarget, Program, Zone,
    ZoneParams,
};
use keyframexl::sequence::SamplePreset;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Decoded audio: sample rate and stereo frames.
type Loaded = (u32, Vec<[f32; 2]>);

const TAG_CONTAINER: u32 = 1;
const TAG_END_PROGRAM: u32 = 2;
const TAG_ZONES: u32 = 3;
const TAG_END_ZONE: u32 = 4;
const TAG_MASTER: u32 = 100;
const TAG_PROGRAM_NAME: u32 = 102;
const ZONE_MAPPING: u32 = 500;
const ZONE_NAME: u32 = 501;
const ZONE_PATH: u32 = 502;
const ZONE_FORMAT: u32 = 503;
const ZONE_FILTER: u32 = 507;
const ZONE_AMP_ENV: u32 = 509;
const ZONE_ENV: u32 = 514;
const ZONE_LFO: u32 = 515;
const ZONE_MATRIX: u32 = 516;
const ZONE_PCM: u32 = 517;
const ZONE_FLAC: u32 = 518;

/// Silence DirectWave pads around every embedded sample plane.
const GUARD: usize = 256;
const HEADER_LEN: usize = 36;
const CHUNK_HEADER: usize = 12;
const MAX_SAMPLE_RATE: u32 = 768_000;

pub fn is_dwp_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"DwPr")
}

/// Read a `.dwp` file. Samples stored outside the program are looked up next
/// to it (see [`read_dwp_bytes_with_loader`]).
pub fn read_dwp(path: &Path) -> Result<SamplePreset, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let mut preset = read_dwp_bytes(&bytes, path.parent(), stem)?;
    if preset.name.is_empty() {
        preset.name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    }
    preset.original_dwp = Arc::new(bytes);
    preset.validate()?;
    Ok(preset)
}

/// Parse program bytes. `base_dir` is the folder of the `.dwp` (for external
/// samples) and `dwp_stem` its file name without extension.
pub fn read_dwp_bytes(
    bytes: &[u8],
    base_dir: Option<&Path>,
    dwp_stem: &str,
) -> Result<SamplePreset, String> {
    read_dwp_bytes_with_loader(bytes, base_dir, dwp_stem, crate::import::read_audio)
}

/// Like [`read_dwp_bytes`], with the decoder for external sample files
/// supplied by the caller.
pub fn read_dwp_bytes_with_loader<F>(
    bytes: &[u8],
    base_dir: Option<&Path>,
    dwp_stem: &str,
    loader: F,
) -> Result<SamplePreset, String>
where
    F: Fn(&Path) -> Result<Loaded, String>,
{
    if !is_dwp_magic(bytes) {
        return Err("Not a DirectWave program (missing DwPr header)".into());
    }
    if bytes.len() < HEADER_LEN + CHUNK_HEADER {
        return Err("DirectWave program is truncated".into());
    }
    let version = rd_u32(bytes, 4);
    let program = parse_program(bytes, version)?;
    let mut notes = Vec::new();
    let mut zones = Vec::with_capacity(program.zones.len());
    for (i, raw) in program.zones.iter().enumerate() {
        let zone = build_zone(raw, version, base_dir, dwp_stem, &loader, &mut notes)
            .map_err(|e| format!("Zone {} \"{}\": {e}", i + 1, raw.name))?;
        zones.push(zone);
    }
    let mut report = format!(
        "Native DirectWave import (format v{version}): {} zone(s) with embedded or external audio, \
         amp and ENV 1/2 envelopes, Filter 1/2, LFO 1/2 and the modulation matrix.",
        zones.len()
    );
    if !program.ignored.is_empty() {
        let list = program
            .ignored
            .iter()
            .map(|(tag, n)| format!("{tag}x{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        report.push_str(&format!(" Program blocks not interpreted: {list}."));
    }
    if !notes.is_empty() {
        report.push_str(&format!(" Approximated or skipped: {}.", notes.join("; ")));
    }
    let preset = SamplePreset {
        name: program.name,
        program: Program {
            volume_db: program.volume_db,
            ..Program::default()
        },
        zones,
        original_dwp: Arc::new(Vec::new()),
        report,
    };
    preset.validate()?;
    Ok(preset)
}

// ------------------------------------------------------------------ chunk walking

struct Chunk<'a> {
    tag: u32,
    payload: &'a [u8],
}

fn rd_u16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2)
        .map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}
fn rd_u32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_bits(rd_u32(b, o))
}

/// Split `data` into blocks, stopping after `stop` if given. A block that
/// runs past the end of the data is an error; fewer than 12 trailing bytes
/// are padding.
fn read_chunks<'a>(data: &'a [u8], base: usize, stop: &[u32]) -> Result<Vec<Chunk<'a>>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + CHUNK_HEADER <= data.len() {
        let tag = rd_u32(data, pos);
        let len = rd_u32(data, pos + 4) as usize;
        let start = pos + CHUNK_HEADER;
        let end = start
            .checked_add(len)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| format!("Truncated block {tag} ({len} bytes) at byte {}", base + pos))?;
        out.push(Chunk {
            tag,
            payload: &data[start..end],
        });
        pos = end;
        if stop.contains(&tag) {
            break;
        }
    }
    Ok(out)
}

struct RawZone<'a> {
    mapping: Option<&'a [u8]>,
    name: String,
    path: String,
    format: Option<&'a [u8]>,
    filters: Vec<&'a [u8]>,
    amp_env: Option<&'a [u8]>,
    envs: Vec<&'a [u8]>,
    lfos: Vec<&'a [u8]>,
    matrix: Vec<&'a [u8]>,
    pcm: Option<&'a [u8]>,
    flac: Option<&'a [u8]>,
}

struct RawProgram<'a> {
    name: String,
    volume_db: f64,
    zones: Vec<RawZone<'a>>,
    ignored: Vec<(u32, usize)>,
}

fn text(bytes: &[u8]) -> String {
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    s.trim_matches('\0').trim().to_owned()
}

fn parse_zone_blocks(data: &[u8], base: usize) -> Result<RawZone<'_>, String> {
    let mut z = RawZone {
        mapping: None,
        name: String::new(),
        path: String::new(),
        format: None,
        filters: Vec::new(),
        amp_env: None,
        envs: Vec::new(),
        lfos: Vec::new(),
        matrix: Vec::new(),
        pcm: None,
        flac: None,
    };
    for c in read_chunks(data, base, &[TAG_END_ZONE])? {
        match c.tag {
            ZONE_MAPPING => z.mapping = Some(c.payload),
            ZONE_NAME => z.name = text(c.payload),
            ZONE_PATH => z.path = text(c.payload),
            ZONE_FORMAT => z.format = Some(c.payload),
            ZONE_FILTER if z.filters.len() < 2 => z.filters.push(c.payload),
            ZONE_AMP_ENV => z.amp_env = Some(c.payload),
            ZONE_ENV if z.envs.len() < 2 => z.envs.push(c.payload),
            ZONE_LFO if z.lfos.len() < 2 => z.lfos.push(c.payload),
            ZONE_MATRIX if z.matrix.len() < keyframexl::sampler::MOD_SLOTS => {
                z.matrix.push(c.payload)
            }
            ZONE_PCM => z.pcm = Some(c.payload),
            ZONE_FLAC => z.flac = Some(c.payload),
            _ => {}
        }
    }
    Ok(z)
}

/// Walk the program-level blocks starting at `data` (file offset `base`).
fn parse_top<'a>(data: &'a [u8], base: usize) -> Result<RawProgram<'a>, String> {
    let mut program = RawProgram {
        name: String::new(),
        volume_db: 0.0,
        zones: Vec::new(),
        ignored: Vec::new(),
    };
    let mut offset = base;
    for c in read_chunks(data, base, &[TAG_END_PROGRAM])? {
        match c.tag {
            TAG_PROGRAM_NAME => program.name = text(c.payload),
            TAG_ZONES => program.zones.push(parse_zone_blocks(c.payload, offset)?),
            TAG_END_PROGRAM => {}
            tag => match program.ignored.iter_mut().find(|(t, _)| *t == tag) {
                Some((_, n)) => *n += 1,
                None => program.ignored.push((tag, 1)),
            },
        }
        offset += CHUNK_HEADER + c.payload.len();
    }
    Ok(program)
}

/// Pick where the program blocks start (container or flat), parse them, and
/// add the master volume.
fn parse_program(bytes: &[u8], version: u32) -> Result<RawProgram<'_>, String> {
    let container_len = (rd_u32(bytes, HEADER_LEN) == TAG_CONTAINER)
        .then(|| rd_u32(bytes, HEADER_LEN + 4) as usize)
        .filter(|&len| (HEADER_LEN + CHUNK_HEADER).saturating_add(len) <= bytes.len());
    let mut program = if let Some(len) = container_len {
        let start = HEADER_LEN + CHUNK_HEADER;
        parse_top(&bytes[start..start + len], start)?
    } else {
        // Older files (or ones with a bogus container length): blocks follow
        // a fixed preamble. Try the offset for this version, then the other.
        let (first, second) = if version == 0x24 {
            (0x5E, 0x5A)
        } else {
            (0x5A, 0x5E)
        };
        let mut result: Result<RawProgram, String> =
            Err("DirectWave program has no readable blocks".into());
        for start in [first, second] {
            let Some(data) = bytes.get(start..) else {
                continue;
            };
            match parse_top(data, start) {
                Ok(p) if !p.zones.is_empty() => {
                    result = Ok(p);
                    break;
                }
                Ok(_) if result.is_err() => result = Err("DirectWave program has no zones".into()),
                Err(e) if result.is_err() => result = Err(e),
                _ => {}
            }
        }
        result?
    };
    if program.zones.is_empty() {
        return Err("DirectWave program has no zones".into());
    }
    // Program master volume: f32 at byte 6 of the first block's payload.
    let master = HEADER_LEN + CHUNK_HEADER;
    if rd_u32(bytes, master) == TAG_MASTER && rd_u32(bytes, master + 4) >= 10 {
        let v = rd_f32(bytes, master + CHUNK_HEADER + 6);
        if v.is_finite() && v > 0.0 && v != 1.0 {
            program.volume_db = (20.0 * (v as f64).log10()).clamp(-60.0, 12.0);
        }
    }
    Ok(program)
}

// ------------------------------------------------------------------ parameter laws

/// DirectWave envelope time: knob 0..1 to seconds.
fn dw_time(knob: f32) -> f64 {
    let k = knob as f64;
    if k > 0.0 {
        (9.07 * k.powf(3.96)).clamp(0.0, 60.0)
    } else {
        0.0
    }
}

fn dw_adsr(b: &[u8]) -> Adsr {
    let s = rd_f32(b, 8) as f64;
    Adsr {
        delay: 0.0,
        attack: dw_time(rd_f32(b, 0)),
        hold: 0.0,
        decay: dw_time(rd_f32(b, 4)),
        sustain: if s.is_finite() {
            s.clamp(0.0, 1.0)
        } else {
            1.0
        },
        release: dw_time(rd_f32(b, 12)),
        ..Adsr::default()
    }
}

/// An untouched DirectWave amp envelope: ours is an equivalent without the
/// instant attack click.
fn is_default_amp_env(b: &[u8], version: u32) -> bool {
    let want: [f32; 4] = match version {
        0x25 => [0.0, 0.5, 1.0, 0.25],
        0x26 => [0.0, 1.0, 1.0, 0.18],
        _ => return false,
    };
    want.iter()
        .enumerate()
        .all(|(i, w)| (rd_f32(b, i * 4) - w).abs() < 1e-4)
}

fn dw_filter(b: &[u8], notes: &mut Vec<String>) -> Filter {
    let kind = match rd_u32(b, 0) {
        0 => FilterKind::Off,
        1 => FilterKind::Lowpass,
        2 => FilterKind::Highpass,
        3 => FilterKind::Bandpass,
        4 => FilterKind::Notch,
        5 => FilterKind::Allpass,
        6 | 7 => {
            note(notes, "Minisynth/Vox filter approximated as lowpass");
            FilterKind::Lowpass
        }
        _ => {
            note(notes, "unknown filter type switched off");
            FilterKind::Off
        }
    };
    let knob = |o: usize| {
        let v = rd_f32(b, o);
        if v.is_finite() {
            v.clamp(0.0, 1.0) as f64
        } else {
            0.0
        }
    };
    Filter {
        kind,
        cutoff: (45.0 * (20000.0_f64 / 45.0).powf(knob(4))).clamp(20.0, 20000.0),
        emphasis: knob(8),
    }
}

fn dw_lfo(b: &[u8]) -> Lfo {
    let rate = rd_f32(b, 4) as f64;
    let rate = if rate.is_finite() {
        rate.clamp(0.0, 1.0)
    } else {
        0.0
    };
    Lfo {
        shape: LfoShape::ALL
            .get(rd_u32(b, 0) as usize)
            .copied()
            .unwrap_or(LfoShape::Sine),
        rate: (20.0 * rate.powi(2)).clamp(0.01, 50.0),
        ..Lfo::default()
    }
}

fn mod_source(id: u16) -> Option<ModSource> {
    Some(match id {
        1 => ModSource::Key,
        2 => ModSource::Velocity,
        5 => ModSource::ProgLfo1,
        6 => ModSource::ProgLfo2,
        9 => ModSource::Env1,
        10 => ModSource::Env2,
        11 => ModSource::Lfo1,
        12 => ModSource::Lfo2,
        15 => ModSource::Random,
        _ => return None,
    })
}

/// Target and the factor from DirectWave full scale to ours.
fn mod_target(id: u16) -> Option<(ModTarget, f64)> {
    const CUTOFF: f64 = 10.6667 / 6.0;
    Some(match id {
        1 => (ModTarget::Pitch, 2.0),
        2 => (ModTarget::Gain, 1.0),
        3 => (ModTarget::Pan, 1.0),
        7 => (ModTarget::Cutoff1, CUTOFF),
        8 => (ModTarget::Emphasis1, 1.0),
        10 => (ModTarget::Cutoff2, CUTOFF),
        11 => (ModTarget::Emphasis2, 1.0),
        _ => return None,
    })
}

fn note(notes: &mut Vec<String>, text: &str) {
    if !notes.iter().any(|n| n == text) {
        notes.push(text.to_owned());
    }
}

// ------------------------------------------------------------------ zones

struct Format {
    frame_count: usize,
    channels: usize,
    sample_rate: u32,
    loop_mode: u32,
    loop_start: usize,
    loop_end: usize,
    bit_depth: u32,
}

fn parse_format(b: &[u8]) -> Option<Format> {
    if b.len() < 40 {
        return None;
    }
    let rate = rd_f32(b, 16);
    let rate = if rate.is_finite() {
        rate.round() as u32
    } else {
        0
    };
    Some(Format {
        frame_count: rd_u32(b, 0) as usize,
        channels: rd_u32(b, 8) as usize,
        sample_rate: if (1..=MAX_SAMPLE_RATE).contains(&rate) {
            rate
        } else {
            44_100
        },
        loop_mode: rd_u32(b, 20),
        loop_start: rd_u32(b, 24) as usize,
        loop_end: rd_u32(b, 28) as usize,
        bit_depth: rd_u32(b, 36),
    })
}

fn build_zone<F>(
    raw: &RawZone,
    version: u32,
    base_dir: Option<&Path>,
    dwp_stem: &str,
    loader: &F,
    notes: &mut Vec<String>,
) -> Result<Zone, String>
where
    F: Fn(&Path) -> Result<Loaded, String>,
{
    let format = raw.format.and_then(parse_format);

    // Audio: embedded FLAC, then embedded PCM, then a file on disk.
    let (sample_rate, frames) = if let Some(flac) = raw.flac {
        decode_flac(flac, format.as_ref())?
    } else if let Some(pcm) = raw.pcm {
        let f = format
            .as_ref()
            .ok_or("embedded PCM without a format block")?;
        (f.sample_rate, decode_pcm(pcm, f)?)
    } else {
        load_external(raw, base_dir, dwp_stem, loader)?
    };
    if frames.is_empty() {
        return Err("sample has no audio frames".into());
    }
    let len = frames.len();

    // Mapping.
    let (mut root, mut low, mut high) = (60u8, 0u8, 127u8);
    let (mut low_velocity, mut high_velocity) = (1u8, 127u8);
    let (mut gain, mut pan) = (1.0f64, 0.0f64);
    if let Some(m) = raw.mapping.filter(|m| m.len() >= 17) {
        root = m[0].min(127);
        low = m[1].min(127);
        high = m[2].min(127);
        if low > high {
            std::mem::swap(&mut low, &mut high);
        }
        low_velocity = m[3].clamp(1, 127);
        high_velocity = m[4].clamp(low_velocity, 127);
        let g = rd_f32(m, 9);
        gain = if g.is_finite() {
            (g as f64).clamp(0.0, 16.0)
        } else {
            1.0
        };
        let p = rd_f32(m, 13);
        pan = if p.is_finite() {
            ((p as f64 - 0.5) * 2.0).clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }

    // Loop. DirectWave stores the last looped frame; ours is exclusive.
    let mut loop_mode = LoopMode::None;
    let (mut loop_start, mut loop_end) = (0, len);
    let mut one_shot = false;
    if let Some(f) = &format {
        let requested = match f.loop_mode {
            0 => LoopMode::None,
            1 => {
                one_shot = true;
                LoopMode::None
            }
            2 => LoopMode::Continuous,
            3 => LoopMode::Sustain,
            4 => LoopMode::PingPong,
            _ => {
                note(notes, "unknown loop mode ignored");
                LoopMode::None
            }
        };
        if requested != LoopMode::None {
            let end = f.loop_end.saturating_add(1).min(len);
            if f.loop_start + 1 < end {
                loop_mode = requested;
                (loop_start, loop_end) = (f.loop_start, end);
            }
        }
    }

    // Sound shaping.
    let mut params = ZoneParams {
        one_shot,
        ..ZoneParams::default()
    };
    for (i, b) in raw.filters.iter().enumerate() {
        if b.len() >= 12 {
            params.filters[i] = dw_filter(b, notes);
        }
    }
    for (i, b) in raw.envs.iter().enumerate() {
        if b.len() >= 16 {
            params.env[i] = dw_adsr(b);
        }
    }
    for (i, b) in raw.lfos.iter().enumerate() {
        if b.len() >= 8 {
            params.lfo[i] = dw_lfo(b);
        }
    }
    let adsr = match raw.amp_env.filter(|b| b.len() >= 16) {
        Some(b) if !is_default_amp_env(b, version) => dw_adsr(b),
        _ => Adsr::default(),
    };

    // Matrix. Velocity to Gain is our velocity tracking when it adds
    // loudness; SFZ-converted files keep the default full tracking.
    let mut velocity_tracking = 1.0f64;
    let mut velocity_set = false;
    for b in raw.matrix.iter().filter(|b| b.len() >= 8) {
        let (source_id, target_id) = (rd_u16(b, 0), rd_u16(b, 2));
        if source_id == 0 || target_id == 0 {
            continue;
        }
        let amount = rd_f32(b, 4) as f64;
        let d = 2.0 * (if amount.is_finite() { amount } else { 0.5 } - 0.5);
        let strength = d * d * d;
        if strength.abs() < 1e-6 {
            continue;
        }
        let (Some(source), Some((target, scale))) = (mod_source(source_id), mod_target(target_id))
        else {
            note(
                notes,
                "modulation slots with sources or targets we do not have",
            );
            continue;
        };
        if source == ModSource::Velocity && target == ModTarget::Gain && !velocity_set {
            velocity_set = true;
            if strength > 0.0 {
                velocity_tracking = strength.min(1.0);
                continue;
            }
            velocity_tracking = 0.0;
        }
        if !params.route(source, target, (strength * scale).clamp(-1.0, 1.0)) {
            note(notes, "modulation matrix full");
        }
    }

    Ok(Zone {
        name: raw.name.clone(),
        root,
        low,
        high,
        low_velocity,
        high_velocity,
        sample_rate,
        frames: Arc::new(frames),
        offset: 0,
        end: len,
        loop_mode,
        loop_start,
        loop_end,
        adsr,
        gain,
        pan,
        tune_cents: 0.0,
        velocity_tracking,
        params,
    })
}

// ------------------------------------------------------------------ audio

fn decode_flac(payload: &[u8], format: Option<&Format>) -> Result<Loaded, String> {
    let stream = if payload.starts_with(b"fLaC") {
        payload
    } else {
        let len = rd_u32(payload, 0) as usize;
        payload
            .get(8..)
            .and_then(|rest| rest.get(..len))
            .ok_or("embedded FLAC block is truncated")?
    };
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(stream))
        .map_err(|e| format!("embedded FLAC: {e}"))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    if channels == 0 || info.bits_per_sample == 0 {
        return Err("embedded FLAC has no channels".into());
    }
    let scale = (1u64 << (info.bits_per_sample - 1)) as f32;
    let mut frames = Vec::new();
    let mut pending = [0.0f32; 2];
    for (i, s) in reader.samples().enumerate() {
        let v = s.map_err(|e| format!("embedded FLAC: {e}"))? as f32 / scale;
        let ch = i % channels;
        if ch < 2 {
            pending[ch] = v;
        }
        if ch + 1 == channels {
            frames.push(if channels == 1 {
                [pending[0], pending[0]]
            } else {
                pending
            });
        }
    }
    // DirectWave pads the stream with a guard of silence on both sides.
    let expected = format.map_or(frames.len(), |f| f.frame_count);
    let frames = if frames.len() >= expected + GUARD {
        frames[GUARD..GUARD + expected].to_vec()
    } else if frames.len() == expected || frames.len() <= GUARD {
        frames
    } else {
        frames[GUARD..].to_vec()
    };
    let rate = format.map_or(info.sample_rate, |f| f.sample_rate);
    Ok((rate, frames))
}

fn decode_pcm(payload: &[u8], f: &Format) -> Result<Vec<[f32; 2]>, String> {
    let channels = f.channels;
    if !(1..=2).contains(&channels) {
        return Err(format!("unsupported channel count {channels}"));
    }
    let frames = f.frame_count;
    let hint = (f.bit_depth / 8) as usize;
    let mut widths = vec![hint];
    widths.extend([4, 3, 2]);
    widths.retain(|w| (2..=4).contains(w));
    // Planes of frames + 2 guards (the format); tolerate bare planes.
    for &width in &widths {
        for guard in [2 * GUARD, 0] {
            let plane = frames + guard;
            if payload.len() != channels * plane * width {
                continue;
            }
            let skip = guard / 2;
            let sample = |c: usize, i: usize| -> f32 {
                let at = (c * plane + skip + i) * width;
                let b = &payload[at..at + width];
                match width {
                    2 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
                    3 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                }
            };
            return Ok((0..frames)
                .map(|i| {
                    let l = sample(0, i);
                    [l, if channels == 2 { sample(1, i) } else { l }]
                })
                .collect());
        }
    }
    Err(format!(
        "embedded PCM size {} does not fit {frames} frames x {channels} channels",
        payload.len()
    ))
}

fn plain_name(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(':')
}

fn load_external<F>(
    raw: &RawZone,
    base_dir: Option<&Path>,
    dwp_stem: &str,
    loader: &F,
) -> Result<Loaded, String>
where
    F: Fn(&Path) -> Result<Loaded, String>,
{
    let stored = raw.path.replace('\\', "/");
    let mut parts = stored.rsplit('/').filter(|p| !p.is_empty());
    let file = parts
        .next()
        .map(str::to_owned)
        .filter(|f| plain_name(f))
        .unwrap_or_else(|| format!("{}.wav", raw.name));
    let parent = parts.next().filter(|p| plain_name(p));
    let base = base_dir
        .ok_or_else(|| format!("has no embedded audio and no folder to look for \"{file}\" in"))?;
    let mut candidates: Vec<PathBuf> = vec![base.join(&file)];
    if plain_name(dwp_stem) {
        candidates.push(base.join(dwp_stem).join(&file));
    }
    if let Some(parent) = parent {
        candidates.push(base.join(parent).join(&file));
    }
    match candidates.iter().find(|c| c.is_file()) {
        Some(found) => loader(found),
        None => Err(format!(
            "sample \"{file}\" not found (looked in {})",
            candidates
                .iter()
                .filter_map(|c| c.parent())
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 48_000;

    fn wav_loader(path: &Path) -> Result<Loaded, String> {
        let mut r = hound::WavReader::open(path).map_err(|e| e.to_string())?;
        let spec = r.spec();
        let samples: Vec<f32> = r
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let frames = if spec.channels == 1 {
            samples.iter().map(|&s| [s, s]).collect()
        } else {
            samples.chunks(2).map(|c| [c[0], c[1]]).collect()
        };
        Ok((spec.sample_rate, frames))
    }

    fn wav_frames(path: &str) -> Vec<[f32; 2]> {
        wav_loader(Path::new(path)).unwrap().1
    }

    // ---- synthetic builder

    fn chunk(tag: u32, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(tag.to_le_bytes());
        v.extend((payload.len() as u32).to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.extend(payload);
        v
    }
    fn floats(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
    fn mapping(root: u8, low: u8, high: u8, lv: u8, hv: u8, gain: f32, pan: f32) -> Vec<u8> {
        let mut m = vec![root, low, high, lv, hv, 0, 0, 0, 0];
        m.extend(floats(&[gain, pan]));
        m.extend([0, 0, 2, 0, 0]);
        chunk(ZONE_MAPPING, &m)
    }
    fn format(frames: u32, channels: u32, mode: u32, start: u32, end: u32, bits: u32) -> Vec<u8> {
        let mut f = Vec::new();
        for v in [frames, 0, channels, 4] {
            f.extend(v.to_le_bytes());
        }
        f.extend((SAMPLE_RATE as f32).to_le_bytes());
        for v in [mode, start, end, 0, bits] {
            f.extend(v.to_le_bytes());
        }
        chunk(ZONE_FORMAT, &f)
    }
    fn matrix_slot(source: u16, target: u16, amount: f32) -> Vec<u8> {
        let mut s = Vec::new();
        s.extend(source.to_le_bytes());
        s.extend(target.to_le_bytes());
        s.extend(amount.to_le_bytes());
        chunk(ZONE_MATRIX, &s)
    }
    /// Value in -1..1 on a grid every format stores exactly.
    fn test_sample(channel: usize, i: usize) -> f32 {
        if channel == 0 {
            ((i * 37 % 2000) as f32 - 1000.0) / 1024.0
        } else {
            ((i * 91 % 2000) as f32 - 1000.0) / 2048.0
        }
    }
    /// Planar PCM: each plane is 256 guard frames, the audio, 256 guard frames.
    fn planar(channels: usize, frames: usize, width: usize, guard: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for c in 0..channels {
            for i in 0..frames + 2 * guard {
                let v = if i < guard || i >= guard + frames {
                    0.0
                } else {
                    test_sample(c, i - guard)
                };
                match width {
                    2 => out.extend(((v * 32768.0) as i16).to_le_bytes()),
                    3 => out.extend(&(((v * 8_388_608.0) as i32).to_le_bytes()[..3])),
                    _ => out.extend(v.to_le_bytes()),
                }
            }
        }
        out
    }
    fn zone(parts: &[Vec<u8>]) -> Vec<u8> {
        let mut body: Vec<u8> = parts.concat();
        body.extend(chunk(TAG_END_ZONE, &[]));
        chunk(TAG_ZONES, &body)
    }
    fn pcm_zone(frames: usize, channels: usize, width: usize, extra: &[Vec<u8>]) -> Vec<u8> {
        let mut parts = vec![
            mapping(60, 48, 72, 0, 127, 1.0, 0.5),
            chunk(ZONE_NAME, b"pcm"),
            format(frames as u32, channels as u32, 0, 0, 0, width as u32 * 8),
        ];
        parts.extend(extra.iter().cloned());
        parts.push(chunk(ZONE_PCM, &planar(channels, frames, width, GUARD)));
        zone(&parts)
    }
    fn build(version: u32, volume: f32, zones: &[Vec<u8>]) -> Vec<u8> {
        let mut master = vec![0u8; 30];
        master[6..10].copy_from_slice(&volume.to_le_bytes());
        let mut body = chunk(TAG_MASTER, &master);
        body.extend(chunk(TAG_PROGRAM_NAME, b"Synthetic\0"));
        body.extend(chunk(103, b"C:\\x.dwp"));
        for z in zones {
            body.extend(z);
        }
        body.extend(chunk(TAG_END_PROGRAM, &[]));
        let mut file = b"DwPr".to_vec();
        file.extend(version.to_le_bytes());
        file.resize(HEADER_LEN, 0);
        file.extend(chunk(TAG_CONTAINER, &body));
        file
    }
    /// Version 36: no usable container; blocks follow a 94-byte preamble.
    fn build_legacy(zones: &[Vec<u8>]) -> Vec<u8> {
        let mut file = b"DwPr".to_vec();
        file.extend(0x24u32.to_le_bytes());
        file.resize(HEADER_LEN, 0);
        file.extend(TAG_CONTAINER.to_le_bytes());
        file.extend(u32::MAX.to_le_bytes());
        file.extend(0u32.to_le_bytes());
        file.extend(chunk(TAG_MASTER, &[0u8; 34]));
        assert_eq!(file.len(), 0x5E);
        file.extend(chunk(TAG_PROGRAM_NAME, b"Legacy"));
        for z in zones {
            file.extend(z);
        }
        file.extend(chunk(TAG_END_PROGRAM, &[]));
        file
    }
    fn parse(bytes: &[u8]) -> Result<SamplePreset, String> {
        read_dwp_bytes_with_loader(bytes, None, "x", wav_loader)
    }

    // ---- fixtures

    #[test]
    fn single_zone_fixture_matches_its_wav() {
        let p = read_dwp(Path::new("tests/fixtures/dwp/test.dwp")).unwrap();
        assert_eq!(p.name, "test");
        assert_eq!(p.zones.len(), 1);
        let z = &p.zones[0];
        assert_eq!((z.root, z.low, z.high), (60, 48, 72));
        assert_eq!((z.low_velocity, z.high_velocity), (1, 127));
        assert_eq!(z.sample_rate, 48_000);
        assert_eq!(z.loop_mode, LoopMode::None);
        assert_eq!(z.frames.len(), 24_064);
        assert_eq!(z.velocity_tracking, 1.0);
        assert!(!p.original_dwp.is_empty());
        let reference = wav_frames("tests/fixtures/dwp/test.wav");
        assert_eq!(reference.len(), 24_000);
        assert_eq!(&z.frames[..24_000], &reference[..]);
        assert!(z.frames[24_000..].iter().all(|f| *f == [0.0; 2]));
        // ENV 1/2 and LFO 1/2 are kept, filters are off.
        assert!(z.params.filters.iter().all(|f| f.kind == FilterKind::Off));
        for e in &z.params.env {
            assert!(e.sustain > 0.98 && e.release > 0.2 && e.attack == 0.0);
        }
        for l in &z.params.lfo {
            assert_eq!((l.shape, l.rate), (LfoShape::Triangle, 0.01));
        }
        assert!(z.params.matrix.iter().all(|m| m.source == ModSource::None));
        assert!(p.report.contains("Native DirectWave"));
    }

    #[test]
    fn multizone_fixture_matches_sources_and_envelopes() {
        let p = read_dwp(Path::new("tests/fixtures/multizone/converted/Zones.dwp")).unwrap();
        assert_eq!(p.name, "Zones");
        assert_eq!(p.zones.len(), 4);
        let expect = [
            ("Low soft", (48, 48, 59), (1, 63), [0.08, 0.12, 0.6, 0.22]),
            (
                "Low bright",
                (48, 48, 59),
                (64, 127),
                [0.005, 0.07, 0.8, 0.4],
            ),
            ("High soft", (60, 60, 72), (1, 63), [0.2, 0.15, 0.4, 0.25]),
            (
                "High bright",
                (60, 60, 72),
                (64, 127),
                [0.002, 0.15, 0.7, 0.6],
            ),
        ];
        for (z, (file, keys, vel, adsr)) in p.zones.iter().zip(expect) {
            assert_eq!((z.root, z.low, z.high), keys, "{file}");
            assert_eq!((z.low_velocity, z.high_velocity), vel, "{file}");
            assert_eq!(z.loop_mode, LoopMode::Continuous, "{file}");
            assert_eq!((z.loop_start, z.loop_end), (384, 4224), "{file}");
            assert_eq!((z.gain, z.pan, z.velocity_tracking), (1.0, 0.0, 1.0));
            let got = [z.adsr.attack, z.adsr.decay, z.adsr.sustain, z.adsr.release];
            for (g, e) in got.iter().zip(adsr) {
                assert!((g - e).abs() < 1e-5, "{file}: {got:?} vs {adsr:?}");
            }
            let source = wav_frames(&format!("tests/fixtures/multizone/source/{file}.wav"));
            assert_eq!(source.len(), 4_800);
            assert_eq!(&z.frames[..4_800], &source[..], "{file}");
            assert!(z.frames[4_800..].iter().all(|f| *f == [0.0; 2]));
        }
    }

    // ---- synthetic parameters

    #[test]
    fn envelopes_filters_lfos_and_matrix_survive() {
        let filter = |kind: u32, cutoff: f32, emph: f32| {
            let mut f = kind.to_le_bytes().to_vec();
            f.extend(floats(&[cutoff, emph, 0.0, 0.0, 0.0]));
            chunk(ZONE_FILTER, &f)
        };
        let lfo = |shape: u32, rate: f32| {
            let mut l = shape.to_le_bytes().to_vec();
            l.extend(floats(&[rate, 0.0, 0.0, 0.0]));
            chunk(ZONE_LFO, &l)
        };
        let knob = |seconds: f64| (seconds / 9.07).powf(1.0 / 3.96) as f32;
        let env = |tag: u32, a: f64, d: f64, s: f32, r: f64| {
            chunk(tag, &floats(&[knob(a), knob(d), s, knob(r)]))
        };
        let extras = vec![
            filter(1, 0.5, 0.25),
            filter(3, 1.0, 1.5),
            env(ZONE_AMP_ENV, 0.08, 0.12, 0.6, 0.22),
            env(ZONE_ENV, 0.5, 1.0, 0.3, 2.0),
            env(ZONE_ENV, 0.01, 0.02, 1.0, 0.03),
            lfo(2, 0.5),
            lfo(6, 1.0),
            // Env1 -> Fl1 cutoff +80 %, inverted, LFO1 -> pitch, LFO2 -> Fl2 emph,
            // Key -> pan, Random -> gain, unknown source, centre (inactive).
            matrix_slot(9, 7, 0.9),
            matrix_slot(10, 10, 0.1),
            matrix_slot(11, 1, 0.75),
            matrix_slot(12, 11, 1.0),
            matrix_slot(1, 3, 0.25),
            matrix_slot(15, 2, 0.75),
            matrix_slot(4, 1, 0.5714),
            matrix_slot(2, 3, 0.5),
        ];
        let p = parse(&build(0x26, 1.0, &[pcm_zone(300, 2, 2, &extras)])).unwrap();
        let z = &p.zones[0];
        assert_eq!(z.frames.len(), 300);

        let (f1, f2) = (&z.params.filters[0], &z.params.filters[1]);
        assert_eq!(f1.kind, FilterKind::Lowpass);
        assert!((f1.cutoff - 45.0 * (20000.0f64 / 45.0).sqrt()).abs() < 1e-6);
        assert!((f1.emphasis - 0.25).abs() < 1e-9);
        assert_eq!(f2.kind, FilterKind::Bandpass);
        assert!((f2.cutoff - 20000.0).abs() < 1e-6);
        assert_eq!(f2.emphasis, 1.0);

        let amp = [z.adsr.attack, z.adsr.decay, z.adsr.sustain, z.adsr.release];
        for (g, e) in amp.iter().zip([0.08, 0.12, 0.6, 0.22]) {
            assert!((g - e).abs() < 1e-5, "{amp:?}");
        }
        let (e1, e2) = (&z.params.env[0], &z.params.env[1]);
        assert!((e1.attack - 0.5).abs() < 1e-5 && (e1.decay - 1.0).abs() < 1e-5);
        assert!((e1.sustain - 0.3).abs() < 1e-6 && (e1.release - 2.0).abs() < 1e-5);
        assert!((e2.attack - 0.01).abs() < 1e-5 && e2.sustain == 1.0);

        let (l1, l2) = (&z.params.lfo[0], &z.params.lfo[1]);
        assert_eq!(l1.shape, LfoShape::Triangle);
        assert!((l1.rate - 5.0).abs() < 1e-9);
        assert_eq!(l2.shape, LfoShape::Random);
        assert!((l2.rate - 20.0).abs() < 1e-9);

        let cutoff = 0.512 * 10.6667 / 6.0;
        let slots: Vec<_> = z
            .params
            .matrix
            .iter()
            .filter(|m| m.source != ModSource::None)
            .map(|m| (m.source, m.target, m.amount))
            .collect();
        let expect = [
            (ModSource::Env1, ModTarget::Cutoff1, cutoff),
            (ModSource::Env2, ModTarget::Cutoff2, -cutoff),
            (ModSource::Lfo1, ModTarget::Pitch, 0.25),
            (ModSource::Lfo2, ModTarget::Emphasis2, 1.0),
            (ModSource::Key, ModTarget::Pan, -0.125),
            (ModSource::Random, ModTarget::Gain, 0.125),
        ];
        assert_eq!(slots.len(), expect.len(), "{slots:?}");
        for (got, want) in slots.iter().zip(expect) {
            assert_eq!((got.0, got.1), (want.0, want.1));
            assert!((got.2 - want.2).abs() < 1e-4, "{got:?} vs {want:?}");
        }
        assert!(p.report.contains("modulation slots"));
        assert_eq!(z.velocity_tracking, 1.0);
    }

    #[test]
    fn velocity_to_gain_sets_tracking_or_routes_when_inverted() {
        let tracked = pcm_zone(64, 1, 2, &[matrix_slot(2, 2, 0.75)]);
        let inverted = pcm_zone(64, 1, 2, &[matrix_slot(2, 2, 0.0)]);
        let flat = pcm_zone(64, 1, 2, &[matrix_slot(2, 2, 0.5)]);
        let p = parse(&build(0x26, 1.0, &[tracked, inverted, flat])).unwrap();
        assert!((p.zones[0].velocity_tracking - 0.125).abs() < 1e-9);
        assert!(
            p.zones[0]
                .params
                .matrix
                .iter()
                .all(|m| m.source == ModSource::None)
        );
        let z = &p.zones[1];
        assert_eq!(z.velocity_tracking, 0.0);
        assert_eq!(
            (
                z.params.matrix[0].source,
                z.params.matrix[0].target,
                z.params.matrix[0].amount
            ),
            (ModSource::Velocity, ModTarget::Gain, -1.0)
        );
        assert_eq!(p.zones[2].velocity_tracking, 1.0);
    }

    #[test]
    fn default_amp_envelope_and_mapping_edge_cases() {
        let version_default = |version: u32, d: f32, r: f32| {
            let z = zone(&[
                mapping(200, 90, 10, 0, 0, f32::NAN, f32::INFINITY),
                chunk(ZONE_AMP_ENV, &floats(&[0.0, d, 1.0, r])),
                format(64, 1, 0, 0, 0, 16),
                chunk(ZONE_PCM, &planar(1, 64, 2, GUARD)),
            ]);
            parse(&build(version, 1.0, &[z])).unwrap().zones.remove(0)
        };
        for (version, d, r) in [(0x25, 0.5, 0.25), (0x26, 1.0, 0.18)] {
            assert_eq!(version_default(version, d, r).adsr, Adsr::default());
        }
        // The same numbers are a real envelope in the other version.
        assert_ne!(version_default(0x26, 0.5, 0.25).adsr, Adsr::default());
        let z = version_default(0x26, 1.0, 0.18);
        assert_eq!((z.root, z.low, z.high), (127, 10, 90));
        assert_eq!((z.low_velocity, z.high_velocity), (1, 1));
        assert_eq!((z.gain, z.pan), (1.0, 0.0));
    }

    #[test]
    fn loop_modes_and_one_shot() {
        let looped = |mode: u32, start: u32, end: u32| {
            let z = zone(&[
                mapping(60, 0, 127, 0, 127, 1.0, 0.5),
                format(200, 1, mode, start, end, 16),
                chunk(ZONE_PCM, &planar(1, 200, 2, GUARD)),
            ]);
            parse(&build(0x26, 1.0, &[z])).unwrap().zones.remove(0)
        };
        let z = looped(3, 10, 99);
        assert_eq!(
            (z.loop_mode, z.loop_start, z.loop_end),
            (LoopMode::Sustain, 10, 100)
        );
        assert_eq!(looped(4, 10, 99).loop_mode, LoopMode::PingPong);
        let z = looped(2, 10, 5000);
        assert_eq!((z.loop_mode, z.loop_end), (LoopMode::Continuous, 200));
        let z = looped(1, 10, 99);
        assert_eq!(z.loop_mode, LoopMode::None);
        assert!(z.params.one_shot);
        // Degenerate loops fall back to no loop.
        assert_eq!(looped(2, 50, 50).loop_mode, LoopMode::None);
        assert_eq!(looped(2, 500, 600).loop_mode, LoopMode::None);
        assert_eq!(looped(9, 10, 99).loop_mode, LoopMode::None);
    }

    #[test]
    fn master_volume_becomes_program_decibels() {
        let z = pcm_zone(64, 1, 2, &[]);
        let p = parse(&build(0x26, 0.5, &[z.clone()])).unwrap();
        assert!((p.program.volume_db + 6.0206).abs() < 1e-3);
        let p = parse(&build(0x26, 100.0, &[z.clone()])).unwrap();
        assert_eq!(p.program.volume_db, 12.0);
        let p = parse(&build(0x26, 1.0, &[z])).unwrap();
        assert_eq!(p.program.volume_db, 0.0);
    }

    // ---- sample storage

    #[test]
    fn planar_pcm_16_24_and_float_keep_channels_apart() {
        for (channels, width) in [(2, 2), (1, 3), (2, 4), (2, 3), (1, 4)] {
            let p = parse(&build(0x26, 1.0, &[pcm_zone(500, channels, width, &[])])).unwrap();
            let z = &p.zones[0];
            assert_eq!(z.frames.len(), 500, "{channels}ch {width}B");
            for i in 0..500 {
                let l = test_sample(0, i);
                let r = if channels == 2 { test_sample(1, i) } else { l };
                assert_eq!(z.frames[i], [l, r], "{channels}ch {width}B frame {i}");
            }
            if channels == 2 {
                assert!(z.frames.iter().any(|f| f[0] != f[1]));
            }
        }
    }

    #[test]
    fn pcm_without_guard_frames_is_tolerated() {
        let z = zone(&[
            mapping(60, 0, 127, 0, 127, 1.0, 0.5),
            format(100, 2, 0, 0, 0, 16),
            chunk(ZONE_PCM, &planar(2, 100, 2, 0)),
        ]);
        let p = parse(&build(0x26, 1.0, &[z])).unwrap();
        assert_eq!(p.zones[0].frames[7], [test_sample(0, 7), test_sample(1, 7)]);
    }

    #[test]
    fn legacy_header_reads_blocks_after_the_preamble() {
        let p = parse(&build_legacy(&[pcm_zone(80, 1, 2, &[])])).unwrap();
        assert_eq!(p.name, "Legacy");
        assert_eq!(p.zones[0].frames.len(), 80);
        assert_eq!(p.zones[0].frames[3], [test_sample(0, 3); 2]);
    }

    #[test]
    fn external_samples_are_found_beside_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &Path, level: i16| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: 22_050,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(path, spec).unwrap();
            for i in 0..400i16 {
                w.write_sample(level + i).unwrap();
                w.write_sample(-level - i).unwrap();
            }
            w.finalize().unwrap();
        };
        let external = |stored: &str, name: &str| {
            zone(&[
                mapping(60, 0, 127, 0, 127, 1.0, 0.5),
                chunk(ZONE_NAME, name.as_bytes()),
                chunk(ZONE_PATH, stored.as_bytes()),
                format(0, 2, 2, 100, 299, 16),
            ])
        };
        let bytes = build(
            0x26,
            1.0,
            &[
                external("D:\\Packs\\Drums\\kick.wav", "kick"),
                external("/old/place/Snares/snare.wav", "snare"),
                external("", "fallback"),
            ],
        );
        let read = |dir: &Path| read_dwp_bytes_with_loader(&bytes, Some(dir), "song", wav_loader);

        assert!(read(dir.path()).unwrap_err().contains("kick.wav"));
        write(&dir.path().join("song").join("kick.wav"), 1000);
        write(&dir.path().join("Snares").join("snare.wav"), 2000);
        write(&dir.path().join("fallback.wav"), 3000);
        let p = read(dir.path()).unwrap();
        let first = |i: usize| p.zones[i].frames[0];
        assert_eq!(first(0), [1000.0 / 32768.0, -1000.0 / 32768.0]);
        assert_eq!(first(1), [2000.0 / 32768.0, -2000.0 / 32768.0]);
        assert_eq!(first(2), [3000.0 / 32768.0, -3000.0 / 32768.0]);
        let z = &p.zones[0];
        assert_eq!((z.sample_rate, z.frames.len()), (22_050, 400));
        assert_eq!(
            (z.loop_mode, z.loop_start, z.loop_end),
            (LoopMode::Continuous, 100, 300)
        );

        // A file next to the program wins over the subfolder.
        write(&dir.path().join("kick.wav"), 5000);
        let p = read(dir.path()).unwrap();
        assert_eq!(p.zones[0].frames[0][0], 5000.0 / 32768.0);

        // And the whole thing through the file API with the production loader.
        let path = dir.path().join("song.dwp");
        std::fs::write(&path, &bytes).unwrap();
        let p = read_dwp(&path).unwrap();
        assert_eq!(p.zones.len(), 3);
        assert_eq!(p.original_dwp.as_slice(), bytes.as_slice());
    }

    #[test]
    fn missing_audio_without_a_folder_is_an_error() {
        let z = zone(&[
            mapping(60, 0, 127, 0, 127, 1.0, 0.5),
            chunk(ZONE_PATH, b"a.wav"),
        ]);
        let err = parse(&build(0x26, 1.0, &[z])).unwrap_err();
        assert!(err.contains("a.wav"), "{err}");
    }

    // ---- malformed input

    #[test]
    fn malformed_files_fail_cleanly() {
        assert!(!is_dwp_magic(b""));
        assert!(is_dwp_magic(b"DwPr...."));
        for bytes in [&b""[..], b"DwPr", b"RIFFxxxxxxxxxxxxxxxx", &[0u8; 100]] {
            assert!(parse(bytes).is_err());
        }
        // Valid header and empty program.
        assert!(
            parse(&build(0x26, 1.0, &[]))
                .unwrap_err()
                .contains("no zones")
        );
        // A zone cut short; and a bogus container length is tolerated.
        let mut file = build(0x26, 1.0, &[pcm_zone(64, 1, 2, &[])]);
        assert!(parse(&file[..file.len() - 30]).is_err());
        file[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&file).unwrap().zones.len(), 1);
        // PCM that does not match the declared format.
        let z = zone(&[
            mapping(60, 0, 127, 0, 127, 1.0, 0.5),
            format(100, 2, 0, 0, 0, 16),
            chunk(ZONE_PCM, &[0u8; 123]),
        ]);
        assert!(parse(&build(0x26, 1.0, &[z])).unwrap_err().contains("PCM"));
        // Corrupt FLAC.
        let z = zone(&[
            mapping(60, 0, 127, 0, 127, 1.0, 0.5),
            format(100, 2, 0, 0, 0, 16),
            chunk(ZONE_FLAC, b"fLaCnonsense"),
        ]);
        assert!(parse(&build(0x26, 1.0, &[z])).unwrap_err().contains("FLAC"));
        // Embedded FLAC cut short of its declared length.
        let z = zone(&[chunk(ZONE_FLAC, &[255, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3])]);
        assert!(
            parse(&build(0x26, 1.0, &[z]))
                .unwrap_err()
                .contains("truncated")
        );
    }

    #[test]
    fn every_truncation_of_a_real_file_is_handled() {
        for path in [
            "tests/fixtures/dwp/test.dwp",
            "tests/fixtures/multizone/converted/Zones.dwp",
        ] {
            let bytes = std::fs::read(path).unwrap();
            assert!(parse(&bytes).is_ok());
            for cut in (0..bytes.len()).step_by(61) {
                // Never panics; a file missing its audio can only be an error.
                let result = parse(&bytes[..cut]);
                if cut < 0xB33 || cut + 100 < bytes.len() {
                    assert!(result.is_err(), "{path} cut at {cut}");
                }
            }
        }
    }
}
