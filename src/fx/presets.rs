//! Factory presets: named parameter sets for the built-in effects. They are
//! code, not files, so they exist on every machine without setup.

use super::FxKind;

pub struct Preset {
    pub name: &'static str,
    pub params: Vec<f32>,
}

fn at(base: &[f32], set: &[(usize, f32)]) -> Vec<f32> {
    let mut p = base.to_vec();
    for &(i, v) in set {
        if let Some(s) = p.get_mut(i) {
            *s = v;
        }
    }
    p
}

/// The preset list for `kind`; empty when the effect has no factory sounds.
pub fn for_kind(kind: FxKind) -> Vec<Preset> {
    let d = kind.defaults();
    let mut out = Vec::new();
    let mut add = |name: &'static str, set: &[(usize, f32)]| {
        out.push(Preset {
            name,
            params: at(&d, set),
        });
    };
    match kind {
        FxKind::Reverb => {
            add(
                "Vocal plate",
                &[
                    (0, 55.0),
                    (1, 35.0),
                    (2, 100.0),
                    (3, 200.0),
                    (4, 18.0),
                    (5, 100.0),
                ],
            );
            add(
                "Large hall",
                &[
                    (0, 92.0),
                    (1, 55.0),
                    (2, 100.0),
                    (3, 90.0),
                    (4, 28.0),
                    (5, 100.0),
                ],
            );
            add(
                "Tight room",
                &[
                    (0, 28.0),
                    (1, 65.0),
                    (2, 100.0),
                    (3, 150.0),
                    (4, 15.0),
                    (5, 100.0),
                ],
            );
        }
        FxKind::Delay => {
            add(
                "Ping-pong 1/8",
                &[
                    (0, 2.0),
                    (1, 45.0),
                    (2, 100.0),
                    (3, 9000.0),
                    (4, 28.0),
                    (5, 100.0),
                ],
            );
            add(
                "Tape slap",
                &[
                    (0, 1.0),
                    (1, 30.0),
                    (2, 0.0),
                    (3, 3500.0),
                    (4, 22.0),
                    (5, 100.0),
                ],
            );
            add(
                "Dub echo",
                &[
                    (0, 3.0),
                    (1, 78.0),
                    (2, 100.0),
                    (3, 2500.0),
                    (4, 35.0),
                    (5, 100.0),
                ],
            );
        }
        FxKind::Compressor => {
            add(
                "Vocal",
                &[(0, -18.0), (1, 3.0), (2, 10.0), (3, 80.0), (4, 3.0)],
            );
            add(
                "Drum bus",
                &[(0, -14.0), (1, 4.0), (2, 20.0), (3, 120.0), (4, 2.0)],
            );
            add(
                "Glue",
                &[(0, -10.0), (1, 2.0), (2, 30.0), (3, 250.0), (4, 1.0)],
            );
        }
        FxKind::Limiter => {
            add("Master safe", &[(0, 0.0), (1, -0.3), (2, 80.0)]);
            add("Loud", &[(0, 4.0), (1, -0.1), (2, 30.0)]);
        }
        FxKind::Bloom => {
            add("Subtle", &[(0, 30.0), (1, 0.0)]);
            add("Warm", &[(0, 55.0), (1, 1.0)]);
            add("Loud", &[(0, 75.0), (1, 2.0)]);
        }
        FxKind::Transient => {
            add("Punch", &[(0, 60.0), (1, 10.0), (2, 0.0)]);
            add("Soften", &[(0, -40.0), (1, 20.0), (2, 0.0)]);
            add("Snare crack", &[(0, 80.0), (1, -10.0), (2, 0.0)]);
        }
        FxKind::MidSide => {
            add("Wide", &[(0, 150.0), (1, 120.0)]);
            add("Mono bass", &[(0, 110.0), (1, 180.0)]);
            add("Mono", &[(0, 0.0), (1, 0.0)]);
        }
        FxKind::Multiband => {
            add(
                "Master glue",
                &[
                    (0, -18.0),
                    (1, 2.0),
                    (2, -16.0),
                    (3, 2.0),
                    (4, -14.0),
                    (5, 1.5),
                    (6, 15.0),
                    (7, 200.0),
                    (8, 1.0),
                ],
            );
            add(
                "Bass control",
                &[
                    (0, -24.0),
                    (1, 3.0),
                    (2, -10.0),
                    (3, 1.5),
                    (4, -8.0),
                    (5, 1.2),
                    (6, 20.0),
                    (7, 150.0),
                    (8, 0.0),
                ],
            );
        }
        FxKind::Time => {
            add(
                "Gate 1/16",
                &[
                    (0, 16.0),
                    (1, 3.0),
                    (2, 25.0),
                    (3, 0.0),
                    (4, 5.0),
                    (5, 0.0),
                    (6, 5.0),
                    (7, 0.0),
                    (8, 5.0),
                    (9, 0.0),
                    (10, 5.0),
                    (11, 0.0),
                    (12, 5.0),
                    (13, 0.0),
                    (14, 5.0),
                    (15, 0.0),
                    (16, 5.0),
                    (17, 0.0),
                    (18, 5.0),
                ],
            );
            add(
                "Stutter 1/8",
                &[
                    (0, 16.0),
                    (1, 4.0),
                    (2, 50.0),
                    (3, 0.0),
                    (4, 0.0),
                    (5, 4.0),
                    (6, 4.0),
                    (7, 0.0),
                    (8, 0.0),
                    (9, 0.0),
                    (10, 4.0),
                    (11, 4.0),
                    (12, 0.0),
                    (13, 0.0),
                    (14, 0.0),
                    (15, 4.0),
                    (16, 4.0),
                    (17, 0.0),
                    (18, 0.0),
                ],
            );
            add(
                "Half-time",
                &[
                    (0, 16.0),
                    (1, 6.0),
                    (2, 25.0),
                    (3, 0.0),
                    (4, 0.0),
                    (5, 0.0),
                    (6, 0.0),
                    (7, 0.0),
                    (8, 0.0),
                    (9, 0.0),
                    (10, 0.0),
                    (11, 1.0),
                    (12, 1.0),
                    (13, 1.0),
                    (14, 1.0),
                    (15, 1.0),
                    (16, 1.0),
                    (17, 1.0),
                    (18, 1.0),
                ],
            );
        }
        FxKind::Vocoder => {
            add("Robot 12", &[(0, 12.0), (1, 4.0), (2, 70.0), (3, 12.0)]);
            add("Airy 16", &[(0, 16.0), (1, 2.0), (2, 120.0), (3, 10.0)]);
        }
        FxKind::Convolution => {
            add("Tight space", &[(0, 0.0), (1, 12000.0), (2, -6.0)]);
            add("Distant", &[(0, 40.0), (1, 6000.0), (2, -4.0)]);
        }
        FxKind::Chorus => {
            add("Wide", &[(0, 12.0), (1, 60.0), (2, 0.45), (3, 55.0)]);
            add("Slow drift", &[(0, 18.0), (1, 35.0), (2, 0.18), (3, 45.0)]);
        }
        FxKind::Drive => {
            add("Warm", &[(0, 6.0), (1, 5000.0), (2, -3.0), (3, 100.0)]);
            add("Crunch", &[(0, 18.0), (1, 7000.0), (2, -6.0), (3, 100.0)]);
        }
        FxKind::Filter => {
            add("Telephone", &[(0, 1.0), (1, 1800.0), (2, 45.0), (3, 100.0)]);
            add("Dark low", &[(0, 0.0), (1, 400.0), (2, 30.0), (3, 100.0)]);
        }
        FxKind::Stereo => {
            add("Wide", &[(0, 140.0), (1, 0.0), (2, 0.0)]);
            add("Narrow", &[(0, 60.0), (1, 0.0), (2, 0.0)]);
        }
        FxKind::Balance => {
            add("Centre", &[(0, 100.0), (1, 0.0)]);
            add("Left", &[(0, 100.0), (1, -100.0)]);
        }
        FxKind::StereoShaper => {
            add(
                "Haas wide",
                &[
                    (0, 100.0),
                    (1, 100.0),
                    (2, 0.0),
                    (3, 0.0),
                    (4, 12.0),
                    (5, 0.0),
                ],
            );
            add(
                "Mono",
                &[
                    (0, 50.0),
                    (1, 50.0),
                    (2, 50.0),
                    (3, 50.0),
                    (4, 0.0),
                    (5, 0.0),
                ],
            );
        }
        FxKind::Squeeze => {
            add(
                "Drum grit",
                &[
                    (0, 70.0),
                    (1, 20.0),
                    (2, 30.0),
                    (3, 1.0),
                    (7, 8000.0),
                    (8, 20.0),
                ],
            );
            add("Telephone", &[(0, 40.0), (7, 1800.0), (8, 60.0), (9, 1.0)]);
        }
        FxKind::HyperChorus => {
            add(
                "Super saw",
                &[
                    (0, 14.0),
                    (1, 30.0),
                    (2, 70.0),
                    (3, 0.5),
                    (4, 80.0),
                    (5, 0.3),
                    (6, 8.0),
                    (7, 80.0),
                    (8, 14000.0),
                    (9, 60.0),
                    (10, 60.0),
                ],
            );
            add(
                "Subtle wide",
                &[
                    (0, 10.0),
                    (1, 10.0),
                    (2, 25.0),
                    (3, 0.4),
                    (4, 40.0),
                    (5, 0.2),
                    (6, 8.0),
                    (7, 120.0),
                    (8, 16000.0),
                    (9, 50.0),
                    (10, 40.0),
                ],
            );
        }
        FxKind::Phaser => {
            add("Slow sweep", &[(0, 0.25), (1, 70.0), (2, 45.0)]);
        }
        FxKind::Eq => {
            add("Smiley", &[(1, 3.0), (3, -2.0), (6, 3.0)]);
        }
        FxKind::TiltEq => {
            add("Bright", &[(0, 4.0), (1, 900.0)]);
            add("Dark", &[(0, -4.0), (1, 900.0)]);
        }
        FxKind::AutoPan => {
            add("Slow", &[(0, 0.3), (1, 60.0)]);
        }
        FxKind::Tremolo => {
            add("Slow", &[(0, 3.0), (1, 50.0), (2, 20.0)]);
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxSlot;

    #[test]
    fn every_preset_is_a_valid_slot() {
        for kind in FxKind::ALL {
            for p in for_kind(kind) {
                let mut slot = FxSlot::new(kind);
                assert_eq!(p.params.len(), slot.params.len(), "{}", p.name);
                slot.params = p.params;
                slot.validate()
                    .unwrap_or_else(|e| panic!("{kind:?} / {}: {e}", p.name));
            }
        }
    }

    #[test]
    fn the_polished_effects_have_sounds() {
        for kind in [
            FxKind::Reverb,
            FxKind::Delay,
            FxKind::Compressor,
            FxKind::Bloom,
            FxKind::Time,
            FxKind::Vocoder,
            FxKind::Convolution,
        ] {
            assert!(!for_kind(kind).is_empty(), "{kind:?}");
        }
    }
}
