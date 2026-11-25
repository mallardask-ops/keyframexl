use keyframexl::engine::Engine;
use keyframexl::fx::{FxKind, FxSlot};
use keyframexl::sequence::{Instrument, InstrumentKind, Note, Pattern, STEP, Song};
use keyframexl::synth::SynthPatch;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
}

fn short_patch() -> SynthPatch {
    let mut patch = SynthPatch::default();
    patch.amp_env.attack = 0.001;
    patch.amp_env.decay = 0.02;
    patch.amp_env.sustain = 0.0;
    patch.amp_env.release = 0.005;
    patch
}

fn synth_song_with_mixer_fx(kind: FxKind) -> (Song, u64) {
    let mut song = Song::default();
    let id = 10;
    let mut inst = Instrument::new(
        id,
        "Lead",
        InstrumentKind::Synth {
            patch: short_patch(),
        },
    );
    inst.mixer = 1;
    song.instruments.push(inst);
    let mut slot = FxSlot::new(kind);
    slot.mix = 1.0;
    song.mixer[1].fx.push(slot);
    (song, id)
}

#[test]
fn live_midi_on_pause_rings_out_mixer_reverb_and_then_idles() {
    let (song, id) = synth_song_with_mixer_fx(FxKind::Reverb);
    let mut e = Engine::new(RATE);
    let mut buf = vec![0.0f32; BLOCK * 2];

    e.live_on(&song, 1, id, 60, 120);
    // Render while the short percussive note plays and finishes its release.
    for _ in 0..8 {
        e.process_live(&song, &mut buf);
    }
    e.live_off(1);
    while e.active_voices() > 0 {
        e.process_live(&song, &mut buf);
    }

    assert!(
        e.busy(),
        "engine must stay busy after dry voice ends so mixer reverb tail rings out"
    );

    let mut tail_peak = 0.0f32;
    for _ in 0..20 {
        e.process_live(&song, &mut buf);
        tail_peak = tail_peak.max(peak(&buf));
    }
    assert!(
        tail_peak > 1e-3,
        "reverb tail should still be audible after dry voice ends (peak {tail_peak})"
    );

    // Draining enough silence lets the reverb decay and puts the engine back to idle.
    let max_blocks = (RATE as usize * 15) / BLOCK;
    for _ in 0..max_blocks {
        if !e.busy() {
            break;
        }
        e.process_live(&song, &mut buf);
    }
    assert!(
        !e.busy(),
        "engine should return to idle once the reverb tail has decayed"
    );
}

#[test]
fn pause_drains_song_fx_tails_instead_of_replaying_them_on_resume() {
    let (mut song, id) = synth_song_with_mixer_fx(FxKind::Reverb);
    song.add_note(1, Note::new(id, 0, STEP, 60, 120));
    let mut e = Engine::new(RATE);
    let mut buf = vec![0.0f32; BLOCK * 2];

    // Play a short burst so the reverb fills up.
    let mut pos = 0;
    for _ in 0..6 {
        pos = e.process(&song, 1, false, pos, &mut buf, true);
    }
    assert!(peak(&buf) > 1e-3);

    // Pause: transport calls stop(), then runs process_live() while busy().
    e.stop();
    assert!(
        e.busy(),
        "reverb tail from playback must keep busy() true on pause so it drains"
    );

    let max_blocks = (RATE as usize * 15) / BLOCK;
    for _ in 0..max_blocks {
        if !e.busy() {
            break;
        }
        e.process_live(&song, &mut buf);
    }
    assert!(!e.busy(), "tail should finish draining during pause");

    // Resume on an empty pattern: no stale frozen tail may burst out.
    let empty_pat = 2;
    song.patterns.push(Pattern::new(empty_pat, "Empty").into());
    e.process(&song, empty_pat, false, 0, &mut buf, true);
    assert!(
        peak(&buf) < 1e-4,
        "resuming playback must not replay stale reverb tail (peak {})",
        peak(&buf)
    );
}

#[test]
fn live_midi_on_pause_rings_out_synth_fx_tail() {
    let mut song = Song::default();
    let mut patch = short_patch();
    let mut reverb = FxSlot::new(FxKind::Reverb);
    reverb.enabled = true;
    reverb.mix = 0.8;
    patch.fx.push(reverb);
    let id = 11;
    song.instruments
        .push(Instrument::new(id, "Pad", InstrumentKind::Synth { patch }));

    let mut e = Engine::new(RATE);
    let mut buf = vec![0.0f32; BLOCK * 2];

    e.live_on(&song, 1, id, 60, 120);
    for _ in 0..8 {
        e.process_live(&song, &mut buf);
    }
    e.live_off(1);
    while e.active_voices() > 0 {
        e.process_live(&song, &mut buf);
    }

    assert!(
        e.busy(),
        "synth-chain reverb tail must keep busy() true while paused"
    );
    e.process_live(&song, &mut buf);
    assert!(
        peak(&buf) > 1e-3,
        "synth-chain reverb tail should be audible after dry voice ends"
    );
}

#[test]
fn dry_song_without_tail_fx_idles_immediately() {
    let (mut song, id) = synth_song_with_mixer_fx(FxKind::Eq);
    song.mixer[1].fx.clear();
    let mut e = Engine::new(RATE);
    let mut buf = vec![0.0f32; BLOCK * 2];

    e.live_on(&song, 1, id, 60, 120);
    for _ in 0..8 {
        e.process_live(&song, &mut buf);
    }
    e.live_off(1);
    while e.active_voices() > 0 {
        e.process_live(&song, &mut buf);
    }
    assert!(
        !e.busy(),
        "without tail-producing effects, engine must idle right after voices end"
    );
}
