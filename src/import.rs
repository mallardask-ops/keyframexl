use keyframexl::sequence::SamplePreset;
use std::{path::Path, process::Command};

pub fn dwp(path: &Path) -> Result<SamplePreset, String> {
    let converter = std::env::var_os("KEYFRAMEXL_CONVERTER")
        .or_else(|| {
            let root = std::env::var_os("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".local/share"))
                })?;
            let path = root.join("keyframexl/tools/convertwithmoss-20.3.0/bin/ConvertWithMoss");
            path.is_file().then(|| path.into_os_string())
        })
        .unwrap_or_else(|| "ConvertWithMoss".into());
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let output = Command::new(converter)
        .args(["-s", "directwave", "-d", "sfz"])
        .arg(path)
        .arg(temp.path())
        .output()
        .map_err(|e| {
            format!("ConvertWithMoss unavailable: {e}. Set KEYFRAMEXL_CONVERTER to its executable.")
        })?;
    if !output.status.success() {
        return Err(format!(
            "Conversion failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let paths = std::fs::read_dir(temp.path())
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sfz")))
        .collect::<Vec<_>>();
    if paths.len() != 1 {
        return Err("Converter did not produce exactly one SFZ".into());
    }
    let text = std::fs::read_to_string(&paths[0]).map_err(|e| e.to_string())?;
    let mut preset = read_sfz(&paths[0], &text)?;
    preset.name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into();
    preset.original_dwp = std::sync::Arc::new(std::fs::read(path).map_err(|e| e.to_string())?);
    Ok(preset)
}

use keyframexl::sampler::{Adsr, LoopMode, Program, Zone, ZoneParams};

use keyframexl::sampler::{Filter, FilterKind, FilterRouting, Lfo, ModSource, ModTarget};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
type Opcodes = BTreeMap<String, String>;

struct WavSpec {
    sample_rate: u32,
}

/// Decode a WAV, FLAC or Ogg Vorbis file to stereo frames.
pub fn read_audio(path: &Path) -> Result<(u32, Vec<[f32; 2]>), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let unreadable =
        |e: &dyn std::fmt::Display| format!("Unreadable audio {}: {e}", path.display());
    match ext.as_str() {
        "wav" => read_wav(&std::fs::read(path).map_err(|e| unreadable(&e))?),
        "flac" => {
            let mut r = claxon::FlacReader::open(path).map_err(|e| unreadable(&e))?;
            let info = r.streaminfo();
            let ch = info.channels as usize;
            let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32;
            let samples: Vec<f32> = r
                .samples()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| unreadable(&e))?;
            Ok((info.sample_rate, to_stereo(&samples, ch)))
        }
        "ogg" => decode_ogg(&std::fs::read(path).map_err(|e| unreadable(&e))?)
            .map_err(|e| unreadable(&e)),
        _ => Err(format!("{} is not WAV, FLAC or Ogg", path.display())),
    }
}

/// Decode an Ogg Vorbis stream to stereo frames: lewton (pure Rust) first,
/// libvorbis when lewton cannot read the headers (floor-0 streams from old
/// encoders such as the Windows ACM codec FL Studio used).
fn decode_ogg(bytes: &[u8]) -> Result<(u32, Vec<[f32; 2]>), String> {
    let cleaned = first_stream(bytes);
    let bytes: &[u8] = &cleaned;
    let lewton = || -> Result<(u32, Vec<[f32; 2]>), String> {
        let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
            .map_err(|e| e.to_string())?;
        let ch = r.ident_hdr.audio_channels as usize;
        let rate = r.ident_hdr.audio_sample_rate;
        let mut samples = Vec::new();
        while let Some(pkt) = r.read_dec_packet_itl().map_err(|e| e.to_string())? {
            samples.extend(pkt.into_iter().map(|v| v as f32 / 32768.0));
        }
        Ok((rate, to_stereo(&samples, ch)))
    };
    lewton().or_else(|first| {
        let mut dec = vorbis_rs::VorbisDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("{first}; libvorbis: {e}"))?;
        let ch = dec.channels().get() as usize;
        let rate = dec.sampling_frequency().get();
        let mut samples = Vec::new();
        while let Some(block) = dec
            .decode_audio_block()
            .map_err(|e| format!("libvorbis: {e}"))?
        {
            let s = block.samples();
            for i in 0..s[0].len() {
                for c in s.iter().take(ch) {
                    samples.push(c[i]);
                }
            }
        }
        Ok((rate, to_stereo(&samples, ch)))
    })
}

/// Keep only the pages of the first logical stream. Some FL Studio samples
/// carry a stray empty page with another serial before the last page, which
/// stops every decoder; whole pages are copied, so their CRCs stay valid.
fn first_stream(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let page = |i: usize| -> Option<(u32, usize)> {
        let h = bytes.get(i..i + 27)?;
        if &h[..4] != b"OggS" {
            return None;
        }
        let serial = u32::from_le_bytes([h[14], h[15], h[16], h[17]]);
        let nseg = h[26] as usize;
        let body: usize = bytes
            .get(i + 27..i + 27 + nseg)?
            .iter()
            .map(|b| *b as usize)
            .sum();
        Some((serial, 27 + nseg + body))
    };
    let Some((first, _)) = page(0) else {
        return bytes.into();
    };
    let (mut i, mut out, mut dropped) = (0, Vec::with_capacity(bytes.len()), false);
    while let Some((serial, len)) = page(i) {
        let end = (i + len).min(bytes.len());
        if serial == first {
            out.extend_from_slice(&bytes[i..end]);
        } else {
            dropped = true;
        }
        i = end;
    }
    if dropped { out.into() } else { bytes.into() }
}

/// WAVE_FORMAT_OGG_VORBIS_MODE_1 / 1+ : the data chunk is a whole Ogg Vorbis
/// stream (FL Studio stores many stock samples this way).
const OGG_IN_WAV: [u16; 2] = [0x674F, 0x676F];

fn to_stereo(samples: &[f32], ch: usize) -> Vec<[f32; 2]> {
    match ch {
        0 => vec![],
        1 => samples.iter().map(|v| [*v, *v]).collect(),
        _ => samples.chunks_exact(ch).map(|f| [f[0], f[1]]).collect(),
    }
}

/// A sampler preset made of one audio file (FL: dropping a sample on a channel).
pub fn sample_preset(path: &Path) -> Result<SamplePreset, String> {
    let (rate, frames) = read_audio(path)?;
    if frames.is_empty() {
        return Err(format!("{} has no audio", path.display()));
    }
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut p = SamplePreset::from_sample(&name, rate, frames);
    p.report = format!(
        "Loaded {} ({} Hz).",
        path.file_name().unwrap_or_default().to_string_lossy(),
        rate
    );
    p.validate()?;
    Ok(p)
}

/// Lenient WAV reader. Converters write `fmt ` chunks of 16, 18, 20 or 40
/// bytes and extra chunks (LIST, smpl, cue); stricter readers reject some of
/// them. Supports PCM 8/16/24/32-bit, float 32/64 and WAVE_FORMAT_EXTENSIBLE.
/// Mono is duplicated to both sides; channels beyond two are dropped.
pub(crate) fn read_wav(d: &[u8]) -> Result<(u32, Vec<[f32; 2]>), String> {
    let u16le = |o: usize| -> Result<u16, String> {
        d.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| "truncated".to_string())
    };
    let u32le = |o: usize| -> Result<u32, String> {
        d.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| "truncated".to_string())
    };
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= d.len() {
        let id = &d[at..at + 4];
        let len = u32le(at + 4)? as usize;
        let body = at + 8;
        let end = (body + len).min(d.len());
        match id {
            b"fmt " => {
                if len < 16 {
                    return Err(format!("fmt chunk of {len} bytes"));
                }
                let mut tag = u16le(body)?;
                if tag == 0xFFFE && len >= 26 {
                    // WAVE_FORMAT_EXTENSIBLE: the real format is the subformat GUID's first two bytes.
                    tag = u16le(body + 24)?;
                }
                fmt = Some((tag, u16le(body + 2)?, u32le(body + 4)?, u16le(body + 14)?));
            }
            b"data" => data = Some(&d[body..end]),
            _ => {}
        }
        at = body + len + (len & 1);
    }
    let (tag, channels, rate, bits) = fmt.ok_or("no fmt chunk")?;
    let data = data.ok_or("no data chunk")?;
    if OGG_IN_WAV.contains(&tag) {
        return decode_ogg(data).map_err(|e| format!("Ogg Vorbis inside WAV: {e}"));
    }
    if channels == 0 || rate == 0 {
        return Err("zero channels or sample rate".into());
    }
    let bytes = (bits as usize).div_ceil(8);
    let sample = |b: &[u8]| -> Result<f32, String> {
        Ok(match (tag, bits) {
            (1, 8) => (b[0] as f32 - 128.0) / 128.0,
            (1, 16) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (3, 64) => f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
            _ => return Err(format!("unsupported sample format {tag} at {bits} bits")),
        })
    };
    let frame_bytes = bytes * channels as usize;
    let mut frames = Vec::with_capacity(data.len() / frame_bytes.max(1));
    for f in data.chunks_exact(frame_bytes) {
        let l = sample(&f[..bytes])?;
        let r = if channels > 1 {
            sample(&f[bytes..bytes * 2])?
        } else {
            l
        };
        frames.push([l, r]);
    }
    if frames.is_empty() {
        return Err("no audio frames".into());
    }
    Ok((rate, frames))
}

/// Map SFZ sound opcodes (as ConvertWithMoss writes them from a .dwp) onto the
/// DirectWave-style zone: filters, envelopes and LFOs routed through the matrix.
fn sound_params(
    op: &Opcodes,
    get: &dyn Fn(&str, f64) -> Result<f64, String>,
) -> Result<ZoneParams, String> {
    let mut p = ZoneParams::default();
    p.key_track = (get("pitch_keytrack", 100.0)? / 100.0).clamp(-1.0, 1.0);
    p.reverse = op.get("direction").is_some_and(|d| d == "reverse");
    p.one_shot = op.get("loop_mode").is_some_and(|m| m == "one_shot");
    if let (Some(g), Some(off)) = (op.get("group"), op.get("off_by"))
        && g == off
    {
        p.mute_group = g.parse::<u8>().unwrap_or(0);
    }
    // Filter: SFZ names the slope in poles; four poles = both filters in series.
    if let Some(kind) = op.get("fil_type") {
        let (k, poles) = match kind.split('_').collect::<Vec<_>>().as_slice() {
            [t, p] => (*t, p.trim_end_matches('p').parse::<u8>().unwrap_or(2)),
            _ => (kind.as_str(), 2),
        };
        let kind = match k {
            "lpf" => FilterKind::Lowpass,
            "hpf" => FilterKind::Highpass,
            "bpf" => FilterKind::Bandpass,
            "brf" => FilterKind::Notch,
            "apf" => FilterKind::Allpass,
            _ => return Err(format!("Unsupported filter type: {kind}")),
        };
        // Resonance in dB -> emphasis via Q = 0.707 * 10^(dB/20), k = 1/Q.
        let q = 0.707 * 10f64.powf(get("resonance", 0.0)?.clamp(0.0, 40.0) / 20.0);
        let emphasis = ((2.0 - 1.0 / q) / 1.96).clamp(0.0, 1.0);
        let f = Filter {
            kind,
            cutoff: get("cutoff", 20000.0)?.clamp(20.0, 20000.0),
            emphasis,
        };
        p.filters[0] = f.clone();
        if poles >= 4 {
            p.filters[1] = f;
            p.routing = FilterRouting::Serial;
        }
    }
    let oct = |cents: f64, target: ModTarget| cents / 1200.0 / target.full_scale();
    let ft = get("fil_keytrack", 0.0)?;
    if ft != 0.0 {
        // Key source is (key - 60) / 60, so cents/key * 60 keys / (1200 * scale).
        p.route(
            ModSource::Key,
            ModTarget::Cutoff1,
            ft * 60.0 / 1200.0 / ModTarget::Cutoff1.full_scale(),
        );
    }
    let fv = get("fil_veltrack", 0.0)?;
    if fv != 0.0 {
        p.route(
            ModSource::Velocity,
            ModTarget::Cutoff1,
            oct(fv, ModTarget::Cutoff1),
        );
    }
    let env = |prefix: &str| -> Result<Adsr, String> {
        Ok(Adsr {
            delay: get(&format!("{prefix}_delay"), 0.0)?,
            attack: get(&format!("{prefix}_attack"), 0.0)?,
            hold: get(&format!("{prefix}_hold"), 0.0)?,
            decay: get(&format!("{prefix}_decay"), 0.0)?,
            sustain: (get(&format!("{prefix}_sustain"), 100.0)? / 100.0).clamp(0.0, 1.0),
            release: get(&format!("{prefix}_release"), 0.0)?,
            ..Adsr::default()
        })
    };
    let fd = get("fileg_depth", 0.0)?;
    if fd != 0.0 {
        p.env[0] = env("fileg")?;
        p.route(
            ModSource::Env1,
            ModTarget::Cutoff1,
            oct(fd, ModTarget::Cutoff1),
        );
    }
    let pd = get("pitcheg_depth", 0.0)?;
    if pd != 0.0 {
        p.env[1] = env("pitcheg")?;
        p.route(ModSource::Env2, ModTarget::Pitch, oct(pd, ModTarget::Pitch));
    }
    // Up to two LFOs, as in a DirectWave zone; extra SFZ LFOs share the last one.
    let mut used = 0;
    for (prefix, target, amount) in [
        (
            "pitchlfo",
            ModTarget::Pitch,
            oct(get("pitchlfo_depth", 0.0)?, ModTarget::Pitch),
        ),
        (
            "amplfo",
            ModTarget::Gain,
            -(1.0 - 10f64.powf(-get("amplfo_depth", 0.0)?.abs() / 20.0)),
        ),
        (
            "fillfo",
            ModTarget::Cutoff1,
            oct(get("fillfo_depth", 0.0)?, ModTarget::Cutoff1),
        ),
    ] {
        if amount == 0.0 {
            continue;
        }
        let slot = used.min(1);
        if used < 2 {
            p.lfo[slot] = Lfo {
                rate: get(&format!("{prefix}_freq"), 5.0)?.clamp(0.01, 50.0),
                ..Lfo::default()
            };
        }
        p.route(
            if slot == 0 {
                ModSource::Lfo1
            } else {
                ModSource::Lfo2
            },
            target,
            amount,
        );
        used += 1;
    }
    p.validate().map_err(str::to_owned)?;
    Ok(p)
}

// This reads the normalized SFZ emitted by the converter, including inherited groups.
fn regions(text: &str) -> Result<Vec<Opcodes>, String> {
    let text = text
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = text.as_bytes();
    let mut markers = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'<' {
            let end = text[at..].find('>').ok_or("Unclosed SFZ header")? + at;
            markers.push((at, end + 1, true));
            at = end + 1;
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            if at < bytes.len() && bytes[at] == b'=' {
                markers.push((start, at + 1, false));
                at += 1;
            }
        } else {
            at += 1
        }
    }
    let mut global = Opcodes::new();
    let mut group = Opcodes::new();
    let mut region = Opcodes::new();
    let mut control = Opcodes::new();
    let mut section = "";
    let mut result = Vec::new();
    for (i, &(start, end, header)) in markers.iter().enumerate() {
        if header {
            if section == "region" {
                result.push(std::mem::take(&mut region));
            }
            section = &text[start + 1..end - 1];
            match section {
                "global" => {}
                "group" => {
                    group = global.clone();
                }
                "region" => {
                    region = global.clone();
                    region.extend(group.clone());
                    region.extend(control.clone());
                }
                "control" => {}
                other => return Err(format!("Unsupported converted SFZ section: {other}")),
            }
        } else {
            let key = text[start..end - 1].trim().to_owned();
            let next = markers.get(i + 1).map(|m| m.0).unwrap_or(text.len());
            let value = text[end..next].trim().to_owned();
            match section {
                "global" => {
                    global.insert(key, value);
                }
                "group" => {
                    group.insert(key, value);
                }
                "region" => {
                    region.insert(key, value);
                }
                "control" => {
                    control.insert(key, value);
                }
                _ => return Err("SFZ opcode outside a section".into()),
            }
        }
    }
    if section == "region" {
        result.push(region)
    }
    if result.is_empty() {
        return Err("Converted instrument contains no regions".into());
    }
    Ok(result)
}
fn read_sfz(path: &Path, text: &str) -> Result<SamplePreset, String> {
    let mut zones = Vec::new();
    let mut omitted = BTreeSet::new();
    let mut bend_range: f64 = 200.0;
    let mut warnings: Vec<String> = Vec::new();
    for op in regions(text)? {
        let get = |key: &str, default: f64| -> Result<f64, String> {
            op.get(key)
                .map(|v| v.parse::<f64>().map_err(|_| format!("Invalid {key}: {v}")))
                .unwrap_or(Ok(default))
        };
        let midi = |key: &str, default: u8| -> Result<u8, String> {
            let v = get(key, default as f64)?;
            if !v.is_finite() || v.fract() != 0.0 || !(0.0..=127.0).contains(&v) {
                return Err(format!("Invalid MIDI range: {key}"));
            }
            Ok(v as u8)
        };
        let file = op.get("sample").ok_or("SFZ sample missing")?;
        let folder = op.get("default_path").map(String::as_str).unwrap_or("");
        let source = path
            .parent()
            .ok_or("SFZ folder missing")?
            .join(folder.replace('\\', "/"))
            .join(file.replace('\\', "/"));
        let wav = std::fs::read(&source)
            .map_err(|e| format!("Missing sample {}: {e}", source.display()))?;
        let (sample_rate, frames) =
            read_wav(&wav).map_err(|e| format!("Unreadable sample {}: {e}", source.display()))?;
        let spec = WavSpec { sample_rate };
        let pos = |key: &str, default: usize| -> Result<usize, String> {
            let v = get(key, default as f64)?;
            if !v.is_finite() || v < 0.0 || v.fract() != 0.0 || v > frames.len() as f64 {
                return Err(format!("Invalid sample frame: {key}={v}"));
            }
            Ok(v as usize)
        };
        // Loop points are clamped to the file; bad ones are switched off below.
        let loop_pos = |key: &str, default: usize| -> Result<usize, String> {
            let v = get(key, default as f64)?;
            if !v.is_finite() || v < 0.0 {
                return Err(format!("Invalid sample frame: {key}={v}"));
            }
            Ok((v as usize).min(frames.len()))
        };
        let end = (pos("end", frames.len())?.saturating_add(1)).min(frames.len());
        let mode = match op.get("loop_mode").map(String::as_str).unwrap_or("no_loop") {
            "no_loop" => LoopMode::None,
            "loop_continuous" => LoopMode::Continuous,
            "loop_sustain" => LoopMode::Sustain,
            "one_shot" => LoopMode::None,
            v => return Err(format!("Unsupported loop mode: {v}")),
        };
        let mode = if op.get("loop_type").is_some_and(|v| v == "alternate") {
            if mode == LoopMode::Sustain {
                return Err("Sustain ping-pong loops are not yet supported".into());
            }
            LoopMode::PingPong
        } else {
            mode
        };
        if op
            .get("loop_type")
            .is_some_and(|v| v != "alternate" && v != "forward")
        {
            return Err("Unsupported loop type".into());
        }
        // `key=N` is shorthand for lokey=hikey=pitch_keycenter=N (ConvertWithMoss
        // writes it for single-key zones); explicit opcodes still win.
        let key = op
            .contains_key("key")
            .then(|| midi("key", 60))
            .transpose()?;
        let mut zone = Zone {
            name: file.clone(),
            root: midi("pitch_keycenter", key.unwrap_or(60))?,
            low: midi("lokey", key.unwrap_or(0))?,
            high: midi("hikey", key.unwrap_or(127))?,
            low_velocity: midi("lovel", 1)?,
            high_velocity: midi("hivel", 127)?,
            sample_rate: spec.sample_rate,
            frames: Arc::new(frames.clone()),
            offset: pos("offset", 0)?,
            end,
            loop_mode: mode,
            loop_start: loop_pos("loop_start", 0)?,
            loop_end: loop_pos("loop_end", end.saturating_sub(1))?
                .saturating_add(1)
                .min(end),
            adsr: Adsr {
                delay: get("ampeg_delay", 0.0)?,
                hold: get("ampeg_hold", 0.0)?,
                attack: get("ampeg_attack", 0.0)?,
                decay: get("ampeg_decay", 0.0)?,
                sustain: (get("ampeg_sustain", 100.0)? / 100.0).clamp(0.0, 1.0),
                release: get("ampeg_release", 0.0)?,
                ..Adsr::default()
            },
            gain: 10.0_f64.powf(get("volume", 0.0)? / 20.0) * get("amplitude", 100.0)? / 100.0,
            pan: get("pan", 0.0)? / 100.0,
            tune_cents: get("tune", 0.0)? + get("transpose", 0.0)? * 100.0,
            velocity_tracking: get("amp_veltrack", 100.0)? / 100.0,
            params: sound_params(&op, &get)?,
        };
        // Some presets ship broken loop points (start past end, zero length).
        // Play the zone without its loop instead of refusing the whole preset.
        if zone.loop_mode != LoopMode::None
            && (zone.loop_start < zone.offset
                || zone.loop_end > zone.end
                || zone.loop_end <= zone.loop_start + 1)
        {
            warnings.push(format!(
                "{file}: loop {}..{} is outside the sample, loop off",
                zone.loop_start, zone.loop_end
            ));
            zone.loop_mode = LoopMode::None;
            zone.loop_start = zone.offset;
            zone.loop_end = zone.end;
        }
        zone.validate().map_err(|e| format!("{file}: {e}"))?;
        if let Some(cents) = op
            .get("bend_up")
            .or(op.get("bend_down"))
            .and_then(|v| v.parse::<f64>().ok())
        {
            bend_range = bend_range.max(cents.abs().min(4800.0));
        }
        for key in op.keys() {
            if ![
                "sample",
                "default_path",
                "pitch_keycenter",
                "key",
                "lokey",
                "hikey",
                "lovel",
                "hivel",
                "offset",
                "end",
                "loop_mode",
                "loop_type",
                "loop_start",
                "loop_end",
                "ampeg_attack",
                "ampeg_decay",
                "ampeg_sustain",
                "ampeg_release",
                "volume",
                "amplitude",
                "pan",
                "tune",
                "transpose",
                "amp_veltrack",
                "ampeg_delay",
                "ampeg_hold",
                "bend_up",
                "bend_down",
                "direction",
                "pitch_keytrack",
                "group",
                "off_by",
                "fil_type",
                "cutoff",
                "resonance",
                "fil_keytrack",
                "fil_veltrack",
                "fileg_delay",
                "fileg_attack",
                "fileg_hold",
                "fileg_decay",
                "fileg_sustain",
                "fileg_release",
                "fileg_depth",
                "pitcheg_delay",
                "pitcheg_attack",
                "pitcheg_hold",
                "pitcheg_decay",
                "pitcheg_sustain",
                "pitcheg_release",
                "pitcheg_depth",
                "amplfo_freq",
                "amplfo_depth",
                "pitchlfo_freq",
                "pitchlfo_depth",
                "fillfo_freq",
                "fillfo_depth",
                "global_label",
                "group_label",
                "region_label",
            ]
            .contains(&key.as_str())
            {
                omitted.insert(key.clone());
            }
        }
        zones.push(zone);
    }
    let mut report = format!(
        "{} zones imported: stereo PCM, key/velocity ranges, root, tuning, gain/pan, loops, one-shot, reverse, DAHDSR, filter type/cutoff/resonance, filter and pitch envelopes, pitch/amp/filter LFOs, key and velocity tracking, mute groups. Converter-normalized values; envelope curves are linear and modulation depths are approximations. Unimplemented SFZ parameters: {}. Original DWP blocks and DirectWave audio remain unverified.",
        zones.len(),
        if omitted.is_empty() {
            "none".into()
        } else {
            omitted.into_iter().collect::<Vec<_>>().join(", ")
        }
    );
    if !warnings.is_empty() {
        report.push_str(&format!(" Fixed on import: {}.", warnings.join("; ")));
    }
    let preset = SamplePreset {
        program: Program {
            bend_range,
            ..Program::default()
        },
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        zones,
        original_dwp: std::sync::Arc::new(vec![]),
        report,
    };
    preset.validate().map_err(str::to_owned)?;
    Ok(preset)
}

#[cfg(test)]
mod tests {
    #[test]
    fn lenient_wav_reader_accepts_converter_fmt_chunks() {
        // 16-bit stereo PCM with a 20-byte fmt chunk and a LIST chunk, as ConvertWithMoss writes.
        let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        d.extend_from_slice(&20u32.to_le_bytes());
        d.extend_from_slice(&[1, 0, 2, 0]);
        d.extend_from_slice(&44100u32.to_le_bytes());
        d.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        d.extend_from_slice(&[4, 0, 16, 0, 0, 0, 0, 0]);
        d.extend_from_slice(b"LIST\x04\0\0\0INFO");
        d.extend_from_slice(b"data");
        d.extend_from_slice(&8u32.to_le_bytes());
        for v in [16384i16, -16384, 0, 32767] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let (rate, frames) = super::read_wav(&d).unwrap();
        assert_eq!(rate, 44100);
        assert_eq!(frames, vec![[0.5, -0.5], [0.0, 32767.0 / 32768.0]]);
        // 24-bit mono.
        let mut m = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        m.extend_from_slice(&16u32.to_le_bytes());
        m.extend_from_slice(&[1, 0, 1, 0]);
        m.extend_from_slice(&48000u32.to_le_bytes());
        m.extend_from_slice(&(48000u32 * 3).to_le_bytes());
        m.extend_from_slice(&[3, 0, 24, 0]);
        m.extend_from_slice(b"data\x03\0\0\0");
        m.extend_from_slice(&[0, 0, 0x40]);
        assert_eq!(super::read_wav(&m).unwrap().1, vec![[0.5, 0.5]]);
        assert!(super::read_wav(b"nope").is_err());
    }

    #[test]
    fn broken_loop_points_turn_the_loop_off_instead_of_failing() {
        let text = "<region> sample=test.wav lokey=0 hikey=127 pitch_keycenter=60 loop_mode=loop_continuous loop_start=24064 loop_end=100";
        let p = super::read_sfz(std::path::Path::new("tests/fixtures/dwp/x.sfz"), text).unwrap();
        assert_eq!(p.zones[0].loop_mode, keyframexl::sampler::LoopMode::None);
        assert!(p.report.contains("loop off"), "{}", p.report);
    }

    #[test]
    fn key_opcode_sets_range_and_root() {
        let text =
            "<region> sample=test.wav key=24 <region> sample=test.wav key=25 pitch_keycenter=30";
        let p = super::read_sfz(std::path::Path::new("tests/fixtures/dwp/x.sfz"), text).unwrap();
        let keys: Vec<_> = p.zones.iter().map(|z| (z.low, z.high, z.root)).collect();
        assert_eq!(
            keys,
            [(24, 24, 24), (25, 25, 30)],
            "one key per zone, explicit root wins"
        );
    }

    #[test]
    fn sfz_sound_opcodes_map_to_filters_envelopes_and_matrix() {
        use keyframexl::sampler::{FilterKind, FilterRouting, ModSource, ModTarget};
        let text = "<group> fil_type=lpf_4p cutoff=1200 resonance=6 fileg_attack=0.1 fileg_depth=2400 \
            pitchlfo_freq=4 pitchlfo_depth=50 amplfo_freq=3 amplfo_depth=3 ampeg_hold=0.2 \
            loop_mode=one_shot group=2 off_by=2 direction=reverse pitch_keytrack=50 bend_up=1200\n\
            <region> sample=test.wav lokey=48 hikey=72 pitch_keycenter=60";
        let p =
            super::read_sfz(std::path::Path::new("tests/fixtures/dwp/probe.sfz"), text).unwrap();
        let z = &p.zones[0].params;
        assert_eq!(z.filters[0].kind, FilterKind::Lowpass);
        assert_eq!(
            z.filters[1].kind,
            FilterKind::Lowpass,
            "4-pole = two in series"
        );
        assert_eq!(z.routing, FilterRouting::Serial);
        assert!((z.filters[0].cutoff - 1200.0).abs() < 1e-9);
        assert!(z.filters[0].emphasis > 0.0);
        let has = |s: ModSource, t: ModTarget| {
            z.matrix
                .iter()
                .any(|m| m.source == s && m.target == t && m.amount != 0.0)
        };
        assert!(has(ModSource::Env1, ModTarget::Cutoff1));
        assert!(has(ModSource::Lfo1, ModTarget::Pitch));
        assert!(has(ModSource::Lfo2, ModTarget::Gain));
        assert!((z.lfo[0].rate - 4.0).abs() < 1e-9 && (z.lfo[1].rate - 3.0).abs() < 1e-9);
        assert!(z.one_shot && z.reverse);
        assert_eq!(z.mute_group, 2);
        assert!((z.key_track - 0.5).abs() < 1e-9);
        assert!((p.zones[0].adsr.hold - 0.2).abs() < 1e-9);
        assert_eq!(p.program.bend_range, 1200.0);
    }

    #[test]
    fn dwp_filter_and_lfos_survive_conversion_when_converter_is_available() {
        if std::env::var_os("KEYFRAMEXL_CONVERTER").is_none() {
            return;
        }
        use keyframexl::sampler::{FilterKind, ModSource, ModTarget};
        // probe.dwp was written by ConvertWithMoss from an SFZ with a 1200 Hz lowpass,
        // a 4 Hz / 50 cent pitch LFO and a 3 Hz / 3 dB amplitude LFO.
        let p = super::dwp(std::path::Path::new("tests/fixtures/dwp/probe.dwp")).unwrap();
        let z = &p.zones[0].params;
        assert_eq!(z.filters[0].kind, FilterKind::Lowpass);
        assert!(
            (z.filters[0].cutoff - 1200.0).abs() < 1.0,
            "cutoff {}",
            z.filters[0].cutoff
        );
        assert!(
            z.matrix
                .iter()
                .any(|m| m.source == ModSource::Lfo1 && m.target == ModTarget::Pitch)
        );
        assert!(
            z.matrix
                .iter()
                .any(|m| m.source == ModSource::Lfo2 && m.target == ModTarget::Gain)
        );
        assert!((p.zones[0].adsr.release - 0.8).abs() < 0.01);
    }

    #[test]
    fn convert_fixture_when_converter_is_available() {
        if std::env::var_os("KEYFRAMEXL_CONVERTER").is_none() {
            return;
        }
        let p = super::dwp(std::path::Path::new("tests/fixtures/dwp/test.dwp")).unwrap();
        let zone = &p.zones[0];
        assert_eq!((zone.root, zone.low, zone.high), (60, 48, 72));
        assert_eq!(zone.sample_rate, 48000);
        assert!(!p.original_dwp.is_empty());
        let mut reference = hound::WavReader::open("tests/fixtures/dwp/test.wav").unwrap();
        for (actual, expected) in zone
            .frames
            .iter()
            .zip(reference.samples::<i16>())
            .take(24000)
        {
            assert_eq!(actual[0], expected.unwrap() as f32 / 32768.0)
        }
        assert_eq!(zone.frames.len(), 24064);
    }
    #[test]
    fn global_group_and_region_inheritance() {
        let regions=super::regions("<global> loop_mode=loop_continuous <group> lokey=48 hikey=59 <region> sample=Low soft.wav hivel=63 <region> sample=Low bright.wav lovel=64 <group> lokey=60 <region> sample=High.wav").unwrap();
        assert_eq!(regions.len(), 3);
        assert_eq!(regions[0]["loop_mode"], "loop_continuous");
        assert_eq!(regions[1]["hikey"], "59");
        assert!(!regions[1].contains_key("hivel"));
        assert!(!regions[2].contains_key("hikey"));
    }
    #[test]
    fn multizone_ranges_loops_adsr_and_disk_roundtrip() {
        use keyframexl::{
            project_io,
            sampler::LoopMode,
            sequence::{Clip, Note, Pattern, Song, render},
        };
        if std::env::var_os("KEYFRAMEXL_CONVERTER").is_none() {
            return;
        }
        let input = tempfile::tempdir().unwrap();
        let input_path = input.path().join("Zones.dwp");
        std::fs::copy("tests/fixtures/multizone/converted/Zones.dwp", &input_path).unwrap();
        let preset = super::dwp(&input_path).unwrap();
        std::fs::remove_file(&input_path).unwrap();
        assert_eq!(preset.zones.len(), 4);
        let expected = [
            (48, 48, 59, 1, 63, 0.08, 0.12, 0.6, 0.22),
            (48, 48, 59, 64, 127, 0.005, 0.07, 0.8, 0.4),
            (60, 60, 72, 1, 63, 0.2, 0.15, 0.4, 0.25),
            (60, 60, 72, 64, 127, 0.002, 0.15, 0.7, 0.6),
        ];
        for (z, (root, low, high, lo_vel, hi_vel, a, d, s, r)) in preset.zones.iter().zip(expected)
        {
            assert_eq!(
                (z.root, z.low, z.high, z.low_velocity, z.high_velocity),
                (root, low, high, lo_vel, hi_vel)
            );
            assert_eq!(z.loop_mode, LoopMode::Continuous);
            assert_eq!((z.loop_start, z.loop_end), (384, 4224));
            for (actual, expected) in [
                (z.adsr.attack, a),
                (z.adsr.decay, d),
                (z.adsr.sustain, s),
                (z.adsr.release, r),
            ] {
                assert!(
                    (actual - expected).abs() < 0.000001,
                    "{actual} != {expected}"
                );
            }
        }
        for (key, vel, index) in [
            (48, 1, 0),
            (59, 63, 0),
            (59, 64, 1),
            (60, 63, 2),
            (60, 64, 3),
            (72, 127, 3),
        ] {
            let matches: Vec<_> = preset
                .zones
                .iter()
                .enumerate()
                .filter(|(_, z)| z.matches(key, vel))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(matches, vec![index]);
        }
        assert!(
            !preset
                .zones
                .iter()
                .any(|z| z.matches(47, 100) || z.matches(73, 100) || z.matches(60, 0))
        );
        for (z, name) in
            preset
                .zones
                .iter()
                .zip(["Low soft", "Low bright", "High soft", "High bright"])
        {
            let mut wav =
                hound::WavReader::open(format!("tests/fixtures/multizone/source/{name}.wav"))
                    .unwrap();
            let source: Vec<i16> = wav.samples::<i16>().collect::<Result<_, _>>().unwrap();
            assert!(z.frames.len() >= 4800);
            for (got, expected) in z
                .frames
                .iter()
                .take(4800)
                .zip(source.as_chunks::<2>().0.iter())
            {
                assert_eq!(
                    *got,
                    [expected[0] as f32 / 32768.0, expected[1] as f32 / 32768.0]
                );
            }
        }
        let rms = |z: &keyframexl::sampler::Zone, from: f64, to: f64, vel: u8| -> f64 {
            let start = (from * 48000.0) as usize;
            let end = (to * 48000.0) as usize;
            ((start..end)
                .map(|f| {
                    let t = f as f64 / 48000.0;
                    let v = z.frame(z.root, vel, t, 1.5, t, 1.5)[0] as f64;
                    v * v
                })
                .sum::<f64>()
                / (end - start) as f64)
                .sqrt()
        };
        let soft = &preset.zones[0];
        assert!(rms(soft, 0.3, 0.35, 40) > 0.02);
        let mut no_loop = soft.clone();
        no_loop.loop_mode = LoopMode::None;
        assert_eq!(rms(&no_loop, 0.3, 0.35, 40), 0.0);
        let mut instant = soft.clone();
        instant.adsr.attack = 0.0;
        instant.adsr.decay = 0.0;
        instant.adsr.sustain = 1.0;
        assert!(rms(soft, 0.01, 0.03, 40) < rms(&instant, 0.01, 0.03, 40) * 0.5);
        assert!(rms(soft, 1.55, 1.60, 40) > 0.005);
        assert_eq!(rms(soft, 1.73, 1.78, 40), 0.0);
        assert!(rms(&preset.zones[3], 2.0, 2.05, 110) > 0.005);
        assert_eq!(rms(&preset.zones[3], 2.11, 2.16, 110), 0.0);
        let mut song = Song {
            patterns: vec![],
            ..Song::default()
        };
        song.set_sampler_preset(4, preset).unwrap();
        for (i, (key, velocity)) in [(48, 40), (48, 110), (60, 40), (60, 110)]
            .into_iter()
            .enumerate()
        {
            let id = i as u64 + 1;
            song.patterns.push(Pattern::new(
                id,
                &format!("Key {key} / velocity {velocity}"),
            ));
            song.add_note(
                id,
                Note::new(4, 0, 12 * keyframexl::sequence::STEP, key, velocity),
            );
            song.clips
                .push(Clip::new(id, i as i64 * keyframexl::sequence::BAR, 0));
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Zones.kfxl");
        project_io::save(&song, &path).unwrap();
        let relocated = tempfile::tempdir().unwrap();
        let moved = relocated.path().join("Relocated.kfxl");
        std::fs::copy(&path, &moved).unwrap();
        std::fs::remove_file(&path).unwrap();
        let reopened = project_io::open(&moved).unwrap();
        let before = render(&song, 1, true);
        let after = render(&reopened, 1, true);
        assert_eq!(before.len(), after.len());
        let a = temp.path().join("before.wav");
        let b = temp.path().join("reopened.wav");
        keyframexl::write_wav(&a, &before).unwrap();
        keyframexl::write_wav(&b, &after).unwrap();
        assert!(
            std::fs::read(&a).unwrap() == std::fs::read(&b).unwrap(),
            "WAV changed after project relocation and reopen"
        );
        if let Some(folder) = std::env::var_os("KEYFRAMEXL_ARTIFACTS") {
            let folder = std::path::PathBuf::from(folder);
            std::fs::create_dir_all(&folder).unwrap();
            for (src, name) in [
                (&moved, "Zones.kfxl"),
                (&a, "before.wav"),
                (&b, "reopened.wav"),
            ] {
                std::fs::copy(src, folder.join(name)).unwrap();
            }
            std::fs::write(folder.join("report.txt"),format!("{}\nFour phrases: key 48 velocity 40; key 48 velocity 110; key 60 velocity 40; key 60 velocity 110.\nBoundaries, stereo PCM, loop frame positions, sustained audio after sample end, attack and release tested. Saved and reopened WAV files are byte-identical.\n",reopened.instrument(4).and_then(|i| i.sampler_preset()).unwrap().report)).unwrap();
        }
    }
}

#[cfg(test)]
mod ogg_in_wav_tests {
    /// A RIFF/WAVE file whose data chunk is an Ogg Vorbis stream, as FL Studio writes.
    fn wrap(ogg: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&0x674Fu16.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&48_000u32.to_le_bytes());
        fmt.extend_from_slice(&12_000u32.to_le_bytes());
        fmt.extend_from_slice(&4u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        let mut body = b"WAVEfmt ".to_vec();
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(ogg.len() as u32).to_le_bytes());
        body.extend_from_slice(ogg);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn fl_style_ogg_inside_wav_decodes() {
        let ogg = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tone.ogg"
        ))
        .unwrap();
        let (rate, frames) = super::read_wav(&wrap(&ogg)).unwrap();
        assert_eq!(rate, 48_000);
        let secs = frames.len() as f64 / 48_000.0;
        assert!((secs - 0.3).abs() < 0.02, "0.3 s tone: {secs}");
        let peak = frames.iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1, "it has the tone (ffmpeg sine is 1/8): {peak}");
        assert!(
            frames.iter().all(|f| f[0] == f[1]),
            "mono spread to both sides"
        );
    }

    /// Every stock FL drum sample decodes (KEYFRAMEXL_FL_PACKS = …/Data/Patches/Packs).
    #[test]
    fn real_fl_drum_packs_decode() {
        let Some(packs) = std::env::var_os("KEYFRAMEXL_FL_PACKS") else {
            return;
        };
        let mut stack = vec![std::path::PathBuf::from(packs).join("Drums")];
        let (mut ok, mut bad) = (0, Vec::new());
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) {
                    match super::read_audio(&p) {
                        Ok((_, f)) if !f.is_empty() => ok += 1,
                        other => bad.push(format!("{}: {:?}", p.display(), other.err())),
                    }
                }
            }
        }
        eprintln!("FL drum samples decoded: {ok}, failed: {}", bad.len());
        assert!(bad.is_empty(), "{bad:#?}");
        assert!(ok > 200);
    }
}
