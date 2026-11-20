//! Application build version and release channel (`stable` vs `nightly`).
//!
//! Nightly builds pass `KFXL_BUILD_VERSION` / `KFXL_VERSION` (e.g.
//! `0.1.3-nightly.202611201530`) and `KFXL_CHANNEL=nightly` at compile time;
//! stable and local builds fall back to `CARGO_PKG_VERSION` and `stable`
//! (unless the version itself contains `-nightly`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Nightly,
}

impl Channel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Nightly => "Nightly",
        }
    }

    pub const fn is_nightly(self) -> bool {
        matches!(self, Self::Nightly)
    }
}

const fn strip_v_const(s: &str) -> &str {
    match s.as_bytes() {
        [b'v' | b'V', ..] => s.split_at(1).1,
        _ => s,
    }
}

const fn eq_ignore_ascii_case(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut i = 0;
    while i < ab.len() {
        if !ab[i].eq_ignore_ascii_case(&bb[i]) {
            return false;
        }
        i += 1;
    }
    true
}

const fn contains_nightly(s: &str) -> bool {
    let needle = b"-nightly";
    let hay = s.as_bytes();
    if hay.len() < needle.len() {
        return false;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let mut j = 0;
        let mut matched = true;
        while j < needle.len() {
            if !hay[i + j].eq_ignore_ascii_case(&needle[j]) {
                matched = false;
                break;
            }
            j += 1;
        }
        if matched {
            return true;
        }
        i += 1;
    }
    false
}

/// Resolve the display/updater version from build-time env vars or `CARGO_PKG_VERSION`.
pub const fn resolve_version<'a>(
    kfxl_build_version: Option<&'a str>,
    kfxl_version: Option<&'a str>,
    pkg_version: &'a str,
) -> &'a str {
    if let Some(v) = kfxl_build_version {
        let v = strip_v_const(v.trim_ascii());
        if !v.is_empty() {
            return v;
        }
    }
    if let Some(v) = kfxl_version {
        let v = strip_v_const(v.trim_ascii());
        if !v.is_empty() {
            return v;
        }
    }
    strip_v_const(pkg_version.trim_ascii())
}

/// Resolve the release channel from `KFXL_CHANNEL` or the version string.
pub const fn resolve_channel(kfxl_channel: Option<&str>, version: &str) -> Channel {
    if let Some(ch) = kfxl_channel {
        let ch = ch.trim_ascii();
        if eq_ignore_ascii_case(ch, "nightly") {
            return Channel::Nightly;
        }
        if eq_ignore_ascii_case(ch, "stable") {
            return Channel::Stable;
        }
    }
    if contains_nightly(version) {
        Channel::Nightly
    } else {
        Channel::Stable
    }
}

/// Effective application version without a leading `v`.
pub const VERSION: &str = resolve_version(
    option_env!("KFXL_BUILD_VERSION"),
    option_env!("KFXL_VERSION"),
    env!("CARGO_PKG_VERSION"),
);

/// Release channel for this build.
pub const CHANNEL: Channel = resolve_channel(option_env!("KFXL_CHANNEL"), VERSION);

pub const fn is_nightly() -> bool {
    CHANNEL.is_nightly()
}

pub fn format_display(version: &str, channel: Channel) -> String {
    let v = version.trim().trim_start_matches(['v', 'V']);
    match channel {
        Channel::Stable => format!("v{v}"),
        Channel::Nightly => format!("v{v} · Nightly"),
    }
}

/// Full display string, e.g. `"v0.1.2"` or `"v0.1.3-nightly.202611201530 · Nightly"`.
pub fn display() -> String {
    format_display(VERSION, CHANNEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_version_prefers_build_env_and_strips_v_prefix() {
        assert_eq!(resolve_version(None, None, "0.1.2"), "0.1.2");
        assert_eq!(
            resolve_version(Some("0.1.3-nightly.202611201530"), None, "0.1.2"),
            "0.1.3-nightly.202611201530"
        );
        assert_eq!(
            resolve_version(None, Some("v0.2.0-nightly.20261120"), "0.1.2"),
            "0.2.0-nightly.20261120"
        );
        assert_eq!(resolve_version(Some(""), Some("   "), "0.1.2"), "0.1.2");
        assert_eq!(resolve_version(Some("v"), None, "0.1.2"), "0.1.2");
    }

    #[test]
    fn resolve_channel_reads_env_or_infers_from_version() {
        assert_eq!(resolve_channel(None, "0.1.2"), Channel::Stable);
        assert_eq!(
            resolve_channel(None, "0.1.3-nightly.202611201530"),
            Channel::Nightly
        );
        assert_eq!(resolve_channel(Some("nightly"), "0.2.0"), Channel::Nightly);
        assert_eq!(resolve_channel(Some("NIGHTLY"), "0.2.0"), Channel::Nightly);
        assert_eq!(
            resolve_channel(Some("stable"), "0.2.0-nightly.1"),
            Channel::Stable
        );
        assert_eq!(
            resolve_channel(Some("unknown"), "0.2.0-nightly.1"),
            Channel::Nightly
        );
        assert_eq!(resolve_channel(Some("unknown"), "0.2.0"), Channel::Stable);
    }

    #[test]
    fn format_display_adds_nightly_marker_when_nightly() {
        assert_eq!(format_display("0.1.2", Channel::Stable), "v0.1.2");
        assert_eq!(format_display("v0.1.2", Channel::Stable), "v0.1.2");
        assert_eq!(
            format_display("0.2.0-nightly.20261120", Channel::Nightly),
            "v0.2.0-nightly.20261120 · Nightly"
        );
    }
}
