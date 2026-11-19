//! FILE → Import / Export → MIDI file. Import adds a new pattern with a channel
//! per MIDI track (drums on channel 10 become Kick / Snare / Hat) and gives the
//! new channels real sounds: a matching General MIDI preset from a .dwp library
//! for melodic tracks, the default kit's samples for drums.

use super::{App, Tab, Tone, library};
use keyframexl::midi::{self, ExportOptions, ImportOptions, MidiSource};
use keyframexl::sequence::InstrumentKind;
use std::path::{Path, PathBuf};

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// How well a preset file name matches a MIDI track / GM instrument name.
/// Whole words count 3, shared 5-letter prefixes ("overdriven"/"overdrive") 2.
fn score(name: &[String], preset: &[String]) -> u32 {
    name.iter()
        .map(|w| {
            if preset.contains(w) {
                3
            } else if w.len() >= 5 && preset.iter().any(|p| p.len() >= 5 && p[..5] == w[..5]) {
                2
            } else {
                0
            }
        })
        .sum()
}

/// Plain choices for one-word track names that match a whole family.
const PLAIN: [(&str, &str); 9] = [
    ("bass", "fingered bass"),
    ("piano", "piano 1"),
    ("guitar", "clean guitar"),
    ("strings", "strings"),
    ("brass", "brass 1"),
    ("organ", "organ 1"),
    ("sax", "alto sax"),
    ("voice", "choir aahs"),
    ("lead", "saw wave"),
];

/// The best .dwp in `libs` for a channel called `name` (folder names count too:
/// "Bass" finds the Bass folder's plain preset).
pub fn best_preset(libs: &[PathBuf], name: &str) -> Option<PathBuf> {
    let want = words(name);
    let plain = PLAIN
        .iter()
        .find(|(k, _)| want.len() == 1 && want[0] == *k)
        .map(|(_, v)| words(v));
    let mut best: Option<(u32, usize, PathBuf)> = None;
    for lib in libs {
        let mut stack = vec![lib.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwp")) {
                    continue;
                }
                let stem = words(&p.file_stem().unwrap_or_default().to_string_lossy());
                let folder = words(
                    &p.parent()
                        .and_then(|f| f.file_name())
                        .unwrap_or_default()
                        .to_string_lossy(),
                );
                let mut s = score(&want, &stem) * 2 + score(&want, &folder);
                if s > 0 && plain.as_ref().is_some_and(|pl| stem.starts_with(pl)) {
                    s += 10;
                }
                // Higher score wins; on a tie the name with fewer extra words, then the shorter.
                let extra =
                    stem.iter().filter(|w| !want.contains(w)).count() * 100 + stem.join(" ").len();
                if s > 0
                    && best
                        .as_ref()
                        .is_none_or(|(bs, bl, _)| s > *bs || (s == *bs && extra < *bl))
                {
                    best = Some((s, extra, p));
                }
            }
        }
    }
    best.map(|(_, _, p)| p)
}

/// Imported channels play 6 dB below their MIDI volume: a General MIDI file is
/// mixed for a synth whose channels are quiet, and a dozen full-level presets
/// sum far past 0 dBFS.
const IMPORT_HEADROOM: f32 = 0.5;

/// Gain staging for a MIDI import: the new channels get headroom and Master gets
/// a Limiter (FL's default template has one) unless it already has one.
/// Returns true when the Limiter was added.
pub(crate) fn stage_import(song: &mut keyframexl::sequence::Song, ids: &[u64]) -> bool {
    use keyframexl::fx::{FxKind, FxSlot};
    for i in song.instruments.iter_mut().filter(|i| ids.contains(&i.id)) {
        i.volume *= IMPORT_HEADROOM;
    }
    let Some(master) = song.mixer.first_mut() else {
        return false;
    };
    if master.fx.iter().any(|f| f.kind == FxKind::Limiter) {
        return false;
    }
    master.fx.push(FxSlot::new(FxKind::Limiter));
    true
}

impl App {
    pub(crate) fn import_midi_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("MIDI", &["mid", "midi"])
            .pick_file()
        {
            self.import_midi(&path);
        }
    }

    /// One undo step; sounds load in the background without extra steps.
    pub(crate) fn import_midi(&mut self, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.say(Tone::Bad, format!("Cannot read {}: {e}", path.display())),
        };
        self.checkpoint();
        // With a drum kit on disk, channel 10 comes in as one kit channel.
        let kits = library::drum_kits(&self.libraries.dwp);
        let opts = ImportOptions {
            drum_kit: !kits.is_empty(),
            ..ImportOptions::default()
        };
        match midi::import_smf(&bytes, &mut self.song, &opts) {
            Ok(report) => {
                self.selected = report.pattern;
                let limiter = if stage_import(&mut self.song, &report.channels) {
                    " Limiter added on Master."
                } else {
                    ""
                };
                self.publish();
                let sounds = self.dress_midi_channels(&report.channels);
                let file = path.file_name().unwrap_or_default().to_string_lossy();
                self.say(
                    Tone::Done,
                    format!("{file}: {}{limiter}{sounds}", midi::summary(&report)),
                );
            }
            Err(e) => {
                self.history.pop();
                self.say(Tone::Bad, format!("Cannot import {}: {e}", path.display()));
            }
        }
    }

    /// Load a preset or sample into each new channel; returns a note for the status.
    fn dress_midi_channels(&mut self, ids: &[u64]) -> String {
        let kit = library::default_kit(&self.libraries);
        let kits = library::drum_kits(&self.libraries.dwp);
        let mut n = 0;
        for &id in ids {
            let Some(inst) = self.song.instrument(id).cloned() else {
                continue;
            };
            let path = match inst.kind {
                InstrumentKind::Drum { voice } => {
                    let want = ["Kick", "Snare", "Hat"]
                        .get(voice as usize)
                        .copied()
                        .unwrap_or("Hat");
                    kit.iter()
                        .find(|(name, _)| *name == want)
                        .map(|(_, p)| p.clone())
                }
                InstrumentKind::Sampler { .. } => {
                    match inst.name.strip_prefix(midi::DRUM_KIT_PREFIX) {
                        Some(kit_name) => library::pick_kit(&kits, kit_name).cloned(),
                        None => best_preset(&self.libraries.dwp, &inst.name),
                    }
                }
                InstrumentKind::Plugin { .. } => None,
                InstrumentKind::Synth { .. }
                | InstrumentKind::Vocal { .. }
                | InstrumentKind::Forge { .. } => None,
            };
            if let Some(p) = path {
                self.load_preset_with(id, p, true);
                n += 1;
            }
        }
        match n {
            0 => String::new(),
            n => format!(" Loading sounds for {n} channel(s)…"),
        }
    }

    /// Ctrl+Shift+M: the song when the playlist has clips, else the pattern.
    pub(crate) fn export_midi(&mut self) {
        let song_mode = !self.song.clips.is_empty();
        let name = format!(
            "{}{}.mid",
            self.project_name(),
            if song_mode { "_song" } else { "" }
        );
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MIDI", &["mid"])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        self.export_midi_to(&path, song_mode);
    }

    pub(crate) fn export_midi_to(&mut self, path: &Path, song_mode: bool) {
        let source = if song_mode {
            MidiSource::Song
        } else {
            MidiSource::Pattern(self.selected)
        };
        let bytes = midi::export_smf(&self.song, &source, &ExportOptions::default());
        match std::fs::write(path, bytes) {
            Ok(()) => self.say(
                Tone::Done,
                format!(
                    "Exported {} ({}).",
                    path.display(),
                    if song_mode { "song" } else { "pattern" }
                ),
            ),
            Err(e) => self.say(Tone::Bad, format!("MIDI not written: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm_names_find_their_presets() {
        let lib = tempfile::tempdir().unwrap();
        for f in [
            "Guitar/Overdrive Guitar.dwp",
            "Guitar/Distortion Guitar (1).dwp",
            "Bass/Fingered Bass.dwp",
            "Bass/Slap Bass 1.dwp",
            "Reed/Baritone Sax.dwp",
            "Synth lead/LM Square.dwp",
            "Ensamble/Strings.dwp",
            "Ensamble/Slow Strings.dwp",
        ] {
            let p = lib.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        let libs = vec![lib.path().to_path_buf()];
        let pick = |n: &str| {
            best_preset(&libs, n).map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
        };
        assert_eq!(
            pick("Overdriven Guitar").as_deref(),
            Some("Overdrive Guitar")
        );
        assert_eq!(
            pick("Distortion Guitar").as_deref(),
            Some("Distortion Guitar (1)")
        );
        assert_eq!(pick("Baritone Sax").as_deref(), Some("Baritone Sax"));
        assert_eq!(
            pick("Strings").as_deref(),
            Some("Strings"),
            "the plain name beats Slow Strings"
        );
        assert_eq!(
            pick("Bass").as_deref(),
            Some("Fingered Bass"),
            "a folder match picks its plainest preset"
        );
        assert_eq!(pick("Square").as_deref(), Some("LM Square"));
        assert_eq!(pick("Kazoo"), None);
    }

    #[test]
    fn import_staging_adds_headroom_and_one_limiter() {
        use keyframexl::fx::FxKind;
        let mut song = keyframexl::sequence::Song::default();
        let a = song.add_sampler("A");
        let keep = song.instruments[0].id;
        let before = song.instrument(keep).unwrap().volume;
        assert!(stage_import(&mut song, &[a]));
        assert_eq!(song.instrument(a).unwrap().volume, 0.5);
        assert_eq!(
            song.instrument(keep).unwrap().volume,
            before,
            "existing channels keep their level"
        );
        assert!(
            !stage_import(&mut song, &[]),
            "a second import keeps the one Limiter"
        );
        assert_eq!(
            song.mixer[0]
                .fx
                .iter()
                .filter(|f| f.kind == FxKind::Limiter)
                .count(),
            1
        );
    }
}

#[cfg(test)]
mod bench {
    /// KEYFRAMEXL_BENCH_MIDI=file.mid with KEYFRAMEXL_BENCH_DWP=library: render
    /// speed and level of the imported song with the presets the import would
    /// load. KEYFRAMEXL_BENCH_RAW skips the import gain staging,
    /// KEYFRAMEXL_BENCH_SOLO adds each channel's solo peak.
    #[test]
    fn imported_song_with_presets_speed() {
        let (Some(mid), Some(lib)) = (
            std::env::var_os("KEYFRAMEXL_BENCH_MIDI"),
            std::env::var_os("KEYFRAMEXL_BENCH_DWP"),
        ) else {
            return;
        };
        let mut song = keyframexl::sequence::Song::default();
        let r = keyframexl::midi::import_smf(
            &std::fs::read(mid).unwrap(),
            &mut song,
            &Default::default(),
        )
        .unwrap();
        let libs = vec![std::path::PathBuf::from(lib)];
        let mut zones = 0;
        for &id in &r.channels {
            let inst = song.instrument(id).unwrap();
            if !inst.is_sampler() {
                continue;
            }
            let name = inst.name.clone();
            if let Some(p) = super::best_preset(&libs, &name)
                && let Ok(preset) = crate::import::dwp(&p)
            {
                zones += preset.zones.len();
                eprintln!("{name:22} <- {}", p.file_stem().unwrap().to_string_lossy());
                song.set_sampler_preset(id, preset).unwrap();
            }
        }
        if std::env::var_os("KEYFRAMEXL_BENCH_RAW").is_none() {
            super::stage_import(&mut song, &r.channels);
        }
        let mut e = keyframexl::engine::Engine::new(48_000);
        e.reserve(song.mixer.len(), 512);
        let mut buf = vec![0.0f32; 1024];
        let (secs, mut at, mut peak_voices) = (20.0, 0, 0);
        let t = std::time::Instant::now();
        for _ in 0..(secs * 48_000.0 / 512.0) as u64 {
            at = e.process(&song, r.pattern, false, at, &mut buf, true);
            peak_voices = peak_voices.max(e.active_voices());
        }
        let took = t.elapsed().as_secs_f64();
        let mut e = keyframexl::engine::Engine::new(48_000);
        let mut peak = 0.0f32;
        let mut over = 0usize;
        let mut at = 0;
        for _ in 0..(30.0 * 48_000.0 / 512.0) as u64 {
            at = e.process(&song, r.pattern, false, at, &mut buf, true);
            peak = peak.max(e.peaks()[0]);
            over += buf.iter().filter(|x| x.abs() >= 0.999).count();
        }
        eprintln!(
            "first 30 s: Master peak before the output clamp {peak:.3} ({:+.1} dB), {over} samples at full scale",
            20.0 * peak.log10()
        );
        if std::env::var_os("KEYFRAMEXL_BENCH_SOLO").is_some() {
            for &id in &r.channels {
                let mut solo = song.clone();
                for i in &mut solo.instruments {
                    i.muted = i.id != id;
                }
                let mut e = keyframexl::engine::Engine::new(48_000);
                let (mut pk, mut at) = (0.0f32, 0);
                for _ in 0..(30.0 * 48_000.0 / 512.0) as u64 {
                    at = e.process(&solo, r.pattern, false, at, &mut buf, true);
                    pk = pk.max(e.peaks()[0]);
                }
                let inst = solo.instrument(id).unwrap();
                eprintln!(
                    "solo {:22} vol {:.2} peak {:+.1} dB",
                    inst.name,
                    inst.volume,
                    20.0 * pk.log10()
                );
            }
        }
        eprintln!(
            "{zones} zones; 20 s of audio in {took:.2} s = {:.1}% of realtime, up to {peak_voices} voices",
            took / secs * 100.0
        );
    }
}

impl App {
    /// The channel a dropped MIDI clip belongs to (FL: drag a groove from a
    /// plugin onto the piano roll): the open piano roll's channel, else the
    /// channel whose window has focus. None = import as a new pattern.
    pub(crate) fn midi_drop_channel(&self) -> Option<u64> {
        // The channel row or piano roll under the pointer, where there is one.
        if let Some(at) = self.drop_pointer
            && let Some((_, id)) = self.midi_zones.iter().rev().find(|(r, _)| r.contains(at))
            && self.song.instruments.iter().any(|i| i.id == *id)
        {
            return Some(*id);
        }
        let roll = self
            .ws
            .dock_ref()
            .iter_all_tabs()
            .find_map(|(_, t)| match t {
                Tab::Roll { instrument } => Some(*instrument),
                _ => None,
            });
        let focus = match &self.focus {
            Some(Tab::Instrument { id }) => Some(*id),
            _ => None,
        };
        roll.or(focus)
            .filter(|id| self.song.instruments.iter().any(|i| i.id == *id))
    }

    /// Notes of a MIDI file into channel `ch` of the current pattern: at the
    /// start when the channel has none there, else from the bar after its last note.
    pub(crate) fn drop_midi_notes(&mut self, path: &Path, ch: u64) {
        use keyframexl::sequence::{BAR, Note};
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let notes = match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| midi::smf_notes(&b))
        {
            Ok(n) if !n.is_empty() => n,
            Ok(_) => return self.say(Tone::Bad, format!("{file} has no notes")),
            Err(e) => return self.say(Tone::Bad, format!("Cannot read {file}: {e}")),
        };
        let pid = self.selected;
        let end = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == ch)
            .map(|n| n.end())
            .max();
        let at = end.map(|e| (e + BAR - 1) / BAR * BAR).unwrap_or(0);
        self.checkpoint();
        let mut ids = Vec::new();
        for (start, length, pitch, velocity) in &notes {
            ids.push(
                self.song
                    .add_note(pid, Note::new(ch, at + start, *length, *pitch, *velocity)),
            );
        }
        // Grow the pattern to whole bars around the clip (FL auto length).
        let last = notes.iter().map(|n| at + n.0 + n.1).max().unwrap_or(0);
        let p = self.pattern_mut();
        if last > p.length {
            p.length = ((last + BAR - 1) / BAR * BAR).min(keyframexl::sequence::MAX_BARS * BAR);
        }
        self.roll_sel = ids;
        self.instrument = ch;
        // Fit the roll to the pattern so the new notes are in view.
        if let Some(v) = self.roll_view.get_mut(&ch) {
            v.cell = 0.0;
            v.x0 = 0.0;
        }
        self.publish();
        let name = self.instrument_name(ch);
        self.say(
            Tone::Done,
            format!(
                "{} note(s) from {file} into {name} at bar {}. Ctrl+Z undoes.",
                notes.len(),
                at / BAR + 1
            ),
        );
    }
}
