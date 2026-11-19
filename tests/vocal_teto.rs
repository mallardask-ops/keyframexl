//! Sings a short line with a real UTAU voicebank. Set KFXL_VOICEBANK to a
//! bank folder (e.g. an unpacked Kasane Teto) and KFXL_VOCAL_OUT to a wav path;
//! without them the test does nothing, so CI needs no voice data.

use keyframexl::{
    sequence::{BAR, Instrument, InstrumentKind, Note, STEP, Song},
    vocal::Voice,
};

#[test]
fn sings_a_line() {
    let (Ok(bank), Ok(out)) = (
        std::env::var("KFXL_VOICEBANK"),
        std::env::var("KFXL_VOCAL_OUT"),
    ) else {
        return;
    };
    let mut song = Song::default();
    song.instruments.push(Instrument::new(
        10,
        "Teto",
        InstrumentKind::Vocal {
            voice: Voice::new(&bank, "Teto"),
        },
    ));
    song.patterns[0].length = 4 * BAR;
    // か ら お け ー: C4 D4 E4 G4 held G4, then a rest and こんにちは.
    let line = [
        ("ka", 60u8, 4i64),
        ("ra", 62, 4),
        ("o", 64, 4),
        ("ke", 67, 4),
        ("+", 69, 8),
        ("-", 60, 4),
        ("ko", 64, 4),
        ("n", 64, 2),
        ("ni", 65, 4),
        ("chi", 67, 4),
        ("ha", 69, 8),
    ];
    let mut t = 0;
    for (l, p, len) in line {
        let id = song.add_note(1, Note::new(10, t * STEP, len * STEP, p, 100));
        song.patterns[0].lyrics.insert(id, l.to_string());
        t += len;
    }
    let audio = keyframexl::engine::render(&song, 1, false, 48_000);
    assert!(audio.len() > 48_000, "rendered {} frames", audio.len());
    let peak = audio
        .iter()
        .flat_map(|f| f.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.05, "silent: peak {peak}");
    let mut w = hound::WavWriter::create(
        &out,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for f in &audio {
        for c in f {
            w.write_sample((c.clamp(-1.0, 1.0) * 32767.0) as i16)
                .unwrap();
        }
    }
    w.finalize().unwrap();
}

/// Downloads Kasane Teto from her site into the user's voicebank folder.
/// Runs only with KFXL_NET=1.
#[test]
fn downloads_and_installs_teto() {
    if std::env::var("KFXL_NET").is_err() {
        return;
    }
    let dir = keyframexl::vocal::install::download_teto().expect("download");
    let bank = keyframexl::vocal::oto::Voicebank::load(&dir).expect("bank loads");
    assert!(
        bank.entries.contains_key("か"),
        "{} aliases",
        bank.entries.len()
    );
    assert!(
        keyframexl::vocal::install::installed()
            .iter()
            .any(|b| b.path == dir)
    );
    println!(
        "installed {} ({} aliases) at {}",
        bank.name,
        bank.entries.len(),
        dir.display()
    );
}

/// Writes a project with a Teto channel and a sung line (for screenshots).
/// Runs only with KFXL_VOCAL_PROJECT=path.kfxl.
#[test]
fn writes_a_vocal_project() {
    let Ok(out) = std::env::var("KFXL_VOCAL_PROJECT") else {
        return;
    };
    let banks = keyframexl::vocal::install::installed();
    let b = banks.first().expect("Teto installed");
    let mut song = Song::default();
    song.instruments.clear();
    song.patterns[0].notes.clear();
    song.instruments.push(Instrument::new(
        1,
        "Kasane Teto",
        InstrumentKind::Vocal {
            voice: Voice::new(&b.path.to_string_lossy(), &b.name),
        },
    ));
    song.patterns[0].length = 4 * BAR;
    let line = [
        ("ka", 60u8, 4i64),
        ("ra", 62, 4),
        ("o", 64, 4),
        ("ke", 67, 4),
        ("+", 69, 8),
        ("-", 60, 4),
        ("ko", 64, 4),
        ("n", 64, 2),
        ("ni", 65, 4),
        ("chi", 67, 4),
        ("ha", 69, 8),
    ];
    let mut t = 0;
    for (l, p, len) in line {
        if l != "-" {
            let id = song.add_note(1, Note::new(1, t * STEP, len * STEP, p, 100));
            song.patterns[0].lyrics.insert(id, l.to_string());
        }
        t += len;
    }
    keyframexl::project_io::save(&song, std::path::Path::new(&out)).unwrap();
}

/// The screenshot's case: notes from step 2 on, looping pattern playback in blocks.
#[test]
fn looping_playback_starts_each_phrase_on_time() {
    let Ok(bank) = std::env::var("KFXL_VOICEBANK") else {
        return;
    };
    let mut song = Song::default();
    song.instruments.push(Instrument::new(
        10,
        "Teto",
        InstrumentKind::Vocal {
            voice: Voice::new(&bank, "Teto"),
        },
    ));
    for (l, p, at, len) in [("по", 63u8, 1i64, 4i64), ("д", 63, 5, 2), ("ро", 61, 7, 2)] {
        let id = song.add_note(1, Note::new(10, at * STEP, len * STEP, p, 100));
        song.patterns[0].lyrics.insert(id, l.to_string());
    }
    let (map, msgs) = keyframexl::vocal::host::render_all(&song, 48_000);
    println!(
        "phrases {} members {} msgs {:?}",
        map.starts.len(),
        map.members.len(),
        msgs
    );
    for s in keyframexl::vocal::plan::plan(&song, 48_000) {
        println!(
            "spec {:?}",
            s.notes
                .iter()
                .map(|n| (n.lyric.clone(), n.start, n.end))
                .collect::<Vec<_>>()
        );
    }
    let map = std::sync::Arc::new(map);
    let mut e = keyframexl::engine::Engine::new(48_000);
    e.vocal.store(map);
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; 512 * 2];
    let mut pos = 0;
    for _ in 0..(48_000 * 4 / 512) {
        pos = e.process(&song, 1, false, pos, &mut buf, true);
        out.extend(buf.iter().step_by(2).copied());
    }
    let loud: Vec<usize> = out
        .chunks(480)
        .enumerate()
        .filter(|(_, c)| c.iter().map(|v| v * v).sum::<f32>() / 480.0 > 1e-5)
        .map(|(i, _)| i)
        .collect();
    println!(
        "pattern {} ticks; loud 10ms blocks: {:?}",
        song.patterns[0].length, loud
    );
}

/// Writes the screenshot's project (KFXL_VOCAL_PROJECT2=path).
#[test]
fn writes_screenshot_project() {
    let Ok(out) = std::env::var("KFXL_VOCAL_PROJECT2") else {
        return;
    };
    let b = &keyframexl::vocal::install::installed()[0];
    let mut song = Song::default();
    song.instruments.clear();
    song.patterns[0].notes.clear();
    song.instruments.push(Instrument::new(
        1,
        "Vocal",
        InstrumentKind::Vocal {
            voice: Voice::new(&b.path.to_string_lossy(), &b.name),
        },
    ));
    for (l, p, at, len) in [("по", 63u8, 1i64, 4i64), ("д", 63, 5, 2), ("ро", 61, 7, 2)] {
        let id = song.add_note(1, Note::new(1, at * STEP, len * STEP, p, 100));
        song.patterns[0].lyrics.insert(id, l.to_string());
    }
    keyframexl::project_io::save(&song, std::path::Path::new(&out)).unwrap();
}
