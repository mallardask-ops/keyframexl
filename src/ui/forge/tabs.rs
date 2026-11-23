//! Secondary Studio tabs (`Presets & Blocks`, `Variations`, `Export`) plus the
//! Master Character & FX strip shown at the bottom of `Design`.

use super::{
    PresetFilter, PresetSub, Tab, VariationSlot, View, cached_user_sounds, fx_list, knob, wave,
    wrapped_chips_idx,
};
use crate::i18n::fill;
use crate::theme as t;
use crate::ui::kit;
use crate::ui::synth::{Edit, panel, pct, secs};
use eframe::egui::{self, Align2, CornerRadius, Sense, Stroke, Ui, vec2};
use keyframexl::forge::{
    self, Curve, CurvePreset, FilterMode, ForgePatch, Layer, MAX_LAYERS, ModalMaterial, NoiseColor,
    Rendered, Source, Wave, export, library::Category, recipes::Recipe,
};
use std::sync::Arc;

/// Built-in reusable single-layer building blocks for quick SFX assembly.
pub(super) struct LayerBlock {
    pub name: &'static str,
    pub build: fn() -> Layer,
}

pub(super) fn layer_blocks(tr: &crate::i18n::Strings) -> Vec<LayerBlock> {
    vec![
        LayerBlock {
            name: tr.forge_block_sub_drop,
            build: || {
                Layer::new(
                    "Sub Drop",
                    Source::Osc {
                        wave: Wave::Sine,
                        hz: 55.0,
                        vibrato_hz: 0.0,
                        vibrato: 0.0,
                        fm_ratio: 2.0,
                        fm_index: 0.8,
                        unison: 1,
                        detune: 0.0,
                        shape: 0.15,
                    },
                    0.32,
                )
                .with_pitch(Curve::new(&[(0.0, 18.0, -0.75), (1.0, -4.0, 0.0)]))
                .with_amp(CurvePreset::Drop.build(0.0, 1.0, 1.0))
                .with_gain(1.1)
            },
        },
        LayerBlock {
            name: tr.forge_block_metal_strike,
            build: || {
                Layer::new(
                    "Metal Strike",
                    Source::modal(ModalMaterial::Metal, 640.0),
                    0.28,
                )
                .with_amp(Curve::flat(1.0))
            },
        },
        LayerBlock {
            name: tr.forge_block_wood_knock,
            build: || {
                Layer::new(
                    "Wood Knock",
                    Source::modal(ModalMaterial::Wood, 320.0),
                    0.18,
                )
                .with_amp(Curve::flat(1.0))
            },
        },
        LayerBlock {
            name: tr.forge_block_glass_ping,
            build: || {
                Layer::new(
                    "Glass Ping",
                    Source::modal(ModalMaterial::Glass, 1180.0),
                    0.45,
                )
                .with_amp(Curve::flat(1.0))
            },
        },
        LayerBlock {
            name: tr.forge_block_gravel_burst,
            build: || {
                Layer::new(
                    "Gravel",
                    Source::Grain {
                        hz: 1100.0,
                        density: 68.0,
                        grain_ms: 14.0,
                        scatter: 0.65,
                        spread: 0.75,
                        spray: 0.8,
                        sample: None,
                    },
                    0.26,
                )
                .with_amp(CurvePreset::Pluck.build(0.0, 1.0, 1.0))
            },
        },
        LayerBlock {
            name: tr.forge_block_plasma_grain,
            build: || {
                Layer::new(
                    "Plasma",
                    Source::Grain {
                        hz: 780.0,
                        density: 46.0,
                        grain_ms: 22.0,
                        scatter: 0.48,
                        spread: 0.8,
                        spray: 0.55,
                        sample: None,
                    },
                    0.55,
                )
                .with_amp(CurvePreset::Drop.build(0.0, 1.0, 1.0))
                .with_morph(CurvePreset::Bell.build(0.2, 0.9, 0.5))
            },
        },
        LayerBlock {
            name: tr.forge_block_stereo_whoosh,
            build: || {
                Layer::new(
                    "Whoosh",
                    Source::Noise {
                        color: NoiseColor::Pink,
                        hz: 4000.0,
                        stereo_width: 0.85,
                        burst: 0.0,
                    },
                    0.65,
                )
                .with_filter(
                    FilterMode::BandPass,
                    CurvePreset::Bell.build(0.18, 0.88, 0.5),
                    0.45,
                )
                .with_amp(CurvePreset::Bell.build(0.0, 1.0, 0.5))
                .with_pan_curve(CurvePreset::Ramp.build(-0.85, 0.85, 0.0))
            },
        },
        LayerBlock {
            name: tr.forge_block_air_tail,
            build: || {
                Layer::new(
                    "Air Tail",
                    Source::Noise {
                        color: NoiseColor::Brown,
                        hz: 4000.0,
                        stereo_width: 0.7,
                        burst: 0.15,
                    },
                    0.8,
                )
                .with_start(0.08)
                .with_amp(CurvePreset::Drop.build(0.0, 0.85, 0.5))
                .with_gain(0.55)
            },
        },
    ]
}

/// Bottom Character macros + Master FX strip on the `Design` tab.
pub(super) fn master_strip(ui: &mut Ui, ed: &mut Edit, id: u64, p: &mut ForgePatch) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    panel(ui, tr.forge_sec_character, t::enc_white(), |ui| {
        ui.horizontal_wrapped(|ui| {
            let macro_colors = [t::enc_white(), t::enc_orange(), t::wait(), t::enc_blue()];
            let macros = tr.forge_macros();
            for i in 0..4 {
                let mut v = p.macros.get(i);
                let def = forge::Macros::default().get(i) as f64;
                knob(
                    ui,
                    ed,
                    macros[i].0,
                    macro_colors[i],
                    &mut v,
                    (0.0, 1.0, def),
                    false,
                    &pct,
                    macros[i].1,
                );
                p.macros.set(i, v);
            }
            knob(
                ui,
                ed,
                tr.forge_k_pitch,
                t::enc_white(),
                &mut p.transpose,
                (-24.0, 24.0, 0.0),
                false,
                &|v| format!("{v:+.1} st"),
                tr.forge_k_pitch_hint,
            );
            knob(
                ui,
                ed,
                tr.forge_k_output,
                t::enc_green(),
                &mut p.gain_db,
                (-36.0, 12.0, 0.0),
                false,
                &|v| format!("{v:+.1} dB"),
                tr.forge_k_output_hint,
            );
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(tr.forge_sec_master_fx.to_uppercase())
                .font(t::mono(10.0))
                .color(t::muted()),
        );
        fx_list(ui, ed, &format!("forge-master-{id}"), &mut p.fx);
    });
}

#[derive(Default)]
pub(super) struct PresetsOut {
    pub pick_recipe: Option<Recipe>,
    pub adopt: Option<ForgePatch>,
    pub append_layer: Option<(String, Layer)>,
    pub inspire: bool,
}

enum PresetItem {
    Builtin(Recipe),
    User(ForgePatch),
}

/// Tab 2: Searchable tagged preset browser + Layer Building Blocks.
pub(super) fn presets_tab(ui: &mut Ui, id: u64, p: &ForgePatch, view: &mut View) -> PresetsOut {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut out = PresetsOut::default();

    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut view.search)
                .desired_width((ui.available_width() - 290.0).max(160.0))
                .hint_text(tr.forge_search_hint),
        );
        let sub_idx = usize::from(view.preset_sub == PresetSub::Blocks);
        if let (Some(i), _) = kit::chips(
            ui,
            &format!("forge-psub-{id}"),
            &[tr.forge_sub_full, tr.forge_sub_blocks],
            sub_idx,
            None,
        ) {
            view.preset_sub = if i == 1 {
                PresetSub::Blocks
            } else {
                PresetSub::Full
            };
        }
        if ui
            .button(tr.forge_inspire_btn)
            .on_hover_text(tr.forge_inspire_tip)
            .clicked()
        {
            out.inspire = true;
        }
    });

    let q = view.search.trim().to_lowercase();
    match view.preset_sub {
        PresetSub::Full => {
            // Category tag filter row.
            ui.horizontal_wrapped(|ui| {
                let mut tag_btn = |label: &str, f: PresetFilter| {
                    let active = view.preset_filter == f;
                    if ui.selectable_label(active, label).clicked() {
                        view.preset_filter = f;
                    }
                };
                tag_btn(tr.forge_cat_all, PresetFilter::All);
                tag_btn(tr.forge_cat_loops, PresetFilter::Loops);
                for cat in Category::ALL {
                    tag_btn(tr.forge_category(cat), PresetFilter::Category(cat));
                }
                tag_btn(tr.forge_my_sounds, PresetFilter::User);
            });

            let user = cached_user_sounds(ui.ctx());
            let mut items: Vec<(String, String, usize, bool, PresetItem)> = Vec::new();
            if view.preset_filter != PresetFilter::User {
                for r in Recipe::ALL {
                    let cat = r.category();
                    if let PresetFilter::Category(want) = view.preset_filter
                        && want != cat
                    {
                        continue;
                    }
                    let (layers_n, is_loop) = r.meta();
                    if view.preset_filter == PresetFilter::Loops && !is_loop {
                        continue;
                    }
                    let label = r.label();
                    let cat_lbl = tr.forge_category(cat);
                    if !q.is_empty()
                        && !label.to_lowercase().contains(&q)
                        && !cat_lbl.to_lowercase().contains(&q)
                    {
                        continue;
                    }
                    items.push((
                        label.into(),
                        cat_lbl.into(),
                        layers_n,
                        is_loop,
                        PresetItem::Builtin(r),
                    ));
                }
            }
            if matches!(
                view.preset_filter,
                PresetFilter::All | PresetFilter::Loops | PresetFilter::User
            ) {
                for u in user.iter() {
                    if view.preset_filter == PresetFilter::Loops && !u.seamless_loop {
                        continue;
                    }
                    if !q.is_empty() && !u.name.to_lowercase().contains(&q) {
                        continue;
                    }
                    items.push((
                        u.name.clone(),
                        tr.forge_my_sounds.into(),
                        u.layers.len(),
                        u.seamless_loop,
                        PresetItem::User(u.clone()),
                    ));
                }
            }

            if items.is_empty() {
                ui.label(egui::RichText::new(tr.forge_no_presets_match).color(t::muted()));
                return out;
            }

            let cols = if ui.available_width() >= 560.0 { 2 } else { 1 };
            let col_w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    for row in items.chunks(cols) {
                        ui.horizontal(|ui| {
                            for (name, cat_lbl, layers_n, is_loop, item) in row {
                                let (r, resp) =
                                    ui.allocate_exact_size(vec2(col_w, 42.0), Sense::click());
                                let active = p.name == *name;
                                ui.painter().rect(
                                    r,
                                    CornerRadius::same(6),
                                    if resp.hovered() || active {
                                        t::raised()
                                    } else {
                                        t::surface()
                                    },
                                    Stroke::new(
                                        1.0_f32,
                                        if active {
                                            t::enc_blue()
                                        } else if resp.hovered() {
                                            t::strong()
                                        } else {
                                            t::line()
                                        },
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                ui.painter().text(
                                    r.left_top() + vec2(9.0, 8.0),
                                    Align2::LEFT_TOP,
                                    name,
                                    t::sans(13.0),
                                    t::text(),
                                );
                                ui.painter().text(
                                    r.left_bottom() + vec2(9.0, -7.0),
                                    Align2::LEFT_BOTTOM,
                                    cat_lbl,
                                    t::mono(9.5),
                                    t::muted(),
                                );
                                let (badge, bcol) = if *is_loop {
                                    (tr.forge_loop_btn.to_uppercase(), t::enc_green())
                                } else {
                                    (format!("{layers_n}L"), t::enc_blue())
                                };
                                ui.painter().text(
                                    r.right_center() + vec2(-9.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    badge,
                                    t::mono(10.0),
                                    bcol,
                                );
                                if resp.clicked() {
                                    match item {
                                        PresetItem::Builtin(rec) => out.pick_recipe = Some(*rec),
                                        PresetItem::User(patch) => out.adopt = Some(patch.clone()),
                                    }
                                }
                            }
                        });
                    }
                });
        }
        PresetSub::Blocks => {
            let full = p.layers.len() >= MAX_LAYERS;
            let blocks: Vec<_> = layer_blocks(tr)
                .into_iter()
                .filter(|b| q.is_empty() || b.name.to_lowercase().contains(&q))
                .collect();
            if blocks.is_empty() {
                ui.label(egui::RichText::new(tr.forge_no_presets_match).color(t::muted()));
                return out;
            }
            let cols = if ui.available_width() >= 560.0 { 2 } else { 1 };
            let col_w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
            for row in blocks.chunks(cols) {
                ui.horizontal(|ui| {
                    for b in row {
                        let layer = (b.build)();
                        let (r, resp) = ui.allocate_exact_size(vec2(col_w, 44.0), Sense::click());
                        ui.painter().rect(
                            r,
                            CornerRadius::same(6),
                            if resp.hovered() && !full {
                                t::raised()
                            } else {
                                t::surface()
                            },
                            Stroke::new(
                                1.0_f32,
                                if resp.hovered() && !full {
                                    t::strong()
                                } else {
                                    t::line()
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            r.left_top() + vec2(9.0, 8.0),
                            Align2::LEFT_TOP,
                            format!("+ {}", b.name),
                            t::sans(13.0),
                            if full { t::muted() } else { t::text() },
                        );
                        ui.painter().text(
                            r.left_bottom() + vec2(9.0, -7.0),
                            Align2::LEFT_BOTTOM,
                            format!("{} · {}", layer.source.label(), secs(layer.length as f64)),
                            t::mono(9.5),
                            t::muted(),
                        );
                        if resp.clicked() && !full {
                            out.append_layer = Some((b.name.to_string(), layer));
                        }
                    }
                });
            }
        }
    }
    out
}

#[derive(Default)]
pub(super) struct VariationsOut {
    pub reroll_all: bool,
    pub reroll_one: Option<usize>,
    pub hear: Option<Arc<Rendered>>,
    pub adopt: Option<ForgePatch>,
}

/// Tab 3: Live Variation / Round-Robin rack with lock status, spread control,
/// single-card reroll, pin protection, and direct handoff to WYSIWYG export.
pub(super) fn variations_tab(
    ui: &mut Ui,
    p: &ForgePatch,
    view: &mut View,
    growing: bool,
) -> VariationsOut {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut out = VariationsOut::default();

    panel(ui, tr.forge_sec_var_rack, t::enc_green(), |ui| {
        ui.horizontal_wrapped(|ui| {
            let mut scratch = Edit::default();
            scratch.knob(
                ui,
                tr.forge_k_files,
                t::enc_white(),
                &mut view.count,
                (1.0, 12.0, 6.0),
                false,
                &|v| format!("{:.0}", v.round()),
                tr.forge_k_files_hint,
            );
            view.count = view.count.round().clamp(1.0, 12.0);

            let mut spread = view.spread as f64;
            scratch.knob(
                ui,
                tr.forge_k_spread_var,
                t::enc_green(),
                &mut spread,
                (0.05, 0.85, 0.18),
                false,
                &pct,
                tr.forge_k_spread_var_hint,
            );
            view.spread = spread as f32;

            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let lbl = if growing {
                        tr.forge_growing
                    } else {
                        tr.forge_reroll_all
                    };
                    if kit::button(ui, lbl, kit::Kind::Normal, !growing)
                        .on_hover_text(tr.forge_reroll_tip)
                        .clicked()
                    {
                        out.reroll_all = true;
                    }
                    if kit::button(ui, tr.forge_export_approved, kit::Kind::Normal, true).clicked()
                    {
                        view.tab = Tab::Export;
                    }
                });
                let locked: Vec<String> = p
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.locked)
                    .map(|(i, _)| format!("{:02}", i + 1))
                    .collect();
                let lock_msg = if locked.is_empty() {
                    tr.forge_locked_none.to_string()
                } else {
                    fill(tr.forge_locked_layers_lbl, &[("list", &locked.join(", "))])
                };
                ui.label(egui::RichText::new(lock_msg).font(t::mono(10.0)).color(
                    if locked.is_empty() {
                        t::faint()
                    } else {
                        t::enc_blue()
                    },
                ));
            });
        });

        ui.add_space(6.0);
        let cols = if ui.available_width() >= 560.0 { 3 } else { 2 };
        let w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
        for row in 0..view.variations.len().div_ceil(cols) {
            ui.horizontal(|ui| {
                for c in 0..cols {
                    let i = row * cols + c;
                    let Some(slot) = view.variations.get_mut(i) else {
                        break;
                    };
                    variation_card(
                        ui,
                        w,
                        i,
                        slot,
                        view.heard == Some(i),
                        &mut out,
                        &mut view.heard,
                    );
                }
            });
        }
    });

    out
}

fn variation_card(
    ui: &mut Ui,
    w: f32,
    i: usize,
    slot: &mut VariationSlot,
    lit: bool,
    out: &mut VariationsOut,
    heard: &mut Option<usize>,
) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    egui::Frame::new()
        .fill(t::surface())
        .stroke(Stroke::new(
            1.0_f32,
            if lit {
                t::enc_green()
            } else if slot.pinned {
                t::enc_blue()
            } else {
                t::line()
            },
        ))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width((w - 14.0).max(110.0));
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:02}", i + 1))
                        .font(t::mono(10.5))
                        .color(if lit { t::enc_green() } else { t::muted() }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(secs(slot.sound.seconds()))
                            .font(t::mono(10.0))
                            .color(t::faint()),
                    );
                });
            });
            let (r, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
            let well = kit::sunken(ui.painter(), r, 5.0).shrink(3.0);
            wave(
                ui.painter(),
                well,
                &slot.sound.frames,
                if lit { t::enc_green() } else { t::muted() },
            );
            if resp.double_clicked() {
                out.adopt = Some(slot.patch.clone());
            } else if resp.clicked() {
                out.hear = Some(slot.sound.clone());
                *heard = Some(i);
            }
            ui.horizontal(|ui| {
                if ui
                    .small_button("▶")
                    .on_hover_text(tr.forge_var_play_tip)
                    .clicked()
                {
                    out.hear = Some(slot.sound.clone());
                    *heard = Some(i);
                }
                if ui
                    .small_button("↻")
                    .on_hover_text(tr.forge_var_reroll_tip)
                    .clicked()
                {
                    out.reroll_one = Some(i);
                }
                let pin_btn = egui::Button::new(
                    egui::RichText::new("★")
                        .font(t::sans(11.0))
                        .color(if slot.pinned { t::wait() } else { t::faint() }),
                )
                .min_size(vec2(22.0, 20.0));
                if ui
                    .add(pin_btn)
                    .on_hover_text(tr.forge_var_pin_tip)
                    .clicked()
                {
                    slot.pinned = !slot.pinned;
                }
                if ui
                    .small_button(tr.forge_var_use)
                    .on_hover_text(tr.forge_use_idea_tip)
                    .clicked()
                {
                    out.adopt = Some(slot.patch.clone());
                }
            });
        });
}

#[derive(Default)]
pub(super) struct ExportOut {
    pub export_channel: bool,
    pub export_pack: bool,
}

/// Tab 4: WYSIWYG Game Export + Sound Pack batch export.
pub(super) fn export_tab(
    ui: &mut Ui,
    id: u64,
    p: &ForgePatch,
    view: &mut View,
    forge_channel_count: usize,
) -> ExportOut {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut out = ExportOut::default();
    let file_count = if view.variations.is_empty() {
        view.count as usize
    } else {
        view.variations.len()
    };

    panel(ui, tr.forge_sec_export, t::enc_green(), |ui| {
        let inner_w = ui.available_width();
        if inner_w >= 620.0 {
            let gap = 12.0;
            let col_w = ((inner_w - gap) * 0.5).floor();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    vec2(col_w, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(col_w);
                        ui.set_max_width(col_w);
                        export_options_ui(ui, id, view, file_count, forge_channel_count);
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(col_w, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(col_w);
                        ui.set_max_width(col_w);
                        export_preview_ui(ui, p, view, file_count, forge_channel_count, &mut out);
                    },
                );
            });
        } else {
            export_options_ui(ui, id, view, file_count, forge_channel_count);
            ui.add_space(6.0);
            export_preview_ui(ui, p, view, file_count, forge_channel_count, &mut out);
        }
    });

    out
}

fn export_options_ui(
    ui: &mut Ui,
    _id: u64,
    view: &mut View,
    file_count: usize,
    forge_channel_count: usize,
) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_scope).color(t::muted()));
        let this_lbl = fill(tr.forge_exp_this_channel, &[("n", &file_count.to_string())]);
        let pack_lbl = fill(
            tr.forge_exp_sound_pack,
            &[("n", &forge_channel_count.to_string())],
        );
        if let Some(i) = wrapped_chips_idx(
            ui,
            &[this_lbl.as_str(), pack_lbl.as_str()],
            usize::from(view.export_pack),
        ) {
            view.export_pack = i == 1;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_format).color(t::muted()));
        if let Some(i) = wrapped_chips_idx(ui, &["WAV 16", "WAV 24", "OGG"], view.format_idx) {
            view.format_idx = i;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_rate).color(t::muted()));
        if let Some(i) = wrapped_chips_idx(ui, &["48 kHz", "44.1 kHz"], usize::from(view.rate_44)) {
            view.rate_44 = i == 1;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_channels).color(t::muted()));
        if let Some(i) = wrapped_chips_idx(
            ui,
            &[tr.forge_exp_stereo, tr.forge_mono],
            usize::from(view.mono),
        ) {
            view.mono = i == 1;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_loudness).color(t::muted()));
        if let Some(i) = wrapped_chips_idx(
            ui,
            &[tr.forge_exp_as_mixed, tr.forge_exp_peak, tr.forge_exp_lufs],
            view.loudness_idx,
        ) {
            view.loudness_idx = i;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr.forge_exp_template).color(t::muted()));
        ui.add(egui::TextEdit::singleline(&mut view.template).desired_width(140.0));
    });
}

fn export_preview_ui(
    ui: &mut Ui,
    p: &ForgePatch,
    view: &View,
    file_count: usize,
    forge_channel_count: usize,
    out: &mut ExportOut,
) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(fill(
                tr.forge_exp_preview_head,
                &[("n", &file_count.to_string())],
            ))
            .font(t::mono(10.0))
            .color(t::muted()),
        );
        if !view.variations.is_empty() {
            ui.label(
                egui::RichText::new(tr.forge_exp_auditioned)
                    .font(t::mono(10.0))
                    .color(t::enc_green()),
            );
        }
    });
    let ext = view.file_format().ext();
    egui::Frame::new()
        .fill(t::sunken())
        .stroke(Stroke::new(1.0_f32, t::edge()))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let show = file_count.min(8);
            for i in 0..show {
                let fname = export::format_filename(&p.name, i + 1, &view.template, ext);
                ui.label(
                    egui::RichText::new(fname)
                        .font(t::mono(11.0))
                        .color(t::text()),
                );
            }
            if file_count > show {
                ui.label(
                    egui::RichText::new(format!("… +{}", file_count - show))
                        .font(t::mono(10.5))
                        .color(t::faint()),
                );
            }
        });
    ui.add_space(6.0);
    if view.export_pack {
        let lbl = fill(
            tr.forge_exp_pack_btn,
            &[("n", &forge_channel_count.to_string())],
        );
        if kit::button(ui, &lbl, kit::Kind::Normal, forge_channel_count > 0)
            .on_hover_text(tr.forge_export_tip)
            .clicked()
        {
            out.export_pack = true;
        }
    } else {
        let lbl = fill(tr.forge_exp_write_btn, &[("n", &file_count.to_string())]);
        if kit::button(ui, &lbl, kit::Kind::Normal, true)
            .on_hover_text(tr.forge_export_tip)
            .clicked()
        {
            out.export_channel = true;
        }
    }
}
