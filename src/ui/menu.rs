//! Main menu (FL: FILE EDIT ADD PATTERNS VIEW OPTIONS TOOLS HELP). Every item
//! carries its shortcut. Items whose engine does not exist yet stay visible but
//! disabled, so the structure and the shortcuts do not move later.

use super::{App, Cmd, Kind, Tab, Tone, kit, workspace::PRESETS};
use crate::theme as t;
use eframe::egui::{self, Button, RichText, Ui, pos2, vec2};

pub const MENU_H: f32 = 26.0;

/// A menu row: label, shortcut, enabled; a disabled row says why on hover.
fn item(ui: &mut Ui, label: &str, key: &str, enabled: bool, later: &str) -> bool {
    let b = Button::new(RichText::new(label).font(t::sans(13.0)))
        .shortcut_text(RichText::new(key).font(t::mono(12.0)))
        .min_size(vec2(220.0, 24.0));
    let r = ui.add_enabled(enabled, b);
    let r = if enabled {
        r
    } else {
        r.on_disabled_hover_text(later)
    };
    if r.clicked() {
        ui.close_menu();
        true
    } else {
        false
    }
}

/// A toggle row for settings that exist in the model already.
fn check(ui: &mut Ui, label: &str, key: &str, on: bool) -> bool {
    let r = kit::menu_check(ui, on, label, key);
    if r.clicked() {
        ui.close_menu();
        true
    } else {
        false
    }
}

impl App {
    pub(super) fn main_menu(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, t::SIDE);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1.0_f32, t::EDGE),
        );
        let row = rect.shrink2(vec2(super::PAGE - 8.0, 0.0));
        // Our own title bar: its empty space moves the window, a double-click
        // maximises (widgets drawn after this take their own clicks).
        let own_bar = self.own_title_bar();
        if own_bar {
            let drag = ui.interact(
                rect,
                ui.id().with("title-drag"),
                egui::Sense::click_and_drag(),
            );
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().button_padding = vec2(8.0, 2.0);
                ui.spacing_mut().interact_size.y = MENU_H - 6.0;
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.add_space(4.0);
                let (mark_rect, mark_resp) =
                    ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::click());
                kit::icon_brand(ui.painter(), mark_rect);
                if mark_resp
                    .on_hover_text("About KeyFrameXL")
                    .clicked()
                {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("about-open"), true));
                }
                ui.add_space(6.0);
                let title = |s: &str| {
                    RichText::new(s)
                        .font(t::medium(12.0))
                        .extra_letter_spacing(0.8)
                        .color(t::MUTED)
                };
                ui.menu_button(title("FILE"), |ui| self.menu_file(ui));
                ui.menu_button(title("EDIT"), |ui| self.menu_edit(ui));
                ui.menu_button(title("ADD"), |ui| self.menu_add(ui));
                ui.menu_button(title("PATTERNS"), |ui| self.menu_patterns(ui));
                ui.menu_button(title("VIEW"), |ui| self.menu_view(ui));
                ui.menu_button(title("OPTIONS"), |ui| self.menu_options(ui));
                ui.menu_button(title("TOOLS"), |ui| self.menu_tools(ui));
                ui.menu_button(title("HELP"), |ui| self.menu_help(ui));
                let name = self
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".into());
                let dirty = if self.dirty() { "  ·  unsaved" } else { "" };
                if own_bar {
                    // Window buttons at the right end; the title centred in the row.
                    let menus_end = ui.min_rect().right();
                    let buttons = self.title_buttons(ui, rect);
                    let text = format!("{name}{dirty}  —  KeyFrameXL");
                    let g = ui.painter().layout_no_wrap(text, t::sans(12.0), t::MUTED);
                    let room = (menus_end + 16.0)..(buttons - 16.0);
                    let x = rect.center().x.clamp(
                        room.start + g.size().x / 2.0,
                        (room.end - g.size().x / 2.0).max(room.start + g.size().x / 2.0),
                    );
                    if room.end - room.start > g.size().x {
                        ui.painter().galley(
                            pos2(x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0),
                            g,
                            t::MUTED,
                        );
                    }
                } else {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("{name}{dirty}"))
                                .font(t::sans(12.0))
                                .color(t::FAINT),
                        );
                    });
                }
            },
        );
    }

    /// Our title bar is on: no desktop decorations (FL draws its own).
    pub(crate) fn own_title_bar(&self) -> bool {
        // Captures show it with KFXL_CAPTURE_TITLEBAR=1.
        (self.capture.is_none() || std::env::var_os("KFXL_CAPTURE_TITLEBAR").is_some())
            && !self.settings.general.system_title_bar
    }

    /// Minimise, maximise / restore and close, flush with the window's top
    /// right corner. Returns where they start (the title stays left of it).
    fn title_buttons(&mut self, ui: &mut Ui, bar: egui::Rect) -> f32 {
        let w = 44.0;
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        let left = bar.right() - 3.0 * w;
        for (k, kind) in ["min", "max", "close"].into_iter().enumerate() {
            let r = egui::Rect::from_min_size(
                pos2(left + k as f32 * w, bar.top()),
                vec2(w, bar.height() - 1.0),
            );
            let resp = ui.interact(r, ui.id().with(("win-button", kind)), egui::Sense::click());
            let hot = resp.hovered();
            let p = ui.painter();
            if hot {
                p.rect_filled(
                    r,
                    0.0,
                    if kind == "close" {
                        t::BRAND_FILL
                    } else {
                        t::CTL_HOVER
                    },
                );
            }
            let ink = if hot { t::TEXT } else { t::MUTED };
            let s = egui::Stroke::new(1.2_f32, ink);
            let c = r.center();
            match kind {
                "min" => {
                    p.line_segment([c + vec2(-5.0, 0.5), c + vec2(5.0, 0.5)], s);
                }
                "max" if maximized => {
                    p.rect_stroke(
                        egui::Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0)),
                        1.0,
                        s,
                        egui::StrokeKind::Middle,
                    );
                    p.line_segment([c + vec2(-2.5, -4.5), c + vec2(4.5, -4.5)], s);
                    p.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 2.5)], s);
                }
                "max" => {
                    p.rect_stroke(
                        egui::Rect::from_center_size(c, vec2(10.0, 10.0)),
                        1.0,
                        s,
                        egui::StrokeKind::Middle,
                    );
                }
                _ => {
                    p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                    p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], s);
                }
            }
            let tip = match kind {
                "min" => "Minimise",
                "max" if maximized => "Restore",
                "max" => "Maximise",
                _ => "Close",
            };
            if resp.on_hover_text(tip).clicked() {
                let cmd = match kind {
                    "min" => egui::ViewportCommand::Minimized(true),
                    "max" => egui::ViewportCommand::Maximized(!maximized),
                    _ => egui::ViewportCommand::Close,
                };
                ui.ctx().send_viewport_cmd(cmd);
            }
        }
        left
    }

    /// With our own title bar: the window's edges and corners resize it
    /// (the desktop does the rest, as for its own frame). Off while maximised.
    pub(crate) fn window_edges(&self, ctx: &egui::Context) {
        if !self.own_title_bar()
            || ctx.input(|i| {
                i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
            })
        {
            return;
        }
        use egui::{CursorIcon as C, viewport::ResizeDirection as D};
        let s = ctx.screen_rect();
        let (e, c) = (4.0, 14.0);
        let zones = [
            (
                D::NorthWest,
                egui::Rect::from_min_size(s.left_top(), vec2(c, c)),
                C::ResizeNwSe,
            ),
            (
                D::NorthEast,
                egui::Rect::from_min_size(pos2(s.right() - c, s.top()), vec2(c, c)),
                C::ResizeNeSw,
            ),
            (
                D::SouthWest,
                egui::Rect::from_min_size(pos2(s.left(), s.bottom() - c), vec2(c, c)),
                C::ResizeNeSw,
            ),
            (
                D::SouthEast,
                egui::Rect::from_min_size(s.right_bottom() - vec2(c, c), vec2(c, c)),
                C::ResizeNwSe,
            ),
            (
                D::North,
                egui::Rect::from_min_max(
                    pos2(s.left() + c, s.top()),
                    pos2(s.right() - c, s.top() + e),
                ),
                C::ResizeVertical,
            ),
            (
                D::South,
                egui::Rect::from_min_max(
                    pos2(s.left() + c, s.bottom() - e),
                    pos2(s.right() - c, s.bottom()),
                ),
                C::ResizeVertical,
            ),
            (
                D::West,
                egui::Rect::from_min_max(
                    pos2(s.left(), s.top() + c),
                    pos2(s.left() + e, s.bottom() - c),
                ),
                C::ResizeHorizontal,
            ),
            (
                D::East,
                egui::Rect::from_min_max(
                    pos2(s.right() - e, s.top() + c),
                    pos2(s.right(), s.bottom() - c),
                ),
                C::ResizeHorizontal,
            ),
        ];
        egui::Area::new(egui::Id::new("window-edges"))
            .order(egui::Order::Foreground)
            .fixed_pos(s.min)
            .interactable(true)
            .show(ctx, |ui| {
                for (k, (dir, r, icon)) in zones.into_iter().enumerate() {
                    let resp = ui
                        .interact(r, ui.id().with(("edge", k)), egui::Sense::drag())
                        .on_hover_cursor(icon);
                    if resp.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                    }
                }
            });
    }

    fn menu_file(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if item(ui, "New", "Ctrl+Shift+N", true, "") {
            self.request(super::Pending::New);
        }
        if item(ui, "Open…", "Ctrl+O", true, "") {
            self.request(super::Pending::Open(None));
        }
        self.recent_menu(ui);
        self.recover_item(ui);
        ui.separator();
        if item(ui, "Save", "Ctrl+S", true, "") {
            self.save();
        }
        if item(ui, "Save as…", "Ctrl+Shift+S", true, "") {
            self.save_as();
        }
        if item(ui, "Save new version", "Ctrl+N", true, "") {
            self.save_new_version();
        }
        if item(ui, "Project info…", "F11", true, "") {
            self.toggle_info(ui.ctx());
        }
        ui.separator();
        ui.menu_button(RichText::new("Export").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            if item(ui, "Audio file (WAV, FLAC, MP3)…", "Ctrl+R", true, "") {
                self.export();
            }
            if item(ui, "MIDI file…", "Ctrl+Shift+M", true, "") {
                self.export_midi();
            }
            ui.separator();
            if item(
                ui,
                "Project bundle (.zip, Collect all & save)…",
                "",
                true,
                "",
            ) {
                self.export_project_bundle();
            }
        });
        ui.menu_button(RichText::new("Import").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            if item(
                ui,
                "Sampler preset (.dwp) into a new channel…",
                "",
                true,
                "",
            ) {
                let id = self.add_sampler();
                self.load_preset_dialog(id);
            }
            if item(ui, "MIDI file…", "", true, "") {
                self.import_midi_dialog();
            }
            ui.separator();
            if item(ui, "Project bundle (.zip)…", "", true, "") {
                self.import_project_bundle();
            }
        });
        ui.separator();
        if item(ui, "Exit", "", true, "") {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn menu_edit(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if item(
            ui,
            "Undo",
            "Ctrl+Z",
            !self.history.is_empty(),
            "Nothing to undo",
        ) {
            self.undo();
        }
        if item(
            ui,
            "Redo",
            "Ctrl+Y",
            !self.future.is_empty(),
            "Nothing to redo",
        ) {
            self.redo();
        }
        ui.separator();
        // Piano roll notes (the selection of the last focused roll).
        let sel = !self.roll_sel.is_empty();
        if item(
            ui,
            "Cut notes",
            "Ctrl+X",
            sel,
            "Select notes in the piano roll first",
        ) {
            self.roll_inst = self.instrument;
            self.copy_notes(true);
        }
        if item(
            ui,
            "Copy notes",
            "Ctrl+C",
            sel,
            "Select notes in the piano roll first",
        ) {
            self.roll_inst = self.instrument;
            self.copy_notes(false);
        }
        if item(
            ui,
            "Paste notes",
            "Ctrl+V",
            !self.note_clip.is_empty(),
            "Copy notes first",
        ) {
            self.roll_inst = self.instrument;
            self.paste_notes();
        }
    }

    fn menu_add(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        ui.menu_button(RichText::new("Channel").font(t::sans(13.0)), |ui| {
            self.add_channel_menu(ui)
        });
        if item(ui, "Effect (opens the mixer)", "F9", true, "") {
            self.win(Cmd::Show(Tab::Mixer));
        }
        ui.menu_button(
            RichText::new("Plugin instrument").font(t::sans(13.0)),
            |ui| self.plugin_instrument_menu(ui),
        );
        ui.separator();
        if item(ui, "Pattern", "Ctrl+Shift+Ins", true, "") {
            self.new_pattern();
        }
    }

    fn menu_patterns(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if item(ui, "Find next empty", "F4", true, "") {
            self.next_empty_pattern();
        }
        if item(ui, "Rename…", "F2", true, "") {
            self.start_rename();
        }
        ui.menu_button(RichText::new("Colour").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            let cur = self.pattern().color;
            if ui
                .selectable_label(cur.is_none(), "By id (default)")
                .clicked()
            {
                self.checkpoint();
                self.pattern_mut().color = None;
                self.publish();
                self.say(Tone::Quiet, "Pattern colour reset to by-id. Ctrl+Z undoes.");
                ui.close_menu();
            }
            ui.separator();
            for col in crate::theme::COLOR_PALETTE {
                let hex = format!("{:02X}{:02X}{:02X}", col.r(), col.g(), col.b());
                let on = cur == Some(crate::theme::u32_from_color(col));
                if ui.selectable_label(on, format!("#{hex}")).clicked() {
                    self.checkpoint();
                    self.pattern_mut().color = Some(crate::theme::u32_from_color(col));
                    self.publish();
                    self.say(
                        Tone::Quiet,
                        format!("Pattern colour #{hex}. Ctrl+Z undoes."),
                    );
                    ui.close_menu();
                }
            }
        });
        if item(ui, "Clone", "", true, "") {
            self.clone_pattern();
        }
        if item(
            ui,
            "Delete",
            "",
            self.song.patterns.len() > 1,
            "The last pattern stays",
        ) {
            self.delete_pattern();
        }
    }

    fn menu_view(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        let roll = Tab::Roll {
            instrument: self.instrument,
        };
        for (label, key, tab) in [
            ("Playlist", "F5", Tab::Playlist),
            ("Channel rack", "F6", Tab::Rack),
            ("Piano roll", "F7", roll),
        ] {
            let open = self
                .ws
                .dock()
                .iter_all_tabs()
                .any(|(_, t)| t.kind() == tab.kind());
            if check(ui, label, key, open) {
                self.win(Cmd::Toggle(tab));
            }
        }
        let open = self
            .ws
            .dock()
            .iter_all_tabs()
            .any(|(_, t)| *t == Tab::Mixer);
        if check(ui, "Mixer", "F9", open) {
            self.win(Cmd::Toggle(Tab::Mixer));
        }
        if check(ui, "Browser", "Alt+F8", self.ws.browser) {
            self.ws.browser = !self.ws.browser;
        }
        if check(ui, "Follow playhead", "", self.follow) {
            self.follow = !self.follow;
        }
        ui.menu_button(RichText::new("Deck monitor").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            for mode in crate::settings::DeckMonitor::ALL {
                if check(
                    ui,
                    mode.label(),
                    "",
                    self.settings.general.deck_monitor == mode,
                ) {
                    self.set_deck_monitor(mode);
                }
            }
        });
        ui.separator();
        ui.menu_button(RichText::new("Workspace").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            for (i, name) in PRESETS.iter().enumerate() {
                if check(ui, name, &format!("Alt+{}", i + 1), self.ws.active == i) {
                    self.win(Cmd::Preset(i));
                }
            }
            ui.separator();
            if item(ui, "Reset this workspace", "", true, "") {
                self.win(Cmd::Reset);
            }
        });
        if item(ui, "Close all windows", "F12", true, "") {
            self.close_all_windows();
        }
        ui.separator();
        ui.menu_button(RichText::new("Interface size").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            use crate::settings::{UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALES, step_ui_scale};
            let z = ui.ctx().zoom_factor();
            if item(
                ui,
                "Larger",
                "Ctrl++",
                z < UI_SCALE_MAX - 0.001,
                "Largest size",
            ) {
                ui.ctx().set_zoom_factor(step_ui_scale(z, true));
            }
            if item(
                ui,
                "Smaller",
                "Ctrl+-",
                z > UI_SCALE_MIN + 0.001,
                "Smallest size",
            ) {
                ui.ctx().set_zoom_factor(step_ui_scale(z, false));
            }
            if item(ui, "100%", "Ctrl+0", (z - 1.0).abs() > 0.001, "") {
                ui.ctx().set_zoom_factor(1.0);
            }
            ui.separator();
            for s in UI_SCALES {
                let on = (z - s).abs() < 0.001;
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{:.0}%", s * 100.0)).font(t::sans(13.0)),
                        )
                        .selected(on),
                    )
                    .clicked()
                {
                    ui.ctx().set_zoom_factor(s);
                    ui.close_menu();
                }
            }
        });
    }

    fn menu_options(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if item(ui, "Audio settings…", "", true, "") {
            self.open_settings(super::settings_ui::Page::Audio);
        }
        if item(ui, "MIDI settings…", "F10", true, "") {
            self.open_settings(super::settings_ui::Page::Midi);
        }
        if item(ui, "Plugins…", "", true, "") {
            self.open_settings(super::settings_ui::Page::Plugins);
        }
        if item(ui, "General settings…", "", true, "") {
            self.open_settings(super::settings_ui::Page::General);
        }
        ui.separator();
        if check(ui, "Metronome", "Ctrl+M", self.settings.general.metronome) {
            self.toggle_metronome();
        }
        if check(
            ui,
            "Typing keyboard to piano",
            "Ctrl+T",
            self.settings.general.typing_keyboard,
        ) {
            self.toggle_typing_piano();
        }
        if check(ui, "Record automation", "", self.rec.automation) {
            self.toggle_record_automation();
        }
        if check(ui, "Record audio input (mic / line)", "", self.rec.input) {
            self.toggle_input();
        }
        ui.separator();
        if check(ui, "Song mode (play the playlist)", "L", self.song_mode) {
            self.set_song_mode(!self.song_mode);
        }
    }

    fn menu_tools(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        let last = self.last_tweaked_target();
        let last_name = last.map(|t| self.auto_label(t));
        ui.add_enabled_ui(last.is_some(), |ui| {
            ui.menu_button(RichText::new("Last tweaked").font(t::sans(13.0)), |ui| {
                kit::menu_style(ui);
                if let Some(name) = &last_name {
                    ui.label(RichText::new(name).font(t::sans(11.5)).color(t::MUTED));
                    ui.separator();
                }
                let Some(target) = last else {
                    return;
                };
                if item(
                    ui,
                    "Create automation clip",
                    "",
                    true,
                    "Add a playlist automation clip for this control",
                ) {
                    self.create_automation(target);
                }
                if item(
                    ui,
                    "Link to controller…",
                    "",
                    true,
                    "Move a knob on your MIDI controller to link this control",
                ) {
                    self.start_midi_learn(target);
                }
                ui.separator();
                if item(ui, "Copy value", "", true, "") {
                    self.copy_last_tweaked(ui.ctx());
                }
                if item(
                    ui,
                    "Paste value",
                    "",
                    self.auto_clipboard.is_some(),
                    "Copy a control value first",
                ) {
                    self.paste_last_tweaked();
                }
                if item(ui, "Reset to default", "", true, "") {
                    self.reset_last_tweaked();
                }
            });
        })
        .response
        .on_disabled_hover_text("Tweak any automatable knob, fader, or plugin parameter first");
        if check(
            ui,
            "Multilink to controllers",
            "Ctrl+J",
            self.multilink.is_some(),
        ) {
            self.toggle_multilink();
        }
        ui.separator();
        if item(
            ui,
            "Riff Machine…",
            "Alt+E",
            true,
            "Generate a riff for the active channel",
        ) {
            let inst = self
                .song
                .instrument(self.instrument)
                .map_or_else(|| self.default_roll(), |i| i.id);
            self.open_roll(inst);
            self.open_riff(inst);
        }
        if item(ui, "Tap tempo", "", true, "") {
            self.tap();
        }
        if item(ui, "Panic: stop all sound", "Ctrl+H", true, "") {
            self.stop();
            self.panic();
            self.say(Tone::Quiet, "All sound stopped.");
        }
    }

    fn menu_help(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if item(ui, "Keyboard shortcuts", "F1", true, "") {
            self.show_shortcuts = true;
        }
        if item(ui, "Manual", "", true, "") {
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let manual = root.join("docs/manual.md");
            let page = if manual.exists() {
                manual
            } else {
                root.join("README.md")
            };
            if let Err(e) = std::process::Command::new("xdg-open").arg(&page).spawn() {
                self.say(Tone::Bad, format!("Could not open {}: {e}", page.display()));
            }
        }
        if item(ui, "Check for updates…", "", true, "") {
            self.check_for_updates(ui.ctx());
        }
        ui.separator();
        if item(ui, "About KeyFrameXL", "", true, "") {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("about-open"), true));
        }
    }

    /// FL F12: close every window of this workspace.
    pub(crate) fn close_all_windows(&mut self) {
        let tabs: Vec<Tab> = self
            .ws
            .dock()
            .iter_all_tabs()
            .map(|(_, t)| t.clone())
            .collect();
        for tab in tabs {
            self.win(Cmd::Close(tab));
        }
    }

    /// Floating list of every shortcut (HELP → Keyboard shortcuts, F1).
    pub(super) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_shortcuts;
        let max_h = (ctx.screen_rect().height() - 140.0).max(200.0);
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .max_height(max_h)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 70.0))
            .show(ctx, |ui| {
                let filter_id = egui::Id::new("shortcut-filter");
                let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("Filter: key or action")
                        .desired_width(f32::INFINITY),
                );
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
                let want = filter.to_lowercase();
                let rows: [(&str, &str); 39] = [
                    ("Space", "Play / stop"),
                    ("Ctrl+Space", "Pause in place"),
                    ("L", "Pattern / song mode"),
                    ("R", "Record audio, MIDI, and automation"),
                    (
                        "F5 · F6 · F7 · F9",
                        "Playlist · Channel rack · Piano roll · Mixer",
                    ),
                    ("Alt+F8", "Show / hide the browser"),
                    ("F12", "Close all windows"),
                    ("Alt+1 · 2 · 3", "Workspaces"),
                    ("Ctrl+Z · Ctrl+Y", "Undo · redo"),
                    ("Ctrl+Shift+N · Ctrl+O · Ctrl+S", "New · open · save"),
                    ("Ctrl+N", "Save new version (name_2, name_3 …)"),
                    ("Ctrl+Shift+S", "Save as"),
                    ("Ctrl+R", "Export wave file"),
                    ("F2", "Rename the pattern"),
                    ("F4", "Next empty pattern"),
                    ("F11", "Project info"),
                    ("Ctrl+Alt+← · →", "Previous · next song marker"),
                    ("Ctrl+1 … 0", "Mute channel 1 … 10"),
                    ("Ctrl+Shift+1 … 0", "Solo channel 1 … 10"),
                    ("Ctrl+Shift+Ins", "New pattern"),
                    ("P B D T", "Piano roll: draw · paint · delete · mute"),
                    ("C E Z S", "Piano roll: slice · select · zoom · slide"),
                    (
                        "Ctrl+Q · Alt+Q",
                        "Piano roll: quick quantize · quantize dialog",
                    ),
                    ("Ctrl+L", "Piano roll: quick legato · Rack: route to mixer"),
                    ("Ctrl+U · Ctrl+G", "Piano roll: chop · glue"),
                    ("Alt+V", "Piano roll: ghost notes"),
                    ("Ctrl+B", "Piano roll: duplicate notes"),
                    ("Ctrl+K", "Rack: graph editor"),
                    ("Alt (drag)", "Skip snap"),
                    ("Shift (knob)", "Fine adjust"),
                    ("Double-click knob", "Reset"),
                    ("Right-click knob", "Type, copy, paste value"),
                    ("Ctrl+J", "Multilink to controllers"),
                    ("Ctrl+H", "Stop all sound (MIDI panic)"),
                    ("F10", "MIDI settings"),
                    ("Ctrl+M", "Metronome"),
                    ("Ctrl+T", "Typing keyboard to piano"),
                    ("Ctrl+= · Ctrl+- · Ctrl+0", "Interface size"),
                    ("F1", "This list"),
                ];
                egui::ScrollArea::vertical()
                    .max_height(max_h - 60.0)
                    .show(ui, |ui| {
                        egui::Grid::new("keys")
                            .num_columns(2)
                            .spacing(vec2(24.0, 4.0))
                            .striped(true)
                            .show(ui, |ui| {
                                for (k, what) in rows.iter().filter(|(k, w)| {
                                    want.is_empty()
                                        || k.to_lowercase().contains(&want)
                                        || w.to_lowercase().contains(&want)
                                }) {
                                    ui.label(RichText::new(*k).font(t::mono(12.0)).color(t::TEXT));
                                    ui.label(
                                        RichText::new(*what).font(t::sans(13.0)).color(t::MUTED),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.show_shortcuts = open;
        self.about_window(ctx);
    }

    /// About dialog (HELP → About KeyFrameXL). The open flag lives in egui
    /// memory, so no App field is needed. Drawn from `shortcuts_window`,
    /// which already runs every frame.
    fn about_window(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("about-open");
        if !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
            return;
        }
        let mut open = true;
        let mut close = false;
        let audio = self.audio.as_ref().map(|a| match a.running.buffer {
            Some(b) => format!("Audio: {} Hz · buffer {b}", a.running.rate),
            None => format!("Audio: {} Hz · buffer default", a.running.rate),
        });
        egui::Window::new("About KeyFrameXL")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(vec2(48.0, 48.0), egui::Sense::hover());
                    kit::icon_brand(ui.painter(), icon_rect);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(
                                RichText::new("KeyFrame")
                                    .font(t::semibold(17.0))
                                    .color(t::TEXT),
                            );
                            ui.label(
                                RichText::new("XL")
                                    .font(t::mono_medium(13.0))
                                    .color(t::BRAND),
                            );
                        });
                        ui.label(
                            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .font(t::mono(12.0))
                                .color(t::MUTED),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Pattern-based music sequencer for Linux.")
                        .font(t::sans(13.0))
                        .color(t::TEXT),
                );
                if let Some(line) = audio {
                    ui.label(RichText::new(line).font(t::mono(12.0)).color(t::MUTED));
                }
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        ctx.data_mut(|d| d.insert_temp(id, open));
    }

    /// Pattern rename popup (F2).
    pub(super) fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((id, mut text)) = self.renaming.take() else {
            return;
        };
        let mut done = false;
        let mut keep = true;
        egui::Window::new("Rename pattern")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 140.0))
            .show(ctx, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .font(t::sans(14.0))
                        .desired_width(260.0),
                );
                r.request_focus();
                let (enter, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                ui.horizontal(|ui| {
                    if kit::button(ui, "Rename", kit::Kind::Primary, !text.trim().is_empty())
                        .clicked()
                        || (enter && !text.trim().is_empty())
                    {
                        done = true;
                    }
                    if kit::button(ui, "Cancel", kit::Kind::Normal, true).clicked() || esc {
                        keep = false;
                    }
                });
            });
        if done {
            self.checkpoint();
            if let Some(p) = self.song.patterns.iter_mut().find(|p| p.id == id) {
                p.name = text.trim().to_string();
            }
            self.say(
                Tone::Quiet,
                format!("Pattern renamed to {}. Ctrl+Z undoes.", text.trim()),
            );
        } else if keep {
            self.renaming = Some((id, text));
        }
    }
}

/// Which window kinds the deck's window buttons show, with their keys.
pub(super) const WINDOW_KEYS: [(Kind, &str); 4] = [
    (Kind::Playlist, "F5"),
    (Kind::Rack, "F6"),
    (Kind::Roll, "F7"),
    (Kind::Mixer, "F9"),
];
