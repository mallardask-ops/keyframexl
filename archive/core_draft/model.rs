//! Project document model (format 2). All musical time in [`crate::core::time::Tick`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::time::Tick;

pub type PatternId = u64;
pub type PartId = u64;
pub type NoteId = u64;
pub type SlideId = u64;
pub type ClipId = u64;
pub type InstrumentId = u64;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Project {
    pub format: u32,
    pub ticks_per_beat: i64,
    pub tempo: f64,
    pub beats_per_bar: u8,
    pub instruments: Vec<Instrument>,
    pub patterns: Vec<Pattern>,
    pub playlist: Vec<Clip>,
    pub next_ids: BTreeMap<String, u64>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Instrument {
    pub id: InstrumentId,
    pub name: String,
    pub kind: InstrumentKind,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum InstrumentKind {
    Drum { row: u8 },
    Sampler { preset: String },
    Plugin { uri: String },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Pattern {
    pub id: PatternId,
    pub name: String,
    pub len_ticks: Tick,
    pub parts: Vec<Part>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Part {
    pub id: PartId,
    pub instrument: InstrumentId,
    pub notes: BTreeMap<NoteId, Note>,
    pub slides: BTreeMap<SlideId, Slide>,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub start: Tick,
    pub len: Tick,
    pub pitch: u8,
    pub velocity: u8,
    pub muted: bool,
    pub slide_group: u16,
    pub color: u8,
    pub channel: u8,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Slide {
    pub id: SlideId,
    pub start: Tick,
    pub len: Tick,
    pub target: u8,
    pub group: u16,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Clip {
    pub id: ClipId,
    pub pattern: PatternId,
    pub start_ticks: Tick,
}

impl Clip {
    pub fn bar(&self, beats_per_bar: i64, ppq: i64) -> i64 {
        self.start_ticks / (beats_per_bar * ppq)
    }
}

impl Project {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.format != 2 {
            return Err("unsupported format");
        }
        if ![96, 192, 240, 384, 480, 960].contains(&self.ticks_per_beat) {
            return Err("bad ticks_per_beat");
        }
        let mut patterns = std::collections::BTreeSet::new();
        for p in &self.patterns {
            if !patterns.insert(p.id) {
                return Err("duplicate pattern id");
            }
            if p.len_ticks <= 0 {
                return Err("bad pattern length");
            }
            for part in &p.parts {
                for n in part.notes.values() {
                    if n.pitch > 127 {
                        return Err("bad pitch");
                    }
                    if !(1..=127).contains(&n.velocity) {
                        return Err("bad velocity");
                    }
                    if n.start < 0 || n.len <= 0 || n.start + n.len > p.len_ticks {
                        return Err("note out of range");
                    }
                }
                for s in part.slides.values() {
                    if s.target > 127 || s.len <= 0 || s.start < 0 {
                        return Err("bad slide");
                    }
                }
            }
        }
        for c in &self.playlist {
            if !patterns.contains(&c.pattern) {
                return Err("clip references missing pattern");
            }
        }
        Ok(())
    }

    pub fn pattern(&self, id: PatternId) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }

    pub fn pattern_mut(&mut self, id: PatternId) -> Option<&mut Pattern> {
        self.patterns.iter_mut().find(|p| p.id == id)
    }

    pub fn alloc(&mut self, key: &str) -> u64 {
        let next = self.next_ids.get(key).copied().unwrap_or(1);
        self.next_ids.insert(key.into(), next + 1);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project {
            format: 2,
            ticks_per_beat: 480,
            tempo: 120.0,
            beats_per_bar: 4,
            instruments: vec![],
            patterns: vec![Pattern {
                id: 1,
                name: "P1".into(),
                len_ticks: 1920,
                parts: vec![],
            }],
            playlist: vec![],
            next_ids: BTreeMap::new(),
        }
    }

    #[test]
    fn invalid_rejected() {
        let mut p = project();
        p.playlist.push(Clip {
            id: 1,
            pattern: 999,
            start_ticks: 0,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn clip_bar_math() {
        let c = Clip {
            id: 1,
            pattern: 1,
            start_ticks: 1920,
        };
        assert_eq!(c.bar(4, 480), 1);
    }
}
