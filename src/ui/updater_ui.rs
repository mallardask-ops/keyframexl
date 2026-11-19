//! Update checker UI: background startup check, non-intrusive status bar pill,
//! and the "Check for updates" window (`HELP → Check for updates…`).

use super::{App, Tone, kit};
use crate::theme as t;
use crate::updater::{
    CURRENT_VERSION, CheckState, InstallState, RELEASES_URL, should_auto_check, unix_now,
};
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
                    self.updater.start(
                        Some(ctx.clone()),
                        false,
                        self.settings.general.nightly_updates,
                    );
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
        self.updater.start(
            Some(ctx.clone()),
            true,
            self.settings.general.nightly_updates,
        );
    }

    /// Floating "Check for updates" window.
    fn updater_window(&mut self, ctx: &egui::Context) {
        let mut open = self.updater.dialog_open;
        let mut action_retry = false;
        let mut action_open_url: Option<String> = None;
        let mut action_copy_cmd = false;
        let mut action_skip: Option<String> = None;
        let mut action_save_prefs = false;
        let mut action_install: Option<crate::updater::ReleaseInfo> = None;
        let mut action_restart: Option<std::path::PathBuf> = None;
        let mut action_close = false;

        let tr = crate::i18n::strings(self.settings.general.language);
        egui::Window::new(tr.update_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
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
                            if kit::button(ui, tr.update_check_again, kit::Kind::Normal, true)
                                .clicked()
                            {
                                action_retry = true;
                            }
                            if kit::button(ui, tr.update_open_release, kit::Kind::Ghost, true)
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
                                RichText::new(tr.update_checking)
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
                                Some(ver) => format!("{} (v{ver})", tr.update_up_to_date),
                                None => tr.update_up_to_date.to_string(),
                            };
                            ui.label(RichText::new(line).font(t::sans(13.0)).color(t::OK));
                        });
                        ui.horizontal(|ui| {
                            if kit::button(ui, tr.update_check_again, kit::Kind::Normal, true)
                                .clicked()
                            {
                                action_retry = true;
                            }
                            if kit::button(ui, tr.update_open_release, kit::Kind::Ghost, true)
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
                            if kit::button(ui, tr.update_check_again, kit::Kind::Normal, true)
                                .clicked()
                            {
                                action_retry = true;
                            }
                            if kit::button(ui, tr.update_open_release, kit::Kind::Ghost, true)
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
                                    "{}: v{CURRENT_VERSION} → v{}",
                                    tr.update_available, info.version
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
                                .inner_margin(egui::Margin::symmetric(12, 10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    egui::ScrollArea::vertical()
                                        .max_height(280.0)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 5.0;
                                            if let Some(url) =
                                                super::markdown::show(ui, &info.notes, 12.5)
                                            {
                                                action_open_url = Some(url);
                                            }
                                        });
                                });
                        }

                        let target = crate::updater::self_update_target();
                        match self.updater.install.clone() {
                            InstallState::Downloading { downloaded, total } => {
                                let mb = |b: u64| b as f64 / 1_048_576.0;
                                let (frac, text) = match total {
                                    Some(t) if t > 0 => (
                                        downloaded as f32 / t as f32,
                                        format!(
                                            "{}  {:.1} / {:.1} MB",
                                            tr.update_downloading,
                                            mb(downloaded),
                                            mb(t)
                                        ),
                                    ),
                                    _ => (
                                        0.0,
                                        format!(
                                            "{}  {:.1} MB",
                                            tr.update_downloading,
                                            mb(downloaded)
                                        ),
                                    ),
                                };
                                ui.add(
                                    egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                                        .fill(t::BRAND_FILL)
                                        .animate(total.is_none())
                                        .text(RichText::new(text).font(t::mono(11.5))),
                                );
                            }
                            InstallState::Verifying | InstallState::Installing => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    let line = if self.updater.install == InstallState::Verifying {
                                        tr.update_verifying
                                    } else {
                                        tr.update_installing
                                    };
                                    ui.label(
                                        RichText::new(line).font(t::sans(13.0)).color(t::MUTED),
                                    );
                                });
                            }
                            InstallState::ReadyToRestart { exe_path, .. } => {
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(
                                        vec2(12.0, 14.0),
                                        egui::Sense::hover(),
                                    );
                                    kit::led(ui.painter(), r.center(), 4.0, Some(t::OK));
                                    ui.label(
                                        RichText::new(tr.update_ready_restart)
                                            .font(t::sans(13.0))
                                            .color(t::OK),
                                    );
                                });
                                if self.dirty() {
                                    ui.label(
                                        RichText::new(tr.update_restart_save_hint)
                                            .font(t::sans(12.5))
                                            .color(t::WAIT),
                                    );
                                }
                                ui.horizontal(|ui| {
                                    if kit::button(
                                        ui,
                                        tr.update_restart_now,
                                        kit::Kind::Primary,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        action_restart = Some(exe_path.clone());
                                    }
                                    if kit::button(ui, tr.update_later, kit::Kind::Ghost, true)
                                        .clicked()
                                    {
                                        action_close = true;
                                    }
                                });
                            }
                            state @ (InstallState::Idle | InstallState::Failed(_)) => {
                                if let InstallState::Failed(msg) = &state {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(
                                            vec2(12.0, 14.0),
                                            egui::Sense::hover(),
                                        );
                                        kit::led(ui.painter(), r.center(), 4.0, Some(t::BAD));
                                        ui.label(
                                            RichText::new(format!("{}: {msg}", tr.update_failed))
                                                .font(t::sans(12.5))
                                                .color(t::BAD),
                                        );
                                    });
                                }
                                let from_source = matches!(&target, Err(e) if e == "source");
                                if from_source {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(tr.update_source_build)
                                                .font(t::sans(12.5))
                                                .color(t::MUTED),
                                        );
                                        ui.label(
                                            RichText::new(SOURCE_UPDATE_CMD)
                                                .font(t::mono(12.0))
                                                .color(t::TEXT)
                                                .background_color(t::RAISED),
                                        );
                                    });
                                }
                                let can_install = target.is_ok() && info.linux_asset_url.is_some();
                                ui.horizontal_wrapped(|ui| {
                                    if can_install
                                        && kit::button(
                                            ui,
                                            tr.update_install_now,
                                            kit::Kind::Primary,
                                            true,
                                        )
                                        .on_hover_text(format!(
                                            "Downloads v{} and replaces {}",
                                            info.version,
                                            target
                                                .as_ref()
                                                .map(|p| p.display().to_string())
                                                .unwrap_or_default()
                                        ))
                                        .clicked()
                                    {
                                        action_install = Some(info.clone());
                                    }
                                    if from_source
                                        && kit::button(
                                            ui,
                                            tr.update_copy_command,
                                            kit::Kind::Primary,
                                            true,
                                        )
                                        .clicked()
                                    {
                                        action_copy_cmd = true;
                                    }
                                    if kit::button(
                                        ui,
                                        tr.update_open_release,
                                        kit::Kind::Ghost,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        action_open_url = Some(info.html_url.clone());
                                    }
                                    if !can_install
                                        && !from_source
                                        && let Some(asset) = &info.linux_asset_url
                                        && kit::button(
                                            ui,
                                            tr.update_download_linux,
                                            kit::Kind::Ghost,
                                            true,
                                        )
                                        .clicked()
                                    {
                                        action_open_url = Some(asset.clone());
                                    }
                                    if kit::button(
                                        ui,
                                        tr.update_skip_version,
                                        kit::Kind::Ghost,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        action_skip = Some(info.version.clone());
                                    }
                                });
                                if let Err(e) = &target
                                    && e != "source"
                                {
                                    ui.label(RichText::new(e).font(t::sans(12.0)).color(t::FAINT));
                                }
                            }
                        }
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if super::settings_ui::kit_switch(ui, &mut self.settings.general.check_updates)
                    {
                        action_save_prefs = true;
                    }
                    ui.label(
                        RichText::new("Check automatically (daily)")
                            .font(t::sans(12.5))
                            .color(t::MUTED),
                    );
                    ui.add_space(12.0);
                    if super::settings_ui::kit_switch(ui, &mut self.settings.general.nightly_updates)
                    {
                        action_save_prefs = true;
                        action_retry = true;
                    }
                    ui.label(
                        RichText::new(tr.update_nightly)
                            .font(t::sans(12.5))
                            .color(t::MUTED),
                    );
                });
            });

        self.updater.dialog_open = open && !action_close;

        if let Some(info) = action_install {
            self.updater.start_install(&info, Some(ctx.clone()));
        }
        if let Some(exe) = action_restart {
            // The normal close path asks about unsaved work; on_exit starts the new build.
            self.relaunch = Some(exe);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

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
