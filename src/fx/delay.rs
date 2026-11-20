//! Tempo-synced delay with ping-pong and tone control.

use super::Unit::*;
use super::dsp::OnePole;
use super::{Ctx, Effect, ParamSpec, p};

pub const PARAMS: &[ParamSpec] = &[
    p("Time", 1.0, 16.0, 3.0, false, Steps),
    p("Feedback", 0.0, 95.0, 40.0, false, Pct),
    p("Ping-pong", 0.0, 100.0, 100.0, false, Pct),
    p("Tone", 500.0, 20000.0, 8000.0, true, Hz),
    p("Wet", 0.0, 100.0, 30.0, false, Pct),
    p("Dry", 0.0, 100.0, 100.0, false, Pct),
];

pub const STEREO_PARAMS: &[ParamSpec] = &[
    p("Time", 0.01, 16.0, 3.0, false, Steps),
    p("Feedback", -95.0, 95.0, 40.0, false, Pct),
    PARAMS[2],
    PARAMS[3],
    p("Wet", 0.0, 200.0, 30.0, false, Pct),
    p("Dry", 0.0, 125.0, 100.0, false, Pct),
    p("Stereo offset", -4.0, 4.0, 0.0, false, Steps),
    p("Input pan", -100.0, 100.0, 0.0, false, Pct),
    p("Separation", -100.0, 100.0, 0.0, false, Pct),
];

const MAX_DELAY_S: f32 = 4.0;

pub struct Delay {
    buf: Vec<[f32; 2]>,
    i: usize,
    tone: OnePole,
    input_delay: Vec<[f32; 2]>,
    input_at: usize,
}

impl Delay {
    pub fn new(rate: f32) -> Self {
        Self {
            buf: vec![[0.0; 2]; (MAX_DELAY_S * rate) as usize],
            i: 0,
            tone: OnePole::default(),
            input_delay: Vec::new(),
            input_at: 0,
        }
    }

    pub fn stereo(rate: f32) -> Self {
        Self {
            buf: vec![[0.0; 2]; (6.01 * rate) as usize],
            input_delay: vec![[0.0; 2]; (1.51 * rate) as usize],
            ..Self::new(rate)
        }
    }
}

impl Effect for Delay {
    fn process(&mut self, p: &[f32], buf: &mut [f32], ctx: &Ctx) {
        let rate = ctx.rate;
        let tempo = ctx.tempo;
        let Self {
            buf: line,
            i,
            tone,
            input_delay,
            input_at,
        } = self;
        let step_s = 60.0 / tempo.clamp(40.0, 300.0) / 4.0;
        let stereo = !input_delay.is_empty();
        let delay_frames = if stereo {
            (p[0] * step_s * rate).round()
        } else {
            p[0].round() * step_s * rate
        };
        let d = (delay_frames as usize).clamp(1, line.len() - 1);
        let fb = p[1] / 100.0;
        let pp = p[2] / 100.0;
        tone.set(p[3], rate);
        let (wet, dry) = (p[4] / 100.0, p[5] / 100.0);
        let (offset, pan, sep) = if stereo {
            (p[6], p[7] / 100.0, p[8] / 100.0)
        } else {
            (0.0, 0.0, 0.0)
        };
        let left_offset = ((-offset).max(0.0) * step_s * rate).round() as usize;
        let right_offset = (offset.max(0.0) * step_s * rate).round() as usize;
        let n = line.len();
        for f in buf.as_chunks_mut::<2>().0 {
            let r = line[(*i + n - d) % n];
            let mid = (r[0] + r[1]) * 0.5 * (1.0 + sep.min(0.0) * 0.5);
            let side = (r[0] - r[1]) * 0.5 * (1.0 - sep.max(0.0));
            let echo = if sep == 0.0 {
                r
            } else {
                [mid + side, mid - side]
            };
            let (dl, dr) = (tone.tick(0, echo[0]), tone.tick(1, echo[1]));
            let dry_in = [f[0], f[1]];
            let input = if stereo {
                let size = input_delay.len();
                input_delay[*input_at] = [f[0] * (1.0 - pan.max(0.0)), f[1] * (1.0 + pan.min(0.0))];
                let l = input_delay[(*input_at + size - left_offset.min(size - 1)) % size][0];
                let r = input_delay[(*input_at + size - right_offset.min(size - 1)) % size][1];
                *input_at = (*input_at + 1) % size;
                [l, r]
            } else {
                dry_in
            };
            // Ping-pong: feedback crosses channels; the input enters on the left.
            let mono = (input[0] + input[1]) * 0.5;
            let in_l = input[0] * (1.0 - pp) + mono * pp;
            let in_r = input[1] * (1.0 - pp);
            line[*i] = [
                in_l + (dl * (1.0 - pp) + dr * pp) * fb,
                in_r + (dr * (1.0 - pp) + dl * pp) * fb,
            ];
            *i = (*i + 1) % n;
            let wet_out = if stereo { echo } else { [dl, dr] };
            f[0] = dry_in[0] * dry + wet_out[0] * wet;
            f[1] = dry_in[1] * dry + wet_out[1] * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fx::{FxKind, FxState};
    const RATE: f32 = 48_000.0;

    fn run(kind: FxKind, set: impl Fn(&mut Vec<f32>), input: &[f32]) -> Vec<f32> {
        let mut p = kind.defaults();
        set(&mut p);
        let mut st = FxState::new(kind, RATE);
        let mut buf = input.to_vec();
        for chunk in buf.chunks_mut(512) {
            st.process(&p, chunk, RATE, 120.0);
        }
        buf
    }

    #[test]
    fn delay_echo_arrives_after_the_synced_time() {
        // 120 BPM: one step = 125 ms; Time 2 steps = 250 ms.
        let mut imp = vec![0.0f32; 2 * 30_000];
        imp[0] = 1.0;
        imp[1] = 1.0;
        let out = run(
            FxKind::Delay,
            |p| {
                p[0] = 2.0;
                p[2] = 0.0;
                p[3] = 20_000.0;
                p[5] = 0.0;
                p[4] = 100.0
            },
            &imp,
        );
        let at = (0.25 * RATE) as usize;
        let (peak_i, _) = out
            .chunks(2)
            .enumerate()
            .map(|(i, f)| (i, f[0].abs()))
            .fold((0, 0.0f32), |a, b| if b.1 > a.1 { b } else { a });
        assert!(
            (peak_i as i64 - at as i64).abs() < 5,
            "echo at {peak_i}, want {at}"
        );
    }
}
