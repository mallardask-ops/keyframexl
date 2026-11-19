//! Singing channels: the voice window (bank, voice settings), installing and
//! downloading voicebanks, and the lyrics helpers the piano roll uses.

use super::{
    App, Tone, kit,
    synth::{Edit, pair, panel, pct, secs, signed},
};
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
                self.say(Tone::Done, format!("{name} installed."));
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
        let mut v = voice.clone();
        let mut ed = Edit::default();
        let mut pick: Option<(String, String)> = None;
        let busy = self.vocal_job.as_ref().map(|(l, _)| l.clone());
        let status = self.audio.as_ref().map(|a| a.vocal.status.clone());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            panel(ui, "voicebank", t::ENC_BLUE, |ui| {
                let installed = install::installed();
                ui.horizontal_wrapped(|ui| {
                    let shown = if v.name.is_empty() { "No voicebank".to_string() } else { v.name.clone() };
                    egui::ComboBox::from_id_salt(("vocal-bank", id)).width(220.0).selected_text(shown).show_ui(ui, |ui| {
                        kit::menu_style(ui);
                        for b in &installed {
                            if ui.selectable_label(b.path.to_string_lossy() == v.bank.as_str(), &b.name).clicked() {
                                pick = Some((b.path.to_string_lossy().to_string(), b.name.clone()));
                            }
                        }
                        if installed.is_empty() {
                            ui.label(egui::RichText::new("None installed yet").color(t::MUTED));
                        }
                    });
                    if kit::button(ui, "Folder…", kit::Kind::Normal, busy.is_none()).on_hover_text("Use an unpacked UTAU voicebank folder (the one with oto.ini)").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        match vocal::oto::Voicebank::load(&dir) {
                            Ok(b) => pick = Some((dir.to_string_lossy().to_string(), b.name)),
                            Err(e) => self.say(Tone::Bad, e),
                        }
                    }
                    if kit::button(ui, "Install zip…", kit::Kind::Normal, busy.is_none()).on_hover_text("Install a downloaded UTAU voicebank (.zip) into KeyFrameXL's voicebank folder").clicked()
                        && let Some(zip) = rfd::FileDialog::new().add_filter("UTAU voicebank", &["zip"]).pick_file()
                    {
                        let name = zip.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Voicebank".into());
                        self.start_job(&format!("Installing {name}"), move || install::install_zip(&zip, &name));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let label = if busy.is_some() { "Working…" } else { "Download Kasane Teto" };
                    if kit::button(ui, label, kit::Kind::Primary, busy.is_none())
                        .on_hover_text("Fetch the free Kasane Teto UTAU voicebank from her official site (about 12 MB)")
                        .clicked()
                    {
                        self.start_job("Downloading Kasane Teto", install::download_teto);
                    }
                    ui.hyperlink_to(egui::RichText::new("Teto's terms of use").size(12.0), install::TETO_TERMS);
                });
                if let Some(label) = &busy {
                    ui.label(egui::RichText::new(format!("{label}…")).color(t::WAIT));
                }
                if v.bank.is_empty() {
                    ui.label(egui::RichText::new("This channel has no voice yet. Download Kasane Teto, install a .zip, or pick a bank folder. Any UTAU voicebank works.").color(t::MUTED));
                } else {
                    ui.label(egui::RichText::new(&v.bank).font(t::sans(11.5)).color(t::FAINT));
                }
                if let Some(st) = &status {
                    if st.busy() {
                        ui.label(egui::RichText::new(format!("Rendering the singing: {} of {}", st.done.load(std::sync::atomic::Ordering::Relaxed), st.total.load(std::sync::atomic::Ordering::Relaxed))).color(t::WAIT));
                    }
                    for m in st.messages() {
                        ui.label(egui::RichText::new(m).color(t::BAD));
                    }
                }
            });
            panel(ui, "voice", t::ENC_GREEN, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let c = t::ENC_GREEN;
                    let p = &mut v.params;
                    ed.knob(ui, "VIBRATO", c, &mut p.vibrato_depth, (0.0, 150.0, 35.0), false, &|x| format!("{x:.0} ct"), "Vibrato depth on long notes, in cents");
                    ed.knob(ui, "RATE", c, &mut p.vibrato_rate, (2.0, 9.0, 5.5), false, &|x| format!("{x:.1} Hz"), "Vibrato speed");
                    ed.knob(ui, "DELAY", c, &mut p.vibrato_delay, (0.0, 1.5, 0.3), false, &secs, "How long a note holds still before the vibrato starts");
                    ed.knob(ui, "FADE IN", c, &mut p.vibrato_fade, (0.0, 1.5, 0.25), false, &secs, "How slowly the vibrato grows to full depth");
                    ed.knob(ui, "GLIDE", c, &mut p.transition, (0.01, 0.3, 0.07), true, &secs, "How long the pitch takes to move between touching notes");
                    ed.knob(ui, "GENDER", c, &mut p.gender, (-1.0, 1.0, 0.0), false, &signed, "Formant shift: darker, bigger voice (−) or brighter, smaller voice (+). The pitch stays.");
                    ed.knob(ui, "BREATH", c, &mut p.breath, (0.0, 1.0, 0.0), false, &pct, "Breathiness");
                });
            });
            pair(ui, |ui, col| {
                if col == 0 {
                    panel(ui, "default lyric", t::ENC_ORANGE, |ui| {
                        ui.label(egui::RichText::new("What a note sings until you type a lyric for it.").color(t::MUTED));
                        let r = ui.add(egui::TextEdit::singleline(&mut v.default_lyric).desired_width(140.0));
                        if r.changed() {
                            if kit::first_edit(ui, &r) {
                                ed.started = true;
                            }
                            ed.changed = true;
                        }
                        kit::end_edit(ui, &r);
                    });
                } else {
                    panel(ui, "singing", t::ENC_WHITE, |ui| {
                        ui.label("Open the piano roll, draw notes, then double-click a note (or press F2) and type its lyric: ka, shi, tsu, か, カ, or Cyrillic.");
                        ui.label(egui::RichText::new("+  keeps the vowel of the note before   ·   -  is a rest   ·   Enter / Tab moves on to the next note").color(t::MUTED));
                        ui.label(egui::RichText::new("Tools → Lyrics… in the piano roll pastes a whole text over the notes. The event lane tunes each note: Velocity = loudness, Mod X = vibrato depth, Mod Y = gender.").color(t::MUTED));
                        if kit::button(ui, "Piano roll", kit::Kind::Normal, true).clicked() {
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
        let mut open = true;
        let mut apply = false;
        let targets = if self.roll_sel.is_empty() {
            self.vocal_notes_in_order().len()
        } else {
            self.roll_sel.len()
        };
        egui::Window::new("Lyrics").open(&mut open).default_width(420.0).collapsible(false).show(ctx, |ui| {
            ui.label(egui::RichText::new("Paste the words. Each syllable goes onto one note, in time order. Separate with spaces or hyphens: ko-n-ni-chi-ha, こんにちは, привіт. + keeps the last vowel, - is a rest.").color(t::MUTED));
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(6).desired_width(f32::INFINITY));
            let n = split_text(&text).len();
            ui.label(format!("{n} syllable(s) for {targets} note(s){}", if self.roll_sel.is_empty() { " in this channel" } else { " selected" }));
            ui.horizontal(|ui| {
                if kit::button(ui, "Apply", kit::Kind::Primary, n > 0 && targets > 0).clicked() {
                    apply = true;
                }
            });
        });
        if apply {
            let n = self.apply_lyrics_text(&text);
            self.say(Tone::Done, format!("{n} lyric(s) set. Ctrl+Z undoes."));
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
