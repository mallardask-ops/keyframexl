#![allow(dead_code)]
//! Native CLAP 1.2 plugin host (Linux and Windows).
//!
//! `libloading` loads the `.clap` dynamic library, the `#[repr(C)]` structs
//! below mirror the CLAP 1.2 C headers, and on Unix `libc::poll` serves
//! `clap.posix-fd-support`. One `ClapPlugin` is one plugin instance, driven
//! from one thread at a time (behind a mutex), the same contract as
//! `Vst2Plugin`.
//!
//! A `.clap` file may hold several plugins: `Bundle.clap#org.example.synth`
//! names one of them (`split_path` / `join_path`).

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_int, c_ulong, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
const GUI_API: &CStr = c"win32";
#[cfg(not(target_os = "windows"))]
const GUI_API: &CStr = c"x11";

// ---------------------------------------------------------------------------
// CLAP 1.2 C ABI
// ---------------------------------------------------------------------------

const NAME_SIZE: usize = 256;
const PATH_SIZE: usize = 1024;
/// `CLAP_BEATTIME_FACTOR` / `CLAP_SECTIME_FACTOR`: 31-bit fixed point.
const FIXED_ONE: i64 = 1 << 31;

const EVENT_NOTE_ON: u16 = 0;
const EVENT_NOTE_OFF: u16 = 1;
const EVENT_PARAM_VALUE: u16 = 5;
const EVENT_MIDI: u16 = 10;

const TRANSPORT_HAS_TEMPO: u32 = 1 << 0;
const TRANSPORT_HAS_BEATS_TIMELINE: u32 = 1 << 1;
const TRANSPORT_HAS_SECONDS_TIMELINE: u32 = 1 << 2;
const TRANSPORT_HAS_TIME_SIGNATURE: u32 = 1 << 3;
const TRANSPORT_IS_PLAYING: u32 = 1 << 4;

const DIALECT_CLAP: u32 = 1 << 0;
const DIALECT_MIDI: u32 = 1 << 1;

const PARAM_IS_STEPPED: u32 = 1 << 0;
const PARAM_IS_HIDDEN: u32 = 1 << 2;
const PARAM_IS_READONLY: u32 = 1 << 3;
const PARAM_IS_AUTOMATABLE: u32 = 1 << 5;

const FD_READ: u32 = 1 << 0;
const FD_WRITE: u32 = 1 << 1;
const FD_ERROR: u32 = 1 << 2;

const LOG_WARNING: i32 = 2;

const PROCESS_ERROR: i32 = 0;

/// Most events one block passes to the plugin.
pub const MAX_EVENTS: usize = 512;
/// Most parameter changes a plugin can report in one block.
const MAX_OUT_PARAMS: usize = 256;
/// Timers fire no faster than this.
const MIN_TIMER_MS: u32 = 10;

#[repr(C)]
#[derive(Clone, Copy)]
struct Version {
    major: u32,
    minor: u32,
    revision: u32,
}
const HOST_VERSION: Version = Version {
    major: 1,
    minor: 2,
    revision: 2,
};

#[repr(C)]
pub(crate) struct Entry {
    clap_version: Version,
    init: Option<unsafe extern "C" fn(*const c_char) -> bool>,
    deinit: Option<unsafe extern "C" fn()>,
    get_factory: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
struct Factory {
    get_plugin_count: Option<unsafe extern "C" fn(*const Factory) -> u32>,
    get_plugin_descriptor: Option<unsafe extern "C" fn(*const Factory, u32) -> *const Descriptor>,
    create_plugin: Option<
        unsafe extern "C" fn(*const Factory, *const RawHost, *const c_char) -> *const RawPlugin,
    >,
}

#[repr(C)]
struct Descriptor {
    clap_version: Version,
    id: *const c_char,
    name: *const c_char,
    vendor: *const c_char,
    url: *const c_char,
    manual_url: *const c_char,
    support_url: *const c_char,
    version: *const c_char,
    description: *const c_char,
    features: *const *const c_char,
}

#[repr(C)]
struct RawPlugin {
    desc: *const Descriptor,
    plugin_data: *mut c_void,
    init: Option<unsafe extern "C" fn(*const RawPlugin) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const RawPlugin)>,
    activate: Option<unsafe extern "C" fn(*const RawPlugin, f64, u32, u32) -> bool>,
    deactivate: Option<unsafe extern "C" fn(*const RawPlugin)>,
    start_processing: Option<unsafe extern "C" fn(*const RawPlugin) -> bool>,
    stop_processing: Option<unsafe extern "C" fn(*const RawPlugin)>,
    reset: Option<unsafe extern "C" fn(*const RawPlugin)>,
    process: Option<unsafe extern "C" fn(*const RawPlugin, *const Process) -> i32>,
    get_extension: Option<unsafe extern "C" fn(*const RawPlugin, *const c_char) -> *const c_void>,
    on_main_thread: Option<unsafe extern "C" fn(*const RawPlugin)>,
}

#[repr(C)]
struct RawHost {
    clap_version: Version,
    host_data: *mut c_void,
    name: *const c_char,
    vendor: *const c_char,
    url: *const c_char,
    version: *const c_char,
    get_extension: Option<unsafe extern "C" fn(*const RawHost, *const c_char) -> *const c_void>,
    request_restart: Option<unsafe extern "C" fn(*const RawHost)>,
    request_process: Option<unsafe extern "C" fn(*const RawHost)>,
    request_callback: Option<unsafe extern "C" fn(*const RawHost)>,
}

#[repr(C)]
struct AudioBuffer {
    data32: *mut *mut f32,
    data64: *mut *mut f64,
    channel_count: u32,
    latency: u32,
    constant_mask: u64,
}

#[repr(C)]
struct Process {
    steady_time: i64,
    frames_count: u32,
    transport: *const TransportEvent,
    audio_inputs: *const AudioBuffer,
    audio_outputs: *mut AudioBuffer,
    audio_inputs_count: u32,
    audio_outputs_count: u32,
    in_events: *const InputEvents,
    out_events: *const OutputEvents,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct EventHeader {
    size: u32,
    time: u32,
    space_id: u16,
    type_: u16,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NoteEvent {
    header: EventHeader,
    note_id: i32,
    port_index: i16,
    channel: i16,
    key: i16,
    velocity: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MidiEvent {
    header: EventHeader,
    port_index: u16,
    data: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ParamValueEvent {
    header: EventHeader,
    param_id: u32,
    cookie: *mut c_void,
    note_id: i32,
    port_index: i16,
    channel: i16,
    key: i16,
    value: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TransportEvent {
    header: EventHeader,
    flags: u32,
    song_pos_beats: i64,
    song_pos_seconds: i64,
    tempo: f64,
    tempo_inc: f64,
    loop_start_beats: i64,
    loop_end_beats: i64,
    loop_start_seconds: i64,
    loop_end_seconds: i64,
    bar_start: i64,
    bar_number: i32,
    tsig_num: u16,
    tsig_denom: u16,
}

#[repr(C)]
struct InputEvents {
    ctx: *mut c_void,
    size: Option<unsafe extern "C" fn(*const InputEvents) -> u32>,
    get: Option<unsafe extern "C" fn(*const InputEvents, u32) -> *const EventHeader>,
}

#[repr(C)]
struct OutputEvents {
    ctx: *mut c_void,
    try_push: Option<unsafe extern "C" fn(*const OutputEvents, *const EventHeader) -> bool>,
}

#[repr(C)]
struct AudioPortInfo {
    id: u32,
    name: [u8; NAME_SIZE],
    flags: u32,
    channel_count: u32,
    port_type: *const c_char,
    in_place_pair: u32,
}

#[repr(C)]
struct PluginAudioPorts {
    count: Option<unsafe extern "C" fn(*const RawPlugin, bool) -> u32>,
    get: Option<unsafe extern "C" fn(*const RawPlugin, u32, bool, *mut AudioPortInfo) -> bool>,
}

#[repr(C)]
struct NotePortInfo {
    id: u32,
    supported_dialects: u32,
    preferred_dialect: u32,
    name: [u8; NAME_SIZE],
}

#[repr(C)]
struct PluginNotePorts {
    count: Option<unsafe extern "C" fn(*const RawPlugin, bool) -> u32>,
    get: Option<unsafe extern "C" fn(*const RawPlugin, u32, bool, *mut NotePortInfo) -> bool>,
}

#[repr(C)]
struct RawParamInfo {
    id: u32,
    flags: u32,
    cookie: *mut c_void,
    name: [u8; NAME_SIZE],
    module: [u8; PATH_SIZE],
    min_value: f64,
    max_value: f64,
    default_value: f64,
}

#[repr(C)]
struct PluginParams {
    count: Option<unsafe extern "C" fn(*const RawPlugin) -> u32>,
    get_info: Option<unsafe extern "C" fn(*const RawPlugin, u32, *mut RawParamInfo) -> bool>,
    get_value: Option<unsafe extern "C" fn(*const RawPlugin, u32, *mut f64) -> bool>,
    value_to_text:
        Option<unsafe extern "C" fn(*const RawPlugin, u32, f64, *mut c_char, u32) -> bool>,
    text_to_value:
        Option<unsafe extern "C" fn(*const RawPlugin, u32, *const c_char, *mut f64) -> bool>,
    flush: Option<unsafe extern "C" fn(*const RawPlugin, *const InputEvents, *const OutputEvents)>,
}

#[repr(C)]
struct IStream {
    ctx: *mut c_void,
    read: Option<unsafe extern "C" fn(*const IStream, *mut c_void, u64) -> i64>,
}

#[repr(C)]
struct OStream {
    ctx: *mut c_void,
    write: Option<unsafe extern "C" fn(*const OStream, *const c_void, u64) -> i64>,
}

#[repr(C)]
struct PluginState {
    save: Option<unsafe extern "C" fn(*const RawPlugin, *const OStream) -> bool>,
    load: Option<unsafe extern "C" fn(*const RawPlugin, *const IStream) -> bool>,
}

#[repr(C)]
struct PluginLatency {
    get: Option<unsafe extern "C" fn(*const RawPlugin) -> u32>,
}

#[repr(C)]
#[derive(Clone, Copy)]
union WindowHandle {
    x11: c_ulong,
    win32: *mut c_void,
}

#[repr(C)]
struct Window {
    api: *const c_char,
    handle: WindowHandle,
}

#[repr(C)]
struct PluginGui {
    is_api_supported: Option<unsafe extern "C" fn(*const RawPlugin, *const c_char, bool) -> bool>,
    get_preferred_api:
        Option<unsafe extern "C" fn(*const RawPlugin, *mut *const c_char, *mut bool) -> bool>,
    create: Option<unsafe extern "C" fn(*const RawPlugin, *const c_char, bool) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const RawPlugin)>,
    set_scale: Option<unsafe extern "C" fn(*const RawPlugin, f64) -> bool>,
    get_size: Option<unsafe extern "C" fn(*const RawPlugin, *mut u32, *mut u32) -> bool>,
    can_resize: Option<unsafe extern "C" fn(*const RawPlugin) -> bool>,
    get_resize_hints: Option<unsafe extern "C" fn(*const RawPlugin, *mut c_void) -> bool>,
    adjust_size: Option<unsafe extern "C" fn(*const RawPlugin, *mut u32, *mut u32) -> bool>,
    set_size: Option<unsafe extern "C" fn(*const RawPlugin, u32, u32) -> bool>,
    set_parent: Option<unsafe extern "C" fn(*const RawPlugin, *const Window) -> bool>,
    set_transient: Option<unsafe extern "C" fn(*const RawPlugin, *const Window) -> bool>,
    suggest_title: Option<unsafe extern "C" fn(*const RawPlugin, *const c_char)>,
    show: Option<unsafe extern "C" fn(*const RawPlugin) -> bool>,
    hide: Option<unsafe extern "C" fn(*const RawPlugin) -> bool>,
}

#[repr(C)]
struct PluginTimerSupport {
    on_timer: Option<unsafe extern "C" fn(*const RawPlugin, u32)>,
}

#[repr(C)]
struct PluginPosixFdSupport {
    on_fd: Option<unsafe extern "C" fn(*const RawPlugin, c_int, u32)>,
}

#[repr(C)]
struct HostGui {
    resize_hints_changed: Option<unsafe extern "C" fn(*const RawHost)>,
    request_resize: Option<unsafe extern "C" fn(*const RawHost, u32, u32) -> bool>,
    request_show: Option<unsafe extern "C" fn(*const RawHost) -> bool>,
    request_hide: Option<unsafe extern "C" fn(*const RawHost) -> bool>,
    closed: Option<unsafe extern "C" fn(*const RawHost, bool)>,
}

#[repr(C)]
struct HostParams {
    rescan: Option<unsafe extern "C" fn(*const RawHost, u32)>,
    clear: Option<unsafe extern "C" fn(*const RawHost, u32, u32)>,
    request_flush: Option<unsafe extern "C" fn(*const RawHost)>,
}

#[repr(C)]
struct HostLatency {
    changed: Option<unsafe extern "C" fn(*const RawHost)>,
}

#[repr(C)]
struct HostLog {
    log: Option<unsafe extern "C" fn(*const RawHost, i32, *const c_char)>,
}

#[repr(C)]
struct HostThreadCheck {
    is_main_thread: Option<unsafe extern "C" fn(*const RawHost) -> bool>,
    is_audio_thread: Option<unsafe extern "C" fn(*const RawHost) -> bool>,
}

#[repr(C)]
struct HostTimerSupport {
    register_timer: Option<unsafe extern "C" fn(*const RawHost, u32, *mut u32) -> bool>,
    unregister_timer: Option<unsafe extern "C" fn(*const RawHost, u32) -> bool>,
}

#[repr(C)]
struct HostPosixFdSupport {
    register_fd: Option<unsafe extern "C" fn(*const RawHost, c_int, u32) -> bool>,
    modify_fd: Option<unsafe extern "C" fn(*const RawHost, c_int, u32) -> bool>,
    unregister_fd: Option<unsafe extern "C" fn(*const RawHost, c_int) -> bool>,
}

#[repr(C)]
struct HostState {
    mark_dirty: Option<unsafe extern "C" fn(*const RawHost)>,
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

fn bytes_to_string(b: &[u8]) -> String {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// A per-thread address: identifies the audio thread without allocating.
fn thread_token() -> usize {
    thread_local! {
        static TOKEN: u8 = const { 0 };
    }
    TOKEN.with(|t| t as *const u8 as usize)
}

/// Splits `file.clap#plugin.id` at the last `#`, but only when the part before
/// it is a `.clap` file. Anything else is a plain path.
pub fn split_path(path: &Path) -> (PathBuf, Option<String>) {
    #[cfg(unix)]
    {
        let bytes = path.as_os_str().as_bytes();
        if let Some(i) = bytes.iter().rposition(|&c| c == b'#') {
            let file = Path::new(std::ffi::OsStr::from_bytes(&bytes[..i]));
            let id = &bytes[i + 1..];
            let is_clap = file
                .extension()
                .is_some_and(|e| e.as_bytes().eq_ignore_ascii_case(b"clap"));
            if is_clap && !id.is_empty() {
                return (
                    file.to_path_buf(),
                    Some(String::from_utf8_lossy(id).into_owned()),
                );
            }
        }
        (path.to_path_buf(), None)
    }
    #[cfg(not(unix))]
    {
        let s = path.to_string_lossy();
        if let Some((head, id)) = s.rsplit_once('#') {
            let file = Path::new(head);
            let is_clap = file
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("clap"));
            if is_clap && !id.is_empty() {
                return (file.to_path_buf(), Some(id.to_string()));
            }
        }
        (path.to_path_buf(), None)
    }
}

/// `<file>#<plugin_id>`: the path form of one plugin inside a multi-plugin bundle.
pub fn join_path(file: &Path, plugin_id: &str) -> PathBuf {
    if plugin_id.is_empty() {
        return file.to_path_buf();
    }
    #[cfg(unix)]
    {
        let mut bytes = file.as_os_str().as_bytes().to_vec();
        bytes.push(b'#');
        bytes.extend_from_slice(plugin_id.as_bytes());
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        let mut os = file.as_os_str().to_os_string();
        os.push("#");
        os.push(plugin_id);
        PathBuf::from(os)
    }
}

// ---------------------------------------------------------------------------
// Descriptors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct ClapDesc {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub instrument: bool,
}

/// Instrument or not, and the category label shown to the user.
fn classify(features: &[String]) -> (bool, String) {
    let has = |f: &str| features.iter().any(|x| x == f);
    let instrument = [
        "instrument",
        "synthesizer",
        "sampler",
        "drum",
        "drum-machine",
    ]
    .iter()
    .any(|f| has(f));
    if instrument {
        return (true, "Instrument".into());
    }
    if has("audio-effect") {
        return (false, "Fx".into());
    }
    let category = match features.first() {
        None => "Fx".into(),
        Some(first) => {
            let words = first.replace('-', " ");
            let mut chars = words.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => "Fx".into(),
            }
        }
    };
    (false, category)
}

/// The descriptor and its raw feature tags (lower case).
fn read_desc(d: &Descriptor) -> (ClapDesc, Vec<String>) {
    let mut features = Vec::new();
    if !d.features.is_null() {
        let mut p = d.features;
        unsafe {
            while !(*p).is_null() {
                features.push(cstr(*p).to_ascii_lowercase());
                p = p.add(1);
            }
        }
    }
    let (instrument, category) = classify(&features);
    let id = cstr(d.id);
    let mut name = cstr(d.name);
    if name.is_empty() {
        name = id.clone();
    }
    (
        ClapDesc {
            id,
            name,
            vendor: cstr(d.vendor),
            category,
            instrument,
        },
        features,
    )
}

// ---------------------------------------------------------------------------
// Library and entry bookkeeping
// ---------------------------------------------------------------------------

fn open_library(file: &Path) -> Result<(libloading::Library, *const Entry), String> {
    unsafe {
        let library = libloading::Library::new(file).map_err(|e| e.to_string())?;
        let sym: libloading::Symbol<'_, *const Entry> = library
            .get(b"clap_entry\0")
            .map_err(|_| "clap_entry missing: not a CLAP plugin".to_string())?;
        // `libloading::Symbol<T>` reinterprets the raw symbol address as `T`, so
        // `*sym` is the exported `clap_entry` struct's address on both Unix and Windows.
        let entry: *const Entry = *sym;
        if entry.is_null() {
            return Err("clap_entry missing: not a CLAP plugin".into());
        }
        Ok((library, entry))
    }
}

/// How many instances (and listings) hold each entry, so `init` runs once on
/// the first and `deinit` once on the last, even when the same `.clap` is
/// loaded several times (the OS loader hands back the same module).
fn registry() -> &'static Mutex<HashMap<usize, usize>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

fn path_to_c_utf8(file: &Path) -> Result<CString, String> {
    #[cfg(unix)]
    {
        CString::new(file.as_os_str().as_bytes()).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        CString::new(file.to_string_lossy().as_bytes()).map_err(|e| e.to_string())
    }
}

fn acquire(entry: *const Entry, file: &Path) -> Result<(), String> {
    let cpath = path_to_c_utf8(file)?;
    let mut reg = lock(registry());
    let key = entry as usize;
    let count = reg.get(&key).copied().unwrap_or(0);
    if count == 0 {
        unsafe {
            if (*entry).clap_version.major < 1 {
                return Err("CLAP version too old".into());
            }
            let init = (*entry).init.ok_or("clap_entry.init missing")?;
            if !init(cpath.as_ptr()) {
                return Err("clap_entry.init failed".into());
            }
        }
    }
    reg.insert(key, count + 1);
    Ok(())
}

fn release(entry: *const Entry) {
    let mut reg = lock(registry());
    let key = entry as usize;
    match reg.get(&key).copied() {
        Some(1) => {
            reg.remove(&key);
            unsafe {
                if let Some(deinit) = (*entry).deinit {
                    deinit();
                }
            }
        }
        Some(n) => {
            reg.insert(key, n - 1);
        }
        None => {}
    }
}

fn factory_of(entry: *const Entry) -> Result<*const Factory, String> {
    unsafe {
        let get = (*entry)
            .get_factory
            .ok_or("clap_entry.get_factory missing")?;
        let factory = get(c"clap.plugin-factory".as_ptr()) as *const Factory;
        if factory.is_null() {
            return Err("no clap.plugin-factory".into());
        }
        Ok(factory)
    }
}

fn read_descs(factory: *const Factory) -> Vec<(ClapDesc, Vec<String>)> {
    let mut out = Vec::new();
    unsafe {
        let (Some(count), Some(get)) = (
            (*factory).get_plugin_count,
            (*factory).get_plugin_descriptor,
        ) else {
            return out;
        };
        for i in 0..count(factory) {
            let d = get(factory, i);
            if !d.is_null() {
                out.push(read_desc(&*d));
            }
        }
    }
    out
}

fn enumerate(entry: *const Entry, file: &Path) -> Result<Vec<ClapDesc>, String> {
    acquire(entry, file)?;
    let result: Result<Vec<ClapDesc>, String> =
        factory_of(entry).map(|f| read_descs(f).into_iter().map(|d| d.0).collect());
    release(entry);
    result
}

/// Every plugin in a `.clap` file.
pub fn list_plugins(file: &Path) -> Result<Vec<ClapDesc>, String> {
    let (_library, entry) = open_library(file)?;
    enumerate(entry, file)
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

struct TimerEntry {
    id: u32,
    period: Duration,
    next: Instant,
}

#[derive(Clone, Copy)]
struct FdEntry {
    fd: c_int,
    flags: u32,
}

/// What the plugin asks of the host. Callbacks may arrive from any thread.
struct HostContext {
    host: RawHost,
    label: String,
    callback: AtomicBool,
    restart: AtomicBool,
    params_rescan: AtomicBool,
    params_flush: AtomicBool,
    latency_changed: AtomicBool,
    /// `(width << 32) | height` of the last `request_resize`, 0 if none.
    resize: AtomicU64,
    /// `thread_token()` of the thread inside `process`, 0 outside it.
    audio_thread: AtomicUsize,
    next_id: AtomicU32,
    timers: Mutex<Vec<TimerEntry>>,
    fds: Mutex<Vec<FdEntry>>,
}

impl HostContext {
    fn new(label: &str) -> Box<Self> {
        let mut ctx = Box::new(Self {
            host: RawHost {
                clap_version: HOST_VERSION,
                host_data: ptr::null_mut(),
                name: c"KeyframeXL".as_ptr(),
                vendor: c"KeyframeXL".as_ptr(),
                url: c"".as_ptr(),
                version: c"1.0".as_ptr(),
                get_extension: Some(host_get_extension),
                request_restart: Some(host_request_restart),
                request_process: Some(host_request_process),
                request_callback: Some(host_request_callback),
            },
            label: label.to_string(),
            callback: AtomicBool::new(false),
            restart: AtomicBool::new(false),
            params_rescan: AtomicBool::new(false),
            params_flush: AtomicBool::new(false),
            latency_changed: AtomicBool::new(false),
            resize: AtomicU64::new(0),
            audio_thread: AtomicUsize::new(0),
            next_id: AtomicU32::new(1),
            timers: Mutex::new(Vec::new()),
            fds: Mutex::new(Vec::new()),
        });
        ctx.host.host_data = (&*ctx as *const HostContext).cast_mut().cast();
        ctx
    }
}

fn ctx_of<'a>(host: *const RawHost) -> Option<&'a HostContext> {
    if host.is_null() {
        return None;
    }
    unsafe { ((*host).host_data as *const HostContext).as_ref() }
}

unsafe extern "C" fn host_request_restart(host: *const RawHost) {
    if let Some(c) = ctx_of(host) {
        c.restart.store(true, Ordering::Release);
    }
}
unsafe extern "C" fn host_request_process(_host: *const RawHost) {}
unsafe extern "C" fn host_request_callback(host: *const RawHost) {
    if let Some(c) = ctx_of(host) {
        c.callback.store(true, Ordering::Release);
    }
}

static HOST_GUI: HostGui = HostGui {
    resize_hints_changed: Some(gui_resize_hints_changed),
    request_resize: Some(gui_request_resize),
    request_show: Some(gui_request_show),
    request_hide: Some(gui_request_hide),
    closed: Some(gui_closed),
};
static HOST_PARAMS: HostParams = HostParams {
    rescan: Some(params_rescan),
    clear: Some(params_clear),
    request_flush: Some(params_request_flush),
};
static HOST_LATENCY: HostLatency = HostLatency {
    changed: Some(latency_changed),
};
static HOST_LOG: HostLog = HostLog { log: Some(log_cb) };
static HOST_THREAD_CHECK: HostThreadCheck = HostThreadCheck {
    is_main_thread: Some(thread_is_main),
    is_audio_thread: Some(thread_is_audio),
};
static HOST_TIMER: HostTimerSupport = HostTimerSupport {
    register_timer: Some(timer_register),
    unregister_timer: Some(timer_unregister),
};
static HOST_FD: HostPosixFdSupport = HostPosixFdSupport {
    register_fd: Some(fd_register),
    modify_fd: Some(fd_modify),
    unregister_fd: Some(fd_unregister),
};
static HOST_STATE: HostState = HostState {
    mark_dirty: Some(state_mark_dirty),
};

unsafe extern "C" fn host_get_extension(_host: *const RawHost, id: *const c_char) -> *const c_void {
    if id.is_null() {
        return ptr::null();
    }
    match unsafe { CStr::from_ptr(id) }.to_bytes() {
        b"clap.gui" => (&HOST_GUI as *const HostGui).cast(),
        b"clap.params" => (&HOST_PARAMS as *const HostParams).cast(),
        b"clap.latency" => (&HOST_LATENCY as *const HostLatency).cast(),
        b"clap.log" => (&HOST_LOG as *const HostLog).cast(),
        b"clap.thread-check" => (&HOST_THREAD_CHECK as *const HostThreadCheck).cast(),
        b"clap.timer-support" => (&HOST_TIMER as *const HostTimerSupport).cast(),
        #[cfg(unix)]
        b"clap.posix-fd-support" => (&HOST_FD as *const HostPosixFdSupport).cast(),
        b"clap.state" => (&HOST_STATE as *const HostState).cast(),
        _ => ptr::null(),
    }
}

unsafe extern "C" fn gui_resize_hints_changed(_host: *const RawHost) {}
unsafe extern "C" fn gui_request_resize(host: *const RawHost, w: u32, h: u32) -> bool {
    match ctx_of(host) {
        Some(c) if w > 0 && h > 0 => {
            c.resize
                .store(((w as u64) << 32) | h as u64, Ordering::Release);
            true
        }
        _ => false,
    }
}
unsafe extern "C" fn gui_request_show(_host: *const RawHost) -> bool {
    false
}
unsafe extern "C" fn gui_request_hide(_host: *const RawHost) -> bool {
    false
}
unsafe extern "C" fn gui_closed(_host: *const RawHost, _was_destroyed: bool) {}

unsafe extern "C" fn params_rescan(host: *const RawHost, _flags: u32) {
    if let Some(c) = ctx_of(host) {
        c.params_rescan.store(true, Ordering::Release);
    }
}
unsafe extern "C" fn params_clear(_host: *const RawHost, _id: u32, _flags: u32) {}
unsafe extern "C" fn params_request_flush(host: *const RawHost) {
    if let Some(c) = ctx_of(host) {
        c.params_flush.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn latency_changed(host: *const RawHost) {
    if let Some(c) = ctx_of(host) {
        c.latency_changed.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn log_cb(host: *const RawHost, severity: i32, msg: *const c_char) {
    if severity >= LOG_WARNING {
        let label = ctx_of(host).map_or("", |c| c.label.as_str());
        eprintln!("[clap {label}] {}", cstr(msg));
    }
}

unsafe extern "C" fn thread_is_audio(host: *const RawHost) -> bool {
    ctx_of(host).is_some_and(|c| c.audio_thread.load(Ordering::Acquire) == thread_token())
}
unsafe extern "C" fn thread_is_main(host: *const RawHost) -> bool {
    !unsafe { thread_is_audio(host) }
}

unsafe extern "C" fn timer_register(host: *const RawHost, period_ms: u32, id: *mut u32) -> bool {
    let Some(c) = ctx_of(host) else { return false };
    if id.is_null() {
        return false;
    }
    let new_id = c.next_id.fetch_add(1, Ordering::Relaxed);
    let period = Duration::from_millis(period_ms.max(MIN_TIMER_MS) as u64);
    lock(&c.timers).push(TimerEntry {
        id: new_id,
        period,
        next: Instant::now() + period,
    });
    unsafe { *id = new_id };
    true
}
unsafe extern "C" fn timer_unregister(host: *const RawHost, id: u32) -> bool {
    let Some(c) = ctx_of(host) else { return false };
    let mut timers = lock(&c.timers);
    let before = timers.len();
    timers.retain(|t| t.id != id);
    timers.len() != before
}

unsafe extern "C" fn fd_register(host: *const RawHost, fd: c_int, flags: u32) -> bool {
    let Some(c) = ctx_of(host) else { return false };
    if fd < 0 {
        return false;
    }
    let mut fds = lock(&c.fds);
    match fds.iter_mut().find(|e| e.fd == fd) {
        Some(e) => e.flags = flags,
        None => fds.push(FdEntry { fd, flags }),
    }
    true
}
unsafe extern "C" fn fd_modify(host: *const RawHost, fd: c_int, flags: u32) -> bool {
    let Some(c) = ctx_of(host) else { return false };
    match lock(&c.fds).iter_mut().find(|e| e.fd == fd) {
        Some(e) => {
            e.flags = flags;
            true
        }
        None => false,
    }
}
unsafe extern "C" fn fd_unregister(host: *const RawHost, fd: c_int) -> bool {
    let Some(c) = ctx_of(host) else { return false };
    let mut fds = lock(&c.fds);
    let before = fds.len();
    fds.retain(|e| e.fd != fd);
    fds.len() != before
}

unsafe extern "C" fn state_mark_dirty(_host: *const RawHost) {}

// ---------------------------------------------------------------------------
// Event lists
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
union EventData {
    header: EventHeader,
    note: NoteEvent,
    midi: MidiEvent,
    param: ParamValueEvent,
}

/// One queued event and its arrival order (the tie-break for equal times).
#[derive(Clone, Copy)]
struct Slot {
    seq: u32,
    ev: EventData,
}

fn header(size: usize, time: u32, type_: u16) -> EventHeader {
    EventHeader {
        size: size as u32,
        time,
        space_id: 0,
        type_,
        flags: 0,
    }
}

fn note_event(time: u32, type_: u16, channel: i16, key: i16, velocity: f64) -> EventData {
    EventData {
        note: NoteEvent {
            header: header(size_of::<NoteEvent>(), time, type_),
            note_id: -1,
            port_index: 0,
            channel,
            key,
            velocity,
        },
    }
}

fn midi_event(time: u32, data: [u8; 3]) -> EventData {
    EventData {
        midi: MidiEvent {
            header: header(size_of::<MidiEvent>(), time, EVENT_MIDI),
            port_index: 0,
            data,
        },
    }
}

fn param_event(time: u32, id: u32, cookie: usize, value: f64) -> EventData {
    EventData {
        param: ParamValueEvent {
            header: header(size_of::<ParamValueEvent>(), time, EVENT_PARAM_VALUE),
            param_id: id,
            cookie: cookie as *mut c_void,
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value,
        },
    }
}

struct InCtx {
    events: *const Slot,
    len: u32,
}

unsafe extern "C" fn in_size(list: *const InputEvents) -> u32 {
    unsafe { (*((*list).ctx as *const InCtx)).len }
}
unsafe extern "C" fn in_get(list: *const InputEvents, index: u32) -> *const EventHeader {
    unsafe {
        let c = &*((*list).ctx as *const InCtx);
        if index >= c.len {
            return ptr::null();
        }
        // The header is the first field of every event type in the union.
        ptr::addr_of!((*c.events.add(index as usize)).ev).cast()
    }
}

struct OutCtx {
    params: *mut Vec<(u32, f64)>,
}

/// Collects the plugin's own parameter changes (its GUI, its automation).
/// Never grows the vector: it was sized up front.
unsafe extern "C" fn out_try_push(list: *const OutputEvents, event: *const EventHeader) -> bool {
    unsafe {
        if event.is_null() {
            return false;
        }
        let h = &*event;
        if h.space_id == 0
            && h.type_ == EVENT_PARAM_VALUE
            && h.size as usize >= size_of::<ParamValueEvent>()
        {
            let e = &*(event as *const ParamValueEvent);
            let params = &mut *(*((*list).ctx as *const OutCtx)).params;
            if params.len() < params.capacity() {
                params.push((e.param_id, e.value));
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Parameters, buses
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ClapParamInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub automatable: bool,
    pub read_only: bool,
    pub stepped: bool,
    pub cookie: usize,
}

impl ClapParamInfo {
    /// Normalized 0..1 to the plugin's own range (whole numbers when stepped).
    pub fn to_plain(&self, norm: f64) -> f64 {
        let n = if norm.is_finite() {
            norm.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = self.min + n * (self.max - self.min);
        if self.stepped {
            v.round()
                .clamp(self.min.min(self.max), self.max.max(self.min))
        } else {
            v
        }
    }

    /// The plugin's own range to normalized 0..1.
    pub fn to_norm(&self, plain: f64) -> f64 {
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON || !plain.is_finite() {
            0.0
        } else {
            ((plain - self.min) / span).clamp(0.0, 1.0)
        }
    }
}

struct ParamSlot {
    info: ClapParamInfo,
    hidden: bool,
    /// The last value the host knows (plain units).
    value: f64,
}

/// Channels per audio bus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClapBusLayout {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

// ---------------------------------------------------------------------------
// State streams
// ---------------------------------------------------------------------------

unsafe extern "C" fn ostream_write(stream: *const OStream, buf: *const c_void, size: u64) -> i64 {
    unsafe {
        if buf.is_null() {
            return -1;
        }
        let out = &mut *((*stream).ctx as *mut Vec<u8>);
        out.extend_from_slice(std::slice::from_raw_parts(buf.cast::<u8>(), size as usize));
        size as i64
    }
}

struct ReadCursor {
    data: *const u8,
    len: usize,
    pos: usize,
}

unsafe extern "C" fn istream_read(stream: *const IStream, buf: *mut c_void, size: u64) -> i64 {
    unsafe {
        if buf.is_null() {
            return -1;
        }
        let c = &mut *((*stream).ctx as *mut ReadCursor);
        let n = (size as usize).min(c.len - c.pos);
        ptr::copy_nonoverlapping(c.data.add(c.pos), buf.cast::<u8>(), n);
        c.pos += n;
        n as i64
    }
}

// ---------------------------------------------------------------------------
// The plugin instance
// ---------------------------------------------------------------------------

pub struct ClapPlugin {
    /// `None` for an in-memory entry (tests).
    library: Option<libloading::Library>,
    entry: *const Entry,
    plugin: *const RawPlugin,
    /// Boxed: the plugin keeps the pointer to its `RawHost`.
    host: Box<HostContext>,
    desc: ClapDesc,
    features: Vec<String>,
    layout: ClapBusLayout,
    audio_ports: *const PluginAudioPorts,
    note_ports: *const PluginNotePorts,
    params_ext: *const PluginParams,
    state_ext: *const PluginState,
    gui_ext: *const PluginGui,
    latency_ext: *const PluginLatency,
    timer_ext: *const PluginTimerSupport,
    fd_ext: *const PluginPosixFdSupport,
    note_inputs: usize,
    /// The plugin takes (and prefers) MIDI events over CLAP note events.
    use_midi: bool,
    /// The plugin accepts MIDI events at all.
    midi_ok: bool,
    params: Vec<ParamSlot>,
    param_index: HashMap<u32, usize>,
    active: bool,
    processing: bool,
    editor: bool,
    sample_rate: f64,
    max_frames: usize,
    steady_time: i64,
    /// Events of the next block, sized at load.
    events: Vec<Slot>,
    out_params: Vec<(u32, f64)>,
    /// Channel pointers and bus descriptors: laid out at load, only the
    /// pointers are rewritten per block. `process` does not allocate.
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    in_bufs: Vec<AudioBuffer>,
    out_bufs: Vec<AudioBuffer>,
    /// Stand-ins for channels the caller did not provide.
    silence: Vec<f32>,
    scratch: Vec<f32>,
}

/// The plugin is driven from one thread at a time (behind a mutex).
unsafe impl Send for ClapPlugin {}

impl ClapPlugin {
    /// `Plain.clap` loads its first plugin, `Bundle.clap#plugin.id` that one.
    pub fn load(path: &Path) -> Result<Self, String> {
        let (file, id) = split_path(path);
        let (library, entry) = open_library(&file)?;
        Self::open(Some(library), entry, &file, id.as_deref())
    }

    /// A plugin from an in-memory entry (no shared object), for unit tests.
    #[cfg(test)]
    pub(crate) fn from_entry(
        entry: *const Entry,
        path: &Path,
        id: Option<&str>,
    ) -> Result<Self, String> {
        Self::open(None, entry, path, id)
    }

    fn open(
        library: Option<libloading::Library>,
        entry: *const Entry,
        file: &Path,
        id: Option<&str>,
    ) -> Result<Self, String> {
        acquire(entry, file)?;
        match Self::instantiate(library, entry, id) {
            Ok(plugin) => Ok(plugin),
            Err(e) => {
                release(entry);
                Err(e)
            }
        }
    }

    fn instantiate(
        library: Option<libloading::Library>,
        entry: *const Entry,
        id: Option<&str>,
    ) -> Result<Self, String> {
        let factory = factory_of(entry)?;
        let descs = read_descs(factory);
        let (desc, features) = match id {
            None => descs.into_iter().next(),
            Some(id) => descs.into_iter().find(|d| d.0.id == id),
        }
        .ok_or_else(|| match id {
            Some(id) => format!("plugin {id} not found"),
            None => "no plugins in file".to_string(),
        })?;
        let cid = CString::new(desc.id.as_str()).map_err(|e| e.to_string())?;
        let host = HostContext::new(&desc.id);
        let plugin = unsafe {
            let create = (*factory)
                .create_plugin
                .ok_or("factory.create_plugin missing")?;
            create(factory, &host.host, cid.as_ptr())
        };
        if plugin.is_null() {
            return Err("create_plugin failed".into());
        }
        unsafe {
            let ok = (*plugin).init.is_some_and(|init| init(plugin));
            if !ok {
                if let Some(destroy) = (*plugin).destroy {
                    destroy(plugin);
                }
                return Err("plugin.init failed".into());
            }
        }
        let ext = |id: &CStr| -> *const c_void {
            unsafe {
                match (*plugin).get_extension {
                    Some(f) => f(plugin, id.as_ptr()),
                    None => ptr::null(),
                }
            }
        };
        let mut this = Self {
            library,
            entry,
            plugin,
            host,
            desc,
            features,
            layout: ClapBusLayout::default(),
            audio_ports: ext(c"clap.audio-ports").cast(),
            note_ports: ext(c"clap.note-ports").cast(),
            params_ext: ext(c"clap.params").cast(),
            state_ext: ext(c"clap.state").cast(),
            gui_ext: ext(c"clap.gui").cast(),
            latency_ext: ext(c"clap.latency").cast(),
            timer_ext: ext(c"clap.timer-support").cast(),
            fd_ext: ext(c"clap.posix-fd-support").cast(),
            note_inputs: 0,
            use_midi: false,
            midi_ok: false,
            params: Vec::new(),
            param_index: HashMap::new(),
            active: false,
            processing: false,
            editor: false,
            sample_rate: 48000.0,
            max_frames: 0,
            steady_time: 0,
            events: Vec::with_capacity(MAX_EVENTS),
            out_params: Vec::with_capacity(MAX_OUT_PARAMS),
            in_ptrs: Vec::new(),
            out_ptrs: Vec::new(),
            in_bufs: Vec::new(),
            out_bufs: Vec::new(),
            silence: Vec::new(),
            scratch: Vec::new(),
        };
        this.read_ports();
        this.reload_params();
        this.build_buffers();
        Ok(this)
    }

    /// Audio buses and note dialect, as the plugin declares them.
    fn read_ports(&mut self) {
        let channels = |is_input: bool| -> Option<Vec<usize>> {
            let ext = unsafe { self.audio_ports.as_ref() }?;
            let (count, get) = (ext.count?, ext.get?);
            let n = unsafe { count(self.plugin, is_input) };
            Some(
                (0..n)
                    .map(|i| {
                        let mut info: AudioPortInfo = unsafe { std::mem::zeroed() };
                        let ok = unsafe { get(self.plugin, i, is_input, &mut info) };
                        if ok {
                            (info.channel_count as usize).clamp(1, 256)
                        } else {
                            2
                        }
                    })
                    .collect(),
            )
        };
        self.layout = match (channels(true), channels(false)) {
            (Some(inputs), Some(outputs)) => ClapBusLayout { inputs, outputs },
            _ => ClapBusLayout {
                inputs: vec![2],
                outputs: vec![2],
            },
        };
        if let Some(ext) = unsafe { self.note_ports.as_ref() }
            && let (Some(count), Some(get)) = (ext.count, ext.get)
        {
            self.note_inputs = unsafe { count(self.plugin, true) } as usize;
            if self.note_inputs > 0 {
                let mut info: NotePortInfo = unsafe { std::mem::zeroed() };
                if unsafe { get(self.plugin, 0, true, &mut info) } {
                    let supported = info.supported_dialects;
                    self.midi_ok = supported & DIALECT_MIDI != 0;
                    self.use_midi = self.midi_ok
                        && (info.preferred_dialect == DIALECT_MIDI
                            || supported & DIALECT_CLAP == 0);
                }
            }
        }
    }

    /// Lay out the bus descriptors and channel pointer lists once.
    fn build_buffers(&mut self) {
        fn build(buses: &[usize], ptrs: &mut Vec<*mut f32>, bufs: &mut Vec<AudioBuffer>) {
            ptrs.clear();
            ptrs.resize(buses.iter().sum(), ptr::null_mut());
            bufs.clear();
            let mut offset = 0;
            for &chans in buses {
                bufs.push(AudioBuffer {
                    data32: unsafe { ptrs.as_mut_ptr().add(offset) },
                    data64: ptr::null_mut(),
                    channel_count: chans as u32,
                    latency: 0,
                    constant_mask: 0,
                });
                offset += chans;
            }
        }
        build(&self.layout.inputs, &mut self.in_ptrs, &mut self.in_bufs);
        build(&self.layout.outputs, &mut self.out_ptrs, &mut self.out_bufs);
    }

    /// (Re)read every parameter and its current value.
    fn reload_params(&mut self) {
        self.params.clear();
        self.param_index.clear();
        let Some(ext) = (unsafe { self.params_ext.as_ref() }) else {
            return;
        };
        let (Some(count), Some(get_info)) = (ext.count, ext.get_info) else {
            return;
        };
        for i in 0..unsafe { count(self.plugin) } {
            let mut raw: RawParamInfo = unsafe { std::mem::zeroed() };
            if !unsafe { get_info(self.plugin, i, &mut raw) } {
                continue;
            }
            let info = ClapParamInfo {
                id: raw.id,
                name: bytes_to_string(&raw.name),
                module: bytes_to_string(&raw.module),
                min: raw.min_value,
                max: raw.max_value,
                default: raw.default_value,
                automatable: raw.flags & PARAM_IS_AUTOMATABLE != 0,
                read_only: raw.flags & PARAM_IS_READONLY != 0,
                stepped: raw.flags & PARAM_IS_STEPPED != 0,
                cookie: raw.cookie as usize,
            };
            let mut value = raw.default_value;
            if let Some(get_value) = ext.get_value {
                let mut v = 0.0;
                if unsafe { get_value(self.plugin, raw.id, &mut v) } && v.is_finite() {
                    value = v;
                }
            }
            self.param_index.insert(raw.id, self.params.len());
            self.params.push(ParamSlot {
                info,
                hidden: raw.flags & PARAM_IS_HIDDEN != 0,
                value,
            });
        }
    }

    fn refresh_param_values(&mut self) {
        let Some(get_value) = (unsafe { self.params_ext.as_ref() }).and_then(|e| e.get_value)
        else {
            return;
        };
        for slot in &mut self.params {
            let mut v = 0.0;
            if unsafe { get_value(self.plugin, slot.info.id, &mut v) } && v.is_finite() {
                slot.value = v;
            }
        }
    }

    pub fn desc(&self) -> &ClapDesc {
        &self.desc
    }

    /// Tagged an instrument, or (with no tag saying otherwise) it takes notes
    /// and makes audio.
    pub fn is_instrument(&self) -> bool {
        if self.desc.instrument {
            return true;
        }
        let tagged_fx = self.features.iter().any(|f| {
            matches!(
                f.as_str(),
                "audio-effect" | "note-effect" | "note-detector" | "analyzer"
            )
        });
        !tagged_fx
            && self.note_inputs > 0
            && !self.audio_ports.is_null()
            && !self.layout.outputs.is_empty()
    }

    pub fn bus_layout(&self) -> &ClapBusLayout {
        &self.layout
    }

    /// Every parameter the plugin declares (hidden and read-only ones too).
    pub fn param_infos(&self) -> Vec<ClapParamInfo> {
        self.params.iter().map(|p| p.info.clone()).collect()
    }

    /// The host's last known value of a parameter, in plugin units.
    pub fn param_value(&self, id: u32) -> Option<f64> {
        self.param_index.get(&id).map(|&i| self.params[i].value)
    }

    fn value_text(&self, id: u32, value: f64) -> String {
        if let Some(ext) = unsafe { self.params_ext.as_ref() }
            && let Some(f) = ext.value_to_text
        {
            let mut buf = [0u8; NAME_SIZE];
            if unsafe {
                f(
                    self.plugin,
                    id,
                    value,
                    buf.as_mut_ptr().cast(),
                    NAME_SIZE as u32,
                )
            } {
                return bytes_to_string(&buf);
            }
        }
        format!("{value:.3}")
    }

    /// `(id, name, normalized 0..1, value text, automatable)` for the
    /// parameters a user can touch (no hidden or read-only ones).
    pub fn params(&self) -> Vec<(u32, String, f64, String, bool)> {
        self.params
            .iter()
            .filter(|p| !p.hidden && !p.info.read_only)
            .map(|p| {
                (
                    p.info.id,
                    p.info.name.clone(),
                    p.info.to_norm(p.value),
                    self.value_text(p.info.id, p.value),
                    p.info.automatable,
                )
            })
            .collect()
    }

    pub fn latency_samples(&self) -> usize {
        unsafe { self.latency_ext.as_ref() }
            .and_then(|e| e.get)
            .map_or(0, |get| unsafe { get(self.plugin) } as usize)
    }

    // ----- lifecycle -----

    /// Runs `f` as the audio thread (what `clap.thread-check` reports).
    fn on_audio_thread<R>(&self, f: impl FnOnce() -> R) -> R {
        self.host
            .audio_thread
            .store(thread_token(), Ordering::Release);
        let r = f();
        self.host.audio_thread.store(0, Ordering::Release);
        r
    }

    pub fn activate(&mut self, sample_rate: f64, max_frames: usize) -> Result<(), String> {
        if self.active {
            self.deactivate();
        }
        let max_frames = max_frames.clamp(1, u32::MAX as usize);
        let ok = unsafe {
            (*self.plugin)
                .activate
                .is_some_and(|f| f(self.plugin, sample_rate, 1, max_frames as u32))
        };
        if !ok {
            return Err("plugin.activate failed".into());
        }
        self.active = true;
        self.sample_rate = sample_rate;
        self.max_frames = max_frames;
        self.silence = vec![0.0; max_frames];
        self.scratch = vec![0.0; max_frames];
        Ok(())
    }

    pub fn start_processing(&mut self) -> Result<(), String> {
        if !self.active {
            return Err("plugin not active".into());
        }
        if self.processing {
            return Ok(());
        }
        let ok = self.on_audio_thread(|| unsafe {
            (*self.plugin)
                .start_processing
                .is_none_or(|f| f(self.plugin))
        });
        if !ok {
            return Err("plugin.start_processing failed".into());
        }
        self.processing = true;
        Ok(())
    }

    pub fn stop_processing(&mut self) {
        if self.processing {
            self.on_audio_thread(|| unsafe {
                if let Some(f) = (*self.plugin).stop_processing {
                    f(self.plugin);
                }
            });
            self.processing = false;
        }
        self.events.clear();
    }

    pub fn deactivate(&mut self) {
        self.stop_processing();
        if self.active {
            unsafe {
                if let Some(f) = (*self.plugin).deactivate {
                    f(self.plugin);
                }
            }
            self.active = false;
        }
    }

    // ----- events -----

    fn push_event(&mut self, ev: EventData) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(Slot {
                seq: self.events.len() as u32,
                ev,
            });
        }
    }

    fn push_note(&mut self, time: u32, channel: u8, pitch: u8, velocity: u8, on: bool) {
        let (channel, pitch) = (channel & 0x0f, pitch & 0x7f);
        let ev = if self.use_midi {
            let status = if on { 0x90 } else { 0x80 } | channel;
            midi_event(
                time,
                [status, pitch, if on { velocity.max(1) } else { velocity }],
            )
        } else {
            note_event(
                time,
                if on { EVENT_NOTE_ON } else { EVENT_NOTE_OFF },
                channel as i16,
                pitch as i16,
                (velocity as f64 / 127.0).clamp(0.0, 1.0),
            )
        };
        self.push_event(ev);
    }

    /// A note on, `sample_offset` frames into the next block.
    pub fn queue_note_on(&mut self, sample_offset: u32, pitch: u8, velocity: u8) {
        self.push_note(sample_offset, 0, pitch, velocity, true);
    }

    pub fn queue_note_off(&mut self, sample_offset: u32, pitch: u8) {
        self.push_note(sample_offset, 0, pitch, 0, false);
    }

    /// A short MIDI message. Plugins that only speak CLAP note events get
    /// note on/off translated and everything else dropped.
    pub fn queue_midi(&mut self, sample_offset: u32, bytes: [u8; 3]) {
        if self.midi_ok {
            self.push_event(midi_event(sample_offset, bytes));
            return;
        }
        let channel = bytes[0] & 0x0f;
        match bytes[0] & 0xf0 {
            0x90 if bytes[2] > 0 => {
                self.push_note(sample_offset, channel, bytes[1], bytes[2], true)
            }
            0x80 | 0x90 => self.push_note(sample_offset, channel, bytes[1], 0, false),
            _ => {}
        }
    }

    /// Set a parameter from its normalized 0..1 position. Playing: it rides
    /// along with the next block. Otherwise `params.flush` delivers it now.
    pub fn set_param_normalized(&mut self, id: u32, norm: f64) {
        let Some(&i) = self.param_index.get(&id) else {
            return;
        };
        let slot = &mut self.params[i];
        if slot.info.read_only {
            return;
        }
        let plain = slot.info.to_plain(norm);
        slot.value = plain;
        let ev = param_event(0, id, slot.info.cookie, plain);
        let flush = (unsafe { self.params_ext.as_ref() }).and_then(|e| e.flush);
        if !self.processing && flush.is_some() {
            self.flush_params(&[Slot { seq: 0, ev }]);
            return;
        }
        // A newer value of the same parameter replaces the queued one.
        for s in self.events.iter_mut() {
            // Only the header is initialised in every variant: look at it first.
            let queued_id = if unsafe { s.ev.header.type_ } == EVENT_PARAM_VALUE {
                Some(unsafe { s.ev.param.param_id })
            } else {
                None
            };
            if queued_id == Some(id) {
                s.ev = ev;
                return;
            }
        }
        self.push_event(ev);
    }

    /// `params.flush` with `events`, outside `process`.
    fn flush_params(&mut self, events: &[Slot]) {
        let Some(flush) = (unsafe { self.params_ext.as_ref() }).and_then(|e| e.flush) else {
            return;
        };
        let in_ctx = InCtx {
            events: events.as_ptr(),
            len: events.len() as u32,
        };
        let input = InputEvents {
            ctx: (&in_ctx as *const InCtx).cast_mut().cast(),
            size: Some(in_size),
            get: Some(in_get),
        };
        self.out_params.clear();
        let out_ctx = OutCtx {
            params: &mut self.out_params,
        };
        let output = OutputEvents {
            ctx: (&out_ctx as *const OutCtx).cast_mut().cast(),
            try_push: Some(out_try_push),
        };
        unsafe { flush(self.plugin, &input, &output) };
        self.apply_out_params();
    }

    fn apply_out_params(&mut self) {
        for &(id, value) in &self.out_params {
            if let Some(&i) = self.param_index.get(&id) {
                self.params[i].value = value;
            }
        }
    }

    // ----- audio -----

    /// One block. `inputs` / `outputs` are `[bus][channel][frame]`; buses or
    /// channels the caller has no buffer for read silence / write to a scratch
    /// buffer. Outputs are cleared first. `false` if the plugin is not active,
    /// `frames` exceeds what `activate` allowed, or the plugin reports an error.
    /// No allocation.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        inputs: &mut [Vec<Vec<f32>>],
        outputs: &mut [Vec<Vec<f32>>],
        frames: usize,
        sample_rate: u32,
        tempo: f64,
        playing: bool,
        frame_pos: u64,
    ) -> bool {
        if frames == 0 || frames > self.max_frames || !self.active {
            return false;
        }
        if !self.processing && self.start_processing().is_err() {
            return false;
        }
        let Some(process) = (unsafe { (*self.plugin).process }) else {
            return false;
        };

        let silence = self.silence.as_mut_ptr();
        let scratch = self.scratch.as_mut_ptr();
        let mut k = 0;
        for (b, &chans) in self.layout.inputs.iter().enumerate() {
            for c in 0..chans {
                self.in_ptrs[k] = inputs
                    .get_mut(b)
                    .and_then(|bus| bus.get_mut(c))
                    .filter(|ch| ch.len() >= frames)
                    .map_or(silence, |ch| ch.as_mut_ptr());
                k += 1;
            }
        }
        let mut k = 0;
        for (b, &chans) in self.layout.outputs.iter().enumerate() {
            for c in 0..chans {
                self.out_ptrs[k] = outputs
                    .get_mut(b)
                    .and_then(|bus| bus.get_mut(c))
                    .filter(|ch| ch.len() >= frames)
                    .map_or(scratch, |ch| {
                        ch[..frames].fill(0.0);
                        ch.as_mut_ptr()
                    });
                k += 1;
            }
        }

        let rate = if sample_rate > 0 {
            sample_rate as f64
        } else {
            self.sample_rate
        };
        let tempo = if tempo.is_finite() && tempo > 0.0 {
            tempo
        } else {
            120.0
        };
        let seconds = frame_pos as f64 / rate;
        let beats = seconds * tempo / 60.0;
        let bar = (beats / 4.0).floor();
        let transport = TransportEvent {
            header: header(size_of::<TransportEvent>(), 0, 9),
            flags: TRANSPORT_HAS_TEMPO
                | TRANSPORT_HAS_BEATS_TIMELINE
                | TRANSPORT_HAS_SECONDS_TIMELINE
                | TRANSPORT_HAS_TIME_SIGNATURE
                | if playing { TRANSPORT_IS_PLAYING } else { 0 },
            song_pos_beats: (beats * FIXED_ONE as f64) as i64,
            song_pos_seconds: (seconds * FIXED_ONE as f64) as i64,
            tempo,
            tempo_inc: 0.0,
            loop_start_beats: 0,
            loop_end_beats: 0,
            loop_start_seconds: 0,
            loop_end_seconds: 0,
            bar_start: (bar * 4.0 * FIXED_ONE as f64) as i64,
            bar_number: bar as i32,
            tsig_num: 4,
            tsig_denom: 4,
        };

        let last = (frames - 1) as u32;
        for s in self.events.iter_mut() {
            unsafe { s.ev.header.time = s.ev.header.time.min(last) };
        }
        self.events
            .sort_unstable_by_key(|s| (unsafe { s.ev.header.time }, s.seq));
        let in_ctx = InCtx {
            events: self.events.as_ptr(),
            len: self.events.len() as u32,
        };
        let input = InputEvents {
            ctx: (&in_ctx as *const InCtx).cast_mut().cast(),
            size: Some(in_size),
            get: Some(in_get),
        };
        self.out_params.clear();
        let out_ctx = OutCtx {
            params: &mut self.out_params,
        };
        let output = OutputEvents {
            ctx: (&out_ctx as *const OutCtx).cast_mut().cast(),
            try_push: Some(out_try_push),
        };

        let block = Process {
            steady_time: self.steady_time,
            frames_count: frames as u32,
            transport: &transport,
            audio_inputs: if self.in_bufs.is_empty() {
                ptr::null()
            } else {
                self.in_bufs.as_ptr()
            },
            audio_outputs: if self.out_bufs.is_empty() {
                ptr::null_mut()
            } else {
                self.out_bufs.as_mut_ptr()
            },
            audio_inputs_count: self.in_bufs.len() as u32,
            audio_outputs_count: self.out_bufs.len() as u32,
            in_events: &input,
            out_events: &output,
        };
        let plugin = self.plugin;
        let status = self.on_audio_thread(|| unsafe { process(plugin, &block) });

        self.apply_out_params();
        self.events.clear();
        self.steady_time += frames as i64;
        status != PROCESS_ERROR
    }

    // ----- state -----

    /// The plugin's state (`clap.state.save`).
    pub fn save_state(&self) -> Option<Vec<u8>> {
        let save = unsafe { self.state_ext.as_ref() }?.save?;
        let mut data: Vec<u8> = Vec::new();
        let stream = OStream {
            ctx: (&mut data as *mut Vec<u8>).cast(),
            write: Some(ostream_write),
        };
        unsafe { save(self.plugin, &stream) }.then_some(data)
    }

    /// Restore what `save_state` returned (`clap.state.load`).
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        let load = unsafe { self.state_ext.as_ref() }
            .and_then(|e| e.load)
            .ok_or("plugin has no state")?;
        let mut cursor = ReadCursor {
            data: bytes.as_ptr(),
            len: bytes.len(),
            pos: 0,
        };
        let stream = IStream {
            ctx: (&mut cursor as *mut ReadCursor).cast(),
            read: Some(istream_read),
        };
        if !unsafe { load(self.plugin, &stream) } {
            return Err("plugin.state.load failed".into());
        }
        self.refresh_param_values();
        Ok(())
    }

    // ----- editor -----

    /// Whether the plugin can embed its editor into the current platform's native window.
    pub fn has_editor(&self) -> bool {
        unsafe { self.gui_ext.as_ref() }
            .and_then(|g| g.is_api_supported)
            .is_some_and(|f| unsafe { f(self.plugin, GUI_API.as_ptr(), false) })
    }

    fn editor_size(&self) -> Option<(i32, i32)> {
        let get = unsafe { self.gui_ext.as_ref() }?.get_size?;
        let (mut w, mut h) = (0u32, 0u32);
        unsafe { get(self.plugin, &mut w, &mut h) }
            .then_some((w.min(i32::MAX as u32) as i32, h.min(i32::MAX as u32) as i32))
    }

    /// Embed the editor into the platform window (`X11` window ID on Linux, `HWND` on Windows).
    /// Returns its size in pixels.
    pub fn editor_open(&mut self, parent_window: u64) -> Result<(i32, i32), String> {
        self.editor_open_with_scale(parent_window, 1.0)
    }

    /// Embed the editor into `parent_window` with an explicit DPI scale factor.
    pub fn editor_open_with_scale(
        &mut self,
        parent_window: u64,
        scale: f64,
    ) -> Result<(i32, i32), String> {
        if self.editor {
            return self
                .editor_size()
                .ok_or_else(|| "editor has no size".into());
        }
        let gui = unsafe { self.gui_ext.as_ref() }.ok_or("plugin has no editor")?;
        let plugin = self.plugin;
        unsafe {
            let create = gui.create.ok_or("gui.create missing")?;
            if !gui
                .is_api_supported
                .is_some_and(|f| f(plugin, GUI_API.as_ptr(), false))
            {
                return Err(format!(
                    "editor does not support {}",
                    GUI_API.to_string_lossy()
                ));
            }
            if !create(plugin, GUI_API.as_ptr(), false) {
                return Err("gui.create failed".into());
            }
            self.editor = true;
            if let Some(set_scale) = gui.set_scale {
                // Plugins that scale themselves refuse: nothing to do.
                set_scale(plugin, scale.max(0.25));
            }
        }
        let attach = || -> Result<(i32, i32), String> {
            let size = self.editor_size().ok_or("gui.get_size failed")?;
            #[cfg(target_os = "windows")]
            let handle = WindowHandle {
                win32: parent_window as usize as *mut c_void,
            };
            #[cfg(not(target_os = "windows"))]
            let handle = WindowHandle {
                x11: parent_window as c_ulong,
            };
            let window = Window {
                api: GUI_API.as_ptr(),
                handle,
            };
            unsafe {
                let set_parent = gui.set_parent.ok_or("gui.set_parent missing")?;
                if !set_parent(plugin, &window) {
                    return Err("gui.set_parent failed".into());
                }
                if let Some(show) = gui.show
                    && !show(plugin)
                {
                    return Err("gui.show failed".into());
                }
            }
            Ok(size)
        };
        match attach() {
            Ok(size) => Ok(size),
            Err(e) => {
                self.editor_close();
                Err(e)
            }
        }
    }

    /// Inform an open editor that the monitor DPI scale changed.
    pub fn editor_set_scale(&mut self, scale: f64) {
        if !self.editor {
            return;
        }
        if let Some(set_scale) = unsafe { self.gui_ext.as_ref() }.and_then(|g| g.set_scale) {
            unsafe {
                set_scale(self.plugin, scale.max(0.25));
            }
        }
    }

    /// The window around the editor changed size.
    pub fn editor_set_size(&mut self, w: u32, h: u32) {
        if !self.editor {
            return;
        }
        let Some(gui) = (unsafe { self.gui_ext.as_ref() }) else {
            return;
        };
        unsafe {
            if !gui.can_resize.is_some_and(|f| f(self.plugin)) {
                return;
            }
            let (mut w, mut h) = (w, h);
            if let Some(adjust) = gui.adjust_size {
                adjust(self.plugin, &mut w, &mut h);
            }
            if let Some(set_size) = gui.set_size {
                set_size(self.plugin, w, h);
            }
        }
    }

    pub fn editor_close(&mut self) {
        if !self.editor {
            return;
        }
        self.editor = false;
        if let Some(gui) = unsafe { self.gui_ext.as_ref() } {
            unsafe {
                if let Some(hide) = gui.hide {
                    hide(self.plugin);
                }
                if let Some(destroy) = gui.destroy {
                    destroy(self.plugin);
                }
            }
        }
    }

    // ----- host upkeep -----

    /// Main-thread upkeep: call it regularly (a UI frame is enough). Runs what
    /// the plugin asked for (`request_callback`, timers, file descriptors) and
    /// returns the editor size the plugin requested, if it did.
    pub fn service(&mut self) -> Option<(i32, i32)> {
        let plugin = self.plugin;
        if self.host.params_rescan.swap(false, Ordering::AcqRel) {
            self.reload_params();
        }
        if self.host.params_flush.swap(false, Ordering::AcqRel) && !self.processing {
            self.flush_params(&[]);
        }
        if self.host.callback.swap(false, Ordering::AcqRel)
            && let Some(f) = unsafe { (*plugin).on_main_thread }
        {
            unsafe { f(plugin) };
        }

        // The plugin may (un)register timers from inside its callbacks: no
        // lock is held while calling out.
        if let Some(on_timer) = unsafe { self.timer_ext.as_ref() }.and_then(|e| e.on_timer) {
            let now = Instant::now();
            let due: Vec<u32> = lock(&self.host.timers)
                .iter_mut()
                .filter(|t| now >= t.next)
                .map(|t| {
                    t.next = now + t.period;
                    t.id
                })
                .collect();
            for id in due {
                if lock(&self.host.timers).iter().any(|t| t.id == id) {
                    unsafe { on_timer(plugin, id) };
                }
            }
        }

        #[cfg(unix)]
        if let Some(on_fd) = unsafe { self.fd_ext.as_ref() }.and_then(|e| e.on_fd) {
            let watched: Vec<FdEntry> = lock(&self.host.fds).clone();
            if !watched.is_empty() {
                let mut polls: Vec<libc::pollfd> = watched
                    .iter()
                    .map(|e| libc::pollfd {
                        fd: e.fd,
                        events: (if e.flags & FD_READ != 0 {
                            libc::POLLIN
                        } else {
                            0
                        }) | (if e.flags & FD_WRITE != 0 {
                            libc::POLLOUT
                        } else {
                            0
                        }),
                        revents: 0,
                    })
                    .collect();
                let n = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, 0) };
                if n > 0 {
                    for p in &polls {
                        let mut flags = 0;
                        if p.revents & libc::POLLIN != 0 {
                            flags |= FD_READ;
                        }
                        if p.revents & libc::POLLOUT != 0 {
                            flags |= FD_WRITE;
                        }
                        if p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                            flags |= FD_ERROR;
                        }
                        if flags != 0 && lock(&self.host.fds).iter().any(|e| e.fd == p.fd) {
                            unsafe { on_fd(plugin, p.fd, flags) };
                        }
                    }
                }
            }
        }

        let size = self.host.resize.swap(0, Ordering::AcqRel);
        (size != 0).then_some(((size >> 32) as i32, size as u32 as i32))
    }
}

impl Drop for ClapPlugin {
    fn drop(&mut self) {
        self.editor_close();
        self.deactivate();
        unsafe {
            if let Some(destroy) = (*self.plugin).destroy {
                destroy(self.plugin);
            }
        }
        release(self.entry);
        // Drop the dynamic library only after `destroy` and `entry.deinit`.
        drop(self.library.take());
    }
}

// ---------------------------------------------------------------------------
// Tests: an in-memory CLAP plugin behind the real C ABI
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // ----- mock plugin world -----

    const ID_GAIN: &str = "kfxl.test.gain";
    const ID_SYNTH: &str = "kfxl.test.synth";
    const ID_MIDI: &str = "kfxl.test.midisynth";
    const ID_UNTAGGED: &str = "kfxl.test.untagged";
    const ID_BARE: &str = "kfxl.test.bare";

    thread_local! {
        static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }
    fn log(s: &'static str) {
        LOG.with(|l| l.borrow_mut().push(s));
    }
    fn take_log() -> Vec<&'static str> {
        LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Gain,
        Synth,
        MidiSynth,
        Untagged,
        Bare,
    }

    struct Mock {
        kind: Kind,
        host: *const RawHost,
        gain_db: f64,
        steps: f64,
        phase: f64,
        freq: f64,
        amp: f64,
        key: i16,
        sample_rate: f64,
        // what the plugin saw
        tempo: f64,
        flags: u32,
        beats: i64,
        seconds: i64,
        tsig: (u16, u16),
        steady: Vec<i64>,
        frames: Vec<u32>,
        event_times: Vec<u32>,
        note_events: usize,
        midi: Vec<[u8; 3]>,
        param_events: usize,
        flushes: usize,
        cookie: usize,
        emit_param: Option<(u32, f64)>,
        thread_audio: Vec<bool>,
        // editor
        api: String,
        parent: u64,
        size: (u32, u32),
        visible: bool,
        created: bool,
        scale_calls: usize,
        timer: u32,
        timer_ticks: usize,
        main_thread_calls: usize,
        fd_events: Vec<(i32, u32)>,
        latency: u32,
        tail: Vec<u8>,
    }

    unsafe fn mock<'a>(plugin: *const RawPlugin) -> &'a mut Mock {
        unsafe { &mut *((*plugin).plugin_data as *mut Mock) }
    }

    unsafe fn host_ext<T>(host: *const RawHost, id: &CStr) -> &'static T {
        unsafe { &*(((*host).get_extension.unwrap())(host, id.as_ptr()) as *const T) }
    }

    struct World {
        _features: Vec<Vec<*const c_char>>,
        descs: Vec<Descriptor>,
    }
    unsafe impl Send for World {}
    unsafe impl Sync for World {}

    fn world() -> &'static World {
        static WORLD: OnceLock<World> = OnceLock::new();
        WORLD.get_or_init(|| {
            let defs: [(&CStr, &CStr, &[&CStr]); 5] = [
                (
                    c"kfxl.test.gain",
                    c"Test Gain",
                    &[c"audio-effect", c"utility"],
                ),
                (
                    c"kfxl.test.synth",
                    c"Test Synth",
                    &[c"instrument", c"synthesizer", c"stereo"],
                ),
                (c"kfxl.test.midisynth", c"Test MIDI Synth", &[c"Instrument"]),
                (c"kfxl.test.untagged", c"Test Untagged", &[]),
                (c"kfxl.test.bare", c"Test Bare", &[c"note-effect"]),
            ];
            let mut features = Vec::new();
            let mut descs = Vec::new();
            for (id, name, tags) in defs {
                let mut list: Vec<*const c_char> = tags.iter().map(|t| t.as_ptr()).collect();
                list.push(ptr::null());
                descs.push(Descriptor {
                    clap_version: HOST_VERSION,
                    id: id.as_ptr(),
                    name: name.as_ptr(),
                    vendor: c"KFXL".as_ptr(),
                    url: ptr::null(),
                    manual_url: ptr::null(),
                    support_url: ptr::null(),
                    version: c"1".as_ptr(),
                    description: ptr::null(),
                    features: list.as_ptr(),
                });
                features.push(list);
            }
            World {
                _features: features,
                descs,
            }
        })
    }

    unsafe extern "C" fn f_count(_f: *const Factory) -> u32 {
        world().descs.len() as u32
    }
    unsafe extern "C" fn f_desc(_f: *const Factory, i: u32) -> *const Descriptor {
        world()
            .descs
            .get(i as usize)
            .map_or(ptr::null(), |d| d as *const Descriptor)
    }
    unsafe extern "C" fn f_create(
        _f: *const Factory,
        host: *const RawHost,
        id: *const c_char,
    ) -> *const RawPlugin {
        let id = cstr(id);
        let kind = match id.as_str() {
            ID_GAIN => Kind::Gain,
            ID_SYNTH => Kind::Synth,
            ID_MIDI => Kind::MidiSynth,
            ID_UNTAGGED => Kind::Untagged,
            ID_BARE => Kind::Bare,
            _ => return ptr::null(),
        };
        let desc = world().descs.iter().find(|d| cstr(d.id) == id).unwrap();
        let state = Box::new(Mock {
            kind,
            host,
            gain_db: 0.0,
            steps: 2.0,
            phase: 0.0,
            freq: 440.0,
            amp: 0.0,
            key: -1,
            sample_rate: 48000.0,
            tempo: 0.0,
            flags: 0,
            beats: 0,
            seconds: 0,
            tsig: (0, 0),
            steady: Vec::new(),
            frames: Vec::new(),
            event_times: Vec::new(),
            note_events: 0,
            midi: Vec::new(),
            param_events: 0,
            flushes: 0,
            cookie: 0,
            emit_param: None,
            thread_audio: Vec::new(),
            api: String::new(),
            parent: 0,
            size: (640, 480),
            visible: false,
            created: false,
            scale_calls: 0,
            timer: 0,
            timer_ticks: 0,
            main_thread_calls: 0,
            fd_events: Vec::new(),
            latency: if kind == Kind::Gain { 128 } else { 0 },
            tail: Vec::new(),
        });
        Box::into_raw(Box::new(RawPlugin {
            desc,
            plugin_data: Box::into_raw(state).cast(),
            init: Some(m_init),
            destroy: Some(m_destroy),
            activate: Some(m_activate),
            deactivate: Some(m_deactivate),
            start_processing: Some(m_start),
            stop_processing: Some(m_stop),
            reset: Some(m_reset),
            process: Some(m_process),
            get_extension: Some(m_get_extension),
            on_main_thread: Some(m_on_main_thread),
        }))
    }

    static FACTORY: Factory = Factory {
        get_plugin_count: Some(f_count),
        get_plugin_descriptor: Some(f_desc),
        create_plugin: Some(f_create),
    };

    unsafe extern "C" fn e_init(path: *const c_char) -> bool {
        !path.is_null()
    }
    unsafe extern "C" fn e_deinit() {}
    unsafe extern "C" fn e_get_factory(id: *const c_char) -> *const c_void {
        if cstr(id) == "clap.plugin-factory" {
            (&FACTORY as *const Factory).cast()
        } else {
            ptr::null()
        }
    }
    unsafe extern "C" fn iso_init(path: *const c_char) -> bool {
        log("entry.init");
        !path.is_null()
    }
    unsafe extern "C" fn iso_deinit() {
        log("entry.deinit");
    }

    static MOCK_ENTRY: Entry = Entry {
        clap_version: HOST_VERSION,
        init: Some(e_init),
        deinit: Some(e_deinit),
        get_factory: Some(e_get_factory),
    };
    /// Only the lifecycle test touches this one: its init/deinit calls are logged.
    static ISO_ENTRY: Entry = Entry {
        clap_version: HOST_VERSION,
        init: Some(iso_init),
        deinit: Some(iso_deinit),
        get_factory: Some(e_get_factory),
    };

    unsafe extern "C" fn m_init(_p: *const RawPlugin) -> bool {
        log("plugin.init");
        true
    }
    unsafe extern "C" fn m_destroy(p: *const RawPlugin) {
        log("destroy");
        unsafe {
            drop(Box::from_raw((*p).plugin_data as *mut Mock));
            drop(Box::from_raw(p as *mut RawPlugin));
        }
    }
    unsafe extern "C" fn m_activate(p: *const RawPlugin, sr: f64, min: u32, max: u32) -> bool {
        log("activate");
        let m = unsafe { mock(p) };
        m.sample_rate = sr;
        min >= 1 && max >= min
    }
    unsafe extern "C" fn m_deactivate(_p: *const RawPlugin) {
        log("deactivate");
    }
    unsafe extern "C" fn m_start(_p: *const RawPlugin) -> bool {
        log("start");
        true
    }
    unsafe extern "C" fn m_stop(_p: *const RawPlugin) {
        log("stop");
    }
    unsafe extern "C" fn m_reset(_p: *const RawPlugin) {}
    unsafe extern "C" fn m_on_main_thread(p: *const RawPlugin) {
        unsafe { mock(p) }.main_thread_calls += 1;
    }

    fn apply_param(m: &mut Mock, e: &ParamValueEvent) {
        m.param_events += 1;
        m.cookie = e.cookie as usize;
        match e.param_id {
            7 => m.gain_db = e.value,
            8 => m.steps = e.value,
            _ => {}
        }
    }

    unsafe fn handle_event(m: &mut Mock, h: &EventHeader) {
        m.event_times.push(h.time);
        match (h.space_id, h.type_) {
            (0, EVENT_NOTE_ON) => {
                let n = unsafe { &*(h as *const EventHeader as *const NoteEvent) };
                m.note_events += 1;
                m.key = n.key;
                m.freq = 440.0 * 2f64.powf((n.key as f64 - 69.0) / 12.0);
                m.amp = n.velocity * 0.5;
            }
            (0, EVENT_NOTE_OFF) => {
                let n = unsafe { &*(h as *const EventHeader as *const NoteEvent) };
                m.note_events += 1;
                if n.key == m.key {
                    m.amp = 0.0;
                }
            }
            (0, EVENT_MIDI) => {
                let e = unsafe { &*(h as *const EventHeader as *const MidiEvent) };
                m.midi.push(e.data);
                match (e.data[0] & 0xf0, e.data[2]) {
                    (0x90, v) if v > 0 => {
                        m.key = e.data[1] as i16;
                        m.freq = 440.0 * 2f64.powf((e.data[1] as f64 - 69.0) / 12.0);
                        m.amp = v as f64 / 127.0 * 0.5;
                    }
                    (0x80, _) | (0x90, _) => m.amp = 0.0,
                    _ => {}
                }
            }
            (0, EVENT_PARAM_VALUE) => {
                let e = unsafe { &*(h as *const EventHeader as *const ParamValueEvent) };
                apply_param(m, e);
            }
            _ => {}
        }
    }

    unsafe extern "C" fn m_process(p: *const RawPlugin, block: *const Process) -> i32 {
        log("process");
        let m = unsafe { mock(p) };
        let block = unsafe { &*block };
        if let Some(t) = unsafe { block.transport.as_ref() } {
            m.tempo = t.tempo;
            m.flags = t.flags;
            m.beats = t.song_pos_beats;
            m.seconds = t.song_pos_seconds;
            m.tsig = (t.tsig_num, t.tsig_denom);
        }
        m.steady.push(block.steady_time);
        m.frames.push(block.frames_count);
        m.thread_audio
            .push(unsafe { thread_is_audio(m.host) } && !unsafe { thread_is_main(m.host) });
        let frames = block.frames_count as usize;
        let events = unsafe { &*block.in_events };
        let n = unsafe { (events.size.unwrap())(events) };
        let mut next = 0;
        let ins: &[AudioBuffer] = if block.audio_inputs_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(block.audio_inputs, block.audio_inputs_count as usize)
            }
        };
        let outs: &[AudioBuffer] = if block.audio_outputs_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(block.audio_outputs, block.audio_outputs_count as usize)
            }
        };
        for i in 0..frames {
            while next < n {
                let h = unsafe { &*(events.get.unwrap())(events, next) };
                if h.time as usize > i {
                    break;
                }
                unsafe { handle_event(m, h) };
                next += 1;
            }
            let gain = 10f64.powf(m.gain_db / 20.0);
            if m.kind == Kind::Gain {
                for c in 0..2 {
                    unsafe {
                        let x = *(*ins[0].data32.add(c)).add(i);
                        *(*outs[0].data32.add(c)).add(i) = x * gain as f32;
                    }
                }
            } else {
                let s = (m.phase.sin() * m.amp * gain) as f32;
                m.phase += std::f64::consts::TAU * m.freq / m.sample_rate;
                unsafe {
                    for c in 0..outs[0].channel_count as usize {
                        *(*outs[0].data32.add(c)).add(i) = s;
                    }
                    if outs.len() > 1 {
                        *(*outs[1].data32).add(i) = s * 0.5;
                    }
                }
            }
        }
        while next < n {
            let h = unsafe { &*(events.get.unwrap())(events, next) };
            unsafe { handle_event(m, h) };
            next += 1;
        }
        if let Some((id, value)) = m.emit_param.take() {
            let e = ParamValueEvent {
                header: header(size_of::<ParamValueEvent>(), 0, EVENT_PARAM_VALUE),
                param_id: id,
                cookie: ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value,
            };
            let out = unsafe { &*block.out_events };
            unsafe { (out.try_push.unwrap())(out, &e.header) };
        }
        1
    }

    fn fill(buf: &mut [u8], s: &str) {
        buf[..s.len()].copy_from_slice(s.as_bytes());
    }

    unsafe extern "C" fn ap_count(p: *const RawPlugin, is_input: bool) -> u32 {
        match (unsafe { mock(p) }.kind, is_input) {
            (Kind::Gain, _) => 1,
            (Kind::Synth, true) => 0,
            (Kind::Synth, false) => 2,
            (_, true) => 0,
            (_, false) => 1,
        }
    }
    unsafe extern "C" fn ap_get(
        p: *const RawPlugin,
        index: u32,
        is_input: bool,
        info: *mut AudioPortInfo,
    ) -> bool {
        let m = unsafe { mock(p) };
        let channels = if m.kind == Kind::Synth && !is_input && index == 1 {
            1
        } else {
            2
        };
        unsafe {
            (*info).id = index;
            fill(&mut (*info).name, "main");
            (*info).channel_count = channels;
        }
        true
    }
    static M_AUDIO_PORTS: PluginAudioPorts = PluginAudioPorts {
        count: Some(ap_count),
        get: Some(ap_get),
    };

    unsafe extern "C" fn np_count(_p: *const RawPlugin, is_input: bool) -> u32 {
        is_input as u32
    }
    unsafe extern "C" fn np_get(
        p: *const RawPlugin,
        _index: u32,
        _is_input: bool,
        info: *mut NotePortInfo,
    ) -> bool {
        let (supported, preferred) = match unsafe { mock(p) }.kind {
            Kind::MidiSynth => (DIALECT_CLAP | DIALECT_MIDI, DIALECT_MIDI),
            Kind::Untagged => (DIALECT_CLAP, DIALECT_CLAP),
            _ => (DIALECT_CLAP | DIALECT_MIDI, DIALECT_CLAP),
        };
        unsafe {
            (*info).supported_dialects = supported;
            (*info).preferred_dialect = preferred;
        }
        true
    }
    static M_NOTE_PORTS: PluginNotePorts = PluginNotePorts {
        count: Some(np_count),
        get: Some(np_get),
    };

    unsafe extern "C" fn pa_count(_p: *const RawPlugin) -> u32 {
        3
    }
    unsafe extern "C" fn pa_info(_p: *const RawPlugin, i: u32, info: *mut RawParamInfo) -> bool {
        let info = unsafe { &mut *info };
        match i {
            0 => {
                info.id = 7;
                info.flags = PARAM_IS_AUTOMATABLE;
                info.cookie = 0xC00C1E as *mut c_void;
                fill(&mut info.name, "Gain");
                fill(&mut info.module, "Mixer");
                (info.min_value, info.max_value, info.default_value) = (-24.0, 12.0, 0.0);
            }
            1 => {
                info.id = 8;
                info.flags = PARAM_IS_STEPPED | PARAM_IS_AUTOMATABLE;
                fill(&mut info.name, "Steps");
                (info.min_value, info.max_value, info.default_value) = (0.0, 4.0, 2.0);
            }
            2 => {
                info.id = 9;
                info.flags = PARAM_IS_READONLY | PARAM_IS_HIDDEN;
                fill(&mut info.name, "Meter");
                (info.min_value, info.max_value, info.default_value) = (0.0, 1.0, 0.0);
            }
            _ => return false,
        }
        true
    }
    unsafe extern "C" fn pa_value(p: *const RawPlugin, id: u32, out: *mut f64) -> bool {
        let m = unsafe { mock(p) };
        let v = match id {
            7 => m.gain_db,
            8 => m.steps,
            9 => 0.25,
            _ => return false,
        };
        unsafe { *out = v };
        true
    }
    unsafe extern "C" fn pa_text(
        _p: *const RawPlugin,
        id: u32,
        value: f64,
        out: *mut c_char,
        cap: u32,
    ) -> bool {
        let s = match id {
            7 => format!("{value:.1} dB"),
            8 => format!("step {}", value as i64),
            _ => return false,
        };
        let n = s.len().min(cap as usize - 1);
        unsafe {
            ptr::copy_nonoverlapping(s.as_ptr(), out.cast::<u8>(), n);
            *out.add(n) = 0;
        }
        true
    }
    unsafe extern "C" fn pa_flush(
        p: *const RawPlugin,
        input: *const InputEvents,
        _out: *const OutputEvents,
    ) {
        let m = unsafe { mock(p) };
        m.flushes += 1;
        let input = unsafe { &*input };
        for i in 0..unsafe { (input.size.unwrap())(input) } {
            let h = unsafe { &*(input.get.unwrap())(input, i) };
            if h.type_ == EVENT_PARAM_VALUE {
                apply_param(m, unsafe {
                    &*(h as *const EventHeader as *const ParamValueEvent)
                });
            }
        }
    }
    static M_PARAMS: PluginParams = PluginParams {
        count: Some(pa_count),
        get_info: Some(pa_info),
        get_value: Some(pa_value),
        value_to_text: Some(pa_text),
        text_to_value: None,
        flush: Some(pa_flush),
    };

    /// 16 bytes of parameters, then the free-form tail, in 7-byte writes.
    unsafe extern "C" fn st_save(p: *const RawPlugin, out: *const OStream) -> bool {
        let m = unsafe { mock(p) };
        let mut blob = Vec::new();
        blob.extend_from_slice(&m.gain_db.to_le_bytes());
        blob.extend_from_slice(&m.steps.to_le_bytes());
        blob.extend_from_slice(&m.tail);
        let out = unsafe { &*out };
        for chunk in blob.chunks(7) {
            let n = unsafe { (out.write.unwrap())(out, chunk.as_ptr().cast(), chunk.len() as u64) };
            if n != chunk.len() as i64 {
                return false;
            }
        }
        true
    }
    /// Reads in 5-byte pieces until the host reports the end.
    unsafe extern "C" fn st_load(p: *const RawPlugin, input: *const IStream) -> bool {
        let m = unsafe { mock(p) };
        let input = unsafe { &*input };
        let mut blob = Vec::new();
        loop {
            let mut piece = [0u8; 5];
            let n = unsafe { (input.read.unwrap())(input, piece.as_mut_ptr().cast(), 5) };
            if n < 0 {
                return false;
            }
            if n == 0 {
                break;
            }
            blob.extend_from_slice(&piece[..n as usize]);
        }
        if blob.len() < 16 {
            return false;
        }
        m.gain_db = f64::from_le_bytes(blob[..8].try_into().unwrap());
        m.steps = f64::from_le_bytes(blob[8..16].try_into().unwrap());
        m.tail = blob[16..].to_vec();
        true
    }
    static M_STATE: PluginState = PluginState {
        save: Some(st_save),
        load: Some(st_load),
    };

    unsafe extern "C" fn lat_get(p: *const RawPlugin) -> u32 {
        unsafe { mock(p) }.latency
    }
    static M_LATENCY: PluginLatency = PluginLatency { get: Some(lat_get) };

    unsafe extern "C" fn gui_supported(
        _p: *const RawPlugin,
        api: *const c_char,
        floating: bool,
    ) -> bool {
        cstr(api) == GUI_API.to_string_lossy() && !floating
    }
    unsafe extern "C" fn gui_create(
        p: *const RawPlugin,
        api: *const c_char,
        floating: bool,
    ) -> bool {
        let m = unsafe { mock(p) };
        m.api = cstr(api);
        m.created = !floating;
        m.created
    }
    unsafe extern "C" fn gui_destroy(p: *const RawPlugin) {
        log("gui.destroy");
        let m = unsafe { mock(p) };
        m.created = false;
        if m.timer != 0 {
            let t: &HostTimerSupport = unsafe { host_ext(m.host, c"clap.timer-support") };
            unsafe { (t.unregister_timer.unwrap())(m.host, m.timer) };
            m.timer = 0;
        }
    }
    unsafe extern "C" fn gui_set_scale(p: *const RawPlugin, _s: f64) -> bool {
        unsafe { mock(p) }.scale_calls += 1;
        false
    }
    unsafe extern "C" fn gui_get_size(p: *const RawPlugin, w: *mut u32, h: *mut u32) -> bool {
        let m = unsafe { mock(p) };
        unsafe {
            *w = m.size.0;
            *h = m.size.1;
        }
        true
    }
    unsafe extern "C" fn gui_can_resize(_p: *const RawPlugin) -> bool {
        true
    }
    unsafe extern "C" fn gui_adjust(_p: *const RawPlugin, w: *mut u32, h: *mut u32) -> bool {
        unsafe {
            *w = (*w).max(200);
            *h = (*h).max(200);
        }
        true
    }
    unsafe extern "C" fn gui_set_size(p: *const RawPlugin, w: u32, h: u32) -> bool {
        unsafe { mock(p) }.size = (w, h);
        true
    }
    unsafe extern "C" fn gui_set_parent(p: *const RawPlugin, window: *const Window) -> bool {
        let m = unsafe { mock(p) };
        let window = unsafe { &*window };
        #[cfg(target_os = "windows")]
        {
            m.parent = unsafe { window.handle.win32 as usize as u64 };
        }
        #[cfg(not(target_os = "windows"))]
        {
            #[allow(clippy::unnecessary_cast)]
            {
                m.parent = unsafe { window.handle.x11 as u64 };
            }
        }
        cstr(window.api) == GUI_API.to_string_lossy()
    }
    unsafe extern "C" fn gui_show(p: *const RawPlugin) -> bool {
        let m = unsafe { mock(p) };
        m.visible = true;
        let t: &HostTimerSupport = unsafe { host_ext(m.host, c"clap.timer-support") };
        unsafe { (t.register_timer.unwrap())(m.host, 10, &mut m.timer) };
        true
    }
    unsafe extern "C" fn gui_hide(p: *const RawPlugin) -> bool {
        unsafe { mock(p) }.visible = false;
        true
    }
    static M_GUI: PluginGui = PluginGui {
        is_api_supported: Some(gui_supported),
        get_preferred_api: None,
        create: Some(gui_create),
        destroy: Some(gui_destroy),
        set_scale: Some(gui_set_scale),
        get_size: Some(gui_get_size),
        can_resize: Some(gui_can_resize),
        get_resize_hints: None,
        adjust_size: Some(gui_adjust),
        set_size: Some(gui_set_size),
        set_parent: Some(gui_set_parent),
        set_transient: None,
        suggest_title: None,
        show: Some(gui_show),
        hide: Some(gui_hide),
    };

    unsafe extern "C" fn timer_cb(p: *const RawPlugin, id: u32) {
        let m = unsafe { mock(p) };
        if id == m.timer {
            m.timer_ticks += 1;
        }
    }
    static M_TIMER: PluginTimerSupport = PluginTimerSupport {
        on_timer: Some(timer_cb),
    };
    unsafe extern "C" fn fd_cb(p: *const RawPlugin, fd: c_int, flags: u32) {
        unsafe { mock(p) }.fd_events.push((fd, flags));
    }
    static M_FD: PluginPosixFdSupport = PluginPosixFdSupport { on_fd: Some(fd_cb) };

    unsafe extern "C" fn m_get_extension(p: *const RawPlugin, id: *const c_char) -> *const c_void {
        let kind = unsafe { mock(p) }.kind;
        let synth = kind == Kind::Synth;
        let audio = kind != Kind::Bare;
        let params = matches!(kind, Kind::Gain | Kind::Synth | Kind::MidiSynth);
        let notes = matches!(kind, Kind::Synth | Kind::MidiSynth | Kind::Untagged);
        match cstr(id).as_str() {
            "clap.audio-ports" if audio => (&M_AUDIO_PORTS as *const PluginAudioPorts).cast(),
            "clap.note-ports" if notes => (&M_NOTE_PORTS as *const PluginNotePorts).cast(),
            "clap.params" if params => (&M_PARAMS as *const PluginParams).cast(),
            "clap.state" if params => (&M_STATE as *const PluginState).cast(),
            "clap.latency" if kind == Kind::Gain => (&M_LATENCY as *const PluginLatency).cast(),
            "clap.gui" if synth => (&M_GUI as *const PluginGui).cast(),
            "clap.timer-support" if synth => (&M_TIMER as *const PluginTimerSupport).cast(),
            "clap.posix-fd-support" if synth => (&M_FD as *const PluginPosixFdSupport).cast(),
            _ => ptr::null(),
        }
    }

    // ----- helpers -----

    fn path() -> &'static Path {
        Path::new("/mock/Test.clap")
    }
    fn load(id: &str) -> ClapPlugin {
        ClapPlugin::from_entry(&MOCK_ENTRY, path(), Some(id)).unwrap()
    }
    fn state(p: &ClapPlugin) -> &'static mut Mock {
        unsafe { mock(p.plugin) }
    }
    fn bufs(buses: &[usize], frames: usize, v: f32) -> Vec<Vec<Vec<f32>>> {
        buses.iter().map(|&c| vec![vec![v; frames]; c]).collect()
    }
    fn run(
        p: &mut ClapPlugin,
        ins: &mut [Vec<Vec<f32>>],
        outs: &mut [Vec<Vec<f32>>],
        frames: usize,
    ) -> bool {
        p.process(ins, outs, frames, 48000, 120.0, false, 0)
    }

    // ----- tests -----

    #[test]
    fn abi_layout() {
        assert_eq!(size_of::<EventHeader>(), 16);
        assert_eq!(size_of::<NoteEvent>(), 40);
        assert_eq!(size_of::<MidiEvent>(), 24);
        assert_eq!(size_of::<ParamValueEvent>(), 56);
        assert_eq!(std::mem::offset_of!(ParamValueEvent, cookie), 24);
        assert_eq!(std::mem::offset_of!(ParamValueEvent, value), 48);
        assert_eq!(size_of::<TransportEvent>(), 104);
        assert_eq!(std::mem::offset_of!(TransportEvent, tempo), 40);
        assert_eq!(std::mem::offset_of!(TransportEvent, bar_number), 96);
        assert_eq!(size_of::<AudioBuffer>(), 32);
        assert_eq!(size_of::<Process>(), 64);
        assert_eq!(std::mem::offset_of!(Process, in_events), 48);
        assert_eq!(size_of::<AudioPortInfo>(), 288);
        assert_eq!(std::mem::offset_of!(AudioPortInfo, port_type), 272);
        assert_eq!(size_of::<NotePortInfo>(), 268);
        assert_eq!(size_of::<RawParamInfo>(), 1320);
        assert_eq!(std::mem::offset_of!(RawParamInfo, module), 272);
        assert_eq!(std::mem::offset_of!(RawParamInfo, min_value), 1296);
        assert_eq!(size_of::<RawPlugin>(), 96);
        assert_eq!(size_of::<RawHost>(), 88);
        assert_eq!(size_of::<Entry>(), 40);
        assert_eq!(size_of::<EventData>(), 56);
    }

    #[test]
    fn split_and_join_paths() {
        let plain = Path::new("/home/u/.clap/Synth.clap");
        assert_eq!(split_path(plain), (plain.to_path_buf(), None));

        let file = Path::new("/home/u/.clap/Bundle.clap");
        let joined = join_path(file, "org.example.synth");
        assert_eq!(
            joined,
            PathBuf::from("/home/u/.clap/Bundle.clap#org.example.synth")
        );
        assert_eq!(
            split_path(&joined),
            (file.to_path_buf(), Some("org.example.synth".to_string()))
        );
        // Ids with dots, dashes and a '#' in a directory name.
        let odd = join_path(Path::new("/a#b/Bundle.clap"), "com.x-y.z");
        assert_eq!(
            split_path(&odd),
            (
                PathBuf::from("/a#b/Bundle.clap"),
                Some("com.x-y.z".to_string())
            )
        );
        // Not a .clap file before the '#', or no id after it: left alone.
        for p in ["/a/dir#1/Plug.vst", "/a/x.clap#", "/a#b/c", "/a/x.clap"] {
            assert_eq!(split_path(Path::new(p)), (PathBuf::from(p), None), "{p}");
        }
        assert_eq!(
            split_path(Path::new("/a/X.CLAP#id")),
            (PathBuf::from("/a/X.CLAP"), Some("id".to_string()))
        );
        assert_eq!(join_path(file, ""), file.to_path_buf());
    }

    #[test]
    fn enumerates_and_classifies() {
        let descs = enumerate(&MOCK_ENTRY, path()).unwrap();
        let by = |id: &str| descs.iter().find(|d| d.id == id).unwrap();
        assert_eq!(descs.len(), 5);
        let gain = by(ID_GAIN);
        assert_eq!(
            (
                gain.name.as_str(),
                gain.vendor.as_str(),
                gain.category.as_str()
            ),
            ("Test Gain", "KFXL", "Fx")
        );
        assert!(!gain.instrument);
        let synth = by(ID_SYNTH);
        assert!(synth.instrument);
        assert_eq!(synth.category, "Instrument");
        // Tags compare case-insensitively.
        assert!(by(ID_MIDI).instrument);
        // No tags: an effect until the note ports say otherwise.
        assert!(!by(ID_UNTAGGED).instrument);
        assert_eq!(by(ID_UNTAGGED).category, "Fx");
        // Other primary feature, tidied.
        assert_eq!(by(ID_BARE).category, "Note effect");
        assert!(!by(ID_BARE).instrument);
    }

    #[test]
    fn classify_mixed_tags() {
        let f = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            classify(&f(&["audio-effect", "filter"])),
            (false, "Fx".into())
        );
        assert_eq!(
            classify(&f(&["audio-effect", "synthesizer"])),
            (true, "Instrument".into())
        );
        assert_eq!(classify(&f(&["drum-machine"])), (true, "Instrument".into()));
        assert_eq!(classify(&f(&["analyzer"])), (false, "Analyzer".into()));
        assert_eq!(classify(&[]), (false, "Fx".into()));
    }

    #[test]
    fn loads_first_plugin_without_id_and_rejects_unknown_id() {
        let p = ClapPlugin::from_entry(&MOCK_ENTRY, path(), None).unwrap();
        assert_eq!(p.desc().id, ID_GAIN);
        drop(p);
        let err = ClapPlugin::from_entry(&MOCK_ENTRY, path(), Some("nope")).err();
        assert_eq!(err.as_deref(), Some("plugin nope not found"));
    }

    #[test]
    fn load_failures_are_errors() {
        assert!(list_plugins(Path::new("/nonexistent/none.clap")).is_err());
        assert!(ClapPlugin::load(Path::new("/nonexistent/none.clap#x")).is_err());
        // A real shared library that is not a CLAP plugin.
        #[cfg(target_os = "windows")]
        let non_clap = Path::new("kernel32.dll");
        #[cfg(not(target_os = "windows"))]
        let non_clap = Path::new("libc.so.6");
        let err = list_plugins(non_clap).unwrap_err();
        assert!(err.contains("clap_entry"), "{err}");
    }

    #[test]
    fn lifecycle_and_entry_refcount() {
        take_log();
        let a = ClapPlugin::from_entry(&ISO_ENTRY, path(), Some(ID_GAIN)).unwrap();
        let b = ClapPlugin::from_entry(&ISO_ENTRY, path(), Some(ID_SYNTH)).unwrap();
        // The entry is initialised once for both instances.
        assert_eq!(take_log(), ["entry.init", "plugin.init", "plugin.init"]);
        drop(a);
        assert_eq!(take_log(), ["destroy"]);
        drop(b);
        assert_eq!(take_log(), ["destroy", "entry.deinit"]);

        // The full order, ended by an explicit stop and deactivate.
        let mut p = ClapPlugin::from_entry(&ISO_ENTRY, path(), Some(ID_GAIN)).unwrap();
        p.activate(48000.0, 128).unwrap();
        p.start_processing().unwrap();
        let mut ins = bufs(&[2], 64, 0.0);
        let mut outs = bufs(&[2], 64, 0.0);
        assert!(run(&mut p, &mut ins, &mut outs, 64));
        p.stop_processing();
        p.deactivate();
        drop(p);
        assert_eq!(
            take_log(),
            [
                "entry.init",
                "plugin.init",
                "activate",
                "start",
                "process",
                "stop",
                "deactivate",
                "destroy",
                "entry.deinit"
            ]
        );

        // Drop tears down a running plugin in order.
        let mut p = ClapPlugin::from_entry(&ISO_ENTRY, path(), Some(ID_GAIN)).unwrap();
        p.activate(44100.0, 64).unwrap();
        p.start_processing().unwrap();
        drop(p);
        assert_eq!(
            take_log(),
            [
                "entry.init",
                "plugin.init",
                "activate",
                "start",
                "stop",
                "deactivate",
                "destroy",
                "entry.deinit"
            ]
        );
    }

    #[test]
    fn inactive_plugin_does_not_process() {
        let mut p = load(ID_GAIN);
        let mut ins = bufs(&[2], 64, 1.0);
        let mut outs = bufs(&[2], 64, 0.0);
        assert!(!run(&mut p, &mut ins, &mut outs, 64));
        p.activate(48000.0, 64).unwrap();
        // More frames than `activate` allowed, and none at all.
        assert!(!run(&mut p, &mut ins, &mut outs, 65));
        assert!(!run(&mut p, &mut ins, &mut outs, 0));
        // `process` starts processing on its own.
        assert!(run(&mut p, &mut ins, &mut outs, 64));
        p.deactivate();
        assert!(!run(&mut p, &mut ins, &mut outs, 64));
        assert!(p.start_processing().is_err());
    }

    #[test]
    fn effect_gain_and_transport() {
        let mut p = load(ID_GAIN);
        assert!(!p.is_instrument());
        assert_eq!(p.bus_layout().inputs, [2]);
        assert_eq!(p.bus_layout().outputs, [2]);
        p.activate(48000.0, 256).unwrap();
        p.start_processing().unwrap();

        let mut ins = bufs(&[2], 128, 0.5);
        let mut outs = bufs(&[2], 128, 9.0);
        // 150 BPM, one second in: 2.5 beats.
        assert!(p.process(&mut ins, &mut outs, 128, 48000, 150.0, true, 48000));
        // Default 0 dB: the input unchanged (the 9.0 was cleared, not kept).
        assert!(outs[0].iter().flatten().all(|&x| (x - 0.5).abs() < 1e-6));
        let m = state(&p);
        assert_eq!(m.tempo, 150.0);
        assert_eq!(m.seconds, FIXED_ONE);
        assert_eq!(m.beats, (2.5 * FIXED_ONE as f64) as i64);
        assert_eq!(m.tsig, (4, 4));
        assert_eq!(m.flags, 1 | 2 | 4 | 8 | 16);
        // Stopped: the playing bit is gone, the rest stays.
        assert!(p.process(&mut ins, &mut outs, 128, 48000, 90.0, false, 0));
        let m = state(&p);
        assert_eq!(m.flags, 1 | 2 | 4 | 8);
        assert_eq!(m.tempo, 90.0);
        assert_eq!(m.beats, 0);
        // steady_time advances by the frames of each block.
        assert_eq!(m.steady, [0, 128]);
        assert_eq!(m.frames, [128, 128]);
        // The plugin sees itself on the audio thread inside `process`.
        assert_eq!(m.thread_audio, [true, true]);

        // Missing buses and channels: silence in, scratch out, no crash.
        assert!(p.process(&mut [], &mut [], 64, 48000, 120.0, true, 0));
        let mut half_in = vec![vec![vec![1.0f32; 64]; 1]];
        let mut short_out = vec![vec![vec![7.0f32; 8]; 2]];
        assert!(p.process(&mut half_in, &mut short_out, 64, 48000, 120.0, true, 0));
        assert!(short_out[0][0].iter().all(|&x| x == 7.0));
    }

    #[test]
    fn synth_notes_in_order_at_their_offsets() {
        let mut p = load(ID_SYNTH);
        assert!(p.is_instrument());
        assert_eq!(p.bus_layout().inputs, [] as [usize; 0]);
        assert_eq!(p.bus_layout().outputs, [2, 1]);
        p.activate(48000.0, 512).unwrap();
        p.start_processing().unwrap();

        let mut outs = bufs(&[2, 1], 256, 0.0);
        // Queued out of order: sorted by time before the plugin sees them.
        p.queue_note_off(200, 69);
        p.queue_note_on(10, 69, 127);
        p.queue_note_on(10, 72, 127);
        assert!(p.process(&mut [], &mut outs, 256, 48000, 120.0, true, 0));
        let m = state(&p);
        assert_eq!(m.event_times, [10, 10, 200]);
        assert_eq!(m.note_events, 3);
        assert_eq!(m.key, 72);
        // Silent before the note, a tone after it.
        assert!(outs[0][0][..10].iter().all(|&x| x == 0.0));
        let peak = outs[0][0][10..200]
            .iter()
            .fold(0f32, |a, &x| a.max(x.abs()));
        assert!(peak > 0.3 && peak <= 0.5 + 1e-6, "peak {peak}");
        assert_eq!(outs[0][0], outs[0][1]);
        let side = outs[1][0][10..200]
            .iter()
            .fold(0f32, |a, &x| a.max(x.abs()));
        assert!(
            (side - peak * 0.5).abs() < 0.05,
            "mono bus {side} vs {peak}"
        );

        // Events are consumed: the next block adds none to what the plugin saw.
        let mut outs = bufs(&[2, 1], 256, 0.0);
        assert!(p.process(&mut [], &mut outs, 256, 48000, 120.0, true, 256));
        assert_eq!(state(&p).event_times, [10, 10, 200]);
        // The note-off at 200 was for key 69; the 72 note rings on.
        assert!(outs[0][0].iter().any(|&x| x.abs() > 0.1));

        // An offset past the end of the block lands on its last frame.
        p.queue_note_off(10_000, 72);
        assert!(p.process(&mut [], &mut outs, 256, 48000, 120.0, true, 512));
        assert_eq!(*state(&p).event_times.last().unwrap(), 255);
    }

    #[test]
    fn midi_dialect_when_the_plugin_prefers_it() {
        let mut p = load(ID_MIDI);
        assert!(p.use_midi);
        p.activate(48000.0, 256).unwrap();
        let mut outs = bufs(&[2], 128, 0.0);
        p.queue_note_on(0, 60, 0);
        p.queue_note_on(4, 64, 100);
        p.queue_note_off(60, 64);
        p.queue_midi(70, [0xB0, 1, 64]);
        assert!(p.process(&mut [], &mut outs, 128, 48000, 120.0, true, 0));
        let m = state(&p);
        assert_eq!(m.note_events, 0);
        // Velocity 0 becomes 1 on a MIDI note on; other messages pass through.
        assert_eq!(
            m.midi,
            [[0x90, 60, 1], [0x90, 64, 100], [0x80, 64, 0], [0xB0, 1, 64]]
        );
        assert!(outs[0][0][8..60].iter().any(|&x| x.abs() > 0.05));
    }

    #[test]
    fn clap_dialect_gets_translated_midi() {
        let mut p = load(ID_SYNTH);
        assert!(!p.use_midi && p.midi_ok);
        p.activate(48000.0, 128).unwrap();
        // The synth prefers CLAP notes: note on goes as a note event.
        p.queue_note_on(0, 60, 64);
        let mut outs = bufs(&[2, 1], 64, 0.0);
        assert!(p.process(&mut [], &mut outs, 64, 48000, 120.0, true, 0));
        assert_eq!((state(&p).note_events, state(&p).midi.len()), (1, 0));
        assert!((state(&p).amp - 64.0 / 127.0 * 0.5).abs() < 1e-9);

        // CLAP-only plugin: MIDI note messages turn into note events, CC is dropped.
        let mut u = load(ID_UNTAGGED);
        assert!(!u.midi_ok);
        u.activate(48000.0, 128).unwrap();
        u.queue_midi(0, [0x92, 61, 100]);
        u.queue_midi(5, [0x82, 61, 0]);
        u.queue_midi(6, [0x92, 61, 0]);
        u.queue_midi(7, [0xB0, 7, 100]);
        let mut outs = bufs(&[2], 64, 0.0);
        assert!(u.process(&mut [], &mut outs, 64, 48000, 120.0, true, 0));
        assert_eq!((state(&u).note_events, state(&u).midi.len()), (3, 0));
    }

    #[test]
    fn untagged_plugin_with_notes_is_an_instrument() {
        let u = load(ID_UNTAGGED);
        assert!(!u.desc().instrument);
        assert!(u.is_instrument());
        // A tagged note effect with no audio is not.
        assert!(!load(ID_BARE).is_instrument());
        assert!(!load(ID_GAIN).is_instrument());
    }

    #[test]
    fn parameters_listed_and_mapped() {
        let p = load(ID_GAIN);
        let infos = p.param_infos();
        assert_eq!(infos.len(), 3);
        assert_eq!(infos[0].cookie, 0xC00C1E);
        assert_eq!(infos[0].module, "Mixer");
        assert!(infos[1].stepped && infos[2].read_only);

        // Hidden and read-only parameters are not offered.
        let listed = p.params();
        assert_eq!(listed.len(), 2);
        let (id, name, norm, text, auto) = &listed[0];
        assert_eq!(
            (*id, name.as_str(), text.as_str(), *auto),
            (7, "Gain", "0.0 dB", true)
        );
        assert!((norm - 24.0 / 36.0).abs() < 1e-12);
        let (id, name, norm, text, _) = &listed[1];
        assert_eq!((*id, name.as_str(), text.as_str()), (8, "Steps", "step 2"));
        assert!((norm - 0.5).abs() < 1e-12);

        let gain = &infos[0];
        assert_eq!(gain.to_plain(0.0), -24.0);
        assert_eq!(gain.to_plain(1.0), 12.0);
        assert!((gain.to_plain(24.0 / 36.0)).abs() < 1e-12);
        assert_eq!(gain.to_plain(7.0), 12.0);
        assert_eq!(gain.to_plain(f64::NAN), -24.0);
        assert!((gain.to_norm(-6.0) - 0.5).abs() < 1e-12);
        assert_eq!(gain.to_norm(100.0), 1.0);
        // Stepped parameters land on whole numbers.
        let steps = &infos[1];
        assert_eq!(steps.to_plain(0.37), 1.0);
        assert_eq!(steps.to_plain(0.63), 3.0);
        // A flat range does not divide by zero.
        let flat = ClapParamInfo {
            min: 1.0,
            max: 1.0,
            ..gain.clone()
        };
        assert_eq!(flat.to_norm(1.0), 0.0);
    }

    #[test]
    fn param_changes_reach_the_plugin() {
        let mut p = load(ID_GAIN);
        p.activate(48000.0, 256).unwrap();
        // Not processing yet: params.flush delivers the change at once.
        p.set_param_normalized(7, 0.5);
        assert_eq!(state(&p).flushes, 1);
        assert!((state(&p).gain_db - -6.0).abs() < 1e-12);
        assert_eq!(state(&p).cookie, 0xC00C1E);
        assert!((p.param_value(7).unwrap() - -6.0).abs() < 1e-12);
        let text = p.params()[0].3.clone();
        assert_eq!(text, "-6.0 dB");
        // Unknown and read-only ids are ignored; out-of-range norms clamp.
        p.set_param_normalized(99, 0.5);
        p.set_param_normalized(9, 0.5);
        p.set_param_normalized(7, 5.0);
        assert_eq!(p.param_value(7), Some(12.0));
        p.set_param_normalized(8, 0.37);
        assert_eq!(p.param_value(8), Some(1.0));

        // While processing the change rides with the next block, sample-exact
        // from its start, and repeated sets of one parameter collapse to one.
        p.start_processing().unwrap();
        let before = state(&p).flushes;
        p.set_param_normalized(7, 0.0);
        p.set_param_normalized(7, 0.0);
        p.set_param_normalized(7, 24.0 / 36.0 - 1e-12);
        assert_eq!(state(&p).flushes, before);
        assert_eq!(state(&p).param_events, 3, "only the flushes so far");
        let mut ins = bufs(&[2], 64, 1.0);
        let mut outs = bufs(&[2], 64, 0.0);
        assert!(run(&mut p, &mut ins, &mut outs, 64));
        assert_eq!(state(&p).param_events, 4);
        assert!(state(&p).gain_db.abs() < 1e-6);
        assert!((outs[0][0][0] - 1.0).abs() < 1e-3);

        // Quieter: -24 dB.
        p.set_param_normalized(7, 0.0);
        assert!(run(&mut p, &mut ins, &mut outs, 64));
        let want = 10f32.powf(-24.0 / 20.0);
        assert!((outs[0][0][5] - want).abs() < 1e-6);
        assert!((outs[0][1][63] - want).abs() < 1e-6);
    }

    #[test]
    fn plugin_reported_param_changes_update_the_cache() {
        let mut p = load(ID_SYNTH);
        p.activate(48000.0, 64).unwrap();
        state(&p).emit_param = Some((8, 3.0));
        let mut outs = bufs(&[2, 1], 64, 0.0);
        assert!(p.process(&mut [], &mut outs, 64, 48000, 120.0, true, 0));
        assert_eq!(p.param_value(8), Some(3.0));
        assert!((p.params()[1].2 - 0.75).abs() < 1e-12);
    }

    #[test]
    fn state_round_trip_in_chunks() {
        let mut p = load(ID_GAIN);
        p.activate(48000.0, 64).unwrap();
        p.set_param_normalized(7, 0.5);
        p.set_param_normalized(8, 1.0);
        // A tail well past one stream chunk.
        state(&p).tail = (0..100_003u32).map(|i| (i % 251) as u8).collect();
        let saved = p.save_state().unwrap();
        assert_eq!(saved.len(), 16 + 100_003);

        p.set_param_normalized(7, 1.0);
        p.set_param_normalized(8, 0.0);
        state(&p).tail.clear();
        p.load_state(&saved).unwrap();
        let m = state(&p);
        assert!((m.gain_db - -6.0).abs() < 1e-12 && m.steps == 4.0);
        assert_eq!(m.tail.len(), 100_003);
        assert_eq!(m.tail[251], 0);
        assert_eq!(m.tail[100_002], (100_002 % 251) as u8);
        // The host's parameter view follows the restored state.
        assert!((p.param_value(7).unwrap() - -6.0).abs() < 1e-12);
        assert_eq!(p.param_value(8), Some(4.0));
        assert_eq!(p.params()[1].3, "step 4");

        // A state the plugin refuses is an error; the plugin is unchanged.
        assert!(p.load_state(&saved[..9]).is_err());
        assert!(p.load_state(&[]).is_err());
        // No state extension: nothing to save or load.
        let mut bare = load(ID_BARE);
        assert!(bare.save_state().is_none());
        assert!(bare.load_state(&saved).is_err());
    }

    #[test]
    fn latency_is_reported() {
        let mut p = load(ID_GAIN);
        assert_eq!(p.latency_samples(), 128);
        p.activate(48000.0, 64).unwrap();
        assert_eq!(p.latency_samples(), 128);
        state(&p).latency = 300;
        assert_eq!(p.latency_samples(), 300);
        assert_eq!(load(ID_SYNTH).latency_samples(), 0);
        assert_eq!(load(ID_BARE).latency_samples(), 0);
    }

    #[test]
    fn missing_extensions_fall_back() {
        let mut p = load(ID_BARE);
        // No audio-ports extension: one stereo bus each way.
        assert_eq!(p.bus_layout().inputs, [2]);
        assert_eq!(p.bus_layout().outputs, [2]);
        assert!(p.params().is_empty() && p.param_infos().is_empty());
        assert!(!p.has_editor());
        assert!(p.editor_open(1).is_err());
        p.editor_set_size(10, 10);
        p.editor_close();
        p.set_param_normalized(7, 1.0);
        assert_eq!(p.service(), None);
        p.activate(48000.0, 64).unwrap();
        let mut ins = bufs(&[2], 64, 0.0);
        let mut outs = bufs(&[2], 64, 0.0);
        assert!(run(&mut p, &mut ins, &mut outs, 64));
        // The plugin without note ports still takes (translated) notes quietly.
        p.queue_note_on(0, 60, 100);
        assert!(run(&mut p, &mut ins, &mut outs, 64));
    }

    #[test]
    fn editor_lifecycle_resize_callback_and_timer() {
        let mut p = load(ID_SYNTH);
        assert!(p.has_editor());
        assert!(!load(ID_GAIN).has_editor());
        p.activate(48000.0, 64).unwrap();

        assert_eq!(p.editor_open(0x4200_0001), Ok((640, 480)));
        {
            let m = state(&p);
            assert_eq!(m.api, GUI_API.to_string_lossy());
            assert_eq!(m.parent, 0x4200_0001);
            assert!(m.created && m.visible);
            // A refused set_scale is not an error.
            assert_eq!(m.scale_calls, 1);
        }
        // Opening again just reports the size.
        assert_eq!(p.editor_open(0x4200_0001), Ok((640, 480)));
        assert_eq!(state(&p).scale_calls, 1);

        // Nothing requested yet.
        assert_eq!(p.service(), None);

        // The plugin asks for a bigger window through the host extension.
        let host = &p.host.host as *const RawHost;
        let gui: &HostGui = unsafe { host_ext(host, c"clap.gui") };
        assert!(unsafe { (gui.request_resize.unwrap())(host, 900, 700) });
        assert!(!unsafe { (gui.request_resize.unwrap())(host, 0, 0) });
        assert_eq!(p.service(), Some((900, 700)));
        assert_eq!(p.service(), None);

        // request_callback: the plugin's on_main_thread runs on the next service.
        unsafe { (p.host.host.request_callback.unwrap())(host) };
        assert_eq!(state(&p).main_thread_calls, 0);
        p.service();
        assert_eq!(state(&p).main_thread_calls, 1);
        p.service();
        assert_eq!(state(&p).main_thread_calls, 1);

        // The 10 ms timer the editor registered fires from service().
        assert_ne!(state(&p).timer, 0);
        lock(&p.host.timers)[0].next = Instant::now() + Duration::from_secs(60);
        p.service();
        assert_eq!(state(&p).timer_ticks, 0, "not due yet");
        lock(&p.host.timers)[0].next = Instant::now() - Duration::from_millis(5);
        p.service();
        assert_eq!(state(&p).timer_ticks, 1);
        assert!(lock(&p.host.timers)[0].next > Instant::now(), "rescheduled");
        p.service();
        assert_eq!(state(&p).timer_ticks, 1);

        // The window followed: resize within the plugin's limits.
        p.editor_set_size(800, 600);
        assert_eq!(state(&p).size, (800, 600));
        p.editor_set_size(10, 10);
        assert_eq!(state(&p).size, (200, 200));

        p.editor_close();
        let m = state(&p);
        assert!(!m.visible && !m.created);
        assert_eq!(m.timer, 0);
        assert!(lock(&p.host.timers).is_empty());
        // Closing twice is harmless.
        p.editor_close();
        // After the editor is gone a size change is ignored.
        p.editor_set_size(500, 500);
        assert_eq!(state(&p).size, (200, 200));
    }

    #[test]
    fn dropping_an_open_editor_closes_it() {
        take_log();
        let mut p = load(ID_SYNTH);
        p.editor_open(5).unwrap();
        drop(p);
        let log = take_log();
        assert_eq!(log.iter().filter(|l| **l == "gui.destroy").count(), 1);
        assert_eq!(log.last(), Some(&"destroy"));
    }

    #[test]
    #[cfg(unix)]
    fn posix_fd_support_polls_registered_fds() {
        let mut p = load(ID_SYNTH);
        let mut fds = [0 as c_int; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let host = &p.host.host as *const RawHost;
        let ext: &HostPosixFdSupport = unsafe { host_ext(host, c"clap.posix-fd-support") };
        assert!(unsafe { (ext.register_fd.unwrap())(host, fds[0], FD_READ) });
        assert!(!unsafe { (ext.register_fd.unwrap())(host, -1, FD_READ) });

        p.service();
        assert!(state(&p).fd_events.is_empty());
        assert_eq!(unsafe { libc::write(fds[1], b"x".as_ptr().cast(), 1) }, 1);
        p.service();
        assert_eq!(state(&p).fd_events, [(fds[0], FD_READ)]);

        // Unregistered: silent even with data waiting.
        assert!(unsafe { (ext.unregister_fd.unwrap())(host, fds[0]) });
        assert!(!unsafe { (ext.unregister_fd.unwrap())(host, fds[0]) });
        p.service();
        assert_eq!(state(&p).fd_events.len(), 1);

        // A writer closing is reported as an error/hangup on a read watch.
        assert!(unsafe { (ext.register_fd.unwrap())(host, fds[0], FD_READ) });
        assert!(unsafe { (ext.modify_fd.unwrap())(host, fds[0], FD_READ | FD_ERROR) });
        unsafe { libc::close(fds[1]) };
        let mut c = [0u8; 1];
        unsafe { libc::read(fds[0], c.as_mut_ptr().cast(), 1) };
        p.service();
        let last = *state(&p).fd_events.last().unwrap();
        assert_eq!(last.0, fds[0]);
        assert_ne!(last.1 & FD_ERROR, 0);
        unsafe { libc::close(fds[0]) };
    }

    #[test]
    fn host_extensions_are_exposed() {
        let p = load(ID_GAIN);
        let host = &p.host.host as *const RawHost;
        let get = |id: &CStr| unsafe { ((*host).get_extension.unwrap())(host, id.as_ptr()) };
        for id in [
            c"clap.gui",
            c"clap.params",
            c"clap.latency",
            c"clap.log",
            c"clap.thread-check",
            c"clap.timer-support",
            #[cfg(unix)]
            c"clap.posix-fd-support",
        ] {
            assert!(!get(id).is_null(), "{id:?}");
        }
        #[cfg(not(unix))]
        assert!(get(c"clap.posix-fd-support").is_null());
        assert!(get(c"clap.nonexistent").is_null());

        // Off the audio thread, the main thread answers true.
        let tc: &HostThreadCheck = unsafe { host_ext(host, c"clap.thread-check") };
        assert!(unsafe { (tc.is_main_thread.unwrap())(host) });
        assert!(!unsafe { (tc.is_audio_thread.unwrap())(host) });
        // Logging does not crash (warnings go to stderr).
        let log: &HostLog = unsafe { host_ext(host, c"clap.log") };
        unsafe { (log.log.unwrap())(host, 2, c"test warning".as_ptr()) };
        unsafe { (log.log.unwrap())(host, 1, c"test info".as_ptr()) };
        // Latency and params change requests are accepted.
        let lat: &HostLatency = unsafe { host_ext(host, c"clap.latency") };
        unsafe { (lat.changed.unwrap())(host) };
        assert!(p.host.latency_changed.load(Ordering::Acquire));
        // Timers: periods are floored, ids are unique and unregister works.
        let t: &HostTimerSupport = unsafe { host_ext(host, c"clap.timer-support") };
        let (mut a, mut b) = (0u32, 0u32);
        assert!(unsafe { (t.register_timer.unwrap())(host, 1, &mut a) });
        assert!(unsafe { (t.register_timer.unwrap())(host, 1000, &mut b) });
        assert_ne!(a, b);
        assert_eq!(lock(&p.host.timers)[0].period, Duration::from_millis(10));
        assert!(unsafe { (t.unregister_timer.unwrap())(host, a) });
        assert!(!unsafe { (t.unregister_timer.unwrap())(host, a) });
        assert!(!unsafe { (t.register_timer.unwrap())(host, 10, ptr::null_mut()) });
    }

    #[test]
    fn params_rescan_rereads_the_list() {
        let mut p = load(ID_GAIN);
        let host = &p.host.host as *const RawHost;
        let hp: &HostParams = unsafe { host_ext(host, c"clap.params") };
        state(&p).gain_db = -12.0;
        unsafe { (hp.rescan.unwrap())(host, 1) };
        p.service();
        assert_eq!(p.param_value(7), Some(-12.0));
        // request_flush while idle flushes the plugin.
        let before = state(&p).flushes;
        unsafe { (hp.request_flush.unwrap())(host) };
        p.service();
        assert_eq!(state(&p).flushes, before + 1);
    }
}
