//! Playlist: tracks down, bars across. Rows are explicit `Clip::track`
//! values; clips play the window `offset .. offset + length` of their pattern.

use super::roll::{bar_pos, snap_floor, snap_round};
use super::{App, Cmd, Kind, Slot, Tab, Tone, Tool, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::sequence::{BAR, Clip, Tick};
use keyframexl::tempo::{Meter, bar_at};

const HEAD_W: f32 = 120.0;
const RULER_H: f32 = 24.0;
const LANE_H: f32 = 32.0;
const LANE_MIN: f32 = 18.0;
const LANE_MAX: f32 = 120.0;
const MIN_BAR_W: f32 = 96.0;
const BAR_MIN: f32 = 24.0;
const BAR_MAX: f32 = 400.0;
const EDGE: f32 = 8.0;
const MARK: f32 = 14.0;
/// Clip header strip with the pattern name.
const HEAD_STRIP: f32 = 13.0;

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move,
    Right,
    Left,
}

#[derive(Clone, Copy)]
struct PlDrag {
    index: usize,
    mode: DragMode,
    grab: Tick,
}

#[derive(Clone, Copy)]
struct PlBox {
    origin_screen: Pos2,
    origin_tick: Tick,
    origin_track: f32,
    right: bool,
    dragging: bool,
    add: bool,
}

#[cfg(test)]
pub(super) fn box_hits(selection: Rect, clip: Rect) -> bool {
    selection.intersects(clip)
}

pub(super) fn paint_positions(start: Tick, end: Tick, len: Tick) -> Vec<Tick> {
    let len = len.max(1);
    let steps = (end - start).abs() / len;
    (0..=steps)
        .map(|i| start + i * len * if end < start { -1 } else { 1 })
        .filter(|&p| p >= 0)
        .collect()
}

pub(super) fn slice_lengths(start: Tick, len: Tick, at: Tick) -> Option<(Tick, Tick)> {
    (at > start && at < start + len).then_some((at - start, start + len - at))
}

/// Ctrl+wheel zoom anchor: the new sideways offset that keeps the content
/// under `pointer_x` (pixels from the view's left edge) in place.
pub(crate) fn zoom_anchor(scroll_x: f32, pointer_x: f32, old_bar_w: f32, new_bar_w: f32) -> f32 {
    ((scroll_x + pointer_x) * new_bar_w / old_bar_w - pointer_x).max(0.0)
}

/// Snap a song tick to the grid, bars counted by time signature: Bar snap
/// lands on bar starts, finer grids count from the bar's own start.
pub(super) fn song_snap(meters: &[Meter], t: Tick, grid: Tick, alt: bool, round: bool) -> Tick {
    let f = |t: Tick, g: Tick| {
        if round {
            snap_round(t, g, alt)
        } else {
            snap_floor(t, g, alt)
        }
    };
    if meters.is_empty() || alt || grid <= 1 {
        return f(t, grid);
    }
    let (_, start, len, _, _) = bar_at(meters, t.max(0));
    if grid >= BAR {
        return if round && t - start >= len / 2 {
            start + len
        } else {
            start
        };
    }
    start + f(t - start, grid).min(len)
}

/// "bar.beat.step" of a song tick, beats in the bar's own unit (7/8: eighths).
pub(crate) fn song_bar_pos(meters: &[Meter], t: Tick) -> String {
    if meters.is_empty() {
        return bar_pos(t);
    }
    let (bar, start, len, num, _) = bar_at(meters, t.max(0));
    let unit = (len / num.max(1) as Tick).max(1);
    let rest = t.max(0) - start;
    let (beat, inner) = (rest / unit, rest % unit);
    let base = format!(
        "{}.{}.{}",
        bar + 1,
        beat + 1,
        inner / keyframexl::sequence::STEP + 1
    );
    if inner % keyframexl::sequence::STEP == 0 {
        base
    } else {
        format!("{base} +{}", inner % keyframexl::sequence::STEP)
    }
}

/// Paint the stereo/mono audio waveform of an audio-clip pattern inside `area`,
/// aligned to `offset`..`offset + len` at `bar_w` px/bar (clipped to `view`).
/// O(visible_width) with stride sampling so multi-minute tracks stay at 60 FPS.
pub(super) fn paint_audio_clip_wave(
    p: &egui::Painter,
    area: Rect,
    view: Rect,
    song: &keyframexl::sequence::Song,
    inst_id: u64,
    offset: Tick,
    plen: Tick,
    bar_w: f32,
    code: egui::Color32,
    faded: bool,
) {
    let Some(preset) = song.instrument(inst_id).and_then(|i| i.sampler_preset()) else {
        return;
    };
    let Some(z) = preset.zones.first() else {
        return;
    };
    let zv = z.view();
    if zv.end <= zv.offset || zv.frames.is_empty() || bar_w <= 0.0 {
        return;
    }
    let x0 = area.left().max(view.left());
    let x1 = area.right().min(view.right());
    if x1 <= x0 {
        return;
    }
    let mid_y = area.center().y;
    let half_h = (area.height() * 0.46).max(1.0);
    let ink = if faded {
        t::faint()
    } else {
        kit::mix(t::text(), code, 0.35)
    };
    p.line_segment(
        [pos2(x0, mid_y), pos2(x1, mid_y)],
        Stroke::new(1.0_f32, ink.gamma_multiply(0.22)),
    );
    let sps = keyframexl::sequence::seconds_per_step(song.tempo);
    let ticks_per_sec = keyframexl::sequence::STEP as f64 / sps;
    let frames_per_tick = (z.sample_rate as f64 / ticks_per_sec) * 2f64.powf(z.tune_cents / 1200.0);
    let step_px = 2.0_f32;
    let span_ticks = (step_px as f64 / bar_w as f64) * BAR as f64;
    let span_frames = (span_ticks * frames_per_tick).max(1.0);
    let plen = plen.max(1);
    let mut x = x0;
    while x < x1 {
        let tick = offset + ((x - area.left()) as f64 / bar_w as f64 * BAR as f64) as Tick;
        let pat_tick = tick.rem_euclid(plen);
        let rel_frame = pat_tick as f64 * frames_per_tick;
        let f_start = if z.params.reverse {
            (zv.end as f64 - 1.0) - rel_frame - span_frames
        } else {
            zv.offset as f64 + rel_frame
        };
        let f_end = f_start + span_frames;
        let lo = (f_start.floor() as isize).max(zv.offset as isize) as usize;
        let hi = (f_end.ceil() as isize).min(zv.end as isize) as usize;
        if lo < hi {
            let count = hi - lo;
            let stride = (count / 16).max(1);
            let mut peak = 0.0_f32;
            let mut idx = lo;
            while idx < hi {
                let [l, r] = zv.frames[idx];
                let a = l.abs().max(r.abs());
                if a > peak {
                    peak = a;
                }
                idx += stride;
            }
            let h = (peak.min(1.0) * half_h).max(1.0);
            p.line_segment(
                [pos2(x + 0.5, mid_y - h), pos2(x + 0.5, mid_y + h)],
                Stroke::new(1.5_f32, ink),
            );
        }
        x += step_px;
    }
}

fn tick_at(x: f32, grid_l: f32, bar_w: f32) -> Tick {
    (((x - grid_l) / bar_w * BAR as f32) as Tick).max(0)
}

fn track_at(y: f32, grid_top: f32, lane_h: f32) -> u32 {
    ((y - grid_top) / lane_h).floor().max(0.0) as u32
}

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
    fn playlist_track_name(&self, t: u32) -> String {
        let custom = self
            .song
            .tracks
            .get(t as usize)
            .map(|tr| tr.name.as_str())
            .unwrap_or("");
        if custom.is_empty() {
            crate::i18n::fill(self.tr().pl_track_n, &[("n", &(t + 1).to_string())])
        } else {
            custom.to_string()
        }
    }

    /// Build a new clip for `self.selected` at `(start, track)`, preserving the
    /// length & trim offset of the last clicked/resized/trimmed clip of that pattern (FL).
    pub(crate) fn make_playlist_clip(&self, start: Tick, track: u32) -> Clip {
        let mut clip = Clip::new(self.selected, start, track);
        if let Some((pid, offset, length)) = self.last_clip
            && pid == self.selected
        {
            clip.offset = offset;
            clip.length = length;
        }
        clip
    }

    pub(crate) fn delete_clip(&mut self, i: usize) {
        if i >= self.song.clips.len() {
            return;
        }
        self.checkpoint();
        self.song.clips.remove(i);
        self.clip = None;
        self.publish();
        self.say(Tone::Quiet, self.tr().pl_clip_deleted);
    }

    /// Time signature `num/den` from bar `bar` on (one undo step). Bars before
    /// keep theirs; clips and notes stay at their ticks.
    pub(crate) fn set_meter(&mut self, bar: u32, num: u8, den: u8) {
        self.checkpoint();
        let m = &mut self.song.meters;
        if m.is_empty() {
            m.push(Meter {
                bar: 0,
                num: 4,
                den: 4,
            });
        }
        m.retain(|x| x.bar != bar);
        m.push(Meter { bar, num, den });
        keyframexl::tempo::normalize_meters(m);
        // Drop changes that repeat the signature before them.
        let mut last = (0u8, 0u8);
        m.retain(|x| {
            let keep = x.bar == 0 || (x.num, x.den) != last;
            last = (x.num, x.den);
            keep
        });
        if m.iter().all(|x| (x.num, x.den) == (4, 4)) {
            m.clear();
        }
        self.publish();
        let msg = crate::i18n::fill(
            self.tr().pl_meter_set,
            &[
                ("num", &num.to_string()),
                ("den", &den.to_string()),
                ("bar", &(bar + 1).to_string()),
            ],
        );
        self.say(Tone::Quiet, msg);
    }

    /// Move the song playhead to the next (`dir` > 0) or previous marker.
    /// "Previous" skips the one just passed by less than a beat, so pressing
    /// it again while playing keeps going back.
    pub(crate) fn jump_marker(&mut self, dir: i32) {
        let now = if self.song_mode {
            self.playhead_tick()
        } else {
            0
        };
        let beat = keyframexl::sequence::BAR / 4;
        let m = &self.song.markers;
        let to = if dir > 0 {
            m.iter().find(|m| m.tick > now)
        } else {
            m.iter().rev().find(|m| m.tick + beat <= now)
        };
        match to.map(|m| (m.tick, m.name.clone())) {
            Some((tick, name)) => {
                self.set_song_mode(true);
                self.seek_tick(tick);
                self.say(
                    Tone::Quiet,
                    format!("{name} · {}", song_bar_pos(&self.song.meters, tick)),
                );
            }
            None if m.is_empty() => self.say(Tone::Quiet, self.tr().pl_no_markers),
            None => self.say(
                Tone::Quiet,
                if dir > 0 {
                    self.tr().pl_no_marker_after
                } else {
                    self.tr().pl_no_marker_before
                },
            ),
        }
    }

    /// Loop the section from marker `i` to the next one (or the song's end).
    pub(crate) fn loop_marker(&mut self, i: usize) {
        let Some((a, b)) = self.song.marker_span(i) else {
            return;
        };
        self.song.loop_range = Some((a, b));
        self.set_song_mode(true);
        self.publish();
        self.seek_tick(a);
        let name = self.song.markers[i].name.clone();
        let a_pos = song_bar_pos(&self.song.meters, a);
        let b_pos = song_bar_pos(&self.song.meters, b);
        let msg = crate::i18n::fill(
            self.tr().pl_looping_marker,
            &[("name", &name), ("a", &a_pos), ("b", &b_pos)],
        );
        self.say(Tone::Quiet, msg);
    }

    fn add_clip(&mut self) {
        self.checkpoint();
        let start = if self.song.clips.is_empty() {
            0
        } else {
            self.song.end_bar_tick()
        };
        self.song.clips.push(Clip::new(self.selected, start, 0));
        self.clip = Some(self.song.clips.len() - 1);
        self.publish();
        let pat = self.pattern().name.clone();
        let pos = song_bar_pos(&self.song.meters, start);
        let msg = crate::i18n::fill(self.tr().pl_placed_track1, &[("pat", &pat), ("pos", &pos)]);
        self.say(Tone::Quiet, msg);
    }

    fn make_unique(&mut self, i: usize) {
        self.checkpoint();
        match self.song.unique(i) {
            Ok(id) => {
                self.selected = id;
                self.publish();
                let pat = self.pattern().name.clone();
                let msg = crate::i18n::fill(self.tr().pl_made_unique, &[("pat", &pat)]);
                self.say(Tone::Quiet, msg);
            }
            Err(e) => {
                self.history.pop();
                self.say(Tone::Bad, e);
            }
        }
    }

    fn toggle_clip_mute(&mut self, i: usize) {
        if i >= self.song.clips.len() {
            return;
        }
        self.checkpoint();
        self.song.clips[i].muted = !self.song.clips[i].muted;
        let on = self.song.clips[i].muted;
        let at = self.song.clips[i].start;
        self.clip = Some(i);
        self.publish();
        let pos = song_bar_pos(&self.song.meters, at);
        let tmpl = if on {
            self.tr().pl_clip_muted
        } else {
            self.tr().pl_clip_unmuted
        };
        let msg = crate::i18n::fill(tmpl, &[("pos", &pos)]);
        self.say(Tone::Quiet, msg);
    }

    pub(super) fn playlist(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let tool_id = egui::Id::new("kfxl-playlist-tool");
        let mut tool = ui
            .data(|d| d.get_temp::<Tool>(tool_id))
            .unwrap_or(Tool::Draw);
        if self.focus_kind() == Some(Kind::Playlist) && !ui.ctx().wants_keyboard_input() {
            for tl in Tool::ALL.into_iter().filter(|t| !matches!(t, Tool::Slide)) {
                if ui.input(|i| !i.modifiers.command && !i.modifiers.alt && i.key_pressed(tl.key()))
                {
                    tool = tl;
                    self.say(
                        Tone::Quiet,
                        crate::i18n::fill(tr.pl_tool_selected, &[("label", tr.tool_label(tl))]),
                    );
                }
            }
        }
        let selection_id = egui::Id::new("kfxl-playlist-selection");
        let mut selection: Vec<usize> = ui.data(|d| d.get_temp(selection_id)).unwrap_or_default();
        selection.retain(|&i| i < self.song.clips.len());
        if self.focus_kind() == Some(Kind::Playlist) && !ui.ctx().wants_keyboard_input() {
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
                selection = (0..self.song.clips.len()).collect();
                self.clip = None;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                selection.clear();
                self.clip = None;
            }
            if !selection.is_empty()
                && ui.input(|i| {
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                })
            {
                self.checkpoint();
                self.song.clips = self
                    .song
                    .clips
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !selection.contains(i))
                    .map(|(_, c)| *c)
                    .collect();
                selection.clear();
                self.clip = None;
                self.publish();
            }
        }
        let n = self.song.clips.len();
        let sub = if n == 0 {
            tr.pl_empty.to_string()
        } else {
            format!(
                "{}  ·  {}",
                tr.pl_clips_count.fmt(tr.lang, n),
                tr.rack_bars.fmt(tr.lang, self.song.bars() as usize)
            )
        };
        let has_sel = self.clip.is_some();
        let mut act: Option<&str> = None;
        App::panel_header(ui, &sub, |ui, slot| {
            if slot == Slot::Menu {
                if ui.button(tr.pl_add_clip).clicked() {
                    act = Some("add");
                    ui.close_menu();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(tr.pl_make_unique))
                    .clicked()
                {
                    act = Some("unique");
                    ui.close_menu();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(tr.pl_mute_clip))
                    .clicked()
                {
                    act = Some("mute");
                    ui.close_menu();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(tr.pl_delete_clip))
                    .clicked()
                {
                    act = Some("delete");
                    ui.close_menu();
                }
                ui.separator();
                if kit::menu_check(ui, self.song_mode, tr.pl_play_song_mode, "").clicked() {
                    self.set_song_mode(!self.song_mode);
                    ui.close_menu();
                }
                if kit::menu_check(ui, self.follow, tr.view_follow_playhead, "")
                    .on_hover_text(tr.pl_follow_tip)
                    .clicked()
                {
                    self.follow = !self.follow;
                    ui.close_menu();
                }
                self.window_menu_tail(ui, Tab::Playlist);
                return;
            }
            let r = kit::button(ui, tr.pl_make_unique, kit::Kind::Normal, has_sel)
                .on_hover_text(tr.pl_make_unique_tip);
            #[cfg(test)]
            {
                self.probe.unique = Some(r.rect);
            }
            if r.clicked() {
                act = Some("unique");
            }
            ui.spacing_mut().item_spacing.x = 2.0;
            for tl in Tool::ALL
                .into_iter()
                .filter(|t| !matches!(t, Tool::Slide))
                .rev()
            {
                let on = tool == tl;
                let r = kit::button_sized(
                    ui,
                    vec2(28.0, kit::TOOL_H),
                    if on {
                        kit::Kind::Normal
                    } else {
                        kit::Kind::Ghost
                    },
                    true,
                    tr.tool_label(tl),
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
                if kit::tip(r, format!("{}  {:?}", tr.tool_label(tl), tl.key())).clicked() {
                    tool = tl;
                }
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            let r = kit::button(ui, tr.pl_add_clip, kit::Kind::Normal, true)
                .on_hover_text(tr.pl_add_clip_tip);
            #[cfg(test)]
            {
                self.probe.add = Some(r.rect);
            }
            if r.clicked() {
                act = Some("add");
            }
        });
        ui.data_mut(|d| d.insert_temp(tool_id, tool));

        match (act, self.clip) {
            (Some("add"), _) => self.add_clip(),
            (Some("unique"), Some(i)) => self.make_unique(i),
            (Some("mute"), Some(i)) => self.toggle_clip_mute(i),
            (Some("delete"), Some(i)) => self.delete_clip(i),
            _ => {}
        }
        // Zoom: horizontal pixels per bar, Ctrl+wheel over the grid.
        let zoom_id = ui.id().with("bar-w");
        // Captures may start zoomed: KFXL_CAPTURE_BAR_W=px.
        static START_W: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        let start_w = *START_W.get_or_init(|| {
            std::env::var("KFXL_CAPTURE_BAR_W")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(MIN_BAR_W)
        });
        let mut bar_w: f32 = ui.data(|d| d.get_temp::<f32>(zoom_id)).unwrap_or(start_w);
        // Track height (FL: Alt+wheel over the tracks, or drag a header's lower edge).
        let lane_id = egui::Id::new("kfxl-pl-lane-h");
        let mut lane_h: f32 = ui
            .data_mut(|d| d.get_persisted::<f32>(lane_id))
            .unwrap_or(LANE_H)
            .clamp(LANE_MIN, LANE_MAX);
        bar_w = bar_w.clamp(BAR_MIN, BAR_MAX);

        let avail_w = ui.available_width();
        let avail_h = ui.available_height();
        // Widths count in 4/4 bars (`bar_w`); a 3/4 bar is narrower, as in Ableton.
        let need_bars = (self.song.end_bar_tick() / BAR) as usize + 2;
        let fit_bars = ((avail_w - HEAD_W) / bar_w).ceil().max(1.0) as usize;
        let bars = need_bars.max(fit_bars).max(4);
        let highest = self.song.clips.iter().map(|c| c.track as usize).max();
        let used_rows = highest.map(|h| h + 4).unwrap_or(4);
        // Ruler, then the pinned tempo lane, then the tracks.
        let tempo_h = if self.tempo_lane_folded(ui) {
            super::tempo_lane::LANE_FOLDED
        } else {
            super::tempo_lane::LANE_OPEN
        };
        let top_h = RULER_H + tempo_h;
        let fill_rows = ((avail_h - top_h) / lane_h).ceil().max(1.0) as usize;
        let rows = used_rows.max(fill_rows).max(1);
        let w = (HEAD_W + bars as f32 * bar_w + 4.0).max(avail_w);
        let h = (top_h + rows as f32 * lane_h + 8.0).max(avail_h.max(top_h + lane_h + 8.0));
        // FL: the track column and the panel header stay put; only the grid
        // and the ruler scroll sideways, by the playlist's own offset.
        let scroll_id = ui.id().with("pl-scroll-x");
        let max_scroll = (w - avail_w).max(0.0);
        let mut scroll_x: f32 = ui
            .data(|d| d.get_temp(scroll_id))
            .unwrap_or(0.0_f32)
            .clamp(0.0, max_scroll);
        let _ = h;
        // The tracks scroll inside the view (the ruler and tempo lane stay).
        let (body, _) = ui.allocate_exact_size(
            vec2(avail_w, avail_h.max(top_h + lane_h + 8.0)),
            Sense::hover(),
        );
        let p = ui.painter().clone();
        let well_rect = Rect::from_min_max(body.min, body.max - vec2(0.0, 2.0));
        let well = kit::sunken(&p, well_rect, 8.0).shrink(1.0);
        // The grid's visible part, right of the track column.
        let view = Rect::from_min_max(pos2(well.left() + HEAD_W, well.top()), well.max);
        // Shift+wheel or a sideways wheel scrolls the grid (Ctrl+wheel zooms, below).
        if ui.rect_contains_pointer(view) {
            let (shift, ctrl_held, dx, dy) = ui.input(|i| {
                let raw = i.raw_scroll_delta;
                let smooth = i.smooth_scroll_delta;
                let pick = |r: f32, s: f32| if r != 0.0 { r } else { s };
                (
                    i.modifiers.shift,
                    i.modifiers.command,
                    pick(raw.x, smooth.x),
                    pick(raw.y, smooth.y),
                )
            });
            let dx = if shift { dy + dx } else { dx };
            if !ctrl_held && dx != 0.0 {
                scroll_x = (scroll_x - dx).clamp(0.0, max_scroll);
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = vec2(0.0, 0.0);
                    i.raw_scroll_delta = vec2(0.0, 0.0);
                });
            }
        }
        // Tracks: the wheel scrolls them up and down (FL), Shift+wheel sideways,
        // and a middle-button drag pans both ways (FL: drag with the wheel).
        let tracks_top = well.top() + top_h;
        let rows_view = Rect::from_min_max(pos2(well.left(), tracks_top), well.max);
        let scroll_y_id = ui.id().with("pl-scroll-y");
        let max_scroll_y = (rows as f32 * lane_h + 8.0 - rows_view.height()).max(0.0);
        let mut scroll_y: f32 = ui
            .data(|d| d.get_temp(scroll_y_id))
            .unwrap_or(0.0_f32)
            .clamp(0.0, max_scroll_y);
        if ui.rect_contains_pointer(rows_view) {
            let (m, dy) = ui.input(|i| {
                let (raw, smooth) = (i.raw_scroll_delta.y, i.smooth_scroll_delta.y);
                (i.modifiers, if raw != 0.0 { raw } else { smooth })
            });
            if m.alt && !m.command && dy != 0.0 {
                // Taller or shorter tracks; the track under the pointer stays under it.
                let old = lane_h;
                lane_h = (lane_h * (1.0 + dy * 0.003).clamp(0.8, 1.25)).clamp(LANE_MIN, LANE_MAX);
                if let Some(py) = ui
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| p.y - tracks_top)
                {
                    scroll_y = ((scroll_y + py) * lane_h / old - py).max(0.0);
                }
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = vec2(0.0, 0.0);
                    i.raw_scroll_delta = vec2(0.0, 0.0);
                });
                ui.ctx().request_repaint();
            } else if !m.command && !m.shift && !m.alt && dy != 0.0 {
                scroll_y = (scroll_y - dy).clamp(0.0, max_scroll_y);
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = vec2(0.0, 0.0);
                    i.raw_scroll_delta = vec2(0.0, 0.0);
                });
            }
        }
        if let Some(d) = super::kit::middle_pan(ui, well, ui.id().with("pl-pan")) {
            scroll_x = (scroll_x - d.x).clamp(0.0, max_scroll);
            scroll_y = (scroll_y - d.y).clamp(0.0, max_scroll_y);
        }
        ui.data_mut(|d| d.insert_temp(scroll_y_id, scroll_y));
        let mut lane_next = lane_h;
        let grid_l = well.left() + HEAD_W - scroll_x;
        let grid = Rect::from_min_max(
            pos2(grid_l, tracks_top - scroll_y),
            pos2(
                grid_l + bars as f32 * bar_w,
                tracks_top - scroll_y + rows as f32 * lane_h,
            ),
        );
        #[cfg(test)]
        {
            self.probe.pl_grid = Some((grid, bar_w, lane_h));
        }
        // Ctrl+wheel over the grid zooms; the wheel is consumed so the dock stays.
        // Pinch (zoom_delta) counts too: on some systems Ctrl+wheel arrives as a pinch.
        let (ctrl, wheel, pinch) = ui.input(|i| {
            let raw = i.raw_scroll_delta.y;
            let smooth = i.smooth_scroll_delta.y;
            let wheel = if raw != 0.0 { raw } else { smooth };
            (i.modifiers.command, wheel, i.zoom_delta())
        });
        if (ctrl || pinch != 1.0)
            && (wheel != 0.0 || pinch != 1.0)
            && ui.rect_contains_pointer(view)
        {
            let factor = if pinch != 1.0 {
                pinch
            } else {
                (1.0 + wheel / 200.0).clamp(0.8, 1.25)
            };
            let old = bar_w;
            bar_w = (bar_w * factor).clamp(BAR_MIN, BAR_MAX);
            ui.data_mut(|d| d.insert_temp(zoom_id, bar_w));
            // The bar under the pointer stays under it.
            if let Some(px) = ui.input(|i| i.pointer.hover_pos()).map(|p| p.x) {
                scroll_x = zoom_anchor(scroll_x, px - view.left(), old, bar_w);
            }
            ui.input_mut(|i| {
                i.smooth_scroll_delta = vec2(0.0, 0.0);
                i.raw_scroll_delta = vec2(0.0, 0.0);
            });
            // A zoomed frame paints again at once, even while stopped.
            ui.ctx().request_repaint();
        } else {
            ui.data_mut(|d| {
                if d.get_temp::<f32>(zoom_id).is_none() {
                    d.insert_temp(zoom_id, bar_w);
                }
            });
        }

        let painter = p.with_clip_rect(well);
        painter.rect_filled(
            Rect::from_min_max(well.left_top(), pos2(well.right(), tracks_top)),
            0.0,
            t::surface(),
        );
        kit::caps(
            &painter,
            pos2(well.left() + 12.0, well.top() + RULER_H / 2.0),
            Align2::LEFT_CENTER,
            tr.pl_track_caps,
            12.0,
            t::faint(),
        );
        painter.line_segment(
            [
                pos2(view.left(), well.top()),
                pos2(view.left(), well.bottom()),
            ],
            Stroke::new(1.0_f32, t::edge()),
        );
        // Grid, ruler and clips paint inside the view only.
        let gp = p.with_clip_rect(view.intersect(well));
        // Bars by time signature; a flag marks each change (amber: structure).
        let meters = self.song.meters.clone();
        let mut bar_lines: Vec<(f32, bool)> = Vec::new();
        let span = bars as Tick * BAR;
        // Marker chips on the ruler, after a time signature flag at the same spot.
        let chip_w = |name: &str| 10.0 + name.chars().take(16).count() as f32 * 6.4;
        let mut marker_chips: Vec<(Rect, usize)> = Vec::new();
        for (i, m) in self.song.markers.iter().enumerate() {
            let x = grid_l + m.tick as f32 / BAR as f32 * bar_w;
            let after_sig = meters.iter().find(|s| {
                self.song.bar_tick(s.bar) == m.tick && (s.bar > 0 || (s.num, s.den) != (4, 4))
            });
            let x = x
                + 1.0
                + after_sig.map_or(0.0, |s| {
                    12.0 + format!("{}/{}", s.num, s.den).len() as f32 * 7.0
                });
            marker_chips.push((
                Rect::from_min_size(pos2(x, well.top() + 2.0), vec2(chip_w(&m.name), 12.0)),
                i,
            ));
        }
        let mut b = 0u32;
        loop {
            let tick = self.song.bar_tick(b);
            if tick >= span {
                break;
            }
            let x = grid_l + tick as f32 / BAR as f32 * bar_w;
            let strong = b.is_multiple_of(4);
            bar_lines.push((x, strong));
            let mut label_x = x + 6.0;
            if let Some(m) = meters
                .iter()
                .find(|m| m.bar == b && (b > 0 || (m.num, m.den) != (4, 4)))
            {
                let text = format!("{}/{}", m.num, m.den);
                let w = 8.0 + text.len() as f32 * 7.0;
                let flag = Rect::from_min_size(pos2(x + 1.0, well.top() + 2.0), vec2(w, 12.0));
                gp.rect_filled(flag, 2.0, t::wait());
                gp.text(
                    flag.center(),
                    Align2::CENTER_CENTER,
                    text,
                    t::mono(10.5),
                    t::sunken(),
                );
                label_x = flag.right() + 4.0;
            }
            if let Some((chip, _)) = marker_chips
                .iter()
                .find(|(r, _)| r.left() >= x && r.left() <= label_x + 1.0)
            {
                label_x = chip.right() + 4.0;
            }
            let label_col = if strong { t::text() } else { t::muted() };
            if bar_w * (self.song.bar_tick(b + 1) - tick) as f32 / BAR as f32 >= 22.0 || strong {
                gp.text(
                    pos2(label_x, well.top() + RULER_H / 2.0 + 3.0),
                    Align2::LEFT_CENTER,
                    format!("{}", b + 1),
                    t::mono(12.0),
                    label_col,
                );
            }
            gp.line_segment(
                [pos2(x, well.top() + 6.0), pos2(x, well.bottom())],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        t::strong().gamma_multiply(0.6)
                    } else {
                        t::line()
                    },
                ),
            );
            b += 1;
        }
        for (chip, i) in &marker_chips {
            let m = &self.song.markers[*i];
            gp.line_segment(
                [
                    pos2(chip.left() - 1.0, well.top() + 2.0),
                    pos2(chip.left() - 1.0, well.top() + RULER_H),
                ],
                Stroke::new(1.0_f32, t::text()),
            );
            gp.rect_filled(*chip, 2.0, t::ctl_hover());
            let name: String = m.name.chars().take(16).collect();
            gp.text(
                pos2(chip.left() + 5.0, chip.center().y),
                Align2::LEFT_CENTER,
                name,
                t::mono(10.5),
                t::text(),
            );
        }
        if n > 0 {
            let x = grid_l + self.song.end_bar_tick() as f32 / BAR as f32 * bar_w;
            gp.line_segment(
                [pos2(x, well.top()), pos2(x, grid.top())],
                Stroke::new(2.0_f32, t::text()),
            );
            gp.text(
                pos2(x - 4.0, well.top() + RULER_H / 2.0),
                Align2::RIGHT_CENTER,
                tr.pl_end_tag,
                t::mono(12.0),
                t::text(),
            );
        }

        // Track rename in flight: (track, text), kept in temp memory.
        // NOTE: the turbofish matters — `get_temp` returns `Option<T>`, so an
        // unadorned `Option<..>` binding would read inner type `T` and never
        // see what was written. Always spell out `Option<..>` here.
        let rename_id = egui::Id::new("kfxl-pl-track-rename2");
        let mut rename: Option<(u32, String)> = ui
            .data(|d| d.get_temp::<Option<(u32, String)>>(rename_id))
            .flatten();
        // Grid background for empty-cell placement (behind the clips).
        // Reserve the bottom horizontal scrollbar strip when present so dragging
        // the scrollbar never triggers a grid click or clears selection.
        let mut grid_seen = grid.intersect(view).intersect(rows_view);
        if max_scroll > 0.0 {
            grid_seen.max.y = grid_seen
                .max
                .y
                .min(well.bottom() - 11.0)
                .max(grid_seen.min.y);
        }
        // Rows and clips: drawn and clickable below the pinned top only.
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(rows_view));
        let full_clip = ui.clip_rect();
        ui.set_clip_rect(full_clip.intersect(rows_view));
        let grid_resp = ui.interact(grid_seen, ui.id().with("grid"), Sense::click_and_drag());
        self.playlist_drop = Some((grid_seen, grid_l, grid.top(), bar_w, lane_h));

        for t in 0..rows {
            let y = grid.top() + t as f32 * lane_h;
            if t % 2 == 1 {
                painter.rect_filled(
                    Rect::from_min_max(pos2(well.left(), y), pos2(well.right(), y + lane_h)),
                    0.0,
                    t::side(),
                );
            }
            painter.line_segment(
                [
                    pos2(well.left(), y + lane_h),
                    pos2(well.right(), y + lane_h),
                ],
                Stroke::new(1.0_f32, t::line().gamma_multiply(0.6)),
            );
            let muted = self.song.track_muted(t as u32);
            // Mute LED: lit while the track plays.
            let led_c = pos2(well.left() + 14.0, y + lane_h / 2.0);
            if muted {
                painter.circle_stroke(led_c, 4.5, Stroke::new(1.5_f32, t::muted()));
            } else {
                kit::led(&painter, led_c, 4.5, Some(t::ok()));
            }
            let led_hit = Rect::from_center_size(led_c, vec2(24.0, 24.0));
            let led_resp = ui.interact(led_hit, ui.id().with(("track-mute", t)), Sense::click());
            let tr = self.tr();
            let t_num = (t + 1).to_string();
            let led_resp = led_resp.on_hover_text(crate::i18n::fill(
                if muted {
                    tr.pl_track_unmute_tip
                } else {
                    tr.pl_track_mute_tip
                },
                &[("n", &t_num)],
            ));
            if led_resp.clicked() {
                self.checkpoint();
                self.song.track_mut(t as u32).muted = !self.song.track_mut(t as u32).muted;
                let now_muted = self.song.track_muted(t as u32);
                self.publish();
                let tr = self.tr();
                self.say(
                    Tone::Quiet,
                    crate::i18n::fill(
                        if now_muted {
                            tr.pl_track_muted
                        } else {
                            tr.pl_track_unmuted
                        },
                        &[("n", &t_num)],
                    ),
                );
            }
            // Name or inline rename editor. A coloured track fills its header
            // (FL), so the colour reads as a track, not a stray stripe.
            let track_color = self
                .song
                .tracks
                .get(t)
                .and_then(|tr| tr.color)
                .map(t::color_from_u32);
            let head_face = Rect::from_min_max(pos2(well.left(), y), pos2(view.left(), y + lane_h));
            // Name ink: dark on light colours, light otherwise.
            let mut name_ink = if muted { t::faint() } else { t::muted() };
            if let Some(col) = track_color {
                painter.rect_filled(head_face, 0.0, kit::mix(t::side(), col, 0.45));
                painter.rect_filled(
                    Rect::from_min_size(head_face.left_top(), vec2(5.0, head_face.height())),
                    0.0,
                    col,
                );
                let lum = 0.299 * f32::from(col.r())
                    + 0.587 * f32::from(col.g())
                    + 0.114 * f32::from(col.b());
                name_ink = if lum > 150.0 { t::key_ink() } else { t::text() };
            }
            if rename.as_ref().is_some_and(|(rt, _)| *rt as usize == t) {
                let txt = rename.clone().map(|(_, s)| s).unwrap_or_default();
                let mut txt = txt;
                let edit_rect = Rect::from_min_size(
                    pos2(well.left() + 32.0, y + 6.0),
                    vec2(HEAD_W - 38.0, 20.0),
                );
                let edit_id = egui::Id::new("kfxl-pl-track-rename2-edit");
                let te = ui.put(edit_rect, egui::TextEdit::singleline(&mut txt).id(edit_id));
                if !te.has_focus() {
                    te.request_focus();
                    // Select the current name so typing replaces it (FL).
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
                        let len = txt.chars().count();
                        state.cursor.set_char_range(Some(egui::text::CCursorRange {
                            primary: egui::text::CCursor {
                                index: len,
                                prefer_next_row: false,
                            },
                            secondary: egui::text::CCursor {
                                index: 0,
                                prefer_next_row: false,
                            },
                        }));
                        state.store(ui.ctx(), edit_id);
                    }
                }
                rename = Some((t as u32, txt.clone()));
                let (enter, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if esc {
                    rename = None;
                } else if enter || te.lost_focus() {
                    // Enter applies as one undo step; clicking away applies too.
                    let name = txt.trim().to_string();
                    self.checkpoint();
                    self.song.track_mut(t as u32).name = name;
                    self.publish();
                    let tr = self.tr();
                    self.say(
                        Tone::Quiet,
                        crate::i18n::fill(tr.pl_track_renamed, &[("n", &t_num)]),
                    );
                    rename = None;
                }
            } else {
                let name = self.playlist_track_name(t as u32);
                let shown = kit::elide(&painter, &name, t::sans(13.0), HEAD_W - 46.0);
                painter.text(
                    pos2(well.left() + 32.0, y + lane_h / 2.0),
                    Align2::LEFT_CENTER,
                    shown,
                    t::sans(13.0),
                    name_ink,
                );
            }
            // Header right-click: rename, colour, mute. Double-click renames too.
            let head_rect =
                Rect::from_min_max(pos2(well.left() + 26.0, y), pos2(view.left(), y + lane_h));
            let head_resp = ui.interact(head_rect, ui.id().with(("track-head", t)), Sense::click());
            // The header's lower edge sets every track's height (FL: drag to resize).
            let edge = Rect::from_min_max(
                pos2(well.left(), y + lane_h - 3.0),
                pos2(view.left(), y + lane_h + 3.0),
            );
            let edge_resp = ui
                .interact(edge, ui.id().with(("track-edge", t)), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeVertical);
            if edge_resp.dragged() {
                lane_next = (lane_h + edge_resp.drag_delta().y / (t as f32 + 1.0))
                    .clamp(LANE_MIN, LANE_MAX);
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if edge_resp.double_clicked() {
                lane_next = LANE_H;
            }
            if head_resp.double_clicked() {
                rename = Some((t as u32, self.playlist_track_name(t as u32)));
            }
            let cur_color = self.song.tracks.get(t).and_then(|tr| tr.color);
            head_resp.context_menu(|ui| {
                kit::menu_style(ui);
                let tr = self.tr();
                if ui
                    .button(tr.pat_rename)
                    .on_hover_text(tr.pl_track_rename_tip)
                    .clicked()
                {
                    rename = Some((t as u32, self.playlist_track_name(t as u32)));
                    ui.close_menu();
                }
                ui.menu_button(tr.pat_colour, |ui| {
                    kit::menu_style(ui);
                    let tr = self.tr();
                    match kit::colour_swatches(ui, cur_color, tr.pl_track_default_colour, None) {
                        Some(None) => {
                            self.checkpoint();
                            self.song.track_mut(t as u32).color = None;
                            self.publish();
                            self.say(
                                Tone::Quiet,
                                crate::i18n::fill(tr.pl_track_colour_reset, &[("n", &t_num)]),
                            );
                            ui.close_menu();
                        }
                        Some(Some(rgb)) => {
                            self.checkpoint();
                            self.song.track_mut(t as u32).color = Some(rgb);
                            self.publish();
                            let hex = format!("{rgb:06X}");
                            self.say(
                                Tone::Quiet,
                                crate::i18n::fill(
                                    tr.pl_track_colour_set,
                                    &[("n", &t_num), ("hex", &hex)],
                                ),
                            );
                            ui.close_menu();
                        }
                        None => {}
                    }
                });
                let label = if self.song.track_muted(t as u32) {
                    tr.pl_track_unmute
                } else {
                    tr.pl_track_mute
                };
                if ui.button(label).clicked() {
                    self.checkpoint();
                    let m = self.song.track_mut(t as u32).muted;
                    self.song.track_mut(t as u32).muted = !m;
                    self.publish();
                    self.say(
                        Tone::Quiet,
                        crate::i18n::fill(
                            if !m {
                                tr.pl_track_muted
                            } else {
                                tr.pl_track_unmuted
                            },
                            &[("n", &t_num)],
                        ),
                    );
                    ui.close_menu();
                }
                // Solo: every other track that exists or holds clips is muted.
                let mut others: Vec<u32> = (0..self.song.tracks.len() as u32)
                    .chain(self.song.clips.iter().map(|c| c.track))
                    .filter(|o| *o != t as u32)
                    .collect();
                others.sort_unstable();
                others.dedup();
                let soloed = !muted
                    && !others.is_empty()
                    && others.iter().all(|o| self.song.track_muted(*o));
                if ui
                    .button(if soloed {
                        tr.pl_track_unsolo
                    } else {
                        tr.pl_track_solo
                    })
                    .clicked()
                {
                    self.checkpoint();
                    for o in others {
                        self.song.track_mut(o).muted = !soloed;
                    }
                    self.song.track_mut(t as u32).muted = false;
                    self.publish();
                    self.say(
                        Tone::Quiet,
                        crate::i18n::fill(
                            if soloed {
                                tr.pl_track_unsoloed
                            } else {
                                tr.pl_track_soloed
                            },
                            &[("n", &t_num)],
                        ),
                    );
                    ui.close_menu();
                }
                if ui.button(tr.pl_track_clear).clicked() {
                    if self.song.clips.iter().any(|c| c.track == t as u32) {
                        self.checkpoint();
                        self.song.clips.retain(|c| c.track != t as u32);
                        self.clip = None;
                        self.publish();
                        self.say(
                            Tone::Quiet,
                            crate::i18n::fill(tr.pl_track_cleared, &[("n", &t_num)]),
                        );
                    }
                    ui.close_menu();
                }
            });
        }
        ui.data_mut(|d| d.insert_temp(rename_id, rename));
        if lane_next != lane_h || ui.data_mut(|d| d.get_persisted::<f32>(lane_id)) != Some(lane_h) {
            ui.data_mut(|d| d.insert_persisted(lane_id, lane_next));
            if lane_next != lane_h {
                ui.ctx().request_repaint();
            }
        }
        // From here on everything belongs to the grid: clipped to the view.
        let painter_full = gp.clone();
        let painter = gp.with_clip_rect(gp.clip_rect().intersect(rows_view));

        if n == 0 {
            let c = grid.center();
            painter.text(
                c,
                Align2::CENTER_CENTER,
                self.tr().pl_empty_grid_prompt,
                t::sans(14.0),
                t::muted(),
            );
        }

        // Clips.
        let clips = self.song.clips.clone();
        // A clip covers exactly its time span (FL): left edge = start, right
        // edge = end, no inset. Very short clips get a wider hit area only.
        let clip_rect = |start: Tick, len: Tick, track: u32| {
            let x = grid_l + start as f32 / BAR as f32 * bar_w;
            let w = (len as f32 / BAR as f32 * bar_w).max(6.0);
            Rect::from_min_size(
                pos2(x, grid.top() + track as f32 * lane_h + 1.0),
                vec2(w, lane_h - 2.0),
            )
        };
        let grid_ticks = self.snap.grid();
        let press_origin = ui.input(|i| i.pointer.press_origin());

        enum Pending {
            Select(usize),
            Open(usize),
            Move {
                i: usize,
                start: Tick,
                track: u32,
            },
            Resize {
                i: usize,
                len: Tick,
            },
            Trim {
                i: usize,
                start: Tick,
                offset: Tick,
                len: Tick,
            },
            Duplicate {
                i: usize,
                start: Tick,
                track: u32,
            },
            Delete(usize),
            Menu(usize, String),
        }
        let mut pending: Vec<Pending> = Vec::new();
        let mut marker_used: Vec<bool> = vec![false; clips.len()];

        for (i, c) in clips.iter().enumerate() {
            let len = self.song.clip_len(c);
            let r = clip_rect(c.start, len, c.track.min(999));
            #[cfg(test)]
            if i == 1 {
                self.probe.clip1 = Some(r);
            }
            let Some(pat) = self.song.patterns.iter().find(|p| p.id == c.pattern) else {
                continue;
            };
            let code = t::pattern_color(pat.id, pat.color);
            let selected = self.clip == Some(i) || selection.contains(&i);
            let faded = c.muted || self.song.track_muted(c.track);
            let resp = ui.interact(
                r.intersect(view),
                ui.id().with(("clip", i)),
                Sense::click_and_drag(),
            );
            let hover_amt = kit::hover_amount(ui, &resp);
            // Body, then a header strip in the pattern colour with the name
            // (FL clip look); square corners so the edges read as time.
            let face = r;
            let body = if faded {
                kit::mix(t::sunken(), code, 0.06)
            } else {
                kit::mix(t::panel_bottom(), code, 0.12 + 0.05 * hover_amt)
            };
            painter.rect_filled(r, 2.0, body);
            let head_h = HEAD_STRIP.min(r.height());
            let head = Rect::from_min_size(r.min, vec2(r.width(), head_h));
            let head_fill = if faded {
                kit::mix(t::sunken(), code, 0.25)
            } else {
                kit::mix(t::panel_top(), code, 0.55 + 0.1 * hover_amt)
            };
            painter.rect_filled(
                head,
                egui::CornerRadius {
                    nw: 2,
                    ne: 2,
                    sw: 0,
                    se: 0,
                },
                head_fill,
            );
            let edge = if selected {
                t::text()
            } else if faded {
                t::line()
            } else {
                kit::mix(t::edge(), code, 0.5)
            };
            painter.rect_stroke(
                r,
                2.0,
                Stroke::new(if selected { 1.5_f32 } else { 1.0 }, edge),
                egui::StrokeKind::Inside,
            );
            let shared = self
                .song
                .clips
                .iter()
                .filter(|x| x.pattern == c.pattern)
                .count();
            let badge = if shared > 1 && r.width() > 70.0 {
                26.0
            } else {
                0.0
            };
            if r.width() > MARK + 12.0 {
                let shown = kit::elide(
                    &painter,
                    &pat.name,
                    t::medium(12.0),
                    (r.width() - MARK - 8.0 - badge).max(8.0),
                );
                painter.text(
                    pos2(r.left() + MARK + 2.0, head.center().y),
                    Align2::LEFT_CENTER,
                    shown,
                    t::medium(12.0),
                    if faded { t::faint() } else { t::text() },
                );
            }
            if badge > 0.0 {
                painter.text(
                    pos2(r.right() - 5.0, head.center().y),
                    Align2::RIGHT_CENTER,
                    format!("×{shared}"),
                    t::mono(12.0),
                    if faded { t::faint() } else { t::muted() },
                );
            }
            // Where the pattern repeats inside a stretched clip.
            let plen = pat.length.max(1);
            let x_of = |tick: Tick| r.left() + (tick - c.offset) as f32 / BAR as f32 * bar_w;
            let mut k = c.offset.div_euclid(plen) + 1;
            while k * plen < c.offset + len {
                let x = x_of(k * plen);
                painter.line_segment(
                    [pos2(x, head.bottom()), pos2(x, r.bottom() - 1.0)],
                    Stroke::new(1.0_f32, code.gamma_multiply(if faded { 0.25 } else { 0.6 })),
                );
                k += 1;
            }
            // Content preview at true time: notes of every pass inside the clip.
            let area = Rect::from_min_max(
                pos2(r.left() + 1.0, head.bottom() + 2.0),
                pos2(r.right() - 1.0, r.bottom() - 2.0),
            );
            if area.height() >= 6.0
                && let Some(env) = &pat.automation
            {
                // Automation clip: its envelope across the clip (FL draws the curve).
                let p2 = painter.with_clip_rect(area.intersect(painter.clip_rect()));
                let ink = if faded { t::faint() } else { t::enc_green() };
                super::automation::paint_clip(&p2, area, env, &self.song, c.offset, len, plen, ink);
            } else if area.height() >= 6.0
                && let Some(inst_id) = pat.audio_clip
            {
                // Audio clip: its waveform across the clip (FL audio clip look).
                let p2 = painter.with_clip_rect(area.intersect(painter.clip_rect()));
                paint_audio_clip_wave(
                    &p2, area, view, &self.song, inst_id, c.offset, plen, bar_w, code, faded,
                );
            } else if area.height() >= 6.0 {
                let pitches = pat.notes.iter().filter(|n| !n.muted).map(|n| n.pitch);
                let low = pitches.clone().min().unwrap_or(0);
                let high = pitches.max().unwrap_or(low);
                let pass0 = c.offset.div_euclid(plen);
                let pass1 = (c.offset + len - 1).div_euclid(plen);
                let p2 = painter.with_clip_rect(area.intersect(painter.clip_rect()));
                for nn in pat.notes.iter().filter(|nn| !nn.muted) {
                    let y = area.top()
                        + (f32::from(high - nn.pitch) + 0.5) / (f32::from(high - low) + 1.0)
                            * area.height();
                    let ink = if faded { t::faint() } else { t::text() };
                    for pass in pass0..=pass1 {
                        let (s0, s1) = (nn.start + pass * plen, nn.end() + pass * plen);
                        if s1 <= c.offset || s0 >= c.offset + len {
                            continue;
                        }
                        let a = x_of(s0.max(c.offset));
                        let b = x_of(s1.min(c.offset + len));
                        p2.line_segment(
                            [pos2(a, y), pos2((b - 1.0).max(a + 1.5), y)],
                            Stroke::new(2.0_f32, ink),
                        );
                    }
                }
            }
            if kit::show_focus(ui, &resp) {
                kit::focus_ring(&painter, r, 6.0);
            }
            // Small ▾ marker at the clip's top-left: the clip menu.
            let marker_rect =
                Rect::from_min_size(face.min + vec2(1.0, 0.0), vec2(MARK, HEAD_STRIP));
            // Drawn, not a glyph: the UI font has no ▾.
            let mc = marker_rect.center();
            let mink = if faded {
                t::faint()
            } else {
                t::text().gamma_multiply(0.85)
            };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    mc + vec2(-3.5, -1.5),
                    mc + vec2(3.5, -1.5),
                    mc + vec2(0.0, 2.5),
                ],
                mink,
                Stroke::NONE,
            ));
            let marker_resp = ui.interact(
                marker_rect.intersect(view),
                ui.id().with(("clip-marker", i)),
                Sense::click(),
            );
            let popup_id = ui.id().with(("clip-menu", i));
            if marker_resp.clicked() && tool == Tool::Draw {
                marker_used[i] = true;
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            let mut menu_pick: Option<String> = None;
            let pat_color = pat.color;
            let id_colour = t::pattern_color(pat.id, None);
            let tr = self.tr();
            let pat_audio_clip = pat.audio_clip;
            kit::popup_menu(ui, popup_id, &marker_resp, 220.0, |ui| {
                if ui.button(tr.pl_open_in_rack).clicked() {
                    menu_pick = Some("rack".to_string());
                }
                if pat_audio_clip.is_some() {
                    if ui.button(tr.pl_open_audio_channel).clicked() {
                        menu_pick = Some("audio-channel".to_string());
                    }
                } else if ui.button(tr.pl_open_in_roll).clicked() {
                    menu_pick = Some("roll".to_string());
                }
                if ui.button(tr.pl_make_unique).clicked() {
                    menu_pick = Some("unique".to_string());
                }
                if ui
                    .button(if c.muted {
                        tr.pl_unmute_clip
                    } else {
                        tr.pl_mute_clip
                    })
                    .clicked()
                {
                    menu_pick = Some("mute".to_string());
                }
                if ui
                    .button(tr.pl_rename_pattern)
                    .on_hover_text(tr.pl_rename_pattern_tip)
                    .clicked()
                {
                    menu_pick = Some("rename-pat".to_string());
                }
                ui.menu_button(tr.pl_pattern_colour, |ui| {
                    kit::menu_style(ui);
                    match kit::colour_swatches(ui, pat_color, tr.pat_colour_by_id, Some(id_colour))
                    {
                        Some(None) => {
                            menu_pick = Some("pat-color-default".to_string());
                            ui.close_menu();
                        }
                        Some(Some(rgb)) => {
                            menu_pick = Some(format!("pat-color-{rgb:06X}"));
                            ui.close_menu();
                        }
                        None => {}
                    }
                });
                if ui.button(tr.pl_delete).clicked() {
                    menu_pick = Some("delete".to_string());
                }
            });
            if let Some(what) = menu_pick {
                ui.memory_mut(|m| m.close_popup());
                pending.push(Pending::Menu(i, what));
            }

            // Cursors.
            let hover_pos = resp.hover_pos();
            if self.song.clips.get(i).is_some() && resp.hovered() && !resp.dragged() {
                if let Some(hp) = hover_pos {
                    let dl = (hp.x - r.left()).abs();
                    let dr = (r.right() - hp.x).abs();
                    if (dl < EDGE || dr < EDGE) && r.width() > EDGE * 1.5 {
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    } else {
                        ui.ctx().set_cursor_icon(CursorIcon::Grab);
                    }
                } else {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
            }

            // Drag state lives in temp memory (one gesture at a time): the mode and
            // grab offset captured at press time, so release can commit without it.
            let drag_id = egui::Id::new("kfxl-playlist-drag");
            if matches!(tool, Tool::Draw | Tool::Select)
                && resp.drag_started()
                && let Some(o) = press_origin
            {
                // A press on the ▾ marker opens the menu, never starts a drag.
                let marker_rect0 =
                    Rect::from_min_size(face.min + vec2(1.0, 0.0), vec2(MARK, HEAD_STRIP));
                if !marker_rect0.contains(o) {
                    let m = if tool == Tool::Select {
                        DragMode::Move
                    } else if (o.x - r.left()).abs() < EDGE && r.width() > EDGE * 1.5 {
                        DragMode::Left
                    } else if (r.right() - o.x).abs() < EDGE && r.width() > EDGE * 1.5 {
                        DragMode::Right
                    } else {
                        DragMode::Move
                    };
                    let grab = tick_at(o.x, grid_l, bar_w) - c.start;
                    ui.data_mut(|d| {
                        d.insert_temp(
                            drag_id,
                            Some(PlDrag {
                                index: i,
                                mode: m,
                                grab,
                            }),
                        )
                    });
                } else {
                    ui.data_mut(|d| d.insert_temp(drag_id, None::<PlDrag>));
                }
            }
            let alt = ui.input(|i| i.modifiers.alt);
            let cur_drag: Option<PlDrag> = ui.data(|d| d.get_temp(drag_id)).flatten();
            let active = cur_drag.filter(|dd| dd.index == i);
            // Ghost preview while dragging.
            if resp.dragged()
                && let Some(pos) = resp.interact_pointer_pos()
                && let Some(dd) = active
            {
                ui.ctx().set_cursor_icon(if dd.mode == DragMode::Move {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::ResizeHorizontal
                });
                match dd.mode {
                    DragMode::Move => {
                        let ns = song_snap(
                            &meters,
                            tick_at(pos.x, grid_l, bar_w) - dd.grab,
                            grid_ticks,
                            alt,
                            true,
                        )
                        .max(0);
                        let nt = track_at(pos.y, grid.top(), lane_h).min(999);
                        if selection.len() > 1 && selection.contains(&i) {
                            let min_s = selection
                                .iter()
                                .filter_map(|&j| clips.get(j).map(|x| x.start))
                                .min()
                                .unwrap_or(0);
                            let min_t = selection
                                .iter()
                                .filter_map(|&j| clips.get(j).map(|x| x.track as i64))
                                .min()
                                .unwrap_or(0);
                            let dt = (ns - c.start).max(-min_s);
                            let dr = (nt as i64 - c.track as i64).max(-min_t);
                            for &j in &selection {
                                if let Some(cj) = clips.get(j) {
                                    let jlen = self.song.clip_len(cj);
                                    let js = (cj.start + dt)
                                        .min(keyframexl::sequence::MAX_BARS * BAR - jlen.max(1));
                                    let jt = (cj.track as i64 + dr).clamp(0, 999) as u32;
                                    painter.rect_stroke(
                                        clip_rect(js, jlen, jt),
                                        3.0,
                                        Stroke::new(1.5_f32, t::text()),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                        } else {
                            painter.rect_stroke(
                                clip_rect(ns, len, nt),
                                3.0,
                                Stroke::new(1.5_f32, t::text()),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    DragMode::Right => {
                        let min = if alt { 1 } else { grid_ticks.max(1) };
                        let end = song_snap(
                            &meters,
                            tick_at(pos.x, grid_l, bar_w),
                            grid_ticks,
                            alt,
                            true,
                        )
                        .max(c.start + min);
                        painter.rect_stroke(
                            clip_rect(c.start, end - c.start, c.track.min(999)),
                            3.0,
                            Stroke::new(1.5_f32, t::text()),
                            egui::StrokeKind::Inside,
                        );
                    }
                    DragMode::Left => {
                        let end = c.start + len;
                        let min = if alt { 1 } else { grid_ticks.max(1) };
                        let mut ns = song_snap(
                            &meters,
                            tick_at(pos.x, grid_l, bar_w),
                            grid_ticks,
                            alt,
                            true,
                        )
                        .clamp(0, end - min);
                        let lo = (c.start - c.offset).max(0);
                        let hi = c.start - c.offset + pat.length - 1;
                        if hi >= lo {
                            ns = ns.clamp(lo, hi);
                        }
                        painter.rect_stroke(
                            clip_rect(ns, end - ns, c.track.min(999)),
                            3.0,
                            Stroke::new(1.5_f32, t::text()),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
            if resp.drag_stopped()
                && !marker_used[i]
                && let Some(dd) = active
            {
                let pos = resp.interact_pointer_pos().or(resp.hover_pos());
                if let Some(pos) = pos {
                    let shift = ui.input(|i| i.modifiers.shift);
                    match dd.mode {
                        DragMode::Move => {
                            let ns = song_snap(
                                &meters,
                                tick_at(pos.x, grid_l, bar_w) - dd.grab,
                                grid_ticks,
                                alt,
                                true,
                            )
                            .max(0);
                            let nt = track_at(pos.y, grid.top(), lane_h).min(999);
                            let ns = ns.min(keyframexl::sequence::MAX_BARS * BAR - len.max(1));
                            if shift {
                                if ns != c.start || nt != c.track {
                                    pending.push(Pending::Duplicate {
                                        i,
                                        start: ns,
                                        track: nt,
                                    });
                                }
                            } else if ns != c.start || nt != c.track {
                                pending.push(Pending::Move {
                                    i,
                                    start: ns,
                                    track: nt,
                                });
                            }
                        }
                        DragMode::Right => {
                            let min = if alt { 1 } else { grid_ticks.max(1) };
                            let end = song_snap(
                                &meters,
                                tick_at(pos.x, grid_l, bar_w),
                                grid_ticks,
                                alt,
                                true,
                            )
                            .max(c.start + min);
                            let nl = (end - c.start).max(1);
                            if nl != len {
                                pending.push(Pending::Resize { i, len: nl });
                            }
                        }
                        DragMode::Left => {
                            let end = c.start + len;
                            let min = if alt { 1 } else { grid_ticks.max(1) };
                            let mut ns = song_snap(
                                &meters,
                                tick_at(pos.x, grid_l, bar_w),
                                grid_ticks,
                                alt,
                                true,
                            )
                            .clamp(0, end - min);
                            let lo = (c.start - c.offset).max(0);
                            let hi = c.start - c.offset + pat.length - 1;
                            if hi >= lo {
                                ns = ns.clamp(lo, hi);
                            } else {
                                ns = c.start;
                            }
                            let no = c.offset + (ns - c.start);
                            let nl = end - ns;
                            if ns != c.start && no >= 0 && no < pat.length && nl >= min {
                                pending.push(Pending::Trim {
                                    i,
                                    start: ns,
                                    offset: no,
                                    len: nl,
                                });
                            }
                        }
                    }
                }
                ui.data_mut(|d| d.insert_temp(drag_id, None::<PlDrag>));
            }
            let tr = self.tr();
            let shared_str = if shared > 1 {
                tr.pl_shares_notes.fmt(tr.lang, shared - 1)
            } else {
                String::new()
            };
            let tip = crate::i18n::fill(
                tr.pl_clip_tip,
                &[
                    ("pat", &pat.name),
                    ("pos", &song_bar_pos(&self.song.meters, c.start)),
                    ("shared", &shared_str),
                ],
            );
            let resp = resp.on_hover_text(tip);
            if marker_used[i] {
                continue;
            }
            if tool == Tool::Draw && resp.double_clicked() {
                pending.push(Pending::Open(i));
            } else if matches!(tool, Tool::Draw | Tool::Select) && resp.clicked() {
                pending.push(Pending::Select(i));
            }
        }

        let can_edit_grid = ui.is_enabled() && !self.welcome_open;
        let box_id = ui.id().with("playlist-box-v2");
        let box_base_id = ui.id().with("playlist-box-base");
        let mut pbox: Option<PlBox> = ui.data(|d| d.get_temp(box_id)).flatten();
        let mut box_base: Vec<usize> = ui.data(|d| d.get_temp(box_base_id)).unwrap_or_default();

        let (right_down, right_pressed, right_pos, shift_mod, ctrl_mod) = ui.input(|i| {
            (
                i.pointer.secondary_down(),
                i.pointer.secondary_pressed(),
                i.pointer.interact_pos(),
                i.modifiers.shift,
                i.modifiers.command,
            )
        });

        // RMB press inside the grid:
        // - Dragging (> 4 px) sweeps a musical-coordinate marquee selection box.
        // - A quick right-click on a clip (no drag) still deletes that clip.
        if can_edit_grid
            && right_pressed
            && ui.rect_contains_pointer(grid_seen)
            && let Some(pos) = right_pos
            && grid_seen.contains(pos)
            && !marker_used.iter().enumerate().any(|(i, used)| {
                *used
                    && clip_rect(
                        clips[i].start,
                        self.song.clip_len(&clips[i]),
                        clips[i].track.min(999),
                    )
                    .contains(pos)
            })
        {
            box_base = if shift_mod {
                selection.clone()
            } else {
                Vec::new()
            };
            pbox = Some(PlBox {
                origin_screen: pos,
                origin_tick: tick_at(pos.x, grid_l, bar_w),
                origin_track: ((pos.y - grid.top()) / lane_h).max(0.0),
                right: true,
                dragging: false,
                add: shift_mod,
            });
        }

        // LMB Select tool or Ctrl+drag in Draw tool starts the same musical marquee.
        if can_edit_grid
            && pbox.is_none()
            && (tool == Tool::Select || (tool == Tool::Draw && ctrl_mod))
            && grid_resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(o) = press_origin
            && grid_seen.contains(o)
        {
            box_base = if shift_mod {
                selection.clone()
            } else {
                Vec::new()
            };
            pbox = Some(PlBox {
                origin_screen: o,
                origin_tick: tick_at(o.x, grid_l, bar_w),
                origin_track: ((o.y - grid.top()) / lane_h).max(0.0),
                right: false,
                dragging: true,
                add: shift_mod,
            });
        }

        // Process a brush once per clip per gesture, even when the pointer stays still.
        let brush_id = ui.id().with("playlist-brush-seen");
        let down = ui.input(|i| i.pointer.primary_down());
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let mut seen: Vec<(Tick, u32, u64)> = ui.data(|d| d.get_temp(brush_id)).unwrap_or_default();
        if pressed || !down {
            seen.clear();
        }
        if can_edit_grid
            && down
            && ui.rect_contains_pointer(grid_seen)
            && matches!(tool, Tool::Delete | Tool::Mute | Tool::Slice)
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && grid_seen.contains(pos)
            && let Some((i, c)) = clips.iter().enumerate().rev().find(|(_, c)| {
                clip_rect(c.start, self.song.clip_len(c), c.track.min(999)).contains(pos)
            })
        {
            let key = (c.start, c.track, c.pattern);
            if !seen.contains(&key) {
                seen.push(key);
                match tool {
                    Tool::Delete => pending.push(Pending::Delete(i)),
                    Tool::Mute => pending.push(Pending::Menu(i, "mute".into())),
                    Tool::Slice if pressed => {
                        let len = self.song.clip_len(c);
                        let at = song_snap(
                            &meters,
                            tick_at(pos.x, grid_l, bar_w),
                            grid_ticks,
                            ui.input(|i| i.modifiers.alt),
                            true,
                        );
                        if let Some((left, right_len)) = slice_lengths(c.start, len, at) {
                            self.checkpoint();
                            let mut right = *c;
                            right.start = at;
                            right.offset = (c.offset + at - c.start)
                                % self
                                    .song
                                    .patterns
                                    .iter()
                                    .find(|p| p.id == c.pattern)
                                    .map(|p| p.length.max(1))
                                    .unwrap_or(1);
                            right.length = Some(right_len);
                            self.song.clips[i].length = Some(left);
                            self.last_clip = Some((c.pattern, c.offset, Some(left)));
                            self.song.clips.push(right);
                            self.publish();
                            self.say(Tone::Quiet, self.tr().pl_clip_sliced);
                        }
                    }
                    _ => {}
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(brush_id, seen));

        let zoom_drag_id = ui.id().with("playlist-zoom-drag");
        let mut zoom_origin: Option<Pos2> = ui.data(|d| d.get_temp(zoom_drag_id)).flatten();
        if can_edit_grid && tool == Tool::Zoom && grid_resp.drag_started() {
            zoom_origin = press_origin;
        }
        if can_edit_grid
            && tool == Tool::Zoom
            && (grid_resp.clicked() || grid_resp.secondary_clicked() || grid_resp.drag_stopped())
        {
            if let Some(pos) = grid_resp.interact_pointer_pos().or(grid_resp.hover_pos()) {
                let old = bar_w;
                let a = zoom_origin.unwrap_or(pos);
                let span = (a.x - pos.x).abs();
                let new = if span > 4.0 {
                    (old * ui.clip_rect().width().max(1.0) / span).clamp(BAR_MIN, BAR_MAX)
                } else {
                    (old * if grid_resp.secondary_clicked() {
                        0.8
                    } else {
                        1.25
                    })
                    .clamp(BAR_MIN, BAR_MAX)
                };
                ui.data_mut(|d| d.insert_temp(zoom_id, new));
                let center = if span > 4.0 {
                    (a.x + pos.x) * 0.5
                } else {
                    pos.x
                };
                // The zoomed span (or the clicked point) ends up in the middle of the view.
                let content = center - view.left() + scroll_x;
                scroll_x = (content * new / old - view.width() / 2.0).max(0.0);
            }
            zoom_origin = None;
        }
        ui.data_mut(|d| d.insert_temp(zoom_drag_id, zoom_origin));

        // Update / finalize active marquee selection (musical coordinates: survives scrolling!).
        if let Some(mut b) = pbox {
            let held = if b.right { right_down } else { down };
            let cur_pos = ui
                .input(|i| i.pointer.interact_pos().or_else(|| i.pointer.hover_pos()))
                .unwrap_or(b.origin_screen);
            if b.right && !b.dragging && (cur_pos - b.origin_screen).length() > 4.0 {
                b.dragging = true;
            }
            if b.dragging {
                let cur_tick = tick_at(cur_pos.x, grid_l, bar_w);
                let cur_track = ((cur_pos.y - grid.top()) / lane_h).max(0.0);
                let (t_min, t_max) = (b.origin_tick.min(cur_tick), b.origin_tick.max(cur_tick));
                let (tr_min, tr_max) =
                    (b.origin_track.min(cur_track), b.origin_track.max(cur_track));
                let a_screen = pos2(
                    grid_l + b.origin_tick as f32 / BAR as f32 * bar_w,
                    grid.top() + b.origin_track * lane_h,
                );
                let rect = Rect::from_two_pos(a_screen, cur_pos).intersect(grid_seen);
                if rect.is_positive() {
                    painter.rect_filled(rect, 2.0, t::text().gamma_multiply(0.08));
                    painter.rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.5_f32, t::text()),
                        egui::StrokeKind::Inside,
                    );
                }
                let hits: Vec<usize> = clips
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        let clen = self.song.clip_len(c);
                        let c_end = c.start + clen;
                        let hit_t = if t_max > t_min {
                            c.start < t_max && c_end > t_min
                        } else {
                            c.start <= t_min && c_end >= t_min
                        };
                        let tr = c.track.min(999) as f32;
                        let hit_tr = (tr + 1.0) > tr_min && tr < tr_max;
                        hit_t && hit_tr
                    })
                    .map(|(i, _)| i)
                    .collect();
                selection = if b.add { box_base.clone() } else { Vec::new() };
                for i in hits {
                    if !selection.contains(&i) {
                        selection.push(i);
                    }
                }
                self.clip = if selection.len() == 1 {
                    Some(selection[0])
                } else {
                    None
                };
            }
            if !held {
                if b.right && !b.dragging {
                    // Quick right-click (no drag): delete clip under pointer, or clear selection on empty grid.
                    if let Some((i, _)) = clips.iter().enumerate().rev().find(|(_, c)| {
                        clip_rect(c.start, self.song.clip_len(c), c.track.min(999))
                            .contains(b.origin_screen)
                    }) {
                        self.checkpoint();
                        self.song.clips.remove(i);
                        self.clip = None;
                        selection.clear();
                        self.publish();
                        self.say(Tone::Quiet, self.tr().clips_deleted_msg(1));
                    } else {
                        selection.clear();
                        self.clip = None;
                    }
                } else if b.dragging {
                    let tr = self.tr();
                    self.say(
                        Tone::Quiet,
                        tr.pl_clips_selected.fmt(tr.lang, selection.len()),
                    );
                }
                pbox = None;
            } else {
                pbox = Some(b);
            }
        }
        ui.data_mut(|d| {
            d.insert_temp(box_id, pbox);
            d.insert_temp(box_base_id, box_base);
        });
        let paint_id = ui.id().with("playlist-paint-active");
        let mut painting: Option<(Tick, u32, bool)> = ui.data(|d| d.get_temp(paint_id)).flatten();
        if can_edit_grid
            && tool == Tool::Paint
            && pressed
            && ui.rect_contains_pointer(grid_seen)
            && let Some(pos) = press_origin.filter(|p| grid_seen.contains(*p))
        {
            painting = Some((
                song_snap(
                    &meters,
                    tick_at(pos.x, grid_l, bar_w),
                    grid_ticks,
                    false,
                    false,
                ),
                track_at(pos.y, grid.top(), lane_h),
                false,
            ));
        }
        if let Some((start, track, mut changed)) = painting {
            if down {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    let end = song_snap(
                        &meters,
                        tick_at(pos.x, grid_l, bar_w),
                        grid_ticks,
                        false,
                        false,
                    );
                    let proto = self.make_playlist_clip(0, track);
                    let len = self.song.clip_len(&proto).max(1);
                    for at in paint_positions(start, end, len) {
                        if at + len <= keyframexl::sequence::MAX_BARS * BAR
                            && !self.song.clips.iter().any(|c| {
                                c.track == track
                                    && c.start < at + len
                                    && at < c.start + self.song.clip_len(c)
                            })
                        {
                            if !changed {
                                self.checkpoint();
                                changed = true;
                            }
                            let clip = self.make_playlist_clip(at, track);
                            self.song.clips.push(clip);
                            self.publish();
                        }
                    }
                }
                painting = Some((start, track, changed));
            } else {
                painting = None;
            }
        }
        ui.data_mut(|d| d.insert_temp(paint_id, painting));

        // Browser audio file dragged directly onto the playlist grid.
        if let Some(drag) = grid_resp.dnd_hover_payload::<super::files::FileDrag>()
            && super::files::is_audio(&drag.0)
            && let Some(pos) = ui.ctx().pointer_hover_pos()
            && grid_seen.contains(pos)
        {
            let alt = ui.input(|i| i.modifiers.alt);
            let start = song_snap(
                &meters,
                tick_at(pos.x, grid_l, bar_w),
                grid_ticks,
                alt,
                false,
            )
            .max(0);
            let track = track_at(pos.y, grid.top(), lane_h).min(999);
            let ghost = clip_rect(start, BAR * 4, track);
            painter.rect_filled(ghost, 2.0, t::brand().gamma_multiply(0.18));
            painter.rect_stroke(
                ghost,
                2.0,
                Stroke::new(1.5_f32, t::brand()),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(drag) = grid_resp.dnd_release_payload::<super::files::FileDrag>()
            && super::files::is_audio(&drag.0)
            && let Some(pos) = ui.ctx().pointer_hover_pos()
        {
            let alt = ui.input(|i| i.modifiers.alt);
            let start = song_snap(
                &meters,
                tick_at(pos.x, grid_l, bar_w),
                grid_ticks,
                alt,
                false,
            )
            .max(0);
            let track = track_at(pos.y, grid.top(), lane_h).min(999);
            self.load_audio_clip(drag.0.clone(), start, track);
        }

        // Empty-cell placement: click where no clip covers.
        if tool == Tool::Draw
            && grid_resp.clicked()
            && let Some(pos) = grid_resp.interact_pointer_pos().or(grid_resp.hover_pos())
        {
            let on_clip = clips.iter().enumerate().any(|(i, c)| {
                if marker_used.get(i).copied().unwrap_or(false) {
                    return false;
                }
                let len = self.song.clip_len(c);
                clip_rect(c.start, len, c.track.min(999)).contains(pos)
            });
            if !on_clip && grid_seen.contains(pos) {
                let alt = ui.input(|i| i.modifiers.alt);
                let start = song_snap(
                    &meters,
                    tick_at(pos.x, grid_l, bar_w),
                    grid_ticks,
                    alt,
                    false,
                )
                .max(0);
                let track = track_at(pos.y, grid.top(), lane_h).min(999);
                let start = start.min(keyframexl::sequence::MAX_BARS * BAR - BAR);
                self.checkpoint();
                let clip = self.make_playlist_clip(start, track);
                self.song.clips.push(clip);
                self.clip = Some(self.song.clips.len() - 1);
                self.publish();
                let tr = self.tr();
                self.say(
                    Tone::Quiet,
                    crate::i18n::fill(
                        tr.pl_placed_at_track,
                        &[
                            ("pat", &self.pattern().name),
                            ("pos", &song_bar_pos(&self.song.meters, start)),
                            ("track", &(track + 1).to_string()),
                        ],
                    ),
                );
            }
        }

        ui.set_clip_rect(full_clip);
        let painter = painter_full;
        // Ruler: click or drag moves the song playhead there (FL: and to Song mode).
        let ruler = Rect::from_min_max(
            pos2(grid_l, well.top()),
            pos2(grid.right(), well.top() + RULER_H),
        )
        .intersect(view);
        let rr = ui.interact(ruler, ui.id().with("ruler"), Sense::click_and_drag());
        if rr.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        // Right-click the ruler: time signature from the bar under the pointer.
        let sig_id = ui.id().with("ruler-sig-bar");
        let mark_id = ui.id().with("ruler-marker");
        let chip_at = |p: Pos2| {
            marker_chips
                .iter()
                .find(|(r, _)| r.expand(2.0).contains(p))
                .map(|&(_, i)| i)
        };
        if rr.secondary_clicked()
            && let Some(pos) = rr.interact_pointer_pos()
        {
            let (bar, ..) = bar_at(&meters, tick_at(pos.x, grid_l, bar_w));
            ui.data_mut(|d| d.insert_temp(sig_id, bar));
            let tick = song_snap(
                &meters,
                tick_at(pos.x, grid_l, bar_w),
                grid_ticks,
                false,
                false,
            )
            .max(0);
            ui.data_mut(|d| d.insert_temp(mark_id, (tick, chip_at(pos))));
        }
        if let Some(hover) = rr.hover_pos()
            && let Some(i) = chip_at(hover)
        {
            let tr = self.tr();
            let marker_tip =
                crate::i18n::fill(tr.pl_marker_tip, &[("name", &self.song.markers[i].name)]);
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                ui.id().with("marker-tip"),
                |ui| {
                    ui.label(&marker_tip);
                },
            );
        }
        rr.context_menu(|ui| {
            kit::menu_style(ui);
            let tr = self.tr();
            let (tick, marker): (Tick, Option<usize>) =
                ui.data(|d| d.get_temp(mark_id)).unwrap_or((0, None));
            match marker.filter(|&i| i < self.song.markers.len()) {
                Some(i) => {
                    kit::menu_header(
                        ui,
                        egui::RichText::new(tr.pl_marker_header).color(t::muted()),
                    );
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.song.markers[i].name)
                            .char_limit(64)
                            .desired_width(180.0),
                    );
                    if edit.gained_focus() {
                        self.checkpoint();
                    }
                    if ui.button(tr.pl_loop_to_next_marker).clicked() {
                        self.loop_marker(i);
                        ui.close_menu();
                    }
                    if ui.button(tr.pl_remove_marker).clicked() {
                        self.checkpoint();
                        let m = self.song.markers.remove(i);
                        self.say(
                            Tone::Quiet,
                            crate::i18n::fill(tr.pl_marker_removed, &[("name", &m.name)]),
                        );
                        ui.close_menu();
                    }
                }
                None => {
                    if ui
                        .button(crate::i18n::fill(
                            tr.pl_add_marker_at,
                            &[("pos", &song_bar_pos(&self.song.meters, tick))],
                        ))
                        .clicked()
                    {
                        self.checkpoint();
                        let i = self.song.add_marker(tick);
                        let mname = self.song.markers[i].name.clone();
                        self.say(
                            Tone::Quiet,
                            crate::i18n::fill(tr.pl_marker_added, &[("name", &mname)]),
                        );
                        ui.close_menu();
                    }
                }
            }
            if self.song.loop_range.is_some() && ui.button(tr.pl_clear_time_sel).clicked() {
                self.song.loop_range = None;
                self.publish();
                ui.close_menu();
            }
            ui.separator();
            let bar: u32 = ui.data(|d| d.get_temp(sig_id)).unwrap_or(0);
            let tick = self.song.bar_tick(bar);
            let (_, _, _, num, den) = bar_at(&self.song.meters, tick);
            kit::menu_header(
                ui,
                egui::RichText::new(crate::i18n::fill(
                    tr.pl_time_sig_from_bar,
                    &[("bar", &(bar + 1).to_string())],
                ))
                .color(t::muted()),
            );
            for (n, d) in [
                (2, 4),
                (3, 4),
                (4, 4),
                (5, 4),
                (6, 8),
                (7, 8),
                (9, 8),
                (12, 8),
            ] {
                if kit::menu_check(ui, (n, d) == (num, den), &format!("{n}/{d}"), "").clicked() {
                    self.set_meter(bar, n, d);
                    ui.close_menu();
                }
            }
            let here = self.song.meters.iter().any(|m| m.bar == bar && bar > 0);
            ui.separator();
            if ui
                .add_enabled(here, egui::Button::new(tr.pl_remove_change_here))
                .clicked()
            {
                self.checkpoint();
                self.song.meters.retain(|m| m.bar != bar);
                if self.song.meters.iter().all(|m| (m.num, m.den) == (4, 4)) {
                    self.song.meters.clear();
                }
                self.publish();
                self.say(
                    Tone::Quiet,
                    crate::i18n::fill(tr.pl_meter_removed, &[("bar", &(bar + 1).to_string())]),
                );
                ui.close_menu();
            }
        });
        // Click: playhead there (and Song mode). Drag: a time selection the
        // song loops (FL). Double-click: the whole song plays again.
        let alt = ui.input(|i| i.modifiers.alt);
        let ruler_tick =
            |x: f32| song_snap(&meters, tick_at(x, grid_l, bar_w), grid_ticks, alt, true).max(0);
        let on_chip = rr.interact_pointer_pos().and_then(chip_at);
        if rr.double_clicked()
            && let Some(i) = on_chip
        {
            self.loop_marker(i);
        } else if rr.double_clicked() {
            if self.song.loop_range.take().is_some() {
                self.publish();
                self.say(Tone::Quiet, self.tr().pl_time_sel_cleared);
            }
        } else if rr.clicked()
            && let Some(pos) = rr.interact_pointer_pos()
        {
            let at = match on_chip {
                Some(i) => self.song.markers[i].tick,
                None => song_snap(
                    &meters,
                    tick_at(pos.x, grid_l, bar_w),
                    grid_ticks,
                    alt,
                    false,
                )
                .max(0),
            };
            self.set_song_mode(true);
            self.seek_tick(at);
        }
        if rr.dragged_by(egui::PointerButton::Primary)
            && let (Some(from), Some(to)) = (
                ui.input(|i| i.pointer.press_origin()),
                rr.interact_pointer_pos(),
            )
        {
            let (a, b) = (ruler_tick(from.x), ruler_tick(to.x));
            if a != b {
                self.song.loop_range = Some((a.min(b), a.max(b)));
            }
        }
        if rr.drag_stopped_by(egui::PointerButton::Primary)
            && let Some((a, b)) = self.song.loop_range
        {
            self.set_song_mode(true);
            self.publish();
            self.seek_tick(a);
            let tr = self.tr();
            self.say(
                Tone::Quiet,
                crate::i18n::fill(
                    tr.pl_looping_span,
                    &[
                        ("a", &song_bar_pos(&meters, a)),
                        ("b", &song_bar_pos(&meters, b)),
                    ],
                ),
            );
        }
        // The time selection: a band on the ruler and a wash over the tracks.
        if let Some((a, b)) = self.song.loop_range {
            let xa = grid_l + a as f32 / BAR as f32 * bar_w;
            let xb = grid_l + b as f32 / BAR as f32 * bar_w;
            let band = Rect::from_min_max(pos2(xa, well.top()), pos2(xb, well.top() + 5.0));
            painter.rect_filled(band, 0.0, t::wait());
            let wash = Rect::from_min_max(pos2(xa, well.top() + 5.0), pos2(xb, grid.bottom()));
            painter.rect_filled(wash, 0.0, t::wait().gamma_multiply(0.07));
            for x in [xa, xb] {
                painter.line_segment(
                    [pos2(x, well.top()), pos2(x, grid.bottom())],
                    Stroke::new(1.0_f32, t::wait().gamma_multiply(0.7)),
                );
            }
        }

        // The pinned tempo lane between the ruler and track 1.
        let lane_top = well.top() + RULER_H;
        self.tempo_lane(
            ui,
            &super::tempo_lane::LaneGeo {
                head: Rect::from_min_max(
                    pos2(well.left(), lane_top),
                    pos2(view.left(), tracks_top),
                ),
                lane: Rect::from_min_max(
                    pos2(grid_l.max(view.left()), lane_top),
                    pos2(grid.right().min(view.right()), tracks_top),
                ),
                grid_l,
                bar_w,
                snap: grid_ticks,
                bars: bar_lines.clone(),
            },
        );

        // Song playhead at the absolute position.
        if self.song_mode {
            let steps = self.position_step() as f32;
            let x = grid_l + steps / 16.0 * bar_w;
            if x >= grid.left() && x <= grid.right() {
                painter.line_segment(
                    [pos2(x, well.top() + RULER_H - 2.0), pos2(x, grid.bottom())],
                    Stroke::new(1.5_f32, t::text()),
                );
                let y = well.top() + RULER_H - 2.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![pos2(x - 6.0, y - 8.0), pos2(x + 6.0, y - 8.0), pos2(x, y)],
                    t::text(),
                    Stroke::NONE,
                ));
            }
            // Follow the playhead while playing (FL): keep it on screen.
            if self.follow && self.is_playing() && max_scroll > 0.0 {
                let content_x = steps / 16.0 * bar_w;
                let margin_l = 24.0;
                let margin_r = 48.0;
                if content_x < scroll_x + margin_l || content_x > scroll_x + view.width() - margin_r
                {
                    scroll_x = (content_x - view.width() * 0.3).clamp(0.0, max_scroll);
                }
            }
        }

        // Sideways scrollbar under the grid (FL): drag the thumb or click the track.
        if max_scroll > 0.0 {
            let track = Rect::from_min_max(
                pos2(view.left() + 4.0, well.bottom() - 9.0),
                pos2(view.right() - 4.0, well.bottom() - 3.0),
            );
            let frac = view.width() / (view.width() + max_scroll);
            let thumb_w = (track.width() * frac).max(24.0);
            let thumb_x = track.left() + (track.width() - thumb_w) * (scroll_x / max_scroll);
            let thumb =
                Rect::from_min_size(pos2(thumb_x, track.top()), vec2(thumb_w, track.height()));
            let sr = ui.interact(track, ui.id().with("pl-hscroll"), Sense::click_and_drag());
            if (sr.dragged() || sr.clicked())
                && let Some(px) = sr.interact_pointer_pos().map(|p| p.x)
            {
                let at = ((px - thumb_w / 2.0 - track.left()) / (track.width() - thumb_w).max(1.0))
                    .clamp(0.0, 1.0);
                scroll_x = at * max_scroll;
            }
            let p = ui.painter();
            p.rect_filled(track, 3.0, t::sunken());
            p.rect_filled(
                thumb,
                3.0,
                if sr.hovered() || sr.dragged() {
                    t::strong()
                } else {
                    t::ctl_hover()
                },
            );
        }
        ui.data_mut(|d| d.insert_temp(scroll_id, scroll_x));
        ui.data_mut(|d| d.insert_temp(selection_id, selection.clone()));
        for p in pending {
            match p {
                Pending::Select(i) => {
                    if i < clips.len() {
                        if shift_mod {
                            if let Some(pos) = selection.iter().position(|&x| x == i) {
                                selection.remove(pos);
                            } else {
                                selection.push(i);
                            }
                            self.clip = if selection.len() == 1 {
                                Some(selection[0])
                            } else {
                                None
                            };
                        } else {
                            self.clip = if tool == Tool::Select { None } else { Some(i) };
                            selection = vec![i];
                        }
                        self.select_pattern_keep_clip(clips[i].pattern);
                        self.last_clip = Some((clips[i].pattern, clips[i].offset, clips[i].length));
                    }
                }
                Pending::Open(i)
                    if i < clips.len()
                        && self
                            .song
                            .patterns
                            .iter()
                            .any(|p| p.id == clips[i].pattern && p.automation.is_some()) =>
                {
                    // An automation clip opens its envelope editor.
                    self.clip = Some(i);
                    self.auto_open = Some(clips[i].pattern);
                }
                Pending::Open(i)
                    if i < clips.len()
                        && self
                            .song
                            .patterns
                            .iter()
                            .any(|p| p.id == clips[i].pattern && p.audio_clip.is_some()) =>
                {
                    // An audio clip opens its sampler/audio channel window (FL).
                    self.clip = Some(i);
                    self.select_pattern(clips[i].pattern);
                    if let Some(inst) = self
                        .song
                        .patterns
                        .iter()
                        .find(|p| p.id == clips[i].pattern)
                        .and_then(|p| p.audio_clip)
                    {
                        self.win(Cmd::Show(Tab::Instrument { id: inst }));
                    }
                }
                Pending::Open(i) => {
                    if i < clips.len() {
                        self.clip = Some(i);
                        self.select_pattern(clips[i].pattern);
                        let inst = self.instrument;
                        self.open_roll(inst);
                    }
                }
                Pending::Move { i, start, track } => {
                    if i < self.song.clips.len() {
                        self.checkpoint();
                        if selection.len() > 1 && selection.contains(&i) {
                            let dt = start - clips[i].start;
                            let dr = track as i64 - clips[i].track as i64;
                            let dt = dt
                                .max(-selection.iter().map(|&j| clips[j].start).min().unwrap_or(0));
                            let dr = dr.max(
                                -selection
                                    .iter()
                                    .map(|&j| clips[j].track as i64)
                                    .min()
                                    .unwrap_or(0),
                            );
                            for &j in &selection {
                                if j < self.song.clips.len() {
                                    self.song.clips[j].start = (clips[j].start + dt).min(
                                        keyframexl::sequence::MAX_BARS * BAR
                                            - self.song.clip_len(&clips[j]),
                                    );
                                    self.song.clips[j].track =
                                        (clips[j].track as i64 + dr).clamp(0, 999) as u32;
                                }
                            }
                            self.clip = None;
                            self.publish();
                            let tr = self.tr();
                            self.say(Tone::Quiet, tr.pl_clips_moved.fmt(tr.lang, selection.len()));
                        } else {
                            self.song.clips[i].start = start;
                            self.song.clips[i].track = track;
                            let moved = self.song.clips[i];
                            self.last_clip = Some((moved.pattern, moved.offset, moved.length));
                            selection = vec![i];
                            self.clip = if tool == Tool::Select { None } else { Some(i) };
                            self.publish();
                            let tr = self.tr();
                            self.say(
                                Tone::Quiet,
                                crate::i18n::fill(
                                    tr.pl_clip_moved,
                                    &[
                                        ("pos", &song_bar_pos(&self.song.meters, start)),
                                        ("track", &(track + 1).to_string()),
                                    ],
                                ),
                            );
                        }
                    }
                }
                Pending::Resize { i, len } => {
                    if i < self.song.clips.len() {
                        self.checkpoint();
                        self.song.clips[i].length = Some(len.max(1));
                        let c = self.song.clips[i];
                        self.select_pattern_keep_clip(c.pattern);
                        self.last_clip = Some((c.pattern, c.offset, c.length));
                        self.clip = Some(i);
                        self.publish();
                        let tr = self.tr();
                        self.say(
                            Tone::Quiet,
                            crate::i18n::fill(
                                tr.pl_clip_length,
                                &[
                                    ("len", &len.to_string()),
                                    (
                                        "pos",
                                        &song_bar_pos(&self.song.meters, self.song.clips[i].start),
                                    ),
                                ],
                            ),
                        );
                    }
                }
                Pending::Trim {
                    i,
                    start,
                    offset,
                    len,
                } => {
                    if i < self.song.clips.len() {
                        self.checkpoint();
                        self.song.clips[i].start = start;
                        self.song.clips[i].offset = offset;
                        self.song.clips[i].length = Some(len.max(1));
                        let c = self.song.clips[i];
                        self.select_pattern_keep_clip(c.pattern);
                        self.last_clip = Some((c.pattern, c.offset, c.length));
                        self.clip = Some(i);
                        self.publish();
                        let tr = self.tr();
                        self.say(
                            Tone::Quiet,
                            crate::i18n::fill(
                                tr.pl_clip_trimmed,
                                &[
                                    ("len", &len.to_string()),
                                    ("pos", &song_bar_pos(&self.song.meters, start)),
                                ],
                            ),
                        );
                    }
                }
                Pending::Duplicate { i, start, track } => {
                    if i < self.song.clips.len() {
                        self.checkpoint();
                        if selection.len() > 1 && selection.contains(&i) {
                            let dt = start - clips[i].start;
                            let dr = track as i64 - clips[i].track as i64;
                            let dt = dt
                                .max(-selection.iter().map(|&j| clips[j].start).min().unwrap_or(0));
                            let dr = dr.max(
                                -selection
                                    .iter()
                                    .map(|&j| clips[j].track as i64)
                                    .min()
                                    .unwrap_or(0),
                            );
                            let mut new_sel = Vec::with_capacity(selection.len());
                            for &j in &selection {
                                if let Some(orig) = clips.get(j) {
                                    let mut nc = *orig;
                                    nc.start = (orig.start + dt).min(
                                        keyframexl::sequence::MAX_BARS * BAR
                                            - self.song.clip_len(orig),
                                    );
                                    nc.track = (orig.track as i64 + dr).clamp(0, 999) as u32;
                                    self.song.clips.push(nc);
                                    new_sel.push(self.song.clips.len() - 1);
                                }
                            }
                            selection = new_sel;
                            self.clip = None;
                            self.publish();
                            let tr = self.tr();
                            self.say(
                                Tone::Quiet,
                                tr.pl_clips_duplicated.fmt(tr.lang, selection.len()),
                            );
                        } else {
                            let mut nc = self.song.clips[i];
                            nc.start = start;
                            nc.track = track;
                            self.song.clips.push(nc);
                            let ni = self.song.clips.len() - 1;
                            self.clip = Some(ni);
                            selection = vec![ni];
                            self.publish();
                            let tr = self.tr();
                            self.say(
                                Tone::Quiet,
                                crate::i18n::fill(
                                    tr.pl_clip_duplicated,
                                    &[
                                        ("pos", &song_bar_pos(&self.song.meters, start)),
                                        ("track", &(track + 1).to_string()),
                                    ],
                                ),
                            );
                        }
                    }
                }
                Pending::Delete(i) => {
                    self.delete_clip(i);
                    selection.clear();
                }
                Pending::Menu(i, what) => {
                    if i >= clips.len() {
                        continue;
                    }
                    match what.as_str() {
                        "rack" => {
                            self.clip = Some(i);
                            self.select_pattern(clips[i].pattern);
                            self.win(Cmd::Show(Tab::Rack));
                        }
                        "roll" => {
                            self.clip = Some(i);
                            self.select_pattern(clips[i].pattern);
                            let inst = self.instrument;
                            self.open_roll(inst);
                        }
                        "audio-channel" => {
                            self.clip = Some(i);
                            self.select_pattern(clips[i].pattern);
                            if let Some(inst) = self
                                .song
                                .patterns
                                .iter()
                                .find(|p| p.id == clips[i].pattern)
                                .and_then(|p| p.audio_clip)
                            {
                                self.set_lead_channel(inst);
                                self.win(Cmd::Show(Tab::Instrument { id: inst }));
                            }
                        }
                        "unique" => self.make_unique(i),
                        "mute" => self.toggle_clip_mute(i),
                        "delete" => self.delete_clip(i),
                        "rename-pat" => {
                            let pid = clips[i].pattern;
                            if let Some(p) = self.song.patterns.iter().find(|p| p.id == pid) {
                                let name = p.name.clone();
                                self.clip = Some(i);
                                self.select_pattern(pid);
                                self.renaming = Some((pid, name));
                            }
                        }
                        "pat-color-default" => {
                            let pid = clips[i].pattern;
                            self.checkpoint();
                            if let Some(p) = self.song.pattern_mut(pid) {
                                p.color = None;
                            }
                            self.publish();
                            self.say(Tone::Quiet, self.tr().pat_colour_reset_status);
                        }
                        w if w.starts_with("pat-color-") => {
                            let pid = clips[i].pattern;
                            let hex = &w["pat-color-".len()..];
                            if let Ok(v) = u32::from_str_radix(hex, 16) {
                                self.checkpoint();
                                if let Some(p) = self.song.pattern_mut(pid) {
                                    p.color = Some(v);
                                }
                                self.publish();
                                let tr = self.tr();
                                self.say(
                                    Tone::Quiet,
                                    crate::i18n::fill(tr.pat_colour_set_status, &[("hex", hex)]),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(selection_id, selection));
    }
}
