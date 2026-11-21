use crate::sequence::Song;
use std::{io::Write, path::Path};

pub const APP_ID: &str = "keyframexl";
/// Envelope version. 1 = current. Bare-song files (no envelope) are V0 legacy.
/// Envelope version. 2 = song format 2 (ticks). Bare-song files (no envelope) are older.
pub const DOC_VERSION: u32 = 2;

#[derive(serde::Serialize)]
struct Envelope<'a> {
    app: &'a str,
    doc: u32,
    song: &'a Song,
}

/// A project with samples: this magic, the JSON length (u64 LE), the JSON,
/// the blob count (u32 LE), then each blob as length (u64 LE) and bytes. The
/// JSON names a blob as "@<index>". Projects without samples stay plain JSON.
const MAGIC: &[u8; 8] = b"KFXLBIN1";

/// The project as bytes: plain JSON, or the container when it holds samples.
pub fn to_bytes(song: &Song) -> Result<Vec<u8>, String> {
    let (json, blobs) = blobs::collect(|| {
        serde_json::to_vec(&Envelope {
            app: APP_ID,
            doc: DOC_VERSION,
            song,
        })
        .map_err(|e| e.to_string())
    })?;
    if blobs.is_empty() {
        return Ok(json);
    }
    let total = 16 + json.len() + 4 + blobs.iter().map(|b| 8 + b.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
    for b in &blobs {
        out.extend_from_slice(&(b.len() as u64).to_le_bytes());
        out.extend_from_slice(b);
    }
    Ok(out)
}

/// Split a container into its JSON and blob ranges.
fn split(bytes: &[u8]) -> Result<(&[u8], Vec<std::ops::Range<usize>>), String> {
    let bad = || "Damaged project file".to_string();
    let u64_at = |at: usize| {
        bytes
            .get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or_else(bad)
    };
    let json_len = u64_at(8)?;
    let json = bytes.get(16..16 + json_len).ok_or_else(bad)?;
    let mut at = 16 + json_len;
    let count = bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or_else(bad)?;
    at += 4;
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u64_at(at)?;
        at += 8;
        if at + len > bytes.len() {
            return Err(bad());
        }
        ranges.push(at..at + len);
        at += len;
    }
    Ok((json, ranges))
}

pub fn save(song: &Song, path: &Path) -> Result<(), String> {
    song.validate().map_err(str::to_owned)?;
    let data = to_bytes(song)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&data)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    // On Unix, fsyncing the parent directory commits the rename entry.
    // On Windows, opening a directory with File::open fails with ERROR_ACCESS_DENIED.
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Parse a project: format 2 directly, format 1 through the migration.
pub fn parse(bytes: &[u8]) -> Result<Song, String> {
    if bytes.starts_with(MAGIC) {
        let (json, ranges) = split(bytes)?;
        return blobs::with_source(bytes, ranges, || parse_json(json));
    }
    parse_json(bytes)
}

fn parse_json(bytes: &[u8]) -> Result<Song, String> {
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Not a project file: {e}"))?;
    let song = if value.get("app").is_some() {
        if value["app"] != APP_ID {
            return Err("Not a KeyFrameXL project".into());
        }
        if value["doc"].as_u64().unwrap_or(0) > DOC_VERSION as u64 {
            return Err("Project is from a newer KeyFrameXL".into());
        }
        value["song"].take()
    } else {
        value
    };
    let version = song.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    let mut song = match version {
        1 => serde_json::from_value::<crate::legacy::V1Song>(song)
            .map_err(|e| e.to_string())?
            .migrate(),
        2 => serde_json::from_value::<Song>(song).map_err(|e| e.to_string())?,
        _ => return Err("Project is from a newer KeyFrameXL".into()),
    };
    song.ensure_note_ids();
    song.validate().map_err(str::to_owned)?;
    Ok(song)
}

pub fn open(path: &Path) -> Result<Song, String> {
    parse(&std::fs::read(path).map_err(|e| e.to_string())?)
}

/// Samples as binary blobs while a project is written or read (per thread,
/// so presets and the clipboard keep their inline base64 form).
pub(crate) mod blobs {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, Weak};

    type Frames = Arc<Vec<[f32; 2]>>;

    #[derive(Default)]
    struct Sink {
        blobs: Vec<Arc<Vec<u8>>>,
        /// The same buffer (two channels sharing one sample) is written once.
        index: HashMap<usize, usize>,
    }

    struct Source<'a> {
        bytes: &'a [u8],
        ranges: Vec<std::ops::Range<usize>>,
        decoded: HashMap<usize, Frames>,
    }

    thread_local! {
        static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
        static SOURCE: RefCell<Option<Source<'static>>> = const { RefCell::new(None) };
    }

    /// Encoded blobs by buffer address, kept while the buffer lives: saving
    /// again (and autosave) does not re-encode unchanged samples.
    static CACHE: Mutex<Vec<(Weak<Vec<[f32; 2]>>, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());

    /// Run `f` (the JSON writer) collecting sample blobs.
    pub fn collect<T>(
        f: impl FnOnce() -> Result<T, String>,
    ) -> Result<(T, Vec<Arc<Vec<u8>>>), String> {
        SINK.with(|s| *s.borrow_mut() = Some(Sink::default()));
        let out = f();
        let sink = SINK.with(|s| s.borrow_mut().take()).unwrap_or_default();
        Ok((out?, sink.blobs))
    }

    /// Run `f` (the JSON reader) with the blobs of a container.
    pub fn with_source<T>(
        bytes: &[u8],
        ranges: Vec<std::ops::Range<usize>>,
        f: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        // SAFETY: the source is removed before this function returns, so the
        // borrowed bytes outlive every use of them.
        let bytes: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        SOURCE.with(|s| {
            *s.borrow_mut() = Some(Source {
                bytes,
                ranges,
                decoded: HashMap::new(),
            })
        });
        let out = f();
        SOURCE.with(|s| *s.borrow_mut() = None);
        out
    }

    /// While a project is written: whether bulky extras (the original DWP) are left out.
    pub fn writing() -> bool {
        SINK.with(|s| s.borrow().is_some())
    }

    /// While a project is written: store `frames` as a blob, returning its reference.
    pub fn store(frames: &Frames) -> Option<String> {
        SINK.with(|s| {
            let mut s = s.borrow_mut();
            let sink = s.as_mut()?;
            let key = Arc::as_ptr(frames) as usize;
            if let Some(i) = sink.index.get(&key) {
                return Some(format!("@{i}"));
            }
            let blob = cached(frames);
            sink.blobs.push(blob);
            let i = sink.blobs.len() - 1;
            sink.index.insert(key, i);
            Some(format!("@{i}"))
        })
    }

    fn cached(frames: &Frames) -> Arc<Vec<u8>> {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(w, _)| w.strong_count() > 0);
        if let Some((_, b)) = cache
            .iter()
            .find(|(w, _)| std::ptr::eq(w.as_ptr(), Arc::as_ptr(frames)))
        {
            return b.clone();
        }
        let blob = Arc::new(encode(frames));
        cache.push((Arc::downgrade(frames), blob.clone()));
        blob
    }

    /// While a project is read: the frames a "@<index>" reference names.
    pub fn load(reference: &str) -> Option<Result<Frames, String>> {
        let i: usize = reference.strip_prefix('@')?.parse().ok()?;
        SOURCE.with(|s| {
            let mut s = s.borrow_mut();
            let src = s.as_mut()?;
            if let Some(f) = src.decoded.get(&i) {
                return Some(Ok(f.clone()));
            }
            let range = src.ranges.get(i)?.clone();
            let out = decode(&src.bytes[range]).map(Arc::new);
            if let Ok(f) = &out {
                src.decoded.insert(i, f.clone());
            }
            Some(out)
        })
    }

    /// Smallest exact form: 16-bit when every sample is a 16-bit value, one
    /// channel when both are equal. Kind byte, frame count (u64 LE), data.
    pub fn encode(frames: &[[f32; 2]]) -> Vec<u8> {
        let mono = frames.iter().all(|f| f[0].to_bits() == f[1].to_bits());
        let short = |v: f32| {
            let s = (v * 32768.0).round();
            (-32768.0..=32767.0).contains(&s) && s / 32768.0 == v
        };
        let int16 = frames.iter().all(|f| short(f[0]) && short(f[1]));
        let kind = (int16 as u8) << 1 | mono as u8;
        let width = if int16 { 2 } else { 4 } * if mono { 1 } else { 2 };
        let mut out = Vec::with_capacity(9 + frames.len() * width);
        out.push(kind);
        out.extend_from_slice(&(frames.len() as u64).to_le_bytes());
        for f in frames {
            for &v in if mono { &f[..1] } else { &f[..] } {
                if int16 {
                    out.extend_from_slice(&((v * 32768.0).round() as i16).to_le_bytes());
                } else {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }

    pub fn decode(b: &[u8]) -> Result<Vec<[f32; 2]>, String> {
        let bad = || "Damaged sample data".to_string();
        let kind = *b.first().ok_or_else(bad)?;
        let n = b
            .get(1..9)
            .map(|x| u64::from_le_bytes(x.try_into().unwrap()) as usize)
            .ok_or_else(bad)?;
        let (int16, mono) = (kind & 2 != 0, kind & 1 != 0);
        let width = if int16 { 2 } else { 4 };
        let channels = if mono { 1 } else { 2 };
        let data = b.get(9..).ok_or_else(bad)?;
        if kind > 3 || data.len() != n * width * channels {
            return Err(bad());
        }
        let value = |c: &[u8]| {
            if int16 {
                i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0
            } else {
                f32::from_le_bytes([c[0], c[1], c[2], c[3]])
            }
        };
        let mut out = Vec::with_capacity(n);
        for frame in data.chunks_exact(width * channels) {
            let l = value(&frame[..width]);
            let r = if mono { l } else { value(&frame[width..]) };
            if !l.is_finite() || !r.is_finite() {
                return Err(bad());
            }
            out.push([l, r]);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_roundtrip_and_foreign_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.kfxl");
        let song = Song::demo();
        save(&song, &path).unwrap();
        let back = open(&path).unwrap();
        assert_eq!(back.tempo, song.tempo);
        assert_eq!(back.patterns[0].notes, song.patterns[0].notes);
        // Bare format-2 song without envelope.
        std::fs::write(&path, serde_json::to_vec(&song).unwrap()).unwrap();
        assert_eq!(
            open(&path).unwrap().patterns[0].notes.len(),
            song.patterns[0].notes.len()
        );
        let bad = br#"{"app":"other","doc":1,"song":{"version":1,"tempo":120.0,"swing":0.0,"patterns":[],"clips":[]}}"#;
        assert!(parse(bad).is_err());
        let newer = br#"{"app":"keyframexl","doc":9,"song":{}}"#;
        assert_eq!(
            parse(newer).unwrap_err(),
            "Project is from a newer KeyFrameXL"
        );
    }

    #[test]
    fn samples_go_in_compact_blobs_shared_once_and_come_back_exactly() {
        use crate::sequence::InstrumentKind;
        use std::sync::Arc;
        let mut song = Song::demo();
        let mut preset = crate::sampler::SamplePreset::test_preset("Blob");
        // 16-bit stereo, a mono 16-bit zone, and a float zone.
        let s16: Vec<[f32; 2]> = (0..4000)
            .map(|i| {
                [
                    ((i * 37 % 65536) - 32768) as f32 / 32768.0,
                    ((i * 11 % 3000) as f32) / 32768.0,
                ]
            })
            .collect();
        let mono: Vec<[f32; 2]> = (0..3000)
            .map(|i| {
                let v = ((i * 5) % 2000) as f32 / 32768.0;
                [v, v]
            })
            .collect();
        let float: Vec<[f32; 2]> = (0..2000)
            .map(|i| {
                [
                    (i as f32 * 0.001).sin() * 0.3,
                    (i as f32 * 0.0013).cos() * 0.2,
                ]
            })
            .collect();
        let base = preset.zones[0].clone();
        preset.zones = [s16.clone(), mono.clone(), float.clone()]
            .into_iter()
            .map(|f| {
                let mut z = base.clone();
                z.end = f.len();
                z.loop_end = f.len();
                z.frames = Arc::new(f);
                z
            })
            .collect();
        preset.original_dwp = Arc::new(vec![9u8; 50_000]);
        let shared = InstrumentKind::Sampler {
            preset: Some(Arc::new(preset.clone())),
        };
        let ids: Vec<u64> = song.instruments.iter().take(2).map(|i| i.id).collect();
        for id in &ids {
            song.instruments
                .iter_mut()
                .find(|i| i.id == *id)
                .unwrap()
                .kind = shared.clone();
        }
        let bytes = to_bytes(&song).unwrap();
        assert!(bytes.starts_with(MAGIC));
        let raw = (s16.len() + mono.len() + float.len()) * 8;
        let (_, ranges) = split(&bytes).unwrap();
        let stored: usize = ranges.iter().map(|r| r.len()).sum();
        // 16-bit stereo 4 B, 16-bit mono 2 B, float 8 B per frame; one copy for both channels.
        assert_eq!(ranges.len(), 3, "two channels share one copy");
        assert_eq!(
            stored,
            s16.len() * 4 + mono.len() * 2 + float.len() * 8 + 3 * 9
        );
        assert!(stored < raw * 6 / 10);
        let back = parse(&bytes).unwrap();
        let p = back
            .instruments
            .iter()
            .find(|i| i.id == ids[1])
            .unwrap()
            .sampler_preset()
            .unwrap();
        assert_eq!(*p.zones[0].frames, s16);
        assert_eq!(*p.zones[1].frames, mono);
        assert_eq!(*p.zones[2].frames, float);
        assert!(
            p.original_dwp.is_empty(),
            "the original DWP stays out of projects"
        );
        let a = back
            .instruments
            .iter()
            .find(|i| i.id == ids[0])
            .unwrap()
            .sampler_preset()
            .unwrap();
        assert!(
            Arc::ptr_eq(&a.zones[0].frames, &p.zones[0].frames),
            "shared on load too"
        );
        // A sample preset on its own keeps the inline form.
        let alone = serde_json::to_string(&preset).unwrap();
        assert!(!alone.contains("\"@0\""));
        assert!(alone.len() > 50_000);
    }

    #[test]
    fn format_1_global_sample_becomes_the_bass_preset() {
        let preset = crate::sampler::SamplePreset::test_preset("MigBass");
        let v = serde_json::json!({"version": 1, "sample": preset, "patterns": [{"id": 1, "name": "P", "notes": [], "slides": []}], "clips": []});
        let song = parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(song.instruments.len(), 4);
        let bass = song.instrument(4).unwrap();
        assert_eq!(bass.name, "MigBass");
        assert!(bass.sampler_preset().is_some());
    }
}

#[cfg(test)]
mod size_check {
    /// `KFXL_SIZE_CHECK=project.kfxl cargo test --release --lib size_check -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn how_big_and_slow_a_project_is() {
        let Some(path) = std::env::var_os("KFXL_SIZE_CHECK") else {
            return;
        };
        let t = std::time::Instant::now();
        let song = super::open(std::path::Path::new(&path)).unwrap();
        println!("open {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let bytes = super::to_bytes(&song).unwrap();
        println!("encode {:?}, {} MB", t.elapsed(), bytes.len() / 1_000_000);
        let t = std::time::Instant::now();
        let bytes = super::to_bytes(&song).unwrap();
        println!(
            "encode again {:?}, {} MB",
            t.elapsed(),
            bytes.len() / 1_000_000
        );
        let dir = tempfile::tempdir().unwrap();
        let t = std::time::Instant::now();
        super::save(&song, &dir.path().join("x.kfxl")).unwrap();
        println!("save (with fsync) {:?}", t.elapsed());
        let t = std::time::Instant::now();
        super::open(&dir.path().join("x.kfxl")).unwrap();
        println!("open new format {:?}", t.elapsed());
    }
}
