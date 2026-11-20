//! Mixer window (FL Mixer, F9): Master and insert strips side by side, the
//! selected track's inspector on the right with its ten effect slots, the
//! selected slot's parameters and the track's sends.
//!
//! Strips are adaptive: tall strips (≥ 300 px) show, top to bottom, number,
//! name, lamp (mute), pan, fader with meter, route to Master; short strips
//! (< 300 px, the mixer docked as a tab) put number + name on one line, lamp
//! and arm next to the pan knob on the next 20 px line, hide SEP (its knob
//! lives in the inspector) and give the fader everything down to the route
//! button. Click a strip to select it. Channels choose their insert
//! in the channel rack (the box left of the name).

use super::{App, Slot, Tab, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::fx::{FxKind, FxSlot};
use keyframexl::sequence::{
    FADER_MAX, FX_SLOTS, Send, gain_db_text, route_would_loop, volume_gain, volume_pos,
};

use keyframexl::fx::{BUILTINS, Builtin};
use std::{collections::BTreeMap, sync::LazyLock};

pub(super) static BUILTIN_GROUPS: LazyLock<BTreeMap<&'static str, Vec<&'static Builtin>>> =
    LazyLock::new(|| {
        let mut groups = BTreeMap::new();
        for effect in &BUILTINS {
            groups
                .entry(effect.category)
                .or_insert_with(Vec::new)
                .push(effect);
        }
        for effects in groups.values_mut() {
            effects.sort_unstable_by_key(|effect| effect.name);
        }
        groups
    });

/// A route cable in progress: the source strip whose route button is dragged.
#[derive(Clone, Copy)]
pub(crate) struct RouteDrag {
    from: usize,
    start: Pos2,
}

/// A route menu (or drop) choice on a strip.
enum RouteEdit {
    /// Route there only, replacing the old destinations.
    Only(u16),
    /// Add a send, keeping the old destinations.
    Add(u16),
    /// Drop every send.
    Clear,
}

const STRIP_W: f32 = 64.0;
const STRIP_GAP: f32 = 4.0;
const INSPECTOR_W: f32 = 320.0;
const DB_MIN: f64 = -60.0;

#[derive(Clone, Default)]
struct MeterState {
    hold_db: Option<f64>,
    hold_until: f64,
    updated_at: f64,
    clipped: bool,
}

/// Fader position in dB from a linear gain (bottom = silence).
fn to_db(gain: f32) -> f64 {
    if gain <= 0.001 {
        DB_MIN
    } else {
        (20.0 * (gain as f64).log10()).max(DB_MIN)
    }
}

impl App {
    /// A dynamics slot's gain reduction for its meter, in dB: the most since
    /// the last frame, falling back at 20 dB/s (`None` without audio).
    fn gain_reduction(&self, ctx: &egui::Context, track: usize, slot: usize) -> Option<f32> {
        let audio = self.audio.as_ref()?;
        let raw = audio
            .reduction
            .get(track * FX_SLOTS + slot)
            .map(|a| f32::from_bits(a.swap(0, std::sync::atomic::Ordering::Relaxed)))
            .unwrap_or(0.0);
        let id = egui::Id::new(("gain-reduction", track, slot));
        let now = ctx.input(|i| i.time);
        let (held, at): (f32, f64) = ctx.data(|d| d.get_temp(id)).unwrap_or((0.0, now));
        let shown = raw.max(held - 20.0 * (now - at) as f32).max(0.0);
        ctx.data_mut(|d| d.insert_temp(id, (shown, now)));
        if shown > 0.0 {
            ctx.request_repaint();
        }
        Some(shown)
    }

    pub(super) fn mixer(&mut self, ui: &mut Ui) {
        let n = self.song.mixer.len();
        if self.mix_sel >= n {
            self.mix_sel = 0;
        }
        let sel = self.mix_sel;
        let sub = format!("{} inserts  ·  selected {}", n - 1, self.mixer_name(sel));
        App::panel_header(ui, &sub, |ui, slot| {
            if slot == Slot::Menu {
                if ui.button("Save track preset…").clicked() {
                    ui.close_menu();
                    self.save_mixer_preset(sel);
                }
                if ui.button("Load track preset…").clicked() {
                    ui.close_menu();
                    self.load_preset_dialog_for(super::presets_ui::Target::Mixer(sel));
                }
                ui.separator();
                if ui.button("Reset selected track").clicked() {
                    self.checkpoint();
                    let name = self.song.mixer[sel].name.clone();
                    self.song.mixer[sel] = keyframexl::sequence::MixerTrack::new(&name);
                    self.publish();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        sel > 0,
                        egui::Button::new("Route selected channels here").shortcut_text("Ctrl+L"),
                    )
                    .clicked()
                {
                    self.route_selected_channels(sel as u16);
                    ui.close_menu();
                }
                let solo = if self.insert_solo(sel) {
                    "Unsolo"
                } else {
                    "Solo selected track"
                };
                if ui
                    .add_enabled(
                        sel > 0,
                        egui::Button::new(solo).shortcut_text("Ctrl+click lamp"),
                    )
                    .clicked()
                {
                    self.solo_insert(sel);
                    ui.close_menu();
                }
                if ui.button("Unmute all tracks").clicked() {
                    self.checkpoint();
                    self.song.mixer.iter_mut().for_each(|m| m.muted = false);
                    self.publish();
                    ui.close_menu();
                }
                let arm = if self.is_armed(sel) {
                    "Disarm"
                } else {
                    "Arm for recording"
                };
                if ui
                    .add(egui::Button::new(arm).shortcut_text("Click ● on strip"))
                    .clicked()
                {
                    self.toggle_arm(sel);
                    ui.close_menu();
                }
                self.window_menu_tail(ui, Tab::Mixer);
            }
        });
        self.read_meters(ui);
        let avail = ui.available_size();
        let strips_w = n as f32 * (STRIP_W + STRIP_GAP) + 42.0;
        let mut action = None;
        // The inspector stays pinned right; strips scroll sideways when the
        // window is narrower than all of them (FL scrolls the strip row too).
        let (body, _) = ui.allocate_exact_size(
            vec2(
                avail.x.max(STRIP_W * 4.0 + INSPECTOR_W + 16.0),
                avail.y.max(200.0),
            ),
            Sense::hover(),
        );
        let p = ui.painter().clone();
        let insp = Rect::from_min_max(pos2(body.right() - INSPECTOR_W, body.top()), body.max);
        let strips = Rect::from_min_max(body.min, pos2(insp.left() - 12.0, body.bottom()));
        let well = kit::sunken(&p, strips, 8.0).shrink(1.0);
        // Strip rects of this frame: drop targets while a route cable is dragged.
        let mut strip_rects: Vec<Rect> = Vec::with_capacity(n);
        ui.scope_builder(egui::UiBuilder::new().max_rect(well), |ui| {
            // The scrollbar stays visible: strips past the edge must look scrollable.
            egui::ScrollArea::horizontal()
                .id_salt("mixer-strips")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    // Room under the strips for the always-visible scrollbar.
                    let (row, _) = ui.allocate_exact_size(
                        vec2(strips_w.max(well.width()), well.height() - 16.0),
                        Sense::hover(),
                    );
                    let p = ui.painter().clone();
                    let (to, from) = routes(&self.song.mixer, sel);
                    for i in 0..n {
                        let x = row.left() + 3.0 + i as f32 * (STRIP_W + STRIP_GAP);
                        let r = Rect::from_min_size(
                            pos2(x, row.top() + 3.0),
                            vec2(STRIP_W, row.height() - 6.0),
                        );
                        self.strip(ui, &p, r, i, &mut action);
                        strip_rects.push(r);
                        if i == 0 {
                            // Master stands apart, like FL's.
                            p.line_segment(
                                [
                                    pos2(r.right() + STRIP_GAP / 2.0, r.top() + 8.0),
                                    pos2(r.right() + STRIP_GAP / 2.0, r.bottom() - 8.0),
                                ],
                                Stroke::new(1.0_f32, t::edge()),
                            );
                        }
                        // Where the selected track goes (blue) and what feeds it (orange), FL's lit routing.
                        // A notch on the top edge points the way: down into a destination, up out of a source.
                        let mark = if to.contains(&i) {
                            Some((t::enc_blue(), true))
                        } else if from.contains(&i) {
                            Some((t::enc_orange(), false))
                        } else {
                            None
                        };
                        if let Some((c, into)) = mark {
                            p.rect_stroke(
                                r.expand(1.0),
                                7.0,
                                Stroke::new(1.5_f32, c),
                                egui::StrokeKind::Outside,
                            );
                            let (cx, y) = (r.center().x, r.top() + 1.0);
                            let tri = if into {
                                vec![pos2(cx - 6.0, y), pos2(cx + 6.0, y), pos2(cx, y + 6.0)]
                            } else {
                                vec![
                                    pos2(cx - 6.0, y + 6.0),
                                    pos2(cx + 6.0, y + 6.0),
                                    pos2(cx, y),
                                ]
                            };
                            p.add(egui::Shape::convex_polygon(tri, c, Stroke::NONE));
                        }
                    }
                    let add = Rect::from_min_size(
                        pos2(
                            row.left() + 3.0 + n as f32 * (STRIP_W + STRIP_GAP),
                            row.top() + 3.0,
                        ),
                        vec2(30.0, 26.0),
                    );
                    let enabled = n < 128 && self.rec.parts.is_empty();
                    let response = ui.interact(
                        add,
                        ui.id().with("mix-add"),
                        if enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    kit::paint_button(
                        ui,
                        add,
                        &response,
                        kit::Kind::Normal,
                        enabled,
                        "Add insert",
                        |p, face, ink| {
                            p.text(
                                face.center(),
                                Align2::CENTER_CENTER,
                                "+",
                                t::sans(16.0),
                                ink,
                            );
                        },
                    );
                    if response.on_hover_text("Add insert").clicked() && enabled {
                        action = Some((true, n));
                    }
                });
        });
        if let Some((insert, at)) = action {
            ui.ctx()
                .data_mut(|d| d.remove::<Vec<MeterState>>(egui::Id::new("mixer-meter-state")));
            self.meters.clear();
            self.checkpoint();
            if insert {
                let at = self.song.insert_track(at);
                for i in &mut self.rec.armed {
                    if *i >= at {
                        *i += 1;
                    }
                }
                for (i, _) in &mut self.fx_windows {
                    if *i >= at {
                        *i += 1;
                    }
                }
                self.mix_solo = self.mix_solo.map(|i| if i >= at { i + 1 } else { i });
                self.mix_sel = at;
            } else {
                self.song.remove_track(at);
                self.rec.armed.retain(|i| *i != at);
                for i in &mut self.rec.armed {
                    if *i > at {
                        *i -= 1;
                    }
                }
                self.fx_windows.retain(|(i, _)| *i != at);
                for (i, _) in &mut self.fx_windows {
                    if *i > at {
                        *i -= 1;
                    }
                }
                self.mix_solo = self.mix_solo.and_then(|i| {
                    if i == at {
                        None
                    } else {
                        Some(if i > at { i - 1 } else { i })
                    }
                });
                self.mix_sel = at.min(self.song.mixer.len() - 1);
            }
            self.mix_slot = 0;
            self.publish();
        }
        // A route cable in progress (FL: drag the "→ M" button onto a strip).
        // The cable follows the pointer in the routing colour; the strip under
        // it lights blue when the drop is valid, red when it would loop or is itself.
        if let Some(drag) = self.route_drag {
            let ptr = ui.input(|i| i.pointer.hover_pos());
            let released = ui.input(|i| i.pointer.any_released());
            let down = ui.input(|i| i.pointer.any_down());
            let ctrl = ui.input(|i| i.modifiers.command);
            let target = ptr.and_then(|q| strip_rects.iter().position(|r| r.contains(q)));
            match target {
                Some(t) if t != drag.from && !route_would_loop(&self.song.mixer, drag.from, t) => {
                    p.rect_stroke(
                        strip_rects[t].expand(1.0),
                        7.0,
                        Stroke::new(2.0_f32, t::enc_blue()),
                        egui::StrokeKind::Outside,
                    );
                }
                Some(t) => {
                    p.rect_stroke(
                        strip_rects[t].expand(1.0),
                        7.0,
                        Stroke::new(2.0_f32, t::bad()),
                        egui::StrokeKind::Outside,
                    );
                }
                None => {}
            }
            if let Some(q) = ptr {
                p.line_segment([drag.start, q], Stroke::new(2.0_f32, t::enc_blue()));
                p.circle_filled(q, 3.0, t::enc_blue());
            }
            if released {
                self.route_drag = None;
                if let Some(t) = target {
                    self.apply_route_edit(
                        drag.from,
                        if ctrl {
                            RouteEdit::Add(t as u16)
                        } else {
                            RouteEdit::Only(t as u16)
                        },
                    );
                }
            } else if !down {
                self.route_drag = None;
            }
        }
        self.mix_strip_rects = strip_rects;
        self.inspector(ui, &p, insp);
    }

    /// Inserts that stay audible when `i` is soloed: itself and the inserts it
    /// sends to or receives from (FL keeps the routing chain alive).
    fn solo_keeps(&self, i: usize, j: usize) -> bool {
        let sends = |a: usize, b: usize| {
            self.song
                .mixer
                .get(a)
                .is_some_and(|m| m.sends.iter().any(|s| s.to as usize == b))
        };
        j == 0 || j == i || sends(i, j) || sends(j, i)
    }

    /// `i` is the soloed insert: it was soloed last, plays, and every insert
    /// outside its chain is still muted.
    pub(crate) fn insert_solo(&self, i: usize) -> bool {
        let n = self.song.mixer.len();
        i > 0
            && self.mix_solo == Some(i)
            && !self.song.mixer[i].muted
            && (1..n).any(|j| !self.solo_keeps(i, j))
            && (1..n).all(|j| self.solo_keeps(i, j) || self.song.mixer[j].muted)
    }

    /// FL solo: mute every other insert (keeping `i`'s send chain); again to unmute all.
    pub(crate) fn solo_insert(&mut self, i: usize) {
        self.checkpoint();
        if self.insert_solo(i) {
            self.song
                .mixer
                .iter_mut()
                .skip(1)
                .for_each(|m| m.muted = false);
            self.say(Tone::Quiet, "All inserts play again. Ctrl+Z undoes.");
        } else {
            for j in 1..self.song.mixer.len() {
                let keep = self.solo_keeps(i, j);
                self.song.mixer[j].muted = !keep;
            }
            self.mix_solo = Some(i);
            self.say(
                Tone::Quiet,
                format!(
                    "{} soloed. Ctrl+click its lamp again to unsolo.",
                    self.mixer_name(i)
                ),
            );
        }
        self.publish();
    }

    /// The strip's built-in 3-band EQ (after the effect slots).
    fn strip_eq(&mut self, ui: &mut Ui, track: usize) {
        kit::caps(
            ui.painter(),
            ui.cursor().min + vec2(0.0, 6.0),
            Align2::LEFT_CENTER,
            "strip eq",
            11.0,
            t::faint(),
        );
        ui.add_space(14.0);
        let fmt = |x: f64| {
            if x.abs() < 0.05 {
                "0 dB".to_string()
            } else {
                format!("{x:+.1} dB")
            }
        };
        let r = keyframexl::sequence::EQ_RANGE as f64;
        ui.horizontal_wrapped(|ui| {
            for (b, (label, color)) in [
                ("Low", t::enc_orange()),
                ("Mid", t::enc_green()),
                ("High", t::enc_blue()),
            ]
            .into_iter()
            .enumerate()
            {
                let hz = keyframexl::sequence::EQ_FREQS[b];
                let hint = format!("{label} band at {} Hz. Double-click for 0 dB.", hz as u32);
                let knob = kit::Knob {
                    label,
                    color,
                    min: -r,
                    max: r,
                    default: 0.0,
                    log: false,
                    fmt: &fmt,
                    hint: &hint,
                };
                let mut v = self.song.mixer[track].eq[b] as f64;
                let (_, started, changed) = kit::knob(ui, &knob, &mut v);
                if started {
                    self.checkpoint();
                }
                if changed {
                    self.song.mixer[track].eq[b] = (v as f32).clamp(
                        -keyframexl::sequence::EQ_RANGE,
                        keyframexl::sequence::EQ_RANGE,
                    );
                    self.publish();
                }
            }
        });
    }

    pub(crate) fn mixer_name(&self, i: usize) -> String {
        match self.song.mixer.get(i) {
            Some(m) if !m.name.is_empty() => m.name.clone(),
            _ if i == 0 => "Master".into(),
            _ => format!("Insert {i}"),
        }
    }

    /// Whether insert `i` carries a real name, not its default (stored names
    /// from FL imports and older files may spell the English default).
    pub(crate) fn has_custom_mixer_name(&self, i: usize) -> bool {
        self.song.mixer.get(i).is_some_and(|m| {
            !m.name.is_empty() && m.name != format!("Insert {i}") && !(i == 0 && m.name == "Master")
        })
    }

    /// FL Ctrl+L: route the rack's selected channels to insert `to`.
    pub(crate) fn route_selected_channels(&mut self, to: u16) {
        let sel = self.rack_sel.clone();
        if sel.is_empty() {
            self.say(
                Tone::Quiet,
                "Select channels in the channel rack first (the bar at each row's right end).",
            );
            return;
        }
        self.checkpoint();
        for i in self
            .song
            .instruments
            .iter_mut()
            .filter(|i| sel.contains(&i.id))
        {
            i.mixer = to;
        }
        self.publish();
        let name = self.mixer_name(to as usize);
        self.say(
            Tone::Quiet,
            format!(
                "{} channel(s) now play through {name}. Ctrl+Z undoes.",
                sel.len()
            ),
        );
    }

    /// First insert no channel uses (FL "next free mixer track").
    pub(crate) fn free_insert(&self) -> Option<u16> {
        (1..self.song.mixer.len() as u16)
            .find(|m| !self.song.instruments.iter().any(|i| i.mixer == *m))
    }

    /// Pull peaks from the audio thread; the meters fall back at ~20 dB/s.
    fn read_meters(&mut self, ui: &Ui) {
        let n = self.song.mixer.len();
        self.meters.resize(n, 0.0);
        let id = egui::Id::new("mixer-meter-state");
        let mut states: Vec<MeterState> = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
        states.resize(n, MeterState::default());
        let time = ui.input(|i| i.time);
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.meter_at).as_secs_f32().min(0.2);
        self.meter_at = now;
        let fall = 10f32.powf(-20.0 * dt / 20.0);
        for (i, m) in self.meters.iter_mut().enumerate() {
            let peak = self
                .audio
                .as_ref()
                .and_then(|a| a.peaks.get(i))
                .map(|a| f32::from_bits(a.swap(0, std::sync::atomic::Ordering::Relaxed)))
                .unwrap_or(0.0);
            *m = (*m * fall).max(peak);
            if *m <= 0.001 {
                *m = 0.0;
            }
            let state = &mut states[i];
            state.clipped |= peak >= 1.0;
            let dt = (time - state.updated_at.max(state.hold_until)).max(0.0);
            if let Some(db) = state.hold_db {
                let db = db - 20.0 * dt;
                state.hold_db = (db > DB_MIN).then_some(db);
            }
            if peak > 0.001 {
                let db = to_db(peak);
                if state.hold_db.is_none_or(|held| db >= held) {
                    state.hold_db = Some(db);
                    state.hold_until = time + 1.5;
                }
            }
            state.updated_at = time;
            if *m > 0.0 || state.hold_db.is_some() {
                ui.ctx().request_repaint();
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, states));
    }

    fn strip(
        &mut self,
        ui: &mut Ui,
        p: &egui::Painter,
        r: Rect,
        i: usize,
        action: &mut Option<(bool, usize)>,
    ) {
        let track = self.song.mixer[i].clone();
        let selected = self.mix_sel == i;
        let sel_before = self.mix_sel;
        let resp = ui.interact(r, ui.id().with(("strip", i)), Sense::click());
        let hover = kit::hover_amount(ui, &resp);
        let top = if selected {
            t::ctl_hover()
        } else {
            kit::mix(t::panel_top(), t::ctl_top(), hover * 0.5)
        };
        kit::raised(
            p,
            r,
            6.0,
            top,
            t::panel_bottom(),
            if selected { t::text() } else { t::edge() },
            false,
            1.0,
        );
        if resp.clicked() {
            self.mix_sel = i;
        }
        resp.context_menu(|ui| {
            kit::menu_style(ui);
            let can_edit = self.rec.parts.is_empty();
            if ui
                .add_enabled(
                    can_edit && i > 0 && self.song.mixer.len() < 128,
                    egui::Button::new("Insert track before"),
                )
                .clicked()
            {
                *action = Some((true, i));
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    can_edit && self.song.mixer.len() < 128,
                    egui::Button::new("Insert track after"),
                )
                .clicked()
            {
                *action = Some((true, i + 1));
                ui.close_menu();
            }
            if ui
                .add_enabled(can_edit && i > 0, egui::Button::new("Delete track"))
                .clicked()
            {
                *action = Some((false, i));
                ui.close_menu();
            }
        });
        let cx = r.center().x;
        // Below 300 px the strip collapses so the fader keeps its travel:
        // number + name share the top line, lamp + arm sit next to the pan
        // knob, SEP hides (the inspector keeps its knob), and the dB readout
        // rides inside the fader's bottom edge. Tall strips keep today's layout.
        let short = r.height() < 300.0;
        let label = if i == 0 {
            "M".to_string()
        } else {
            format!("{i}")
        };
        let route_h = if i == 0 { 0.0 } else { 26.0 };
        let (lamp_r, arm_r, pan_r, sep_r, fr, db_below): (
            Rect,
            Rect,
            Rect,
            Option<Rect>,
            Rect,
            bool,
        );
        if short {
            p.text(
                pos2(r.left() + 6.0, r.top() + 11.0),
                Align2::LEFT_CENTER,
                &label,
                t::mono(12.0),
                if i == 0 { t::text() } else { t::muted() },
            );
            // A default name repeats the number, so only a real name is printed ("Inse…" says nothing).
            let full = self.mixer_name(i);
            let named = self.has_custom_mixer_name(i);
            if named {
                let name = kit::elide(p, &full, t::sans(12.0), r.width() - 28.0);
                p.text(
                    pos2(r.left() + 22.0, r.top() + 11.0),
                    Align2::LEFT_CENTER,
                    name,
                    t::sans(12.0),
                    t::text(),
                );
            } else if i == 0 {
                p.text(
                    pos2(r.left() + 22.0, r.top() + 11.0),
                    Align2::LEFT_CENTER,
                    "Master",
                    t::sans(12.0),
                    t::text(),
                );
            }
            lamp_r =
                Rect::from_center_size(pos2(r.left() + 11.0, r.top() + 31.0), vec2(18.0, 18.0));
            arm_r = Rect::from_center_size(pos2(r.left() + 28.0, r.top() + 31.0), vec2(16.0, 16.0));
            pan_r = Rect::from_center_size(pos2(r.left() + 49.0, r.top() + 31.0), vec2(20.0, 20.0));
            sep_r = None;
            let fbot = if i == 0 {
                r.bottom() - 6.0
            } else {
                r.bottom() - 28.0
            };
            fr = Rect::from_min_max(pos2(r.left() + 8.0, r.top() + 44.0), pos2(cx + 6.0, fbot));
            db_below = false;
        } else {
            p.text(
                pos2(cx, r.top() + 12.0),
                Align2::CENTER_CENTER,
                label,
                t::mono(12.0),
                if i == 0 { t::text() } else { t::muted() },
            );
            arm_r =
                Rect::from_center_size(pos2(r.right() - 12.0, r.top() + 12.0), vec2(16.0, 16.0));
            let name = kit::elide(p, &self.mixer_name(i), t::sans(12.0), r.width() - 8.0);
            p.text(
                pos2(cx, r.top() + 28.0),
                Align2::CENTER_CENTER,
                name,
                t::sans(12.0),
                t::text(),
            );
            lamp_r = Rect::from_center_size(pos2(cx, r.top() + 48.0), vec2(18.0, 18.0));
            pan_r = Rect::from_center_size(pos2(cx - 13.0, r.top() + 74.0), vec2(20.0, 20.0));
            sep_r = Some(Rect::from_center_size(
                pos2(cx + 13.0, r.top() + 74.0),
                vec2(20.0, 20.0),
            ));
            fr = Rect::from_min_max(
                pos2(r.left() + 8.0, r.top() + 94.0),
                pos2(cx + 6.0, r.bottom() - 30.0 - route_h),
            );
            db_below = true;
        }
        // Arm (FL: the disk-record button at the strip top).
        let ar = ui.interact(arm_r, ui.id().with(("mix-arm", i)), Sense::click());
        let armed = self.is_armed(i);
        let recording = armed && self.rec.parts.iter().any(|p| p.track == i);
        let dot = if recording {
            t::bad()
        } else if armed {
            t::brand()
        } else if ar.hovered() {
            t::muted()
        } else {
            t::line()
        };
        p.circle_filled(arm_r.center(), 4.0, dot);
        let tip = if armed {
            "Armed: records while Record is on and playing. Click to disarm."
        } else {
            "Arm: record this track to WAV"
        };
        if ar.on_hover_text(tip).clicked() {
            self.toggle_arm(i);
        }
        // Lamp: lit while the track plays.
        let lr = ui.interact(lamp_r, ui.id().with(("mix-mute", i)), Sense::click());
        let solo = self.insert_solo(i);
        kit::led(
            p,
            lamp_r.center(),
            4.5,
            (!track.muted).then_some(if solo { t::enc_blue() } else { t::ok() }),
        );
        let ctrl = ui.input(|inp| inp.modifiers.command);
        let tip = if i == 0 {
            if track.muted {
                "Muted: click to unmute"
            } else {
                "Click to mute"
            }
        } else if solo {
            "Solo: Ctrl+click or right-click to unsolo"
        } else if track.muted {
            "Muted: click to unmute · Ctrl+click or right-click to solo"
        } else {
            "Click to mute · Ctrl+click or right-click to solo"
        };
        let lr = lr.on_hover_text(tip);
        if i > 0 && (lr.secondary_clicked() || (lr.clicked() && ctrl)) {
            self.solo_insert(i);
        } else if lr.clicked() {
            self.checkpoint();
            self.song.mixer[i].muted = !track.muted;
            self.publish();
        }
        // Pan and stereo separation, side by side (FL: pan · sep). Short
        // strips hide SEP; the pan tooltip says where it went.
        if let Some(sep_r) = sep_r {
            let mut sep = track.sep as f64;
            let sep_fmt = |v: f64| {
                if v.abs() < 0.005 {
                    "0".to_string()
                } else if v < 0.0 {
                    format!("{:.0}% narrower", -v * 100.0)
                } else {
                    format!("{:.0}% wider", v * 100.0)
                }
            };
            let spec = kit::Knob {
                label: "Sep",
                color: t::enc_green(),
                min: -1.0,
                max: 1.0,
                default: 0.0,
                log: false,
                fmt: &sep_fmt,
                hint: "Stereo separation: left merges to mono, right widens. Double-click resets.",
            };
            let (_, started, changed) =
                kit::mini_knob(ui, sep_r, ui.id().with(("mix-sep", i)), &spec, &mut sep);
            if started {
                self.checkpoint();
            }
            if changed {
                self.song.mixer[i].sep = sep as f32;
                self.publish();
            }
        }
        let mut pan = track.pan as f64;
        let fmt = |v: f64| {
            if v.abs() < 0.005 {
                "C".to_string()
            } else if v < 0.0 {
                format!("L{:.0}", -v * 100.0)
            } else {
                format!("R{:.0}", v * 100.0)
            }
        };
        let sep_now = if track.sep.abs() < 0.005 {
            "0".to_string()
        } else if track.sep < 0.0 {
            format!("{:.0}% narrower", -track.sep * 100.0)
        } else {
            format!("{:.0}% wider", track.sep * 100.0)
        };
        let pan_hint = if short {
            format!("Track pan. Stereo separation {sep_now} (SEP knob in the inspector)")
        } else {
            "Track pan".to_string()
        };
        let spec = kit::Knob {
            label: "Pan",
            color: t::enc_white(),
            min: -1.0,
            max: 1.0,
            default: 0.0,
            log: false,
            fmt: &fmt,
            hint: pan_hint.as_str(),
        };
        kit::automatable(
            ui,
            keyframexl::automation::Target::MixerPan { track: i as u16 },
        );
        let (_, started, changed) =
            kit::mini_knob(ui, pan_r, ui.id().with(("mix-pan", i)), &spec, &mut pan);
        if started {
            self.checkpoint();
        }
        if changed {
            let v = pan as f32;
            self.song.mixer[i].pan = v;
            self.publish_param(
                keyframexl::automation::Target::MixerPan { track: i as u16 },
                v,
            );
        }
        // Fader and meter.
        // The fader travels along the shared volume taper; `v` is its position.
        let mut v = volume_pos(track.volume) as f64;
        let max_db = 20.0 * (FADER_MAX as f64).log10();
        let fmt = |pos: f64| gain_db_text(volume_gain(pos as f32));
        let spec = kit::Knob {
            label: "Volume",
            color: t::enc_orange(),
            min: 0.0,
            max: 1.0,
            default: volume_pos(1.0) as f64,
            log: false,
            fmt: &fmt,
            hint: "Track volume. Double-click for 0 dB.",
        };
        kit::automatable(
            ui,
            keyframexl::automation::Target::MixerVolume { track: i as u16 },
        );
        let (_, started, changed) =
            kit::fader(ui, fr, ui.id().with(("mix-fader", i)), &spec, &mut v);
        if started {
            self.checkpoint();
        }
        if changed {
            let gain = volume_gain(v as f32);
            self.song.mixer[i].volume = gain;
            self.publish_param(
                keyframexl::automation::Target::MixerVolume { track: i as u16 },
                gain,
            );
        }
        let meter = Rect::from_min_max(
            pos2(cx + 11.0, fr.top() + 8.0),
            pos2(cx + 21.0, fr.bottom() - 8.0),
        );
        kit::sunken(p, meter.expand(1.0), 2.0);
        let level = self.meters.get(i).copied().unwrap_or(0.0);
        // Same scale as the fader beside it, so 0 dB lines up with the unity mark.
        let y_for = |db: f64| {
            meter.bottom()
                - if db <= DB_MIN {
                    0.0
                } else {
                    volume_pos(10f64.powf(db / 20.0) as f32)
                } * meter.height()
        };
        if level > 0.0 {
            let db = to_db(level);
            for (low, high, color) in [
                (DB_MIN, -6.0, t::ok()),
                (-6.0, -1.0, t::wait()),
                (-1.0, max_db, t::bad()),
            ] {
                if db > low {
                    p.rect_filled(
                        Rect::from_min_max(
                            pos2(meter.left(), y_for(db.min(high))),
                            pos2(meter.right(), y_for(low)),
                        ),
                        0.0,
                        color,
                    );
                }
            }
        }
        let state_id = egui::Id::new("mixer-meter-state");
        let states: Vec<MeterState> = ui.ctx().data(|d| d.get_temp(state_id)).unwrap_or_default();
        let state = states.get(i).cloned().unwrap_or_default();
        if let Some(db) = state.hold_db {
            let y = y_for(db).clamp(meter.top() + 1.0, meter.bottom() - 1.0);
            p.line_segment(
                [pos2(meter.left(), y), pos2(meter.right(), y)],
                Stroke::new(2.0_f32, t::text()),
            );
        }
        let clipped = state.clipped;
        let clip_r = Rect::from_center_size(pos2(meter.center().x, fr.top() + 3.0), vec2(8.0, 6.0));
        let cr = ui.interact(clip_r, ui.id().with(("mix-clipbtn", i)), Sense::click());
        if clipped {
            p.rect_filled(clip_r, 1.0, t::bad());
        } else {
            p.rect_filled(clip_r, 1.0, t::lcd_off());
        }
        let clip_tip = "Clip indicator: lights at 0 dBFS; click to reset";
        if cr.on_hover_text(clip_tip).clicked() {
            let mut states = states;
            if let Some(state) = states.get_mut(i) {
                state.clipped = false;
            }
            ui.ctx().data_mut(|d| d.insert_temp(state_id, states));
        }
        if db_below {
            p.text(
                pos2(cx, r.bottom() - 18.0 - route_h),
                Align2::CENTER_CENTER,
                fmt(v).replace(" dB", ""),
                t::mono(12.0),
                t::muted(),
            );
        } else {
            // Short strips: the readout rides inside the fader's bottom edge
            // (the fader tooltip shows it on hover too).
            let shown = fmt(v).replace(" dB", "");
            let chip = Rect::from_center_size(
                pos2(fr.center().x, fr.bottom() - 9.0),
                vec2(kit::text_width(p, &shown, t::mono(12.0)) + 10.0, 15.0),
            );
            p.rect_filled(chip, 3.0, t::sunken());
            p.text(
                chip.center(),
                Align2::CENTER_CENTER,
                shown,
                t::mono(12.0),
                t::muted(),
            );
        }
        // Route to Master (inserts). Click toggles the Master route; drag the
        // button onto another strip to route there (Ctrl adds a send instead);
        // Ctrl+click sends the selected strip there; right-click lists every insert.
        if i > 0 {
            let rr =
                Rect::from_center_size(pos2(cx, r.bottom() - 16.0), vec2(STRIP_W - 16.0, 20.0));
            let resp = ui.interact(rr, ui.id().with(("mix-route", i)), Sense::click_and_drag());
            let on = track.to_master;
            let face = if on {
                kit::control(p, rr, 4.0, kit::hover_amount(ui, &resp), false)
            } else {
                kit::sunken(p, rr, 4.0)
            };
            p.text(
                face.center(),
                Align2::CENTER_CENTER,
                "→ M",
                t::mono(12.0),
                if on { t::text() } else { t::faint() },
            );
            let mut edit = None;
            resp.context_menu(|ui| {
                kit::menu_style(ui);
                let m = &self.song.mixer[i];
                let to_master = m.to_master;
                let has: Vec<bool> = (0..self.song.mixer.len())
                    .map(|j| m.sends.iter().any(|s| s.to as usize == j))
                    .collect();
                for j in 1..self.song.mixer.len() {
                    if j == i {
                        ui.add_enabled(false, egui::Button::new(self.mixer_name(j)));
                        continue;
                    }
                    if ui.selectable_label(has[j], self.mixer_name(j)).clicked() {
                        edit = Some(RouteEdit::Only(j as u16));
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.menu_button("Add send to", |ui| {
                    kit::menu_style(ui);
                    for j in 1..self.song.mixer.len() {
                        if j == i {
                            continue;
                        }
                        let dup = has[j];
                        let loops = route_would_loop(&self.song.mixer, i, j);
                        let tip = if dup {
                            "Already sends here"
                        } else if loops {
                            "That would loop"
                        } else {
                            "Add a send at 100%"
                        };
                        if ui
                            .add_enabled(!dup && !loops, egui::Button::new(self.mixer_name(j)))
                            .on_hover_text(tip)
                            .clicked()
                        {
                            edit = Some(RouteEdit::Add(j as u16));
                            ui.close_menu();
                        }
                    }
                });
                if ui
                    .add_enabled(!m.sends.is_empty(), egui::Button::new("Remove all sends"))
                    .clicked()
                {
                    edit = Some(RouteEdit::Clear);
                    ui.close_menu();
                }
                ui.separator();
                if ui.selectable_label(to_master, "Route to Master").clicked() {
                    edit = Some(RouteEdit::Only(0));
                    ui.close_menu();
                }
            });
            if let Some(e) = edit {
                self.apply_route_edit(i, e);
            }
            if resp.drag_started() {
                self.route_drag = Some(RouteDrag {
                    from: i,
                    start: rr.center(),
                });
            }
            let ctrl = ui.input(|i| i.modifiers.command);
            if resp.clicked() && self.route_drag.is_none() {
                if ctrl && sel_before != i {
                    self.mix_sel = sel_before;
                    if sel_before == 0 {
                        self.say(
                            Tone::Quiet,
                            "Master has no output to send: select an insert first.",
                        );
                    } else {
                        self.apply_route_edit(sel_before, RouteEdit::Add(i as u16));
                    }
                } else {
                    self.checkpoint();
                    self.song.mixer[i].to_master = !on;
                    self.publish();
                    let name = self.mixer_name(i);
                    self.say(
                        Tone::Quiet,
                        if on {
                            format!("{name} no longer feeds Master. Ctrl+Z undoes.")
                        } else {
                            format!("{name} feeds Master again. Ctrl+Z undoes.")
                        },
                    );
                }
            }
        }
    }

    /// One route change from a menu, a Ctrl+click or a drop: one undo step.
    fn apply_route_edit(&mut self, from: usize, edit: RouteEdit) {
        let name = self.mixer_name(from);
        let dest = match edit {
            RouteEdit::Only(to) | RouteEdit::Add(to) => self.mixer_name(to as usize),
            RouteEdit::Clear => String::new(),
        };
        match edit {
            RouteEdit::Only(to) => {
                if to as usize == from {
                    self.say(Tone::Quiet, "Not routed: a track can't feed itself.");
                    return;
                }
                if route_would_loop(&self.song.mixer, from, to as usize) {
                    self.say(
                        Tone::Quiet,
                        format!("Not routed: {name} → {dest} would loop."),
                    );
                    return;
                }
                self.checkpoint();
                self.song.mixer[from].route_only(to);
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!("{name} now routes to {dest} only. Ctrl+Z undoes."),
                );
            }
            RouteEdit::Add(to) => {
                if to as usize == from {
                    self.say(Tone::Quiet, "Not routed: a track can't feed itself.");
                    return;
                }
                if route_would_loop(&self.song.mixer, from, to as usize) {
                    self.say(
                        Tone::Quiet,
                        format!("Not routed: {name} → {dest} would loop."),
                    );
                    return;
                }
                if to == 0 {
                    if self.song.mixer[from].to_master {
                        self.say(Tone::Quiet, format!("{name} already routes to Master."));
                    } else {
                        self.checkpoint();
                        self.song.mixer[from].to_master = true;
                        self.publish();
                        self.say(
                            Tone::Quiet,
                            format!("{name} feeds Master again. Ctrl+Z undoes."),
                        );
                    }
                    return;
                }
                if let Some(send) = self.song.mixer[from].sends.iter().find(|s| s.to == to) {
                    let lv = send.level;
                    self.say(
                        Tone::Quiet,
                        format!("{name} already sends to {dest} ({:.0}%).", lv * 100.0),
                    );
                    return;
                }
                self.checkpoint();
                self.song.mixer[from].add_send(to);
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!("{name} now sends to {dest} at 100%. Ctrl+Z undoes."),
                );
            }
            RouteEdit::Clear => {
                if self.song.mixer[from].sends.is_empty() {
                    self.say(Tone::Quiet, format!("{name} has no sends."));
                } else {
                    self.checkpoint();
                    self.song.mixer[from].clear_sends();
                    self.publish();
                    self.say(
                        Tone::Quiet,
                        format!("{name}: all sends removed. Ctrl+Z undoes."),
                    );
                }
            }
        }
    }

    fn inspector(&mut self, ui: &mut Ui, p: &egui::Painter, r: Rect) {
        let i = self.mix_sel;
        kit::raised(
            p,
            r,
            8.0,
            t::panel_top(),
            t::panel_bottom(),
            t::edge(),
            false,
            1.0,
        );
        let inner = r.shrink(12.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::LEFT)), |ui| {
          egui::ScrollArea::vertical().id_salt("mixer-inspector").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            // Name, editable in place (one undo step).
            let mut name = self.mixer_name(i);
            let te = ui.add(egui::TextEdit::singleline(&mut name).font(t::semibold(16.0)).frame(false).desired_width(inner.width()));
            if te.changed() {
                if kit::first_edit(ui, &te) {
                    self.checkpoint();
                }
                self.song.mixer[i].name = name;
                self.publish();
            }
            kit::end_edit(ui, &te);
            if i > 0 {
                let m = &self.song.mixer[i];
                let mut dest: Vec<String> = Vec::new();
                if m.to_master {
                    dest.push("Master".into());
                }
                for snd in &m.sends {
                    dest.push(format!("{} ({:.0}%)", self.mixer_name(snd.to as usize), snd.level * 100.0));
                }
                let to = if dest.is_empty() { "nowhere: not routed, silent".to_string() } else { dest.join(", ") };
                ui.label(egui::RichText::new(format!("To: {to}")).font(t::sans(12.0)).color(if dest.is_empty() { t::wait() } else { t::faint() }));
                // Sidechain links both ways: what ducks this insert, what it ducks.
                let keyed_by: Vec<String> = m.fx.iter().filter_map(|f| f.sidechain).map(|k| self.mixer_name(k as usize)).collect();
                let keys: Vec<String> = (1..self.song.mixer.len())
                    .filter(|&j| j != i && self.song.mixer[j].fx.iter().any(|f| f.sidechain == Some(i as u16)))
                    .map(|j| self.mixer_name(j))
                    .collect();
                if !keyed_by.is_empty() {
                    ui.label(egui::RichText::new(format!("Ducks under: {}", keyed_by.join(", "))).font(t::sans(12.0)).color(t::faint()));
                }
                if !keys.is_empty() {
                    ui.label(egui::RichText::new(format!("Sidechains: {}", keys.join(", "))).font(t::sans(12.0)).color(t::faint()));
                }
            }
            ui.add_space(6.0);
            // Pan + stereo separation: short strips hide SEP, so it always lives here too.
            ui.horizontal(|ui| {
                let mut pan = self.song.mixer[i].pan as f64;
                let pan_fmt = |v: f64| if v.abs() < 0.005 { "C".to_string() } else if v < 0.0 { format!("L{:.0}", -v * 100.0) } else { format!("R{:.0}", v * 100.0) };
                let pan_knob = kit::Knob { label: "Pan", color: t::enc_white(), min: -1.0, max: 1.0, default: 0.0, log: false, fmt: &pan_fmt, hint: "Track pan" };
                kit::automatable(ui, keyframexl::automation::Target::MixerPan { track: i as u16 });
                let (_, started, changed) = kit::knob(ui, &pan_knob, &mut pan);
                if started {
                    self.checkpoint();
                }
                if changed {
                    let v = pan as f32;
                    self.song.mixer[i].pan = v;
                    self.publish_param(
                        keyframexl::automation::Target::MixerPan { track: i as u16 },
                        v,
                    );
                }
                let mut sep = self.song.mixer[i].sep as f64;
                let sep_fmt = |v: f64| if v.abs() < 0.005 { "0".to_string() } else if v < 0.0 { format!("{:.0}% narrower", -v * 100.0) } else { format!("{:.0}% wider", v * 100.0) };
                let sep_knob = kit::Knob { label: "Sep", color: t::enc_green(), min: -1.0, max: 1.0, default: 0.0, log: false, fmt: &sep_fmt, hint: "Stereo separation: left merges to mono, right widens. Double-click resets." };
                let (_, started, changed) = kit::knob(ui, &sep_knob, &mut sep);
                if started {
                    self.checkpoint();
                }
                if changed {
                    self.song.mixer[i].sep = sep as f32;
                    self.publish();
                }
            });
            ui.add_space(6.0);
            kit::caps(ui.painter(), ui.cursor().min + vec2(0.0, 6.0), Align2::LEFT_CENTER, "effects", 11.0, t::faint());
            ui.add_space(14.0);
            // The slot list scrolls on its own when the inspector is short, so
            // all ten slots stay reachable at dock height.
            let slots_max = (r.height() - 300.0).clamp(72.0, FX_SLOTS as f32 * 28.0);
            egui::ScrollArea::vertical().id_salt("mixer-fx-slots").max_height(slots_max).auto_shrink([false, false]).show(ui, |ui| {
                for k in 0..FX_SLOTS {
                    self.fx_slot_row(ui, i, k);
                }
            });
            ui.add_space(8.0);
            self.strip_eq(ui, i);
            if i > 0 {
                ui.add_space(8.0);
                self.sends_ui(ui, i);
            }
          });
        });
    }

    fn fx_slot_row(&mut self, ui: &mut Ui, track: usize, k: usize) {
        let slot = self.song.mixer[track].fx.get(k).cloned();
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
        let p = ui.painter().clone();
        let selected = self.mix_slot == k;
        let resp = ui.interact(row, ui.id().with(("slot", track, k)), Sense::click());
        if selected {
            kit::thumb(&p, row);
        } else if resp.hovered() {
            p.rect_filled(row, 4.0, t::raised().gamma_multiply(0.6));
        }
        p.text(
            pos2(row.left() + 6.0, row.center().y),
            Align2::LEFT_CENTER,
            format!("{}", k + 1),
            t::mono(12.0),
            t::faint(),
        );
        match &slot {
            Some(s) => {
                let lamp = Rect::from_center_size(
                    pos2(row.left() + 26.0, row.center().y),
                    vec2(16.0, 16.0),
                );
                let lr = ui.interact(lamp, ui.id().with(("slot-on", track, k)), Sense::click());
                kit::led(&p, lamp.center(), 3.5, s.enabled.then_some(t::ok()));
                if lr
                    .on_hover_text(if s.enabled {
                        "On: click to bypass"
                    } else {
                        "Bypassed: click to turn on"
                    })
                    .clicked()
                {
                    self.checkpoint();
                    self.song.mixer[track].fx[k].enabled = !s.enabled;
                    self.publish();
                }
                // A slot listening to another insert says which (FL: the key icon).
                let key_w = match s.sidechain {
                    Some(i) => {
                        let r = p.text(
                            pos2(row.right() - 30.0, row.center().y),
                            Align2::RIGHT_CENTER,
                            format!("SC {i}"),
                            t::mono(11.0),
                            t::enc_orange(),
                        );
                        r.width() + 6.0
                    }
                    None => 0.0,
                };
                let name = kit::elide(&p, s.label(), t::sans(13.0), row.width() - 72.0 - key_w);
                p.text(
                    pos2(row.left() + 40.0, row.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    t::sans(13.0),
                    if s.enabled { t::text() } else { t::muted() },
                );
                // Slot mix.
                let mr = Rect::from_center_size(
                    pos2(row.right() - 14.0, row.center().y),
                    vec2(18.0, 18.0),
                );
                let mut mix = s.mix as f64;
                let spec = kit::Knob {
                    label: "Mix",
                    color: t::enc_green(),
                    min: 0.0,
                    max: 1.0,
                    default: 1.0,
                    log: false,
                    fmt: &|v| format!("{:.0}%", v * 100.0),
                    hint: "Slot mix (dry / wet)",
                };
                let (_, started, changed) = kit::mini_knob(
                    ui,
                    mr,
                    ui.id().with(("slot-mix", track, k)),
                    &spec,
                    &mut mix,
                );
                if started {
                    self.checkpoint();
                }
                if changed {
                    let v = mix as f32;
                    self.song.mixer[track].fx[k].mix = v;
                    self.publish_param(
                        keyframexl::automation::Target::FxMix {
                            track: track as u16,
                            slot: k as u8,
                        },
                        v,
                    );
                }
            }
            None => {
                p.text(
                    pos2(row.left() + 26.0, row.center().y),
                    Align2::LEFT_CENTER,
                    "(empty)",
                    t::sans(13.0),
                    t::faint(),
                );
            }
        }
        let resp = resp.on_hover_text(
            "Click opens the effect in its own window · right-click to choose or remove",
        );
        if resp.clicked() {
            self.mix_slot = k;
            if slot.is_some() {
                self.open_fx_window(track, k);
            }
        }
        // Choose an effect: right-click, or click an empty slot.
        let open_menu = |ui: &mut Ui, this: &mut App| {
            kit::menu_style(ui);
            ui.set_min_width(160.0);
            for (&category, effects) in BUILTIN_GROUPS.iter() {
                ui.menu_button(category, |ui| {
                    kit::menu_style(ui);
                    for effect in effects {
                        if ui.button(effect.name).clicked() {
                            this.set_slot(track, k, Some(effect.kind));
                            ui.close_menu();
                        }
                    }
                });
            }
            ui.separator();
            ui.menu_button("External plugins", |ui| {
                kit::menu_style(ui);
                let mut effects = this.plugin_choices(false);
                effects.sort_by_cached_key(|e| (e.name.to_lowercase(), e.vendor.to_lowercase()));
                if effects.is_empty() {
                    ui.label(if this.plugins.scan_rx.is_some() {
                        "Checking plugins…"
                    } else {
                        "No effect plugins found"
                    });
                }
                for e in &effects {
                    if ui
                        .button(&e.name)
                        .on_hover_text(format!("{} · {}", e.vendor, e.path.display()))
                        .clicked()
                    {
                        this.set_slot_plugin(track, k, e);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Plugin settings…").clicked() {
                    this.open_settings(super::settings_ui::Page::Plugins);
                    ui.close_menu();
                }
            });
            ui.separator();
            if slot.is_some() && ui.button("Save effect preset…").clicked() {
                ui.close_menu();
                this.save_fx_preset(track, k);
            }
            if ui.button("Load effect preset…").clicked() {
                ui.close_menu();
                this.load_preset_dialog_for(super::presets_ui::Target::Fx(track, k));
            }
            if let Some(s) = slot.as_ref().filter(|s| s.plugin.is_some()) {
                ui.separator();
                let now = s.sidechain.map(|i| i as usize);
                ui.menu_button("Sidechain input", |ui| {
                    kit::menu_style(ui);
                    ui.label(egui::RichText::new("For plugins with a sidechain input (a ducking compressor, a gate); others ignore it").font(t::sans(12.0)).color(t::faint()));
                    if let Some(to) = this.sidechain_choices(ui, track, now, "Off") {
                        this.set_sidechain(track, k, to);
                        ui.close_menu();
                    }
                });
            }
            if slot.is_some() {
                ui.separator();
                if ui.button("Remove").clicked() {
                    this.set_slot(track, k, None);
                    ui.close_menu();
                }
            }
        };
        resp.context_menu(|ui| open_menu(ui, self));
        if slot.is_none() {
            let id = ui.make_persistent_id(("slot-pick", track, k));
            let mut menu = egui::menu::BarState::load(ui.ctx(), id);
            menu.bar_menu(&resp, |ui| open_menu(ui, self));
            menu.store(ui.ctx(), id);
        }
    }

    /// Put `kind` into a slot (None empties it). Slots stay contiguous like FL's list.
    pub(super) fn set_slot(&mut self, track: usize, k: usize, kind: Option<FxKind>) {
        if self.song.mixer[track]
            .fx
            .get(k)
            .is_some_and(|s| s.plugin.is_some())
        {
            self.collect_plugin_states();
        }
        self.checkpoint();
        let fx = &mut self.song.mixer[track].fx;
        match kind {
            Some(kind) if k < fx.len() => fx[k] = FxSlot::new(kind),
            Some(kind) => {
                fx.push(FxSlot::new(kind));
                self.mix_slot = fx.len() - 1;
            }
            None if k < fx.len() => {
                fx.remove(k);
            }
            None => {}
        }
        self.publish();
        let what = kind.map(|k| k.label()).unwrap_or("Effect removed");
        self.say(
            Tone::Quiet,
            format!("{what} on {}. Ctrl+Z undoes.", self.mixer_name(track)),
        );
    }

    /// A hosted effect plugin into a slot; its editor opens once it has loaded.
    pub(crate) fn set_slot_plugin(&mut self, track: usize, k: usize, e: &crate::plugins::Scanned) {
        if self.song.mixer[track]
            .fx
            .get(k)
            .is_some_and(|s| s.plugin.is_some())
        {
            self.collect_plugin_states();
        }
        self.checkpoint();
        let id = self.song.next_plugin_id();
        let plugin = keyframexl::sequence::PluginRef {
            id,
            format: crate::plugins::format_of(&e.path).into(),
            path: e.path.display().to_string(),
            name: e.name.clone(),
            vendor: e.vendor.clone(),
            state: String::new(),
            outs: vec![],
        };
        let slot = FxSlot::plugin(plugin);
        let fx = &mut self.song.mixer[track].fx;
        if k < fx.len() {
            fx[k] = slot;
        } else {
            fx.push(slot);
            self.mix_slot = fx.len() - 1;
        }
        self.publish();
        self.plugins_frame();
        if let Some(l) = self.plugins.live.get_mut(&id) {
            l.show_when_loaded = true;
        }
        self.say(
            Tone::Wait,
            format!("Loading {} on {}…", e.name, self.mixer_name(track)),
        );
    }

    /// Double-click on a slot: a plugin's own editor, or the built-in's window.
    pub(crate) fn open_fx_window(&mut self, track: usize, k: usize) {
        let Some(slot) = self.song.mixer[track].fx.get(k) else {
            return;
        };
        match &slot.plugin {
            Some(p) => {
                let id = p.id;
                self.open_plugin(id);
            }
            None => {
                if !self.fx_windows.contains(&(track, k)) {
                    self.fx_windows.push((track, k));
                }
            }
        }
    }

    /// Built-in effects as their own floating windows (FL: plugin windows).
    /// Impulse responses are stored as paths: decode the files the UI has not
    /// seen yet (opening a project, undo), then rebuild those slots' states.
    pub(crate) fn sync_convolution_irs(&mut self, ctx: &egui::Context) {
        use keyframexl::fx::FxKind;
        let mut todo: Vec<(usize, usize, std::path::PathBuf)> = Vec::new();
        for (t, track) in self.song.mixer.iter().enumerate() {
            for (k, slot) in track.fx.iter().enumerate() {
                if slot.kind == FxKind::Convolution
                    && let Some(path) = &slot.ir
                    && keyframexl::fx::ir_cached(path).is_none()
                {
                    todo.push((t, k, path.clone()));
                }
            }
        }
        if todo.is_empty() {
            return;
        }
        let id = egui::Id::new("conv-ir-tried");
        let mut tried: std::collections::HashSet<std::path::PathBuf> =
            ctx.data(|d| d.get_temp(id)).unwrap_or_default();
        let mut changed = false;
        for (t, k, path) in todo {
            if !tried.insert(path.clone()) {
                continue;
            }
            let loaded = std::fs::read(&path)
                .ok()
                .and_then(|b| crate::import::read_wav(&b).ok());
            match loaded {
                Some((rate, frames)) => {
                    let mono: Vec<f32> = frames.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
                    keyframexl::fx::ir_install(&path, mono, rate as f32);
                    if let Some(a) = &self.audio {
                        a.invalidate_fx(t, k);
                    }
                    changed = true;
                }
                None => self.say(
                    Tone::Bad,
                    format!("Impulse response not loaded: {}", path.display()),
                ),
            }
        }
        ctx.data_mut(|d| d.insert_temp(id, tried));
        if changed {
            self.publish();
        }
    }

    /// Pick a .wav impulse response for a Convolution slot.
    fn load_ir(&mut self, track: usize, k: usize) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Wave", &["wav"])
            .pick_file()
        else {
            return;
        };
        let loaded = std::fs::read(&path)
            .ok()
            .and_then(|b| crate::import::read_wav(&b).ok());
        let Some((rate, frames)) = loaded else {
            self.say(
                Tone::Bad,
                format!("Not a readable wave file: {}", path.display()),
            );
            return;
        };
        let mono: Vec<f32> = frames.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
        keyframexl::fx::ir_install(&path, mono, rate as f32);
        self.checkpoint();
        if let Some(slot) = self.song.mixer.get_mut(track).and_then(|m| m.fx.get_mut(k)) {
            slot.ir = Some(path.clone());
        }
        if let Some(a) = &self.audio {
            a.invalidate_fx(track, k);
        }
        self.publish();
        self.say(
            Tone::Done,
            format!(
                "Impulse loaded: {}. Ctrl+Z undoes.",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
        );
    }

    pub(crate) fn fx_windows_ui(&mut self, ctx: &egui::Context) {
        let mut keep = Vec::new();
        let mut scoped = false;
        let bounds = super::workspace::window_bounds(ctx.available_rect());
        for (n, (track, k)) in std::mem::take(&mut self.fx_windows).into_iter().enumerate() {
            let Some(slot) = self
                .song
                .mixer
                .get(track)
                .and_then(|m| m.fx.get(k))
                .cloned()
            else {
                continue;
            };
            // A new window opens in the middle of the screen, each next one a
            // step down-right (egui's own free-spot search could put it below
            // a tall floating mixer, off the screen).
            let place = |size: egui::Vec2| {
                // The screen under the toolbars (`bounds` can be only the
                // space the dock left over).
                let screen = ctx.screen_rect();
                let area =
                    Rect::from_min_max(pos2(screen.left(), screen.top() + 100.0), screen.max);
                let at = area.center() - size / 2.0 + vec2(28.0, 28.0) * (n % 6) as f32;
                at.min(area.max - size).max(area.min)
            };
            let mut open = true;
            if let Some(p) = &slot.plugin {
                // A hosted effect: the wrapper with its editor inside.
                let title = format!("{} — {}", p.name, self.mixer_name(track));
                egui::Window::new(title)
                    .id(egui::Id::new(("fx-window", track, k)))
                    .open(&mut open)
                    .collapsible(true)
                    .resizable(false)
                    .default_pos(place(vec2(480.0, 520.0)))
                    .constrain_to(bounds)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| self.plugin_wrapper(ui, p));
                if open {
                    keep.push((track, k));
                }
                continue;
            }
            let title = format!("{} — {}", slot.kind.label(), self.mixer_name(track));
            let eq = slot.kind == FxKind::ParamEq;
            let rows = slot.kind.params().len().div_ceil(3);
            let tall = rows > 3 || matches!(slot.kind, FxKind::Eq | FxKind::StereoShaper);
            let size = if eq {
                vec2(720.0, 640.0)
            } else {
                vec2(400.0, if tall { 560.0 } else { 440.0 })
            };
            // One EQ at a time gets the analyser: the first one open.
            let spectrum = if eq && !scoped {
                scoped = true;
                self.eq_spectrum(ctx, track)
            } else {
                None
            };
            egui::Window::new(title)
                .id(egui::Id::new(("fx-window", track, k)))
                .open(&mut open)
                .collapsible(true)
                .resizable(eq)
                .default_pos(place(size))
                .default_size(size)
                .resizable(true)
                .min_width(if eq { 560.0 } else { 0.0 })
                .constrain_to(bounds)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        // The title bar names the effect; here: on/off, where it comes from, reset.
                        let mut on = slot.enabled;
                        if super::settings_ui::kit_switch(ui, &mut on) {
                            self.checkpoint();
                            self.song.mixer[track].fx[k].enabled = on;
                            self.publish();
                        }
                        let b = slot.kind.builtin();
                        ui.label(
                            egui::RichText::new(if on { "On" } else { "Bypassed" })
                                .font(t::sans(13.0))
                                .color(if on { t::text() } else { t::wait() }),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} · {}", b.vendor, b.category))
                                .font(t::sans(12.0))
                                .color(t::faint()),
                        );
                        if kit::tool_button(ui, "Reset", kit::Kind::Ghost, true)
                            .on_hover_text("All parameters to their defaults")
                            .clicked()
                        {
                            self.checkpoint();
                            self.song.mixer[track].fx[k].params = slot.kind.defaults();
                            self.publish();
                        }
                    });
                    // A/B compare and the factory sounds, on their own row so
                    // the title controls never crowd them.
                    let ab_id = egui::Id::new(("fx-ab", track, k));
                    let mut ab: (Vec<f32>, Vec<f32>, usize) = ctx
                        .data(|d| d.get_temp(ab_id))
                        .unwrap_or_else(|| (slot.params.clone(), slot.params.clone(), 0));
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("A/B").font(t::sans(11.0)).color(t::faint()),
                        );
                        let a = kit::tool_button(
                            ui,
                            "A",
                            if ab.2 == 0 { kit::Kind::Normal } else { kit::Kind::Ghost },
                            true,
                        )
                        .on_hover_text("Tweak A, switch to B, tweak B, switch back");
                        let b = kit::tool_button(
                            ui,
                            "B",
                            if ab.2 == 1 { kit::Kind::Normal } else { kit::Kind::Ghost },
                            true,
                        );
                        let mut pick = None;
                        if a.clicked() && ab.2 != 0 {
                            pick = Some(0);
                        }
                        if b.clicked() && ab.2 != 1 {
                            pick = Some(1);
                        }
                        if let Some(pick) = pick {
                            let current = self.song.mixer[track].fx[k].params.clone();
                            if ab.2 == 0 {
                                ab.0 = current;
                            } else {
                                ab.1 = current;
                            }
                            ab.2 = pick;
                            self.song.mixer[track].fx[k].params =
                                if pick == 0 { ab.0.clone() } else { ab.1.clone() };
                            self.publish();
                        }
                        ui.add_space(10.0);
                        let presets = keyframexl::fx::presets::for_kind(slot.kind);
                        if presets.is_empty() {
                            ui.add_enabled(false, egui::Button::new("Presets"));
                        } else {
                            ui.menu_button("Presets", |ui| {
                                for p in &presets {
                                    if ui.button(p.name).clicked() {
                                        self.checkpoint();
                                        self.song.mixer[track].fx[k].params = p.params.clone();
                                        self.publish();
                                        ui.close_menu();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Factory settings for this effect");
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("Two settings to compare")
                                        .font(t::sans(11.0))
                                        .color(t::faint()),
                                );
                            },
                        );
                    });
                    ctx.data_mut(|d| d.insert_temp(ab_id, ab));
                    if slot.kind == FxKind::Convolution {
                        ui.horizontal(|ui| {
                            let name = slot
                                .ir
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Studio (built-in)".into());
                            ui.label(
                                egui::RichText::new("Impulse").font(t::sans(12.0)).color(t::faint()),
                            );
                            ui.label(
                                egui::RichText::new(name).font(t::sans(13.0)).color(t::text()),
                            )
                            .on_hover_text(
                                slot.ir
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| {
                                        "The built-in studio impulse: load a .wav for your own space"
                                            .into()
                                    }),
                            );
                            if kit::button(ui, "Load IR…", kit::Kind::Normal, true).clicked() {
                                self.load_ir(track, k);
                            }
                            if slot.ir.is_some()
                                && kit::button(ui, "Built-in", kit::Kind::Ghost, true).clicked()
                            {
                                self.checkpoint();
                                self.song.mixer[track].fx[k].ir = None;
                                if let Some(a) = &self.audio {
                                    a.invalidate_fx(track, k);
                                }
                                self.publish();
                            }
                        });
                        ui.add_space(2.0);
                    }
                    ui.add_space(6.0);
                    match slot.kind {
                        FxKind::ParamEq => {
                            let mut params = slot.params.clone();
                            let fid = egui::Id::new(("eq-focus", track, k));
                            let mut focus: Option<usize> =
                                ui.data(|d| d.get_temp(fid)).unwrap_or(None);
                            let rate = self
                                .audio
                                .as_ref()
                                .map_or(self.settings.audio.rate, |a| a.running.rate)
                                as f32;
                            let h = (ui.available_width() * 0.42).clamp(220.0, 420.0);
                            let (started, changed) = super::fx_views::eq(
                                ui,
                                &mut params,
                                slot.kind.params(),
                                rate,
                                spectrum.as_deref(),
                                h,
                                &mut focus,
                            );
                            ui.data_mut(|d| d.insert_temp(fid, focus));
                            if started {
                                self.checkpoint();
                            }
                            if changed {
                                self.song.mixer[track].fx[k].params = params;
                                self.publish();
                            }
                        }
                        FxKind::Compressor | FxKind::Limiter => {
                            let gr = self.gain_reduction(ui.ctx(), track, k);
                            super::fx_views::dynamics(ui, slot.kind, &slot.params, gr);
                        }
                        FxKind::Delay | FxKind::Reverb => {
                            super::fx_views::ambience(ui, slot.kind, &slot.params)
                        }
                        FxKind::Multiband => {
                            let mut params = slot.params.clone();
                            let (started, changed) =
                                super::fx_views::multiband(ui, track, k, &mut params);
                            if started {
                                self.checkpoint();
                            }
                            if changed {
                                self.song.mixer[track].fx[k].params = params;
                                self.publish();
                            }
                        }
                        FxKind::Time => {
                            let mut params = slot.params.clone();
                            let (started, changed) = super::fx_views::time(ui, &mut params);
                            if started {
                                self.checkpoint();
                            }
                            if changed {
                                self.song.mixer[track].fx[k].params = params;
                                self.publish();
                            }
                        }
                        FxKind::Filter => self.fx_custom(ui, track, k, super::fx_views::filter),
                        FxKind::Drive => self.fx_custom(ui, track, k, super::fx_views::drive),
                        FxKind::Chorus => self.fx_custom(ui, track, k, super::fx_views::chorus),
                        FxKind::HyperChorus => {
                            self.fx_custom(ui, track, k, super::fx_views::hyper_chorus)
                        }
                        FxKind::Squeeze => self.fx_custom(ui, track, k, super::fx_views::squeeze),
                        FxKind::Stereo => self.fx_custom(ui, track, k, super::fx_views::stereo),
                        FxKind::Balance => self.fx_custom(ui, track, k, super::fx_views::balance),
                        FxKind::MidSide => self.fx_custom(ui, track, k, super::fx_views::mid_side),
                        FxKind::Phaser => self.fx_custom(ui, track, k, super::fx_views::phaser),
                        FxKind::Eq => self.fx_custom(ui, track, k, super::fx_views::eq3),
                        FxKind::AutoPan => self.fx_custom(ui, track, k, super::fx_views::auto_pan),
                        FxKind::Tremolo => self.fx_custom(ui, track, k, super::fx_views::tremolo),
                        FxKind::Bitcrusher => {
                            self.fx_custom(ui, track, k, super::fx_views::bitcrusher)
                        }
                        FxKind::NoiseGate => {
                            self.fx_custom(ui, track, k, super::fx_views::noise_gate)
                        }
                        FxKind::RingMod => self.fx_custom(ui, track, k, super::fx_views::ring_mod),
                        FxKind::TiltEq => self.fx_custom(ui, track, k, super::fx_views::tilt_eq),
                        FxKind::Wavefolder => {
                            self.fx_custom(ui, track, k, super::fx_views::wavefolder)
                        }
                        FxKind::Transient => {
                            self.fx_custom(ui, track, k, super::fx_views::transient)
                        }
                        FxKind::Vocoder => self.fx_custom(ui, track, k, super::fx_views::vocoder),
                        FxKind::Convolution => {
                            self.fx_custom(ui, track, k, super::fx_views::convolution)
                        }
                        FxKind::StereoShaper => {
                            self.fx_custom(ui, track, k, super::fx_views::stereo_shaper)
                        }
                        _ => super::fx_response::panel(ui, slot.kind, &slot.params),
                    }
                    if slot.kind.takes_sidechain() {
                        self.sidechain_row(ui, track, k);
                    }
                    ui.add_space(6.0);
                    self.fx_params(ui, track, k);
                });
            if open {
                keep.push((track, k));
            }
        }
        if !scoped && let Some(a) = &self.audio {
            a.scope
                .track
                .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
        }
        // Windows opened meanwhile (double-click inside a window) stay too.
        keep.extend(std::mem::take(&mut self.fx_windows));
        keep.dedup();
        self.fx_windows = keep;
    }

    /// The detector's input (FL: sidechain): this insert, or another one
    /// whose output drives the gain (a kick ducking a bass).
    fn sidechain_row(&mut self, ui: &mut Ui, track: usize, k: usize) {
        let now = self.song.mixer[track].fx[k].sidechain.map(|s| s as usize);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            kit::caps(ui.painter(), ui.cursor().min + vec2(0.0, 12.0), Align2::LEFT_CENTER, "Sidechain", 11.0, t::faint());
            ui.add_space(78.0);
            let mut pick: Option<Option<usize>> = None;
            let shown = now.map(|i| self.insert_label(i)).unwrap_or_else(|| "Off · own signal".into());
            kit::dropdown(ui, &format!("sc-{track}-{k}"), &shown, 200.0, |ui| {
                pick = self.sidechain_choices(ui, track, now, "Off · own signal");
            })
            .on_hover_text("Which insert feeds the sidechain: the kick to duck this one under it, or a voice to drive the vocoder");
            if let Some(to) = pick {
                self.set_sidechain(track, k, to);
            }
        });
    }

    fn insert_label(&self, i: usize) -> String {
        format!("{i} · {}", self.mixer_name(i))
    }

    /// The inserts a slot on `track` can listen to; the one clicked, if any
    /// (`Some(None)`: off). Inserts this one already feeds are greyed out.
    fn sidechain_choices(
        &self,
        ui: &mut Ui,
        track: usize,
        now: Option<usize>,
        off: &str,
    ) -> Option<Option<usize>> {
        let mut pick = None;
        if ui.selectable_label(now.is_none(), off).clicked() {
            pick = Some(None);
        }
        for i in (1..self.song.mixer.len()).filter(|&i| i != track) {
            // A key that this insert already feeds would close a loop.
            let ok = !keyframexl::sequence::route_would_loop(&self.song.mixer, i, track)
                || now == Some(i);
            let r = ui.add_enabled(
                ok,
                egui::SelectableLabel::new(now == Some(i), self.insert_label(i)),
            );
            let r = if ok {
                r
            } else {
                r.on_disabled_hover_text(
                    "This insert already feeds this one: it would close a loop",
                )
            };
            if r.clicked() {
                pick = Some(Some(i));
            }
        }
        pick
    }

    /// Point slot `k`'s sidechain at insert `to` (None: off), with undo.
    pub(crate) fn set_sidechain(&mut self, track: usize, k: usize, to: Option<usize>) {
        let Some(slot) = self.song.mixer.get(track).and_then(|m| m.fx.get(k)) else {
            return;
        };
        if slot.sidechain.map(|s| s as usize) == to {
            return;
        }
        let plugin = slot.plugin.is_some();
        self.checkpoint();
        self.song.mixer[track].fx[k].sidechain = to.map(|i| i as u16);
        self.publish();
        let what = match (to, plugin) {
            (Some(i), false) => format!(
                "{} now ducks under {}.",
                self.mixer_name(track),
                self.insert_label(i)
            ),
            (Some(i), true) => format!(
                "{} reaches {}'s sidechain input.",
                self.insert_label(i),
                self.song.mixer[track].fx[k].label()
            ),
            (None, false) => format!(
                "{} compresses on its own signal again.",
                self.mixer_name(track)
            ),
            (None, true) => format!(
                "{} has no sidechain now.",
                self.song.mixer[track].fx[k].label()
            ),
        };
        self.say(Tone::Quiet, format!("{what} Ctrl+Z undoes."));
    }

    /// The analysed spectrum of insert `track` (dB per bin), smoothed for
    /// the eye: rises fast, falls at 40 dB/s. Points the analyser at it.
    fn eq_spectrum(&self, ctx: &egui::Context, track: usize) -> Option<Vec<f32>> {
        let audio = self.audio.as_ref()?;
        audio
            .scope
            .track
            .store(track, std::sync::atomic::Ordering::Relaxed);
        let mut frames = vec![0.0f32; 4096];
        audio.scope.read(&mut frames);
        let now = super::fx_views::spectrum_db(&frames);
        let id = egui::Id::new(("eq-spectrum", track));
        let t_now = ctx.input(|i| i.time);
        let (old, at): (Vec<f32>, f64) =
            ctx.data(|d| d.get_temp(id)).unwrap_or((now.clone(), t_now));
        let fall = 40.0 * (t_now - at).max(0.0) as f32;
        let shown: Vec<f32> = now
            .iter()
            .zip(&old)
            .map(|(&n, &o)| {
                if n > o {
                    o + (n - o) * 0.6
                } else {
                    n.max(o - fall)
                }
            })
            .collect();
        ctx.data_mut(|d| d.insert_temp(id, (shown.clone(), t_now)));
        ctx.request_repaint();
        Some(shown)
    }

    /// Parametric EQ 2's band strip under the graph: a column per band with
    /// Freq · Gain · Q (FL's band table, turned upright).
    fn param_eq_bands(&mut self, ui: &mut Ui, track: usize, k: usize, slot: &FxSlot) {
        let specs = slot.kind.params();
        let names = ["Low", "2", "3", "4", "5", "6", "High"];
        let focus: Option<usize> = ui
            .data(|d| d.get_temp(egui::Id::new(("eq-focus", track, k))))
            .unwrap_or(None);
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
        let col = area.width() / 7.0;
        let p = ui.painter().clone();
        for (b, name) in names.iter().enumerate() {
            let c = Rect::from_min_size(
                pos2(area.left() + b as f32 * col, area.top()),
                vec2(col, area.height()),
            )
            .shrink2(vec2(3.0, 0.0));
            let active = slot.params[b * 3 + 1].abs() >= 0.05;
            let color = super::fx_views::band_color(b);
            p.rect_filled(
                c,
                4.0,
                if focus == Some(b) {
                    t::raised()
                } else {
                    t::sunken().gamma_multiply(0.5)
                },
            );
            p.rect_filled(
                Rect::from_min_size(c.min, vec2(c.width(), 3.0)),
                1.0,
                if active {
                    color
                } else {
                    color.gamma_multiply(0.35)
                },
            );
            kit::caps(
                &p,
                pos2(c.center().x, c.top() + 14.0),
                Align2::CENTER_CENTER,
                name,
                11.0,
                if active { t::text() } else { t::muted() },
            );
            for j in 0..3 {
                let pi = b * 3 + j;
                let spec = &specs[pi];
                let y = c.top() + 42.0 + j as f32 * 36.0;
                let kr = Rect::from_center_size(pos2(c.left() + 16.0, y), vec2(22.0, 22.0));
                let fmt = |v: f64| keyframexl::fx::format(spec, v as f32);
                let label = ["Freq", "Gain", "Q"][j];
                let knob = kit::Knob {
                    label,
                    color,
                    min: spec.min as f64,
                    max: spec.max as f64,
                    default: spec.default as f64,
                    log: spec.log,
                    fmt: &fmt,
                    hint: spec.name,
                };
                let mut v = slot.params[pi] as f64;
                kit::automatable(
                    ui,
                    keyframexl::automation::Target::FxParam {
                        track: track as u16,
                        slot: k as u8,
                        param: pi as u16,
                    },
                );
                let (_, started, changed) =
                    kit::mini_knob(ui, kr, ui.id().with(("peq", track, k, pi)), &knob, &mut v);
                if started {
                    self.checkpoint();
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(("eq-focus", track, k)), Some(b)));
                }
                if changed {
                    let v = v as f32;
                    self.song.mixer[track].fx[k].params[pi] = v;
                    self.publish_param(
                        keyframexl::automation::Target::FxParam {
                            track: track as u16,
                            slot: k as u8,
                            param: pi as u16,
                        },
                        v,
                    );
                }
                let shown = keyframexl::fx::format(spec, self.song.mixer[track].fx[k].params[pi]);
                p.text(
                    pos2(kr.right() + 5.0, y - 6.0),
                    Align2::LEFT_CENTER,
                    label,
                    t::sans(10.0),
                    t::faint(),
                );
                p.text(
                    pos2(kr.right() + 5.0, y + 7.0),
                    Align2::LEFT_CENTER,
                    kit::elide(&p, &shown, t::mono(11.0), c.right() - kr.right() - 6.0),
                    t::mono(11.0),
                    if active || j != 1 {
                        t::text()
                    } else {
                        t::muted()
                    },
                );
            }
        }
    }

    /// The controls of slot `k`: knobs for a built-in, status and editor for a plugin.
    fn fx_custom(
        &mut self,
        ui: &mut Ui,
        track: usize,
        k: usize,
        f: impl FnOnce(&mut Ui, usize, usize, &mut [f32]) -> (bool, bool),
    ) {
        let Some(slot) = self
            .song
            .mixer
            .get(track)
            .and_then(|m| m.fx.get(k))
            .cloned()
        else {
            return;
        };
        let mut params = slot.params.clone();
        let (started, changed) = f(ui, track, k, &mut params);
        if started {
            self.checkpoint();
        }
        if changed {
            if let Some(slot) = self.song.mixer.get_mut(track).and_then(|m| m.fx.get_mut(k)) {
                slot.params = params;
            }
            self.publish();
        }
    }

    fn fx_params(&mut self, ui: &mut Ui, track: usize, k: usize) {
        let Some(slot) = self.song.mixer[track].fx.get(k).cloned() else {
            return;
        };
        if let Some(p) = &slot.plugin {
            let (status, color) = self.plugin_status(p.id);
            let fmt = if p.format.is_empty() {
                crate::plugins::format_of(std::path::Path::new(&p.path)).to_uppercase()
            } else {
                p.format.to_uppercase()
            };
            ui.label(
                egui::RichText::new(format!(
                    "{} · {fmt}",
                    if p.vendor.is_empty() {
                        "Plugin"
                    } else {
                        &p.vendor
                    }
                ))
                .font(t::sans(12.0))
                .color(t::faint()),
            );
            ui.label(egui::RichText::new(status).font(t::sans(12.0)).color(color));
            let open = self.plugins.live.get(&p.id).is_some_and(|l| l.editor);
            if kit::button(
                ui,
                if open { "Hide editor" } else { "Open plugin" },
                kit::Kind::Normal,
                true,
            )
            .clicked()
            {
                if open && self.plugins.parent.is_none() {
                    self.show_editor(p.id);
                } else {
                    self.open_plugin(p.id);
                }
            }
            return;
        }
        if slot.kind == FxKind::Time
            || slot.kind == FxKind::Multiband
            || super::fx_views::has_face(slot.kind)
        {
            return;
        }
        if slot.kind == FxKind::Bloom {
            let mut params = slot.params.clone();
            let (started, changed) =
                super::fx_views::bloom(ui, track, k, &mut params, slot.enabled);
            if started {
                self.checkpoint();
            }
            if changed {
                self.song.mixer[track].fx[k].params = params;
                self.publish();
            }
            return;
        }
        if slot.kind == FxKind::ParamEq {
            self.param_eq_bands(ui, track, k, &slot);
            return;
        }
        let groups: &[&str] = match slot.kind {
            FxKind::Compressor => &["Dynamics", "Timing / output"],
            FxKind::Limiter => &["Level", "Release"],
            FxKind::Delay => &["Echo", "Tone / mix"],
            FxKind::Reverb => &["Space", "Filter / mix"],
            FxKind::Squeeze => &["Squarize", "Puncher", "Filter", "Output"],
            FxKind::HyperChorus => &["Delay line", "Movement", "Filters", "Output"],
            FxKind::StereoShaper => &["Left & right", "Delay / phase"],
            _ => &["Controls", "Output"],
        };
        for (group, specs) in slot.kind.params().chunks(3).enumerate() {
            ui.add_space(4.0);
            let (head, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
            kit::caps(
                ui.painter(),
                head.left_center(),
                Align2::LEFT_CENTER,
                groups
                    .get(group)
                    .copied()
                    .unwrap_or(groups[groups.len() - 1]),
                10.5,
                t::faint(),
            );
            ui.horizontal_wrapped(|ui| {
                for (offset, spec) in specs.iter().enumerate() {
                    // A fixed cell per knob, so a long name never runs into the next one.
                    ui.allocate_ui_with_layout(
                        vec2(118.0, 46.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(118.0);
                            let pi = group * 3 + offset;
                            let mut v = slot.params[pi] as f64;
                            let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
                            let color = [
                                t::enc_blue(),
                                t::enc_green(),
                                t::enc_white(),
                                t::enc_orange(),
                            ][pi % 4];
                            let knob = kit::Knob {
                                label: spec.name,
                                color,
                                min: spec.min as f64,
                                max: spec.max as f64,
                                default: spec.default as f64,
                                log: spec.log,
                                fmt: &fmt,
                                hint: spec.name,
                            };
                            kit::automatable(
                                ui,
                                keyframexl::automation::Target::FxParam {
                                    track: track as u16,
                                    slot: k as u8,
                                    param: pi as u16,
                                },
                            );
                            let (_, started, changed) = kit::knob(ui, &knob, &mut v);
                            if started {
                                self.checkpoint();
                            }
                            if changed {
                                let v = if matches!(spec.name, "Type" | "Bits") {
                                    v.round()
                                } else {
                                    v
                                } as f32;
                                self.song.mixer[track].fx[k].params[pi] = v;
                                self.publish_param(
                                    keyframexl::automation::Target::FxParam {
                                        track: track as u16,
                                        slot: k as u8,
                                        param: pi as u16,
                                    },
                                    v,
                                );
                            }
                        },
                    );
                }
            });
        }
    }

    fn sends_ui(&mut self, ui: &mut Ui, track: usize) {
        kit::caps(
            ui.painter(),
            ui.cursor().min + vec2(0.0, 6.0),
            Align2::LEFT_CENTER,
            "sends",
            11.0,
            t::faint(),
        );
        ui.add_space(14.0);
        let sends = self.song.mixer[track].sends.clone();
        for (si, s) in sends.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("→ {}", self.mixer_name(s.to as usize)))
                        .font(t::sans(13.0)),
                );
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                let mut lv = s.level as f64;
                let spec = kit::Knob {
                    label: "Send",
                    color: t::enc_blue(),
                    min: 0.0,
                    max: 1.0,
                    default: 1.0,
                    log: false,
                    fmt: &|v| format!("{:.0}%", v * 100.0),
                    hint: "Send level",
                };
                let (_, started, changed) =
                    kit::mini_knob(ui, r, ui.id().with(("send", track, si)), &spec, &mut lv);
                if started {
                    self.checkpoint();
                }
                if changed {
                    self.song.mixer[track].sends[si].level = lv as f32;
                    self.publish();
                }
                if kit::tool_button(ui, "×", kit::Kind::Ghost, true)
                    .on_hover_text("Remove send")
                    .clicked()
                {
                    self.checkpoint();
                    self.song.mixer[track].sends.remove(si);
                    self.publish();
                }
            });
        }
        let targets: Vec<usize> = (track + 1..self.song.mixer.len())
            .filter(|t| !sends.iter().any(|s| s.to as usize == *t))
            .collect();
        if !targets.is_empty() {
            let mut pick = None;
            kit::dropdown(ui, &format!("send-{track}"), "Add send", 140.0, |ui| {
                for t in &targets {
                    if ui.button(self.mixer_name(*t)).clicked() {
                        pick = Some(*t);
                    }
                }
            })
            .on_hover_text("Send this track to a later insert (a bus); sends go after the fader");
            if let Some(to) = pick {
                self.checkpoint();
                self.song.mixer[track].sends.push(Send {
                    to: to as u16,
                    level: 1.0,
                });
                self.publish();
            }
        }
    }
}

/// The selected track's routes: tracks it sends into, and tracks sending into it.
/// Master is not lit as a source of everything when it is selected.
fn routes(mixer: &[keyframexl::sequence::MixerTrack], sel: usize) -> (Vec<usize>, Vec<usize>) {
    let mut to = Vec::new();
    let mut from = Vec::new();
    if let Some(m) = mixer.get(sel) {
        if sel > 0 && m.to_master {
            to.push(0);
        }
        to.extend(m.sends.iter().map(|s| s.to as usize));
    }
    if sel > 0 {
        for (j, m) in mixer.iter().enumerate() {
            if j != sel && m.sends.iter().any(|s| s.to as usize == sel) {
                from.push(j);
            }
        }
    }
    (to, from)
}

#[cfg(test)]
mod route_tests {
    use super::routes;
    use keyframexl::sequence::{MixerTrack, Send};

    #[test]
    fn routes_of_a_track() {
        let mut mixer: Vec<MixerTrack> =
            (0..5).map(|i| MixerTrack::new(&format!("T{i}"))).collect();
        mixer[1].sends.push(Send { to: 3, level: 0.5 });
        mixer[2].sends.push(Send { to: 3, level: 1.0 });
        mixer[3].to_master = false;
        assert_eq!(routes(&mixer, 1), (vec![0, 3], vec![]));
        assert_eq!(
            routes(&mixer, 3),
            (vec![], vec![1, 2]),
            "not to Master, fed by 1 and 2"
        );
        assert_eq!(routes(&mixer, 0), (vec![], vec![]), "Master lights nothing");
    }
}

#[cfg(test)]
mod route_gesture_tests {
    //! The route button gestures drive the real mixer tab headless, so the
    //! strip rects are exact (no window-manager offset like a live check has).
    use super::super::{App, Tab};
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
    use keyframexl::sequence::{Send, Song};

    fn setup() -> (egui::Context, App) {
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        (ctx, App::with_song(Song::default(), None))
    }

    fn run_with(ctx: &egui::Context, app: &mut App, events: Vec<Event>, mods: Modifiers) {
        let mut tab = Tab::Mixer;
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

    fn button_at(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        }
    }

    fn layout(ctx: &egui::Context, app: &mut App) {
        run(ctx, app, vec![]);
        run(ctx, app, vec![]);
        assert!(!app.mix_strip_rects.is_empty(), "strips laid out");
    }

    /// Centre of strip `i`'s route button (tall strips: bottom − 16).
    fn route_button(app: &App, i: usize) -> Pos2 {
        let r = app.mix_strip_rects[i];
        pos2(r.center().x, r.bottom() - 16.0)
    }

    fn drag(ctx: &egui::Context, app: &mut App, mods: Modifiers, a: Pos2, b: Pos2) {
        let press = Event::PointerButton {
            pos: a,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: mods,
        };
        run_with(ctx, app, vec![Event::PointerMoved(a), press], mods);
        run_with(ctx, app, vec![Event::PointerMoved(a + (b - a) * 0.5)], mods);
        run_with(ctx, app, vec![Event::PointerMoved(b)], mods);
        let release = Event::PointerButton {
            pos: b,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: mods,
        };
        run_with(ctx, app, vec![release], mods);
        run(ctx, app, vec![]);
    }

    fn click(ctx: &egui::Context, app: &mut App, pos: Pos2) {
        run(
            ctx,
            app,
            vec![Event::PointerMoved(pos), button_at(pos, true)],
        );
        run(ctx, app, vec![button_at(pos, false)]);
    }

    #[test]
    fn drag_reroutes_and_ctrl_drag_adds_a_send() {
        let (ctx, mut app) = setup();
        layout(&ctx, &mut app);
        let from = route_button(&app, 3);
        let to = app.mix_strip_rects[5].center();
        let h0 = app.history.len();
        drag(&ctx, &mut app, Modifiers::default(), from, to);
        assert!(!app.song.mixer[3].to_master, "replaced, not added");
        assert_eq!(app.song.mixer[3].sends, vec![Send { to: 5, level: 1.0 }]);
        assert_eq!(app.history.len(), h0 + 1, "one undo step");
        assert!(
            app.status.1.contains("Insert 3"),
            "says what happened: {}",
            app.status.1
        );
        // Ctrl drag adds instead of replacing.
        let to6 = app.mix_strip_rects[6].center();
        let from3 = route_button(&app, 3);
        drag(
            &ctx,
            &mut app,
            Modifiers {
                command: true,
                ..Default::default()
            },
            from3,
            to6,
        );
        assert_eq!(
            app.song.mixer[3].sends,
            vec![Send { to: 5, level: 1.0 }, Send { to: 6, level: 1.0 }]
        );
    }

    #[test]
    fn loops_and_self_drops_are_refused() {
        let (ctx, mut app) = setup();
        layout(&ctx, &mut app);
        // 3 -> 5 first, so 5 -> 3 would close a loop.
        let from3 = route_button(&app, 3);
        let at5 = app.mix_strip_rects[5].center();
        drag(&ctx, &mut app, Modifiers::default(), from3, at5);
        assert_eq!(app.song.mixer[3].sends, vec![Send { to: 5, level: 1.0 }]);
        let h0 = app.history.len();
        let from5 = route_button(&app, 5);
        let at3 = app.mix_strip_rects[3].center();
        drag(&ctx, &mut app, Modifiers::default(), from5, at3);
        assert!(app.song.mixer[5].sends.is_empty(), "no loop created");
        assert_eq!(app.history.len(), h0, "refused: no undo step");
        assert!(
            app.status.1.contains("would loop"),
            "says why: {}",
            app.status.1
        );
        let from3 = route_button(&app, 3);
        let at3 = app.mix_strip_rects[3].center();
        drag(&ctx, &mut app, Modifiers::default(), from3, at3);
        assert_eq!(app.song.mixer[3].sends, vec![Send { to: 5, level: 1.0 }]);
        assert!(
            app.status.1.contains("can't feed itself"),
            "{}",
            app.status.1
        );
    }

    #[test]
    fn click_toggles_and_ctrl_click_sends() {
        let (ctx, mut app) = setup();
        layout(&ctx, &mut app);
        let at4 = route_button(&app, 4);
        click(&ctx, &mut app, at4);
        assert!(
            !app.song.mixer[4].to_master,
            "plain click cuts the Master route"
        );
        assert!(
            app.status.1.contains("no longer feeds Master"),
            "{}",
            app.status.1
        );
        // Ctrl+click on strip 5's button sends the selected strip (4) there.
        app.mix_sel = 4;
        run(&ctx, &mut app, vec![]);
        let at = route_button(&app, 5);
        let ctrl = Modifiers {
            command: true,
            ..Default::default()
        };
        let press = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: ctrl,
        };
        run_with(&ctx, &mut app, vec![Event::PointerMoved(at), press], ctrl);
        let release = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: ctrl,
        };
        run_with(&ctx, &mut app, vec![release], ctrl);
        assert_eq!(app.mix_sel, 4, "selection stays on the source");
        assert_eq!(app.song.mixer[4].sends, vec![Send { to: 5, level: 1.0 }]);
    }
}
