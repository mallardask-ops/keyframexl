use super::App;
use super::show::{ArrangeLayout, FallLayout, ShowChannels, ShowView, collect_fall_window};
use crate::theme as t;
use eframe::egui::{self, RawInput, Rect, pos2, vec2};
use keyframexl::sequence::{
    BAR, Clip, InstrumentKind, Note, STEP, Song, clip_notes, song_max_note_len,
};
use keyframexl::tempo::{TempoPoint, TempoShape, TempoTable};

fn demo_song() -> Song {
    keyframexl::demo::build(None)
}

fn synth_kind() -> InstrumentKind {
    InstrumentKind::Synth {
        patch: Default::default(),
    }
}

#[test]
fn clip_notes_respects_offset_repeats_and_four_level_mutes() {
    let mut song = Song::default();
    let inst_a = song.add_channel("Synth A", synth_kind());
    let inst_b = song.add_channel("Synth B", synth_kind());
    let pat = song.patterns[0].id;
    // Pattern is 1 bar (1920 ticks); note 1 at tick 0, note 2 at tick 4*STEP (480).
    song.add_note(pat, Note::new(inst_a, 0, 2 * STEP, 60, 100));
    song.add_note(pat, Note::new(inst_a, 4 * STEP, 2 * STEP, 64, 100));
    song.add_note(pat, Note::new(inst_b, 4 * STEP, 2 * STEP, 67, 100));

    // Clip offset skips tick 0 on the first pass, length spans 1.5 bars so pass 2 triggers tick 0!
    let mut c = Clip::new(pat, BAR, 0);
    c.offset = 2 * STEP;
    c.length = Some(BAR + 4 * STEP);
    song.clips.push(c);

    let notes = clip_notes(&song, 0, 4 * BAR, song_max_note_len(&song));
    // Expected starts:
    // Pass 0 (t in [240, 240+2400=2640)): t=480 (inst_a, inst_b) -> playlist tick BAR + 240
    // Pass 1 (k=1, plen=1920): t=1920 (inst_a) -> playlist tick BAR + 1680; t=2400 (inst_a, inst_b) -> playlist tick BAR + 2160
    let a_starts: Vec<_> = notes
        .iter()
        .filter(|n| n.note.instrument == inst_a)
        .map(|n| n.start)
        .collect();
    assert_eq!(
        a_starts,
        vec![BAR + 2 * STEP, BAR + 14 * STEP, BAR + 18 * STEP]
    );

    // Mute instrument B -> disappears.
    song.instruments
        .iter_mut()
        .find(|i| i.id == inst_b)
        .unwrap()
        .muted = true;
    let after_inst_mute = clip_notes(&song, 0, 4 * BAR, song_max_note_len(&song));
    assert!(after_inst_mute.iter().all(|n| n.note.instrument == inst_a));

    // Mute track 0 -> empty.
    song.track_mut(0).muted = true;
    assert!(clip_notes(&song, 0, 4 * BAR, song_max_note_len(&song)).is_empty());
    song.track_mut(0).muted = false;

    // Mute clip -> empty.
    song.clips[0].muted = true;
    assert!(clip_notes(&song, 0, 4 * BAR, song_max_note_len(&song)).is_empty());
}

#[test]
fn clip_notes_synth_rings_past_clip_end_while_vocal_clips_and_is_monophonic() {
    let mut song = Song::default();
    let synth = song.add_channel("Lead", synth_kind());
    let vocal = song.add_channel(
        "Voice",
        InstrumentKind::Vocal {
            voice: keyframexl::vocal::Voice::new("", "Voice"),
        },
    );
    let pat = song.patterns[0].id;
    // Both start at tick 0 and last 1 bar (1920), plus overlapping vocal note at 4*STEP (480).
    song.add_note(pat, Note::new(synth, 0, BAR, 60, 100));
    song.add_note(pat, Note::new(vocal, 0, BAR, 62, 100));
    song.add_note(pat, Note::new(vocal, 4 * STEP, BAR, 65, 100));

    // Clip ends early at 8*STEP (half bar).
    let mut c = Clip::new(pat, 0, 0);
    c.length = Some(8 * STEP);
    song.clips.push(c);

    let notes = clip_notes(&song, 0, 2 * BAR, BAR);
    let sn = notes.iter().find(|n| n.note.instrument == synth).unwrap();
    assert_eq!(
        sn.end, BAR,
        "synth note must ring for its full length past clip end"
    );

    let vns: Vec<_> = notes
        .iter()
        .filter(|n| n.note.instrument == vocal)
        .collect();
    assert_eq!(vns.len(), 2);
    assert_eq!(
        vns[0].end,
        4 * STEP,
        "first vocal note must clip to next vocal start"
    );
    assert_eq!(
        vns[1].end,
        8 * STEP,
        "vocal note must clip to clip window end"
    );
}

#[test]
fn clip_notes_shared_pattern_emits_distinct_occurrences() {
    let mut song = Song::default();
    let inst = song.add_channel("Keys", synth_kind());
    let pat = song.patterns[0].id;
    song.add_note(pat, Note::new(inst, 0, 4 * STEP, 60, 100));
    song.clips.push(Clip::new(pat, 0, 0));
    song.clips.push(Clip::new(pat, 2 * BAR, 0));

    let notes = clip_notes(&song, 0, 4 * BAR, BAR);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].start, 0);
    assert_eq!(notes[1].start, 2 * BAR);
}

#[test]
fn arrange_layout_handles_extreme_sizes_and_monotonic_mapping() {
    let song = demo_song();
    for (w, h) in [
        (0.0, 0.0),
        (320.0, 240.0),
        (1366.0, 768.0),
        (1920.0, 1080.0),
        (3840.0, 2160.0),
    ] {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
        let layout = ArrangeLayout::compute(&song, rect, (0, song.end_bar_tick()));
        let mut prev = f32::NEG_INFINITY;
        for bar in 0..=song.bars() {
            let x = layout.x_of(song.bar_tick(bar));
            assert!(x.is_finite());
            assert!(x >= prev - 1e-4, "x_of must be monotonic");
            prev = x;
        }
        if w > 0.0 {
            let mid = song.end_bar_tick() / 2;
            let round = layout.tick_at(layout.x_of(mid));
            assert!((round - mid).abs() <= 4);
        }
    }
}

#[test]
fn fall_layout_lands_note_on_play_line_and_wraps_across_loop() {
    let mut song = Song::default();
    song.tempo = 120.0;
    let inst = song.add_channel("Lead", synth_kind());
    let pat = song.patterns[0].id;
    song.add_note(pat, Note::new(inst, 0, 4 * STEP, 60, 100));
    song.clips.push(Clip::new(pat, 0, 0));

    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let layout = FallLayout::compute(&song, rect, ShowChannels::All);
    let rate = 48_000u32;
    let bar_frames = (2.0 * rate as f64) as u64; // 120 BPM => 1 bar = 2.0s
    let now = bar_frames as f64 - 0.5 * rate as f64; // 0.5s before loop wrap
    let lookahead = 2.0 * rate as f64;

    assert!(
        (layout.y_of_frame(now, now, lookahead) - layout.play_y).abs() < 1e-3,
        "event at now_frame must land exactly on play_y"
    );

    let notes = collect_fall_window(
        &song,
        true,
        pat,
        None,
        rate,
        (0, bar_frames),
        now,
        lookahead,
        ShowChannels::All,
        &layout.perc_channels,
    );
    // The note at bar 0 must appear 0.5s in the future via loop wrap!
    assert!(
        notes
            .iter()
            .any(|n| (n.start_frame - (now + 0.5 * rate as f64)).abs() < 4.0),
        "wrapped note after loop end must fall into view smoothly"
    );
}

#[test]
fn fall_window_handles_tempo_ramp_and_swing() {
    let mut song = Song::default();
    song.tempo = 120.0;
    song.swing = 0.5;
    song.tempo_map.points = vec![
        TempoPoint {
            tick: 0,
            bpm: 120.0,
            shape: TempoShape::Step,
        },
        TempoPoint {
            tick: BAR,
            bpm: 180.0,
            shape: TempoShape::Linear,
        },
    ];
    let inst = song.add_channel("Bass", synth_kind());
    let pat = song.patterns[0].id;
    song.add_note(pat, Note::new(inst, STEP, STEP, 48, 100)); // odd 16th -> receives swing
    song.clips.push(Clip::new(pat, 0, 0));

    let rate = 48_000u32;
    let table = TempoTable::build(&song.tempo_map, song.tempo, rate);
    let notes = collect_fall_window(
        &song,
        true,
        pat,
        Some(&table),
        rate,
        (0, table.frames_at(BAR) as u64),
        0.0,
        3.0 * rate as f64,
        ShowChannels::All,
        &[],
    );
    assert!(!notes.is_empty());
    let base_f = table.frames_at(STEP);
    assert!(
        notes[0].start_frame > base_f + 100.0,
        "odd 16th note with swing=0.5 must be shifted late relative to straight grid"
    );
}

#[test]
fn show_enter_exit_restores_demo_loop_without_dirtying_project() {
    let song = demo_song();
    let orig_loop = song.loop_range;
    assert!(orig_loop.is_some(), "demo song has a chorus loop_range");

    let mut app = App::with_song(song, None);
    let ctx = egui::Context::default();
    ctx.set_fonts(t::font_definitions(None));
    assert!(!app.dirty());

    app.enter_show(ShowView::Arrange, Some(&ctx));
    assert!(app.show.is_some());
    assert_eq!(
        app.song.loop_range, None,
        "Demo mode must temporarily clear chorus loop for full-song showcase"
    );
    assert!(
        !app.dirty(),
        "entering Demo mode must not mark project dirty"
    );

    // Paint both Arrange and Falling notes in Dark and Light themes at multiple sizes.
    for theme in [t::Theme::Dark, t::Theme::Light] {
        t::apply(&ctx, theme);
        for (view, size) in [
            (ShowView::Arrange, vec2(1366.0, 768.0)),
            (ShowView::Notes, vec2(1366.0, 768.0)),
            (ShowView::Arrange, vec2(320.0, 200.0)),
            (ShowView::Notes, vec2(320.0, 200.0)),
        ] {
            app.show.as_mut().unwrap().view = view;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| {
                assert!(app.show_overlay(ctx));
            });
        }
    }

    assert!(
        !app.dirty(),
        "Demo mode painting must remain strictly read-only"
    );
    app.exit_show(Some(&ctx));
    assert!(app.show.is_none());
    assert_eq!(
        app.song.loop_range, orig_loop,
        "exiting Demo mode must restore original loop_range verbatim"
    );
    assert!(!app.dirty());
}
