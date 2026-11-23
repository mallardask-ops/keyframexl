//! Main menu actions: pattern management, save path, window commands.

use super::*;
use keyframexl::sequence::{Note, STEP};

fn app() -> App {
    App::with_song(Song::demo(), None)
}

#[test]
fn next_empty_pattern_reuses_or_creates() {
    let mut app = app();
    app.next_empty_pattern();
    assert_eq!(
        app.song.patterns.len(),
        2,
        "no empty pattern after 1: a new one"
    );
    let second = app.selected;
    app.select_pattern(1);
    app.next_empty_pattern();
    assert_eq!(app.selected, second, "the empty pattern 2 is reused");
    assert_eq!(app.song.patterns.len(), 2);
}

#[test]
fn clone_copies_notes_with_new_ids_and_undoes() {
    let mut app = app();
    let notes = app.pattern().notes.len();
    app.clone_pattern();
    assert_eq!(app.song.patterns.len(), 2);
    assert_eq!(app.pattern().notes.len(), notes);
    assert_eq!(
        app.pattern_index(app.selected),
        1,
        "the copy sits after the original"
    );
    let ids: std::collections::HashSet<u64> = app
        .song
        .patterns
        .iter()
        .flat_map(|p| p.notes.iter().map(|n| n.id))
        .collect();
    assert_eq!(ids.len(), notes * 2, "every note id stays unique");
    assert!(app.song.validate().is_ok());
    assert!(app.undo());
    assert_eq!(app.song.patterns.len(), 1);
}

#[test]
fn delete_pattern_takes_its_clips_and_keeps_the_last() {
    let mut app = app();
    app.new_pattern();
    let id = app.selected;
    let sel = app.selected;
    app.song.add_note(sel, Note::new(1, 0, STEP, 60, 100));
    app.song
        .clips
        .push(keyframexl::sequence::Clip::new(id, 0, 0));
    app.song.clips.push(keyframexl::sequence::Clip::new(
        1,
        keyframexl::sequence::BAR,
        0,
    ));
    app.delete_pattern();
    assert_eq!(app.song.patterns.len(), 1);
    assert_eq!(app.song.clips.len(), 1);
    assert!(app.song.validate().is_ok());
    app.delete_pattern();
    assert_eq!(app.song.patterns.len(), 1, "the last pattern stays");
    assert_eq!(app.status.0, Tone::Bad);
}

#[test]
fn open_and_save_track_the_path_and_unsaved_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.kfxl");
    keyframexl::project_io::save(&Song::demo(), &path).unwrap();
    let mut app = app();
    app.open_path(&path);
    assert_eq!(app.path.as_deref(), Some(path.as_path()));
    assert!(!app.dirty());
    app.new_pattern();
    assert!(app.dirty());
    app.save();
    assert!(
        !app.dirty(),
        "Save writes to the known path without a dialog"
    );
    assert_eq!(
        keyframexl::project_io::open(&path).unwrap().patterns.len(),
        2
    );
}

#[test]
fn rename_pattern_is_one_undo_step() {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let mut app = app();
    app.start_rename();
    app.renaming.as_mut().unwrap().1 = "Verse".into();
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    let _ = ctx.run(
        egui::RawInput {
            events: vec![enter],
            ..Default::default()
        },
        |ctx| app.rename_window(ctx),
    );
    assert_eq!(app.pattern().name, "Verse");
    assert!(app.renaming.is_none());
    assert!(app.undo());
    assert_eq!(app.pattern().name, "Pattern 1");
}

#[test]
fn rack_channels_move_clone_and_delete_as_one_step_each() {
    let mut app = app();
    app.rack_sel = vec![4];
    app.move_channels(-1);
    let order: Vec<u64> = app.song.instruments.iter().map(|i| i.id).collect();
    assert_eq!(order, vec![1, 2, 4, 3]);
    app.clone_channels();
    assert_eq!(app.song.instruments.len(), 5);
    let clone = app.rack_sel[0];
    assert_eq!(
        app.song.instruments[3].id, clone,
        "the clone sits under its source"
    );
    let src = app.song.patterns[0]
        .notes
        .iter()
        .filter(|n| n.instrument == 4)
        .count();
    assert_eq!(
        app.song.patterns[0]
            .notes
            .iter()
            .filter(|n| n.instrument == clone)
            .count(),
        src
    );
    assert!(app.song.validate().is_ok());
    app.delete_channels();
    assert_eq!(app.song.instruments.len(), 4);
    assert!(app.undo());
    assert_eq!(app.song.instruments.len(), 5);
}

/// Wait for a background preset/sample load to land.
fn finish_import(app: &mut App) {
    for _ in 0..500 {
        if app.importing.is_empty() {
            return;
        }
        app.poll_import();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("import did not finish");
}

#[test]
fn audio_files_load_as_one_zone_samplers_and_drums_convert() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("Snap.wav");
    keyframexl::write_wav(&wav, &vec![[0.25f32, -0.25]; 4800]).unwrap();
    let mut app = app();
    // Double-click in the browser with nothing selected: a new channel named after the file.
    app.load_file(wav.clone(), false);
    finish_import(&mut app);
    let inst = app.song.instruments.last().unwrap();
    assert_eq!(inst.name, "Snap");
    let preset = inst.sampler_preset().unwrap();
    assert_eq!(preset.zones.len(), 1);
    assert_eq!(preset.zones[0].frames.len(), 4800);
    // Dropped on the Kick drum voice: it becomes a sampler with that sample, one undo step.
    let before = app.history.len();
    app.drop_file_on_channel(1, wav);
    finish_import(&mut app);
    assert!(app.is_sampler(1));
    assert_eq!(app.instrument_name(1), "Snap");
    assert_eq!(app.history.len(), before + 1);
    assert!(app.song.validate().is_ok());
}

#[test]
fn unsaved_work_waits_for_an_answer_before_new_or_open() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other.kfxl");
    let mut two = Song::demo();
    two.tempo = 97.0;
    keyframexl::project_io::save(&two, &other).unwrap();
    let mut app = app();
    app.request(Pending::New);
    assert!(app.pending.is_none(), "nothing unsaved: New runs at once");
    app.new_pattern();
    let patterns = app.song.patterns.len();
    app.request(Pending::Open(Some(other.clone())));
    assert_eq!(
        app.pending,
        Some(Pending::Open(Some(other.clone()))),
        "unsaved: Open waits"
    );
    assert_eq!(
        app.song.patterns.len(),
        patterns,
        "the work is untouched while asking"
    );

    // Cancel (Esc) keeps everything.
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let esc = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    let _ = ctx.run(
        egui::RawInput {
            events: vec![esc],
            ..Default::default()
        },
        |ctx| app.unsaved_window(ctx),
    );
    assert!(app.pending.is_none());
    assert!(app.dirty());

    // Save (Enter) writes to the known path, then opens the other project.
    let mine = dir.path().join("mine.kfxl");
    app.path = Some(mine.clone());
    app.request(Pending::Open(Some(other.clone())));
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    let _ = ctx.run(
        egui::RawInput {
            events: vec![enter],
            ..Default::default()
        },
        |ctx| app.unsaved_window(ctx),
    );
    assert_eq!(
        keyframexl::project_io::open(&mine).unwrap().patterns.len(),
        patterns,
        "saved first"
    );
    assert_eq!(app.song.tempo, 97.0, "then opened");
    assert_eq!(app.path.as_deref(), Some(other.as_path()));
    assert!(!app.dirty());
    let names: Vec<_> = app.recent.iter().filter_map(|p| p.file_name()).collect();
    assert_eq!(
        names,
        ["other.kfxl", "mine.kfxl"],
        "Open recent: newest first"
    );
}

#[test]
fn missing_recent_project_drops_out_of_the_list() {
    let mut app = app();
    let gone = std::path::PathBuf::from("/nonexistent/gone.kfxl");
    app.recent = vec![gone.clone()];
    app.open_path(&gone);
    assert!(app.recent.is_empty());
    assert_eq!(app.status.0, Tone::Bad);
}

#[test]
fn insert_solo_mutes_the_rest_but_keeps_its_send_chain() {
    let mut app = app();
    app.song.mixer[2]
        .sends
        .push(keyframexl::sequence::Send { to: 5, level: 0.5 });
    app.solo_insert(2);
    assert!(app.insert_solo(2));
    let muted: Vec<usize> = (0..app.song.mixer.len())
        .filter(|&j| app.song.mixer[j].muted)
        .collect();
    assert!(
        !muted.contains(&0) && !muted.contains(&2) && !muted.contains(&5),
        "Master, the insert and its send target play"
    );
    assert_eq!(muted.len(), app.song.mixer.len() - 3);
    app.solo_insert(2);
    assert!(
        app.song.mixer.iter().all(|m| !m.muted),
        "solo again unmutes all"
    );
    app.undo();
    assert!(app.insert_solo(2), "each solo is one undo step");
}

#[test]
fn arm_record_and_takes_write_numbered_wavs() {
    let mut app = app();
    app.toggle_arm(3);
    assert!(app.is_armed(3));
    app.toggle_record();
    assert!(app.rec.on);
    app.toggle_arm(3);
    assert!(!app.is_armed(3), "a second click disarms");
    let dir = tempfile::tempdir().unwrap();
    let part = |frames: Vec<[f32; 2]>| record::TakePart {
        track: 1,
        rx: rtrb::RingBuffer::new(1).1,
        frames,
        rate: 48_000,
    };
    let parts = vec![part(vec![[0.25, -0.25]; 4800]), part(vec![])];
    let names = vec!["Drums/Bus".to_string(), "Empty".to_string()];
    let files = record::write_takes(dir.path(), "My song", &parts, &names).unwrap();
    assert_eq!(files.len(), 1, "empty takes are skipped");
    assert_eq!(
        files[0].file_name().unwrap(),
        "My song - Drums_Bus - take 1.wav"
    );
    let again = record::write_takes(dir.path(), "My song", &parts, &names).unwrap();
    assert_eq!(
        again[0].file_name().unwrap(),
        "My song - Drums_Bus - take 2.wav",
        "takes never overwrite"
    );
    let (_, back) = crate::import::read_audio(&files[0]).unwrap();
    assert_eq!(back.len(), 4800);
}

fn run_deck(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1366.0, 768.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::TopBottomPanel::top("deck")
            .exact_height(DECK_H)
            .show(ctx, |ui| app.deck(ui));
    });
}

#[test]
fn tempo_field_takes_wheel_and_typed_values() {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let mut app = app();
    run_deck(&ctx, &mut app, vec![]);
    let at = app.probe.bpm.expect("tempo field painted").center();
    run_deck(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
    for _ in 0..30 {
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 50.0),
            modifiers: Default::default(),
        };
        run_deck(&ctx, &mut app, vec![wheel]);
    }
    for _ in 0..60 {
        run_deck(&ctx, &mut app, vec![]);
    }
    assert_eq!(app.song.tempo, 150.0, "30 notches up: +30 BPM");
    assert_eq!(app.history.len(), 1, "one wheel run, one undo step");

    // Double-click opens an empty field; typed digits replace the tempo.
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    for _ in 0..2 {
        run_deck(&ctx, &mut app, vec![press(true)]);
        run_deck(&ctx, &mut app, vec![press(false)]);
    }
    run_deck(&ctx, &mut app, vec![]);
    run_deck(&ctx, &mut app, vec![egui::Event::Text("97".into())]);
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    run_deck(&ctx, &mut app, vec![enter.clone()]);
    run_deck(&ctx, &mut app, vec![]);
    assert_eq!(app.song.tempo, 97.0, "typed value replaces, not appends");
    // An empty Enter keeps the tempo.
    for _ in 0..2 {
        run_deck(&ctx, &mut app, vec![press(true)]);
        run_deck(&ctx, &mut app, vec![press(false)]);
    }
    run_deck(&ctx, &mut app, vec![]);
    run_deck(&ctx, &mut app, vec![enter]);
    assert_eq!(app.song.tempo, 97.0);
}

#[test]
fn export_dialog_renders_on_a_worker_and_closes_with_the_files() {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.export_prefs.wav = Some(keyframexl::render::Format::Wav16);
    app.export_prefs.flac = Some(keyframexl::render::Format::Flac24);
    app.export_prefs.tail = keyframexl::render::Tail::Cut;
    app.open_export(dir.path().join("loop.wav"), false);
    let frame = |app: &mut App, events: Vec<egui::Event>| {
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.export_window(ctx),
        );
    };
    frame(&mut app, vec![]);
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&mut app, vec![enter]);
    // Generous: on a busy CI runner (Windows, tests in parallel) the render
    // has taken over 30 s; a stuck export still fails, with its state.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while app.export.is_some() && std::time::Instant::now() < until {
        frame(&mut app, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        app.export.is_none(),
        "the dialog closes when done; still open at {:?} ({:?}, error {:?}), status: {}",
        app.export.as_ref().map(|d| d.progress),
        app.export.as_ref().map(|d| d.stage.clone()),
        app.export.as_ref().and_then(|d| d.error.clone()),
        app.status.1
    );
    assert_eq!(app.status.0, Tone::Done, "{}", app.status.1);
    assert!(
        app.status.1.contains("loop.wav") && app.status.1.contains("loop.flac"),
        "{}",
        app.status.1
    );
    assert!(dir.path().join("loop.wav").exists() && dir.path().join("loop.flac").exists());
    assert!(app.history.is_empty(), "exporting is not an edit");
}

#[test]
fn save_new_version_counts_up_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.path = Some(dir.path().join("beat.kfxl"));
    app.save();
    app.new_pattern();
    app.save_new_version();
    assert_eq!(
        app.path.as_deref(),
        Some(dir.path().join("beat_2.kfxl").as_path())
    );
    app.save_new_version();
    assert_eq!(
        app.path.as_deref(),
        Some(dir.path().join("beat_3.kfxl").as_path())
    );
    std::fs::write(dir.path().join("beat_4.kfxl"), b"keep").unwrap();
    app.save_new_version();
    assert_eq!(
        app.path.as_deref(),
        Some(dir.path().join("beat_5.kfxl").as_path()),
        "an existing file is skipped"
    );
    assert_eq!(
        std::fs::read(dir.path().join("beat_4.kfxl")).unwrap(),
        b"keep"
    );
    assert_eq!(
        keyframexl::project_io::open(&dir.path().join("beat.kfxl"))
            .unwrap()
            .patterns
            .len(),
        1,
        "the first version is untouched"
    );
    assert!(!app.dirty());
}

#[test]
fn transport_tempo_edits_the_tempo_event_at_the_playhead() {
    use keyframexl::sequence::BAR;
    use keyframexl::tempo::{TempoPoint, TempoShape::*};
    let mut app = app();
    app.song_mode = true;
    // No map: the song tempo, as before.
    app.set_edited_tempo(133.0);
    assert_eq!(app.song.tempo, 133.0);
    app.song.tempo_map.points = vec![
        TempoPoint {
            tick: 0,
            bpm: 120.0,
            shape: Linear,
        },
        TempoPoint {
            tick: 4 * BAR,
            bpm: 160.0,
            shape: Step,
        },
    ];
    // At bar 1 the first event changes, and the song tempo follows it.
    app.set_tempo_at(0, 100.0);
    assert_eq!(app.song.tempo_map.points[0].bpm, 100.0);
    assert_eq!(app.song.tempo, 100.0);
    // Inside the ramp: the ramp splits, a step point at the playhead takes the value.
    app.set_tempo_at(2 * BAR, 90.0);
    let m = &app.song.tempo_map.points;
    assert_eq!(m.len(), 3);
    assert_eq!((m[1].tick, m[1].bpm, m[1].shape), (2 * BAR, 90.0, Step));
    assert_eq!(m[0].bpm, 100.0, "the ramp's left point keeps its value");
    // Past a step point: that point changes, nothing is added.
    app.set_tempo_at(6 * BAR, 170.0);
    assert_eq!(app.song.tempo_map.points.len(), 3);
    assert_eq!(app.song.tempo_map.points[2].bpm, 170.0);
    // Pattern mode plays the song tempo, so the transport edits that.
    app.song_mode = false;
    app.set_edited_tempo(128.0);
    assert_eq!(app.song.tempo, 128.0);
    assert_eq!(app.song.tempo_map.points[0].bpm, 100.0);
}
