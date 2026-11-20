//! Project format 1 (steps, three fixed drum rows) and its migration to the
//! current model. Read-only: nothing writes format 1 any more.
//!
//! Migration keeps the sound bit for bit: drum steps become notes on their
//! drum channel in the order the old engine triggered them (row, then step,
//! then the old notes), a row muted or left out by solo becomes muted notes.

use crate::sequence::{
    BAR, Clip, Glide, Instrument, InstrumentKind, Note, Pattern, STEP, SamplePreset, Song,
};
use serde::Deserialize;
use std::sync::Arc;

fn default_velocity() -> u8 {
    100
}
/// Format-1 notes predate instruments; they belong to the Bass sampler (id 4).
fn default_instrument() -> u64 {
    4
}
fn default_tempo() -> f64 {
    120.0
}

#[derive(Deserialize)]
struct V1Note {
    start: u8,
    length: u8,
    pitch: u8,
    #[serde(default = "default_velocity")]
    velocity: u8,
    group: u8,
    #[serde(default = "default_instrument")]
    instrument: u64,
}

#[derive(Deserialize)]
struct V1Glide {
    start: u8,
    length: u8,
    target: u8,
    group: u8,
    #[serde(default = "default_instrument")]
    instrument: u64,
}

/// Drum lane: 0 = off, 1..=127 = velocity. The oldest files store bools.
fn de_drums<'de, D>(d: D) -> Result<[[u8; 16]; 3], D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum S {
        B(bool),
        N(u8),
    }
    let rows: Vec<Vec<S>> = Vec::deserialize(d)?;
    if rows.len() != 3 || rows.iter().any(|r| r.len() != 16) {
        return Err(serde::de::Error::custom("drums must be 3x16"));
    }
    let mut out = [[0u8; 16]; 3];
    for (r, row) in rows.iter().enumerate() {
        for (i, s) in row.iter().enumerate() {
            out[r][i] = match s {
                S::B(true) => 100,
                S::B(false) => 0,
                S::N(n) => (*n).min(127),
            };
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct V1Pattern {
    id: u64,
    name: String,
    #[serde(default, deserialize_with = "de_drums")]
    drums: [[u8; 16]; 3],
    #[serde(default)]
    muted: [bool; 3],
    #[serde(default)]
    solo: [bool; 3],
    notes: Vec<V1Note>,
    slides: Vec<V1Glide>,
}

#[derive(Deserialize)]
struct V1Clip {
    pattern: u64,
    bar: u32,
}

#[derive(Deserialize)]
pub struct V1Song {
    #[serde(default = "default_tempo")]
    tempo: f64,
    #[serde(default)]
    swing: f64,
    #[serde(default)]
    sample: Option<Arc<SamplePreset>>,
    #[serde(default)]
    instruments: Vec<Instrument>,
    patterns: Vec<V1Pattern>,
    clips: Vec<V1Clip>,
}

fn v1_instruments() -> Vec<Instrument> {
    vec![
        Instrument::new(1, "Kick", InstrumentKind::Drum { voice: 0 }),
        Instrument::new(2, "Snare", InstrumentKind::Drum { voice: 1 }),
        Instrument::new(3, "Hat", InstrumentKind::Drum { voice: 2 }),
        Instrument::new(4, "Bass", InstrumentKind::Sampler { preset: None }),
    ]
}

impl V1Song {
    pub fn migrate(self) -> Song {
        let mut instruments = self.instruments;
        if instruments.is_empty() {
            // Files from before the instrument registry: the global sample is the Bass preset.
            instruments = v1_instruments();
            if let Some(preset) = self.sample {
                let bass = instruments.iter_mut().find(|i| i.id == 4).expect("bass");
                bass.name = preset.name.clone();
                bass.kind = InstrumentKind::Sampler {
                    preset: Some(preset),
                };
            }
        }
        // Every drum row needs a channel to own its steps.
        for row in 0..3u8 {
            if !instruments.iter().any(|i| i.drum_voice() == Some(row)) {
                let id = instruments.iter().map(|i| i.id).max().unwrap_or(0) + 1;
                instruments.push(Instrument::new(
                    id,
                    crate::sequence::DRUM_VOICES[row as usize],
                    InstrumentKind::Drum { voice: row },
                ));
            }
        }
        let drum_id = |row: u8| {
            instruments
                .iter()
                .find(|i| i.drum_voice() == Some(row))
                .map(|i| i.id)
                .expect("drum channel")
        };
        let mut next_id = 0u64;
        let mut id = || {
            next_id += 1;
            next_id
        };
        let patterns = self
            .patterns
            .into_iter()
            .map(|p| {
                let solo_any = p.solo.iter().any(|&s| s);
                let mut notes = Vec::new();
                for row in 0..3 {
                    let muted = p.muted[row] || (solo_any && !p.solo[row]);
                    for (step, &vel) in p.drums[row].iter().enumerate() {
                        if vel > 0 {
                            notes.push(Note {
                                id: id(),
                                muted,
                                ..Note::new(drum_id(row as u8), step as i64 * STEP, STEP, 60, vel)
                            });
                        }
                    }
                }
                for n in p.notes {
                    notes.push(Note {
                        id: id(),
                        group: n.group,
                        ..Note::new(
                            n.instrument,
                            n.start as i64 * STEP,
                            n.length as i64 * STEP,
                            n.pitch,
                            n.velocity,
                        )
                    });
                }
                let slides = p
                    .slides
                    .into_iter()
                    .map(|s| Glide {
                        start: s.start as i64 * STEP,
                        length: s.length as i64 * STEP,
                        target: s.target,
                        group: s.group,
                        instrument: s.instrument,
                    })
                    .collect();
                Pattern {
                    id: p.id,
                    name: p.name,
                    length: BAR,
                    notes,
                    slides,
                    automation: None,
                    color: None,
                    loop_range: None,
                    lyrics: Default::default(),
                }
            })
            .map(std::sync::Arc::new)
            .collect();
        Song {
            native_fl: None,
            version: 2,
            tempo: if self.tempo == 0.0 {
                default_tempo()
            } else {
                self.tempo
            },
            swing: self.swing,
            instruments,
            patterns,
            clips: {
                // Format 1 stacked clips that share a bar into lanes; lanes become tracks.
                let mut clips: Vec<Clip> = Vec::new();
                for c in self.clips {
                    let start = c.bar as i64 * BAR;
                    let track = (0..)
                        .find(|t| !clips.iter().any(|o| o.track == *t && o.start == start))
                        .unwrap_or(0);
                    clips.push(Clip::new(c.pattern, start, track));
                }
                clips
            },
            tracks: vec![],
            mixer: crate::sequence::default_mixer(),
            tempo_map: crate::tempo::TempoMap::default(),
            meters: vec![],
            loop_range: None,
            markers: vec![],
            info: Default::default(),
            game_style: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::render;

    /// FNV-1a over the length and every sample's bits: stable across toolchains.
    fn hash(v: &[[f32; 2]]) -> String {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut eat = |x: u64| {
            for b in x.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        };
        eat(v.len() as u64);
        for f in v {
            eat(f[0].to_bits() as u64);
            eat(f[1].to_bits() as u64);
        }
        format!("{h:016x}")
    }

    /// The renders were recorded with the format-1 engine before the migration
    /// existed (tests/fixtures/v1/renders.txt). Same projects, same samples.
    #[test]
    fn format_1_projects_render_bit_identical_after_migration() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1");
        let expected = std::fs::read_to_string(dir.join("renders.txt")).unwrap();
        let mut checked = 0;
        for line in expected.lines() {
            let mut it = line.split_whitespace();
            let (name, pat, song) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
            let s = crate::project_io::open(&dir.join(format!("{name}.kfxl"))).unwrap();
            assert_eq!(s.version, 2);
            assert_eq!(
                hash(&render(&s, 1, false)),
                pat,
                "{name}: pattern render changed"
            );
            assert_eq!(
                hash(&render(&s, 1, true)),
                song,
                "{name}: song render changed"
            );
            checked += 1;
        }
        assert_eq!(checked, 3);
    }

    #[test]
    fn bool_drums_mutes_and_solo_become_notes() {
        let mut rows = vec![vec![serde_json::json!(false); 16]; 3];
        rows[0][0] = serde_json::json!(true);
        rows[1][4] = serde_json::json!(90);
        rows[2][2] = serde_json::json!(true);
        let v = serde_json::json!({"version": 1, "patterns": [{"id": 1, "name": "P", "drums": rows,
            "muted": [false, false, true], "solo": [false, false, false],
            "notes": [{"start": 6, "length": 4, "pitch": 48, "group": 0}], "slides": []}], "clips": [{"pattern": 1, "bar": 2}]});
        let song = serde_json::from_value::<V1Song>(v).unwrap().migrate();
        song.validate().unwrap();
        let n = &song.patterns[0].notes;
        assert_eq!(n.len(), 4);
        assert_eq!(
            (n[0].instrument, n[0].start, n[0].length, n[0].velocity),
            (1, 0, STEP, 100)
        );
        assert_eq!(
            (n[1].instrument, n[1].start, n[1].velocity),
            (2, 4 * STEP, 90)
        );
        assert!(n[2].muted, "muted row keeps its steps as muted notes");
        assert_eq!(
            (n[3].instrument, n[3].start, n[3].length, n[3].velocity),
            (4, 720, 480, 100)
        );
        assert_eq!(song.clips[0].start, 2 * BAR);
        assert_eq!(song.patterns[0].length, BAR);
    }
}
