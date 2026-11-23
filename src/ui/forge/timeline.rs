//! Forge master waveform & interactive visual layer timeline.
//! Drag clip bodies to adjust `start`, drag left/right handles to trim `length`,
//! toggle mute/solo/lock inline, and drop audio files directly from the browser.

use super::{Bouncing, Into, PresetSub, Tab, View, source_color, wave};
use crate::i18n::fill;
use crate::theme as t;
use crate::ui::kit;
use crate::ui::synth::{Edit, panel, secs};
use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::forge::{
    self, Curve, ForgePatch, ForgeSound, Layer, MAX_LAYERS, MAX_SECONDS, ModalMaterial, NoiseColor,
    Source, Wave,
};
use std::path::PathBuf;

/// Actions emitted by the timeline stage for the parent editor to run.
#[derive(Default)]
pub(super) struct TimelineOut {
    pub play: bool,
    pub load_sample: Option<usize>,
    pub drop_file: Option<PathBuf>,
    pub bounce: Option<(Into, forge::Bounce)>,
}

/// Crisp monochrome padlock icon drawn directly on the painter (no emoji).
fn draw_lock(p: &egui::Painter, r: Rect, locked: bool, color: Color32) {
    let c = r.center();
    let stroke = Stroke::new(1.25_f32, color);
    let body = Rect::from_center_size(c + vec2(0.0, 2.0), vec2(8.0, 5.5));
    p.rect_stroke(
        body,
        CornerRadius::same(1),
        stroke,
        egui::StrokeKind::Inside,
    );
    let arch_top = body.top() - 3.5;
    if locked {
        p.line_segment(
            [pos2(c.x - 2.3, body.top()), pos2(c.x - 2.3, arch_top + 1.5)],
            stroke,
        );
        p.line_segment(
            [pos2(c.x - 2.3, arch_top + 1.5), pos2(c.x, arch_top)],
            stroke,
        );
        p.line_segment(
            [pos2(c.x, arch_top), pos2(c.x + 2.3, arch_top + 1.5)],
            stroke,
        );
        p.line_segment(
            [pos2(c.x + 2.3, arch_top + 1.5), pos2(c.x + 2.3, body.top())],
            stroke,
        );
    } else {
        p.line_segment(
            [pos2(c.x - 2.3, body.top()), pos2(c.x - 2.3, arch_top + 1.2)],
            stroke,
        );
        p.line_segment(
            [pos2(c.x - 2.3, arch_top + 1.2), pos2(c.x + 0.8, arch_top)],
            stroke,
        );
        p.line_segment(
            [pos2(c.x + 0.8, arch_top), pos2(c.x + 2.6, arch_top + 1.2)],
            stroke,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stage_panel(
    ui: &mut Ui,
    ed: &mut Edit,
    id: u64,
    sound: &ForgeSound,
    loading: bool,
    p: &mut ForgePatch,
    view: &mut View,
    jobs: &Bouncing,
    playhead_frac: Option<f32>,
) -> TimelineOut {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut out = TimelineOut::default();

    let span = p
        .layers
        .iter()
        .map(|l| l.start + l.length)
        .fold(0.5f32, f32::max)
        .max(sound.rendered.seconds() as f32)
        .clamp(0.25, MAX_SECONDS);
    let view_span = (span * 1.08).min(MAX_SECONDS);

    panel(ui, tr.forge_sec_timeline, t::enc_white(), |ui| {
        // HUD row: duration, peak dB, LUFS, One-shot / Seamless loop tag.
        ui.horizontal_wrapped(|ui| {
            let db = 20.0 * sound.rendered.peak.max(1e-6).log10();
            let hud = fill(
                tr.forge_hud_stats,
                &[
                    ("len", &secs(sound.rendered.seconds())),
                    ("db", &format!("{db:.1}")),
                    ("lufs", &format!("{:.1}", sound.rendered.lufs)),
                ],
            );
            ui.label(
                egui::RichText::new(hud)
                    .font(t::mono(10.5))
                    .color(t::muted()),
            );
            let (mode_txt, mode_col) = if p.seamless_loop {
                (tr.forge_mode_loop, t::enc_green())
            } else {
                (tr.forge_mode_oneshot, t::faint())
            };
            ui.label(
                egui::RichText::new(mode_txt)
                    .font(t::mono(10.0))
                    .color(mode_col),
            );
            if sound.stale() || loading {
                ui.label(
                    egui::RichText::new(tr.forge_rendering_caps)
                        .font(t::mono(10.0))
                        .color(t::wait()),
                );
            }
        });

        // Master waveform well with live playhead.
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
        let painter = ui.painter();
        let well = kit::sunken(painter, r, 6.0).shrink(4.0);
        wave(painter, well, &sound.rendered.frames, t::enc_white());
        if let Some(frac) = playhead_frac {
            let x = well.left() + frac.clamp(0.0, 1.0) * well.width();
            painter.line_segment(
                [pos2(x, well.top()), pos2(x, well.bottom())],
                Stroke::new(1.5_f32, t::text()),
            );
        }
        if resp.on_hover_text(tr.forge_click_to_hear).clicked() {
            out.play = true;
        }

        ui.add_space(4.0);

        // Layer timeline rows.
        let solo_any = p.layers.iter().any(|l| l.solo);
        let mut remove = None;
        let panel_inner_w = ui.available_width();

        for (i, l) in p.layers.iter_mut().enumerate() {
            let selected = view.layer == i;
            let color = source_color(&l.source);
            let audible = if solo_any { l.solo } else { l.enabled };

            ui.push_id(("forge-tl-row", id, i), |ui| {
                let row_bg = if selected { t::selection() } else { t::bg() };
                let row_stroke = if selected {
                    Stroke::new(1.0_f32, t::strong())
                } else {
                    Stroke::new(1.0_f32, t::line().gamma_multiply(0.4))
                };
                egui::Frame::new()
                    .fill(row_bg)
                    .stroke(row_stroke)
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(5, 3))
                    .show(ui, |ui| {
                        let row_w = (panel_inner_w - 10.0).max(200.0);
                        ui.set_width(row_w);
                        ui.set_max_width(row_w);
                        let spacing = 5.0;
                        let del_w = 22.0;
                        let meta_w = (row_w * 0.27).clamp(155.0, 210.0);
                        let track_w = (row_w - meta_w - del_w - spacing * 2.0).max(60.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = spacing;
                            // Left fixed-width meta column so all timeline tracks align vertically.
                            ui.allocate_ui_with_layout(
                                vec2(meta_w, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(meta_w);
                                    ui.set_max_width(meta_w);
                                    if crate::ui::settings_ui::kit_switch(ui, &mut l.enabled) {
                                        ed.started = true;
                                        ed.changed = true;
                                    }
                                    let solo_btn = egui::Button::new(
                                        egui::RichText::new("S")
                                            .font(t::mono(10.5))
                                            .color(if l.solo { t::wait() } else { t::muted() }),
                                    )
                                    .min_size(vec2(20.0, 20.0));
                                    if ui.add(solo_btn).on_hover_text(tr.forge_solo_tip).clicked() {
                                        l.solo = !l.solo;
                                        ed.started = true;
                                        ed.changed = true;
                                    }
                                    // Lock toggle button (drawn vector padlock).
                                    let (lr, lresp) =
                                        ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                                    let lcol = if l.locked { t::enc_blue() } else { t::faint() };
                                    ui.painter().rect(
                                        lr,
                                        CornerRadius::same(4),
                                        if l.locked {
                                            t::enc_blue().gamma_multiply(0.16)
                                        } else {
                                            t::sunken()
                                        },
                                        Stroke::new(
                                            1.0_f32,
                                            if l.locked { t::enc_blue() } else { t::line() },
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                    draw_lock(ui.painter(), lr, l.locked, lcol);
                                    if lresp.on_hover_text(tr.forge_lock_tip).clicked() {
                                        l.locked = !l.locked;
                                        ed.started = true;
                                        ed.changed = true;
                                    }

                                    let name_txt = format!("{:02} {}", i + 1, l.name);
                                    let name_lbl = egui::RichText::new(name_txt)
                                        .font(t::sans(12.5))
                                        .color(if audible { t::text() } else { t::muted() });
                                    if ui
                                        .add(
                                            egui::Label::new(name_lbl)
                                                .truncate()
                                                .sense(Sense::click()),
                                        )
                                        .on_hover_text(tr.forge_edit_layer_tip)
                                        .clicked()
                                    {
                                        view.layer = i;
                                    }
                                },
                            );

                            // Right interactive clip track (exact budgeted width).
                            let (tr_rect, tr_resp) = ui
                                .allocate_exact_size(vec2(track_w, 24.0), Sense::click_and_drag());
                            let p_draw = ui.painter();
                            let inner = kit::sunken(p_draw, tr_rect, 4.0).shrink(1.5);
                            // Faint vertical time divisions.
                            for step in 1..5 {
                                let gx = inner.left() + inner.width() * (step as f32 / 5.0);
                                p_draw.line_segment(
                                    [pos2(gx, inner.top()), pos2(gx, inner.bottom())],
                                    Stroke::new(1.0_f32, t::surface()),
                                );
                            }

                            let x0 = inner.left()
                                + (l.start / view_span).clamp(0.0, 1.0) * inner.width();
                            let x1 = inner.left()
                                + ((l.start + l.length) / view_span).clamp(0.0, 1.0)
                                    * inner.width();
                            let clip_rect = Rect::from_min_max(
                                pos2(x0, inner.top() + 1.0),
                                pos2(x1.max(x0 + 8.0).min(inner.right()), inner.bottom() - 1.0),
                            );
                            let fill_col = if audible {
                                color.gamma_multiply(0.28)
                            } else {
                                t::surface()
                            };
                            let edge_col = if selected {
                                color
                            } else if audible {
                                color.gamma_multiply(0.7)
                            } else {
                                t::line()
                            };
                            p_draw.rect(
                                clip_rect,
                                CornerRadius::same(3),
                                fill_col,
                                Stroke::new(if selected { 1.4_f32 } else { 1.0_f32 }, edge_col),
                                egui::StrokeKind::Inside,
                            );
                            let badge = format!(
                                "{} · {}",
                                l.source.label().to_uppercase(),
                                secs(l.length as f64)
                            );
                            if clip_rect.width() > 38.0 {
                                kit::caps(
                                    p_draw,
                                    pos2(clip_rect.left() + 5.0, clip_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    &badge,
                                    9.0,
                                    if audible { t::text() } else { t::muted() },
                                );
                            }
                            if let Some(frac) = playhead_frac {
                                let px = inner.left() + frac.clamp(0.0, 1.0) * inner.width();
                                p_draw.line_segment(
                                    [pos2(px, inner.top()), pos2(px, inner.bottom())],
                                    Stroke::new(1.0_f32, t::text().gamma_multiply(0.65)),
                                );
                            }

                            // Interactive clip drag: left edge = trim start, right edge = resize length, body = move start.
                            #[derive(Clone, Copy)]
                            enum ClipDrag {
                                Move { orig_start: f32 },
                                Left { orig_start: f32, orig_end: f32 },
                                Right { orig_len: f32 },
                            }
                            let drag_id = ui.id().with(("clip-drag", id, i));
                            if tr_resp.clicked() {
                                view.layer = i;
                            }
                            if tr_resp.drag_started()
                                && let Some(pos) = ui
                                    .input(|inp| inp.pointer.press_origin())
                                    .or(tr_resp.interact_pointer_pos())
                            {
                                view.layer = i;
                                let edge = 7.0;
                                let mode = if (pos.x - clip_rect.right()).abs() <= edge {
                                    ClipDrag::Right { orig_len: l.length }
                                } else if (pos.x - clip_rect.left()).abs() <= edge {
                                    ClipDrag::Left {
                                        orig_start: l.start,
                                        orig_end: l.start + l.length,
                                    }
                                } else {
                                    ClipDrag::Move {
                                        orig_start: l.start,
                                    }
                                };
                                ui.data_mut(|d| d.insert_temp(drag_id, Some((pos.x, mode))));
                                ed.started = true;
                            }
                            if tr_resp.dragged()
                                && let Some((origin_x, mode)) = ui
                                    .data(|d| d.get_temp::<Option<(f32, ClipDrag)>>(drag_id))
                                    .flatten()
                                && let Some(pos) = tr_resp.interact_pointer_pos()
                            {
                                let dt = (pos.x - origin_x) / inner.width().max(1.0) * view_span;
                                match mode {
                                    ClipDrag::Move { orig_start } => {
                                        l.start =
                                            (orig_start + dt).clamp(0.0, MAX_SECONDS - l.length);
                                    }
                                    ClipDrag::Left {
                                        orig_start,
                                        orig_end,
                                    } => {
                                        let ns = (orig_start + dt).clamp(0.0, orig_end - 0.01);
                                        l.start = ns;
                                        l.length = (orig_end - ns).clamp(0.01, MAX_SECONDS - ns);
                                    }
                                    ClipDrag::Right { orig_len } => {
                                        l.length =
                                            (orig_len + dt).clamp(0.01, MAX_SECONDS - l.start);
                                    }
                                }
                                ed.changed = true;
                            }
                            if tr_resp.drag_stopped() {
                                ui.data_mut(|d| d.remove::<Option<(f32, ClipDrag)>>(drag_id));
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("×")
                                            .font(t::mono(11.0))
                                            .color(t::muted()),
                                    )
                                    .min_size(vec2(del_w, 20.0)),
                                )
                                .on_hover_text(tr.forge_delete_layer_tip)
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
            });
        }

        if let Some(i) = remove {
            p.layers.remove(i);
            view.layer = view.layer.min(p.layers.len().saturating_sub(1));
            ed.started = true;
            ed.changed = true;
        }
        if let Some((into, b)) = jobs.now {
            let channel = jobs.name(b.channel);
            let note = crate::ui::roll::note_name(b.key);
            let text = match into {
                Into::NewLayer => fill(
                    tr.forge_bouncing_new,
                    &[("channel", channel), ("note", &note)],
                ),
                Into::Layer(i) => fill(
                    tr.forge_bouncing_layer,
                    &[
                        ("channel", channel),
                        ("note", &note),
                        ("n", &format!("{:02}", i + 1)),
                    ],
                ),
            };
            ui.label(egui::RichText::new(text).color(t::wait()));
        }
        if p.layers.is_empty() {
            ui.label(egui::RichText::new(tr.forge_layers_empty).color(t::muted()));
        }

        // Accept WAV/FLAC/OGG drag-and-drop from the DAW Files browser onto the timeline.
        let drop_zone = ui.interact(
            ui.min_rect(),
            ui.id().with(("forge-tl-drop", id)),
            Sense::hover(),
        );
        if let Some(drag) = drop_zone.dnd_release_payload::<crate::ui::files::FileDrag>() {
            out.drop_file = Some(drag.0.clone());
        }

        // Timeline footer: + Layer menu, + Layer Block, Duplicate, Layer counter.
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            let full = p.layers.len() >= MAX_LAYERS;
            ui.add_enabled_ui(!full, |ui| {
                ui.menu_button(tr.forge_add_layer, |ui| {
                    kit::menu_style(ui);
                    let new = if ui.button(tr.forge_src_osc).clicked() {
                        Some(Layer::new("Tone", Source::osc(Wave::Square, 440.0), 0.3))
                    } else if ui.button(tr.forge_src_modal).clicked() {
                        Some(
                            Layer::new("Strike", Source::modal(ModalMaterial::Metal, 480.0), 0.35)
                                .with_amp(Curve::flat(1.0)),
                        )
                    } else if ui.button(tr.forge_src_grain).clicked() {
                        Some(
                            Layer::new("Texture", Source::grain(750.0, 40.0, 24.0), 0.5)
                                .with_amp(Curve::flat(1.0)),
                        )
                    } else if ui.button(tr.forge_src_noise).clicked() {
                        Some(Layer::new(
                            "Noise",
                            Source::noise(NoiseColor::White, 4000.0),
                            0.3,
                        ))
                    } else if ui.button(tr.forge_src_sample_menu).clicked() {
                        let mut l = Layer::new("Sample", Source::osc(Wave::Sine, 440.0), 0.3);
                        l.amp = Curve::flat(1.0);
                        out.load_sample = Some(p.layers.len());
                        Some(l)
                    } else if !jobs.channels.is_empty()
                        && let Some(ch) = ui
                            .menu_button(tr.forge_src_channel_note, |ui| {
                                kit::menu_style(ui);
                                let mut pick = None;
                                for (cid, name) in jobs.channels {
                                    if ui
                                        .button(name)
                                        .on_hover_text(tr.forge_src_channel_note_tip)
                                        .clicked()
                                    {
                                        pick = Some(*cid);
                                    }
                                }
                                pick
                            })
                            .inner
                            .flatten()
                    {
                        out.bounce = Some((
                            Into::NewLayer,
                            forge::Bounce {
                                channel: ch,
                                key: 60,
                                hold: 0.5,
                            },
                        ));
                        ui.close_menu();
                        None
                    } else if ui.button(tr.forge_src_tide_note).clicked() {
                        Some(Layer::new(
                            "Synth",
                            Source::Synth {
                                patch: Box::new(keyframexl::synth::SynthPatch::factory(2)),
                                key: 60,
                            },
                            0.5,
                        ))
                    } else {
                        None
                    };
                    if let Some(l) = new {
                        p.layers.push(l);
                        view.layer = p.layers.len() - 1;
                        ed.started = true;
                        ed.changed = true;
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text(if full {
                    tr.forge_layers_max_tip
                } else {
                    tr.forge_add_layer_tip
                });
                if ui
                    .button(tr.forge_add_block_btn)
                    .on_hover_text(tr.forge_add_block_tip)
                    .clicked()
                {
                    view.tab = Tab::Presets;
                    view.preset_sub = PresetSub::Blocks;
                }
                if !p.layers.is_empty()
                    && ui
                        .button(tr.forge_duplicate)
                        .on_hover_text(tr.forge_duplicate_tip)
                        .clicked()
                {
                    let mut l = p.layers[view.layer].clone();
                    l.name = fill(tr.forge_layer_copy_suffix, &[("name", &l.name)]);
                    p.layers.insert(view.layer + 1, l);
                    view.layer += 1;
                    ed.started = true;
                    ed.changed = true;
                }
            });

            ui.label(
                egui::RichText::new(fill(
                    tr.forge_layer_count,
                    &[
                        ("n", &p.layers.len().to_string()),
                        ("max", &MAX_LAYERS.to_string()),
                    ],
                ))
                .font(t::mono(10.0))
                .color(t::faint()),
            )
            .on_hover_text(tr.forge_timeline_hint);
        });
    });

    out
}
