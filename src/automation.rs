//! Automation clips (FL): an envelope over time that moves one control while
//! the song plays. An automation clip is a pattern with an envelope instead of
//! notes, placed in the playlist like any clip.

use crate::sequence::{Song, Tick};
use serde::{Deserialize, Serialize};

/// The control an envelope drives.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    MixerVolume {
        track: u16,
    },
    MixerPan {
        track: u16,
    },
    ChannelVolume {
        inst: u64,
    },
    ChannelPan {
        inst: u64,
    },
    /// A built-in effect's parameter: mixer track, slot, parameter index.
    FxParam {
        track: u16,
        slot: u8,
        param: u16,
    },
    FxMix {
        track: u16,
        slot: u8,
    },
    /// A hosted plugin's parameter (0..1, the plugin's own scale).
    PluginParam {
        plugin: u64,
        param: u32,
    },
}

/// The highest mixer fader gain.
pub const VOLUME_MAX: f32 = crate::sequence::FADER_MAX;
/// The highest channel volume (same as a fader).
pub const CHANNEL_MAX: f32 = crate::sequence::FADER_MAX;

impl Target {
    /// Volume moves along the fader taper (`sequence::volume_gain`), so an
    /// envelope or a MIDI fader halfway up sounds halfway; the rest is linear.
    pub fn tapered(&self) -> bool {
        matches!(
            self,
            Target::MixerVolume { .. } | Target::ChannelVolume { .. }
        )
    }

    /// `v` as a fraction 0..1 of the control's travel, given its `range`.
    pub fn to_unit(&self, v: f32, lo: f32, hi: f32) -> f32 {
        if self.tapered() {
            crate::sequence::volume_pos(v)
        } else {
            ((v - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0)
        }
    }

    /// The value at fraction `u` of the control's travel (inverse of `to_unit`).
    pub fn from_unit(&self, u: f32, lo: f32, hi: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        if self.tapered() {
            crate::sequence::volume_gain(u).min(hi)
        } else {
            lo + (hi - lo) * u
        }
    }

    /// The value range in the control's own units.
    pub fn range(&self, song: &Song) -> (f32, f32) {
        match *self {
            Target::MixerVolume { .. } => (0.0, VOLUME_MAX),
            Target::ChannelVolume { .. } => (0.0, CHANNEL_MAX),
            Target::MixerPan { .. } | Target::ChannelPan { .. } => (-1.0, 1.0),
            Target::FxParam { track, slot, param } => song
                .mixer
                .get(track as usize)
                .and_then(|m| m.fx.get(slot as usize))
                .and_then(|s| s.kind.params().get(param as usize))
                .map(|p| (p.min, p.max))
                .unwrap_or((0.0, 1.0)),
            Target::FxMix { .. } | Target::PluginParam { .. } => (0.0, 1.0),
        }
    }

    /// The control's value in the song as it stands (what the envelope starts from).
    /// Plugin parameters live in the plugin, not the song: `None`.
    pub fn current(&self, song: &Song) -> Option<f32> {
        match *self {
            Target::MixerVolume { track } => song.mixer.get(track as usize).map(|m| m.volume),
            Target::MixerPan { track } => song.mixer.get(track as usize).map(|m| m.pan),
            Target::ChannelVolume { inst } => song.instrument(inst).map(|i| i.volume),
            Target::ChannelPan { inst } => song.instrument(inst).map(|i| i.pan),
            Target::FxParam { track, slot, param } => song
                .mixer
                .get(track as usize)
                .and_then(|m| m.fx.get(slot as usize))
                .and_then(|s| s.params.get(param as usize))
                .copied(),
            Target::FxMix { track, slot } => song
                .mixer
                .get(track as usize)
                .and_then(|m| m.fx.get(slot as usize))
                .map(|s| s.mix),
            Target::PluginParam { .. } => None,
        }
    }

    /// Whether the control still exists in `song`.
    pub fn exists(&self, song: &Song) -> bool {
        match *self {
            Target::PluginParam { plugin, .. } => {
                song.instruments
                    .iter()
                    .filter_map(|i| i.plugin())
                    .any(|p| p.id == plugin)
                    || song
                        .mixer
                        .iter()
                        .flat_map(|m| &m.fx)
                        .filter_map(|s| s.plugin.as_ref())
                        .any(|p| p.id == plugin)
            }
            _ => self.current(song).is_some(),
        }
    }

    /// The control's default value in its own units.
    pub fn default_value(&self, song: &Song) -> f32 {
        match *self {
            Target::MixerVolume { .. } | Target::ChannelVolume { .. } => 1.0,
            Target::MixerPan { .. } | Target::ChannelPan { .. } => 0.0,
            Target::FxMix { .. } => 1.0,
            Target::FxParam { track, slot, param } => song
                .mixer
                .get(track as usize)
                .and_then(|m| m.fx.get(slot as usize))
                .and_then(|s| s.kind.params().get(param as usize))
                .map(|p| p.default)
                .unwrap_or(0.0),
            Target::PluginParam { .. } => 0.5,
        }
    }

    /// Apply `value` (clamped to the target's range) to `song`.
    /// Returns `true` if a song-owned control was updated; plugin parameters return `false`.
    pub fn set(&self, song: &mut Song, value: f32) -> bool {
        let (lo, hi) = self.range(song);
        let v = value.clamp(lo.min(hi), hi.max(lo));
        match *self {
            Target::MixerVolume { track } => {
                if let Some(m) = song.mixer.get_mut(track as usize) {
                    m.volume = v;
                    return true;
                }
            }
            Target::MixerPan { track } => {
                if let Some(m) = song.mixer.get_mut(track as usize) {
                    m.pan = v;
                    return true;
                }
            }
            Target::ChannelVolume { inst } => {
                if let Some(i) = song.instruments.iter_mut().find(|i| i.id == inst) {
                    i.volume = v;
                    return true;
                }
            }
            Target::ChannelPan { inst } => {
                if let Some(i) = song.instruments.iter_mut().find(|i| i.id == inst) {
                    i.pan = v;
                    return true;
                }
            }
            Target::FxParam { track, slot, param } => {
                if let Some(p) = song
                    .mixer
                    .get_mut(track as usize)
                    .and_then(|m| m.fx.get_mut(slot as usize))
                    .and_then(|s| s.params.get_mut(param as usize))
                {
                    *p = v;
                    return true;
                }
            }
            Target::FxMix { track, slot } => {
                if let Some(effect) = song
                    .mixer
                    .get_mut(track as usize)
                    .and_then(|m| m.fx.get_mut(slot as usize))
                {
                    effect.mix = v;
                    return true;
                }
            }
            Target::PluginParam { .. } => {}
        }
        false
    }
}

/// A point of the envelope. `tension` bends the segment to the next point:
/// 0 is a straight line, positive starts slow, negative starts fast (FL: single curve).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub tick: Tick,
    pub value: f32,
    #[serde(default)]
    pub tension: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub target: Target,
    /// Sorted by tick.
    pub points: Vec<Point>,
    #[serde(default, skip_serializing_if = "Playback::is_default")]
    pub playback: Playback,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Playback {
    #[default]
    Clip,
    Hold,
    Events,
}

impl Playback {
    fn is_default(&self) -> bool {
        *self == Self::Clip
    }
}

/// The curve between two points: `x` 0..1 through the segment, bent by `tension`.
pub fn shape(x: f32, tension: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if tension.abs() < 1e-3 {
        return x;
    }
    // Exponent 1/8 .. 8 across tension -1 .. 1.
    x.powf(8f32.powf(tension.clamp(-1.0, 1.0)))
}

impl Envelope {
    /// A flat envelope at `value` from 0 to `length`.
    pub fn flat(target: Target, value: f32, length: Tick) -> Self {
        Self {
            target,
            playback: Playback::Clip,
            points: vec![
                Point {
                    tick: 0,
                    value,
                    tension: 0.0,
                },
                Point {
                    tick: length.max(1),
                    value,
                    tension: 0.0,
                },
            ],
        }
    }

    /// The value at `tick` (clip-local). Recorded events change the value at their tick.
    pub fn value_at(&self, tick: Tick) -> Option<f32> {
        if self.playback == Playback::Events {
            return self.event_at(tick).map(|p| p.value);
        }
        let first = self.points.first()?;
        if tick <= first.tick {
            return Some(first.value);
        }
        let i = self.points.partition_point(|p| p.tick <= tick);
        let Some(b) = self.points.get(i) else {
            return self.points.last().map(|p| p.value);
        };
        let a = self.points[i - 1];
        let span = (b.tick - a.tick).max(1) as f32;
        let x = shape((tick - a.tick) as f32 / span, a.tension);
        if self.target.tapered() {
            // Interpolate along the fader travel: a straight fade sounds even.
            let (pa, pb) = (
                crate::sequence::volume_pos(a.value),
                crate::sequence::volume_pos(b.value),
            );
            return Some(crate::sequence::volume_gain(pa + (pb - pa) * x));
        }
        Some(a.value + (b.value - a.value) * x)
    }

    fn event_at(&self, tick: Tick) -> Option<&Point> {
        let end = self.points.partition_point(|p| p.tick <= tick);
        self.points.get(end.checked_sub(1)?)
    }

    /// Keep points sorted and inside the value range (after edits).
    pub fn tidy(&mut self, lo: f32, hi: f32) {
        self.points.sort_by_key(|p| p.tick);
        for p in &mut self.points {
            p.value = p.value.clamp(lo.min(hi), hi.max(lo));
            p.tension = p.tension.clamp(-1.0, 1.0);
            p.tick = p.tick.max(0);
        }
    }
}

/// Thin raw recorded `(tick, value)` samples into clean editable envelope points.
/// Uses vertical Ramer–Douglas–Peucker in normalized `0..1` travel space so the
/// resulting linear envelope stays within `tolerance` of the recorded curve.
pub fn thin_points(
    target: Target,
    lo: f32,
    hi: f32,
    samples: &[(Tick, f32)],
    tolerance: f32,
) -> Vec<Point> {
    if samples.is_empty() {
        return Vec::new();
    }
    let (vlo, vhi) = (lo.min(hi), hi.max(lo));
    // Collapse same-tick samples (keeping the latest value) after sorting by tick.
    let mut sorted: Vec<(Tick, f32)> = samples
        .iter()
        .map(|&(t, v)| (t.max(0), v.clamp(vlo, vhi)))
        .collect();
    sorted.sort_by_key(|&(t, _)| t);
    let mut dedup: Vec<(Tick, f32)> = Vec::with_capacity(sorted.len());
    for (t, v) in sorted {
        if let Some(last) = dedup.last_mut()
            && last.0 == t
        {
            last.1 = v;
        } else {
            dedup.push((t, v));
        }
    }
    if dedup.len() <= 2 {
        return dedup
            .into_iter()
            .map(|(tick, value)| Point {
                tick,
                value,
                tension: 0.0,
            })
            .collect();
    }
    let units: Vec<f32> = dedup
        .iter()
        .map(|&(_, v)| target.to_unit(v, lo, hi))
        .collect();
    let mut keep = vec![false; dedup.len()];
    keep[0] = true;
    keep[dedup.len() - 1] = true;

    fn rdp(pts: &[(Tick, f32)], units: &[f32], a: usize, b: usize, tol: f32, keep: &mut [bool]) {
        if b <= a + 1 {
            return;
        }
        let t0 = pts[a].0 as f32;
        let span = (pts[b].0 - pts[a].0).max(1) as f32;
        let u0 = units[a];
        let du = units[b] - u0;
        let mut best_idx = 0usize;
        let mut best_err = 0.0f32;
        for i in (a + 1)..b {
            let frac = (pts[i].0 as f32 - t0) / span;
            let expected = u0 + du * frac;
            let err = (units[i] - expected).abs();
            if err > best_err {
                best_err = err;
                best_idx = i;
            }
        }
        if best_err > tol {
            keep[best_idx] = true;
            rdp(pts, units, a, best_idx, tol, keep);
            rdp(pts, units, best_idx, b, tol, keep);
        }
    }

    rdp(
        &dedup,
        &units,
        0,
        dedup.len() - 1,
        tolerance.max(0.0),
        &mut keep,
    );
    dedup
        .into_iter()
        .enumerate()
        .filter_map(|(i, (tick, value))| {
            keep[i].then_some(Point {
                tick,
                value,
                tension: 0.0,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
pub struct ValueAt {
    pub target: Target,
    pub value: f32,
    tick: Tick,
}

/// Values in force at song tick `at`, including FL controls held between clips.
pub fn values_at(song: &Song, at: Tick, out: &mut Vec<ValueAt>) {
    out.clear();
    for clip in song
        .clips
        .iter()
        .filter(|c| !c.muted && !song.track_muted(c.track))
    {
        let Some(p) = song.patterns.iter().find(|p| p.id == clip.pattern) else {
            continue;
        };
        let Some(env) = &p.automation else { continue };
        let len = clip.length.unwrap_or(p.length - clip.offset).max(1);
        if at < clip.start {
            continue;
        }
        let elapsed = at - clip.start;
        let (tick, value) = match env.playback {
            Playback::Clip if elapsed >= len => continue,
            Playback::Clip => (
                at,
                env.value_at((elapsed + clip.offset).rem_euclid(p.length.max(1))),
            ),
            Playback::Hold => {
                let elapsed = elapsed.min(len);
                (clip.start + elapsed, env.value_at(clip.offset + elapsed))
            }
            Playback::Events => {
                let local = clip.offset + elapsed.min(len - 1);
                let Some(point) = env
                    .event_at(local)
                    .filter(|point| point.tick >= clip.offset)
                else {
                    continue;
                };
                (clip.start + point.tick - clip.offset, Some(point.value))
            }
        };
        let Some(value) = value else {
            continue;
        };
        let sample = ValueAt {
            target: env.target,
            value,
            tick,
        };
        if let Some(slot) = out.iter_mut().find(|s| s.target == env.target) {
            if slot.tick <= tick {
                *slot = sample;
            }
        } else if out.len() < out.capacity() || out.capacity() == 0 {
            // A full list on the audio thread drops the extra clip instead of allocating.
            out.push(sample);
        }
    }
}

/// The automated value of `target` in `vals`, if any.
pub fn find(vals: &[ValueAt], target: Target) -> Option<f32> {
    vals.iter().find(|s| s.target == target).map(|s| s.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::{BAR, Clip, Pattern};

    #[test]
    fn values_follow_the_points() {
        let t = Target::MixerPan { track: 1 };
        let env = Envelope {
            target: t,
            playback: Playback::Clip,
            points: vec![
                Point {
                    tick: 0,
                    value: 0.0,
                    tension: 0.0,
                },
                Point {
                    tick: 100,
                    value: 1.0,
                    tension: 0.0,
                },
            ],
        };
        assert_eq!(env.value_at(-5), Some(0.0));
        assert_eq!(env.value_at(50), Some(0.5));
        assert_eq!(env.value_at(100), Some(1.0));
        assert_eq!(env.value_at(500), Some(1.0));
    }

    #[test]
    fn tension_bends_but_keeps_the_ends() {
        assert_eq!(shape(0.0, 0.7), 0.0);
        assert_eq!(shape(1.0, -0.7), 1.0);
        assert!(shape(0.5, 0.5) < 0.5, "positive tension starts slow");
        assert!(shape(0.5, -0.5) > 0.5, "negative tension starts fast");
    }

    #[test]
    fn clips_in_the_playlist_drive_their_target_and_loop() {
        let mut song = Song::default();
        let t = Target::MixerPan { track: 2 };
        let mut p = Pattern::new(50, "Pan");
        p.length = BAR;
        p.automation = Some(Envelope {
            target: t,
            playback: Playback::Clip,
            points: vec![
                Point {
                    tick: 0,
                    value: -1.0,
                    tension: 0.0,
                },
                Point {
                    tick: BAR,
                    value: 1.0,
                    tension: 0.0,
                },
            ],
        });
        song.patterns.push(p);
        let mut c = Clip::new(50, BAR, 3);
        c.length = Some(2 * BAR);
        song.clips.push(c);
        let mut out = Vec::with_capacity(8);
        values_at(&song, 0, &mut out);
        assert!(out.is_empty(), "before the clip");
        values_at(&song, BAR + BAR / 2, &mut out);
        assert_eq!(find(&out, t), Some(0.0));
        values_at(&song, 2 * BAR + BAR / 4, &mut out);
        assert_eq!(
            find(&out, t),
            Some(-0.5),
            "the envelope loops over a longer clip"
        );
        values_at(&song, 3 * BAR, &mut out);
        assert!(out.is_empty(), "after the clip");
    }

    #[test]
    fn ranges_and_current_values() {
        let song = Song::default();
        assert_eq!(Target::MixerPan { track: 0 }.range(&song), (-1.0, 1.0));
        assert_eq!(
            Target::MixerVolume { track: 0 }.current(&song),
            Some(song.mixer[0].volume)
        );
        assert_eq!(Target::ChannelVolume { inst: 999 }.current(&song), None);
        assert!(!Target::ChannelVolume { inst: 999 }.exists(&song));
    }

    #[test]
    fn a_straight_volume_fade_moves_along_the_fader_not_the_gain() {
        let t = Target::ChannelVolume { inst: 1 };
        let env = Envelope {
            target: t,
            playback: Playback::Clip,
            points: vec![
                Point {
                    tick: 0,
                    value: 1.0,
                    tension: 0.0,
                },
                Point {
                    tick: 100,
                    value: 0.0,
                    tension: 0.0,
                },
            ],
        };
        let mid = env.value_at(50).unwrap();
        // Linear gain would be 0.5 (−6 dB): most of the fade would happen in the last moments.
        let pos = crate::sequence::volume_pos(1.0) / 2.0;
        assert!(
            (mid - crate::sequence::volume_gain(pos)).abs() < 1e-6,
            "{mid}"
        );
        assert!(20.0 * mid.log10() < -15.0);
        assert_eq!(env.value_at(0), Some(1.0));
        assert_eq!(env.value_at(100), Some(0.0));
        // The editor's y and a MIDI fader use the same travel.
        let (lo, hi) = (0.0, VOLUME_MAX);
        assert!((t.from_unit(t.to_unit(0.3, lo, hi), lo, hi) - 0.3).abs() < 1e-5);
        assert!((t.to_unit(1.0, lo, hi) - 0.807).abs() < 0.002);
        let pan = Target::ChannelPan { inst: 1 };
        assert_eq!(pan.from_unit(0.5, -1.0, 1.0), 0.0, "pan stays linear");
    }

    #[test]
    fn target_set_and_default_value_round_trip() {
        let mut song = Song::default();
        let pan = Target::MixerPan { track: 1 };
        assert!(pan.set(&mut song, 2.5));
        assert_eq!(pan.current(&song), Some(1.0), "clamps to range");
        let pan_def = pan.default_value(&song);
        assert!(pan.set(&mut song, pan_def));
        assert_eq!(pan.current(&song), Some(0.0));
        let vol = Target::MixerVolume { track: 0 };
        assert!(vol.set(&mut song, 0.4));
        assert_eq!(vol.current(&song), Some(0.4));
        let vol_def = vol.default_value(&song);
        assert!(vol.set(&mut song, vol_def));
        assert_eq!(vol.current(&song), Some(1.0));
        assert!(!Target::ChannelVolume { inst: 999 }.set(&mut song, 0.5));
    }

    #[test]
    fn thin_points_reduces_collinear_samples_and_keeps_corners() {
        let t = Target::MixerPan { track: 1 };
        // Straight ramp from -1 to +1 over 100 ticks with 101 dense points -> thins to 2 points.
        let ramp: Vec<(Tick, f32)> = (0..=100)
            .map(|i| (i * 4, -1.0 + 2.0 * (i as f32 / 100.0)))
            .collect();
        let thinned = thin_points(t, -1.0, 1.0, &ramp, 0.005);
        assert_eq!(thinned.len(), 2);
        assert_eq!(thinned[0].tick, 0);
        assert_eq!(thinned[1].tick, 400);

        // Triangle wave (-1 -> +1 -> -1) keeps the peak corner.
        let mut tri = Vec::new();
        for i in 0..=50 {
            tri.push((i * 4, -1.0 + 2.0 * (i as f32 / 50.0)));
        }
        for i in 1..=50 {
            tri.push(((50 + i) * 4, 1.0 - 2.0 * (i as f32 / 50.0)));
        }
        let thinned = thin_points(t, -1.0, 1.0, &tri, 0.005);
        assert_eq!(thinned.len(), 3);
        assert_eq!(thinned[1].tick, 200);
        assert!((thinned[1].value - 1.0).abs() < 1e-6);

        // Duplicate ticks keep the latest sample.
        let dup = vec![(10, -0.5), (10, 0.5), (20, 0.5)];
        let thinned = thin_points(t, -1.0, 1.0, &dup, 0.005);
        assert_eq!(thinned.len(), 2);
        assert_eq!(thinned[0].value, 0.5);
    }
}
