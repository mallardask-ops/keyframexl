//! Tempo-synced beat buffer (FL: Gross Beat's time side): every 1/16 step
//! replays the audio of the step before it, at a rate the step chooses.
//! Play passes the input through, so an all-Play pattern is transparent.

use super::Unit::*;
use super::{Ctx, Effect, ParamSpec, p};

pub const STEPS: usize = 16;

pub const MODES: [&str; 6] = ["Play", "Half", "Double", "Reverse", "Stutter", "Gate"];
pub const HINTS: [&str; 6] = [
    "Play through",
    "Half speed",
    "Double speed",
    "Play the last step backwards",
    "Repeat a slice of the last step",
    "Silence",
];

pub const PARAMS: &[ParamSpec] = &[
    p("Loop", 1.0, STEPS as f32, STEPS as f32, false, Steps),
    p("Smooth", 1.0, 100.0, 6.0, false, Ms),
    p("Slice", 5.0, 100.0, 25.0, false, Pct),
    p("Step 1", 0.0, 5.0, 0.0, false, Plain),
    p("Step 2", 0.0, 5.0, 0.0, false, Plain),
    p("Step 3", 0.0, 5.0, 0.0, false, Plain),
    p("Step 4", 0.0, 5.0, 0.0, false, Plain),
    p("Step 5", 0.0, 5.0, 0.0, false, Plain),
    p("Step 6", 0.0, 5.0, 0.0, false, Plain),
    p("Step 7", 0.0, 5.0, 0.0, false, Plain),
    p("Step 8", 0.0, 5.0, 0.0, false, Plain),
    p("Step 9", 0.0, 5.0, 0.0, false, Plain),
    p("Step 10", 0.0, 5.0, 0.0, false, Plain),
    p("Step 11", 0.0, 5.0, 0.0, false, Plain),
    p("Step 12", 0.0, 5.0, 0.0, false, Plain),
    p("Step 13", 0.0, 5.0, 0.0, false, Plain),
    p("Step 14", 0.0, 5.0, 0.0, false, Plain),
    p("Step 15", 0.0, 5.0, 0.0, false, Plain),
    p("Step 16", 0.0, 5.0, 0.0, false, Plain),
];

const MAX_S: f32 = 4.0;

pub struct Time {
    buf: Vec<[f32; 2]>,
    w: usize,
    started: bool,
    step: usize,
    mode: usize,
    prev: usize,
    next: usize,
    /// Samples left in this step, and its full length.
    left: usize,
    len: usize,
    /// Read position in the ring, fractional for rate changes.
    r: f64,
    /// Write position when the step began: the step replays the audio before it.
    anchor: usize,
    slice: usize,
    fade: usize,
    fade_in: bool,
    fade_out: bool,
    last: [f32; 2],
}

impl Time {
    pub fn new(rate: f32) -> Self {
        Self {
            buf: vec![[0.0; 2]; (MAX_S * rate.max(1000.0)) as usize],
            w: 0,
            started: false,
            step: 0,
            mode: 0,
            prev: 0,
            next: 0,
            left: 0,
            len: 1,
            r: 0.0,
            anchor: 0,
            slice: 1,
            fade: 1,
            fade_in: false,
            fade_out: false,
            last: [0.0; 2],
        }
    }

    fn begin(&mut self, p: &[f32], step_len: usize, fade: usize, slice: usize) {
        let steps = (p[0].round() as usize).clamp(1, STEPS);
        self.prev = if self.started { self.mode } else { 0 };
        self.step = if self.started {
            (self.step + 1) % steps
        } else {
            0
        };
        self.started = true;
        let mode = |i: usize| (p[3 + i.min(STEPS - 1)].round() as usize).min(MODES.len() - 1);
        self.mode = mode(self.step);
        self.next = mode((self.step + 1) % steps);
        self.anchor = self.w;
        self.len = step_len;
        self.left = step_len;
        self.slice = slice.min(step_len);
        self.fade = fade;
        // A run of Play steps stays transparent; anything else may jump, so
        // its edges get a short ramp.
        self.fade_in = self.mode != 0 || self.prev != 0;
        self.fade_out = self.mode != 0 || self.next != 0;
        self.r = match self.mode {
            1 | 2 => self.anchor as f64 - step_len as f64,
            _ => self.anchor as f64 - 1.0,
        };
    }

    fn at(&self, r: f64) -> [f32; 2] {
        let n = self.buf.len();
        let x = r.rem_euclid(n as f64);
        let i = (x.floor() as usize) % n;
        let j = (i + 1) % n;
        let t = (x - x.floor()) as f32;
        [
            self.buf[i][0] + (self.buf[j][0] - self.buf[i][0]) * t,
            self.buf[i][1] + (self.buf[j][1] - self.buf[i][1]) * t,
        ]
    }
}

impl Effect for Time {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate.max(1000.0);
        let step_len = ((60.0 / ctx.tempo.clamp(40.0, 300.0) / 4.0) * rate)
            .round()
            .max(2.0) as usize;
        let fade = (((p[1] / 1000.0) * rate).round() as usize).clamp(1, (step_len / 2).max(1));
        let slice = (((p[2] / 100.0) * step_len as f32).round() as usize).clamp(1, step_len);
        let n = self.buf.len();
        for f in buf.chunks_exact_mut(2) {
            if self.left == 0 {
                self.begin(p, step_len, fade, slice);
            }
            // Write first: a step reads the audio right before its own start.
            self.buf[self.w] = [f[0], f[1]];
            let src = match self.mode {
                0 => [f[0], f[1]],
                1 | 2 => {
                    let s = self.at(self.r);
                    self.r += if self.mode == 1 { 0.5 } else { 2.0 };
                    if self.mode == 2 && self.r >= self.anchor as f64 {
                        self.r -= self.len as f64;
                    }
                    s
                }
                3 => {
                    let s = self.at(self.r);
                    self.r -= 1.0;
                    if self.r < self.anchor as f64 - self.len as f64 {
                        self.r += self.len as f64;
                    }
                    s
                }
                4 => {
                    let age = self.len - self.left;
                    let i = (self.anchor + n - self.slice + (age % self.slice)) % n;
                    self.buf[i]
                }
                _ => [0.0; 2],
            };
            let age = self.len - self.left;
            let g_in = if self.fade_in {
                (age as f32 / self.fade as f32).min(1.0)
            } else {
                1.0
            };
            let g_out = if self.fade_out {
                (self.left as f32 / self.fade as f32).min(1.0)
            } else {
                1.0
            };
            let g = g_in.min(g_out);
            self.last[0] += (src[0] - self.last[0]) * g;
            self.last[1] += (src[1] - self.last[1]) * g;
            f[0] = self.last[0];
            f[1] = self.last[1];
            self.left -= 1;
            self.w = (self.w + 1) % n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RATE: f32 = 48_000.0;

    fn run(set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = super::super::FxKind::Time.defaults();
        set(&mut p);
        let mut st = super::super::FxState::new(super::super::FxKind::Time, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    fn ramp(n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = (i % 1000) as f32 / 1000.0 - 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn all_play_is_transparent() {
        let input = ramp(4800);
        let out = run(|_| {}, &input);
        assert_eq!(out, input);
    }

    #[test]
    fn gate_silences_only_its_step() {
        let input = ramp(4 * 6000); // four 1/16 steps at 120 BPM
        let out = run(|p| p[3] = 5.0, &input); // step 1 gated
        let step = 6000usize;
        assert!(out[..step * 2].iter().all(|v| *v == 0.0));
        assert!(out[step * 2..step * 4].iter().any(|v| *v != 0.0));
        // The first step is silent; the next step plays through after its
        // short edge fade (the gate -> play jump is ramped).
        let fade = 400; // 6 ms at 48 kHz is 288 frames
        assert_eq!(
            &out[(step + fade) * 2..(2 * step) * 2],
            &input[(step + fade) * 2..(2 * step) * 2]
        );
    }

    #[test]
    fn stutter_repeats_a_slice() {
        let input = ramp(4 * 6000);
        let out = run(
            |p| {
                p[1] = 1.0; // 1 ms edge fades
                p[2] = 50.0; // half a step
                p[4] = 4.0; // step 2 stutters
            },
            &input,
        );
        let step = 6000usize;
        let slice = step / 2;
        let s = &out[step * 2..step * 4];
        // Skip the step's fade-in, where the repeat is blended from the old
        // stream; the rest of the slice repeats exactly.
        // Edge fades sit on both ends of the step.
        for f in 100..slice - 100 {
            assert!(
                (s[2 * f] - s[2 * (f + slice)]).abs() < 1e-5,
                "frame {f} does not repeat"
            );
        }
    }

    #[test]
    fn every_step_has_a_valid_mode() {
        assert_eq!(PARAMS.len(), 3 + STEPS);
        for s in &PARAMS[3..] {
            assert_eq!((s.min, s.max), (0.0, 5.0));
        }
    }
}
