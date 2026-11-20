//! Built-in drum kits: every piece is a Forge recipe, rendered on the spot,
//! so the app ships no sample files and a project stores only the recipe.
//! Core is 808/909-style drum machine synthesis; Chip is game-console
//! percussion. All of it is our own synthesis (MIT, like the app).

use crate::forge::{Curve, FilterMode, ForgePatch, Layer, Macros, NoiseColor, Source, Wave};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kit {
    Core,
    Chip,
}

/// One sound of a kit.
pub struct Piece {
    /// Stable id (`kick`, `hat_closed`…): what a new project maps by.
    pub id: &'static str,
    pub name: &'static str,
    pub patch: ForgePatch,
}

impl Kit {
    pub const ALL: [Kit; 2] = [Kit::Core, Kit::Chip];

    pub fn label(self) -> &'static str {
        match self {
            Kit::Core => "KFX Core (808/909-style)",
            Kit::Chip => "KFX Chip (game console)",
        }
    }

    pub fn pieces(self) -> Vec<Piece> {
        let list: Vec<(&'static str, &'static str, ForgePatch, f32)> = match self {
            Kit::Core => vec![
                ("kick", "Kick 808", kick_808(), 0.85),
                ("kick_909", "Kick 909", kick_909(), 0.85),
                ("snare", "Snare 909", snare_909(), 0.7),
                ("snare_808", "Snare 808", snare_808(), 0.7),
                ("clap", "Clap", clap(), 0.65),
                ("rim", "Rim", rim(), 0.55),
                ("hat_closed", "Hat Closed", hat(0.07, 0.0), 0.45),
                ("hat_pedal", "Hat Pedal", hat(0.16, -0.08), 0.4),
                ("hat_open", "Hat Open", hat(0.55, -0.04), 0.45),
                ("tom_low", "Tom Low", tom(82.0), 0.7),
                ("tom_mid", "Tom Mid", tom(110.0), 0.7),
                ("tom_high", "Tom High", tom(147.0), 0.7),
                ("cowbell", "Cowbell", cowbell(), 0.5),
                ("clave", "Clave", clave(), 0.5),
                ("shaker", "Shaker", shaker(), 0.4),
                ("crash", "Crash", cymbal(1.9, 0.74, false), 0.5),
                ("ride", "Ride", cymbal(1.6, 0.8, true), 0.45),
            ],
            Kit::Chip => vec![
                ("kick", "Chip Kick", chip_kick(), 0.8),
                ("snare", "Chip Snare", chip_snare(), 0.65),
                ("hat_closed", "Chip Hat", chip_hat(0.05), 0.45),
                ("hat_open", "Chip Open Hat", chip_hat(0.3), 0.45),
                ("tom_low", "Chip Tom", chip_tom(110.0), 0.6),
                ("tom_high", "Chip Tom High", chip_tom(185.0), 0.6),
                ("blip", "Chip Blip", chip_blip(), 0.5),
                ("crash", "Chip Crash", chip_crash(), 0.5),
            ],
        };
        list.into_iter()
            .map(|(id, name, mut patch, peak)| {
                patch.name = name.into();
                level(&mut patch, peak);
                Piece { id, name, patch }
            })
            .collect()
    }

    /// A piece by id.
    pub fn piece(self, id: &str) -> Option<Piece> {
        self.pieces().into_iter().find(|p| p.id == id)
    }
}

/// What a new project's drum channels get (channel name → Core piece).
pub const STARTER: [(&str, &str); 3] =
    [("Kick", "kick"), ("Snare", "snare"), ("Hat", "hat_closed")];

/// Set the patch's gain so it renders with this peak (a kit's balance: the
/// kick loudest, hats lower), rounded so the recipe reads cleanly.
fn level(patch: &mut ForgePatch, peak: f32) {
    let r = crate::forge::render::render(patch);
    if r.peak > 1e-4 {
        let db = 20.0 * (peak / r.peak).log10();
        patch.gain_db = (((patch.gain_db + db) * 10.0).round() / 10.0).clamp(-36.0, 12.0);
    }
}

fn patch(layers: Vec<Layer>) -> ForgePatch {
    ForgePatch {
        layers,
        macros: Macros::default(),
        ..ForgePatch::blank()
    }
}

/// Full at once, falling away; lower `tension` falls faster at first.
fn decay(tension: f32) -> Curve {
    Curve::new(&[(0.0, 1.0, tension), (1.0, 0.0, 0.0)])
}

/// Filter cutoff value for `hz` (Forge maps 0..1 to 20 Hz..20 kHz).
fn cut(hz: f32) -> f32 {
    (hz / 20.0).log(1000.0)
}

/// A pitch drop from `from` semitones to 0 over the first `share` of the layer.
fn drop(from: f32, share: f32) -> Curve {
    Curve::new(&[(0.0, from, -0.75), (share, 0.0, 0.0), (1.0, 0.0, 0.0)])
}

fn noise(name: &str, len: f32) -> Layer {
    Layer::new(name, Source::noise(NoiseColor::White, 8000.0), len)
}

fn kick_808() -> ForgePatch {
    patch(vec![
        Layer::new("Body", Source::osc(Wave::Sine, 49.0), 1.25)
            .with_pitch(drop(22.0, 0.05))
            .with_amp(decay(0.35))
            .with_gain(1.1),
        noise("Click", 0.01)
            .with_amp(decay(-0.5))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(2500.0)), 0.0)
            .with_gain(0.35),
    ])
}

fn kick_909() -> ForgePatch {
    patch(vec![
        Layer::new("Body", Source::osc(Wave::Sine, 54.0), 0.5)
            .with_pitch(drop(30.0, 0.18))
            .with_amp(decay(0.1))
            .with_gain(1.15),
        Layer::new("Edge", Source::osc(Wave::Triangle, 54.0), 0.12)
            .with_pitch(drop(30.0, 0.6))
            .with_amp(decay(-0.4))
            .with_gain(0.3),
        noise("Click", 0.012)
            .with_amp(decay(-0.6))
            .with_filter(FilterMode::LowPass, Curve::flat(cut(6000.0)), 0.1)
            .with_gain(0.45),
    ])
}

fn snare_909() -> ForgePatch {
    patch(vec![
        Layer::new("Tone", Source::osc(Wave::Triangle, 185.0), 0.13)
            .with_pitch(drop(7.0, 0.3))
            .with_amp(decay(-0.4))
            .with_gain(0.75),
        Layer::new("Tone 2", Source::osc(Wave::Sine, 330.0), 0.08)
            .with_pitch(drop(5.0, 0.3))
            .with_amp(decay(-0.5))
            .with_gain(0.35),
        noise("Snappy", 0.26)
            .with_amp(decay(-0.45))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(1800.0)), 0.05)
            .with_gain(0.7),
        noise("Body noise", 0.11)
            .with_amp(decay(-0.5))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(900.0)), 0.2)
            .with_gain(0.5),
    ])
}

fn snare_808() -> ForgePatch {
    patch(vec![
        Layer::new("Low", Source::osc(Wave::Sine, 180.0), 0.2)
            .with_pitch(drop(3.0, 0.2))
            .with_amp(decay(-0.3))
            .with_gain(0.8),
        Layer::new("High", Source::osc(Wave::Sine, 332.0), 0.12)
            .with_amp(decay(-0.4))
            .with_gain(0.45),
        noise("Snappy", 0.2)
            .with_amp(decay(-0.55))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(3500.0)), 0.0)
            .with_gain(0.55),
    ])
}

fn clap() -> ForgePatch {
    let burst = |start: f32| {
        noise("Burst", 0.012)
            .with_start(start)
            .with_amp(decay(-0.3))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(1300.0)), 0.25)
            .with_gain(2.0)
    };
    patch(vec![
        burst(0.0),
        burst(0.011),
        burst(0.022),
        noise("Tail", 0.32)
            .with_start(0.03)
            .with_amp(decay(-0.55))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(1200.0)), 0.2)
            .with_gain(2.0),
    ])
}

fn rim() -> ForgePatch {
    patch(vec![
        Layer::new("Ping", Source::osc(Wave::Triangle, 820.0), 0.03)
            .with_amp(decay(-0.4))
            .with_gain(0.8),
        Layer::new("Wood", Source::osc(Wave::Square, 1700.0), 0.012)
            .with_amp(decay(-0.4))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(2500.0)), 0.3)
            .with_gain(0.3),
        noise("Snap", 0.02)
            .with_amp(decay(-0.6))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(3000.0)), 0.2)
            .with_gain(0.4),
    ])
}

/// The six detuned squares of the 808 metal voices.
const METAL: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

/// A hi-hat: the metal through a high-pass, plus a little noise; `len` sets
/// closed/pedal/open, `bend` a slight pitch sag on the longer ones.
fn hat(len: f32, bend: f32) -> ForgePatch {
    let mut layers: Vec<Layer> = METAL[..5]
        .iter()
        .map(|&hz| {
            Layer::new("Metal", Source::osc(Wave::Square, hz), len)
                .with_pitch(Curve::new(&[(0.0, 0.0, 0.0), (1.0, bend, 0.0)]))
                .with_amp(decay(-0.5))
                .with_filter(FilterMode::HighPass, Curve::flat(cut(7500.0)), 0.25)
                .with_gain(0.32)
        })
        .collect();
    layers.push(
        noise("Air", len * 0.9)
            .with_amp(decay(-0.55))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(9000.0)), 0.0)
            .with_gain(0.45),
    );
    patch(layers)
}

fn tom(hz: f32) -> ForgePatch {
    patch(vec![
        Layer::new("Head", Source::osc(Wave::Sine, hz), 0.45)
            .with_pitch(Curve::new(&[
                (0.0, 12.0, -0.7),
                (0.3, 1.5, 0.0),
                (1.0, 0.0, 0.0),
            ]))
            .with_amp(decay(-0.25))
            .with_gain(1.0),
        noise("Skin", 0.05)
            .with_amp(decay(-0.5))
            .with_filter(FilterMode::LowPass, Curve::flat(cut(3500.0)), 0.0)
            .with_gain(0.25),
    ])
}

fn cowbell() -> ForgePatch {
    let tone = |hz: f32| {
        Layer::new("Bell", Source::osc(Wave::Square, hz), 0.3)
            .with_amp(Curve::new(&[
                (0.0, 1.0, -0.6),
                (0.15, 0.45, -0.3),
                (1.0, 0.0, 0.0),
            ]))
            .with_filter(FilterMode::BandPass, Curve::flat(cut(2600.0)), 0.35)
            .with_gain(0.6)
    };
    patch(vec![tone(540.0), tone(800.0)])
}

fn clave() -> ForgePatch {
    patch(vec![
        Layer::new("Wood", Source::osc(Wave::Sine, 2500.0), 0.04)
            .with_amp(decay(-0.3))
            .with_gain(0.85),
    ])
}

fn shaker() -> ForgePatch {
    patch(vec![
        noise("Beads", 0.12)
            .with_amp(Curve::new(&[
                (0.0, 0.2, 0.4),
                (0.35, 1.0, -0.5),
                (1.0, 0.0, 0.0),
            ]))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(5500.0)), 0.1)
            .with_gain(0.8),
    ])
}

/// Crash or ride: the metal opened lower and long; a ride adds its ping.
fn cymbal(len: f32, hp: f32, ping: bool) -> ForgePatch {
    let mut layers: Vec<Layer> = METAL[..4]
        .iter()
        .map(|&hz| {
            Layer::new("Metal", Source::osc(Wave::Square, hz * 1.5), len)
                .with_amp(decay(-0.45))
                .with_filter(FilterMode::HighPass, Curve::flat(hp), 0.2)
                .with_gain(0.28)
        })
        .collect();
    layers.push(
        noise("Wash", len)
            .with_amp(decay(-0.5))
            .with_filter(
                FilterMode::HighPass,
                Curve::new(&[(0.0, cut(4000.0), 0.0), (1.0, cut(7000.0), 0.0)]),
                0.0,
            )
            .with_gain(if ping { 0.3 } else { 0.55 }),
    );
    if ping {
        layers.push(
            Layer::new("Ping", Source::osc(Wave::Triangle, 3150.0), 0.6)
                .with_amp(decay(-0.35))
                .with_gain(0.3),
        );
    }
    patch(layers)
}

fn chip_kick() -> ForgePatch {
    patch(vec![
        Layer::new("Drop", Source::osc(Wave::Triangle, 55.0), 0.16)
            .with_pitch(Curve::new(&[(0.0, 36.0, -0.6), (1.0, 0.0, 0.0)]))
            .with_amp(Curve::new(&[
                (0.0, 1.0, 0.0),
                (0.7, 0.8, 0.0),
                (1.0, 0.0, 0.0),
            ])),
    ])
}

fn chip_snare() -> ForgePatch {
    patch(vec![
        Layer::new("Noise", Source::noise(NoiseColor::Digital, 9000.0), 0.16)
            .with_amp(decay(-0.3))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(400.0)), 0.0)
            .with_gain(0.7),
        Layer::new("Tone", Source::osc(Wave::Triangle, 220.0), 0.07)
            .with_pitch(Curve::new(&[(0.0, 12.0, -0.5), (1.0, 0.0, 0.0)]))
            .with_amp(decay(0.0))
            .with_gain(0.6),
    ])
}

fn chip_hat(len: f32) -> ForgePatch {
    patch(vec![
        Layer::new("Noise", Source::noise(NoiseColor::Digital, 30000.0), len)
            .with_amp(decay(-0.4))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(5000.0)), 0.0)
            .with_gain(0.6),
    ])
}

fn chip_tom(hz: f32) -> ForgePatch {
    patch(vec![
        Layer::new("Tom", Source::osc(Wave::Pulse25, hz), 0.22)
            .with_pitch(Curve::new(&[(0.0, 7.0, -0.4), (1.0, -5.0, 0.0)]))
            .with_amp(decay(-0.2))
            // A narrow pulse has a DC offset: the console's output capacitor took it out.
            .with_filter(FilterMode::HighPass, Curve::flat(cut(60.0)), 0.0)
            .with_gain(0.55),
    ])
}

fn chip_blip() -> ForgePatch {
    patch(vec![
        Layer::new("Blip", Source::osc(Wave::Pulse12, 1046.5), 0.06)
            .with_pitch(Curve::new(&[
                (0.0, 12.0, 0.0),
                (0.4, 12.0, 0.0),
                (0.41, 0.0, 0.0),
            ]))
            .with_amp(Curve::new(&[(0.0, 0.8, 0.0), (1.0, 0.0, 0.0)]))
            // A narrow pulse has a DC offset: the console's output capacitor took it out.
            .with_filter(FilterMode::HighPass, Curve::flat(cut(60.0)), 0.0),
    ])
}

fn chip_crash() -> ForgePatch {
    patch(vec![
        Layer::new("Wash", Source::noise(NoiseColor::Digital, 14000.0), 1.1)
            .with_pitch(Curve::new(&[(0.0, 0.0, 0.0), (1.0, -12.0, 0.0)]))
            .with_amp(decay(-0.45))
            .with_filter(FilterMode::HighPass, Curve::flat(cut(400.0)), 0.0)
            .with_gain(0.6),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::render::render;

    fn rms(f: &[[f32; 2]]) -> f32 {
        (f.iter().map(|s| s[0] * s[0]).sum::<f32>() / f.len().max(1) as f32).sqrt()
    }

    #[test]
    fn every_piece_sounds_without_clipping_and_ends_clean() {
        for kit in Kit::ALL {
            let pieces = kit.pieces();
            let mut ids = std::collections::HashSet::new();
            for p in &pieces {
                assert!(ids.insert(p.id), "{kit:?}: duplicate id {}", p.id);
                let mut patch = p.patch.clone();
                patch.tidy();
                assert_eq!(patch, p.patch, "{}: a recipe stays as written", p.name);
                let r = render(&p.patch);
                assert!(r.peak > 0.35, "{}: peak {}", p.name, r.peak);
                assert!(r.peak <= 0.9, "{}: peak {}", p.name, r.peak);
                let last = r.frames.last().unwrap();
                assert!(last[0].abs() < 1e-3, "{}: ends with a click", p.name);
                let mean = r.frames.iter().map(|f| f[0]).sum::<f32>() / r.frames.len() as f32;
                assert!(mean.abs() < 0.02, "{}: DC {mean}", p.name);
            }
        }
    }

    #[test]
    fn core_has_the_pieces_a_project_maps_and_long_tails() {
        for (_, id) in STARTER {
            assert!(Kit::Core.piece(id).is_some(), "{id}");
        }
        let secs = |id: &str| {
            let r = render(&Kit::Core.piece(id).unwrap().patch);
            r.frames.len() as f32 / r.rate as f32
        };
        assert!(secs("kick") > 1.0, "808 kick {}", secs("kick"));
        assert!(secs("crash") > 1.2);
        assert!(secs("hat_closed") < 0.15);
        assert!(secs("hat_open") > 3.0 * secs("hat_closed"));
    }

    #[test]
    fn a_kick_is_low_and_a_hat_is_high() {
        // Zero crossings per second: a rough brightness.
        let zc = |id: &str| {
            let r = render(&Kit::Core.piece(id).unwrap().patch);
            let n = r.frames.len().min(r.rate as usize / 10);
            let f = &r.frames[..n];
            let c = f
                .windows(2)
                .filter(|w| (w[0][0] >= 0.0) != (w[1][0] >= 0.0))
                .count();
            (c as f32 / 2.0) / (n as f32 / r.rate as f32)
        };
        assert!(zc("kick") < 150.0, "kick {}", zc("kick"));
        assert!(zc("hat_closed") > 4000.0, "hat {}", zc("hat_closed"));
        let snare = render(&Kit::Core.piece("snare").unwrap().patch);
        assert!(rms(&snare.frames) > 0.03);
    }
}
