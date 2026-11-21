//! Non-intrusive GitHub Releases / Tags update checker.
//!
//! Runs completely off the UI and audio threads with short timeouts, zero
//! telemetry (only standard GitHub REST GET requests), daily rate-limiting
//! for automatic checks, and full opt-out via Settings or
//! `KFXL_NO_UPDATE_CHECK=1`.

use std::cmp::Ordering;
use std::fmt;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const REPO: &str = "mallardask-ops/keyframexl";
pub const RELEASES_URL: &str = "https://github.com/mallardask-ops/keyframexl/releases";
/// Nightly builds set `KFXL_BUILD_VERSION` / `KFXL_VERSION` (e.g.
/// `0.1.3-nightly.202611201530`) at compile time; stable builds use the crate version.
pub const CURRENT_VERSION: &str = keyframexl::version::VERSION;

/// Automatic background checks run at most once every 24 hours.
pub const AUTO_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Parsed SemVer (`MAJOR.MINOR.PATCH[-prerelease]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    /// Parse a version tag like `"v0.2.0"`, `"0.2.1-beta.1"`, or `"1.0.0+build"`.
    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim();
        let s = s
            .strip_prefix('v')
            .or_else(|| s.strip_prefix('V'))
            .unwrap_or(s);
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
            Some(_) => return None,
            None => (s, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next()?.parse::<u64>().ok()?;
        let patch = parts.next()?.parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_prerelease(a, b),
            })
    }
}

fn compare_prerelease(a: &str, b: &str) -> Ordering {
    for (pa, pb) in a.split('.').zip(b.split('.')) {
        let ord = match (pa.parse::<u64>(), pb.parse::<u64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => pa.cmp(pb),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.split('.').count().cmp(&b.split('.').count())
}

/// Information about a newer release discovered on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub html_url: String,
    pub linux_asset_url: Option<String>,
    pub linux_sha256_url: Option<String>,
    pub published_at: Option<String>,
}

/// Live progress of downloading and installing an update in place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InstallState {
    #[default]
    Idle,
    Downloading {
        downloaded: u64,
        total: Option<u64>,
    },
    Verifying,
    Installing,
    ReadyToRestart {
        version: String,
        exe_path: std::path::PathBuf,
    },
    Failed(String),
}

/// Outcome of one check against GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    Available(ReleaseInfo),
    UpToDate { latest: Option<String> },
}

/// Live UI state of the background update checker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Idle,
    Checking,
    Available(ReleaseInfo),
    UpToDate {
        latest: Option<String>,
    },
    Error(String),
}

/// Current unix timestamp in seconds.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// True when `KFXL_NO_UPDATE_CHECK` is set to a truthy value.
pub fn disabled_by_env() -> bool {
    match std::env::var("KFXL_NO_UPDATE_CHECK") {
        Ok(v) => {
            let t = v.trim();
            !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false")
        }
        Err(_) => false,
    }
}

/// Pure interval check independent of process environment (for deterministic tests).
fn should_auto_check_inner(
    enabled: bool,
    last_check: u64,
    now_secs: u64,
    env_disabled: bool,
) -> bool {
    enabled
        && !env_disabled
        && (last_check == 0 || now_secs.saturating_sub(last_check) >= AUTO_CHECK_INTERVAL_SECS)
}

/// Whether an automatic startup check is due.
pub fn should_auto_check(enabled: bool, last_check: u64, now_secs: u64) -> bool {
    should_auto_check_inner(enabled, last_check, now_secs, disabled_by_env())
}

/// True when the user previously chose "Skip this version".
pub fn is_skipped(skipped: Option<&str>, version: &str) -> bool {
    let Some(sk) = skipped else {
        return false;
    };
    fn norm(s: &str) -> &str {
        let s = s.trim();
        s.strip_prefix('v')
            .or_else(|| s.strip_prefix('V'))
            .unwrap_or(s)
    }
    norm(sk) == norm(version)
}

/// Pick the most suitable Linux release asset `(name, url)`, ignoring checksums/signatures.
fn pick_linux_asset(assets: &[serde_json::Value]) -> Option<(String, String)> {
    let mut fallback = None;
    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(url) = asset.get("browser_download_url").and_then(|v| v.as_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".sha256")
            || lower.ends_with(".sha512")
            || lower.ends_with(".sig")
            || lower.ends_with(".asc")
            || lower.ends_with(".txt")
        {
            continue;
        }
        if lower.ends_with(".appimage") {
            return Some((name.to_string(), url.to_string()));
        }
        if fallback.is_none()
            && (lower.ends_with(".tar.gz")
                || lower.ends_with(".tgz")
                || lower.ends_with(".tar.xz")
                || lower.contains("linux"))
        {
            fallback = Some((name.to_string(), url.to_string()));
        }
    }
    fallback
}

/// Find the companion `.sha256` asset URL for `asset_name`, if published alongside it.
fn pick_linux_sha256(assets: &[serde_json::Value], asset_name: &str) -> Option<String> {
    let want = format!("{}.sha256", asset_name.to_ascii_lowercase());
    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if name.to_ascii_lowercase() == want {
            return asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }
    None
}

/// Evaluate a GitHub `/releases/latest` JSON payload against `current`.
pub fn parse_release_json(body: &str, current: &Version) -> Result<UpdateOutcome, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid release response: {e}"))?;
    evaluate_release(&v, current, current.is_prerelease())
}

/// Evaluate a GitHub `/releases` list (nightly channel): the highest
/// non-draft release wins, prereleases included when `allow_pre`.
pub fn parse_releases_list_json(
    body: &str,
    current: &Version,
    allow_pre: bool,
) -> Result<UpdateOutcome, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid releases response: {e}"))?;
    let arr = v.as_array().ok_or("Releases response is not a list")?;
    let best = arr
        .iter()
        .filter(|r| !r.get("draft").and_then(|b| b.as_bool()).unwrap_or(false))
        .filter_map(|r| {
            let ver = Version::parse(r.get("tag_name")?.as_str()?)?;
            let pre = r
                .get("prerelease")
                .and_then(|b| b.as_bool())
                .unwrap_or(false)
                || ver.is_prerelease();
            (allow_pre || !pre).then_some((ver, r))
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    match best {
        Some((_, r)) => evaluate_release(r, current, allow_pre),
        None => Ok(UpdateOutcome::UpToDate { latest: None }),
    }
}

fn evaluate_release(
    v: &serde_json::Value,
    current: &Version,
    allow_pre: bool,
) -> Result<UpdateOutcome, String> {
    if v.get("draft").and_then(|b| b.as_bool()).unwrap_or(false) {
        return Ok(UpdateOutcome::UpToDate { latest: None });
    }
    let tag = v
        .get("tag_name")
        .and_then(|s| s.as_str())
        .ok_or("Release response has no tag_name")?;
    let Some(remote) = Version::parse(tag) else {
        return Ok(UpdateOutcome::UpToDate { latest: None });
    };
    let is_pre = v
        .get("prerelease")
        .and_then(|b| b.as_bool())
        .unwrap_or(false)
        || remote.is_prerelease();
    if is_pre && !allow_pre {
        return Ok(UpdateOutcome::UpToDate { latest: None });
    }
    if remote <= *current {
        return Ok(UpdateOutcome::UpToDate {
            latest: Some(remote.to_string()),
        });
    }
    let title = v
        .get("name")
        .and_then(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(tag)
        .to_string();
    let notes = v
        .get("body")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let html_url = v
        .get("html_url")
        .and_then(|s| s.as_str())
        .unwrap_or(RELEASES_URL)
        .to_string();
    let published_at = v
        .get("published_at")
        .and_then(|s| s.as_str())
        .map(|s| s.split('T').next().unwrap_or(s).to_string());
    let assets = v.get("assets").and_then(|a| a.as_array());
    let picked = assets.and_then(|arr| pick_linux_asset(arr));
    let linux_sha256_url = picked
        .as_ref()
        .and_then(|(name, _)| assets.and_then(|arr| pick_linux_sha256(arr, name)));
    let linux_asset_url = picked.map(|(_, url)| url);
    Ok(UpdateOutcome::Available(ReleaseInfo {
        version: remote.to_string(),
        tag: tag.to_string(),
        title,
        notes,
        html_url,
        linux_asset_url,
        linux_sha256_url,
        published_at,
    }))
}

/// Evaluate a GitHub `/tags` JSON payload (used when no GitHub Release exists yet).
pub fn parse_tags_json(body: &str, current: &Version) -> Result<UpdateOutcome, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid tags response: {e}"))?;
    let Some(arr) = v.as_array() else {
        return Ok(UpdateOutcome::UpToDate { latest: None });
    };
    let mut best: Option<(Version, String)> = None;
    for entry in arr {
        let Some(tag) = entry.get("name").and_then(|s| s.as_str()) else {
            continue;
        };
        let Some(ver) = Version::parse(tag) else {
            continue;
        };
        if ver.is_prerelease() && !current.is_prerelease() {
            continue;
        }
        if best.as_ref().is_none_or(|(b, _)| ver > *b) {
            best = Some((ver, tag.to_string()));
        }
    }
    match best {
        Some((ver, tag)) if ver > *current => Ok(UpdateOutcome::Available(ReleaseInfo {
            version: ver.to_string(),
            tag: tag.clone(),
            title: format!("KeyFrameXL {ver}"),
            notes: String::new(),
            html_url: format!("https://github.com/{REPO}/releases/tag/{tag}"),
            linux_asset_url: None,
            linux_sha256_url: None,
            published_at: None,
        })),
        Some((ver, _)) => Ok(UpdateOutcome::UpToDate {
            latest: Some(ver.to_string()),
        }),
        None => Ok(UpdateOutcome::UpToDate { latest: None }),
    }
}

/// Perform a blocking check against GitHub (call only from a background thread).
/// `nightly`: follow the nightly channel (prereleases included).
pub fn fetch_latest(nightly: bool) -> Result<UpdateOutcome, String> {
    let current = Version::parse(CURRENT_VERSION)
        .ok_or_else(|| format!("Invalid build version: {CURRENT_VERSION}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(6))
        .build();
    let ua = format!("KeyFrameXL/{CURRENT_VERSION}");
    if nightly {
        let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=30");
        let body = agent
            .get(&url)
            .set("User-Agent", &ua)
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(friendly_http_error)?
            .into_string()
            .map_err(|e| format!("Could not read release info: {e}"))?;
        return parse_releases_list_json(&body, &current, true);
    }
    let latest_url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    match agent
        .get(&latest_url)
        .set("User-Agent", &ua)
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(resp) => {
            let body = resp
                .into_string()
                .map_err(|e| format!("Could not read release info: {e}"))?;
            parse_release_json(&body, &current)
        }
        Err(ureq::Error::Status(404, _)) => {
            // No published GitHub Release yet: fall back cleanly to git tags.
            let tags_url = format!("https://api.github.com/repos/{REPO}/tags?per_page=10");
            match agent
                .get(&tags_url)
                .set("User-Agent", &ua)
                .set("Accept", "application/vnd.github+json")
                .call()
            {
                Ok(resp) => {
                    let body = resp
                        .into_string()
                        .map_err(|e| format!("Could not read tags: {e}"))?;
                    parse_tags_json(&body, &current)
                }
                Err(ureq::Error::Status(404, _)) => Ok(UpdateOutcome::UpToDate { latest: None }),
                Err(e) => Err(friendly_http_error(e)),
            }
        }
        Err(e) => Err(friendly_http_error(e)),
    }
}

fn friendly_http_error(err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(403 | 429, _) => {
            "GitHub API rate limit reached — try again later.".to_string()
        }
        ureq::Error::Status(code, _) => format!("GitHub returned HTTP {code}."),
        ureq::Error::Transport(_) => {
            "Could not reach GitHub — check your internet connection.".to_string()
        }
    }
}

/// Non-blocking update checker and self-installer owned by `App`.
#[derive(Default)]
pub struct UpdateChecker {
    pub state: CheckState,
    pub install: InstallState,
    pub dialog_open: bool,
    pub checked_on_startup: bool,
    manual: bool,
    rx: Option<mpsc::Receiver<Result<UpdateOutcome, String>>>,
    install_rx: Option<mpsc::Receiver<InstallState>>,
}

impl UpdateChecker {
    /// Start a background check if one is not already running.
    pub fn start(&mut self, ctx: Option<eframe::egui::Context>, manual: bool, nightly: bool) {
        if self.rx.is_some() {
            if manual {
                self.dialog_open = true;
            }
            return;
        }
        self.manual = manual;
        if manual {
            self.dialog_open = true;
        }
        self.state = CheckState::Checking;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::Builder::new()
            .name("kfxl-updater".into())
            .spawn(move || {
                let res = fetch_latest(nightly);
                let _ = tx.send(res);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .ok();
    }

    /// Download, verify, and atomically install `info`'s Linux release over the running binary.
    pub fn start_install(&mut self, info: &ReleaseInfo, ctx: Option<eframe::egui::Context>) {
        if self.is_installing() {
            return;
        }
        let Some(asset_url) = info.linux_asset_url.clone() else {
            self.install = InstallState::Failed(
                "This release does not include a Linux binary archive.".into(),
            );
            return;
        };
        let target = match self_update_target() {
            Ok(t) => t,
            Err(e) if e == "source" => {
                self.install = InstallState::Failed(
                    "This build runs from a cargo target directory; update it from source.".into(),
                );
                return;
            }
            Err(e) => {
                self.install = InstallState::Failed(e);
                return;
            }
        };
        let sha_url = info.linux_sha256_url.clone();
        let version = info.version.clone();
        self.install = InstallState::Downloading {
            downloaded: 0,
            total: None,
        };
        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        std::thread::Builder::new()
            .name("kfxl-self-update".into())
            .spawn(move || {
                let notify = |s: InstallState| {
                    let _ = tx.send(s);
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                };
                match download_and_install(&asset_url, sha_url.as_deref(), &target, &notify) {
                    Ok(()) => notify(InstallState::ReadyToRestart {
                        version,
                        exe_path: target,
                    }),
                    Err(e) => notify(InstallState::Failed(e)),
                }
            })
            .ok();
    }

    /// Poll both background workers without blocking. Returns `true` when state changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.rx = None;
                    self.state = match res {
                        Ok(UpdateOutcome::Available(info)) => CheckState::Available(info),
                        Ok(UpdateOutcome::UpToDate { latest }) => CheckState::UpToDate { latest },
                        Err(e) => {
                            if self.manual {
                                CheckState::Error(e)
                            } else {
                                // Automatic background check failures stay completely silent.
                                CheckState::Idle
                            }
                        }
                    };
                    changed = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    if self.manual {
                        self.state = CheckState::Error("Update check was interrupted.".into());
                    } else {
                        self.state = CheckState::Idle;
                    }
                    changed = true;
                }
            }
        }
        if let Some(irx) = &self.install_rx {
            let mut done = false;
            loop {
                match irx.try_recv() {
                    Ok(st) => {
                        done = matches!(
                            st,
                            InstallState::ReadyToRestart { .. } | InstallState::Failed(_)
                        );
                        self.install = st;
                        changed = true;
                        if done {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !matches!(
                            self.install,
                            InstallState::ReadyToRestart { .. } | InstallState::Failed(_)
                        ) {
                            self.install =
                                InstallState::Failed("Update installation was interrupted.".into());
                            changed = true;
                        }
                        done = true;
                        break;
                    }
                }
            }
            if done {
                self.install_rx = None;
            }
        }
        changed
    }

    /// Returns the discovered release if an update is available and not skipped.
    pub fn available(&self, skipped: Option<&str>) -> Option<&ReleaseInfo> {
        match &self.state {
            CheckState::Available(info) if !is_skipped(skipped, &info.version) => Some(info),
            _ => None,
        }
    }

    pub fn is_checking(&self) -> bool {
        self.rx.is_some()
    }

    pub fn is_installing(&self) -> bool {
        self.install_rx.is_some()
            || matches!(
                self.install,
                InstallState::Downloading { .. }
                    | InstallState::Verifying
                    | InstallState::Installing
            )
    }
}

/// The binary a self-update would replace, or why it should not: a cargo
/// build tree (`target/debug`, `target/release`) is updated from source.
/// `KFXL_FORCE_SELF_UPDATE=1` lifts the source-build guard for testing.
pub fn self_update_target() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .map_err(|e| format!("Could not locate current executable: {e}"))?;
    let forced = std::env::var("KFXL_FORCE_SELF_UPDATE").is_ok_and(|v| v == "1");
    if !forced && is_cargo_build_path(&exe) {
        return Err("source".into());
    }
    Ok(exe)
}

fn is_cargo_build_path(exe: &std::path::Path) -> bool {
    let parts: Vec<_> = exe.components().map(|c| c.as_os_str().to_owned()).collect();
    parts.windows(2).any(|w| {
        w[0] == "target"
            && (w[1] == "debug" || w[1] == "release" || w[1] == "x86_64-unknown-linux-gnu")
    })
}

/// Launch the freshly installed binary with the original arguments.
pub fn relaunch_binary(exe_path: &std::path::Path) -> Result<(), String> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::Command::new(exe_path)
        .args(&args)
        .spawn()
        .map_err(|e| format!("Could not restart {}: {e}", exe_path.display()))?;
    Ok(())
}

/// Full self-update pipeline executed on a worker thread.
fn download_and_install(
    asset_url: &str,
    sha256_url: Option<&str>,
    current_exe: &std::path::Path,
    notify: &impl Fn(InstallState),
) -> Result<(), String> {
    use std::io::{Read, Write};

    let exe_dir = current_exe
        .parent()
        .ok_or("Executable has no parent directory")?;

    // Check write permission before spending bandwidth.
    let probe = exe_dir.join(format!(".kfxl-update-probe-{}", std::process::id()));
    std::fs::File::create(&probe).map_err(|e| {
        format!(
            "Cannot write to {} ({e}). Download the archive manually or update with elevated permissions.",
            exe_dir.display()
        )
    })?;
    let _ = std::fs::remove_file(&probe);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .build();
    let ua = format!("KeyFrameXL/{CURRENT_VERSION}");

    let resp = agent
        .get(asset_url)
        .set("User-Agent", &ua)
        .call()
        .map_err(friendly_http_error)?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());

    let tmp_dir = tempfile::tempdir_in(exe_dir)
        .or_else(|_| tempfile::tempdir())
        .map_err(|e| format!("Could not create temp directory: {e}"))?;
    let archive_name = asset_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("keyframexl-update.tar.gz");
    let archive_path = tmp_dir.path().join(archive_name);

    {
        let mut out = std::fs::File::create(&archive_path)
            .map_err(|e| format!("Could not create download file: {e}"))?;
        let mut reader = resp.into_reader();
        let mut buf = [0u8; 32 * 1024];
        let mut downloaded = 0u64;
        let mut last_ping = std::time::Instant::now();
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("Download interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("Disk write error: {e}"))?;
            downloaded += n as u64;
            if last_ping.elapsed() >= Duration::from_millis(80) {
                notify(InstallState::Downloading { downloaded, total });
                last_ping = std::time::Instant::now();
            }
        }
        out.flush().map_err(|e| format!("Disk flush error: {e}"))?;
        notify(InstallState::Downloading { downloaded, total });
    }

    let expected_sha = if let Some(url) = sha256_url {
        notify(InstallState::Verifying);
        let resp = agent
            .get(url)
            .set("User-Agent", &ua)
            .call()
            .map_err(friendly_http_error)?;
        let text = resp
            .into_string()
            .map_err(|e| format!("Could not read SHA-256 checksum: {e}"))?;
        parse_sha256_text(&text)
    } else {
        None
    };

    notify(InstallState::Installing);
    if sha256_url.is_some() && expected_sha.is_none() {
        return Err("The release's SHA-256 file could not be read.".into());
    }
    install_from_archive(&archive_path, expected_sha.as_deref(), current_exe)
}

/// Extract (or accept direct ELF/AppImage) from `archive_path`, verify optional
/// SHA-256 hex digest, and atomically replace `target_exe`.
pub(crate) fn install_from_archive(
    archive_path: &std::path::Path,
    expected_sha256: Option<&str>,
    target_exe: &std::path::Path,
) -> Result<(), String> {
    if let Some(expected) = expected_sha256 {
        let mut f = std::fs::File::open(archive_path)
            .map_err(|e| format!("Could not open downloaded archive: {e}"))?;
        let actual = sha256_hex(&mut f).map_err(|e| format!("Checksum error: {e}"))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "SHA-256 checksum mismatch (expected {expected}, got {actual})."
            ));
        }
    }

    let exe_dir = target_exe
        .parent()
        .ok_or("Target executable has no parent directory")?;
    let extract_dir = tempfile::tempdir_in(exe_dir)
        .map_err(|e| format!("Could not create staging directory: {e}"))?;

    let lower = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let new_bin_src = if lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.xz")
    {
        let status = std::process::Command::new("tar")
            .arg("-xf")
            .arg(archive_path)
            .arg("-C")
            .arg(extract_dir.path())
            .status()
            .map_err(|e| format!("Could not run `tar` to extract update: {e}"))?;
        if !status.success() {
            return Err("Extracting the release archive failed (`tar` returned an error).".into());
        }
        find_extracted_binary(extract_dir.path())
            .ok_or("Release archive did not contain a `keyframexl` binary.")?
    } else {
        // Direct AppImage or raw Linux ELF binary.
        archive_path.to_path_buf()
    };

    // Sanity-check ELF magic before overwriting the running executable.
    {
        use std::io::Read;
        let mut f = std::fs::File::open(&new_bin_src)
            .map_err(|e| format!("Could not read extracted binary: {e}"))?;
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)
            .map_err(|_| "Downloaded binary is empty or truncated.".to_string())?;
        if &magic != b"\x7fELF" {
            return Err("Downloaded file is not a valid Linux ELF executable.".into());
        }
    }

    // Stage next to `target_exe` (same filesystem) so `rename` is strictly atomic.
    let staged = exe_dir.join(format!(".keyframexl.update.{}", std::process::id()));
    std::fs::copy(&new_bin_src, &staged).map_err(|e| format!("Could not stage new binary: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Could not mark updated binary executable: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&staged, target_exe) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("Could not replace {}: {e}", target_exe.display()));
    }
    Ok(())
}

fn find_extracted_binary(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|s| s.to_str()) == Some("keyframexl") {
                return Some(p);
            }
        }
    }
    None
}

fn parse_sha256_text(text: &str) -> Option<String> {
    let tok = text.split_whitespace().next()?;
    if tok.len() == 64 && tok.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(tok.to_ascii_lowercase())
    } else {
        None
    }
}

/// Pure-Rust streaming SHA-256 (FIPS 180-4) returning lowercase hex.
pub(crate) fn sha256_hex<R: std::io::Read>(reader: &mut R) -> std::io::Result<String> {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let compress = |h: &mut [u32; 8], block: &[u8; 64]| {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (dst, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *dst = dst.wrapping_add(add);
        }
    };

    let mut pending = [0u8; 64];
    let mut plen = 0usize;
    let mut total_bytes = 0u64;
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total_bytes += n as u64;
        let mut off = 0;
        while off < n {
            let take = (64 - plen).min(n - off);
            pending[plen..plen + take].copy_from_slice(&buf[off..off + take]);
            plen += take;
            off += take;
            if plen == 64 {
                compress(&mut h, &pending);
                plen = 0;
            }
        }
    }
    pending[plen] = 0x80;
    plen += 1;
    if plen > 56 {
        pending[plen..64].fill(0);
        compress(&mut h, &pending);
        plen = 0;
    }
    pending[plen..56].fill(0);
    pending[56..64].copy_from_slice(&(total_bytes * 8).to_be_bytes());
    compress(&mut h, &pending);

    let mut out = String::with_capacity(64);
    for word in h {
        use std::fmt::Write;
        let _ = write!(&mut out, "{word:08x}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_build_paths_are_not_self_updated() {
        use std::path::Path;
        assert!(is_cargo_build_path(Path::new(
            "/home/u/kfxl/target/release/keyframexl"
        )));
        assert!(is_cargo_build_path(Path::new(
            "/home/u/kfxl/target/debug/keyframexl"
        )));
        assert!(!is_cargo_build_path(Path::new(
            "/opt/keyframexl-linux-x86_64/keyframexl"
        )));
        assert!(!is_cargo_build_path(Path::new("/home/u/target/keyframexl")));
    }

    #[test]
    fn sha256_matches_known_vectors() {
        let hex = |s: &[u8]| sha256_hex(&mut &s[..]).unwrap();
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let long = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&long),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            parse_sha256_text(
                "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  x.tar.gz\n"
            )
            .as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(parse_sha256_text("nope"), None);
    }

    #[test]
    fn install_from_archive_replaces_binary_and_checks_sha() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("keyframexl-linux-x86_64");
        std::fs::create_dir(&stage).unwrap();
        let new_bin = b"\x7fELFnew-build".to_vec();
        std::fs::write(stage.join("keyframexl"), &new_bin).unwrap();
        let archive = dir.path().join("keyframexl-linux-x86_64.tar.gz");
        let ok = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(dir.path())
            .arg("keyframexl-linux-x86_64")
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let target_dir = dir.path().join("install");
        std::fs::create_dir(&target_dir).unwrap();
        let target = target_dir.join("keyframexl");
        std::fs::write(&target, b"\x7fELFold").unwrap();

        // A wrong checksum leaves the old binary alone.
        let bad = "0".repeat(64);
        assert!(install_from_archive(&archive, Some(&bad), &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFold");

        let sha = sha256_hex(&mut std::fs::File::open(&archive).unwrap()).unwrap();
        install_from_archive(&archive, Some(&sha), &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), new_bin);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        // Nothing staged is left next to the binary.
        let leftovers: Vec<_> = std::fs::read_dir(&target_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    }

    #[test]
    fn release_json_pairs_archive_with_its_checksum() {
        let cur = Version::parse("0.1.0").unwrap();
        let json = r#"{
            "tag_name": "v0.1.1", "draft": false, "prerelease": false,
            "assets": [
                {"name": "keyframexl-linux-x86_64.tar.gz.sha256", "browser_download_url": "https://example.com/sha"},
                {"name": "keyframexl-linux-x86_64.tar.gz", "browser_download_url": "https://example.com/tar"}
            ]
        }"#;
        let UpdateOutcome::Available(info) = parse_release_json(json, &cur).unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(
            info.linux_asset_url.as_deref(),
            Some("https://example.com/tar")
        );
        assert_eq!(
            info.linux_sha256_url.as_deref(),
            Some("https://example.com/sha")
        );
    }

    /// Downloads the real published release: `cargo test --bin keyframexl -- --ignored live_github`.
    #[test]
    #[ignore = "network"]
    fn live_github_release_installs_into_temp_dir() {
        let body = ureq::get(&format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .set("User-Agent", "KeyFrameXL-test")
        .call()
        .expect("GitHub reachable")
        .into_string()
        .unwrap();
        let UpdateOutcome::Available(info) =
            parse_release_json(&body, &Version::parse("0.0.1").unwrap()).unwrap()
        else {
            panic!("no release");
        };
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("keyframexl");
        std::fs::write(&target, b"\x7fELFold").unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        download_and_install(
            info.linux_asset_url.as_deref().expect("linux asset"),
            Some(info.linux_sha256_url.as_deref().expect("sha256 asset")),
            &target,
            &|s| seen.borrow_mut().push(s),
        )
        .unwrap();
        let seen = seen.into_inner();
        assert!(seen.contains(&InstallState::Verifying));
        let len = std::fs::metadata(&target).unwrap().len();
        assert!(len > 1_000_000);
        eprintln!(
            "installed v{} ({len} bytes, {} progress events)",
            info.version,
            seen.len()
        );
    }

    #[test]
    fn nightly_channel_picks_highest_release() {
        let stable = Version::parse("0.1.2").unwrap();
        let list = r#"[
            {"tag_name": "v0.1.3-nightly.202611200900", "prerelease": true, "draft": false},
            {"tag_name": "v0.1.3-nightly.202611211200", "prerelease": true, "draft": false,
             "assets": [{"name": "keyframexl-linux-x86_64.tar.gz", "browser_download_url": "https://example.com/n"}]},
            {"tag_name": "v0.1.4-nightly.1", "prerelease": true, "draft": true},
            {"tag_name": "v0.1.2", "prerelease": false, "draft": false}
        ]"#;
        let UpdateOutcome::Available(info) = parse_releases_list_json(list, &stable, true).unwrap()
        else {
            panic!("nightly expected");
        };
        assert_eq!(info.version, "0.1.3-nightly.202611211200");
        assert_eq!(
            info.linux_asset_url.as_deref(),
            Some("https://example.com/n")
        );
        // Stable channel ignores nightlies.
        assert_eq!(
            parse_releases_list_json(list, &stable, false).unwrap(),
            UpdateOutcome::UpToDate {
                latest: Some("0.1.2".into())
            }
        );
        // A nightly moves on to the matching stable release.
        let n = Version::parse("0.1.3-nightly.202611211200").unwrap();
        assert!(Version::parse("0.1.3").unwrap() > n);
        assert!(Version::parse("0.1.3-nightly.202611211201").unwrap() > n);
        assert_eq!(
            parse_releases_list_json(list, &n, true).unwrap(),
            UpdateOutcome::UpToDate {
                latest: Some("0.1.3-nightly.202611211200".into())
            }
        );
    }

    #[test]
    fn semver_parsing_and_ordering() {
        let v010 = Version::parse("0.1.0").unwrap();
        let v020 = Version::parse("v0.2.0").unwrap();
        let v0100 = Version::parse("V0.10.0+abc").unwrap();
        let v020_rc1 = Version::parse("0.2.0-rc.1").unwrap();
        let v020_rc2 = Version::parse("0.2.0-rc.2").unwrap();
        let v020_rc10 = Version::parse("0.2.0-rc.10").unwrap();

        assert!(v010 < v020_rc1);
        assert!(v020_rc1 < v020_rc2);
        assert!(v020_rc2 < v020_rc10);
        assert!(v020_rc10 < v020);
        assert!(v020 < v0100);
        assert_eq!(v020.to_string(), "0.2.0");
        assert_eq!(v020_rc1.to_string(), "0.2.0-rc.1");

        assert!(Version::parse("not-a-version").is_none());
        assert!(Version::parse("1.2").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
        assert!(Version::parse("1.2.3-").is_none());
    }

    #[test]
    fn parse_release_json_detects_newer_and_picks_linux_asset() {
        let cur = Version::parse("0.1.0").unwrap();
        let json = r#"{
            "tag_name": "v0.2.0",
            "name": "KeyFrameXL 0.2.0",
            "body": "Added project bundles and MIDI Clock.",
            "html_url": "https://github.com/mallardask-ops/keyframexl/releases/tag/v0.2.0",
            "published_at": "2026-06-01T12:00:00Z",
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": "keyframexl-0.2.0-linux-x86_64.tar.gz", "browser_download_url": "https://example.com/tar"},
                {"name": "KeyFrameXL-0.2.0-x86_64.AppImage", "browser_download_url": "https://example.com/appimage"}
            ]
        }"#;
        let out = parse_release_json(json, &cur).unwrap();
        match out {
            UpdateOutcome::Available(info) => {
                assert_eq!(info.version, "0.2.0");
                assert_eq!(info.tag, "v0.2.0");
                assert_eq!(
                    info.linux_asset_url.as_deref(),
                    Some("https://example.com/appimage")
                );
                assert_eq!(info.published_at.as_deref(), Some("2026-06-01"));
            }
            other => panic!("expected Available, got {other:?}"),
        }

        // Same version => UpToDate
        let cur2 = Version::parse("0.2.0").unwrap();
        assert_eq!(
            parse_release_json(json, &cur2).unwrap(),
            UpdateOutcome::UpToDate {
                latest: Some("0.2.0".into())
            }
        );

        // Prerelease ignored for stable builds
        let pre = r#"{"tag_name":"v0.3.0-beta.1","prerelease":true,"draft":false}"#;
        assert_eq!(
            parse_release_json(pre, &cur).unwrap(),
            UpdateOutcome::UpToDate { latest: None }
        );
    }

    #[test]
    fn parse_tags_json_fallback_finds_highest_stable() {
        let cur = Version::parse("0.1.0").unwrap();
        let tags = r#"[
            {"name": "v0.1.0"},
            {"name": "v0.3.0-alpha.1"},
            {"name": "v0.2.5"},
            {"name": "nightly"}
        ]"#;
        match parse_tags_json(tags, &cur).unwrap() {
            UpdateOutcome::Available(info) => {
                assert_eq!(info.version, "0.2.5");
                assert_eq!(info.tag, "v0.2.5");
            }
            other => panic!("expected Available, got {other:?}"),
        }
        assert_eq!(
            parse_tags_json("[]", &cur).unwrap(),
            UpdateOutcome::UpToDate { latest: None }
        );
    }

    #[test]
    fn auto_check_interval_and_skip_matching() {
        let now = 1_700_100_000;
        assert!(should_auto_check_inner(true, 0, now, false));
        assert!(!should_auto_check_inner(true, 0, now, true));
        assert!(!should_auto_check_inner(false, 0, now, false));
        assert!(!should_auto_check_inner(true, now - 3600, now, false));
        assert!(should_auto_check_inner(
            true,
            now - AUTO_CHECK_INTERVAL_SECS,
            now,
            false
        ));

        assert!(is_skipped(Some("0.2.0"), "v0.2.0"));
        assert!(is_skipped(Some("v0.2.0"), "0.2.0"));
        assert!(!is_skipped(Some("0.2.0"), "0.2.1"));
        assert!(!is_skipped(None, "0.2.0"));
    }
}
