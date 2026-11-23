//! Files for a game: a sound and its variations as `name_01.wav`,
//! `name_02.wav`… (16/24-bit WAV or OGG), trimmed (except seamless loops),
//! optionally peak- or LUFS-normalized, at 44.1 or 48 kHz. Supports WYSIWYG
//! export of pre-auditioned variations and multi-channel Sound Pack batch export.

use super::{ForgePatch, Rendered, mutate};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFormat {
    /// 16-bit PCM: every engine reads it.
    Wav,
    /// 24-bit PCM: studio mastering / high headroom.
    Wav24,
    /// Ogg Vorbis: small, what most engines stream.
    Ogg,
}

impl FileFormat {
    pub fn ext(self) -> &'static str {
        match self {
            FileFormat::Wav | FileFormat::Wav24 => "wav",
            FileFormat::Ogg => "ogg",
        }
    }
}

/// Output loudness target during export.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Loudness {
    /// Keep each sound's authored mix level (preserves UI vs Weapon balance).
    #[default]
    AsMixed,
    /// Normalize peak amplitude to target dBFS (e.g. −1.0 dB).
    PeakDb(f32),
    /// Normalize approximate integrated loudness to target LUFS (e.g. −14.0 LUFS),
    /// soft-clamped below −0.5 dBFS peak to prevent clipping.
    Lufs(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// How many files: the sound itself, then variations of it.
    pub count: usize,
    pub format: FileFormat,
    /// 44_100 or 48_000.
    pub rate: u32,
    /// Legacy peak-normalize flag (−1 dBFS). Honoured when `loudness` is `AsMixed`.
    pub normalize: bool,
    /// Explicit loudness mode (`AsMixed`, `PeakDb`, `Lufs`).
    pub loudness: Loudness,
    /// Left and right averaged into one channel (positioned sounds in a game).
    pub mono: bool,
    /// File names start with this ("jump" → jump_01.wav).
    pub stem: String,
    /// Filename template without extension (`{name}` and `{nn}` / `{n}` placeholders).
    pub template: String,
    /// Seeds the variations.
    pub seed: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            count: 5,
            format: FileFormat::Wav,
            rate: 48_000,
            normalize: false,
            loudness: Loudness::AsMixed,
            mono: false,
            stem: "sound".into(),
            template: "{name}_{nn}".into(),
            seed: 1,
        }
    }
}

impl Options {
    fn effective_loudness(&self) -> Loudness {
        match self.loudness {
            Loudness::AsMixed if self.normalize => Loudness::PeakDb(-1.0),
            other => other,
        }
    }
}

/// A file-name stem from a sound's name: lower case, `_` for spaces and
/// anything a file system or game engine might trip on.
pub fn stem_of(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '-' {
            s.push(c);
        } else if !s.ends_with('_') {
            s.push('_');
        }
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "sound".into() } else { s }
}

/// Format an output filename from stem, 1-based index, template and extension.
pub fn format_filename(stem: &str, index_1based: usize, template: &str, ext: &str) -> String {
    let safe_stem = stem_of(stem);
    let tpl = if template.trim().is_empty() {
        "{name}_{nn}"
    } else {
        template.trim()
    };
    let rendered = tpl
        .replace("{name}", &safe_stem)
        .replace("{nn}", &format!("{index_1based:02}"))
        .replace("{n}", &index_1based.to_string());
    let clean = stem_of(&rendered);
    format!("{clean}.{ext}")
}

/// The patches an export writes: the sound as it is, then `count - 1` variations.
pub fn patches(p: &ForgePatch, opt: &Options) -> Vec<ForgePatch> {
    let mut v = vec![p.clone()];
    v.extend(mutate::variations(
        p,
        opt.count.saturating_sub(1),
        opt.seed,
        mutate::VARIATION,
    ));
    v.truncate(opt.count.max(1));
    v
}

/// Render and write every file into `dir`. Returns the paths written.
pub fn export(p: &ForgePatch, dir: &Path, opt: &Options) -> Result<Vec<PathBuf>, String> {
    export_patches(&patches(p, opt), dir, opt)
}

/// WYSIWYG export: render and write the exact approved patches into `dir`.
pub fn export_patches(
    list: &[ForgePatch],
    dir: &Path,
    opt: &Options,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for (i, patch) in list.iter().enumerate() {
        let rendered = super::render::render(patch);
        let frames = finish(&rendered, opt);
        let fname = format_filename(&opt.stem, i + 1, &opt.template, opt.format.ext());
        let path = dir.join(fname);
        write_audio(&path, &frames, opt)?;
        out.push(path);
    }
    Ok(out)
}

/// Batch Sound Pack export: export multiple `(stem, approved_patches)` channels
/// into `dir` using shared format/rate/loudness settings.
pub fn export_sound_pack(
    channels: &[(String, Vec<ForgePatch>)],
    dir: &Path,
    opt: &Options,
) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for (stem, list) in channels {
        if list.is_empty() {
            continue;
        }
        let mut ch_opt = opt.clone();
        ch_opt.stem = stem.clone();
        out.extend(export_patches(list, dir, &ch_opt)?);
    }
    Ok(out)
}

fn write_audio(path: &Path, frames: &[[f32; 2]], opt: &Options) -> Result<(), String> {
    match opt.format {
        FileFormat::Wav if opt.mono => write_mono_wav(path, frames, opt.rate, 16),
        FileFormat::Wav => {
            crate::render::write_wav_at(path, frames, crate::render::Format::Wav16, true, opt.rate)
        }
        FileFormat::Wav24 if opt.mono => write_mono_wav(path, frames, opt.rate, 24),
        FileFormat::Wav24 => {
            crate::render::write_wav_at(path, frames, crate::render::Format::Wav24, false, opt.rate)
        }
        FileFormat::Ogg => write_ogg(path, frames, opt.rate, opt.mono),
    }
}

/// The rendered sound as the file gets it: silence trimmed from the front
/// (unless seamless loop), resampled, mono-folded, and loudness-matched.
pub fn finish(r: &Rendered, opt: &Options) -> Vec<[f32; 2]> {
    let first = if r.seamless_loop {
        0
    } else {
        r.frames
            .iter()
            .position(|f| f[0].abs().max(f[1].abs()) > 1e-3)
            .unwrap_or(0)
    };
    let mut frames = resample(&r.frames[first..], r.rate, opt.rate);
    if opt.mono {
        for f in &mut frames {
            let m = (f[0] + f[1]) * 0.5;
            *f = [m, m];
        }
    }
    match opt.effective_loudness() {
        Loudness::AsMixed => {}
        Loudness::PeakDb(target_db) => {
            let peak = frames
                .iter()
                .map(|f| f[0].abs().max(f[1].abs()))
                .fold(0.0, f32::max);
            if peak > 1e-6 {
                let target = 10f32.powf(target_db.clamp(-24.0, 0.0) / 20.0);
                let g = target / peak;
                for f in &mut frames {
                    f[0] *= g;
                    f[1] *= g;
                }
            }
        }
        Loudness::Lufs(target_lufs) => {
            let cur = super::render::integrated_lufs(&frames, opt.rate);
            if cur > -90.0 {
                let mut g = 10f32.powf((target_lufs.clamp(-36.0, -4.0) - cur) / 20.0);
                let peak = frames
                    .iter()
                    .map(|f| f[0].abs().max(f[1].abs()))
                    .fold(0.0, f32::max);
                // Guard against clipping above −0.5 dBFS.
                if peak * g > 0.944 {
                    g = 0.944 / peak.max(1e-6);
                }
                for f in &mut frames {
                    f[0] *= g;
                    f[1] *= g;
                }
            }
        }
    }
    frames
}

/// Windowed-sinc (Lanczos, 16 lobes) rate conversion; a copy when the rates match.
pub fn resample(frames: &[[f32; 2]], from: u32, to: u32) -> Vec<[f32; 2]> {
    if from == to || frames.is_empty() {
        return frames.to_vec();
    }
    const A: f64 = 16.0;
    let step = from as f64 / to as f64;
    // Going down, the filter narrows to the new Nyquist.
    let cut = (to as f64 / from as f64).min(1.0);
    let n = ((frames.len() as f64) / step).floor() as usize;
    let sinc = |x: f64| {
        if x.abs() < 1e-9 {
            1.0
        } else {
            let px = std::f64::consts::PI * x;
            px.sin() / px
        }
    };
    (0..n)
        .map(|i| {
            let c = i as f64 * step;
            let reach = (A / cut).ceil() as isize;
            let base = c.floor() as isize;
            let mut acc = [0.0f64; 2];
            for k in base - reach + 1..=base + reach {
                if k < 0 || k as usize >= frames.len() {
                    continue;
                }
                let x = (c - k as f64) * cut;
                if x.abs() >= A {
                    continue;
                }
                let w = cut * sinc(x) * sinc(x / A);
                let f = frames[k as usize];
                acc[0] += f[0] as f64 * w;
                acc[1] += f[1] as f64 * w;
            }
            [acc[0] as f32, acc[1] as f32]
        })
        .collect()
}

/// One channel, 16- or 24-bit PCM (the left of `frames`, already the mix).
fn write_mono_wav(path: &Path, frames: &[[f32; 2]], rate: u32, bits: u16) -> Result<(), String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(err)?;
    if bits == 24 {
        for f in frames {
            w.write_sample((f[0].clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
                .map_err(err)?;
        }
    } else {
        for f in frames {
            w.write_sample((f[0].clamp(-1.0, 1.0) * 32767.0).round() as i16)
                .map_err(err)?;
        }
    }
    w.finalize().map_err(err)
}

fn write_ogg(path: &Path, frames: &[[f32; 2]], rate: u32, mono: bool) -> Result<(), String> {
    use std::num::{NonZeroU8, NonZeroU32};
    let err = |e: vorbis_rs::VorbisError| format!("{}: {e}", path.display());
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = vorbis_rs::VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(rate).ok_or("bad sample rate")?,
        NonZeroU8::new(if mono { 1 } else { 2 }).unwrap(),
        std::io::BufWriter::new(file),
        // A fixed stream serial: the same sound writes the same file.
        0x4B46_584C,
    )
    .build()
    .map_err(err)?;
    for chunk in frames.chunks(4096) {
        let l: Vec<f32> = chunk.iter().map(|f| f[0]).collect();
        if mono {
            enc.encode_audio_block([l]).map_err(err)?;
        } else {
            let r: Vec<f32> = chunk.iter().map(|f| f[1]).collect();
            enc.encode_audio_block([l, r]).map_err(err)?;
        }
    }
    enc.finish().map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::recipes::Recipe;
    use super::*;

    #[test]
    fn stems_are_file_safe() {
        assert_eq!(stem_of("UI click"), "ui_click");
        assert_eq!(stem_of("  Boss / Roar!! "), "boss_roar");
        assert_eq!(stem_of("Стрибок 2"), "стрибок_2");
        assert_eq!(stem_of("///"), "sound");
    }

    #[test]
    fn five_different_files_in_each_format() {
        let dir = tempfile::tempdir().unwrap();
        for (format, rate) in [
            (FileFormat::Wav, 44_100),
            (FileFormat::Wav24, 48_000),
            (FileFormat::Ogg, 48_000),
        ] {
            let opt = Options {
                format,
                rate,
                stem: "Step".into(),
                ..Options::default()
            };
            let paths = export(&Recipe::Step.patch(), dir.path(), &opt).unwrap();
            assert_eq!(paths.len(), 5);
            assert!(paths[0].ends_with(format!("step_01.{}", format.ext())));
            let bytes: Vec<Vec<u8>> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
            for (i, b) in bytes.iter().enumerate() {
                assert!(b.len() > 1000);
                assert!(!bytes[..i].contains(b), "file {i} repeats an earlier one");
            }
        }
        let r = hound::WavReader::open(dir.path().join("step_01.wav")).unwrap();
        assert_eq!(r.spec().sample_rate, 48_000);
        assert_eq!(r.spec().bits_per_sample, 24);
        assert_eq!(r.spec().channels, 2);
    }

    #[test]
    fn normalising_peaks_just_under_full_scale() {
        let r = super::super::render::render(&Recipe::UiClick.patch());
        let opt = Options {
            normalize: true,
            ..Options::default()
        };
        let f = finish(&r, &opt);
        let peak = f
            .iter()
            .map(|f| f[0].abs().max(f[1].abs()))
            .fold(0.0, f32::max);
        assert!((peak - 0.891).abs() < 2e-3);
        // Starts on the sound, not on silence.
        assert!(f[0][0].abs() > 1e-3 || f[0][1].abs() > 1e-3);
    }

    #[test]
    fn lufs_normalization_and_sound_pack_export_work() {
        let r = super::super::render::render(&Recipe::Hum.patch());
        let opt = Options {
            loudness: Loudness::Lufs(-16.0),
            ..Options::default()
        };
        let f = finish(&r, &opt);
        let lufs = super::super::render::integrated_lufs(&f, opt.rate);
        assert!((lufs - (-16.0)).abs() < 1.0, "got {lufs} LUFS");

        let dir = tempfile::tempdir().unwrap();
        let pack = vec![
            ("Jump".into(), vec![Recipe::Jump.patch()]),
            (
                "Coin".into(),
                vec![
                    Recipe::Coin.patch(),
                    mutate::variation_at(&Recipe::Coin.patch(), 1, 7, 0.2),
                ],
            ),
        ];
        let paths = export_sound_pack(&pack, dir.path(), &Options::default()).unwrap();
        assert_eq!(paths.len(), 3);
        assert!(dir.path().join("jump_01.wav").is_file());
        assert!(dir.path().join("coin_01.wav").is_file());
        assert!(dir.path().join("coin_02.wav").is_file());
    }

    #[test]
    fn a_set_keeps_its_levels_and_mono_is_one_channel() {
        let peak = |r: Recipe| {
            let f = finish(
                &super::super::render::render(&r.patch()),
                &Options::default(),
            );
            f.iter()
                .map(|f| f[0].abs().max(f[1].abs()))
                .fold(0.0, f32::max)
        };
        let (click, shot) = (peak(Recipe::UiClick), peak(Recipe::Shot));
        assert!(
            shot > 2.0 * click,
            "a click ({click}) sits well under a shot ({shot})"
        );
        let dir = tempfile::tempdir().unwrap();
        let opt = Options {
            count: 1,
            mono: true,
            stem: "hover".into(),
            ..Options::default()
        };
        export(&Recipe::Lib(0).patch(), dir.path(), &opt).unwrap();
        let r = hound::WavReader::open(dir.path().join("hover_01.wav")).unwrap();
        assert_eq!(r.spec().channels, 1);
        let ogg = Options {
            format: FileFormat::Ogg,
            ..opt
        };
        export(&Recipe::Lib(0).patch(), dir.path(), &ogg).unwrap();
        assert!(dir.path().join("hover_01.ogg").is_file());
    }

    #[test]
    fn resampling_keeps_a_tone() {
        let tone: Vec<[f32; 2]> = (0..48_000)
            .map(|i| {
                let v = (i as f32 * 1000.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        let out = resample(&tone, 48_000, 44_100);
        assert!((out.len() as i64 - 44_100).abs() <= 1);
        // Same frequency: zero crossings per second stay ~2000.
        let crossings = out[1000..43_000]
            .windows(2)
            .filter(|w| (w[0][0] < 0.0) != (w[1][0] < 0.0))
            .count() as f64
            / (42_000.0 / 44_100.0);
        assert!((crossings - 2000.0).abs() < 10.0, "{crossings}");
        let peak = out[1000..43_000]
            .iter()
            .map(|f| f[0].abs())
            .fold(0.0, f32::max);
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }
}
