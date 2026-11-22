//! Control side of one bridged instance: starts the child, hands it the shared
//! block, and watches it. Never called from the audio callback; the proxy only
//! reads the `crashed` flag this sets.

use super::ipc::{ChildGuard, ControlListener, ControlStream, Endpoint};
use super::protocol::{Message, VERSION, read_message, write_message};
use super::shm::Shm;
use std::{
    io,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How long a published block may stay unanswered before the child counts as
/// hung. Plugins under load (a sampler streaming a 2 GB library, several
/// editors opening) can stall a few seconds and recover; silence is better
/// than closing them.
pub(crate) const HANG_AFTER: Duration = Duration::from_secs(5);
/// Generous limit while an offline export runs (e.g. drum samplers streaming
/// synchronously from disk or switching `ProcessMode::Offline`).
pub(crate) const OFFLINE_HANG_AFTER: Duration = Duration::from_secs(60);

/// Why an instance stopped.
#[derive(Clone, Debug, PartialEq)]
pub enum Fault {
    /// The child exited (crash, abort, a dying Wine host taking it along).
    Exited(Option<i32>),
    /// The child stopped answering and was killed.
    Hung,
}

pub(crate) struct Supervisor {
    pub shm: Arc<Shm>,
    pub crashed: Arc<AtomicBool>,
    pub fault: Arc<Mutex<Option<Fault>>>,
    socket: Mutex<ControlStream>,
    stop: Arc<AtomicBool>,
    monitor: Option<std::thread::JoinHandle<()>>,
    _endpoint: Endpoint,
}

impl Supervisor {
    /// Start a child with `command` (the program's own `plugin-host`, or a test
    /// harness), shake hands and load `mode`.
    pub fn start(
        mut command: Command,
        generation: u64,
        rate: u32,
        max_frames: u32,
        max_events: u32,
        mode: &str,
    ) -> io::Result<Self> {
        let endpoint = Endpoint::new(generation)?;
        let control_addr = endpoint.control_addr();
        let shm_name = endpoint.shm_name();
        let shm = Arc::new(Shm::create(&shm_name, max_frames, max_events)?);
        let mut listener = ControlListener::bind(&control_addr)?;
        crate::platform::hide_console(&mut command);
        let mut child = command
            .env("KFXL_BRIDGE_SOCKET", &control_addr)
            .stdin(Stdio::null())
            .spawn()?;
        let guard = ChildGuard::attach(&child);
        let result = (|| {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                if let Some(socket) = listener.accept_step(Duration::from_millis(5))? {
                    break socket;
                }
                if child.try_wait()?.is_some() {
                    return Err(io::Error::other("plugin host exited during startup"));
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "plugin host did not start",
                    ));
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(30)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            write_message(
                &mut socket,
                &Message::Hello {
                    version: VERSION,
                    generation,
                    rate,
                    max_frames,
                    max_events,
                },
            )?;
            match read_message(&mut socket)? {
                Message::Ready {
                    version: VERSION,
                    generation: g,
                } if g == generation => {}
                Message::Error { reason } => return Err(io::Error::other(reason)),
                _ => return Err(io::Error::other("plugin host handshake rejected")),
            }
            write_message(
                &mut socket,
                &Message::Load {
                    mode: mode.into(),
                    shm: shm_name,
                },
            )?;
            match read_message(&mut socket)? {
                Message::Loaded => Ok(socket),
                Message::Error { reason } => Err(io::Error::other(reason)),
                _ => Err(io::Error::other("plugin host load rejected")),
            }
        })();
        let socket = match result {
            Ok(s) => s,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let crashed = Arc::new(AtomicBool::new(false));
        let fault = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let monitor = {
            let (crashed, fault, stop, shm) =
                (crashed.clone(), fault.clone(), stop.clone(), shm.clone());
            std::thread::Builder::new()
                .name("plugin-bridge-watch".into())
                .spawn(move || watch(child, guard, shm, crashed, fault, stop))?
        };
        Ok(Self {
            shm,
            crashed,
            fault,
            socket: Mutex::new(socket),
            stop,
            monitor: Some(monitor),
            _endpoint: endpoint,
        })
    }
}

/// Reap the child when it exits; kill it when a block stays unanswered too long.
fn watch(
    mut child: Child,
    _guard: ChildGuard,
    shm: Arc<Shm>,
    crashed: Arc<AtomicBool>,
    fault: Arc<Mutex<Option<Fault>>>,
    stop: Arc<AtomicBool>,
) {
    let mut waiting_since: Option<(u32, Instant)> = None;
    loop {
        if stop.load(Ordering::Acquire) {
            // Shutdown was sent: give the plugin time to unload cleanly, then insist.
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        if let Ok(Some(status)) = child.try_wait() {
            *fault.lock().unwrap_or_else(|e| e.into_inner()) = Some(Fault::Exited(status.code()));
            crashed.store(true, Ordering::Release);
            return;
        }
        let h = shm.header();
        let (i, o) = (
            h.in_seq.load(Ordering::Acquire),
            h.out_seq.load(Ordering::Acquire),
        );
        waiting_since = if i == o {
            None
        } else {
            let limit = if h.offline.load(Ordering::Acquire) != 0 {
                OFFLINE_HANG_AFTER
            } else {
                HANG_AFTER
            };
            match waiting_since {
                Some((seq, since)) if seq == i => {
                    if since.elapsed() >= limit {
                        *fault.lock().unwrap_or_else(|e| e.into_inner()) = Some(Fault::Hung);
                        crashed.store(true, Ordering::Release);
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                    Some((seq, since))
                }
                _ => Some((i, Instant::now())),
            }
        };
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Supervisor {
    /// A control message to the child (control threads only).
    pub fn send(&self, m: &Message) -> io::Result<()> {
        let mut s = self
            .socket
            .lock()
            .map_err(|_| io::Error::other("bridge socket poisoned"))?;
        write_message(&mut *s, m)
    }

    /// A second handle on the control channel for a reader thread (blocking reads).
    pub fn reader(&self) -> io::Result<ControlStream> {
        let mut s = self
            .socket
            .lock()
            .map_err(|_| io::Error::other("bridge socket poisoned"))?
            .try_clone()?;
        s.set_read_timeout(None)?;
        Ok(s)
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Ok(mut s) = self.socket.lock() {
            let _ = write_message(&mut *s, &Message::Shutdown);
        }
        self.stop.store(true, Ordering::Release);
        if let Some(m) = self.monitor.take() {
            let _ = m.join();
        }
    }
}
