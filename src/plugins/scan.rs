//! Plugin folder discovery, out-of-process probing (`keyframexl plugin-probe`),
//! and scan-cache persistence.

use super::format_of;
use super::wine::apply_wine_for;
use crate::clap::ClapPlugin;
use crate::vst2::Vst2Plugin;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vst3_host::{
    Vst3Host,
    audio::AudioBuffers,
    midi::{MidiChannel, MidiEvent},
};

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
    /// Bridged Windows plugin (yabridge on Linux).
    #[serde(default)]
    pub bridged: bool,
}

impl Scanned {
    pub fn usable(&self) -> bool {
        self.status == Status::Ok
    }
}

/// Extract the cross-OS filename stem (handling both `/` and `\`) and optional
/// CLAP `#<id>` suffix from a plugin path saved on Linux or Windows.
fn plugin_path_key(raw: &str) -> (String, Option<String>) {
    let (file_part, clap_id) = match raw.find(".clap#").or_else(|| {
        let lower = raw.to_ascii_lowercase();
        lower.find(".clap#")
    }) {
        Some(idx) => {
            let split = idx + ".clap".len();
            (&raw[..split], Some(raw[split + 1..].to_string()))
        }
        None => (raw, None),
    };
    let trimmed = file_part.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    let stem = match base.rsplit_once('.') {
        Some((s, ext))
            if ext.eq_ignore_ascii_case("vst3")
                || ext.eq_ignore_ascii_case("clap")
                || ext.eq_ignore_ascii_case("so")
                || ext.eq_ignore_ascii_case("dll") =>
        {
            s
        }
        _ => base,
    };
    (stem.to_ascii_lowercase(), clap_id)
}

fn format_of_raw_path(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase();
    if lower.contains(".clap#") || lower.ends_with(".clap") {
        "clap"
    } else if lower.ends_with(".so") || lower.ends_with(".dll") {
        "vst2"
    } else {
        "vst3"
    }
}

/// Resolve a saved `PluginRef` to a loadable path on this computer.
///
/// 1. If the stored path (or separator-normalized path) already exists on disk,
///    or is a special FL link/native marker, use it as-is.
/// 2. Otherwise match against usable `scanned` plugins of the same format:
///    first by CLAP sub-plugin ID (`#<id>`), then by cross-OS bundle/library
///    stem (`.dll` ↔ `.so`, `.vst3`, `.clap`), and finally by case-insensitive
///    plugin name (preferring a matching vendor when multiple candidates exist).
/// 3. If nothing matches, return the original path so the UI reports it missing.
pub fn resolve_plugin_path(pref: &keyframexl::sequence::PluginRef, scanned: &[Scanned]) -> PathBuf {
    let raw = pref.path.trim();
    if raw.is_empty() {
        return PathBuf::new();
    }
    let direct = PathBuf::from(raw);
    if crate::fl_link::is_link(&direct) || crate::fl_native::slot(raw).is_some() {
        return direct;
    }
    if crate::clap::split_path(&direct).0.exists() {
        return direct;
    }
    // Try normalizing path separators for cross-OS relative paths.
    let normalized = if cfg!(target_os = "windows") {
        raw.replace('/', "\\")
    } else {
        raw.replace('\\', "/")
    };
    if normalized != raw {
        let norm_path = PathBuf::from(&normalized);
        if crate::clap::split_path(&norm_path).0.exists() {
            return norm_path;
        }
    }

    let want_fmt = if pref.format.is_empty() {
        format_of_raw_path(raw)
    } else {
        pref.format.as_str()
    };
    let (want_stem, want_clap_id) = plugin_path_key(raw);

    let candidates: Vec<&Scanned> = scanned
        .iter()
        .filter(|s| s.usable() && format_of(&s.path).eq_ignore_ascii_case(want_fmt))
        .collect();

    let pick_best = |matches: Vec<&Scanned>| -> Option<PathBuf> {
        if matches.is_empty() {
            return None;
        }
        if matches.len() == 1 {
            return Some(matches[0].path.clone());
        }
        // Prefer exact vendor + name match, then vendor match, then name match.
        let by_both = matches.iter().find(|s| {
            !pref.vendor.is_empty()
                && s.vendor.eq_ignore_ascii_case(&pref.vendor)
                && !pref.name.is_empty()
                && s.name.eq_ignore_ascii_case(&pref.name)
        });
        let by_vendor = matches
            .iter()
            .find(|s| !pref.vendor.is_empty() && s.vendor.eq_ignore_ascii_case(&pref.vendor));
        let by_name = matches
            .iter()
            .find(|s| !pref.name.is_empty() && s.name.eq_ignore_ascii_case(&pref.name));
        by_both
            .or(by_vendor)
            .or(by_name)
            .or(matches.first())
            .map(|s| s.path.clone())
    };

    // Tier 1: CLAP unique plugin ID (`...foo.clap#<id>`).
    if let Some(want_id) = want_clap_id.as_deref() {
        let exact_id: Vec<&Scanned> = candidates
            .iter()
            .copied()
            .filter(|s| {
                let (_, cid) = plugin_path_key(&s.path.to_string_lossy());
                cid.as_deref() == Some(want_id)
            })
            .collect();
        if let Some(found) = pick_best(exact_id) {
            return found;
        }
        let ci_id: Vec<&Scanned> = candidates
            .iter()
            .copied()
            .filter(|s| {
                let (_, cid) = plugin_path_key(&s.path.to_string_lossy());
                cid.as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(want_id))
            })
            .collect();
        if let Some(found) = pick_best(ci_id) {
            return found;
        }
    }

    // Tier 2: bundle / library stem + format (handles Linux .so <-> Windows .dll).
    if !want_stem.is_empty() {
        let stem_matches: Vec<&Scanned> = candidates
            .iter()
            .copied()
            .filter(|s| {
                let (stem, _) = plugin_path_key(&s.path.to_string_lossy());
                stem == want_stem
            })
            .collect();
        if let Some(found) = pick_best(stem_matches) {
            return found;
        }
    }

    // Tier 3: plugin display name + format (preferring vendor match).
    if !pref.name.is_empty() {
        let name_matches: Vec<&Scanned> = candidates
            .iter()
            .copied()
            .filter(|s| s.name.eq_ignore_ascii_case(&pref.name))
            .collect();
        if let Some(found) = pick_best(name_matches) {
            return found;
        }
    }

    direct
}

/// Standard VST3 folders for the current OS plus `VST3_PATH` and the user's extra folders.
pub fn search_dirs(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let common = std::env::var_os("COMMONPROGRAMFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Common Files"));
        v.push(common.join("VST3"));
        if let Some(local) = dirs::data_local_dir() {
            v.push(local.join("Programs").join("Common").join("VST3"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Only loadable bundles are scanned: native Linux ones and yabridge shims
        // under `~/.vst3`. Raw Windows bundles inside the Wine prefix are skipped.
        if let Some(h) = dirs::home_dir() {
            v.push(h.join(".vst3"));
        }
        v.push("/usr/lib/vst3".into());
        v.push("/usr/local/lib/vst3".into());
        v.push("/opt/vst3".into());
    }
    if let Ok(p) = std::env::var("VST3_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    v.extend(extra.iter().cloned());
    dedup_paths(v)
}

/// Standard VST2 folders for the current OS plus `VST_PATH`.
pub fn vst2_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let pf = std::env::var_os("PROGRAMFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        let common = std::env::var_os("COMMONPROGRAMFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| pf.join("Common Files"));
        v.push(pf.join("Steinberg").join("VstPlugins"));
        v.push(pf.join("VstPlugins"));
        v.push(common.join("VST2"));
        v.push(PathBuf::from(r"C:\VstPlugins"));
        if let Some(local) = dirs::data_local_dir() {
            v.push(local.join("Programs").join("Common").join("VST2"));
        }
        // Image-Line FL Studio VST plugins (e.g. FL Studio VSTi.dll).
        let il = pf.join("Image-Line");
        if let Ok(rd) = std::fs::read_dir(&il) {
            let mut fl_dirs: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.to_ascii_lowercase().starts_with("fl studio"))
                })
                .map(|p| p.join("Plugins").join("VST"))
                .collect();
            fl_dirs.sort();
            v.extend(fl_dirs);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(h) = dirs::home_dir() {
            v.push(h.join(".vst"));
        }
        v.push("/usr/lib/vst".into());
        v.push("/usr/local/lib/vst".into());
    }
    if let Some(p) = std::env::var_os("VST_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    dedup_paths(v)
}

/// Standard CLAP folders for the current OS plus `CLAP_PATH`.
pub fn clap_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let common = std::env::var_os("COMMONPROGRAMFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Common Files"));
        v.push(common.join("CLAP"));
        if let Some(local) = dirs::data_local_dir() {
            v.push(local.join("Programs").join("Common").join("CLAP"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(h) = dirs::home_dir() {
            v.push(h.join(".clap"));
        }
        v.push("/usr/lib/clap".into());
        v.push("/usr/local/lib/clap".into());
    }
    if let Some(p) = std::env::var_os("CLAP_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    dedup_paths(v)
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
            out.push(p);
        }
    }
    out
}

/// `.vst3` bundles under `dirs` (a bundle is not searched inside).
/// Depth 6 covers nested vendor/yabridge folders; symlinks are followed.
/// Both directory bundles and single-file `.vst3` modules are collected.
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

/// VST2 libraries (`.dll` on Windows, `.so` on Linux) under `dirs`, depth 6.
pub fn vst2_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    const EXT: &str = "dll";
    #[cfg(not(target_os = "windows"))]
    const EXT: &str = "so";
    collect_files_with_ext(dirs, EXT)
}

/// CLAP libraries (`.clap`) under `dirs`, following symlinks, depth 6.
pub fn clap_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    collect_files_with_ext(dirs, "clap")
}

fn collect_files_with_ext(dirs: &[PathBuf], ext: &str) -> Vec<PathBuf> {
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
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

pub(crate) fn stamp(p: &Path) -> u64 {
    let (base, _) = crate::clap::split_path(p);
    let p = base.as_path();
    let mut latest: Option<std::time::SystemTime> = None;
    for sub in ["Contents/x86_64-linux", "Contents/x86_64-win"] {
        let dir = p.join(sub);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
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
    cache.sort_by_key(|a| a.name.to_lowercase());
    save_cache(&cache);
    s
}

/// Probe what changed since the cache (or everything with `force`); three at once.
/// `progress` gets (done, total, name) as probes finish.
pub fn scan(extra: &[PathBuf], force: bool, progress: impl Fn(usize, usize, &str)) -> Vec<Scanned> {
    let cache = if force { vec![] } else { load_cache() };
    let mut found = bundles(&search_dirs(extra));
    let mut v2 = vst2_dirs();
    v2.extend(extra.iter().cloned());
    found.extend(vst2_files(&v2));
    let mut cl = clap_dirs();
    cl.extend(extra.iter().cloned());
    found.extend(clap_files(&cl));
    found.sort();
    found.dedup();
    let mut out: Vec<Scanned> = Vec::new();
    let mut todo = Vec::new();
    for p in found {
        let st = stamp(&p);
        let cached: Vec<Scanned> = cache
            .iter()
            .filter(|c| c.stamp == st && (c.path == p || crate::clap::split_path(&c.path).0 == p))
            .cloned()
            .collect();
        if !cached.is_empty() {
            out.extend(cached);
        } else {
            todo.push((p, st));
        }
    }
    let total = todo.len();
    let done = Arc::new(Mutex::new(Vec::new()));
    let finished = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for chunk in todo.chunks(3) {
        let handles: Vec<_> = chunk
            .iter()
            .cloned()
            .map(|(p, st)| {
                let done = done.clone();
                let finished = finished.clone();
                std::thread::spawn(move || {
                    let entries = probe_in_child_many(&p, st);
                    finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    done.lock().unwrap().extend(entries);
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let d = done.lock().unwrap();
        let n = finished.load(std::sync::atomic::Ordering::Relaxed);
        if let Some(last) = d.last() {
            progress(n, total, &last.name);
        }
    }
    out.extend(done.lock().unwrap().drain(..));
    out.retain(|s| !crate::fl_link::is_link(&s.path));
    if let Some(link) = fl_link_entry() {
        out.push(link);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    save_cache(&out);
    out
}

/// The FL Studio live link, offered as an instrument when FL is installed (Linux/Wine).
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

/// One entry from a multi-plugin CLAP file probe.
#[derive(Serialize, Deserialize)]
struct ProbeEntry {
    path: PathBuf,
    name: String,
    vendor: String,
    category: String,
    instrument: bool,
    gui: bool,
    status: Status,
}

fn probe_in_child(path: &Path, st: u64) -> Scanned {
    probe_in_child_many(path, st)
        .into_iter()
        .next()
        .unwrap_or_else(|| Scanned {
            path: path.to_path_buf(),
            name: path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            vendor: String::new(),
            category: String::new(),
            instrument: false,
            gui: false,
            status: Status::Failed("No plugin reported by probe".into()),
            stamp: st,
            bridged: false,
        })
}

fn probe_in_child_many(path: &Path, st: u64) -> Vec<Scanned> {
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    #[cfg(target_os = "linux")]
    let bridged = path.join("Contents/x86_64-win").exists()
        || (format_of(path) == "vst2" && path.with_extension("dll").exists());
    #[cfg(not(target_os = "linux"))]
    let bridged = false;

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
        return vec![s];
    };
    let mut command = std::process::Command::new(exe);
    apply_wine_for(&mut command, path);
    crate::platform::hide_console(&mut command);
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
            return vec![s];
        }
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            // Large samplers (SSD5 etc.) can take a while on first start;
            // 90 s still keeps a rescan interactive.
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
    let multi: Vec<Scanned> = text
        .lines()
        .filter_map(|l| l.strip_prefix("KFXL_ENTRY "))
        .filter_map(|j| serde_json::from_str::<ProbeEntry>(j).ok())
        .map(|e| Scanned {
            path: e.path,
            name: e.name,
            vendor: e.vendor,
            category: e.category,
            instrument: e.instrument,
            gui: e.gui,
            status: e.status,
            stamp: st,
            bridged: false,
        })
        .collect();
    if !multi.is_empty() {
        return multi;
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
    // On Linux, a Windows bundle without its Wine side looks like a missing file: say so.
    #[cfg(target_os = "linux")]
    if s.status != Status::Ok
        && s.bridged
        && format_of(path) == "vst3"
        && !path.join("Contents/x86_64-linux").exists()
    {
        s.status = Status::Failed("yabridge shim missing (run yabridgectl sync)".into());
    }
    vec![s]
}

fn probe_clap_instance(path: &Path) -> Result<ProbeOut, String> {
    let mut p = ClapPlugin::load(path)?;
    let desc = p.desc().clone();
    let instrument = p.is_instrument();
    let gui = p.has_editor();
    let layout = p.bus_layout().clone();
    p.activate(48_000.0, 512)?;
    p.start_processing()?;
    let mut ins: Vec<Vec<Vec<f32>>> = layout
        .inputs
        .iter()
        .map(|&ch| vec![vec![0.0f32; 512]; ch.max(1)])
        .collect();
    let mut outs: Vec<Vec<Vec<f32>>> = layout
        .outputs
        .iter()
        .map(|&ch| vec![vec![0.0f32; 512]; ch.max(1)])
        .collect();
    for i in 0..40 {
        if i == 2 && instrument {
            p.queue_note_on(0, 60, 90);
        }
        if i == 30 && instrument {
            p.queue_note_off(0, 60);
        }
        if !p.process(
            &mut ins,
            &mut outs,
            512,
            48_000,
            120.0,
            true,
            (i * 512) as u64,
        ) {
            return Err("CLAP process failed".into());
        }
    }
    p.stop_processing();
    p.deactivate();
    let name = if desc.name.trim().is_empty() {
        crate::clap::split_path(path)
            .0
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        desc.name
    };
    Ok(ProbeOut {
        name,
        vendor: desc.vendor,
        category: if desc.category.trim().is_empty() {
            if instrument {
                "Instrument".into()
            } else {
                "Fx".into()
            }
        } else {
            desc.category
        },
        instrument,
        gui,
    })
}

/// Child side of the scan: load, describe, play a note for a moment.
pub fn probe_main(path: &Path) -> i32 {
    let say = |s: String| {
        use std::io::Write;
        let mut o = std::io::stdout();
        let _ = writeln!(o, "KFXL {s}");
        let _ = o.flush();
    };
    if format_of(path) == "clap" {
        let (file, wanted_id) = crate::clap::split_path(path);
        if wanted_id.is_none() {
            let descs = match crate::clap::list_plugins(&file) {
                Ok(d) => d,
                Err(e) => {
                    say(format!("error {e}"));
                    return 1;
                }
            };
            if descs.len() > 1 {
                use std::io::Write;
                let mut o = std::io::stdout();
                for desc in descs {
                    let sub_path = crate::clap::join_path(&file, &desc.id);
                    let entry = match probe_clap_instance(&sub_path) {
                        Ok(out) => ProbeEntry {
                            path: sub_path,
                            name: out.name,
                            vendor: out.vendor,
                            category: out.category,
                            instrument: out.instrument,
                            gui: out.gui,
                            status: Status::Ok,
                        },
                        Err(e) => ProbeEntry {
                            path: sub_path,
                            name: desc.name,
                            vendor: desc.vendor,
                            category: desc.category,
                            instrument: desc.instrument,
                            gui: false,
                            status: Status::Failed(e),
                        },
                    };
                    if let Ok(json) = serde_json::to_string(&entry) {
                        let _ = writeln!(o, "KFXL_ENTRY {json}");
                        let _ = o.flush();
                    }
                }
                say("ok".into());
                std::process::exit(0);
            }
        }
        match probe_clap_instance(path) {
            Ok(out) => {
                say(serde_json::to_string(&out).unwrap_or_default());
                say("ok".into());
                std::process::exit(0);
            }
            Err(e) => {
                say(format!("error {e}"));
                return 1;
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_libraries_are_found_and_identified_with_optional_sub_id() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("Vendor");
        std::fs::create_dir_all(&sub).unwrap();
        let p1 = dir.path().join("SurgeXT.clap");
        let p2 = sub.join("Suite.clap");
        std::fs::write(&p1, b"clap1").unwrap();
        std::fs::write(&p2, b"clap2").unwrap();
        std::fs::write(dir.path().join("ignore.txt"), b"txt").unwrap();

        let found = clap_files(&[dir.path().to_path_buf()]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["SurgeXT.clap", "Suite.clap"]);
        assert_eq!(format_of(&p1), "clap");
        let sub_path = crate::clap::join_path(&p2, "org.example.synth");
        assert_eq!(format_of(&sub_path), "clap");
        assert_eq!(stamp(&sub_path), stamp(&p2));
        assert!(stamp(&sub_path) > 0);
    }

    #[test]
    fn vst2_libraries_are_found_in_subfolders_and_told_apart_from_vst3() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("yabridge");
        std::fs::create_dir_all(sub.join("Vendor")).unwrap();
        for f in [
            "SSDSampler5.so",
            "SSDSampler5.dll",
            "Vendor/Other.so",
            "Vendor/Other.dll",
            "notes.txt",
        ] {
            std::fs::write(sub.join(f), b"x").unwrap();
        }
        let found = vst2_files(&[dir.path().to_path_buf()]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        #[cfg(target_os = "windows")]
        assert_eq!(names, ["SSDSampler5.dll", "Other.dll"]);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(names, ["SSDSampler5.so", "Other.so"]);
        assert_eq!(format_of(&found[0]), "vst2");
        assert_eq!(format_of(Path::new("/x/Grand Piano.vst3")), "vst3");
    }

    #[test]
    fn standard_search_dirs_include_platform_locations_and_extras() {
        let extra = vec![PathBuf::from("custom-plugins")];
        let vst3 = search_dirs(&extra);
        assert!(vst3.contains(&PathBuf::from("custom-plugins")));
        assert!(!vst2_dirs().is_empty());
        assert!(!clap_dirs().is_empty());
        #[cfg(target_os = "windows")]
        {
            assert!(vst3.iter().any(|p| p.ends_with("VST3")));
            assert!(clap_dirs().iter().any(|p| p.ends_with("CLAP")));
            assert!(
                vst2_dirs()
                    .iter()
                    .any(|p| p.to_string_lossy().to_ascii_lowercase().contains("vst"))
            );
        }
    }

    #[test]
    fn resolve_plugin_path_matches_across_linux_and_windows() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("Local.vst3");
        std::fs::write(&existing, b"vst3").unwrap();

        let scanned = vec![
            Scanned {
                path: PathBuf::from(r"C:\Program Files\Common Files\VST3\Vital.vst3"),
                name: "Vital".into(),
                vendor: "Vital Audio".into(),
                category: "Instrument".into(),
                instrument: true,
                gui: true,
                status: Status::Ok,
                stamp: 1,
                bridged: false,
            },
            Scanned {
                path: PathBuf::from(
                    r"C:\Program Files\Common Files\CLAP\Surge XT.clap#org.surge-synth-team.surge-xt",
                ),
                name: "Surge XT".into(),
                vendor: "Surge Synth Team".into(),
                category: "Instrument".into(),
                instrument: true,
                gui: true,
                status: Status::Ok,
                stamp: 1,
                bridged: false,
            },
            Scanned {
                path: PathBuf::from(r"C:\Program Files\Steinberg\VstPlugins\SSDSampler5.dll"),
                name: "SSD5".into(),
                vendor: "Steven Slate".into(),
                category: "Instrument".into(),
                instrument: true,
                gui: true,
                status: Status::Ok,
                stamp: 1,
                bridged: false,
            },
            Scanned {
                path: PathBuf::from("/usr/lib/vst3/AcmeComp.vst3"),
                name: "Compressor".into(),
                vendor: "Acme".into(),
                category: "Fx".into(),
                instrument: false,
                gui: true,
                status: Status::Ok,
                stamp: 1,
                bridged: false,
            },
            Scanned {
                path: PathBuf::from("/usr/lib/vst3/BetaComp.vst3"),
                name: "Compressor".into(),
                vendor: "Beta".into(),
                category: "Fx".into(),
                instrument: false,
                gui: true,
                status: Status::Ok,
                stamp: 1,
                bridged: false,
            },
        ];

        let mk =
            |format: &str, path: &str, name: &str, vendor: &str| keyframexl::sequence::PluginRef {
                id: 1,
                format: format.into(),
                path: path.into(),
                name: name.into(),
                vendor: vendor.into(),
                state: String::new(),
                outs: vec![],
            };

        // 1. Existing path wins directly.
        let p_local = mk("vst3", &existing.to_string_lossy(), "Local", "Acme");
        assert_eq!(resolve_plugin_path(&p_local, &scanned), existing);

        // 2. Linux VST3 path resolves to Windows VST3 by stem.
        let p_vst3 = mk(
            "vst3",
            "/home/user/.vst3/Vital.vst3",
            "Vital",
            "Vital Audio",
        );
        assert_eq!(
            resolve_plugin_path(&p_vst3, &scanned),
            PathBuf::from(r"C:\Program Files\Common Files\VST3\Vital.vst3")
        );

        // 3. Linux CLAP sub-plugin path resolves to Windows CLAP by #id even if filename differs.
        let p_clap = mk(
            "clap",
            "/home/user/.clap/SurgeXT-Linux.clap#org.surge-synth-team.surge-xt",
            "Surge XT",
            "Surge Synth Team",
        );
        assert_eq!(
            resolve_plugin_path(&p_clap, &scanned),
            PathBuf::from(
                r"C:\Program Files\Common Files\CLAP\Surge XT.clap#org.surge-synth-team.surge-xt"
            )
        );

        // 4. Linux VST2 .so resolves to Windows VST2 .dll by stem.
        let p_vst2 = mk(
            "vst2",
            "/home/user/.vst/SSDSampler5.so",
            "SSD5",
            "Steven Slate",
        );
        assert_eq!(
            resolve_plugin_path(&p_vst2, &scanned),
            PathBuf::from(r"C:\Program Files\Steinberg\VstPlugins\SSDSampler5.dll")
        );

        // 5. Name fallback disambiguates by vendor when stems differ.
        let p_vendor = mk("vst3", r"C:\VST3\DifferentFile.vst3", "Compressor", "Beta");
        assert_eq!(
            resolve_plugin_path(&p_vendor, &scanned),
            PathBuf::from("/usr/lib/vst3/BetaComp.vst3")
        );

        // 6. Unknown plugin preserves its original path so the UI can report it missing.
        let p_missing = mk("vst3", "/home/user/.vst3/Unknown.vst3", "Unknown", "None");
        assert_eq!(
            resolve_plugin_path(&p_missing, &scanned),
            PathBuf::from("/home/user/.vst3/Unknown.vst3")
        );
    }
}
