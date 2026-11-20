//! Time stretch (pitch kept) with Signalsmith Stretch (vendor/signalsmith,
//! MIT): a whole buffer to an exact length, the same bytes on every run.

unsafe extern "C" {
    fn kfx_stretch(
        seed: u64,
        channels: i32,
        sample_rate: f32,
        input: *const f32,
        in_len: i32,
        output: *mut f32,
        out_len: i32,
        cheaper: i32,
    ) -> i32;
}

/// `frames` (stereo, at `rate`) stretched to exactly `out_len` frames.
/// Too short an input (under ~0.1 s) is resampled plainly instead.
pub fn stretch(frames: &[[f32; 2]], rate: u32, out_len: usize, seed: u64) -> Vec<[f32; 2]> {
    if out_len == 0 || frames.is_empty() {
        return vec![[0.0; 2]; out_len];
    }
    if out_len == frames.len() {
        return frames.to_vec();
    }
    let n = frames.len();
    let mut input = vec![0f32; 2 * n];
    for (i, f) in frames.iter().enumerate() {
        input[i] = f[0];
        input[n + i] = f[1];
    }
    let mut output = vec![0f32; 2 * out_len];
    // SAFETY: both buffers hold 2 planar channels of the lengths passed.
    let ok = unsafe {
        kfx_stretch(
            seed,
            2,
            rate as f32,
            input.as_ptr(),
            n as i32,
            output.as_mut_ptr(),
            out_len as i32,
            0,
        )
    };
    if ok == 0 {
        return resample(frames, out_len);
    }
    (0..out_len)
        .map(|i| [output[i], output[out_len + i]])
        .collect()
}

/// Linear resampling to `out_len` frames (pitch moves with the length).
fn resample(frames: &[[f32; 2]], out_len: usize) -> Vec<[f32; 2]> {
    let step = frames.len() as f64 / out_len as f64;
    (0..out_len)
        .map(|i| {
            let x = i as f64 * step;
            let k = (x as usize).min(frames.len() - 1);
            let f = (x - k as f64) as f32;
            let b = frames[(k + 1).min(frames.len() - 1)];
            let a = frames[k];
            [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, secs: f32, rate: u32) -> Vec<[f32; 2]> {
        (0..(secs * rate as f32) as usize)
            .map(|i| {
                let v = (i as f32 * hz * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    /// Zero crossings per second of the left channel, away from the ends.
    fn freq(f: &[[f32; 2]], rate: u32) -> f32 {
        let a = f.len() / 5;
        let b = f.len() - a;
        let crossings = f[a..b]
            .windows(2)
            .filter(|w| (w[0][0] < 0.0) != (w[1][0] < 0.0))
            .count();
        crossings as f32 / 2.0 / ((b - a) as f32 / rate as f32)
    }

    #[test]
    fn longer_and_shorter_keep_the_pitch_and_the_exact_length() {
        let rate = 48_000;
        let src = tone(440.0, 1.0, rate);
        for out in [36_000usize, 48_000 * 3 / 2, 72_001] {
            let s = stretch(&src, rate, out, 7);
            assert_eq!(s.len(), out);
            let hz = freq(&s, rate);
            assert!((hz - 440.0).abs() < 3.0, "{out}: {hz} Hz");
            // The level holds in the body (0.5 peak → 0.354 RMS).
            let body = &s[out / 5..out - out / 5];
            let rms = (body.iter().map(|f| f[0] * f[0]).sum::<f32>() / body.len() as f32).sqrt();
            assert!((rms - 0.354).abs() < 0.05, "{out}: rms {rms}");
            let peak = s.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
            println!("{out}: peak {peak} rms {rms}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_bytes() {
        let rate = 44_100;
        let mut src = tone(220.0, 0.8, rate);
        // A click: transients randomise phases in the library.
        src[10_000] = [0.9, -0.9];
        let a = stretch(&src, rate, 60_000, 3);
        let b = stretch(&src, rate, 60_000, 3);
        assert_eq!(a, b);
    }
}
