//! Non-Linux stubs for the permanent Wine-only FL Studio helpers (`fl_link`,
//! `fl_native`, and `flp_freeze`). On Windows FL Studio VSTi loads natively as
//! a regular plugin.

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
