use super::roll::{MoveMode, move_target, snap_floor, snap_round};
use super::*;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Vec2, pos2};
use keyframexl::sequence::{BAR, STEP};

fn app() -> App {
    App::with_song(Song::default(), None)
}

#[derive(Clone, Copy)]
enum View {
    Playlist,
    Rack,
    Roll,
}

fn run(ctx: &egui::Context, app: &mut App, view: View, events: Vec<Event>) {
    run_with(ctx, app, view, events, Modifiers::default());
}

fn run_with(
    ctx: &egui::Context,
    app: &mut App,
    view: View,
    events: Vec<Event>,
    modifiers: Modifiers,
) {
    let mut tab = match view {
        View::Playlist => Tab::Playlist,
        View::Rack => Tab::Rack,
        View::Roll => Tab::Roll { instrument: 4 },
    };
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
        modifiers,
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
    });
}

fn button(pos: Pos2, primary: bool, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: if primary {
            PointerButton::Primary
        } else {
            PointerButton::Secondary
        },
        pressed,
        modifiers: Modifiers::default(),
    }
}

fn click(ctx: &egui::Context, app: &mut App, view: View, pos: Pos2) {
    run(
        ctx,
        app,
        view,
        vec![Event::PointerMoved(pos), button(pos, true, true)],
    );
    run(ctx, app, view, vec![button(pos, true, false)]);
}

fn drag(ctx: &egui::Context, app: &mut App, view: View, a: Pos2, b: Pos2) {
    run(
        ctx,
        app,
        view,
        vec![Event::PointerMoved(a), button(a, true, true)],
    );
    run(ctx, app, view, vec![Event::PointerMoved(a + (b - a) * 0.5)]);
    run(ctx, app, view, vec![Event::PointerMoved(b)]);
    run(ctx, app, view, vec![button(b, true, false)]);
}

fn setup() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    (ctx, app())
}

#[test]
fn move_target_matches_preview_and_commit() {
    let beat = 4 * STEP;
    assert_eq!(
        move_target(
            MoveMode::Move,
            0,
            2 * STEP,
            48,
            0,
            5 * STEP,
            60,
            beat,
            false,
            BAR
        ),
        (beat, 2 * STEP, 60)
    );
    let b = move_target(
        MoveMode::ResizeRight,
        beat,
        2 * STEP,
        60,
        0,
        6 * STEP,
        62,
        beat,
        false,
        BAR,
    );
    assert_eq!(b, (beat, beat, 60));
    // Alt places on any tick.
    assert_eq!(
        move_target(
            MoveMode::Move,
            0,
            2 * STEP,
            48,
            0,
            5 * STEP + 7,
            60,
            beat,
            true,
            BAR
        ),
        (5 * STEP + 7, 2 * STEP, 60)
    );
    // A long note cannot be snapped past the end of the pattern.
    assert_eq!(
        move_target(
            MoveMode::Move,
            0,
            beat,
            48,
            0,
            15 * STEP,
            48,
            BAR,
            false,
            BAR
        )
        .0,
        3 * beat
    );
}

#[test]
fn snap_quantizes_and_alt_bypasses() {
    assert_eq!(snap_round(5 * STEP, 4 * STEP, false), 4 * STEP);
    assert_eq!(snap_round(6 * STEP, 4 * STEP, false), 8 * STEP);
    assert_eq!(snap_floor(7 * STEP, 4 * STEP, false), 4 * STEP);
    assert_eq!(snap_round(6 * STEP + 3, 4 * STEP, true), 6 * STEP + 3);
    // Triplet grid: 160 ticks.
    assert_eq!(snap_round(170, 160, false), 160);
    assert_eq!(snap_floor(37, 1, false), 37);
}

#[test]
fn triplet_snap_draws_off_the_step_grid() {
    let (ctx, mut app) = setup();
    app.snap = Snap::Triplet;
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    // 1.5 steps = 180 ticks: the triplet cell starting at 160.
    click(&ctx, &mut app, View::Roll, g.point(1.5, 60));
    let n = &app.song.patterns[0].notes;
    assert_eq!((n[0].start, n[0].length), (160, 160));
    assert!(app.song.validate().is_ok());
}

#[test]
fn rack_click_toggles_and_undo_restores() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Rack, vec![]);
    let pad = app.probe.pad.expect("pad painted").center();
    click(&ctx, &mut app, View::Rack, pad);
    let step = app.song.patterns[0]
        .step_note(1, 0)
        .map(|i| app.song.patterns[0].notes[i]);
    assert_eq!(
        step.map(|n| (n.start, n.length, n.velocity)),
        Some((0, STEP, 100))
    );
    assert_eq!(app.history.len(), 1);
    assert!(app.undo());
    assert!(app.song.patterns[0].notes.is_empty());
}

#[test]
fn roll_press_places_a_note_and_the_drag_moves_it() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.expect("grid painted");
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(0.5, 48),
        g.point(7.5, 48),
    );
    let n = &app.song.patterns[0].notes;
    assert_eq!(n.len(), 1);
    assert_eq!((n[0].start, n[0].length, n[0].pitch), (7 * STEP, STEP, 48));
    assert_eq!(app.history.len(), 1, "place + move is one undo step");
    app.tool = Tool::Slide;
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(3.5, 60),
        g.point(5.5, 60),
    );
    let s = &app.song.patterns[0].slides;
    assert_eq!(s.len(), 1);
    assert_eq!(
        (s[0].start, s[0].length, s[0].target),
        (3 * STEP, 3 * STEP, 60)
    );
}

#[test]
fn roll_move_is_one_undo_step() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 48));
    let h = app.history.len();
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(0.5, 48),
        g.point(2.5, 48),
    );
    assert_eq!(app.song.patterns[0].notes[0].start, 2 * STEP);
    assert_eq!(app.history.len(), h + 1);
    assert!(app.undo());
    assert_eq!(app.song.patterns[0].notes[0].start, 0);
}

#[test]
fn new_notes_take_the_last_length_and_edges_resize() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    // Grab the right edge of the one-step note and pull it to step 4.
    let edge = pos2(g.x(STEP) - 2.0, g.point(0.0, 50).y);
    drag(&ctx, &mut app, View::Roll, edge, g.point(4.0, 50));
    assert_eq!(app.song.patterns[0].notes[0].length, 4 * STEP);
    // The next note is four steps long too (FL: last touched length).
    click(&ctx, &mut app, View::Roll, g.point(8.5, 52));
    let n = app.song.patterns[0]
        .notes
        .iter()
        .find(|n| n.pitch == 52)
        .unwrap();
    assert_eq!((n.start, n.length), (8 * STEP, 4 * STEP));
}

#[test]
fn shift_drag_duplicates_and_box_select_deletes() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let a = g.point(0.5, 50);
    let b = g.point(4.5, 50);
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![
            Event::PointerMoved(a),
            Event::PointerButton {
                pos: a,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            },
        ],
        shift,
    );
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(b)],
        shift,
    );
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerButton {
            pos: b,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        }],
        shift,
    );
    let starts: Vec<i64> = app.song.patterns[0].notes.iter().map(|n| n.start).collect();
    assert_eq!(starts, vec![0, 4 * STEP], "original stays, copy moved");
    // Ctrl+drag a box over both, then Delete.
    let ctrl = Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let (c, d) = (g.point(0.0, 51) + Vec2::new(2.0, 0.0), g.point(6.0, 49));
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![
            Event::PointerMoved(c),
            Event::PointerButton {
                pos: c,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: ctrl,
            },
        ],
        ctrl,
    );
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(d)],
        ctrl,
    );
    run_with(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerButton {
            pos: d,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: ctrl,
        }],
        ctrl,
    );
    assert_eq!(app.roll_sel.len(), 2);
    app.delete_selected_notes();
    assert!(app.song.patterns[0].notes.is_empty());
}

#[test]
fn duplicate_paste_nudge_and_quantize() {
    let mut app = app();
    app.roll_inst = 4;
    let id = app
        .song
        .add_note(1, keyframexl::sequence::Note::new(4, 0, 2 * STEP, 60, 100));
    app.roll_sel = vec![id];
    app.duplicate_notes();
    assert_eq!(app.song.patterns[0].notes[1].start, 2 * STEP);
    app.nudge_notes(0, 12, 0);
    assert_eq!(app.song.patterns[0].notes[1].pitch, 72);
    app.copy_notes(false);
    app.paste_notes();
    assert_eq!(app.song.patterns[0].notes.len(), 3);
    // Off-grid note quantizes back.
    app.roll_sel.clear();
    std::sync::Arc::make_mut(&mut app.song.patterns[0]).notes[0].start = 37;
    app.quantize_notes();
    assert_eq!(app.song.patterns[0].notes[0].start, 0);
    assert!(app.song.validate().is_ok());
}

#[test]
fn drawing_past_the_end_grows_the_pattern() {
    let mut app = app();
    app.roll_inst = 4;
    let id = app.song.add_note(
        1,
        keyframexl::sequence::Note::new(4, BAR - STEP, STEP, 60, 100),
    );
    app.roll_sel = vec![id];
    app.nudge_notes(4 * STEP, 0, 0);
    assert_eq!(app.song.patterns[0].length, 2 * BAR);
    assert!(app.song.validate().is_ok());
}

#[test]
fn event_lane_sets_pan_of_a_note() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(2.5, 55));
    app.lane_target = super::roll::LaneTarget::Pan;
    let x = g.x(2 * STEP) + 3.0;
    let top = pos2(x, g.lane.top() + 9.0);
    click(&ctx, &mut app, View::Roll, top);
    let n = app.song.patterns[0].notes[0];
    assert!(n.pan > 0.9, "pan near right: {}", n.pan);
}

#[test]
fn pitch_lane_draws_moves_and_removes_a_vocal_curve_point() {
    let (ctx, mut app) = setup();
    let voice = keyframexl::vocal::Voice {
        bank: String::new(),
        name: "Test".into(),
        default_lyric: "a".into(),
        params: Default::default(),
    };
    // The test roll shows channel 4: make it sing.
    let vocal = 4;
    app.song
        .instruments
        .iter_mut()
        .find(|i| i.id == vocal)
        .unwrap()
        .kind = keyframexl::sequence::InstrumentKind::Vocal { voice };
    app.roll_inst = vocal;
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    app.song.add_note(
        1,
        keyframexl::sequence::Note::new(vocal, 2 * STEP, 4 * STEP, 60, 100),
    );
    let note = app.song.patterns[0]
        .notes
        .iter()
        .find(|n| n.instrument == vocal)
        .copied()
        .unwrap();
    app.lane_target = super::roll::LaneTarget::Pitch;
    run(&ctx, &mut app, View::Roll, vec![]);
    // Half way up the upper half: +3 semitones (the lane spans ±6), at the note's start.
    let area_top = g.lane.top() + 8.0;
    let area_bottom = g.lane.bottom() - 6.0;
    let y = (area_top + area_bottom) / 2.0 - (area_bottom - area_top) / 4.0;
    let at = pos2(g.x(note.start), y);
    click(&ctx, &mut app, View::Roll, at);
    let curve = app.song.patterns[0]
        .pitch
        .get(&note.id)
        .cloned()
        .expect("a curve");
    assert_eq!(curve.points.len(), 1);
    assert_eq!(curve.points[0].at, 0);
    assert_eq!(curve.points[0].cents, 300.0);
    // Dragged down to the middle: 0 cents.
    let mid = pos2(at.x, (area_top + area_bottom) / 2.0);
    drag(&ctx, &mut app, View::Roll, at, mid);
    let curve = app.song.patterns[0].pitch.get(&note.id).cloned().unwrap();
    assert_eq!(curve.points[0].cents, 0.0);
    // A double click removes it, and the empty curve goes.
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![
            Event::PointerMoved(mid),
            button(mid, true, true),
            button(mid, true, false),
            button(mid, true, true),
            button(mid, true, false),
        ],
    );
    assert!(!app.song.patterns[0].pitch.contains_key(&note.id));
    assert!(app.song.validate().is_ok());
}

#[test]
fn misses_leave_history_untouched() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    let e = g.point(5.5, 60);
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(e), button(e, false, true)],
    );
    run(&ctx, &mut app, View::Roll, vec![button(e, false, false)]);
    let v = pos2(g.x(5 * STEP + STEP / 2), g.lane.center().y);
    click(&ctx, &mut app, View::Roll, v);
    assert_eq!(app.history.len(), 0);
}

#[test]
fn right_click_erases_one_note() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(2.5, 50));
    click(&ctx, &mut app, View::Roll, g.point(2.5, 55));
    assert_eq!(app.song.patterns[0].notes.len(), 2);
    let e = g.point(2.5, 55);
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(e), button(e, false, true)],
    );
    run(&ctx, &mut app, View::Roll, vec![button(e, false, false)]);
    let n = &app.song.patterns[0].notes;
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].pitch, 50);
}

#[test]
fn playlist_make_unique_detaches_second_clip() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Playlist, vec![]);
    let add = app.probe.add.unwrap().center();
    click(&ctx, &mut app, View::Playlist, add);
    click(&ctx, &mut app, View::Playlist, add);
    assert_eq!(app.song.clips.len(), 2);
    run(&ctx, &mut app, View::Playlist, vec![]);
    let second = app.probe.clip1.unwrap().center();
    click(&ctx, &mut app, View::Playlist, second);
    assert_eq!(app.clip, Some(1));
    run(&ctx, &mut app, View::Playlist, vec![]);
    let unique = app.probe.unique.unwrap().center();
    click(&ctx, &mut app, View::Playlist, unique);
    assert_ne!(app.song.clips[0].pattern, app.song.clips[1].pattern);
    let sel = app.selected;
    app.song
        .add_note(sel, keyframexl::sequence::Note::new(1, 0, STEP, 60, 100));
    assert!(app.song.patterns[0].notes.is_empty());
}

#[test]
fn new_pattern_is_empty_and_selected() {
    let mut app = app();
    app.song = Song::demo();
    app.new_pattern();
    assert_eq!(app.song.patterns.len(), 2);
    assert_eq!(app.selected, 2);
    assert!(app.pattern().notes.is_empty());
}

#[test]
fn window_commands_open_focus_toggle_and_float() {
    let mut app = app();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
    let roll = Tab::Roll { instrument: 4 };
    // Arrange preset starts with Playlist + Rack, no roll.
    assert!(app.ws.dock().find_tab(&roll).is_none());
    app.win(Cmd::Show(roll.clone()));
    app.apply_win_cmds(area);
    // The roll opens as a tab next to the rack and takes focus.
    let (s, n, _) = app.ws.dock().find_tab(&roll).expect("roll opened");
    let (rs, rn, _) = app.ws.dock().find_tab(&Tab::Rack).unwrap();
    assert_eq!((s, n), (rs, rn));
    assert_eq!(workspace::focused(app.ws.dock()), Some(roll.clone()));
    // F7 again closes the focused roll (FL behaviour).
    app.win(Cmd::Toggle(roll.clone()));
    app.apply_win_cmds(area);
    assert!(app.ws.dock().find_tab(&roll).is_none());
    // Float puts it into its own window surface.
    app.win(Cmd::Float(roll.clone()));
    app.apply_win_cmds(area);
    let (s, _, _) = app.ws.dock().find_tab(&roll).unwrap();
    assert!(!s.is_main());
    // Dock brings it back to the main surface.
    app.win(Cmd::Dock(roll.clone()));
    app.apply_win_cmds(area);
    assert!(app.ws.dock().find_tab(&roll).unwrap().0.is_main());
}

#[test]
fn presets_keep_their_own_layout_and_reset() {
    let mut app = app();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
    app.win(Cmd::Close(Tab::Playlist));
    app.apply_win_cmds(area);
    assert!(app.ws.dock().find_tab(&Tab::Playlist).is_none());
    app.win(Cmd::Preset(1));
    app.apply_win_cmds(area);
    assert!(
        app.ws
            .dock()
            .find_tab(&Tab::Roll { instrument: 4 })
            .is_some()
    );
    app.win(Cmd::Preset(0));
    app.apply_win_cmds(area);
    assert!(
        app.ws.dock().find_tab(&Tab::Playlist).is_none(),
        "Arrange remembers its edit"
    );
    app.win(Cmd::Reset);
    app.apply_win_cmds(area);
    assert!(app.ws.dock().find_tab(&Tab::Playlist).is_some());
}

#[test]
fn reopening_closed_playlist_uses_focused_main_pane() {
    let mut app = app();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
    // Open Mixer as a tab beside Rack (not root/Playlist), focus it, close Playlist.
    app.win(Cmd::Show(Tab::Mixer));
    app.apply_win_cmds(area);
    let (ms, _, _) = app.ws.dock().find_tab(&Tab::Mixer).unwrap();
    app.win(Cmd::Close(Tab::Playlist));
    app.apply_win_cmds(area);
    assert!(app.ws.dock().find_tab(&Tab::Playlist).is_none());
    // Ensure Mixer's pane is the focused leaf in main surface.
    app.win(Cmd::Show(Tab::Mixer));
    app.apply_win_cmds(area);
    // Reopening closed Playlist lands in the focused main pane (with Mixer).
    app.win(Cmd::Show(Tab::Playlist));
    app.apply_win_cmds(area);
    let (ps, pn, _) = app
        .ws
        .dock()
        .find_tab(&Tab::Playlist)
        .expect("playlist reopened");
    let (ms2, mn2, _) = app.ws.dock().find_tab(&Tab::Mixer).unwrap();
    assert_eq!((ps, pn), (ms2, mn2));
    assert!(ps.is_main() && ms.is_main());
}

#[test]
fn group_move_node_and_surface_keeps_combined_tabs_together() {
    let mut dock = egui_dock::DockState::new(vec![Tab::Rack]);
    let [_, right] = dock.main_surface_mut().split_right(
        egui_dock::NodeIndex::root(),
        0.5,
        vec![Tab::Mixer, Tab::Playlist],
    );

    // Tear the entire Mixer + Playlist group out into a single floating window.
    dock.move_node(
        (egui_dock::SurfaceIndex::main(), right),
        egui_dock::TabDestination::Window(Rect::from_min_size(
            Pos2::new(100.0, 80.0),
            Vec2::new(500.0, 400.0),
        )),
    );
    let (ms, mn, _) = dock.find_tab(&Tab::Mixer).expect("mixer floated");
    let (ps, pn, _) = dock.find_tab(&Tab::Playlist).expect("playlist floated");
    assert!(!ms.is_main());
    assert_eq!(
        (ms, mn),
        (ps, pn),
        "Mixer and Playlist float together in one window"
    );

    // Dock the entire floating surface back onto the main Rack pane as one group.
    let (rs, rn, _) = dock.find_tab(&Tab::Rack).unwrap();
    dock.move_surface(
        ms,
        egui_dock::TabDestination::Node(rs, rn, egui_dock::TabInsert::Append),
    );
    let (ms2, mn2, _) = dock.find_tab(&Tab::Mixer).expect("mixer re-docked");
    let (ps2, pn2, _) = dock.find_tab(&Tab::Playlist).expect("playlist re-docked");
    assert!(ms2.is_main());
    assert_eq!((ms2, mn2), (rs, rn));
    assert_eq!((ps2, pn2), (rs, rn));
}

#[test]
fn push_silence_scopes_drains_frozen_waveform() {
    let engine = keyframexl::engine::Engine::new(48_000);
    let scope = engine.master_scope();
    scope.track.store(0, std::sync::atomic::Ordering::Relaxed);
    // Write non-zero signal into the ring buffer as if playback just stopped.
    for slot in scope.ring.iter() {
        slot.store(0.75_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
    let mut buf = [0.0f32; 256];
    scope.read(&mut buf);
    assert!(buf.iter().any(|&v| v.abs() > 0.5));
    // Draining silence clears the buffer so the deck monitor settles to 0.
    engine.push_silence_scopes(keyframexl::engine::SCOPE_LEN);
    scope.read(&mut buf);
    assert!(
        buf.iter().all(|&v| v == 0.0),
        "scope must settle to 0.0 after silence"
    );
}

#[test]
fn welcome_modal_blocks_background_rack_clicks_so_new_project_does_not_prompt() {
    let mut app = app();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::font_definitions(None));
    // Start cleanly with Welcome modal open.
    app.welcome_open = true;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 760.0))),
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!app.welcome_open, |ui| app.rack(ui));
        });
    });
    // Click right on a rack step pad while Welcome modal is open.
    let pad = app.probe.pad.expect("rack pad rect").center();
    let raw_press = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 760.0))),
        events: vec![
            egui::Event::PointerMoved(pad),
            egui::Event::PointerButton {
                pos: pad,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
        ],
        ..Default::default()
    };
    let _ = ctx.run(raw_press, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!app.welcome_open, |ui| app.rack(ui));
        });
    });
    assert!(
        !app.dirty(),
        "clicks behind Welcome modal must not dirty the project"
    );
    app.request(super::project::Pending::New);
    assert!(
        app.pending.is_none(),
        "New project from Welcome must not trigger unsaved-changes confirmation"
    );
}

#[test]
fn two_piano_rolls_keep_separate_instruments() {
    let mut app = app();
    app.song
        .instruments
        .push(keyframexl::sequence::Instrument::new(
            9,
            "Lead",
            keyframexl::sequence::InstrumentKind::Sampler { preset: None },
        ));
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
    app.open_roll(4);
    app.open_roll(9);
    app.apply_win_cmds(area);
    let rolls: Vec<_> = app
        .ws
        .dock()
        .iter_all_tabs()
        .filter(|(_, t)| t.kind() == Kind::Roll)
        .collect();
    assert_eq!(rolls.len(), 2);
    // Removing the instrument closes its roll.
    app.song.instruments.retain(|i| i.id != 9);
    app.prune_tabs();
    assert_eq!(
        app.ws
            .dock()
            .iter_all_tabs()
            .filter(|(_, t)| t.kind() == Kind::Roll)
            .count(),
        1
    );
}

#[test]
fn workspace_round_trips_through_ron() {
    let mut app = app();
    app.win(Cmd::Float(Tab::Roll { instrument: 4 }));
    app.apply_win_cmds(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0)));
    let text = app.ws.to_text();
    let back: workspace::Workspace = ron::from_str(&text).expect("layout parses");
    assert_eq!(back.slots.len(), 3);
    assert!(
        back.slots[0]
            .find_tab(&Tab::Roll { instrument: 4 })
            .is_some()
    );
}

#[test]
fn piano_roll_fits_a_short_window() {
    let (ctx, mut app) = setup();
    app.song = Song::demo();
    let mut tab = Tab::Roll { instrument: 4 };
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 330.0))),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
        });
    }
    let g = app.probe.roll.unwrap();
    assert!(g.lane.bottom() <= 330.0, "lane inside the window");
    assert!(g.grid.bottom() <= 330.0);
    // Opens centred between the bass note C3 and its slide target C4.
    assert!(g.visible(54), "F#3 visible: {}..{}", g.bottom, g.top);
    assert!(g.top - g.bottom >= 4);
}

#[test]
fn add_channel_opens_its_instrument_window() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Rack, vec![]);
    let add = app.probe.add_channel.expect("add channel button").center();
    let count = app.song.instruments.len();
    click(&ctx, &mut app, View::Rack, add);
    assert!(ctx.memory(|m| m.any_popup_open()));
    assert_eq!(app.song.instruments.len(), count);
    let id = app.add_sampler();
    assert_eq!(app.song.instruments.last().unwrap().name, "Sampler 1");
    app.apply_win_cmds(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0)));
    let (surface, _, _) = app
        .ws
        .dock()
        .find_tab(&Tab::Instrument { id })
        .expect("window open");
    assert!(!surface.is_main(), "instrument windows float");
}

#[test]
fn preset_loads_into_its_target_channel_and_takes_its_name() {
    let mut app = app();
    let id = app.add_sampler();
    let (tx, rx) = std::sync::mpsc::channel();
    app.importing = vec![(id, rx, false)];
    let preset = keyframexl::sampler::SamplePreset {
        program: Default::default(),
        name: "Warm Keys".into(),
        zones: vec![],
        original_dwp: std::sync::Arc::new(vec![]),
        report: String::new(),
    };
    tx.send(Ok(preset)).unwrap();
    app.poll_import();
    let inst = app.song.instruments.iter().find(|i| i.id == id).unwrap();
    assert_eq!(inst.name, "Warm Keys");
    assert_eq!(inst.sampler_preset().unwrap().name, "Warm Keys");
    // The demo Bass sampler is untouched.
    assert!(
        app.song
            .instruments
            .iter()
            .find(|i| i.id == 4)
            .unwrap()
            .sampler_preset()
            .is_none()
    );
}

#[test]
fn delete_channel_is_one_undo_step() {
    let mut app = app();
    app.song = Song::demo();
    let before = app.song.patterns[0].notes.len();
    app.delete_channel(4);
    assert!(!app.is_sampler(4));
    assert!(app.song.patterns[0].notes.iter().all(|n| n.instrument != 4));
    assert!(app.undo());
    assert!(app.is_sampler(4));
    assert_eq!(app.song.patterns[0].notes.len(), before);
    // Drum channels are ordinary channels: they delete with their steps.
    app.delete_channel(1);
    assert!(!app.song.instruments.iter().any(|i| i.id == 1));
    assert!(app.song.patterns[0].notes.iter().all(|n| n.instrument != 1));
}

#[test]
fn sampler_edits_current_zone_or_all_zones_with_undo() {
    let mut app = app();
    app.song =
        keyframexl::project_io::open(std::path::Path::new("artifacts/dwp-multizone/Zones.kfxl"))
            .expect("fixture project");
    let id = app.default_roll();
    let cutoffs = |app: &App| -> Vec<f64> {
        let inst = app.song.instruments.iter().find(|i| i.id == id).unwrap();
        inst.sampler_preset()
            .unwrap()
            .zones
            .iter()
            .map(|z| z.params.filters[0].cutoff)
            .collect()
    };
    let before = cutoffs(&app);
    app.sampler_ui(id).zone = Some(1);
    app.edit_zones(id, true, |z| z.params.filters[0].cutoff = 900.0);
    let after = cutoffs(&app);
    assert_eq!(after[1], 900.0);
    assert_eq!(after[0], before[0], "other zones untouched");
    app.sampler_ui(id).zone = None;
    app.edit_zones(id, true, |z| z.params.filters[0].cutoff = 500.0);
    assert!(cutoffs(&app).iter().all(|c| *c == 500.0));
    assert!(app.undo());
    assert_eq!(cutoffs(&app), after);
    assert!(app.undo());
    assert_eq!(cutoffs(&app), before);
}

#[test]
fn duplicate_tabs_are_removed() {
    let mut app = app();
    let roll = Tab::Roll { instrument: 4 };
    // Force the duplicate the user saw: the same roll in two places.
    app.ws.dock().push_to_first_leaf(roll.clone());
    app.ws.dock().add_window(vec![roll.clone()]);
    assert_eq!(
        app.ws
            .dock()
            .iter_all_tabs()
            .filter(|(_, t)| **t == roll)
            .count(),
        2
    );
    app.prune_tabs();
    assert_eq!(
        app.ws
            .dock()
            .iter_all_tabs()
            .filter(|(_, t)| **t == roll)
            .count(),
        1
    );
    assert!(
        app.ws.dock().find_tab(&Tab::Playlist).is_some(),
        "other tabs stay"
    );
}

#[test]
fn typed_values_understand_units() {
    use super::kit::parse_value;
    assert_eq!(parse_value("50%", "20%"), Some(0.5));
    assert_eq!(
        parse_value("50", "20%"),
        Some(0.5),
        "bare number takes the shown unit"
    );
    assert_eq!(parse_value("1.5 kHz", "800 Hz"), Some(1500.0));
    assert_eq!(parse_value("250ms", "1.20 s"), Some(0.25));
    assert_eq!(parse_value("-6,5 dB", "0.0 dB"), Some(-6.5));
    assert_eq!(parse_value("abc", "1"), None);
}

/// A knob in a tall scroll area, as in the sampler window.
fn knob_in_scroll(ctx: &egui::Context, value: &mut f64, events: Vec<Event>) -> (Rect, f32, bool) {
    knob_in_scroll_with(ctx, value, events, Modifiers::default())
}

fn knob_in_scroll_with(
    ctx: &egui::Context,
    value: &mut f64,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> (Rect, f32, bool) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
        modifiers,
        events,
        ..Default::default()
    };
    let mut out = (Rect::NOTHING, 0.0, false);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let sa = egui::ScrollArea::vertical()
                .id_salt("knobs")
                .show(ui, |ui| {
                    let k = super::kit::Knob {
                        label: "Cut",
                        color: t::enc_blue(),
                        min: 0.0,
                        max: 1.0,
                        default: 0.5,
                        log: false,
                        fmt: &|v| format!("{v:.3}"),
                        hint: "",
                    };
                    let (r, started, _) = super::kit::knob(ui, &k, value);
                    ui.allocate_space(Vec2::new(10.0, 2000.0));
                    out.0 = r.rect;
                    out.2 = started;
                });
            out.1 = sa.state.offset.y;
        });
    });
    out
}

#[test]
fn wheel_turns_the_knob_not_the_page() {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let mut v = 0.5;
    let (rect, _, _) = knob_in_scroll(&ctx, &mut v, vec![]);
    let at = rect.center();
    let wheel = |dy: f32| Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(0.0, dy),
        modifiers: Modifiers::default(),
    };
    let mut starts = 0;
    let (_, _, s) = knob_in_scroll(&ctx, &mut v, vec![Event::PointerMoved(at), wheel(50.0)]);
    starts += s as i32;
    let mut offset = 0.0;
    for _ in 0..30 {
        let (_, o, s) = knob_in_scroll(&ctx, &mut v, vec![]);
        starts += s as i32;
        offset = o;
    }
    assert_eq!(offset, 0.0, "the page did not scroll");
    assert!(
        (v - 0.51).abs() < 0.002,
        "one notch is 1% of the travel, got {v}"
    );
    assert_eq!(starts, 1, "one wheel turn is one undo step");
    // Away from the knob the wheel scrolls the page as usual.
    let away = rect.center() + Vec2::new(0.0, 120.0);
    knob_in_scroll(&ctx, &mut v, vec![Event::PointerMoved(away), wheel(-80.0)]);
    for _ in 0..30 {
        offset = knob_in_scroll(&ctx, &mut v, vec![]).1;
    }
    assert!(offset > 10.0, "page scrolls elsewhere: {offset}");
}

#[test]
fn knob_drag_travel_and_fine_mode() {
    let ctx = egui::Context::default();
    t::setup(&ctx);
    let mut v = 0.0;
    let (rect, _, _) = knob_in_scroll(&ctx, &mut v, vec![]);
    let a = rect.center();
    let mv = |p: Pos2| Event::PointerMoved(p);
    knob_in_scroll(&ctx, &mut v, vec![mv(a), button(a, true, true)]);
    knob_in_scroll(&ctx, &mut v, vec![mv(a - Vec2::new(0.0, 4.0))]);
    let start = v;
    knob_in_scroll(
        &ctx,
        &mut v,
        vec![
            mv(a - Vec2::new(0.0, 104.0)),
            Event::MouseMoved(Vec2::new(0.0, -100.0)),
        ],
    );
    knob_in_scroll(
        &ctx,
        &mut v,
        vec![button(a - Vec2::new(0.0, 104.0), true, false)],
    );
    assert!(
        (v - start - 0.5).abs() < 0.01,
        "100 px is half the travel: {start} -> {v}"
    );
    // Shift: ten times finer.
    let before = v;
    let b = a;
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    knob_in_scroll(&ctx, &mut v, vec![mv(b), button(b, true, true)]);
    knob_in_scroll(&ctx, &mut v, vec![mv(b + Vec2::new(0.0, 4.0))]);
    let mid = v;
    knob_in_scroll_with(
        &ctx,
        &mut v,
        vec![
            mv(b + Vec2::new(0.0, 104.0)),
            Event::MouseMoved(Vec2::new(0.0, 100.0)),
        ],
        shift,
    );
    knob_in_scroll_with(
        &ctx,
        &mut v,
        vec![button(b + Vec2::new(0.0, 104.0), true, false)],
        shift,
    );
    assert!(
        ((mid - v) - 0.05).abs() < 0.005,
        "Shift: 100 px is 5%: {before} {mid} -> {v}"
    );
    // Double-click resets to the default.
    v = 0.9;
    for _ in 0..2 {
        knob_in_scroll(&ctx, &mut v, vec![mv(a), button(a, true, true)]);
        knob_in_scroll(&ctx, &mut v, vec![button(a, true, false)]);
    }
    assert_eq!(v, 0.5);
}

#[test]
fn browser_search_filters_by_substring() {
    use super::browser::matches;
    assert!(matches("", "Pattern 1"));
    assert!(matches("pat", "Pattern 1"));
    assert!(matches("PAT", "pattern 2"));
    assert!(!matches("bass", "Pattern 1"));
    assert!(matches("kick", "808 Kick"));
    assert!(!matches("kick", "Snare"));
}

#[test]
fn paint_tool_lays_a_run_of_notes_in_one_undo_step() {
    let (ctx, mut app) = setup();
    app.tool = Tool::Paint;
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(0.5, 50),
        g.point(3.5, 52),
    );
    let mut starts: Vec<i64> = app.song.patterns[0].notes.iter().map(|n| n.start).collect();
    starts.sort();
    assert_eq!(
        starts,
        vec![0, STEP, 2 * STEP, 3 * STEP],
        "one note per step the pointer passed"
    );
    assert_eq!(app.history.len(), 1);
    assert!(app.undo());
    assert!(app.song.patterns[0].notes.is_empty());
}

#[test]
fn delete_mute_and_slice_tools() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    for (s, p) in [(0.5, 50), (4.5, 50), (8.5, 50)] {
        click(&ctx, &mut app, View::Roll, g.point(s, p));
    }
    // Make the third note four steps long for the slice.
    std::sync::Arc::make_mut(&mut app.song.patterns[0]).notes[2].length = 4 * STEP;
    app.tool = Tool::Mute;
    click(&ctx, &mut app, View::Roll, g.point(4.5, 50));
    assert!(app.song.patterns[0].notes[1].muted);
    let h = app.history.len();
    app.tool = Tool::Delete;
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    assert_eq!(app.song.patterns[0].notes.len(), 2);
    assert_eq!(app.history.len(), h + 1);
    app.tool = Tool::Slice;
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(10.0, 53),
        g.point(10.0, 47),
    );
    let mut cut: Vec<(i64, i64)> = app.song.patterns[0]
        .notes
        .iter()
        .filter(|n| n.start >= 8 * STEP)
        .map(|n| (n.start, n.length))
        .collect();
    cut.sort();
    assert_eq!(cut, vec![(8 * STEP, 2 * STEP), (10 * STEP, 2 * STEP)]);
    assert!(app.song.validate().is_ok());
}

#[test]
fn select_tool_boxes_without_ctrl_and_zoom_tool_fills_the_view() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    click(&ctx, &mut app, View::Roll, g.point(2.5, 50));
    app.roll_sel.clear();
    app.tool = Tool::Select;
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(0.2, 52),
        g.point(3.8, 48),
    );
    assert_eq!(app.roll_sel.len(), 2);
    app.tool = Tool::Zoom;
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(2.0, 50),
        g.point(14.0, 50),
    );
    run(&ctx, &mut app, View::Roll, vec![]);
    let z = app.probe.roll.unwrap();
    assert!(
        (z.cell - g.grid.width() / 12.0).abs() < 1.0,
        "twelve steps fill the grid: {}",
        z.cell
    );
    assert_eq!(z.x0, 2 * STEP);
}

#[test]
fn note_colour_applies_to_selection_and_new_notes() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    app.set_note_color(2);
    assert_eq!(app.song.patterns[0].notes[0].color, 2);
    click(&ctx, &mut app, View::Roll, g.point(4.5, 50));
    assert!(app.song.patterns[0].notes.iter().all(|n| n.color == 2));
}

#[test]
fn slides_move_and_resize_by_hand() {
    let (ctx, mut app) = setup();
    app.tool = Tool::Slide;
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    drag(
        &ctx,
        &mut app,
        View::Roll,
        g.point(2.5, 60),
        g.point(5.5, 60),
    );
    let made = app.song.patterns[0].slides.len();
    let s0 = *app.song.patterns[0].slides.last().unwrap();
    let h = app.history.len();
    // Drag the body: two steps later and two semitones up, one undo step.
    let body = pos2(g.x(s0.start) + g.cell * 1.5, g.point(0.0, 60).y);
    drag(
        &ctx,
        &mut app,
        View::Roll,
        body,
        body + vec2(2.0 * g.cell, -2.0 * g.row),
    );
    let s1 = *app.song.patterns[0].slides.last().unwrap();
    assert_eq!(app.song.patterns[0].slides.len(), made, "moved, not added");
    assert_eq!(
        (s1.start, s1.target, s1.length),
        (s0.start + 2 * STEP, 62, s0.length)
    );
    assert_eq!(app.history.len(), h + 1);
    // Pull the right edge out by two steps (Draw tool edits slides too).
    app.tool = Tool::Draw;
    let edge = pos2(g.x(s1.start + s1.length) - 2.0, g.point(0.0, 62).y);
    drag(
        &ctx,
        &mut app,
        View::Roll,
        edge,
        pos2(g.x(s1.start + s1.length + 2 * STEP), edge.y),
    );
    let s2 = *app.song.patterns[0].slides.last().unwrap();
    assert_eq!((s2.start, s2.length), (s1.start, s1.length + 2 * STEP));
    assert!(
        app.song.patterns[0].notes.iter().all(|n| n.pitch != 62),
        "no note was drawn on the slide"
    );
}

#[test]
fn porta_toggles_on_selection_and_new_notes() {
    let (ctx, mut app) = setup();
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.unwrap();
    click(&ctx, &mut app, View::Roll, g.point(0.5, 50));
    app.porta_notes();
    assert!(
        app.song.patterns[0]
            .notes
            .iter()
            .find(|n| n.pitch == 50)
            .unwrap()
            .porta
    );
    app.porta_notes();
    assert!(
        !app.song.patterns[0]
            .notes
            .iter()
            .find(|n| n.pitch == 50)
            .unwrap()
            .porta,
        "again turns it off"
    );
    app.porta_new = true;
    click(&ctx, &mut app, View::Roll, g.point(6.5, 53));
    assert!(
        app.song.patterns[0]
            .notes
            .iter()
            .find(|n| n.pitch == 53)
            .unwrap()
            .porta
    );
}

#[test]
fn create_automation_clip_from_a_control() {
    use keyframexl::automation::Target;
    let mut app = app();
    app.song.mixer[2].volume = 0.5;
    let before = app.song.patterns.len();
    app.create_automation(Target::MixerVolume { track: 2 });
    let p = app.song.patterns.last().expect("pattern added");
    assert_eq!(app.song.patterns.len(), before + 1);
    let env = p.automation.as_ref().expect("an automation clip");
    assert_eq!(env.value_at(0), Some(0.5), "starts at the control's value");
    assert!(
        p.name.ends_with("volume"),
        "named after the control: {}",
        p.name
    );
    let clip = app
        .song
        .clips
        .iter()
        .find(|c| c.pattern == p.id)
        .expect("placed in the playlist");
    assert_eq!(clip.start, 0);
    assert_eq!(app.auto_open, Some(p.id), "editor opens");
    // F4 skips it: automation clips are not patterns to fill.
    app.selected = app.song.patterns[0].id;
    app.next_empty_pattern();
    assert!(app.pattern().automation.is_none());
    // Undo (the new pattern from F4, then the clip) takes it away again.
    app.undo();
    app.undo();
    assert_eq!(app.song.patterns.len(), before);
    assert!(app.song.clips.is_empty());
}

#[test]
fn cloning_plugin_channel_assigns_fresh_plugin_id() {
    use keyframexl::sequence::{InstrumentKind, PluginRef};
    let mut app = app();
    let plugin = PluginRef {
        id: 11,
        format: "vst3".into(),
        path: "/x/GrandPiano.vst3".into(),
        name: "Grand Piano".into(),
        vendor: String::new(),
        state: String::new(),
        outs: vec![],
    };
    let orig_id = app
        .song
        .add_channel("Grand Piano", InstrumentKind::Plugin { plugin });
    app.song.plugin_ref_mut(11).unwrap().set_state(b"preset-1");
    app.rack_sel = vec![orig_id];

    app.clone_channels();

    assert_eq!(app.rack_sel.len(), 1);
    let clone_id = app.rack_sel[0];
    assert_ne!(clone_id, orig_id);
    let orig_plug = app.song.instrument(orig_id).unwrap().plugin().unwrap();
    let clone_plug = app.song.instrument(clone_id).unwrap().plugin().unwrap();
    assert_eq!(orig_plug.id, 11);
    assert_ne!(
        clone_plug.id, orig_plug.id,
        "cloned plugin channel must get its own plugin instance id"
    );
    assert_eq!(clone_plug.state_bytes(), b"preset-1");
}

#[test]
fn plugin_parameter_edits_undo_and_window_edits_are_kept() {
    use keyframexl::sequence::{InstrumentKind, PluginRef};
    let mut app = app();
    let plugin = PluginRef {
        id: 77,
        format: "vst3".into(),
        path: "/x/P.vst3".into(),
        name: "P".into(),
        vendor: String::new(),
        state: String::new(),
        outs: vec![],
    };
    app.song.instruments[0].kind = InstrumentKind::Plugin { plugin };
    app.song.plugin_ref_mut(77).unwrap().set_state(b"s0");
    app.plugins.live.insert(
        77,
        super::plugins_ui::Live {
            path: "/x/P.vst3".into(),
            proc: None,
            name: "P".into(),
            gui: true,
            editor: false,
            error: None,
            show_when_loaded: false,
            outs: 0,
        },
    );
    let state = |app: &App| app.song.plugin_refs()[0].state_bytes();

    // An unrelated edit, then the plugin's own window changes it: the snapshot
    // takes the new state too, so undoing the edit does not revert the plugin.
    app.checkpoint();
    app.store_states(vec![(77, b"s1".to_vec())]);
    assert_eq!(
        app.history.last().unwrap().plugin_refs()[0].state_bytes(),
        b"s1"
    );
    app.undo();
    assert_eq!(state(&app), b"s1");

    // A Parameters-page edit is its own undo step: the read-back state stays
    // out of the snapshot, and undo goes back to the state before it, with the
    // parameter values seen with that state (some plugins do not report them).
    let row = |value| crate::plugins::ParamRow {
        id: 3,
        name: "Drive".into(),
        value,
        text: String::new(),
        automatable: true,
    };
    app.plugins.params.insert(77, (vec![row(0.0)], 5.0));
    app.store_states(vec![(77, b"s1".to_vec())]);
    app.checkpoint();
    app.plugins.param_dirty = true;
    app.plugins.params.get_mut(&77).unwrap().0[0].value = 0.7;
    app.store_states(vec![(77, b"s2".to_vec())]);
    assert_eq!(
        app.history.last().unwrap().plugin_refs()[0].state_bytes(),
        b"s1"
    );
    assert!(!app.plugins.param_dirty, "read back");
    app.undo();
    assert_eq!(state(&app), b"s1");
    assert_eq!(
        app.plugins.params[&77].0[0].value, 0.0,
        "the value goes back too"
    );
    assert_eq!(
        app.plugins.params[&77].1,
        f64::NEG_INFINITY,
        "the parameter list is read again"
    );
    app.redo();
    assert_eq!(app.plugins.params[&77].0[0].value, 0.7);
    assert_eq!(state(&app), b"s2");
}

/// Two piano rolls side by side in one frame (A left, B right).
fn run_two_rolls(ctx: &egui::Context, app: &mut App, a: u64, b: u64, events: Vec<Event>) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let full = ui.max_rect();
            let (l, r) = full.split_left_right_at_fraction(0.5);
            for (rect, inst) in [(l, a), (r, b)] {
                let mut tab = Tab::Roll { instrument: inst };
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(4.0)), |ui| {
                    app.tab_ui(ui, &mut tab)
                });
            }
        });
    });
}

#[test]
fn notes_copied_in_one_open_roll_paste_into_another() {
    let ctx = egui::Context::default();
    let mut app = app();
    let (a, b) = (4u64, app.song.instruments[0].id);
    assert_ne!(a, b);
    for inst in [a, b] {
        app.ws
            .dock()
            .push_to_first_leaf(Tab::Roll { instrument: inst });
    }
    app.focus = Some(Tab::Roll { instrument: a });
    app.roll_inst = a;
    let id = app
        .song
        .add_note(1, keyframexl::sequence::Note::new(a, 0, STEP, 48, 100));
    app.roll_sel = vec![id];
    // Drawing the other roll no longer takes the selection or the keys away.
    run_two_rolls(&ctx, &mut app, a, b, vec![]);
    assert_eq!(app.roll_inst, a, "the focused roll keeps the keys");
    assert_eq!(
        app.roll_sel,
        vec![id],
        "the selection survives the other roll"
    );
    app.copy_notes(false);
    // A press in roll B makes it the one Paste goes to.
    let in_b = pos2(1366.0 * 0.75, 400.0);
    run_two_rolls(
        &ctx,
        &mut app,
        a,
        b,
        vec![Event::PointerMoved(in_b), button(in_b, true, true)],
    );
    run_two_rolls(&ctx, &mut app, a, b, vec![button(in_b, true, false)]);
    assert_eq!(app.roll_inst, b);
    app.paste_notes();
    let pasted: Vec<_> = app.song.patterns[0]
        .notes
        .iter()
        .filter(|n| n.instrument == b && n.pitch == 48)
        .collect();
    assert_eq!(pasted.len(), 1, "the note landed in roll B's channel");
}

#[test]
fn a_channels_notes_copy_paste_duplicate_and_clear_from_the_rack() {
    let mut app = app();
    let (from, to) = (app.song.instruments[0].id, app.song.instruments[1].id);
    std::sync::Arc::make_mut(&mut app.song.patterns[0])
        .notes
        .clear();
    for k in 0..3 {
        app.song.add_note(
            1,
            keyframexl::sequence::Note::new(from, k * 5 * STEP, STEP, 40 + k as u8, 100),
        );
    }
    app.copy_channel_notes(from, false);
    app.paste_channel_notes(to);
    let of = |app: &App, id: u64| {
        app.song.patterns[0]
            .notes
            .iter()
            .filter(|n| n.instrument == id)
            .count()
    };
    assert_eq!(of(&app, to), 3);
    app.paste_channel_notes(to);
    assert_eq!(of(&app, to), 3, "paste replaces, it does not stack");
    app.duplicate_channel_notes(to);
    assert_eq!(of(&app, to), 6);
    assert!(
        app.song.patterns[0]
            .notes
            .iter()
            .any(|n| n.instrument == to && n.start == BAR),
        "the copy starts on the next bar"
    );
    app.copy_channel_notes(from, true);
    assert_eq!(of(&app, from), 0, "cut");
    app.clear_channel_notes(to);
    assert_eq!(of(&app, to), 0);
    app.undo();
    assert_eq!(of(&app, to), 6, "one undo step brings them back");
    assert!(app.song.validate().is_ok());
}

#[test]
fn the_middle_button_pans_the_piano_roll() {
    let ctx = egui::Context::default();
    let mut app = app();
    app.roll_inst = 4;
    run(&ctx, &mut app, View::Roll, vec![]);
    let before = app.roll_view[&4];
    let at = pos2(700.0, 400.0);
    let middle = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Middle,
        pressed,
        modifiers: Modifiers::default(),
    };
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(at), middle(true)],
    );
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(at + Vec2::new(0.0, 5.0 * before.row))],
    );
    run(&ctx, &mut app, View::Roll, vec![middle(false)]);
    let after = app.roll_view[&4];
    assert!(
        (after.top - before.top - 5.0).abs() < 0.6,
        "five rows down shows five pitches higher: {} → {}",
        before.top,
        after.top
    );
}

#[test]
fn dragging_the_rolls_ruler_loops_that_span_and_a_double_click_clears_it() {
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let mut app = app();
    app.roll_inst = 4;
    app.set_song_mode(true);
    run(&ctx, &mut app, View::Roll, vec![]);
    // The ruler sits just above the grid.
    let g = app.probe.roll.expect("roll painted");
    let y = g.grid.top() - 8.0;
    let (a, b) = (pos2(g.x(4 * STEP), y), pos2(g.x(8 * STEP), y));
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![
            Event::PointerMoved(a),
            Event::PointerButton {
                pos: a,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(a + (b - a) * 0.5)],
    );
    run(&ctx, &mut app, View::Roll, vec![Event::PointerMoved(b)]);
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerButton {
            pos: b,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );
    assert_eq!(app.pattern().loop_range, Some((4 * STEP, 8 * STEP)));
    assert!(!app.song_mode, "the loop plays in Pattern mode");
    let dbl = |pressed| Event::PointerButton {
        pos: a,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    };
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![dbl(true), dbl(false), dbl(true), dbl(false)],
    );
    run(&ctx, &mut app, View::Roll, vec![]);
    assert_eq!(app.pattern().loop_range, None);
}

#[test]
fn the_rolls_playhead_follows_its_clip_in_the_song() {
    use keyframexl::sequence::Clip;
    let mut song = Song::default();
    let pid = song.patterns[0].id;
    std::sync::Arc::make_mut(&mut song.patterns[0]).length = BAR;
    let mut c = Clip::new(pid, 2 * BAR, 0);
    c.offset = 4 * STEP;
    c.length = Some(3 * BAR);
    song.clips = vec![c];
    let at = super::roll::song_to_pattern_tick;
    assert_eq!(at(&song, pid, BAR), None, "before the clip");
    assert_eq!(
        at(&song, pid, 2 * BAR),
        Some(4 * STEP),
        "the clip starts past its cut"
    );
    assert_eq!(
        at(&song, pid, 2 * BAR + 13 * STEP),
        Some(STEP),
        "the pattern repeats inside a long clip"
    );
    assert_eq!(at(&song, pid, 5 * BAR), None, "after the clip");
    // A clip on a muted playlist track must not hijack the piano roll playhead.
    song.track_mut(0).muted = true;
    assert_eq!(
        at(&song, pid, 2 * BAR),
        None,
        "muted playlist track clips are skipped"
    );
}

#[test]
fn a_plugin_dropped_between_channels_lands_there_and_undoes_in_one_step() {
    let mut app = app();
    let ids: Vec<u64> = app.song.instruments.iter().map(|i| i.id).collect();
    let scanned = crate::plugins::Scanned {
        path: "/nonexistent/Dropped.vst3".into(),
        name: "Dropped".into(),
        vendor: String::new(),
        category: String::new(),
        instrument: true,
        gui: false,
        status: crate::plugins::Status::Ok,
        stamp: 0,
        bridged: false,
    };
    app.insert_dropped_channel(1, |a| a.add_plugin_channel(&scanned));
    assert_eq!(app.song.instruments.len(), ids.len() + 1);
    assert_eq!(app.song.instruments[1].name, "Dropped", "it sits second");
    assert_eq!(app.song.instruments[0].id, ids[0]);
    assert_eq!(app.song.instruments[2].id, ids[1]);
    assert!(app.song.validate().is_ok());
    app.undo();
    let back: Vec<u64> = app.song.instruments.iter().map(|i| i.id).collect();
    assert_eq!(back, ids, "one undo removes it");
}

#[test]
fn the_count_in_lasts_one_bar_of_the_first_meter() {
    let mut app = app();
    app.song.tempo = 120.0;
    app.song.meters.clear();
    let rate = app
        .audio
        .as_ref()
        .map_or(app.settings.audio.rate, |a| a.running.rate) as u64;
    assert_eq!(
        app.count_in_frames(),
        rate * 2,
        "4/4 at 120 BPM: two seconds"
    );
    app.song.meters.push(keyframexl::tempo::Meter {
        bar: 0,
        num: 3,
        den: 4,
    });
    assert_eq!(app.count_in_frames(), rate * 3 / 2, "3/4: three beats");
}

#[test]
fn tide_renders_all_oscillators_and_lower_decks_cleanly() {
    let (ctx, mut app) = setup();
    let patch = keyframexl::synth::SynthPatch::factory(17);
    let id = app.song.add_channel(
        "Tide Test",
        keyframexl::sequence::InstrumentKind::Synth { patch },
    );
    let mut tab = Tab::Instrument { id };
    for (w, h) in [(1040.0_f32, 680.0_f32), (680.0_f32, 540.0_f32)] {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
        });
    }
}

#[test]
fn sampler_renders_empty_and_multizone_pages_cleanly() {
    let (ctx, mut app) = setup();
    let empty_id = app.add_sampler();
    let mut empty_tab = Tab::Instrument { id: empty_id };
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1040.0, 680.0))),
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut empty_tab));
    });

    app.song =
        keyframexl::project_io::open(std::path::Path::new("artifacts/dwp-multizone/Zones.kfxl"))
            .expect("fixture project");
    let id = app.default_roll();
    let mut tab = Tab::Instrument { id };
    for page in 0..5 {
        for zone in [Some(0), None] {
            {
                let st = app.sampler_ui(id);
                st.page = page;
                st.zone = zone;
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1040.0, 680.0))),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
            });
        }
    }
}

#[test]
fn a_new_project_gets_the_built_in_drums_and_kits_add_as_channels() {
    let mut app = app();
    app.settings.general.default_kit = true;
    app.new_project();
    for name in ["Kick", "Snare", "Hat"] {
        let i = app
            .song
            .instruments
            .iter()
            .find(|i| i.name == name)
            .unwrap();
        let keyframexl::sequence::InstrumentKind::Forge { sound } = &i.kind else {
            panic!("{name} is {:?}", i.kind);
        };
        assert!(sound.rendered.peak > 0.3, "{name} sounds");
    }
    assert!(!app.dirty(), "the starter sounds are not an edit");
    let before = app.song.instruments.len();
    app.add_kit(keyframexl::kits::Kit::Chip);
    assert_eq!(app.song.instruments.len(), before + 8);
    assert!(app.song.instruments.iter().any(|i| i.name == "Chip Kick"));
    app.undo();
    assert_eq!(app.song.instruments.len(), before);
}

/// A tab group dragged by its grip (the left edge of its tab bar) over another
/// panel: egui_dock hit `unreachable!` on the hover check and closed the app.
#[test]
fn dragging_a_tab_group_by_its_grip_does_not_crash() {
    struct Viewer;
    impl egui_dock::TabViewer for Viewer {
        type Tab = u8;
        fn title(&mut self, tab: &mut u8) -> egui::WidgetText {
            format!("Tab {tab}").into()
        }
        fn ui(&mut self, ui: &mut egui::Ui, tab: &mut u8) {
            ui.label(format!("{tab}"));
        }
    }
    let mut dock = egui_dock::DockState::new(vec![1u8, 2]);
    dock.main_surface_mut()
        .split_right(egui_dock::NodeIndex::root(), 0.5, vec![3]);
    let ctx = egui::Context::default();
    let frame = |dock: &mut egui_dock::DockState<u8>, events: Vec<Event>| {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // As the app shows its dock: no collapse buttons left of the grip.
                egui_dock::DockArea::new(dock)
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(ui, &mut Viewer);
            });
        });
    };
    frame(&mut dock, vec![]);
    // The grip sits at the left of the first leaf's tab bar.
    let leaf = dock.main_surface()[egui_dock::NodeIndex::root().left()]
        .rect()
        .expect("laid out");
    let grip = leaf.left_top() + vec2(9.0, 12.0);
    let over = pos2(600.0, 300.0);
    frame(
        &mut dock,
        vec![Event::PointerMoved(grip), button(grip, true, true)],
    );
    for k in 1..=10 {
        let p = grip + (over - grip) * (k as f32 / 10.0);
        frame(&mut dock, vec![Event::PointerMoved(p)]);
    }
    frame(&mut dock, vec![button(over, true, false)]);
    frame(&mut dock, vec![]);
    let tabs: usize = dock.iter_all_tabs().count();
    assert_eq!(tabs, 3, "no tab lost");
}

#[test]
fn window_geometry_recording_handles_wayland_fullscreen_and_startup_maximize() {
    let (ctx, mut app) = setup();
    app.settings.general.window_size = Some([1440.0, 900.0]);
    app.settings.general.window_maximized = true;
    app.startup_maximized = true;

    let step = |app: &mut App,
                size: [f32; 2],
                with_inner_rect: bool,
                maximized: Option<bool>,
                fullscreen: Option<bool>,
                z: f32,
                time: f64| {
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0], size[1]))),
            time: Some(time),
            ..Default::default()
        };
        let vp = raw.viewports.entry(egui::ViewportId::ROOT).or_default();
        vp.inner_rect = with_inner_rect
            .then(|| Rect::from_min_size(pos2(10.0, 20.0), Vec2::new(size[0], size[1])));
        vp.maximized = maximized;
        vp.fullscreen = fullscreen;
        let mut out = (false, false, 0.0);
        let full = ctx.run(raw, |ctx| {
            out = app.record_window_geometry(ctx, z);
        });
        (
            out,
            full.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .clone(),
        )
    };

    // Frame 1: WM hasn't mapped/maximized yet -> must not clobber startup maximize.
    let ((dirty, _, _), cmds) = step(
        &mut app,
        [1440.0, 900.0],
        true,
        Some(false),
        None,
        1.0,
        0.01,
    );
    assert!(!dirty);
    assert!(app.settings.general.window_maximized);
    assert!(!cmds.contains(&egui::ViewportCommand::Maximized(true)));

    // Frame 2: still reporting unmaximized after mapping -> nudges maximize once.
    let ((dirty, _, _), cmds) = step(
        &mut app,
        [1440.0, 900.0],
        true,
        Some(false),
        None,
        1.0,
        0.03,
    );
    assert!(!dirty);
    assert!(app.settings.general.window_maximized);
    assert!(cmds.contains(&egui::ViewportCommand::Maximized(true)));

    // Frame 3: WM confirms maximize -> clears startup flag and preserves normal window_size.
    let ((dirty, _, _), _) = step(
        &mut app,
        [2560.0, 1440.0],
        true,
        Some(true),
        None,
        1.0,
        0.05,
    );
    assert!(!dirty);
    assert!(!app.startup_maximized);
    assert!(app.settings.general.window_maximized);
    assert_eq!(app.settings.general.window_size, Some([1440.0, 900.0]));

    // User restores window on Wayland (inner_rect == None) with ui_scale 1.25 -> records logical size.
    let ((dirty, _, _), _) = step(
        &mut app,
        [1280.0, 720.0],
        false,
        Some(false),
        Some(false),
        1.25,
        0.2,
    );
    assert!(dirty);
    assert!(!app.settings.general.window_maximized);
    assert_eq!(app.settings.general.window_size, Some([1600.0, 900.0]));

    // Fullscreen does not overwrite the restored window size.
    let ((dirty, _, _), _) = step(
        &mut app,
        [2560.0, 1440.0],
        true,
        Some(false),
        Some(true),
        1.0,
        0.4,
    );
    assert!(!dirty);
    assert_eq!(app.settings.general.window_size, Some([1600.0, 900.0]));
}

/// Drags the right edge of window `id`, then its bottom edge, and checks the
/// other axis never moves (the height used to creep on width drags).
fn edge_drag_keeps_other_axis(id: &str, mut show: impl FnMut(&egui::Context)) {
    let ctx = egui::Context::default();
    ctx.set_fonts(t::font_definitions(None));
    t::apply(&ctx, t::Theme::Dark);
    let win = egui::Id::new(id);
    let mut time = 0.0;
    let mut step = |events: Vec<Event>| {
        time += 1.0 / 60.0;
        let raw = egui::RawInput {
            time: Some(time),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(raw, &mut show);
        ctx.memory(|m| m.area_rect(win)).expect("window shown")
    };
    let mut rect = Rect::NOTHING;
    for _ in 0..4 {
        rect = step(vec![]);
    }
    let h = rect.height();
    let a = pos2(rect.right(), rect.center().y);
    step(vec![Event::PointerMoved(a)]);
    step(vec![button(a, true, true)]);
    for i in 1..=12 {
        rect = step(vec![Event::PointerMoved(a + vec2(i as f32 * 10.0, 0.0))]);
        assert!(
            (rect.height() - h).abs() <= 0.5,
            "height {h} → {}",
            rect.height()
        );
    }
    step(vec![button(a + vec2(120.0, 0.0), true, false)]);
    for _ in 0..3 {
        rect = step(vec![]);
    }
    assert!(
        (rect.height() - h).abs() <= 0.5,
        "height {h} → {}",
        rect.height()
    );
    let w = rect.width();
    let b = pos2(rect.center().x, rect.bottom());
    step(vec![Event::PointerMoved(b)]);
    step(vec![button(b, true, true)]);
    for i in 1..=10 {
        rect = step(vec![Event::PointerMoved(b - vec2(0.0, i as f32 * 8.0))]);
        assert!(
            (rect.width() - w).abs() <= 0.5,
            "width {w} → {}",
            rect.width()
        );
    }
    assert!(rect.height() < h - 40.0, "bottom drag shortens the window");
}

#[test]
fn export_window_edge_drag_keeps_other_axis() {
    let mut app = App::with_song(keyframexl::demo::build(None), None);
    app.open_export(std::path::PathBuf::from("/tmp/mix.wav"), true);
    edge_drag_keeps_other_axis("export-window", |ctx| app.export_window(ctx));
    assert!(app.export.is_some(), "the drag does not close the dialog");
}

#[test]
fn export_file_name_edit_keeps_folder_and_extension() {
    use std::path::Path;
    let p = Path::new("/music/beats/loop.wav");
    assert_eq!(
        super::export::renamed(p, "night drive"),
        Path::new("/music/beats/night drive.wav")
    );
    assert_eq!(
        super::export::renamed(p, "a/b"),
        Path::new("/music/beats/ab.wav")
    );
    assert_eq!(super::export::renamed(p, "  "), p);
}

#[test]
fn export_allows_clearing_file_name_and_errors_only_on_start() {
    let mut app = App::with_song(Song::default(), None);
    app.open_export(std::path::PathBuf::from("/music/beats/loop.wav"), false);
    {
        let d = app.export.as_mut().unwrap();
        assert_eq!(d.file_stem, "loop");
        d.file_stem.clear();
    }
    app.start_export();
    let d = app.export.as_ref().unwrap();
    assert_eq!(d.file_stem, "");
    assert_eq!(d.error.as_deref(), Some(app.tr().exp_name_empty));
    assert!(d.stage.is_empty());
}

#[test]
fn manual_window_edge_drag_keeps_other_axis() {
    let mut app = App::with_song(Song::default(), None);
    app.show_manual = true;
    edge_drag_keeps_other_axis("kfxl-manual-window", |ctx| app.manual_window(ctx));
}

#[test]
fn tap_window_takes_space_from_play_and_counts_it() {
    let (ctx, mut app) = setup();
    app.tap();
    assert_eq!(app.tap_win.as_ref().map(|w| w.meter.tap_count()), Some(1));
    // A second tap within 80 ms is a bounce, so start the count afresh.
    app.tap_win.as_mut().unwrap().meter.reset();
    let space = |pressed| Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    let mut leaked = true;
    let input = egui::RawInput {
        events: vec![space(true), Event::Text(" ".into()), space(false)],
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        app.tap_keys(ctx);
        leaked = ctx.input(|i| i.key_pressed(egui::Key::Space));
    });
    assert!(!leaked, "Space must not reach play / stop");
    assert_eq!(app.tap_win.as_ref().map(|w| w.meter.tap_count()), Some(1));
    // Escape closes the window.
    let esc = egui::RawInput {
        events: vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        }],
        ..Default::default()
    };
    let _ = ctx.run(esc, |ctx| app.tap_keys(ctx));
    assert!(app.tap_win.is_none());
}

#[test]
fn midi_notes_tap_on_the_device_clock_and_set_the_tempo() {
    use crate::midi_in::{Event as Midi, Incoming};
    let mut app = app();
    app.tap();
    app.tap_win.as_mut().unwrap().meter.reset();
    let t0 = std::time::Instant::now();
    // 125 BPM on the driver's clock (0.48 s), while the UI saw them jittered.
    let events: Vec<Incoming> = (0..5u64)
        .flat_map(|k| {
            let note = Incoming {
                port: "Keys".into(),
                stamp_us: 1_000_000 + k * 480_000,
                received_at: t0 + std::time::Duration::from_millis(k * 480 + (k % 2) * 30),
                event: Midi::On {
                    pitch: 60,
                    velocity: 100,
                    frame: 0,
                },
            };
            let off = Incoming {
                event: Midi::Off {
                    pitch: 60,
                    frame: 0,
                },
                ..note.clone()
            };
            [note, off]
        })
        .collect();
    app.tap_midi(&events);
    let w = app.tap_win.as_ref().unwrap();
    assert_eq!(w.meter.tap_count(), 5, "note-offs do not tap");
    assert_eq!(w.meter.bpm(), Some(125.0));
    assert_eq!(app.edited_tempo(), 125.0, "applied while tapping");
    // The whole run is one undo step.
    app.undo();
    assert_ne!(app.edited_tempo(), 125.0);
}

#[test]
fn a_closed_panel_opens_in_the_focused_window_or_its_own() {
    let mut app = app();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
    // Arrange: Playlist above, Channel rack below. Close the rack and work
    // in the playlist.
    app.win(Cmd::Close(Tab::Rack));
    app.win(Cmd::Show(Tab::Playlist));
    app.apply_win_cmds(area);
    let playlist = app.ws.dock().find_tab(&Tab::Playlist).unwrap();
    // F6 brings the rack back as a tab beside the focused playlist, not
    // into the first pane that happens to be open.
    app.win(Cmd::Toggle(Tab::Rack));
    app.apply_win_cmds(area);
    let rack = app.ws.dock().find_tab(&Tab::Rack).expect("rack opened");
    assert_eq!((rack.0, rack.1), (playlist.0, playlist.1));
    assert_eq!(workspace::focused(app.ws.dock()), Some(Tab::Rack));
    // With "In a new window" it gets a window of its own.
    app.settings.general.panel_open = crate::settings::PanelOpen::Window;
    app.win(Cmd::Close(Tab::Rack));
    app.apply_win_cmds(area);
    app.win(Cmd::Toggle(Tab::Rack));
    app.apply_win_cmds(area);
    let (s, _, _) = app.ws.dock().find_tab(&Tab::Rack).expect("rack opened");
    assert!(!s.is_main(), "a floating window");
}

#[test]
fn midi_keys_highlight_on_piano_roll_keyboard_and_clear_on_release() {
    use crate::midi_in::Event as Midi;
    let mut app = app();
    let inst = app.midi_target();
    app.roll_inst = inst;
    app.focus = Some(Tab::Roll { instrument: inst });

    let ctx = egui::Context::default();
    let mut target = inst;
    let count_brand_rects = |out: &egui::FullOutput, roll: roll::Geom| -> usize {
        fn walk(shape: &egui::epaint::Shape, brand: egui::Color32, max_x: f32) -> usize {
            match shape {
                egui::epaint::Shape::Rect(r)
                    if r.fill == brand && r.rect.right() <= max_x + 0.5 =>
                {
                    1
                }
                egui::epaint::Shape::Vec(v) => v.iter().map(|s| walk(s, brand, max_x)).sum(),
                _ => 0,
            }
        }
        let brand = crate::theme::brand();
        out.shapes
            .iter()
            .map(|cs| walk(&cs.shape, brand, roll.grid.left()))
            .sum()
    };

    // Baseline: no MIDI keys held -> no lit brand-colored key on the keyboard strip.
    let out_idle = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.roll(ui, &mut target));
    });
    let g = app.probe.roll.expect("roll geometry");
    assert_eq!(count_brand_rects(&out_idle, g), 0);

    // Hold a white key (E5 = 76) and a black key (F#5 = 78) within view.
    let white = (g.bottom..=g.top).find(|&p| p % 12 == 4).unwrap();
    let black = (g.bottom..=g.top).find(|&p| p % 12 == 6).unwrap();
    app.midi_keys[white as usize] = true;
    app.midi_keys[black as usize] = true;
    let out_lit = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.roll(ui, &mut target));
    });
    assert_eq!(count_brand_rects(&out_lit, g), 2);

    // MIDI All Notes Off (CC 123) clears stuck keys.
    app.midi_keys[white as usize] = true;
    app.midi_keys[black as usize] = true;
    app.on_midi(
        inst,
        Midi::Control {
            channel: 1,
            cc: 123,
            value: 0,
        },
        false,
    );
    assert!(!app.midi_keys[white as usize]);
    assert!(!app.midi_keys[black as usize]);
    let out_off = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.roll(ui, &mut target));
    });
    assert_eq!(count_brand_rects(&out_off, g), 0);
}

#[test]
fn audio_clip_adds_to_playlist_and_renders_waveform_cleanly() {
    let mut app = app();
    let preset =
        keyframexl::sequence::SamplePreset::from_sample("Song", 48_000, vec![[0.6, -0.4]; 48_000]);
    let (inst_id, pat_id, clip_idx) = app.song.add_audio_clip("Song", preset, 0, 1).unwrap();
    app.select_pattern(pat_id);
    app.clip = Some(clip_idx);
    app.song.validate().unwrap();
    assert_eq!(app.song.pattern(pat_id).unwrap().audio_clip, Some(inst_id));

    // Playlist renders the audio clip waveform without panicking and records its drop zone.
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.playlist(ui));
    });
    assert!(app.playlist_drop.is_some());
}

#[test]
fn floating_window_blocks_rack_pad_click_through() {
    let (ctx, mut app) = setup();
    // Initial frame records pad 0's rect.
    run(&ctx, &mut app, View::Rack, vec![]);
    let pad = app.probe.pad.expect("pad rect");
    let before = app.song.patterns[0].notes.len();

    // Draw a floating egui::Window covering pad 0, then click right on pad 0.
    let center = pad.center();
    let frame_with_overlay = |ctx: &egui::Context, app: &mut App, events: Vec<Event>| {
        let mut tab = Tab::Rack;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
            egui::Window::new("Floating Sampler")
                .fixed_pos(pad.min - Vec2::new(20.0, 20.0))
                .fixed_size(Vec2::new(260.0, 200.0))
                .show(ctx, |ui| {
                    ui.label("Sampler controls");
                });
        });
    };
    frame_with_overlay(&ctx, &mut app, vec![Event::PointerMoved(center)]);
    frame_with_overlay(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(center), button(center, true, true)],
    );
    frame_with_overlay(&ctx, &mut app, vec![button(center, true, false)]);
    assert_eq!(
        app.song.patterns[0].notes.len(),
        before,
        "clicking a floating window must not click through to Channel Rack pads"
    );
}

#[test]
fn roll_velocity_lane_edits_unselected_note_and_wheel_over_note_changes_velocity() {
    let (ctx, mut app) = setup();
    let id1 = app
        .song
        .add_note(1, keyframexl::sequence::Note::new(4, 0, STEP * 2, 52, 100));
    let id2 = app.song.add_note(
        1,
        keyframexl::sequence::Note::new(4, STEP * 4, STEP * 2, 52, 80),
    );
    // Keep note 1 selected (just like after placing a note).
    app.roll_sel = vec![id1];
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.expect("roll geom");

    // 1. Click note 2's velocity stem in the event lane even though note 1 is selected!
    let stem_x = g.x(STEP * 4) + 3.0;
    let stem_y = g.lane.top() + 18.0;
    click(&ctx, &mut app, View::Roll, pos2(stem_x, stem_y));
    let v2_after_lane = app
        .pattern()
        .notes
        .iter()
        .find(|n| n.id == id2)
        .unwrap()
        .velocity;
    assert!(
        v2_after_lane > 95,
        "event lane should edit unselected note 2's velocity, got {v2_after_lane}"
    );

    // 2. Hover over note 2 and scroll mouse wheel -> changes note 2's velocity without scrolling pitch!
    let top_before = app.roll_view.get(&4).unwrap().top;
    let note2_pos = g.point(4.5, 52);
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![
            Event::PointerMoved(note2_pos),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -40.0),
                modifiers: Modifiers::default(),
            },
        ],
    );
    let v2_after_wheel = app
        .pattern()
        .notes
        .iter()
        .find(|n| n.id == id2)
        .unwrap()
        .velocity;
    let top_after = app.roll_view.get(&4).unwrap().top;
    assert!(
        v2_after_wheel < v2_after_lane,
        "wheel down over note should lower its velocity ({v2_after_lane} -> {v2_after_wheel})"
    );
    assert!(
        (top_after - top_before).abs() < 1e-4,
        "scrolling over a note must not scroll the piano roll viewport"
    );
}

#[test]
fn roll_double_click_opens_note_properties_and_paint_tool_paints_freely() {
    let (ctx, mut app) = setup();
    let id = app
        .song
        .add_note(1, keyframexl::sequence::Note::new(4, 0, STEP * 2, 52, 100));
    run(&ctx, &mut app, View::Roll, vec![]);
    let g = app.probe.roll.expect("roll geom");
    let p = g.point(0.5, 52);
    // Double-click the note -> opens Note Properties dialog.
    click(&ctx, &mut app, View::Roll, p);
    click(&ctx, &mut app, View::Roll, p);
    assert_eq!(app.note_props.map(|d| d.note_id), Some(id));
    // Render the dialog cleanly without panicking, then return to the roll.
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.note_props_window(ctx));
    app.note_props = None;
    run(&ctx, &mut app, View::Roll, vec![]);

    // Brush tool (Tool::Paint) paints both leftward and across pitches!
    app.tool = Tool::Paint;
    app.last_len = Some(STEP);
    let start = g.point(6.5, 54);
    let mid = g.point(4.5, 54);
    let end = g.point(4.5, 56);
    run(
        &ctx,
        &mut app,
        View::Roll,
        vec![Event::PointerMoved(start), button(start, true, true)],
    );
    run(&ctx, &mut app, View::Roll, vec![Event::PointerMoved(mid)]);
    run(&ctx, &mut app, View::Roll, vec![Event::PointerMoved(end)]);
    run(&ctx, &mut app, View::Roll, vec![button(end, true, false)]);
    let painted: Vec<(i64, u8)> = app
        .pattern()
        .notes
        .iter()
        .filter(|n| n.id != id)
        .map(|n| (n.start, n.pitch))
        .collect();
    assert!(
        painted.contains(&(6 * STEP, 54)) && painted.contains(&(4 * STEP, 54)),
        "brush tool should paint leftward: {painted:?}"
    );
    assert!(
        painted.contains(&(4 * STEP, 56)),
        "brush tool should paint across pitches: {painted:?}"
    );
}

#[test]
fn selecting_instrument_sets_lead_for_midi_and_piano_roll() {
    let mut app = app();
    let ch = app.song.instruments[1].id;
    assert_ne!(app.instrument, ch);
    app.set_lead_channel(ch);
    assert_eq!(app.instrument, ch);
    assert_eq!(app.rack_sel, vec![ch]);
    assert_eq!(app.midi_target(), ch);

    // Focusing another channel's Instrument window routes MIDI straight to it.
    let ch2 = app.song.instruments[2].id;
    app.focus = Some(Tab::Instrument { id: ch2 });
    assert_eq!(app.midi_target(), ch2);
}

#[test]
fn playlist_remembers_resized_clip_length_when_placing() {
    let mut app = app();
    app.select_pattern(1);
    app.last_clip = Some((1, STEP * 4, Some(STEP * 8)));
    let placed = app.make_playlist_clip(BAR, 2);
    assert_eq!(placed.pattern, 1);
    assert_eq!(placed.offset, STEP * 4);
    assert_eq!(placed.length, Some(STEP * 8));

    // Selecting a different pattern from the pattern selector resets to default length.
    app.new_pattern();
    let fresh = app.make_playlist_clip(0, 0);
    assert_eq!(fresh.offset, 0);
    assert_eq!(fresh.length, None);
}

/// Every Settings page fits the Settings window at its minimum size (920 px
/// less the tab body's margins) in every language: nothing is cut off on the
/// right, and the footer's Close button stays in view.
#[test]
fn settings_pages_fit_the_minimum_window() {
    use crate::ui::settings_ui::Page;
    let body = 920.0 - 30.0;
    for lang in crate::i18n::Language::ALL {
        for page in [Page::Audio, Page::Midi, Page::Plugins, Page::General] {
            let (ctx, mut app) = setup();
            app.settings.general.language = lang;
            app.open_settings(page);
            let mut used = 0.0;
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(body, 520.0))),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            app.settings_tab(ui);
                            used = ui.min_rect().width();
                        });
                });
            }
            assert!(used <= body + 0.5, "{lang:?} {page:?} needs {used} px");
        }
    }
}

/// The window toolbar on a floating window hidden under another: the button
/// brings it to the front (it used to only take focus, or close it when it had
/// focus already); pressed again on the front window, it closes it.
#[test]
fn toolbar_button_raises_a_covered_floating_window() {
    let (ctx, mut app) = setup();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0));
    let frame = |app: &mut App| {
        let input = egui::RawInput {
            screen_rect: Some(area),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let a = app.ws.active;
                let mut dock =
                    std::mem::replace(&mut app.ws.slots[a], egui_dock::DockState::new(vec![]));
                egui_dock::DockArea::new(&mut dock)
                    .id(egui::Id::new(("kfxl-dock", a)))
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .window_bounds(area)
                    .show_inside(ui, &mut workspace::Viewer { app: &mut *app });
                app.ws.slots[a] = dock;
            });
            app.apply_win_cmds(area);
        });
    };
    app.win(Cmd::Float(Tab::Mixer));
    app.apply_win_cmds(area);
    frame(&mut app);
    app.win(Cmd::Float(Tab::Playlist));
    app.apply_win_cmds(area);
    for _ in 0..2 {
        frame(&mut app);
    }
    let rect_of = |app: &mut App, tab: &Tab| {
        let (s, _, _) = app.ws.dock().find_tab(tab).unwrap();
        app.ws.dock().get_window_state_mut(s).unwrap().rect()
    };
    let overlap = rect_of(&mut app, &Tab::Mixer).intersect(rect_of(&mut app, &Tab::Playlist));
    assert!(overlap.is_positive());
    let playlist_layer = ctx.layer_id_at(overlap.center()).unwrap();
    // Mixer has the focus but sits under the playlist: the button raises it.
    let (ms, mn, mt) = app.ws.dock().find_tab(&Tab::Mixer).unwrap();
    app.ws.dock().set_active_tab((ms, mn, mt));
    app.ws.dock().set_focused_node_and_surface((ms, mn));
    app.win(Cmd::Toggle(Tab::Mixer));
    frame(&mut app);
    frame(&mut app);
    assert!(app.ws.dock().find_tab(&Tab::Mixer).is_some(), "not closed");
    assert_ne!(
        ctx.layer_id_at(overlap.center()),
        Some(playlist_layer),
        "mixer in front"
    );
    assert_eq!(app.ws.dock().top_window(), Some(ms));
    // In front and focused: the button closes it.
    app.win(Cmd::Toggle(Tab::Mixer));
    frame(&mut app);
    assert!(app.ws.dock().find_tab(&Tab::Mixer).is_none(), "closed");
    // Showing the playlist again keeps it in front.
    app.win(Cmd::Show(Tab::Playlist));
    frame(&mut app);
    assert_eq!(ctx.layer_id_at(overlap.center()), Some(playlist_layer));
}

/// A docked panel under a floating window that fills the work area (the
/// owner's case: a maximized Piano roll over a docked Channel rack). The
/// panel's toolbar button folds the window away instead of doing nothing
/// (or closing the hidden panel); the window's own button opens it again.
#[test]
fn toolbar_button_uncovers_a_docked_panel_under_a_floating_window() {
    let (ctx, mut app) = setup();
    let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0));
    let frame = |app: &mut App| {
        let input = egui::RawInput {
            screen_rect: Some(area),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let a = app.ws.active;
                let mut dock =
                    std::mem::replace(&mut app.ws.slots[a], egui_dock::DockState::new(vec![]));
                egui_dock::DockArea::new(&mut dock)
                    .id(egui::Id::new(("kfxl-dock", a)))
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .window_bounds(area)
                    .show_inside(ui, &mut workspace::Viewer { app: &mut *app });
                app.ws.slots[a] = dock;
            });
            app.apply_win_cmds(area);
        });
    };
    let roll = Tab::Roll { instrument: 4 };
    assert!(
        app.ws.dock().find_tab(&Tab::Rack).unwrap().0.is_main(),
        "rack docked"
    );
    app.win(Cmd::Float(roll.clone()));
    app.apply_win_cmds(area);
    let (rs, _, _) = app.ws.dock().find_tab(&roll).unwrap();
    app.ws
        .dock()
        .get_window_state_mut(rs)
        .unwrap()
        .set_snap(Some(egui_dock::Snap::Max));
    for _ in 0..2 {
        frame(&mut app);
    }
    // The rack has the focus but the maximized roll hides it.
    let (s, n, t) = app.ws.dock().find_tab(&Tab::Rack).unwrap();
    app.ws.dock().set_active_tab((s, n, t));
    app.ws.dock().set_focused_node_and_surface((s, n));
    app.win(Cmd::Toggle(Tab::Rack));
    frame(&mut app);
    assert!(
        app.ws.dock().find_tab(&Tab::Rack).is_some(),
        "rack not closed"
    );
    let folded = |app: &mut App| {
        let (rs, _, _) = app.ws.dock().find_tab(&roll).unwrap();
        app.ws
            .dock()
            .get_window_state_mut(rs)
            .unwrap()
            .is_minimized()
    };
    assert!(folded(&mut app), "the roll folds away");
    // The roll's button opens it again, in front.
    app.win(Cmd::Toggle(roll.clone()));
    frame(&mut app);
    assert!(!folded(&mut app), "roll open again");
    assert!(app.ws.dock().find_tab(&roll).is_some());
}

/// Reordering mixer effects: the slot moves, the ones between shift, and its
/// automation, open window and selection follow it.
#[test]
fn moving_an_effect_slot_takes_its_automation_and_window_along() {
    use keyframexl::automation::{Envelope, Playback, Target};
    use keyframexl::fx::{FxKind, FxSlot};
    let mut app = app();
    app.song.mixer[1].fx = vec![
        FxSlot::new(FxKind::Delay),
        FxSlot::new(FxKind::Reverb),
        FxSlot::new(FxKind::Chorus),
    ];
    std::sync::Arc::make_mut(&mut app.song.patterns[0]).automation = Some(Envelope {
        target: Target::FxMix { track: 1, slot: 0 },
        points: vec![],
        playback: Playback::default(),
    });
    app.fx_windows = vec![(1, 0), (1, 2)];
    app.mix_sel = 1;
    app.mix_slot = 0;
    app.move_fx_slot(1, 0, 2);
    let kinds: Vec<FxKind> = app.song.mixer[1].fx.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![FxKind::Reverb, FxKind::Chorus, FxKind::Delay]);
    assert_eq!(
        app.song.patterns[0].automation.as_ref().unwrap().target,
        Target::FxMix { track: 1, slot: 2 },
        "the delay's automation follows it"
    );
    assert_eq!(
        app.fx_windows,
        vec![(1, 2), (1, 1)],
        "windows follow their slots"
    );
    assert_eq!(app.mix_slot, 2, "the selection follows the moved slot");
    assert!(app.status.1.contains("slot 3"), "{}", app.status.1);
    // One undo step puts it back.
    app.undo();
    let kinds: Vec<FxKind> = app.song.mixer[1].fx.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![FxKind::Delay, FxKind::Reverb, FxKind::Chorus]);
}
