//! Instrument window (FL: channel settings / plugin window). The Sampler is our
//! own instrument; a DirectWave .dwp is one preset format it loads, next to
//! the formats and plugin hosts that come later.

use super::{App, Cmd, Slot, Tab, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Rect, Sense, Ui, vec2};
use keyframexl::sequence::{DRUM_VOICES, InstrumentKind};

/// Files the sampler loads today: DirectWave presets and audio. Extend here, not with new menu items.
pub const PRESET_EXTS: [&str; 4] = ["dwp", "wav", "flac", "ogg"];

impl App {
    pub(crate) fn add_sampler(&mut self) -> u64 {
        self.checkpoint();
        let name = self.song.next_sampler_name();
        let id = self.song.add_sampler(&name);
        self.publish();
        self.win(Cmd::Show(Tab::Instrument { id }));
        self.say(
            Tone::Quiet,
            format!("{name} added. Load a preset into it, or draw notes. Ctrl+Z undoes."),
        );
        id
    }

    /// Add a built-in drum voice channel (kick, snare, hat).
    pub(crate) fn add_drum(&mut self, voice: u8) -> u64 {
        self.checkpoint();
        let name = self.song.free_name(DRUM_VOICES[voice as usize]);
        let id = self.song.add_channel(&name, InstrumentKind::Drum { voice });
        self.publish();
        self.say(
            Tone::Quiet,
            format!("{name} added. Click its steps. Ctrl+Z undoes."),
        );
        id
    }

    /// FL: Add → channel. The instrument list grows with plugin hosting.
    pub(crate) fn add_channel_menu(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        if ui
            .button("Sampler")
            .on_hover_text("Our sampler: loads DirectWave .dwp presets")
            .clicked()
        {
            self.add_sampler();
            ui.close_menu();
        }
        if ui.button("KeyFrame Tide (synth)").clicked() {
            self.checkpoint();
            let name = self.song.free_name("KeyFrame Tide");
            let id = self.song.add_channel(
                &name,
                InstrumentKind::Synth {
                    patch: Default::default(),
                },
            );
            self.publish();
            self.win(Cmd::Show(Tab::Instrument { id }));
            ui.close_menu();
        }
        if ui
            .button("Forge (game sounds)")
            .on_hover_text("Build jumps, coins, hits and lasers from layers, then export them for your game")
            .clicked()
        {
            self.add_forge();
            ui.close_menu();
        }
        if ui
            .button("Vocal (singing voice)")
            .on_hover_text("Notes with lyrics, sung from a UTAU voicebank such as Kasane Teto")
            .clicked()
        {
            self.add_vocal();
            ui.close_menu();
        }
        for plugin in self.plugin_choices(true) {
            if ui.button(&plugin.name).clicked() {
                self.add_plugin_channel(&plugin);
                ui.close_menu();
            }
        }
        if ui.button("More plugins…").clicked() {
            Self::toggle_picker(ui.ctx());
            ui.close_menu();
        }
        ui.separator();
        for (v, name) in DRUM_VOICES.iter().enumerate() {
            if ui.button(format!("Drum voice · {name}")).clicked() {
                self.add_drum(v as u8);
                ui.close_menu();
            }
        }
    }

    /// Usable VST3 instruments from the scan (Settings → Plugins lists the rest).
    pub(crate) fn plugin_instrument_menu(&mut self, ui: &mut Ui) {
        kit::menu_style(ui);
        let list = self.plugin_choices(true);
        if list.is_empty() {
            let note = if self.plugins.scan_rx.is_some() {
                "Checking plugins…"
            } else {
                "No instrument plugins found"
            };
            kit::menu_header(
                ui,
                egui::RichText::new(note)
                    .font(t::sans(13.0))
                    .color(t::MUTED),
            );
        }
        for s in list {
            let hover = format!("{} · {}", s.vendor, s.path.display());
            if ui.button(&s.name).on_hover_text(hover).clicked() {
                self.add_plugin_channel(&s);
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("Plugin settings…").clicked() {
            self.open_settings(super::settings_ui::Page::Plugins);
            ui.close_menu();
        }
    }

    pub(crate) fn delete_channel(&mut self, id: u64) {
        let name = self.instrument_name(id);
        // A plugin's sound settings go into the undo step, so Ctrl+Z restores them.
        if self
            .song
            .instrument(id)
            .is_some_and(|i| i.plugin().is_some())
        {
            self.collect_plugin_states();
        }
        self.checkpoint();
        match self.song.remove_instrument(id) {
            Ok(()) => {
                self.publish();
                self.say(
                    Tone::Quiet,
                    format!("{name} and its notes deleted. Ctrl+Z undoes."),
                );
            }
            Err(e) => {
                self.history.pop();
                self.say(Tone::Bad, e);
            }
        }
    }

    /// Ask for a preset file and load it into sampler `id` in the background.
    pub(crate) fn load_preset_dialog(&mut self, id: u64) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Sampler preset or sample (.dwp, .wav, .flac, .ogg)",
                &PRESET_EXTS,
            )
            .pick_file()
        {
            self.load_preset(id, path);
        }
    }

    pub(crate) fn load_preset(&mut self, id: u64, path: std::path::PathBuf) {
        self.load_preset_with(id, path, false);
    }

    /// Load in the background; `quiet` = part of the default kit (no undo
    /// step, no status, the project stays saved).
    pub(crate) fn load_preset_with(&mut self, id: u64, path: std::path::PathBuf, quiet: bool) {
        // Each .dwp starts a converter process: run three at a time, queue the rest.
        if self.importing.len() >= 3 {
            self.load_queue.retain(|(t, _, _)| *t != id);
            self.load_queue.push_back((id, path, quiet));
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        std::thread::spawn(move || {
            let audio = super::files::is_audio(&path);
            let _ = tx.send(if audio {
                crate::import::sample_preset(&path)
            } else {
                crate::import::dwp(&path)
            });
        });
        self.importing.retain(|(t, _, _)| *t != id);
        self.importing.push((id, rx, quiet));
        if !quiet {
            self.say(
                Tone::Wait,
                format!("Loading {file} into {}", self.instrument_name(id)),
            );
        }
    }

    /// Files dropped on the window: presets go to the focused sampler (or a new
    /// one), a project file opens.
    pub(crate) fn dropped_files(&mut self, ctx: &egui::Context) {
        let files: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if files.is_empty() {
            return;
        }
        let ppp = ctx.pixels_per_point();
        self.drop_pointer = self
            .plugins
            .parent
            .and_then(crate::plugins::pointer_in)
            .map(|(x, y)| egui::pos2(x as f32 / ppp, y as f32 / ppp))
            .or_else(|| ctx.input(|i| i.pointer.latest_pos()));
        for path in files {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if PRESET_EXTS.contains(&ext.as_str()) {
                let target = match &self.focus {
                    Some(Tab::Instrument { id }) | Some(Tab::Roll { instrument: id })
                        if self.is_sampler(*id) =>
                    {
                        *id
                    }
                    _ => self.add_sampler(),
                };
                self.load_preset(target, path);
            } else if ext == "kfxl" || ext == "flp" {
                self.request(super::Pending::Open(Some(path)));
            } else if ext == "mid" || ext == "midi" {
                // A clip dragged out of a plugin (or a file) onto a channel's work:
                // its notes go to that channel; otherwise a full import.
                match self.midi_drop_channel() {
                    Some(ch) => self.drop_midi_notes(&path, ch),
                    None => self.import_midi(&path),
                }
            } else if keyframexl::preset::Kind::of_path(&path).is_some() {
                self.open_preset_file(&path);
            } else {
                self.say(Tone::Bad, format!("{} is not a sample, preset or project. Drop .wav, .flac, .ogg, .dwp or .kfxl.", path.display()));
            }
        }
    }

    /// Where a dropped preset would go, for the drop overlay.
    pub(crate) fn drop_target_name(&self) -> String {
        match &self.focus {
            Some(Tab::Instrument { id }) | Some(Tab::Roll { instrument: id })
                if self.is_sampler(*id) =>
            {
                self.instrument_name(*id)
            }
            _ => "a new sampler".into(),
        }
    }

    pub(crate) fn is_sampler(&self, id: u64) -> bool {
        self.song
            .instruments
            .iter()
            .any(|i| i.id == id && matches!(i.kind, InstrumentKind::Sampler { .. }))
    }

    pub(super) fn instrument_window(&mut self, ui: &mut Ui, id: u64) {
        let Some(inst) = self.song.instruments.iter().find(|i| i.id == id).cloned() else {
            ui.label("This channel no longer exists.");
            return;
        };
        let ch = self
            .song
            .instruments
            .iter()
            .position(|i| i.id == id)
            .unwrap_or(0)
            + 1;
        let sampler = matches!(inst.kind, InstrumentKind::Sampler { .. });
        let has_sound = matches!(&inst.kind, InstrumentKind::Sampler { preset: Some(_) });
        let kind = match &inst.kind {
            InstrumentKind::Sampler { .. } => "sampler",
            InstrumentKind::Plugin { .. } => "plugin",
            InstrumentKind::Drum { .. } => "drum voice",
            InstrumentKind::Synth { .. } => "KeyFrame Tide",
            InstrumentKind::Vocal { .. } => "vocal",
            InstrumentKind::Forge { .. } => "forge",
        };
        let sub = format!("{kind}  ·  ch {ch:02}");
        // A plugin channel is FL's wrapper: one compact bar, then the plugin.
        if let InstrumentKind::Plugin { plugin } = &inst.kind {
            ui.add_space(6.0);
            self.plugin_panel(ui, id, plugin);
            return;
        }
        let loading = self.importing.iter().any(|(t, _, _)| *t == id);
        App::panel_header(ui, &sub, |ui, slot| {
            if slot == Slot::Menu {
                if sampler
                    && ui
                        .add_enabled(!loading, egui::Button::new("Load sound (.dwp, sample)…"))
                        .clicked()
                {
                    self.load_preset_dialog(id);
                    ui.close_menu();
                }
                if ui.button("Piano roll").clicked() {
                    self.open_roll(id);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save channel preset…").clicked() {
                    ui.close_menu();
                    self.save_channel_preset(id);
                }
                if ui.button("Load channel preset…").clicked() {
                    ui.close_menu();
                    self.load_preset_dialog_for(super::presets_ui::Target::Channel(id));
                }
                ui.separator();
                if ui.button("Delete channel").clicked() {
                    self.delete_channel(id);
                    ui.close_menu();
                }
                self.window_menu_tail(ui, Tab::Instrument { id });
                return;
            }
            // The empty sampler shows its own big Load button; one is enough.
            if sampler && (has_sound || loading) {
                let label = if loading {
                    "Loading"
                } else {
                    "Replace sound…"
                };
                if kit::button(ui, label, kit::Kind::Normal, !loading)
                    .on_hover_text("Load a DirectWave .dwp or a sample into this sampler. You can also drop the file on the window.")
                    .clicked()
                {
                    self.load_preset_dialog(id);
                }
            }
            if kit::button(ui, "Piano roll", kit::Kind::Normal, true)
                .on_hover_text("Edit this channel's notes  F7")
                .clicked()
            {
                self.open_roll(id);
            }
        });

        // Name: edit in place, one undo step per edit.
        let mut name = inst.name.clone();
        let r = ui.add(
            egui::TextEdit::singleline(&mut name)
                .font(t::semibold(20.0))
                .frame(false)
                .desired_width(ui.available_width().min(420.0))
                .hint_text("Channel name"),
        );
        if r.changed() && !name.trim().is_empty() {
            // One undo step per rename, made by its first keystroke (focus alone changes nothing).
            if super::kit::first_edit(ui, &r) {
                self.checkpoint();
            }
            if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == id) {
                i.name = name.trim().to_string();
            }
            self.publish();
        }
        super::kit::end_edit(ui, &r);
        r.on_hover_text("Click to rename");
        ui.add_space(6.0);

        if let InstrumentKind::Synth { patch } = &inst.kind {
            self.synth_editor(ui, id, patch);
            return;
        }
        if let InstrumentKind::Vocal { voice } = &inst.kind {
            self.vocal_editor(ui, id, voice);
            return;
        }
        if let InstrumentKind::Forge { sound } = &inst.kind {
            self.forge_editor(ui, id, sound);
            return;
        }

        // A file from the browser dropped on the window loads into this channel.
        let whole = ui.interact(
            ui.max_rect(),
            ui.id().with(("inst-drop", id)),
            Sense::hover(),
        );
        if let Some(drag) = whole.dnd_release_payload::<super::files::FileDrag>() {
            self.drop_file_on_channel(id, drag.0.clone());
        }
        let InstrumentKind::Sampler { preset } = &inst.kind else {
            ui.label(egui::RichText::new("Built-in drum voice. Its steps live in the channel rack and the piano roll; sound parameters arrive with the parameter system.").color(t::MUTED));
            return;
        };
        let Some(preset) = preset.clone() else {
            self.empty_sampler(ui, id, loading);
            return;
        };

        self.sampler_editor(ui, id, &preset);
    }

    /// A plugin channel: FL's wrapper window (the plugin, its parameters).
    fn plugin_panel(&mut self, ui: &mut Ui, id: u64, plugin: &keyframexl::sequence::PluginRef) {
        self.plugins.tab = Some(Tab::Instrument { id });
        self.plugin_wrapper(ui, plugin);
        self.plugins.tab = None;
    }

    fn empty_sampler(&mut self, ui: &mut Ui, id: u64, loading: bool) {
        let h = ui.available_height().clamp(200.0, 260.0);
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        let p = ui.painter();
        p.rect_filled(area, 8.0, t::SMP_CHASSIS);
        p.rect_stroke(
            area,
            8.0,
            egui::Stroke::new(1.0_f32, t::SMP_LINE),
            egui::StrokeKind::Inside,
        );
        // Subtle brass top rail matching the loaded DWP Sampler console.
        p.line_segment(
            [
                egui::pos2(area.left() + 14.0, area.top() + 1.0),
                egui::pos2(area.right() - 14.0, area.top() + 1.0),
            ],
            egui::Stroke::new(2.0_f32, t::SMP_GOLD.gamma_multiply(0.45)),
        );
        let well = area.shrink2(vec2(14.0, 14.0));
        p.rect_filled(well, 6.0, t::SMP_WELL);
        p.rect_stroke(
            well,
            6.0,
            egui::Stroke::new(1.0_f32, t::SMP_LINE),
            egui::StrokeKind::Inside,
        );

        let c = well.center();
        let mark_r = Rect::from_center_size(c - vec2(0.0, 52.0), vec2(42.0, 26.0));
        super::sampler::paint_sampler_mark(p, mark_r);

        if loading {
            p.text(
                c + vec2(0.0, 4.0),
                Align2::CENTER_CENTER,
                "Loading sound…",
                t::medium(15.0),
                t::SMP_GOLD,
            );
            return;
        }

        p.text(
            c - vec2(0.0, 16.0),
            Align2::CENTER_CENTER,
            "DWP SAMPLER  ·  NO SAMPLE LOADED",
            t::semibold(13.0),
            t::SMP_GOLD,
        );
        p.text(
            c + vec2(0.0, 6.0),
            Align2::CENTER_CENTER,
            "Drop a DirectWave .dwp preset or audio file (.wav, .flac, .ogg) anywhere in this window.",
            t::sans(13.0),
            t::SMP_SAND,
        );
        let b = Rect::from_center_size(c + vec2(0.0, 44.0), vec2(170.0, kit::CONTROL_H));
        let r = ui.interact(b, ui.id().with(("load-empty", id)), Sense::click());
        kit::paint_button(
            ui,
            b,
            &r,
            kit::Kind::Normal,
            true,
            "Load sound",
            |p, face, ink| {
                p.text(
                    face.center(),
                    Align2::CENTER_CENTER,
                    "Load sound…",
                    t::sans(14.0),
                    ink,
                );
            },
        );
        if r.clicked() {
            self.load_preset_dialog(id);
        }
    }
}

pub(super) fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
