//! Piano roll edits on a set of notes (FL: Tools → Quick legato, Chop, Glue,
//! Quantize). Pure functions: the caller picks the notes, applies the result
//! and makes the undo step.

use crate::sequence::{BAR, MAX_BARS, Note, Tick};

/// Ctrl+L: every note lasts until the next later start; the last ones keep
/// their length. Chord notes (same start) all reach the next chord.
pub fn legato(notes: &mut [Note]) -> usize {
    let mut starts: Vec<Tick> = notes.iter().map(|n| n.start).collect();
    starts.sort_unstable();
    starts.dedup();
    let mut changed = 0;
    for n in notes.iter_mut() {
        let next = starts.iter().copied().find(|&s| s > n.start);
        if let Some(next) = next
            && next - n.start != n.length
        {
            n.length = next - n.start;
            changed += 1;
        }
    }
    changed
}

/// Ctrl+U: a note longer than `grid` becomes pieces of `grid` (the last one
/// takes what is left). Pieces after the first have id 0: the caller gives ids.
pub fn chop(note: &Note, grid: Tick) -> Vec<Note> {
    if grid <= 0 || note.length <= grid {
        return vec![*note];
    }
    let mut out = Vec::new();
    let mut t = 0;
    while t < note.length {
        let len = grid.min(note.length - t);
        out.push(Note {
            id: if t == 0 { note.id } else { 0 },
            start: note.start + t,
            length: len,
            ..*note
        });
        t += len;
    }
    out
}

/// Ctrl+G: notes of one pitch that touch or overlap become one note (the
/// first keeps its id, velocity and the rest). Returns the notes to keep and
/// the ids that were merged away.
pub fn glue(notes: &[Note]) -> (Vec<Note>, Vec<u64>) {
    let mut sorted = notes.to_vec();
    sorted.sort_by_key(|n| (n.instrument, n.pitch, n.start));
    let mut keep: Vec<Note> = Vec::new();
    let mut gone = Vec::new();
    for n in sorted {
        if let Some(last) = keep.last_mut()
            && last.instrument == n.instrument
            && last.pitch == n.pitch
            && n.start <= last.end()
        {
            let end = last.end().max(n.end());
            last.length = end - last.start;
            gone.push(n.id);
            continue;
        }
        keep.push(n);
    }
    (keep, gone)
}

/// Alt+Q: move starts (and optionally ends) toward the grid by `strength`
/// 0..1. Returns how many notes changed.
pub fn quantize(notes: &mut [Note], grid: Tick, strength: f32, starts: bool, ends: bool) -> usize {
    if grid <= 1 {
        return 0;
    }
    let s = strength.clamp(0.0, 1.0) as f64;
    let toward = |t: Tick| -> Tick {
        let q = ((t as f64 / grid as f64).round() as Tick) * grid;
        t + ((q - t) as f64 * s).round() as Tick
    };
    let mut changed = 0;
    for n in notes.iter_mut() {
        let (a, b) = (n.start, n.end());
        let na = if starts { toward(a).max(0) } else { a };
        let nb = if ends { toward(b) } else { b - a + na };
        let len = (nb - na).max(if ends { grid.min(b - a).max(1) } else { 1 });
        if na != n.start || len != n.length {
            n.start = na;
            n.length = len;
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u64, start: Tick, length: Tick, pitch: u8) -> Note {
        Note {
            id,
            ..Note::new(1, start, length, pitch, 100)
        }
    }

    #[test]
    fn legato_reaches_the_next_start() {
        let mut v = vec![
            note(1, 0, 10, 60),
            note(2, 0, 10, 64),
            note(3, 240, 10, 62),
            note(4, 480, 10, 60),
        ];
        assert_eq!(legato(&mut v), 3);
        assert_eq!(
            (v[0].length, v[1].length, v[2].length, v[3].length),
            (240, 240, 240, 10)
        );
    }

    #[test]
    fn chop_and_glue_undo_each_other() {
        let n = note(7, 120, 250, 60);
        let parts = chop(&n, 120);
        assert_eq!(
            parts
                .iter()
                .map(|p| (p.start, p.length))
                .collect::<Vec<_>>(),
            vec![(120, 120), (240, 120), (360, 10)]
        );
        assert_eq!(parts[0].id, 7);
        let ids: Vec<Note> = parts
            .into_iter()
            .enumerate()
            .map(|(i, p)| Note {
                id: 7 + i as u64,
                ..p
            })
            .collect();
        let (keep, gone) = glue(&ids);
        assert_eq!(keep.len(), 1);
        assert_eq!((keep[0].id, keep[0].start, keep[0].length), (7, 120, 250));
        assert_eq!(gone, vec![8, 9]);
        // Different pitches, or a gap, stay apart.
        let (keep, _) = glue(&[
            note(1, 0, 100, 60),
            note(2, 100, 100, 61),
            note(3, 300, 10, 60),
        ]);
        assert_eq!(keep.len(), 3);
    }

    #[test]
    fn quantize_strength_and_ends() {
        let mut v = vec![note(1, 130, 100, 60)];
        assert_eq!(quantize(&mut v, 120, 0.5, true, false), 1);
        assert_eq!(
            (v[0].start, v[0].length),
            (125, 100),
            "half way to 120, length kept"
        );
        let mut v = vec![note(1, 130, 100, 60)];
        quantize(&mut v, 120, 1.0, true, true);
        assert_eq!(
            (v[0].start, v[0].length),
            (120, 120),
            "start and end on the grid"
        );
        assert_eq!(quantize(&mut v, 120, 1.0, true, true), 0);
    }
}

/// Spread simultaneous notes by pitch.
pub fn strum(notes: &mut [Note], step: Tick, down: bool) {
    let original = notes.to_vec();
    for n in notes {
        let rank = original
            .iter()
            .filter(|x| {
                x.start == n.start
                    && if down {
                        x.pitch > n.pitch
                    } else {
                        x.pitch < n.pitch
                    }
            })
            .count();
        n.start = n
            .start
            .saturating_add(step.max(0).saturating_mul(rank as Tick))
            .min((MAX_BARS * BAR - n.length).max(0));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArpDirection {
    Up,
    Down,
    UpDown,
}

/// Split each group of simultaneous notes into a repeating melody.
pub fn arpeggiate(notes: &[Note], rate: Tick, direction: ArpDirection) -> Vec<Note> {
    if rate <= 0 {
        return notes.to_vec();
    }
    let mut starts: Vec<_> = notes.iter().map(|n| n.start).collect();
    starts.sort_unstable();
    starts.dedup();
    let mut out = Vec::new();
    for start in starts {
        let mut chord: Vec<_> = notes.iter().filter(|n| n.start == start).copied().collect();
        chord.sort_by_key(|n| n.pitch);
        let end = chord.iter().map(Note::end).max().unwrap_or(start);
        let mut t = start;
        let mut index = 0usize;
        while t < end {
            let count = chord.len();
            let k = match direction {
                ArpDirection::Up => index % count,
                ArpDirection::Down => count - 1 - index % count,
                ArpDirection::UpDown if count > 1 => {
                    let x = index % (2 * count - 2);
                    if x < count { x } else { 2 * count - 2 - x }
                }
                ArpDirection::UpDown => 0,
            };
            let mut n = chord[k];
            n.id = 0;
            n.start = t;
            n.length = rate.min(end - t);
            out.push(n);
            t += rate;
            index += 1;
        }
    }
    out
}

/// Apply seeded variations within the requested bounds.
pub fn randomize(notes: &mut [Note], velocity: u8, ticks: Tick, seed: u64) {
    let mut state = seed;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state >> 32
    };
    let ticks = ticks.clamp(0, i32::MAX as Tick);
    for n in notes {
        let dv = (next() % (2 * velocity as u64 + 1)) as i64 - velocity as i64;
        let dt = (next() % (2 * ticks as u64 + 1)) as Tick - ticks;
        n.velocity = (n.velocity as i64 + dv).clamp(1, 127) as u8;
        n.start = n
            .start
            .saturating_add(dt)
            .clamp(0, (MAX_BARS * BAR - n.length).max(0));
    }
}

/// Mirror pitches or positions within the selection bounds.
pub fn flip(notes: &mut [Note], vertical: bool) {
    if notes.is_empty() {
        return;
    }
    if vertical {
        let lo = notes.iter().map(|n| n.pitch).min().unwrap() as u16;
        let hi = notes.iter().map(|n| n.pitch).max().unwrap() as u16;
        for n in notes {
            n.pitch = (lo + hi - n.pitch as u16) as u8;
        }
    } else {
        let lo = notes.iter().map(|n| n.start).min().unwrap();
        let hi = notes.iter().map(Note::end).max().unwrap();
        for n in notes {
            n.start = lo + (hi - n.end());
        }
    }
}

/// Clamp pitch and velocity to inclusive ranges.
pub fn limit(notes: &mut [Note], velocity: (u8, u8), pitch: (u8, u8)) {
    let (vl, vh) = (
        velocity.0.min(velocity.1).clamp(1, 127),
        velocity.0.max(velocity.1).clamp(1, 127),
    );
    let (pl, ph) = (pitch.0.min(pitch.1).min(127), pitch.0.max(pitch.1).min(127));
    for n in notes {
        n.velocity = n.velocity.clamp(vl, vh);
        n.pitch = n.pitch.clamp(pl, ph);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    fn chord() -> Vec<Note> {
        vec![
            Note::new(1, 10, 40, 60, 100),
            Note::new(1, 10, 40, 64, 100),
            Note::new(1, 10, 40, 67, 100),
        ]
    }
    #[test]
    fn strum_both_ways() {
        let mut n = chord();
        strum(&mut n, 5, false);
        assert_eq!(n.iter().map(|n| n.start).collect::<Vec<_>>(), [10, 15, 20]);
        let mut n = chord();
        strum(&mut n, 5, true);
        assert_eq!(n[0].start, 20);
    }
    #[test]
    fn arp_orders_and_lengths() {
        let n = chord();
        for (d, pitches) in [
            (ArpDirection::Up, vec![60, 64, 67, 60]),
            (ArpDirection::Down, vec![67, 64, 60, 67]),
            (ArpDirection::UpDown, vec![60, 64, 67, 64]),
        ] {
            let a = arpeggiate(&n, 10, d);
            assert_eq!(a.iter().map(|n| n.pitch).collect::<Vec<_>>(), pitches);
            assert_eq!(a.last().unwrap().end(), 50);
        }
    }
    #[test]
    fn random_is_seeded_and_bounded() {
        let mut a = chord();
        let mut b = a.clone();
        randomize(&mut a, 10, 5, 42);
        randomize(&mut b, 10, 5, 42);
        assert_eq!(
            a.iter().map(|n| (n.start, n.velocity)).collect::<Vec<_>>(),
            b.iter().map(|n| (n.start, n.velocity)).collect::<Vec<_>>()
        );
        assert!(
            a.iter()
                .all(|n| (5..=15).contains(&n.start) && (90..=110).contains(&n.velocity))
        );
    }
    #[test]
    fn flip_twice_restores() {
        let mut a = chord();
        a[0].start = 0;
        let orig: Vec<_> = a.iter().map(|n| (n.start, n.pitch)).collect();
        flip(&mut a, true);
        flip(&mut a, false);
        flip(&mut a, true);
        flip(&mut a, false);
        assert_eq!(
            a.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(),
            orig
        );
    }
    #[test]
    fn limits_ranges() {
        let mut n = chord();
        limit(&mut n, (50, 80), (62, 65));
        assert_eq!(
            n.iter().map(|n| (n.pitch, n.velocity)).collect::<Vec<_>>(),
            [(62, 80), (64, 80), (65, 80)]
        );
    }
}
