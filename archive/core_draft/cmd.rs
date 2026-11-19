//! Command stack undo: one finished gesture = one history entry.
//! Drag events sharing a gesture id coalesce instead of growing history.

use super::model::{Note, NoteId, PartId, PatternId, Project, Slide};

#[derive(Clone, Debug)]
pub enum Command {
    AddNote {
        pattern: PatternId,
        part: PartId,
        note: Note,
    },
    DeleteNotes {
        pattern: PatternId,
        part: PartId,
        notes: Vec<Note>,
        slides: Vec<Slide>,
    },
    MoveNotes {
        pattern: PatternId,
        part: PartId,
        ids: Vec<NoteId>,
        d_tick: super::time::Tick,
        d_pitch: i16,
        gesture: u64,
    },
    SetVelocity {
        pattern: PatternId,
        part: PartId,
        before: Vec<(NoteId, u8)>,
        after: Vec<(NoteId, u8)>,
        gesture: u64,
    },
    AddSlide {
        pattern: PatternId,
        part: PartId,
        slide: Slide,
    },
    MakeUnique {
        new_pattern: PatternId,
        snapshot: super::model::Pattern,
        clip_index: usize,
        old_pattern: PatternId,
    },
}

fn part_mut(project: &mut Project, pattern: PatternId, part: PartId) -> Option<&mut super::model::Part> {
    project
        .pattern_mut(pattern)
        .and_then(|p| p.parts.iter_mut().find(|pt| pt.id == part))
}

impl Command {
    fn apply(&self, project: &mut Project) {
        match self {
            Command::AddNote { pattern, part, note } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    pt.notes.insert(note.id, *note);
                }
            }
            Command::DeleteNotes { pattern, part, notes, slides } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for n in notes {
                        pt.notes.remove(&n.id);
                    }
                    for s in slides {
                        pt.slides.remove(&s.id);
                    }
                }
            }
            Command::MoveNotes { pattern, part, ids, d_tick, d_pitch, .. } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for id in ids {
                        if let Some(n) = pt.notes.get_mut(id) {
                            n.start += *d_tick;
                            n.pitch = (n.pitch as i16 + *d_pitch).clamp(0, 127) as u8;
                        }
                    }
                }
            }
            Command::SetVelocity { pattern, part, after, .. } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for (id, v) in after {
                        if let Some(n) = pt.notes.get_mut(id) {
                            n.velocity = *v;
                        }
                    }
                }
            }
            Command::AddSlide { pattern, part, slide } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    pt.slides.insert(slide.id, *slide);
                }
            }
            Command::MakeUnique { new_pattern, snapshot, clip_index, .. } => {
                let mut snap = snapshot.clone();
                snap.id = *new_pattern;
                project.patterns.push(snap);
                if let Some(clip) = project.playlist.get_mut(*clip_index) {
                    clip.pattern = *new_pattern;
                }
            }
        }
    }

    fn revert(&self, project: &mut Project) {
        match self {
            Command::AddNote { pattern, part, note } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    pt.notes.remove(&note.id);
                }
            }
            Command::DeleteNotes { pattern, part, notes, slides } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for n in notes {
                        pt.notes.insert(n.id, *n);
                    }
                    for s in slides {
                        pt.slides.insert(s.id, *s);
                    }
                }
            }
            Command::MoveNotes { pattern, part, ids, d_tick, d_pitch, .. } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for id in ids {
                        if let Some(n) = pt.notes.get_mut(id) {
                            n.start -= *d_tick;
                            n.pitch = (n.pitch as i16 - *d_pitch).clamp(0, 127) as u8;
                        }
                    }
                }
            }
            Command::SetVelocity { pattern, part, before, .. } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    for (id, v) in before {
                        if let Some(n) = pt.notes.get_mut(id) {
                            n.velocity = *v;
                        }
                    }
                }
            }
            Command::AddSlide { pattern, part, slide } => {
                if let Some(pt) = part_mut(project, *pattern, *part) {
                    pt.slides.remove(&slide.id);
                }
            }
            Command::MakeUnique { new_pattern, clip_index, old_pattern, .. } => {
                project.patterns.retain(|p| p.id != *new_pattern);
                if let Some(clip) = project.playlist.get_mut(*clip_index) {
                    clip.pattern = *old_pattern;
                }
            }
        }
    }

    /// Drag coalescing: same gesture id merges into the running entry.
    fn merge_into(&mut self, other: &Command) -> bool {
        match (self, other) {
            (
                Command::MoveNotes { d_tick: a, d_pitch: b, gesture: g1, .. },
                Command::MoveNotes { d_tick: c, d_pitch: d, gesture: g2, .. },
            ) if g1 == g2 => {
                *a += *c;
                *b += *d;
                true
            }
            (
                Command::SetVelocity { after: a, gesture: g1, .. },
                Command::SetVelocity { after: b, gesture: g2, .. },
            ) if g1 == g2 => {
                *a = b.clone();
                true
            }
            _ => false,
        }
    }
}

/// Own history record (avoids registry confusion around the `undo` crate name).
#[derive(Default, Debug)]
pub struct Record {
    done: Vec<Command>,
    undone: Vec<Command>,
    next_gesture: u64,
}

impl Record {
    pub fn begin_gesture(&mut self) -> u64 {
        self.next_gesture += 1;
        self.next_gesture
    }

    pub fn exec(&mut self, project: &mut Project, cmd: Command) {
        if let Some(last) = self.done.last_mut() {
            if last.merge_into(&cmd) {
                cmd.apply(project);
                self.undone.clear();
                return;
            }
        }
        cmd.apply(project);
        self.done.push(cmd);
        self.undone.clear();
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        if let Some(cmd) = self.done.pop() {
            cmd.revert(project);
            self.undone.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        if let Some(cmd) = self.undone.pop() {
            cmd.apply(project);
            self.done.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.done.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Instrument, InstrumentKind, Part, Pattern};
    use std::collections::BTreeMap;

    fn doc() -> (Project, PartId) {
        let mut p = Project {
            format: 2,
            ticks_per_beat: 480,
            tempo: 120.0,
            beats_per_bar: 4,
            instruments: vec![Instrument {
                id: 1,
                name: "Bass".into(),
                kind: InstrumentKind::Sampler { preset: "x".into() },
            }],
            patterns: vec![Pattern {
                id: 1,
                name: "P1".into(),
                len_ticks: 1920,
                parts: vec![Part {
                    id: 1,
                    instrument: 1,
                    notes: BTreeMap::new(),
                    slides: BTreeMap::new(),
                }],
            }],
            playlist: vec![],
            next_ids: BTreeMap::new(),
        };
        let _ = &mut p;
        (p, 1)
    }

    #[test]
    fn one_gesture_is_one_undo_and_drag_coalesces() {
        let (mut p, part) = doc();
        let mut h = Record::default();
        let g = h.begin_gesture();
        h.exec(
            &mut p,
            Command::AddNote {
                pattern: 1,
                part,
                note: Note {
                    id: 1,
                    start: 0,
                    len: 120,
                    pitch: 48,
                    velocity: 100,
                    muted: false,
                    slide_group: 0,
                    color: 0,
                    channel: 0,
                },
            },
        );
        for _ in 0..50 {
            h.exec(
                &mut p,
                Command::MoveNotes {
                    pattern: 1,
                    part,
                    ids: vec![1],
                    d_tick: 1,
                    d_pitch: 0,
                    gesture: g,
                },
            );
        }
        // 1 add + 1 coalesced move
        assert_eq!(h.len(), 2);
        assert!(h.undo(&mut p));
        let n = p.pattern(1).unwrap().parts[0].notes[&1];
        assert_eq!(n.start, 0);
        assert!(h.undo(&mut p));
        assert!(p.pattern(1).unwrap().parts[0].notes.is_empty());
        assert!(h.redo(&mut p));
        assert!(h.redo(&mut p));
        assert_eq!(p.pattern(1).unwrap().parts[0].notes[&1].start, 50);
    }
}
