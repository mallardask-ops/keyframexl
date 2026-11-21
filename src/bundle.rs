//! Project packaging ("Collect all & save"): one zip holding the project,
//! its external impulse responses and — optionally — its voicebanks, plus a
//! manifest and a README. Importing unpacks such a bundle into a working
//! folder and rewires the references to the extracted files. Plugin binaries
//! are never included; their state already travels inside the project.

use crate::fx::FxKind;
use crate::project_io;
use crate::sequence::{InstrumentKind, PluginRef, Song};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub const BUNDLE_FORMAT: u32 = 1;

/// Safety cap on the files taken from one voicebank: banks can be huge.
const MAX_BANK_FILES: usize = 5000;

#[derive(Clone, Debug, Default)]
pub struct BundleOptions {
    /// UTAU voicebanks are large and licensed by their authors, so they stay
    /// out of the bundle unless this is set.
    pub include_voicebanks: bool,
}

/// One convolution slot's impulse response as stored in the archive. Slots
/// sharing a file share its `archive_path`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BundledIr {
    pub track: usize,
    pub slot: usize,
    pub archive_path: String,
    pub original_name: String,
}

/// A hosted plugin the project needs on the target computer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginEntry {
    pub where_used: String,
    pub format: String,
    pub name: String,
    pub vendor: String,
    pub path: String,
    pub has_state: bool,
}

/// A vocal channel's voicebank: bundled, or left as a path reference.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VoicebankEntry {
    pub channel: String,
    pub bank: String,
    pub bundled: bool,
}

/// The bundle's inventory (`manifest.json`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BundleManifest {
    pub format: u32,
    pub app: String,
    pub app_version: String,
    pub project_file: String,
    pub irs: Vec<BundledIr>,
    /// IR paths that did not exist (or were unreadable) at export time.
    pub missing_irs: Vec<String>,
    pub plugins: Vec<PluginEntry>,
    pub voicebanks: Vec<VoicebankEntry>,
}

#[derive(Clone, Debug)]
pub struct ExportReport {
    pub manifest: BundleManifest,
    /// Distinct IR files stored in the archive (shared files count once).
    pub bundled_irs: usize,
    pub missing_irs: Vec<String>,
    pub plugins: usize,
    pub voicebanks: usize,
}

#[derive(Clone, Debug)]
pub struct ImportedBundle {
    pub song: Song,
    /// The unpacked, rewired project, ready to open and save normally.
    pub project_path: PathBuf,
    pub manifest: Option<BundleManifest>,
    pub restored_irs: usize,
}

/// Whether the bytes start with a zip local file header.
pub fn is_bundle_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// Collect the project and its external files into one zip at `dest_zip`.
/// Missing impulse responses never fail the export: they are listed in the
/// manifest and README, and the project keeps its reference to them.
pub fn export_bundle(
    song: &Song,
    project_stem: &str,
    dest_zip: &Path,
    opts: &BundleOptions,
) -> Result<ExportReport, String> {
    song.validate().map_err(str::to_owned)?;
    let mut bundled_song = song.clone();
    let project_file = format!("{}.kfxl", sanitize_stem(project_stem));
    let mut manifest = BundleManifest {
        format: BUNDLE_FORMAT,
        app: project_io::APP_ID.to_string(),
        app_version: crate::version::VERSION.to_string(),
        project_file: project_file.clone(),
        irs: vec![],
        missing_irs: vec![],
        plugins: vec![],
        voicebanks: vec![],
    };

    // Impulse responses: one archive copy per distinct file, every slot that
    // uses it rewritten to that copy. Bytes stream into the zip below.
    let mut ir_files: Vec<(String, PathBuf)> = Vec::new();
    let mut ir_by_source: HashMap<PathBuf, String> = HashMap::new();
    let mut missing_seen: HashSet<String> = HashSet::new();
    for (t, track) in bundled_song.mixer.iter_mut().enumerate() {
        for (s, slot) in track.fx.iter_mut().enumerate() {
            let Some(ir) = slot.ir.clone() else { continue };
            let display = ir.display().to_string();
            let key = std::fs::canonicalize(&ir).unwrap_or_else(|_| ir.clone());
            if let Some(archive_path) = ir_by_source.get(&key) {
                manifest.irs.push(BundledIr {
                    track: t,
                    slot: s,
                    archive_path: archive_path.clone(),
                    original_name: display,
                });
                slot.ir = Some(PathBuf::from(archive_path));
                continue;
            }
            let readable = std::fs::File::open(&ir)
                .ok()
                .filter(|f| f.metadata().is_ok_and(|m| m.is_file()));
            if readable.is_none() {
                if missing_seen.insert(display.clone()) {
                    manifest.missing_irs.push(display);
                }
                continue;
            }
            let file_name = ir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let archive_path = format!("ir/{}_{}", ir_files.len(), safe_name(file_name, "ir.wav"));
            ir_by_source.insert(key, archive_path.clone());
            ir_files.push((archive_path.clone(), ir));
            manifest.irs.push(BundledIr {
                track: t,
                slot: s,
                archive_path: archive_path.clone(),
                original_name: display,
            });
            slot.ir = Some(PathBuf::from(archive_path));
        }
    }

    // Hosted plugins: channels first, then mixer slots.
    for inst in &song.instruments {
        if let InstrumentKind::Plugin { plugin } = &inst.kind {
            manifest
                .plugins
                .push(plugin_entry(plugin, format!("channel \"{}\"", inst.name)));
        }
    }
    for track in &song.mixer {
        for (s, slot) in track.fx.iter().enumerate() {
            if slot.kind == FxKind::Plugin
                && let Some(plugin) = &slot.plugin
            {
                manifest.plugins.push(plugin_entry(
                    plugin,
                    format!("mixer \"{}\" slot {}", track.name, s),
                ));
            }
        }
    }

    // Voicebanks: only with the option on, and only when the folder exists.
    let mut bank_files: Vec<(String, PathBuf)> = Vec::new();
    let mut bank_dirs: HashMap<PathBuf, String> = HashMap::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for inst in &mut bundled_song.instruments {
        let InstrumentKind::Vocal { voice } = &mut inst.kind else {
            continue;
        };
        let mut entry = VoicebankEntry {
            channel: inst.name.clone(),
            bank: voice.bank.clone(),
            bundled: false,
        };
        let root = PathBuf::from(&voice.bank);
        if opts.include_voicebanks && root.is_dir() {
            let key = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
            if let Some(dir) = bank_dirs.get(&key) {
                entry.bundled = true;
                voice.bank = dir.clone();
            } else {
                let base = safe_name(
                    root.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                    "voicebank",
                );
                let mut name = base.clone();
                let mut n = 2;
                while !used_names.insert(name.clone()) {
                    name = format!("{base}_{n}");
                    n += 1;
                }
                let dir = format!("voicebanks/{name}");
                let files = collect_bank_files(&root, &dir);
                if !files.is_empty() {
                    bank_dirs.insert(key, dir.clone());
                    bank_files.extend(files);
                    entry.bundled = true;
                    voice.bank = dir;
                }
            }
        }
        manifest.voicebanks.push(entry);
    }

    let project_bytes = project_io::to_bytes(&bundled_song)?;
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let readme = readme_text(&manifest, opts);

    // Atomic, like project_io::save: write a temp file beside the target,
    // then rename it over.
    let parent = dest_zip
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(temp.as_file_mut());
    write_entry(&mut zip, &project_file, &project_bytes)?;
    write_entry(&mut zip, "manifest.json", &manifest_json)?;
    write_entry(&mut zip, "README.txt", readme.as_bytes())?;
    for (name, source) in &ir_files {
        copy_entry(&mut zip, name, source)?;
    }
    for (name, source) in &bank_files {
        copy_entry(&mut zip, name, source)?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(dest_zip).map_err(|e| e.to_string())?;
    std::fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| e.to_string())?;

    Ok(ExportReport {
        bundled_irs: ir_files.len(),
        missing_irs: manifest.missing_irs.clone(),
        plugins: manifest.plugins.len(),
        voicebanks: manifest.voicebanks.len(),
        manifest,
    })
}

/// Unpack a bundle into `extract_dir`: impulse responses and voicebanks are
/// extracted, the project's references are rewired to them, and the restored
/// project is saved as a normal `.kfxl` beside them.
pub fn import_bundle(zip_path: &Path, extract_dir: &Path) -> Result<ImportedBundle, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Not a zip bundle: {e}"))?;

    // Validate every entry before anything touches the disk.
    let mut entries: Vec<(usize, PathBuf, bool)> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = safe_entry_path(&file)?;
        entries.push((i, rel, file.is_dir()));
    }

    // The manifest is optional: a hand-made zip with just a .kfxl works too.
    let manifest = match archive.by_name("manifest.json") {
        Ok(mut file) => {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            let manifest: BundleManifest =
                serde_json::from_slice(&buf).map_err(|e| format!("Bad bundle manifest: {e}"))?;
            if manifest.format > BUNDLE_FORMAT {
                return Err("Bundle is from a newer KeyFrameXL".into());
            }
            Some(manifest)
        }
        Err(_) => None,
    };

    // The project: the manifest's file, else the first root-level .kfxl.
    let project_rel = manifest
        .as_ref()
        .and_then(|m| {
            entries
                .iter()
                .find(|(_, rel, dir)| !dir && rel.as_path() == Path::new(&m.project_file))
        })
        .or_else(|| {
            entries.iter().find(|(_, rel, dir)| {
                !dir && rel.parent().is_some_and(|p| p.as_os_str().is_empty())
                    && rel
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("kfxl"))
            })
        })
        .map(|(_, rel, _)| rel.clone())
        .ok_or("Bundle holds no project (.kfxl) file")?;

    std::fs::create_dir_all(extract_dir).map_err(|e| e.to_string())?;
    for (i, rel, is_dir) in &entries {
        if !(rel.starts_with("ir") || rel.starts_with("voicebanks")) {
            continue;
        }
        let out = extract_dir.join(rel);
        if *is_dir {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut src = archive.by_index(*i).map_err(|e| e.to_string())?;
        let mut dst = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::io::copy(&mut src, &mut dst).map_err(|e| format!("{}: {e}", out.display()))?;
    }

    let mut bytes = Vec::new();
    archive
        .by_name(&project_rel.to_string_lossy())
        .map_err(|e| e.to_string())?
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let mut song = project_io::parse(&bytes)?;

    // Point bundled references at the extracted files; missing ones keep
    // their original path.
    let mut restored_irs = 0;
    for track in &mut song.mixer {
        for slot in &mut track.fx {
            let Some(ir) = &slot.ir else { continue };
            if !ir.starts_with("ir") {
                continue;
            }
            let joined = extract_dir.join(ir);
            if joined.is_file() {
                slot.ir = Some(joined);
                restored_irs += 1;
            }
        }
    }
    for inst in &mut song.instruments {
        if let InstrumentKind::Vocal { voice } = &mut inst.kind
            && Path::new(&voice.bank).starts_with("voicebanks")
        {
            let joined = extract_dir.join(&voice.bank);
            if joined.is_dir() {
                voice.bank = joined.to_string_lossy().into_owned();
            }
        }
    }

    let project_path = extract_dir.join(&project_rel);
    project_io::save(&song, &project_path)?;
    Ok(ImportedBundle {
        song,
        project_path,
        manifest,
        restored_irs,
    })
}

/// A stem safe on any filesystem: ASCII letters, digits, `-` and `_`, with
/// anything else folded to `_` and the edges trimmed. Empty results fall
/// back to "project".
fn sanitize_stem(stem: &str) -> String {
    let cleaned = ascii_name(stem, false);
    if cleaned.is_empty() {
        "project".into()
    } else {
        cleaned
    }
}

/// A safe file or folder name; dots survive when `keep_dot` (extensions).
fn safe_name(name: &str, fallback: &str) -> String {
    let cleaned = ascii_name(name, true);
    if cleaned.is_empty() {
        fallback.into()
    } else {
        cleaned
    }
}

fn ascii_name(name: &str, keep_dot: bool) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || (keep_dot && c == '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    cleaned
        .trim_matches(|c| c == '_' || c == '.')
        .chars()
        .take(80)
        .collect()
}

fn plugin_entry(plugin: &PluginRef, where_used: String) -> PluginEntry {
    PluginEntry {
        where_used,
        format: plugin.format.clone(),
        name: plugin.name.clone(),
        vendor: plugin.vendor.clone(),
        path: plugin.path.clone(),
        has_state: !plugin.state.is_empty(),
    }
}

/// The bank's files as (archive path, source path) pairs: hidden entries and
/// symlinks stay out, and at most `MAX_BANK_FILES` files are taken.
fn collect_bank_files(root: &Path, archive_dir: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(|e| e.ok()) {
            if out.len() >= MAX_BANK_FILES {
                return out;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                stack.push((e.path(), rel));
            } else if kind.is_file() {
                out.push((format!("{archive_dir}/{rel}"), e.path()));
            }
        }
    }
    out
}

fn entry_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn write_entry<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    zip.start_file(name, entry_options())
        .map_err(|e| e.to_string())?;
    zip.write_all(bytes).map_err(|e| e.to_string())
}

fn copy_entry<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    source: &Path,
) -> Result<(), String> {
    zip.start_file(name, entry_options())
        .map_err(|e| e.to_string())?;
    let mut src = std::fs::File::open(source).map_err(|e| format!("{}: {e}", source.display()))?;
    std::io::copy(&mut src, zip).map_err(|e| format!("{}: {e}", source.display()))?;
    Ok(())
}

/// An entry's path relative to the extraction root. `enclosed_name` rejects
/// absolute paths and escaping `..`; backslashes and any internal `..` are
/// refused on top, so nothing can write outside the folder.
fn safe_entry_path<R: Read + Seek>(file: &zip::read::ZipFile<'_, R>) -> Result<PathBuf, String> {
    let bad = || format!("Unsafe path in bundle: {}", file.name());
    if file.name().contains('\\') {
        return Err(bad());
    }
    let path = file.enclosed_name().ok_or_else(bad)?;
    let mut rel = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(part) => rel.push(part),
            Component::CurDir => {}
            _ => return Err(bad()),
        }
    }
    if rel.as_os_str().is_empty() {
        return Err(bad());
    }
    Ok(rel)
}

/// The human-readable summary that travels beside the manifest.
fn readme_text(m: &BundleManifest, opts: &BundleOptions) -> String {
    let mut text = format!(
        "KeyFrameXL project bundle\n\
         =========================\n\n\
         Project: {} ({} {}, bundle format {})\n\n\
         Import this zip in KeyFrameXL (File > Import Bundle), or unzip it\n\
         and open {} directly.\n",
        m.project_file, m.app, m.app_version, m.format, m.project_file
    );

    let mut seen = HashSet::new();
    let bundled: Vec<&BundledIr> = m
        .irs
        .iter()
        .filter(|ir| seen.insert(&ir.archive_path))
        .collect();
    text.push_str(&format!("\nImpulse responses: {} bundled\n", bundled.len()));
    for ir in bundled {
        text.push_str(&format!(
            "  {}  (was {})\n",
            ir.archive_path, ir.original_name
        ));
    }
    if !m.missing_irs.is_empty() {
        text.push_str("\nMissing impulse responses (still referenced by the project):\n");
        for path in &m.missing_irs {
            text.push_str(&format!("  {path}\n"));
        }
    }

    text.push_str(&format!("\nPlugins: {}\n", m.plugins.len()));
    if !m.plugins.is_empty() {
        text.push_str(
            "Plugin binaries are not included: install them at the paths below or\n\
             re-link them in KeyFrameXL. Their state is embedded in the project file.\n",
        );
        for p in &m.plugins {
            let by = if p.vendor.is_empty() {
                String::new()
            } else {
                format!(" by {}", p.vendor)
            };
            text.push_str(&format!(
                "  {} [{}]{by}\n    path: {}\n    used: {}\n",
                p.name, p.format, p.path, p.where_used
            ));
        }
    }

    if !m.voicebanks.is_empty() {
        text.push_str(&format!("\nVoicebanks: {}\n", m.voicebanks.len()));
        for v in &m.voicebanks {
            if v.bundled {
                text.push_str(&format!(
                    "  {} (channel \"{}\"), bundled\n",
                    v.bank, v.channel
                ));
            } else {
                text.push_str(&format!(
                    "  {} (channel \"{}\"), not bundled\n",
                    v.bank, v.channel
                ));
            }
        }
        if !opts.include_voicebanks {
            text.push_str(
                "Voicebanks are large and licensed by their authors; enable\n\
                 \"include voicebanks\" in the export options to bundle them.\n",
            );
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxSlot;
    use crate::vocal::Voice;

    fn conv_slot(ir: PathBuf) -> FxSlot {
        let mut slot = FxSlot::new(FxKind::Convolution);
        slot.ir = Some(ir);
        slot
    }

    fn test_plugin(id: u64, name: &str) -> PluginRef {
        let mut plugin = PluginRef {
            id,
            format: "vst3".into(),
            path: format!("/usr/lib/vst3/{name}.vst3"),
            name: name.into(),
            vendor: "Acme".into(),
            state: String::new(),
            outs: vec![],
        };
        plugin.set_state(b"plugin-state");
        plugin
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let zip = ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        zip.file_names().map(str::to_string).collect()
    }

    fn read_zip_text(path: &Path, entry: &str) -> String {
        let mut zip = ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut text = String::new();
        zip.by_name(entry)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    #[test]
    fn export_and_import_round_trip_with_ir_and_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let ir_path = dir.path().join("room.wav");
        let ir_bytes = b"RIFF\x24\x00\x00\x00WAVEfmt fake-ir-payload".to_vec();
        std::fs::write(&ir_path, &ir_bytes).unwrap();
        let missing = dir.path().join("gone.wav");

        let mut song = Song::default();
        song.mixer[1].fx.push(conv_slot(ir_path.clone()));
        song.mixer[1].fx.push(conv_slot(missing.clone()));
        song.add_channel(
            "Lead",
            InstrumentKind::Plugin {
                plugin: test_plugin(1, "TestVerb"),
            },
        );

        let zip_path = dir.path().join("bundle.zip");
        let report = export_bundle(&song, "My Song", &zip_path, &BundleOptions::default()).unwrap();
        assert_eq!(report.bundled_irs, 1);
        assert_eq!(report.missing_irs.len(), 1);
        assert_eq!(report.plugins, 1);
        assert_eq!(report.voicebanks, 0);
        assert_eq!(report.manifest.project_file, "My_Song.kfxl");
        assert!(is_bundle_zip(&std::fs::read(&zip_path).unwrap()));
        assert!(!is_bundle_zip(b"KFXLBIN1"));

        let names = zip_names(&zip_path);
        for expected in ["My_Song.kfxl", "manifest.json", "README.txt"] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
        assert_eq!(names.iter().filter(|n| n.starts_with("ir/")).count(), 1);

        let manifest: BundleManifest =
            serde_json::from_str(&read_zip_text(&zip_path, "manifest.json")).unwrap();
        assert_eq!(manifest.format, BUNDLE_FORMAT);
        assert_eq!(manifest.app, project_io::APP_ID);
        assert_eq!(manifest.irs.len(), 1);
        assert_eq!(manifest.irs[0].track, 1);
        assert_eq!(manifest.irs[0].slot, 0);
        assert_eq!(manifest.irs[0].original_name, ir_path.display().to_string());
        assert_eq!(manifest.missing_irs, vec![missing.display().to_string()]);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins[0].name, "TestVerb");
        assert!(manifest.plugins[0].has_state);
        assert!(manifest.voicebanks.is_empty());

        let readme = read_zip_text(&zip_path, "README.txt");
        assert!(readme.contains("TestVerb"));
        assert!(readme.contains(&missing.display().to_string()));
        assert!(readme.contains("state is embedded"));

        let out = tempfile::tempdir().unwrap();
        let imported = import_bundle(&zip_path, out.path()).unwrap();
        assert_eq!(imported.restored_irs, 1);
        assert_eq!(imported.manifest.as_ref(), Some(&manifest));
        let restored = imported.song.mixer[1].fx[0].ir.clone().unwrap();
        assert_eq!(restored, out.path().join(&manifest.irs[0].archive_path));
        assert_eq!(std::fs::read(&restored).unwrap(), ir_bytes);
        // The missing IR keeps its original reference.
        assert_eq!(imported.song.mixer[1].fx[1].ir, Some(missing));
        assert_eq!(imported.project_path, out.path().join("My_Song.kfxl"));
        let reopened = project_io::open(&imported.project_path).unwrap();
        assert_eq!(reopened.mixer[1].fx[0].ir, Some(restored));
        assert_eq!(reopened.instruments.last().unwrap().name, "Lead");
    }

    #[test]
    fn deduplicates_shared_ir_files() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared.wav");
        std::fs::write(&shared, b"shared-ir").unwrap();
        let mut song = Song::default();
        song.mixer[1].fx.push(conv_slot(shared.clone()));
        song.mixer[2].fx.push(conv_slot(shared.clone()));

        let zip_path = dir.path().join("bundle.zip");
        let report = export_bundle(&song, "shared", &zip_path, &BundleOptions::default()).unwrap();
        assert_eq!(report.bundled_irs, 1);
        assert_eq!(report.manifest.irs.len(), 2);
        assert_eq!(
            report.manifest.irs[0].archive_path,
            report.manifest.irs[1].archive_path
        );
        assert_eq!(
            zip_names(&zip_path)
                .iter()
                .filter(|n| n.starts_with("ir/"))
                .count(),
            1
        );

        let out = tempfile::tempdir().unwrap();
        let imported = import_bundle(&zip_path, out.path()).unwrap();
        assert_eq!(imported.restored_irs, 2);
        let a = imported.song.mixer[1].fx[0].ir.clone().unwrap();
        let b = imported.song.mixer[2].fx[0].ir.clone().unwrap();
        assert_eq!(a, b);
        assert_eq!(std::fs::read(&a).unwrap(), b"shared-ir");
    }

    #[test]
    fn rejects_path_traversal_entries() {
        for bad in [
            "../evil.txt",
            "/abs/evil.txt",
            "..\\evil.txt",
            "ir/../../evil.txt",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let zip_path = dir.path().join("bad.zip");
            let mut zip = ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            zip.start_file(bad, entry_options()).unwrap();
            zip.write_all(b"evil").unwrap();
            zip.finish().unwrap();

            let out = tempfile::tempdir().unwrap();
            let err = import_bundle(&zip_path, out.path()).unwrap_err();
            assert!(err.contains("Unsafe path"), "{bad}: {err}");
            assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
            assert!(!dir.path().join("evil.txt").exists());
        }
    }

    #[test]
    fn sanitize_stem_handles_edge_cases() {
        assert_eq!(sanitize_stem(""), "project");
        assert_eq!(sanitize_stem("   "), "project");
        assert_eq!(sanitize_stem("日本語 ♪♫"), "project");
        assert_eq!(sanitize_stem("..."), "project");
        assert_eq!(sanitize_stem("../../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize_stem("My Song!"), "My_Song");
        assert_eq!(sanitize_stem("take-01_final"), "take-01_final");
        assert!(sanitize_stem(&"x".repeat(300)).chars().count() <= 80);

        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("bundle.zip");
        let report = export_bundle(
            &Song::default(),
            "../evil name",
            &zip_path,
            &BundleOptions::default(),
        )
        .unwrap();
        assert_eq!(report.manifest.project_file, "evil_name.kfxl");
        assert!(zip_names(&zip_path).iter().any(|n| n == "evil_name.kfxl"));
    }

    #[test]
    fn voicebanks_bundled_only_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let bank = dir.path().join("Teto Bank");
        std::fs::create_dir_all(bank.join("sub")).unwrap();
        std::fs::write(bank.join("oto.ini"), b"[a.wav]\n").unwrap();
        std::fs::write(bank.join("sub/a.wav"), b"wav").unwrap();
        std::fs::write(bank.join(".hidden"), b"secret").unwrap();
        let mut song = Song::default();
        song.add_channel(
            "Vox",
            InstrumentKind::Vocal {
                voice: Voice::new(&bank.to_string_lossy(), "Teto"),
            },
        );

        // Default: listed, not bundled.
        let plain = dir.path().join("plain.zip");
        let report = export_bundle(&song, "vox", &plain, &BundleOptions::default()).unwrap();
        assert_eq!(report.voicebanks, 1);
        assert!(!report.manifest.voicebanks[0].bundled);
        assert!(
            !zip_names(&plain)
                .iter()
                .any(|n| n.starts_with("voicebanks/"))
        );

        // Enabled: files travel, hidden ones stay out.
        let full = dir.path().join("full.zip");
        let report = export_bundle(
            &song,
            "vox",
            &full,
            &BundleOptions {
                include_voicebanks: true,
            },
        )
        .unwrap();
        assert!(report.manifest.voicebanks[0].bundled);
        let names = zip_names(&full);
        assert!(names.iter().any(|n| n == "voicebanks/Teto_Bank/oto.ini"));
        assert!(names.iter().any(|n| n == "voicebanks/Teto_Bank/sub/a.wav"));
        assert!(!names.iter().any(|n| n.contains(".hidden")));

        let out = tempfile::tempdir().unwrap();
        let imported = import_bundle(&full, out.path()).unwrap();
        let InstrumentKind::Vocal { voice } = &imported.song.instruments.last().unwrap().kind
        else {
            panic!("vocal channel lost");
        };
        let extracted = out.path().join("voicebanks/Teto_Bank");
        assert_eq!(voice.bank, extracted.to_string_lossy());
        assert!(extracted.join("oto.ini").is_file());
        assert!(!extracted.join(".hidden").exists());
    }

    #[test]
    fn imports_manual_zip_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let mut song = Song::default();
        song.mixer[1]
            .fx
            .push(conv_slot(PathBuf::from("ir/0_room.wav")));
        let zip_path = dir.path().join("manual.zip");
        let mut zip = ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        write_entry(&mut zip, "song.kfxl", &project_io::to_bytes(&song).unwrap()).unwrap();
        write_entry(&mut zip, "ir/0_room.wav", b"ir-bytes").unwrap();
        zip.finish().unwrap();

        let out = tempfile::tempdir().unwrap();
        let imported = import_bundle(&zip_path, out.path()).unwrap();
        assert!(imported.manifest.is_none());
        assert_eq!(imported.restored_irs, 1);
        assert_eq!(imported.project_path, out.path().join("song.kfxl"));
        assert_eq!(
            std::fs::read(imported.song.mixer[1].fx[0].ir.as_ref().unwrap()).unwrap(),
            b"ir-bytes"
        );
        assert!(project_io::open(&imported.project_path).is_ok());
    }
}
