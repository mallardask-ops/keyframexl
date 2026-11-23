//! Forge selected-layer inspector: source selector (`Osc+`, `Modal`, `Grain`,
//! `Noise`, `Sample`, `Tide`), source DSP knobs, timing/level knobs, and Layer FX.

use super::{
    Bouncing, Into, View, chips, fx_list, hz, knob, source_color, source_kind, wrapped_chips_idx,
};
use crate::i18n::fill;
use crate::theme as t;
use crate::ui::kit;
use crate::ui::synth::{Edit, panel, pct, secs, signed};
use eframe::egui::{self, Align2, Sense, Ui, vec2};
use keyframexl::forge::{self, Layer, ModalMaterial, NoiseColor, Source, Wave};

pub(super) struct InspectorOut {
    pub load_sample: Option<usize>,
    pub load_grain_sample: Option<usize>,
    pub bounce: Option<(Into, forge::Bounce)>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn inspector_panel(
    ui: &mut Ui,
    ed: &mut Edit,
    id: u64,
    l: &mut Layer,
    view: &View,
    jobs: &Bouncing,
) -> InspectorOut {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut out = InspectorOut {
        load_sample: None,
        load_grain_sample: None,
        bounce: None,
    };
    let color = source_color(&l.source);
    let salt = format!("forge-{id}-{}", view.layer);

    panel(
        ui,
        &fill(
            tr.forge_sec_layer,
            &[("n", &format!("{:02}", view.layer + 1)), ("name", &l.name)],
        ),
        color,
        |ui| {
            // Name + timing summary.
            ui.horizontal(|ui| {
                let mut name = l.name.clone();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(180.0)
                        .hint_text(tr.forge_layer_name_hint),
                );
                if r.changed() {
                    if kit::first_edit(ui, &r) {
                        ed.started = true;
                    }
                    l.name = name;
                    ed.changed = true;
                }
                kit::end_edit(ui, &r);
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {}",
                        secs(l.start as f64),
                        secs(l.length as f64)
                    ))
                    .font(t::mono(10.5))
                    .color(t::faint()),
                );
            });

            // Source switcher (wrapping so long localized labels never push the column wide).
            let kinds = [
                tr.forge_src_osc,
                tr.forge_src_modal,
                tr.forge_src_grain,
                tr.forge_src_noise,
                tr.forge_src_sample,
                tr.forge_src_synth,
            ];
            let at = source_kind(&l.source);
            if let Some(k) = wrapped_chips_idx(ui, &kinds, at)
                && k != at
            {
                match k {
                    0 => l.source = Source::osc(Wave::Square, 440.0),
                    1 => l.source = Source::modal(ModalMaterial::Metal, 480.0),
                    2 => l.source = Source::grain(750.0, 40.0, 24.0),
                    3 => l.source = Source::noise(NoiseColor::White, 4000.0),
                    4 => out.load_sample = Some(view.layer),
                    _ => {
                        l.source = Source::Synth {
                            patch: Box::new(keyframexl::synth::SynthPatch::factory(2)),
                            key: 60,
                        }
                    }
                }
                if k != 4 {
                    ed.started = true;
                    ed.changed = true;
                }
            }

            // Source-specific controls.
            match &mut l.source {
                Source::Osc {
                    wave,
                    hz: f,
                    vibrato_hz,
                    vibrato,
                    fm_ratio,
                    fm_index,
                    unison,
                    detune,
                    shape,
                } => {
                    chips(
                        ui,
                        ed,
                        &format!("{salt}-wave"),
                        &Wave::ALL,
                        &Wave::ALL.map(|w| tr.forge_wave(w)),
                        wave,
                    );
                    ui.horizontal_wrapped(|ui| {
                        knob(
                            ui,
                            ed,
                            tr.forge_k_pitch,
                            color,
                            f,
                            (20.0, 8000.0, 440.0),
                            true,
                            &hz,
                            tr.forge_k_osc_pitch_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_fm_idx,
                            color,
                            fm_index,
                            (0.0, 8.0, 0.0),
                            false,
                            &|v| format!("{v:.2}"),
                            tr.forge_k_fm_idx_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_fm_ratio,
                            color,
                            fm_ratio,
                            (0.25, 12.0, 1.0),
                            true,
                            &|v| format!("{v:.2}x"),
                            tr.forge_k_fm_ratio_hint,
                        );
                        let mut u = *unison as f64;
                        ed.knob(
                            ui,
                            tr.forge_k_unison,
                            color,
                            &mut u,
                            (1.0, 4.0, 1.0),
                            false,
                            &|v| format!("{:.0}v", v.round()),
                            tr.forge_k_unison_hint,
                        );
                        *unison = u.round().clamp(1.0, 4.0) as u8;
                        knob(
                            ui,
                            ed,
                            tr.forge_k_detune,
                            color,
                            detune,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.forge_k_detune_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_shape,
                            color,
                            shape,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.forge_k_shape_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_vibrato,
                            color,
                            vibrato,
                            (0.0, 4.0, 0.0),
                            false,
                            &|v| format!("{v:.2} st"),
                            tr.forge_k_vibrato_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_rate,
                            color,
                            vibrato_hz,
                            (0.0, 30.0, 6.0),
                            false,
                            &|v| format!("{v:.1} Hz"),
                            tr.forge_k_vib_rate_hint,
                        );
                    });
                }
                Source::Modal {
                    material,
                    hz: f,
                    damping,
                    inharmonicity,
                    strike,
                } => {
                    chips(
                        ui,
                        ed,
                        &format!("{salt}-mat"),
                        &ModalMaterial::ALL,
                        &ModalMaterial::ALL.map(|m| tr.forge_material(m)),
                        material,
                    );
                    ui.horizontal_wrapped(|ui| {
                        knob(
                            ui,
                            ed,
                            tr.forge_k_pitch,
                            color,
                            f,
                            (40.0, 6000.0, 480.0),
                            true,
                            &hz,
                            tr.forge_k_osc_pitch_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_damp,
                            color,
                            damping,
                            (0.0, 1.0, 0.45),
                            false,
                            &pct,
                            tr.forge_k_damp_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_inharm,
                            color,
                            inharmonicity,
                            (0.0, 1.0, 0.35),
                            false,
                            &pct,
                            tr.forge_k_inharm_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_strike,
                            color,
                            strike,
                            (0.0, 1.0, 0.75),
                            false,
                            &pct,
                            tr.forge_k_strike_hint,
                        );
                    });
                }
                Source::Grain {
                    hz: f,
                    density,
                    grain_ms,
                    scatter,
                    spread,
                    spray,
                    sample,
                } => {
                    ui.horizontal_wrapped(|ui| {
                        let lbl = sample
                            .as_ref()
                            .map_or(tr.forge_grain_proc, |s| s.name.as_str());
                        ui.label(
                            egui::RichText::new(lbl)
                                .font(t::sans(12.5))
                                .color(t::text()),
                        );
                        if ui.button(tr.forge_grain_sample).clicked() {
                            out.load_grain_sample = Some(view.layer);
                        }
                        if sample.is_some() && ui.button(tr.forge_grain_clear).clicked() {
                            *sample = None;
                            ed.started = true;
                            ed.changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if sample.is_none() {
                            knob(
                                ui,
                                ed,
                                tr.forge_k_pitch,
                                color,
                                f,
                                (40.0, 8000.0, 750.0),
                                true,
                                &hz,
                                tr.forge_k_osc_pitch_hint,
                            );
                        }
                        knob(
                            ui,
                            ed,
                            tr.forge_k_density,
                            color,
                            density,
                            (2.0, 160.0, 40.0),
                            true,
                            &|v| format!("{v:.0}/s"),
                            tr.forge_k_density_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_grain_ms,
                            color,
                            grain_ms,
                            (5.0, 200.0, 24.0),
                            true,
                            &|v| format!("{v:.0} ms"),
                            tr.forge_k_grain_ms_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_scatter,
                            color,
                            scatter,
                            (0.0, 1.0, 0.35),
                            false,
                            &pct,
                            tr.forge_k_scatter_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_spread,
                            color,
                            spread,
                            (0.0, 1.0, 0.55),
                            false,
                            &pct,
                            tr.forge_k_spread_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_spray,
                            color,
                            spray,
                            (0.0, 1.0, 0.45),
                            false,
                            &pct,
                            tr.forge_k_spray_hint,
                        );
                    });
                }
                Source::Noise {
                    color: nc,
                    hz: f,
                    stereo_width,
                    burst,
                } => {
                    chips(
                        ui,
                        ed,
                        &format!("{salt}-noise"),
                        &NoiseColor::ALL,
                        &NoiseColor::ALL.map(|c| tr.forge_noise_color(c)),
                        nc,
                    );
                    ui.horizontal_wrapped(|ui| {
                        if matches!(nc, NoiseColor::Digital | NoiseColor::Crackle) {
                            let hint = if *nc == NoiseColor::Digital {
                                tr.forge_k_digital_rate_hint
                            } else {
                                tr.forge_k_crackle_rate_hint
                            };
                            knob(
                                ui,
                                ed,
                                tr.forge_k_rate,
                                color,
                                f,
                                (10.0, 48_000.0, 4000.0),
                                true,
                                &hz,
                                hint,
                            );
                        }
                        knob(
                            ui,
                            ed,
                            tr.forge_k_width,
                            color,
                            stereo_width,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.forge_k_width_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_burst,
                            color,
                            burst,
                            (0.0, 1.0, 0.0),
                            false,
                            &pct,
                            tr.forge_k_burst_hint,
                        );
                    });
                }
                Source::Sample {
                    name,
                    reverse,
                    from,
                    trim_start,
                    trim_end,
                    ..
                } => {
                    if let Some(b) = from {
                        let mut nb = *b;
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt(format!("{salt}-from"))
                                .width(150.0)
                                .selected_text(jobs.name(nb.channel))
                                .show_ui(ui, |ui| {
                                    kit::menu_style(ui);
                                    for (cid, n) in jobs.channels {
                                        ui.selectable_value(&mut nb.channel, *cid, n);
                                    }
                                });
                            let mut k = nb.key as f64;
                            let mut scratch = Edit::default();
                            scratch.knob(
                                ui,
                                tr.forge_k_note,
                                color,
                                &mut k,
                                (24.0, 96.0, 60.0),
                                false,
                                &|v| crate::ui::roll::note_name(v.round() as u8),
                                tr.forge_k_bounce_note_hint,
                            );
                            nb.key = k.round().clamp(0.0, 127.0) as u8;
                            let mut h = nb.hold as f64;
                            scratch.knob(
                                ui,
                                tr.forge_k_hold,
                                color,
                                &mut h,
                                (0.02, 4.0, 0.5),
                                true,
                                &|v| secs(v),
                                tr.forge_k_hold_hint,
                            );
                            nb.hold = h as f32;
                            let busy = jobs.now.is_some();
                            if kit::button(
                                ui,
                                tr.forge_bounce_btn,
                                kit::Kind::Normal,
                                !busy && jobs.channels.iter().any(|c| c.0 == nb.channel),
                            )
                            .on_hover_text(tr.forge_bounce_btn_tip)
                            .clicked()
                            {
                                out.bounce = Some((Into::Layer(view.layer), nb));
                            }
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(name.as_str()).font(t::sans(13.0)));
                        if ui.button(tr.forge_replace_btn).clicked() {
                            out.load_sample = Some(view.layer);
                        }
                        if crate::ui::settings_ui::kit_switch(ui, reverse) {
                            ed.started = true;
                            ed.changed = true;
                        }
                        ui.label(tr.forge_reverse_lbl);
                    });
                    ui.horizontal_wrapped(|ui| {
                        knob(
                            ui,
                            ed,
                            tr.forge_k_trim_in,
                            color,
                            trim_start,
                            (0.0, 0.95, 0.0),
                            false,
                            &pct,
                            tr.forge_k_trim_in_hint,
                        );
                        knob(
                            ui,
                            ed,
                            tr.forge_k_trim_out,
                            color,
                            trim_end,
                            (0.05, 1.0, 1.0),
                            false,
                            &pct,
                            tr.forge_k_trim_out_hint,
                        );
                        if *trim_end < *trim_start + 0.01 {
                            *trim_end = (*trim_start + 0.01).min(1.0);
                        }
                    });
                }
                Source::Synth { patch, key } => {
                    ui.horizontal_wrapped(|ui| {
                        let names = keyframexl::synth::SynthPatch::PRESETS;
                        let now = (0..names.len())
                            .find(|&i| **patch == keyframexl::synth::SynthPatch::factory(i));
                        egui::ComboBox::from_id_salt(format!("{salt}-tide"))
                            .width(150.0)
                            .selected_text(now.map_or(tr.forge_edited_patch, |i| names[i]))
                            .show_ui(ui, |ui| {
                                kit::menu_style(ui);
                                for (i, n) in names.iter().enumerate() {
                                    if ui.selectable_label(now == Some(i), *n).clicked() {
                                        **patch = keyframexl::synth::SynthPatch::factory(i);
                                        ed.started = true;
                                        ed.changed = true;
                                    }
                                }
                            });
                        let mut k = *key as f64;
                        ed.knob(
                            ui,
                            tr.forge_k_note,
                            color,
                            &mut k,
                            (24.0, 96.0, 60.0),
                            false,
                            &|v| crate::ui::roll::note_name(v.round() as u8),
                            tr.forge_k_synth_note_hint,
                        );
                        *key = k.round().clamp(0.0, 127.0) as u8;
                    });
                }
            }

            // Layer timing & mix knobs.
            ui.horizontal_wrapped(|ui| {
                knob(
                    ui,
                    ed,
                    tr.forge_k_start,
                    t::enc_white(),
                    &mut l.start,
                    (0.0, 2.0, 0.0),
                    false,
                    &|v| secs(v),
                    tr.forge_k_start_hint,
                );
                knob(
                    ui,
                    ed,
                    tr.forge_k_length,
                    t::enc_white(),
                    &mut l.length,
                    (0.01, forge::MAX_SECONDS as f64, 0.3),
                    true,
                    &|v| secs(v),
                    tr.forge_k_length_hint,
                );
                knob(
                    ui,
                    ed,
                    tr.forge_k_gain,
                    t::enc_white(),
                    &mut l.gain,
                    (0.0, 2.0, 1.0),
                    false,
                    &pct,
                    tr.forge_k_gain_hint,
                );
                knob(
                    ui,
                    ed,
                    tr.forge_k_pan,
                    t::enc_white(),
                    &mut l.pan,
                    (-1.0, 1.0, 0.0),
                    false,
                    &signed,
                    tr.forge_k_pan_hint,
                );
            });

            // Layer FX chain.
            ui.add_space(2.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
            kit::caps(
                ui.painter(),
                r.left_center(),
                Align2::LEFT_CENTER,
                tr.forge_layer_fx_caps,
                10.0,
                t::muted(),
            );
            fx_list(ui, ed, &format!("{salt}-fx"), &mut l.fx);
        },
    );

    out
}
