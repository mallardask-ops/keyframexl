//! Open v1 (.kfxl without `format`) or v2, migrate v1 -> v2.
//! Mapping: 1 old step = 120 ticks (1/16), bar = 1920 ticks, drums -> notes 36/38/42.

use std::collections::BTreeMap;
use std::path::Path;

use super::model::{Clip, Instrument, InstrumentKind, Note, Part, Pattern, Project};
use super::time::{bar_ticks, step_ticks, TICKS_PER_BEAT};

#[derive(serde::Deserialize)]
struct LegacyNote {
    #[serde(default)]
    start: u8,
    #[serde(default)]
    length: u8,
    #[serde(default)]
    pitch: u8,
    #[serde(default)]
    velocity: u8,
    #[serde(default)]
    group: u16,
}

#[derive(serde::Deserialize)]
struct LegacySlide {
    #[serde(default)]
    start: u8,
    #[serde(default)]
    length: u8,
    #[serde(default)]
    target: u8,
    #[serde(default)]
    group: u16,
}

#[derive(serde::Deserialize)]
struct LegacyPattern {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    drums: [[bool; 16]; 3],
    #[serde(default)]
    notes: Vec<LegacyNote>,
    #[serde(default)]
    slides: Vec<LegacySlide>,
}

#[derive(serde::Deserialize)]
struct LegacyClip {
    #[serde(default)]
    pattern: u64,
    #[serde(default)]
    bar: u64,
}

#[derive(serde::Deserialize)]
struct LegacySong {
    #[serde(default)]
    patterns: Vec<LegacyPattern>,
    #[serde(default)]
    clips: Vec<LegacyClip>,
}

pub fn migrate_v1(old: &str) -> Result<Project, String> {
    let legacy: LegacySong = serde_json::from_str(old).map_err(|e| e.to_string())?;
    let step = step_ticks();
    let bar = bar_ticks(4);
    let mut next_note: u64 = 1;
    let mut next_slide: u64 = 1;
    let mut next_part: u64 = 1;
    let mut instruments = vec![
        Instrument { id: 1, name: "Kick".into(), kind: InstrumentKind::Drum { row: 0 } },
        Instrument { id: 2, name: "Snare".into(), kind: InstrumentKind::Drum { row: 1 } },
        Instrument { id: 3, name: "Hat".into(), kind: InstrumentKind::Drum { row: 2 } },
        Instrument { id: 4, name: "Bass".into(), kind: InstrumentKind::Sampler { preset: "demo".into() } },
    ];
    let _ = &mut instruments;
    let mut patterns = Vec::new();
    for lp in legacy.patterns {
        let pid = if lp.id == 0 { patterns.len() as u64 + 1 } else { lp.id };
        let mut parts: BTreeMap<u64, Part> = BTreeMap::new();
        let mut part_for = |parts: &mut BTreeMap<u64, Part>, inst: u64| -> u64 {
            if let Some((id, _)) = parts.iter().find(|(_, pt)| pt.instrument == inst) {
                return *id;
            }
            let id = next_part;
            next_part += 1;
            parts.insert(
                id,
                Part { id, instrument: inst, notes: BTreeMap::new(), slides: BTreeMap::new() },
            );
            id
        };
        for (row, pitch) in [(0, 36u8), (1, 38u8), (2, 42u8)] {
            for (s, on) in lp.drums[row].iter().enumerate() {
                if *on {
                    let pid_part = part_for(&mut parts, (row + 1) as u64);
                    let id = next_note;
                    next_note += 1;
                    parts.get_mut(&pid_part).unwrap().notes.insert(
                        id,
                        Note {
                            id,
                            start: s as i64 * step,
                            len: step,
                            pitch,
                            velocity: 100,
                            muted: false,
                            slide_group: 0,
                            color: 0,
                            channel: 0,
                        },
                    );
                }
            }
        }
        let bass_part = part_for(&mut parts, 4);
        for n in lp.notes {
            let id = next_note;
            next_note += 1;
            parts.get_mut(&bass_part).unwrap().notes.insert(
                id,
                Note {
                    id,
                    start: n.start as i64 * step,
                    len: (n.length.max(1)) as i64 * step,
                    pitch: n.pitch,
                    velocity: n.velocity.clamp(1, 127),
                    muted: false,
                    slide_group: n.group,
                    color: 0,
                    channel: 0,
                },
            );
        }
        for gl in lp.slides {
            let id = next_slide;
            next_slide += 1;
            parts.get_mut(&bass_part).unwrap().slides.insert(
                id,
                super::model::Slide {
                    id,
                    start: gl.start as i64 * step,
                    len: (gl.length.max(1)) as i64 * step,
                    target: gl.target,
                    group: gl.group,
                },
            );
        }
        patterns.push(Pattern {
            id: pid,
            name: if lp.name.is_empty() { format!("Pattern {pid}") } else { lp.name },
            len_ticks: bar,
            parts: parts.into_values().collect(),
        });
    }
    let mut next_clip: u64 = 1;
    let mut playlist = Vec::new();
    for c in legacy.clips {
        let id = next_clip;
        next_clip += 1;
        playlist.push(Clip {
            id,
            pattern: c.pattern,
            start_ticks: c.bar as i64 * bar,
        });
    }
    let project = Project {
        format: 2,
        ticks_per_beat: TICKS_PER_BEAT,
        tempo: 120.0,
        beats_per_bar: 4,
        instruments,
        patterns,
        playlist,
        next_ids: BTreeMap::from([
            ("note".into(), next_note),
            ("slide".into(), next_slide),
            ("part".into(), next_part),
            ("clip".into(), next_clip),
        ]),
    };
    project.validate().map_err(|e| e.to_string())?;
    Ok(project)
}

pub fn open_any(path: &Path) -> Result<Project, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if let Ok(p) = serde_json::from_slice::<Project>(&bytes) {
        p.validate().map_err(|e| e.to_string())?;
        return Ok(p);
    }
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    migrate_v1(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_step_maps_to_120_ticks() {
        let v1 = r#"{"patterns":[{"id":1,"name":"P1","drums":[[false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false],[false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false],[false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]],"notes":[{"start":6,"length":4,"pitch":48,"velocity":100,"group":0}],"slides":[]}],"clips":[{"pattern":1,"bar":1}]}"#;
        let p = migrate_v1(v1).unwrap();
        let part = &p.patterns[0].parts.iter().find(|pt| pt.instrument == 4).unwrap();
        let n = part.notes.values().next().unwrap();
        assert_eq!((n.start, n.len), (720, 480));
        assert_eq!(p.playlist[0].start_ticks, 1920);
    }

    #[test]
    fn old_files_without_version_open() {
        let v1 = r#"{"patterns":[],"clips":[]}"#;
        assert!(migrate_v1(v1).is_ok());
    }
}
