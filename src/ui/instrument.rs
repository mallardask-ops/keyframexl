//! Instrument window (FL: channel settings / plugin window). The Sampler is our
//! own instrument; a DirectWave .dwp is one preset format it loads, next to
//! the formats and plugin hosts that come later.

use super::{App, Cmd, Slot, Tab, Tone, kit};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Align2, Rect, Sense, Ui, vec2};
use keyframexl::sequence::{DRUM_VOICES, InstrumentKind};

/// Audio formats the sampler and playlist audio clips load.
pub const AUDIO_EXTS: [&str; 8] = ["wav", "flac", "ogg", "mp3", "m4a", "aac", "aif", "aiff"];
/// Files the sampler loads today: DirectWave presets and audio. Extend here, not with new menu items.
pub const PRESET_EXTS: [&str; 9] = [
    "dwp", "wav", "flac", "ogg", "mp3", "m4a", "aac", "aif", "aiff",
];

impl App {
    pub(crate) fn add_sampler(&mut self) -> u64 {
        self.checkpoint();
        let name = self.song.next_sampler_name();
        let id = self.song.add_sampler(&name);
        self.set_lead_channel(id);
        self.publish();
        self.win(Cmd::Show(Tab::Instrument { id }));
        let msg = fill(self.tr().inst_sampler_added, &[("name", &name)]);
        self.say(Tone::Quiet, msg);
        id
    }

    /// Add a built-in drum voice channel (kick, snare, hat).
    pub(crate) fn add_drum(&mut self, voice: u8) -> u64 {
        self.checkpoint();
        let name = self.song.free_name(DRUM_VOICES[voice as usize]);
        let id = self.song.add_channel(&name, InstrumentKind::Drum { voice });
        self.set_lead_channel(id);
        self.publish();
        let msg = fill(self.tr().inst_drum_added, &[("name", &name)]);
        self.say(Tone::Quiet, msg);
        id
    }

    /// FL: Add → channel. The instrument list grows with plugin hosting.
    pub(crate) fn add_channel_menu(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        kit::menu_style(ui);
        if ui
            .button(tr.inst_menu_sampler)
            .on_hover_text(tr.inst_menu_sampler_tip)
            .clicked()
        {
            self.add_sampler();
            ui.close_menu();
        }
        if ui
            .button(tr.inst_menu_audio_clip)
            .on_hover_text(tr.inst_menu_audio_clip_tip)
            .clicked()
        {
            self.add_audio_clip_dialog();
            ui.close_menu();
        }
        if ui.button(tr.inst_menu_tide).clicked() {
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
            .button(tr.inst_menu_forge)
            .on_hover_text(tr.inst_menu_forge_tip)
            .clicked()
        {
            self.add_forge();
            ui.close_menu();
        }
        ui.menu_button(tr.inst_menu_drum_kit, |ui| {
            kit::menu_style(ui);
            for k in keyframexl::kits::Kit::ALL {
                if ui
                    .button(tr.drum_kit_label(k))
                    .on_hover_text(tr.inst_menu_drum_kit_tip)
                    .clicked()
                {
                    self.add_kit(k);
                    ui.close_menu();
                }
            }
        });
        if ui
            .button(tr.inst_menu_vocal)
            .on_hover_text(tr.inst_menu_vocal_tip)
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
        if ui.button(tr.inst_menu_more_plugins).clicked() {
            Self::toggle_picker(ui.ctx());
            ui.close_menu();
        }
        ui.separator();
        for (v, name) in DRUM_VOICES.iter().enumerate() {
            if ui
                .button(fill(tr.inst_menu_drum_voice, &[("name", name)]))
                .clicked()
            {
                self.add_drum(v as u8);
                ui.close_menu();
            }
        }
    }

    /// Usable VST3 instruments from the scan (Settings → Plugins lists the rest).
    pub(crate) fn plugin_instrument_menu(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        kit::menu_style(ui);
        let list = self.plugin_choices(true);
        if list.is_empty() {
            let note = if self.plugins.scan_rx.is_some() {
                tr.mixer_checking_plugins
            } else {
                tr.inst_no_plugins
            };
            kit::menu_header(
                ui,
                egui::RichText::new(note)
                    .font(t::sans(13.0))
                    .color(t::muted()),
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
        if ui.button(tr.mixer_plugin_settings).clicked() {
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
                let msg = fill(self.tr().inst_deleted, &[("name", &name)]);
                self.say(Tone::Quiet, msg);
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
            .add_filter(self.tr().inst_dlg_sampler_filter, &PRESET_EXTS)
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
            let name = self.instrument_name(id);
            let msg = fill(
                self.tr().inst_loading_into,
                &[("file", &file), ("name", &name)],
            );
            self.say(Tone::Wait, msg);
        }
    }

    /// Pick an audio/music file and place it as an audio clip on the playlist.
    pub(crate) fn add_audio_clip_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.tr().imp_filter_audio, &AUDIO_EXTS)
            .pick_file()
        {
            let track = self.next_free_audio_track();
            self.load_audio_clip(path, 0, track);
        }
    }

    /// First empty playlist track (or next track after the last occupied one)
    /// so imported/dropped music tracks stack neatly without overlapping.
    pub(crate) fn next_free_audio_track(&self) -> u32 {
        let mut t = 0u32;
        while t < 999 && self.song.clips.iter().any(|c| c.track == t) {
            t += 1;
        }
        t
    }

    /// Decode an audio file in the background and place it as an FL-style
    /// audio clip pattern on the playlist at `(start, track)`.
    pub(crate) fn load_audio_clip(
        &mut self,
        path: std::path::PathBuf,
        start: keyframexl::sequence::Tick,
        track: u32,
    ) {
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::import::sample_preset(&path));
        });
        self.importing_clips.push((start, track, rx));
        let msg = fill(self.tr().inst_loading_audio_clip, &[("file", &file)]);
        self.say(Tone::Wait, msg);
    }

    /// If an audio file dropped from the OS should become a playlist audio clip,
    /// return its `(start, track)` on the playlist.
    fn audio_clip_drop_pos(&self) -> Option<(keyframexl::sequence::Tick, u32)> {
        if let Some((grid_seen, grid_l, grid_top, bar_w, lane_h)) = self.playlist_drop {
            if let Some(pos) = self.drop_pointer
                && grid_seen.contains(pos)
            {
                let raw_tick = ((pos.x - grid_l) / bar_w * keyframexl::sequence::BAR as f32).floor()
                    as keyframexl::sequence::Tick;
                let start = super::playlist::song_snap(
                    &self.song.meters,
                    raw_tick,
                    self.snap.grid(),
                    false,
                    false,
                )
                .max(0);
                let track = ((pos.y - grid_top) / lane_h).floor().clamp(0.0, 999.0) as u32;
                return Some((start, track));
            }
            let pointer_on_channel = self
                .drop_pointer
                .is_some_and(|p| self.midi_zones.iter().any(|(r, _)| r.contains(p)));
            let sampler_focused = matches!(
                self.focus,
                Some(Tab::Instrument { id }) | Some(Tab::Roll { instrument: id })
                    if self.is_sampler(id)
            );
            if matches!(self.focus, Some(Tab::Playlist))
                || (!pointer_on_channel && !sampler_focused)
            {
                return Some((0, self.next_free_audio_track()));
            }
        }
        None
    }

    /// Files dropped on the window: audio files dropped onto the playlist (or
    /// generically when the playlist is open) become audio clips; presets go to
    /// the focused sampler (or a new one); a project file opens.
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
        let mut clip_drop = self.audio_clip_drop_pos();
        for path in files {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if AUDIO_EXTS.contains(&ext.as_str())
                && let Some((start, track)) = clip_drop
            {
                self.load_audio_clip(path, start, track);
                clip_drop = Some((start, (track + 1).min(999)));
            } else if PRESET_EXTS.contains(&ext.as_str()) {
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
                let path_str = path.display().to_string();
                let msg = fill(self.tr().inst_drop_unsupported, &[("path", &path_str)]);
                self.say(Tone::Bad, msg);
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
            _ => self.tr().inst_drop_new_sampler.into(),
        }
    }

    pub(crate) fn is_sampler(&self, id: u64) -> bool {
        self.song
            .instruments
            .iter()
            .any(|i| i.id == id && matches!(i.kind, InstrumentKind::Sampler { .. }))
    }

    pub(super) fn instrument_window(&mut self, ui: &mut Ui, id: u64) {
        let tr = self.tr();
        let Some(inst) = self.song.instruments.iter().find(|i| i.id == id).cloned() else {
            ui.label(tr.inst_gone);
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
            InstrumentKind::Sampler { .. } => tr.inst_kind_sampler,
            InstrumentKind::Plugin { .. } => tr.inst_kind_plugin,
            InstrumentKind::Drum { .. } => tr.inst_kind_drum,
            InstrumentKind::Synth { .. } => tr.inst_kind_synth,
            InstrumentKind::Vocal { .. } => tr.inst_kind_vocal,
            InstrumentKind::Forge { .. } => tr.inst_kind_forge,
        };
        let ch_str = format!("{ch:02}");
        let sub = fill(tr.inst_sub_ch, &[("kind", kind), ("ch", &ch_str)]);
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
                        .add_enabled(!loading, egui::Button::new(tr.inst_load_sound_menu))
                        .clicked()
                {
                    self.load_preset_dialog(id);
                    ui.close_menu();
                }
                if ui.button(tr.win_roll).clicked() {
                    self.open_roll(id);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr.inst_save_ch_preset).clicked() {
                    ui.close_menu();
                    self.save_channel_preset(id);
                }
                if ui.button(tr.inst_load_ch_preset).clicked() {
                    ui.close_menu();
                    self.load_preset_dialog_for(super::presets_ui::Target::Channel(id));
                }
                ui.separator();
                if ui.button(tr.inst_delete_channel).clicked() {
                    self.delete_channel(id);
                    ui.close_menu();
                }
                self.window_menu_tail(ui, Tab::Instrument { id });
                return;
            }
            // The empty sampler shows its own big Load button; one is enough.
            if sampler && (has_sound || loading) {
                let label = if loading {
                    tr.inst_loading_btn
                } else {
                    tr.inst_replace_sound
                };
                if kit::button(ui, label, kit::Kind::Normal, !loading)
                    .on_hover_text(tr.inst_replace_sound_tip)
                    .clicked()
                {
                    self.load_preset_dialog(id);
                }
            }
            if kit::button(ui, tr.win_roll, kit::Kind::Normal, true)
                .on_hover_text(tr.inst_roll_btn_tip)
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
                .hint_text(tr.inst_ch_name_hint),
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
        r.on_hover_text(tr.inst_click_rename);
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
            ui.label(egui::RichText::new(tr.inst_drum_desc).color(t::muted()));
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
        let tr = self.tr();
        let _dark = t::Faceplate::enter(ui);
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
                tr.inst_loading_sound,
                t::medium(15.0),
                t::SMP_GOLD,
            );
            return;
        }

        p.text(
            c - vec2(0.0, 16.0),
            Align2::CENTER_CENTER,
            tr.inst_empty_sampler_head,
            t::semibold(13.0),
            t::SMP_GOLD,
        );
        p.text(
            c + vec2(0.0, 6.0),
            Align2::CENTER_CENTER,
            tr.inst_empty_sampler_desc,
            t::sans(13.0),
            t::SMP_SAND,
        );
        let btn_w = (kit::text_width(p, tr.inst_load_sound_btn, t::sans(14.0)) + 40.0).max(170.0);
        let b = Rect::from_center_size(c + vec2(0.0, 44.0), vec2(btn_w, kit::CONTROL_H));
        let r = ui.interact(b, ui.id().with(("load-empty", id)), Sense::click());
        kit::paint_button(
            ui,
            b,
            &r,
            kit::Kind::Normal,
            true,
            tr.inst_load_sound_a11y,
            |p, face, ink| {
                p.text(
                    face.center(),
                    Align2::CENTER_CENTER,
                    tr.inst_load_sound_btn,
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
