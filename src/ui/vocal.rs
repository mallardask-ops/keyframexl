//! Singing channels: the voice window (bank, voice settings), installing and
//! downloading voicebanks, and the lyrics helpers the piano roll uses.

use super::{
    App, Tone, kit,
    synth::{Edit, pair, panel, pct, secs, signed},
};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Ui, vec2};
use keyframexl::{
    sequence::InstrumentKind,
    vocal::{self, Voice, install, lyric},
};

impl App {
    /// Per-frame: finish a download / install, and refresh singing when a bank arrived.
    pub(super) fn vocal_frame(&mut self, ctx: &egui::Context) {
        let Some((label, rx)) = &self.vocal_job else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(path)) => {
                let label = label.clone();
                self.vocal_job = None;
                let dir = path.to_string_lossy().to_string();
                // Channels that were waiting for a bank sing with it from now on.
                let name = install::installed()
                    .into_iter()
                    .find(|b| b.path == path)
                    .map(|b| b.name)
                    .unwrap_or_else(|| label.clone());
                self.checkpoint();
                let mut any = false;
                for i in &mut self.song.instruments {
                    if let InstrumentKind::Vocal { voice } = &mut i.kind
                        && voice.bank.is_empty()
                    {
                        voice.bank = dir.clone();
                        voice.name = name.clone();
                        any = true;
                    }
                }
                if !any {
                    self.history.pop();
                }
                if let Some(a) = &self.audio {
                    a.vocal.reload(&dir);
                }
                self.publish();
                let msg = fill(self.tr().vocal_installed, &[("name", &name)]);
                self.say(Tone::Done, msg);
            }
            Ok(Err(e)) => {
                self.vocal_job = None;
                self.say(Tone::Bad, e);
            }
            Err(mpsc_err) => {
                if matches!(mpsc_err, std::sync::mpsc::TryRecvError::Disconnected) {
                    self.vocal_job = None;
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }

    /// Add a singing channel; it takes the first installed voicebank (Teto first).
    pub(crate) fn add_vocal(&mut self) -> u64 {
        self.checkpoint();
        let banks = install::installed();
        let pick = banks
            .iter()
            .find(|b| b.name.contains("テト") || b.name.to_lowercase().contains("teto"))
            .or(banks.first());
        let voice = match pick {
            Some(b) => Voice::new(&b.path.to_string_lossy(), &b.name),
            None => Voice::new("", ""),
        };
        let name = self.song.free_name("Vocal");
        let id = self
            .song
            .add_channel(&name, InstrumentKind::Vocal { voice });
        self.publish();
        self.win(super::Cmd::Show(super::workspace::Tab::Instrument { id }));
        id
    }

    pub(crate) fn is_vocal(&self, id: u64) -> bool {
        self.song
            .instruments
            .iter()
            .any(|i| i.id == id && matches!(i.kind, InstrumentKind::Vocal { .. }))
    }

    fn start_job(
        &mut self,
        label: &str,
        work: impl FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static,
    ) {
        if self.vocal_job.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        self.vocal_job = Some((label.to_string(), rx));
    }

    pub(super) fn vocal_editor(&mut self, ui: &mut Ui, id: u64, voice: &Voice) {
        let tr = self.tr();
        let mut v = voice.clone();
        let mut ed = Edit::default();
        let mut pick: Option<(String, String)> = None;
        let busy = self.vocal_job.as_ref().map(|(l, _)| l.clone());
        let status = self.audio.as_ref().map(|a| a.vocal.status.clone());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                panel(ui, tr.vocal_sec_bank, t::enc_blue(), |ui| {
                    let installed = install::installed();
                    ui.horizontal_wrapped(|ui| {
                        let shown = if v.name.is_empty() {
                            tr.vocal_no_bank.to_string()
                        } else {
                            v.name.clone()
                        };
                        egui::ComboBox::from_id_salt(("vocal-bank", id))
                            .width(220.0)
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                kit::menu_style(ui);
                                for b in &installed {
                                    if ui
                                        .selectable_label(
                                            b.path.to_string_lossy() == v.bank.as_str(),
                                            &b.name,
                                        )
                                        .clicked()
                                    {
                                        pick = Some((
                                            b.path.to_string_lossy().to_string(),
                                            b.name.clone(),
                                        ));
                                    }
                                }
                                if installed.is_empty() {
                                    ui.label(
                                        egui::RichText::new(tr.vocal_none_installed)
                                            .color(t::muted()),
                                    );
                                }
                            });
                        if kit::button(ui, tr.vocal_folder_btn, kit::Kind::Normal, busy.is_none())
                            .on_hover_text(tr.vocal_folder_tip)
                            .clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            match vocal::oto::Voicebank::load(&dir) {
                                Ok(b) => pick = Some((dir.to_string_lossy().to_string(), b.name)),
                                Err(e) => self.say(Tone::Bad, e),
                            }
                        }
                        if kit::button(
                            ui,
                            tr.vocal_install_zip_btn,
                            kit::Kind::Normal,
                            busy.is_none(),
                        )
                        .on_hover_text(tr.vocal_install_zip_tip)
                        .clicked()
                            && let Some(zip) = rfd::FileDialog::new()
                                .add_filter(tr.vocal_dlg_utau_filter, &["zip"])
                                .pick_file()
                        {
                            let name = zip
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| tr.vocal_bank_fallback.into());
                            let job_label = fill(tr.vocal_installing, &[("name", &name)]);
                            self.start_job(&job_label, move || install::install_zip(&zip, &name));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let label = if busy.is_some() {
                            tr.vocal_working
                        } else {
                            tr.vocal_download_teto
                        };
                        if kit::button(ui, label, kit::Kind::Primary, busy.is_none())
                            .on_hover_text(tr.vocal_download_teto_tip)
                            .clicked()
                        {
                            self.start_job(tr.vocal_downloading_teto, install::download_teto);
                        }
                        ui.hyperlink_to(
                            egui::RichText::new(tr.vocal_teto_terms).size(12.0),
                            install::TETO_TERMS,
                        );
                    });
                    if let Some(label) = &busy {
                        ui.label(egui::RichText::new(format!("{label}…")).color(t::wait()));
                    }
                    if v.bank.is_empty() {
                        ui.label(egui::RichText::new(tr.vocal_empty_hint).color(t::muted()));
                    } else {
                        ui.label(
                            egui::RichText::new(&v.bank)
                                .font(t::sans(11.5))
                                .color(t::faint()),
                        );
                    }
                    if let Some(st) = &status {
                        if st.busy() {
                            let done = st
                                .done
                                .load(std::sync::atomic::Ordering::Relaxed)
                                .to_string();
                            let total = st
                                .total
                                .load(std::sync::atomic::Ordering::Relaxed)
                                .to_string();
                            ui.label(
                                egui::RichText::new(fill(
                                    tr.vocal_rendering_progress,
                                    &[("done", &done), ("total", &total)],
                                ))
                                .color(t::wait()),
                            );
                        }
                        for m in st.messages() {
                            ui.label(egui::RichText::new(m).color(t::bad()));
                        }
                    }
                });
                panel(ui, tr.vocal_sec_voice, t::enc_green(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        use keyframexl::automation::{Target, VocalParam};
                        let c = t::enc_green();
                        let p = &mut v.params;
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::VibratoDepth,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_vibrato,
                            c,
                            &mut p.vibrato_depth,
                            (0.0, 150.0, 35.0),
                            false,
                            &|x| format!("{x:.0} ct"),
                            tr.vocal_k_vibrato_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::VibratoRate,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_rate,
                            c,
                            &mut p.vibrato_rate,
                            (2.0, 9.0, 5.5),
                            false,
                            &|x| format!("{x:.1} Hz"),
                            tr.vocal_k_rate_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::VibratoDelay,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_delay,
                            c,
                            &mut p.vibrato_delay,
                            (0.0, 1.5, 0.3),
                            false,
                            &secs,
                            tr.vocal_k_delay_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::VibratoFade,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_fade_in,
                            c,
                            &mut p.vibrato_fade,
                            (0.0, 1.5, 0.25),
                            false,
                            &secs,
                            tr.vocal_k_fade_in_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::Transition,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_glide,
                            c,
                            &mut p.transition,
                            (0.01, 0.3, 0.07),
                            true,
                            &secs,
                            tr.vocal_k_glide_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::Gender,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_gender,
                            c,
                            &mut p.gender,
                            (-1.0, 1.0, 0.0),
                            false,
                            &signed,
                            tr.vocal_k_gender_hint,
                        );
                        kit::automatable(
                            ui,
                            Target::VocalParam {
                                inst: id,
                                param: VocalParam::Breath,
                            },
                        );
                        ed.knob(
                            ui,
                            tr.vocal_k_breath,
                            c,
                            &mut p.breath,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.vocal_k_breath_hint,
                        );
                    });
                });
                pair(ui, |ui, col| {
                    if col == 0 {
                        panel(ui, tr.vocal_sec_default_lyric, t::enc_orange(), |ui| {
                            ui.label(
                                egui::RichText::new(tr.vocal_default_lyric_desc).color(t::muted()),
                            );
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut v.default_lyric)
                                    .desired_width(140.0),
                            );
                            if r.changed() {
                                if kit::first_edit(ui, &r) {
                                    ed.started = true;
                                }
                                ed.changed = true;
                            }
                            kit::end_edit(ui, &r);
                        });
                    } else {
                        panel(ui, tr.vocal_sec_singing, t::enc_white(), |ui| {
                            ui.label(tr.vocal_singing_p1);
                            ui.label(egui::RichText::new(tr.vocal_singing_p2).color(t::muted()));
                            ui.label(egui::RichText::new(tr.vocal_singing_p3).color(t::muted()));
                            if kit::button(ui, tr.win_roll, kit::Kind::Normal, true).clicked() {
                                self.open_roll(id);
                            }
                        });
                    }
                });
            });
        if let Some((dir, name)) = pick {
            v.bank = dir;
            v.name = name;
            ed.started = true;
            ed.changed = true;
        }
        if ed.started {
            self.checkpoint();
        }
        if ed.changed {
            if let Some(InstrumentKind::Vocal { voice }) = self
                .song
                .instruments
                .iter_mut()
                .find(|i| i.id == id)
                .map(|i| &mut i.kind)
            {
                *voice = v;
            }
            self.publish();
        }
    }

    // ------------------------------------------------------------- lyrics

    /// Set (or clear) a note's lyric; one undo step.
    pub(crate) fn set_lyric(&mut self, note: u64, text: &str) {
        let text = text.trim();
        let current = self
            .pattern()
            .lyrics
            .get(&note)
            .map(String::as_str)
            .unwrap_or("");
        if current == text {
            return;
        }
        self.checkpoint();
        let p = self.pattern_mut();
        if text.is_empty() {
            p.lyrics.remove(&note);
        } else {
            p.lyrics.insert(note, text.to_string());
        }
        self.publish();
    }

    /// The channel's notes of this pattern, in time order.
    pub(crate) fn vocal_notes_in_order(&self) -> Vec<u64> {
        let me = self.roll_inst;
        let mut notes: Vec<_> = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == me)
            .collect();
        notes.sort_by_key(|n| (n.start, n.id));
        notes.into_iter().map(|n| n.id).collect()
    }

    pub(crate) fn start_lyric_edit(&mut self, note: u64) {
        let text = self
            .pattern()
            .lyrics
            .get(&note)
            .cloned()
            .unwrap_or_default();
        self.lyric_edit = Some((note, text));
    }

    /// Spread a text over the notes (the selection, else the whole channel in
    /// this pattern), one syllable per note.
    pub(crate) fn apply_lyrics_text(&mut self, text: &str) -> usize {
        let order = self.vocal_notes_in_order();
        let targets: Vec<u64> = if self.roll_sel.is_empty() {
            order
        } else {
            order
                .into_iter()
                .filter(|id| self.roll_sel.contains(id))
                .collect()
        };
        let syllables = split_text(text);
        if syllables.is_empty() || targets.is_empty() {
            return 0;
        }
        self.checkpoint();
        let mut n = 0;
        let p = self.pattern_mut();
        for (id, s) in targets.iter().zip(&syllables) {
            p.lyrics.insert(*id, s.clone());
            n += 1;
        }
        self.publish();
        n
    }

    /// Window with a text box; Apply writes it over the notes.
    pub(crate) fn lyrics_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.lyrics_dialog.take() else {
            return;
        };
        let tr = self.tr();
        let lang = tr.lang;
        let mut open = true;
        let mut apply = false;
        let targets = if self.roll_sel.is_empty() {
            self.vocal_notes_in_order().len()
        } else {
            self.roll_sel.len()
        };
        egui::Window::new(tr.vocal_lyrics_title)
            .id(egui::Id::new("kfxl-lyrics-window"))
            .open(&mut open)
            .default_width(420.0)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(tr.vocal_lyrics_desc).color(t::muted()));
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                let n = split_text(&text).len();
                let syl = tr.vocal_syllables.fmt(lang, n);
                let notes = if self.roll_sel.is_empty() {
                    tr.vocal_notes_in_channel.fmt(lang, targets)
                } else {
                    tr.vocal_notes_selected.fmt(lang, targets)
                };
                ui.label(fill(
                    tr.vocal_lyrics_summary,
                    &[("syllables", &syl), ("notes", &notes)],
                ));
                ui.horizontal(|ui| {
                    if kit::button(
                        ui,
                        tr.common_apply,
                        kit::Kind::Primary,
                        n > 0 && targets > 0,
                    )
                    .clicked()
                    {
                        apply = true;
                    }
                });
            });
        if apply {
            let n = self.apply_lyrics_text(&text);
            let msg = tr.vocal_lyrics_set.fmt(lang, n);
            self.say(Tone::Done, msg);
            return;
        }
        if open {
            self.lyrics_dialog = Some(text);
        }
    }
}

/// A text as one entry per note: words by whitespace and hyphens, each into
/// its syllables (kana, romaji and Cyrillic split; `+` and `-` stay).
pub fn split_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || c == '-') {
        if word.is_empty() {
            continue;
        }
        match lyric::parse(word) {
            lyric::Token::Rest => out.push("-".into()),
            lyric::Token::Hold => out.push("+".into()),
            lyric::Token::Sing(list) => out.extend(
                list.into_iter()
                    .map(|s| if s == "ー" { "+".to_string() } else { s }),
            ),
        }
    }
    out
}
