//! Files for a game: a sound and its variations as `name_01.wav`,
//! `name_02.wav`… (or .ogg), trimmed, optionally normalised, at 44.1 or 48 kHz.

use super::{ForgePatch, Rendered, mutate};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFormat {
    /// 16-bit PCM: every engine reads it.
    Wav,
    /// Ogg Vorbis: small, what most engines stream.
    Ogg,
}

impl FileFormat {
    pub fn ext(self) -> &'static str {
        match self {
            FileFormat::Wav => "wav",
            FileFormat::Ogg => "ogg",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// How many files: the sound itself, then variations of it.
    pub count: usize,
    pub format: FileFormat,
    /// 44_100 or 48_000.
    pub rate: u32,
    /// Peak at −1 dBFS.
    pub normalize: bool,
    /// File names start with this ("jump" → jump_01.wav).
    pub stem: String,
    /// Seeds the variations.
    pub seed: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            count: 5,
            format: FileFormat::Wav,
            rate: 48_000,
            normalize: true,
            stem: "sound".into(),
            seed: 1,
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

/// The patches an export writes: the sound as it is, then `count - 1` variations.
pub fn patches(p: &ForgePatch, opt: &Options) -> Vec<ForgePatch> {
    let mut v = vec![p.clone()];
    v.extend(mutate::variations(p, opt.count.saturating_sub(1), opt.seed, mutate::VARIATION));
    v.truncate(opt.count.max(1));
    v
}

/// Render and write every file into `dir`. Returns the paths written.
pub fn export(p: &ForgePatch, dir: &Path, opt: &Options) -> Result<Vec<PathBuf>, String> {
    let stem = stem_of(&opt.stem);
    let mut out = Vec::new();
    for (i, patch) in patches(p, opt).iter().enumerate() {
        let frames = finish(&super::render::render(patch), opt);
        let path = dir.join(format!("{stem}_{:02}.{}", i + 1, opt.format.ext()));
        match opt.format {
            FileFormat::Wav => crate::render::write_wav_at(
                &path,
                &frames,
                crate::render::Format::Wav16,
                true,
                opt.rate,
            )?,
            FileFormat::Ogg => write_ogg(&path, &frames, opt.rate)?,
        }
        out.push(path);
    }
    Ok(out)
}

/// The rendered sound as the file gets it: silence trimmed from the front,
/// resampled, normalised.
pub fn finish(r: &Rendered, opt: &Options) -> Vec<[f32; 2]> {
    let first = r
        .frames
        .iter()
        .position(|f| f[0].abs().max(f[1].abs()) > 1e-3)
        .unwrap_or(0);
    let mut frames = resample(&r.frames[first..], r.rate, opt.rate);
    if opt.normalize {
        let peak = frames
            .iter()
            .map(|f| f[0].abs().max(f[1].abs()))
            .fold(0.0, f32::max);
        if peak > 1e-6 {
            let g = 0.891 / peak;
            for f in &mut frames {
                f[0] *= g;
                f[1] *= g;
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

fn write_ogg(path: &Path, frames: &[[f32; 2]], rate: u32) -> Result<(), String> {
    use std::num::{NonZeroU8, NonZeroU32};
    let err = |e: vorbis_rs::VorbisError| format!("{}: {e}", path.display());
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = vorbis_rs::VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(rate).ok_or("bad sample rate")?,
        NonZeroU8::new(2).unwrap(),
        std::io::BufWriter::new(file),
        // A fixed stream serial: the same sound writes the same file.
        0x4B46_584C,
    )
    .build()
    .map_err(err)?;
    for chunk in frames.chunks(4096) {
        let l: Vec<f32> = chunk.iter().map(|f| f[0]).collect();
        let r: Vec<f32> = chunk.iter().map(|f| f[1]).collect();
        enc.encode_audio_block([l, r]).map_err(err)?;
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
        for (format, rate) in [(FileFormat::Wav, 44_100), (FileFormat::Ogg, 48_000)] {
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
        assert_eq!(r.spec().sample_rate, 44_100);
        assert_eq!(r.spec().channels, 2);
    }

    #[test]
    fn normalising_peaks_just_under_full_scale() {
        let r = super::super::render::render(&Recipe::UiClick.patch());
        let f = finish(&r, &Options::default());
        let peak = f.iter().map(|f| f[0].abs().max(f[1].abs())).fold(0.0, f32::max);
        assert!((peak - 0.891).abs() < 1e-3);
        // Starts on the sound, not on silence.
        assert!(f[0][0].abs() > 1e-3 || f[0][1].abs() > 1e-3);
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
        let peak = out[1000..43_000].iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }
}
