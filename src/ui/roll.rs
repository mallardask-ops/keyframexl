//! Piano roll (FL canon): keyboard, ruler with playhead, note grid, event lane.
//!
//! Draw tool: pressing on an empty cell places a note of the last used length
//! and the drag moves it; drag a note to move it (the whole selection when it
//! is selected), its edges to resize; Shift+drag duplicates; Ctrl+drag selects
//! with a box; right-click (and right-drag) erases. Alt skips snap. Wheel
//! scrolls pitch, Shift+wheel scrolls time, Ctrl+wheel zooms time at the
//! pointer, Alt+wheel changes the row height. The pattern grows when notes
//! go past its end.

use super::{App, Slot, Snap, Tab, Tone, Tool, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::sequence::{BAR, Glide, MAX_BARS, Note, STEP, Tick, steps};

const KEYS_W: f32 = 48.0;
const RULER_H: f32 = 24.0;
const LANE_H: f32 = 88.0;
/// The shortest open event lane, and the splitter above it.
const LANE_MIN: f32 = 36.0;
const SPLIT_H: f32 = 10.0;
/// Full pitch range of the roll, C1..C7. The window shows a scrolled slice of it.
const LOWEST: u8 = 24;
const HIGHEST: u8 = 96;
pub const ROW_MIN: f32 = 8.0;
pub const ROW_MAX: f32 = 30.0;
/// Pixels per step, zoom range.
const CELL_MIN: f32 = 4.0;
const CELL_MAX: f32 = 160.0;
const EDGE_GRAB: f32 = 7.0;

/// Number of note colour groups (FL: note colour). The colour is data on the note.
const NOTE_COLOR_COUNT: usize = 16;

/// Top, fill and edge of a note in colour group `c`.
pub(crate) fn note_colors(c: u8) -> (egui::Color32, egui::Color32, egui::Color32) {
    let base = match c as usize % NOTE_COLOR_COUNT {
        0 => return (t::brand_top(), t::brand_fill(), t::brand_edge()),
        1 => t::enc_blue(),
        2 => t::enc_green(),
        3 => t::wait(),
        4 => t::enc_orange(),
        5 => t::enc_white(),
        6 => t::HUE_PINK,
        7 => t::HUE_PURPLE,
        8 => t::HUE_CYAN,
        9 => t::HUE_LIME,
        10 => t::HUE_GOLD,
        11 => t::HUE_BROWN,
        12 => t::HUE_ROSE,
        13 => t::HUE_TEAL,
        14 => t::HUE_INDIGO,
        _ => t::HUE_GREY,
    };
    (
        base,
        kit::mix(base, egui::Color32::BLACK, 0.12),
        kit::mix(base, egui::Color32::BLACK, 0.6),
    )
}

pub fn note_name(pitch: u8) -> String {
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", N[(pitch % 12) as usize], pitch as i16 / 12 - 1)
}

/// FL-style position: bar.beat.step (1-based), with leftover ticks.
pub fn bar_pos(t: Tick) -> String {
    let step = t / STEP;
    let rest = t % STEP;
    let base = format!("{}.{}.{}", step / 16 + 1, step % 16 / 4 + 1, step % 4 + 1);
    if rest == 0 {
        base
    } else {
        format!("{base} +{rest}")
    }
}

fn is_black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Scroll and zoom of one piano roll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RollView {
    /// Highest visible pitch (fractional while scrolling).
    pub top: f32,
    /// Row height in pixels.
    pub row: f32,
    /// First visible tick.
    pub x0: f32,
    /// Pixels per step; 0 = fit the pattern on first draw.
    pub cell: f32,
}

/// What the event lane under the grid shows and edits (FL: Target control).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum LaneTarget {
    #[default]
    Velocity,
    Pan,
    Release,
    Fine,
    ModX,
    ModY,
    /// Vocal notes' drawn pitch curves (not a note value: `Pattern::pitch`).
    Pitch,
}

impl LaneTarget {
    pub const ALL: [LaneTarget; 7] = [
        Self::Velocity,
        Self::Pan,
        Self::Release,
        Self::Fine,
        Self::ModX,
        Self::ModY,
        Self::Pitch,
    ];
    /// Centred targets draw stems from the middle.
    fn centred(self) -> bool {
        matches!(self, Self::Pan | Self::Fine)
    }
    /// Value as 0..1.
    pub fn get(self, n: &Note) -> f32 {
        match self {
            Self::Velocity => n.velocity as f32 / 127.0,
            Self::Pan => (n.pan + 1.0) / 2.0,
            Self::Release => n.release,
            Self::Fine => (n.fine + 100.0) / 200.0,
            Self::ModX => n.mod_x,
            Self::ModY => n.mod_y,
            Self::Pitch => 0.5,
        }
    }
    pub fn set(self, n: &mut Note, v: f32) {
        let v = v.clamp(0.0, 1.0);
        match self {
            Self::Velocity => n.velocity = ((v * 127.0).round() as u8).max(1),
            Self::Pan => n.pan = v * 2.0 - 1.0,
            Self::Release => n.release = v,
            Self::Fine => n.fine = v * 200.0 - 100.0,
            Self::ModX => n.mod_x = v,
            Self::ModY => n.mod_y = v,
            Self::Pitch => {}
        }
    }
    fn default_value(self) -> f32 {
        match self {
            Self::Velocity => 100.0 / 127.0,
            _ => 0.5,
        }
    }
    fn text(self, n: &Note) -> String {
        match self {
            Self::Velocity => format!("{}", n.velocity),
            Self::Pan if n.pan.abs() < 0.01 => "C".into(),
            Self::Pan => format!(
                "{}{:.0}",
                if n.pan < 0.0 { "L" } else { "R" },
                n.pan.abs() * 100.0
            ),
            Self::Release => format!("{:.0}%", n.release * 100.0),
            Self::Fine => format!("{:+.0} ct", n.fine),
            Self::ModX => format!("{:.0}%", n.mod_x * 100.0),
            Self::ModY => format!("{:.0}%", n.mod_y * 100.0),
            Self::Pitch => String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MoveMode {
    Move,
    ResizeRight,
    ResizeLeft,
}

/// A pointer gesture in the grid, from press to release.
#[derive(Clone, Debug)]
pub(crate) enum Gesture {
    /// Move / resize notes; `orig` holds them as they were at the press.
    Edit {
        mode: MoveMode,
        anchor: u64,
        at: Tick,
        pitch: u8,
        orig: Vec<Note>,
        duplicate: bool,
    },
    /// Box selection (Ctrl+drag or Select tool), anchored in musical coordinates
    /// so scrolling while selecting never loses off-screen notes.
    Select {
        from_tick: Tick,
        from_pitch: f32,
        add: bool,
        base: Vec<u64>,
    },
    /// Right button held: erase what the pointer touches.
    Erase,
    /// Slide tool: drag stretches a new slide.
    Slide { at: Tick, pitch: u8 },
    /// Move or resize an existing slide (Draw or Slide tool on a slide).
    SlideEdit {
        index: usize,
        mode: MoveMode,
        at: Tick,
        pitch: u8,
        orig: Glide,
    },
    /// Paint tool: notes of `len` follow the pointer from `next` on.
    Paint { len: Tick, next: Tick },
    /// Mute tool: notes the pointer touches become `to`.
    Mute { to: bool },
    /// Slice tool: a cut line from the press point.
    Slice { from: Pos2 },
    /// Zoom tool: a span to zoom to.
    Zoom { from: Pos2 },
}

/// Grid geometry, shared by painting, hit-testing and tests.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geom {
    pub grid: Rect,
    /// Pixels per step.
    pub cell: f32,
    pub row: f32,
    pub lane: Rect,
    /// Highest and lowest visible pitch.
    pub top: u8,
    pub bottom: u8,
    /// Pattern length in ticks.
    pub len: Tick,
    /// First visible tick.
    pub x0: Tick,
}
impl Geom {
    pub fn x(&self, t: Tick) -> f32 {
        self.grid.left() + steps(t - self.x0) as f32 * self.cell
    }
    pub fn y(&self, pitch: u8) -> f32 {
        self.grid.top() + (self.top as f32 - pitch as f32) * self.row
    }
    /// Screen y for a continuous pitch coordinate (where `pitch` occupies `[pitch, pitch+1]`).
    pub fn y_f32(&self, pitch_f: f32) -> f32 {
        self.grid.top() + (self.top as f32 + 1.0 - pitch_f) * self.row
    }
    /// Continuous pitch coordinate under screen y.
    pub fn pitch_f32(&self, y: f32) -> f32 {
        (self.top as f32 + 1.0 - (y - self.grid.top()) / self.row)
            .clamp(LOWEST as f32, HIGHEST as f32 + 1.0)
    }
    /// Unsnapped tick under screen x, clamped to valid song ticks.
    pub fn tick_f(&self, x: f32) -> Tick {
        (self.x0 + ((x - self.grid.left()) / self.cell * STEP as f32).round() as Tick)
            .clamp(0, MAX_BARS * BAR)
    }
    /// A point at `step` (fractional) and the middle of `pitch`'s row.
    #[allow(dead_code)]
    pub fn point(&self, step: f32, pitch: u8) -> Pos2 {
        pos2(
            self.x((step * STEP as f32) as Tick),
            self.y(pitch) + self.row / 2.0,
        )
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible(&self, pitch: u8) -> bool {
        (self.bottom..=self.top).contains(&pitch)
    }
    /// Last tick on screen.
    pub fn end(&self) -> Tick {
        self.x0 + (self.grid.width() / self.cell * STEP as f32) as Tick
    }
    pub fn note_rect(&self, start: Tick, len: Tick, pitch: u8) -> Rect {
        let (a, b) = (self.x(start), self.x(start + len));
        Rect::from_min_max(
            pos2(a + 1.0, self.y(pitch) + 1.0),
            pos2((b - 1.0).max(a + 3.0), self.y(pitch) + self.row - 1.0),
        )
    }
    /// Tick and pitch under a point inside the grid (tick not snapped).
    pub fn locate(&self, p: Pos2) -> Option<(Tick, u8)> {
        if !self.grid.contains(p) || p.x >= self.grid.right() || p.y >= self.grid.bottom() {
            return None;
        }
        Some(self.locate_clamped(p))
    }
    /// Tick and pitch for any point, clamped to the grid (drags may leave it).
    pub fn locate_clamped(&self, p: Pos2) -> (Tick, u8) {
        let t = self.x0 + ((p.x - self.grid.left()) / self.cell * STEP as f32).floor() as Tick;
        let row = ((p.y - self.grid.top()) / self.row)
            .floor()
            .clamp(0.0, (self.top - self.bottom) as f32) as u8;
        (t.clamp(0, MAX_BARS * BAR - 1), self.top - row)
    }
}

/// Snap down to the grid (FL "cell": the grid cell under the pointer).
pub(crate) fn snap_floor(t: Tick, grid: Tick, alt: bool) -> Tick {
    if alt || grid <= 1 {
        t
    } else {
        t.div_euclid(grid) * grid
    }
}

/// Snap to the nearest grid line (moves and resize edges).
/// Song tick `t` as a tick of pattern `pid`, through the first unmuted clip
/// of it that plays at `t` (offset cut, repeats past the pattern's end).
pub(crate) fn song_to_pattern_tick(
    song: &keyframexl::sequence::Song,
    pid: u64,
    t: Tick,
) -> Option<Tick> {
    let plen = song.pattern(pid)?.length.max(1);
    song.clips
        .iter()
        .filter(|c| c.pattern == pid && !c.muted && t >= c.start && t < c.start + song.clip_len(c))
        .map(|c| (t - c.start + c.offset).rem_euclid(plen))
        .next()
}

pub(crate) fn snap_round(t: Tick, grid: Tick, alt: bool) -> Tick {
    if alt || grid <= 1 {
        t
    } else {
        (t + grid / 2).div_euclid(grid) * grid
    }
}

/// Target of a move or resize of one note (the reference the gestures follow; tests).
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn move_target(
    mode: MoveMode,
    start: Tick,
    len: Tick,
    pitch: u8,
    grab: Tick,
    at: Tick,
    at_pitch: u8,
    grid: Tick,
    alt: bool,
    pattern_len: Tick,
) -> (Tick, Tick, u8) {
    let min = if alt { 1 } else { grid.max(1) };
    match mode {
        MoveMode::Move => {
            let s = snap_round(at - grab, grid, alt).clamp(0, (pattern_len - len).max(0));
            (s, len, at_pitch)
        }
        MoveMode::ResizeRight => {
            let end = snap_round(at, grid, alt).max(start + min);
            (start, (end - start).min(pattern_len - start).max(1), pitch)
        }
        MoveMode::ResizeLeft => {
            let end = start + len;
            let s = snap_round(at, grid, alt).clamp(0, (end - min).max(0));
            (s, end - s, pitch)
        }
    }
}

impl App {
    /// `pitch` moved onto the scale when "Snap notes to scale" is on.
    pub(crate) fn scale_pitch(&self, pitch: u8) -> u8 {
        match self.roll_scale {
            Some(sc) if self.scale_snap => sc.snap(pitch),
            _ => pitch,
        }
    }

    fn channels(&self) -> Vec<(u64, String)> {
        self.song
            .instruments
            .iter()
            .map(|i| (i.id, i.name.clone()))
            .collect()
    }

    /// Notes of the roll's channel in the current pattern that are selected.
    pub(crate) fn selected_notes(&self) -> Vec<Note> {
        let me = self.roll_inst;
        self.pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me && self.roll_sel.contains(&n.id))
            .copied()
            .collect()
    }

    fn note_mut(&mut self, id: u64) -> Option<&mut Note> {
        self.pattern_mut().notes.iter_mut().find(|n| n.id == id)
    }

    /// Grow the pattern to whole bars when notes run past its end (FL auto length).
    /// Where this pattern is playing, in its own ticks: in Pattern mode the
    /// transport folded into the pattern or its loop; in Song mode through
    /// the playlist clip of this pattern under the song playhead (a clip that
    /// runs past the pattern repeats it, FL), None between its clips.
    pub(crate) fn roll_playhead(&self) -> Option<Tick> {
        let t = self.playhead_tick();
        if !self.song_mode {
            return Some(t);
        }
        song_to_pattern_tick(&self.song, self.selected, t)
    }

    pub(crate) fn fit_pattern(&mut self) {
        let p = self.pattern_mut();
        let end = p
            .notes
            .iter()
            .map(|n| n.end())
            .chain(p.slides.iter().map(|s| s.start + s.length))
            .max()
            .unwrap_or(0);
        if end > p.length {
            p.length = (((end + BAR - 1) / BAR) * BAR).min(MAX_BARS * BAR);
        }
    }

    /// Several piano rolls can be open at once. One of them owns the
    /// selection, the keys and Paste (`roll_inst`): the focused one, or the
    /// one just clicked. The others draw with their own channel and give the
    /// owner back, so selecting in one roll and pasting in another works.
    pub(super) fn roll(&mut self, ui: &mut Ui, inst: &mut u64) {
        let owner = self.roll_inst;
        let pressed =
            ui.input(|i| i.pointer.any_pressed()) && ui.rect_contains_pointer(ui.max_rect());
        let owner_open = self
            .ws
            .dock_ref()
            .iter_all_tabs()
            .any(|(_, t)| *t == Tab::Roll { instrument: owner });
        let mine = owner == *inst || pressed || !owner_open;
        if mine && owner != *inst {
            self.roll_sel.clear();
        }
        self.roll_body(ui, inst);
        if !mine {
            self.roll_inst = owner;
        }
    }

    fn roll_body(&mut self, ui: &mut Ui, inst: &mut u64) {
        self.midi_zones
            .push((ui.clip_rect().intersect(ui.max_rect()), *inst));
        let channels = self.channels();
        self.roll_inst = *inst;
        if !channels.iter().any(|(id, _)| *id == self.roll_inst)
            && let Some((id, _)) = channels.first()
        {
            self.roll_inst = *id;
        }
        let iname = channels
            .iter()
            .find(|(id, _)| *id == self.roll_inst)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| self.tr().roll_channel_fallback.into());
        let idx = self.pattern_index(self.selected) + 1;
        let sub = format!("{idx:02}  {}  ·  {iname}", self.pattern().name);
        self.roll_header(ui, &sub, &channels, &iname);
        if *inst != self.roll_inst {
            self.roll_sel.clear();
        }
        *inst = self.roll_inst;

        // Geometry: the roll always fits its window; pitch and time scroll inside.
        let (body, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let p = ui.painter().clone();
        // Event lane under the grid (FL event editor): its height is dragged at
        // the splitter above it; 0 folds it to the splitter alone. Very short
        // rolls drop it.
        let lane_key = egui::Id::new("roll-lane-h");
        let want: f32 = ui
            .data(|d| d.get_temp(lane_key))
            .unwrap_or((body.height() * 0.28).clamp(LANE_MIN, LANE_H));
        let fits = body.height() >= 180.0;
        let lane_h = if fits && want > 0.0 {
            want.clamp(LANE_MIN, (body.height() * 0.45).max(LANE_MIN))
        } else {
            0.0
        };
        let lane_gap = if fits { SPLIT_H } else { 0.0 };
        let me = self.roll_inst;
        let fixed = 4.0 + RULER_H + 3.0 + lane_gap + lane_h + 2.0;
        let plen = self.pattern().length;
        let grid_w = body.width() - 3.0 - KEYS_W;
        let mut v = match self.roll_view.get(&me) {
            Some(v) => *v,
            None => {
                // First open: show all of this channel's notes and the whole pattern.
                let (lo, hi) = self.note_span(me);
                // Lyrics are written on the notes: a singing channel keeps its rows tall enough to read them.
                let min_row = if self.is_vocal(me) { 20.0 } else { ROW_MIN };
                let row = ((body.height() - fixed) / (hi - lo + 5) as f32)
                    .floor()
                    .clamp(ROW_MIN, 18.0)
                    .max(min_row);
                let span = ((body.height() - fixed) / row).floor() - 1.0;
                RollView {
                    top: ((lo as f32 + hi as f32) / 2.0 + span / 2.0).round(),
                    row,
                    x0: 0.0,
                    cell: 0.0,
                }
            }
        };
        if v.cell <= 0.0 {
            v.cell = (grid_w / steps(plen) as f32).clamp(CELL_MIN, CELL_MAX);
        }
        let hover_grid = ui.rect_contains_pointer(Rect::from_min_size(
            body.min,
            vec2(body.width(), body.height() - lane_h - lane_gap),
        ));
        if hover_grid {
            self.roll_wheel(
                ui,
                &mut v,
                body.left() + 3.0 + KEYS_W,
                body.top() + 4.0 + RULER_H,
            );
        }
        // Middle-button drag pans pitch and time (FL: drag with the wheel button).
        let pan_area = Rect::from_min_size(
            body.min,
            vec2(body.width(), body.height() - lane_h - lane_gap),
        );
        if let Some(d) = kit::middle_pan(ui, pan_area, ui.id().with("roll-pan")) {
            v.top += d.y / v.row;
            v.x0 = (v.x0 - d.x / v.cell * STEP as f32).max(0.0);
        }
        let grid_h_avail = (body.height() - fixed).max(v.row * 4.0);
        let n_rows =
            ((grid_h_avail / v.row).floor() as usize).clamp(4, (HIGHEST - LOWEST + 1) as usize);
        let well = Rect::from_min_size(
            body.min + vec2(0.0, 4.0),
            vec2(body.width(), RULER_H + v.row * n_rows as f32 + 3.0),
        );
        let inner = kit::sunken(&p, well, 8.0).shrink(1.5);
        let grid = Rect::from_min_max(
            pos2(inner.left() + KEYS_W, inner.top() + RULER_H),
            pos2(inner.right(), inner.top() + RULER_H + v.row * n_rows as f32),
        );
        let lane = Rect::from_min_max(
            pos2(well.left(), well.bottom() + lane_gap),
            pos2(well.right(), well.bottom() + lane_gap + lane_h),
        );
        let span = n_rows as f32 - 1.0;
        v.top = v.top.clamp(LOWEST as f32 + span, HIGHEST as f32);
        // Time scrolls up to two bars past the pattern end.
        let view_ticks = grid.width() / v.cell * STEP as f32;
        v.x0 =
            v.x0.clamp(0.0, ((plen + 2 * BAR) as f32 - view_ticks).max(0.0));
        // Follow the playhead in pattern mode (FL): keep it on screen.
        if self.follow && self.is_playing() && !self.song_mode {
            let len_frames = self.frames_per_step() * steps(plen) as f64;
            if len_frames > 0.0 {
                let tick =
                    (self.position_frames() as f64 % len_frames / len_frames * plen as f64) as f32;
                if tick < v.x0 + 8.0 || tick > v.x0 + view_ticks - v.cell * 2.0 {
                    v.x0 = (tick - view_ticks * 0.3)
                        .clamp(0.0, ((plen + 2 * BAR) as f32 - view_ticks).max(0.0));
                }
            }
        }
        self.roll_view.insert(me, v);
        let top_i = v.top.round() as u8;
        let g = Geom {
            grid,
            cell: v.cell,
            row: v.row,
            lane,
            top: top_i,
            bottom: top_i - span as u8,
            len: plen,
            x0: v.x0 as Tick,
        };
        #[cfg(test)]
        {
            self.probe.roll = Some(g);
        }

        self.paint_keys_and_grid(&p, &g, inner);
        // Keys play while held; sliding across them plays each key (FL).
        let keys = Rect::from_min_max(
            pos2(inner.left(), grid.top()),
            pos2(grid.left(), grid.bottom()),
        );
        let kr = ui.interact(keys, ui.id().with("roll-keys"), Sense::click_and_drag());
        if kr.is_pointer_button_down_on()
            && let Some(q) = ui.input(|i| i.pointer.interact_pos())
        {
            let (_, pitch) = g.locate_clamped(pos2(
                grid.left() + 1.0,
                q.y.clamp(grid.top(), grid.bottom() - 1.0),
            ));
            // Velocity from where the key is struck: further right is louder (FL).
            let vel = (40.0 + 87.0 * ((q.x - keys.left()) / keys.width()).clamp(0.0, 1.0)) as u8;
            self.hold_note(me, pitch, vel);
            self.key_down = Some(pitch);
        } else if self.key_down.take().is_some() {
            self.release_note();
        }
        if let Some(k) = self.key_down {
            let y = g.y(k);
            if g.visible(k) {
                p.rect_filled(
                    Rect::from_min_max(pos2(keys.left(), y), pos2(keys.right(), y + g.row)),
                    0.0,
                    t::text().gamma_multiply(0.35),
                );
            }
        }
        // The ruler (pattern mode): click moves the playhead there; drag
        // selects a span the pattern then loops (FL); double-click clears it.
        let ruler = Rect::from_min_max(
            pos2(grid.left(), inner.top()),
            pos2(grid.right(), grid.top()),
        );
        let rr = ui.interact(ruler, ui.id().with("roll-ruler"), Sense::click_and_drag());
        let alt = ui.input(|i| i.modifiers.alt);
        let snap_grid = self.snap.grid();
        let ruler_tick = |x: f32| {
            snap_round(
                g.locate_clamped(pos2(x, grid.top() + 1.0)).0,
                snap_grid,
                alt,
            )
            .max(0)
        };
        if rr.double_clicked() {
            if self.pattern_mut().loop_range.take().is_some() {
                self.publish();
                self.say(Tone::Quiet, self.tr().roll_loop_cleared);
            }
        } else if rr.clicked()
            && let Some(q) = rr.interact_pointer_pos()
        {
            let (tick, _) = g.locate_clamped(pos2(q.x, grid.top() + 1.0));
            self.seek_tick(snap_floor(tick, self.snap.grid(), false));
        }
        if rr.dragged_by(egui::PointerButton::Primary)
            && let (Some(from), Some(to)) = (
                ui.input(|i| i.pointer.press_origin()),
                rr.interact_pointer_pos(),
            )
        {
            let (a, b) = (ruler_tick(from.x), ruler_tick(to.x));
            if a != b {
                self.pattern_mut().loop_range = Some((a.min(b), a.max(b)));
            }
        }
        if rr.drag_stopped_by(egui::PointerButton::Primary)
            && let Some((a, b)) = self.pattern().loop_range
        {
            // Grow the pattern when the span runs past its end.
            let p = self.pattern_mut();
            if b > p.length {
                p.length = ((b + BAR - 1) / BAR * BAR).min(MAX_BARS * BAR);
            }
            self.set_song_mode(false);
            self.publish();
            self.seek_tick(a);
            let msg = crate::i18n::fill(
                self.tr().roll_looping_span,
                &[("a", &bar_pos(a)), ("b", &bar_pos(b))],
            );
            self.say(Tone::Quiet, msg);
        }
        if rr.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let resp = ui.interact(grid, ui.id().with("roll-grid"), Sense::click_and_drag());
        let scrollbar_busy = self.roll_scrollbars(ui, &p, &g, plen);
        let ghost_key = ui.id().with("roll-ghost-click");
        let pointer = resp.interact_pointer_pos();
        let ghost = pointer.and_then(|q| {
            if !self.ghosts {
                return None;
            }
            self.pattern()
                .notes
                .iter()
                .rev()
                .find(|n| {
                    n.instrument != me
                        && g.visible(n.pitch)
                        && g.note_rect(n.start, n.length, n.pitch).contains(q)
                })
                .map(|n| n.instrument)
        });
        let now = ui.input(|i| i.time);
        let prior: Option<(u64, u64, u64, u64, f64)> = ui.data(|d| d.get_temp(ghost_key));
        let switch = if resp.double_clicked() && !scrollbar_busy {
            pointer.and_then(|q| {
                let own = self.note_at(&g, q);
                let created = prior.filter(|(pattern, channel, _, target, time)| {
                    *pattern == self.selected
                        && *channel == me
                        && Some(*target) == ghost
                        && now - time < 1.0
                });
                if own.is_none()
                    || own.is_some_and(|n| created.is_some_and(|(_, _, id, _, _)| n.id == id))
                {
                    ghost.map(|target| (target, created.map(|(_, _, id, _, _)| id)))
                } else {
                    None
                }
            })
        } else {
            None
        };
        if let Some((target, created)) = switch {
            if let Some(id) = created {
                self.pattern_mut().notes.retain(|n| n.id != id);
                for song in self.history.iter_mut().rev().take(2) {
                    if let Some(pattern) = song.pattern_mut(self.selected) {
                        pattern.notes.retain(|n| n.id != id);
                    }
                }
            }
            self.roll_gesture = None;
            self.release_note();
            self.roll_sel.clear();
            self.roll_inst = target;
            *inst = target;
            ui.data_mut(|d| d.remove::<(u64, u64, u64, u64, f64)>(ghost_key));
            ui.ctx().request_repaint();
            return;
        }
        let new_on_ghost = ui.is_enabled()
            && !self.welcome_open
            && !scrollbar_busy
            && ui.input(|i| i.pointer.primary_pressed())
            && resp.hovered()
            && ghost.is_some()
            && pointer.is_some_and(|q| self.note_at(&g, q).is_none());
        let count = self.pattern().notes.len();
        self.roll_input(ui, &p, &g, &resp, scrollbar_busy);
        if new_on_ghost
            && self.pattern().notes.len() > count
            && let (Some(target), Some(&id)) = (ghost, self.roll_sel.first())
        {
            ui.data_mut(|d| d.insert_temp(ghost_key, (self.selected, me, id, target, now)));
        }
        self.paint_notes(&p.with_clip_rect(grid), &g);
        if self.is_vocal(me) {
            if resp.double_clicked()
                && !scrollbar_busy
                && let Some(q) = resp.interact_pointer_pos()
                && let Some(n) = self.note_at(&g, q)
            {
                self.roll_gesture = None;
                self.start_lyric_edit(n.id);
            }
            self.lyric_overlay(ui, &g);
        }
        if let Some((
            _,
            Gesture::Select {
                from_tick,
                from_pitch,
                ..
            },
        )) = &self.roll_gesture
            && let Some(now) = ui.input(|i| i.pointer.interact_pos())
        {
            let from = pos2(g.x(*from_tick), g.y_f32(*from_pitch));
            let r = Rect::from_two_pos(from, now).intersect(grid);
            if r.is_positive() {
                p.rect_filled(r, 2.0, t::text().gamma_multiply(0.06));
                p.rect_stroke(
                    r,
                    2.0,
                    Stroke::new(1.0_f32, t::text()),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if self.focus == Some(Tab::Roll { instrument: me }) {
            self.roll_keys(ui, &g);
        }
        if lane_gap > 0.0 {
            let split = Rect::from_min_max(
                pos2(well.left(), well.bottom()),
                pos2(well.right(), well.bottom() + lane_gap),
            );
            Self::lane_splitter(
                ui,
                &p,
                split,
                body.bottom() - 2.0,
                lane_h,
                lane_key,
                self.tr().lane_target_label(self.lane_target),
            );
        }
        if g.lane.height() > 0.0 {
            self.event_lane(ui, &p, &g);
        }
    }

    /// The bar between the grid and the event lane: drag to size the lane,
    /// down to nothing; double-click folds or opens it.
    fn lane_splitter(
        ui: &Ui,
        p: &egui::Painter,
        split: Rect,
        bottom: f32,
        lane_h: f32,
        key: egui::Id,
        target: &str,
    ) {
        let tr = crate::i18n::ctx_tr(ui.ctx());
        let r = ui
            .interact(split, key.with("split"), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::ResizeVertical);
        let hot = r.hovered() || r.dragged();
        let c = split.center();
        p.line_segment(
            [c - vec2(16.0, 0.0), c + vec2(16.0, 0.0)],
            Stroke::new(2.0_f32, if hot { t::text() } else { t::strong() }),
        );
        if lane_h <= 0.0 {
            kit::caps(
                p,
                pos2(split.left() + 8.0, c.y),
                Align2::LEFT_CENTER,
                &format!("{target}  ▴"),
                10.0,
                if hot { t::text() } else { t::faint() },
            );
        }
        let r = r.on_hover_text(if lane_h > 0.0 {
            tr.roll_lane_fold_tip
        } else {
            tr.roll_lane_open_tip
        });
        if r.double_clicked() {
            let open = if lane_h > 0.0 { 0.0 } else { LANE_H };
            ui.data_mut(|d| d.insert_temp(key, open));
        } else if r.dragged()
            && let Some(q) = r.interact_pointer_pos()
        {
            let h = bottom - q.y - SPLIT_H / 2.0;
            // Below the smallest lane it folds away.
            let h = if h < LANE_MIN * 0.6 {
                0.0
            } else {
                h.max(LANE_MIN)
            };
            ui.data_mut(|d| d.insert_temp(key, h));
            ui.ctx().request_repaint();
        }
    }

    fn roll_header(&mut self, ui: &mut Ui, sub: &str, channels: &[(u64, String)], iname: &str) {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum BatchNoteDialog {
            Strum,
            Arpeggiate,
            Randomize,
            Flip,
            Limit,
        }
        impl BatchNoteDialog {
            fn from_capture(name: &str) -> Option<Self> {
                match name {
                    "Strum" => Some(Self::Strum),
                    "Arpeggiate" => Some(Self::Arpeggiate),
                    "Randomize" => Some(Self::Randomize),
                    "Flip" => Some(Self::Flip),
                    "Limit" => Some(Self::Limit),
                    _ => None,
                }
            }
            fn title(self, tr: &crate::i18n::Strings) -> &'static str {
                match self {
                    Self::Strum => tr.roll_tool_strum,
                    Self::Arpeggiate => tr.roll_tool_arpeggiate,
                    Self::Randomize => tr.roll_tool_randomize,
                    Self::Flip => tr.roll_tool_flip,
                    Self::Limit => tr.roll_tool_limit,
                }
            }
        }
        #[derive(Clone, Copy)]
        enum RollAction {
            SelectAll,
            Deselect,
            Copy,
            Cut,
            Paste,
            Duplicate,
            Delete,
            QuickQuantize,
            Quantize,
            QuickLegato,
            Chop,
            Glue,
            Batch(BatchNoteDialog),
            RiffMachine,
            MuteUnmute,
            PortaToggle,
            OctaveUp,
            OctaveDown,
            Lyrics,
        }

        let tr = self.tr();
        let tab = Tab::Roll {
            instrument: self.roll_inst,
        };
        let has_sel = !self.roll_sel.is_empty();
        let mut action: Option<RollAction> = None;
        App::panel_header(ui, sub, |ui, slot| {
            if slot == Slot::Menu {
                ui.menu_button(tr.menu_edit, |ui| {
                    kit::menu_style(ui);
                    for (act, label, key, needs) in [
                        (
                            RollAction::SelectAll,
                            tr.roll_edit_select_all,
                            "Ctrl+A",
                            false,
                        ),
                        (RollAction::Deselect, tr.roll_edit_deselect, "Ctrl+D", true),
                        (RollAction::Copy, tr.roll_edit_copy, "Ctrl+C", true),
                        (RollAction::Cut, tr.roll_edit_cut, "Ctrl+X", true),
                        (RollAction::Paste, tr.roll_edit_paste, "Ctrl+V", false),
                        (
                            RollAction::Duplicate,
                            tr.roll_edit_duplicate,
                            "Ctrl+B",
                            true,
                        ),
                        (RollAction::Delete, tr.pl_delete, "Del", true),
                    ] {
                        let enabled = !needs || has_sel;
                        if ui
                            .add_enabled(enabled, egui::Button::new(label).shortcut_text(key))
                            .clicked()
                        {
                            action = Some(act);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr.menu_tools, |ui| {
                    kit::menu_style(ui);
                    for (act, label, key) in [
                        (
                            RollAction::QuickQuantize,
                            tr.roll_tool_quick_quantize,
                            "Ctrl+Q",
                        ),
                        (RollAction::Quantize, tr.roll_tool_quantize, "Alt+Q"),
                        (RollAction::QuickLegato, tr.roll_tool_quick_legato, "Ctrl+L"),
                        (RollAction::Chop, tr.roll_tool_chop, "Ctrl+U"),
                        (RollAction::Glue, tr.roll_tool_glue, "Ctrl+G"),
                        (
                            RollAction::Batch(BatchNoteDialog::Strum),
                            tr.roll_tool_strum,
                            "",
                        ),
                        (
                            RollAction::Batch(BatchNoteDialog::Arpeggiate),
                            tr.roll_tool_arpeggiate,
                            "",
                        ),
                        (RollAction::RiffMachine, tr.tools_riff_machine, "Alt+E"),
                        (
                            RollAction::Batch(BatchNoteDialog::Randomize),
                            tr.roll_tool_randomize,
                            "",
                        ),
                        (
                            RollAction::Batch(BatchNoteDialog::Flip),
                            tr.roll_tool_flip,
                            "",
                        ),
                        (
                            RollAction::Batch(BatchNoteDialog::Limit),
                            tr.roll_tool_limit,
                            "",
                        ),
                        (RollAction::MuteUnmute, tr.roll_tool_mute_unmute, ""),
                        (RollAction::PortaToggle, tr.roll_tool_porta_toggle, ""),
                        (RollAction::OctaveUp, tr.roll_tool_octave_up, "Ctrl+↑"),
                        (RollAction::OctaveDown, tr.roll_tool_octave_down, "Ctrl+↓"),
                    ] {
                        if ui
                            .add(egui::Button::new(label).shortcut_text(key))
                            .clicked()
                        {
                            action = Some(act);
                            ui.close_menu();
                        }
                    }
                    if self.is_vocal(self.roll_inst) {
                        ui.separator();
                        if ui
                            .add(egui::Button::new(tr.roll_tool_lyrics).shortcut_text(""))
                            .on_hover_text(tr.roll_tool_lyrics_tip)
                            .clicked()
                        {
                            action = Some(RollAction::Lyrics);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    let r = ui.add(egui::Button::new(tr.roll_porta_new).selected(self.porta_new));
                    if r.on_hover_text(tr.roll_porta_new_tip).clicked() {
                        self.porta_new = !self.porta_new;
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr.roll_menu_scale, |ui| {
                    kit::menu_style(ui);
                    use keyframexl::scale::{Kind, ROOTS, Scale};
                    if ui
                        .selectable_label(self.roll_scale.is_none(), tr.roll_scale_off)
                        .clicked()
                    {
                        self.roll_scale = None;
                        ui.close_menu();
                    }
                    ui.separator();
                    let cur = self.roll_scale.unwrap_or(Scale {
                        root: 0,
                        kind: Kind::Major,
                    });
                    kit::menu_header(
                        ui,
                        egui::RichText::new(tr.roll_scale_root)
                            .font(t::sans(11.0))
                            .color(t::faint()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(220.0);
                        for (i, r) in ROOTS.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    self.roll_scale.is_some() && cur.root == i as u8,
                                    *r,
                                )
                                .clicked()
                            {
                                self.roll_scale = Some(Scale {
                                    root: i as u8,
                                    ..cur
                                });
                            }
                        }
                    });
                    ui.separator();
                    for k in Kind::ALL {
                        if ui
                            .selectable_label(
                                self.roll_scale.is_some_and(|s| s.kind == k),
                                tr.scale_kind_label(k),
                            )
                            .clicked()
                        {
                            self.roll_scale = Some(Scale { kind: k, ..cur });
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    let r = ui.add_enabled(
                        self.roll_scale.is_some(),
                        egui::Button::new(tr.roll_scale_snap).selected(self.scale_snap),
                    );
                    if r.on_hover_text(tr.roll_scale_snap_tip).clicked() {
                        self.scale_snap = !self.scale_snap;
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr.roll_menu_snap, |ui| {
                    kit::menu_style(ui);
                    for s in Snap::ALL {
                        if ui
                            .selectable_label(self.snap == s, tr.snap_label(s))
                            .clicked()
                        {
                            self.snap = s;
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr.roll_menu_tool, |ui| {
                    kit::menu_style(ui);
                    for tl in Tool::ALL {
                        let r = ui.add(
                            egui::Button::new(tr.tool_label(tl))
                                .selected(self.tool == tl)
                                .shortcut_text(format!("{:?}", tl.key())),
                        );
                        if r.on_hover_text(tr.tool_hint(tl)).clicked() {
                            self.tool = tl;
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr.roll_menu_note_color, |ui| {
                    kit::menu_style(ui);
                    for c in 0u8..(NOTE_COLOR_COUNT as u8) {
                        let name = tr.note_color_name(c);
                        let (r, resp) = ui.allocate_exact_size(vec2(160.0, 22.0), Sense::click());
                        let (top, _, _) = note_colors(c);
                        ui.painter().rect_filled(
                            Rect::from_center_size(
                                pos2(r.left() + 12.0, r.center().y),
                                vec2(14.0, 10.0),
                            ),
                            3.0,
                            top,
                        );
                        let ink = if self.note_color == c {
                            t::text()
                        } else {
                            t::muted()
                        };
                        ui.painter().text(
                            pos2(r.left() + kit::MENU_PAD.x, r.center().y),
                            Align2::LEFT_CENTER,
                            name,
                            t::sans(13.0),
                            ink,
                        );
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 3.0, t::hi());
                        }
                        if resp.clicked() {
                            self.set_note_color(c);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr.roll_menu_target_ch, |ui| {
                    kit::menu_style(ui);
                    for (id, name) in channels {
                        if ui.selectable_label(self.roll_inst == *id, name).clicked() {
                            self.roll_inst = *id;
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr.roll_menu_target_ctrl, |ui| {
                    kit::menu_style(ui);
                    for lt in LaneTarget::ALL {
                        if ui
                            .selectable_label(self.lane_target == lt, tr.lane_target_label(lt))
                            .clicked()
                        {
                            self.lane_target = lt;
                            ui.close_menu();
                        }
                    }
                });
                if kit::menu_check(ui, self.ghosts, tr.roll_ghost_notes, "Alt+V")
                    .on_hover_text(tr.roll_ghost_notes_tip)
                    .clicked()
                {
                    self.ghosts = !self.ghosts;
                    ui.close_menu();
                }
                if ui.button(tr.roll_fit_pattern).clicked() {
                    if let Some(v) = self.roll_view.get_mut(&self.roll_inst) {
                        v.cell = 0.0;
                        v.x0 = 0.0;
                    }
                    ui.close_menu();
                }
                self.window_menu_tail(ui, tab.clone());
                return;
            }
            let mut vel = self.velocity as f64;
            let (_, _, changed) = kit::knob(
                ui,
                &kit::Knob {
                    label: tr.roll_vel_label,
                    color: t::enc_white(),
                    min: 1.0,
                    max: 127.0,
                    default: 100.0,
                    log: false,
                    fmt: &|v| format!("{v:.0}"),
                    hint: tr.roll_vel_hint,
                },
                &mut vel,
            );
            if changed {
                self.velocity = vel.round() as u8;
            }
            ui.add_space(4.0);
            // FL: the tool icons sit in the toolbar, one click each.
            ui.spacing_mut().item_spacing.x = 2.0;
            // The toolbar fills right to left: reversed, they read Draw … Slide.
            for tl in Tool::ALL.into_iter().rev() {
                let on = self.tool == tl;
                let kind = if on {
                    kit::Kind::Normal
                } else {
                    kit::Kind::Ghost
                };
                let tl_label = tr.tool_label(tl);
                let r = kit::button_sized(
                    ui,
                    vec2(28.0, kit::TOOL_H),
                    kind,
                    true,
                    tl_label,
                    |p, face, ink| {
                        tool_icon(p, face.center(), tl, if on { t::text() } else { ink });
                        if on {
                            p.line_segment(
                                [
                                    pos2(face.left() + 7.0, face.bottom() - 2.0),
                                    pos2(face.right() - 7.0, face.bottom() - 2.0),
                                ],
                                Stroke::new(1.5_f32, t::text()),
                            );
                        }
                    },
                );
                if kit::tip(
                    r,
                    format!("{tl_label}  {:?}\n{}", tl.key(), tr.tool_hint(tl)),
                )
                .clicked()
                {
                    self.tool = tl;
                }
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            if channels.len() > 1 {
                ui.add_space(4.0);
                let mut pick = None;
                kit::dropdown(ui, "roll-channel", iname, 150.0, |ui| {
                    for (id, name) in channels {
                        if ui.selectable_label(self.roll_inst == *id, name).clicked() {
                            pick = Some(*id);
                        }
                    }
                })
                .on_hover_text(tr.roll_channel_tip);
                if let Some(id) = pick {
                    self.roll_inst = id;
                }
            }
        });
        match action {
            Some(RollAction::SelectAll) => self.select_all_notes(),
            Some(RollAction::Deselect) => self.roll_sel.clear(),
            Some(RollAction::Copy) => self.copy_notes(false),
            Some(RollAction::Cut) => self.copy_notes(true),
            Some(RollAction::Paste) => self.paste_notes(),
            Some(RollAction::Duplicate) => self.duplicate_notes(),
            Some(RollAction::Lyrics) => self.lyrics_dialog = Some(String::new()),
            Some(RollAction::Delete) => self.delete_selected_notes(),
            Some(RollAction::QuickQuantize) => self.quantize_notes(),
            Some(RollAction::Quantize) => self.open_quantize(ui.ctx()),
            Some(RollAction::RiffMachine) => self.open_riff(self.roll_inst),
            Some(RollAction::QuickLegato) => self.legato_notes(),
            Some(RollAction::Chop) => self.chop_notes(),
            Some(RollAction::Glue) => self.glue_notes(),
            Some(RollAction::Batch(dlg)) => {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("batch-note-dialog"), dlg));
            }
            Some(RollAction::MuteUnmute) => self.mute_notes(),
            Some(RollAction::PortaToggle) => self.porta_notes(),
            Some(RollAction::OctaveUp) => self.nudge_notes(0, 12, 0),
            Some(RollAction::OctaveDown) => self.nudge_notes(0, -12, 0),
            None => {}
        }
        let dialog_id = egui::Id::new("batch-note-dialog");
        // Captures: KFXL_CAPTURE_NOTE_DIALOG=Strum (etc.) opens that dialog once.
        static CAPTURE_DIALOG: std::sync::Once = std::sync::Once::new();
        if let Ok(name) = std::env::var("KFXL_CAPTURE_NOTE_DIALOG")
            && let Some(dlg) = BatchNoteDialog::from_capture(&name)
        {
            CAPTURE_DIALOG.call_once(|| {
                ui.ctx().data_mut(|d| d.insert_temp(dialog_id, dlg));
            });
        }
        let dialog: Option<BatchNoteDialog> = ui.ctx().data(|d| d.get_temp(dialog_id));
        if let Some(dialog) = dialog {
            let params_id = egui::Id::new("batch-note-params");
            let mut params: (i64, i64, i64, i64, i32, u64) =
                ui.ctx().data(|d| d.get_temp(params_id)).unwrap_or_else(|| {
                    if dialog == BatchNoteDialog::Limit {
                        (1, 127, 0, 127, 0, 1)
                    } else {
                        (30, 10, 1, 127, 0, 1)
                    }
                });
            let mut apply = false;
            let mut open = true;
            let mut cancel = false;
            let steps_unit = tr.roll_steps_unit;
            egui::Window::new(dialog.title(tr))
                .id(egui::Id::new("batch-note-window"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(Align2::CENTER_TOP, vec2(0.0, 140.0))
                .show(ui.ctx(), |ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                    let note = match dialog {
                        BatchNoteDialog::Strum => tr.roll_batch_strum_desc,
                        BatchNoteDialog::Arpeggiate => tr.roll_batch_arp_desc,
                        BatchNoteDialog::Randomize => tr.roll_batch_rand_desc,
                        BatchNoteDialog::Flip => tr.roll_batch_flip_desc,
                        BatchNoteDialog::Limit => tr.roll_batch_limit_desc,
                    };
                    ui.label(
                        egui::RichText::new(note)
                            .font(t::sans(12.0))
                            .color(t::faint()),
                    );
                    // 480 ticks = one beat (PPQ 480), 120 = one step.
                    let ticks = |ui: &mut Ui, v: &mut i64, max: i64| {
                        kit::drag_value(
                            ui,
                            egui::DragValue::new(v)
                                .range(0..=max)
                                .speed(5.0)
                                .custom_formatter(|x, _| {
                                    format!("{:.2} {steps_unit}", x / STEP as f64)
                                })
                                .custom_parser(|s| {
                                    s.trim()
                                        .trim_end_matches(steps_unit)
                                        .trim_end_matches("steps")
                                        .trim()
                                        .parse::<f64>()
                                        .ok()
                                        .map(|x| x * STEP as f64)
                                }),
                        );
                    };
                    egui::Grid::new("batch-note-grid")
                        .num_columns(2)
                        .spacing(vec2(14.0, 8.0))
                        .show(ui, |ui| {
                            let label = |ui: &mut Ui, s: &str| {
                                ui.label(
                                    egui::RichText::new(s).font(t::sans(13.0)).color(t::text()),
                                );
                            };
                            match dialog {
                                BatchNoteDialog::Strum | BatchNoteDialog::Arpeggiate => {
                                    label(
                                        ui,
                                        if dialog == BatchNoteDialog::Strum {
                                            tr.roll_delay_per_note
                                        } else {
                                            tr.roll_note_length
                                        },
                                    );
                                    ticks(ui, &mut params.0, 3840);
                                    ui.end_row();
                                    label(ui, tr.roll_direction);
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(&mut params.4, 0, tr.roll_dir_up);
                                        ui.selectable_value(&mut params.4, 1, tr.roll_dir_down);
                                        if dialog == BatchNoteDialog::Arpeggiate {
                                            ui.selectable_value(
                                                &mut params.4,
                                                2,
                                                tr.roll_dir_up_down,
                                            );
                                        }
                                    });
                                    ui.end_row();
                                }
                                BatchNoteDialog::Randomize => {
                                    label(ui, tr.roll_start_pm);
                                    ticks(ui, &mut params.0, 3840);
                                    ui.end_row();
                                    label(ui, tr.roll_vel_pm);
                                    kit::drag_value(
                                        ui,
                                        egui::DragValue::new(&mut params.1).range(0..=127),
                                    );
                                    ui.end_row();
                                    label(ui, tr.rack_humanize_seed);
                                    kit::drag_value(ui, egui::DragValue::new(&mut params.5));
                                    ui.end_row();
                                }
                                BatchNoteDialog::Flip => {
                                    label(ui, tr.roll_flip_axis);
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(
                                            &mut params.4,
                                            0,
                                            tr.roll_flip_vertical,
                                        );
                                        ui.selectable_value(
                                            &mut params.4,
                                            1,
                                            tr.roll_flip_horizontal,
                                        );
                                    });
                                    ui.end_row();
                                }
                                BatchNoteDialog::Limit => {
                                    label(ui, tr.roll_vel_label);
                                    ui.horizontal(|ui| {
                                        kit::drag_value(
                                            ui,
                                            egui::DragValue::new(&mut params.0).range(1..=127),
                                        );
                                        ui.label(tr.roll_range_to);
                                        kit::drag_value(
                                            ui,
                                            egui::DragValue::new(&mut params.1).range(1..=127),
                                        );
                                    });
                                    ui.end_row();
                                    label(ui, tr.roll_pitch);
                                    ui.horizontal(|ui| {
                                        kit::drag_value(
                                            ui,
                                            egui::DragValue::new(&mut params.2)
                                                .range(0..=127)
                                                .custom_formatter(|x, _| note_name(x as u8)),
                                        );
                                        ui.label(tr.roll_range_to);
                                        kit::drag_value(
                                            ui,
                                            egui::DragValue::new(&mut params.3)
                                                .range(0..=127)
                                                .custom_formatter(|x, _| note_name(x as u8)),
                                        );
                                    });
                                    ui.end_row();
                                }
                            }
                        });
                    let applies = if self.roll_sel.is_empty() {
                        tr.roll_batch_applies_all
                    } else {
                        tr.roll_batch_applies_sel
                    };
                    ui.label(
                        egui::RichText::new(applies)
                            .font(t::sans(12.0))
                            .color(t::faint()),
                    );
                    ui.horizontal(|ui| {
                        if kit::button(ui, tr.common_apply, kit::Kind::Primary, true).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            apply = true;
                        }
                        if kit::button(ui, tr.common_cancel, kit::Kind::Normal, true).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            cancel = true;
                        }
                    });
                });
            if cancel {
                open = false;
            }
            ui.ctx().data_mut(|d| d.insert_temp(params_id, params));
            if apply {
                use keyframexl::note_ops::{self, ArpDirection};
                let mut notes = self.target_notes();
                if !notes.is_empty() {
                    self.checkpoint();
                    match dialog {
                        BatchNoteDialog::Strum => {
                            note_ops::strum(&mut notes, params.0, params.4 == 1)
                        }
                        BatchNoteDialog::Randomize => {
                            note_ops::randomize(&mut notes, params.1 as u8, params.0, params.5)
                        }
                        BatchNoteDialog::Flip => note_ops::flip(&mut notes, params.4 == 0),
                        BatchNoteDialog::Limit => note_ops::limit(
                            &mut notes,
                            (params.0 as u8, params.1 as u8),
                            (params.2 as u8, params.3 as u8),
                        ),
                        BatchNoteDialog::Arpeggiate => {
                            let direction = match params.4 {
                                1 => ArpDirection::Down,
                                2 => ArpDirection::UpDown,
                                _ => ArpDirection::Up,
                            };
                            let result = note_ops::arpeggiate(&notes, params.0, direction);
                            let ids: Vec<_> = notes.iter().map(|n| n.id).collect();
                            self.pattern_mut().notes.retain(|n| !ids.contains(&n.id));
                            let selected = !self.roll_sel.is_empty();
                            let pid = self.selected;
                            let new_ids: Vec<_> = result
                                .into_iter()
                                .map(|n| self.song.add_note(pid, n))
                                .collect();
                            if selected {
                                self.roll_sel = new_ids;
                            }
                            notes.clear();
                        }
                    }
                    self.put_notes(&notes);
                    self.fit_pattern();
                    self.publish();
                    let msg = crate::i18n::fill(
                        tr.roll_batch_applied,
                        &[("name", dialog.title(tr).trim_end_matches('…'))],
                    );
                    self.say(Tone::Quiet, msg);
                } else {
                    self.say(Tone::Quiet, tr.roll_batch_no_notes);
                }
                open = false;
            }
            if !open {
                ui.ctx().data_mut(|d| {
                    d.remove::<BatchNoteDialog>(dialog_id);
                    d.remove::<(i64, i64, i64, i64, i32, u64)>(params_id);
                });
            }
        }
    }

    /// Wheel: pitch, Shift = time, Ctrl = time zoom at the pointer, Alt = row height.
    /// Real scrollbars (FL): pitch on the right edge of the grid, time along
    /// the bottom. Drag the thumb; click the track to page.
    fn roll_scrollbars(&mut self, ui: &mut Ui, p: &egui::Painter, g: &Geom, plen: Tick) -> bool {
        let me = self.roll_inst;
        let Some(mut v) = self.roll_view.get(&me).copied() else {
            return false;
        };
        let mut busy = false;
        let bar = |ui: &mut Ui, track: Rect, thumb: Rect, id: &str| -> (egui::Response, f32) {
            let r = ui.interact(track, ui.id().with(id), Sense::click_and_drag());
            let lit = r.hovered() || r.dragged();
            p.rect_filled(
                track,
                3.0,
                t::sunken().gamma_multiply(if lit { 1.0 } else { 0.6 }),
            );
            p.rect_filled(
                thumb,
                3.0,
                if lit {
                    t::strong()
                } else {
                    t::strong().gamma_multiply(0.55)
                },
            );
            let page = r
                .clicked()
                .then(|| r.interact_pointer_pos())
                .flatten()
                .map(|q| {
                    if thumb.contains(q) {
                        0.0
                    } else if q.y < thumb.top() || q.x < thumb.left() {
                        -1.0
                    } else {
                        1.0
                    }
                })
                .unwrap_or(0.0);
            (r, page)
        };
        // Pitch.
        let total = (HIGHEST - LOWEST + 1) as f32;
        let shown = (g.top - g.bottom + 1) as f32;
        if shown < total {
            let track = Rect::from_min_max(
                pos2(g.grid.right() - 9.0, g.grid.top() + 2.0),
                pos2(g.grid.right() - 2.0, g.grid.bottom() - 11.0),
            );
            let t0 = (HIGHEST as f32 - v.top) / total;
            let thumb = Rect::from_min_max(
                pos2(track.left(), track.top() + t0 * track.height()),
                pos2(
                    track.right(),
                    track.top() + ((t0 + shown / total) * track.height()).min(track.height()),
                ),
            );
            let (r, page) = bar(ui, track, thumb, "roll-vscroll");
            if r.hovered() || r.dragged() || r.clicked() {
                busy = true;
            }
            if r.dragged() {
                v.top -= r.drag_delta().y / track.height() * total;
            }
            v.top -= page * shown;
            v.top = v.top.clamp(LOWEST as f32 + shown - 1.0, HIGHEST as f32);
        }
        // Time.
        let span = (plen + 2 * BAR) as f32;
        let view = g.grid.width() / v.cell * STEP as f32;
        if view < span {
            let track = Rect::from_min_max(
                pos2(g.grid.left() + 2.0, g.grid.bottom() - 9.0),
                pos2(g.grid.right() - 11.0, g.grid.bottom() - 2.0),
            );
            let t0 = v.x0 / span;
            let thumb = Rect::from_min_max(
                pos2(track.left() + t0 * track.width(), track.top()),
                pos2(
                    (track.left() + (t0 + view / span) * track.width()).min(track.right()),
                    track.bottom(),
                ),
            );
            let (r, page) = bar(ui, track, thumb, "roll-hscroll");
            if r.hovered() || r.dragged() || r.clicked() {
                busy = true;
            }
            if r.dragged() {
                v.x0 += r.drag_delta().x / track.width() * span;
            }
            v.x0 += page * view;
            v.x0 = v.x0.clamp(0.0, (span - view).max(0.0));
        }
        self.roll_view.insert(me, v);
        busy
    }

    fn roll_wheel(&mut self, ui: &mut Ui, v: &mut RollView, grid_left: f32, grid_top: f32) {
        let (d, m, at, zoom) = ui.input_mut(|i| {
            let raw = i.raw_scroll_delta;
            let smooth = i.smooth_scroll_delta;
            let pick = |r: f32, s: f32| if r != 0.0 { r } else { s };
            let d = egui::Vec2::new(pick(raw.x, smooth.x), pick(raw.y, smooth.y));
            i.smooth_scroll_delta = egui::Vec2::ZERO;
            i.raw_scroll_delta = egui::Vec2::ZERO;
            (d, i.modifiers, i.pointer.hover_pos(), i.zoom_delta())
        });
        if d == egui::Vec2::ZERO && zoom == 1.0 {
            return;
        }
        if m.command || zoom != 1.0 {
            let f = if zoom != 1.0 {
                zoom
            } else {
                (1.0 + d.y * 0.004).clamp(0.5, 2.0)
            };
            let px = at.map(|p| p.x - grid_left).unwrap_or(0.0).max(0.0);
            let t_at = v.x0 + px / v.cell * STEP as f32;
            v.cell = (v.cell * f).clamp(CELL_MIN, CELL_MAX);
            v.x0 = (t_at - px / v.cell * STEP as f32).max(0.0);
            ui.ctx().request_repaint();
        } else if m.alt {
            // Taller or shorter rows; the pitch under the pointer stays under it.
            let dy = d.y + d.x;
            if dy != 0.0 {
                let old = v.row;
                v.row = (v.row * (1.0 + dy * 0.003).clamp(0.8, 1.25)).clamp(ROW_MIN, ROW_MAX);
                if let Some(py) = at.map(|p| p.y - grid_top) {
                    v.top += py / v.row - py / old;
                }
                ui.ctx().request_repaint();
            }
        } else {
            let (dy, dx) = if m.shift {
                (0.0, d.y + d.x)
            } else {
                (d.y, d.x)
            };
            v.top += dy / v.row;
            v.x0 -= dx / v.cell * STEP as f32;
        }
    }

    fn paint_keys_and_grid(&self, p: &egui::Painter, g: &Geom, inner: Rect) {
        let keys = Rect::from_min_max(
            pos2(inner.left(), g.grid.top()),
            pos2(g.grid.left(), g.grid.bottom()),
        );
        let ruler = Rect::from_min_max(
            pos2(g.grid.left(), inner.top()),
            pos2(g.grid.right(), g.grid.top()),
        );
        p.rect_filled(
            Rect::from_min_max(inner.left_top(), pos2(inner.right(), g.grid.top())),
            0.0,
            t::surface(),
        );
        kit::caps(
            p,
            pos2(inner.left() + 8.0, ruler.center().y),
            Align2::LEFT_CENTER,
            self.tr().roll_bar_caps,
            11.0,
            t::faint(),
        );
        let gp = p.with_clip_rect(Rect::from_min_max(ruler.min, g.grid.max));
        // Steps that fit on screen; thin out lines when zoomed far out.
        let first = (g.x0 / STEP).max(0);
        let last = g.end() / STEP + 1;
        let every = if g.cell < 6.0 { 4 } else { 1 };
        for s in first..=last {
            let x = g.x(s * STEP);
            if s % 16 == 0 || (s % 4 == 0 && g.cell * 4.0 > 34.0) {
                let label = if s % 16 == 0 {
                    format!("{}", s / 16 + 1)
                } else {
                    format!("{}.{}", s / 16 + 1, s % 16 / 4 + 1)
                };
                gp.text(
                    pos2(x + 4.0, ruler.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    t::mono(12.0),
                    if s % 16 == 0 { t::text() } else { t::muted() },
                );
                gp.line_segment(
                    [pos2(x, ruler.top() + 4.0), pos2(x, ruler.bottom())],
                    Stroke::new(1.0_f32, t::strong()),
                );
            } else if s % every == 0 {
                gp.line_segment(
                    [pos2(x, ruler.bottom() - 4.0), pos2(x, ruler.bottom())],
                    Stroke::new(1.0_f32, t::line()),
                );
            }
        }
        // Rows: white-key rows lighter than black-key rows.
        for pitch in (g.bottom..=g.top).rev() {
            let y = g.y(pitch);
            let rr = Rect::from_min_max(pos2(g.grid.left(), y), pos2(g.grid.right(), y + g.row));
            let row_fill = match self.roll_scale {
                // In the scale: the lighter row; outside: darker than a black-key row.
                Some(sc) if sc.is_root(pitch) => t::row_root(),
                Some(sc) if sc.contains(pitch) => t::row_white(),
                Some(_) => t::row_out(),
                None if is_black(pitch) => t::row_black(),
                None => t::row_white(),
            };
            p.rect_filled(rr, 0.0, row_fill);
            if pitch % 12 == 0 || pitch % 12 == 5 {
                p.line_segment(
                    [
                        pos2(g.grid.left(), y + g.row),
                        pos2(g.grid.right(), y + g.row),
                    ],
                    Stroke::new(
                        1.0_f32,
                        if pitch % 12 == 0 {
                            t::strong().gamma_multiply(0.6)
                        } else {
                            t::line()
                        },
                    ),
                );
            }
            let key = Rect::from_min_max(pos2(keys.left(), y), pos2(keys.right(), y + g.row));
            p.rect_filled(key, 0.0, t::key_white());
            if is_black(pitch) {
                // The seam between the two white keys shows only beside the black one.
                let black_r = key.left() + key.width() * 0.62;
                p.line_segment(
                    [
                        pos2(black_r, key.center().y),
                        pos2(key.right(), key.center().y),
                    ],
                    Stroke::new(1.0_f32, t::key_white_edge()),
                );
                p.rect_filled(
                    Rect::from_min_max(key.min, pos2(black_r, key.bottom())),
                    1.0,
                    t::key_black(),
                );
            } else if matches!(pitch % 12, 0 | 5) {
                p.line_segment(
                    [
                        pos2(key.left(), key.bottom() - 0.5),
                        pos2(key.right(), key.bottom() - 0.5),
                    ],
                    Stroke::new(1.0_f32, t::key_white_edge()),
                );
            }
            let named = match self.roll_scale {
                Some(sc) => sc.is_root(pitch),
                None => pitch % 12 == 0,
            };
            if named && g.row >= 10.0 {
                let ink = if is_black(pitch) {
                    t::key_white()
                } else {
                    t::key_ink()
                };
                p.text(
                    pos2(key.right() - 5.0, key.center().y),
                    Align2::RIGHT_CENTER,
                    note_name(pitch),
                    t::mono(12.0),
                    ink,
                );
            }
        }
        p.line_segment(
            [
                pos2(keys.right(), keys.top()),
                pos2(keys.right(), keys.bottom()),
            ],
            Stroke::new(1.0_f32, t::edge()),
        );
        let snap = self.snap.grid();
        for s in first..=last {
            let x = g.x(s * STEP);
            let (w, c) = if s % 16 == 0 {
                (1.5_f32, t::strong())
            } else if s % 4 == 0 {
                (1.0_f32, t::strong().gamma_multiply(0.7))
            } else if s % every == 0 {
                (1.0_f32, t::line())
            } else {
                continue;
            };
            gp.line_segment(
                [pos2(x, g.grid.top()), pos2(x, g.grid.bottom())],
                Stroke::new(w, c),
            );
        }
        // Snap lines finer than a step (1/32, triplets) when there is room.
        if snap < STEP && snap > 1 && g.cell * snap as f32 / STEP as f32 > 6.0 {
            let mut tk = (g.x0 / snap) * snap;
            while tk <= g.end() {
                if tk % STEP != 0 {
                    let x = g.x(tk);
                    gp.line_segment(
                        [pos2(x, g.grid.top()), pos2(x, g.grid.bottom())],
                        Stroke::new(1.0_f32, t::line().gamma_multiply(0.5)),
                    );
                }
                tk += snap;
            }
        }
        // Past the pattern end: shaded, still drawable (the pattern grows).
        let end_x = g.x(g.len);
        if end_x < g.grid.right() {
            let shade = Rect::from_min_max(
                pos2(end_x.max(g.grid.left()), ruler.top()),
                g.grid.right_bottom(),
            );
            p.rect_filled(shade, 0.0, egui::Color32::from_black_alpha(90));
            if end_x >= g.grid.left() {
                p.line_segment(
                    [pos2(end_x, ruler.top()), pos2(end_x, g.grid.bottom())],
                    Stroke::new(1.5_f32, t::wait()),
                );
            }
        }
        // The loop: a band on the ruler and a wash over the notes (as in the playlist).
        if let Some((a, b)) = self.pattern().loop_range {
            let (xa, xb) = (g.x(a), g.x(b));
            let lp = p.with_clip_rect(Rect::from_min_max(ruler.min, g.grid.max));
            lp.rect_filled(
                Rect::from_min_max(pos2(xa, ruler.top()), pos2(xb, ruler.top() + 5.0)),
                0.0,
                t::wait(),
            );
            lp.rect_filled(
                Rect::from_min_max(pos2(xa, ruler.top() + 5.0), pos2(xb, g.grid.bottom())),
                0.0,
                t::wait().gamma_multiply(0.07),
            );
            for x in [xa, xb] {
                lp.line_segment(
                    [pos2(x, ruler.top()), pos2(x, g.grid.bottom())],
                    Stroke::new(1.0_f32, t::wait().gamma_multiply(0.7)),
                );
            }
        }
        // Playhead: white line with a caret in the ruler.
        if self.is_playing()
            && let Some(tick) = self.roll_playhead()
        {
            let x = g.x(tick);
            if (g.grid.left()..=g.grid.right()).contains(&x) {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        pos2(x - 5.0, ruler.top() + 6.0),
                        pos2(x + 5.0, ruler.top() + 6.0),
                        pos2(x, ruler.bottom() - 2.0),
                    ],
                    t::text(),
                    Stroke::NONE,
                ));
                p.line_segment(
                    [pos2(x, g.grid.top()), pos2(x, g.grid.bottom())],
                    Stroke::new(1.5_f32, t::text()),
                );
            }
        }
    }

    fn paint_notes(&self, p: &egui::Painter, g: &Geom) {
        let pat = self.pattern();
        let me = self.roll_inst;
        let vocal = self
            .song
            .instruments
            .iter()
            .find(|i| i.id == me)
            .and_then(|i| match &i.kind {
                keyframexl::sequence::InstrumentKind::Vocal { voice } => {
                    Some(voice.default_lyric.clone())
                }
                _ => None,
            });
        let range = g.bottom..=g.top;
        let on_screen = |s: Tick, l: Tick| s + l >= g.x0 && s <= g.end();
        if self.ghosts {
            for n in pat.notes.iter().filter(|n| {
                n.instrument != me && range.contains(&n.pitch) && on_screen(n.start, n.length)
            }) {
                let r = g.note_rect(n.start, n.length, n.pitch);
                let (top, fill, _) = note_colors(n.color);
                p.rect_filled(r, 3.0, fill.gamma_multiply(0.25));
                p.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0_f32, top.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
            }
        }
        for s in pat
            .slides
            .iter()
            .filter(|s| range.contains(&s.target) && on_screen(s.start, s.length))
        {
            let own = s.instrument == me;
            if !own && !self.ghosts {
                continue;
            }
            let r = g.note_rect(s.start, s.length, s.target);
            let col = if own { t::wait() } else { t::strong() };
            if own {
                p.rect_filled(r, 3.0, t::wait_soft());
            }
            p.rect_stroke(r, 3.0, Stroke::new(1.5_f32, col), egui::StrokeKind::Inside);
            if let Some(src) = pat
                .notes
                .iter()
                .filter(|n| {
                    n.group == s.group
                        && n.instrument == s.instrument
                        && n.start <= s.start
                        && n.end() > s.start
                })
                .max_by_key(|n| n.pitch)
                && range.contains(&src.pitch)
            {
                p.line_segment(
                    [
                        pos2(r.left(), g.y(src.pitch) + g.row / 2.0),
                        pos2(r.right(), r.center().y),
                    ],
                    Stroke::new(1.5_f32, col),
                );
            }
            if own && r.width() > 64.0 {
                p.text(
                    pos2(r.left() + 8.0, r.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::fill(self.tr().roll_slide_to, &[("note", &note_name(s.target))]),
                    t::sans(12.0),
                    t::text(),
                );
            }
        }
        for n in pat.notes.iter().filter(|n| {
            n.instrument == me && range.contains(&n.pitch) && on_screen(n.start, n.length)
        }) {
            let r = g.note_rect(n.start, n.length, n.pitch);
            let soft = (1.0 - n.velocity as f32 / 127.0) * 0.7;
            let selected = self.roll_sel.contains(&n.id);
            let face = if n.muted {
                p.rect_stroke(
                    r,
                    4.0,
                    Stroke::new(1.5_f32, note_colors(n.color).0.gamma_multiply(0.45)),
                    egui::StrokeKind::Inside,
                );
                r
            } else {
                let (top, fill, edge) = note_colors(n.color);
                kit::raised(
                    p,
                    r,
                    4.0,
                    kit::mix(top, t::brand_soft(), soft),
                    kit::mix(fill, t::brand_soft(), soft),
                    edge,
                    false,
                    1.0,
                )
            };
            if selected {
                p.rect_stroke(
                    r.expand(1.0),
                    4.0,
                    Stroke::new(1.5_f32, t::text()),
                    egui::StrokeKind::Outside,
                );
            }
            if n.porta {
                // FL marks porta notes with a ramp into the note.
                let w = (r.width() * 0.4).min(12.0);
                p.line_segment(
                    [
                        pos2(r.left() + 2.0, r.bottom() - 2.0),
                        pos2(r.left() + 2.0 + w, r.top() + 2.0),
                    ],
                    Stroke::new(1.5_f32, t::text().gamma_multiply(0.8)),
                );
            }
            if face.width() > 34.0 && face.height() >= 12.0 {
                let ink = if n.muted { t::muted() } else { t::on_brand() };
                match vocal.as_ref() {
                    // A singing channel names the syllable (what the note sings), not the pitch.
                    Some(default) => {
                        let (text, own) = match pat.lyrics.get(&n.id) {
                            Some(l) => (l.as_str(), true),
                            None => (default.as_str(), false),
                        };
                        let shown = kit::elide(p, text, t::semibold(13.0), face.width() - 10.0);
                        p.text(
                            pos2(face.left() + 6.0, face.center().y),
                            Align2::LEFT_CENTER,
                            shown,
                            t::semibold(13.0),
                            if own { ink } else { ink.gamma_multiply(0.55) },
                        );
                    }
                    None => {
                        p.text(
                            pos2(face.left() + 6.0, face.center().y),
                            Align2::LEFT_CENTER,
                            note_name(n.pitch),
                            t::sans(12.0),
                            ink,
                        );
                    }
                }
            }
        }
    }

    /// Lowest and highest pitch this instrument uses here (notes and slide targets), else C4.
    fn note_span(&self, inst: u64) -> (u8, u8) {
        let pat = self.pattern();
        let v: Vec<u8> = pat
            .notes
            .iter()
            .filter(|n| n.instrument == inst)
            .map(|n| n.pitch)
            .chain(
                pat.slides
                    .iter()
                    .filter(|s| s.instrument == inst)
                    .map(|s| s.target),
            )
            .collect();
        match (v.iter().min(), v.iter().max()) {
            (Some(lo), Some(hi)) => (*lo, *hi),
            _ => (60, 60),
        }
    }

    /// Topmost note of the roll's channel under a point.
    fn note_at(&self, g: &Geom, p: Pos2) -> Option<Note> {
        let (at, pitch) = g.locate(p)?;
        let me = self.roll_inst;
        self.pattern()
            .notes
            .iter()
            .rev()
            .find(|n| n.instrument == me && n.pitch == pitch && at >= n.start && at < n.end())
            .copied()
    }

    /// The lyric being typed, over its note. Enter or Tab keeps it and moves
    /// on to the next note; Esc drops it.
    fn lyric_overlay(&mut self, ui: &Ui, g: &Geom) {
        let Some((id, mut text)) = self.lyric_edit.clone() else {
            return;
        };
        let Some(n) = self.pattern().notes.iter().find(|n| n.id == id).copied() else {
            self.lyric_edit = None;
            return;
        };
        let r = g.note_rect(n.start, n.length, n.pitch);
        let key = egui::Id::new("lyric-edit-focus");
        let first = ui.ctx().data(|d| d.get_temp::<u64>(key)) != Some(id);
        let (mut done, mut cancel, mut next) = (false, false, false);
        let lyric_hint = self.tr().roll_lyric_hint;
        egui::Area::new(egui::Id::new(("lyric-edit", id)))
            .order(egui::Order::Foreground)
            .fixed_pos(r.left_top())
            .show(ui.ctx(), |ui| {
                let mut out = egui::TextEdit::singleline(&mut text)
                    .desired_width(r.width().max(96.0))
                    .font(t::semibold(13.0))
                    .hint_text(lyric_hint)
                    .show(ui);
                let resp = out.response.clone();
                if first {
                    // Typing replaces the old lyric.
                    resp.request_focus();
                    let n = text.chars().count();
                    out.state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(n),
                        )));
                    out.state.store(ui.ctx(), resp.id);
                    ui.ctx().data_mut(|d| d.insert_temp(key, id));
                } else if resp.lost_focus() {
                    let (enter, tab, esc) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::Enter),
                            i.key_pressed(egui::Key::Tab),
                            i.key_pressed(egui::Key::Escape),
                        )
                    });
                    done = true;
                    cancel = esc;
                    next = enter || tab;
                }
            });
        if done {
            ui.ctx().data_mut(|d| d.remove::<u64>(key));
            self.lyric_edit = None;
            if !cancel {
                self.set_lyric(id, &text);
                if next {
                    let order = self.vocal_notes_in_order();
                    if let Some(i) = order.iter().position(|x| *x == id)
                        && let Some(&following) = order.get(i + 1)
                    {
                        self.roll_sel = vec![following];
                        self.start_lyric_edit(following);
                    }
                }
            }
        } else {
            self.lyric_edit = Some((id, text));
        }
    }

    fn slide_at(&self, at: Tick, pitch: u8) -> Option<usize> {
        let me = self.roll_inst;
        self.pattern().slides.iter().rposition(|s| {
            s.instrument == me && s.target == pitch && at >= s.start && at < s.start + s.length
        })
    }

    /// This channel's slide under `q` and which part was hit.
    fn slide_hit(&self, g: &Geom, q: Pos2) -> Option<(usize, MoveMode)> {
        let me = self.roll_inst;
        let slides = &self.pattern().slides;
        let i = slides.iter().rposition(|s| {
            s.instrument == me
                && g.visible(s.target)
                && g.note_rect(s.start, s.length, s.target).contains(q)
        })?;
        let s = slides[i];
        let probe = Note::new(me, s.start, s.length, s.target, 100);
        Some((i, Self::edge_mode(g, &probe, q.x)))
    }

    fn edge_mode(g: &Geom, n: &Note, x: f32) -> MoveMode {
        let (a, b) = (g.x(n.start), g.x(n.end()));
        if b - a > EDGE_GRAB * 2.5 {
            if b - x < EDGE_GRAB {
                return MoveMode::ResizeRight;
            }
            if x - a < EDGE_GRAB * 0.8 {
                return MoveMode::ResizeLeft;
            }
        } else if b - x < EDGE_GRAB * 0.6 && b - a > 6.0 {
            return MoveMode::ResizeRight;
        }
        MoveMode::Move
    }

    fn roll_input(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        g: &Geom,
        resp: &egui::Response,
        scrollbar_busy: bool,
    ) {
        let (alt, shift, ctrl) =
            ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.command));
        let grid = self.snap.grid();
        let me = self.roll_inst;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let (pressed, secondary_pressed, down, secondary_down) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
            )
        });
        let can_start = ui.is_enabled() && !self.welcome_open && !scrollbar_busy;
        let over = can_start && pointer.is_some_and(|q| g.grid.contains(q)) && resp.hovered();

        // Cursor tells what a press will do.
        if can_start
            && self.roll_gesture.is_none()
            && let Some(hp) = resp.hover_pos()
            && g.grid.contains(hp)
        {
            let hit = self.note_at(g, hp);
            let on_slide = if hit.is_none() && matches!(self.tool, Tool::Draw | Tool::Slide) {
                self.slide_hit(g, hp)
            } else {
                None
            };
            let icon = match (on_slide, self.tool) {
                (Some((_, MoveMode::Move)), _) => CursorIcon::Grab,
                (Some(_), _) => CursorIcon::ResizeHorizontal,
                (None, tool) => match tool {
                    Tool::Draw | Tool::Paint | Tool::Select => match hit {
                        Some(n) => match Self::edge_mode(g, &n, hp.x) {
                            MoveMode::Move => CursorIcon::Grab,
                            _ => CursorIcon::ResizeHorizontal,
                        },
                        None if ctrl || self.tool == Tool::Select => CursorIcon::Crosshair,
                        None => CursorIcon::Default,
                    },
                    Tool::Delete if hit.is_some() => CursorIcon::NotAllowed,
                    Tool::Mute if hit.is_some() => CursorIcon::PointingHand,
                    Tool::Slice => CursorIcon::ResizeColumn,
                    Tool::Zoom => CursorIcon::ZoomIn,
                    _ => CursorIcon::Default,
                },
            };
            ui.ctx().set_cursor_icon(icon);
        }

        // ---- press: start a gesture
        if pressed && over && self.roll_gesture.as_ref().is_none_or(|g| g.0 != me) {
            let q = pointer.unwrap();
            let (at, pitch) = g.locate_clamped(q);
            let pitch = self.scale_pitch(pitch);
            let hit = self.note_at(g, q);
            let slide = if hit.is_none() && matches!(self.tool, Tool::Draw | Tool::Slide) && !ctrl {
                self.slide_hit(g, q)
            } else {
                None
            };
            let gesture = if (ctrl && self.tool == Tool::Draw)
                || (self.tool == Tool::Select && hit.is_none())
            {
                Some(Gesture::Select {
                    from_tick: g.tick_f(q.x),
                    from_pitch: g.pitch_f32(q.y),
                    add: shift,
                    base: if shift {
                        self.roll_sel.clone()
                    } else {
                        Vec::new()
                    },
                })
            } else if let Some((index, mode)) = slide {
                self.checkpoint();
                let orig = self.pattern().slides[index];
                Some(Gesture::SlideEdit {
                    index,
                    mode,
                    at,
                    pitch,
                    orig,
                })
            } else if self.tool == Tool::Slide {
                Some(Gesture::Slide { at, pitch })
            } else if self.tool == Tool::Delete {
                Some(Gesture::Erase)
            } else if self.tool == Tool::Mute {
                Some(Gesture::Mute {
                    to: hit.map(|n| !n.muted).unwrap_or(true),
                })
            } else if self.tool == Tool::Slice {
                Some(Gesture::Slice { from: q })
            } else if self.tool == Tool::Zoom {
                Some(Gesture::Zoom { from: q })
            } else if let Some(n) = hit {
                let mode = Self::edge_mode(g, &n, q.x);
                if !self.roll_sel.contains(&n.id) {
                    self.roll_sel = vec![n.id];
                }
                let mut orig = self.selected_notes();
                self.checkpoint();
                let duplicate = shift && mode == MoveMode::Move;
                if duplicate {
                    // The copies move; the originals stay.
                    let mut ids = Vec::new();
                    let pid = self.selected;
                    for o in &mut orig {
                        let id = self.song.add_note(pid, *o);
                        ids.push(id);
                        o.id = id;
                    }
                    self.roll_sel = ids;
                }
                let anchor = orig
                    .iter()
                    .find(|o| o.start == n.start && o.pitch == n.pitch)
                    .map(|o| o.id)
                    .unwrap_or(n.id);
                if mode == MoveMode::Move {
                    // A singing note previews its own syllable.
                    let lyric = self
                        .pattern()
                        .lyrics
                        .get(&n.id)
                        .map(|l| keyframexl::vocal::lyric_hash(l))
                        .unwrap_or(0);
                    self.hold_note_sung(me, n.pitch, n.velocity, lyric);
                }
                Some(Gesture::Edit {
                    mode,
                    anchor,
                    at,
                    pitch,
                    orig,
                    duplicate,
                })
            } else {
                // Empty cell: place a note of the last length; the drag moves it.
                let start = snap_floor(at, grid, alt);
                let len = self.last_len.unwrap_or(grid.max(STEP / 4)).max(1);
                self.checkpoint();
                let pid = self.selected;
                let note = Note {
                    color: self.note_color,
                    porta: self.porta_new,
                    ..Note::new(me, start, len, pitch, self.velocity)
                };
                let id = self.song.add_note(pid, note);
                self.fit_pattern();
                self.roll_sel = vec![id];
                self.hold_note(me, pitch, self.velocity);
                if self.tool == Tool::Paint {
                    Some(Gesture::Paint {
                        len,
                        next: start + len,
                    })
                } else {
                    Some(Gesture::Edit {
                        mode: MoveMode::Move,
                        anchor: id,
                        at,
                        pitch,
                        orig: vec![Note { id, ..note }],
                        duplicate: false,
                    })
                }
            };
            if let Some(gs) = gesture {
                self.roll_gesture = Some((me, gs));
            }
        }
        if secondary_pressed && over && self.roll_gesture.is_none() {
            if self.tool == Tool::Zoom {
                let x = pointer.map(|q| q.x).unwrap_or(g.grid.center().x);
                self.zoom_roll(g, x, x, 0.5);
            } else {
                self.roll_gesture = Some((me, Gesture::Erase));
            }
        }

        // ---- hold: apply the gesture live
        let Some((owner, gesture)) = self.roll_gesture.clone() else {
            return;
        };
        if owner != me {
            return;
        }
        let q = pointer.unwrap_or_default();
        let (at, pitch) = g.locate_clamped(q);
        match &gesture {
            Gesture::Edit {
                mode,
                anchor,
                at: from,
                pitch: from_pitch,
                orig,
                ..
            } if down => {
                let Some(a) = orig.iter().find(|o| o.id == *anchor).copied() else {
                    return;
                };
                let min = if alt { 1 } else { grid.max(1) };
                let lo_start = orig.iter().map(|o| o.start).min().unwrap_or(0);
                match mode {
                    MoveMode::Move => {
                        let target = snap_round(a.start + (at - from), grid, alt);
                        let dt = (target - a.start).max(-lo_start);
                        let dp_raw = pitch as i16 - *from_pitch as i16;
                        // On a scale the grabbed note lands on it; a chord keeps its shape.
                        let dp_raw = self.scale_pitch((a.pitch as i16 + dp_raw).clamp(0, 127) as u8)
                            as i16
                            - a.pitch as i16;
                        let lo_p = orig.iter().map(|o| o.pitch).min().unwrap_or(0) as i16;
                        let hi_p = orig.iter().map(|o| o.pitch).max().unwrap_or(127) as i16;
                        let dp = dp_raw.clamp(-lo_p, 127 - hi_p);
                        for o in orig {
                            if let Some(n) = self.note_mut(o.id) {
                                n.start = o.start + dt;
                                n.pitch = (o.pitch as i16 + dp) as u8;
                            }
                        }
                        if self.held.is_some() {
                            self.hold_note(me, (a.pitch as i16 + dp) as u8, a.velocity);
                        }
                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                    }
                    MoveMode::ResizeRight => {
                        let end = snap_round(at.max(a.start), grid, alt).max(a.start + min);
                        let d = end - a.end();
                        for o in orig {
                            if let Some(n) = self.note_mut(o.id) {
                                n.length = (o.length + d).max(min.min(o.length));
                            }
                        }
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    }
                    MoveMode::ResizeLeft => {
                        let s = snap_round(at, grid, alt).clamp(0, a.end() - min);
                        let d = (s - a.start).max(-lo_start);
                        for o in orig {
                            if let Some(n) = self.note_mut(o.id) {
                                let ns = (o.start + d).min(o.end() - 1).max(0);
                                n.length = o.end() - ns;
                                n.start = ns;
                            }
                        }
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    }
                }
            }
            Gesture::Slide {
                at: from,
                pitch: sp,
            } if down => {
                let a = snap_floor((*from).min(at), grid, alt);
                let b = if alt {
                    (*from).max(at).max(a + 1)
                } else {
                    snap_floor((*from).max(at), grid, false) + grid
                };
                p.rect_stroke(
                    g.note_rect(a, b - a, *sp),
                    4.0,
                    Stroke::new(1.5_f32, t::text()),
                    egui::StrokeKind::Inside,
                );
            }
            Gesture::SlideEdit {
                index,
                mode,
                at: from,
                pitch: from_pitch,
                orig,
            } if down => {
                let min = if alt { 1 } else { grid.max(1) };
                let mut s = *orig;
                match mode {
                    MoveMode::Move => {
                        s.start = snap_round(orig.start + (at - from), grid, alt).max(0);
                        s.target = (orig.target as i16 + pitch as i16 - *from_pitch as i16)
                            .clamp(0, 127) as u8;
                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                    }
                    MoveMode::ResizeRight => {
                        let end = snap_round(at, grid, alt).max(orig.start + min);
                        s.length = end - orig.start;
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    }
                    MoveMode::ResizeLeft => {
                        let end = orig.start + orig.length;
                        s.start = snap_round(at, grid, alt).clamp(0, end - min);
                        s.length = end - s.start;
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    }
                }
                if let Some(slot) = self.pattern_mut().slides.get_mut(*index) {
                    *slot = s;
                }
            }
            Gesture::Paint { len, next } if down => {
                let target = snap_floor(at, grid, alt);
                let mut nx = *next;
                let pid = self.selected;
                while nx <= target && nx + len <= MAX_BARS * BAR {
                    let note = Note {
                        color: self.note_color,
                        porta: self.porta_new,
                        ..Note::new(me, nx, *len, pitch, self.velocity)
                    };
                    self.song.add_note(pid, note);
                    nx += len;
                }
                if nx != *next {
                    self.roll_gesture = Some((
                        me,
                        Gesture::Paint {
                            len: *len,
                            next: nx,
                        },
                    ));
                    self.fit_pattern();
                }
            }
            Gesture::Mute { to } if down => {
                if let Some(n) = self.note_at(g, q)
                    && n.muted != *to
                {
                    self.erase_once();
                    if let Some(m) = self.note_mut(n.id) {
                        m.muted = *to;
                    }
                }
            }
            Gesture::Slice { from } if down => {
                let x = g.x(snap_round(at, grid, alt));
                p.line_segment(
                    [
                        pos2(x, from.y.min(q.y) - g.row / 2.0),
                        pos2(x, from.y.max(q.y) + g.row / 2.0),
                    ],
                    Stroke::new(1.5_f32, t::text()),
                );
            }
            Gesture::Zoom { from } if down => {
                let r = Rect::from_x_y_ranges(from.x.min(q.x)..=from.x.max(q.x), g.grid.y_range());
                p.rect_filled(r, 0.0, t::hi());
                p.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.0_f32, t::strong()),
                    egui::StrokeKind::Inside,
                );
            }
            Gesture::Select {
                from_tick,
                from_pitch,
                add,
                base,
            } if down => {
                let cur_tick = g.tick_f(q.x);
                let cur_pitch = g.pitch_f32(q.y);
                let (t0, t1) = ((*from_tick).min(cur_tick), (*from_tick).max(cur_tick));
                let (p0, p1) = (from_pitch.min(cur_pitch), from_pitch.max(cur_pitch));
                let hits: Vec<u64> = self
                    .pattern()
                    .notes
                    .iter()
                    .filter(|n| {
                        n.instrument == me
                            && n.end() > t0
                            && n.start <= t1
                            && (n.pitch as f32 + 1.0) > p0
                            && (n.pitch as f32) < p1
                    })
                    .map(|n| n.id)
                    .collect();
                self.roll_sel = if *add { base.clone() } else { Vec::new() };
                for id in hits {
                    if !self.roll_sel.contains(&id) {
                        self.roll_sel.push(id);
                    }
                }
            }
            Gesture::Erase
                if (secondary_down || (self.tool == Tool::Delete && down))
                    && g.grid.contains(q) =>
            {
                if self.tool == Tool::Slide {
                    if let Some(i) = self.slide_at(at, pitch) {
                        self.erase_once();
                        self.pattern_mut().slides.remove(i);
                    }
                } else if let Some(n) = self.note_at(g, q) {
                    self.erase_once();
                    self.pattern_mut().notes.retain(|x| x.id != n.id);
                    self.roll_sel.retain(|x| *x != n.id);
                }
            }
            _ => {}
        }

        // ---- release: commit
        let released = match gesture {
            Gesture::Erase => !secondary_down && !(self.tool == Tool::Delete && down),
            _ => !down,
        };
        if !released {
            return;
        }
        self.roll_gesture = None;
        self.release_note();
        match gesture {
            Gesture::Edit {
                mode,
                anchor,
                orig,
                duplicate,
                ..
            } => {
                let now: Vec<Note> = orig
                    .iter()
                    .filter_map(|o| self.pattern().notes.iter().find(|n| n.id == o.id).copied())
                    .collect();
                let moved = now
                    .iter()
                    .zip(&orig)
                    .any(|(n, o)| (n.start, n.length, n.pitch) != (o.start, o.length, o.pitch));
                let created = !duplicate
                    && orig.len() == 1
                    && self.history.last().is_some_and(|h| {
                        !h.patterns
                            .iter()
                            .any(|p| p.notes.iter().any(|n| n.id == anchor))
                    });
                if !moved && !created {
                    // A click on a note: selection only. Undo the copies of a Shift+click.
                    if duplicate && let Some(prev) = self.history.pop() {
                        self.song = prev;
                        self.roll_sel = orig.iter().map(|o| o.id).collect();
                    } else {
                        self.history.pop();
                    }
                    return;
                }
                self.fit_pattern();
                if let Some(a) = now.iter().find(|n| n.id == anchor) {
                    self.last_len = Some(a.length);
                }
                self.publish();
                let a = now
                    .iter()
                    .find(|n| n.id == anchor)
                    .copied()
                    .unwrap_or(orig[0]);
                let tr = self.tr();
                let many = if now.len() > 1 {
                    tr.roll_notes_count.fmt(tr.lang, now.len())
                } else {
                    note_name(a.pitch)
                };
                let text = if created && !moved {
                    crate::i18n::fill(
                        tr.roll_note_placed,
                        &[
                            ("note", &note_name(a.pitch)),
                            ("pos", &bar_pos(a.start)),
                            ("len", &tr.length_text(a.length)),
                        ],
                    )
                } else if duplicate {
                    crate::i18n::fill(
                        tr.roll_notes_copied_to,
                        &[("what", &many), ("pos", &bar_pos(a.start))],
                    )
                } else {
                    match mode {
                        MoveMode::Move => crate::i18n::fill(
                            tr.roll_notes_moved_to,
                            &[("what", &many), ("pos", &bar_pos(a.start))],
                        ),
                        _ => crate::i18n::fill(
                            tr.roll_notes_resized,
                            &[("what", &many), ("len", &tr.length_text(a.length))],
                        ),
                    }
                };
                self.say(Tone::Quiet, text);
            }
            Gesture::Select {
                from_tick,
                from_pitch,
                add,
                base,
            } => {
                let cur_tick = g.tick_f(q.x);
                let cur_pitch = g.pitch_f32(q.y);
                let (t0, t1) = (from_tick.min(cur_tick), from_tick.max(cur_tick));
                let (p0, p1) = (from_pitch.min(cur_pitch), from_pitch.max(cur_pitch));
                let hits: Vec<u64> = self
                    .pattern()
                    .notes
                    .iter()
                    .filter(|n| {
                        n.instrument == me
                            && n.end() > t0
                            && n.start <= t1
                            && (n.pitch as f32 + 1.0) > p0
                            && (n.pitch as f32) < p1
                    })
                    .map(|n| n.id)
                    .collect();
                self.roll_sel = if add { base } else { Vec::new() };
                for id in hits {
                    if !self.roll_sel.contains(&id) {
                        self.roll_sel.push(id);
                    }
                }
                let tr = self.tr();
                let msg = tr.roll_notes_selected.fmt(tr.lang, self.roll_sel.len());
                self.say(Tone::Quiet, msg);
            }
            Gesture::Erase => {
                if self.erasing {
                    self.erasing = false;
                    self.publish();
                    self.say(Tone::Quiet, self.tr().roll_erased);
                }
            }
            Gesture::Mute { to } => {
                if self.erasing {
                    self.erasing = false;
                    self.publish();
                    let tr = self.tr();
                    self.say(
                        Tone::Quiet,
                        if to {
                            tr.roll_notes_muted_status
                        } else {
                            tr.roll_notes_unmuted_status
                        },
                    );
                }
            }
            Gesture::SlideEdit { index, orig, .. } => {
                let now = self.pattern().slides.get(index).copied();
                if now == Some(orig) {
                    self.history.pop();
                    let msg = crate::i18n::fill(
                        self.tr().roll_slide_help,
                        &[("note", &note_name(orig.target))],
                    );
                    self.say(Tone::Quiet, msg);
                } else if let Some(s) = now {
                    self.fit_pattern();
                    self.publish();
                    let tr = self.tr();
                    let msg = crate::i18n::fill(
                        tr.roll_slide_edited,
                        &[
                            ("note", &note_name(s.target)),
                            ("pos", &bar_pos(s.start)),
                            ("len", &tr.length_text(s.length)),
                        ],
                    );
                    self.say(Tone::Quiet, msg);
                }
            }
            Gesture::Paint { len, next } => {
                self.fit_pattern();
                self.publish();
                self.last_len = Some(len);
                let msg = crate::i18n::fill(
                    self.tr().roll_painted,
                    &[("note", &note_name(pitch)), ("pos", &bar_pos(next))],
                );
                self.say(Tone::Quiet, msg);
            }
            Gesture::Slice { from } => {
                let cut = snap_round(at, grid, alt);
                let (_, p0) = g.locate_clamped(from);
                self.checkpoint();
                let n = self.slice_notes_at(cut, p0.min(pitch)..=p0.max(pitch));
                if n == 0 {
                    self.history.pop();
                    self.say(Tone::Quiet, self.tr().roll_slice_none);
                } else {
                    self.publish();
                    let tr = self.tr();
                    let msg = tr
                        .roll_notes_cut_at
                        .fmt_with(tr.lang, n, &[("pos", &bar_pos(cut))]);
                    self.say(Tone::Quiet, msg);
                }
            }
            Gesture::Zoom { from } => self.zoom_roll(g, from.x, q.x, 2.0),
            Gesture::Slide {
                at: from,
                pitch: sp,
            } => {
                let a = snap_floor(from.min(at), grid, alt);
                let b = if alt {
                    from.max(at).max(a + 1)
                } else {
                    snap_floor(from.max(at), grid, false) + grid
                };
                self.checkpoint();
                self.pattern_mut().slides.push(Glide {
                    start: a,
                    length: b - a,
                    target: sp,
                    group: 0,
                    instrument: me,
                });
                self.fit_pattern();
                self.publish();
                let msg = crate::i18n::fill(
                    self.tr().roll_slide_added,
                    &[("note", &note_name(sp)), ("pos", &bar_pos(a))],
                );
                self.say(Tone::Quiet, msg);
            }
        }
    }

    /// Zoom tool: a drawn span fills the view; a click zooms by `factor` there.
    fn zoom_roll(&mut self, g: &Geom, a: f32, b: f32, factor: f32) {
        let me = self.roll_inst;
        let Some(v) = self.roll_view.get_mut(&me) else {
            return;
        };
        let tick_at = |x: f32| v.x0 + (x - g.grid.left()) / v.cell * STEP as f32;
        let (lo, hi) = (a.min(b), a.max(b));
        if hi - lo >= 6.0 {
            let (t0, t1) = (tick_at(lo), tick_at(hi));
            v.cell = (g.grid.width() / ((t1 - t0) / STEP as f32)).clamp(CELL_MIN, CELL_MAX);
            v.x0 = t0.max(0.0);
        } else {
            let t = tick_at(lo);
            v.cell = (v.cell * factor).clamp(CELL_MIN, CELL_MAX);
            v.x0 = (t - (lo - g.grid.left()) / v.cell * STEP as f32).max(0.0);
        }
    }

    /// First erase of a right-drag takes the undo step.
    fn erase_once(&mut self) {
        if !self.erasing {
            self.checkpoint();
            self.erasing = true;
        }
    }

    // ---------------------------------------------------------------- note commands

    /// Toggle porta on the selection (all on unless all already are).
    pub(crate) fn porta_notes(&mut self) {
        if self.roll_sel.is_empty() {
            self.say(Tone::Quiet, self.tr().roll_porta_select_first);
            return;
        }
        self.checkpoint();
        let sel = self.roll_sel.clone();
        let on = !self
            .pattern()
            .notes
            .iter()
            .filter(|n| sel.contains(&n.id))
            .all(|n| n.porta);
        for n in self
            .pattern_mut()
            .notes
            .iter_mut()
            .filter(|n| sel.contains(&n.id))
        {
            n.porta = on;
        }
        self.publish();
        let tr = self.tr();
        let msg = if on {
            tr.roll_porta_on_notes.fmt(tr.lang, sel.len())
        } else {
            tr.roll_porta_off_notes.fmt(tr.lang, sel.len())
        };
        self.say(Tone::Quiet, msg);
    }

    /// Colour for new notes; recolours the selection too (one undo step).
    pub(crate) fn set_note_color(&mut self, c: u8) {
        self.note_color = c;
        let tr = self.tr();
        let color = tr.note_color_name(c).to_lowercase();
        if self.roll_sel.is_empty() {
            let msg = crate::i18n::fill(tr.roll_new_notes_color, &[("color", &color)]);
            self.say(Tone::Quiet, msg);
            return;
        }
        self.checkpoint();
        let sel = self.roll_sel.clone();
        for n in self
            .pattern_mut()
            .notes
            .iter_mut()
            .filter(|n| sel.contains(&n.id))
        {
            n.color = c;
        }
        self.publish();
        let msg = tr
            .roll_notes_coloured
            .fmt_with(tr.lang, sel.len(), &[("color", &color)]);
        self.say(Tone::Quiet, msg);
    }

    /// Cut this channel's notes that cross `at` in two (Slice tool). Returns the count.
    pub(crate) fn slice_notes_at(
        &mut self,
        at: Tick,
        pitches: std::ops::RangeInclusive<u8>,
    ) -> usize {
        let me = self.roll_inst;
        let pid = self.selected;
        let cut: Vec<Note> = self
            .pattern()
            .notes
            .iter()
            .filter(|n| {
                n.instrument == me && pitches.contains(&n.pitch) && n.start < at && n.end() > at
            })
            .copied()
            .collect();
        for n in &cut {
            if let Some(m) = self.note_mut(n.id) {
                m.length = at - n.start;
            }
            self.song.add_note(
                pid,
                Note {
                    start: at,
                    length: n.end() - at,
                    ..*n
                },
            );
        }
        cut.len()
    }

    pub(crate) fn select_all_notes(&mut self) {
        let me = self.roll_inst;
        self.roll_sel = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me)
            .map(|n| n.id)
            .collect();
    }

    pub(crate) fn delete_selected_notes(&mut self) {
        if self.roll_sel.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = std::mem::take(&mut self.roll_sel);
        self.pattern_mut().notes.retain(|n| !sel.contains(&n.id));
        self.publish();
        let tr = self.tr();
        let msg = tr.roll_notes_deleted.fmt(tr.lang, sel.len());
        self.say(Tone::Quiet, msg);
    }

    pub(crate) fn copy_notes(&mut self, cut: bool) {
        let notes = self.selected_notes();
        if notes.is_empty() {
            return;
        }
        self.note_clip = notes.clone();
        if cut {
            self.delete_selected_notes();
        } else {
            let tr = self.tr();
            let msg = tr.roll_notes_copied.fmt(tr.lang, notes.len());
            self.say(Tone::Quiet, msg);
        }
    }

    /// Paste where the notes were copied from, into this roll's channel; the
    /// pasted notes become the selection.
    pub(crate) fn paste_notes(&mut self) {
        if self.note_clip.is_empty() {
            self.say(Tone::Quiet, self.tr().roll_nothing_to_paste);
            return;
        }
        self.checkpoint();
        let me = self.roll_inst;
        let pid = self.selected;
        let mut ids = Vec::new();
        for n in self.note_clip.clone() {
            ids.push(self.song.add_note(
                pid,
                Note {
                    instrument: me,
                    ..n
                },
            ));
        }
        self.fit_pattern();
        self.roll_sel = ids;
        self.publish();
        let tr = self.tr();
        let msg = tr.roll_notes_pasted.fmt(tr.lang, self.roll_sel.len());
        self.say(Tone::Quiet, msg);
    }

    /// FL Ctrl+B: copy the selection right after itself.
    pub(crate) fn duplicate_notes(&mut self) {
        let notes = self.selected_notes();
        let (Some(a), Some(b)) = (
            notes.iter().map(|n| n.start).min(),
            notes.iter().map(|n| n.end()).max(),
        ) else {
            return;
        };
        let grid = self.snap.grid().max(1);
        let span = ((b - a + grid - 1) / grid) * grid;
        self.checkpoint();
        let pid = self.selected;
        let mut ids = Vec::new();
        for n in notes {
            ids.push(self.song.add_note(
                pid,
                Note {
                    start: n.start + span,
                    ..n
                },
            ));
        }
        self.fit_pattern();
        self.roll_sel = ids;
        self.publish();
        let tr = self.tr();
        let msg = tr.roll_notes_duplicated.fmt_with(
            tr.lang,
            self.roll_sel.len(),
            &[("pos", &bar_pos(a + span))],
        );
        self.say(Tone::Quiet, msg);
    }

    /// Move the selection by ticks and semitones; `dlen` changes lengths.
    pub(crate) fn nudge_notes(&mut self, dt: Tick, dp: i16, dlen: Tick) {
        let notes = self.selected_notes();
        if notes.is_empty() {
            return;
        }
        let lo_start = notes.iter().map(|n| n.start).min().unwrap_or(0);
        let dt = dt.max(-lo_start);
        let lo_p = notes.iter().map(|n| n.pitch).min().unwrap_or(0) as i16;
        let hi_p = notes.iter().map(|n| n.pitch).max().unwrap_or(127) as i16;
        let dp = dp.clamp(-lo_p, 127 - hi_p);
        if dt == 0 && dp == 0 && dlen == 0 {
            return;
        }
        self.checkpoint();
        for o in notes {
            if let Some(n) = self.note_mut(o.id) {
                n.start = o.start + dt;
                n.pitch = (o.pitch as i16 + dp) as u8;
                n.length = (o.length + dlen).max(1);
            }
        }
        self.fit_pattern();
        self.publish();
    }

    /// Ctrl+Q: note starts to the snap grid (the selection, else every note of the channel).
    pub(crate) fn quantize_notes(&mut self) {
        let grid = self.snap.grid();
        if grid <= 1 {
            self.say(Tone::Quiet, self.tr().roll_snap_off_quantize);
            return;
        }
        let me = self.roll_inst;
        let sel = self.roll_sel.clone();
        self.checkpoint();
        let mut moved = 0;
        for n in self
            .pattern_mut()
            .notes
            .iter_mut()
            .filter(|n| n.instrument == me && (sel.is_empty() || sel.contains(&n.id)))
        {
            let q = snap_round(n.start, grid, false);
            if q != n.start {
                n.start = q;
                moved += 1;
            }
        }
        if moved == 0 {
            self.history.pop();
            self.say(Tone::Quiet, self.tr().roll_already_on_grid);
            return;
        }
        self.fit_pattern();
        self.publish();
        let tr = self.tr();
        let msg =
            tr.roll_notes_quantized
                .fmt_with(tr.lang, moved, &[("snap", tr.snap_label(self.snap))]);
        self.say(Tone::Quiet, msg);
    }

    pub(crate) fn mute_notes(&mut self) {
        let sel = self.roll_sel.clone();
        if sel.is_empty() {
            return;
        }
        self.checkpoint();
        let all_muted = self.selected_notes().iter().all(|n| n.muted);
        for n in self
            .pattern_mut()
            .notes
            .iter_mut()
            .filter(|n| sel.contains(&n.id))
        {
            n.muted = !all_muted;
        }
        self.publish();
        let tr = self.tr();
        let msg = if all_muted {
            tr.roll_notes_unmuted_n.fmt(tr.lang, sel.len())
        } else {
            tr.roll_notes_muted_n.fmt(tr.lang, sel.len())
        };
        self.say(Tone::Quiet, msg);
    }

    /// Keys while this piano roll has focus.
    fn roll_keys(&mut self, ui: &Ui, _g: &Geom) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }
        let grid = self.snap.grid().max(1);
        let (m, events) = ui.input(|i| (i.modifiers, i.events.clone()));
        let pressed = |k: egui::Key| ui.input(|i| i.key_pressed(k));
        for e in &events {
            match e {
                egui::Event::Copy => self.copy_notes(false),
                egui::Event::Cut => self.copy_notes(true),
                egui::Event::Paste(_) => self.paste_notes(),
                _ => {}
            }
        }
        if m.command {
            if pressed(egui::Key::A) {
                self.select_all_notes();
            }
            if pressed(egui::Key::D) {
                self.roll_sel.clear();
            }
            if pressed(egui::Key::B) {
                self.duplicate_notes();
            }
            if pressed(egui::Key::Q) {
                self.quantize_notes();
            }
            if pressed(egui::Key::L) {
                self.legato_notes();
            }
            if pressed(egui::Key::U) {
                self.chop_notes();
            }
            if pressed(egui::Key::G) {
                self.glue_notes();
            }
            if pressed(egui::Key::C) {
                self.copy_notes(false);
            }
            if pressed(egui::Key::X) {
                self.copy_notes(true);
            }
            if pressed(egui::Key::V) {
                self.paste_notes();
            }
        }
        if pressed(egui::Key::F2) && self.is_vocal(self.roll_inst) {
            let first = self
                .vocal_notes_in_order()
                .into_iter()
                .find(|id| self.roll_sel.contains(id));
            if let Some(id) = first {
                self.start_lyric_edit(id);
            }
        }
        if m.alt && !m.command && pressed(egui::Key::Q) {
            self.open_quantize(ui.ctx());
        }
        if m.alt && !m.command && pressed(egui::Key::V) {
            self.ghosts = !self.ghosts;
            let tr = self.tr();
            self.say(
                Tone::Quiet,
                if self.ghosts {
                    tr.roll_ghosts_on
                } else {
                    tr.roll_ghosts_off
                },
            );
        }
        if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
            self.delete_selected_notes();
        }
        let oct = if m.command { 12 } else { 1 };
        if pressed(egui::Key::ArrowUp) {
            self.nudge_notes(0, oct, 0);
        }
        if pressed(egui::Key::ArrowDown) {
            self.nudge_notes(0, -oct, 0);
        }
        let step = if m.alt { 1 } else { grid };
        for (key, sign) in [(egui::Key::ArrowLeft, -1), (egui::Key::ArrowRight, 1)] {
            // Ctrl+Alt+arrows jump between song markers, not notes.
            if pressed(key) && !(m.command && m.alt) {
                if m.shift {
                    self.nudge_notes(0, 0, sign * step);
                } else {
                    self.nudge_notes(sign * step, 0, 0);
                }
            }
        }
    }

    // ---------------------------------------------------------------- event lane

    /// Stems at note starts for the target control (FL event editor). Click
    /// or drag sets the nearest stem; with a selection, only selected notes.
    /// Alt+click resets. Click the caption to switch the target.
    fn event_lane(&mut self, ui: &Ui, p: &egui::Painter, g: &Geom) {
        let lane = g.lane;
        let well = kit::sunken(p, lane, 8.0);
        let target = self.lane_target;
        // Caption: click cycles the target, right-click lists them.
        let cap = Rect::from_min_size(well.left_top() + vec2(4.0, 4.0), vec2(KEYS_W - 6.0, 20.0));
        let cr = ui.interact(cap, ui.id().with("lane-target"), Sense::click());
        if cr.hovered() {
            p.rect_filled(cap, 4.0, t::raised());
        }
        let tr = self.tr();
        let target_label = tr.lane_target_label(target);
        kit::caps(
            p,
            cap.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            tr.lane_target_short(target),
            11.0,
            if cr.hovered() { t::text() } else { t::muted() },
        );
        let cr = cr.on_hover_text(crate::i18n::fill(
            tr.roll_lane_target_tip,
            &[("label", target_label)],
        ));
        if cr.clicked() {
            let i = LaneTarget::ALL
                .iter()
                .position(|x| *x == target)
                .unwrap_or(0);
            self.lane_target = LaneTarget::ALL[(i + 1) % LaneTarget::ALL.len()];
        }
        cr.context_menu(|ui| {
            kit::menu_style(ui);
            for lt in LaneTarget::ALL {
                if ui
                    .selectable_label(self.lane_target == lt, tr.lane_target_label(lt))
                    .clicked()
                {
                    self.lane_target = lt;
                    ui.close_menu();
                }
            }
        });
        let area = Rect::from_min_max(
            pos2(g.grid.left(), well.top() + 8.0),
            pos2(g.grid.right(), well.bottom() - 6.0),
        );
        let lp = p.with_clip_rect(area.expand2(vec2(0.0, 6.0)));
        if target == LaneTarget::Pitch {
            self.pitch_lane(ui, &lp, g, area);
            return;
        }
        for v in [0.25f32, 0.5, 0.75, 1.0] {
            let y = area.bottom() - v * area.height();
            lp.line_segment(
                [pos2(area.left(), y), pos2(area.right(), y)],
                Stroke::new(
                    1.0_f32,
                    t::line().gamma_multiply(if v == 0.5 && target.centred() {
                        1.0
                    } else {
                        0.5
                    }),
                ),
            );
        }
        let me = self.roll_inst;
        let sel = self.roll_sel.clone();
        let base = if target.centred() {
            area.center().y
        } else {
            area.bottom()
        };
        for n in self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me && n.start + n.length >= g.x0 && n.start <= g.end())
        {
            let x = g.x(n.start) + 3.0;
            let y = area.bottom() - target.get(n) * area.height();
            let on = sel.is_empty() || sel.contains(&n.id);
            let col = if on { t::brand() } else { t::strong() };
            lp.line_segment([pos2(x, base), pos2(x, y)], Stroke::new(2.0_f32, col));
            lp.circle_filled(pos2(x, y), 4.0, col);
            lp.circle_filled(pos2(x, y), 1.5, t::brand_edge());
        }
        let resp = ui.interact(area, ui.id().with("event-lane"), Sense::click_and_drag());
        let Some(pos) = resp.interact_pointer_pos() else {
            if let Some(h) = resp.hover_pos()
                && let Some(n) = self.lane_note_near(g, h.x)
            {
                resp.on_hover_text(format!(
                    "{} {}: {}",
                    note_name(n.pitch),
                    target_label,
                    target.text(&n)
                ));
            }
            return;
        };
        let Some(n) = self.lane_note_near(g, pos.x) else {
            return;
        };
        if resp.drag_started() || resp.clicked() {
            self.checkpoint();
        }
        if resp.clicked() || resp.dragged() {
            let alt = ui.input(|i| i.modifiers.alt);
            let v = if alt {
                target.default_value()
            } else {
                (area.bottom() - pos.y) / area.height()
            };
            // Notes stacked on the same start (a chord) change together.
            let ids: Vec<u64> = self
                .pattern()
                .notes
                .iter()
                .filter(|x| {
                    x.instrument == me
                        && x.start == n.start
                        && (sel.is_empty() || sel.contains(&x.id))
                })
                .map(|x| x.id)
                .collect();
            for id in ids {
                if let Some(x) = self.note_mut(id) {
                    target.set(x, v);
                }
            }
            self.publish();
            let shown = self
                .pattern()
                .notes
                .iter()
                .find(|x| x.id == n.id)
                .map(|x| target.text(x))
                .unwrap_or_default();
            let msg = crate::i18n::fill(
                tr.roll_lane_set_status,
                &[
                    ("note", &note_name(n.pitch)),
                    ("pos", &bar_pos(n.start)),
                    ("label", target_label),
                    ("val", &shown),
                ],
            );
            self.say(Tone::Quiet, msg);
        }
    }

    /// The Pitch lane: each vocal note's drawn curve, in cents from the note
    /// (±6 semitones, or ±1 octave when a point is further). Click adds a point, drag moves it (snaps to the grid and
    /// to semitones, Alt frees both, Shift keeps its time), double-click
    /// removes it, right-click sets its shape or the note's vibrato.
    fn pitch_lane(&mut self, ui: &Ui, lp: &egui::Painter, g: &Geom, area: Rect) {
        use keyframexl::sequence::{CurveShape, NotePitch, NoteVibrato, PitchPoint};
        let tr = self.tr();
        let me = self.roll_inst;
        // ±6 semitones, ±1 octave once a visible point goes further.
        let wide = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me)
            .any(|n| {
                self.pattern()
                    .pitch
                    .get(&n.id)
                    .is_some_and(|c| c.points.iter().any(|p| p.cents.abs() > 600.0))
            });
        let range: f32 = if wide { 1200.0 } else { 600.0 };
        let half = area.height() / 2.0;
        let y_of = |c: f32| area.center().y - c.clamp(-range, range) / range * half;
        let c_of = |y: f32| (area.center().y - y) / half * range;
        for (c, strong) in [
            (-range, false),
            (-range / 2.0, false),
            (0.0, true),
            (range / 2.0, false),
            (range, false),
        ] {
            let y = y_of(c);
            lp.line_segment(
                [pos2(area.left(), y), pos2(area.right(), y)],
                Stroke::new(
                    1.0_f32,
                    t::line().gamma_multiply(if strong { 1.0 } else { 0.5 }),
                ),
            );
        }
        let me = self.roll_inst;
        let vocal = self
            .song
            .instrument(me)
            .is_some_and(|i| matches!(i.kind, keyframexl::sequence::InstrumentKind::Vocal { .. }));
        if !vocal {
            lp.text(
                area.center(),
                Align2::CENTER_CENTER,
                tr.roll_pitch_vocal_only,
                t::sans(12.0),
                t::faint(),
            );
            return;
        }
        let notes: Vec<Note> = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me && n.start + n.length >= g.x0 && n.start <= g.end())
            .copied()
            .collect();
        let sel = self.roll_sel.clone();
        // Where each point is on screen: (note id, point index, position).
        let mut handles: Vec<(u64, usize, Pos2)> = Vec::new();
        for n in &notes {
            let (x0, x1) = (g.x(n.start), g.x(n.start + n.length));
            let on = sel.is_empty() || sel.contains(&n.id);
            let col = if on { t::brand() } else { t::strong() };
            let curve = self.pattern().pitch.get(&n.id).cloned().unwrap_or_default();
            let mut pts = Vec::new();
            let steps = ((x1 - x0) / 3.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let f = k as f32 / steps as f32;
                let at = f as f64 * n.length as f64;
                let mut c = curve.cents_at(at, n.length) as f32;
                if let Some(v) = curve.vibrato {
                    // Sketch of the note's own vibrato over its last part.
                    let from = 1.0 - v.len.clamp(0.0, 1.0);
                    if f > from {
                        let phase = (f - from) * 14.0 * std::f32::consts::TAU;
                        c += v.depth * phase.sin();
                    }
                }
                pts.push(pos2(x0 + (x1 - x0) * f, y_of(c)));
            }
            lp.add(egui::Shape::line(
                pts,
                Stroke::new(if on { 1.8_f32 } else { 1.2 }, col),
            ));
            for (i, pt) in curve.points.iter().enumerate() {
                let pos = pos2(g.x(n.start + pt.at), y_of(pt.cents));
                lp.circle_filled(pos, 4.0, col);
                lp.circle_filled(pos, 1.5, t::brand_edge());
                handles.push((n.id, i, pos));
            }
        }
        let resp = ui.interact(area, ui.id().with("pitch-lane"), Sense::click_and_drag());
        let drag_id = ui.id().with("pitch-drag");
        let menu_id = ui.id().with("pitch-menu");
        let hit = |pos: Pos2| {
            handles
                .iter()
                .filter(|h| h.2.distance(pos) < 8.0)
                .min_by(|a, b| a.2.distance(pos).total_cmp(&b.2.distance(pos)))
                .map(|h| (h.0, h.1))
        };
        let note_at = |pos: Pos2| {
            notes
                .iter()
                .find(|n| (g.x(n.start)..=g.x(n.start + n.length)).contains(&pos.x))
                .copied()
        };
        let (alt, shift) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift));
        let grid = self.snap.grid();
        // Point (tick from the note's start, cents) under the pointer.
        let place = |n: &Note, pos: Pos2| {
            let tick = snap_round(g.tick_f(pos.x), grid, alt).clamp(n.start, n.start + n.length);
            let c = c_of(pos.y).clamp(-range, range);
            let c = if alt {
                c.round()
            } else {
                (c / 100.0).round() * 100.0
            };
            (tick - n.start, c)
        };
        if let Some(pos) = resp.hover_pos() {
            resp.clone().on_hover_text(match note_at(pos) {
                Some(n) => {
                    let cents = format!(
                        "{:+.0}",
                        self.pattern().pitch.get(&n.id).map_or(0.0, |c| {
                            c.cents_at((g.tick_f(pos.x) - n.start) as f64, n.length)
                        })
                    );
                    crate::i18n::fill(
                        tr.roll_pitch_note_tip,
                        &[("note", &note_name(n.pitch)), ("cents", &cents)],
                    )
                }
                None => crate::i18n::fill(
                    tr.roll_pitch_range_tip,
                    &[("st", &format!("{}", range / 100.0))],
                ),
            });
        }
        let mut changed = false;
        if resp.double_clicked() {
            if let Some(pos) = resp.interact_pointer_pos()
                && let Some((id, i)) = hit(pos)
            {
                self.checkpoint();
                let pat = self.pattern_mut();
                if let Some(c) = pat.pitch.get_mut(&id) {
                    if i < c.points.len() {
                        c.points.remove(i);
                    }
                    if c.is_empty() {
                        pat.pitch.remove(&id);
                    }
                }
                changed = true;
            }
        } else if (resp.drag_started() || resp.clicked())
            && let Some(pos) = resp.interact_pointer_pos()
        {
            match hit(pos) {
                Some(h) => {
                    self.checkpoint();
                    ui.data_mut(|d| d.insert_temp(drag_id, Some(h)));
                }
                None => {
                    if let Some(n) = note_at(pos) {
                        self.checkpoint();
                        let (at, cents) = place(&n, pos);
                        let c = self.pattern_mut().pitch.entry(n.id).or_default();
                        if c.points.len() < NotePitch::MAX_POINTS {
                            let i = c.points.partition_point(|p| p.at < at);
                            if c.points.get(i).is_some_and(|p| p.at == at) {
                                c.points[i].cents = cents;
                            } else {
                                c.points.insert(
                                    i,
                                    PitchPoint {
                                        at,
                                        cents,
                                        shape: CurveShape::Smooth,
                                    },
                                );
                            }
                            ui.data_mut(|d| d.insert_temp(drag_id, Some((n.id, i))));
                            changed = true;
                        }
                    }
                }
            }
        }
        let dragging: Option<(u64, usize)> = ui.data(|d| d.get_temp(drag_id)).flatten();
        if resp.dragged()
            && let (Some((id, i)), Some(pos)) = (dragging, resp.interact_pointer_pos())
            && let Some(n) = notes.iter().find(|n| n.id == id).copied()
        {
            let (at, cents) = place(&n, pos);
            if let Some(c) = self.pattern_mut().pitch.get_mut(&id)
                && i < c.points.len()
            {
                // A point stays between its neighbours while it moves.
                let lo = if i > 0 { c.points[i - 1].at + 1 } else { 0 };
                let hi = c.points.get(i + 1).map_or(n.length, |p| p.at - 1);
                if !shift {
                    c.points[i].at = at.clamp(lo, hi.max(lo));
                }
                c.points[i].cents = cents;
                changed = true;
            }
        }
        if resp.drag_stopped() || resp.clicked() {
            if let Some((id, _)) = dragging {
                let len = notes.iter().find(|n| n.id == id).map_or(0, |n| n.length);
                if let Some(c) = self.pattern_mut().pitch.get_mut(&id) {
                    c.tidy(len);
                }
            }
            ui.data_mut(|d| d.insert_temp::<Option<(u64, usize)>>(drag_id, None));
        }
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let target = hit(pos)
                .map(|(id, i)| (id, Some(i)))
                .or_else(|| note_at(pos).map(|n| (n.id, None)));
            ui.data_mut(|d| d.insert_temp(menu_id, target));
        }
        resp.context_menu(|ui| {
            kit::menu_style(ui);
            let Some((id, point)): Option<(u64, Option<usize>)> =
                ui.data(|d| d.get_temp(menu_id)).flatten()
            else {
                ui.close_menu();
                return;
            };
            let current = self.pattern().pitch.get(&id).cloned().unwrap_or_default();
            if let Some(i) = point.filter(|i| *i < current.points.len()) {
                kit::menu_header(
                    ui,
                    egui::RichText::new(tr.roll_pitch_curve_header).color(t::muted()),
                );
                for shape in CurveShape::ALL {
                    if ui
                        .selectable_label(
                            current.points[i].shape == shape,
                            tr.curve_shape_label(shape),
                        )
                        .clicked()
                    {
                        self.checkpoint();
                        if let Some(c) = self.pattern_mut().pitch.get_mut(&id) {
                            c.points[i].shape = shape;
                        }
                        self.publish();
                        ui.close_menu();
                    }
                }
                if ui.button(tr.roll_pitch_remove_pt).clicked() {
                    self.checkpoint();
                    let pat = self.pattern_mut();
                    if let Some(c) = pat.pitch.get_mut(&id) {
                        c.points.remove(i);
                        if c.is_empty() {
                            pat.pitch.remove(&id);
                        }
                    }
                    self.publish();
                    ui.close_menu();
                }
                ui.separator();
            }
            kit::menu_header(
                ui,
                egui::RichText::new(tr.roll_pitch_vib_header).color(t::muted()),
            );
            let vib = current.vibrato;
            for (label, value) in [
                (tr.roll_pitch_vib_channel, None),
                (tr.roll_pitch_vib_own, Some(NoteVibrato::default())),
                (
                    tr.roll_pitch_vib_wide,
                    Some(NoteVibrato {
                        len: 0.7,
                        depth: 80.0,
                        ..NoteVibrato::default()
                    }),
                ),
                (
                    tr.roll_pitch_vib_none,
                    Some(NoteVibrato {
                        depth: 0.0,
                        ..NoteVibrato::default()
                    }),
                ),
            ] {
                if ui.selectable_label(vib == value, label).clicked() {
                    self.checkpoint();
                    let pat = self.pattern_mut();
                    let c = pat.pitch.entry(id).or_default();
                    c.vibrato = value;
                    if c.is_empty() {
                        pat.pitch.remove(&id);
                    }
                    self.publish();
                    ui.close_menu();
                }
            }
            if !current.points.is_empty() && ui.button(tr.roll_pitch_clear_curve).clicked() {
                self.checkpoint();
                let pat = self.pattern_mut();
                if let Some(c) = pat.pitch.get_mut(&id) {
                    c.points.clear();
                    if c.is_empty() {
                        pat.pitch.remove(&id);
                    }
                }
                self.publish();
                ui.close_menu();
            }
        });
        if changed {
            self.publish();
        }
    }

    /// The stem nearest to `x` (within 10 px), among selected notes when there is a selection.
    fn lane_note_near(&self, g: &Geom, x: f32) -> Option<Note> {
        let me = self.roll_inst;
        self.pattern()
            .notes
            .iter()
            .filter(|n| {
                n.instrument == me && (self.roll_sel.is_empty() || self.roll_sel.contains(&n.id))
            })
            .map(|n| ((g.x(n.start) + 3.0 - x).abs(), *n))
            .filter(|(d, _)| *d < 10.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n)
    }
}

/// Toolbar icon for a piano roll tool, drawn at `c` (about 14 px).
fn tool_icon(p: &egui::Painter, c: Pos2, tool: Tool, col: egui::Color32) {
    let st = Stroke::new(1.5_f32, col);
    match tool {
        Tool::Draw => {
            // Pencil.
            p.line_segment(
                [c + vec2(-5.0, 5.0), c + vec2(4.0, -4.0)],
                Stroke::new(3.0_f32, col),
            );
            p.line_segment([c + vec2(-6.0, 6.0), c + vec2(-4.5, 4.5)], st);
        }
        Tool::Paint => {
            // Brush: handle and a wide tip.
            p.line_segment([c + vec2(-1.0, 1.0), c + vec2(5.0, -5.0)], st);
            p.circle_filled(c + vec2(-3.0, 3.0), 3.0, col);
        }
        Tool::Delete => {
            p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], st);
            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], st);
        }
        Tool::Mute => {
            p.circle_stroke(c, 5.5, st);
            p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], st);
        }
        Tool::Slice => {
            // Knife edge through a note.
            p.rect_stroke(
                Rect::from_center_size(c, vec2(12.0, 6.0)),
                1.5,
                st,
                egui::StrokeKind::Inside,
            );
            p.line_segment([c + vec2(1.0, -7.0), c + vec2(-1.0, 7.0)], st);
        }
        Tool::Select => {
            let r = Rect::from_center_size(c, vec2(12.0, 10.0));
            for (a, b) in [
                (r.left_top(), r.right_top()),
                (r.right_top(), r.right_bottom()),
                (r.right_bottom(), r.left_bottom()),
                (r.left_bottom(), r.left_top()),
            ] {
                p.add(egui::Shape::dashed_line(&[a, b], st, 2.0, 2.0));
            }
        }
        Tool::Zoom => {
            p.circle_stroke(c + vec2(-1.5, -1.5), 4.0, st);
            p.line_segment(
                [c + vec2(1.5, 1.5), c + vec2(5.5, 5.5)],
                Stroke::new(2.0_f32, col),
            );
        }
        Tool::Slide => {
            // A note with a rising arrow (glide).
            p.line_segment([c + vec2(-5.0, 4.0), c + vec2(4.0, -4.0)], st);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(5.5, -5.5),
                    c + vec2(0.5, -4.5),
                    c + vec2(4.5, -0.5),
                ],
                col,
                Stroke::NONE,
            ));
        }
    }
}

impl App {
    /// The selection, or every note of the roll's channel when nothing is selected.
    fn target_notes(&self) -> Vec<Note> {
        let me = self.roll_inst;
        let sel = &self.roll_sel;
        self.pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me && (sel.is_empty() || sel.contains(&n.id)))
            .copied()
            .collect()
    }

    /// Write edited copies back by id.
    fn put_notes(&mut self, edited: &[Note]) {
        for e in edited {
            if let Some(n) = self.note_mut(e.id) {
                *n = *e;
            }
        }
    }

    /// Ctrl+L (FL: quick legato).
    pub(crate) fn legato_notes(&mut self) {
        let mut v = self.target_notes();
        self.checkpoint();
        let n = keyframexl::note_ops::legato(&mut v);
        if n == 0 {
            self.history.pop();
            self.say(Tone::Quiet, self.tr().roll_legato_none);
            return;
        }
        self.put_notes(&v);
        self.fit_pattern();
        self.publish();
        let tr = self.tr();
        let msg = tr.roll_legato_n.fmt(tr.lang, n);
        self.say(Tone::Quiet, msg);
    }

    /// Ctrl+U (FL: chop): notes cut into pieces of the snap grid.
    pub(crate) fn chop_notes(&mut self) {
        let grid = self.snap.grid();
        if grid <= 1 {
            self.say(Tone::Quiet, self.tr().roll_snap_off_chop);
            return;
        }
        let v = self.target_notes();
        if !v.iter().any(|n| n.length > grid) {
            let tr = self.tr();
            let msg = crate::i18n::fill(
                tr.roll_chop_none_longer,
                &[("snap", tr.snap_label(self.snap))],
            );
            self.say(Tone::Quiet, msg);
            return;
        }
        self.checkpoint();
        let pid = self.selected;
        let mut sel = Vec::new();
        let mut pieces = 0;
        for n in v {
            let parts = keyframexl::note_ops::chop(&n, grid);
            pieces += parts.len();
            for part in parts {
                if part.id == n.id {
                    self.put_notes(&[part]);
                    sel.push(part.id);
                } else {
                    sel.push(self.song.add_note(pid, part));
                }
            }
        }
        if !self.roll_sel.is_empty() {
            self.roll_sel = sel;
        }
        self.publish();
        let tr = self.tr();
        let msg =
            tr.roll_chopped_n
                .fmt_with(tr.lang, pieces, &[("snap", tr.snap_label(self.snap))]);
        self.say(Tone::Quiet, msg);
    }

    /// Ctrl+G (FL: glue): touching notes of one pitch become one.
    pub(crate) fn glue_notes(&mut self) {
        let v = self.target_notes();
        let (keep, gone) = keyframexl::note_ops::glue(&v);
        if gone.is_empty() {
            self.say(Tone::Quiet, self.tr().roll_glue_none);
            return;
        }
        self.checkpoint();
        self.put_notes(&keep);
        self.pattern_mut().notes.retain(|n| !gone.contains(&n.id));
        self.roll_sel.retain(|id| !gone.contains(id));
        self.publish();
        let tr = self.tr();
        let msg = tr.roll_glued_n.fmt(tr.lang, gone.len());
        self.say(Tone::Quiet, msg);
    }

    pub(crate) fn open_quantize(&mut self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("quantize-dlg"), true));
    }

    /// Alt+Q (FL: quantize tool): strength, starts and ends, Apply.
    pub(crate) fn quantize_window(&mut self, ctx: &egui::Context) {
        let key = egui::Id::new("quantize-dlg");
        if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
            return;
        }
        let opts_key = egui::Id::new("quantize-opts");
        let (mut strength, mut starts, mut ends) = ctx
            .data(|d| d.get_temp::<(f32, bool, bool)>(opts_key))
            .unwrap_or((1.0, true, false));
        let mut open = true;
        let mut apply = false;
        let tr = self.tr();
        let what = if self.roll_sel.is_empty() {
            tr.roll_quantize_all_notes.to_string()
        } else {
            tr.roll_quantize_sel_n.fmt(tr.lang, self.roll_sel.len())
        };
        let sub = crate::i18n::fill(
            tr.roll_quantize_sub,
            &[("snap", tr.snap_label(self.snap)), ("what", &what)],
        );
        egui::Window::new(tr.roll_quantize_title)
            .id(egui::Id::new("roll-quantize-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 140.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(&sub)
                        .font(t::sans(13.0))
                        .color(t::muted()),
                );
                ui.add_space(4.0);
                let mut pct = strength * 100.0;
                ui.add(
                    egui::Slider::new(&mut pct, 0.0..=100.0)
                        .suffix(" %")
                        .text(tr.roll_quantize_strength),
                );
                strength = pct / 100.0;
                ui.checkbox(&mut starts, tr.roll_quantize_starts);
                ui.checkbox(&mut ends, tr.roll_quantize_ends);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::button(ui, tr.common_apply, kit::Kind::Primary, starts || ends)
                        .clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        apply = true;
                    }
                    if kit::button(ui, tr.common_close, kit::Kind::Normal, true).clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        ctx.data_mut(|d| d.insert_temp(key, false));
                    }
                });
            });
        ctx.data_mut(|d| d.insert_temp(opts_key, (strength, starts, ends)));
        if !open {
            ctx.data_mut(|d| d.insert_temp(key, false));
        }
        if apply {
            let grid = self.snap.grid();
            if grid <= 1 {
                self.say(Tone::Quiet, tr.roll_snap_off_quantize);
                return;
            }
            let mut v = self.target_notes();
            self.checkpoint();
            let n = keyframexl::note_ops::quantize(&mut v, grid, strength, starts, ends);
            if n == 0 {
                self.history.pop();
                self.say(Tone::Quiet, tr.roll_already_on_grid);
                return;
            }
            self.put_notes(&v);
            self.fit_pattern();
            self.publish();
            let pct = format!("{:.0}", strength * 100.0);
            let msg = tr
                .roll_quantize_pct_n
                .fmt_with(tr.lang, n, &[("pct", &pct)]);
            self.say(Tone::Quiet, msg);
        }
    }
}
