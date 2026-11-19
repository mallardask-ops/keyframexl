//! VST3 plugins (Windows ones through yabridge, native Linux ones directly).
//!
//! * **Scan**: every bundle under the VST3 folders is probed in a child process
//!   (`keyframexl plugin-probe <path>`): it loads the plugin, plays a note and
//!   reports what it is. A plugin that crashes or hangs is marked as such and
//!   never loaded into the program. Results are cached by path and file time.
//! * **Host thread**: one thread owns every loaded plugin (vst3-host only lets
//!   the loading thread use a plugin's controller): it loads them, keeps their
//!   editor windows (X11, through XWayland on Wayland) and runs their event
//!   loop at 60 Hz. The UI talks to it over channels and never waits on a plugin.
//! * **Audio**: each instance reaches the engine as a [`Processor`] that locks
//!   the plugin for one block.

use crate::vst2::Vst2Plugin;
use keyframexl::plugin::{PluginEvent, Processor, Shared, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use vst3_host::{
    Plugin, Vst3Host, audio::AudioBuffers, audio::BusAudioBuffers, midi::MidiChannel,
    midi::MidiEvent,
};

// ------------------------------------------------------------------ wine

/// The Wine `bin` folder plugin processes run Windows plugins with (yabridge
/// finds `wine` through PATH). Set from the settings; None is the system Wine.
static WINE_BIN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The Wine `bin` folder plugins run with; `None` = the system Wine.
pub fn wine_bin() -> Option<PathBuf> {
    WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_wine(bin: Option<PathBuf>) {
    let bin = bin.filter(|b| b.join("wine").exists());
    *WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()) = bin.clone();
    // Get the prefix ready now, so the first plugin does not wait for it.
    if let Some(bin) = bin {
        let _ = std::thread::Builder::new()
            .name("wine-prefix".into())
            .spawn(move || prepare_prefix(&bin));
    }
}

/// The prefix plugin processes use: $WINEPREFIX, else ~/.wine.
pub fn wine_prefix() -> Option<PathBuf> {
    std::env::var_os("WINEPREFIX")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".wine")))
}

/// The wineserver the chosen Wine would run (system: the one on PATH).
fn own_wineserver() -> Option<PathBuf> {
    let bin = WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = match bin {
        Some(b) => b.join("wineserver"),
        None => std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join("wineserver"))
            .find(|p| p.is_file())?,
    };
    std::fs::canonicalize(&server).ok()
}

/// A wineserver from another Wine build holding our prefix: (pid, its program).
/// Two Wine versions cannot share a prefix at once; while it runs, every
/// plugin started with the chosen Wine fails ("Failed to start the Wine
/// plugin host"), so the settings page offers to stop it.
pub fn foreign_wineserver() -> Option<(u32, PathBuf)> {
    let ours = own_wineserver();
    wineservers_on_prefix()
        .into_iter()
        .find(|(_, exe)| ours.as_ref() != Some(exe))
}

/// Every wineserver holding our prefix: (pid, its program).
fn wineservers_on_prefix() -> Vec<(u32, PathBuf)> {
    match wine_prefix() {
        Some(p) => wineservers_on(&p),
        None => Vec::new(),
    }
}

/// Wineservers serving `prefix`.
fn wineservers_on(prefix: &Path) -> Vec<(u32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(prefix) = std::fs::canonicalize(prefix) else {
        return out;
    };
    let home = dirs::home_dir();
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return out;
    };
    for e in procs.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(exe) = std::fs::read_link(e.path().join("exe")) else {
            continue;
        };
        if exe.file_name().is_none_or(|n| n != "wineserver") {
            continue;
        }
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        // Its prefix: WINEPREFIX from its environment, else ~/.wine.
        let env = std::fs::read(e.path().join("environ")).unwrap_or_default();
        let theirs = env
            .split(|b| *b == 0)
            .find_map(|kv| {
                kv.strip_prefix(b"WINEPREFIX=")
                    .map(|v| PathBuf::from(String::from_utf8_lossy(v).to_string()))
            })
            .or_else(|| home.as_ref().map(|h| h.join(".wine")));
        let same_prefix = theirs
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|p| p == prefix);
        if same_prefix {
            out.push((pid, exe));
        }
    }
    out
}

/// Proton's Wine always runs as "steamuser", so plugins under it look for
/// their settings, licenses and library paths in users/steamuser, not in the
/// user's own profile (SSD5: "Cannot find your instruments"). Make
/// users/steamuser a link to the user's profile, keeping what was there as a
/// backup folder. Only while no Wine runs on the prefix.
fn link_steamuser(prefix: &Path) {
    let users = prefix.join("drive_c/users");
    let Some(me) = std::env::var_os("USER").filter(|u| !u.is_empty() && u != "steamuser") else {
        return;
    };
    let mine = users.join(&me);
    let steam = users.join("steamuser");
    if !mine.is_dir() || std::fs::symlink_metadata(&steam).is_ok_and(|m| m.file_type().is_symlink())
    {
        return;
    }
    if !wineservers_on(prefix).is_empty() {
        static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !TOLD.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "[wine] Wine runs on {}; steamuser is linked on a later start",
                prefix.display()
            );
        }
        return;
    }
    if steam.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = users.join(format!("steamuser.before-keyframexl-{stamp}"));
        if let Err(e) = std::fs::rename(&steam, &backup) {
            eprintln!("[wine] cannot move {}: {e}", steam.display());
            return;
        }
        eprintln!("[wine] kept the old Proton profile as {}", backup.display());
    }
    match std::os::unix::fs::symlink(&me, &steam) {
        Ok(()) => eprintln!(
            "[wine] {} now points at {}",
            steam.display(),
            mine.display()
        ),
        Err(e) => eprintln!("[wine] cannot link steamuser: {e}"),
    }
}

/// Stop that wineserver the way its own Wine does (`wineserver -k`): every
/// Windows program on the prefix closes. Then wait for it to go.
pub fn stop_foreign_wineserver(pid: u32, exe: &Path) -> Result<(), String> {
    let prefix = wine_prefix().ok_or("No Wine prefix")?;
    std::process::Command::new(exe)
        .arg("-k")
        .env("WINEPREFIX", &prefix)
        .status()
        .map_err(|e| e.to_string())?;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while Path::new(&format!("/proc/{pid}")).exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if Path::new(&format!("/proc/{pid}")).exists() {
        Err("It is still running".into())
    } else {
        Ok(())
    }
}

/// One preparation at a time; plugin processes wait for it (`apply_wine`).
static PREFIX_PREP: Mutex<()> = Mutex::new(());

/// A Proton build finds its vkd3d DLLs only through placeholders in the
/// prefix's system32, which `wineboot` writes when WINEDLLPATH names them.
/// A prefix that this build updated without them (started bare, as yabridge
/// does) lacks them, and wined3d → dxgi → the plugin fail to load. Update the
/// prefix once with the right paths, as Proton's own launcher would.
fn prepare_prefix(bin: &Path) {
    let _guard = PREFIX_PREP.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dlls) = wine_dll_path(bin) else {
        return;
    };
    let Some(prefix) = wine_prefix() else { return };
    if let Some((pid, exe)) = foreign_wineserver() {
        // wineboot would fail the same way; the settings page offers to stop it.
        eprintln!(
            "[wine] {} holds {} (pid {pid}); not updating the prefix",
            exe.display(),
            prefix.display()
        );
        return;
    }
    link_steamuser(&prefix);
    update_prefix(bin, &prefix, dlls);
    if DXVK.load(std::sync::atomic::Ordering::Relaxed) && prefix.join("system.reg").exists() {
        install_dxvk(bin, &prefix);
    }
}

/// `wineboot -u` with the vkd3d paths when the placeholders are missing.
fn update_prefix(bin: &Path, prefix: &Path, dlls: std::ffi::OsString) {
    // No prefix yet: Wine creates it with these paths on first use.
    if !prefix.join("system.reg").exists()
        || prefix
            .join("drive_c/windows/system32/libvkd3d-1.dll")
            .exists()
    {
        return;
    }
    eprintln!(
        "[wine] updating {} for {} (vkd3d placeholders)",
        prefix.display(),
        bin.display()
    );
    let mut path = vec![bin.to_path_buf()];
    if let Some(p) = std::env::var_os("PATH") {
        path.extend(std::env::split_paths(&p));
    }
    let mut c = std::process::Command::new(bin.join("wine"));
    c.args(["wineboot", "-u"])
        .env("WINEPREFIX", prefix)
        .env("WINEDLLPATH", dlls)
        .env("WINEDEBUG", "-all")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Ok(joined) = std::env::join_paths(path) {
        c.env("PATH", joined);
    }
    match c.spawn() {
        Ok(mut child) => {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(180);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < until => {
                        std::thread::sleep(std::time::Duration::from_millis(200))
                    }
                    _ => {
                        let _ = child.kill();
                        break;
                    }
                }
            }
            let ok = prefix
                .join("drive_c/windows/system32/libvkd3d-1.dll")
                .exists();
            eprintln!(
                "[wine] prefix update {}",
                if ok { "done" } else { "did not add vkd3d" }
            );
        }
        Err(e) => eprintln!("[wine] cannot run wineboot: {e}"),
    }
}

/// Put the chosen Wine first in a plugin process's PATH.
pub fn apply_wine(c: &mut std::process::Command) {
    let Some(bin) = WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    prepare_prefix(&bin);
    let mut path = vec![bin];
    if let Some(p) = std::env::var_os("PATH") {
        path.extend(std::env::split_paths(&p));
    }
    if let Ok(joined) = std::env::join_paths(path) {
        c.env("PATH", joined);
    }
    if let Some(dlls) = wine_dll_path(&bin_dir(c)) {
        c.env("WINEDLLPATH", dlls);
    }
    if DXVK.load(std::sync::atomic::Ordering::Relaxed) && has_dxvk(&bin_dir(c)) {
        c.env("WINEDLLOVERRIDES", dxvk_overrides());
    }
}

/// Whether a Windows plugin is 32-bit: the DLL yabridge loads for a `.so`
/// (beside it, usually a link into the prefix), read from its PE header.
pub fn is_32bit_windows(path: &Path) -> bool {
    let dll = path.with_extension("dll");
    let Ok(d) = std::fs::read(&dll).or_else(|_| std::fs::read(path)) else {
        return false;
    };
    let pe = d
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    match pe.and_then(|o| d.get(o..o + 6)) {
        Some(h) if &h[..4] == b"PE\0\0" => u16::from_le_bytes([h[4], h[5]]) == 0x14c,
        _ => false,
    }
}

/// A Wine build that still runs 32-bit Windows programs the old way
/// (yabridge's 32-bit host is a 32-bit winelib program): Proton's.
fn wine32_bin() -> Option<PathBuf> {
    let builds = wine_builds();
    let pick = |w: &str| {
        builds
            .iter()
            .filter_map(|(n, b)| b.clone().filter(|_| n.contains(w)))
            .next()
    };
    pick("Proton-CachyOS")
        .or_else(|| pick("Proton"))
        .or_else(|| pick("GE"))
}

/// The prefix 32-bit plugins run in when the rest use the system Wine: two
/// Wine versions cannot share one prefix. A copy of the plugin prefix
/// (reflinked where the filesystem can, so it costs no space), made once.
fn wine32_prefix() -> Option<PathBuf> {
    static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
    let mut ready = READY.lock().unwrap_or_else(|e| e.into_inner());
    if ready.is_some() {
        return ready.clone();
    }
    let dir = dirs::data_dir()?.join("keyframexl/wine32-prefix");
    if !dir.join("system.reg").exists() {
        let src = wine_prefix()?;
        let _ = std::fs::create_dir_all(dir.parent()?);
        let _ = std::fs::remove_dir_all(&dir);
        let ok = std::process::Command::new("cp")
            .args(["-a", "--reflink=always"])
            .arg(&src)
            .arg(&dir)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            // No reflinks here: a fresh prefix with the plugin prefix's
            // registry (plugins load from their own folders by path).
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).ok()?;
            for f in ["system.reg", "user.reg", "userdef.reg"] {
                let _ = std::fs::copy(src.join(f), dir.join(f));
            }
        }
        eprintln!(
            "[wine] 32-bit plugin prefix at {} ({})",
            dir.display(),
            if ok { "reflinked copy" } else { "fresh" }
        );
    }
    *ready = Some(dir.clone());
    Some(dir)
}

/// `apply_wine` for one plugin: a 32-bit Windows plugin under the system
/// Wine (whose new WoW64 cannot start yabridge's 32-bit host: c000007b) runs
/// with Proton in its own prefix instead.
pub fn apply_wine_for(c: &mut std::process::Command, plugin: &Path) {
    if wine_bin().is_none()
        && is_32bit_windows(plugin)
        && let Some(bin) = wine32_bin()
        && let Some(prefix) = wine32_prefix()
    {
        let mut path = vec![bin.clone()];
        if let Some(p) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(path) {
            c.env("PATH", joined);
        }
        if let Some(dlls) = wine_dll_path(&bin) {
            {
                let _guard = PREFIX_PREP.lock().unwrap_or_else(|e| e.into_inner());
                update_prefix(&bin, &prefix, dlls.clone());
            }
            c.env("WINEDLLPATH", dlls);
        }
        c.env("WINEPREFIX", &prefix);
        return;
    }
    apply_wine(c);
}

/// Use the build's DXVK for Direct3D in plugin processes (Settings → Plugins).
static DXVK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_dxvk(on: bool) {
    DXVK.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a Wine bin folder comes with DXVK (Proton builds do).
pub fn has_dxvk(bin: &Path) -> bool {
    dxvk_dir(bin).is_some()
}

fn dxvk_dir(bin: &Path) -> Option<PathBuf> {
    let root = bin.parent()?;
    ["lib/wine/dxvk", "lib64/wine/dxvk"]
        .iter()
        .map(|d| root.join(d))
        .find(|d| d.join("x86_64-windows/dxgi.dll").is_file())
}

/// JUCE 8 plugins (SSD5 and others) draw with Direct2D and repaint on
/// `IDXGIOutput::WaitForVBlank`, a stub in Wine's dxgi: they draw once and
/// then never again, so clicks seem to do nothing. DXVK implements it.
/// `install_dxvk` puts its DLLs in the prefix (as Proton does); only plugin
/// processes prefer them, through this override. Without it Wine keeps using
/// its own (checked: the system32 copies load as builtin).
fn dxvk_overrides() -> std::ffi::OsString {
    let mut overrides = "dxgi,d3d11,d3d10core=n,b".to_string();
    if let Some(old) = std::env::var_os("WINEDLLOVERRIDES").filter(|v| !v.is_empty()) {
        overrides = format!("{};{overrides}", old.to_string_lossy());
    }
    overrides.into()
}

/// Copy the build's DXVK DLLs into the prefix (system32: 64-bit, syswow64:
/// 32-bit) when they differ. Each file is replaced by a rename, so a process
/// that has the old one mapped keeps working. A prefix update by wineboot
/// puts Wine's placeholders back; the next plugin start copies again.
fn install_dxvk(bin: &Path, prefix: &Path) {
    let Some(dir) = dxvk_dir(bin) else { return };
    for (arch, sys) in [("x86_64-windows", "system32"), ("i386-windows", "syswow64")] {
        let to_dir = prefix.join("drive_c/windows").join(sys);
        if !to_dir.is_dir() {
            continue;
        }
        for name in ["dxgi.dll", "d3d11.dll", "d3d10core.dll"] {
            let from = dir.join(arch).join(name);
            let to = to_dir.join(name);
            let Ok(new) = std::fs::read(&from) else {
                continue;
            };
            if std::fs::read(&to).is_ok_and(|old| old == new) {
                continue;
            }
            let tmp = to_dir.join(format!(".{name}.keyframexl"));
            match std::fs::write(&tmp, &new).and_then(|_| std::fs::rename(&tmp, &to)) {
                Ok(()) => eprintln!("[wine] DXVK {name} → {}", to.display()),
                Err(e) => eprintln!("[wine] cannot install DXVK {name}: {e}"),
            }
        }
    }
}

/// The chosen bin folder (first PATH entry `apply_wine` set).
fn bin_dir(c: &std::process::Command) -> PathBuf {
    c.get_envs()
        .find(|(k, _)| *k == "PATH")
        .and_then(|(_, v)| v)
        .and_then(|v| std::env::split_paths(v).next())
        .unwrap_or_default()
}

/// Proton keeps vkd3d apart (`files/lib/vkd3d`), and its launcher puts it on
/// WINEDLLPATH; started bare, wined3d → dxgi fail and plugins such as SSD5
/// do not load. Same folders as Proton's own launcher, plus any WINEDLLPATH set.
fn wine_dll_path(bin: &Path) -> Option<std::ffi::OsString> {
    let root = bin.parent()?;
    let lib = ["lib", "lib64"]
        .iter()
        .map(|l| root.join(l))
        .find(|l| l.join("vkd3d").is_dir())?;
    let mut dirs = vec![lib.join("vkd3d"), lib.join("wine")];
    if let Some(p) = std::env::var_os("WINEDLLPATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(dirs).ok()
}

/// Wine builds on this computer: (label, bin folder). The system one first
/// (bin None). Looks in /opt, PortProton, Steam compatibility tools and Lutris.
pub fn wine_builds() -> Vec<(String, Option<PathBuf>)> {
    let mut out = vec![("System Wine".to_string(), None)];
    // ~/.steam/root links to the same Steam folder: compare real paths.
    let mut seen = std::collections::HashSet::new();
    let mut add = |label: String, bin: PathBuf| {
        if bin.join("wine").is_file()
            && seen.insert(std::fs::canonicalize(&bin).unwrap_or_else(|_| bin.clone()))
        {
            out.push((label, Some(bin)));
        }
    };
    let list = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d)
            .map(|r| r.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        v.sort();
        v
    };
    for d in list(Path::new("/opt")).into_iter().filter(|d| {
        d.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("wine"))
    }) {
        add(
            d.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            d.join("bin"),
        );
    }
    if let Some(h) = dirs::home_dir() {
        for (root, kind) in [
            (h.join("PortProton/data/dist"), "PortProton"),
            (h.join(".local/share/Steam/compatibilitytools.d"), "Steam"),
            (h.join(".steam/root/compatibilitytools.d"), "Steam"),
            (h.join(".local/share/lutris/runners/wine"), "Lutris"),
        ] {
            for d in list(&root) {
                let name = d
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                for bin in [d.join("bin"), d.join("files/bin"), d.join("dist/bin")] {
                    add(format!("{name} ({kind})"), bin);
                }
            }
        }
    }
    out
}

/// "vst2" for a Linux VST2 library (`.so`, e.g. a yabridge shim), else "vst3".
pub fn format_of(path: &Path) -> &'static str {
    if crate::fl_link::is_link(path) {
        return "fl";
    }
    if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("so"))
    {
        "vst2"
    } else {
        "vst3"
    }
}

/// Largest block handed to a plugin at once (bigger engine blocks are split).
pub const MAX_BLOCK: usize = 2048;

// ------------------------------------------------------------------ scan

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Ok,
    /// Loaded but a test block never came back.
    Hangs,
    /// The probe process died.
    Crashes,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scanned {
    pub path: PathBuf,
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub instrument: bool,
    pub gui: bool,
    pub status: Status,
    /// Modification time of the bundle when probed (seconds), for the cache.
    pub stamp: u64,
    /// Bridged Windows plugin (yabridge).
    #[serde(default)]
    pub bridged: bool,
}

impl Scanned {
    pub fn usable(&self) -> bool {
        self.status == Status::Ok
    }
}

/// Standard VST3 folders (Linux) plus `$VST3_PATH` and the user's extra folders.
/// Only loadable bundles are scanned: native Linux ones and yabridge shims
/// under `~/.vst3` (e.g. `~/.vst3/yabridge/SSD5.vst3` after `yabridgectl sync`).
/// Raw Windows bundles inside the Wine prefix are intentionally skipped: they
/// cannot be loaded directly and would only show up as failing duplicates.
pub fn search_dirs(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".vst3"));
    }
    v.push("/usr/lib/vst3".into());
    v.push("/usr/local/lib/vst3".into());
    v.push("/opt/vst3".into());
    if let Ok(p) = std::env::var("VST3_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    v.extend(extra.iter().cloned());
    v.dedup();
    v
}

/// `.vst3` bundles under `dirs` (a bundle is not searched inside).
/// Depth 6 covers `~/.vst3/yabridge/<group>/Name.vst3`; symlinks are followed
/// so yabridge links resolve. Both directory bundles and single-file `.vst3`
/// modules (some Windows installers) are collected.
pub fn bundles(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = dirs.iter().map(|d| (d.clone(), 0)).collect();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    while let Some((d, depth)) = stack.pop() {
        let canon = std::fs::canonicalize(&d).unwrap_or_else(|_| d.clone());
        if !seen.insert(canon) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("vst3"))
            {
                out.push(p);
            } else if depth < 6 {
                // `is_dir` follows symlinks; a symlink loop is stopped by `seen`.
                if p.is_dir() {
                    stack.push((p, depth + 1));
                } else if p.is_symlink() {
                    // A symlinked bundle (yabridge): resolve one level.
                    if let Ok(target) = std::fs::read_link(&p) {
                        let target = if target.is_absolute() {
                            target
                        } else {
                            d.join(target)
                        };
                        if target
                            .extension()
                            .is_some_and(|x| x.eq_ignore_ascii_case("vst3"))
                        {
                            out.push(target);
                        } else if target.is_dir() {
                            stack.push((target, depth + 1));
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn stamp(p: &Path) -> u64 {
    // A yabridge bundle changes on either side: the native shim or the Windows dll.
    let mut latest: Option<std::time::SystemTime> = None;
    for sub in ["Contents/x86_64-linux", "Contents/x86_64-win"] {
        let dir = p.join(sub);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                // Recurse one level: some bundles keep the module in a subfolder.
                let ep = e.path();
                let candidates = if ep.is_dir() {
                    std::fs::read_dir(&ep)
                        .map(|rd| rd.flatten().map(|x| x.path()).collect::<Vec<_>>())
                        .unwrap_or_else(|_| vec![ep.clone()])
                } else {
                    vec![ep.clone()]
                };
                for c in candidates {
                    if let Ok(m) = std::fs::metadata(&c).and_then(|m| m.modified()) {
                        latest = Some(latest.map(|l| l.max(m)).unwrap_or(m));
                    }
                }
            }
        }
    }
    latest
        .or_else(|| std::fs::metadata(p).ok()?.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("keyframexl").join("plugins.ron"))
}

pub fn load_cache() -> Vec<Scanned> {
    cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| ron::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(list: &[Scanned]) {
    if let Some(p) = cache_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
        if let Ok(text) = ron::ser::to_string_pretty(list, Default::default()) {
            let _ = std::fs::write(p, text);
        }
    }
}

/// Probe one bundle now and store the verdict in the cache (Settings → Rescan).
/// Returns the fresh entry so the UI can show it without a full rescan.
pub fn rescan_one(path: &Path) -> Scanned {
    let st = stamp(path);
    let s = probe_in_child(path, st);
    let mut cache = load_cache();
    if let Some(slot) = cache.iter_mut().find(|c| c.path == s.path) {
        *slot = s.clone();
    } else {
        cache.push(s.clone());
    }
    cache.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    save_cache(&cache);
    s
}

/// Probe what changed since the cache (or everything with `force`); three at once.
/// `progress` gets (done, total, name) as probes finish.
/// VST2 folders: `~/.vst` (yabridge puts its shims in `~/.vst/yabridge`),
/// the system ones and `$VST_PATH`.
pub fn vst2_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".vst"));
    }
    v.push("/usr/lib/vst".into());
    v.push("/usr/local/lib/vst".into());
    if let Some(p) = std::env::var_os("VST_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    v
}

/// VST2 libraries (`.so`) under `dirs`, following symlinks, depth 6.
pub fn vst2_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = dirs.iter().map(|d| (d.clone(), 0)).collect();
    let mut seen = std::collections::HashSet::new();
    while let Some((d, depth)) = stack.pop() {
        if !seen.insert(std::fs::canonicalize(&d).unwrap_or_else(|_| d.clone())) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 6 {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("so")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn scan(extra: &[PathBuf], force: bool, progress: impl Fn(usize, usize, &str)) -> Vec<Scanned> {
    let cache = if force { vec![] } else { load_cache() };
    let mut found = bundles(&search_dirs(extra));
    found.extend(vst2_files(&vst2_dirs()));
    let mut out: Vec<Scanned> = Vec::new();
    let mut todo = Vec::new();
    for p in found {
        let st = stamp(&p);
        match cache.iter().find(|c| c.path == p && c.stamp == st) {
            Some(c) => out.push(c.clone()),
            None => todo.push((p, st)),
        }
    }
    let total = todo.len();
    let done = Arc::new(Mutex::new(Vec::new()));
    for chunk in todo.chunks(3) {
        let handles: Vec<_> = chunk
            .iter()
            .cloned()
            .map(|(p, st)| {
                let done = done.clone();
                std::thread::spawn(move || {
                    let s = probe_in_child(&p, st);
                    done.lock().unwrap().push(s);
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let d = done.lock().unwrap();
        if let Some(last) = d.last() {
            progress(d.len(), total, &last.name);
        }
    }
    out.extend(done.lock().unwrap().drain(..));
    out.retain(|s| !crate::fl_link::is_link(&s.path));
    if let Some(link) = fl_link_entry() {
        out.push(link);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    save_cache(&out);
    out
}

/// The FL Studio live link, offered as an instrument when FL is installed.
fn fl_link_entry() -> Option<Scanned> {
    crate::fl_link::find_fl()?;
    Some(Scanned {
        path: PathBuf::from(crate::fl_link::PATH),
        name: "FL Studio (live link)".into(),
        vendor: "Image-Line".into(),
        category: "Instrument".into(),
        instrument: true,
        gui: false,
        status: Status::Ok,
        stamp: 0,
        bridged: true,
    })
}

/// What the probe child prints (one JSON line).
#[derive(Serialize, Deserialize)]
struct ProbeOut {
    name: String,
    vendor: String,
    category: String,
    instrument: bool,
    gui: bool,
}

fn probe_in_child(path: &Path, st: u64) -> Scanned {
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let bridged = path.join("Contents/x86_64-win").exists()
        || (format_of(path) == "vst2" && path.with_extension("dll").exists());
    let mut s = Scanned {
        path: path.to_path_buf(),
        name,
        vendor: String::new(),
        category: String::new(),
        instrument: false,
        gui: false,
        status: Status::Crashes,
        stamp: st,
        bridged,
    };
    let Ok(exe) = std::env::current_exe() else {
        s.status = Status::Failed("Cannot find the program to probe with".into());
        return s;
    };
    let mut command = std::process::Command::new(exe);
    apply_wine_for(&mut command, path);
    let child = command
        .arg("plugin-probe")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            s.status = Status::Failed(format!("Cannot start the probe: {e}"));
            return s;
        }
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            // Large samplers (SSD5 etc.) under Wine/yabridge can take a while
            // on first start; 90 s still keeps a rescan interactive.
            Ok(None) if started.elapsed() > Duration::from_secs(90) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_string(&mut text);
    }
    let mut lines = text.lines().filter(|l| l.starts_with("KFXL "));
    if let Some(info) = lines
        .next()
        .and_then(|l| serde_json::from_str::<ProbeOut>(&l[5..]).ok())
    {
        s.name = info.name;
        s.vendor = info.vendor;
        s.category = info.category;
        s.instrument = info.instrument;
        s.gui = info.gui;
    }
    let verdict = lines.next().map(|l| l[5..].to_string());
    s.status = match (status, verdict.as_deref()) {
        (_, Some("ok")) => Status::Ok,
        (_, Some(e)) if e.starts_with("error ") => Status::Failed(e[6..].to_string()),
        (None, _) => Status::Hangs,
        (Some(_), _) if text.contains("KFXL {") => Status::Crashes,
        (Some(_), _) => Status::Failed("Could not load (no probe answer)".into()),
    };
    // A Windows bundle without its Wine side looks like a missing file: say so.
    if s.status != Status::Ok
        && s.bridged
        && format_of(path) == "vst3"
        && !path.join("Contents/x86_64-linux").exists()
    {
        s.status = Status::Failed("yabridge shim missing (run yabridgectl sync)".into());
    }
    s
}

/// Child side of the scan: load, describe, play a note for a moment.
pub fn probe_main(path: &Path) -> i32 {
    let say = |s: String| {
        use std::io::Write;
        let mut o = std::io::stdout();
        let _ = writeln!(o, "KFXL {s}");
        let _ = o.flush();
    };
    if format_of(path) == "vst2" {
        let mut p = match Vst2Plugin::load(path) {
            Ok(p) => p,
            Err(e) => {
                say(format!("error {e}"));
                return 1;
            }
        };
        let (instrument, ni, no, _) = p.info();
        let name = Some(p.name())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let out = ProbeOut {
            name,
            vendor: p.vendor(),
            category: if instrument {
                "Instrument".into()
            } else {
                "Fx".into()
            },
            instrument,
            gui: p.has_editor(),
        };
        say(serde_json::to_string(&out).unwrap_or_default());
        p.configure(48_000.0, 512);
        p.start();
        let mut ins = vec![vec![0.0f32; 512]; ni.min(2)];
        let mut outs = vec![vec![0.0f32; 512]; no.min(2)];
        let (mut silence, mut scratch) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        for i in 0..40 {
            if i == 2 && instrument {
                p.queue_midi([0x90, 60, 90], 0);
            }
            if i == 30 && instrument {
                p.queue_midi([0x80, 60, 0], 0);
            }
            p.flush_midi();
            p.process_into(&mut ins, &mut outs, &mut silence, &mut scratch, 512);
        }
        p.stop();
        say("ok".into());
        std::process::exit(0);
    }
    let mut host = match Vst3Host::builder()
        .sample_rate(48_000.0)
        .block_size(512)
        .build()
    {
        Ok(h) => h,
        Err(e) => {
            say(format!("error {e}"));
            return 1;
        }
    };
    let mut plugin = match host.load_plugin(path) {
        Ok(p) => p,
        Err(e) => {
            say(format!("error {e}"));
            return 1;
        }
    };
    let info = plugin.info().clone();
    let instrument = info.category.contains("Instrument")
        || info.has_midi_input && !info.category.contains("Fx");
    let out = ProbeOut {
        name: info.name.clone(),
        vendor: info.vendor.clone(),
        category: info.category.clone(),
        instrument,
        gui: plugin.has_editor(),
    };
    say(serde_json::to_string(&out).unwrap_or_default());
    if let Err(e) = plugin.start_processing() {
        say(format!("error {e}"));
        return 1;
    }
    let mut buf = AudioBuffers::new(if instrument { 0 } else { 2 }, 2, 512, 48_000.0);
    for i in 0..40 {
        if i == 2 && instrument {
            let _ = plugin.send_midi_event_at(
                MidiEvent::NoteOn {
                    channel: MidiChannel::Ch1,
                    note: 60,
                    velocity: 90,
                },
                0,
            );
        }
        if i == 30 && instrument {
            let _ = plugin.send_midi_note_off(60, MidiChannel::Ch1);
        }
        if let Err(e) = plugin.process_audio(&mut buf) {
            say(format!("error {e}"));
            return 1;
        }
    }
    let _ = plugin.stop_processing();
    say("ok".into());
    // Leave without running the plugin's teardown: some bridged plugins hang there.
    std::process::exit(0);
}

// ------------------------------------------------------------- processing

/// The audio thread's side of one loaded VST3 plugin, bus by bus: input bus
/// 0 is the effect's signal, input bus 1 its sidechain; output bus 0 is the
/// main pair, the aux output buses (a multi-out instrument's kick, snare ...)
/// become extra pairs for the mixer.
struct Vst3Proc {
    plugin: Arc<Mutex<Plugin>>,
    buf: BusAudioBuffers,
    effect: bool,
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
        // While the control side holds the plugin (an editor opening under
        // Wine, a state save) the block plays silence instead of waiting: a
        // bridged plugin's process would otherwise stall, be taken for hung
        // and closed (the same as `Vst2Proc`).
        let Ok(mut plugin) = self.plugin.try_lock() else {
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
            for (i, f) in src.chunks_exact(2).enumerate() {
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
    for (i, f) in dst.chunks_exact_mut(2).enumerate() {
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

impl Processor for Vst3Proc {
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
/// stereo bus; a multi-out instrument (SSD5: 48 outputs) gives the rest as
/// extra pairs for the mixer.
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
        let pairs = if stride == 0 {
            0
        } else {
            self.extra.min(extra.len() / stride)
        };
        // Opening the editor can take seconds under Wine while it holds the
        // plugin: the block plays silence instead of stalling the audio (the
        // bridge would take a stall for a hang and close the plugin).
        let Ok(mut plugin) = self.plugin.try_lock() else {
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
                for (i, f) in io[done * 2..(done + n) * 2].chunks_exact(2).enumerate() {
                    self.inputs[0][i] = f[0];
                    self.inputs[1][i] = f[1];
                }
            }
            if self.inputs.len() == 4 {
                match key {
                    Some(key) => {
                        for (i, f) in key[done * 2..(done + n) * 2].chunks_exact(2).enumerate() {
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
                for (i, f) in io[done * 2..(done + n) * 2].chunks_exact_mut(2).enumerate() {
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
                    for (i, f) in lane.chunks_exact_mut(2).enumerate() {
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

/// A loaded plugin of either format.
#[derive(Clone)]
enum Handle {
    V3(Arc<Mutex<Plugin>>),
    V2(Arc<Mutex<Vst2Plugin>>),
    /// FL Studio live link: no editor, parameters or state of its own.
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
            Handle::Fl => Ok(()),
        }
    }

    fn close_editor(&self) {
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
            Handle::Fl => {}
        }
    }

    /// Editor upkeep (run loop / idle); a size the editor asked for.
    fn service(&self) -> Option<(i32, i32)> {
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
            Handle::Fl => None,
        }
    }

    fn embed(
        &self,
        conn: &xcb::Connection,
        parent: u32,
        rect: [i32; 4],
        clip: [i32; 4],
    ) -> Result<(xcb::x::Window, xcb::x::Window, i32, i32), String> {
        match self {
            Handle::V3(p) => {
                let mut p = p.lock().map_err(|_| "busy".to_string())?;
                let size = p.get_editor_size().ok();
                embed(conn, size, parent, rect, clip, |win| {
                    p.open_editor(vst3_host::plugin::WindowHandle::from_x11(win))
                        .map_err(|e| e.to_string())
                })
            }
            Handle::V2(p) => {
                let mut p = p.lock().map_err(|_| "busy".to_string())?;
                // Size after opening: some plugins (Odyssey under yabridge) never
                // answer effEditGetRect before their editor exists.
                let (frame, win, w, h) = embed(conn, None, parent, rect, clip, |win| {
                    if p.editor_open(win as usize) {
                        Ok(())
                    } else {
                        Err("the plugin did not open its editor".into())
                    }
                })?;
                let size = p
                    .editor_rect()
                    .map(|r| ((r.right - r.left) as i32, (r.bottom - r.top) as i32))
                    .filter(|s| s.0 > 0 && s.1 > 0);
                match size {
                    Some((nw, nh)) if (nw, nh) != (w, h) => {
                        conn.send_request(&xcb::x::ConfigureWindow {
                            window: win,
                            value_list: &[
                                xcb::x::ConfigWindow::Width(nw as u32),
                                xcb::x::ConfigWindow::Height(nh as u32),
                            ],
                        });
                        let _ = conn.flush();
                        Ok((frame, win, nw, nh))
                    }
                    _ => Ok((frame, win, w, h)),
                }
            }
            Handle::Fl => {
                Err("The FL Studio link has no editor here: FL Studio shows its own window".into())
            }
        }
    }
}

fn v2_restore(p: &mut Vst2Plugin, state: &[u8]) -> Result<(), String> {
    if let Some(chunk) = state.strip_prefix(V2_CHUNK) {
        // The answer to effSetChunk means nothing (JUCE plugins always say 0
        // and take the state): only a plugin without chunks cannot take it.
        if !p.has_chunk() {
            return Err("the plugin does not take saved state".into());
        }
        p.set_chunk(false, chunk);
    } else if let Some(values) = state.strip_prefix(V2_PARAMS) {
        for (i, v) in values.chunks_exact(4).enumerate() {
            p.set_parameter(i as i32, f32::from_le_bytes([v[0], v[1], v[2], v[3]]));
        }
    } else if !state.is_empty() {
        return Err("not a VST2 state".into());
    }
    Ok(())
}

/// Load either format, restore `state`, start processing. (handle, name, has editor, audio side)
fn load_any(
    host: &mut Vst3Host,
    path: &Path,
    state: &[u8],
    rate: u32,
    effect: bool,
) -> Result<(Handle, String, bool, Shared), String> {
    if crate::fl_link::is_link(path) {
        let link = crate::fl_link::FlLink::new(rate)?;
        return Ok((
            Handle::Fl,
            "FL Studio (live link)".into(),
            false,
            keyframexl::plugin::share(link),
        ));
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
    /// Put the editor inside our own X11 window `parent` (FL's wrapper window):
    /// `rect` = x, y, width, height of the whole editor in window pixels, `clip`
    /// = the part of it that shows (the rest is cut off, not moved). Hidden
    /// while `visible` is off.
    Embed {
        id: u64,
        parent: u32,
        rect: [i32; 4],
        clip: [i32; 4],
        visible: bool,
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
    /// Unload everything and end the thread (answered by `Reply::Bye`): plugins
    /// (yabridge above all) must be torn down before the process exits.
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

/// An editor living inside our window as a child X11 window.
struct Embedded {
    /// Shows only `clip` of the editor; `win` sits inside it, offset.
    frame: xcb::x::Window,
    /// The plugin's parent window, the editor's full size.
    win: xcb::x::Window,
    rect: [i32; 4],
    clip: [i32; 4],
    visible: bool,
    /// Screen position last told to the editor (`announce`).
    told: Option<(i16, i16)>,
    /// Plugin windows already marked as shown (`mark_shown`).
    marked: Vec<u32>,
}

struct Loaded {
    plugin: Handle,
    window: Option<vst3_host::window::PluginWindow>,
    embed: Option<Embedded>,
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

/// Create the clip frame under `parent`, the editor area inside it, and attach
/// the editor. Returns (frame, area, editor width, height).
fn embed(
    conn: &xcb::Connection,
    size: Option<(i32, i32)>,
    parent: u32,
    rect: [i32; 4],
    clip: [i32; 4],
    open: impl FnOnce(u32) -> Result<(), String>,
) -> Result<(xcb::x::Window, xcb::x::Window, i32, i32), String> {
    use xcb::{Xid, XidNew};
    let (w, h) = size.unwrap_or((rect[2], rect[3]));
    let (w, h) = (w.max(1), h.max(1));
    let child = |parent: xcb::x::Window, r: [i32; 4]| -> Result<xcb::x::Window, String> {
        let win: xcb::x::Window = conn.generate_id();
        conn.send_and_check_request(&xcb::x::CreateWindow {
            depth: xcb::x::COPY_FROM_PARENT as u8,
            wid: win,
            parent,
            x: r[0] as i16,
            y: r[1] as i16,
            width: r[2].max(1) as u16,
            height: r[3].max(1) as u16,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: xcb::x::COPY_FROM_PARENT,
            value_list: &[xcb::x::Cw::BackPixel(0)],
        })
        .map_err(|e| format!("Cannot create the editor area: {e}"))?;
        conn.send_request(&xcb::x::MapWindow { window: win });
        Ok(win)
    };
    let frame = child(xcb::x::Window::new(parent), clip)?;
    let win = match child(frame, [rect[0] - clip[0], rect[1] - clip[1], w, h]) {
        Ok(w) => w,
        Err(e) => {
            conn.send_request(&xcb::x::DestroyWindow { window: frame });
            let _ = conn.flush();
            return Err(e);
        }
    };
    let _ = conn.flush();
    open(win.resource_id()).map_err(|e| {
        conn.send_request(&xcb::x::DestroyWindow { window: frame });
        let _ = conn.flush();
        format!("Editor: {e}")
    })?;
    Ok((frame, win, w, h))
}

fn place(conn: &xcb::Connection, e: &mut Embedded, rect: [i32; 4], clip: [i32; 4], visible: bool) {
    let geo = |r: [i32; 4]| {
        [
            xcb::x::ConfigWindow::X(r[0]),
            xcb::x::ConfigWindow::Y(r[1]),
            xcb::x::ConfigWindow::Width(r[2].max(1) as u32),
            xcb::x::ConfigWindow::Height(r[3].max(1) as u32),
        ]
    };
    if clip != e.clip {
        conn.send_request(&xcb::x::ConfigureWindow {
            window: e.frame,
            value_list: &geo(clip),
        });
    }
    if rect != e.rect || clip != e.clip {
        conn.send_request(&xcb::x::ConfigureWindow {
            window: e.win,
            value_list: &geo([rect[0] - clip[0], rect[1] - clip[1], rect[2], rect[3]]),
        });
    }
    e.rect = rect;
    e.clip = clip;
    if visible != e.visible {
        if visible {
            conn.send_request(&xcb::x::MapWindow { window: e.frame });
        } else {
            conn.send_request(&xcb::x::UnmapWindow { window: e.frame });
        }
        e.visible = visible;
    }
    let _ = conn.flush();
}

/// Tell the editor where its area is on screen, with a synthetic
/// ConfigureNotify on the area window (ICCCM: root coordinates). yabridge hands
/// this to Wine, which maps the pointer through it; without it clicks land as if
/// the editor sat at the screen corner. Also needed when our main window moves,
/// which the area window does not hear about. Sent only when the position changed.
fn announce(conn: &xcb::Connection, e: &mut Embedded) {
    let Ok(tree) = conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree { window: e.win }))
    else {
        return;
    };
    let Ok(at) = conn.wait_for_reply(conn.send_request(&xcb::x::TranslateCoordinates {
        src_window: e.win,
        dst_window: tree.root(),
        src_x: 0,
        src_y: 0,
    })) else {
        return;
    };
    let pos = (at.dst_x(), at.dst_y());
    if e.told == Some(pos) {
        return;
    }
    e.told = Some(pos);
    let ev = xcb::x::ConfigureNotifyEvent::new(
        e.win,
        e.win,
        xcb::x::WINDOW_NONE,
        pos.0,
        pos.1,
        e.rect[2].max(1) as u16,
        e.rect[3].max(1) as u16,
        0,
        false,
    );
    conn.send_request(&xcb::x::SendEvent {
        propagate: false,
        destination: xcb::x::SendEventDest::Window(e.win),
        event_mask: xcb::x::EventMask::STRUCTURE_NOTIFY,
        event: &ev,
    });
    let _ = conn.flush();
}

/// Mark the plugin's own windows (children and grandchildren of the area: for
/// yabridge, its wrapper and the Wine window) as shown (WM_STATE Normal). Wine
/// 10+ ignores position updates for a window until a window manager has
/// "mapped" it, and no window manager ever sees an embedded window, so without
/// this Wine keeps the editor at the screen corner and every click misses.
fn mark_shown(conn: &xcb::Connection, e: &mut Embedded) {
    use xcb::Xid;
    static WM_STATE: std::sync::OnceLock<Option<xcb::x::Atom>> = std::sync::OnceLock::new();
    let Some(atom) = *WM_STATE.get_or_init(|| {
        conn.wait_for_reply(conn.send_request(&xcb::x::InternAtom {
            only_if_exists: false,
            name: b"WM_STATE",
        }))
        .ok()
        .map(|r| r.atom())
    }) else {
        return;
    };
    let children = |w: xcb::x::Window| {
        conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree { window: w }))
            .map(|t| t.children().to_vec())
            .unwrap_or_default()
    };
    for top in children(e.win) {
        for w in std::iter::once(top).chain(children(top)) {
            if e.marked.contains(&w.resource_id()) {
                continue;
            }
            e.marked.push(w.resource_id());
            // A window that appeared late (yabridge makes the Wine window after
            // the editor opens, VST2 especially) has not heard the position yet.
            e.told = None;
            // [state, icon window]: NormalState = 1.
            conn.send_request(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: w,
                property: atom,
                r#type: atom,
                data: &[1u32, 0],
            });
        }
    }
    let _ = conn.flush();
}

fn unembed(conn: Option<&xcb::Connection>, l: &mut Loaded) {
    if let Some(e) = l.embed.take() {
        l.plugin.close_editor();
        if let Some(c) = conn {
            c.send_request(&xcb::x::DestroyWindow { window: e.frame });
            let _ = c.flush();
        }
    }
}

pub struct HostHandle {
    tx: mpsc::Sender<Req>,
    pub rx: mpsc::Receiver<Reply>,
}

impl HostHandle {
    pub fn new(tx: mpsc::Sender<Req>, rx: mpsc::Receiver<Reply>) -> Self {
        Self { tx, rx }
    }

    pub fn send(&self, r: Req) {
        let _ = self.tx.send(r);
    }

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
/// A bus the plugin refuses stays off.
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

/// The pointer over X11 window `win`, in its pixels (None off X11).
pub fn pointer_in(win: u32) -> Option<(i32, i32)> {
    let (conn, _) = xcb::Connection::connect(None).ok()?;
    let window = <xcb::x::Window as xcb::XidNew>::new(win);
    let r = conn
        .wait_for_reply(conn.send_request(&xcb::x::QueryPointer { window }))
        .ok()?;
    r.same_screen()
        .then_some((r.win_x() as i32, r.win_y() as i32))
}

/// Editor closed, processing stopped, instance released (the audio side must
/// have let go of it first for the module to unload).
fn close(xconn: Option<&xcb::Connection>, mut l: Loaded) {
    unembed(xconn, &mut l);
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
    // Our own X11 connection for editor child windows (made on first embed).
    let mut xconn: Option<xcb::Connection> = None;
    let mut last_told = std::time::Instant::now();
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
                    drop(old);
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
                visible,
            }) => {
                let Some(l) = loaded.get_mut(&id) else {
                    continue;
                };
                if xconn.is_none() {
                    xconn = xcb::Connection::connect(None).ok().map(|(c, _)| c);
                }
                let Some(conn) = &xconn else {
                    let _ = tx.send(Reply::Failed {
                        id,
                        error: "No X11 display for the editor".into(),
                    });
                    continue;
                };
                match &mut l.embed {
                    Some(e) => place(conn, e, rect, clip, visible),
                    None if visible => {
                        if let Some(mut w) = l.window.take() {
                            w.close();
                        }
                        let res = l.plugin.embed(conn, parent, rect, clip);
                        match res {
                            Ok((frame, win, w, h)) => {
                                l.embed = Some(Embedded {
                                    frame,
                                    win,
                                    rect: [rect[0], rect[1], w, h],
                                    clip,
                                    visible: true,
                                    told: None,
                                    marked: Vec::new(),
                                });
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
                    close(xconn.as_ref(), l);
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
                    unembed(xconn.as_ref(), l);
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
                    unembed(xconn.as_ref(), &mut l);
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
        // Embedded editors: the plugin's run loop, its own resize requests.
        if let Some(conn) = &xconn {
            while let Ok(Some(_)) = conn.poll_for_event() {}
        }
        let tell = last_told.elapsed() >= Duration::from_millis(200);
        if tell {
            last_told = std::time::Instant::now();
        }
        for (id, l) in loaded.iter_mut() {
            let Some(e) = &mut l.embed else { continue };
            if tell
                && e.visible
                && let Some(conn) = &xconn
            {
                mark_shown(conn, e);
                announce(conn, e);
            }
            {
                if let Some((w, h)) = l.plugin.service()
                    && let Some(conn) = &xconn
                {
                    let rect = [e.rect[0], e.rect[1], w, h];
                    let (clip, vis) = (e.clip, e.visible);
                    place(conn, e, rect, clip, vis);
                    let _ = tx.send(Reply::EditorSize { id: *id, w, h });
                    poke();
                }
            }
        }
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
    }
    let trace = std::env::var_os("KFXL_PLUGIN_TRACE").is_some();
    for (id, l) in loaded.drain() {
        if trace {
            eprintln!("host: closing plugin {id}");
        }
        close(xconn.as_ref(), l);
    }
    if trace {
        eprintln!("host: plugins closed");
    }
    drop(host);
    let _ = tx.send(Reply::Bye);
    for (_, mut l) in loaded.drain() {
        unembed(xconn.as_ref(), &mut l);
        if let Some(w) = &mut l.window {
            w.close();
        }
    }
}

#[cfg(test)]
mod vst2_scan_tests {
    use super::*;

    #[test]
    fn vst2_libraries_are_found_in_subfolders_and_told_apart_from_vst3() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("yabridge");
        std::fs::create_dir_all(sub.join("Vendor")).unwrap();
        for f in [
            "SSDSampler5.so",
            "SSDSampler5.dll",
            "Vendor/Other.so",
            "notes.txt",
        ] {
            std::fs::write(sub.join(f), b"x").unwrap();
        }
        let found = vst2_files(&[dir.path().to_path_buf()]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["SSDSampler5.so", "Other.so"]);
        assert_eq!(format_of(&found[0]), "vst2");
        assert_eq!(format_of(Path::new("/x/Grand Piano.vst3")), "vst3");
    }
}

#[cfg(test)]
mod wine_tests {
    #[test]
    fn wine_builds_start_with_the_system_one_and_point_at_real_wine() {
        let builds = super::wine_builds();
        assert_eq!(builds[0], ("System Wine".to_string(), None));
        for (label, bin) in &builds[1..] {
            let bin = bin.as_ref().unwrap();
            assert!(bin.join("wine").is_file(), "{label}: {}", bin.display());
            println!("{label}: {}", bin.display());
        }
    }

    #[test]
    fn another_wines_server_on_the_prefix_is_found_and_stopped() {
        // Needs real Wine builds: KEYFRAMEXL_TEST_WINE=<a non-system wine bin folder>.
        let Some(other) = std::env::var_os("KEYFRAMEXL_TEST_WINE").map(std::path::PathBuf::from)
        else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: test-only; no other test reads WINEPREFIX.
        unsafe { std::env::set_var("WINEPREFIX", dir.path()) };
        std::fs::create_dir_all(dir.path()).unwrap();
        super::set_wine(Some(other));
        assert!(super::foreign_wineserver().is_none(), "nothing runs yet");
        // The system Wine's server holds the prefix.
        std::process::Command::new("wineserver")
            .arg("-p20")
            .env("WINEPREFIX", dir.path())
            .status()
            .unwrap();
        let (pid, exe) = (0..50)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                super::foreign_wineserver()
            })
            .expect("the system wineserver is foreign to the chosen build");
        assert!(exe.ends_with("wineserver"));
        super::stop_foreign_wineserver(pid, &exe).unwrap();
        assert!(super::foreign_wineserver().is_none());
        super::set_wine(None);
    }

    #[test]
    fn protons_steamuser_profile_becomes_the_users_own() {
        let Some(me) = std::env::var_os("USER").filter(|u| !u.is_empty() && u != "steamuser")
        else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let users = dir.path().join("drive_c/users");
        std::fs::create_dir_all(users.join(&me).join("AppData/Roaming/ssd_sampler5")).unwrap();
        std::fs::write(
            users
                .join(&me)
                .join("AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
            "base dir",
        )
        .unwrap();
        std::fs::create_dir_all(users.join("steamuser/AppData/Roaming/ssd_sampler5")).unwrap();
        std::fs::write(
            users.join("steamuser/AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
            "empty",
        )
        .unwrap();
        super::link_steamuser(dir.path());
        let seen = std::fs::read_to_string(
            users.join("steamuser/AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
        )
        .unwrap();
        assert_eq!(seen, "base dir", "Proton now reads the user's own settings");
        let backups: Vec<_> = std::fs::read_dir(&users)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("steamuser.before-keyframexl-")
            })
            .collect();
        assert_eq!(backups.len(), 1, "the old profile is kept");
        // Twice is harmless.
        super::link_steamuser(dir.path());
        assert!(
            std::fs::symlink_metadata(users.join("steamuser"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn dxvk_goes_into_the_prefix_by_rename_and_only_when_it_differs() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("files");
        std::fs::create_dir_all(files.join("bin")).unwrap();
        for arch in ["x86_64-windows", "i386-windows"] {
            let d = files.join("lib/wine/dxvk").join(arch);
            std::fs::create_dir_all(&d).unwrap();
            for n in ["dxgi.dll", "d3d11.dll", "d3d10core.dll"] {
                std::fs::write(d.join(n), format!("dxvk {arch} {n}")).unwrap();
            }
        }
        let bin = files.join("bin");
        assert!(super::has_dxvk(&bin));
        let prefix = dir.path().join("pfx");
        let sys = prefix.join("drive_c/windows/system32");
        std::fs::create_dir_all(&sys).unwrap();
        std::fs::create_dir_all(prefix.join("drive_c/windows/syswow64")).unwrap();
        std::fs::write(sys.join("dxgi.dll"), "wine placeholder").unwrap();
        super::install_dxvk(&bin, &prefix);
        assert_eq!(
            std::fs::read_to_string(sys.join("dxgi.dll")).unwrap(),
            "dxvk x86_64-windows dxgi.dll"
        );
        assert_eq!(
            std::fs::read_to_string(prefix.join("drive_c/windows/syswow64/d3d11.dll")).unwrap(),
            "dxvk i386-windows d3d11.dll"
        );
        // The same files again: nothing is rewritten (the inode stays).
        use std::os::unix::fs::MetadataExt;
        let ino = std::fs::metadata(sys.join("dxgi.dll")).unwrap().ino();
        super::install_dxvk(&bin, &prefix);
        assert_eq!(std::fs::metadata(sys.join("dxgi.dll")).unwrap().ino(), ino);
        // A plain Wine has no DXVK: nothing to do.
        assert!(!super::has_dxvk(&dir.path().join("plain/bin")));
    }

    #[test]
    fn proton_builds_get_their_vkd3d_on_the_dll_path() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("files");
        std::fs::create_dir_all(files.join("bin")).unwrap();
        std::fs::create_dir_all(files.join("lib/vkd3d/x86_64-windows")).unwrap();
        std::fs::create_dir_all(files.join("lib/wine")).unwrap();
        let got = super::wine_dll_path(&files.join("bin")).expect("a Proton layout");
        let dirs: Vec<_> = std::env::split_paths(&got).collect();
        assert_eq!(dirs[0], files.join("lib/vkd3d"));
        assert_eq!(dirs[1], files.join("lib/wine"));
        // A plain Wine build (no vkd3d folder) is left alone.
        let plain = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plain.path().join("bin")).unwrap();
        assert!(super::wine_dll_path(&plain.path().join("bin")).is_none());
    }

    #[test]
    fn the_chosen_wine_goes_first_in_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wine"), b"").unwrap();
        super::set_wine(Some(dir.path().to_path_buf()));
        let mut c = std::process::Command::new("true");
        super::apply_wine(&mut c);
        let path = c
            .get_envs()
            .find(|(k, _)| *k == "PATH")
            .and_then(|(_, v)| v)
            .unwrap()
            .to_owned();
        assert!(std::env::split_paths(&path).next() == Some(dir.path().to_path_buf()));
        super::set_wine(None);
    }
}

#[cfg(test)]
mod vst3_bus_check {
    use super::*;

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
        // The Wine the program is set to (another wineserver on the prefix
        // would refuse us): its environment, as plugin processes get it.
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
        // Samplers load their sounds in the background first.
        std::thread::sleep(std::time::Duration::from_secs(5));
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
            // The old single-pair path, for comparison.
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
