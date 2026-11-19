//! UTAU voicebank: `oto.ini` entries (alias → wav region), `prefix.map`
//! (pitch-dependent spellings) and `character.txt` (name).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// One `oto.ini` line: where in a wav a syllable lives. All times in ms.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub wav: PathBuf,
    pub alias: String,
    /// Start of the used part of the wav.
    pub offset: f64,
    /// Fixed region after the offset (the consonant): not stretched by note length.
    pub consonant: f64,
    /// > 0: unused tail measured from the wav's end. < 0: the used length from the offset.
    pub cutoff: f64,
    /// Where in the used part the note's start falls.
    pub preutter: f64,
    /// How much of the start crossfades with the previous syllable.
    pub overlap: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Voicebank {
    pub root: PathBuf,
    pub name: String,
    pub entries: HashMap<String, Entry>,
    /// MIDI note → (prefix, suffix) from `prefix.map`.
    pub prefix_map: HashMap<u8, (String, String)>,
}

/// Shift-JIS is what UTAU writes; UTF-8 is what newer tools write.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

/// `C4` → 60 (UTAU names middle C `C4`).
pub fn note_number(name: &str) -> Option<u8> {
    let mut chars = name.trim().chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (shift, oct) = match rest.strip_prefix('#') {
        Some(o) => (1, o),
        None => match rest.strip_prefix('b') {
            Some(o) => (-1, o),
            None => (0, rest.as_str()),
        },
    };
    let oct: i32 = oct.parse().ok()?;
    let n = (oct + 1) * 12 + base + shift;
    (0..128).contains(&n).then_some(n as u8)
}

pub fn parse_oto(text: &str, dir: &Path, into: &mut HashMap<String, Entry>) {
    for line in text.lines() {
        let Some((wav, rest)) = line.split_once('=') else {
            continue;
        };
        let wav = wav.trim();
        if wav.is_empty() {
            continue;
        }
        let mut f = rest.split(',');
        let alias = f.next().unwrap_or("").trim().to_string();
        let mut num = || {
            f.next()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        let (offset, consonant, cutoff, preutter, overlap) = (num(), num(), num(), num(), num());
        let alias = if alias.is_empty() {
            Path::new(wav)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            alias
        };
        // The first line for an alias wins, like UTAU.
        into.entry(alias.clone()).or_insert(Entry {
            wav: dir.join(wav),
            alias,
            offset,
            consonant,
            cutoff,
            preutter,
            overlap,
        });
    }
}

fn parse_prefix_map(text: &str, into: &mut HashMap<u8, (String, String)>) {
    for line in text.lines() {
        let mut f = line.split('\t');
        let (Some(note), prefix, suffix) = (
            f.next().and_then(note_number),
            f.next().unwrap_or(""),
            f.next().unwrap_or(""),
        ) else {
            continue;
        };
        into.insert(note, (prefix.trim().to_string(), suffix.trim().to_string()));
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    items.sort();
    for p in &items {
        if p.is_file()
            && p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("oto.ini"))
        {
            out.push(p.clone());
        }
    }
    if depth < 6 {
        for p in items.iter().filter(|p| p.is_dir()) {
            walk(p, depth + 1, out);
        }
    }
}

impl Voicebank {
    /// Load every `oto.ini` under `dir` (a bank's sub-folders merge, the
    /// shallower and earlier ones winning).
    pub fn load(dir: &Path) -> Result<Self, String> {
        let mut otos = Vec::new();
        walk(dir, 0, &mut otos);
        if otos.is_empty() {
            return Err(format!("no oto.ini under {}", dir.display()));
        }
        let mut bank = Voicebank {
            root: dir.to_path_buf(),
            ..Default::default()
        };
        for oto in &otos {
            let bytes = std::fs::read(oto).map_err(|e| format!("{}: {e}", oto.display()))?;
            let folder = oto.parent().unwrap_or(dir);
            parse_oto(&decode_text(&bytes), folder, &mut bank.entries);
            if let Ok(b) = std::fs::read(folder.join("prefix.map")) {
                parse_prefix_map(&decode_text(&b), &mut bank.prefix_map);
            }
        }
        bank.entries.retain(|_, e| e.wav.is_file());
        if bank.entries.is_empty() {
            return Err("the voicebank's oto.ini points at no wav files".into());
        }
        bank.name = character_name(dir).unwrap_or_else(|| {
            dir.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        Ok(bank)
    }

    /// The entry for one of `aliases` (tried in order), with the pitch's
    /// `prefix.map` spelling preferred.
    pub fn find(&self, aliases: &[String], pitch: u8) -> Option<&Entry> {
        let pm = self.prefix_map.get(&pitch);
        for a in aliases {
            if let Some((p, s)) = pm
                && let Some(e) = self.entries.get(&format!("{p}{a}{s}"))
            {
                return Some(e);
            }
            if let Some(e) = self.entries.get(a) {
                return Some(e);
            }
        }
        None
    }
}

/// `name=` of the nearest `character.txt`.
pub fn character_name(dir: &Path) -> Option<String> {
    for p in [dir.to_path_buf(), dir.parent()?.to_path_buf()] {
        if let Ok(b) = std::fs::read(p.join("character.txt")) {
            for line in decode_text(&b).lines() {
                if let Some(n) = line.strip_prefix("name=") {
                    let n = n.trim();
                    if !n.is_empty() {
                        return Some(n.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_like_utau() {
        let mut m = HashMap::new();
        parse_oto(
            "_あ.wav=あ,24,56,73,5,20\n_あ.wav=- あ,24,56,-300,5,20\nx.wav=,1,2,3,4,5\n",
            Path::new("/b"),
            &mut m,
        );
        let e = &m["あ"];
        assert_eq!(
            (e.offset, e.consonant, e.cutoff, e.preutter, e.overlap),
            (24.0, 56.0, 73.0, 5.0, 20.0)
        );
        assert_eq!(e.wav, Path::new("/b/_あ.wav"));
        assert_eq!(m["- あ"].cutoff, -300.0);
        // Empty alias = the wav's name.
        assert!(m.contains_key("x"));
    }

    #[test]
    fn note_names() {
        assert_eq!(note_number("C4"), Some(60));
        assert_eq!(note_number("A4"), Some(69));
        assert_eq!(note_number("C#3"), Some(49));
        assert_eq!(note_number("zz"), None);
    }

    #[test]
    fn decodes_shift_jis() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("あ");
        assert_eq!(decode_text(&bytes), "あ");
        assert_eq!(decode_text("あ".as_bytes()), "あ");
    }
}
