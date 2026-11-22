//! Platform abstraction layer (`docs/v0.2.1.md`, W0/W1).
//!
//! Keeps OS-specific desktop integration (opening folders/URLs, system dark/light
//! theme, system locale, CJK font lookup, desktop stderr log file, process exit,
//! child-process console hiding, and FL Studio pack discovery) in one place.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(target_os = "linux"))]
pub mod stubs;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "windows")]
use windows as imp;

use std::path::{Path, PathBuf};

/// Open a local folder or file in the desktop's file manager / default app.
pub fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::open_path(path)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Open an http/https URL in the user's default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::open_url(url)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Query the operating system's dark/light interface preference.
pub fn query_system_theme() -> Option<crate::theme::Theme> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::query_system_theme()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Query the OS UI locale tag (e.g. `"uk-UA"`, `"uk_UA.UTF-8"`, `"en-US"`).
pub fn system_locale() -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::system_locale()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        posix_locale_env()
    }
}

/// Standard POSIX locale env vars (`LC_ALL`, `LC_MESSAGES`, `LANG`, `LANGUAGE`).
pub(crate) fn posix_locale_env() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Find a system font that contains Japanese kana (for Vocal / UTAU lyrics).
pub fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::find_cjk_font()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Redirect stderr to a rotating log file when launched from the desktop
/// without a controlling terminal.
pub fn setup_log_file() {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    imp::setup_log_file();
}

/// One-time process-level startup hygiene (e.g. restoring an inherited
/// soft `RLIMIT_RTTIME` cap on Linux after an in-place updater restart).
pub fn prepare_process() {
    #[cfg(target_os = "linux")]
    imp::prepare_process();
}

/// Terminate the process immediately after saving/unloading, skipping C/C++
/// static destructors that buggy plugin libraries can crash or deadlock in.
pub fn exit_immediately(code: i32) -> ! {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::exit_immediately(code)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        std::process::exit(code)
    }
}

/// Prevent a spawned background CLI helper (such as `ffmpeg`) from flashing a
/// console window on Windows.
pub fn hide_console(cmd: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    imp::hide_console(cmd);
    #[cfg(not(target_os = "windows"))]
    let _ = cmd;
}

/// Re-attach to the parent console when a GUI-subsystem Windows binary is
/// invoked with CLI subcommands from cmd/PowerShell.
pub fn attach_parent_console() {
    #[cfg(target_os = "windows")]
    imp::attach_parent_console();
}

/// Candidate `Image-Line` installation roots where FL Studio `Data/Patches/Packs`
/// may live on this OS.
pub fn fl_image_line_roots(home: &Path) -> Vec<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::fl_image_line_roots(home)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = home;
        Vec::new()
    }
}

/// Active FL Studio installation directory (e.g. `FL Studio 2025`), if installed.
pub fn fl_install_dir() -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::fl_install_dir()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// FL Studio user data folder (`Documents/Image-Line`), native on Windows or
/// inside the Wine prefix on Linux.
pub fn fl_user_data_dir() -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::fl_user_data_dir()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Default folders scanned for `.dwp` sound libraries on this OS.
pub fn default_mounts(home: &Path) -> Vec<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        imp::default_mounts(home)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let mut v = Vec::new();
        v.extend(dirs::audio_dir());
        v.push(home.join("Music"));
        v.dedup();
        v
    }
}
