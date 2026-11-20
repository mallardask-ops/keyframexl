//! The vocal pipeline on a tiny synthetic UTAU bank (no voice data needed).

use keyframexl::{
    sequence::{BAR, Instrument, InstrumentKind, Note, STEP, Song},
    vocal::{Voice, host, plan},
};

/// A bank with `a` (a held vowel) and `ka` (a click, then the vowel), 44.1 kHz mono.
fn bank() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, click: f64| {
        let mut w = hound::WavWriter::create(
            dir.path().join(name),
            hound::WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        let mut seed = 12345u32;
        for i in 0..(44_100 * 6 / 10) {
            let t = i as f64 / 44_100.0;
            let mut v = 0.0;
            for h in 1..=12 {
                v += (std::f64::consts::TAU * 180.0 * h as f64 * t).sin() / (h as f64).powf(1.3);
            }
            v *= 0.18 * (1.0 - (t - 0.3).abs().min(0.3) / 0.35);
            if t < click {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                v = (seed >> 16) as f64 / 32768.0 - 1.0;
                v *= 0.15;
            }
            w.write_sample((v * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
    };
    write("_a.wav", 0.0);
    write("_ka.wav", 0.05);
    // offset, consonant, cutoff (negative: used length), preutterance, overlap
    std::fs::write(
        dir.path().join("oto.ini"),
        "_a.wav=a,20,60,-400,30,20\n_ka.wav=ka,20,90,-400,60,20\n",
    )
    .unwrap();
    dir
}

fn song_with(bank: &str, notes: &[(&str, u8, i64, i64)]) -> Song {
    let mut song = Song::default();
    song.instruments.push(Instrument::new(
        10,
        "Voice",
        InstrumentKind::Vocal {
            voice: Voice::new(bank, "test"),
        },
    ));
    std::sync::Arc::make_mut(&mut song.patterns[0]).length = 4 * BAR;
    for (l, p, at, len) in notes {
        let id = song.add_note(1, Note::new(10, at * STEP, len * STEP, *p, 100));
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .lyrics
            .insert(id, l.to_string());
    }
    song
}

/// Dominant period of a mono slice by autocorrelation, as Hz.
fn pitch(x: &[f32], rate: f32) -> f32 {
    let (lo, hi) = ((rate / 700.0) as usize, (rate / 80.0) as usize);
    let (mut best, mut at) = (0.0f32, lo);
    for lag in lo..hi {
        let c: f32 = x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
        if c > best {
            best = c;
            at = lag;
        }
    }
    rate / at as f32
}

#[test]
fn sings_the_scored_pitch_and_starts_before_the_note() {
    let dir = bank();
    // One note: ka at C4 on bar 1 step 5 (0.625 s at 120 bpm), 6 steps long.
    let song = song_with(&dir.path().to_string_lossy(), &[("ka", 60, 4, 8)]);
    let audio = keyframexl::engine::render(&song, 1, false, 48_000);
    let mono: Vec<f32> = audio.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
    let note_at = (0.5 * 48_000.0) as usize;
    // The click starts `preutterance` (60 ms) before the note: sound is there by then, silence before.
    let first = mono.iter().position(|v| v.abs() > 0.01).expect("sound");
    assert!(
        first + 48_000 / 10 > note_at && first <= note_at,
        "first sound at {first}, note at {note_at}"
    );
    // The vowel sings the score's pitch (C4 = 261.6 Hz), whatever the bank was recorded at (180 Hz).
    let mid = &mono[note_at + 12_000..note_at + 12_000 + 4096];
    let f = pitch(mid, 48_000.0);
    assert!((f - 261.6).abs() < 6.0, "sang {f} Hz");
}

#[test]
fn legato_notes_make_one_phrase_and_rests_split_it() {
    let dir = bank();
    let b = dir.path().to_string_lossy().to_string();
    let touching = song_with(&b, &[("a", 60, 0, 4), ("ka", 64, 4, 4)]);
    assert_eq!(plan::plan(&touching, 48_000).len(), 1);
    let apart = song_with(&b, &[("a", 60, 0, 4), ("ka", 64, 8, 4)]);
    assert_eq!(plan::plan(&apart, 48_000).len(), 2);
    // A moved phrase is the same audio: nothing to render again.
    let moved = song_with(&b, &[("a", 60, 16, 4), ("ka", 64, 20, 4)]);
    assert_eq!(
        plan::plan(&touching, 48_000)[0].key,
        plan::plan(&moved, 48_000)[0].key
    );
    // A changed lyric is not.
    let other = song_with(&b, &[("ka", 60, 0, 4), ("ka", 64, 4, 4)]);
    assert_ne!(
        plan::plan(&touching, 48_000)[0].key,
        plan::plan(&other, 48_000)[0].key
    );
}

#[test]
fn a_missing_syllable_is_reported_not_fatal() {
    let dir = bank();
    let song = song_with(
        &dir.path().to_string_lossy(),
        &[("a", 60, 0, 4), ("ke", 62, 4, 4)],
    );
    let (map, msgs) = host::render_all(&song, 48_000);
    assert!(!map.starts.is_empty(), "the sung part still plays");
    // "ke" is not in the bank (け): it is listed, not silently dropped.
    assert!(msgs.iter().any(|m| m.contains("け")), "{msgs:?}");
}

#[test]
fn a_missing_bank_is_an_error_message_and_silence() {
    let song = song_with("/nonexistent/bank", &[("a", 60, 0, 4)]);
    let (map, msgs) = host::render_all(&song, 48_000);
    assert!(map.starts.is_empty());
    assert!(!msgs.is_empty());
}

#[test]
fn copying_a_note_copies_its_lyric_and_ids_stay_unique() {
    let dir = bank();
    let mut song = song_with(&dir.path().to_string_lossy(), &[("ka", 60, 0, 4)]);
    let original = song.patterns[0].notes[0];
    let copy = song.add_note(1, original);
    assert_ne!(copy, original.id);
    assert_eq!(
        song.patterns[0].lyrics.get(&copy).map(String::as_str),
        Some("ka")
    );
    // A deleted note's lyric keeps its id taken, so a new note cannot inherit it by accident.
    std::sync::Arc::make_mut(&mut song.patterns[0])
        .notes
        .retain(|n| n.id != copy);
    let fresh = song.add_note(1, Note::new(10, 8 * STEP, STEP, 60, 100));
    assert!(!song.patterns[0].lyrics.contains_key(&fresh));
}

fn run(song: &Song, map: &std::sync::Arc<keyframexl::vocal::VocalMap>, block: usize) -> Vec<f32> {
    use keyframexl::engine::Engine;
    let mut e = Engine::new(48_000);
    e.vocal.store(map.clone());
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; block * 2];
    let mut pos = 0u64;
    for _ in 0..(48_000 * 3 / block) {
        pos = e.process(song, 1, false, pos, &mut buf, false);
        out.extend_from_slice(&buf);
    }
    out
}

#[test]
fn block_size_does_not_change_the_sound_and_a_note_at_zero_starts_cut_not_late() {
    let dir = bank();
    // A note on the very first step: its consonant starts before the song does.
    let song = song_with(
        &dir.path().to_string_lossy(),
        &[("ka", 60, 0, 8), ("a", 64, 8, 8)],
    );
    // Rendered once: WORLD's noise generator carries state between calls.
    let map = std::sync::Arc::new(host::render_all(&song, 48_000).0);
    let a = run(&song, &map, 300);
    let b = run(&song, &map, 512);
    let n = a.len().min(b.len());
    let bad = a[..n]
        .iter()
        .zip(&b[..n])
        .position(|(x, y)| (x - y).abs() > 1e-6);
    assert!(
        bad.is_none(),
        "live blocks differ at sample {:?} ({:?} vs {:?})",
        bad,
        bad.map(|i| a[i]),
        bad.map(|i| b[i])
    );
    // Sound from the first frame (the consonant is cut by the song start, not delayed).
    assert!(a[..4000].iter().any(|v| v.abs() > 0.01));
}

#[test]
fn a_clicked_note_sings_its_own_syllable_not_the_default() {
    use keyframexl::vocal::lyric_hash;
    let dir = bank();
    let mut song = song_with(&dir.path().to_string_lossy(), &[("ka", 60, 0, 4)]);
    if let Some(InstrumentKind::Vocal { voice }) = song
        .instruments
        .iter_mut()
        .find(|i| i.id == 10)
        .map(|i| &mut i.kind)
    {
        voice.default_lyric = "a".into();
    }
    let h = host::Host::new();
    h.update(&song, 48_000);
    let t0 = std::time::Instant::now();
    while !h
        .map
        .load()
        .previews
        .contains_key(&(10, 60, lyric_hash("ka")))
        || !h.map.load().previews.contains_key(&(10, 60, 0))
    {
        assert!(
            t0.elapsed().as_secs() < 30,
            "never rendered: ka {} default {} of {} previews, notes {:?}",
            h.map
                .load()
                .previews
                .contains_key(&(10, 60, lyric_hash("ka"))),
            h.map.load().previews.contains_key(&(10, 60, 0)),
            h.map.load().previews.len(),
            h.status.messages()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let map = h.map.load_full();
    let ka = map.previews[&(10, 60, lyric_hash("ka"))].clone();
    let a = map.previews[&(10, 60, 0)].clone();
    let play = |lyric: u64| {
        let mut e = keyframexl::engine::Engine::new(48_000);
        e.vocal.store(map.clone());
        e.live_lyric
            .store(lyric, std::sync::atomic::Ordering::Relaxed);
        e.live_on(&song, 1, 10, 60, 100);
        let mut buf = vec![0.0f32; 2 * 2048];
        e.process_live(&song, &mut buf);
        buf.iter().step_by(2).copied().collect::<Vec<f32>>()
    };
    let near = |out: &[f32], ph: &[f32]| out.iter().zip(ph).all(|(x, y)| (x - y).abs() < 1e-4);
    assert!(
        near(&play(lyric_hash("ka")), &ka.data),
        "sang something else than ka"
    );
    assert!(near(&play(0), &a.data), "no hint: the default lyric");
}

#[test]
fn a_late_note_sounds_at_its_own_time() {
    let dir = bank();
    // One note on step 9 (1.0 s at 120 bpm).
    let song = song_with(&dir.path().to_string_lossy(), &[("ka", 60, 8, 4)]);
    let audio = keyframexl::engine::render(&song, 1, false, 48_000);
    let first = audio.iter().position(|f| f[0].abs() > 0.01).expect("sound");
    assert!(
        first > 48_000 * 9 / 10,
        "first sound at {first} frames, note at 48000"
    );
}

#[test]
fn a_held_key_sings_with_the_transport_stopped() {
    let dir = bank();
    let mut song = song_with(&dir.path().to_string_lossy(), &[("a", 60, 0, 4)]);
    if let Some(InstrumentKind::Vocal { voice }) = song
        .instruments
        .iter_mut()
        .find(|i| i.id == 10)
        .map(|i| &mut i.kind)
    {
        voice.default_lyric = "a".into();
    }
    let h = host::Host::new();
    h.update(&song, 48_000);
    let t0 = std::time::Instant::now();
    while !h.map.load().previews.contains_key(&(10, 62, 0)) {
        assert!(t0.elapsed().as_secs() < 30);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut e = keyframexl::engine::Engine::new(48_000);
    e.vocal.store(h.map.load_full());
    e.live_on(&song, 1, 10, 62, 100);
    // The audio thread renders a stopped transport only while the engine is busy.
    assert!(e.busy(), "a sounding vocal key must keep the engine busy");
}
