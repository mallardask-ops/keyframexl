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
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
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
        let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
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
    pub published_at: Option<String>,
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
    UpToDate { latest: Option<String> },
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
fn should_auto_check_inner(enabled: bool, last_check: u64, now_secs: u64, env_disabled: bool) -> bool {
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

/// Pick the most suitable Linux release asset URL, if any.
fn pick_linux_asset(assets: &[serde_json::Value]) -> Option<String> {
    let mut fallback = None;
    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(url) = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".appimage") {
            return Some(url.to_string());
        }
        if fallback.is_none()
            && (lower.contains("linux") || lower.ends_with(".tar.gz") || lower.ends_with(".tar.xz"))
        {
            fallback = Some(url.to_string());
        }
    }
    fallback
}

/// Evaluate a GitHub `/releases/latest` JSON payload against `current`.
pub fn parse_release_json(body: &str, current: &Version) -> Result<UpdateOutcome, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid release response: {e}"))?;
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
    if is_pre && !current.is_prerelease() {
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
    let linux_asset_url = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|arr| pick_linux_asset(arr));
    Ok(UpdateOutcome::Available(ReleaseInfo {
        version: remote.to_string(),
        tag: tag.to_string(),
        title,
        notes,
        html_url,
        linux_asset_url,
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
            published_at: None,
        })),
        Some((ver, _)) => Ok(UpdateOutcome::UpToDate {
            latest: Some(ver.to_string()),
        }),
        None => Ok(UpdateOutcome::UpToDate { latest: None }),
    }
}

/// Perform a blocking check against GitHub (call only from a background thread).
pub fn fetch_latest() -> Result<UpdateOutcome, String> {
    let current =
        Version::parse(CURRENT_VERSION).ok_or_else(|| format!("Invalid build version: {CURRENT_VERSION}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(6))
        .build();
    let ua = format!("KeyFrameXL/{CURRENT_VERSION}");
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

/// Non-blocking update checker owned by `App`.
#[derive(Default)]
pub struct UpdateChecker {
    pub state: CheckState,
    pub dialog_open: bool,
    pub checked_on_startup: bool,
    manual: bool,
    rx: Option<mpsc::Receiver<Result<UpdateOutcome, String>>>,
}

impl UpdateChecker {
    /// Start a background check if one is not already running.
    pub fn start(&mut self, ctx: Option<eframe::egui::Context>, manual: bool) {
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
                let res = fetch_latest();
                let _ = tx.send(res);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .ok();
    }

    /// Poll the background worker without blocking. Returns `true` when a check just finished.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
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
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                if self.manual {
                    self.state = CheckState::Error("Update check was interrupted.".into());
                } else {
                    self.state = CheckState::Idle;
                }
                true
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
