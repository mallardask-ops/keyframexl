//! Small DSP helpers for the vocal renderer.

/// Windowed-sinc (Lanczos) resampling of a whole buffer.
pub fn resample(x: &[f64], from: u32, to: u32) -> Vec<f64> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let n = (x.len() as f64 * ratio).round() as usize;
    // Lowpass at the lower Nyquist when going down.
    let cutoff = ratio.min(1.0);
    const A: i64 = 8;
    let sinc = |t: f64| {
        if t.abs() < 1e-9 {
            1.0
        } else {
            (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
        }
    };
    (0..n)
        .map(|i| {
            let pos = i as f64 / ratio;
            let base = pos.floor() as i64;
            let span = (A as f64 / cutoff).ceil() as i64;
            let mut acc = 0.0;
            let mut wsum = 0.0;
            for k in (base - span + 1)..=(base + span) {
                if k < 0 || k >= x.len() as i64 {
                    continue;
                }
                let d = (pos - k as f64) * cutoff;
                if d.abs() >= A as f64 {
                    continue;
                }
                let wgt = sinc(d) * sinc(d / A as f64);
                acc += x[k as usize] * wgt;
                wsum += wgt;
            }
            if wsum.abs() > 1e-9 { acc / wsum } else { 0.0 }
        })
        .collect()
}
