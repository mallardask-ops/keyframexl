//! Selection + clipboard (ID-based, never index-based).

use std::collections::BTreeSet;

use super::model::{Note, NoteId, PatternId, PartId, Slide, SlideId};
use super::time::Tick;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Selection {
    pub pattern: Option<PatternId>,
    pub part: Option<PartId>,
    pub notes: BTreeSet<NoteId>,
    pub slides: BTreeSet<SlideId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clipboard {
    pub anchor_tick: Tick,
    pub anchor_pitch: u8,
    pub notes: Vec<Note>,
    pub slides: Vec<Slide>,
}
