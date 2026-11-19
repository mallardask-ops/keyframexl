//! Deck: the instrument's top plate under the main menu. Row 1: transport
//! keys, Pattern/Song, the LCD (position, tempo with ▲▼), Tap, meters.
//! Row 2: window buttons, snap, pattern selector, hint bar. One red control
//! on screen: Play.

use super::{App, Cmd, Kind, PAGE, Snap, Tab, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align, Align2, Layout, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

pub const ROW1_H: f32 = 48.0;
pub const ROW2_H: f32 = 34.0;

impl App {
    /// Two rows under the main menu (FL: transport panel, then the window /
    /// snap / pattern / hint row).
    pub(super) fn deck(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, t::SIDE);
        p.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0_f32, t::EDGE),
        );
        let row1 = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + ROW1_H));
        let row2 = Rect::from_min_max(pos2(rect.left(), row1.bottom()), rect.max);
        p.line_segment(
            [
                pos2(rect.left() + PAGE, row2.top()),
                pos2(rect.right() - PAGE, row2.top()),
            ],
            Stroke::new(1.0_f32, t::LINE),
        );

        let r1 = Rect::from_min_max(
            pos2(rect.left() + PAGE, row1.top()),
            pos2(row1.right() - PAGE, row1.bottom()),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(r1)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                self.transport_keys(ui);
                ui.add_space(8.0);
                let (pick, _) = kit::chips(
                    ui,
                    "mode",
                    &["Pattern", "Song"],
                    self.song_mode as usize,
                    None,
                );
                if let Some(i) = pick {
                    self.set_song_mode(i == 1);
                }
                ui.add_space(8.0);
                self.lcd(ui);
                let r = kit::button(ui, "Tap", kit::Kind::Normal, true);
                if kit::bar_hint(r, "Tap tempo: click three or more times on the beat").clicked() {
                    self.tap();
                }
                ui.add_space(4.0);
                self.transport_toggles(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.meters(ui));
            },
        );

        let r2 = Rect::from_min_max(
            pos2(rect.left() + PAGE, row2.top()),
            pos2(row2.right() - PAGE, row2.bottom()),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(r2)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                self.window_buttons(ui);
                ui.add_space(10.0);
                self.snap_selector(ui);
                ui.add_space(4.0);
                self.pattern_selector(ui);
                ui.add_space(12.0);
                // Hint bar (FL): what the control under the pointer does; with
                // nothing hovered, how to use the focused window. Results go to the status line.
                // Fallback stays short (cut at the second " · "); the full text
                // lives in the hover tooltip. Elided hover hints tooltip too.
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), kit::TOOL_H), Sense::hover());
                let (full, display, color) = match kit::hovered_hint(ui.ctx()) {
                    Some(h) => (h.clone(), h, t::MUTED),
                    None => {
                        let full = self.hint().to_string();
                        let mut seen = 0;
                        let mut cut = full.len();
                        for (i, _) in full.match_indices(" · ") {
                            seen += 1;
                            if seen == 2 {
                                cut = i;
                                break;
                            }
                        }
                        let short = if seen >= 2 {
                            full[..cut].to_string()
                        } else {
                            full.clone()
                        };
                        (full, short, t::FAINT)
                    }
                };
                let shown = kit::elide(ui.painter(), &display, t::sans(12.0), rect.width());
                ui.painter().text(
                    pos2(rect.left(), rect.center().y),
                    Align2::LEFT_CENTER,
                    &shown,
                    t::sans(12.0),
                    color,
                );
                if shown != display || display != full {
                    resp.on_hover_text(full);
                }
            },
        );
    }

    /// FL: one button per window with an open lamp. Click opens or focuses,
    /// the F-key toggles.
    fn window_buttons(&mut self, ui: &mut Ui) {
        let roll = Tab::Roll {
            instrument: self.instrument,
        };
        let focus = self.focus_kind();
        let font = t::sans(13.0);
        let button =
            |ui: &mut Ui,
             label: &str,
             lit: bool,
             current: bool,
             icon: &dyn Fn(&egui::Painter, egui::Pos2, egui::Color32)| {
                // FL: lamp, icon, name.
                let w = 8.0
                    + 4.0
                    + 6.0
                    + 14.0
                    + 6.0
                    + kit::text_width(ui.painter(), label, font.clone())
                    + 10.0;
                kit::button_sized(
                    ui,
                    vec2(w, kit::TOOL_H),
                    kit::Kind::Normal,
                    true,
                    label,
                    |p, face, ink| {
                        let ic = if current {
                            t::TEXT
                        } else if lit {
                            ink
                        } else {
                            t::FAINT
                        };
                        kit::led(
                            p,
                            pos2(face.left() + 10.0, face.center().y),
                            2.0,
                            lit.then_some(t::TEXT),
                        );
                        icon(p, pos2(face.left() + 18.0 + 7.0, face.center().y - 1.0), ic);
                        p.text(
                            pos2(face.left() + 18.0 + 14.0 + 6.0, face.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            font.clone(),
                            if lit { ink } else { t::MUTED },
                        );
                        if current {
                            p.line_segment(
                                [
                                    pos2(face.left() + 8.0, face.bottom() - 2.0),
                                    pos2(face.right() - 8.0, face.bottom() - 2.0),
                                ],
                                Stroke::new(1.5_f32, t::TEXT),
                            );
                        }
                    },
                )
            };
        for (kind, key) in super::menu::WINDOW_KEYS {
            let tab = match kind {
                Kind::Playlist => Tab::Playlist,
                Kind::Rack => Tab::Rack,
                Kind::Mixer => Tab::Mixer,
                _ => roll.clone(),
            };
            let open = self
                .ws
                .dock()
                .iter_all_tabs()
                .any(|(_, t)| t.kind() == kind);
            let current = focus == Some(kind);
            let label = match kind {
                Kind::Playlist => "Playlist",
                Kind::Rack => "Rack",
                Kind::Mixer => "Mixer",
                _ => "Roll",
            };
            let r = button(ui, label, open, current, &|p, c, col| match kind {
                Kind::Playlist => kit::icon_playlist(p, c, col),
                Kind::Rack => kit::icon_rack(p, c, col),
                Kind::Mixer => kit::icon_mixer(p, c, col),
                _ => kit::icon_roll(p, c, col),
            });
            let r = kit::bar_hint(
                r,
                format!(
                    "{}  {key}{}",
                    kind.title(),
                    if open { " · open" } else { "" }
                ),
            );
            if r.clicked() {
                self.win(Cmd::Toggle(tab));
            }
        }
        let on = self.ws.browser;
        let r = button(ui, "Browser", on, false, &|p, c, col| {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(14.0, 11.0)),
                1.5,
                Stroke::new(1.5_f32, col),
                egui::StrokeKind::Inside,
            );
            p.line_segment(
                [c + vec2(-2.0, -5.5), c + vec2(-2.0, 5.5)],
                Stroke::new(1.5_f32, col),
            );
        });
        if kit::bar_hint(r, "Browser  Alt+F8").clicked() {
            self.ws.browser = !self.ws.browser;
        }
    }

    /// Global snap (FL: main snap). The piano roll follows it.
    fn snap_selector(&mut self, ui: &mut Ui) {
        let label = format!("Snap  {}", self.snap.label());
        let mut pick = None;
        let r = kit::dropdown(ui, "snap", &label, 110.0, |ui| {
            for s in Snap::ALL {
                if ui
                    .selectable_label(
                        s == self.snap,
                        egui::RichText::new(s.label()).font(t::sans(13.0)),
                    )
                    .clicked()
                {
                    pick = Some(s);
                }
            }
        });
        kit::bar_hint(
            r,
            "Grid for drawing and moving notes. Off places notes freely. Alt skips snap while dragging.",
        );
        if let Some(s) = pick {
            self.snap = s;
        }
    }

    /// FL pattern selector: − previous, the list, + next (a new one after the last).
    fn pattern_selector(&mut self, ui: &mut Ui) {
        let idx = self.pattern_index(self.selected);
        let prev = prev_note_pattern(&self.song.patterns, idx);
        let next = next_note_pattern(&self.song.patterns, idx);
        let r = kit::button_sized(
            ui,
            vec2(kit::TOOL_H, kit::TOOL_H),
            kit::Kind::Normal,
            prev.is_some(),
            "−",
            |p, face, ink| {
                p.text(
                    face.center(),
                    Align2::CENTER_CENTER,
                    "−",
                    t::sans(13.0),
                    ink,
                );
            },
        );
        if kit::bar_hint(r, "Previous pattern").clicked()
            && let Some(j) = prev
        {
            let id = self.song.patterns[j].id;
            self.select_pattern(id);
        }
        let mut pick = None;
        let name = self.pattern().name.clone();
        let r = kit::dropdown(
            ui,
            "pattern",
            &format!("{:02}  {name}", idx + 1),
            170.0,
            |ui| {
                // Automation clips are patterns too, but they live in the playlist only (FL).
                for (i, p) in self
                    .song
                    .patterns
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.automation.is_none())
                {
                    if ui
                        .selectable_label(
                            p.id == self.selected,
                            egui::RichText::new(format!("{:02}  {}", i + 1, p.name))
                                .font(t::sans(13.0)),
                        )
                        .clicked()
                    {
                        pick = Some(p.id);
                    }
                }
            },
        );
        // Double-click renames the active pattern (same as F2).
        if r.double_clicked() {
            ui.memory_mut(|m| m.close_popup());
            self.renaming = Some((self.selected, name.clone()));
        }
        // Mouse wheel over the pattern selector switches patterns (FL style).
        let wheel_hit = r.hovered() && !ui.memory(|m| m.any_popup_open());
        if wheel_hit {
            let dy = ui.input_mut(|i| {
                let d = if i.raw_scroll_delta.y != 0.0 {
                    i.raw_scroll_delta.y
                } else {
                    i.smooth_scroll_delta.y
                };
                if d != 0.0 {
                    i.raw_scroll_delta = egui::Vec2::ZERO;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                }
                d
            });
            if dy != 0.0 {
                let acc_id = ui.make_persistent_id("pattern_wheel_acc");
                let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
                // Reset accumulator on direction reversal.
                if acc * dy < 0.0 {
                    acc = 0.0;
                }
                acc += dy;
                // A single mouse-wheel notch is typically 20–50 px; fire promptly
                // on a notch while smoothing trackpad gestures.
                let step = if acc.abs() >= 18.0 {
                    let s = acc.signum() as i32;
                    acc = 0.0;
                    s
                } else {
                    0
                };
                ui.data_mut(|d| d.insert_temp(acc_id, acc));
                if step > 0 {
                    if let Some(j) = next {
                        pick = Some(self.song.patterns[j].id);
                    }
                } else if step < 0 {
                    if let Some(j) = prev {
                        pick = Some(self.song.patterns[j].id);
                    }
                }
            }
        }
        kit::bar_hint(
            r,
            "Pattern to edit and play. Scroll to switch, double-click or F2 renames, F4 jumps to the next empty one.",
        );
        if let Some(id) = pick {
            self.select_pattern(id);
        }
        let r = kit::button_sized(
            ui,
            vec2(kit::TOOL_H, kit::TOOL_H),
            kit::Kind::Normal,
            true,
            "+",
            |p, face, ink| {
                p.text(
                    face.center(),
                    Align2::CENTER_CENTER,
                    "+",
                    t::sans(13.0),
                    ink,
                );
            },
        );
        if kit::bar_hint(
            r,
            if next.is_some() {
                "Next pattern"
            } else {
                "New pattern"
            },
        )
        .clicked()
        {
            if let Some(j) = next {
                let id = self.song.patterns[j].id;
                self.select_pattern(id);
            } else {
                self.new_pattern();
            }
        }
    }

    /// CPU load of the audio callback and sounding voices (FL: CPU / polyphony).
    fn meters(&mut self, ui: &mut Ui) {
        let (text, tip) = match &self.audio {
            Some(a) => {
                let load = a.load.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10.0;
                let voices = a.voices.load(std::sync::atomic::Ordering::Relaxed);
                (
                    format!("cpu {load:>3.0}%   voices {voices:>3}"),
                    "Audio thread load and sounding voices",
                )
            }
            None => (
                "no audio".to_string(),
                "No audio output: check PipeWire, then restart",
            ),
        };
        let g = kit::caps_galley(ui.painter(), &text, 12.0, t::MUTED);
        let (r, resp) = ui.allocate_exact_size(g.size(), Sense::hover());
        ui.painter().galley(r.min, g, t::MUTED);
        kit::bar_hint(resp, tip);
        if let Some(a) = &self.audio {
            use std::sync::atomic::Ordering;
            let count = a.overruns.load(Ordering::Relaxed);
            let now = ui.input(|i| i.time);
            let id = ui.make_persistent_id("audio_overload_lamp");
            let (last, mut changed_at) = ui
                .data_mut(|d| d.get_temp::<(u32, Option<f64>)>(id))
                .unwrap_or((0, None));
            if count != last {
                changed_at = Some(now);
            }
            let lit = changed_at.is_some_and(|time| now - time < 2.0);
            if lit {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        2.0 - (now - changed_at.unwrap()),
                    ));
            }
            let (lamp, response) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::click());
            ui.painter()
                .circle_filled(lamp.center(), 3.5, if lit { t::BAD } else { t::LCD_OFF });
            let response = response.on_hover_text(format!(
                "Audio overloads: {count} since start. Click to reset."
            ));
            if response.clicked() {
                a.overruns.store(0, Ordering::Relaxed);
                ui.data_mut(|d| d.insert_temp(id, (0_u32, None::<f64>)));
            } else {
                ui.data_mut(|d| d.insert_temp(id, (count, changed_at)));
            }
        }
        self.deck_monitor(ui);
    }

    /// Recessed Master bus oscilloscope / spectrum display in the transport deck.
    fn deck_monitor(&mut self, ui: &mut Ui) {
        use crate::settings::DeckMonitor;
        use std::sync::atomic::Ordering;

        let mode = self.settings.general.deck_monitor;
        let Some(a) = &self.audio else { return };
        if mode == DeckMonitor::Off {
            a.master_scope.track.store(usize::MAX, Ordering::Relaxed);
            return;
        }
        a.master_scope.track.store(0, Ordering::Relaxed);

        ui.add_space(6.0);
        let size = vec2(110.0, 32.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 4.0, t::SUNKEN);
        p.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, t::EDGE),
            egui::StrokeKind::Inside,
        );
        let inner = rect.shrink2(vec2(3.0, 3.0));
        // Subtle baseline / grid.
        let mid_y = inner.center().y;
        p.line_segment(
            [pos2(inner.left(), mid_y), pos2(inner.right(), mid_y)],
            Stroke::new(1.0_f32, t::LCD_OFF),
        );

        let mut buf = [0.0f32; 1024];
        a.master_scope.read(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        if self.is_playing() || peak > 1e-4 {
            ui.ctx().request_repaint();
        }

        match mode {
            DeckMonitor::Scope => {
                // Rising zero-crossing trigger in the first half keeps periodic waves steady.
                let mut start = 0usize;
                if peak > 1e-3 {
                    for i in 1..512 {
                        if buf[i - 1] <= 0.0 && buf[i] > 0.0 {
                            start = i;
                            break;
                        }
                    }
                }
                let span = 512usize;
                let step = 4usize;
                let count = span / step;
                let mut pts = Vec::with_capacity(count);
                let half_h = inner.height() * 0.46;
                for i in 0..count {
                    let s = buf[start + i * step].clamp(-1.0, 1.0);
                    let x = inner.left() + inner.width() * (i as f32 / (count - 1).max(1) as f32);
                    let y = mid_y - s * half_h;
                    pts.push(pos2(x, y));
                }
                let color = if peak > 0.99 { t::BAD } else { t::OK };
                p.add(Shape::line(pts, Stroke::new(1.25_f32, color)));
            }
            DeckMonitor::Spectrum => {
                let db = super::fx_views::spectrum_db(&buf);
                let rate = a.running.rate.max(8_000) as f32;
                let nyq = rate * 0.5;
                let bins = db.len().max(1);
                const BARS: usize = 22;
                let f_lo = 40.0f32;
                let f_hi = nyq.min(18_000.0).max(f_lo * 2.0);
                let ratio = f_hi / f_lo;

                let cache_id = ui.make_persistent_id("deck_spec_bars");
                let mut held: [f32; BARS] = ui
                    .data_mut(|d| d.get_temp(cache_id))
                    .unwrap_or([0.0; BARS]);

                let gap = 1.5;
                let bw = ((inner.width() - gap * (BARS - 1) as f32) / BARS as f32).max(1.5);
                for (i, slot) in held.iter_mut().enumerate() {
                    let f0 = f_lo * ratio.powf(i as f32 / BARS as f32);
                    let f1 = f_lo * ratio.powf((i + 1) as f32 / BARS as f32);
                    let k0 = ((f0 / nyq) * bins as f32).floor() as usize;
                    let k1 = (((f1 / nyq) * bins as f32).ceil() as usize)
                        .max(k0 + 1)
                        .min(bins);
                    let mut m = -96.0f32;
                    for &v in &db[k0.min(bins - 1)..k1] {
                        m = m.max(v);
                    }
                    let norm = ((m + 60.0) / 60.0).clamp(0.0, 1.0);
                    *slot = norm.max(*slot * 0.82);
                    if *slot < 0.004 {
                        *slot = 0.0;
                    }
                    let h = (*slot * inner.height()).max(1.0);
                    let x = inner.left() + i as f32 * (bw + gap);
                    let bar = Rect::from_min_max(
                        pos2(x, inner.bottom() - h),
                        pos2(x + bw, inner.bottom()),
                    );
                    let col = if *slot > 0.92 {
                        t::BAD
                    } else if *slot > 0.78 {
                        t::WAIT
                    } else {
                        t::ENC_BLUE
                    };
                    p.rect_filled(bar, 1.0, col);
                }
                if held.iter().any(|&v| v > 0.0) {
                    ui.ctx().request_repaint();
                }
                ui.data_mut(|d| d.insert_temp(cache_id, held));
            }
            DeckMonitor::Off => {}
        }

        let tip = match mode {
            DeckMonitor::Scope => "Master oscilloscope · click for spectrum, right-click for mode",
            DeckMonitor::Spectrum => "Master spectrum · click for oscilloscope, right-click for mode",
            DeckMonitor::Off => "",
        };
        let resp = kit::bar_hint(resp, tip);
        if resp.clicked() {
            let next = match mode {
                DeckMonitor::Scope => DeckMonitor::Spectrum,
                DeckMonitor::Spectrum => DeckMonitor::Scope,
                DeckMonitor::Off => DeckMonitor::Scope,
            };
            self.set_deck_monitor(next);
        }
        resp.context_menu(|ui| {
            kit::menu_style(ui);
            for opt in DeckMonitor::ALL {
                if ui
                    .selectable_label(mode == opt, opt.label())
                    .clicked()
                {
                    self.set_deck_monitor(opt);
                    ui.close_menu();
                }
            }
        });
    }

    pub(super) fn set_deck_monitor(&mut self, mode: crate::settings::DeckMonitor) {
        self.settings.general.deck_monitor = mode;
        if mode == crate::settings::DeckMonitor::Off
            && let Some(a) = &self.audio
        {
            a.master_scope
                .track
                .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
        }
        self.save_settings();
    }

    /// Metronome, count-in and wait-for-input (FL's three small keys by the
    /// transport). A lit key has a green lamp.
    fn transport_toggles(&mut self, ui: &mut Ui) {
        let size = vec2(34.0, 34.0);
        let counting = self
            .audio
            .as_ref()
            .is_some_and(|a| a.count_in.load(std::sync::atomic::Ordering::Relaxed) > 0);
        let g = self.settings.general.clone();
        let lamp = |p: &egui::Painter, face: Rect, on: bool| {
            kit::led(
                p,
                face.right_top() + vec2(-6.0, 6.0),
                2.5,
                on.then_some(t::OK),
            )
        };
        // Metronome: the pyramid and its arm.
        let r = kit::button_sized(
            ui,
            size,
            kit::Kind::Normal,
            true,
            "Metronome",
            |p, face, ink| {
                let c = face.center() + vec2(0.0, 1.0);
                let ink = if g.metronome { ink } else { t::FAINT };
                let s = Stroke::new(1.5_f32, ink);
                p.add(Shape::closed_line(
                    vec![
                        c + vec2(-4.0, -9.0),
                        c + vec2(4.0, -9.0),
                        c + vec2(8.0, 9.0),
                        c + vec2(-8.0, 9.0),
                    ],
                    s,
                ));
                p.line_segment([c + vec2(0.0, 6.0), c + vec2(6.0, -7.0)], s);
                lamp(p, face, g.metronome);
            },
        );
        if kit::bar_hint(
            r,
            if g.metronome {
                "Metronome on: clicks every beat while playing  Ctrl+M"
            } else {
                "Metronome: click every beat while playing  Ctrl+M"
            },
        )
        .clicked()
        {
            self.toggle_metronome();
        }
        // Count-in: a stopwatch; it blinks while counting.
        let blink = counting && (ui.input(|i| i.time) * 4.0) as i64 % 2 == 0;
        let r = kit::button_sized(
            ui,
            size,
            kit::Kind::Normal,
            true,
            "Count-in",
            |p, face, ink| {
                let c = face.center() + vec2(0.0, 1.5);
                let ink = if g.count_in || counting {
                    if blink { t::OK } else { ink }
                } else {
                    t::FAINT
                };
                let s = Stroke::new(1.5_f32, ink);
                p.circle_stroke(c, 8.0, s);
                p.line_segment([c + vec2(-2.5, -11.5), c + vec2(2.5, -11.5)], s);
                p.line_segment([c, c + vec2(0.0, -5.0)], s);
                p.line_segment([c, c + vec2(4.0, 2.0)], s);
                lamp(p, face, g.count_in);
            },
        );
        if counting {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
        let when = if g.count_in_always {
            "before every Play"
        } else {
            "before recording"
        };
        let tip = if g.count_in {
            format!("Count-in on: one bar of clicks {when} · right-click for when")
        } else {
            format!("Count-in: one bar of clicks {when} · right-click for when")
        };
        let r = kit::bar_hint(r, tip);
        if r.clicked() {
            self.settings.general.count_in = !g.count_in;
            self.save_settings();
            self.say(
                Tone::Quiet,
                if self.settings.general.count_in {
                    format!("Count-in on: one bar {when}.")
                } else {
                    "Count-in off.".to_string()
                },
            );
        }
        r.context_menu(|ui| {
            kit::menu_style(ui);
            let mut always = self.settings.general.count_in_always;
            if ui.radio(!always, "Only when recording").clicked() {
                always = false;
            }
            if ui.radio(always, "Before every Play").clicked() {
                always = true;
            }
            if always != self.settings.general.count_in_always {
                self.settings.general.count_in_always = always;
                self.settings.general.count_in = true;
                self.save_settings();
                ui.close_menu();
            }
        });
        // Wait for input: an hourglass; it blinks while waiting.
        let waiting = self.waiting_input;
        let blink = waiting && (ui.input(|i| i.time) * 3.0) as i64 % 2 == 0;
        let r = kit::button_sized(
            ui,
            size,
            kit::Kind::Normal,
            true,
            "Wait for input",
            |p, face, ink| {
                let c = face.center() + vec2(0.0, 1.0);
                let ink = if g.wait_input || waiting {
                    if blink { t::OK } else { ink }
                } else {
                    t::FAINT
                };
                let s = Stroke::new(1.5_f32, ink);
                p.line_segment([c + vec2(-6.0, -9.0), c + vec2(6.0, -9.0)], s);
                p.line_segment([c + vec2(-6.0, 9.0), c + vec2(6.0, 9.0)], s);
                p.add(Shape::line(
                    vec![c + vec2(-5.0, -9.0), c + vec2(5.0, 9.0)],
                    s,
                ));
                p.add(Shape::line(
                    vec![c + vec2(5.0, -9.0), c + vec2(-5.0, 9.0)],
                    s,
                ));
                lamp(p, face, g.wait_input);
            },
        );
        if waiting {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
        let tip = if g.wait_input {
            "Wait for input on: with Record on, Play waits for your first note"
        } else {
            "Wait for input: with Record on, Play waits for your first note"
        };
        if kit::bar_hint(r, tip).clicked() {
            self.settings.general.wait_input = !g.wait_input;
            if !self.settings.general.wait_input {
                self.waiting_input = false;
            }
            self.save_settings();
            self.say(
                Tone::Quiet,
                if self.settings.general.wait_input {
                    "Wait for input on: with Record on, Play starts at your first note."
                } else {
                    "Wait for input off."
                },
            );
        }
    }

    fn transport_keys(&mut self, ui: &mut Ui) {
        let playing = self.is_playing();
        let enabled = self.audio.is_some();
        let size = vec2(48.0, 40.0);
        let name = if playing { "Pause" } else { "Play" };
        let r = kit::button_sized(
            ui,
            size,
            kit::Kind::Primary,
            enabled,
            name,
            |p, face, ink| {
                if playing {
                    kit::icon_pause(p, face.center(), 14.0, ink);
                } else {
                    kit::icon_play(p, face.center() + vec2(1.0, 0.0), 16.0, ink);
                }
            },
        );
        if kit::bar_hint(
            r,
            if playing {
                "Pause in place  Ctrl+Space"
            } else {
                "Play  Space"
            },
        )
        .clicked()
        {
            self.play_pause();
        }
        let r = kit::button_sized(
            ui,
            size,
            kit::Kind::Normal,
            enabled,
            "Stop",
            |p, face, ink| {
                kit::icon_stop(p, face.center(), 13.0, ink);
            },
        );
        if kit::bar_hint(r, "Stop and return to the start  Space").clicked() {
            self.stop();
        }
        // Record: armed mixer tracks record while playing (FL: R).
        let on = self.rec.on;
        let taking = !self.rec.parts.is_empty();
        // No start time in record.rs: remember when the take began in egui
        // memory while parts are non-empty.
        let start_id = egui::Id::new("kfxl-rec-start");
        let now = ui.input(|i| i.time);
        if taking {
            let started: Option<f64> = ui.data(|d| d.get_temp(start_id));
            if started.is_none() {
                ui.data_mut(|d| d.insert_temp(start_id, now));
            }
        } else {
            ui.data_mut(|d| d.remove::<f64>(start_id));
        }
        let started: Option<f64> = ui.data(|d| d.get_temp(start_id));
        let r = kit::button_sized(
            ui,
            vec2(40.0, 40.0),
            kit::Kind::Normal,
            enabled,
            "Record",
            |p, face, _| {
                let c = if on || taking { t::BRAND } else { t::FAINT };
                if taking {
                    // Recording: the whole key face reads red.
                    p.circle_filled(face.center(), 11.0, c);
                    p.circle_stroke(
                        face.center(),
                        14.0,
                        Stroke::new(1.0_f32, c.gamma_multiply(0.6)),
                    );
                } else {
                    p.circle_filled(face.center(), 6.5, c);
                    if on {
                        p.circle_stroke(
                            face.center(),
                            9.5,
                            Stroke::new(1.0_f32, c.gamma_multiply(0.6)),
                        );
                    }
                }
            },
        );
        let auto_taking = !self.auto_take.lanes.is_empty();
        let tip = if taking {
            format!(
                "Recording {} track(s). Stop or press R to finish the take.",
                self.rec.parts.len()
            )
        } else if auto_taking {
            format!(
                "Recording automation ({} control(s)). Stop or press R to finish.",
                self.auto_take.lanes.len()
            )
        } else if on {
            "Record is on: armed mixer tracks, MIDI notes, and tweaked controls record while playing  R · Right-click for filters".to_string()
        } else {
            "Record audio, MIDI notes, and automation while playing  R · Right-click for filters".to_string()
        };
        let r = kit::bar_hint(r, tip);
        if r.clicked() {
            self.toggle_record();
        }
        r.context_menu(|ui| {
            kit::menu_style(ui);
            let mut auto_rec = self.rec.automation;
            if ui.checkbox(&mut auto_rec, "Record automation").clicked() {
                self.toggle_record_automation();
                ui.close_menu();
            }
            let mut input_rec = self.rec.input;
            if ui
                .checkbox(&mut input_rec, "Record audio input (mic / line)")
                .clicked()
            {
                self.toggle_input();
                ui.close_menu();
            }
        });
        if taking {
            let elapsed = started.map(|t0| (now - t0).max(0.0) as u64).unwrap_or(0);
            ui.label(
                egui::RichText::new(format!("REC {}:{:02}", elapsed / 60, elapsed % 60))
                    .font(t::mono(12.0))
                    .color(t::BRAND),
            );
            ui.ctx().request_repaint();
        }
    }

    /// Recessed segment display: position and tempo. Ghost segments stay visible.
    fn lcd(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(vec2(272.0, 42.0), Sense::hover());
        let p = ui.painter();
        let well = kit::sunken(p, rect, 8.0);
        let playing = self.is_playing();
        let step = self.position_step();
        let (mut bar, mut sixteenth) = (step / 16 + 1, step % 16 + 1);
        // Song mode with time signatures: the bar's own count (3/4 runs to 12).
        let mut sig = (4u8, 4u8);
        if self.song_mode && !self.song.meters.is_empty() {
            let tick = step as keyframexl::sequence::Tick * keyframexl::sequence::STEP;
            let (b, start, _, num, den) = keyframexl::tempo::bar_at(&self.song.meters, tick);
            bar = b as usize + 1;
            sixteenth = ((tick - start) / keyframexl::sequence::STEP) as usize + 1;
            sig = (num, den);
        }
        let h = 18.0;
        let top = well.top() + 18.0;
        let lx = well.left() + 12.0;
        // Run indicator: a lit triangle while the transport moves.
        let run = if playing { t::LCD_ON } else { t::LCD_OFF };
        kit::icon_play(p, pos2(lx + 4.0, top + h / 2.0), 10.0, run);
        let x0 = lx + 16.0;
        kit::caps(
            p,
            pos2(x0, well.top() + 3.0),
            Align2::LEFT_TOP,
            "bar",
            12.0,
            t::FAINT,
        );
        let w = kit::seg_text(
            p,
            pos2(x0, top),
            h,
            &format!("{:02}", bar.min(99)),
            t::LCD_ON,
            t::LCD_OFF,
        );
        let x1 = x0 + w + 4.0;
        let w2 = kit::seg_text(p, pos2(x1, top), h, ":", t::LCD_ON, t::LCD_OFF);
        let x2 = x1 + w2 + 2.0;
        kit::caps(
            p,
            pos2(x2, well.top() + 3.0),
            Align2::LEFT_TOP,
            "step",
            12.0,
            t::FAINT,
        );
        let w3 = kit::seg_text(
            p,
            pos2(x2, top),
            h,
            &format!("{sixteenth:02}"),
            t::LCD_ON,
            t::LCD_OFF,
        );
        // Divider, then tempo.
        let dx = x2 + w3 + 10.0;
        p.line_segment(
            [pos2(dx, well.top() + 6.0), pos2(dx, well.bottom() - 6.0)],
            Stroke::new(1.0_f32, t::LINE),
        );
        let tx = dx + 12.0;
        kit::caps(
            p,
            pos2(tx, well.top() + 3.0),
            Align2::LEFT_TOP,
            "bpm",
            12.0,
            t::FAINT,
        );
        let tempo = format!("{:5.1}", self.tempo_now());
        let p = p.clone();
        let steppers = Rect::from_min_max(
            pos2(well.right() - 26.0, well.top() + 3.0),
            pos2(well.right() - 4.0, well.bottom() - 3.0),
        );
        let bpm = Rect::from_min_max(
            pos2(dx + 2.0, well.top() + 2.0),
            pos2(steppers.left() - 34.0, well.bottom() - 2.0),
        );
        #[cfg(test)]
        {
            self.probe.bpm = Some(bpm);
        }
        self.tempo_field(ui, bpm, &p);
        let typing = ui
            .data(|d| d.get_temp::<String>(ui.id().with("tempo-type")))
            .is_some();
        if !typing {
            kit::seg_text(&p, pos2(tx, top), h, &tempo, t::LCD_ON, t::LCD_OFF);
        }
        kit::caps(
            &p,
            pos2(steppers.left() - 6.0, well.top() + 3.0),
            Align2::RIGHT_TOP,
            &format!("{}/{}", sig.0, sig.1),
            12.0,
            t::FAINT,
        );
        // A map in play: a blue dot left of the digits (address colour; shape = state).
        if self.tempo_table().is_some() {
            p.circle_filled(pos2(tx - 6.0, top + h / 2.0), 2.0, t::ENC_BLUE);
        }
        self.tempo_steppers(ui, steppers);
    }

    /// ▲▼ on the LCD: ±1 BPM, Shift ±0.1; held, they repeat. One undo step per press.
    fn tempo_steppers(&mut self, ui: &mut Ui, rect: Rect) {
        let half = rect.height() / 2.0;
        let (shift, now) = ui.input(|i| (i.modifiers.shift, i.time));
        for (k, up) in [(0, true), (1, false)] {
            let r = Rect::from_min_size(
                pos2(rect.left(), rect.top() + k as f32 * half),
                vec2(rect.width(), half - 1.0),
            );
            let id = ui.id().with(("tempo-step", up));
            let resp = ui.interact(r, id, Sense::click());
            let name = if up { "Tempo up" } else { "Tempo down" };
            kit::paint_button(
                ui,
                r,
                &resp,
                kit::Kind::Normal,
                true,
                name,
                |p, face, ink| {
                    let c = face.center();
                    let (a, b) = (4.0, 2.5);
                    let pts = if up {
                        vec![c + vec2(0.0, -b), c + vec2(a, b), c + vec2(-a, b)]
                    } else {
                        vec![c + vec2(0.0, b), c + vec2(-a, -b), c + vec2(a, -b)]
                    };
                    p.add(Shape::convex_polygon(pts, ink, Stroke::NONE));
                },
            );
            let resp = kit::bar_hint(resp, format!("{name} 1 BPM · Shift 0.1 · hold to repeat"));
            // Press: one step and an undo point; held past 0.4 s it repeats every 60 ms.
            let held: Option<f64> = ui.data(|d| d.get_temp(id));
            let mut steps = 0;
            if resp.is_pointer_button_down_on() {
                match held {
                    None => {
                        self.checkpoint();
                        steps = 1;
                        ui.data_mut(|d| d.insert_temp(id, now + 0.4));
                    }
                    Some(next) if now >= next => {
                        steps = 1;
                        ui.data_mut(|d| d.insert_temp(id, now + 0.06));
                    }
                    _ => {}
                }
                ui.ctx().request_repaint();
            } else if held.is_some() {
                ui.data_mut(|d| d.remove::<f64>(id));
            }
            if steps > 0 {
                let d = if shift { 0.1 } else { 1.0 } * if up { 1.0 } else { -1.0 };
                let tempo = (((self.edited_tempo() + d) * 10.0).round() / 10.0).clamp(40.0, 300.0);
                if tempo != self.edited_tempo() {
                    self.set_edited_tempo(tempo);
                    self.publish();
                    self.say(Tone::Quiet, format!("Tempo {tempo:.1} BPM. Ctrl+Z undoes."));
                }
            }
        }
    }

    /// FL tempo field: drag up/down (Shift fine), wheel ±1 BPM (Shift ±0.1),
    /// double-click to type, right-click to reset. One undo step per gesture.
    fn tempo_field(&mut self, ui: &mut Ui, rect: Rect, p: &egui::Painter) {
        let r = ui.interact(rect, ui.id().with("tempo-field"), Sense::click_and_drag());
        let type_id = ui.id().with("tempo-type");
        let wheel_id = ui.id().with("tempo-wheel");
        let now = ui.input(|i| i.time);
        if r.hovered() {
            p.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0_f32, t::LINE),
                egui::StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let before = self.edited_tempo();
        let mut tempo = before;
        let digit_id = r.id.with("tempo-digit");
        let fractional = ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| tempo_fractional(rect, pos.x));
        if r.drag_started() {
            self.checkpoint();
            ui.data_mut(|d| {
                d.insert_temp(digit_id, fractional);
                d.insert_temp(r.id.with("tempo-drag-remainder"), 0.0_f32);
            });
        }
        let drag = kit::captured_drag(ui, &r);
        let drag_fractional = ui
            .data(|d| d.get_temp::<bool>(digit_id))
            .unwrap_or(fractional);
        let remainder_id = r.id.with("tempo-drag-remainder");
        let motion = ui.data(|d| d.get_temp::<f32>(remainder_id)).unwrap_or(0.0) - drag.y;
        let steps = (motion / 5.0).trunc();
        ui.data_mut(|d| d.insert_temp(remainder_id, motion - steps * 5.0));
        tempo += steps as f64 * if drag_fractional { 0.1 } else { 1.0 };
        if r.hovered() {
            // Count notches from the raw delta (egui spreads the smooth one
            // over frames, and per-frame rounding would lose the tail).
            let d = ui.input_mut(|i| {
                let d = i.raw_scroll_delta.y;
                i.raw_scroll_delta = egui::Vec2::ZERO;
                i.smooth_scroll_delta = egui::Vec2::ZERO;
                d
            });
            if d != 0.0 {
                // A run of wheel turns is one undo step.
                let last: f64 = ui.data(|m| m.get_temp(wheel_id)).unwrap_or(-10.0);
                if now - last > 0.6 {
                    self.checkpoint();
                }
                ui.data_mut(|m| m.insert_temp(wheel_id, now));
                tempo += (d / 50.0) as f64 * if fractional { 0.1 } else { 1.0 };
            }
        }
        if r.secondary_clicked() {
            self.checkpoint();
            tempo = 120.0;
        }
        if r.double_clicked() {
            // Empty with the current tempo as a hint: typing replaces it.
            ui.data_mut(|m| m.insert_temp(type_id, String::new()));
        }
        if let Some(mut text) = ui.data(|m| m.get_temp::<String>(type_id)) {
            let te = ui.put(
                rect.shrink(4.0),
                egui::TextEdit::singleline(&mut text)
                    .font(t::mono(16.0))
                    .hint_text(format!("{before:.1}"))
                    .desired_width(rect.width() - 8.0),
            );
            te.request_focus();
            let (enter, esc) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if enter && text.trim().is_empty() {
                ui.data_mut(|m| m.remove::<String>(type_id));
            } else if enter {
                match text.trim().replace(',', ".").parse::<f64>() {
                    Ok(v) if (40.0..=300.0).contains(&v) => {
                        self.checkpoint();
                        tempo = v;
                        ui.data_mut(|m| m.remove::<String>(type_id));
                    }
                    _ => {
                        self.say(Tone::Bad, "Tempo is 40 to 300 BPM.");
                        ui.data_mut(|m| m.insert_temp(type_id, text));
                    }
                }
            } else if esc || te.lost_focus() {
                ui.data_mut(|m| m.remove::<String>(type_id));
            } else {
                ui.data_mut(|m| m.insert_temp(type_id, text));
            }
        }
        let tempo = (tempo * 10.0).round() / 10.0;
        let tempo = tempo.clamp(40.0, 300.0);
        if tempo != before {
            self.set_edited_tempo(tempo);
            self.publish();
            if self.tempo_table().is_some() {
                let at = self.playhead_tick();
                self.say(
                    Tone::Quiet,
                    format!(
                        "Tempo event at {} → {tempo:.1} BPM. Ctrl+Z undoes.",
                        super::roll::bar_pos(at)
                    ),
                );
            } else {
                self.say(Tone::Quiet, format!("Tempo {tempo:.1} BPM. Ctrl+Z undoes."));
            }
        }
        kit::bar_hint(
            r,
            "Tempo: drag or wheel over a digit · double-click to type · right-click for 120",
        );
    }
}

fn tempo_fractional(rect: Rect, x: f32) -> bool {
    let advance = kit::seg_digit_w(18.0) + 18.0 * 0.26;
    x >= rect.left() + 10.0 + 3.0 * advance
}

/// Nearest note (non-automation) pattern before / after `from` in list order.
/// Automation clips live in the playlist only (FL): ◀ / ▶ never land on them.
fn prev_note_pattern(patterns: &[keyframexl::sequence::Pattern], from: usize) -> Option<usize> {
    (0..from.min(patterns.len()))
        .rev()
        .find(|&i| patterns[i].automation.is_none())
}

fn next_note_pattern(patterns: &[keyframexl::sequence::Pattern], from: usize) -> Option<usize> {
    (from.saturating_add(1)..patterns.len()).find(|&i| patterns[i].automation.is_none())
}

#[cfg(test)]
mod tempo_digit_tests {
    use super::*;
    #[test]
    fn fractional_digit_starts_after_three_digits() {
        let rect = Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(100.0, 30.0));
        let boundary = 110.0 + 3.0 * (kit::seg_digit_w(18.0) + 18.0 * 0.26);
        assert!(!tempo_fractional(rect, boundary - 0.1));
        assert!(tempo_fractional(rect, boundary));
    }
}

#[cfg(test)]
mod note_step_tests {
    use super::{next_note_pattern, prev_note_pattern};
    use keyframexl::automation::{Envelope, Target};
    use keyframexl::sequence::Pattern;

    fn notes(id: u64) -> Pattern {
        Pattern::new(id, &format!("Pattern {id}"))
    }

    fn auto(id: u64) -> Pattern {
        let mut p = Pattern::new(id, "Auto");
        p.automation = Some(Envelope::flat(
            Target::MixerVolume { track: 0 },
            0.5,
            p.length,
        ));
        p
    }

    #[test]
    fn steppers_skip_automation_clips() {
        let ps = vec![notes(1), auto(2), notes(3), auto(4)];
        assert_eq!(prev_note_pattern(&ps, 2), Some(0));
        assert_eq!(next_note_pattern(&ps, 0), Some(2));
        // At the last note pattern there is no next note: + makes a new one.
        assert_eq!(next_note_pattern(&ps, 2), None);
        assert_eq!(prev_note_pattern(&ps, 0), None);
        // From an automation clip both directions find the nearest notes.
        assert_eq!(prev_note_pattern(&ps, 1), Some(0));
        assert_eq!(next_note_pattern(&ps, 1), Some(2));
        // A trailing automation clip still counts as the end.
        let ps = vec![notes(1), auto(2)];
        assert_eq!(next_note_pattern(&ps, 0), None);
    }
}
