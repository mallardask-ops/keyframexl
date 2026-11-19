//! The tempo lane: a pinned row under the playlist ruler that draws and edits
//! the song's tempo map (FL's tempo track). Song-level, so it never scrolls
//! vertically and no track mute or clip edit touches it.

use super::playlist::{song_bar_pos, song_snap};
use super::{App, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::sequence::{BAR, Tick};
use keyframexl::tempo::{TEMPO_MAX, TEMPO_MIN, TempoPoint, TempoShape, TempoTable};

/// Lane height: open and folded.
pub(super) const LANE_OPEN: f32 = 48.0;
pub(super) const LANE_FOLDED: f32 = 16.0;

/// Where the lane sits and how ticks map to x.
pub(super) struct LaneGeo {
    /// The lane's head cell, in the track column.
    pub head: Rect,
    /// The lane's visible part over the grid.
    pub lane: Rect,
    pub grid_l: f32,
    pub bar_w: f32,
    pub snap: Tick,
    /// Bar lines: x and whether it is a strong (every 4th) bar.
    pub bars: Vec<(f32, bool)>,
}

impl App {
    pub(super) fn tempo_lane_folded(&self, ui: &Ui) -> bool {
        ui.ctx()
            .data(|d| d.get_temp::<bool>(egui::Id::new("kfxl-tempo-folded")))
            .unwrap_or(false)
    }

    /// The map as drawn: an empty map shows one ghost point at the song tempo.
    fn shown_points(&self) -> Vec<TempoPoint> {
        if self.song.tempo_map.is_empty() {
            vec![TempoPoint {
                tick: 0,
                bpm: self.song.tempo,
                shape: TempoShape::Step,
            }]
        } else {
            self.song.tempo_map.points.clone()
        }
    }

    pub(super) fn tempo_lane(&mut self, ui: &mut Ui, g: &LaneGeo) {
        let folded = self.tempo_lane_folded(ui);
        let p = ui.painter().clone();
        // Head cell: caption, fold arrow, tempo at the playhead.
        let hp = p.with_clip_rect(g.head);
        hp.line_segment(
            [g.head.left_top(), g.head.right_top()],
            Stroke::new(1.0_f32, t::EDGE),
        );
        let fold = Rect::from_min_size(
            pos2(g.head.left() + 6.0, g.head.top()),
            vec2(14.0, LANE_FOLDED),
        );
        let fr = ui
            .interact(fold, ui.id().with("tempo-fold"), Sense::click())
            .on_hover_text(if folded {
                "Open the tempo lane"
            } else {
                "Fold the tempo lane"
            });
        hp.text(
            fold.center(),
            Align2::CENTER_CENTER,
            if folded { "▸" } else { "▾" },
            t::mono(11.0),
            if fr.hovered() { t::TEXT } else { t::FAINT },
        );
        if fr.clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("kfxl-tempo-folded"), !folded));
        }
        let cap_y = if folded {
            g.head.center().y
        } else {
            g.head.top() + 12.0
        };
        kit::caps(
            &hp,
            pos2(g.head.left() + 22.0, cap_y),
            Align2::LEFT_CENTER,
            "tempo",
            12.0,
            t::FAINT,
        );
        let now = self.tempo_now();
        let value_y = if folded {
            g.head.center().y
        } else {
            g.head.top() + 12.0
        };
        hp.text(
            pos2(g.head.right() - 10.0, value_y),
            Align2::RIGHT_CENTER,
            format!("{now:.1}"),
            t::mono(12.0),
            t::TEXT,
        );

        let lane = g.lane;
        if lane.width() <= 0.0 {
            return;
        }
        let lp = p.with_clip_rect(lane);
        lp.rect_filled(lane, 0.0, t::SUNKEN);
        lp.line_segment(
            [lane.left_top(), lane.right_top()],
            Stroke::new(1.0_f32, t::EDGE),
        );
        let inner = lane.shrink2(vec2(0.0, 4.0));
        // Bar lines run through the lane, as on the grid (time signatures counted).
        for &(bx, strong) in &g.bars {
            lp.vline(
                bx,
                lane.y_range(),
                Stroke::new(
                    1.0_f32,
                    if strong {
                        t::STRONG.gamma_multiply(0.6)
                    } else {
                        t::LINE
                    },
                ),
            );
        }
        // The scale fits the map (a 90→120 change must read at 48 px), rounded
        // to 20 BPM and at least 80 wide; it stays put while a point drags.
        let range_id = ui.id().with("tempo-range");
        let dragging_now = ui
            .data(|d| d.get_temp::<usize>(ui.id().with("tempo-drag")))
            .is_some();
        let (lo, hi) = match ui
            .data(|d| d.get_temp::<(f64, f64)>(range_id))
            .filter(|_| dragging_now)
        {
            Some(r) => r,
            None => {
                let pts = self.shown_points();
                let min = pts.iter().map(|p| p.bpm).fold(f64::MAX, f64::min);
                let max = pts.iter().map(|p| p.bpm).fold(f64::MIN, f64::max);
                let mut lo = ((min - 20.0) / 20.0).floor() * 20.0;
                let mut hi = ((max + 20.0) / 20.0).ceil() * 20.0;
                if hi - lo < 80.0 {
                    let mid = ((lo + hi) / 2.0 / 20.0).round() * 20.0;
                    lo = mid - 40.0;
                    hi = mid + 40.0;
                }
                let lo = lo.max(TEMPO_MIN);
                let hi = hi.min(TEMPO_MAX).max(lo + 40.0);
                ui.data_mut(|d| d.insert_temp(range_id, (lo, hi)));
                (lo, hi)
            }
        };
        let y = move |bpm: f64| {
            inner.bottom() - ((bpm - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * inner.height()
        };
        let bpm_at_y = move |py: f32| {
            (lo + ((inner.bottom() - py) / inner.height()) as f64 * (hi - lo))
                .clamp(TEMPO_MIN, TEMPO_MAX)
        };
        let x = |tk: Tick| g.grid_l + tk as f32 / BAR as f32 * g.bar_w;
        let tick_at = |px: f32| (((px - g.grid_l) / g.bar_w * BAR as f32) as Tick).max(0);
        if !folded {
            // A line every 20 BPM; the scale's ends read in the head cell.
            let mut b = lo + 20.0;
            while b < hi {
                lp.hline(
                    lane.x_range(),
                    y(b),
                    Stroke::new(1.0_f32, t::LINE.gamma_multiply(0.45)),
                );
                b += 20.0;
            }
            hp.text(
                pos2(g.head.left() + 22.0, g.head.bottom() - 9.0),
                Align2::LEFT_CENTER,
                format!("{lo:.0}–{hi:.0}"),
                t::mono(10.0),
                t::FAINT,
            );
        }

        let ghost = self.song.tempo_map.is_empty();
        let pts = self.shown_points();
        // The curve: steps hold then jump, linear segments ramp (with a fill).
        let end_x = lane.right();
        let mut line: Vec<Pos2> = Vec::new();
        for (i, pt) in pts.iter().enumerate() {
            let a = pos2(x(pt.tick), y(pt.bpm));
            line.push(a);
            let next = pts.get(i + 1);
            match (pt.shape, next) {
                (TempoShape::Linear, Some(n)) => {
                    let b = pos2(x(n.tick), y(n.bpm));
                    if !folded {
                        let quad = vec![a, b, pos2(b.x, inner.bottom()), pos2(a.x, inner.bottom())];
                        lp.add(egui::Shape::convex_polygon(
                            quad,
                            t::ENC_BLUE.gamma_multiply(0.12),
                            Stroke::NONE,
                        ));
                    }
                }
                (_, Some(n)) => line.push(pos2(x(n.tick), y(pt.bpm))),
                (_, None) => line.push(pos2(end_x.max(a.x), y(pt.bpm))),
            }
        }
        let col = if ghost {
            t::ENC_BLUE.gamma_multiply(0.45)
        } else {
            t::ENC_BLUE
        };
        lp.add(egui::Shape::line(
            line,
            Stroke::new(if folded { 1.4_f32 } else { 1.6 }, col),
        ));

        let resp = ui.interact(lane, ui.id().with("tempo-lane"), Sense::click_and_drag());
        let drag_id = ui.id().with("tempo-drag");
        let dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
        let hit = |pos: Pos2| -> Option<usize> {
            if ghost {
                return None;
            }
            pts.iter()
                .position(|pt| pos2(x(pt.tick), y(pt.bpm)).distance(pos) < 8.0)
        };
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let hovered = pointer.and_then(hit);
        let r = if folded { 3.0 } else { 4.5 };
        for (i, pt) in pts.iter().enumerate() {
            let c = pos2(x(pt.tick), y(pt.bpm));
            let on = hovered == Some(i) || dragging == Some(i);
            lp.circle_filled(
                c,
                if on { r + 1.5 } else { r },
                if on { t::TEXT } else { col },
            );
            lp.circle_stroke(
                c,
                if on { r + 1.5 } else { r },
                Stroke::new(1.0_f32, t::EDGE),
            );
        }
        let table = TempoTable::build(
            &keyframexl::tempo::TempoMap {
                points: pts.clone(),
            },
            self.song.tempo,
            48_000,
        );
        if let Some(pos) = resp.hover_pos() {
            let tk = tick_at(pos.x);
            let (bpm, at) = match hovered {
                Some(i) => (pts[i].bpm, pts[i].tick),
                None => (table.bpm_at(tk), tk),
            };
            let tip = format!("{bpm:.1} BPM @ {}", song_bar_pos(&self.song.meters, at));
            let anchor = if pos.x > lane.right() - 160.0 {
                Align2::RIGHT_TOP
            } else {
                Align2::LEFT_TOP
            };
            let off = if anchor == Align2::RIGHT_TOP {
                vec2(-10.0, 2.0)
            } else {
                vec2(10.0, 2.0)
            };
            lp.text(
                pos2(pos.x, lane.top()) + off,
                anchor,
                tip,
                t::mono(11.0),
                t::MUTED,
            );
        }
        if folded {
            // Folded: a readout only; open the lane to edit.
            if resp.clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("kfxl-tempo-folded"), false));
            }
            return;
        }

        let alt = ui.input(|i| i.modifiers.alt);
        let meters = self.song.meters.clone();
        let snap = |tk: Tick| song_snap(&meters, tk, g.snap, alt, false).max(0);
        // Drag a point: tick between its neighbours, BPM 40–300. The first stays at 0.
        if resp.drag_started()
            && let Some(i) = resp.interact_pointer_pos().and_then(hit)
        {
            self.checkpoint();
            ui.data_mut(|d| d.insert_temp(drag_id, i));
        }
        if resp.dragged()
            && let (Some(i), Some(pos)) = (
                ui.data(|d| d.get_temp::<usize>(drag_id)),
                resp.interact_pointer_pos(),
            )
        {
            let m = &mut self.song.tempo_map.points;
            if i < m.len() {
                let lo = if i == 0 { 0 } else { m[i - 1].tick + 1 };
                let hi = m.get(i + 1).map(|n| n.tick - 1).unwrap_or(Tick::MAX / 4);
                m[i].tick = if i == 0 {
                    0
                } else {
                    song_snap(&meters, tick_at(pos.x), g.snap, alt, true).clamp(lo, hi.max(lo))
                };
                m[i].bpm = (bpm_at_y(pos.y) * 10.0).round() / 10.0;
                let (bpm, at) = (m[i].bpm, m[i].tick);
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!(
                        "Tempo {bpm:.1} BPM at {}.",
                        song_bar_pos(&self.song.meters, at)
                    ),
                );
            }
        }
        if resp.drag_stopped() && ui.data(|d| d.get_temp::<usize>(drag_id)).is_some() {
            ui.data_mut(|d| d.remove::<usize>(drag_id));
            self.say(Tone::Quiet, "Tempo point moved. Ctrl+Z undoes.");
        }
        let added_id = ui.id().with("tempo-added");
        let now_t = ui.input(|i| i.time);
        // Double-click: toggle the segment under the pointer between jump and ramp.
        // The pair's first click added a point; that edit is rolled back first.
        if resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            if let Some(at) = ui.data(|d| d.get_temp::<f64>(added_id))
                && now_t - at < 1.0
                && let Some(prev) = self.history.pop()
            {
                self.song = prev;
            }
            ui.data_mut(|d| d.remove::<f64>(added_id));
            let tk = tick_at(pos.x);
            let m = &self.song.tempo_map.points;
            match m
                .iter()
                .rposition(|p| p.tick <= tk)
                .filter(|i| i + 1 < m.len())
            {
                Some(i) => {
                    self.checkpoint();
                    let pt = &mut self.song.tempo_map.points[i];
                    pt.shape = if pt.shape == TempoShape::Step {
                        TempoShape::Linear
                    } else {
                        TempoShape::Step
                    };
                    let ramp = pt.shape == TempoShape::Linear;
                    self.say(Tone::Quiet, if ramp { "Tempo ramps to the next point. Double-click again to jump. Ctrl+Z undoes." } else { "Tempo jumps at the next point. Double-click again to ramp. Ctrl+Z undoes." });
                }
                None => self.say(
                    Tone::Quiet,
                    "Double-click between two tempo points to ramp or jump.",
                ),
            }
            self.publish();
        } else if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && hit(pos).is_none()
        {
            // Click empty lane: a step point there at the curve's value.
            let tk = snap(tick_at(pos.x));
            let bpm = (table.bpm_at(tk) * 10.0).round() / 10.0;
            let fresh = self.song.tempo_map.is_empty();
            if fresh || (tk > 0 && !self.song.tempo_map.points.iter().any(|p| p.tick == tk)) {
                self.checkpoint();
                ui.data_mut(|d| d.insert_temp(added_id, now_t));
                let m = &mut self.song.tempo_map.points;
                if fresh {
                    m.push(TempoPoint {
                        tick: 0,
                        bpm: self.song.tempo,
                        shape: TempoShape::Step,
                    });
                }
                if tk > 0 {
                    m.push(TempoPoint {
                        tick: tk,
                        bpm,
                        shape: TempoShape::Step,
                    });
                    m.sort_by_key(|p| p.tick);
                }
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!(
                        "Tempo point at {} ({bpm:.1} BPM). Drag it up or down. Ctrl+Z undoes.",
                        song_bar_pos(&self.song.meters, tk)
                    ),
                );
            }
        }
        // Right-click a point: delete it (the one at bar 1 stays: the song needs a start tempo).
        if resp.secondary_clicked()
            && let Some(i) = resp.interact_pointer_pos().and_then(hit)
        {
            if i == 0 {
                self.say(
                    Tone::Quiet,
                    "The first tempo point stays. Right-click the lane to reset the map.",
                );
            } else {
                self.checkpoint();
                self.song.tempo_map.points.remove(i);
                self.publish();
                self.say(Tone::Quiet, "Tempo point deleted. Ctrl+Z undoes.");
            }
        }
        // Lane menu: reset or scale the whole map (destructive, so it lives here).
        resp.context_menu(|ui| {
            kit::menu_style(ui);
            if hovered.is_some() {
                ui.close_menu();
                return;
            }
            if ui.button("Reset to one tempo").clicked() {
                self.checkpoint();
                self.song.tempo_map.points.clear();
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!(
                        "Tempo map cleared: {:.1} BPM throughout. Ctrl+Z undoes.",
                        self.song.tempo
                    ),
                );
                ui.close_menu();
            }
            for (label, k) in [("Half time (×0.5)", 0.5), ("Double time (×2)", 2.0)] {
                if ui
                    .add_enabled(!self.song.tempo_map.is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    self.checkpoint();
                    for p in &mut self.song.tempo_map.points {
                        p.bpm = (p.bpm * k).clamp(TEMPO_MIN, TEMPO_MAX);
                    }
                    self.publish();
                    self.say(Tone::Quiet, "Tempo map scaled. Ctrl+Z undoes.");
                    ui.close_menu();
                }
            }
        });
    }
}
