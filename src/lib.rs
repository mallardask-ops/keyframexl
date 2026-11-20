//! Offline reference renderer for validating timing and slide behaviour.
use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug)]
pub struct Slide {
    pub start: f64,
    pub duration: f64,
    pub target_semitones: f64,
}

/// Pitch offset for an already active voice. Times are in beats.
pub fn pitch_at(beat: f64, slide: Slide) -> f64 {
    if beat <= slide.start {
        return 0.0;
    }
    if slide.duration <= 0.0 {
        return slide.target_semitones;
    }
    ((beat - slide.start) / slide.duration).clamp(0.0, 1.0) * slide.target_semitones
}

/// Two bars of a deterministic, stereo demonstration at 120 BPM.
pub fn demo() -> Vec<[f32; 2]> {
    let frames = SAMPLE_RATE as usize * 4;
    let mut result = Vec::with_capacity(frames);
    let mut phase = 0.0_f64;
    let slide = Slide {
        start: 1.0,
        duration: 1.0,
        target_semitones: 12.0,
    };
    for frame in 0..frames {
        let seconds = frame as f64 / SAMPLE_RATE as f64;
        let beat = seconds * 2.0;
        let bar_beat = beat % 4.0;
        let pitch = pitch_at(bar_beat, slide);
        phase = (phase + 130.812_782_65 * 2.0_f64.powf(pitch / 12.0) / SAMPLE_RATE as f64) % 1.0;
        let attack = (bar_beat * 16.0).clamp(0.0, 1.0);
        let release = ((4.0 - bar_beat) * 8.0).clamp(0.0, 1.0);
        let bass = (TAU as f64 * phase).sin() * 0.16 * attack * release;
        let kick_t = seconds % 0.5;
        let kick = if kick_t < 0.23 {
            let env = (-kick_t * 23.0).exp();
            let kick_phase =
                TAU as f64 * (45.0 * kick_t + 80.0 * (1.0 - (-kick_t * 22.0).exp()) / 22.0);
            kick_phase.sin() * env * 0.35
        } else {
            0.0
        };
        let sample = (bass + kick).clamp(-1.0, 1.0) as f32;
        result.push([sample, sample]);
    }
    result
}

pub fn write_wav(
    path: impl AsRef<std::path::Path>,
    audio: &[[f32; 2]],
) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for frame in audio {
        for sample in frame {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
    }
    writer.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slide_reaches_and_holds_target() {
        let s = Slide {
            start: 1.0,
            duration: 1.0,
            target_semitones: 12.0,
        };
        assert_eq!(pitch_at(0.5, s), 0.0);
        assert_eq!(pitch_at(1.5, s), 6.0);
        assert_eq!(pitch_at(2.0, s), 12.0);
        assert_eq!(pitch_at(3.0, s), 12.0);
    }
    #[test]
    fn demo_is_stereo_finite_and_four_seconds() {
        let frames = demo();
        assert_eq!(frames.len(), SAMPLE_RATE as usize * 4);
        assert!(
            frames
                .iter()
                .all(|f| f[0].is_finite() && f[0].abs() <= 1.0 && f[0] == f[1])
        );
        assert!(frames.iter().any(|f| f[0].abs() > 0.1));
    }
}

/// Stable identifiers are scoped to a project; patterns own their event data.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub instrument: u64,
    pub start_ticks: u64,
    pub length_ticks: u64,
    pub pitch: u8,
    pub velocity: u8,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub id: u64,
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub pattern_id: u64,
    pub start_ticks: u64,
}

#[derive(Default, Debug)]
pub struct Project {
    pub patterns: Vec<Pattern>,
    pub clips: Vec<Clip>,
}

impl Project {
    /// Copy a referenced pattern and point only the selected clip at it.
    pub fn make_unique(&mut self, clip_index: usize, new_id: u64) -> Result<(), &'static str> {
        if self.patterns.iter().any(|p| p.id == new_id) {
            return Err("pattern ID already exists");
        }
        let old_id = self
            .clips
            .get(clip_index)
            .ok_or("clip not found")?
            .pattern_id;
        let mut copy = self
            .patterns
            .iter()
            .find(|p| p.id == old_id)
            .ok_or("pattern not found")?
            .clone();
        copy.id = new_id;
        self.patterns.push(copy);
        self.clips[clip_index].pattern_id = new_id;
        Ok(())
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;
    #[test]
    fn make_unique_preserves_original_and_note_details() {
        let note = Note {
            instrument: 42,
            start_ticks: 7,
            length_ticks: 103,
            pitch: 48,
            velocity: 91,
            muted: false,
        };
        let mut project = Project {
            patterns: vec![Pattern {
                id: 1,
                notes: vec![note.clone()],
            }],
            clips: vec![
                Clip {
                    pattern_id: 1,
                    start_ticks: 0,
                },
                Clip {
                    pattern_id: 1,
                    start_ticks: 384,
                },
            ],
        };
        project.make_unique(1, 2).unwrap();
        project.patterns[1].notes[0].pitch = 60;
        assert_eq!(project.clips[0].pattern_id, 1);
        assert_eq!(project.patterns[0].notes[0], note);
        assert_eq!(project.clips[1].pattern_id, 2);
        assert_eq!(project.patterns[1].notes[0].start_ticks, 7);
        assert!(project.make_unique(0, 2).is_err());
    }
}

pub mod automation;
pub mod bundle;
pub mod denormal;
pub mod engine;
pub mod flp;
mod flp_fx;
pub mod forge;
pub mod fx;
pub mod kits;
pub mod legacy;
pub mod link;
pub mod midi;
pub mod mp3;
pub mod note_ops;
pub mod plugin;
pub mod preset;
pub mod project_io;
pub mod render;
pub mod riff;
pub mod sampler;
pub mod scale;
pub mod sequence;
pub mod step_ops;
pub mod synth;
mod synth_presets;
pub mod tempo;
pub mod vocal;
