//! Plugins in their own process (FL: "bridged"). A crashing or hanging plugin,
//! or a Wine host dying under yabridge, takes down only its `plugin-host`
//! child: the instance goes silent and reports the fault instead of aborting
//! the program. See docs/plugin-bridge-plan.md.
//!
//! The audio path is [`BridgeProc`]: it publishes block N to the child and
//! plays block N-1, so a bridged plugin adds one block of latency and never
//! waits for the child.
#![allow(dead_code)]

mod host;
mod ipc;
mod protocol;
mod shm;
mod supervisor;

pub use host::start_bridged_host;

pub use supervisor::Fault;

use ipc::ControlStream;
use keyframexl::plugin::{PluginEvent, Processor, Transport};
use protocol::{Message, VERSION, read_message, write_message};
use shm::{RawEvent, Shm};
use std::{
    io,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use supervisor::Supervisor;

// ------------------------------------------------------------------ parent

/// The engine's view of a bridged instance (implements [`Processor`]).
pub struct BridgeProc {
    sup: Arc<Supervisor>,
    /// Blocks published so far (the child answers each in turn).
    published: u32,
    /// Frames of the block the child is (or was last) working on.
    pending_frames: usize,
    /// Child output waiting to be played: one block behind the input.
    fifo: Vec<f32>,
    fifo_len: usize,
    /// The same delay for a multi-out plugin's extra pairs: one lane of
    /// `fifo.len()` per pair, kept in step with `fifo`.
    xfifo: Vec<f32>,
    /// Extra pairs the child wrote last.
    pairs: usize,
    /// Blocks the child had not finished in time (played as silence).
    pub xruns: Arc<AtomicU64>,
}

impl BridgeProc {
    /// Start a `plugin-host` child for `mode` (for now the built-in test modes:
    /// `gain:F`, `synth`, `crash-after:N`, `hang`).
    pub fn start(command: Command, rate: u32, max_frames: u32, mode: &str) -> io::Result<Self> {
        static GEN: AtomicU64 = AtomicU64::new(1);
        let generation = GEN.fetch_add(1, Ordering::Relaxed);
        let sup = Supervisor::start(
            command,
            generation,
            rate,
            max_frames,
            keyframexl::plugin::MAX_EVENTS as u32,
            mode,
        )?;
        let mf = max_frames as usize;
        Ok(Self {
            sup: Arc::new(sup),
            published: 0,
            pending_frames: 0,
            fifo: vec![0.0; mf * 2 * 3],
            fifo_len: 0,
            xfifo: Vec::new(),
            pairs: 0,
            xruns: Arc::new(AtomicU64::new(0)),
        })
    }

    /// The program's own binary as the child.
    pub fn own_command() -> io::Result<Command> {
        let mut c = Command::new(std::env::current_exe()?);
        c.arg("plugin-host");
        crate::plugins::apply_wine(&mut c);
        Ok(c)
    }

    /// `own_command` for one plugin (32-bit Windows plugins may need another Wine).
    pub fn own_command_for(plugin: &std::path::Path) -> io::Result<Command> {
        let mut c = Command::new(std::env::current_exe()?);
        c.arg("plugin-host");
        crate::plugins::apply_wine_for(&mut c, plugin);
        Ok(c)
    }

    pub fn crashed(&self) -> bool {
        self.sup.crashed.load(Ordering::Acquire)
    }

    pub fn crash_flag(&self) -> Arc<AtomicBool> {
        self.sup.crashed.clone()
    }

    pub fn fault(&self) -> Option<Fault> {
        self.sup.fault.lock().ok().and_then(|f| f.clone())
    }

    /// Fill `out` (and the extra lanes that fit) from the delayed output;
    /// what is missing stays silent.
    fn pop_fifo(&mut self, out: &mut [f32], extra: &mut [f32], stride: usize) {
        let n = out.len().min(self.fifo_len);
        out[..n].copy_from_slice(&self.fifo[..n]);
        out[n..].fill(0.0);
        let cap = self.fifo.len();
        for k in 0..self.xfifo.len() / cap {
            let lane = &mut self.xfifo[k * cap..(k + 1) * cap];
            if stride > 0 && k * stride + out.len() <= extra.len() {
                let dst = &mut extra[k * stride..k * stride + out.len()];
                dst[..n].copy_from_slice(&lane[..n]);
                dst[n..].fill(0.0);
            }
            lane.copy_within(n..self.fifo_len, 0);
        }
        self.fifo.copy_within(n..self.fifo_len, 0);
        self.fifo_len -= n;
    }

    /// Extra lanes of `len` samples that fit in `extra` at `stride`.
    fn lanes(&self, extra: &[f32], stride: usize, len: usize) -> usize {
        if stride == 0 || extra.len() < len {
            0
        } else {
            ((extra.len() - len) / stride + 1).min(self.pairs)
        }
    }

    fn block(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: Option<&[f32]>,
        extra: &mut [f32],
        stride: usize,
        transport: &Transport,
    ) -> usize {
        let shm = &*self.sup.shm;
        let h = shm.header();
        if self.crashed() {
            io.fill(0.0);
            return 0;
        }
        let offline = keyframexl::plugin::OFFLINE.load(Ordering::Acquire);
        if offline {
            // Export: no deadline, no latency. Let the child finish what it has,
            // forget the live pipeline, then run this block and wait for it.
            if !self.wait_idle() {
                io.fill(0.0);
                return 0;
            }
            self.fifo_len = 0;
        }
        if h.out_seq.load(Ordering::Acquire) != self.published {
            // The child is still on the last block: this one is lost (silence),
            // and nothing new is published until it catches up.
            self.xruns.fetch_add(1, Ordering::Relaxed);
            self.pop_fifo(io, extra, stride);
            return self.lanes(extra, stride, io.len());
        }
        // The child is idle: its output for the last block is ready, and the
        // block buffers are ours until the next publish.
        if self.published != 0 {
            let n = self.pending_frames * 2;
            // SAFETY: the child is idle (out_seq == published), see shm.rs.
            let out = unsafe { &shm.output()[..n] };
            let room = self.fifo.len() - self.fifo_len;
            let k = n.min(room);
            self.fifo[self.fifo_len..self.fifo_len + k].copy_from_slice(&out[..k]);
            let pairs =
                (h.pairs.load(Ordering::Relaxed) as usize).min(keyframexl::plugin::MAX_EXTRA_PAIRS);
            let cap = self.fifo.len();
            if pairs * cap > self.xfifo.len() {
                // Once, when a multi-out plugin first answers.
                self.xfifo.resize(pairs * cap, 0.0);
            }
            self.pairs = pairs;
            let lane_in = shm.max_frames() * 2;
            // SAFETY: the child is idle, as for `output`.
            let src = unsafe { shm.extra() };
            for lane in 0..self.xfifo.len() / cap {
                let dst =
                    &mut self.xfifo[lane * cap + self.fifo_len..lane * cap + self.fifo_len + k];
                if lane < pairs {
                    dst.copy_from_slice(&src[lane * lane_in..lane * lane_in + k])
                } else {
                    dst.fill(0.0)
                }
            }
            self.fifo_len += k;
        }
        let frames = io.len() / 2;
        // SAFETY: as above, the block is ours until in_seq is published.
        unsafe {
            shm.input()[..frames * 2].copy_from_slice(&io[..frames * 2]);
            if let Some(key) = key {
                shm.key()[..frames * 2].copy_from_slice(&key[..frames * 2]);
            }
            h.keyed.store(key.is_some() as u32, Ordering::Relaxed);
            let ev = shm.events();
            let mut n = 0;
            for e in events.iter().take(ev.len()) {
                ev[n] = match *e {
                    PluginEvent::NoteOn {
                        at,
                        pitch,
                        velocity,
                    } => RawEvent {
                        kind: 0,
                        b: [pitch, velocity, 0],
                        at,
                    },
                    PluginEvent::NoteOff { at, pitch } => RawEvent {
                        kind: 1,
                        b: [pitch, 0, 0],
                        at,
                    },
                    PluginEvent::Midi { at, bytes } => RawEvent {
                        kind: 2,
                        b: bytes,
                        at,
                    },
                };
                n += 1;
            }
            h.n_events.store(n as u32, Ordering::Relaxed);
        }
        h.frames.store(frames as u32, Ordering::Relaxed);
        h.rate.store(transport.rate, Ordering::Relaxed);
        h.playing.store(transport.playing as u32, Ordering::Relaxed);
        h.offline.store(offline as u32, Ordering::Relaxed);
        h.cmd.store(shm::CMD_PROCESS, Ordering::Relaxed);
        h.tempo_bits
            .store(transport.tempo.to_bits(), Ordering::Relaxed);
        h.frame.store(transport.frame, Ordering::Relaxed);
        self.pending_frames = frames;
        self.published = self.published.wrapping_add(1);
        h.in_seq.store(self.published, Ordering::Release);
        shm.wake_child();
        if offline {
            let ok = self.wait_idle();
            // Nothing is pending for the live pipeline after an offline block.
            self.pending_frames = 0;
            // SAFETY: the child is idle again (or gone: then silence).
            if !ok {
                io.fill(0.0);
                return 0;
            }
            let shm = &*self.sup.shm;
            io[..frames * 2].copy_from_slice(unsafe { &shm.output()[..frames * 2] });
            self.pairs = (shm.header().pairs.load(Ordering::Relaxed) as usize)
                .min(keyframexl::plugin::MAX_EXTRA_PAIRS);
            let lanes = self.lanes(extra, stride, frames * 2);
            let lane_in = shm.max_frames() * 2;
            let src = unsafe { shm.extra() };
            for k in 0..lanes {
                extra[k * stride..k * stride + frames * 2]
                    .copy_from_slice(&src[k * lane_in..k * lane_in + frames * 2]);
            }
            return lanes;
        }
        self.pop_fifo(io, extra, stride);
        self.lanes(extra, stride, io.len())
    }

    /// Offline only: wait until the child has answered the last block (up to
    /// the hang limit). `false` when it died or hung meanwhile.
    fn wait_idle(&self) -> bool {
        let h = self.sup.shm.header();
        let limit = if keyframexl::plugin::OFFLINE.load(Ordering::Acquire) {
            supervisor::OFFLINE_HANG_AFTER
        } else {
            supervisor::HANG_AFTER
        };
        let until = std::time::Instant::now() + limit;
        while h.out_seq.load(Ordering::Acquire) != self.published {
            if self.crashed() || std::time::Instant::now() >= until {
                return false;
            }
            std::thread::yield_now();
        }
        true
    }

    fn send_cmd(&mut self, offline: bool, cmd: u32) {
        if self.crashed() {
            return;
        }
        let shm = &*self.sup.shm;
        let h = shm.header();
        h.offline.store(offline as u32, Ordering::Release);
        if !self.wait_idle() {
            return;
        }
        self.fifo_len = 0;
        self.pending_frames = 0;
        h.cmd.store(cmd, Ordering::Relaxed);
        h.frames.store(0, Ordering::Relaxed);
        h.n_events.store(0, Ordering::Relaxed);
        self.published = self.published.wrapping_add(1);
        h.in_seq.store(self.published, Ordering::Release);
        shm.wake_child();
        let _ = self.wait_idle();
        h.cmd.store(shm::CMD_PROCESS, Ordering::Relaxed);
    }
}

impl Processor for BridgeProc {
    fn prepare_offline(&mut self) {
        self.send_cmd(true, shm::CMD_PREPARE_OFFLINE);
    }

    fn finish_offline(&mut self) {
        self.send_cmd(false, shm::CMD_FINISH_OFFLINE);
    }

    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], transport: &Transport) {
        self.run(events, io, None, &mut [], 0, transport);
    }

    fn process_keyed(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: &[f32],
        transport: &Transport,
    ) {
        self.run(events, io, Some(key), &mut [], 0, transport);
    }

    fn extra_pairs(&self) -> usize {
        self.pairs
    }

    /// One block behind live (the pipeline), none offline; plus the plugin's own.
    fn latency(&self, frames: usize) -> usize {
        let own = self.sup.shm.header().latency.load(Ordering::Relaxed) as usize;
        let pipeline = if keyframexl::plugin::OFFLINE.load(Ordering::Acquire) || self.crashed() {
            0
        } else {
            frames.min(self.sup.shm.max_frames())
        };
        pipeline + own
    }

    fn process_multi(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        extra: &mut [f32],
        stride: usize,
        transport: &Transport,
    ) -> usize {
        self.run(events, io, None, extra, stride, transport)
    }
}

impl BridgeProc {
    fn run(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: Option<&[f32]>,
        extra: &mut [f32],
        stride: usize,
        transport: &Transport,
    ) -> usize {
        let max = self.sup.shm.max_frames() * 2;
        if io.len() <= max {
            return self.block(events, io, key, extra, stride, transport);
        }
        let mut wrote = usize::MAX;
        // Longer than the shared block: in pieces, events shifted into each.
        let mut start = 0;
        while start < io.len() {
            let end = (start + max).min(io.len());
            let (f0, f1) = ((start / 2) as u32, (end / 2) as u32);
            let mut piece = [PluginEvent::Midi {
                at: 0,
                bytes: [0; 3],
            }; keyframexl::plugin::MAX_EVENTS];
            let mut n = 0;
            for e in events.iter().filter(|e| e.at() >= f0 && e.at() < f1) {
                if n == piece.len() {
                    break;
                }
                piece[n] = match *e {
                    PluginEvent::NoteOn {
                        at,
                        pitch,
                        velocity,
                    } => PluginEvent::NoteOn {
                        at: at - f0,
                        pitch,
                        velocity,
                    },
                    PluginEvent::NoteOff { at, pitch } => {
                        PluginEvent::NoteOff { at: at - f0, pitch }
                    }
                    PluginEvent::Midi { at, bytes } => PluginEvent::Midi { at: at - f0, bytes },
                };
                n += 1;
            }
            let t = Transport {
                frame: transport.frame + f0 as u64,
                ..*transport
            };
            let at = start.min(extra.len());
            let lanes = if stride == 0 {
                &mut [][..]
            } else {
                &mut extra[at..]
            };
            wrote = wrote.min(self.block(
                &piece[..n],
                &mut io[start..end],
                key.map(|k| &k[start..end]),
                lanes,
                stride,
                &t,
            ));
            start = end;
        }
        if wrote == usize::MAX { 0 } else { wrote }
    }
}

// ------------------------------------------------------------------- child

/// The built-in stand-ins the bridge is tested with.
enum Mode {
    Gain(f32),
    /// Input on the main pair, then extra pairs of constant 0.25, 0.5, ... (multi-out tests).
    Multi(usize),
    Synth {
        notes: [Option<(f64, f32)>; 128],
    },
    /// A sidechain-only effect: plays its key, silence without one.
    Key,
    CrashAfter(u64),
    Hang,
}

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        if let Some(g) = s.strip_prefix("gain:") {
            return g.parse().ok().map(Mode::Gain);
        }
        if let Some(n) = s.strip_prefix("multi:") {
            return n.parse().ok().map(Mode::Multi);
        }
        if let Some(n) = s.strip_prefix("crash-after:") {
            return n.parse().ok().map(Mode::CrashAfter);
        }
        match s {
            "synth" => Some(Mode::Synth { notes: [None; 128] }),
            "hang" => Some(Mode::Hang),
            "key" => Some(Mode::Key),
            _ => None,
        }
    }
}

/// `keyframexl plugin-host`: the child's whole life. The control endpoint comes
/// in `KFXL_BRIDGE_SOCKET`.
pub(crate) fn child_main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("KFXL_BRIDGE_SOCKET")?;
    let mut socket = ControlStream::connect(&path)?;
    let rate = match read_message(&mut socket)? {
        Message::Hello {
            version,
            generation,
            rate,
            max_frames,
            max_events,
        } if version == VERSION && rate > 0 && max_frames > 0 && max_events > 0 => {
            write_message(
                &mut socket,
                &Message::Ready {
                    version,
                    generation,
                },
            )?;
            rate
        }
        _ => {
            write_message(
                &mut socket,
                &Message::Error {
                    reason: format!("protocol version {VERSION} expected"),
                },
            )?;
            return Err("bad handshake".into());
        }
    };
    let (mut mode, shm) = match read_message(&mut socket)? {
        // A real plugin host: requests over the control channel, audio through the block.
        Message::Load { mode, shm } if mode == "host" => match Shm::open(&shm) {
            Ok(shm) => {
                write_message(&mut socket, &Message::Loaded)?;
                host::child_host(socket, shm, rate)
            }
            Err(e) => {
                write_message(
                    &mut socket,
                    &Message::Error {
                        reason: format!("audio block: {e}"),
                    },
                )?;
                return Err(e.into());
            }
        },
        Message::Load { mode, shm } => match (Mode::parse(&mode), Shm::open(&shm)) {
            (Some(m), Ok(s)) => (m, s),
            (None, _) => {
                write_message(
                    &mut socket,
                    &Message::Error {
                        reason: format!("unknown plugin {mode}"),
                    },
                )?;
                return Err("bad load".into());
            }
            (_, Err(e)) => {
                write_message(
                    &mut socket,
                    &Message::Error {
                        reason: format!("audio block: {e}"),
                    },
                )?;
                return Err(e.into());
            }
        },
        _ => return Err("expected Load".into()),
    };
    write_message(&mut socket, &Message::Loaded)?;
    let h = shm.header();
    // What it last finished, not what is published: a block published before
    // this loop started must still be processed.
    let mut seen = h.out_seq.load(Ordering::Acquire);
    let mut blocks = 0u64;
    loop {
        let next = h.in_seq.load(Ordering::Acquire);
        if next == seen {
            shm.wait_for_block(seen, 100);
            // The parent closed the control channel or said goodbye: done.
            if socket.try_closed() {
                return Ok(());
            }
            continue;
        }
        let cmd = h.cmd.load(Ordering::Acquire);
        if cmd == shm::CMD_PREPARE_OFFLINE || cmd == shm::CMD_FINISH_OFFLINE {
            if cmd == shm::CMD_PREPARE_OFFLINE
                && let Mode::Synth { notes } = &mut mode
            {
                notes.fill(None);
            }
            seen = next;
            h.out_seq.store(next, Ordering::Release);
            continue;
        }
        let frames = (h.frames.load(Ordering::Relaxed) as usize).min(shm.max_frames());
        // SAFETY: published by the parent with Release, read after Acquire; ours until out_seq.
        let (input, output, events) = unsafe {
            (
                &shm.input()[..frames * 2],
                &mut shm.output()[..frames * 2],
                &shm.events()
                    [..(h.n_events.load(Ordering::Relaxed) as usize).min(shm.max_events())],
            )
        };
        match &mut mode {
            Mode::Gain(g) => {
                for (o, i) in output.iter_mut().zip(input) {
                    *o = i * *g;
                }
            }
            Mode::Synth { notes } => render_synth(notes, events, output, rate as f64),
            Mode::Key => {
                if h.keyed.load(Ordering::Relaxed) != 0 {
                    // SAFETY: ours until out_seq, like `input`.
                    output.copy_from_slice(unsafe { &shm.key()[..frames * 2] });
                } else {
                    output.fill(0.0);
                }
            }
            Mode::Multi(pairs) => {
                output.copy_from_slice(input);
                let stride = shm.max_frames() * 2;
                // SAFETY: ours until out_seq, like `output`.
                let extra = unsafe { shm.extra() };
                for k in 0..*pairs {
                    extra[k * stride..k * stride + frames * 2].fill(0.25 * (k + 1) as f32);
                }
            }
            Mode::CrashAfter(n) => {
                if blocks >= *n {
                    // Mid-block, like a plugin dying in process() (no core dump: tests stay fast).
                    crate::platform::exit_immediately(134);
                }
                output.copy_from_slice(input);
            }
            Mode::Hang => loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            },
        }
        blocks += 1;
        h.pairs.store(
            if let Mode::Multi(p) = mode {
                p as u32
            } else {
                0
            },
            Ordering::Relaxed,
        );
        seen = next;
        h.out_seq.store(next, Ordering::Release);
    }
}

fn render_synth(
    notes: &mut [Option<(f64, f32)>; 128],
    events: &[RawEvent],
    out: &mut [f32],
    rate: f64,
) {
    let mut ei = 0;
    for f in 0..out.len() / 2 {
        while ei < events.len() && events[ei].at as usize <= f {
            let e = events[ei];
            match e.kind {
                0 => notes[e.b[0] as usize & 127] = Some((0.0, e.b[1] as f32 / 127.0 * 0.2)),
                1 => notes[e.b[0] as usize & 127] = None,
                _ => {}
            }
            ei += 1;
        }
        let mut s = 0.0;
        for (k, n) in notes.iter_mut().enumerate() {
            if let Some((ph, amp)) = n {
                s += (*ph * std::f64::consts::TAU).sin() as f32 * *amp;
                *ph = (*ph + 440.0 * 2f64.powf((k as f64 - 69.0) / 12.0) / rate).fract();
            }
        }
        out[f * 2] = s;
        out[f * 2 + 1] = s;
    }
}

#[cfg(test)]
mod tests;
