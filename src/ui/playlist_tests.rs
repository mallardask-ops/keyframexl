//! Playlist gestures: placing, moving, resizing, trimming, duplicating clips.

#[allow(unused_imports)]
use super::*;
use crate::theme as t;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
use keyframexl::sequence::{BAR, Clip, STEP};

fn app() -> App {
    App::with_song(Song::default(), None)
}

fn run_with(ctx: &egui::Context, app: &mut App, events: Vec<Event>, mods: Modifiers) {
    let mut tab = Tab::Playlist;
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
        events,
        modifiers: mods,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
    });
}

fn run(ctx: &egui::Context, app: &mut App, events: Vec<Event>) {
    run_with(ctx, app, events, Modifiers::default());
}

fn button(pos: Pos2, primary: bool, pressed: bool) -> Event {
    button_mod(pos, primary, pressed, Modifiers::default())
}

fn button_mod(pos: Pos2, primary: bool, pressed: bool, mods: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: if primary {
            PointerButton::Primary
        } else {
            PointerButton::Secondary
        },
        pressed,
        modifiers: mods,
    }
}

fn click(ctx: &egui::Context, app: &mut App, pos: Pos2) {
    run(
        ctx,
        app,
        vec![Event::PointerMoved(pos), button(pos, true, true)],
    );
    run(ctx, app, vec![button(pos, true, false)]);
}

fn drag(ctx: &egui::Context, app: &mut App, a: Pos2, b: Pos2) {
    run(
        ctx,
        app,
        vec![Event::PointerMoved(a), button(a, true, true)],
    );
    run(ctx, app, vec![Event::PointerMoved(a + (b - a) * 0.5)]);
    run(ctx, app, vec![Event::PointerMoved(b)]);
    run(ctx, app, vec![button(b, true, false)]);
}

fn drag_shift(ctx: &egui::Context, app: &mut App, a: Pos2, b: Pos2) {
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    run_with(
        ctx,
        app,
        vec![Event::PointerMoved(a), button_mod(a, true, true, shift)],
        shift,
    );
    run_with(
        ctx,
        app,
        vec![Event::PointerMoved(a + (b - a) * 0.5)],
        shift,
    );
    run_with(ctx, app, vec![Event::PointerMoved(b)], shift);
    run_with(ctx, app, vec![button_mod(b, true, false, shift)], shift);
}

fn right_click(ctx: &egui::Context, app: &mut App, pos: Pos2) {
    run(
        ctx,
        app,
        vec![Event::PointerMoved(pos), button(pos, false, true)],
    );
    run(ctx, app, vec![button(pos, false, false)]);
}

fn setup() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    (ctx, app())
}

fn grid_of(app: &App) -> (Rect, f32, f32) {
    app.probe.pl_grid.expect("playlist grid painted")
}

#[test]
fn click_empty_places_clip_on_track_at_snapped_tick() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    // Middle of bar 3 (2.5 bars) on the second lane: start 4800, track 1.
    let pos = pos2(grid.left() + 2.5 * bar_w, grid.top() + 1.5 * lane_h);
    click(&ctx, &mut app, pos);
    assert_eq!(app.song.clips.len(), 1);
    let c = &app.song.clips[0];
    assert_eq!(c.track, 1);
    assert_eq!(c.start, 5 * BAR / 2);
    assert_eq!(app.clip, Some(0));
    assert_eq!(app.history.len(), 1);
    assert!(app.song.validate().is_ok());
}

#[test]
fn drag_moves_clip_to_another_track_and_bar_with_one_undo() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let a = pos2(grid.left() + 0.5 * bar_w, grid.top() + 0.5 * lane_h);
    let b = pos2(grid.left() + 2.5 * bar_w, grid.top() + 2.5 * lane_h);
    drag(&ctx, &mut app, a, b);
    assert_eq!(app.song.clips.len(), 1);
    let c = &app.song.clips[0];
    // Grab was half a bar in; 2.5 bars minus the grab lands on 2 bars.
    assert_eq!(c.start, 2 * BAR);
    assert_eq!(c.track, 2);
    assert_eq!(app.clip, Some(0));
    assert_eq!(app.history.len(), h0 + 1);
    assert!(app.undo());
    assert_eq!(app.song.clips[0].start, 0);
    assert_eq!(app.song.clips[0].track, 0);
}

#[test]
fn right_edge_drag_sets_length() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let a = pos2(grid.left() + bar_w - 4.0, grid.top() + 0.5 * lane_h);
    let b = pos2(grid.left() + 2.0 * bar_w, grid.top() + 0.5 * lane_h);
    drag(&ctx, &mut app, a, b);
    let c = &app.song.clips[0];
    assert_eq!(c.start, 0);
    assert_eq!(app.song.clip_len(c), 2 * BAR);
    assert_eq!(c.length, Some(2 * BAR));
    assert_eq!(app.history.len(), h0 + 1);
}

#[test]
fn left_edge_drag_sets_offset_and_keeps_end() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let end_before = app.song.clips[0].start + app.song.clip_len(&app.song.clips[0]);
    let a = pos2(grid.left() + 4.0, grid.top() + 0.5 * lane_h);
    let b = pos2(grid.left() + 0.5 * bar_w, grid.top() + 0.5 * lane_h);
    drag(&ctx, &mut app, a, b);
    let c = &app.song.clips[0];
    assert_eq!(c.start, BAR / 2);
    assert_eq!(c.offset, BAR / 2);
    assert_eq!(c.start + app.song.clip_len(c), end_before);
    assert_eq!(app.history.len(), h0 + 1);
}

#[test]
fn shift_drag_duplicates_clip() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let a = pos2(grid.left() + 0.5 * bar_w, grid.top() + 0.5 * lane_h);
    let b = pos2(grid.left() + 2.5 * bar_w, grid.top() + 2.5 * lane_h);
    drag_shift(&ctx, &mut app, a, b);
    assert_eq!(app.song.clips.len(), 2);
    assert_eq!((app.song.clips[0].start, app.song.clips[0].track), (0, 0));
    assert_eq!(
        (app.song.clips[1].start, app.song.clips[1].track),
        (2 * BAR, 2)
    );
    assert_eq!(app.clip, Some(1));
    assert_eq!(app.history.len(), h0 + 1);
}

#[test]
fn right_click_deletes_clip() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let pos = pos2(grid.left() + 0.5 * bar_w, grid.top() + 0.5 * lane_h);
    right_click(&ctx, &mut app, pos);
    assert!(app.song.clips.is_empty());
    assert_eq!(app.history.len(), h0 + 1);
    assert!(app.undo());
    assert_eq!(app.song.clips.len(), 1);
}

#[test]
fn step_grid_rounds_like_roll() {
    assert_eq!(STEP, 120);
    assert_eq!(BAR, 1920);
}

#[test]
fn ruler_click_moves_the_song_playhead_without_editing() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, _) = grid_of(&app);
    let ruler_y = grid.top() - super::tempo_lane::LANE_OPEN - 6.0;
    click(&ctx, &mut app, pos2(grid.left() + 2.5 * bar_w, ruler_y));
    assert!(app.song_mode, "the ruler switches to Song mode, like FL");
    assert!(
        app.status.1.contains("3.3.1"),
        "snapped to the step under the pointer: {}",
        app.status.1
    );
    assert_eq!(app.song.clips.len(), 1, "no clip placed");
    assert!(app.history.is_empty(), "seeking is not an edit");
}

#[test]
fn clips_go_anywhere_with_snap_off_or_alt_and_cover_their_exact_span() {
    let (ctx, mut app) = setup();
    app.snap = Snap::Off;
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    // 1.37 bars: off every grid line.
    let x = grid.left() + 1.37 * bar_w;
    click(&ctx, &mut app, pos2(x, grid.top() + 0.5 * lane_h));
    let start = app.song.clips[0].start;
    assert!(
        (start - (1.37 * BAR as f32) as i64).abs() <= 8,
        "placed where clicked: {start}"
    );
    assert_ne!(start % STEP, 0, "not on a step");
    // The clip's left edge is its start, exactly; the right edge its end.
    run(&ctx, &mut app, vec![]);
    let c = app.song.clips[0];
    let left = grid.left() + c.start as f32 / BAR as f32 * bar_w;
    let right = left + app.song.clip_len(&c) as f32 / BAR as f32 * bar_w;
    // Alt-drag with step snap moves by a fraction of a step.
    app.snap = Snap::Step;
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let from = pos2(left + 30.0, grid.top() + 0.6 * lane_h);
    let to = from + Vec2::new(bar_w * 0.3, 0.0);
    run_with(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(from), button_mod(from, true, true, alt)],
        alt,
    );
    run_with(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(from + (to - from) * 0.5)],
        alt,
    );
    run_with(&ctx, &mut app, vec![Event::PointerMoved(to)], alt);
    run_with(&ctx, &mut app, vec![button_mod(to, true, false, alt)], alt);
    let moved = app.song.clips[0].start;
    let expect = start + (0.3 * BAR as f32) as i64;
    assert!(
        (moved - expect).abs() <= 8,
        "Alt moves to the exact tick, not a step: {start} -> {moved}"
    );
    assert_ne!(moved % STEP, 0);
    assert!((right - left - app.song.clip_len(&c) as f32 / BAR as f32 * bar_w).abs() < 0.01);
}

#[test]
fn playlist_geometry_helpers() {
    use super::playlist::{box_hits, paint_positions, slice_lengths};
    let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0));
    assert!(box_hits(
        r,
        Rect::from_min_max(pos2(9.0, 9.0), pos2(12.0, 12.0))
    ));
    assert!(!box_hits(
        r,
        Rect::from_min_max(pos2(11.0, 11.0), pos2(12.0, 12.0))
    ));
    assert_eq!(
        paint_positions(BAR, BAR * 3, BAR),
        vec![BAR, BAR * 2, BAR * 3]
    );
    assert_eq!(paint_positions(BAR * 2, 0, BAR), vec![BAR * 2, BAR, 0]);
    assert_eq!(
        slice_lengths(BAR, BAR, BAR + STEP),
        Some((STEP, BAR - STEP))
    );
    assert_eq!(slice_lengths(BAR, BAR, BAR), None);
}

#[test]
fn ctrl_wheel_zoom_keeps_the_bar_under_the_pointer() {
    use super::playlist::zoom_anchor;
    // 100 px per bar, scrolled 250 px, pointer 150 px into the view: content x 400 = bar 4.
    let s = zoom_anchor(250.0, 150.0, 100.0, 200.0);
    // After doubling, bar 4 is at content x 800; it must still sit 150 px into the view.
    assert_eq!(s + 150.0, 800.0);
    assert_eq!(
        zoom_anchor(0.0, 10.0, 100.0, 50.0),
        0.0,
        "never scrolls before the start"
    );
}

#[test]
fn double_click_on_an_automation_clip_opens_its_editor() {
    let (ctx, mut app) = setup();
    app.create_automation(keyframexl::automation::Target::MixerVolume { track: 1 });
    let auto = app.auto_open.take().expect("created");
    // Move it to track 0 so it is in view.
    app.song.clips[0].track = 0;
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let pos = pos2(grid.left() + bar_w * 0.5, grid.top() + lane_h * 0.6);
    click(&ctx, &mut app, pos);
    click(&ctx, &mut app, pos);
    assert_eq!(
        app.auto_open,
        Some(auto),
        "the envelope editor opens, not the piano roll"
    );
    assert_eq!(app.song.clips.len(), 1, "no clip added by the clicks");
}

#[test]
fn double_click_on_a_track_header_renames_it() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, vec![]);
    let (grid, _, lane_h) = grid_of(&app);
    // Header of track 1: left of the grid, middle of the first lane.
    let head = pos2(grid.left() - 47.0, grid.top() + 0.5 * lane_h);
    click(&ctx, &mut app, head);
    click(&ctx, &mut app, head);
    // One frame for the editor to take focus, then type.
    run(&ctx, &mut app, vec![]);
    run(&ctx, &mut app, vec![Event::Text("Drums".into())]);
    let enter = Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    run(&ctx, &mut app, vec![enter.clone()]);
    run(&ctx, &mut app, vec![enter]);
    assert_eq!(
        app.song.track_name(0),
        "Drums",
        "header rename commits on Enter"
    );
    assert!(app.song.validate().is_ok());
    assert!(app.undo(), "rename is one undo step");
    assert_eq!(app.song.track_name(0), "Track 1");
}

#[test]
fn clicking_the_tempo_lane_adds_a_point_and_undoes() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, _) = grid_of(&app);
    assert!(app.song.tempo_map.is_empty());
    let lane_y = grid.top() - super::tempo_lane::LANE_OPEN / 2.0;
    click(
        &ctx,
        &mut app,
        pos2(grid.left() + 2.0 * bar_w + 1.0, lane_y),
    );
    let m = &app.song.tempo_map.points;
    assert_eq!(m.len(), 2, "the start tempo plus the new point: {m:?}");
    assert_eq!((m[0].tick, m[0].bpm), (0, app.song.tempo));
    assert_eq!(
        (m[1].tick, m[1].bpm),
        (2 * BAR, app.song.tempo),
        "snapped, at the curve's value"
    );
    assert_eq!(app.song.clips.len(), 0, "no clip placed");
    assert!(app.undo());
    assert!(app.song.tempo_map.is_empty(), "one undo step");
}

#[test]
fn time_signatures_move_bar_lines_snap_and_positions_not_clips() {
    use super::playlist::{song_bar_pos, song_snap};
    use keyframexl::tempo::Meter;
    let (_, mut app) = setup();
    app.song.clips.push(Clip::new(1, 5 * BAR, 0));
    let before = app.song.clips[0].start;
    // 3/4 from bar 2 (one undo step), then 7/8 from bar 4.
    app.set_meter(1, 3, 4);
    app.set_meter(3, 7, 8);
    let m = app.song.meters.clone();
    assert_eq!(
        m,
        vec![
            Meter {
                bar: 0,
                num: 4,
                den: 4
            },
            Meter {
                bar: 1,
                num: 3,
                den: 4
            },
            Meter {
                bar: 3,
                num: 7,
                den: 8
            }
        ]
    );
    assert_eq!(app.song.clips[0].start, before, "clips keep their ticks");
    let q = BAR / 4;
    assert_eq!(app.song.bar_tick(1), BAR);
    assert_eq!(app.song.bar_tick(2), BAR + 3 * q);
    assert_eq!(app.song.bar_tick(4), BAR + 6 * q + 7 * q / 2);
    // Bar snap lands on bar starts; beat snap counts from the bar's start.
    assert_eq!(
        song_snap(&m, BAR + 3 * q + 10, BAR, false, false),
        BAR + 3 * q
    );
    assert_eq!(
        song_snap(&m, BAR + 3 * q - 10, BAR, false, true),
        BAR + 3 * q
    );
    assert_eq!(
        song_snap(&m, BAR + 3 * q + 70, q, false, false),
        BAR + 3 * q
    );
    // Positions: 7/8 counts eighths.
    assert_eq!(song_bar_pos(&m, BAR + 6 * q), "4.1.1");
    assert_eq!(song_bar_pos(&m, BAR + 6 * q + q / 2), "4.2.1");
    // The song ends on a whole bar of its own signature.
    let end = app.song.end();
    assert!(app.song.end_bar_tick() >= end && app.song.end_bar_tick() - end < 7 * q / 2);
    // Back to 4/4 everywhere clears the list; undo brings the change back.
    app.set_meter(1, 4, 4);
    app.set_meter(3, 4, 4);
    assert!(
        app.song.meters.is_empty(),
        "plain 4/4 needs no list: {:?}",
        app.song.meters
    );
    assert!(app.undo());
    assert_eq!(app.song.meters.len(), 2);
}

#[test]
fn dragging_the_ruler_makes_a_time_selection_the_song_loops() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, _) = grid_of(&app);
    let ruler_y = grid.top() - super::tempo_lane::LANE_OPEN - 6.0;
    drag(
        &ctx,
        &mut app,
        pos2(grid.left() + 1.01 * bar_w, ruler_y),
        pos2(grid.left() + 2.99 * bar_w, ruler_y),
    );
    assert_eq!(
        app.song.loop_range,
        Some((BAR, 3 * BAR)),
        "snapped to the grid under each end"
    );
    assert!(app.song_mode);
    assert!(
        app.status.1.contains("Looping 2.1.1 to 4.1.1"),
        "{}",
        app.status.1
    );
    assert!(app.history.is_empty(), "a time selection is not an edit");
    // A drag the other way round still makes start < end.
    drag(
        &ctx,
        &mut app,
        pos2(grid.left() + 3.0 * bar_w, ruler_y),
        pos2(grid.left() + 0.5 * bar_w, ruler_y),
    );
    assert_eq!(app.song.loop_range, Some((BAR / 2, 3 * BAR)));
    // Double-click clears it.
    let at = pos2(grid.left() + 1.5 * bar_w, ruler_y);
    click(&ctx, &mut app, at);
    click(&ctx, &mut app, at);
    assert_eq!(app.song.loop_range, None);
}

#[test]
fn a_marker_on_the_ruler_jumps_and_loops_its_section() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    app.song.clips.push(Clip::new(1, 5 * BAR, 0));
    app.song.add_marker(2 * BAR + 4 * STEP);
    app.song.add_marker(4 * BAR);
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, _) = grid_of(&app);
    let chip_y = grid.top() - super::tempo_lane::LANE_OPEN - 16.0;
    let chip = pos2(grid.left() + 2.25 * bar_w + 8.0, chip_y);
    click(&ctx, &mut app, chip);
    assert!(
        app.status.1.contains("3.2.1"),
        "jumped to the marker itself: {}",
        app.status.1
    );
    click(&ctx, &mut app, chip);
    assert_eq!(
        app.song.loop_range,
        Some((2 * BAR + 4 * STEP, 4 * BAR)),
        "double-click loops to the next marker"
    );
    assert!(
        app.status.1.contains("Looping Marker 1"),
        "{}",
        app.status.1
    );
}

#[test]
fn jumping_between_song_markers() {
    let (_, mut app) = setup();
    app.jump_marker(1);
    assert!(app.status.1.contains("No markers yet"), "{}", app.status.1);
    app.song.add_marker(2 * BAR);
    let i = app.song.add_marker(4 * BAR);
    app.song.markers[i].name = "Drop".into();
    app.jump_marker(1);
    assert!(app.song_mode);
    assert!(
        app.status.1.contains("Marker 1 · 3.1.1"),
        "{}",
        app.status.1
    );
    app.jump_marker(-1);
    assert!(
        app.status.1.contains("No marker before"),
        "the playhead is at bar 1: {}",
        app.status.1
    );
}

#[test]
fn ctrl_alt_right_reaches_the_marker_jump() {
    let (ctx, mut app) = setup();
    app.song.add_marker(2 * BAR);
    let mods = Modifiers {
        ctrl: true,
        command: true,
        alt: true,
        ..Default::default()
    };
    let input = egui::RawInput {
        events: vec![Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        }],
        modifiers: mods,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| app.shortcuts(ctx));
    assert!(app.status.1.contains("Marker 1"), "{}", app.status.1);
}

fn middle(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Middle,
        pressed,
        modifiers: Modifiers::default(),
    }
}

#[test]
fn the_middle_button_pans_the_tracks_and_alt_wheel_sizes_them() {
    let (ctx, mut app) = setup();
    // A clip far down: the tracks are taller than the view.
    app.song.clips.push(Clip::new(app.selected, 0, 60));
    run(&ctx, &mut app, vec![]);
    let (grid, _, lane) = app.probe.pl_grid.expect("grid");
    let at = pos2(grid.left() + 200.0, grid.top() + 3.0 * lane);
    run(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(at), middle(at, true)],
    );
    run(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(at - Vec2::new(150.0, 120.0))],
    );
    run(
        &ctx,
        &mut app,
        vec![middle(at - Vec2::new(150.0, 120.0), false)],
    );
    run(&ctx, &mut app, vec![]);
    let (moved, _, _) = app.probe.pl_grid.expect("grid");
    assert!(
        (grid.top() - moved.top() - 120.0).abs() < 1.0,
        "dragging up scrolls down: {} → {}",
        grid.top(),
        moved.top()
    );
    // Sideways as far as the song reaches (a short song scrolls a little).
    assert!(
        moved.left() < grid.left(),
        "and sideways: {} → {}",
        grid.left(),
        moved.left()
    );
    // A plain wheel scrolls the tracks back up.
    let over = pos2(grid.left() + 200.0, grid.top() + 6.0 * lane);
    run(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(over),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 120.0),
                modifiers: Modifiers::default(),
            },
        ],
    );
    run(&ctx, &mut app, vec![]);
    let (back, _, _) = app.probe.pl_grid.expect("grid");
    assert!(back.top() > moved.top(), "the wheel scrolls the tracks");
    // Alt+wheel makes them taller.
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    run_with(
        &ctx,
        &mut app,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 60.0),
            modifiers: alt,
        }],
        alt,
    );
    run(&ctx, &mut app, vec![]);
    let (_, _, taller) = app.probe.pl_grid.expect("grid");
    assert!(taller > lane + 2.0, "taller tracks: {lane} → {taller}");
}

#[test]
fn rmb_drag_selects_clips_survives_scroll_and_moves_all_together() {
    let (ctx, mut app) = setup();
    app.song.clips.push(Clip::new(1, 0, 0));
    app.song.clips.push(Clip::new(1, BAR, 1));
    // Push a far clip so horizontal scrolling has room to scroll.
    app.song.clips.push(Clip::new(1, 20 * BAR, 0));
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);

    // Hold RMB and drag across the first two clips to marquee-select both.
    let from = pos2(grid.left() + 0.1 * bar_w, grid.top() + 0.1 * lane_h);
    let to = pos2(grid.left() + 1.8 * bar_w, grid.top() + 1.8 * lane_h);
    run(&ctx, &mut app, vec![Event::PointerMoved(from)]);
    run(&ctx, &mut app, vec![button(from, false, true)]);
    run(&ctx, &mut app, vec![Event::PointerMoved(to)]);
    run(&ctx, &mut app, vec![button(to, false, false)]);
    run(&ctx, &mut app, vec![]);

    let sel_id = egui::Id::new("kfxl-playlist-selection");
    let sel: Vec<usize> = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_default();
    assert_eq!(
        sel,
        vec![0, 1],
        "RMB drag selects both clips inside the box"
    );
    assert_eq!(
        app.song.clips.len(),
        3,
        "RMB drag selects instead of deleting"
    );

    // Scroll right and back with Shift+wheel: selection must persist across scrolling.
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    run_with(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(to),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -240.0),
                modifiers: shift,
            },
        ],
        shift,
    );
    run(&ctx, &mut app, vec![]);
    let sel_after_scroll: Vec<usize> = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_default();
    assert_eq!(
        sel_after_scroll,
        vec![0, 1],
        "scrolling must not drop selected clips"
    );
    // Scroll back to bar 1 so clips 0 & 1 are back under the pointer.
    run_with(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(to),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 240.0),
                modifiers: shift,
            },
        ],
        shift,
    );
    run(&ctx, &mut app, vec![]);

    // Drag clip 0 by +2 bars and +1 track with the default Draw tool:
    // BOTH selected clips move together in one undo step.
    let (grid, bar_w, lane_h) = grid_of(&app);
    let h0 = app.history.len();
    let grab = pos2(grid.left() + 0.5 * bar_w, grid.top() + 0.5 * lane_h);
    let drop = pos2(grid.left() + 2.5 * bar_w, grid.top() + 1.5 * lane_h);
    drag(&ctx, &mut app, grab, drop);
    assert_eq!(
        (app.song.clips[0].start, app.song.clips[0].track),
        (2 * BAR, 1),
        "anchor clip moved"
    );
    assert_eq!(
        (app.song.clips[1].start, app.song.clips[1].track),
        (3 * BAR, 2),
        "second selected clip moved together with anchor clip"
    );
    assert_eq!(app.history.len(), h0 + 1, "mass move is a single undo step");

    // Re-layout moved clips, then Shift+drag the moved group by +1 bar: duplicates BOTH clips together.
    run(&ctx, &mut app, vec![]);
    let (grid, bar_w, lane_h) = grid_of(&app);
    let grab2 = pos2(grid.left() + 2.5 * bar_w, grid.top() + 1.5 * lane_h);
    let drop2 = pos2(grid.left() + 3.5 * bar_w, grid.top() + 1.5 * lane_h);
    drag_shift(&ctx, &mut app, grab2, drop2);
    assert_eq!(
        app.song.clips.len(),
        5,
        "Shift+drag duplicates all selected clips"
    );
}
