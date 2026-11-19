//! Musical time in integer ticks. Internal canon: 480 ticks per beat.

/// Ticks per beat (PPQ). Divisible by 2,3,4,5,6,8; exact import of 96/240/480.
pub const TICKS_PER_BEAT: i64 = 480;
/// Musical time. Non-negative in the document; negative only in take buffers.
pub type Tick = i64;

/// Ticks of one 1/16 step (old `start: u8` unit).
pub fn step_ticks() -> Tick {
    TICKS_PER_BEAT / 4
}

/// Ticks of one bar.
pub fn bar_ticks(beats_per_bar: i64) -> Tick {
    beats_per_bar * TICKS_PER_BEAT
}

/// Round to the grid; grid <= 0 returns input unchanged.
pub fn quantize(t: Tick, grid: Tick) -> Tick {
    if grid <= 0 {
        return t;
    }
    ((t + grid / 2) / grid) * grid
}

/// Convert ticks to beats at the given resolution.
pub fn ticks_to_beats(t: Tick, ppq: i64) -> f64 {
    t as f64 / ppq as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_is_120_ticks() {
        assert_eq!(step_ticks(), 120);
    }

    #[test]
    fn triplet_eighth_is_160_ticks() {
        // 1/8 triplet = quarter / 3
        assert_eq!(TICKS_PER_BEAT / 3, 160);
    }

    #[test]
    fn quantize_rounds_to_grid() {
        assert_eq!(quantize(7, 120), 0);
        assert_eq!(quantize(70, 120), 120);
        assert_eq!(quantize(-5, 0), -5);
    }
}
