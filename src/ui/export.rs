//! Export audio (FL: File → Export → Wave + the Rendering dialog). The file is
//! chosen first, then this window sets source, tail, formats and options;
//! Start renders on a worker thread with progress and Cancel.

use super::{App, Tone, kit};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Align2, RichText, vec2};
use keyframexl::render::{self, Format, Progress, Tail};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

/// Choices that carry over from one export to the next.
#[derive(Clone, Debug)]
pub struct ExportPrefs {
    pub tail: Tail,
    pub wav: Option<Format>,
    pub flac: Option<Format>,
    pub mp3: Option<Format>,
    pub normalize: bool,
    pub dither: bool,
    /// FL "Split mixer tracks": a file per used insert besides the full mix.
    pub split: bool,
}

impl Default for ExportPrefs {
    fn default() -> Self {
        Self {
            tail: Tail::Leave,
            wav: Some(Format::Wav24),
            flac: None,
            mp3: None,
            normalize: false,
            dither: true,
            split: false,
        }
    }
}

pub struct ExportDialog {
    pub path: PathBuf,
    pub song_mode: bool,
    /// Song mode, only the playlist's time selection.
    pub selection: bool,
    running: Option<(mpsc::Receiver<Progress>, Arc<AtomicBool>)>,
    progress: f32,
    stage: String,
    error: Option<String>,
}

const WAV: [Option<Format>; 4] = [
    None,
    Some(Format::Wav16),
    Some(Format::Wav24),
    Some(Format::Wav32Float),
];
const FLAC: [Option<Format>; 3] = [None, Some(Format::Flac16), Some(Format::Flac24)];
const MP3: [Option<Format>; 4] = [
    None,
    Some(Format::Mp3_128),
    Some(Format::Mp3_192),
    Some(Format::Mp3_320),
];

fn clock(seconds: f64) -> String {
    format!(
        "{}:{:06.3}",
        (seconds / 60.0).floor() as u64,
        seconds % 60.0
    )
}

impl App {
    /// Ctrl+R / FILE → Export → Audio file: pick the file, then the dialog.
    pub(crate) fn export(&mut self) {
        let tr = self.tr();
        let song_mode = !self.song.clips.is_empty();
        let name = format!(
            "{}.{}",
            self.project_name(),
            self.export_prefs
                .wav
                .or(self.export_prefs.flac)
                .or(self.export_prefs.mp3)
                .map(Format::ext)
                .unwrap_or("wav")
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.exp_audio_filter, &["wav", "flac", "mp3"])
            .set_file_name(name)
            .save_file()
        {
            self.open_export(path, song_mode);
        }
    }

    pub(crate) fn open_export(&mut self, path: PathBuf, song_mode: bool) {
        // The extension the person typed picks the format family.
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("flac") if self.export_prefs.flac.is_none() => {
                self.export_prefs.flac = Some(Format::Flac24);
                self.export_prefs.wav = None;
            }
            Some("wav") if self.export_prefs.wav.is_none() => {
                self.export_prefs.wav = Some(Format::Wav24)
            }
            Some("mp3") if self.export_prefs.mp3.is_none() && keyframexl::mp3::available() => {
                self.export_prefs.mp3 = Some(Format::Mp3_320);
                self.export_prefs.wav = None;
                self.export_prefs.flac = None;
            }
            _ => {}
        }
        // A time selection in the song is what gets exported first (FL).
        let selection = song_mode && self.song.loop_range.is_some();
        self.export = Some(ExportDialog {
            path,
            song_mode,
            selection,
            running: None,
            progress: 0.0,
            stage: String::new(),
            error: None,
        });
    }

    fn export_job(&self, d: &ExportDialog) -> render::Job {
        let p = &self.export_prefs;
        render::Job {
            song: self.song.clone(),
            pattern: self.selected,
            song_mode: d.song_mode,
            range: if d.song_mode && d.selection {
                self.song.loop_range
            } else {
                None
            },
            tail: p.tail,
            formats: p
                .wav
                .into_iter()
                .chain(p.flac)
                .chain(p.mp3.filter(|_| keyframexl::mp3::available()))
                .collect(),
            normalize: p.normalize,
            dither: p.dither,
            path: d.path.clone(),
            rate: self.rate(),
            plugins: render::Plugins(
                self.plugins
                    .live
                    .iter()
                    .filter_map(|(id, l)| Some((*id, l.proc.clone()?)))
                    .collect(),
            ),
        }
    }

    /// The full mix, then (split) one job per used insert, named
    /// "<file> - 03 Drums" beside it.
    fn export_jobs(&self, d: &ExportDialog) -> Vec<render::Job> {
        let main = self.export_job(d);
        if !self.export_prefs.split {
            return vec![main];
        }
        let stem = d
            .path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut jobs = vec![];
        for (t, song) in render::stems(&self.song) {
            // No dots or slashes: the format sets the extension.
            let name: String = self
                .mixer_name(t)
                .chars()
                .map(|c| {
                    if matches!(c, '/' | '\\' | '.') || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            let mut job = self.export_job(d);
            job.song = song;
            job.path = d
                .path
                .with_file_name(format!("{stem} - {t:02} {}", name.trim()));
            jobs.push(job);
        }
        std::iter::once(main).chain(jobs).collect()
    }

    /// Start rendering on a worker thread.
    pub(crate) fn start_export(&mut self) {
        let tr = self.tr();
        if self.export.is_none() {
            return;
        }
        if let Some(error) = self
            .song
            .instruments
            .iter()
            .filter(|i| !i.muted)
            .filter_map(|i| i.plugin().map(|p| (i, p)))
            .filter(|(_, p)| crate::fl_native::slot(&p.path).is_some())
            .find_map(|(i, p)| {
                let live = self.plugins.live.get(&p.id);
                live.is_none_or(|l| l.proc.is_none()).then(|| {
                    format!(
                        "{} — {}",
                        i.name,
                        live.and_then(|l| l.error.as_deref())
                            .unwrap_or(tr.exp_waiting_fl)
                    )
                })
            })
        {
            if let Some(d) = &mut self.export {
                d.error = Some(error);
            }
            return;
        }
        // A Forge sound still rendering from the last knob turn goes out as it is now.
        keyframexl::forge::refresh_all(&mut self.song);
        let Some(d) = &self.export else { return };
        let jobs = self.export_jobs(d);
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        // The live output goes quiet and leaves the plugins to the render.
        let hold = self
            .audio
            .as_ref()
            .filter(|_| jobs.iter().any(|j| !j.plugins.0.is_empty()))
            .map(|a| a.hold_plugins.clone());
        if let Some(h) = &hold {
            h.store(true, Ordering::Relaxed);
        }
        std::thread::spawn(move || {
            // Jobs one after another; progress spans all of them, files add up.
            let n = jobs.len() as f32;
            let mut files = Vec::new();
            let mut total: Option<render::Stats> = None;
            for (i, job) in jobs.iter().enumerate() {
                let mut end = None;
                render::run(job, &c, &mut |p| match p {
                    Progress::Rendering(f) => {
                        let _ = tx.send(Progress::Rendering((i as f32 + f) / n));
                    }
                    Progress::Done(..) | Progress::Cancelled | Progress::Failed(_) => end = Some(p),
                    other => {
                        let _ = tx.send(other);
                    }
                });
                match end {
                    Some(Progress::Done(written, stats)) => {
                        files.extend(written);
                        // The full mix speaks for length and clipping.
                        total.get_or_insert(stats);
                    }
                    Some(other) => {
                        let _ = tx.send(other);
                        files.clear();
                        break;
                    }
                    None => {}
                }
            }
            if let Some(stats) = total.filter(|_| !files.is_empty()) {
                let _ = tx.send(Progress::Done(files, stats));
            }
            if let Some(h) = hold {
                h.store(false, Ordering::Relaxed);
            }
        });
        if let Some(d) = &mut self.export {
            d.running = Some((rx, cancel));
            d.progress = 0.0;
            d.stage = tr.exp_rendering.into();
            d.error = None;
        }
    }

    /// Live check: `KFXL_CAPTURE_EXPORT=out.wav` exports the song once the
    /// project's samples and plugins have loaded (or after
    /// `KFXL_CAPTURE_EXPORT_WAIT` seconds, default 120), through the same
    /// export as the dialog, then closes the window.
    pub(crate) fn capture_export(&mut self, ctx: &egui::Context) {
        use std::sync::Mutex;
        // None = not started, Some(false) = rendering, Some(true) = done.
        static STATE: Mutex<Option<bool>> = Mutex::new(None);
        let Ok(path) = std::env::var("KFXL_CAPTURE_EXPORT") else {
            return;
        };
        let Ok(mut state) = STATE.lock() else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        match *state {
            None => {
                let wait = std::env::var("KFXL_CAPTURE_EXPORT_WAIT")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(900.0);
                let refs: Vec<u64> = self.song.plugin_refs().iter().map(|p| p.id).collect();
                let pending: Vec<String> = refs
                    .iter()
                    .filter(|id| {
                        self.plugins
                            .live
                            .get(id)
                            .is_none_or(|l| l.proc.is_none() && l.error.is_none())
                    })
                    .map(|id| id.to_string())
                    .collect();
                let loading = !self.importing.is_empty()
                    || !self.load_queue.is_empty()
                    || self.flp_freeze.is_some();
                let time = ctx.input(|i| i.time);
                if time < 3.0 || ((loading || !pending.is_empty()) && time < wait) {
                    return;
                }
                for (id, l) in &self.plugins.live {
                    if let Some(e) = &l.error {
                        eprintln!("Export: plugin {id} ({}) failed: {e}", l.name);
                    }
                }
                eprintln!(
                    "Export: starting after {time:.1} s ({} plugin(s) still loading, samples loading: {loading})",
                    pending.len()
                );
                self.open_export(PathBuf::from(&path), !self.song.clips.is_empty());
                // KFXL_CAPTURE_STEMS=1: first each channel alone and dry (no
                // mixer effects, faders at 0 dB), not normalized, in a folder
                // beside the file.
                if std::env::var_os("KFXL_CAPTURE_STEMS").is_some()
                    && let Some(d) = &self.export
                {
                    let dir = PathBuf::from(&path).with_extension("");
                    let _ = std::fs::create_dir_all(&dir);
                    let mut jobs = vec![];
                    for inst in &self.song.instruments {
                        if !self
                            .song
                            .patterns
                            .iter()
                            .any(|p| p.notes.iter().any(|n| n.instrument == inst.id))
                        {
                            continue;
                        }
                        let mut job = self.export_job(d);
                        job.tail = Tail::Cut;
                        job.normalize = false;
                        job.formats = vec![Format::Wav24];
                        for i in &mut job.song.instruments {
                            i.muted = i.id != inst.id;
                        }
                        for t in &mut job.song.mixer {
                            t.fx.iter_mut().for_each(|f| f.enabled = false);
                            t.volume = 1.0;
                            t.pan = 0.0;
                            t.muted = false;
                            t.sends.clear();
                            t.to_master = true;
                            t.eq = [0.0; 3];
                            t.sep = 0.0;
                        }
                        job.path = dir.join(inst.name.replace('/', "_"));
                        jobs.push(job);
                    }
                    let hold = self.audio.as_ref().map(|a| a.hold_plugins.clone());
                    if let Some(h) = &hold {
                        h.store(true, Ordering::Relaxed);
                    }
                    eprintln!("Export: {} stems into {}", jobs.len(), dir.display());
                    let cancel = AtomicBool::new(false);
                    for job in &jobs {
                        render::run(job, &cancel, &mut |p| {
                            if let Progress::Failed(e) = p {
                                eprintln!("Stem failed: {e}");
                            }
                        });
                    }
                    if let Some(h) = hold {
                        h.store(false, Ordering::Relaxed);
                    }
                }
                self.start_export();
                *state = Some(false);
            }
            Some(false) => {
                self.poll_export();
                if self.export.is_none() {
                    eprintln!("Export: {}", self.status.1);
                    *state = Some(true);
                    self.quit_ok = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else if let Some(e) = self.export.as_ref().and_then(|d| d.error.clone()) {
                    eprintln!("Export failed: {e}");
                    *state = Some(true);
                    self.quit_ok = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Some(true) => {}
        }
    }

    /// Progress from the worker; the result closes the dialog or shows the error.
    pub(crate) fn poll_export(&mut self) {
        let tr = self.tr();
        let Some(d) = &mut self.export else { return };
        let Some((rx, _)) = &d.running else { return };
        let mut finished = None;
        while let Ok(p) = rx.try_recv() {
            match p {
                Progress::Rendering(f) => {
                    d.progress = f;
                    d.stage = fill(
                        tr.exp_rendering_pct,
                        &[("pct", &format!("{:.0}", f * 100.0))],
                    );
                }
                Progress::Encoding(f) => d.stage = fill(tr.exp_writing, &[("format", f.label())]),
                other => finished = Some(other),
            }
        }
        match finished {
            Some(Progress::Done(files, stats)) => {
                let mut names: Vec<String> = files
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                if names.len() > 3 {
                    let more = names.len() - 1;
                    names.truncate(1);
                    names.push(tr.exp_stem_files.fmt(tr.lang, more));
                }
                let clip = if stats.clipped { tr.exp_clipped } else { "" };
                let tone = if stats.clipped { Tone::Bad } else { Tone::Done };
                self.export = None;
                self.say(
                    tone,
                    fill(
                        tr.exp_done,
                        &[
                            ("files", &names.join(", ")),
                            ("len", &clock(stats.seconds)),
                            ("db", &format!("{:.1}", stats.peak_db)),
                            ("clip", clip),
                        ],
                    ),
                );
            }
            Some(Progress::Cancelled) => {
                d.running = None;
                d.stage.clear();
                d.error = Some(tr.exp_cancelled.into());
            }
            Some(Progress::Failed(e)) => {
                d.running = None;
                d.stage.clear();
                d.error = Some(e);
            }
            _ => {}
        }
    }

    pub(crate) fn export_window(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        self.poll_export();
        let Some(d) = &self.export else { return };
        let running = d.running.is_some();
        let (path, song_mode, progress, stage, error) = (
            d.path.clone(),
            d.song_mode,
            d.progress,
            d.stage.clone(),
            d.error.clone(),
        );
        let selection = d.selection && song_mode && self.song.loop_range.is_some();
        let mut open = true;
        let mut close = false;
        let mut start = false;
        let mut pick_path = false;
        let mut set_song = None;
        let pattern_name = self.pattern().name.clone();
        let len_pattern =
            render::main_frames(&self.song, self.selected, false) as f64 / render::RATE as f64;
        let len_song =
            render::main_frames(&self.song, self.selected, true) as f64 / render::RATE as f64;
        // The time selection's length (and its bars, for the chip).
        let sel = self.song.loop_range.map(|(a, b)| {
            let rate = self.rate();
            let table = (!self.song.tempo_map.is_empty()).then(|| {
                keyframexl::tempo::TempoTable::build(&self.song.tempo_map, self.song.tempo, rate)
            });
            let f = |t| {
                keyframexl::sequence::tick_frames(&self.song, table.as_ref(), t, rate) as f64
                    / rate as f64
            };
            (
                super::playlist::song_bar_pos(&self.song.meters, a),
                super::playlist::song_bar_pos(&self.song.meters, b),
                f(b) - f(a),
            )
        });
        let mut set_sel = None;
        let has_clips = !self.song.clips.is_empty();
        let stem_count = render::stems(&self.song).len();
        let mut prefs = self.export_prefs.clone();
        egui::Window::new(tr.exp_title)
            .collapsible(false)
            .open(&mut open)
            .resizable(true)
            .default_size(vec2(490.0, 575.0))
            .min_size(vec2(425.0, 260.0))
            .default_pos(egui::pos2(ctx.screen_rect().center().x - 245.0, 110.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let label = |ui: &mut egui::Ui, text: &str| {
                    ui.add_space(4.0);
                    let r = ui
                        .allocate_exact_size(vec2(ui.available_width(), 14.0), egui::Sense::hover())
                        .0;
                    kit::caps(
                        ui.painter(),
                        r.left_center(),
                        Align2::LEFT_CENTER,
                        text,
                        12.0,
                        t::faint(),
                    );
                };
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - 96.0).max(130.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!running, |ui| {
                            label(ui, tr.exp_sec_file);
                            // The name matters most; the folder is context.
                            let stem = path
                                .file_stem()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let folder = path
                                .parent()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                let shown = kit::elide(
                                    ui.painter(),
                                    &stem,
                                    t::medium(16.0),
                                    (ui.available_width() - 90.0).max(70.0),
                                );
                                ui.label(
                                    RichText::new(shown).font(t::medium(16.0)).color(t::text()),
                                )
                                .on_hover_text(path.display().to_string());
                                if kit::tool_button(ui, tr.exp_change, kit::Kind::Ghost, true)
                                    .clicked()
                                {
                                    pick_path = true;
                                }
                            });
                            let folder = kit::elide(ui.painter(), &folder, t::sans(12.0), 400.0);
                            ui.label(
                                RichText::new(fill(tr.exp_in_folder, &[("folder", &folder)]))
                                    .font(t::sans(12.0))
                                    .color(t::faint()),
                            );
                            ui.separator();
                            label(ui, tr.exp_sec_source);
                            let mut items = vec![
                                fill(
                                    tr.exp_src_pattern,
                                    &[("name", &pattern_name), ("len", &clock(len_pattern))],
                                ),
                                fill(tr.exp_src_song, &[("len", &clock(len_song))]),
                            ];
                            if let Some((a, b, secs)) = &sel {
                                items.push(fill(
                                    tr.exp_src_selection,
                                    &[
                                        ("a", &a.to_string()),
                                        ("b", &b.to_string()),
                                        ("len", &clock(*secs)),
                                    ],
                                ));
                            }
                            let items: Vec<&str> = items.iter().map(String::as_str).collect();
                            let at = if selection { 2 } else { song_mode as usize };
                            let (pick, _) = kit::chips(ui, "export-source", &items, at, None);
                            if let Some(i) = pick {
                                set_song = Some(i >= 1);
                                set_sel = Some(i == 2);
                            }
                            if !has_clips {
                                ui.label(
                                    RichText::new(tr.exp_empty_playlist)
                                        .font(t::sans(12.0))
                                        .color(t::faint()),
                                );
                            }
                            label(ui, tr.exp_sec_tail);
                            let tails: Vec<&str> =
                                Tail::ALL.iter().map(|x| tr.export_tail(*x)).collect();
                            let (pick, _) = kit::chips(
                                ui,
                                "export-tail",
                                &tails,
                                Tail::ALL.iter().position(|x| *x == prefs.tail).unwrap_or(0),
                                None,
                            );
                            if let Some(i) = pick {
                                prefs.tail = Tail::ALL[i];
                            }
                            ui.label(
                                RichText::new(tr.export_tail_hint(prefs.tail))
                                    .font(t::sans(12.0))
                                    .color(t::faint()),
                            );
                            ui.separator();
                            label(ui, tr.exp_sec_formats);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("WAV").font(t::sans(13.0)).color(t::muted()),
                                );
                                let names = [tr.exp_off, "16-bit", "24-bit", "32 float"];
                                let (pick, _) = kit::chips(
                                    ui,
                                    "export-wav",
                                    &names,
                                    WAV.iter().position(|f| *f == prefs.wav).unwrap_or(0),
                                    None,
                                );
                                if let Some(i) = pick {
                                    prefs.wav = WAV[i];
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("FLAC").font(t::sans(13.0)).color(t::muted()),
                                );
                                let names = [tr.exp_off, "16-bit", "24-bit"];
                                let (pick, _) = kit::chips(
                                    ui,
                                    "export-flac",
                                    &names,
                                    FLAC.iter().position(|f| *f == prefs.flac).unwrap_or(0),
                                    None,
                                );
                                if let Some(i) = pick {
                                    prefs.flac = FLAC[i];
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("MP3").font(t::sans(13.0)).color(t::muted()),
                                );
                                if keyframexl::mp3::available() {
                                    let names = [tr.exp_off, "128", "192", "320 kbps"];
                                    let (pick, _) = kit::chips(
                                        ui,
                                        "export-mp3",
                                        &names,
                                        MP3.iter().position(|f| *f == prefs.mp3).unwrap_or(0),
                                        None,
                                    );
                                    if let Some(i) = pick {
                                        prefs.mp3 = MP3[i];
                                    }
                                } else {
                                    ui.label(
                                        RichText::new(tr.exp_needs_lame)
                                            .font(t::sans(12.0))
                                            .color(t::faint()),
                                    );
                                }
                            });
                            let mut open_project_info = false;
                            if prefs.mp3.is_some() && keyframexl::mp3::available() {
                                let i = &self.song.info;
                                let mut tags = [
                                    (tr.exp_tag_title, &i.title),
                                    (tr.exp_tag_author, &i.author),
                                    (tr.exp_tag_album, &i.album),
                                    (tr.exp_tag_genre, &i.genre),
                                    (tr.exp_tag_year, &i.year),
                                    (tr.exp_tag_copyright, &i.copyright),
                                ]
                                .iter()
                                .filter(|(_, v)| !v.trim().is_empty())
                                .map(|(k, v)| format!("{k} “{}”", v.trim()))
                                .collect::<Vec<_>>();
                                if !i.url.trim().is_empty() {
                                    tags.push(tr.exp_tag_url.into());
                                }
                                if !i.comments.trim().is_empty() {
                                    tags.push(tr.exp_tag_comments.into());
                                }
                                let note = if tags.is_empty() {
                                    tr.exp_tags_none.to_string()
                                } else {
                                    fill(tr.exp_tags, &[("tags", &tags.join(", "))])
                                };
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(note).font(t::sans(12.0)).color(t::faint()),
                                    );
                                    if kit::tool_button(
                                        ui,
                                        tr.info_edit_from_export,
                                        kit::Kind::Ghost,
                                        !running,
                                    )
                                    .clicked()
                                    {
                                        open_project_info = true;
                                    }
                                });
                            }
                            if open_project_info {
                                self.open_info(ui.ctx());
                            }
                            label(ui, tr.exp_sec_options);
                            ui.checkbox(
                                &mut prefs.normalize,
                                RichText::new(tr.exp_normalize).font(t::sans(13.0)),
                            );
                            let any16 = prefs.wav == Some(Format::Wav16)
                                || prefs.flac == Some(Format::Flac16);
                            ui.add_enabled(
                                any16,
                                egui::Checkbox::new(
                                    &mut prefs.dither,
                                    RichText::new(tr.exp_dither).font(t::sans(13.0)),
                                ),
                            )
                            .on_disabled_hover_text(tr.exp_dither_only16);
                            ui.checkbox(
                                &mut prefs.split,
                                RichText::new(tr.exp_split.fmt(tr.lang, stem_count))
                                    .font(t::sans(13.0)),
                            )
                            .on_hover_text(tr.exp_split_tip);
                        });
                    });
                // Size estimate (FLAC: upper bound) and the result line.
                let secs = if selection {
                    sel.as_ref().map_or(len_song, |s| s.2)
                } else if song_mode {
                    len_song
                } else {
                    len_pattern
                } + if prefs.tail == Tail::Leave { 2.0 } else { 0.0 };
                let secs = secs
                    * if prefs.split {
                        1.0 + stem_count as f64
                    } else {
                        1.0
                    };
                let bytes: u64 = prefs
                    .wav
                    .into_iter()
                    .chain(prefs.flac)
                    .chain(prefs.mp3)
                    .map(|f| (f.bytes_per_second() as f64 * secs) as u64)
                    .sum();
                ui.separator();
                let rate = self.rate();
                let mb = bytes as f64 / 1e6 * rate as f64 / render::RATE as f64;
                ui.label(
                    RichText::new(fill(
                        tr.exp_size,
                        &[
                            ("mb", &format!("{mb:.1}")),
                            ("khz", &format!("{:.1}", rate as f64 / 1000.0)),
                        ],
                    ))
                    .font(t::sans(12.0))
                    .color(t::faint()),
                );
                if let Some(e) = &error {
                    ui.label(RichText::new(e).font(t::sans(13.0)).color(t::bad()));
                }
                if running {
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .desired_width(400.0)
                            .text(RichText::new(&stage).font(t::sans(12.0))),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let none = prefs.wav.is_none()
                        && prefs.flac.is_none()
                        && (prefs.mp3.is_none() || !keyframexl::mp3::available());
                    if !running {
                        let r = kit::button(ui, tr.exp_start, kit::Kind::Primary, !none);
                        let r = if none {
                            r.on_disabled_hover_text(tr.exp_choose_format)
                        } else {
                            r
                        };
                        if r.clicked() || (!none && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                            start = true;
                        }
                    }
                    if kit::button(ui, tr.common_cancel, kit::Kind::Normal, true).clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        close = true;
                    }
                });
            });
        self.export_prefs = prefs;
        if let Some(v) = set_sel
            && let Some(d) = &mut self.export
        {
            d.selection = v;
        }
        if let Some(v) = set_song
            && let Some(d) = &mut self.export
        {
            d.song_mode = v;
        }
        if pick_path {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if let Some(p) = rfd::FileDialog::new()
                .add_filter(tr.exp_audio_filter, &["wav", "flac", "mp3"])
                .set_file_name(name)
                .save_file()
                && let Some(d) = &mut self.export
            {
                d.path = p;
            }
        }
        if start {
            self.start_export();
        }
        if close || !open {
            match self.export.as_ref().and_then(|d| d.running.as_ref()) {
                // Cancel while rendering stops the worker; the dialog stays to say so.
                Some((_, cancel)) => cancel.store(true, Ordering::Relaxed),
                None => self.export = None,
            }
        }
        if self.export.as_ref().is_some_and(|d| d.running.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
