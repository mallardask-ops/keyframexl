//! KeyFrameXL shell: deck (transport) on top, browser on the left, a dock of
//! editor windows in the middle (tabs, splits, floating), status line at the
//! bottom. Layout and look follow docs/ui-rules.md; surfaces are painted with `kit`.

mod automation;
mod browser;
mod deck;
mod export;
mod files;
mod forge;
mod fx_response;
mod fx_views;
mod import_dialog;
mod instrument;
mod kit;
mod library;
mod manual;
mod markdown;
mod menu;
mod midi_io;
mod mixer;
mod playlist;
mod plugins_ui;
mod presets_ui;
mod project;
mod rack;
pub(crate) mod record;
mod riff_machine;
pub(crate) mod roll;
mod sampler;
mod script;
mod settings_ui;
mod shortcuts;
mod show;
mod synth;
mod tap;
mod tempo_lane;
mod updater_ui;
mod vocal;
pub(crate) mod welcome;
mod workspace;

pub(crate) use project::Pending;
pub use workspace::{Cmd, Kind, Tab};

use crate::{audio::Audio, theme as t};
#[cfg(test)]
use eframe::egui::Rect;
use eframe::egui::{self, Align2, Pos2, Sense, Stroke, vec2};
use keyframexl::sequence::Song;
use std::{
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

pub const DECK_H: f32 = deck::ROW1_H + deck::ROW2_H;
pub const BROWSER_W: f32 = 208.0;
pub const STATUS_H: f32 = 28.0;
pub const PAGE: f32 = 16.0;
/// The Bass sampler of a new project: the piano roll opens on it.
pub const BASS: u64 = 4;

/// Piano roll tools, FL's toolbar and keys.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum Tool {
    #[default]
    Draw,
    Paint,
    Delete,
    Mute,
    Slice,
    Select,
    Zoom,
    Slide,
}

impl Tool {
    pub const ALL: [Tool; 8] = [
        Tool::Draw,
        Tool::Paint,
        Tool::Delete,
        Tool::Mute,
        Tool::Slice,
        Tool::Select,
        Tool::Zoom,
        Tool::Slide,
    ];
    pub fn key(self) -> egui::Key {
        match self {
            Tool::Draw => egui::Key::P,
            Tool::Paint => egui::Key::B,
            Tool::Delete => egui::Key::D,
            Tool::Mute => egui::Key::T,
            Tool::Slice => egui::Key::C,
            Tool::Select => egui::Key::E,
            Tool::Zoom => egui::Key::Z,
            Tool::Slide => egui::Key::S,
        }
    }
}

/// Piano roll snap. A view setting: notes keep any tick; Alt skips snap.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum Snap {
    Half,
    #[default]
    Step,
    Triplet,
    Beat,
    Bar,
    Off,
}
impl Snap {
    pub const ALL: [Snap; 6] = [
        Snap::Half,
        Snap::Step,
        Snap::Triplet,
        Snap::Beat,
        Snap::Bar,
        Snap::Off,
    ];
    /// Grid in ticks.
    pub fn grid(self) -> keyframexl::sequence::Tick {
        use keyframexl::sequence::{BAR, PPQ, STEP};
        match self {
            Snap::Half => STEP / 2,
            Snap::Step => STEP,
            Snap::Triplet => PPQ / 3,
            Snap::Beat => PPQ,
            Snap::Bar => BAR,
            Snap::Off => 1,
        }
    }
}

/// Which part of a window header a caller fills, see [`App::panel_header`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Menu,
    Tools,
}

/// Status tone. Green only for the result of an action the user just ran.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Quiet,
    Done,
    Wait,
    Bad,
}

pub struct App {
    pub(crate) song: Song,
    history: Vec<Song>,
    future: Vec<Song>,
    pub(crate) ws: workspace::Workspace,
    win_cmds: Vec<Cmd>,
    /// Tab with keyboard/pointer focus at the start of this frame.
    pub(crate) focus: Option<Tab>,
    persist: bool,
    startup_maximized: bool,
    startup_maximize_nudge_done: bool,
    startup_maximize_frames: u8,
    layout_text: String,
    layout_saved_at: f64,
    pub(crate) selected: u64,
    /// Instrument of the last focused piano roll (F7 opens it).
    pub(crate) instrument: u64,
    /// Instrument of the piano roll being drawn right now.
    pub(crate) roll_inst: u64,
    /// Piano roll scroll and zoom per instrument.
    pub(crate) roll_view: std::collections::HashMap<u64, roll::RollView>,
    /// Selected notes (ids) of the roll being edited.
    pub(crate) roll_sel: Vec<u64>,
    riff: Option<riff_machine::Session>,
    /// Where a dropped MIDI clip lands, as drawn last frame: channel rack
    /// rows and piano rolls (screen rect, channel).
    pub(crate) midi_zones: Vec<(egui::Rect, u64)>,
    /// The pointer where files were just dropped (X11 asked directly: a drag
    /// from a plugin editor leaves egui's own pointer stale).
    pub(crate) drop_pointer: Option<egui::Pos2>,
    /// Length of the last note drawn, moved or resized (FL: new notes take it).
    pub(crate) last_len: Option<keyframexl::sequence::Tick>,
    /// What the piano roll's event lane edits.
    pub(crate) lane_target: roll::LaneTarget,
    /// Copied notes (Ctrl+C), positions as copied.
    pub(crate) note_clip: Vec<keyframexl::sequence::Note>,
    /// Other channels' notes as outlines in the piano roll.
    pub(crate) ghosts: bool,
    /// A right-drag erase already took its undo step.
    erasing: bool,
    /// Selected channels in the channel rack (FL selector).
    pub(crate) rack_sel: Vec<u64>,
    /// Channel rack graph editor shown (Ctrl+K).
    pub(crate) rack_graph: bool,
    /// Mixer: selected track and effect slot, meter levels, last meter read.
    pub(crate) mix_sel: usize,
    pub(crate) mix_slot: usize,
    /// A route cable being dragged from a strip's route button, if any.
    pub(crate) route_drag: Option<mixer::RouteDrag>,
    /// Strip rects of the last mixer frame (drop targets; tests read them too).
    pub(crate) mix_strip_rects: Vec<eframe::egui::Rect>,
    /// The insert soloed last (its lamp shows blue while the solo holds).
    pub(crate) mix_solo: Option<usize>,
    /// Colour group for new piano roll notes.
    pub(crate) note_color: u8,
    /// New piano roll notes are porta notes.
    pub(crate) porta_new: bool,
    /// Piano roll key for row highlighting (FL: Scale highlighting); None = off.
    pub(crate) roll_scale: Option<keyframexl::scale::Scale>,
    /// A channel just made for a dropped file: its sound landing joins that undo step.
    pub(crate) join_preset: Option<u64>,
    /// Automation clip (pattern id) open in the envelope editor.
    pub(crate) auto_open: Option<u64>,
    /// Last value sent to each automated plugin parameter (plugin, param).
    pub(crate) auto_sent: std::collections::HashMap<(u64, u32), f32>,
    /// Most recently tweaked automatable control (`Tools → Last tweaked`).
    pub(crate) last_tweaked: Option<keyframexl::automation::Target>,
    /// Value copied via `Tools → Last tweaked → Copy value`.
    pub(crate) auto_clipboard: Option<(keyframexl::automation::Target, f32)>,
    /// Active Multilink to controllers session (`Ctrl+J`).
    pub(crate) multilink: Option<automation::Multilink>,
    /// Live automation samples captured while Record + Play are active.
    pub(crate) auto_take: automation::AutoTake,
    /// New and moved notes land on the scale.
    pub(crate) scale_snap: bool,
    /// Armed mixer tracks, Record switch and takes in progress.
    pub(crate) rec: record::Recorder,
    /// Sound libraries found on this computer (filled in the background).
    pub(crate) libraries: library::Libraries,
    libraries_rx: Option<mpsc::Receiver<library::Libraries>>,
    /// The live note sounding while a key or the mouse is held: (id, channel, pitch).
    pub(crate) held: Option<(u64, u64, u8)>,
    live_id: u64,
    /// Live check: a MIDI file to import after the library search.
    capture_midi: Option<std::path::PathBuf>,
    /// Piano roll key held down (drawn lit).
    pub(crate) key_down: Option<u8>,
    /// Export audio dialog and the choices it remembers.
    pub(crate) export: Option<export::ExportDialog>,
    pub(crate) export_prefs: export::ExportPrefs,
    meters: Vec<f32>,
    meter_at: std::time::Instant,
    /// Browser tab: 0 project, 1 files.
    pub(crate) browser_tab: usize,
    pub(crate) files: files::Files,
    /// File being previewed and its decode in flight.
    pub(crate) previewing: Option<std::path::PathBuf>,
    preview_rx: Option<(
        std::path::PathBuf,
        mpsc::Receiver<Result<crate::audio::Preview, String>>,
    )>,
    /// Forge renders, mutations and bounces in flight.
    pub(crate) forge_jobs: forge::Jobs,
    /// Sampler editor page and zone selection per channel.
    pub(crate) sampler_view: std::collections::HashMap<u64, sampler::SamplerUi>,
    pub(crate) clip: Option<usize>,
    pub(crate) tool: Tool,
    pub(crate) snap: Snap,
    pub(crate) velocity: u8,
    pub(crate) song_mode: bool,
    /// Keep the playhead on screen while playing (FL: follow).
    pub(crate) follow: bool,
    status: (Tone, String),
    roll_gesture: Option<(u64, roll::Gesture)>,
    pub(crate) tap_win: Option<tap::TapWindow>,
    audio: Option<Audio>,
    capture: Option<(std::path::PathBuf, u32)>,
    /// Scripted input for live checks (`KFXL_CAPTURE_SCRIPT`).
    script: Option<script::Script>,
    /// Where the project was opened from or last saved; Save writes here.
    pub(crate) path: Option<std::path::PathBuf>,
    /// Edits since start, and the count at the last save/open (unsaved marker).
    edits: u64,
    saved_edits: u64,
    /// Pattern rename in progress (F2): (pattern, text).
    renaming: Option<(u64, String)>,
    show_shortcuts: bool,
    /// Stretched zone audio made in the background: (instrument, zone, result).
    stretch_jobs: (
        std::sync::mpsc::Sender<(u64, usize, keyframexl::sampler::Stretched)>,
        std::sync::mpsc::Receiver<(u64, usize, keyframexl::sampler::Stretched)>,
    ),
    /// Stretch keys being made now.
    stretch_busy: std::collections::HashSet<u64>,
    /// HELP → Manual window.
    show_manual: bool,
    /// A New/Open/Quit waiting on "Save changes?".
    pub(crate) pending: Option<Pending>,
    /// The close was confirmed (or nothing was unsaved).
    quit_ok: bool,
    /// An installed update to start once this window has closed.
    pub(crate) relaunch: Option<std::path::PathBuf>,
    /// Open recent, newest first.
    pub(crate) recent: Vec<std::path::PathBuf>,
    title: String,
    /// Edit count and time of the last autosave.
    autosaved: (u64, f64),
    /// Preset load in flight: (target sampler, result).
    /// Preset loads in flight: (target channel, result, quiet). Quiet loads
    /// (the default kit) take no undo step and leave the project saved.
    /// Loads waiting for a free slot (three run at once).
    pub(crate) load_queue: std::collections::VecDeque<(u64, std::path::PathBuf, bool)>,
    /// FL audio clip start points (seconds) to cut a sampler's sample at once
    /// it loads (see `flp::slice_preset`).
    pub(crate) flp_slices: std::collections::HashMap<u64, keyframexl::flp::AudioSlices>,
    /// FL Studio rendering an imported project's generators: progress
    /// lines, then the WAV per FL channel (see `flp_freeze`).
    pub(crate) flp_freeze: Option<std::sync::mpsc::Receiver<FreezeMsg>>,
    /// FL channel → (name, instrument, mixer insert) of the import waiting for them.
    pub(crate) flp_freeze_for: Vec<(u32, String, u64, u16)>,
    /// Voicebank download / install running in the background, with its label.
    /// The system CJK font is loaded (once a singing channel exists).
    cjk_font: bool,
    pub(crate) vocal_job: Option<(String, mpsc::Receiver<Result<std::path::PathBuf, String>>)>,
    /// A note's lyric being typed in the piano roll (note id, text), and the lyrics dialog's text.
    pub(crate) lyric_edit: Option<(u64, String)>,
    pub(crate) lyrics_dialog: Option<String>,
    pub(crate) importing: Vec<(
        u64,
        mpsc::Receiver<Result<keyframexl::sequence::SamplePreset, String>>,
        bool,
    )>,
    /// Options (F10): audio, MIDI, plugins, general; and its window.
    pub(crate) settings: crate::settings::Settings,
    pub(crate) settings_ui: Option<settings_ui::SettingsUi>,
    /// MIDI keyboards; keys held on them (lit in the piano roll); notes being recorded.
    pub(crate) midi_in: Option<crate::midi_in::MidiIn>,
    /// MIDI hardware output ports & 24 PPQN Clock Master.
    pub(crate) midi_out: Option<crate::midi_out::MidiOut>,
    pub(crate) midi_keys: [bool; 128],
    pub(crate) midi_rec: settings_ui::Recording,
    pub(crate) midi_take: bool,
    /// Controller learn: the control waiting for its MIDI CC, if armed.
    pub(crate) midi_learn: Option<keyframexl::automation::Target>,
    /// Last controller move, to group one gesture into one undo step.
    pub(crate) midi_cc_last: Option<std::time::Instant>,
    /// Non-blocking GitHub Releases update checker.
    pub(crate) updater: crate::updater::UpdateChecker,
    /// Startup / hub welcome screen state and visibility.
    pub(crate) welcome: welcome::WelcomeState,
    pub(crate) welcome_open: bool,
    import_dialog: Option<import_dialog::ImportDialog>,
    /// Hosted plugins (VST3, VST2, CLAP).
    pub(crate) plugins: plugins_ui::PluginState,
    /// Built-in effects open in their own windows: (mixer track, slot).
    pub(crate) fx_windows: Vec<(usize, usize)>,
    /// Play was pressed with Record and Wait for input on: the first note starts it.
    pub(crate) waiting_input: bool,
    /// Full-canvas Demo / Showcase overlay state.
    pub(crate) show: Option<show::ShowState>,
    /// Open Project info on the next UI frame (e.g. when a loaded project sets `show_on_open`).
    pub(crate) open_info_next: bool,
    #[cfg(test)]
    pub(crate) probe: Probe,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct Probe {
    pub pad: Option<Rect>,
    pub roll: Option<roll::Geom>,
    pub add: Option<Rect>,
    pub unique: Option<Rect>,
    pub clip1: Option<Rect>,
    pub add_channel: Option<Rect>,
    /// Playlist grid: (grid rect, pixels per bar, lane height, first visible bar).
    pub pl_grid: Option<(Rect, f32, f32)>,
    /// Tempo field on the LCD.
    pub bpm: Option<Rect>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        t::setup(&cc.egui_ctx);
        let args: Vec<_> = std::env::args().collect();
        let kind = args.get(1).map(String::as_str);
        let capture = kind.is_some_and(|k| k.starts_with("--capture-"));
        if capture {
            cc.egui_ctx.set_pixels_per_point(1.0);
            // Capture runs record takes too: keep them out of the user's music folder.
            let _ = record::CAPTURE_RECORDINGS
                .set(std::env::temp_dir().join("keyframexl-capture-recordings"));
        }
        let mut song = if capture || args.iter().any(|a| a == "--demo") {
            Song::demo()
        } else {
            Song::default()
        };
        // KFXL_CAPTURE_DEMO=1: the showcase song (its voice written to a temp folder),
        // kept as it is by every capture mode.
        let demo = capture && std::env::var_os("KFXL_CAPTURE_DEMO").is_some();
        if demo {
            let voice = std::env::temp_dir().join("keyframexl-capture-voice");
            let _ = keyframexl::demo::write_voice(&voice);
            song = keyframexl::demo::build(Some(&voice));
        }
        // Captures can show a real project: KFXL_CAPTURE_PROJECT=path.kfxl
        if capture
            && let Ok(path) = std::env::var("KFXL_CAPTURE_PROJECT")
            && let Ok(opened) = keyframexl::project_io::open(std::path::Path::new(&path))
        {
            song = opened;
        }
        if kind == Some("--capture-playlist") && !demo {
            song.clips.push(keyframexl::sequence::Clip::new(1, 0, 0));
            song.clips.push(keyframexl::sequence::Clip::new(
                1,
                keyframexl::sequence::BAR,
                0,
            ));
            let mut c = keyframexl::sequence::Clip::new(1, 2 * keyframexl::sequence::BAR, 1);
            c.muted = true;
            song.clips.push(c);
            song.clips.push(keyframexl::sequence::Clip::new(
                1,
                4 * keyframexl::sequence::BAR,
                2,
            ));
            // Off the grid, and stretched over three passes of its pattern.
            song.clips.push(keyframexl::sequence::Clip::new(
                1,
                5 * keyframexl::sequence::BAR + 3 * keyframexl::sequence::STEP + 37,
                3,
            ));
            let mut long = keyframexl::sequence::Clip::new(1, 0, 4);
            long.length = Some(3 * keyframexl::sequence::BAR);
            song.clips.push(long);
            // KFXL_CAPTURE_LOOP=1: a time selection over bars 2-4.
            if std::env::var("KFXL_CAPTURE_LOOP").is_ok() {
                song.loop_range = Some((keyframexl::sequence::BAR, 4 * keyframexl::sequence::BAR));
                let bar = keyframexl::sequence::BAR;
                for (tick, name) in [
                    (0, "Intro"),
                    (bar, "Verse"),
                    (4 * bar, "Drop"),
                    (5 * bar + bar / 2, "Fill"),
                ] {
                    let i = song.add_marker(tick);
                    song.markers[i].name = name.into();
                }
            }
            // KFXL_CAPTURE_TEMPO=1: a tempo map with a jump and a ramp.
            if std::env::var("KFXL_CAPTURE_TEMPO").is_ok() {
                use keyframexl::tempo::{TempoPoint, TempoShape::*};
                let bar = keyframexl::sequence::BAR;
                song.tempo_map.points = vec![
                    TempoPoint {
                        tick: 0,
                        bpm: song.tempo,
                        shape: Step,
                    },
                    TempoPoint {
                        tick: 2 * bar,
                        bpm: 90.0,
                        shape: Linear,
                    },
                    TempoPoint {
                        tick: 5 * bar,
                        bpm: 170.0,
                        shape: Step,
                    },
                ];
                use keyframexl::tempo::Meter;
                song.meters = vec![
                    Meter {
                        bar: 0,
                        num: 4,
                        den: 4,
                    },
                    Meter {
                        bar: 3,
                        num: 3,
                        den: 4,
                    },
                    Meter {
                        bar: 5,
                        num: 7,
                        den: 8,
                    },
                ];
            }
        }
        // Captures (screenshots, live checks) run on defaults, never the user's settings.
        let mut settings = if capture {
            crate::settings::Settings::default()
        } else {
            crate::settings::Settings::load()
        };
        if let Some(lang) = crate::i18n::Language::from_env_override() {
            settings.general.language = lang;
        }
        // Captures can pick the Wine build for plugins: KFXL_CAPTURE_WINE=<bin folder>.
        if capture && let Some(w) = std::env::var_os("KFXL_CAPTURE_WINE") {
            settings.plugins.wine = Some(w.into());
        }
        crate::plugins::set_dxvk(settings.plugins.dxvk);
        crate::plugins::set_wine(settings.plugins.wine.clone());
        let tr = crate::i18n::strings(settings.general.language);
        let (audio, status) = match Audio::open(song.clone(), &settings.audio) {
            Ok(a) => (Some(a), (Tone::Quiet, String::new())),
            Err(e) => (
                None,
                (
                    Tone::Bad,
                    crate::i18n::fill(tr.status_no_audio_init, &[("err", &e.to_string())]),
                ),
            ),
        };
        if capture && let Some(a) = &audio {
            a.playing.store(true, Ordering::Relaxed);
        }
        let mut app = Self::with_song(song, audio);
        app.status = status;
        if !capture && let Some((_, exe)) = crate::plugins::foreign_wineserver() {
            app.status = (
                Tone::Bad,
                crate::i18n::fill(
                    tr.status_wine_conflict,
                    &[("name", &exe.display().to_string())],
                ),
            );
        }
        if let Some(a) = &app.audio {
            a.metronome
                .store(settings.general.metronome, Ordering::Relaxed);
        }
        app.settings = settings;
        if capture {
            app.ws.active = match kind {
                Some("--capture-roll") => 1,
                Some("--capture-edit") => 2,
                _ => 0,
            };
            // Live checks (recorded from PipeWire by scripts):
            // KFXL_CAPTURE_PREVIEW=file previews a browser file with the transport stopped;
            // KFXL_CAPTURE_EQ=freq,gain puts Parametric EQ 2 on Master with band 4 there;
            // KFXL_CAPTURE_RECORD=1 arms Master with Record on (the take is written on exit).
            // KFXL_CAPTURE_LIBRARIES=1 runs the library search and default kit as a normal start does.
            if std::env::var("KFXL_CAPTURE_LIBRARIES").is_ok() {
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let home = dirs::home_dir().unwrap_or_default();
                    let _ = tx.send(library::find(&home, &library::default_mounts(&home)));
                });
                app.libraries_rx = Some(rx);
            }
            // KFXL_CAPTURE_MIDI=file.mid imports it once the libraries are known.
            if let Ok(p) = std::env::var("KFXL_CAPTURE_MIDI") {
                app.capture_midi = Some(std::path::PathBuf::from(p));
            }
            if std::env::var("KFXL_CAPTURE_STOPPED").is_ok()
                && let Some(a) = &app.audio
            {
                a.playing.store(false, Ordering::Relaxed);
            }
            if let Ok(file) = std::env::var("KFXL_CAPTURE_PREVIEW") {
                if let Some(a) = &app.audio {
                    a.playing.store(false, Ordering::Relaxed);
                }
                app.toggle_preview(std::path::PathBuf::from(file));
            }
            if let Some((f, g)) = std::env::var("KFXL_CAPTURE_EQ").ok().and_then(|v| {
                let (f, g) = v.split_once(',')?;
                Some((f.parse::<f32>().ok()?, g.parse::<f32>().ok()?))
            }) {
                let mut slot = keyframexl::fx::FxSlot::new(keyframexl::fx::FxKind::ParamEq);
                slot.params[9] = f;
                slot.params[10] = g;
                slot.params[11] = 0.7;
                app.song.mixer[0].fx.push(slot);
                app.publish();
                app.fx_windows.push((0, app.song.mixer[0].fx.len() - 1));
            }
            if let Ok(spec) = std::env::var("KFXL_CAPTURE_FX") {
                use keyframexl::fx::FxKind;
                let (name, values) = spec.split_once(':').unwrap_or((&spec, ""));
                let kind = match name {
                    "time" => Some(FxKind::Time),
                    "transient" => Some(FxKind::Transient),
                    "midside" => Some(FxKind::MidSide),
                    "vocoder" => Some(FxKind::Vocoder),
                    "multiband" => Some(FxKind::Multiband),
                    "convolution" => Some(FxKind::Convolution),
                    "balance" => Some(FxKind::Balance),
                    "stereoshaper" => Some(FxKind::StereoShaper),
                    "squeeze" => Some(FxKind::Squeeze),
                    "hyperchorus" => Some(FxKind::HyperChorus),
                    "filter" => Some(FxKind::Filter),
                    "drive" => Some(FxKind::Drive),
                    "chorus" => Some(FxKind::Chorus),
                    "stereo" => Some(FxKind::Stereo),
                    "phaser" => Some(FxKind::Phaser),
                    "eq" => Some(FxKind::Eq),
                    "autopan" => Some(FxKind::AutoPan),
                    "tremolo" => Some(FxKind::Tremolo),
                    "bitcrusher" => Some(FxKind::Bitcrusher),
                    "noisegate" => Some(FxKind::NoiseGate),
                    "ringmod" => Some(FxKind::RingMod),
                    "tilteq" => Some(FxKind::TiltEq),
                    "wavefolder" => Some(FxKind::Wavefolder),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let mut slot = keyframexl::fx::FxSlot::new(kind);
                    for (i, v) in values.split(',').enumerate().take(slot.params.len()) {
                        slot.params[i] = v.trim().parse().unwrap_or(slot.params[i]);
                    }
                    if kind == FxKind::Convolution
                        && let Ok(path) = std::env::var("KFXL_CAPTURE_IR")
                    {
                        slot.ir = Some(path.into());
                    }
                    app.song.mixer[0].fx.push(slot);
                    app.publish();
                    app.fx_windows.push((0, app.song.mixer[0].fx.len() - 1));
                }
            }
            if let Ok(path) = std::env::var("KFXL_CAPTURE_SCRIPT") {
                match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| script::Script::parse(&t))
                {
                    Ok(s) => app.script = Some(s),
                    Err(e) => eprintln!("Script not loaded: {e}"),
                }
            }
            // KFXL_CAPTURE_RECENT=a.kfxl:b.kfxl fills FILE → Open recent.
            if let Ok(list) = std::env::var("KFXL_CAPTURE_RECENT") {
                app.recent = std::env::split_paths(&list).collect();
            }
            // KFXL_CAPTURE_SEARCH=text types it into the browser's search.
            if let Ok(q) = std::env::var("KFXL_CAPTURE_SEARCH") {
                cc.egui_ctx
                    .data_mut(|d| d.insert_temp(egui::Id::new("kfxl-browser-search"), q));
            }
            // KFXL_CAPTURE_TAP=1 opens the tap tempo window with a steady run.
            if std::env::var("KFXL_CAPTURE_TAP").is_ok() {
                app.tap_win = Some(tap::TapWindow::demo());
            }
            // KFXL_CAPTURE_INFO=1 fills in Project info and opens its window.
            if std::env::var("KFXL_CAPTURE_INFO").is_ok() {
                app.song.info = keyframexl::sequence::ProjectInfo {
                    title: "Нічне місто".into(),
                    author: "Demo Artist".into(),
                    album: "Night Sessions".into(),
                    genre: "Deep House".into(),
                    year: "2026".into(),
                    copyright: "CC BY 4.0".into(),
                    url: "https://example.com/night-city".into(),
                    comments: "Kick from Kit 3, pad resampled from take 2.".into(),
                    show_on_open: false,
                    scale: Some(keyframexl::scale::Scale {
                        root: 9,
                        kind: keyframexl::scale::Kind::Minor,
                    }),
                };
                app.roll_scale = app.song.info.scale;
                app.toggle_info(&cc.egui_ctx);
            }
            // KFXL_CAPTURE_UPDATE=notes.md[:stage] opens the update window on a
            // made-up v9.9.9 with those notes; stage: idle|download|ready|failed.
            if let Ok(spec) = std::env::var("KFXL_CAPTURE_UPDATE") {
                use crate::updater::{CheckState, InstallState, ReleaseInfo};
                let (file, stage) = spec.rsplit_once(':').unwrap_or((spec.as_str(), "idle"));
                app.updater.state = CheckState::Available(ReleaseInfo {
                    version: "9.9.9".into(),
                    tag: "v9.9.9".into(),
                    title: "KeyFrameXL v9.9.9".into(),
                    notes: std::fs::read_to_string(file).unwrap_or_default(),
                    html_url: crate::updater::RELEASES_URL.into(),
                    asset_url: Some("https://example.com/keyframexl-linux-x86_64.tar.gz".into()),
                    sha256_url: None,
                    published_at: Some("2026-11-19".into()),
                });
                app.updater.install = match stage {
                    "download" => InstallState::Downloading {
                        downloaded: 7_340_032,
                        total: Some(18_874_368),
                    },
                    "ready" => InstallState::ReadyToRestart {
                        version: "9.9.9".into(),
                        exe_path: std::path::PathBuf::from("/opt/keyframexl/keyframexl"),
                    },
                    "failed" => InstallState::Failed("SHA-256 checksum mismatch.".into()),
                    _ => InstallState::Idle,
                };
                app.updater.dialog_open = true;
            }
            // KFXL_CAPTURE_EXPORT=path.wav opens the export dialog on that file.
            if let Ok(path) = std::env::var("KFXL_CAPTURE_EXPORT") {
                let song_mode = !app.song.clips.is_empty();
                app.open_export(std::path::PathBuf::from(path), song_mode);
            }
            if std::env::var("KFXL_CAPTURE_RECORD").is_ok() {
                app.toggle_arm(0);
                app.rec.on = true;
            }
            // KFXL_CAPTURE_SONG=1: Song mode; KFXL_CAPTURE_POS=bar puts the playhead there.
            if std::env::var_os("KFXL_CAPTURE_SONG").is_some() {
                app.set_song_mode(true);
            }
            if let Some(bar) = std::env::var("KFXL_CAPTURE_POS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
            {
                app.seek_tick(bar * keyframexl::sequence::BAR);
            }
            // KFXL_CAPTURE_INSTRUMENT=name: the channel the Roll and Instrument captures show.
            let named = std::env::var("KFXL_CAPTURE_INSTRUMENT").ok().and_then(|n| {
                app.song
                    .instruments
                    .iter()
                    .find(|i| i.name == n)
                    .map(|i| i.id)
            });
            if let Some(id) = named {
                app.roll_inst = id;
                app.instrument = id;
                for slot in &mut app.ws.slots {
                    for (_, tab) in slot.iter_all_tabs_mut() {
                        if let Tab::Roll { instrument } = tab {
                            *instrument = id;
                        }
                    }
                }
                if let Some(p) = app
                    .song
                    .patterns
                    .iter()
                    .find(|p| p.notes.iter().any(|n| n.instrument == id))
                {
                    app.selected = p.id;
                }
            }
            if kind == Some("--capture-roll") && !demo {
                // Show colour groups and a muted note.
                let bass = app.default_roll();
                let notes: Vec<usize> = app
                    .pattern()
                    .notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.instrument == bass)
                    .map(|(i, _)| i)
                    .collect();
                for (k, i) in notes.into_iter().enumerate() {
                    let n = &mut app.pattern_mut().notes[i];
                    n.color = (k % 3) as u8;
                    n.muted = k == 3;
                }
                let pid = app.selected;
                for c in 1..6u8 {
                    let note = keyframexl::sequence::Note {
                        color: c,
                        porta: c == 3,
                        ..keyframexl::sequence::Note::new(
                            bass,
                            c as i64 * 2 * keyframexl::sequence::STEP,
                            2 * keyframexl::sequence::STEP,
                            45 + c,
                            100,
                        )
                    };
                    app.song.add_note(pid, note);
                }
                app.tool = Tool::Paint;
            }
            if kind == Some("--capture-instrument") {
                let id = named.unwrap_or_else(|| app.default_roll());
                if std::env::var_os("KFXL_CAPTURE_SYNTH").is_some()
                    && let Some(inst) = app.song.instruments.iter_mut().find(|i| i.id == id)
                {
                    // KFXL_CAPTURE_SYNTH=N shows factory preset N.
                    let preset = std::env::var("KFXL_CAPTURE_SYNTH")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let mut patch = keyframexl::synth::SynthPatch::factory(preset);
                    // KFXL_CAPTURE_CURVES=1: bent envelope segments.
                    if std::env::var_os("KFXL_CAPTURE_CURVES").is_some() {
                        patch.amp_env = keyframexl::sampler::Adsr {
                            attack: 0.3,
                            decay: 0.6,
                            sustain: 0.4,
                            release: 0.8,
                            attack_curve: 0.7,
                            decay_curve: 0.8,
                            release_curve: 0.6,
                            ..Default::default()
                        };
                        patch.filter_env = keyframexl::sampler::Adsr {
                            attack: 0.5,
                            decay: 0.4,
                            sustain: 0.6,
                            release: 0.5,
                            attack_curve: -0.7,
                            release_curve: -0.6,
                            ..Default::default()
                        };
                    }
                    inst.kind = keyframexl::sequence::InstrumentKind::Synth { patch };
                    inst.name = "KeyFrame Tide".into();
                }
                // KFXL_CAPTURE_FORGE=N shows a Forge channel on recipe N.
                if let Some(n) = std::env::var("KFXL_CAPTURE_FORGE")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    && let Some(inst) = app.song.instruments.iter_mut().find(|i| i.id == id)
                {
                    use keyframexl::forge::recipes::Recipe;
                    let r = Recipe::ALL[n.min(Recipe::ALL.len() - 1)];
                    inst.kind = keyframexl::sequence::InstrumentKind::Forge {
                        sound: keyframexl::forge::ForgeSound::new(r.patch()),
                    };
                    inst.name = r.label().to_string();
                }
                app.win(Cmd::Show(Tab::Instrument { id }));
                if let Some(page) = std::env::var("KFXL_CAPTURE_PAGE")
                    .ok()
                    .and_then(|p| p.parse().ok())
                {
                    app.sampler_view.insert(
                        id,
                        sampler::SamplerUi {
                            page,
                            zone: Some(0),
                        },
                    );
                }
            }
            // KFXL_CAPTURE_UNSAVED=1 shows the "Save changes?" dialog.
            if std::env::var("KFXL_CAPTURE_UNSAVED").is_ok() {
                app.checkpoint();
                app.pending = Some(Pending::Quit);
            }
            // KFXL_CAPTURE_FILES=dir shows the browser's Files tab on that folder.
            if let Ok(dir) = std::env::var("KFXL_CAPTURE_FILES") {
                app.browser_tab = 1;
                app.files.roots = vec![std::path::PathBuf::from(&dir)];
                app.files.open.insert(std::path::PathBuf::from(&dir));
            }
            if kind == Some("--capture-mixer") && demo {
                app.mix_sel = 1;
                app.win(Cmd::Show(Tab::Mixer));
            }
            if kind == Some("--capture-mixer") && !demo {
                // A small mix to look at: drums through insert 1 with an EQ, a reverb send.
                app.song.instruments[0].mixer = 1;
                app.song.instruments[1].mixer = 1;
                app.song.mixer[1].name = "Drums".into();
                app.song.mixer[1]
                    .fx
                    .push(keyframexl::fx::FxSlot::new(keyframexl::fx::FxKind::ParamEq));
                app.song.mixer[1].fx.push(keyframexl::fx::FxSlot::new(
                    keyframexl::fx::FxKind::Compressor,
                ));
                app.song.mixer[1]
                    .sends
                    .push(keyframexl::sequence::Send { to: 2, level: 0.4 });
                app.song.mixer[2].name = "Reverb".into();
                app.song.mixer[2]
                    .fx
                    .push(keyframexl::fx::FxSlot::new(keyframexl::fx::FxKind::Reverb));
                app.song.mixer[1].eq = [3.0, -2.5, 4.0];
                app.solo_insert(1);
                app.song.mixer[2].sep = 0.5;
                app.toggle_arm(2);
                app.rec.on = true;
                app.mix_sel = 1;
                // KFXL_CAPTURE_GR: a hard-working compressor, its window open (GR meter).
                if std::env::var("KFXL_CAPTURE_GR").is_ok() {
                    app.song.mixer[1].fx[1].params[0] = -40.0;
                    app.song.mixer[1].fx[1].params[1] = 8.0;
                    app.open_fx_window(1, 1);
                }
                // KFXL_CAPTURE_SC: the hat on insert 4 ducks under the kick on insert 3.
                if std::env::var("KFXL_CAPTURE_SC").is_ok() {
                    use keyframexl::fx::{FxKind, FxSlot};
                    app.song.instruments[0].mixer = 3;
                    app.song.instruments[2].mixer = 4;
                    app.song.mixer[3].name = "Kick".into();
                    app.song.mixer[4].name = "Hat".into();
                    let mut comp = FxSlot::new(FxKind::Compressor);
                    comp.params[0] = -36.0;
                    comp.params[1] = 12.0;
                    comp.sidechain = Some(3);
                    app.song.mixer[4].fx.push(comp);
                    for m in &mut app.song.mixer {
                        m.muted = false;
                    }
                    app.mix_sel = 4;
                    app.open_fx_window(4, 0);
                }
                app.publish();
                app.win(Cmd::Show(Tab::Mixer));
                // KFXL_CAPTURE_FLOAT_MIXER=1: the mixer as a floating window.
                if std::env::var("KFXL_CAPTURE_FLOAT_MIXER").is_ok() {
                    app.win(Cmd::Float(Tab::Mixer));
                }
            }
            if kind == Some("--capture-float") {
                app.win(Cmd::Float(Tab::Roll {
                    instrument: app.default_roll(),
                }));
            }
        } else {
            app.ws = workspace::Workspace::load(app.default_roll());
            cc.egui_ctx.set_zoom_factor(
                app.settings
                    .general
                    .ui_scale
                    .clamp(crate::settings::UI_SCALE_MIN, crate::settings::UI_SCALE_MAX),
            );
            app.files = files::Files::new(true);
            app.layout_text = app.ws.to_text();
            app.persist = true;
            app.startup_maximized = app.settings.general.window_maximized;
            cc.egui_ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("decorations-applied"),
                    app.settings.general.system_title_bar,
                );
            });
            app.recent = project::load_recent();
            app.midi_in = Some(crate::midi_in::MidiIn::new(app.settings.midi.clone()));
            app.midi_out = Some(crate::midi_out::MidiOut::new());
            app.route_midi(Some(&cc.egui_ctx));
            app.start_plugins(Some(&cc.egui_ctx));
            // Look for FL Studio packs and preset folders without blocking the window.
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let home = dirs::home_dir().unwrap_or_default();
                let _ = tx.send(library::find(&home, &library::default_mounts(&home)));
            });
            app.libraries_rx = Some(rx);
        }
        // A project file on the command line (double-click / Open With in the
        // file manager): open it instead of the default empty project.
        let mut opened_cli = false;
        if !capture
            && let Some(arg) = args.iter().skip(1).find(|a| !a.starts_with('-'))
            && (arg.to_ascii_lowercase().ends_with(".kfxl")
                || arg.to_ascii_lowercase().ends_with(".flp"))
        {
            let p = std::path::PathBuf::from(arg);
            if p.is_file() {
                app.open_path(&p);
                opened_cli = true;
            }
        }
        app.welcome_open = (!capture && !opened_cli && app.settings.general.show_welcome)
            || (capture && std::env::var_os("KFXL_CAPTURE_WELCOME").is_some());
        // KFXL_CAPTURE_WELCOME=genres opens it on the Genres tab.
        // KFXL_CAPTURE_LANE=pitch: the roll's lane shows pitch curves.
        if capture && std::env::var("KFXL_CAPTURE_LANE").as_deref() == Ok("pitch") {
            app.lane_target = roll::LaneTarget::Pitch;
        }
        if capture && std::env::var("KFXL_CAPTURE_WELCOME").as_deref() == Ok("genres") {
            app.welcome.tab = welcome::WelcomeTab::Genres;
        }
        // KFXL_CAPTURE_MANUAL=N opens the manual on chapter N; KFXL_CAPTURE_SHORTCUTS=1 the F1 list.
        if capture && let Ok(n) = std::env::var("KFXL_CAPTURE_MANUAL") {
            app.show_manual = true;
            let n: usize = n.parse().unwrap_or(0);
            cc.egui_ctx
                .data_mut(|d| d.insert_temp(egui::Id::new("manual-page"), n));
        }
        if capture && std::env::var_os("KFXL_CAPTURE_SHORTCUTS").is_some() {
            app.show_shortcuts = true;
        }
        // Live checks: KFXL_CAPTURE_MIDI_IN=1 listens to MIDI keyboards,
        // KFXL_CAPTURE_PLUGINS=1 starts the plugin host and scan.
        if capture && std::env::var_os("KFXL_CAPTURE_MIDI_IN").is_some() {
            app.midi_in = Some(crate::midi_in::MidiIn::new(app.settings.midi.clone()));
            app.route_midi(Some(&cc.egui_ctx));
        }
        if capture && std::env::var_os("KFXL_CAPTURE_PLUGINS").is_some() {
            app.start_plugins(Some(&cc.egui_ctx));
        }
        // A capture of an imported FL project goes through the same import
        // as File → Open (samples, plugins and all; with
        // KFXL_CAPTURE_PLUGINS=1 the plugins load too).
        if capture
            && let Ok(path) = std::env::var("KFXL_CAPTURE_PROJECT")
            && path.to_ascii_lowercase().ends_with(".flp")
        {
            app.open_path(std::path::Path::new(&path));
        }
        // KFXL_CAPTURE_PLUGIN=bundle.vst3 adds it as a channel playing a chord
        // pattern while the demo's own channels are muted (live plugin check);
        // KFXL_CAPTURE_PLUGIN_FX=bundle.vst3 puts an effect plugin on Master.
        if capture && let Ok(path) = std::env::var("KFXL_CAPTURE_PLUGIN") {
            if app.plugins.host.is_none() {
                app.start_plugins(Some(&cc.egui_ctx));
            }
            for i in &mut app.song.instruments {
                i.muted = true;
            }
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let pid = app.song.next_plugin_id();
            let plugin = keyframexl::sequence::PluginRef {
                id: pid,
                format: crate::plugins::format_of(std::path::Path::new(&path)).into(),
                path,
                name: name.clone(),
                vendor: String::new(),
                state: String::new(),
                outs: vec![],
            };
            let id = app.song.add_channel(
                &name,
                keyframexl::sequence::InstrumentKind::Plugin { plugin },
            );
            let pat = app.selected;
            use keyframexl::sequence::{Note, STEP};
            for (k, pitch) in [60u8, 64, 67, 72].into_iter().enumerate() {
                app.song.add_note(
                    pat,
                    Note::new(id, k as i64 * 4 * STEP, 3 * STEP, pitch, 100),
                );
            }
            app.rack_sel = vec![id];
            // The plugin is the channel the Instrument capture shows.
            app.instrument = id;
            app.roll_inst = id;
            for cmd in &mut app.win_cmds {
                if let Cmd::Show(Tab::Instrument { id: shown }) = cmd {
                    *shown = id;
                }
            }
            if std::env::var_os("KFXL_CAPTURE_PLUGIN_EDITOR").is_some() {
                app.plugins_frame();
                if let Some(l) = app.plugins.live.get_mut(&pid) {
                    l.show_when_loaded = true;
                }
            }
            if std::env::var_os("KFXL_CAPTURE_PLUGIN_PARAMS").is_some() {
                app.plugins
                    .page
                    .insert(pid, plugins_ui::WrapPage::Parameters);
            }
            if std::env::var_os("KFXL_CAPTURE_PLUGIN_SETTINGS").is_some() {
                app.plugins.page.insert(pid, plugins_ui::WrapPage::Settings);
            }
            app.publish();
        }
        if capture && let Ok(path) = std::env::var("KFXL_CAPTURE_PLUGIN_FX") {
            if app.plugins.host.is_none() {
                app.start_plugins(Some(&cc.egui_ctx));
            }
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let pid = app.song.next_plugin_id();
            let plugin = keyframexl::sequence::PluginRef {
                id: pid,
                format: crate::plugins::format_of(std::path::Path::new(&path)).into(),
                path,
                name,
                vendor: String::new(),
                state: String::new(),
                outs: vec![],
            };
            app.song.mixer[0]
                .fx
                .push(keyframexl::fx::FxSlot::plugin(plugin));
            app.mix_sel = 0;
            app.mix_slot = app.song.mixer[0].fx.len() - 1;
            app.publish();
        }
        // KFXL_CAPTURE_METRONOME=1 clicks with the demo's channels muted;
        // KFXL_CAPTURE_INPUT=1 records the audio input (take written on exit).
        if capture && std::env::var_os("KFXL_CAPTURE_METRONOME").is_some() {
            for i in &mut app.song.instruments {
                i.muted = true;
            }
            app.publish();
            app.toggle_metronome();
        }
        // KFXL_CAPTURE_AUTOMATION: an Insert 1 pan clip with a sweep, editor open.
        if capture && std::env::var_os("KFXL_CAPTURE_AUTOMATION").is_some() {
            app.create_automation(keyframexl::automation::Target::MixerPan { track: 1 });
            if let Some(p) = app.song.patterns.last_mut().map(std::sync::Arc::make_mut)
                && let Some(e) = &mut p.automation
            {
                use keyframexl::automation::Point;
                let bar = keyframexl::sequence::BAR;
                e.points = vec![
                    Point {
                        tick: 0,
                        value: -1.0,
                        tension: 0.0,
                    },
                    Point {
                        tick: bar,
                        value: 1.0,
                        tension: 0.5,
                    },
                    Point {
                        tick: 2 * bar,
                        value: -0.4,
                        tension: -0.5,
                    },
                    Point {
                        tick: 4 * bar,
                        value: 0.6,
                        tension: 0.0,
                    },
                ];
            }
            // `closed`: only the clip in the playlist.
            if std::env::var("KFXL_CAPTURE_AUTOMATION").is_ok_and(|v| v == "closed") {
                app.auto_open = None;
            }
        }
        // KFXL_CAPTURE_SCALE=root,kind (indices into scale::ROOTS / Kind::ALL).
        if capture
            && let Ok(v) = std::env::var("KFXL_CAPTURE_SCALE")
            && let Some((r, k)) = v.split_once(',')
            && let (Ok(r), Ok(k)) = (r.parse::<u8>(), k.parse::<usize>())
        {
            let kind = keyframexl::scale::Kind::ALL[k.min(keyframexl::scale::Kind::ALL.len() - 1)];
            app.roll_scale = Some(keyframexl::scale::Scale { root: r % 12, kind });
        }
        if capture && std::env::var_os("KFXL_CAPTURE_INPUT").is_some() {
            app.rec.input = true;
            app.rec.on = true;
        }
        // KFXL_CAPTURE_SETTINGS=audio|midi|plugins|general opens that page;
        // KFXL_CAPTURE_FX_WINDOW=track,slot (slot may be `last`) opens an effect's window.
        if capture && let Ok(page) = std::env::var("KFXL_CAPTURE_SETTINGS") {
            let page = match page.as_str() {
                "midi" => settings_ui::Page::Midi,
                "plugins" => settings_ui::Page::Plugins,
                "general" => settings_ui::Page::General,
                _ => settings_ui::Page::Audio,
            };
            app.open_settings(page);
        }
        if capture
            && let Ok(v) = std::env::var("KFXL_CAPTURE_FX_WINDOW")
            && let Some((t, k)) = v.split_once(',')
            && let Ok(t) = t.parse::<usize>()
            && let Some(k) = if k == "last" {
                app.song
                    .mixer
                    .get(t)
                    .and_then(|m| m.fx.len().checked_sub(1))
            } else {
                k.parse().ok()
            }
        {
            app.open_fx_window(t, k);
        }
        if capture {
            app.capture = Some((
                args.get(2)
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| "capture.png".into()),
                0,
            ));
        }
        let show_view = match std::env::var("KFXL_SHOW_VIEW").as_deref() {
            Ok("notes") | Ok("fall") => show::ShowView::Notes,
            _ => show::ShowView::Arrange,
        };
        if kind == Some("--capture-show") {
            app.enter_show(show_view, Some(&cc.egui_ctx));
            if let Some(st) = &mut app.show {
                st.hud = std::env::var_os("KFXL_SHOW_HUD").is_some();
            }
            let tick_override = std::env::var("KFXL_SHOW_TICK")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .or_else(|| {
                    std::env::var("KFXL_CAPTURE_POS")
                        .ok()
                        .and_then(|v| v.parse::<i64>().ok())
                        .map(|bar| bar * keyframexl::sequence::BAR)
                });
            if let Some(tk) = tick_override {
                let f = app.tick_frame_at(tk.max(0));
                if let Some(st) = &mut app.show {
                    st.clock = show::ShowClock::Fixed(f);
                }
            }
        } else if !capture && args.iter().skip(1).any(|a| a == "--show") {
            if !opened_cli {
                app.open_demo_song();
            }
            app.enter_show(show_view, Some(&cc.egui_ctx));
        }
        app
    }

    pub(crate) fn tr(&self) -> &'static crate::i18n::Strings {
        crate::i18n::strings(self.settings.general.language)
    }

    pub(crate) fn with_song(song: Song, audio: Option<Audio>) -> Self {
        let selected = song.patterns.first().map(|p| p.id).unwrap_or(1);
        let roll_scale = song.info.scale;
        Self {
            song,
            history: Vec::new(),
            future: Vec::new(),
            ws: workspace::Workspace::new(BASS),
            win_cmds: Vec::new(),
            focus: None,
            persist: false,
            startup_maximized: false,
            startup_maximize_nudge_done: false,
            startup_maximize_frames: 0,
            layout_text: String::new(),
            layout_saved_at: 0.0,
            selected,
            instrument: BASS,
            roll_inst: BASS,
            roll_view: Default::default(),
            roll_sel: Vec::new(),
            riff: None,
            midi_zones: Vec::new(),
            drop_pointer: None,
            last_len: None,
            lane_target: roll::LaneTarget::Velocity,
            note_clip: Vec::new(),
            ghosts: true,
            erasing: false,
            rack_sel: Vec::new(),
            rack_graph: false,
            mix_sel: 0,
            mix_slot: 0,
            route_drag: None,
            mix_strip_rects: Vec::new(),
            mix_solo: None,
            note_color: 0,
            porta_new: false,
            roll_scale,
            join_preset: None,
            auto_open: None,
            auto_sent: std::collections::HashMap::new(),
            last_tweaked: None,
            auto_clipboard: None,
            multilink: None,
            auto_take: Default::default(),
            scale_snap: false,
            rec: Default::default(),
            libraries: Default::default(),
            libraries_rx: None,
            held: None,
            forge_jobs: Default::default(),
            live_id: 0,
            capture_midi: None,
            key_down: None,
            export: None,
            export_prefs: Default::default(),
            meters: Vec::new(),
            meter_at: std::time::Instant::now(),
            browser_tab: 0,
            files: files::Files::new(false),
            previewing: None,
            preview_rx: None,
            sampler_view: Default::default(),
            clip: None,
            tool: Tool::Draw,
            snap: Snap::Step,
            velocity: 100,
            song_mode: false,
            follow: true,
            status: (Tone::Quiet, String::new()),
            roll_gesture: None,
            tap_win: None,
            audio,
            capture: None,
            script: None,
            path: None,
            edits: 0,
            saved_edits: 0,
            renaming: None,
            show_shortcuts: false,
            stretch_jobs: std::sync::mpsc::channel(),
            stretch_busy: Default::default(),
            show_manual: false,
            pending: None,
            quit_ok: false,
            relaunch: None,
            recent: Vec::new(),
            title: String::new(),
            autosaved: (0, 0.0),
            cjk_font: false,
            vocal_job: None,
            lyric_edit: None,
            lyrics_dialog: None,
            importing: Vec::new(),
            load_queue: Default::default(),
            flp_slices: Default::default(),
            flp_freeze: None,
            flp_freeze_for: Vec::new(),
            settings: Default::default(),
            settings_ui: None,
            midi_in: None,
            midi_out: None,
            midi_keys: [false; 128],
            midi_rec: Default::default(),
            midi_take: false,
            midi_learn: None,
            midi_cc_last: None,
            updater: Default::default(),
            welcome: Default::default(),
            welcome_open: false,
            import_dialog: None,
            plugins: Default::default(),
            fx_windows: Vec::new(),
            waiting_input: false,
            show: None,
            open_info_next: false,
            #[cfg(test)]
            probe: Probe::default(),
        }
    }

    // ------------------------------------------------------------ state

    pub(crate) fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.status = (tone, text.into());
    }

    pub(crate) fn checkpoint(&mut self) {
        self.edits += 1;
        self.history.push(self.song.clone());
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.future.clear();
    }

    pub(crate) fn dirty(&self) -> bool {
        self.edits != self.saved_edits
    }

    pub(crate) fn undo(&mut self) -> bool {
        let tr = self.tr();
        self.settle_plugin_states();
        if let Some(prev) = self.history.pop() {
            self.edits += 1;
            self.future.push(std::mem::replace(&mut self.song, prev));
            if let Some(left) = self.future.pop() {
                self.restore_plugin_states(&left);
                self.future.push(left);
            }
            self.fix_selection();
            self.publish();
            self.say(Tone::Quiet, tr.status_undone);
            true
        } else {
            self.say(Tone::Quiet, tr.edit_nothing_undo);
            false
        }
    }

    pub(crate) fn redo(&mut self) -> bool {
        let tr = self.tr();
        self.settle_plugin_states();
        if let Some(next) = self.future.pop() {
            self.edits += 1;
            self.history.push(std::mem::replace(&mut self.song, next));
            if let Some(left) = self.history.pop() {
                self.restore_plugin_states(&left);
                self.history.push(left);
            }
            self.fix_selection();
            self.publish();
            self.say(Tone::Quiet, tr.status_redone);
            true
        } else {
            self.say(Tone::Quiet, tr.edit_nothing_redo);
            false
        }
    }

    fn fix_selection(&mut self) {
        if !self.song.patterns.iter().any(|p| p.id == self.selected) {
            self.selected = self.song.patterns[0].id;
        }
        if self.clip.is_some_and(|i| i >= self.song.clips.len()) {
            self.clip = None;
        }
    }

    pub(crate) fn publish(&self) {
        if let Some(a) = &self.audio {
            a.send_song(self.song.clone());
            a.pattern.store(self.selected, Ordering::Relaxed);
        }
    }

    /// Stream a single live parameter tweak lock-free over `rtrb` without cloning the song.
    /// Falls back to a full song snapshot only if the parameter ring is full.
    pub(crate) fn publish_param(&self, target: keyframexl::automation::Target, value: f32) {
        if let Some(a) = &self.audio
            && !a.send_param(target, value)
        {
            a.send_song(self.song.clone());
        }
    }

    pub(crate) fn pattern(&self) -> &keyframexl::sequence::Pattern {
        self.song
            .patterns
            .iter()
            .find(|p| p.id == self.selected)
            .expect("selected pattern exists")
    }

    pub(crate) fn pattern_mut(&mut self) -> &mut keyframexl::sequence::Pattern {
        let id = self.selected;
        self.song.pattern_mut(id).expect("selected pattern exists")
    }

    pub(crate) fn pattern_index(&self, id: u64) -> usize {
        self.song
            .patterns
            .iter()
            .position(|p| p.id == id)
            .unwrap_or(0)
    }

    pub(crate) fn select_pattern(&mut self, id: u64) {
        self.selected = id;
        self.publish();
    }

    pub(crate) fn new_pattern(&mut self) {
        let tr = self.tr();
        self.checkpoint();
        let id = self.song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        self.song
            .patterns
            .push(keyframexl::sequence::Pattern::new(id, &format!("Pattern {id}")).into());
        // Keep the new pattern right after the current one.
        let at = self.pattern_index(self.selected) + 1;
        let p = self.song.patterns.pop().expect("pushed");
        self.song
            .patterns
            .insert(at.min(self.song.patterns.len()), p);
        self.selected = id;
        self.publish();
        self.say(
            Tone::Quiet,
            crate::i18n::fill(tr.status_pattern_added, &[("id", &id.to_string())]),
        );
    }

    /// FL F4: the next pattern without notes after the current one, else a new one.
    pub(crate) fn next_empty_pattern(&mut self) {
        let tr = self.tr();
        let from = self.pattern_index(self.selected);
        let empty = self
            .song
            .patterns
            .iter()
            .skip(from + 1)
            .find(|p| p.notes.is_empty() && p.slides.is_empty() && p.automation.is_none())
            .map(|p| p.id);
        match empty {
            Some(id) => {
                self.select_pattern(id);
                let name = self.pattern().name.clone();
                self.say(
                    Tone::Quiet,
                    crate::i18n::fill(tr.status_pattern_empty_selected, &[("name", &name)]),
                );
            }
            None => self.new_pattern(),
        }
    }

    pub(crate) fn start_rename(&mut self) {
        self.renaming = Some((self.selected, self.pattern().name.clone()));
    }

    pub(crate) fn clone_pattern(&mut self) {
        let tr = self.tr();
        self.checkpoint();
        let mut p = self.pattern().clone();
        p.id = self.song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        p.name = format!("{} copy", p.name);
        p.renumber_notes(self.song.next_note_id());
        let at = self.pattern_index(self.selected) + 1;
        let (id, name) = (p.id, p.name.clone());
        self.song.patterns.insert(at, p.into());
        self.select_pattern(id);
        self.say(
            Tone::Quiet,
            crate::i18n::fill(tr.status_pattern_copy_added, &[("name", &name)]),
        );
    }

    /// Delete the selected pattern and its clips. The last pattern stays.
    pub(crate) fn delete_pattern(&mut self) {
        let tr = self.tr();
        if self.song.patterns.len() < 2 {
            self.say(Tone::Bad, tr.status_pattern_last_stays);
            return;
        }
        self.checkpoint();
        let id = self.selected;
        let at = self.pattern_index(id);
        let name = self.pattern().name.clone();
        let clips = self.song.clips.iter().filter(|c| c.pattern == id).count();
        self.song.patterns.remove(at);
        self.song.clips.retain(|c| c.pattern != id);
        self.clip = None;
        self.selected = self.song.patterns[at.min(self.song.patterns.len() - 1)].id;
        self.publish();
        self.say(
            Tone::Quiet,
            tr.pattern_deleted_clips
                .fmt_with(tr.lang, clips, &[("name", &name)]),
        );
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|a| a.playing.load(Ordering::Relaxed))
    }

    pub(crate) fn position_frames(&self) -> u64 {
        self.audio
            .as_ref()
            .map(|a| a.position.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub(crate) fn frames_per_step(&self) -> f64 {
        keyframexl::sequence::seconds_per_step(self.song.tempo) * self.rate() as f64
    }

    /// Step since the loop start, for the playhead and the LCD. The audio
    /// position itself runs monotonically (MIDI takes outgrow one loop), so
    /// the display wraps it by the current loop length.
    pub(crate) fn position_step(&self) -> usize {
        (self.frame_tick_at(self.loop_pos()) / keyframexl::sequence::STEP).max(0) as usize
    }

    /// The transport's loop as frames: the song's time selection in Song
    /// mode when there is one, else the whole pattern or song.
    pub(crate) fn loop_bounds(&self) -> (u64, u64) {
        let table = self.tempo_table();
        keyframexl::sequence::loop_bounds(
            &self.song,
            table.as_ref(),
            self.selected,
            self.song_mode,
            self.rate(),
        )
    }

    /// The audio position folded into the loop, as the engine plays it.
    pub(crate) fn loop_pos(&self) -> u64 {
        keyframexl::sequence::wrap_frame(self.position_frames(), self.loop_bounds())
    }

    /// The song's tempo map as frames, when it plays in Song mode with one.
    pub(crate) fn tempo_table(&self) -> Option<keyframexl::tempo::TempoTable> {
        (self.song_mode && !self.song.tempo_map.is_empty()).then(|| {
            keyframexl::tempo::TempoTable::build(&self.song.tempo_map, self.song.tempo, self.rate())
        })
    }

    /// Frames since the start → tick, through the tempo map when there is one.
    pub(crate) fn frame_tick_at(&self, frame: u64) -> keyframexl::sequence::Tick {
        match self.tempo_table() {
            Some(t) => t.tick_at_frame(frame),
            None => {
                (frame as f64 / self.frames_per_step() * keyframexl::sequence::STEP as f64)
                    as keyframexl::sequence::Tick
            }
        }
    }

    /// Tick → frames since the start, through the tempo map when there is one.
    pub(crate) fn tick_frame_at(&self, t: keyframexl::sequence::Tick) -> u64 {
        match self.tempo_table() {
            Some(table) => table.frames_at(t) as u64,
            None => (keyframexl::sequence::steps(t) * self.frames_per_step()) as u64,
        }
    }

    /// The playhead as a song tick (Song mode, wrapped by the song length).
    pub(crate) fn playhead_tick(&self) -> keyframexl::sequence::Tick {
        self.frame_tick_at(self.loop_pos())
    }

    /// The tempo the transport edits: the song's, or with a tempo map the
    /// event in force at the playhead (FL: the tempo knob edits that event).
    pub(crate) fn edited_tempo(&self) -> f64 {
        if self.tempo_table().is_none() {
            return self.song.tempo;
        }
        let at = self.playhead_tick();
        let m = &self.song.tempo_map.points;
        m.iter()
            .rposition(|p| p.tick <= at)
            .map(|i| m[i].bpm)
            .unwrap_or(self.song.tempo)
    }

    /// Write the transport's tempo (see `edited_tempo`). Inside a ramp, the
    /// ramp splits: a step point at the playhead takes the new value.
    pub(crate) fn set_edited_tempo(&mut self, bpm: f64) {
        if self.tempo_table().is_none() {
            self.song.tempo = bpm;
            return;
        }
        let at = self.playhead_tick();
        self.set_tempo_at(at, bpm);
    }

    fn set_tempo_at(&mut self, at: keyframexl::sequence::Tick, bpm: f64) {
        let m = &mut self.song.tempo_map.points;
        match m.iter().rposition(|p| p.tick <= at) {
            Some(i) if m[i].tick == at || m[i].shape == keyframexl::tempo::TempoShape::Step => {
                m[i].bpm = bpm
            }
            Some(i) => m.insert(
                i + 1,
                keyframexl::tempo::TempoPoint {
                    tick: at,
                    bpm,
                    shape: keyframexl::tempo::TempoShape::Step,
                },
            ),
            None => self.song.tempo = bpm,
        }
        if m.first().is_some_and(|p| p.tick == 0) {
            self.song.tempo = m[0].bpm;
        }
    }

    /// Tempo at the playhead (the map's when it has one).
    pub(crate) fn tempo_now(&self) -> f64 {
        match self.tempo_table() {
            Some(t) => t.bpm_at(t.tick_at_frame(self.loop_pos())),
            None => self.song.tempo,
        }
    }

    /// Current step inside the selected pattern while playing.
    pub(crate) fn playing_step(&self) -> Option<usize> {
        let n = self.pattern().step_count().max(1);
        self.is_playing().then(|| self.position_step() % n)
    }

    /// Move the playhead to tick `t` of the loop (pattern or song).
    pub(crate) fn seek_tick(&mut self, t: keyframexl::sequence::Tick) {
        let tr = self.tr();
        let frames = self.tick_frame_at(t);
        if let Some(a) = &self.audio {
            a.position.store(frames, Ordering::Relaxed);
        }
        let at = if self.song_mode {
            playlist::song_bar_pos(&self.song.meters, t)
        } else {
            roll::bar_pos(t)
        };
        self.say(
            Tone::Quiet,
            crate::i18n::fill(tr.status_playhead_at, &[("pos", &at)]),
        );
    }

    pub(crate) fn play_pause(&mut self) {
        let tr = self.tr();
        let (start, end) = self.loop_bounds();
        let at = self.loop_pos();
        let g = self.settings.general.clone();
        let bar = self.count_in_frames();
        let Some(a) = &self.audio else { return };
        let on = !a.playing.load(Ordering::Relaxed);
        // Play again while counting in or waiting: cancel, stay stopped.
        if self.waiting_input || a.count_in.load(Ordering::Relaxed) > 0 {
            a.count_in.store(0, Ordering::Relaxed);
            self.waiting_input = false;
            self.say(Tone::Quiet, tr.status_start_cancelled);
            return;
        }
        // With a time selection, play starts inside it (FL).
        if on && !(start..end).contains(&at) {
            a.position.store(start, Ordering::Relaxed);
        }
        if on && self.rec.on && g.wait_input {
            self.waiting_input = true;
            self.say(Tone::Wait, tr.status_waiting_input);
            return;
        }
        if on && g.count_in && (g.count_in_always || self.rec.on) {
            a.count_in_len.store(bar, Ordering::Relaxed);
            a.count_in.store(bar, Ordering::Relaxed);
            self.say(
                Tone::Wait,
                if self.rec.on {
                    tr.status_count_in_rec
                } else {
                    tr.status_count_in_play
                },
            );
            return;
        }
        a.playing.store(on, Ordering::Relaxed);
    }

    /// Frames in the bar the count-in clicks (the first bar's meter, the song tempo).
    pub(crate) fn count_in_frames(&self) -> u64 {
        let (_, _, blen, _, _) = keyframexl::tempo::bar_at(&self.song.meters, 0);
        let per_tick = 60.0 / self.song.tempo.clamp(10.0, 999.0)
            * self
                .audio
                .as_ref()
                .map_or(self.settings.audio.rate, |a| a.running.rate) as f64
            / keyframexl::sequence::PPQ as f64;
        (blen as f64 * per_tick).round().max(1.0) as u64
    }

    /// The first note while waiting for input starts the transport.
    pub(crate) fn input_arrived(&mut self) {
        if !self.waiting_input {
            return;
        }
        let tr = self.tr();
        self.waiting_input = false;
        if let Some(a) = &self.audio {
            a.playing.store(true, Ordering::Relaxed);
        }
        self.say(Tone::Quiet, tr.status_rec_from_first_note);
    }

    pub(crate) fn stop(&mut self) {
        // Stop rewinds to the loop's start: bar 1, or the time selection.
        let (start, _) = self.loop_bounds();
        self.waiting_input = false;
        if let Some(a) = &self.audio {
            a.count_in.store(0, Ordering::Relaxed);
            a.playing.store(false, Ordering::Relaxed);
            a.position.store(start, Ordering::Relaxed);
        }
    }

    pub(crate) fn set_song_mode(&mut self, on: bool) {
        if self.song_mode == on {
            return;
        }
        self.song_mode = on;
        if let Some(a) = &self.audio {
            a.song_mode.store(on, Ordering::Relaxed);
            a.position.store(0, Ordering::Relaxed);
        }
    }

    pub(crate) fn open_roll(&mut self, instrument: u64) {
        self.instrument = instrument;
        self.win(Cmd::Show(Tab::Roll { instrument }));
    }

    pub(crate) fn focus_kind(&self) -> Option<Kind> {
        self.focus.as_ref().map(Tab::kind)
    }

    // ------------------------------------------------------------ files

    /// Save to the project's file; ask for one the first time.
    pub(crate) fn save(&mut self) {
        match self.path.clone() {
            Some(path) => self.save_to(path),
            None => self.save_as(),
        }
    }

    /// FL's Save new version: `beat.kfxl` → `beat_2.kfxl`, `beat_2` → `beat_3`;
    /// never overwrites. An unsaved project asks for a name first.
    pub(crate) fn save_new_version(&mut self) {
        let Some(path) = self.path.clone() else {
            self.save_as();
            return;
        };
        let target = next_version(&path);
        self.save_to(target);
    }

    pub(crate) fn save_as(&mut self) {
        let tr = self.tr();
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project.kfxl".into());
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.dlg_filter_kfxl, &["kfxl"])
            .set_file_name(name)
            .save_file()
        {
            self.save_to(path);
        }
    }

    fn save_to(&mut self, path: std::path::PathBuf) {
        let tr = self.tr();
        // Plugins keep their sound settings inside themselves: fetch them first.
        self.collect_plugin_states();
        match keyframexl::project_io::save(&self.song, &path) {
            Ok(()) => {
                self.say(
                    Tone::Done,
                    crate::i18n::fill(tr.status_saved, &[("path", &path.display().to_string())]),
                );
                self.remember(&path);
                self.path = Some(path);
                self.saved_edits = self.edits;
            }
            Err(e) => self.say(
                Tone::Bad,
                crate::i18n::fill(tr.status_not_saved, &[("err", &e.to_string())]),
            ),
        }
    }

    pub(crate) fn new_project(&mut self) {
        let tr = self.tr();
        self.flp_freeze = None;
        self.close_riff();
        self.unload_all_plugins();
        self.checkpoint();
        self.song = Song::default();
        self.selected = self.song.patterns[0].id;
        self.roll_scale = None;
        self.open_info_next = false;
        self.clip = None;
        self.path = None;
        self.saved_edits = self.edits;
        self.publish();
        self.say(Tone::Quiet, tr.status_new_project);
        self.load_default_kit();
    }

    pub(crate) fn export_project_bundle(&mut self) {
        let tr = self.tr();
        let stem = self.project_name();
        let default_name = format!("{stem}.zip");
        let Some(dest) = rfd::FileDialog::new()
            .add_filter(tr.dlg_filter_bundle, &["zip"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        self.collect_plugin_states();
        let opts = keyframexl::bundle::BundleOptions::default();
        match keyframexl::bundle::export_bundle(&self.song, &stem, &dest, &opts) {
            Ok(report) => {
                let mut parts = vec![crate::i18n::fill(
                    tr.status_bundled,
                    &[("path", &dest.display().to_string())],
                )];
                if report.bundled_irs > 0 {
                    parts.push(tr.bundled_irs.fmt(tr.lang, report.bundled_irs));
                }
                if report.plugins > 0 {
                    parts.push(tr.bundled_plugin_refs.fmt(tr.lang, report.plugins));
                }
                if !report.missing_irs.is_empty() {
                    parts.push(
                        tr.bundled_missing_irs
                            .fmt(tr.lang, report.missing_irs.len()),
                    );
                }
                let tone = if report.missing_irs.is_empty() {
                    Tone::Done
                } else {
                    Tone::Wait
                };
                self.say(tone, parts.join(" · "));
            }
            Err(e) => self.say(
                Tone::Bad,
                crate::i18n::fill(tr.status_bundle_export_failed, &[("err", &e.to_string())]),
            ),
        }
    }

    pub(crate) fn import_project_bundle(&mut self) {
        let tr = self.tr();
        let Some(zip_path) = rfd::FileDialog::new()
            .add_filter(tr.dlg_filter_bundle, &["zip"])
            .pick_file()
        else {
            return;
        };
        self.request(Pending::Open(Some(zip_path)));
    }

    fn import_bundle_path(&mut self, zip_path: &std::path::Path) {
        let tr = self.tr();
        let stem = zip_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "project_bundle".into());
        let parent = zip_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let extract_dir = parent.join(&stem);
        match keyframexl::bundle::import_bundle(zip_path, &extract_dir) {
            Ok(imported) => {
                self.flp_freeze = None;
                self.close_riff();
                self.unload_all_plugins();
                self.checkpoint();
                self.selected = imported.song.patterns[0].id;
                self.roll_scale = imported.song.info.scale;
                self.open_info_next = imported.song.info.show_on_open;
                self.song = imported.song;
                self.clip = None;
                self.path = Some(imported.project_path.clone());
                self.saved_edits = self.edits;
                self.publish();
                self.remember(&imported.project_path);
                let extra = if imported.restored_irs > 0 {
                    tr.bundle_restored_irs.fmt(tr.lang, imported.restored_irs)
                } else {
                    String::new()
                };
                self.say(
                    Tone::Done,
                    crate::i18n::fill(
                        tr.status_bundle_unpacked,
                        &[
                            ("path", &extract_dir.display().to_string()),
                            ("extra", &extra),
                        ],
                    ),
                );
            }
            Err(e) => self.say(
                Tone::Bad,
                crate::i18n::fill(tr.status_bundle_open_failed, &[("err", &e.to_string())]),
            ),
        }
    }

    pub(crate) fn open(&mut self) {
        let tr = self.tr();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.dlg_filter_open_all, &["kfxl", "zip", "flp"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    pub(crate) fn open_path(&mut self, path: &std::path::Path) {
        let tr = self.tr();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("flp"))
        {
            return self.import_flp(path);
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            return self.import_bundle_path(path);
        }
        match keyframexl::project_io::open(path) {
            Ok(s) => {
                self.flp_freeze = None;
                self.close_riff();
                self.unload_all_plugins();
                self.checkpoint();
                self.selected = s.patterns[0].id;
                self.roll_scale = s.info.scale;
                self.open_info_next = s.info.show_on_open;
                self.song = s;
                self.clip = None;
                self.path = Some(path.to_path_buf());
                self.saved_edits = self.edits;
                self.publish();
                self.say(
                    Tone::Done,
                    crate::i18n::fill(tr.status_opened, &[("path", &path.display().to_string())]),
                );
                self.remember(path);
            }
            Err(e) => {
                self.recent.retain(|p| p != path);
                self.say(
                    Tone::Bad,
                    crate::i18n::fill(
                        tr.status_cannot_open,
                        &[
                            ("path", &path.display().to_string()),
                            ("err", &e.to_string()),
                        ],
                    ),
                );
            }
        }
    }

    pub(crate) fn import_flp_dialog(&mut self) {
        let tr = self.tr();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.dlg_filter_flp, &["flp"])
            .pick_file()
        {
            self.request(Pending::Open(Some(path)));
        }
    }

    fn load_welcome_song(
        &mut self,
        song: Song,
        status: impl Into<String>,
        with_default_kit: bool,
        open_instrument: Option<u64>,
    ) {
        self.flp_freeze = None;
        self.close_riff();
        self.unload_all_plugins();
        self.checkpoint();
        self.selected = song.patterns.first().map(|p| p.id).unwrap_or(1);
        if let Some(id) = open_instrument.or_else(|| song.instruments.first().map(|i| i.id)) {
            self.instrument = id;
            self.roll_inst = id;
        }
        self.roll_scale = song.info.scale;
        self.open_info_next = false;
        self.song = song;
        self.clip = None;
        self.path = None;
        self.saved_edits = self.edits;
        self.publish();
        if with_default_kit {
            self.load_default_kit();
        }
        if let Some(id) = open_instrument {
            self.win(Cmd::Show(Tab::Instrument { id }));
        }
        self.say(Tone::Done, status);
    }

    pub(super) fn welcome_frame(&mut self, ctx: &egui::Context) {
        if !self.welcome_open {
            return;
        }
        let tr = self.tr();
        let update_version = match &self.updater.state {
            crate::updater::CheckState::Available(info)
                if !crate::updater::is_skipped(
                    self.settings.general.skipped_version.as_deref(),
                    &info.version,
                ) =>
            {
                Some(info.version.clone())
            }
            _ => None,
        };
        let was_show = self.settings.general.show_welcome;
        let action = welcome::show_welcome(
            ctx,
            &mut self.welcome,
            tr,
            &self.recent,
            &mut self.settings.general.show_welcome,
            update_version.as_deref(),
        );
        if self.settings.general.show_welcome != was_show {
            self.save_settings();
        }
        let Some(action) = action else {
            return;
        };
        self.welcome_open = false;
        match action {
            welcome::WelcomeAction::Close => {}
            welcome::WelcomeAction::NewProject => self.request(Pending::New),
            welcome::WelcomeAction::OpenProject => self.request(Pending::Open(None)),
            welcome::WelcomeAction::OpenRecent(path) => self.request(Pending::Open(Some(path))),
            welcome::WelcomeAction::ImportBundle => self.import_project_bundle(),
            welcome::WelcomeAction::ImportFlp => self.import_flp_dialog(),
            welcome::WelcomeAction::OpenSettings => self.open_settings(settings_ui::Page::Audio),
            welcome::WelcomeAction::OpenUpdater => {
                self.updater.dialog_open = true;
            }
            welcome::WelcomeAction::LoadTemplate(kind) => {
                let song = welcome::build_template(kind);
                let title = tr.template_title(kind);
                let with_kit = matches!(
                    kind,
                    welcome::TemplateKind::EmptyBeat
                        | welcome::TemplateKind::DemoBeat
                        | welcome::TemplateKind::TideStudio
                        | welcome::TemplateKind::VocalSession
                );
                let open_inst = match kind {
                    welcome::TemplateKind::TideStudio => song
                        .instruments
                        .iter()
                        .find(|i| {
                            matches!(i.kind, keyframexl::sequence::InstrumentKind::Synth { .. })
                        })
                        .map(|i| i.id),
                    welcome::TemplateKind::ForgeSfx => song
                        .instruments
                        .iter()
                        .find(|i| {
                            matches!(i.kind, keyframexl::sequence::InstrumentKind::Forge { .. })
                        })
                        .map(|i| i.id),
                    welcome::TemplateKind::VocalSession => song
                        .instruments
                        .iter()
                        .find(|i| {
                            matches!(i.kind, keyframexl::sequence::InstrumentKind::Vocal { .. })
                        })
                        .map(|i| i.id),
                    _ => None,
                };
                self.load_welcome_song(
                    song,
                    crate::i18n::fill(tr.status_started_template, &[("title", title)]),
                    with_kit,
                    open_inst,
                );
            }
            welcome::WelcomeAction::LoadGenre(g) => {
                let song = g.build();
                self.load_welcome_song(
                    song,
                    crate::i18n::fill(tr.status_started_template, &[("title", tr.genre_title(g))]),
                    false,
                    None,
                );
            }
            welcome::WelcomeAction::LoadTidePreset(idx) => {
                let song = welcome::build_tide_preset_song(idx);
                let open_inst = song.instruments.first().map(|i| i.id);
                let name = song.info.title.clone();
                self.load_welcome_song(
                    song,
                    crate::i18n::fill(tr.status_loaded_tide_preset, &[("name", &name)]),
                    false,
                    open_inst,
                );
            }
            welcome::WelcomeAction::LoadForgeRecipe(idx) => {
                let song = welcome::build_forge_recipe_song(idx);
                let open_inst = song.instruments.first().map(|i| i.id);
                let name = song.info.title.clone();
                self.load_welcome_song(
                    song,
                    crate::i18n::fill(tr.status_loaded_forge_recipe, &[("name", &name)]),
                    false,
                    open_inst,
                );
            }
        }
    }

    /// An FL Studio project becomes a new, unsaved project (saving writes a
    /// .kfxl; the .flp is never changed). The full report goes to the
    /// project's comments (Project info), a summary to the status line.
    fn import_flp(&mut self, path: &std::path::Path) {
        let tr = self.tr();
        let bytes = std::fs::read(path).unwrap_or_default();
        match keyframexl::flp::read_flp(&bytes) {
            Ok((mut s, report)) => {
                let file = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let lines = report.lines();
                s.info.title = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let native = Self::flp_native_plugins(&mut s, &report, &bytes);
                let (mut plugins, mut missing) = self.flp_plugins(&mut s, &report, &bytes);
                match native {
                    Ok(n) => plugins += n,
                    Err(e) => missing.push(e),
                }
                let samples = self.flp_samples(&report);
                self.flp_slices = report.audio_slices.iter().cloned().collect();
                self.start_flp_freeze(&s, &report, &bytes, &file);
                let mut notes = lines.clone();
                notes.extend(missing.iter().cloned());
                let notes_s = notes.join("\n");
                s.info.comments = crate::i18n::fill(
                    tr.flp_import_comments_header,
                    &[("file", &file), ("notes", &notes_s)],
                );
                self.close_riff();
                self.unload_all_plugins();
                self.checkpoint();
                self.selected = s.patterns[0].id;
                self.song = s;
                self.clip = None;
                self.path = None;
                self.publish();
                let mut loaded = 0;
                for (id, sample) in samples {
                    self.load_preset(id, sample);
                    loaded += 1;
                }
                let gaps = if missing.is_empty() {
                    String::new()
                } else {
                    tr.flp_missing_plugins.fmt(tr.lang, missing.len())
                };
                let plugins_s = tr.flp_plugins_count.fmt(tr.lang, plugins);
                let samples_s = tr.flp_samples_restored.fmt(tr.lang, loaded);
                self.say(
                    Tone::Done,
                    crate::i18n::fill(
                        tr.status_flp_imported,
                        &[
                            ("file", &file),
                            ("summary", &lines[0]),
                            ("plugins", &plugins_s),
                            ("samples", &samples_s),
                            ("gaps", &gaps),
                        ],
                    ),
                );
            }
            Err(e) => self.say(
                Tone::Bad,
                crate::i18n::fill(
                    tr.status_cannot_import,
                    &[
                        ("path", &path.display().to_string()),
                        ("err", &e.to_string()),
                    ],
                ),
            ),
        }
    }

    fn flp_native_plugins(
        song: &mut Song,
        report: &keyframexl::flp::FlpReport,
        bytes: &[u8],
    ) -> Result<usize, String> {
        use keyframexl::sequence::{InstrumentKind, NativeFlProject, PluginRef};
        let mut instruments: Vec<_> = report
            .channels
            .iter()
            .filter(|c| c.in_native_bridge())
            .filter_map(|c| c.instrument.map(|id| (c, id)))
            .collect();
        instruments.sort_by_key(|(c, _)| !c.plugin.eq_ignore_ascii_case("FLEX"));
        let flex_count = instruments
            .iter()
            .take_while(|(c, _)| c.plugin.eq_ignore_ascii_case("FLEX"))
            .count();
        if flex_count <= 16 {
            instruments.truncate(16);
        }
        if instruments.is_empty() {
            return Ok(0);
        }
        let channels = instruments.iter().map(|(c, _)| c.index).collect::<Vec<_>>();
        let (data, midi_channels) = keyframexl::flp::native_host_flp(bytes, &channels)?;
        let next = song
            .plugin_refs()
            .iter()
            .map(|p| p.id)
            .filter(|id| id & keyframexl::flp::PLUGIN_KEY == 0)
            .max()
            .unwrap_or(0)
            + 1;
        for (slot, &(channel, id)) in instruments.iter().enumerate() {
            if let Some(i) = song.instruments.iter_mut().find(|i| i.id == id) {
                i.kind = InstrumentKind::Plugin {
                    plugin: PluginRef {
                        id: next + slot as u64,
                        path: crate::fl_native::path(slot),
                        format: "fl-native".into(),
                        vendor: "Image-Line".into(),
                        name: channel.plugin.clone(),
                        state: String::new(),
                        outs: Vec::new(),
                    },
                };
            }
        }
        song.native_fl = Some(std::sync::Arc::new(NativeFlProject::new(
            &data,
            midi_channels,
        )));
        Ok(instruments.len())
    }

    /// VST channels of an imported FL project become plugin channels of the
    /// same plugin (found by name and format among the scanned ones) with the
    /// state FL saved. Returns how many, and a report line per plugin missing.
    fn flp_plugins(
        &self,
        song: &mut Song,
        report: &keyframexl::flp::FlpReport,
        flp: &[u8],
    ) -> (usize, Vec<String>) {
        use keyframexl::sequence::{InstrumentKind, PluginRef};
        let tr = self.tr();
        let (mut done, mut missing) = (0, Vec::new());
        // The import's placeholder plugin ids (for automation) → real ids.
        let key = keyframexl::flp::PLUGIN_KEY;
        let fresh = |song: &Song| {
            song.plugin_refs()
                .iter()
                .map(|p| p.id)
                .filter(|i| i & key == 0)
                .max()
                .unwrap_or(0)
                + 1
        };
        let mut ids = std::collections::BTreeMap::new();
        let set = |song: &mut Song, id: u64, plugin: PluginRef| {
            if let Some(inst) = song.instruments.iter_mut().find(|i| i.id == id) {
                inst.kind = InstrumentKind::Plugin { plugin };
            }
        };
        // Compatible generators open with their saved channel state in FL Studio VSTi.
        let vsti = self.plugins.list.iter().find(|p| {
            p.usable() && p.name == "FL Studio VSTi" && crate::plugins::format_of(&p.path) == "vst2"
        });
        for c in &report.channels {
            let Some(id) = c.instrument else { continue };
            if !c.in_fl_vsti() || song.instrument(id).and_then(|i| i.plugin()).is_some() {
                continue;
            }
            let Some(v) = vsti else {
                // A Tide stand-in plays it meanwhile (see the import report).
                if !song
                    .instrument(id)
                    .is_some_and(|i| matches!(i.kind, InstrumentKind::Synth { .. }))
                {
                    missing.push(crate::i18n::fill(
                        tr.flp_note_needs_vsti,
                        &[("ch", &c.name), ("plugin", &c.plugin)],
                    ));
                }
                continue;
            };
            match keyframexl::flp::fl_vsti_state(flp, c.index) {
                Ok(state) => {
                    let mut plugin = PluginRef {
                        id: fresh(song),
                        format: "vst2".into(),
                        path: v.path.display().to_string(),
                        name: format!("FL Studio VSTi ({})", c.plugin),
                        vendor: "Image-Line".into(),
                        state: String::new(),
                        outs: vec![],
                    };
                    plugin.set_state(&state);
                    set(song, id, plugin);
                    done += 1;
                }
                Err(e) => missing.push(crate::i18n::fill(
                    tr.flp_note_not_moved,
                    &[
                        ("ch", &c.name),
                        ("plugin", &c.plugin),
                        ("err", &e.to_string()),
                    ],
                )),
            }
        }
        // VST effects in the mixer: the import left FL's Windows path; find
        // the plugin here or take the slot out.
        for t in 0..song.mixer.len() {
            let mut k = 0;
            while k < song.mixer[t].fx.len() {
                let Some(p) = song.mixer[t].fx[k]
                    .plugin
                    .clone()
                    .filter(|p| !p.path.starts_with('/'))
                else {
                    k += 1;
                    continue;
                };
                let found = self.plugins.list.iter().find(|s| {
                    s.usable()
                        && s.name.eq_ignore_ascii_case(&p.name)
                        && crate::plugins::format_of(&s.path) == p.format
                });
                match found {
                    Some(s) => {
                        let id = fresh(song);
                        let slot = song.mixer[t].fx[k].plugin.as_mut().expect("plugin slot");
                        ids.insert(slot.id, id);
                        slot.id = id;
                        slot.path = s.path.display().to_string();
                        slot.name = s.name.clone();
                        done += 1;
                        k += 1;
                    }
                    None => {
                        let fmt_u = p.format.to_uppercase();
                        missing.push(crate::i18n::fill(
                            tr.flp_note_fx_missing,
                            &[
                                ("track", &song.mixer[t].name),
                                ("fmt", &fmt_u),
                                ("plugin", &p.name),
                            ],
                        ));
                        song.mixer[t].fx.remove(k);
                        // Automation of later effects on this insert follows them.
                        for pat in song
                            .patterns
                            .iter_mut()
                            .filter(|p| p.automation.is_some())
                            .map(std::sync::Arc::make_mut)
                        {
                            use keyframexl::automation::Target::{FxMix, FxParam};
                            match pat.automation.as_mut().map(|env| &mut env.target) {
                                Some(FxParam { track, slot, .. } | FxMix { track, slot })
                                    if *track as usize == t && *slot as usize > k =>
                                {
                                    *slot -= 1;
                                }
                                Some(FxParam { track, slot, .. } | FxMix { track, slot })
                                    if *track as usize == t && *slot as usize == k =>
                                {
                                    pat.automation = None;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        for c in &report.channels {
            let (Some(vst), Some(id)) = (&c.vst, c.instrument) else {
                continue;
            };
            let format = if vst.vst3 { "vst3" } else { "vst2" };
            let found = self.plugins.list.iter().find(|p| {
                p.usable()
                    && p.name.eq_ignore_ascii_case(&vst.name)
                    && crate::plugins::format_of(&p.path) == format
            });
            let Some(p) = found else {
                let fmt_u = format.to_uppercase();
                missing.push(crate::i18n::fill(
                    tr.flp_note_inst_missing,
                    &[("ch", &c.name), ("fmt", &fmt_u), ("plugin", &vst.name)],
                ));
                continue;
            };
            let mut plugin = keyframexl::sequence::PluginRef {
                id: fresh(song),
                format: format.into(),
                path: p.path.display().to_string(),
                name: p.name.clone(),
                vendor: p.vendor.clone(),
                state: String::new(),
                outs: vec![],
            };
            plugin.set_state(&vst.state);
            ids.insert(keyframexl::flp::channel_key(c.index), plugin.id);
            if let Some(inst) = song.instruments.iter_mut().find(|i| i.id == id) {
                inst.kind = keyframexl::sequence::InstrumentKind::Plugin { plugin };
                done += 1;
            }
        }
        for name in keyframexl::flp::resolve_plugin_keys(song, &ids) {
            missing.push(crate::i18n::fill(
                tr.flp_note_auto_missing,
                &[("name", &name)],
            ));
        }
        (done, missing)
    }

    /// Sampler channels of an imported FL project and the sample each had,
    /// found on this computer (FL's paths point into the Wine prefix).
    fn flp_samples(&self, report: &keyframexl::flp::FlpReport) -> Vec<(u64, std::path::PathBuf)> {
        let prefix = std::env::var_os("WINEPREFIX")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".wine"));
        let install =
            crate::fl_link::find_fl().and_then(|exe| exe.parent().map(|d| d.to_path_buf()));
        let packs = install.as_ref().map(|d| d.join("Data/Patches"));
        report
            .channels
            .iter()
            .filter_map(|c| {
                let id = c.instrument?;
                match (&c.sample, &c.dwp) {
                    (Some(s), _) => Some((
                        id,
                        keyframexl::flp::resolve_path(s, &prefix, install.as_deref())?,
                    )),
                    // DirectWave programs: FL's own packs, then the user's folder.
                    (None, Some(d)) => {
                        let docs = prefix
                            .join("drive_c/users")
                            .join(std::env::var("USER").unwrap_or_default())
                            .join("Documents/Image-Line");
                        Some((
                            id,
                            packs
                                .as_deref()
                                .and_then(|p| keyframexl::flp::find_file(p, d))
                                .or_else(|| keyframexl::flp::find_file(&docs, d))?,
                        ))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// FL renders native generators dry, with their original note expression and slides.
    fn start_flp_freeze(
        &mut self,
        song: &Song,
        report: &keyframexl::flp::FlpReport,
        bytes: &[u8],
        file: &str,
    ) {
        self.flp_freeze = None;
        self.flp_freeze_for.clear();
        if crate::fl_link::find_fl().is_none() {
            return;
        }
        for c in report
            .channels
            .iter()
            .filter(|c| c.enabled && c.in_fl_vsti())
        {
            let Some(id) = c.instrument else { continue };
            if song
                .instrument(id)
                .and_then(|i| i.plugin())
                .is_some_and(|p| p.format == "fl-native")
            {
                continue;
            }
            if !song.clips.iter().filter(|clip| !clip.muted).any(|clip| {
                song.patterns
                    .iter()
                    .any(|p| p.id == clip.pattern && p.notes.iter().any(|n| n.instrument == id))
            }) {
                continue;
            }
            let insert = song.instrument(id).map_or(0, |i| i.mixer);
            self.flp_freeze_for
                .push((c.index, c.name.clone(), id, insert));
        }
        if self.flp_freeze_for.is_empty() {
            return;
        }
        let list: Vec<(u32, String)> = self
            .flp_freeze_for
            .iter()
            .map(|(c, n, _, _)| (*c, n.clone()))
            .collect();
        let bytes = bytes.to_vec();
        let name = file.trim_end_matches(".flp").to_string();
        let (tx, rx) = mpsc::channel();
        self.flp_freeze = Some(rx);
        std::thread::spawn(move || {
            let say = |s: String| {
                let _ = tx.send(FreezeMsg::Progress(s));
            };
            let r = crate::flp_freeze::freeze(&bytes, &name, &list, &say).map(|wavs| {
                wavs.into_iter()
                    .filter_map(|(ch, wav)| match crate::import::sample_preset(&wav) {
                        Ok(preset) => Some((ch, preset)),
                        Err(e) => {
                            eprintln!("Cannot load FL render for channel {ch}: {e}");
                            None
                        }
                    })
                    .collect()
            });
            let _ = tx.send(FreezeMsg::Done(r));
        });
    }

    /// Play each completed FL render from the song start via its source insert.
    fn poll_flp_freeze(&mut self) {
        let Some(rx) = &self.flp_freeze else { return };
        let mut done = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                FreezeMsg::Progress(s) => self.status = (Tone::Wait, s),
                FreezeMsg::Done(r) => done = Some(r),
            }
        }
        let Some(r) = done else { return };
        self.flp_freeze = None;
        let tr = self.tr();
        let wavs = match r {
            Ok(w) => w,
            Err(e) => {
                self.say(
                    Tone::Bad,
                    crate::i18n::fill(tr.status_fl_render_err, &[("err", &e.to_string())]),
                );
                return;
            }
        };
        use keyframexl::sequence::{
            BAR, Clip, InstrumentKind, MAX_BARS, Note, Pattern, STEP, Tick,
        };
        let clock =
            keyframexl::tempo::TempoTable::build(&self.song.tempo_map, self.song.tempo, 48_000);
        let track = self
            .song
            .clips
            .iter()
            .map(|c| c.track)
            .max()
            .map_or(0, |t| t + 1);
        let mut loaded = 0;
        for (ch, preset) in wavs {
            let Some((_, name, inst, insert)) = self.flp_freeze_for.iter().find(|f| f.0 == ch)
            else {
                continue;
            };
            let Some(original) = self.song.instrument(*inst) else {
                continue;
            };
            if original.name != *name {
                continue;
            }
            let muted = original.muted;
            let Some(zone) = preset.zones.first() else {
                continue;
            };
            if !zone
                .frames
                .iter()
                .any(|frame| frame[0] != 0.0 || frame[1] != 0.0)
            {
                eprintln!("FL render for {name} contains no audio");
                continue;
            }
            let frames =
                zone.end.saturating_sub(zone.offset) as f64 * 48_000.0 / zone.sample_rate as f64;
            let length = (clock.tick_at(frames).ceil() as Tick).clamp(1, MAX_BARS * BAR);
            let label = format!("{name} (FL render)");
            let id = self.song.add_channel(
                &label,
                InstrumentKind::Sampler {
                    preset: Some(std::sync::Arc::new(preset)),
                },
            );
            if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == id) {
                i.mixer = *insert;
                i.muted = muted;
            }
            if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == *inst) {
                i.muted = true;
            }
            let pid = self.song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            let mut pattern = Pattern::new(pid, &label);
            pattern.notes.push(Note::new(id, 0, length, 60, 127));
            pattern.length = (length + STEP - 1) / STEP * STEP;
            self.song.patterns.push(pattern.into());
            self.song.clips.push(Clip {
                pattern: pid,
                start: 0,
                track: track + loaded,
                offset: 0,
                length: Some(length),
                muted: false,
            });
            loaded += 1;
        }
        while self.song.tracks.len() < (track + loaded) as usize {
            self.song.tracks.push(Default::default());
        }
        self.song.ensure_note_ids();
        self.publish();
        let total = self.flp_freeze_for.len();
        self.say(
            if loaded as usize == total {
                Tone::Done
            } else {
                Tone::Bad
            },
            crate::i18n::fill(
                tr.status_fl_rendered,
                &[
                    ("loaded", &loaded.to_string()),
                    ("total", &total.to_string()),
                ],
            ),
        );
    }

    pub(crate) fn poll_import(&mut self) {
        let mut done = Vec::new();
        self.importing
            .retain(|(id, rx, quiet)| match rx.try_recv() {
                Ok(r) => {
                    done.push((*id, r, *quiet));
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        for (target, result, quiet) in done {
            self.land_preset(target, result, quiet);
        }
        while self.importing.len() < 3
            && let Some((id, path, quiet)) = self.load_queue.pop_front()
        {
            self.load_preset_with(id, path, quiet);
        }
    }

    fn land_preset(
        &mut self,
        target: u64,
        result: Result<keyframexl::sequence::SamplePreset, String>,
        quiet: bool,
    ) {
        let tr = self.tr();
        match result {
            Ok(preset) => {
                let preset = match self.flp_slices.remove(&target) {
                    Some(slices) => keyframexl::flp::slice_preset(preset, &slices),
                    None => preset,
                };
                if !quiet {
                    if self.join_preset == Some(target) {
                        self.join_preset = None;
                    } else {
                        self.checkpoint();
                    }
                }
                let name = preset.name.clone();
                let report = preset.report.clone();
                // A drum voice that receives a sample becomes a sampler (FL: drop on a channel).
                let was_drum = self
                    .song
                    .instrument(target)
                    .is_some_and(|i| !i.is_sampler());
                if was_drum
                    && let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == target)
                {
                    i.kind = keyframexl::sequence::InstrumentKind::Sampler { preset: None };
                }
                // A channel still called "Sampler N" takes the preset's name.
                let rename =
                    !quiet && (was_drum || self.instrument_name(target).starts_with("Sampler "));
                match self.song.set_sampler_preset(target, preset) {
                    Ok(()) => {
                        if rename
                            && let Some(i) =
                                self.song.instruments.iter_mut().find(|i| i.id == target)
                        {
                            i.name = name.clone();
                        }
                        self.publish();
                        if !quiet {
                            let target_name = self.instrument_name(target);
                            self.say(
                                Tone::Done,
                                crate::i18n::fill(
                                    tr.status_preset_loaded,
                                    &[
                                        ("name", &name),
                                        ("target", &target_name),
                                        ("report", &report),
                                    ],
                                ),
                            );
                        }
                    }
                    Err(e) => {
                        if !quiet {
                            self.history.pop();
                        }
                        self.say(
                            Tone::Bad,
                            crate::i18n::fill(tr.status_not_loaded, &[("err", e)]),
                        );
                    }
                }
            }
            Err(e) => self.say(
                Tone::Bad,
                crate::i18n::fill(tr.status_preset_not_loaded, &[("err", &e)]),
            ),
        }
    }

    // ------------------------------------------------------------ keys

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::E)) {
            let inst = self
                .song
                .instrument(self.instrument)
                .map_or_else(|| self.default_roll(), |i| i.id);
            self.open_roll(inst);
            self.open_riff(inst);
            return;
        }
        if self.settings.general.typing_keyboard {
            self.typing_piano(ctx);
        }
        let (ctrl, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
        if pressed(egui::Key::Space) {
            // Space: play / stop with rewind. Ctrl+Space: pause in place.
            if self.is_playing() && !ctrl {
                self.stop();
            } else {
                self.play_pause();
            }
        }
        if ctrl && pressed(egui::Key::Z) {
            if shift {
                self.redo();
            } else {
                self.undo();
            }
        }
        if ctrl && pressed(egui::Key::Y) {
            self.redo();
        }
        if ctrl && pressed(egui::Key::S) {
            if shift {
                self.save_as();
            } else {
                self.save();
            }
        }
        if ctrl && shift && pressed(egui::Key::M) {
            self.export_midi();
        }
        if ctrl && pressed(egui::Key::R) {
            self.export();
        }
        if ctrl && pressed(egui::Key::H) {
            let tr = self.tr();
            self.stop();
            self.panic();
            self.say(Tone::Quiet, tr.status_all_sound_stopped);
        }
        if ctrl && !shift && !ctx.input(|i| i.modifiers.alt) && pressed(egui::Key::J) {
            self.toggle_multilink();
        }
        if ctrl && !shift && pressed(egui::Key::M) {
            self.toggle_metronome();
        }
        if ctrl && !shift && pressed(egui::Key::T) {
            self.toggle_typing_piano();
        }
        // FL: F10 opens the settings.
        if pressed(egui::Key::F10) {
            match self.settings_ui {
                Some(_) => self.settings_ui = None,
                None => self.open_settings(settings_ui::Page::Midi),
            }
        }
        if !ctrl && pressed(egui::Key::F1) {
            self.show_shortcuts = !self.show_shortcuts;
        }
        if !ctrl && pressed(egui::Key::F2) {
            self.start_rename();
        }
        if !ctrl && pressed(egui::Key::F4) {
            self.next_empty_pattern();
        }
        // Ctrl+Alt+← / →: the previous or next song marker.
        if ctrl && ctx.input(|i| i.modifiers.alt) {
            for (key, dir) in [(egui::Key::ArrowLeft, -1), (egui::Key::ArrowRight, 1)] {
                if pressed(key) {
                    self.jump_marker(dir);
                }
            }
        }
        if !ctrl && pressed(egui::Key::F11) {
            self.toggle_info(ctx);
        }
        if pressed(egui::Key::F12) {
            self.close_all_windows();
        }
        if ctrl && pressed(egui::Key::O) {
            self.request(Pending::Open(None));
        }
        // FL: Ctrl+N saves a new version (name_2, name_3 …); New is Ctrl+Shift+N.
        if ctrl && !shift && pressed(egui::Key::N) {
            self.save_new_version();
        }
        if ctrl && shift && pressed(egui::Key::N) {
            self.request(Pending::New);
        }
        // FL: F5/F6/F7 open a window, or close it when it already has focus.
        let roll = Tab::Roll {
            instrument: self.instrument,
        };
        for (tab, key) in [
            (Tab::Playlist, egui::Key::F5),
            (Tab::Rack, egui::Key::F6),
            (roll, egui::Key::F7),
            (Tab::Mixer, egui::Key::F9),
        ] {
            if pressed(key) {
                self.win(Cmd::Toggle(tab));
            }
        }
        let alt = ctx.input(|i| i.modifiers.alt);
        if alt && pressed(egui::Key::F8) {
            self.ws.browser = !self.ws.browser;
        }
        if pressed(egui::Key::F8) && !alt {
            Self::toggle_picker(ctx);
        }
        for (i, key) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3]
            .into_iter()
            .enumerate()
        {
            if alt && !ctrl && pressed(key) {
                self.win(Cmd::Preset(i));
            }
        }
        if !ctrl && !alt && pressed(egui::Key::R) {
            self.toggle_record();
        }
        if !ctrl && pressed(egui::Key::L) {
            self.set_song_mode(!self.song_mode);
        }
        if self.focus_kind() == Some(Kind::Roll) && !ctrl && !alt {
            let tr = self.tr();
            for tool in Tool::ALL {
                if pressed(tool.key()) {
                    self.tool = tool;
                    self.say(
                        Tone::Quiet,
                        crate::i18n::fill(
                            tr.status_tool_selected,
                            &[("label", tr.tool_label(tool)), ("hint", tr.tool_hint(tool))],
                        ),
                    );
                }
            }
        }
        // FL: Ctrl+1…0 mute the first ten channels, Ctrl+Shift+1…0 solo them.
        if ctrl {
            use egui::Key::*;
            for (n, key) in [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0]
                .into_iter()
                .enumerate()
            {
                if pressed(key)
                    && let Some(id) = self.song.instruments.get(n).map(|i| i.id)
                {
                    if shift {
                        self.toggle_solo(id);
                    } else {
                        self.toggle_mute(id);
                    }
                }
            }
        }
        if ctrl && shift && pressed(egui::Key::Insert) {
            self.new_pattern();
        }
        if self.focus_kind() == Some(Kind::Playlist)
            && (pressed(egui::Key::Delete) || pressed(egui::Key::Backspace))
            && let Some(i) = self.clip
        {
            self.delete_clip(i);
        }
    }

    // ------------------------------------------------------------ shell

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        let rect = ui.max_rect();
        let p = ui.painter();
        p.rect_filled(rect, 0.0, t::side());
        p.line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0_f32, t::edge()),
        );
        let y = rect.center().y;
        let mut x = rect.left() + PAGE;
        // Controller learn or Multilink arms the status: it beats any older message.
        let armed = self
            .midi_learn
            .map(|t| {
                crate::i18n::fill(
                    tr.status_midi_learn_prompt,
                    &[("target", &self.auto_label(t))],
                )
            })
            .or_else(|| self.multilink_status());
        let (tone, text): (Tone, &String) = match &armed {
            Some(line) => (Tone::Wait, line),
            None => (self.status.0, &self.status.1),
        };
        let ink = match tone {
            Tone::Quiet => t::muted(),
            Tone::Done => t::ok(),
            Tone::Wait => t::wait(),
            Tone::Bad => t::bad(),
        };
        if tone != Tone::Quiet {
            let pulse = if tone == Tone::Wait {
                let s = ui.input(|i| i.time) as f32;
                0.55 + 0.45 * (s * std::f32::consts::TAU / 1.4).cos().abs()
            } else {
                1.0
            };
            p.circle_filled(Pos2::new(x + 4.0, y), 4.0, ink.gamma_multiply(pulse));
            x += 16.0;
        }
        // Right-aligned alerts: no audio output, and a quiet update pill if available.
        let mut right_edge = match &self.audio {
            Some(_) => rect.right() - PAGE,
            None => {
                kit::caps(
                    p,
                    Pos2::new(rect.right() - PAGE, y),
                    Align2::RIGHT_CENTER,
                    tr.status_no_audio_pill,
                    12.0,
                    t::bad(),
                )
                .left()
                    - 14.0
            }
        };
        if let Some(info) = self
            .updater
            .available(self.settings.general.skipped_version.as_deref())
            .cloned()
        {
            let label = match &self.updater.install {
                crate::updater::InstallState::ReadyToRestart { version, .. } => {
                    crate::i18n::fill(tr.status_update_restart_pill, &[("ver", version)])
                }
                _ if self.updater.is_installing() => {
                    crate::i18n::fill(tr.status_update_installing_pill, &[("ver", &info.version)])
                }
                _ => crate::i18n::fill(tr.status_update_avail_pill, &[("ver", &info.version)]),
            };
            let galley = kit::caps_galley(p, &label, 11.0, t::wait());
            let pill_w = galley.size().x + 16.0;
            let pill_h = 18.0;
            let pill = egui::Rect::from_min_max(
                Pos2::new(right_edge - pill_w, y - pill_h * 0.5),
                Pos2::new(right_edge, y + pill_h * 0.5),
            );
            let resp = ui
                .interact(pill, ui.id().with("update_pill"), Sense::click())
                .on_hover_text(crate::i18n::fill(
                    tr.status_update_avail_tip,
                    &[("ver", &info.version)],
                ));
            let fill = if resp.hovered() {
                t::ctl_top()
            } else {
                t::wait_soft()
            };
            p.rect_filled(pill, 9.0, fill);
            p.rect_stroke(
                pill,
                9.0,
                Stroke::new(1.0_f32, t::wait()),
                egui::StrokeKind::Inside,
            );
            p.galley(
                Pos2::new(pill.left() + 8.0, y - galley.size().y * 0.5),
                galley,
                t::wait(),
            );
            if resp.clicked() {
                self.updater.dialog_open = true;
            }
            right_edge = pill.left() - 12.0;
        }
        // A long message stops short of it; hover shows all of it.
        let room = (right_edge - 24.0 - x).max(0.0);
        let shown = kit::elide(p, text, t::sans(13.0), room);
        let r = p.text(
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            &shown,
            t::sans(13.0),
            ink,
        );
        if shown != *text {
            let text = text.clone();
            ui.interact(r, ui.id().with("status_full"), egui::Sense::hover())
                .on_hover_text(text);
        }
    }

    /// One line of how-to for the current view; shown when there is no news.
    fn hint(&self) -> &'static str {
        let tr = self.tr();
        match self.focus_kind() {
            Some(Kind::Rack) => tr.hint_rack,
            Some(Kind::Roll) => tr.hint_roll,
            Some(Kind::Playlist) => tr.hint_playlist,
            Some(Kind::Mixer) => tr.hint_mixer,
            Some(Kind::Instrument) => tr.hint_instrument,
            Some(Kind::Riff) => tr.hint_riff,
            Some(Kind::Settings) => tr.hint_settings,
            None => tr.hint_empty,
        }
    }

    pub(crate) fn tab_ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Rack => self.rack(ui),
            Tab::Roll { instrument } => self.roll(ui, instrument),
            Tab::Playlist => self.playlist(ui),
            Tab::Instrument { id } => self.instrument_window(ui, *id),
            Tab::Mixer => self.mixer(ui),
            Tab::Riff => self.riff_tab(ui),
            Tab::Settings => self.settings_tab(ui),
        }
    }

    /// Window header strip (FL window header): the ▾ window menu, caps context,
    /// the window's tools on the right, an edge seam under it. The title lives
    /// on the tab. `f` is called with [`Slot::Menu`] inside the menu (only while
    /// it is open) and with [`Slot::Tools`] for the right-hand tools.
    pub(crate) fn panel_header(
        ui: &mut egui::Ui,
        sub: &str,
        mut f: impl FnMut(&mut egui::Ui, Slot),
    ) {
        let tr = crate::i18n::ctx_tr(ui.ctx());
        ui.horizontal(|ui| {
            ui.set_height(kit::CONTROL_H);
            ui.spacing_mut().button_padding = vec2(4.0, 2.0);
            let b = egui::Button::new("")
                .min_size(vec2(kit::TOOL_H, kit::TOOL_H))
                .corner_radius(kit::RADIUS - 2.0);
            let menu = egui::menu::menu_custom_button(ui, b, |ui| {
                ui.set_min_width(220.0);
                kit::menu_style(ui);
                f(ui, Slot::Menu);
            });
            let r = menu.response.on_hover_text(tr.win_header_menu_tip);
            let c = r.rect.center();
            let ink = if r.hovered() { t::text() } else { t::muted() };
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(-5.0, -2.5),
                    c + vec2(5.0, -2.5),
                    c + vec2(0.0, 3.5),
                ],
                ink,
                Stroke::NONE,
            ));
            ui.add_space(6.0);
            let p = ui.painter().clone();
            // The tools (right) keep their room; the title shortens with an
            // ellipsis, or goes, so the two never overlap in a narrow window.
            let tools_id = ui.id().with("header-tools-w");
            let tools_w: f32 = ui.data(|d| d.get_temp(tools_id)).unwrap_or(0.0);
            let room = ui.available_width() - tools_w - 12.0;
            let mut text = sub.to_string();
            let mut g = kit::caps_galley(&p, &text, 11.0, t::muted());
            while g.size().x > room && !text.is_empty() {
                text.pop();
                let t = text.trim_end();
                g = kit::caps_galley(&p, &format!("{t}…"), 11.0, t::muted());
            }
            if room > 24.0 && !text.trim().is_empty() {
                let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                p.galley(r.min + vec2(0.0, 1.0), g, t::muted());
            }
            let tools = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let right = ui.max_rect().right();
                f(ui, Slot::Tools);
                right - ui.min_rect().left()
            });
            let w = tools.inner;
            if (w - tools_w).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(tools_id, w));
                ui.ctx().request_repaint();
            }
        });
        ui.add_space(4.0);
        let y = ui.cursor().top();
        let clip = ui.clip_rect();
        ui.painter().line_segment(
            [Pos2::new(clip.left(), y), Pos2::new(clip.right(), y)],
            Stroke::new(1.0_f32, t::edge()),
        );
        ui.add_space(8.0);
    }

    /// Items every window menu ends with: float or dock, close.
    pub(crate) fn window_menu_tail(&mut self, ui: &mut egui::Ui, tab: Tab) {
        let tr = self.tr();
        ui.separator();
        let floating = self
            .ws
            .dock()
            .iter_all_tabs()
            .any(|(at, t)| *t == tab && at.0 != egui_dock::SurfaceIndex::main());
        if floating {
            if ui.button(tr.win_dock).clicked() {
                self.win(Cmd::Dock(tab.clone()));
                ui.close_menu();
            }
        } else if ui.button(tr.win_detach_float).clicked() {
            self.win(Cmd::Float(tab.clone()));
            ui.close_menu();
        }
        if ui.button(tr.common_close).clicked() {
            self.win(Cmd::Close(tab));
            ui.close_menu();
        }
    }

    /// Every window closed: say so and offer the way back.
    fn empty_workspace(&mut self, ui: &mut egui::Ui, area: egui::Rect) {
        let tr = self.tr();
        let c = area.center();
        let p = ui.painter();
        p.text(
            c - vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            tr.empty_all_closed,
            t::medium(15.0),
            t::text(),
        );
        p.text(
            c - vec2(0.0, 6.0),
            Align2::CENTER_CENTER,
            tr.empty_keys_hint,
            t::sans(14.0),
            t::muted(),
        );
        let bw = (kit::text_width(p, tr.empty_restore_layout, t::sans(14.0)) + 32.0).max(170.0);
        let b = egui::Rect::from_center_size(c + vec2(0.0, 34.0), vec2(bw, kit::CONTROL_H));
        let r = ui.interact(b, ui.id().with("restore-layout"), Sense::click());
        kit::paint_button(
            ui,
            b,
            &r,
            kit::Kind::Normal,
            true,
            tr.empty_restore_layout,
            |p, face, ink| {
                p.text(
                    face.center(),
                    Align2::CENTER_CENTER,
                    tr.empty_restore_layout,
                    t::sans(14.0),
                    ink,
                );
            },
        );
        if r.clicked() {
            self.win(Cmd::Reset);
        }
    }

    /// While files hover over the window, say what a drop will do.
    fn drop_overlay(&self, ctx: &egui::Context) {
        let tr = self.tr();
        let names: Vec<String> = ctx.input(|i| {
            i.raw
                .hovered_files
                .iter()
                .filter_map(|f| {
                    f.path
                        .as_ref()
                        .and_then(|p| p.extension())
                        .map(|e| e.to_string_lossy().to_ascii_lowercase())
                })
                .collect()
        });
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let text = match names.first().map(String::as_str) {
            Some("dwp") => {
                crate::i18n::fill(tr.drop_dwp_into, &[("target", &self.drop_target_name())])
            }
            Some("wav" | "flac" | "ogg") => {
                crate::i18n::fill(tr.drop_sample_into, &[("target", &self.drop_target_name())])
            }
            Some("kfxl") => tr.drop_open_project.to_string(),
            Some("flp") => tr.drop_import_flp.to_string(),
            // Where it lands depends on the row under the pointer at the drop.
            Some("mid" | "midi") => tr.drop_midi_notes.to_string(),
            _ => tr.drop_any_file.to_string(),
        };
        let screen = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop"),
        ));
        p.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
        let b = egui::Rect::from_center_size(
            screen.center(),
            vec2(kit::text_width(&p, &text, t::medium(15.0)) + 48.0, 56.0),
        );
        kit::control(&p, b, 10.0, 0.0, false);
        p.text(
            b.center() - vec2(0.0, 1.0),
            Align2::CENTER_CENTER,
            text,
            t::medium(15.0),
            t::text(),
        );
    }

    fn capture_frame(&mut self, ctx: &egui::Context) {
        let Some((path, frame)) = &mut self.capture else {
            return;
        };
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            match image::save_buffer(
                &*path,
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            ) {
                Ok(()) => eprintln!(
                    "Captured {}x{} to {}",
                    image.size[0],
                    image.size[1],
                    path.display()
                ),
                Err(e) => eprintln!("Capture failed: {e}"),
            }
            // The capture's own close is not a person leaving unsaved work.
            self.quit_ok = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if *frame >= 20
            && *frame != u32::MAX
            && std::env::var("KFXL_CAPTURE_AFTER_MS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .is_none_or(|ms| ctx.input(|i| i.time) * 1000.0 >= ms)
        {
            *frame = u32::MAX - 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        *frame = frame.saturating_add(1);
    }
}

impl App {
    /// Stretched sampler zones: start what is stale on a worker, install
    /// what is made (if the zone still asks for it).
    fn poll_stretch(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok((inst, zone, made)) = self.stretch_jobs.1.try_recv() {
            self.stretch_busy.remove(&made.key);
            changed |= keyframexl::sampler::install_stretch(&mut self.song, inst, zone, made);
        }
        if changed {
            self.publish();
        }
        let tempo = self.song.tempo;
        for (inst, zi) in keyframexl::sampler::stale_stretches(&self.song) {
            let Some(zone) = self.song.instrument(inst).and_then(|i| match &i.kind {
                keyframexl::sequence::InstrumentKind::Sampler { preset: Some(p) } => {
                    p.zones.get(zi).cloned()
                }
                _ => None,
            }) else {
                continue;
            };
            let Some(key) = zone.stretch_key(tempo) else {
                continue;
            };
            if !self.stretch_busy.insert(key) {
                continue;
            }
            let tx = self.stretch_jobs.0.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                if let Some(made) = zone.render_stretch(tempo) {
                    let _ = tx.send((inst, zi, made));
                    ctx.request_repaint();
                }
            });
        }
    }

    /// Update `settings.general` from the current viewport state. Returns
    /// `(geom_dirty, close_requested, now)`.
    pub(crate) fn record_window_geometry(
        &mut self,
        ctx: &egui::Context,
        z: f32,
    ) -> (bool, bool, f64) {
        let (inner_size, maximized, fullscreen, minimized, close_requested, now) = ctx.input(|i| {
            let vp = i.viewport();
            // Wayland does not expose `inner_position`, so `vp.inner_rect` is `None`
            // there; `i.screen_rect()` still holds the logical inner size.
            let inner_size = vp
                .inner_rect
                .map(|r| r.size())
                .unwrap_or_else(|| i.screen_rect().size());
            (
                inner_size,
                vp.maximized.unwrap_or(false),
                vp.fullscreen.unwrap_or(false),
                vp.minimized.unwrap_or(false),
                vp.close_requested(),
                i.time,
            )
        });
        let mut geom_dirty = false;
        if self.startup_maximized {
            if maximized {
                self.startup_maximized = false;
            } else {
                // `eframe` maps the root window after frame 1; resend once
                // visible in case the WM/compositor dropped pre-map maximize.
                if self.startup_maximize_frames >= 1 && !self.startup_maximize_nudge_done {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                    self.startup_maximize_nudge_done = true;
                    ctx.request_repaint();
                }
                self.startup_maximize_frames = self.startup_maximize_frames.saturating_add(1);
                if now > 1.5 && self.startup_maximize_frames > 15 {
                    self.startup_maximized = false;
                }
            }
        }
        if !minimized
            && !self.startup_maximized
            && maximized != self.settings.general.window_maximized
        {
            self.settings.general.window_maximized = maximized;
            geom_dirty = true;
        }
        if !minimized
            && !maximized
            && !fullscreen
            && inner_size.x.is_finite()
            && inner_size.y.is_finite()
            && inner_size.x > 200.0
            && inner_size.y > 200.0
        {
            let logical = [(inner_size.x * z).round(), (inner_size.y * z).round()];
            if let Some(sanitized) = crate::settings::GeneralSettings::sanitize_window_size(logical)
            {
                let changed = self.settings.general.window_size.is_none_or(|old| {
                    (old[0] - sanitized[0]).abs() >= 2.0 || (old[1] - sanitized[1]).abs() >= 2.0
                });
                if changed {
                    self.settings.general.window_size = Some(sanitized);
                    geom_dirty = true;
                }
            }
        }
        (geom_dirty, close_requested, now)
    }

    /// Record logical inner size (when neither maximized nor fullscreen) and
    /// maximized state into settings, saving after a short settle delay or
    /// immediately on close.
    pub(crate) fn update_window_geometry(&mut self, ctx: &egui::Context, z: f32) {
        if !self.persist {
            return;
        }
        let (geom_dirty, close_requested, now) = self.record_window_geometry(ctx, z);
        let save_id = egui::Id::new("kfxl-window-geom-save");
        if close_requested {
            if geom_dirty || ctx.data(|d| d.get_temp::<f64>(save_id)).is_some() {
                ctx.data_mut(|d| d.remove::<f64>(save_id));
                self.save_settings();
            }
        } else if geom_dirty {
            ctx.data_mut(|d| d.insert_temp(save_id, now));
        } else if let Some(since) = ctx.data(|d| d.get_temp::<f64>(save_id))
            && now - since > 0.8
        {
            ctx.data_mut(|d| d.remove::<f64>(save_id));
            self.save_settings();
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        kit::relative_value_input(ctx, raw);
        if let Some(s) = &mut self.script {
            s.feed(raw.time.unwrap_or(0.0), raw);
        }
    }

    /// A take in progress is written before the window goes away.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.persist {
            self.save_settings();
            let text = self.ws.to_text();
            let _ = workspace::Workspace::save(&text);
        }
        if !self.rec.parts.is_empty() {
            self.finish_takes();
        }
        self.shutdown_show();
        self.shutdown_plugins();
        if let Some(exe) = self.relaunch.take()
            && let Err(e) = crate::updater::relaunch_binary(&exe)
        {
            eprintln!("{e}");
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let frame_start = std::time::Instant::now();
        crate::i18n::set_ctx_lang(ctx, self.settings.general.language);
        self.poll_stretch(ctx);
        // The theme switches before anything draws (the style is read per frame).
        let want = t::resolve(self.settings.general.theme);
        if want != t::theme() {
            t::apply(ctx, want);
        }
        self.sync_convolution_irs(ctx);
        self.capture_export(ctx);
        self.capture_forge_bounce();
        // On X11 plugin editors are placed inside our window (FL's wrapper).
        if self.plugins.parent.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            #[allow(clippy::unnecessary_cast)]
            {
                self.plugins.parent = match frame.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Xlib(h)) => Some(h.window as u32),
                    Ok(RawWindowHandle::Xcb(h)) => Some(h.window.get()),
                    _ => None,
                };
            }
        }
        kit::track_keyboard_nav(ctx);
        let was = self.focus.take();
        self.focus = workspace::focused(self.ws.dock());
        if let Some(Tab::Roll { instrument }) = &self.focus {
            self.instrument = *instrument;
            // A roll that just got the focus owns the selection and Paste.
            if was != self.focus && self.roll_inst != *instrument {
                self.roll_inst = *instrument;
                self.roll_sel.clear();
            }
        }
        if let Some(a) = &self.audio {
            a.tick();
        }
        self.riff_main_input(ctx);
        self.record_frame();
        self.vocal_frame(ctx);
        if !self.cjk_font
            && self
                .song
                .instruments
                .iter()
                .any(|i| matches!(i.kind, keyframexl::sequence::InstrumentKind::Vocal { .. }))
        {
            self.cjk_font = true;
            if let Some(font) = t::find_cjk_font() {
                ctx.set_fonts(t::font_definitions(Some(font)));
            }
        }
        if let Some(libs) = self.libraries_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.libraries_rx = None;
            self.libraries = libs;
            self.add_library_roots();
            // A fresh, untouched project gets real sounds.
            if self.path.is_none() && !self.dirty() {
                self.load_default_kit();
            }
            if let Some(p) = self.capture_midi.take() {
                self.import_midi(&p);
            }
        }
        self.midi_frame();
        self.midi_learn_frame(ctx);
        self.audio_watch(ctx);
        self.plugins_frame();
        if self.show_overlay(ctx) {
            self.plugins_end_frame(ctx);
            return;
        }
        self.poll_import();
        self.poll_flp_freeze();
        self.poll_preview();
        self.poll_forge(ctx);
        self.dropped_files(ctx);
        // Rebuilt as the rack and rolls draw below.
        self.midi_zones.clear();
        self.tap_keys(ctx);
        self.shortcuts(ctx);
        // The desktop's frame follows the setting, also when it changes.
        if self.capture.is_none() {
            let want = !self.own_title_bar();
            let id = egui::Id::new("decorations-applied");
            if ctx.data(|d| d.get_temp::<bool>(id)) != Some(want) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(want));
                ctx.data_mut(|d| d.insert_temp(id, want));
            }
        }
        self.window_edges(ctx);
        egui::TopBottomPanel::top("menu")
            .exact_height(menu::MENU_H)
            .frame(egui::Frame::new().fill(t::side()))
            .show_separator_line(false)
            .show(ctx, |ui| self.main_menu(ui));
        egui::TopBottomPanel::top("deck")
            .exact_height(DECK_H)
            .frame(egui::Frame::new().fill(t::side()))
            .show_separator_line(false)
            .show(ctx, |ui| self.deck(ui));
        egui::TopBottomPanel::bottom("status")
            .exact_height(STATUS_H)
            .frame(egui::Frame::new())
            .show_separator_line(false)
            .show(ctx, |ui| self.status_bar(ui));
        if self.ws.browser {
            let panel = egui::SidePanel::left("browser")
                .default_width(browser::width(ctx))
                .width_range(160.0..=360.0)
                .resizable(true)
                .frame(egui::Frame::new().fill(t::side()))
                .show_separator_line(false)
                .show(ctx, |ui| self.browser(ui));
            browser::save_width(ctx, panel.response.rect.width(), self.persist);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t::bg()))
            .show(ctx, |ui| {
                let outer = ui.max_rect();
                let p = ui.painter();
                p.line_segment(
                    [outer.left_top(), outer.right_top()],
                    Stroke::new(1.0_f32, t::edge()),
                );
                p.line_segment(
                    [outer.left_top(), outer.left_bottom()],
                    Stroke::new(1.0_f32, t::edge()),
                );
                let style = workspace::dock_style(ui.style());
                // Floating windows: the tab bar is the title bar, flush with the
                // window edge (no second frame around it).
                ui.style_mut().spacing.window_margin = egui::Margin::ZERO;
                // Floating windows stay inside the work area: its edges and
                // sectors are what they snap to.
                let bounds = outer.shrink(4.0);
                let a = self.ws.active;
                let mut dock =
                    std::mem::replace(&mut self.ws.slots[a], egui_dock::DockState::new(vec![]));
                let modal_open = self.welcome_open || self.pending.is_some();
                ui.add_enabled_ui(!modal_open, |ui| {
                    egui_dock::DockArea::new(&mut dock)
                        .id(egui::Id::new(("kfxl-dock", a)))
                        .style(style)
                        .show_add_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_leaf_close_all_buttons(false)
                        .window_bounds(bounds)
                        .show_inside(ui, &mut workspace::Viewer { app: self });
                });
                self.ws.slots[a] = dock;
                if self.ws.dock().main_surface().is_empty() && self.ws.dock().surfaces_count() <= 1
                {
                    self.empty_workspace(ui, outer);
                }
                self.apply_win_cmds(outer);
                self.sync_tool_tabs();
            });
        self.shortcuts_window(ctx);
        self.manual_window(ctx);
        self.rename_window(ctx);
        self.export_window(ctx);
        self.tap_window(ctx);
        self.import_window(ctx);
        self.fx_windows_ui(ctx);
        self.automation_frame(ctx);
        self.automation_window(ctx);
        self.plugin_picker(ctx);
        self.quantize_window(ctx);
        self.lyrics_window(ctx);
        self.project_frame(ctx);
        self.updater_frame(ctx);
        self.welcome_frame(ctx);
        // egui's own Ctrl+= / Ctrl+− have no limits; keep the layout usable.
        use crate::settings::{UI_SCALE_MAX, UI_SCALE_MIN};
        let z = ctx.zoom_factor();
        if !(UI_SCALE_MIN..=UI_SCALE_MAX).contains(&z) {
            ctx.set_zoom_factor(z.clamp(UI_SCALE_MIN, UI_SCALE_MAX));
        }
        // Whatever changed the size (keys, VIEW menu), Settings remember it.
        let z = ctx.zoom_factor();
        if (z - self.settings.general.ui_scale).abs() > 0.001 {
            self.settings.general.ui_scale = z;
            self.save_settings();
        }
        if self.persist {
            self.ws.zoom = z;
            self.update_window_geometry(ctx, z);
        }
        self.prune_tabs();
        self.autosave_layout(ctx.input(|i| i.time));
        self.drop_overlay(ctx);
        self.plugins_end_frame(ctx);
        kit::finish_pointer_capture(ctx, self.plugins.parent.is_some());
        self.capture_frame(ctx);
        if std::env::var_os("KFXL_FRAME_TRACE").is_some() {
            let ms = frame_start.elapsed().as_secs_f64() * 1000.0;
            if ms > 20.0 {
                eprintln!("slow frame {ms:.1} ms");
            }
        }
        let scripted = self.script.as_ref().is_some_and(|s| s.pending());
        let midi_lit = self.midi_keys.iter().any(|k| *k);
        let plugins_busy = self.plugins.scan_rx.is_some()
            || self
                .plugins
                .live
                .values()
                .any(|l| l.proc.is_none() && l.error.is_none());
        let fast = midi_lit
            || plugins_busy
            || self.is_playing()
            || !self.importing.is_empty()
            || self.vocal_job.is_some()
            || self.audio.as_ref().is_some_and(|a| a.vocal.status.busy())
            || !self.load_queue.is_empty()
            || self.status.0 == Tone::Wait
            || !self.rec.parts.is_empty()
            || scripted;
        ctx.request_repaint_after(Duration::from_millis(if fast { 16 } else { 250 }));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod playlist_tests;
#[cfg(test)]
mod show_tests;

/// The next free `name_N.kfxl` beside `path` (FL: Save new version).
pub(crate) fn next_version(path: &std::path::Path) -> std::path::PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    let (base, n) = match stem.rsplit_once('_') {
        Some((b, d)) if !b.is_empty() && d.parse::<u32>().is_ok() => {
            (b.to_string(), d.parse::<u32>().unwrap())
        }
        _ => (stem.clone(), 1),
    };
    (n + 1..)
        .map(|k| path.with_file_name(format!("{base}_{k}.kfxl")))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Messages from FL Studio rendering imported channels.
pub(crate) enum FreezeMsg {
    Progress(String),
    Done(Result<Vec<(u32, keyframexl::sequence::SamplePreset)>, String>),
}
