//! Pure tap-tempo measurement: averaging window, median-based outlier
//! rejection, bounce coalescing, timeout auto-reset, and BPM rounding.

use std::collections::VecDeque;

/// Lowest BPM KeyFrameXL supports.
pub const MIN_BPM: f64 = 40.0;
/// Highest BPM KeyFrameXL supports.
pub const MAX_BPM: f64 = 300.0;
/// Pause longer than this (in seconds) resets the tap sequence automatically.
/// 2.2 s is comfortably above a 40 BPM beat (1.5 s) while resetting quickly.
pub const DEFAULT_TIMEOUT_S: f64 = 2.2;
/// Intervals shorter than 80 ms (>750 BPM) are ignored to coalesce simultaneous
/// MIDI chord notes and switch bounce.
pub const MIN_INTERVAL_S: f64 = 0.08;
/// Intervals deviating by more than 35% from the window median are ignored as
/// outliers (e.g. a missed beat = +100% or accidental double-tap = -50%).
pub const OUTLIER_REL_THRESHOLD: f64 = 0.35;

/// How many recent tap intervals contribute to the averaged BPM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvgWindow {
    Four,
    #[default]
    Eight,
    Sixteen,
}

impl AvgWindow {
    pub const ALL: [Self; 3] = [Self::Four, Self::Eight, Self::Sixteen];
    pub const MAX_INTERVALS: usize = 16;

    pub const fn count(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Four => 0,
            Self::Eight => 1,
            Self::Sixteen => 2,
        }
    }

    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Four,
            2 => Self::Sixteen,
            _ => Self::Eight,
        }
    }
}

/// Output precision of the measured BPM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rounding {
    Integer,
    #[default]
    OneDecimal,
}

impl Rounding {
    pub fn apply(self, bpm: f64) -> f64 {
        let clamped = bpm.clamp(MIN_BPM, MAX_BPM);
        match self {
            Self::Integer => clamped.round(),
            Self::OneDecimal => (clamped * 10.0).round() / 10.0,
        }
    }

    pub fn format(self, bpm: f64) -> String {
        match self {
            Self::Integer => format!("{:.0}", self.apply(bpm)),
            Self::OneDecimal => format!("{:.1}", self.apply(bpm)),
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Integer => 0,
            Self::OneDecimal => 1,
        }
    }

    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Integer,
            _ => Self::OneDecimal,
        }
    }
}

/// Clock domain of a tap event. Consecutive taps from the same clock use its
/// native high-resolution timestamps (e.g. per-port MIDI microsecond stamps);
/// taps across different clocks fall back to the shared wall clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TapClock {
    Ui,
    MidiPort(String),
}

/// One recorded tap interval for UI inspection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntervalSample {
    pub seconds: f64,
    pub bpm: f64,
    pub outlier: bool,
}

/// Outcome of feeding one tap into [`TapMeter`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapResult {
    /// False when the tap was coalesced as bounce / simultaneous chord note.
    pub accepted: bool,
    /// True when this tap started a fresh sequence (first tap or after timeout).
    pub started_new_run: bool,
    /// Rounded BPM after this tap, if at least one interval has been measured.
    pub bpm: Option<f64>,
}

/// Compute average BPM and per-interval outlier flags from a slice of positive
/// tap intervals (in seconds).
///
/// With 3 or more intervals, any interval whose relative deviation from the
/// median exceeds [`OUTLIER_REL_THRESHOLD`] is excluded from the mean.
pub fn compute_bpm(intervals: &[f64]) -> Option<(f64, Vec<bool>)> {
    if intervals.is_empty() {
        return None;
    }
    let len = intervals.len();
    if len < 3 {
        let mean = intervals.iter().sum::<f64>() / len as f64;
        if mean <= 0.0 || !mean.is_finite() {
            return None;
        }
        return Some((60.0 / mean, vec![false; len]));
    }
    let mut sorted = intervals.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let med = if len.is_multiple_of(2) {
        0.5 * (sorted[len / 2 - 1] + sorted[len / 2])
    } else {
        sorted[len / 2]
    };
    if med <= 0.0 || !med.is_finite() {
        return None;
    }
    let mut outliers = Vec::with_capacity(len);
    let mut sum = 0.0;
    let mut kept = 0usize;
    for &dt in intervals {
        let is_outlier = (dt - med).abs() / med > OUTLIER_REL_THRESHOLD;
        outliers.push(is_outlier);
        if !is_outlier {
            sum += dt;
            kept += 1;
        }
    }
    let avg_dt = if kept > 0 { sum / kept as f64 } else { med };
    Some((60.0 / avg_dt, outliers))
}

/// Stateful tap-tempo tracker.
#[derive(Clone, Debug)]
pub struct TapMeter {
    pub timeout_s: f64,
    pub window: AvgWindow,
    pub rounding: Rounding,
    tap_count: usize,
    last_clock: Option<(TapClock, f64)>,
    last_wall_s: Option<f64>,
    intervals: VecDeque<f64>,
}

impl Default for TapMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl TapMeter {
    pub fn new() -> Self {
        Self {
            timeout_s: DEFAULT_TIMEOUT_S,
            window: AvgWindow::default(),
            rounding: Rounding::default(),
            tap_count: 0,
            last_clock: None,
            last_wall_s: None,
            intervals: VecDeque::with_capacity(AvgWindow::MAX_INTERVALS),
        }
    }

    /// Clear all recorded taps and intervals.
    pub fn reset(&mut self) {
        self.tap_count = 0;
        self.last_clock = None;
        self.last_wall_s = None;
        self.intervals.clear();
    }

    /// Reset automatically if more than `timeout_s` has elapsed since the last tap.
    /// Returns `true` if a reset occurred.
    pub fn expire(&mut self, wall_now_s: f64) -> bool {
        if self
            .last_wall_s
            .is_some_and(|prev| wall_now_s - prev > self.timeout_s)
        {
            self.reset();
            true
        } else {
            false
        }
    }

    /// Record a tap on the UI clock at `time_s` seconds and return the measured BPM.
    pub fn tap(&mut self, time_s: f64) -> Option<f64> {
        self.tap_with_clock(TapClock::Ui, time_s, time_s).bpm
    }

    /// Record a tap with both a source-native clock (`clock`, `clock_s`) and a
    /// unified wall clock (`wall_s`).
    pub fn tap_with_clock(&mut self, clock: TapClock, clock_s: f64, wall_s: f64) -> TapResult {
        let mut started_new_run = false;
        if self.expire(wall_s) {
            started_new_run = true;
        }
        if self.tap_count == 0 {
            self.tap_count = 1;
            self.last_clock = Some((clock, clock_s));
            self.last_wall_s = Some(wall_s);
            return TapResult {
                accepted: true,
                started_new_run: true,
                bpm: None,
            };
        }
        let dt = match (&self.last_clock, self.last_wall_s) {
            (Some((prev_clock, prev_clock_s)), _)
                if *prev_clock == clock && clock_s > *prev_clock_s =>
            {
                clock_s - *prev_clock_s
            }
            (_, Some(prev_wall_s)) => wall_s - prev_wall_s,
            _ => 0.0,
        };
        if !dt.is_finite() || dt < MIN_INTERVAL_S {
            return TapResult {
                accepted: false,
                started_new_run: false,
                bpm: self.bpm(),
            };
        }
        if dt > self.timeout_s {
            self.reset();
            self.tap_count = 1;
            self.last_clock = Some((clock, clock_s));
            self.last_wall_s = Some(wall_s);
            return TapResult {
                accepted: true,
                started_new_run: true,
                bpm: None,
            };
        }
        if self.intervals.len() == AvgWindow::MAX_INTERVALS {
            self.intervals.pop_front();
        }
        self.intervals.push_back(dt);
        self.tap_count += 1;
        self.last_clock = Some((clock, clock_s));
        self.last_wall_s = Some(wall_s);
        TapResult {
            accepted: true,
            started_new_run,
            bpm: self.bpm(),
        }
    }

    fn active_intervals(&self) -> Vec<f64> {
        let take = self.window.count().min(self.intervals.len());
        let skip = self.intervals.len() - take;
        self.intervals.iter().skip(skip).copied().collect()
    }

    /// Unrounded, unclamped BPM from the active averaging window.
    pub fn bpm_raw(&self) -> Option<f64> {
        let iv = self.active_intervals();
        compute_bpm(&iv).map(|(bpm, _)| bpm)
    }

    /// Clamped and rounded BPM ready to display or apply.
    pub fn bpm(&self) -> Option<f64> {
        self.bpm_raw().map(|b| self.rounding.apply(b))
    }

    /// Recent intervals in the active window with outlier annotations for UI display.
    pub fn recent_samples(&self) -> Vec<IntervalSample> {
        let iv = self.active_intervals();
        let Some((_, outliers)) = compute_bpm(&iv) else {
            return Vec::new();
        };
        iv.into_iter()
            .zip(outliers)
            .map(|(seconds, outlier)| IntervalSample {
                seconds,
                bpm: (60.0 / seconds).clamp(MIN_BPM, MAX_BPM),
                outlier,
            })
            .collect()
    }

    pub fn tap_count(&self) -> usize {
        self.tap_count
    }

    pub fn last_tap_wall_s(&self) -> Option<f64> {
        self.last_wall_s
    }

    /// Short-lived visual pulse (1.0 → 0.0 over 180 ms) right after a tap.
    pub fn beat_flash(&self, wall_now_s: f64) -> f32 {
        const FLASH_S: f64 = 0.18;
        let Some(last) = self.last_wall_s else {
            return 0.0;
        };
        let dt = wall_now_s - last;
        if !(0.0..FLASH_S).contains(&dt) {
            return 0.0;
        }
        (1.0 - (dt / FLASH_S) as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_taps_measure_exact_bpm() {
        let mut m = TapMeter::new();
        assert_eq!(m.tap(1.0), None);
        assert_eq!(m.tap_count(), 1);
        assert_eq!(m.tap(1.5), Some(120.0));
        assert_eq!(m.tap(2.0), Some(120.0));
        assert_eq!(m.tap(2.5), Some(120.0));
        assert_eq!(m.tap_count(), 4);
    }

    #[test]
    fn outlier_rejection_ignores_missed_beat_and_double_tap() {
        // Missed beat (1.0 s) amidst 120 BPM (0.5 s) taps.
        let (bpm, flags) = compute_bpm(&[0.5, 0.5, 1.0, 0.5]).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9);
        assert_eq!(flags, vec![false, false, true, false]);

        // Accidental half-beat double-tap (0.25 s) amidst 120 BPM (0.5 s) taps.
        let (bpm, flags) = compute_bpm(&[0.5, 0.25, 0.5, 0.5]).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9);
        assert_eq!(flags, vec![false, true, false, false]);

        // Normal human jitter (±4%) is averaged rather than rejected.
        let (bpm, flags) = compute_bpm(&[0.48, 0.52, 0.49, 0.51]).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9);
        assert_eq!(flags, vec![false, false, false, false]);
    }

    #[test]
    fn timeout_resets_sequence_automatically() {
        let mut m = TapMeter::new();
        m.tap(1.0);
        m.tap(1.5);
        m.tap(2.0);
        assert_eq!(m.bpm(), Some(120.0));

        // Still within timeout (2.2 s) -> does not expire.
        assert!(!m.expire(4.0));
        assert_eq!(m.tap_count(), 3);

        // Past timeout -> expires cleanly.
        assert!(m.expire(4.5));
        assert_eq!(m.tap_count(), 0);
        assert_eq!(m.bpm(), None);

        // Or a new tap arriving after the pause starts a fresh run.
        let r1 = m.tap_with_clock(TapClock::Ui, 10.0, 10.0);
        assert!(r1.accepted && r1.started_new_run);
        assert_eq!(r1.bpm, None);
        let r2 = m.tap_with_clock(TapClock::Ui, 10.6, 10.6);
        assert!(r2.accepted && !r2.started_new_run);
        assert_eq!(r2.bpm, Some(100.0));
        let r3 = m.tap_with_clock(TapClock::Ui, 13.5, 13.5);
        assert!(r3.accepted && r3.started_new_run);
        assert_eq!(r3.bpm, None);
        assert_eq!(m.tap_count(), 1);
    }

    #[test]
    fn debounce_coalesces_simultaneous_chord_notes() {
        let mut m = TapMeter::new();
        assert!(m.tap_with_clock(TapClock::Ui, 1.000, 1.000).accepted);
        // Second note of a chord 12 ms later is ignored.
        let chord = m.tap_with_clock(TapClock::Ui, 1.012, 1.012);
        assert!(!chord.accepted);
        assert_eq!(m.tap_count(), 1);
        // Next beat 500 ms after the first note measures 120 BPM.
        let next = m.tap_with_clock(TapClock::Ui, 1.500, 1.500);
        assert!(next.accepted);
        assert_eq!(next.bpm, Some(120.0));
    }

    #[test]
    fn averaging_window_and_rounding_update_live() {
        let mut m = TapMeter::new();
        // Start with four 100 BPM intervals (0.6 s), then four 125.4 BPM intervals.
        let mut t = 0.0;
        m.tap(t);
        for _ in 0..4 {
            t += 0.6;
            m.tap(t);
        }
        let dt_fast = 60.0 / 125.4;
        for _ in 0..4 {
            t += dt_fast;
            m.tap(t);
        }
        // Window of 4 sees only the four 125.4 BPM intervals.
        m.window = AvgWindow::Four;
        m.rounding = Rounding::OneDecimal;
        assert_eq!(m.bpm(), Some(125.4));
        m.rounding = Rounding::Integer;
        assert_eq!(m.bpm(), Some(125.0));

        // Window of 8 mixes the slower and faster runs.
        m.window = AvgWindow::Eight;
        let bpm8 = m.bpm().unwrap();
        assert!(bpm8 > 100.0 && bpm8 < 125.0, "got {bpm8}");
    }

    #[test]
    fn clamps_to_supported_tempo_range() {
        let mut m = TapMeter::new();
        // Very slow (1.8 s = 33.3 BPM) clamps to 40.0.
        m.tap(0.0);
        assert_eq!(m.tap(1.8), Some(MIN_BPM));
        // Very fast (0.15 s = 400 BPM) clamps to 300.0.
        m.reset();
        m.tap(0.0);
        assert_eq!(m.tap(0.15), Some(MAX_BPM));
    }

    #[test]
    fn midi_port_clock_uses_driver_timestamps_and_switches_cleanly() {
        let mut m = TapMeter::new();
        let port = TapClock::MidiPort("MPK mini".into());
        // Even if UI wall_s has heavy frame jitter (480 ms / 520 ms), driver
        // timestamps at exact 500_000 us intervals yield exact 120.0 BPM.
        m.tap_with_clock(port.clone(), 10.000, 1.000);
        m.tap_with_clock(port.clone(), 10.500, 1.480);
        let r = m.tap_with_clock(port.clone(), 11.000, 2.020);
        assert_eq!(r.bpm, Some(120.0));

        // Switching to keyboard tap falls back to wall_s without an epoch jump.
        let kb = m.tap_with_clock(TapClock::Ui, 2.520, 2.520);
        assert!(kb.accepted);
        assert_eq!(kb.bpm, Some(120.0));
    }

    #[test]
    fn recent_samples_reflect_window_and_outliers() {
        let mut m = TapMeter::new();
        m.window = AvgWindow::Four;
        for t in [0.0, 0.5, 1.0, 2.0, 2.5] {
            m.tap(t);
        }
        let samples = m.recent_samples();
        assert_eq!(samples.len(), 4);
        assert!(!samples[0].outlier);
        assert!(!samples[1].outlier);
        assert!(samples[2].outlier);
        assert!(!samples[3].outlier);
    }
}
