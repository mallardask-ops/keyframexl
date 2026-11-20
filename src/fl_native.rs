//! One standalone FL Studio for the native generators of an imported project.

use crate::fl_link;
use crate::flp_freeze::{Display, click, windows};
use keyframexl::plugin::{FL_PITCH, PluginEvent, Processor, Shared, Transport, share};
use keyframexl::sequence::NativeFlProject;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const MIDI_CLIENT: &str = "KeyFrameXL";
const SCRIPT: &str = "KeyFrameXL FLEX bridge";
const QUANTUM: usize = 1024;
const MAX_QUEUE: usize = QUANTUM * 2;
const PITCH_LSB_CC: u8 = 118;
const PITCH_MSB_CC: u8 = 119;

pub fn path(slot: usize) -> String {
    format!("fl-native:{slot}")
}

pub fn slot(path: &str) -> Option<usize> {
    path.strip_prefix("fl-native:")?.parse().ok()
}

pub struct Session {
    pub source: Arc<NativeFlProject>,
    pub rate: u32,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub rx: mpsc::Receiver<Message>,
    pub outputs: Vec<Shared>,
    pub error: Option<String>,
}

pub enum Message {
    Ready(Vec<Shared>),
    Error(String),
}

impl Session {
    pub fn start(source: Arc<NativeFlProject>, rate: u32) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let quit = stop.clone();
        let project = source.clone();
        let worker = std::thread::spawn(move || {
            if let Err(e) = run(&project, rate, &quit, &tx) {
                let _ = tx.send(Message::Error(e));
            }
        });
        Self {
            source,
            rate,
            stop,
            worker: Some(worker),
            rx,
            outputs: Vec::new(),
            error: None,
        }
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        let Some(worker) = self.worker.take() else {
            return;
        };
        let until = Instant::now() + Duration::from_secs(3);
        while !worker.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        if worker.is_finished() {
            let _ = worker.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

struct Lane {
    slot: u8,
    audio: rtrb::Consumer<[f32; 2]>,
    midi: rtrb::Producer<Midi>,
    tempo: u32,
    rate: u32,
    alive: Arc<AtomicBool>,
    underrun: bool,
}

type Midi = ([u8; 3], Instant);

impl Processor for Lane {
    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], transport: &Transport) {
        let start = Instant::now();
        if self.slot == 0 {
            let tempo = (transport.tempo * 10.0).round().clamp(100.0, 9990.0) as u32;
            if tempo != self.tempo
                && self
                    .midi
                    .push(([0xb0, (tempo >> 7) as u8, (tempo & 127) as u8], start))
                    .is_ok()
            {
                self.tempo = tempo;
            }
        }
        for event in events {
            let (at, message) = match *event {
                PluginEvent::NoteOn {
                    at,
                    pitch,
                    velocity,
                } => (at, [0x90 | self.slot, pitch, velocity]),
                PluginEvent::NoteOff { at, pitch } => (at, [0x80 | self.slot, pitch, 0]),
                PluginEvent::Midi {
                    at,
                    bytes: [FL_PITCH, lsb, msb],
                } => (at, [0xe0 | self.slot, lsb, msb]),
                PluginEvent::Midi {
                    at,
                    bytes: [status, 120 | 123, _],
                } if status & 0xf0 == 0xb0 => (at, [0xb0 | self.slot, 127, 0]),
                _ => continue,
            };
            let due = start + Duration::from_secs_f64(at as f64 / self.rate as f64);
            let _ = self.midi.push((message, due));
        }

        let frames = io.len() / 2;
        if !self.underrun && keyframexl::plugin::OFFLINE.load(Ordering::Acquire) {
            let deadline = Instant::now() + Duration::from_secs(1);
            while self.audio.slots() < frames && self.alive.load(Ordering::Acquire) {
                if Instant::now() >= deadline {
                    self.underrun = true;
                    break;
                }
                std::thread::sleep(Duration::from_micros(500));
            }
        }
        let queued = self.audio.slots();
        if queued > frames + MAX_QUEUE
            && let Ok(skip) = self.audio.read_chunk(queued - frames - QUANTUM)
        {
            skip.commit_all();
        }
        for frame in io.chunks_exact_mut(2) {
            frame.copy_from_slice(&self.audio.pop().unwrap_or([0.0; 2]));
        }
    }

    fn latency(&self, _: usize) -> usize {
        MAX_QUEUE + QUANTUM
    }

    fn needs_realtime(&self) -> bool {
        true
    }

    fn available(&self) -> bool {
        self.alive.load(Ordering::Acquire) && !self.underrun
    }

    fn prepare_offline(&mut self) {
        self.underrun = false;
    }
}

struct WineProcess {
    child: Child,
    wine: PathBuf,
    prefix: PathBuf,
}

impl Drop for WineProcess {
    fn drop(&mut self) {
        let _ = Command::new(self.wine.join("wineserver"))
            .env("WINEPREFIX", &self.prefix)
            .arg("-k")
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn win_path(path: &Path) -> String {
    format!("Z:{}", path.to_string_lossy().replace('/', "\\"))
}

fn driver(data: &Path) -> Result<PathBuf, String> {
    let mut paths = vec![
        data.join("pipeasio/lib/wine"),
        PathBuf::from("/usr/lib/wine"),
    ];
    if let Some(path) = std::env::var_os("KFXL_PIPEASIO_WINE") {
        paths.insert(0, path.into());
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/lib/wine"));
    }
    paths.into_iter().find(|p| {
        p.join("x86_64-windows/pipeasio64.dll").is_file() && p.join("x86_64-unix/pipeasio64.so").is_file()
    }).ok_or_else(|| "FL bridge needs PipeASIO. Run scripts/setup-fl-pipeasio.sh or set KFXL_PIPEASIO_WINE to its Wine library directory.".into())
}

fn private_documents(prefix: &Path) -> Result<PathBuf, String> {
    let user = std::env::var_os("USER").ok_or("Cannot find the local account for the FL bridge")?;
    let rel = PathBuf::from("drive_c/users").join(user).join("Documents");
    let docs = prefix.join(&rel);
    if docs.is_symlink() {
        std::fs::remove_file(&docs).map_err(|e| e.to_string())?;
    }
    let shared = docs.join("Image-Line/FLEX");
    std::fs::create_dir_all(&shared).map_err(|e| e.to_string())?;
    let packs = fl_link::prefix().join(rel).join("Image-Line/FLEX/Packs");
    let link = shared.join("Packs");
    if packs.is_dir() && std::fs::symlink_metadata(&link).is_err() {
        std::os::unix::fs::symlink(packs, link).map_err(|e| e.to_string())?;
    }
    Ok(docs)
}

fn install_script(docs: &Path, channels: &[u32], port: &str) -> Result<(), String> {
    let folder = docs.join("Image-Line/FL Studio/Settings/Hardware/KeyFrameXL");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let script = format!(
        "# name={SCRIPT}\n# supportedDevices={MIDI_CLIENT} - {port}\nSLOTS = {channels:?}\n{}",
        include_str!("../assets/fl-native-midi.py")
    );
    std::fs::write(folder.join("device_KeyFrameXL.py"), script).map_err(|e| e.to_string())
}

fn configure(
    bin: &Path,
    prefix: &Path,
    exe: &Path,
    dll: &Path,
    work: &Path,
    display: &Display,
    rate: u32,
    midi_port: &str,
) -> Result<(), String> {
    let cmd = || {
        let mut c = fl_link::wine_cmd(bin, prefix);
        c.env("DISPLAY", display.name())
            .env_remove("WAYLAND_DISPLAY")
            .env("WINEDEBUG", "-all")
            .env("WINEDLLPATH", dll)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        c
    };
    let pe = dll.join("x86_64-windows/pipeasio64.dll");
    std::fs::copy(pe, prefix.join("drive_c/windows/system32/pipeasio64.dll"))
        .map_err(|e| format!("Cannot stage the PipeASIO driver: {e}"))?;
    if !cmd()
        .args(["regsvr32", "/s", "pipeasio64.dll"])
        .status()
        .is_ok_and(|s| s.success())
    {
        return Err("Wine could not register PipeASIO for the FL bridge".into());
    }
    let version = fl_link::registry_key(exe);
    let base = format!("Software\\Image-Line\\{version}\\Devices");
    let midi_key = format!("{base}\\MIDI input\\");
    let port = format!("{MIDI_CLIENT} - {midi_port}");
    let mut reg = format!(
        "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\{base}\\Audio output]\r\n\"Device name\"=\"1PipeASIO\"\r\n\"Sample rate\"=\"{rate}\"\r\n\r\n"
    );
    let current = std::fs::read_to_string(prefix.join("user.reg")).unwrap_or_default();
    let escaped = midi_key.replace('\\', "\\\\");
    for line in current.lines() {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|s| s.strip_prefix(&escaped))
            .and_then(|s| s.split(']').next())
            && name != port
        {
            reg.push_str(&format!(
                "[HKEY_CURRENT_USER\\{midi_key}{name}]\r\n\"Enabled\"=\"0\"\r\n\r\n"
            ));
        }
    }
    reg.push_str(&format!(
        "[HKEY_CURRENT_USER\\{midi_key}{port}]\r\n\"Enabled\"=\"1\"\r\n\"Type\"=\"-3\"\r\n\"Port\"=\"-1\"\r\n\"ScriptFolder\"=\"{SCRIPT}\"\r\n\r\n"
    ));
    let regfile = work.join("bridge.reg");
    let content: Vec<u8> = std::iter::once(0xfeffu16)
        .chain(reg.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(&regfile, content).map_err(|e| e.to_string())?;
    if !cmd()
        .args(["regedit", "/S", &win_path(&regfile)])
        .status()
        .is_ok_and(|s| s.success())
    {
        return Err("Wine could not configure the FL bridge's audio and MIDI".into());
    }
    Ok(())
}

fn run(
    project: &NativeFlProject,
    rate: u32,
    stop: &AtomicBool,
    tx: &mpsc::Sender<Message>,
) -> Result<(), String> {
    use midir::os::unix::VirtualOutput;
    let count = project.channels.len();
    if !(1..=16).contains(&count) {
        return Err("Invalid FL bridge channel count".into());
    }
    let bin = fl_link::system_wine().ok_or("The FL bridge needs system Wine")?;
    let data = dirs::data_dir()
        .ok_or("Cannot locate the KeyFrameXL data folder")?
        .join("keyframexl");
    let dll = driver(&data)?;
    let prefix = data.join("fl-native-prefix");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(data.join("fl-native.lock"))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while lock.try_lock().is_err() {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Another KeyFrameXL instance holds the FL bridge".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    fl_link::prepare_fl_prefix(&bin, &prefix)?;
    let exe = fl_link::find_fl_in(&prefix).ok_or("FL Studio was not found in the Wine prefix")?;
    let docs = private_documents(&prefix)?;
    let work = tempfile::tempdir_in(&data).map_err(|e| e.to_string())?;
    let midi_port = format!("FL {:x}", std::process::id());
    install_script(&docs, &project.channels, &midi_port)?;
    let flp = work.path().join("KeyFrameXL FLEX.flp");
    std::fs::write(&flp, project.bytes()?).map_err(|e| e.to_string())?;
    let config = work.path().join("pipeasio");
    std::fs::create_dir(&config).map_err(|e| e.to_string())?;
    let node = format!("kfxl-flex-{}", std::process::id());
    std::fs::write(
        config.join("config.ini"),
        format!("[pipeasio]\nnode_name={node}\nbuffer_size={QUANTUM}\nfixed_buffer_size=1\n"),
    )
    .map_err(|e| e.to_string())?;
    let display = Display::start()?;
    let mut midi = midir::MidiOutput::new(MIDI_CLIENT)
        .map_err(|e| e.to_string())?
        .create_virtual(&midi_port)
        .map_err(|e| e.to_string())?;
    configure(
        &bin,
        &prefix,
        &exe,
        &dll,
        work.path(),
        &display,
        rate,
        &midi_port,
    )?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let log = std::fs::File::create(data.join("fl-native.log")).map_err(|e| e.to_string())?;
    let child = fl_link::wine_cmd(&bin, &prefix)
        .arg(&exe)
        .arg(win_path(&flp))
        .current_dir(exe.parent().unwrap_or(Path::new("/")))
        .env("DISPLAY", display.name())
        .env_remove("WAYLAND_DISPLAY")
        .env("WINEDEBUG", "-all")
        .env("WINEDLLPATH", dll)
        .env("XDG_CONFIG_HOME", work.path())
        .env("PIPEASIO_NUMBER_INPUTS", "0")
        .env("PIPEASIO_NUMBER_OUTPUTS", (2 * (count + 1)).to_string())
        .env("PIPEASIO_CONNECT_TO_HARDWARE", "off")
        .env("PIPEASIO_SAMPLE_RATE", rate.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .map_err(|e| format!("Cannot start FL Studio: {e}"))?;
    let mut fl = WineProcess {
        child,
        prefix,
        wine: bin,
    };
    let capture_node = format!("{node}-input");
    let map = (0..count * 2)
        .map(|i| format!("AUX{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut capture = ChildGuard(
        Command::new("pw-cat")
            .args([
                "-r",
                "-a",
                "--target",
                "0",
                "--format",
                "f32",
                "--latency",
                &QUANTUM.to_string(),
                "--rate",
                &rate.to_string(),
                "--channels",
                &(count * 2).to_string(),
                "--channel-map",
                &map,
                "-P",
                &format!("node.name={capture_node}"),
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot start PipeWire capture (pw-cat): {e}"))?,
    );
    let mut lanes = Vec::new();
    let mut writers = Vec::new();
    let mut receivers = Vec::new();
    let alive = Arc::new(AtomicBool::new(true));
    for slot in 0..count {
        let (audio_tx, audio_rx) = rtrb::RingBuffer::new(8192);
        let (midi_tx, midi_rx) = rtrb::RingBuffer::new(512);
        lanes.push(share(Lane {
            slot: slot as u8,
            audio: audio_rx,
            midi: midi_tx,
            tempo: 0,
            rate,
            alive: alive.clone(),
            underrun: false,
        }));
        writers.push(audio_tx);
        receivers.push(midi_rx);
    }
    let mut input = capture
        .0
        .stdout
        .take()
        .ok_or("PipeWire capture did not open")?;
    let frame_size = count * 8;
    let mut block = vec![0u8; frame_size * 128];
    let mut pending = vec![None::<Midi>; count];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while input.read_exact(&mut block).is_ok() {
                for frame in block.chunks_exact(frame_size) {
                    for (pair, w) in frame.chunks_exact(8).zip(&mut writers) {
                        let _ = w.push([
                            f32::from_le_bytes(pair[..4].try_into().unwrap()),
                            f32::from_le_bytes(pair[4..].try_into().unwrap()),
                        ]);
                    }
                }
            }
            alive.store(false, Ordering::Release);
        });
        // Drain pw-cat before linking FL's outputs. Its stdout must keep
        // flowing while FL opens the project and the host handles dialogs.
        let connected = connect(
            &mut fl,
            &display,
            &node,
            &capture_node,
            count,
            stop,
            &midi_port,
        );
        if !matches!(connected, Ok(true)) {
            let _ = capture.0.kill();
            return connected.map(|_| ());
        }
        let _ = tx.send(Message::Ready(lanes));
        let outcome = loop {
            if stop.load(Ordering::Acquire) {
                break Ok(());
            }
            if !alive.load(Ordering::Acquire) || fl.child.try_wait().ok().flatten().is_some() {
                break Err("The FL bridge stopped. Reopen the project to reconnect.".into());
            }
            let now = Instant::now();
            for (receiver, event) in receivers.iter_mut().zip(&mut pending) {
                while let Some((message, due)) = event.take().or_else(|| receiver.pop().ok()) {
                    if due > now {
                        *event = Some((message, due));
                        break;
                    }
                    if message[0] & 0xf0 == 0xe0 {
                        let slot = message[0] & 0x0f;
                        let _ = midi.send(&[0xb0 | slot, PITCH_LSB_CC, message[1]]);
                        let _ = midi.send(&[0xb0 | slot, PITCH_MSB_CC, message[2]]);
                    } else {
                        let _ = midi.send(&message);
                    }
                }
            }
            std::thread::sleep(Duration::from_micros(600));
        };
        alive.store(false, Ordering::Release);
        let _ = capture.0.kill();
        outcome
    })
}

fn ports(flag: &str) -> String {
    Command::new("pw-link")
        .arg(flag)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn midi_connected(port: &str) -> bool {
    let Ok(clients) = std::fs::read_to_string("/proc/asound/seq/clients") else {
        return false;
    };
    let name = format!("\"{port}\"");
    clients.lines().collect::<Vec<_>>().windows(2).any(|lines| {
        lines[0].contains(&name) && lines[1].trim_start().starts_with("Connecting To:")
    })
}

fn connect(
    fl: &mut WineProcess,
    display: &Display,
    node: &str,
    capture: &str,
    count: usize,
    stop: &AtomicBool,
    midi_port: &str,
) -> Result<bool, String> {
    let (x, screen) = xcb::Connection::connect_with_extensions(
        Some(&display.name()),
        &[xcb::Extension::Test],
        &[],
    )
    .map_err(|e| format!("Cannot reach FL Studio's display: {e}"))?;
    let root = x
        .get_setup()
        .roots()
        .nth(screen as usize)
        .ok_or("FL Studio has no display")?
        .root();
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = None;
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        if fl.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("FL Studio exited while loading the bridged instruments".into());
        }
        let mut blocked = false;
        let mut project_open = false;
        for (name, x0, y, w, h) in windows(&x, root) {
            if name.starts_with("KeyFrameXL FLEX.flp - FL Studio") {
                project_open = true;
            } else if name.starts_with("Problems loading") {
                click(&x, root, x0 + 369, y + h as i16 - 28);
                blocked = true;
            } else if name == "Error" {
                click(&x, root, x0 + w as i16 / 2 - 10, y + h as i16 - 22);
                blocked = true;
            } else if name == "Warning" {
                return Err(
                    "FL Studio reported a warning while loading the bridged instruments. See fl-native.log."
                        .into(),
                );
            }
        }
        if !blocked
            && project_open
            && midi_connected(midi_port)
            && ports("-o")
                .lines()
                .any(|s| s == format!("{node}:out_{}", count * 2 + 2))
            && ports("-i")
                .lines()
                .any(|s| s == format!("{capture}:input_AUX{}", count * 2 - 1))
        {
            // Wait for FL to reopen its audio device with the loaded project.
            if seen.get_or_insert_with(Instant::now).elapsed() >= Duration::from_secs(3) {
                for i in 0..count * 2 {
                    let out = format!("{node}:out_{}", i + 3);
                    let into = format!("{capture}:input_AUX{i}");
                    if !Command::new("pw-link")
                        .args([&out, &into])
                        .status()
                        .is_ok_and(|s| s.success())
                    {
                        return Err(format!("Cannot link FL Studio audio output {}", i + 3));
                    }
                }
                return Ok(true);
            }
        } else {
            seen = None;
        }
        if Instant::now() > deadline {
            return Err("FL Studio did not finish connecting its MIDI script and audio outputs. See fl-native.log in the KeyFrameXL data folder.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Test the same FL process, script and PipeWire lanes used by the editor.
pub fn test(path: &Path, channels: &[u32]) -> Result<(), String> {
    let source = std::fs::read(path).map_err(|e| e.to_string())?;
    let (flp, midi_channels) = keyframexl::flp::native_host_flp(&source, channels)?;
    let project = Arc::new(NativeFlProject::new(&flp, midi_channels));
    let rate = 48_000;
    let session = Session::start(project, rate);
    let lanes = match session
        .rx
        .recv_timeout(Duration::from_secs(240))
        .map_err(|e| e.to_string())?
    {
        Message::Ready(lanes) => lanes,
        Message::Error(e) => return Err(e),
    };
    let transport = Transport {
        rate,
        tempo: 94.661,
        playing: false,
        frame: 0,
    };
    let frames = 256;
    let mut io = vec![0.0; 2 * frames];
    let start = Instant::now();
    let mut clock = 0;
    let mut exercise = |slots: &[usize]| {
        let mut peaks = vec![0.0f32; lanes.len()];
        let mut idle = vec![0.0f32; lanes.len()];
        for block in 0..570 {
            let due = start + Duration::from_secs_f64(clock as f64 / f64::from(rate));
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            clock += frames;
            for (i, lane) in lanes.iter().enumerate() {
                let on = PluginEvent::NoteOn {
                    at: 0,
                    pitch: 67,
                    velocity: 110,
                };
                let off = PluginEvent::NoteOff { at: 0, pitch: 67 };
                let event = match (slots.contains(&i), block) {
                    (true, 48) => std::slice::from_ref(&on),
                    (true, 125) => std::slice::from_ref(&off),
                    _ => &[],
                };
                lane.lock().unwrap().process(event, &mut io, &transport);
                if block < 48 {
                    idle[i] = io.iter().fold(idle[i], |p, x| p.max(x.abs()));
                } else if block < 180 {
                    peaks[i] = io.iter().fold(peaks[i], |p, x| p.max(x.abs()));
                }
            }
        }
        println!("Active {slots:?}, peaks {peaks:?}");
        (peaks, idle)
    };
    let mut failure = None;
    for i in 0..channels.len() {
        let (peaks, idle) = exercise(&[i]);
        // A previous preset's reverb can still be decaying in its own lane.
        if peaks[i] < 0.001
            || peaks
                .iter()
                .enumerate()
                .any(|(k, v)| k != i && *v > 0.002 + idle[k] * 2.0)
        {
            failure = Some("An FL Studio channel was silent or leaked to another output");
        }
    }
    let (peaks, _) = exercise(&(0..channels.len()).collect::<Vec<_>>());
    if peaks.iter().any(|p| *p < 0.001) {
        failure = Some("An FL Studio channel was silent during simultaneous playback");
    }
    session.shutdown();
    failure.map_or(Ok(()), |e| Err(e.into()))
}
