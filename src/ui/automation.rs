//! Automation clips in the app: create one from a control's right-click menu,
//! edit its envelope, and drive plugin parameters while the song plays.

use super::{App, Tone, kit};
use crate::plugins::Req;
use crate::theme as t;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, pos2, vec2};
use keyframexl::automation::{Envelope, Playback, Point, Target, thin_points, values_at};
use keyframexl::sequence::{BAR, Clip, Pattern, STEP, Tick};

/// A new clip covers this many bars (FL starts with the visible song span; four bars is a useful default).
const NEW_BARS: Tick = 4;
/// Normalized travel tolerance (~0.6%) when thinning live-recorded automation points.
const AUTO_THIN_TOL: f32 = 0.006;

/// Active Multilink to controllers session (FL: Ctrl+J).
/// Tweaked controls queue up; incoming distinct MIDI CCs bind them in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Multilink {
    pub targets: Vec<Target>,
    /// `(channel, cc)` pairs already consumed in this session so a continuous
    /// stream from one physical knob does not swallow subsequent queued targets.
    pub bound_ccs: Vec<(u8, u8)>,
    pub linked: usize,
}

/// Live automation samples captured while Record + Play are active.
#[derive(Clone, Debug, Default)]
pub(crate) struct AutoTake {
    pub lanes: Vec<(Target, Vec<(Tick, f32)>)>,
}

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
    pub(crate) fn auto_current(&self, target: Target) -> f32 {
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

    /// The most recently tweaked automatable control, if it still exists in the song.
    pub(crate) fn last_tweaked_target(&self) -> Option<Target> {
        self.last_tweaked.filter(|t| t.exists(&self.song))
    }

    /// Apply `value` (in the control's own units) to a song control or hosted plugin parameter.
    /// Does not create an undo checkpoint (callers decide gesture grouping).
    pub(crate) fn apply_target_value(&mut self, target: Target, value: f32) -> bool {
        if !target.exists(&self.song) {
            return false;
        }
        if let Target::PluginParam { plugin, param } = target {
            let v = value.clamp(0.0, 1.0);
            if let Some(h) = &self.plugins.host {
                h.send(Req::SetParam {
                    id: plugin,
                    param,
                    value: v as f64,
                });
            }
            if let Some(p) = self.plugins.params.get_mut(&plugin)
                && let Some(r) = p.0.iter_mut().find(|r| r.id == param)
            {
                r.value = v as f64;
            }
            let now = std::time::Instant::now();
            self.plugins.refresh_at = Some(now + std::time::Duration::from_millis(500));
            self.plugins.param_dirty = true;
            self.dirty_plugin_state();
            return true;
        }
        if target.set(&mut self.song, value) {
            self.publish();
            true
        } else {
            false
        }
    }

    /// Called whenever the user tweaks an automatable control (via UI knob/fader,
    /// plugin parameters, plugin GUI, or MIDI CC).
    pub(crate) fn on_target_tweaked(&mut self, target: Target) {
        if !target.exists(&self.song) {
            return;
        }
        self.last_tweaked = Some(target);
        if let Some(ml) = &mut self.multilink
            && !ml.targets.contains(&target)
        {
            ml.targets.push(target);
        }
        if self.rec.on && self.rec.automation && self.is_playing() {
            let tick = self.frame_tick_at(self.loop_pos());
            let value = self.auto_current(target);
            self.record_auto_point(target, tick, value);
        }
    }

    /// Copy the last tweaked control's current value for `Tools → Last tweaked → Copy value`.
    pub(crate) fn copy_last_tweaked(&mut self, ctx: &egui::Context) {
        let Some(target) = self.last_tweaked_target() else {
            return;
        };
        let value = self.auto_current(target);
        self.auto_clipboard = Some((target, value));
        let text = self.auto_value_text(target, value);
        ctx.copy_text(text.clone());
        self.say(
            Tone::Quiet,
            format!("Copied {} ({text}).", self.auto_label(target)),
        );
    }

    /// Paste the previously copied value into the last tweaked control.
    pub(crate) fn paste_last_tweaked(&mut self) {
        let (Some(target), Some((src, raw))) = (self.last_tweaked_target(), self.auto_clipboard)
        else {
            return;
        };
        let (src_lo, src_hi) = src.range(&self.song);
        let (dst_lo, dst_hi) = target.range(&self.song);
        let value = if std::mem::discriminant(&src) == std::mem::discriminant(&target)
            && (src_lo - dst_lo).abs() < 1e-6
            && (src_hi - dst_hi).abs() < 1e-6
        {
            raw
        } else {
            let u = src.to_unit(raw, src_lo, src_hi);
            target.from_unit(u, dst_lo, dst_hi)
        };
        if matches!(target, Target::PluginParam { .. }) {
            self.collect_plugin_states();
        }
        self.checkpoint();
        if self.apply_target_value(target, value) {
            let text = self.auto_value_text(target, value);
            self.say(
                Tone::Quiet,
                format!("Pasted {text} to {}. Ctrl+Z undoes.", self.auto_label(target)),
            );
        }
    }

    /// Reset the last tweaked control back to its default value.
    pub(crate) fn reset_last_tweaked(&mut self) {
        let Some(target) = self.last_tweaked_target() else {
            return;
        };
        let def = target.default_value(&self.song);
        if matches!(target, Target::PluginParam { .. }) {
            self.collect_plugin_states();
        }
        self.checkpoint();
        if self.apply_target_value(target, def) {
            let text = self.auto_value_text(target, def);
            self.say(
                Tone::Quiet,
                format!("Reset {} to {text}. Ctrl+Z undoes.", self.auto_label(target)),
            );
        }
    }

    /// Toggle Multilink to controllers (`Ctrl+J`).
    pub(crate) fn toggle_multilink(&mut self) {
        if self.multilink.is_some() {
            self.cancel_multilink();
            return;
        }
        self.midi_learn = None;
        self.multilink = Some(Multilink::default());
        self.say(
            Tone::Wait,
            "Multilink armed: tweak controls to queue them, then move knobs on your controller. Ctrl+J or Esc finishes.",
        );
    }

    /// End or cancel Multilink mode.
    pub(crate) fn cancel_multilink(&mut self) {
        let Some(ml) = self.multilink.take() else {
            return;
        };
        if ml.linked > 0 {
            self.say(
                Tone::Done,
                format!(
                    "Multilink finished: {} control{} linked.",
                    ml.linked,
                    if ml.linked == 1 { "" } else { "s" }
                ),
            );
        } else {
            self.say(Tone::Quiet, "Multilink cancelled.");
        }
    }

    /// Consume an incoming MIDI CC while Multilink is armed.
    /// Returns `true` if Multilink handled the event.
    pub(crate) fn multilink_cc(&mut self, channel: u8, cc: u8) -> bool {
        let Some(ml) = &mut self.multilink else {
            return false;
        };
        if ml.targets.is_empty() {
            self.say(
                Tone::Wait,
                "Multilink armed: tweak one or more controls first, then move a controller knob.",
            );
            return true;
        }
        // Ignore repeated CC values from a physical knob already bound during this session.
        if ml.bound_ccs.contains(&(channel, cc)) {
            return true;
        }
        let target = ml.targets.remove(0);
        ml.bound_ccs.push((channel, cc));
        ml.linked += 1;
        let remaining = ml.targets.len();
        let total_linked = ml.linked;
        let label = self.auto_label(target);
        self.settings.midi.bind(channel, cc, target);
        self.save_settings();
        self.midi_cc_last = None;
        if remaining == 0 {
            self.multilink = None;
            self.say(
                Tone::Done,
                format!(
                    "Multilink complete: linked CH {channel} CC {cc} to {label} ({total_linked} total)."
                ),
            );
        } else {
            let next = self
                .multilink
                .as_ref()
                .and_then(|m| m.targets.first().copied())
                .map(|t| self.auto_label(t))
                .unwrap_or_default();
            self.say(
                Tone::Wait,
                format!(
                    "Linked CH {channel} CC {cc} to {label}. Move next controller knob for {next} ({remaining} left)…"
                ),
            );
        }
        true
    }

    /// Live status-bar override while Multilink is armed.
    pub(crate) fn multilink_status(&self) -> Option<String> {
        let ml = self.multilink.as_ref()?;
        if let Some(&next) = ml.targets.first() {
            Some(format!(
                "Multilink ({} queued — next: {}): move a controller knob, or tweak more controls. Esc finishes.",
                ml.targets.len(),
                self.auto_label(next)
            ))
        } else {
            Some(
                "Multilink armed: tweak controls to queue them, then move knobs on your controller. Ctrl+J or Esc finishes."
                    .into(),
            )
        }
    }

    /// Record a single live automation sample at song `tick`.
    pub(crate) fn record_auto_point(&mut self, target: Target, tick: Tick, value: f32) {
        let tick = tick.max(0);
        if let Some((_, pts)) = self.auto_take.lanes.iter_mut().find(|(t, _)| *t == target) {
            if let Some(&(last_tick, _)) = pts.last() {
                if tick == last_tick {
                    pts.last_mut().expect("non-empty").1 = value;
                    return;
                }
                // Loop wrap: overwrite previous pass from the wrapped position onward.
                if tick + STEP < last_tick {
                    pts.retain(|&(t, _)| t < tick);
                }
            }
            pts.push((tick, value));
        } else {
            self.auto_take.lanes.push((target, vec![(tick, value)]));
        }
    }

    /// Finalize all recorded live automation lanes into thinned playlist automation clips
    /// as a single undo step.
    pub(crate) fn commit_auto_take(&mut self) {
        let lanes = std::mem::take(&mut self.auto_take.lanes);
        if lanes.is_empty() {
            return;
        }
        self.checkpoint();
        let mut committed = 0usize;
        let mut last_summary = String::new();
        for (target, samples) in lanes {
            if samples.is_empty() || !target.exists(&self.song) {
                continue;
            }
            let (lo, hi) = target.range(&self.song);
            let thinned = thin_points(target, lo, hi, &samples, AUTO_THIN_TOL);
            if thinned.is_empty() {
                continue;
            }
            let first_tick = thinned.first().expect("non-empty").tick;
            let last_tick = thinned.last().expect("non-empty").tick;
            let name = self.auto_label(target);

            // Reuse an existing unmuted automation clip for this target if it starts at or before the take.
            let existing = self.song.clips.iter().enumerate().find_map(|(ci, clip)| {
                if clip.muted || clip.start > first_tick {
                    return None;
                }
                let pi = self.song.patterns.iter().position(|p| {
                    p.id == clip.pattern
                        && p.automation.as_ref().is_some_and(|e| e.target == target)
                })?;
                Some((ci, pi))
            });

            if let Some((ci, pi)) = existing {
                let clip_start = self.song.clips[ci].start;
                let clip_offset = self.song.clips[ci].offset;
                let local_min = (first_tick - clip_start + clip_offset).max(0);
                let local_max = (last_tick - clip_start + clip_offset).max(local_min);
                let needed_len = ((local_max / BAR) + 1) * BAR;
                let pat = &mut self.song.patterns[pi];
                if needed_len > pat.length {
                    pat.length = needed_len;
                }
                if let Some(env) = &mut pat.automation {
                    env.points
                        .retain(|q| q.tick < local_min || q.tick > local_max);
                    for pt in &thinned {
                        env.points.push(Point {
                            tick: (pt.tick - clip_start + clip_offset).max(0),
                            value: pt.value,
                            tension: 0.0,
                        });
                    }
                    env.tidy(lo, hi);
                    if let Some(last) = env.points.last().copied()
                        && last.tick < pat.length
                    {
                        env.points.push(Point {
                            tick: pat.length,
                            value: last.value,
                            tension: 0.0,
                        });
                    }
                }
                let pat_len = self.song.patterns[pi].length;
                if let Some(clen) = self.song.clips[ci].length.as_mut()
                    && *clen < pat_len - clip_offset
                {
                    *clen = pat_len - clip_offset;
                }
                let track = self.song.clips[ci].track;
                last_summary = format!(
                    "Recorded automation for \"{name}\" ({} points) on playlist track {}.",
                    thinned.len(),
                    track + 1
                );
            } else {
                let start_bar = (first_tick / BAR) * BAR;
                let end_bar = ((last_tick / BAR) + 1) * BAR;
                let len = (end_bar - start_bar).max(BAR);
                let mut points: Vec<Point> = thinned
                    .iter()
                    .map(|pt| Point {
                        tick: (pt.tick - start_bar).max(0),
                        value: pt.value,
                        tension: 0.0,
                    })
                    .collect();
                if let Some(first) = points.first().copied()
                    && first.tick > 0
                {
                    points.insert(
                        0,
                        Point {
                            tick: 0,
                            value: first.value,
                            tension: 0.0,
                        },
                    );
                }
                if let Some(last) = points.last().copied()
                    && last.tick < len
                {
                    points.push(Point {
                        tick: len,
                        value: last.value,
                        tension: 0.0,
                    });
                }
                let id = self.song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                let mut pat = Pattern::new(id, &name);
                pat.length = len;
                pat.automation = Some(Envelope {
                    target,
                    points,
                    playback: Playback::Clip,
                });
                self.song.patterns.push(pat);
                let track = self
                    .song
                    .clips
                    .iter()
                    .map(|c| c.track + 1)
                    .max()
                    .unwrap_or(0);
                self.song.clips.push(Clip::new(id, start_bar, track));
                last_summary = format!(
                    "Recorded automation for \"{name}\" ({} points) on playlist track {}.",
                    thinned.len(),
                    track + 1
                );
            }
            committed += 1;
        }
        if committed == 0 {
            self.history.pop();
            return;
        }
        self.publish();
        let mode_note = if self.song_mode {
            ""
        } else {
            " Switch to Song mode (L) to hear it."
        };
        let msg = if committed == 1 {
            format!("{last_summary}{mode_note} Ctrl+Z undoes.")
        } else {
            format!("Recorded automation for {committed} controls.{mode_note} Ctrl+Z undoes.")
        };
        self.say(Tone::Done, msg);
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
        self.say(
            Tone::Quiet,
            format!(
                "Automation clip \"{name}\" on playlist track {}. It plays in Song mode. Ctrl+Z undoes.",
                track + 1
            ),
        );
    }

    /// Once per frame: tweaked controls, live automation recording, menu requests,
    /// and plugin parameters while the song plays.
    pub(crate) fn automation_frame(&mut self, ctx: &egui::Context) {
        for target in kit::take_tweaked(ctx) {
            self.on_target_tweaked(target);
        }
        let recording = self.rec.on && self.rec.automation && self.is_playing();
        if !recording && !self.auto_take.lanes.is_empty() {
            self.commit_auto_take();
        }
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
            // Do not fight the user's hand while actively recording this parameter.
            if recording
                && self
                    .auto_take
                    .lanes
                    .iter()
                    .any(|(t, _)| *t == sample.target)
            {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use keyframexl::sequence::Song;

    #[test]
    fn last_tweaked_tracks_copy_paste_and_reset_with_undo() {
        let mut app = App::with_song(Song::default(), None);
        assert_eq!(app.last_tweaked_target(), None);

        let pan1 = Target::MixerPan { track: 1 };
        let pan2 = Target::MixerPan { track: 2 };
        app.song.mixer[1].pan = -0.6;
        app.on_target_tweaked(pan1);
        assert_eq!(app.last_tweaked_target(), Some(pan1));

        let ctx = egui::Context::default();
        app.copy_last_tweaked(&ctx);
        assert_eq!(app.auto_clipboard, Some((pan1, -0.6)));

        // Tweak track 2 pan, then paste track 1's copied value into it.
        app.on_target_tweaked(pan2);
        app.paste_last_tweaked();
        assert!((app.song.mixer[2].pan - (-0.6)).abs() < 1e-6);

        // Reset to default returns pan to 0.0 and is undoable.
        app.reset_last_tweaked();
        assert!((app.song.mixer[2].pan - 0.0).abs() < 1e-6);
        assert!(app.undo());
        assert!((app.song.mixer[2].pan - (-0.6)).abs() < 1e-6);

        // Cross-target paste maps through normalized travel (pan -1..1 -> FxMix 0..1).
        app.song.mixer[1].fx.push(keyframexl::fx::FxSlot::new(
            keyframexl::fx::FxKind::Reverb,
        ));
        let mix = Target::FxMix { track: 1, slot: 0 };
        app.on_target_tweaked(mix);
        app.paste_last_tweaked();
        // pan -0.6 is 20% of [-1, 1], so FxMix becomes 0.2.
        assert!((app.song.mixer[1].fx[0].mix - 0.2).abs() < 1e-5);
    }

    #[test]
    fn multilink_queues_deduplicates_ignores_repeated_cc_and_auto_finishes() {
        let mut app = App::with_song(Song::default(), None);
        let t1 = Target::MixerPan { track: 1 };
        let t2 = Target::MixerVolume { track: 1 };
        let t3 = Target::ChannelPan { inst: 1 };

        app.toggle_multilink();
        assert!(app.multilink.is_some());
        assert!(app.multilink_status().is_some());

        // Tweak controls (with a duplicate tweak of t1).
        app.on_target_tweaked(t1);
        app.on_target_tweaked(t1);
        app.on_target_tweaked(t2);
        app.on_target_tweaked(t3);
        assert_eq!(app.multilink.as_ref().unwrap().targets, vec![t1, t2, t3]);

        // Continuous CC 74 messages from one physical knob bind only t1.
        assert!(app.multilink_cc(1, 74));
        assert!(app.multilink_cc(1, 74));
        assert!(app.multilink_cc(1, 74));
        assert_eq!(app.multilink.as_ref().unwrap().targets, vec![t2, t3]);
        assert_eq!(
            app.settings.midi.binding_for_cc(1, 74).map(|b| b.target),
            Some(t1)
        );

        // Second knob binds t2, third knob binds t3 and auto-finishes Multilink.
        assert!(app.multilink_cc(1, 71));
        assert!(app.multilink.is_some());
        assert!(app.multilink_cc(1, 75));
        assert!(app.multilink.is_none());
        assert_eq!(
            app.settings.midi.binding_for_cc(1, 71).map(|b| b.target),
            Some(t2)
        );
        assert_eq!(
            app.settings.midi.binding_for_cc(1, 75).map(|b| b.target),
            Some(t3)
        );

        // Cancel before any links clears Multilink without binding.
        app.toggle_multilink();
        app.on_target_tweaked(Target::MixerVolume { track: 0 });
        app.cancel_multilink();
        assert!(app.multilink.is_none());
    }

    #[test]
    fn live_automation_recording_thins_points_merges_and_undoes_in_one_step() {
        let mut app = App::with_song(Song::default(), None);
        app.song_mode = true;
        let target = Target::MixerPan { track: 1 };
        let base_history = app.history.len();

        // Simulate a dense recorded sweep from -1.0 to +1.0 across bar 1, then back to 0.0 in bar 2.
        for i in 0..=40 {
            let tick = i * (BAR / 40);
            let val = -1.0 + 2.0 * (i as f32 / 40.0);
            app.record_auto_point(target, tick, val);
        }
        for i in 1..=40 {
            let tick = BAR + i * (BAR / 40);
            let val = 1.0 - (i as f32 / 40.0);
            app.record_auto_point(target, tick, val);
        }
        app.commit_auto_take();

        assert_eq!(app.history.len(), base_history + 1, "one undo step per take");
        assert!(app.auto_take.lanes.is_empty());

        let auto_pat = app
            .song
            .patterns
            .iter()
            .find(|p| p.automation.as_ref().is_some_and(|e| e.target == target))
            .expect("created automation pattern");
        let env = auto_pat.automation.as_ref().unwrap();
        assert!(
            env.points.len() <= 5,
            "thinned dense sweep to corners + endpoints, got {}",
            env.points.len()
        );
        assert!((env.value_at(0).unwrap() - (-1.0)).abs() < 0.02);
        assert!((env.value_at(BAR).unwrap() - 1.0).abs() < 0.02);
        assert!((env.value_at(2 * BAR).unwrap() - 0.0).abs() < 0.02);
        assert!(app.song.clips.iter().any(|c| c.pattern == auto_pat.id));

        // Recording a second pass over bar 2 updates the existing clip instead of duplicating it.
        let clips_before = app.song.clips.len();
        app.record_auto_point(target, BAR, 0.5);
        app.record_auto_point(target, BAR + BAR / 2, -0.5);
        app.commit_auto_take();
        assert_eq!(app.song.clips.len(), clips_before);
        let auto_pat = app
            .song
            .patterns
            .iter()
            .find(|p| p.automation.as_ref().is_some_and(|e| e.target == target))
            .unwrap();
        let env = auto_pat.automation.as_ref().unwrap();
        assert!((env.value_at(BAR + BAR / 2).unwrap() - (-0.5)).abs() < 0.02);

        // Undo reverts the second pass, then the first pass.
        assert!(app.undo());
        assert!(app.undo());
        assert!(
            !app.song
                .patterns
                .iter()
                .any(|p| p.automation.as_ref().is_some_and(|e| e.target == target))
        );
        assert!(app.song.validate().is_ok());
    }

    #[test]
    fn live_automation_loop_wrap_overwrites_tail_cleanly() {
        let mut app = App::with_song(Song::default(), None);
        let target = Target::MixerPan { track: 1 };
        app.record_auto_point(target, 0, -1.0);
        app.record_auto_point(target, BAR / 2, 0.0);
        app.record_auto_point(target, BAR - 1, 1.0);
        // Playhead wraps back to BAR / 4 and user tweaks to 0.75.
        app.record_auto_point(target, BAR / 4, 0.75);
        let (_, pts) = &app.auto_take.lanes[0];
        assert_eq!(pts, &vec![(0, -1.0), (BAR / 4, 0.75)]);
    }
}
