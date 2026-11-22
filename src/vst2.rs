#![allow(dead_code)]
use std::{ffi::c_void, path::Path, ptr};

type Dispatch = unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
type Host = unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
type Process = unsafe extern "C" fn(*mut AEffect, *mut *mut f32, *mut *mut f32, i32);
#[repr(C)]
struct AEffect {
    magic: i32,
    dispatcher: Option<Dispatch>,
    process: Option<Process>,
    set_parameter: Option<unsafe extern "C" fn(*mut AEffect, i32, f32)>,
    get_parameter: Option<unsafe extern "C" fn(*mut AEffect, i32) -> f32>,
    num_programs: i32,
    num_params: i32,
    num_inputs: i32,
    num_outputs: i32,
    flags: i32,
    resvd1: isize,
    resvd2: isize,
    initial_delay: i32,
    real_qualities: i32,
    off_qualities: i32,
    io_ratio: f32,
    object: *mut c_void,
    user: *mut c_void,
    unique_id: i32,
    version: i32,
    process_replacing: Option<Process>,
    process_double_replacing: Option<unsafe extern "C" fn()>,
    future: [u8; 56],
}
#[repr(C)]
#[derive(Default)]
struct VstTimeInfo {
    sample_pos: f64,
    sample_rate: f64,
    nano_seconds: f64,
    ppq_pos: f64,
    tempo: f64,
    bar_start_pos: f64,
    cycle_start_pos: f64,
    cycle_end_pos: f64,
    time_sig_numerator: i32,
    time_sig_denominator: i32,
    smpte_offset: i32,
    smpte_frame_rate: i32,
    samples_to_next_clock: i32,
    flags: i32,
}
struct Context {
    time: VstTimeInfo,
    block: i32,
    resize: std::sync::atomic::AtomicU64,
}
#[repr(C)]
struct MidiEvent {
    kind: i32,
    byte_size: i32,
    delta_frames: i32,
    flags: i32,
    note_length: i32,
    note_offset: i32,
    midi_data: [u8; 4],
    detune: i8,
    note_off_velocity: u8,
    reserved1: u8,
    reserved2: u8,
}
#[repr(C)]
struct Events {
    num_events: i32,
    reserved: isize,
    events: [*mut MidiEvent; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EditorRect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}
fn contexts() -> &'static std::sync::Mutex<std::collections::HashMap<usize, usize>> {
    static CONTEXTS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<usize, usize>>,
    > = std::sync::OnceLock::new();
    CONTEXTS.get_or_init(Default::default)
}
unsafe extern "C" fn host(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    _ptr: *mut c_void,
    _opt: f32,
) -> isize {
    if opcode == 1 {
        return 2400;
    }
    if effect.is_null() {
        return 0;
    }
    let ctx = contexts()
        .lock()
        .unwrap()
        .get(&(effect as usize))
        .copied()
        .unwrap_or(0) as *mut Context;
    if ctx.is_null() {
        return 0;
    }
    match opcode {
        7 => unsafe { &mut (*ctx).time as *mut VstTimeInfo as isize },
        16 => unsafe { (*ctx).time.sample_rate as isize },
        17 => unsafe { (*ctx).block as isize },
        15 if index > 0 && value > 0 && value <= i32::MAX as isize => {
            unsafe {
                (*ctx).resize.store(
                    ((index as u64) << 32) | value as u64,
                    std::sync::atomic::Ordering::Relaxed,
                )
            };
            1
        }
        3 | 42 => 1,
        _ => 0,
    }
}
pub struct Vst2Plugin {
    effect: *mut AEffect,
    /// Kept alive until after `Drop::drop` finishes `effClose` (struct fields drop after `Drop::drop`).
    _library: libloading::Library,
    context: Box<Context>,
    active: bool,
    editor: bool,
    /// Channel pointer lists, sized at load: `process_into` does not allocate.
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    /// Events of one block, sized at load (`queue_midi` / `flush_midi`).
    events: Vec<MidiEvent>,
    event_ptrs: Vec<*mut MidiEvent>,
}

/// The plugin is driven from one thread at a time (behind a mutex).
unsafe impl Send for Vst2Plugin {}

/// Most events one block passes to the plugin.
pub const MAX_EVENTS: usize = 512;

/// `VstEvents` with room for `MAX_EVENTS` pointers.
#[repr(C)]
struct EventList {
    num_events: i32,
    reserved: isize,
    events: [*mut MidiEvent; MAX_EVENTS],
}
impl Vst2Plugin {
    pub fn load(path: &Path) -> Result<Self, String> {
        unsafe {
            let library = libloading::Library::new(path).map_err(|e| e.to_string())?;
            let entry: libloading::Symbol<'_, unsafe extern "C" fn(Host) -> *mut AEffect> = library
                .get(b"VSTPluginMain\0")
                .or_else(|_| library.get(b"main\0"))
                .map_err(|_| "VSTPluginMain missing".to_string())?;
            let effect = entry(host);
            if effect.is_null() {
                return Err("VSTPluginMain returned null".into());
            }
            if (*effect).magic != 0x56737450 || (*effect).dispatcher.is_none() {
                return Err("invalid AEffect".into());
            }
            let mut context = Box::new(Context {
                time: VstTimeInfo {
                    sample_rate: 48000.0,
                    tempo: 120.0,
                    flags: 2 | 512 | 1024,
                    ..Default::default()
                },
                block: 512,
                resize: std::sync::atomic::AtomicU64::new(0),
            });
            (*effect).resvd1 = (&mut *context as *mut Context) as isize;
            contexts()
                .lock()
                .unwrap()
                .insert(effect as usize, (*effect).resvd1 as usize);
            let (ni, no) = (
                (*effect).num_inputs.max(0) as usize,
                (*effect).num_outputs.max(0) as usize,
            );
            let mut plugin = Self {
                effect,
                _library: library,
                context,
                active: false,
                editor: false,
                in_ptrs: vec![ptr::null_mut(); ni],
                out_ptrs: vec![ptr::null_mut(); no],
                events: Vec::with_capacity(MAX_EVENTS),
                event_ptrs: Vec::with_capacity(MAX_EVENTS),
            };
            plugin.call(0, 0, 0, ptr::null_mut(), 0.0);
            Ok(plugin)
        }
    }
    fn call(&mut self, op: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        unsafe { ((*self.effect).dispatcher.unwrap())(self.effect, op, index, value, ptr, opt) }
    }
    pub fn configure(&mut self, rate: f32, block: i32) {
        self.context.time.sample_rate = rate as f64;
        self.context.block = block;
        self.call(10, 0, 0, ptr::null_mut(), rate);
        self.call(11, 0, block as isize, ptr::null_mut(), 0.0);
    }
    pub fn start(&mut self) {
        if !self.active {
            self.call(12, 0, 1, ptr::null_mut(), 0.0);
            self.call(71, 0, 0, ptr::null_mut(), 0.0);
            self.active = true;
        }
    }
    pub fn stop(&mut self) {
        if self.active {
            self.call(72, 0, 0, ptr::null_mut(), 0.0);
            self.call(12, 0, 0, ptr::null_mut(), 0.0);
            self.active = false;
        }
    }
    /// The plugin's reported latency in frames (`initialDelay`).
    pub fn initial_delay(&self) -> usize {
        unsafe { (*self.effect).initial_delay.max(0) as usize }
    }

    pub fn info(&self) -> (bool, usize, usize, usize) {
        unsafe {
            let e = &*self.effect;
            (
                e.flags & 256 != 0,
                e.num_inputs.max(0) as usize,
                e.num_outputs.max(0) as usize,
                e.num_params.max(0) as usize,
            )
        }
    }
    fn string(&mut self, op: i32, index: i32) -> String {
        let mut buf = [0u8; 256];
        self.call(op, index, 0, buf.as_mut_ptr().cast(), 0.0);
        let end = buf.iter().position(|&x| x == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }
    pub fn name(&mut self) -> String {
        self.string(45, 0)
    }
    pub fn vendor(&mut self) -> String {
        self.string(47, 0)
    }
    pub fn parameter_name(&mut self, index: i32) -> String {
        self.string(8, index)
    }
    pub fn parameter_display(&mut self, index: i32) -> String {
        self.string(7, index)
    }
    pub fn parameter(&self, index: i32) -> Option<f32> {
        unsafe { (*self.effect).get_parameter.map(|f| f(self.effect, index)) }
    }
    pub fn set_parameter(&mut self, index: i32, value: f32) {
        unsafe {
            if let Some(f) = (*self.effect).set_parameter {
                f(self.effect, index, value)
            }
        }
    }
    pub fn chunk(&mut self, program: bool) -> Option<Vec<u8>> {
        if unsafe { (*self.effect).flags & 32 == 0 } {
            return None;
        }
        let mut data: *mut c_void = ptr::null_mut();
        let len = self.call(
            23,
            program as i32,
            0,
            (&mut data as *mut *mut c_void).cast(),
            0.0,
        );
        if len <= 0 || data.is_null() {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize).to_vec() })
    }
    pub fn set_chunk(&mut self, program: bool, data: &[u8]) -> bool {
        if unsafe { (*self.effect).flags & 32 == 0 } {
            return false;
        }
        self.call(
            24,
            program as i32,
            data.len() as isize,
            data.as_ptr() as *mut c_void,
            0.0,
        ) != 0
    }
    pub fn midi(&mut self, status: u8, note: u8, velocity: u8) {
        let mut event = MidiEvent {
            kind: 1,
            byte_size: std::mem::size_of::<MidiEvent>() as i32,
            delta_frames: 0,
            flags: 0,
            note_length: 0,
            note_offset: 0,
            midi_data: [status, note, velocity, 0],
            detune: 0,
            note_off_velocity: 0,
            reserved1: 0,
            reserved2: 0,
        };
        let mut events = Events {
            num_events: 1,
            reserved: 0,
            events: [&mut event, ptr::null_mut()],
        };
        self.call(25, 0, 0, (&mut events as *mut Events).cast(), 0.0);
    }
    pub fn process(
        &mut self,
        inputs: &mut [Vec<f32>],
        outputs: &mut [Vec<f32>],
        frames: usize,
    ) -> Result<(), &'static str> {
        let (_, ni, no, _) = self.info();
        if !self.active
            || inputs.len() != ni
            || outputs.len() != no
            || inputs
                .iter()
                .chain(outputs.iter())
                .any(|b| b.len() < frames)
            || frames > i32::MAX as usize
        {
            return Err("invalid audio buffers or inactive plugin");
        }
        let process =
            unsafe { (*self.effect).process_replacing }.ok_or("processReplacing unavailable")?;
        let mut ins: Vec<_> = inputs.iter_mut().map(|b| b.as_mut_ptr()).collect();
        let mut outs: Vec<_> = outputs.iter_mut().map(|b| b.as_mut_ptr()).collect();
        unsafe {
            process(
                self.effect,
                ins.as_mut_ptr(),
                outs.as_mut_ptr(),
                frames as i32,
            )
        };
        self.context.time.sample_pos += frames as f64;
        self.context.time.ppq_pos +=
            frames as f64 / self.context.time.sample_rate * self.context.time.tempo / 60.0;
        Ok(())
    }
    /// Tempo and transport the plugin reads through audioMasterGetTime.
    pub fn set_time(&mut self, tempo: f64, playing: bool) {
        let t = &mut self.context.time;
        t.tempo = tempo.max(1.0);
        // kVstTransportPlaying (2), ppq/tempo valid (512 | 1024).
        t.flags = if playing { 2 } else { 0 } | 512 | 1024;
    }

    /// Queue a short MIDI message `delta` frames into the next block.
    pub fn queue_midi(&mut self, bytes: [u8; 3], delta: i32) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(MidiEvent {
                kind: 1,
                byte_size: std::mem::size_of::<MidiEvent>() as i32,
                delta_frames: delta.max(0),
                flags: 1,
                note_length: 0,
                note_offset: 0,
                midi_data: [bytes[0], bytes[1], bytes[2], 0],
                detune: 0,
                note_off_velocity: 0,
                reserved1: 0,
                reserved2: 0,
            });
        }
    }

    /// Hand the queued events to the plugin (before `process_into`). No allocation.
    pub fn flush_midi(&mut self) {
        if self.events.is_empty() {
            return;
        }
        self.event_ptrs.clear();
        for e in self.events.iter_mut() {
            self.event_ptrs.push(e);
        }
        let mut list = EventList {
            num_events: self.events.len() as i32,
            reserved: 0,
            events: [ptr::null_mut(); MAX_EVENTS],
        };
        list.events[..self.event_ptrs.len()].copy_from_slice(&self.event_ptrs);
        self.call(25, 0, 0, (&mut list as *mut EventList).cast(), 0.0);
        self.events.clear();
    }

    /// processReplacing over channels the caller owns. Inputs the caller has
    /// no channel for read `silence` (all zeros); outputs it has no channel
    /// for (SSD5 has 48) all write into `scratch`. No allocation.
    pub fn process_into(
        &mut self,
        inputs: &mut [Vec<f32>],
        outputs: &mut [Vec<f32>],
        silence: &mut [f32],
        scratch: &mut [f32],
        frames: usize,
    ) -> bool {
        let Some(process) = (unsafe { (*self.effect).process_replacing }) else {
            return false;
        };
        if !self.active || frames == 0 || silence.len() < frames || scratch.len() < frames {
            return false;
        }
        for (i, p) in self.in_ptrs.iter_mut().enumerate() {
            *p = match inputs.get_mut(i) {
                Some(b) if b.len() >= frames => b.as_mut_ptr(),
                _ => silence.as_mut_ptr(),
            };
        }
        for (i, p) in self.out_ptrs.iter_mut().enumerate() {
            *p = match outputs.get_mut(i) {
                Some(b) if b.len() >= frames => b.as_mut_ptr(),
                _ => scratch.as_mut_ptr(),
            };
        }
        unsafe {
            process(
                self.effect,
                self.in_ptrs.as_mut_ptr(),
                self.out_ptrs.as_mut_ptr(),
                frames as i32,
            )
        };
        let t = &mut self.context.time;
        t.sample_pos += frames as f64;
        t.ppq_pos += frames as f64 / t.sample_rate * t.tempo / 60.0;
        true
    }

    /// Whether the plugin keeps its state in a chunk (else: parameters only).
    pub fn has_chunk(&self) -> bool {
        unsafe { (*self.effect).flags & 32 != 0 }
    }

    pub fn has_editor(&self) -> bool {
        unsafe { (*self.effect).flags & 1 != 0 }
    }

    pub fn editor_rect(&mut self) -> Option<EditorRect> {
        let mut rect: *mut EditorRect = ptr::null_mut();
        self.call(13, 0, 0, (&mut rect as *mut *mut EditorRect).cast(), 0.0);
        unsafe { rect.as_ref().copied() }
    }
    pub fn editor_open(&mut self, parent: usize) -> bool {
        if self.editor {
            return true;
        }
        self.editor = self.call(14, 0, 0, parent as *mut c_void, 0.0) != 0;
        self.editor
    }
    pub fn editor_idle(&mut self) {
        if self.editor {
            self.call(19, 0, 0, ptr::null_mut(), 0.0);
        }
    }
    pub fn take_editor_resize_request(&mut self) -> Option<(i32, i32)> {
        let size = self
            .context
            .resize
            .swap(0, std::sync::atomic::Ordering::Relaxed);
        (size != 0).then_some(((size >> 32) as i32, size as i32))
    }
    pub fn editor_close(&mut self) {
        if self.editor {
            self.call(15, 0, 0, ptr::null_mut(), 0.0);
            self.editor = false;
        }
    }
}
impl Drop for Vst2Plugin {
    fn drop(&mut self) {
        self.editor_close();
        self.stop();
        self.call(1, 0, 0, ptr::null_mut(), 0.0);
        contexts().lock().unwrap().remove(&(self.effect as usize));
    }
}
#[cfg(target_os = "linux")]
pub fn probe(path: &Path) -> Result<(), String> {
    let mut p = Vst2Plugin::load(path)?;
    let (synth, ni, no, np) = p.info();
    println!(
        "Name: {}\nVendor: {}\nSynth: {synth}\nInputs: {ni}\nOutputs: {no}\nParameters: {np}",
        p.name(),
        p.vendor()
    );
    println!(
        "Bank chunk bytes: {}",
        p.chunk(false).map_or(0, |c| c.len())
    );
    p.configure(48000.0, 512);
    p.start();
    // KFXL_PROBE_EDITOR: open the editor in a plain X11 window for the whole
    // run (before the state goes in, as when a user loads a preset into an
    // open plugin), so `keyframexl x11-shot` can show what it displays.
    let _editor = std::env::var_os("KFXL_PROBE_EDITOR").and_then(|_| {
        let (conn, screen) = xcb::Connection::connect(None).ok()?;
        let root = conn.get_setup().roots().nth(screen as usize)?.root();
        let (w, h) = p.editor_rect().map_or((800, 600), |r| {
            (
                (r.right - r.left).max(100) as u16,
                (r.bottom - r.top).max(100) as u16,
            )
        });
        let win: xcb::x::Window = conn.generate_id();
        conn.send_request(&xcb::x::CreateWindow {
            depth: xcb::x::COPY_FROM_PARENT as u8,
            wid: win,
            parent: root,
            x: 0,
            y: 0,
            width: w,
            height: h,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: xcb::x::COPY_FROM_PARENT,
            value_list: &[],
        });
        conn.send_request(&xcb::x::MapWindow { window: win });
        conn.flush().ok()?;
        println!(
            "Editor opened: {} ({w}x{h})",
            p.editor_open(xcb::Xid::resource_id(&win) as usize)
        );
        Some(conn)
    });
    // KFXL_PROBE_CHUNK_IN: restore this state file before playing (e.g. an
    // FL Studio VSTi project), KFXL_PROBE_CHUNK_OUT: save the state after.
    if let Some(file) = std::env::var_os("KFXL_PROBE_CHUNK_IN") {
        let data = std::fs::read(&file).map_err(|e| e.to_string())?;
        // KFXL_PROBE_CHUNK_LATE=n: process n blocks first, as a running DAW
        // would before the user loads a preset.
        let late = std::env::var("KFXL_PROBE_CHUNK_LATE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let (mut ins, mut outs) = (vec![vec![0.0; 512]; ni], vec![vec![0.0; 512]; no]);
        for _ in 0..late {
            p.process(&mut ins, &mut outs, 512)?;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        println!("Chunk restored: {}", p.set_chunk(false, &data));
    }
    // KFXL_PROBE_PLAY: report a playing transport (120 BPM), so a hosted
    // sequencer (FL Studio VSTi) plays its own pattern.
    if std::env::var_os("KFXL_PROBE_PLAY").is_some() {
        p.set_time(120.0, true);
    }
    let mut ins = vec![vec![0.0; 512]; ni];
    let mut outs = vec![vec![0.0; 512]; no];
    let (mut peak, mut sum, mut count) = (0f32, 0f64, 0usize);
    // Notes in turn (a drum kit: kick, snare, hi-hat, toms, crash), each
    // half a second, so a multi-out kit shows which output each one uses.
    let notes = [48u8, 36, 38, 42, 45, 49];
    let mut per_out = vec![[0f32; 6]; no];
    // Sample libraries load in the background: give them time first
    // (KFXL_PROBE_WARM seconds of wall clock, default 4 s of audio).
    if let Some(secs) = std::env::var("KFXL_PROBE_WARM")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while std::time::Instant::now() < until {
            for b in &mut outs {
                b.fill(0.0);
            }
            p.process(&mut ins, &mut outs, 512)?;
            p.editor_idle();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let warm = 48000 * 4;
    for n in (0..warm + notes.len() * 24000).step_by(512) {
        let frames = 512;
        let slot = n.checked_sub(warm).map(|t| t / 24000);
        if let Some(k) = slot
            && (n - warm) % 24000 == 0
        {
            if k > 0 {
                p.midi(0x80, notes[k - 1], 0);
            }
            p.midi(0x90, notes[k], 100);
        }
        for b in &mut outs {
            b.fill(0.0);
        }
        p.process(&mut ins, &mut outs, frames)?;
        for (c, b) in outs.iter().enumerate() {
            for x in &b[..frames] {
                peak = peak.max(x.abs());
                sum += (*x as f64).powi(2);
                count += 1;
                if let Some(k) = slot {
                    per_out[c][k] = per_out[c][k].max(x.abs());
                }
            }
        }
        p.editor_idle();
    }
    p.midi(0x80, notes[notes.len() - 1], 0);
    if let Some(file) = std::env::var_os("KFXL_PROBE_CHUNK_OUT") {
        let data = p.chunk(false).unwrap_or_default();
        std::fs::write(&file, &data).map_err(|e| e.to_string())?;
        println!("Chunk saved: {} bytes", data.len());
    }
    println!("Peak per output pair for notes {notes:?}:");
    for pair in 0..no.div_ceil(2) {
        let peaks: Vec<f32> = (0..notes.len())
            .map(|k| per_out[2 * pair][k].max(per_out.get(2 * pair + 1).map_or(0.0, |o| o[k])))
            .collect();
        if peaks.iter().any(|v| *v > 1e-5) {
            println!(
                "  {}-{}: {:?}",
                2 * pair + 1,
                2 * pair + 2,
                peaks.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>()
            );
        }
    }
    println!(
        "Peak: {peak:.8}\nRMS: {:.8}",
        if count == 0 {
            0.0
        } else {
            (sum / count as f64).sqrt()
        }
    );
    Ok(())
}
#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;
    #[test]
    fn abi_layout() {
        assert_eq!(std::mem::size_of::<AEffect>(), 0xc0);
        assert_eq!(std::mem::offset_of!(AEffect, resvd1), 64);
        assert_eq!(std::mem::offset_of!(AEffect, process_replacing), 120);
        assert_eq!(std::mem::size_of::<MidiEvent>(), 32);
        assert_eq!(std::mem::offset_of!(Events, events), 16);
        assert_eq!(std::mem::size_of::<Events>(), 32);
        assert_eq!(std::mem::size_of::<VstTimeInfo>(), 88);
    }
}
