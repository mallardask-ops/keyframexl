//! Tap tempo window: tap with the mouse, the keyboard or any MIDI note.

use super::*;
use crate::i18n::fill;
use eframe::egui::{Align2, Rect, RichText, Stroke, pos2, vec2};
use keyframexl::tap_tempo::{AvgWindow, Rounding, TapClock, TapMeter};
use std::time::Instant;

/// Open tap tempo window: the meter and which inputs tap.
pub(crate) struct TapWindow {
    pub meter: TapMeter,
    /// Wall clock zero for every input, so mouse, keys and MIDI share one timeline.
    epoch: Instant,
    pub auto_apply: bool,
    pub keys: bool,
    pub midi: bool,
    /// The run of taps already made its one undo step.
    checkpointed: bool,
    pedal_down: bool,
}

impl TapWindow {
    pub fn new() -> Self {
        Self {
            meter: TapMeter::new(),
            epoch: Instant::now(),
            auto_apply: true,
            keys: true,
            midi: true,
            checkpointed: false,
            pedal_down: false,
        }
    }

    /// For screenshots: a run at about 124 BPM with one stray tap, just played.
    pub fn demo() -> Self {
        let mut w = Self::new();
        w.auto_apply = false;
        // Still shown when the screenshot is taken seconds later.
        w.meter.timeout_s = 3600.0;
        let now = w.secs(Instant::now()) + 4.0;
        let gaps = [0.48, 0.49, 0.47, 0.62, 0.48, 0.49, 0.48];
        let mut at = now - gaps.iter().sum::<f64>();
        w.meter.tap(at);
        for g in gaps {
            at += g;
            w.meter.tap(at);
        }
        w.epoch -= std::time::Duration::from_secs(4);
        w
    }

    fn secs(&self, at: Instant) -> f64 {
        at.saturating_duration_since(self.epoch).as_secs_f64()
    }
}

/// Keys that tap while the window is open.
const TAP_KEYS: [egui::Key; 3] = [egui::Key::Space, egui::Key::Enter, egui::Key::T];

impl App {
    /// Toolbar Tap and Tools → Tap tempo: open the window (the click is the first tap).
    pub(crate) fn tap(&mut self) {
        if self.tap_win.is_none() {
            self.tap_win = Some(TapWindow::new());
        }
        self.tap_at(TapClock::Ui, None, Instant::now());
    }

    /// One tap. `clock_s` is the source's own time (MIDI driver stamp), if any.
    fn tap_at(&mut self, clock: TapClock, clock_s: Option<f64>, at: Instant) {
        let Some(w) = &mut self.tap_win else { return };
        let wall = w.secs(at);
        let r = w.meter.tap_with_clock(clock, clock_s.unwrap_or(wall), wall);
        if r.started_new_run {
            w.checkpointed = false;
        }
        if r.accepted && w.auto_apply && w.meter.tap_count() >= 3 {
            self.apply_tap_tempo();
        }
    }

    fn apply_tap_tempo(&mut self) {
        let tr = self.tr();
        let Some(w) = &mut self.tap_win else { return };
        let Some(bpm) = w.meter.bpm() else { return };
        // One undo step per run of taps: the first tempo it sets.
        let first = !w.checkpointed;
        w.checkpointed = true;
        if (self.edited_tempo() - bpm).abs() < 1e-6 {
            return;
        }
        if first {
            self.checkpoint();
        }
        self.set_edited_tempo(bpm);
        self.publish();
        let bpm_s = format!("{bpm:.1}");
        self.say(
            Tone::Quiet,
            fill(tr.status_tempo_from_taps, &[("bpm", &bpm_s)]),
        );
    }

    /// Before the shortcuts: tap keys go to the window, not to play / stop.
    pub(crate) fn tap_keys(&mut self, ctx: &egui::Context) {
        let Some(w) = &self.tap_win else { return };
        if ctx.wants_keyboard_input() {
            return;
        }
        let keys = w.keys;
        let mut taps = 0;
        let mut close = false;
        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if modifiers.is_none() => {
                    if *key == egui::Key::Escape {
                        close |= *pressed;
                        return false;
                    }
                    if keys && TAP_KEYS.contains(key) {
                        if *pressed && !*repeat {
                            taps += 1;
                        }
                        return false;
                    }
                    true
                }
                // Space also arrives as text; it must not type into anything.
                egui::Event::Text(t) if keys && t == " " => false,
                _ => true,
            })
        });
        let now = Instant::now();
        for _ in 0..taps {
            self.tap_at(TapClock::Ui, None, now);
        }
        if close {
            self.tap_win = None;
        }
    }

    /// From `midi_frame`: note-ons and sustain pedal presses tap.
    pub(crate) fn tap_midi(&mut self, events: &[crate::midi_in::Incoming]) {
        let Some(w) = &mut self.tap_win else { return };
        if !w.midi {
            return;
        }
        let mut taps = Vec::new();
        for inc in events {
            let hit = match inc.event {
                crate::midi_in::Event::On { velocity, .. } => velocity > 0,
                crate::midi_in::Event::Control { cc: 64, value, .. } => {
                    let down = value >= 64;
                    let press = down && !w.pedal_down;
                    w.pedal_down = down;
                    press
                }
                _ => false,
            };
            if hit {
                let clock = (inc.stamp_us > 0).then(|| {
                    (
                        TapClock::MidiPort(inc.port.clone()),
                        inc.stamp_us as f64 / 1e6,
                    )
                });
                taps.push((clock, inc.received_at));
            }
        }
        for (clock, at) in taps {
            match clock {
                Some((c, s)) => self.tap_at(c, Some(s), at),
                None => self.tap_at(TapClock::Ui, None, at),
            }
        }
    }

    pub(crate) fn tap_window(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        let tempo = self.edited_tempo();
        let Some(w) = &mut self.tap_win else { return };
        let now = w.secs(Instant::now());
        if w.meter.expire(now) {
            w.checkpointed = false;
        }
        let flash = w.meter.beat_flash(now);
        let bpm = w.meter.bpm();
        let count = w.meter.tap_count();
        let samples = w.meter.recent_samples();
        let mut open = true;
        let mut pad_tap = false;
        let mut apply = false;
        let mut reset = false;
        egui::Window::new(tr.tools_tap_tempo)
            .id(egui::Id::new("tap-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(egui::pos2(ctx.screen_rect().center().x - 170.0, 120.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                let width = 340.0;
                // The pad: big BPM, lights on every tap.
                let (pad, resp) = ui.allocate_exact_size(vec2(width, 150.0), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(pad, 8.0, t::sunken());
                if flash > 0.0 {
                    p.rect_filled(pad, 8.0, t::brand().gamma_multiply(0.35 * flash));
                }
                let edge = if resp.hovered() {
                    t::focus()
                } else {
                    t::edge()
                };
                p.rect_stroke(
                    pad,
                    8.0,
                    Stroke::new(1.0_f32, edge),
                    egui::StrokeKind::Inside,
                );
                let (big, ink) = match bpm {
                    Some(b) => (w.meter.rounding.format(b), t::text()),
                    None => (tr.tap_pad_empty.to_string(), t::muted()),
                };
                p.text(
                    pad.center() - vec2(0.0, 14.0),
                    Align2::CENTER_CENTER,
                    big,
                    t::mono(46.0),
                    ink,
                );
                p.text(
                    pad.center_bottom() - vec2(0.0, 22.0),
                    Align2::CENTER_CENTER,
                    fill(tr.tap_taps, &[("n", &count.to_string())]),
                    t::sans(12.0),
                    t::faint(),
                );
                if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
                    pad_tap = true;
                }
                resp.on_hover_text(tr.tap_pad_hint);

                // Recent intervals as bars around the average; outliers dimmed.
                let (strip, _) = ui.allocate_exact_size(vec2(width, 28.0), egui::Sense::hover());
                if let Some(avg) = bpm {
                    let n = samples.len().max(1) as f32;
                    let bw = (strip.width() / n).min(20.0);
                    for (k, s) in samples.iter().enumerate() {
                        let dev = ((s.bpm - avg) / avg).clamp(-0.2, 0.2) as f32 / 0.2;
                        let h = 3.0 + dev.abs() * 11.0;
                        let x = strip.left() + k as f32 * bw;
                        let mid = strip.center().y;
                        let r = if dev >= 0.0 {
                            Rect::from_min_max(pos2(x + 2.0, mid - h), pos2(x + bw - 2.0, mid))
                        } else {
                            Rect::from_min_max(pos2(x + 2.0, mid), pos2(x + bw - 2.0, mid + h))
                        };
                        let c = if s.outlier { t::faint() } else { t::brand() };
                        ui.painter().rect_filled(r, 2.0, c);
                    }
                }
                ui.label(
                    RichText::new(fill(
                        tr.tap_project_tempo,
                        &[("bpm", &format!("{tempo:.1}"))],
                    ))
                    .font(t::sans(12.0))
                    .color(t::muted()),
                );
                ui.separator();

                // A label column, so the chips line up.
                let row = |ui: &mut egui::Ui, label: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(
                        vec2(width, kit::TOOL_H + 6.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let (r, resp) = ui.allocate_exact_size(
                                vec2(110.0, kit::TOOL_H),
                                egui::Sense::hover(),
                            );
                            ui.painter().text(
                                r.left_center(),
                                Align2::LEFT_CENTER,
                                label,
                                t::sans(13.0),
                                t::text(),
                            );
                            body(ui);
                            resp
                        },
                    )
                    .inner
                };
                row(ui, tr.tap_average, &mut |ui| {
                    let names = ["4", "8", "16"];
                    let (pick, _) =
                        kit::chips(ui, "tap-average", &names, w.meter.window.index(), None);
                    if let Some(i) = pick {
                        w.meter.window = AvgWindow::from_index(i);
                    }
                })
                .on_hover_text(tr.tap_average_tip);
                row(ui, tr.tap_rounding, &mut |ui| {
                    let names = [tr.tap_round_whole, "0.1"];
                    let (pick, _) =
                        kit::chips(ui, "tap-rounding", &names, w.meter.rounding.index(), None);
                    if let Some(i) = pick {
                        w.meter.rounding = Rounding::from_index(i);
                    }
                });
                ui.checkbox(
                    &mut w.auto_apply,
                    RichText::new(tr.tap_auto_apply).font(t::sans(13.0)),
                );
                ui.label(
                    RichText::new(tr.tap_inputs)
                        .font(t::sans(12.0))
                        .color(t::faint()),
                );
                ui.checkbox(&mut w.keys, RichText::new(tr.tap_keys).font(t::sans(13.0)))
                    .on_hover_text(tr.tap_keys_tip);
                ui.checkbox(&mut w.midi, RichText::new(tr.tap_midi).font(t::sans(13.0)))
                    .on_hover_text(tr.tap_midi_tip);
                ui.separator();
                ui.horizontal(|ui| {
                    let can = bpm.is_some_and(|b| (b - tempo).abs() > 1e-6);
                    if kit::button(ui, tr.tap_apply, kit::Kind::Primary, can).clicked() {
                        apply = true;
                    }
                    if kit::button(ui, tr.tap_reset, kit::Kind::Normal, count > 0).clicked() {
                        reset = true;
                    }
                });
            });
        if pad_tap {
            self.tap_at(TapClock::Ui, None, Instant::now());
        }
        if apply {
            self.apply_tap_tempo();
        }
        if reset && let Some(w) = &mut self.tap_win {
            w.meter.reset();
            w.checkpointed = false;
        }
        if !open {
            self.tap_win = None;
        }
        if self.tap_win.is_some() {
            // The flash fades and the run times out without input.
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
}
