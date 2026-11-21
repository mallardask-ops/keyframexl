//! Non-Linux stubs used during W0 so the UI, engine and tests compile and pass
//! on Windows before native plugin hosting (W3) and the IPC bridge (W4) land.
//! The Wine-only `fl_link`, `fl_native` and `flp_freeze` stubs remain permanent
//! on Windows, where FL Studio VSTi loads natively as a regular plugin.

#![allow(dead_code, unused_variables)]

pub mod fl_link {
    use std::path::{Path, PathBuf};

    pub const PATH: &str = "fl-link:FL Studio";
    pub const SINK: &str = "kfxl_fl_link";
    pub const DEVICE: &str = "KeyFrameXL - FL link";

    pub fn is_link(path: &Path) -> bool {
        path == Path::new(PATH)
    }

    pub fn ensure_relay() -> Result<(), String> {
        Err("FL Link is only used under Linux/Wine".into())
    }

    pub fn find_fl() -> Option<PathBuf> {
        None
    }

    pub fn open_fl(_project: Option<&Path>) -> Result<(), String> {
        Err("FL Link is only used under Linux/Wine".into())
    }

    pub fn reconnect() -> Result<(), String> {
        Err("FL Link is only used under Linux/Wine".into())
    }

    #[derive(Clone, Debug, Default)]
    pub enum Progress {
        #[default]
        Idle,
        Busy(String),
        Error(String),
    }

    pub fn progress() -> Progress {
        Progress::Idle
    }

    pub fn run(_what: &str, _job: impl FnOnce() -> Result<(), String> + Send + 'static) {}

    #[derive(Clone, Debug, Default)]
    pub struct Preflight {
        pub vkd3d: bool,
        pub verdana: bool,
        pub fl: u32,
        pub wine: Option<String>,
    }

    impl Preflight {
        pub fn needs_cert_patch(&self) -> bool {
            false
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct Stats {
        pub round_trip: u32,
        pub silent_secs: Option<f32>,
        pub underruns: u64,
        pub recent_underruns: u64,
        pub queue: usize,
    }

    impl Stats {
        pub fn round_trip_ms(&self) -> Option<f32> {
            None
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct Status {
        pub fl: bool,
        pub port: bool,
        pub listens: bool,
        pub sink: bool,
        pub stats: Option<Stats>,
        pub preflight: Preflight,
    }

    pub fn status() -> Status {
        Status::default()
    }

    #[derive(Clone, Debug)]
    pub enum LinkState {
        Off,
        Starting(String),
        Connected,
        Degraded(String),
        Error(String),
    }

    pub fn state(_st: &Status, _p: &Progress) -> LinkState {
        LinkState::Off
    }
}

pub mod fl_native {
    use keyframexl::plugin::Shared;
    use keyframexl::sequence::NativeFlProject;
    use std::sync::{Arc, mpsc};

    pub fn path(slot: usize) -> String {
        format!("fl-native:{slot}")
    }

    pub fn slot(path: &str) -> Option<usize> {
        path.strip_prefix("fl-native:")?.parse().ok()
    }

    pub enum Message {
        Ready(Vec<Shared>),
        Error(String),
    }

    pub struct Session {
        pub source: Arc<NativeFlProject>,
        pub rate: u32,
        pub rx: mpsc::Receiver<Message>,
        pub outputs: Vec<Shared>,
        pub error: Option<String>,
    }

    impl Session {
        pub fn start(source: Arc<NativeFlProject>, rate: u32) -> Self {
            let (tx, rx) = mpsc::channel();
            let _ = tx.send(Message::Error(
                "Standalone FL bridge is only used under Linux/Wine".into(),
            ));
            Self {
                source,
                rate,
                rx,
                outputs: Vec::new(),
                error: None,
            }
        }

        pub fn shutdown(self) {}
    }
}

pub mod flp_freeze {
    use std::path::PathBuf;

    pub fn freeze(
        _bytes: &[u8],
        _name: &str,
        _channels: &[(u32, String)],
        _say: &dyn Fn(String),
    ) -> Result<Vec<(u32, PathBuf)>, String> {
        Err("FL freeze via Wine is only available on Linux".into())
    }
}

pub mod plugins {
    use keyframexl::plugin::Shared;
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    pub fn set_wine(_bin: Option<PathBuf>) {}
    pub fn set_dxvk(_on: bool) {}
    pub fn wine_prefix() -> Option<PathBuf> {
        None
    }
    pub fn foreign_wineserver() -> Option<(u32, PathBuf)> {
        None
    }
    pub fn stop_foreign_wineserver(_pid: u32, _exe: &Path) -> Result<(), String> {
        Ok(())
    }
    pub fn has_dxvk(_bin: &Path) -> bool {
        false
    }
    pub fn wine_builds() -> Vec<(String, Option<PathBuf>)> {
        Vec::new()
    }

    pub fn format_of(path: &Path) -> &'static str {
        if super::fl_link::is_link(path) {
            return "fl";
        }
        let s = path.to_string_lossy();
        let file = Path::new(s.split_once('#').map_or(s.as_ref(), |(f, _)| f));
        if file
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("clap"))
        {
            return "clap";
        }
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("so") || x.eq_ignore_ascii_case("dll"))
        {
            "vst2"
        } else {
            "vst3"
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub enum Status {
        Ok,
        Hangs,
        Crashes,
        Failed(String),
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct Scanned {
        pub path: PathBuf,
        pub name: String,
        pub vendor: String,
        pub category: String,
        pub instrument: bool,
        pub gui: bool,
        pub status: Status,
        pub stamp: u64,
        #[serde(default)]
        pub bridged: bool,
    }

    impl Scanned {
        pub fn usable(&self) -> bool {
            self.status == Status::Ok
        }
    }

    pub fn search_dirs(_extra: &[PathBuf]) -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn vst2_dirs() -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn clap_dirs() -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn load_cache() -> Vec<Scanned> {
        Vec::new()
    }
    pub fn rescan_one(path: &Path) -> Scanned {
        Scanned {
            path: path.to_path_buf(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            vendor: String::new(),
            category: String::new(),
            instrument: false,
            gui: false,
            status: Status::Failed("Plugin hosting on Windows is scheduled for W3".into()),
            stamp: 0,
            bridged: false,
        }
    }
    pub fn scan(
        _extra: &[PathBuf],
        _force: bool,
        _progress: impl Fn(usize, usize, &str),
    ) -> Vec<Scanned> {
        Vec::new()
    }
    pub fn probe_main(_path: &Path) -> i32 {
        1
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum Req {
        Load {
            id: u64,
            path: PathBuf,
            state: Vec<u8>,
            effect: bool,
        },
        Unload(u64),
        ShowEditor(u64),
        HideEditor(u64),
        SaveStates,
        Rate(u32),
        Embed {
            id: u64,
            parent: u32,
            rect: [i32; 4],
            clip: [i32; 4],
            regions: Vec<[i32; 4]>,
        },
        Params(u64),
        SetParam {
            id: u64,
            param: u32,
            value: f64,
        },
        SetState {
            id: u64,
            state: Vec<u8>,
        },
        Quit,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ParamRow {
        pub id: u32,
        pub name: String,
        pub value: f64,
        pub text: String,
        pub automatable: bool,
    }

    pub enum Reply {
        Loaded {
            id: u64,
            proc: Shared,
            name: String,
            gui: bool,
            outs: usize,
        },
        Failed {
            id: u64,
            error: String,
        },
        Editor {
            id: u64,
            open: bool,
        },
        States(Vec<(u64, Vec<u8>)>),
        EditorSize {
            id: u64,
            w: i32,
            h: i32,
        },
        Params {
            id: u64,
            rows: Vec<ParamRow>,
        },
        Bye,
    }

    pub struct HostHandle {
        tx: mpsc::Sender<Req>,
        pub rx: mpsc::Receiver<Reply>,
    }

    impl HostHandle {
        pub fn new(tx: mpsc::Sender<Req>, rx: mpsc::Receiver<Reply>) -> Self {
            Self { tx, rx }
        }
        pub fn send(&self, r: Req) {
            let _ = self.tx.send(r);
        }
        pub fn sender(&self) -> mpsc::Sender<Req> {
            self.tx.clone()
        }
    }

    pub fn start_host(_rate: u32, repaint: Option<eframe::egui::Context>) -> HostHandle {
        let (tx, rx) = mpsc::channel::<Req>();
        let (rtx, rrx) = mpsc::channel::<Reply>();
        std::thread::Builder::new()
            .name("plugin-host-stub".into())
            .spawn(move || {
                while let Ok(req) = rx.recv() {
                    match req {
                        Req::Load { id, .. } => {
                            let _ = rtx.send(Reply::Failed {
                                id,
                                error: "Plugin hosting on Windows is scheduled for W3".into(),
                            });
                        }
                        Req::SaveStates => {
                            let _ = rtx.send(Reply::States(Vec::new()));
                        }
                        Req::Params(id) => {
                            let _ = rtx.send(Reply::Params {
                                id,
                                rows: Vec::new(),
                            });
                        }
                        Req::Quit => {
                            let _ = rtx.send(Reply::Bye);
                            break;
                        }
                        _ => {}
                    }
                    if let Some(ctx) = &repaint {
                        ctx.request_repaint();
                    }
                }
            })
            .expect("plugin host stub thread");
        HostHandle { tx, rx: rrx }
    }

    pub fn pointer_in(_win: u32) -> Option<(i32, i32)> {
        None
    }
}

pub mod bridge {
    use super::plugins::{self, HostHandle};
    use std::io;

    pub fn start_bridged_host(rate: u32, repaint: Option<eframe::egui::Context>) -> HostHandle {
        plugins::start_host(rate, repaint)
    }

    pub fn child_main() -> io::Result<()> {
        Err(io::Error::other(
            "Out-of-process plugin bridge on Windows is scheduled for W4",
        ))
    }
}
