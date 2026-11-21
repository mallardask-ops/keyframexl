//! Arm and record (FL: the arm button on a mixer strip + Record). While
//! Record is on and the transport plays, every armed mixer track's post-fader
//! sound streams from the audio thread over an `rtrb` ring; when either stops,
//! each take is written to a WAV in the recordings folder. With the audio
//! input armed, the input device (microphone, line in) is recorded alongside.

use super::{App, Tone};
use crate::audio::TapCmd;
use crate::i18n::fill;
use keyframexl::engine::{MAX_TAPS, Tap};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

/// Ring size per armed track: 4 s at 48 kHz, drained every UI frame.
const TAP_FRAMES: usize = 96_000 * 4;

/// `TakePart::track` of the audio input.
pub const INPUT: usize = usize::MAX;

/// One armed track (or the input) being recorded.
pub struct TakePart {
    pub track: usize,
    pub rx: rtrb::Consumer<[f32; 2]>,
    pub frames: Vec<[f32; 2]>,
    /// Sample rate the frames came at.
    pub rate: u32,
}

pub struct Recorder {
    /// Record button (FL: the record switch beside Play).
    pub on: bool,
    /// Armed mixer tracks, by index.
    pub armed: Vec<usize>,
    /// Takes in progress.
    pub parts: Vec<TakePart>,
    /// Record the audio input device too (FL: a mixer track's input).
    pub input: bool,
    /// The open input stream while recording it.
    pub input_stream: Option<cpal::Stream>,
    /// Record live parameter tweaks as automation clips while Record + Play are active.
    pub automation: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            on: false,
            armed: Vec::new(),
            parts: Vec::new(),
            input: false,
            input_stream: None,
            automation: true,
        }
    }
}

/// Open the input device for recording: frames go into the returned ring.
fn open_input(
    name: Option<&str>,
) -> Result<(cpal::Stream, rtrb::Consumer<[f32; 2]>, u32, String), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let named = name.and_then(|want| {
        host.input_devices()
            .ok()?
            .find(|d| d.name().ok().as_deref() == Some(want))
    });
    let device = match named {
        Some(d) => d,
        None => host.default_input_device().ok_or("No audio input")?,
    };
    let dev_name = device.name().unwrap_or_else(|_| "Input".into());
    let def = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = def.sample_rate().0;
    let range = device
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .filter(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.channels() <= 2
                && c.min_sample_rate().0 <= rate
                && c.max_sample_rate().0 >= rate
        })
        .max_by_key(|c| c.channels())
        .ok_or_else(|| format!("{dev_name} offers no float input"))?;
    let config = range.with_sample_rate(cpal::SampleRate(rate)).config();
    let ch = config.channels as usize;
    let (mut tx, rx) = rtrb::RingBuffer::<[f32; 2]>::new(rate as usize * 8);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                for f in data.chunks_exact(ch) {
                    let frame = if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] };
                    let _ = tx.push(frame);
                }
            },
            |e| eprintln!("Input error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rx, rate, dev_name))
}

/// Set in capture runs: takes go to a scratch folder, not the user's music.
pub static CAPTURE_RECORDINGS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn recordings_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("KEYFRAMEXL_RECORDINGS") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = CAPTURE_RECORDINGS.get() {
        return dir.clone();
    }
    dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("KeyFrameXL")
        .join("Recordings")
}

impl App {
    pub(crate) fn toggle_arm(&mut self, track: usize) {
        let tr = self.tr();
        if let Some(i) = self.rec.armed.iter().position(|t| *t == track) {
            self.rec.armed.remove(i);
            self.say(
                Tone::Quiet,
                fill(tr.rec_disarmed, &[("track", &self.mixer_name(track))]),
            );
        } else if self.rec.armed.len() >= MAX_TAPS {
            self.say(
                Tone::Bad,
                fill(tr.rec_max_taps, &[("n", &MAX_TAPS.to_string())]),
            );
        } else {
            self.rec.armed.push(track);
            let next = if self.rec.on {
                tr.rec_press_play
            } else {
                tr.rec_turn_on
            };
            self.say(
                Tone::Quiet,
                fill(
                    tr.rec_armed,
                    &[("track", &self.mixer_name(track)), ("next", next)],
                ),
            );
        }
    }

    pub(crate) fn is_armed(&self, track: usize) -> bool {
        self.rec.armed.contains(&track)
    }

    pub(crate) fn toggle_record(&mut self) {
        let tr = self.tr();
        self.rec.on = !self.rec.on;
        let audio = self.rec.armed.len() + self.rec.input as usize;
        let midi = if self.midi_in.as_ref().is_some_and(|m| m.connected() > 0) {
            tr.rec_midi_note
        } else {
            ""
        };
        let auto = if self.rec.automation {
            tr.rec_auto_note
        } else {
            ""
        };
        let text = match (self.rec.on, audio) {
            (false, _) => tr.rec_off.to_string(),
            (true, 0) => fill(tr.rec_on_none, &[("auto", auto), ("midi", midi)]),
            (true, n) => tr
                .rec_on_n
                .fmt_with(tr.lang, n, &[("auto", auto), ("midi", midi)]),
        };
        self.say(Tone::Quiet, text);
    }

    pub(crate) fn toggle_record_automation(&mut self) {
        let tr = self.tr();
        self.rec.automation = !self.rec.automation;
        let text = if self.rec.automation {
            tr.rec_auto_on
        } else {
            tr.rec_auto_off
        };
        self.say(Tone::Quiet, text);
    }

    /// Arm / disarm the audio input (microphone, line in).
    pub(crate) fn toggle_input(&mut self) {
        let tr = self.tr();
        self.rec.input = !self.rec.input;
        let dev = self
            .settings
            .audio
            .input
            .clone()
            .unwrap_or_else(|| tr.rec_default_input.into());
        let text = if self.rec.input {
            fill(tr.rec_input_armed, &[("dev", &dev)])
        } else {
            tr.rec_input_disarmed.into()
        };
        self.say(Tone::Quiet, text);
    }

    /// Once per UI frame: start, feed or finish takes.
    pub(crate) fn record_frame(&mut self) {
        let tr = self.tr();
        let Some(audio) = &self.audio else { return };
        let active = self.rec.on
            && (!self.rec.armed.is_empty() || self.rec.input)
            && audio.playing.load(Ordering::Relaxed);
        if active && self.rec.parts.is_empty() {
            let rate = audio.running.rate;
            let Ok(mut taps) = audio.taps.lock() else {
                return;
            };
            for &track in &self.rec.armed {
                let (tx, rx) = rtrb::RingBuffer::new(TAP_FRAMES);
                if taps
                    .0
                    .push(TapCmd::Add(Tap {
                        track,
                        tx,
                        dropped: 0,
                    }))
                    .is_ok()
                {
                    self.rec.parts.push(TakePart {
                        track,
                        rx,
                        frames: Vec::new(),
                        rate,
                    });
                }
            }
            drop(taps);
            if self.rec.input {
                match open_input(self.settings.audio.input.as_deref()) {
                    Ok((stream, rx, rate, _)) => {
                        self.rec.input_stream = Some(stream);
                        self.rec.parts.push(TakePart {
                            track: INPUT,
                            rx,
                            frames: Vec::new(),
                            rate,
                        });
                    }
                    Err(e) => {
                        self.rec.input = false;
                        self.say(
                            Tone::Bad,
                            fill(tr.rec_input_err, &[("err", &e.to_string())]),
                        );
                    }
                }
            }
        }
        for part in &mut self.rec.parts {
            while let Ok(f) = part.rx.pop() {
                part.frames.push(f);
            }
        }
        if !active && !self.rec.parts.is_empty() {
            self.finish_takes();
        }
    }

    /// Stop the taps, collect the last frames and write one WAV per track.
    pub(crate) fn finish_takes(&mut self) {
        let tr = self.tr();
        let mut dropped = 0;
        if let Some(audio) = &self.audio
            && let Ok(mut taps) = audio.taps.lock()
        {
            let _ = taps.0.push(TapCmd::StopAll);
            // The audio thread hands taps back on its next block.
            let until = std::time::Instant::now() + std::time::Duration::from_millis(200);
            let mut back = 0;
            let taps_out = self.rec.parts.iter().filter(|p| p.track != INPUT).count();
            while back < taps_out && std::time::Instant::now() < until {
                while let Ok(tap) = taps.1.pop() {
                    dropped += tap.dropped;
                    back += 1;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        // The input stops first, so its last frames are in the ring.
        self.rec.input_stream = None;
        let mut parts = std::mem::take(&mut self.rec.parts);
        for part in &mut parts {
            while let Ok(f) = part.rx.pop() {
                part.frames.push(f);
            }
        }
        let names: Vec<String> = parts
            .iter()
            .map(|p| {
                if p.track == INPUT {
                    "Input".to_string()
                } else {
                    self.mixer_name(p.track)
                }
            })
            .collect();
        match write_takes(&recordings_dir(), &self.project_name(), &parts, &names) {
            Ok(files) if files.is_empty() => self.say(Tone::Quiet, tr.rec_nothing),
            Ok(files) => {
                let secs = parts
                    .iter()
                    .map(|p| p.frames.len() as f64 / p.rate.max(1) as f64)
                    .fold(0.0, f64::max);
                self.files.add_root(recordings_dir());
                self.files.refresh();
                let warn = if dropped > 0 {
                    tr.rec_frames_lost.fmt(tr.lang, dropped as usize)
                } else {
                    String::new()
                };
                let what = if files.len() == 1 {
                    files[0]
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    tr.rec_takes.fmt(tr.lang, files.len())
                };
                self.say(
                    if dropped > 0 { Tone::Bad } else { Tone::Done },
                    fill(
                        tr.rec_saved,
                        &[
                            ("what", &what),
                            ("secs", &format!("{secs:.1}")),
                            ("warn", &warn),
                        ],
                    ),
                );
            }
            Err(e) => self.say(Tone::Bad, fill(tr.rec_take_err, &[("err", &e.to_string())])),
        }
    }
}

/// Write each non-empty take as `<project> - <track> - take N.wav`; returns the files.
pub fn write_takes(
    dir: &std::path::Path,
    project: &str,
    parts: &[TakePart],
    names: &[String],
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let clean = |s: &str| {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    let mut out = Vec::new();
    for (part, name) in parts.iter().zip(names) {
        if part.frames.is_empty() {
            continue;
        }
        let base = format!("{} - {}", clean(project), clean(name));
        let path = (1..)
            .map(|n| dir.join(format!("{base} - take {n}.wav")))
            .find(|p| !p.exists())
            .expect("a free name");
        // 24-bit at the output's rate: what the speakers got, without rounding it down.
        keyframexl::render::write_wav_at(
            &path,
            &part.frames,
            keyframexl::render::Format::Wav24,
            false,
            part.rate,
        )?;
        out.push(path);
    }
    Ok(out)
}
