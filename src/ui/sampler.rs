//! Sampler editor (our DirectWave): Program, Zone, Filter & envelopes,
//! Modulation (LFOs + 16-slot matrix), Zones. Names follow DirectWave; see
//! research_notes/directwave-params.md. Edits go to the selected zone or to
//! all zones (DirectWave "Edit: Current / Global").

use super::{App, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::sampler::{
    Adsr, FilterKind, FilterRouting, GlideMode, LfoShape, LoopMode, MOD_SLOTS, ModSource,
    ModTarget, PlayMode, SamplePreset, Zone,
};
use keyframexl::sequence::InstrumentKind;
use std::sync::Arc;

const PAGES: [&str; 5] = ["Program", "Zone", "Filter & Env", "Modulation", "Zones"];

/// Per-window editor state: page and zone selection (None = all zones).
#[derive(Clone, Copy, Default)]
pub(crate) struct SamplerUi {
    pub page: usize,
    pub zone: Option<usize>,
}

/// DirectWave Loop Type, mapped onto loop mode + one-shot.
const LOOP_TYPES: [&str; 5] = ["Disabled", "Forward", "One Shot", "Sustained", "Bounce"];
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
        // No loop points in the file: loop the whole sample.
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
    if v < 1.0 {
        format!("{:.0} ms", v * 1000.0)
    } else {
        format!("{v:.2} s")
    }
}
fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
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

/// A knob spec with a log mapping for frequencies and times.
struct P<'a> {
    label: &'a str,
    min: f64,
    max: f64,
    default: f64,
    fmt: &'a dyn Fn(f64) -> String,
    hint: &'a str,
    /// Drag on a log scale (times, frequencies, rates).
    log: bool,
}

impl App {
    pub(super) fn sampler_ui(&mut self, id: u64) -> &mut SamplerUi {
        self.sampler_view.entry(id).or_default()
    }

    /// Change the preset of sampler `id`. One undo step per gesture: the caller
    /// passes `started` from the knob.
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
            // Params only: knobs cannot touch samples, and the per-sample
            // scan is linear in the audio data (jank on every drag tick).
            if p.validate_params().is_err() {
                self.say(Tone::Bad, "That value is out of range; it was not applied.");
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

    /// One knob bound to a value; returns Some(new) when it moved, plus gesture start.
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
        let st = *self.sampler_ui(id);
        // Zone map: click a zone to edit only it; click empty space for all zones.
        self.zone_map(ui, id, preset, st.zone);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (pick, _) = kit::chips(ui, &format!("smp-page-{id}"), &PAGES, st.page, None);
            if let Some(p) = pick {
                self.sampler_ui(id).page = p;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (pick, r) = kit::chips(ui, &format!("smp-scope-{id}"), &["Current zone", "All zones"], st.zone.is_none() as usize, None);
                match pick {
                    Some(0) if st.zone.is_none() => self.sampler_ui(id).zone = Some(0),
                    Some(1) => self.sampler_ui(id).zone = None,
                    _ => {}
                }
                ui.interact(r, ui.id().with("scope-tip"), Sense::hover()).on_hover_text(match st.zone {
                    Some(i) => format!("Edits change zone {:02} only. Click a zone on the map to pick another.", i + 1),
                    None => format!("Edits change all {} zones at once (DirectWave Global).", preset.zones.len()),
                });
            });
        });
        ui.add_space(10.0);
        let zi = st.zone.unwrap_or(0).min(preset.zones.len() - 1);
        let z = preset.zones[zi].clone();
        match st.page {
            0 => self.page_program(ui, id, preset),
            1 => self.page_zone(ui, id, &z),
            2 => self.page_filter(ui, id, &z),
            3 => self.page_modulation(ui, id, &z),
            _ => self.page_zones(ui, preset),
        }
    }

    fn zone_map(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset, selected: Option<usize>) {
        let p = ui.painter().clone();
        let (cap, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        kit::caps(
            &p,
            cap.left_center(),
            Align2::LEFT_CENTER,
            &format!(
                "{} zone(s)  ·  keys × velocity  ·  click a zone to edit only it",
                preset.zones.len()
            ),
            11.0,
            t::FAINT,
        );
        let map_h = 120.0;
        let (area, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), map_h + 22.0), Sense::click());
        let well = kit::sunken(
            &p,
            Rect::from_min_size(area.min, vec2(area.width(), map_h)),
            8.0,
        )
        .shrink(6.0);
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
        let keys = (hi - lo + 1) as f32;
        let kx = |k: u8| well.left() + (k.saturating_sub(lo)) as f32 / keys * well.width();
        let vy = |v: u8| well.bottom() - v as f32 / 127.0 * well.height();
        for v in [32u8, 64, 96] {
            p.line_segment(
                [pos2(well.left(), vy(v)), pos2(well.right(), vy(v))],
                Stroke::new(1.0_f32, t::LINE.gamma_multiply(0.5)),
            );
        }
        for k in (lo..=hi).filter(|k| k % 12 == 0) {
            p.line_segment(
                [pos2(kx(k), well.top()), pos2(kx(k), well.bottom())],
                Stroke::new(1.0_f32, t::LINE),
            );
            p.text(
                pos2(kx(k) + 3.0, area.top() + map_h + 4.0),
                Align2::LEFT_TOP,
                super::roll::note_name(k),
                t::mono(12.0),
                t::MUTED,
            );
        }
        let pointer = ui.ctx().pointer_hover_pos();
        let mut hovered = None;
        for (i, z) in preset.zones.iter().enumerate() {
            let code = t::PATTERN_CODES[i % t::PATTERN_CODES.len()];
            let r = Rect::from_min_max(
                pos2(kx(z.low), vy(z.high_velocity)),
                pos2(kx(z.high) + well.width() / keys, vy(z.low_velocity)),
            )
            .shrink(1.0);
            let on = selected.is_none_or(|s| s == i);
            p.rect_filled(r, 3.0, code.gamma_multiply(if on { 0.22 } else { 0.06 }));
            p.rect_stroke(
                r,
                3.0,
                Stroke::new(
                    if selected == Some(i) { 2.5_f32 } else { 1.5 },
                    if on { code } else { code.gamma_multiply(0.4) },
                ),
                egui::StrokeKind::Inside,
            );
            p.text(
                r.left_top() + vec2(6.0, 4.0),
                Align2::LEFT_TOP,
                format!("{:02}", i + 1),
                t::mono(12.0),
                if on { t::TEXT } else { t::FAINT },
            );
            // Root marker.
            let rx = kx(z.root) + well.width() / keys / 2.0;
            if r.x_range().contains(rx) {
                p.line_segment(
                    [pos2(rx, r.bottom() - 6.0), pos2(rx, r.bottom())],
                    Stroke::new(2.0_f32, code),
                );
            }
            if pointer.is_some_and(|q| r.contains(q)) {
                hovered = Some(i);
            }
        }
        if let Some(i) = hovered {
            let z = &preset.zones[i];
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                egui::Id::new(("zone-tip", id)),
                |ui| {
                    ui.label(format!(
                    "Zone {:02}: {}\nKeys {} – {}, root {}\nVelocity {} – {}\nClick to edit only this zone",
                    i + 1,
                    super::instrument::file_name(&z.name),
                    super::roll::note_name(z.low),
                    super::roll::note_name(z.high),
                    super::roll::note_name(z.root),
                    z.low_velocity,
                    z.high_velocity
                ));
                },
            );
        }
        if resp.clicked() {
            self.sampler_ui(id).zone = hovered;
        }
    }

    fn section(ui: &mut Ui, title: &str, color: Color32) {
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
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
    }

    fn page_program(&mut self, ui: &mut Ui, id: u64, preset: &SamplePreset) {
        let pr = preset.program.clone();
        Self::section(ui, "program", t::ENC_WHITE);
        ui.horizontal_wrapped(|ui| {
            let c = t::ENC_WHITE;
            let specs: [(P, f64); 4] = [
                (
                    P {
                        label: "Volume",
                        min: -60.0,
                        max: 12.0,
                        default: 0.0,
                        fmt: &db,
                        hint: "Program volume",
                        log: false,
                    },
                    pr.volume_db,
                ),
                (
                    P {
                        label: "Pan",
                        min: -1.0,
                        max: 1.0,
                        default: 0.0,
                        fmt: &pan,
                        hint: "Added to every zone's panning",
                        log: false,
                    },
                    pr.pan,
                ),
                (
                    P {
                        label: "Trans",
                        min: -48.0,
                        max: 48.0,
                        default: 0.0,
                        fmt: &|v| format!("{:+.0} st", v.round()),
                        hint: "Transpose in semitones",
                        log: false,
                    },
                    pr.transpose as f64,
                ),
                (
                    P {
                        label: "PB rng",
                        min: 0.0,
                        max: 4800.0,
                        default: 200.0,
                        fmt: &|v| format!("{:.0} ct", v),
                        hint: "Pitch bend range for MIDI input (cents)",
                        log: false,
                    },
                    pr.bend_range,
                ),
            ];
            for (k, (spec, v)) in specs.iter().enumerate() {
                let new = self.pknob(ui, spec, c, *v);
                if let Some(v) = new {
                    self.edit_preset(id, false, |p| match k {
                        0 => p.program.volume_db = v,
                        1 => p.program.pan = v,
                        2 => p.program.transpose = v.round() as i8,
                        _ => p.program.bend_range = v.round(),
                    });
                }
            }
        });
        Self::section(ui, "play mode and glide", t::ENC_WHITE);
        ui.horizontal(|ui| {
            let modes: Vec<&str> = PlayMode::ALL.iter().map(|m| m.label()).collect();
            let cur = PlayMode::ALL
                .iter()
                .position(|m| *m == pr.play_mode)
                .unwrap_or(0);
            if let (Some(i), _) = kit::chips(ui, &format!("pm-{id}"), &modes, cur, None) {
                self.edit_preset(id, true, |p| p.program.play_mode = PlayMode::ALL[i]);
            }
            ui.add_space(8.0);
            let new = self.pknob(
                ui,
                &P {
                    label: "Poly",
                    min: 1.0,
                    max: 64.0,
                    default: 32.0,
                    fmt: &|v| format!("{:.0}", v.round()),
                    hint: "Voices in Poly mode",
                    log: false,
                },
                t::ENC_WHITE,
                pr.polyphony as f64,
            );
            if let Some(v) = new {
                self.edit_preset(id, false, |p| p.program.polyphony = v.round() as u8);
            }
            let new = self.pknob(
                ui,
                &P {
                    label: "Glide",
                    min: 0.0,
                    max: 10.0,
                    default: 0.0,
                    fmt: &|v| if v <= 0.0 { "off".into() } else { secs(v) },
                    hint: "Portamento time between notes",
                    log: true,
                },
                t::ENC_WHITE,
                pr.glide,
            );
            if let Some(v) = new {
                self.edit_preset(id, false, |p| {
                    p.program.glide = if v < 0.0011 { 0.0 } else { v }
                });
            }
            let gm: Vec<&str> = GlideMode::ALL.iter().map(|m| m.label()).collect();
            let cur = GlideMode::ALL
                .iter()
                .position(|m| *m == pr.glide_mode)
                .unwrap_or(0);
            if let (Some(i), r) = kit::chips(ui, &format!("gm-{id}"), &gm, cur, None) {
                let _ = r;
                self.edit_preset(id, true, |p| p.program.glide_mode = GlideMode::ALL[i]);
            }
        });
        ui.label(egui::RichText::new("Poly plays chords. Mono plays one note and retriggers. Legato glides between overlapping notes without retriggering. Glide Fixed always slides; Auto only on overlapping notes.").color(t::FAINT).size(12.0));
        if !preset.report.is_empty() {
            Self::section(ui, "import report", t::FAINT);
            ui.label(
                egui::RichText::new(&preset.report)
                    .color(t::MUTED)
                    .size(13.0),
            );
        }
    }

    fn page_zone(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
        ui.set_width(360.0);
        Self::section(ui, "pitch", t::WAIT);
        ui.horizontal_wrapped(|ui| {
            let semi = (z.tune_cents / 100.0).round();
            let fine = z.tune_cents - semi * 100.0;
            let rows: [(P, f64); 3] = [
                (P { label: "Semi", min: -36.0, max: 36.0, default: 0.0, fmt: &|v| format!("{:+.0}", v.round()), hint: "Transpose in semitones (SEMI)", log: false }, semi),
                (P { label: "Fine", min: -50.0, max: 50.0, default: 0.0, fmt: &|v| format!("{v:+.0} ct"), hint: "Fine tune in cents (FINE)", log: false }, fine),
                (P { label: "KTrk", min: -1.0, max: 1.0, default: 1.0, fmt: &|v| format!("{:+.0}", v * 100.0), hint: "Keyboard track: 100 = a semitone per key, 0 = every key plays the root", log: false }, z.params.key_track),
            ];
            for (k, (spec, v)) in rows.iter().enumerate() {
                let new = self.pknob(ui, spec, t::WAIT, *v);
                if let Some(v) = new {
                    self.edit_zones(id, false, |z| {
                        let semi = (z.tune_cents / 100.0).round();
                        let fine = z.tune_cents - semi * 100.0;
                        match k {
                            0 => z.tune_cents = v.round() * 100.0 + fine,
                            1 => z.tune_cents = semi * 100.0 + v,
                            _ => z.params.key_track = v,
                        }
                    });
                }
            }
        });
        });
        ui.vertical(|ui| {
        ui.set_width(360.0);
        Self::section(ui, "amp", t::ENC_BLUE);
        ui.horizontal_wrapped(|ui| {
            let gain_db = 20.0 * z.gain.max(1e-6).log10();
            let rows: [(P, f64); 3] = [
                (P { label: "Gain", min: -60.0, max: 24.0, default: 0.0, fmt: &db, hint: "Zone gain", log: false }, gain_db.max(-60.0)),
                (P { label: "Pan", min: -1.0, max: 1.0, default: 0.0, fmt: &pan, hint: "Zone panning", log: false }, z.pan),
                (P { label: "Vel", min: 0.0, max: 1.0, default: 1.0, fmt: &pct, hint: "How much velocity changes the level", log: false }, z.velocity_tracking),
            ];
            for (k, (spec, v)) in rows.iter().enumerate() {
                let new = self.pknob(ui, spec, t::ENC_BLUE, *v);
                if let Some(v) = new {
                    self.edit_zones(id, false, |z| match k {
                        0 => z.gain = 10f64.powf(v / 20.0).min(16.0),
                        1 => z.pan = v,
                        _ => z.velocity_tracking = v,
                    });
                }
            }
        });
        });
        });
        let env = z.adsr.clone();
        if let Some((env, _)) =
            self.envelope(ui, &format!("amp-{id}"), "amp envelope", t::ENC_BLUE, &env)
        {
            self.edit_zones(id, false, |z| z.adsr = env.clone());
        }
        Self::section(ui, "sample", t::ENC_GREEN);
        ui.horizontal(|ui| {
            if let (Some(i), _) =
                kit::chips(ui, &format!("lt-{id}"), &LOOP_TYPES, loop_type(z), None)
            {
                self.edit_zones(id, true, |z| set_loop_type(z, i));
            }
            ui.add_space(8.0);
            let (pick, _) = kit::chips(
                ui,
                &format!("rev-{id}"),
                &["Forward", "Reverse"],
                z.params.reverse as usize,
                None,
            );
            if let Some(i) = pick {
                self.edit_zones(id, true, |z| z.params.reverse = i == 1);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let len = (z.end - z.offset.min(z.end)) as f64;
            let start = z.offset as f64 / z.frames.len().max(1) as f64;
            let new = self.pknob(
                ui,
                &P {
                    label: "Start",
                    min: 0.0,
                    max: 0.99,
                    default: 0.0,
                    fmt: &pct,
                    hint: "Sample start (red marker in DirectWave)",
                    log: false,
                },
                t::ENC_GREEN,
                start,
            );
            if let Some(v) = new {
                self.edit_zones(id, false, |z| {
                    let o = (v * z.frames.len() as f64) as usize;
                    z.offset = o.min(z.end.saturating_sub(2));
                    if z.loop_mode != LoopMode::None && z.loop_start < z.offset {
                        z.loop_start = z.offset;
                    }
                });
            }
            let new = self.pknob(
                ui,
                &P {
                    label: "Group",
                    min: 0.0,
                    max: 16.0,
                    default: 0.0,
                    fmt: &|v| {
                        if v < 0.5 {
                            "off".into()
                        } else {
                            format!("{:.0}", v.round())
                        }
                    },
                    hint: "Mute group: a zone in the same group cuts this one (open/closed hat)",
                    log: false,
                },
                t::ENC_GREEN,
                z.params.mute_group as f64,
            );
            if let Some(v) = new {
                self.edit_zones(id, false, |z| z.params.mute_group = v.round() as u8);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{:.0} frames · {} Hz · loop {}",
                    len,
                    z.sample_rate,
                    if z.loop_mode == LoopMode::None {
                        "off".into()
                    } else {
                        format!("{}..{}", z.loop_start, z.loop_end)
                    }
                ))
                .color(t::FAINT)
                .size(12.0),
            );
        });
    }

    fn page_filter(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        for fi in 0..2 {
            Self::section(ui, &format!("filter {}", fi + 1), t::ENC_ORANGE);
            let f = z.params.filters[fi].clone();
            ui.horizontal_wrapped(|ui| {
                let kinds: Vec<&str> = FilterKind::ALL.iter().map(|k| k.label()).collect();
                let cur = FilterKind::ALL
                    .iter()
                    .position(|k| *k == f.kind)
                    .unwrap_or(0);
                if let (Some(i), _) = kit::chips(ui, &format!("fk-{id}-{fi}"), &kinds, cur, None) {
                    self.edit_zones(id, true, |z| z.params.filters[fi].kind = FilterKind::ALL[i]);
                }
                ui.add_space(8.0);
                let new = self.pknob(
                    ui,
                    &P {
                        label: "Freq",
                        min: 20.0,
                        max: 20000.0,
                        default: 20000.0,
                        fmt: &hz,
                        hint: "Cutoff frequency",
                        log: true,
                    },
                    t::ENC_ORANGE,
                    f.cutoff,
                );
                if let Some(v) = new {
                    self.edit_zones(id, false, |z| z.params.filters[fi].cutoff = v);
                }
                let new = self.pknob(
                    ui,
                    &P {
                        label: "Emph",
                        min: 0.0,
                        max: 1.0,
                        default: 0.0,
                        fmt: &pct,
                        hint: "Emphasis: resonance at the cutoff",
                        log: false,
                    },
                    t::ENC_ORANGE,
                    f.emphasis,
                );
                if let Some(v) = new {
                    self.edit_zones(id, false, |z| z.params.filters[fi].emphasis = v);
                }
            });
        }
        ui.horizontal(|ui| {
            let routes: Vec<&str> = FilterRouting::ALL.iter().map(|r| r.label()).collect();
            let cur = FilterRouting::ALL.iter().position(|r| *r == z.params.routing).unwrap_or(0);
            if let (Some(i), _) = kit::chips(ui, &format!("fr-{id}"), &routes, cur, None) {
                self.edit_zones(id, true, |z| z.params.routing = FilterRouting::ALL[i]);
            }
            ui.label(egui::RichText::new("Serial: filter 1 feeds filter 2. Parallel: both hear the sample and are mixed.").color(t::FAINT).size(12.0));
        });
        for ei in 0..2 {
            let env = z.params.env[ei].clone();
            if let Some((env, _)) = self.envelope(
                ui,
                &format!("env{ei}-{id}"),
                &format!("env {} · route it in modulation", ei + 1),
                t::ENC_GREEN,
                &env,
            ) {
                self.edit_zones(id, false, |z| z.params.env[ei] = env.clone());
            }
        }
    }

    fn page_modulation(&mut self, ui: &mut Ui, id: u64, z: &Zone) {
        for li in 0..2 {
            Self::section(ui, &format!("lfo {}", li + 1), t::ENC_GREEN);
            let l = z.params.lfo[li].clone();
            ui.horizontal_wrapped(|ui| {
                let mut shape = l.shape;
                egui::ComboBox::from_id_salt(("lfo-shape", id, li))
                    .selected_text(shape.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        kit::menu_style(ui);
                        for s in LfoShape::ALL {
                            ui.selectable_value(&mut shape, *s, s.label());
                        }
                    });
                if shape != l.shape {
                    self.edit_zones(id, true, |z| z.params.lfo[li].shape = shape);
                }
                let rows: [(P, f64); 3] = [
                    (
                        P {
                            label: "Rate",
                            min: 0.01,
                            max: 50.0,
                            default: 5.0,
                            fmt: &|v| format!("{v:.2} Hz"),
                            hint: "LFO speed",
                            log: true,
                        },
                        l.rate,
                    ),
                    (
                        P {
                            label: "Attack",
                            min: 0.0,
                            max: 10.0,
                            default: 0.0,
                            fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                            hint: "Time for the modulation to fade in",
                            log: false,
                        },
                        l.attack,
                    ),
                    (
                        P {
                            label: "Phase",
                            min: 0.0,
                            max: 1.0,
                            default: 0.0,
                            fmt: &|v| format!("{:.0}°", v * 360.0),
                            hint: "Start phase",
                            log: false,
                        },
                        l.phase,
                    ),
                ];
                for (k, (spec, v)) in rows.iter().enumerate() {
                    let new = self.pknob(ui, spec, t::ENC_GREEN, *v);
                    if let Some(v) = new {
                        self.edit_zones(id, false, |z| match k {
                            0 => z.params.lfo[li].rate = v,
                            1 => z.params.lfo[li].attack = v,
                            _ => z.params.lfo[li].phase = v,
                        });
                    }
                }
                if let (Some(i), _) = kit::chips(
                    ui,
                    &format!("fr-{id}-{li}"),
                    &["Sync", "Free Run"],
                    l.free_run as usize,
                    None,
                ) {
                    self.edit_zones(id, true, |z| z.params.lfo[li].free_run = i == 1);
                }
            });
        }
        Self::section(ui, "modulation matrix", t::ENC_GREEN);
        ui.label(egui::RichText::new("Source → target × amount. 100% means: Pitch ±12 st, Gain ±100%, Pan full, Filter freq ±6 oct, Emphasis full, LFO rate ±3 oct.").color(t::FAINT).size(12.0));
        let used = z
            .params
            .matrix
            .iter()
            .filter(|m| m.source != ModSource::None)
            .count();
        let shown = (used + 1).min(MOD_SLOTS);
        egui::Grid::new(("matrix", id))
            .num_columns(4)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for (h, w) in [
                    ("#", 20.0),
                    ("Source", 120.0),
                    ("Target", 140.0),
                    ("Amount", 220.0),
                ] {
                    let (r, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
                    kit::caps(
                        ui.painter(),
                        r.left_center(),
                        Align2::LEFT_CENTER,
                        h,
                        10.0,
                        t::FAINT,
                    );
                }
                ui.end_row();
                for si in 0..shown {
                    let m = z.params.matrix[si];
                    ui.label(
                        egui::RichText::new(format!("{:02}", si + 1))
                            .monospace()
                            .color(t::MUTED),
                    );
                    let mut src = m.source;
                    egui::ComboBox::from_id_salt(("msrc", id, si))
                        .selected_text(src.label())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            for s in ModSource::ALL {
                                ui.selectable_value(&mut src, *s, s.label());
                            }
                        });
                    let mut tgt = m.target;
                    egui::ComboBox::from_id_salt(("mtgt", id, si))
                        .selected_text(tgt.label())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            for s in ModTarget::ALL {
                                ui.selectable_value(&mut tgt, *s, s.label());
                            }
                        });
                    if src != m.source || tgt != m.target {
                        self.edit_zones(id, true, |z| {
                            z.params.matrix[si].source = src;
                            z.params.matrix[si].target = tgt;
                        });
                    }
                    let mut a = m.amount as f64;
                    let mut edit = (false, false);
                    ui.add_enabled_ui(src != ModSource::None, |ui| {
                        let spec = kit::Knob {
                            label: "Amount",
                            color: t::TEXT,
                            min: -1.0,
                            max: 1.0,
                            default: 0.0,
                            log: false,
                            fmt: &|v| format!("{:+.0}%", v * 100.0),
                            hint: "Modulation amount",
                        };
                        let (_, started, changed) = kit::knob(ui, &spec, &mut a);
                        edit = (started, changed);
                    });
                    if edit.0 {
                        self.checkpoint();
                    }
                    if edit.1 {
                        self.edit_zones(id, false, |z| z.params.matrix[si].amount = a);
                    }
                    ui.end_row();
                }
            });
    }

    fn page_zones(&mut self, ui: &mut Ui, preset: &SamplePreset) {
        let p = ui.painter().clone();
        let cols = [
            ("Zone", 0.0),
            ("Sample", 0.06),
            ("Keys", 0.36),
            ("Root", 0.50),
            ("Velocity", 0.58),
            ("Loop", 0.70),
            ("A / D / S / R", 0.82),
        ];
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let w = head.width();
        for (h, f) in cols {
            kit::caps(
                &p,
                pos2(head.left() + w * f, head.center().y),
                Align2::LEFT_CENTER,
                h,
                11.0,
                t::FAINT,
            );
        }
        for (i, z) in preset.zones.iter().enumerate() {
            let (row, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            p.line_segment(
                [row.left_top(), row.right_top()],
                Stroke::new(1.0_f32, t::LINE),
            );
            let cells = [
                format!("{:02}", i + 1),
                super::instrument::file_name(&z.name),
                format!(
                    "{} – {}",
                    super::roll::note_name(z.low),
                    super::roll::note_name(z.high)
                ),
                super::roll::note_name(z.root),
                format!("{} – {}", z.low_velocity, z.high_velocity),
                LOOP_TYPES[loop_type(z)].to_string(),
                format!(
                    "{} / {} / {:.0}% / {}",
                    secs(z.adsr.attack),
                    secs(z.adsr.decay),
                    z.adsr.sustain * 100.0,
                    secs(z.adsr.release)
                ),
            ];
            for (k, (_, f)) in cols.iter().enumerate() {
                let font = if k == 1 { t::sans(13.0) } else { t::mono(12.0) };
                let max = w * (cols.get(k + 1).map(|c| c.1).unwrap_or(1.0) - f) - 8.0;
                let shown = kit::elide(&p, &cells[k], font.clone(), max);
                p.text(
                    pos2(row.left() + w * f, row.center().y),
                    Align2::LEFT_CENTER,
                    shown,
                    font,
                    t::TEXT,
                );
            }
        }
    }

    /// DAHDSR knobs with a drawn curve. Returns the new envelope and gesture start.
    fn envelope(
        &mut self,
        ui: &mut Ui,
        key: &str,
        title: &str,
        color: Color32,
        env: &Adsr,
    ) -> Option<(Adsr, bool)> {
        Self::section(ui, title, color);
        let mut out = None;
        ui.horizontal(|ui| {
            // Curve.
            let (r, _) = ui.allocate_exact_size(vec2(170.0, 64.0), Sense::hover());
            let p = ui.painter();
            let well = kit::sunken(p, r, 6.0).shrink(6.0);
            let total = env.delay + env.attack + env.hold + env.decay + env.release;
            let sus_w = (total * 0.3).max(0.05);
            let span = total + sus_w;
            let x = |s: f64| well.left() + (s / span) as f32 * well.width();
            let y = |l: f64| well.bottom() - l as f32 * well.height();
            let mut tt = 0.0;
            let mut pts = vec![pos2(x(0.0), y(0.0))];
            tt += env.delay;
            pts.push(pos2(x(tt), y(0.0)));
            tt += env.attack;
            pts.push(pos2(x(tt), y(1.0)));
            tt += env.hold;
            pts.push(pos2(x(tt), y(1.0)));
            tt += env.decay;
            pts.push(pos2(x(tt), y(env.sustain)));
            tt += sus_w;
            pts.push(pos2(x(tt), y(env.sustain)));
            tt += env.release;
            pts.push(pos2(x(tt), y(0.0)));
            p.add(egui::Shape::line(pts, Stroke::new(1.5_f32, color)));
            let _ = key;
            let rows: [(P, f64); 6] = [
                (
                    P {
                        label: "Delay",
                        min: 0.0,
                        max: 10.0,
                        default: 0.0,
                        fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                        hint: "Time before the attack starts",
                        log: true,
                    },
                    env.delay,
                ),
                (
                    P {
                        label: "Attack",
                        min: 0.0,
                        max: 10.0,
                        default: 0.005,
                        fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                        hint: "Rise time",
                        log: true,
                    },
                    env.attack,
                ),
                (
                    P {
                        label: "Hold",
                        min: 0.0,
                        max: 10.0,
                        default: 0.0,
                        fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                        hint: "Time at full level after the attack",
                        log: true,
                    },
                    env.hold,
                ),
                (
                    P {
                        label: "Decay",
                        min: 0.0,
                        max: 20.0,
                        default: 0.0,
                        fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                        hint: "Fall time to the sustain level",
                        log: true,
                    },
                    env.decay,
                ),
                (
                    P {
                        label: "Sustain",
                        min: 0.0,
                        max: 1.0,
                        default: 1.0,
                        fmt: &pct,
                        hint: "Level while the key is held",
                        log: false,
                    },
                    env.sustain,
                ),
                (
                    P {
                        label: "Release",
                        min: 0.0,
                        max: 20.0,
                        default: 0.015,
                        fmt: &|v| if v <= 0.0 { "0".into() } else { secs(v) },
                        hint: "Fade after the key is released",
                        log: true,
                    },
                    env.release,
                ),
            ];
            for (k, (spec, v)) in rows.iter().enumerate() {
                let new = self.pknob(ui, spec, color, *v);
                if let Some(v) = new {
                    let mut e = env.clone();
                    let v = if spec.log && v < 0.0011 { 0.0 } else { v };
                    match k {
                        0 => e.delay = v,
                        1 => e.attack = v,
                        2 => e.hold = v,
                        3 => e.decay = v,
                        4 => e.sustain = v,
                        _ => e.release = v,
                    }
                    out = Some((e, false));
                }
            }
        });
        out
    }
}
