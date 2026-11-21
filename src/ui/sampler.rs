//! DWP Sampler editor: a warm charcoal & brass multi-zone studio console.
//! Features an interactive Key × Velocity map fused to a playable piano strip,
//! a zoomable audio waveform scope with draggable Start/End/Loop splice flags
//! and zero-crossing snap, a live dual-filter response graph, LFO scopes, and
//! a compact visual modulation matrix.

use super::{App, Tone, kit};
use crate::i18n::fill;
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use keyframexl::sampler::{
    Adsr, Filter, FilterKind, FilterRouting, GlideMode, Lfo, LfoShape, LoopMode, MOD_SLOTS,
    ModSource, ModTarget, PlayMode, SamplePreset, Zone,
};
use keyframexl::sequence::InstrumentKind;
use std::sync::Arc;

const TWO_COLUMNS: f32 = 740.0;

/// Per-window editor state: page and zone selection (None = all zones).
#[derive(Clone, Copy)]
pub(crate) struct SamplerUi {
    pub page: usize,
    pub zone: Option<usize>,
}

impl Default for SamplerUi {
    fn default() -> Self {
        Self {
            // Land on Zone so the waveform & sample controls are immediately visible.
            page: 1,
            zone: Some(0),
        }
    }
}

/// Transient waveform viewport state (zoom & scroll), stored in egui temp data.
#[derive(Clone, Copy)]
struct WaveViewState {
    /// Visible fraction start (0..1) and end (0..1).
    start: f64,
    end: f64,
}

impl Default for WaveViewState {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 1.0,
        }
    }
}

impl WaveViewState {
    fn span(self) -> f64 {
        (self.end - self.start).max(0.002)
    }
    fn zoom_at(&mut self, anchor: f64, factor: f64) {
        let cur = self.span();
        let next = (cur * factor).clamp(0.002, 1.0);
        let focus = self.start + anchor.clamp(0.0, 1.0) * cur;
        let mut s = focus - anchor.clamp(0.0, 1.0) * next;
        let mut e = s + next;
        if s < 0.0 {
            e -= s;
            s = 0.0;
        }
        if e > 1.0 {
            s = (s - (e - 1.0)).max(0.0);
            e = 1.0;
        }
        self.start = s;
        self.end = e;
    }
    fn pan(&mut self, delta_frac: f64) {
        let span = self.span();
        let s = (self.start + delta_frac).clamp(0.0, 1.0 - span);
        self.start = s;
        self.end = s + span;
    }
}

/// DirectWave Loop Type, mapped onto loop mode + one-shot.
fn loop_type(z: &Zone) -> usize {
    if z.params.one_shot {
        return 2;
    }
    match z.loop_mode {
        LoopMode::None => 0,
        LoopMode::Continuous => 1,
        LoopMode::Sustain => 3,
        LoopMode::PingPong => 4,
    }
}
fn set_loop_type(z: &mut Zone, i: usize) {
    z.params.one_shot = i == 2;
    z.loop_mode = match i {
        1 => LoopMode::Continuous,
        3 => LoopMode::Sustain,
        4 => LoopMode::PingPong,
        _ => LoopMode::None,
    };
    if z.loop_mode != LoopMode::None && z.loop_end <= z.loop_start + 1 {
        z.loop_start = z.offset;
        z.loop_end = z.end;
    }
}

fn db(v: f64) -> String {
    if v <= -59.9 {
        "-inf dB".into()
    } else {
        format!("{v:+.1} dB")
    }
}
fn secs(v: f64) -> String {
    if v <= 0.0005 {
        "0".into()
    } else if v < 1.0 {
        format!("{:.0} ms", v * 1000.0)
    } else {
        format!("{v:.2} s")
    }
}
fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}
fn signed(v: f64) -> String {
    if v.abs() < 0.005 {
        "0%".into()
    } else {
        format!("{:+.0}%", v * 100.0)
    }
}
fn pan(v: f64) -> String {
    match v {
        v if v.abs() < 0.005 => "C".into(),
        v if v < 0.0 => format!("L{:.0}", -v * 50.0),
        v => format!("R{:.0}", v * 50.0),
    }
}
fn hz(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.2} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

/// Find the nearest upward zero-crossing around `idx` within `min..=max` to prevent clicks.
pub(crate) fn snap_zero_crossing(frames: &[[f32; 2]], idx: usize, min: usize, max: usize) -> usize {
    if frames.len() < 2 || min >= max {
        return idx.clamp(min, max);
    }
    let lo = min.min(frames.len() - 1);
    let hi = max.min(frames.len() - 1);
    let target = idx.clamp(lo, hi);
    let radius = 512usize;
    let search_lo = target.saturating_sub(radius).max(lo.max(1));
    let search_hi = (target + radius).min(hi);
    let mono = |i: usize| 0.5 * (frames[i][0] + frames[i][1]);
    let mut best = target;
    let mut best_dist = usize::MAX;
    for i in search_lo..=search_hi {
        let prev = mono(i - 1);
        let cur = mono(i);
        if prev < 0.0 && cur >= 0.0 {
            let cand = if prev.abs() < cur.abs() { i - 1 } else { i };
            let d = cand.abs_diff(target);
            if d < best_dist {
                best_dist = d;
                best = cand;
            }
        }
    }
    best
}

/// Compute downsampled peak columns `(min, max)` for `frames[f0..f1]`.
fn compute_peaks(frames: &[[f32; 2]], f0: usize, f1: usize, buckets: usize) -> Vec<(f32, f32)> {
    let n = frames.len();
    if n == 0 || buckets == 0 || f0 >= f1 || f0 >= n {
        return vec![(0.0, 0.0); buckets];
    }
    let f1 = f1.min(n);
    let span = f1 - f0;
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        let a = f0 + (b * span) / buckets;
        let c = (f0 + ((b + 1) * span) / buckets).max(a + 1).min(f1);
        let step = ((c - a) / 64).max(1);
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        let mut i = a;
        while i < c {
            let s = 0.5 * (frames[i][0] + frames[i][1]);
            lo = lo.min(s);
            hi = hi.max(s);
            i += step;
        }
        out.push((lo.clamp(-1.0, 1.0), hi.clamp(-1.0, 1.0)));
    }
    out
}

/// Evaluate SVF/biquad magnitude gain for DirectWave filter types.
fn filter_gain(kind: FilterKind, freq: f64, cutoff: f64, emphasis: f64) -> f64 {
    if kind == FilterKind::Off {
        return 1.0;
    }
    let q = 0.5 + emphasis.clamp(0.0, 1.0) * 9.5;
    let k = freq / cutoff.clamp(20.0, 20000.0);
    let k2 = k * k;
    let denom = ((1.0 - k2).powi(2) + (k / q).powi(2)).sqrt().max(0.001);
    match kind {
        FilterKind::Off => 1.0,
        FilterKind::Lowpass => 1.0 / denom,
        FilterKind::Highpass => k2 / denom,
        FilterKind::Bandpass => (k / q) / denom,
        FilterKind::Notch => (1.0 - k2).abs() / denom,
        FilterKind::Allpass => 1.0,
    }
}

/// A knob spec with a log mapping for frequencies and times.
struct P<'a> {
    label: &'a str,
    min: f64,
    max: f64,
    default: f64,
    fmt: &'a dyn Fn(f64) -> String,
    hint: &'a str,
    log: bool,
}

// ---------------------------------------------------------------- Sampler visual primitives

fn smp_well(p: &egui::Painter, rect: Rect, radius: f32) -> Rect {
    p.rect_filled(rect, radius, t::edge());
    let well = rect.shrink(1.0);
    let inner = (radius - 1.0).max(1.0);
    p.rect_filled(well, inner, t::SMP_WELL);
    p.rect_stroke(
        well,
        inner,
        Stroke::new(1.0_f32, t::SMP_LINE.gamma_multiply(0.8)),
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

fn smp_bay(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t::SMP_DECK)
        .stroke(Stroke::new(1.0_f32, t::SMP_LINE))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            body(ui);
        });
}

fn smp_section_head(ui: &mut Ui, title: &str, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_size(pos2(r.left(), r.center().y - 3.5), vec2(7.0, 7.0)),
        1.5,
        color,
    );
    kit::caps(
        p,
        pos2(r.left() + 13.0, r.center().y),
        Align2::LEFT_CENTER,
        title,
        11.0,
        t::SMP_SAND,
    );
}

/// Brass-accented segmented tabs for DWP Sampler.
fn smp_pills(
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
    p.rect_filled(track, 6.0, t::SMP_WELL);
    p.rect_stroke(
        track,
        6.0,
        Stroke::new(1.0_f32, t::SMP_LINE),
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
            p.rect_filled(r, 4.0, t::SMP_DECK_HI);
            p.line_segment(
                [
                    pos2(r.left() + 4.0, r.bottom() - 1.0),
                    pos2(r.right() - 4.0, r.bottom() - 1.0),
                ],
                Stroke::new(1.8_f32, accent),
            );
        } else if resp.hovered() {
            p.rect_filled(r, 4.0, t::SMP_DECK.gamma_multiply(0.8));
        }
        p.text(
            r.center() - vec2(0.0, if on { 0.5 } else { 0.0 }),
            Align2::CENTER_CENTER,
            *label,
            if on { t::medium(12.5) } else { font.clone() },
            if on || resp.hovered() {
                t::text()
            } else {
                t::SMP_SAND
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

/// Paint the bespoke DWP Sampler twin tape-reel / zone-grid studio mark.
pub(super) fn paint_sampler_mark(p: &egui::Painter, rect: Rect) {
    let c = rect.center();
    let r = (rect.height() * 0.34).max(5.0);
    let lx = c.x - rect.width() * 0.24;
    let rx = c.x + rect.width() * 0.24;
    p.line_segment(
        [pos2(lx, c.y + r * 0.8), pos2(rx, c.y + r * 0.8)],
        Stroke::new(1.4_f32, t::SMP_LOOP),
    );
    for cx in [lx, rx] {
        p.circle_stroke(pos2(cx, c.y), r, Stroke::new(1.6_f32, t::SMP_GOLD));
        p.circle_filled(pos2(cx, c.y), r * 0.35, t::SMP_GOLD);
    }
}

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

fn smp_bipolar_bar(
    ui: &mut Ui,
    width: f32,
    color: Color32,
    value: &mut f64,
    enabled: bool,
    hint: &str,
) -> (bool, bool) {
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), sense);
    let mut started = false;
    let mut changed = false;
    if enabled {
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
    }
    let p = ui.painter();
    let well = smp_well(p, rect, 4.0);
    let mid = well.center().x;
    p.line_segment(
        [pos2(mid, well.top() + 2.0), pos2(mid, well.bottom() - 2.0)],
        Stroke::new(1.0_f32, t::SMP_LINE),
    );
    if enabled {
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
    }
    p.text(
        well.center(),
        Align2::CENTER_CENTER,
        if enabled { signed(*value) } else { "--".into() },
        t::mono(12.0),
        if enabled { t::text() } else { t::faint() },
    );
    let tr = crate::i18n::ctx_tr(ui.ctx());
    kit::tip(resp, fill(tr.smp_bipolar_tip, &[("hint", hint)]));
    (started, changed)
}

fn mod_source_color(s: ModSource) -> Color32 {
    match s {
        ModSource::None => t::strong(),
        ModSource::Lfo1 | ModSource::Lfo2 | ModSource::ProgLfo1 | ModSource::ProgLfo2 => {
            t::SMP_START
        }
        ModSource::Env1 | ModSource::Env2 => t::SMP_GOLD,
        ModSource::Velocity | ModSource::Key => t::SMP_FILTER2,
        ModSource::Random => t::SMP_LOOP,
    }
}

fn is_black_key(k: u8) -> bool {
    matches!(k % 12, 1 | 3 | 6 | 8 | 10)
}

// ---------------------------------------------------------------- Main Sampler Editor

impl App {
    pub(super) fn sampler_ui(&mut self, id: u64) -> &mut SamplerUi {
        self.sampler_view.entry(id).or_default()
    }

    /// Change the preset of sampler `id`. One undo step per gesture.
    fn edit_preset(&mut self, id: u64, started: bool, f: impl FnOnce(&mut SamplePreset)) {
        if started {
            self.checkpoint();
        }
        if let Some(InstrumentKind::Sampler { preset: Some(arc) }) = self
            .song
            .instruments
            .iter_mut()
            .find(|i| i.id == id)
            .map(|i| &mut i.kind)
        {
            let p = Arc::make_mut(arc);
            f(p);
            if p.validate_params().is_err() {
                let msg = self.tr().smp_val_out_of_range.to_string();
                self.say(Tone::Bad, msg);
            }
        }
        self.publish();
    }

    /// Apply `f` to the selected zone or every zone.
    pub(super) fn edit_zones(&mut self, id: u64, started: bool, f: impl Fn(&mut Zone)) {
        let sel = self.sampler_ui(id).zone;
        self.edit_preset(id, started, |p| match sel {
            Some(i) => {
                if let Some(z) = p.zones.get_mut(i) {
                    f(z)
                }
            }
            None => p.zones.iter_mut().for_each(&f),
        });
    }

    /// Edit a specific zone index directly (used by map/waveform handles).
    fn edit_zone_at(&mut self, id: u64, zone_idx: usize, started: bool, f: impl FnOnce(&mut Zone)) {
        self.edit_preset(id, started, |p| {
            if let Some(z) = p.zones.get_mut(zone_idx) {
                f(z);
            }
        });
    }

    /// Knob bound to a value. A gesture start is one undo step.
    fn pknob(&mut self, ui: &mut Ui, spec: &P, color: Color32, value: f64) -> Option<f64> {
        let mut v = value;
        let (_, started, changed) = kit::knob(
            ui,
            &kit::Knob {
                label: spec.label,
                color,
                min: spec.min,
                max: spec.max,
                default: spec.default,
                log: spec.log,
                fmt: spec.fmt,
                hint: spec.hint,
            },
            &mut v,
        );
        if started {
            self.checkpoint();
        }
        changed.then_some(v.clamp(spec.min, spec.max))
    }

    pub(super) fn sampler_editor(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset) {
        let tr = self.tr();
        let lang = tr.lang;
        if preset.zones.is_empty() {
            ui.label(egui::RichText::new(tr.smp_no_zones).color(t::muted()));
            return;
        }
        let mut st = *self.sampler_ui(id);
        if let Some(zi) = st.zone
            && zi >= preset.zones.len()
        {
            st.zone = Some(preset.zones.len() - 1);
            self.sampler_ui(id).zone = st.zone;
        }
        let zi = st.zone.unwrap_or(0).min(preset.zones.len() - 1);
        let z = preset.zones[zi].clone();
        let pages = tr.smp_pages();

        egui::Frame::new()
            .fill(t::SMP_CHASSIS)
            .stroke(Stroke::new(1.0_f32, t::SMP_LINE))
            .corner_radius(9.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let _dark = t::Faceplate::enter(ui);
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

                // 1. Studio Console Header: DWP mark, active zone telemetry, Audition, Scope toggle.
                self.sampler_header(ui, id, preset, &z, zi, st);

                // 2. Hero Key × Velocity Map fused to playable Piano Keyboard Strip.
                self.zone_map(ui, id, preset, st.zone);

                // 3. Page Navigation Tabs.
                ui.horizontal(|ui| {
                    if let Some(p) = smp_pills(
                        ui,
                        &format!("smp-page-{id}"),
                        &pages,
                        st.page.min(pages.len() - 1),
                        t::SMP_GOLD,
                    ) {
                        self.sampler_ui(id).page = p;
                        st.page = p;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let scope_text = match st.zone {
                            Some(i) => {
                                let n = format!("{:02}", i + 1);
                                let lo = super::roll::note_name(z.low);
                                let hi = super::roll::note_name(z.high);
                                fill(tr.smp_editing_zone, &[("n", &n), ("lo", &lo), ("hi", &hi)])
                            }
                            None => tr.smp_global_edit_zones.fmt(lang, preset.zones.len()),
                        };
                        ui.label(egui::RichText::new(scope_text).font(t::mono(12.0)).color(
                            if st.zone.is_some() {
                                t::SMP_GOLD
                            } else {
                                t::SMP_LOOP
                            },
                        ));
                    });
                });

                // 4. Scrollable Active Page Body (header, zone map, and keyboard remain pinned).
                egui::ScrollArea::vertical()
                    .id_salt(("smp-page-scroll", id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        match st.page {
                            0 => self.page_program(ui, id, preset),
                            1 => self.page_zone(ui, id, zi, &z),
                            2 => self.page_filter(ui, id, &z),
                            3 => self.page_modulation(ui, id, &z),
                            _ => self.page_zones(ui, id, preset, st.zone),
                        }
                    });
            });
    }

    fn sampler_header(
        &mut self,
        ui: &mut Ui,
        id: u64,
        preset: &SamplePreset,
        z: &Zone,
        zi: usize,
        st: SamplerUi,
    ) {
        let tr = self.tr();
        egui::Frame::new()
            .fill(t::SMP_DECK)
            .stroke(Stroke::new(1.0_f32, t::SMP_LINE))
            .corner_radius(7.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 4.0);

                    let (mark, _) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::hover());
                    paint_sampler_mark(ui.painter(), mark);
                    ui.label(
                        egui::RichText::new("DWP")
                            .font(t::semibold(15.0))
                            .color(t::text()),
                    );

                    // Zone & audio telemetry badge.
                    let dur = z.end.saturating_sub(z.offset) as f64 / z.sample_rate.max(1) as f64;
                    let sample_name = super::instrument::file_name(&z.name);
                    let meta = format!(
                        "{} {} · {:02} {} · {} · {:.1} kHz · {}",
                        preset.zones.len(),
                        tr.smp_zn_short,
                        zi + 1,
                        kit::elide(ui.painter(), &sample_name, t::mono(12.0), 170.0),
                        super::roll::note_name(z.root),
                        z.sample_rate as f64 / 1000.0,
                        secs(dur)
                    );
                    ui.label(
                        egui::RichText::new(meta)
                            .font(t::mono(12.0))
                            .color(t::SMP_SAND),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Current Zone vs All Zones scope toggle.
                        let zn_total = preset.zones.len().to_string();
                        let zn_cur = format!("{:02}", zi + 1);
                        let all_label = fill(tr.smp_scope_all, &[("n", &zn_total)]);
                        let cur_label = fill(tr.smp_scope_zone, &[("n", &zn_cur)]);
                        if let Some(pick) = smp_pills(
                            ui,
                            &format!("smp-scope-{id}"),
                            &[cur_label.as_str(), all_label.as_str()],
                            st.zone.is_none() as usize,
                            if st.zone.is_some() {
                                t::SMP_GOLD
                            } else {
                                t::SMP_LOOP
                            },
                        ) {
                            match pick {
                                0 if st.zone.is_none() => self.sampler_ui(id).zone = Some(0),
                                1 => self.sampler_ui(id).zone = None,
                                _ => {}
                            }
                        }

                        // Hold-to-audition button playing the active zone's root note.
                        let aud_id = ui.id().with(("smp-aud-btn", id));
                        let (btn_rect, btn_resp) =
                            ui.allocate_exact_size(vec2(88.0, 24.0), Sense::click_and_drag());
                        let held_now = btn_resp.is_pointer_button_down_on();
                        let was_held: bool = ui.data(|d| d.get_temp(aud_id)).unwrap_or(false);
                        if held_now {
                            self.hold_note(id, z.root, 104);
                        } else if was_held {
                            self.release_note();
                        }
                        ui.data_mut(|d| d.insert_temp(aud_id, held_now));

                        let p = ui.painter();
                        p.rect_filled(
                            btn_rect,
                            5.0,
                            if held_now {
                                t::SMP_GOLD
                            } else if btn_resp.hovered() {
                                t::SMP_DECK_HI
                            } else {
                                t::SMP_WELL
                            },
                        );
                        p.rect_stroke(
                            btn_rect,
                            5.0,
                            Stroke::new(1.0_f32, if held_now { t::SMP_GOLD } else { t::SMP_LINE }),
                            egui::StrokeKind::Inside,
                        );
                        p.text(
                            btn_rect.center(),
                            Align2::CENTER_CENTER,
                            format!("▶ {}", super::roll::note_name(z.root)),
                            t::mono(12.0),
                            if held_now { t::edge() } else { t::SMP_GOLD },
                        );
                        kit::tip(btn_resp, tr.smp_audition_tip);
                    });
                });
            });
    }

    /// Interactive Key × Velocity map fused directly to a playable piano keyboard strip.
    fn zone_map(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset, selected: Option<usize>) {
        let map_h = 68.0;
        let kb_h = 24.0;
        let (area, resp) = ui.allocate_exact_size(
            vec2(ui.available_width(), map_h + kb_h),
            Sense::click_and_drag(),
        );
        let p = ui.painter().clone();
        let map_outer = Rect::from_min_size(area.min, vec2(area.width(), map_h));
        let well = smp_well(&p, map_outer, 6.0).shrink2(vec2(4.0, 4.0));
        let kb_rect = Rect::from_min_max(
            pos2(well.left(), map_outer.bottom() + 1.0),
            pos2(well.right(), area.bottom()),
        );

        let lo = preset
            .zones
            .iter()
            .map(|z| z.low)
            .min()
            .unwrap_or(48)
            .saturating_sub(4)
            / 12
            * 12;
        let hi = ((preset.zones.iter().map(|z| z.high).max().unwrap_or(72) as u16 + 4).min(127)
            as u8
            / 12
            * 12
            + 11)
            .min(127);
        let keys = (hi - lo + 1).max(12) as f32;
        let key_w = well.width() / keys;
        let kx = |k: u8| well.left() + (k.saturating_sub(lo)) as f32 * key_w;
        let vy = |v: u8| well.bottom() - v as f32 / 127.0 * well.height();
        let x_to_key = |px: f32| -> u8 {
            let k = lo as f32 + ((px - well.left()) / key_w).floor();
            k.clamp(0.0, 127.0) as u8
        };
        let y_to_vel = |py: f32| -> u8 {
            let v = (well.bottom() - py) / well.height().max(1.0) * 127.0;
            v.clamp(0.0, 127.0).round() as u8
        };

        // Velocity horizontal rules.
        for v in [32u8, 64, 96] {
            p.line_segment(
                [pos2(well.left(), vy(v)), pos2(well.right(), vy(v))],
                Stroke::new(1.0_f32, t::SMP_LINE.gamma_multiply(0.45)),
            );
        }
        // Octave vertical rules.
        for k in (lo..=hi).filter(|k| k % 12 == 0) {
            p.line_segment(
                [pos2(kx(k), well.top()), pos2(kx(k), well.bottom())],
                Stroke::new(1.0_f32, t::SMP_LINE),
            );
        }

        let held_pitch = self
            .held
            .and_then(|(_, ch, pitch)| (ch == id).then_some(pitch));
        let pointer = ui.ctx().pointer_hover_pos();
        let mut hovered_zone = None;

        // Paint zones.
        let mut zone_rects: Vec<Rect> = Vec::with_capacity(preset.zones.len());
        for (i, z) in preset.zones.iter().enumerate() {
            let code = t::pattern_codes()[i % t::pattern_codes().len()];
            let r = Rect::from_min_max(
                pos2(kx(z.low), vy(z.high_velocity)),
                pos2(kx(z.high) + key_w, vy(z.low_velocity)),
            )
            .shrink(1.0);
            zone_rects.push(r);
            let on = selected.is_none_or(|s| s == i);
            let sounding = held_pitch.is_some_and(|hp| z.matches(hp, 100));
            let fill_a = if sounding {
                0.38
            } else if on {
                0.22
            } else {
                0.07
            };
            p.rect_filled(r, 3.0, code.gamma_multiply(fill_a));
            let border_col = if selected == Some(i) {
                t::SMP_GOLD
            } else if on {
                code
            } else {
                code.gamma_multiply(0.35)
            };
            p.rect_stroke(
                r,
                3.0,
                Stroke::new(if selected == Some(i) { 2.0_f32 } else { 1.2 }, border_col),
                egui::StrokeKind::Inside,
            );
            let tag = if r.width() > 96.0 {
                format!("{:02} {}", i + 1, super::instrument::file_name(&z.name))
            } else {
                format!("{:02}", i + 1)
            };
            let shown = kit::elide(&p, &tag, t::mono(12.0), (r.width() - 10.0).max(16.0));
            p.text(
                r.left_top() + vec2(5.0, 3.0),
                Align2::LEFT_TOP,
                shown,
                t::mono(12.0),
                if on { t::text() } else { t::faint() },
            );
            if pointer.is_some_and(|q| r.contains(q) && map_outer.contains(q)) {
                hovered_zone = Some(i);
            }
        }

        // Paint fused Piano Keyboard strip aligned 1:1 with lo..=hi.
        p.rect_filled(kb_rect, 3.0, t::edge());
        for k in lo..=hi {
            let x0 = kx(k);
            let kr = Rect::from_min_size(
                pos2(x0, kb_rect.top()),
                vec2((key_w - 0.5).max(1.5), kb_rect.height()),
            );
            let black = is_black_key(k);
            let active = held_pitch == Some(k);
            let col = if active {
                t::SMP_GOLD
            } else if black {
                t::key_black()
            } else {
                t::key_white()
            };
            p.rect_filled(kr, 1.5, col);
            if k % 12 == 0 {
                p.text(
                    pos2(kr.left() + 2.0, kb_rect.bottom() - 2.0),
                    Align2::LEFT_BOTTOM,
                    super::roll::note_name(k),
                    t::mono(10.0),
                    if active { t::edge() } else { t::key_ink() },
                );
            }
        }

        // Paint Root Key diamond marker for selected (or first) zone.
        let sel_idx = selected.unwrap_or(0).min(preset.zones.len() - 1);
        let sel_z = &preset.zones[sel_idx];
        let root_cx =
            (kx(sel_z.root) + key_w * 0.5).clamp(kb_rect.left() + 4.0, kb_rect.right() - 4.0);
        let root_cy = kb_rect.top() + 5.0;
        let diamond = vec![
            pos2(root_cx, root_cy - 4.5),
            pos2(root_cx + 4.5, root_cy),
            pos2(root_cx, root_cy + 4.5),
            pos2(root_cx - 4.5, root_cy),
        ];
        p.add(Shape::convex_polygon(
            diamond,
            t::SMP_GOLD,
            Stroke::new(1.0_f32, t::edge()),
        ));

        // Interaction state machine: 0=none, 1=piano audition, 2=drag root, 3..6=resize zone L/R/T/B.
        let mode_id = ui.id().with(("smp-map-mode", id));
        let mut mode: u8 = ui.data(|d| d.get_temp(mode_id)).unwrap_or(0);

        // Edge hover cursor hint on selected zone.
        let edge_at = |pt: Pos2| -> Option<u8> {
            let zi = selected?;
            let r = *zone_rects.get(zi)?;
            if !r.expand(5.0).contains(pt) {
                return None;
            }
            if (pt.x - r.left()).abs() <= 6.0 {
                Some(3)
            } else if (pt.x - r.right()).abs() <= 6.0 {
                Some(4)
            } else if (pt.y - r.top()).abs() <= 6.0 {
                Some(5)
            } else if (pt.y - r.bottom()).abs() <= 6.0 {
                Some(6)
            } else {
                None
            }
        };

        if let Some(pt) = pointer {
            if pt.distance(pos2(root_cx, root_cy)) <= 7.0 {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            } else if let Some(e) = edge_at(pt) {
                ui.ctx().set_cursor_icon(if e <= 4 {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                });
            }
        }

        let shift_or_alt = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
        if resp.drag_started() || resp.is_pointer_button_down_on() {
            if mode == 0
                && let Some(pt) = resp.interact_pointer_pos()
            {
                if pt.distance(pos2(root_cx, root_cy)) <= 7.5
                    || (shift_or_alt.1 && kb_rect.contains(pt))
                {
                    mode = 2;
                    self.checkpoint();
                } else if kb_rect.contains(pt) || (shift_or_alt.0 && map_outer.contains(pt)) {
                    mode = 1;
                } else if let Some(e) = edge_at(pt) {
                    mode = e;
                    self.checkpoint();
                }
            }
            if let Some(pt) = resp.interact_pointer_pos() {
                match mode {
                    1 => {
                        let key = x_to_key(pt.x);
                        let vel = if map_outer.contains(pt) {
                            y_to_vel(pt.y).max(1)
                        } else {
                            let f = ((pt.y - kb_rect.top()) / kb_rect.height().max(1.0))
                                .clamp(0.0, 1.0);
                            (50.0 + f * 77.0).round() as u8
                        };
                        self.hold_note(id, key, vel);
                    }
                    2 => {
                        let new_root = x_to_key(pt.x);
                        if new_root != sel_z.root {
                            self.edit_zone_at(id, sel_idx, false, |z| z.root = new_root);
                        }
                    }
                    3..=6 => {
                        if let Some(zi) = selected {
                            let k = x_to_key(pt.x);
                            let v = y_to_vel(pt.y);
                            self.edit_zone_at(id, zi, false, |z| match mode {
                                3 => z.low = k.min(z.high),
                                4 => z.high = k.max(z.low),
                                5 => z.high_velocity = v.max(z.low_velocity),
                                _ => z.low_velocity = v.min(z.high_velocity),
                            });
                        }
                    }
                    _ => {}
                }
            }
        } else {
            if mode == 1 {
                self.release_note();
            }
            mode = 0;
        }
        ui.data_mut(|d| d.insert_temp(mode_id, mode));

        // Plain click inside map selects zone (or All zones on empty space).
        if resp.clicked()
            && !shift_or_alt.0
            && !shift_or_alt.1
            && let Some(pt) = resp.interact_pointer_pos()
            && map_outer.contains(pt)
        {
            self.sampler_ui(id).zone = hovered_zone;
        }

        // Hover tooltip on map zones.
        if let Some(i) = hovered_zone
            && mode == 0
        {
            let tr = self.tr();
            let z = &preset.zones[i];
            let n_str = format!("{:02}", i + 1);
            let name = super::instrument::file_name(&z.name);
            let lo_s = super::roll::note_name(z.low);
            let hi_s = super::roll::note_name(z.high);
            let root_s = super::roll::note_name(z.root);
            let vlo_s = z.low_velocity.to_string();
            let vhi_s = z.high_velocity.to_string();
            let tip_text = fill(
                tr.smp_zone_map_tip,
                &[
                    ("n", &n_str),
                    ("name", &name),
                    ("lo", &lo_s),
                    ("hi", &hi_s),
                    ("root", &root_s),
                    ("vlo", &vlo_s),
                    ("vhi", &vhi_s),
                ],
            );
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                egui::Id::new(("zone-tip", id)),
                |ui| {
                    ui.label(tip_text);
                },
            );
        }
    }

    // ---------------------------------------------------------------- Page 0: Program

    fn page_program(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset) {
        let tr = self.tr();
        let lang = tr.lang;
        let pr = preset.program.clone();
        let wide = ui.available_width() >= TWO_COLUMNS;
        let draw_output = |this: &mut Self, ui: &mut Ui| {
            smp_bay(ui, |ui| {
                smp_section_head(ui, tr.smp_sec_prog_out, t::SMP_GOLD);
                ui.horizontal_wrapped(|ui| {
                    let c = t::SMP_GOLD;
                    let specs: [(P, f64); 4] = [
                        (
                            P {
                                label: tr.smp_k_volume,
                                min: -60.0,
                                max: 12.0,
                                default: 0.0,
                                fmt: &db,
                                hint: tr.smp_k_volume_hint,
                                log: false,
                            },
                            pr.volume_db,
                        ),
                        (
                            P {
                                label: tr.smp_k_pan,
                                min: -1.0,
                                max: 1.0,
                                default: 0.0,
                                fmt: &pan,
                                hint: tr.smp_k_pan_hint,
                                log: false,
                            },
                            pr.pan,
                        ),
                        (
                            P {
                                label: tr.smp_k_trans,
                                min: -48.0,
                                max: 48.0,
                                default: 0.0,
                                fmt: &|v| format!("{:+.0} st", v.round()),
                                hint: tr.smp_k_trans_hint,
                                log: false,
                            },
                            pr.transpose as f64,
                        ),
                        (
                            P {
                                label: tr.smp_k_pb_rng,
                                min: 0.0,
                                max: 4800.0,
                                default: 200.0,
                                fmt: &|v| format!("{:.0} ct", v),
                                hint: tr.smp_k_pb_rng_hint,
                                log: false,
                            },
                            pr.bend_range,
                        ),
                    ];
                    for (k, (spec, v)) in specs.iter().enumerate() {
                        if let Some(nv) = this.pknob(ui, spec, c, *v) {
                            this.edit_preset(id, false, |p| match k {
                                0 => p.program.volume_db = nv,
                                1 => p.program.pan = nv,
                                2 => p.program.transpose = nv.round() as i8,
                                _ => p.program.bend_range = nv.round(),
                            });
                        }
                    }
                });
            });
        };

        let draw_voice = |this: &mut Self, ui: &mut Ui| {
            smp_bay(ui, |ui| {
                smp_section_head(ui, tr.smp_sec_voice_glide, t::SMP_LOOP);
                ui.horizontal_wrapped(|ui| {
                    let modes: Vec<&str> =
                        PlayMode::ALL.iter().map(|m| tr.smp_play_mode(*m)).collect();
                    let cur = PlayMode::ALL
                        .iter()
                        .position(|m| *m == pr.play_mode)
                        .unwrap_or(0);
                    if let Some(i) = smp_pills(ui, &format!("pm-{id}"), &modes, cur, t::SMP_GOLD) {
                        this.edit_preset(id, true, |p| p.program.play_mode = PlayMode::ALL[i]);
                    }
                    let gm: Vec<&str> = GlideMode::ALL
                        .iter()
                        .map(|m| tr.smp_glide_mode(*m))
                        .collect();
                    let gcur = GlideMode::ALL
                        .iter()
                        .position(|m| *m == pr.glide_mode)
                        .unwrap_or(0);
                    if let Some(i) = smp_pills(ui, &format!("gm-{id}"), &gm, gcur, t::SMP_LOOP) {
                        this.edit_preset(id, true, |p| p.program.glide_mode = GlideMode::ALL[i]);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if let Some(v) = this.pknob(
                        ui,
                        &P {
                            label: tr.smp_k_poly,
                            min: 1.0,
                            max: 64.0,
                            default: 32.0,
                            fmt: &|v| format!("{:.0}", v.round()),
                            hint: tr.smp_k_poly_hint,
                            log: false,
                        },
                        t::SMP_GOLD,
                        pr.polyphony as f64,
                    ) {
                        this.edit_preset(id, false, |p| p.program.polyphony = v.round() as u8);
                    }
                    let off_str = tr.smp_off_lower;
                    let fmt_glide = move |v: f64| {
                        if v <= 0.0 { off_str.into() } else { secs(v) }
                    };
                    if let Some(v) = this.pknob(
                        ui,
                        &P {
                            label: tr.smp_k_glide,
                            min: 0.0,
                            max: 10.0,
                            default: 0.0,
                            fmt: &fmt_glide,
                            hint: tr.smp_k_glide_hint,
                            log: true,
                        },
                        t::SMP_LOOP,
                        pr.glide,
                    ) {
                        this.edit_preset(id, false, |p| {
                            p.program.glide = if v < 0.0011 { 0.0 } else { v }
                        });
                    }
                });
            });
        };

        if wide {
            let avail = ui.available_width();
            let half = ((avail - 8.0) * 0.5).floor();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui_with_layout(
                    vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| draw_output(self, ui),
                );
                ui.allocate_ui_with_layout(
                    vec2(avail - 8.0 - half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| draw_voice(self, ui),
                );
            });
        } else {
            draw_output(self, ui);
            draw_voice(self, ui);
        }

        // Preset Diagnostics & Import Report.
        smp_bay(ui, |ui| {
            smp_section_head(ui, tr.smp_sec_summary, t::SMP_SAND);
            let lo = preset.zones.iter().map(|z| z.low).min().unwrap_or(0);
            let hi = preset.zones.iter().map(|z| z.high).max().unwrap_or(127);
            let total_frames: usize = preset.zones.iter().map(|z| z.frames.len()).sum();
            let looping = preset
                .zones
                .iter()
                .filter(|z| z.loop_mode != LoopMode::None)
                .count();
            let zones_str = tr.smp_zones_count.fmt(lang, preset.zones.len());
            let lo_str = super::roll::note_name(lo);
            let hi_str = super::roll::note_name(hi);
            let looped_str = looping.to_string();
            let mb_str = format!("{:.2}", (total_frames * 8) as f64 / (1024.0 * 1024.0));
            ui.label(
                egui::RichText::new(fill(
                    tr.smp_summary_line,
                    &[
                        ("zones", &zones_str),
                        ("lo", &lo_str),
                        ("hi", &hi_str),
                        ("looped", &looped_str),
                        ("mb", &mb_str),
                    ],
                ))
                .font(t::mono(12.5))
                .color(t::text()),
            );
            if !preset.report.is_empty() {
                egui::Frame::new()
                    .fill(t::SMP_WELL)
                    .stroke(Stroke::new(1.0_f32, t::SMP_LINE))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&preset.report)
                                .font(t::sans(12.5))
                                .color(t::SMP_SAND),
                        );
                    });
            }
        });
    }

    // ---------------------------------------------------------------- Page 1: Zone & Waveform

    fn page_zone(&mut self, ui: &mut Ui, id: u64, zi: usize, z: &Zone) {
        let tr = self.tr();
        // Hero Interactive Sample Waveform Scope.
        self.waveform_scope(ui, id, zi, z);

        // Pitch + Amp side-by-side bays.
        let wide = ui.available_width() >= TWO_COLUMNS;
        let draw_pitch = |this: &mut Self, ui: &mut Ui| {
            smp_bay(ui, |ui| {
                ui.horizontal(|ui| {
                    let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                    ui.painter().rect_filled(sq, 1.5, t::SMP_GOLD);
                    kit::caps(
                        ui.painter(),
                        pos2(sq.right() + 6.0, sq.center().y),
                        Align2::LEFT_CENTER,
                        tr.smp_sec_pitch_root,
                        11.0,
                        t::SMP_SAND,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("▶")
                            .on_hover_text(tr.smp_root_up_tip)
                            .clicked()
                            && z.root < 127
                        {
                            this.edit_zones(id, true, |z| z.root = (z.root + 1).min(127));
                        }
                        let note_str = super::roll::note_name(z.root);
                        ui.label(
                            egui::RichText::new(fill(tr.smp_root_badge, &[("note", &note_str)]))
                                .font(t::mono(12.0))
                                .color(t::SMP_GOLD),
                        );
                        if ui
                            .small_button("◀")
                            .on_hover_text(tr.smp_root_down_tip)
                            .clicked()
                            && z.root > 0
                        {
                            this.edit_zones(id, true, |z| z.root = z.root.saturating_sub(1));
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    let semi = (z.tune_cents / 100.0).round();
                    let fine = z.tune_cents - semi * 100.0;
                    let rows: [(P, f64); 3] = [
                        (
                            P {
                                label: tr.smp_k_semi,
                                min: -36.0,
                                max: 36.0,
                                default: 0.0,
                                fmt: &|v| format!("{:+.0} st", v.round()),
                                hint: tr.smp_k_semi_hint,
                                log: false,
                            },
                            semi,
                        ),
                        (
                            P {
                                label: tr.smp_k_fine,
                                min: -50.0,
                                max: 50.0,
                                default: 0.0,
                                fmt: &|v| format!("{v:+.0} ct"),
                                hint: tr.smp_k_fine_hint,
                                log: false,
                            },
                            fine,
                        ),
                        (
                            P {
                                label: tr.smp_k_ktrk,
                                min: -1.0,
                                max: 1.0,
                                default: 1.0,
                                fmt: &|v| format!("{:+.0}%", v * 100.0),
                                hint: tr.smp_k_ktrk_hint,
                                log: false,
                            },
                            z.params.key_track,
                        ),
                    ];
                    for (k, (spec, v)) in rows.iter().enumerate() {
                        if let Some(nv) = this.pknob(ui, spec, t::SMP_GOLD, *v) {
                            this.edit_zones(id, false, |z| {
                                let semi = (z.tune_cents / 100.0).round();
                                let fine = z.tune_cents - semi * 100.0;
                                match k {
                                    0 => z.tune_cents = nv.round() * 100.0 + fine,
                                    1 => z.tune_cents = semi * 100.0 + nv,
                                    _ => z.params.key_track = nv,
                                }
                            });
                        }
                    }
                });
            });
        };

        let draw_amp = |this: &mut Self, ui: &mut Ui| {
            smp_bay(ui, |ui| {
                smp_section_head(ui, tr.smp_sec_amp_grp, t::SMP_START);
                ui.horizontal_wrapped(|ui| {
                    let gain_db = 20.0 * z.gain.max(1e-6).log10();
                    let off_str = tr.smp_off_lower;
                    let fmt_grp = move |v: f64| {
                        if v < 0.5 {
                            off_str.into()
                        } else {
                            format!("{:.0}", v.round())
                        }
                    };
                    let rows: [(P, f64); 4] = [
                        (
                            P {
                                label: tr.smp_k_gain,
                                min: -60.0,
                                max: 24.0,
                                default: 0.0,
                                fmt: &db,
                                hint: tr.smp_k_gain_hint,
                                log: false,
                            },
                            gain_db.max(-60.0),
                        ),
                        (
                            P {
                                label: tr.smp_k_pan,
                                min: -1.0,
                                max: 1.0,
                                default: 0.0,
                                fmt: &pan,
                                hint: tr.smp_k_zone_pan_hint,
                                log: false,
                            },
                            z.pan,
                        ),
                        (
                            P {
                                label: tr.smp_k_vel,
                                min: 0.0,
                                max: 1.0,
                                default: 1.0,
                                fmt: &pct,
                                hint: tr.smp_k_vel_hint,
                                log: false,
                            },
                            z.velocity_tracking,
                        ),
                        (
                            P {
                                label: tr.smp_k_group,
                                min: 0.0,
                                max: 16.0,
                                default: 0.0,
                                fmt: &fmt_grp,
                                hint: tr.smp_k_group_hint,
                                log: false,
                            },
                            z.params.mute_group as f64,
                        ),
                    ];
                    for (k, (spec, v)) in rows.iter().enumerate() {
                        if let Some(nv) = this.pknob(ui, spec, t::SMP_START, *v) {
                            this.edit_zones(id, false, |z| match k {
                                0 => z.gain = 10f64.powf(nv / 20.0).min(16.0),
                                1 => z.pan = nv,
                                2 => z.velocity_tracking = nv,
                                _ => z.params.mute_group = nv.round() as u8,
                            });
                        }
                    }
                });
            });
        };

        if wide {
            let avail = ui.available_width();
            let left_w = ((avail - 8.0) * 0.44).floor();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui_with_layout(
                    vec2(left_w, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| draw_pitch(self, ui),
                );
                ui.allocate_ui_with_layout(
                    vec2(avail - 8.0 - left_w, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| draw_amp(self, ui),
                );
            });
        } else {
            draw_pitch(self, ui);
            draw_amp(self, ui);
        }

        // Time stretch: the pitch stays, the length follows a ratio or the tempo.
        smp_bay(ui, |ui| {
            use keyframexl::sampler::StretchMode;
            smp_section_head(ui, tr.smp_sec_stretch, t::SMP_FILTER2);
            ui.horizontal_wrapped(|ui| {
                let modes: Vec<&str> = StretchMode::ALL
                    .iter()
                    .map(|m| tr.smp_stretch_mode(*m))
                    .collect();
                let cur = StretchMode::ALL
                    .iter()
                    .position(|m| *m == z.stretch.mode)
                    .unwrap_or(0);
                if let Some(i) = smp_pills(
                    ui,
                    &format!("stretch-mode-{id}"),
                    &modes,
                    cur,
                    t::SMP_FILTER2,
                ) {
                    self.edit_zones(id, true, |z| z.stretch.mode = StretchMode::ALL[i]);
                }
                match z.stretch.mode {
                    StretchMode::Off => {}
                    StretchMode::Time => {
                        let spec = P {
                            label: tr.smp_k_length,
                            min: 0.25,
                            max: 4.0,
                            default: 1.0,
                            fmt: &|v| format!("{:.0}%", v * 100.0),
                            hint: tr.smp_k_length_hint,
                            log: true,
                        };
                        if let Some(nv) = self.pknob(ui, &spec, t::SMP_FILTER2, z.stretch.amount) {
                            self.edit_zones(id, false, |z| z.stretch.amount = nv.clamp(0.25, 4.0));
                        }
                    }
                    StretchMode::Beat => {
                        let spec = P {
                            label: tr.smp_k_beats,
                            min: 0.25,
                            max: 64.0,
                            default: 4.0,
                            fmt: &|v| format!("{v:.2}"),
                            hint: tr.smp_k_beats_hint,
                            log: true,
                        };
                        if let Some(nv) = self.pknob(ui, &spec, t::SMP_FILTER2, z.stretch.beats) {
                            // Quarter-beat steps.
                            self.edit_zones(id, false, |z| {
                                z.stretch.beats = ((nv * 4.0).round() / 4.0).clamp(0.25, 64.0)
                            });
                        }
                    }
                }
                let len = z.end.saturating_sub(z.offset) as f64 / z.sample_rate.max(1) as f64;
                let len_str = format!("{len:.2}");
                let state = match z.stretch_target(self.song.tempo) {
                    None if z.stretch.mode != StretchMode::Off => {
                        fill(tr.smp_stretch_recorded, &[("len", &len_str)])
                    }
                    None => String::new(),
                    Some(n) => {
                        let to = n as f64 / z.sample_rate.max(1) as f64;
                        let to_str = format!("{to:.2}");
                        let ready = if z.stretch_stale(self.song.tempo) {
                            tr.smp_stretch_rendering
                        } else {
                            tr.smp_stretch_ready
                        };
                        fill(
                            tr.smp_stretch_arrow,
                            &[("len", &len_str), ("to", &to_str), ("state", ready)],
                        )
                    }
                };
                if !state.is_empty() {
                    ui.label(
                        egui::RichText::new(state)
                            .font(t::mono(11.0))
                            .color(t::faint()),
                    );
                }
            });
        });

        // Interactive bendable DAHDSR Amp Envelope.
        smp_bay(ui, |ui| {
            let env = z.adsr.clone();
            if let Some(new_env) = self.envelope_bay(
                ui,
                &format!("amp-{id}"),
                tr.smp_sec_amp_env,
                t::SMP_GOLD,
                &env,
            ) {
                self.edit_zones(id, false, |z| z.adsr = new_env.clone());
            }
        });
    }

    /// Hero Interactive Audio Waveform Scope with draggable Start/End/Loop flags,
    /// zoom/pan, loop mode controls, and zero-crossing snap.
    fn waveform_scope(&mut self, ui: &mut Ui, id: u64, zi: usize, z: &Zone) {
        let tr = self.tr();
        let lang = tr.lang;
        let loop_types = tr.smp_loop_types();
        let wv_key = egui::Id::new(("smp-wave-view", id, zi));
        let mut wv: WaveViewState = ui.ctx().data(|d| d.get_temp(wv_key)).unwrap_or_default();
        let total = z.frames.len().max(1);

        smp_bay(ui, |ui| {
            // Top control rail: Loop Mode, Forward/Reverse, Zoom, Snap 0x.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                if let Some(i) = smp_pills(
                    ui,
                    &format!("lt-{id}"),
                    &loop_types,
                    loop_type(z),
                    t::SMP_LOOP,
                ) {
                    self.edit_zones(id, true, |z| set_loop_type(z, i));
                }
                if let Some(i) = smp_pills(
                    ui,
                    &format!("rev-{id}"),
                    &[tr.smp_dir_fwd, tr.smp_dir_rev],
                    z.params.reverse as usize,
                    t::SMP_GOLD,
                ) {
                    self.edit_zones(id, true, |z| z.params.reverse = i == 1);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(tr.smp_snap_0x)
                        .on_hover_text(tr.smp_snap_0x_tip)
                        .clicked()
                    {
                        self.edit_zones(id, true, |z| {
                            let len = z.frames.len();
                            if len < 4 {
                                return;
                            }
                            let new_off =
                                snap_zero_crossing(&z.frames, z.offset, 0, z.end.saturating_sub(2));
                            z.offset = new_off;
                            if z.loop_mode != LoopMode::None {
                                let ls = snap_zero_crossing(
                                    &z.frames,
                                    z.loop_start.max(z.offset),
                                    z.offset,
                                    z.loop_end.saturating_sub(2).max(z.offset),
                                );
                                let le = snap_zero_crossing(
                                    &z.frames,
                                    z.loop_end,
                                    (ls + 2).min(z.end),
                                    z.end,
                                );
                                if le > ls + 1 {
                                    z.loop_start = ls;
                                    z.loop_end = le;
                                }
                            }
                        });
                    }
                    if ui
                        .small_button(tr.smp_fit)
                        .on_hover_text(tr.smp_fit_tip)
                        .clicked()
                    {
                        wv = WaveViewState::default();
                    }
                    if ui
                        .small_button("+")
                        .on_hover_text(tr.smp_zoom_in_tip)
                        .clicked()
                    {
                        wv.zoom_at(0.5, 0.55);
                    }
                    if ui
                        .small_button("−")
                        .on_hover_text(tr.smp_zoom_out_tip)
                        .clicked()
                    {
                        wv.zoom_at(0.5, 1.8);
                    }
                });
            });

            // Waveform Canvas.
            let (area, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::click_and_drag());
            let p = ui.painter().clone();
            let well = smp_well(&p, area, 6.0);
            let r = well.shrink2(vec2(4.0, 6.0));

            // Scroll wheel zooms around cursor; middle-drag pans.
            if resp.hovered() {
                let wheel = ui.input_mut(|i| {
                    let d = i.smooth_scroll_delta.y;
                    if d != 0.0 {
                        i.smooth_scroll_delta.y = 0.0;
                    }
                    d
                });
                if wheel != 0.0 {
                    let anchor = resp
                        .hover_pos()
                        .map(|pt| ((pt.x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0) as f64)
                        .unwrap_or(0.5);
                    let factor = if wheel > 0.0 { 0.8 } else { 1.25 };
                    wv.zoom_at(anchor, factor);
                }
            }
            if let Some(delta) = kit::middle_pan(ui, area, ui.id().with(("smp-wave-pan", id))) {
                let df = -(delta.x as f64 / r.width().max(1.0) as f64) * wv.span();
                wv.pan(df);
            }

            let f_to_x = |frame: usize| -> f32 {
                let frac = frame as f64 / total as f64;
                r.left() + ((frac - wv.start) / wv.span()) as f32 * r.width()
            };
            let x_to_f = |px: f32| -> usize {
                let u = ((px - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0) as f64;
                ((wv.start + u * wv.span()) * total as f64)
                    .round()
                    .clamp(0.0, total as f64) as usize
            };

            // Compute / paint downsampled waveform peaks.
            let f0 = (wv.start * total as f64).floor().clamp(0.0, total as f64) as usize;
            let f1 = (wv.end * total as f64).ceil().clamp(1.0, total as f64) as usize;
            let buckets = (r.width().round() as usize).clamp(32, 480);
            let peaks = compute_peaks(&z.frames, f0, f1, buckets);

            // Center zero line.
            p.line_segment(
                [r.left_center(), r.right_center()],
                Stroke::new(1.0_f32, t::SMP_LINE.gamma_multiply(0.6)),
            );

            let looping = z.loop_mode != LoopMode::None && z.loop_end > z.loop_start + 1;
            let sx = f_to_x(z.offset);
            let ex = f_to_x(z.end);
            let lsx = f_to_x(z.loop_start);
            let lex = f_to_x(z.loop_end);

            // Shaded Loop band behind waveform.
            if looping {
                let lr = Rect::from_min_max(
                    pos2(lsx.clamp(r.left(), r.right()), r.top()),
                    pos2(lex.clamp(r.left(), r.right()), r.bottom()),
                );
                if lr.width() > 1.0 {
                    p.rect_filled(lr, 0.0, t::SMP_LOOP.gamma_multiply(0.14));
                }
            }

            // Draw waveform bars (active region warm gold, trimmed regions muted).
            let bw = r.width() / buckets as f32;
            let half_h = r.height() * 0.44;
            for (bi, &(lo_s, hi_s)) in peaks.iter().enumerate() {
                let x = r.left() + (bi as f32 + 0.5) * bw;
                let frame_at = f0 + (bi * (f1.saturating_sub(f0))) / buckets.max(1);
                let inside = frame_at >= z.offset && frame_at <= z.end;
                let in_loop = looping && frame_at >= z.loop_start && frame_at <= z.loop_end;
                let col = if !inside {
                    t::SMP_LINE
                } else if in_loop {
                    t::SMP_GOLD
                } else {
                    t::SMP_GOLD.gamma_multiply(0.85)
                };
                let y0 = r.center().y - hi_s * half_h;
                let y1 = r.center().y - lo_s * half_h;
                p.line_segment(
                    [
                        pos2(x, y0.min(r.center().y - 0.5)),
                        pos2(x, y1.max(r.center().y + 0.5)),
                    ],
                    Stroke::new(bw.max(1.0), col),
                );
            }

            // Darken trimmed regions outside offset..end.
            if sx > r.left() {
                p.rect_filled(
                    Rect::from_min_max(r.min, pos2(sx.min(r.right()), r.bottom())),
                    0.0,
                    t::SMP_WELL.gamma_multiply(0.58),
                );
            }
            if ex < r.right() {
                p.rect_filled(
                    Rect::from_min_max(pos2(ex.max(r.left()), r.top()), r.max),
                    0.0,
                    t::SMP_WELL.gamma_multiply(0.58),
                );
            }

            // Paint draggable splice flags: S (Start), E (End), LS (Loop Start), LE (Loop End).
            let paint_flag = |px: f32, top: bool, label: &str, col: Color32| {
                if px < r.left() - 2.0 || px > r.right() + 2.0 {
                    return;
                }
                p.line_segment(
                    [pos2(px, r.top()), pos2(px, r.bottom())],
                    Stroke::new(1.5_f32, col),
                );
                let fw = if label.len() > 1 { 20.0 } else { 15.0 };
                let fh = 14.0;
                let fy = if top { r.top() } else { r.bottom() - fh };
                let fx = if label == "E" || label == "LE" {
                    (px - fw).max(r.left())
                } else {
                    px.min(r.right() - fw)
                };
                let fr = Rect::from_min_size(pos2(fx, fy), vec2(fw, fh));
                p.rect_filled(fr, 2.0, col);
                p.text(
                    fr.center(),
                    Align2::CENTER_CENTER,
                    label,
                    t::mono(10.0),
                    t::edge(),
                );
            };

            paint_flag(sx, true, "S", t::SMP_START);
            paint_flag(ex, true, "E", t::SMP_GOLD);
            if looping {
                paint_flag(lsx, false, "LS", t::SMP_LOOP);
                paint_flag(lex, false, "LE", t::SMP_LOOP);
            }

            // Handle dragging: 1=Start, 2=End, 3=LoopStart, 4=LoopEnd.
            let h_id = ui.id().with(("smp-wave-handle", id, zi));
            let hit_handle = |pt: Pos2| -> Option<u8> {
                let mut best: Option<(u8, f32)> = None;
                let mut check = |code: u8, hx: f32, y_bias: bool| {
                    let dx = (pt.x - hx).abs();
                    let dy_ok = if y_bias {
                        pt.y <= r.center().y + 12.0
                    } else {
                        pt.y >= r.center().y - 12.0
                    };
                    if dx <= 10.0 && dy_ok && best.is_none_or(|(_, bd)| dx < bd) {
                        best = Some((code, dx));
                    }
                };
                check(1, sx, true);
                check(2, ex, true);
                if looping {
                    check(3, lsx, false);
                    check(4, lex, false);
                }
                // Fallback: nearest vertical line regardless of top/bottom half.
                if best.is_none() {
                    let mut cands = vec![(1u8, (pt.x - sx).abs()), (2u8, (pt.x - ex).abs())];
                    if looping {
                        cands.push((3, (pt.x - lsx).abs()));
                        cands.push((4, (pt.x - lex).abs()));
                    }
                    cands.sort_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some(&(code, d)) = cands.first()
                        && d <= 9.0
                    {
                        best = Some((code, d));
                    }
                }
                best.map(|(c, _)| c)
            };

            if resp.hover_pos().and_then(hit_handle).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if resp.drag_started() {
                let h = resp.interact_pointer_pos().and_then(hit_handle);
                ui.data_mut(|d| d.insert_temp(h_id, h));
                if h.is_some() {
                    self.checkpoint();
                }
            }
            let active_h: Option<u8> = ui.data(|d| d.get_temp(h_id)).flatten();
            if resp.dragged()
                && let Some(h) = active_h
                && let Some(pt) = resp.interact_pointer_pos()
            {
                let f = x_to_f(pt.x);
                self.edit_zones(id, false, |z| {
                    let len = z.frames.len();
                    if len < 3 {
                        return;
                    }
                    match h {
                        1 => {
                            z.offset = f.min(z.end.saturating_sub(2));
                            if z.loop_mode != LoopMode::None && z.loop_start < z.offset {
                                z.loop_start = z.offset;
                                if z.loop_end <= z.loop_start + 1 {
                                    z.loop_end = (z.loop_start + 2).min(z.end);
                                }
                            }
                        }
                        2 => {
                            z.end = f.clamp((z.offset + 2).min(len), len);
                            if z.loop_mode != LoopMode::None && z.loop_end > z.end {
                                z.loop_end = z.end;
                                if z.loop_start + 1 >= z.loop_end {
                                    z.loop_start = z.loop_end.saturating_sub(2).max(z.offset);
                                }
                            }
                        }
                        3 if z.loop_mode != LoopMode::None => {
                            z.loop_start =
                                f.clamp(z.offset, z.loop_end.saturating_sub(2).max(z.offset));
                        }
                        4 if z.loop_mode != LoopMode::None => {
                            z.loop_end = f.clamp((z.loop_start + 2).min(z.end), z.end);
                        }
                        _ => {}
                    }
                });
            }
            if resp.drag_stopped() {
                ui.data_mut(|d| d.remove::<Option<u8>>(h_id));
            }

            // Centered top telemetry readout (stays clear of S and E splice flags).
            let active_ms = z.end.saturating_sub(z.offset) as f64 / z.sample_rate.max(1) as f64;
            let len_s = secs(active_ms);
            let info = if looping {
                let lp_ms =
                    z.loop_end.saturating_sub(z.loop_start) as f64 / z.sample_rate.max(1) as f64;
                let lp_s = secs(lp_ms);
                fill(tr.smp_wave_len_loop, &[("len", &len_s), ("lp", &lp_s)])
            } else {
                let frames_s = tr
                    .smp_frames_count
                    .fmt(lang, z.end.saturating_sub(z.offset));
                fill(
                    tr.smp_wave_len_frames,
                    &[("len", &len_s), ("frames", &frames_s)],
                )
            };
            p.text(
                well.center_top() + vec2(0.0, 4.0),
                Align2::CENTER_TOP,
                info,
                t::mono(12.0),
                t::SMP_SAND,
            );
            let s_str = z.offset.to_string();
            let e_str = z.end.to_string();
            let tot_str = total.to_string();
            let ls_str = z.loop_start.to_string();
            let le_str = z.loop_end.to_string();
            kit::tip(
                resp,
                fill(
                    tr.smp_wave_tip,
                    &[
                        ("s", &s_str),
                        ("e", &e_str),
                        ("total", &tot_str),
                        ("ls", &ls_str),
                        ("le", &le_str),
                    ],
                ),
            );
        });

        ui.ctx().data_mut(|d| d.insert_temp(wv_key, wv));
    }

    // ---------------------------------------------------------------- Page 2: Filter & Env

    fn page_filter(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        let tr = self.tr();
        // Hero Interactive Dual-Filter Frequency Graph.
        smp_bay(ui, |ui| {
            ui.horizontal(|ui| {
                let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                ui.painter().rect_filled(sq, 1.5, t::SMP_GOLD);
                kit::caps(
                    ui.painter(),
                    pos2(sq.right() + 6.0, sq.center().y),
                    Align2::LEFT_CENTER,
                    tr.smp_sec_dual_filter,
                    11.0,
                    t::SMP_SAND,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let routes = [tr.smp_route_serial, tr.smp_route_parallel];
                    let cur = FilterRouting::ALL
                        .iter()
                        .position(|r| *r == z.params.routing)
                        .unwrap_or(0);
                    if let Some(i) = smp_pills(ui, &format!("fr-{id}"), &routes, cur, t::SMP_GOLD) {
                        self.edit_zones(id, true, |z| z.params.routing = FilterRouting::ALL[i]);
                    }
                });
            });

            self.dual_filter_scope(ui, id, z);
        });

        // Side-by-side Filter 1 & Filter 2 controls.
        let wide = ui.available_width() >= TWO_COLUMNS;
        let filter_card = |this: &mut Self, ui: &mut Ui, fi: usize| {
            let col = if fi == 0 {
                t::enc_orange()
            } else {
                t::SMP_FILTER2
            };
            let f = z.params.filters[fi].clone();
            smp_bay(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                    ui.painter().rect_filled(sq, 1.5, col);
                    let fn_str = (fi + 1).to_string();
                    ui.label(
                        egui::RichText::new(fill(tr.smp_filter_n, &[("n", &fn_str)]))
                            .font(t::mono(12.0))
                            .color(t::SMP_SAND),
                    );
                    let short = [
                        tr.smp_fk_short_off,
                        "LP",
                        "HP",
                        "BP",
                        tr.smp_fk_short_notch,
                        "AP",
                    ];
                    let cur = FilterKind::ALL
                        .iter()
                        .position(|k| *k == f.kind)
                        .unwrap_or(0);
                    if let Some(i) = smp_pills(ui, &format!("fk-{id}-{fi}"), &short, cur, col) {
                        this.edit_zones(id, true, |z| {
                            z.params.filters[fi].kind = FilterKind::ALL[i]
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if let Some(v) = this.pknob(
                        ui,
                        &P {
                            label: tr.smp_k_freq,
                            min: 20.0,
                            max: 20000.0,
                            default: 20000.0,
                            fmt: &hz,
                            hint: tr.smp_k_freq_hint,
                            log: true,
                        },
                        col,
                        f.cutoff,
                    ) {
                        this.edit_zones(id, false, |z| z.params.filters[fi].cutoff = v);
                    }
                    if let Some(v) = this.pknob(
                        ui,
                        &P {
                            label: tr.smp_k_emph,
                            min: 0.0,
                            max: 1.0,
                            default: 0.0,
                            fmt: &pct,
                            hint: tr.smp_k_emph_hint,
                            log: false,
                        },
                        col,
                        f.emphasis,
                    ) {
                        this.edit_zones(id, false, |z| z.params.filters[fi].emphasis = v);
                    }
                });
            });
        };

        if wide {
            let avail = ui.available_width();
            let half = ((avail - 8.0) * 0.5).floor();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui_with_layout(
                    vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| filter_card(self, ui, 0),
                );
                ui.allocate_ui_with_layout(
                    vec2(avail - 8.0 - half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| filter_card(self, ui, 1),
                );
            });
        } else {
            filter_card(self, ui, 0);
            filter_card(self, ui, 1);
        }

        // ENV 1 & ENV 2 (interactive bendable DAHDSR curves).
        for ei in 0..2 {
            let src = if ei == 0 {
                ModSource::Env1
            } else {
                ModSource::Env2
            };
            let routed: Vec<&str> = z
                .params
                .matrix
                .iter()
                .filter(|m| m.source == src && m.amount.abs() > 0.001)
                .map(|m| tr.smp_mod_target(m.target))
                .collect();
            let en_str = (ei + 1).to_string();
            let title = if routed.is_empty() {
                fill(tr.smp_env_unrouted, &[("n", &en_str)])
            } else {
                let targets = routed.join(", ");
                fill(tr.smp_env_routed, &[("n", &en_str), ("targets", &targets)])
            };
            let env = z.params.env[ei].clone();
            smp_bay(ui, |ui| {
                if let Some(new_env) =
                    self.envelope_bay(ui, &format!("env{ei}-{id}"), &title, t::SMP_START, &env)
                {
                    self.edit_zones(id, false, |z| z.params.env[ei] = new_env.clone());
                }
            });
        }
    }

    fn dual_filter_scope(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        let tr = self.tr();
        let (area, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), 108.0), Sense::click_and_drag());
        let p = ui.painter().clone();
        let well = smp_well(&p, area, 6.0);
        let r = Rect::from_min_max(well.min + vec2(10.0, 20.0), well.max - vec2(10.0, 14.0));

        let fx = |freq: f64| {
            r.left() + ((freq.clamp(20.0, 20000.0) / 20.0).log10() / 3.0) as f32 * r.width()
        };
        let y_db = |db: f64| r.bottom() - ((db + 42.0) / 60.0) as f32 * r.height();

        for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
            let x = fx(f);
            p.line_segment(
                [pos2(x, r.top()), pos2(x, r.bottom())],
                Stroke::new(1.0_f32, t::SMP_LINE.gamma_multiply(0.5)),
            );
            p.text(
                pos2(x, well.bottom() - 2.0),
                Align2::CENTER_BOTTOM,
                label,
                t::mono(12.0),
                t::SMP_SAND.gamma_multiply(0.65),
            );
        }
        p.line_segment(
            [pos2(r.left(), y_db(0.0)), pos2(r.right(), y_db(0.0))],
            Stroke::new(1.0_f32, t::SMP_LINE),
        );

        let f1 = &z.params.filters[0];
        let f2 = &z.params.filters[1];
        let make_pts = |eval: &dyn Fn(f64) -> f64| -> Vec<Pos2> {
            (0..=120)
                .map(|i| {
                    let u = i as f64 / 120.0;
                    let freq = 20.0 * 1000.0_f64.powf(u);
                    let g = eval(freq);
                    let d = (20.0 * g.max(0.001).log10()).clamp(-42.0, 18.0);
                    pos2(r.left() + u as f32 * r.width(), y_db(d))
                })
                .collect()
        };

        if f1.kind != FilterKind::Off {
            let pts1 = make_pts(&|fr| filter_gain(f1.kind, fr, f1.cutoff, f1.emphasis));
            p.add(Shape::line(
                pts1,
                Stroke::new(1.4_f32, t::enc_orange().gamma_multiply(0.75)),
            ));
        }
        if f2.kind != FilterKind::Off {
            let pts2 = make_pts(&|fr| filter_gain(f2.kind, fr, f2.cutoff, f2.emphasis));
            p.add(Shape::line(
                pts2,
                Stroke::new(1.4_f32, t::SMP_FILTER2.gamma_multiply(0.75)),
            ));
        }

        // Combined serial / parallel response in signature warm brass.
        let sum_pts = make_pts(&|fr| {
            let g1 = filter_gain(f1.kind, fr, f1.cutoff, f1.emphasis);
            let g2 = filter_gain(f2.kind, fr, f2.cutoff, f2.emphasis);
            match (f1.kind != FilterKind::Off, f2.kind != FilterKind::Off) {
                (false, false) => 1.0,
                (true, false) => g1,
                (false, true) => g2,
                (true, true) => match z.params.routing {
                    FilterRouting::Serial => g1 * g2,
                    FilterRouting::Parallel => 0.5 * (g1 + g2),
                },
            }
        });
        fill_under_curve(&p, &sum_pts, r.bottom(), t::SMP_GOLD.gamma_multiply(0.12));
        p.add(Shape::line(sum_pts, Stroke::new(2.0_f32, t::SMP_GOLD)));

        // Draggable nodes for Filter 1 and Filter 2.
        let node_pos = |flt: &Filter| {
            pos2(
                fx(flt.cutoff),
                r.bottom() - (0.28 + flt.emphasis as f32 * 0.62) * r.height(),
            )
        };
        let nodes = [(0usize, f1, t::enc_orange()), (1usize, f2, t::SMP_FILTER2)];
        let drag_id = ui.id().with(("smp-flt-node", id));
        if resp.drag_started()
            && let Some(pt) = resp.interact_pointer_pos()
        {
            let hit = nodes
                .iter()
                .filter(|(_, f, _)| f.kind != FilterKind::Off)
                .min_by(|(_, a, _), (_, b, _)| {
                    pt.distance(node_pos(a))
                        .total_cmp(&pt.distance(node_pos(b)))
                })
                .filter(|(_, f, _)| pt.distance(node_pos(f)) <= 22.0)
                .map(|(i, _, _)| *i);
            ui.data_mut(|d| d.insert_temp(drag_id, hit));
            if hit.is_some() {
                self.checkpoint();
            }
        }
        let active_fi: Option<usize> = ui.data(|d| d.get_temp(drag_id)).flatten();
        if resp.dragged()
            && let Some(fi) = active_fi
        {
            let fine = ui.input(|i| i.modifiers.shift);
            let d = kit::captured_drag(ui, &resp);
            let s = if fine { 0.15 } else { 1.0 };
            self.edit_zones(id, false, |z| {
                let flt = &mut z.params.filters[fi];
                if d.x != 0.0 {
                    let u = ((flt.cutoff / 20.0).log10() / 3.0
                        + d.x as f64 / r.width().max(60.0) as f64 * s)
                        .clamp(0.0, 1.0);
                    flt.cutoff = (20.0 * 1000.0_f64.powf(u)).clamp(20.0, 20000.0);
                }
                if d.y != 0.0 {
                    flt.emphasis = (flt.emphasis - d.y as f64 / r.height().max(40.0) as f64 * s)
                        .clamp(0.0, 1.0);
                }
            });
        }
        if resp.drag_stopped() {
            ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
        }

        for (fi, flt, col) in nodes {
            if flt.kind == FilterKind::Off {
                continue;
            }
            let c = node_pos(flt);
            p.circle_filled(c, 8.0, t::SMP_WELL);
            p.circle_filled(c, 6.5, col);
            p.text(
                c,
                Align2::CENTER_CENTER,
                format!("{}", fi + 1),
                t::mono(10.0),
                t::edge(),
            );
        }

        let status = match (f1.kind != FilterKind::Off, f2.kind != FilterKind::Off) {
            (false, false) => tr.smp_filters_bypassed.to_string(),
            (true, false) => format!("F1 {} {}", tr.smp_filter_kind(f1.kind), hz(f1.cutoff)),
            (false, true) => format!("F2 {} {}", tr.smp_filter_kind(f2.kind), hz(f2.cutoff)),
            (true, true) => format!(
                "F1 {} · F2 {} ({})",
                hz(f1.cutoff),
                hz(f2.cutoff),
                tr.smp_filter_routing(z.params.routing)
            ),
        };
        p.text(
            well.left_top() + vec2(8.0, 4.0),
            Align2::LEFT_TOP,
            status,
            t::mono(12.0),
            t::SMP_SAND,
        );
        kit::tip(resp, tr.smp_dual_filter_tip);
    }

    // ---------------------------------------------------------------- Page 3: Modulation

    fn page_modulation(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        let tr = self.tr();
        let wide = ui.available_width() >= TWO_COLUMNS;
        let lfo_card = |this: &mut Self, ui: &mut Ui, li: usize| {
            let l = z.params.lfo[li].clone();
            smp_bay(ui, |ui| {
                ui.horizontal(|ui| {
                    let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                    ui.painter().rect_filled(sq, 1.5, t::SMP_START);
                    ui.label(
                        egui::RichText::new(format!("LFO {}", li + 1))
                            .font(t::mono(12.0))
                            .color(t::SMP_SAND),
                    );
                    let mut shape = l.shape;
                    egui::ComboBox::from_id_salt(("lfo-shape", id, li))
                        .selected_text(tr.smp_lfo_shape(shape))
                        .width(104.0)
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            for s in LfoShape::ALL {
                                ui.selectable_value(&mut shape, *s, tr.smp_lfo_shape(*s));
                            }
                        });
                    if shape != l.shape {
                        this.edit_zones(id, true, |z| z.params.lfo[li].shape = shape);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(i) = smp_pills(
                            ui,
                            &format!("fr-{id}-{li}"),
                            &[tr.smp_lfo_sync, tr.smp_lfo_free],
                            l.free_run as usize,
                            t::SMP_START,
                        ) {
                            this.edit_zones(id, true, |z| z.params.lfo[li].free_run = i == 1);
                        }
                    });
                });

                sampler_lfo_scope(ui, &l);

                ui.horizontal_wrapped(|ui| {
                    let rows: [(P, f64); 3] = [
                        (
                            P {
                                label: tr.smp_k_rate,
                                min: 0.01,
                                max: 50.0,
                                default: 5.0,
                                fmt: &|v| format!("{v:.2} Hz"),
                                hint: tr.smp_k_lfo_rate_hint,
                                log: true,
                            },
                            l.rate,
                        ),
                        (
                            P {
                                label: tr.smp_k_attack,
                                min: 0.0,
                                max: 10.0,
                                default: 0.0,
                                fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                                hint: tr.smp_k_mod_fade_hint,
                                log: false,
                            },
                            l.attack,
                        ),
                        (
                            P {
                                label: tr.smp_k_phase,
                                min: 0.0,
                                max: 1.0,
                                default: 0.0,
                                fmt: &|v| format!("{:.0}°", v * 360.0),
                                hint: tr.smp_k_phase_hint,
                                log: false,
                            },
                            l.phase,
                        ),
                    ];
                    for (k, (spec, v)) in rows.iter().enumerate() {
                        if let Some(nv) = this.pknob(ui, spec, t::SMP_START, *v) {
                            this.edit_zones(id, false, |z| match k {
                                0 => z.params.lfo[li].rate = nv,
                                1 => z.params.lfo[li].attack = nv,
                                _ => z.params.lfo[li].phase = nv,
                            });
                        }
                    }
                });
            });
        };

        if wide {
            let avail = ui.available_width();
            let half = ((avail - 8.0) * 0.5).floor();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui_with_layout(
                    vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| lfo_card(self, ui, 0),
                );
                ui.allocate_ui_with_layout(
                    vec2(avail - 8.0 - half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| lfo_card(self, ui, 1),
                );
            });
        } else {
            lfo_card(self, ui, 0);
            lfo_card(self, ui, 1);
        }

        // Visual Modulation Matrix.
        smp_bay(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (sq, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                ui.painter().rect_filled(sq, 1.5, t::SMP_GOLD);
                ui.label(
                    egui::RichText::new(tr.smp_mod_matrix_caps)
                        .font(t::mono(12.0))
                        .color(t::SMP_SAND),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let quick = [
                        (
                            format!("+ LFO2→{}", tr.smp_k_pan),
                            ModSource::Lfo2,
                            ModTarget::Pan,
                            0.45,
                        ),
                        (
                            format!("+ {}→F1", tr.smp_k_vel),
                            ModSource::Velocity,
                            ModTarget::Cutoff1,
                            0.45,
                        ),
                        (
                            format!("+ {}→F1", tr.smp_ms_env1),
                            ModSource::Env1,
                            ModTarget::Cutoff1,
                            0.50,
                        ),
                        (
                            format!("+ LFO1→{}", tr.smp_mt_pitch),
                            ModSource::Lfo1,
                            ModTarget::Pitch,
                            0.05,
                        ),
                    ];
                    for (label, src, dst, amt) in quick {
                        if ui.small_button(label).clicked() {
                            self.edit_zones(id, true, |z| {
                                z.params.route(src, dst, amt);
                            });
                        }
                    }
                });
            });

            let used = z
                .params
                .matrix
                .iter()
                .rposition(|m| m.source != ModSource::None)
                .map(|i| i + 1)
                .unwrap_or(0);
            let shown = (used + 1).min(MOD_SLOTS);
            let cols = if wide { 2 } else { 1 };
            let row_w = (ui.available_width() - 12.0 * (cols as f32 - 1.0)) / cols as f32;

            for chunk_start in (0..shown).step_by(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for c in 0..cols {
                        let si = chunk_start + c;
                        if si >= shown {
                            break;
                        }
                        let m = z.params.matrix[si];
                        ui.allocate_ui_with_layout(
                            vec2(row_w, 26.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let (dot, _) =
                                    ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                                ui.painter()
                                    .rect_filled(dot, 1.5, mod_source_color(m.source));
                                ui.label(
                                    egui::RichText::new(format!("{:02}", si + 1))
                                        .font(t::mono(12.0))
                                        .color(t::faint()),
                                );
                                let mut src = m.source;
                                egui::ComboBox::from_id_salt(("msrc", id, si))
                                    .selected_text(tr.smp_mod_source(src))
                                    .width(86.0)
                                    .show_ui(ui, |ui| {
                                        kit::menu_style(ui);
                                        for s in ModSource::ALL {
                                            ui.selectable_value(
                                                &mut src,
                                                *s,
                                                tr.smp_mod_source(*s),
                                            );
                                        }
                                    });
                                ui.label(egui::RichText::new("→").color(t::SMP_SAND));
                                let mut tgt = m.target;
                                egui::ComboBox::from_id_salt(("mtgt", id, si))
                                    .selected_text(tr.smp_mod_target(tgt))
                                    .width(106.0)
                                    .show_ui(ui, |ui| {
                                        kit::menu_style(ui);
                                        for s in ModTarget::ALL {
                                            ui.selectable_value(
                                                &mut tgt,
                                                *s,
                                                tr.smp_mod_target(*s),
                                            );
                                        }
                                    });
                                if src != m.source || tgt != m.target {
                                    self.edit_zones(id, true, |z| {
                                        z.params.matrix[si].source = src;
                                        z.params.matrix[si].target = tgt;
                                        if src != ModSource::None
                                            && z.params.matrix[si].amount == 0.0
                                        {
                                            z.params.matrix[si].amount = 0.25;
                                        }
                                    });
                                }
                                let mut a = m.amount;
                                let bar_w = (ui.available_width() - 28.0).clamp(62.0, 120.0);
                                let (started, changed) = smp_bipolar_bar(
                                    ui,
                                    bar_w,
                                    mod_source_color(src),
                                    &mut a,
                                    src != ModSource::None,
                                    tr.smp_mod_depth_hint,
                                );
                                if started {
                                    self.checkpoint();
                                }
                                if changed {
                                    self.edit_zones(id, false, |z| z.params.matrix[si].amount = a);
                                }
                                if m.source != ModSource::None
                                    && ui
                                        .small_button("×")
                                        .on_hover_text(tr.smp_clear_slot_tip)
                                        .clicked()
                                {
                                    self.edit_zones(id, true, |z| {
                                        z.params.matrix[si].source = ModSource::None;
                                        z.params.matrix[si].amount = 0.0;
                                    });
                                }
                            },
                        );
                    }
                });
            }
        });
    }

    // ---------------------------------------------------------------- Page 4: Zones Table

    fn page_zones(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset, selected: Option<usize>) {
        let tr = self.tr();
        let loop_types = tr.smp_loop_types();
        smp_bay(ui, |ui| {
            let p = ui.painter().clone();
            let cols = [
                ("", 0.0),
                ("#", 0.04),
                (tr.smp_col_wave, 0.085),
                (tr.smp_col_sample, 0.20),
                (tr.smp_col_keys, 0.425),
                (tr.smp_col_root, 0.535),
                (tr.smp_col_vel, 0.605),
                (tr.smp_col_loop, 0.695),
                ("A / D / S / R", 0.795),
            ];
            let (head, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
            let w = head.width();
            for (h, f) in cols {
                if !h.is_empty() {
                    kit::caps(
                        &p,
                        pos2(head.left() + w * f, head.center().y),
                        Align2::LEFT_CENTER,
                        h,
                        11.0,
                        t::SMP_SAND,
                    );
                }
            }

            let short_t = |v: f64| -> String {
                if v < 0.001 {
                    "0".into()
                } else if v < 1.0 {
                    format!("{:.0}m", v * 1000.0)
                } else {
                    format!("{v:.1}s")
                }
            };

            for (i, z) in preset.zones.iter().enumerate() {
                let (row, row_resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
                let is_sel = selected == Some(i);
                let code = t::pattern_codes()[i % t::pattern_codes().len()];
                if is_sel {
                    p.rect_filled(row, 4.0, t::SMP_DECK_HI);
                    p.rect_filled(
                        Rect::from_min_size(row.min, vec2(3.0, row.height())),
                        1.0,
                        t::SMP_GOLD,
                    );
                } else if row_resp.hovered() {
                    p.rect_filled(row, 4.0, t::SMP_DECK_HI.gamma_multiply(0.45));
                }
                p.line_segment(
                    [row.left_top(), row.right_top()],
                    Stroke::new(1.0_f32, t::SMP_LINE),
                );

                // Inline Audition button.
                let play_r = Rect::from_center_size(
                    pos2(row.left() + 14.0, row.center().y),
                    vec2(22.0, 22.0),
                );
                let play_resp = ui.interact(
                    play_r,
                    ui.id().with(("zn-play", id, i)),
                    Sense::click_and_drag(),
                );
                let held_id = ui.id().with(("zn-play-held", id, i));
                let down = play_resp.is_pointer_button_down_on();
                let was_down: bool = ui.data(|d| d.get_temp(held_id)).unwrap_or(false);
                if down {
                    self.hold_note(id, z.root, 104);
                } else if was_down {
                    self.release_note();
                }
                ui.data_mut(|d| d.insert_temp(held_id, down));
                p.rect_filled(play_r, 4.0, if down { t::SMP_GOLD } else { t::SMP_WELL });
                p.text(
                    play_r.center(),
                    Align2::CENTER_CENTER,
                    "▶",
                    t::mono(10.0),
                    if down { t::edge() } else { t::SMP_GOLD },
                );

                // Mini waveform sparkline.
                let spark = Rect::from_min_size(
                    pos2(row.left() + w * 0.085, row.top() + 5.0),
                    vec2((w * 0.105 - 6.0).max(28.0), row.height() - 10.0),
                );
                let peaks = compute_peaks(&z.frames, z.offset, z.end, 28);
                let sbw = spark.width() / peaks.len().max(1) as f32;
                for (bi, &(lo_s, hi_s)) in peaks.iter().enumerate() {
                    let x = spark.left() + (bi as f32 + 0.5) * sbw;
                    let y0 = spark.center().y - hi_s * spark.height() * 0.45;
                    let y1 = spark.center().y - lo_s * spark.height() * 0.45;
                    p.line_segment(
                        [pos2(x, y0), pos2(x, y1)],
                        Stroke::new(1.2_f32, if is_sel { t::SMP_GOLD } else { code }),
                    );
                }

                let cells = [
                    format!("{:02}", i + 1),
                    super::instrument::file_name(&z.name),
                    format!(
                        "{}–{}",
                        super::roll::note_name(z.low),
                        super::roll::note_name(z.high)
                    ),
                    super::roll::note_name(z.root),
                    format!("{}–{}", z.low_velocity, z.high_velocity),
                    loop_types[loop_type(z)].to_string(),
                    format!(
                        "{} / {} / {:.0}% / {}",
                        short_t(z.adsr.attack),
                        short_t(z.adsr.decay),
                        z.adsr.sustain * 100.0,
                        short_t(z.adsr.release)
                    ),
                ];
                let text_cols = [1usize, 3, 4, 5, 6, 7, 8];
                for (ci, &col_i) in text_cols.iter().enumerate() {
                    let f = cols[col_i].1;
                    let next_f = cols.get(col_i + 1).map(|c| c.1).unwrap_or(1.0);
                    let font = if col_i == 3 {
                        t::sans(13.0)
                    } else {
                        t::mono(12.0)
                    };
                    let max_w = (w * (next_f - f) - 8.0).max(20.0);
                    let shown = kit::elide(&p, &cells[ci], font.clone(), max_w);
                    p.text(
                        pos2(row.left() + w * f, row.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        font,
                        if col_i == 1 { code } else { t::text() },
                    );
                }

                if row_resp.double_clicked() && !down {
                    self.sampler_ui(id).zone = Some(i);
                    self.sampler_ui(id).page = 1;
                } else if row_resp.clicked() && !down {
                    self.sampler_ui(id).zone = Some(i);
                }
            }
        });
    }

    // ---------------------------------------------------------------- Interactive Bendable DAHDSR

    fn envelope_bay(
        &mut self,
        ui: &mut Ui,
        key: &str,
        title: &str,
        color: Color32,
        env: &Adsr,
    ) -> Option<Adsr> {
        smp_section_head(ui, title, color);
        let mut out: Option<Adsr> = None;
        ui.horizontal_wrapped(|ui| {
            // Interactive bendable DAHDSR canvas.
            let (area, resp) = ui.allocate_exact_size(vec2(210.0, 66.0), Sense::click_and_drag());
            let p = ui.painter().clone();
            let well = smp_well(&p, area, 6.0);
            let r = well.shrink2(vec2(8.0, 8.0));

            let total = (env.delay + env.attack + env.hold + env.decay + env.release).max(0.05);
            let sus_w = (total * 0.3).max(0.05);
            let span = total + sus_w;
            let x = |s: f64| r.left() + (s / span) as f32 * r.width();
            let y = |l: f64| r.bottom() - l.clamp(0.0, 1.0) as f32 * r.height();

            let a0 = env.delay;
            let a1 = a0 + env.attack;
            let d0 = a1 + env.hold;
            let d1 = d0 + env.decay;
            let r0 = d1 + sus_w;
            let r1 = r0 + env.release;
            let spans = [(a0, a1), (d0, d1), (r0, r1)];
            let seg_at = |px: f32| {
                spans
                    .iter()
                    .position(|&(s0, s1)| s1 > s0 && px >= x(s0) - 2.0 && px <= x(s1) + 2.0)
            };

            let drag_id = ui.id().with(("smp-env-seg", key));
            let hover = resp.hover_pos().and_then(|pt| seg_at(pt.x));
            if hover.is_some() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if resp.drag_started() {
                let s = resp.interact_pointer_pos().and_then(|pt| seg_at(pt.x));
                ui.data_mut(|d| d.insert_temp(drag_id, s));
                if s.is_some() {
                    self.checkpoint();
                }
            }
            let seg: Option<usize> = ui.data(|d| d.get_temp(drag_id)).flatten();
            let mut cur = env.clone();
            if resp.dragged()
                && let Some(i) = seg
            {
                let dy = kit::captured_drag(ui, &resp).y as f64 / 56.0;
                let c = match i {
                    0 => &mut cur.attack_curve,
                    1 => &mut cur.decay_curve,
                    _ => &mut cur.release_curve,
                };
                *c = (*c + if i == 0 { -dy } else { dy }).clamp(-1.0, 1.0);
                out = Some(cur.clone());
            }
            if resp.double_clicked()
                && let Some(i) = hover
            {
                let c = match i {
                    0 => &mut cur.attack_curve,
                    1 => &mut cur.decay_curve,
                    _ => &mut cur.release_curve,
                };
                if *c != 0.0 {
                    self.checkpoint();
                    *c = 0.0;
                    out = Some(cur.clone());
                }
            }
            if resp.drag_stopped() {
                ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
            }

            let shown_env = out.as_ref().unwrap_or(env);
            let mut pts = vec![pos2(x(0.0), y(0.0)), pos2(x(a0), y(0.0))];
            let steps = 20;
            for k in 1..=steps {
                let at = a0 + shown_env.attack * k as f64 / steps as f64;
                pts.push(pos2(x(at), y(shown_env.value(at, d1 + 1e9))));
            }
            pts.push(pos2(x(d0), y(1.0)));
            for k in 1..=steps {
                let at = d0 + shown_env.decay * k as f64 / steps as f64;
                pts.push(pos2(x(at), y(shown_env.value(at, d1 + 1e9))));
            }
            pts.push(pos2(x(r0), y(shown_env.sustain)));
            for k in 1..=steps {
                let pr = k as f64 / steps as f64;
                pts.push(pos2(
                    x(r0 + shown_env.release * pr),
                    y(shown_env.sustain
                        * (1.0 - keyframexl::sampler::bend(pr, shown_env.release_curve))),
                ));
            }
            if let Some(i) = seg.or(hover) {
                let (s0, s1) = spans[i];
                p.rect_filled(
                    Rect::from_min_max(pos2(x(s0), r.top()), pos2(x(s1), r.bottom())),
                    2.0,
                    color.gamma_multiply(0.09),
                );
            }
            fill_under_curve(&p, &pts, r.bottom(), color.gamma_multiply(0.14));
            p.add(Shape::line(pts, Stroke::new(1.8_f32, color)));
            let tr = self.tr();
            kit::tip(resp, tr.smp_env_curve_tip);

            let rows: [(P, f64); 6] = [
                (
                    P {
                        label: tr.smp_k_delay,
                        min: 0.0,
                        max: 10.0,
                        default: 0.0,
                        fmt: &secs,
                        hint: tr.smp_k_delay_hint,
                        log: true,
                    },
                    env.delay,
                ),
                (
                    P {
                        label: tr.smp_k_attack,
                        min: 0.0,
                        max: 10.0,
                        default: 0.005,
                        fmt: &secs,
                        hint: tr.smp_k_attack_hint,
                        log: true,
                    },
                    env.attack,
                ),
                (
                    P {
                        label: tr.smp_k_hold,
                        min: 0.0,
                        max: 10.0,
                        default: 0.0,
                        fmt: &secs,
                        hint: tr.smp_k_hold_hint,
                        log: true,
                    },
                    env.hold,
                ),
                (
                    P {
                        label: tr.smp_k_decay,
                        min: 0.0,
                        max: 20.0,
                        default: 0.0,
                        fmt: &secs,
                        hint: tr.smp_k_decay_hint,
                        log: true,
                    },
                    env.decay,
                ),
                (
                    P {
                        label: tr.smp_k_sustain,
                        min: 0.0,
                        max: 1.0,
                        default: 1.0,
                        fmt: &pct,
                        hint: tr.smp_k_sustain_hint,
                        log: false,
                    },
                    env.sustain,
                ),
                (
                    P {
                        label: tr.smp_k_release,
                        min: 0.0,
                        max: 20.0,
                        default: 0.015,
                        fmt: &secs,
                        hint: tr.smp_k_release_hint,
                        log: true,
                    },
                    env.release,
                ),
            ];
            for (k, (spec, v)) in rows.iter().enumerate() {
                if let Some(nv) = self.pknob(ui, spec, color, *v) {
                    let mut e = out.clone().unwrap_or_else(|| env.clone());
                    let nv = if spec.log && nv < 0.0011 { 0.0 } else { nv };
                    match k {
                        0 => e.delay = nv,
                        1 => e.attack = nv,
                        2 => e.hold = nv,
                        3 => e.decay = nv,
                        4 => e.sustain = nv,
                        _ => e.release = nv,
                    }
                    out = Some(e);
                }
            }
        });
        out
    }
}

/// Live LFO scope visualizing all 8 DirectWave shapes, start phase, and attack fade-in.
fn sampler_lfo_scope(ui: &mut Ui, lfo: &Lfo) {
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
    let p = ui.painter();
    let well = smp_well(p, area, 6.0);
    let r = Rect::from_min_max(well.min + vec2(8.0, 18.0), well.max - vec2(8.0, 6.0));
    p.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::SMP_LINE.gamma_multiply(0.6)),
    );

    let cycles = 2.5f64;
    let total_t = cycles / lfo.rate.max(0.05);
    let pts: Vec<Pos2> = (0..=160)
        .map(|i| {
            let u = i as f64 / 160.0;
            let t_sec = u * total_t;
            let fade = if lfo.attack > 0.0 {
                (t_sec / lfo.attack).min(1.0)
            } else {
                1.0
            };
            let v = lfo.shape.value(lfo.phase + u * cycles) * fade;
            pos2(
                r.left() + u as f32 * r.width(),
                r.center().y - (v as f32).clamp(-1.0, 1.0) * r.height() * 0.44,
            )
        })
        .collect();

    fill_under_curve(p, &pts, r.center().y, t::SMP_START.gamma_multiply(0.12));
    p.add(Shape::line(pts, Stroke::new(1.7_f32, t::SMP_START)));

    // Subtle moving phase dot.
    let now = ui.input(|i| i.time);
    let ph_u = (now * lfo.rate.min(8.0) / cycles).fract();
    let dot_v = lfo.shape.value(lfo.phase + ph_u * cycles);
    p.circle_filled(
        pos2(
            r.left() + ph_u as f32 * r.width(),
            r.center().y - (dot_v as f32).clamp(-1.0, 1.0) * r.height() * 0.44,
        ),
        3.2,
        t::text(),
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(40));

    p.text(
        well.right_top() + vec2(-6.0, 4.0),
        Align2::RIGHT_TOP,
        format!("{:.2} Hz · {:.0}°", lfo.rate, lfo.phase * 360.0),
        t::mono(12.0),
        t::SMP_SAND,
    );
}

#[cfg(test)]
mod sampler_ui_tests {
    use super::*;

    #[test]
    fn snap_zero_crossing_finds_nearest_upward_crossing() {
        let mut frames = vec![[0.0f32; 2]; 200];
        for (i, f) in frames.iter_mut().enumerate() {
            let s = ((i as f32 - 100.0) * 0.1).sin();
            *f = [s, s];
        }
        let snapped = snap_zero_crossing(&frames, 104, 0, 199);
        assert_eq!(snapped, 100);
    }

    #[test]
    fn compute_peaks_handles_empty_and_normal_audio() {
        assert_eq!(compute_peaks(&[], 0, 0, 16).len(), 16);
        let frames = vec![[0.5, 0.5], [-0.8, -0.8], [0.2, 0.2], [0.9, 0.9]];
        let p = compute_peaks(&frames, 0, 4, 2);
        assert_eq!(p.len(), 2);
        assert!(p[0].0 <= -0.79 && p[0].1 >= 0.49);
        assert!(p[1].1 >= 0.89);
    }

    #[test]
    fn filter_gain_is_finite_for_all_kinds() {
        for &kind in FilterKind::ALL {
            for freq in [40.0, 1000.0, 12000.0] {
                let g = filter_gain(kind, freq, 1000.0, 0.5);
                assert!(g.is_finite() && g >= 0.0);
            }
        }
    }
}
