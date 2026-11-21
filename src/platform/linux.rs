//! Linux desktop integration: xdg-open, xdg-desktop-portal theme, fontconfig,
//! `~/.local/state` stderr log file, Wine FL Studio roots, and mount points.

use crate::theme::Theme;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn open_path(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn open_url(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Query xdg-desktop-portal (`org.freedesktop.appearance color-scheme`).
pub fn query_system_theme() -> Option<Theme> {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.ReadOne",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;
    crate::theme::parse_color_scheme(&String::from_utf8_lossy(&out.stdout))
}

pub fn system_locale() -> Option<String> {
    super::posix_locale_env()
}

/// A system font that has kana, found through fontconfig (`fc-match`).
pub fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    let out = Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", "sans:lang=ja"])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines();
    let file = lines.next()?.trim();
    let index = lines
        .next()
        .and_then(|i| i.trim().parse().ok())
        .unwrap_or(0);
    let bytes = std::fs::read(file).ok()?;
    (bytes.len() > 1000).then_some((bytes, index))
}

/// Started from the desktop (no terminal): redirect stderr to
/// `~/.local/state/keyframexl/keyframexl.log` (previous run to `.log.1`).
pub fn setup_log_file() {
    use std::os::fd::AsRawFd;
    // SAFETY: isatty only reads the descriptor's state.
    if unsafe { libc::isatty(2) } == 1 {
        return;
    }
    let Some(dir) = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .map(|d| d.join("keyframexl"))
    else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("keyframexl.log");
    let _ = std::fs::rename(&path, dir.join("keyframexl.log.1"));
    let Ok(file) = std::fs::File::create(&path) else {
        return;
    };
    // SAFETY: fd 2 is replaced by a file we keep open for the program's life;
    // children (plugin processes) inherit it.
    unsafe { libc::dup2(file.as_raw_fd(), 2) };
    std::mem::forget(file);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    eprintln!(
        "KeyFrameXL {} started (unix time {stamp})",
        keyframexl::version::display()
    );
}

pub fn exit_immediately(code: i32) -> ! {
    // SAFETY: nothing is left to run; `_exit` skips C atexit handlers that
    // plugin libraries (yabridge among them) can abort in.
    unsafe { libc::_exit(code) }
}

/// Wine prefixes in `home` (`.wine`, `.wine-fl`, …) → `drive_c/Program Files/Image-Line`.
pub fn fl_image_line_roots(home: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(home)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with(".wine"))
                })
                .map(|p| p.join("drive_c/Program Files/Image-Line"))
                .collect()
        })
        .unwrap_or_default()
}

/// Mount points worth a look: `/mnt/*`, `/media/$USER/*`, `/run/media/$USER/*`, Music.
pub fn default_mounts(home: &Path) -> Vec<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let mut v = subdirs(Path::new("/mnt"));
    v.extend(subdirs(&Path::new("/media").join(&user)));
    v.extend(subdirs(&Path::new("/run/media").join(&user)));
    v.extend(dirs::audio_dir());
    v.push(home.join("Music"));
    v.dedup();
    v
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && !p
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}
