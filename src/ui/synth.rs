//! KeyFrame Tide editor: oscillator and filter side by side, the two
//! envelopes under them, macros across the bottom. Each section carries its
//! address colour (blue oscillator, orange filter, green amp, white macros).

use super::{App, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::{
    sampler::Adsr,
    sequence::InstrumentKind,
    synth::{
        Filter, Lfo, LfoDivision, LfoShape, ModRoute, ModSource, ModTarget, Param, SynthPatch,
        Table, TableBank, Warp,
    },
};

/// Two columns from this width on, one below it.
const TWO_COLUMNS: f32 = 760.0;

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
        Table::SineSaw => "Sine–Saw",
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
        Warp::FmSub => "FM from sub",
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

    /// A pick among a few options (chips): one undo step per pick.
    fn chips<T: Copy + PartialEq>(
        &mut self,
        ui: &mut Ui,
        id: &str,
        items: &[T],
        names: &[&str],
        value: &mut T,
    ) {
        let at = items.iter().position(|v| v == value).unwrap_or(0);
        if let (Some(i), _) = kit::chips(ui, id, names, at, None)
            && i != at
        {
            *value = items[i];
            self.started = true;
            self.changed = true;
        }
    }
}

/// A section card: colour square, caps title, then the body on the panel.
pub(super) fn panel(ui: &mut Ui, title: &str, color: Color32, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t::SURFACE)
        .stroke(Stroke::new(1.0_f32, t::EDGE))
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
                t::MUTED,
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

/// The LFO's shape over two cycles (sample-and-hold as fixed steps).
fn lfo_view(ui: &mut Ui, lfo: &Lfo) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    let painter = ui.painter();
    let r = kit::sunken(painter, r, 6.0).shrink(7.0);
    painter.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::SURFACE),
    );
    let steps = [0.3, -0.7, 0.9, -0.2, 0.5, -0.9, 0.1, 0.7];
    let value = |x: f64| {
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
    let points: Vec<_> = (0..=200)
        .map(|i| {
            let x = i as f64 / 200.0;
            pos2(
                r.left() + x as f32 * r.width(),
                r.center().y - value(x) as f32 * r.height() * 0.45,
            )
        })
        .collect();
    let color = if lfo.shape == LfoShape::Off {
        t::STRONG
    } else {
        t::ENC_GREEN
    };
    painter.add(egui::Shape::line(points, Stroke::new(1.8_f32, color)));
    let rate = if lfo.shape == LfoShape::Off {
        "off".to_string()
    } else if lfo.sync {
        division_name(lfo.division).to_string()
    } else {
        format!("{:.2} Hz", lfo.rate.clamp(0.01, 30.0))
    };
    kit::caps(
        painter,
        r.right_top() + vec2(-2.0, 2.0),
        Align2::RIGHT_TOP,
        &rate,
        10.0,
        t::FAINT,
    );
}

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

/// A switch with its caption to the right (kit style).
fn labelled_switch(ui: &mut Ui, ed: &mut Edit, label: &str, on: &mut bool, hint: &str) {
    ui.horizontal(|ui| {
        if super::settings_ui::kit_switch(ui, on) {
            ed.started = true;
            ed.changed = true;
        }
        let (r, resp) = ui.allocate_exact_size(vec2(58.0, 20.0), Sense::hover());
        kit::caps(
            ui.painter(),
            r.left_center(),
            Align2::LEFT_CENTER,
            label,
            10.0,
            t::MUTED,
        );
        resp.on_hover_text(hint);
    });
}

/// Two LFOs: pick one, its shape, rate (free in Hz or synced to the tempo), retrigger.
fn lfo_panel(ui: &mut Ui, ed: &mut Edit, id: u64, lfos: &mut [Lfo; 2]) {
    panel(ui, "lfo", t::ENC_GREEN, |ui| {
        let key = egui::Id::new(("tide-lfo", id));
        let mut which: usize = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
        if let (Some(i), _) = kit::chips(
            ui,
            &format!("tide-lfo-pick-{id}"),
            &["LFO 1", "LFO 2"],
            which,
            None,
        ) {
            which = i;
            ui.ctx().data_mut(|d| d.insert_temp(key, which));
        }
        let lfo = &mut lfos[which.min(1)];
        lfo_view(ui, lfo);
        ed.chips(
            ui,
            &format!("tide-lfo-shape-{id}-{which}"),
            &SHAPES,
            &["Off", "Sine", "Tri", "Saw", "Square", "S&H"],
            &mut lfo.shape,
        );
        ui.horizontal_wrapped(|ui| {
            if lfo.sync {
                ed.chips(
                    ui,
                    &format!("tide-lfo-div-{id}-{which}"),
                    &DIVISIONS,
                    &DIVISIONS.map(division_name),
                    &mut lfo.division,
                );
            } else {
                if lfo.rate <= 0.0 {
                    lfo.rate = 1.0;
                }
                ed.knob(
                    ui,
                    "RATE",
                    t::ENC_GREEN,
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
                    "Cycles per second",
                );
            }
        });
        ui.horizontal(|ui| {
            labelled_switch(
                ui,
                ed,
                "sync",
                &mut lfo.sync,
                "Follow the song tempo in note divisions",
            );
            labelled_switch(
                ui,
                ed,
                "retrig",
                &mut lfo.retrigger,
                "Restart the cycle on every note (off: one free-running cycle for the channel)",
            );
        });
    });
}

/// The synth's own effects after the voices, in a fixed order: a switch and
/// the knobs that matter for each (the mixer's editors have the rest).
fn fx_panel(ui: &mut Ui, ed: &mut Edit, id: u64, p: &mut SynthPatch) {
    use keyframexl::fx::{FxKind, FxSlot};
    use keyframexl::synth::FX_KINDS;
    type Knobs = &'static [(&'static str, usize, &'static str)];
    let knobs = |k: FxKind| -> Knobs {
        match k {
            FxKind::Drive => &[
                ("DRIVE", 0, "Saturation before the output level"),
                ("TONE", 1, "Brightness of the saturated signal"),
                ("MIX", 3, "Blend with the clean signal"),
            ],
            FxKind::Chorus => &[
                ("DEPTH", 1, "How far the copies sway"),
                ("RATE", 2, "How fast they sway"),
                ("MIX", 3, "Blend with the dry signal"),
            ],
            FxKind::Delay => &[
                ("TIME", 0, "Echo time in steps (follows the tempo)"),
                ("REPEAT", 1, "Feedback: how many echoes"),
                ("WET", 4, "Echo level"),
            ],
            _ => &[
                ("SIZE", 0, "Room size"),
                ("DAMP", 1, "Darker tail"),
                ("WET", 4, "Reverb level"),
            ],
        }
    };
    panel(ui, "effects", t::ENC_WHITE, |ui| {
        for kind in FX_KINDS {
            let at = p.fx.iter().position(|s| s.kind == kind);
            ui.horizontal(|ui| {
                let mut on = at.is_some_and(|i| p.fx[i].enabled);
                if super::settings_ui::kit_switch(ui, &mut on) {
                    ed.started = true;
                    ed.changed = true;
                    match at {
                        Some(i) => p.fx[i].enabled = on,
                        None => {
                            p.fx.push(FxSlot::new(kind));
                            p.fx.sort_by_key(|s| FX_KINDS.iter().position(|k| *k == s.kind));
                        }
                    }
                }
                let (r, _) = ui.allocate_exact_size(vec2(64.0, 20.0), Sense::hover());
                kit::caps(
                    ui.painter(),
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    kind.builtin().name,
                    10.0,
                    if on { t::TEXT } else { t::MUTED },
                );
                let Some(i) = p.fx.iter().position(|s| s.kind == kind) else {
                    return;
                };
                let specs = kind.params();
                let slot = &mut p.fx[i];
                for &(label, n, hint) in knobs(kind) {
                    let Some(spec) = specs.get(n) else { continue };
                    let mut v = slot.params[n] as f64;
                    let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
                    ui.push_id((id, label, n), |ui| {
                        ui.add_enabled_ui(slot.enabled, |ui| {
                            ed.knob(
                                ui,
                                label,
                                t::ENC_WHITE,
                                &mut v,
                                (spec.min as f64, spec.max as f64, spec.default as f64),
                                spec.log,
                                &fmt,
                                hint,
                            );
                        });
                    });
                    slot.params[n] = v as f32;
                }
            });
        }
    });
}

/// Colour of a modulation source (its card's address colour).
fn source_color(s: ModSource) -> Color32 {
    match s {
        ModSource::Lfo1 | ModSource::Lfo2 => t::ENC_GREEN,
        ModSource::FilterEnv => t::ENC_ORANGE,
        // What the player does: velocity and the live controllers.
        ModSource::Velocity | ModSource::ModWheel | ModSource::Pressure | ModSource::Slide => {
            t::ENC_BLUE
        }
        ModSource::Macro(_) => t::ENC_WHITE,
    }
}

/// The modulation matrix: one row per route (source → target, depth, remove).
fn mod_panel(ui: &mut Ui, ed: &mut Edit, id: u64, p: &mut SynthPatch) {
    panel(ui, "modulation", t::ENC_GREEN, |ui| {
        let mut remove = None;
        let cols = if ui.available_width() >= TWO_COLUMNS {
            2
        } else {
            1
        };
        let width = (ui.available_width() - 16.0 * (cols as f32 - 1.0)) / cols as f32;
        if p.mods.is_empty() {
            ui.label(egui::RichText::new("No routes. Add one to let an LFO, the filter envelope, velocity or a macro move a control.").font(t::sans(12.0)).color(t::FAINT));
        }
        for (row, chunk) in p.mods.chunks_mut(cols).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (k, r) in chunk.iter_mut().enumerate() {
                    let i = row * cols + k;
                    ui.allocate_ui_with_layout(
                        vec2(width, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().rect_filled(dot, 1.0, source_color(r.source));
                            egui::ComboBox::from_id_salt(("tide-mod-src", id, i))
                                .width(96.0)
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
                            let (arrow, _) =
                                ui.allocate_exact_size(vec2(12.0, 22.0), Sense::hover());
                            ui.painter().text(
                                arrow.center(),
                                Align2::CENTER_CENTER,
                                "→",
                                t::sans(13.0),
                                t::FAINT,
                            );
                            egui::ComboBox::from_id_salt(("tide-mod-dst", id, i))
                                .width((width - 230.0).clamp(90.0, 150.0))
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
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                            let k = kit::Knob {
                                label: "DEPTH",
                                color: source_color(r.source),
                                min: -1.0,
                                max: 1.0,
                                default: 0.0,
                                log: false,
                                fmt: &signed,
                                hint: "How far the source moves the target, either way",
                            };
                            let (_, s, c) = kit::mini_knob(
                                ui,
                                rect,
                                ui.id().with(("tide-mod-depth", i)),
                                &k,
                                &mut r.depth,
                            );
                            ed.started |= s;
                            ed.changed |= c;
                            ui.label(
                                egui::RichText::new(signed(r.depth))
                                    .font(t::mono(12.0))
                                    .color(t::MUTED),
                            );
                            if ui
                                .small_button("×")
                                .on_hover_text("Remove this route")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        },
                    );
                }
            });
        }
        if let Some(i) = remove {
            p.mods.remove(i);
            ed.started = true;
            ed.changed = true;
        }
        if p.mods.len() < 16
            && ui
                .button("+ Add route")
                .on_hover_text("LFO 1 → Cutoff to start; change both in the row")
                .clicked()
        {
            p.mods.push(ModRoute {
                source: ModSource::Lfo1,
                target: ModTarget::Cutoff,
                depth: 0.25,
            });
            ed.started = true;
            ed.changed = true;
        }
    });
}

/// Level, coarse and fine tuning of one oscillator.
fn tuning(
    ui: &mut Ui,
    ed: &mut Edit,
    level: &mut f64,
    coarse: &mut f64,
    fine: &mut f64,
    level_default: f64,
) {
    ui.horizontal_wrapped(|ui| {
        let c = t::ENC_BLUE;
        ed.knob(
            ui,
            "LEVEL",
            c,
            level,
            (0.0, 1.0, level_default),
            false,
            &pct,
            "Oscillator level",
        );
        ed.knob(
            ui,
            "COARSE",
            c,
            coarse,
            (-24.0, 24.0, 0.0),
            false,
            &|v| format!("{:+.0} st", v.round()),
            "Pitch in semitones",
        );
        ed.knob(
            ui,
            "FINE",
            c,
            fine,
            (-100.0, 100.0, 0.0),
            false,
            &|v| format!("{:+.0} ct", v.round()),
            "Pitch in cents",
        );
    });
}

/// Oscillator C: its partials as bars (what the controls give a C3), then
/// level, pitch, how many partials, tilt, odd/even and shimmer.
fn spectral_panel(ui: &mut Ui, ed: &mut Edit, c: &mut keyframexl::synth::Spectral) {
    use keyframexl::synth::MAX_PARTIALS;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    let painter = ui.painter();
    let r = kit::sunken(painter, r, 6.0).shrink(7.0);
    let mut amps = [0.0; MAX_PARTIALS];
    let n = c.amplitudes(130.8, 48_000.0, 0.0, &mut amps);
    let top = amps[..n].iter().fold(1e-9f64, |m, a| m.max(*a));
    let w = r.width() / MAX_PARTIALS as f32;
    let on = c.level > 0.0;
    for (i, a) in amps[..n].iter().enumerate() {
        let h = (a / top) as f32 * r.height();
        let x = r.left() + i as f32 * w;
        let bar = Rect::from_min_max(
            pos2(x + 1.0, r.bottom() - h.max(1.0)),
            pos2(x + w - 1.0, r.bottom()),
        );
        painter.rect_filled(bar, 1.0, if on { t::ENC_BLUE } else { t::STRONG });
    }
    let text = if on {
        format!("{n} partials")
    } else {
        "off · raise level".to_string()
    };
    kit::caps(
        painter,
        r.right_top() + vec2(-2.0, 2.0),
        Align2::RIGHT_TOP,
        &text,
        10.0,
        t::FAINT,
    );
    ui.horizontal_wrapped(|ui| {
        let col = t::ENC_BLUE;
        ed.knob(
            ui,
            "LEVEL",
            col,
            &mut c.level,
            (0.0, 1.0, 0.0),
            false,
            &pct,
            "Spectral oscillator level (0 = off)",
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
            (1.0, MAX_PARTIALS as f64, 16.0),
            false,
            &|v| format!("{:.0}", v.round()),
            "How many harmonics: 1 is a sine",
        );
        c.partials = partials.round().clamp(1.0, MAX_PARTIALS as f64) as u8;
    });
    ui.horizontal_wrapped(|ui| {
        let col = t::ENC_BLUE;
        ed.knob(
            ui,
            "TILT",
            col,
            &mut c.tilt,
            (-1.0, 1.0, 0.0),
            false,
            &signed,
            "Dark (−) to bright (+); 0 is a saw's slope",
        );
        ed.knob(
            ui,
            "ODD",
            col,
            &mut c.odd,
            (-1.0, 1.0, 0.0),
            false,
            &signed,
            "+ only odd harmonics (hollow, square-like), − only even (an octave up, airy)",
        );
        ed.knob(
            ui,
            "SHIMMER",
            col,
            &mut c.shimmer,
            (0.0, 1.0, 0.0),
            false,
            &pct,
            "Each harmonic swells and fades on its own slow cycle",
        );
    });
}

/// The warp mode chips with their caption.
fn mode_row(ui: &mut Ui, ed: &mut Edit, id: &str, warp: &mut Warp) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(44.0, 28.0), Sense::hover());
        kit::caps(
            ui.painter(),
            r.left_center(),
            Align2::LEFT_CENTER,
            "mode",
            10.0,
            t::FAINT,
        );
        ed.chips(ui, id, &WARPS, &WARPS.map(warp_name), warp);
    });
}

/// The oscillator's current frame, with its neighbours faint behind it.
fn wave_view(ui: &mut Ui, table: Table, position: f64) {
    static BANK: std::sync::OnceLock<TableBank> = std::sync::OnceLock::new();
    let bank = BANK.get_or_init(TableBank::new);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::hover());
    let painter = ui.painter();
    let r = kit::sunken(painter, r, 6.0).shrink(8.0);
    painter.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::SURFACE),
    );
    for delta in [-0.3, -0.15, 0.15, 0.3, 0.0] {
        let points: Vec<_> = (0..=160)
            .map(|i| {
                let x = i as f64 / 160.0;
                let v = bank.sample(table, (position + delta).clamp(0.0, 1.0), x, 220.0, 48000.0);
                pos2(
                    r.left() + x as f32 * r.width(),
                    r.center().y - (v as f32).clamp(-1.2, 1.2) * r.height() * 0.42,
                )
            })
            .collect();
        let (w, c) = if delta == 0.0 {
            (1.8_f32, t::ENC_BLUE)
        } else {
            (1.0_f32, t::ENC_BLUE.gamma_multiply(0.22))
        };
        painter.add(egui::Shape::line(points, Stroke::new(w, c)));
    }
    kit::caps(
        painter,
        r.left_top() + vec2(2.0, 2.0),
        Align2::LEFT_TOP,
        &format!("{}  ·  frame {:.0}%", table_name(table), position * 100.0),
        10.0,
        t::FAINT,
    );
}

/// The filter's response from 20 Hz to 20 kHz (an approximation of the SVF).
fn filter_view(ui: &mut Ui, p: &SynthPatch) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::hover());
    let painter = ui.painter();
    let r = kit::sunken(painter, r, 6.0).shrink(8.0);
    let y_db = |db: f64| r.bottom() - ((db + 48.0) / 66.0) as f32 * r.height();
    for f in [100.0, 1000.0, 10000.0] {
        let x = r.left() + ((f / 20.0_f64).log10() / 3.0) as f32 * r.width();
        painter.line_segment(
            [pos2(x, r.top()), pos2(x, r.bottom())],
            Stroke::new(1.0_f32, t::SURFACE),
        );
    }
    painter.line_segment(
        [pos2(r.left(), y_db(0.0)), pos2(r.right(), y_db(0.0))],
        Stroke::new(1.0_f32, t::SURFACE),
    );
    let q = 0.5 + p.resonance * 9.5;
    let points: Vec<_> = (0..=120)
        .map(|i| {
            let x = i as f64 / 120.0;
            let f = 20.0 * 1000.0_f64.powf(x);
            let k = f / p.cutoff;
            let denom = ((1.0 - k * k).powi(2) + (k / q).powi(2)).sqrt().max(0.001);
            let gain = match p.filter {
                Filter::LowPass => 1.0 / denom,
                Filter::HighPass => k * k / denom,
                Filter::BandPass => k / q / denom,
            };
            let db = (20.0 * gain.max(0.001).log10()).clamp(-48.0, 18.0);
            pos2(r.left() + x as f32 * r.width(), y_db(db))
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.8_f32, t::ENC_ORANGE),
    ));
    let cx = r.left() + ((p.cutoff / 20.0).log10() / 3.0) as f32 * r.width();
    painter.line_segment(
        [pos2(cx, r.top()), pos2(cx, r.bottom())],
        Stroke::new(1.0_f32, t::ENC_ORANGE.gamma_multiply(0.35)),
    );
    kit::caps(
        painter,
        r.left_top() + vec2(2.0, 2.0),
        Align2::LEFT_TOP,
        &hz(p.cutoff),
        10.0,
        t::FAINT,
    );
}

/// The envelope's shape, scaled to its own length (sustain drawn as a third of it).
/// Bend of segment `i`: attack, decay, release.
fn curve_mut(e: &mut Adsr, i: usize) -> &mut f64 {
    match i {
        0 => &mut e.attack_curve,
        1 => &mut e.decay_curve,
        _ => &mut e.release_curve,
    }
}

/// The envelope's shape. Drag a segment (attack, decay, release) up or down
/// to bend it; double-click it to straighten it again.
fn envelope_view(ui: &mut Ui, ed: &mut Edit, e: &mut Adsr, color: Color32) {
    let (r, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let r = kit::sunken(&painter, r, 6.0).shrink(7.0);
    let total = e.delay + e.attack + e.hold + e.decay + e.release;
    let sus = (total * 0.3).max(0.05);
    let span = total + sus;
    let x = |s: f64| r.left() + (s / span) as f32 * r.width();
    let y = |l: f64| r.bottom() - l as f32 * r.height();
    // Segment spans in seconds: attack, decay, release (the bendable ones).
    let a0 = e.delay;
    let d0 = a0 + e.attack + e.hold;
    let r0 = d0 + e.decay + sus;
    let spans = [
        (a0, a0 + e.attack),
        (d0, d0 + e.decay),
        (r0, r0 + e.release),
    ];
    let seg_at = |px: f32| {
        spans
            .iter()
            .position(|&(s0, s1)| s1 > s0 && px >= x(s0) - 2.0 && px <= x(s1) + 2.0)
    };
    let drag_id = resp.id.with("seg");
    let hover = resp.hover_pos().and_then(|p| seg_at(p.x));
    if hover.is_some() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if resp.drag_started() {
        let seg = resp.interact_pointer_pos().and_then(|p| seg_at(p.x));
        ui.data_mut(|d| d.insert_temp(drag_id, seg));
        ed.started |= seg.is_some();
    }
    let seg: Option<usize> = ui.data(|d| d.get_temp(drag_id)).flatten();
    if resp.dragged()
        && let Some(i) = seg
    {
        // Up bows the line up: a rising segment goes fast first (+),
        // a falling one slow first (−).
        let dy = resp.drag_delta().y as f64 / 60.0;
        let c = curve_mut(e, i);
        *c = (*c + if i == 0 { -dy } else { dy }).clamp(-1.0, 1.0);
        ed.changed = true;
    }
    if resp.double_clicked()
        && let Some(i) = hover
        && *curve_mut(e, i) != 0.0
    {
        *curve_mut(e, i) = 0.0;
        ed.started = true;
        ed.changed = true;
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
    }
    // The line, each bendable segment sampled through its curve.
    let mut pts = vec![pos2(x(0.0), y(0.0)), pos2(x(a0), y(0.0))];
    let steps = 24;
    let level = |e: &Adsr, at: f64| e.value(at, d0 + e.decay + 1e9);
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
        let p = k as f64 / steps as f64;
        pts.push(pos2(
            x(r0 + e.release * p),
            y(e.sustain * (1.0 - keyframexl::sampler::bend(p, e.release_curve))),
        ));
    }
    if let Some(i) = seg.or(hover) {
        let (s0, s1) = spans[i];
        painter.rect_filled(
            Rect::from_min_max(pos2(x(s0), r.top()), pos2(x(s1), r.bottom())),
            2.0,
            color.gamma_multiply(0.08),
        );
    }
    painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, color)));
    kit::caps(
        &painter,
        r.right_top() + vec2(-2.0, 2.0),
        Align2::RIGHT_TOP,
        &secs(total),
        10.0,
        t::FAINT,
    );
    let bent = [e.attack_curve, e.decay_curve, e.release_curve]
        .iter()
        .any(|c| *c != 0.0);
    resp.on_hover_text(if bent {
        "Drag a segment up or down to bend it · double-click straightens it"
    } else {
        "Drag a segment up or down to bend it"
    });
}

/// Attack, decay, sustain, release knobs under an envelope view.
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
            "Fall time to the sustain level",
        );
        ed.knob(
            ui,
            "SUSTAIN",
            color,
            &mut e.sustain,
            (0.0, 1.0, 1.0),
            false,
            &pct,
            "Level while the key is held",
        );
        ed.knob(
            ui,
            "RELEASE",
            color,
            &mut e.release,
            (0.0, 20.0, 0.015),
            true,
            &secs,
            "Fade after the key is released",
        );
    });
    // Log knobs never reach zero exactly: the bottom of the travel is zero.
    for v in [&mut e.attack, &mut e.decay, &mut e.release] {
        if *v < 0.0011 {
            *v = 0.0;
        }
    }
}

impl App {
    pub(super) fn synth_editor(&mut self, ui: &mut Ui, id: u64, patch: &SynthPatch) {
        let mut p = patch.clone();
        let mut ed = Edit::default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            self.synth_header(ui, id, &mut p, &mut ed);
            pair(ui, |ui, col| {
                if col == 0 {
                    panel(ui, "osc a", t::ENC_BLUE, |ui| {
                        wave_view(ui, p.table, p.position);
                        ed.chips(ui, &format!("tide-table-{id}"), &TABLES, &TABLES.map(table_name), &mut p.table);
                        ui.horizontal_wrapped(|ui| {
                            let c = t::ENC_BLUE;
                            ed.knob(ui, "POSITION", c, &mut p.position, (0.0, 1.0, 0.0), false, &pct, "Where in the table the oscillator reads: morphs the wave");
                            ed.knob(ui, "WARP", c, &mut p.warp_amount, (-1.0, 1.0, 0.0), false, &signed, "How hard the warp mode bends the wave");
                            ed.knob(ui, "SUB", c, &mut p.sub, (0.0, 1.0, 0.0), false, &pct, "Sine one octave down");
                            ed.knob(ui, "NOISE", c, &mut p.noise, (0.0, 1.0, 0.0), false, &pct, "White noise level");
                        });
                        tuning(ui, &mut ed, &mut p.level, &mut p.coarse, &mut p.fine, 1.0);
                        mode_row(ui, &mut ed, &format!("tide-warp-{id}"), &mut p.warp);
                    });
                    panel(ui, "osc b", t::ENC_BLUE, |ui| {
                        let b = &mut p.osc_b;
                        wave_view(ui, b.table, b.position);
                        ed.chips(ui, &format!("tide-table-b-{id}"), &TABLES, &TABLES.map(table_name), &mut b.table);
                        ui.horizontal_wrapped(|ui| {
                            let c = t::ENC_BLUE;
                            ed.knob(ui, "POSITION", c, &mut b.position, (0.0, 1.0, 0.0), false, &pct, "Where in the table the oscillator reads: morphs the wave");
                            ed.knob(ui, "WARP", c, &mut b.warp_amount, (-1.0, 1.0, 0.0), false, &signed, "How hard the warp mode bends the wave");
                        });
                        tuning(ui, &mut ed, &mut b.level, &mut b.coarse, &mut b.fine, 0.0);
                        mode_row(ui, &mut ed, &format!("tide-warp-b-{id}"), &mut b.warp);
                    });
                    panel(ui, "osc c · spectral", t::ENC_BLUE, |ui| spectral_panel(ui, &mut ed, &mut p.spectral));
                } else {
                    panel(ui, "filter", t::ENC_ORANGE, |ui| {
                        filter_view(ui, &p);
                        ed.chips(ui, &format!("tide-filter-{id}"), &FILTERS, &["Low pass", "High pass", "Band pass"], &mut p.filter);
                        ui.horizontal_wrapped(|ui| {
                            let c = t::ENC_ORANGE;
                            ed.knob(ui, "CUTOFF", c, &mut p.cutoff, (20.0, 20000.0, 18000.0), true, &hz, "Cutoff frequency");
                            ed.knob(ui, "RESO", c, &mut p.resonance, (0.0, 1.0, 0.0), false, &pct, "Resonance at the cutoff");
                            ed.knob(ui, "KEY", c, &mut p.keytrack, (-1.0, 1.0, 0.0), false, &signed, "How far the cutoff follows the note pitch");
                            ed.knob(ui, "ENV", c, &mut p.env_amount, (-1.0, 1.0, 0.0), false, &signed, "How far the filter envelope moves the cutoff");
                        });
                    });
                    panel(ui, "filter envelope", t::ENC_ORANGE, |ui| {
                        envelope_view(ui, &mut ed, &mut p.filter_env, t::ENC_ORANGE);
                        envelope_knobs(ui, &mut ed, &mut p.filter_env, t::ENC_ORANGE);
                    });
                    lfo_panel(ui, &mut ed, id, &mut p.lfos);
                    panel(ui, "voices, glide and bend", t::ENC_WHITE, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let c = t::ENC_WHITE;
                            let mut voices = p.unison as f64;
                            ed.knob(ui, "VOICES", c, &mut voices, (1.0, 7.0, 1.0), false, &|v| format!("{:.0}", v.round()), "Stacked copies of the oscillators per note");
                            p.unison = voices.round().clamp(1.0, 7.0) as u8;
                            ed.knob(ui, "DETUNE", c, &mut p.detune, (0.0, 1.0, 0.0), false, &pct, "Pitch spread between the unison voices");
                            ed.knob(ui, "SPREAD", c, &mut p.spread, (0.0, 1.0, 0.0), false, &pct, "Stereo width of the unison voices");
                            ed.knob(ui, "GLIDE", c, &mut p.glide, (0.0, 10.0, 0.0), true, &|v| if v < 0.0011 { "off".into() } else { secs(v) }, "Slide time from one note to the next");
                            ed.knob(ui, "BEND", c, &mut p.bend_range, (0.0, 48.0, 2.0), false, &|v| format!("±{:.0} st", v.round()), "How far the MIDI pitch wheel bends, in semitones");
                            p.bend_range = p.bend_range.round();
                        });
                        labelled_switch(ui, &mut ed, "MPE", &mut p.mpe, "MPE controllers: channel 1 bends every note, channels 2-16 each bend (±48 st), press and slide their own note");
                    });
                }
            });
            panel(ui, "amp envelope", t::ENC_GREEN, |ui| {
                pair(ui, |ui, col| if col == 0 { envelope_view(ui, &mut ed, &mut p.amp_env, t::ENC_GREEN) } else { envelope_knobs(ui, &mut ed, &mut p.amp_env, t::ENC_GREEN) });
            });
            self.synth_macros(ui, id, &mut p, &mut ed);
            mod_panel(ui, &mut ed, id, &mut p);
            fx_panel(ui, &mut ed, id, &mut p);
        });
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

    /// Preset stepper and list (factory, then the saved ones), Save and
    /// Load; "edited" once the patch leaves the preset it came from.
    fn synth_header(&mut self, ui: &mut Ui, id: u64, p: &mut SynthPatch, ed: &mut Edit) {
        let key = egui::Id::new(("tide-preset", id));
        let user = super::presets_ui::cached_synth_presets(ui.ctx());
        // Factory presets then saved ones, as one list the arrows step through.
        let names: Vec<&str> = SynthPatch::PRESETS
            .iter()
            .copied()
            .chain(user.iter().map(|(_, u)| u.name.as_str()))
            .collect();
        let patch_of = |i: usize| {
            if i < SynthPatch::PRESETS.len() {
                SynthPatch::factory(i)
            } else {
                user[i - SynthPatch::PRESETS.len()].1.patch.clone()
            }
        };
        let n = names.len();
        // The preset the patch still is (a reopened project), else the last picked.
        let now = (0..n).find(|&i| *p == patch_of(i));
        let mut idx = now
            .or_else(|| ui.ctx().data(|d| d.get_temp::<usize>(key)))
            .unwrap_or(0)
            .min(n - 1);
        let mut pick = None;
        let mut save = false;
        let mut load = false;
        ui.horizontal(|ui| {
            if ui.button("◀").on_hover_text("Previous preset").clicked() {
                pick = Some((idx + n - 1) % n);
            }
            let shown = if now.is_some() { names[idx].to_string() } else { format!("{} · edited", names[idx]) };
            egui::ComboBox::from_id_salt(("tide-presets", id)).width(190.0).selected_text(shown).show_ui(ui, |ui| {
                kit::menu_style(ui);
                for (i, name) in names.iter().enumerate() {
                    if i == SynthPatch::PRESETS.len() {
                        ui.separator();
                        ui.label(egui::RichText::new("Saved").font(t::sans(11.5)).color(t::FAINT));
                    }
                    if ui.selectable_label(Some(i) == now, *name).clicked() {
                        pick = Some(i);
                    }
                }
            });
            if ui.button("▶").on_hover_text("Next preset").clicked() {
                pick = Some((idx + 1) % n);
            }
            save = ui.button("Save…").on_hover_text("Save this patch as a preset (Presets/Synth): it joins this list and the browser").clicked();
            load = ui.button("Load…").on_hover_text("Open a saved .kfxsynth preset").clicked();
        });
        if let Some(i) = pick {
            idx = i;
            *p = patch_of(i);
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
            // The editor's copy is what the channel will hold: store it first.
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
                .iter()
                .find(|i| i.id == id)
                .map(|i| &i.kind)
            {
                *p = patch.clone();
            }
        }
    }

    /// Four macros: a knob each, and up to four routes (target and depth).
    fn synth_macros(&mut self, ui: &mut Ui, id: u64, p: &mut SynthPatch, ed: &mut Edit) {
        panel(ui, "macros", t::ENC_WHITE, |ui| {
            let cols = if ui.available_width() >= TWO_COLUMNS {
                4
            } else {
                2
            };
            let width = (ui.available_width() - 12.0 * (cols as f32 - 1.0)) / cols as f32;
            for row in p.macros.chunks_mut(cols).enumerate() {
                let (r, macros) = row;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for (k, m) in macros.iter_mut().enumerate() {
                        let i = r * cols + k;
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ed.knob(ui, &format!("MACRO {}", i + 1), t::ENC_WHITE, &mut m.value, (0.0, 1.0, 0.0), false, &pct, "Moves every route below by its depth");
                            // Routes in use (depth not 0), then one free slot to add a target.
                            let free = m.routes.iter().position(|r| r.depth.abs() <= 0.001);
                            for (j, route) in m.routes.iter_mut().enumerate() {
                                if route.depth.abs() <= 0.001 {
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    egui::ComboBox::from_id_salt(("tide-route", id, i, j)).width(width - 34.0).selected_text(param_name(route.target)).show_ui(ui, |ui| {
                                        kit::menu_style(ui);
                                        for v in PARAMS {
                                            if ui.selectable_value(&mut route.target, v, param_name(v)).changed() {
                                                ed.started = true;
                                                ed.changed = true;
                                            }
                                        }
                                    });
                                    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                                    let k = kit::Knob { label: "DEPTH", color: t::ENC_WHITE, min: -1.0, max: 1.0, default: 0.0, log: false, fmt: &signed, hint: "How far the macro moves this target. Double-click: 0 removes the route" };
                                    let (_, s, c) = kit::mini_knob(ui, rect, ui.id().with(("tide-depth", i, j)), &k, &mut route.depth);
                                    ed.started |= s;
                                    ed.changed |= c;
                                });
                            }
                            if let Some(j) = free {
                                let add = egui::RichText::new("+ Add target").font(t::sans(12.0)).color(t::FAINT);
                                egui::ComboBox::from_id_salt(("tide-add", id, i)).width(width - 34.0).selected_text(add).show_ui(ui, |ui| {
                                    kit::menu_style(ui);
                                    for v in PARAMS {
                                        if ui.selectable_label(false, param_name(v)).clicked() {
                                            m.routes[j] = keyframexl::synth::Route { target: v, depth: 0.5 };
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
}
