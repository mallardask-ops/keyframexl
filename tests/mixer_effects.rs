use keyframexl::fx::{FxKind, FxSlot, FxState};

#[test]
fn new_effects_round_trip_and_sound_the_same_across_block_sizes() {
    use FxKind::*;
    let input: Vec<_> = (0..24000)
        .flat_map(|i| {
            let phase = i as f32 * 0.13;
            let env = if i % 4000 < 1000 { 0.001 } else { 0.8 };
            [phase.sin() * env, (phase * 1.37).sin() * env]
        })
        .collect();
    for kind in [
        AutoPan, Tremolo, Phaser, RingMod, Wavefolder, NoiseGate, TiltEq, Bitcrusher,
    ] {
        let slot = FxSlot::new(kind);
        let json = serde_json::to_string(&slot).unwrap();
        assert_eq!(serde_json::from_str::<FxSlot>(&json).unwrap(), slot);
        for rate in [44100., 96000.] {
            let render = |block| {
                let mut fx = FxState::new(kind, rate);
                let mut out = input.clone();
                for chunk in out.chunks_mut(block) {
                    fx.process(&slot.params, chunk, rate, 120.);
                }
                out
            };
            let out = render(146);
            assert_eq!(out, render(1024), "{kind:?}");
            assert!(out.iter().all(|v| v.is_finite()), "{kind:?}");
            assert!(
                out.iter().zip(&input).any(|(a, b)| (a - b).abs() > 0.01),
                "{kind:?}"
            );
        }
    }
}

#[test]
fn pan_preserves_mono_source_power_and_gate_rejects_low_level_noise() {
    let mut signal = vec![0.25; 96000];
    let mut pan = FxState::new(FxKind::AutoPan, 48000.);
    pan.process(&[1., 100.], &mut signal, 48000., 120.);
    assert!(
        signal
            .as_chunks::<2>()
            .0
            .iter()
            .all(|f| (f[0] * f[0] + f[1] * f[1] - 0.125).abs() < 1e-6)
    );
    assert!(
        signal
            .as_chunks::<2>()
            .0
            .iter()
            .any(|f| f[0] < 0.001 && f[1] > 0.35)
    );

    let mut gate = FxState::new(FxKind::NoiseGate, 48000.);
    let params = FxKind::NoiseGate.defaults();
    signal.fill(0.001);
    gate.process(&params, &mut signal, 48000., 120.);
    assert!(signal.iter().all(|v| *v == 0.));
    signal.fill(0.2);
    gate.process(&params, &mut signal, 48000., 120.);
    assert!(signal.last().unwrap() > &0.199);
}
