//! A drawn line over a layer's life: pitch, volume or filter against time.
//! Time runs 0..1 across the layer; the bend between points is the
//! automation clip's (`automation::shape`), so both editors feel the same.

use serde::{Deserialize, Serialize};

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
}
