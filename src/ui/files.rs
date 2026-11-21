//! Browser Files tab (FL browser): user folders as a tree, read lazily in the
//! background. Click an audio file to hear it, again or Esc to stop. Double-click
//! or Enter loads: audio and .dwp into the selected sampler channel (else a new
//! one), .kfxl opens. Drag a file onto a channel, the rack or an instrument
//! window. Roots and open folders persist in ~/.config/keyframexl/browser.ron.

use super::{App, Tone, kit};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

const ROW_H: f32 = 24.0;
/// File types the browser shows.
pub const SHOWN: [&str; 8] = ["wav", "flac", "ogg", "dwp", "kfxl", "flp", "mid", "midi"];

/// A file dragged out of the browser.
#[derive(Clone, Debug)]
pub struct FileDrag(pub PathBuf);

#[derive(Clone)]
struct RootOrder(PathBuf);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Reading,
    Unreadable,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

impl Entry {
    fn bare(path: PathBuf, name: String, dir: bool) -> Self {
        Self {
            path,
            name,
            dir,
            kind: if dir { EntryKind::Dir } else { EntryKind::File },
            size: 0,
            modified: None,
        }
    }

    fn placeholder(path: PathBuf, name: String, kind: EntryKind) -> Self {
        Self {
            path,
            name,
            dir: false,
            kind,
            size: 0,
            modified: None,
        }
    }

    fn is_placeholder(&self) -> bool {
        matches!(self.kind, EntryKind::Reading | EntryKind::Unreadable)
    }
}

/// Browser sort order; folders always come first (FL: sort by name / type).
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum Sort {
    #[default]
    Name,
    Type,
    Newest,
    Largest,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Name, Sort::Type, Sort::Newest, Sort::Largest];
    pub fn label(self, tr: &crate::i18n::Strings) -> &'static str {
        match self {
            Sort::Name => tr.fb_sort_name,
            Sort::Type => tr.fb_sort_type,
            Sort::Newest => tr.fb_sort_newest,
            Sort::Largest => tr.fb_sort_largest,
        }
    }
    fn order(self, v: &mut [Entry]) {
        let name = |e: &Entry| e.name.to_lowercase();
        v.sort_by(|a, b| {
            b.dir.cmp(&a.dir).then_with(|| match self {
                Sort::Name => name(a).cmp(&name(b)),
                Sort::Type => ext(&a.path)
                    .cmp(&ext(&b.path))
                    .then_with(|| name(a).cmp(&name(b))),
                Sort::Newest => b
                    .modified
                    .cmp(&a.modified)
                    .then_with(|| name(a).cmp(&name(b))),
                Sort::Largest => b.size.cmp(&a.size).then_with(|| name(a).cmp(&name(b))),
            })
        });
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    roots: Vec<PathBuf>,
    open: Vec<PathBuf>,
    #[serde(default)]
    sort: Sort,
}

/// Folders a search reads at most, and files it keeps (keeps the scan bounded
/// when a root is the home folder).
const INDEX_DIRS: usize = 20_000;
const INDEX_FILES: usize = 100_000;

/// Every usable file under the roots, read in the background for search.
#[derive(Default)]
struct Index {
    files: Vec<Entry>,
    dirs: usize,
    done: bool,
    rx: Option<mpsc::Receiver<IndexMsg>>,
}

enum IndexMsg {
    Files(Vec<Entry>, usize),
    Done,
}

pub struct Files {
    pub roots: Vec<PathBuf>,
    pub open: HashSet<PathBuf>,
    /// Folder listings; `None` while loading, `Err` when unreadable.
    listings: HashMap<PathBuf, Option<Result<Vec<Entry>, String>>>,
    tx: mpsc::Sender<(PathBuf, Result<Vec<Entry>, String>)>,
    rx: mpsc::Receiver<(PathBuf, Result<Vec<Entry>, String>)>,
    pub selected: Option<PathBuf>,
    persist: bool,
    pub sort: Sort,
    index: Index,
}

fn config() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("keyframexl").join("browser.ron"))
}

pub fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub fn is_audio(p: &Path) -> bool {
    matches!(ext(p).as_str(), "wav" | "flac" | "ogg")
}

/// One folder: subfolders and the files we can use, in `sort` order.
pub fn list(dir: &Path, sort: Sort) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut out: Vec<Entry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let meta = std::fs::metadata(&path).ok();
            let dir = meta.as_ref().is_some_and(|m| m.is_dir());
            (dir || SHOWN.contains(&ext(&path).as_str())).then(|| Entry {
                path,
                name,
                dir,
                kind: if dir { EntryKind::Dir } else { EntryKind::File },
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta.and_then(|m| m.modified().ok()),
            })
        })
        .collect();
    sort.order(&mut out);
    Ok(out)
}

/// Walk `roots` breadth-first in the background, sending usable files in batches.
fn spawn_index(roots: Vec<PathBuf>) -> mpsc::Receiver<IndexMsg> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut queue: std::collections::VecDeque<PathBuf> = roots.into();
        let (mut dirs, mut files) = (0, 0);
        let mut seen = HashSet::new();
        while let Some(dir) = queue.pop_front() {
            if dirs >= INDEX_DIRS || files >= INDEX_FILES {
                break;
            }
            // Symlink loops and roots inside roots are read once.
            if !seen.insert(dir.canonicalize().unwrap_or_else(|_| dir.clone())) {
                continue;
            }
            dirs += 1;
            let Ok(entries) = list(&dir, Sort::Name) else {
                continue;
            };
            let mut batch = Vec::new();
            for e in entries {
                if e.dir {
                    queue.push_back(e.path);
                } else {
                    files += 1;
                    batch.push(e);
                }
            }
            if tx.send(IndexMsg::Files(batch, dirs)).is_err() {
                return;
            }
        }
        let _ = tx.send(IndexMsg::Done);
    });
    rx
}

impl Files {
    pub fn new(persist: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let saved: Saved = if persist {
            config()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| ron::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Saved::default()
        };
        let mut roots = saved.roots;
        if roots.is_empty() {
            roots = [dirs::audio_dir(), dirs::home_dir()]
                .into_iter()
                .flatten()
                .filter(|p| p.is_dir())
                .collect();
            roots.dedup();
        }
        Self {
            roots,
            open: saved.open.into_iter().collect(),
            listings: HashMap::new(),
            tx,
            rx,
            selected: None,
            persist,
            sort: saved.sort,
            index: Index::default(),
        }
    }

    fn save(&self) {
        if !self.persist {
            return;
        }
        let s = Saved {
            roots: self.roots.clone(),
            open: self.open.iter().cloned().collect(),
            sort: self.sort,
        };
        if let (Some(p), Ok(text)) = (config(), ron::to_string(&s)) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, text);
        }
    }

    /// Listing of `dir`, starting a background read the first time.
    fn listing(&mut self, dir: &Path) -> Option<&Result<Vec<Entry>, String>> {
        if !self.listings.contains_key(dir) {
            self.listings.insert(dir.to_path_buf(), None);
            let (tx, d, sort) = (self.tx.clone(), dir.to_path_buf(), self.sort);
            std::thread::spawn(move || {
                let r = list(&d, sort);
                let _ = tx.send((d, r));
            });
        }
        self.listings.get(dir).and_then(|l| l.as_ref())
    }

    fn poll(&mut self) -> bool {
        let mut any = false;
        while let Ok((d, mut r)) = self.rx.try_recv() {
            // A read started before a sort change arrives in the old order.
            if let Ok(v) = &mut r {
                self.sort.order(v);
            }
            self.listings.insert(d, Some(r));
            any = true;
        }
        if let Some(rx) = &self.index.rx {
            while let Ok(msg) = rx.try_recv() {
                any = true;
                match msg {
                    IndexMsg::Files(batch, dirs) => {
                        self.index.files.extend(batch);
                        self.index.dirs = dirs;
                    }
                    IndexMsg::Done => self.index.done = true,
                }
            }
            if self.index.done {
                self.index.rx = None;
            }
        }
        any
    }

    pub fn refresh(&mut self) {
        self.listings.clear();
        self.index = Index::default();
    }

    /// Re-order every folder read so far; new reads use the order too.
    pub fn set_sort(&mut self, sort: Sort) {
        self.sort = sort;
        for v in self.listings.values_mut().flatten().flatten() {
            sort.order(v);
        }
        self.save();
    }

    /// Search progress: (folders read, finished).
    pub fn index_state(&self) -> (usize, bool) {
        (self.index.dirs, self.index.done)
    }

    /// Wait for the background search scan (tests).
    #[cfg(test)]
    pub fn wait_index(&mut self) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !self.index.done && std::time::Instant::now() < until {
            self.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    pub fn add_root(&mut self, p: PathBuf) {
        if !self.roots.contains(&p) {
            self.roots.push(p.clone());
            self.open.insert(p);
            self.save();
        }
    }

    pub fn toggle(&mut self, p: &Path) {
        if !self.open.remove(p) {
            self.open.insert(p.to_path_buf());
        }
        self.save();
    }

    /// Visible rows: (depth, entry, open). A filter shows matching files
    /// under every root, flat, in sort order; the first search starts a
    /// background scan of all subfolders.
    pub(crate) fn rows(
        &mut self,
        filter: &str,
        tr: &crate::i18n::Strings,
    ) -> Vec<(usize, Entry, bool)> {
        let mut out = Vec::new();
        let roots = self.roots.clone();
        if !filter.is_empty() {
            if self.index.rx.is_none() && !self.index.done {
                self.index.rx = Some(spawn_index(roots));
            }
            let q = filter.to_lowercase();
            let mut hits: Vec<Entry> = self
                .listings
                .values()
                .flatten()
                .flatten()
                .flatten()
                .chain(self.index.files.iter())
                .filter(|e| !e.dir && e.name.to_lowercase().contains(&q))
                .cloned()
                .collect();
            hits.sort_by(|a, b| a.path.cmp(&b.path));
            hits.dedup_by(|a, b| a.path == b.path);
            self.sort.order(&mut hits);
            return hits.into_iter().map(|e| (0, e, false)).collect();
        }
        for r in roots {
            let name = r
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| r.display().to_string());
            let open = self.open.contains(&r);
            out.push((0, Entry::bare(r.clone(), name, true), open));
            if open {
                self.walk(&r, 1, &mut out, tr);
            }
        }
        out
    }

    fn walk(
        &mut self,
        dir: &Path,
        depth: usize,
        out: &mut Vec<(usize, Entry, bool)>,
        tr: &crate::i18n::Strings,
    ) {
        let entries = match self.listing(dir) {
            Some(Ok(v)) => v.clone(),
            Some(Err(e)) => {
                out.push((
                    depth,
                    Entry::placeholder(
                        dir.join("?"),
                        crate::i18n::fill(tr.fb_unreadable, &[("err", e)]),
                        EntryKind::Unreadable,
                    ),
                    false,
                ));
                return;
            }
            None => {
                out.push((
                    depth,
                    Entry::placeholder(dir.join("…"), tr.fb_reading.into(), EntryKind::Reading),
                    false,
                ));
                return;
            }
        };
        for e in entries {
            let open = e.dir && self.open.contains(&e.path);
            out.push((depth, e.clone(), open));
            if open && depth < 24 {
                self.walk(&e.path, depth + 1, out, tr);
            }
        }
    }
}

impl App {
    pub(super) fn files_tab(&mut self, ui: &mut Ui, filter: &str) {
        let tr = self.tr();
        if self.files.poll() {
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            if kit::tool_button(ui, tr.fb_add_folder, kit::Kind::Normal, true)
                .on_hover_text(tr.fb_add_folder_tip)
                .clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                self.files.add_root(dir);
            }
            if kit::tool_button(ui, tr.fb_refresh, kit::Kind::Ghost, true)
                .on_hover_text(tr.fb_refresh_tip)
                .clicked()
            {
                self.files.refresh();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // The caption is as wide as its translation ("sort" / "сортування").
            let g = kit::caps_galley(ui.painter(), tr.fb_sort_caps, 11.0, t::faint());
            let (r, _) =
                ui.allocate_exact_size(vec2(g.size().x + 6.0, kit::TOOL_H), Sense::hover());
            ui.painter().galley(
                pos2(r.left() + 2.0, r.center().y - g.size().y / 2.0),
                g,
                t::faint(),
            );
            let mut pick = None;
            let w = ui.available_width() - 2.0;
            kit::dropdown(ui, "files-sort", self.files.sort.label(tr), w, |ui| {
                for s in Sort::ALL {
                    if ui
                        .selectable_label(self.files.sort == s, s.label(tr))
                        .clicked()
                    {
                        pick = Some(s);
                        ui.close_menu();
                    }
                }
            })
            .on_hover_text(tr.fb_sort_tip);
            if let Some(s) = pick {
                self.files.set_sort(s);
            }
        });
        ui.add_space(4.0);
        let rows = self.files.rows(filter, tr);
        if !filter.is_empty() {
            let (dirs, done) = self.files.index_state();
            let text = if done {
                tr.fb_matches
                    .fmt_with(tr.lang, rows.len(), &[("dirs", &dirs.to_string())])
            } else {
                fill(tr.fb_searching, &[("dirs", &dirs.to_string())])
            };
            ui.label(
                egui::RichText::new(text)
                    .font(t::sans(12.0))
                    .color(t::faint()),
            );
            if !done {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if rows.is_empty() && filter.is_empty() {
            ui.label(
                egui::RichText::new(tr.fb_no_folders)
                    .font(t::sans(12.0))
                    .color(t::faint()),
            );
        }
        let mut action: Option<(PathBuf, &str)> = None;
        let mut order = None;
        for (depth, e, open) in &rows {
            let is_root = *depth == 0 && e.dir && filter.is_empty();
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
            let resp = ui.interact(
                rect,
                ui.id().with(("file", &e.path)),
                Sense::click_and_drag(),
            );
            if is_root {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    resp.dnd_set_drag_payload(RootOrder(e.path.clone()));
                }
                if let Some((from, after)) = super::browser::sort_drop::<RootOrder>(ui, &resp) {
                    order = Some((from, e.path.clone(), after));
                }
            }
            let p = ui.painter();
            let current = self.files.selected.as_ref() == Some(&e.path);
            if current {
                kit::thumb(p, rect);
            } else if resp.hovered() {
                p.rect_filled(rect, kit::RADIUS - 1.0, t::raised().gamma_multiply(0.6));
            }
            let x = rect.left() + 6.0 + *depth as f32 * 12.0;
            let c = pos2(x + 5.0, rect.center().y);
            if e.dir {
                let pts = if *open {
                    vec![
                        c + vec2(-4.0, -2.0),
                        c + vec2(4.0, -2.0),
                        c + vec2(0.0, 3.0),
                    ]
                } else {
                    vec![
                        c + vec2(-2.0, -4.0),
                        c + vec2(3.0, 0.0),
                        c + vec2(-2.0, 4.0),
                    ]
                };
                p.add(egui::Shape::convex_polygon(pts, t::muted(), Stroke::NONE));
            } else if !e.is_placeholder() {
                let tag = ext(&e.path);
                let playing = self.previewing.as_ref() == Some(&e.path);
                kit::caps(
                    p,
                    c - vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    &tag,
                    9.0,
                    if playing { t::ok() } else { t::faint() },
                );
            }
            let tx = x + if e.dir { 14.0 } else { 30.0 };
            let ink = if e.dir || current {
                t::text()
            } else {
                t::muted()
            };
            let shown = kit::elide(p, &e.name, t::sans(13.0), rect.right() - tx - 4.0);
            p.text(
                pos2(tx, rect.center().y),
                Align2::LEFT_CENTER,
                shown,
                t::sans(13.0),
                ink,
            );
            if e.is_placeholder() {
                continue;
            }
            // Drag a file out (FL: drop on a channel or the rack).
            if !e.dir {
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), FileDrag(e.path.clone()));
                }
                if resp.dragged()
                    && let Some(pos) = ui.ctx().pointer_interact_pos()
                {
                    let layer =
                        egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("file-drag"));
                    let lp = ui.ctx().layer_painter(layer);
                    let label = fill(tr.fb_drag, &[("name", &e.name)]);
                    let wdt = kit::text_width(&lp, &label, t::sans(13.0)) + 20.0;
                    let r = Rect::from_min_size(pos + vec2(14.0, 8.0), vec2(wdt, 26.0));
                    kit::control(&lp, r, 6.0, 0.0, false);
                    lp.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        t::sans(13.0),
                        t::text(),
                    );
                }
            }
            if resp.clicked() {
                self.files.selected = Some(e.path.clone());
                if e.dir {
                    self.files.toggle(&e.path);
                } else if is_audio(&e.path) {
                    action = Some((e.path.clone(), "preview"));
                }
            }
            if resp.double_clicked() && !e.dir {
                action = Some((e.path.clone(), "load"));
            }
            let resp = if is_root {
                resp.on_hover_text(tr.fb_drag_reorder)
            } else {
                resp
            };
            resp.context_menu(|ui| {
                kit::menu_style(ui);
                let key = e.path.display().to_string();
                let fav = if super::browser::is_favorite(ui.ctx(), &key) {
                    tr.fb_fav_remove
                } else {
                    tr.fb_fav_add
                };
                if ui.button(fav).clicked() {
                    super::browser::toggle_favorite(ui.ctx(), key.clone());
                    ui.close_menu();
                }
                super::browser::tag_menu(ui, &key);
                if !e.dir && ui.button(tr.fb_load_selected).clicked() {
                    action = Some((e.path.clone(), "load"));
                    ui.close_menu();
                }
                if !e.dir && ui.button(tr.fb_load_new).clicked() {
                    action = Some((e.path.clone(), "new"));
                    ui.close_menu();
                }
                if is_root && ui.button(tr.fb_remove_root).clicked() {
                    action = Some((e.path.clone(), "remove"));
                    ui.close_menu();
                }
                if ui.button(tr.fb_copy_path).clicked() {
                    ui.ctx().copy_text(e.path.display().to_string());
                    ui.close_menu();
                }
            });
        }
        if let Some((from, to, after)) = order
            && super::browser::reorder(&mut self.files.roots, &from.0, &to, after)
        {
            self.files.save();
        }
        self.files_keys(ui, &rows, &mut action);
        if let Some((path, what)) = action {
            match what {
                "preview" => self.toggle_preview(path),
                "load" => self.load_file(path, false),
                "new" => self.load_file(path, true),
                "remove" => {
                    self.files.roots.retain(|r| *r != path);
                    self.files.save();
                }
                _ => {}
            }
        }
    }

    /// Arrows walk the rows, Right/Left open and close folders, Enter loads,
    /// Esc stops the preview. Only while the pointer is over the browser.
    fn files_keys(
        &mut self,
        ui: &Ui,
        rows: &[(usize, Entry, bool)],
        action: &mut Option<(PathBuf, &str)>,
    ) {
        if ui.ctx().wants_keyboard_input() || !ui.ui_contains_pointer() {
            return;
        }
        let pressed = |k: egui::Key| ui.input(|i| i.key_pressed(k));
        let at = self
            .files
            .selected
            .as_ref()
            .and_then(|s| rows.iter().position(|r| &r.1.path == s));
        let go = |i: usize| rows.get(i).map(|r| r.1.path.clone());
        if pressed(egui::Key::ArrowDown) {
            self.files.selected = go(at
                .map(|i| (i + 1).min(rows.len().saturating_sub(1)))
                .unwrap_or(0));
        }
        if pressed(egui::Key::ArrowUp) {
            self.files.selected = go(at.map(|i| i.saturating_sub(1)).unwrap_or(0));
        }
        if let Some(i) = at {
            let (_, e, open) = &rows[i];
            if e.dir
                && ((pressed(egui::Key::ArrowRight) && !open)
                    || (pressed(egui::Key::ArrowLeft) && *open))
            {
                self.files.toggle(&e.path);
            }
            if pressed(egui::Key::Enter) && !e.dir {
                *action = Some((e.path.clone(), "load"));
            }
        }
        if pressed(egui::Key::Escape) {
            self.stop_preview();
        }
    }

    /// Click plays a file; clicking the playing one stops it.
    pub(crate) fn toggle_preview(&mut self, path: PathBuf) {
        if self.previewing.as_ref() == Some(&path) && !self.preview_finished() {
            self.stop_preview();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::import::read_audio(&p).map(|(rate, frames)| {
                crate::audio::Preview {
                    frames: Arc::new(frames),
                    rate,
                }
            }));
        });
        self.preview_rx = Some((path.clone(), rx));
        self.previewing = Some(path);
    }

    /// Hear one note of channel `inst` (FL: preview notes while editing).
    /// Sound `pitch` on channel `inst` until `release_note` (FL: the note
    /// plays while the key or mouse is held). Holding another pitch moves it.
    pub(crate) fn hold_note(&mut self, inst: u64, pitch: u8, velocity: u8) {
        self.hold_note_sung(inst, pitch, velocity, 0);
    }

    /// Hold a key; on a singing channel it sings `lyric` (`vocal::lyric_hash`, 0 = the default).
    pub(crate) fn hold_note_sung(&mut self, inst: u64, pitch: u8, velocity: u8, lyric: u64) {
        if self.held.is_some_and(|(_, i, p)| i == inst && p == pitch) {
            return;
        }
        self.release_note();
        let Some(audio) = &self.audio else { return };
        self.live_id += 1;
        audio.live(crate::audio::LiveCmd::On {
            id: self.live_id,
            inst,
            pitch,
            velocity: velocity.max(1),
            ch: 0,
            lyric,
        });
        self.held = Some((self.live_id, inst, pitch));
    }

    pub(crate) fn release_note(&mut self) {
        if let (Some((id, _, _)), Some(audio)) = (self.held.take(), &self.audio) {
            audio.live(crate::audio::LiveCmd::Off { id });
        }
    }

    fn preview_finished(&self) -> bool {
        self.audio
            .as_ref()
            .is_none_or(|a| a.preview_done.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub(crate) fn stop_preview(&mut self) {
        if let Some(a) = &self.audio {
            a.preview.store(None);
        }
        self.previewing = None;
        self.preview_rx = None;
    }

    /// Hand a decoded preview to the audio thread.
    pub(super) fn poll_preview(&mut self) {
        let tr = self.tr();
        let Some((path, rx)) = &self.preview_rx else {
            return;
        };
        let Ok(result) = rx.try_recv() else { return };
        let path = path.clone();
        self.preview_rx = None;
        match result {
            Ok(clip) => {
                if let Some(a) = &self.audio {
                    a.preview_done
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    a.preview.store(Some(Arc::new(clip)));
                }
                if !path.as_os_str().is_empty() {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    self.say(Tone::Quiet, fill(tr.fb_previewing, &[("name", &name)]));
                }
            }
            Err(e) => {
                self.previewing = None;
                self.say(Tone::Bad, e);
            }
        }
    }

    /// Load a browser file: audio or .dwp into a sampler, .kfxl opens.
    pub(crate) fn load_file(&mut self, path: PathBuf, new_channel: bool) {
        let e = ext(&path);
        if e == "kfxl" || e == "flp" {
            self.request(super::Pending::Open(Some(path)));
            return;
        }
        if e == "mid" || e == "midi" {
            self.import_midi(&path);
            return;
        }
        let target = if new_channel {
            None
        } else {
            self.rack_sel
                .first()
                .copied()
                .filter(|id| self.is_sampler(*id))
                .or_else(|| match &self.focus {
                    Some(super::Tab::Instrument { id })
                    | Some(super::Tab::Roll { instrument: id })
                        if self.is_sampler(*id) =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
        };
        let id = match target {
            Some(id) => id,
            None => {
                // New channel + its sound: one undo step (the channel's).
                let id = self.add_sampler();
                self.join_preset = Some(id);
                id
            }
        };
        self.load_preset(id, path);
    }

    /// A browser file dropped on channel `id` (drum voices become samplers).
    pub(crate) fn drop_file_on_channel(&mut self, id: u64, path: PathBuf) {
        if matches!(ext(&path).as_str(), "kfxl" | "flp") {
            self.request(super::Pending::Open(Some(path)));
            return;
        }
        self.load_preset(id, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_shows_folders_first_and_only_usable_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Kicks")).unwrap();
        for f in ["b.wav", "a.FLAC", "notes.txt", ".hidden.wav", "song.kfxl"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let names: Vec<String> = list(dir.path(), Sort::Name)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["Kicks", "a.FLAC", "b.wav", "song.kfxl"]);
        assert!(list(&dir.path().join("missing"), Sort::Name).is_err());
    }

    #[test]
    fn sort_orders_keep_folders_on_top() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Loops")).unwrap();
        std::fs::write(dir.path().join("big.wav"), vec![0u8; 3000]).unwrap();
        std::fs::write(dir.path().join("a.ogg"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.path().join("mid.flac"), vec![0u8; 500]).unwrap();
        let names = |s: Sort| {
            list(dir.path(), s)
                .unwrap()
                .into_iter()
                .map(|e| e.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Sort::Name), ["Loops", "a.ogg", "big.wav", "mid.flac"]);
        assert_eq!(names(Sort::Type), ["Loops", "mid.flac", "a.ogg", "big.wav"]);
        assert_eq!(
            names(Sort::Largest),
            ["Loops", "big.wav", "mid.flac", "a.ogg"]
        );
    }

    #[test]
    fn search_finds_files_in_folders_never_opened() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("Packs").join("Drums").join("Kicks");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Kick Punchy.wav"), b"x").unwrap();
        std::fs::write(dir.path().join("kick top.wav"), b"x").unwrap();
        std::fs::write(dir.path().join("snare.wav"), b"x").unwrap();
        let mut files = Files::new(false);
        files.roots = vec![dir.path().to_path_buf()];
        files.open.clear();
        assert!(
            files.rows("kick", &crate::i18n::EN).is_empty() || !files.index_state().1,
            "the first search starts a scan"
        );
        files.wait_index();
        let hits: Vec<String> = files
            .rows("kick", &crate::i18n::EN)
            .into_iter()
            .map(|r| r.1.name)
            .collect();
        assert_eq!(hits, ["Kick Punchy.wav", "kick top.wav"]);
        assert_eq!(files.index_state(), (4, true), "every folder read once");
    }
}
