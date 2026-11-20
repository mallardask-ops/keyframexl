//! Live link to a running FL Studio (FLEX and other Image-Line generators that
//! do not work inside "FL Studio VSTi", see docs/fl-link.md).
//!
//! The link is an instrument like a plugin: the engine hands it the channel's
//! notes and takes a stereo block back. Notes leave through an ALSA sequencer
//! port ("KeyFrameXL - FL link" in FL's MIDI settings; our own MIDI input skips it); FL plays into its own
//! PipeWire sink, whose monitor we read back. FL's selected channel plays the
//! notes (FL's default routing for a MIDI input).
//!
//! Threads: the audio thread only pushes MIDI bytes into one ring and pops
//! samples from another. A MIDI thread sends, a capture thread runs `parec`
//! and also keeps FL's playback stream on our sink.

use keyframexl::plugin::{OFFLINE, PluginEvent, Processor, Transport};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

/// What the project stores as the "plugin path" of a link channel.
pub const PATH: &str = "fl-link:FL Studio";
/// PipeWire/Pulse sink FL plays into.
pub const SINK: &str = "kfxl_fl_link";
/// ALSA sequencer client and port; Wine shows them as "KeyFrameXL - FL link".
/// The client name keeps `midi_in::skip` from playing our own notes back.
const CLIENT: &str = "KeyFrameXL";
const PORT: &str = "FL link";
pub const DEVICE: &str = "KeyFrameXL - FL link";

/// The relay's input: link channels write here, the relay forwards to `PORT`.
const RELAY_IN: &str = "FL link in";

/// The port FL opens lives as long as the program, in the main process: Wine
/// lists MIDI devices only when FL starts, so a port that came and went with
/// the channel (it runs in its own plugin process) left FL deaf until a
/// restart. Link channels send to the relay's input instead.
static RELAY: std::sync::Mutex<Option<midir::MidiInputConnection<()>>> =
    std::sync::Mutex::new(None);

/// Make the relay once (main process, before a link channel loads).
pub fn ensure_relay() -> Result<(), String> {
    let mut relay = RELAY.lock().unwrap_or_else(|e| e.into_inner());
    if relay.is_some() {
        return Ok(());
    }
    use midir::os::unix::{VirtualInput, VirtualOutput};
    let mut out = midir::MidiOutput::new(CLIENT)
        .map_err(|e| format!("MIDI: {e}"))?
        .create_virtual(PORT)
        .map_err(|e| format!("MIDI port: {e}"))?;
    let mut input = midir::MidiInput::new(CLIENT).map_err(|e| format!("MIDI: {e}"))?;
    input.ignore(midir::Ignore::None);
    let conn = input
        .create_virtual(
            RELAY_IN,
            move |_, msg, _| {
                let _ = out.send(msg);
            },
            (),
        )
        .map_err(|e| format!("MIDI port: {e}"))?;
    *relay = Some(conn);
    Ok(())
}

/// Where a link channel sends its notes: the relay when the main process has
/// one, else a port of its own (a `fl-link-test` run without the program).
fn sender() -> Result<midir::MidiOutputConnection, String> {
    let out = midir::MidiOutput::new(CLIENT).map_err(|e| format!("MIDI: {e}"))?;
    let relay = out.ports().into_iter().find(|p| {
        out.port_name(p)
            .is_ok_and(|n| n.contains(&format!("{CLIENT}:{RELAY_IN}")))
    });
    match relay {
        Some(p) => out
            .connect(&p, "FL link sender")
            .map_err(|e| format!("MIDI: {e}")),
        None => {
            use midir::os::unix::VirtualOutput;
            out.create_virtual(PORT)
                .map_err(|e| format!("MIDI port: {e}"))
        }
    }
}

pub fn is_link(path: &Path) -> bool {
    path.as_os_str() == PATH
}

/// The user's prefix: where FL Studio is installed and the plugins run.
pub(crate) fn prefix() -> PathBuf {
    std::env::var_os("WINEPREFIX")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".wine"))
}

/// FLEX works only in FL Studio itself on a stock Wine: inside "FL Studio
/// VSTi" (any Wine) and in FL on Proton builds its engine crashes while
/// starting. So FL runs on the system Wine.
pub(crate) fn system_wine() -> Option<PathBuf> {
    std::env::var_os("KFXL_FL_WINE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?).find(|d| {
                d.join("wine").is_file() && !d.to_string_lossy().contains("compatibilitytools")
            })
        })
        .filter(|d| d.join("wine").is_file())
}

/// Where FL runs: the user's prefix when the plugins use the system Wine too;
/// otherwise its own prefix, because one prefix cannot serve two Wine builds
/// at once (the plugins' wineserver would refuse FL's).
pub fn fl_prefix() -> PathBuf {
    if let Some(p) = std::env::var_os("KFXL_FL_PREFIX") {
        return p.into();
    }
    if crate::plugins::wine_bin().is_none() {
        prefix()
    } else {
        dirs::data_dir()
            .unwrap_or_default()
            .join("keyframexl/fl-prefix")
    }
}

/// What the last action of the channel window (open FL, reconnect) is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Progress {
    #[default]
    Idle,
    Busy(String),
    Failed(String),
}

static PROGRESS: std::sync::Mutex<Progress> = std::sync::Mutex::new(Progress::Idle);

fn set_progress(p: Progress) {
    *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = p;
}

pub fn progress() -> Progress {
    PROGRESS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Run `job` off the UI thread, showing `what` while it works and its error after.
pub fn run(what: &str, job: impl FnOnce() -> Result<(), String> + Send + 'static) {
    set_progress(Progress::Busy(what.into()));
    std::thread::spawn(move || {
        set_progress(match job() {
            Ok(()) => Progress::Idle,
            Err(e) => Progress::Failed(e),
        });
    });
}

pub(crate) fn wine_cmd(bin: &Path, prefix: &Path) -> Command {
    let mut c = Command::new(bin.join("wine"));
    let path = std::iter::once(bin.to_path_buf()).chain(
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    c.env("WINEPREFIX", prefix)
        .env("PATH", std::env::join_paths(path).unwrap_or_default())
        .env_remove("WINEDLLPATH")
        .env_remove("WINEDLLOVERRIDES")
        .env_remove("LD_LIBRARY_PATH");
    // A separate prefix must not put menu entries and file associations
    // pointing at itself into the user's desktop.
    if prefix != self::prefix() {
        c.env("WINEDLLOVERRIDES", "winemenubuilder.exe=d");
    }
    c
}

/// Registry sections of `reg` (Wine's own text format) whose key starts with
/// `root`, verbatim. Duplicate MIDI inputs FL keeps for one device ("… #2")
/// are switched off: FL cannot open both and stops at an error.
fn registry_sections(reg: &str, root: &str) -> String {
    let mut out = String::new();
    let mut keep = false;
    let mut duplicate = false;
    for line in reg.lines() {
        if let Some(rest) = line.strip_prefix('[') {
            keep = rest.starts_with(root);
            let name = line.split(']').next().unwrap_or_default();
            duplicate = keep
                && name.contains("\\\\MIDI input\\\\")
                && name
                    .rsplit_once(" #")
                    .is_some_and(|(_, n)| n.chars().all(|c| c.is_ascii_digit()));
            if keep {
                out.push('\n');
            }
        }
        if keep {
            if duplicate && line.starts_with("\"Enabled\"=") {
                out.push_str("\"Enabled\"=\"0\"\n");
            } else {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// Registry keys FL needs, per file: its own settings and registration, and
/// the "FL Studio ASIO" driver (its ASIO entry and COM class). Without the
/// driver FL falls back to DirectSound with a 4096-frame buffer (~200 ms).
fn registry_roots(system_reg: &str) -> [(&'static str, Vec<String>); 2] {
    let clsid = system_reg
        .split("[Software\\\\ASIO\\\\FL Studio ASIO]")
        .nth(1)
        .and_then(|rest| {
            rest.lines()
                .take(6)
                .find_map(|l| l.strip_prefix("\"CLSID\"=\""))
        })
        .map(|v| v.trim_end_matches('"').to_string());
    let mut system = vec![
        "Software\\\\Image-Line".to_string(),
        "Software\\\\ASIO\\\\FL Studio ASIO".into(),
        "Software\\\\Wow6432Node\\\\ASIO\\\\FL Studio ASIO".into(),
    ];
    if let Some(c) = clsid {
        system.push(format!("Software\\\\Classes\\\\CLSID\\\\{c}"));
        system.push(format!(
            "Software\\\\Classes\\\\Wow6432Node\\\\CLSID\\\\{c}"
        ));
    }
    [
        ("user.reg", vec!["Software\\\\Image-Line".to_string()]),
        ("system.reg", system),
    ]
}

/// Append to FL's prefix the keys from `registry_roots` it does not have yet
/// (its wineserver must be down: Wine rewrites these files on exit).
fn copy_registry(src: &Path, bin: &Path, dst: &Path) -> Result<(), String> {
    let _ = Command::new(bin.join("wineserver"))
        .env("WINEPREFIX", dst)
        .arg("-w")
        .status();
    let system_reg =
        String::from_utf8_lossy(&std::fs::read(src.join("system.reg")).map_err(|e| e.to_string())?)
            .into_owned();
    for (file, roots) in registry_roots(&system_reg) {
        let from =
            String::from_utf8_lossy(&std::fs::read(src.join(file)).map_err(|e| e.to_string())?)
                .into_owned();
        let have =
            String::from_utf8_lossy(&std::fs::read(dst.join(file)).map_err(|e| e.to_string())?)
                .into_owned();
        let add: String = roots
            .iter()
            .filter(|r| !have.contains(&format!("\n[{r}")))
            .map(|r| registry_sections(&from, r))
            .collect();
        if add.is_empty() {
            continue;
        }
        set_progress(Progress::Busy(
            "Copying FL Studio's settings and registration…".into(),
        ));
        let mut to = std::fs::OpenOptions::new()
            .append(true)
            .open(dst.join(file))
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        to.write_all(add.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Make FL's own prefix once: a fresh one on the system Wine with FL Studio,
/// FL Cloud Plugins and their registry from the user's prefix. Files are
/// reflinked where the filesystem can (btrfs: no extra space). Documents in
/// both prefixes are the same folder, so projects, presets and FLEX packs
/// stay shared.
pub(crate) fn prepare_fl_prefix(bin: &Path, dst: &Path) -> Result<(), String> {
    let src = prefix();
    if dst == src {
        return Ok(());
    }
    if dst.join("drive_c/Program Files/Image-Line").is_dir() && dst.join("system.reg").is_file() {
        // A prefix made by an older version may lack keys added since.
        return copy_registry(&src, bin, dst);
    }
    set_progress(Progress::Busy("Creating FL Studio's prefix…".into()));
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let ok = wine_cmd(bin, dst)
        .args(["wineboot", "-i"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    let _ = Command::new(bin.join("wineserver"))
        .env("WINEPREFIX", dst)
        .arg("-w")
        .status();
    if !ok.success() {
        return Err("wineboot failed for FL Studio's prefix".into());
    }
    set_progress(Progress::Busy("Copying FL Studio into its prefix…".into()));
    let user = std::env::var("USER").unwrap_or_default();
    for dir in [
        "Program Files/Image-Line",
        "Program Files/FL Cloud Plugins",
        "ProgramData/FL Cloud Plugins",
        "ProgramData/Image-Line",
        &format!("users/{user}/AppData/Roaming/Image-Line"),
    ] {
        let from = src.join("drive_c").join(dir);
        if !from.exists() {
            continue;
        }
        let to = dst.join("drive_c").join(dir);
        let _ = std::fs::create_dir_all(to.parent().unwrap_or(dst));
        let st = Command::new("cp")
            .args(["-a", "--reflink=auto"])
            .arg(&from)
            .arg(&to)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(format!("Cannot copy {dir}"));
        }
    }
    copy_registry(&src, bin, dst)?;
    Ok(())
}

/// FL Studio for offline renders (`flp_freeze`): the system Wine, a prefix of
/// its own (FL may already be open for the live link; one prefix runs one
/// FL), and FL's program there. Made on first use like the link's prefix.
pub fn render_setup() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let bin = system_wine().ok_or("FL Studio renders need the system Wine (wine in PATH)")?;
    let pfx = dirs::data_dir()
        .ok_or("No data folder")?
        .join("keyframexl/fl-render-prefix");
    prepare_fl_prefix(&bin, &pfx)?;
    let exe = find_fl_in(&pfx).ok_or("FL Studio was not found in the Wine prefix")?;
    Ok((bin, pfx, exe))
}

/// `wine` in `prefix` with `bin` first in PATH (the render's FL).
pub fn render_command(bin: &Path, prefix: &Path) -> Command {
    wine_cmd(bin, prefix)
}

/// The newest `FL64.exe` in the Wine prefix (FL Studio 2025 before 21).
pub fn find_fl() -> Option<PathBuf> {
    find_fl_in(&prefix())
}

pub(crate) fn find_fl_in(prefix: &Path) -> Option<PathBuf> {
    let root = prefix.join("drive_c/Program Files/Image-Line");
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|d| d.path().join("FL64.exe"))
        .filter(|p| p.is_file())
        .collect();
    found.sort_by_key(|p| fl_version(p));
    found.pop()
}

/// "FL Studio 2025" → 2025, "FL Studio 21" → 21 (only for ordering).
fn fl_version(exe: &Path) -> u32 {
    let dir = exe
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n: u32 = dir
        .rsplit(' ')
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if n < 100 { 2000 + n } else { n }
}

/// FL's registry key for that install: FL Studio 2025 keeps "FL Studio 25".
pub(crate) fn registry_key(exe: &Path) -> String {
    format!("FL Studio {}", fl_version(exe) % 100)
}

/// Whether an FL Studio process runs (ours or started by hand).
pub fn fl_running() -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|d| {
            std::fs::read(d.path().join("cmdline")).is_ok_and(|c| {
                let c = String::from_utf8_lossy(&c).to_lowercase();
                c.contains("fl64.exe") && !c.contains("yabridge")
            })
        })
}

/// Whether our sequencer port exists (a link channel is loaded somewhere).
pub fn port_present() -> bool {
    std::fs::read_to_string("/proc/asound/seq/clients")
        .is_ok_and(|s| s.contains(&format!("\"{CLIENT}\"")) && s.contains(&format!("\"{PORT}\"")))
}

/// Make our sink if it is not there yet. Returns the module to unload, if we
/// loaded it.
fn ensure_sink() -> Result<Option<u32>, String> {
    let list = Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .map_err(|e| format!("pactl: {e}"))?;
    if String::from_utf8_lossy(&list.stdout)
        .lines()
        .any(|l| l.split('\t').nth(1) == Some(SINK))
    {
        return Ok(None);
    }
    let out = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={SINK}"),
            "rate=48000",
            "channels=2",
            "sink_properties=device.description=KeyframeXL-FL-link",
        ])
        .output()
        .map_err(|e| format!("pactl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Cannot create the FL link sink: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().parse().ok())
}

/// Move FL's playback streams onto our sink (FL started by hand plays to the
/// default output; PULSE_SINK covers only FL started by `open_fl`).
fn capture_fl_streams() {
    let Ok(out) = Command::new("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .output()
    else {
        return;
    };
    let Ok(list) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return;
    };
    let sink = sink_index();
    for s in list {
        let props = &s["properties"];
        let binary = props["application.process.binary"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let name = props["application.name"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        if !(binary.contains("fl64") || name.contains("fl64") || name.contains("fl studio")) {
            continue;
        }
        if sink.is_some() && s["sink"].as_u64() == sink {
            continue;
        }
        if let Some(index) = s["index"].as_u64() {
            let _ = Command::new("pactl")
                .args(["move-sink-input", &index.to_string(), SINK])
                .output();
        }
    }
}

fn sink_index() -> Option<u64> {
    let out = Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.split('\t').nth(1) == Some(SINK))?
        .split('\t')
        .next()?
        .parse()
        .ok()
}

/// Start FL Studio with its sound on our sink and our port enabled as a MIDI
/// input (FL's Options → MIDI settings, written before FL reads them), on the
/// system Wine (see `system_wine`). Nothing happens when FL already runs.
/// Blocks while FL's prefix is made the first time: call it off the UI thread.
pub fn open_fl(project: Option<&Path>) -> Result<(), String> {
    if fl_running() {
        return Ok(());
    }
    let bin =
        system_wine().ok_or("FLEX needs the system Wine (wine in PATH), which was not found")?;
    let pfx = fl_prefix();
    prepare_fl_prefix(&bin, &pfx)?;
    let exe = find_fl_in(&pfx).ok_or("FL Studio was not found in the Wine prefix")?;
    ensure_sink()?;
    set_progress(Progress::Busy("Starting FL Studio…".into()));
    let key = format!(
        "HKCU\\Software\\Image-Line\\{}\\Devices\\MIDI input\\{DEVICE}",
        registry_key(&exe)
    );
    let _ = wine_cmd(&bin, &pfx)
        .args([
            "reg", "add", &key, "/v", "Enabled", "/t", "REG_SZ", "/d", "1", "/f",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let mut c = wine_cmd(&bin, &pfx);
    c.arg(&exe);
    if let Some(p) = project {
        c.arg(p);
    }
    // FL Studio ASIO plays through Wine's PulseAudio driver; asking Pulse
    // (PipeWire) for 10 ms buffers cuts ~15 ms of the round trip. 5 ms
    // makes it run dry (clicks).
    c.env("PULSE_LATENCY_MSEC", "10");
    c.current_dir(exe.parent().unwrap_or(Path::new("/")))
        .env("PULSE_SINK", SINK)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // KFXL_FL_LOG=file: FL's Wine output, for diagnosis (with WINEDEBUG).
    if let Some(log) = std::env::var_os("KFXL_FL_LOG").and_then(|f| std::fs::File::create(f).ok()) {
        c.stderr(log);
    }
    c.spawn()
        .map_err(|e| format!("Cannot start FL Studio: {e}"))?;
    Ok(())
}

/// Whether FL Studio has opened our port: the port has a subscriber that is
/// not PipeWire's own bridge nor us (`/proc/asound/seq/clients`).
pub fn fl_listens() -> bool {
    std::fs::read_to_string("/proc/asound/seq/clients").is_ok_and(|s| listens_in(&s))
}

fn listens_in(clients: &str) -> bool {
    // Client number → name, and the subscribers of our port.
    let mut names = std::collections::HashMap::new();
    let mut ours = false;
    let mut in_port = false;
    let mut subscribers = Vec::new();
    for line in clients.lines() {
        if let Some(rest) = line.strip_prefix("Client ") {
            let (n, name) = rest.split_once(" : ").unwrap_or_default();
            let name = name.split('"').nth(1).unwrap_or_default();
            names.insert(n.trim().to_string(), name.to_string());
            ours = name == CLIENT;
            in_port = false;
        } else if let Some(rest) = line.trim_start().strip_prefix("Port ") {
            in_port = ours && rest.contains(&format!("\"{PORT}\""));
        } else if in_port && let Some(to) = line.trim_start().strip_prefix("Connecting To:") {
            subscribers.extend(
                to.split(',')
                    .filter_map(|c| Some(c.trim().split(':').next()?.to_string())),
            );
        }
    }
    subscribers.iter().any(|c| {
        let name = names.get(c).map(String::as_str).unwrap_or_default();
        !name.starts_with("PipeWire") && name != CLIENT
    })
}

/// What FLEX needs in FL's prefix, as reported to work (vkd3d-proton's
/// Direct3D 12 and the Verdana font), and the versions involved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preflight {
    /// FL Studio's version year (2025, 2021), 0 when FL is not installed.
    pub fl: u32,
    pub vkd3d: bool,
    pub verdana: bool,
    /// `wine --version` of the Wine FL runs on.
    pub wine: Option<String>,
}

impl Preflight {
    pub fn of(prefix: &Path) -> Self {
        let win = prefix.join("drive_c/windows");
        // Wine's own d3d12.dll is a builtin stub; vkd3d-proton's is not.
        let vkd3d = ["system32", "syswow64"].iter().any(|d| {
            std::fs::read(win.join(d).join("d3d12.dll")).is_ok_and(|b| {
                !b.windows(16).any(|w| w == b"Wine builtin DLL")
                    && !b.windows(20).any(|w| w == b"Wine placeholder DLL")
            })
        });
        let verdana = std::fs::read_dir(win.join("Fonts"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|f| {
                f.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .starts_with("verdana")
            });
        let wine = system_wine().and_then(|bin| {
            let out = Command::new(bin.join("wine"))
                .arg("--version")
                .output()
                .ok()?;
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        });
        Self {
            fl: find_fl_in(prefix).map(|e| fl_version(&e)).unwrap_or(0),
            vkd3d,
            verdana,
            wine,
        }
    }

    /// FL 2026.1.4 and later fail Wine's certificate check without Wine
    /// MR 11824 (in DAW-GE-Proton11-6c, not in stock Wine yet).
    pub fn needs_cert_patch(&self) -> bool {
        self.fl >= 2026
    }
}

/// What the channel window shows. A monitor thread refreshes it once a
/// second (it runs `pactl` and reads /proc), so the UI never waits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub fl: bool,
    pub port: bool,
    /// FL has our port open as a MIDI input.
    pub listens: bool,
    pub sink: bool,
    pub stats: Option<Stats>,
    pub preflight: Preflight,
}

static STATUS: std::sync::Mutex<Option<Status>> = std::sync::Mutex::new(None);

pub fn status() -> Status {
    static MONITOR: std::sync::Once = std::sync::Once::new();
    MONITOR.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("fl-link-status".into())
            .spawn(|| {
                let mut preflight = Preflight::default();
                let mut tick = 0u64;
                loop {
                    if tick.is_multiple_of(10) {
                        preflight = Preflight::of(&fl_prefix());
                    }
                    let s = Status {
                        fl: fl_running(),
                        port: port_present(),
                        listens: fl_listens(),
                        sink: sink_index().is_some(),
                        stats: read_stats(),
                        preflight: preflight.clone(),
                    };
                    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
                    std::thread::sleep(Duration::from_secs(1));
                    tick += 1;
                }
            });
    });
    STATUS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// The channel's state, from what the monitor saw and the last action.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    /// FL Studio is not running.
    Off,
    Starting(String),
    Connected,
    /// Works, but something needs a look.
    Degraded(String),
    Error(String),
}

pub fn state(st: &Status, p: &Progress) -> LinkState {
    if let Progress::Failed(e) = p {
        return LinkState::Error(e.clone());
    }
    if let Some(e) = st.stats.as_ref().and_then(|s| s.error.clone()) {
        return LinkState::Error(e);
    }
    if let Progress::Busy(what) = p {
        return LinkState::Starting(what.clone());
    }
    if !st.fl {
        return LinkState::Off;
    }
    if !st.port {
        return LinkState::Error("The MIDI port is missing: reload this channel.".into());
    }
    if !st.sink {
        return LinkState::Error("The link's sound sink is missing: press Reconnect.".into());
    }
    if !st.listens {
        return LinkState::Degraded(
            "FL Studio has not opened the \"KeyFrameXL - FL link\" input: restart FL Studio."
                .into(),
        );
    }
    match &st.stats {
        None => LinkState::Degraded("The link channel is not running.".into()),
        Some(s) if s.recent_underruns > 0 => LinkState::Degraded(format!(
            "The sound has gaps ({} in the last seconds): close other programs or raise the buffer.",
            s.recent_underruns
        )),
        Some(_) => LinkState::Connected,
    }
}

/// Bring the link back after PipeWire restarted or FL moved its output:
/// the port relay, the sink, FL's streams on it. Blocks (`pactl`).
pub fn reconnect() -> Result<(), String> {
    ensure_relay()?;
    ensure_sink()?;
    capture_fl_streams();
    if !port_present() {
        return Err("The MIDI port is missing: reload this channel.".into());
    }
    if fl_running() && !fl_listens() {
        return Err(
            "FL Studio has not opened the \"KeyFrameXL - FL link\" input: restart FL Studio."
                .into(),
        );
    }
    Ok(())
}

/// What a running link measured. The link may run in a plugin process of
/// its own, so it writes these to a small file the channel window reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Note → sound in frames, 0 = not measured yet.
    pub round_trip: u32,
    pub rate: u32,
    pub underruns: u32,
    /// Underruns over the last ~5 s.
    pub recent_underruns: u32,
    /// Frames of FL's sound queued past the block being played.
    pub queue: u32,
    /// Seconds since FL's sound was last heard (None: not yet).
    pub silent_secs: Option<f32>,
    /// The capture could not run (no `parec`).
    pub error: Option<String>,
}

impl Stats {
    pub fn round_trip_ms(&self) -> Option<f32> {
        (self.round_trip > 0 && self.rate > 0)
            .then(|| self.round_trip as f32 * 1000.0 / self.rate as f32)
    }

    fn to_text(&self) -> String {
        format!(
            "round_trip {}\nrate {}\nunderruns {}\nrecent {}\nqueue {}\nsilent {}\nerror {}\n",
            self.round_trip,
            self.rate,
            self.underruns,
            self.recent_underruns,
            self.queue,
            self.silent_secs.map(|s| s.to_string()).unwrap_or_default(),
            self.error.clone().unwrap_or_default()
        )
    }

    fn from_text(t: &str) -> Self {
        let mut s = Self::default();
        for line in t.lines() {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            match k {
                "round_trip" => s.round_trip = v.parse().unwrap_or(0),
                "rate" => s.rate = v.parse().unwrap_or(0),
                "underruns" => s.underruns = v.parse().unwrap_or(0),
                "recent" => s.recent_underruns = v.parse().unwrap_or(0),
                "queue" => s.queue = v.parse().unwrap_or(0),
                "silent" => s.silent_secs = v.parse().ok(),
                "error" if !v.is_empty() => s.error = Some(v.to_string()),
                _ => {}
            }
        }
        s
    }
}

fn stats_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("keyframexl-fl-link.stats")
}

/// The running link's numbers; None when no link wrote them in the last 3 s.
fn read_stats() -> Option<Stats> {
    let path = stats_path();
    let fresh = std::fs::metadata(&path)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .is_ok_and(|d| d < Duration::from_secs(3));
    fresh.then(|| Stats::from_text(&std::fs::read_to_string(path).unwrap_or_default()))
}

/// Status the link threads publish (the instance's own process).
#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    /// Round trip note → sound in frames, 0 = not measured yet.
    round_trip: AtomicU32,
    underruns: AtomicU32,
    queue: AtomicU32,
    /// Frames played, and the frame FL's sound was last heard at (+1; 0 = never).
    clock: AtomicU64,
    last_loud: AtomicU64,
    error: std::sync::Mutex<Option<String>>,
}

/// Write the link's numbers for the channel window twice a second.
fn publish_stats(rate: u32, s: &Shared) {
    let mut history = std::collections::VecDeque::new();
    while !s.stop.load(Ordering::Relaxed) {
        let underruns = s.underruns.load(Ordering::Relaxed);
        history.push_back(underruns);
        if history.len() > 10 {
            history.pop_front();
        }
        let clock = s.clock.load(Ordering::Relaxed);
        let loud = s.last_loud.load(Ordering::Relaxed);
        let stats = Stats {
            round_trip: s.round_trip.load(Ordering::Relaxed),
            rate,
            underruns,
            recent_underruns: underruns - history.front().copied().unwrap_or(underruns),
            queue: s.queue.load(Ordering::Relaxed),
            silent_secs: (loud > 0)
                .then(|| clock.saturating_sub(loud - 1) as f32 / rate.max(1) as f32),
            error: s.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        };
        let path = stats_path();
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, stats.to_text()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = std::fs::remove_file(stats_path());
}

/// The engine side of a link channel.
pub struct FlLink {
    midi: rtrb::Producer<[u8; 3]>,
    audio: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
    module: Option<u32>,
    /// Frames played so far, and when the last note went out (round trip).
    clock: u64,
    pending: Option<u64>,
    quiet: u32,
    trips: Vec<u32>,
    /// Queued audio past one block that we keep (frames, see `MAX_QUEUE`).
    max_queue: usize,
    /// FL has sent audio since the link started (a short block after that
    /// is an underrun, heard as a click).
    flowing: bool,
}

impl FlLink {
    pub fn new(rate: u32) -> Result<Self, String> {
        let module = ensure_sink()?;
        let (midi, mut midi_rx) = rtrb::RingBuffer::<[u8; 3]>::new(1024);
        let (mut audio_tx, audio) = rtrb::RingBuffer::<f32>::new(rate as usize * 2);
        let shared = Arc::new(Shared::default());
        let mut port = sender()?;
        let s = shared.clone();
        std::thread::Builder::new()
            .name("fl-link-midi".into())
            .spawn(move || {
                while !s.stop.load(Ordering::Relaxed) {
                    while let Ok(m) = midi_rx.pop() {
                        let len = match m[0] & 0xF0 {
                            0xC0 | 0xD0 => 2,
                            _ => 3,
                        };
                        let _ = port.send(&m[..len]);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                // Nothing may keep sounding in FL after the channel goes.
                let _ = port.send(&[0xB0, 123, 0]);
            })
            .map_err(|e| e.to_string())?;
        // An idle sink's monitor sends nothing, so this cannot wait for audio.
        let s = shared.clone();
        std::thread::Builder::new()
            .name("fl-link-streams".into())
            .spawn(move || {
                while !s.stop.load(Ordering::Relaxed) {
                    capture_fl_streams();
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
            .map_err(|e| e.to_string())?;
        let s = shared.clone();
        std::thread::Builder::new()
            .name("fl-link-stats".into())
            .spawn(move || publish_stats(rate, &s))
            .map_err(|e| e.to_string())?;
        let s = shared.clone();
        std::thread::Builder::new()
            .name("fl-link-audio".into())
            .spawn(move || capture(rate, &s, &mut audio_tx))
            .map_err(|e| e.to_string())?;
        let max_queue = std::env::var("KFXL_FL_QUEUE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(MAX_QUEUE);
        Ok(Self {
            midi,
            audio,
            shared,
            module,
            clock: 0,
            pending: None,
            quiet: 0,
            trips: Vec::new(),
            max_queue,
            flowing: false,
        })
    }

    fn send(&mut self, m: [u8; 3]) {
        let _ = self.midi.push(m);
    }
}

/// Read FL's sound from the sink monitor until the link stops; restart
/// `parec` if it ends, and keep FL's streams on the sink.
fn capture(rate: u32, s: &Shared, out: &mut rtrb::Producer<f32>) {
    while !s.stop.load(Ordering::Relaxed) {
        let child = Command::new("parec")
            .args([
                "-d",
                &format!("{SINK}.monitor"),
                "--format=float32le",
                &format!("--rate={rate}"),
                "--channels=2",
                "--raw",
                "--latency-msec=5",
                "--client-name=KeyframeXL FL link",
            ])
            .env("PULSE_LATENCY_MSEC", "10")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => {
                *s.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
                c
            }
            Err(e) => {
                *s.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!(
                    "Cannot run parec ({e}): install pulseaudio-utils (or your distribution's parec)."
                ));
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let mut pipe = child.stdout.take().expect("piped");
        let mut buf = vec![0u8; 4096];
        let mut carry = Vec::new();
        while !s.stop.load(Ordering::Relaxed) {
            let n = match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            carry.extend_from_slice(&buf[..n]);
            let whole = carry.len() / 4 * 4;
            for b in carry[..whole].as_chunks::<4>().0 {
                // A full ring means the engine is not pulling (stopped): drop.
                let _ = out.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
            carry.drain(..whole);
        }
        stop_child(&mut child);
    }
}

fn stop_child(c: &mut Child) {
    let _ = c.kill();
    let _ = c.wait();
}

impl Drop for FlLink {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // The stats thread may not get to it before the process ends.
        let _ = std::fs::remove_file(stats_path());
        if let Some(m) = self.module {
            let _ = Command::new("pactl")
                .args(["unload-module", &m.to_string()])
                .output();
        }
    }
}

/// Most audio we keep queued past the block being played (frames) before
/// dropping the excess down to half of it: FL and the engine run on
/// different clocks, and whatever stays queued is heard as delay. `parec`
/// delivers about every 5 ms, so a little above that keeps blocks whole.
const MAX_QUEUE: usize = 512;

impl Processor for FlLink {
    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], _transport: &Transport) {
        let frames = io.len() / 2;
        // An export runs faster than real time: FL cannot follow it.
        if OFFLINE.load(Ordering::Relaxed) {
            io.fill(0.0);
            return;
        }
        for e in events {
            match *e {
                PluginEvent::NoteOn {
                    pitch,
                    velocity,
                    at,
                } => {
                    self.send([0x90, pitch, velocity.max(1)]);
                    if self.pending.is_none() && self.quiet > 4 {
                        self.pending = Some(self.clock + at as u64);
                    }
                }
                PluginEvent::NoteOff { pitch, .. } => self.send([0x80, pitch, 0]),
                PluginEvent::Midi { bytes, .. } => self.send(bytes),
            }
        }
        let queued = self.audio.slots() / 2;
        if queued > frames + self.max_queue {
            let drop = (queued - frames - self.max_queue / 2) * 2;
            if let Ok(c) = self.audio.read_chunk(drop) {
                c.commit_all();
            }
        }
        let take = (self.audio.slots() / 2).min(frames) * 2;
        match self.audio.read_chunk(take) {
            Ok(c) => {
                let (a, b) = c.as_slices();
                io[..a.len()].copy_from_slice(a);
                io[a.len()..a.len() + b.len()].copy_from_slice(b);
                c.commit_all();
            }
            Err(_) => io[..take].fill(0.0),
        }
        io[take..].fill(0.0);
        self.shared
            .queue
            .store((self.audio.slots() / 2) as u32, Ordering::Relaxed);
        if take > 0 {
            self.flowing = true;
        }
        if self.flowing && take < io.len() {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
        // Round trip: the first loud sample after a note sent into silence.
        let loud = io
            .as_chunks::<2>()
            .0
            .iter()
            .position(|f| f[0].abs().max(f[1].abs()) > 1e-3);
        match (loud, self.pending) {
            (Some(i), Some(sent)) => {
                let trip = (self.clock + i as u64).saturating_sub(sent) as u32;
                self.pending = None;
                self.trips.push(trip);
                if self.trips.len() > 5 {
                    self.trips.remove(0);
                }
                let mut t = self.trips.clone();
                t.sort_unstable();
                self.shared
                    .round_trip
                    .store(t[t.len() / 2], Ordering::Relaxed);
            }
            (Some(_), None) => {}
            (None, _) => {}
        }
        if let Some(i) = loud {
            self.shared
                .last_loud
                .store(self.clock + i as u64 + 1, Ordering::Relaxed);
        }
        self.quiet = if loud.is_some() {
            0
        } else {
            self.quiet.saturating_add(1)
        };
        // A note that never sounded (FL closed, another channel selected).
        if self
            .pending
            .is_some_and(|p| self.clock.saturating_sub(p) > 48_000)
        {
            self.pending = None;
        }
        self.clock += frames as u64;
        self.shared.clock.store(self.clock, Ordering::Relaxed);
    }

    fn latency(&self, _frames: usize) -> usize {
        // Rounded to 256 frames so small changes do not move everything else.
        let t = self.shared.round_trip.load(Ordering::Relaxed) as usize;
        (t / 256 * 256).min(keyframexl::plugin::MAX_PDC - 1)
    }
}

/// `keyframexl fl-link-test [project.flp]`: FL loads (KFXL_FL_WAIT seconds,
/// default 60), then C4 sounds once a second for 8 s; prints each note's peak
/// and the measured round trip.
pub fn fl_link_test(project: Option<&Path>) -> Result<(), String> {
    let rate = 48_000u32;
    ensure_relay()?;
    let mut link = FlLink::new(rate)?;
    // KFXL_FL_NO_OPEN: check only our side (ports, relay), FL is not started.
    if std::env::var_os("KFXL_FL_NO_OPEN").is_none() {
        open_fl(project)?;
    }
    let wait: u64 = std::env::var("KFXL_FL_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let block = 480usize;
    let t = Transport {
        rate,
        tempo: 120.0,
        playing: false,
        frame: 0,
    };
    let mut io = vec![0.0f32; block * 2];
    let start = std::time::Instant::now();
    let mut n = 0u64;
    let pace = |n: u64| {
        let due = start + Duration::from_secs_f64(n as f64 * block as f64 / rate as f64);
        if let Some(d) = due.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(d);
        }
    };
    let blocks_per_s = rate as u64 / block as u64;
    for _ in 0..wait * blocks_per_s {
        link.process(&[], &mut io, &t);
        n += 1;
        pace(n);
    }
    println!("FL running: {}, port: {}", fl_running(), port_present());
    for k in 0..8 {
        let mut peak = 0f32;
        for b in 0..blocks_per_s {
            let ev = match b {
                0 => vec![PluginEvent::NoteOn {
                    at: 0,
                    pitch: 60,
                    velocity: 100,
                }],
                30 => vec![PluginEvent::NoteOff { at: 0, pitch: 60 }],
                _ => vec![],
            };
            link.process(&ev, &mut io, &t);
            peak = io.iter().fold(peak, |p, x| p.max(x.abs()));
            n += 1;
            pace(n);
        }
        println!(
            "note {k}: peak {peak:.4}, round trip {} frames, queued {} frames, underruns {}",
            link.shared.round_trip.load(Ordering::Relaxed),
            link.audio.slots() / 2,
            link.shared.underruns.load(Ordering::Relaxed)
        );
    }
    // The channel reloads (new plugin process in the program): FL keeps the
    // relay's port and must still hear the new instance.
    drop(link);
    let mut link = FlLink::new(rate)?;
    for k in 0..3 {
        let mut peak = 0f32;
        for b in 0..blocks_per_s {
            let ev = match b {
                0 => vec![PluginEvent::NoteOn {
                    at: 0,
                    pitch: 64,
                    velocity: 100,
                }],
                30 => vec![PluginEvent::NoteOff { at: 0, pitch: 64 }],
                _ => vec![],
            };
            link.process(&ev, &mut io, &t);
            peak = io.iter().fold(peak, |p, x| p.max(x.abs()));
            n += 1;
            pace(n);
        }
        println!("after reload, note {k}: peak {peak:.4}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_2025_after_21_and_map_to_registry_keys() {
        let a = Path::new("/p/Image-Line/FL Studio 21/FL64.exe");
        let b = Path::new("/p/Image-Line/FL Studio 2025/FL64.exe");
        assert!(fl_version(b) > fl_version(a));
        assert_eq!(registry_key(b), "FL Studio 25");
        assert_eq!(registry_key(a), "FL Studio 21");
    }

    #[test]
    fn registry_copy_keeps_image_line_keys_and_switches_off_duplicate_midi_inputs() {
        let reg = "WINE REGISTRY Version 2\n\n[Software\\\\Wine] 1\n\"A\"=\"1\"\n\n[Software\\\\Image-Line\\\\FL Studio 25\\\\Devices\\\\MIDI input\\\\PipeWire - input #2] 2\n\"Enabled\"=\"1\"\n\"Port\"=\"-1\"\n\n[Software\\\\Image-Line\\\\FL Studio 25\\\\Devices\\\\MIDI input\\\\MPK mini] 3\n\"Enabled\"=\"1\"\n";
        let out = registry_sections(reg, "Software\\\\Image-Line");
        assert!(!out.contains("Wine]"));
        assert!(out.contains("PipeWire - input #2] 2\n\"Enabled\"=\"0\"\n\"Port\"=\"-1\""));
        assert!(out.contains("MPK mini] 3\n\"Enabled\"=\"1\""));
    }

    #[test]
    fn registry_roots_include_the_fl_asio_driver_class() {
        let sys = "[Software\\\\ASIO\\\\FL Studio ASIO] 1\n#time=1\n\"CLSID\"=\"{188135E1-7171-3434-854F-01A3C71F3DF9}\"\n\"Description\"=\"FL Studio ASIO\"\n";
        let [(_, user), (_, system)] = registry_roots(sys);
        assert_eq!(user, ["Software\\\\Image-Line"]);
        assert!(system.contains(
            &"Software\\\\Classes\\\\CLSID\\\\{188135E1-7171-3434-854F-01A3C71F3DF9}".to_string()
        ));
        assert!(system.contains(&"Software\\\\ASIO\\\\FL Studio ASIO".to_string()));
    }

    #[test]
    fn link_path_is_recognised() {
        assert!(is_link(Path::new(PATH)));
        assert!(!is_link(Path::new("/home/u/.vst3/x.vst3")));
    }

    #[test]
    fn fl_listening_means_a_subscriber_other_than_pipewire() {
        let clients = "Client 128 : \"KeyFrameXL\" [User Legacy]
  Port   0 : \"FL link\" (R-e-) [Out]
    Connecting To: 144:0
  Port   1 : \"FL link in\" (-We-) [In]
Client 144 : \"PipeWire-System\" [User Legacy]
  Port   0 : \"input\" (rwe-) [In/Out]
";
        assert!(!listens_in(clients));
        let fl = clients.replace("144:0\n", "144:0, 150:0\n")
            + "Client 150 : \"FL64\" [User Legacy]\n  Port   0 : \"in\" (-We-) [In]\n";
        assert!(listens_in(&fl));
        assert!(!listens_in(""));
    }

    #[test]
    fn link_state_follows_status_and_progress() {
        let good = Status {
            fl: true,
            port: true,
            listens: true,
            sink: true,
            stats: Some(Stats {
                round_trip: 2688,
                rate: 48_000,
                ..Stats::default()
            }),
            preflight: Preflight::default(),
        };
        let idle = Progress::Idle;
        assert_eq!(state(&good, &idle), LinkState::Connected);
        assert_eq!(
            state(
                &Status {
                    fl: false,
                    ..good.clone()
                },
                &idle
            ),
            LinkState::Off
        );
        assert!(matches!(
            state(&Status::default(), &Progress::Busy("Starting".into())),
            LinkState::Starting(_)
        ));
        assert!(matches!(
            state(&good, &Progress::Failed("no wine".into())),
            LinkState::Error(e) if e == "no wine"
        ));
        assert!(matches!(
            state(
                &Status {
                    listens: false,
                    ..good.clone()
                },
                &idle
            ),
            LinkState::Degraded(_)
        ));
        assert!(matches!(
            state(
                &Status {
                    sink: false,
                    ..good.clone()
                },
                &idle
            ),
            LinkState::Error(_)
        ));
        let gappy = Status {
            stats: Some(Stats {
                recent_underruns: 3,
                ..Stats::default()
            }),
            ..good.clone()
        };
        assert!(matches!(state(&gappy, &idle), LinkState::Degraded(m) if m.contains('3')));
        let broken = Status {
            stats: Some(Stats {
                error: Some("Cannot run parec".into()),
                ..Stats::default()
            }),
            ..good
        };
        assert!(matches!(state(&broken, &idle), LinkState::Error(_)));
    }

    #[test]
    fn stats_survive_the_file_and_give_milliseconds() {
        let s = Stats {
            round_trip: 2688,
            rate: 48_000,
            underruns: 4,
            recent_underruns: 1,
            queue: 128,
            silent_secs: Some(0.25),
            error: Some("Cannot run parec (x)".into()),
        };
        assert_eq!(Stats::from_text(&s.to_text()), s);
        assert_eq!(s.round_trip_ms(), Some(56.0));
        assert_eq!(Stats::default().round_trip_ms(), None);
        let none = Stats::default();
        assert_eq!(Stats::from_text(&none.to_text()), none);
    }

    #[test]
    fn preflight_finds_vkd3d_and_verdana() {
        let dir = tempfile::tempdir().unwrap();
        let win = dir.path().join("drive_c/windows");
        std::fs::create_dir_all(win.join("system32")).unwrap();
        std::fs::create_dir_all(win.join("Fonts")).unwrap();
        std::fs::write(win.join("system32/d3d12.dll"), b"MZ...Wine builtin DLL...").unwrap();
        let p = Preflight::of(dir.path());
        assert!(!p.vkd3d && !p.verdana && p.fl == 0);
        std::fs::write(win.join("system32/d3d12.dll"), b"MZ vkd3d-proton").unwrap();
        std::fs::write(win.join("Fonts/Verdana.ttf"), b"font").unwrap();
        let fl = dir
            .path()
            .join("drive_c/Program Files/Image-Line/FL Studio 2026");
        std::fs::create_dir_all(&fl).unwrap();
        std::fs::write(fl.join("FL64.exe"), b"MZ").unwrap();
        let p = Preflight::of(dir.path());
        assert!(p.vkd3d && p.verdana);
        assert_eq!(p.fl, 2026);
        assert!(p.needs_cert_patch());
    }
}
