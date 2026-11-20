//! FL Studio's own effects as built-in stand-ins: what their saved settings
//! (the slot's plugin data, 32-bit values) and automation (parameter numbers,
//! 0..1 across each parameter's raw range) mean for our effects.
//!
//! Layouts and value scales follow FL Studio 2025 files and the open DawVert
//! converter (its `fl_studio.ddef` / `.dset` and FL-to-universal formulas).
//! The match is approximate: the algorithms differ, the settings carry over.

use crate::fx::{FxKind, FxSlot};

/// One FL plugin parameter feeding one of ours.
struct Link {
    /// FL's parameter number (what automation names); `None`: not automatable here.
    fl: Option<u16>,
    /// Index of the 32-bit value in the plugin data.
    at: usize,
    /// The raw range automation's 0..1 spans.
    lo: f32,
    hi: f32,
    /// Our parameter index.
    ours: usize,
    map: fn(f32) -> f32,
}

const fn link(fl: u16, at: usize, lo: f32, hi: f32, ours: usize, map: fn(f32) -> f32) -> Link {
    Link {
        fl: Some(fl),
        at,
        lo,
        hi,
        ours,
        map,
    }
}

const fn fixed(at: usize, ours: usize, map: fn(f32) -> f32) -> Link {
    Link {
        fl: None,
        at,
        lo: 0.0,
        hi: 0.0,
        ours,
        map,
    }
}

fn db(amp: f32) -> f32 {
    20.0 * amp.max(1e-6).log10()
}

fn eq_gain(r: f32) -> f32 {
    r / 100.0
}
/// 20 Hz .. 20 kHz, exponential.
fn eq_freq(r: f32) -> f32 {
    20.0 * 1000f32.powf(r / 65536.0)
}
fn eq_peak_q(r: f32) -> f32 {
    1.0 / ((r / 65536.0 + 0.01) * 3.0)
}
fn eq_shelf_q(r: f32) -> f32 {
    (1.0 - r / 65536.0) * 1.2
}
fn tenth(r: f32) -> f32 {
    r / 10.0
}
fn same(r: f32) -> f32 {
    r
}
fn of_128(r: f32) -> f32 {
    r / 128.0 * 100.0
}
fn volume_320(r: f32) -> f32 {
    r / 320.0 * 200.0
}
/// PanOMatic's pan runs 0..256 with 128 in the centre.
fn pano_pan(r: f32) -> f32 {
    (r - 128.0) / 128.0 * 100.0
}
/// Stereo Shaper's matrix gains are ±25600.
fn matrix_256(r: f32) -> f32 {
    r / 256.0
}
fn shaper_delay(r: f32) -> f32 {
    (r / 48_000.0 * 1000.0).clamp(0.0, 100.0)
}
fn shaper_phase(r: f32) -> f32 {
    (r / 4096.0 * 180.0).clamp(-180.0, 180.0)
}
fn bank_tone(r: f32) -> f32 {
    500.0 * 40f32.powf(r / 65536.0)
}
fn of_6000(r: f32) -> f32 {
    r / 6000.0 * 100.0
}
/// FL Delay 3's tone knob, 0..3000, as a low-pass: 500 Hz .. 20 kHz.
fn delay3_tone(r: f32) -> f32 {
    500.0 * 40f32.powf(r / 3000.0)
}
fn of_1024(r: f32) -> f32 {
    r / 1024.0 * 100.0
}
/// FL Reeverb 2 damping is a cutoff in 100 Hz steps (high = bright): ours
/// is an amount, 0 at 22 kHz up to 100 at 500 Hz.
fn damping(r: f32) -> f32 {
    let hz = (r * 100.0).max(500.0);
    100.0 * (1.0 - (hz / 500.0).ln() / (22100f32 / 500.0).ln())
}
/// FL tempo-synced delay times are in 1/48 steps.
fn steps(r: f32) -> f32 {
    r / 48.0
}
fn offset_steps(raw: f32, curve: i32) -> f32 {
    raw.signum() * (raw.abs() / 512.0).clamp(0.0, 1.0).powi(curve) * 4.0
}
fn delay2_offset(raw: f32) -> f32 {
    offset_steps(raw, 4)
}
fn ping_pong(r: f32) -> f32 {
    if r >= 1.5 { 100.0 } else { 0.0 }
}
fn fb_tone(r: f32) -> f32 {
    500.0 * 40f32.powf(r / 128.0)
}
fn feedback3(r: f32) -> f32 {
    r / 6000.0 * 100.0
}
fn filter_cutoff(r: f32) -> f32 {
    (r / 1024.0 * 2.0).powf(0.95).powf(3.325) * 1000.0
}
fn fast_lp_cutoff(r: f32) -> f32 {
    ((0.2 + r / 1024.0).powf(1.5) * 100.0).powi(2) * 925.0 / 2048.0
}
/// FL's resonance as a Q of 1..7 (DawVert), ours as the filter's damping.
fn fast_lp_reso(r: f32) -> f32 {
    let q = r / 1024.0 * 6.0 + 1.0;
    (2.0 - 1.0 / q) / 1.96 * 100.0
}
fn chorus_delay(r: f32) -> f32 {
    0.05 + r / 1024.0 * 29.95
}
fn chorus_rate(r: f32) -> f32 {
    (r / 5000.0).powi(2) * 5.0
}
fn of_5000(r: f32) -> f32 {
    r / 5000.0 * 100.0
}
fn width(r: f32) -> f32 {
    100.0 + r / 96.0 * 100.0
}
fn dist_drive(r: f32) -> f32 {
    r / 192.0 * 36.0
}
fn dist_out(r: f32) -> f32 {
    db(r / 128.0)
}
fn feedback_6000(r: f32) -> f32 {
    r / 6000.0 * 85.0
}
fn feedback_1000(r: f32) -> f32 {
    r / 1000.0 * 85.0
}
fn clip_threshold(r: f32) -> f32 {
    db(r / 127.0)
}
fn clip_output(r: f32) -> f32 {
    if r > 128.0 {
        (r - 128.0) * 0.176
    } else {
        db((r / 128.0).powi(2))
    }
}
fn flanger_delay(r: f32) -> f32 {
    1.0 + r / 1024.0 * 9.0
}
fn flanger_rate(r: f32) -> f32 {
    0.05 + (r / 5000.0).powi(2) * 2.0
}
fn flangus_rate(r: f32) -> f32 {
    0.05 + (r / 1024.0).powi(2) * 4.95
}
fn flangus_delay(r: f32) -> f32 {
    1.0 + r / 128.0 * 29.0
}
fn blood_drive(r: f32) -> f32 {
    db(1.5 * (1.0 + 3.6 * (r / 10000.0).powi(2)))
}
fn blood_post_filter(r: f32) -> f32 {
    20_000.0 * 40f32.powf(-r / 10000.0)
}
fn hundreds(r: f32) -> f32 {
    r * 100.0
}
fn reverb_width(r: f32) -> f32 {
    100.0 - r.max(0.0) / 64.0 * 100.0
}
fn gain_10000(r: f32) -> f32 {
    db(r / 10000.0)
}
fn phaser_rate(r: f32) -> f32 {
    0.03 + (r / 5000.0).powi(2) * 5.97
}
/// Transient Processor knobs: 0..160, 80 = neutral.
fn transient_pct(r: f32) -> f32 {
    (r - 80.0) / 80.0 * 100.0
}
fn transient_db(r: f32) -> f32 {
    (r - 80.0) / 80.0 * 12.0
}
fn feedback_1024_sq(r: f32) -> f32 {
    (r / 1024.0).powi(2) * 100.0
}
fn tone_1024(r: f32) -> f32 {
    500.0 * 40f32.powf(r / 1024.0)
}
fn of_65536(r: f32) -> f32 {
    r / 65536.0 * 100.0
}
fn lowcut_65536(r: f32) -> f32 {
    20.0 * 50f32.powf(r / 65536.0)
}
/// Fruity Squeeze: the step size (1..32768 of 16-bit) as our 16→1-bit Squarize.
fn squeeze_steps(r: f32) -> f32 {
    r.max(1.0).log2() / 15.0 * 100.0
}
fn squeeze_gain(r: f32) -> f32 {
    r / 127.0 * 100.0
}
/// Its filter: 50 Hz .. 33 kHz on an exponential scale.
fn squeeze_freq(r: f32) -> f32 {
    50.0 * (33_075.0f32 / 50.0).powf(r / 65536.0)
}
/// Preserve and Impact: 44101^(r/44100) − 1 samples at 44.1 kHz, as a share
/// of our 50 ms.
fn squeeze_time(r: f32) -> f32 {
    let samples = 44_101f32.powf(r / 44_100.0) - 1.0;
    samples / 44_100.0 / 0.05 * 100.0
}
/// Hyper Chorus values are 0..1000.
fn chorus_ms(r: f32) -> f32 {
    1.0 + r / 1000.0 * 29.0
}
fn chorus_feedback(r: f32) -> f32 {
    r / 1000.0 * 60.0
}
fn chorus_hz(r: f32) -> f32 {
    0.05 * 100f32.powf(r / 1000.0)
}
/// Parametric EQ (the first): 10 Hz .. 16 kHz on a curved scale (DawVert).
fn eq1_freq(r: f32) -> f32 {
    10.0 * 1600f32.powf((r / 65536.0).max(0.0).powf(0.6))
}
fn spread_width(r: f32) -> f32 {
    100.0 + r / 100.0
}

/// The stand-in for an FL effect and how its settings carry over. `None`
/// links: the stand-in has no reading of FL's settings (left switched off).
fn table(fl: &str) -> Option<(FxKind, Option<Vec<Link>>)> {
    let kind = crate::flp::native_effect(fl)?;
    let n = fl.to_lowercase();
    let links = match n.as_str() {
        "fruity parametric eq 2" => Some(
            (0..7)
                .flat_map(|b| {
                    let shelf = b == 0 || b == 6;
                    [
                        link(b as u16, 1 + b, -1800.0, 1800.0, 3 * b + 1, eq_gain),
                        link(7 + b as u16, 8 + b, 0.0, 65536.0, 3 * b, eq_freq),
                        link(
                            14 + b as u16,
                            15 + b,
                            0.0,
                            65536.0,
                            3 * b + 2,
                            if shelf { eq_shelf_q } else { eq_peak_q },
                        ),
                    ]
                })
                .collect(),
        ),
        "fruity compressor" => Some(vec![
            link(0, 1, -600.0, 0.0, 0, tenth),
            link(1, 2, 0.0, 300.0, 1, tenth),
            link(3, 4, 0.0, 4000.0, 2, tenth),
            link(4, 5, 0.0, 4000.0, 3, same),
            link(2, 3, 0.0, 300.0, 4, tenth),
        ]),
        "fruity reeverb 2" => Some(vec![
            link(3, 4, 0.0, 100.0, 0, same),
            link(6, 7, 0.0, 221.0, 1, damping),
            link(0, 1, 0.0, 3000.0, 3, same),
            link(12, 13, 0.0, 160.0, 4, of_128),
            link(10, 11, 0.0, 160.0, 5, of_128),
            link(5, 6, 1.0, 200.0, 6, hundreds),
            link(1, 2, 0.0, 221.0, 7, hundreds),
            link(2, 3, 0.0, 384.0, 8, same),
            link(11, 12, 0.0, 160.0, 9, of_128),
            link(9, 10, -64.0, 64.0, 2, reverb_width),
        ]),
        "fruity delay 2" => Some(vec![
            link(4, 4, 0.0, 768.0, 0, steps),
            link(3, 3, 0.0, 128.0, 1, of_128),
            link(6, 6, 0.0, 2.0, 2, ping_pong),
            link(7, 7, 0.0, 128.0, 3, fb_tone),
            link(1, 1, 0.0, 160.0, 4, of_128),
            link(2, 2, 0.0, 160.0, 5, of_128),
            link(5, 5, -512.0, 512.0, 6, delay2_offset),
            link(0, 0, -128.0, 128.0, 7, of_128),
        ]),
        "fruity delay bank" => Some(vec![]),
        "fruity delay 3" => Some(vec![
            fixed(5, 0, steps),
            fixed(15, 1, feedback3),
            fixed(8, 2, of_6000),
            fixed(25, 3, delay3_tone),
            fixed(24, 4, of_6000),
            fixed(26, 5, of_6000),
        ]),
        "fruity fast lp" => Some(vec![
            link(0, 1, 0.0, 1024.0, 1, fast_lp_cutoff),
            link(1, 2, 0.0, 1024.0, 2, fast_lp_reso),
        ]),
        "fruity balance" => Some(vec![
            link(0, 0, -128.0, 128.0, 1, of_128),
            link(1, 1, 0.0, 320.0, 0, volume_320),
        ]),
        // PanOMatic's LFO is not carried over; its pan and volume are.
        "fruity panomatic" => Some(vec![
            link(0, 0, 0.0, 256.0, 1, pano_pan),
            link(1, 1, 0.0, 320.0, 0, volume_320),
        ]),
        // Fruity Stereo Shaper: r2l, l2l, r2r, l2r, delay, dephase.
        "fruity stereo shaper" => Some(vec![
            link(1, 2, -25600.0, 25600.0, 0, matrix_256),
            link(2, 3, -25600.0, 25600.0, 1, matrix_256),
            link(3, 4, -25600.0, 25600.0, 2, matrix_256),
            link(0, 1, -25600.0, 25600.0, 3, matrix_256),
            link(4, 5, -4096.0, 4096.0, 4, shaper_delay),
            link(5, 6, -4096.0, 4096.0, 5, shaper_phase),
        ]),
        "fruity filter" => Some(vec![
            link(0, 1, 0.0, 1024.0, 1, filter_cutoff),
            link(1, 2, 0.0, 1024.0, 2, of_1024),
        ]),
        "fruity chorus" => Some(vec![
            link(0, 1, 0.0, 1024.0, 0, chorus_delay),
            link(1, 2, 0.0, 5000.0, 1, of_5000),
            link(3, 4, 0.0, 5000.0, 2, chorus_rate),
        ]),
        "fruity stereo enhancer" => Some(vec![
            link(2, 2, -96.0, 96.0, 0, width),
            link(0, 0, -128.0, 128.0, 1, of_128),
        ]),
        "fruity fast dist" => Some(vec![
            link(0, 0, 0.0, 192.0, 0, dist_drive),
            link(4, 4, 0.0, 128.0, 2, dist_out),
            link(3, 3, 0.0, 128.0, 3, of_128),
        ]),
        // Soundgoodizer: version, mode, amount (DawVert's struct). Bloom has
        // the same one knob and four A-D profiles.
        "soundgoodizer" => Some(vec![fixed(2, 0, tenth), fixed(1, 1, same)]),
        // Fruity Multiband Compressor: low/mid/high thresholds and ratios,
        // then a shared attack/release and the output gain. Maximus keeps our
        // defaults (its bands have modes, not thresholds).
        "fruity multiband compressor" => Some(vec![
            fixed(24, 0, multiband_db),
            fixed(25, 1, multiband_ratio),
            fixed(16, 2, multiband_db),
            fixed(17, 3, multiband_ratio),
            fixed(7, 4, multiband_db),
            fixed(8, 5, multiband_ratio),
            fixed(26, 6, multiband_attack),
            fixed(27, 7, multiband_release),
            fixed(1, 8, multiband_db),
        ]),
        // Vintage Phaser: feedback; its sweep range becomes the depth and its
        // mix the slot mix (below). The LFO rate is not carried over.
        "vintage phaser" => Some(vec![fixed(3, 2, feedback_6000)]),
        "fruity phaser" => Some(vec![
            link(0, 1, 0.0, 5000.0, 0, phaser_rate),
            link(6, 7, 0.0, 1000.0, 2, feedback_1000),
        ]),
        "vintage chorus" => Some(vec![fixed(8, 3, of_6000)]),
        "fruity flanger" => Some(vec![
            link(0, 1, 0.0, 1024.0, 0, flanger_delay),
            link(1, 2, 0.0, 5000.0, 1, of_5000),
            link(2, 3, 0.0, 5000.0, 2, flanger_rate),
            link(10, 11, 0.0, 5000.0, 3, of_5000),
        ]),
        "fruity flangus" => Some(vec![
            link(3, 3, 0.0, 128.0, 0, flangus_delay),
            link(1, 1, 0.0, 1024.0, 1, of_1024),
            link(2, 2, 0.0, 1024.0, 2, flangus_rate),
        ]),
        "fruity soft clipper" => Some(vec![
            link(0, 0, 0.0, 127.0, 0, clip_threshold),
            link(1, 1, 0.0, 160.0, 1, clip_output),
        ]),
        "fruity blood overdrive" => Some(vec![
            link(2, 3, 0.0, 10000.0, 0, blood_drive),
            link(4, 5, 0.0, 10000.0, 1, blood_post_filter),
            link(5, 6, 0.0, 10000.0, 2, gain_10000),
        ]),
        "transient processor" => Some(vec![
            fixed(1, 0, transient_pct),
            fixed(3, 1, transient_pct),
            fixed(4, 2, transient_db),
        ]),
        // The first Fruity Delay: input, feedback, cutoff, tempo, steps, mode.
        "fruity delay" => Some(vec![
            link(4, 5, 0.0, 16.0, 0, same),
            link(1, 2, 0.0, 1024.0, 1, feedback_1024_sq),
            link(5, 6, 0.0, 2.0, 2, ping_pong),
            link(2, 3, 0.0, 1024.0, 3, tone_1024),
            link(0, 1, 0.0, 1024.0, 4, of_1024),
        ]),
        // The first Fruity Reeverb: values 0..65536.
        "fruity reeverb" => Some(vec![
            link(3, 4, 0.0, 65536.0, 0, of_65536),
            link(7, 8, 0.0, 65536.0, 1, of_65536),
            link(0, 1, 0.0, 65536.0, 3, lowcut_65536),
            link(9, 10, 0.0, 65536.0, 4, of_65536),
            link(8, 9, 0.0, 65536.0, 5, of_65536),
        ]),
        // The first Parametric EQ: EQ 2's layout without the version.
        "fruity parametric eq" => Some(
            (0..7)
                .flat_map(|b| {
                    let shelf = b == 0 || b == 6;
                    [
                        link(b as u16, b, -1800.0, 1800.0, 3 * b + 1, eq_gain),
                        link(7 + b as u16, 7 + b, 0.0, 65536.0, 3 * b, eq1_freq),
                        link(
                            14 + b as u16,
                            14 + b,
                            0.0,
                            65536.0,
                            3 * b + 2,
                            if shelf { eq_shelf_q } else { eq_peak_q },
                        ),
                    ]
                })
                .collect(),
        ),
        "spreader" => Some(vec![link(0, 1, 0.0, 10000.0, 0, spread_width)]),
        // Version, then gain, mix, steps, filter on, filter type, resonance,
        // frequency, pre/post, relation, impact, preserve, amount (LYNX's
        // notes; checked against FL 2025 files).
        "fruity squeeze" | "squeeze" => Some(vec![
            link(0, 1, 0.0, 160.0, 6, squeeze_gain),
            link(1, 2, 0.0, 128.0, 5, of_128),
            link(2, 3, 1.0, 32768.0, 0, squeeze_steps),
            link(5, 6, 0.0, 65536.0, 8, of_65536),
            link(6, 7, 0.0, 65536.0, 7, squeeze_freq),
            link(7, 8, 0.0, 1.0, 9, same),
            link(8, 9, 0.0, 3.0, 3, same),
            link(9, 10, 0.0, 44100.0, 2, squeeze_time),
            link(10, 11, 0.0, 44100.0, 1, squeeze_time),
            link(11, 12, 0.0, 65536.0, 4, of_65536),
        ]),
        // Version, then delay, feedback, rate, amount, movement, movement
        // speed, ArachnaChor mode, low pass (Hz), high pass (Hz), dry/wet,
        // phase difference; 0..1000 unless said.
        "hyper chorus" => Some(vec![
            link(0, 1, 0.0, 1000.0, 0, chorus_ms),
            link(1, 2, 0.0, 1000.0, 1, chorus_feedback),
            link(2, 3, 0.0, 1000.0, 3, chorus_hz),
            link(3, 4, 0.0, 1000.0, 2, tenth),
            link(4, 5, 0.0, 1000.0, 4, tenth),
            link(5, 6, 0.0, 1000.0, 5, chorus_hz),
            link(7, 8, 7140.0, 22000.0, 8, same),
            link(8, 9, 120.0, 10900.0, 7, same),
            link(9, 10, 0.0, 1000.0, 10, tenth),
            link(10, 11, 0.0, 1000.0, 9, tenth),
        ]),
        "maximus" | "fruity convolver" | "fruity convolver 2" => Some(vec![]),
        // Kept at our defaults, which are near FL's: a limiter that only
        // catches peaks.
        _ if kind == FxKind::Limiter => Some(vec![]),
        _ => None,
    };
    Some((kind, links))
}

fn value(d: &[u8], at: usize) -> Option<f32> {
    d.get(at * 4..at * 4 + 4)
        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32)
}

/// FL's multiband values are 0..65536 for a linear amplitude (0 dB at the top).
fn multiband_db(r: f32) -> f32 {
    db((r / 65536.0).max(1e-6))
}
fn multiband_ratio(r: f32) -> f32 {
    1.0 + (r / 65536.0) * 9.0
}
fn multiband_attack(r: f32) -> f32 {
    (r / 1000.0 * 100.0).clamp(1.0, 100.0)
}
fn multiband_release(r: f32) -> f32 {
    (r / 10000.0 * 1000.0).clamp(20.0, 1000.0)
}

fn clamp(kind: FxKind, ours: usize, v: f32) -> f32 {
    kind.params().get(ours).map_or(v, |p| v.clamp(p.min, p.max))
}

fn delay_bank(data: &[u8], slot: &mut FxSlot, notes: &mut Vec<String>) -> bool {
    const FLAGS: usize = 536;
    const FLAGS_SIZE: usize = 32;
    if data.len() < FLAGS + 8 * FLAGS_SIZE {
        return false;
    }
    let bank_flags = |i: usize| &data[FLAGS + i * FLAGS_SIZE..FLAGS + (i + 1) * FLAGS_SIZE];
    let mut active = (0..8).filter(|&i| bank_flags(i)[29] != 0);
    let raw = |i| value(data, i).unwrap_or(0.0);
    slot.params[5] = clamp(slot.kind, 5, of_128(raw(1)));
    let Some(bank) = active.next() else {
        slot.params[4] = 0.0;
        return true;
    };
    if active.next().is_some() {
        notes.push(format!(
            "Delay Bank: playing active bank {}; additional banks are retained in the FL state",
            bank + 1
        ));
    }
    let flags = bank_flags(bank);
    if flags[31] == 0 {
        notes.push("Delay Bank: free-running delay time approximated as tempo-synced".into());
    }
    let at = 5 + 16 * bank;
    let feedback = raw(at + 6) / 128.0 * (raw(4) / 128.0);
    let first_echo = if flags[30] == 0 { feedback } else { 1.0 };
    let wet = of_128(raw(2))
        * (raw(3) / 128.0)
        * (raw(at + 1) / 128.0)
        * (raw(at + 14) / 128.0)
        * first_echo;
    for (param, value) in [
        (0, raw(at + 8) / 480.0),
        (1, feedback * 100.0),
        (2, 0.0),
        (3, bank_tone(raw(at + 10))),
        (4, wet),
        (6, offset_steps(raw(at + 9), 6)),
        (7, of_128(raw(at))),
        (8, of_128(raw(at + 7))),
    ] {
        slot.params[param] = clamp(slot.kind, param, value);
    }
    true
}

/// The built-in for an FL effect with its saved settings carried over, and
/// whether they were (the slot can then play; otherwise it stays off).
/// Notes on what did not carry over go to `notes`.
/// Extra slots to put right after it come third (an EQ's high and low pass
/// bands become filters).
pub(crate) fn stand_in(
    fl: &str,
    data: &[u8],
    notes: &mut Vec<String>,
) -> Option<(FxSlot, bool, Vec<FxSlot>)> {
    let (kind, links) = table(fl)?;
    let mut slot = FxSlot::new(kind);
    let mut extra = Vec::new();
    let Some(links) = links else {
        return Some((slot, false, extra));
    };
    for l in &links {
        if let Some(r) = value(data, l.at)
            && let Some(p) = slot.params.get_mut(l.ours)
        {
            *p = clamp(kind, l.ours, (l.map)(r));
        }
    }
    match fl.to_lowercase().as_str() {
        "maximus" => notes.push(
            "Maximus: Multiband at our defaults (its bands have modes, not thresholds)".into(),
        ),
        "vocodex" | "fruity vocodex" => notes.push(
            "Vocodex: Vocoder is off — its carrier/sidechain routing is not imported yet".into(),
        ),
        "gross beat" => notes.push(
            "Gross Beat: Time is off — its step curve is not imported yet".into(),
        ),
        "fruity waveshaper" | "waveshaper" => notes.push(
            "WaveShaper: our Wavefolder is off — its curve is not imported yet".into(),
        ),
        "fruity squeeze" | "squeeze" => {
            // Its filter switched off: ours wide open.
            if value(data, 4) == Some(0.0) {
                slot.params[7] = 20_000.0;
                slot.params[8] = 0.0;
            } else if value(data, 5).is_some_and(|t| t != 0.0) {
                notes.push("Fruity Squeeze: its filter type is not low pass; ours is".into());
            }
        }
        "hyper chorus" => notes.push(
            "Hyper Chorus: settings carried over approximately (delay, feedback and rates on our scales)"
                .into(),
        ),
        "fruity delay bank" => {
            let converted = delay_bank(data, &mut slot, notes);
            return Some((slot, converted, extra));
        }
        "fruity delay 2" => {
            if value(data, 6) == Some(1.0) {
                slot.params[1] = -slot.params[1];
            }
        }
        "fruity panomatic" => notes.push(
            "PanOMatic: pan and volume carried over; its LFO is not imported yet".into(),
        ),
        "fruity stereo shaper" => notes.push(
            "Stereo Shaper: the matrix, delay and dephase are carried; negative delays are clamped to zero"
                .into(),
        ),
        "fruity multiband compressor" => notes.push(
            "Multiband Compressor: thresholds and ratios carried over approximately; the crossover frequencies are fixed at 200 Hz / 2 kHz"
                .into(),
        ),
        "fruity convolver" | "fruity convolver 2" => notes.push(
            "Fruity Convolver: Convolution with the built-in studio impulse; load FL's .wav as the IR"
                .into(),
        ),
        "vintage phaser" => {
            // Sweep range (min, max in the same units) → depth; mix 0..100.
            let (lo, hi) = (value(data, 1).unwrap_or(1.0).max(1.0), value(data, 2).unwrap_or(1.0).max(1.0));
            slot.params[1] = clamp(FxKind::Phaser, 1, (hi.max(lo) / hi.min(lo)).ln() / 20f32.ln() * 100.0 + 20.0);
            slot.mix = (value(data, 22).unwrap_or(100.0) / 100.0).clamp(0.0, 1.0);
            notes.push("Vintage Phaser: feedback, sweep range and mix carried over; its LFO rate is not".into());
        }
        "fruity phaser" => {
            let (lo, hi) = (value(data, 2).unwrap_or(0.0), value(data, 3).unwrap_or(1000.0));
            slot.params[1] = clamp(FxKind::Phaser, 1, (hi - lo).abs() / 1000.0 * 100.0);
            slot.mix = (value(data, 8).unwrap_or(1024.0) / 1024.0).clamp(0.0, 1.0);
        }
        "fruity flangus" => {
            let (dry, wet) = (value(data, 6).unwrap_or(128.0).abs(), value(data, 7).unwrap_or(128.0).abs());
            slot.params[3] = clamp(FxKind::Chorus, 3, wet / (wet + dry).max(1.0) * 100.0);
        }
        "fruity blood overdrive" => {
            // The ×100 switch adds 40 dB of drive.
            if value(data, 4).unwrap_or(0.0) != 0.0 {
                slot.params[0] = clamp(FxKind::Drive, 0, slot.params[0] + 40.0);
            }
        }
        "fruity parametric eq 2" | "fruity parametric eq" => {
            // Band types: 0 off, 1 low pass, 2 band pass, 3 high pass, 4 notch,
            // 5 low shelf, 6 peak, 7 high shelf. Ours: shelf, five peaks, shelf.
            let eq2 = fl.eq_ignore_ascii_case("fruity parametric eq 2");
            let at = if eq2 { 1 } else { 0 };
            let freq = if eq2 { eq_freq } else { eq1_freq };
            for b in 0..7 {
                let t = value(data, 21 + at + b).unwrap_or(0.0) as i32;
                if t == 1 || t == 3 {
                    // A 12 dB/octave filter at the band's frequency; FL's
                    // width sets its Q (2^(5 (1 - 2w)), from DawVert).
                    let mut f = FxSlot::new(FxKind::Filter);
                    let w = value(data, 14 + at + b).unwrap_or(0.0) / 65536.0;
                    let q = 2f32.powf((1.0 - 2.0 * w) * 5.0).max(0.51);
                    f.params[0] = if t == 3 { 2.0 } else { 0.0 };
                    f.params[1] = clamp(
                        FxKind::Filter,
                        1,
                        freq(value(data, 7 + at + b).unwrap_or(0.0)),
                    );
                    f.params[2] = clamp(FxKind::Filter, 2, (2.0 - 1.0 / q) / 1.96 * 100.0);
                    extra.push(f);
                    slot.params[3 * b + 1] = 0.0;
                } else if !(5..=7).contains(&t) {
                    if t != 0 {
                        let what = ["", "low pass", "band pass", "high pass", "notch"]
                            [t.clamp(0, 4) as usize];
                        notes.push(format!("{fl}: band {} is a {what}, left flat", b + 1));
                    }
                    slot.params[3 * b + 1] = 0.0;
                }
            }
        }
        "fruity filter" => {
            // Low, band and high pass are mixed in FL: take the strongest.
            let (lp, bp, hp) = (
                value(data, 3).unwrap_or(0.0),
                value(data, 4).unwrap_or(0.0),
                value(data, 5).unwrap_or(0.0),
            );
            slot.params[0] = if hp > lp && hp >= bp {
                2.0
            } else if bp > lp {
                1.0
            } else {
                0.0
            };
        }
        _ => {}
    }
    Some((slot, true, extra))
}

/// An EQ 2 band frequency automation whose band became a pass filter: which
/// of the extra slots (0 = first after the EQ) and the cutoff of each value.
pub(crate) fn pass_band(
    fl: &str,
    data: &[u8],
    fl_param: u16,
) -> Option<(usize, impl Fn(f32) -> f32)> {
    if !fl.eq_ignore_ascii_case("fruity parametric eq 2") || !(7..14).contains(&fl_param) {
        return None;
    }
    let pass = |b: usize| matches!(value(data, 22 + b).unwrap_or(0.0) as i32, 1 | 3);
    let b = (fl_param - 7) as usize;
    if !pass(b) {
        return None;
    }
    let before = (0..b).filter(|&x| pass(x)).count();
    Some((before, |u: f32| {
        clamp(FxKind::Filter, 1, eq_freq(u * 65536.0))
    }))
}

/// What an FL automation of parameter `fl_param` (0..1 values) drives on the
/// stand-in: our parameter index and the conversion of each value.
pub(crate) fn automation(fl: &str, fl_param: u16) -> Option<(usize, impl Fn(f32) -> f32)> {
    let (kind, links) = table(fl)?;
    let l = links?.into_iter().find(|l| l.fl == Some(fl_param))?;
    let (lo, hi, map, ours) = (l.lo, l.hi, l.map, l.ours);
    Some((ours, move |u: f32| {
        clamp(kind, ours, map(lo + (hi - lo) * u))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(v: &[i32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn parametric_eq_2_settings_carry_over() {
        // From a real project: band 1 shelf at -18 dB, band 7 off.
        let mut v = vec![
            8, -1800, 0, -669, 0, 107, -574, -500, 21163, 18204, 25038, 32768, 34930, 56164, 61077,
        ];
        v.extend([
            40124, 25412, 25412, 25412, 25412, 25412, 40124, 5, 6, 6, 6, 6, 6, 0,
        ]);
        let mut notes = vec![];
        v[22] = 3;
        let (slot, ok, extra) = stand_in("Fruity Parametric EQ 2", &data(&v), &mut notes).unwrap();
        assert!(ok);
        assert_eq!(slot.kind, FxKind::ParamEq);
        // Band 1 as a high pass: flat in the EQ, a filter after it.
        assert_eq!(slot.params[1], 0.0);
        assert_eq!(extra.len(), 1);
        assert_eq!((extra[0].kind, extra[0].params[0]), (FxKind::Filter, 2.0));
        assert!((extra[0].params[1] - eq_freq(21163.0)).abs() < 1e-3);
        assert!((slot.params[7] - -6.69).abs() < 1e-4);
        // 32768 / 65536 = half way on a 20 Hz .. 20 kHz log scale.
        assert!((slot.params[9] - 632.46).abs() < 0.1, "{}", slot.params[9]);
        assert_eq!(slot.params[19], 0.0, "band 7 off: flat");
        assert!(notes.is_empty());
        let (i, f) = automation("Fruity Parametric EQ 2", 3).unwrap();
        assert_eq!(i, 10);
        assert!((f(0.5) - 0.0).abs() < 1e-4 && (f(1.0) - 18.0).abs() < 1e-4);
    }

    #[test]
    fn new_effects_stand_in_for_their_fl_counterparts() {
        let mut notes = vec![];
        // version, mode, amount (DawVert's struct): C at 55%.
        let (bloom, ok, _) = stand_in("Soundgoodizer", &data(&[0, 2, 550]), &mut notes).unwrap();
        assert_eq!(bloom.kind, FxKind::Bloom);
        assert!(ok, "Bloom's profiles and knob match Soundgoodizer");
        assert_eq!((bloom.params[0], bloom.params[1]), (55.0, 2.0));
        let (multi, ok, _) = stand_in("Maximus", &[], &mut notes).unwrap();
        assert_eq!(multi.kind, FxKind::Multiband);
        assert!(ok);
        let (conv, ok, _) = stand_in("Fruity Convolver", &[], &mut notes).unwrap();
        assert_eq!(conv.kind, FxKind::Convolution);
        assert!(ok, "the built-in IR makes it playable");
        let (voc, ok, _) = stand_in("Vocodex", &[], &mut notes).unwrap();
        assert_eq!(voc.kind, FxKind::Vocoder);
        assert!(!ok, "no sidechain routing is carried yet: stays off");
        let (beat, ok, _) = stand_in("Gross Beat", &[], &mut notes).unwrap();
        assert_eq!(beat.kind, FxKind::Time);
        assert!(!ok, "its time curve is not carried yet: stays off");
        assert!(notes.iter().any(|n| n.contains("Maximus")));
        assert!(notes.iter().any(|n| n.contains("Convolver")));
        // Low -6 dB at 5.5:1, mid 0 dB at 1:1, high -12 dB at 10:1.
        let mut v = vec![0i32; 30];
        v[1] = 65536;
        v[7] = 16384;
        v[8] = 65536;
        v[16] = 65536;
        v[17] = 0;
        v[24] = 32768;
        v[25] = 32768;
        v[26] = 500;
        v[27] = 5000;
        let (mb, ok, _) = stand_in("Fruity Multiband Compressor", &data(&v), &mut notes).unwrap();
        assert_eq!(mb.kind, FxKind::Multiband);
        assert!(ok);
        assert!((mb.params[0] - -6.02).abs() < 0.1, "{}", mb.params[0]);
        assert!((mb.params[1] - 5.5).abs() < 1e-4);
        assert!((mb.params[2] - 0.0).abs() < 1e-4);
        assert!((mb.params[3] - 1.0).abs() < 1e-4);
        assert!((mb.params[4] - -12.04).abs() < 0.1);
        assert!((mb.params[5] - 10.0).abs() < 1e-4);
        assert!((mb.params[6] - 50.0).abs() < 1e-4);
        assert!((mb.params[7] - 500.0).abs() < 1e-4);
        assert!((mb.params[8] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn modulation_drive_and_dynamics_from_user_projects() {
        let run = |fl: &str, v: &[i32]| stand_in(fl, &data(v), &mut vec![]).unwrap();
        // Vintage Phaser (evergreen): feedback 3000/6000, mix 50 %.
        let mut vp = vec![
            3, 896070, 1588996, 3000, 0, 125, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 3000, 1, 0, 0, 3200,
            2000, 50,
        ];
        vp.extend([0; 14]);
        let (p, ok, _) = run("Vintage Phaser", &vp);
        assert!(ok && p.kind == FxKind::Phaser);
        assert_eq!((p.params[2], p.mix), (42.5, 0.5));
        // Fruity Phaser (Dear Abonent): rate, 70 % depth, wet half.
        let (p, _, _) = run(
            "Fruity Phaser",
            &[1, 2500, 100, 800, 0, 512, 7, 400, 512, 4000],
        );
        assert!(
            (p.params[1] - 70.0).abs() < 1e-3 && (p.params[2] - 34.0).abs() < 1e-3 && p.mix == 0.5
        );
        // Soft Clipper (Dear Abonent): threshold 100 is about -2 dBFS.
        let (c, ok, _) = run("Fruity Soft Clipper", &[100, 130]);
        assert!(
            ok && c.kind == FxKind::SoftClipper
                && (c.params[0] + 2.08).abs() < 0.01
                && (c.params[1] - 0.352).abs() < 0.001
        );
        // Flangus: equal dry and wet → half mix.
        let (f, _, _) = run("Fruity Flangus", &[4, 512, 768, 8, 128, -32, 96, 96]);
        assert!(f.kind == FxKind::Chorus && f.params[3] == 50.0 && f.params[1] == 50.0);
        // Blood Overdrive: post gain 10000 = 0 dB.
        let (o, _, _) = run(
            "Fruity Blood Overdrive",
            &[3, 0, 5000, 1406, 0, 0, 10000, 0, 0],
        );
        assert!(
            o.kind == FxKind::Drive
                && o.params[2].abs() < 1e-4
                && (o.params[0] - 4.12).abs() < 0.01
                && o.params[1] == 20_000.0
        );
        // Transient Processor: 80 = neutral.
        let (t, ok, _) = run(
            "Transient Processor",
            &[1, 120, 0, 80, 80, 1, 1, 0, 80, 1, 5, 12800],
        );
        assert!(ok && t.kind == FxKind::Transient);
        assert_eq!((t.params[0], t.params[1], t.params[2]), (50.0, 0.0, 0.0));
    }

    #[test]
    fn unknown_layouts_stay_off() {
        let (slot, ok, _) = stand_in("Fruity Love Philter", &[0; 36], &mut vec![]).unwrap();
        assert!(!ok);
        assert_eq!(slot.kind, FxKind::Filter);
        assert!(stand_in("Edison", &[], &mut vec![]).is_none());
    }

    #[test]
    fn squeeze_and_hyper_chorus_carry_their_settings() {
        // From FL 2025 files: a glitchy riser and a chorus on a bass.
        let squeeze = data(&[0, 127, 64, 12144, 1, 0, 0, 57151, 1, 0, 0, 7500, 0]);
        let (s, ok, _) = stand_in("Fruity Squeeze", &squeeze, &mut vec![]).unwrap();
        assert!(ok);
        assert_eq!(s.kind, FxKind::Squeeze);
        assert!((s.params[0] - 90.4).abs() < 0.5, "squarize {}", s.params[0]);
        assert!((s.params[5] - 50.0).abs() < 0.1);
        assert!((s.params[6] - 100.0).abs() < 0.1);
        assert!(
            (s.params[7] - 14_400.0).abs() < 300.0,
            "freq {}",
            s.params[7]
        );
        assert_eq!(s.params[9], 1.0);
        assert_eq!(s.params[2], 0.0, "no impact: no Puncher");
        // Filter off: ours wide open.
        let off = data(&[0, 127, 64, 1, 0, 0, 32768, 32768, 0, 0, 0, 7500, 0]);
        let (s, _, _) = stand_in("Fruity Squeeze", &off, &mut vec![]).unwrap();
        assert_eq!(s.params[7], 20_000.0);
        assert_eq!(s.params[0], 0.0, "step 1 keeps 16 bits");
        let chorus = data(&[2, 586, 477, 443, 456, 507, 711, 0, 22000, 1974, 900, 500, 0]);
        let (h, ok, _) = stand_in("Hyper Chorus", &chorus, &mut vec![]).unwrap();
        assert!(ok);
        assert_eq!(h.kind, FxKind::HyperChorus);
        assert!((h.params[0] - 18.0).abs() < 0.1, "delay {}", h.params[0]);
        assert_eq!(h.params[7], 1974.0, "high pass");
        assert_eq!(h.params[8], 20_000.0, "low pass clamped to ours");
        assert!((h.params[10] - 90.0).abs() < 0.1, "mix");
        assert!((h.params[9] - 50.0).abs() < 0.1, "phase");
    }

    #[test]
    fn squeeze_without_impact_leaves_the_sound_alone() {
        use crate::fx::FxState;
        let mut p = FxKind::Squeeze.defaults();
        p[0] = 0.0;
        p[2] = 0.0;
        p[4] = 0.0;
        p[7] = 20_000.0;
        let input: Vec<f32> = (0..4800)
            .flat_map(|i| {
                let v = (i as f32 * 0.01).sin() * 0.5;
                [v, v]
            })
            .collect();
        let mut buf = input.clone();
        let mut st = FxState::new(FxKind::Squeeze, 48_000.0);
        st.process(&p, &mut buf, 48_000.0, 120.0);
        let zeros = buf
            .iter()
            .zip(&input)
            .filter(|(o, i)| o.abs() < 1e-6 && i.abs() > 0.1)
            .count();
        assert_eq!(zeros, 0, "no punched samples");
    }

    #[test]
    fn delay_bank_uses_the_active_bank_and_its_first_echo_level() {
        let mut v = vec![0i32; 198];
        v[1..5].fill(128);
        let at = 21;
        v[at + 1] = 128;
        v[at + 6] = 64;
        v[at + 7] = -128;
        v[at + 8] = 7680;
        v[at + 10] = 65536;
        v[at + 14] = 128;
        let mut state = data(&v);
        state[536 + 32 + 29] = 1;
        state[536 + 32 + 31] = 1;
        let (d, ok, _) = stand_in("Fruity Delay Bank", &state, &mut vec![]).unwrap();
        assert_eq!(d.kind, FxKind::StereoDelay);
        assert!(ok);
        assert_eq!((d.params[0], d.params[1], d.params[2]), (16.0, 50.0, 0.0));
        assert!((d.params[3] - 20000.0).abs() < 1e-3);
        assert_eq!((d.params[4], d.params[5]), (50.0, 100.0));
        assert_eq!(d.params[8], -100.0);
        let mut buffer = vec![0.0f32; 2 * 2001];
        buffer[0] = 1.0;
        let mut fx = crate::fx::FxState::new(d.kind, 1000.0);
        fx.process(&d.params, &mut buffer, 1000.0, 120.0);
        assert_eq!(&buffer[..2], &[1.0, 0.0]);
        assert_eq!(&buffer[4000..], &[0.375, -0.125]);
    }

    #[test]
    fn balance_panomatic_and_stereo_shaper_carry_their_settings() {
        let mut notes = vec![];
        let (b, ok, _) = stand_in("Fruity Balance", &data(&[-64, 160]), &mut notes).unwrap();
        assert_eq!(b.kind, FxKind::Balance);
        assert!(ok);
        assert_eq!((b.params[0], b.params[1]), (100.0, -50.0));
        let (p, ok, _) = stand_in(
            "Fruity PanOMatic",
            &data(&[192, 160, 0, 0, 0, 0]),
            &mut notes,
        )
        .unwrap();
        assert_eq!(p.kind, FxKind::Balance);
        assert!(ok);
        assert_eq!((p.params[0], p.params[1]), (100.0, 50.0));
        // l2l/r2r full, R->L half, 10 ms delay, 90 degrees.
        let v = [0, 12_800, 25_600, 25_600, 0, 480, 2048, 0];
        let (s, ok, _) = stand_in("Fruity Stereo Shaper", &data(&v), &mut notes).unwrap();
        assert_eq!(s.kind, FxKind::StereoShaper);
        assert!(ok);
        assert_eq!(
            (s.params[0], s.params[1], s.params[2], s.params[3]),
            (100.0, 100.0, 0.0, 50.0)
        );
        assert!((s.params[4] - 10.0).abs() < 1e-4);
        assert!((s.params[5] - 90.0).abs() < 1e-4);
        assert!(notes.iter().any(|n| n.contains("PanOMatic")));
        assert!(notes.iter().any(|n| n.contains("Stereo Shaper")));
    }

    #[test]
    fn compressor_and_delay_scales() {
        let (c, _, _) = stand_in(
            "Fruity Compressor",
            &data(&[2, -160, 58, 44, 1003, 694, 6, 1]),
            &mut vec![],
        )
        .unwrap();
        assert_eq!((c.params[0], c.params[1], c.params[4]), (-16.0, 5.8, 4.4));
        assert!((c.params[2] - 100.0).abs() < 0.5 && (c.params[3] - 694.0).abs() < 1e-3);
        let (d, _, _) = stand_in(
            "Fruity Delay 2",
            &data(&[0, 128, 128, 64, 144, 0, 2, 128]),
            &mut vec![],
        )
        .unwrap();
        assert_eq!(
            (d.params[0], d.params[2], d.params[4], d.params[5]),
            (3.0, 100.0, 100.0, 100.0)
        );
        assert!((d.params[1] - 50.0).abs() < 1e-3);
        // Delay 3: 3 steps, 50% feedback, full stereo, mid tone, 25% wet.
        let mut v = vec![0i32; 27];
        v[5] = 144;
        v[8] = 6000;
        v[15] = 3000;
        v[24] = 1500;
        v[25] = 1500;
        v[26] = 6000;
        let (d3, _, _) = stand_in("Fruity Delay 3", &data(&v), &mut vec![]).unwrap();
        assert_eq!(d3.params[0], 3.0);
        assert!((d3.params[1] - 50.0).abs() < 1e-4);
        assert!((d3.params[2] - 100.0).abs() < 1e-4);
        assert!((d3.params[3] - 500.0 * 40f32.powf(0.5)).abs() < 0.1);
        assert!((d3.params[4] - 25.0).abs() < 1e-4);
        assert!((d3.params[5] - 100.0).abs() < 1e-4);
    }
}
