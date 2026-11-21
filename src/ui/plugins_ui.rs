//! Plugins in the project: keep the loaded instances in step with the song,
//! hand them to the audio thread, open their editors, collect their state on
//! save, and the Plugins page of Settings (scan results, folders, rescan).

use super::{
    App, Tone, kit,
    settings_ui::{card, kit_switch, row},
};
use crate::i18n::fill;
use crate::plugins::{Reply, Req, Scanned, Status};
use crate::settings::Settings;
use crate::theme as t;
use eframe::egui::{self, RichText};
use keyframexl::plugin::{Instance, Shared};
use keyframexl::sequence::{InstrumentKind, PluginRef};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

/// One plugin of the project as the UI tracks it.
pub struct Live {
    pub path: String,
    pub proc: Option<Shared>,
    pub name: String,
    pub gui: bool,
    pub editor: bool,
    pub error: Option<String>,
    /// Open the editor as soon as it has loaded.
    pub show_when_loaded: bool,
    /// Output pairs past the main one (a multi-out instrument).
    pub outs: usize,
}

/// Scan progress and results.
pub enum ScanMsg {
    Progress(usize, usize, String),
    Done(Vec<Scanned>),
}

/// A page of the plugin window (FL wrapper: the plugin, its parameters).
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum WrapPage {
    #[default]
    Plugin,
    Parameters,
    /// FL: the wrapper's settings (processing, routing, state).
    Settings,
}

#[derive(Default)]
pub struct PluginState {
    pub host: Option<crate::plugins::HostHandle>,
    pub native_fl: Option<crate::fl_native::Session>,
    pub live: HashMap<u64, Live>,
    pub list: Vec<Scanned>,
    pub scan_rx: Option<mpsc::Receiver<ScanMsg>>,
    pub scan_note: Option<String>,
    /// Our main window on X11: plugin editors go inside it. None on Wayland.
    pub parent: Option<u32>,
    /// Editor size in pixels as the plugin reports it.
    pub editor_size: HashMap<u64, (i32, i32)>,
    /// Where each embedded editor was put last: (rect, visible).
    pub placed: HashMap<u64, ([i32; 4], [i32; 4], bool)>,
    /// Editors drawn this frame (the rest are hidden at the frame's end).
    pub seen: Vec<u64>,
    /// Page per plugin window, parameter lists (with when they were read), filter text.
    pub page: HashMap<u64, WrapPage>,
    pub params: HashMap<u64, (Vec<crate::plugins::ParamRow>, f64)>,
    pub param_filter: String,
    /// The dock tab drawing a wrapper now (it grows to fit the editor).
    pub tab: Option<super::Tab>,
    /// Editor size each window already grew for.
    pub grown: HashMap<u64, (i32, i32)>,
    /// Our preset files per (kind, plugin path), re-read every few seconds: (time, files).
    pub preset_list: HashMap<(&'static str, String), (f64, Vec<std::path::PathBuf>)>,
    /// The rect a window was last grown to: egui_dock's `rect()` keeps the
    /// window's first rect, so growing from it again would shrink the window.
    pub grown_rect: HashMap<String, egui::Rect>,
    /// The preset last stepped to per channel or slot (wrapper ◀ ▶).
    pub preset_at: HashMap<String, std::path::PathBuf>,
    /// Single-plugin reprobe in flight: (path, receiver). The probe runs in a
    /// child process on a worker thread; the UI stays responsive.
    pub rescan_one: Option<(PathBuf, mpsc::Receiver<Scanned>)>,
    /// Parameters page edits: the last one (a drag within 0.6 s of it stays one
    /// undo step), when to read the plugin states back after it, and whether
    /// the song's copy of a state is behind the plugin because of them.
    pub param_last: Option<std::time::Instant>,
    pub refresh_at: Option<std::time::Instant>,
    pub param_dirty: bool,
    /// Parameter values seen with each stored state, by (plugin, state hash):
    /// undo sends them too, as some plugins restore a state without updating
    /// the values their controller reports.
    pub known: HashMap<(u64, u64), Vec<(u32, f64)>>,
}

fn state_hash(state: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state.hash(&mut h);
    h.finish()
}

impl App {
    /// Start the host thread and a background scan (cached results show at once).
    pub(crate) fn start_plugins(&mut self, ctx: Option<&egui::Context>) {
        self.plugins.list = crate::plugins::load_cache();
        self.plugins.host = Some(self.new_plugin_host(ctx.cloned()));
        self.rescan_plugins(false);
    }

    /// In-process, or each plugin in its own process (Settings → Plugins).
    fn new_plugin_host(&self, ctx: Option<egui::Context>) -> crate::plugins::HostHandle {
        if self.settings.plugins.bridged && std::env::var_os("KFXL_NO_BRIDGE").is_none() {
            crate::bridge::start_bridged_host(self.rate(), ctx)
        } else {
            crate::plugins::start_host(self.rate(), ctx)
        }
    }

    /// Switch between in-process and bridged plugins: states are kept, every
    /// plugin unloads, and the new host loads them again.
    pub(crate) fn restart_plugin_host(&mut self, ctx: &egui::Context) {
        self.collect_plugin_states();
        if let Some(a) = &self.audio {
            for id in self.plugins.live.keys() {
                a.remove_plugin(*id);
            }
        }
        if let Some(h) = self.plugins.host.take() {
            h.send(Req::Quit);
            let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
            while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
                match h.rx.recv_timeout(left) {
                    Ok(Reply::Bye) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        }
        self.plugins.live.clear();
        self.plugins.placed.clear();
        self.plugins.editor_size.clear();
        self.plugins.params.clear();
        self.plugins.host = Some(self.new_plugin_host(Some(ctx.clone())));
    }

    /// A crashed or failed plugin: load it again from its last saved state.
    pub(crate) fn reload_plugin(&mut self, id: u64) {
        let tr = self.tr();
        let name = self
            .plugins
            .live
            .get(&id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        self.say(
            Tone::Wait,
            fill(tr.plg_reloading, &[(tr.plg_col_name, &name)]),
        );
        self.unload_plugin(id);
        // plugins_frame loads it again next frame, as the song still has it.
    }

    pub(crate) fn rescan_plugins(&mut self, force: bool) {
        if self.plugins.scan_rx.is_some() {
            return;
        }
        let extra = self.settings.plugins.extra_dirs.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let list = crate::plugins::scan(&extra, force, move |d, n, name| {
                let _ = tx2.send(ScanMsg::Progress(d, n, name.to_string()));
            });
            let _ = tx.send(ScanMsg::Done(list));
        });
        self.plugins.scan_rx = Some(rx);
    }

    /// Instruments and effects the scan found usable.
    pub(crate) fn plugin_choices(&self, instruments: bool) -> Vec<Scanned> {
        self.plugins
            .list
            .iter()
            .filter(|s| {
                s.instrument == instruments
                    && s.usable()
                    && !self
                        .settings
                        .plugins
                        .blocked
                        .contains(&s.path.display().to_string())
            })
            .cloned()
            .collect()
    }

    /// Once per frame: scan news, host replies, load/unload to match the song.
    pub(crate) fn plugins_frame(&mut self) {
        let tr = self.tr();
        if let Some(rx) = &self.plugins.scan_rx {
            for msg in rx.try_iter().collect::<Vec<_>>() {
                match msg {
                    ScanMsg::Progress(d, n, name) => {
                        self.plugins.scan_note = Some(fill(
                            tr.plg_checking,
                            &[
                                ("d", &d.to_string()),
                                ("n", &n.to_string()),
                                (tr.plg_col_name, &name),
                            ],
                        ))
                    }
                    ScanMsg::Done(list) => {
                        let ok = list.iter().filter(|s| s.usable()).count();
                        self.plugins.scan_note = Some(fill(
                            tr.plg_scan_done,
                            &[("ok", &ok.to_string()), ("n", &list.len().to_string())],
                        ));
                        self.plugins.list = list;
                        self.plugins.scan_rx = None;
                    }
                }
                if self.plugins.scan_rx.is_none() {
                    break;
                }
            }
        }
        let Some(host) = &self.plugins.host else {
            return;
        };
        let replies: Vec<Reply> = host.rx.try_iter().collect();
        for r in replies {
            self.plugin_reply(r);
        }
        if self
            .plugins
            .refresh_at
            .is_some_and(|t| std::time::Instant::now() >= t)
        {
            self.plugins.refresh_at = None;
            if let Some(h) = &self.plugins.host {
                h.send(Req::SaveStates);
            }
        }
        // Load what the song has and the host does not; unload what it lost.
        let wanted: Vec<PluginRef> = self.song.plugin_refs().into_iter().cloned().collect();
        self.sync_native_fl(&wanted);
        let effects: Vec<u64> = self
            .song
            .mixer
            .iter()
            .flat_map(|t| t.fx.iter().filter_map(|f| f.plugin.as_ref().map(|p| p.id)))
            .collect();
        for r in &wanted {
            if crate::fl_native::slot(&r.path).is_some() {
                continue;
            }
            let stale = self
                .plugins
                .live
                .get(&r.id)
                .is_some_and(|l| l.path != r.path);
            if stale {
                self.unload_plugin(r.id);
            }
            if let std::collections::hash_map::Entry::Vacant(e) = self.plugins.live.entry(r.id) {
                e.insert(Live {
                    path: r.path.clone(),
                    proc: None,
                    name: r.name.clone(),
                    gui: false,
                    editor: false,
                    error: None,
                    show_when_loaded: false,
                    outs: 0,
                });
                if crate::fl_link::is_link(std::path::Path::new(&r.path))
                    && let Err(e) = crate::fl_link::ensure_relay()
                {
                    eprintln!("FL link: {e}");
                }
                if let Some(h) = &self.plugins.host {
                    h.send(Req::Load {
                        id: r.id,
                        path: PathBuf::from(&r.path),
                        state: r.state_bytes(),
                        effect: effects.contains(&r.id),
                    });
                }
            }
        }
        let gone: Vec<u64> = self
            .plugins
            .live
            .keys()
            .copied()
            .filter(|id| !wanted.iter().any(|r| r.id == *id))
            .collect();
        for id in gone {
            self.unload_plugin(id);
        }
    }

    fn sync_native_fl(&mut self, wanted: &[PluginRef]) {
        let tr = self.tr();
        use crate::fl_native::{Message, Session, slot};
        let has_channels = wanted.iter().any(|r| slot(&r.path).is_some());
        let source = self.song.native_fl.clone().filter(|_| has_channels);
        let rate = self.rate();
        let same = match (&self.plugins.native_fl, &source) {
            (Some(session), Some(source)) => {
                std::sync::Arc::ptr_eq(&session.source, source) && session.rate == rate
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            let old = self
                .plugins
                .live
                .iter()
                .filter(|(_, l)| slot(&l.path).is_some())
                .map(|(&id, _)| id)
                .collect::<Vec<_>>();
            for id in old {
                self.unload_plugin(id);
            }
            self.plugins.native_fl = source.map(|source| Session::start(source, rate));
            if self.plugins.native_fl.is_some() {
                self.say(Tone::Wait, tr.plg_opening_fl);
            }
        }
        let mut notice = None;
        if let Some(session) = &mut self.plugins.native_fl {
            for msg in session.rx.try_iter() {
                match msg {
                    Message::Ready(outputs) => {
                        notice = Some((
                            Tone::Done,
                            fill(tr.plg_fl_connected, &[("n", &outputs.len().to_string())]),
                        ));
                        session.outputs = outputs;
                    }
                    Message::Error(error) => {
                        notice = Some((Tone::Bad, error.clone()));
                        session.error = Some(error);
                        session.outputs.clear();
                    }
                }
            }
        }
        if let Some((tone, message)) = notice {
            self.say(tone, message);
        }
        let mut used = [false; 16];
        for r in wanted {
            let Some(index) = slot(&r.path) else { continue };
            let duplicate = used
                .get_mut(index)
                .is_some_and(|seen| std::mem::replace(seen, true));
            let (proc, error) = if duplicate {
                (None, Some(tr.plg_fl_output_taken.into()))
            } else if let Some(session) = &self.plugins.native_fl {
                (
                    session.outputs.get(index).cloned(),
                    session.error.clone().or_else(|| {
                        (index >= session.source.channels.len())
                            .then(|| tr.plg_fl_output_invalid.into())
                    }),
                )
            } else {
                (None, Some(tr.plg_fl_no_state.into()))
            };
            let old = self.plugins.live.get(&r.id);
            let unchanged = old.is_some_and(|l| {
                l.path == r.path
                    && l.error == error
                    && match (&l.proc, &proc) {
                        (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            });
            if unchanged {
                continue;
            }
            self.unload_plugin(r.id);
            self.plugins.live.insert(
                r.id,
                Live {
                    path: r.path.clone(),
                    proc: None,
                    name: r.name.clone(),
                    gui: false,
                    editor: false,
                    error,
                    show_when_loaded: false,
                    outs: 0,
                },
            );
            if let Some(proc) = proc {
                self.plugin_reply(Reply::Loaded {
                    id: r.id,
                    proc,
                    name: r.name.clone(),
                    gui: false,
                    outs: 0,
                });
            }
        }
    }

    fn plugin_reply(&mut self, r: Reply) {
        let tr = self.tr();
        if std::env::var_os("KFXL_PLUGIN_TRACE").is_some() {
            match &r {
                Reply::Loaded { id, name, gui, .. } => {
                    eprintln!("plugin {id} loaded: {name} (editor: {gui})")
                }
                Reply::Failed { id, error } => eprintln!("plugin {id} failed: {error}"),
                Reply::Editor { id, open } => eprintln!("plugin {id} editor open: {open}"),
                Reply::EditorSize { id, w, h } => eprintln!("plugin {id} editor size: {w}x{h}"),
                Reply::Params { id, rows } => eprintln!(
                    "plugin {id} parameters: {} first {:?}",
                    rows.len(),
                    rows.iter()
                        .take(7)
                        .map(|r| (r.name.as_str(), r.value, r.text.as_str()))
                        .collect::<Vec<_>>()
                ),
                Reply::States(s) => eprintln!("plugin states: {}", s.len()),
                Reply::Bye => eprintln!("plugin host ended"),
            }
        }
        match r {
            Reply::Loaded {
                id,
                proc,
                name,
                gui,
                outs,
            } => {
                let Some(l) = self.plugins.live.get_mut(&id) else {
                    return;
                };
                l.proc = Some(proc.clone());
                l.name = name.clone();
                l.gui = gui;
                l.outs = outs;
                l.error = None;
                let show = std::mem::take(&mut l.show_when_loaded);
                if let Some(a) = &self.audio
                    && !a.add_plugin(Instance::new(id, proc))
                {
                    self.say(
                        Tone::Bad,
                        fill(tr.plg_audio_busy, &[(tr.plg_col_name, &name)]),
                    );
                }
                if show {
                    self.open_plugin(id);
                }
            }
            Reply::Failed { id, error } => {
                eprintln!("plugin {id} failed: {error}");
                let name = self
                    .plugins
                    .live
                    .get(&id)
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                if let Some(l) = self.plugins.live.get_mut(&id) {
                    l.error = Some(error.clone());
                }
                self.say(Tone::Bad, format!("{name}: {error}"));
            }
            Reply::Editor { id, open } => {
                if let Some(l) = self.plugins.live.get_mut(&id) {
                    l.editor = open;
                }
            }
            Reply::States(states) => self.store_states(states),
            Reply::EditorSize { id, w, h } => {
                self.plugins.editor_size.insert(id, (w, h));
            }
            Reply::Params { id, rows } => {
                let mut moved = Vec::new();
                if self.plugins.live.get(&id).is_some_and(|l| l.editor)
                    && !self.plugins.param_dirty
                    && let Some((prev, _)) = self.plugins.params.get(&id)
                {
                    for r in &rows {
                        if !r.automatable {
                            continue;
                        }
                        if let Some(old) = prev.iter().find(|p| p.id == r.id)
                            && (r.value - old.value).abs() > 1e-4
                        {
                            // Skip values that match what our own automation clip just sent.
                            let from_auto = self
                                .auto_sent
                                .get(&(id, r.id))
                                .is_some_and(|&v| (v as f64 - r.value).abs() < 1e-3);
                            if !from_auto {
                                moved.push(r.id);
                            }
                        }
                    }
                }
                let now = self.plugins.params.get(&id).map(|p| p.1).unwrap_or(0.0);
                self.plugins.params.insert(id, (rows, now));
                // Ignore bulk preset jumps (>4 parameters changing at once).
                if !moved.is_empty() && moved.len() <= 4 {
                    for param in moved {
                        self.on_target_tweaked(keyframexl::automation::Target::PluginParam {
                            plugin: id,
                            param,
                        });
                    }
                }
            }
            Reply::Bye => self.plugins.host = None,
        }
    }

    /// New plugin states into the song. A state that moved because of our own
    /// parameter edits only updates the song: the undo step before the edit
    /// keeps the old one. A state that moved in the plugin's own window has no
    /// undo step, so the snapshots that held the same old state take the new
    /// one too, and undoing an unrelated edit does not revert it.
    pub(crate) fn store_states(&mut self, states: Vec<(u64, Vec<u8>)>) {
        let spread = !self.plugins.param_dirty;
        for (id, bytes) in states {
            let Some(r) = self.song.plugin_ref_mut(id) else {
                continue;
            };
            let old = std::mem::take(&mut r.state);
            r.set_state(&bytes);
            let new = r.state.clone();
            if let Some((rows, _)) = self.plugins.params.get(&id) {
                if self.plugins.known.len() > 512 {
                    self.plugins.known.clear();
                }
                self.plugins.known.insert(
                    (id, state_hash(&new)),
                    rows.iter().map(|r| (r.id, r.value)).collect(),
                );
            }
            if !spread || old == new {
                continue;
            }
            for snap in self
                .history
                .iter_mut()
                .rev()
                .chain(self.future.iter_mut().rev())
            {
                match snap.plugin_ref_mut(id) {
                    Some(s) if s.state == old => s.state = new.clone(),
                    _ => break,
                }
            }
        }
        if self.plugins.refresh_at.is_none() {
            self.plugins.param_dirty = false;
        }
    }

    /// Before undo or redo: a parameter edit whose state was not read back yet
    /// is read now, so the step being left keeps it.
    pub(crate) fn settle_plugin_states(&mut self) {
        if self.plugins.param_dirty {
            self.plugins.refresh_at = None;
            self.collect_plugin_states();
            self.plugins.param_dirty = false;
        }
    }

    /// After undo or redo: plugins whose state differs between the song left
    /// and the song now get the restored state (an empty one reloads defaults).
    pub(crate) fn restore_plugin_states(&mut self, before: &keyframexl::sequence::Song) {
        let changed: Vec<(u64, Vec<u8>, u64)> = self
            .song
            .plugin_refs()
            .into_iter()
            .filter(|r| self.plugins.live.contains_key(&r.id))
            .filter(|r| {
                before
                    .plugin_refs()
                    .iter()
                    .any(|b| b.id == r.id && b.path == r.path && b.state != r.state)
            })
            .map(|r| (r.id, r.state_bytes(), state_hash(&r.state)))
            .collect();
        for (id, state, hash) in changed {
            if state.is_empty() {
                self.unload_plugin(id);
                continue;
            }
            let mut send = vec![Req::SetState { id, state }];
            // The values that went with the state, where they differ from now.
            let values = self
                .plugins
                .known
                .get(&(id, hash))
                .cloned()
                .unwrap_or_default();
            if let Some((rows, read)) = self.plugins.params.get_mut(&id) {
                for (param, value) in values {
                    if let Some(r) = rows.iter_mut().find(|r| r.id == param)
                        && (r.value - value).abs() > 1e-6
                    {
                        send.push(Req::SetParam { id, param, value });
                        r.value = value;
                    }
                }
                *read = f64::NEG_INFINITY;
            }
            let sent = send.len() - 1;
            if let Some(h) = &self.plugins.host {
                for r in send {
                    h.send(r);
                }
            }
            if std::env::var_os("KFXL_PLUGIN_TRACE").is_some() {
                eprintln!("plugin {id} state restored by undo/redo, {sent} parameters sent");
            }
        }
    }

    fn unload_plugin(&mut self, id: u64) {
        self.plugins.placed.remove(&id);
        self.plugins.editor_size.remove(&id);
        if self.plugins.live.remove(&id).is_some() {
            if let Some(a) = &self.audio {
                a.remove_plugin(id);
            }
            if let Some(h) = &self.plugins.host {
                h.send(Req::Unload(id));
            }
        }
    }

    /// On exit: audio stops first (it holds the instances), then the host
    /// thread unloads every plugin. Without this yabridge aborts the process
    /// when its library is torn down under live instances.
    pub(crate) fn shutdown_plugins(&mut self) {
        let Some(h) = self.plugins.host.take() else {
            return;
        };
        let trace = std::env::var_os("KFXL_PLUGIN_TRACE").is_some();
        if trace {
            eprintln!("shutdown: stopping audio");
        }
        self.audio = None;
        if let Some(native) = self.plugins.native_fl.take() {
            native.shutdown();
        }
        // Our last references to the instances, so the host thread drops the final ones.
        self.plugins.live.clear();
        if trace {
            eprintln!("shutdown: audio stopped, unloading plugins");
        }
        h.send(Req::Quit);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
            match h.rx.recv_timeout(left) {
                Ok(Reply::Bye) | Err(_) => break,
                Ok(_) => {}
            }
        }
        if trace {
            eprintln!("shutdown: done");
        }
    }

    /// Before a project is replaced: the next one loads its own instances.
    pub(crate) fn unload_all_plugins(&mut self) {
        self.plugins.native_fl = None;
        let ids: Vec<u64> = self.plugins.live.keys().copied().collect();
        for id in ids {
            self.unload_plugin(id);
        }
    }

    /// Ask every plugin for its state and put it into the song (before saving).
    /// Waits up to ~0.8 s; plugins that do not answer keep their last state.
    /// (A full 3 s block here used to stall every save on one slow plugin.)
    pub(crate) fn collect_plugin_states(&mut self) {
        if self.plugins.live.is_empty() {
            return;
        }
        let Some(h) = &self.plugins.host else { return };
        h.send(Req::SaveStates);
        let until = std::time::Instant::now() + std::time::Duration::from_millis(800);
        let mut other = Vec::new();
        let mut states = None;
        while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
            let Some(h) = &self.plugins.host else { break };
            match h.rx.recv_timeout(left) {
                Ok(Reply::States(s)) => {
                    states = Some(s);
                    break;
                }
                Ok(r) => other.push(r),
                Err(_) => break,
            }
        }
        for r in other {
            self.plugin_reply(r);
        }
        if let Some(s) = states {
            self.store_states(s);
        }
    }

    /// After the audio output restarted: same rate re-adds the instances, a new
    /// rate reloads every plugin at it.
    pub(crate) fn reopen_plugins(&mut self) {
        let rate = self.rate();
        if let Some(h) = &self.plugins.host {
            h.send(Req::Rate(rate));
        }
        if let Some(a) = &self.audio {
            for (id, l) in &self.plugins.live {
                if let Some(p) = &l.proc {
                    a.add_plugin(Instance::new(*id, p.clone()));
                }
            }
        }
    }

    /// FL: open the plugin's window (channel window / effect window). With an
    /// X11 main window the editor lives inside it; otherwise it offers the
    /// editor's own window.
    pub(crate) fn open_plugin(&mut self, id: u64) {
        if self
            .plugins
            .live
            .get(&id)
            .is_some_and(|l| l.proc.is_none() && l.error.is_none())
            && let Some(l) = self.plugins.live.get_mut(&id)
        {
            l.show_when_loaded = true;
        }
        if let Some(ch) = self
            .song
            .instruments
            .iter()
            .find(|i| i.plugin().is_some_and(|p| p.id == id))
            .map(|i| i.id)
        {
            self.win(super::Cmd::Show(super::Tab::Instrument { id: ch }));
            return;
        }
        let slot = self.song.mixer.iter().enumerate().find_map(|(t, m)| {
            m.fx.iter()
                .position(|f| f.plugin.as_ref().is_some_and(|p| p.id == id))
                .map(|k| (t, k))
        });
        // The wrapper window either way; without an X11 parent it offers the
        // editor's own window instead of embedding it.
        if let Some((t, k)) = slot
            && !self.fx_windows.contains(&(t, k))
        {
            self.fx_windows.push((t, k));
        }
    }

    /// The plugin window's body (FL wrapper): page tabs, then the editor or
    /// the parameter list.
    pub(crate) fn plugin_wrapper(&mut self, ui: &mut egui::Ui, plugin: &PluginRef) {
        let tr = self.tr();
        let id = plugin.id;
        let page = self.plugins.page.get(&id).copied().unwrap_or_default();
        let (status, color) = self.plugin_status(id);
        let mut new_page = page;
        ui.horizontal(|ui| {
            let pages = [WrapPage::Plugin, WrapPage::Parameters, WrapPage::Settings];
            let (pick, _) = kit::chips(
                ui,
                &format!("wrap-page-{id}"),
                &[tr.plg_page_plugin, tr.plg_page_params, tr.plg_page_settings],
                pages.iter().position(|p| *p == page).unwrap_or(0),
                None,
            );
            if let Some(i) = pick {
                new_page = pages[i];
            }
            let fmt = plugin.format.to_uppercase();
            let vendor = if plugin.vendor.is_empty() {
                fmt
            } else {
                format!("{} · {fmt}", plugin.vendor)
            };
            ui.label(RichText::new(vendor).font(t::sans(12.0)).color(t::faint()));
            // A failure reads short here; the whole story is in the tooltip.
            let failed = self.plugins.live.get(&id).and_then(|l| l.error.clone());
            match failed {
                Some(err) => {
                    let short = if err.contains("crashed") || err.contains("stopped responding") {
                        tr.plg_crashed
                    } else {
                        tr.plg_failed
                    };
                    ui.label(RichText::new(short).font(t::sans(12.0)).color(t::bad()))
                        .on_hover_text(&err);
                    if kit::tip(
                        kit::tool_button(ui, tr.plg_reload, kit::Kind::Normal, true),
                        fill(tr.plg_reload_err_tip, &[("err", &err)]),
                    )
                    .clicked()
                    {
                        self.reload_plugin(id);
                    }
                }
                None => {
                    ui.label(RichText::new(status).font(t::sans(12.0)).color(color));
                }
            }
            let channel = self
                .song
                .instruments
                .iter()
                .find(|i| i.plugin().is_some_and(|p| p.id == id))
                .map(|i| i.id);
            self.preset_stepper(ui, plugin, channel);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("☰").font(t::sans(14.0)), |ui| {
                    kit::menu_style(ui);
                    let slot = self.song.mixer.iter().enumerate().find_map(|(t, m)| {
                        m.fx.iter()
                            .position(|f| f.plugin.as_ref().is_some_and(|p| p.id == id))
                            .map(|k| (t, k))
                    });
                    if let Some(ch) = channel {
                        if ui.button(tr.inst_save_ch_preset).clicked() {
                            ui.close_menu();
                            self.save_channel_preset(ch);
                        }
                        if ui.button(tr.inst_load_ch_preset).clicked() {
                            ui.close_menu();
                            self.load_preset_dialog_for(super::presets_ui::Target::Channel(ch));
                        }
                        ui.separator();
                    }
                    if let Some((t, k)) = slot {
                        if ui.button(tr.plg_save_fx_preset).clicked() {
                            ui.close_menu();
                            self.save_fx_preset(t, k);
                        }
                        if ui.button(tr.plg_load_fx_preset).clicked() {
                            ui.close_menu();
                            self.load_preset_dialog_for(super::presets_ui::Target::Fx(t, k));
                        }
                        ui.separator();
                    }
                    if let Some(ch) = channel {
                        if ui.button(tr.plg_piano_roll_f7).clicked() {
                            self.open_roll(ch);
                            ui.close_menu();
                        }
                        if ui.button(tr.inst_delete_channel).clicked() {
                            self.delete_channel(ch);
                            ui.close_menu();
                        }
                        ui.separator();
                    }
                    let separate = self
                        .plugins
                        .live
                        .get(&id)
                        .is_some_and(|l| l.editor && !self.plugins.placed.contains_key(&id));
                    if ui
                        .button(if separate {
                            tr.plg_close_separate
                        } else {
                            tr.plg_editor_separate
                        })
                        .clicked()
                    {
                        if let Some(h) = &self.plugins.host {
                            h.send(Req::HideEditor(id));
                            if !separate {
                                h.send(Req::ShowEditor(id));
                            }
                        }
                        self.plugins.placed.remove(&id);
                        ui.close_menu();
                    }
                    if ui.button(tr.plg_page_params).clicked() {
                        new_page = WrapPage::Parameters;
                        ui.close_menu();
                    }
                });
            });
        });
        if new_page != page {
            self.plugins.page.insert(id, new_page);
        }
        ui.add_space(4.0);
        match new_page {
            WrapPage::Plugin => self.embedded_editor(ui, id),
            WrapPage::Parameters => self.parameters_page(ui, id),
            WrapPage::Settings => self.wrapper_settings(ui, plugin),
        }
    }

    /// A multi-out instrument's outputs (FL: the wrapper's output routing):
    /// auto-map every pair to its own insert, or route each pair by hand.
    fn output_rows(&mut self, ui: &mut egui::Ui, ch: u64, id: u64, extra: usize) {
        let tr = self.tr();
        let outs = self
            .song
            .instrument(ch)
            .and_then(|i| i.plugin())
            .map(|p| p.outs.clone())
            .unwrap_or_default();
        let routed = outs.iter().filter(|&&o| o > 0).count();
        let hint = tr.plg_outputs_hint;
        row(ui, tr.plg_outputs, Some(hint), |ui| {
            ui.label(
                egui::RichText::new(fill(
                    tr.plg_outputs_summary,
                    &[
                        ("n", &(extra + 1).to_string()),
                        ("routed", &routed.to_string()),
                    ],
                ))
                .font(t::mono(12.0))
                .color(t::muted()),
            );
            if kit::tool_button(ui, tr.plg_map_to_inserts, kit::Kind::Ghost, true)
                .on_hover_text(tr.plg_map_to_inserts_tip)
                .clicked()
            {
                self.checkpoint();
                let n = self.song.map_plugin_outputs(ch, extra);
                self.publish();
                self.say(Tone::Done, fill(tr.plg_mapped, &[("n", &n.to_string())]));
            }
            if routed > 0
                && kit::tool_button(ui, tr.plg_unroute, kit::Kind::Ghost, true)
                    .on_hover_text(tr.plg_unroute_tip)
                    .clicked()
            {
                self.checkpoint();
                if let Some(keyframexl::sequence::InstrumentKind::Plugin { plugin }) = self
                    .song
                    .instruments
                    .iter_mut()
                    .find(|i| i.id == ch)
                    .map(|i| &mut i.kind)
                {
                    plugin.outs.clear();
                }
                self.publish();
            }
        });
        let names: Vec<String> = (0..self.song.mixer.len())
            .map(|m| {
                if m == 0 {
                    tr.plg_with_1_2.to_string()
                } else {
                    format!("{m:02}  {}", self.mixer_name(m))
                }
            })
            .collect();
        egui::CollapsingHeader::new(
            egui::RichText::new(tr.plg_route_each_pair)
                .font(t::sans(12.0))
                .color(t::muted()),
        )
        .id_salt(("wrap-outs", id))
        .show(ui, |ui| {
            egui::Grid::new(("wrap-outs-grid", id))
                .num_columns(2)
                .spacing(egui::vec2(10.0, 4.0))
                .show(ui, |ui| {
                    for k in 0..extra {
                        let now = outs.get(k).copied().unwrap_or(0) as usize;
                        ui.label(
                            egui::RichText::new(format!("{}-{}", 2 * k + 3, 2 * k + 4))
                                .font(t::mono(12.0))
                                .color(t::text()),
                        );
                        let mut pick = None;
                        kit::dropdown(
                            ui,
                            &format!("wrap-out-{id}-{k}"),
                            names.get(now).map(String::as_str).unwrap_or("?"),
                            200.0,
                            |ui| {
                                for (m, label) in names.iter().enumerate() {
                                    if ui.selectable_label(now == m, label).clicked() {
                                        pick = Some(m as u16);
                                    }
                                }
                            },
                        );
                        ui.end_row();
                        if let Some(m) = pick
                            && m as usize != now
                        {
                            self.checkpoint();
                            if let Some(keyframexl::sequence::InstrumentKind::Plugin { plugin }) =
                                self.song
                                    .instruments
                                    .iter_mut()
                                    .find(|i| i.id == ch)
                                    .map(|i| &mut i.kind)
                            {
                                if plugin.outs.len() <= k {
                                    plugin.outs.resize(k + 1, 0);
                                }
                                plugin.outs[k] = m;
                                while plugin.outs.last() == Some(&0) {
                                    plugin.outs.pop();
                                }
                            }
                            self.publish();
                        }
                    }
                });
        });
    }

    /// The wrapper's Settings page (FL: wrapper settings): where the plugin
    /// plays, how it runs, its saved state.
    fn wrapper_settings(&mut self, ui: &mut egui::Ui, plugin: &PluginRef) {
        let tr = self.tr();
        let id = plugin.id;
        let channel = self
            .song
            .instruments
            .iter()
            .find(|i| i.plugin().is_some_and(|p| p.id == id))
            .map(|i| i.id);
        let slot = self.song.mixer.iter().enumerate().find_map(|(t, m)| {
            m.fx.iter()
                .position(|f| f.plugin.as_ref().is_some_and(|p| p.id == id))
                .map(|k| (t, k))
        });
        egui::ScrollArea::vertical()
            .id_salt(("wrap-settings", id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(ch) = channel {
                    card(ui, tr.plg_card_channel, |ui| {
                        row(
                            ui,
                            tr.plg_mixer_insert,
                            Some(tr.plg_mixer_insert_hint),
                            |ui| {
                                let now = self.song.instrument(ch).map(|i| i.mixer).unwrap_or(0);
                                let names: Vec<String> = (0..self.song.mixer.len())
                                    .map(|m| {
                                        if m == 0 {
                                            tr.mixer_master.to_string()
                                        } else {
                                            format!("{m:02}  {}", self.mixer_name(m))
                                        }
                                    })
                                    .collect();
                                let mut pick = None;
                                kit::dropdown(
                                    ui,
                                    &format!("wrap-insert-{id}"),
                                    &names[now as usize],
                                    200.0,
                                    |ui| {
                                        for (m, label) in names.iter().enumerate() {
                                            if ui
                                                .selectable_label(now as usize == m, label)
                                                .clicked()
                                            {
                                                pick = Some(m as u16);
                                            }
                                        }
                                    },
                                );
                                if let Some(m) = pick
                                    && m != now
                                {
                                    self.checkpoint();
                                    if let Some(i) =
                                        self.song.instruments.iter_mut().find(|i| i.id == ch)
                                    {
                                        i.mixer = m;
                                    }
                                    self.publish();
                                }
                            },
                        );
                        let extra = self.plugins.live.get(&id).map(|l| l.outs).unwrap_or(0);
                        if extra > 0 {
                            self.output_rows(ui, ch, id, extra);
                        }
                        row(ui, tr.plg_vol_pan, None, |ui| {
                            let Some(inst) = self.song.instrument(ch).cloned() else {
                                return;
                            };
                            let pan_fmt = |v: f64| {
                                if v.abs() < 0.005 {
                                    "C".to_string()
                                } else if v < 0.0 {
                                    format!("L{:.0}", -v * 100.0)
                                } else {
                                    format!("R{:.0}", v * 100.0)
                                }
                            };
                            let vol_fmt = |pos: f64| {
                                keyframexl::sequence::gain_db_text(
                                    keyframexl::sequence::volume_gain(pos as f32),
                                )
                            };
                            let knobs = [
                                (
                                    kit::Knob {
                                        label: tr.plg_k_volume,
                                        color: t::enc_orange(),
                                        min: 0.0,
                                        max: 1.0,
                                        default: keyframexl::sequence::volume_pos(1.0) as f64,
                                        log: false,
                                        fmt: &vol_fmt,
                                        hint: tr.plg_k_volume_hint,
                                    },
                                    keyframexl::sequence::volume_pos(inst.volume) as f64,
                                    keyframexl::automation::Target::ChannelVolume { inst: ch },
                                ),
                                (
                                    kit::Knob {
                                        label: tr.plg_k_pan,
                                        color: t::enc_white(),
                                        min: -1.0,
                                        max: 1.0,
                                        default: 0.0,
                                        log: false,
                                        fmt: &pan_fmt,
                                        hint: tr.plg_k_pan_hint,
                                    },
                                    inst.pan as f64,
                                    keyframexl::automation::Target::ChannelPan { inst: ch },
                                ),
                            ];
                            for (k, (spec, v, target)) in knobs.into_iter().enumerate() {
                                let mut v = v;
                                kit::automatable(ui, target);
                                let (_, started, changed) = kit::knob(ui, &spec, &mut v);
                                if started {
                                    self.checkpoint();
                                }
                                if changed
                                    && let Some(i) =
                                        self.song.instruments.iter_mut().find(|i| i.id == ch)
                                {
                                    if k == 0 {
                                        i.volume = keyframexl::sequence::volume_gain(v as f32);
                                    } else {
                                        i.pan = v as f32;
                                    }
                                    self.publish();
                                }
                            }
                        });
                        row(
                            ui,
                            tr.plg_stuck_notes,
                            Some(tr.plg_stuck_notes_hint),
                            |ui| {
                                if kit::tool_button(ui, tr.plg_notes_off, kit::Kind::Ghost, true)
                                    .clicked()
                                    && let Some(a) = &self.audio
                                    && let Ok(mut q) = a.live.lock()
                                {
                                    let _ = q.push(crate::audio::LiveCmd::Midi {
                                        inst: ch,
                                        bytes: [0xB0, 120, 0],
                                    });
                                    let _ = q.push(crate::audio::LiveCmd::Midi {
                                        inst: ch,
                                        bytes: [0xB0, 123, 0],
                                    });
                                }
                            },
                        );
                    });
                }
                if let Some((track, k)) = slot {
                    card(ui, tr.plg_card_mixer_slot, |ui| {
                        row(ui, tr.plg_place, None, |ui| {
                            ui.label(
                                RichText::new(fill(
                                    tr.plg_place_slot,
                                    &[
                                        ("track", &self.mixer_name(track)),
                                        ("n", &(k + 1).to_string()),
                                    ],
                                ))
                                .font(t::sans(13.0))
                                .color(t::text()),
                            );
                        });
                        row(ui, "On", Some(tr.plg_on_hint), |ui| {
                            let mut on = self.song.mixer[track].fx[k].enabled;
                            if kit_switch(ui, &mut on) {
                                self.checkpoint();
                                self.song.mixer[track].fx[k].enabled = on;
                                self.publish();
                            }
                        });
                        row(ui, tr.plg_mix, Some(tr.plg_mix_hint), |ui| {
                            let mut v = self.song.mixer[track].fx[k].mix as f64;
                            let fmt = |v: f64| format!("{:.0}%", v * 100.0);
                            let spec = kit::Knob {
                                label: tr.plg_mix,
                                color: t::enc_green(),
                                min: 0.0,
                                max: 1.0,
                                default: 1.0,
                                log: false,
                                fmt: &fmt,
                                hint: tr.plg_mix_knob_hint,
                            };
                            let (_, started, changed) = kit::knob(ui, &spec, &mut v);
                            if started {
                                self.checkpoint();
                            }
                            if changed {
                                self.song.mixer[track].fx[k].mix = v as f32;
                                self.publish();
                            }
                        });
                    });
                }
                card(ui, tr.plg_card_processing, |ui| {
                    let bridged = self.settings.plugins.bridged
                        && std::env::var_os("KFXL_NO_BRIDGE").is_none();
                    row(ui, tr.plg_process, None, |ui| {
                        let text = if bridged {
                            tr.plg_process_bridged
                        } else {
                            tr.plg_process_inside
                        };
                        ui.label(RichText::new(text).font(t::sans(13.0)).color(if bridged {
                            t::ok()
                        } else {
                            t::wait()
                        }));
                    });
                    row(ui, tr.plg_latency, None, |ui| {
                        let ms = self
                            .audio
                            .as_ref()
                            .map(|a| a.running.latency_ms())
                            .unwrap_or(0.0);
                        let text = if bridged {
                            fill(tr.plg_latency_buffer, &[("ms", &format!("{ms:.1}"))])
                        } else {
                            tr.plg_latency_none.to_string()
                        };
                        ui.label(RichText::new(text).font(t::sans(13.0)).color(t::text()));
                    });
                    ui.horizontal(|ui| {
                        if kit::tool_button(ui, tr.plg_change_in_settings, kit::Kind::Ghost, true)
                            .clicked()
                        {
                            self.open_settings(super::settings_ui::Page::Plugins);
                        }
                    });
                });
                card(ui, tr.plg_card_state, |ui| {
                    row(ui, tr.plg_file, None, |ui| {
                        ui.label(
                            RichText::new(&plugin.path)
                                .font(t::mono(11.5))
                                .color(t::faint()),
                        );
                    });
                    row(
                        ui,
                        tr.plg_saved_state,
                        Some(tr.plg_saved_state_hint),
                        |ui| {
                            let size = plugin.state.len() * 3 / 4;
                            let text = if size == 0 {
                                tr.plg_none_yet.to_string()
                            } else if size < 1024 {
                                fill(tr.plg_state_bytes, &[("n", &size.to_string())])
                            } else {
                                format!("{:.1} KB", size as f64 / 1024.0)
                            };
                            ui.label(RichText::new(text).font(t::sans(13.0)).color(t::text()));
                        },
                    );
                    ui.horizontal(|ui| {
                        if kit::tool_button(ui, tr.plg_reload, kit::Kind::Ghost, true)
                            .on_hover_text(tr.plg_reload_tip)
                            .clicked()
                        {
                            self.reload_plugin(id);
                        }
                        if kit::tool_button(ui, tr.plg_reset_defaults, kit::Kind::Ghost, true)
                            .on_hover_text(tr.plg_reset_defaults_tip)
                            .clicked()
                        {
                            self.checkpoint();
                            if let Some(r) = self.song.plugin_ref_mut(id) {
                                r.state.clear();
                            }
                            self.publish();
                            self.reload_plugin(id);
                            self.say(
                                Tone::Quiet,
                                fill(tr.plg_reset_done, &[(tr.plg_col_name, &plugin.name)]),
                            );
                        }
                    });
                });
            });
    }

    fn embedded_editor(&mut self, ui: &mut egui::Ui, id: u64) {
        let tr = self.tr();
        let Some(l) = self.plugins.live.get(&id) else {
            ui.label(
                RichText::new(tr.plg_not_loaded_dot)
                    .font(t::sans(13.0))
                    .color(t::faint()),
            );
            return;
        };
        if l.proc.is_none() {
            return;
        }
        if crate::fl_link::is_link(std::path::Path::new(&l.path)) {
            fl_link_panel(ui);
            return;
        }
        if crate::fl_native::slot(&l.path).is_some() {
            ui.label(
                RichText::new(tr.plg_flex_standalone)
                    .font(t::sans(13.0))
                    .color(t::text()),
            );
            return;
        }
        if !l.gui {
            ui.label(
                RichText::new(tr.plg_no_editor)
                    .font(t::sans(13.0))
                    .color(t::muted()),
            );
            return;
        }
        let Some(parent) = self.plugins.parent else {
            ui.label(
                RichText::new(tr.plg_wayland_note)
                    .font(t::sans(12.0))
                    .color(t::faint()),
            );
            let open = l.editor;
            if kit::button(
                ui,
                if open {
                    tr.plg_hide_editor
                } else {
                    tr.plg_show_editor
                },
                kit::Kind::Primary,
                true,
            )
            .clicked()
            {
                self.show_editor(id);
            }
            return;
        };
        let ppp = ui.ctx().pixels_per_point();
        let (w, h) = self
            .plugins
            .editor_size
            .get(&id)
            .copied()
            .unwrap_or((640, 400));
        let size = egui::vec2(w as f32 / ppp, h as f32 / ppp);
        // FL sizes its wrapper to the plugin: the floating window fits the
        // editor once per editor size (larger or smaller), then the user may
        // resize it by hand.
        // A preset reload first reports a small size: wait until it holds 0.4 s.
        let now = ui.input(|i| i.time);
        let held_id = egui::Id::new(("editor-size-held", id));
        let since = match ui.data(|d| d.get_temp::<((i32, i32), f64)>(held_id)) {
            Some((s, t)) if s == (w, h) => t,
            _ => {
                ui.data_mut(|d| d.insert_temp(held_id, ((w, h), now)));
                now
            }
        };
        let settled = now - since >= 0.4;
        if !settled {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
        let off = size - ui.available_size();
        if settled
            && (off.x.abs() > 1.0 || off.y.abs() > 1.0)
            && self.plugins.grown.get(&id) != Some(&(w, h))
            && let Some(tab) = self.plugins.tab.clone()
        {
            self.plugins.grown.insert(id, (w, h));
            self.win(super::Cmd::Grow(tab, off));
        }
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
        // Only the part of the editor inside this window shows; popups and
        // menus of our own hide it (an X11 child would cover them). Floating
        // windows sit over the side panels too; the menu, deck and status bar
        // stay clear.
        let (screen, work) = (ui.ctx().screen_rect(), ui.ctx().available_rect());
        let band = egui::Rect::from_x_y_ranges(screen.x_range(), work.y_range());
        let shown = rect.intersect(ui.clip_rect()).intersect(band);
        let popup = ui.ctx().memory(|m| m.any_popup_open());
        let visible = shown.width() > 4.0 && shown.height() > 4.0 && !popup;
        let px = |r: egui::Rect| {
            [
                (r.left() * ppp).round() as i32,
                (r.top() * ppp).round() as i32,
                (r.width() * ppp).round().max(1.0) as i32,
                (r.height() * ppp).round().max(1.0) as i32,
            ]
        };
        // The editor keeps its own size; the frame around it cuts off what is hidden.
        let full = [px(rect)[0], px(rect)[1], w, h];
        let clip = if visible { px(shown) } else { full };
        self.plugins.seen.push(id);
        if self.plugins.placed.get(&id) != Some(&(full, clip, visible)) {
            self.plugins.placed.insert(id, (full, clip, visible));
            if let Some(h) = &self.plugins.host {
                h.send(Req::Embed {
                    id,
                    parent,
                    rect: full,
                    clip,
                    visible,
                });
            }
            // The host moves the X window after this frame: paint again so the
            // spot it left does not keep a stale picture of the editor.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn parameters_page(&mut self, ui: &mut egui::Ui, id: u64) {
        let tr = self.tr();
        let now = ui.input(|i| i.time);
        // Re-read twice a second: the editor changes them too.
        let stale = self.plugins.params.get(&id).is_none_or(|p| now - p.1 > 0.5);
        if stale {
            if let Some(h) = &self.plugins.host {
                h.send(Req::Params(id));
            }
            let rows = self
                .plugins
                .params
                .remove(&id)
                .map(|p| p.0)
                .unwrap_or_default();
            self.plugins.params.insert(id, (rows, now));
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.plugins.param_filter)
                    .hint_text(tr.plg_filter_params)
                    .desired_width(240.0),
            );
            let n = self.plugins.params.get(&id).map(|p| p.0.len()).unwrap_or(0);
            ui.label(
                RichText::new(fill(tr.plg_n_params, &[("n", &n.to_string())]))
                    .font(t::sans(12.0))
                    .color(t::faint()),
            );
        });
        let want = self.plugins.param_filter.to_lowercase();
        let rows: Vec<crate::plugins::ParamRow> = self
            .plugins
            .params
            .get(&id)
            .map(|p| {
                p.0.iter()
                    .filter(|r| want.is_empty() || r.name.to_lowercase().contains(&want))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut set = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Name | control | value text; the control takes the room left over.
                let (name_w, text_w) = (200.0, 120.0);
                ui.spacing_mut().slider_width =
                    (ui.available_width() - name_w - text_w - 40.0).clamp(120.0, 520.0);
                egui::Grid::new(("params", id))
                    .num_columns(3)
                    .striped(true)
                    .min_col_width(0.0)
                    .spacing(egui::vec2(12.0, 4.0))
                    .show(ui, |ui| {
                        for r in &rows {
                            let left = egui::Layout::left_to_right(egui::Align::Center);
                            ui.allocate_ui_with_layout(egui::vec2(name_w, 24.0), left, |ui| {
                                ui.set_width(name_w);
                                let color = if r.automatable { t::text() } else { t::muted() };
                                let name = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(&r.name).font(t::sans(13.0)).color(color),
                                        )
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                    )
                                    .on_hover_text(&r.name);
                                if r.automatable {
                                    name.context_menu(|ui| {
                                        kit::auto_menu_item(
                                            ui,
                                            keyframexl::automation::Target::PluginParam {
                                                plugin: id,
                                                param: r.id,
                                            },
                                        )
                                    });
                                }
                            });
                            // On/off parameters get a switch, the rest a slider over the plugin's 0..1 range.
                            if matches!(r.text.trim(), "On" | "Off") {
                                let on = r.value >= 0.5;
                                let (pick, _) = kit::chips(
                                    ui,
                                    &format!("param-{id}-{}", r.id),
                                    &[tr.plg_off, tr.plg_on],
                                    on as usize,
                                    Some(&[44.0, 44.0]),
                                );
                                if let Some(i) = pick
                                    && (i == 1) != on
                                {
                                    set.push((r.id, i as f64));
                                }
                            } else {
                                let mut v = r.value;
                                let slider =
                                    ui.add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false));
                                if slider.changed() {
                                    set.push((r.id, v));
                                }
                                if r.automatable {
                                    slider.context_menu(|ui| {
                                        kit::auto_menu_item(
                                            ui,
                                            keyframexl::automation::Target::PluginParam {
                                                plugin: id,
                                                param: r.id,
                                            },
                                        )
                                    });
                                }
                            }
                            // A switch already shows its state; other values print the plugin's text.
                            let text = if matches!(r.text.trim(), "On" | "Off") {
                                String::new()
                            } else {
                                tidy_value(&r.text)
                            };
                            ui.allocate_ui_with_layout(egui::vec2(text_w, 24.0), left, |ui| {
                                ui.set_width(text_w);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(text).font(t::mono(12.0)).color(t::muted()),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.end_row();
                        }
                    });
            });
        if !set.is_empty() {
            // The first change of a gesture is an undo step: read the plugin's
            // state as it is now, so undo has something to go back to.
            let now = std::time::Instant::now();
            if self
                .plugins
                .param_last
                .is_none_or(|t| now - t > std::time::Duration::from_millis(600))
            {
                self.collect_plugin_states();
                self.checkpoint();
            }
            self.plugins.param_last = Some(now);
            self.plugins.refresh_at = Some(now + std::time::Duration::from_millis(500));
            self.plugins.param_dirty = true;
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(550));
        }
        for (param, value) in set {
            if let Some(h) = &self.plugins.host {
                h.send(Req::SetParam { id, param, value });
            }
            let mut automatable = false;
            if let Some(p) = self.plugins.params.get_mut(&id)
                && let Some(r) = p.0.iter_mut().find(|r| r.id == param)
            {
                r.value = value;
                automatable = r.automatable;
            }
            if automatable {
                kit::note_tweaked(
                    ui.ctx(),
                    keyframexl::automation::Target::PluginParam { plugin: id, param },
                );
            }
            // Read back soon (not every frame of a drag) so the text follows.
            let now = ui.input(|i| i.time);
            if let Some(p) = self.plugins.params.get_mut(&id) {
                p.1 = p.1.min(now - 0.4);
            }
            self.dirty_plugin_state();
        }
    }

    /// A parameter changed: the project has unsaved changes (the state is read on save).
    pub(crate) fn dirty_plugin_state(&mut self) {
        self.edits += 1;
    }

    /// End of frame: editors whose window was not drawn this frame are hidden.
    pub(crate) fn plugins_end_frame(&mut self) {
        let seen = std::mem::take(&mut self.plugins.seen);
        let hide: Vec<u64> = self
            .plugins
            .placed
            .iter()
            .filter(|(id, (_, _, vis))| *vis && !seen.contains(id))
            .map(|(id, _)| *id)
            .collect();
        for id in hide {
            if let Some((rect, clip, _)) = self.plugins.placed.get(&id).copied() {
                self.plugins.placed.insert(id, (rect, clip, false));
                if let (Some(h), Some(parent)) = (&self.plugins.host, self.plugins.parent) {
                    h.send(Req::Embed {
                        id,
                        parent,
                        rect,
                        clip,
                        visible: false,
                    });
                }
            }
        }
    }

    pub(crate) fn show_editor(&mut self, id: u64) {
        let tr = self.tr();
        let Some(l) = self.plugins.live.get_mut(&id) else {
            return;
        };
        if l.proc.is_none() {
            l.show_when_loaded = true;
            return;
        }
        if !l.gui {
            let name = l.name.clone();
            self.say(
                Tone::Quiet,
                fill(tr.plg_no_editor_window, &[(tr.plg_col_name, &name)]),
            );
            return;
        }
        if let Some(h) = &self.plugins.host {
            h.send(if l.editor {
                Req::HideEditor(id)
            } else {
                Req::ShowEditor(id)
            });
        }
    }

    /// FL: Add → instrument plugin. A new channel on Master, selected, editor opens.
    pub(crate) fn add_plugin_channel(&mut self, s: &Scanned) {
        let tr = self.tr();
        // FL plays only its selected channel: a second link would play the
        // same sound twice.
        if crate::fl_link::is_link(&s.path)
            && let Some(other) = self.song.instruments.iter().find(|i| {
                matches!(&i.kind, InstrumentKind::Plugin { plugin }
                    if crate::fl_link::is_link(std::path::Path::new(&plugin.path)))
            })
        {
            self.say(
                Tone::Bad,
                fill(tr.plg_link_exists, &[(tr.plg_col_name, &other.name)]),
            );
            return;
        }
        self.checkpoint();
        let pid = self.song.next_plugin_id();
        let name = self.song.free_name(&s.name);
        let plugin = PluginRef {
            id: pid,
            format: crate::plugins::format_of(&s.path).into(),
            path: s.path.display().to_string(),
            name: s.name.clone(),
            vendor: s.vendor.clone(),
            state: String::new(),
            outs: vec![],
        };
        let id = self
            .song
            .add_channel(&name, InstrumentKind::Plugin { plugin });
        self.rack_sel = vec![id];
        self.instrument = id;
        self.publish();
        self.plugins_frame();
        if let Some(l) = self.plugins.live.get_mut(&pid) {
            l.show_when_loaded = true;
        }
        self.say(
            Tone::Wait,
            fill(tr.plg_loading_name, &[(tr.plg_col_name, &s.name)]),
        );
    }

    /// Status line for a plugin channel or slot.
    pub(crate) fn plugin_status(&self, id: u64) -> (String, eframe::egui::Color32) {
        let tr = self.tr();
        match self.plugins.live.get(&id) {
            None => (tr.plg_st_not_loaded.into(), t::faint()),
            Some(l) if l.error.is_some() => (l.error.clone().unwrap_or_default(), t::bad()),
            Some(l) if l.proc.is_none() => (tr.plg_st_loading.into(), t::wait()),
            Some(l) if l.editor => (tr.plg_st_editor_open.into(), t::ok()),
            Some(_) => (tr.plg_st_ready.into(), t::muted()),
        }
    }

    /// Settings → Plugins (FL: Manage plugins): find, check and act on plugins.
    pub(crate) fn plugins_page(&mut self, ui: &mut egui::Ui, s: &mut Settings) {
        let tr = self.tr();
        let id = egui::Id::new("settings-plugins");
        let (mut query, mut kind, mut picked) = ui
            .data(|d| d.get_temp::<(String, usize, Option<PathBuf>)>(id))
            .unwrap_or_default();
        // A single-plugin reprobe finished: swap it into the list.
        // (The worker already wrote it to the cache via rescan_one.)
        let mut single_done = false;
        if let Some((_, rx)) = &self.plugins.rescan_one {
            match rx.try_recv() {
                Ok(fresh) => {
                    if let Some(slot) = self.plugins.list.iter_mut().find(|c| c.path == fresh.path)
                    {
                        *slot = fresh;
                    } else {
                        self.plugins.list.push(fresh);
                        self.plugins.list.sort_by_key(|a| a.name.to_lowercase());
                    }
                    single_done = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => single_done = true,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if single_done {
            self.plugins.rescan_one = None;
        }
        let busy = self.plugins.scan_rx.is_some();
        let single_busy = self.plugins.rescan_one.as_ref().map(|(p, _)| p.clone());
        card(ui, tr.plg_card_safety, |ui| {
            row(ui, tr.plg_bridged, Some(tr.plg_bridged_hint), |ui| {
                let mut on = s.plugins.bridged;
                if kit_switch(ui, &mut on) {
                    s.plugins.bridged = on;
                }
            });
        });
        // Toolbar: search, kind filter, counts, rescan.
        let list = self.plugins.list.clone();
        let blocked = s.plugins.blocked.clone();
        let hidden = |p: &Scanned| blocked.contains(&p.path.display().to_string());
        let problems = list.iter().filter(|p| !p.usable()).count();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text(tr.plg_search)
                    .desired_width(190.0),
            );
            let kinds = [
                tr.plg_kind_all,
                tr.plg_kind_instruments,
                tr.plg_kind_effects,
                tr.plg_kind_problems,
            ];
            if let (Some(i), _) = kit::chips(ui, "plug-kind", &kinds, kind, None) {
                kind = i;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kit::tool_button(
                    ui,
                    if busy {
                        tr.plg_scanning
                    } else {
                        tr.plg_rescan_all
                    },
                    kit::Kind::Ghost,
                    !busy,
                )
                .clicked()
                {
                    self.rescan_plugins(true);
                }
            });
        });
        let ready = list.len() - problems;
        let note = if busy {
            self.plugins.scan_note.clone().unwrap_or_default()
        } else {
            fill(
                tr.plg_counts,
                &[
                    ("n", &list.len().to_string()),
                    (tr.plg_st_ready, &ready.to_string()),
                    ("problems", &problems.to_string()),
                ],
            )
        };
        ui.label(RichText::new(note).font(t::sans(12.0)).color(if busy {
            t::wait()
        } else {
            t::faint()
        }));
        let want = query.to_lowercase();
        let shown: Vec<&Scanned> = list
            .iter()
            .filter(|p| match kind {
                1 => p.instrument,
                2 => !p.instrument,
                3 => !p.usable(),
                _ => true,
            })
            .filter(|p| {
                want.is_empty()
                    || p.name.to_lowercase().contains(&want)
                    || p.vendor.to_lowercase().contains(&want)
            })
            .collect();
        // The table.
        egui::Frame::new()
            .fill(t::sunken())
            .stroke(egui::Stroke::new(1.0_f32, t::edge()))
            .corner_radius(9.0)
            .inner_margin(egui::Margin::symmetric(0, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let w = ui.available_width();
                let cols = [30.0, 0.40 * w, 0.14 * w, 0.24 * w];
                let x = |i: usize| cols[..i].iter().sum::<f32>();
                let (head, _) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::hover());
                for (i, h) in [
                    tr.plg_col_name,
                    tr.plg_col_type,
                    tr.plg_col_vendor,
                    tr.plg_col_status,
                ]
                .iter()
                .enumerate()
                {
                    kit::caps(
                        ui.painter(),
                        egui::pos2(head.left() + x(i + 1), head.center().y),
                        egui::Align2::LEFT_CENTER,
                        h,
                        10.5,
                        t::faint(),
                    );
                }
                if shown.is_empty() {
                    ui.add_space(8.0);
                    let text = if list.is_empty() {
                        tr.plg_list_empty
                    } else {
                        tr.plg_nothing_matches
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new(text).font(t::sans(13.0)).color(t::faint()));
                    });
                    ui.add_space(8.0);
                }
                egui::ScrollArea::vertical()
                    .id_salt("plug-table")
                    .max_height(210.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (n, p) in shown.iter().enumerate() {
                            let (r, resp) =
                                ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
                            let on = picked.as_ref() == Some(&p.path);
                            let fill = if on {
                                t::selection()
                            } else if resp.hovered() {
                                t::raised()
                            } else if n % 2 == 1 {
                                t::side()
                            } else {
                                t::sunken()
                            };
                            let pt = ui.painter();
                            pt.rect_filled(r, 0.0, fill);
                            let off = hidden(p);
                            let ink = if off || !p.usable() {
                                t::faint()
                            } else {
                                t::text()
                            };
                            let y = r.center().y;
                            let (status, color) = match &p.status {
                                Status::Ok if off => (tr.plg_st_hidden.to_string(), t::faint()),
                                Status::Ok => (tr.plg_st_ready.to_string(), t::ok()),
                                Status::Hangs => (tr.plg_st_hangs.to_string(), t::bad()),
                                Status::Crashes => (tr.plg_st_crashes.to_string(), t::bad()),
                                Status::Failed(e) => (e.clone(), t::bad()),
                            };
                            kit::led(pt, egui::pos2(r.left() + 16.0, y), 3.5, Some(color));
                            let font = t::sans(13.0);
                            let cell = |i: usize| r.left() + x(i);
                            pt.text(
                                egui::pos2(cell(1), y),
                                egui::Align2::LEFT_CENTER,
                                kit::elide(pt, &p.name, font.clone(), cols[1] - 10.0),
                                font.clone(),
                                ink,
                            );
                            pt.text(
                                egui::pos2(cell(2), y),
                                egui::Align2::LEFT_CENTER,
                                format!(
                                    "{} · {}",
                                    if p.instrument {
                                        tr.plg_instrument
                                    } else {
                                        tr.plg_effect
                                    },
                                    crate::plugins::format_of(&p.path).to_uppercase()
                                ),
                                t::sans(12.0),
                                t::muted(),
                            );
                            pt.text(
                                egui::pos2(cell(3), y),
                                egui::Align2::LEFT_CENTER,
                                kit::elide(pt, &p.vendor, t::sans(12.0), cols[3] - 10.0),
                                t::sans(12.0),
                                t::muted(),
                            );
                            pt.text(
                                egui::pos2(cell(4), y),
                                egui::Align2::LEFT_CENTER,
                                kit::elide(pt, &status, t::sans(12.0), w - x(4) - 12.0),
                                t::sans(12.0),
                                color,
                            );
                            if resp.clicked() {
                                picked = Some(p.path.clone());
                            }
                            if resp.double_clicked() && p.usable() && p.instrument {
                                self.add_plugin_channel(p);
                            }
                        }
                    });
            });
        // Details and actions for the picked plugin.
        if let Some(p) = picked
            .as_ref()
            .and_then(|path| list.iter().find(|p| &p.path == path))
            .cloned()
        {
            card(ui, tr.plg_card_selected, |ui| {
                ui.label(
                    RichText::new(&p.name)
                        .font(t::medium(16.0))
                        .color(t::text()),
                );
                let mut facts = vec![
                    if p.vendor.is_empty() {
                        tr.plg_unknown_vendor.to_string()
                    } else {
                        p.vendor.clone()
                    },
                    if p.instrument {
                        tr.plg_instrument.into()
                    } else {
                        tr.plg_effect.into()
                    },
                    crate::plugins::format_of(&p.path).to_uppercase(),
                ];
                if !p.category.is_empty() {
                    facts.push(p.category.clone());
                }
                if p.bridged {
                    facts.push(tr.plg_via_yabridge.into());
                }
                if !p.gui {
                    facts.push(tr.plg_no_own_editor.into());
                }
                ui.label(
                    RichText::new(facts.join(" · "))
                        .font(t::sans(12.5))
                        .color(t::muted()),
                );
                ui.label(
                    RichText::new(p.path.display().to_string())
                        .font(t::mono(11.5))
                        .color(t::faint()),
                );
                let off = hidden(&p);
                let this_busy = single_busy.as_ref() == Some(&p.path);
                ui.horizontal_wrapped(|ui| {
                    if p.instrument {
                        if kit::tool_button(ui, tr.plg_add_to_rack, kit::Kind::Primary, p.usable())
                            .clicked()
                        {
                            self.add_plugin_channel(&p);
                        }
                    } else {
                        let t_ = self.mix_sel;
                        let label = fill(tr.plg_add_to, &[("track", &self.mixer_name(t_))]);
                        if kit::tool_button(ui, &label, kit::Kind::Primary, p.usable()).clicked() {
                            let k = self.song.mixer.get(t_).map(|m| m.fx.len()).unwrap_or(0);
                            self.set_slot_plugin(t_, k, &p);
                        }
                    }
                    if kit::tool_button(ui, tr.plg_show_in_folder, kit::Kind::Ghost, true).clicked()
                    {
                        let dir = p
                            .path
                            .parent()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| p.path.clone());
                        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                    }
                    if kit::tool_button(
                        ui,
                        if this_busy {
                            tr.plg_probing
                        } else {
                            tr.plg_rescan_this
                        },
                        kit::Kind::Ghost,
                        !this_busy && !busy,
                    )
                    .on_hover_text(tr.plg_rescan_this_tip)
                    .clicked()
                    {
                        let path = p.path.clone();
                        let (tx, rx) = mpsc::channel();
                        std::thread::spawn(move || {
                            let fresh = crate::plugins::rescan_one(&path);
                            let _ = tx.send(fresh);
                        });
                        self.plugins.rescan_one = Some((p.path.clone(), rx));
                    }
                    let key = p.path.display().to_string();
                    if kit::tool_button(
                        ui,
                        if off {
                            tr.plg_show_in_lists
                        } else {
                            tr.plg_hide_from_lists
                        },
                        kit::Kind::Ghost,
                        true,
                    )
                    .on_hover_text(tr.plg_hide_tip)
                    .clicked()
                    {
                        if off {
                            s.plugins.blocked.retain(|b| b != &key);
                        } else {
                            s.plugins.blocked.push(key);
                        }
                    }
                });
                if !p.usable() {
                    ui.label(
                        RichText::new(tr.plg_failed_test)
                            .font(t::sans(12.0))
                            .color(t::wait()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let label = if p.instrument {
                            tr.plg_try_rack.to_string()
                        } else {
                            fill(
                                tr.plg_try_add_to,
                                &[("track", &self.mixer_name(self.mix_sel))],
                            )
                        };
                        if kit::tool_button(ui, label.as_str(), kit::Kind::Ghost, true).clicked() {
                            if p.instrument {
                                self.add_plugin_channel(&p);
                            } else {
                                let t_ = self.mix_sel;
                                let k = self.song.mixer.get(t_).map(|m| m.fx.len()).unwrap_or(0);
                                self.set_slot_plugin(t_, k, &p);
                            }
                        }
                    });
                    ui.label(
                        RichText::new(tr.plg_missing_hint)
                            .font(t::sans(12.0))
                            .color(t::faint()),
                    );
                }
            });
        }
        card(ui, tr.plg_card_windows, |ui| {
            // Builds are found on disk once per opening of the page.
            let key = egui::Id::new("wine-builds");
            let builds: Vec<(String, Option<PathBuf>)> = match ui.data(|d| d.get_temp(key)) {
                Some(b) => b,
                None => {
                    let b = crate::plugins::wine_builds();
                    ui.data_mut(|d| d.insert_temp(key, b.clone()));
                    b
                }
            };
            let current = builds
                .iter()
                .find(|(_, b)| *b == s.plugins.wine)
                .map(|(l, _)| l.clone())
                .unwrap_or_else(|| {
                    s.plugins
                        .wine
                        .as_ref()
                        .map(|b| b.display().to_string())
                        .unwrap_or_else(|| tr.plg_system_wine.into())
                });
            row(ui, "Wine", Some(tr.plg_wine_hint), |ui| {
                egui::ComboBox::from_id_salt("wine-build")
                    .width(300.0)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        kit::menu_style(ui);
                        for (label, bin) in &builds {
                            if ui
                                .selectable_label(*bin == s.plugins.wine, label)
                                .on_hover_text(
                                    bin.as_ref()
                                        .map(|b| b.display().to_string())
                                        .unwrap_or_else(|| tr.plg_wine_from_path.into()),
                                )
                                .clicked()
                            {
                                s.plugins.wine = bin.clone();
                            }
                        }
                    });
                if kit::tool_button(ui, tr.plg_other, kit::Kind::Ghost, true)
                    .on_hover_text(tr.plg_other_tip)
                    .clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                {
                    let bin = if d.join("wine").is_file() {
                        d
                    } else if d.join("bin/wine").is_file() {
                        d.join("bin")
                    } else {
                        d.join("files/bin")
                    };
                    if bin.join("wine").is_file() {
                        s.plugins.wine = Some(bin);
                    } else {
                        self.say(Tone::Bad, tr.plg_no_wine);
                    }
                }
            });
            if s.plugins
                .wine
                .as_deref()
                .is_some_and(crate::plugins::has_dxvk)
            {
                row(ui, "DXVK", Some(tr.plg_dxvk_hint), |ui| {
                    kit_switch(ui, &mut s.plugins.dxvk);
                });
            }
            // Another Wine's server on the same prefix blocks this one entirely.
            let now = ui.input(|i| i.time);
            let ckey = egui::Id::new("wine-conflict");
            let conflict: Option<(u32, PathBuf)> =
                match ui.data(|d| d.get_temp::<(f64, Option<(u32, PathBuf)>)>(ckey)) {
                    Some((at, c)) if now - at < 2.0 => c,
                    _ => {
                        let c = crate::plugins::foreign_wineserver();
                        ui.data_mut(|d| d.insert_temp(ckey, (now, c.clone())));
                        c
                    }
                };
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(2));
            if let Some((pid, exe)) = conflict {
                let prefix = crate::plugins::wine_prefix()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, Some(t::bad()));
                    ui.label(
                        RichText::new(fill(
                            tr.plg_other_wine,
                            &[("prefix", &prefix), ("exe", &exe.display().to_string())],
                        ))
                        .font(t::sans(12.5))
                        .color(t::text()),
                    );
                    if kit::tool_button(ui, tr.plg_stop_it, kit::Kind::Normal, true)
                        .on_hover_text(tr.plg_stop_it_tip)
                        .clicked()
                    {
                        match crate::plugins::stop_foreign_wineserver(pid, &exe) {
                            Ok(()) => {
                                ui.data_mut(|d| d.remove::<(f64, Option<(u32, PathBuf)>)>(ckey));
                                let bin = s.plugins.wine.clone();
                                crate::plugins::set_wine(bin);
                                self.restart_plugin_host(ui.ctx());
                                self.say(Tone::Quiet, tr.plg_wine_stopped);
                            }
                            Err(e) => self.say(
                                Tone::Bad,
                                fill(tr.plg_stop_wine_err, &[("err", &e.to_string())]),
                            ),
                        }
                    }
                });
            }
        });
        card(ui, tr.plg_card_folders, |ui| {
            let mut std_dirs = crate::plugins::search_dirs(&[]);
            std_dirs.extend(crate::plugins::vst2_dirs());
            std_dirs.extend(crate::plugins::clap_dirs());
            std_dirs.dedup();
            for d in &std_dirs {
                ui.horizontal(|ui| {
                    let there = d.is_dir();
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, there.then_some(t::ok()));
                    ui.label(
                        RichText::new(d.display().to_string())
                            .font(t::mono(12.0))
                            .color(if there { t::text() } else { t::faint() }),
                    );
                    if !there {
                        ui.label(
                            RichText::new(tr.plg_missing)
                                .font(t::sans(11.5))
                                .color(t::faint()),
                        );
                    }
                });
            }
            let mut remove = None;
            for (i, d) in s.plugins.extra_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, d.is_dir().then_some(t::ok()));
                    ui.label(
                        RichText::new(d.display().to_string())
                            .font(t::mono(12.0))
                            .color(t::text()),
                    );
                    if kit::tool_button(ui, tr.mixer_remove, kit::Kind::Ghost, true).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.plugins.extra_dirs.remove(i);
            }
            ui.horizontal(|ui| {
                if kit::tool_button(ui, tr.plg_add_folder, kit::Kind::Ghost, true).clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                    && !s.plugins.extra_dirs.contains(&d)
                {
                    s.plugins.extra_dirs.push(d);
                }
                ui.label(
                    RichText::new(tr.plg_paths_note)
                        .font(t::sans(12.0))
                        .color(t::faint()),
                );
            });
        });
        ui.data_mut(|d| d.insert_temp(id, (query, kind, picked)));
    }
}

/// A plugin's value text, minus float noise: "0.0000000" → "0", "-6.0200001 dB" → "-6.02 dB".
fn tidy_value(text: &str) -> String {
    let text = text.trim();
    let (num, unit) = text.split_once(' ').unwrap_or((text, ""));
    match num.parse::<f64>() {
        Ok(v) if num.contains('.') => {
            let mut n = format!("{v:.2}");
            while n.ends_with('0') {
                n.pop();
            }
            if n.ends_with('.') {
                n.pop();
            }
            if n == "-0" {
                n = "0".into();
            }
            if unit.is_empty() {
                n
            } else {
                format!("{n} {unit}")
            }
        }
        _ => text.to_string(),
    }
}

impl App {
    /// FL wrapper ◀ preset ▶: steps through our saved presets of this plugin
    /// (channel presets for a channel, effect presets for a mixer slot).
    fn preset_stepper(&mut self, ui: &mut egui::Ui, plugin: &PluginRef, channel: Option<u64>) {
        let tr = self.tr();
        use super::presets_ui::Target;
        use keyframexl::preset::Kind;
        let id = plugin.id;
        let slot = self.song.mixer.iter().enumerate().find_map(|(t, m)| {
            m.fx.iter()
                .position(|f| f.plugin.as_ref().is_some_and(|p| p.id == id))
                .map(|k| (t, k))
        });
        let (kind, target, key) = match (channel, slot) {
            (Some(ch), _) => (Kind::Channel, Target::Channel(ch), format!("ch{ch}")),
            (None, Some((t, k))) => (Kind::Fx, Target::Fx(t, k), format!("fx{t}.{k}")),
            _ => return,
        };
        let now = ui.input(|i| i.time);
        let cache_key = (kind.ext(), plugin.path.clone());
        if self
            .plugins
            .preset_list
            .get(&cache_key)
            .is_none_or(|(at, _)| now - at > 3.0)
        {
            let list = super::presets_ui::plugin_presets(kind, &plugin.path);
            self.plugins
                .preset_list
                .insert(cache_key.clone(), (now, list));
        }
        let list = self
            .plugins
            .preset_list
            .get(&cache_key)
            .map(|(_, l)| l.clone())
            .unwrap_or_default();
        let cur = self
            .plugins
            .preset_at
            .get(&key)
            .and_then(|p| list.iter().position(|q| q == p));
        ui.add_space(8.0);
        let n = list.len();
        let tip = if n == 0 {
            fill(
                tr.plg_no_saved_presets,
                &[
                    ("kind", &kind.label().to_lowercase()),
                    (tr.plg_col_name, &plugin.name),
                ],
            )
        } else {
            fill(
                tr.plg_saved_presets,
                &[("n", &n.to_string()), (tr.plg_col_name, &plugin.name)],
            )
        };
        let prev = kit::tool_button(ui, "◀", kit::Kind::Ghost, n > 0);
        let name = cur
            .map(|i| {
                list[i]
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_else(|| {
                if n == 0 {
                    tr.plg_no_presets.into()
                } else {
                    tr.plg_preset.into()
                }
            });
        // Fixed width: the arrows stay put while stepping (FL's preset box).
        let shown = kit::elide(ui.painter(), &name, t::sans(12.0), 140.0);
        let (r, resp) =
            ui.allocate_exact_size(egui::vec2(144.0, kit::TOOL_H), egui::Sense::hover());
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            shown,
            t::sans(12.0),
            if cur.is_some() { t::text() } else { t::faint() },
        );
        kit::tip(resp, tip);
        let next = kit::tool_button(ui, "▶", kit::Kind::Ghost, n > 0);
        let step = if kit::tip(prev, tr.tide_prev_preset).clicked() {
            Some(cur.map(|i| (i + n - 1) % n).unwrap_or(n - 1))
        } else if kit::tip(next, tr.tide_next_preset).clicked() {
            Some(cur.map(|i| (i + 1) % n).unwrap_or(0))
        } else {
            None
        };
        if let Some(i) = step {
            let path = list[i].clone();
            self.load_preset_into(&path, target);
            self.plugins.preset_at.insert(key, path);
        }
    }
}

/// The FL Studio live link channel: FL runs on its own (FLEX works only
/// there); this shows the link's state, its measured delay and what FLEX
/// needs, and starts or reconnects it.
fn fl_link_panel(ui: &mut egui::Ui) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    use crate::fl_link::{self as fl, LinkState};
    let st = fl::status();
    let progress = fl::progress();
    let state = fl::state(&st, &progress);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(500));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr.plg_fl_live_link)
                .font(t::medium(15.0))
                .color(t::text()),
        );
        let (word, color) = match &state {
            LinkState::Off => (tr.plg_link_not_running, None),
            LinkState::Starting(_) => (tr.plg_link_starting, Some(t::wait())),
            LinkState::Connected => (tr.plg_link_connected, Some(t::ok())),
            LinkState::Degraded(_) => (tr.plg_link_check, Some(t::wait())),
            LinkState::Error(_) => (tr.plg_link_error, Some(t::bad())),
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(word).font(t::sans(13.0)).color(t::text()));
            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::hover());
            kit::led(ui.painter(), r.center(), 4.0, color);
        });
    });
    ui.label(
        RichText::new(tr.plg_link_about)
            .font(t::sans(13.0))
            .color(t::muted()),
    );
    match &state {
        LinkState::Starting(m) | LinkState::Degraded(m) => {
            ui.label(RichText::new(m).font(t::sans(13.0)).color(t::wait()));
        }
        LinkState::Error(m) => {
            ui.label(RichText::new(m).font(t::sans(13.0)).color(t::bad()));
        }
        _ => {}
    }
    ui.add_space(8.0);
    let stats = st.stats.clone().unwrap_or_default();
    let delay = match stats.round_trip_ms() {
        Some(ms) => fill(
            tr.plg_link_delay_val,
            &[
                ("ms", &format!("{ms:.0}")),
                ("frames", &stats.round_trip.to_string()),
            ],
        ),
        None => tr.plg_link_not_measured.into(),
    };
    let flow = match stats.silent_secs {
        _ if st.stats.is_none() => tr.plg_link_chan_off.to_string(),
        None => tr.plg_link_no_sound.to_string(),
        Some(s) if s < 1.0 => tr.plg_link_sound_in.to_string(),
        Some(s) => fill(tr.plg_link_last_sound, &[("s", &format!("{s:.0}"))]),
    };
    let rows: [(Option<bool>, &str, String); 7] = [
        (
            Some(st.fl),
            "FL Studio",
            if st.fl {
                tr.plg_link_running
            } else {
                tr.plg_link_not_running_lc
            }
            .into(),
        ),
        (
            Some(st.port),
            tr.plg_link_notes,
            if st.port {
                tr.plg_link_port_ready
            } else {
                tr.plg_link_port_missing
            }
            .into(),
        ),
        (
            st.fl.then_some(st.listens),
            tr.plg_link_fl_listens,
            if !st.fl {
                "—"
            } else if st.listens {
                tr.plg_link_input_open
            } else {
                tr.plg_link_restart_fl
            }
            .into(),
        ),
        (
            Some(st.sink),
            tr.plg_link_sound,
            if st.sink {
                tr.plg_link_plays_here
            } else {
                tr.plg_link_no_sink
            }
            .into(),
        ),
        (
            st.stats
                .as_ref()
                .map(|s| s.silent_secs.is_some_and(|x| x < 5.0)),
            tr.plg_link_flow,
            flow,
        ),
        (
            st.stats.as_ref().map(|s| s.round_trip > 0),
            tr.plg_link_delay,
            delay,
        ),
        (
            st.stats.as_ref().map(|s| s.recent_underruns == 0),
            tr.plg_link_gaps,
            fill(
                tr.plg_link_gaps_val,
                &[
                    ("all", &stats.underruns.to_string()),
                    ("recent", &stats.recent_underruns.to_string()),
                    ("queued", &stats.queue.to_string()),
                ],
            ),
        ),
    ];
    egui::Grid::new("fl-link-rows")
        .num_columns(3)
        .spacing(egui::vec2(8.0, 4.0))
        .show(ui, |ui| {
            for (ok, what, text) in rows {
                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::hover());
                kit::led(
                    ui.painter(),
                    r.center(),
                    3.5,
                    ok.map(|ok| if ok { t::ok() } else { t::wait() }),
                );
                ui.label(RichText::new(what).font(t::sans(13.0)).color(t::text()));
                ui.label(RichText::new(text).font(t::sans(13.0)).color(t::muted()));
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let busy = matches!(progress, fl::Progress::Busy(_));
    ui.horizontal(|ui| {
        if kit::button(ui, tr.plg_open_fl, kit::Kind::Primary, !st.fl && !busy).clicked() {
            fl::run(tr.plg_starting_fl, || fl::open_fl(None));
        }
        if kit::button(ui, tr.plg_reconnect, kit::Kind::Ghost, !busy)
            .on_hover_text(tr.plg_reconnect_tip)
            .clicked()
        {
            fl::run(tr.plg_reconnecting, fl::reconnect);
        }
    });
    ui.add_space(10.0);
    // FLEX: what is known to make it start under Wine, checked in FL's prefix.
    let pf = &st.preflight;
    ui.label(RichText::new("FLEX").font(t::medium(13.0)).color(t::text()));
    let ready = pf.vkd3d && pf.verdana;
    ui.label(
        RichText::new(if ready {
            tr.plg_flex_ready
        } else {
            tr.plg_flex_needs
        })
        .font(t::sans(12.0))
        .color(if ready { t::muted() } else { t::wait() }),
    );
    ui.label(
        RichText::new(fill(
            tr.plg_flex_facts,
            &[
                ("vkd3d", if pf.vkd3d { tr.plg_yes } else { tr.plg_no }),
                ("verdana", if pf.verdana { tr.plg_yes } else { tr.plg_no }),
                (
                    "fl",
                    &match pf.fl {
                        0 => tr.plg_fl_not_found.to_string(),
                        v => format!("FL Studio {v}"),
                    },
                ),
                (
                    "wine",
                    pf.wine.as_deref().unwrap_or(tr.plg_system_wine_missing),
                ),
            ],
        ))
        .font(t::mono(11.0))
        .color(t::faint()),
    );
    if pf.needs_cert_patch() {
        ui.label(
            RichText::new(tr.plg_fl_cert_note)
                .font(t::sans(12.0))
                .color(t::wait()),
        );
    }
    ui.label(
        RichText::new(tr.plg_flex_not_vsti)
            .font(t::sans(12.0))
            .color(t::faint()),
    );
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr.plg_link_howto)
            .font(t::sans(12.0))
            .color(t::faint()),
    );
    ui.label(
        RichText::new(tr.plg_link_no_export)
            .font(t::sans(12.0))
            .color(t::faint()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_text_is_tidied() {
        assert_eq!(tidy_value("0.0000000"), "0");
        assert_eq!(tidy_value("-6.0200001 dB"), "-6.02 dB");
        assert_eq!(tidy_value("1.5000000"), "1.5");
        assert_eq!(tidy_value("Off"), "Off");
        assert_eq!(tidy_value("440 Hz"), "440 Hz");
    }
}
