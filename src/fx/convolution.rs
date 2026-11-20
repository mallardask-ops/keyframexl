//! Convolution reverb: the insert's signal is convolved with an impulse
//! response (a .wav file, or a built-in studio IR). Overlap-add with a fixed
//! FFT size keeps latency at zero, so the dry/wet slot mix stays aligned.

use super::Unit::*;
use super::dsp::OnePole;
use super::fft::{Complex, fft};
use super::{Ctx, Effect, ParamSpec, db, p};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const PARAMS: &[ParamSpec] = &[
    p("Pre-delay", 0.0, 200.0, 0.0, false, Ms),
    p("Tone", 500.0, 20000.0, 12000.0, true, Hz),
    p("Gain", -24.0, 12.0, -6.0, false, Db),
];

/// A mono impulse response at its own sample rate.
pub struct Ir {
    pub taps: Vec<f32>,
    pub rate: f32,
}

static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Ir>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<Ir>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remember a decoded IR (the UI calls this after reading a .wav).
pub fn install(path: &Path, taps: Vec<f32>, rate: f32) {
    let ir = Arc::new(Ir { taps, rate });
    if let Ok(mut m) = cache().lock() {
        m.insert(path.to_path_buf(), ir);
    }
}

/// The IR for `path`, if the UI has decoded it.
pub fn cached(path: &Path) -> Option<Arc<Ir>> {
    cache().lock().ok()?.get(path).cloned()
}

/// The longest IR the convolution accepts, in seconds.
const MAX_IR_S: f32 = 0.6;
const MAX_PRE_S: f32 = 0.2;

fn resample(taps: &[f32], from: f32, to: f32) -> Vec<f32> {
    if (from - to).abs() < 0.5 || taps.is_empty() {
        return taps.to_vec();
    }
    let n = (taps.len() as f64 * to as f64 / from as f64).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * from as f64 / to as f64;
            let i0 = (x.floor() as usize).min(taps.len() - 1);
            let i1 = (i0 + 1).min(taps.len() - 1);
            let t = (x - i0 as f64) as f32;
            taps[i0] + (taps[i1] - taps[i0]) * t
        })
        .collect()
}

/// A diffuse decaying IR with a few early reflections, for when no file is
/// loaded. Peak-normalised so a dry impulse does not jump in level.
fn studio(rate: f32) -> Vec<f32> {
    let n = (MAX_IR_S * 0.75 * rate) as usize;
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((seed >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0
    };
    let mut v: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            (-6.9 * t / (0.75 * MAX_IR_S)).exp() * rnd()
        })
        .collect();
    for (ms, g) in [(9.0, 0.7), (17.0, 0.5), (27.0, 0.35), (43.0, 0.22)] {
        let i = (ms / 1000.0 * rate) as usize;
        if i < n {
            v[i] += g;
        }
    }
    let peak = v.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        for x in &mut v {
            *x /= peak;
        }
    }
    v
}

pub struct Convolution {
    taps: Vec<f32>,
    /// Frames per block: the engine's block size, fixed on the first call.
    b: usize,
    /// IR partitions, 2b-point spectra.
    parts: Vec<Vec<Complex>>,
    /// The last input spectra, one per partition (a ring).
    hist: Vec<Vec<Complex>>,
    hist_at: usize,
    acc: Vec<Complex>,
    /// The tail of the last block, added to the next one.
    overlap: [Vec<f32>; 2],
    spec: Vec<Complex>,
    pre: Vec<[f32; 2]>,
    pre_i: usize,
    tone: OnePole,
    rate: f32,
}

impl Convolution {
    pub fn new(rate: f32, ir: Option<Arc<Ir>>) -> Self {
        let rate = rate.max(1000.0);
        let mut taps = match ir {
            Some(ir) => resample(&ir.taps, ir.rate.max(1000.0), rate),
            None => studio(rate),
        };
        taps.truncate((MAX_IR_S * rate) as usize);
        if taps.is_empty() {
            taps.push(1.0);
        }
        Self {
            taps,
            b: 0,
            parts: Vec::new(),
            hist: Vec::new(),
            hist_at: 0,
            acc: Vec::new(),
            overlap: [Vec::new(), Vec::new()],
            spec: Vec::new(),
            pre: vec![[0.0; 2]; (MAX_PRE_S * rate) as usize + 1],
            pre_i: 0,
            tone: OnePole::default(),
            rate,
        }
    }

    /// Split the IR into blocks of `b` and precompute their spectra. Uniform
    /// partitioned convolution: every block is FFT'd once and multiplied by
    /// each partition, so latency stays at zero and long IRs stay cheap.
    fn build(&mut self, b: usize) {
        let b = b.max(1);
        let n = (2 * b).next_power_of_two();
        self.parts.clear();
        let mut at = 0;
        while at < self.taps.len() {
            let end = (at + b).min(self.taps.len());
            let mut spec: Vec<Complex> = self.taps[at..end]
                .iter()
                .map(|t| Complex::new(*t, 0.0))
                .chain(std::iter::repeat(Complex::default()))
                .take(n)
                .collect();
            fft(&mut spec, false);
            self.parts.push(spec);
            at += b;
        }
        let p = self.parts.len();
        self.hist = vec![vec![Complex::default(); n]; p];
        self.hist_at = 0;
        self.acc = vec![Complex::default(); n];
        self.overlap = [vec![0.0; n], vec![0.0; n]];
        self.spec = vec![Complex::default(); n];
        self.b = b;
    }
}

impl Effect for Convolution {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        if (ctx.rate - self.rate).abs() > 0.5 {
            return;
        }
        let frames = buf.len() / 2;
        if frames == 0 {
            return;
        }
        if self.b != frames {
            self.build(frames);
        }
        let b = self.b;
        let n = self.spec.len();
        let parts = self.parts.len();
        self.tone.set(p[1].min(self.rate * 0.45), self.rate);
        let gain = db(p[2]);
        let pre = ((p[0].clamp(0.0, MAX_PRE_S * 1000.0) / 1000.0) * self.rate) as usize;
        let pre = pre.min(self.pre.len().saturating_sub(1));
        for i in 0..b {
            let (l, r) = (buf[2 * i], buf[2 * i + 1]);
            let (dl, dr) = if pre == 0 {
                (l, r)
            } else {
                let len = self.pre.len();
                let idx = (self.pre_i + len - pre) % len;
                let d = self.pre[idx];
                self.pre[self.pre_i] = [l, r];
                self.pre_i = (self.pre_i + 1) % len;
                (d[0], d[1])
            };
            self.spec[i] = Complex::new(dl, dr);
        }
        for slot in &mut self.spec[b..n] {
            *slot = Complex::default();
        }
        fft(&mut self.spec, false);
        self.hist_at = (self.hist_at + 1) % parts.max(1);
        let at = self.hist_at;
        if parts > 0 {
            self.hist[at].copy_from_slice(&self.spec);
            self.acc.iter_mut().for_each(|c| *c = Complex::default());
            for q in 0..parts {
                let idx = (at + parts - q) % parts;
                let h = &self.parts[q];
                for (a, (x, h)) in self.acc.iter_mut().zip(self.hist[idx].iter().zip(h)) {
                    *a += *x * *h;
                }
            }
            fft(&mut self.acc, true);
        }
        for i in 0..b {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            if parts > 0 {
                l = self.acc[i].re + self.overlap[0][i];
                r = self.acc[i].im + self.overlap[1][i];
                self.overlap[0][i] = self.acc[b + i].re;
                self.overlap[1][i] = self.acc[b + i].im;
            }
            buf[2 * i] = self.tone.tick(0, l) * gain;
            buf[2 * i + 1] = self.tone.tick(1, r) * gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::FxKind;
    use super::*;
    const RATE: f32 = 48_000.0;

    fn run(conv: &mut Convolution, input: &[f32]) -> Vec<f32> {
        let mut p = FxKind::Convolution.defaults();
        p[1] = 20000.0;
        p[2] = 0.0;
        let ctx = Ctx {
            rate: RATE,
            tempo: 120.0,
        };
        let mut out = input.to_vec();
        for chunk in out.chunks_mut(512) {
            conv.process(&p, chunk, &ctx);
        }
        out
    }

    fn taps(n: usize) -> Vec<f32> {
        (0..n).map(|i| 1.0 / (i as f32 + 1.0)).collect()
    }

    #[test]
    fn an_impulse_comes_out_as_the_ir() {
        let h = taps(64);
        let ir = Arc::new(Ir {
            taps: h.clone(),
            rate: RATE,
        });
        let mut conv = Convolution::new(RATE, Some(ir));
        let mut input = vec![0.0f32; 2048];
        input[0] = 1.0;
        let out = run(&mut conv, &input);
        // The tone stage is a one-pole low pass: pass the ideal IR through the
        // same filter before comparing.
        let mut tone = OnePole::default();
        tone.set(20000.0, RATE);
        for (i, tap) in h.iter().enumerate() {
            let want = tone.tick(0, *tap);
            assert!(
                (out[2 * i] - want).abs() < 1e-3,
                "tap {i}: {} vs {want}",
                out[2 * i]
            );
            assert!(out[2 * i + 1].abs() < 1e-3);
        }
        assert!(out[2 * h.len() + 2..].iter().all(|v| v.abs() < 1e-3));
    }

    #[test]
    fn an_ir_longer_than_one_block_is_correct() {
        let h: Vec<f32> = (0..600).map(|i| 1.0 / (i as f32 + 1.0)).collect();
        let ir = Arc::new(Ir {
            taps: h.clone(),
            rate: RATE,
        });
        let mut conv = Convolution::new(RATE, Some(ir));
        let mut input = vec![0.0f32; 2048];
        input[0] = 1.0;
        let out = run(&mut conv, &input);
        let mut tone = OnePole::default();
        tone.set(20000.0, RATE);
        for (i, tap) in h.iter().enumerate().take(600) {
            let want = tone.tick(0, *tap);
            assert!(
                (out[2 * i] - want).abs() < 2e-3,
                "tap {i}: {} vs {want}",
                out[2 * i]
            );
        }
    }

    #[test]
    fn silence_stays_silent() {
        let ir = Arc::new(Ir {
            taps: taps(32),
            rate: RATE,
        });
        let mut conv = Convolution::new(RATE, Some(ir));
        let out = run(&mut conv, &vec![0.0f32; 4096]);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn the_cache_round_trips() {
        let path = PathBuf::from("/tmp/kfxl-ir-cache-test.wav");
        install(&path, taps(8), RATE);
        let ir = cached(&path).unwrap();
        assert_eq!(ir.taps.len(), 8);
        assert_eq!(ir.rate, RATE);
    }

    #[test]
    fn the_builtin_ir_is_finite_and_decays() {
        let mut conv = Convolution::new(RATE, None);
        let mut input = vec![0.0f32; 48000];
        input[0] = 1.0;
        let out = run(&mut conv, &input);
        assert!(out.iter().all(|v| v.is_finite()));
        let early: f32 = out[..2400].iter().map(|v| v.abs()).sum();
        let late: f32 = out[24000..].iter().map(|v| v.abs()).sum();
        assert!(early > late * 2.0, "the tail does not decay");
    }
}
