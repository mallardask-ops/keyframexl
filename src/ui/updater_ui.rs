//! Update checker UI: background startup check, non-intrusive status bar pill,
//! and the "Check for updates" window (`HELP → Check for updates…`).

use super::{App, Tone, kit};
use crate::theme as t;
use crate::updater::{CURRENT_VERSION, CheckState, RELEASES_URL, should_auto_check, unix_now};
use eframe::egui::{self, Align2, RichText, Stroke, vec2};

const SOURCE_UPDATE_CMD: &str = "git pull && cargo build --release";

impl App {
    /// Poll the background update worker and kick off the daily check once on startup.
    pub(super) fn updater_frame(&mut self, ctx: &egui::Context) {
        if !self.updater.checked_on_startup {
            self.updater.checked_on_startup = true;
            // Never make network calls during headless tests or screenshot captures.
            if self.persist && self.capture.is_none() {
                let now = unix_now();
                if should_auto_check(
                    self.settings.general.check_updates,
                    self.settings.general.last_update_check,
                    now,
                ) {
                    self.settings.general.last_update_check = now;
                    self.save_settings();
                    self.updater.start(Some(ctx.clone()), false);
                }
            }
        }
        self.updater.poll();
        if self.updater.dialog_open {
            self.updater_window(ctx);
        }
    }

    /// Manually trigger an update check and open the dialog.
    pub(crate) fn check_for_updates(&mut self, ctx: &egui::Context) {
        let now = unix_now();
        self.settings.general.last_update_check = now;
        self.save_settings();
        self.updater.start(Some(ctx.clone()), true);
    }

    /// Floating "Check for updates" window.
    fn updater_window(&mut self, ctx: &egui::Context) {
        let mut open = self.updater.dialog_open;
        let mut action_retry = false;
        let mut action_open_url: Option<String> = None;
        let mut action_copy_cmd = false;
        let mut action_skip: Option<String> = None;
        let mut action_save_prefs = false;

        egui::Window::new("Check for updates")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(450.0)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 96.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 10.0);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("KeyFrameXL")
                            .font(t::semibold(15.0))
                            .color(t::TEXT),
                    );
                    ui.label(
                        RichText::new(format!("v{CURRENT_VERSION}"))
                            .font(t::mono(12.5))
                            .color(t::MUTED),
                    );
                });

                match self.updater.state.clone() {
                    CheckState::Idle => {
                        ui.label(
                            RichText::new("Click Check now to look for newer releases on GitHub.")
                                .font(t::sans(13.0))
                                .color(t::MUTED),
                        );
                        ui.horizontal(|ui| {
                            if kit::button(ui, "Check now", kit::Kind::Normal, true).clicked() {
                                action_retry = true;
                            }
                            if kit::button(ui, "Open GitHub releases", kit::Kind::Ghost, true)
                                .clicked()
                            {
                                action_open_url = Some(RELEASES_URL.to_string());
                            }
                        });
                    }
                    CheckState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("Checking GitHub for updates…")
                                    .font(t::sans(13.0))
                                    .color(t::MUTED),
                            );
                        });
                    }
                    CheckState::UpToDate { latest } => {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(vec2(12.0, 14.0), egui::Sense::hover());
                            kit::led(ui.painter(), r.center(), 4.0, Some(t::OK));
                            let line = match latest {
                                Some(ver) => format!("You're up to date (latest: v{ver})."),
                                None => "You're up to date (no GitHub releases published yet)."
                                    .to_string(),
                            };
                            ui.label(RichText::new(line).font(t::sans(13.0)).color(t::OK));
                        });
                        ui.horizontal(|ui| {
                            if kit::button(ui, "Check again", kit::Kind::Normal, true).clicked() {
                                action_retry = true;
                            }
                            if kit::button(ui, "Open GitHub releases", kit::Kind::Ghost, true)
                                .clicked()
                            {
                                action_open_url = Some(RELEASES_URL.to_string());
                            }
                        });
                    }
                    CheckState::Error(msg) => {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(vec2(12.0, 14.0), egui::Sense::hover());
                            kit::led(ui.painter(), r.center(), 4.0, Some(t::BAD));
                            ui.label(RichText::new(msg).font(t::sans(13.0)).color(t::BAD));
                        });
                        ui.horizontal(|ui| {
                            if kit::button(ui, "Retry", kit::Kind::Normal, true).clicked() {
                                action_retry = true;
                            }
                            if kit::button(ui, "Open GitHub releases", kit::Kind::Ghost, true)
                                .clicked()
                            {
                                action_open_url = Some(RELEASES_URL.to_string());
                            }
                        });
                    }
                    CheckState::Available(info) => {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(vec2(12.0, 14.0), egui::Sense::hover());
                            kit::led(ui.painter(), r.center(), 4.0, Some(t::WAIT));
                            ui.label(
                                RichText::new(format!(
                                    "Update available: v{CURRENT_VERSION} → v{}",
                                    info.version
                                ))
                                .font(t::semibold(13.5))
                                .color(t::WAIT),
                            );
                            if let Some(date) = &info.published_at {
                                ui.label(
                                    RichText::new(format!("({date})"))
                                        .font(t::mono(11.5))
                                        .color(t::FAINT),
                                );
                            }
                        });

                        if !info.notes.is_empty() {
                            egui::Frame::new()
                                .fill(t::SUNKEN)
                                .stroke(Stroke::new(1.0_f32, t::EDGE))
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    egui::ScrollArea::vertical()
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(&info.notes)
                                                    .font(t::sans(12.5))
                                                    .color(t::TEXT),
                                            );
                                        });
                                });
                        }

                        ui.horizontal_wrapped(|ui| {
                            if kit::button(ui, "Open release page", kit::Kind::Normal, true)
                                .clicked()
                            {
                                action_open_url = Some(info.html_url.clone());
                            }
                            if let Some(asset) = &info.linux_asset_url
                                && kit::button(ui, "Download Linux build", kit::Kind::Ghost, true)
                                    .clicked()
                            {
                                action_open_url = Some(asset.clone());
                            }
                            if kit::button(ui, "Copy update command", kit::Kind::Ghost, true)
                                .on_hover_text(SOURCE_UPDATE_CMD)
                                .clicked()
                            {
                                action_copy_cmd = true;
                            }
                            if kit::button(ui, "Skip this version", kit::Kind::Ghost, true)
                                .clicked()
                            {
                                action_skip = Some(info.version.clone());
                            }
                        });
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if super::settings_ui::kit_switch(ui, &mut self.settings.general.check_updates) {
                        action_save_prefs = true;
                    }
                    ui.label(
                        RichText::new("Check automatically (daily)")
                            .font(t::sans(12.5))
                            .color(t::MUTED),
                    );
                });
            });

        self.updater.dialog_open = open;

        if action_save_prefs {
            self.save_settings();
        }
        if action_retry {
            self.check_for_updates(ctx);
        }
        if let Some(url) = action_open_url {
            if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                self.say(Tone::Bad, format!("Could not open browser: {e}"));
            } else {
                self.say(Tone::Done, "Opened release page in browser.");
            }
        }
        if action_copy_cmd {
            ctx.copy_text(SOURCE_UPDATE_CMD.to_string());
            self.say(Tone::Done, format!("Copied: {SOURCE_UPDATE_CMD}"));
        }
        if let Some(ver) = action_skip {
            self.settings.general.skipped_version = Some(ver.clone());
            self.save_settings();
            self.updater.dialog_open = false;
            self.say(Tone::Quiet, format!("Skipped update v{ver}."));
        }
    }
}
