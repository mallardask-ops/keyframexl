//! A drawn line over a layer's life: pitch, volume, filter, pan or morph
//! against time. Time runs 0..1 across the layer; the bend between points is
//! the automation clip's (`automation::shape`), so both editors feel the same.

use serde::{Deserialize, Serialize};

/// One-click curve shapes for rapid sound shaping in the curve editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvePreset {
    Drop,
    Pluck,
    Ramp,
    Bell,
    Stutter,
    Flat,
}

impl CurvePreset {
    pub const ALL: [CurvePreset; 6] = [
        CurvePreset::Drop,
        CurvePreset::Pluck,
        CurvePreset::Ramp,
        CurvePreset::Bell,
        CurvePreset::Stutter,
        CurvePreset::Flat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CurvePreset::Drop => "Drop",
            CurvePreset::Pluck => "Pluck",
            CurvePreset::Ramp => "Ramp",
            CurvePreset::Bell => "Bell",
            CurvePreset::Stutter => "Stutter",
            CurvePreset::Flat => "Flat",
        }
    }

    /// Build a curve inside `[lo, hi]` with neutral resting level `neutral`.
    pub fn build(self, lo: f32, hi: f32, neutral: f32) -> Curve {
        let mid = lo + (hi - lo) * 0.5;
        let lerp = |u: f32| lo + (hi - lo) * u;
        match self {
            CurvePreset::Flat => Curve::flat(neutral.clamp(lo, hi)),
            CurvePreset::Drop => {
                Curve::new(&[(0.0, hi, -0.55), (0.45, lerp(0.25), -0.25), (1.0, lo, 0.0)])
            }
            CurvePreset::Pluck => {
                Curve::new(&[(0.0, hi, -0.7), (0.22, lerp(0.18), -0.4), (1.0, lo, 0.0)])
            }
            CurvePreset::Ramp => Curve::new(&[(0.0, lo, 0.45), (1.0, hi, 0.0)]),
            CurvePreset::Bell => Curve::new(&[(0.0, lo, 0.35), (0.38, hi, -0.35), (1.0, lo, 0.0)]),
            CurvePreset::Stutter => Curve::new(&[
                (0.0, hi, -0.5),
                (0.24, lerp(0.15), 0.0),
                (0.33, hi, -0.5),
                (0.58, lerp(0.15), 0.0),
                (0.66, lerp(0.85), -0.5),
                (1.0, lo.max(mid.min(lo)), 0.0),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// 0..1 across the layer.
    pub t: f32,
    pub v: f32,
    /// Bend of the segment that starts here, −1..1 (0 is straight).
    #[serde(default)]
    pub tension: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// Sorted by `t`; never empty.
    pub points: Vec<CurvePoint>,
}

impl Curve {
    pub fn flat(v: f32) -> Self {
        Self::new(&[(0.0, v, 0.0), (1.0, v, 0.0)])
    }

    /// True when every point sits within `1e-5` of `v`.
    pub fn is_flat_at(&self, v: f32) -> bool {
        self.points.iter().all(|p| (p.v - v).abs() < 1e-5)
    }

    /// From `(t, value, tension)` triples.
    pub fn new(points: &[(f32, f32, f32)]) -> Self {
        let mut c = Self {
            points: points
                .iter()
                .map(|&(t, v, tension)| CurvePoint { t, v, tension })
                .collect(),
        };
        if c.points.is_empty() {
            c.points.push(CurvePoint {
                t: 0.0,
                v: 0.0,
                tension: 0.0,
            });
        }
        c
    }

    /// The value at `t` (0..1).
    pub fn at(&self, t: f32) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if t <= first.t {
            return first.v;
        }
        let i = self.points.partition_point(|p| p.t <= t);
        let Some(b) = self.points.get(i) else {
            return self.points.last().map_or(0.0, |p| p.v);
        };
        let a = self.points[i - 1];
        let span = (b.t - a.t).max(1e-6);
        let x = crate::automation::shape((t - a.t) / span, a.tension);
        a.v + (b.v - a.v) * x
    }

    /// Keep the points sorted, inside 0..1 in time and `lo..hi` in value.
    pub fn tidy(&mut self, lo: f32, hi: f32) {
        for p in &mut self.points {
            p.t = if p.t.is_finite() {
                p.t.clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.v = if p.v.is_finite() {
                p.v.clamp(lo, hi)
            } else {
                lo.max(0.0).min(hi)
            };
            p.tension = if p.tension.is_finite() {
                p.tension.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        self.points.sort_by(|a, b| a.t.total_cmp(&b.t));
        if self.points.is_empty() {
            self.points.push(CurvePoint {
                t: 0.0,
                v: lo.max(0.0).min(hi),
                tension: 0.0,
            });
        }
    }

    /// Every value scaled about `pivot` (the Energy knob deepens a sweep).
    pub fn scaled(&self, pivot: f32, by: f32) -> Self {
        let mut c = self.clone();
        for p in &mut c.points {
            p.v = pivot + (p.v - pivot) * by;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_and_holds_the_ends() {
        let c = Curve::new(&[(0.2, 1.0, 0.0), (0.6, 3.0, 0.0)]);
        assert_eq!(c.at(0.0), 1.0);
        assert!((c.at(0.4) - 2.0).abs() < 1e-5);
        assert_eq!(c.at(1.0), 3.0);
    }

    #[test]
    fn tension_bends_the_segment() {
        let slow = Curve::new(&[(0.0, 0.0, 0.8), (1.0, 1.0, 0.0)]);
        let fast = Curve::new(&[(0.0, 0.0, -0.8), (1.0, 1.0, 0.0)]);
        assert!(slow.at(0.5) < 0.2 && fast.at(0.5) > 0.8);
    }

    #[test]
    fn tidy_sorts_and_clamps() {
        let mut c = Curve::new(&[(0.9, 5.0, 0.0), (-1.0, f32::NAN, 3.0)]);
        c.tidy(0.0, 1.0);
        assert_eq!(c.points[0].t, 0.0);
        assert_eq!(c.points[0].tension, 1.0);
        assert_eq!(c.points[1].v, 1.0);
    }

    #[test]
    fn presets_stay_within_bounds_and_match_shapes() {
        for preset in CurvePreset::ALL {
            let mut c = preset.build(-12.0, 12.0, 0.0);
            let orig = c.clone();
            c.tidy(-12.0, 12.0);
            assert_eq!(c, orig, "{preset:?} should already be tidy");
            for i in 0..=20 {
                let v = c.at(i as f32 / 20.0);
                assert!((-12.0..=12.0).contains(&v), "{preset:?} out of bounds: {v}");
            }
        }
        let drop = CurvePreset::Drop.build(0.0, 1.0, 0.5);
        assert!(drop.at(0.0) > drop.at(0.5) && drop.at(0.5) > drop.at(1.0));
        let ramp = CurvePreset::Ramp.build(0.0, 1.0, 0.5);
        assert!(ramp.at(0.0) < ramp.at(0.5) && ramp.at(0.5) < ramp.at(1.0));
        let bell = CurvePreset::Bell.build(0.0, 1.0, 0.5);
        assert!(bell.at(0.38) > bell.at(0.0) && bell.at(0.38) > bell.at(1.0));
        assert!(CurvePreset::Flat.build(0.0, 1.0, 0.5).is_flat_at(0.5));
    }
}
