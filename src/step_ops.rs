//! Step-sequencer edits for one channel (FL: channel rack step tools).
//! Pure functions: the caller picks one instrument's notes, applies the
//! result and makes the undo step.

use crate::sequence::{BAR, MAX_BARS, Note, STEP, STEP_PITCH, Tick};

/// Step index of a tick, or `None` when it is off the grid or past the end.
fn step_of(start: Tick, step_count: usize) -> Option<usize> {
    if step_count == 0 || start < 0 || start % STEP != 0 {
        return None;
    }
    let s = (start / STEP) as usize;
    (s < step_count).then_some(s)
}

/// Move every step note one step left (`right == false`) or right, wrapping
/// at the pattern ends. Notes off the grid stay where they are.
pub fn shift(notes: &mut [Note], step_count: usize, right: bool) {
    if step_count == 0 {
        return;
    }
    for n in notes.iter_mut() {
        let Some(s) = step_of(n.start, step_count) else {
            continue;
        };
        let t = if right {
            (s + 1) % step_count
        } else {
            (s + step_count - 1) % step_count
        };
        n.start = t as Tick * STEP;
    }
}

/// Write a note on every Nth step, keeping the notes already there (their
/// velocities stay). New notes use `velocity` at the step pitch.
pub fn fill_every(
    notes: &[Note],
    step_count: usize,
    every: usize,
    instrument: u64,
    velocity: u8,
) -> Vec<Note> {
    if every == 0 {
        return notes.to_vec();
    }
    let mut out = notes.to_vec();
    for s in (0..step_count).step_by(every) {
        if !out.iter().any(|n| n.start == s as Tick * STEP) {
            out.push(Note::new(
                instrument,
                s as Tick * STEP,
                STEP,
                STEP_PITCH,
                velocity,
            ));
        }
    }
    out.sort_by_key(|n| n.start);
    out
}

/// Steps with notes go quiet, empty steps get a note at `velocity`.
pub fn invert(notes: &[Note], step_count: usize, instrument: u64, velocity: u8) -> Vec<Note> {
    let mut out = Vec::new();
    for s in 0..step_count {
        if !notes.iter().any(|n| n.start == s as Tick * STEP) {
            out.push(Note::new(
                instrument,
                s as Tick * STEP,
                STEP,
                STEP_PITCH,
                velocity,
            ));
        }
    }
    out
}

/// Drop every note in the slice. Returns how many went away.
pub fn clear(notes: &mut Vec<Note>) -> usize {
    let n = notes.len();
    notes.clear();
    n
}

/// Seeded velocity ±`amount` and start ±`ticks` (same LCG as
/// `note_ops::randomize`, so the same seed gives the same result).
/// Velocity stays in 1..127, starts at tick 0 or later. The rack UI
/// defaults `ticks` to 0 so steps stay on the grid.
pub fn humanize(notes: &mut [Note], amount: u8, ticks: Tick, seed: u64) {
    let mut state = seed;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040887963407);
        state >> 32
    };
    let ticks = ticks.clamp(0, i32::MAX as Tick);
    for n in notes {
        let dv = (next() % (2 * amount as u64 + 1)) as i64 - amount as i64;
        let dt = (next() % (2 * ticks as u64 + 1)) as Tick - ticks;
        n.velocity = (n.velocity as i64 + dv).clamp(1, 127) as u8;
        n.start = n
            .start
            .saturating_add(dt)
            .clamp(0, (MAX_BARS * BAR - n.length).max(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: u64, s: Tick, vel: u8) -> Note {
        Note {
            id,
            ..Note::new(1, s * STEP, STEP, STEP_PITCH, vel)
        }
    }

    #[test]
    fn shift_wraps_at_both_ends() {
        let mut v = vec![step(1, 0, 100), step(2, 3, 90)];
        shift(&mut v, 4, true);
        assert_eq!((v[0].start, v[1].start), (STEP, 0));
        shift(&mut v, 4, false);
        assert_eq!((v[0].start, v[1].start), (0, 3 * STEP));
        assert_eq!((v[0].velocity, v[1].velocity), (100, 90));
    }

    #[test]
    fn shift_leaves_off_grid_notes_alone() {
        let mut v = vec![Note {
            id: 1,
            ..Note::new(1, 10, STEP, 60, 100)
        }];
        shift(&mut v, 16, true);
        assert_eq!(v[0].start, 10);
    }

    #[test]
    fn fill_keeps_existing_velocities() {
        let v = vec![step(1, 0, 70)];
        let out = fill_every(&v, 8, 2, 1, 100);
        assert_eq!(
            out.iter().map(|n| n.start).collect::<Vec<_>>(),
            vec![0, 2 * STEP, 4 * STEP, 6 * STEP]
        );
        assert_eq!(out[0].velocity, 70, "painted velocity stays");
        assert!(out[1..].iter().all(|n| n.velocity == 100));
    }

    #[test]
    fn invert_flips_present_and_missing() {
        let v = vec![step(1, 0, 100), step(2, 2, 100)];
        let out = invert(&v, 4, 1, 100);
        assert_eq!(
            out.iter().map(|n| n.start).collect::<Vec<_>>(),
            vec![STEP, 3 * STEP]
        );
    }

    #[test]
    fn clear_reports_and_empties() {
        let mut v = vec![step(1, 0, 100), step(2, 4, 90)];
        assert_eq!(clear(&mut v), 2);
        assert!(v.is_empty());
    }

    #[test]
    fn humanize_same_seed_same_result() {
        let mut a = vec![step(1, 0, 100), step(2, 4, 100)];
        let mut b = a.clone();
        humanize(&mut a, 15, 6, 42);
        humanize(&mut b, 15, 6, 42);
        assert_eq!(
            a.iter().map(|n| (n.start, n.velocity)).collect::<Vec<_>>(),
            b.iter().map(|n| (n.start, n.velocity)).collect::<Vec<_>>()
        );
        assert!(a.iter().all(|n| (85..=115).contains(&n.velocity)));
        assert!(
            a.iter()
                .all(|n| n.start >= 0 && (n.start - (n.start / STEP) * STEP).abs() <= 6)
        );
    }

    #[test]
    fn humanize_zero_amount_changes_nothing() {
        let mut v = vec![step(1, 0, 100)];
        let before = v.clone();
        humanize(&mut v, 0, 0, 7);
        assert_eq!(v, before);
    }
}
