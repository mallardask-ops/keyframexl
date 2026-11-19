//! Automation clips in the app: create one from a control's right-click menu,
//! edit its envelope, and drive plugin parameters while the song plays.

use super::{App, Tone, kit};
use crate::plugins::Req;
use crate::theme as t;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, pos2, vec2};
use keyframexl::automation::{Envelope, Point, Target, values_at};
use keyframexl::sequence::{BAR, Clip, Pattern, STEP, Tick};

/// A new clip covers this many bars (FL starts with the visible song span; four bars is a useful default).
const NEW_BARS: Tick = 4;

impl App {
    /// The control's name as people know it: "Insert 2 volume", "Bass pan", "Reverb · Mix".
    pub(crate) fn auto_label(&self, target: Target) -> String {
        match target {
            Target::MixerVolume { track } => format!("{} volume", self.mixer_name(track as usize)),
            Target::MixerPan { track } => format!("{} pan", self.mixer_name(track as usize)),
            Target::ChannelVolume { inst } => format!("{} volume", self.instrument_name(inst)),
            Target::ChannelPan { inst } => format!("{} pan", self.instrument_name(inst)),
            Target::FxMix { track, slot } => {
                let name = self
                    .song
                    .mixer
                    .get(track as usize)
                    .and_then(|m| m.fx.get(slot as usize))
                    .map_or("Effect", |fx| fx.label());
                format!("{name} mix · {}", self.mixer_name(track as usize))
            }
            Target::FxParam { track, slot, param } => {
                let slot = self
                    .song
                    .mixer
                    .get(track as usize)
                    .and_then(|m| m.fx.get(slot as usize));
                let name = slot
                    .and_then(|s| s.kind.params().get(param as usize))
                    .map(|p| p.name)
                    .unwrap_or("parameter");
                let fx = slot.map(|s| s.kind.label()).unwrap_or("Effect");
                format!("{fx} {name} · {}", self.mixer_name(track as usize))
            }
            Target::PluginParam { plugin, param } => {
                let row = self
                    .plugins
                    .params
                    .get(&plugin)
                    .and_then(|p| p.0.iter().find(|r| r.id == param));
                let pname = self
                    .plugins
                    .live
                    .get(&plugin)
                    .map(|l| l.name.clone())
                    .unwrap_or_else(|| "Plugin".into());
                match row {
                    Some(r) => format!("{pname} · {}", r.name),
                    None => format!("{pname} · parameter {param}"),
                }
            }
        }
    }

    /// The value now, in the control's units (a plugin parameter from the last read of its page).
    fn auto_current(&self, target: Target) -> f32 {
        if let Target::PluginParam { plugin, param } = target {
            return self
                .plugins
                .params
                .get(&plugin)
                .and_then(|p| p.0.iter().find(|r| r.id == param))
                .map(|r| r.value as f32)
                .unwrap_or(0.5);
        }
        target.current(&self.song).unwrap_or(0.0)
    }

    /// FL: right-click a control → Create automation clip. A flat envelope at the
    /// current value on a free playlist track at bar 1; the editor opens.
    pub(crate) fn create_automation(&mut self, target: Target) {
        self.checkpoint();
        let id = self.song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let name = self.auto_label(target);
        let mut p = Pattern::new(id, &name);
        p.length = NEW_BARS * BAR;
        p.automation = Some(Envelope::flat(target, self.auto_current(target), p.length));
        self.song.patterns.push(p);
        let track = self
            .song
            .clips
            .iter()
            .map(|c| c.track + 1)
            .max()
            .unwrap_or(0);
        self.song.clips.push(Clip::new(id, 0, track));
        self.publish();
        self.auto_open = Some(id);
        self.say(Tone::Quiet, format!("Automation clip \"{name}\" on playlist track {}. It plays in Song mode. Ctrl+Z undoes.", track + 1));
    }

    /// Once per frame: requests from control menus, plugin parameters while the song plays.
    pub(crate) fn automation_frame(&mut self, ctx: &egui::Context) {
        if let Some(target) = kit::take_auto_request(ctx) {
            self.create_automation(target);
        }
        // Plugin parameters live in the plugin host: send the envelope values at
        // frame rate (the engine handles mixer, channel and effect values per block).
        if !(self.is_playing() && self.song_mode) || self.plugins.host.is_none() {
            self.auto_sent.clear();
            return;
        }
        let fps = self.frames_per_step();
        if fps <= 0.0 {
            return;
        }
        let at = self.frame_tick_at(self.loop_pos());
        let mut vals = Vec::new();
        values_at(&self.song, at, &mut vals);
        for sample in vals {
            let v = sample.value;
            let Target::PluginParam { plugin, param } = sample.target else {
                continue;
            };
            if self
                .auto_sent
                .get(&(plugin, param))
                .is_some_and(|last| (last - v).abs() < 1e-4)
            {
                continue;
            }
            self.auto_sent.insert((plugin, param), v);
            if let Some(h) = &self.plugins.host {
                h.send(Req::SetParam {
                    id: plugin,
                    param,
                    value: v as f64,
                });
            }
        }
        ctx.request_repaint();
    }

    /// The envelope editor (FL: the automation clip window).
    pub(crate) fn automation_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.auto_open else { return };
        let Some(pi) = self
            .song
            .patterns
            .iter()
            .position(|p| p.id == id && p.automation.is_some())
        else {
            self.auto_open = None;
            return;
        };
        let title = format!("Automation · {}", self.song.patterns[pi].name);
        let mut open = true;
        egui::Window::new(title)
            .id(egui::Id::new("automation-editor"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(640.0, 300.0))
            .min_width(360.0)
            .show(ctx, |ui| self.automation_body(ui, pi));
        if !open {
            self.auto_open = None;
        }
    }

    fn automation_body(&mut self, ui: &mut egui::Ui, pi: usize) {
        let env = self.song.patterns[pi].automation.clone().expect("checked");
        let len = self.song.patterns[pi].length.max(STEP);
        let (lo, hi) = env.target.range(&self.song);
        // Header: what it drives, its length.
        ui.horizontal(|ui| {
            let (ok, note) = if env.target.exists(&self.song) {
                (t::MUTED, self.auto_label(env.target))
            } else {
                (t::BAD, "Its control is gone".to_string())
            };
            ui.label(egui::RichText::new(note).font(t::sans(13.0)).color(ok));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bars = len / BAR;
                if kit::tool_button(ui, "+", kit::Kind::Ghost, bars < 64)
                    .on_hover_text("One bar longer")
                    .clicked()
                {
                    self.set_auto_length(pi, len + BAR);
                }
                ui.label(
                    egui::RichText::new(format!("{bars} bar{}", if bars == 1 { "" } else { "s" }))
                        .font(t::mono(12.0))
                        .color(t::TEXT),
                );
                if kit::tool_button(ui, "−", kit::Kind::Ghost, bars > 1)
                    .on_hover_text("One bar shorter")
                    .clicked()
                {
                    self.set_auto_length(pi, len - BAR);
                }
            });
        });
        ui.label(egui::RichText::new("Click to add a point, drag to move it, right-click to delete. Alt+drag up/down on a segment bends it.").font(t::sans(11.5)).color(t::FAINT));
        let size = vec2(ui.available_width(), ui.available_height().max(160.0));
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 6.0, t::SUNKEN);
        let inner = rect.shrink2(vec2(10.0, 12.0));
        let x = |tk: Tick| inner.left() + inner.width() * tk as f32 / len as f32;
        let target = env.target;
        let y = |v: f32| inner.bottom() - inner.height() * target.to_unit(v, lo, hi);
        let tick_at =
            |px: f32| (((px - inner.left()) / inner.width()).clamp(0.0, 1.0) * len as f32) as Tick;
        let value_at = |py: f32| target.from_unit((inner.bottom() - py) / inner.height(), lo, hi);
        // Grid: bars and beats.
        let mut tk = 0;
        while tk <= len {
            let strong = tk % BAR == 0;
            p.vline(
                x(tk),
                inner.y_range(),
                Stroke::new(
                    1.0_f32,
                    if strong {
                        t::LINE
                    } else {
                        t::LINE.gamma_multiply(0.45)
                    },
                ),
            );
            if strong {
                p.text(
                    pos2(x(tk) + 3.0, rect.top() + 2.0),
                    Align2::LEFT_TOP,
                    format!("{}", tk / BAR + 1),
                    t::mono(10.5),
                    t::FAINT,
                );
            }
            tk += BAR / 4;
        }
        if lo < 0.0 && hi > 0.0 {
            p.hline(inner.x_range(), y(0.0), Stroke::new(1.0_f32, t::LINE));
        }
        // The curve, filled below like FL's clip.
        let mut line: Vec<Pos2> = Vec::new();
        let n = (inner.width() / 3.0).max(2.0) as usize;
        for i in 0..=n {
            let tk = len * i as Tick / n as Tick;
            line.push(pos2(x(tk), y(env.value_at(tk).unwrap_or(lo))));
        }
        let mut fill = line.clone();
        fill.push(pos2(inner.right(), inner.bottom()));
        fill.push(pos2(inner.left(), inner.bottom()));
        for w in line.windows(2) {
            let quad = vec![
                w[0],
                w[1],
                pos2(w[1].x, inner.bottom()),
                pos2(w[0].x, inner.bottom()),
            ];
            p.add(egui::Shape::convex_polygon(
                quad,
                t::ENC_GREEN.gamma_multiply(0.12),
                Stroke::NONE,
            ));
        }
        p.add(egui::Shape::line(line, Stroke::new(1.6_f32, t::ENC_GREEN)));
        // Points and gestures.
        let hit = |pos: Pos2| {
            env.points
                .iter()
                .position(|pt| pos2(x(pt.tick), y(pt.value)).distance(pos) < 8.0)
        };
        let drag_id = ui.id().with("auto-drag");
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let hovered = pointer.and_then(hit);
        for (i, pt) in env.points.iter().enumerate() {
            let c = pos2(x(pt.tick), y(pt.value));
            let on = hovered == Some(i) || ui.data(|d| d.get_temp::<usize>(drag_id)) == Some(i);
            p.circle_filled(
                c,
                if on { 6.0 } else { 4.5 },
                if on { t::TEXT } else { t::ENC_GREEN },
            );
            p.circle_stroke(c, if on { 6.0 } else { 4.5 }, Stroke::new(1.0_f32, t::EDGE));
        }
        if let Some(pos) = resp.hover_pos() {
            let v = value_at(pos.y);
            p.text(
                pos + vec2(10.0, -14.0),
                Align2::LEFT_BOTTOM,
                self.auto_value_text(env.target, v),
                t::mono(11.0),
                t::MUTED,
            );
        }
        let alt = ui.input(|i| i.modifiers.alt);
        if resp.drag_started() {
            self.checkpoint();
            if let Some(pos) = resp.interact_pointer_pos() {
                let at = hit(pos).or_else(|| {
                    if alt {
                        return None;
                    }
                    // Dragging empty space: a new point there, then drag it.
                    let e = self.song.patterns[pi].automation.as_mut().expect("checked");
                    e.points.push(Point {
                        tick: tick_at(pos.x),
                        value: value_at(pos.y),
                        tension: 0.0,
                    });
                    e.tidy(lo, hi);
                    e.points.iter().position(|q| q.tick == tick_at(pos.x))
                });
                // Alt+drag: the segment under the pointer, by its left point.
                let seg = if alt {
                    env.points
                        .iter()
                        .rposition(|q| q.tick <= tick_at(pos.x))
                        .map(|i| i + 1_000_000)
                } else {
                    None
                };
                if let Some(i) = seg.or(at) {
                    ui.data_mut(|d| d.insert_temp(drag_id, i));
                }
            }
        }
        if resp.dragged()
            && let (Some(i), Some(pos)) = (
                ui.data(|d| d.get_temp::<usize>(drag_id)),
                resp.interact_pointer_pos(),
            )
        {
            let e = self.song.patterns[pi].automation.as_mut().expect("checked");
            if i >= 1_000_000 {
                let k = i - 1_000_000;
                if let Some(q) = e.points.get_mut(k) {
                    q.tension = (q.tension - resp.drag_delta().y / 120.0).clamp(-1.0, 1.0);
                }
            } else if i < e.points.len() {
                // The first point stays at the start; the others keep their order.
                let last = e.points.len() - 1;
                let lo_t = if i == 0 { 0 } else { e.points[i - 1].tick + 1 };
                let hi_t = if i == last {
                    len
                } else {
                    e.points[i + 1].tick - 1
                };
                let tk = if i == 0 {
                    0
                } else {
                    tick_at(pos.x).clamp(lo_t, hi_t.max(lo_t))
                };
                e.points[i].tick = tk;
                e.points[i].value = value_at(pos.y);
            }
            self.publish();
        }
        if resp.drag_stopped() {
            ui.data_mut(|d| d.remove::<usize>(drag_id));
        }
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && hit(pos).is_none()
        {
            self.checkpoint();
            let e = self.song.patterns[pi].automation.as_mut().expect("checked");
            e.points.push(Point {
                tick: tick_at(pos.x),
                value: value_at(pos.y),
                tension: 0.0,
            });
            e.tidy(lo, hi);
            self.publish();
        }
        if resp.secondary_clicked()
            && let Some(i) = resp.interact_pointer_pos().and_then(hit)
            && env.points.len() > 2
        {
            self.checkpoint();
            self.song.patterns[pi]
                .automation
                .as_mut()
                .expect("checked")
                .points
                .remove(i);
            self.publish();
        }
    }

    fn set_auto_length(&mut self, pi: usize, len: Tick) {
        self.checkpoint();
        let p = &mut self.song.patterns[pi];
        let old = p.length;
        p.length = len.max(BAR);
        if let Some(e) = &mut p.automation {
            // Longer: the last value holds to the new end. Shorter: points past it go.
            if len > old {
                if let Some(last) = e.points.last().copied() {
                    e.points.push(Point { tick: len, ..last });
                }
            } else {
                e.points.retain(|q| q.tick <= len);
                if e.points.len() < 2 {
                    let v = e.points.first().map(|q| q.value).unwrap_or(0.0);
                    e.points = vec![
                        Point {
                            tick: 0,
                            value: v,
                            tension: 0.0,
                        },
                        Point {
                            tick: len,
                            value: v,
                            tension: 0.0,
                        },
                    ];
                }
            }
        }
        self.publish();
    }

    /// A value as the control shows it.
    fn auto_value_text(&self, target: Target, v: f32) -> String {
        match target {
            Target::MixerVolume { .. } => {
                if v <= 1e-5 {
                    "−∞ dB".into()
                } else {
                    format!("{:+.1} dB", 20.0 * v.log10())
                }
            }
            Target::ChannelVolume { .. } => keyframexl::sequence::gain_db_text(v),
            Target::MixerPan { .. } | Target::ChannelPan { .. } => {
                if v.abs() < 0.005 {
                    "C".into()
                } else if v < 0.0 {
                    format!("L{:.0}", -v * 100.0)
                } else {
                    format!("R{:.0}", v * 100.0)
                }
            }
            Target::FxParam { track, slot, param } => self
                .song
                .mixer
                .get(track as usize)
                .and_then(|m| m.fx.get(slot as usize))
                .and_then(|s| s.kind.params().get(param as usize))
                .map(|spec| keyframexl::fx::format(spec, v))
                .unwrap_or_else(|| format!("{v:.2}")),
            Target::FxMix { .. } | Target::PluginParam { .. } => format!("{:.0}%", v * 100.0),
        }
    }
}

/// Draw an automation clip's envelope inside a playlist clip rectangle.
pub(crate) fn paint_clip(
    p: &egui::Painter,
    r: Rect,
    env: &Envelope,
    song: &keyframexl::sequence::Song,
    from: Tick,
    span: Tick,
    plen: Tick,
    color: egui::Color32,
) {
    let (lo, hi) = env.target.range(song);
    let n = (r.width() / 3.0).max(2.0) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let tk = from + span * i as Tick / n as Tick;
            let v = env.value_at(tk.rem_euclid(plen.max(1))).unwrap_or(lo);
            pos2(
                r.left() + r.width() * i as f32 / n as f32,
                r.bottom() - 3.0 - (r.height() - 6.0) * env.target.to_unit(v, lo, hi),
            )
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.4_f32, color)));
}
