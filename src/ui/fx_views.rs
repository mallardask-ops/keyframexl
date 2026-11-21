use super::kit;
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::fx::FxKind;

/// EQ band colours (the encoder addresses, repeating).
pub fn band_color(b: usize) -> Color32 {
    [
        t::enc_blue(),
        t::enc_orange(),
        t::enc_green(),
        t::enc_white(),
    ][b % 4]
}

pub(super) fn band_db(band: usize, f: f32, hz: f32, gain: f32, q: f32, rate: f32) -> f32 {
    let w = std::f32::consts::TAU * (hz / rate).min(0.49);
    let (s, c) = w.sin_cos();
    let a = 10f32.powf(gain / 40.0);
    let alpha = s / (2.0 * q.max(0.05));
    let (b0, b1, b2, a0, a1, a2) = match band {
        0 => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * c + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - k),
                (a + 1.0) + (a - 1.0) * c + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - k,
            )
        }
        6 => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * c + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - k),
                (a + 1.0) - (a - 1.0) * c + k,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - k,
            )
        }
        _ => (
            1.0 + alpha * a,
            -2.0 * c,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * c,
            1.0 - alpha / a,
        ),
    };
    let x = std::f32::consts::TAU * f / rate;
    let mag = |v0: f32, v1: f32, v2: f32| {
        let re = v0 + v1 * x.cos() + v2 * (2.0 * x).cos();
        let im = -v1 * x.sin() - v2 * (2.0 * x).sin();
        re * re + im * im
    };
    10.0 * (mag(b0, b1, b2) / mag(a0, a1, a2)).log10()
}
fn curve(x: f32, threshold: f32, ratio: f32) -> f32 {
    if x <= threshold {
        x
    } else {
        threshold + (x - threshold) / ratio
    }
}
fn freq_x(r: Rect, f: f32) -> f32 {
    r.left() + r.width() * (f / 20.0).ln() / 1000f32.ln()
}
fn canvas(ui: &mut Ui, height: f32) -> Rect {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(r, 3.0, t::sunken());
    r
}
/// Hann-windowed magnitude spectrum of `x` (length a power of two), in dB
/// relative to a full-scale sine, one value per bin up to Nyquist.
pub fn spectrum_db(x: &[f32]) -> Vec<f32> {
    let n = x.len();
    debug_assert!(n.is_power_of_two());
    let mut re: Vec<f32> = x
        .iter()
        .enumerate()
        .map(|(i, v)| v * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()))
        .collect();
    let mut im = vec![0.0f32; n];
    // Iterative radix-2 FFT.
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (ws, wc) = (ang * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * wc - im[b] * ws, re[b] * ws + im[b] * wc);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
    // A full-scale sine reads 0 dB (the Hann window halves the amplitude).
    let scale = 4.0 / n as f32;
    (0..n / 2)
        .map(|k| {
            20.0 * ((re[k] * re[k] + im[k] * im[k]).sqrt() * scale)
                .max(1e-6)
                .log10()
        })
        .collect()
}

/// Lowest level the spectrum shows, in dBFS.
const SPEC_FLOOR: f32 = -90.0;

/// The gain range of the EQ graph, ± dB.
const EQ_RANGE: f32 = 18.0;

/// Parametric EQ 2's graph (FL): spectrum behind, each band's shape filled
/// in its colour, the sum on top, numbered handles to drag (frequency and
/// gain), wheel for Q, double-click to reset. `focus` is the band last
/// touched (its readout shows). Returns (edit started, changed).
pub fn eq(
    ui: &mut Ui,
    params: &mut [f32],
    specs: &[keyframexl::fx::ParamSpec],
    rate: f32,
    spectrum: Option<&[f32]>,
    height: f32,
    focus: &mut Option<usize>,
) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let full = canvas(ui, height);
    let p = ui.painter_at(full);
    // Room for the dB scale on the right and the Hz scale below.
    let r = Rect::from_min_max(full.min + vec2(0.0, 8.0), full.max - vec2(30.0, 18.0));
    let fx = |f: f32| freq_x(r, f);
    let gy = |g: f32| r.center().y - g / (2.0 * EQ_RANGE) * r.height();
    // Grid: decades bright, the steps between faint.
    for dec in [10.0f32, 100.0, 1000.0, 10000.0] {
        for m in 1..10 {
            let f = dec * m as f32;
            if !(20.0..=20000.0).contains(&f) {
                continue;
            }
            let x = fx(f);
            let c = if m == 1 {
                t::line()
            } else {
                t::line().gamma_multiply(0.45)
            };
            p.line_segment(
                [pos2(x, r.top()), pos2(x, r.bottom())],
                Stroke::new(1.0_f32, c),
            );
        }
    }
    for (f, label) in [
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1000.0, "1k"),
        (2000.0, "2k"),
        (5000.0, "5k"),
        (10000.0, "10k"),
    ] {
        p.text(
            pos2(fx(f), full.bottom() - 3.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            t::mono(10.0),
            t::faint(),
        );
    }
    for g in [-18.0f32, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
        let y = gy(g);
        let c = if g == 0.0 {
            t::faint()
        } else {
            t::line().gamma_multiply(0.7)
        };
        p.line_segment(
            [pos2(r.left(), y), pos2(r.right(), y)],
            Stroke::new(1.0_f32, c),
        );
        p.text(
            pos2(full.right() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            if g > 0.0 {
                format!("+{g:.0}")
            } else {
                format!("{g:.0}")
            },
            t::mono(10.0),
            t::faint(),
        );
    }
    // The spectrum, filled from the floor, on its own dBFS scale.
    if let Some(db) = spectrum.filter(|d| d.len() > 2) {
        let bin_hz = rate / 2.0 / db.len() as f32;
        let steps = (r.width() / 2.0).max(16.0) as usize;
        let mut pts = Vec::with_capacity(steps + 3);
        pts.push(pos2(r.left(), r.bottom()));
        for i in 0..=steps {
            let f0 = 20.0 * 1000f32.powf(i as f32 / steps as f32);
            let f1 = 20.0 * 1000f32.powf((i + 1) as f32 / steps as f32);
            // The loudest bin under this column (low bins span several columns).
            let (b0, b1) = (
                (f0 / bin_hz) as usize,
                ((f1 / bin_hz) as usize).max((f0 / bin_hz) as usize + 1),
            );
            let v = db[b0.min(db.len() - 1)..b1.min(db.len())]
                .iter()
                .cloned()
                .fold(SPEC_FLOOR, f32::max);
            let y = r.bottom() - ((v - SPEC_FLOOR) / -SPEC_FLOOR).clamp(0.0, 1.0) * r.height();
            pts.push(pos2(fx(f0), y));
        }
        pts.push(pos2(r.right(), r.bottom()));
        let top: Vec<_> = pts[1..pts.len() - 1].to_vec();
        // Convex fills tear on a jagged outline: fill column by column.
        let mesh_c = t::muted().gamma_multiply(0.22);
        let mut mesh = egui::Mesh::default();
        for w in top.windows(2) {
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(w[0], mesh_c);
            mesh.colored_vertex(w[1], mesh_c);
            mesh.colored_vertex(pos2(w[1].x, r.bottom()), mesh_c);
            mesh.colored_vertex(pos2(w[0].x, r.bottom()), mesh_c);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        p.add(mesh);
        p.add(egui::Shape::line(
            top,
            Stroke::new(1.0_f32, t::muted().gamma_multiply(0.7)),
        ));
    }
    let steps = (r.width() / 2.0).max(32.0) as usize;
    let freqs: Vec<f32> = (0..=steps)
        .map(|i| 20.0 * 1000f32.powf(i as f32 / steps as f32))
        .collect();
    let band = |b: usize, f: f32| {
        band_db(
            b,
            f,
            params[b * 3],
            params[b * 3 + 1],
            params[b * 3 + 2],
            rate,
        )
    };
    // Each active band's own shape, filled toward 0 dB in its colour.
    for b in 0..7 {
        if params[b * 3 + 1].abs() < 0.05 {
            continue;
        }
        let c = band_color(b);
        let mut mesh = egui::Mesh::default();
        let zero = gy(0.0);
        let fill = c.gamma_multiply(if *focus == Some(b) { 0.34 } else { 0.22 });
        let line: Vec<_> = freqs
            .iter()
            .map(|&f| pos2(fx(f), gy(band(b, f).clamp(-EQ_RANGE, EQ_RANGE))))
            .collect();
        for w in line.windows(2) {
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(w[0], fill);
            mesh.colored_vertex(w[1], fill);
            mesh.colored_vertex(pos2(w[1].x, zero), fill);
            mesh.colored_vertex(pos2(w[0].x, zero), fill);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        p.add(mesh);
        p.add(egui::Shape::line(
            line,
            Stroke::new(1.0_f32, c.gamma_multiply(0.8)),
        ));
    }
    // The sum: what the signal gets.
    let sum: Vec<_> = freqs
        .iter()
        .map(|&f| {
            pos2(
                fx(f),
                gy((0..7)
                    .map(|b| band(b, f))
                    .sum::<f32>()
                    .clamp(-EQ_RANGE, EQ_RANGE)),
            )
        })
        .collect();
    p.add(egui::Shape::line(sum, Stroke::new(2.0_f32, t::text())));
    let mut started = false;
    let mut changed = false;
    let mut hot = None;
    for b in 0..7 {
        let i = b * 3;
        let center = pos2(fx(params[i]), gy(params[i + 1]));
        let response = ui.interact(
            Rect::from_center_size(center, vec2(22.0, 22.0)),
            ui.id().with(("eq-handle", b)),
            Sense::click_and_drag(),
        );
        let active = params[i + 1].abs() >= 0.05;
        let lit = response.hovered() || response.dragged() || *focus == Some(b);
        if lit {
            p.circle_stroke(center, 10.0, Stroke::new(1.5_f32, band_color(b)));
        }
        p.circle_filled(
            center,
            7.0,
            if active || lit {
                band_color(b)
            } else {
                band_color(b).gamma_multiply(0.45)
            },
        );
        p.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{}", b + 1),
            t::mono(9.5),
            t::sunken(),
        );
        if response.drag_started() || response.clicked() {
            *focus = Some(b);
        }
        if response.drag_started() {
            started = true;
        }
        if response.dragged() {
            // Shift: fine steps (FL); from the drag delta so the handle never jumps.
            let fine = if ui.input(|i| i.modifiers.shift) {
                0.2
            } else {
                1.0
            };
            let d = kit::captured_drag(ui, &response) * fine;
            let x = (fx(params[i]) + d.x - r.left()) / r.width();
            params[i] = (20.0 * 1000f32.powf(x.clamp(0.0, 1.0))).clamp(specs[i].min, specs[i].max);
            params[i + 1] =
                (params[i + 1] - d.y / r.height() * 2.0 * EQ_RANGE).clamp(-EQ_RANGE, EQ_RANGE);
            changed = true;
        }
        if response.double_clicked() {
            for j in 0..3 {
                params[i + j] = specs[i + j].default;
            }
            started = true;
            changed = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                params[i + 2] = (params[i + 2] * (scroll * 0.003).exp())
                    .clamp(specs[i + 2].min, specs[i + 2].max);
                started = true;
                changed = true;
                *focus = Some(b);
            }
            hot = Some(b);
        }
        if response.dragged() {
            hot = Some(b);
        }
        response.on_hover_text(tr.fxv_eq_tip);
    }
    // Readout of the band under the pointer, else the focused one.
    if let Some(b) = hot.or(*focus) {
        let i = b * 3;
        let fmt = |k: usize| keyframexl::fx::format(&specs[i + k], params[i + k]);
        let band = fill(tr.fxv_band_n, &[("n", &(b + 1).to_string())]);
        let name = match b {
            0 => tr.fxv_low_shelf,
            6 => tr.fxv_high_shelf,
            _ => band.as_str(),
        };
        let text = format!("{name}   {}   {}   Q {}", fmt(0), fmt(1), fmt(2));
        let at = pos2(r.left() + 8.0, full.top() + 6.0);
        let g = p.layout_no_wrap(text, t::mono(11.5), t::text());
        let bg = Rect::from_min_size(at - vec2(4.0, 2.0), g.size() + vec2(8.0, 4.0));
        p.rect_filled(bg, 3.0, t::sunken().gamma_multiply(0.85));
        p.rect_filled(
            Rect::from_min_size(bg.min, vec2(3.0, bg.height())),
            1.0,
            band_color(b),
        );
        p.galley(at + vec2(2.0, 0.0), g, t::text());
    }
    (started, changed)
}
/// Full scale of the gain reduction meter, in dB.
const GR_RANGE: f32 = 24.0;

/// Transfer curve of a compressor or limiter, with its gain reduction meter
/// on the right (`gr` in dB, `None` without audio).
pub fn dynamics(ui: &mut Ui, kind: FxKind, p: &[f32], gr: Option<f32>) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let full = canvas(ui, 130.0);
    let painter = ui.painter();
    // The meter: a strip on the right that fills down from the top, like a
    // console's GR needle; the reading sits above it.
    let meter = Rect::from_min_max(
        pos2(full.right() - 14.0, full.top() + 18.0),
        pos2(full.right() - 6.0, full.bottom() - 6.0),
    );
    let r = Rect::from_min_max(full.min, pos2(meter.left() - 8.0, full.max.y));
    painter.rect_filled(meter, 1.0, t::lcd_off());
    for db in [3.0, 6.0, 12.0, 18.0] {
        let y = meter.top() + meter.height() * db / GR_RANGE;
        painter.line_segment(
            [pos2(meter.left() - 3.0, y), pos2(meter.left() - 1.0, y)],
            Stroke::new(1.0_f32, t::faint()),
        );
    }
    let g = gr.unwrap_or(0.0).clamp(0.0, GR_RANGE);
    if g > 0.05 {
        let fill = Rect::from_min_max(
            meter.min,
            pos2(meter.right(), meter.top() + meter.height() * g / GR_RANGE),
        );
        painter.rect_filled(fill, 1.0, t::wait());
    }
    painter.text(
        pos2(full.right() - 4.0, full.top() + 3.0),
        egui::Align2::RIGHT_TOP,
        if gr.is_some() {
            format!("GR {:.1}", -g).replace("-0.0", "0.0")
        } else {
            "GR".to_string()
        },
        t::mono(10.5),
        if g > 0.05 { t::text() } else { t::muted() },
    );
    let xy = |x: f32, y: f32| {
        pos2(
            r.left() + (x + 60.0) / 60.0 * r.width(),
            r.bottom() - (y + 60.0) / 60.0 * r.height(),
        )
    };
    painter.line_segment(
        [xy(-60.0, -60.0), xy(0.0, 0.0)],
        Stroke::new(1.0_f32, t::line()),
    );
    let threshold = if kind == FxKind::Compressor {
        p[0]
    } else {
        p[1] - p[0]
    };
    let points = (0..=120)
        .map(|i| {
            let x = -60.0 + i as f32 * 0.5;
            let y = if kind == FxKind::Compressor {
                curve(x, p[0], p[1]) + p[4]
            } else {
                (x + p[0]).min(p[1])
            };
            xy(x, y.clamp(-60.0, 0.0))
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(2.0_f32, t::enc_blue()),
    ));
    let x = xy(threshold.clamp(-60.0, 0.0), -60.0).x;
    painter.line_segment(
        [pos2(x, r.top()), pos2(x, r.bottom())],
        Stroke::new(1.0_f32, t::muted()),
    );
    painter.text(
        r.left_top() + vec2(4.0, 3.0),
        egui::Align2::LEFT_TOP,
        tr.fxv_out_db,
        t::sans(10.0),
        t::muted(),
    );
    painter.text(
        r.right_bottom() - vec2(4.0, 3.0),
        egui::Align2::RIGHT_BOTTOM,
        tr.fxv_in_db,
        t::sans(10.0),
        t::muted(),
    );
}
pub fn ambience(ui: &mut Ui, kind: FxKind, p: &[f32]) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let r = canvas(ui, 90.0);
    let painter = ui.painter();
    if kind == FxKind::Delay {
        for n in 1..=8 {
            let x = r.left() + r.width() * (n as f32 * p[0] / 128.0);
            if x > r.right() {
                break;
            }
            let h = r.height() * 0.85 * (p[1] / 100.0).powi(n - 1);
            painter.line_segment(
                [pos2(x, r.bottom()), pos2(x, r.bottom() - h)],
                Stroke::new(2.0_f32, t::enc_blue()),
            );
        }
        painter.text(
            r.left_top() + vec2(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            fill(tr.fxv_steps_per_tap, &[("n", &(p[0] as i32).to_string())]),
            t::sans(11.0),
            t::muted(),
        );
    } else {
        let feedback = 0.7 + 0.28 * p[0] / 100.0;
        let damp = p[1] / 100.0;
        let points = (0..=100)
            .map(|i| {
                let x = i as f32 / 100.0;
                pos2(
                    r.left() + x * r.width(),
                    r.bottom()
                        - r.height() * 0.85 * feedback.powf(x * 30.0) * (1.0 - damp * 0.5 * x),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(2.0_f32, t::enc_blue()),
        ));
        painter.text(
            r.left_top() + vec2(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            tr.fxv_decay_approx,
            t::sans(11.0),
            t::muted(),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peak_center() {
        for gain in [-18.0, -6.0, 6.0, 18.0] {
            assert!((band_db(3, 1000.0, 1000.0, gain, 1.0, 48_000.0) - gain).abs() < 0.1);
        }
    }
    #[test]
    fn compressor() {
        assert_eq!(curve(-30.0, -20.0, 4.0), -30.0);
        assert_eq!(curve(-12.0, -20.0, 4.0), -18.0);
    }
}

#[cfg(test)]
mod spectrum_tests {
    #[test]
    fn a_full_scale_sine_reads_0_db_in_its_bin_and_little_elsewhere() {
        let n = 4096;
        let bin = 100;
        let x: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * bin as f32 * i as f32 / n as f32).sin())
            .collect();
        let db = super::spectrum_db(&x);
        assert!(db[bin].abs() < 0.5, "peak {}", db[bin]);
        assert!(db[bin + 10] < -60.0, "leakage {}", db[bin + 10]);
    }
}

pub fn bloom(
    ui: &mut Ui,
    track: usize,
    slot: usize,
    params: &mut [f32],
    enabled: bool,
) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    use super::kit;
    use egui::Align2;

    let modes = [
        ("A", tr.fxv_bloom_balanced, Color32::from_rgb(119, 206, 246)),
        ("B", tr.fxv_bloom_warm, Color32::from_rgb(244, 177, 100)),
        ("C", tr.fxv_bloom_presence, Color32::from_rgb(140, 224, 172)),
        ("D", tr.fxv_bloom_dense, Color32::from_rgb(211, 162, 242)),
    ];
    let selected = (params[1].round() as usize).min(3);
    let accent = if enabled {
        modes[selected].2
    } else {
        t::muted()
    };
    let (panel, _) =
        ui.allocate_exact_size(vec2(ui.available_width().max(250.0), 245.0), Sense::hover());
    let center = panel.center_top() + vec2(0.0, 94.0);
    ui.painter()
        .rect_filled(panel, 8.0, Color32::from_rgb(27, 31, 35));
    let mut amount = params[0] as f64;
    let knob = kit::Knob {
        label: tr.fxv_amount,
        color: accent,
        min: 0.0,
        max: 100.0,
        default: 40.0,
        log: false,
        fmt: &|x| format!("{x:.0}%"),
        hint: tr.fxv_amount_hint,
    };
    kit::automatable(
        ui,
        keyframexl::automation::Target::FxParam {
            track: track as u16,
            slot: slot as u8,
            param: 0,
        },
    );
    let dial = Rect::from_center_size(center, vec2(112.0, 112.0));
    let (_, mut started, mut changed) =
        kit::mini_knob(ui, dial, ui.id().with("amount"), &knob, &mut amount);
    params[0] = amount as f32;
    let paint = ui.painter();
    for tick in 0..48 {
        let angle = std::f32::consts::PI * (0.75 + 1.5 * tick as f32 / 47.0);
        let ray = vec2(angle.cos(), angle.sin());
        let color = if (tick as f64 + 0.5) < amount * 0.48 {
            accent
        } else {
            Color32::from_gray(67)
        };
        paint.line_segment(
            [center + ray * 65.0, center + ray * 75.0],
            Stroke::new(3.0_f32, color),
        );
    }
    paint.circle_filled(center, 27.0, Color32::from_rgb(36, 41, 45));
    paint.text(
        center,
        Align2::CENTER_CENTER,
        format!("{amount:.0}%"),
        t::mono(18.0),
        t::text(),
    );
    kit::caps(
        paint,
        center + vec2(0.0, 76.0),
        Align2::CENTER_CENTER,
        tr.fxv_amount,
        11.0,
        t::muted(),
    );
    for (i, (letter, name, _)) in modes.iter().enumerate() {
        let pos = panel.center_bottom() + vec2((i as f32 - 1.5) * 53.0, -43.0);
        let rect = Rect::from_center_size(pos, vec2(48.0, 29.0));
        let response = ui
            .put(
                rect,
                egui::Button::new(egui::RichText::new(*letter).font(t::mono(15.0)).color(
                    if i == selected {
                        Color32::BLACK
                    } else {
                        t::text()
                    },
                ))
                .fill(if i == selected {
                    accent
                } else {
                    Color32::from_rgb(51, 57, 62)
                }),
            )
            .on_hover_text(*name);
        if response.clicked() && selected != i {
            params[1] = i as f32;
            started = true;
            changed = true;
        }
    }
    ui.painter().text(
        panel.center_bottom() - vec2(0.0, 15.0),
        Align2::CENTER_CENTER,
        modes[selected].1,
        t::sans(13.0),
        t::text(),
    );
    (started, changed)
}

/// FL Gross Beat's time side: 16 steps of 1/16, each one a playback mode.
pub fn time(ui: &mut Ui, params: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    use super::kit;
    use keyframexl::fx::{TIME_MODES, TIME_STEPS};
    let colors = [
        Color32::from_rgb(64, 69, 74),
        t::enc_blue(),
        t::enc_green(),
        t::enc_orange(),
        Color32::from_rgb(211, 162, 242),
        Color32::from_rgb(120, 58, 52),
    ];
    let mut started = false;
    let mut changed = false;
    let mut steps = params[0].round().clamp(1.0, TIME_STEPS as f32) as usize;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr.fxv_loop)
                .font(t::sans(13.0))
                .color(t::muted()),
        );
        if kit::drag_value(
            ui,
            egui::DragValue::new(&mut steps)
                .range(1..=TIME_STEPS)
                .suffix(tr.fxv_steps_suffix),
        )
        .changed()
        {
            params[0] = steps as f32;
            started = true;
            changed = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button(tr.fxv_clear)
                .on_hover_text(tr.fxv_clear_tip)
                .clicked()
            {
                params[3..3 + TIME_STEPS].fill(0.0);
                started = true;
                changed = true;
            }
            if ui
                .small_button(tr.fxv_dice)
                .on_hover_text(tr.fxv_dice_tip)
                .clicked()
            {
                let mut seed = (ui.input(|i| i.time) * 1000.0) as u64 ^ 0x9e37_79b9;
                for i in 0..TIME_STEPS {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    // Mostly play, with dashes of the other modes.
                    let r = (seed >> 33) % 100;
                    params[3 + i] = if r < 55 {
                        0.0
                    } else if r < 68 {
                        4.0
                    } else if r < 78 {
                        5.0
                    } else if r < 88 {
                        1.0
                    } else if r < 96 {
                        2.0
                    } else {
                        3.0
                    };
                }
                started = true;
                changed = true;
            }
        });
    });
    let (face, _) =
        ui.allocate_exact_size(vec2(ui.available_width().max(300.0), 116.0), Sense::hover());
    ui.painter()
        .rect_filled(face, 6.0, Color32::from_rgb(27, 31, 35));
    let inner = face.shrink(8.0);
    ui.painter().text(
        inner.left_top(),
        egui::Align2::LEFT_TOP,
        tr.fxv_time_note,
        t::sans(10.5),
        t::faint(),
    );
    let (cols, rows, gap) = (8usize, 2usize, 3.0);
    let cw = (inner.width() - gap * (cols - 1) as f32) / cols as f32;
    let ch = (inner.height() - 16.0 - gap * (rows - 1) as f32) / rows as f32;
    for i in 0..TIME_STEPS {
        let rect = Rect::from_min_size(
            pos2(
                inner.left() + (i % cols) as f32 * (cw + gap),
                inner.top() + 16.0 + (i / cols) as f32 * (ch + gap),
            ),
            vec2(cw, ch),
        );
        let mode = (params[3 + i].round() as usize).min(TIME_MODES.len() - 1);
        let active = i < steps;
        let response = ui
            .interact(rect, ui.id().with(("time-step", i)), Sense::click())
            .on_hover_text(fill(
                tr.fxv_step_tip,
                &[
                    ("n", &(i + 1).to_string()),
                    ("mode", tr.fx_time_modes()[mode]),
                    ("hint", tr.fx_time_hints()[mode]),
                ],
            ));
        if response.clicked() {
            params[3 + i] = ((mode + 1) % TIME_MODES.len()) as f32;
            started = true;
            changed = true;
        }
        if response.secondary_clicked() {
            params[3 + i] = 0.0;
            started = true;
            changed = true;
        }
        let paint = ui.painter();
        let fill = if active {
            colors[mode].gamma_multiply(if mode == 0 { 0.55 } else { 0.28 })
        } else {
            Color32::from_rgb(22, 25, 28)
        };
        paint.rect_filled(rect, 4.0, fill);
        paint.rect_stroke(
            rect,
            4.0,
            Stroke::new(
                1.0_f32,
                if active {
                    colors[mode].gamma_multiply(0.8)
                } else {
                    t::edge()
                },
            ),
            egui::StrokeKind::Inside,
        );
        let ink = if active {
            colors[mode]
        } else {
            t::faint().gamma_multiply(0.5)
        };
        paint.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr.fx_time_modes()[mode],
            t::mono(9.5),
            ink,
        );
        paint.text(
            rect.left_top() + vec2(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            format!("{}", i + 1),
            t::mono(8.0),
            t::faint().gamma_multiply(0.7),
        );
    }
    let mut smooth = params[1] as f64;
    let mut slice = params[2] as f64;
    ui.horizontal(|ui| {
        for (v, spec, color, hint) in [
            (
                &mut smooth,
                keyframexl::fx::FxKind::Time.params()[1],
                t::enc_white(),
                tr.fxv_smooth_hint,
            ),
            (
                &mut slice,
                keyframexl::fx::FxKind::Time.params()[2],
                Color32::from_rgb(211, 162, 242),
                tr.fxv_slice_hint,
            ),
        ] {
            let fmt = |x: f64| keyframexl::fx::format(&spec, x as f32);
            let knob = kit::Knob {
                label: spec.name,
                color,
                min: spec.min as f64,
                max: spec.max as f64,
                default: spec.default as f64,
                log: spec.log,
                fmt: &fmt,
                hint,
            };
            let (_, s, c) = kit::knob(ui, &knob, v);
            started |= s;
            changed |= c;
        }
    });
    params[1] = smooth as f32;
    params[2] = slice as f32;
    (started, changed)
}

/// Three bands side by side, each with its threshold and ratio (FL: Maximus,
/// simplified). Shared attack/release/output below.
pub fn multiband(ui: &mut Ui, track: usize, slot: usize, params: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    use super::kit;
    let specs = keyframexl::fx::FxKind::Multiband.params();
    let mut started = false;
    let mut changed = false;
    let names = [tr.fxv_low, tr.fxv_mid, tr.fxv_high];
    let colors = [t::enc_orange(), t::enc_green(), t::enc_blue()];
    ui.horizontal(|ui| {
        for band in 0..3 {
            ui.vertical(|ui| {
                kit::caps(
                    ui.painter(),
                    ui.cursor().min + vec2(0.0, 8.0),
                    egui::Align2::LEFT_CENTER,
                    names[band],
                    10.5,
                    colors[band],
                );
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    for k in 0..2 {
                        let pi = band * 2 + k;
                        let spec = specs[pi];
                        let mut v = params[pi] as f64;
                        let fmt = |x: f64| keyframexl::fx::format(&spec, x as f32);
                        let knob = kit::Knob {
                            label: if k == 0 {
                                tr.fxv_threshold
                            } else {
                                tr.fxv_ratio
                            },
                            color: colors[band],
                            min: spec.min as f64,
                            max: spec.max as f64,
                            default: spec.default as f64,
                            log: spec.log,
                            fmt: &fmt,
                            hint: if k == 0 {
                                tr.fxv_threshold_hint
                            } else {
                                tr.fxv_ratio_hint
                            },
                        };
                        kit::automatable(
                            ui,
                            keyframexl::automation::Target::FxParam {
                                track: track as u16,
                                slot: slot as u8,
                                param: pi as u16,
                            },
                        );
                        let (_, s, c) = kit::knob(ui, &knob, &mut v);
                        started |= s;
                        changed |= c;
                        params[pi] = v as f32;
                    }
                });
            });
        }
    });
    ui.add_space(4.0);
    ui.separator();
    ui.horizontal(|ui| {
        for pi in 6..9 {
            let spec = specs[pi];
            let mut v = params[pi] as f64;
            let fmt = |x: f64| keyframexl::fx::format(&spec, x as f32);
            let knob = kit::Knob {
                label: spec.name,
                color: t::enc_white(),
                min: spec.min as f64,
                max: spec.max as f64,
                default: spec.default as f64,
                log: spec.log,
                fmt: &fmt,
                hint: spec.name,
            };
            kit::automatable(
                ui,
                keyframexl::automation::Target::FxParam {
                    track: track as u16,
                    slot: slot as u8,
                    param: pi as u16,
                },
            );
            let (_, s, c) = kit::knob(ui, &knob, &mut v);
            started |= s;
            changed |= c;
            params[pi] = v as f32;
        }
    });
    (started, changed)
}

// ---------------------------------------------------------------- faceplates

use keyframexl::fx::ParamSpec;

/// Travel position 0..1 for a value (the same curve the knobs use).
fn param_pos(spec: &ParamSpec, v: f32) -> f32 {
    if spec.log {
        if spec.min == 0.0 && spec.max > 0.0 {
            return (v.max(0.0) / spec.max).sqrt().clamp(0.0, 1.0);
        }
        let lo = spec.min.max(1e-4).ln();
        let hi = spec.max.ln();
        return ((v.max(spec.min.max(1e-4)).ln() - lo) / (hi - lo)).clamp(0.0, 1.0);
    }
    ((v - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)
}

/// A large instrument dial with a label and a live value.
fn face_dial(
    ui: &mut Ui,
    track: usize,
    slot: usize,
    pi: usize,
    spec: &ParamSpec,
    params: &mut [f32],
    color: Color32,
    size: f32,
) -> (bool, bool) {
    let (cell, _) = ui.allocate_exact_size(vec2(size + 18.0, size + 58.0), Sense::hover());
    let center = pos2(cell.center().x, cell.top() + size / 2.0 + 24.0);
    let dial = Rect::from_center_size(center, vec2(size, size));
    let mut v = params[pi] as f64;
    let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
    let knob = kit::Knob {
        label: spec.name,
        color,
        min: spec.min as f64,
        max: spec.max as f64,
        default: spec.default as f64,
        log: spec.log,
        fmt: &fmt,
        hint: spec.name,
    };
    kit::automatable(
        ui,
        keyframexl::automation::Target::FxParam {
            track: track as u16,
            slot: slot as u8,
            param: pi as u16,
        },
    );
    let (_, started, changed) = kit::mini_knob(ui, dial, ui.id().with(("face", pi)), &knob, &mut v);
    params[pi] = v as f32;
    let p = ui.painter();
    let frac = param_pos(spec, v as f32);
    let r = size / 2.0 + 2.0;
    for k in 0..25 {
        let a = std::f32::consts::PI * (0.8 + 1.4 * k as f32 / 24.0);
        let ray = vec2(a.cos(), a.sin());
        let on = k as f32 / 24.0 <= frac;
        p.line_segment(
            [center + ray * r, center + ray * (r + 3.0)],
            Stroke::new(2.0_f32, if on { color } else { Color32::from_gray(55) }),
        );
    }
    if size >= 54.0 {
        let label = spec.name.strip_prefix("Filter ").unwrap_or(spec.name);
        p.text(
            pos2(cell.center().x, cell.top() + 3.0),
            egui::Align2::CENTER_TOP,
            label.to_uppercase(),
            t::sans(9.0),
            t::muted(),
        );
    }
    p.text(
        pos2(cell.center().x, cell.bottom() - 1.0),
        egui::Align2::CENTER_BOTTOM,
        keyframexl::fx::format(spec, v as f32),
        t::mono(12.0),
        t::text(),
    );
    (started, changed)
}

/// A square X/Y pad driving two parameters.
fn xy_pad(
    ui: &mut Ui,
    track: usize,
    slot: usize,
    xpi: usize,
    ypi: usize,
    xspec: &ParamSpec,
    yspec: &ParamSpec,
    params: &mut [f32],
    color: Color32,
    title: &str,
) -> (bool, bool) {
    let size = 104.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size + 44.0), Sense::click_and_drag());
    let pad = Rect::from_min_size(rect.min, vec2(size, size));
    let mut nx = param_pos(xspec, params[xpi]);
    let mut ny = param_pos(yspec, params[ypi]);
    let mut started = false;
    let mut changed = false;
    if (resp.dragged() || resp.clicked())
        && let Some(q) = resp.interact_pointer_pos()
    {
        nx = ((q.x - pad.left()) / pad.width()).clamp(0.0, 1.0);
        ny = (1.0 - (q.y - pad.top()) / pad.height()).clamp(0.0, 1.0);
        params[xpi] = xspec.min + nx * (xspec.max - xspec.min);
        params[ypi] = yspec.min + ny * (yspec.max - yspec.min);
        started = resp.drag_started() || resp.clicked();
        changed = true;
    }
    let p = ui.painter();
    p.rect_filled(pad, 5.0, Color32::from_rgb(24, 28, 31));
    p.rect_stroke(
        pad,
        5.0,
        Stroke::new(1.0_f32, t::edge()),
        egui::StrokeKind::Inside,
    );
    for k in 1..4 {
        let x = pad.left() + pad.width() * k as f32 / 4.0;
        let y = pad.top() + pad.height() * k as f32 / 4.0;
        p.vline(
            x,
            pad.y_range(),
            Stroke::new(1.0_f32, Color32::from_gray(42)),
        );
        p.hline(
            pad.x_range(),
            y,
            Stroke::new(1.0_f32, Color32::from_gray(42)),
        );
    }
    let at = pos2(
        pad.left() + nx * pad.width(),
        pad.top() + (1.0 - ny) * pad.height(),
    );
    p.line_segment(
        [pos2(at.x, pad.top()), pos2(at.x, pad.bottom())],
        Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
    );
    p.line_segment(
        [pos2(pad.left(), at.y), pos2(pad.right(), at.y)],
        Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
    );
    p.circle_filled(at, 5.0, color);
    p.circle_stroke(at, 8.0, Stroke::new(1.0_f32, color.gamma_multiply(0.6)));
    kit::caps(
        p,
        pos2(pad.center().x, pad.bottom() + 6.0),
        egui::Align2::CENTER_TOP,
        title,
        9.5,
        t::muted(),
    );
    p.text(
        pos2(pad.center().x, pad.bottom() + 20.0),
        egui::Align2::CENTER_TOP,
        format!("X {}", keyframexl::fx::format(xspec, params[xpi])),
        t::mono(9.5),
        t::faint(),
    );
    p.text(
        pos2(pad.center().x, pad.bottom() + 32.0),
        egui::Align2::CENTER_TOP,
        format!("Y {}", keyframexl::fx::format(yspec, params[ypi])),
        t::mono(9.5),
        t::faint(),
    );
    let _ = (track, slot);
    (started, changed)
}

fn face_bg(ui: &mut Ui, height: f32) -> (Rect, egui::Painter) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(280.0), height),
        Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_rgb(23, 27, 30));
    p.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0_f32, t::edge()),
        egui::StrokeKind::Inside,
    );
    (rect, p)
}

fn curve_point(rect: Rect, x: f32, y: f32) -> egui::Pos2 {
    pos2(
        egui::lerp(rect.x_range(), x.clamp(0.0, 1.0)),
        egui::lerp(rect.y_range(), 1.0 - y.clamp(0.0, 1.0)),
    )
}

/// Fruity Filter: one big cutoff dial, a filter-type switch and a live curve.
pub fn filter(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Filter.params();
    let (mut started, mut changed) = (false, false);
    let kind = p[0].round() as usize;
    let fc = p[1];
    let res = p[2] / 100.0;
    let (face, paint) = face_bg(ui, 138.0);
    let plot = face.shrink2(vec2(14.0, 16.0));
    for k in 1..4 {
        let x = plot.left() + plot.width() * k as f32 / 4.0;
        paint.vline(
            x,
            plot.y_range(),
            Stroke::new(1.0_f32, Color32::from_gray(40)),
        );
    }
    for k in 1..3 {
        let y = plot.top() + plot.height() * k as f32 / 3.0;
        paint.hline(
            plot.x_range(),
            y,
            Stroke::new(1.0_f32, Color32::from_gray(40)),
        );
    }
    let curve: Vec<_> = (0..=96)
        .map(|i| {
            let f = 20.0 * 1000f32.powf(i as f32 / 96.0);
            let x = f / fc;
            let mut mag = match kind {
                1 => 1.0 / (1.0 + (x - 1.0).abs() * 4.0 * (1.0 - res * 0.7)),
                2 => x * x / (1.0 + x * x),
                _ => 1.0 / (1.0 + x * x),
            };
            mag *= 1.0 + res * 4.0 * (-((x.log2()) / 0.25).powi(2)).exp();
            let db = 20.0 * mag.max(1e-4).log10();
            curve_point(plot, i as f32 / 96.0, (db + 36.0) / 48.0)
        })
        .collect();
    paint.add(egui::Shape::line(
        curve,
        Stroke::new(2.0_f32, t::enc_blue()),
    ));
    let fpos = (fc / 20.0).log(1000.0);
    paint.vline(
        egui::lerp(plot.x_range(), fpos.clamp(0.0, 1.0)),
        plot.y_range(),
        Stroke::new(1.0_f32, t::wait().gamma_multiply(0.7)),
    );
    paint.text(
        plot.left_top() + vec2(0.0, -1.0),
        egui::Align2::LEFT_TOP,
        "20",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_top() + vec2(0.0, -1.0),
        egui::Align2::RIGHT_TOP,
        "20k",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_blue(), 78.0);
        started |= s;
        changed |= c;
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(tr.fxv_type)
                    .font(t::sans(10.0))
                    .color(t::faint()),
            );
            let mut t = kind.clamp(0, 2);
            for (i, name) in ["LP", "BP", "HP"].iter().enumerate() {
                if ui
                    .selectable_label(t == i, egui::RichText::new(*name).font(t::mono(12.0)))
                    .clicked()
                {
                    p[0] = i as f32;
                    started = true;
                    changed = true;
                    t = i;
                }
            }
            let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_white(), 54.0);
            started |= s;
            changed |= c;
        });
        ui.add_space(6.0);
        let (s, c) = face_dial(ui, track, slot, 3, &specs[3], p, t::enc_green(), 54.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Drive: a transfer curve on a hot plate, one big drive dial.
pub fn drive(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Drive.params();
    let (mut started, mut changed) = (false, false);
    let pre = 10f32.powf(p[0] / 20.0);
    let (face, paint) = face_bg(ui, 128.0);
    let plot = face.shrink2(vec2(18.0, 14.0));
    let mid = plot.center().y;
    paint.hline(
        plot.x_range(),
        mid,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    paint.vline(
        plot.center().x,
        plot.y_range(),
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    let line: Vec<_> = (0..=80)
        .map(|i| {
            let x = i as f32 / 80.0 * 2.0 - 1.0;
            let y = (x * pre).tanh() / pre.max(1.0).sqrt();
            pos2(
                egui::lerp(plot.x_range(), i as f32 / 80.0),
                mid - y * plot.height() * 0.42,
            )
        })
        .collect();
    paint.add(egui::Shape::line(
        line,
        Stroke::new(2.2_f32, t::enc_orange()),
    ));
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        "IN",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_bottom() + vec2(0.0, -3.0),
        egui::Align2::RIGHT_BOTTOM,
        "OUT",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_orange(), 78.0);
        started |= s;
        changed |= c;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (pi, color) in [(1, t::enc_white()), (2, t::enc_green())] {
                    let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 54.0);
                    started |= s;
                    changed |= c;
                }
            });
            let (s, c) = face_dial(ui, track, slot, 3, &specs[3], p, t::enc_blue(), 54.0);
            started |= s;
            changed |= c;
        });
    });
    (started, changed)
}

/// Chorus: orbiting delay lines, depth and rate up front.
pub fn chorus(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Chorus.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 122.0);
    let c = face.center();
    let r = 34.0;
    let t = ui.input(|i| i.time) as f32;
    paint.circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_gray(55)));
    for i in 0..3 {
        let phase = t * p[2] * std::f32::consts::TAU + i as f32 * 2.094;
        let dot = c + vec2(phase.cos(), phase.sin()) * (r + 10.0);
        paint.circle_filled(dot, 3.5, t::enc_blue().gamma_multiply(0.85));
        paint.line_segment(
            [c, dot],
            Stroke::new(1.0_f32, t::enc_blue().gamma_multiply(0.25)),
        );
    }
    paint.circle_filled(c, 5.0, t::enc_blue());
    paint.text(
        pos2(face.left() + 16.0, face.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{:.2} ms", p[0]),
        t::mono(12.0),
        t::muted(),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_blue(), 72.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_green(), 60.0);
        started |= s;
        changed |= c;
        ui.vertical(|ui| {
            let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_white(), 48.0);
            started |= s;
            changed |= c;
        });
        let (s, c) = face_dial(ui, track, slot, 3, &specs[3], p, t::enc_white(), 48.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Hyper Chorus: the FL three X/Y pads, line count and filters below.
pub fn hyper_chorus(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::HyperChorus.params();
    let (mut started, mut changed) = (false, false);
    ui.horizontal(|ui| {
        for (xpi, ypi, title, color) in [
            (0, 1, tr.fxv_delay, t::enc_blue()),
            (2, 3, tr.fxv_mod, t::enc_green()),
            (4, 5, tr.fxv_move, t::enc_orange()),
        ] {
            let (s, c) = xy_pad(
                ui,
                track,
                slot,
                xpi,
                ypi,
                &specs[xpi],
                &specs[ypi],
                p,
                color,
                title,
            );
            started |= s;
            changed |= c;
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        kit::caps(
            ui.painter(),
            ui.cursor().min + vec2(0.0, 10.0),
            egui::Align2::LEFT_CENTER,
            tr.fxv_lines,
            10.0,
            t::faint(),
        );
        ui.add_space(40.0);
        for n in 4..=8 {
            if ui
                .selectable_label(
                    p[6].round() as i32 == n,
                    egui::RichText::new(format!("{n}")).font(t::mono(12.0)),
                )
                .clicked()
            {
                p[6] = n as f32;
                started = true;
                changed = true;
            }
        }
    });
    for pair in [
        [(7, t::enc_white()), (8, t::enc_white())],
        [(9, t::enc_green()), (10, t::enc_blue())],
    ] {
        ui.horizontal(|ui| {
            for (pi, color) in pair {
                let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 54.0);
                started |= s;
                changed |= c;
            }
        });
    }
    (started, changed)
}

/// Squeeze: the puncher's keep/hit pattern and a bit staircase.
pub fn squeeze(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Squeeze.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 104.0);
    let plot = face.shrink2(vec2(12.0, 14.0));
    let keep = (p[1] / 100.0 * 60.0).round().max(0.0) as usize;
    let hit = match p[3].round() as usize {
        1 => keep,
        2 => keep * 2,
        3 => keep / 2,
        _ => (p[2] / 100.0 * 60.0).round().max(0.0) as usize,
    };
    let period = (keep + hit).max(1);
    for i in 0..60 {
        let x = plot.left() + plot.width() * i as f32 / 60.0;
        let in_keep = i % period < keep;
        let h = if in_keep { 0.7 } else { p[4] / 100.0 * 0.7 };
        paint.rect_filled(
            Rect::from_min_max(
                pos2(x, plot.bottom() - plot.height() * h),
                pos2(x + plot.width() / 62.0, plot.bottom()),
            ),
            0.0,
            if in_keep {
                t::enc_green().gamma_multiply(0.6)
            } else {
                t::wait().gamma_multiply(0.8)
            },
        );
    }
    let bits = 16.0 - (p[0] / 100.0) * 15.0;
    let steps = 2f32.powf(bits.min(5.0));
    let line: Vec<_> = (0..=80)
        .map(|i| {
            let x = i as f32 / 80.0;
            let y = ((x * 2.0 - 1.0) * steps).round() / steps;
            curve_point(plot, x, (y + 1.0) * 0.5)
        })
        .collect();
    paint.add(egui::Shape::line(
        line,
        Stroke::new(1.6_f32, t::text().gamma_multiply(0.7)),
    ));
    paint.text(
        plot.left_top() + vec2(0.0, 2.0),
        egui::Align2::LEFT_TOP,
        format!("{bits:.0} bit"),
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (pi, color) in [
            (0, t::enc_white()),
            (4, t::wait()),
            (1, t::enc_white()),
            (2, t::enc_white()),
        ] {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 54.0);
            started |= s;
            changed |= c;
        }
    });
    ui.horizontal(|ui| {
        for (pi, color) in [
            (5, t::enc_green()),
            (6, t::enc_white()),
            (7, t::enc_blue()),
            (8, t::enc_blue()),
        ] {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 54.0);
            started |= s;
            changed |= c;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr.fxv_relation)
                .font(t::sans(10.0))
                .color(t::faint()),
        );
        ui.add_space(6.0);
        let mut rel = p[3].round() as usize;
        for (i, name) in [
            tr.fxv_rel_none,
            tr.fxv_rel_equal,
            tr.fxv_rel_double,
            tr.fxv_rel_half,
        ]
        .iter()
        .enumerate()
        {
            if ui
                .selectable_label(rel == i, egui::RichText::new(*name).font(t::mono(11.0)))
                .clicked()
            {
                p[3] = i as f32;
                started = true;
                changed = true;
                rel = i;
            }
        }
        ui.add_space(8.0);
        let mut pre = p[9] >= 0.5;
        if ui.checkbox(&mut pre, tr.fxv_pre).changed() {
            p[9] = if pre { 1.0 } else { 0.0 };
            started = true;
            changed = true;
        }
    });
    (started, changed)
}

/// Stereo: a vectorscope and three dials.
pub fn stereo(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Stereo.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 132.0);
    let c = face.center();
    let r = 46.0;
    paint.circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_gray(60)));
    paint.line_segment(
        [c + vec2(-r, r), c + vec2(r, -r)],
        Stroke::new(1.0_f32, Color32::from_gray(42)),
    );
    paint.line_segment(
        [c + vec2(-r, -r), c + vec2(r, r)],
        Stroke::new(1.0_f32, Color32::from_gray(42)),
    );
    let w = (p[0] / 100.0).clamp(0.0, 2.0);
    let bal = p[1] / 100.0;
    let a = 0.35 + 0.55 * w.min(1.0);
    let tilt = bal * 0.6;
    let pts: Vec<_> = (0..=64)
        .map(|i| {
            let t = i as f32 / 64.0 * std::f32::consts::TAU;
            let x = t.sin() * a;
            let y = (t + tilt).cos() * (1.35 - a).clamp(0.2, 1.2);
            c + vec2(x * r, -y * r)
        })
        .collect();
    paint.add(egui::Shape::line(pts, Stroke::new(2.0_f32, t::enc_green())));
    paint.text(
        c + vec2(0.0, r + 10.0),
        egui::Align2::CENTER_TOP,
        "L",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        c + vec2(r + 8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "R",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_green(), 74.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_white(), 58.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_blue(), 58.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Balance: two level bars and a big pan dial.
pub fn balance(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Balance.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 92.0);
    let plot = face.shrink2(vec2(24.0, 14.0));
    let pan = (p[1] / 100.0).clamp(-1.0, 1.0);
    let vol = p[0] / 100.0;
    let lg = vol * if pan < 0.0 { 1.0 } else { 1.0 - pan };
    let rg = vol * if pan > 0.0 { 1.0 } else { 1.0 + pan };
    for (i, (g, name)) in [(lg, "L"), (rg, "R")].into_iter().enumerate() {
        let x = plot.left() + plot.width() * (0.3 + 0.4 * i as f32);
        let h = plot.height() * g.clamp(0.0, 1.0) * 0.75;
        paint.rect_filled(
            Rect::from_min_max(
                pos2(x - 22.0, plot.bottom() - h),
                pos2(x + 22.0, plot.bottom()),
            ),
            3.0,
            if i == 0 {
                t::enc_blue()
            } else {
                t::enc_green()
            }
            .gamma_multiply(0.6),
        );
        paint.text(
            pos2(x, plot.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            name,
            t::mono(9.0),
            t::faint(),
        );
    }
    let at = plot.center().x + pan * plot.width() * 0.35;
    paint.vline(at, plot.y_range(), Stroke::new(1.5_f32, t::wait()));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_white(), 76.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_blue(), 76.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Mid/Side: the mid and side levels as two bars, width and mono below.
pub fn mid_side(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::MidSide.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 92.0);
    let plot = face.shrink2(vec2(24.0, 14.0));
    let side = (p[0] / 100.0).clamp(0.0, 2.0);
    for (i, (g, name, color)) in [
        (1.0f32, tr.fxv_mid, t::enc_blue()),
        (side, tr.fxv_side, t::enc_green()),
    ]
    .into_iter()
    .enumerate()
    {
        let x = plot.left() + plot.width() * (0.3 + 0.4 * i as f32);
        let h = plot.height() * (g / 2.0).clamp(0.0, 1.0) * 0.8;
        paint.rect_filled(
            Rect::from_min_max(
                pos2(x - 26.0, plot.bottom() - h),
                pos2(x + 26.0, plot.bottom()),
            ),
            3.0,
            color.gamma_multiply(0.6),
        );
        paint.text(
            pos2(x, plot.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            name,
            t::mono(9.0),
            t::faint(),
        );
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_green(), 72.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_blue(), 54.0);
        started |= s;
        changed |= c;
        ui.vertical(|ui| {
            let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_white(), 48.0);
            started |= s;
            changed |= c;
        });
        let (s, c) = face_dial(ui, track, slot, 3, &specs[3], p, t::enc_white(), 48.0);
        started |= s;
        changed |= c;
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr.fxv_solo)
                .font(t::sans(10.0))
                .color(t::faint()),
        );
        let mut solo = p[4].round() as usize;
        for (i, name) in [tr.fxv_solo_off, tr.fxv_solo_mid, tr.fxv_solo_side]
            .iter()
            .enumerate()
        {
            if ui
                .selectable_label(solo == i, egui::RichText::new(*name).font(t::mono(11.0)))
                .clicked()
            {
                p[4] = i as f32;
                started = true;
                changed = true;
                solo = i;
            }
        }
    });
    (started, changed)
}

/// Phaser: the notches sweeping across the spectrum.
pub fn phaser(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Phaser.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 108.0);
    let plot = face.shrink2(vec2(16.0, 18.0));
    let t = ui.input(|i| i.time) as f32;
    let depth = p[1] / 100.0;
    let line: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f32 / 120.0;
            let f = 100.0 * 100f32.powf(x);
            let mut y = 1.0f32;
            for k in 0..4 {
                let pos = (t * p[0] + k as f32 * 0.25).fract();
                let cf = 100.0 * 100f32.powf(pos);
                let d = (f / cf).log10();
                y -= depth * 0.5 * (-(d / 0.09).powi(2)).exp();
            }
            curve_point(plot, x, y * 0.8 + 0.1)
        })
        .collect();
    paint.add(egui::Shape::line(
        line,
        Stroke::new(2.0_f32, t::enc_orange()),
    ));
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        "100 Hz",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_top(),
        egui::Align2::RIGHT_TOP,
        "10 kHz",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_orange(), 72.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_green(), 60.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_white(), 60.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// The kinds drawn by a faceplate instead of the generic knob grid.
pub fn has_face(kind: FxKind) -> bool {
    matches!(
        kind,
        FxKind::Filter
            | FxKind::Drive
            | FxKind::Chorus
            | FxKind::HyperChorus
            | FxKind::Squeeze
            | FxKind::Stereo
            | FxKind::Balance
            | FxKind::MidSide
            | FxKind::Phaser
            | FxKind::Eq
            | FxKind::AutoPan
            | FxKind::Tremolo
            | FxKind::Bitcrusher
            | FxKind::NoiseGate
            | FxKind::RingMod
            | FxKind::TiltEq
            | FxKind::Wavefolder
            | FxKind::Transient
            | FxKind::Vocoder
            | FxKind::Convolution
            | FxKind::StereoShaper
    )
}

// ------------------------------------------------------- remaining faceplates

/// 3-band EQ: three console columns over a response curve.
pub fn eq3(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Eq.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 132.0);
    let plot = face.shrink2(vec2(16.0, 14.0));
    paint.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    let line: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f32 / 120.0;
            let f = 20.0 * 1000f32.powf(x);
            let low = p[1] / (1.0 + (f / p[0]).powi(2));
            let mid = p[3] * (-((f / p[2]).log2() / 0.55).powi(2)).exp();
            let high = p[6] * (f / p[5]).powi(2) / (1.0 + (f / p[5]).powi(2));
            let db = low + mid + high;
            curve_point(plot, x, (db + 18.0) / 36.0)
        })
        .collect();
    paint.add(egui::Shape::line(
        line,
        Stroke::new(2.0_f32, t::enc_green()),
    ));
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        "20",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_top(),
        egui::Align2::RIGHT_TOP,
        "20k",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (name, color, idxs) in [
            (tr.fxv_low, t::enc_orange(), vec![0usize, 1]),
            (tr.fxv_mid, t::enc_green(), vec![2, 3, 4]),
            (tr.fxv_high, t::enc_blue(), vec![5, 6]),
        ] {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(name).font(t::sans(9.0)).color(color));
                for pi in idxs {
                    let size = if pi == 4 { 44.0 } else { 52.0 };
                    let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, size);
                    started |= s;
                    changed |= c;
                }
            });
        }
    });
    (started, changed)
}

/// AutoPan: the signal drifting across the stereo field.
pub fn auto_pan(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::AutoPan.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 100.0);
    let plot = face.shrink2(vec2(22.0, 22.0));
    paint.line_segment(
        [plot.left_center(), plot.right_center()],
        Stroke::new(1.0_f32, Color32::from_gray(60)),
    );
    for (x, name) in [
        (plot.left(), "L"),
        (plot.center().x, "C"),
        (plot.right(), "R"),
    ] {
        paint.text(
            pos2(x, plot.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            name,
            t::mono(9.0),
            t::faint(),
        );
    }
    let t = ui.input(|i| i.time) as f32 * p[0] * std::f32::consts::TAU;
    let pts: Vec<_> = (0..=120)
        .map(|i| {
            let ph = t + i as f32 / 120.0 * std::f32::consts::TAU * 2.0;
            let x = (ph.sin() * p[1] / 100.0) * 0.5 + 0.5;
            curve_point(plot, x, 0.5 + ph.cos() * 0.08)
        })
        .collect();
    paint.add(egui::Shape::line(pts, Stroke::new(1.5_f32, t::enc_green())));
    let x = (t.sin() * p[1] / 100.0) * 0.5 + 0.5;
    paint.circle_filled(
        pos2(egui::lerp(plot.x_range(), x), plot.center().y),
        5.0,
        t::enc_green(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..2 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_green(), 72.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Tremolo: the amplitude envelope itself.
pub fn tremolo(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Tremolo.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 100.0);
    let plot = face.shrink2(vec2(18.0, 16.0));
    paint.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    let t = ui.input(|i| i.time) as f32;
    let pts: Vec<_> = (0..=160)
        .map(|i| {
            let x = i as f32 / 160.0;
            let ph = (t * p[0] + x * 2.0) * std::f32::consts::TAU;
            let shape = p[2] / 100.0;
            let tri = 1.0
                - 2.0
                    * (ph / std::f32::consts::TAU)
                        .fract()
                        .mul_add(2.0, -1.0)
                        .abs();
            let sq = if ph.sin() >= 0.0 { 1.0 } else { -1.0 };
            let wave = ph.sin() * (1.0 - shape) + tri * shape * 0.5 + sq * shape * 0.5;
            let y = 0.5 + wave * p[1] / 100.0 * 0.45;
            curve_point(plot, x, y)
        })
        .collect();
    paint.add(egui::Shape::line(pts, Stroke::new(2.0_f32, t::enc_green())));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..3 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_green(), 64.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Bitcrusher: a stepped sine showing the bit depth and clock.
pub fn bitcrusher(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Bitcrusher.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 108.0);
    let plot = face.shrink2(vec2(18.0, 14.0));
    let bits = p[0].round().clamp(2.0, 16.0);
    let levels = 2f32.powf(bits - 1.0);
    let hold = (48000.0 / p[1]).clamp(1.0, 80.0) as usize;
    let mut pts = Vec::new();
    for i in 0..=160 {
        let x = i as f32 / 160.0;
        let src = (x * 4.0 * std::f32::consts::TAU).sin();
        let idx = (i / hold.max(1)) * hold.max(1);
        let held = (idx as f32 / 160.0 * 4.0 * std::f32::consts::TAU).sin();
        let q = (held * levels).round() / levels;
        pts.push(curve_point(plot, x, 0.5 + q * 0.36));
        let _ = src;
    }
    paint.add(egui::Shape::line(
        pts,
        Stroke::new(2.0_f32, t::enc_orange()),
    ));
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        format!("{bits:.0} bit"),
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_white(), 72.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_blue(), 60.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_green(), 60.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// NoiseGate: a level meter with the threshold and a state light.
pub fn noise_gate(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::NoiseGate.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 96.0);
    let plot = face.shrink2(vec2(18.0, 20.0));
    let thr = (p[0] / 80.0).clamp(0.0, 1.0);
    let t = ui.input(|i| i.time) as f32;
    for i in 0..40 {
        let x = plot.left() + plot.width() * i as f32 / 40.0;
        let level = (0.5 + 0.5 * (t * 2.0 + i as f32 * 0.4).sin()).max(0.0);
        let open = level > thr;
        paint.rect_filled(
            Rect::from_min_max(
                pos2(x, plot.bottom() - plot.height() * level * 0.9),
                pos2(x + plot.width() / 44.0, plot.bottom()),
            ),
            0.0,
            if open {
                t::enc_green()
            } else {
                Color32::from_gray(55)
            },
        );
    }
    let y = plot.bottom() - plot.height() * thr * 0.9;
    paint.hline(plot.x_range(), y, Stroke::new(1.5_f32, t::wait()));
    paint.text(
        pos2(plot.right(), y - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        keyframexl::fx::format(&specs[0], p[0]),
        t::mono(9.0),
        t::wait(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..3 {
            let color = if pi == 0 {
                t::enc_green()
            } else {
                t::enc_white()
            };
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 64.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Ring modulator: carrier and product.
pub fn ring_mod(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::RingMod.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 100.0);
    let plot = face.shrink2(vec2(18.0, 18.0));
    let t = ui.input(|i| i.time) as f32;
    let carrier = (t * p[0] * 0.2).sin();
    let dry: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f32 / 120.0;
            curve_point(plot, x, 0.5 + (x * 4.0 * std::f32::consts::TAU).sin() * 0.3)
        })
        .collect();
    paint.add(egui::Shape::line(
        dry,
        Stroke::new(1.0_f32, Color32::from_gray(80)),
    ));
    let wet: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f32 / 120.0;
            let s = (x * 4.0 * std::f32::consts::TAU).sin()
                * (x * 4.0 * std::f32::consts::TAU + carrier * 3.0).sin();
            curve_point(plot, x, 0.5 + s * 0.34 * p[1] / 100.0)
        })
        .collect();
    paint.add(egui::Shape::line(
        wet,
        Stroke::new(1.8_f32, Color32::from_rgb(198, 150, 237)),
    ));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..2 {
            let (s, c) = face_dial(
                ui,
                track,
                slot,
                pi,
                &specs[pi],
                p,
                Color32::from_rgb(198, 150, 237),
                72.0,
            );
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Tilt EQ: the tilt line around a pivot.
pub fn tilt_eq(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::TiltEq.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 100.0);
    let plot = face.shrink2(vec2(18.0, 16.0));
    paint.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    let pivot = (p[1] / 20.0).ln() / (1000.0f32).ln();
    let pts = [
        curve_point(plot, 0.0, 0.5 - p[0] / 24.0),
        curve_point(plot, pivot.clamp(0.0, 1.0), 0.5),
        curve_point(plot, 1.0, 0.5 + p[0] / 24.0),
    ];
    paint.add(egui::Shape::line(
        pts.to_vec(),
        Stroke::new(2.0_f32, t::enc_blue()),
    ));
    paint.circle_filled(pts[1], 4.0, t::wait());
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        "20",
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_top(),
        egui::Align2::RIGHT_TOP,
        "20k",
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..2 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_blue(), 72.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Wavefolder: the fold transfer, not a smooth saturation.
pub fn wavefolder(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::Wavefolder.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 112.0);
    let plot = face.shrink2(vec2(22.0, 14.0));
    paint.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    paint.vline(
        plot.center().x,
        plot.y_range(),
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    let pre = 10f32.powf(p[0] / 20.0);
    let line: Vec<_> = (0..=160)
        .map(|i| {
            let x = i as f32 / 160.0 * 2.0 - 1.0;
            let y = 1.0 - ((x * pre + 1.0).rem_euclid(4.0) - 2.0).abs();
            pos2(
                egui::lerp(plot.x_range(), i as f32 / 160.0),
                plot.center().y - y * plot.height() * 0.4,
            )
        })
        .collect();
    paint.add(egui::Shape::line(
        line,
        Stroke::new(2.0_f32, Color32::from_rgb(247, 134, 142)),
    ));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(
            ui,
            track,
            slot,
            0,
            &specs[0],
            p,
            Color32::from_rgb(247, 134, 142),
            72.0,
        );
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_white(), 60.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Transient: the attack and the body as an envelope.
pub fn transient(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Transient.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 112.0);
    let plot = face.shrink2(vec2(18.0, 16.0));
    let attack = p[0] / 100.0;
    let sustain = p[1] / 100.0;
    let pts: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f32 / 120.0;
            let body = 0.35 + sustain * 0.25;
            let head = (-x * 14.0).exp() * (0.65 + attack * 0.5);
            curve_point(plot, x, (body + head).clamp(0.05, 0.95))
        })
        .collect();
    paint.add(egui::Shape::line(
        pts,
        Stroke::new(2.0_f32, t::enc_orange()),
    ));
    paint.text(
        plot.left_bottom() + vec2(0.0, 2.0),
        egui::Align2::LEFT_TOP,
        tr.fxv_hit,
        t::mono(9.0),
        t::faint(),
    );
    paint.text(
        plot.right_bottom() + vec2(0.0, 2.0),
        egui::Align2::RIGHT_TOP,
        tr.fxv_body,
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (s, c) = face_dial(ui, track, slot, 0, &specs[0], p, t::enc_orange(), 72.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 1, &specs[1], p, t::enc_green(), 64.0);
        started |= s;
        changed |= c;
        let (s, c) = face_dial(ui, track, slot, 2, &specs[2], p, t::enc_white(), 54.0);
        started |= s;
        changed |= c;
    });
    (started, changed)
}

/// Vocoder: the band bank lit by the modulator.
pub fn vocoder(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Vocoder.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 112.0);
    let plot = face.shrink2(vec2(14.0, 16.0));
    let bands = p[0].round().clamp(4.0, 16.0) as usize;
    for i in 0..16 {
        let x = plot.left() + plot.width() * (i as f32 + 0.5) / 16.0;
        let active = i < bands;
        let h = if active {
            (0.25 + 0.6 * (-((i as f32 - 7.0) / 5.0).powi(2)).exp())
                * (0.8 + 0.2 * (i as f32 * 1.7).sin())
        } else {
            0.06
        };
        paint.rect_filled(
            Rect::from_min_max(
                pos2(x - plot.width() / 40.0, plot.bottom() - plot.height() * h),
                pos2(x + plot.width() / 40.0, plot.bottom()),
            ),
            0.0,
            if active {
                Color32::from_rgb(211, 162, 242)
            } else {
                Color32::from_gray(48)
            },
        );
    }
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        fill(tr.fxv_bands, &[("n", &bands.to_string())]),
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..4 {
            let color = if pi == 0 {
                Color32::from_rgb(211, 162, 242)
            } else {
                t::enc_white()
            };
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, color, 54.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Convolution: the impulse response's own decay.
pub fn convolution(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let specs = FxKind::Convolution.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 100.0);
    let plot = face.shrink2(vec2(16.0, 14.0));
    paint.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0_f32, Color32::from_gray(50)),
    );
    let decay = 2.8f32;
    let pts: Vec<_> = (0..=140)
        .map(|i| {
            let x = i as f32 / 140.0;
            let env = (-decay * x).exp();
            let noise = (x * 900.0).sin() * (x * 157.0).cos();
            curve_point(plot, x, 0.5 + env * noise * 0.36)
        })
        .collect();
    paint.add(egui::Shape::line(pts, Stroke::new(1.2_f32, t::enc_blue())));
    paint.text(
        plot.left_top(),
        egui::Align2::LEFT_TOP,
        tr.fxv_impulse,
        t::mono(9.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..3 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_blue(), 64.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}

/// Stereo Shaper: the 2x2 matrix as a grid, then delay and phase.
pub fn stereo_shaper(ui: &mut Ui, track: usize, slot: usize, p: &mut [f32]) -> (bool, bool) {
    let specs = FxKind::StereoShaper.params();
    let (mut started, mut changed) = (false, false);
    let (face, paint) = face_bg(ui, 88.0);
    let plot = face.shrink2(vec2(18.0, 12.0));
    let cell = vec2(plot.width() / 2.0 - 4.0, plot.height() / 2.0 - 4.0);
    for r in 0..2 {
        for c in 0..2 {
            let rect = Rect::from_min_size(
                pos2(
                    plot.left() + c as f32 * (cell.x + 8.0),
                    plot.top() + r as f32 * (cell.y + 8.0),
                ),
                cell,
            );
            paint.rect_filled(rect, 3.0, Color32::from_rgb(33, 39, 43));
            paint.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0_f32, t::edge()),
                egui::StrokeKind::Inside,
            );
            let pi = match (r, c) {
                (0, 0) => 0,
                (0, 1) => 2,
                (1, 0) => 3,
                _ => 1,
            };
            let names = [["L → L", "L → R"], ["R → L", "R → R"]];
            paint.text(
                rect.left_top() + vec2(4.0, 3.0),
                egui::Align2::LEFT_TOP,
                names[r][c],
                t::sans(8.0),
                t::faint(),
            );
            paint.text(
                rect.center() + vec2(0.0, 5.0),
                egui::Align2::CENTER_CENTER,
                keyframexl::fx::format(&specs[pi], p[pi]),
                t::mono(11.0),
                if pi == 3 { t::enc_orange() } else { t::text() },
            );
        }
    }
    paint.text(
        plot.left_top() + vec2(0.0, -10.0),
        egui::Align2::LEFT_TOP,
        "L",
        t::mono(8.0),
        t::faint(),
    );
    paint.text(
        plot.right_top() + vec2(0.0, -10.0),
        egui::Align2::RIGHT_TOP,
        "R",
        t::mono(8.0),
        t::faint(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for pi in 0..4 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_white(), 50.0);
            started |= s;
            changed |= c;
        }
    });
    ui.horizontal(|ui| {
        for pi in 4..6 {
            let (s, c) = face_dial(ui, track, slot, pi, &specs[pi], p, t::enc_blue(), 54.0);
            started |= s;
            changed |= c;
        }
    });
    (started, changed)
}
