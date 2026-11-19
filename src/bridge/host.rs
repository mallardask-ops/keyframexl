//! Real plugins across the bridge. The child runs the same plugin host thread
//! as the in-process mode (`plugins::start_host`), fed by requests from the
//! socket; the parent's router looks to the UI exactly like that host thread
//! (`HostHandle`), with one child per plugin instance.

use super::protocol::{Message, WireReply, read_message, write_message};
use super::shm::{Shm, futex_wait};
use super::{BridgeProc, Fault};
use crate::plugins::{HostHandle, Reply, Req};
use keyframexl::plugin::{MAX_EVENTS, PluginEvent, Shared, Transport, share};
use std::{
    collections::HashMap,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// The largest block the engine sends (Settings → Audio → buffer tops out at 8192).
pub(crate) const MAX_FRAMES: u32 = 8192;

// ------------------------------------------------------------------- child

/// The child's life in `host` mode: plugin host thread + socket + audio loop.
pub(crate) fn child_host(socket: UnixStream, shm: Shm, rate: u32) -> ! {
    let host = crate::plugins::start_host(rate, None);
    let proc: Arc<Mutex<Option<Shared>>> = Arc::new(Mutex::new(None));
    let done = Arc::new(AtomicBool::new(false));
    // Socket → host thread. The parent closing the socket means: unload and leave.
    {
        let mut rd = socket.try_clone().expect("socket clone");
        let tx = host.sender();
        let done = done.clone();
        std::thread::spawn(move || {
            loop {
                match read_message(&mut rd) {
                    Ok(Message::Req(r)) => {
                        let _ = tx.send(r);
                    }
                    Ok(Message::Shutdown) | Err(_) => {
                        let _ = tx.send(Req::Quit);
                        break;
                    }
                    Ok(_) => {}
                }
            }
            done.store(true, Ordering::Release);
        });
    }
    // Host thread → socket; the processor stays here for the audio loop.
    {
        let mut wr = socket;
        let proc = proc.clone();
        std::thread::spawn(move || {
            while let Ok(r) = host.rx.recv() {
                let wire = match r {
                    Reply::Loaded {
                        id,
                        proc: p,
                        name,
                        gui,
                        outs,
                    } => {
                        *proc.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
                        WireReply::Loaded {
                            id,
                            name,
                            gui,
                            outs,
                        }
                    }
                    Reply::Failed { id, error } => WireReply::Failed { id, error },
                    Reply::Editor { id, open } => WireReply::Editor { id, open },
                    Reply::States(s) => WireReply::States(s),
                    Reply::EditorSize { id, w, h } => WireReply::EditorSize { id, w, h },
                    Reply::Params { id, rows } => WireReply::Params { id, rows },
                    Reply::Bye => {
                        let _ = write_message(&mut wr, &Message::Reply(WireReply::Bye));
                        // Plugins are unloaded: leave without the C library's exit
                        // handlers (yabridge aborts when its statics are torn down).
                        // SAFETY: nothing is left to run in this process.
                        unsafe { libc::_exit(0) };
                    }
                };
                if write_message(&mut wr, &Message::Reply(wire)).is_err() {
                    break;
                }
            }
        });
    }
    audio_loop(&shm, &proc, &done, rate)
}

fn audio_loop(shm: &Shm, proc: &Mutex<Option<Shared>>, done: &AtomicBool, rate: u32) -> ! {
    let h = shm.header();
    // What it last finished, not what is published: a block published before
    // this loop started must still be processed.
    let mut seen = h.out_seq.load(Ordering::Acquire);
    let mut events: Vec<PluginEvent> = Vec::with_capacity(MAX_EVENTS);
    loop {
        let next = h.in_seq.load(Ordering::Acquire);
        if next == seen {
            if done.load(Ordering::Acquire) {
                // The host thread is unloading; it ends the process with Bye.
                std::thread::sleep(Duration::from_secs(5));
                // SAFETY: as above, a last resort if unloading hangs.
                unsafe { libc::_exit(0) };
            }
            futex_wait(&h.in_seq, seen, 100);
            continue;
        }
        let frames = (h.frames.load(Ordering::Relaxed) as usize).min(shm.max_frames());
        let n = (h.n_events.load(Ordering::Relaxed) as usize).min(shm.max_events());
        // SAFETY: published by the parent (Release / Acquire); ours until out_seq.
        let (input, output, raw) = unsafe {
            (
                &shm.input()[..frames * 2],
                &mut shm.output()[..frames * 2],
                &shm.events()[..n],
            )
        };
        output.copy_from_slice(input);
        events.clear();
        events.extend(raw.iter().map(|e| match e.kind {
            0 => PluginEvent::NoteOn {
                at: e.at,
                pitch: e.b[0],
                velocity: e.b[1],
            },
            1 => PluginEvent::NoteOff {
                at: e.at,
                pitch: e.b[0],
            },
            _ => PluginEvent::Midi {
                at: e.at,
                bytes: e.b,
            },
        }));
        let t = Transport {
            rate: match h.rate.load(Ordering::Relaxed) {
                0 => rate,
                r => r,
            },
            tempo: super::shm::tempo_of(h),
            playing: h.playing.load(Ordering::Relaxed) != 0,
            frame: h.frame.load(Ordering::Relaxed),
        };
        let p = proc.lock().ok().and_then(|g| g.clone());
        let mut pairs = 0;
        let mut lag = 0;
        match p {
            Some(p) => {
                if let Ok(mut p) = p.lock() {
                    lag = p.latency(frames);
                    if h.keyed.load(Ordering::Relaxed) != 0 {
                        // SAFETY: ours until out_seq, like `input`.
                        let key = unsafe { &shm.key()[..frames * 2] };
                        p.process_keyed(&events, output, key, &t);
                    } else if p.extra_pairs() == 0 {
                        p.process(&events, output, &t);
                    } else {
                        let stride = shm.max_frames() * 2;
                        // SAFETY: ours until out_seq, like `output`.
                        let extra = unsafe { shm.extra() };
                        pairs = p.process_multi(&events, output, extra, stride, &t);
                    }
                }
            }
            None => output.fill(0.0),
        }
        h.pairs.store(pairs as u32, Ordering::Relaxed);
        h.latency.store(lag as u32, Ordering::Relaxed);
        seen = next;
        h.out_seq.store(next, Ordering::Release);
    }
}

// ------------------------------------------------------------------ parent

struct Child {
    proc: Option<BridgeProc>,
    sup: Arc<super::supervisor::Supervisor>,
    crashed: Arc<AtomicBool>,
    reported: bool,
}

/// Like `plugins::start_host`, with every plugin in its own process.
pub fn start_bridged_host(rate: u32, repaint: Option<eframe::egui::Context>) -> HostHandle {
    let (tx, rx) = mpsc::channel::<Req>();
    let (rtx, rrx) = mpsc::channel::<Reply>();
    std::thread::Builder::new()
        .name("plugin-bridge".into())
        .spawn(move || router(rate, rx, rtx, repaint))
        .expect("plugin bridge thread");
    HostHandle::new(tx, rrx)
}

fn router(
    mut rate: u32,
    rx: mpsc::Receiver<Req>,
    rtx: mpsc::Sender<Reply>,
    repaint: Option<eframe::egui::Context>,
) {
    let (wtx, wrx) = mpsc::channel::<(u64, WireReply)>();
    let mut children: HashMap<u64, Child> = HashMap::new();
    let poke = || {
        if let Some(c) = &repaint {
            c.request_repaint();
        }
    };
    loop {
        // Child answers first: they turn into the host thread's replies.
        while let Ok((id, w)) = wrx.try_recv() {
            forward(id, w, &mut children, &rtx);
            poke();
        }
        // A child that died or hung: the UI hears it once, the plugin stays silent.
        for (id, c) in children.iter_mut() {
            if !c.reported && c.crashed.load(Ordering::Acquire) {
                c.reported = true;
                let why = match c
                    .proc
                    .as_ref()
                    .and_then(|p| p.fault())
                    .or_else(|| c.sup.fault.lock().ok().and_then(|f| f.clone()))
                {
                    Some(Fault::Hung) => "stopped responding".to_string(),
                    Some(Fault::Exited(Some(code))) => format!("crashed (exit {code})"),
                    _ => "crashed".to_string(),
                };
                eprintln!("bridge: plugin {id} {why}");
                let _ = rtx.send(Reply::Failed { id: *id, error: format!("The plugin {why}. Its process was closed; the program keeps running. Reload it from its window.") });
                poke();
            }
        }
        let req = match rx.recv_timeout(Duration::from_millis(30)) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => Req::Quit,
        };
        match req {
            Req::Load {
                id,
                path,
                state,
                effect,
            } => {
                children.remove(&id);
                match spawn(id, rate, &wtx, std::path::Path::new(&path)) {
                    Ok(c) => {
                        let _ = c.sup.send(&Message::Req(Req::Load {
                            id,
                            path,
                            state,
                            effect,
                        }));
                        children.insert(id, c);
                    }
                    Err(e) => {
                        let _ = rtx.send(Reply::Failed {
                            id,
                            error: format!("Plugin process: {e}"),
                        });
                    }
                }
                poke();
            }
            Req::Unload(id) => {
                if let Some(c) = children.remove(&id) {
                    let _ = c.sup.send(&Message::Req(Req::Quit));
                }
            }
            Req::SaveStates => {
                // Every child's state, merged into one answer (the UI waits for one).
                let live: Vec<u64> = children
                    .iter()
                    .filter(|(_, c)| !c.crashed.load(Ordering::Acquire))
                    .map(|(id, _)| *id)
                    .collect();
                for id in &live {
                    let _ = children[id].sup.send(&Message::Req(Req::SaveStates));
                }
                let mut states = Vec::new();
                let mut waiting: Vec<u64> = live;
                let until = Instant::now() + Duration::from_secs(3);
                while !waiting.is_empty() {
                    let Some(left) = until.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    match wrx.recv_timeout(left) {
                        Ok((id, WireReply::States(s))) => {
                            waiting.retain(|w| *w != id);
                            states.extend(s);
                        }
                        Ok((id, w)) => forward(id, w, &mut children, &rtx),
                        Err(_) => break,
                    }
                }
                let _ = rtx.send(Reply::States(states));
            }
            Req::Rate(r) => {
                rate = r;
                for c in children.values() {
                    let _ = c.sup.send(&Message::Req(Req::Rate(r)));
                }
            }
            Req::Quit => {
                for c in children.values() {
                    let _ = c.sup.send(&Message::Req(Req::Quit));
                }
                // Each child unloads its plugin and leaves; dropping the
                // supervisors waits for that (then insists).
                let until = Instant::now() + Duration::from_secs(2);
                let mut left: Vec<u64> = children.keys().copied().collect();
                while !left.is_empty() {
                    let Some(t) = until.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    match wrx.recv_timeout(t) {
                        Ok((id, WireReply::Bye)) => left.retain(|l| *l != id),
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                children.clear();
                let _ = rtx.send(Reply::Bye);
                return;
            }
            Req::ShowEditor(id) | Req::HideEditor(id) | Req::Params(id) => {
                if let Some(c) = children.get(&id) {
                    let _ = c.sup.send(&Message::Req(req.clone()));
                }
            }
            Req::Embed { id, .. } | Req::SetParam { id, .. } | Req::SetState { id, .. } => {
                if let Some(c) = children.get(&id) {
                    let _ = c.sup.send(&Message::Req(req.clone()));
                }
            }
        }
    }
}

fn spawn(
    id: u64,
    rate: u32,
    wtx: &mpsc::Sender<(u64, WireReply)>,
    plugin: &std::path::Path,
) -> std::io::Result<Child> {
    let proc = BridgeProc::start(
        BridgeProc::own_command_for(plugin)?,
        rate,
        MAX_FRAMES,
        "host",
    )?;
    let sup = proc.sup.clone();
    let crashed = proc.crash_flag();
    let mut rd = sup.reader()?;
    let wtx = wtx.clone();
    std::thread::Builder::new()
        .name(format!("plugin-bridge-{id}"))
        .spawn(move || {
            while let Ok(m) = read_message(&mut rd) {
                if let Message::Reply(w) = m
                    && wtx.send((id, w)).is_err()
                {
                    break;
                }
            }
        })?;
    Ok(Child {
        proc: Some(proc),
        sup,
        crashed,
        reported: false,
    })
}

/// A child's answer as the host thread's reply; `Loaded` hands the UI the proxy.
fn forward(id: u64, w: WireReply, children: &mut HashMap<u64, Child>, rtx: &mpsc::Sender<Reply>) {
    let r = match w {
        WireReply::Loaded {
            id: pid,
            name,
            gui,
            outs,
        } => {
            let Some(p) = children.get_mut(&id).and_then(|c| c.proc.take()) else {
                return;
            };
            Reply::Loaded {
                id: pid,
                proc: share(p),
                name,
                gui,
                outs,
            }
        }
        WireReply::Failed { id, error } => Reply::Failed { id, error },
        WireReply::Editor { id, open } => Reply::Editor { id, open },
        WireReply::States(s) => Reply::States(s),
        WireReply::EditorSize { id, w, h } => Reply::EditorSize { id, w, h },
        WireReply::Params { id, rows } => Reply::Params { id, rows },
        WireReply::Bye => return,
    };
    let _ = rtx.send(r);
}
