//! Starting points: a recipe is a ready sound to bend into your own, and a
//! style turns any of them toward the family a game's sounds belong to.

use super::{Curve, FilterMode, ForgePatch, Layer, MAX_LAYERS, Macros, NoiseColor, Source, Wave};
use crate::fx::{FxKind, FxSlot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recipe {
    Jump,
    Coin,
    Shot,
    Laser,
    Hit,
    Explosion,
    Step,
    UiClick,
    Magic,
    Portal,
    Hum,
}

/// A built-in effect at its defaults with some parameters set.
fn fx(kind: FxKind, set: &[(usize, f32)]) -> FxSlot {
    let mut s = FxSlot::new(kind);
    for &(i, v) in set {
        s.params[i] = v;
    }
    s
}

/// Volume that starts at full and falls away, faster with lower `tension`.
fn decay(tension: f32) -> Curve {
    Curve::new(&[(0.0, 1.0, tension), (1.0, 0.0, 0.0)])
}

fn patch(name: &str, layers: Vec<Layer>, fx: Vec<FxSlot>, macros: Macros) -> ForgePatch {
    ForgePatch {
        name: name.into(),
        layers,
        fx,
        macros,
        ..ForgePatch::blank()
    }
}

fn knobs(size: f32, energy: f32, dirt: f32, space: f32) -> Macros {
    Macros {
        size,
        energy,
        dirt,
        space,
    }
}

impl Recipe {
    pub const ALL: [Recipe; 11] = [
        Recipe::Jump,
        Recipe::Coin,
        Recipe::Shot,
        Recipe::Laser,
        Recipe::Hit,
        Recipe::Explosion,
        Recipe::Step,
        Recipe::UiClick,
        Recipe::Magic,
        Recipe::Portal,
        Recipe::Hum,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Recipe::Jump => "Jump",
            Recipe::Coin => "Coin",
            Recipe::Shot => "Shot",
            Recipe::Laser => "Laser",
            Recipe::Hit => "Hit",
            Recipe::Explosion => "Explosion",
            Recipe::Step => "Step",
            Recipe::UiClick => "UI click",
            Recipe::Magic => "Magic",
            Recipe::Portal => "Portal",
            Recipe::Hum => "Hum",
        }
    }

    /// File-name stem for exports ("jump_01.wav").
    pub fn stem(self) -> &'static str {
        match self {
            Recipe::Jump => "jump",
            Recipe::Coin => "coin",
            Recipe::Shot => "shot",
            Recipe::Laser => "laser",
            Recipe::Hit => "hit",
            Recipe::Explosion => "explosion",
            Recipe::Step => "step",
            Recipe::UiClick => "ui_click",
            Recipe::Magic => "magic",
            Recipe::Portal => "portal",
            Recipe::Hum => "hum",
        }
    }

    pub fn patch(self) -> ForgePatch {
        match self {
            Recipe::Jump => patch(
                "Jump",
                vec![
                    Layer::new("Rise", Source::osc(Wave::Pulse25, 330.0), 0.26)
                        .with_pitch(Curve::new(&[
                            (0.0, 0.0, -0.5),
                            (0.7, 12.0, 0.0),
                            (1.0, 14.0, 0.0),
                        ]))
                        .with_amp(Curve::new(&[
                            (0.0, 0.9, 0.0),
                            (0.55, 0.8, 0.3),
                            (1.0, 0.0, 0.0),
                        ])),
                    Layer::new("Body", Source::osc(Wave::Triangle, 165.0), 0.2)
                        .with_pitch(Curve::new(&[(0.0, 0.0, -0.5), (1.0, 12.0, 0.0)]))
                        .with_amp(decay(-0.2))
                        .with_gain(0.5),
                ],
                vec![],
                knobs(0.5, 0.5, 0.0, 0.0),
            ),
            Recipe::Coin => patch(
                "Coin",
                vec![
                    Layer::new("Ding", Source::osc(Wave::Pulse25, 988.0), 0.08)
                        .with_amp(Curve::flat(0.85)),
                    Layer::new("Dong", Source::osc(Wave::Pulse25, 1319.0), 0.42)
                        .with_start(0.075)
                        .with_amp(Curve::new(&[
                            (0.0, 0.85, 0.0),
                            (0.25, 0.7, -0.4),
                            (1.0, 0.0, 0.0),
                        ])),
                ],
                vec![],
                knobs(0.5, 0.5, 0.0, 0.08),
            ),
            Recipe::Shot => patch(
                "Shot",
                vec![
                    Layer::new("Blast", Source::noise(NoiseColor::White, 4000.0), 0.28)
                        .with_amp(decay(-0.7))
                        .with_filter(
                            FilterMode::LowPass,
                            Curve::new(&[(0.0, 0.92, -0.4), (1.0, 0.45, 0.0)]),
                            0.15,
                        ),
                    Layer::new("Thump", Source::osc(Wave::Sine, 110.0), 0.16)
                        .with_pitch(Curve::new(&[(0.0, 14.0, -0.7), (1.0, -10.0, 0.0)]))
                        .with_amp(decay(-0.3))
                        .with_gain(1.2),
                    Layer::new("Tail", Source::noise(NoiseColor::Crackle, 900.0), 0.45)
                        .with_start(0.02)
                        .with_amp(decay(-0.5))
                        .with_gain(0.35),
                ],
                vec![],
                knobs(0.5, 0.6, 0.25, 0.15),
            ),
            Recipe::Laser => patch(
                "Laser",
                vec![
                    Layer::new("Zap", Source::osc(Wave::Saw, 1500.0), 0.3)
                        .with_pitch(Curve::new(&[(0.0, 0.0, -0.55), (1.0, -30.0, 0.0)]))
                        .with_amp(Curve::new(&[(0.0, 1.0, 0.2), (1.0, 0.0, 0.0)]))
                        .with_filter(FilterMode::LowPass, Curve::flat(0.8), 0.3),
                    Layer::new(
                        "Buzz",
                        Source::Osc {
                            wave: Wave::Square,
                            hz: 750.0,
                            vibrato_hz: 28.0,
                            vibrato: 0.6,
                        },
                        0.26,
                    )
                    .with_pitch(Curve::new(&[(0.0, 0.0, -0.55), (1.0, -30.0, 0.0)]))
                    .with_amp(decay(0.0))
                    .with_gain(0.45)
                    .with_pan(0.3),
                ],
                vec![fx(FxKind::Delay, &[(0, 1.0), (1, 25.0), (4, 18.0)])],
                knobs(0.5, 0.5, 0.0, 0.0),
            ),
            Recipe::Hit => patch(
                "Hit",
                vec![
                    Layer::new("Punch", Source::osc(Wave::Sine, 85.0), 0.24)
                        .with_pitch(Curve::new(&[(0.0, 20.0, -0.8), (1.0, 0.0, 0.0)]))
                        .with_amp(decay(-0.25))
                        .with_gain(1.3),
                    Layer::new("Snap", Source::noise(NoiseColor::White, 4000.0), 0.035)
                        .with_amp(decay(-0.4))
                        .with_filter(FilterMode::HighPass, Curve::flat(0.62), 0.1)
                        .with_gain(0.8),
                    Layer::new("Body", Source::noise(NoiseColor::Pink, 4000.0), 0.13)
                        .with_amp(decay(-0.5))
                        .with_filter(
                            FilterMode::LowPass,
                            Curve::new(&[(0.0, 0.62, 0.0), (1.0, 0.38, 0.0)]),
                            0.2,
                        ),
                ],
                vec![fx(FxKind::Transient, &[])],
                knobs(0.5, 0.55, 0.15, 0.05),
            ),
            Recipe::Explosion => patch(
                "Explosion",
                vec![
                    Layer::new("Blast", Source::noise(NoiseColor::White, 4000.0), 0.3)
                        .with_amp(decay(-0.6))
                        .with_filter(
                            FilterMode::LowPass,
                            Curve::new(&[(0.0, 0.85, 0.0), (1.0, 0.5, 0.0)]),
                            0.0,
                        ),
                    Layer::new("Rumble", Source::noise(NoiseColor::Brown, 4000.0), 1.7)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.02, 1.0, -0.35),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(
                            FilterMode::LowPass,
                            Curve::new(&[(0.0, 0.72, -0.3), (1.0, 0.3, 0.0)]),
                            0.2,
                        )
                        .with_gain(1.4),
                    Layer::new("Sub", Source::osc(Wave::Sine, 55.0), 0.9)
                        .with_pitch(Curve::new(&[(0.0, 12.0, -0.5), (1.0, -12.0, 0.0)]))
                        .with_amp(decay(-0.2))
                        .with_gain(1.1),
                    Layer::new("Debris", Source::noise(NoiseColor::Crackle, 350.0), 1.3)
                        .with_start(0.06)
                        .with_amp(Curve::new(&[(0.0, 0.8, -0.2), (1.0, 0.0, 0.0)]))
                        .with_filter(FilterMode::HighPass, Curve::flat(0.5), 0.0)
                        .with_gain(0.5),
                ],
                vec![],
                knobs(0.6, 0.55, 0.35, 0.35),
            ),
            Recipe::Step => patch(
                "Step",
                vec![
                    Layer::new("Scuff", Source::noise(NoiseColor::Pink, 4000.0), 0.09)
                        .with_amp(decay(-0.6))
                        .with_filter(FilterMode::BandPass, Curve::flat(0.5), 0.3)
                        .with_gain(1.4),
                    Layer::new("Thud", Source::osc(Wave::Sine, 70.0), 0.08)
                        .with_pitch(Curve::new(&[(0.0, 5.0, -0.5), (1.0, 0.0, 0.0)]))
                        .with_amp(decay(-0.3))
                        .with_gain(0.7),
                    Layer::new("Grit", Source::noise(NoiseColor::Crackle, 2500.0), 0.06)
                        .with_amp(decay(-0.3))
                        .with_gain(0.3),
                ],
                vec![],
                knobs(0.5, 0.5, 0.0, 0.05),
            ),
            Recipe::UiClick => patch(
                "UI click",
                vec![
                    Layer::new("Tick", Source::osc(Wave::Sine, 2000.0), 0.035)
                        .with_amp(decay(-0.6)),
                    Layer::new("Edge", Source::noise(NoiseColor::White, 4000.0), 0.012)
                        .with_amp(decay(-0.3))
                        .with_filter(FilterMode::HighPass, Curve::flat(0.78), 0.0)
                        .with_gain(0.4),
                ],
                vec![],
                knobs(0.5, 0.5, 0.0, 0.0),
            ),
            Recipe::Magic => patch(
                "Magic",
                vec![
                    Layer::new("Sparkle", Source::osc(Wave::Triangle, 880.0), 0.7)
                        .with_pitch(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.14, 0.0, 0.0),
                            (0.14, 4.0, 0.0),
                            (0.28, 4.0, 0.0),
                            (0.28, 7.0, 0.0),
                            (0.42, 7.0, 0.0),
                            (0.42, 12.0, 0.0),
                            (0.56, 12.0, 0.0),
                            (0.56, 16.0, 0.0),
                            (0.70, 16.0, 0.0),
                            (0.70, 19.0, 0.0),
                            (0.84, 19.0, 0.0),
                            (0.84, 24.0, 0.0),
                            (1.0, 24.0, 0.0),
                        ]))
                        .with_amp(Curve::new(&[
                            (0.0, 0.8, 0.0),
                            (0.7, 0.6, 0.3),
                            (1.0, 0.0, 0.0),
                        ])),
                    Layer::new(
                        "Shimmer",
                        Source::Osc {
                            wave: Wave::Sine,
                            hz: 1760.0,
                            vibrato_hz: 6.0,
                            vibrato: 0.3,
                        },
                        0.9,
                    )
                    .with_amp(Curve::new(&[
                        (0.0, 0.0, 0.0),
                        (0.2, 0.6, 0.0),
                        (1.0, 0.0, 0.0),
                    ]))
                    .with_gain(0.4)
                    .with_pan(-0.3),
                    Layer::new("Dust", Source::noise(NoiseColor::White, 4000.0), 0.8)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.3, 1.0, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(FilterMode::HighPass, Curve::flat(0.85), 0.2)
                        .with_gain(0.25)
                        .with_pan(0.3),
                ],
                vec![fx(FxKind::Chorus, &[])],
                knobs(0.5, 0.5, 0.0, 0.45),
            ),
            Recipe::Portal => patch(
                "Portal",
                vec![
                    Layer::new("Sweep", Source::osc(Wave::Saw, 110.0), 1.5)
                        .with_pitch(Curve::new(&[(0.0, -12.0, 0.3), (1.0, 24.0, 0.0)]))
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.15, 0.8, 0.0),
                            (0.8, 0.7, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(
                            FilterMode::LowPass,
                            Curve::new(&[(0.0, 0.25, 0.0), (0.6, 0.8, 0.0), (1.0, 0.35, 0.0)]),
                            0.6,
                        ),
                    Layer::new(
                        "Throb",
                        Source::Osc {
                            wave: Wave::Sine,
                            hz: 55.0,
                            vibrato_hz: 7.0,
                            vibrato: 1.0,
                        },
                        1.5,
                    )
                    .with_amp(Curve::new(&[
                        (0.0, 0.0, 0.0),
                        (0.2, 1.0, 0.0),
                        (1.0, 0.0, 0.0),
                    ])),
                    Layer::new("Wind", Source::noise(NoiseColor::White, 4000.0), 1.4)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.5, 0.9, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(
                            FilterMode::BandPass,
                            Curve::new(&[(0.0, 0.3, 0.0), (1.0, 0.85, 0.0)]),
                            0.5,
                        )
                        .with_gain(0.35),
                ],
                vec![fx(FxKind::Phaser, &[])],
                knobs(0.5, 0.5, 0.0, 0.5),
            ),
            Recipe::Hum => patch(
                "Hum",
                vec![
                    Layer::new("Drone", Source::osc(Wave::Saw, 55.0), 2.5)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.25, 0.8, 0.0),
                            (0.75, 0.8, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(FilterMode::LowPass, Curve::flat(0.38), 0.4)
                        .with_pan(-0.4),
                    Layer::new("Beat", Source::osc(Wave::Saw, 55.4), 2.5)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.25, 0.8, 0.0),
                            (0.75, 0.8, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_filter(FilterMode::LowPass, Curve::flat(0.38), 0.4)
                        .with_pan(0.4),
                    Layer::new(
                        "Warble",
                        Source::Osc {
                            wave: Wave::Sine,
                            hz: 110.0,
                            vibrato_hz: 0.4,
                            vibrato: 0.2,
                        },
                        2.5,
                    )
                    .with_amp(Curve::new(&[
                        (0.0, 0.0, 0.0),
                        (0.3, 0.6, 0.0),
                        (0.7, 0.6, 0.0),
                        (1.0, 0.0, 0.0),
                    ])),
                    Layer::new("Air", Source::noise(NoiseColor::Brown, 4000.0), 2.5)
                        .with_amp(Curve::new(&[
                            (0.0, 0.0, 0.0),
                            (0.3, 1.0, 0.0),
                            (0.7, 1.0, 0.0),
                            (1.0, 0.0, 0.0),
                        ]))
                        .with_gain(0.3),
                ],
                vec![fx(FxKind::Chorus, &[])],
                knobs(0.5, 0.5, 0.0, 0.3),
            ),
        }
    }
}

/// The family a game's sounds belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    /// Chip sounds: pulse waves, chip noise, fewer bits.
    Retro,
    /// Soft and bouncy: round waves, deeper sweeps, no grit.
    Cartoon,
    /// Big and dirty: lower, driven, compressed.
    Heavy,
    /// Wobbling, phased, roomy.
    SciFi,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Retro, Style::Cartoon, Style::Heavy, Style::SciFi];

    pub fn label(self) -> &'static str {
        match self {
            Style::Retro => "Retro",
            Style::Cartoon => "Cartoon",
            Style::Heavy => "Heavy",
            Style::SciFi => "Sci-fi",
        }
    }

    /// Turn `p` toward this style.
    pub fn apply(self, p: &mut ForgePatch) {
        let m = &mut p.macros;
        match self {
            Style::Retro => {
                for l in &mut p.layers {
                    match &mut l.source {
                        Source::Osc { wave, .. } => {
                            *wave = match *wave {
                                Wave::Triangle => Wave::Triangle,
                                Wave::Saw => Wave::Pulse12,
                                Wave::Sine => Wave::Triangle,
                                w => w,
                            }
                        }
                        Source::Noise { color, hz } => {
                            *color = NoiseColor::Digital;
                            *hz = 8000.0;
                        }
                        _ => {}
                    }
                    // Chips have no smooth filters.
                    if l.filter == FilterMode::LowPass {
                        l.filter = FilterMode::Off;
                    }
                }
                p.fx.push(fx(
                    FxKind::Bitcrusher,
                    &[(0, 7.0), (1, 22_000.0), (2, 100.0)],
                ));
                m.space *= 0.3;
                m.dirt = m.dirt.min(0.3);
            }
            Style::Cartoon => {
                for l in &mut p.layers {
                    if let Source::Osc { wave, .. } = &mut l.source {
                        *wave = match *wave {
                            Wave::Saw | Wave::Pulse12 => Wave::Triangle,
                            Wave::Square | Wave::Pulse25 => Wave::Sine,
                            w => w,
                        };
                    }
                    if let Source::Noise { color, .. } = &mut l.source
                        && *color == NoiseColor::Brown
                    {
                        *color = NoiseColor::Pink;
                    }
                    l.pitch = l.pitch.scaled(0.0, 1.3);
                }
                m.dirt = 0.0;
                m.size = (m.size - 0.1).max(0.0);
                m.energy = (m.energy + 0.15).min(1.0);
                m.space = m.space.min(0.25);
            }
            Style::Heavy => {
                m.size = (m.size + 0.2).min(1.0);
                m.dirt = m.dirt.max(0.3);
                m.energy = (m.energy + 0.1).min(1.0);
                if p.layers.len() < MAX_LAYERS && !p.layers.iter().any(|l| l.name == "Weight") {
                    let length = p
                        .layers
                        .iter()
                        .map(|l| l.start + l.length)
                        .fold(0.1, f32::max);
                    p.layers.push(
                        Layer::new(
                            "Weight",
                            Source::osc(Wave::Sine, 48.0),
                            (length * 0.6).min(1.2),
                        )
                        .with_pitch(Curve::new(&[(0.0, 7.0, -0.6), (1.0, -5.0, 0.0)]))
                        .with_amp(decay(-0.3))
                        .with_gain(0.8),
                    );
                }
                p.fx.push(fx(FxKind::Compressor, &[]));
            }
            Style::SciFi => {
                for l in &mut p.layers {
                    if let Source::Osc {
                        vibrato_hz,
                        vibrato,
                        ..
                    } = &mut l.source
                        && *vibrato == 0.0
                    {
                        *vibrato_hz = 9.0;
                        *vibrato = 0.35;
                    }
                }
                p.fx.push(fx(FxKind::Phaser, &[]));
                p.fx.push(fx(FxKind::RingMod, &[(1, 25.0)]));
                m.space = m.space.max(0.35);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_are_tidy_and_distinct() {
        let mut seen = Vec::new();
        for r in Recipe::ALL {
            let p = r.patch();
            let mut t = p.clone();
            t.tidy();
            assert_eq!(t, p, "{r:?} is not already tidy");
            assert!(!p.layers.is_empty() && p.layers.len() <= MAX_LAYERS);
            assert!(!seen.contains(&r.stem()));
            seen.push(r.stem());
        }
    }

    #[test]
    fn every_style_keeps_every_recipe_playable() {
        for s in Style::ALL {
            for r in Recipe::ALL {
                let mut p = r.patch();
                s.apply(&mut p);
                let sound = super::super::ForgeSound::new(p);
                assert!(sound.rendered.peak > 0.05, "{s:?} {r:?}");
            }
        }
    }
}
