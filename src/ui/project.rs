//! Project lifecycle around the file commands: ask before unsaved work is
//! replaced or the window closes, remember recent projects, keep the window
//! title on the project name.

use super::{App, Tone, kit};
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
                    self.say(Tone::Done, "Recovered the autosave. Save it to keep it.");
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
            .unwrap_or_else(|| "Untitled".into())
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

    /// Open or close Project info (F11, FILE → Project info).
    pub(crate) fn toggle_info(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("kfxl-project-info");
        let open = ctx.data(|d| d.get_temp(id).unwrap_or(false));
        ctx.data_mut(|d| d.insert_temp(id, !open));
    }

    /// Title, author, genre and comments (FL: Project info). One undo step
    /// per field edit; the export writes them into MP3 tags.
    fn info_window(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("kfxl-project-info");
        let mut open = ctx.data(|d| d.get_temp(id).unwrap_or(false));
        if !open {
            return;
        }
        egui::Window::new("Project info")
            .id(egui::Id::new("project-info-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                let mut info = self.song.info.clone();
                let row = |ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str| {
                    ui.label(RichText::new(label).font(t::sans(12.0)).color(t::FAINT));
                    let r = ui.add(
                        egui::TextEdit::singleline(text)
                            .hint_text(hint)
                            .char_limit(200)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    r
                };
                let rs = [
                    row(ui, "Title", &mut info.title, &self.project_name()),
                    row(ui, "Author", &mut info.author, "Your name"),
                    row(ui, "Genre", &mut info.genre, "House, Ambient, …"),
                ];
                ui.label(
                    RichText::new("Comments")
                        .font(t::sans(12.0))
                        .color(t::FAINT),
                );
                let c = ui.add(
                    egui::TextEdit::multiline(&mut info.comments)
                        .hint_text("Notes for later: samples used, ideas, credits")
                        .char_limit(20_000)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                let all: Vec<&egui::Response> = rs.iter().chain(std::iter::once(&c)).collect();
                if all.iter().any(|r| r.changed()) {
                    if all
                        .iter()
                        .filter(|r| r.changed())
                        .any(|r| kit::first_edit(ui, r))
                    {
                        self.checkpoint();
                    }
                    self.song.info = info;
                }
                for r in all {
                    kit::end_edit(ui, r);
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "MP3 exports carry the title, author, genre and comments as tags.",
                    )
                    .font(t::sans(11.5))
                    .color(t::FAINT),
                );
            });
        ctx.data_mut(|d| d.insert_temp(id, open));
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
        let Some(cmd) = self.pending.clone() else {
            return;
        };
        let mut answer = None;
        let what = match &cmd {
            Pending::New => "starting a new project",
            Pending::Open(_) => "opening another project",
            Pending::Quit => "closing",
        };
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 140.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Save changes to {} before {what}?",
                        self.project_name()
                    ))
                    .font(t::sans(14.0))
                    .color(t::TEXT),
                );
                ui.add_space(10.0);
                let (enter, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                ui.horizontal(|ui| {
                    if kit::button(ui, "Save", kit::Kind::Primary, true).clicked() || enter {
                        answer = Some(true);
                    }
                    if kit::button(ui, "Don't save", kit::Kind::Normal, true).clicked() {
                        answer = Some(false);
                    }
                    if kit::button(ui, "Cancel", kit::Kind::Normal, true).clicked() || esc {
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
                    self.say(Tone::Bad, "Not saved, so nothing was replaced.");
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
        let path = autosave_path().filter(|p| p.exists());
        let when = path
            .as_ref()
            .and_then(|p| p.metadata().ok()?.modified().ok()?.elapsed().ok())
            .map(|d| match d.as_secs() {
                s if s < 120 => "just now".to_string(),
                s if s < 7200 => format!("{} min ago", s / 60),
                s if s < 172800 => format!("{} h ago", s / 3600),
                s => format!("{} days ago", s / 86400),
            });
        let label = match &when {
            Some(w) => format!("Recover autosave ({w})"),
            None => "Recover autosave".into(),
        };
        let r = ui.add_enabled(
            path.is_some(),
            egui::Button::new(RichText::new(label).font(t::sans(13.0))),
        );
        let r = if path.is_none() {
            r.on_disabled_hover_text("No autosave yet: unsaved work is written every two minutes.")
        } else {
            r
        };
        if r.clicked() {
            ui.close_menu();
            self.request(Pending::Open(path));
        }
    }

    pub(crate) fn recent_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(RichText::new("Open recent").font(t::sans(13.0)), |ui| {
            kit::menu_style(ui);
            if self.recent.is_empty() {
                ui.add_enabled(
                    false,
                    egui::Button::new(RichText::new("No recent projects").font(t::sans(13.0))),
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
                .button(RichText::new("Clear list").font(t::sans(13.0)))
                .clicked()
            {
                ui.close_menu();
                self.clear_recent();
            }
        });
    }
}
