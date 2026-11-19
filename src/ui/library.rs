//! Sound libraries already on this computer: FL Studio's stock packs (inside
//! a Wine prefix) and folders of DirectWave presets (.dwp, e.g. the SGM General
//! MIDI set). Found libraries join the Files browser, and a new project's
//! channels load real sounds from them instead of the built-in test synths.

use super::App;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Libraries {
    /// FL Studio `Data/Patches/Packs` (newest version first).
    pub fl_packs: Option<PathBuf>,
    /// Folders that hold .dwp presets within two levels.
    pub dwp: Vec<PathBuf>,
}

fn has_dwp(dir: &Path, depth: usize) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwp")) {
            return true;
        }
        if depth > 0 && p.is_dir() && has_dwp(&p, depth - 1) {
            return true;
        }
    }
    false
}

fn children(dir: &Path) -> Vec<PathBuf> {
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

/// Look in the usual places: Wine prefixes in home, mounted disks, the music folder.
pub fn find(home: &Path, mounts: &[PathBuf]) -> Libraries {
    let mut libs = Libraries::default();
    let mut packs: Vec<PathBuf> = Vec::new();
    // Wine prefixes are hidden folders (.wine, .wine-fl …), so read home directly.
    let prefixes: Vec<PathBuf> = std::fs::read_dir(home)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with(".wine"))
                })
                .collect()
        })
        .unwrap_or_default();
    for prefix in prefixes {
        let il = prefix.join("drive_c/Program Files/Image-Line");
        for fl in children(&il).into_iter().filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("FL Studio"))
        }) {
            let p = fl.join("Data/Patches/Packs");
            if p.join("Drums").is_dir() {
                packs.push(p);
            }
        }
    }
    // Newest version wins: compare the number after "FL Studio" (2025 > 21).
    let version = |p: &PathBuf| -> u32 {
        p.ancestors()
            .filter_map(|a| a.file_name())
            .find_map(|n| {
                n.to_string_lossy()
                    .strip_prefix("FL Studio ")
                    .and_then(|v| v.split_whitespace().next()?.parse().ok())
            })
            .unwrap_or(0)
    };
    packs.sort_by_key(version);
    libs.fl_packs = packs.pop();
    for base in mounts {
        for dir in children(base) {
            if has_dwp(&dir, 2) {
                libs.dwp.push(dir);
            }
        }
    }
    libs
}

/// Mount points worth a look: /mnt/*, /media/$USER/*, /run/media/$USER/*, the music folder.
pub fn default_mounts(home: &Path) -> Vec<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let mut v = children(Path::new("/mnt"));
    v.extend(children(&Path::new("/media").join(&user)));
    v.extend(children(&Path::new("/run/media").join(&user)));
    v.extend(dirs::audio_dir());
    v.push(home.join("Music"));
    v.dedup();
    v
}

/// The first file in `dir` whose name contains every word, preferring `first`.
fn pick(dir: &Path, first: &str, words: &[&str]) -> Option<PathBuf> {
    let exact = dir.join(first);
    if exact.is_file() {
        return Some(exact);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files.into_iter().find(|p| {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        words.iter().all(|w| n.contains(w)) && super::files::is_audio(p)
    })
}

/// What a new project loads: (channel name, file).
pub fn default_kit(libs: &Libraries) -> Vec<(&'static str, PathBuf)> {
    let mut kit = Vec::new();
    if let Some(packs) = &libs.fl_packs {
        let drums = packs.join("Drums");
        for (name, dir, file, words) in [
            ("Kick", "Kicks", "909 Kick.wav", &["kick"][..]),
            ("Snare", "Snares", "909 Snare.wav", &["snare"][..]),
            ("Hat", "Hats", "909 CH.wav", &["ch"][..]),
        ] {
            if let Some(p) = pick(&drums.join(dir), file, words) {
                kit.push((name, p));
            }
        }
    }
    for lib in &libs.dwp {
        let bass = ["Bass/Fingered Bass.dwp", "Bass/Acoustic Bass.dwp"]
            .iter()
            .map(|f| lib.join(f))
            .find(|p| p.is_file());
        if let Some(p) = bass {
            kit.push(("Bass", p));
            break;
        }
    }
    kit
}

impl App {
    /// Load real sounds into the matching channels of the current project,
    /// quietly (the project stays unchanged for undo and saving).
    pub(crate) fn load_default_kit(&mut self) {
        if !self.settings.general.default_kit {
            return;
        }
        let kit = default_kit(&self.libraries);
        for (name, path) in kit {
            if let Some(id) = self
                .song
                .instruments
                .iter()
                .find(|i| i.name == name)
                .map(|i| i.id)
            {
                // FL's new-channel level (78%): full-scale stock samples leave headroom.
                if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == id) {
                    i.volume = 0.78;
                }
                self.load_preset_with(id, path, true);
            }
        }
    }

    /// Found libraries join the Files browser once.
    pub(crate) fn add_library_roots(&mut self) {
        let mut roots: Vec<PathBuf> = self.libraries.fl_packs.iter().cloned().collect();
        roots.extend(self.libraries.dwp.iter().cloned());
        for r in roots {
            self.files.add_root(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_fl_packs_and_dwp_folders_and_builds_a_kit() {
        let home = tempfile::tempdir().unwrap();
        let disk = tempfile::tempdir().unwrap();
        for v in ["FL Studio 21", "FL Studio 2025"] {
            let drums = home
                .path()
                .join(".wine/drive_c/Program Files/Image-Line")
                .join(v)
                .join("Data/Patches/Packs/Drums");
            for (d, f) in [
                ("Kicks", "909 Kick.wav"),
                ("Snares", "707 Snare.wav"),
                ("Hats", "808 CH.wav"),
            ] {
                std::fs::create_dir_all(drums.join(d)).unwrap();
                std::fs::write(drums.join(d).join(f), b"RIFF").unwrap();
            }
        }
        let sgm = disk.path().join("SGM set");
        std::fs::create_dir_all(sgm.join("Bass")).unwrap();
        std::fs::write(sgm.join("Bass/Fingered Bass.dwp"), b"x").unwrap();
        std::fs::create_dir_all(disk.path().join("Photos")).unwrap();
        let libs = find(home.path(), &[disk.path().to_path_buf()]);
        assert!(
            libs.fl_packs
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .contains("FL Studio 2025"),
            "the newest FL wins"
        );
        assert_eq!(libs.dwp, vec![sgm.clone()], "only folders with presets");
        let kit = default_kit(&libs);
        let names: Vec<(&str, String)> = kit
            .iter()
            .map(|(n, p)| (*n, p.file_name().unwrap().to_string_lossy().to_string()))
            .collect();
        assert_eq!(
            names,
            [
                ("Kick", "909 Kick.wav".into()),
                ("Snare", "707 Snare.wav".into()),
                ("Hat", "808 CH.wav".into()),
                ("Bass", "Fingered Bass.dwp".into())
            ],
            "exact names first, else the first matching file"
        );
        assert_eq!(
            find(disk.path(), &[]),
            Libraries::default(),
            "nothing found, nothing changes"
        );
    }
}

/// Drum kit presets in the .dwp libraries: files named "... (drumkit)" or
/// kept in a "drum kits" folder (the SGM set's DrumKits).
pub fn drum_kits(libs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for lib in libs {
        let mut stack = vec![(lib.clone(), 0usize)];
        while let Some((d, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for p in rd.flatten().map(|e| e.path()) {
                if p.is_dir() {
                    if depth < 4 {
                        stack.push((p, depth + 1));
                    }
                    continue;
                }
                if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwp")) {
                    continue;
                }
                let squash = |s: &str| s.to_lowercase().replace([' ', '_', '-'], "");
                let stem = squash(&p.file_stem().unwrap_or_default().to_string_lossy());
                let folder = squash(
                    &p.parent()
                        .and_then(|f| f.file_name())
                        .unwrap_or_default()
                        .to_string_lossy(),
                );
                if stem.contains("drumkit") || folder.contains("drumkit") {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The kit for a GM kit name ("Standard", "TR-808", "Room" ...): the file
/// whose name holds it, "Standard" kits preferring the first (Standard 1);
/// else the first Standard kit, else any kit.
pub fn pick_kit<'a>(kits: &'a [PathBuf], name: &str) -> Option<&'a PathBuf> {
    let squash = |s: &str| s.to_lowercase().replace([' ', '_', '-'], "");
    let want = squash(name);
    let stem = |p: &PathBuf| squash(&p.file_stem().unwrap_or_default().to_string_lossy());
    kits.iter()
        .find(|p| stem(p).contains(&want))
        .or_else(|| kits.iter().find(|p| stem(p).contains("standard")))
        .or_else(|| kits.first())
}

#[cfg(test)]
mod kit_check {
    /// `cargo test --release --bin keyframexl kit_check -- --ignored --nocapture`:
    /// the .dwp libraries and drum kits this computer has.
    #[test]
    #[ignore]
    fn lists_the_drum_kits_found_here() {
        let home = dirs::home_dir().unwrap();
        let libs = super::find(&home, &super::default_mounts(&home));
        println!("dwp libraries: {:?}", libs.dwp);
        let kits = super::drum_kits(&libs.dwp);
        println!("{} kits", kits.len());
        for name in ["Standard", "Room", "TR-808", "Jazz", "Brush"] {
            println!(
                "{name} → {:?}",
                super::pick_kit(&kits, name).and_then(|p| p.file_name())
            );
        }
    }
}
