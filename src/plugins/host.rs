//! Plugin audio processors (`Vst3Proc`, `Vst2Proc`, `ClapProc`), unified
//! `Handle`, and the host control thread (`start_host`).

use super::{MAX_BLOCK, NativeWindow, format_of};
use crate::clap::ClapPlugin;
use crate::vst2::Vst2Plugin;
use keyframexl::plugin::{PluginEvent, Processor, Shared, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use vst3_host::{
    Plugin, Vst3Host,
    audio::BusAudioBuffers,
    midi::{MidiChannel, MidiEvent},
};

#[cfg(target_os = "windows")]
use super::editor_win32::{Editors, Embedded};
#[cfg(target_os = "linux")]
use super::editor_x11::{Editors, Embedded};

// ------------------------------------------------------------- processing

/// The audio thread's side of one loaded VST3 plugin, bus by bus: input bus
/// 0 is the effect's signal, input bus 1 its sidechain; output bus 0 is the
/// main pair, the aux output buses become extra pairs for the mixer.
struct Vst3Proc {
    plugin: Arc<Mutex<Plugin>>,
    buf: BusAudioBuffers,
    effect: bool,
    rate: u32,
    /// Extra stereo pairs, in bus order: (output bus, its left channel, its
    /// right channel), mono buses giving one channel twice.
    pairs: Vec<(usize, usize, usize)>,
    /// The input bus the sidechain key goes to, when the plugin has one.
    key_bus: Option<usize>,
    tempo: f64,
    playing: bool,
    /// Its reported latency, read while processing.
    delay: usize,
}

/// Stereo pairs of `buses` past the main one, at most `MAX_EXTRA_PAIRS`.
fn bus_pairs(outputs: &[vst3_host::audio::AudioBusConfig]) -> Vec<(usize, usize, usize)> {
    let mut pairs = Vec::new();
    for (b, bus) in outputs
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, b)| b.active && b.channel_count > 0)
    {
        for c in (0..bus.channel_count).step_by(2) {
            pairs.push((b, c, (c + 1).min(bus.channel_count - 1)));
        }
    }
    pairs.truncate(keyframexl::plugin::MAX_EXTRA_PAIRS);
    pairs
}

impl Vst3Proc {
    fn new(plugin: Arc<Mutex<Plugin>>, effect: bool, rate: f64) -> Self {
        let layout = plugin.lock().ok().and_then(|p| p.audio_bus_layout().ok());
        let layout = layout.unwrap_or_else(|| {
            let stereo = vst3_host::audio::AudioBusConfig {
                channel_count: 2,
                active: true,
            };
            vst3_host::audio::AudioBusLayout {
                inputs: if effect { vec![stereo] } else { vec![] },
                outputs: vec![stereo],
            }
        });
        let pairs = if effect {
            vec![]
        } else {
            bus_pairs(&layout.outputs)
        };
        let key_bus = (effect
            && layout
                .inputs
                .get(1)
                .is_some_and(|b| b.active && b.channel_count > 0))
        .then_some(1);
        // Channels are allocated at MAX_BLOCK; `process` only resizes within that.
        let buf = BusAudioBuffers::new(&layout, MAX_BLOCK, rate);
        Self {
            plugin,
            buf,
            effect,
            rate: rate.round() as u32,
            pairs,
            key_bus,
            tempo: 0.0,
            playing: false,
            delay: 0,
        }
    }

    fn run(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: Option<&[f32]>,
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        let pairs = if stride == 0 || extra.len() < io.len() {
            0
        } else {
            self.pairs.len().min((extra.len() - io.len()) / stride + 1)
        };
        let offline = keyframexl::plugin::OFFLINE.load(std::sync::atomic::Ordering::Acquire);
        let Ok(mut plugin) = (if offline {
            self.plugin.lock().ok()
        } else {
            self.plugin.try_lock().ok()
        })
        .ok_or(()) else {
            if !self.effect {
                io.fill(0.0);
            }
            return 0;
        };
        self.delay = plugin.latency_samples() as usize;
        if t.tempo != self.tempo {
            self.tempo = t.tempo;
            let _ = plugin.set_tempo(t.tempo);
        }
        if t.playing != self.playing {
            self.playing = t.playing;
            let _ = plugin.set_playing(t.playing);
        }
        let frames = io.len() / 2;
        let mut done = 0;
        let mut ev = 0;
        while done < frames {
            let n = (frames - done).min(MAX_BLOCK);
            for bus in self
                .buf
                .inputs
                .iter_mut()
                .chain(self.buf.outputs.iter_mut())
            {
                for c in &mut bus.channels {
                    // Within the capacity reserved at load: no allocation.
                    c.resize(n, 0.0);
                }
            }
            while ev < events.len() && (events[ev].at() as usize) < done + n {
                let at = events[ev].at() as i32 - done as i32;
                let _ = plugin.send_midi_event_at(to_midi(&events[ev]), at.max(0));
                ev += 1;
            }
            let span = done * 2..(done + n) * 2;
            if self.effect
                && let Some(main) = self.buf.inputs.first_mut()
            {
                fill_bus(&mut main.channels, &io[span.clone()]);
            }
            if let Some(b) = self.key_bus {
                let bus = &mut self.buf.inputs[b].channels;
                match key {
                    Some(key) => fill_bus(bus, &key[span.clone()]),
                    None => bus.iter_mut().for_each(|c| c.fill(0.0)),
                }
            }
            if plugin.process_bus_audio(&mut self.buf).is_ok() {
                if let Some(main) = self.buf.outputs.first() {
                    read_pair(&main.channels, 0, 1, &mut io[span.clone()]);
                }
                for (k, &(b, l, r)) in self.pairs.iter().take(pairs).enumerate() {
                    read_pair(
                        &self.buf.outputs[b].channels,
                        l,
                        r,
                        &mut extra[k * stride + span.start..k * stride + span.end],
                    );
                }
            }
            done += n;
        }
        pairs
    }
}

/// Interleaved stereo `src` into a bus: L and R, or their sum on a mono bus.
fn fill_bus(bus: &mut [Vec<f32>], src: &[f32]) {
    match bus {
        [] => {}
        [mono] => mono
            .iter_mut()
            .zip(src.chunks_exact(2))
            .for_each(|(m, f)| *m = (f[0] + f[1]) * 0.5),
        [l, r, rest @ ..] => {
            for (i, f) in src.as_chunks::<2>().0.iter().enumerate() {
                l[i] = f[0];
                r[i] = f[1];
            }
            rest.iter_mut().for_each(|c| c.fill(0.0));
        }
    }
}

/// Channels `l` and `r` of a bus into interleaved stereo `dst` (silence
/// when the bus has none).
fn read_pair(bus: &[Vec<f32>], l: usize, r: usize, dst: &mut [f32]) {
    let (Some(lc), Some(rc)) = (bus.get(l), bus.get(r).or(bus.get(l))) else {
        dst.fill(0.0);
        return;
    };
    for (i, f) in dst.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        f[0] = lc[i];
        f[1] = rc[i];
    }
}

fn to_midi(e: &PluginEvent) -> MidiEvent {
    let ch = MidiChannel::Ch1;
    match *e {
        PluginEvent::NoteOn {
            pitch, velocity, ..
        } => MidiEvent::NoteOn {
            channel: ch,
            note: pitch,
            velocity,
        },
        PluginEvent::NoteOff { pitch, .. } => MidiEvent::NoteOff {
            channel: ch,
            note: pitch,
            velocity: 0,
        },
        PluginEvent::Midi { bytes, .. } => match bytes[0] & 0xF0 {
            0xE0 => MidiEvent::PitchBend {
                channel: ch,
                value: (bytes[2] as u16) << 7 | bytes[1] as u16,
            },
            0xC0 => MidiEvent::ProgramChange {
                channel: ch,
                program: bytes[1],
            },
            0xD0 => MidiEvent::ChannelAftertouch {
                channel: ch,
                pressure: bytes[1],
            },
            _ => MidiEvent::ControlChange {
                channel: ch,
                controller: bytes[1],
                value: bytes[2],
            },
        },
    }
}

/// Flush active voices, held MIDI notes, sustain pedal, pitch bend and
/// residual delay/reverb tails before an offline render begins.
fn flush_processor(proc: &mut dyn Processor, rate: u32) {
    let reset = keyframexl::plugin::reset_events();
    let mut buf = [0.0f32; 512 * 2];
    let t = Transport {
        rate: rate.max(8_000),
        tempo: 120.0,
        playing: false,
        frame: 0,
    };
    proc.process(&reset, &mut buf, &t);
    // Drain any residual reverb/release/buffer tail (up to ~250 ms) until silent.
    let mut silent_blocks = 0u32;
    for _ in 0..24 {
        buf.fill(0.0);
        proc.process(&[], &mut buf, &t);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak < 1e-5 {
            silent_blocks += 1;
            if silent_blocks >= 2 {
                break;
            }
        } else {
            silent_blocks = 0;
        }
    }
}

impl Processor for Vst3Proc {
    fn prepare_offline(&mut self) {
        if let Ok(mut plugin) = self.plugin.lock() {
            let _ = plugin.stop_processing();
            let _ = plugin.set_process_mode(vst3_host::ProcessMode::Offline);
            let _ = plugin.set_playing(false);
            let _ = plugin.set_transport_position(vst3_host::TransportPosition {
                samples: 0,
                quarter_notes: 0.0,
            });
            let _ = plugin.start_processing();
        }
        self.playing = false;
        self.tempo = 0.0;
        let rate = self.rate;
        flush_processor(self, rate);
        if let Ok(mut plugin) = self.plugin.lock() {
            let _ = plugin.set_transport_position(vst3_host::TransportPosition {
                samples: 0,
                quarter_notes: 0.0,
            });
        }
        self.playing = false;
        self.tempo = 0.0;
    }

    fn finish_offline(&mut self) {
        if let Ok(mut plugin) = self.plugin.lock() {
            let _ = plugin.stop_processing();
            let _ = plugin.set_process_mode(vst3_host::ProcessMode::Realtime);
            let _ = plugin.set_playing(false);
            let _ = plugin.start_processing();
        }
        self.playing = false;
        self.tempo = 0.0;
    }

    fn latency(&self, _frames: usize) -> usize {
        self.delay
    }

    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], t: &Transport) {
        self.run(events, io, None, &mut [], 0, t);
    }

    fn process_keyed(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: &[f32],
        t: &Transport,
    ) {
        self.run(events, io, Some(key), &mut [], 0, t);
    }

    fn extra_pairs(&self) -> usize {
        self.pairs.len()
    }

    fn process_multi(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        self.run(events, io, None, extra, stride, t)
    }
}

/// The audio thread's side of one VST2 plugin: the first two outputs are the
/// stereo bus; a multi-out instrument gives the rest as extra pairs for the mixer.
struct Vst2Proc {
    plugin: Arc<Mutex<Vst2Plugin>>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    silence: Vec<f32>,
    scratch: Vec<f32>,
    /// Output pairs past the main one (instruments only).
    extra: usize,
    /// Its reported latency, read while processing (the lock is ours then).
    delay: usize,
}

impl Vst2Proc {
    fn new(plugin: Arc<Mutex<Vst2Plugin>>, effect: bool) -> Self {
        let channel = || vec![0.0f32; MAX_BLOCK];
        let (ins, outs) = plugin
            .lock()
            .map(|p| (p.info().1, p.info().2))
            .unwrap_or((2, 2));
        let extra = if effect {
            0
        } else {
            outs.saturating_sub(1) / 2
        }
        .min(keyframexl::plugin::MAX_EXTRA_PAIRS);
        let outputs = (0..2 + extra * 2).map(|_| channel()).collect();
        // An effect with four inputs takes its sidechain on 3-4 (the VST2 custom).
        let inputs = if !effect {
            0
        } else if ins >= 4 {
            4
        } else {
            2
        };
        Self {
            plugin,
            inputs: (0..inputs).map(|_| channel()).collect(),
            outputs,
            silence: channel(),
            scratch: channel(),
            extra,
            delay: 0,
        }
    }
}

fn midi_bytes(e: &PluginEvent) -> [u8; 3] {
    match *e {
        PluginEvent::NoteOn {
            pitch, velocity, ..
        } => [0x90, pitch, velocity.max(1)],
        PluginEvent::NoteOff { pitch, .. } => [0x80, pitch, 0],
        PluginEvent::Midi { bytes, .. } => [bytes[0], bytes[1], bytes[2]],
    }
}

impl Processor for Vst2Proc {
    fn prepare_offline(&mut self) {
        if let Ok(mut plugin) = self.plugin.lock() {
            plugin.stop();
            plugin.start();
        }
        flush_processor(self, 48_000);
    }

    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], t: &Transport) {
        self.run(events, io, None, &mut [], 0, t);
    }

    fn process_keyed(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: &[f32],
        t: &Transport,
    ) {
        self.run(events, io, Some(key), &mut [], 0, t);
    }

    fn extra_pairs(&self) -> usize {
        self.extra
    }

    fn latency(&self, _frames: usize) -> usize {
        self.delay
    }

    fn process_multi(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        self.run(events, io, None, extra, stride, t)
    }
}

impl Vst2Proc {
    fn run(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: Option<&[f32]>,
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        let pairs = extra
            .len()
            .checked_div(stride)
            .map_or(0, |n| self.extra.min(n));
        let offline = keyframexl::plugin::OFFLINE.load(std::sync::atomic::Ordering::Acquire);
        let Ok(mut plugin) = (if offline {
            self.plugin.lock().ok()
        } else {
            self.plugin.try_lock().ok()
        })
        .ok_or(()) else {
            io.fill(0.0);
            return 0;
        };
        plugin.set_time(t.tempo, t.playing);
        self.delay = plugin.initial_delay();
        let frames = io.len() / 2;
        let mut done = 0;
        let mut ev = 0;
        while done < frames {
            let n = (frames - done).min(MAX_BLOCK);
            while ev < events.len() && (events[ev].at() as usize) < done + n {
                plugin.queue_midi(
                    midi_bytes(&events[ev]),
                    events[ev].at() as i32 - done as i32,
                );
                ev += 1;
            }
            plugin.flush_midi();
            if !self.inputs.is_empty() {
                for (i, f) in io[done * 2..(done + n) * 2]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .enumerate()
                {
                    self.inputs[0][i] = f[0];
                    self.inputs[1][i] = f[1];
                }
            }
            if self.inputs.len() == 4 {
                match key {
                    Some(key) => {
                        for (i, f) in key[done * 2..(done + n) * 2]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .enumerate()
                        {
                            self.inputs[2][i] = f[0];
                            self.inputs[3][i] = f[1];
                        }
                    }
                    None => self.inputs[2..].iter_mut().for_each(|c| c.fill(0.0)),
                }
            }
            if plugin.process_into(
                &mut self.inputs,
                &mut self.outputs,
                &mut self.silence,
                &mut self.scratch,
                n,
            ) {
                let mono = plugin.info().2 < 2;
                for (i, f) in io[done * 2..(done + n) * 2]
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .enumerate()
                {
                    f[0] = self.outputs[0][i];
                    f[1] = if mono {
                        self.outputs[0][i]
                    } else {
                        self.outputs[1][i]
                    };
                }
                for k in 0..pairs {
                    let (l, r) = (&self.outputs[2 + 2 * k], &self.outputs[3 + 2 * k]);
                    let lane = &mut extra[k * stride + done * 2..k * stride + (done + n) * 2];
                    for (i, f) in lane.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                        f[0] = l[i];
                        f[1] = r[i];
                    }
                }
            }
            done += n;
        }
        pairs
    }
}

/// The audio thread's side of one CLAP plugin: bus 0 is the main stereo I/O,
/// input bus 1 is the effect's sidechain key, and aux output buses become
/// extra stereo pairs for multi-out instruments.
struct ClapProc {
    plugin: Arc<Mutex<ClapPlugin>>,
    inputs: Vec<Vec<Vec<f32>>>,
    outputs: Vec<Vec<Vec<f32>>>,
    effect: bool,
    /// Extra stereo pairs past bus 0: (output bus index, left ch, right ch).
    pairs: Vec<(usize, usize, usize)>,
    /// Input bus index for sidechain key, when the plugin has one.
    key_bus: Option<usize>,
    /// Reported latency in samples, refreshed during `run`.
    delay: usize,
}

impl ClapProc {
    fn new(plugin: Arc<Mutex<ClapPlugin>>, effect: bool) -> Self {
        let layout =
            plugin
                .lock()
                .map(|p| p.bus_layout().clone())
                .unwrap_or(crate::clap::ClapBusLayout {
                    inputs: if effect { vec![2] } else { vec![] },
                    outputs: vec![2],
                });
        let mut pairs = Vec::new();
        if !effect {
            for (b, &ch) in layout.outputs.iter().enumerate().skip(1) {
                if ch == 0 {
                    continue;
                }
                for c in (0..ch).step_by(2) {
                    pairs.push((b, c, (c + 1).min(ch - 1)));
                }
            }
            pairs.truncate(keyframexl::plugin::MAX_EXTRA_PAIRS);
        }
        let key_bus = (effect && layout.inputs.get(1).is_some_and(|&ch| ch > 0)).then_some(1);
        let inputs = layout
            .inputs
            .iter()
            .map(|&ch| vec![vec![0.0f32; MAX_BLOCK]; ch])
            .collect();
        let outputs = layout
            .outputs
            .iter()
            .map(|&ch| vec![vec![0.0f32; MAX_BLOCK]; ch])
            .collect();
        Self {
            plugin,
            inputs,
            outputs,
            effect,
            pairs,
            key_bus,
            delay: 0,
        }
    }

    fn run(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: Option<&[f32]>,
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        let pairs = if stride == 0 || extra.len() < io.len() {
            0
        } else {
            self.pairs.len().min((extra.len() - io.len()) / stride + 1)
        };
        let offline = keyframexl::plugin::OFFLINE.load(std::sync::atomic::Ordering::Acquire);
        let Ok(mut plugin) = (if offline {
            self.plugin.lock().ok()
        } else {
            self.plugin.try_lock().ok()
        })
        .ok_or(()) else {
            if !self.effect {
                io.fill(0.0);
            }
            return 0;
        };
        self.delay = plugin.latency_samples();
        let frames = io.len() / 2;
        let mut done = 0;
        let mut ev = 0;
        while done < frames {
            let n = (frames - done).min(MAX_BLOCK);
            while ev < events.len() && (events[ev].at() as usize) < done + n {
                let at = (events[ev].at() as i32 - done as i32).max(0) as u32;
                match events[ev] {
                    PluginEvent::NoteOn {
                        pitch, velocity, ..
                    } => plugin.queue_note_on(at, pitch, velocity),
                    PluginEvent::NoteOff { pitch, .. } => plugin.queue_note_off(at, pitch),
                    PluginEvent::Midi { bytes, .. } => plugin.queue_midi(at, bytes),
                }
                ev += 1;
            }
            let span = done * 2..(done + n) * 2;
            if self.effect
                && let Some(main) = self.inputs.first_mut()
            {
                fill_bus(main, &io[span.clone()]);
            }
            if let Some(b) = self.key_bus
                && let Some(bus) = self.inputs.get_mut(b)
            {
                match key {
                    Some(key) => fill_bus(bus, &key[span.clone()]),
                    None => bus.iter_mut().for_each(|c| c[..n].fill(0.0)),
                }
            }
            if plugin.process(
                &mut self.inputs,
                &mut self.outputs,
                n,
                t.rate,
                t.tempo,
                t.playing,
                t.frame + done as u64,
            ) {
                if let Some(main) = self.outputs.first() {
                    read_pair(main, 0, 1, &mut io[span.clone()]);
                }
                for (k, &(b, l, r)) in self.pairs.iter().take(pairs).enumerate() {
                    if let Some(bus) = self.outputs.get(b) {
                        read_pair(
                            bus,
                            l,
                            r,
                            &mut extra[k * stride + span.start..k * stride + span.end],
                        );
                    }
                }
            }
            done += n;
        }
        pairs
    }
}

impl Processor for ClapProc {
    fn prepare_offline(&mut self) {
        if let Ok(mut plugin) = self.plugin.lock() {
            plugin.stop_processing();
            let _ = plugin.start_processing();
        }
        flush_processor(self, 48_000);
    }

    fn latency(&self, _frames: usize) -> usize {
        self.delay
    }

    fn process(&mut self, events: &[PluginEvent], io: &mut [f32], t: &Transport) {
        self.run(events, io, None, &mut [], 0, t);
    }

    fn process_keyed(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        key: &[f32],
        t: &Transport,
    ) {
        self.run(events, io, Some(key), &mut [], 0, t);
    }

    fn extra_pairs(&self) -> usize {
        self.pairs.len()
    }

    fn process_multi(
        &mut self,
        events: &[PluginEvent],
        io: &mut [f32],
        extra: &mut [f32],
        stride: usize,
        t: &Transport,
    ) -> usize {
        self.run(events, io, None, extra, stride, t)
    }
}

/// A loaded plugin of any supported format.
#[derive(Clone)]
pub(super) enum Handle {
    V3(Arc<Mutex<Plugin>>),
    V2(Arc<Mutex<Vst2Plugin>>),
    Clap(Arc<Mutex<ClapPlugin>>),
    /// FL Studio live link (Linux/Wine): no editor, parameters or state of its own.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Fl,
}

/// VST2 state as we store it: the plugin's chunk, or (plugins without one)
/// every parameter as f32.
const V2_CHUNK: &[u8] = b"KFV2C";
const V2_PARAMS: &[u8] = b"KFV2P";

impl Handle {
    fn params(&self) -> Vec<ParamRow> {
        match self {
            Handle::V3(p) => p.lock().map(|p| params(&p)).unwrap_or_default(),
            Handle::V2(p) => {
                let Ok(mut p) = p.lock() else { return vec![] };
                let n = p.info().3;
                (0..n as i32)
                    .filter_map(|i| {
                        let name = p.parameter_name(i);
                        let value = p.parameter(i)? as f64;
                        (!name.trim().is_empty()).then(|| ParamRow {
                            id: i as u32,
                            text: p.parameter_display(i),
                            name,
                            value,
                            automatable: true,
                        })
                    })
                    .collect()
            }
            Handle::Clap(p) => p
                .lock()
                .map(|p| {
                    p.params()
                        .into_iter()
                        .map(|(id, name, value, text, automatable)| ParamRow {
                            id,
                            name,
                            value,
                            text,
                            automatable,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Handle::Fl => vec![],
        }
    }

    fn set_param(&self, id: u32, value: f64) {
        match self {
            Handle::V3(p) => {
                if let Ok(mut p) = p.lock() {
                    let _ = p.set_parameter(id, value.clamp(0.0, 1.0));
                }
            }
            Handle::V2(p) => {
                if let Ok(mut p) = p.lock() {
                    p.set_parameter(id as i32, value.clamp(0.0, 1.0) as f32);
                }
            }
            Handle::Clap(p) => {
                if let Ok(mut p) = p.lock() {
                    p.set_param_normalized(id, value.clamp(0.0, 1.0));
                }
            }
            Handle::Fl => {}
        }
    }

    fn save_state(&self) -> Option<Vec<u8>> {
        match self {
            Handle::V3(p) => p.lock().ok()?.save_state().ok(),
            Handle::V2(p) => {
                let mut p = p.lock().ok()?;
                if p.has_chunk() {
                    let chunk = p.chunk(false)?;
                    Some([V2_CHUNK, &chunk].concat())
                } else {
                    let n = p.info().3 as i32;
                    let mut out = V2_PARAMS.to_vec();
                    for i in 0..n {
                        out.extend_from_slice(&p.parameter(i).unwrap_or(0.0).to_le_bytes());
                    }
                    Some(out)
                }
            }
            Handle::Clap(p) => p.lock().ok()?.save_state(),
            Handle::Fl => Some(vec![]),
        }
    }

    fn load_state(&self, state: &[u8]) -> Result<(), String> {
        match self {
            Handle::V3(p) => p
                .lock()
                .map_err(|_| "busy".to_string())?
                .load_state(state)
                .map_err(|e| e.to_string()),
            Handle::V2(p) => {
                let mut p = p.lock().map_err(|_| "busy".to_string())?;
                v2_restore(&mut p, state)
            }
            Handle::Clap(p) => {
                let mut p = p.lock().map_err(|_| "busy".to_string())?;
                p.load_state(state)
            }
            Handle::Fl => Ok(()),
        }
    }

    pub(super) fn close_editor(&self) {
        match self {
            Handle::V3(p) => {
                if let Ok(mut p) = p.lock() {
                    let _ = p.close_editor();
                }
            }
            Handle::V2(p) => {
                if let Ok(mut p) = p.lock() {
                    p.editor_close();
                }
            }
            Handle::Clap(p) => {
                if let Ok(mut p) = p.lock() {
                    p.editor_close();
                }
            }
            Handle::Fl => {}
        }
    }

    fn stop(&self) {
        match self {
            Handle::V3(p) => {
                if let Ok(mut p) = p.lock() {
                    let _ = p.stop_processing();
                }
            }
            Handle::V2(p) => {
                if let Ok(mut p) = p.lock() {
                    p.stop();
                }
            }
            Handle::Clap(p) => {
                if let Ok(mut p) = p.lock() {
                    p.stop_processing();
                    p.deactivate();
                }
            }
            Handle::Fl => {}
        }
    }

    /// Editor upkeep (run loop / idle); a size the editor asked for.
    pub(super) fn service(&self) -> Option<(i32, i32)> {
        if keyframexl::plugin::OFFLINE.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        match self {
            Handle::V3(p) => {
                let mut p = p.try_lock().ok()?;
                p.service_run_loop();
                p.take_editor_resize_request()
            }
            Handle::V2(p) => {
                let mut p = p.try_lock().ok()?;
                p.editor_idle();
                p.take_editor_resize_request()
            }
            Handle::Clap(p) => {
                let mut p = p.try_lock().ok()?;
                p.service()
            }
            Handle::Fl => None,
        }
    }
}

fn v2_restore(p: &mut Vst2Plugin, state: &[u8]) -> Result<(), String> {
    if let Some(chunk) = state.strip_prefix(V2_CHUNK) {
        if !p.has_chunk() {
            return Err("the plugin does not take saved state".into());
        }
        p.set_chunk(false, chunk);
    } else if let Some(values) = state.strip_prefix(V2_PARAMS) {
        for (i, v) in values.as_chunks::<4>().0.iter().enumerate() {
            p.set_parameter(i as i32, f32::from_le_bytes([v[0], v[1], v[2], v[3]]));
        }
    } else if !state.is_empty() {
        return Err("not a VST2 state".into());
    }
    Ok(())
}

/// Load any supported format, restore `state`, start processing.
/// Returns `(handle, name, has_editor, audio_processor)`.
fn load_any(
    host: &mut Vst3Host,
    path: &Path,
    state: &[u8],
    rate: u32,
    effect: bool,
) -> Result<(Handle, String, bool, Shared), String> {
    #[cfg(target_os = "linux")]
    if crate::fl_link::is_link(path) {
        let link = crate::fl_link::FlLink::new(rate)?;
        return Ok((
            Handle::Fl,
            "FL Studio (live link)".into(),
            false,
            keyframexl::plugin::share(link),
        ));
    }
    #[cfg(not(target_os = "linux"))]
    if crate::fl_link::is_link(path) {
        return Err("FL Link is only used under Linux/Wine".into());
    }
    if format_of(path) == "clap" {
        let mut p = ClapPlugin::load(path)?;
        p.activate(rate as f64, MAX_BLOCK)?;
        if !state.is_empty()
            && let Err(e) = p.load_state(state)
        {
            eprintln!("{}: state not restored: {e}", path.display());
        }
        p.start_processing()?;
        let desc = p.desc();
        let name = if desc.name.trim().is_empty() {
            crate::clap::split_path(path)
                .0
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            desc.name.clone()
        };
        let gui = p.has_editor();
        let p = Arc::new(Mutex::new(p));
        let proc = keyframexl::plugin::share(ClapProc::new(p.clone(), effect));
        return Ok((Handle::Clap(p), name, gui, proc));
    }
    if format_of(path) == "vst2" {
        let mut p = Vst2Plugin::load(path)?;
        p.configure(rate as f32, MAX_BLOCK as i32);
        if !state.is_empty()
            && let Err(e) = v2_restore(&mut p, state)
        {
            eprintln!("{}: state not restored: {e}", path.display());
        }
        p.set_time(120.0, false);
        p.start();
        let name = Some(p.name())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let gui = p.has_editor();
        let p = Arc::new(Mutex::new(p));
        let proc = keyframexl::plugin::share(Vst2Proc::new(p.clone(), effect));
        return Ok((Handle::V2(p), name, gui, proc));
    }
    let plugin = load(host, path, state, rate, effect)?;
    let name = plugin.info().name.clone();
    let gui = plugin.has_editor();
    let plugin = Arc::new(Mutex::new(plugin));
    let proc = keyframexl::plugin::share(Vst3Proc::new(plugin.clone(), effect, rate as f64));
    Ok((Handle::V3(plugin), name, gui, proc))
}

// ------------------------------------------------------------- host thread

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Req {
    /// Load `path` as instance `id` with `state`; `effect` = audio input wanted.
    Load {
        id: u64,
        path: PathBuf,
        state: Vec<u8>,
        effect: bool,
    },
    Unload(u64),
    ShowEditor(u64),
    HideEditor(u64),
    /// Reply with every instance's state.
    SaveStates,
    /// New output rate: every plugin reloads with its current state.
    Rate(u32),
    /// Put the editor inside our own window `parent` (FL's wrapper window):
    /// `rect` = x, y, width, height of the whole editor in window pixels, `clip`
    /// = the base viewport. `regions` are the visible parts after subtracting
    /// overlapping UI, in the same window coordinates. Empty regions hide it.
    Embed {
        id: u64,
        parent: NativeWindow,
        rect: [i32; 4],
        clip: [i32; 4],
        regions: Vec<[i32; 4]>,
    },
    /// Reply with the plugin's parameters (MIDI-controller stand-ins left out).
    Params(u64),
    /// Set a parameter (normalized 0..1), as if turned in the editor.
    SetParam {
        id: u64,
        param: u32,
        value: f64,
    },
    /// Put a saved state back into a loaded plugin (undo / redo).
    SetState {
        id: u64,
        state: Vec<u8>,
    },
    /// Unload everything and end the thread (answered by `Reply::Bye`).
    Quit,
}

/// One plugin parameter for the Parameters page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamRow {
    pub id: u32,
    pub name: String,
    pub value: f64,
    pub text: String,
    pub automatable: bool,
}

pub enum Reply {
    /// `outs`: output pairs past the main one (multi-out instruments).
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
    /// The embedded editor's size in pixels (on open and when the plugin resizes).
    EditorSize {
        id: u64,
        w: i32,
        h: i32,
    },
    Params {
        id: u64,
        rows: Vec<ParamRow>,
    },
    /// The host thread has unloaded everything and ended (after `Req::Quit`).
    Bye,
}

pub(super) struct Loaded {
    pub(super) plugin: Handle,
    window: Option<vst3_host::window::PluginWindow>,
    pub(super) embed: Option<Embedded>,
    path: PathBuf,
    effect: bool,
}

fn params(plugin: &Plugin) -> Vec<ParamRow> {
    let Ok(list) = plugin.get_parameters() else {
        return vec![];
    };
    list.into_iter()
        // Plugins expose 2048 "MIDI CC n|ch" parameters for controller mapping: not settings.
        .filter(|p| {
            !p.name.starts_with("MIDI CC")
                && !p.name.starts_with("CC ")
                && !p.is_read_only
                && !p.name.trim().is_empty()
        })
        .map(|p| ParamRow {
            id: p.id,
            text: plugin.format_parameter(p.id, p.value).unwrap_or_default(),
            name: p.name,
            value: p.value,
            automatable: p.can_automate,
        })
        .collect()
}

pub struct HostHandle {
    tx: mpsc::Sender<Req>,
    pub rx: mpsc::Receiver<Reply>,
}

impl HostHandle {
    #[allow(dead_code)]
    pub fn new(tx: mpsc::Sender<Req>, rx: mpsc::Receiver<Reply>) -> Self {
        Self { tx, rx }
    }

    pub fn send(&self, r: Req) {
        let _ = self.tx.send(r);
    }

    #[allow(dead_code)]
    pub fn sender(&self) -> mpsc::Sender<Req> {
        self.tx.clone()
    }
}

pub fn start_host(rate: u32, repaint: Option<eframe::egui::Context>) -> HostHandle {
    let (tx, rx) = mpsc::channel::<Req>();
    let (rtx, rrx) = mpsc::channel::<Reply>();
    std::thread::Builder::new()
        .name("plugin-host".into())
        .spawn(move || host_thread(rate, rx, rtx, repaint))
        .expect("plugin host thread");
    HostHandle { tx, rx: rrx }
}

fn load(
    host: &mut Vst3Host,
    path: &Path,
    state: &[u8],
    rate: u32,
    effect: bool,
) -> Result<Plugin, String> {
    let mut plugin = host.load_plugin(path).map_err(|e| e.to_string())?;
    activate_buses(&mut plugin, effect);
    if !state.is_empty()
        && let Err(e) = plugin.load_state(state)
    {
        eprintln!("{}: state not restored: {e}", path.display());
    }
    let _ = plugin.set_tempo(120.0);
    let _ = rate;
    plugin.start_processing().map_err(|e| e.to_string())?;
    Ok(plugin)
}

/// Turn on what plugins leave off by default: an effect's sidechain input
/// (input bus 1), an instrument's aux outputs (as many as the mixer takes).
fn activate_buses(plugin: &mut Plugin, effect: bool) {
    use vst3_host::audio::{BusDirection, MediaType};
    let Ok(layout) = plugin.audio_bus_layout() else {
        return;
    };
    if effect {
        if layout
            .inputs
            .get(1)
            .is_some_and(|b| !b.active && b.channel_count > 0)
        {
            let _ = plugin.set_bus_active(MediaType::Audio, BusDirection::Input, 1, true);
        }
        return;
    }
    let mut room = keyframexl::plugin::MAX_EXTRA_PAIRS;
    for (i, bus) in layout.outputs.iter().enumerate().skip(1) {
        let pairs = bus.channel_count.div_ceil(2);
        if room < pairs.max(1) {
            break;
        }
        if !bus.active
            && bus.channel_count > 0
            && plugin
                .set_bus_active(MediaType::Audio, BusDirection::Output, i as i32, true)
                .is_err()
        {
            continue;
        }
        room -= pairs;
    }
}

fn unembed(editors: &mut Editors, l: &mut Loaded) {
    if let Some(e) = l.embed.take() {
        editors.unembed(&l.plugin, e);
    }
}

/// Editor closed, processing stopped, instance released.
fn close(editors: &mut Editors, mut l: Loaded) {
    unembed(editors, &mut l);
    if let Some(w) = &mut l.window {
        w.close();
    }
    l.plugin.stop();
}

fn host_thread(
    mut rate: u32,
    rx: mpsc::Receiver<Req>,
    tx: mpsc::Sender<Reply>,
    repaint: Option<eframe::egui::Context>,
) {
    let build = |rate: u32| {
        Vst3Host::builder()
            .sample_rate(rate as f64)
            .block_size(MAX_BLOCK)
            .build()
    };
    let mut host = match build(rate) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Plugin host unavailable: {e}");
            return;
        }
    };
    let mut loaded: HashMap<u64, Loaded> = HashMap::new();
    let mut editors = Editors::new();
    let poke = || {
        if let Some(c) = &repaint {
            c.request_repaint();
        }
    };
    loop {
        let wait = if loaded
            .values()
            .any(|l| l.window.is_some() || l.embed.is_some())
        {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        };
        match rx.recv_timeout(wait) {
            Ok(Req::Load {
                id,
                path,
                state,
                effect,
            }) => {
                if let Some(old) = loaded.remove(&id) {
                    close(&mut editors, old);
                }
                match load_any(&mut host, &path, &state, rate, effect) {
                    Ok((plugin, name, gui, proc)) => {
                        loaded.insert(
                            id,
                            Loaded {
                                plugin,
                                window: None,
                                embed: None,
                                path,
                                effect,
                            },
                        );
                        let outs = proc.lock().map(|p| p.extra_pairs()).unwrap_or(0);
                        let _ = tx.send(Reply::Loaded {
                            id,
                            proc,
                            name,
                            gui,
                            outs,
                        });
                    }
                    Err(error) => {
                        let _ = tx.send(Reply::Failed { id, error });
                    }
                }
                poke();
            }
            Ok(Req::Embed {
                id,
                parent,
                rect,
                clip,
                regions,
            }) => {
                let visible = !regions.is_empty();
                let Some(l) = loaded.get_mut(&id) else {
                    continue;
                };
                match &mut l.embed {
                    Some(e) => {
                        editors.update(e, rect, clip, &regions);
                    }
                    None if visible => {
                        if let Some(mut w) = l.window.take() {
                            w.close();
                        }
                        match editors.embed(&l.plugin, parent, rect, clip, &regions) {
                            Ok((emb, w, h)) => {
                                l.embed = Some(emb);
                                let _ = tx.send(Reply::EditorSize { id, w, h });
                                let _ = tx.send(Reply::Editor { id, open: true });
                            }
                            Err(error) => {
                                let _ = tx.send(Reply::Failed { id, error });
                            }
                        }
                        poke();
                    }
                    None => {}
                }
            }
            Ok(Req::Params(id)) => {
                if let Some(l) = loaded.get(&id) {
                    let _ = tx.send(Reply::Params {
                        id,
                        rows: l.plugin.params(),
                    });
                    poke();
                }
            }
            Ok(Req::SetParam { id, param, value }) => {
                if let Some(l) = loaded.get(&id) {
                    l.plugin.set_param(param, value);
                }
            }
            Ok(Req::SetState { id, state }) => {
                if let Some(l) = loaded.get(&id)
                    && let Err(e) = l.plugin.load_state(&state)
                {
                    eprintln!("plugin {id}: state not restored: {e}");
                }
            }
            Ok(Req::Unload(id)) => {
                if let Some(l) = loaded.remove(&id) {
                    close(&mut editors, l);
                }
            }
            Ok(Req::Quit) => break,
            Ok(Req::ShowEditor(id)) => {
                if let Some(l) = loaded.get_mut(&id) {
                    if l.window.as_ref().is_some_and(|w| w.is_open()) {
                        continue;
                    }
                    let Handle::V3(plugin) = &l.plugin else {
                        let _ = tx.send(Reply::Failed {
                            id,
                            error: "This plugin's editor opens inside its window only.".into(),
                        });
                        continue;
                    };
                    let mut w = vst3_host::window::PluginWindow::new(plugin.clone());
                    match w.open() {
                        Ok(()) => {
                            l.window = Some(w);
                            let _ = tx.send(Reply::Editor { id, open: true });
                        }
                        Err(e) => {
                            let _ = tx.send(Reply::Failed {
                                id,
                                error: format!("Editor: {e}"),
                            });
                        }
                    }
                    poke();
                }
            }
            Ok(Req::HideEditor(id)) => {
                if let Some(l) = loaded.get_mut(&id) {
                    let had = l.embed.is_some() || l.window.is_some();
                    unembed(&mut editors, l);
                    if let Some(mut w) = l.window.take() {
                        w.close();
                    }
                    if had {
                        let _ = tx.send(Reply::Editor { id, open: false });
                        poke();
                    }
                }
            }
            Ok(Req::SaveStates) => {
                let states = loaded
                    .iter()
                    .filter_map(|(id, l)| Some((*id, l.plugin.save_state()?)))
                    .collect();
                let _ = tx.send(Reply::States(states));
                poke();
            }
            Ok(Req::Rate(r)) if r != rate => {
                rate = r;
                let states: Vec<(u64, Vec<u8>, PathBuf, bool)> = loaded
                    .iter()
                    .map(|(id, l)| {
                        (
                            *id,
                            l.plugin.save_state().unwrap_or_default(),
                            l.path.clone(),
                            l.effect,
                        )
                    })
                    .collect();
                for (_, mut l) in loaded.drain() {
                    unembed(&mut editors, &mut l);
                    if let Some(w) = &mut l.window {
                        w.close();
                    }
                }
                if let Ok(h) = build(rate) {
                    host = h;
                }
                for (id, state, path, effect) in states {
                    match load_any(&mut host, &path, &state, rate, effect) {
                        Ok((plugin, name, gui, proc)) => {
                            loaded.insert(
                                id,
                                Loaded {
                                    plugin,
                                    window: None,
                                    embed: None,
                                    path,
                                    effect,
                                },
                            );
                            let outs = proc.lock().map(|p| p.extra_pairs()).unwrap_or(0);
                            let _ = tx.send(Reply::Loaded {
                                id,
                                proc,
                                name,
                                gui,
                                outs,
                            });
                        }
                        Err(error) => {
                            let _ = tx.send(Reply::Failed { id, error });
                        }
                    }
                }
                poke();
            }
            Ok(Req::Rate(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Embedded editors: OS events, run loop, DPI updates, and resize requests.
        editors.poll(&mut loaded, &tx, &poke);

        // Separate editor windows: window events, run loop, closes from the title bar.
        for (id, l) in loaded.iter_mut() {
            let Some(w) = &mut l.window else { continue };
            let _ = w.service_platform_events();
            let _ = l.plugin.service();
            if w.closed_by_user() {
                w.close();
                l.window = None;
                let _ = tx.send(Reply::Editor {
                    id: *id,
                    open: false,
                });
                poke();
            }
        }
        // CLAP main-thread callbacks, timers, and param flushes run even without an open editor.
        for l in loaded.values_mut() {
            if l.embed.is_none() && l.window.is_none() && matches!(l.plugin, Handle::Clap(_)) {
                let _ = l.plugin.service();
            }
        }
    }
    let trace = std::env::var_os("KFXL_PLUGIN_TRACE").is_some();
    for (id, l) in loaded.drain() {
        if trace {
            eprintln!("host: closing plugin {id}");
        }
        close(&mut editors, l);
    }
    if trace {
        eprintln!("host: plugins closed");
    }
    drop(host);
    let _ = tx.send(Reply::Bye);
}

#[cfg(test)]
mod vst3_bus_check {
    use super::*;
    use crate::plugins::wine::{apply_wine, set_dxvk, set_wine};
    use vst3_host::audio::AudioBuffers;

    /// A real plugin through the playback path (buses activated, `Vst3Proc`):
    /// `KFXL_VST3_CHECK=<path.vst3> [KFXL_VST3_EFFECT=1] cargo test --release
    /// --bin keyframexl vst3_bus_check -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_plugin_sounds_through_its_buses() {
        let Some(path) = std::env::var_os("KFXL_VST3_CHECK") else {
            return;
        };
        let effect = std::env::var_os("KFXL_VST3_EFFECT").is_some();
        let prefs = crate::settings::Settings::load();
        set_dxvk(prefs.plugins.dxvk);
        set_wine(prefs.plugins.wine.clone());
        let mut c = std::process::Command::new("true");
        apply_wine(&mut c);
        for (k, v) in c.get_envs() {
            // SAFETY: before any plugin thread starts.
            unsafe {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
        let mut host = Vst3Host::builder()
            .sample_rate(48_000.0)
            .block_size(MAX_BLOCK)
            .build()
            .unwrap();
        let plugin = load(&mut host, Path::new(&path), &[], 48_000, effect).unwrap();
        println!("layout {:?}", plugin.audio_bus_layout());
        let plugin = Arc::new(Mutex::new(plugin));
        let mut proc = Vst3Proc::new(plugin.clone(), effect, 48_000.0);
        println!(
            "extra pairs {}, sidechain bus {:?}",
            proc.extra_pairs(),
            proc.key_bus
        );
        let t = Transport {
            rate: 48_000,
            tempo: 120.0,
            playing: true,
            frame: 0,
        };
        let stride = 512 * 2;
        let mut extra = vec![0.0f32; stride * keyframexl::plugin::MAX_EXTRA_PAIRS];
        let mut peak = 0.0f32;
        let mut extra_peak = 0.0f32;
        std::thread::sleep(Duration::from_secs(5));
        for i in 0..200 {
            let mut io: Vec<f32> = if effect {
                (0..stride)
                    .map(|k| ((k / 2) as f32 * 0.06).sin() * 0.3)
                    .collect()
            } else {
                vec![0.0; stride]
            };
            let ev = if i % 50 == 2 && !effect {
                vec![PluginEvent::NoteOn {
                    at: 0,
                    pitch: 60,
                    velocity: 100,
                }]
            } else {
                vec![]
            };
            let wrote = proc.process_multi(&ev, &mut io, &mut extra, stride, &t);
            peak = io.iter().fold(peak, |m, s| m.max(s.abs()));
            extra_peak = extra[..wrote * stride]
                .iter()
                .fold(extra_peak, |m, s| m.max(s.abs()));
        }
        println!("main peak {peak}, extra peak {extra_peak}");
        if std::env::var_os("KFXL_VST3_FLAT").is_some() {
            let mut buf = AudioBuffers::new(if effect { 2 } else { 0 }, 2, 512, 48_000.0);
            let mut flat = 0.0f32;
            let mut p = plugin.lock().unwrap();
            for _ in 0..200 {
                for c in buf.inputs.iter_mut() {
                    for (k, s) in c.iter_mut().enumerate() {
                        *s = (k as f32 * 0.06).sin() * 0.3;
                    }
                }
                let _ = p.process_audio(&mut buf);
                flat = buf
                    .outputs
                    .iter()
                    .flatten()
                    .fold(flat, |m, s| m.max(s.abs()));
            }
            println!("flat peak {flat}");
        }
        assert!(peak > 1e-4, "the main pair is silent");
        let _ = plugin.lock().map(|mut p| p.stop_processing());
        std::process::exit(0);
    }
}
