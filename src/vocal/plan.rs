//! The song's vocal notes grouped into phrases.

use super::{Params, RNote};
use crate::sequence::{InstrumentKind, Song, seconds_per_step, steps, swing_offset};
use std::hash::Hasher;

/// One phrase to render, with what it depends on hashed into `key`.
#[derive(Clone, Debug)]
pub struct Spec {
    pub inst: u64,
    pub bank: String,
    pub params: Params,
    pub notes: Vec<RNote>,
    pub key: u64,
}

struct Fnv(u64);
impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
}

fn hash_f(h: &mut Fnv, v: f64) {
    h.write(&v.to_bits().to_le_bytes());
}

/// Phrases are cut where a rest is longer than this (seconds).
pub const GAP: f64 = 0.03;

pub fn plan(song: &Song, rate: u32) -> Vec<Spec> {
    let sps = seconds_per_step(song.tempo);
    let swing = song.swing.clamp(0.0, 0.6);
    let time = |t: crate::sequence::Tick| steps(t) * sps + swing_offset(t, sps, swing);
    let mut out = Vec::new();
    for inst in &song.instruments {
        let InstrumentKind::Vocal { voice } = &inst.kind else {
            continue;
        };
        if inst.muted {
            continue;
        }
        for pat in &song.patterns {
            let mut notes: Vec<&crate::sequence::Note> = pat
                .notes
                .iter()
                .filter(|n| n.instrument == inst.id && !n.muted && pat.automation.is_none())
                .collect();
            notes.sort_by_key(|n| (n.start, n.id));
            // One voice: a note ends where the next begins.
            let mut r: Vec<RNote> = Vec::new();
            for (i, n) in notes.iter().enumerate() {
                if let Some(prev) = i.checked_sub(1).map(|j| notes[j])
                    && prev.start == n.start
                {
                    continue;
                }
                let end_tick = notes
                    .iter()
                    .skip(i + 1)
                    .find(|m| m.start > n.start)
                    .map(|m| m.start.min(n.end()))
                    .unwrap_or(n.end());
                r.push(RNote {
                    id: n.id,
                    start: time(n.start),
                    end: time(end_tick),
                    pitch: n.pitch as f64 + n.fine as f64 / 100.0,
                    velocity: n.velocity,
                    lyric: pat
                        .lyrics
                        .get(&n.id)
                        .cloned()
                        .unwrap_or_else(|| voice.default_lyric.clone()),
                    vibrato: (n.mod_x as f64 * 2.0).clamp(0.0, 2.0),
                    gender: ((n.mod_y as f64 - 0.5) * 2.0).clamp(-1.0, 1.0),
                });
            }
            let mut start = 0;
            for i in 1..=r.len() {
                if i == r.len() || r[i].start - r[i - 1].end > GAP {
                    let notes = r[start..i].to_vec();
                    start = i;
                    if notes.is_empty() {
                        continue;
                    }
                    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
                    h.write(voice.bank.as_bytes());
                    h.write(&rate.to_le_bytes());
                    let p = &voice.params;
                    for v in [
                        p.vibrato_depth,
                        p.vibrato_rate,
                        p.vibrato_delay,
                        p.vibrato_fade,
                        p.transition,
                        p.gender,
                        p.breath,
                    ] {
                        hash_f(&mut h, v);
                    }
                    // Times relative to the first note: moving a phrase as a whole renders nothing again.
                    let base = notes[0].start;
                    for n in &notes {
                        hash_f(&mut h, n.start - base);
                        hash_f(&mut h, n.end - base);
                        hash_f(&mut h, n.pitch);
                        h.write(&[n.velocity]);
                        h.write(n.lyric.as_bytes());
                        hash_f(&mut h, n.vibrato);
                        hash_f(&mut h, n.gender);
                    }
                    out.push(Spec {
                        inst: inst.id,
                        bank: voice.bank.clone(),
                        params: p.clone(),
                        notes,
                        key: h.finish(),
                    });
                }
            }
        }
    }
    out
}

/// One preview per key for every singing channel: the default lyric held on that pitch.
pub fn previews(song: &Song, rate: u32) -> Vec<(u64, u8, u64, Spec)> {
    let mut out = Vec::new();
    for inst in &song.instruments {
        let InstrumentKind::Vocal { voice } = &inst.kind else {
            continue;
        };
        if voice.bank.is_empty() {
            continue;
        }
        for pitch in super::PREVIEW_PITCHES {
            let note = RNote {
                id: 0,
                start: 0.0,
                end: super::PREVIEW_LEN,
                pitch: pitch as f64,
                velocity: 100,
                lyric: voice.default_lyric.clone(),
                vibrato: 1.0,
                gender: 0.0,
            };
            let mut h = Fnv(0x9e37_79b9_7f4a_7c15);
            h.write(voice.bank.as_bytes());
            h.write(&rate.to_le_bytes());
            h.write(voice.default_lyric.as_bytes());
            h.write(&[pitch]);
            let p = &voice.params;
            for v in [
                p.vibrato_depth,
                p.vibrato_rate,
                p.vibrato_delay,
                p.vibrato_fade,
                p.transition,
                p.gender,
                p.breath,
            ] {
                hash_f(&mut h, v);
            }
            out.push((
                inst.id,
                pitch,
                0,
                Spec {
                    inst: inst.id,
                    bank: voice.bank.clone(),
                    params: p.clone(),
                    notes: vec![note],
                    key: h.finish(),
                },
            ));
        }
        // Each written note also previews its own syllable at its own pitch (a click on it sings that).
        let mut seen = std::collections::HashSet::new();
        for pat in &song.patterns {
            for n in pat.notes.iter().filter(|n| n.instrument == inst.id) {
                let Some(l) = pat.lyrics.get(&n.id) else {
                    continue;
                };
                let lh = super::lyric_hash(l);
                if out.len() > 600
                    || !seen.insert((n.pitch, lh))
                    || (super::PREVIEW_PITCHES.contains(&n.pitch) && *l == voice.default_lyric)
                {
                    continue;
                }
                let note = RNote {
                    id: 0,
                    start: 0.0,
                    end: super::PREVIEW_LEN,
                    pitch: n.pitch as f64,
                    velocity: 100,
                    lyric: l.clone(),
                    vibrato: 1.0,
                    gender: 0.0,
                };
                let mut h = Fnv(0x9e37_79b9_7f4a_7c15);
                h.write(voice.bank.as_bytes());
                h.write(&rate.to_le_bytes());
                h.write(l.as_bytes());
                h.write(&[n.pitch]);
                let p = &voice.params;
                for v in [
                    p.vibrato_depth,
                    p.vibrato_rate,
                    p.vibrato_delay,
                    p.vibrato_fade,
                    p.transition,
                    p.gender,
                    p.breath,
                ] {
                    hash_f(&mut h, v);
                }
                out.push((
                    inst.id,
                    n.pitch,
                    lh,
                    Spec {
                        inst: inst.id,
                        bank: voice.bank.clone(),
                        params: p.clone(),
                        notes: vec![note],
                        key: h.finish(),
                    },
                ));
            }
        }
    }
    out
}
