use super::{App, Pending, Tone, files, kit};
use crate::theme as t;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ImportDialog {
    path: Option<PathBuf>,
    new_channel: bool,
}

impl App {
    pub(super) fn import_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.import_dialog.take() else {
            return;
        };
        let mut open = true;
        let mut browse = false;
        let mut import = false;
        let mut cancel = false;
        egui::Window::new("Import files")
            .id(egui::Id::new("kfxl-import"))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.0)
            .default_pos(egui::pos2(ctx.screen_rect().center().x - 210.0, 140.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let name = dialog.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy());
                    ui.label(RichText::new(name.as_deref().unwrap_or("Choose a file to import")).font(t::medium(15.0)))
                        .on_hover_text(dialog.path.as_ref().map_or_else(String::new, |p| p.display().to_string()));
                });
                if kit::button(ui, if dialog.path.is_some() { "Change file…" } else { "Browse…" }, kit::Kind::Normal, true).clicked() {
                    browse = true;
                }
                ui.separator();
                let valid = dialog.path.as_ref().is_some_and(|path| path.is_file());
                if let Some(path) = &dialog.path {
                    ui.label(RichText::new("Destination").font(t::medium(13.0)));
                    match files::ext(path).as_str() {
                        "flp" => { ui.label("Open as a new, unsaved project"); }
                        "zip" => { ui.label("Unpack beside the bundle and open the project"); }
                        "mid" | "midi" => { ui.label("Add a new pattern and channels"); }
                        _ => {
                            ui.radio_value(&mut dialog.new_channel, true, "New sampler channel");
                            ui.radio_value(&mut dialog.new_channel, false, "Selected sampler, or a new channel");
                        }
                    }
                    if !valid {
                        ui.label(RichText::new("File is unavailable. Choose another file.").color(t::bad()));
                    }
                } else {
                    ui.label(RichText::new("Audio, DirectWave preset, MIDI, FL Studio project, or KeyFrameXL bundle").color(t::muted()));
                }
                ui.horizontal(|ui| {
                    import = kit::button(ui, "Import", kit::Kind::Primary, valid).clicked();
                    cancel = kit::button(ui, "Cancel", kit::Kind::Ghost, true).clicked();
                });
            });
        if browse
            && let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Files KeyFrameXL can import",
                    &["wav", "flac", "ogg", "dwp", "mid", "midi", "flp", "zip"],
                )
                .add_filter("Audio and DirectWave", &["wav", "flac", "ogg", "dwp"])
                .add_filter("MIDI", &["mid", "midi"])
                .add_filter("Project and bundle", &["flp", "zip"])
                .pick_file()
        {
            dialog.path = Some(path);
            dialog.new_channel = true;
        }
        if import && let Some(path) = dialog.path.take() {
            match files::ext(&path).as_str() {
                "flp" | "zip" => self.request(Pending::Open(Some(path))),
                "wav" | "flac" | "ogg" | "dwp" | "mid" | "midi" => {
                    self.load_file(path, dialog.new_channel)
                }
                _ => self.say(Tone::Bad, "Choose a supported file type."),
            }
        } else if open && !cancel && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.import_dialog = Some(dialog);
        }
    }
}
