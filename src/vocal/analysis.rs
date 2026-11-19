//! WORLD analysis of one voicebank wav: pitch, spectral envelope, aperiodicity
//! per 5 ms frame. Analysis is the slow part, so results are cached on disk.

use rsworld_sys as w;
use std::path::{Path, PathBuf};

pub const FS: i32 = 44_100;
pub const FRAME_MS: f64 = 5.0;
/// Bumped when the analysis changes, so stale cache files are not read.
const VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"KVA1";

pub struct Analysis {
    pub frames: usize,
    pub bins: usize,
    /// Hz per frame, 0 = unvoiced.
    pub f0: Vec<f32>,
    /// ln of the power spectrum, `frames * bins`.
    pub lnsp: Vec<f32>,
    /// Aperiodicity 0..1, `frames * bins`.
    pub ap: Vec<f32>,
    /// Length of the wav in ms.
    pub wav_ms: f64,
}

impl Analysis {
    pub fn lnsp_row(&self, i: usize) -> &[f32] {
        &self.lnsp[i * self.bins..(i + 1) * self.bins]
    }
    pub fn ap_row(&self, i: usize) -> &[f32] {
        &self.ap[i * self.bins..(i + 1) * self.bins]
    }
}

/// A wav as mono f64 at `FS`.
pub fn load_wav(path: &Path) -> Result<Vec<f64>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .filter_map(|s| s.ok())
            .map(|s| s as f64)
            .collect(),
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f64;
            if spec.bits_per_sample <= 16 {
                r.samples::<i16>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f64 / scale)
                    .collect()
            } else {
                r.samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f64 / scale)
                    .collect()
            }
        }
    };
    let mono: Vec<f64> = if ch == 1 {
        raw
    } else {
        raw.chunks(ch)
            .map(|c| c.iter().sum::<f64>() / ch as f64)
            .collect()
    };
    Ok(if spec.sample_rate as i32 == FS {
        mono
    } else {
        super::dsp::resample(&mono, spec.sample_rate, FS as u32)
    })
}

fn cache_file(wav: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(wav).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in wav
        .to_string_lossy()
        .bytes()
        .chain(meta.len().to_le_bytes())
        .chain(mtime.to_le_bytes())
        .chain(VERSION.to_le_bytes())
    {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    Some(
        dirs::cache_dir()?
            .join("keyframexl")
            .join("vocal")
            .join(format!("{h:016x}.kva")),
    )
}

fn read_cache(path: &Path) -> Option<Analysis> {
    let b = std::fs::read(path).ok()?;
    if b.get(..4)? != MAGIC {
        return None;
    }
    let u32_at = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let frames = u32_at(4)? as usize;
    let bins = u32_at(8)? as usize;
    let wav_ms = f32::from_bits(u32_at(12)?) as f64;
    let mut o = 16;
    let n = frames * bins;
    if b.len() != o + frames * 4 + n * 2 + n * 2 {
        return None;
    }
    let f0 = (0..frames)
        .map(|i| f32::from_bits(u32_at(o + i * 4).unwrap_or(0)))
        .collect();
    o += frames * 4;
    let lnsp = (0..n)
        .map(|i| i16::from_le_bytes([b[o + i * 2], b[o + i * 2 + 1]]) as f32 / 400.0)
        .collect();
    o += n * 2;
    let ap = (0..n)
        .map(|i| u16::from_le_bytes([b[o + i * 2], b[o + i * 2 + 1]]) as f32 / 65535.0)
        .collect();
    Some(Analysis {
        frames,
        bins,
        f0,
        lnsp,
        ap,
        wav_ms,
    })
}

fn write_cache(path: &Path, a: &Analysis) {
    let mut b = Vec::with_capacity(16 + a.frames * 4 + a.frames * a.bins * 4);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&(a.frames as u32).to_le_bytes());
    b.extend_from_slice(&(a.bins as u32).to_le_bytes());
    b.extend_from_slice(&(a.wav_ms as f32).to_bits().to_le_bytes());
    for f in &a.f0 {
        b.extend_from_slice(&f.to_bits().to_le_bytes());
    }
    for v in &a.lnsp {
        b.extend_from_slice(&((v * 400.0).round().clamp(-32000.0, 32000.0) as i16).to_le_bytes());
    }
    for v in &a.ap {
        b.extend_from_slice(&((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes());
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &b).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Analyse `x` (mono, `FS`).
pub fn analyse(x: &[f64], wav_ms: f64) -> Analysis {
    let x = if x.len() < 2048 {
        // WORLD needs a little material to work with.
        let mut v = x.to_vec();
        v.resize(2048, 0.0);
        v
    } else {
        x.to_vec()
    };
    unsafe {
        let mut dio = w::DioOption::new();
        dio.f0_floor = 70.0;
        dio.f0_ceil = 900.0;
        dio.frame_period = FRAME_MS;
        dio.speed = 1;
        let n = w::GetSamplesForDIO(FS, x.len() as i32, FRAME_MS) as usize;
        let mut t = vec![0.0f64; n];
        let mut f0 = vec![0.0f64; n];
        w::Dio(
            x.as_ptr(),
            x.len() as i32,
            FS,
            &dio,
            t.as_mut_ptr(),
            f0.as_mut_ptr(),
        );
        let mut refined = vec![0.0f64; n];
        w::StoneMask(
            x.as_ptr(),
            x.len() as i32,
            FS,
            t.as_ptr(),
            f0.as_ptr(),
            n as i32,
            refined.as_mut_ptr(),
        );
        // StoneMask can lose a frame's voicing; keep DIO's decision.
        for (r, o) in refined.iter_mut().zip(&f0) {
            if *o <= 0.0 {
                *r = 0.0;
            }
        }
        let mut ct = w::CheapTrickOption::new(FS);
        ct.f0_floor = 70.0;
        w::GetFFTSizeForCheapTrick(FS, &mut ct);
        let bins = (ct.fft_size / 2 + 1) as usize;
        let mut sp = vec![0.0f64; n * bins];
        let mut ap = vec![0.0f64; n * bins];
        let mut sp_rows: Vec<*mut f64> = (0..n).map(|i| sp.as_mut_ptr().add(i * bins)).collect();
        let mut ap_rows: Vec<*mut f64> = (0..n).map(|i| ap.as_mut_ptr().add(i * bins)).collect();
        w::CheapTrick(
            x.as_ptr(),
            x.len() as i32,
            FS,
            t.as_ptr(),
            refined.as_ptr(),
            n as i32,
            &ct,
            sp_rows.as_mut_ptr(),
        );
        let d4c = w::D4COption::new();
        w::D4C(
            x.as_ptr(),
            x.len() as i32,
            FS,
            t.as_ptr(),
            refined.as_ptr(),
            n as i32,
            ct.fft_size,
            &d4c,
            ap_rows.as_mut_ptr(),
        );
        Analysis {
            frames: n,
            bins,
            f0: refined.iter().map(|v| *v as f32).collect(),
            lnsp: sp.iter().map(|v| (v.max(1e-30)).ln() as f32).collect(),
            ap: ap.iter().map(|v| v.clamp(0.0, 1.0) as f32).collect(),
            wav_ms,
        }
    }
}

/// Analysis of a bank wav: from the disk cache, else computed and cached.
pub fn load(wav: &Path) -> Result<Analysis, String> {
    let cache = cache_file(wav);
    if let Some(a) = cache.as_deref().and_then(read_cache) {
        return Ok(a);
    }
    let x = load_wav(wav)?;
    let a = analyse(&x, x.len() as f64 * 1000.0 / FS as f64);
    if let Some(c) = &cache {
        write_cache(c, &a);
    }
    Ok(a)
}

/// Synthesise frames back to audio at `FS`. `sp` is linear power, `ap` 0..1.
pub fn synthesise(f0: &[f64], sp: &mut [f64], ap: &mut [f64], bins: usize) -> Vec<f64> {
    let n = f0.len();
    if n == 0 {
        return Vec::new();
    }
    let fft = (bins - 1) * 2;
    let len = (n as f64 * FRAME_MS / 1000.0 * FS as f64) as usize;
    let mut y = vec![0.0f64; len];
    let sp_rows: Vec<*const f64> = (0..n).map(|i| sp[i * bins..].as_ptr()).collect();
    let ap_rows: Vec<*const f64> = (0..n).map(|i| ap[i * bins..].as_ptr()).collect();
    unsafe {
        w::Synthesis(
            f0.as_ptr(),
            n as i32,
            sp_rows.as_ptr(),
            ap_rows.as_ptr(),
            fft as i32,
            FRAME_MS,
            FS,
            len as i32,
            y.as_mut_ptr(),
        );
    }
    y
}
