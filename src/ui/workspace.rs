//! Window management: every editor is a tab that can be docked (tabs or
//! splits) or torn off into a floating window. Three workspace presets keep
//! their own layout; the set is saved between runs.

use super::App;
use crate::theme as t;
use eframe::egui::{self, CornerRadius, Margin, Stroke};
use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabIndex};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Tab {
    Playlist,
    Rack,
    Roll {
        instrument: u64,
    },
    /// Channel settings / instrument window.
    Instrument {
        id: u64,
    },
    Mixer,
    /// Tool windows: live in the dock while their session is open, like
    /// plugin windows, and are never saved into the layout.
    Riff,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Playlist,
    Rack,
    Roll,
    Instrument,
    Mixer,
    Riff,
    Settings,
}

impl Tab {
    pub fn kind(&self) -> Kind {
        match self {
            Tab::Playlist => Kind::Playlist,
            Tab::Rack => Kind::Rack,
            Tab::Roll { .. } => Kind::Roll,
            Tab::Instrument { .. } => Kind::Instrument,
            Tab::Mixer => Kind::Mixer,
            Tab::Riff => Kind::Riff,
            Tab::Settings => Kind::Settings,
        }
    }
}

impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Kind::Playlist => "Playlist",
            Kind::Rack => "Channel rack",
            Kind::Roll => "Piano roll",
            Kind::Instrument => "Instrument",
            Kind::Mixer => "Mixer",
            Kind::Riff => "Riff Machine",
            Kind::Settings => "Settings",
        }
    }
}

pub const PRESETS: [&str; 3] = ["Arrange", "Compose", "Edit"];

/// Default layout of a preset. `roll` is the instrument a piano roll opens for.
pub fn preset_layout(preset: usize, roll: u64) -> DockState<Tab> {
    let roll = Tab::Roll { instrument: roll };
    match preset {
        // Song first: playlist on top, the rack under it.
        0 => {
            let mut d = DockState::new(vec![Tab::Playlist]);
            d.main_surface_mut()
                .split_below(NodeIndex::root(), 0.48, vec![Tab::Rack]);
            d
        }
        // Pattern first: rack on top, piano roll under it, playlist behind the rack.
        1 => {
            let mut d = DockState::new(vec![Tab::Rack, Tab::Playlist]);
            d.main_surface_mut()
                .split_below(NodeIndex::root(), 0.5, vec![roll]);
            d
        }
        // Notes first: a big piano roll, rack and playlist below it.
        _ => {
            let mut d = DockState::new(vec![roll]);
            d.main_surface_mut().split_below(
                NodeIndex::root(),
                0.68,
                vec![Tab::Rack, Tab::Playlist],
            );
            d
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Workspace {
    pub active: usize,
    pub slots: Vec<DockState<Tab>>,
    /// Interface size (egui zoom factor), Ctrl+= / Ctrl+- / Ctrl+0.
    #[serde(default = "unit")]
    pub zoom: f32,
    /// Browser column shown (Alt+F8).
    #[serde(default = "yes")]
    pub browser: bool,
}

fn unit() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

impl Workspace {
    pub fn new(roll: u64) -> Self {
        Self {
            active: 0,
            slots: (0..PRESETS.len()).map(|i| preset_layout(i, roll)).collect(),
            zoom: 1.0,
            browser: true,
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        Some(dirs::config_dir()?.join("keyframexl").join("workspace.ron"))
    }

    pub fn load(roll: u64) -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| ron::from_str::<Workspace>(&s).ok())
            .filter(|w| w.slots.len() == PRESETS.len() && w.active < PRESETS.len())
            .map(|mut w| {
                for d in &mut w.slots {
                    strip_transient(d);
                }
                w
            })
            .unwrap_or_else(|| Self::new(roll))
    }

    /// RON, not JSON: dock rects are infinite until first drawn, JSON cannot hold that.
    pub fn to_text(&self) -> String {
        // Tool windows are sessions, not layout: never persist them.
        let mut copy = Workspace {
            active: self.active,
            slots: self.slots.clone(),
            zoom: self.zoom,
            browser: self.browser,
        };
        for d in &mut copy.slots {
            strip_transient(d);
        }
        ron::to_string(&copy).unwrap_or_default()
    }

    pub fn save(text: &str) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn dock(&mut self) -> &mut DockState<Tab> {
        &mut self.slots[self.active]
    }

    pub fn dock_ref(&self) -> &DockState<Tab> {
        &self.slots[self.active]
    }
}

/// Window commands, applied after the dock area has drawn this frame.
#[derive(Clone, Debug)]
pub enum Cmd {
    /// Open if closed, then bring to front and focus.
    Show(Tab),
    /// FL F-key behaviour: close if it is the focused tab, otherwise show.
    Toggle(Tab),
    /// Open (or move) into a floating window.
    Float(Tab),
    Close(Tab),
    /// Bring a floating window back into the main dock.
    Dock(Tab),
    Preset(usize),
    Reset,
    /// Resize a floating window by this much (a plugin editor fits it; may shrink).
    Grow(Tab, egui::Vec2),
}

fn find(d: &DockState<Tab>, tab: &Tab) -> Option<(SurfaceIndex, NodeIndex, TabIndex)> {
    d.find_tab(tab)
}

/// Drop session tabs (Riff Machine, Settings) from a layout.
fn strip_transient(d: &mut DockState<Tab>) {
    let gone: Vec<Tab> = d
        .iter_all_tabs()
        .filter(|&(_, t)| matches!(t, Tab::Riff | Tab::Settings))
        .map(|(_, t)| t.clone())
        .collect();
    for t in gone {
        if let Some(at) = find(d, &t) {
            d.remove_tab(at);
        }
    }
}

fn find_kind(d: &DockState<Tab>, kind: Kind) -> Option<(SurfaceIndex, NodeIndex)> {
    d.iter_all_tabs()
        .find(|(_, t)| t.kind() == kind)
        .map(|(at, _)| at)
}

fn focus(d: &mut DockState<Tab>, at: (SurfaceIndex, NodeIndex, TabIndex)) {
    d.set_active_tab(at);
    d.set_focused_node_and_surface((at.0, at.1));
}

/// Put a tab where a user expects it: next to its relatives, else a sensible split.
fn place(d: &mut DockState<Tab>, tab: Tab) {
    let sibling = match tab.kind() {
        Kind::Roll => find_kind(d, Kind::Roll).or_else(|| find_kind(d, Kind::Rack)),
        Kind::Rack => find_kind(d, Kind::Roll).or_else(|| find_kind(d, Kind::Mixer)),
        // Beside the rack and roll: the playlist above stays in view.
        Kind::Mixer => find_kind(d, Kind::Rack).or_else(|| find_kind(d, Kind::Roll)),
        // Reopening a closed Playlist lands in the currently focused main pane
        // (instead of always jumping to the root / first pane).
        Kind::Playlist => d.focused_leaf().filter(|(s, _)| s.is_main()),
        Kind::Instrument | Kind::Riff | Kind::Settings => None,
    };
    if let Some((s, n)) = sibling {
        d[s][n].append_tab(tab.clone());
    } else if d.main_surface().is_empty() {
        d.main_surface_mut().push_to_first_leaf(tab.clone());
    } else if tab.kind() == Kind::Playlist {
        d.main_surface_mut()
            .split_above(NodeIndex::root(), 0.44, vec![tab.clone()]);
    } else if tab.kind() == Kind::Mixer {
        // Docked along the bottom: it shows next to the work instead of over it.
        d.main_surface_mut()
            .split_below(NodeIndex::root(), 0.5, vec![tab.clone()]);
    } else {
        d.main_surface_mut()
            .split_below(NodeIndex::root(), 0.56, vec![tab.clone()]);
    }
    if let Some(at) = find(d, &tab) {
        focus(d, at);
    }
}

pub fn focused(d: &mut DockState<Tab>) -> Option<Tab> {
    d.find_active_focused().map(|(_, t)| t.clone())
}

impl App {
    pub(crate) fn win(&mut self, cmd: Cmd) {
        self.win_cmds.push(cmd);
    }

    pub(crate) fn apply_win_cmds(&mut self, area: egui::Rect) {
        for cmd in std::mem::take(&mut self.win_cmds) {
            match cmd {
                Cmd::Show(tab) => {
                    let d = self.ws.dock();
                    match find(d, &tab) {
                        Some(at) => focus(d, at),
                        // Instrument and tool windows float, like plugin windows in FL.
                        None if matches!(
                            tab.kind(),
                            Kind::Instrument | Kind::Riff | Kind::Settings
                        ) =>
                        {
                            self.win_cmds_float(tab, area)
                        }
                        None => place(d, tab),
                    }
                }
                Cmd::Toggle(tab) => {
                    let d = self.ws.dock();
                    let is_focused = focused(d).is_some_and(|f| f == tab);
                    match find(d, &tab) {
                        Some(at) if is_focused => {
                            d.remove_tab(at);
                        }
                        Some(at) => focus(d, at),
                        None => place(d, tab),
                    }
                }
                Cmd::Float(tab) => self.win_cmds_float(tab, area),
                Cmd::Grow(tab, by) => {
                    let d = self.ws.dock();
                    if let Some((s, _, _)) = find(d, &tab)
                        && !s.is_main()
                        && let Some(w) = d.get_window_state_mut(s)
                    {
                        // Before its first frame a window has no rect yet: start from the default.
                        let key = format!("{tab:?}");
                        let r = match self.plugins.grown_rect.get(&key) {
                            Some(r) => *r,
                            None => w.rect(),
                        };
                        let r = if r.is_finite() && r.is_positive() {
                            r
                        } else {
                            egui::Rect::from_center_size(
                                area.center(),
                                egui::vec2((area.width() * 0.86).min(1040.0), area.height() * 0.92),
                            )
                        };
                        let size = (r.size() + by)
                            .max(egui::vec2(360.0, 240.0))
                            .min(area.size());
                        // Keep it on screen: move up/left when it would run off.
                        let pos = egui::pos2(
                            r.left().min(area.right() - size.x).max(area.left()),
                            r.top().min(area.bottom() - size.y).max(area.top()),
                        );
                        w.set_position(pos).set_size(size);
                        self.plugins
                            .grown_rect
                            .insert(key, egui::Rect::from_min_size(pos, size));
                    }
                }
                Cmd::Close(tab) => {
                    let d = self.ws.dock();
                    if let Some(at) = find(d, &tab) {
                        d.remove_tab(at);
                    }
                }
                Cmd::Dock(tab) => {
                    let d = self.ws.dock();
                    if let Some(at) = find(d, &tab) {
                        d.remove_tab(at);
                    }
                    place(d, tab);
                }
                Cmd::Preset(i) => {
                    self.ws.active = i.min(PRESETS.len() - 1);
                    self.say(
                        super::Tone::Quiet,
                        format!("Workspace {}", PRESETS[self.ws.active]),
                    );
                }
                Cmd::Reset => {
                    let a = self.ws.active;
                    self.ws.slots[a] = preset_layout(a, self.default_roll());
                    self.say(super::Tone::Quiet, format!("{} layout reset", PRESETS[a]));
                }
            }
        }
    }

    /// Riff Machine and Settings are tabs while their session lives: closing
    /// the window ends the session, ending the session closes the window.
    pub(crate) fn sync_tool_tabs(&mut self) {
        let d = self.ws.dock();
        let riff_tab = d.iter_all_tabs().any(|(_, t)| *t == Tab::Riff);
        let settings_tab = d.iter_all_tabs().any(|(_, t)| *t == Tab::Settings);
        if self.riff.is_some() != riff_tab {
            if riff_tab {
                self.win(Cmd::Close(Tab::Riff));
            } else if let Some(mut s) = self.riff.take() {
                self.end_riff_preview(&mut s);
            }
        }
        if self.settings_ui.is_some() != settings_tab {
            if settings_tab {
                self.win(Cmd::Close(Tab::Settings));
            } else {
                self.settings_ui = None;
            }
        }
    }

    fn win_cmds_float(&mut self, tab: Tab, area: egui::Rect) {
        let tab_kind = tab.kind();
        // Content size for the rack before the dock is borrowed.
        let rack_size = if tab_kind == Kind::Rack {
            let steps = self.pattern().step_count().max(16);
            let beats = steps.div_ceil(4).max(1) as f32;
            let min_tray = 4.0 * 22.0 + 4.0 * 4.0;
            let max_tray = 4.0 * 34.0 + 4.0 * 4.0;
            let avail = (area.width() - 230.0 - 24.0).max(min_tray);
            let tray_w = ((avail / beats).clamp(min_tray, max_tray)).min(180.0);
            let want_w =
                (230.0 + beats * tray_w + 48.0).clamp(560.0, (area.width() * 0.94).max(560.0));
            let channels = self.song.instruments.len().max(1) as f32;
            let want_h = (32.0 + 26.0 + 14.0 + 18.0 + channels * 30.0 + 64.0 + 40.0)
                .clamp(320.0, (area.height() * 0.86).max(320.0));
            Some(egui::vec2(
                want_w.min(area.width()),
                want_h.min(area.height()),
            ))
        } else {
            None
        };
        let d = self.ws.dock();
        if let Some(at) = find(d, &tab) {
            d.remove_tab(at);
        }
        // Cascade new windows so they do not hide each other exactly.
        let n = d.surfaces_count().saturating_sub(1) as f32;
        // Instrument windows hold a full editor: give them most of the work area.
        let big = tab_kind == Kind::Instrument;
        let s = d.add_window(vec![tab]);
        let size = if tab_kind == Kind::Mixer {
            egui::vec2(
                area.width() * 0.94,
                (area.height() * 0.62).max(420.0).min(area.height()),
            )
        } else if tab_kind == Kind::Riff {
            egui::vec2(852.0f32.min(area.width()), 640.0f32.min(area.height()))
        } else if tab_kind == Kind::Settings {
            egui::vec2(
                920.0f32.min((area.width() - 24.0).max(600.0)),
                680.0f32.min((area.height() - 24.0).max(480.0)),
            )
        } else if big {
            egui::vec2((area.width() * 0.86).min(1040.0), area.height() * 0.92)
        } else if let Some(rs) = rack_size {
            // The rack sizes to its content: wide enough for the steps, tall
            // enough for the channels, but always inside the work area. A fixed
            // 0.62×0.66 window either clips a long pattern or leaves a short one
            // swimming in empty space, and the outer scroll then fights the pads.
            rs
        } else {
            egui::vec2(
                (area.width() * 0.62).min(900.0),
                (area.height() * 0.66).min(540.0),
            )
        };
        if let Some(w) = d.get_window_state_mut(s) {
            w.set_position(area.center() - size / 2.0 + egui::vec2(24.0, 24.0) * (n % 6.0))
                .set_size(size);
        }
        d.set_focused_node_and_surface((s, NodeIndex::root()));
    }

    /// Instrument a piano roll opens for when none is named.
    pub(crate) fn default_roll(&self) -> u64 {
        self.song
            .instruments
            .iter()
            .find(|i| i.is_sampler())
            .or(self.song.instruments.first())
            .map(|i| i.id)
            .unwrap_or(super::BASS)
    }

    /// Drop piano rolls of instruments that no longer exist (after Open / Undo),
    /// and any second copy of a tab: one window per editor and channel.
    pub(crate) fn prune_tabs(&mut self) {
        let ids: Vec<u64> = self.song.instruments.iter().map(|i| i.id).collect();
        for d in &mut self.ws.slots {
            let mut seen: Vec<Tab> = Vec::new();
            let mut dupes: Vec<(SurfaceIndex, NodeIndex, TabIndex)> = Vec::new();
            let mut last: Option<(SurfaceIndex, NodeIndex)> = None;
            let mut index = 0;
            for (at, tab) in d.iter_all_tabs() {
                // Tabs come node by node, in order: count the position inside the node.
                index = if last == Some(at) { index + 1 } else { 0 };
                last = Some(at);
                if seen.contains(tab) {
                    dupes.push((at.0, at.1, TabIndex(index)));
                } else {
                    seen.push(tab.clone());
                }
            }
            // Remove from the back so earlier indices stay valid.
            for at in dupes.into_iter().rev() {
                d.remove_tab(at);
            }
            let stale: Vec<Tab> = d
                .iter_all_tabs()
                .filter_map(|(_, t)| match t {
                    Tab::Roll { instrument } | Tab::Instrument { id: instrument }
                        if !ids.contains(instrument) =>
                    {
                        Some(t.clone())
                    }
                    _ => None,
                })
                .collect();
            for tab in stale {
                if let Some(at) = d.find_tab(&tab) {
                    d.remove_tab(at);
                }
            }
        }
    }

    /// Persist the layout at most once a second, only when it changed.
    pub(crate) fn autosave_layout(&mut self, now: f64) {
        if !self.persist || now - self.layout_saved_at < 1.0 {
            return;
        }
        self.layout_saved_at = now;
        let text = self.ws.to_text();
        if text != self.layout_text {
            if let Err(e) = Workspace::save(&text) {
                self.say(super::Tone::Bad, format!("Layout not saved: {e}"));
            }
            self.layout_text = text;
        }
    }

    pub(crate) fn instrument_name(&self, id: u64) -> String {
        self.song
            .instruments
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "Sampler".into())
    }
}

pub struct Viewer<'a> {
    pub app: &'a mut App,
}

impl egui_dock::TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Roll { instrument } => {
                format!("Piano roll · {}", self.app.instrument_name(*instrument)).into()
            }
            Tab::Instrument { id } => {
                let kind = match self.app.song.instrument(*id).map(|i| &i.kind) {
                    Some(keyframexl::sequence::InstrumentKind::Sampler { .. }) => "Sampler",
                    Some(keyframexl::sequence::InstrumentKind::Plugin { .. }) => "Plugin",
                    Some(keyframexl::sequence::InstrumentKind::Synth { .. }) => "Synth",
                    Some(keyframexl::sequence::InstrumentKind::Vocal { .. }) => "Vocal",
                    Some(keyframexl::sequence::InstrumentKind::Forge { .. }) => "Forge",
                    _ => "Drum",
                };
                format!("{kind} · {}", self.app.instrument_name(*id)).into()
            }
            other => other.kind().title().into(),
        }
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("kfxl-tab", format!("{tab:?}")))
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        self.app.tab_ui(ui, tab);
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            // The playlist scrolls its own grid; its track column stays put.
            Tab::Playlist => [false, false],
            Tab::Rack => [true, true],
            Tab::Roll { .. } => [false, false],
            Tab::Instrument { .. } => [false, true],
            Tab::Mixer => [true, false],
            Tab::Riff | Tab::Settings => [false, false],
        }
    }

    fn context_menu(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Tab,
        surface: SurfaceIndex,
        _node: NodeIndex,
    ) {
        if surface.is_main() {
            if ui.button("Float this window").clicked() {
                self.app.win(Cmd::Float(tab.clone()));
                ui.close_menu();
            }
        } else if ui.button("Dock this window").clicked() {
            self.app.win(Cmd::Dock(tab.clone()));
            ui.close_menu();
        }
    }
}

/// Where floating windows may go: off the sides and bottom of the work area
/// (FL), never above it, so a title bar stays in reach.
pub fn window_bounds(work: egui::Rect) -> egui::Rect {
    let reach = work.size() * 0.75;
    egui::Rect::from_min_max(work.min - egui::vec2(reach.x, 0.0), work.max + reach)
}

/// Dock chrome in kit terms: dark tab bar, raised active tab, edge seams.
pub fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(style);
    s.dock_area_padding = Some(Margin::same(8));
    s.main_surface_border_stroke = Stroke::NONE;
    s.separator.width = 1.0;
    s.separator.extra = 0.0;
    s.separator.extra_interact_width = 7.0;
    s.separator.color_idle = t::EDGE;
    s.separator.color_hovered = t::STRONG;
    s.separator.color_dragged = t::TEXT;
    s.tab_bar.bg_fill = t::SIDE;
    s.tab_bar.height = 32.0;
    s.tab_bar.hline_color = t::EDGE;
    s.tab_bar.corner_radius = CornerRadius {
        nw: 10,
        ne: 10,
        sw: 0,
        se: 0,
    };
    s.tab_bar.fill_tab_bar = true;
    let tab_r = CornerRadius {
        nw: 7,
        ne: 7,
        sw: 0,
        se: 0,
    };
    let base = egui_dock::TabInteractionStyle {
        outline_color: t::EDGE,
        corner_radius: tab_r,
        bg_fill: t::SIDE,
        text_color: t::MUTED,
    };
    s.tab.inactive = base.clone();
    s.tab.hovered = egui_dock::TabInteractionStyle {
        bg_fill: t::RAISED,
        text_color: t::TEXT,
        ..base.clone()
    };
    s.tab.active = egui_dock::TabInteractionStyle {
        bg_fill: t::PANEL_TOP,
        text_color: t::TEXT,
        ..base.clone()
    };
    s.tab.focused = egui_dock::TabInteractionStyle {
        bg_fill: t::CTL_TOP,
        text_color: t::TEXT,
        ..base.clone()
    };
    let kb = egui_dock::TabInteractionStyle {
        outline_color: t::FOCUS,
        ..s.tab.focused.clone()
    };
    s.tab.active_with_kb_focus = kb.clone();
    s.tab.inactive_with_kb_focus = egui_dock::TabInteractionStyle {
        outline_color: t::FOCUS,
        ..base.clone()
    };
    s.tab.focused_with_kb_focus = kb;
    s.tab.hline_below_active_tab_name = false;
    s.tab.minimum_width = Some(120.0);
    s.tab.tab_body.bg_fill = t::PANEL_BOTTOM;
    s.tab.tab_body.stroke = Stroke::new(1.0_f32, t::EDGE);
    s.tab.tab_body.corner_radius = CornerRadius {
        nw: 0,
        ne: 0,
        sw: 10,
        se: 10,
    };
    s.tab.tab_body.inner_margin = Margin::symmetric(14, 10);
    s.buttons.close_tab_color = t::FAINT;
    s.buttons.close_tab_active_color = t::TEXT;
    s.buttons.close_tab_bg_fill = t::CTL_HOVER;
    s.overlay.selection_color = t::TEXT.gamma_multiply(0.22);
    s.overlay.button_color = t::CTL_TOP;
    s.overlay.button_border_stroke = Stroke::new(1.0_f32, t::EDGE);
    s.overlay.hovered_leaf_highlight.color = t::TEXT.gamma_multiply(0.08);
    s.overlay.hovered_leaf_highlight.stroke = Stroke::new(1.0_f32, t::TEXT);
    s
}
