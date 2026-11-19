//! Voicebanks on disk: where they live, installing one from a zip (UTAU zips
//! name files in Shift-JIS), and fetching Kasane Teto from her official site.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Official download of the standard Kasane Teto UTAU voicebank (single notes).
/// Free for non-commercial use under the terms in its readme; fetched on
/// request, never bundled.
pub const TETO_URL: &str = "https://kasaneteto.jp/assets/download/utau/TETO-tandoku-100619.zip";
pub const TETO_TERMS: &str = "https://kasaneteto.jp/guideline/voice.html";

pub fn bank_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("keyframexl")
        .join("voicebanks")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub name: String,
    pub path: PathBuf,
}

/// Banks under the voicebank folder: every sub-folder that holds an `oto.ini`.
pub fn installed() -> Vec<Installed> {
    let root = bank_root();
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut v: Vec<Installed> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && has_oto(p, 0))
        .map(|p| Installed {
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: p,
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn has_oto(dir: &Path, depth: usize) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_file()
            && p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("oto.ini"))
        {
            return true;
        }
        if depth < 6 && p.is_dir() && has_oto(&p, depth + 1) {
            return true;
        }
    }
    false
}

fn entry_name(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(raw).0.into_owned(),
    }
}

/// Unpack a UTAU zip into `<bank root>/<name>`; returns that folder.
pub fn install_zip(zip_path: &Path, name: &str) -> Result<PathBuf, String> {
    let f = std::fs::File::open(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("not a zip: {e}"))?;
    let dest = bank_root().join(sanitize(name));
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut file = z.by_index(i).map_err(|e| e.to_string())?;
        let rel = entry_name(file.name_raw());
        // A zip must not write outside its folder.
        let rel_path = Path::new(&rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        let out = dest.join(rel_path);
        if file.is_dir() || rel.ends_with('/') {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::File::create(&out)
            .and_then(|mut o| o.write_all(&buf))
            .map_err(|e| format!("{}: {e}", out.display()))?;
    }
    if !has_oto(&dest, 0) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err("that zip holds no UTAU voicebank (no oto.ini)".into());
    }
    Ok(dest)
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "voicebank".into() } else { s }
}

/// Download Kasane Teto from her site and install it.
pub fn download_teto() -> Result<PathBuf, String> {
    let resp = ureq::get(TETO_URL)
        .timeout(std::time::Duration::from_secs(300))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(200 << 20)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("download failed: {e}"))?;
    let tmp = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile()
        .map_err(|e| e.to_string())?;
    std::fs::write(tmp.path(), &bytes).map_err(|e| e.to_string())?;
    install_zip(tmp.path(), "Kasane Teto")
}
