//! Program settings (FL: Options → Audio / MIDI / General settings), kept in
//! `~/.config/keyframexl/settings.ron`. Unknown or missing fields fall back to
//! the defaults, so an old file never stops the program from starting.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Settings {
    pub audio: AudioSettings,
    pub midi: MidiSettings,
    pub plugins: PluginSettings,
    pub general: GeneralSettings,
}

/// Audio driver API on Windows (`WASAPI` vs `ASIO`); ignored on Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioDriver {
    #[default]
    Wasapi,
    Asio,
}

impl AudioDriver {
    #[allow(dead_code)]
    pub const ALL: [Self; 2] = [Self::Wasapi, Self::Asio];

    #[allow(dead_code)]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wasapi => "WASAPI",
            Self::Asio => "ASIO",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Preferred audio driver API on Windows (`WASAPI` or `ASIO`).
    pub driver: AudioDriver,
    /// Output device name; `None` = the system default.
    pub output: Option<String>,
    /// Input device for recording; `None` = the system default.
    pub input: Option<String>,
    /// Sample rate in Hz.
    pub rate: u32,
    /// Frames per callback; `None` = what the device prefers.
    pub buffer: Option<u32>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            driver: AudioDriver::Wasapi,
            output: None,
            input: None,
            rate: 48_000,
            buffer: None,
        }
    }
}

/// Offered sample rates and buffer sizes.
pub const RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];
pub const BUFFERS: [u32; 8] = [64, 128, 256, 512, 1024, 2048, 4096, 8192];

/// How key velocity maps to note velocity (FL: MIDI settings → velocity curve).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VelocityCurve {
    Linear,
    /// Light playing comes out louder.
    Soft,
    /// Needs a firm hit for full level.
    Hard,
    /// Every note at the same level.
    Fixed,
}

impl VelocityCurve {
    pub const ALL: [Self; 4] = [Self::Linear, Self::Soft, Self::Hard, Self::Fixed];
    /// Map 1..=127 to 1..=127; `fixed` is the level for `Fixed`.
    pub fn apply(self, v: u8, fixed: u8) -> u8 {
        let x = v.clamp(1, 127) as f32 / 127.0;
        let y = match self {
            Self::Linear => x,
            Self::Soft => x.sqrt(),
            Self::Hard => x * x,
            Self::Fixed => return fixed.clamp(1, 127),
        };
        ((y * 127.0).round() as u8).clamp(1, 127)
    }
}

/// How KeyFrameXL synchronises MIDI Clock (0xF8 / Start / Stop) with external gear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MidiClockMode {
    #[default]
    Off,
    /// Send 24 PPQN MIDI Clock + Start/Continue/Stop to enabled output ports.
    Master,
    /// Follow incoming 24 PPQN MIDI Clock + Start/Continue/Stop from input ports.
    Slave,
}

impl MidiClockMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Master, Self::Slave];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiSettings {
    /// Input ports that are switched on, by name. New devices start on.
    pub enabled: Vec<String>,
    /// Ports the user switched off (so a replugged device stays off).
    pub disabled: Vec<String>,
    /// Output ports that are switched on, by name. Output ports start off until enabled.
    pub out_enabled: Vec<String>,
    /// Output ports explicitly switched off.
    pub out_disabled: Vec<String>,
    /// MIDI Clock synchronisation mode (Off / Master / Slave).
    pub clock: MidiClockMode,
    pub velocity: VelocityCurve,
    pub fixed_velocity: u8,
    /// Semitones added to every incoming note.
    pub transpose: i8,
    /// Sustain pedal (CC 64) holds notes.
    pub sustain: bool,
    /// Recorded notes snap to the grid.
    pub quantize_record: bool,
    /// Only this MIDI channel (1..=16) is heard; 0 = all (omni).
    pub channel: u8,
    /// Controller links: a MIDI channel + CC number driving a control.
    /// Device-agnostic (no port name), so a replugged controller keeps working.
    pub bindings: Vec<MidiBinding>,
}

/// One controller link: channel (1..=16) + CC (0..=127) drives a control.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MidiBinding {
    pub channel: u8,
    pub cc: u8,
    pub target: keyframexl::automation::Target,
}

impl MidiBinding {
    pub fn new(channel: u8, cc: u8, target: keyframexl::automation::Target) -> Self {
        Self {
            channel: channel.clamp(1, 16),
            cc: cc.min(127),
            target,
        }
    }

    /// A CC value 0..=127 mapped over the control's range.
    pub fn value_for(&self, v: u8, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (v.min(127) as f32 / 127.0)
    }
}

impl Default for MidiSettings {
    fn default() -> Self {
        Self {
            enabled: vec![],
            disabled: vec![],
            out_enabled: vec![],
            out_disabled: vec![],
            clock: MidiClockMode::Off,
            velocity: VelocityCurve::Linear,
            fixed_velocity: 100,
            transpose: 0,
            sustain: true,
            quantize_record: false,
            channel: 0,
            bindings: vec![],
        }
    }
}

impl MidiSettings {
    /// A port is on unless the user switched it off.
    pub fn is_on(&self, port: &str) -> bool {
        !self.disabled.iter().any(|p| p == port)
    }
    pub fn set_on(&mut self, port: &str, on: bool) {
        self.disabled.retain(|p| p != port);
        self.enabled.retain(|p| p != port);
        if on {
            self.enabled.push(port.to_string());
        } else {
            self.disabled.push(port.to_string());
        }
    }

    /// An output port starts off until explicitly switched on (avoids spamming gear).
    pub fn is_out_on(&self, port: &str) -> bool {
        self.out_enabled.iter().any(|p| p == port)
    }
    pub fn set_out_on(&mut self, port: &str, on: bool) {
        self.out_disabled.retain(|p| p != port);
        self.out_enabled.retain(|p| p != port);
        if on {
            self.out_enabled.push(port.to_string());
        } else {
            self.out_disabled.push(port.to_string());
        }
    }

    /// The binding driving `target`, if any.
    #[cfg(test)]
    pub fn binding_for_target(
        &self,
        target: keyframexl::automation::Target,
    ) -> Option<&MidiBinding> {
        self.bindings.iter().find(|b| b.target == target)
    }

    /// The binding for this controller, if any.
    pub fn binding_for_cc(&self, channel: u8, cc: u8) -> Option<&MidiBinding> {
        self.bindings
            .iter()
            .find(|b| b.channel == channel && b.cc == cc)
    }

    /// Link a controller to a control. One link per controller and one per
    /// control: an old link on either side moves here.
    pub fn bind(&mut self, channel: u8, cc: u8, target: keyframexl::automation::Target) {
        let b = MidiBinding::new(channel, cc, target);
        self.bindings
            .retain(|o| o.channel != b.channel || o.cc != b.cc);
        self.bindings.retain(|o| o.target != target);
        self.bindings.push(b);
    }

    /// Forget the link driving `target`. True when one was there.
    pub fn unbind_target(&mut self, target: keyframexl::automation::Target) -> bool {
        let n = self.bindings.len();
        self.bindings.retain(|b| b.target != target);
        self.bindings.len() != n
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSettings {
    /// Searched besides the standard VST3 folders.
    pub extra_dirs: Vec<PathBuf>,
    /// Plugins that crashed or hung while loading; shown, not loaded again until cleared.
    pub blocked: Vec<String>,
    /// Each plugin in its own process (FL: bridged): a crash silences that
    /// plugin instead of closing the program. Costs one audio block of latency.
    pub bridged: bool,
    /// The Wine `bin` folder yabridge runs Windows plugins with (first in
    /// PATH of the plugin processes). None: the system Wine.
    pub wine: Option<PathBuf>,
    /// Use the chosen build's DXVK (Proton has it) for Direct3D in plugins:
    /// JUCE 8 editors (SSD5) only redraw with it.
    pub dxvk: bool,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            extra_dirs: vec![],
            blocked: vec![],
            bridged: true,
            wine: None,
            dxvk: true,
        }
    }
}

/// Master bus visualisation in the top transport deck (FL: oscilloscope / spectrum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeckMonitor {
    Off,
    #[default]
    Scope,
    Spectrum,
}

impl DeckMonitor {
    pub const ALL: [Self; 3] = [Self::Off, Self::Scope, Self::Spectrum];
}

/// Light or dark interface; System follows the desktop (xdg-desktop-portal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::System];
}

/// Where a closed panel (Playlist, Channel rack, Piano roll, Mixer) opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PanelOpen {
    /// As a tab in the window that has the focus.
    #[default]
    Focused,
    /// In its own floating window.
    Window,
}

impl PanelOpen {
    pub const ALL: [Self; 2] = [Self::Focused, Self::Window];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    /// Minutes between autosaves; 0 = off.
    pub autosave_minutes: u32,
    /// Load real sounds (FL packs / .dwp) into a new project.
    pub default_kit: bool,
    /// Ask before closing a project with unsaved changes.
    pub confirm_close: bool,
    /// Click on every beat while playing (FL: Ctrl+M).
    pub metronome: bool,
    /// One bar of clicks before the song starts (FL: countdown before recording).
    pub count_in: bool,
    /// The count-in before every Play, not only with Record on.
    pub count_in_always: bool,
    /// With Record on, Play waits for the first played note (FL: wait for input).
    pub wait_input: bool,
    /// The computer keyboard plays notes (FL: Ctrl+T, typing keyboard to piano).
    pub typing_keyboard: bool,
    /// Run as an X11 window (XWayland on a Wayland desktop): plugin editors
    /// then sit inside our windows. Takes effect on the next start.
    pub x11: bool,
    /// The desktop's own title bar instead of ours (the menu row with the
    /// project name and window buttons, FL style).
    pub system_title_bar: bool,
    /// Interface size on top of the system scale, 0.5..=2.0 (FL: Options →
    /// General → zoom). Ctrl+= / Ctrl+- / Ctrl+0 change it too.
    pub ui_scale: f32,
    /// Master bus oscilloscope / spectrum monitor in the top transport deck.
    pub deck_monitor: DeckMonitor,
    /// Dark (default), light, or the desktop's choice.
    pub theme: ThemeMode,
    /// Automatically check GitHub Releases once per day for newer versions.
    pub check_updates: bool,
    /// Follow nightly builds (GitHub prereleases) instead of stable releases only.
    /// Off for new installs; a settings file that has it keeps its value.
    pub nightly_updates: bool,
    /// Unix timestamp (seconds) of the last automatic update check.
    pub last_update_check: u64,
    /// Version tag the user chose to skip (e.g. "0.2.0").
    pub skipped_version: Option<String>,
    /// Show the Welcome Screen hub with recent projects and templates on startup.
    pub show_welcome: bool,
    /// Interface language (English or Ukrainian).
    pub language: crate::i18n::Language,
    /// Saved outer/inner OS logical window size `[width, height]` restored on next start.
    pub window_size: Option<[f32; 2]>,
    /// Whether the main OS window was maximized when last used.
    pub window_maximized: bool,
    /// Where a closed panel opens: a tab in the focused window, or its own window.
    pub panel_open: PanelOpen,
}

/// Interface sizes offered in Settings and stepped through by VIEW → Interface size.
pub const UI_SCALES: [f32; 12] = [
    0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0,
];
pub const UI_SCALE_MIN: f32 = 0.5;
pub const UI_SCALE_MAX: f32 = 2.0;

pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [1366.0, 768.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [1180.0, 680.0];
pub const MAX_WINDOW_SIZE: [f32; 2] = [7680.0, 4320.0];

impl GeneralSettings {
    /// Reject non-finite or absurdly small sizes and clamp valid ones to the
    /// supported logical window bounds.
    pub fn sanitize_window_size(size: [f32; 2]) -> Option<[f32; 2]> {
        let [w, h] = size;
        if !w.is_finite() || !h.is_finite() || w < 600.0 || h < 400.0 {
            return None;
        }
        Some([
            w.clamp(MIN_WINDOW_SIZE[0], MAX_WINDOW_SIZE[0]),
            h.clamp(MIN_WINDOW_SIZE[1], MAX_WINDOW_SIZE[1]),
        ])
    }

    /// Logical window size to restore on startup, falling back to the default.
    pub fn restored_window_size(&self) -> [f32; 2] {
        self.window_size
            .and_then(Self::sanitize_window_size)
            .unwrap_or(DEFAULT_WINDOW_SIZE)
    }
}

/// The next offered size above (`up`) or below the current one.
pub fn step_ui_scale(current: f32, up: bool) -> f32 {
    let next = if up {
        UI_SCALES.iter().copied().find(|s| *s > current + 0.001)
    } else {
        UI_SCALES
            .iter()
            .rev()
            .copied()
            .find(|s| *s < current - 0.001)
    };
    next.unwrap_or(current)
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            autosave_minutes: 2,
            default_kit: true,
            confirm_close: true,
            metronome: false,
            count_in: false,
            count_in_always: false,
            wait_input: false,
            typing_keyboard: false,
            x11: true,
            system_title_bar: false,
            ui_scale: 1.0,
            deck_monitor: DeckMonitor::Scope,
            theme: ThemeMode::Dark,
            check_updates: true,
            nightly_updates: false,
            last_update_check: 0,
            skipped_version: None,
            show_welcome: true,
            language: crate::i18n::Language::English,
            window_size: None,
            window_maximized: false,
            panel_open: PanelOpen::Focused,
        }
    }
}

pub fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("keyframexl").join("settings.ron"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| ron::from_str(&s).ok())
            .unwrap_or_else(|| {
                let mut s = Self::default();
                s.general.language = crate::i18n::Language::from_system();
                s
            })
    }

    pub fn save(&self) -> Result<(), String> {
        let p = path().ok_or("No config folder")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?;
        let tmp = p.with_extension("ron.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_installs_follow_stable_but_saved_choice_is_kept() {
        assert!(!Settings::default().general.nightly_updates);
        let saved: Settings = ron::from_str("(general: (nightly_updates: true))").unwrap();
        assert!(saved.general.nightly_updates);
    }

    #[test]
    fn settings_round_trip_and_old_files_load() {
        let mut s = Settings::default();
        s.audio.rate = 44_100;
        s.audio.buffer = Some(256);
        s.midi.set_on("Keystation 49", false);
        s.midi.transpose = -12;
        let text = ron::ser::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
        let old: Settings = ron::from_str("(audio: (rate: 96000))").unwrap();
        assert_eq!(old.audio.rate, 96_000);
        assert_eq!(
            old.midi,
            MidiSettings::default(),
            "missing parts take defaults"
        );
        assert!(!s.midi.is_on("Keystation 49"));
        assert!(s.midi.is_on("Other keys"), "new devices start on");
    }

    #[test]
    fn ui_scale_steps_through_the_offered_sizes() {
        assert_eq!(step_ui_scale(1.0, false), 0.9);
        assert_eq!(step_ui_scale(1.0, true), 1.1);
        assert_eq!(
            step_ui_scale(0.83, false),
            0.8,
            "an odd size snaps to the next offered one"
        );
        assert_eq!(step_ui_scale(UI_SCALE_MAX, true), UI_SCALE_MAX);
        assert_eq!(step_ui_scale(UI_SCALE_MIN, false), UI_SCALE_MIN);
        let old: Settings = ron::from_str("(general: (autosave_minutes: 5))").unwrap();
        assert_eq!(old.general.ui_scale, 1.0, "older files start at 100%");
    }

    #[test]
    fn velocity_curves() {
        assert_eq!(VelocityCurve::Linear.apply(64, 100), 64);
        assert!(VelocityCurve::Soft.apply(40, 100) > 40);
        assert!(VelocityCurve::Hard.apply(40, 100) < 40);
        assert_eq!(VelocityCurve::Fixed.apply(5, 100), 100);
        for c in VelocityCurve::ALL {
            assert_eq!(c.apply(127, 127), 127);
            assert!(c.apply(1, 1) >= 1);
        }
    }

    #[test]
    fn controller_bindings_round_trip_and_move() {
        use keyframexl::automation::Target;
        let mut m = MidiSettings::default();
        m.bind(1, 74, Target::MixerVolume { track: 0 });
        m.bind(2, 10, Target::ChannelPan { inst: 4 });
        assert_eq!(
            m.binding_for_cc(1, 74).map(|b| b.target),
            Some(Target::MixerVolume { track: 0 })
        );
        assert_eq!(
            m.binding_for_target(Target::ChannelPan { inst: 4 })
                .map(|b| (b.channel, b.cc)),
            Some((2, 10))
        );
        // One link per controller and one per control: re-linking moves it.
        m.bind(1, 74, Target::ChannelPan { inst: 4 });
        assert_eq!(m.bindings.len(), 1);
        assert_eq!(
            m.binding_for_cc(1, 74).map(|b| b.target),
            Some(Target::ChannelPan { inst: 4 })
        );
        assert!(m.unbind_target(Target::ChannelPan { inst: 4 }));
        assert!(m.bindings.is_empty());
        assert!(!m.unbind_target(Target::ChannelPan { inst: 4 }));
        // Values map 0..127 over the range.
        let b = MidiBinding::new(1, 74, Target::MixerPan { track: 0 });
        assert_eq!(b.value_for(0, -1.0, 1.0), -1.0);
        assert_eq!(b.value_for(127, -1.0, 1.0), 1.0);
        assert!((b.value_for(64, 0.0, 1.0) - 64.0 / 127.0).abs() < 1e-6);
        // Round-trip with bindings, and old files without them.
        let mut s = Settings::default();
        s.midi.bind(3, 7, Target::MixerPan { track: 2 });
        let text = ron::ser::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
        let old: Settings = ron::from_str("(midi: (channel: 2))").unwrap();
        assert!(
            old.midi.bindings.is_empty(),
            "older files start with no links"
        );
    }

    #[test]
    fn midi_out_clock_monitor_and_update_settings_round_trip() {
        let mut s = Settings::default();
        assert!(!s.midi.is_out_on("Synth Port"));
        s.midi.set_out_on("Synth Port", true);
        assert!(s.midi.is_out_on("Synth Port"));
        s.midi.clock = MidiClockMode::Master;
        s.general.deck_monitor = DeckMonitor::Spectrum;
        s.general.check_updates = false;
        s.general.last_update_check = 1_700_000_000;
        s.general.skipped_version = Some("0.2.0".into());

        let text = ron::ser::to_string(&s).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(back, s);

        let old: Settings = ron::from_str("(general: (autosave_minutes: 3))").unwrap();
        assert_eq!(old.general.deck_monitor, DeckMonitor::Scope);
        assert!(old.general.check_updates);
        assert_eq!(old.general.last_update_check, 0);
        assert_eq!(old.general.skipped_version, None);
        assert_eq!(old.midi.clock, MidiClockMode::Off);
    }

    #[test]
    fn window_geometry_round_trip_and_sanitization() {
        let mut s = Settings::default();
        assert_eq!(s.general.window_size, None);
        assert!(!s.general.window_maximized);
        assert_eq!(s.general.restored_window_size(), DEFAULT_WINDOW_SIZE);

        s.general.window_size = Some([1920.0, 1080.0]);
        s.general.window_maximized = true;
        let text = ron::ser::to_string(&s).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(back.general.window_size, Some([1920.0, 1080.0]));
        assert!(back.general.window_maximized);
        assert_eq!(back.general.restored_window_size(), [1920.0, 1080.0]);

        let old: Settings = ron::from_str("(general: (autosave_minutes: 2))").unwrap();
        assert_eq!(old.general.window_size, None);
        assert!(!old.general.window_maximized);

        assert_eq!(GeneralSettings::sanitize_window_size([0.0, 0.0]), None);
        assert_eq!(GeneralSettings::sanitize_window_size([300.0, 200.0]), None);
        assert_eq!(
            GeneralSettings::sanitize_window_size([f32::NAN, 800.0]),
            None
        );
        assert_eq!(
            GeneralSettings::sanitize_window_size([f32::INFINITY, 800.0]),
            None
        );
        assert_eq!(
            GeneralSettings::sanitize_window_size([800.0, 500.0]),
            Some(MIN_WINDOW_SIZE)
        );
        assert_eq!(
            GeneralSettings::sanitize_window_size([9999.0, 9999.0]),
            Some(MAX_WINDOW_SIZE)
        );
    }
}
