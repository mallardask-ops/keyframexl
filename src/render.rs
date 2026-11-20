//! Offline rendering for export (FL: the Rendering dialog). Renders a pattern
//! or the playlist through the same engine as live playback, handles the tail
//! (leave / cut / wrap), normalizes, dithers 16-bit output and writes WAV
//! (16/24/32f) and FLAC (16/24). Runs on a worker thread with progress and
//! cancel; everything here is plain functions so tests can call them.

use crate::engine::Engine;
use crate::sequence::{Song, loop_frames};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const RATE: u32 = 48_000;
/// Longest tail rendered after the end (reverbs, long releases).
pub const MAX_TAIL_SECONDS: f64 = 30.0;
/// Below this the tail counts as silent (−80 dBFS) …
const SILENCE: f32 = 1e-4;
/// … for this long, with no voice sounding.
const SILENT_RUN_SECONDS: f64 = 0.5;
const BLOCK: usize = 512;

/// What happens to sound that rings past the end (FL: Tail).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tail {
    /// Keep rendering until it is silent (FL: Leave remainder).
    #[default]
    Leave,
    /// Stop exactly at the end.
    Cut,
    /// Fold the tail onto the start, for seamless loops.
    Wrap,
}

impl Tail {
    pub const ALL: [Tail; 3] = [Tail::Leave, Tail::Cut, Tail::Wrap];
    pub fn label(self) -> &'static str {
        match self {
            Tail::Leave => "Leave remainder",
            Tail::Cut => "Cut remainder",
            Tail::Wrap => "Wrap remainder",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Tail::Leave => "Reverb and release ring out after the end (up to 30 s)",
            Tail::Cut => "The file ends exactly at the last bar",
            Tail::Wrap => "The ring-out is mixed onto the start: loops seamlessly",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Wav16,
    Wav24,
    Wav32Float,
    Flac16,
    Flac24,
    Mp3_128,
    Mp3_192,
    Mp3_320,
}

impl Format {
    /// Constant bitrate of an MP3 format.
    pub fn kbps(self) -> Option<u16> {
        match self {
            Format::Mp3_128 => Some(128),
            Format::Mp3_192 => Some(192),
            Format::Mp3_320 => Some(320),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Format::Wav16 | Format::Wav24 | Format::Wav32Float => "wav",
            Format::Flac16 | Format::Flac24 => "flac",
            Format::Mp3_128 | Format::Mp3_192 | Format::Mp3_320 => "mp3",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Format::Wav16 => "WAV 16-bit",
            Format::Wav24 => "WAV 24-bit",
            Format::Wav32Float => "WAV 32-bit float",
            Format::Flac16 => "FLAC 16-bit",
            Format::Flac24 => "FLAC 24-bit",
            Format::Mp3_128 => "MP3 128 kbps",
            Format::Mp3_192 => "MP3 192 kbps",
            Format::Mp3_320 => "MP3 320 kbps",
        }
    }
    pub fn bits(self) -> u16 {
        match self {
            Format::Wav16
            | Format::Flac16
            | Format::Mp3_128
            | Format::Mp3_192
            | Format::Mp3_320 => 16,
            Format::Wav24 | Format::Flac24 => 24,
            Format::Wav32Float => 32,
        }
    }
    /// Bytes per second of stereo audio (FLAC: an upper bound).
    pub fn bytes_per_second(self) -> u64 {
        if let Some(k) = self.kbps() {
            return k as u64 * 1000 / 8;
        }
        RATE as u64 * 2 * self.bits() as u64 / 8
    }
}

/// Everything the dialog decided.
#[derive(Clone, Debug)]
pub struct Job {
    pub song: Song,
    pub pattern: u64,
    pub song_mode: bool,
    pub tail: Tail,
    pub formats: Vec<Format>,
    /// Peak to −1 dBFS.
    pub normalize: bool,
    /// TPDF dither on 16-bit output.
    pub dither: bool,
    /// Output path; each format replaces the extension.
    pub path: PathBuf,
    /// Sample rate of the file (hosted plugins run at the output's rate).
    pub rate: u32,
    /// Hosted plugin instances to render with (the live output leaves them alone meanwhile).
    pub plugins: Plugins,
    /// Song mode: only these ticks (FL: export the time selection).
    pub range: Option<(crate::sequence::Tick, crate::sequence::Tick)>,
}

/// Plugin instances for an offline render, by plugin id.
#[derive(Clone, Default)]
pub struct Plugins(pub Vec<(u64, crate::plugin::Shared)>);

impl std::fmt::Debug for Plugins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} plugin(s)", self.0.len())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub seconds: f64,
    /// Peak before normalizing, in dBFS.
    pub peak_db: f32,
    /// Some samples hit full scale (the engine clamps at ±1).
    pub clipped: bool,
}

#[derive(Debug)]
pub enum Progress {
    /// Fraction of the main part rendered, 0..1 (the tail adds a little more).
    Rendering(f32),
    Encoding(Format),
    Done(Vec<PathBuf>, Stats),
    Cancelled,
    Failed(String),
}

/// Length of the main part (no tail), in frames.
pub fn main_frames(song: &Song, pattern: u64, song_mode: bool) -> u64 {
    loop_frames(song, pattern, song_mode, RATE)
}

/// Render to stereo frames. `None` when cancelled.
pub fn render_audio(
    song: &Song,
    pattern: u64,
    song_mode: bool,
    tail: Tail,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Option<Vec<[f32; 2]>> {
    render_with(
        song,
        pattern,
        song_mode,
        tail,
        RATE,
        &Plugins::default(),
        cancel,
        progress,
    )
}

/// [`render_audio`] at `rate`, with hosted plugins.
#[allow(clippy::too_many_arguments)]
pub fn render_with(
    song: &Song,
    pattern: u64,
    song_mode: bool,
    tail: Tail,
    rate: u32,
    plugins: &Plugins,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Option<Vec<[f32; 2]>> {
    render_range(
        song, pattern, song_mode, None, tail, rate, plugins, cancel, progress,
    )
}

/// The song cut to `a..b`: clips past `b` are gone and clips over it end
/// there, so a tail rings out without the music that follows.
pub fn cut_to(song: &Song, (a, b): (crate::sequence::Tick, crate::sequence::Tick)) -> Song {
    let mut s = song.clone();
    let lens: Vec<_> = s.clips.iter().map(|c| song.clip_len(c)).collect();
    let keep: Vec<bool> = s
        .clips
        .iter()
        .zip(&lens)
        .map(|(c, l)| {
            let held = song
                .patterns
                .iter()
                .find(|p| p.id == c.pattern)
                .and_then(|p| p.automation.as_ref())
                .is_some_and(|e| e.playback != crate::automation::Playback::Clip);
            c.start < b && (c.start + l > a || held)
        })
        .collect();
    for (c, l) in s.clips.iter_mut().zip(&lens) {
        if c.start + l > b {
            c.length = Some(b - c.start);
        }
    }
    let mut k = keep.iter();
    s.clips.retain(|_| *k.next().unwrap_or(&false));
    s
}

/// [`render_with`], in song mode limited to the ticks `range` when given.
#[allow(clippy::too_many_arguments)]
pub fn render_range(
    song: &Song,
    pattern: u64,
    song_mode: bool,
    range: Option<(crate::sequence::Tick, crate::sequence::Tick)>,
    tail: Tail,
    rate: u32,
    plugins: &Plugins,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Option<Vec<[f32; 2]>> {
    let range = range.filter(|(a, b)| song_mode && b > a && *a >= 0);
    let cut;
    let song = match range {
        Some(r) => {
            cut = cut_to(song, r);
            &cut
        }
        None => song,
    };
    let mut engine = Engine::new(rate);
    engine.clip_output = false;
    if song
        .instruments
        .iter()
        .any(|i| matches!(i.kind, crate::sequence::InstrumentKind::Vocal { .. }))
    {
        // Singing is rendered ahead; an export waits for it.
        engine.vocal.store(std::sync::Arc::new(
            crate::vocal::host::render_all(song, rate).0,
        ));
    }
    for (id, p) in &plugins.0 {
        if song
            .instruments
            .iter()
            .any(|i| i.muted && i.plugin().is_some_and(|r| r.id == *id))
        {
            continue;
        }
        if let Ok(mut processor) = p.lock() {
            processor.prepare_offline();
        }
        engine.adopt_plugin(crate::plugin::Instance::new(*id, p.clone()));
    }
    // Plugins may take their time while rendering (cleared on every way out).
    struct Offline;
    impl Drop for Offline {
        fn drop(&mut self) {
            crate::plugin::OFFLINE.store(false, std::sync::atomic::Ordering::Release);
        }
    }
    let _offline = (!plugins.0.is_empty()).then(|| {
        crate::plugin::OFFLINE.store(true, std::sync::atomic::Ordering::Release);
        Offline
    });
    let realtime = plugins
        .0
        .iter()
        .any(|(id, p)| {
            song.instruments
                .iter()
                .any(|i| !i.muted && i.plugin().is_some_and(|r| r.id == *id))
                && p.lock().is_ok_and(|p| p.needs_realtime())
        })
        .then(std::time::Instant::now);
    let pace = |pos| {
        if let Some(start) = realtime
            && let Some(wait) = (start
                + std::time::Duration::from_secs_f64(pos as f64 / rate as f64))
            .checked_duration_since(std::time::Instant::now())
        {
            std::thread::sleep(wait);
        }
    };
    // The first frame and how many: the whole loop, or the selection.
    let (first, length) = match range {
        Some((a, b)) => {
            let table = (!song.tempo_map.is_empty())
                .then(|| crate::tempo::TempoTable::build(&song.tempo_map, song.tempo, rate));
            let (s, e) = (
                crate::sequence::tick_frames(song, table.as_ref(), a, rate),
                crate::sequence::tick_frames(song, table.as_ref(), b, rate),
            );
            (s, e.saturating_sub(s).max(1))
        }
        None => (0, loop_frames(song, pattern, song_mode, rate).max(1)),
    };
    let cap = length + (MAX_TAIL_SECONDS * rate as f64) as u64;
    let silent_needed = (SILENT_RUN_SECONDS * rate as f64) as u64;
    let mut out: Vec<f32> = Vec::with_capacity(length as usize * 2);
    let mut buf = vec![0.0f32; BLOCK * 2];
    let (mut pos, mut silent_run, mut blocks) = (0u64, 0u64, 0u32);
    loop {
        let in_main = pos < length;
        if !in_main
            && (tail == Tail::Cut
                || pos >= cap
                || (silent_run >= silent_needed && engine.active_voices() == 0))
        {
            break;
        }
        let n = if in_main {
            (length - pos).min(BLOCK as u64)
        } else {
            BLOCK as u64
        } as usize;
        pace(pos);
        let slice = &mut buf[..n * 2];
        engine.process(song, pattern, song_mode, first + pos, slice, false);
        if !in_main {
            let loud = slice.iter().any(|s| s.abs() > SILENCE);
            silent_run = if loud { 0 } else { silent_run + n as u64 };
        }
        out.extend_from_slice(slice);
        pos += n as u64;
        blocks += 1;
        if blocks.is_multiple_of(64) {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            progress((pos as f32 / length as f32).min(1.0));
        }
    }
    // Latency compensation delays the whole mix by the engine's latency
    // (plugins' own look-ahead; bridges add none offline): render that much
    // more and drop it from the start, so the file starts on the beat.
    let lag = engine.latency();
    let mut left = lag;
    while left > 0 {
        let n = left.min(BLOCK);
        pace(pos);
        engine.process(
            song,
            pattern,
            song_mode,
            first + pos,
            &mut buf[..n * 2],
            false,
        );
        out.extend_from_slice(&buf[..n * 2]);
        pos += n as u64;
        left -= n;
    }
    out.drain(..(lag * 2).min(out.len()));
    let mut frames: Vec<[f32; 2]> = out.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
    if tail == Tail::Wrap && frames.len() > length as usize {
        let ring = frames.split_off(length as usize);
        for (i, f) in ring.iter().enumerate() {
            let d = &mut frames[i % length as usize];
            d[0] += f[0];
            d[1] += f[1];
        }
    }
    progress(1.0);
    Some(frames)
}

pub fn peak(frames: &[[f32; 2]]) -> f32 {
    frames
        .iter()
        .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
}

/// Scale so the loudest sample sits at −1 dBFS. Silence stays silent.
pub fn normalize(frames: &mut [[f32; 2]]) {
    let p = peak(frames);
    if p > 0.0 {
        let g = 10f32.powf(-1.0 / 20.0) / p;
        for f in frames {
            f[0] *= g;
            f[1] *= g;
        }
    }
}

/// Integer samples at `bits`, interleaved, with optional TPDF dither (±1 LSB
/// triangular, deterministic seed so exports are repeatable).
pub fn to_int(frames: &[[f32; 2]], bits: u16, dither: bool) -> Vec<i32> {
    let max = ((1i64 << (bits - 1)) - 1) as f32;
    let mut seed = 0x9E37_79B9_u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let mut out = Vec::with_capacity(frames.len() * 2);
    for f in frames {
        for s in f {
            let d = if dither { rnd() - rnd() } else { 0.0 };
            out.push(
                (s.clamp(-1.0, 1.0) * max + d)
                    .round()
                    .clamp(-max - 1.0, max) as i32,
            );
        }
    }
    out
}

pub fn write_wav(
    path: &Path,
    frames: &[[f32; 2]],
    format: Format,
    dither: bool,
) -> Result<(), String> {
    write_wav_at(path, frames, format, dither, RATE)
}

pub fn write_wav_at(
    path: &Path,
    frames: &[[f32; 2]],
    format: Format,
    dither: bool,
    rate: u32,
) -> Result<(), String> {
    let float = format == Format::Wav32Float;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: format.bits(),
        sample_format: if float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut w = hound::WavWriter::create(path, spec).map_err(err)?;
    if float {
        for f in frames {
            w.write_sample(f[0]).map_err(err)?;
            w.write_sample(f[1]).map_err(err)?;
        }
    } else {
        for s in to_int(frames, format.bits(), dither && format.bits() == 16) {
            w.write_sample(s).map_err(err)?;
        }
    }
    w.finalize().map_err(err)
}

pub fn write_flac(
    path: &Path,
    frames: &[[f32; 2]],
    format: Format,
    dither: bool,
) -> Result<(), String> {
    write_flac_at(path, frames, format, dither, RATE)
}

pub fn write_flac_at(
    path: &Path,
    frames: &[[f32; 2]],
    format: Format,
    dither: bool,
    rate: u32,
) -> Result<(), String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let bits = format.bits() as usize;
    let samples = to_int(frames, format.bits(), dither && bits == 16);
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| format!("FLAC settings: {e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(&samples, 2, bits, rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC: {e:?}"))?;
    std::fs::write(path, sink.as_slice()).map_err(|e| format!("{}: {e}", path.display()))
}

/// The file each format goes to: the chosen path with that format's extension.
pub fn output_path(base: &Path, format: Format) -> PathBuf {
    base.with_extension(format.ext())
}

/// Render and write every format; reports progress. Completed files are kept
/// even when a later format fails (FL keeps finished formats).
pub fn run(job: &Job, cancel: &AtomicBool, send: &mut dyn FnMut(Progress)) {
    let mut frames = match render_range(
        &job.song,
        job.pattern,
        job.song_mode,
        job.range,
        job.tail,
        job.rate,
        &job.plugins,
        cancel,
        &mut |p| send(Progress::Rendering(p)),
    ) {
        Some(f) => f,
        None => return send(Progress::Cancelled),
    };
    if job.plugins.0.iter().any(|(id, p)| {
        job.song
            .instruments
            .iter()
            .any(|i| !i.muted && i.plugin().is_some_and(|r| r.id == *id))
            && p.lock().is_ok_and(|p| !p.available())
    }) {
        return send(Progress::Failed(
            "Audio from an external instrument was interrupted during export.".into(),
        ));
    }
    if job.normalize {
        normalize(&mut frames);
    }
    let p = peak(&frames);
    let stats = Stats {
        seconds: frames.len() as f64 / job.rate as f64,
        peak_db: if p > 0.0 {
            20.0 * p.log10()
        } else {
            f32::NEG_INFINITY
        },
        clipped: p >= 1.0,
    };
    let mut written = Vec::new();
    for &f in &job.formats {
        if cancel.load(Ordering::Relaxed) {
            return send(Progress::Cancelled);
        }
        send(Progress::Encoding(f));
        let path = output_path(&job.path, f);
        let r = match f.ext() {
            "wav" => write_wav_at(&path, &frames, f, job.dither, job.rate),
            "mp3" => crate::mp3::write(
                &path,
                &frames,
                job.rate,
                f.kbps().unwrap_or(192),
                &job.song.info,
            ),
            _ => write_flac_at(&path, &frames, f, job.dither, job.rate),
        };
        match r {
            Ok(()) => written.push(path),
            Err(e) => {
                let kept = if written.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Kept: {}.",
                        written
                            .iter()
                            .filter_map(|p| p.file_name())
                            .map(|n| n.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return send(Progress::Failed(format!(
                    "{} not written: {e}.{kept}",
                    f.label()
                )));
            }
        }
    }
    send(Progress::Done(written, stats));
}

/// FL "Split mixer tracks": one song per used insert, playing only what passes
/// through that insert (its channels, and tracks that send into it), with
/// Master left neutral: Master effects, EQ and fader stay out of stems.
/// Returns (insert index, song) in mixer order.
pub fn stems(song: &Song) -> Vec<(usize, Song)> {
    let n = song.mixer.len();
    // reach[a][t]: track a's signal arrives at t (itself, or through sends).
    let mut reach = vec![vec![false; n]; n];
    for (a, row) in reach.iter_mut().enumerate() {
        let mut stack = vec![a];
        while let Some(x) = stack.pop() {
            if std::mem::replace(&mut row[x], true) {
                continue;
            }
            stack.extend(
                song.mixer[x]
                    .sends
                    .iter()
                    .map(|s| s.to as usize)
                    .filter(|&d| d < n),
            );
        }
    }
    let used = |t: usize| {
        song.instruments
            .iter()
            .any(|i| !i.muted && (i.mixer as usize) < n && reach[i.mixer as usize][t])
    };
    (1..n)
        .filter(|&t| used(t) && !song.mixer[t].muted)
        .map(|t| {
            let mut s = song.clone();
            for (o, m) in s.mixer.iter_mut().enumerate() {
                if o == 0 {
                    m.fx.clear();
                    m.eq = [0.0; 3];
                    m.volume = 1.0;
                    m.pan = 0.0;
                    m.sep = 0.0;
                    m.muted = false;
                } else if o == t {
                    m.sends.clear();
                    m.to_master = true;
                } else if reach[o][t] {
                    // Into t only: no direct path to Master, no sends elsewhere.
                    m.to_master = false;
                    m.sends
                        .retain(|x| (x.to as usize) < n && reach[x.to as usize][t]);
                } else {
                    m.muted = true;
                }
            }
            for i in &mut s.instruments {
                if (i.mixer as usize) >= n || !reach[i.mixer as usize][t] {
                    i.muted = true;
                }
            }
            (t, s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A silent plugin instrument that reports look-ahead latency.
    struct LookAhead;

    impl crate::plugin::Processor for LookAhead {
        fn process(
            &mut self,
            _: &[crate::plugin::PluginEvent],
            io: &mut [f32],
            _: &crate::plugin::Transport,
        ) {
            io.fill(0.0);
        }
        fn latency(&self, _: usize) -> usize {
            700
        }
    }

    #[test]
    fn an_export_with_a_look_ahead_plugin_still_starts_on_the_beat() {
        let mut song = Song::demo();
        let pat = song.patterns[0].id;
        let never = AtomicBool::new(false);
        let plain = render_audio(&song, pat, false, Tail::Cut, &never, &mut |_| {}).unwrap();
        let r = crate::sequence::PluginRef {
            id: 5,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: "LA".into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        song.add_channel("LA", crate::sequence::InstrumentKind::Plugin { plugin: r });
        let plugins = Plugins(vec![(5, crate::plugin::share(LookAhead))]);
        let late = render_with(
            &song,
            pat,
            false,
            Tail::Cut,
            RATE,
            &plugins,
            &never,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(late.len(), plain.len(), "same length");
        let worst = plain
            .iter()
            .zip(&late)
            .map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs()))
            .fold(0f32, f32::max);
        assert!(worst < 1e-6, "aligned with the plain render: {worst}");
    }

    #[test]
    fn stems_split_by_insert_and_leave_master_neutral() {
        let mut song = Song::demo();
        for (n, i) in song.instruments.iter_mut().enumerate() {
            i.mixer = if n == 0 { 1 } else { 2 };
            i.muted = false;
        }
        song.mixer[0]
            .fx
            .push(crate::fx::FxSlot::new(crate::fx::FxKind::ALL[0]));
        song.mixer[0].volume = 0.3;
        // Insert 2 also feeds insert 3 (a reverb bus).
        song.mixer[2]
            .sends
            .push(crate::sequence::Send { to: 3, level: 0.5 });
        let stems = stems(&song);
        let ids: Vec<usize> = stems.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            ids,
            vec![1, 2, 3],
            "inserts 1, 2 and the bus that 2 sends to"
        );
        let (_, one) = &stems[0];
        assert!(
            one.mixer[0].fx.is_empty() && one.mixer[0].volume == 1.0,
            "no Master FX or fader"
        );
        assert!(!one.instruments[0].muted && one.instruments[1..].iter().all(|i| i.muted));
        let (_, bus) = &stems[2];
        assert!(
            !bus.mixer[2].to_master,
            "insert 2 reaches the bus stem only through its send"
        );
        assert!(!bus.mixer[2].muted && bus.instruments[0].muted && !bus.instruments[1].muted);
        let full = render_audio(
            &stems[0].1,
            stems[0].1.patterns[0].id,
            false,
            Tail::Cut,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert!(peak(&full) > 0.0, "a stem has sound");
    }
    use crate::fx::{FxKind, FxSlot};
    use crate::sequence::{BAR, Note, STEP};

    fn go(song: &Song, tail: Tail) -> Vec<[f32; 2]> {
        render_audio(song, 1, false, tail, &AtomicBool::new(false), &mut |_| {}).unwrap()
    }

    fn reverb_song() -> Song {
        let mut s = Song::default();
        s.patterns[0].notes.clear();
        s.add_note(1, Note::new(1, 0, STEP, 60, 110));
        s.patterns[0].length = BAR;
        s.mixer[0].fx.push(FxSlot::new(FxKind::Reverb));
        s
    }

    #[test]
    fn cut_is_exact_leave_rings_out_wrap_folds_back() {
        let s = reverb_song();
        let length = main_frames(&s, 1, false) as usize;
        let cut = go(&s, Tail::Cut);
        assert_eq!(cut.len(), length);
        let leave = go(&s, Tail::Leave);
        assert!(
            leave.len() > length + RATE as usize / 4,
            "the reverb rings past the bar: {}",
            leave.len() - length
        );
        assert!(leave.len() < length + (MAX_TAIL_SECONDS * RATE as f64) as usize);
        let tail_end = &leave[leave.len() - (SILENT_RUN_SECONDS * RATE as f64) as usize..];
        assert!(peak(tail_end) <= SILENCE, "it stops once silent");
        let wrap = go(&s, Tail::Wrap);
        assert_eq!(wrap.len(), length);
        let energy = |v: &[[f32; 2]]| {
            v.iter()
                .map(|f| (f[0] * f[0] + f[1] * f[1]) as f64)
                .sum::<f64>()
        };
        // Wrap adds the ring-out to the start, so the head has more energy than Cut's.
        assert!(energy(&wrap[..length / 4]) > energy(&cut[..length / 4]));
    }

    #[test]
    fn a_dry_song_matches_the_old_render() {
        let s = Song::demo();
        let old = crate::engine::render(&s, 1, false, RATE);
        let new = go(&s, Tail::Leave);
        assert_eq!(
            new[..old.len().min(new.len())],
            old[..old.len().min(new.len())],
            "same engine, same samples"
        );
    }

    #[test]
    fn formats_write_readable_files_at_their_depth() {
        let dir = tempfile::tempdir().unwrap();
        let s = Song::demo();
        let job = Job {
            song: s.clone(),
            pattern: 1,
            song_mode: false,
            tail: Tail::Cut,
            formats: vec![Format::Wav24, Format::Flac16],
            normalize: true,
            dither: true,
            path: dir.path().join("demo.wav"),
            rate: RATE,
            plugins: Plugins::default(),
            range: None,
        };
        let mut events = Vec::new();
        run(&job, &AtomicBool::new(false), &mut |p| events.push(p));
        let Some(Progress::Done(files, stats)) = events.pop() else {
            panic!("{events:?}")
        };
        assert_eq!(files.len(), 2);
        assert!(
            stats.seconds > 1.9 && stats.seconds < 2.1,
            "one bar at 120: {}",
            stats.seconds
        );
        let wav = hound::WavReader::open(&files[0]).unwrap();
        assert_eq!(
            (
                wav.spec().bits_per_sample,
                wav.spec().sample_rate,
                wav.spec().channels
            ),
            (24, RATE, 2)
        );
        let peak24 = wav
            .into_samples::<i32>()
            .map(|s| s.unwrap().abs())
            .max()
            .unwrap() as f32
            / 8_388_607.0;
        assert!(
            (20.0 * peak24.log10() + 1.0).abs() < 0.05,
            "normalized to -1 dBFS: {peak24}"
        );
        let mut flac = claxon::FlacReader::open(&files[1]).unwrap();
        assert_eq!(
            (
                flac.streaminfo().bits_per_sample,
                flac.streaminfo().sample_rate
            ),
            (16, RATE)
        );
        let n = flac.samples().count();
        assert_eq!(
            n,
            main_frames(&s, 1, false) as usize * 2,
            "every sample made it through FLAC"
        );
        assert!(events.iter().any(|e| matches!(e, Progress::Rendering(_))));
    }

    #[test]
    fn export_keeps_float_headroom_for_normalizing() {
        use crate::sequence::{FADER_MAX, InstrumentKind, Note, STEP, SamplePreset};
        let mut song = Song::default();
        song.patterns[0].notes.clear();
        let preset = SamplePreset::from_sample("Tone", RATE, vec![[1.0; 2]; 4096]);
        let id = song.add_channel(
            "Tone",
            InstrumentKind::Sampler {
                preset: Some(std::sync::Arc::new(preset)),
            },
        );
        song.patterns[0]
            .notes
            .push(Note::new(id, 0, STEP * 4, 60, 127));
        song.mixer[0].volume = FADER_MAX;
        let mut frames = go(&song, Tail::Cut);
        assert!(
            peak(&frames) > 1.0,
            "export should preserve the mixer's headroom"
        );
        normalize(&mut frames);
        assert!((20.0 * peak(&frames).log10() + 1.0).abs() < 0.01);
    }

    #[test]
    fn an_mp3_export_carries_the_project_info_when_lame_is_installed() {
        if !crate::mp3::available() {
            return;
        }
        let dir = std::env::var_os("KFXL_KEEP_MP3").map(std::path::PathBuf::from);
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Song::demo();
        s.info.title = "Нічне місто".into();
        s.info.author = "KFXL test".into();
        let job = Job {
            song: s,
            pattern: 1,
            song_mode: false,
            tail: Tail::Cut,
            formats: vec![Format::Mp3_192],
            normalize: false,
            dither: false,
            path: dir.as_deref().unwrap_or(tmp.path()).join("demo.wav"),
            rate: RATE,
            plugins: Plugins::default(),
            range: None,
        };
        let mut events = Vec::new();
        run(&job, &AtomicBool::new(false), &mut |p| events.push(p));
        let Some(Progress::Done(files, _)) = events.pop() else {
            panic!("{events:?}")
        };
        assert_eq!(files[0].extension().unwrap(), "mp3");
        let bytes = std::fs::read(&files[0]).unwrap();
        assert!(
            bytes
                .windows("Нічне місто".len())
                .any(|w| w == "Нічне місто".as_bytes())
        );
        // Two seconds at 192 kbps: about 48 kB.
        assert!(
            (40_000..60_000).contains(&bytes.len()),
            "{} bytes",
            bytes.len()
        );
    }

    #[test]
    fn a_selection_renders_its_bars_and_its_tail_without_what_follows() {
        use crate::sequence::{BAR, Clip};
        let mut s = Song::demo();
        s.clips = (0..4).map(|b| Clip::new(1, b * BAR, 0)).collect();
        let never = AtomicBool::new(false);
        let go = |tail| {
            render_range(
                &s,
                1,
                true,
                Some((BAR, 2 * BAR)),
                tail,
                RATE,
                &Plugins::default(),
                &never,
                &mut |_| {},
            )
            .unwrap()
        };
        let bar = main_frames(&s, 1, false) as usize;
        let cut = go(Tail::Cut);
        assert_eq!(cut.len(), bar, "exactly bar 2");
        assert!(peak(&cut) > 0.05, "the bar plays");
        let full = render_range(
            &s,
            1,
            true,
            None,
            Tail::Cut,
            RATE,
            &Plugins::default(),
            &never,
            &mut |_| {},
        )
        .unwrap();
        // Bar 2 of the whole song, past the ring-over from bar 1, is the same.
        let late = bar / 2..bar;
        let diff = cut[late.clone()]
            .iter()
            .zip(&full[bar + late.start..bar + late.end])
            .map(|(a, b)| (a[0] - b[0]).abs())
            .fold(0f32, f32::max);
        assert!(diff < 1e-4, "same samples as in the song: {diff}");
        let leave = go(Tail::Leave);
        assert!(
            leave.len() < bar + bar / 2,
            "the tail ends: bars 3 and 4 do not play ({} frames)",
            leave.len() - bar
        );
        // The cut song keeps a clip running into the selection's end, shortened.
        let mut long = Song::demo();
        long.clips = vec![
            {
                let mut c = Clip::new(1, BAR / 2, 0);
                c.length = Some(3 * BAR);
                c
            },
            Clip::new(1, 3 * BAR, 1),
        ];
        let c = cut_to(&long, (BAR, 2 * BAR));
        assert_eq!(c.clips.len(), 1);
        assert_eq!(c.clips[0].start + c.clip_len(&c.clips[0]), 2 * BAR);
    }

    #[test]
    fn dither_is_only_for_16_bit_and_cancel_stops() {
        let frames = vec![[0.25f32, -0.25]; 1000];
        let plain = to_int(&frames, 16, false);
        let dithered = to_int(&frames, 16, true);
        assert!(plain.iter().all(|s| s.abs() == 8192));
        assert!(
            dithered.iter().any(|s| s.abs() != 8192),
            "dither moves the LSB"
        );
        assert!(
            dithered.iter().all(|s| (s.abs() - 8192).abs() <= 1),
            "by at most one step"
        );
        let cancel = AtomicBool::new(true);
        assert!(
            render_audio(&Song::demo(), 1, false, Tail::Leave, &cancel, &mut |_| {}).is_none()
                || main_frames(&Song::demo(), 1, false) < 64 * BLOCK as u64
        );
    }
}
