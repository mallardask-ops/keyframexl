//! Windows desktop integration: ShellExecuteW, Registry dark/light theme,
//! `GetUserDefaultLocaleName`, Windows CJK fonts, `%LOCALAPPDATA%` stderr log
//! file, `ExitProcess`, `CREATE_NO_WINDOW`, and native FL Studio paths.

use crate::theme::Theme;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::IntoRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn to_wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn shell_open(target: &OsStr) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = to_wide(OsStr::new("open"));
    let file = to_wide(target);
    // SAFETY: both pointers are NUL-terminated UTF-16 strings alive for the call.
    let rc = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if rc <= 32 {
        Err(format!("ShellExecuteW failed (code {rc})"))
    } else {
        Ok(())
    }
}

pub fn open_path(path: &Path) -> Result<(), String> {
    shell_open(path.as_os_str())
}

pub fn open_url(url: &str) -> Result<(), String> {
    shell_open(OsStr::new(url))
}

/// Read `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`.
/// `0` = dark, non-zero = light.
pub fn query_system_theme() -> Option<Theme> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let subkey = to_wide(OsStr::new(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
    ));
    let value = to_wide(OsStr::new("AppsUseLightTheme"));
    let mut data: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    // SAFETY: valid NUL-terminated wide strings and aligned u32 output buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    if status == ERROR_SUCCESS {
        Some(parse_apps_use_light_theme(data))
    } else {
        None
    }
}

fn parse_apps_use_light_theme(val: u32) -> Theme {
    if val == 0 { Theme::Dark } else { Theme::Light }
}

/// Query Windows user default locale (`uk-UA`, `en-US`, …), falling back to
/// POSIX env vars if set (for tests or Git Bash).
pub fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    if let Some(env_loc) = super::posix_locale_env() {
        return Some(env_loc);
    }
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    // SAFETY: valid writable UTF-16 buffer of length 85.
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if len > 1 {
        let s = String::from_utf16_lossy(&buf[..(len as usize - 1)]);
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// System font containing Japanese kana (for Vocal / UTAU lyrics).
pub fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts = windir.join("Fonts");
    for name in [
        "YuGothM.ttc",
        "YuGothR.ttc",
        "meiryo.ttc",
        "msgothic.ttc",
        "yugothm.ttc",
    ] {
        let p = fonts.join(name);
        if let Ok(bytes) = std::fs::read(&p)
            && bytes.len() > 1000
        {
            return Some((bytes, 0));
        }
    }
    None
}

/// Redirect stderr to `%LOCALAPPDATA%\keyframexl\keyframexl.log` when launched
/// without an attached console window.
pub fn setup_log_file() {
    use windows_sys::Win32::System::Console::{GetConsoleWindow, STD_ERROR_HANDLE, SetStdHandle};

    // SAFETY: querying whether this process owns a console window.
    if unsafe { !GetConsoleWindow().is_null() } {
        return;
    }
    let Some(dir) = dirs::data_local_dir().map(|d| d.join("keyframexl")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("keyframexl.log");
    let old = dir.join("keyframexl.log.1");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(&path, &old);
    let Ok(file) = std::fs::File::create(&path) else {
        return;
    };
    let handle = file.into_raw_handle();
    // SAFETY: handle stays open for the lifetime of the process; Rust's stderr
    // writes through STD_ERROR_HANDLE and children inherit it.
    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, handle);
    }
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
    use windows_sys::Win32::System::Threading::ExitProcess;
    // SAFETY: called only after saving, stopping audio, and unloading plugins.
    unsafe { ExitProcess(code as u32) }
}

pub fn hide_console(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

pub fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: best-effort attach to parent cmd/PowerShell when invoked via CLI.
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Native Windows FL Studio install roots (`Program Files\Image-Line`).
pub fn fl_image_line_roots(_home: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            roots.push(PathBuf::from(base).join("Image-Line"));
        }
    }
    for fallback in [
        r"C:\Program Files\Image-Line",
        r"C:\Program Files (x86)\Image-Line",
        r"D:\Program Files\Image-Line",
    ] {
        let p = PathBuf::from(fallback);
        if !roots.contains(&p) {
            roots.push(p);
        }
    }
    roots
}

/// Find the newest native Windows `FL Studio *` installation directory.
pub fn fl_install_dir() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut installs: Vec<PathBuf> = fl_image_line_roots(&home)
        .into_iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().starts_with("fl studio"))
        })
        .collect();
    // Prefer newest FL Studio version (e.g. FL Studio 2025 over FL Studio 21).
    installs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    installs.into_iter().next()
}

pub fn fl_user_data_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|d| d.join("Image-Line"))
}

/// Default sound library roots on Windows.
pub fn default_mounts(home: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    v.extend(dirs::audio_dir());
    v.push(home.join("Music"));
    if let Some(docs) = dirs::document_dir() {
        v.push(docs.join("Image-Line"));
    }
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_theme_value_maps_zero_to_dark() {
        assert_eq!(parse_apps_use_light_theme(0), Theme::Dark);
        assert_eq!(parse_apps_use_light_theme(1), Theme::Light);
    }

    #[test]
    fn windows_locale_query_returns_non_empty_tag() {
        let loc = system_locale();
        assert!(
            loc.as_deref().is_some_and(|s| s.len() >= 2),
            "expected Windows locale tag, got {loc:?}"
        );
    }
}
