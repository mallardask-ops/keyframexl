//! Project lifecycle around the file commands: ask before unsaved work is
//! replaced or the window closes, remember recent projects, keep the window
//! title on the project name.

use super::{App, Tone, kit};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, RichText, vec2};
use std::path::{Path, PathBuf};

pub const RECENT_MAX: usize = 10;

/// A command that would drop unsaved changes; it waits for an answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    New,
    /// Open this project, or ask for one with the file dialog.
    Open(Option<PathBuf>),
    Quit,
}

pub fn autosave_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("keyframexl").join("autosave.kfxl"))
}

/// Unsaved work goes to the autosave file this often (seconds).
const AUTOSAVE_EVERY: f64 = 120.0;
const AUTOSAVE_DEFAULT_MINUTES: u32 = 2;

fn recent_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("keyframexl").join("recent.ron"))
}

pub fn load_recent() -> Vec<PathBuf> {
    recent_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| ron::from_str::<Vec<PathBuf>>(&s).ok())
        .unwrap_or_default()
}

impl App {
    /// Run `cmd` now, or ask first when it would drop unsaved changes.
    pub(crate) fn request(&mut self, cmd: Pending) {
        if self.dirty() && self.settings.general.confirm_close {
            self.pending = Some(cmd);
        } else {
            self.perform(cmd);
        }
    }

    fn perform(&mut self, cmd: Pending) {
        let tr = self.tr();
        match cmd {
            Pending::New => self.new_project(),
            Pending::Open(None) => self.open(),
            Pending::Open(Some(path)) => {
                self.open_path(&path);
                if Some(&path) == autosave_path().as_ref() && self.path.as_ref() == Some(&path) {
                    // A recovered project is unsaved work, not a file to save over.
                    self.path = None;
                    self.saved_edits = self.edits.wrapping_sub(1);
                    self.recent.retain(|p| p != &path);
                    self.save_recent();
                    self.say(Tone::Done, tr.proj_recovered);
                }
            }
            Pending::Quit => self.quit_ok = true,
        }
    }

    pub(crate) fn project_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.tr().untitled.into())
    }

    /// Move `path` to the top of Open recent.
    pub(crate) fn remember(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if Some(&path) == autosave_path().as_ref() {
            return;
        }
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(RECENT_MAX);
        self.save_recent();
    }

    pub(crate) fn clear_recent(&mut self) {
        self.recent.clear();
        self.save_recent();
    }

    fn save_recent(&self) {
        if !self.persist {
            return;
        }
        if let Some(path) = recent_path()
            && let Ok(text) = ron::ser::to_string_pretty(&self.recent, Default::default())
        {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
            let _ = std::fs::write(path, text);
        }
    }

    /// Window close: hold it while there are unsaved changes; title upkeep.
    pub(crate) fn project_frame(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_ok
            && self.dirty()
            && self.settings.general.confirm_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        if self.quit_ok {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.autosave(ctx.input(|i| i.time));
        let title = format!(
            "{}{} — KeyFrameXL",
            self.project_name(),
            if self.dirty() { " *" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        self.unsaved_window(ctx);
        self.info_window(ctx);
    }

    /// Open Project info (e.g. from Export dialog or show_on_open).
    pub(crate) fn open_info(&self, ctx: &egui::Context) {
        let id = egui::Id::new("kfxl-project-info");
        ctx.data_mut(|d| d.insert_temp(id, true));
    }

    /// Open or close Project info (F11, FILE → Project info).
    pub(crate) fn toggle_info(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("kfxl-project-info");
        let open = ctx.data(|d| d.get_temp(id).unwrap_or(false));
        ctx.data_mut(|d| d.insert_temp(id, !open));
    }

    /// Metadata, session notes, and project musical settings (FL: Project info).
    /// Text edits collapse into one undo step per field; MP3 exports embed
    /// populated metadata as ID3v2.4 frames.
    fn info_window(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        let id = egui::Id::new("kfxl-project-info");
        let mut open = ctx.data(|d| d.get_temp(id).unwrap_or(false));
        if self.open_info_next {
            self.open_info_next = false;
            open = true;
            ctx.data_mut(|d| d.insert_temp(id, true));
        }
        if !open {
            return;
        }
        let tab_id = egui::Id::new("kfxl-project-info-tab");
        let mut tab = ctx
            .data(|d| d.get_temp::<usize>(tab_id).unwrap_or(0))
            .min(2);

        let project_stem = self.project_name();
        let file_label = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());
        let stats = ProjectStats::collect(self);

        let mut open_url: Option<String> = None;
        let mut open_export = false;
        let mut set_meter_req: Option<(u8, u8)> = None;
        let mut set_game_style: Option<Option<keyframexl::forge::recipes::Style>> = None;
        let mut set_scale_req: Option<Option<keyframexl::scale::Scale>> = None;
        let mut close_now = false;

        let screen = ctx.screen_rect();
        let max_win = vec2(
            (screen.width() - 40.0).max(420.0),
            (screen.height() - 40.0).max(360.0),
        );
        let default_pos = egui::pos2(
            (screen.center().x - 250.0).max(24.0),
            (screen.top() + 80.0).max(24.0),
        );

        egui::Window::new(tr.info_window_title)
            .id(egui::Id::new("project-info-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(500.0, 560.0))
            .min_size(vec2(420.0, 360.0))
            .max_size(max_win)
            .default_pos(default_pos)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                let win_w = ui.available_width().max(320.0);

                // Compact project header: elided title + optional filename + live stats.
                let heading = if self.song.info.title.trim().is_empty() {
                    project_stem.as_str()
                } else {
                    self.song.info.title.trim()
                };
                ui.allocate_ui_with_layout(
                    vec2(win_w, 22.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let file_shown = file_label.as_deref().map(|f| {
                            kit::elide(ui.painter(), f, t::mono(12.0), (win_w * 0.38).max(80.0))
                        });
                        let file_w = file_shown.as_deref().map_or(0.0, |f| {
                            kit::text_width(ui.painter(), f, t::mono(12.0)) + 12.0
                        });
                        let title_shown = kit::elide(
                            ui.painter(),
                            heading,
                            t::semibold(16.0),
                            (win_w - file_w).max(120.0),
                        );
                        ui.label(
                            RichText::new(title_shown)
                                .font(t::semibold(16.0))
                                .color(t::text()),
                        );
                        if let Some(file) = file_shown {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(file).font(t::mono(12.0)).color(t::faint()),
                                    );
                                },
                            );
                        }
                    },
                );

                ui.allocate_ui_with_layout(
                    vec2(win_w, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_max_width(win_w);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
                            let summary = format!(
                                "{} · {} · {} · {} · {}",
                                tr.info_stat_channels.fmt(tr.lang, stats.channels),
                                tr.info_stat_patterns.fmt(tr.lang, stats.patterns),
                                tr.info_stat_clips.fmt(tr.lang, stats.clips),
                                tr.info_stat_bars.fmt(tr.lang, stats.bars as usize),
                                format_duration(stats.seconds),
                            );
                            ui.label(RichText::new(summary).font(t::sans(12.0)).color(t::muted()));
                            if stats.plugins > 0 {
                                let p_text = if stats.missing_plugins > 0 {
                                    format!(
                                        "· {} ({})",
                                        tr.info_stat_plugins.fmt(tr.lang, stats.plugins),
                                        tr.info_stat_missing_plugins
                                            .fmt(tr.lang, stats.missing_plugins),
                                    )
                                } else {
                                    format!(
                                        "· {}",
                                        tr.info_stat_plugins.fmt(tr.lang, stats.plugins)
                                    )
                                };
                                ui.label(RichText::new(p_text).font(t::sans(12.0)).color(
                                    if stats.missing_plugins > 0 {
                                        t::wait()
                                    } else {
                                        t::muted()
                                    },
                                ));
                            }
                        });
                    },
                );

                ui.add_space(2.0);
                let tabs = [tr.info_tab_details, tr.info_tab_notes, tr.info_tab_settings];
                let (picked, _) = kit::chips(ui, "project-info-tabs", &tabs, tab, None);
                if let Some(i) = picked {
                    tab = i;
                }
                ui.separator();

                // Reserve exact footer height (separator + spacing + 26px Close button)
                // so the ScrollArea never pushes the resizable window taller.
                const FOOTER_RESERVE: f32 = 46.0;
                let body_h = (ui.available_height() - FOOTER_RESERVE).max(120.0);

                let mut info = self.song.info.clone();
                let mut info_text_changed = false;
                let mut info_first_edit = false;
                let mut info_discrete_changed = false;
                let mut edit_responses: Vec<egui::Response> = Vec::new();

                ui.allocate_ui_with_layout(
                    vec2(win_w, body_h),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("project-info-scroll", tab))
                            .max_height(body_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let w = (ui.available_width() - 8.0).max(240.0);
                                ui.set_max_width(w);
                                match tab {
                                    0 => {
                                        // Title + optional "From file" fill when a file path is present.
                                        field_label(ui, tr.info_title);
                                        let can_fill = self.path.is_some()
                                            && info.title.trim() != project_stem.trim();
                                        if can_fill {
                                            let btn_w = tool_btn_width(ui, tr.info_from_file);
                                            let edit_w = (w - btn_w - 6.0).max(120.0);
                                            ui.horizontal(|ui| {
                                                let r = singleline_field(
                                                    ui,
                                                    &mut info.title,
                                                    &project_stem,
                                                    keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                                    edit_w,
                                                );
                                                if r.changed() {
                                                    info_text_changed = true;
                                                    info_first_edit |= kit::first_edit(ui, &r);
                                                }
                                                edit_responses.push(r);
                                                if kit::tool_button(
                                                    ui,
                                                    tr.info_from_file,
                                                    kit::Kind::Normal,
                                                    true,
                                                )
                                                .clicked()
                                                {
                                                    info.title = project_stem.clone();
                                                    info_discrete_changed = true;
                                                }
                                            });
                                        } else {
                                            let r = singleline_field(
                                                ui,
                                                &mut info.title,
                                                &project_stem,
                                                keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                                w,
                                            );
                                            if r.changed() {
                                                info_text_changed = true;
                                                info_first_edit |= kit::first_edit(ui, &r);
                                            }
                                            edit_responses.push(r);
                                        }
                                        ui.add_space(2.0);

                                        // Author / Artist
                                        field_label(ui, tr.info_author);
                                        let r = singleline_field(
                                            ui,
                                            &mut info.author,
                                            tr.info_author_hint,
                                            keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                            w,
                                        );
                                        if r.changed() {
                                            info_text_changed = true;
                                            info_first_edit |= kit::first_edit(ui, &r);
                                        }
                                        edit_responses.push(r);
                                        ui.add_space(2.0);

                                        // Album / EP + Year side by side (exact width = w)
                                        let col_gap = 8.0;
                                        let year_w = 88.0_f32.min(w * 0.28);
                                        let album_w = (w - year_w - col_gap).max(120.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = col_gap;
                                            ui.allocate_ui_with_layout(
                                                vec2(album_w, 0.0),
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    field_label(ui, tr.info_album);
                                                    let r = singleline_field(
                                                        ui,
                                                        &mut info.album,
                                                        tr.info_album_hint,
                                                        keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                                        album_w,
                                                    );
                                                    if r.changed() {
                                                        info_text_changed = true;
                                                        info_first_edit |= kit::first_edit(ui, &r);
                                                    }
                                                    edit_responses.push(r);
                                                },
                                            );
                                            ui.allocate_ui_with_layout(
                                                vec2(year_w, 0.0),
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    field_label(ui, tr.info_year);
                                                    let r = singleline_field(
                                                        ui,
                                                        &mut info.year,
                                                        tr.info_year_hint,
                                                        keyframexl::sequence::ProjectInfo::MAX_YEAR,
                                                        year_w,
                                                    );
                                                    if r.changed() {
                                                        info_text_changed = true;
                                                        info_first_edit |= kit::first_edit(ui, &r);
                                                    }
                                                    edit_responses.push(r);
                                                },
                                            );
                                        });
                                        ui.add_space(2.0);

                                        // Genre + preset picker
                                        field_label(ui, tr.info_genre);
                                        ui.horizontal(|ui| {
                                            let edit_w = (w - PRESET_BTN_W - 6.0).max(120.0);
                                            let r = singleline_field(
                                                ui,
                                                &mut info.genre,
                                                tr.info_genre_hint,
                                                keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                                edit_w,
                                            );
                                            if r.changed() {
                                                info_text_changed = true;
                                                info_first_edit |= kit::first_edit(ui, &r);
                                            }
                                            edit_responses.push(r);
                                            if let Some(pick) = preset_picker(
                                                ui,
                                                "project-info-genre-presets",
                                                &info.genre,
                                                GENRE_PRESETS,
                                                true,
                                            ) {
                                                info.genre = pick.into();
                                                info_discrete_changed = true;
                                            }
                                        });
                                        ui.add_space(2.0);

                                        // Copyright / License + preset picker
                                        field_label(ui, tr.info_copyright);
                                        ui.horizontal(|ui| {
                                            let edit_w = (w - PRESET_BTN_W - 6.0).max(120.0);
                                            let r = singleline_field(
                                                ui,
                                                &mut info.copyright,
                                                tr.info_copyright_hint,
                                                keyframexl::sequence::ProjectInfo::MAX_FIELD,
                                                edit_w,
                                            );
                                            if r.changed() {
                                                info_text_changed = true;
                                                info_first_edit |= kit::first_edit(ui, &r);
                                            }
                                            edit_responses.push(r);
                                            let license_presets = [
                                                "CC0 1.0",
                                                "CC BY 4.0",
                                                "CC BY-SA 4.0",
                                                "CC BY-NC 4.0",
                                                tr.info_license_all_rights,
                                            ];
                                            if let Some(pick) = preset_picker(
                                                ui,
                                                "project-info-license-presets",
                                                &info.copyright,
                                                &license_presets,
                                                false,
                                            ) {
                                                info.copyright = pick.into();
                                                info_discrete_changed = true;
                                            }
                                        });
                                        ui.add_space(2.0);

                                        // Website / Link + Open button
                                        field_label(ui, tr.info_url);
                                        let url_valid = is_http_url(&info.url);
                                        ui.horizontal(|ui| {
                                            let btn_w = tool_btn_width(ui, tr.info_url_open);
                                            let edit_w = (w - btn_w - 6.0).max(120.0);
                                            let r = singleline_field(
                                                ui,
                                                &mut info.url,
                                                tr.info_url_hint,
                                                keyframexl::sequence::ProjectInfo::MAX_URL,
                                                edit_w,
                                            );
                                            if r.changed() {
                                                info_text_changed = true;
                                                info_first_edit |= kit::first_edit(ui, &r);
                                            }
                                            edit_responses.push(r);
                                            if kit::tool_button(
                                                ui,
                                                tr.info_url_open,
                                                kit::Kind::Normal,
                                                url_valid,
                                            )
                                            .clicked()
                                            {
                                                open_url = Some(info.url.trim().to_string());
                                            }
                                        });
                                        if !info.url.trim().is_empty() && !url_valid {
                                            ui.label(
                                                RichText::new(tr.info_url_invalid)
                                                    .font(t::sans(12.0))
                                                    .color(t::wait()),
                                            );
                                        }

                                        ui.add_space(4.0);
                                        if ui
                                            .checkbox(
                                                &mut info.show_on_open,
                                                RichText::new(tr.info_show_on_open)
                                                    .font(t::sans(13.0))
                                                    .color(t::text()),
                                            )
                                            .on_hover_text(tr.info_show_on_open_hint)
                                            .changed()
                                        {
                                            info_discrete_changed = true;
                                        }

                                        ui.add_space(4.0);
                                        ui.separator();

                                        // Live ID3v2.4 tag preview header + recessed well
                                        ui.allocate_ui_with_layout(
                                            vec2(w, kit::TOOL_H),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let r = ui
                                                    .allocate_exact_size(
                                                        vec2(
                                                            (w - tool_btn_width(
                                                                ui,
                                                                tr.info_open_export,
                                                            ) - 8.0)
                                                                .max(100.0),
                                                            16.0,
                                                        ),
                                                        egui::Sense::hover(),
                                                    )
                                                    .0;
                                                kit::caps(
                                                    ui.painter(),
                                                    r.left_center(),
                                                    egui::Align2::LEFT_CENTER,
                                                    tr.info_tags_heading,
                                                    12.0,
                                                    t::faint(),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if kit::tool_button(
                                                            ui,
                                                            tr.info_open_export,
                                                            kit::Kind::Ghost,
                                                            true,
                                                        )
                                                        .clicked()
                                                        {
                                                            open_export = true;
                                                        }
                                                    },
                                                );
                                            },
                                        );

                                        let well_inner_w = (w - 22.0).max(160.0);
                                        egui::Frame::new()
                                            .fill(t::sunken())
                                            .stroke(egui::Stroke::new(1.0_f32, t::line()))
                                            .corner_radius(6.0)
                                            .inner_margin(egui::Margin::symmetric(10, 8))
                                            .show(ui, |ui| {
                                                ui.set_min_width(well_inner_w);
                                                ui.set_max_width(well_inner_w);
                                                let frames = preview_id3_frames(&info);
                                                if frames.is_empty() {
                                                    ui.label(
                                                        RichText::new(tr.info_tags_empty)
                                                            .font(t::sans(12.0))
                                                            .color(t::faint()),
                                                    );
                                                } else {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.spacing_mut().item_spacing =
                                                            vec2(10.0, 4.0);
                                                        for (code, val) in frames {
                                                            let mut job =
                                                                egui::text::LayoutJob::default();
                                                            job.append(
                                                                code,
                                                                0.0,
                                                                egui::TextFormat {
                                                                    font_id: t::mono(11.5),
                                                                    color: t::faint(),
                                                                    ..Default::default()
                                                                },
                                                            );
                                                            job.append(
                                                                &format!(" {val}"),
                                                                0.0,
                                                                egui::TextFormat {
                                                                    font_id: t::mono(11.5),
                                                                    color: t::text(),
                                                                    ..Default::default()
                                                                },
                                                            );
                                                            ui.label(job);
                                                        }
                                                    });
                                                }
                                            });
                                    }
                                    1 => {
                                        let chars = info.comments.chars().count();
                                        let lines = if info.comments.is_empty() {
                                            0
                                        } else {
                                            info.comments.lines().count()
                                        };
                                        let counter = format!(
                                            "{chars}/{} · {lines}L",
                                            keyframexl::sequence::ProjectInfo::MAX_COMMENTS
                                        );
                                        ui.allocate_ui_with_layout(
                                            vec2(w, kit::TOOL_H),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let counter_w = kit::text_width(
                                                    ui.painter(),
                                                    &counter,
                                                    t::mono(11.5),
                                                );
                                                let reserved = tool_btn_width(ui, tr.info_copy)
                                                    + tool_btn_width(ui, tr.info_clear)
                                                    + counter_w
                                                    + 24.0;
                                                let heading_r = ui
                                                    .allocate_exact_size(
                                                        vec2((w - reserved).max(60.0), 16.0),
                                                        egui::Sense::hover(),
                                                    )
                                                    .0;
                                                kit::caps_fit(
                                                    ui.painter(),
                                                    heading_r.left_center(),
                                                    egui::Align2::LEFT_CENTER,
                                                    tr.info_comments,
                                                    12.0,
                                                    t::faint(),
                                                    heading_r.width(),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if kit::tool_button(
                                                            ui,
                                                            tr.info_clear,
                                                            kit::Kind::Ghost,
                                                            !info.comments.is_empty(),
                                                        )
                                                        .clicked()
                                                        {
                                                            info.comments.clear();
                                                            info_discrete_changed = true;
                                                        }
                                                        if kit::tool_button(
                                                            ui,
                                                            tr.info_copy,
                                                            kit::Kind::Normal,
                                                            !info.comments.is_empty(),
                                                        )
                                                        .clicked()
                                                        {
                                                            ui.ctx()
                                                                .copy_text(info.comments.clone());
                                                        }
                                                        ui.label(
                                                            RichText::new(&counter)
                                                                .font(t::mono(11.5))
                                                                .color(t::faint()),
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                        ui.add_space(2.0);
                                        let notes_h = (body_h - kit::TOOL_H - 18.0).max(160.0);
                                        let c = ui.add_sized(
                                            vec2(w, notes_h),
                                            egui::TextEdit::multiline(&mut info.comments)
                                                .font(t::sans(13.0))
                                                .hint_text(tr.info_comments_hint)
                                                .char_limit(
                                                    keyframexl::sequence::ProjectInfo::MAX_COMMENTS,
                                                )
                                                .margin(egui::Margin::symmetric(8, 6)),
                                        );
                                        if c.changed() {
                                            info_text_changed = true;
                                            info_first_edit |= kit::first_edit(ui, &c);
                                        }
                                        edit_responses.push(c);
                                    }
                                    _ => {
                                        // 1. Tempo, Swing & Time signature
                                        let mut tempo = self.song.tempo;
                                        setting_row(ui, w, tr.info_set_tempo, |ui| {
                                            let r = kit::drag_value(
                                                ui,
                                                egui::DragValue::new(&mut tempo)
                                                    .range(40.0..=300.0)
                                                    .speed(0.25)
                                                    .fixed_decimals(1)
                                                    .suffix(" BPM"),
                                            );
                                            if r.changed() {
                                                if kit::first_edit(ui, &r) {
                                                    self.checkpoint();
                                                }
                                                self.song.tempo = tempo.clamp(40.0, 300.0);
                                                self.publish();
                                            }
                                            kit::end_edit(ui, &r);
                                        });
                                        if !self.song.tempo_map.is_empty() {
                                            ui.label(
                                                RichText::new(tr.info_set_tempo_map_note)
                                                    .font(t::sans(12.0))
                                                    .color(t::wait()),
                                            );
                                        }

                                        let mut swing_pct =
                                            (self.song.swing * 100.0).clamp(0.0, 60.0);
                                        setting_row(ui, w, tr.info_set_swing, |ui| {
                                            let r = kit::drag_value(
                                                ui,
                                                egui::DragValue::new(&mut swing_pct)
                                                    .range(0.0..=60.0)
                                                    .speed(0.25)
                                                    .fixed_decimals(0)
                                                    .suffix("%"),
                                            );
                                            if r.changed() {
                                                if kit::first_edit(ui, &r) {
                                                    self.checkpoint();
                                                }
                                                self.song.swing =
                                                    (swing_pct / 100.0).clamp(0.0, 0.6);
                                                self.publish();
                                            }
                                            kit::end_edit(ui, &r);
                                        });

                                        let (cur_num, cur_den) = self
                                            .song
                                            .meters
                                            .first()
                                            .filter(|m| m.bar == 0)
                                            .map(|m| (m.num, m.den))
                                            .unwrap_or((4, 4));
                                        let mut num = cur_num;
                                        let extra_meters =
                                            self.song.meters.iter().filter(|m| m.bar > 0).count();
                                        setting_row(ui, w, tr.info_set_meter, |ui| {
                                            let dens = [2u8, 4, 8, 16];
                                            let den_labels = ["2", "4", "8", "16"];
                                            let cur_den_idx = dens
                                                .iter()
                                                .position(|d| *d == cur_den)
                                                .unwrap_or(1);
                                            let (picked_den, _) = kit::chips(
                                                ui,
                                                "project-info-meter-den",
                                                &den_labels,
                                                cur_den_idx,
                                                None,
                                            );
                                            if let Some(i) = picked_den {
                                                set_meter_req = Some((cur_num, dens[i]));
                                            }
                                            ui.label(
                                                RichText::new("/")
                                                    .font(t::mono(13.0))
                                                    .color(t::faint()),
                                            );
                                            let r = kit::drag_value(
                                                ui,
                                                egui::DragValue::new(&mut num)
                                                    .range(1..=16)
                                                    .speed(0.1),
                                            );
                                            if r.changed() && num != cur_num {
                                                set_meter_req = Some((num, cur_den));
                                            }
                                        });
                                        if extra_meters > 0 {
                                            ui.label(
                                                RichText::new(
                                                    tr.info_set_meter_changes
                                                        .fmt(tr.lang, extra_meters),
                                                )
                                                .font(t::sans(12.0))
                                                .color(t::faint()),
                                            );
                                        }

                                        ui.add_space(4.0);
                                        ui.separator();

                                        // 2. Project musical key / scale
                                        use keyframexl::scale::{Kind, ROOTS, Scale};
                                        let proj_scale = self.song.info.scale;
                                        ui.allocate_ui_with_layout(
                                            vec2(w, kit::TOOL_H),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let show_adopt = self.roll_scale != proj_scale
                                                    && self.roll_scale.is_some();
                                                let right_w = tool_btn_width(ui, tr.info_set_key_off)
                                                    + if show_adopt {
                                                        tool_btn_width(ui, tr.info_set_adopt_roll)
                                                            + 8.0
                                                    } else {
                                                        0.0
                                                    }
                                                    + 12.0;
                                                let r = ui
                                                    .allocate_exact_size(
                                                        vec2((w - right_w).max(60.0), 16.0),
                                                        egui::Sense::hover(),
                                                    )
                                                    .0;
                                                kit::caps_fit(
                                                    ui.painter(),
                                                    r.left_center(),
                                                    egui::Align2::LEFT_CENTER,
                                                    tr.info_set_key,
                                                    12.0,
                                                    t::faint(),
                                                    r.width(),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if kit::tool_button(
                                                            ui,
                                                            tr.info_set_key_off,
                                                            if proj_scale.is_none() {
                                                                kit::Kind::Brand
                                                            } else {
                                                                kit::Kind::Normal
                                                            },
                                                            true,
                                                        )
                                                        .clicked()
                                                            && proj_scale.is_some()
                                                        {
                                                            set_scale_req = Some(None);
                                                        }
                                                        if self.roll_scale != proj_scale
                                                            && self.roll_scale.is_some()
                                                            && kit::tool_button(
                                                                ui,
                                                                tr.info_set_adopt_roll,
                                                                kit::Kind::Ghost,
                                                                true,
                                                            )
                                                            .clicked()
                                                        {
                                                            set_scale_req = Some(self.roll_scale);
                                                        }
                                                    },
                                                );
                                            },
                                        );

                                        let cur = proj_scale.unwrap_or(Scale {
                                            root: 0,
                                            kind: Kind::Minor,
                                        });
                                        let key_gap = 2.0;
                                        let key_w = ((w - key_gap * 11.0) / 12.0).floor().max(24.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = key_gap;
                                            for (idx, root_name) in ROOTS.iter().enumerate() {
                                                let selected = proj_scale
                                                    .is_some_and(|s| s.root == idx as u8);
                                                let kind = if selected {
                                                    kit::Kind::Brand
                                                } else {
                                                    kit::Kind::Normal
                                                };
                                                if kit::button_sized(
                                                    ui,
                                                    vec2(key_w, kit::TOOL_H),
                                                    kind,
                                                    true,
                                                    root_name,
                                                    |p, face, ink| {
                                                        p.text(
                                                            face.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            *root_name,
                                                            t::mono(12.0),
                                                            ink,
                                                        );
                                                    },
                                                )
                                                .clicked()
                                                {
                                                    set_scale_req = Some(Some(Scale {
                                                        root: idx as u8,
                                                        kind: cur.kind,
                                                    }));
                                                }
                                            }
                                        });

                                        ui.add_space(4.0);
                                        let scale_label = match proj_scale {
                                            Some(s) => format!(
                                                "{} {}",
                                                ROOTS[(s.root as usize) % 12],
                                                tr.scale_kind_label(s.kind)
                                            ),
                                            None => tr.info_set_key_off.to_string(),
                                        };
                                        kit::dropdown(
                                            ui,
                                            "project-info-scale-kind",
                                            &scale_label,
                                            210.0_f32.min(w),
                                            |ui| {
                                                for k in Kind::ALL {
                                                    let sel =
                                                        proj_scale.is_some_and(|s| s.kind == k);
                                                    if ui
                                                        .selectable_label(sel, tr.scale_kind_label(k))
                                                        .clicked()
                                                    {
                                                        set_scale_req = Some(Some(Scale {
                                                            root: cur.root,
                                                            kind: k,
                                                        }));
                                                    }
                                                }
                                            },
                                        );
                                        ui.add_enabled(
                                            self.roll_scale.is_some() || proj_scale.is_some(),
                                            egui::Checkbox::new(
                                                &mut self.scale_snap,
                                                RichText::new(tr.info_set_snap_scale)
                                                    .font(t::sans(13.0)),
                                            ),
                                        );

                                        ui.add_space(4.0);
                                        ui.separator();

                                        // 3. Forge sound style
                                        use keyframexl::forge::recipes::Style;
                                        section_caps(ui, tr.info_set_game_style, w);
                                        let mut labels = vec![tr.info_set_game_style_none];
                                        labels.extend(Style::ALL.iter().map(|s| tr.forge_style(*s)));
                                        let cur = self.song.game_style.map_or(0, |s| {
                                            Style::ALL.iter().position(|x| *x == s).unwrap_or(0) + 1
                                        });
                                        let chip_w =
                                            ((w - 6.0) / labels.len() as f32).floor().max(52.0);
                                        let widths = vec![chip_w; labels.len()];
                                        let (picked, _) = kit::chips(
                                            ui,
                                            "project-info-game-style",
                                            &labels,
                                            cur,
                                            Some(&widths),
                                        );
                                        if let Some(i) = picked {
                                            let next = if i == 0 {
                                                None
                                            } else {
                                                Some(Style::ALL[i - 1])
                                            };
                                            if next != self.song.game_style {
                                                set_game_style = Some(next);
                                            }
                                        }
                                    }
                                }
                            });
                    },
                );

                if info_text_changed || info_discrete_changed {
                    if info_discrete_changed || info_first_edit {
                        self.checkpoint();
                    }
                    self.song.info = info;
                }
                for r in &edit_responses {
                    kit::end_edit(ui, r);
                }
                ui.separator();
                ui.allocate_ui_with_layout(
                    vec2(win_w, kit::TOOL_H),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if kit::tool_button(ui, tr.info_close, kit::Kind::Normal, true).clicked() {
                            close_now = true;
                        }
                    },
                );
            });

        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_now = true;
        }
        if close_now {
            open = false;
        }
        if let Some((num, den)) = set_meter_req {
            self.set_meter(0, num, den);
        }
        if let Some(next_scale) = set_scale_req {
            self.checkpoint();
            self.song.info.scale = next_scale;
            self.roll_scale = next_scale;
        }
        if let Some(next_style) = set_game_style {
            self.checkpoint();
            self.song.game_style = next_style;
        }
        if let Some(url) = open_url {
            if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                self.say(
                    Tone::Bad,
                    crate::i18n::fill(tr.info_url_open_err, &[("err", &e.to_string())]),
                );
            } else {
                self.say(
                    Tone::Done,
                    crate::i18n::fill(tr.info_url_opened, &[("url", &url)]),
                );
            }
        }
        if open_export {
            self.export();
        }
        ctx.data_mut(|d| {
            d.insert_temp(id, open);
            d.insert_temp(tab_id, tab);
        });
    }

    /// Every two minutes of unsaved work, write it to the autosave file
    /// (off the UI thread). FILE → Recover autosave reads it back.
    fn autosave(&mut self, now: f64) {
        // Live checks run without persistence; KFXL_CAPTURE_AUTOSAVE=secs turns it on, faster.
        let every = std::env::var("KFXL_CAPTURE_AUTOSAVE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok());
        if !(self.persist || every.is_some()) || !self.dirty() || self.edits == self.autosaved.0 {
            return;
        }
        if self.autosaved.1 == 0.0 {
            self.autosaved.1 = now;
        }
        let minutes = self.settings.general.autosave_minutes;
        if every.is_none() && minutes == 0 {
            return;
        }
        let interval = if minutes == AUTOSAVE_DEFAULT_MINUTES {
            AUTOSAVE_EVERY
        } else {
            minutes as f64 * 60.0
        };
        if now - self.autosaved.1 < every.unwrap_or(interval) {
            return;
        }
        self.autosaved = (self.edits, now);
        self.collect_plugin_states();
        let Some(path) = autosave_path() else { return };
        let song = self.song.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
            let tmp = path.with_extension("kfxl.tmp");
            if keyframexl::project_io::save(&song, &tmp).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        });
    }

    /// "Save changes?" for the pending command: Save / Don't save / Cancel.
    pub(crate) fn unsaved_window(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        let Some(cmd) = self.pending.clone() else {
            return;
        };
        let mut answer = None;
        let what = match &cmd {
            Pending::New => tr.proj_before_new,
            Pending::Open(_) => tr.proj_before_open,
            Pending::Quit => tr.proj_before_quit,
        };
        egui::Window::new(tr.proj_unsaved_title)
            .id(egui::Id::new("unsaved-changes-window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 140.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(fill(
                        tr.proj_save_before,
                        &[("name", &self.project_name()), ("what", what)],
                    ))
                    .font(t::sans(14.0))
                    .color(t::text()),
                );
                ui.add_space(10.0);
                let (enter, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                ui.horizontal(|ui| {
                    if kit::button(ui, tr.file_save, kit::Kind::Primary, true).clicked() || enter {
                        answer = Some(true);
                    }
                    if kit::button(ui, tr.proj_dont_save, kit::Kind::Normal, true).clicked() {
                        answer = Some(false);
                    }
                    if kit::button(ui, tr.common_cancel, kit::Kind::Normal, true).clicked() || esc {
                        self.pending = None;
                        // A cancelled close is not a restart into the update either.
                        self.relaunch = None;
                    }
                });
            });
        match answer {
            Some(true) => {
                self.pending = None;
                self.save();
                if self.dirty() {
                    // Save as… was cancelled or failed: keep the work open.
                    self.say(Tone::Bad, tr.proj_not_saved);
                } else {
                    self.perform(cmd);
                }
            }
            Some(false) => {
                self.pending = None;
                self.perform(cmd);
            }
            None => {}
        }
    }

    pub(crate) fn recover_item(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        let path = autosave_path().filter(|p| p.exists());
        let when = path
            .as_ref()
            .and_then(|p| p.metadata().ok()?.modified().ok()?.elapsed().ok())
            .map(|d| match d.as_secs() {
                s if s < 120 => tr.proj_just_now.to_string(),
                s if s < 7200 => tr.proj_min_ago.fmt(tr.lang, (s / 60) as usize),
                s if s < 172800 => tr.proj_h_ago.fmt(tr.lang, (s / 3600) as usize),
                s => tr.proj_days_ago.fmt(tr.lang, (s / 86400) as usize),
            });
        let label = match &when {
            Some(w) => fill(tr.proj_recover_autosave_when, &[("when", w)]),
            None => tr.proj_recover_autosave.into(),
        };
        let r = ui.add_enabled(
            path.is_some(),
            egui::Button::new(RichText::new(label).font(t::sans(13.0))),
        );
        let r = if path.is_none() {
            r.on_disabled_hover_text(tr.proj_no_autosave)
        } else {
            r
        };
        if r.clicked() {
            ui.close_menu();
            self.request(Pending::Open(path));
        }
    }

    pub(crate) fn recent_menu(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        ui.menu_button(
            RichText::new(tr.file_open_recent).font(t::sans(13.0)),
            |ui| {
                kit::menu_style(ui);
                if self.recent.is_empty() {
                    ui.add_enabled(
                        false,
                        egui::Button::new(RichText::new(tr.proj_no_recent).font(t::sans(13.0))),
                    );
                    return;
                }
                for path in self.recent.clone() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let r = ui
                        .add(egui::Button::new(RichText::new(name).font(t::sans(13.0))))
                        .on_hover_text(path.display().to_string());
                    if r.clicked() {
                        ui.close_menu();
                        self.request(Pending::Open(Some(path)));
                    }
                }
                ui.separator();
                if ui
                    .button(RichText::new(tr.proj_clear_list).font(t::sans(13.0)))
                    .clicked()
                {
                    ui.close_menu();
                    self.clear_recent();
                }
            },
        );
    }
}

const GENRE_PRESETS: &[&str] = &[
    "Ambient",
    "Chiptune",
    "Cinematic",
    "Classical",
    "Deep House",
    "Drum & bass",
    "Electronic",
    "Hip-hop",
    "House",
    "Jazz",
    "Lo-fi hip-hop",
    "Pop",
    "Rock",
    "Soundtrack",
    "Synthwave",
    "Techno",
    "Trap",
];

struct ProjectStats {
    channels: usize,
    patterns: usize,
    clips: usize,
    bars: u32,
    seconds: f64,
    plugins: usize,
    missing_plugins: usize,
}

impl ProjectStats {
    fn collect(app: &App) -> Self {
        let plugin_refs = app.song.plugin_refs();
        let missing_plugins = plugin_refs
            .iter()
            .filter(|r| {
                app.plugins
                    .live
                    .get(&r.id)
                    .is_some_and(|l| l.error.is_some())
            })
            .count();
        let has_clips = !app.song.clips.is_empty();
        let frames = keyframexl::render::main_frames(&app.song, app.selected, has_clips);
        let seconds = frames as f64 / keyframexl::render::RATE as f64;
        Self {
            channels: app.song.instruments.len(),
            patterns: app.song.patterns.len(),
            clips: app.song.clips.len(),
            bars: app.song.bars(),
            seconds,
            plugins: plugin_refs.len(),
            missing_plugins,
        }
    }
}

const PRESET_BTN_W: f32 = 28.0;

fn tool_btn_width(ui: &egui::Ui, label: &str) -> f32 {
    kit::text_width(ui.painter(), label, t::sans(13.0)) + 20.0
}

fn field_label(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).font(t::sans(12.0)).color(t::muted()));
}

fn section_caps(ui: &mut egui::Ui, text: &str, width: f32) {
    let r = ui
        .allocate_exact_size(vec2(width, 16.0), egui::Sense::hover())
        .0;
    kit::caps(
        ui.painter(),
        r.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        12.0,
        t::faint(),
    );
}

fn singleline_field(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
    max_chars: usize,
    width: f32,
) -> egui::Response {
    ui.add_sized(
        vec2(width, kit::TOOL_H),
        egui::TextEdit::singleline(value)
            .font(t::sans(13.0))
            .hint_text(hint)
            .char_limit(max_chars)
            .vertical_align(egui::Align::Center)
            .margin(egui::Margin::symmetric(8, 4)),
    )
}

fn preset_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    current: &str,
    presets: &[&'static str],
    ignore_case: bool,
) -> Option<&'static str> {
    let popup_id = ui.make_persistent_id(id_salt);
    let btn = kit::button_sized(
        ui,
        vec2(PRESET_BTN_W, kit::TOOL_H),
        kit::Kind::Normal,
        true,
        id_salt,
        |p, face, ink| {
            p.text(
                face.center(),
                egui::Align2::CENTER_CENTER,
                "▾",
                t::sans(12.0),
                ink,
            );
        },
    );
    if btn.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let mut picked = None;
    kit::popup_menu(ui, popup_id, &btn, 180.0, |ui| {
        for &item in presets {
            let selected = if ignore_case {
                current.eq_ignore_ascii_case(item)
            } else {
                current == item
            };
            if ui.selectable_label(selected, item).clicked() {
                picked = Some(item);
                ui.close_menu();
            }
        }
    });
    picked
}

fn setting_row(ui: &mut egui::Ui, width: f32, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        vec2(width, kit::CONTROL_H),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(label).font(t::sans(13.0)).color(t::text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
        },
    );
}

fn is_http_url(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() || u.chars().any(char::is_whitespace) {
        return false;
    }
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty())
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

fn preview_id3_frames(info: &keyframexl::sequence::ProjectInfo) -> Vec<(&'static str, String)> {
    let clip = |s: &str, max: usize| {
        let trimmed = s.trim();
        let count = trimmed.chars().count();
        if count <= max {
            trimmed.to_string()
        } else {
            let mut out: String = trimmed.chars().take(max.saturating_sub(1)).collect();
            out.push('…');
            out
        }
    };
    let mut out = Vec::new();
    for (code, raw, max) in [
        ("TIT2", &info.title, 28),
        ("TPE1", &info.author, 24),
        ("TALB", &info.album, 24),
        ("TCON", &info.genre, 20),
        ("TDRC", &info.year, 12),
        ("TCOP", &info.copyright, 24),
        ("WXXX", &info.url, 28),
    ] {
        if !raw.trim().is_empty() {
            out.push((code, clip(raw, max)));
        }
    }
    if !info.comments.trim().is_empty() {
        let first_line = info
            .comments
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        out.push(("COMM", clip(first_line, 28)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyframexl::scale::{Kind, Scale};
    use keyframexl::sequence::{ProjectInfo, Song};

    #[test]
    fn http_url_validation_accepts_web_urls_and_rejects_unsafe_or_empty() {
        assert!(is_http_url("https://example.com/track"));
        assert!(is_http_url("  http://localhost:8080/demo  "));
        assert!(!is_http_url(""));
        assert!(!is_http_url("example.com"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("javascript:alert(1)"));
        assert!(!is_http_url("file:///etc/passwd"));
        assert!(!is_http_url("https://example.com/with space"));
    }

    #[test]
    fn id3_preview_reflects_populated_fields_and_truncates_long_values() {
        let mut info = ProjectInfo::default();
        assert!(preview_id3_frames(&info).is_empty());

        info.title = "Midnight Skyline Over The Distant Harbor".into();
        info.author = "KeyFrameXL".into();
        info.album = "Sessions".into();
        info.year = "2026".into();
        info.copyright = "CC0 1.0".into();
        info.url = "https://example.com/skyline".into();
        info.comments = "\nFirst real line of notes\nSecond line".into();

        let frames = preview_id3_frames(&info);
        let codes: Vec<_> = frames.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            codes,
            vec!["TIT2", "TPE1", "TALB", "TDRC", "TCOP", "WXXX", "COMM"]
        );
        assert!(frames[0].1.ends_with('…'));
        assert_eq!(frames.last().unwrap().1, "First real line of notes");
    }

    #[test]
    fn project_info_window_renders_all_tabs_in_dark_light_and_both_languages() {
        let mut song = keyframexl::demo::build(None);
        song.info = ProjectInfo {
            title: "Нічне місто".into(),
            author: "Studio".into(),
            album: "EP 1".into(),
            genre: "House".into(),
            year: "2026".into(),
            copyright: "CC BY 4.0".into(),
            url: "https://example.com/ep1".into(),
            comments: "Kick from Kit 3\nPad take 2".into(),
            show_on_open: true,
            scale: Some(Scale {
                root: 9,
                kind: Kind::Minor,
            }),
        };

        let mut app = App::with_song(song, None);
        assert_eq!(
            app.roll_scale,
            Some(Scale {
                root: 9,
                kind: Kind::Minor,
            })
        );

        let ctx = egui::Context::default();
        ctx.set_fonts(t::font_definitions(None));
        app.open_info(&ctx);

        for lang in crate::i18n::Language::ALL {
            app.settings.general.language = lang;
            for theme in [t::Theme::Dark, t::Theme::Light] {
                t::apply(&ctx, theme);
                for tab in 0..3usize {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-project-info-tab"), tab));
                    let raw = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::pos2(0.0, 0.0),
                            vec2(1280.0, 800.0),
                        )),
                        ..Default::default()
                    };
                    let _ = ctx.run(raw, |ctx| {
                        app.info_window(ctx);
                    });
                }
            }
        }
        assert!(ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("kfxl-project-info"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn opening_project_with_show_on_open_queues_info_window_and_syncs_scale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auto_info.kfxl");
        let mut song = Song::default();
        song.info.title = "Auto Open".into();
        song.info.show_on_open = true;
        song.info.scale = Some(Scale {
            root: 2,
            kind: Kind::Dorian,
        });
        keyframexl::project_io::save(&song, &path).unwrap();

        let mut app = App::with_song(Song::default(), None);
        assert_eq!(app.roll_scale, None);
        assert!(!app.open_info_next);

        app.open_path(&path);
        assert_eq!(app.roll_scale, song.info.scale);
        assert!(app.open_info_next);

        let ctx = egui::Context::default();
        ctx.set_fonts(t::font_definitions(None));
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            app.info_window(ctx);
        });
        assert!(!app.open_info_next);
        assert!(ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("kfxl-project-info"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn project_info_window_size_stays_stable_across_frames() {
        let mut song = keyframexl::demo::build(None);
        song.info = ProjectInfo {
            title: "Very Long Project Title With Ukrainian Тестова Назва Композиції".into(),
            author: "KeyFrameXL Studio".into(),
            album: "Midnight Sessions Vol. 2".into(),
            genre: "Lo-fi hip-hop".into(),
            year: "2026".into(),
            copyright: "CC BY-SA 4.0".into(),
            url: "not-a-valid-url-yet".into(),
            comments: "Line 1\nLine 2\nLine 3".into(),
            show_on_open: true,
            scale: Some(Scale {
                root: 9,
                kind: Kind::Minor,
            }),
        };
        song.meters.push(keyframexl::tempo::Meter {
            bar: 4,
            num: 3,
            den: 4,
        });

        let fake_path = PathBuf::from("/tmp/long_project_filename_example.kfxl");
        let mut app = App::with_song(song, None);
        app.path = Some(fake_path);
        app.roll_scale = Some(Scale {
            root: 0,
            kind: Kind::Major,
        });

        for lang in crate::i18n::Language::ALL {
            app.settings.general.language = lang;
            for tab in 0..3usize {
                let ctx = egui::Context::default();
                ctx.set_fonts(t::font_definitions(None));
                t::apply(&ctx, t::Theme::Dark);
                app.open_info(&ctx);
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-project-info-tab"), tab));

                let mut rects = Vec::new();
                for frame in 0..20 {
                    let raw = egui::RawInput {
                        time: Some(frame as f64 * (1.0 / 60.0)),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::pos2(0.0, 0.0),
                            vec2(1280.0, 800.0),
                        )),
                        ..Default::default()
                    };
                    let _ = ctx.run(raw, |ctx| {
                        app.info_window(ctx);
                    });
                    let rect = ctx
                        .memory(|m| m.area_rect(egui::Id::new("project-info-window")))
                        .expect("project-info-window rect should exist");
                    rects.push(rect);
                }

                let settled = rects[2].size();
                let final_sz = rects[19].size();
                let trace: Vec<_> = rects.iter().take(6).map(|r| r.size()).collect();
                assert!(
                    (final_sz.x - settled.x).abs() <= 0.5,
                    "window width drifted on lang={lang:?} tab={tab}: frame2={} -> frame19={} (first 6: {trace:?})",
                    settled.x,
                    final_sz.x
                );
                assert!(
                    (final_sz.y - settled.y).abs() <= 0.5,
                    "window height drifted on lang={lang:?} tab={tab}: frame2={} -> frame19={} (first 6: {trace:?})",
                    settled.y,
                    final_sz.y
                );
                assert!(
                    final_sz.x <= 560.0 && final_sz.y <= 640.0,
                    "window grew past expected default bounds on lang={lang:?} tab={tab}: {:?} (first 6: {trace:?})",
                    final_sz
                );
            }

            // Switching tabs repeatedly in the same context must not drift size either.
            let ctx = egui::Context::default();
            ctx.set_fonts(t::font_definitions(None));
            t::apply(&ctx, t::Theme::Dark);
            app.open_info(&ctx);
            let mut sizes = Vec::new();
            for step in 0..18usize {
                let tab = step % 3;
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-project-info-tab"), tab));
                let raw = egui::RawInput {
                    time: Some(step as f64 * (1.0 / 60.0)),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        vec2(1280.0, 800.0),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run(raw, |ctx| {
                    app.info_window(ctx);
                });
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::new("project-info-window")))
                    .expect("project-info-window rect should exist");
                sizes.push(rect.size());
            }
            let base = sizes[3];
            let end = *sizes.last().unwrap();
            assert!(
                (end.x - base.x).abs() <= 0.5 && (end.y - base.y).abs() <= 0.5,
                "window drifted while switching tabs on lang={lang:?}: {:?} -> {:?}",
                base,
                end
            );
        }
    }
}
