//! Forge curve editor: Pitch, Volume, Filter, Pan, and Morph contours with
//! one-click shape presets, semitone snap, tension bending, and ghost waveform.

use super::{Line, View, chips, cutoff_hz, hz, knob, wave, wrapped_chips_idx};
use crate::i18n::fill;
use crate::theme as t;
use crate::ui::kit;
use crate::ui::synth::{Edit, panel, pct, secs, signed};
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::forge::{Curve, CurvePoint, CurvePreset, FilterMode, Layer, PITCH_RANGE};

pub(super) fn curves_panel(
    ui: &mut Ui,
    ed: &mut Edit,
    salt: &str,
    l: &mut Layer,
    view: &mut View,
    ghost_frames: &[[f32; 2]],
) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let line_color = match view.line {
        Line::Pitch => t::enc_blue(),
        Line::Volume => t::enc_green(),
        Line::Filter => t::enc_orange(),
        Line::Pan => t::enc_white(),
        Line::Morph => t::wait(),
    };
    panel(ui, tr.forge_sec_curves, line_color, |ui| {
        let lines = [
            Line::Pitch,
            Line::Volume,
            Line::Filter,
            Line::Pan,
            Line::Morph,
        ];
        let at = lines.iter().position(|v| *v == view.line).unwrap_or(0);
        ui.horizontal_wrapped(|ui| {
            if let Some(i) = wrapped_chips_idx(
                ui,
                &[
                    tr.forge_line_pitch,
                    tr.forge_line_volume,
                    tr.forge_line_filter,
                    tr.forge_line_pan,
                    tr.forge_line_morph,
                ],
                at,
            ) {
                view.line = lines[i];
            }
            let range_lbl = match view.line {
                Line::Pitch => {
                    let most = l.pitch.points.iter().map(|p| p.v.abs()).fold(0.0, f32::max);
                    let span = ((most / 12.0).ceil() * 12.0).clamp(12.0, PITCH_RANGE);
                    fill(
                        tr.forge_curve_range_pitch,
                        &[("span", &format!("{span:.0}"))],
                    )
                }
                Line::Volume => tr.forge_curve_range_amp.into(),
                Line::Filter => tr.forge_curve_range_filter.into(),
                Line::Pan => tr.forge_curve_range_pan.into(),
                Line::Morph => tr.forge_curve_range_morph.into(),
            };
            ui.label(
                egui::RichText::new(range_lbl)
                    .font(t::mono(10.0))
                    .color(t::faint()),
            );
        });

        // One-click curve presets.
        let show_presets = view.line != Line::Filter || l.filter != FilterMode::Off;
        if show_presets {
            ui.horizontal_wrapped(|ui| {
                for preset in CurvePreset::ALL {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(tr.forge_curve_preset(preset))
                                    .font(t::mono(10.0))
                                    .color(t::muted()),
                            )
                            .min_size(vec2(42.0, 20.0)),
                        )
                        .clicked()
                    {
                        match view.line {
                            Line::Pitch => l.pitch = preset.build(-12.0, 12.0, 0.0),
                            Line::Volume => l.amp = preset.build(0.0, 1.0, 1.0),
                            Line::Filter => l.cutoff = preset.build(0.08, 0.96, 1.0),
                            Line::Pan => l.pan_curve = preset.build(-1.0, 1.0, 0.0),
                            Line::Morph => l.morph = preset.build(0.0, 1.0, 0.5),
                        }
                        ed.started = true;
                        ed.changed = true;
                    }
                }
            });
        }

        let length = l.length as f64;
        match view.line {
            Line::Pitch => curve_editor(
                ui,
                ed,
                &format!("{salt}-pitch"),
                &mut l.pitch,
                Line::Pitch,
                t::enc_blue(),
                length,
                ghost_frames,
            ),
            Line::Volume => curve_editor(
                ui,
                ed,
                &format!("{salt}-amp"),
                &mut l.amp,
                Line::Volume,
                t::enc_green(),
                length,
                ghost_frames,
            ),
            Line::Filter => {
                chips(
                    ui,
                    ed,
                    &format!("{salt}-fmode"),
                    &FilterMode::ALL,
                    &FilterMode::ALL.map(|m| tr.forge_filter_mode(m)),
                    &mut l.filter,
                );
                if l.filter != FilterMode::Off {
                    curve_editor(
                        ui,
                        ed,
                        &format!("{salt}-cut"),
                        &mut l.cutoff,
                        Line::Filter,
                        t::enc_orange(),
                        length,
                        ghost_frames,
                    );
                    ui.horizontal_wrapped(|ui| {
                        knob(
                            ui,
                            ed,
                            tr.forge_k_reso,
                            t::enc_orange(),
                            &mut l.resonance,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.forge_k_reso_hint,
                        );
                    });
                }
            }
            Line::Pan => curve_editor(
                ui,
                ed,
                &format!("{salt}-pan"),
                &mut l.pan_curve,
                Line::Pan,
                t::enc_white(),
                length,
                ghost_frames,
            ),
            Line::Morph => curve_editor(
                ui,
                ed,
                &format!("{salt}-morph"),
                &mut l.morph,
                Line::Morph,
                t::wait(),
                length,
                ghost_frames,
            ),
        }
    });
}

/// A drawn line: drag points, double-click to add or remove one, drag the
/// line between two points to bend it. Pitch snaps to semitones (Alt: free).
#[allow(clippy::too_many_arguments)]
pub(super) fn curve_editor(
    ui: &mut Ui,
    ed: &mut Edit,
    salt: &str,
    c: &mut Curve,
    kind: Line,
    color: Color32,
    length: f64,
    ghost_frames: &[[f32; 2]],
) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let (lo, hi) = match kind {
        Line::Pitch => {
            let most = c.points.iter().map(|p| p.v.abs()).fold(0.0, f32::max);
            let span = ((most / 12.0).ceil() * 12.0).clamp(12.0, PITCH_RANGE);
            (-span, span)
        }
        Line::Pan => (-1.0, 1.0),
        _ => (0.0, 1.0),
    };
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let r = kit::sunken(&painter, area, 6.0).shrink(10.0);
    let x = |t: f32| r.left() + t * r.width();
    let y = |v: f32| r.bottom() - (v - lo) / (hi - lo) * r.height();
    let to_t = |px: f32| ((px - r.left()) / r.width()).clamp(0.0, 1.0);
    let to_v = |py: f32| (lo + (r.bottom() - py) / r.height() * (hi - lo)).clamp(lo, hi);

    // Subtle ghost waveform behind the curve for timing reference.
    if !ghost_frames.is_empty() {
        wave(&painter, r, ghost_frames, t::line().gamma_multiply(0.55));
    }

    // Guides: octaves for pitch, decades for filter, center for pan/morph/volume.
    let guide = Stroke::new(1.0_f32, t::surface());
    match kind {
        Line::Pitch => {
            let mut v = lo;
            while v <= hi + 0.1 {
                let s = if v == 0.0 {
                    Stroke::new(1.0_f32, t::line())
                } else {
                    guide
                };
                painter.line_segment([pos2(r.left(), y(v)), pos2(r.right(), y(v))], s);
                kit::caps(
                    &painter,
                    pos2(r.left() + 2.0, y(v) - 1.0),
                    Align2::LEFT_BOTTOM,
                    &format!("{v:+.0}"),
                    9.0,
                    t::faint(),
                );
                v += 12.0;
            }
        }
        Line::Filter => {
            for (u, label) in [(1.0f32 / 3.0, "200 HZ"), (2.0 / 3.0, "2 KHZ")] {
                painter.line_segment([pos2(r.left(), y(u)), pos2(r.right(), y(u))], guide);
                kit::caps(
                    &painter,
                    pos2(r.left() + 2.0, y(u) - 1.0),
                    Align2::LEFT_BOTTOM,
                    label,
                    9.0,
                    t::faint(),
                );
            }
        }
        Line::Pan => {
            painter.line_segment(
                [pos2(r.left(), y(0.0)), pos2(r.right(), y(0.0))],
                Stroke::new(1.0_f32, t::line()),
            );
            kit::caps(
                &painter,
                pos2(r.left() + 2.0, r.top() + 10.0),
                Align2::LEFT_BOTTOM,
                "R",
                9.0,
                t::faint(),
            );
            kit::caps(
                &painter,
                pos2(r.left() + 2.0, r.bottom() - 1.0),
                Align2::LEFT_BOTTOM,
                "L",
                9.0,
                t::faint(),
            );
        }
        Line::Volume | Line::Morph => {
            painter.line_segment([pos2(r.left(), y(0.5)), pos2(r.right(), y(0.5))], guide);
        }
    }

    let pts: Vec<_> = (0..=240)
        .map(|i| {
            let tt = i as f32 / 240.0;
            pos2(x(tt), y(c.at(tt).clamp(lo, hi)))
        })
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, color)));

    let near = |c: &Curve, pos: egui::Pos2| {
        c.points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, pos2(x(p.t), y(p.v.clamp(lo, hi))).distance(pos)))
            .filter(|(_, d)| *d <= 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    #[derive(Clone, Copy)]
    enum Grab {
        Point(usize),
        Bend(usize),
    }
    let grab_id = ui.id().with((salt, "grab"));
    let hover = resp.hover_pos();
    let over = hover.and_then(|h| near(c, h));
    if resp.drag_started()
        && let Some(pos) = ui
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos())
    {
        let g = match near(c, pos) {
            Some(i) => Some(Grab::Point(i)),
            None => {
                let tt = to_t(pos.x);
                let i = c.points.partition_point(|p| p.t <= tt);
                (i > 0 && i < c.points.len()).then_some(Grab::Bend(i - 1))
            }
        };
        ui.data_mut(|d| d.insert_temp(grab_id, g));
        ed.started |= g.is_some();
    }
    let grab: Option<Grab> = ui.data(|d| d.get_temp(grab_id)).flatten();
    if resp.dragged()
        && let Some(g) = grab
        && let Some(pos) = resp.interact_pointer_pos()
    {
        match g {
            Grab::Point(i) if i < c.points.len() => {
                let min = if i > 0 { c.points[i - 1].t } else { 0.0 };
                let max = c.points.get(i + 1).map_or(1.0, |p| p.t);
                let mut v = to_v(pos.y);
                if kind == Line::Pitch && !ui.input(|i| i.modifiers.alt) {
                    v = v.round();
                }
                c.points[i].t = to_t(pos.x).clamp(min, max);
                c.points[i].v = v;
            }
            Grab::Bend(i) if i + 1 < c.points.len() => {
                let dy = kit::captured_drag(ui, &resp).y / 60.0;
                let rising = c.points[i + 1].v >= c.points[i].v;
                let p = &mut c.points[i];
                p.tension = (p.tension + if rising { dy } else { -dy }).clamp(-1.0, 1.0);
            }
            _ => {}
        }
        ed.changed = true;
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<Grab>>(grab_id));
    }
    if resp.double_clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        match near(c, pos) {
            Some(i) if c.points.len() > 2 => {
                c.points.remove(i);
            }
            Some(_) => {}
            None => {
                let mut v = to_v(pos.y);
                if kind == Line::Pitch {
                    v = v.round();
                }
                let tt = to_t(pos.x);
                let at = c.points.partition_point(|p| p.t <= tt);
                c.points.insert(
                    at,
                    CurvePoint {
                        t: tt,
                        v,
                        tension: 0.0,
                    },
                );
            }
        }
        ed.started = true;
        ed.changed = true;
    }
    let value = |v: f32| match kind {
        Line::Pitch => format!("{v:+.1} st"),
        Line::Volume | Line::Morph => pct(v as f64),
        Line::Filter => hz(cutoff_hz(v)),
        Line::Pan => signed(v as f64),
    };
    for (i, p) in c.points.iter().enumerate() {
        let at = pos2(x(p.t), y(p.v.clamp(lo, hi)));
        let lit = over == Some(i) || matches!(grab, Some(Grab::Point(g)) if g == i);
        painter.circle_filled(at, if lit { 5.5 } else { 4.0 }, color);
        painter.circle_stroke(
            at,
            if lit { 5.5 } else { 4.0 },
            Stroke::new(1.0_f32, t::edge()),
        );
        if lit {
            kit::caps(
                &painter,
                at + vec2(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                &format!("{} · {}", secs(p.t as f64 * length), value(p.v)),
                10.0,
                t::text(),
            );
        }
    }
    let _ = Rect::NOTHING;
    resp.on_hover_text(match kind {
        Line::Pitch => tr.forge_curve_pitch_tip,
        _ => tr.forge_curve_tip,
    });
}
