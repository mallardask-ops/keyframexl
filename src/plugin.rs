//! Hosted plugins as the engine sees them: a processor that takes timed note
//! events and a stereo block. The format (VST3 through yabridge, later CLAP)
//! lives in the program; the engine only knows this trait, so it stays
//! testable with a stand-in plugin.
//!
//! Instances are shared between the UI thread (editor, state, parameters) and
//! the audio thread (process) behind one mutex; the UI holds it only briefly.

use std::sync::{Arc, Mutex};

/// A note or controller at frame `at` of the block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PluginEvent {
    NoteOn {
        at: u32,
        pitch: u8,
        velocity: u8,
    },
    NoteOff {
        at: u32,
        pitch: u8,
    },
    /// Raw channel message (controllers, pitch bend, aftertouch).
    Midi {
        at: u32,
        bytes: [u8; 3],
    },
}

impl PluginEvent {
    pub fn at(&self) -> u32 {
        match *self {
            Self::NoteOn { at, .. } | Self::NoteOff { at, .. } | Self::Midi { at, .. } => at,
        }
    }
}

/// Song state for the block (plugins read tempo and position from it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
    pub rate: u32,
    pub tempo: f64,
    pub playing: bool,
    /// Song position at the block start, in frames.
    pub frame: u64,
}

pub trait Processor: Send {
    /// Render one block in place: `io` is interleaved stereo, holding the input
    /// for an effect and silence for an instrument. `events` are sorted by `at`.
    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], transport: &Transport);

    /// Stereo output pairs past the main one (a multi-out instrument: SSD5
    /// has 23), at most [`MAX_EXTRA_PAIRS`].
    fn extra_pairs(&self) -> usize {
        0
    }

    /// `process`, also writing the extra pairs: pair `k` goes interleaved to
    /// `extra[k * stride..][..io.len()]`. Returns how many pairs it wrote; the
    /// caller hears only those.
    fn process_multi(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        extra: &mut [f32],
        stride: usize,
        transport: &Transport,
    ) -> usize {
        let _ = (extra, stride);
        self.process(events, io, transport);
        0
    }

    /// `process` with a sidechain `key` (interleaved stereo, as long as `io`):
    /// an effect with a sidechain input (VST3 aux input bus, VST2 inputs 3-4)
    /// hears it there; others ignore it.
    fn process_keyed(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: &[f32],
        transport: &Transport,
    ) {
        let _ = key;
        self.process(events, io, transport);
    }
    /// Frames its output lags its input in a block of `frames`: the plugin's
    /// own reported latency, plus the bridge's block when it runs live in
    /// another process. The engine delays everything else to match (PDC).
    fn latency(&self, frames: usize) -> usize {
        let _ = frames;
        0
    }

    /// External instruments run on an independent audio clock.
    fn needs_realtime(&self) -> bool {
        false
    }

    fn available(&self) -> bool {
        true
    }

    fn prepare_offline(&mut self) {}
}

/// The longest delay latency compensation will add, in frames.
pub const MAX_PDC: usize = 16384;

/// A stereo delay line for latency compensation. The ring is allocated the
/// first time a delay is asked for, once (the audio thread does not grow it
/// again).
#[derive(Default)]
pub struct Pdc {
    ring: Vec<f32>,
    pos: usize,
}

impl Pdc {
    /// Delay interleaved stereo `io` by `frames` (up to `MAX_PDC - 1`) in place.
    pub fn run(&mut self, io: &mut [f32], frames: usize) {
        if self.ring.is_empty() {
            if frames == 0 {
                return;
            }
            self.ring = vec![0.0; MAX_PDC * 2];
        }
        let len = self.ring.len();
        let d = frames.min(MAX_PDC - 1) * 2;
        for s in io.iter_mut() {
            self.ring[self.pos] = *s;
            *s = self.ring[(self.pos + len - d) % len];
            self.pos = (self.pos + 1) % len;
        }
    }
}

/// Most output pairs past the main one a plugin may have (64 channels in all).
pub const MAX_EXTRA_PAIRS: usize = 31;

pub type Shared = Arc<Mutex<Box<dyn Processor>>>;

/// Set while an offline render (export) runs: processors may then wait for
/// their work instead of keeping to the audio deadline (a bridged plugin
/// answers each block at once, without its live latency).
pub static OFFLINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn share(p: impl Processor + 'static) -> Shared {
    Arc::new(Mutex::new(Box::new(p)))
}

/// Notes in flight and the event list of one instance, sized once (the audio
/// thread never grows them).
pub struct Instance {
    pub id: u64,
    pub proc: Shared,
    pub(crate) events: Vec<PluginEvent>,
    /// Sounding notes: (note id, pitch, engine frame of the note-off; MAX = held live).
    pub(crate) notes: Vec<(u64, u8, u64)>,
    native_pitch: NativePitch,
    /// Latency compensation of its output (an instrument) or of the dry
    /// signal beside it (an effect with Mix below 100%).
    pub(crate) pdc: Pdc,
    /// The same for each extra output pair of a multi-out instrument.
    pub(crate) pdc_extra: Vec<Pdc>,
    /// Peak of its last block (the channel rack's tail light).
    pub(crate) level: f32,
    /// Channel volume/pan gains reached at the end of the last block
    /// (smoothing); `None` starts at the next block's target.
    pub(crate) gain_state: Option<[f64; 2]>,
}

pub const MAX_EVENTS: usize = 1024;
pub const MAX_NOTES: usize = 256;

// Reserved internally for the FL bridge's channel pitch updates.
pub const FL_PITCH: u8 = 0xf4;
const FL_PITCH_CENTER: i32 = 8192;

pub(crate) struct PitchSlide {
    pub start: u64,
    pub end: u64,
    pub cents: f64,
}

struct NativePitch {
    pending: Vec<PitchSlide>,
    active: Option<(PitchSlide, f64)>,
    next: Option<u64>,
    off: u64,
    cents: f64,
    /// The last pitch sent, `None` when the plugin's pitch is unknown: an
    /// export renders through another `Instance` on the same FL process, so
    /// a cached value may no longer be what FL plays.
    sent: Option<i32>,
}

impl Instance {
    pub fn new(id: u64, proc: Shared) -> Self {
        Self {
            id,
            proc,
            events: Vec::with_capacity(MAX_EVENTS),
            notes: Vec::with_capacity(MAX_NOTES),
            native_pitch: NativePitch {
                pending: Vec::with_capacity(128),
                active: None,
                next: None,
                off: 0,
                cents: 0.0,
                sent: None,
            },
            pdc: Pdc::default(),
            pdc_extra: (0..MAX_EXTRA_PAIRS).map(|_| Pdc::default()).collect(),
            level: 0.0,
            gain_state: None,
        }
    }

    pub(crate) fn push(&mut self, e: PluginEvent) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(e);
        }
    }

    pub(crate) fn start_fl_pitch(
        &mut self,
        block_clock: u64,
        on: u64,
        off: u64,
        cents: f64,
        interval: u64,
        slides: impl Iterator<Item = PitchSlide>,
    ) {
        self.flush_fl_pitch(block_clock, on, interval);
        let pitch = &mut self.native_pitch;
        pitch.pending.clear();
        pitch.active = None;
        // Every note states its pitch: the channel may have been left bent
        // by an export or by another engine on the same FL process.
        pitch.sent = None;
        pitch.off = off;
        pitch.cents = cents;
        let limit = pitch.pending.capacity();
        pitch.pending.extend(slides.take(limit));
        pitch
            .pending
            .sort_unstable_by_key(|slide| std::cmp::Reverse(slide.start));
        pitch.next = pitch.pending.last().map(|slide| slide.start);
        if let Some(event) = pitch.event(on.saturating_sub(block_clock) as u32) {
            self.push(event);
        }
    }

    pub(crate) fn flush_fl_pitch(&mut self, block_clock: u64, until: u64, interval: u64) {
        while let Some(at) = self
            .native_pitch
            .next
            .filter(|&t| t < until.min(self.native_pitch.off))
        {
            let pitch = &mut self.native_pitch;
            if let Some((slide, from)) = &pitch.active {
                let progress = (at - slide.start) as f64 / (slide.end - slide.start).max(1) as f64;
                pitch.cents = from + (slide.cents - from) * progress.min(1.0);
                if at >= slide.end {
                    pitch.active = None;
                }
            }
            while pitch.pending.last().is_some_and(|s| s.start <= at) {
                pitch.active = pitch.pending.pop().map(|slide| (slide, pitch.cents));
            }
            pitch.next = pitch.pending.last().map(|s| s.start);
            if let Some((slide, _)) = &pitch.active {
                let next = (at + interval).min(slide.end.max(at + 1));
                pitch.next = Some(pitch.next.map_or(next, |start| start.min(next)));
            }
            if let Some(event) = pitch.event(at.saturating_sub(block_clock) as u32) {
                self.push(event);
            }
        }
    }

    pub(crate) fn stop_fl_pitch(&mut self) {
        let pitch = &mut self.native_pitch;
        pitch.next = None;
        pitch.active = None;
        pitch.pending.clear();
        pitch.cents = 0.0;
        pitch.sent = None;
        if let Some(event) = pitch.event(0) {
            self.push(event);
        }
    }
}

impl NativePitch {
    fn event(&mut self, at: u32) -> Option<PluginEvent> {
        let cents = (self.cents.round() as i32).clamp(-FL_PITCH_CENTER, FL_PITCH_CENTER - 1);
        if self.sent == Some(cents) {
            return None;
        }
        self.sent = Some(cents);
        let value = cents + FL_PITCH_CENTER;
        Some(PluginEvent::Midi {
            at,
            bytes: [FL_PITCH, (value & 127) as u8, (value >> 7) as u8],
        })
    }
}

/// A plugin that plays a sine per held note (tests and the engine's own checks).
pub struct TestSynth {
    phases: [Option<(f64, f32)>; 128],
    pub seen: Arc<Mutex<Vec<PluginEvent>>>,
}

impl Default for TestSynth {
    fn default() -> Self {
        Self {
            phases: [None; 128],
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Processor for TestSynth {
    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], t: &Transport) {
        if let Ok(mut s) = self.seen.lock() {
            s.extend_from_slice(events);
        }
        let frames = io.len() / 2;
        let mut ev = events.iter().peekable();
        for i in 0..frames {
            while let Some(e) = ev.next_if(|e| e.at() as usize <= i) {
                match *e {
                    PluginEvent::NoteOn {
                        pitch, velocity, ..
                    } => self.phases[pitch as usize] = Some((0.0, velocity as f32 / 127.0 * 0.25)),
                    PluginEvent::NoteOff { pitch, .. } => self.phases[pitch as usize] = None,
                    PluginEvent::Midi { .. } => {}
                }
            }
            let mut s = 0.0f32;
            for (k, p) in self.phases.iter_mut().enumerate() {
                if let Some((ph, amp)) = p {
                    s += (*ph * std::f64::consts::TAU).sin() as f32 * *amp;
                    *ph = (*ph + 440.0 * 2f64.powf((k as f64 - 69.0) / 12.0) / t.rate as f64) % 1.0;
                }
            }
            io[i * 2] += s;
            io[i * 2 + 1] += s;
        }
    }
}

/// An effect that scales its input (tests).
pub struct TestGain(pub f32);

impl Processor for TestGain {
    fn process(&mut self, _: &[PluginEvent], io: &mut [f32], _: &Transport) {
        for s in io.iter_mut() {
            *s *= self.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pitches(i: &Instance) -> Vec<i32> {
        i.events
            .iter()
            .filter_map(|e| match e {
                PluginEvent::Midi { bytes, .. } if bytes[0] == FL_PITCH => {
                    Some(i32::from(bytes[1]) | (i32::from(bytes[2]) << 7))
                }
                _ => None,
            })
            .map(|v| v - FL_PITCH_CENTER)
            .collect()
    }

    #[test]
    fn every_fl_note_states_its_pitch_even_after_another_instance_bent_it() {
        let proc = share(TestSynth::default());
        // An export's instance leaves the shared FL channel bent by +300 cents.
        let mut export = Instance::new(1, proc.clone());
        export.start_fl_pitch(0, 0, 1000, 300.0, 10, std::iter::empty());
        assert_eq!(pitches(&export), vec![300]);
        // The live instance plays an unbent note: it must say 0, although it
        // never sent anything else.
        let mut live = Instance::new(1, proc);
        live.start_fl_pitch(0, 0, 1000, 0.0, 10, std::iter::empty());
        assert_eq!(pitches(&live), vec![0]);
        // And again on its next note, in case the export ran in between.
        live.events.clear();
        live.start_fl_pitch(1000, 1000, 2000, 0.0, 10, std::iter::empty());
        assert_eq!(pitches(&live), vec![0]);
    }
}
