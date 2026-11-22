//! VST3, VST2, and CLAP plugin scanning, audio processing, and embedded editors.
//!
//! * **Scan** ([`scan`]): every bundle under the VST3/VST2/CLAP folders is probed
//!   in a throwaway child process (`keyframexl plugin-probe <path>`): it loads
//!   the plugin, plays a note, and reports what it is. A plugin that crashes or
//!   hangs is marked as such and never loaded into the program. Results are
//!   cached by path and modification time.
//! * **Host thread** ([`host`]): one thread owns every loaded plugin (`vst3-host`
//!   requires the loading thread to drive the controller), keeps their embedded
//!   editor containers (X11 child windows on Linux, Win32 child `HWND`s on
//!   Windows), and runs their event loops at 60 Hz.
//! * **Audio**: each instance reaches the engine as a `Processor` that locks the
//!   plugin for one block.

#[cfg(target_os = "windows")]
mod editor_win32;
#[cfg(target_os = "linux")]
mod editor_x11;
pub mod host;
pub mod scan;
pub mod wine;

use serde::{Deserialize, Serialize};
use std::path::Path;

pub use host::{HostHandle, ParamRow, Reply, Req, start_host};
pub use scan::{
    Scanned, Status, clap_dirs, load_cache, probe_main, rescan_one, scan, search_dirs, vst2_dirs,
};
#[allow(unused_imports)]
pub use wine::{
    apply_wine, apply_wine_for, foreign_wineserver, has_dxvk, set_dxvk, set_wine,
    stop_foreign_wineserver, wine_builds, wine_prefix,
};
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use wine::{is_32bit_windows, wine_bin};

/// Largest block handed to a plugin at once (bigger engine blocks are split).
pub const MAX_BLOCK: usize = 2048;

/// Platform window handle of the host eframe window, used for embedding plugin
/// editors and querying cursor coordinates during drag-and-drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeWindow {
    X11(u32),
    Win32(u64),
}

/// `"clap"` for a CLAP library (`.clap` or `.clap#<id>`), `"vst2"` for a VST2
/// library (`.so` on Linux, `.dll` on Windows), `"fl"` for the FL Studio live
/// link, else `"vst3"`.
pub fn format_of(path: &Path) -> &'static str {
    if crate::fl_link::is_link(path) {
        return "fl";
    }
    let (file, _) = crate::clap::split_path(path);
    if file
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("clap"))
    {
        return "clap";
    }
    if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("so") || x.eq_ignore_ascii_case("dll"))
    {
        "vst2"
    } else {
        "vst3"
    }
}

/// The pointer over the host window `win`, in its physical client pixels.
pub fn pointer_in(win: NativeWindow) -> Option<(i32, i32)> {
    #[cfg(target_os = "linux")]
    {
        editor_x11::pointer_in(win)
    }
    #[cfg(target_os = "windows")]
    {
        editor_win32::pointer_in(win)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = win;
        None
    }
}
