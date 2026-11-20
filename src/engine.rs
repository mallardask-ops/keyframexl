//! Voice engine: block rendering with stateful voices. The audio callback and
//! the offline export run this same code, so what you hear is what you export.
//!
//! Each block: collect the hits (drum steps, note starts) that fall inside it,
//! render the running voices up to each hit, start the hit's voices, continue.
//! Voices own their time, sample position, filter state and envelopes, so a
//! release rings across the loop seam without any special case.

use crate::sampler::{
    FilterKind, FilterRouting, GlideMode, LoopMode, ModSource, ModTarget, PlayMode, Program,
    ProgramFx, SamplePreset, TriggerMode, Zone,
};
use crate::sequence::{
    BAR, InstrumentKind, Note, Pattern, Song, Tick, drum, loop_bounds, loop_frames,
    seconds_per_step, steps, swing_offset, wrap_frame,
};

const MAX_VOICES: usize = 128;
/// Fade used when a voice is stolen or cut by its group.
const KILL_SECONDS: f64 = 0.005;
/// Time constant of the parameter smoothers (faders, slot mixes, channel
/// gains, effect parameters): ~5 ms, independent of rate and block size.
const SMOOTH_TAU_SEC: f64 = 0.005;
/// Offline renders stop this long after the last voice at the latest.
const MAX_TAIL_SECONDS: f64 = 30.0;

/// Per-sample one-pole coefficient toward a new parameter target.
fn smooth_alpha(rate: f64) -> f64 {
    1.0 - (-1.0 / (SMOOTH_TAU_SEC * rate)).exp()
}

/// Topology-preserving state-variable filter (Simper/Cytomic). Stable under
/// fast cutoff modulation, which a sampler's envelopes and LFOs need.
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f64,
    ic2: f64,
}
impl Svf {
    /// Returns (low, band, high).
    fn tick(&mut self, x: f64, g: f64, k: f64) -> (f64, f64, f64) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Drum {
        row: usize,
    },
    Synth,
    /// A sampler channel without a preset plays a sine so notes stay audible.
    Tone,
    Zone {
        zone: usize,
    },
    /// A Forge channel's rendered sound, played as a one-shot.
    Forge,
}

#[derive(Clone, Copy)]
struct Voice {
    inst: u64,
    note: u64,
    source: Source,
    key: u8,
    vel: u8,
    group: u8,
    cut_group: u8,
    /// Seconds since the voice started.
    t: f64,
    /// Seconds the key is held; infinite for one-shots.
    gate: f64,
    /// Seconds the note itself lasts (a one-shot sample rings past it): what
    /// the channel rack lights as "playing"; after it, the tail.
    key_until: f64,
    /// Sample position (zones) or oscillator phase in cycles (tone).
    pos: f64,
    phases: [f64; 7],
    phases_b: [f64; 7],
    /// Spectral oscillator: phase (cycles), partial amplitudes, how many,
    /// and frames until they are worked out again.
    phase_c: f64,
    spec: [f64; crate::synth::MAX_PARTIALS],
    spec_n: usize,
    spec_wait: u32,
    /// MIDI channel 0..15 of a live note (MPE: its own controllers).
    ch: u8,
    /// Synth caches, refilled only when their inputs change (most frames
    /// they do not): unison pitch ratios of oscillators A and B, the note's
    /// frequency, the warp exponents, and the filter coefficient.
    tide: TideCache,
    /// Live pitch bend in semitones: where it heads, and where it is now
    /// (smoothed per frame so the wheel never steps).
    bend_to: f64,
    bend_now: f64,
    /// Live mod wheel, pressure and slide (CC74), 0..1.
    wheel: f64,
    pressure: f64,
    cc74: f64,
    noise_state: u32,
    dir: f64,
    /// Note slide: (start s, duration s, semitones).
    slide: Option<(f64, f64, f64)>,
    /// Portamento: semitones still to travel and over how long, starting at `glide_t0`.
    glide_from: f64,
    glide_time: f64,
    glide_t0: f64,
    /// Per filter, per channel.
    filter: [[Svf; 2]; 2],
    /// Song time (s) when the voice started, for free-running LFOs.
    clock: f64,
    /// Per-note random value in -1..1 (matrix source "Random").
    rnd: f64,
    /// Note pan -1..1 and fine pitch in semitones.
    pan: f64,
    fine: f64,
    /// Fade-out start time when stolen or cut.
    kill: Option<f64>,
    done: bool,
    /// Last pitch (semitones) and its playback ratio: `powf` only when the
    /// pitch moves (slides, glides, LFO). Same input, same result: renders
    /// stay bit-identical.
    ratio_cache: (f64, f64),
    /// Per filter: last (cutoff mod, emphasis mod) and its (g, k).
    coeff_cache: [(f64, f64, (f64, f64)); 2],
    /// Channel volume/pan gains reached at the last rendered frame
    /// (smoothing); `None` starts at the next block's target.
    gain_state: Option<[f64; 2]>,
    /// Per-voice effects (`VoiceFx`): ring-mod carrier phase, decimator
    /// countdown and held frame, phaser LFO phase and its 4 allpass states
    /// per channel. Untouched while the effects are off.
    rm_phase: f64,
    dec_acc: f64,
    dec_hold: [f64; 2],
    ph_phase: f64,
    ph_ap: [[f64; 4]; 2],
}

impl Voice {
    fn new(inst: u64, note: u64, source: Source, key: u8, vel: u8, gate: f64) -> Self {
        Self {
            inst,
            note,
            source,
            key,
            vel,
            group: 0,
            cut_group: 0,
            t: 0.0,
            gate,
            key_until: gate,
            pos: 0.0,
            phases: [0.0; 7],
            phases_b: [0.0; 7],
            phase_c: 0.0,
            spec: [0.0; crate::synth::MAX_PARTIALS],
            spec_n: 0,
            spec_wait: 0,
            ch: 0,
            tide: TideCache::default(),
            bend_to: 0.0,
            bend_now: 0.0,
            wheel: 0.0,
            pressure: 0.0,
            cc74: 0.0,
            noise_state: (note as u32)
                .wrapping_mul(747796405)
                .wrapping_add(2891336453),
            dir: 1.0,
            slide: None,
            glide_from: 0.0,
            glide_time: 0.0,
            glide_t0: 0.0,
            filter: [[Svf::default(); 2]; 2],
            clock: 0.0,
            rnd: 0.0,
            pan: 0.0,
            fine: 0.0,
            kill: None,
            done: false,
            ratio_cache: (f64::NAN, 0.0),
            coeff_cache: [(f64::NAN, f64::NAN, (0.0, 0.0)); 2],
            gain_state: None,
            rm_phase: 0.0,
            dec_acc: 0.0,
            dec_hold: [0.0; 2],
            ph_phase: 0.0,
            ph_ap: [[0.0; 4]; 2],
        }
    }

    fn held(&self) -> bool {
        self.kill.is_none() && self.t < self.gate
    }

    fn kill_now(&mut self) {
        if self.kill.is_none() {
            self.kill = Some(self.t);
        }
    }

    fn kill_gain(&mut self) -> f64 {
        match self.kill {
            Some(k) => {
                let g = 1.0 - (self.t - k) / KILL_SECONDS;
                if g <= 0.0 {
                    self.done = true;
                }
                g.max(0.0)
            }
            None => 1.0,
        }
    }

    /// Semitone offset from slides and portamento at the current time.
    fn bend(&self) -> f64 {
        let mut s = 0.0;
        if let Some((start, dur, delta)) = self.slide {
            let x = ((self.t - start) / dur.max(1e-9)).clamp(0.0, 1.0);
            s += delta * x;
        }
        if self.glide_from != 0.0 {
            let x = if self.glide_time > 0.0 {
                ((self.t - self.glide_t0) / self.glide_time).clamp(0.0, 1.0)
            } else {
                1.0
            };
            s += self.glide_from * (1.0 - x);
        }
        s
    }

    fn render_drum(&mut self, row: usize, dt: f64) -> [f64; 2] {
        // Pitched like a sample (FL): C5 (60) plays as recorded, an octave
        // up runs twice as fast. Exactly 1.0 at 60 keeps old renders identical.
        let ratio = if self.key == 60 {
            1.0
        } else {
            2f64.powf((self.key as f64 - 60.0) / 12.0)
        };
        let ts = self.t * ratio;
        if ts >= 0.25 {
            self.done = true;
            return [0.0; 2];
        }
        let v = drum(row, ts) * self.vel as f64 / 100.0 * self.kill_gain();
        self.t += dt;
        if self.pan != 0.0 {
            [v * (1.0 - self.pan.max(0.0)), v * (1.0 + self.pan.min(0.0))]
        } else {
            [v, v]
        }
    }

    /// Matrix source values for this voice, in `SynthPatch::source_index` order.
    fn mod_sources(
        &self,
        p: &crate::synth::SynthPatch,
        tempo: f64,
    ) -> [f64; crate::synth::MOD_SOURCES] {
        let seed = self.rnd.to_bits() ^ self.note.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let m = |i: usize| p.macros[i].value;
        [
            p.lfos[0].at(self.t, self.clock, tempo, seed),
            p.lfos[1].at(self.t, self.clock, tempo, seed ^ 0xA5A5),
            p.filter_env.value(self.t, self.gate),
            self.vel as f64 / 127.0,
            m(0),
            m(1),
            m(2),
            m(3),
            self.wheel,
            self.pressure,
            self.cc74,
        ]
    }

    fn render_synth(
        &mut self,
        p: &crate::synth::SynthPatch,
        bank: &crate::synth::TableBank,
        rate: f64,
    ) -> [f64; 2] {
        use crate::synth::{Filter, Warp};
        if self.t >= self.gate + p.amp_env.release {
            self.done = true;
            return [0.0; 2];
        }
        if self.bend_now != self.bend_to {
            // About 3 ms to follow the wheel.
            let k = (-1.0 / (0.003 * rate)).exp();
            self.bend_now = self.bend_to + (self.bend_now - self.bend_to) * k;
            if (self.bend_now - self.bend_to).abs() < 1e-6 {
                self.bend_now = self.bend_to;
            }
        }
        let semis = self.key as f64 + self.bend() + self.bend_now + self.fine - 69.0;
        if semis.to_bits() != self.tide.hz_key.to_bits() {
            self.tide.hz_key = semis;
            self.tide.hz = 440.0 * 2f64.powf(semis / 12.0);
        }
        let hz = self.tide.hz;
        let sub = (self.pos * std::f64::consts::TAU).sin();
        let mut out = [0.0; 2];
        let count = p.unison as usize;
        // Unison pitch ratios (A and B): only when detune or tuning moves.
        let key = [
            p.detune,
            p.coarse,
            p.fine,
            p.osc_b.coarse,
            p.osc_b.fine,
            count as f64,
        ];
        if key
            .iter()
            .zip(&self.tide.uni_key)
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            self.tide.uni_key = key;
            self.tide.levels_hz = f64::NAN;
            for i in 0..count {
                let side = if count == 1 {
                    0.0
                } else {
                    i as f64 * 2.0 / (count - 1) as f64 - 1.0
                };
                self.tide.uni_a[i] =
                    2f64.powf((side * p.detune + p.coarse + p.fine / 100.0) / 12.0);
                self.tide.uni_b[i] =
                    2f64.powf((side * p.detune + p.osc_b.coarse + p.osc_b.fine / 100.0) / 12.0);
            }
        }
        // Bend warp exponents, per oscillator.
        for (k, w) in [p.warp_amount, p.osc_b.warp_amount].into_iter().enumerate() {
            if w.to_bits() != self.tide.warp_key[k].to_bits() {
                self.tide.warp_key[k] = w;
                self.tide.warp_exp[k] = 2f64.powf(w * 2.0);
            }
        }
        let [exp_a, exp_b] = self.tide.warp_exp;
        // Mip levels follow the pitch: refreshed when it or the ratios move.
        if hz.to_bits() != self.tide.levels_hz.to_bits() {
            self.tide.levels_hz = hz;
            for i in 0..count {
                self.tide.lvl_a[i] = crate::synth::TableBank::level(hz * self.tide.uni_a[i], rate);
                self.tide.lvl_b[i] = crate::synth::TableBank::level(hz * self.tide.uni_b[i], rate);
            }
        }
        for i in 0..count {
            let side = if count == 1 {
                0.0
            } else {
                i as f64 * 2.0 / (count - 1) as f64 - 1.0
            };
            let freq = hz * self.tide.uni_a[i];
            let phase = self.phases[i];
            let warped = match p.warp {
                Warp::Bend => {
                    if exp_a == 1.0 {
                        phase
                    } else {
                        phase.powf(exp_a)
                    }
                }
                Warp::Sync => (phase * (1.0 + p.warp_amount.abs() * 3.0)).fract(),
                Warp::PwmMirror => {
                    let split = (0.5 + p.warp_amount * 0.45).clamp(0.05, 0.95);
                    let mirrored = if phase < split {
                        phase / split
                    } else {
                        (1.0 - phase) / (1.0 - split)
                    };
                    phase * (1.0 - p.warp_amount.abs()) + mirrored * p.warp_amount.abs()
                }
                Warp::FmSub => phase + sub * p.warp_amount * 0.25,
            };
            let mut s =
                bank.sample_at(p.table, p.position, warped, self.tide.lvl_a[i]) / count as f64;
            if p.level != 1.0 {
                s *= p.level;
            }
            if p.osc_b.level != 0.0 {
                let b = &p.osc_b;
                let freq_b = hz * self.tide.uni_b[i];
                let phase_b = self.phases_b[i];
                let warped_b = match b.warp {
                    Warp::Bend => {
                        if exp_b == 1.0 {
                            phase_b
                        } else {
                            phase_b.powf(exp_b)
                        }
                    }
                    Warp::Sync => (phase_b * (1.0 + b.warp_amount.abs() * 3.0)).fract(),
                    Warp::PwmMirror => {
                        let split = (0.5 + b.warp_amount * 0.45).clamp(0.05, 0.95);
                        let mirrored = if phase_b < split {
                            phase_b / split
                        } else {
                            (1.0 - phase_b) / (1.0 - split)
                        };
                        phase_b * (1.0 - b.warp_amount.abs()) + mirrored * b.warp_amount.abs()
                    }
                    Warp::FmSub => phase_b + sub * b.warp_amount * 0.25,
                };
                s += bank.sample_at(b.table, b.position, warped_b, self.tide.lvl_b[i]) * b.level
                    / count as f64;
                self.phases_b[i] = (phase_b + freq_b / rate).fract();
            }
            let pan = side * p.spread;
            out[0] += s * (1.0 - pan.max(0.0));
            out[1] += s * (1.0 + pan.min(0.0));
            self.phases[i] = (phase + freq / rate).fract();
        }
        if p.spectral.level > 0.0 {
            let c = &p.spectral;
            let freq_c = hz * 2f64.powf(c.coarse / 12.0);
            // Amplitudes every 32 frames (like the modulation): shimmer and
            // routes move slowly, and the sum below stays cheap.
            if self.spec_wait == 0 {
                self.spec_n = c.amplitudes(freq_c, rate, self.t, &mut self.spec);
                self.spec_wait = 32;
            }
            self.spec_wait -= 1;
            // sin(kφ) by the Chebyshev recurrence: one sin and cos per frame.
            let ph = self.phase_c * std::f64::consts::TAU;
            let (s1, c1) = ph.sin_cos();
            let two_cos = 2.0 * c1;
            let (mut prev, mut cur) = (0.0, s1);
            let mut sum = self.spec[0] * s1;
            for a in &self.spec[1..self.spec_n] {
                let next = two_cos * cur - prev;
                prev = cur;
                cur = next;
                sum += a * cur;
            }
            let v = sum * c.level;
            out[0] += v;
            out[1] += v;
            self.phase_c = (self.phase_c + freq_c / rate).fract();
        }
        self.pos = (self.pos + hz * 0.5 / rate).fract();
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        let noise = self.noise_state as f64 / u32::MAX as f64 * 2.0 - 1.0;
        let cutoff = (p.cutoff
            * 2f64.powf(
                p.keytrack * (self.key as f64 - 60.0) / 12.0
                    + p.env_amount * p.filter_env.value(self.t, self.gate) * 6.0,
            ))
        .clamp(20.0, rate * 0.45);
        if cutoff.to_bits() != self.tide.cut_key.to_bits() {
            self.tide.cut_key = cutoff;
            self.tide.g = (std::f64::consts::PI * cutoff / rate).tan();
        }
        let g = self.tide.g;
        let k = 2.0 - p.resonance * 1.95;
        let amp =
            p.amp_env.value(self.t, self.gate) * self.vel as f64 / 100.0 * 0.3 * self.kill_gain();
        for (ch, sample) in out.iter_mut().enumerate() {
            let (low, band, high) =
                self.filter[0][ch].tick(*sample + sub * p.sub + noise * p.noise, g, k);
            *sample = match p.filter {
                Filter::LowPass => low,
                Filter::HighPass => high,
                Filter::BandPass => band,
            } * amp;
        }
        out[0] *= 1.0 - self.pan.max(0.0);
        out[1] *= 1.0 + self.pan.min(0.0);
        self.t += 1.0 / rate;
        out
    }

    fn render_tone(&mut self, dt: f64) -> [f64; 2] {
        let env = crate::sampler::Adsr::default();
        if self.t >= self.gate + env.release {
            self.done = true;
            return [0.0; 2];
        }
        let hz = 440.0 * 2f64.powf((self.key as f64 + self.bend() + self.fine - 69.0) / 12.0);
        let v = (self.pos * std::f64::consts::TAU).sin()
            * 0.16
            * env.value(self.t, self.gate)
            * self.vel as f64
            / 100.0
            * self.kill_gain();
        self.pos += hz * dt;
        self.t += dt;
        if self.pan != 0.0 {
            [v * (1.0 - self.pan.max(0.0)), v * (1.0 + self.pan.min(0.0))]
        } else {
            [v, v]
        }
    }

    /// A Forge sound: its frames at a speed set by the key (C5 = as rendered),
    /// note fine pitch and slides; velocity sets the level.
    fn render_forge(&mut self, r: &crate::forge::Rendered, rate: f64) -> [f64; 2] {
        let n = r.frames.len();
        if self.pos < 0.0 || self.pos + 1.0 >= n as f64 {
            self.done = true;
            return [0.0; 2];
        }
        let semis = self.key as f64 + self.bend() + self.fine - crate::forge::ROOT_KEY as f64;
        if self.ratio_cache.0 != semis {
            self.ratio_cache = (semis, 2f64.powf(semis / 12.0));
        }
        let i = self.pos as usize;
        let f = self.pos - i as f64;
        let (a, b) = (r.frames[i], r.frames[i + 1]);
        let g = self.vel as f64 / 100.0 * self.kill_gain();
        let l = (a[0] as f64 + (b[0] - a[0]) as f64 * f) * g;
        let rr = (a[1] as f64 + (b[1] - a[1]) as f64 * f) * g;
        self.pos += self.ratio_cache.1 * r.rate as f64 / rate;
        self.t += 1.0 / rate;
        if self.pan != 0.0 {
            [
                l * (1.0 - self.pan.max(0.0)),
                rr * (1.0 + self.pan.min(0.0)),
            ]
        } else {
            [l, rr]
        }
    }

    fn render_zone(&mut self, z: &Zone, program: &Program, rate: f64) -> [f64; 2] {
        let p = &z.params;
        let gate = self.gate;
        if self.t >= gate + z.adsr.release {
            self.done = true;
            return [0.0; 2];
        }
        // Modulation matrix: sources, then per-target sums (DirectWave Modulation Matrix).
        let mut m = [0.0f64; 9];
        let active = p.matrix.iter().any(|s| s.source != ModSource::None);
        let target = |t: ModTarget| t as usize;
        let mut lfo = [0.0; 2];
        if active {
            // Program LFOs: only worked out when a slot reads one.
            let mut plfo = [0.0; 2];
            for s in p.matrix.iter() {
                match s.source {
                    ModSource::ProgLfo1 => plfo[0] = program.lfo[0].at(self.t, self.clock, 1.0),
                    ModSource::ProgLfo2 => plfo[1] = program.lfo[1].at(self.t, self.clock, 1.0),
                    _ => {}
                }
            }
            // LFO rate is itself a target, so resolve it from non-LFO sources first.
            let mut rate_oct = [0.0; 2];
            for s in p
                .matrix
                .iter()
                .filter(|s| matches!(s.target, ModTarget::Lfo1Rate | ModTarget::Lfo2Rate))
            {
                let v = self.mod_source(s.source, p, gate, [0.0; 2], plfo);
                rate_oct[(s.target == ModTarget::Lfo2Rate) as usize] +=
                    v * s.amount * s.target.full_scale();
            }
            for (i, l) in p.lfo.iter().enumerate() {
                lfo[i] = l.at(self.t, self.clock, 2f64.powf(rate_oct[i]));
            }
            for s in p.matrix.iter().filter(|s| s.source != ModSource::None) {
                m[target(s.target)] += self.mod_source(s.source, p, gate, lfo, plfo)
                    * s.amount
                    * s.target.full_scale();
            }
        }

        // Pitch.
        let semis = (self.key as f64 - z.root as f64) * p.key_track
            + z.tune_cents / 100.0
            + program.transpose as f64
            + self.bend()
            + m[target(ModTarget::Pitch)];
        let semis = if self.fine != 0.0 {
            semis + self.fine
        } else {
            semis
        };
        let ratio = if self.ratio_cache.0 == semis {
            self.ratio_cache.1
        } else {
            let r = 2f64.powf(semis / 12.0) * z.sample_rate as f64 / rate;
            self.ratio_cache = (semis, r);
            r
        };

        // Read the sample (linear interpolation, loop-aware) and advance.
        let looping = !p.reverse
            && !p.one_shot
            && z.loop_mode != LoopMode::None
            && z.loop_end > z.loop_start + 1
            && (z.loop_mode != LoopMode::Sustain || self.t < gate);
        if self.pos < z.offset as f64 || self.pos >= z.end as f64 {
            self.done = true;
            return [0.0; 2];
        }
        let i = self.pos.floor() as usize;
        let frac = (self.pos - i as f64) as f32;
        let next = if looping && self.dir > 0.0 && i + 1 >= z.loop_end {
            if z.loop_mode == LoopMode::PingPong {
                z.loop_end - 1
            } else {
                z.loop_start
            }
        } else if self.dir < 0.0 {
            i.saturating_sub(1).max(z.offset)
        } else {
            (i + 1).min(z.end - 1)
        };
        let (a, b) = (z.frames[i], z.frames[next]);
        let s = [
            (a[0] + (b[0] - a[0]) * frac) as f64,
            (a[1] + (b[1] - a[1]) * frac) as f64,
        ];
        self.pos += ratio * self.dir;
        if looping {
            let (ls, le) = (z.loop_start as f64, z.loop_end as f64);
            if z.loop_mode == LoopMode::PingPong {
                if self.dir > 0.0 && self.pos >= le - 1.0 {
                    self.pos = 2.0 * (le - 1.0) - self.pos;
                    self.dir = -1.0;
                } else if self.dir < 0.0 && self.pos <= ls {
                    self.pos = 2.0 * ls - self.pos;
                    self.dir = 1.0;
                }
            } else if self.pos >= le {
                self.pos = ls + (self.pos - ls) % (le - ls);
            }
        }

        // Two filters, serial or parallel (DirectWave Filter 1 & 2).
        let mut out = s;
        let on = [
            p.filters[0].kind != FilterKind::Off,
            p.filters[1].kind != FilterKind::Off,
        ];
        if on[0] || on[1] {
            let mut coeff = |fi: usize, cut: f64, emph: f64| {
                let cache = &mut self.coeff_cache[fi];
                if !on[fi] {
                    return (0.0, 0.0);
                }
                if cache.0 == cut && cache.1 == emph {
                    return cache.2;
                }
                let f = &p.filters[fi];
                let fc = (f.cutoff * 2f64.powf(cut)).clamp(20.0, rate * 0.45);
                let g = (std::f64::consts::PI * fc / rate).tan();
                let k = 2.0 - 1.96 * (f.emphasis + emph).clamp(0.0, 1.0);
                *cache = (cut, emph, (g, k));
                (g, k)
            };
            let c = [
                coeff(
                    0,
                    m[target(ModTarget::Cutoff1)],
                    m[target(ModTarget::Emphasis1)],
                ),
                coeff(
                    1,
                    m[target(ModTarget::Cutoff2)],
                    m[target(ModTarget::Emphasis2)],
                ),
            ];
            for (ch, x) in out.iter_mut().enumerate() {
                let mut run = |fi: usize, input: f64| -> f64 {
                    let (g, k) = c[fi];
                    let (lp, bp, hp) = self.filter[fi][ch].tick(input, g, k);
                    match p.filters[fi].kind {
                        FilterKind::Lowpass => lp,
                        FilterKind::Highpass => hp,
                        FilterKind::Bandpass => bp,
                        FilterKind::Notch => lp + hp,
                        FilterKind::Allpass => input - 2.0 * k * bp,
                        FilterKind::Off => input,
                    }
                };
                *x = match p.routing {
                    FilterRouting::Serial => {
                        let y = if on[0] { run(0, *x) } else { *x };
                        if on[1] { run(1, y) } else { y }
                    }
                    FilterRouting::Parallel => match on {
                        [true, true] => run(0, *x) + run(1, *x),
                        [true, false] => run(0, *x),
                        _ => run(1, *x),
                    },
                };
            }
        }

        if p.voice_fx.active() {
            out = self.voice_fx(&p.voice_fx, out, rate);
        }

        // Amplitude and pan.
        let vel_gain =
            (1.0 - z.velocity_tracking) + z.velocity_tracking * (self.vel as f64 / 127.0);
        let mut amp = z.adsr.value(self.t, gate) * z.gain * vel_gain * program.gain();
        amp *= (1.0 + m[target(ModTarget::Gain)]).clamp(0.0, 2.0);
        let pan = z.pan + program.pan + m[target(ModTarget::Pan)];
        let pan = if self.pan != 0.0 { pan + self.pan } else { pan }.clamp(-1.0, 1.0);
        amp *= self.kill_gain();
        self.t += 1.0 / rate;
        [
            out[0] * amp * (1.0 - pan.max(0.0)),
            out[1] * amp * (1.0 + pan.min(0.0)),
        ]
    }

    fn mod_source(
        &self,
        source: ModSource,
        p: &crate::sampler::ZoneParams,
        gate: f64,
        lfo: [f64; 2],
        plfo: [f64; 2],
    ) -> f64 {
        match source {
            ModSource::None => 0.0,
            ModSource::Lfo1 => lfo[0],
            ModSource::Lfo2 => lfo[1],
            ModSource::Env1 => p.env[0].value(self.t, gate),
            ModSource::Env2 => p.env[1].value(self.t, gate),
            ModSource::Velocity => self.vel as f64 / 127.0,
            ModSource::Key => (self.key as f64 - 60.0) / 60.0,
            ModSource::Random => self.rnd,
            ModSource::ProgLfo1 => plfo[0],
            ModSource::ProgLfo2 => plfo[1],
        }
    }

    /// Per-voice effects on one stereo frame (`VoiceFx`): ring modulator,
    /// decimator, quantizer, phaser, each crossfaded by its own mix.
    fn voice_fx(&mut self, fx: &crate::sampler::VoiceFx, mut out: [f64; 2], rate: f64) -> [f64; 2] {
        if fx.ringmod_mix > 0.0 {
            let c = (std::f64::consts::TAU * self.rm_phase).sin();
            self.rm_phase = (self.rm_phase + fx.ringmod_rate / rate).fract();
            let m = fx.ringmod_mix;
            for x in out.iter_mut() {
                *x = *x * (1.0 - m) + *x * c * m;
            }
        }
        if fx.decimator_mix > 0.0 {
            let step = 1.0 + 63.0 * fx.decimator_rate * fx.decimator_rate;
            // Counts down to the next captured frame; the first frame is captured at once.
            if self.dec_acc <= 0.0 {
                self.dec_hold = out;
                self.dec_acc += step;
            }
            self.dec_acc -= 1.0;
            let m = fx.decimator_mix;
            for (x, h) in out.iter_mut().zip(self.dec_hold) {
                *x = *x * (1.0 - m) + h * m;
            }
        }
        if fx.quantizer_mix > 0.0 {
            let levels = 2.0_f64.powf(fx.quantizer_bits.clamp(1.0, 16.0) - 1.0);
            let m = fx.quantizer_mix;
            for x in out.iter_mut() {
                *x = *x * (1.0 - m) + (*x * levels).round() / levels * m;
            }
        }
        if fx.phaser_mix > 0.0 {
            // The LFO sweeps the allpass corner over ~200..3000 Hz (exponentially).
            let sweep = 0.5 + 0.5 * (std::f64::consts::TAU * self.ph_phase).sin();
            self.ph_phase = (self.ph_phase + fx.phaser_freq / rate).fract();
            let hz = 200.0 * 15.0_f64.powf(sweep);
            let t = (std::f64::consts::PI * hz.min(rate * 0.45) / rate).tan();
            let a = (t - 1.0) / (t + 1.0);
            let m = fx.phaser_mix;
            for (ch, x) in out.iter_mut().enumerate() {
                let mut y = *x;
                for z in self.ph_ap[ch].iter_mut() {
                    let o = a * y + *z;
                    *z = y - a * o;
                    y = o;
                }
                *x = *x * (1.0 - m) + 0.5 * (*x + y) * m;
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct Hit {
    at: u64,
    tick: Tick,
    pattern: usize,
    note: usize,
    /// Frames of a vocal phrase that fall before the song's start.
    skip: usize,
}

/// A rendered vocal phrase playing: a mono buffer read from `pos`.
struct PhraseVoice {
    inst: u64,
    phrase: std::sync::Arc<crate::vocal::Phrase>,
    /// The live note that holds it (keyboard), else 0.
    live: u64,
    pos: usize,
    /// Frames of fade-out left once cut (transport change), else `None`.
    fade: Option<(usize, usize)>,
    /// Channel volume/pan gains reached at the last rendered frame
    /// (smoothing); `None` starts at the next block's target.
    gain_state: Option<[f64; 2]>,
}

/// `KFXL_VOCAL_TRACE=1`: log vocal triggers (debugging only; prints on the audio thread).
fn trace_vocal() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("KFXL_VOCAL_TRACE").is_some())
}

/// Vocal phrases sounding at once (reserved up front).
const MAX_PHRASES: usize = 16;

/// Note ids of live (keyboard / preview) notes carry this bit.
pub const LIVE: u64 = 1 << 62;

/// Mixer tracks recorded at once (FL: armed tracks).
pub const MAX_TAPS: usize = 16;

/// Frames the analyser ring holds (a power of two).
pub const SCOPE_LEN: usize = 4096;

/// The last frames (mono, post-fader) of one mixer track, for an EQ's
/// spectrum: the UI sets `track` (`usize::MAX`: off) and reads `ring`
/// ending at `pos`. Lock-free; a torn read only blurs one frame of the view.
pub struct Scope {
    pub track: std::sync::atomic::AtomicUsize,
    pub pos: std::sync::atomic::AtomicUsize,
    pub ring: Vec<std::sync::atomic::AtomicU32>,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            track: usize::MAX.into(),
            pos: 0.into(),
            ring: (0..SCOPE_LEN).map(|_| 0.into()).collect(),
        }
    }
}

impl Scope {
    /// The newest `out.len()` frames, oldest first.
    pub fn read(&self, out: &mut [f32]) {
        use std::sync::atomic::Ordering::Relaxed;
        let end = self.pos.load(Relaxed);
        let n = out.len().min(SCOPE_LEN);
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = f32::from_bits(self.ring[(end + SCOPE_LEN - n + i) % SCOPE_LEN].load(Relaxed));
        }
    }
}

/// A recording tap: post-fader frames of mixer `track` go into `tx`. Frames
/// that do not fit (the reader fell behind) are counted in `dropped`.
pub struct Tap {
    pub track: usize,
    pub tx: rtrb::Producer<[f32; 2]>,
    pub dropped: u64,
}

/// Synth channels that can have their own effects at once.
const MAX_SYNTH_FX: usize = 32;

/// One synth channel's own effect chain (see `Engine::run_synth_fx`).
struct SynthFx {
    /// The channel; `None`: a free entry.
    inst: Option<u64>,
    buf: Vec<f32>,
    states: Vec<Option<crate::fx::FxState>>,
}

/// Sampler channels whose program has built-in effects at once.
const MAX_SAMPLER_FX: usize = 16;
/// How long a program's delay and reverb sends keep running after the last input.
const DELAY_TAIL_SECONDS: f64 = 4.0;
const REVERB_TAIL_SECONDS: f64 = 3.5;
/// Programs/trigger groups remembered for round robin (reserved up front).
const MAX_TRIGGER_STATES: usize = 64;

/// One sampler channel's program effects (`ProgramFx`): its voices render
/// into `buf`, the chain runs once per block, the result joins the channel's bus.
struct SamplerFx {
    /// The channel; `None`: a free entry.
    inst: Option<u64>,
    buf: Vec<f32>,
    /// Still sounding after the voices: a delay or reverb tail is due or audible.
    ringing: bool,
    /// Frames since the effects last had input.
    quiet: u64,
    dsp: ProgFxDsp,
}

/// Fixed-size buffers of the program effects, sized once for the sample
/// rate (zeroed memory, so untouched pages cost nothing).
struct ProgFxDsp {
    chorus: [Vec<f32>; 2],
    chorus_w: usize,
    chorus_phase: f32,
    delay: [Vec<f32>; 2],
    delay_w: [usize; 2],
    delay_lp: [f32; 2],
    comb: [[Vec<f32>; 4]; 2],
    comb_i: [[usize; 4]; 2],
    comb_lp: [[f32; 4]; 2],
    allpass: [[Vec<f32>; 2]; 2],
    allpass_i: [[usize; 2]; 2],
    rate: f32,
}

impl ProgFxDsp {
    const COMBS: [usize; 4] = [1116, 1188, 1277, 1356];
    const ALLPASSES: [usize; 2] = [556, 441];
    const SPREAD: usize = 23;

    fn new(rate: f64) -> Self {
        let rate = rate.max(8000.0);
        let secs = |s: f64| (s * rate).round() as usize;
        let tuned = |n: usize| ((n as f64 * rate / 44_100.0).round() as usize).max(8);
        Self {
            chorus: [vec![0.0; secs(0.03) + 4], vec![0.0; secs(0.03) + 4]],
            chorus_w: 0,
            chorus_phase: 0.0,
            delay: [vec![0.0; secs(0.35).max(2)], vec![0.0; secs(0.26).max(2)]],
            delay_w: [0; 2],
            delay_lp: [0.0; 2],
            comb: std::array::from_fn(|ch| {
                std::array::from_fn(|i| vec![0.0; tuned(Self::COMBS[i] + ch * Self::SPREAD)])
            }),
            comb_i: [[0; 4]; 2],
            comb_lp: [[0.0; 4]; 2],
            allpass: std::array::from_fn(|ch| {
                std::array::from_fn(|i| vec![0.0; tuned(Self::ALLPASSES[i] + ch * Self::SPREAD)])
            }),
            allpass_i: [[0; 2]; 2],
            rate: rate as f32,
        }
    }

    /// Forget everything the effects remember (memset, no allocation).
    fn clear(&mut self) {
        for v in self
            .chorus
            .iter_mut()
            .chain(self.delay.iter_mut())
            .chain(self.comb.iter_mut().flatten())
            .chain(self.allpass.iter_mut().flatten())
        {
            v.fill(0.0);
        }
        self.chorus_w = 0;
        self.chorus_phase = 0.0;
        self.delay_w = [0; 2];
        self.delay_lp = [0.0; 2];
        self.comb_i = [[0; 4]; 2];
        self.comb_lp = [[0.0; 4]; 2];
        self.allpass_i = [[0; 2]; 2];
    }

    /// Run drive, then the chorus, delay and reverb sends over interleaved stereo.
    fn process(&mut self, fx: &ProgramFx, buf: &mut [f32]) {
        let (drive, chorus, delay, reverb) = (
            fx.drive as f32,
            fx.chorus as f32,
            fx.delay as f32,
            fx.reverb as f32,
        );
        let k = 1.0 + drive * 8.0;
        let norm = k.tanh();
        let cho_inc = 0.65 / self.rate;
        for frame in buf.as_chunks_mut::<2>().0 {
            let mut dry = [frame[0], frame[1]];
            if drive > 0.0 {
                for x in dry.iter_mut() {
                    *x += ((*x * k).tanh() / norm - *x) * drive;
                }
            }
            let mut y = dry;
            if chorus > 0.0 {
                // 8..22 ms, left and right a quarter cycle apart.
                let (sn, cs) = (std::f32::consts::TAU * self.chorus_phase).sin_cos();
                let len = self.chorus[0].len();
                for (ch, lfo) in [sn, cs].into_iter().enumerate() {
                    let buf = &mut self.chorus[ch];
                    buf[self.chorus_w] = dry[ch];
                    let d = (0.015 + 0.007 * lfo) * self.rate;
                    let pos = self.chorus_w as f32 - d;
                    let pos = if pos < 0.0 { pos + len as f32 } else { pos };
                    let i = pos as usize;
                    let f = pos - i as f32;
                    let (a, b) = (buf[i % len], buf[(i + 1) % len]);
                    y[ch] += (a + (b - a) * f) * chorus * 0.5;
                }
                self.chorus_w = (self.chorus_w + 1) % len;
                self.chorus_phase = (self.chorus_phase + cho_inc).fract();
            }
            if delay > 0.0 {
                for ch in 0..2 {
                    let w = self.delay_w[ch];
                    let buf = &mut self.delay[ch];
                    let o = buf[w];
                    self.delay_lp[ch] += (o - self.delay_lp[ch]) * 0.45;
                    buf[w] = dry[ch] + self.delay_lp[ch] * 0.42;
                    self.delay_w[ch] = (w + 1) % buf.len();
                    y[ch] += o * delay * 0.55;
                }
            }
            if reverb > 0.0 {
                let input = (dry[0] + dry[1]) * 0.5 * 0.5;
                for ch in 0..2 {
                    let mut sum = 0.0;
                    for i in 0..4 {
                        let buf = &mut self.comb[ch][i];
                        let at = self.comb_i[ch][i];
                        let o = buf[at];
                        self.comb_lp[ch][i] = o * 0.8 + self.comb_lp[ch][i] * 0.2;
                        buf[at] = input + self.comb_lp[ch][i] * 0.88;
                        self.comb_i[ch][i] = (at + 1) % buf.len();
                        sum += o;
                    }
                    let mut x = sum * 0.25;
                    for i in 0..2 {
                        let buf = &mut self.allpass[ch][i];
                        let at = self.allpass_i[ch][i];
                        let b = buf[at];
                        buf[at] = x + b * 0.5;
                        self.allpass_i[ch][i] = (at + 1) % buf.len();
                        x = b - x;
                    }
                    y[ch] += x * reverb * 0.6;
                }
            }
            frame[0] = y[0];
            frame[1] = y[1];
        }
    }
}

/// Per-voice synth values worked out once and reused while their inputs
/// stay put (see `Voice::render_synth`). NaN keys: nothing cached yet.
#[derive(Clone, Copy)]
struct TideCache {
    uni_key: [f64; 6],
    uni_a: [f64; 7],
    uni_b: [f64; 7],
    /// Table mip levels per unison voice (A, B), for `levels_hz`.
    lvl_a: [usize; 7],
    lvl_b: [usize; 7],
    levels_hz: f64,
    hz_key: f64,
    hz: f64,
    warp_key: [f64; 2],
    warp_exp: [f64; 2],
    cut_key: f64,
    g: f64,
}

impl Default for TideCache {
    fn default() -> Self {
        Self {
            uni_key: [f64::NAN; 6],
            uni_a: [1.0; 7],
            uni_b: [1.0; 7],
            lvl_a: [0; 7],
            lvl_b: [0; 7],
            levels_hz: f64::NAN,
            hz_key: f64::NAN,
            hz: 0.0,
            warp_key: [f64::NAN; 2],
            warp_exp: [1.0; 2],
            cut_key: f64::NAN,
            g: 0.0,
        }
    }
}

/// Synth channels that can hold live controller state at once.
const MAX_LIVE_CTL: usize = 64;

/// The MIDI controllers a synth channel last got: per MIDI channel (MPE),
/// and the latest from any channel (an ordinary keyboard).
#[derive(Clone, Copy)]
struct LiveCtl {
    inst: u64,
    /// Pitch wheel −1..1, pressure and slide 0..1, per MIDI channel.
    bend: [f64; 16],
    pressure: [f64; 16],
    slide: [f64; 16],
    any_bend: f64,
    any_pressure: f64,
    any_slide: f64,
    wheel: f64,
}

impl LiveCtl {
    fn new(inst: u64) -> Self {
        Self {
            inst,
            bend: [0.0; 16],
            pressure: [0.0; 16],
            slide: [0.0; 16],
            any_bend: 0.0,
            any_pressure: 0.0,
            any_slide: 0.0,
            wheel: 0.0,
        }
    }

    /// Take one MIDI message (status with its channel).
    fn take(&mut self, [status, d1, d2]: [u8; 3]) {
        let ch = (status & 0x0F) as usize;
        match status & 0xF0 {
            0xE0 => {
                let v = ((((d2 as i32) << 7) | d1 as i32) - 8192) as f64 / 8192.0;
                self.bend[ch] = v;
                self.any_bend = v;
            }
            0xD0 => {
                self.pressure[ch] = d1 as f64 / 127.0;
                self.any_pressure = self.pressure[ch];
            }
            0xB0 if d1 == 1 => self.wheel = d2 as f64 / 127.0,
            0xB0 if d1 == 74 => {
                self.slide[ch] = d2 as f64 / 127.0;
                self.any_slide = self.slide[ch];
            }
            // Reset all controllers.
            0xB0 if d1 == 121 => *self = Self::new(self.inst),
            _ => {}
        }
    }

    /// (bend in semitones, pressure, slide) for a note on MIDI channel `ch`.
    fn for_note(&self, ch: u8, p: &crate::synth::SynthPatch) -> (f64, f64, f64) {
        let ch = (ch & 15) as usize;
        if p.mpe {
            // Channel 1 is the master: it bends every note; the others their own.
            let own = if ch == 0 { 0.0 } else { self.bend[ch] * 48.0 };
            (
                self.bend[0] * p.bend_range + own,
                self.pressure[ch],
                self.slide[ch],
            )
        } else {
            (
                self.any_bend * p.bend_range,
                self.any_pressure,
                self.any_slide,
            )
        }
    }
}

pub struct Engine {
    rate: f64,
    pub(crate) clip_output: bool,
    /// Song-mode tempo map as frames (None: one tempo, the old formulas).
    /// Rebuilt only when the map changes; lookups never allocate.
    tempo: Option<crate::tempo::TempoTable>,
    block_at: u64,
    block_wrap: Option<(usize, u64)>,
    tempo_key: (Vec<crate::tempo::TempoPoint>, u64),
    /// Seconds per step at the playhead this block (map tempo).
    sps_block: f64,
    tables: &'static crate::synth::TableBank,
    voices: Vec<Voice>,
    hits: Vec<Hit>,
    /// Last key per instrument, for mono glide.
    last_key: Vec<(u64, u8)>,
    next_note: u64,
    transport: (bool, u64, u64),
    /// Frames rendered since the engine started (song clock for Free Run LFOs).
    clock: u64,
    /// One stereo buffer per mixer track (0 = Master) for the current block.
    buses: Vec<Vec<f32>>,
    /// Inserts in processing order (each after the inserts that feed it), and
    /// a done flag per insert: sized with the buses, reused every block.
    mix_order: Vec<usize>,
    mix_done: Vec<bool>,
    /// Unmixed inserts that still send into each insert (routing order).
    mix_feeds: Vec<u32>,
    /// Effect state per mixer track and slot.
    fx: Vec<Vec<Option<crate::fx::FxState>>>,
    /// Built-in strip EQ per mixer track.
    eq: Vec<Option<crate::fx::FxState>>,
    /// Fader gains reached at the end of the last block (smoothing).
    gain_state: Vec<Option<[f32; 2]>>,
    /// Wet/dry mix reached at the end of the last block per effect slot
    /// (`track * FX_SLOTS + slot`), for the ~5 ms mix smoother.
    fx_mix_state: Vec<Option<f32>>,
    /// The hosted plugin each slot's mix smoother last saw: a swap resets it.
    fx_mix_plugin: Vec<Option<u64>>,
    /// Smoothed built-in effect parameters per effect slot. An empty value
    /// list means "not started": the next block snaps to its target. The
    /// vectors are reserved in `grow_buses` and never freed on the audio
    /// thread (a reset only clears them).
    fx_param_state: Vec<Option<(crate::fx::FxKind, Vec<f32>)>>,
    /// Post-fader peak per mixer track for the last block.
    peaks: Vec<f32>,
    scratch: Vec<f32>,
    /// Extra output pairs of a multi-out plugin, `frames * 2` apart.
    extra: Vec<f32>,
    /// Latency compensation (PDC): the built-in instruments' share of each
    /// bus, delayed to the slowest plugin instrument (`src_latency`) …
    bus_pdc: Vec<crate::plugin::Pdc>,
    /// … and each insert's output, delayed to the slowest effect chain.
    track_pdc: Vec<crate::plugin::Pdc>,
    /// Each insert's effect latency this block.
    fx_latency: Vec<usize>,
    src_latency: usize,
    fx_latency_max: usize,
    /// Synth channels with their own effects: their voices render here, the
    /// chain runs once per block, then the result joins the channel's bus.
    synth_fx: Vec<SynthFx>,
    /// Live MIDI controllers per synth channel (sized once: no allocation).
    ctl: Vec<LiveCtl>,
    /// Gain reduction in dB of each built-in effect slot this block
    /// (`track * FX_SLOTS + slot`), for the compressor's meter.
    reduction: Vec<f32>,
    /// Armed mixer tracks being recorded: post-fader frames go to each tap.
    taps: Vec<Tap>,
    /// The track an open EQ is analysing.
    scope: std::sync::Arc<Scope>,
    /// Dedicated post-fader Master scope for the transport deck oscilloscope / spectrum.
    master_scope: std::sync::Arc<Scope>,
    /// Hosted plugins (instruments and effects), installed from the UI thread.
    plugins: Vec<crate::plugin::Instance>,
    /// Block offset and engine frame of the hit being triggered (plugin events).
    cursor: usize,
    hit_clock: u64,
    /// Transport handed to plugins for the current block.
    plugin_transport: crate::plugin::Transport,
    /// Automation values for the current block (song mode), reserved up front.
    auto: Vec<crate::automation::ValueAt>,
    /// An automated effect's parameters for one process call.
    auto_params: Vec<f32>,
    /// Live overrides from the UI (`link::ParamReceiver`): a fader or effect
    /// knob moved while the song plays or sits stopped. Automation wins over
    /// them; a newer song snapshot supersedes them (`clear_live_params`).
    live_params: Vec<LiveParam>,
    /// Rendered singing the UI side publishes; read without locking.
    pub vocal: std::sync::Arc<arc_swap::ArcSwap<crate::vocal::VocalMap>>,
    /// Lyric hash (`vocal::lyric_hash`) the next live vocal note sings; 0 = the channel's default.
    pub live_lyric: std::sync::Arc<std::sync::atomic::AtomicU64>,
    phrases: Vec<PhraseVoice>,
    hit_skip: usize,
    /// Sampler channels with program effects (see `SamplerFx`).
    sampler_fx: Vec<SamplerFx>,
    /// Round robin: (channel, trigger group, notes counted, last zone played).
    trigger_state: Vec<(u64, u8, u32, u32)>,
}

/// Plugin instances one engine holds (reserved up front).
pub const MAX_PLUGINS: usize = 64;

/// Distinct controls the UI may override live at once (reserved up front).
pub const MAX_LIVE_PARAMS: usize = 256;

/// One live override: the control and the value it holds until the next song.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveParam {
    pub target: crate::automation::Target,
    pub value: f32,
}

/// The value of `target` from automation, else from a live override.
fn override_of(
    auto: &[crate::automation::ValueAt],
    live: &[LiveParam],
    target: crate::automation::Target,
) -> Option<f32> {
    crate::automation::find(auto, target)
        .or_else(|| live.iter().find(|p| p.target == target).map(|p| p.value))
}

impl Engine {
    pub fn new(rate: u32) -> Self {
        static TABLES: std::sync::OnceLock<crate::synth::TableBank> = std::sync::OnceLock::new();
        Self {
            rate: rate as f64,
            clip_output: true,
            tables: TABLES.get_or_init(crate::synth::TableBank::new),
            voices: Vec::with_capacity(MAX_VOICES),
            hits: Vec::with_capacity(256),
            last_key: Vec::with_capacity(32),
            next_note: 0,
            transport: (false, 0, 0),
            clock: 0,
            buses: Vec::new(),
            mix_order: Vec::new(),
            mix_done: Vec::new(),
            mix_feeds: Vec::new(),
            fx: Vec::new(),
            gain_state: Vec::new(),
            fx_mix_state: Vec::new(),
            fx_mix_plugin: Vec::new(),
            fx_param_state: Vec::new(),
            eq: Vec::new(),
            peaks: Vec::new(),
            scratch: Vec::new(),
            extra: Vec::new(),
            bus_pdc: Vec::new(),
            track_pdc: Vec::new(),
            fx_latency: Vec::new(),
            src_latency: 0,
            fx_latency_max: 0,
            reduction: Vec::new(),
            ctl: Vec::with_capacity(MAX_LIVE_CTL),
            synth_fx: (0..MAX_SYNTH_FX)
                .map(|_| SynthFx {
                    inst: None,
                    buf: Vec::new(),
                    states: Vec::with_capacity(crate::synth::FX_KINDS.len()),
                })
                .collect(),
            taps: Vec::with_capacity(MAX_TAPS),
            scope: Default::default(),
            master_scope: Default::default(),
            plugins: Vec::with_capacity(MAX_PLUGINS),
            cursor: 0,
            hit_clock: 0,
            plugin_transport: crate::plugin::Transport {
                rate,
                tempo: 120.0,
                playing: false,
                frame: 0,
            },
            tempo: None,
            block_at: 0,
            block_wrap: None,
            tempo_key: (Vec::new(), 0),
            sps_block: seconds_per_step(120.0),
            auto: Vec::with_capacity(64),
            auto_params: Vec::with_capacity(64),
            live_params: Vec::new(),
            vocal: std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
                crate::vocal::VocalMap::default(),
            )),
            live_lyric: Default::default(),
            phrases: Vec::with_capacity(MAX_PHRASES),
            hit_skip: 0,
            sampler_fx: (0..MAX_SAMPLER_FX)
                .map(|_| SamplerFx {
                    inst: None,
                    buf: Vec::new(),
                    ringing: false,
                    quiet: 0,
                    dsp: ProgFxDsp::new(rate as f64),
                })
                .collect(),
            trigger_state: Vec::with_capacity(MAX_TRIGGER_STATES),
        }
    }

    /// Install a plugin instance built off the audio thread. Returns the one it
    /// replaces (same id), or the new one back when the table is full.
    pub fn adopt_plugin(
        &mut self,
        inst: crate::plugin::Instance,
    ) -> Option<crate::plugin::Instance> {
        if let Some(i) = self.plugins.iter().position(|p| p.id == inst.id) {
            return Some(std::mem::replace(&mut self.plugins[i], inst));
        }
        if self.plugins.len() >= self.plugins.capacity() {
            return Some(inst);
        }
        self.plugins.push(inst);
        None
    }

    /// Remove an instance; the caller frees it off the audio thread.
    pub fn take_plugin(&mut self, id: u64) -> Option<crate::plugin::Instance> {
        let i = self.plugins.iter().position(|p| p.id == id)?;
        Some(self.plugins.swap_remove(i))
    }

    pub fn plugin_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.plugins.iter().map(|p| p.id)
    }

    /// Anything that may sound while stopped: voices, or plugins (tails, held notes).
    /// The tempo map in frames for the block just processed (Song mode with a map).
    /// Where the last `process` block started in the loop, and the frame
    /// inside it where the loop restarted with the position it restarted at.
    pub fn block_timeline(&self) -> (u64, Option<(usize, u64)>) {
        (self.block_at, self.block_wrap)
    }

    pub fn tempo_table(&self) -> Option<&crate::tempo::TempoTable> {
        self.tempo.as_ref()
    }

    pub fn busy(&self) -> bool {
        !self.voices.is_empty()
            || !self.phrases.is_empty()
            || !self.plugins.is_empty()
            || self.sampler_fx.iter().any(|c| c.ringing)
    }

    /// A raw MIDI message (controller, pitch bend) to a plugin channel now.
    pub fn live_midi(&mut self, song: &Song, inst: u64, bytes: [u8; 3]) {
        match song.instrument(inst).map(|i| &i.kind) {
            Some(InstrumentKind::Synth { .. }) => {
                let at = match self.ctl.iter().position(|c| c.inst == inst) {
                    Some(i) => i,
                    None if self.ctl.len() < self.ctl.capacity() => {
                        self.ctl.push(LiveCtl::new(inst));
                        self.ctl.len() - 1
                    }
                    None => return,
                };
                self.ctl[at].take(bytes);
            }
            Some(InstrumentKind::Plugin { plugin }) => {
                // Plugins get channel 1, as before MPE reached the synth.
                let bytes = [bytes[0] & 0xF0, bytes[1], bytes[2]];
                if let Some(pl) = self.plugins.iter_mut().find(|p| p.id == plugin.id) {
                    pl.push(crate::plugin::PluginEvent::Midi { at: 0, bytes });
                }
            }
            _ => {}
        }
    }

    /// Size every mixer buffer up front (live use): after this, blocks of up
    /// to `frames` and mixers of up to `tracks` never allocate in `process`.
    pub fn reserve(&mut self, tracks: usize, frames: usize) {
        self.grow_buses(tracks, frames);
        self.live_params.reserve(MAX_LIVE_PARAMS);
        for c in &mut self.synth_fx {
            if c.buf.len() < frames * 2 {
                c.buf.resize(frames * 2, 0.0);
            }
        }
        for c in &mut self.sampler_fx {
            if c.buf.len() < frames * 2 {
                c.buf.resize(frames * 2, 0.0);
            }
        }
    }

    /// Hold `target` at `value` until a newer song supersedes it (a fader or
    /// effect knob the UI moved). Never allocates once `reserve` has run; a
    /// full list drops the newest control instead of growing.
    pub fn apply_live_param(&mut self, target: crate::automation::Target, value: f32) {
        if let Some(p) = self.live_params.iter_mut().find(|p| p.target == target) {
            p.value = value;
            return;
        }
        // `reserve` holds `MAX_LIVE_PARAMS`; an offline engine that never
        // reserved grows up to the same count.
        if self.live_params.len() < MAX_LIVE_PARAMS {
            self.live_params.push(LiveParam { target, value });
        }
    }

    /// The live override held for `target`, if any.
    pub fn live_param(&self, target: crate::automation::Target) -> Option<f32> {
        override_of(&[], &self.live_params, target)
    }

    /// Forget every live override (a newer song snapshot carries them). Keeps
    /// the reserved capacity. Stopping the transport does not call this: a
    /// fader moved while stopped still holds when Play is hit.
    pub fn clear_live_params(&mut self) {
        self.live_params.clear();
    }

    /// Install a slot state built off the audio thread. Returns the state it
    /// replaces so the caller can free it elsewhere.
    pub fn adopt_fx(
        &mut self,
        track: usize,
        slot: usize,
        state: crate::fx::FxState,
    ) -> Option<crate::fx::FxState> {
        let slots = self.fx.get_mut(track)?;
        if slot >= slots.len() {
            if slot >= slots.capacity() {
                return Some(state);
            }
            slots.resize_with(slot + 1, || None);
        }
        reset_fx_smoothers(
            &mut self.fx_mix_state,
            &mut self.fx_mix_plugin,
            &mut self.fx_param_state,
            track * crate::sequence::FX_SLOTS + slot,
        );
        slots[slot].replace(state)
    }

    /// The analyser the UI reads (see [`Scope`]).
    pub fn scope(&self) -> std::sync::Arc<Scope> {
        self.scope.clone()
    }

    /// Dedicated post-fader Master analyser for the Deck monitor.
    pub fn master_scope(&self) -> std::sync::Arc<Scope> {
        self.master_scope.clone()
    }

    /// Push `frames` of silence into active scopes when the transport is stopped
    /// and idle, so oscilloscope / spectrum displays decay cleanly to zero
    /// instead of freezing on the last audio block.
    pub fn push_silence_scopes(&self, frames: usize) {
        use std::sync::atomic::Ordering::Relaxed;
        let zero = 0.0_f32.to_bits();
        if self.master_scope.track.load(Relaxed) == 0 {
            let mut at = self.master_scope.pos.load(Relaxed);
            for _ in 0..frames {
                self.master_scope.ring[at].store(zero, Relaxed);
                at = (at + 1) % SCOPE_LEN;
            }
            self.master_scope.pos.store(at, Relaxed);
        }
        if self.scope.track.load(Relaxed) != usize::MAX {
            let mut at = self.scope.pos.load(Relaxed);
            for _ in 0..frames {
                self.scope.ring[at].store(zero, Relaxed);
                at = (at + 1) % SCOPE_LEN;
            }
            self.scope.pos.store(at, Relaxed);
        }
    }

    /// Start recording a mixer track. Gives the tap back when all are in use.
    pub fn add_tap(&mut self, tap: Tap) -> Result<(), Tap> {
        if self.taps.len() >= MAX_TAPS {
            return Err(tap);
        }
        self.taps.push(tap);
        Ok(())
    }

    /// Stop recording: hand back one tap (call until `None`).
    pub fn take_tap(&mut self) -> Option<Tap> {
        self.taps.pop()
    }

    /// Post-fader peak of each mixer track over the last block (0 = Master).
    /// Frames the output lags the timeline after latency compensation (the
    /// slowest instrument plus the slowest insert chain and Master's own).
    pub fn latency(&self) -> usize {
        self.src_latency + self.fx_latency_max + self.fx_latency.first().copied().unwrap_or(0)
    }

    /// Gain reduction in dB of each built-in effect slot in the last block,
    /// at `track * FX_SLOTS + slot` (0 for effects that do not reduce).
    pub fn reductions(&self) -> &[f32] {
        &self.reduction
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// Silence everything now (transport stopped).
    pub fn stop(&mut self) {
        self.voices.clear();
        self.phrases.clear();
        self.last_key.clear();
        // Round robin starts over, program effect tails are cut.
        self.trigger_state.clear();
        for c in &mut self.sampler_fx {
            if c.inst.take().is_some() || c.ringing {
                c.dsp.clear();
            }
            c.ringing = false;
            c.quiet = 0;
        }
        // Sequenced plugin notes end now; live (held) ones keep sounding.
        for pl in &mut self.plugins {
            let mut i = 0;
            while i < pl.notes.len() {
                if pl.notes[i].2 != u64::MAX {
                    let pitch = pl.notes[i].1;
                    pl.notes.swap_remove(i);
                    pl.push(crate::plugin::PluginEvent::NoteOff { at: 0, pitch });
                } else {
                    i += 1;
                }
            }
            pl.stop_fl_pitch();
        }
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
            + self.phrases.len()
            + self.sampler_fx.iter().filter(|c| c.ringing).count()
    }

    /// What each channel (by its index in `song.instruments`, up to
    /// `out.len()`) sounds like now: 2 = a note is playing, 1 = only its
    /// tail rings on (release, a one-shot past its note, a plugin's decay),
    /// 0 = silent. No allocation.
    pub fn channel_activity(&self, song: &Song, out: &mut [u8]) {
        let n = song.instruments.len().min(out.len());
        out[..n].fill(0);
        let index = |id: u64| song.instruments[..n].iter().position(|i| i.id == id);
        for v in &self.voices {
            if let Some(i) = index(v.inst) {
                let s = if v.t < v.key_until && v.kill.is_none() {
                    2
                } else {
                    1
                };
                out[i] = out[i].max(s);
            }
        }
        for pv in &self.phrases {
            if let Some(i) = index(pv.inst) {
                out[i] = out[i].max(if pv.fade.is_some() { 1 } else { 2 });
            }
        }
        for pl in &self.plugins {
            if let Some(i) = song.instruments[..n]
                .iter()
                .position(|x| x.plugin().is_some_and(|p| p.id == pl.id))
            {
                let s = if !pl.notes.is_empty() {
                    2
                } else if pl.level > 1e-4 {
                    1
                } else {
                    0
                };
                out[i] = out[i].max(s);
            }
        }
    }

    /// Render interleaved stereo into `out` starting at loop position `start`.
    /// Returns the position after the block. With `looping`, the timeline wraps.
    pub fn process(
        &mut self,
        song: &Song,
        pattern: u64,
        song_mode: bool,
        start: u64,
        out: &mut [f32],
        looping: bool,
    ) -> u64 {
        crate::denormal::flush_to_zero();
        out.fill(0.0);
        let frames = out.len() / 2;
        self.prepare_buses(song.mixer.len().max(1), frames);
        self.prepare_sampler_fx(song, frames);
        self.sync_tempo(song, song_mode);
        let length = match &self.tempo {
            Some(t) => t.frames_at(song.end_bar_tick()) as u64,
            None => loop_frames(song, pattern, song_mode, self.rate as u32),
        }
        .max(1);
        // The loop wraps to its start: 0, or the song's time selection.
        let bounds = if looping {
            loop_bounds(
                song,
                self.tempo.as_ref(),
                pattern,
                song_mode,
                self.rate as u32,
            )
        } else {
            (0, u64::MAX)
        };
        let transport = (song_mode, pattern, length);
        if transport != self.transport {
            // Pattern switched or tempo changed: let ringing voices fade, not click.
            for v in &mut self.voices {
                v.kill_now();
            }
            for pv in &mut self.phrases {
                pv.fade.get_or_insert((256, 256));
            }
            self.transport = transport;
        }
        let mut pos = if looping {
            wrap_frame(start, bounds)
        } else {
            start
        };
        self.block_at = pos;
        self.block_wrap = None;
        // Automation clips play in Song mode (FL), read once per block.
        let mut bpm = song.tempo;
        if song_mode {
            let at = self.song_tick(song, pos);
            crate::automation::values_at(song, at, &mut self.auto);
            if let Some(t) = &self.tempo {
                bpm = t.bpm_at(at);
                self.sps_block = seconds_per_step(bpm);
            }
        } else {
            self.auto.clear();
        }
        let block_clock = self.clock;
        self.plugin_transport = crate::plugin::Transport {
            rate: self.rate as u32,
            tempo: bpm,
            playing: true,
            frame: pos,
        };
        let mut done = 0;
        while done < frames {
            let span = if looping {
                ((bounds.1 - pos) as usize).min(frames - done)
            } else {
                frames - done
            };
            self.segment(song, pattern, song_mode, pos, done, span);
            done += span;
            pos += span as u64;
            if looping && pos >= bounds.1 {
                pos = bounds.0;
                if done < frames {
                    self.block_wrap = Some((done, pos));
                }
            }
        }
        self.cursor = 0;
        self.run_synth_fx(song, frames);
        self.run_sampler_fx(song, frames);
        self.run_plugins(song, frames, block_clock);
        self.mix(song, frames, out);
        if self.clip_output {
            for s in out.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }
        }
        pos
    }

    /// Render `frames` frames of the timeline from `from` into the buses at block offset `at`.
    fn segment(
        &mut self,
        song: &Song,
        pattern: u64,
        song_mode: bool,
        from: u64,
        at: usize,
        frames: usize,
    ) {
        let to = from + frames as u64;
        self.collect_hits(song, pattern, song_mode, from, to);
        let mut cursor = from;
        let mut written = 0;
        for i in 0..self.hits.len() {
            let hit = self.hits[i];
            let n = (hit.at - cursor) as usize;
            self.render_voices(song, at + written, n);
            self.render_phrases(song, at + written, n);
            written += n;
            cursor = hit.at;
            let clock = (self.clock + written as u64) as f64 / self.rate;
            self.cursor = at + written;
            self.hit_clock = self.clock + written as u64;
            self.trigger(song, clock, hit);
        }
        self.render_voices(song, at + written, frames - written);
        self.render_phrases(song, at + written, frames - written);
        self.clock += frames as u64;
    }

    /// Grow (never shrink) the per-track buffers; shrinking would free memory
    /// on the audio thread.
    fn grow_buses(&mut self, tracks: usize, frames: usize) {
        if self.buses.len() < tracks {
            let rate = self.rate as f32;
            self.buses.resize_with(tracks, Vec::new);
            self.fx
                .resize_with(tracks, || Vec::with_capacity(crate::sequence::FX_SLOTS));
            self.eq.resize_with(tracks, || {
                Some(crate::fx::FxState::new(crate::fx::FxKind::Eq, rate))
            });
            self.gain_state.resize(tracks, None);
            self.fx_mix_state
                .resize(tracks * crate::sequence::FX_SLOTS, None);
            self.fx_mix_plugin
                .resize(tracks * crate::sequence::FX_SLOTS, None);
            self.fx_param_state
                .resize_with(tracks * crate::sequence::FX_SLOTS, || {
                    Some((crate::fx::FxKind::Eq, Vec::with_capacity(64)))
                });
            self.peaks.resize(tracks, 0.0);
            self.mix_order
                .reserve(tracks.saturating_sub(self.mix_order.capacity()));
            self.mix_done.resize(tracks, false);
            self.mix_feeds.resize(tracks, 0);
            self.bus_pdc.resize_with(tracks, Default::default);
            self.track_pdc.resize_with(tracks, Default::default);
            self.fx_latency.resize(tracks, 0);
            self.reduction
                .resize(tracks * crate::sequence::FX_SLOTS, 0.0);
        }
        for b in &mut self.buses {
            if b.len() < frames * 2 {
                b.resize(frames * 2, 0.0);
            }
        }
        if self.scratch.len() < frames * 2 {
            self.scratch.resize(frames * 2, 0.0);
        }
    }

    /// Size and clear the per-track buffers for a block.
    fn prepare_buses(&mut self, tracks: usize, frames: usize) {
        self.grow_buses(tracks, frames);
        for b in &mut self.buses[..tracks] {
            b[..frames * 2].fill(0.0);
        }
        for c in self.synth_fx.iter_mut().filter(|c| c.inst.is_some()) {
            if c.buf.len() < frames * 2 {
                // Once per larger block (like the buses).
                c.buf.resize(frames * 2, 0.0);
            }
            c.buf[..frames * 2].fill(0.0);
        }
        // Tracks past the song's mixer read as silent.
        for p in &mut self.peaks[tracks..] {
            *p = 0.0;
        }
    }

    /// Synth channels' own effect chains, then each into its channel's bus.
    /// Runs every block while the entry lives, so delay and reverb tails ring on.
    fn run_synth_fx(&mut self, song: &Song, frames: usize) {
        let n = frames * 2;
        let (rate, tempo) = (self.rate as f32, song.tempo as f32);
        for c in &mut self.synth_fx {
            let Some(id) = c.inst else { continue };
            let Some(inst) = song.instrument(id) else {
                c.inst = None;
                continue;
            };
            let InstrumentKind::Synth { patch } = &inst.kind else {
                c.inst = None;
                continue;
            };
            if c.buf.len() < n {
                continue;
            }
            let buf = &mut c.buf[..n];
            for (k, slot) in patch.fx.iter().enumerate() {
                if k >= c.states.capacity() {
                    break;
                }
                if c.states.len() <= k {
                    // Within the capacity reserved at start: no allocation.
                    c.states.resize_with(k + 1, || None);
                }
                if !slot.enabled {
                    continue;
                }
                if c.states[k].as_ref().is_none_or(|st| st.kind() != slot.kind) {
                    // Offline renders build here; live, the UI thread sends it first (`adopt_synth_fx`).
                    c.states[k] = Some(crate::fx::FxState::new(slot.kind, rate));
                }
                let st = c.states[k].as_mut().expect("built");
                if slot.mix >= 1.0 {
                    st.process(&slot.params, buf, rate, tempo);
                } else {
                    let dry = &mut self.scratch[..n];
                    dry.copy_from_slice(buf);
                    st.process(&slot.params, buf, rate, tempo);
                    for (w, d) in buf.iter_mut().zip(dry.iter()) {
                        *w = *d + (*w - *d) * slot.mix;
                    }
                }
            }
            let bus = (inst.mixer as usize).min(self.buses.len() - 1);
            for (d, s) in self.buses[bus][..n].iter_mut().zip(buf.iter()) {
                *d += *s;
            }
        }
    }

    /// Give every sampler channel whose program has effects an entry, and
    /// size and clear the buffers of the entries in use. Nothing to do (and
    /// nothing touched) while no program uses effects.
    fn prepare_sampler_fx(&mut self, song: &Song, frames: usize) {
        for i in &song.instruments {
            let Some(p) = i.sampler_preset() else {
                continue;
            };
            if !p.program.fx.active() || self.sampler_fx.iter().any(|c| c.inst == Some(i.id)) {
                continue;
            }
            if let Some(c) = self.sampler_fx.iter_mut().find(|c| c.inst.is_none()) {
                c.inst = Some(i.id);
            }
        }
        let n = frames * 2;
        for c in self.sampler_fx.iter_mut().filter(|c| c.inst.is_some()) {
            if c.buf.len() < n {
                // Once per larger block (like the buses); `reserve` does it up front.
                c.buf.resize(n, 0.0);
            }
            c.buf[..n].fill(0.0);
        }
    }

    /// Program effects of each sampler channel, then into its channel's bus.
    /// Runs while voices sound or the tail rings.
    fn run_sampler_fx(&mut self, song: &Song, frames: usize) {
        let n = frames * 2;
        for c in &mut self.sampler_fx {
            let Some(id) = c.inst else { continue };
            let inst = song.instrument(id);
            let Some(fx) = inst
                .and_then(|i| i.sampler_preset())
                .map(|p| &p.program.fx)
                .filter(|fx| fx.active())
            else {
                // Effects turned off, or the channel changed: free the entry.
                c.inst = None;
                c.ringing = false;
                c.quiet = 0;
                c.dsp.clear();
                continue;
            };
            if c.buf.len() < n {
                continue;
            }
            let buf = &mut c.buf[..n];
            let heard = buf.iter().any(|s| *s != 0.0);
            if !heard && !c.ringing {
                continue;
            }
            c.dsp.process(fx, buf);
            c.quiet = if heard { 0 } else { c.quiet + frames as u64 };
            // Echoes and reverb keep coming for a while after the last input
            // (silent gaps included); then it rings until the output is inaudible.
            let tail = [
                (fx.delay > 0.0, DELAY_TAIL_SECONDS),
                (fx.reverb > 0.0, REVERB_TAIL_SECONDS),
                (fx.chorus > 0.0, 0.05),
            ]
            .iter()
            .filter(|t| t.0)
            .fold(0.0, |a: f64, t| a.max(t.1));
            c.ringing = (c.quiet as f64) < tail * self.rate || buf.iter().any(|s| s.abs() > 1e-5);
            if !c.ringing {
                c.dsp.clear();
            }
            let bus = (inst.map_or(0, |i| i.mixer as usize)).min(self.buses.len() - 1);
            for (d, s) in self.buses[bus][..n].iter_mut().zip(buf.iter()) {
                *d += *s;
            }
        }
    }

    /// A synth channel's effect state built on the UI thread. Gives back the
    /// state it replaces (or the new one when no room is left).
    pub fn adopt_synth_fx(
        &mut self,
        inst: u64,
        slot: usize,
        state: crate::fx::FxState,
    ) -> Option<crate::fx::FxState> {
        let n = match self.synth_fx.iter().position(|c| c.inst == Some(inst)) {
            Some(n) => n,
            None => {
                let Some(n) = self.synth_fx.iter().position(|c| c.inst.is_none()) else {
                    return Some(state);
                };
                self.synth_fx[n].inst = Some(inst);
                self.synth_fx[n].states.clear();
                n
            }
        };
        let c = &mut self.synth_fx[n];
        if slot >= c.states.capacity() {
            return Some(state);
        }
        if c.states.len() <= slot {
            c.states.resize_with(slot + 1, || None);
        }
        c.states[slot].replace(state)
    }

    /// Due note-offs, then one process call per plugin instrument; its output
    /// goes into its channel's insert with the channel's volume and pan.
    /// Effect plugins keep their events for `mix`.
    fn run_plugins(&mut self, song: &Song, frames: usize, block_clock: u64) {
        if self.plugins.is_empty() {
            return;
        }
        let end = block_clock + frames as u64;
        let n = frames * 2;
        // PDC: every channel lines up with the slowest plugin instrument.
        // The buses hold only the built-in instruments so far: delay them.
        let lat =
            |pl: &crate::plugin::Instance| pl.proc.lock().map(|p| p.latency(frames)).unwrap_or(0);
        self.src_latency = self
            .plugins
            .iter()
            .filter(|pl| {
                song.instruments
                    .iter()
                    .any(|i| i.plugin().is_some_and(|p| p.id == pl.id))
            })
            .map(lat)
            .max()
            .unwrap_or(0)
            .min(crate::plugin::MAX_PDC - 1);
        let tracks = song.mixer.len().max(1).min(self.buses.len());
        for (b, d) in self.buses[..tracks].iter_mut().zip(&mut self.bus_pdc) {
            d.run(&mut b[..n], self.src_latency);
        }
        for k in 0..self.plugins.len() {
            let pl = &mut self.plugins[k];
            pl.flush_fl_pitch(block_clock, end, (self.rate as u64 / 240).max(1));
            let mut i = 0;
            while i < pl.notes.len() {
                let (_, pitch, off) = pl.notes[i];
                if off < end {
                    let at = off.saturating_sub(block_clock).min(frames as u64 - 1) as u32;
                    pl.notes.swap_remove(i);
                    pl.push(crate::plugin::PluginEvent::NoteOff { at, pitch });
                } else {
                    i += 1;
                }
            }
            pl.events.sort_by_key(|e| e.at());
            let Some(inst) = song
                .instruments
                .iter()
                .find(|i| i.plugin().is_some_and(|p| p.id == pl.id))
            else {
                continue;
            };
            let io = &mut self.scratch[..n];
            io.fill(0.0);
            let mut wrote = 0;
            let mut lag = 0;
            if let Ok(mut proc) = pl.proc.lock() {
                lag = proc.latency(frames);
                let pairs = proc.extra_pairs().min(crate::plugin::MAX_EXTRA_PAIRS);
                if pairs == 0 {
                    proc.process(&pl.events, io, &self.plugin_transport);
                } else {
                    if self.extra.len() < pairs * n {
                        // Once per larger plugin or block size (like `scratch`).
                        self.extra.resize(pairs * n, 0.0);
                    }
                    self.extra[..pairs * n].fill(0.0);
                    wrote = proc
                        .process_multi(
                            &pl.events,
                            io,
                            &mut self.extra[..pairs * n],
                            n,
                            &self.plugin_transport,
                        )
                        .min(pairs);
                }
            }
            pl.events.clear();
            pl.level = io.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            let wait = self.src_latency.saturating_sub(lag);
            pl.pdc.run(io, wait);
            for (k, d) in pl.pdc_extra.iter_mut().enumerate().take(wrote) {
                d.run(&mut self.extra[k * n..(k + 1) * n], wait);
            }
            let bus = (inst.mixer as usize).min(self.buses.len() - 1);
            let target = if inst.muted {
                [0.0; 2]
            } else {
                channel_gains(&self.auto, &self.live_params, inst).unwrap_or_else(|| inst.gains())
            };
            // A fresh instance starts at the channel's gains, not gliding to them.
            let mut g = pl.gain_state.unwrap_or(target);
            // Extra outputs go to their mixer tracks (FL: the plugin's output
            // routing); an unrouted one joins the main output.
            let outs = inst.plugin().map(|p| p.outs.as_slice()).unwrap_or(&[]);
            if g == target {
                let add = |d: &mut [f32], s: &[f32]| {
                    for (d, s) in d.as_chunks_mut::<2>().0.iter_mut().zip(s.chunks_exact(2)) {
                        d[0] += (s[0] as f64 * g[0]) as f32;
                        d[1] += (s[1] as f64 * g[1]) as f32;
                    }
                };
                add(&mut self.buses[bus][..n], io);
                for k in 0..wrote {
                    let to = outs
                        .get(k)
                        .map(|&t| t as usize)
                        .filter(|&t| t > 0 && t < self.buses.len())
                        .unwrap_or(bus);
                    add(&mut self.buses[to][..n], &self.extra[k * n..(k + 1) * n]);
                }
            } else {
                // Smooth the channel gain per sample, advancing the curve once
                // for the main output and every extra pair alike.
                let alpha = smooth_alpha(self.rate);
                let mut dest = [bus; crate::plugin::MAX_EXTRA_PAIRS];
                for (k, d) in dest.iter_mut().enumerate().take(wrote) {
                    *d = outs
                        .get(k)
                        .map(|&t| t as usize)
                        .filter(|&t| t > 0 && t < self.buses.len())
                        .unwrap_or(bus);
                }
                for i in 0..frames {
                    g[0] += (target[0] - g[0]) * alpha;
                    g[1] += (target[1] - g[1]) * alpha;
                    if (target[0] - g[0]).abs() < 1e-6 {
                        g[0] = target[0];
                    }
                    if (target[1] - g[1]).abs() < 1e-6 {
                        g[1] = target[1];
                    }
                    let f = &mut self.buses[bus][i * 2..i * 2 + 2];
                    f[0] += (io[i * 2] as f64 * g[0]) as f32;
                    f[1] += (io[i * 2 + 1] as f64 * g[1]) as f32;
                    for k in 0..wrote {
                        let s = &self.extra[k * n + i * 2..k * n + i * 2 + 2];
                        let f = &mut self.buses[dest[k]][i * 2..i * 2 + 2];
                        f[0] += (s[0] as f64 * g[0]) as f32;
                        f[1] += (s[1] as f64 * g[1]) as f32;
                    }
                }
            }
            pl.gain_state = Some(g);
        }
    }

    /// Inserts in order (each feeds Master and later inserts), then Master into `out`.
    fn mix(&mut self, song: &Song, frames: usize, out: &mut [f32]) {
        let n = frames * 2;
        let rate = self.rate as f32;
        let tempo = song.tempo as f32;
        let count = self.buses.len().min(song.mixer.len().max(1));
        self.reduction.fill(0.0);
        // Each insert's hosted-effect latency (PDC), and the slowest chain.
        self.fx_latency_max = 0;
        for t in 0..count {
            let mut sum = 0;
            for slot in song.mixer[t].fx.iter().filter(|s| s.enabled) {
                if let Some(pref) = &slot.plugin
                    && let Some(pl) = self.plugins.iter().find(|p| p.id == pref.id)
                {
                    sum += pl.proc.lock().map(|p| p.latency(frames)).unwrap_or(0);
                }
            }
            self.fx_latency[t] = sum.min(crate::plugin::MAX_PDC - 1);
            if t > 0 {
                self.fx_latency_max = self.fx_latency_max.max(self.fx_latency[t]);
            }
        }
        // Inserts in routing order: each after every insert that sends to it
        // (ties in index order, so forward-only songs mix exactly as before),
        // Master last. `validate` rejects loops; a loop left anyway falls back
        // to index order for the rest.
        self.mix_order.clear();
        let done = &mut self.mix_done[..count];
        let feeds = &mut self.mix_feeds[..count];
        done.fill(false);
        feeds.fill(0);
        // A sidechain key (FL) must be mixed before the insert listening to it.
        let key_of = |c: usize, f: &crate::fx::FxSlot| {
            f.sidechain
                .map(|k| k as usize)
                .filter(|&k| k > 0 && k < count && k != c)
        };
        let mut keyed = false;
        for (c, m) in song.mixer[..count].iter().enumerate().skip(1) {
            for send in &m.sends {
                if let Some(f) = feeds.get_mut(send.to as usize) {
                    *f += 1;
                }
            }
            for _ in m.fx.iter().filter_map(|f| key_of(c, f)) {
                feeds[c] += 1;
                keyed = true;
            }
        }
        for _ in 1..count {
            let ready = (1..count).find(|&c| !done[c] && feeds[c] == 0);
            let pick = ready
                .or_else(|| (1..count).find(|&c| !done[c]))
                .unwrap_or(1);
            done[pick] = true;
            for send in &song.mixer[pick].sends {
                if let Some(f) = feeds.get_mut(send.to as usize) {
                    *f = f.saturating_sub(1);
                }
            }
            if keyed {
                for c in 1..count {
                    for _ in song.mixer[c]
                        .fx
                        .iter()
                        .filter(|f| key_of(c, f) == Some(pick))
                    {
                        feeds[c] = feeds[c].saturating_sub(1);
                    }
                }
            }
            self.mix_order.push(pick);
        }
        self.mix_order.push(0);
        for oi in 0..self.mix_order.len() {
            let t = self.mix_order[oi];
            let Some(track) = song.mixer.get(t) else {
                continue;
            };
            let bus = &mut self.buses[t][..n];
            if track.muted {
                bus.fill(0.0);
                self.peaks[t] = 0.0;
                // A muted armed track records silence: the take keeps its timing.
                for tap in self.taps.iter_mut().filter(|tap| tap.track == t) {
                    for _ in 0..frames {
                        if tap.tx.push([0.0; 2]).is_err() {
                            tap.dropped += 1;
                        }
                    }
                }
                continue;
            }
            // Effect chain.
            let slots = &mut self.fx[t];
            if slots.len() < track.fx.len() {
                // Within the reserved capacity on the live engine: no allocation.
                slots.resize_with(track.fx.len(), || None);
            }
            for (k, slot) in track.fx.iter().enumerate() {
                let si = t * crate::sequence::FX_SLOTS + k;
                if !slot.enabled {
                    // A bypassed slot forgets its smoothers: re-enabling snaps.
                    reset_fx_smoothers(
                        &mut self.fx_mix_state,
                        &mut self.fx_mix_plugin,
                        &mut self.fx_param_state,
                        si,
                    );
                    continue;
                }
                let mix_target = override_of(
                    &self.auto,
                    &self.live_params,
                    crate::automation::Target::FxMix {
                        track: t as u16,
                        slot: k as u8,
                    },
                )
                .unwrap_or(slot.mix);
                // A fresh slot starts at its mix, not gliding to it.
                let mut mix = self
                    .fx_mix_state
                    .get(si)
                    .copied()
                    .flatten()
                    .unwrap_or(mix_target);
                // This insert's signal, and the sidechain key's (mixed earlier).
                let (bus, key) = bus_and_key(
                    &mut self.buses,
                    t,
                    slot.sidechain
                        .map(|k| k as usize)
                        .filter(|&k| k > 0 && k < count),
                    n,
                );
                if let Some(pref) = &slot.plugin {
                    // Hosted effect: the instance runs in place, with the slot mix.
                    if self.fx_mix_plugin.get(si).copied().flatten() != Some(pref.id) {
                        // A different plugin in the slot: start the smoothers fresh.
                        reset_fx_smoothers(
                            &mut self.fx_mix_state,
                            &mut self.fx_mix_plugin,
                            &mut self.fx_param_state,
                            si,
                        );
                        if let Some(p) = self.fx_mix_plugin.get_mut(si) {
                            *p = Some(pref.id);
                        }
                        mix = mix_target;
                    }
                    let Some(pl) = self.plugins.iter_mut().find(|p| p.id == pref.id) else {
                        continue;
                    };
                    let blending = mix < 1.0 || mix != mix_target;
                    let dry = &mut self.scratch[..n];
                    if blending {
                        dry.copy_from_slice(bus);
                    }
                    let mut lag = 0;
                    if let Ok(mut proc) = pl.proc.lock() {
                        lag = proc.latency(frames);
                        match key {
                            Some(key) => {
                                proc.process_keyed(&pl.events, bus, key, &self.plugin_transport)
                            }
                            None => proc.process(&pl.events, bus, &self.plugin_transport),
                        }
                    }
                    pl.events.clear();
                    if blending {
                        // The dry signal waits for the wet one (no comb filter).
                        pl.pdc.run(dry, lag);
                        if mix != mix_target {
                            // f64 travel (an f32 curve stalls short of the
                            // snap threshold), once per frame for both sides.
                            let (mut cur, to) = (mix as f64, mix_target as f64);
                            let alpha = smooth_alpha(rate as f64);
                            for (w, d) in bus
                                .as_chunks_mut::<2>()
                                .0
                                .iter_mut()
                                .zip(dry.chunks_exact(2))
                            {
                                cur += (to - cur) * alpha;
                                if (to - cur).abs() < 1e-6 {
                                    cur = to;
                                }
                                let m = cur as f32;
                                w[0] = d[0] + (w[0] - d[0]) * m;
                                w[1] = d[1] + (w[1] - d[1]) * m;
                            }
                            mix = cur as f32;
                        } else {
                            for (w, d) in bus.iter_mut().zip(dry.iter()) {
                                *w = *d + (*w - *d) * mix;
                            }
                        }
                    }
                    if let Some(m) = self.fx_mix_state.get_mut(si) {
                        *m = Some(mix);
                    }
                    continue;
                }
                if self.fx_mix_plugin.get(si).copied().flatten().is_some() {
                    // The slot was a hosted plugin: start the smoothers fresh.
                    reset_fx_smoothers(
                        &mut self.fx_mix_state,
                        &mut self.fx_mix_plugin,
                        &mut self.fx_param_state,
                        si,
                    );
                    mix = mix_target;
                }
                if slots[k].as_ref().is_none_or(|s| s.kind() != slot.kind) {
                    // Offline renders build here; the live engine gets states
                    // built on the UI thread (`adopt_fx`) before the song arrives.
                    slots[k] = Some(crate::fx::FxState::for_slot(slot, rate));
                    reset_fx_smoothers(
                        &mut self.fx_mix_state,
                        &mut self.fx_mix_plugin,
                        &mut self.fx_param_state,
                        si,
                    );
                    mix = mix_target;
                }
                let st = slots[k].as_mut().expect("built");
                // Overridden parameters: a copy with the live values in it, then
                // the envelope values on top (automation wins).
                let mut params: &[f32] = &slot.params;
                let is_slot = |target: crate::automation::Target| matches!(target, crate::automation::Target::FxParam { track: at, slot: s, .. } if at as usize == t && s as usize == k);
                let automated = self.auto.iter().any(|sample| is_slot(sample.target))
                    || self.live_params.iter().any(|sample| is_slot(sample.target));
                if automated {
                    self.auto_params.clear();
                    self.auto_params
                        .extend(slot.params.iter().take(self.auto_params.capacity()));
                    let live = self.live_params.iter().map(|p| (p.target, p.value));
                    let auto = self.auto.iter().map(|p| (p.target, p.value));
                    for (target, value) in live.chain(auto) {
                        if let crate::automation::Target::FxParam {
                            track: at,
                            slot: s,
                            param,
                        } = target
                            && at as usize == t
                            && s as usize == k
                            && let Some(p) = self.auto_params.get_mut(param as usize)
                        {
                            *p = value;
                        }
                    }
                    params = &self.auto_params;
                }
                // Continuous parameters glide ~5 ms toward the target, once per
                // block; discrete ones (Steps, fixed ranges) snap. While nothing
                // is automated and the state matches the slot, `slot.params`
                // passes straight through.
                if let Some(Some((kind, vals))) = self.fx_param_state.get_mut(si) {
                    if *kind != slot.kind || (!vals.is_empty() && vals.len() != slot.params.len()) {
                        vals.clear();
                        *kind = slot.kind;
                    }
                    if automated || (!vals.is_empty() && vals.as_slice() != slot.params) {
                        if vals.is_empty() {
                            // First smoothed block: start at the target, no lag.
                            vals.extend_from_slice(params);
                        } else {
                            let alpha_block =
                                1.0 - (-(frames as f32) / (SMOOTH_TAU_SEC as f32 * rate)).exp();
                            let specs = slot.kind.params();
                            for (i, cur) in vals.iter_mut().enumerate() {
                                let Some(&target) = params.get(i) else { break };
                                match specs.get(i) {
                                    Some(spec)
                                        if spec.unit != crate::fx::Unit::Steps
                                            && spec.max > spec.min =>
                                    {
                                        *cur += (target - *cur) * alpha_block;
                                        if (target - *cur).abs()
                                            <= 1e-5 * (spec.max - spec.min).max(1.0)
                                        {
                                            *cur = target;
                                        }
                                    }
                                    _ => *cur = target,
                                }
                            }
                            params = vals.as_slice();
                        }
                    }
                }
                let dry = &mut self.scratch[..n];
                let blending = mix < 1.0 || mix != mix_target;
                if blending {
                    dry.copy_from_slice(bus);
                }
                match key {
                    Some(key) => st.process_keyed(params, bus, key, rate, tempo),
                    None => st.process(params, bus, rate, tempo),
                }
                if blending {
                    if mix != mix_target {
                        // f64 travel (an f32 curve stalls short of the snap
                        // threshold), once per frame for both sides.
                        let (mut cur, to) = (mix as f64, mix_target as f64);
                        let alpha = smooth_alpha(rate as f64);
                        for (w, d) in bus
                            .as_chunks_mut::<2>()
                            .0
                            .iter_mut()
                            .zip(dry.chunks_exact(2))
                        {
                            cur += (to - cur) * alpha;
                            if (to - cur).abs() < 1e-6 {
                                cur = to;
                            }
                            let m = cur as f32;
                            w[0] = d[0] + (w[0] - d[0]) * m;
                            w[1] = d[1] + (w[1] - d[1]) * m;
                        }
                        mix = cur as f32;
                    } else {
                        for (w, d) in bus.iter_mut().zip(dry.iter()) {
                            *w = *d + (*w - *d) * mix;
                        }
                    }
                }
                if let Some(m) = self.fx_mix_state.get_mut(si) {
                    *m = Some(mix);
                }
                if let Some(g) = self.reduction.get_mut(t * crate::sequence::FX_SLOTS + k) {
                    *g = st.reduction();
                }
            }
            let bus = &mut self.buses[t][..n];
            // PDC: inserts with faster effect chains wait for the slowest
            // (Master is shared by all, so it needs none).
            if t > 0 {
                let d = self.fx_latency_max.saturating_sub(self.fx_latency[t]);
                self.track_pdc[t].run(bus, d);
            }
            // Built-in EQ, skipped while flat.
            if track.eq != [0.0; 3] {
                let st = self.eq[t]
                    .get_or_insert_with(|| crate::fx::FxState::new(crate::fx::FxKind::Eq, rate));
                st.process(&track.eq_params(), bus, rate, tempo);
            }
            // Stereo separation (FL: the SEP knob), skipped at 0.
            if track.sep != 0.0 {
                let width = 1.0 + track.sep;
                for f in bus.as_chunks_mut::<2>().0 {
                    let (m, s) = ((f[0] + f[1]) * 0.5, (f[0] - f[1]) * 0.5 * width);
                    f[0] = m + s;
                    f[1] = m - s;
                }
            }
            // Fader and pan, smoothed per sample (~5 ms); skipped at unity.
            let target = mixer_gains(&self.auto, &self.live_params, t, track);
            // A fresh engine starts at the fader, not at unity.
            let start = self.gain_state[t].unwrap_or(target);
            let mut end = start;
            if start != [1.0; 2] || target != [1.0; 2] {
                if start == target {
                    for f in bus.as_chunks_mut::<2>().0 {
                        f[0] *= start[0];
                        f[1] *= start[1];
                    }
                } else {
                    // f64 travel: an f32 curve stalls a few ulps short of the
                    // target and never reaches the snap threshold.
                    let mut cur = [start[0] as f64, start[1] as f64];
                    let to = [target[0] as f64, target[1] as f64];
                    let alpha = smooth_alpha(self.rate);
                    for f in bus.as_chunks_mut::<2>().0 {
                        cur[0] += (to[0] - cur[0]) * alpha;
                        cur[1] += (to[1] - cur[1]) * alpha;
                        if (to[0] - cur[0]).abs() < 1e-6 {
                            cur[0] = to[0];
                        }
                        if (to[1] - cur[1]).abs() < 1e-6 {
                            cur[1] = to[1];
                        }
                        f[0] *= cur[0] as f32;
                        f[1] *= cur[1] as f32;
                    }
                    end = [cur[0] as f32, cur[1] as f32];
                }
            }
            self.gain_state[t] = Some(end);
            self.peaks[t] = bus.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            if self.scope.track.load(std::sync::atomic::Ordering::Relaxed) == t {
                use std::sync::atomic::Ordering::Relaxed;
                let mut at = self.scope.pos.load(Relaxed);
                for f in bus.as_chunks::<2>().0 {
                    self.scope.ring[at].store(((f[0] + f[1]) * 0.5).to_bits(), Relaxed);
                    at = (at + 1) % SCOPE_LEN;
                }
                self.scope.pos.store(at, Relaxed);
            }
            if t == 0
                && self
                    .master_scope
                    .track
                    .load(std::sync::atomic::Ordering::Relaxed)
                    == 0
            {
                use std::sync::atomic::Ordering::Relaxed;
                let mut at = self.master_scope.pos.load(Relaxed);
                for f in bus.as_chunks::<2>().0 {
                    self.master_scope.ring[at].store(((f[0] + f[1]) * 0.5).to_bits(), Relaxed);
                    at = (at + 1) % SCOPE_LEN;
                }
                self.master_scope.pos.store(at, Relaxed);
            }
            for tap in self.taps.iter_mut().filter(|tap| tap.track == t) {
                for f in bus.as_chunks::<2>().0 {
                    if tap.tx.push([f[0], f[1]]).is_err() {
                        tap.dropped += 1;
                    }
                }
            }
            if t == 0 {
                break;
            }
            // Route: post-fader into Master and sends.
            let (lo, hi) = self.buses.split_at_mut(t);
            let src = &hi[0][..n];
            if track.to_master {
                for (d, s) in lo[0][..n].iter_mut().zip(src) {
                    *d += *s;
                }
            }
            for send in &track.sends {
                let to = send.to as usize;
                if to > t && to < count {
                    let (a, b) = hi.split_at_mut(to - t);
                    for (d, s) in b[0][..n].iter_mut().zip(&a[0][..n]) {
                        *d += *s * send.level;
                    }
                } else if to > 0 && to < t {
                    // A send back to an earlier insert (it mixes after this one).
                    for (d, s) in lo[to][..n].iter_mut().zip(&hi[0][..n]) {
                        *d += *s * send.level;
                    }
                }
            }
        }
        out[..n].copy_from_slice(&self.buses[0][..n]);
    }

    fn sps(song: &Song) -> f64 {
        seconds_per_step(song.tempo)
    }

    /// Seconds per step now: the map's tempo at the playhead, else the song's.
    fn sps_now(&self, song: &Song) -> f64 {
        if self.tempo.is_some() {
            self.sps_block
        } else {
            Self::sps(song)
        }
    }

    /// Keep the tempo table in step with the song's map (song mode only).
    fn sync_tempo(&mut self, song: &Song, song_mode: bool) {
        if !song_mode || song.tempo_map.is_empty() {
            self.tempo = None;
            return;
        }
        let bits = song.tempo.to_bits();
        if self.tempo.is_none()
            || self.tempo_key.1 != bits
            || self.tempo_key.0 != song.tempo_map.points
        {
            self.tempo_key = (song.tempo_map.points.clone(), bits);
            self.tempo = Some(crate::tempo::TempoTable::build(
                &song.tempo_map,
                song.tempo,
                self.rate as u32,
            ));
        }
    }

    /// Frames from the start of a pattern to tick `t`, swing included.
    fn note_frame(&self, song: &Song, t: Tick) -> u64 {
        let sps = Self::sps(song);
        ((steps(t) * sps + swing_offset(t, sps, song.swing.clamp(0.0, 0.6))) * self.rate).ceil()
            as u64
    }

    /// Frames from the song start to playlist tick `t`. Whole bars count in
    /// truncated bar frames, as format 1 did, so old songs keep their timing.
    fn clip_frame(&self, song: &Song, t: Tick) -> u64 {
        if let Some(table) = &self.tempo {
            return table.frames_at_ceil(t);
        }
        let bar = (16.0 * Self::sps(song) * self.rate) as u64;
        let rest = t.rem_euclid(BAR);
        (t / BAR) as u64 * bar
            + if rest == 0 {
                0
            } else {
                (steps(rest) * Self::sps(song) * self.rate).ceil() as u64
            }
    }

    /// Playlist tick at song frame `f` (the inverse of `clip_frame`).
    fn song_tick(&self, song: &Song, f: u64) -> Tick {
        if let Some(table) = &self.tempo {
            return table.tick_at_frame(f);
        }
        let bar = ((16.0 * Self::sps(song) * self.rate) as u64).max(1);
        let rest = (f % bar) as f64 / (Self::sps(song) * self.rate) * crate::sequence::STEP as f64;
        (f / bar) as Tick * BAR + rest as Tick
    }

    fn collect_hits(&mut self, song: &Song, pattern: u64, song_mode: bool, from: u64, to: u64) {
        self.hits.clear();
        // `window`: the pattern ticks this pass plays. A clip may cut its
        // pattern, or run longer than it: then the pattern repeats (FL).
        // `start`: the clip's playlist tick (song mode), for the tempo map.
        let vocal = self.vocal.load();
        let add = |hits: &mut Vec<Hit>,
                   offset: u64,
                   start: Tick,
                   pi: usize,
                   p: &Pattern,
                   e: &Engine,
                   window: (Tick, Tick)| {
            let plen = p.length.max(1);
            let (first, last) = (window.0.div_euclid(plen), (window.1 - 1).div_euclid(plen));
            for (ni, n) in p.notes.iter().enumerate() {
                if n.muted || song.instrument(n.instrument).is_none_or(|i| i.muted) {
                    continue;
                }
                // A vocal phrase starts at its first note, early by the syllable's preutterance;
                // the notes after it are inside the phrase's audio.
                let mut lead = 0usize;
                if matches!(
                    song.instrument(n.instrument).map(|i| &i.kind),
                    Some(InstrumentKind::Vocal { .. })
                ) {
                    if vocal.members.contains(&n.id) {
                        continue;
                    }
                    match vocal.starts.get(&n.id) {
                        Some(ph) => lead = ph.lead,
                        None => continue,
                    }
                }
                for k in first..=last {
                    let t = n.start + k * plen;
                    if t < window.0 || t >= window.1 {
                        continue;
                    }
                    let local = t - window.0;
                    let at = match &e.tempo {
                        // With tempo changes a clip's notes follow the map, not the clip's first tempo.
                        Some(table) => {
                            let sps = seconds_per_step(table.bpm_at(start + local));
                            (table.frames_at(start + local)
                                + swing_offset(local, sps, song.swing.clamp(0.0, 0.6)) * e.rate)
                                .ceil() as u64
                        }
                        None => offset + e.note_frame(song, local),
                    };
                    let (at, skip) = (
                        at.saturating_sub(lead as u64),
                        (lead as u64).saturating_sub(at) as usize,
                    );
                    if (from..to).contains(&at) {
                        hits.push(Hit {
                            at,
                            tick: start + local,
                            pattern: pi,
                            note: ni,
                            skip,
                        });
                    }
                }
            }
        };
        let mut hits = std::mem::take(&mut self.hits);
        if song_mode {
            for c in song
                .clips
                .iter()
                .filter(|c| !c.muted && !song.track_muted(c.track))
            {
                if let Some(pi) = song.patterns.iter().position(|p| p.id == c.pattern) {
                    let p = &song.patterns[pi];
                    let offset = self.clip_frame(song, c.start);
                    let window = (c.offset, c.offset + song.clip_len(c));
                    let span = self.clip_frame(song, window.1 - window.0);
                    if offset < to && offset + span > from {
                        add(&mut hits, offset, c.start, pi, p, self, window);
                    }
                }
            }
        } else if let Some(pi) = song.patterns.iter().position(|p| p.id == pattern) {
            add(
                &mut hits,
                0,
                0,
                pi,
                &song.patterns[pi],
                self,
                (0, song.patterns[pi].length),
            );
        }
        hits.sort_by_key(|h| h.at);
        self.hits = hits;
    }

    fn push(&mut self, v: Voice) {
        if self.voices.len() >= MAX_VOICES {
            // Steal the oldest voice outright; the pool is full.
            if let Some(i) = (0..self.voices.len())
                .max_by(|a, b| self.voices[*a].t.total_cmp(&self.voices[*b].t))
            {
                self.voices.swap_remove(i);
            }
        }
        self.voices.push(v);
    }

    fn trigger(&mut self, song: &Song, clock: f64, hit: Hit) {
        self.next_note += 1;
        let note_id = self.next_note;
        let p = &song.patterns[hit.pattern];
        let n = &p.notes[hit.note];
        self.hit_skip = hit.skip;
        self.start_note(song, clock, note_id, n, Some((p, hit.tick)));
        self.hit_skip = 0;
    }

    /// A note played from the keyboard or the piano roll (FL: preview while
    /// held). It sounds through its channel and mixer insert until `live_off`.
    pub fn live_on(&mut self, song: &Song, id: u64, inst: u64, pitch: u8, velocity: u8) {
        self.live_on_ch(song, id, inst, pitch, velocity, 0);
    }

    /// A live note from MIDI channel `ch` (0..15): MPE gives each its own controllers.
    pub fn live_on_ch(&mut self, song: &Song, id: u64, inst: u64, pitch: u8, velocity: u8, ch: u8) {
        let n = Note::new(inst, 0, Tick::MAX / 1024, pitch, velocity.max(1));
        self.start_note(song, 0.0, LIVE | id, &n, None);
        let ctl = self.ctl.iter().find(|c| c.inst == inst).copied();
        let patch = song.instrument(inst).and_then(|i| match &i.kind {
            InstrumentKind::Synth { patch } => Some(patch),
            _ => None,
        });
        for v in self.voices.iter_mut().filter(|v| v.note == LIVE | id) {
            v.ch = ch & 15;
            // A note starts at the wheel's pitch, not gliding to it.
            if let (Some(c), Some(p)) = (ctl, patch) {
                let (b, pr, sl) = c.for_note(v.ch, p);
                (v.bend_to, v.bend_now, v.pressure, v.cc74, v.wheel) = (b, b, pr, sl, c.wheel);
            }
        }
    }

    /// Release a live note: its envelope goes to release.
    pub fn live_off(&mut self, id: u64) {
        for v in self.voices.iter_mut().filter(|v| v.note == LIVE | id) {
            v.gate = v.gate.min(v.t);
            v.key_until = v.key_until.min(v.t);
        }
        for pv in self.phrases.iter_mut().filter(|pv| pv.live == LIVE | id) {
            // The key is up: the note dies away over ~60 ms.
            pv.fade.get_or_insert((2880, 2880));
        }
        for pl in &mut self.plugins {
            for n in pl.notes.iter_mut().filter(|n| n.0 == LIVE | id) {
                // Released at the start of the next block.
                n.2 = 0;
            }
        }
    }

    /// Render only what sounds (live notes, ringing tails) with the transport stopped.
    pub fn process_live(&mut self, song: &Song, out: &mut [f32]) {
        crate::denormal::flush_to_zero();
        out.fill(0.0);
        self.auto.clear();
        let frames = out.len() / 2;
        self.prepare_buses(song.mixer.len().max(1), frames);
        self.prepare_sampler_fx(song, frames);
        self.render_voices(song, 0, frames);
        self.render_phrases(song, 0, frames);
        self.run_synth_fx(song, frames);
        self.run_sampler_fx(song, frames);
        self.plugin_transport = crate::plugin::Transport {
            rate: self.rate as u32,
            tempo: song.tempo,
            playing: false,
            frame: 0,
        };
        let clock = self.clock;
        self.run_plugins(song, frames, clock);
        self.mix(song, frames, out);
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }

    fn start_note(
        &mut self,
        song: &Song,
        clock: f64,
        note_id: u64,
        n: &Note,
        score: Option<(&Pattern, Tick)>,
    ) {
        let Some(instrument) = song.instrument(n.instrument) else {
            return;
        };
        let p = score.map(|(p, _)| p);
        match &instrument.kind {
            InstrumentKind::Plugin { plugin } => {
                let is_native = plugin.format == "fl-native";
                let live = note_id & LIVE != 0;
                let on = if live { self.clock } else { self.hit_clock };
                let sps = self.sps_now(song);
                let relative_frame = |delta| {
                    if let (Some(table), Some((_, tick))) = (&self.tempo, score) {
                        (table.frames_at(tick + delta) - table.frames_at(tick)).max(0.0)
                    } else {
                        steps(delta) * sps * self.rate
                    }
                };
                let off = if note_id & LIVE != 0 {
                    u64::MAX
                } else {
                    let frames = if is_native {
                        relative_frame(n.length)
                    } else {
                        steps(n.length) * sps * self.rate
                    };
                    on + (frames as u64).max(1)
                };
                let at = self.cursor as u32;
                let Some(pl) = self.plugins.iter_mut().find(|x| x.id == plugin.id) else {
                    return;
                };
                // The same key again: end the old note first (plugins pair on/off by key).
                if let Some(i) = pl.notes.iter().position(|x| x.1 == n.pitch) {
                    pl.notes.swap_remove(i);
                    pl.push(crate::plugin::PluginEvent::NoteOff { at, pitch: n.pitch });
                }
                if pl.notes.len() < crate::plugin::MAX_NOTES {
                    if is_native {
                        let slides = p.into_iter().flat_map(|p| {
                            p.slides.iter().filter_map(move |slide| {
                                if slide.instrument != n.instrument
                                    || slide.group != n.group
                                    || !(n.start..n.end()).contains(&slide.start)
                                {
                                    return None;
                                }
                                let highest = p
                                    .notes
                                    .iter()
                                    .filter(|other| {
                                        other.instrument == n.instrument
                                            && other.group == n.group
                                            && !other.muted
                                            && other.start <= slide.start
                                            && other.end() > slide.start
                                    })
                                    .map(|other| other.pitch)
                                    .max()
                                    .unwrap_or(n.pitch);
                                let delta = slide.start - n.start;
                                let start = on + relative_frame(delta).round() as u64;
                                let end = on + relative_frame(delta + slide.length).round() as u64;
                                Some(crate::plugin::PitchSlide {
                                    start,
                                    end,
                                    cents: f64::from(i16::from(slide.target) - i16::from(highest))
                                        * 100.0,
                                })
                            })
                        });
                        pl.start_fl_pitch(
                            on.saturating_sub(u64::from(at)),
                            on,
                            off,
                            f64::from(n.fine),
                            (self.rate as u64 / 240).max(1),
                            slides,
                        );
                    }
                    pl.notes.push((note_id, n.pitch, off));
                    pl.push(crate::plugin::PluginEvent::NoteOn {
                        at,
                        pitch: n.pitch,
                        velocity: n.velocity.max(1),
                    });
                }
            }
            InstrumentKind::Vocal { .. } => {
                // Sequenced notes only: a phrase is rendered ahead from the score.
                let vm = self.vocal.load();
                if note_id & LIVE != 0 {
                    // A held key: the channel's default lyric on that pitch, from just before its consonant ends.
                    let want = self.live_lyric.load(std::sync::atomic::Ordering::Relaxed);
                    if let Some(ph) = vm
                        .previews
                        .get(&(n.instrument, n.pitch, want))
                        .or_else(|| vm.previews.get(&(n.instrument, n.pitch, 0)))
                        && self.phrases.len() < MAX_PHRASES
                    {
                        self.phrases.push(PhraseVoice {
                            inst: n.instrument,
                            phrase: ph.clone(),
                            live: note_id,
                            pos: 0,
                            fade: None,
                            gain_state: None,
                        });
                    }
                    return;
                }
                if trace_vocal() {
                    eprintln!(
                        "vocal: note {} pitch {} hit at frame {} (block {}) phrase {}",
                        n.id,
                        n.pitch,
                        self.hit_clock,
                        self.block_at,
                        vm.starts.contains_key(&n.id)
                    );
                }
                if let Some(ph) = vm.starts.get(&n.id)
                    && self.phrases.len() < MAX_PHRASES
                {
                    self.phrases.push(PhraseVoice {
                        inst: n.instrument,
                        phrase: ph.clone(),
                        live: 0,
                        pos: self.hit_skip,
                        fade: None,
                        gain_state: None,
                    });
                }
            }
            InstrumentKind::Forge { sound } => {
                let r = &sound.rendered;
                let mut v = Voice::new(
                    n.instrument,
                    note_id,
                    Source::Forge,
                    n.pitch,
                    n.velocity,
                    f64::INFINITY,
                );
                // A one-shot: the rack lights it while the sound lasts.
                v.key_until = r.seconds();
                v.pan = n.pan as f64;
                v.fine = n.fine as f64 / 100.0;
                v.slide = p
                    .and_then(|p| note_slide(p, n, self.sps_now(song), song.swing.clamp(0.0, 0.6)));
                self.push(v);
            }
            InstrumentKind::Drum { voice } => {
                let mut v = Voice::new(
                    n.instrument,
                    note_id,
                    Source::Drum {
                        row: *voice as usize,
                    },
                    n.pitch,
                    n.velocity,
                    0.25,
                );
                v.pan = n.pan as f64;
                self.push(v);
            }
            InstrumentKind::Synth { patch } => {
                let sps = self.sps_now(song);
                let mut v = Voice::new(
                    n.instrument,
                    note_id,
                    Source::Synth,
                    n.pitch,
                    n.velocity,
                    steps(n.length) * sps,
                );
                v.clock = clock;
                v.pan = n.pan as f64;
                v.fine = n.fine as f64 / 100.0;
                v.slide = p.and_then(|p| note_slide(p, n, sps, song.swing.clamp(0.0, 0.6)));
                if let Some(last) = self
                    .last_key
                    .iter()
                    .find(|(i, _)| *i == n.instrument)
                    .map(|x| x.1)
                    && (patch.glide > 0.0 || n.porta)
                {
                    v.glide_from = last as f64 - n.pitch as f64;
                    v.glide_time = if patch.glide > 0.0 { patch.glide } else { sps };
                }
                self.remember_key(n.instrument, n.pitch);
                self.push(v);
            }
            InstrumentKind::Sampler { preset } => {
                if preset.is_none() && instrument.fl_plugin.is_some() {
                    return;
                }
                let preset = preset.as_deref();
                let sps = self.sps_now(song);
                let gate = steps(n.length) * sps;
                let slide = p.and_then(|p| note_slide(p, n, sps, song.swing.clamp(0.0, 0.6)));
                let program = preset.map(|p| p.program.clone()).unwrap_or_default();
                // Play Mode and Glide (DirectWave Program tab).
                let held = self
                    .voices
                    .iter()
                    .any(|v| v.inst == n.instrument && v.held());
                let last = self
                    .last_key
                    .iter()
                    .find(|(i, _)| *i == n.instrument)
                    .map(|k| k.1);
                let mut glide_from = 0.0;
                if program.glide > 0.0
                    && let Some(last) = last
                    && (program.glide_mode == GlideMode::Fixed || held)
                {
                    glide_from = last as f64 - n.pitch as f64;
                }
                // A porta note glides in even when the channel has no glide.
                let mut glide_time = program.glide;
                if n.porta
                    && glide_from == 0.0
                    && let Some(last) = last
                {
                    glide_from = last as f64 - n.pitch as f64;
                    if glide_time <= 0.0 {
                        glide_time = sps;
                    }
                }
                match program.play_mode {
                    PlayMode::Legato if held => {
                        // Overlapping note: retarget the sounding voice, no retrigger.
                        for v in self
                            .voices
                            .iter_mut()
                            .filter(|v| v.inst == n.instrument && v.held())
                        {
                            v.glide_from = v.key as f64 + v.bend() - n.pitch as f64;
                            v.glide_time = program.glide;
                            v.glide_t0 = v.t;
                            v.key = n.pitch;
                            v.gate = v.t + gate;
                            v.key_until = v.gate;
                            v.slide = None;
                        }
                        self.remember_key(n.instrument, n.pitch);
                        return;
                    }
                    PlayMode::Mono | PlayMode::Legato => {
                        for v in self.voices.iter_mut().filter(|v| v.inst == n.instrument) {
                            v.kill_now();
                        }
                    }
                    PlayMode::Poly => loop {
                        // Steal the oldest notes past the polyphony limit. Fixed buffer: no allocation here.
                        let mut notes = [0u64; MAX_VOICES];
                        let mut count = 0;
                        for v in self
                            .voices
                            .iter()
                            .filter(|v| v.inst == n.instrument && v.kill.is_none())
                        {
                            if !notes[..count].contains(&v.note) {
                                notes[count] = v.note;
                                count += 1;
                            }
                        }
                        if count < program.polyphony as usize {
                            break;
                        }
                        let oldest = *notes[..count].iter().min().unwrap();
                        for v in self.voices.iter_mut().filter(|v| v.note == oldest) {
                            v.kill_now();
                        }
                    },
                }
                self.remember_key(n.instrument, n.pitch);
                let mut base = Voice::new(
                    n.instrument,
                    note_id,
                    Source::Tone,
                    n.pitch,
                    n.velocity,
                    gate,
                );
                base.group = n.group;
                base.pan = n.pan as f64;
                base.fine = n.fine as f64 / 100.0;
                base.slide = slide;
                base.glide_from = glide_from;
                base.glide_time = glide_time;
                base.clock = clock;
                base.rnd = {
                    let mut h = note_id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    h ^= h >> 29;
                    (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
                };
                let Some(preset) = preset else {
                    self.push(base);
                    return;
                };
                self.start_zones(preset, base);
            }
        }
    }

    fn start_zones(&mut self, preset: &SamplePreset, base: Voice) {
        // Trigger groups (round robin): one zone per group plays. Groups
        // already handled, as bits 0..16 of a stack word: no allocation.
        let mut done = 0u32;
        for (zi, z) in preset.zones.iter().enumerate() {
            if !z.matches(base.key, base.vel) {
                continue;
            }
            let g = z.params.trigger_group;
            let mode = z.params.trigger_mode;
            if g != 0 && mode != TriggerMode::Normal {
                let bit = 1u32 << (g.min(16) - 1);
                if done & bit != 0 {
                    continue;
                }
                done |= bit;
                // The first matching zone of the group sets its mode.
                let pick = self.pick_trigger_zone(preset, &base, g, mode);
                self.start_zone(preset, base, pick);
                continue;
            }
            self.start_zone(preset, base, zi);
        }
    }

    /// The zone that plays for trigger group `g` this note: among the zones
    /// matching the note in that group (not `Normal`), by cycling, a hash of
    /// the note, or a hash that skips the zone played last.
    fn pick_trigger_zone(
        &mut self,
        preset: &SamplePreset,
        base: &Voice,
        g: u8,
        mode: TriggerMode,
    ) -> usize {
        let in_group = |z: &Zone| {
            z.matches(base.key, base.vel)
                && z.params.trigger_group == g
                && z.params.trigger_mode != TriggerMode::Normal
        };
        let count = preset.zones.iter().filter(|z| in_group(z)).count().max(1);
        let nth = |k: usize, skip: Option<usize>| {
            preset
                .zones
                .iter()
                .enumerate()
                .filter(|(zi, z)| in_group(z) && Some(*zi) != skip)
                .nth(k)
                .map_or(0, |(zi, _)| zi)
        };
        let at = match self
            .trigger_state
            .iter()
            .position(|s| s.0 == base.inst && s.1 == g)
        {
            Some(at) => at,
            None if self.trigger_state.len() < self.trigger_state.capacity() => {
                self.trigger_state.push((base.inst, g, 0, u32::MAX));
                self.trigger_state.len() - 1
            }
            // Table full: no memory of earlier notes, still deterministic.
            None => return nth(base.note as usize % count, None),
        };
        let (_, _, counter, last) = self.trigger_state[at];
        let mut h = base.note ^ base.inst.rotate_left(17) ^ (g as u64).rotate_left(33);
        h = (h ^ (counter as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93))
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        let pick = match mode {
            TriggerMode::Cycle => nth(counter as usize % count, None),
            TriggerMode::Random => nth(h as usize % count, None),
            _ => {
                let last = (last != u32::MAX).then_some(last as usize);
                let others = if count > 1 && last.is_some_and(|l| in_group(&preset.zones[l])) {
                    count - 1
                } else {
                    count
                };
                let skip = if others < count { last } else { None };
                nth(h as usize % others, skip)
            }
        };
        let s = &mut self.trigger_state[at];
        s.2 = counter.wrapping_add(1);
        s.3 = pick as u32;
        pick
    }

    fn start_zone(&mut self, preset: &SamplePreset, base: Voice, zi: usize) {
        let z = &preset.zones[zi];
        let mut v = base;
        v.source = Source::Zone { zone: zi };
        v.cut_group = z.params.mute_group;
        if z.params.one_shot {
            v.gate = f64::INFINITY;
        }
        if z.params.reverse {
            v.pos = (z.end - 1) as f64;
            v.dir = -1.0;
        } else {
            v.pos = z.offset as f64;
        }
        if v.cut_group != 0 {
            for o in self
                .voices
                .iter_mut()
                .filter(|o| o.inst == v.inst && o.cut_group == v.cut_group)
            {
                o.kill_now();
            }
        }
        self.push(v);
    }

    fn remember_key(&mut self, inst: u64, key: u8) {
        let room = self.last_key.len() < self.last_key.capacity();
        match self.last_key.iter_mut().find(|(i, _)| *i == inst) {
            Some(slot) => slot.1 = key,
            None if room => self.last_key.push((inst, key)),
            None => {}
        }
    }

    /// Mix the sounding vocal phrases into their channels' buses.
    fn render_phrases(&mut self, song: &Song, at: usize, frames: usize) {
        if frames == 0 || self.phrases.is_empty() {
            return;
        }
        let auto = &self.auto;
        let live = &self.live_params;
        let alpha = smooth_alpha(self.rate);
        for pv in &mut self.phrases {
            let inst = song.instrument(pv.inst);
            let bus = inst
                .map(|i| i.mixer as usize)
                .filter(|b| *b < self.buses.len())
                .unwrap_or(0);
            let target = inst
                .map(|i| channel_gains(auto, live, i).unwrap_or_else(|| i.gains()))
                .unwrap_or([1.0, 1.0]);
            // A fresh phrase starts at the channel's gains, not gliding to them.
            let mut gain = pv.gain_state.unwrap_or(target);
            let out = &mut self.buses[bus][at * 2..(at + frames) * 2];
            let data = &pv.phrase.data;
            for frame in out.as_chunks_mut::<2>().0 {
                let Some(&s) = data.get(pv.pos) else { break };
                let mut s = s as f64;
                if let Some((total, left)) = &mut pv.fade {
                    s *= *left as f64 / *total as f64;
                    *left = left.saturating_sub(1);
                }
                if gain != target {
                    gain[0] += (target[0] - gain[0]) * alpha;
                    gain[1] += (target[1] - gain[1]) * alpha;
                    if (target[0] - gain[0]).abs() < 1e-6 {
                        gain[0] = target[0];
                    }
                    if (target[1] - gain[1]).abs() < 1e-6 {
                        gain[1] = target[1];
                    }
                }
                frame[0] += (s * gain[0]) as f32;
                frame[1] += (s * gain[1]) as f32;
                pv.pos += 1;
            }
            pv.gain_state = Some(gain);
        }
        self.phrases
            .retain(|pv| pv.pos < pv.phrase.data.len() && pv.fade.is_none_or(|(_, left)| left > 0));
    }

    /// Render the voices for `frames` frames at block offset `at`, each into
    /// the bus of its channel's mixer insert.
    fn render_voices(&mut self, song: &Song, at: usize, frames: usize) {
        if frames == 0 || self.voices.is_empty() {
            return;
        }
        let rate = self.rate;
        let dt = 1.0 / rate;
        let alpha = smooth_alpha(rate);
        let buses = &mut self.buses;
        let chans = &mut self.synth_fx;
        let sfx = &mut self.sampler_fx;
        let auto = &self.auto;
        let live = &self.live_params;
        let ctl = &self.ctl;
        for v in &mut self.voices {
            let inst = song.instrument(v.inst);
            let bus = inst
                .map(|i| i.mixer as usize)
                .filter(|b| *b < buses.len())
                .unwrap_or(0);
            let orig = inst.and_then(|i| match &i.kind {
                InstrumentKind::Synth { patch } => Some(patch),
                _ => None,
            });
            // A synth with its own effects plays into its channel buffer.
            let mut chan = None;
            if orig.is_some_and(|p| p.fx.iter().any(|s| s.enabled)) {
                let found = chans.iter().position(|c| c.inst == Some(v.inst));
                let n = found.or_else(|| chans.iter().position(|c| c.inst.is_none()));
                if let Some(n) = n {
                    if found.is_none() {
                        // Its buffer is sized from the next block on.
                        chans[n].inst = Some(v.inst);
                    }
                    if chans[n].buf.len() >= (at + frames) * 2 {
                        chan = Some(n);
                    }
                }
            }
            let preset = match v.source {
                Source::Zone { .. } => inst.and_then(|i| i.sampler_preset()),
                _ => None,
            };
            // A program with effects plays into its channel's buffer.
            let schan = preset
                .filter(|p| p.program.fx.active())
                .and_then(|_| sfx.iter().position(|c| c.inst == Some(v.inst)))
                .filter(|n| sfx[*n].buf.len() >= (at + frames) * 2);
            let out = match (chan, schan) {
                (Some(n), _) => &mut chans[n].buf[at * 2..(at + frames) * 2],
                (None, Some(n)) => &mut sfx[n].buf[at * 2..(at + frames) * 2],
                _ => &mut buses[bus][at * 2..(at + frames) * 2],
            };
            let forge = match (v.source, inst.map(|i| &i.kind)) {
                (Source::Forge, Some(InstrumentKind::Forge { sound })) => Some(&*sound.rendered),
                _ => None,
            };
            // Live controllers for this note, once per block.
            if let (Some(p), Some(c)) = (orig, ctl.iter().find(|c| c.inst == v.inst)) {
                let (b, pr, sl) = c.for_note(v.ch, p);
                (v.bend_to, v.pressure, v.cc74, v.wheel) = (b, pr, sl, c.wheel);
            }
            let mut synth = orig.map(|p| p.modulated_for_render());
            // The modulation matrix re-applies every 32 frames on top of the
            // patch after macros (no allocation: `mods` stays empty in copies).
            let unmodulated = synth
                .as_ref()
                .filter(|_| orig.is_some_and(|o| !o.mods.is_empty()))
                .cloned();
            let mut until_mod = 0usize;
            // Channel volume and pan, smoothed ~5 ms toward the block's target;
            // skipped at unity so old renders stay bit-exact.
            let target = inst
                .and_then(|i| channel_gains(auto, live, i))
                .or_else(|| {
                    inst.filter(|i| i.volume != 1.0 || i.pan != 0.0)
                        .map(|i| i.gains())
                })
                .unwrap_or([1.0; 2]);
            // A fresh voice starts at the channel's gains, not gliding to them.
            let mut gain = v.gain_state.unwrap_or(target);
            for frame in out.as_chunks_mut::<2>().0 {
                if v.done {
                    break;
                }
                let s = match v.source {
                    Source::Drum { row } => v.render_drum(row, dt),
                    Source::Synth => match synth.as_mut() {
                        Some(p) => {
                            if let (Some(base), Some(o)) = (&unmodulated, orig) {
                                if until_mod == 0 {
                                    *p = base.clone();
                                    let src = v.mod_sources(o, song.tempo);
                                    o.apply_mods(p, &src);
                                    until_mod = 32;
                                }
                                until_mod -= 1;
                            }
                            v.render_synth(p, self.tables, rate)
                        }
                        None => {
                            v.done = true;
                            [0.0; 2]
                        }
                    },
                    Source::Tone => v.render_tone(dt),
                    Source::Forge => match forge {
                        Some(r) => v.render_forge(r, rate),
                        // The channel changed type under the voice.
                        None => {
                            v.done = true;
                            [0.0; 2]
                        }
                    },
                    Source::Zone { zone } => {
                        match preset.and_then(|p| p.zones.get(zone).map(|z| (z, &p.program))) {
                            Some((z, program)) => v.render_zone(z, program, rate),
                            // The preset changed under the voice: drop it.
                            None => {
                                v.done = true;
                                [0.0; 2]
                            }
                        }
                    }
                };
                let s = if gain != target {
                    gain[0] += (target[0] - gain[0]) * alpha;
                    gain[1] += (target[1] - gain[1]) * alpha;
                    if (target[0] - gain[0]).abs() < 1e-6 {
                        gain[0] = target[0];
                    }
                    if (target[1] - gain[1]).abs() < 1e-6 {
                        gain[1] = target[1];
                    }
                    [s[0] * gain[0], s[1] * gain[1]]
                } else if gain == [1.0; 2] {
                    s
                } else {
                    [s[0] * gain[0], s[1] * gain[1]]
                };
                frame[0] += s[0] as f32;
                frame[1] += s[1] as f32;
            }
            v.gain_state = Some(gain);
        }
        self.voices.retain(|v| !v.done);
    }
}

/// The first slide that starts inside this note on its instrument and group,
/// as (start s, duration s, semitones) relative to the note.
fn note_slide(p: &Pattern, n: &Note, sps: f64, swing: f64) -> Option<(f64, f64, f64)> {
    let s = p
        .slides
        .iter()
        .filter(|s| {
            s.instrument == n.instrument
                && s.group == n.group
                && s.start >= n.start
                && s.start < n.end()
        })
        .min_by_key(|s| s.start)?;
    let highest = p
        .notes
        .iter()
        .filter(|v| {
            v.instrument == s.instrument
                && v.group == s.group
                && v.start <= s.start
                && v.end() > s.start
        })
        .map(|v| v.pitch)
        .max()
        .unwrap_or(n.pitch);
    Some((
        steps(s.start - n.start) * sps + swing_offset(s.start, sps, swing)
            - swing_offset(n.start, sps, swing),
        steps(s.length) * sps,
        s.target as f64 - highest as f64,
    ))
}

/// Offline render: one pass of the pattern (or the playlist), then the tail
/// until every voice has finished.
pub fn render(song: &Song, pattern: u64, song_mode: bool, rate: u32) -> Vec<[f32; 2]> {
    let mut engine = Engine::new(rate);
    if song
        .instruments
        .iter()
        .any(|i| matches!(i.kind, InstrumentKind::Vocal { .. }))
    {
        // Singing is rendered ahead; an export waits for it.
        engine.vocal.store(std::sync::Arc::new(
            crate::vocal::host::render_all(song, rate).0,
        ));
    }
    let length = loop_frames(song, pattern, song_mode, rate);
    let cap = length + (MAX_TAIL_SECONDS * rate as f64) as u64;
    let block = 512u64;
    let mut out: Vec<f32> = Vec::with_capacity(length as usize * 2);
    let mut buf = vec![0.0f32; block as usize * 2];
    let mut pos = 0u64;
    while pos < length || (engine.active_voices() > 0 && pos < cap) {
        let n = if pos < length {
            (length - pos).min(block)
        } else {
            block
        };
        let slice = &mut buf[..n as usize * 2];
        engine.process(song, pattern, song_mode, pos, slice, false);
        out.extend_from_slice(slice);
        pos += n;
    }
    out.as_chunks::<2>()
        .0
        .iter()
        .map(|c| [c[0], c[1]])
        .collect()
}

/// Insert `t`'s buffer to process, and insert `key`'s to listen to (not `t` itself).
fn bus_and_key(
    buses: &mut [Vec<f32>],
    t: usize,
    key: Option<usize>,
    n: usize,
) -> (&mut [f32], Option<&[f32]>) {
    match key {
        Some(k) if k < t => {
            let (lo, hi) = buses.split_at_mut(t);
            (&mut hi[0][..n], Some(&lo[k][..n]))
        }
        Some(k) if k > t && k < buses.len() => {
            let (lo, hi) = buses.split_at_mut(k);
            (&mut lo[t][..n], Some(&hi[0][..n]))
        }
        _ => (&mut buses[t][..n], None),
    }
}

/// Forget one slot's smoothers (bypassed, or a different effect in it): the
/// next block starts at the slot's own values. The parameter vectors stay
/// allocated; only their contents clear.
fn reset_fx_smoothers(
    mix: &mut [Option<f32>],
    plugin: &mut [Option<u64>],
    params: &mut [Option<(crate::fx::FxKind, Vec<f32>)>],
    si: usize,
) {
    if let Some(m) = mix.get_mut(si) {
        *m = None;
    }
    if let Some(p) = plugin.get_mut(si) {
        *p = None;
    }
    if let Some(Some((_, vals))) = params.get_mut(si) {
        vals.clear();
    }
}

/// A channel's left/right gains with automated volume or pan, if either is automated.
fn channel_gains(
    auto: &[crate::automation::ValueAt],
    live: &[LiveParam],
    inst: &crate::sequence::Instrument,
) -> Option<[f64; 2]> {
    use crate::automation::Target;
    let vol = override_of(auto, live, Target::ChannelVolume { inst: inst.id });
    let pan = override_of(auto, live, Target::ChannelPan { inst: inst.id });
    if vol.is_none() && pan.is_none() {
        return None;
    }
    let (v, p) = (
        vol.unwrap_or(inst.volume) as f64,
        pan.unwrap_or(inst.pan) as f64,
    );
    Some([v * (1.0 - p.max(0.0)), v * (1.0 + p.min(0.0))])
}

/// A mixer track's fader and pan gains, automation applied.
fn mixer_gains(
    auto: &[crate::automation::ValueAt],
    live: &[LiveParam],
    t: usize,
    track: &crate::sequence::MixerTrack,
) -> [f32; 2] {
    use crate::automation::Target;
    if auto.is_empty() && live.is_empty() {
        return track.gains();
    }
    let v =
        override_of(auto, live, Target::MixerVolume { track: t as u16 }).unwrap_or(track.volume);
    let p = override_of(auto, live, Target::MixerPan { track: t as u16 }).unwrap_or(track.pan);
    [v * (1.0 - p.max(0.0)), v * (1.0 + p.min(0.0))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::{Adsr, ZoneParams};
    use crate::sequence::{Instrument, Note};
    use std::sync::Arc;

    const RATE: u32 = 48_000;

    /// A song whose Bass channel (id 4) plays one zone made of `frames`.
    fn song_with(frames: Vec<[f32; 2]>, setup: impl FnOnce(&mut Zone, &mut Program)) -> Song {
        let end = frames.len();
        let mut zone = Zone {
            name: "test".into(),
            root: 60,
            low: 0,
            high: 127,
            low_velocity: 1,
            high_velocity: 127,
            sample_rate: RATE,
            frames: Arc::new(frames),
            offset: 0,
            end,
            loop_mode: LoopMode::None,
            loop_start: 0,
            loop_end: end,
            adsr: Adsr {
                attack: 0.0,
                release: 0.0,
                ..Adsr::default()
            },
            gain: 1.0,
            pan: 0.0,
            tune_cents: 0.0,
            velocity_tracking: 0.0,
            params: ZoneParams::default(),
        };
        let mut program = Program::default();
        setup(&mut zone, &mut program);
        let preset = SamplePreset {
            name: "t".into(),
            program,
            zones: vec![zone],
            original_dwp: std::sync::Arc::new(vec![]),
            report: String::new(),
        };
        preset.validate().unwrap();
        let mut song = Song::default();
        song.set_sampler_preset(4, preset).unwrap();
        song
    }

    fn note(start: i64, length: i64, pitch: u8) -> Note {
        Note::new(
            4,
            start * crate::sequence::STEP,
            length * crate::sequence::STEP,
            pitch,
            100,
        )
    }

    fn energy(v: &[[f32; 2]]) -> f64 {
        v.iter().map(|s| (s[0] * s[0] + s[1] * s[1]) as f64).sum()
    }

    #[test]
    fn a_channel_reads_playing_then_ringing_then_silent() {
        // A long sample with a 0.3 s release on Bass (id 4).
        let song = song_with(vec![[0.5; 2]; RATE as usize * 4], |z, _| {
            z.adsr.release = 0.3
        });
        let bass = song.instruments.iter().position(|i| i.id == 4).unwrap();
        let mut e = Engine::new(RATE);
        let mut buf = vec![0.0f32; 512 * 2];
        let mut act = [0u8; 16];
        e.live_on(&song, 1, 4, 60, 100);
        e.process_live(&song, &mut buf);
        e.channel_activity(&song, &mut act);
        assert_eq!(act[bass], 2, "the key is down: playing");
        assert!(
            act.iter().enumerate().all(|(i, a)| i == bass || *a == 0),
            "other channels are silent"
        );
        e.live_off(1);
        e.process_live(&song, &mut buf);
        e.channel_activity(&song, &mut act);
        assert_eq!(act[bass], 1, "released: the tail rings");
        for _ in 0..(RATE as usize / 512) {
            e.process_live(&song, &mut buf);
        }
        e.channel_activity(&song, &mut act);
        assert_eq!(act[bass], 0, "the release is over");
    }

    #[test]
    fn a_one_shot_rings_past_its_note_as_a_tail() {
        // A one-step note (1/16 at 120 BPM = 6000 frames) of a 2 s one-shot.
        let mut song = song_with(vec![[0.5; 2]; RATE as usize * 2], |z, _| {
            z.params.one_shot = true
        });
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes = vec![note(0, 1, 60)];
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 4 * crate::sequence::BAR;
        let bass = song.instruments.iter().position(|i| i.id == 4).unwrap();
        let mut e = Engine::new(RATE);
        let mut buf = vec![0.0f32; 1000 * 2];
        let mut act = [0u8; 16];
        e.process(&song, 1, false, 0, &mut buf, false);
        e.channel_activity(&song, &mut act);
        assert_eq!(act[bass], 2, "inside its note: playing");
        let mut pos = 1000;
        while pos < 8000 {
            e.process(&song, 1, false, pos, &mut buf, false);
            pos += 1000;
        }
        e.channel_activity(&song, &mut act);
        assert_eq!(act[bass], 1, "the note ended, the sample goes on");
    }

    #[test]
    fn synth_b_octave_and_legacy_defaults() {
        let bank = crate::synth::TableBank::new();
        let mut p: crate::synth::SynthPatch = serde_json::from_str("{}").unwrap();
        let mut old = Voice::new(1, 1, Source::Synth, 69, 100, 10.0);
        let mut default_voice = old;
        for _ in 0..100 {
            assert_eq!(
                old.render_synth(&p, &bank, 48000.0),
                default_voice.render_synth(&crate::synth::SynthPatch::default(), &bank, 48000.0)
            );
        }
        p.osc_b.level = 1.0;
        p.osc_b.coarse = 12.0;
        let mut voice = Voice::new(1, 1, Source::Synth, 69, 100, 10.0);
        voice.render_synth(&p, &bank, 48000.0);
        assert!((voice.phases_b[0] - 2.0 * voice.phases[0]).abs() < 1e-12);
    }

    #[test]
    fn synth_pitch_filter_and_stereo() {
        let bank = crate::synth::TableBank::new();
        let mut patch = crate::synth::SynthPatch::default();
        patch.amp_env.attack = 0.0;
        let render = |p: &crate::synth::SynthPatch| {
            let mut voice = Voice::new(4, 1, Source::Synth, 69, 100, 2.0);
            (0..RATE)
                .map(|_| voice.render_synth(p, &bank, RATE as f64))
                .collect::<Vec<_>>()
        };
        let sine = render(&patch);
        let crossings = sine
            .windows(2)
            .filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0)
            .count();
        assert!((crossings as f64 - 440.0).abs() < 4.4, "{crossings}");
        assert!(sine.iter().any(|s| s[0].abs() > 0.01));
        patch.position = 1.0;
        let bright = render(&patch);
        patch.cutoff = 300.0;
        let dark = render(&patch);
        let high = |s: &[[f64; 2]]| {
            s.windows(2)
                .map(|w| (w[1][0] - w[0][0]).powi(2))
                .sum::<f64>()
        };
        assert!(high(&dark) < high(&bright) * 0.2);
        patch.unison = 5;
        patch.detune = 0.2;
        patch.spread = 0.8;
        assert!(render(&patch).iter().any(|s| (s[0] - s[1]).abs() > 0.001));
    }

    #[test]
    fn synth_live_note_uses_engine_bus() {
        let mut song = Song::default();
        song.instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .kind = InstrumentKind::Synth {
            patch: Default::default(),
        };
        let mut e = Engine::new(RATE);
        let mut block = vec![0.0f32; 1024];
        e.live_on(&song, 7, 4, 69, 100);
        e.process_live(&song, &mut block);
        assert!(block.iter().any(|x| x.abs() > 0.001));
    }

    /// One second of a live A4 on a synth channel with `patch` (left channel).
    fn tide_live(patch: crate::synth::SynthPatch) -> Vec<f32> {
        let mut song = Song::default();
        song.instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .kind = InstrumentKind::Synth { patch };
        let mut e = Engine::new(RATE);
        e.live_on(&song, 7, 4, 69, 100);
        let mut out = Vec::new();
        let mut block = vec![0.0f32; 1024];
        while out.len() < RATE as usize {
            block.fill(0.0);
            e.process_live(&song, &mut block);
            out.extend(block.as_chunks::<2>().0.iter().map(|f| f[0]));
        }
        out
    }

    fn tide_patch() -> crate::synth::SynthPatch {
        let mut p = crate::synth::SynthPatch::default();
        p.amp_env.attack = 0.0;
        p
    }

    /// Magnitude of `hz` in `s` (one DFT bin over the steady part).
    fn bin(s: &[f32], hz: f64) -> f64 {
        let part = &s[RATE as usize / 4..];
        let (mut re, mut im) = (0.0, 0.0);
        for (i, x) in part.iter().enumerate() {
            let w = std::f64::consts::TAU * hz * i as f64 / RATE as f64;
            re += *x as f64 * w.cos();
            im += *x as f64 * w.sin();
        }
        (re * re + im * im).sqrt() / part.len() as f64
    }

    #[test]
    fn the_spectral_oscillator_plays_the_partials_it_is_told() {
        let mut p = tide_patch();
        p.level = 0.0;
        p.spectral = crate::synth::Spectral {
            level: 1.0,
            partials: 8,
            odd: 1.0,
            ..Default::default()
        };
        let s = tide_live(p.clone());
        let f = 440.0;
        let odd: Vec<f64> = [1.0, 3.0, 5.0, 7.0]
            .iter()
            .map(|k| bin(&s, f * k))
            .collect();
        let even: Vec<f64> = [2.0, 4.0, 6.0].iter().map(|k| bin(&s, f * k)).collect();
        assert!(odd.iter().all(|m| *m > 1e-3), "odd partials sound: {odd:?}");
        assert!(
            even.iter().all(|m| *m < odd[3] * 0.05),
            "even partials gone: {even:?} vs {odd:?}"
        );
        assert!(
            bin(&s, f * 9.0) < odd[3] * 0.05,
            "nothing past the 8 asked for"
        );
        // A saw's tilt: the third partial about a third of the first.
        assert!(
            (odd[1] / odd[0] - 1.0 / 3.0).abs() < 0.05,
            "1/k slope: {}",
            odd[1] / odd[0]
        );
        // Off (level 0) is silent and costs nothing.
        p.spectral.level = 0.0;
        assert!(tide_live(p).iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn spectral_partials_stop_below_nyquist() {
        let c = crate::synth::Spectral {
            level: 1.0,
            partials: 32,
            ..Default::default()
        };
        let mut amps = [0.0; crate::synth::MAX_PARTIALS];
        assert_eq!(c.amplitudes(110.0, 48_000.0, 0.0, &mut amps), 32);
        assert_eq!(
            c.amplitudes(4_000.0, 48_000.0, 0.0, &mut amps),
            5,
            "5 × 4 kHz is under 21.6 kHz, 6 × is not"
        );
        assert_eq!(
            c.amplitudes(30_000.0, 48_000.0, 0.0, &mut amps),
            1,
            "the fundamental always plays"
        );
    }

    /// Half a second of live synth output after `setup` (left channel).
    fn tide_after(
        patch: crate::synth::SynthPatch,
        setup: impl FnOnce(&mut Engine, &Song),
    ) -> Vec<f32> {
        let mut song = Song::default();
        song.instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .kind = InstrumentKind::Synth { patch };
        let mut e = Engine::new(RATE);
        setup(&mut e, &song);
        let mut out = Vec::new();
        let mut block = vec![0.0f32; 1024];
        while out.len() < RATE as usize / 2 {
            e.process_live(&song, &mut block);
            out.extend(block.as_chunks::<2>().0.iter().map(|f| f[0]));
        }
        out
    }

    /// The strongest of `hz` in `s` (DFT bins).
    fn loudest(s: &[f32], hz: &[f64]) -> f64 {
        *hz.iter()
            .max_by(|a, b| bin(s, **a).total_cmp(&bin(s, **b)))
            .unwrap()
    }

    #[test]
    fn the_pitch_wheel_bends_a_synth_by_its_range() {
        let mut p = tide_patch();
        p.table = crate::synth::Table::SineSaw;
        p.bend_range = 2.0;
        let up = tide_after(p.clone(), |e, song| {
            e.live_midi(song, 4, [0xE0, 0x7F, 0x7F]);
            e.live_on(song, 1, 4, 69, 100);
        });
        assert_eq!(
            loudest(&up, &[440.0, 466.16, 493.88]),
            493.88,
            "full wheel up: A4 + 2 semitones"
        );
        // A wheel moved while the note sounds glides it there.
        let moved = tide_after(p, |e, song| {
            e.live_on(song, 1, 4, 69, 100);
            e.live_midi(song, 4, [0xE3, 0x00, 0x00]);
        });
        assert_eq!(
            loudest(&moved, &[392.0, 415.3, 440.0]),
            392.0,
            "down 2 from any channel without MPE"
        );
    }

    #[test]
    fn mpe_bends_only_the_note_on_its_channel() {
        let mut p = tide_patch();
        p.mpe = true;
        // Two notes, channels 2 and 3; channel 3's wheel a quarter up = +12 st.
        let s = tide_after(p, |e, song| {
            e.live_on_ch(song, 1, 4, 57, 100, 1);
            e.live_on_ch(song, 2, 4, 64, 100, 2);
            e.live_midi(song, 4, [0xE2, 0x00, 0x50]);
        });
        let a3 = bin(&s, 220.0);
        let e4 = bin(&s, 329.63);
        let e5 = bin(&s, 659.26);
        assert!(a3 > 1e-3, "channel 2's note stays: {a3}");
        assert!(
            e5 > e4 * 4.0,
            "channel 3's note went up an octave: {e5} vs {e4}"
        );
    }

    #[test]
    fn the_mod_wheel_drives_a_synth_route() {
        use crate::synth::{ModRoute, ModSource, ModTarget};
        let mut p = tide_patch();
        p.cutoff = 200.0;
        p.table = crate::synth::Table::SquarePulse;
        p.mods.push(ModRoute {
            source: ModSource::ModWheel,
            target: ModTarget::Cutoff,
            depth: 1.0,
        });
        let rms = |s: &[f32]| {
            (s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
        };
        let closed = tide_after(p.clone(), |e, song| e.live_on(song, 1, 4, 69, 100));
        let open = tide_after(p, |e, song| {
            e.live_midi(song, 4, [0xB0, 1, 127]);
            e.live_on(song, 1, 4, 69, 100);
        });
        assert!(
            rms(&open) > rms(&closed) * 2.0,
            "the wheel opens the filter: {} vs {}",
            rms(&open),
            rms(&closed)
        );
    }

    /// `cargo test --release --lib tide_cpu -- --ignored --nocapture`:
    /// how long 16 heavy notes take to render, against real time.
    #[test]
    #[ignore]
    fn tide_cpu() {
        let mut p = tide_patch();
        p.unison = 7;
        p.detune = 0.2;
        p.osc_b.level = 0.5;
        p.spectral.level = 0.5;
        p.spectral.partials = 32;
        p.cutoff = 3000.0;
        p.resonance = 0.3;
        p.filter_env = crate::sampler::Adsr {
            attack: 0.2,
            decay: 0.5,
            sustain: 0.3,
            ..Default::default()
        };
        p.env_amount = 0.5;
        p.lfos[0] = crate::synth::Lfo {
            shape: crate::synth::LfoShape::Sine,
            rate: 3.0,
            ..Default::default()
        };
        p.mods.push(crate::synth::ModRoute {
            source: crate::synth::ModSource::Lfo1,
            target: crate::synth::ModTarget::Cutoff,
            depth: 0.3,
        });
        if let Ok(v) = std::env::var("KFXL_BENCH") {
            for part in v.split(',') {
                match part {
                    "noc" => p.spectral.level = 0.0,
                    "nob" => p.osc_b.level = 0.0,
                    "uni1" => p.unison = 1,
                    "nolfo" => p.mods.clear(),
                    _ => {}
                }
            }
        }
        let mut song = Song::default();
        song.instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .kind = InstrumentKind::Synth { patch: p };
        let mut e = Engine::new(RATE);
        for k in 0..16 {
            e.live_on(&song, k, 4, 40 + k as u8 * 2, 100);
        }
        let mut block = vec![0.0f32; 512];
        let seconds = 10.0;
        let t0 = std::time::Instant::now();
        for _ in 0..(seconds * RATE as f64 / 256.0) as usize {
            e.process_live(&song, &mut block);
        }
        let took = t0.elapsed().as_secs_f64();
        println!(
            "16 notes x 7 unison, 3 oscillators: {:.1}% of one core ({took:.2} s for {seconds} s)",
            took / seconds * 100.0
        );
    }

    #[test]
    fn every_factory_synth_preset_is_valid_and_plays() {
        use crate::synth::{FX_KINDS, SynthPatch};
        for (i, name) in SynthPatch::PRESETS.iter().enumerate() {
            let p = SynthPatch::factory(i);
            p.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
            let order: Vec<usize> =
                p.fx.iter()
                    .map(|s| FX_KINDS.iter().position(|k| *k == s.kind).unwrap())
                    .collect();
            assert!(
                order.windows(2).all(|w| w[0] < w[1]),
                "{name}: effects in chain order"
            );
            let s = tide_live(p);
            let peak = s.iter().fold(0f32, |m, x| m.max(x.abs()));
            assert!(peak.is_finite() && peak > 0.02, "{name} plays: peak {peak}");
        }
    }

    #[test]
    fn a_synths_own_delay_rings_on_after_its_note_ends() {
        let render = |fx: bool| {
            let mut p = tide_patch();
            p.amp_env.release = 0.01;
            if fx {
                let mut d = crate::fx::FxSlot::new(crate::fx::FxKind::Delay);
                d.params[0] = 4.0; // 4 steps = 0.5 s at 120 BPM
                d.params[1] = 50.0;
                p.fx.push(d);
                // A disabled slot changes nothing.
                let mut off = crate::fx::FxSlot::new(crate::fx::FxKind::Reverb);
                off.enabled = false;
                p.fx.push(off);
            }
            let mut song = Song::default();
            song.tempo = 120.0;
            song.instruments
                .iter_mut()
                .find(|i| i.id == 4)
                .unwrap()
                .kind = InstrumentKind::Synth { patch: p };
            song.validate().unwrap();
            let mut e = Engine::new(RATE);
            let mut block = vec![0.0f32; 512];
            // Two silent blocks first: the channel claims its buffer.
            e.process_live(&song, &mut block);
            e.live_on(&song, 7, 4, 69, 100);
            let mut out = Vec::new();
            while out.len() < RATE as usize {
                if out.len() >= RATE as usize / 10 && out.len() < RATE as usize / 10 + 256 {
                    e.live_off(7);
                }
                block.fill(0.0);
                e.process_live(&song, &mut block);
                out.extend(block.as_chunks::<2>().0.iter().map(|f| f[0]));
            }
            out
        };
        let (dry, wet) = (render(false), render(true));
        let rms = |s: &[f32]| {
            (s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
        };
        let late = RATE as usize * 45 / 100..RATE as usize * 70 / 100;
        assert!(
            rms(&dry[late.clone()]) < 1e-4,
            "no effect: silent after the note"
        );
        assert!(
            rms(&wet[late]) > 0.01,
            "the echo half a second later: {}",
            rms(&wet[RATE as usize * 45 / 100..RATE as usize * 70 / 100])
        );
        let first = RATE as usize / 20..RATE as usize / 10;
        assert!(
            (rms(&wet[first.clone()]) - rms(&dry[first])).abs() < 0.02,
            "the note itself plays dry as before"
        );
    }

    #[test]
    fn tide_lfo_route_moves_the_cutoff_over_time() {
        use crate::synth::{LfoShape, ModRoute, ModSource, ModTarget};
        let mut p = tide_patch();
        p.position = 1.0;
        p.cutoff = 1500.0;
        p.lfos[0].shape = LfoShape::Sine;
        p.lfos[0].rate = 2.0;
        p.lfos[0].retrigger = true;
        p.mods.push(ModRoute {
            source: ModSource::Lfo1,
            target: ModTarget::Cutoff,
            depth: 0.5,
        });
        let s = tide_live(p);
        let win = RATE as usize / 8;
        let high: Vec<f64> = s
            .chunks_exact(win)
            .map(|w| w.windows(2).map(|d| ((d[1] - d[0]) as f64).powi(2)).sum())
            .collect();
        let (lo, hi) = high
            .iter()
            .fold((f64::MAX, 0.0f64), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(
            hi > lo * 3.0,
            "high-frequency energy should swing with the LFO: {high:?}"
        );
    }

    #[test]
    fn tide_pitch_route_at_half_depth_is_an_octave() {
        use crate::synth::{ModRoute, ModSource, ModTarget};
        let mut p = tide_patch();
        p.macros[0].value = 1.0;
        p.mods.push(ModRoute {
            source: ModSource::Macro(0),
            target: ModTarget::PitchA,
            depth: 0.5,
        });
        let s = tide_live(p);
        let crossings = s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((crossings as f64 - 880.0).abs() < 9.0, "{crossings}");
    }

    #[test]
    fn tide_retriggered_lfo_starts_each_note_alike() {
        let mut p = tide_patch();
        p.lfos[0].shape = crate::synth::LfoShape::Saw;
        p.lfos[0].rate = 3.0;
        let mut a = Voice::new(4, 1, Source::Synth, 69, 100, 1.0);
        let mut b = Voice::new(4, 2, Source::Synth, 69, 100, 1.0);
        a.clock = 0.0;
        b.clock = 0.37;
        a.t = 0.1;
        b.t = 0.1;
        assert_ne!(
            a.mod_sources(&p, 120.0)[0],
            b.mod_sources(&p, 120.0)[0],
            "free-running: follows the song clock"
        );
        p.lfos[0].retrigger = true;
        assert_eq!(
            a.mod_sources(&p, 120.0)[0],
            b.mod_sources(&p, 120.0)[0],
            "retriggered: same phase in every note"
        );
    }

    #[test]
    fn tide_without_routes_renders_as_before() {
        let p = tide_patch();
        let mut with = p.clone();
        with.lfos[0].shape = crate::synth::LfoShape::Sine;
        assert_eq!(
            tide_live(p),
            tide_live(with),
            "an LFO without routes changes nothing"
        );
    }

    #[test]
    fn a_tempo_change_moves_later_clips_and_old_songs_stay_flat() {
        use crate::sequence::{BAR, Clip};
        use crate::tempo::{TempoPoint, TempoShape};
        let mut song = Song::default();
        song.tempo = 120.0;
        song.swing = 0.0;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        song.add_note(1, note(0, 1, 60));
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = BAR;
        song.clips = vec![Clip::new(1, 2 * BAR, 0)];
        let onset_after = |v: &[[f32; 2]], from: usize| {
            v[from..]
                .iter()
                .position(|s| s[0].abs() > 1e-4)
                .map(|i| i + from)
        };
        let flat = render(&song, 1, true, RATE);
        let bar120 = (RATE * 2) as usize; // 4/4 at 120 BPM = 2 s
        let flat_second = onset_after(&flat, 0).unwrap();
        assert!(
            (flat_second as i64 - 2 * bar120 as i64).abs() < 64,
            "flat: clip 2 at bar 3 ({flat_second})"
        );
        // 240 BPM from bar 2: bar 2 lasts 1 s, so clip 2 starts at 3 s, not 4 s.
        song.tempo_map.points = vec![
            TempoPoint {
                tick: 0,
                bpm: 120.0,
                shape: TempoShape::Step,
            },
            TempoPoint {
                tick: BAR,
                bpm: 240.0,
                shape: TempoShape::Step,
            },
        ];
        let mapped = render(&song, 1, true, RATE);
        let second = onset_after(&mapped, 0).unwrap();
        assert!(
            (second as i64 - (bar120 + bar120 / 2) as i64).abs() < 64,
            "mapped: clip 2 at 3 s ({second})"
        );
        assert!(mapped.len() < flat.len(), "the song gets shorter");
        let first = |v: &[[f32; 2]]| v[second..second + 2048].to_vec();
        assert_eq!(
            first(&mapped),
            first(&flat[flat_second - second..]),
            "the same hit, only earlier"
        );
    }

    #[test]
    fn a_clip_longer_than_its_pattern_repeats_it() {
        use crate::sequence::{BAR, Clip};
        let mut song = Song::default();
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        song.add_note(1, note(0, 1, 60));
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = BAR;
        let one = Clip::new(1, 0, 0);
        song.clips = vec![one];
        let single = render(&song, 1, true, RATE);
        let mut long = one;
        long.length = Some(3 * BAR);
        song.clips = vec![long];
        let triple = render(&song, 1, true, RATE);
        let bar = single.len().min(triple.len() / 3);
        let energy_at = |v: &[[f32; 2]], k: usize| energy(&v[k * bar..k * bar + bar / 4]);
        assert!(energy_at(&triple, 0) > 0.0);
        assert!(
            (energy_at(&triple, 1) - energy_at(&triple, 0)).abs()
                < 1e-6 * energy_at(&triple, 0).max(1.0),
            "bar 2 replays bar 1"
        );
        assert!(
            (energy_at(&triple, 2) - energy_at(&triple, 0)).abs()
                < 1e-6 * energy_at(&triple, 0).max(1.0),
            "bar 3 too"
        );
    }

    #[test]
    fn live_notes_sound_while_held_and_release_after() {
        let song = Song::default();
        let mut e = Engine::new(RATE);
        let mut block = vec![0.0f32; 1024];
        e.process_live(&song, &mut block);
        assert!(block.iter().all(|s| *s == 0.0), "silent before");
        e.live_on(&song, 1, 4, 57, 100); // Bass
        let mut held = 0.0f64;
        for _ in 0..40 {
            e.process_live(&song, &mut block);
            held += block.iter().map(|s| (s * s) as f64).sum::<f64>();
        }
        assert!(
            held > 0.0 && e.active_voices() > 0,
            "a held key keeps sounding (0.4 s)"
        );
        e.live_off(1);
        for _ in 0..200 {
            e.process_live(&song, &mut block);
        }
        assert_eq!(
            e.active_voices(),
            0,
            "released: the voice ends after its release"
        );
    }

    #[test]
    fn drum_voices_follow_the_key_and_stay_exact_at_c5() {
        let mut song = Song::default();
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        song.add_note(1, Note::new(1, 0, crate::sequence::STEP, 60, 100));
        let c5 = render(&song, 1, false, RATE);
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes[0].pitch = 72;
        let c6 = render(&song, 1, false, RATE);
        let sound = |v: &[[f32; 2]]| v.iter().rposition(|f| f[0].abs() > 1e-4).unwrap_or(0);
        let (a, b) = (sound(&c5), sound(&c6));
        assert!(
            (b as f64 / a as f64 - 0.5).abs() < 0.05,
            "an octave up runs twice as fast: {a} -> {b}"
        );
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes[0].pitch = 60;
        assert_eq!(
            render(&song, 1, false, RATE),
            c5,
            "C5 is the recorded sound"
        );
    }

    #[test]
    fn porta_notes_glide_in_from_the_previous_note() {
        let mut song = Song::default();
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        song.add_note(1, note(0, 4, 48));
        song.add_note(1, note(4, 4, 60));
        let plain = render(&song, 1, false, RATE);
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes[1].porta = true;
        let porta = render(&song, 1, false, RATE);
        let step = (RATE as f64 * 60.0 / song.tempo / 4.0) as usize;
        assert_eq!(
            plain[..4 * step - 10],
            porta[..4 * step - 10],
            "the first note is untouched"
        );
        assert_ne!(
            plain[4 * step..6 * step],
            porta[4 * step..6 * step],
            "the porta note bends in"
        );
        let text = serde_json::to_string(&song.patterns[0].notes[1]).unwrap();
        assert!(text.contains("porta"));
        assert!(
            !serde_json::to_string(&song.patterns[0].notes[0])
                .unwrap()
                .contains("porta"),
            "off is not written"
        );
    }

    #[test]
    fn live_blocks_match_the_export() {
        let mut song = Song::demo();
        song.swing = 0.25;
        let offline = render(&song, 1, false, RATE);
        let length = loop_frames(&song, 1, false, RATE) as usize;
        let mut engine = Engine::new(RATE);
        let mut live = Vec::new();
        let mut pos = 0;
        let mut buf = vec![0.0f32; 2 * 300];
        while live.len() < length * 2 {
            pos = engine.process(&song, 1, false, pos, &mut buf, true);
            live.extend_from_slice(&buf);
        }
        for f in 0..length {
            assert_eq!([live[f * 2], live[f * 2 + 1]], offline[f], "frame {f}");
        }
    }

    #[test]
    fn a_time_selection_loops_the_song_between_its_bars() {
        let mut song = Song::demo();
        song.clips = (0..4)
            .map(|b| crate::sequence::Clip::new(1, b * BAR, 0))
            .collect();
        song.loop_range = Some((BAR, 2 * BAR));
        let bar = loop_frames(&song, 1, false, RATE);
        let (start, end) = loop_bounds(&song, None, 1, true, RATE);
        assert_eq!((start, end), (bar, 2 * bar));
        let mut engine = Engine::new(RATE);
        // 700 frames: no block boundary lands on a seam, so both wraps fall inside one.
        let mut buf = vec![0.0f32; 2 * 700];
        let mut at = 0u64;
        let mut wraps = Vec::new();
        let mut loud_after = 0.0f32;
        while at < 4 * bar {
            engine.process(&song, 1, true, at, &mut buf, true);
            let (from, wrap) = engine.block_timeline();
            // A lead-in plays bar 1, then every block stays inside the loop.
            assert_eq!(from, wrap_frame(at, (start, end)));
            assert!(from < end, "block at {from} ran past the loop end {end}");
            if at >= bar {
                assert!(from >= start, "block at {from} fell before the loop start");
            }
            if let Some((w, to)) = wrap {
                assert_eq!(to, start, "the loop restarts at the selection, not bar 1");
                wraps.push(at + w as u64);
            }
            if at >= end {
                loud_after = buf.iter().fold(loud_after, |m, x| m.max(x.abs()));
            }
            at += 700;
        }
        assert_eq!(wraps, [end, 3 * bar, 4 * bar], "each pass is one bar long");
        assert!(loud_after > 0.05, "the looped bar keeps playing its notes");
    }

    #[test]
    fn release_rings_across_the_loop_seam() {
        let mut song = song_with(vec![[0.5; 2]; RATE as usize * 4], |z, _| {
            z.adsr.release = 0.5;
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(15, 1, 60));
        let length = loop_frames(&song, 1, false, RATE);
        let mut engine = Engine::new(RATE);
        let mut buf = vec![0.0f32; length as usize * 2];
        engine.process(&song, 1, false, 0, &mut buf, true);
        let mut next = vec![0.0f32; 2 * 100];
        engine.process(&song, 1, false, 0, &mut next, true);
        assert!(next[20].abs() > 0.01, "release continues after the seam");
    }

    #[test]
    fn low_pass_filter_tames_highs() {
        let nyquist: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|i| [if i % 2 == 0 { 0.5 } else { -0.5 }; 2])
            .collect();
        let mut dry = song_with(nyquist.clone(), |_, _| {});
        std::sync::Arc::make_mut(&mut dry.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let mut wet = song_with(nyquist, |z, _| {
            for f in &mut z.params.filters {
                f.kind = FilterKind::Lowpass;
                f.cutoff = 300.0;
            }
        });
        std::sync::Arc::make_mut(&mut wet.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let (d, w) = (
            energy(&render(&dry, 1, false, RATE)),
            energy(&render(&wet, 1, false, RATE)),
        );
        assert!(w < d * 0.01, "dry {d}, filtered {w}");
    }

    #[test]
    fn mono_keeps_one_note_and_cut_groups_choke() {
        let mut song = song_with(vec![[0.5; 2]; RATE as usize], |_, p| {
            p.play_mode = PlayMode::Mono
        });
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes =
            vec![note(0, 8, 60), note(2, 8, 64)];
        let mut e = Engine::new(RATE);
        let mut buf = vec![0.0f32; 2 * (RATE as usize / 2)];
        e.process(&song, 1, false, 0, &mut buf, false);
        assert_eq!(e.voices.iter().filter(|v| v.kill.is_none()).count(), 1);
        // Polyphonic with a cut group behaves the same way: the new hit chokes the old.
        let mut song = song_with(vec![[0.5; 2]; RATE as usize], |z, _| {
            z.params.mute_group = 1
        });
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes =
            vec![note(0, 8, 60), note(2, 8, 64)];
        let mut e = Engine::new(RATE);
        e.process(&song, 1, false, 0, &mut buf, false);
        assert_eq!(e.voices.iter().filter(|v| v.kill.is_none()).count(), 1);
    }

    #[test]
    fn one_shot_ignores_note_length_and_reverse_starts_at_the_end() {
        let ramp: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|i| [i as f32 / RATE as f32; 2])
            .collect();
        let mut song = song_with(ramp.clone(), |z, _| z.params.one_shot = true);
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 1, 60));
        let out = render(&song, 1, false, RATE);
        let step = (seconds_per_step(120.0) * RATE as f64) as usize;
        assert!(
            out[step * 3][0] > 0.1,
            "still playing after the 1-step gate"
        );
        let mut song = song_with(ramp, |z, _| {
            z.params.reverse = true;
            z.adsr.attack = 0.0;
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 4, 60));
        let out = render(&song, 1, false, RATE);
        assert!(
            out[0][0] > 0.99,
            "reverse begins at the last frame: {}",
            out[0][0]
        );
    }

    #[test]
    fn matrix_routes_lfo_to_gain_and_env_to_cutoff() {
        use crate::sampler::{ModSource as S, ModTarget as T};
        let dc = vec![[0.5f32; 2]; RATE as usize];
        let mut song = song_with(dc.clone(), |z, _| {
            z.params.lfo[0].rate = 8.0;
            z.params.route(S::Lfo1, T::Gain, -1.0);
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let out = render(&song, 1, false, RATE);
        let window = &out[1000..RATE as usize / 2];
        let (lo, hi) = window
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s[0]), b.max(s[0])));
        assert!(hi - lo > 0.3, "tremolo swings: {lo}..{hi}");
        // ENV 1 opens a closed lowpass over time: early output quieter than late.
        let saw: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|i| [((i % 40) as f32 / 20.0 - 1.0) * 0.5; 2])
            .collect();
        let mut song = song_with(saw, |z, _| {
            z.params.filters[0] = crate::sampler::Filter {
                kind: FilterKind::Lowpass,
                cutoff: 60.0,
                emphasis: 0.0,
            };
            z.params.env[0] = crate::sampler::Adsr {
                attack: 0.5,
                sustain: 1.0,
                ..crate::sampler::Adsr::flat()
            };
            z.params.route(S::Env1, T::Cutoff1, 1.0);
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 16, 60));
        let out = render(&song, 1, false, RATE);
        let early = energy(&out[0..4800]);
        let late = energy(&out[24000..28800]);
        assert!(late > early * 4.0, "early {early}, late {late}");
    }

    #[test]
    fn legato_overlap_retargets_without_retrigger() {
        let mut song = song_with(vec![[0.5; 2]; RATE as usize], |_, p| {
            p.play_mode = PlayMode::Legato;
            p.glide = 0.1;
        });
        std::sync::Arc::make_mut(&mut song.patterns[0]).notes =
            vec![note(0, 8, 60), note(4, 8, 67)];
        let mut e = Engine::new(RATE);
        let step = (seconds_per_step(120.0) * RATE as f64) as usize;
        let mut buf = vec![0.0f32; 2 * (step * 5)];
        e.process(&song, 1, false, 0, &mut buf, false);
        assert_eq!(e.voices.len(), 1, "one voice, retargeted");
        assert_eq!(e.voices[0].key, 67);
        assert!(
            e.voices[0].t > seconds_per_step(120.0) * 4.5,
            "envelope kept running"
        );
    }

    #[test]
    fn automation_clip_moves_the_master_fader_in_song_mode() {
        use crate::automation::{Envelope, Target};
        use crate::sequence::{BAR, Clip};
        let mut song = Song::default();
        song.instruments.push(Instrument::new(
            9,
            "S",
            InstrumentKind::Sampler { preset: None },
        ));
        // Two bars of a held tone.
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 2 * BAR;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(Note {
                instrument: 9,
                ..note(0, 32, 69)
            });
        song.clips.push(Clip::new(1, 0, 0));
        let plain = render(&song, 1, true, RATE);
        // Master down to silence for the second bar only.
        let mut auto = Pattern::new(7, "Master");
        auto.length = BAR;
        auto.automation = Some(Envelope::flat(Target::MixerVolume { track: 0 }, 0.0, BAR));
        song.patterns.push(auto.into());
        song.clips.push(Clip::new(7, BAR, 1));
        let ducked = render(&song, 1, true, RATE);
        let bar = (16.0 * seconds_per_step(song.tempo) * RATE as f64) as usize;
        let first = |o: &[[f32; 2]]| energy(&o[bar / 4..bar / 2]);
        let second = |o: &[[f32; 2]]| energy(&o[bar + bar / 4..bar + bar / 2]);
        assert!(
            (first(&plain) - first(&ducked)).abs() < 1e-6,
            "bar 1 untouched"
        );
        assert!(
            second(&plain) > 1.0 && second(&ducked) < 1e-6,
            "bar 2 silenced: {} vs {}",
            second(&plain),
            second(&ducked)
        );
    }

    #[test]
    fn empty_sampler_plays_a_test_tone() {
        let mut song = Song::default();
        song.instruments.push(Instrument::new(
            9,
            "S",
            InstrumentKind::Sampler { preset: None },
        ));
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(Note {
                instrument: 9,
                ..note(0, 4, 69)
            });
        assert!(energy(&render(&song, 1, false, RATE)) > 1.0);
        song.instruments.last_mut().unwrap().fl_plugin =
            Some(crate::sequence::FlPluginState::new("FLEX", b"preset"));
        assert_eq!(energy(&render(&song, 1, false, RATE)), 0.0);
    }

    /// `frames` frames of the song in 64-sample blocks (a tiny live buffer).
    fn render64(song: &Song, song_mode: bool, frames: usize) -> Vec<[f32; 2]> {
        let mut e = Engine::new(RATE);
        let mut out = Vec::new();
        let mut buf = [0.0f32; 64 * 2];
        let mut pos = 0;
        while out.len() < frames {
            pos = e.process(song, 1, song_mode, pos, &mut buf, true);
            out.extend(buf.as_chunks::<2>().0.iter().map(|f| [f[0], f[1]]));
        }
        out.truncate(frames);
        out
    }

    /// A bar in frames at the song's tempo.
    fn bar_frames(song: &Song) -> usize {
        (16.0 * seconds_per_step(song.tempo) * RATE as f64) as usize
    }

    /// Two bars of a held 0.5 DC tone, ready for automation clips.
    fn held_tone_song() -> Song {
        let mut song = song_with(vec![[0.5; 2]; RATE as usize * 4], |_, _| {});
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 2 * crate::sequence::BAR;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 32, 60));
        song.clips.push(crate::sequence::Clip::new(1, 0, 0));
        song
    }

    /// An automation clip holding `value` over the second bar of `target`.
    fn step_at_bar_two(song: &mut Song, id: u64, target: crate::automation::Target, value: f32) {
        let bar = crate::sequence::BAR;
        let mut auto = Pattern::new(id, "Step");
        auto.length = bar;
        auto.automation = Some(crate::automation::Envelope::flat(target, value, bar));
        song.patterns.push(auto.into());
        song.clips.push(crate::sequence::Clip::new(id, bar, 1));
    }

    /// The last frame after `blocks` 64-frame blocks of pattern 1 played from
    /// the start (the smoothers settle well inside 150 blocks).
    fn live_tail(e: &mut Engine, song: &Song, blocks: usize) -> [f32; 2] {
        // Restarting at 0 would stack a second note on the sounding one.
        e.stop();
        let mut buf = [0.0f32; 64 * 2];
        let mut pos = 0;
        for _ in 0..blocks {
            pos = e.process(song, 1, false, pos, &mut buf, true);
        }
        [buf[126], buf[127]]
    }

    fn near(got: [f32; 2], want: [f32; 2], what: &str) {
        assert!(
            (got[0] - want[0]).abs() < 1e-4 && (got[1] - want[1]).abs() < 1e-4,
            "{what}: {got:?} vs {want:?}"
        );
    }

    #[test]
    fn live_params_move_faders_pans_and_channels() {
        use crate::automation::Target;
        let song = held_tone_song();
        let mut e = Engine::new(RATE);
        near(live_tail(&mut e, &song, 150), [0.5, 0.5], "untouched");
        for (target, value, want) in [
            (Target::MixerVolume { track: 0 }, 0.2, [0.1, 0.1]),
            (Target::MixerPan { track: 0 }, 1.0, [0.0, 0.1]),
            (Target::ChannelVolume { inst: 4 }, 0.5, [0.0, 0.05]),
            (Target::MixerPan { track: 0 }, 0.0, [0.05, 0.05]),
            (Target::ChannelPan { inst: 4 }, -1.0, [0.05, 0.0]),
        ] {
            e.apply_live_param(target, value);
            // Cumulative: each step adds to the ones before it.
            near(live_tail(&mut e, &song, 150), want, &format!("{target:?}"));
        }
        e.clear_live_params();
        near(live_tail(&mut e, &song, 150), [0.5, 0.5], "cleared");
    }

    #[test]
    fn a_live_param_updates_in_place_and_survives_stop() {
        use crate::automation::Target;
        let song = held_tone_song();
        let mut e = Engine::new(RATE);
        let t = Target::MixerVolume { track: 0 };
        e.apply_live_param(t, 0.2);
        e.apply_live_param(t, 0.4);
        assert_eq!(e.live_params.len(), 1, "one entry per control");
        e.stop();
        assert_eq!(e.live_params.len(), 1, "a fader moved while stopped holds");
        near(live_tail(&mut e, &song, 150), [0.2, 0.2], "after Play");
    }

    #[test]
    fn live_params_respect_the_reserved_capacity() {
        use crate::automation::Target;
        let mut e = Engine::new(RATE);
        e.reserve(4, 64);
        let cap = e.live_params.capacity();
        assert!(cap >= MAX_LIVE_PARAMS);
        for track in 0..(cap as u16 + 50) {
            e.apply_live_param(Target::MixerVolume { track }, 0.5);
        }
        assert_eq!(
            e.live_params.len(),
            MAX_LIVE_PARAMS,
            "a full list drops, never grows"
        );
        assert_eq!(e.live_params.capacity(), cap);
        e.clear_live_params();
        assert_eq!(e.live_params.capacity(), cap, "clearing keeps the room");
        // An engine that never reserved still takes up to the cap.
        let mut offline = Engine::new(RATE);
        for track in 0..(MAX_LIVE_PARAMS as u16 + 10) {
            offline.apply_live_param(Target::MixerVolume { track }, 0.5);
        }
        assert_eq!(offline.live_params.len(), MAX_LIVE_PARAMS);
    }

    #[test]
    fn a_live_fx_mix_and_param_change_the_sound() {
        use crate::automation::Target;
        let mut song = held_tone_song();
        // Balance at 0% volume is silent at full mix.
        let mut bal = crate::fx::FxSlot::new(crate::fx::FxKind::Balance);
        bal.params[0] = 0.0;
        song.mixer[0].fx.push(bal);
        let mut e = Engine::new(RATE);
        near(live_tail(&mut e, &song, 150), [0.0, 0.0], "silent balance");
        e.apply_live_param(
            Target::FxParam {
                track: 0,
                slot: 0,
                param: 0,
            },
            50.0,
        );
        near(
            live_tail(&mut e, &song, 150),
            [0.25, 0.25],
            "live volume 50%",
        );
        e.apply_live_param(Target::FxMix { track: 0, slot: 0 }, 0.0);
        near(
            live_tail(&mut e, &song, 150),
            [0.5, 0.5],
            "live mix fully dry",
        );
        e.clear_live_params();
        near(
            live_tail(&mut e, &song, 150),
            [0.0, 0.0],
            "back to the song",
        );
    }

    #[test]
    fn automation_wins_over_live_params() {
        use crate::automation::Target;
        let mut song = held_tone_song();
        let mut bal = crate::fx::FxSlot::new(crate::fx::FxKind::Balance);
        bal.params[0] = 100.0;
        song.mixer[0].fx.push(bal);
        // Bar 1: master fader 0.5 and balance volume 50%, both automated.
        for (id, target, value) in [
            (7u64, Target::MixerVolume { track: 0 }, 0.5f32),
            (
                8,
                Target::FxParam {
                    track: 0,
                    slot: 0,
                    param: 0,
                },
                50.0,
            ),
        ] {
            let mut clip = Pattern::new(id, "Held");
            clip.length = crate::sequence::BAR;
            clip.automation = Some(crate::automation::Envelope::flat(
                target,
                value,
                crate::sequence::BAR,
            ));
            song.patterns.push(clip.into());
            song.clips.push(crate::sequence::Clip::new(id, 0, 1));
        }
        let run = |live: &[(Target, f32)]| {
            let mut e = Engine::new(RATE);
            for &(t, v) in live {
                e.apply_live_param(t, v);
            }
            let mut buf = [0.0f32; 64 * 2];
            let mut pos = 0;
            for _ in 0..150 {
                pos = e.process(&song, 1, true, pos, &mut buf, true);
            }
            [buf[126], buf[127]]
        };
        let base = run(&[]);
        near(base, [0.5 * 0.5 * 0.5; 2], "automation alone");
        let both = [
            (Target::MixerVolume { track: 0 }, 1.0),
            (
                Target::FxParam {
                    track: 0,
                    slot: 0,
                    param: 0,
                },
                100.0,
            ),
        ];
        assert_eq!(run(&both), base, "automated controls ignore live values");
        // A control no clip drives still takes the live value.
        let pan = [(Target::MixerPan { track: 0 }, 1.0)];
        near(run(&pan), [0.0, base[1]], "an undriven pan follows live");
    }

    #[test]
    fn a_live_param_moves_a_stopped_live_note() {
        use crate::automation::Target;
        let song = song_with(vec![[0.5; 2]; RATE as usize * 4], |_, _| {});
        let mut e = Engine::new(RATE);
        let mut block = vec![0.0f32; 64 * 2];
        e.live_on(&song, 1, 4, 60, 100);
        for _ in 0..150 {
            e.process_live(&song, &mut block);
        }
        near([block[126], block[127]], [0.5, 0.5], "live note");
        e.apply_live_param(Target::MixerVolume { track: 0 }, 0.5);
        e.apply_live_param(Target::ChannelPan { inst: 4 }, 1.0);
        for _ in 0..150 {
            e.process_live(&song, &mut block);
        }
        near(
            [block[126], block[127]],
            [0.0, 0.25],
            "live params while stopped",
        );
    }

    #[test]
    fn a_mixer_volume_step_ramps_smoothly_in_tiny_blocks() {
        use crate::automation::Target;
        let mut song = held_tone_song();
        // Master fader steps 1.0 -> 0.2 at bar 2.
        step_at_bar_two(&mut song, 7, Target::MixerVolume { track: 0 }, 0.2);
        let bar = bar_frames(&song);
        let out = render64(&song, true, 2 * bar);
        assert!(
            out[..bar].iter().all(|f| f[0] == 0.5 && f[1] == 0.5),
            "bar 1 untouched at unity"
        );
        // No single-sample click: every step is at most one one-pole move.
        let alpha = smooth_alpha(RATE as f64) as f32;
        let bound = 0.5 * 0.8 * alpha * 1.001;
        let max_delta = out[bar - 1..]
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_delta > 0.0, "the fader moved");
        assert!(max_delta <= bound, "click: {max_delta} > {bound}");
        // Settled to 0.5 * 0.2 well within 50 ms, exact shortly after.
        let settled = bar + (0.05 * RATE as f64) as usize;
        assert!(
            out[settled..settled + 64]
                .iter()
                .all(|f| (f[0] - 0.1).abs() < 1e-4),
            "not settled after 50 ms"
        );
        let exact = bar + (0.1 * RATE as f64) as usize;
        assert!(
            out[exact..exact + 64]
                .iter()
                .all(|f| (f[0] - 0.1).abs() < 1e-7),
            "never snapped to the target"
        );
    }

    #[test]
    fn an_fx_mix_step_ramps_smoothly_in_tiny_blocks() {
        use crate::automation::Target;
        let mut song = held_tone_song();
        // A silent wet path: full mix is silence, no mix is the 0.5 tone.
        let mut bal = crate::fx::FxSlot::new(crate::fx::FxKind::Balance);
        bal.params[0] = 0.0;
        song.mixer[0].fx.push(bal);
        step_at_bar_two(&mut song, 7, Target::FxMix { track: 0, slot: 0 }, 0.0);
        let bar = bar_frames(&song);
        let out = render64(&song, true, 2 * bar);
        assert!(
            out[..bar].iter().all(|f| f[0] == 0.0),
            "full wet: the silent balance"
        );
        let alpha = smooth_alpha(RATE as f64) as f32;
        let bound = 0.5 * alpha * 1.001;
        let max_delta = out[bar - 1..]
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_delta > 0.0, "the mix moved");
        assert!(max_delta <= bound, "discontinuity: {max_delta} > {bound}");
        let settled = bar + (0.05 * RATE as f64) as usize;
        assert!(
            out[settled..settled + 64]
                .iter()
                .all(|f| (f[0] - 0.5).abs() < 1e-4),
            "not dry after 50 ms"
        );
    }

    #[test]
    fn a_channel_volume_step_ramps_while_a_still_channel_stays_exact() {
        use crate::automation::Target;
        // A stationary channel at non-unity volume: 64-sample live blocks must
        // match the 512-sample export bit for bit.
        let mut still = held_tone_song();
        still
            .instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .volume = 0.7;
        let offline = render(&still, 1, false, RATE);
        // The export adds a silent tail block past the song; compare the song.
        let length = loop_frames(&still, 1, false, RATE) as usize;
        let live = render64(&still, false, length);
        assert_eq!(
            live,
            offline[..length],
            "a stationary channel stays bit-exact"
        );
        // The same channel automated: 1.0 -> 0.25 at bar 2.
        let mut moved = still.clone();
        moved
            .instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .volume = 1.0;
        step_at_bar_two(&mut moved, 7, Target::ChannelVolume { inst: 4 }, 0.25);
        let bar = bar_frames(&moved);
        let out = render64(&moved, true, 2 * bar);
        assert!(
            out[..bar].iter().all(|f| f[0] == 0.5),
            "bar 1 untouched at unity"
        );
        let alpha = smooth_alpha(RATE as f64) as f32;
        let bound = 0.5 * 0.75 * alpha * 1.001;
        let max_delta = out[bar - 1..]
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_delta > 0.0, "the volume moved");
        assert!(max_delta <= bound, "discontinuity: {max_delta} > {bound}");
        let settled = bar + (0.05 * RATE as f64) as usize;
        assert!(
            out[settled..settled + 64]
                .iter()
                .all(|f| (f[0] - 0.125).abs() < 1e-4),
            "not settled after 50 ms"
        );
    }

    #[test]
    fn automated_fx_params_glide_continuous_and_snap_discrete() {
        use crate::automation::Target;
        use crate::sequence::FX_SLOTS;
        // A 6 kHz tone: bright enough that closing the cutoff is audible.
        let bright: Vec<[f32; 2]> = (0..RATE as usize * 4)
            .map(|i| {
                let s = ((i as f64 * std::f64::consts::FRAC_PI_4).sin() * 0.5) as f32;
                [s; 2]
            })
            .collect();
        let mut song = song_with(bright, |_, _| {});
        song.instruments
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .mixer = 1;
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 2 * crate::sequence::BAR;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 32, 60));
        song.clips.push(crate::sequence::Clip::new(1, 0, 0));
        song.mixer[1]
            .fx
            .push(crate::fx::FxSlot::new(crate::fx::FxKind::Filter));
        song.mixer[2]
            .fx
            .push(crate::fx::FxSlot::new(crate::fx::FxKind::Delay));
        // Cutoff (continuous Hz) 10 kHz -> 200 Hz; delay time (Steps) 4 -> 1.
        for (id, target, from, to) in [
            (
                7u64,
                Target::FxParam {
                    track: 1,
                    slot: 0,
                    param: 1,
                },
                10000.0f32,
                200.0f32,
            ),
            (
                8,
                Target::FxParam {
                    track: 2,
                    slot: 0,
                    param: 0,
                },
                4.0,
                1.0,
            ),
        ] {
            let mut first = Pattern::new(id, "From");
            first.length = crate::sequence::BAR;
            first.automation = Some(crate::automation::Envelope::flat(
                target,
                from,
                crate::sequence::BAR,
            ));
            song.patterns.push(first.into());
            song.clips.push(crate::sequence::Clip::new(id, 0, 1));
            step_at_bar_two(&mut song, id + 2, target, to);
        }
        let bar = bar_frames(&song);
        let mut e = Engine::new(RATE);
        let mut buf = [0.0f32; 64 * 2];
        let mut pos = 0;
        let rms = |b: &[f32]| {
            (b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / b.len() as f64).sqrt()
        };
        let mut before = 0.0;
        for _ in 0..bar / 64 {
            pos = e.process(&song, 1, true, pos, &mut buf, true);
            before = rms(&buf);
        }
        let (fi, di) = (FX_SLOTS, 2 * FX_SLOTS);
        let cut = |e: &Engine| e.fx_param_state[fi].as_ref().unwrap().1[1];
        let time = |e: &Engine| e.fx_param_state[di].as_ref().unwrap().1[0];
        // The first automated block snaps to the envelope (no initial lag).
        assert_eq!(cut(&e), 10000.0);
        assert_eq!(time(&e), 4.0);
        // One 64-frame block into bar 2: the cutoff glides, the steps snap.
        pos = e.process(&song, 1, true, pos, &mut buf, true);
        let first = rms(&buf);
        let alpha_block = 1.0 - (-64.0f32 / (SMOOTH_TAU_SEC as f32 * RATE as f32)).exp();
        let want = 10000.0 + (200.0 - 10000.0) * alpha_block;
        assert!(
            (cut(&e) - want).abs() < 0.5,
            "one block of glide: {} vs {want}",
            cut(&e)
        );
        assert!(cut(&e) > 5000.0, "not halfway in one block");
        assert_eq!(time(&e), 1.0, "a Steps parameter snaps at once");
        assert!(
            first > before * 0.5,
            "the sound did not jump to the closed filter: {first} vs {before}"
        );
        // Well inside 100 ms the cutoff has settled exactly, and the sound with it.
        let mut late = 0.0;
        for _ in 0..(RATE as usize / 10 / 64) {
            pos = e.process(&song, 1, true, pos, &mut buf, true);
            late = rms(&buf);
        }
        assert_eq!(cut(&e), 200.0);
        assert_eq!(time(&e), 1.0);
        assert!(late < before * 0.1, "the cutoff closed: {late} vs {before}");
    }

    // ------------------------------------------------ DirectWave P2/P3: round robin, voice FX, program LFOs and sends

    fn sine_frames(seconds: f64, hz: f64) -> Vec<[f32; 2]> {
        (0..(RATE as f64 * seconds) as usize)
            .map(|i| {
                let v = (std::f64::consts::TAU * hz * i as f64 / RATE as f64).sin() as f32 * 0.5;
                [v, v]
            })
            .collect()
    }

    /// The Bass channel's preset with `n` full-range zones of DC `0.1 * (i + 1)`,
    /// all in trigger group 1 under `mode`, plus the program shell.
    fn round_robin_song(n: usize, mode: crate::sampler::TriggerMode) -> Song {
        let mut song = song_with(vec![[0.1; 2]; 64], |_, _| {});
        let mut preset = song
            .instrument(4)
            .unwrap()
            .sampler_preset()
            .unwrap()
            .clone();
        for i in 0..n {
            let mut z = preset.zones[0].clone();
            z.frames = Arc::new(vec![[0.1 * (i + 1) as f32; 2]; 64]);
            z.params.trigger_group = 1;
            z.params.trigger_mode = mode;
            if i == 0 {
                preset.zones[0] = z;
            } else {
                preset.zones.push(z);
            }
        }
        song.set_sampler_preset(4, preset).unwrap();
        song
    }

    /// Zones that start for note id `note`, in the order they were pushed.
    fn started_zones(e: &mut Engine, song: &Song, note: u64) -> Vec<usize> {
        let preset = song.instrument(4).unwrap().sampler_preset().unwrap();
        let before = e.voices.len();
        e.start_zones(preset, Voice::new(4, note, Source::Tone, 60, 100, 1.0));
        e.voices[before..]
            .iter()
            .map(|v| match v.source {
                Source::Zone { zone } => zone,
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn cycle_trigger_group_alternates_zones_in_order() {
        let song = round_robin_song(3, crate::sampler::TriggerMode::Cycle);
        let mut e = Engine::new(RATE);
        let picks: Vec<usize> = (1..=6)
            .map(|n| {
                let z = started_zones(&mut e, &song, n);
                assert_eq!(z.len(), 1, "one zone of the group plays");
                z[0]
            })
            .collect();
        assert_eq!(picks, [0, 1, 2, 0, 1, 2]);
        // A stop starts the cycle over.
        e.stop();
        assert_eq!(started_zones(&mut e, &song, 99), [0]);
    }

    #[test]
    fn avoid_previous_never_repeats_and_random_is_deterministic() {
        let song = round_robin_song(3, crate::sampler::TriggerMode::AvoidPrevious);
        let mut e = Engine::new(RATE);
        let picks: Vec<usize> = (1..=20)
            .map(|n| started_zones(&mut e, &song, n)[0])
            .collect();
        assert!(picks.windows(2).all(|w| w[0] != w[1]), "{picks:?}");
        assert!(
            (0..3).all(|z| picks.contains(&z)),
            "all zones get used: {picks:?}"
        );
        // One zone only: nothing to avoid, it plays every time.
        let one = round_robin_song(1, crate::sampler::TriggerMode::AvoidPrevious);
        let mut e1 = Engine::new(RATE);
        assert!((1..5).all(|n| started_zones(&mut e1, &one, n) == [0]));
        // Random: same notes, same picks, in a fresh engine.
        let song = round_robin_song(4, crate::sampler::TriggerMode::Random);
        let run = || {
            let mut e = Engine::new(RATE);
            (1..=16)
                .map(|n| started_zones(&mut e, &song, n)[0])
                .collect::<Vec<_>>()
        };
        let (a, b) = (run(), run());
        assert_eq!(a, b);
        assert!(a.iter().any(|z| *z != a[0]), "not constant: {a:?}");
    }

    #[test]
    fn zones_outside_a_group_or_in_normal_mode_always_play() {
        let mut song = round_robin_song(3, crate::sampler::TriggerMode::Cycle);
        let mut preset = song
            .instrument(4)
            .unwrap()
            .sampler_preset()
            .unwrap()
            .clone();
        preset.zones[2].params.trigger_group = 0; // always plays
        preset.zones[1].params.trigger_mode = crate::sampler::TriggerMode::Normal; // plays too
        song.set_sampler_preset(4, preset).unwrap();
        let mut e = Engine::new(RATE);
        // Zone 1 (Normal) and 2 (group 0) every note; zone 0 is alone in its group.
        for n in 1..=3 {
            let mut z = started_zones(&mut e, &song, n);
            z.sort();
            assert_eq!(z, [0, 1, 2]);
        }
    }

    #[test]
    fn voice_fx_change_the_sound_stay_bounded_and_off_means_identical() {
        use crate::sampler::VoiceFx;
        let tone = sine_frames(1.0, 440.0);
        let render_fx = |fx: VoiceFx| {
            let mut song = song_with(tone.clone(), |z, _| z.params.voice_fx = fx);
            std::sync::Arc::make_mut(&mut song.patterns[0])
                .notes
                .push(note(0, 8, 60));
            render(&song, 1, false, RATE)
        };
        let dry = render_fx(VoiceFx::default());
        // Every parameter moved, every mix off: bit-identical to the plain voice.
        let off = render_fx(VoiceFx {
            ringmod_rate: 1234.0,
            decimator_rate: 0.9,
            quantizer_bits: 3.0,
            phaser_freq: 7.0,
            ..VoiceFx::default()
        });
        assert_eq!(dry, off);
        let cases: [(&str, VoiceFx); 4] = [
            (
                "ringmod",
                VoiceFx {
                    ringmod_mix: 1.0,
                    ..VoiceFx::default()
                },
            ),
            (
                "decimator",
                VoiceFx {
                    decimator_mix: 1.0,
                    decimator_rate: 1.0,
                    ..VoiceFx::default()
                },
            ),
            (
                "quantizer",
                VoiceFx {
                    quantizer_mix: 1.0,
                    quantizer_bits: 2.0,
                    ..VoiceFx::default()
                },
            ),
            (
                "phaser",
                VoiceFx {
                    phaser_mix: 1.0,
                    phaser_freq: 2.0,
                    ..VoiceFx::default()
                },
            ),
        ];
        let window = 2000..RATE as usize / 2;
        for (name, fx) in cases {
            assert!(fx.active() && fx.validate().is_ok());
            let wet = render_fx(fx);
            assert_eq!(wet.len(), dry.len(), "{name}: same length");
            assert!(
                wet.iter()
                    .flatten()
                    .all(|v| v.is_finite() && v.abs() <= 1.0),
                "{name}: finite and bounded"
            );
            let diff: f64 = wet[window.clone()]
                .iter()
                .zip(&dry[window.clone()])
                .map(|(a, b)| ((a[0] - b[0]) as f64).powi(2))
                .sum();
            assert!(diff > 1.0, "{name}: audibly different, diff {diff}");
            assert!(energy(&wet[window.clone()]) > 1.0, "{name}: still sounds");
        }
    }

    #[test]
    fn decimator_holds_and_quantizer_snaps_to_levels() {
        use crate::sampler::VoiceFx;
        let tone = sine_frames(0.5, 440.0);
        let render_fx = |fx: VoiceFx| {
            let mut song = song_with(tone.clone(), |z, _| z.params.voice_fx = fx);
            std::sync::Arc::make_mut(&mut song.patterns[0])
                .notes
                .push(note(0, 4, 60));
            render(&song, 1, false, RATE)
        };
        // Full decimation: runs of 64 equal frames.
        let dec = render_fx(VoiceFx {
            decimator_mix: 1.0,
            decimator_rate: 1.0,
            ..VoiceFx::default()
        });
        let runs = dec[1000..3000].windows(2).filter(|w| w[0] == w[1]).count();
        assert!(runs > 1900, "held between captures: {runs}");
        // 2 bits: levels of 1/2 -> only multiples of 0.5 (the sine peaks at 0.5).
        let q = render_fx(VoiceFx {
            quantizer_mix: 1.0,
            quantizer_bits: 2.0,
            ..VoiceFx::default()
        });
        assert!(
            q[1000..3000]
                .iter()
                .all(|f| (f[0] * 2.0 - (f[0] * 2.0).round()).abs() < 1e-6)
        );
    }

    #[test]
    fn program_lfos_modulate_gain_and_pitch() {
        use crate::sampler::{ModSource as S, ModTarget as T};
        let dc = vec![[0.5f32; 2]; RATE as usize];
        let mut song = song_with(dc.clone(), |z, p| {
            p.lfo[0].rate = 8.0;
            z.params.route(S::ProgLfo1, T::Gain, -1.0);
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let out = render(&song, 1, false, RATE);
        let (lo, hi) = out[1000..RATE as usize / 2]
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s[0]), b.max(s[0])));
        assert!(hi - lo > 0.3, "program LFO 1 tremolo swings: {lo}..{hi}");
        // The zone's own LFO 1 is left alone: nothing routed from it, no swing.
        let mut plain = song_with(dc, |z, p| {
            p.lfo[0].rate = 8.0;
            z.params.route(S::Lfo1, T::Gain, -1.0);
            z.params.lfo[0].rate = 0.01;
        });
        std::sync::Arc::make_mut(&mut plain.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let flat = render(&plain, 1, false, RATE);
        let (lo2, hi2) = flat[1000..RATE as usize / 2]
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s[0]), b.max(s[0])));
        assert!(hi2 - lo2 < 0.05, "zone LFO is independent: {lo2}..{hi2}");
        // Program LFO 2 to pitch bends a ramp: differs from the unmodulated render.
        let ramp: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|i| [(i % 100) as f32 / 100.0 - 0.5; 2])
            .collect();
        let run = |route: bool| {
            let mut song = song_with(ramp.clone(), |z, p| {
                p.lfo[1].rate = 5.0;
                if route {
                    z.params.route(S::ProgLfo2, T::Pitch, 0.5);
                }
            });
            std::sync::Arc::make_mut(&mut song.patterns[0])
                .notes
                .push(note(0, 8, 60));
            render(&song, 1, false, RATE)
        };
        let (bent, straight) = (run(true), run(false));
        assert!(bent.iter().flatten().all(|v| v.is_finite()));
        assert_ne!(bent, straight);
    }

    #[test]
    fn program_fx_transform_and_ring_on_after_the_note() {
        use crate::sampler::ProgramFx;
        // A 0.1 s burst: the note (1 step = 0.125 s) and sample end before 0.2 s.
        let burst = sine_frames(0.1, 330.0);
        let render_fx = |fx: ProgramFx| {
            let mut song = song_with(burst.clone(), |_, p| p.fx = fx);
            std::sync::Arc::make_mut(&mut song.patterns[0])
                .notes
                .push(note(0, 1, 60));
            render(&song, 1, false, RATE)
        };
        let dry = render_fx(ProgramFx::default());
        let tail =
            |o: &[[f32; 2]]| energy(&o[RATE as usize * 6 / 10..o.len().min(RATE as usize * 2)]);
        assert!(tail(&dry) < 1e-9, "dry: silent after the note");
        let first = |o: &[[f32; 2]]| o[..RATE as usize].to_vec();
        let d = ProgramFx::default;
        for (name, fx) in [
            ("drive", ProgramFx { drive: 1.0, ..d() }),
            ("chorus", ProgramFx { chorus: 1.0, ..d() }),
            ("delay", ProgramFx { delay: 1.0, ..d() }),
            ("reverb", ProgramFx { reverb: 1.0, ..d() }),
        ] {
            assert!(fx.active() && fx.validate().is_ok());
            let wet = render_fx(fx);
            assert!(wet.iter().flatten().all(|v| v.is_finite()), "{name}");
            assert_ne!(first(&wet), first(&dry), "{name}: changes the sound");
            match name {
                "delay" | "reverb" => {
                    assert!(
                        tail(&wet) > 1e-4,
                        "{name}: rings after the note: {}",
                        tail(&wet)
                    );
                    assert!(
                        wet.len() > dry.len(),
                        "{name}: the export waits for the tail"
                    );
                }
                _ => {}
            }
        }
        // Left echoes after ~0.35 s, right after ~0.26 s.
        let echo = render_fx(ProgramFx { delay: 1.0, ..d() });
        let at = |ch: usize, a: f64, b: f64| -> f64 {
            echo[(a * RATE as f64) as usize..(b * RATE as f64) as usize]
                .iter()
                .map(|f| (f[ch] as f64).powi(2))
                .sum()
        };
        assert!(
            at(0, 0.36, 0.44) > 100.0 * at(0, 0.22, 0.34).max(1e-12),
            "left echo at 0.35 s"
        );
        assert!(
            at(1, 0.27, 0.35) > 100.0 * at(1, 0.15, 0.25).max(1e-12),
            "right echo at 0.26 s"
        );
    }

    #[test]
    fn program_fx_off_touches_nothing_and_is_block_size_independent() {
        use crate::sampler::ProgramFx;
        // Defaults: the sample comes out as is, and no effect entry is ever claimed.
        let mut song = song_with(vec![[0.5; 2]; 4000], |_, p| p.fx = ProgramFx::default());
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 8, 60));
        let mut e = Engine::new(RATE);
        let mut buf = vec![0.0f32; 512 * 2];
        e.process(&song, 1, false, 0, &mut buf, false);
        assert!(buf[..400].iter().all(|v| *v == 0.5), "plain pass-through");
        assert!(e.sampler_fx.iter().all(|c| c.inst.is_none() && !c.ringing));
        // Effects on: the result does not depend on the block size.
        let burst = sine_frames(0.2, 300.0);
        let mut song = song_with(burst, |_, p| {
            p.fx = ProgramFx {
                drive: 0.5,
                chorus: 0.6,
                delay: 0.5,
                reverb: 0.5,
            }
        });
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .push(note(0, 2, 60));
        let run = |block: usize| {
            let mut e = Engine::new(RATE);
            let mut out = Vec::new();
            let mut buf = vec![0.0f32; block * 2];
            let mut at = 0;
            while out.len() < RATE as usize * 2 {
                at = e.process(&song, 1, false, at, &mut buf, false);
                out.extend_from_slice(&buf);
            }
            out.truncate(RATE as usize * 2);
            out
        };
        assert_eq!(run(512), run(256));
        // Turning the effects off frees the entry once the channel is told so.
        let mut off = song.clone();
        let mut preset = off.instrument(4).unwrap().sampler_preset().unwrap().clone();
        preset.program.fx = ProgramFx::default();
        off.set_sampler_preset(4, preset).unwrap();
        let mut e = Engine::new(RATE);
        let mut buf = vec![0.0f32; 512 * 2];
        e.process(&song, 1, false, 0, &mut buf, false);
        assert!(e.sampler_fx.iter().any(|c| c.inst == Some(4)));
        e.process(&off, 1, false, 512, &mut buf, false);
        assert!(e.sampler_fx.iter().all(|c| c.inst.is_none()));
    }
}

#[cfg(test)]
mod mixer_tests {
    use crate::fx::{FxKind, FxSlot};
    use crate::sequence::{Note, STEP, Send, Song, render};

    fn peak(v: &[[f32; 2]]) -> f32 {
        v.iter()
            .map(|f| f[0].abs().max(f[1].abs()))
            .fold(0.0, f32::max)
    }

    fn kick() -> Song {
        let mut s = Song::default();
        s.add_note(1, Note::new(1, 0, STEP, 60, 100));
        s
    }

    #[test]
    fn channel_through_an_insert_follows_its_fader_and_mute() {
        let dry = render(&kick(), 1, false);
        let mut s = kick();
        s.instruments[0].mixer = 3;
        assert_eq!(render(&s, 1, false), dry, "unity insert changes nothing");
        s.mixer[3].volume = 0.5;
        let half = render(&s, 1, false);
        assert!((peak(&half) / peak(&dry) - 0.5).abs() < 0.02);
        s.mixer[3].muted = true;
        assert_eq!(peak(&render(&s, 1, false)), 0.0);
        s.mixer[3].muted = false;
        s.mixer[0].muted = true;
        assert_eq!(peak(&render(&s, 1, false)), 0.0, "master mute silences all");
    }

    #[test]
    fn taps_record_exactly_what_the_track_plays_and_silence_when_muted() {
        let mut s = kick();
        s.instruments[0].mixer = 2;
        s.mixer[2].volume = 0.5;
        s.mixer[4].muted = true;
        let mut e = super::Engine::new(48_000);
        e.reserve(s.mixer.len(), 512);
        let (tx, mut master) = rtrb::RingBuffer::new(48_000);
        e.add_tap(super::Tap {
            track: 0,
            tx,
            dropped: 0,
        })
        .ok()
        .unwrap();
        let (tx, mut muted) = rtrb::RingBuffer::new(48_000);
        e.add_tap(super::Tap {
            track: 4,
            tx,
            dropped: 0,
        })
        .ok()
        .unwrap();
        let mut out = vec![0.0f32; 1024];
        let mut heard = Vec::new();
        let mut at = 0;
        for _ in 0..20 {
            at = e.process(&s, 1, false, at, &mut out, true);
            heard.extend(out.as_chunks::<2>().0.iter().map(|f| [f[0], f[1]]));
        }
        let taken: Vec<[f32; 2]> = std::iter::from_fn(|| master.pop().ok()).collect();
        assert_eq!(
            taken, heard,
            "the Master tap is the output, frame for frame"
        );
        assert!(peak(&taken) > 0.0);
        let silent: Vec<[f32; 2]> = std::iter::from_fn(|| muted.pop().ok()).collect();
        assert_eq!(silent.len(), heard.len(), "a muted armed track keeps time");
        assert_eq!(peak(&silent), 0.0);
        while e.take_tap().is_some() {}
        e.process(&s, 1, false, at, &mut out, true);
        assert!(master.pop().is_err(), "stopped taps get nothing");
    }

    #[test]
    fn sep_narrows_to_mono_and_skips_at_zero() {
        let mut s = kick();
        s.instruments[0].pan = 0.8;
        s.instruments[0].mixer = 1;
        let dry = render(&s, 1, false);
        s.mixer[1].sep = 0.0;
        assert_eq!(render(&s, 1, false), dry);
        s.mixer[1].sep = -1.0;
        let mono = render(&s, 1, false);
        assert!(
            mono.iter().all(|f| (f[0] - f[1]).abs() < 1e-6),
            "sep -100% is mono"
        );
        assert!(
            dry.iter().any(|f| (f[0] - f[1]).abs() > 1e-3),
            "the dry kick was panned"
        );
        s.mixer[1].sep = 1.5;
        assert!(s.validate().is_err());
    }

    #[test]
    fn strip_eq_is_silent_when_flat_and_shapes_when_turned() {
        let dry = render(&kick(), 1, false);
        let mut s = kick();
        s.instruments[0].mixer = 2;
        s.mixer[2].eq = [0.0; 3];
        assert_eq!(render(&s, 1, false), dry, "a flat EQ is skipped");
        s.mixer[2].eq = [-18.0, 0.0, 0.0];
        s.validate().unwrap();
        let cut = render(&s, 1, false);
        assert!(
            peak(&cut) < peak(&dry) * 0.9,
            "a kick loses level when its lows are cut"
        );
        let text = ron::to_string(&s.mixer[2]).unwrap();
        assert!(
            ron::from_str::<crate::sequence::MixerTrack>(&text)
                .unwrap()
                .eq
                == [-18.0, 0.0, 0.0]
        );
        s.mixer[2].eq = [30.0, 0.0, 0.0];
        assert!(s.validate().is_err(), "EQ gains stay within ±18 dB");
    }

    #[test]
    fn a_send_back_to_an_earlier_insert_is_heard_the_same_block() {
        let dry = render(&kick(), 1, false);
        // Channel → insert 3 → (send) insert 2 → Master: insert 3 must mix first.
        let mut s = kick();
        s.instruments[0].mixer = 3;
        s.mixer[3].to_master = false;
        s.mixer[3].sends.push(Send { to: 2, level: 1.0 });
        s.validate().unwrap();
        let back = render(&s, 1, false);
        assert!(peak(&dry) > 0.0);
        assert!(
            (peak(&back) - peak(&dry)).abs() < 1e-4,
            "{} vs {}",
            peak(&back),
            peak(&dry)
        );
        // Same timing as the dry kick: no block of delay.
        let onset = |v: &[[f32; 2]]| v.iter().position(|f| f[0].abs() > 1e-4);
        assert_eq!(onset(&back), onset(&dry));
        // A longer chain backwards: 5 → 4 → 1 → Master.
        let mut s = kick();
        s.instruments[0].mixer = 5;
        for (from, to) in [(5usize, 4u16), (4, 1)] {
            s.mixer[from].to_master = false;
            s.mixer[from].sends.push(Send { to, level: 1.0 });
        }
        s.validate().unwrap();
        assert!((peak(&render(&s, 1, false)) - peak(&dry)).abs() < 1e-4);
    }

    #[test]
    fn sends_reach_a_later_insert_without_the_master_route() {
        let dry = render(&kick(), 1, false);
        let mut s = kick();
        s.instruments[0].mixer = 1;
        s.mixer[1].to_master = false;
        assert_eq!(peak(&render(&s, 1, false)), 0.0, "no route, no sound");
        s.mixer[1].sends.push(Send { to: 2, level: 1.0 });
        s.validate().unwrap();
        let sent = render(&s, 1, false);
        assert!((peak(&sent) - peak(&dry)).abs() < 1e-4);
        // A send backwards would make a loop: refused.
        s.mixer[2].sends.push(Send { to: 1, level: 1.0 });
        assert!(s.validate().is_err());
    }

    #[test]
    fn insert_effects_run_and_master_limiter_holds_the_ceiling() {
        let mut s = kick();
        s.instruments[0].volume = 1.25;
        let mut lim = FxSlot::new(FxKind::Limiter);
        lim.params[0] = 12.0;
        lim.params[1] = -12.0;
        s.mixer[0].fx.push(lim);
        s.validate().unwrap();
        let out = render(&s, 1, false);
        let ceil = 10f32.powf(-12.0 / 20.0);
        assert!(peak(&out) <= ceil + 1e-5, "peak {}", peak(&out));
        assert!(peak(&out) > ceil * 0.8);
        // A bypassed slot does nothing.
        s.mixer[0].fx[0].enabled = false;
        assert!(peak(&render(&s, 1, false)) > ceil * 1.1);
    }

    #[test]
    fn mixer_survives_save_and_load() {
        let mut s = kick();
        s.mixer[2].fx.push(FxSlot::new(FxKind::Reverb));
        s.mixer[2].name = "Drums".into();
        let back: Song = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.mixer, s.mixer);
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    use crate::fx::FxSlot;
    use crate::plugin::{Instance, PluginEvent, TestGain, TestSynth, share};
    use crate::sequence::{PluginRef, STEP};

    fn plugin_song() -> (Song, u64, u64) {
        let mut song = Song::default();
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        let r = PluginRef {
            id: 9,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: "Synth".into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        let inst = song.add_channel("Synth", InstrumentKind::Plugin { plugin: r });
        let pat = song.patterns[0].id;
        song.add_note(pat, Note::new(inst, STEP, 2 * STEP, 69, 127));
        (song, inst, pat)
    }

    /// Main pair 0.1, extra pairs 0.2, 0.3, ... (a multi-out drum sampler).
    struct TestMulti(usize);

    impl crate::plugin::Processor for TestMulti {
        fn process(&mut self, _: &[PluginEvent], io: &mut [f32], _: &crate::plugin::Transport) {
            io.fill(0.1);
        }
        fn extra_pairs(&self) -> usize {
            self.0
        }
        fn process_multi(
            &mut self,
            e: &[PluginEvent],
            io: &mut [f32],
            extra: &mut [f32],
            stride: usize,
            t: &crate::plugin::Transport,
        ) -> usize {
            self.process(e, io, t);
            for k in 0..self.0 {
                extra[k * stride..k * stride + io.len()].fill(0.2 + 0.1 * k as f32);
            }
            self.0
        }
    }

    /// A plugin that delays its input by `lag` frames and says so; with an
    /// `impulse` it adds one sample of that height at its very first frame.
    struct Lag {
        ring: std::collections::VecDeque<[f32; 2]>,
        lag: usize,
        impulse: Option<f32>,
    }

    impl Lag {
        fn new(lag: usize, impulse: Option<f32>) -> Self {
            Self {
                ring: std::iter::repeat_n([0.0; 2], lag).collect(),
                lag,
                impulse,
            }
        }
    }

    impl crate::plugin::Processor for Lag {
        fn process(&mut self, _: &[PluginEvent], io: &mut [f32], _: &crate::plugin::Transport) {
            for f in io.as_chunks_mut::<2>().0 {
                let mut x = [f[0], f[1]];
                if let Some(v) = self.impulse.take() {
                    x = [v, v];
                }
                self.ring.push_back(x);
                let y = self.ring.pop_front().unwrap();
                f[0] = y[0];
                f[1] = y[1];
            }
        }
        fn latency(&self, _: usize) -> usize {
            self.lag
        }
    }

    /// A steady level on both channels.
    struct Dc(f32);

    impl crate::plugin::Processor for Dc {
        fn process(&mut self, _: &[PluginEvent], io: &mut [f32], _: &crate::plugin::Transport) {
            io.fill(self.0);
        }
    }

    #[test]
    fn a_sidechained_compressor_ducks_under_a_later_insert() {
        let (mut song, inst, pat) = plugin_song();
        song.instruments.retain(|i| i.id != inst);
        let r = |id, name: &str| PluginRef {
            id,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: name.into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        let pad = song.add_channel(
            "Pad",
            InstrumentKind::Plugin {
                plugin: r(1, "Pad"),
            },
        );
        let kick = song.add_channel(
            "Kick",
            InstrumentKind::Plugin {
                plugin: r(2, "Kick"),
            },
        );
        for (id, t) in [(pad, 1), (kick, 3)] {
            let i = song.instruments.iter_mut().find(|i| i.id == id).unwrap();
            i.mixer = t;
            i.volume = 1.0;
            i.pan = 0.0;
        }
        // The kick keys the pad's compressor but stays out of Master.
        song.mixer[3].to_master = false;
        let mut comp = crate::fx::FxSlot::new(crate::fx::FxKind::Compressor);
        comp.params[0] = -10.0;
        comp.params[1] = 20.0;
        comp.params[2] = 0.1;
        song.mixer[1].fx.push(comp);
        let peak_of_pad = |song: &Song| {
            let mut e = Engine::new(48_000);
            e.adopt_plugin(Instance::new(1, share(Dc(0.2))));
            e.adopt_plugin(Instance::new(2, share(Dc(0.9))));
            render_blocks(&mut e, song, pat, 8);
            e.peaks()[1]
        };
        song.validate().unwrap();
        let alone = peak_of_pad(&song);
        assert!(
            (alone - 0.2).abs() < 1e-3,
            "under the threshold on its own: {alone}"
        );
        song.mixer[1].fx[0].sidechain = Some(3);
        song.validate().unwrap();
        let ducked = peak_of_pad(&song);
        assert!(
            ducked < 0.09,
            "the kick on insert 3 ducks the pad on insert 1: {ducked}"
        );
        // Inserting a track before 3 keeps the key on the kick's insert.
        song.insert_track(2);
        assert_eq!(song.mixer[1].fx[0].sidechain, Some(4));
        song.remove_track(4);
        assert_eq!(
            song.mixer[1].fx[0].sidechain, None,
            "a removed key clears the input"
        );
    }

    #[test]
    fn a_sidechain_that_closes_a_loop_fails_validation() {
        let mut song = Song::default();
        song.mixer[1]
            .sends
            .push(crate::sequence::Send { to: 2, level: 1.0 });
        let mut comp = crate::fx::FxSlot::new(crate::fx::FxKind::Compressor);
        comp.sidechain = Some(2);
        song.mixer[1].fx.push(comp);
        assert!(song.validate().is_err(), "1 -> 2 and 2 keys 1");
        song.mixer[1].sends.clear();
        assert!(song.validate().is_ok());
        song.mixer[1].fx[0].sidechain = Some(1);
        assert!(song.validate().is_err(), "an insert cannot key itself");
        let mut delay = crate::fx::FxSlot::new(crate::fx::FxKind::Delay);
        delay.sidechain = Some(2);
        song.mixer[1].fx = vec![delay];
        assert!(song.validate().is_err(), "a delay has no sidechain input");
    }

    fn render_blocks(e: &mut Engine, song: &Song, pat: u64, blocks: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; 256 * 2];
        let mut out = Vec::new();
        let mut at = 0;
        for _ in 0..blocks {
            at = e.process(song, pat, false, at, &mut buf, false);
            out.extend_from_slice(&buf);
        }
        out
    }

    #[test]
    fn built_in_instruments_wait_for_the_slowest_plugin_instrument() {
        let mut song = Song::demo();
        let pat = song.patterns[0].id;
        let plain = render_blocks(&mut Engine::new(48_000), &song, pat, 40);
        let r = PluginRef {
            id: 7,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: "Late".into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        song.add_channel("Late", InstrumentKind::Plugin { plugin: r });
        let mut e = Engine::new(48_000);
        e.adopt_plugin(Instance::new(7, share(Lag::new(300, None))));
        let late = render_blocks(&mut e, &song, pat, 40);
        assert_eq!(e.latency(), 300);
        assert!(plain.iter().any(|x| x.abs() > 0.05), "the demo plays");
        for i in 0..plain.len() - 600 {
            assert!(
                (late[i + 600] - plain[i]).abs() < 1e-6,
                "sample {i}: built-ins shifted by the plugin's 300 frames"
            );
        }
        assert!(late[..600].iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_slow_effects_dry_signal_waits_for_it_and_other_inserts_line_up() {
        let (mut song, inst, pat) = plugin_song();
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .notes
            .clear();
        // Two instruments that each make one click at their first frame:
        // A on insert 1 through a 100-frame effect at 50% mix, B on insert 2.
        let r = |id, name: &str| PluginRef {
            id,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: name.into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        song.instruments.retain(|i| i.id != inst);
        let a = song.add_channel("A", InstrumentKind::Plugin { plugin: r(1, "A") });
        let b = song.add_channel("B", InstrumentKind::Plugin { plugin: r(2, "B") });
        for (id, t) in [(a, 1), (b, 2)] {
            song.instruments
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .mixer = t;
        }
        let mut slot = crate::fx::FxSlot::plugin(r(3, "Delay"));
        slot.mix = 0.5;
        song.mixer[1].fx.push(slot);
        song.validate().unwrap();
        let mut e = Engine::new(48_000);
        e.adopt_plugin(Instance::new(1, share(Lag::new(0, Some(0.25)))));
        e.adopt_plugin(Instance::new(2, share(Lag::new(0, Some(0.25)))));
        e.adopt_plugin(Instance::new(3, share(Lag::new(100, None))));
        let out = render_blocks(&mut e, &song, pat, 4);
        let hits: Vec<(usize, f32)> = out
            .chunks(2)
            .enumerate()
            .filter(|(_, f)| f[0].abs() > 1e-6)
            .map(|(i, f)| (i, f[0]))
            .collect();
        // Without PDC: A's dry half at 0, its wet half at 100, B at 0.
        assert_eq!(hits.len(), 1, "one click, everything lined up: {hits:?}");
        assert_eq!(hits[0].0, 100);
        assert!(
            (hits[0].1 - 0.5).abs() < 1e-5,
            "A (dry + wet halves) plus B: {}",
            hits[0].1
        );
    }

    /// A sidechain-only plugin effect: plays its key, silence without one.
    struct KeyEcho;
    impl crate::plugin::Processor for KeyEcho {
        fn process(
            &mut self,
            _: &[crate::plugin::PluginEvent],
            io: &mut [f32],
            _: &crate::plugin::Transport,
        ) {
            io.fill(0.0);
        }
        fn process_keyed(
            &mut self,
            _: &[crate::plugin::PluginEvent],
            io: &mut [f32],
            key: &[f32],
            _: &crate::plugin::Transport,
        ) {
            io.copy_from_slice(key);
        }
    }

    #[test]
    fn a_plugin_effect_hears_another_insert_on_its_sidechain_input() {
        let (mut song, inst, pat) = plugin_song();
        song.instruments.retain(|i| i.id != inst);
        let r = |id, name: &str| PluginRef {
            id,
            format: "vst3".into(),
            path: "/x.vst3".into(),
            name: name.into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        let pad = song.add_channel(
            "Pad",
            InstrumentKind::Plugin {
                plugin: r(1, "Pad"),
            },
        );
        let kick = song.add_channel(
            "Kick",
            InstrumentKind::Plugin {
                plugin: r(2, "Kick"),
            },
        );
        for (id, t) in [(pad, 1), (kick, 3)] {
            let i = song.instruments.iter_mut().find(|i| i.id == id).unwrap();
            i.mixer = t;
            i.volume = 1.0;
            i.pan = 0.0;
        }
        song.mixer[3].to_master = false;
        song.mixer[1]
            .fx
            .push(crate::fx::FxSlot::plugin(r(3, "Gate")));
        let peak_of_pad = |song: &Song| {
            let mut e = Engine::new(48_000);
            e.adopt_plugin(Instance::new(1, share(Dc(0.2))));
            e.adopt_plugin(Instance::new(2, share(Dc(0.7))));
            e.adopt_plugin(Instance::new(3, share(KeyEcho)));
            render_blocks(&mut e, song, pat, 4);
            e.peaks()[1]
        };
        song.validate().unwrap();
        assert_eq!(peak_of_pad(&song), 0.0, "no key: the effect hears nothing");
        song.mixer[1].fx[0].sidechain = Some(3);
        song.validate().expect("a plugin slot takes a sidechain");
        let keyed = peak_of_pad(&song);
        assert!(
            (keyed - 0.7).abs() < 1e-3,
            "insert 3 (mixed later in index order) reached the plugin: {keyed}"
        );
    }

    #[test]
    fn multi_out_pairs_reach_their_mixer_tracks_and_the_rest_joins_the_main_one() {
        let (mut song, inst, pat) = plugin_song();
        let i = song.instruments.iter_mut().find(|i| i.id == inst).unwrap();
        i.mixer = 1;
        if let InstrumentKind::Plugin { plugin } = &mut i.kind {
            plugin.outs = vec![3, 0];
        }
        song.validate().unwrap();
        let mut e = Engine::new(48_000);
        e.adopt_plugin(Instance::new(9, share(TestMulti(3))));
        let mut buf = vec![0.0f32; 256 * 2];
        e.process(&song, pat, false, 0, &mut buf, true);
        let p = e.peaks();
        assert!((p[3] - 0.2).abs() < 1e-4, "pair 3-4 on insert 3: {}", p[3]);
        // Main 0.1, the unrouted (0) and the unlisted pair 0.3 + 0.4 stay on the channel's insert.
        assert!((p[1] - 0.8).abs() < 1e-4, "the rest on insert 1: {}", p[1]);
        // Inserting a track before insert 3 keeps the route on the same track.
        song.insert_track(2);
        assert_eq!(
            song.instrument(inst).unwrap().plugin().unwrap().outs,
            vec![4, 0]
        );
        song.remove_track(4);
        assert_eq!(
            song.instrument(inst).unwrap().plugin().unwrap().outs,
            vec![0, 0],
            "a removed track's pair goes back to the channel"
        );
    }

    #[test]
    fn plugin_notes_arrive_on_their_frame_and_end_after_their_length() {
        let (song, _, pat) = plugin_song();
        let synth = TestSynth::default();
        let seen = synth.seen.clone();
        let mut e = Engine::new(48_000);
        assert!(e.adopt_plugin(Instance::new(9, share(synth))).is_none());
        let mut buf = vec![0.0f32; 512 * 2];
        let mut at = 0;
        let mut peak = 0.0f32;
        for _ in 0..40 {
            at = e.process(&song, pat, false, at, &mut buf, true);
            peak = peak.max(buf.iter().fold(0.0, |m, x| m.max(x.abs())));
        }
        let sps = crate::sequence::seconds_per_step(song.tempo) * 48_000.0;
        let ev = seen.lock().unwrap().clone();
        let on = ev
            .iter()
            .position(|e| matches!(e, PluginEvent::NoteOn { pitch: 69, .. }))
            .expect("note on");
        let off = ev
            .iter()
            .position(|e| matches!(e, PluginEvent::NoteOff { pitch: 69, .. }))
            .expect("note off");
        assert!(on < off);
        // Note on at step 1 = frame sps: block sps / 512, offset sps % 512.
        assert_eq!(ev[on].at() as u64, (sps.ceil() as u64) % 512);
        assert!(peak > 0.1, "the plugin's sound reaches the output: {peak}");
    }

    #[test]
    fn live_notes_hold_until_released_and_stop_ends_sequenced_notes() {
        let (song, inst, _) = plugin_song();
        let synth = TestSynth::default();
        let seen = synth.seen.clone();
        let mut e = Engine::new(48_000);
        e.adopt_plugin(Instance::new(9, share(synth)));
        let mut buf = vec![0.0f32; 256 * 2];
        e.live_on(&song, 1, inst, 60, 100);
        assert!(e.busy());
        for _ in 0..4 {
            e.process_live(&song, &mut buf);
        }
        assert!(
            buf.iter().any(|x| x.abs() > 0.01),
            "held live note sounds while stopped"
        );
        e.live_off(1);
        e.process_live(&song, &mut buf);
        let ev = seen.lock().unwrap().clone();
        assert_eq!(
            ev.first(),
            Some(&PluginEvent::NoteOn {
                at: 0,
                pitch: 60,
                velocity: 100
            })
        );
        assert_eq!(ev.last(), Some(&PluginEvent::NoteOff { at: 0, pitch: 60 }));
        assert!(e.take_plugin(9).is_some());
        assert!(e.take_plugin(9).is_none());
    }

    #[test]
    fn native_fl_pitch_follows_successive_slides_and_resets_on_the_next_note() {
        let (mut song, inst, pat) = plugin_song();
        let i = song.instruments.iter_mut().find(|i| i.id == inst).unwrap();
        let InstrumentKind::Plugin { plugin } = &mut i.kind else {
            unreachable!()
        };
        plugin.format = "fl-native".into();
        std::sync::Arc::make_mut(&mut song.patterns[0]).slides =
            [(STEP + STEP / 3, STEP / 3, 71), (STEP + STEP, STEP / 3, 68)]
                .map(|(start, length, target)| crate::sequence::Glide {
                    start,
                    length,
                    target,
                    instrument: inst,
                    group: 0,
                })
                .to_vec();
        song.add_note(pat, Note::new(inst, 3 * STEP, STEP, 72, 100));
        let synth = TestSynth::default();
        let seen = synth.seen.clone();
        let mut engine = Engine::new(48_000);
        engine.adopt_plugin(Instance::new(9, share(synth)));
        let mut buf = [0.0; 512 * 2];
        let mut pos = 0;
        for _ in 0..40 {
            pos = engine.process(&song, pat, false, pos, &mut buf, false);
        }
        let events = seen.lock().unwrap();
        let pitch: Vec<_> = events
            .iter()
            .filter_map(|e| match *e {
                PluginEvent::Midi {
                    bytes: [crate::plugin::FL_PITCH, lsb, msb],
                    ..
                } => Some(i32::from(msb) * 128 + i32::from(lsb) - 8192),
                _ => None,
            })
            .collect();
        let first = pitch.iter().position(|&c| c == 200).unwrap();
        let second = pitch.iter().position(|&c| c == -100).unwrap();
        assert!(first < second);
        assert!(pitch[..first].iter().any(|c| (0..200).contains(c)));
        assert_eq!(pitch.last(), Some(&0));
    }

    #[test]
    fn effect_plugin_runs_in_its_slot_with_the_slot_mix() {
        let mut song = Song::demo();
        let pat = song.patterns[0].id;
        let r = PluginRef {
            id: 4,
            format: "vst3".into(),
            path: "/fx.vst3".into(),
            name: "Half".into(),
            vendor: String::new(),
            state: String::new(),
            outs: vec![],
        };
        let render = |song: &Song| {
            let mut e = Engine::new(48_000);
            e.adopt_plugin(Instance::new(4, share(TestGain(0.5))));
            let mut buf = vec![0.0f32; 512 * 2];
            let mut out = Vec::new();
            let mut at = 0;
            for _ in 0..200 {
                at = e.process(song, pat, false, at, &mut buf, true);
                out.extend_from_slice(&buf);
            }
            out.iter().fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let dry = render(&song);
        assert!(dry > 0.05, "the demo channels play: {dry}");
        song.mixer[0].fx.push(FxSlot::plugin(r));
        song.validate().unwrap();
        let half = render(&song);
        assert!(
            (half / dry - 0.5).abs() < 0.01,
            "plugin gain on Master: {dry} → {half}"
        );
        song.mixer[0].fx[0].mix = 0.5;
        let blend = render(&song);
        assert!(
            (blend / dry - 0.75).abs() < 0.01,
            "slot mix blends dry and wet: {blend}"
        );
    }
}
