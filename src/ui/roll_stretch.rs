//! FL-style time-stretch handle for selected notes in the Piano Roll.
//!
//! When two or more notes are selected, a circular `<->` handle appears to the
//! right of the selection span. Dragging it scales every selected note's
//! `start` and `length` proportionally relative to the selection's start
//! (snapping the stretched end to the active grid, or freely with `Alt`).

use eframe::egui::{self, Align2, Color32, Painter, Pos2, Rect, Stroke, pos2};
use keyframexl::sequence::{Note, Tick};

use super::roll::{Geom, snap_round};
use crate::theme as t;

const HANDLE_OFFSET_X: f32 = 12.0;
const HANDLE_RADIUS: f32 = 8.5;
const HANDLE_HIT_RADIUS: f32 = 11.5;

/// Bounding span of the currently selected notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StretchBounds {
    pub origin: Tick,
    pub end: Tick,
    pub min_pitch: u8,
    pub max_pitch: u8,
}

impl StretchBounds {
    pub fn span(&self) -> Tick {
        self.end - self.origin
    }
}

/// Compute the bounding span for a selection. Requires at least two notes with
/// a positive total span.
pub(crate) fn selection_bounds(notes: &[Note]) -> Option<StretchBounds> {
    if notes.len() < 2 {
        return None;
    }
    let mut origin = Tick::MAX;
    let mut end = Tick::MIN;
    let mut min_pitch = u8::MAX;
    let mut max_pitch = u8::MIN;
    for n in notes {
        origin = origin.min(n.start);
        end = end.max(n.end());
        min_pitch = min_pitch.min(n.pitch);
        max_pitch = max_pitch.max(n.pitch);
    }
    (end > origin).then_some(StretchBounds {
        origin,
        end,
        min_pitch,
        max_pitch,
    })
}

/// Screen center of the stretch handle for `bounds`, or `None` if scrolled out
/// of the grid horizontally.
pub(crate) fn handle_center(g: &Geom, bounds: &StretchBounds) -> Option<Pos2> {
    let raw_x = g.x(bounds.end) + HANDLE_OFFSET_X;
    if raw_x < g.grid.left() - HANDLE_RADIUS || raw_x > g.grid.right() + HANDLE_RADIUS {
        return None;
    }
    let cx = raw_x.clamp(
        g.grid.left() + HANDLE_RADIUS + 2.0,
        g.grid.right() - HANDLE_RADIUS - 2.0,
    );
    let top_y = g.y(bounds.max_pitch);
    let bot_y = g.y(bounds.min_pitch) + g.row;
    let cy = ((top_y + bot_y) * 0.5).clamp(
        g.grid.top() + HANDLE_RADIUS + 3.0,
        g.grid.bottom() - HANDLE_RADIUS - 3.0,
    );
    Some(pos2(cx, cy))
}

/// Hit-test the stretch handle at pointer position `q`.
pub(crate) fn hit_handle(g: &Geom, notes: &[Note], q: Pos2) -> Option<StretchBounds> {
    let bounds = selection_bounds(notes)?;
    let center = handle_center(g, &bounds)?;
    (q.distance(center) <= HANDLE_HIT_RADIUS).then_some(bounds)
}

/// Compute the target selection span for a drag to tick `at`.
pub(crate) fn target_span(origin: Tick, at: Tick, grid: Tick, alt: bool) -> Tick {
    let min_span = if alt { 1 } else { grid.max(1) };
    let target_end = snap_round(at, grid, alt);
    (target_end - origin).max(min_span)
}

/// Scale a single note's `start` and `length` proportionally from `orig_span`
/// to `new_span`, anchored at `origin`.
///
/// Scaling both start and end keeps adjacent notes seamlessly touching with no
/// 1-tick gaps or overlaps.
pub(crate) fn stretch_note(
    orig: &Note,
    origin: Tick,
    orig_span: Tick,
    new_span: Tick,
) -> (Tick, Tick) {
    if orig_span <= 0 || new_span <= 0 {
        return (orig.start, orig.length.max(1));
    }
    let scale = new_span as f64 / orig_span as f64;
    let rel_start = (orig.start - origin) as f64;
    let rel_end = (orig.end() - origin) as f64;
    let new_start = (origin + (rel_start * scale).round() as Tick).max(0);
    let new_end = origin + (rel_end * scale).round() as Tick;
    let new_length = (new_end - new_start).max(1);
    (new_start, new_length)
}

/// Paint the stretch handle (and live scale badge when `active_ratio` is set).
pub(crate) fn paint_handle(
    p: &Painter,
    g: &Geom,
    notes: &[Note],
    hover: Option<Pos2>,
    active_ratio: Option<f64>,
) {
    let Some(bounds) = selection_bounds(notes) else {
        return;
    };
    let Some(center) = handle_center(g, &bounds) else {
        return;
    };
    let active = active_ratio.is_some();
    let hovered = hover.is_some_and(|q| q.distance(center) <= HANDLE_HIT_RADIUS);

    // Subtle vertical edge marker at the selection's right edge.
    let end_x = g.x(bounds.end);
    if end_x >= g.grid.left() && end_x <= g.grid.right() {
        let y0 = g.y(bounds.max_pitch).clamp(g.grid.top(), g.grid.bottom());
        let y1 = (g.y(bounds.min_pitch) + g.row).clamp(g.grid.top(), g.grid.bottom());
        let line_col = if active || hovered {
            t::brand()
        } else {
            t::brand().gamma_multiply(0.55)
        };
        p.line_segment(
            [pos2(end_x, y0), pos2(end_x, y1)],
            Stroke::new(1.25_f32, line_col),
        );
        p.line_segment(
            [
                pos2(end_x, center.y),
                pos2(center.x - HANDLE_RADIUS, center.y),
            ],
            Stroke::new(1.0_f32, line_col),
        );
    }

    // Circular handle with `<->` double-arrow glyph.
    let fill = if active {
        t::brand()
    } else if hovered {
        t::brand_soft()
    } else {
        t::surface()
    };
    let stroke_col = if active || hovered {
        t::text()
    } else {
        t::brand()
    };
    let glyph_col = if active {
        Color32::from_rgb(16, 16, 20)
    } else {
        t::text()
    };
    p.circle_filled(center, HANDLE_RADIUS, fill);
    p.circle_stroke(center, HANDLE_RADIUS, Stroke::new(1.5_f32, stroke_col));

    // Draw crisp `<->` arrows inside the handle.
    let s = Stroke::new(1.4_f32, glyph_col);
    let y = center.y;
    let l = center.x - 4.5;
    let r = center.x + 4.5;
    p.line_segment([pos2(l, y), pos2(r, y)], s);
    p.line_segment([pos2(l, y), pos2(l + 2.5, y - 2.5)], s);
    p.line_segment([pos2(l, y), pos2(l + 2.5, y + 2.5)], s);
    p.line_segment([pos2(r, y), pos2(r - 2.5, y - 2.5)], s);
    p.line_segment([pos2(r, y), pos2(r - 2.5, y + 2.5)], s);

    if let Some(ratio) = active_ratio {
        let label = format!("{ratio:.2}×");
        let badge_w = 44.0;
        let badge_h = 18.0;
        let bx = if center.x + HANDLE_RADIUS + 6.0 + badge_w <= g.grid.right() {
            center.x + HANDLE_RADIUS + 6.0
        } else {
            (center.x - HANDLE_RADIUS - 6.0 - badge_w).max(g.grid.left() + 2.0)
        };
        let badge = Rect::from_min_size(
            pos2(bx, center.y - badge_h * 0.5),
            egui::vec2(badge_w, badge_h),
        );
        p.rect_filled(badge, 4.0, t::surface());
        p.rect_stroke(
            badge,
            4.0,
            Stroke::new(1.0_f32, t::brand()),
            egui::StrokeKind::Inside,
        );
        p.text(
            badge.center(),
            Align2::CENTER_CENTER,
            label,
            t::mono(11.0),
            t::text(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyframexl::sequence::{BAR, STEP};

    #[test]
    fn selection_bounds_requires_at_least_two_notes() {
        assert!(selection_bounds(&[]).is_none());
        let one = [Note::new(1, 0, STEP, 60, 100)];
        assert!(selection_bounds(&one).is_none());

        let two = [
            Note::new(1, STEP, STEP, 60, 100),
            Note::new(1, 3 * STEP, 2 * STEP, 67, 100),
        ];
        let b = selection_bounds(&two).expect("bounds");
        assert_eq!(b.origin, STEP);
        assert_eq!(b.end, 5 * STEP);
        assert_eq!(b.span(), 4 * STEP);
        assert_eq!(b.min_pitch, 60);
        assert_eq!(b.max_pitch, 67);
    }

    #[test]
    fn stretch_note_scales_double_and_half_gaplessly() {
        // 4 quarter-bar notes filling 1 bar.
        let q = BAR / 4;
        let notes: Vec<Note> = (0..4)
            .map(|i| Note::new(1, i * q, q, 60 + i as u8, 100))
            .collect();
        let b = selection_bounds(&notes).unwrap();

        // Stretch 1 bar -> 2 bars (2.0x).
        let doubled: Vec<(Tick, Tick)> = notes
            .iter()
            .map(|n| stretch_note(n, b.origin, b.span(), 2 * BAR))
            .collect();
        assert_eq!(
            doubled,
            vec![(0, 2 * q), (2 * q, 2 * q), (4 * q, 2 * q), (6 * q, 2 * q)]
        );

        // Compress 1 bar -> 1/2 bar (0.5x).
        let halved: Vec<(Tick, Tick)> = notes
            .iter()
            .map(|n| stretch_note(n, b.origin, b.span(), BAR / 2))
            .collect();
        assert_eq!(
            halved,
            vec![(0, q / 2), (q / 2, q / 2), (q, q / 2), (3 * q / 2, q / 2)]
        );
    }

    #[test]
    fn stretch_note_keeps_nonzero_origin_anchored() {
        let origin = 4 * STEP;
        let notes = [
            Note::new(1, origin, 2 * STEP, 60, 100),
            Note::new(1, origin + 2 * STEP, 2 * STEP, 64, 100),
        ];
        let b = selection_bounds(&notes).unwrap();
        let stretched: Vec<(Tick, Tick)> = notes
            .iter()
            .map(|n| stretch_note(n, b.origin, b.span(), 8 * STEP))
            .collect();
        assert_eq!(
            stretched,
            vec![(origin, 4 * STEP), (origin + 4 * STEP, 4 * STEP)]
        );
    }

    #[test]
    fn stretch_note_clamps_extreme_compression_to_positive_length() {
        let n = Note::new(1, 0, STEP, 60, 100);
        let (start, len) = stretch_note(&n, 0, 100 * BAR, 1);
        assert_eq!(start, 0);
        assert!(len >= 1);
    }

    #[test]
    fn target_span_snaps_to_grid_and_alt_bypasses() {
        let origin = 4 * STEP;
        // Without Alt, snaps to nearest grid multiple and respects minimum span.
        assert_eq!(
            target_span(origin, origin + 2 * STEP + 3, STEP, false),
            2 * STEP
        );
        assert_eq!(target_span(origin, origin - STEP, STEP, false), STEP);
        // With Alt, tick-exact.
        assert_eq!(
            target_span(origin, origin + 2 * STEP + 3, STEP, true),
            2 * STEP + 3
        );
        assert_eq!(target_span(origin, origin - STEP, STEP, true), 1);
    }
}
