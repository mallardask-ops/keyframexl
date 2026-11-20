//! KeyFrame Tide editor: an abyssal slate & cyan wavetable/spectral cockpit.
//! Hero oscilloscope stage (3D topographic wavetable + interactive filter node)
//! on top, a tabbed studio deck (Envelopes / LFO & Mod / Effects / Voices & Macros)
//! in the middle, and pinned performance macros + audition keyboard across the bottom.

use super::{App, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use keyframexl::{
    sampler::Adsr,
    sequence::InstrumentKind,
    synth::{
        Filter, Lfo, LfoDivision, LfoShape, ModRoute, ModSource, ModTarget, Param, SynthPatch,
        Table, TableBank, Warp,
    },
};

/// Two columns from this width on, one below it.
const TWO_COLUMNS: f32 = 740.0;

pub(super) fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}
pub(super) fn signed(v: f64) -> String {
    if v.abs() < 0.005 {
        "0%".into()
    } else {
        format!("{:+.0}%", v * 100.0)
    }
}
fn hz(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}
pub(super) fn secs(v: f64) -> String {
    if v <= 0.0005 {
        "0".into()
    } else if v < 1.0 {
        format!("{:.0} ms", v * 1000.0)
    } else {
        format!("{v:.2} s")
    }
}

fn table_name(v: Table) -> &'static str {
    match v {
        Table::SineSaw => "Sine-Saw",
        Table::SquarePulse => "Square",
        Table::Organ => "Organ",
        Table::Vowel => "Vowel",
    }
}
fn warp_name(v: Warp) -> &'static str {
    match v {
        Warp::Bend => "Bend",
        Warp::Sync => "Sync",
        Warp::PwmMirror => "Mirror",
        Warp::FmSub => "FM Sub",
    }
}
fn param_name(v: Param) -> &'static str {
    match v {
        Param::Position => "Position",
        Param::Warp => "Warp",
        Param::Cutoff => "Cutoff",
        Param::Resonance => "Resonance",
        Param::Sub => "Sub",
        Param::Noise => "Noise",
        Param::Detune => "Detune",
        Param::Spread => "Spread",
    }
}
const TABLES: [Table; 4] = [
    Table::SineSaw,
    Table::SquarePulse,
    Table::Organ,
    Table::Vowel,
];
const WARPS: [Warp; 4] = [Warp::Bend, Warp::Sync, Warp::PwmMirror, Warp::FmSub];
const FILTERS: [Filter; 3] = [Filter::LowPass, Filter::HighPass, Filter::BandPass];
const PARAMS: [Param; 8] = [
    Param::Position,
    Param::Warp,
    Param::Cutoff,
    Param::Resonance,
    Param::Sub,
    Param::Noise,
    Param::Detune,
    Param::Spread,
];
const SHAPES: [LfoShape; 6] = [
    LfoShape::Off,
    LfoShape::Sine,
    LfoShape::Triangle,
    LfoShape::Saw,
    LfoShape::Square,
    LfoShape::SampleAndHold,
];
const DIVISIONS: [LfoDivision; 6] = [
    LfoDivision::Whole,
    LfoDivision::Half,
    LfoDivision::Quarter,
    LfoDivision::Eighth,
    LfoDivision::Sixteenth,
    LfoDivision::ThirtySecond,
];

/// Transient UI state for one Tide window, kept in egui temp data.
#[derive(Clone)]
struct TideView {
    /// 0 = Osc A, 1 = Osc B, 2 = Osc C (Spectral)
    osc_tab: usize,
    /// 0 = Envelopes, 1 = LFO & Mod, 2 = Effects, 3 = Voices & Macros
    deck_tab: usize,
    /// Active LFO tab (0 or 1)
    lfo_tab: usize,
    /// Remembered non-zero levels when muting oscillators via their LED toggle.
    saved_levels: [f64; 3],
    /// A/B comparison slot: 0 = A active, 1 = B active, plus the inactive snapshot.
    ab_slot: u8,
    ab_other: Option<SynthPatch>,
    /// Mini-keyboard base octave (default 3 => C3..B4).
    kb_octave: i8,
    /// Seed for deterministic-feeling musical patch mutation.
    mutate_seq: u64,
}

impl Default for TideView {
    fn default() -> Self {
        let osc_tab = std::env::var("KFXL_CAPTURE_TIDE_OSC")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(0, |v| v.min(2));
        let deck_tab = std::env::var("KFXL_CAPTURE_TIDE_DECK")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(0, |v| v.min(3));
        Self {
            osc_tab,
            deck_tab,
            lfo_tab: 0,
            saved_levels: [1.0, 0.7, 0.75],
            ab_slot: 0,
            ab_other: None,
            kb_octave: 3,
            mutate_seq: 1,
        }
    }
}

/// What one edit did this frame: a gesture started (undo step), a value changed.
#[derive(Default)]
pub(super) struct Edit {
    pub(super) started: bool,
    pub(super) changed: bool,
}

impl Edit {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn knob(
        &mut self,
        ui: &mut Ui,
        label: &str,
        color: Color32,
        value: &mut f64,
        (min, max, default): (f64, f64, f64),
        log: bool,
        fmt: &dyn Fn(f64) -> String,
        hint: &str,
    ) {
        let (_, s, c) = kit::knob(
            ui,
            &kit::Knob {
                label,
                color,
                min,
                max,
                default,
                log,
                fmt,
                hint,
            },
            value,
        );
        self.started |= s;
        self.changed |= c;
    }
}

/// Shared section card used by Forge and Vocal editors.
pub(super) fn panel(ui: &mut Ui, title: &str, color: Color32, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t::surface())
        .stroke(Stroke::new(1.0_f32, t::edge()))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(
                Rect::from_min_size(pos2(r.left(), r.center().y - 3.0), vec2(6.0, 6.0)),
                1.0,
                color,
            );
            kit::caps(
                p,
                pos2(r.left() + 14.0, r.center().y),
                Align2::LEFT_CENTER,
                title,
                11.0,
                t::muted(),
            );
            body(ui);
        });
}

/// Draw column 0 and 1 side by side when there is room, else stacked.
pub(super) fn pair(ui: &mut Ui, mut column: impl FnMut(&mut Ui, usize)) {
    if ui.available_width() >= TWO_COLUMNS {
        ui.columns(2, |c| {
            column(&mut c[0], 0);
            column(&mut c[1], 1);
        });
    } else {
        column(ui, 0);
        column(ui, 1);
    }
}

// ---------------------------------------------------------------- Tide primitives

/// Recessed deep-slate oscilloscope well unique to KeyFrame Tide.
fn tide_well(p: &egui::Painter, rect: Rect, radius: f32) -> Rect {
    p.rect_filled(rect, radius, t::edge());
    let well = rect.shrink(1.0);
    let inner = (radius - 1.0).max(1.0);
    p.rect_filled(well, inner, t::TIDE_WELL);
    p.rect_stroke(
        well,
        inner,
        Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.75)),
        egui::StrokeKind::Inside,
    );
    let y = well.top() + 0.5;
    p.line_segment(
        [
            pos2(well.left() + inner * 0.6, y),
            pos2(well.right() - inner * 0.6, y),
        ],
        Stroke::new(1.0_f32, t::low()),
    );
    well
}

/// Tide module bay container (anodized slate-blue panel with crisp seam).
fn tide_bay(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t::TIDE_DECK)
        .stroke(Stroke::new(1.0_f32, t::TIDE_LINE))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            body(ui);
        });
}

/// Compact hardware LED toggle pill (Tide-styled, avoids overusing brand red).
fn tide_toggle(ui: &mut Ui, label: &str, on: bool, accent: Color32, hint: &str) -> egui::Response {
    let font = t::mono(12.0);
    let tw = kit::text_width(ui.painter(), label, font.clone());
    let size = vec2(tw + 26.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let fill = if on {
        t::TIDE_DECK_HI
    } else if resp.hovered() {
        t::TIDE_DECK_HI.gamma_multiply(0.7)
    } else {
        t::TIDE_WELL
    };
    let border = if on {
        accent.gamma_multiply(0.75)
    } else if resp.hovered() {
        t::TIDE_LINE
    } else {
        t::edge()
    };
    p.rect_filled(rect, 5.0, fill);
    p.rect_stroke(
        rect,
        5.0,
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    kit::led(
        p,
        pos2(rect.left() + 10.0, rect.center().y),
        3.2,
        on.then_some(accent),
    );
    p.text(
        pos2(rect.left() + 19.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        if on { t::text() } else { t::TIDE_MIST },
    );
    if kit::show_focus(ui, &resp) {
        kit::focus_ring(p, rect, 5.0);
    }
    kit::tip(resp, hint)
}

/// Compact segmented selector specifically tuned for Tide's dense oscilloscope headers.
fn tide_pills(
    ui: &mut Ui,
    id: &str,
    items: &[&str],
    selected: usize,
    accent: Color32,
) -> Option<usize> {
    let font = t::sans(12.5);
    let ws: Vec<f32> = items
        .iter()
        .map(|s| kit::text_width(ui.painter(), s, font.clone()) + 16.0)
        .collect();
    let total = ws.iter().sum::<f32>() + 4.0;
    let h = 24.0;
    let (track, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(track, 6.0, t::TIDE_WELL);
    p.rect_stroke(
        track,
        6.0,
        Stroke::new(1.0_f32, t::TIDE_LINE),
        egui::StrokeKind::Inside,
    );
    let mut x = track.left() + 2.0;
    let mut clicked = None;
    for (i, (w, label)) in ws.iter().zip(items).enumerate() {
        let r = Rect::from_min_size(pos2(x, track.top() + 2.0), vec2(*w, h - 4.0));
        x += *w;
        let resp = ui.interact(r, ui.id().with((id, i)), Sense::click());
        let on = i == selected;
        if on {
            p.rect_filled(r, 4.0, t::TIDE_DECK_HI);
            p.line_segment(
                [
                    pos2(r.left() + 4.0, r.bottom() - 1.0),
                    pos2(r.right() - 4.0, r.bottom() - 1.0),
                ],
                Stroke::new(1.8_f32, accent),
            );
        } else if resp.hovered() {
            p.rect_filled(r, 4.0, t::TIDE_DECK.gamma_multiply(0.8));
        }
        p.text(
            r.center() - vec2(0.0, if on { 0.5 } else { 0.0 }),
            Align2::CENTER_CENTER,
            *label,
            if on { t::medium(12.5) } else { font.clone() },
            if on || resp.hovered() {
                t::text()
            } else {
                t::TIDE_MIST
            },
        );
        if kit::show_focus(ui, &resp) {
            kit::focus_ring(&p, r, 4.0);
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Interactive bipolar modulation depth bar (-1..1). Drag horizontally, double-click resets.
fn bipolar_bar(
    ui: &mut Ui,
    id: egui::Id,
    width: f32,
    color: Color32,
    value: &mut f64,
    hint: &str,
) -> (bool, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
    let mut started = false;
    let mut changed = false;
    let before = *value;
    if resp.drag_started() {
        started = true;
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let dx = kit::captured_drag(ui, &resp).x as f64 / (width as f64 * 0.5);
        *value = (*value + dx * if fine { 0.15 } else { 1.0 }).clamp(-1.0, 1.0);
    }
    if resp.double_clicked() {
        started = true;
        *value = 0.0;
    }
    if (*value - before).abs() > f64::EPSILON {
        changed = true;
    }
    let p = ui.painter();
    let well = tide_well(p, rect, 4.0);
    let mid = well.center().x;
    p.line_segment(
        [pos2(mid, well.top() + 2.0), pos2(mid, well.bottom() - 2.0)],
        Stroke::new(1.0_f32, t::TIDE_LINE),
    );
    let frac = (*value as f32).clamp(-1.0, 1.0);
    let x1 = mid + frac * (well.width() * 0.5 - 2.0);
    let (lo, hi) = if x1 >= mid { (mid, x1) } else { (x1, mid) };
    if hi - lo > 0.5 {
        p.rect_filled(
            Rect::from_min_max(pos2(lo, well.top() + 3.0), pos2(hi, well.bottom() - 3.0)),
            2.0,
            color.gamma_multiply(if resp.hovered() || resp.dragged() {
                0.85
            } else {
                0.65
            }),
        );
    }
    p.text(
        well.center(),
        Align2::CENTER_CENTER,
        signed(*value),
        t::mono(12.0),
        t::text(),
    );
    let _ = id;
    kit::tip(
        resp,
        format!("{hint}\nDrag left/right · Shift fine · Double-click resets"),
    );
    (started, changed)
}

/// Exact warp function matching `src/engine.rs` so the oscilloscope never lies.
fn warp_phase(phase: f64, warp: Warp, warp_amount: f64) -> f64 {
    match warp {
        Warp::Bend => {
            if warp_amount.abs() < 1e-6 {
                phase
            } else {
                phase.clamp(0.0, 1.0).powf(2f64.powf(warp_amount * 2.0))
            }
        }
        Warp::Sync => (phase * (1.0 + warp_amount.abs() * 3.0)).fract(),
        Warp::PwmMirror => {
            let split = (0.5 + warp_amount * 0.45).clamp(0.05, 0.95);
            let mirrored = if phase < split {
                phase / split
            } else {
                (1.0 - phase) / (1.0 - split)
            };
            phase * (1.0 - warp_amount.abs()) + mirrored * warp_amount.abs()
        }
        Warp::FmSub => {
            let sub = (phase * 0.5 * std::f64::consts::TAU).sin();
            (phase + sub * warp_amount * 0.25).rem_euclid(1.0)
        }
    }
}

fn table_bank() -> &'static TableBank {
    static BANK: std::sync::OnceLock<TableBank> = std::sync::OnceLock::new();
    BANK.get_or_init(TableBank::new)
}

/// Paint the Tide brand wave-crest emblem (3 layered tidal arcs).
fn paint_tide_mark(p: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    for (idx, (amp, y_off, col, sw)) in [
        (0.18, 0.72, t::TIDE_DEEP, 1.4_f32),
        (0.24, 0.54, t::TIDE_CYAN.gamma_multiply(0.55), 1.5_f32),
        (0.30, 0.36, t::TIDE_CYAN, 2.0_f32),
    ]
    .into_iter()
    .enumerate()
    {
        let pts: Vec<Pos2> = (0..=24)
            .map(|i| {
                let x = i as f32 / 24.0;
                let ph = x * std::f32::consts::TAU + idx as f32 * 0.55;
                let env = (x * std::f32::consts::PI).sin();
                pos2(
                    rect.left() + x * w,
                    rect.top() + h * y_off - ph.sin() * env * h * amp,
                )
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(sw, col)));
    }
}

/// Fill area under a polyline down to `baseline_y` cleanly (handles non-convex waves).
fn fill_under_curve(p: &egui::Painter, pts: &[Pos2], baseline_y: f32, color: Color32) {
    if pts.len() < 2 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for w in pts.windows(2) {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(w[0], color);
        mesh.colored_vertex(w[1], color);
        mesh.colored_vertex(pos2(w[1].x, baseline_y), color);
        mesh.colored_vertex(pos2(w[0].x, baseline_y), color);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    p.add(mesh);
}

// ---------------------------------------------------------------- Hero scopes

/// Signature 3D topographic wavetable scope with real warp distortion & XY drag.
#[allow(clippy::too_many_arguments)]
fn tide_wave_scope(
    ui: &mut Ui,
    ed: &mut Edit,
    table: Table,
    position: &mut f64,
    warp: Warp,
    warp_amount: &mut f64,
    active: bool,
    modulated_pos: Option<f64>,
) {
    let bank = table_bank();
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::click_and_drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    if resp.drag_started() {
        ed.started = true;
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = kit::captured_drag(ui, &resp);
        let scale = if fine { 0.15 } else { 1.0 };
        if d.x != 0.0 {
            *position =
                (*position + d.x as f64 / area.width().max(80.0) as f64 * scale).clamp(0.0, 1.0);
            ed.changed = true;
        }
        if d.y != 0.0 {
            *warp_amount = (*warp_amount
                - d.y as f64 / (area.height().max(60.0) as f64 * 0.6) * scale)
                .clamp(-1.0, 1.0);
            ed.changed = true;
        }
    }
    if resp.double_clicked() && warp_amount.abs() > 1e-6 {
        *warp_amount = 0.0;
        ed.started = true;
        ed.changed = true;
    }

    let p = ui.painter();
    let well = tide_well(p, area, 7.0);
    let r = well.shrink2(vec2(10.0, 10.0));

    // Subtle sea-level grid & vertical phase ticks.
    for q in [0.25f32, 0.5, 0.75] {
        let x = r.left() + r.width() * q;
        p.line_segment(
            [pos2(x, r.top() + 14.0), pos2(x, r.bottom() - 4.0)],
            Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.35)),
        );
    }
    let front_cy = r.center().y + 12.0;
    p.line_segment(
        [pos2(r.left(), front_cy), pos2(r.right(), front_cy)],
        Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.55)),
    );

    // 7 receding topographic wavetable strata (back -> front).
    let slices = 7usize;
    let trace_w = r.width() * 0.88;
    let dx_max = r.width() - trace_w;
    let dy_max = 26.0f32;
    let amp_h = r.height() * 0.30;

    for s in (0..slices).rev() {
        let depth = s as f32 / (slices - 1) as f32; // 0 = front, 1 = back
        let slice_pos = s as f64 / (slices - 1) as f64;
        let x0 = r.left() + depth * dx_max;
        let cy = front_cy - depth * dy_max;
        let pts: Vec<Pos2> = (0..=96)
            .map(|i| {
                let x = i as f64 / 96.0;
                let wph = warp_phase(x, warp, *warp_amount * (1.0 - depth as f64 * 0.4));
                let v = bank.sample(table, slice_pos, wph, 220.0, 48000.0);
                pos2(
                    x0 + x as f32 * trace_w,
                    cy - (v as f32).clamp(-1.2, 1.2) * amp_h * (1.0 - depth * 0.22),
                )
            })
            .collect();
        let near = 1.0 - (slice_pos - *position).abs() as f32;
        let alpha = (0.14 + 0.22 * near * (1.0 - depth * 0.4)).clamp(0.08, 0.38);
        let col = if active {
            t::TIDE_DEEP.gamma_multiply(alpha * 2.2)
        } else {
            t::TIDE_LINE.gamma_multiply(0.45)
        };
        p.add(Shape::line(pts, Stroke::new(1.0_f32, col)));
    }

    // Active warped foreground crest, placed at its depth along the tide ribbon.
    let active_depth = *position as f32;
    let x0 = r.left() + active_depth * dx_max;
    let cy = front_cy - active_depth * dy_max;
    let scale = 1.0 - active_depth * 0.16;
    let front_pts: Vec<Pos2> = (0..=160)
        .map(|i| {
            let x = i as f64 / 160.0;
            let wph = warp_phase(x, warp, *warp_amount);
            let v = bank.sample(table, *position, wph, 220.0, 48000.0);
            pos2(
                x0 + x as f32 * trace_w,
                cy - (v as f32).clamp(-1.2, 1.2) * amp_h * scale,
            )
        })
        .collect();

    let wave_col = if active { t::TIDE_CYAN } else { t::strong() };
    if active {
        fill_under_curve(p, &front_pts, cy, t::TIDE_CYAN.gamma_multiply(0.11));
    }
    p.add(Shape::line(front_pts, Stroke::new(2.0_f32, wave_col)));

    // Position rail along bottom edge + macro-modulated ghost indicator.
    let rail = Rect::from_min_max(
        pos2(r.left(), well.bottom() - 5.0),
        pos2(r.right(), well.bottom() - 2.0),
    );
    p.rect_filled(rail, 1.5, t::TIDE_LINE.gamma_multiply(0.5));
    let px = rail.left() + *position as f32 * rail.width();
    p.rect_filled(
        Rect::from_min_max(rail.min, pos2(px, rail.bottom())),
        1.5,
        wave_col.gamma_multiply(0.65),
    );
    if let Some(mpos) = modulated_pos
        && (mpos - *position).abs() > 0.01
    {
        let mx = rail.left() + mpos.clamp(0.0, 1.0) as f32 * rail.width();
        p.circle_filled(pos2(mx, rail.center().y), 2.5, t::text());
    }

    // Crisp mono HUD readout.
    p.text(
        well.left_top() + vec2(8.0, 5.0),
        Align2::LEFT_TOP,
        format!(
            "{}  POS {:.0}%  {} {}",
            table_name(table).to_uppercase(),
            *position * 100.0,
            warp_name(warp).to_uppercase(),
            signed(*warp_amount)
        ),
        t::mono(12.0),
        if active { t::TIDE_MIST } else { t::faint() },
    );
    if !active {
        p.text(
            well.right_top() + vec2(-8.0, 5.0),
            Align2::RIGHT_TOP,
            "MUTED",
            t::mono(12.0),
            t::wait(),
        );
    } else if resp.hovered() || resp.dragged() {
        p.text(
            well.right_top() + vec2(-8.0, 5.0),
            Align2::RIGHT_TOP,
            "X:POS  Y:WARP",
            t::mono(12.0),
            t::TIDE_CYAN,
        );
    }

    kit::tip(
        resp,
        "Drag horizontally to morph Position, vertically to bend Warp · Shift fine · Double-click resets Warp",
    );
}

/// Interactive harmonic spectrum scope for Osc C (Spectral).
fn tide_spectral_scope(ui: &mut Ui, ed: &mut Edit, c: &mut keyframexl::synth::Spectral) {
    use keyframexl::synth::MAX_PARTIALS;
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::click_and_drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    if resp.drag_started() {
        ed.started = true;
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = kit::captured_drag(ui, &resp);
        if d.x.abs() > 0.0
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let frac = ((pos.x - area.left()) / area.width().max(1.0)).clamp(0.0, 1.0);
            let n = (frac * MAX_PARTIALS as f32)
                .ceil()
                .clamp(1.0, MAX_PARTIALS as f32) as u8;
            if n != c.partials {
                c.partials = n;
                ed.changed = true;
            }
        }
        if d.y != 0.0 {
            let scale = if fine { 0.15 } else { 1.0 };
            c.tilt = (c.tilt - d.y as f64 / 70.0 * scale).clamp(-1.0, 1.0);
            ed.changed = true;
        }
    }
    if resp.double_clicked() && c.tilt.abs() > 1e-6 {
        c.tilt = 0.0;
        ed.started = true;
        ed.changed = true;
    }

    let on = c.level > 0.0;
    let anim_t = if on && c.shimmer > 0.0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        ui.input(|i| i.time)
    } else {
        0.0
    };

    let p = ui.painter();
    let well = tide_well(p, area, 7.0);
    let r = Rect::from_min_max(well.min + vec2(10.0, 22.0), well.max - vec2(10.0, 10.0));
    let mut amps = [0.0; MAX_PARTIALS];
    let n = c.amplitudes(130.8, 48_000.0, anim_t, &mut amps);
    let top = amps[..n].iter().fold(1e-9f64, |m, a| m.max(*a));
    let bw = r.width() / MAX_PARTIALS as f32;

    for i in 0..MAX_PARTIALS {
        let x = r.left() + i as f32 * bw;
        if i < n {
            let h = ((amps[i] / top) as f32 * r.height()).max(2.0);
            let odd = i % 2 == 0; // harmonic 1, 3, 5...
            let col = if !on {
                t::strong().gamma_multiply(0.5)
            } else if odd {
                t::TIDE_CYAN
            } else {
                t::TIDE_MIST
            };
            let bar = Rect::from_min_max(
                pos2(x + 1.5, r.bottom() - h),
                pos2(x + bw - 1.5, r.bottom()),
            );
            p.rect_filled(bar, 1.5, col.gamma_multiply(0.85));
            p.rect_filled(
                Rect::from_min_max(
                    bar.min,
                    pos2(bar.right(), (bar.top() + 2.0).min(bar.bottom())),
                ),
                1.0,
                if on { t::text() } else { t::strong() },
            );
        } else {
            let stub = Rect::from_min_max(
                pos2(x + 2.0, r.bottom() - 2.0),
                pos2(x + bw - 2.0, r.bottom()),
            );
            p.rect_filled(stub, 1.0, t::TIDE_LINE.gamma_multiply(0.45));
        }
    }

    p.text(
        well.left_top() + vec2(8.0, 5.0),
        Align2::LEFT_TOP,
        format!("SPECTRAL  {n} PARTIALS  TILT {}", signed(c.tilt)),
        t::mono(12.0),
        if on { t::TIDE_MIST } else { t::faint() },
    );
    p.text(
        well.right_top() + vec2(-8.0, 5.0),
        Align2::RIGHT_TOP,
        if on { "X:COUNT  Y:TILT" } else { "MUTED" },
        t::mono(12.0),
        if on { t::TIDE_CYAN } else { t::wait() },
    );
    kit::tip(
        resp,
        "Drag horizontally to set harmonic count, vertically to tilt spectrum · Double-click resets tilt",
    );
}

/// Interactive Filter Frequency Scope: drag node for Cutoff (X) & Resonance (Y),
/// plus live Envelope Modulation ghost curve.
fn tide_filter_scope(ui: &mut Ui, ed: &mut Edit, p: &mut SynthPatch) {
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 108.0), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let well = tide_well(&painter, area, 7.0);
    let r = Rect::from_min_max(well.min + vec2(10.0, 22.0), well.max - vec2(10.0, 14.0));

    let fx = |freq: f64| {
        r.left() + ((freq.clamp(20.0, 20000.0) / 20.0).log10() / 3.0) as f32 * r.width()
    };
    let y_db = |db: f64| r.bottom() - ((db + 42.0) / 60.0) as f32 * r.height();

    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    if resp.drag_started() {
        ed.started = true;
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = kit::captured_drag(ui, &resp);
        let scale = if fine { 0.15 } else { 1.0 };
        if d.x != 0.0 {
            let cur_u = (p.cutoff / 20.0).log10() / 3.0;
            let next_u = (cur_u + d.x as f64 / r.width().max(60.0) as f64 * scale).clamp(0.0, 1.0);
            p.cutoff = (20.0 * 1000.0_f64.powf(next_u)).clamp(20.0, 20000.0);
            ed.changed = true;
        }
        if d.y != 0.0 {
            p.resonance =
                (p.resonance - d.y as f64 / r.height().max(40.0) as f64 * scale).clamp(0.0, 1.0);
            ed.changed = true;
        }
    }
    if resp.double_clicked() && p.resonance > 0.0 {
        p.resonance = 0.0;
        ed.started = true;
        ed.changed = true;
    }

    // Frequency decade grid & 0 dB axis.
    for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
        let x = fx(f);
        painter.line_segment(
            [pos2(x, r.top()), pos2(x, r.bottom())],
            Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.45)),
        );
        painter.text(
            pos2(x, well.bottom() - 2.0),
            Align2::CENTER_BOTTOM,
            label,
            t::mono(12.0),
            t::TIDE_MIST.gamma_multiply(0.65),
        );
    }
    painter.line_segment(
        [pos2(r.left(), y_db(0.0)), pos2(r.right(), y_db(0.0))],
        Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.65)),
    );

    let curve_pts = |cutoff: f64, reso: f64| -> Vec<Pos2> {
        let q = 0.5 + reso * 9.5;
        (0..=120)
            .map(|i| {
                let u = i as f64 / 120.0;
                let f = 20.0 * 1000.0_f64.powf(u);
                let k = f / cutoff.clamp(20.0, 20000.0);
                let denom = ((1.0 - k * k).powi(2) + (k / q).powi(2)).sqrt().max(0.001);
                let gain = match p.filter {
                    Filter::LowPass => 1.0 / denom,
                    Filter::HighPass => k * k / denom,
                    Filter::BandPass => k / q / denom,
                };
                let db = (20.0 * gain.max(0.001).log10()).clamp(-42.0, 18.0);
                pos2(r.left() + u as f32 * r.width(), y_db(db))
            })
            .collect()
    };

    // Envelope peak ghost curve when ENV depth != 0.
    if p.env_amount.abs() > 0.01 {
        let env_cut = (p.cutoff * 2.0_f64.powf(p.env_amount * 6.0)).clamp(20.0, 20000.0);
        let ghost = curve_pts(env_cut, p.resonance);
        painter.add(Shape::line(
            ghost,
            Stroke::new(1.2_f32, t::TIDE_CORAL.gamma_multiply(0.32)),
        ));
    }

    let pts = curve_pts(p.cutoff, p.resonance);
    fill_under_curve(
        &painter,
        &pts,
        r.bottom(),
        t::TIDE_CORAL.gamma_multiply(0.14),
    );
    painter.add(Shape::line(pts, Stroke::new(2.0_f32, t::TIDE_CORAL)));

    // Draggable Cutoff / Resonance node.
    let cx = fx(p.cutoff);
    let cy = r.bottom() - (0.28 + p.resonance as f32 * 0.64) * r.height();
    painter.line_segment(
        [pos2(cx, r.top()), pos2(cx, r.bottom())],
        Stroke::new(1.0_f32, t::TIDE_CORAL.gamma_multiply(0.35)),
    );
    let node_r = if resp.hovered() || resp.dragged() {
        6.0
    } else {
        5.0
    };
    painter.circle_filled(pos2(cx, cy), node_r + 2.0, t::TIDE_WELL);
    painter.circle_filled(pos2(cx, cy), node_r, t::TIDE_CORAL);
    painter.circle_stroke(pos2(cx, cy), node_r, Stroke::new(1.2_f32, t::text()));

    painter.text(
        well.left_top() + vec2(8.0, 5.0),
        Align2::LEFT_TOP,
        format!("{}  RESO {:.0}%", hz(p.cutoff), p.resonance * 100.0),
        t::mono(12.0),
        t::TIDE_MIST,
    );
    if p.env_amount.abs() > 0.01 {
        painter.text(
            well.right_top() + vec2(-8.0, 5.0),
            Align2::RIGHT_TOP,
            format!("ENV {}", signed(p.env_amount)),
            t::mono(12.0),
            t::TIDE_CORAL,
        );
    }
    kit::tip(
        resp,
        "Drag horizontally for Cutoff, vertically for Resonance · Shift fine · Double-click resets Resonance",
    );
}

// ---------------------------------------------------------------- Envelopes

fn curve_mut(e: &mut Adsr, i: usize) -> &mut f64 {
    match i {
        0 => &mut e.attack_curve,
        1 => &mut e.decay_curve,
        _ => &mut e.release_curve,
    }
}

/// Interactive envelope canvas: drag breakpoints (Attack, Decay/Sustain, Release)
/// OR drag any segment middle up/down to bend its curve.
fn envelope_view(ui: &mut Ui, ed: &mut Edit, e: &mut Adsr, color: Color32) {
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let well = tide_well(&painter, area, 6.0);
    let r = well.shrink2(vec2(10.0, 10.0));

    let total = (e.delay + e.attack + e.hold + e.decay + e.release).max(0.05);
    let sus = (total * 0.32).max(0.05);
    let span = total + sus;
    let x = |s: f64| r.left() + (s / span) as f32 * r.width();
    let y = |l: f64| r.bottom() - l.clamp(0.0, 1.0) as f32 * r.height();

    let a0 = e.delay;
    let a1 = a0 + e.attack;
    let d0 = a1 + e.hold;
    let d1 = d0 + e.decay;
    let r0 = d1 + sus;
    let r1 = r0 + e.release;

    let knots = [
        pos2(x(a1), y(1.0)),       // 0: Attack peak
        pos2(x(d1), y(e.sustain)), // 1: Decay / Sustain knee
        pos2(x(r1), y(0.0)),       // 2: Release end
    ];
    let spans = [(a0, a1), (d0, d1), (r0, r1)];

    // Hit test: prefer knots (within 10px), else segment bend.
    let hit_mode = |pt: Pos2| -> Option<i8> {
        for (idx, k) in knots.iter().enumerate() {
            if pt.distance(*k) <= 10.0 {
                return Some(idx as i8); // 0, 1, 2 = knot
            }
        }
        spans
            .iter()
            .position(|&(s0, s1)| s1 > s0 && pt.x >= x(s0) - 2.0 && pt.x <= x(s1) + 2.0)
            .map(|i| -(i as i8) - 1) // -1, -2, -3 = bend segment 0, 1, 2
    };

    let drag_id = resp.id.with("tide-env-mode");
    let hover = resp.hover_pos().and_then(hit_mode);
    if let Some(m) = hover {
        ui.ctx().set_cursor_icon(if m >= 0 {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::ResizeVertical
        });
    }
    if resp.drag_started() {
        let m = resp.interact_pointer_pos().and_then(hit_mode);
        ui.data_mut(|d| d.insert_temp(drag_id, m));
        ed.started |= m.is_some();
    }
    let active_mode: Option<i8> = ui.data(|d| d.get_temp(drag_id)).flatten();
    if resp.dragged()
        && let Some(m) = active_mode
    {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = kit::captured_drag(ui, &resp);
        let s = if fine { 0.2 } else { 1.0 };
        if m >= 0 {
            let dt = d.x as f64 / r.width().max(40.0) as f64 * span * s;
            match m {
                0 => e.attack = (e.attack + dt).clamp(0.0, 10.0),
                1 => {
                    e.decay = (e.decay + dt).clamp(0.0, 20.0);
                    e.sustain =
                        (e.sustain - d.y as f64 / r.height().max(30.0) as f64 * s).clamp(0.0, 1.0);
                }
                _ => e.release = (e.release + dt).clamp(0.0, 20.0),
            }
            ed.changed = true;
        } else {
            let seg = (-m - 1) as usize;
            let dy = d.y as f64 / 56.0 * s;
            let c = curve_mut(e, seg);
            *c = (*c + if seg == 0 { -dy } else { dy }).clamp(-1.0, 1.0);
            ed.changed = true;
        }
    }
    if resp.double_clicked()
        && let Some(m) = hover
        && m < 0
    {
        let seg = (-m - 1) as usize;
        if *curve_mut(e, seg) != 0.0 {
            *curve_mut(e, seg) = 0.0;
            ed.started = true;
            ed.changed = true;
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<i8>>(drag_id));
    }

    // Build curve polyline.
    let mut pts = vec![pos2(x(0.0), y(0.0)), pos2(x(a0), y(0.0))];
    let steps = 24;
    let level = |env: &Adsr, at: f64| env.value(at, d1 + 1e9);
    for k in 1..=steps {
        let at = a0 + e.attack * k as f64 / steps as f64;
        pts.push(pos2(x(at), y(level(e, at))));
    }
    pts.push(pos2(x(d0), y(1.0)));
    for k in 1..=steps {
        let at = d0 + e.decay * k as f64 / steps as f64;
        pts.push(pos2(x(at), y(level(e, at))));
    }
    pts.push(pos2(x(r0), y(e.sustain)));
    for k in 1..=steps {
        let prog = k as f64 / steps as f64;
        pts.push(pos2(
            x(r0 + e.release * prog),
            y(e.sustain * (1.0 - keyframexl::sampler::bend(prog, e.release_curve))),
        ));
    }

    if let Some(m) = active_mode.or(hover)
        && m < 0
    {
        let seg = (-m - 1) as usize;
        let (s0, s1) = spans[seg];
        painter.rect_filled(
            Rect::from_min_max(pos2(x(s0), r.top()), pos2(x(s1), r.bottom())),
            2.0,
            color.gamma_multiply(0.09),
        );
    }

    fill_under_curve(&painter, &pts, r.bottom(), color.gamma_multiply(0.14));
    painter.add(Shape::line(pts, Stroke::new(1.8_f32, color)));

    for (idx, k) in knots.iter().enumerate() {
        let lit = active_mode == Some(idx as i8) || hover == Some(idx as i8);
        painter.circle_filled(*k, if lit { 4.8 } else { 3.6 }, t::TIDE_WELL);
        painter.circle_filled(*k, if lit { 3.8 } else { 2.8 }, color);
    }

    painter.text(
        well.right_top() + vec2(-6.0, 4.0),
        Align2::RIGHT_TOP,
        secs(e.delay + e.attack + e.hold + e.decay + e.release),
        t::mono(12.0),
        t::TIDE_MIST,
    );
    kit::tip(
        resp,
        "Drag dots to shape A / D / S / R · Drag segment up/down to bend curve · Double-click straightens",
    );
}

fn envelope_knobs(ui: &mut Ui, ed: &mut Edit, e: &mut Adsr, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ed.knob(
            ui,
            "ATTACK",
            color,
            &mut e.attack,
            (0.0, 10.0, 0.005),
            true,
            &secs,
            "Rise time",
        );
        ed.knob(
            ui,
            "DECAY",
            color,
            &mut e.decay,
            (0.0, 20.0, 0.0),
            true,
            &secs,
            "Fall time to sustain",
        );
        ed.knob(
            ui,
            "SUSTAIN",
            color,
            &mut e.sustain,
            (0.0, 1.0, 1.0),
            false,
            &pct,
            "Held level",
        );
        ed.knob(
            ui,
            "RELEASE",
            color,
            &mut e.release,
            (0.0, 20.0, 0.015),
            true,
            &secs,
            "Fade after note off",
        );
    });
    for v in [&mut e.attack, &mut e.decay, &mut e.release] {
        if *v < 0.0011 {
            *v = 0.0;
        }
    }
}

// ---------------------------------------------------------------- LFO & Modulation

fn division_name(d: LfoDivision) -> &'static str {
    match d {
        LfoDivision::Whole => "1/1",
        LfoDivision::Half => "1/2",
        LfoDivision::Quarter => "1/4",
        LfoDivision::Eighth => "1/8",
        LfoDivision::Sixteenth => "1/16",
        LfoDivision::ThirtySecond => "1/32",
    }
}

fn source_color(s: ModSource) -> Color32 {
    match s {
        ModSource::Lfo1 | ModSource::Lfo2 => t::TIDE_MINT,
        ModSource::FilterEnv => t::TIDE_CORAL,
        ModSource::Velocity | ModSource::ModWheel | ModSource::Pressure | ModSource::Slide => {
            t::TIDE_CYAN
        }
        ModSource::Macro(_) => t::enc_white(),
    }
}

fn lfo_scope(ui: &mut Ui, lfo: &Lfo, tempo: f64) {
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    let p = ui.painter();
    let well = tide_well(p, area, 6.0);
    let r = well.shrink2(vec2(8.0, 8.0));
    p.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.6)),
    );
    let steps = [0.3, -0.7, 0.9, -0.2, 0.5, -0.9, 0.1, 0.7];
    let val_at = |x: f64| {
        let ph = (x * 2.0).fract();
        match lfo.shape {
            LfoShape::Off => 0.0,
            LfoShape::Sine => (ph * std::f64::consts::TAU).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (ph - 0.5).abs(),
            LfoShape::Saw => ph * 2.0 - 1.0,
            LfoShape::Square => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => steps[((x * 8.0) as usize).min(7)],
        }
    };
    let pts: Vec<Pos2> = (0..=180)
        .map(|i| {
            let x = i as f64 / 180.0;
            pos2(
                r.left() + x as f32 * r.width(),
                r.center().y - val_at(x) as f32 * r.height() * 0.44,
            )
        })
        .collect();
    let col = if lfo.shape == LfoShape::Off {
        t::strong()
    } else {
        t::TIDE_MINT
    };
    if lfo.shape != LfoShape::Off {
        fill_under_curve(p, &pts, r.center().y, col.gamma_multiply(0.12));
    }
    p.add(Shape::line(pts, Stroke::new(1.8_f32, col)));

    // Live moving phase dot while LFO is active.
    if lfo.shape != LfoShape::Off {
        let freq = lfo.frequency(tempo);
        let t_now = ui.input(|i| i.time);
        let ph = (t_now * freq * 0.5).fract();
        let dot = pos2(
            r.left() + ph as f32 * r.width(),
            r.center().y - val_at(ph) as f32 * r.height() * 0.44,
        );
        p.circle_filled(dot, 3.8, t::text());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    let rate = if lfo.shape == LfoShape::Off {
        "OFF".to_string()
    } else if lfo.sync {
        format!("SYNC {}", division_name(lfo.division))
    } else {
        format!("{:.2} Hz", lfo.rate.clamp(0.01, 30.0))
    };
    p.text(
        well.right_top() + vec2(-6.0, 4.0),
        Align2::RIGHT_TOP,
        rate,
        t::mono(12.0),
        t::TIDE_MIST,
    );
}

// ---------------------------------------------------------------- FX Rack Bays

/// Bespoke mini visual inside each Tide FX stomp bay.
fn fx_mini_scope(ui: &mut Ui, kind: keyframexl::fx::FxKind, params: &[f32], on: bool) {
    use keyframexl::fx::FxKind;
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::hover());
    let p = ui.painter();
    let well = tide_well(p, area, 5.0);
    let r = well.shrink(5.0);
    let col = if on {
        t::TIDE_CYAN
    } else {
        t::strong().gamma_multiply(0.55)
    };
    match kind {
        FxKind::Drive => {
            let drive = 1.0 + (params.first().copied().unwrap_or(6.0) / 10.0).max(0.0);
            let pts: Vec<Pos2> = (0..=48)
                .map(|i| {
                    let x = i as f32 / 48.0 * 2.0 - 1.0;
                    let y = (x * drive).tanh();
                    pos2(
                        r.left() + (x + 1.0) * 0.5 * r.width(),
                        r.center().y - y * r.height() * 0.45,
                    )
                })
                .collect();
            p.line_segment(
                [r.left_center(), r.right_center()],
                Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.5)),
            );
            p.add(Shape::line(pts, Stroke::new(1.6_f32, col)));
        }
        FxKind::Chorus => {
            let depth = (params.get(1).copied().unwrap_or(40.0) / 100.0).clamp(0.1, 1.0);
            for (ph, alpha) in [(0.0f32, 1.0), (1.3, 0.5)] {
                let pts: Vec<Pos2> = (0..=48)
                    .map(|i| {
                        let x = i as f32 / 48.0;
                        let y = (x * std::f32::consts::TAU * 2.0 + ph).sin() * depth;
                        pos2(
                            r.left() + x * r.width(),
                            r.center().y - y * r.height() * 0.42,
                        )
                    })
                    .collect();
                p.add(Shape::line(
                    pts,
                    Stroke::new(1.5_f32, col.gamma_multiply(alpha)),
                ));
            }
        }
        FxKind::Delay => {
            let steps = params.first().copied().unwrap_or(3.0).clamp(1.0, 16.0);
            let fb = (params.get(1).copied().unwrap_or(40.0) / 100.0).clamp(0.05, 0.95);
            let taps = 6usize;
            let gap = (r.width() / (taps as f32 + 0.5)) * (0.55 + 0.45 * (steps / 16.0).min(1.0));
            for i in 0..taps {
                let x = r.left() + 4.0 + i as f32 * gap;
                if x > r.right() - 2.0 {
                    break;
                }
                let h = r.height() * fb.powi(i as i32);
                p.rect_filled(
                    Rect::from_min_max(pos2(x, r.bottom() - h), pos2(x + 4.0, r.bottom())),
                    1.0,
                    col.gamma_multiply(1.0 - i as f32 * 0.12),
                );
            }
        }
        _ => {
            // Reverb exponential diffuse tail.
            let size = (params.first().copied().unwrap_or(50.0) / 100.0).clamp(0.15, 1.0);
            let damp = (params.get(1).copied().unwrap_or(40.0) / 100.0).clamp(0.0, 1.0);
            let cols = 20usize;
            let bw = r.width() / cols as f32;
            for i in 0..cols {
                let u = i as f32 / cols as f32;
                let env = (-u * (3.8 - size * 2.4 + damp * 0.9)).exp();
                let jitter = 0.72 + 0.28 * ((i as f32 * 2.7).sin().abs());
                let h = (r.height() * env * jitter).max(1.5);
                let x = r.left() + i as f32 * bw;
                p.rect_filled(
                    Rect::from_min_max(
                        pos2(x + 1.0, r.bottom() - h),
                        pos2(x + bw - 1.0, r.bottom()),
                    ),
                    1.0,
                    col.gamma_multiply(0.35 + 0.65 * env),
                );
            }
        }
    }
}

// ---------------------------------------------------------------- Unison Fan Visualizer

fn unison_fan_scope(ui: &mut Ui, unison: u8, detune: f64, spread: f64) {
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::hover());
    let p = ui.painter();
    let well = tide_well(p, area, 6.0);
    let r = well.shrink2(vec2(14.0, 10.0));
    p.line_segment(
        [pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())],
        Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.5)),
    );
    p.line_segment(
        [pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())],
        Stroke::new(1.0_f32, t::TIDE_LINE),
    );
    let count = unison.clamp(1, 7) as usize;
    for i in 0..count {
        let side = if count == 1 {
            0.0f32
        } else {
            i as f32 * 2.0 / (count - 1) as f32 - 1.0
        };
        let pan_x = r.center().x + side * (spread as f32) * (r.width() * 0.44);
        let detune_lift = (1.0 - side.abs() * (0.25 + 0.35 * detune as f32)).clamp(0.35, 1.0);
        let top_y = r.bottom() - r.height() * 0.85 * detune_lift;
        let col = if i == count / 2 {
            t::TIDE_CYAN
        } else {
            t::TIDE_MIST.gamma_multiply(0.85)
        };
        p.line_segment(
            [pos2(pan_x, r.bottom()), pos2(pan_x, top_y)],
            Stroke::new(2.2_f32, col),
        );
        p.circle_filled(pos2(pan_x, top_y), 3.0, col);
    }
    p.text(
        well.left_top() + vec2(6.0, 4.0),
        Align2::LEFT_TOP,
        "L",
        t::mono(12.0),
        t::TIDE_MIST,
    );
    p.text(
        well.right_top() + vec2(-6.0, 4.0),
        Align2::RIGHT_TOP,
        "R",
        t::mono(12.0),
        t::TIDE_MIST,
    );
    p.text(
        pos2(well.center().x, well.top() + 4.0),
        Align2::CENTER_TOP,
        format!("{count} VOICE{}", if count == 1 { "" } else { "S" }),
        t::mono(12.0),
        t::TIDE_CYAN,
    );
}

// ---------------------------------------------------------------- Oscillator mini-thumbnail tabs

fn osc_tab_button(
    ui: &mut Ui,
    id: egui::Id,
    w: f32,
    label: &str,
    selected: bool,
    level: f64,
    preview_fn: impl FnOnce(&egui::Painter, Rect, Color32),
) -> (bool, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    let p = ui.painter();
    let on = level > 0.0;
    let bg = if selected {
        t::TIDE_DECK_HI
    } else if resp.hovered() {
        t::TIDE_DECK_HI.gamma_multiply(0.6)
    } else {
        t::TIDE_WELL
    };
    p.rect_filled(rect, 6.0, bg);
    p.rect_stroke(
        rect,
        6.0,
        Stroke::new(
            1.0_f32,
            if selected {
                t::TIDE_CYAN.gamma_multiply(0.8)
            } else {
                t::TIDE_LINE
            },
        ),
        egui::StrokeKind::Inside,
    );
    if selected {
        p.line_segment(
            [
                pos2(rect.left() + 6.0, rect.top() + 1.0),
                pos2(rect.right() - 6.0, rect.top() + 1.0),
            ],
            Stroke::new(2.0_f32, t::TIDE_CYAN),
        );
    }

    // Power LED hit area on the left.
    let led_rect = Rect::from_min_size(rect.min, vec2(22.0, rect.height()));
    let led_resp = ui.interact(led_rect, id.with("pwr"), Sense::click());
    kit::led(p, led_rect.center(), 3.8, on.then_some(t::TIDE_CYAN));
    if led_resp.hovered() {
        p.circle_stroke(led_rect.center(), 6.5, Stroke::new(1.0_f32, t::TIDE_MIST));
    }

    p.text(
        pos2(rect.left() + 23.0, rect.top() + 5.0),
        Align2::LEFT_TOP,
        label,
        if selected {
            t::semibold(12.0)
        } else {
            t::medium(12.0)
        },
        if selected || on {
            t::text()
        } else {
            t::TIDE_MIST
        },
    );
    p.text(
        pos2(rect.left() + 23.0, rect.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        if on { pct(level) } else { "OFF".into() },
        t::mono(11.0),
        if on { t::TIDE_CYAN } else { t::faint() },
    );

    // Mini waveform thumbnail on the right half of the tab.
    let thumb_w = (rect.width() - 80.0).clamp(32.0, 56.0);
    let thumb = Rect::from_min_max(
        pos2(rect.right() - thumb_w - 7.0, rect.top() + 7.0),
        pos2(rect.right() - 7.0, rect.bottom() - 7.0),
    );
    if thumb.width() > 16.0 {
        preview_fn(p, thumb, if on { t::TIDE_CYAN } else { t::strong() });
    }

    kit::tip(led_resp.clone(), format!("Toggle {label} on/off"));
    let toggled_power = led_resp.clicked();
    let picked_tab = resp.clicked() && !toggled_power;
    (picked_tab, toggled_power)
}

fn mini_wave_thumb(
    p: &egui::Painter,
    r: Rect,
    table: Table,
    pos: f64,
    warp: Warp,
    warp_amt: f64,
    col: Color32,
) {
    let bank = table_bank();
    let pts: Vec<Pos2> = (0..=28)
        .map(|i| {
            let x = i as f64 / 28.0;
            let wph = warp_phase(x, warp, warp_amt);
            let v = bank.sample(table, pos, wph, 220.0, 48000.0);
            pos2(
                r.left() + x as f32 * r.width(),
                r.center().y - (v as f32).clamp(-1.1, 1.1) * r.height() * 0.44,
            )
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.3_f32, col)));
}

fn mini_spec_thumb(p: &egui::Painter, r: Rect, c: &keyframexl::synth::Spectral, col: Color32) {
    let mut amps = [0.0; keyframexl::synth::MAX_PARTIALS];
    let n = c.amplitudes(130.8, 48_000.0, 0.0, &mut amps).min(12);
    let top = amps[..n].iter().fold(1e-9f64, |m, a| m.max(*a));
    let bw = r.width() / 12.0;
    for (i, a) in amps[..n].iter().enumerate() {
        let h = ((a / top) as f32 * r.height()).max(1.5);
        let x = r.left() + i as f32 * bw;
        p.rect_filled(
            Rect::from_min_max(
                pos2(x + 0.5, r.bottom() - h),
                pos2(x + bw - 0.5, r.bottom()),
            ),
            0.5,
            col,
        );
    }
}

/// Musical, bounded patch mutation so sound design exploration is one click away.
fn mutate_patch(p: &mut SynthPatch, seed: u64) {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    let mut next = || -> f64 {
        s ^= s >> 30;
        s = s.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        s ^= s >> 27;
        s = s.wrapping_mul(0x94D0_49BB_1331_11EB);
        s ^= s >> 31;
        ((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    };
    p.position = (p.position + next() * 0.22).clamp(0.0, 1.0);
    p.warp_amount = (p.warp_amount + next() * 0.25).clamp(-1.0, 1.0);
    let cut_oct = next() * 0.8;
    p.cutoff = (p.cutoff * 2.0_f64.powf(cut_oct)).clamp(120.0, 18000.0);
    p.resonance = (p.resonance + next() * 0.14).clamp(0.0, 0.75);
    p.detune = (p.detune + next() * 0.08).clamp(0.0, 0.6);
    if p.osc_b.level > 0.0 {
        p.osc_b.position = (p.osc_b.position + next() * 0.22).clamp(0.0, 1.0);
        p.osc_b.warp_amount = (p.osc_b.warp_amount + next() * 0.25).clamp(-1.0, 1.0);
    }
    if p.spectral.level > 0.0 {
        p.spectral.tilt = (p.spectral.tilt + next() * 0.25).clamp(-1.0, 1.0);
        p.spectral.shimmer = (p.spectral.shimmer + next() * 0.2).clamp(0.0, 1.0);
    }
}

// ---------------------------------------------------------------- Mini Audition Keyboard

const WHITE_OFFSETS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const BLACK_SLOTS: [(usize, u8); 5] = [(0, 1), (1, 3), (3, 6), (4, 8), (5, 10)];

/// Shared 2-octave playable audition keyboard strip (used by Tide; velocity by click Y).
fn audition_keyboard(app: &mut App, ui: &mut Ui, id: u64, octave: &mut i8, accent: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let oct_down = ui
            .add_enabled(
                *octave > 0,
                egui::Button::new("◀").min_size(vec2(22.0, 32.0)),
            )
            .on_hover_text("Octave down");
        if oct_down.clicked() {
            *octave = (*octave - 1).max(0);
        }
        let oct_up = ui
            .add_enabled(
                *octave < 6,
                egui::Button::new("▶").min_size(vec2(22.0, 32.0)),
            )
            .on_hover_text("Octave up");
        if oct_up.clicked() {
            *octave = (*octave + 1).min(6);
        }

        let kb_w = ui.available_width().max(180.0);
        let kb_h = 34.0;
        let (area, resp) = ui.allocate_exact_size(vec2(kb_w, kb_h), Sense::click_and_drag());
        let p = ui.painter();
        p.rect_filled(area, 4.0, t::edge());
        let inner = area.shrink(1.0);
        let base_note = ((*octave + 1) * 12) as u8;
        let octaves = 2usize;
        let whites = octaves * 7;
        let ww = inner.width() / whites as f32;

        let mut black_rects: Vec<(u8, Rect)> = Vec::new();
        let mut white_rects: Vec<(u8, Rect)> = Vec::new();

        for oct in 0..octaves {
            for (wi, &semi) in WHITE_OFFSETS.iter().enumerate() {
                let idx = oct * 7 + wi;
                let note = (base_note + (oct as u8) * 12 + semi).min(127);
                let r = Rect::from_min_size(
                    pos2(inner.left() + idx as f32 * ww, inner.top()),
                    vec2((ww - 1.0).max(2.0), inner.height()),
                );
                white_rects.push((note, r));
            }
            for &(slot, semi) in &BLACK_SLOTS {
                let idx = oct * 7 + slot;
                let note = (base_note + (oct as u8) * 12 + semi).min(127);
                let bw = ww * 0.62;
                let bx = inner.left() + (idx as f32 + 1.0) * ww - bw * 0.5;
                let r = Rect::from_min_size(pos2(bx, inner.top()), vec2(bw, inner.height() * 0.60));
                black_rects.push((note, r));
            }
        }

        let held_pitch = app
            .held
            .and_then(|(_, ch, pitch)| (ch == id).then_some(pitch));

        // Paint white keys.
        for &(note, r) in &white_rects {
            let active = held_pitch == Some(note);
            p.rect_filled(r, 2.0, if active { accent } else { t::key_white() });
            if note % 12 == 0 {
                p.text(
                    pos2(r.center().x, r.bottom() - 2.0),
                    Align2::CENTER_BOTTOM,
                    super::roll::note_name(note),
                    t::mono(10.0),
                    if active { t::edge() } else { t::key_ink() },
                );
            }
        }
        // Paint black keys on top.
        for &(note, r) in &black_rects {
            let active = held_pitch == Some(note);
            p.rect_filled(r, 2.0, if active { accent } else { t::key_black() });
            p.rect_stroke(
                r,
                2.0,
                Stroke::new(1.0_f32, t::edge()),
                egui::StrokeKind::Inside,
            );
        }

        // Interaction: black keys take priority over white keys underneath.
        let down = resp.is_pointer_button_down_on() || resp.dragged();
        let kb_drag_id = ui.id().with(("tide-kb-held", id));
        let was_down: bool = ui.data(|d| d.get_temp(kb_drag_id)).unwrap_or(false);
        if down {
            if let Some(pos) = resp.interact_pointer_pos() {
                let hit = black_rects
                    .iter()
                    .find(|(_, r)| r.contains(pos))
                    .or_else(|| white_rects.iter().find(|(_, r)| r.contains(pos)))
                    .map(|&(n, _)| n);
                if let Some(note) = hit {
                    let y_frac = ((pos.y - inner.top()) / inner.height().max(1.0)).clamp(0.0, 1.0);
                    let vel = (48.0 + y_frac * 79.0).round() as u8;
                    app.hold_note(id, note, vel);
                }
            }
        } else if was_down {
            app.release_note();
        }
        ui.data_mut(|d| d.insert_temp(kb_drag_id, down));
        kit::tip(
            resp,
            "Click or drag keys to audition · Lower on key = higher velocity",
        );
    });
}

// ---------------------------------------------------------------- Main Tide Editor

impl App {
    pub(super) fn synth_editor(&mut self, ui: &mut Ui, id: u64, patch: &SynthPatch) {
        let mut p = patch.clone();
        let mut ed = Edit::default();
        let view_key = egui::Id::new(("tide-view", id));
        let mut view: TideView = ui.ctx().data(|d| d.get_temp(view_key)).unwrap_or_default();
        let tempo = self.song.tempo;

        egui::Frame::new()
            .fill(t::TIDE_CHASSIS)
            .stroke(Stroke::new(1.0_f32, t::TIDE_LINE))
            .corner_radius(9.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let _dark = t::Faceplate::enter(ui);
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

                // 1. Faceplate Header: Brand mark, live signal chain, presets, A/B, Mutate, Init
                self.tide_header(ui, id, &mut p, &mut view, &mut ed);

                // 2+3. Scrollable Sound Stage + Lower Studio Deck (leaves exact room for pinned footer)
                let footer_reserve = if ui.available_width() >= TWO_COLUMNS {
                    60.0
                } else {
                    108.0
                };
                let body_h = (ui.available_height() - footer_reserve).max(140.0);
                egui::ScrollArea::vertical()
                    .id_salt(("tide-scroll", id))
                    .max_height(body_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        // 2. Hero Sound Stage: Oscillators (left) + Filter & Quick Voice (right)
                        let wide = ui.available_width() >= TWO_COLUMNS;
                        if wide {
                            let avail = ui.available_width();
                            let gap = 6.0;
                            let left_w = ((avail - gap) * 0.52).floor();
                            let right_w = avail - gap - left_w;
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                ui.allocate_ui_with_layout(
                                    vec2(left_w, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| self.tide_osc_bay(ui, id, &mut p, &mut view, &mut ed),
                                );
                                ui.allocate_ui_with_layout(
                                    vec2(right_w, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| self.tide_filter_bay(ui, id, &mut p, &mut view, &mut ed),
                                );
                            });
                        } else {
                            self.tide_osc_bay(ui, id, &mut p, &mut view, &mut ed);
                            self.tide_filter_bay(ui, id, &mut p, &mut view, &mut ed);
                        }

                        // 3. Lower Studio Deck (Tabbed: Envelopes / LFO & Mod / Effects / Voices & Macros)
                        self.tide_lower_deck(ui, id, &mut p, &mut view, &mut ed, tempo);
                    });

                // 4. Pinned Performance Footer: 4 Macros + Audition Mini-Keyboard (always visible!)
                self.tide_performance_footer(ui, id, &mut p, &mut view, &mut ed);
            });

        ui.ctx().data_mut(|d| d.insert_temp(view_key, view));

        if ed.started {
            self.checkpoint();
        }
        if ed.changed {
            if let Some(InstrumentKind::Synth { patch }) = self
                .song
                .instruments
                .iter_mut()
                .find(|i| i.id == id)
                .map(|i| &mut i.kind)
            {
                *patch = p;
            }
            self.publish();
        }
    }

    fn tide_header(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
    ) {
        let key = egui::Id::new(("tide-preset", id));
        let user = super::presets_ui::cached_synth_presets(ui.ctx());
        let names: Vec<&str> = SynthPatch::PRESETS
            .iter()
            .copied()
            .chain(user.iter().map(|(_, u)| u.name.as_str()))
            .collect();
        let factory = SynthPatch::factory_all();
        let patch_of = |i: usize| -> &SynthPatch {
            if i < factory.len() {
                &factory[i]
            } else {
                &user[i - factory.len()].1.patch
            }
        };
        let n = names.len();
        let now = (0..n).find(|&i| p == patch_of(i));
        let mut idx = now
            .or_else(|| ui.ctx().data(|d| d.get_temp::<usize>(key)))
            .unwrap_or(0)
            .min(n - 1);
        let mut pick = None;
        let mut save = false;
        let mut load = false;

        egui::Frame::new()
            .fill(t::TIDE_DECK)
            .stroke(Stroke::new(1.0_f32, t::TIDE_LINE))
            .corner_radius(7.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);

                    // Bespoke Tide wave-crest emblem + wordmark.
                    let (mark, _) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::hover());
                    paint_tide_mark(ui.painter(), mark);
                    ui.label(
                        egui::RichText::new("TIDE")
                            .font(t::semibold(15.0))
                            .color(t::text()),
                    );

                    // Interactive signal-chain indicators: [A][B][C] -> [FLT] -> [FX]
                    let fx_on = p.fx.iter().filter(|s| s.enabled).count();
                    let chain_items = [
                        (
                            "A",
                            p.level > 0.0,
                            t::TIDE_CYAN,
                            0usize,
                            "Jump to Oscillator A",
                        ),
                        (
                            "B",
                            p.osc_b.level > 0.0,
                            t::TIDE_CYAN,
                            1,
                            "Jump to Oscillator B",
                        ),
                        (
                            "C",
                            p.spectral.level > 0.0,
                            t::TIDE_CYAN,
                            2,
                            "Jump to Oscillator C (Spectral)",
                        ),
                    ];
                    for (tag, active, col, tab_idx, tip) in chain_items {
                        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        let pr = ui.painter();
                        pr.rect_filled(
                            r,
                            4.0,
                            if active {
                                col.gamma_multiply(0.20)
                            } else {
                                t::TIDE_WELL
                            },
                        );
                        pr.rect_stroke(
                            r,
                            4.0,
                            Stroke::new(1.0_f32, if active { col } else { t::TIDE_LINE }),
                            egui::StrokeKind::Inside,
                        );
                        pr.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            tag,
                            t::mono(12.0),
                            if active { t::text() } else { t::faint() },
                        );
                        if kit::tip(resp, tip).clicked() {
                            view.osc_tab = tab_idx;
                        }
                    }
                    let fx_label = if fx_on > 0 {
                        format!("FX·{fx_on}")
                    } else {
                        "FX".into()
                    };
                    let (fx_r, fx_resp) = ui.allocate_exact_size(vec2(38.0, 22.0), Sense::click());
                    let pr = ui.painter();
                    pr.rect_filled(
                        fx_r,
                        4.0,
                        if fx_on > 0 {
                            t::TIDE_MINT.gamma_multiply(0.20)
                        } else {
                            t::TIDE_WELL
                        },
                    );
                    pr.rect_stroke(
                        fx_r,
                        4.0,
                        Stroke::new(
                            1.0_f32,
                            if fx_on > 0 {
                                t::TIDE_MINT
                            } else {
                                t::TIDE_LINE
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    pr.text(
                        fx_r.center(),
                        Align2::CENTER_CENTER,
                        fx_label,
                        t::mono(12.0),
                        if fx_on > 0 { t::text() } else { t::faint() },
                    );
                    if kit::tip(fx_resp, "Open Effects bay").clicked() {
                        view.deck_tab = 2;
                    }

                    ui.separator();

                    // Preset stepper & dropdown.
                    if ui.button("◀").on_hover_text("Previous preset").clicked() {
                        pick = Some((idx + n - 1) % n);
                    }
                    let shown = if now.is_some() {
                        names[idx].to_string()
                    } else {
                        format!("{} · edited", names[idx])
                    };
                    egui::ComboBox::from_id_salt(("tide-presets", id))
                        .width(156.0)
                        .height(460.0)
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            let head = |ui: &mut Ui, text: &str| {
                                ui.label(
                                    egui::RichText::new(text)
                                        .font(t::sans(12.0))
                                        .color(t::faint()),
                                );
                            };
                            // Factory presets by category, then the saved ones.
                            for cat in SynthPatch::CATEGORY_ORDER {
                                let mut first = true;
                                for (i, name) in names
                                    .iter()
                                    .enumerate()
                                    .take(SynthPatch::PRESETS.len())
                                    .filter(|(i, _)| SynthPatch::CATEGORIES[*i] == cat)
                                {
                                    if first && cat != "Init" {
                                        head(ui, cat);
                                    }
                                    first = false;
                                    if ui.selectable_label(Some(i) == now, *name).clicked() {
                                        pick = Some(i);
                                    }
                                }
                            }
                            for (i, name) in
                                names.iter().enumerate().skip(SynthPatch::PRESETS.len())
                            {
                                if i == SynthPatch::PRESETS.len() {
                                    ui.separator();
                                    head(ui, "Saved");
                                }
                                if ui.selectable_label(Some(i) == now, *name).clicked() {
                                    pick = Some(i);
                                }
                            }
                        });
                    if ui.button("▶").on_hover_text("Next preset").clicked() {
                        pick = Some((idx + 1) % n);
                    }

                    // A/B patch compare.
                    let ab_label = if view.ab_slot == 0 { "A/b" } else { "a/B" };
                    let ab_resp = ui.button(ab_label).on_hover_text(
                        "Compare A/B states · Shift+click copies current patch to the other slot",
                    );
                    if ab_resp.clicked() {
                        if ui.input(|i| i.modifiers.shift) {
                            view.ab_other = Some(p.clone());
                        } else {
                            let other = view.ab_other.replace(p.clone());
                            if let Some(prev) = other {
                                *p = prev;
                                ed.started = true;
                                ed.changed = true;
                            }
                            view.ab_slot = 1 - view.ab_slot;
                        }
                    }

                    if ui
                        .button("Mutate")
                        .on_hover_text("Subtle musical variation of wave, warp, filter and detune")
                        .clicked()
                    {
                        view.mutate_seq = view.mutate_seq.wrapping_add(1);
                        mutate_patch(p, view.mutate_seq ^ id);
                        ed.started = true;
                        ed.changed = true;
                    }
                    if ui
                        .button("Init")
                        .on_hover_text("Reset patch to clean Init state")
                        .clicked()
                    {
                        *p = SynthPatch::default();
                        ed.started = true;
                        ed.changed = true;
                    }
                    save = ui
                        .button("Save…")
                        .on_hover_text("Save this patch as a preset (Presets/Synth)")
                        .clicked();
                    load = ui
                        .button("Load…")
                        .on_hover_text("Open a saved .kfxsynth preset")
                        .clicked();
                });
            });

        if let Some(i) = pick {
            idx = i;
            *p = patch_of(i).clone();
            ed.started = true;
            ed.changed = true;
        }
        ui.ctx().data_mut(|d| d.insert_temp(key, idx));
        if save {
            let name = if now.is_some() {
                names[idx].to_string()
            } else {
                format!("{} edit", names[idx])
            };
            if let Some(InstrumentKind::Synth { patch }) = self
                .song
                .instruments
                .iter_mut()
                .find(|i| i.id == id)
                .map(|i| &mut i.kind)
            {
                *patch = p.clone();
            }
            self.save_synth_preset(ui.ctx(), id, &name);
        }
        if load {
            self.load_preset_dialog_for(super::presets_ui::Target::Synth(Some(id)));
            if let Some(InstrumentKind::Synth { patch }) = self
                .song
                .instruments
                .iter_mut()
                .find(|i| i.id == id)
                .map(|i| &mut i.kind)
            {
                *p = patch.clone();
            }
        }
    }

    fn tide_osc_bay(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
    ) {
        let modulated = p.modulated();
        tide_bay(ui, |ui| {
            // Oscillator switcher tabs with power LED + mini waveform thumbnails.
            let tab_w = ((ui.available_width() - 12.0) / 3.0).max(112.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (pick_a, pwr_a) = osc_tab_button(
                    ui,
                    ui.id().with(("osc-a", id)),
                    tab_w,
                    "OSC A",
                    view.osc_tab == 0,
                    p.level,
                    |pr, r, col| {
                        mini_wave_thumb(pr, r, p.table, p.position, p.warp, p.warp_amount, col)
                    },
                );
                if pick_a {
                    view.osc_tab = 0;
                }
                if pwr_a {
                    if p.level > 0.0 {
                        view.saved_levels[0] = p.level;
                        p.level = 0.0;
                    } else {
                        p.level = view.saved_levels[0].max(0.75);
                    }
                    ed.started = true;
                    ed.changed = true;
                }

                let (pick_b, pwr_b) = osc_tab_button(
                    ui,
                    ui.id().with(("osc-b", id)),
                    tab_w,
                    "OSC B",
                    view.osc_tab == 1,
                    p.osc_b.level,
                    |pr, r, col| {
                        mini_wave_thumb(
                            pr,
                            r,
                            p.osc_b.table,
                            p.osc_b.position,
                            p.osc_b.warp,
                            p.osc_b.warp_amount,
                            col,
                        )
                    },
                );
                if pick_b {
                    view.osc_tab = 1;
                }
                if pwr_b {
                    if p.osc_b.level > 0.0 {
                        view.saved_levels[1] = p.osc_b.level;
                        p.osc_b.level = 0.0;
                    } else {
                        p.osc_b.level = view.saved_levels[1].max(0.65);
                    }
                    ed.started = true;
                    ed.changed = true;
                }

                let (pick_c, pwr_c) = osc_tab_button(
                    ui,
                    ui.id().with(("osc-c", id)),
                    tab_w,
                    "OSC C",
                    view.osc_tab == 2,
                    p.spectral.level,
                    |pr, r, col| mini_spec_thumb(pr, r, &p.spectral, col),
                );
                if pick_c {
                    view.osc_tab = 2;
                }
                if pwr_c {
                    if p.spectral.level > 0.0 {
                        view.saved_levels[2] = p.spectral.level;
                        p.spectral.level = 0.0;
                    } else {
                        p.spectral.level = view.saved_levels[2].max(0.75);
                    }
                    ed.started = true;
                    ed.changed = true;
                }
            });

            match view.osc_tab {
                0 => {
                    tide_wave_scope(
                        ui,
                        ed,
                        p.table,
                        &mut p.position,
                        p.warp,
                        &mut p.warp_amount,
                        p.level > 0.0,
                        Some(modulated.position),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let cur_t = TABLES.iter().position(|t| *t == p.table).unwrap_or(0);
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-tbl-a-{id}"),
                            &TABLES.map(table_name),
                            cur_t,
                            t::TIDE_CYAN,
                        ) && i != cur_t
                        {
                            p.table = TABLES[i];
                            ed.started = true;
                            ed.changed = true;
                        }
                        let cur_w = WARPS.iter().position(|w| *w == p.warp).unwrap_or(0);
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-wrp-a-{id}"),
                            &WARPS.map(warp_name),
                            cur_w,
                            t::TIDE_CYAN,
                        ) && i != cur_w
                        {
                            p.warp = WARPS[i];
                            ed.started = true;
                            ed.changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let c = t::TIDE_CYAN;
                        ed.knob(
                            ui,
                            "POS",
                            c,
                            &mut p.position,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Wavetable frame morph",
                        );
                        ed.knob(
                            ui,
                            "WARP",
                            c,
                            &mut p.warp_amount,
                            (-1.0, 1.0, 0.0),
                            false,
                            &signed,
                            "Phase warp depth",
                        );
                        ed.knob(
                            ui,
                            "LEVEL",
                            c,
                            &mut p.level,
                            (0.0, 1.0, 1.0),
                            false,
                            &pct,
                            "Oscillator A level",
                        );
                        ed.knob(
                            ui,
                            "COARSE",
                            c,
                            &mut p.coarse,
                            (-24.0, 24.0, 0.0),
                            false,
                            &|v| format!("{:+.0} st", v.round()),
                            "Pitch in semitones",
                        );
                        ed.knob(
                            ui,
                            "FINE",
                            c,
                            &mut p.fine,
                            (-100.0, 100.0, 0.0),
                            false,
                            &|v| format!("{:+.0} ct", v.round()),
                            "Pitch in cents",
                        );
                        ed.knob(
                            ui,
                            "SUB",
                            t::TIDE_MIST,
                            &mut p.sub,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Sub-octave sine",
                        );
                        ed.knob(
                            ui,
                            "NOISE",
                            t::TIDE_MIST,
                            &mut p.noise,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "White noise generator",
                        );
                    });
                }
                1 => {
                    let b = &mut p.osc_b;
                    tide_wave_scope(
                        ui,
                        ed,
                        b.table,
                        &mut b.position,
                        b.warp,
                        &mut b.warp_amount,
                        b.level > 0.0,
                        None,
                    );
                    ui.horizontal_wrapped(|ui| {
                        let cur_t = TABLES.iter().position(|t| *t == b.table).unwrap_or(0);
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-tbl-b-{id}"),
                            &TABLES.map(table_name),
                            cur_t,
                            t::TIDE_CYAN,
                        ) && i != cur_t
                        {
                            b.table = TABLES[i];
                            ed.started = true;
                            ed.changed = true;
                        }
                        let cur_w = WARPS.iter().position(|w| *w == b.warp).unwrap_or(0);
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-wrp-b-{id}"),
                            &WARPS.map(warp_name),
                            cur_w,
                            t::TIDE_CYAN,
                        ) && i != cur_w
                        {
                            b.warp = WARPS[i];
                            ed.started = true;
                            ed.changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let c = t::TIDE_CYAN;
                        ed.knob(
                            ui,
                            "POS",
                            c,
                            &mut b.position,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Oscillator B frame morph",
                        );
                        ed.knob(
                            ui,
                            "WARP",
                            c,
                            &mut b.warp_amount,
                            (-1.0, 1.0, 0.0),
                            false,
                            &signed,
                            "Oscillator B warp depth",
                        );
                        ed.knob(
                            ui,
                            "LEVEL",
                            c,
                            &mut b.level,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Oscillator B level (0 = off)",
                        );
                        ed.knob(
                            ui,
                            "COARSE",
                            c,
                            &mut b.coarse,
                            (-24.0, 24.0, 0.0),
                            false,
                            &|v| format!("{:+.0} st", v.round()),
                            "Pitch in semitones",
                        );
                        ed.knob(
                            ui,
                            "FINE",
                            c,
                            &mut b.fine,
                            (-100.0, 100.0, 0.0),
                            false,
                            &|v| format!("{:+.0} ct", v.round()),
                            "Pitch in cents",
                        );
                    });
                }
                _ => {
                    let c = &mut p.spectral;
                    tide_spectral_scope(ui, ed, c);
                    ui.horizontal_wrapped(|ui| {
                        let col = t::TIDE_CYAN;
                        ed.knob(
                            ui,
                            "LEVEL",
                            col,
                            &mut c.level,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Spectral additive level",
                        );
                        ed.knob(
                            ui,
                            "COARSE",
                            col,
                            &mut c.coarse,
                            (-24.0, 24.0, 0.0),
                            false,
                            &|v| format!("{:+.0} st", v.round()),
                            "Pitch in semitones",
                        );
                        let mut partials = c.partials as f64;
                        ed.knob(
                            ui,
                            "PARTIALS",
                            col,
                            &mut partials,
                            (1.0, keyframexl::synth::MAX_PARTIALS as f64, 16.0),
                            false,
                            &|v| format!("{:.0}", v.round()),
                            "Number of harmonics",
                        );
                        c.partials = partials
                            .round()
                            .clamp(1.0, keyframexl::synth::MAX_PARTIALS as f64)
                            as u8;
                        ed.knob(
                            ui,
                            "TILT",
                            col,
                            &mut c.tilt,
                            (-1.0, 1.0, 0.0),
                            false,
                            &signed,
                            "Dark (-) to bright (+)",
                        );
                        ed.knob(
                            ui,
                            "ODD",
                            col,
                            &mut c.odd,
                            (-1.0, 1.0, 0.0),
                            false,
                            &signed,
                            "+ odd harmonics, - even harmonics",
                        );
                        ed.knob(
                            ui,
                            "SHIMMER",
                            col,
                            &mut c.shimmer,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Harmonic shimmer motion",
                        );
                    });
                }
            }
        });
    }

    fn tide_filter_bay(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
    ) {
        tide_bay(ui, |ui| {
            ui.horizontal(|ui| {
                let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                ui.painter().rect_filled(sq, 1.5, t::TIDE_CORAL);
                ui.label(
                    egui::RichText::new("FILTER")
                        .font(t::mono(12.0))
                        .color(t::TIDE_MIST),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cur = FILTERS.iter().position(|f| *f == p.filter).unwrap_or(0);
                    if let Some(i) = tide_pills(
                        ui,
                        &format!("tide-flt-{id}"),
                        &["LP", "HP", "BP"],
                        cur,
                        t::TIDE_CORAL,
                    ) && i != cur
                    {
                        p.filter = FILTERS[i];
                        ed.started = true;
                        ed.changed = true;
                    }
                });
            });

            tide_filter_scope(ui, ed, p);

            ui.horizontal_wrapped(|ui| {
                let c = t::TIDE_CORAL;
                ed.knob(
                    ui,
                    "CUTOFF",
                    c,
                    &mut p.cutoff,
                    (20.0, 20000.0, 18000.0),
                    true,
                    &hz,
                    "Cutoff frequency",
                );
                ed.knob(
                    ui,
                    "RESO",
                    c,
                    &mut p.resonance,
                    (0.0, 1.0, 0.0),
                    false,
                    &pct,
                    "Filter resonance",
                );
                ed.knob(
                    ui,
                    "KEY",
                    c,
                    &mut p.keytrack,
                    (-1.0, 1.0, 0.0),
                    false,
                    &signed,
                    "Cutoff keytracking",
                );
                ed.knob(
                    ui,
                    "ENV",
                    c,
                    &mut p.env_amount,
                    (-1.0, 1.0, 0.0),
                    false,
                    &signed,
                    "Filter envelope depth",
                );
            });

            // Quick Voice summary strip (1-click unison / glide visibility without tab switch).
            egui::Frame::new()
                .fill(t::TIDE_WELL)
                .stroke(Stroke::new(1.0_f32, t::TIDE_LINE.gamma_multiply(0.7)))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(
                            egui::RichText::new("UNISON")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                        if ui
                            .small_button("−")
                            .on_hover_text("Fewer unison voices")
                            .clicked()
                            && p.unison > 1
                        {
                            p.unison -= 1;
                            ed.started = true;
                            ed.changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!("{}", p.unison))
                                .font(t::mono(13.0))
                                .color(t::TIDE_CYAN),
                        );
                        if ui
                            .small_button("+")
                            .on_hover_text("More unison voices")
                            .clicked()
                            && p.unison < 7
                        {
                            p.unison += 1;
                            ed.started = true;
                            ed.changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "DET {:.0}%  SPR {:.0}%  GLD {}",
                                p.detune * 100.0,
                                p.spread * 100.0,
                                if p.glide < 0.0011 {
                                    "off".into()
                                } else {
                                    secs(p.glide)
                                }
                            ))
                            .font(t::mono(12.0))
                            .color(t::TIDE_MIST),
                        );
                        if ui
                            .small_button("Edit")
                            .on_hover_text("Open Voices, Glide & Macros tab")
                            .clicked()
                        {
                            view.deck_tab = 3;
                        }
                    });
                });
        });
    }

    fn tide_lower_deck(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
        tempo: f64,
    ) {
        let mod_label = if p.mods.is_empty() {
            "LFO & Mod".to_string()
        } else {
            format!("LFO & Mod ({})", p.mods.len())
        };
        let fx_count = p.fx.iter().filter(|s| s.enabled).count();
        let fx_label = if fx_count == 0 {
            "Effects".to_string()
        } else {
            format!("Effects ({fx_count})")
        };
        let tabs = [
            "Envelopes",
            mod_label.as_str(),
            fx_label.as_str(),
            "Voices & Macros",
        ];

        ui.horizontal(|ui| {
            if let Some(i) = tide_pills(
                ui,
                &format!("tide-deck-{id}"),
                &tabs,
                view.deck_tab.min(3),
                t::TIDE_CYAN,
            ) {
                view.deck_tab = i;
            }
        });

        match view.deck_tab {
            0 => self.tide_deck_envelopes(ui, p, ed),
            1 => self.tide_deck_modulation(ui, id, p, view, ed, tempo),
            2 => self.tide_deck_effects(ui, id, p, ed),
            _ => self.tide_deck_voices_macros(ui, id, p, ed),
        }
    }

    fn tide_deck_envelopes(&mut self, ui: &mut Ui, p: &mut SynthPatch, ed: &mut Edit) {
        pair(ui, |ui, col| {
            if col == 0 {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(sq, 1.5, t::TIDE_MINT);
                        ui.label(
                            egui::RichText::new("AMP ENVELOPE")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                    });
                    envelope_view(ui, ed, &mut p.amp_env, t::TIDE_MINT);
                    envelope_knobs(ui, ed, &mut p.amp_env, t::TIDE_MINT);
                });
            } else {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(sq, 1.5, t::TIDE_CORAL);
                        ui.label(
                            egui::RichText::new("FILTER ENVELOPE")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                    });
                    envelope_view(ui, ed, &mut p.filter_env, t::TIDE_CORAL);
                    envelope_knobs(ui, ed, &mut p.filter_env, t::TIDE_CORAL);
                });
            }
        });
    }

    fn tide_deck_modulation(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
        tempo: f64,
    ) {
        pair(ui, |ui, col| {
            if col == 0 {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-lfo-sel-{id}"),
                            &["LFO 1", "LFO 2"],
                            view.lfo_tab.min(1),
                            t::TIDE_MINT,
                        ) {
                            view.lfo_tab = i;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let lfo = &mut p.lfos[view.lfo_tab.min(1)];
                            if tide_toggle(
                                ui,
                                "RETRIG",
                                lfo.retrigger,
                                t::TIDE_MINT,
                                "Restart LFO phase on every note",
                            )
                            .clicked()
                            {
                                lfo.retrigger = !lfo.retrigger;
                                ed.started = true;
                                ed.changed = true;
                            }
                            if tide_toggle(
                                ui,
                                "SYNC",
                                lfo.sync,
                                t::TIDE_MINT,
                                "Lock LFO rate to song tempo divisions",
                            )
                            .clicked()
                            {
                                lfo.sync = !lfo.sync;
                                ed.started = true;
                                ed.changed = true;
                            }
                        });
                    });

                    let which = view.lfo_tab.min(1);
                    let lfo = &mut p.lfos[which];
                    lfo_scope(ui, lfo, tempo);
                    let cur_s = SHAPES.iter().position(|s| *s == lfo.shape).unwrap_or(0);
                    if let Some(i) = tide_pills(
                        ui,
                        &format!("tide-lfo-shp-{id}-{which}"),
                        &["Off", "Sine", "Tri", "Saw", "Sq", "S&H"],
                        cur_s,
                        t::TIDE_MINT,
                    ) && i != cur_s
                    {
                        lfo.shape = SHAPES[i];
                        ed.started = true;
                        ed.changed = true;
                    }
                    if lfo.sync {
                        let cur_d = DIVISIONS
                            .iter()
                            .position(|d| *d == lfo.division)
                            .unwrap_or(2);
                        if let Some(i) = tide_pills(
                            ui,
                            &format!("tide-lfo-div-{id}-{which}"),
                            &DIVISIONS.map(division_name),
                            cur_d,
                            t::TIDE_MINT,
                        ) && i != cur_d
                        {
                            lfo.division = DIVISIONS[i];
                            ed.started = true;
                            ed.changed = true;
                        }
                    } else {
                        if lfo.rate <= 0.0 {
                            lfo.rate = 1.0;
                        }
                        ed.knob(
                            ui,
                            "RATE",
                            t::TIDE_MINT,
                            &mut lfo.rate,
                            (0.01, 30.0, 1.0),
                            true,
                            &|v| {
                                if v < 1.0 {
                                    format!("{v:.2} Hz")
                                } else {
                                    format!("{v:.1} Hz")
                                }
                            },
                            "LFO cycles per second",
                        );
                    }
                });
            } else {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(sq, 1.5, t::TIDE_MINT);
                        ui.label(
                            egui::RichText::new("MOD MATRIX")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if p.mods.len() < 16 && ui.small_button("+ Route").clicked() {
                                p.mods.push(ModRoute {
                                    source: ModSource::Lfo1,
                                    target: ModTarget::Cutoff,
                                    depth: 0.25,
                                });
                                ed.started = true;
                                ed.changed = true;
                            }
                        });
                    });

                    if p.mods.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No active routes. Add one below or pick a quick route:",
                            )
                            .font(t::sans(12.5))
                            .color(t::TIDE_MIST),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for (label, src, dst, depth) in [
                                ("LFO 1 → Cutoff", ModSource::Lfo1, ModTarget::Cutoff, 0.35),
                                ("LFO 1 → Pos", ModSource::Lfo1, ModTarget::Position, 0.25),
                                ("Vel → Cutoff", ModSource::Velocity, ModTarget::Cutoff, 0.40),
                                ("Wheel → Warp", ModSource::ModWheel, ModTarget::Warp, 0.50),
                            ] {
                                if ui.small_button(label).clicked() {
                                    p.mods.push(ModRoute {
                                        source: src,
                                        target: dst,
                                        depth,
                                    });
                                    ed.started = true;
                                    ed.changed = true;
                                }
                            }
                        });
                    }

                    let mut remove = None;
                    for (i, r) in p.mods.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            let (dot, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                            ui.painter().rect_filled(dot, 1.5, source_color(r.source));
                            egui::ComboBox::from_id_salt(("tide-msrc", id, i))
                                .width(92.0)
                                .selected_text(r.source.name())
                                .show_ui(ui, |ui| {
                                    kit::menu_style(ui);
                                    for s in ModSource::ALL {
                                        if ui.selectable_value(&mut r.source, s, s.name()).changed()
                                        {
                                            ed.started = true;
                                            ed.changed = true;
                                        }
                                    }
                                });
                            ui.label(egui::RichText::new("→").color(t::TIDE_MIST));
                            egui::ComboBox::from_id_salt(("tide-mdst", id, i))
                                .width(102.0)
                                .selected_text(r.target.name())
                                .show_ui(ui, |ui| {
                                    kit::menu_style(ui);
                                    for tg in ModTarget::ALL {
                                        if ui
                                            .selectable_value(&mut r.target, tg, tg.name())
                                            .changed()
                                        {
                                            ed.started = true;
                                            ed.changed = true;
                                        }
                                    }
                                });
                            let bar_w = (ui.available_width() - 32.0).clamp(64.0, 140.0);
                            let (s, c) = bipolar_bar(
                                ui,
                                ui.id().with(("tide-mdep", id, i)),
                                bar_w,
                                source_color(r.source),
                                &mut r.depth,
                                "Modulation depth",
                            );
                            ed.started |= s;
                            ed.changed |= c;
                            if ui.small_button("×").on_hover_text("Remove route").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        p.mods.remove(i);
                        ed.started = true;
                        ed.changed = true;
                    }
                });
            }
        });
    }

    fn tide_deck_effects(&mut self, ui: &mut Ui, id: u64, p: &mut SynthPatch, ed: &mut Edit) {
        use keyframexl::fx::{FxKind, FxSlot};
        use keyframexl::synth::FX_KINDS;
        type Knobs = &'static [(&'static str, usize, &'static str)];
        let knobs = |k: FxKind| -> Knobs {
            match k {
                FxKind::Drive => &[
                    ("DRIVE", 0, "Saturation gain"),
                    ("TONE", 1, "Saturated brightness"),
                    ("MIX", 3, "Dry/wet blend"),
                ],
                FxKind::Chorus => &[
                    ("DEPTH", 1, "Detune sway depth"),
                    ("RATE", 2, "Modulation speed"),
                    ("MIX", 3, "Dry/wet blend"),
                ],
                FxKind::Delay => &[
                    ("TIME", 0, "Echo time in steps"),
                    ("REPEAT", 1, "Feedback echoes"),
                    ("WET", 4, "Echo level"),
                ],
                _ => &[
                    ("SIZE", 0, "Room size"),
                    ("DAMP", 1, "High-frequency damping"),
                    ("WET", 4, "Reverb mix"),
                ],
            }
        };

        let cols = if ui.available_width() >= TWO_COLUMNS {
            4
        } else {
            2
        };
        let gap = 8.0;
        let col_w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;

        for row in FX_KINDS.chunks(cols) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &kind in row {
                    ui.allocate_ui_with_layout(
                        vec2(col_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            tide_bay(ui, |ui| {
                                let at = p.fx.iter().position(|s| s.kind == kind);
                                let on = at.is_some_and(|i| p.fx[i].enabled);
                                if tide_toggle(
                                    ui,
                                    kind.builtin().name,
                                    on,
                                    t::TIDE_CYAN,
                                    "Toggle this built-in synth effect",
                                )
                                .clicked()
                                {
                                    ed.started = true;
                                    ed.changed = true;
                                    match at {
                                        Some(i) => p.fx[i].enabled = !p.fx[i].enabled,
                                        None => {
                                            p.fx.push(FxSlot::new(kind));
                                            p.fx.sort_by_key(|s| {
                                                FX_KINDS.iter().position(|k| *k == s.kind)
                                            });
                                        }
                                    }
                                }
                                let default_slot = FxSlot::new(kind);
                                let cur_params: &[f32] =
                                    p.fx.iter()
                                        .find(|s| s.kind == kind)
                                        .map(|s| s.params.as_slice())
                                        .unwrap_or(&default_slot.params);
                                fx_mini_scope(ui, kind, cur_params, on);

                                let specs = kind.params();
                                let slot_idx = p.fx.iter().position(|s| s.kind == kind);
                                ui.add_enabled_ui(on, |ui| {
                                    for &(label, n, hint) in knobs(kind) {
                                        let Some(spec) = specs.get(n) else { continue };
                                        let mut v = slot_idx
                                            .map(|i| p.fx[i].params[n] as f64)
                                            .unwrap_or(spec.default as f64);
                                        let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
                                        ui.push_id((id, kind.builtin().name, label, n), |ui| {
                                            ed.knob(
                                                ui,
                                                label,
                                                if on { t::TIDE_CYAN } else { t::strong() },
                                                &mut v,
                                                (
                                                    spec.min as f64,
                                                    spec.max as f64,
                                                    spec.default as f64,
                                                ),
                                                spec.log,
                                                &fmt,
                                                hint,
                                            );
                                        });
                                        if let Some(i) = slot_idx {
                                            p.fx[i].params[n] = v as f32;
                                        }
                                    }
                                });
                            });
                        },
                    );
                }
            });
        }
    }

    fn tide_deck_voices_macros(&mut self, ui: &mut Ui, id: u64, p: &mut SynthPatch, ed: &mut Edit) {
        pair(ui, |ui, col| {
            if col == 0 {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(sq, 1.5, t::TIDE_CYAN);
                        ui.label(
                            egui::RichText::new("VOICES, GLIDE & MPE")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if tide_toggle(
                                ui,
                                "MPE",
                                p.mpe,
                                t::TIDE_CYAN,
                                "Per-note pitch bend (±48 st), pressure and slide (channels 2-16)",
                            )
                            .clicked()
                            {
                                p.mpe = !p.mpe;
                                ed.started = true;
                                ed.changed = true;
                            }
                        });
                    });
                    unison_fan_scope(ui, p.unison, p.detune, p.spread);
                    ui.horizontal_wrapped(|ui| {
                        let c = t::TIDE_CYAN;
                        let mut voices = p.unison as f64;
                        ed.knob(
                            ui,
                            "VOICES",
                            c,
                            &mut voices,
                            (1.0, 7.0, 1.0),
                            false,
                            &|v| format!("{:.0}", v.round()),
                            "Stacked unison voices",
                        );
                        p.unison = voices.round().clamp(1.0, 7.0) as u8;
                        ed.knob(
                            ui,
                            "DETUNE",
                            c,
                            &mut p.detune,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Unison pitch spread",
                        );
                        ed.knob(
                            ui,
                            "SPREAD",
                            c,
                            &mut p.spread,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            "Stereo width",
                        );
                        ed.knob(
                            ui,
                            "GLIDE",
                            t::enc_white(),
                            &mut p.glide,
                            (0.0, 10.0, 0.0),
                            true,
                            &|v| if v < 0.0011 { "off".into() } else { secs(v) },
                            "Portamento glide time",
                        );
                        ed.knob(
                            ui,
                            "BEND",
                            t::enc_white(),
                            &mut p.bend_range,
                            (0.0, 48.0, 2.0),
                            false,
                            &|v| format!("±{:.0} st", v.round()),
                            "MIDI pitch wheel range",
                        );
                        p.bend_range = p.bend_range.round();
                    });
                });
            } else {
                tide_bay(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(sq, 1.5, t::enc_white());
                        ui.label(
                            egui::RichText::new("MACRO ROUTING")
                                .font(t::mono(12.0))
                                .color(t::TIDE_MIST),
                        );
                    });
                    let cols = 2;
                    let width = (ui.available_width() - 10.0) * 0.5;
                    for (r, macros) in p.macros.chunks_mut(cols).enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            for (k, m) in macros.iter_mut().enumerate() {
                                let i = r * cols + k;
                                ui.vertical(|ui| {
                                    ui.set_width(width);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "MACRO {}  ({})",
                                            i + 1,
                                            pct(m.value)
                                        ))
                                        .font(t::mono(12.0))
                                        .color(t::text()),
                                    );
                                    let free =
                                        m.routes.iter().position(|rt| rt.depth.abs() <= 0.001);
                                    for (j, route) in m.routes.iter_mut().enumerate() {
                                        if route.depth.abs() <= 0.001 {
                                            continue;
                                        }
                                        ui.horizontal(|ui| {
                                            egui::ComboBox::from_id_salt(("tide-route", id, i, j))
                                                .width((width - 82.0).max(70.0))
                                                .selected_text(param_name(route.target))
                                                .show_ui(ui, |ui| {
                                                    kit::menu_style(ui);
                                                    for v in PARAMS {
                                                        if ui
                                                            .selectable_value(
                                                                &mut route.target,
                                                                v,
                                                                param_name(v),
                                                            )
                                                            .changed()
                                                        {
                                                            ed.started = true;
                                                            ed.changed = true;
                                                        }
                                                    }
                                                });
                                            let (s, c) = bipolar_bar(
                                                ui,
                                                ui.id().with(("tide-mbar", id, i, j)),
                                                68.0,
                                                t::TIDE_CYAN,
                                                &mut route.depth,
                                                "Macro route depth (double-click removes)",
                                            );
                                            ed.started |= s;
                                            ed.changed |= c;
                                        });
                                    }
                                    if let Some(j) = free {
                                        let add = egui::RichText::new("+ Target")
                                            .font(t::sans(12.0))
                                            .color(t::TIDE_MIST);
                                        egui::ComboBox::from_id_salt(("tide-add", id, i))
                                            .width((width - 24.0).max(80.0))
                                            .selected_text(add)
                                            .show_ui(ui, |ui| {
                                                kit::menu_style(ui);
                                                for v in PARAMS {
                                                    if ui
                                                        .selectable_label(false, param_name(v))
                                                        .clicked()
                                                    {
                                                        m.routes[j] = keyframexl::synth::Route {
                                                            target: v,
                                                            depth: 0.5,
                                                        };
                                                        ed.started = true;
                                                        ed.changed = true;
                                                    }
                                                }
                                            });
                                    }
                                });
                            }
                        });
                    }
                });
            }
        });
    }

    /// Pinned performance strip: 4 live Macros on the left, 2-octave Audition Keyboard on the right.
    fn tide_performance_footer(
        &mut self,
        ui: &mut Ui,
        id: u64,
        p: &mut SynthPatch,
        view: &mut TideView,
        ed: &mut Edit,
    ) {
        egui::Frame::new()
            .fill(t::TIDE_DECK)
            .stroke(Stroke::new(1.0_f32, t::TIDE_LINE))
            .corner_radius(7.0)
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let wide = ui.available_width() >= TWO_COLUMNS;
                let short_param = |param: Param| match param {
                    Param::Position => "POS",
                    Param::Warp => "WARP",
                    Param::Cutoff => "CUT",
                    Param::Resonance => "RESO",
                    Param::Sub => "SUB",
                    Param::Noise => "NOISE",
                    Param::Detune => "DET",
                    Param::Spread => "SPR",
                };
                let draw_macros = |ui: &mut Ui, p: &mut SynthPatch, ed: &mut Edit| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, m) in p.macros.iter_mut().enumerate() {
                            let assigned = m
                                .routes
                                .iter()
                                .find(|r| r.depth.abs() > 0.001)
                                .map(|r| short_param(r.target));
                            let label = match assigned {
                                Some(tgt) => format!("M{} {tgt}", i + 1),
                                None => format!("MACRO {}", i + 1),
                            };
                            ed.knob(
                                ui,
                                &label,
                                if assigned.is_some() {
                                    t::TIDE_CYAN
                                } else {
                                    t::TIDE_MIST
                                },
                                &mut m.value,
                                (0.0, 1.0, 0.0),
                                false,
                                &pct,
                                "Performance macro: moves its assigned targets in Voices & Macros",
                            );
                        }
                    });
                };
                if wide {
                    let avail = ui.available_width();
                    let left_w = 424.0_f32.min((avail * 0.54).floor());
                    let right_w = (avail - left_w - 8.0).max(200.0);
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.allocate_ui_with_layout(
                            vec2(left_w, 40.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| draw_macros(ui, p, ed),
                        );
                        ui.allocate_ui_with_layout(
                            vec2(right_w, 40.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| audition_keyboard(self, ui, id, &mut view.kb_octave, t::TIDE_CYAN),
                        );
                    });
                } else {
                    draw_macros(ui, p, ed);
                    audition_keyboard(self, ui, id, &mut view.kb_octave, t::TIDE_CYAN);
                }
            });
    }
}

#[cfg(test)]
mod tide_ui_tests {
    use super::*;

    #[test]
    fn warp_phase_stays_finite_and_bounded() {
        for warp in WARPS {
            for amt in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                for i in 0..=20 {
                    let ph = i as f64 / 20.0;
                    let w = warp_phase(ph, warp, amt);
                    assert!(
                        w.is_finite() && (0.0..=1.0).contains(&w),
                        "warp {warp:?} amt {amt} ph {ph} -> {w}"
                    );
                }
            }
        }
    }

    #[test]
    fn mutate_patch_always_produces_valid_patch() {
        for seed in 1..25 {
            let mut p = SynthPatch::factory((seed as usize) % SynthPatch::PRESETS.len());
            mutate_patch(&mut p, seed);
            assert!(p.validate().is_ok(), "seed {seed} produced invalid patch");
        }
    }
}
