//! MIDI keyboards and controllers (FL: Options → MIDI settings → Input).
//!
//! Every enabled input port gets a connection whose callback runs on midir's
//! thread: notes go straight to the audio thread as live notes for the selected
//! channel (no wait for the next UI frame), and a copy goes to the UI for the
//! keyboard display, the activity light and note recording. Ports are matched
//! by name, so a replugged keyboard comes back on its own.

use crate::audio::LiveCmd;
use crate::settings::MidiSettings;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};

/// Live-note ids from MIDI: far above the UI's counter, one per incoming key.
const MIDI_ID: u64 = 1 << 40;

/// What reached the UI.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `pitch` after transpose; `frame` is the song position when it arrived.
    On {
        pitch: u8,
        velocity: u8,
        frame: u64,
    },
    Off {
        pitch: u8,
        frame: u64,
    },
    /// A controller (`cc` 0..=127), pitch bend (`cc` 255), program change
    /// (`cc` 254) or aftertouch (`cc` 253); `channel` is 1..=16.
    Control {
        channel: u8,
        cc: u8,
        value: u16,
    },
    /// Incoming MIDI Clock Start (0xFA) in Slave mode.
    ClockStart,
    /// Incoming MIDI Clock Continue (0xFB) in Slave mode.
    ClockContinue,
    /// Incoming MIDI Clock Stop (0xFC) in Slave mode.
    ClockStop,
    /// Estimated tempo from incoming 24 PPQN MIDI Clock ticks (0xF8) in Slave mode.
    ClockTempo(f64),
}

/// One message from a port, with its driver microsecond timestamp and callback
/// arrival instant for low-jitter timing (e.g. MIDI tap tempo).
#[derive(Clone, Debug)]
pub struct Incoming {
    pub port: String,
    pub stamp_us: u64,
    pub received_at: std::time::Instant,
    pub event: Event,
}

/// Where callbacks send their notes; swapped when the audio output restarts.
#[derive(Default)]
pub struct Route {
    pub live: Option<Arc<Mutex<rtrb::Producer<LiveCmd>>>>,
    pub position: Option<Arc<AtomicU64>>,
    pub repaint: Option<eframe::egui::Context>,
}

/// Keys and pedal, shared by all ports (two keyboards play one channel).
struct Keys {
    /// Original key → (sounding pitch, held by the pedal after release).
    sounding: [Option<(u8, bool)>; 128],
    pedal: bool,
}

/// Estimates tempo from incoming 24 PPQN MIDI Clock ticks (`0xF8`).
#[derive(Default)]
struct ClockTracker {
    /// Rolling window of recent tick timestamps in microseconds (up to 25 = 1 beat).
    stamps: std::collections::VecDeque<u64>,
    ticks_since_emit: u8,
    last_bpm: Option<f64>,
}

impl ClockTracker {
    const WINDOW: usize = 25;
    /// Gaps longer than 500 ms (~5 BPM) mean the external clock stopped or restarted.
    const MAX_GAP_US: u64 = 500_000;

    fn reset(&mut self) {
        self.stamps.clear();
        self.ticks_since_emit = 0;
        self.last_bpm = None;
    }

    fn tick(&mut self, stamp_us: u64) -> Option<f64> {
        if let Some(&prev) = self.stamps.back()
            && (stamp_us <= prev || stamp_us - prev > Self::MAX_GAP_US)
        {
            self.stamps.clear();
            self.ticks_since_emit = 0;
        }
        self.stamps.push_back(stamp_us);
        while self.stamps.len() > Self::WINDOW {
            self.stamps.pop_front();
        }
        self.ticks_since_emit = self.ticks_since_emit.saturating_add(1);
        if self.stamps.len() < 7 || self.ticks_since_emit < 6 {
            return None;
        }
        self.ticks_since_emit = 0;
        let span_us = self.stamps.back()? - self.stamps.front()?;
        if span_us == 0 {
            return None;
        }
        let intervals = (self.stamps.len() - 1) as f64;
        let raw_bpm = 60_000_000.0 * intervals / (span_us as f64 * 24.0);
        let bpm = (raw_bpm.clamp(20.0, 300.0) * 10.0).round() / 10.0;
        if self.last_bpm.is_some_and(|prev| (prev - bpm).abs() < 0.05) {
            return None;
        }
        self.last_bpm = Some(bpm);
        Some(bpm)
    }
}

pub struct Shared {
    /// Channel (instrument id) that incoming notes play.
    pub target: AtomicU64,
    pub prefs: Mutex<MidiSettings>,
    pub route: Mutex<Route>,
    keys: Mutex<Keys>,
    clock: Mutex<ClockTracker>,
}

impl Shared {
    fn send(&self, cmd: LiveCmd) {
        if let Ok(r) = self.route.lock()
            && let Some(live) = &r.live
            && let Ok(mut l) = live.lock()
        {
            let _ = l.push(cmd);
        }
    }

    fn frame(&self) -> u64 {
        self.route
            .lock()
            .ok()
            .and_then(|r| r.position.as_ref().map(|p| p.load(Ordering::Relaxed)))
            .unwrap_or(0)
    }

    /// Turn raw bytes into live notes and a UI event.
    pub fn handle(&self, bytes: &[u8]) -> Option<Event> {
        self.handle_at(0, bytes)
    }

    /// Turn raw bytes with a microsecond timestamp (from midir) into live notes / clock events.
    pub fn handle_at(&self, stamp_us: u64, bytes: &[u8]) -> Option<Event> {
        let (&status, rest) = bytes.split_first()?;
        if status < 0x80 {
            return None;
        }
        let prefs = self.prefs.lock().ok()?.clone();
        if status >= 0xF0 {
            if prefs.clock != crate::settings::MidiClockMode::Slave {
                return None;
            }
            let mut clk = self.clock.lock().ok()?;
            return match status {
                0xF8 => clk.tick(stamp_us).map(Event::ClockTempo),
                0xFA => {
                    clk.reset();
                    Some(Event::ClockStart)
                }
                0xFB => Some(Event::ClockContinue),
                0xFC => {
                    clk.reset();
                    Some(Event::ClockStop)
                }
                _ => None,
            };
        }
        let channel = (status & 0x0F) + 1;
        if prefs.channel != 0 && prefs.channel != channel {
            return None;
        }
        let d1 = rest.first().copied().unwrap_or(0) & 0x7F;
        let d2 = rest.get(1).copied().unwrap_or(0) & 0x7F;
        let mut keys = self.keys.lock().ok()?;
        match status & 0xF0 {
            0x90 if d2 > 0 => {
                let pitch = (d1 as i16 + prefs.transpose as i16).clamp(0, 127) as u8;
                let velocity = prefs.velocity.apply(d2, prefs.fixed_velocity);
                let id = MIDI_ID | d1 as u64;
                if keys.sounding[d1 as usize].take().is_some() {
                    // Struck again while the pedal held it: restart.
                    self.send(LiveCmd::Off { id });
                }
                keys.sounding[d1 as usize] = Some((pitch, false));
                let inst = self.target.load(Ordering::Relaxed);
                self.send(LiveCmd::On {
                    id,
                    inst,
                    pitch,
                    velocity,
                    ch: status & 0x0F,
                    lyric: 0,
                });
                Some(Event::On {
                    pitch,
                    velocity,
                    frame: self.frame(),
                })
            }
            0x80 | 0x90 => {
                let (pitch, _) = keys.sounding[d1 as usize]?;
                if keys.pedal && prefs.sustain {
                    keys.sounding[d1 as usize] = Some((pitch, true));
                } else {
                    keys.sounding[d1 as usize] = None;
                    self.send(LiveCmd::Off {
                        id: MIDI_ID | d1 as u64,
                    });
                }
                Some(Event::Off {
                    pitch,
                    frame: self.frame(),
                })
            }
            0xB0 => {
                // Controllers reach plugin channels (the pedal only when we do not hold notes ourselves).
                if d1 != 64 || !prefs.sustain {
                    self.send(LiveCmd::Midi {
                        inst: self.target.load(Ordering::Relaxed),
                        bytes: [0xB0 | (status & 0x0F), d1, d2],
                    });
                }
                match d1 {
                    64 => {
                        keys.pedal = d2 >= 64;
                        if !keys.pedal {
                            self.release_held(&mut keys);
                        }
                    }
                    // All sound off / all notes off.
                    120 | 123 => self.release_all(&mut keys),
                    _ => {}
                }
                Some(Event::Control {
                    channel,
                    cc: d1,
                    value: d2 as u16,
                })
            }
            0xE0 => {
                self.send(LiveCmd::Midi {
                    inst: self.target.load(Ordering::Relaxed),
                    bytes: [0xE0 | (status & 0x0F), d1, d2],
                });
                Some(Event::Control {
                    channel,
                    cc: 255,
                    value: (d2 as u16) << 7 | d1 as u16,
                })
            }
            0xC0 => {
                self.send(LiveCmd::Midi {
                    inst: self.target.load(Ordering::Relaxed),
                    bytes: [0xC0 | (status & 0x0F), d1, 0],
                });
                Some(Event::Control {
                    channel,
                    cc: 254,
                    value: d1 as u16,
                })
            }
            0xD0 => {
                self.send(LiveCmd::Midi {
                    inst: self.target.load(Ordering::Relaxed),
                    bytes: [0xD0 | (status & 0x0F), d1, 0],
                });
                Some(Event::Control {
                    channel,
                    cc: 253,
                    value: d1 as u16,
                })
            }
            _ => None,
        }
    }

    fn release_held(&self, keys: &mut Keys) {
        for k in 0..128 {
            if let Some((_, true)) = keys.sounding[k] {
                keys.sounding[k] = None;
                self.send(LiveCmd::Off {
                    id: MIDI_ID | k as u64,
                });
            }
        }
    }

    fn release_all(&self, keys: &mut Keys) {
        keys.pedal = false;
        for k in 0..128 {
            if keys.sounding[k].take().is_some() {
                self.send(LiveCmd::Off {
                    id: MIDI_ID | k as u64,
                });
            }
        }
    }

    /// Stop every note that came from MIDI (FL: panic).
    pub fn panic(&self) {
        if let Ok(mut keys) = self.keys.lock() {
            self.release_all(&mut keys);
        }
        for k in 0..128u64 {
            self.send(LiveCmd::Off { id: MIDI_ID | k });
        }
    }
}

/// A port the system lists, and whether it is connected.
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub name: String,
    pub connected: bool,
    pub error: Option<String>,
}

pub struct MidiIn {
    pub shared: Arc<Shared>,
    conns: Vec<(String, midir::MidiInputConnection<()>)>,
    errors: Vec<(String, String)>,
    pub ports: Vec<Port>,
    tx: mpsc::Sender<Incoming>,
    rx: mpsc::Receiver<Incoming>,
    last_scan: Option<std::time::Instant>,
    /// When a note or controller last arrived (the activity light), and from where.
    pub last_activity: Option<std::time::Instant>,
    pub last_port: Option<String>,
}

/// Our own ports (midir lists the program's virtual ports too) and the ALSA
/// "Midi Through" loop are not keyboards (`KFXL_MIDI_THROUGH=1` keeps the loop,
/// for live checks that play a file into it).
fn skip(name: &str) -> bool {
    (name.starts_with("Midi Through") && std::env::var_os("KFXL_MIDI_THROUGH").is_none())
        || name.contains("KeyFrameXL")
}

/// A port's name without ALSA's "client:port" numbers, which change when a
/// device is plugged in again ("MPK mini:MPK mini MIDI 1 24:0" → "MPK mini:MPK mini MIDI 1").
pub fn port_key(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((head, tail))
            if tail.split_once(':').is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && a.chars().chain(b.chars()).all(|c| c.is_ascii_digit())
            }) =>
        {
            head
        }
        _ => name,
    }
}

impl MidiIn {
    pub fn new(prefs: MidiSettings) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            target: AtomicU64::new(0),
            prefs: Mutex::new(prefs),
            route: Mutex::new(Route::default()),
            keys: Mutex::new(Keys {
                sounding: [None; 128],
                pedal: false,
            }),
            clock: Mutex::new(ClockTracker::default()),
        });
        Self {
            shared,
            conns: vec![],
            errors: vec![],
            ports: vec![],
            tx,
            rx,
            last_scan: None,
            last_activity: None,
            last_port: None,
        }
    }

    pub fn set_prefs(&self, prefs: &MidiSettings) {
        if let Ok(mut p) = self.shared.prefs.lock() {
            *p = prefs.clone();
        }
    }

    /// Connect enabled ports that appeared, drop ones that left or were switched off.
    /// Cheap enough for every couple of seconds.
    pub fn refresh(&mut self, prefs: &MidiSettings) {
        self.last_scan = Some(std::time::Instant::now());
        let Ok(probe) = midir::MidiInput::new("KeyFrameXL scan") else {
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
        self.conns
            .retain(|(n, _)| names.contains(n) && prefs.is_on(port_key(n)));
        self.errors.retain(|(n, _)| names.contains(n));
        for name in &names {
            if !prefs.is_on(port_key(name))
                || self.conns.iter().any(|(n, _)| n == name)
                || self.errors.iter().any(|(n, _)| n == name)
            {
                continue;
            }
            match self.connect(name) {
                Ok(c) => self.conns.push((name.clone(), c)),
                Err(e) => self.errors.push((name.clone(), e)),
            }
        }
        self.ports = names
            .iter()
            .map(|n| Port {
                name: port_key(n).to_string(),
                connected: self.conns.iter().any(|(c, _)| c == n),
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
    }

    fn connect(&self, name: &str) -> Result<midir::MidiInputConnection<()>, String> {
        let mut input = midir::MidiInput::new("KeyFrameXL").map_err(|e| e.to_string())?;
        // Keep Time (0xF8/0xFA/0xFB/0xFC) so MIDI Clock Slave works when enabled.
        input.ignore(midir::Ignore::Sysex | midir::Ignore::ActiveSense);
        let port = input
            .ports()
            .into_iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(name))
            .ok_or("Port is gone")?;
        let (shared, tx, port_name) = (self.shared.clone(), self.tx.clone(), name.to_string());
        input
            .connect(
                &port,
                "KeyFrameXL input",
                move |stamp, bytes, _| {
                    let received_at = std::time::Instant::now();
                    if let Some(event) = shared.handle_at(stamp, bytes) {
                        let _ = tx.send(Incoming {
                            port: port_name.clone(),
                            stamp_us: stamp,
                            received_at,
                            event,
                        });
                        if let Ok(r) = shared.route.lock()
                            && let Some(ctx) = &r.repaint
                        {
                            ctx.request_repaint();
                        }
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())
    }

    /// Rescan now and then (hot-plug) and hand over what arrived since last frame.
    pub fn poll(&mut self, prefs: &MidiSettings) -> Vec<Incoming> {
        if self
            .last_scan
            .is_none_or(|t| t.elapsed().as_secs_f64() > 2.0)
        {
            self.refresh(prefs);
        }
        let got: Vec<Incoming> = self.rx.try_iter().collect();
        if let Some(last) = got.last() {
            self.last_activity = Some(std::time::Instant::now());
            self.last_port = Some(last.port.clone());
        }
        got
    }

    pub fn connected(&self) -> usize {
        self.conns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::VelocityCurve;

    fn shared(prefs: MidiSettings) -> (Arc<Shared>, rtrb::Consumer<LiveCmd>) {
        let (tx, rx) = rtrb::RingBuffer::new(512);
        let s = MidiIn::new(prefs).shared;
        s.route.lock().unwrap().live = Some(Arc::new(Mutex::new(tx)));
        s.target.store(7, Ordering::Relaxed);
        (s, rx)
    }

    fn drain(rx: &mut rtrb::Consumer<LiveCmd>) -> Vec<String> {
        let mut v = vec![];
        while let Ok(c) = rx.pop() {
            v.push(match c {
                LiveCmd::On {
                    id,
                    inst,
                    pitch,
                    velocity,
                    ..
                } => format!("on {} {inst} {pitch} {velocity}", id & 0xFF),
                LiveCmd::Off { id } => format!("off {}", id & 0xFF),
                LiveCmd::Midi { inst, bytes } => {
                    format!("midi {inst} {:02X} {} {}", bytes[0], bytes[1], bytes[2])
                }
            });
        }
        v
    }

    #[test]
    fn keys_play_the_selected_channel_with_transpose_and_curve() {
        let (s, mut rx) = shared(MidiSettings {
            transpose: 12,
            velocity: VelocityCurve::Fixed,
            fixed_velocity: 90,
            ..Default::default()
        });
        assert_eq!(
            s.handle(&[0x90, 60, 30]),
            Some(Event::On {
                pitch: 72,
                velocity: 90,
                frame: 0
            })
        );
        assert_eq!(
            s.handle(&[0x80, 60, 0]),
            Some(Event::Off {
                pitch: 72,
                frame: 0
            })
        );
        assert_eq!(
            s.handle(&[0x90, 61, 0]),
            None,
            "note-on with velocity 0 is an off for a key that is not down"
        );
        assert_eq!(drain(&mut rx), ["on 60 7 72 90", "off 60"]);
    }

    #[test]
    fn sustain_pedal_holds_released_keys_until_lifted() {
        let (s, mut rx) = shared(MidiSettings::default());
        s.handle(&[0xB0, 64, 127]);
        assert_eq!(
            drain(&mut rx),
            Vec::<String>::new(),
            "the pedal is ours, not the plugin's"
        );
        s.handle(&[0x90, 48, 100]);
        s.handle(&[0x80, 48, 0]);
        assert_eq!(
            drain(&mut rx),
            ["on 48 7 48 100"],
            "the pedal keeps it sounding"
        );
        s.handle(&[0x90, 48, 100]);
        assert_eq!(
            drain(&mut rx),
            ["off 48", "on 48 7 48 100"],
            "a held key struck again restarts"
        );
        s.handle(&[0x80, 48, 0]);
        s.handle(&[0xB0, 64, 0]);
        assert_eq!(drain(&mut rx), ["off 48"]);
    }

    #[test]
    fn port_names_lose_their_alsa_numbers() {
        assert_eq!(
            port_key("MPK mini:MPK mini MIDI 1 24:0"),
            "MPK mini:MPK mini MIDI 1"
        );
        assert_eq!(port_key("Keystation 49"), "Keystation 49");
        assert_eq!(port_key("Synth 2:3 part"), "Synth 2:3 part");
    }

    #[test]
    fn channel_filter_and_controls() {
        let (s, mut rx) = shared(MidiSettings {
            channel: 2,
            ..Default::default()
        });
        assert_eq!(
            s.handle(&[0x90, 60, 100]),
            None,
            "channel 1 is filtered out"
        );
        assert!(s.handle(&[0x91, 60, 100]).is_some());
        assert_eq!(
            s.handle(&[0xE1, 0x00, 0x40]),
            Some(Event::Control {
                channel: 2,
                cc: 255,
                value: 8192
            })
        );
        assert_eq!(
            s.handle(&[0xB1, 74, 100]),
            Some(Event::Control {
                channel: 2,
                cc: 74,
                value: 100
            }),
            "CCs carry their channel for controller learn"
        );
        s.handle(&[0xB1, 123, 0]);
        // The MIDI channel rides along (MPE on the synth; plugins still get channel 1).
        assert_eq!(
            drain(&mut rx),
            [
                "on 60 7 60 100",
                "midi 7 E1 0 64",
                "midi 7 B1 74 100",
                "midi 7 B1 123 0",
                "off 60"
            ],
            "bend goes to the channel; all notes off"
        );
        assert_eq!(s.handle(&[0xF8]), None, "clock is ignored");
    }

    #[test]
    fn clock_slave_tracks_tempo_and_transport() {
        let (s, _) = shared(MidiSettings {
            clock: crate::settings::MidiClockMode::Slave,
            ..Default::default()
        });
        assert_eq!(s.handle_at(1_000, &[0xFA]), Some(Event::ClockStart));
        assert_eq!(s.handle_at(2_000, &[0xFB]), Some(Event::ClockContinue));
        assert_eq!(s.handle_at(3_000, &[0xFC]), Some(Event::ClockStop));

        // 120 BPM at 24 PPQN = 60_000_000 / (120 * 24) = 20_833.333... us per tick.
        let mut last = None;
        for i in 0..25u64 {
            let stamp = (i as f64 * (60_000_000.0 / (120.0 * 24.0))).round() as u64;
            if let Some(ev) = s.handle_at(stamp, &[0xF8]) {
                last = Some(ev);
            }
        }
        assert_eq!(last, Some(Event::ClockTempo(120.0)));

        // Tempo jump to 140 BPM.
        let base = 2_000_000u64; // >500ms gap resets window cleanly
        let mut last = None;
        for i in 0..25u64 {
            let stamp = base + (i as f64 * (60_000_000.0 / (140.0 * 24.0))).round() as u64;
            if let Some(ev) = s.handle_at(stamp, &[0xF8]) {
                last = Some(ev);
            }
        }
        assert_eq!(last, Some(Event::ClockTempo(140.0)));
    }
}
