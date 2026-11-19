//! MIDI hardware output and the 24 PPQN clock master (FL: Options → MIDI settings → Output).
//!
//! Enabled output ports stay open in a shared list, so the UI can send notes,
//! controllers and panic directly, while one background thread owns the clock:
//! it parks on a condition variable whenever Clock Master is off or the transport
//! is stopped, then wakes 24 times per beat and writes `0xF8` to every port.
//! Start, Continue and Stop are edge-triggered by the UI, so a held transport
//! never repeats them.

use crate::midi_in::port_key;
use std::sync::{
    Arc, Condvar, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// MIDI Clock is 24 pulses per quarter note.
const PPQN: f64 = 24.0;

/// Output ports are rescanned for hot-plug as often as the input side.
const SCAN_SECS: f64 = 2.0;

/// A port the system lists, and whether it is connected.
#[derive(Clone, Debug, PartialEq)]
pub struct OutPort {
    pub name: String,
    pub connected: bool,
    pub error: Option<String>,
}

/// What the clock thread and the UI share. The condition variable keeps the
/// thread completely asleep (no polling) unless the clock is actually running.
struct ClockState {
    quit: AtomicBool,
    enabled: AtomicBool,
    playing: AtomicBool,
    /// BPM × 100, so 120.00 BPM is 12000 (an atomic integer, not a float).
    bpm_x100: AtomicU32,
    /// Clock pulses sent; the UI can use it as an activity indicator.
    tx_count: AtomicU64,
    condvar: (Mutex<()>, Condvar),
}

impl ClockState {
    fn new() -> Self {
        Self {
            quit: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            bpm_x100: AtomicU32::new(12_000),
            tx_count: AtomicU64::new(0),
            condvar: (Mutex::new(()), Condvar::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.condvar.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wake the clock thread after a state change. Atomics are stored before the
    /// lock, so a notification cannot slip between the thread's check and its wait.
    fn wake(&self) {
        let _guard = self.lock();
        self.condvar.1.notify_all();
    }

    fn active(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) && self.playing.load(Ordering::Relaxed)
    }
}

pub struct MidiOut {
    conns: Arc<Mutex<Vec<(String, midir::MidiOutputConnection)>>>,
    pub ports: Vec<OutPort>,
    errors: Vec<(String, String)>,
    last_scan: Option<Instant>,
    clock: Arc<ClockState>,
    thread: Option<JoinHandle<()>>,
    /// Transport edges already sent, so a held play state never repeats them.
    was_playing: bool,
    was_master: bool,
}

/// Seconds between two MIDI Clock pulses at `bpm` (24 per quarter note).
pub fn clock_interval(bpm: f64) -> Duration {
    Duration::from_secs_f64(60.0 / (clamp_bpm(bpm) * PPQN))
}

/// The transport message for one state edge: Start at song start, Continue
/// mid-song, Stop on release, and nothing while the state is unchanged.
pub fn transport_message(was_active: bool, now_active: bool, at_start: bool) -> Option<u8> {
    match (was_active, now_active) {
        (false, true) if at_start => Some(0xFA),
        (false, true) => Some(0xFB),
        (true, false) => Some(0xFC),
        _ => None,
    }
}

/// The clock honours 10..=999 BPM; nonsense tempos fall back to 120.
fn clamp_bpm(bpm: f64) -> f64 {
    if bpm.is_finite() {
        bpm.clamp(10.0, 999.0)
    } else {
        120.0
    }
}

fn bpm_x100(bpm: f64) -> u32 {
    (clamp_bpm(bpm) * 100.0).round() as u32
}

/// Our own ports (midir lists the program's virtual ports too) and the ALSA
/// "Midi Through" loop are not outputs (`KFXL_MIDI_THROUGH=1` keeps the loop,
/// for live checks).
fn skip(name: &str) -> bool {
    (name.starts_with("Midi Through") && std::env::var_os("KFXL_MIDI_THROUGH").is_none())
        || name.contains("KeyFrameXL")
}

impl MidiOut {
    pub fn new() -> Self {
        let conns = Arc::new(Mutex::new(Vec::new()));
        let clock = Arc::new(ClockState::new());
        let thread = std::thread::Builder::new()
            .name("KeyFrameXL clock".into())
            .spawn({
                let (conns, clock) = (conns.clone(), clock.clone());
                move || clock_thread(conns, clock)
            })
            .ok();
        Self {
            conns,
            ports: vec![],
            errors: vec![],
            last_scan: None,
            clock,
            thread,
            was_playing: false,
            was_master: false,
        }
    }

    /// Connect enabled ports that appeared, drop ones that left or were switched
    /// off. Cheap enough for every couple of seconds.
    pub fn refresh(&mut self, is_enabled: impl Fn(&str) -> bool) {
        self.last_scan = Some(Instant::now());
        let Ok(probe) = midir::MidiOutput::new("KeyFrameXL out scan") else {
            if let Ok(mut conns) = self.conns.lock() {
                conns.clear();
            }
            self.ports.clear();
            return;
        };
        let names: Vec<String> = probe
            .ports()
            .iter()
            .filter_map(|p| probe.port_name(p).ok())
            .filter(|n| !skip(n))
            .collect();
        drop(probe);
        if let Ok(mut conns) = self.conns.lock() {
            conns.retain(|(n, _)| names.contains(n) && is_enabled(port_key(n)));
        }
        self.errors.retain(|(n, _)| names.contains(n));
        for name in &names {
            if !is_enabled(port_key(name)) || self.has(name) || self.failed(name) {
                continue;
            }
            match self.connect(name) {
                Ok(conn) => {
                    if let Ok(mut conns) = self.conns.lock() {
                        conns.push((name.clone(), conn));
                    }
                }
                Err(e) => self.errors.push((name.clone(), e)),
            }
        }
        self.ports = names
            .iter()
            .map(|n| OutPort {
                name: port_key(n).to_string(),
                connected: self.has(n),
                error: self
                    .errors
                    .iter()
                    .find(|(c, _)| c == n)
                    .map(|(_, e)| e.clone()),
            })
            .collect();
    }

    /// Forget a failed port so the next refresh tries again.
    pub fn retry(&mut self) {
        self.errors.clear();
        self.last_scan = None;
    }

    /// Rescan now and then (hot-plug) without blocking the frame.
    pub fn poll_refresh(&mut self, is_enabled: impl Fn(&str) -> bool) {
        if self
            .last_scan
            .is_none_or(|t| t.elapsed().as_secs_f64() > SCAN_SECS)
        {
            self.refresh(is_enabled);
        }
    }

    /// Send bytes to every open output port; a transient per-port error is ignored.
    pub fn send_raw(&self, bytes: &[u8]) {
        if let Ok(mut conns) = self.conns.lock() {
            for (_, conn) in conns.iter_mut() {
                let _ = conn.send(bytes);
            }
        }
    }

    /// Update the clock thread's state. Transport messages are not sent here:
    /// [`sync_transport`](Self::sync_transport) or the explicit send methods do
    /// that, so a state written every frame never spams the ports.
    pub fn set_clock(&self, enabled: bool, playing: bool, bpm: f64) {
        let was_active = self.clock.active();
        let bpm_x100 = bpm_x100(bpm);
        let old_bpm = self.clock.bpm_x100.swap(bpm_x100, Ordering::Relaxed);
        self.clock.enabled.store(enabled, Ordering::Relaxed);
        self.clock.playing.store(playing, Ordering::Relaxed);
        if was_active != (enabled && playing) || old_bpm != bpm_x100 {
            self.clock.wake();
        }
    }

    /// Follow the transport exactly once per edge: Start (or Continue) when
    /// Clock Master playback begins, Stop when it ends. An unchanged state
    /// sends nothing, so calling this every frame is safe.
    pub fn sync_transport(&mut self, clock_master: bool, playing: bool, at_start: bool, bpm: f64) {
        let was_active = self.was_master && self.was_playing;
        let now_active = clock_master && playing;
        // The edge goes first: a slave must not see a clock pulse before Start,
        // nor one after Stop.
        if let Some(msg) = transport_message(was_active, now_active, at_start) {
            self.send_raw(&[msg]);
        }
        self.was_master = clock_master;
        self.was_playing = playing;
        self.set_clock(clock_master, playing, bpm);
    }

    /// All Sound Off, All Notes Off and Sustain Off on every channel (FL: panic).
    pub fn panic(&self) {
        if let Ok(mut conns) = self.conns.lock() {
            for (_, conn) in conns.iter_mut() {
                for ch in 0..16u8 {
                    for cc in [120, 123, 64] {
                        let _ = conn.send(&[0xB0 | ch, cc, 0]);
                    }
                }
            }
        }
    }

    /// Open output ports.
    pub fn connected_count(&self) -> usize {
        self.conns.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Clock pulses sent so far (the activity indicator).
    pub fn ticks(&self) -> u64 {
        self.clock.tx_count.load(Ordering::Relaxed)
    }

    fn has(&self, name: &str) -> bool {
        self.conns
            .lock()
            .is_ok_and(|conns| conns.iter().any(|(n, _)| n == name))
    }

    fn failed(&self, name: &str) -> bool {
        self.errors.iter().any(|(n, _)| n == name)
    }

    fn connect(&self, name: &str) -> Result<midir::MidiOutputConnection, String> {
        let output = midir::MidiOutput::new("KeyFrameXL out").map_err(|e| e.to_string())?;
        let port = output
            .ports()
            .into_iter()
            .find(|p| output.port_name(p).ok().as_deref() == Some(name))
            .ok_or("Port is gone")?;
        output
            .connect(&port, "KeyFrameXL out")
            .map_err(|e| e.to_string())
    }
}

impl Drop for MidiOut {
    fn drop(&mut self) {
        self.clock.quit.store(true, Ordering::Release);
        self.clock.wake();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The clock thread: sleep on the condition variable while inactive, then keep
/// an absolute target so a late wake-up is absorbed instead of drifting slower.
fn clock_thread(
    conns: Arc<Mutex<Vec<(String, midir::MidiOutputConnection)>>>,
    clock: Arc<ClockState>,
) {
    let mut guard = clock.lock();
    let mut next: Option<Instant> = None;
    let mut interval = Duration::ZERO;
    loop {
        if clock.quit.load(Ordering::Acquire) {
            break;
        }
        if !clock.active() {
            next = None;
            guard = clock
                .condvar
                .1
                .wait(guard)
                .unwrap_or_else(|e| e.into_inner());
            continue;
        }
        let now = Instant::now();
        let iv = clock_interval(clock.bpm_x100.load(Ordering::Relaxed) as f64 / 100.0);
        if next.is_none() {
            // Starting: pulse right away, then hold the grid.
            next = Some(now);
            interval = iv;
        } else if iv != interval {
            // Tempo changed: re-space from now, never a catch-up burst.
            next = Some(now + iv);
            interval = iv;
        }
        let target = next.unwrap_or(now);
        if target > now {
            let (g, _) = clock
                .condvar
                .1
                .wait_timeout(guard, target - now)
                .unwrap_or_else(|e| e.into_inner());
            guard = g;
            continue;
        }
        if let Ok(mut conns) = conns.lock() {
            for (_, conn) in conns.iter_mut() {
                let _ = conn.send(&[0xF8]);
            }
        }
        clock.tx_count.fetch_add(1, Ordering::Relaxed);
        let mut t = target + iv;
        if now.saturating_duration_since(t) > iv * 4 {
            // A suspended machine or a long stall: re-align instead of racing.
            t = now + iv;
        }
        next = Some(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(d: Duration) -> f64 {
        d.as_secs_f64() * 1000.0
    }

    #[test]
    fn clock_interval_matches_24_ppqn() {
        assert!((millis(clock_interval(120.0)) - 20.833_333).abs() < 0.001);
        assert!((millis(clock_interval(60.0)) - 41.666_667).abs() < 0.001);
        assert!((millis(clock_interval(240.0)) - 10.416_667).abs() < 0.001);
        assert_eq!(
            clock_interval(120.0),
            Duration::from_secs_f64(60.0 / 2880.0)
        );
        // Slower/faster than the range clamps, not divides by a wild number.
        assert_eq!(clock_interval(5.0), clock_interval(10.0));
        assert_eq!(
            clock_interval(10.0),
            Duration::from_secs_f64(60.0 / (10.0 * 24.0))
        );
        assert_eq!(clock_interval(2_000.0), clock_interval(999.0));
        assert_eq!(
            clock_interval(999.0),
            Duration::from_secs_f64(60.0 / (999.0 * 24.0))
        );
        assert_eq!(clock_interval(0.0), clock_interval(10.0));
    }

    #[test]
    fn transport_edges_emit_start_continue_stop() {
        assert_eq!(
            transport_message(false, true, true),
            Some(0xFA),
            "playing from the top starts"
        );
        assert_eq!(
            transport_message(false, true, false),
            Some(0xFB),
            "playing mid-song continues"
        );
        assert_eq!(
            transport_message(true, false, false),
            Some(0xFC),
            "stopping is a stop wherever it lands"
        );
        assert_eq!(
            transport_message(true, false, true),
            Some(0xFC),
            "stop never repeats a start"
        );
        assert_eq!(transport_message(true, true, true), None);
        assert_eq!(transport_message(false, false, true), None);
    }

    #[test]
    fn midi_out_starts_and_drops_cleanly() {
        let out = MidiOut::new();
        out.set_clock(false, false, 120.0);
        assert_eq!(out.connected_count(), 0);
        assert_eq!(out.ticks(), 0);
        drop(out);
    }
}
