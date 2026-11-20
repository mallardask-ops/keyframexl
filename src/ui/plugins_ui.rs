//! Plugins in the project: keep the loaded instances in step with the song,
//! hand them to the audio thread, open their editors, collect their state on
//! save, and the Plugins page of Settings (scan results, folders, rescan).

use super::{
    App, Tone, kit,
    settings_ui::{card, kit_switch, row},
};
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
        let name = self
            .plugins
            .live
            .get(&id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        self.say(
            Tone::Wait,
            format!("Reloading {name} from its last saved state…"),
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
        if let Some(rx) = &self.plugins.scan_rx {
            for msg in rx.try_iter().collect::<Vec<_>>() {
                match msg {
                    ScanMsg::Progress(d, n, name) => {
                        self.plugins.scan_note = Some(format!("Checking plugins {d}/{n}: {name}"))
                    }
                    ScanMsg::Done(list) => {
                        let ok = list.iter().filter(|s| s.usable()).count();
                        self.plugins.scan_note =
                            Some(format!("{ok} of {} plugins ready", list.len()));
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
                self.say(Tone::Wait, "Opening instruments in FL Studio…");
            }
        }
        let mut notice = None;
        if let Some(session) = &mut self.plugins.native_fl {
            for msg in session.rx.try_iter() {
                match msg {
                    Message::Ready(outputs) => {
                        notice = Some((
                            Tone::Done,
                            format!("FL Studio connected {} instrument channels.", outputs.len()),
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
                (
                    None,
                    Some("This FL Studio output already belongs to another channel.".into()),
                )
            } else if let Some(session) = &self.plugins.native_fl {
                (
                    session.outputs.get(index).cloned(),
                    session.error.clone().or_else(|| {
                        (index >= session.source.channels.len())
                            .then(|| "Invalid FL Studio output.".into())
                    }),
                )
            } else {
                (
                    None,
                    Some("This project has no saved state for the FL Studio instruments.".into()),
                )
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
                        format!("{name}: the audio thread is busy, try again."),
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
        let id = plugin.id;
        let page = self.plugins.page.get(&id).copied().unwrap_or_default();
        let (status, color) = self.plugin_status(id);
        let mut new_page = page;
        ui.horizontal(|ui| {
            let pages = [WrapPage::Plugin, WrapPage::Parameters, WrapPage::Settings];
            let (pick, _) = kit::chips(
                ui,
                &format!("wrap-page-{id}"),
                &["Plugin", "Parameters", "Settings"],
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
            ui.label(RichText::new(vendor).font(t::sans(12.0)).color(t::FAINT));
            // A failure reads short here; the whole story is in the tooltip.
            let failed = self.plugins.live.get(&id).and_then(|l| l.error.clone());
            match failed {
                Some(err) => {
                    let short = if err.contains("crashed") || err.contains("stopped responding") {
                        "crashed"
                    } else {
                        "failed"
                    };
                    ui.label(RichText::new(short).font(t::sans(12.0)).color(t::BAD))
                        .on_hover_text(&err);
                    if kit::tip(
                        kit::tool_button(ui, "Reload", kit::Kind::Normal, true),
                        format!("{err}\nLoad it again from the last saved state"),
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
                        if ui.button("Save channel preset…").clicked() {
                            ui.close_menu();
                            self.save_channel_preset(ch);
                        }
                        if ui.button("Load channel preset…").clicked() {
                            ui.close_menu();
                            self.load_preset_dialog_for(super::presets_ui::Target::Channel(ch));
                        }
                        ui.separator();
                    }
                    if let Some((t, k)) = slot {
                        if ui.button("Save effect preset…").clicked() {
                            ui.close_menu();
                            self.save_fx_preset(t, k);
                        }
                        if ui.button("Load effect preset…").clicked() {
                            ui.close_menu();
                            self.load_preset_dialog_for(super::presets_ui::Target::Fx(t, k));
                        }
                        ui.separator();
                    }
                    if let Some(ch) = channel {
                        if ui.button("Piano roll  F7").clicked() {
                            self.open_roll(ch);
                            ui.close_menu();
                        }
                        if ui.button("Delete channel").clicked() {
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
                            "Close separate window"
                        } else {
                            "Editor in a separate window"
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
                    if ui.button("Parameters").clicked() {
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
        let outs = self
            .song
            .instrument(ch)
            .and_then(|i| i.plugin())
            .map(|p| p.outs.clone())
            .unwrap_or_default();
        let routed = outs.iter().filter(|&&o| o > 0).count();
        let hint = "Outputs 1-2 play on the channel's insert. Each other pair plays on the insert set here; an unrouted pair joins 1-2. Choose which sound goes to which output in the plugin's own mixer.";
        row(ui, "Outputs", Some(hint), |ui| {
            ui.label(
                egui::RichText::new(format!("{} stereo · {routed} routed", extra + 1))
                    .font(t::mono(12.0))
                    .color(t::MUTED),
            );
            if kit::tool_button(ui, "Map to inserts", kit::Kind::Ghost, true)
                .on_hover_text(
                    "Give every output pair its own free insert, named after it (FL: auto map)",
                )
                .clicked()
            {
                self.checkpoint();
                let n = self.song.map_plugin_outputs(ch, extra);
                self.publish();
                self.say(
                    Tone::Done,
                    format!("Mapped {n} output pairs to inserts. Ctrl+Z undoes."),
                );
            }
            if routed > 0
                && kit::tool_button(ui, "Unroute", kit::Kind::Ghost, true)
                    .on_hover_text("Every pair back on the channel's insert")
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
                    "With 1-2".to_string()
                } else {
                    format!("{m:02}  {}", self.mixer_name(m))
                }
            })
            .collect();
        egui::CollapsingHeader::new(
            egui::RichText::new("Route each pair")
                .font(t::sans(12.0))
                .color(t::MUTED),
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
                                .color(t::TEXT),
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
        egui::ScrollArea::vertical().id_salt(("wrap-settings", id)).auto_shrink([false, false]).show(ui, |ui| {
            if let Some(ch) = channel {
                card(ui, "channel", |ui| {
                    row(ui, "Mixer insert", Some("Where this channel plays. Ctrl+L in the mixer routes the selected channels."), |ui| {
                        let now = self.song.instrument(ch).map(|i| i.mixer).unwrap_or(0);
                        let names: Vec<String> = (0..self.song.mixer.len()).map(|m| if m == 0 { "Master".to_string() } else { format!("{m:02}  {}", self.mixer_name(m)) }).collect();
                        let mut pick = None;
                        kit::dropdown(ui, &format!("wrap-insert-{id}"), &names[now as usize], 200.0, |ui| {
                            for (m, label) in names.iter().enumerate() {
                                if ui.selectable_label(now as usize == m, label).clicked() {
                                    pick = Some(m as u16);
                                }
                            }
                        });
                        if let Some(m) = pick
                            && m != now
                        {
                            self.checkpoint();
                            if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == ch) {
                                i.mixer = m;
                            }
                            self.publish();
                        }
                    });
                    let extra = self.plugins.live.get(&id).map(|l| l.outs).unwrap_or(0);
                    if extra > 0 {
                        self.output_rows(ui, ch, id, extra);
                    }
                    row(ui, "Volume and pan", None, |ui| {
                        let Some(inst) = self.song.instrument(ch).cloned() else { return };
                        let pan_fmt = |v: f64| if v.abs() < 0.005 { "C".to_string() } else if v < 0.0 { format!("L{:.0}", -v * 100.0) } else { format!("R{:.0}", v * 100.0) };
                        let vol_fmt = |pos: f64| keyframexl::sequence::gain_db_text(keyframexl::sequence::volume_gain(pos as f32));
                        let knobs = [
                            (kit::Knob { label: "Volume", color: t::ENC_ORANGE, min: 0.0, max: 1.0, default: keyframexl::sequence::volume_pos(1.0) as f64, log: false, fmt: &vol_fmt, hint: "Channel volume. Double-click for 0 dB." }, keyframexl::sequence::volume_pos(inst.volume) as f64, keyframexl::automation::Target::ChannelVolume { inst: ch }),
                            (kit::Knob { label: "Pan", color: t::ENC_WHITE, min: -1.0, max: 1.0, default: 0.0, log: false, fmt: &pan_fmt, hint: "Channel pan" }, inst.pan as f64, keyframexl::automation::Target::ChannelPan { inst: ch }),
                        ];
                        for (k, (spec, v, target)) in knobs.into_iter().enumerate() {
                            let mut v = v;
                            kit::automatable(ui, target);
                            let (_, started, changed) = kit::knob(ui, &spec, &mut v);
                            if started {
                                self.checkpoint();
                            }
                            if changed && let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == ch) {
                                if k == 0 {
                                    i.volume = keyframexl::sequence::volume_gain(v as f32);
                                } else {
                                    i.pan = v as f32;
                                }
                                self.publish();
                            }
                        }
                    });
                    row(ui, "Stuck notes", Some("All notes off for this plugin (MIDI CC 120 and 123)."), |ui| {
                        if kit::tool_button(ui, "Notes off", kit::Kind::Ghost, true).clicked()
                            && let Some(a) = &self.audio
                            && let Ok(mut q) = a.live.lock()
                        {
                            let _ = q.push(crate::audio::LiveCmd::Midi { inst: ch, bytes: [0xB0, 120, 0] });
                            let _ = q.push(crate::audio::LiveCmd::Midi { inst: ch, bytes: [0xB0, 123, 0] });
                        }
                    });
                });
            }
            if let Some((tr, k)) = slot {
                card(ui, "mixer slot", |ui| {
                    row(ui, "Place", None, |ui| {
                        ui.label(RichText::new(format!("{} · slot {}", self.mixer_name(tr), k + 1)).font(t::sans(13.0)).color(t::TEXT));
                    });
                    row(ui, "On", Some("Off bypasses the effect (the slot LED)."), |ui| {
                        let mut on = self.song.mixer[tr].fx[k].enabled;
                        if kit_switch(ui, &mut on) {
                            self.checkpoint();
                            self.song.mixer[tr].fx[k].enabled = on;
                            self.publish();
                        }
                    });
                    row(ui, "Mix", Some("Dry/wet of this slot."), |ui| {
                        let mut v = self.song.mixer[tr].fx[k].mix as f64;
                        let fmt = |v: f64| format!("{:.0}%", v * 100.0);
                        let spec = kit::Knob { label: "Mix", color: t::ENC_GREEN, min: 0.0, max: 1.0, default: 1.0, log: false, fmt: &fmt, hint: "Slot dry/wet" };
                        let (_, started, changed) = kit::knob(ui, &spec, &mut v);
                        if started {
                            self.checkpoint();
                        }
                        if changed {
                            self.song.mixer[tr].fx[k].mix = v as f32;
                            self.publish();
                        }
                    });
                });
            }
            card(ui, "processing", |ui| {
                let bridged = self.settings.plugins.bridged && std::env::var_os("KFXL_NO_BRIDGE").is_none();
                row(ui, "Process", None, |ui| {
                    let text = if bridged { "Own process (bridged): a crash silences only this plugin" } else { "Inside the program: a crash closes it" };
                    ui.label(RichText::new(text).font(t::sans(13.0)).color(if bridged { t::OK } else { t::WAIT }));
                });
                row(ui, "Latency", None, |ui| {
                    let ms = self.audio.as_ref().map(|a| a.running.latency_ms()).unwrap_or(0.0);
                    let text = if bridged { format!("One audio buffer, ≈{ms:.1} ms") } else { "None added".to_string() };
                    ui.label(RichText::new(text).font(t::sans(13.0)).color(t::TEXT));
                });
                ui.horizontal(|ui| {
                    if kit::tool_button(ui, "Change in Settings → Plugins", kit::Kind::Ghost, true).clicked() {
                        self.open_settings(super::settings_ui::Page::Plugins);
                    }
                });
            });
            card(ui, "state", |ui| {
                row(ui, "File", None, |ui| {
                    ui.label(RichText::new(&plugin.path).font(t::mono(11.5)).color(t::FAINT));
                });
                row(ui, "Saved state", Some("The plugin's settings are stored in the project when it is saved."), |ui| {
                    let size = plugin.state.len() * 3 / 4;
                    let text = if size == 0 { "none yet".to_string() } else if size < 1024 { format!("{size} bytes") } else { format!("{:.1} KB", size as f64 / 1024.0) };
                    ui.label(RichText::new(text).font(t::sans(13.0)).color(t::TEXT));
                });
                ui.horizontal(|ui| {
                    if kit::tool_button(ui, "Reload", kit::Kind::Ghost, true).on_hover_text("Load it again from the last saved state").clicked() {
                        self.reload_plugin(id);
                    }
                    if kit::tool_button(ui, "Reset to defaults", kit::Kind::Ghost, true).on_hover_text("Forget its settings and load it fresh. Ctrl+Z undoes.").clicked() {
                        self.checkpoint();
                        if let Some(r) = self.song.plugin_ref_mut(id) {
                            r.state.clear();
                        }
                        self.publish();
                        self.reload_plugin(id);
                        self.say(Tone::Quiet, format!("{} reset to its defaults. Ctrl+Z undoes.", plugin.name));
                    }
                });
            });
        });
    }

    fn embedded_editor(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(l) = self.plugins.live.get(&id) else {
            ui.label(
                RichText::new("Not loaded.")
                    .font(t::sans(13.0))
                    .color(t::FAINT),
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
                RichText::new("FLEX runs in standalone FL Studio.")
                    .font(t::sans(13.0))
                    .color(t::TEXT),
            );
            return;
        }
        if !l.gui {
            ui.label(RichText::new("This plugin has no editor of its own. Its controls are on the Parameters page.").font(t::sans(13.0)).color(t::MUTED));
            return;
        }
        let Some(parent) = self.plugins.parent else {
            ui.label(RichText::new("The editor opens in its own window here (Wayland). Settings → General → \"Run as an X11 window\" puts it inside this window, like FL.").font(t::sans(12.0)).color(t::FAINT));
            let open = l.editor;
            if kit::button(
                ui,
                if open { "Hide editor" } else { "Show editor" },
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
                    .hint_text("Filter parameters")
                    .desired_width(240.0),
            );
            let n = self.plugins.params.get(&id).map(|p| p.0.len()).unwrap_or(0);
            ui.label(
                RichText::new(format!("{n} parameters"))
                    .font(t::sans(12.0))
                    .color(t::FAINT),
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
                                let color = if r.automatable { t::TEXT } else { t::MUTED };
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
                                    &["Off", "On"],
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
                                        RichText::new(text).font(t::mono(12.0)).color(t::MUTED),
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
        let Some(l) = self.plugins.live.get_mut(&id) else {
            return;
        };
        if l.proc.is_none() {
            l.show_when_loaded = true;
            return;
        }
        if !l.gui {
            let name = l.name.clone();
            self.say(Tone::Quiet, format!("{name} has no editor window."));
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
                format!(
                    "\"{}\" already links to FL Studio. One link channel at a time: FL plays only its selected channel.",
                    other.name
                ),
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
        self.say(Tone::Wait, format!("Loading {}…", s.name));
    }

    /// Status line for a plugin channel or slot.
    pub(crate) fn plugin_status(&self, id: u64) -> (String, eframe::egui::Color32) {
        match self.plugins.live.get(&id) {
            None => ("not loaded".into(), t::FAINT),
            Some(l) if l.error.is_some() => (l.error.clone().unwrap_or_default(), t::BAD),
            Some(l) if l.proc.is_none() => ("loading…".into(), t::WAIT),
            Some(l) if l.editor => ("editor open".into(), t::OK),
            Some(_) => ("ready".into(), t::MUTED),
        }
    }

    /// Settings → Plugins (FL: Manage plugins): find, check and act on plugins.
    pub(crate) fn plugins_page(&mut self, ui: &mut egui::Ui, s: &mut Settings) {
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
        card(ui, "safety", |ui| {
            row(
                ui,
                "Bridged",
                Some(
                    "Each plugin runs in its own process. A crash silences it and offers Reload instead of closing the program. Costs one audio buffer of latency.",
                ),
                |ui| {
                    let mut on = s.plugins.bridged;
                    if kit_switch(ui, &mut on) {
                        s.plugins.bridged = on;
                    }
                },
            );
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
                    .hint_text("Search name or vendor")
                    .desired_width(190.0),
            );
            let kinds = ["All", "Instruments", "Effects", "Problems"];
            if let (Some(i), _) = kit::chips(ui, "plug-kind", &kinds, kind, None) {
                kind = i;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kit::tool_button(
                    ui,
                    if busy { "Scanning…" } else { "Rescan all" },
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
            format!(
                "{} found · {ready} ready · {problems} with problems",
                list.len()
            )
        };
        ui.label(RichText::new(note).font(t::sans(12.0)).color(if busy {
            t::WAIT
        } else {
            t::FAINT
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
        egui::Frame::new().fill(t::SUNKEN).stroke(egui::Stroke::new(1.0_f32, t::EDGE)).corner_radius(9.0).inner_margin(egui::Margin::symmetric(0, 4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width();
            let cols = [30.0, 0.40 * w, 0.14 * w, 0.24 * w];
            let x = |i: usize| cols[..i].iter().sum::<f32>();
            let (head, _) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::hover());
            for (i, h) in ["name", "type", "vendor", "status"].iter().enumerate() {
                kit::caps(ui.painter(), egui::pos2(head.left() + x(i + 1), head.center().y), egui::Align2::LEFT_CENTER, h, 10.5, t::FAINT);
            }
            if shown.is_empty() {
                ui.add_space(8.0);
                let text = if list.is_empty() { "No plugins yet. Windows plugins (e.g. SSD5) need yabridge: yabridgectl add <folder>, then yabridgectl sync. Then Rescan all, or add the folder below." } else { "Nothing matches." };
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(text).font(t::sans(13.0)).color(t::FAINT));
                });
                ui.add_space(8.0);
            }
            egui::ScrollArea::vertical().id_salt("plug-table").max_height(210.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (n, p) in shown.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
                    let on = picked.as_ref() == Some(&p.path);
                    let fill = if on { t::SELECTION } else if resp.hovered() { t::RAISED } else if n % 2 == 1 { t::SIDE } else { t::SUNKEN };
                    let pt = ui.painter();
                    pt.rect_filled(r, 0.0, fill);
                    let off = hidden(p);
                    let ink = if off || !p.usable() { t::FAINT } else { t::TEXT };
                    let y = r.center().y;
                    let (status, color) = match &p.status {
                        Status::Ok if off => ("hidden from lists".to_string(), t::FAINT),
                        Status::Ok => ("ready".to_string(), t::OK),
                        Status::Hangs => ("hangs, not loaded".to_string(), t::BAD),
                        Status::Crashes => ("crashes, not loaded".to_string(), t::BAD),
                        Status::Failed(e) => (e.clone(), t::BAD),
                    };
                    kit::led(pt, egui::pos2(r.left() + 16.0, y), 3.5, Some(color));
                    let font = t::sans(13.0);
                    let cell = |i: usize| r.left() + x(i);
                    pt.text(egui::pos2(cell(1), y), egui::Align2::LEFT_CENTER, kit::elide(pt, &p.name, font.clone(), cols[1] - 10.0), font.clone(), ink);
                    pt.text(egui::pos2(cell(2), y), egui::Align2::LEFT_CENTER, format!("{} · {}", if p.instrument { "Instrument" } else { "Effect" }, crate::plugins::format_of(&p.path).to_uppercase()), t::sans(12.0), t::MUTED);
                    pt.text(egui::pos2(cell(3), y), egui::Align2::LEFT_CENTER, kit::elide(pt, &p.vendor, t::sans(12.0), cols[3] - 10.0), t::sans(12.0), t::MUTED);
                    pt.text(egui::pos2(cell(4), y), egui::Align2::LEFT_CENTER, kit::elide(pt, &status, t::sans(12.0), w - x(4) - 12.0), t::sans(12.0), color);
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
            card(ui, "selected", |ui| {
                ui.label(RichText::new(&p.name).font(t::medium(16.0)).color(t::TEXT));
                let mut facts = vec![
                    if p.vendor.is_empty() {
                        "unknown vendor".to_string()
                    } else {
                        p.vendor.clone()
                    },
                    if p.instrument {
                        "Instrument".into()
                    } else {
                        "Effect".into()
                    },
                    crate::plugins::format_of(&p.path).to_uppercase(),
                ];
                if !p.category.is_empty() {
                    facts.push(p.category.clone());
                }
                if p.bridged {
                    facts.push("Windows plugin via yabridge".into());
                }
                if !p.gui {
                    facts.push("no editor of its own".into());
                }
                ui.label(
                    RichText::new(facts.join(" · "))
                        .font(t::sans(12.5))
                        .color(t::MUTED),
                );
                ui.label(
                    RichText::new(p.path.display().to_string())
                        .font(t::mono(11.5))
                        .color(t::FAINT),
                );
                let off = hidden(&p);
                let this_busy = single_busy.as_ref() == Some(&p.path);
                ui.horizontal_wrapped(|ui| {
                    if p.instrument {
                        if kit::tool_button(ui, "Add to channel rack", kit::Kind::Primary, p.usable()).clicked() {
                            self.add_plugin_channel(&p);
                        }
                    } else {
                        let t_ = self.mix_sel;
                        let label = format!("Add to {}", self.mixer_name(t_));
                        if kit::tool_button(ui, &label, kit::Kind::Primary, p.usable()).clicked() {
                            let k = self.song.mixer.get(t_).map(|m| m.fx.len()).unwrap_or(0);
                            self.set_slot_plugin(t_, k, &p);
                        }
                    }
                    if kit::tool_button(ui, "Show in folder", kit::Kind::Ghost, true).clicked() {
                        let dir = p.path.parent().map(PathBuf::from).unwrap_or_else(|| p.path.clone());
                        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                    }
                    if kit::tool_button(ui, if this_busy { "Probing…" } else { "Rescan this plugin" }, kit::Kind::Ghost, !this_busy && !busy)
                        .on_hover_text("Probe just this bundle again (up to 90 s) without rescanning everything.")
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
                    if kit::tool_button(ui, if off { "Show in lists" } else { "Hide from lists" }, kit::Kind::Ghost, true)
                        .on_hover_text("Hidden plugins stay installed but are not offered in the browser, the + menu or F8.")
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
                    ui.label(RichText::new("It failed the test load, so the browser hides it. If you just installed it (e.g. SSD5 via yabridge sync), press Rescan this plugin. You can also try loading it anyway:").font(t::sans(12.0)).color(t::WAIT));
                    ui.horizontal_wrapped(|ui| {
                        let label = if p.instrument {
                            "Try adding to channel rack anyway".to_string()
                        } else {
                            format!("Try adding to {} anyway", self.mixer_name(self.mix_sel))
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
                    ui.label(RichText::new("If it is missing from the list entirely, it is not under any folder below. Add its folder below, then Rescan all. Check with: keyframexl plugin-scan").font(t::sans(12.0)).color(t::FAINT));
                }
            });
        }
        card(ui, "windows plugins", |ui| {
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
                        .unwrap_or_else(|| "System Wine".into())
                });
            row(
                ui,
                "Wine",
                Some(
                    "yabridge runs Windows plugins with this Wine. Some plugins (SSD5 and others) behave better on Wine Staging or Proton builds. All use the same prefix (~/.wine); a newer Wine updates it.",
                ),
                |ui| {
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
                                            .unwrap_or_else(|| "wine from PATH".into()),
                                    )
                                    .clicked()
                                {
                                    s.plugins.wine = bin.clone();
                                }
                            }
                        });
                    if kit::tool_button(ui, "Other…", kit::Kind::Ghost, true)
                        .on_hover_text("Pick a Wine bin folder (it must contain wine)")
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
                            self.say(
                                Tone::Bad,
                                "No wine in that folder (looked in it, bin/ and files/bin/).",
                            );
                        }
                    }
                },
            );
            if s.plugins
                .wine
                .as_deref()
                .is_some_and(crate::plugins::has_dxvk)
            {
                row(
                    ui,
                    "DXVK",
                    Some(
                        "Direct3D through Vulkan from this build, for plugin windows. JUCE 8 plugins (SSD5 and others) only redraw with it: without it they draw once and clicks seem to do nothing.",
                    ),
                    |ui| {
                        kit_switch(ui, &mut s.plugins.dxvk);
                    },
                );
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
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, Some(t::BAD));
                    ui.label(
                        RichText::new(format!("Another Wine is running on {prefix} ({}). Plugins cannot start with the chosen Wine until it stops.", exe.display()))
                            .font(t::sans(12.5))
                            .color(t::TEXT),
                    );
                    if kit::tool_button(ui, "Stop it", kit::Kind::Normal, true)
                        .on_hover_text("Runs its wineserver -k: Windows programs using this prefix close. Then reload the plugins.")
                        .clicked()
                    {
                        match crate::plugins::stop_foreign_wineserver(pid, &exe) {
                            Ok(()) => {
                                ui.data_mut(|d| d.remove::<(f64, Option<(u32, PathBuf)>)>(ckey));
                                let bin = s.plugins.wine.clone();
                                crate::plugins::set_wine(bin);
                                self.restart_plugin_host(ui.ctx());
                                self.say(Tone::Quiet, "The other Wine stopped. Plugins restart with the chosen Wine.");
                            }
                            Err(e) => self.say(Tone::Bad, format!("Could not stop the other Wine: {e}")),
                        }
                    }
                });
            }
        });
        card(ui, "folders", |ui| {
            let mut std_dirs = crate::plugins::search_dirs(&[]);
            std_dirs.extend(crate::plugins::vst2_dirs());
            std_dirs.extend(crate::plugins::clap_dirs());
            std_dirs.dedup();
            for d in &std_dirs {
                ui.horizontal(|ui| {
                    let there = d.is_dir();
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, there.then_some(t::OK));
                    ui.label(
                        RichText::new(d.display().to_string())
                            .font(t::mono(12.0))
                            .color(if there { t::TEXT } else { t::FAINT }),
                    );
                    if !there {
                        ui.label(RichText::new("missing").font(t::sans(11.5)).color(t::FAINT));
                    }
                });
            }
            let mut remove = None;
            for (i, d) in s.plugins.extra_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    kit::led(ui.painter(), r.center(), 3.0, d.is_dir().then_some(t::OK));
                    ui.label(
                        RichText::new(d.display().to_string())
                            .font(t::mono(12.0))
                            .color(t::TEXT),
                    );
                    if kit::tool_button(ui, "Remove", kit::Kind::Ghost, true).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.plugins.extra_dirs.remove(i);
            }
            ui.horizontal(|ui| {
                if kit::tool_button(ui, "Add folder…", kit::Kind::Ghost, true).clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                    && !s.plugins.extra_dirs.contains(&d)
                {
                    s.plugins.extra_dirs.push(d);
                }
                ui.label(RichText::new("Also $VST3_PATH, $VST_PATH, and $CLAP_PATH. yabridge puts Windows plugins in ~/.vst3/yabridge (VST3) and ~/.vst/yabridge (VST2).").font(t::sans(12.0)).color(t::FAINT));
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
            format!(
                "No saved {}s for {} yet: ☰ → Save",
                kind.label().to_lowercase(),
                plugin.name
            )
        } else {
            format!("{n} saved preset(s) of {}", plugin.name)
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
                    "No presets".into()
                } else {
                    "Preset".into()
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
            if cur.is_some() { t::TEXT } else { t::FAINT },
        );
        kit::tip(resp, tip);
        let next = kit::tool_button(ui, "▶", kit::Kind::Ghost, n > 0);
        let step = if kit::tip(prev, "Previous preset").clicked() {
            Some(cur.map(|i| (i + n - 1) % n).unwrap_or(n - 1))
        } else if kit::tip(next, "Next preset").clicked() {
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
    use crate::fl_link::{self as fl, LinkState};
    let st = fl::status();
    let progress = fl::progress();
    let state = fl::state(&st, &progress);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(500));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("FL Studio (live link)")
                .font(t::medium(15.0))
                .color(t::TEXT),
        );
        let (word, color) = match &state {
            LinkState::Off => ("Not running", None),
            LinkState::Starting(_) => ("Starting", Some(t::WAIT)),
            LinkState::Connected => ("Connected", Some(t::OK)),
            LinkState::Degraded(_) => ("Check", Some(t::WAIT)),
            LinkState::Error(_) => ("Error", Some(t::BAD)),
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(word).font(t::sans(13.0)).color(t::TEXT));
            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::hover());
            kit::led(ui.painter(), r.center(), 4.0, color);
        });
    });
    ui.label(RichText::new("This channel plays FL Studio's selected channel: its notes go to FL, FL's sound comes back here. Use it for FLEX and other Image-Line instruments that do not work inside FL Studio VSTi.").font(t::sans(13.0)).color(t::MUTED));
    match &state {
        LinkState::Starting(m) | LinkState::Degraded(m) => {
            ui.label(RichText::new(m).font(t::sans(13.0)).color(t::WAIT));
        }
        LinkState::Error(m) => {
            ui.label(RichText::new(m).font(t::sans(13.0)).color(t::BAD));
        }
        _ => {}
    }
    ui.add_space(8.0);
    let stats = st.stats.clone().unwrap_or_default();
    let delay = match stats.round_trip_ms() {
        Some(ms) => format!("{ms:.0} ms ({} frames), compensated", stats.round_trip),
        None => "not measured yet: play a note".into(),
    };
    let flow = match stats.silent_secs {
        _ if st.stats.is_none() => "the link channel is not running".to_string(),
        None => "no sound from FL yet".to_string(),
        Some(s) if s < 1.0 => "FL's sound is coming in".to_string(),
        Some(s) => format!("last sound {s:.0} s ago"),
    };
    let rows: [(Option<bool>, &str, String); 7] = [
        (
            Some(st.fl),
            "FL Studio",
            if st.fl { "running" } else { "not running" }.into(),
        ),
        (
            Some(st.port),
            "Notes",
            if st.port {
                "port \"KeyFrameXL - FL link\" ready"
            } else {
                "port missing (reload this channel)"
            }
            .into(),
        ),
        (
            st.fl.then_some(st.listens),
            "FL listens",
            if !st.fl {
                "—"
            } else if st.listens {
                "FL has the input open"
            } else {
                "not yet: restart FL Studio"
            }
            .into(),
        ),
        (
            Some(st.sink),
            "Sound",
            if st.sink {
                "FL plays into this channel"
            } else {
                "no link sink (Reconnect makes it)"
            }
            .into(),
        ),
        (
            st.stats
                .as_ref()
                .map(|s| s.silent_secs.is_some_and(|x| x < 5.0)),
            "Flow",
            flow,
        ),
        (st.stats.as_ref().map(|s| s.round_trip > 0), "Delay", delay),
        (
            st.stats.as_ref().map(|s| s.recent_underruns == 0),
            "Gaps",
            format!(
                "{} in all, {} lately · {} frames queued",
                stats.underruns, stats.recent_underruns, stats.queue
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
                    ok.map(|ok| if ok { t::OK } else { t::WAIT }),
                );
                ui.label(RichText::new(what).font(t::sans(13.0)).color(t::TEXT));
                ui.label(RichText::new(text).font(t::sans(13.0)).color(t::MUTED));
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let busy = matches!(progress, fl::Progress::Busy(_));
    ui.horizontal(|ui| {
        if kit::button(ui, "Open FL Studio", kit::Kind::Primary, !st.fl && !busy).clicked() {
            fl::run("Starting FL Studio…", || fl::open_fl(None));
        }
        if kit::button(ui, "Reconnect", kit::Kind::Ghost, !busy)
            .on_hover_text("Make the port and the sound sink again and move FL's sound onto it")
            .clicked()
        {
            fl::run("Reconnecting…", fl::reconnect);
        }
    });
    ui.add_space(10.0);
    // FLEX: what is known to make it start under Wine, checked in FL's prefix.
    let pf = &st.preflight;
    ui.label(RichText::new("FLEX").font(t::medium(13.0)).color(t::TEXT));
    let ready = pf.vkd3d && pf.verdana;
    ui.label(
        RichText::new(if ready {
            "FL's Wine prefix has vkd3d-proton and Verdana, which FLEX is reported to need."
        } else {
            "FLEX is reported to start under Wine only when FL's prefix has vkd3d-proton (its d3d12.dll in system32) and the Verdana font; without them it crashes while loading. Nothing is installed for you."
        })
        .font(t::sans(12.0))
        .color(if ready { t::MUTED } else { t::WAIT }),
    );
    ui.label(
        RichText::new(format!(
            "vkd3d-proton: {} · Verdana: {} · {} · {}",
            if pf.vkd3d { "yes" } else { "no" },
            if pf.verdana { "yes" } else { "no" },
            match pf.fl {
                0 => "FL Studio not found".to_string(),
                v => format!("FL Studio {v}"),
            },
            pf.wine.as_deref().unwrap_or("system Wine not found"),
        ))
        .font(t::mono(11.0))
        .color(t::FAINT),
    );
    if pf.needs_cert_patch() {
        ui.label(RichText::new("FL Studio 2026.1.4 and later do not start on stock Wine (\"The validity of the program could not be verified\"): use a Wine with merge request 11824, such as DAW-GE-Proton11-6c, or FL 2026.1.3.").font(t::sans(12.0)).color(t::WAIT));
    }
    ui.label(
        RichText::new("FLEX does not run inside FL Studio VSTi; this link is the way to use it.")
            .font(t::sans(12.0))
            .color(t::FAINT),
    );
    ui.add_space(8.0);
    ui.label(RichText::new("In FL: add the instrument to the Channel rack and keep its channel selected. FL hears this channel's notes on its MIDI input \"KeyFrameXL - FL link\" (switched on for you). The input stays while KeyFrameXL runs; restart FL only if it was opened before the first FL link channel of this session. One link channel at a time: FL plays only its selected channel.").font(t::sans(12.0)).color(t::FAINT));
    ui.label(
        RichText::new("Not in WAV export yet: FL cannot render faster than real time.")
            .font(t::sans(12.0))
            .color(t::FAINT),
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
