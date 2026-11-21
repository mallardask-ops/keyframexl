//! The physical look, painted: one light source from above, two depths
//! (raised and sunken), soft shadow only on panels. Port of the BMS kit's
//! `chrome.py` plus the TE pieces (segment LCD, colour-coded encoders, pads).

use crate::theme as t;
use eframe::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui,
    Vec2, epaint, pos2, vec2,
};
use std::f32::consts::PI;

pub const RADIUS: f32 = 7.0;
pub const EDGE_W: f32 = 1.0;
pub const DEPTH: f32 = 1.0;
pub const CONTROL_H: f32 = 34.0;
pub const HOVER_S: f32 = 0.12;

/// Menu row padding: a fixed check gutter on the left, so every label in
/// every dropdown starts at the same x (FL). egui resets menu padding to
/// (2, 0) per popup, so each menu contents calls [`menu_style`] again.
pub const MENU_PAD: Vec2 = vec2(24.0, 2.0);

/// Call first in every menu / submenu contents closure.
pub fn menu_check(ui: &mut Ui, on: bool, label: &str, key: &str) -> Response {
    let b = egui::Button::new(egui::RichText::new(label).font(t::sans(13.0)))
        .shortcut_text(egui::RichText::new(key).font(t::mono(12.0)));
    let r = ui.add(b);
    if on {
        let c = pos2(r.rect.left() + MENU_PAD.x / 2.0, r.rect.center().y);
        ui.painter()
            .text(c, Align2::CENTER_CENTER, "✓", t::sans(13.0), t::text());
    }
    r
}

/// A section header inside a menu, indented to the row labels.
pub fn menu_header(ui: &mut Ui, text: egui::RichText) {
    ui.horizontal(|ui| {
        ui.add_space(MENU_PAD.x);
        ui.label(text);
    });
}

pub fn mix(a: C, b: C, k: f32) -> C {
    let k = k.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    C::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// Rounded rectangle with a vertical gradient, anti-aliased like any egui fill.
#[allow(dead_code)]
pub fn gradient_rrect(p: &Painter, rect: Rect, radius: f32, top: C, bottom: C) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut tess = epaint::Tessellator::new(
        p.ctx().pixels_per_point(),
        epaint::TessellationOptions::default(),
        [1, 1],
        vec![],
    );
    let mut mesh = epaint::Mesh::default();
    tess.tessellate_rect(
        &epaint::RectShape::filled(rect, radius, C::WHITE),
        &mut mesh,
    );
    let h = rect.height().max(1.0);
    for v in &mut mesh.vertices {
        let k = (v.pos.y - rect.top()) / h;
        let coverage = v.color.a() as f32 / 255.0;
        v.color = mix(top, bottom, k).gamma_multiply(coverage);
    }
    p.add(Shape::mesh(mesh));
}

/// Convex shape: thick dark outline, a lip under it, a lit top edge. Returns the face.
pub fn raised(
    p: &Painter,
    rect: Rect,
    radius: f32,
    top: C,
    bottom: C,
    edge: C,
    pressed: bool,
    depth: f32,
) -> Rect {
    p.rect_filled(rect, radius, edge);
    let mut face = Rect::from_min_max(
        rect.min + vec2(EDGE_W, EDGE_W),
        rect.max - vec2(EDGE_W, EDGE_W + if pressed { 0.0 } else { depth }),
    );
    if pressed {
        face.min.y += depth.min(1.0);
    }
    let inner = (radius - EDGE_W).max(1.0);
    let (a, b) = if pressed {
        (bottom, top)
    } else {
        (top, bottom)
    };
    p.rect_filled(face, inner, mix(a, b, 0.5));
    if !pressed && face.width() > 2.0 * inner {
        let y = face.top() + 0.5;
        p.line_segment(
            [
                pos2(face.left() + inner * 0.6, y),
                pos2(face.right() - inner * 0.6, y),
            ],
            Stroke::new(1.0_f32, t::hi()),
        );
    }
    face
}

/// Recessed well: darkest thing on the page, thick outline, inner shadow on top.
pub fn sunken(p: &Painter, rect: Rect, radius: f32) -> Rect {
    p.rect_filled(rect, radius, t::edge());
    let well = rect.shrink(EDGE_W);
    let inner = (radius - EDGE_W).max(1.0);
    p.rect_filled(well, inner, t::sunken());
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

/// Raised control face in the neutral palette, with hover mixing.
pub fn control(p: &Painter, rect: Rect, radius: f32, hover: f32, pressed: bool) -> Rect {
    raised(
        p,
        rect,
        radius,
        mix(t::ctl_top(), t::ctl_hover(), hover),
        mix(t::ctl_bottom(), t::ctl_top(), hover),
        t::edge(),
        pressed,
        DEPTH,
    )
}

/// The lifted element of a track: active tab, current menu row.
pub fn thumb(p: &Painter, rect: Rect) -> Rect {
    raised(
        p,
        rect,
        RADIUS - 1.0,
        t::ctl_top(),
        t::ctl_bottom(),
        t::edge(),
        false,
        1.0,
    )
}

pub fn focus_ring(p: &Painter, rect: Rect, radius: f32) {
    p.rect_stroke(
        rect.expand(2.0),
        radius + 2.0,
        Stroke::new(1.5_f32, t::focus()),
        egui::StrokeKind::Outside,
    );
}

fn kbnav_id() -> egui::Id {
    egui::Id::new("kfxl-keyboard-nav")
}

/// Track whether the last navigation was keyboard (Tab) or pointer. Call once per frame.
pub fn track_keyboard_nav(ctx: &egui::Context) {
    let (tab, press) = ctx.input(|i| {
        (
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Tab,
                        pressed: true,
                        ..
                    }
                )
            }),
            i.pointer.any_pressed(),
        )
    });
    if tab || press {
        ctx.data_mut(|d| d.insert_temp(kbnav_id(), tab));
    }
}

/// Focus ring only after keyboard navigation, like the kit (State_KeyboardFocusChange).
pub fn show_focus(ui: &Ui, r: &Response) -> bool {
    r.has_focus()
        && ui
            .ctx()
            .data(|d| d.get_temp::<bool>(kbnav_id()).unwrap_or(false))
}

pub fn hover_amount(ui: &Ui, r: &Response) -> f32 {
    ui.ctx()
        .animate_bool_with_time(r.id.with("hover"), r.hovered(), HOVER_S)
}

/// Engraved caps label: uppercase mono with tracking, the TE legend.
pub fn caps_galley(p: &Painter, text: &str, size: f32, color: C) -> std::sync::Arc<egui::Galley> {
    // Type floor: no label below 12 px (both audits: 10–11 px caps were unreadable).
    let size = size.max(12.0);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: t::mono(size),
            color,
            extra_letter_spacing: size * 0.08,
            ..Default::default()
        },
    );
    p.layout_job(job)
}

pub fn caps(p: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: C) -> Rect {
    let g = caps_galley(p, text, size, color);
    let r = align.anchor_size(pos, g.size());
    p.galley(r.min, g, color);
    r
}

pub fn text_width(p: &Painter, s: &str, font: FontId) -> f32 {
    p.layout_no_wrap(s.to_owned(), font, C::WHITE).size().x
}

/// Shorten text with an ellipsis so it fits `max_w`.
pub fn elide(p: &Painter, s: &str, font: FontId, max_w: f32) -> String {
    if text_width(p, s, font.clone()) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
            + "…";
        if text_width(p, &cand, font.clone()) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string()
        + "…"
}

/// Caps label cut to a width (caps have tracking, so measure the galley).
#[allow(dead_code)]
pub fn caps_fit(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    color: C,
    max_w: f32,
) -> Rect {
    let fits = |s: &str| caps_galley(p, s, size, color).size().x <= max_w;
    if fits(text) {
        return caps(p, pos, align, text, size, color);
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand = chars[..mid]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
            + "…";
        if fits(&cand) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let shown = chars[..lo]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string()
        + "…";
    caps(p, pos, align, &shown, size, color)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    /// The transport Play/Pause key: brand face, white ink.
    Brand,
    Normal,
    Ghost,
}

/// Kit button: 34px, raised, Sans 14 (Primary: Medium, brand face, white ink).
pub fn button(ui: &mut Ui, label: &str, kind: Kind, enabled: bool) -> Response {
    let font = if matches!(kind, Kind::Primary | Kind::Brand) {
        t::medium(14.0)
    } else {
        t::sans(14.0)
    };
    let w = text_width(ui.painter(), label, font.clone()) + 32.0;
    button_sized(
        ui,
        vec2(w, CONTROL_H),
        kind,
        enabled,
        label,
        |p, face, ink| {
            p.text(
                face.center(),
                Align2::CENTER_CENTER,
                label,
                font.clone(),
                ink,
            );
        },
    )
}

pub fn button_sized(
    ui: &mut Ui,
    size: Vec2,
    kind: Kind,
    enabled: bool,
    name: &str,
    content: impl FnOnce(&Painter, Rect, C),
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    paint_button(ui, rect, &r, kind, enabled, name, content);
    r
}

pub fn paint_button(
    ui: &Ui,
    rect: Rect,
    r: &Response,
    kind: Kind,
    enabled: bool,
    name: &str,
    content: impl FnOnce(&Painter, Rect, C),
) {
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = ui.painter();
    let hover = if enabled { hover_amount(ui, r) } else { 0.0 };
    let down = enabled && r.is_pointer_button_down_on();
    let rect = rect.shrink(0.5);
    let (face, ink) = if !enabled {
        (control(p, rect, RADIUS, 0.0, false), t::faint())
    } else {
        match kind {
            Kind::Brand => (
                raised(
                    p,
                    rect,
                    RADIUS,
                    mix(t::brand_top(), t::brand_glow(), hover),
                    mix(t::brand_fill(), t::brand_top(), hover),
                    t::brand_edge(),
                    down,
                    DEPTH,
                ),
                t::on_brand(),
            ),
            Kind::Ghost if hover == 0.0 && !down => (rect.shrink(EDGE_W), t::muted()),
            _ => (control(p, rect, RADIUS, hover, down), t::text()),
        }
    };
    content(p, face, ink);
    if enabled && show_focus(ui, r) {
        focus_ring(p, rect, RADIUS);
    }
}

/// FL hint bar: the hovered control's hint, shown in the deck's second row
/// (read one frame later, which nobody can see).
pub fn set_hint(ctx: &egui::Context, text: &str) {
    let pass = ctx.cumulative_pass_nr();
    let line = text.replace('\n', "  ·  ");
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-hint"), (pass, line)));
}

/// The hint of the control under the pointer, if one set it last frame.
pub fn hovered_hint(ctx: &egui::Context) -> Option<String> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(u64, String)>(egui::Id::new("kfxl-hint")))
        .filter(|(n, _)| pass.saturating_sub(*n) <= 1)
        .map(|(_, s)| s)
}

/// Hint bar only, no tooltip: for controls on the deck, where a tooltip
/// would cover the hint bar that already says the same.
pub fn bar_hint(r: Response, text: impl Into<String>) -> Response {
    if r.hovered() {
        set_hint(&r.ctx, &text.into());
    }
    r
}

/// Tooltip and hint bar in one call: use instead of `on_hover_text`.
pub fn tip(r: Response, text: impl Into<String>) -> Response {
    let text = text.into();
    if r.hovered() {
        set_hint(&r.ctx, &text);
    }
    r.on_hover_text(text)
}

/// Toolbar height for the deck's second row and window headers.
pub const TOOL_H: f32 = 26.0;

/// Compact toolbar button: 26px, Sans 13.
pub fn tool_button(ui: &mut Ui, label: &str, kind: Kind, enabled: bool) -> Response {
    let font = t::sans(13.0);
    let w = text_width(ui.painter(), label, font.clone()) + 20.0;
    button_sized(ui, vec2(w, TOOL_H), kind, enabled, label, |p, face, ink| {
        p.text(
            face.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            ink,
        );
    })
}

/// Toolbar dropdown: a compact button showing `label` and a caret; the menu
/// opens below it. Returns the button response.
/// A choice list: the stock `egui::ComboBox` (keyboard, Esc, click-outside,
/// screen clamping and scrolling come with it).
pub fn dropdown(
    ui: &mut Ui,
    id: &str,
    label: &str,
    width: f32,
    menu: impl FnOnce(&mut Ui),
) -> Response {
    let shown = elide(ui.painter(), label, t::sans(13.0), width - 28.0);
    egui::ComboBox::from_id_salt(("dropdown", id))
        .selected_text(egui::RichText::new(shown).font(t::sans(13.0)))
        .width(width)
        .height(360.0)
        .show_ui(ui, |ui| {
            menu_style(ui);
            ui.set_min_width(width.max(160.0));
            menu(ui);
        })
        .response
}

/// Menu look for a popup body: text rows, highlight on hover, no frames.
/// The left padding is the fixed check gutter, so every label in every
/// dropdown starts at the same x (FL).
pub fn menu_style(ui: &mut Ui) {
    let s = ui.style_mut();
    s.spacing.button_padding = MENU_PAD;
    s.spacing.item_spacing = vec2(6.0, 1.0);
    s.spacing.interact_size.y = 24.0;
    s.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    // Keep noninteractive.weak_bg_fill opaque (the menu fill) so egui's
    // fade_out_to_color() greys out disabled rows instead of fading them to
    // alpha 0, and keep bg_stroke visible so ui.separator() renders a hairline.
    s.visuals.widgets.noninteractive.weak_bg_fill = t::panel_top();
    s.visuals.widgets.noninteractive.bg_fill = t::panel_top();
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, t::line());
    s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    s.visuals.widgets.active.bg_stroke = Stroke::NONE;
    s.wrap_mode = Some(egui::TextWrapMode::Extend);
}

/// A popup list under `anchor` (left-click menus): menu look, bounded height, scrolls.
pub fn popup_menu(
    ui: &mut Ui,
    id: egui::Id,
    anchor: &Response,
    min_width: f32,
    add: impl FnOnce(&mut Ui),
) {
    egui::popup_below_widget(
        ui,
        id,
        anchor,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            menu_style(ui);
            ui.set_min_width(min_width);
            let max_h = (ui.ctx().screen_rect().height() - 120.0).clamp(160.0, 480.0);
            egui::ScrollArea::vertical().max_height(max_h).show(ui, add);
        },
    );
}

/// Segmented control: sunken track, raised sliding thumb. Returns the clicked index.
pub fn chips(
    ui: &mut Ui,
    id: &str,
    items: &[&str],
    selected: usize,
    widths: Option<&[f32]>,
) -> (Option<usize>, Rect) {
    let font = t::sans(13.0);
    let ws: Vec<f32> = match widths {
        Some(w) => w.to_vec(),
        None => items
            .iter()
            .map(|s| text_width(ui.painter(), s, font.clone()) + 26.0)
            .collect(),
    };
    let total = ws.iter().sum::<f32>() + 6.0;
    let (track, _) = ui.allocate_exact_size(vec2(total, CONTROL_H), Sense::hover());
    let p = ui.painter().clone();
    sunken(&p, track, 8.0);
    let mut x = track.left() + 3.0;
    let mut rects = Vec::new();
    for w in &ws {
        rects.push(Rect::from_min_size(
            pos2(x, track.top() + 3.0),
            vec2(*w, CONTROL_H - 6.0),
        ));
        x += w;
    }
    let tid = ui.id().with((id, "thumb"));
    let tx = ui
        .ctx()
        .animate_value_with_time(tid, rects[selected].left(), 0.2);
    let tw = ui
        .ctx()
        .animate_value_with_time(tid.with("w"), rects[selected].width(), 0.2);
    thumb(
        &p,
        Rect::from_min_size(
            pos2(tx, rects[selected].top()),
            vec2(tw, rects[selected].height()),
        ),
    );
    let mut clicked = None;
    for (i, (r, label)) in rects.iter().zip(items).enumerate() {
        let resp = ui.interact(*r, ui.id().with((id, i)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                i == selected,
                *label,
            )
        });
        let on = i == selected;
        let ink = if on || resp.hovered() {
            t::text()
        } else {
            t::muted()
        };
        let f = if on { t::medium(13.0) } else { font.clone() };
        p.text(
            r.center() - vec2(0.0, 0.5),
            Align2::CENTER_CENTER,
            *label,
            f,
            ink,
        );
        if show_focus(ui, &resp) {
            focus_ring(&p, r.shrink(1.0), RADIUS - 1.0);
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    (clicked, track)
}

/// A labelled LED: lit is solid colour, unlit is a dark dimple. No glow.
pub fn led(p: &Painter, c: Pos2, r: f32, lit: Option<C>) {
    p.circle_filled(c, r + 1.0, t::edge());
    match lit {
        Some(col) => p.circle_filled(c, r, col),
        None => p.circle_filled(c, r, t::lcd_off()),
    };
}

// ---------------------------------------------------------------- encoder

pub struct Knob<'a> {
    pub label: &'a str,
    pub color: C,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// Turn on a log scale (times, frequencies, rates). `min` may be 0: the
    /// bottom of the travel then snaps to 0.
    pub log: bool,
    pub fmt: &'a dyn Fn(f64) -> String,
    pub hint: &'a str,
}

/// Lowest value a log knob travels to before snapping to 0.
const LOG_FLOOR: f64 = 1e-4;
/// Pixels of vertical drag for the full travel (Shift: ten times finer).
const DRAG_TRAVEL: f64 = 200.0;
/// Wheel points per notch (egui line scroll) and notches for the full travel.
const NOTCH: f64 = 50.0;
const NOTCHES: f64 = 100.0;
/// Wheel turns closer together than this are one undo step.
const WHEEL_GESTURE: f64 = 0.6;

impl Knob<'_> {
    /// Value -> travel position 0..1.
    fn to_pos(&self, v: f64) -> f64 {
        if self.log {
            if self.min == 0.0 && self.max > 0.0 {
                // A log knob that starts at 0 has a huge dead zone at the
                // bottom (the first pixels only move it by microseconds). A
                // square curve makes 0 respond immediately and still reaches
                // the long times smoothly.
                return ((v.max(0.0) / self.max).sqrt()).clamp(0.0, 1.0);
            }
            let lo = self.min.max(LOG_FLOOR).ln();
            let hi = self.max.ln();
            ((v.max(self.min.max(LOG_FLOOR)).ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
        } else {
            ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
    /// Travel position 0..1 -> value.
    fn at_pos(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        if self.log {
            if self.min == 0.0 && self.max > 0.0 {
                return self.max * x * x;
            }
            if self.min <= 0.0 && x <= 1e-6 {
                return 0.0;
            }
            let lo = self.min.max(LOG_FLOOR).ln();
            (lo + x * (self.max.ln() - lo))
                .exp()
                .clamp(self.min, self.max)
        } else {
            self.min + x * (self.max - self.min)
        }
    }
}

#[cfg(test)]
mod knob_curve_tests {
    use super::*;

    fn fmt_time(v: f64) -> String {
        format!("{v}")
    }

    fn time_knob(max: f64) -> Knob<'static> {
        Knob {
            label: "Decay",
            color: t::enc_white(),
            min: 0.0,
            max,
            default: 0.0,
            log: true,
            fmt: &fmt_time,
            hint: "test",
        }
    }

    #[test]
    fn a_log_knob_from_zero_moves_with_the_first_pixels() {
        let k = time_knob(20.0);
        assert_eq!(k.at_pos(0.0), 0.0);
        // One pixel of a 200 px travel.
        let v = k.at_pos(0.005);
        assert!(v > 0.0, "still stuck at zero: {v}");
        assert!((v - 20.0 * 0.005 * 0.005).abs() < 1e-9);
        // And the curve is monotonic and reaches the top.
        assert!(k.at_pos(0.5) > k.at_pos(0.2));
        assert!((k.at_pos(1.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn the_curve_round_trips() {
        let k = time_knob(20.0);
        for x in [0.0, 0.01, 0.1, 0.5, 1.0] {
            let v = k.at_pos(x);
            assert!((k.to_pos(v) - x).abs() < 1e-9, "x {x}, v {v}");
        }
    }
}

/// Parse a typed value. Understands the units our knobs print: %, Hz/kHz,
/// ms/s, dB, st, ct. A bare number takes the unit the knob shows (`example`).
pub fn parse_value(text: &str, example: &str) -> Option<f64> {
    fn split(s: &str) -> Option<(f64, String)> {
        let s = s.trim();
        let end = s
            .char_indices()
            .find(|(i, c)| {
                !(c.is_ascii_digit()
                    || *c == '.'
                    || *c == ','
                    || ((*c == '-' || *c == '+') && *i == 0))
            })
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        let n: f64 = s[..end].replace(',', ".").parse().ok()?;
        Some((n, s[end..].trim().to_ascii_lowercase()))
    }
    let scale = |unit: &str| -> f64 {
        match unit {
            "%" => 0.01,
            "k" | "khz" => 1000.0,
            "ms" => 0.001,
            _ => 1.0,
        }
    };
    let (n, unit) = split(text)?;
    let unit = if unit.is_empty() {
        split(example).map(|e| e.1).unwrap_or_default()
    } else {
        unit
    };
    let v = n * scale(&unit);
    v.is_finite().then_some(v)
}

/// The knob value for a number typed in its display units. Most knobs show
/// their value as is; a knob whose readout is another scale (a volume fader
/// stores its travel but shows dB) is solved by bisection on its readout.
fn typed_to_value(k: &Knob, typed: f64) -> f64 {
    let shown = |v: f64| parse_value(&(k.fmt)(v), &(k.fmt)(v)).unwrap_or(f64::NEG_INFINITY);
    // A readout that does not parse (pan's "L20") is left to the raw path.
    let raw = |v: f64| {
        let s = shown(v);
        s == f64::NEG_INFINITY || (s - v).abs() <= 0.02 * v.abs().max(1e-3)
    };
    if raw(k.max) && raw(k.min + 0.37 * (k.max - k.min)) {
        return typed;
    }
    let (mut lo, mut hi) = (k.min, k.max);
    if typed <= shown(lo) {
        return lo;
    }
    if typed >= shown(hi) {
        return hi;
    }
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if shown(mid) < typed {
            lo = mid
        } else {
            hi = mid
        }
    }
    // Below every readout but the bottom one (a fader's "−∞ dB"): the bottom.
    if shown(hi) - typed > 1.0 { k.min } else { hi }
}

#[derive(Clone, Default)]
struct KnobMemory {
    /// Time of the last wheel turn, to group turns into one undo step.
    wheel_at: Option<f64>,
    /// Typed entry in progress.
    typing: Option<String>,
    bad: bool,
}

/// Copied knob value (FL: Copy value / Paste value), shared by every knob.
fn clip_id() -> egui::Id {
    egui::Id::new("kfxl-knob-clipboard")
}

/// Where a knob's typed-value editor goes.
enum TypeAt {
    /// Over the printed value next to the knob.
    InPlace(Rect),
    /// A small floating field under the knob (mini knobs print no value).
    Below,
}

/// Pointer movement for a drag gesture, in points: the plain per-frame delta.
/// (An older version warped the cursor back with `CursorPosition` every frame;
/// on XWayland the warp eats the motion events, so every knob froze with its
/// cursor hidden and no edit ever landed.)
pub(crate) fn captured_drag(ui: &Ui, r: &Response) -> egui::Vec2 {
    if r.dragged() {
        ui.input(|i| i.pointer.delta())
    } else {
        egui::Vec2::ZERO
    }
}

/// True on the first change of a text field in one focus run: the moment to
/// make the edit's single undo step. Pair with `end_edit` every frame.
pub fn first_edit(ui: &Ui, r: &Response) -> bool {
    let id = r.id.with("first-edit");
    let seen = !r.gained_focus() && ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    ui.data_mut(|d| d.insert_temp(id, true));
    !seen
}

/// A field gained or lost focus: its next change starts a new undo step.
pub fn end_edit(ui: &Ui, r: &Response) {
    if r.lost_focus() || r.gained_focus() {
        ui.data_mut(|d| d.insert_temp(r.id.with("first-edit"), false));
    }
}

const AUTO_NEXT: &str = "kfxl-auto-next";
const AUTO_REQUEST: &str = "kfxl-auto-request";
const LEARN_REQUEST: &str = "kfxl-midi-learn-request";
const AUTO_TWEAKED: &str = "kfxl-auto-tweaked";

/// The next knob drawn drives `target`: its right-click menu offers
/// "Create automation clip" (FL).
pub fn automatable(ui: &Ui, target: keyframexl::automation::Target) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(AUTO_NEXT), target));
}

/// Record that `target` was tweaked by the user during this frame.
pub fn note_tweaked(ctx: &egui::Context, target: keyframexl::automation::Target) {
    let id = egui::Id::new(AUTO_TWEAKED);
    ctx.data_mut(|d| {
        let mut list = d
            .get_temp::<Vec<keyframexl::automation::Target>>(id)
            .unwrap_or_default();
        list.push(target);
        d.insert_temp(id, list);
    });
}

/// Drain every automatable target tweaked during this frame.
pub fn take_tweaked(ctx: &egui::Context) -> Vec<keyframexl::automation::Target> {
    let id = egui::Id::new(AUTO_TWEAKED);
    ctx.data_mut(|d| {
        let v = d
            .get_temp::<Vec<keyframexl::automation::Target>>(id)
            .unwrap_or_default();
        d.remove::<Vec<keyframexl::automation::Target>>(id);
        v
    })
}

/// "Create automation clip" plus "Link to controller…" for any control's own
/// context menu.
pub fn auto_menu_item(ui: &mut Ui, target: keyframexl::automation::Target) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    menu_style(ui);
    if ui
        .button(tr.kit_create_auto)
        .on_hover_text(tr.kit_create_auto_tip)
        .clicked()
    {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(AUTO_REQUEST), target));
        ui.close_menu();
    }
    if ui
        .button(tr.kit_link_ctrl)
        .on_hover_text(tr.kit_link_ctrl_tip)
        .clicked()
    {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(LEARN_REQUEST), target));
        ui.close_menu();
    }
}

/// The controller-learn target asked for this frame, if any.
pub fn take_learn_request(ctx: &egui::Context) -> Option<keyframexl::automation::Target> {
    take_temp(ctx, egui::Id::new(LEARN_REQUEST))
}

/// The automation clip asked for this frame, if any (the app creates it).
pub fn take_auto_request(ctx: &egui::Context) -> Option<keyframexl::automation::Target> {
    take_temp(ctx, egui::Id::new(AUTO_REQUEST))
}

fn take_temp(ctx: &egui::Context, id: egui::Id) -> Option<keyframexl::automation::Target> {
    ctx.data_mut(|d| {
        let v = d.get_temp::<keyframexl::automation::Target>(id);
        d.remove::<keyframexl::automation::Target>(id);
        v
    })
}

/// Shared knob behaviour: drag, wheel, arrows, reset, right-click menu, typed
/// entry. Returns (gesture_started, changed, typing).
fn knob_behaviour(
    ui: &mut Ui,
    r: &mut Response,
    k: &Knob,
    value: &mut f64,
    type_at: TypeAt,
) -> (bool, bool, bool) {
    let auto = take_temp(ui.ctx(), egui::Id::new(AUTO_NEXT));
    let mem_id = r.id.with("knob-mem");
    let mut mem: KnobMemory = ui.data(|d| d.get_temp(mem_id)).unwrap_or_default();
    let before = *value;
    let mut pos = k.to_pos(*value);
    let mut started = false;
    let (fine, ctrl, now) = ui.input(|i| (i.modifiers.shift, i.modifiers.command, i.time));
    let scale = if fine { 0.1 } else { 1.0 };
    let mut set: Option<f64> = None;

    if r.clicked() || r.drag_started() {
        r.request_focus();
    }
    if r.drag_started() {
        started = true;
    }
    pos -= captured_drag(ui, r).y as f64 / DRAG_TRAVEL * scale;
    if r.hovered() {
        // Take the wheel from any scroll area around us: the knob turns, the page stays.
        let wheel = ui.input_mut(|i| {
            let d = i.smooth_scroll_delta.y + i.smooth_scroll_delta.x;
            i.smooth_scroll_delta = egui::Vec2::ZERO;
            i.raw_scroll_delta = egui::Vec2::ZERO;
            d
        });
        if wheel != 0.0 {
            started = mem.wheel_at.is_none_or(|at| now - at > WHEEL_GESTURE);
            mem.wheel_at = Some(now);
            pos += wheel as f64 / NOTCH / NOTCHES * scale;
        }
    }
    if r.has_focus() && mem.typing.is_none() {
        let (up, down) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
            )
        });
        if up || down {
            started = true;
            pos += if up { 1.0 } else { -1.0 } / NOTCHES * scale;
        }
    }
    if r.double_clicked() || (ctrl && r.clicked()) {
        set = Some(k.default);
    }
    let shown_now = (k.fmt)(*value);
    let tr = crate::i18n::ctx_tr(ui.ctx());
    r.context_menu(|ui| {
        menu_style(ui);
        let default_shown = (k.fmt)(k.default);
        if ui
            .button(tr.kit_reset)
            .on_hover_text(crate::i18n::fill(
                tr.kit_reset_back_to,
                &[("value", &default_shown)],
            ))
            .clicked()
        {
            set = Some(k.default);
            ui.close_menu();
        }
        if ui.button(tr.kit_type_value).clicked() {
            mem.typing = Some(shown_now.clone());
            mem.bad = false;
            ui.close_menu();
        }
        ui.separator();
        if ui.button(tr.kit_copy_value).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(clip_id(), *value));
            ui.ctx().copy_text(shown_now.clone());
            ui.close_menu();
        }
        let copied: Option<f64> = ui.ctx().data(|d| d.get_temp(clip_id()));
        let paste = ui.add_enabled(copied.is_some(), egui::Button::new(tr.kit_paste_value));
        if let Some(v) = copied
            && paste.clicked()
        {
            set = Some(v);
            ui.close_menu();
        }
        ui.separator();
        match auto {
            Some(target) => auto_menu_item(ui, target),
            None => {
                ui.add_enabled(false, egui::Button::new(tr.kit_create_auto))
                    .on_disabled_hover_text(tr.kit_auto_unavailable);
                ui.add_enabled(false, egui::Button::new(tr.kit_link_ctrl))
                    .on_disabled_hover_text(tr.kit_link_unavailable);
            }
        }
    });

    if let Some(v) = set {
        started = true;
        *value = v.clamp(k.min, k.max);
    } else {
        *value = k.at_pos(pos);
        // Untouched travel keeps the exact value (log mapping round-trips are not exact).
        if pos == k.to_pos(before) {
            *value = before;
        }
    }
    let mut changed = (*value - before).abs() > f64::EPSILON;

    let typing = mem.typing.is_some();
    if let Some(text) = mem.typing.as_mut() {
        // Typed entry: Enter applies, Esc cancels, clicking away cancels.
        let edit_id = r.id.with("type");
        let edit = |ui: &mut Ui, text: &mut String, width: f32| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(edit_id)
                    .font(t::mono(13.0))
                    .desired_width(width)
                    .margin(vec2(2.0, 0.0)),
            )
        };
        let te = match type_at {
            TypeAt::InPlace(rect) => {
                let te = ui.put(
                    rect,
                    egui::TextEdit::singleline(text)
                        .id(edit_id)
                        .font(t::mono(13.0))
                        .desired_width(rect.width())
                        .margin(vec2(2.0, 0.0)),
                );
                if mem.bad {
                    ui.painter().rect_stroke(
                        rect.expand(1.0),
                        3.0,
                        Stroke::new(1.5_f32, t::bad()),
                        egui::StrokeKind::Outside,
                    );
                }
                te
            }
            TypeAt::Below => {
                let at = r.rect.center_bottom() + vec2(-40.0, 4.0);
                egui::Area::new(r.id.with("type-area"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(at)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style())
                            .stroke(if mem.bad {
                                Stroke::new(1.5_f32, t::bad())
                            } else {
                                ui.style().visuals.window_stroke
                            })
                            .show(ui, |ui| edit(ui, text, 72.0))
                            .inner
                    })
                    .inner
            }
        };
        if !te.has_focus() && !te.lost_focus() {
            te.request_focus();
        }
        let (enter, esc) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if esc {
            mem.typing = None;
        } else if enter {
            match parse_value(text, &shown_now) {
                Some(v) => {
                    let v = typed_to_value(k, v).clamp(k.min, k.max);
                    if (v - *value).abs() > f64::EPSILON {
                        *value = v;
                        started = true;
                        changed = true;
                    }
                    mem.typing = None;
                }
                None => mem.bad = true,
            }
        } else if te.lost_focus() {
            mem.typing = None;
        }
    }
    if changed {
        r.mark_changed();
        if let Some(target) = auto {
            note_tweaked(ui.ctx(), target);
        }
    }
    ui.data_mut(|d| d.insert_temp(mem_id, mem));
    (started, changed, typing)
}

/// Value arc (270°, from bottom-left clockwise), cap and pointer.
fn paint_encoder(ui: &Ui, r: &Response, c: Pos2, ring: f32, frac: f32, color: C, track_w: f32) {
    let p = ui.painter();
    let hover = hover_amount(ui, r);
    let a0 = 0.75 * PI;
    let arc = |from: f32, to: f32, rad: f32| -> Vec<Pos2> {
        let n = 32;
        (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                c + vec2(a.cos(), a.sin()) * rad
            })
            .collect()
    };
    p.add(Shape::line(
        arc(a0, a0 + 1.5 * PI, ring),
        Stroke::new(track_w, t::sunken()),
    ));
    if frac > 0.001 {
        p.add(Shape::line(
            arc(a0, a0 + 1.5 * PI * frac, ring),
            Stroke::new(track_w, color),
        ));
    }
    let cap = ring - track_w - 2.0;
    p.circle_filled(c + vec2(0.0, 1.0), cap + 1.5, t::edge());
    p.circle_filled(c, cap, mix(t::ctl_top(), t::ctl_hover(), hover));
    let a = a0 + 1.5 * PI * frac;
    let dir = vec2(a.cos(), a.sin());
    p.line_segment(
        [c + dir * (cap * 0.25), c + dir * (cap - 1.0)],
        Stroke::new(if ring > 10.0 { 2.0_f32 } else { 1.5_f32 }, color),
    );
    if show_focus(ui, r) {
        p.circle_stroke(c, ring + 3.0, Stroke::new(1.5_f32, t::focus()));
    }
}

/// OP-1 style encoder: dark raised cap, value arc and pointer in its address
/// colour, label and value printed next to it.
/// Drag up/down (Shift fine), wheel over it (Shift fine), arrows when focused,
/// double-click or Ctrl+click resets, right-click: reset, type, copy, paste.
/// Returns (response, gesture_started, changed); a gesture is one undo step.
pub fn knob(ui: &mut Ui, k: &Knob, value: &mut f64) -> (Response, bool, bool) {
    let d = 30.0;
    let label_w = 64.0;
    let (rect, mut r) =
        ui.allocate_exact_size(vec2(d + 8.0 + label_w, 40.0), Sense::click_and_drag());
    let tx = rect.left() + d + 10.0;
    let value_rect = Rect::from_min_max(
        pos2(tx - 3.0, rect.center().y),
        pos2(rect.right(), rect.bottom()),
    );
    let (started, changed, typing) =
        knob_behaviour(ui, &mut r, k, value, TypeAt::InPlace(value_rect));
    let shown = (k.fmt)(*value);
    let tr = crate::i18n::ctx_tr(ui.ctx());
    r.widget_info(|| egui::WidgetInfo::slider(true, *value, format!("{} {}", k.label, shown)));
    let r = tip(
        r,
        format!(
            "{} {}\n{}\n{}",
            k.label, shown, k.hint, tr.kit_knob_tip_footer
        ),
    );
    paint_encoder(
        ui,
        &r,
        pos2(rect.left() + d / 2.0 + 1.0, rect.center().y),
        d / 2.0,
        k.to_pos(*value) as f32,
        k.color,
        3.0,
    );
    let p = ui.painter();
    caps(
        p,
        pos2(tx, rect.center().y - 2.0),
        Align2::LEFT_BOTTOM,
        k.label,
        11.0,
        t::muted(),
    );
    if !typing {
        p.text(
            pos2(tx, rect.center().y + 1.0),
            Align2::LEFT_TOP,
            shown,
            t::mono(13.0),
            t::text(),
        );
    }
    (r, started, changed)
}

/// Small encoder for dense rows (FL channel pan / volume): no printed label,
/// the value shows on hover. Same gestures as [`knob`].
pub fn mini_knob(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    k: &Knob,
    value: &mut f64,
) -> (Response, bool, bool) {
    let mut r = ui.interact(rect, id, Sense::click_and_drag());
    let (started, changed, _) = knob_behaviour(ui, &mut r, k, value, TypeAt::Below);
    let shown = (k.fmt)(*value);
    r.widget_info(|| egui::WidgetInfo::slider(true, *value, format!("{} {}", k.label, shown)));
    let r = tip(r, format!("{} {}\n{}", k.label, shown, k.hint));
    paint_encoder(
        ui,
        &r,
        rect.center(),
        rect.width().min(rect.height()) / 2.0,
        k.to_pos(*value) as f32,
        k.color,
        2.0,
    );
    (r, started, changed)
}

// ---------------------------------------------------------------- segment LCD

const SEGS: [u8; 10] = [
    0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110, 0b1101101, 0b1111101, 0b0000111,
    0b1111111, 0b1101111,
];

fn seg_mask(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(SEGS[ch as usize - '0' as usize]),
        '-' => Some(0b1000000),
        ' ' => Some(0),
        _ => None,
    }
}

/// Seven-segment digit width for a given height.
pub fn seg_digit_w(h: f32) -> f32 {
    h * 0.52
}

/// Paint seven-segment text (digits, '-', ' ', '.', ':') with ghost segments, like a real LCD.
/// Returns the painted width.
pub fn seg_text(p: &Painter, origin: Pos2, h: f32, s: &str, on: C, off: C) -> f32 {
    let w = seg_digit_w(h);
    let th = (h * 0.13).max(2.0);
    let gap = th * 0.22;
    let adv = w + h * 0.26;
    let mut x = origin.x;
    let y = origin.y;
    for ch in s.chars() {
        match ch {
            '.' => {
                let r = th * 0.6;
                p.circle_filled(pos2(x - h * 0.13, y + h - r), r, on);
                continue;
            }
            ':' => {
                let r = th * 0.55;
                p.circle_filled(pos2(x + th, y + h * 0.3), r, on);
                p.circle_filled(pos2(x + th, y + h * 0.72), r, on);
                x += th * 2.0 + h * 0.12;
                continue;
            }
            _ => {}
        }
        let Some(mask) = seg_mask(ch) else {
            x += adv;
            continue;
        };
        let hseg = |x0: f32, x1: f32, yc: f32| {
            vec![
                pos2(x0, yc),
                pos2(x0 + th / 2.0, yc - th / 2.0),
                pos2(x1 - th / 2.0, yc - th / 2.0),
                pos2(x1, yc),
                pos2(x1 - th / 2.0, yc + th / 2.0),
                pos2(x0 + th / 2.0, yc + th / 2.0),
            ]
        };
        let vseg = |xc: f32, y0: f32, y1: f32| {
            vec![
                pos2(xc, y0),
                pos2(xc + th / 2.0, y0 + th / 2.0),
                pos2(xc + th / 2.0, y1 - th / 2.0),
                pos2(xc, y1),
                pos2(xc - th / 2.0, y1 - th / 2.0),
                pos2(xc - th / 2.0, y0 + th / 2.0),
            ]
        };
        let (l, r) = (x + th / 2.0, x + w - th / 2.0);
        let (top, mid, bot) = (y + th / 2.0, y + h / 2.0, y + h - th / 2.0);
        let segs = [
            hseg(l + gap, r - gap, top),
            vseg(r, top + gap, mid - gap),
            vseg(r, mid + gap, bot - gap),
            hseg(l + gap, r - gap, bot),
            vseg(l, mid + gap, bot - gap),
            vseg(l, top + gap, mid - gap),
            hseg(l + gap, r - gap, mid),
        ];
        for (i, pts) in segs.into_iter().enumerate() {
            let lit = mask & (1 << i) != 0;
            p.add(Shape::convex_polygon(
                pts,
                if lit { on } else { off },
                Stroke::NONE,
            ));
        }
        x += adv;
    }
    x - origin.x
}

// ---------------------------------------------------------------- icons (20 grid, 1.5 line)

pub fn icon_play(p: &Painter, c: Pos2, s: f32, col: C) {
    let h = s * 0.5;
    p.add(Shape::convex_polygon(
        vec![
            c + vec2(-h * 0.7, -h),
            c + vec2(h, 0.0),
            c + vec2(-h * 0.7, h),
        ],
        col,
        Stroke::NONE,
    ));
}

pub fn icon_pause(p: &Painter, c: Pos2, s: f32, col: C) {
    let h = s * 0.5;
    for dx in [-h * 0.45, h * 0.45] {
        p.rect_filled(
            Rect::from_center_size(c + vec2(dx, 0.0), vec2(s * 0.24, s)),
            1.0,
            col,
        );
    }
}

pub fn icon_stop(p: &Painter, c: Pos2, s: f32, col: C) {
    p.rect_filled(
        Rect::from_center_size(c, vec2(s * 0.84, s * 0.84)),
        1.5,
        col,
    );
}

pub fn icon_plus(p: &Painter, c: Pos2, s: f32, col: C) {
    let h = s / 2.0;
    p.line_segment(
        [c - vec2(h, 0.0), c + vec2(h, 0.0)],
        Stroke::new(1.5_f32, col),
    );
    p.line_segment(
        [c - vec2(0.0, h), c + vec2(0.0, h)],
        Stroke::new(1.5_f32, col),
    );
}

/// Playlist: three staggered clip bars.
pub fn icon_playlist(p: &Painter, c: Pos2, col: C) {
    let st = Stroke::new(1.5_f32, col);
    for (i, (x0, x1)) in [(-8.0, 1.0), (-3.0, 8.0), (-8.0, 4.0)].iter().enumerate() {
        let y = c.y - 5.0 + i as f32 * 5.0;
        p.line_segment([pos2(c.x + x0, y), pos2(c.x + x1, y)], st);
    }
}

/// Rack: a 4x3 grid of step dimples.
pub fn icon_rack(p: &Painter, c: Pos2, col: C) {
    for row in 0..3 {
        for k in 0..4 {
            let q = c + vec2(-6.0 + k as f32 * 4.0, -4.0 + row as f32 * 4.0);
            if (row + k) % 2 == 0 {
                p.rect_filled(Rect::from_center_size(q, vec2(2.6, 2.6)), 0.5, col);
            } else {
                p.rect_stroke(
                    Rect::from_center_size(q, vec2(2.4, 2.4)),
                    0.5,
                    Stroke::new(1.0_f32, col),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

/// Piano roll: keys at the left, two notes.
pub fn icon_roll(p: &Painter, c: Pos2, col: C) {
    let st = Stroke::new(1.5_f32, col);
    p.line_segment([c + vec2(-8.0, -7.0), c + vec2(-8.0, 7.0)], st);
    p.line_segment(
        [c + vec2(-4.0, -3.0), c + vec2(3.0, -3.0)],
        Stroke::new(3.0_f32, col),
    );
    p.line_segment(
        [c + vec2(0.0, 3.0), c + vec2(8.0, 3.0)],
        Stroke::new(3.0_f32, col),
    );
}

pub fn icon_mixer(p: &Painter, c: Pos2, col: C) {
    for (k, y) in [(-5.0f32, 2.0f32), (0.0, -2.5), (5.0, 1.0)] {
        p.line_segment(
            [c + vec2(k, -6.0), c + vec2(k, 6.0)],
            Stroke::new(1.0_f32, col),
        );
        p.rect_filled(
            Rect::from_center_size(c + vec2(k, y), vec2(4.0, 2.5)),
            0.5,
            col,
        );
    }
}

/// KeyFrameXL Step Matrix K brand mark, scaled to `rect`.
pub fn icon_brand(p: &Painter, rect: Rect) {
    let s = rect.width().min(rect.height());
    let base = Rect::from_center_size(rect.center(), vec2(s, s));
    let u = s / 128.0;
    let box_at = |x: f32, y: f32, w: f32, h: f32| {
        Rect::from_min_size(base.min + vec2(x * u, y * u), vec2(w * u, h * u))
    };

    if s <= 22.0 {
        p.rect_filled(base, s * 0.22, t::bg());
        p.rect_stroke(
            base,
            s * 0.22,
            Stroke::new(1.0_f32, t::line()),
            egui::StrokeKind::Inside,
        );
        for (x, y) in [
            (20.0, 20.0),
            (20.0, 51.0),
            (20.0, 82.0),
            (82.0, 20.0),
            (82.0, 82.0),
        ] {
            p.rect_filled(box_at(x, y, 26.0, 26.0), (5.0 * u).max(0.8), t::text());
        }
        p.rect_filled(
            box_at(51.0, 51.0, 26.0, 26.0),
            (5.0 * u).max(0.8),
            t::brand(),
        );
        return;
    }

    let chassis = box_at(2.0, 2.0, 124.0, 124.0);
    p.rect_filled(chassis, 28.0 * u, t::panel_top());
    p.rect_stroke(
        chassis,
        28.0 * u,
        Stroke::new((2.0 * u).max(1.0), t::line()),
        egui::StrokeKind::Inside,
    );

    let well = box_at(14.0, 14.0, 100.0, 100.0);
    p.rect_filled(well, 18.0 * u, t::sunken());
    p.rect_stroke(
        well,
        18.0 * u,
        Stroke::new((1.5 * u).max(1.0), t::line()),
        egui::StrokeKind::Inside,
    );

    for (x, y) in [(52.0, 24.0), (80.0, 52.0), (52.0, 80.0)] {
        let r = box_at(x, y, 24.0, 24.0);
        p.rect_filled(r, 6.0 * u, t::surface());
        p.rect_stroke(
            r,
            6.0 * u,
            Stroke::new(1.0_f32, t::line()),
            egui::StrokeKind::Inside,
        );
    }

    for (x, y) in [
        (24.0, 24.0),
        (24.0, 52.0),
        (24.0, 80.0),
        (80.0, 24.0),
        (80.0, 80.0),
    ] {
        p.rect_filled(box_at(x, y, 24.0, 24.0), 6.0 * u, t::text());
    }

    p.rect_filled(box_at(52.0, 52.0, 24.0, 24.0), 6.0 * u, t::brand());
    p.rect_filled(box_at(57.0, 70.0, 14.0, 2.6), 1.3 * u, t::on_brand());
}

/// Vertical fader (FL mixer). `k` is the travel position; the bottom is
/// silence. Same gestures as [`knob`]: drag (Shift fine), wheel, double-click
/// or Ctrl+click to the default (unity), right-click to type or copy.
pub fn fader(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    k: &Knob,
    value: &mut f64,
) -> (Response, bool, bool) {
    let mut r = ui.interact(rect, id, Sense::click_and_drag());
    let (started, changed, _) = knob_behaviour(ui, &mut r, k, value, TypeAt::Below);
    let shown = (k.fmt)(*value);
    r.widget_info(|| egui::WidgetInfo::slider(true, *value, format!("{} {}", k.label, shown)));
    let r = tip(r, format!("{} {}\n{}", k.label, shown, k.hint));
    let p = ui.painter();
    let track = Rect::from_center_size(rect.center(), vec2(4.0, rect.height() - 16.0));
    p.rect_filled(track, 2.0, t::sunken());
    let pos = k.to_pos(*value) as f32;
    // Unity mark (the default: 0 dB).
    let unity = track.bottom() - k.to_pos(k.default) as f32 * track.height();
    p.line_segment(
        [
            pos2(rect.left() + 2.0, unity),
            pos2(rect.left() + 8.0, unity),
        ],
        Stroke::new(1.0_f32, t::muted()),
    );
    let y = track.bottom() - pos * track.height();
    let fill = Rect::from_min_max(pos2(track.left(), y), track.right_bottom());
    p.rect_filled(fill, 2.0, k.color.gamma_multiply(0.6));
    let hover = hover_amount(ui, &r);
    let cap = Rect::from_center_size(pos2(rect.center().x, y), vec2(rect.width().min(26.0), 12.0));
    raised(
        p,
        cap,
        3.0,
        mix(t::ctl_top(), t::ctl_hover(), hover),
        t::ctl_bottom(),
        t::edge(),
        r.is_pointer_button_down_on(),
        1.0,
    );
    p.line_segment(
        [
            pos2(cap.left() + 4.0, cap.center().y),
            pos2(cap.right() - 4.0, cap.center().y),
        ],
        Stroke::new(1.5_f32, k.color),
    );
    if show_focus(ui, &r) {
        focus_ring(p, rect, 4.0);
    }
    (r, started, changed)
}

#[cfg(test)]
mod typed_tests {
    use super::*;

    #[test]
    fn typing_db_into_a_volume_fader_lands_on_that_level() {
        use keyframexl::sequence::{gain_db_text, volume_gain, volume_pos};
        let fmt = |pos: f64| gain_db_text(volume_gain(pos as f32));
        let k = Knob {
            label: "Volume",
            color: t::enc_orange(),
            min: 0.0,
            max: 1.0,
            default: volume_pos(1.0) as f64,
            log: false,
            fmt: &fmt,
            hint: "",
        };
        for want in [-6.0, 0.0, -24.0, 3.0] {
            let pos = typed_to_value(&k, parse_value(&format!("{want}"), &fmt(0.5)).unwrap());
            let got = 20.0 * (volume_gain(pos as f32) as f64).log10();
            assert!((got - want).abs() < 0.06, "typed {want} dB, got {got}");
        }
        assert_eq!(typed_to_value(&k, 40.0), 1.0, "past the top clamps");
        assert_eq!(
            typed_to_value(&k, -200.0),
            0.0,
            "below the bottom is silence"
        );
        // A knob showing its own value keeps typing exact.
        let pct = |v: f64| format!("{:.0}%", v * 100.0);
        let k = Knob {
            label: "Mix",
            color: t::enc_orange(),
            min: 0.0,
            max: 1.0,
            default: 1.0,
            log: false,
            fmt: &pct,
            hint: "",
        };
        assert_eq!(typed_to_value(&k, 0.42), 0.42);
    }
}

/// Middle-button drag that started inside `area` (FL: drag with the wheel
/// button to scroll): this frame's pointer movement while it lasts.
pub fn middle_pan(ui: &Ui, area: Rect, id: egui::Id) -> Option<Vec2> {
    let (pressed, down, pos, delta) = ui.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Middle),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.interact_pos(),
            i.pointer.delta(),
        )
    });
    let mut on: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    if pressed && pos.is_some_and(|p| area.contains(p)) && ui.rect_contains_pointer(area) {
        on = true;
    }
    if !down {
        on = false;
    }
    ui.data_mut(|d| d.insert_temp(id, on));
    if !on {
        return None;
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    (delta != Vec2::ZERO).then_some(delta)
}
