use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use keyframexl::{
    engine::{Engine, Tap},
    link::{FxSender, ParamSender, SongSender, fx_link, param_link, song_link},
    sequence::Song,
};
use std::sync::{
    Arc, Mutex, OnceLock,
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

/// The sound server an output device reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioBackend {
    PipeWire,
    PulseAudio,
    Alsa,
}

impl AudioBackend {
    pub fn label(self) -> &'static str {
        match self {
            AudioBackend::PipeWire => "PipeWire",
            AudioBackend::PulseAudio => "PulseAudio",
            AudioBackend::Alsa => "ALSA",
        }
    }
}

/// Backend behind a cpal (ALSA host) device name. `default` and `sysdefault`
/// reach PipeWire when its daemon runs; direct `hw:` / `plughw:` names stay ALSA.
pub fn detect_backend(device_name: &str, pipewire_running: bool) -> AudioBackend {
    let n = device_name.to_ascii_lowercase();
    if n.contains("pipewire") {
        AudioBackend::PipeWire
    } else if n.contains("pulse") {
        AudioBackend::PulseAudio
    } else if pipewire_running && (n.is_empty() || n == "default" || n.starts_with("sysdefault")) {
        AudioBackend::PipeWire
    } else {
        AudioBackend::Alsa
    }
}

/// Is a PipeWire socket in one of `dirs`? (Pure, so tests need no daemon.)
fn pipewire_socket_in(dirs: impl IntoIterator<Item = std::path::PathBuf>) -> bool {
    dirs.into_iter().any(|d| {
        let sock = d.join("pipewire-0");
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            std::fs::metadata(&sock).is_ok_and(|m| m.file_type().is_socket())
        }
        #[cfg(not(unix))]
        {
            sock.exists()
        }
    })
}

/// The PipeWire daemon's socket exists (a stat, never a connect: cannot block).
pub fn pipewire_daemon_active() -> bool {
    let dirs = ["PIPEWIRE_RUNTIME_DIR", "XDG_RUNTIME_DIR"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from);
    pipewire_socket_in(dirs)
}

/// `PIPEWIRE_LATENCY` value (`frames/rate`) that asks PipeWire for exactly this quantum.
pub fn pipewire_latency_hint(buffer: Option<u32>, rate: u32) -> Option<String> {
    match buffer {
        Some(b) if rate > 0 => Some(format!("{b}/{rate}")),
        _ => None,
    }
}

/// Did the user launch us with their own `PIPEWIRE_LATENCY`? Read once, before
/// this module ever touches the variable; their value is never overridden.
fn user_set_pipewire_latency() -> bool {
    static USER: OnceLock<bool> = OnceLock::new();
    *USER.get_or_init(|| std::env::var_os("PIPEWIRE_LATENCY").is_some())
}

/// Order in which `Audio::open` tries output devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Named,
    PipeWire,
    Default,
}

fn pick_order(named_found: bool, pipewire_active: bool, pipewire_device: bool) -> Vec<Pick> {
    let mut v = Vec::new();
    if named_found {
        v.push(Pick::Named);
    }
    if pipewire_active && pipewire_device {
        v.push(Pick::PipeWire);
    }
    v.push(Pick::Default);
    v
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
    /// Which sound server the device goes through.
    pub backend: AudioBackend,
    /// The named output in the settings was missing, so the default (or the
    /// PipeWire device) is playing instead: the recovery watches for its return.
    pub fallback: bool,
}

impl Running {
    /// Output latency of one buffer in milliseconds (unknown buffer: the usual 1024).
    pub fn latency_ms(&self) -> f64 {
        self.buffer.unwrap_or(1024) as f64 / self.rate as f64 * 1000.0
    }
}

/// What the recovery sees of the running stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamHealth {
    /// The error callback fired (`Audio::failed`).
    pub failed: bool,
    /// `Audio::callbacks` now.
    pub callbacks: u64,
    /// `Running::fallback`: not the output the settings name.
    pub fallback: bool,
    /// `(playing, position)` to carry over a reconnect.
    pub transport: (bool, u64),
}

/// Why a reconnect is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconnect {
    /// The stream reported an error.
    Error,
    /// Callbacks stopped coming with no error (the sound server restarted).
    Stalled,
    /// The output named in the settings is back after a fallback.
    PreferredBack,
    /// No stream and an output device is there again.
    DeviceBack,
    /// No stream; the backoff timer ran out.
    Retry,
}

/// Output hot-reconnect state machine. Pure: time comes in as seconds, the
/// device probe as a closure, so it runs without audio hardware. The UI calls
/// `observe` once per frame and `reconnected` with each reopen's outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioRecovery {
    /// Armed once a stream has run (or a restart failed): a run that never had
    /// audio (headless, tests) is left alone.
    pub armed: bool,
    /// When the next reopen is due (seconds), while disconnected.
    pub retry_at: Option<f64>,
    /// Wait after the next failed reopen: 0.5 s, doubling to 5 s; 0.5 s again once healthy.
    pub backoff_secs: f64,
    pub attempts: u32,
    /// `(playing, position)` from the last live stream, kept across failed reopens.
    pub saved_transport: Option<(bool, u64)>,
    pub last_callbacks: u64,
    pub last_callback_time: f64,
    pub last_device_poll: f64,
    /// What the stream's error callback said, or the last reopen's error.
    pub last_error: Option<String>,
    opened_at: f64,
    device_present: bool,
}

impl Default for AudioRecovery {
    fn default() -> Self {
        Self {
            armed: false,
            retry_at: None,
            backoff_secs: Self::BACKOFF_MIN,
            attempts: 0,
            saved_transport: None,
            last_callbacks: 0,
            last_callback_time: 0.0,
            last_device_poll: f64::NEG_INFINITY,
            last_error: None,
            opened_at: 0.0,
            device_present: false,
        }
    }
}

impl AudioRecovery {
    pub const BACKOFF_MIN: f64 = 0.5;
    pub const BACKOFF_MAX: f64 = 5.0;
    /// Seconds without a callback (on a stream at least this old) that count as a stall.
    pub const STALL_SECS: f64 = 2.0;
    /// Seconds between looks for a returned device.
    pub const POLL_SECS: f64 = 2.0;

    /// Is a reopen pending (stream gone, failed or stalled)?
    pub fn recovering(&self) -> bool {
        self.retry_at.is_some()
    }

    fn begin(&mut self, now: f64, transport: Option<(bool, u64)>, error: Option<String>) {
        if transport.is_some() {
            self.saved_transport = transport;
        }
        if error.is_some() {
            self.last_error = error;
        }
        self.retry_at = Some(now);
        self.device_present = false;
        self.last_device_poll = now;
    }

    /// Call once per frame. `health`: the running stream, or `None` without one.
    /// `error`: the stream's error text (read only when it failed).
    /// `device_present`: whether the preferred device (while on a fallback) or
    /// any output (without a stream) is listed; called at most every `POLL_SECS`.
    pub fn observe(
        &mut self,
        now: f64,
        health: Option<StreamHealth>,
        error: impl FnOnce() -> Option<String>,
        device_present: impl FnOnce() -> bool,
    ) -> Option<Reconnect> {
        let poll_due = now - self.last_device_poll >= Self::POLL_SECS;
        let Some(h) = health else {
            if !self.armed {
                return None;
            }
            if self.retry_at.is_none() {
                // Armed with no stream and no timer: a reopen failed outside us.
                self.begin(now, None, None);
            }
            if self.retry_at.is_some_and(|t| t <= now) {
                return Some(Reconnect::Retry);
            }
            if poll_due {
                self.last_device_poll = now;
                let present = device_present();
                let edge = present && !self.device_present;
                self.device_present = present;
                if edge {
                    return Some(Reconnect::DeviceBack);
                }
            }
            return None;
        };
        if !self.armed {
            self.armed = true;
            self.opened_at = now;
            self.last_callbacks = h.callbacks;
            self.last_callback_time = now;
        }
        if h.failed {
            self.begin(now, Some(h.transport), error());
            return Some(Reconnect::Error);
        }
        if h.callbacks != self.last_callbacks {
            self.last_callbacks = h.callbacks;
            self.last_callback_time = now;
            // Healthy: forget the trouble.
            self.retry_at = None;
            self.backoff_secs = Self::BACKOFF_MIN;
            self.attempts = 0;
            self.saved_transport = None;
            self.last_error = None;
        } else if now - self.opened_at >= Self::STALL_SECS
            && now - self.last_callback_time >= Self::STALL_SECS
        {
            self.begin(
                now,
                Some(h.transport),
                Some("The audio callback stopped (the sound server may have restarted).".into()),
            );
            return Some(Reconnect::Stalled);
        }
        if h.fallback && poll_due {
            self.last_device_poll = now;
            let present = device_present();
            let edge = present && !self.device_present;
            self.device_present = present;
            if edge {
                self.saved_transport = Some(h.transport);
                return Some(Reconnect::PreferredBack);
            }
        }
        None
    }

    /// The outcome of the reopen `observe` asked for (or of a manual one).
    pub fn reconnected(&mut self, now: f64, ok: bool, error: Option<String>) {
        self.armed = true;
        if ok {
            self.retry_at = None;
            self.opened_at = now;
            self.last_callbacks = 0;
            self.last_callback_time = now;
            self.last_device_poll = now;
        } else {
            self.attempts += 1;
            self.retry_at = Some(now + self.backoff_secs);
            self.backoff_secs = (self.backoff_secs * 2.0).min(Self::BACKOFF_MAX);
            if error.is_some() {
                self.last_error = error;
            }
        }
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
    /// Edited songs and live parameter tweaks go to the audio thread over `rtrb`
    /// rings (only the UI thread locks this; the audio thread owns the receivers).
    pub link: Mutex<(FxSender, SongSender, ParamSender)>,
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
    /// Master bus's last frames, for the Deck oscilloscope / spectrum monitor.
    pub master_scope: Arc<keyframexl::engine::Scope>,
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
    /// Audio callbacks run so far: stops advancing when the stream stalls
    /// without an error (a PipeWire restart can do that).
    pub callbacks: Arc<AtomicU64>,
    /// What the stream's error callback reported when `failed` flipped.
    pub last_error: Arc<Mutex<Option<String>>>,
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

    /// Stream one live parameter edit lock-free without cloning `Song`.
    /// Returns `false` if the ring is full (caller can fall back to `send_song`).
    pub fn send_param(&self, target: keyframexl::automation::Target, value: f32) -> bool {
        self.link.lock().is_ok_and(|mut l| {
            let generation = l.1.generation();
            l.2.send(generation, target, value)
        })
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
        let pipewire = pipewire_daemon_active();
        let outputs: Vec<cpal::Device> = host
            .output_devices()
            .map(|d| d.collect())
            .unwrap_or_default();
        let named_at = cfg.output.as_ref().and_then(|want| {
            outputs
                .iter()
                .position(|d| d.name().ok().as_ref() == Some(want))
        });
        let pipewire_at = outputs
            .iter()
            .position(|d| d.name().is_ok_and(|n| n == "pipewire"));
        // The first candidate that offers stereo f32 wins; the named one first,
        // then (with a PipeWire daemon) its own ALSA device, then the default.
        let mut chosen = None;
        let mut last_err = String::from("No default audio output");
        for pick_kind in pick_order(named_at.is_some(), pipewire, pipewire_at.is_some()) {
            let device = match pick_kind {
                Pick::Named => named_at.map(|i| outputs[i].clone()),
                Pick::PipeWire => pipewire_at.map(|i| outputs[i].clone()),
                Pick::Default => host.default_output_device(),
            };
            let Some(device) = device else { continue };
            let name = device.name().unwrap_or_else(|_| "Default".into());
            match device.supported_output_configs() {
                Ok(c) => {
                    let configs: Vec<_> = c.collect();
                    let stereo = |rate: u32| {
                        configs.iter().any(|c| {
                            c.sample_format() == cpal::SampleFormat::F32
                                && c.channels() == 2
                                && c.min_sample_rate().0 <= rate
                                && c.max_sample_rate().0 >= rate
                        })
                    };
                    if stereo(cfg.rate) || stereo(48_000) {
                        chosen = Some((device, name, configs, pick_kind != Pick::Named));
                        break;
                    }
                    last_err =
                        format!("{name} does not offer stereo f32 at {} or 48 kHz", cfg.rate);
                }
                Err(e) => last_err = format!("{name}: {e}"),
            }
        }
        let Some((device, device_name, configs, substitute)) = chosen else {
            return Err(last_err);
        };
        let fallback = cfg.output.is_some() && substitute;
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
        let backend = detect_backend(&device_name, pipewire);
        if backend == AudioBackend::PipeWire && !user_set_pipewire_latency() {
            // SAFETY: `open` runs on the UI thread, before this stream's threads
            // start; PipeWire's client library reads the variable when the stream
            // connects, and the app sets it nowhere else.
            unsafe {
                match pipewire_latency_hint(buffer, rate) {
                    Some(v) => std::env::set_var("PIPEWIRE_LATENCY", v),
                    None => std::env::remove_var("PIPEWIRE_LATENCY"),
                }
            }
        }
        let running = Running {
            device: device_name,
            rate,
            buffer,
            backend,
            fallback,
        };
        let (mut fx_tx, mut fx_rx) = fx_link(rate);
        fx_tx.sync(&song);
        let first_song = song.clone();
        let (link, mut rx) = song_link(song);
        let (param_tx, mut param_rx) = param_link();
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
        let master_scope = engine.master_scope();
        let failed = Arc::new(AtomicBool::new(false));
        let fail = failed.clone();
        let callbacks = Arc::new(AtomicU64::new(0));
        let tick = callbacks.clone();
        let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let err_slot = last_error.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    tick.fetch_add(1, Ordering::Relaxed);
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
                    if rx.update_with_changed().1 {
                        engine.clear_live_params();
                    }
                    param_rx.drain(rx.generation(), &mut engine);
                    let snap = rx.current();
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
                        if let Ok(mut slot) = err_slot.lock() {
                            *slot = Some(err.to_string());
                        }
                    }
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            running,
            link: Mutex::new((fx_tx, link, param_tx)),
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
            master_scope,
            preview,
            preview_done,
            failed,
            callbacks,
            last_error,
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
    fn backend_follows_the_device_name() {
        use AudioBackend::*;
        assert_eq!(detect_backend("pipewire", false), PipeWire);
        assert_eq!(detect_backend("pulse", true), PulseAudio);
        assert_eq!(detect_backend("default", true), PipeWire);
        assert_eq!(detect_backend("sysdefault:CARD=PCH", true), PipeWire);
        assert_eq!(detect_backend("default", false), Alsa);
        assert_eq!(detect_backend("hw:CARD=PCH,DEV=0", true), Alsa);
        assert_eq!(detect_backend("plughw:CARD=USB", true), Alsa);
        assert_eq!(PipeWire.label(), "PipeWire");
        assert_eq!(PulseAudio.label(), "PulseAudio");
        assert_eq!(Alsa.label(), "ALSA");
    }

    #[test]
    fn latency_hint_needs_a_buffer_and_a_rate() {
        assert_eq!(
            pipewire_latency_hint(Some(256), 48_000).as_deref(),
            Some("256/48000")
        );
        assert_eq!(
            pipewire_latency_hint(Some(64), 44_100).as_deref(),
            Some("64/44100")
        );
        assert_eq!(pipewire_latency_hint(None, 48_000), None);
        assert_eq!(pipewire_latency_hint(Some(256), 0), None);
    }

    #[test]
    fn device_order_prefers_pipewire_and_always_ends_at_default() {
        use Pick::*;
        assert_eq!(pick_order(true, true, true), [Named, PipeWire, Default]);
        assert_eq!(pick_order(false, true, true), [PipeWire, Default]);
        assert_eq!(pick_order(false, true, false), [Default]);
        assert_eq!(pick_order(false, false, true), [Default]);
        assert_eq!(pick_order(true, false, false), [Named, Default]);
    }

    #[cfg(unix)]
    #[test]
    fn pipewire_socket_is_found_by_stat() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!pipewire_socket_in([dir.path().to_path_buf()]));
        std::fs::write(dir.path().join("pipewire-0"), b"").unwrap();
        assert!(
            !pipewire_socket_in([dir.path().to_path_buf()]),
            "a plain file is no socket"
        );
        std::fs::remove_file(dir.path().join("pipewire-0")).unwrap();
        let _l = std::os::unix::net::UnixListener::bind(dir.path().join("pipewire-0")).unwrap();
        assert!(pipewire_socket_in([
            std::path::PathBuf::from("/nonexistent"),
            dir.path().to_path_buf()
        ]));
    }

    fn live(callbacks: u64) -> StreamHealth {
        StreamHealth {
            failed: false,
            callbacks,
            fallback: false,
            transport: (true, 1000),
        }
    }
    fn watch(
        r: &mut AudioRecovery,
        now: f64,
        h: Option<StreamHealth>,
        present: bool,
    ) -> Option<Reconnect> {
        r.observe(now, h, || Some("boom".into()), || present)
    }

    #[test]
    fn recovery_leaves_a_run_without_audio_alone() {
        let mut r = AudioRecovery::default();
        for i in 0..20 {
            assert_eq!(watch(&mut r, i as f64, None, true), None);
        }
        assert!(!r.recovering());
    }

    #[test]
    fn backoff_doubles_to_the_cap_and_resets_when_healthy() {
        let mut r = AudioRecovery::default();
        assert_eq!(watch(&mut r, 0.0, Some(live(10)), false), None);
        let mut failed = live(10);
        failed.failed = true;
        assert_eq!(
            watch(&mut r, 1.0, Some(failed), false),
            Some(Reconnect::Error)
        );
        assert_eq!(r.last_error.as_deref(), Some("boom"));
        let mut now = 1.0;
        let mut waits = Vec::new();
        for _ in 0..6 {
            assert_eq!(watch(&mut r, now, None, false), Some(Reconnect::Retry));
            r.reconnected(now, false, Some("busy".into()));
            waits.push(r.retry_at.unwrap() - now);
            // Not due a moment before the timer.
            assert_eq!(watch(&mut r, now + 0.01, None, false), None);
            now = r.retry_at.unwrap();
        }
        assert_eq!(waits, [0.5, 1.0, 2.0, 4.0, 5.0, 5.0]);
        assert_eq!(r.attempts, 6);
        assert_eq!(r.last_error.as_deref(), Some("busy"));
        // A reopen works and callbacks come: all forgotten.
        assert_eq!(watch(&mut r, now, None, false), Some(Reconnect::Retry));
        r.reconnected(now, true, None);
        assert!(!r.recovering());
        assert_eq!(watch(&mut r, now + 0.1, Some(live(3)), false), None);
        assert_eq!(r.backoff_secs, AudioRecovery::BACKOFF_MIN);
        assert_eq!(
            (r.attempts, r.saved_transport, r.last_error.clone()),
            (0, None, None)
        );
    }

    #[test]
    fn a_silent_stream_counts_as_stalled_after_two_seconds() {
        let mut r = AudioRecovery::default();
        assert_eq!(watch(&mut r, 0.0, Some(live(100)), false), None);
        assert_eq!(watch(&mut r, 0.5, Some(live(130)), false), None);
        // Counter frozen at 130 since 0.5 s.
        assert_eq!(watch(&mut r, 2.4, Some(live(130)), false), None);
        assert_eq!(
            watch(&mut r, 2.5, Some(live(130)), false),
            Some(Reconnect::Stalled)
        );
        assert_eq!(r.saved_transport, Some((true, 1000)));
        assert!(r.recovering());
    }

    #[test]
    fn a_young_or_advancing_stream_is_not_stalled() {
        let mut r = AudioRecovery::default();
        // Reopened at 10 s: no callbacks yet for 1.9 s is still fine.
        r.reconnected(10.0, true, None);
        assert_eq!(watch(&mut r, 11.9, Some(live(0)), false), None);
        assert_eq!(
            watch(&mut r, 12.0, Some(live(0)), false),
            Some(Reconnect::Stalled)
        );
        // A counter that keeps moving never stalls.
        let mut r = AudioRecovery::default();
        for i in 0..50u64 {
            assert_eq!(watch(&mut r, i as f64, Some(live(i * 40)), false), None);
        }
    }

    #[test]
    fn the_preferred_device_coming_back_reconnects_once() {
        let mut r = AudioRecovery::default();
        let mut fb = live(0);
        fb.fallback = true;
        let mut n = 0;
        let mut step = |r: &mut AudioRecovery, now: f64, present: bool| {
            n += 100;
            let mut h = fb;
            h.callbacks = n;
            watch(r, now, Some(h), present)
        };
        assert_eq!(step(&mut r, 0.0, false), None);
        // Polled only every 2 s, and absent means nothing happens.
        assert_eq!(step(&mut r, 1.0, true), None, "not polled yet");
        assert_eq!(step(&mut r, 2.0, false), None);
        assert_eq!(step(&mut r, 3.0, true), None, "not polled yet");
        assert_eq!(step(&mut r, 4.0, true), Some(Reconnect::PreferredBack));
        assert_eq!(r.saved_transport, Some((true, 1000)));
        // Still on the fallback after the reopen: no reconnect loop.
        r.reconnected(4.0, true, None);
        for t in [5.0, 6.0, 8.0, 10.0, 12.0] {
            assert_eq!(step(&mut r, t, true), None, "t={t}");
        }
        // A healthy, non-fallback stream never polls.
        let mut ok = AudioRecovery::default();
        for i in 0..10u64 {
            let out = ok.observe(
                i as f64 * 3.0,
                Some(live(i + 1)),
                || None,
                || panic!("polled"),
            );
            assert_eq!(out, None);
        }
    }

    #[test]
    fn a_due_timer_retries_even_while_no_device_is_listed() {
        let mut r = AudioRecovery::default();
        watch(&mut r, 0.0, Some(live(5)), false);
        let mut failed = live(5);
        failed.failed = true;
        watch(&mut r, 1.0, Some(failed), false);
        assert_eq!(watch(&mut r, 1.0, None, false), Some(Reconnect::Retry));
        r.reconnected(1.0, false, None);
        assert_eq!(r.retry_at, Some(1.5));
        assert_eq!(watch(&mut r, 1.4, None, false), None);
        assert_eq!(watch(&mut r, 1.5, None, false), Some(Reconnect::Retry));
    }

    #[test]
    fn device_edge_triggers_before_the_timer() {
        let mut r = AudioRecovery::default();
        r.reconnected(0.0, false, None); // armed, nothing running
        r.retry_at = Some(100.0);
        assert_eq!(watch(&mut r, 1.0, None, false), None);
        assert_eq!(watch(&mut r, 3.0, None, true), Some(Reconnect::DeviceBack));
        // Same presence, next poll: no second trigger.
        assert_eq!(watch(&mut r, 5.0, None, true), None);
    }

    #[test]
    fn transport_survives_failed_reopens() {
        let mut r = AudioRecovery::default();
        watch(&mut r, 0.0, Some(live(7)), false);
        let mut failed = live(7);
        failed.failed = true;
        failed.transport = (true, 123_456);
        assert_eq!(
            watch(&mut r, 1.0, Some(failed), false),
            Some(Reconnect::Error)
        );
        let mut now = 1.0;
        for _ in 0..4 {
            assert_eq!(watch(&mut r, now, None, false), Some(Reconnect::Retry));
            r.reconnected(now, false, None);
            now = r.retry_at.unwrap();
            assert_eq!(r.saved_transport, Some((true, 123_456)));
        }
        // The reopen works: still saved for the restore, cleared once callbacks flow.
        r.reconnected(now, true, None);
        assert_eq!(r.saved_transport, Some((true, 123_456)));
        assert_eq!(watch(&mut r, now + 0.1, Some(live(1)), false), None);
        assert_eq!(r.saved_transport, None);
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
