//! FL Studio's own generators as KeyFrame Tide stand-ins, for when FL Studio
//! VSTi is not there to play them (without one the channel would be silent).
//! The FL state stays with the channel, so FL Studio VSTi still takes over
//! when it is installed.

use crate::synth::{Oscillator, SynthPatch, Table};

/// A Tide patch standing in for FL generator `plugin`, and a line for the
/// import report. `None`: no stand-in for that generator.
pub(crate) fn stand_in(plugin: &str, data: &[u8]) -> Option<(SynthPatch, String)> {
    match plugin.to_lowercase().as_str() {
        "3x osc" => three_osc(data).map(|p| {
            (
                p,
                "3x Osc → KeyFrame Tide with its three oscillators (the channel's own filter and envelopes are not read yet)"
                    .into(),
            )
        }),
        "flex" => Some(flex(data)),
        _ => None,
    }
}

fn int(d: &[u8], at: usize) -> Option<i32> {
    d.get(at * 4..at * 4 + 4)
        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// FL 3x Osc. Per oscillator: mix level (the first one's slot holds the
/// version), pan, shape, coarse, fine, stereo offset, detune; DawVert's
/// `3x_osc` struct, checked against FL 2025 files. Shapes: 0 sine,
/// 1 triangle, 2 square, 3 saw, 4 rounded saw, 5 noise, 6 custom.
fn three_osc(d: &[u8]) -> Option<SynthPatch> {
    if d.len() < 92 {
        return None;
    }
    let osc = |n: usize| {
        let at = 7 * n;
        let v = |k| int(d, at + k).unwrap_or(0);
        Osc {
            mix: if n == 0 { 0.0 } else { v(0) as f64 / 128.0 },
            shape: v(2),
            coarse: v(3) as f64,
            // Fine is in cents; detune is a smaller fine step on top.
            fine: v(4) as f64 + v(6) as f64 * 0.5,
        }
    };
    let (a, b, c) = (osc(0), osc(1), osc(2));
    // FL's mix law: osc 3 takes its share, osc 2 a share of the rest.
    let g3 = c.mix.clamp(0.0, 1.0);
    let g2 = (1.0 - g3) * b.mix.clamp(0.0, 1.0);
    let g1 = (1.0 - g3) - g2;
    let mut p = SynthPatch {
        level: 0.0,
        ..SynthPatch::default()
    };
    for (o, gain, slot) in [(a, g1, 0), (b, g2, 1), (c, g3, 2)] {
        if gain < 1e-3 {
            continue;
        }
        if o.shape == 5 {
            p.noise = (p.noise + gain).min(1.0);
            continue;
        }
        let (table, position) = table_of(o.shape);
        let fine = o.fine.clamp(-100.0, 100.0);
        let coarse = o.coarse.clamp(-48.0, 48.0);
        match slot {
            0 => {
                p.table = table;
                p.position = position;
                p.level = gain;
                p.coarse = coarse;
                p.fine = fine;
            }
            1 => {
                p.osc_b = Oscillator {
                    table,
                    position,
                    level: gain,
                    coarse,
                    fine,
                    ..Oscillator::default()
                };
            }
            _ => {
                // The third oscillator is Tide's additive one: the same
                // shapes as harmonics (a triangle is odd harmonics, dark).
                p.spectral.level = gain;
                p.spectral.coarse = coarse;
                let (partials, tilt, odd) = match o.shape {
                    0 => (1, 0.0, 0.0),
                    1 => (16, -1.0, 1.0),
                    2 => (32, 0.0, 1.0),
                    4 => (16, -0.5, 0.0),
                    _ => (32, 0.0, 0.0),
                };
                p.spectral.partials = partials;
                p.spectral.tilt = tilt;
                p.spectral.odd = odd;
            }
        }
    }
    // The oscillators are summed at FL's level, which runs hot next to Tide's.
    p.level *= 0.8;
    p.osc_b.level *= 0.8;
    p.spectral.level *= 0.8;
    p.noise *= 0.5;
    Some(p)
}

struct Osc {
    mix: f64,
    shape: i32,
    coarse: f64,
    fine: f64,
}

/// A 3x Osc shape on Tide's wavetables: sine-to-saw and square.
fn table_of(shape: i32) -> (Table, f64) {
    match shape {
        0 => (Table::SineSaw, 0.0),
        1 => (Table::SineSaw, 0.15),
        2 => (Table::SquarePulse, 0.0),
        4 => (Table::SineSaw, 0.6),
        // Saw, and the custom wave as one.
        _ => (Table::SineSaw, 1.0),
    }
}

/// FLEX: its sound is in an encrypted pack, so a Tide factory preset of the
/// same kind plays instead, chosen from the pack and preset names.
fn flex(d: &[u8]) -> (SynthPatch, String) {
    let text = |at: usize| {
        d.get(at..at + 1024)
            .map(|b| {
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                String::from_utf8_lossy(&b[..end]).trim().to_string()
            })
            .unwrap_or_default()
    };
    let (pack, preset) = (text(0x10), text(0x411));
    let category = flex_category(&format!("{pack} {preset}"));
    let index = SynthPatch::CATEGORIES
        .iter()
        .position(|c| *c == category)
        .unwrap_or(0);
    let name = match (pack.is_empty(), preset.is_empty()) {
        (false, false) => format!("{pack} / {preset}"),
        (false, true) => pack.clone(),
        _ => "FLEX".into(),
    };
    (
        SynthPatch::factory(index),
        format!(
            "FLEX ({name}) → Tide \"{}\" ({category}): FLEX's own sound needs FL Studio",
            SynthPatch::PRESETS[index]
        ),
    )
}

/// The Tide category a FLEX pack or preset name points at.
fn flex_category(name: &str) -> &'static str {
    let n = format!(" {} ", name.to_lowercase());
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&[" bas ", "bass", " 808", " sub "]) {
        "Bass"
    } else if has(&[" plk ", "pluck", "harp", "guitar"]) {
        "Pluck"
    } else if has(&[
        " pad ", "pad ", "atmo", "sphere", "cloud", "string", "choir",
    ]) {
        "Pad"
    } else if has(&[" led ", "lead"]) {
        "Lead"
    } else if has(&[" key ", " keys ", "piano", "rhodes", "organ"]) {
        "Keys"
    } else if has(&["bell", "sparkle", "mallet", "chime"]) {
        "Bell"
    } else if has(&[" arp"]) {
        "Arp"
    } else if has(&["sfx", " fx ", "riser", "crash", "impact", "sweep"]) {
        "FX"
    } else {
        "Keys"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(v: &[i32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn three_osc_carries_shapes_tuning_and_the_mix_law() {
        // Saw at −24, square at −12 (mix 44, detune −10), sine third (mix 32).
        let mut v = vec![0; 23];
        v[..21].copy_from_slice(&[
            14, 0, 3, -24, 0, 0, 0, 44, 0, 2, -12, 0, 0, -10, 32, 0, 0, 0, 0, 0, 0,
        ]);
        let p = three_osc(&data(&v)).unwrap();
        assert_eq!(p.table, Table::SineSaw);
        assert_eq!(p.position, 1.0);
        assert_eq!(p.coarse, -24.0);
        assert_eq!(p.osc_b.table, Table::SquarePulse);
        assert_eq!(p.osc_b.coarse, -12.0);
        assert_eq!(p.osc_b.fine, -5.0);
        assert_eq!(p.spectral.partials, 1, "a sine third oscillator");
        // g3 = 0.25, g2 = 0.75 · 44/128, g1 the rest; then 0.8 of each.
        let g3 = 32.0 / 128.0;
        let g2 = (1.0 - g3) * 44.0 / 128.0;
        assert!((p.spectral.level - g3 * 0.8).abs() < 1e-9);
        assert!((p.osc_b.level - g2 * 0.8).abs() < 1e-9);
        assert!((p.level - (1.0 - g3 - g2) * 0.8).abs() < 1e-9);
        assert!(p.validate().is_ok());
        assert!(three_osc(&[0; 40]).is_none());
    }

    #[test]
    fn three_osc_noise_and_default_render() {
        // FL's default patch with oscillator 1 on noise (a riser).
        let mut v = vec![0; 23];
        v[..21].copy_from_slice(&[
            14, 0, 5, 0, 0, 0, 0, 0, 0, 0, -12, 0, 0, 0, 0, 0, 0, -24, 0, 0, 0,
        ]);
        let p = three_osc(&data(&v)).unwrap();
        assert!(p.noise > 0.0);
        assert_eq!(p.level, 0.0);
        assert_eq!(p.osc_b.level, 0.0);
    }

    #[test]
    fn flex_picks_a_preset_by_its_name() {
        let mut d = vec![0u8; 2707];
        let put =
            |d: &mut Vec<u8>, at: usize, s: &str| d[at..at + s.len()].copy_from_slice(s.as_bytes());
        put(&mut d, 0x10, "Melodic Techno");
        put(&mut d, 0x411, "BAS Lottunma");
        let (p, note) = flex(&d);
        let i = SynthPatch::CATEGORIES
            .iter()
            .position(|c| *c == "Bass")
            .unwrap();
        assert_eq!(p, SynthPatch::factory(i));
        assert!(note.contains("Melodic Techno / BAS Lottunma"), "{note}");
        for (name, cat) in [
            ("Mobile Synth Pluck Synpluck Blip", "Pluck"),
            ("Melodic Techno PAD Minoo", "Pad"),
            ("Melodic Techno LED Kian", "Lead"),
            ("Melodic Techno KEY Eligash", "Keys"),
            ("Arksun Cityscape Fairy Sparkle", "Bell"),
            ("Melodic Techno SFX Crasha", "FX"),
            ("Mobile Tuned 808 Bass 808 Flattened", "Bass"),
            ("Olbaid Compendium Scatter Clouds", "Pad"),
            ("Synthesizer Science", "Keys"),
        ] {
            assert_eq!(flex_category(name), cat, "{name}");
        }
    }

    /// Every 3x Osc in the shapes FL offers, as a chord: audible, no clipping.
    #[test]
    fn three_osc_stand_ins_render_clean() {
        use crate::sequence::{BAR, InstrumentKind, Note, Song};
        for shape in 0..7 {
            let mut v = vec![0; 23];
            v[..21].copy_from_slice(&[
                14, 0, shape, 0, 0, 0, 0, 64, 0, shape, -12, 0, 0, 5, 64, 0, shape, -24, 0, 0, 0,
            ]);
            let mut song = Song::default();
            song.instruments.clear();
            let id = song.add_channel(
                "3x Osc",
                InstrumentKind::Synth {
                    patch: three_osc(&data(&v)).unwrap(),
                },
            );
            std::sync::Arc::make_mut(&mut song.patterns[0]).length = BAR;
            for k in [48u8, 55, 60, 64] {
                song.add_note(1, Note::new(id, 0, BAR / 2, k, 100));
            }
            let f = crate::engine::render(&song, 1, false, 48_000);
            let peak = f
                .iter()
                .map(|s| s[0].abs().max(s[1].abs()))
                .fold(0.0, f32::max);
            assert!(peak > 0.05 && peak < 0.98, "shape {shape}: peak {peak}");
        }
    }
}
