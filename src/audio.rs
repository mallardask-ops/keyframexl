use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use keyframexl::{
    engine::{Engine, Tap},
    link::{FxSender, SongSender, fx_link, song_link},
    sequence::Song,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
};

/// A file played from the browser, outside the song (FL: preview).
pub struct Preview {
    pub frames: Arc<Vec<[f32; 2]>>,
    pub rate: u32,
}

/// Mixes a preview clip into an output block; owns its play position.
#[derive(Default)]
struct PreviewPlayer {
    clip: Option<Arc<Preview>>,
    pos: f64,
}

impl PreviewPlayer {
    fn run(
        &mut self,
        shared: &ArcSwapOption<Preview>,
        done: &AtomicBool,
        data: &mut [f32],
        rate: u32,
    ) {
        let now = shared.load_full();
        let same = match (&now, &self.clip) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            // A new clip starts from the top. The old Arc drops here; the UI keeps
            // its own reference until it replaces it, so nothing big frees here.
            self.clip = now;
            self.pos = 0.0;
        }
        let Some(clip) = &self.clip else { return };
        let step = clip.rate as f64 / rate as f64;
        let n = clip.frames.len();
        for f in data.chunks_exact_mut(2) {
            let i = self.pos as usize;
            if i + 1 >= n {
                done.store(true, Ordering::Relaxed);
                break;
            }
            let fr = (self.pos - i as f64) as f32;
            let (a, b) = (clip.frames[i], clip.frames[i + 1]);
            f[0] += (a[0] + (b[0] - a[0]) * fr) * 0.8;
            f[1] += (a[1] + (b[1] - a[1]) * fr) * 0.8;
            self.pos += step;
        }
    }
}

/// Add what `make` writes to `data`, `lag` frames late (the mix's latency),
/// through `scratch` and `pdc`. No lag, or a block longer than `scratch`:
/// `make` writes straight into `data`. No allocation.
fn delayed(
    data: &mut [f32],
    scratch: &mut [f32],
    pdc: &mut keyframexl::plugin::Pdc,
    lag: usize,
    make: impl FnOnce(&mut [f32]),
) {
    match scratch.get_mut(..data.len()) {
        Some(c) if lag > 0 => {
            c.fill(0.0);
            make(c);
            pdc.run(c, lag);
            for (d, x) in data.iter_mut().zip(c.iter()) {
                *d += *x;
            }
        }
        _ => make(data),
    }
}

/// Beat clicks laid over the output while playing (FL: metronome).
#[derive(Default)]
struct Metronome {
    /// Frames into the click that sounds, and its pitch.
    t: Option<(u32, f32)>,
}

impl Metronome {
    /// `at`: loop position of the block's first frame; `wrap`: frame where the
    /// loop restarted inside the block and the position it restarted at.
    fn run(
        &mut self,
        data: &mut [f32],
        at: u64,
        wrap: Option<(usize, u64)>,
        tempo: f64,
        map: Option<&keyframexl::tempo::TempoTable>,
        meters: &[keyframexl::tempo::Meter],
        rate: u32,
    ) {
        use keyframexl::sequence::{PPQ, Tick};
        let beat = (60.0 / tempo.clamp(10.0, 999.0) * rate as f64).max(1.0);
        let len = (0.03 * rate as f64) as u32;
        // With a tempo map or time signatures the clicks follow them: one per
        // beat of the bar's own unit (7/8 counts eighths), the bar's first higher.
        let per_tick = beat / PPQ as f64;
        let tick_of = |pos: u64| -> Tick {
            match map {
                Some(m) => m.tick_at_frame(pos),
                None => (pos as f64 / per_tick) as Tick,
            }
        };
        let frame_of = |t: Tick| -> u64 {
            match map {
                Some(m) => m.frames_at_ceil(t),
                None => (t as f64 * per_tick).ceil() as u64,
            }
        };
        let shaped = map.is_some() || !meters.is_empty();
        for (i, f) in data.chunks_exact_mut(2).enumerate() {
            let pos = match wrap {
                Some((w, to)) if i >= w => to + (i - w) as u64,
                _ => at + i as u64,
            };
            let (on, accent) = if shaped {
                let t = tick_of(pos);
                let (_, start, blen, num, _) = keyframexl::tempo::bar_at(meters, t);
                let unit = (blen / num.max(1) as Tick).max(1);
                let b0 = start + (t - start) / unit * unit;
                // Float rounding can put the beat's own frame one tick early: check the next beat too.
                let b1 = (b0 + unit).min(start + blen);
                if frame_of(b0) == pos {
                    (true, b0 == start)
                } else if frame_of(b1) == pos {
                    (true, b1 == start + blen)
                } else {
                    (false, false)
                }
            } else {
                let k = (pos as f64 / beat).floor();
                (pos as f64 - k * beat < 1.0, (k as u64).is_multiple_of(4))
            };
            if on {
                // Bar starts click higher.
                let hz = if accent { 1760.0 } else { 1320.0 };
                self.t = Some((0, hz));
            }
            if let Some((n, hz)) = &mut self.t {
                let x = *n as f32 / rate as f32;
                let s = (x * *hz * std::f32::consts::TAU).sin() * (-x * 160.0).exp() * 0.35;
                f[0] += s;
                f[1] += s;
                *n += 1;
                if *n >= len {
                    self.t = None;
                }
            }
        }
    }
}

/// Keyboard / piano roll notes played live (FL: preview while held).
#[derive(Clone, Copy, Debug)]
pub enum LiveCmd {
    /// `ch`: MIDI channel 0..15 (MPE), 0 for the mouse and typing keys.
    /// `lyric`: on a singing channel, the syllable to sing (`vocal::lyric_hash`), 0 = its default.
    On {
        id: u64,
        inst: u64,
        pitch: u8,
        velocity: u8,
        ch: u8,
        lyric: u64,
    },
    Off {
        id: u64,
    },
    /// Controller / pitch bend for a plugin channel.
    Midi {
        inst: u64,
        bytes: [u8; 3],
    },
}

/// Plugin instances in and out of the audio thread.
pub enum PluginCmd {
    Add(keyframexl::plugin::Instance),
    Remove(u64),
}

/// Recording control for the audio thread.
pub enum TapCmd {
    Add(Tap),
    StopAll,
}

/// Output devices the host offers (names as `AudioSettings::output` stores them).
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut v: Vec<String> = host
        .output_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    v.dedup();
    v
}

/// Input devices (microphones, line in).
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut v: Vec<String> = host
        .input_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    v.dedup();
    v
}

/// Buffer frames to request: the user's pick when the device allows it, else
/// a low-latency default (256 ≈ 5 ms at 48 kHz, then 512) so live keys answer
/// fast. The old "Auto" took the device default, which on the ALSA bridge is
/// often 1024+ frames (≈ 25 ms) before any plugin or system latency.
fn pick_buffer(want: Option<u32>, range: &cpal::SupportedBufferSize) -> Option<u32> {
    let allows = |b: u32| match range {
        cpal::SupportedBufferSize::Range { min, max } => (*min..=*max).contains(&b),
        cpal::SupportedBufferSize::Unknown => true,
    };
    if let Some(b) = want {
        return allows(b).then_some(b);
    }
    // Auto names a size only when the range is known: a Fixed request on an
    // Unknown range could fail the open and leave no audio at all.
    if matches!(range, cpal::SupportedBufferSize::Range { .. }) {
        for b in [256, 512] {
            if allows(b) {
                return Some(b);
            }
        }
    }
    None
}

/// What the running stream actually got.
/// Channels whose activity the channel rack can light.
pub const MAX_ACTIVITY: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub device: String,
    pub rate: u32,
    /// Frames per callback when fixed; `None` = device default (only when the
    /// range is unknown or covers neither 256 nor 512).
    pub buffer: Option<u32>,
}

impl Running {
    /// Output latency of one buffer in milliseconds (unknown buffer: the usual 1024).
    pub fn latency_ms(&self) -> f64 {
        self.buffer.unwrap_or(1024) as f64 / self.rate as f64 * 1000.0
    }
}

pub struct Audio {
    pub running: Running,
    /// Live notes to the audio thread (the UI and the MIDI input threads lock it briefly).
    pub live: Arc<Mutex<rtrb::Producer<LiveCmd>>>,
    /// Install / remove plugin instances; removed ones come back to be freed here.
    pub plugins: Mutex<(
        rtrb::Producer<PluginCmd>,
        rtrb::Consumer<keyframexl::plugin::Instance>,
    )>,
    /// While set (offline export uses the instances), the output is silent and
    /// the audio thread leaves plugins alone.
    pub hold_plugins: Arc<AtomicBool>,
    /// Start/stop recording taps; finished taps come back on `taps_back`.
    pub taps: Mutex<(rtrb::Producer<TapCmd>, rtrb::Consumer<Tap>)>,
    /// Edited songs go to the audio thread over an `rtrb` ring (only the UI
    /// thread locks this; the audio thread owns the receiving end).
    pub link: Mutex<(FxSender, SongSender)>,
    /// Renders singing channels in the background and hands the audio thread the result.
    pub vocal: Arc<keyframexl::vocal::host::Host>,
    pub playing: Arc<AtomicBool>,
    pub song_mode: Arc<AtomicBool>,
    pub pattern: Arc<AtomicU64>,
    pub position: Arc<AtomicU64>,
    /// Callback time as a share of the buffer duration, in tenths of a percent.
    pub load: Arc<AtomicU32>,
    pub overruns: Arc<AtomicU32>,
    /// Voices sounding after the last callback.
    pub voices: Arc<AtomicU32>,
    /// Post-fader peak per mixer track (f32 bits), 0 = Master.
    pub peaks: Arc<Vec<AtomicU32>>,
    /// Per channel (index in the song's instruments): 2 a note plays, 1 its
    /// tail rings, 0 silent; held at its most until the UI takes it.
    pub activity: Arc<Vec<AtomicU8>>,
    /// Gain reduction in dB (f32 bits) per built-in effect slot,
    /// `track * FX_SLOTS + slot`, held at its most until the UI takes it.
    pub reduction: Arc<Vec<AtomicU32>>,
    /// One track's last frames, for the EQ's spectrum.
    pub scope: Arc<keyframexl::engine::Scope>,
    /// Browser preview: set to play, clear to stop. `preview_done` turns true at its end.
    pub preview: Arc<ArcSwapOption<Preview>>,
    pub preview_done: Arc<AtomicBool>,
    /// Beat clicks while playing.
    pub metronome: Arc<AtomicBool>,
    /// Count-in: frames of clicks left before the transport starts by itself
    /// (0: none), out of `count_in_len`.
    pub count_in: Arc<AtomicU64>,
    pub count_in_len: Arc<AtomicU64>,
    /// The stream reported an error (the device went away, or ALSA keeps
    /// failing after an underrun): the UI reopens the output.
    pub failed: Arc<AtomicBool>,
    _stream: cpal::Stream,
}
impl Audio {
    /// Send an edited song to the audio thread (never blocks it).
    /// Rebuild one slot's effect state on the next send (a new impulse response).
    pub fn invalidate_fx(&self, track: usize, slot: usize) {
        if let Ok(mut l) = self.link.lock() {
            l.0.invalidate(track, slot);
        }
    }

    pub fn send_song(&self, song: Song) {
        if let Ok(mut l) = self.link.lock() {
            // Effect states first: they are installed before the song plays.
            l.0.sync(&song);
            self.vocal.update(&song, self.running.rate);
            l.1.send(Arc::new(song));
        }
    }

    /// Play or release a live note (never blocks; dropped if the ring is full).
    pub fn live(&self, cmd: LiveCmd) {
        if let Ok(mut l) = self.live.lock() {
            let _ = l.push(cmd);
        }
    }

    /// Hand a plugin instance to the audio thread (false: the queue is full).
    pub fn add_plugin(&self, inst: keyframexl::plugin::Instance) -> bool {
        self.plugins
            .lock()
            .is_ok_and(|mut p| p.0.push(PluginCmd::Add(inst)).is_ok())
    }

    pub fn remove_plugin(&self, id: u64) {
        if let Ok(mut p) = self.plugins.lock() {
            let _ = p.0.push(PluginCmd::Remove(id));
        }
    }

    /// Once per UI frame: retry a pending song, free replaced ones.
    pub fn tick(&self) {
        if let Ok(mut l) = self.link.lock() {
            l.1.tick();
        }
        // Instances the audio thread let go of are dropped here (they may block).
        if let Ok(mut p) = self.plugins.lock() {
            while p.1.pop().is_ok() {}
        }
    }

    #[cfg(test)]
    pub fn new(song: Song) -> Result<Self, String> {
        Self::open(song, &crate::settings::AudioSettings::default())
    }

    /// Open the output from `cfg`; an unknown device falls back to the default,
    /// an unsupported rate to 48 kHz.
    pub fn open(song: Song, cfg: &crate::settings::AudioSettings) -> Result<Self, String> {
        let host = cpal::default_host();
        let named = cfg.output.as_ref().and_then(|want| {
            host.output_devices()
                .ok()?
                .find(|d| d.name().ok().as_ref() == Some(want))
        });
        let device = match named {
            Some(d) => d,
            None => host
                .default_output_device()
                .ok_or("No default audio output")?,
        };
        let device_name = device.name().unwrap_or_else(|_| "Default".into());
        let configs: Vec<_> = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .collect();
        let pick = |rate: u32| {
            configs.iter().find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.channels() == 2
                    && c.min_sample_rate().0 <= rate
                    && c.max_sample_rate().0 >= rate
            })
        };
        let (range, rate) = match pick(cfg.rate) {
            Some(c) => (c, cfg.rate),
            None => (
                pick(48_000).ok_or_else(|| {
                    format!(
                        "{device_name} does not offer stereo f32 at {} or 48 kHz",
                        cfg.rate
                    )
                })?,
                48_000,
            ),
        };
        let buffer = pick_buffer(cfg.buffer, &range.buffer_size());
        let mut config = range.with_sample_rate(cpal::SampleRate(rate)).config();
        if let Some(b) = buffer {
            config.buffer_size = cpal::BufferSize::Fixed(b);
        }
        let running = Running {
            device: device_name,
            rate,
            buffer,
        };
        let (mut fx_tx, mut fx_rx) = fx_link(rate);
        fx_tx.sync(&song);
        let first_song = song.clone();
        let (link, mut rx) = song_link(song);
        let (live_tx, mut live_rx) = rtrb::RingBuffer::<LiveCmd>::new(256);
        let mut was_playing = false;
        let (tap_cmd_tx, mut tap_cmd_rx) =
            rtrb::RingBuffer::<TapCmd>::new(keyframexl::engine::MAX_TAPS * 2);
        let (mut tap_back_tx, tap_back_rx) =
            rtrb::RingBuffer::<Tap>::new(keyframexl::engine::MAX_TAPS * 2);
        let (plug_tx, mut plug_rx) =
            rtrb::RingBuffer::<PluginCmd>::new(keyframexl::engine::MAX_PLUGINS * 2);
        let (mut plug_back_tx, plug_back_rx) =
            rtrb::RingBuffer::<keyframexl::plugin::Instance>::new(
                keyframexl::engine::MAX_PLUGINS * 4,
            );
        let hold_plugins = Arc::new(AtomicBool::new(false));
        let metronome = Arc::new(AtomicBool::new(false));
        let count_in = Arc::new(AtomicU64::new(0));
        let count_in_len = Arc::new(AtomicU64::new(0));
        let (cin, cin_len) = (count_in.clone(), count_in_len.clone());
        let mut count_metro = Metronome::default();
        let metro_on = metronome.clone();
        let mut metro = Metronome::default();
        // Clicks wait for the mix's latency (PDC) like the music does. Both
        // buffers are sized here: the audio thread never allocates them.
        let mut clicks = vec![0.0f32; 16_384 * 2];
        let mut click_pdc = keyframexl::plugin::Pdc::default();
        click_pdc.run(&mut [], 1);
        let hold = hold_plugins.clone();
        let playing = Arc::new(AtomicBool::new(false));
        let song_mode = Arc::new(AtomicBool::new(false));
        let pattern = Arc::new(AtomicU64::new(1));
        let position = Arc::new(AtomicU64::new(0));
        let load = Arc::new(AtomicU32::new(0));
        let overruns = Arc::new(AtomicU32::new(0));
        let callback_overruns = overruns.clone();
        let voices = Arc::new(AtomicU32::new(0));
        let peaks: Arc<Vec<AtomicU32>> = Arc::new((0..128).map(|_| AtomicU32::new(0)).collect());
        let (ld, vc, pk) = (load.clone(), voices.clone(), peaks.clone());
        let activity: Arc<Vec<AtomicU8>> =
            Arc::new((0..MAX_ACTIVITY).map(|_| AtomicU8::new(0)).collect());
        let act = activity.clone();
        let mut act_buf = [0u8; MAX_ACTIVITY];
        let mut publish_activity = move |engine: &Engine, song: &Song| {
            engine.channel_activity(song, &mut act_buf);
            for (slot, v) in act.iter().zip(act_buf.iter()).take(song.instruments.len()) {
                slot.fetch_max(*v, Ordering::Relaxed);
            }
        };
        let reduction: Arc<Vec<AtomicU32>> = Arc::new(
            (0..128 * keyframexl::sequence::FX_SLOTS)
                .map(|_| AtomicU32::new(0))
                .collect(),
        );
        let gr = reduction.clone();
        let preview: Arc<ArcSwapOption<Preview>> = Arc::new(ArcSwapOption::empty());
        let preview_done = Arc::new(AtomicBool::new(false));
        let (pv, pd) = (preview.clone(), preview_done.clone());
        let mut player = PreviewPlayer::default();
        let (p, m, i, pos) = (
            playing.clone(),
            song_mode.clone(),
            pattern.clone(),
            position.clone(),
        );
        // The engine owns its voices; it is created here and lives in the callback.
        let mut engine = Engine::new(rate);
        let vocal = Arc::new(keyframexl::vocal::host::Host::new());
        engine.vocal = vocal.map.clone();
        vocal.update(&first_song, rate);
        // Buffers sized before the stream starts: the callback never allocates
        // for blocks up to 8192 frames and mixers up to 128 tracks.
        engine.reserve(128, 8192);
        let scope = engine.scope();
        let failed = Arc::new(AtomicBool::new(false));
        let fail = failed.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    // Newest song first, even while stopped, so Play starts current.
                    fx_rx.drain(&mut engine);
                    while let Ok(cmd) = tap_cmd_rx.pop() {
                        match cmd {
                            TapCmd::Add(tap) => {
                                if let Err(tap) = engine.add_tap(tap) {
                                    let _ = tap_back_tx.push(tap);
                                }
                            }
                            TapCmd::StopAll => {
                                while let Some(tap) = engine.take_tap() {
                                    let _ = tap_back_tx.push(tap);
                                }
                            }
                        }
                    }
                    while let Ok(cmd) = plug_rx.pop() {
                        let back = match cmd {
                            PluginCmd::Add(inst) => engine.adopt_plugin(inst),
                            PluginCmd::Remove(id) => engine.take_plugin(id),
                        };
                        if let Some(old) = back {
                            let _ = plug_back_tx.push(old);
                        }
                    }
                    let snap = rx.update();
                    while let Ok(cmd) = live_rx.pop() {
                        match cmd {
                            LiveCmd::On {
                                id,
                                inst,
                                pitch,
                                velocity,
                                ch,
                                lyric,
                            } => {
                                // Same thread as the note: the syllable travels with its own key press.
                                engine.live_lyric.store(lyric, Ordering::Relaxed);
                                engine.live_on_ch(snap, id, inst, pitch, velocity, ch);
                            }
                            LiveCmd::Off { id } => engine.live_off(id),
                            LiveCmd::Midi { inst, bytes } => engine.live_midi(snap, inst, bytes),
                        }
                    }
                    if hold.load(Ordering::Relaxed) {
                        // An export is rendering with the plugin instances.
                        data.fill(0.0);
                        return;
                    }
                    if !p.load(Ordering::Relaxed) {
                        if was_playing {
                            // Transport stopped: the song's notes stop at once.
                            engine.stop();
                            was_playing = false;
                        }
                        if engine.busy() {
                            engine.process_live(snap, data);
                        } else {
                            data.fill(0.0);
                        }
                        // Count-in: a bar of clicks, then Play by itself.
                        let left = cin.load(Ordering::Relaxed);
                        if left > 0 {
                            let len = cin_len.load(Ordering::Relaxed).max(left);
                            let frames = (data.len() / 2) as u64;
                            let n = frames.min(left) as usize;
                            count_metro.run(
                                &mut data[..n * 2],
                                len - left,
                                None,
                                snap.tempo,
                                None,
                                &[],
                                rate,
                            );
                            let rest = left - n as u64;
                            // A Stop or Play from the UI meanwhile wins.
                            if cin
                                .compare_exchange(left, rest, Ordering::Relaxed, Ordering::Relaxed)
                                .is_ok()
                                && rest == 0
                            {
                                p.store(true, Ordering::Relaxed);
                            }
                        }
                        ld.store(0, Ordering::Relaxed);
                        vc.store(engine.active_voices() as u32, Ordering::Relaxed);
                        publish_activity(&engine, snap);
                        player.run(&pv, &pd, data, rate);
                        return;
                    }
                    was_playing = true;
                    let started = std::time::Instant::now();
                    let at = pos.load(Ordering::Relaxed);
                    engine.process(
                        snap,
                        i.load(Ordering::Relaxed),
                        m.load(Ordering::Relaxed),
                        at,
                        data,
                        true,
                    );
                    if metro_on.load(Ordering::Relaxed) {
                        let (from, wrap) = engine.block_timeline();
                        let table = engine.tempo_table();
                        delayed(data, &mut clicks, &mut click_pdc, engine.latency(), |out| {
                            metro.run(out, from, wrap, snap.tempo, table, &snap.meters, rate)
                        });
                    }
                    // Monotonic transport clock for MIDI takes: the engine
                    // wraps `next` every loop, but recording needs the absolute
                    // position so the pattern grows past one loop instead of
                    // wrapping every pass into bar 1. A UI seek (Stop rewinds
                    // to 0) still wins if it happened meanwhile.
                    let _ = pos.compare_exchange(
                        at,
                        at + (data.len() / 2) as u64,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    );
                    player.run(&pv, &pd, data, rate);
                    let budget = (data.len() / 2) as f64 / rate as f64;
                    let used = started.elapsed().as_secs_f64() / budget;
                    if used > 1.0 {
                        callback_overruns.fetch_add(1, Ordering::Relaxed);
                    }
                    // Smooth so the number is readable.
                    let prev = ld.load(Ordering::Relaxed) as f64;
                    ld.store((prev * 0.9 + used * 1000.0 * 0.1) as u32, Ordering::Relaxed);
                    vc.store(engine.active_voices() as u32, Ordering::Relaxed);
                    publish_activity(&engine, snap);
                    for (slot, v) in pk.iter().zip(engine.peaks()) {
                        // Hold the loudest until the UI reads and clears it.
                        let old = f32::from_bits(slot.load(Ordering::Relaxed));
                        if *v > old {
                            slot.store(v.to_bits(), Ordering::Relaxed);
                        }
                    }
                    for (slot, v) in gr.iter().zip(engine.reductions()) {
                        if *v > f32::from_bits(slot.load(Ordering::Relaxed)) {
                            slot.store(v.to_bits(), Ordering::Relaxed);
                        }
                    }
                },
                // Once: a broken ALSA stream reports on every poll, millions of times.
                move |err| {
                    if !fail.swap(true, Ordering::Relaxed) {
                        eprintln!("Audio error: {err}");
                    }
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            running,
            link: Mutex::new((fx_tx, link)),
            taps: Mutex::new((tap_cmd_tx, tap_back_rx)),
            plugins: Mutex::new((plug_tx, plug_back_rx)),
            hold_plugins,
            vocal,
            live: Arc::new(Mutex::new(live_tx)),
            playing,
            song_mode,
            pattern,
            position,
            load,
            overruns,
            voices,
            peaks,
            activity,
            reduction,
            scope,
            preview,
            preview_done,
            failed,
            metronome,
            count_in,
            count_in_len,
            _stream: stream,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_wait_for_the_mix_latency() {
        let mut scratch = vec![0.0f32; 64 * 2];
        let mut pdc = keyframexl::plugin::Pdc::default();
        pdc.run(&mut [], 1);
        let click = |out: &mut [f32]| out[0] = 1.0;
        let mut first = vec![0.0f32; 16 * 2];
        delayed(&mut first, &mut scratch, &mut pdc, 20, click);
        assert!(first.iter().all(|x| *x == 0.0), "not yet: 20 frames late");
        let mut second = vec![0.0f32; 16 * 2];
        delayed(&mut second, &mut scratch, &mut pdc, 20, |_| {});
        assert_eq!(second[(20 - 16) * 2], 1.0, "frame 20 overall");
        let mut now = vec![0.0f32; 4];
        delayed(&mut now, &mut scratch, &mut pdc, 0, click);
        assert_eq!(now[0], 1.0, "no latency: straight in");
    }
    #[test]
    fn output_stream_advances_when_enabled() {
        if std::env::var_os("KEYFRAMEXL_TEST_AUDIO").is_none() {
            return;
        }
        let audio = Audio::new(Song::default()).expect("audio output");
        audio.playing.store(true, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(350));
        assert!(
            audio.position.load(Ordering::Relaxed) > 0,
            "output callback did not advance"
        );
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use keyframexl::fx::{FxKind, FxSlot};

    #[test]
    fn auto_buffer_prefers_256_then_512_then_device_default() {
        use cpal::SupportedBufferSize as B;
        let range = |min, max| B::Range { min, max };
        assert_eq!(pick_buffer(None, &range(64, 8192)), Some(256));
        assert_eq!(pick_buffer(None, &range(300, 8192)), Some(512));
        assert_eq!(pick_buffer(None, &range(64, 128)), None);
        assert_eq!(pick_buffer(None, &B::Unknown), None);
        assert_eq!(pick_buffer(Some(1024), &range(64, 8192)), Some(1024));
        assert_eq!(pick_buffer(Some(64), &range(512, 8192)), None);
    }

    /// Real output device: a song sent over the rtrb link after start plays
    /// through an insert with effects and shows on the meters.
    #[test]
    fn songs_sent_over_the_link_reach_the_speakers() {
        if std::env::var_os("KEYFRAMEXL_TEST_AUDIO").is_none() {
            return;
        }
        let audio = Audio::new(Song::default()).expect("audio output");
        let mut song = Song::demo();
        for i in &mut song.instruments {
            i.mixer = 3;
        }
        song.mixer[3].fx.push(FxSlot::new(FxKind::ParamEq));
        song.mixer[3].fx.push(FxSlot::new(FxKind::Stereo));
        song.mixer[3].fx.push(FxSlot::new(FxKind::Reverb));
        song.validate().unwrap();
        audio.send_song(song);
        audio.playing.store(true, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(600));
        audio.tick();
        let peak = |t: usize| f32::from_bits(audio.peaks[t].load(Ordering::Relaxed));
        assert!(peak(3) > 0.01, "insert 3 meter: {}", peak(3));
        assert!(peak(0) > 0.01, "master meter: {}", peak(0));
        assert_eq!(peak(5), 0.0, "an unused insert stays silent");
        eprintln!("live peaks: master {:.3}, insert 3 {:.3}", peak(0), peak(3));
    }
}

#[cfg(test)]
mod live_record_tests {
    /// Real output device: arm Master, Record on, play, stop; the take holds the song.
    #[test]
    fn armed_master_records_a_take_while_playing() {
        if std::env::var_os("KEYFRAMEXL_TEST_AUDIO").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: test-only, set before any thread reads it.
        unsafe { std::env::set_var("KEYFRAMEXL_RECORDINGS", dir.path()) };
        let song = keyframexl::sequence::Song::demo();
        let audio = super::Audio::new(song.clone()).expect("audio output");
        let mut app = crate::ui::App::with_song(song, Some(audio));
        app.toggle_arm(0);
        app.toggle_record();
        app.play_pause();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(700);
        while std::time::Instant::now() < until {
            app.record_frame();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        app.stop();
        app.record_frame();
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(files.len(), 1, "one take: {files:?}");
        let (_, frames) = crate::import::read_audio(&files[0]).unwrap();
        let secs = frames.len() as f64 / 48_000.0;
        let peak = frames
            .iter()
            .map(|f| f[0].abs().max(f[1].abs()))
            .fold(0.0f32, f32::max);
        eprintln!(
            "live take: {} ({secs:.2} s, peak {peak:.3})",
            files[0].display()
        );
        assert!(secs > 0.5, "about the time it played: {secs}");
        assert!(peak > 0.05, "the take has sound");
    }
}

#[cfg(test)]
mod metronome_tests {
    use super::Metronome;
    use keyframexl::sequence::BAR;
    use keyframexl::tempo::{Meter, TempoMap, TempoPoint, TempoShape::*, TempoTable};

    /// Frames where a click starts, over `frames`, in 256-frame blocks.
    fn clicks(map: Option<&TempoTable>, meters: &[Meter], frames: u64) -> Vec<u64> {
        let mut m = Metronome::default();
        let mut out = Vec::new();
        let mut at = 0;
        let mut prev = 0.0f32;
        while at < frames {
            let mut block = vec![0.0f32; 512];
            m.run(&mut block, at, None, 120.0, map, meters, 48_000);
            for (i, f) in block.chunks_exact(2).enumerate() {
                // A click is a sine from phase 0: its first sample is 0, so it began one frame earlier.
                if f[0] != 0.0 && prev == 0.0 {
                    out.push(at + i as u64 - 1);
                }
                prev = f[0];
            }
            at += 256;
        }
        out
    }

    #[test]
    fn clicks_follow_the_tempo_map_including_ramps() {
        // Flat 120: a click every 0.5 s.
        let flat = clicks(None, &[], 48_000 * 3);
        assert_eq!(&flat[..4], &[0, 24_000, 48_000, 72_000]);
        // 120, then 240 from bar 2: beats every 0.25 s after 2 s, none lost.
        let jump = TempoTable::build(
            &TempoMap {
                points: vec![
                    TempoPoint {
                        tick: 0,
                        bpm: 120.0,
                        shape: Step,
                    },
                    TempoPoint {
                        tick: BAR,
                        bpm: 240.0,
                        shape: Step,
                    },
                ],
            },
            120.0,
            48_000,
        );
        let c = clicks(Some(&jump), &[], 48_000 * 3);
        assert_eq!(
            &c[..7],
            &[0, 24_000, 48_000, 72_000, 96_000, 108_000, 120_000]
        );
        // A ramp: every beat clicks exactly once, intervals shrink.
        let ramp = TempoTable::build(
            &TempoMap {
                points: vec![
                    TempoPoint {
                        tick: 0,
                        bpm: 90.0,
                        shape: Linear,
                    },
                    TempoPoint {
                        tick: 4 * BAR,
                        bpm: 180.0,
                        shape: Step,
                    },
                ],
            },
            90.0,
            48_000,
        );
        let c = clicks(Some(&ramp), &[], ramp.frames_at(4 * BAR) as u64);
        // The last block runs past bar 4, so beat 17 (bar 5) may show too.
        assert!((16..=17).contains(&c.len()), "16 beats in 4 bars: {c:?}");
        for (k, f) in c.iter().enumerate() {
            assert_eq!(*f, ramp.frames_at_ceil(k as i64 * BAR / 4), "beat {k}");
        }
        assert!(c[15] - c[14] < c[1] - c[0], "faster at the end");
    }

    #[test]
    fn clicks_count_the_time_signature() {
        // 120 BPM, 3/4 from bar 1 then 7/8 from bar 3: quarters, then eighths.
        let meters = [
            Meter {
                bar: 0,
                num: 3,
                den: 4,
            },
            Meter {
                bar: 2,
                num: 7,
                den: 8,
            },
        ];
        let c = clicks(None, &meters, 48_000 * 5);
        let q: u64 = 24_000; // a quarter at 120 BPM
        assert_eq!(
            &c[..8],
            &[0, q, 2 * q, 3 * q, 4 * q, 5 * q, 6 * q, 6 * q + q / 2],
            "3/4 twice, then eighths"
        );
        // The accent (higher click) falls on bar starts: count zero crossings over 400 frames
        // (1760 Hz ≈ 29, 1320 Hz ≈ 22).
        let crossings = |at: u64| {
            let mut m = Metronome::default();
            let mut block = vec![0.0f32; 2 * 600];
            m.run(&mut block, at - 100, None, 120.0, None, &meters, 48_000);
            let s: Vec<f32> = block
                .chunks_exact(2)
                .skip(101)
                .take(400)
                .map(|f| f[0])
                .collect();
            s.windows(2)
                .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                .count()
        };
        assert!(
            crossings(3 * q) >= 27,
            "bar 2 (3/4) starts high: {}",
            crossings(3 * q)
        );
        assert!(
            crossings(4 * q) <= 24,
            "its second beat is low: {}",
            crossings(4 * q)
        );
        assert!(crossings(6 * q) >= 27, "bar 3 (7/8) starts high");
        assert!(crossings(6 * q + q / 2) <= 24, "then low eighths");
    }
}
