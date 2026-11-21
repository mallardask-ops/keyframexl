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
        let tr = self.tr();
        let mut open = true;
        let mut browse = false;
        let mut import = false;
        let mut cancel = false;
        egui::Window::new(tr.imp_title)
            .id(egui::Id::new("kfxl-import"))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.0)
            .default_pos(egui::pos2(ctx.screen_rect().center().x - 210.0, 140.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let name = dialog
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|s| s.to_string_lossy());
                    ui.label(
                        RichText::new(name.as_deref().unwrap_or(tr.imp_choose_file))
                            .font(t::medium(15.0)),
                    )
                    .on_hover_text(
                        dialog
                            .path
                            .as_ref()
                            .map_or_else(String::new, |p| p.display().to_string()),
                    );
                });
                if kit::button(
                    ui,
                    if dialog.path.is_some() {
                        tr.imp_change_file
                    } else {
                        tr.imp_browse
                    },
                    kit::Kind::Normal,
                    true,
                )
                .clicked()
                {
                    browse = true;
                }
                ui.separator();
                let valid = dialog.path.as_ref().is_some_and(|path| path.is_file());
                if let Some(path) = &dialog.path {
                    ui.label(RichText::new(tr.imp_destination).font(t::medium(13.0)));
                    match files::ext(path).as_str() {
                        "flp" => {
                            ui.label(tr.imp_dest_flp);
                        }
                        "zip" => {
                            ui.label(tr.imp_dest_bundle);
                        }
                        "mid" | "midi" => {
                            ui.label(tr.imp_dest_midi);
                        }
                        _ => {
                            ui.radio_value(&mut dialog.new_channel, true, tr.imp_dest_new_sampler);
                            ui.radio_value(
                                &mut dialog.new_channel,
                                false,
                                tr.imp_dest_selected_sampler,
                            );
                        }
                    }
                    if !valid {
                        ui.label(RichText::new(tr.imp_file_unavailable).color(t::bad()));
                    }
                } else {
                    ui.label(RichText::new(tr.imp_supported_hint).color(t::muted()));
                }
                ui.horizontal(|ui| {
                    import = kit::button(ui, tr.imp_action, kit::Kind::Primary, valid).clicked();
                    cancel = kit::button(ui, tr.common_cancel, kit::Kind::Ghost, true).clicked();
                });
            });
        if browse
            && let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    tr.imp_filter_all,
                    &["wav", "flac", "ogg", "dwp", "mid", "midi", "flp", "zip"],
                )
                .add_filter(tr.imp_filter_audio, &["wav", "flac", "ogg", "dwp"])
                .add_filter("MIDI", &["mid", "midi"])
                .add_filter(tr.imp_filter_project, &["flp", "zip"])
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
                _ => self.say(Tone::Bad, tr.imp_unsupported_err),
            }
        } else if open && !cancel && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.import_dialog = Some(dialog);
        }
    }
}
