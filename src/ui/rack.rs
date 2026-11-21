//! Channel rack: one row per channel. Step pads sit in beat trays (EP-133 keys
//! in a recess); a lit pad is a one-step note, its bar the velocity. A channel
//! whose notes leave the step grid shows a note preview instead (FL). LEDs
//! above the pads carry the playhead.

use super::{App, Cmd, Slot, Tab, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::sequence::{Note, STEP, STEP_PITCH, Tick};

const NAME_W: f32 = 230.0;

#[derive(Clone, Copy)]
struct ChannelDrag(u64);
const MIN_PAD_W: f32 = 22.0;
/// Pads stay near-square like FL's; extra width is left empty, not stretched.
const MAX_PAD_W: f32 = 34.0;
const TRAY_GAP: f32 = 0.0;
const PAD_GAP: f32 = 4.0;
const PAD_H: f32 = 22.0;
const ROW_H: f32 = 30.0;
const LED_ROW: f32 = 18.0;
const GRAPH_H: f32 = 72.0;
/// Thin step-number row above the LED row (numbers 1..N, 12 px mono).
const STEP_NUM_H: f32 = 14.0;
/// Warm-white lit pad (FL-style). Only the velocity bar stays red.
const LIT_PAD_TOP: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xEB, 0xDD);
const LIT_PAD_BOTTOM: egui::Color32 = egui::Color32::from_rgb(0xD6, 0xCC, 0xB4);

impl App {
    /// Rack header: window menu, swing (FL global swing lives on the rack).
    fn rack_header(&mut self, ui: &mut Ui, sub: &str) {
        let steps = self.pattern().step_count();
        App::panel_header(ui, sub, |ui, slot| {
            if slot == Slot::Menu {
                ui.menu_button("Add channel", |ui| self.add_channel_menu(ui));
                ui.separator();
                if ui.button("Unmute all channels").clicked() {
                    self.checkpoint();
                    self.song
                        .instruments
                        .iter_mut()
                        .for_each(|i| i.muted = false);
                    self.publish();
                    ui.close_menu();
                }
                if ui.button("Clear this pattern's notes").clicked() {
                    self.checkpoint();
                    self.pattern_mut().notes.clear();
                    self.pattern_mut().slides.clear();
                    self.publish();
                    self.say(Tone::Quiet, "Pattern cleared. Ctrl+Z undoes.");
                    ui.close_menu();
                }
                let any = !self.rack_sel.is_empty();
                if ui
                    .add_enabled(
                        any,
                        egui::Button::new("Clone selected").shortcut_text("Alt+C"),
                    )
                    .clicked()
                {
                    self.clone_channels();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(any, egui::Button::new("Move up").shortcut_text("Alt+↑"))
                    .clicked()
                {
                    self.move_channels(-1);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(any, egui::Button::new("Move down").shortcut_text("Alt+↓"))
                    .clicked()
                {
                    self.move_channels(1);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        any,
                        egui::Button::new("Delete selected").shortcut_text("Alt+Del"),
                    )
                    .clicked()
                {
                    self.delete_channels();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        any,
                        egui::Button::new("Route selected to a free insert")
                            .shortcut_text("Ctrl+L"),
                    )
                    .clicked()
                {
                    if let Some(to) = self.free_insert() {
                        self.route_selected_channels(to);
                    }
                    ui.close_menu();
                }
                ui.separator();
                if kit::menu_check(ui, self.rack_graph, "Graph editor (velocity)", "Ctrl+K")
                    .clicked()
                {
                    self.rack_graph = !self.rack_graph;
                    ui.close_menu();
                }
                ui.menu_button("Pattern length", |ui| {
                    kit::menu_style(ui);
                    for bars in [1i64, 2, 4, 8] {
                        if ui.button(format!("{bars} bar(s)")).clicked() {
                            self.checkpoint();
                            let p = self.pattern_mut();
                            let need = p
                                .notes
                                .iter()
                                .map(|n| n.end())
                                .chain(p.slides.iter().map(|s| s.start + s.length))
                                .max()
                                .unwrap_or(0);
                            let len = (bars * keyframexl::sequence::BAR).max(
                                ((need + keyframexl::sequence::BAR - 1)
                                    / keyframexl::sequence::BAR)
                                    * keyframexl::sequence::BAR,
                            );
                            p.length = len;
                            self.publish();
                            ui.close_menu();
                        }
                    }
                });
                self.window_menu_tail(ui, Tab::Rack);
                return;
            }
            let mut swing = self.song.swing;
            let (_, started, changed) = kit::knob(
                ui,
                &kit::Knob {
                    label: "Swing",
                    color: t::enc_green(),
                    min: 0.0,
                    max: 0.6,
                    default: 0.0,
                    log: false,
                    fmt: &|v| format!("{:.0}%", v * 100.0),
                    hint: "Delays every second 1/16. 0% is straight.",
                },
                &mut swing,
            );
            if started {
                self.checkpoint();
            }
            if changed {
                self.song.swing = swing;
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!("Swing {:.0}%. Ctrl+Z undoes.", swing * 100.0),
                );
            }
            let g = kit::caps_galley(ui.painter(), &format!("{steps} steps"), 11.0, t::faint());
            let (r, _) = ui.allocate_exact_size(g.size() + vec2(8.0, 0.0), Sense::hover());
            ui.painter().galley(r.min, g, t::faint());
        });
    }

    pub(crate) fn toggle_mute(&mut self, id: u64) {
        self.checkpoint();
        let Some(inst) = self.song.instruments.iter_mut().find(|i| i.id == id) else {
            return;
        };
        inst.muted = !inst.muted;
        let (name, on) = (inst.name.clone(), inst.muted);
        self.publish();
        self.say(
            Tone::Quiet,
            format!(
                "{name} {}. Ctrl+Z undoes.",
                if on { "muted" } else { "unmuted" }
            ),
        );
    }

    pub(crate) fn toggle_solo(&mut self, id: u64) {
        self.checkpoint();
        self.song.solo(id);
        let name = self.instrument_name(id);
        let on = self.song.is_solo(id);
        self.publish();
        self.say(
            Tone::Quiet,
            if on {
                format!("Only {name} plays. Solo again to hear all. Ctrl+Z undoes.")
            } else {
                "All channels play. Ctrl+Z undoes.".into()
            },
        );
    }

    /// Switch step `step` of channel `id`: a one-step note at C5.
    fn toggle_step(&mut self, id: u64, step: usize, shift: bool) {
        let name = self.instrument_name(id);
        self.checkpoint();
        let at = self.pattern().step_note(id, step);
        let vel = at.map(|i| self.pattern().notes[i].velocity).unwrap_or(0);
        let nv = if shift {
            match vel {
                0 => 127,
                127 => 70,
                70 => 100,
                _ => 127,
            }
        } else if at.is_some() {
            0
        } else {
            100
        };
        match (at, nv) {
            (Some(i), 0) => {
                self.pattern_mut().notes.remove(i);
            }
            (Some(i), v) => self.pattern_mut().notes[i].velocity = v,
            (None, v) => {
                let pid = self.selected;
                self.song
                    .add_note(pid, Note::new(id, step as i64 * STEP, STEP, STEP_PITCH, v));
            }
        }
        self.publish();
        self.say(
            Tone::Quiet,
            if nv == 0 {
                format!("{name} step {:02} off. Ctrl+Z undoes.", step + 1)
            } else {
                format!(
                    "{name} step {:02} at velocity {nv}. Ctrl+Z undoes.",
                    step + 1
                )
            },
        );
    }

    /// Step tools act on this channel's notes in the current pattern only
    /// while they sit on the step grid; off-grid channels edit in the roll.
    fn step_target(&self, id: u64) -> Option<usize> {
        self.pattern()
            .is_step_channel(id)
            .then(|| self.pattern().step_count())
    }

    fn step_notes(&self, id: u64) -> Vec<Note> {
        self.pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == id)
            .copied()
            .collect()
    }

    fn no_steps(&mut self, id: u64) {
        let name = self.instrument_name(id);
        self.say(
            Tone::Quiet,
            format!("{name} has off-grid notes: edit them in the piano roll."),
        );
    }

    fn step_shift(&mut self, id: u64, right: bool) {
        let name = self.instrument_name(id);
        let Some(steps) = self.step_target(id) else {
            self.no_steps(id);
            return;
        };
        self.checkpoint();
        let mut chan = self.step_notes(id);
        keyframexl::step_ops::shift(&mut chan, steps, right);
        let moved: std::collections::HashMap<u64, Tick> =
            chan.into_iter().map(|n| (n.id, n.start)).collect();
        for n in self
            .pattern_mut()
            .notes
            .iter_mut()
            .filter(|n| n.instrument == id)
        {
            if let Some(s) = moved.get(&n.id) {
                n.start = *s;
            }
        }
        self.publish();
        self.say(
            Tone::Quiet,
            format!(
                "{name} steps shifted {}. Ctrl+Z undoes.",
                if right { "right" } else { "left" }
            ),
        );
    }

    fn step_fill(&mut self, id: u64, every: usize) {
        let name = self.instrument_name(id);
        let Some(steps) = self.step_target(id) else {
            self.no_steps(id);
            return;
        };
        self.checkpoint();
        let chan = self.step_notes(id);
        let have: std::collections::HashSet<u64> = chan.iter().map(|n| n.id).collect();
        let filled = keyframexl::step_ops::fill_every(&chan, steps, every, id, 100);
        let pid = self.selected;
        let mut added = 0;
        for n in filled {
            if have.contains(&n.id) {
                continue;
            }
            self.song.add_note(pid, n);
            added += 1;
        }
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name}: filled every {every} ({added} step(s) added). Ctrl+Z undoes."),
        );
    }

    fn step_invert(&mut self, id: u64) {
        let name = self.instrument_name(id);
        let Some(steps) = self.step_target(id) else {
            self.no_steps(id);
            return;
        };
        self.checkpoint();
        let chan = self.step_notes(id);
        let off = chan.len();
        let inv = keyframexl::step_ops::invert(&chan, steps, id, 100);
        let on = inv.len();
        self.pattern_mut().notes.retain(|n| n.instrument != id);
        let pid = self.selected;
        for n in inv {
            self.song.add_note(pid, n);
        }
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name} steps inverted ({off} off, {on} on). Ctrl+Z undoes."),
        );
    }

    fn step_clear(&mut self, id: u64) {
        let name = self.instrument_name(id);
        let Some(_) = self.step_target(id) else {
            self.no_steps(id);
            return;
        };
        self.checkpoint();
        let mut chan = self.step_notes(id);
        let n = keyframexl::step_ops::clear(&mut chan);
        self.pattern_mut().notes.retain(|n| n.instrument != id);
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name} steps cleared ({n} removed). Ctrl+Z undoes."),
        );
    }

    /// This channel's notes in the current pattern, in time order.
    fn channel_notes(&self, id: u64) -> Vec<Note> {
        let mut v: Vec<Note> = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == id)
            .copied()
            .collect();
        v.sort_by_key(|n| (n.start, n.pitch));
        v
    }

    /// Copy, cut, paste, duplicate and clear a channel's notes in the current
    /// pattern (FL: the channel's Edit items). The clipboard is the piano
    /// roll's, so notes move between channels, rolls and the rack.
    pub(crate) fn channel_notes_menu(&mut self, ui: &mut Ui, id: u64) {
        let has = self.pattern().notes.iter().any(|n| n.instrument == id);
        let clip = !self.note_clip.is_empty();
        let item = |ui: &mut Ui, label: &str, key: &str, on: bool, why: &str| {
            let r = ui.add_enabled(on, egui::Button::new(label).shortcut_text(key));
            if on {
                r.clicked()
            } else {
                r.on_disabled_hover_text(why).clicked()
            }
        };
        if item(
            ui,
            "Copy notes",
            "",
            has,
            "This channel has no notes in this pattern",
        ) {
            self.copy_channel_notes(id, false);
            ui.close_menu();
        }
        if item(
            ui,
            "Cut notes",
            "",
            has,
            "This channel has no notes in this pattern",
        ) {
            self.copy_channel_notes(id, true);
            ui.close_menu();
        }
        if item(
            ui,
            "Paste notes (replace)",
            "",
            clip,
            "Copy notes first (here or in a piano roll)",
        ) {
            self.paste_channel_notes(id);
            ui.close_menu();
        }
        if item(
            ui,
            "Duplicate notes to the right",
            "",
            has,
            "This channel has no notes in this pattern",
        ) {
            self.duplicate_channel_notes(id);
            ui.close_menu();
        }
        if item(
            ui,
            "Clear notes",
            "",
            has,
            "This channel has no notes in this pattern",
        ) {
            self.clear_channel_notes(id);
            ui.close_menu();
        }
        if ui
            .button("Clone channel")
            .on_hover_text("A copy of the channel with its notes, below it")
            .clicked()
        {
            self.rack_sel = vec![id];
            self.clone_channels();
            ui.close_menu();
        }
    }

    pub(crate) fn copy_channel_notes(&mut self, id: u64, cut: bool) {
        let notes = self.channel_notes(id);
        if notes.is_empty() {
            return;
        }
        let name = self.instrument_name(id);
        self.note_clip = notes.clone();
        if cut {
            self.checkpoint();
            self.pattern_mut().notes.retain(|n| n.instrument != id);
            self.roll_sel.clear();
            self.publish();
            self.say(Tone::Quiet, format!("{} note(s) cut from {name}. Paste them on any channel or in a piano roll. Ctrl+Z undoes.", notes.len()));
        } else {
            self.say(
                Tone::Quiet,
                format!(
                    "{} note(s) of {name} copied. Paste them on any channel or in a piano roll.",
                    notes.len()
                ),
            );
        }
    }

    /// The clipboard becomes this channel's notes, at the times they were copied from.
    pub(crate) fn paste_channel_notes(&mut self, id: u64) {
        if self.note_clip.is_empty() {
            return;
        }
        let name = self.instrument_name(id);
        self.checkpoint();
        let pid = self.selected;
        self.pattern_mut().notes.retain(|n| n.instrument != id);
        for n in self.note_clip.clone() {
            self.song.add_note(
                pid,
                Note {
                    instrument: id,
                    ..n
                },
            );
        }
        self.fit_pattern();
        self.roll_sel.clear();
        self.publish();
        self.say(
            Tone::Quiet,
            format!(
                "{} note(s) pasted into {name}. Ctrl+Z undoes.",
                self.note_clip.len()
            ),
        );
    }

    /// The channel's notes again right after themselves, from the next whole bar.
    pub(crate) fn duplicate_channel_notes(&mut self, id: u64) {
        use keyframexl::sequence::BAR;
        let notes = self.channel_notes(id);
        let Some(end) = notes.iter().map(|n| n.end()).max() else {
            return;
        };
        let span = ((end + BAR - 1) / BAR).max(1) * BAR;
        let name = self.instrument_name(id);
        self.checkpoint();
        let pid = self.selected;
        for n in &notes {
            self.song.add_note(
                pid,
                Note {
                    start: n.start + span,
                    ..*n
                },
            );
        }
        self.fit_pattern();
        self.publish();
        self.say(
            Tone::Quiet,
            format!(
                "{name}: {} note(s) repeated from bar {}. Ctrl+Z undoes.",
                notes.len(),
                span / BAR + 1
            ),
        );
    }

    pub(crate) fn clear_channel_notes(&mut self, id: u64) {
        let n = self.channel_notes(id).len();
        if n == 0 {
            return;
        }
        let name = self.instrument_name(id);
        self.checkpoint();
        self.pattern_mut().notes.retain(|x| x.instrument != id);
        self.roll_sel.clear();
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name}: {n} note(s) removed. Ctrl+Z undoes."),
        );
    }

    /// Humanize dialog for one channel: amount slider, seed, Apply/Cancel.
    fn rack_humanize_dialog(&mut self, ui: &mut Ui) {
        let key = egui::Id::new("rack-humanize");
        let mut st: Option<(u64, i64, i64, u64)> = ui.ctx().data(|d| d.get_temp(key));
        let Some((id, ref mut amount, ref mut timing, ref mut seed)) = st else {
            return;
        };
        let name = self.instrument_name(id);
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Humanize steps")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 140.0))
            .show(ui.ctx(), |ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                ui.label(egui::RichText::new("Shifts velocity (and a touch of timing) by random amounts. The same seed gives the same result.").font(t::sans(12.0)).color(t::faint()));
                egui::Grid::new("rack-humanize-grid").num_columns(2).spacing(vec2(14.0, 8.0)).show(ui, |ui| {
                    ui.label(egui::RichText::new("Velocity ±").font(t::sans(13.0)).color(t::text()));
                    kit::drag_value(ui, egui::DragValue::new(amount).range(0..=64));
                    ui.end_row();
                    ui.label(egui::RichText::new("Timing ±").font(t::sans(13.0)).color(t::text()));
                    kit::drag_value(ui, egui::DragValue::new(timing).range(0..=12).custom_formatter(|x, _| format!("{x:.0} ticks")).custom_parser(|s| s.trim().trim_end_matches("ticks").trim().parse::<f64>().ok()));
                    ui.end_row();
                    ui.label(egui::RichText::new("Seed").font(t::sans(13.0)).color(t::text()));
                    kit::drag_value(ui, egui::DragValue::new(seed));
                    ui.end_row();
                });
                ui.label(egui::RichText::new(format!("Applies to the steps of {name}. Ctrl+Z undoes.")).font(t::sans(12.0)).color(t::faint()));
                ui.horizontal(|ui| {
                    if kit::button(ui, "Apply", kit::Kind::Primary, true).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = true;
                    }
                    if kit::button(ui, "Cancel", kit::Kind::Normal, true).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        if cancel || !open {
            ui.ctx()
                .data_mut(|d| d.remove::<Option<(u64, i64, i64, u64)>>(key));
            return;
        }
        ui.ctx()
            .data_mut(|d| d.insert_temp(key, Some((id, *amount, *timing, *seed))));
        if apply {
            let chan = self.step_notes(id);
            if chan.is_empty() || self.step_target(id).is_none() {
                self.no_steps(id);
            } else {
                self.checkpoint();
                let mut chan = chan;
                keyframexl::step_ops::humanize(
                    &mut chan,
                    (*amount).clamp(0, 127) as u8,
                    *timing,
                    *seed,
                );
                let edit: std::collections::HashMap<u64, (Tick, u8)> = chan
                    .into_iter()
                    .map(|n| (n.id, (n.start, n.velocity)))
                    .collect();
                for n in self
                    .pattern_mut()
                    .notes
                    .iter_mut()
                    .filter(|n| n.instrument == id)
                {
                    if let Some((s, v)) = edit.get(&n.id) {
                        n.start = *s;
                        n.velocity = *v;
                    }
                }
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!("Humanized {name} (±{amount} velocity). Ctrl+Z undoes."),
                );
            }
            ui.ctx()
                .data_mut(|d| d.remove::<Option<(u64, i64, i64, u64)>>(key));
        }
    }

    pub(super) fn rack(&mut self, ui: &mut Ui) {
        let idx = self.pattern_index(self.selected) + 1;
        let sub = format!("{idx:02}  {}", self.pattern().name);
        self.rack_header(ui, &sub);
        egui::ScrollArea::vertical()
            .id_salt("rack-rows")
            .auto_shrink([false, true])
            .drag_to_scroll(false)
            .show(ui, |ui| {
                let area = ui.available_rect_before_wrap();
                let mut pane = area;
                pane.min.x += NAME_W.min(pane.width() * 0.78);
                let mut pins =
                    ui.new_child(egui::UiBuilder::new().id_salt("rack-pins").max_rect(area));
                let output = ui.scope_builder(egui::UiBuilder::new().max_rect(pane), |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("rack-steps")
                        .min_scrolled_width(0.0)
                        .auto_shrink([false, true])
                        .drag_to_scroll(false)
                        .show(ui, |ui| {
                            self.rack_grid(ui, &mut pins, area.left(), pane.left(), area.right())
                        })
                });
                ui.advance_cursor_after_rect(output.response.rect);
                self.rack_add_channel(ui);
            });
        if self.focus == Some(Tab::Rack) {
            self.rack_keys(ui);
        }
        self.rack_humanize_dialog(ui);
    }

    fn rack_grid(&mut self, ui: &mut Ui, pins: &mut Ui, left: f32, fixed_grid_l: f32, right: f32) {
        let pat = self.pattern().clone();
        let steps = pat.step_count();
        let beats = steps.div_ceil(4).max(1);
        let channels: Vec<(u64, String, bool, bool)> = self
            .song
            .instruments
            .iter()
            .map(|i| (i.id, i.name.clone(), i.muted, i.is_sampler()))
            .collect();
        let graph_h = if self.rack_graph { GRAPH_H + 10.0 } else { 0.0 };
        let need = 22.0 + STEP_NUM_H + LED_ROW + channels.len() as f32 * ROW_H + graph_h;
        let min_tray = 4.0 * MIN_PAD_W + 4.0 * PAD_GAP;
        let min_w = beats as f32 * min_tray + (beats - 1) as f32 * TRAY_GAP + 24.0;
        let (body, _) =
            ui.allocate_exact_size(vec2(ui.available_width().max(min_w), need), Sense::hover());
        let p = ui.painter().clone();

        let grid_l = body.left();
        let grid_w = body.right() - grid_l;
        let max_tray = 4.0 * MAX_PAD_W + 4.0 * PAD_GAP;
        let tray_w =
            ((grid_w - (beats - 1) as f32 * TRAY_GAP) / beats as f32).clamp(min_tray, max_tray);
        let pad_w = (tray_w - 4.0 * PAD_GAP) / 4.0;
        let pad_x = |i: usize| {
            grid_l
                + (i / 4) as f32 * (tray_w + TRAY_GAP)
                + PAD_GAP / 2.0
                + (i % 4) as f32 * (pad_w + PAD_GAP)
        };
        let grid_r = grid_l + beats as f32 * tray_w + (beats - 1) as f32 * TRAY_GAP;

        // Only what is on screen is drawn and hit-tested: long patterns
        // (an imported song can be 2 000+ steps) stay fast.
        let clip = ui.clip_rect();
        let beat_at = |x: f32| ((x - grid_l) / (tray_w + TRAY_GAP)).floor().max(0.0) as usize;
        let beat_range = beat_at(clip.left())..(beat_at(clip.right()) + 1).min(beats);
        let step_range = (beat_range.start * 4).min(steps)..(beat_range.end * 4).min(steps);
        // One pass over the notes: which channels are step channels, and the note on each step.
        let mut step_ok: std::collections::HashMap<u64, bool> = std::collections::HashMap::new();
        let mut step_map: std::collections::HashMap<(u64, usize), usize> =
            std::collections::HashMap::new();
        for (k, n) in pat.notes.iter().enumerate() {
            let fits =
                n.start % keyframexl::sequence::STEP == 0 && n.length == keyframexl::sequence::STEP;
            *step_ok.entry(n.instrument).or_insert(true) &= fits;
            if n.start % keyframexl::sequence::STEP == 0 {
                step_map
                    .entry((
                        n.instrument,
                        (n.start / keyframexl::sequence::STEP) as usize,
                    ))
                    .or_insert(k);
            }
        }
        let is_step = |id: u64| {
            step_ok.get(&id).copied().unwrap_or(true)
                && !pat.slides.iter().any(|s| s.instrument == id)
        };

        // Beat captions and trays.
        let y0 = body.top();
        let tray_top = y0 + 18.0;
        let tray_bottom = tray_top + STEP_NUM_H + LED_ROW + channels.len() as f32 * ROW_H + 4.0;
        for beat in beat_range.clone() {
            let x = grid_l + beat as f32 * (tray_w + TRAY_GAP);
            let label = if beats > 4 {
                format!("{}.{}", beat / 4 + 1, beat % 4 + 1)
            } else {
                format!("beat {}", beat + 1)
            };
            kit::caps(
                &p,
                pos2(x + 4.0, y0 + 8.0),
                Align2::LEFT_CENTER,
                &label,
                11.0,
                t::faint(),
            );
            p.rect_filled(
                Rect::from_min_max(pos2(x, tray_top), pos2(x + tray_w, tray_bottom)),
                0.0,
                if beat % 2 == 0 {
                    t::sunken()
                } else {
                    t::panel_bottom()
                },
            );
        }
        let playing = self.playing_step();
        // Step numbers: 1..=step count, 12 px mono. First step of each beat brighter.
        for i in step_range.clone() {
            let c = pos2(pad_x(i) + pad_w / 2.0, tray_top + STEP_NUM_H / 2.0 + 1.0);
            let ink = if i % 4 == 0 { t::text() } else { t::muted() };
            p.text(
                c,
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                t::mono(12.0),
                ink,
            );
        }
        for i in step_range.clone() {
            let c = pos2(
                pad_x(i) + pad_w / 2.0,
                tray_top + STEP_NUM_H + LED_ROW / 2.0 + 1.0,
            );
            kit::led(&p, c, 3.0, (playing == Some(i)).then_some(t::text()));
        }

        let shift = ui.input(|i| i.modifiers.shift);
        let mut step_click: Option<(u64, usize, bool)> = None;
        let stroke_id = ui.id().with("rack-stroke");
        let (primary, secondary, pp, sp, pos) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.pointer.interact_pos(),
            )
        });
        // The frame a button comes up still paints the last stretch of the path.
        let (pr, sr_up, origin) = ui.input(|i| {
            (
                i.pointer.primary_released(),
                i.pointer.secondary_released(),
                i.pointer.press_origin(),
            )
        });
        let mut stroke: Option<PadStroke> = ui.data(|d| d.get_temp(stroke_id)).flatten();
        // Step pads as drawn, for the stroke's hit test after the rows.
        let mut pads: Vec<(u64, usize, Rect)> = Vec::new();
        let fixed_painter = pins.painter().clone();
        let mut channels_ui = pins.new_child(egui::UiBuilder::new().id_salt("channel-controls"));
        let mut channel_clip = channels_ui.clip_rect();
        channel_clip.max.x = channel_clip.max.x.min(fixed_grid_l);
        channels_ui.set_clip_rect(channel_clip);
        let channel_painter = channels_ui.painter().clone();
        let mut move_channel = None;
        for (n, (id, name, muted, _)) in channels.iter().enumerate() {
            let id = *id;
            let y = tray_top + STEP_NUM_H + LED_ROW + n as f32 * ROW_H + (ROW_H - PAD_H) / 2.0;
            let mid = y + PAD_H / 2.0;
            let audible = !*muted;
            let soloed = self.song.is_solo(id);
            let row_rect = Rect::from_min_max(pos2(left, y), pos2(right, y + PAD_H));
            // The whole row slot (pads, name, the gaps around) takes drops.
            let slot = Rect::from_min_max(
                pos2(left, y - (ROW_H - PAD_H) / 2.0),
                pos2(right, y - (ROW_H - PAD_H) / 2.0 + ROW_H),
            );
            let row_drop =
                pins.interact(slot, ui.id().with(("plugin-row-drop", id)), Sense::hover());
            // A MIDI clip dropped from outside (a plugin's groove) lands here too.
            let shown = slot.intersect(pins.clip_rect());
            if shown.is_positive() {
                self.midi_zones.push((shown, id));
            }
            if row_drop.dnd_hover_payload::<ChannelDrag>().is_some()
                && let Some(pos) = ui.ctx().pointer_interact_pos()
            {
                let after = pos.y >= slot.center().y;
                let y = if after { slot.bottom() } else { slot.top() };
                fixed_painter.hline(slot.x_range(), y, Stroke::new(2.0_f32, t::focus()));
                if let Some(drag) = row_drop.dnd_release_payload::<ChannelDrag>() {
                    move_channel = Some((drag.0, id, after));
                }
            }
            // A .dwp, sample or plugin dragged here (FL: drop on the rack): the
            // row's middle replaces this channel's sound, its top or bottom
            // quarter inserts a new channel before or after it.
            let dragging = row_drop
                .dnd_hover_payload::<super::files::FileDrag>()
                .is_some()
                || row_drop
                    .dnd_hover_payload::<super::browser::PluginDrag>()
                    .is_some();
            let place = ui.input(|i| i.pointer.hover_pos()).map(|q| {
                let f = (q.y - slot.top()) / slot.height();
                if f < 0.25 {
                    Some(n)
                } else if f > 0.75 {
                    Some(n + 1)
                } else {
                    None
                }
            });
            if dragging {
                match place.flatten() {
                    Some(at) => {
                        let ly = if at == n { slot.top() } else { slot.bottom() };
                        fixed_painter.line_segment(
                            [pos2(left + 4.0, ly), pos2(right, ly)],
                            Stroke::new(2.5_f32, t::text()),
                        );
                        fixed_painter.circle_filled(pos2(left + 4.0, ly), 4.0, t::text());
                    }
                    None => {
                        fixed_painter.rect_stroke(
                            row_rect.expand(1.0),
                            6.0,
                            Stroke::new(1.5_f32, t::text()),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            }
            let insert_at = place.flatten();
            if let Some(at) = insert_at
                && let Some(drag) = row_drop.dnd_release_payload::<super::files::FileDrag>()
            {
                self.insert_dropped_channel(at, |app| app.load_file(drag.0.clone(), true));
            } else if let Some(at) = insert_at
                && let Some(drag) = row_drop.dnd_release_payload::<super::browser::PluginDrag>()
            {
                self.insert_dropped_channel(at, |app| app.add_plugin_channel(&drag.0));
            } else if let Some(drag) = row_drop.dnd_release_payload::<super::files::FileDrag>() {
                let path = drag.0.clone();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if ext == "kfxl" || ext == "flp" {
                    self.request(super::Pending::Open(Some(path)));
                } else if ext == "mid" || ext == "midi" {
                    self.drop_midi_notes(&path, id);
                } else if keyframexl::preset::Kind::of_path(&path).is_some() {
                    self.open_preset_file(&path);
                } else {
                    self.drop_file_on_channel(id, path);
                }
            }
            if let Some(drag) = row_drop.dnd_release_payload::<super::browser::PluginDrag>() {
                let plugin = &drag.0;
                self.collect_plugin_states();
                // FL renames the channel to the plugin unless the user renamed it.
                let custom = self
                    .song
                    .instrument(id)
                    .is_some_and(|inst| is_custom_channel_name(&inst.name, &inst.kind));
                self.checkpoint();
                let pid = self.song.next_plugin_id();
                let reference = keyframexl::sequence::PluginRef {
                    id: pid,
                    format: crate::plugins::format_of(&plugin.path).into(),
                    path: plugin.path.display().to_string(),
                    name: plugin.name.clone(),
                    vendor: plugin.vendor.clone(),
                    state: String::new(),
                    outs: vec![],
                };
                // `free_name` sees the channel's own old name as taken, so keep
                // the bare plugin name when nothing else uses it.
                let renamed = if custom {
                    name.clone()
                } else if self
                    .song
                    .instruments
                    .iter()
                    .any(|i| i.id != id && i.name == plugin.name)
                {
                    self.song.free_name(&plugin.name)
                } else {
                    plugin.name.clone()
                };
                if let Some(inst) = self.song.instruments.iter_mut().find(|i| i.id == id) {
                    inst.kind = keyframexl::sequence::InstrumentKind::Plugin { plugin: reference };
                    if !custom {
                        inst.name = renamed.clone();
                    }
                }
                self.publish();
                self.plugins_frame();
                self.say(
                    Tone::Quiet,
                    format!(
                        "{} replaced the instrument on {renamed}. Ctrl+Z undoes.",
                        plugin.name
                    ),
                );
            }
            self.channel_strip(
                &mut channels_ui,
                &channel_painter,
                left,
                fixed_grid_l - 26.0,
                mid,
                id,
                name,
                n,
                audible,
                soloed,
            );
            // Selector (FL: the bar at the row's right end). Click selects, Ctrl toggles, Shift extends.
            // Hit area is 24x24; the painted bar stays small and centred in it.
            let sel_hit = Rect::from_center_size(pos2(fixed_grid_l - 14.0, mid), vec2(24.0, 24.0));
            let sr = channels_ui.interact(
                sel_hit,
                channels_ui.id().with(("select", id)),
                Sense::click(),
            );
            let on = self.rack_sel.contains(&id);
            let hover = kit::hover_amount(&channels_ui, &sr);
            let face = if on {
                t::text()
            } else {
                kit::mix(t::sunken(), t::strong(), hover)
            };
            channel_painter.rect_filled(sel_hit.shrink2(vec2(7.0, 2.0)), 3.0, face);
            if sr
                .on_hover_text(format!("Select {name}  · Ctrl toggles · Shift extends"))
                .clicked()
            {
                let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                self.select_channel(id, ctrl, shift);
            }

            if is_step(id) {
                for i in step_range.clone() {
                    let rect = Rect::from_min_size(pos2(pad_x(i), y), vec2(pad_w, PAD_H));
                    let resp =
                        ui.interact(rect, ui.id().with(("pad", id, i)), Sense::click_and_drag());
                    #[cfg(test)]
                    if n == 0 && i == 0 {
                        self.probe.pad = Some(rect);
                    }
                    let note = step_map.get(&(id, i)).map(|k| pat.notes[*k]);
                    let vel = note.map(|n| n.velocity).unwrap_or(0);
                    // A muted note (e.g. from an old project's solo) shows like a muted channel.
                    let heard = audible && !note.is_some_and(|n| n.muted);
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Checkbox,
                            true,
                            vel > 0,
                            format!("{name} step {}", i + 1),
                        )
                    });
                    self.paint_pad(ui, rect, &resp, i, vel, heard, playing == Some(i));
                    let shown = rect.intersect(ui.clip_rect());
                    pads.push((id, i, shown));
                    // The press point, not where the pointer got to by this frame.
                    // Never trigger through disabled/background UI or modal overlays (e.g. Welcome Screen).
                    if ui.is_enabled()
                        && !self.welcome_open
                        && origin.is_some_and(|p| shown.contains(p))
                        && (pp || sp)
                        && !(shift && pp)
                    {
                        // FL: one value per stroke, from the pad it starts on.
                        stroke = Some(PadStroke {
                            right: sp,
                            fill: if sp { false } else { vel == 0 },
                            visited: Vec::new(),
                            last: origin,
                            changed: false,
                        });
                    }
                    if shift && resp.clicked() {
                        step_click = Some((id, i, true));
                    }
                }
            } else {
                // Notes off the step grid: a preview; the piano roll edits them (FL).
                let rect = Rect::from_min_max(pos2(grid_l, y), pos2(grid_r, y + PAD_H));
                let resp = ui.interact(rect, ui.id().with(("preview", id)), Sense::click());
                let hot = resp.hovered();
                let well = kit::sunken(&p, rect, 8.0);
                if hot {
                    p.rect_filled(well, 7.0, t::raised().gamma_multiply(0.35));
                }
                self.paint_note_strip(&p, well.shrink2(vec2(4.0, 5.0)), id, &pat, hot);
                let resp = resp.on_hover_text(format!("{name} plays notes off the step grid. Click to edit them in the piano roll, right-click to copy, paste or duplicate them."));
                if resp.clicked() {
                    self.open_roll(id);
                }
                resp.context_menu(|ui| {
                    kit::menu_style(ui);
                    if ui.button("Piano roll").clicked() {
                        self.open_roll(id);
                        ui.close_menu();
                    }
                    ui.separator();
                    self.channel_notes_menu(ui, id);
                });
            }
        }

        // Graph editor (FL Ctrl+K): velocity of the selected channel's steps.
        if self.rack_graph {
            let lane = Rect::from_min_max(
                pos2(grid_l, tray_bottom + 8.0),
                pos2(grid_r, tray_bottom + 8.0 + GRAPH_H),
            );
            self.graph_editor(ui, &p, lane, &pad_x, pad_w, steps);
        }

        if let Some((from, to, after)) = move_channel {
            self.reorder_channel(from, to, after);
        }

        // The stroke covers the path since last frame, so a fast move skips no pad.
        if let Some(st) = &mut stroke
            && (if st.right {
                secondary || sr_up
            } else {
                primary || pr
            })
            && let Some(now) = pos
        {
            let from = st.last.unwrap_or(now);
            let n = ((now - from).length() / 4.0).ceil().max(1.0) as usize;
            let mut hits: Vec<(u64, usize)> = Vec::new();
            for k in 0..=n {
                let q = from + (now - from) * (k as f32 / n as f32);
                if let Some((id, i, _)) = pads.iter().find(|(_, _, r)| r.contains(q))
                    && !st.visited.contains(&(*id, *i))
                {
                    st.visited.push((*id, *i));
                    hits.push((*id, *i));
                }
            }
            st.last = Some(now);
            if !hits.is_empty() {
                // The pads were drawn before this change: show it next frame.
                ui.ctx().request_repaint();
            }
            let fill = st.fill;
            for (id, step) in hits {
                let at = self.pattern().step_note(id, step);
                if at.is_some() == fill {
                    continue;
                }
                // One undo step per stroke, made by its first real change.
                if !st.changed {
                    self.checkpoint();
                    st.changed = true;
                }
                if let Some(i) = at {
                    self.pattern_mut().notes.remove(i);
                } else {
                    let pid = self.selected;
                    self.song.add_note(
                        pid,
                        Note::new(id, step as i64 * STEP, STEP, STEP_PITCH, 100),
                    );
                }
                self.publish();
            }
        }
        if !primary && !secondary {
            stroke = None;
        }
        ui.data_mut(|d| d.insert_temp(stroke_id, stroke));
        if let Some((id, step, shift)) = step_click {
            self.toggle_step(id, step, shift);
        }
        let drop_rect = Rect::from_min_max(pos2(left, body.top()), pos2(right, body.bottom()));
        self.rack_drop(pins, drop_rect);
    }

    fn rack_add_channel(&mut self, ui: &mut Ui) {
        let (add, _) = ui.allocate_exact_size(vec2(150.0, kit::CONTROL_H), Sense::hover());
        let r = ui.interact(add, ui.id().with("add-channel"), Sense::click());
        kit::paint_button(
            ui,
            add,
            &r,
            kit::Kind::Ghost,
            true,
            "Add channel",
            |p, face, ink| {
                kit::icon_plus(p, pos2(face.left() + 16.0, face.center().y), 10.0, ink);
                p.text(
                    pos2(face.left() + 30.0, face.center().y),
                    Align2::LEFT_CENTER,
                    "Add channel",
                    t::sans(14.0),
                    ink,
                );
            },
        );
        #[cfg(test)]
        {
            self.probe.add_channel = Some(add);
        }
        let r = r.on_hover_text("Add a channel");
        let add_popup = ui.make_persistent_id("add-channel-menu");
        if r.clicked() {
            ui.memory_mut(|m| m.toggle_popup(add_popup));
        }
        kit::popup_menu(ui, add_popup, &r, 180.0, |ui| self.add_channel_menu(ui));

        let drop_rect = Rect::from_min_max(
            add.min,
            pos2(ui.max_rect().right(), ui.clip_rect().bottom()),
        );
        self.rack_drop(ui, drop_rect);
    }

    fn rack_drop(&mut self, ui: &Ui, rect: Rect) {
        let area = ui.interact(rect, ui.id().with("rack-drop"), Sense::hover());
        if let Some(drag) = area.dnd_release_payload::<super::files::FileDrag>() {
            self.load_file(drag.0.clone(), true);
        }
        if let Some(drag) = area.dnd_release_payload::<super::browser::PluginDrag>() {
            self.add_plugin_channel(&drag.0);
        }
    }

    fn reorder_channel(&mut self, from: u64, to: u64, after: bool) {
        let find = |id| self.song.instruments.iter().position(|inst| inst.id == id);
        let (Some(source), Some(target)) = (find(from), find(to)) else {
            return;
        };
        let boundary = target + usize::from(after);
        let destination = boundary - usize::from(source < boundary);
        if source != destination {
            self.checkpoint();
            let channel = self.song.instruments.remove(source);
            self.song.instruments.insert(destination, channel);
            self.publish();
        }
    }

    /// Make a channel with `add` (a dropped file or plugin; it lands last)
    /// and move it to rack position `at`.
    pub(crate) fn insert_dropped_channel(&mut self, at: usize, add: impl FnOnce(&mut Self)) {
        let before: Vec<u64> = self.song.instruments.iter().map(|i| i.id).collect();
        add(self);
        let Some(new) = self
            .song
            .instruments
            .iter()
            .position(|i| !before.contains(&i.id))
        else {
            return;
        };
        let inst = self.song.instruments.remove(new);
        let at = at.min(self.song.instruments.len());
        let name = inst.name.clone();
        self.song.instruments.insert(at, inst);
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name} added as channel {}. Ctrl+Z undoes.", at + 1),
        );
    }

    fn set_channel_insert(&mut self, id: u64, to: u16) {
        self.step_channel_insert(id, to, true);
    }

    /// `fresh`: the first change of a gesture, which makes the undo step.
    fn step_channel_insert(&mut self, id: u64, to: u16, fresh: bool) {
        if self.song.instrument(id).is_some_and(|i| i.mixer == to) {
            return;
        }
        if fresh {
            self.checkpoint();
        }
        if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == id) {
            i.mixer = to;
        }
        self.publish();
        let (name, target) = (self.instrument_name(id), self.mixer_name(to as usize));
        self.say(
            Tone::Quiet,
            format!("{name} plays through {target}. Ctrl+Z undoes."),
        );
    }

    fn select_channel(&mut self, id: u64, toggle: bool, extend: bool) {
        let ids: Vec<u64> = self.song.instruments.iter().map(|i| i.id).collect();
        if extend && let Some(&last) = self.rack_sel.last() {
            let (a, b) = (
                ids.iter().position(|x| *x == last).unwrap_or(0),
                ids.iter().position(|x| *x == id).unwrap_or(0),
            );
            for x in &ids[a.min(b)..=a.max(b)] {
                if !self.rack_sel.contains(x) {
                    self.rack_sel.push(*x);
                }
            }
        } else if toggle {
            if let Some(i) = self.rack_sel.iter().position(|x| *x == id) {
                self.rack_sel.remove(i);
            } else {
                self.rack_sel.push(id);
            }
        } else {
            self.rack_sel = vec![id];
        }
        self.rack_sel.retain(|x| ids.contains(x));
    }

    /// Move the selected channels one row up (-1) or down (+1).
    pub(crate) fn move_channels(&mut self, dir: i32) {
        let sel = self.rack_sel.clone();
        if sel.is_empty() {
            return;
        }
        let n = self.song.instruments.len();
        let pos: Vec<usize> = (0..n)
            .filter(|i| sel.contains(&self.song.instruments[*i].id))
            .collect();
        if (dir < 0 && pos.first() == Some(&0)) || (dir > 0 && pos.last() == Some(&(n - 1))) {
            return;
        }
        self.checkpoint();
        let order: Vec<usize> = if dir < 0 {
            pos
        } else {
            pos.into_iter().rev().collect()
        };
        for i in order {
            let j = (i as i32 + dir) as usize;
            self.song.instruments.swap(i, j);
        }
        self.publish();
    }

    /// FL Alt+C: copy the selected channels with their notes in every pattern.
    pub(crate) fn clone_channels(&mut self) {
        let sel = self.rack_sel.clone();
        if sel.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new_ids = Vec::new();
        for id in sel {
            let Some(at) = self.song.instruments.iter().position(|i| i.id == id) else {
                continue;
            };
            let mut inst = self.song.instruments[at].clone();
            let nid = self
                .song
                .instruments
                .iter()
                .map(|i| i.id)
                .max()
                .unwrap_or(0)
                + 1;
            inst.id = nid;
            inst.name = self.song.free_name(&inst.name);
            self.song.instruments.insert(at + 1, inst);
            let pids: Vec<u64> = self.song.patterns.iter().map(|p| p.id).collect();
            for pid in pids {
                let notes: Vec<Note> = self
                    .song
                    .pattern(pid)
                    .map(|p| {
                        p.notes
                            .iter()
                            .filter(|n| n.instrument == id)
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                for n in notes {
                    self.song.add_note(
                        pid,
                        Note {
                            instrument: nid,
                            ..n
                        },
                    );
                }
                if let Some(p) = self.song.pattern_mut(pid) {
                    let slides: Vec<_> = p
                        .slides
                        .iter()
                        .filter(|s| s.instrument == id)
                        .copied()
                        .collect();
                    p.slides
                        .extend(slides.into_iter().map(|s| keyframexl::sequence::Glide {
                            instrument: nid,
                            ..s
                        }));
                }
            }
            new_ids.push(nid);
        }
        self.rack_sel = new_ids;
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{} channel(s) cloned. Ctrl+Z undoes.", self.rack_sel.len()),
        );
    }

    pub(crate) fn delete_channels(&mut self) {
        let sel = std::mem::take(&mut self.rack_sel);
        if sel.is_empty() {
            return;
        }
        self.checkpoint();
        for id in &sel {
            let _ = self.song.remove_instrument(*id);
        }
        self.publish();
        self.say(
            Tone::Quiet,
            format!(
                "{} channel(s) deleted with their notes. Ctrl+Z undoes.",
                sel.len()
            ),
        );
    }

    fn rack_keys(&mut self, ui: &Ui) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }
        let m = ui.input(|i| i.modifiers);
        let pressed = |k: egui::Key| ui.input(|i| i.key_pressed(k));
        if m.alt && pressed(egui::Key::ArrowUp) {
            self.move_channels(-1);
        }
        if m.alt && pressed(egui::Key::ArrowDown) {
            self.move_channels(1);
        }
        if m.alt && pressed(egui::Key::C) {
            self.clone_channels();
        }
        if m.alt && pressed(egui::Key::Delete) {
            self.delete_channels();
        }
        if m.command && pressed(egui::Key::K) {
            self.rack_graph = !self.rack_graph;
        }
        if m.command && pressed(egui::Key::L) {
            match self.free_insert() {
                Some(to) => self.route_selected_channels(to),
                None => self.say(
                    Tone::Bad,
                    "Every insert already has channels. Pick one in the box left of the name.",
                ),
            }
        }
    }

    /// Velocity stems over the steps of the first selected channel (else the first channel).
    #[allow(clippy::too_many_arguments)]
    fn graph_editor(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        lane: Rect,
        pad_x: &dyn Fn(usize) -> f32,
        pad_w: f32,
        steps: usize,
    ) {
        let Some(id) = self.rack_sel.first().copied().or(self
            .song
            .instruments
            .first()
            .map(|i| i.id))
        else {
            return;
        };
        let well = kit::sunken(p, lane, 8.0);
        let area = well.shrink2(vec2(0.0, 8.0));
        let name = self.instrument_name(id);
        kit::caps(
            p,
            pos2(lane.left() - 8.0, lane.top() + 10.0),
            Align2::RIGHT_CENTER,
            &format!("vel · {name}"),
            11.0,
            t::faint(),
        );
        let pat = self.pattern().clone();
        for i in 0..steps {
            let Some(k) = pat.step_note(id, i) else {
                continue;
            };
            let n = pat.notes[k];
            let x = pad_x(i) + pad_w / 2.0;
            let y = area.bottom() - n.velocity as f32 / 127.0 * area.height();
            p.line_segment(
                [pos2(x, area.bottom()), pos2(x, y)],
                Stroke::new(2.0_f32, t::brand()),
            );
            p.circle_filled(pos2(x, y), 4.0, t::brand());
        }
        let r = ui.interact(area, ui.id().with("graph-editor"), Sense::click_and_drag());
        let Some(pos) = r.interact_pointer_pos() else {
            return;
        };
        let Some(step) = (0..steps).min_by(|a, b| {
            (pad_x(*a) + pad_w / 2.0 - pos.x)
                .abs()
                .total_cmp(&(pad_x(*b) + pad_w / 2.0 - pos.x).abs())
        }) else {
            return;
        };
        let Some(k) = pat.step_note(id, step) else {
            return;
        };
        if r.drag_started() || r.clicked() {
            self.checkpoint();
        }
        if r.clicked() || r.dragged() {
            let alt = ui.input(|i| i.modifiers.alt);
            let v = if alt {
                100
            } else {
                (((area.bottom() - pos.y) / area.height()) * 127.0).clamp(1.0, 127.0) as u8
            };
            self.pattern_mut().notes[k].velocity = v;
            self.publish();
            self.say(
                Tone::Quiet,
                format!("{name} step {:02}: velocity {v}. Ctrl+Z undoes.", step + 1),
            );
        }
    }

    /// FL channel row head: mute LED (Ctrl/right-click: solo), pan, volume,
    /// mixer insert, name button.
    #[allow(clippy::too_many_arguments)]
    fn channel_strip(
        &mut self,
        ui: &mut Ui,
        p: &egui::Painter,
        left: f32,
        right: f32,
        mid: f32,
        id: u64,
        name: &str,
        n: usize,
        audible: bool,
        soloed: bool,
    ) {
        // Mute LED: filled while the channel plays, hollow ring while muted
        // (shape as well as colour, so colour-blind users can tell them apart).
        // Hit area is 24x24; the painted LED stays small and centred in it.
        let led = Rect::from_center_size(pos2(left + 12.0, mid), vec2(24.0, 24.0));
        let r = ui.interact(led, ui.id().with(("mute", id)), Sense::click());
        r.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                true,
                audible,
                format!("{name} on"),
            )
        });
        let hover = kit::hover_amount(ui, &r);
        if audible {
            kit::led(
                p,
                led.center(),
                4.5,
                Some(if soloed { t::enc_blue() } else { t::ok() }),
            );
        } else {
            p.circle_stroke(led.center(), 4.5, Stroke::new(1.5_f32, t::muted()));
        }
        if hover > 0.0 {
            p.circle_stroke(
                led.center(),
                7.5,
                Stroke::new(1.0_f32, t::strong().gamma_multiply(hover)),
            );
        }
        if kit::show_focus(ui, &r) {
            kit::focus_ring(p, led, 6.0);
        }
        let key = if n < 3 {
            format!(" · Ctrl+{}", n + 1)
        } else {
            String::new()
        };
        let r = r.on_hover_text(format!(
            "{name}: click mutes{key} · right-click or Ctrl+click solos"
        ));
        let ctrl = ui.input(|i| i.modifiers.command);
        if r.secondary_clicked() || (r.clicked() && ctrl) {
            self.toggle_solo(id);
        } else if r.clicked() {
            self.toggle_mute(id);
        }

        // Pan and volume (FL channel knobs), one undo step per gesture.
        // Each hit area is 24x24; the painted encoder stays small and centred.
        let Some(inst) = self.song.instrument(id).cloned() else {
            return;
        };
        let mut x = left + 24.0;
        // Volume turns along the shared taper: the knob holds its travel position.
        let vol_fmt = |pos: f64| {
            keyframexl::sequence::gain_db_text(keyframexl::sequence::volume_gain(pos as f32))
        };
        let specs: [(&str, f64, f64, f64, f64, &dyn Fn(f64) -> String, &str); 2] = [
            (
                "Pan",
                inst.pan as f64,
                -1.0,
                1.0,
                0.0,
                &|v: f64| {
                    if v.abs() < 0.005 {
                        "C".into()
                    } else if v < 0.0 {
                        format!("L{:.0}", -v * 100.0)
                    } else {
                        format!("R{:.0}", v * 100.0)
                    }
                },
                "Channel pan",
            ),
            (
                "Vol",
                keyframexl::sequence::volume_pos(inst.volume) as f64,
                0.0,
                1.0,
                keyframexl::sequence::volume_pos(1.0) as f64,
                &vol_fmt,
                "Channel volume. Double-click for 0 dB.",
            ),
        ];
        for (k, (label, v, min, max, default, fmt, hint)) in specs.into_iter().enumerate() {
            let rect = Rect::from_center_size(pos2(x + 12.0, mid), vec2(24.0, 24.0));
            let mut val = v;
            let color = if k == 0 {
                t::enc_white()
            } else {
                t::enc_orange()
            };
            let spec = kit::Knob {
                label,
                color,
                min,
                max,
                default,
                log: false,
                fmt,
                hint,
            };
            kit::automatable(
                ui,
                if k == 0 {
                    keyframexl::automation::Target::ChannelPan { inst: id }
                } else {
                    keyframexl::automation::Target::ChannelVolume { inst: id }
                },
            );
            let (_, started, changed) = kit::mini_knob(
                ui,
                rect,
                ui.id().with(("chan-knob", id, k)),
                &spec,
                &mut val,
            );
            if started {
                self.checkpoint();
            }
            if changed && let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == id) {
                let (target, sent) = if k == 0 {
                    i.pan = val as f32;
                    (
                        keyframexl::automation::Target::ChannelPan { inst: id },
                        i.pan,
                    )
                } else {
                    i.volume = keyframexl::sequence::volume_gain(val as f32);
                    (
                        keyframexl::automation::Target::ChannelVolume { inst: id },
                        i.volume,
                    )
                };
                self.publish_param(target, sent);
                self.say(
                    Tone::Quiet,
                    format!(
                        "{name} {} {}. Ctrl+Z undoes.",
                        label.to_lowercase(),
                        fmt(val)
                    ),
                );
            }
            x += 24.0;
        }

        // Mixer insert (FL: the number box). Master until the mixer exists.
        // Hit area is at least 24x24 ("M" and numbers share the same box).
        let mbox = Rect::from_min_size(pos2(x + 2.0, mid - 12.0), vec2(32.0, 24.0));
        let well = kit::sunken(p, mbox, 4.0);
        // Master shows as "--" (FL): "M" next to the mute lamp reads as Mute.
        let target = if inst.mixer == 0 {
            "--".to_string()
        } else {
            format!("{}", inst.mixer)
        };
        p.text(
            well.center(),
            Align2::CENTER_CENTER,
            &target,
            t::mono(12.0),
            if audible { t::muted() } else { t::wait() },
        );
        let mr = ui.interact(
            mbox,
            ui.id().with(("mixer-box", id)),
            Sense::click_and_drag(),
        );
        if mr.hovered() {
            p.rect_stroke(
                mbox,
                4.0,
                Stroke::new(1.0_f32, t::strong()),
                egui::StrokeKind::Inside,
            );
        }
        let mr = mr.on_hover_text(format!(
            "Mixer insert: {}. Click to choose, scroll or drag to step.",
            self.mixer_name(inst.mixer as usize)
        ));
        let wheel = kit::wheel_steps(ui, &mr);
        let acc_id = ui.id().with(("mixer-drag", id));
        let drag = kit::captured_drag(ui, &mr);
        let remainder = if mr.drag_started() {
            0.0
        } else {
            ui.data(|d| d.get_temp::<f32>(acc_id)).unwrap_or(0.0)
        };
        let acc = remainder - drag.y;
        let drag_steps = (acc / 10.0).trunc();
        ui.data_mut(|d| d.insert_temp(acc_id, acc - drag_steps * 10.0));
        let steps = wheel + drag_steps as i32;
        if steps != 0 {
            let to = stepped_insert(inst.mixer, steps, self.song.mixer.len());
            // One undo step per drag, or per run of wheel notches (0.6 s apart at most).
            let run_id = ui.id().with(("mixer-step-run", id));
            let now = ui.input(|i| i.time);
            let last: f64 = ui.data(|d| d.get_temp(run_id)).unwrap_or(-10.0);
            let fresh = if mr.dragged() {
                mr.drag_started() || last < 0.0
            } else {
                now - last > 0.6
            };
            ui.data_mut(|d| d.insert_temp(run_id, if mr.dragged() { 0.0 } else { now }));
            self.step_channel_insert(id, to, fresh);
        }
        if mr.drag_stopped() {
            ui.data_mut(|d| d.insert_temp(ui.id().with(("mixer-step-run", id)), -10.0_f64));
        }
        let pid = ui.make_persistent_id(("mixer-pick", id));
        if mr.clicked() {
            ui.memory_mut(|m| m.toggle_popup(pid));
        }
        let mut pick = None;
        let names: Vec<String> = (0..self.song.mixer.len())
            .map(|m| {
                if m == 0 {
                    "Master".to_string()
                } else {
                    format!("{m:02}  {}", self.mixer_name(m))
                }
            })
            .collect();
        kit::popup_menu(ui, pid, &mr, 160.0, |ui| {
            for (m, label) in names.iter().enumerate() {
                if ui
                    .selectable_label(inst.mixer as usize == m, label)
                    .clicked()
                {
                    pick = Some(m as u16);
                }
            }
        });
        if let Some(m) = pick {
            self.set_channel_insert(id, m);
        }

        let label = Rect::from_min_max(
            pos2(mbox.right() + 4.0, mid - 12.0),
            pos2(right, mid + 12.0),
        );
        if label.is_positive() {
            self.channel_label(ui, p, label, id, name, audible);
        }
    }

    /// Channel name button (FL). Click opens the instrument window, right-click
    /// offers the piano roll, preset loading and deletion.
    /// How lit a channel's button is: (note playing, tail ringing), each 0..1,
    /// fading out over a short moment so a hit is seen, not just blinked.
    fn channel_glow(&mut self, ui: &Ui, id: u64) -> (f32, f32) {
        let Some(a) = &self.audio else {
            return (0.0, 0.0);
        };
        let Some(i) = self.song.instruments.iter().position(|x| x.id == id) else {
            return (0.0, 0.0);
        };
        let Some(slot) = a.activity.get(i) else {
            return (0.0, 0.0);
        };
        let now = ui.input(|i| i.time);
        let state = slot.swap(0, std::sync::atomic::Ordering::Relaxed);
        let key = egui::Id::new(("channel-glow", id));
        let (mut play_t, mut tail_t): (f64, f64) =
            ui.data(|d| d.get_temp(key)).unwrap_or((f64::MIN, f64::MIN));
        if state == 2 {
            play_t = now;
        }
        if state >= 1 {
            tail_t = now;
        }
        ui.data_mut(|d| d.insert_temp(key, (play_t, tail_t)));
        let fade = |t: f64, hold: f64| (1.0 - ((now - t - hold) / 0.18).clamp(0.0, 1.0)) as f32;
        let (play, tail) = (fade(play_t, 0.03), fade(tail_t, 0.05));
        if play > 0.0 || tail > 0.0 || self.is_playing() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(30));
        }
        (play, tail)
    }

    fn channel_label(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        rect: Rect,
        id: u64,
        name: &str,
        audible: bool,
    ) {
        let r = ui.interact(rect, ui.id().with(("channel", id)), Sense::click_and_drag());
        if r.drag_started_by(egui::PointerButton::Primary) {
            r.dnd_set_drag_payload(ChannelDrag(id));
        }
        // A file from the browser dropped here loads into this channel.
        if r.dnd_hover_payload::<super::files::FileDrag>().is_some() {
            p.rect_stroke(
                rect.expand(2.0),
                6.0,
                Stroke::new(2.0_f32, t::text()),
                egui::StrokeKind::Outside,
            );
        }
        if let Some(drag) = r.dnd_release_payload::<super::files::FileDrag>() {
            self.drop_file_on_channel(id, drag.0.clone());
        }
        let hover = kit::hover_amount(ui, &r);
        // Lit while its note plays (FL: the channel button), a low bar while
        // only its tail rings on.
        let (play, tail) = self.channel_glow(ui, id);
        let top = kit::mix(
            kit::mix(t::ctl_top(), t::ctl_hover(), hover),
            kit::mix(t::ctl_top(), t::ok(), 0.45),
            play,
        );
        let face = kit::raised(
            p,
            rect,
            5.0,
            top,
            t::ctl_bottom(),
            t::edge(),
            r.is_pointer_button_down_on(),
            1.0,
        );
        let tail = tail.max(play);
        if tail > 0.0 {
            let bar = Rect::from_min_max(
                pos2(face.left() + 4.0, face.bottom() - 3.0),
                pos2(face.right() - 4.0, face.bottom() - 1.0),
            );
            p.rect_filled(bar, 1.0, t::ok().gamma_multiply(0.25 + 0.55 * tail));
        }
        let shown = kit::elide(p, name, t::medium(13.0), face.width() - 14.0);
        p.text(
            pos2(face.left() + 8.0, face.center().y),
            Align2::LEFT_CENTER,
            shown,
            t::medium(13.0),
            if audible { t::text() } else { t::faint() },
        );
        if kit::show_focus(ui, &r) {
            kit::focus_ring(p, rect, 5.0);
        }
        let sampler = self.is_sampler(id);
        let plugin = self
            .song
            .instrument(id)
            .and_then(|i| i.plugin())
            .map(|p| p.id);
        let r = r.on_hover_text(format!(
            "{name}: click for the {} window, drag to reorder, right-click for more",
            if plugin.is_some() {
                "plugin's"
            } else {
                "instrument"
            }
        ));
        if r.clicked() {
            match plugin {
                // FL: a plugin channel's button opens the plugin itself.
                Some(pid) => self.open_plugin(pid),
                None => self.win(Cmd::Show(Tab::Instrument { id })),
            }
        }
        r.context_menu(|ui| {
            kit::menu_style(ui);
            if ui.button("Instrument window").clicked() {
                self.win(Cmd::Show(Tab::Instrument { id }));
                ui.close_menu();
            }
            if ui.button("Piano roll").clicked() {
                self.open_roll(id);
                ui.close_menu();
            }
            if sampler && ui.button("Load preset").clicked() {
                self.load_preset_dialog(id);
                ui.close_menu();
            }
            ui.separator();
            self.channel_notes_menu(ui, id);
            ui.separator();
            ui.menu_button("Steps", |ui| {
                kit::menu_style(ui);
                if ui.button("Shift left").clicked() {
                    self.step_shift(id, false);
                    ui.close_menu();
                }
                if ui.button("Shift right").clicked() {
                    self.step_shift(id, true);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Fill every 2").clicked() {
                    self.step_fill(id, 2);
                    ui.close_menu();
                }
                if ui.button("Fill every 4").clicked() {
                    self.step_fill(id, 4);
                    ui.close_menu();
                }
                if ui.button("Fill every 8").clicked() {
                    self.step_fill(id, 8);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Invert").clicked() {
                    self.step_invert(id);
                    ui.close_menu();
                }
                if ui.button("Clear").clicked() {
                    self.step_clear(id);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Humanize…").clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("rack-humanize"),
                            Some((id, 15i64, 0i64, 1u64)),
                        )
                    });
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button("Delete channel").clicked() {
                self.delete_channel(id);
                ui.close_menu();
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_pad(
        &self,
        ui: &Ui,
        rect: Rect,
        r: &Response,
        i: usize,
        vel: u8,
        audible: bool,
        current: bool,
    ) {
        let p = ui.painter();
        let hover = kit::hover_amount(ui, r);
        let down = r.is_pointer_button_down_on();
        let lift: f32 = if current { 0.35_f32 } else { 0.0 }.max(hover * 0.6);
        let face = if vel > 0 && audible {
            // FL-style lit pad: warm white; only the velocity bar stays red.
            kit::raised(
                p,
                rect,
                6.0,
                kit::mix(LIT_PAD_TOP, egui::Color32::WHITE, lift * 0.5),
                kit::mix(LIT_PAD_BOTTOM, LIT_PAD_TOP, lift),
                t::edge(),
                down,
                kit::DEPTH,
            )
        } else if vel > 0 {
            kit::raised(
                p,
                rect,
                6.0,
                t::brand_soft(),
                kit::mix(t::brand_soft(), t::edge(), 0.3),
                t::edge(),
                down,
                kit::DEPTH,
            )
        } else if audible && (i / 4) % 2 == 1 {
            kit::raised(
                p,
                rect,
                6.0,
                kit::mix(t::ctl_top(), t::panel_top(), 0.18),
                t::ctl_bottom(),
                t::edge(),
                down,
                kit::DEPTH,
            )
        } else if audible {
            kit::control(p, rect, 6.0, lift, down)
        } else {
            kit::raised(
                p,
                rect,
                6.0,
                t::ctl_bottom(),
                t::panel_bottom(),
                t::edge(),
                down,
                kit::DEPTH,
            )
        };
        // Printed legend, top-left like a hardware key — only when the pad
        // is big enough to hold it without overlap.
        if rect.width() >= 28.0 && rect.height() >= 28.0 {
            let legend = if vel > 0 && audible {
                t::key_ink()
            } else if current {
                t::text()
            } else {
                t::muted()
            };
            p.text(
                face.left_top() + vec2(6.0, 4.0),
                Align2::LEFT_TOP,
                format!("{:02}", i + 1),
                t::mono(12.0),
                legend,
            );
        }
        if vel > 0 {
            // Velocity bar along the bottom of the key, 2 px tall. Stays red
            // even on a warm-white lit pad.
            let track = Rect::from_min_max(
                pos2(face.left() + 4.0, face.bottom() - 5.0),
                pos2(face.right() - 4.0, face.bottom() - 3.0),
            );
            let (bg, fg) = if audible {
                (t::brand_edge(), t::brand_fill())
            } else {
                (t::edge(), t::brand())
            };
            p.rect_filled(track, 1.0, bg);
            let mut fill = track;
            fill.set_width(track.width() * vel as f32 / 127.0);
            p.rect_filled(fill, 1.0, fg);
        }
        if kit::show_focus(ui, r) {
            kit::focus_ring(p, rect, 6.0);
        }
    }

    /// Mini piano roll of one channel: beat grid, notes, slides.
    fn paint_note_strip(
        &self,
        p: &egui::Painter,
        area: Rect,
        inst: u64,
        pat: &keyframexl::sequence::Pattern,
        hot: bool,
    ) {
        let len = pat.length as f32;
        let sx = |t: i64| area.left() + t as f32 / len * area.width();
        for b in 1..(pat.length / (STEP * 4)) {
            let x = sx(b * STEP * 4);
            p.line_segment(
                [pos2(x, area.top()), pos2(x, area.bottom())],
                Stroke::new(1.0_f32, t::line()),
            );
        }
        let notes: Vec<_> = pat.notes.iter().filter(|n| n.instrument == inst).collect();
        let slides: Vec<_> = pat.slides.iter().filter(|s| s.instrument == inst).collect();
        let lo = notes
            .iter()
            .map(|n| n.pitch)
            .chain(slides.iter().map(|s| s.target))
            .min()
            .unwrap_or(48);
        let hi = notes
            .iter()
            .map(|n| n.pitch)
            .chain(slides.iter().map(|s| s.target))
            .max()
            .unwrap_or(60)
            .max(lo + 12);
        let py = |pitch: u8| {
            area.bottom() - 3.0 - (pitch - lo) as f32 / (hi - lo) as f32 * (area.height() - 6.0)
        };
        for s in &slides {
            let y = py(s.target);
            let (a, b) = (sx(s.start), sx(s.start + s.length));
            p.line_segment([pos2(a, y), pos2(b, y)], Stroke::new(3.0_f32, t::wait()));
            if b - a > 70.0 {
                p.text(
                    pos2(a, y + 4.0),
                    Align2::LEFT_TOP,
                    format!("slide to {}", super::roll::note_name(s.target)),
                    t::sans(12.0),
                    t::wait(),
                );
            }
        }
        let seen = p.clip_rect();
        for n in &notes {
            if sx(n.end()) < seen.left() || sx(n.start) > seen.right() {
                continue;
            }
            let r = Rect::from_min_max(
                pos2(sx(n.start) + 1.0, py(n.pitch) - 2.0),
                pos2(
                    (sx(n.end()) - 1.0).max(sx(n.start) + 3.0),
                    py(n.pitch) + 2.0,
                ),
            );
            p.rect_filled(r, 2.0, if n.muted { t::strong() } else { t::text() });
        }
        if hot && area.height() >= 24.0 {
            let tag = "Open in piano roll  F7";
            let w = kit::text_width(p, tag, t::sans(13.0)) + 20.0;
            let r = Rect::from_min_size(
                pos2(area.right() - w, area.center().y - 13.0),
                vec2(w, 26.0),
            );
            p.rect_filled(r, 6.0, t::raised());
            p.rect_stroke(
                r,
                6.0,
                Stroke::new(1.0_f32, t::strong()),
                egui::StrokeKind::Inside,
            );
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                tag,
                t::sans(13.0),
                t::text(),
            );
        }
    }
}

fn stepped_insert(current: u16, steps: i32, tracks: usize) -> u16 {
    (i64::from(current) + i64::from(steps)).clamp(0, tracks.saturating_sub(1) as i64) as u16
}

/// A channel name the user chose: anything but the instrument's own default —
/// the old plugin's name, the sampler preset's name, or a fresh built-in name
/// (`Kick`, `Sampler 2`, …). Dropping a plugin on a default-named channel
/// renames it (FL); a custom name stays.
fn is_custom_channel_name(name: &str, kind: &keyframexl::sequence::InstrumentKind) -> bool {
    match kind {
        keyframexl::sequence::InstrumentKind::Plugin { plugin } if name == plugin.name => {
            return false;
        }
        keyframexl::sequence::InstrumentKind::Sampler { preset: Some(p) } if name == p.name => {
            return false;
        }
        _ => {}
    }
    const BUILTINS: [&str; 5] = ["Kick", "Snare", "Hat", "Bass", "Sampler"];
    if BUILTINS.contains(&name) {
        return false;
    }
    if let Some((base, num)) = name.rsplit_once(' ')
        && num.parse::<u32>().is_ok()
        && BUILTINS.contains(&base)
    {
        return false;
    }
    true
}

#[cfg(test)]
mod insert_tests {
    use super::stepped_insert;
    #[test]
    fn insert_steps_stay_within_tracks() {
        assert_eq!(stepped_insert(0, -3, 4), 0);
        assert_eq!(stepped_insert(2, 1, 4), 3);
        assert_eq!(stepped_insert(2, 9, 4), 3);
        assert_eq!(stepped_insert(0, 1, 0), 0);
    }
}

#[cfg(test)]
mod channel_name_tests {
    use super::is_custom_channel_name;
    use keyframexl::sequence::{InstrumentKind, PluginRef};
    use std::sync::Arc;

    fn plugin_kind(name: &str) -> InstrumentKind {
        InstrumentKind::Plugin {
            plugin: PluginRef {
                id: 1,
                format: "vst3".into(),
                path: "x".into(),
                name: name.into(),
                vendor: String::new(),
                state: String::new(),
                outs: vec![],
            },
        }
    }

    #[test]
    fn defaults_are_not_custom() {
        assert!(!is_custom_channel_name("Dexed", &plugin_kind("Dexed")));
        assert!(!is_custom_channel_name(
            "Kick",
            &InstrumentKind::Drum { voice: 0 }
        ));
        assert!(!is_custom_channel_name(
            "Bass",
            &InstrumentKind::Sampler { preset: None }
        ));
        assert!(!is_custom_channel_name(
            "Sampler 1",
            &InstrumentKind::Sampler { preset: None }
        ));
        assert!(!is_custom_channel_name(
            "Kick 2",
            &InstrumentKind::Drum { voice: 0 }
        ));
        let preset = keyframexl::sequence::SamplePreset::test_preset("Keys");
        assert!(!is_custom_channel_name(
            "Keys",
            &InstrumentKind::Sampler {
                preset: Some(Arc::new(preset))
            }
        ));
    }

    #[test]
    fn user_names_are_custom() {
        assert!(is_custom_channel_name(
            "My Bass",
            &InstrumentKind::Drum { voice: 0 }
        ));
        assert!(is_custom_channel_name("My Bass", &plugin_kind("Dexed")));
        let preset = keyframexl::sequence::SamplePreset::test_preset("Keys");
        assert!(is_custom_channel_name(
            "My Keys",
            &InstrumentKind::Sampler {
                preset: Some(Arc::new(preset))
            }
        ));
        assert!(is_custom_channel_name(
            "Sampler live",
            &InstrumentKind::Sampler { preset: None }
        ));
    }
}

/// A held-button stroke across step pads (FL paints one value per stroke).
#[derive(Clone)]
struct PadStroke {
    /// Right button: clears.
    right: bool,
    /// Set (true) or clear (false) every pad entered.
    fill: bool,
    visited: Vec<(u64, usize)>,
    /// Pointer at the last frame: the path from there is painted too.
    last: Option<egui::Pos2>,
    /// An undo step was made for this stroke.
    changed: bool,
}
