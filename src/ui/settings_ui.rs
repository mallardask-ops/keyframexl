//! Settings window (FL: F10 / Options → Audio, MIDI, Plugins, General) and the
//! per-frame MIDI input work: lit keys, the activity light, note recording.

use super::{App, Cmd, Tab, Tone, kit};
use crate::midi_in::Event;
use crate::settings::{BUFFERS, RATES, Settings, VelocityCurve};
use crate::theme as t;
use eframe::egui::{self, Align2, RichText, vec2};
use keyframexl::sequence::{BAR, MAX_BARS, Note, STEP, Tick};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Page {
    #[default]
    Audio,
    Midi,
    Plugins,
    General,
}

impl Page {
    const ALL: [Page; 4] = [Page::Audio, Page::Midi, Page::Plugins, Page::General];
    fn label(self) -> &'static str {
        match self {
            Page::Audio => "Audio",
            Page::Midi => "MIDI",
            Page::Plugins => "Plugins",
            Page::General => "General",
        }
    }
    /// One line under the page title.
    fn blurb(self) -> &'static str {
        match self {
            Page::Audio => "Output and input devices, sample rate, buffer.",
            Page::Midi => "Keyboards and controllers, how notes come in.",
            Page::Plugins => "Found plugins, their state, where to look for more.",
            Page::General => "Interface size, saving, window system.",
        }
    }
}

/// The open window and what it listed when it opened.
pub struct SettingsUi {
    pub page: Page,
    outputs: Vec<String>,
    inputs: Vec<String>,
}

/// A key held on a MIDI keyboard while recording: (channel, start tick, velocity).
pub type Recording = std::collections::HashMap<u8, (u64, Tick, u8)>;

impl App {
    pub(crate) fn open_settings(&mut self, page: Page) {
        self.settings_ui = Some(SettingsUi {
            page,
            outputs: crate::audio::output_devices(),
            inputs: crate::audio::input_devices(),
        });
        self.win(Cmd::Show(Tab::Settings));
        if let Some(m) = &mut self.midi_in {
            m.retry();
            m.refresh(&self.settings.midi);
        }
    }

    pub(crate) fn save_settings(&mut self) {
        if !self.persist {
            return;
        }
        if let Err(e) = self.settings.save() {
            self.say(Tone::Bad, format!("Settings not saved: {e}"));
        }
    }

    /// Sample rate of the running output (48 kHz without one).
    pub(crate) fn rate(&self) -> u32 {
        self.audio
            .as_ref()
            .map(|a| a.running.rate)
            .unwrap_or(keyframexl::sequence::RATE)
    }

    /// Reopen the output with the current audio settings; the song carries over.
    fn reopen_audio(&mut self) -> Result<crate::audio::Running, String> {
        if !self.rec.parts.is_empty() {
            self.finish_takes();
        }
        self.audio = None;
        let a = crate::audio::Audio::open(self.song.clone(), &self.settings.audio)?;
        a.pattern.store(self.selected, Ordering::Relaxed);
        a.song_mode.store(self.song_mode, Ordering::Relaxed);
        a.metronome
            .store(self.settings.general.metronome, Ordering::Relaxed);
        let r = a.running.clone();
        self.audio = Some(a);
        self.route_midi(None);
        self.reopen_plugins();
        Ok(r)
    }

    pub(crate) fn restart_audio(&mut self) {
        match self.reopen_audio() {
            Ok(r) => self.say(Tone::Done, format!("Audio: {}", audio_summary(&r))),
            Err(e) => self.say(
                Tone::Bad,
                format!("Audio output failed: {e}. Pick another device in Settings (F10)."),
            ),
        }
    }

    /// Keeps the output alive: a stream that failed (device gone, ALSA stuck
    /// after an underrun), one whose callbacks stopped (PipeWire restarted),
    /// or no stream at all after a failed reopen is retried with a growing
    /// pause; the transport carries on from where it was. While the output
    /// named in Settings is missing, a returning device switches back to it.
    pub(crate) fn audio_watch(&mut self, ctx: &egui::Context) {
        use crate::audio::{AudioRecovery, Reconnect, StreamHealth};
        let id = egui::Id::new("audio-recovery");
        let mut rec = ctx
            .data(|d| d.get_temp::<AudioRecovery>(id))
            .unwrap_or_default();
        let now = ctx.input(|i| i.time);
        let health = self.audio.as_ref().map(|a| StreamHealth {
            failed: a.failed.load(Ordering::Relaxed),
            callbacks: a.callbacks.load(Ordering::Relaxed),
            fallback: a.running.fallback,
            transport: (
                a.playing.load(Ordering::Relaxed),
                a.position.load(Ordering::Relaxed),
            ),
        });
        let last_error = self
            .audio
            .as_ref()
            .and_then(|a| a.last_error.lock().ok().and_then(|e| e.clone()));
        let want = self.settings.audio.output.clone();
        let why = rec.observe(
            now,
            health,
            || last_error,
            || {
                let devices = crate::audio::output_devices();
                match &want {
                    Some(w) => devices.contains(w),
                    None => !devices.is_empty(),
                }
            },
        );
        if let Some(why) = why {
            let had_error = rec.last_error.clone();
            match self.reopen_audio() {
                Ok(r) => {
                    rec.reconnected(now, true, None);
                    if let (Some((playing, at)), Some(a)) = (rec.saved_transport, &self.audio) {
                        a.position.store(at, Ordering::Relaxed);
                        a.playing.store(playing, Ordering::Relaxed);
                    }
                    let how = match why {
                        Reconnect::PreferredBack => {
                            "Reconnected to the preferred output".to_string()
                        }
                        Reconnect::Error => "Audio recovered after an error".to_string(),
                        Reconnect::Stalled => {
                            "Audio recovered after the stream stalled".to_string()
                        }
                        Reconnect::DeviceBack | Reconnect::Retry => {
                            "Audio recovered after a disconnect".to_string()
                        }
                    };
                    let tone = if why == Reconnect::PreferredBack {
                        Tone::Done
                    } else {
                        Tone::Bad
                    };
                    self.say(tone, format!("{how}: {}", audio_summary(&r)));
                }
                Err(e) => {
                    rec.reconnected(now, false, Some(e.clone()));
                    let wait = rec.retry_at.map_or(0.0, |t| (t - now).max(0.0));
                    let cause = had_error.unwrap_or(e);
                    self.say(
                        Tone::Bad,
                        format!("Audio output lost: {cause}. Retrying in {wait:.1} s."),
                    );
                }
            }
        }
        // A frame now and then keeps the watch running while nothing else repaints.
        if rec.armed {
            ctx.request_repaint_after(std::time::Duration::from_millis(if self.audio.is_some() {
                1000
            } else {
                250
            }));
        }
        ctx.data_mut(|d| d.insert_temp(id, rec));
    }

    /// Point MIDI callbacks at the current output (after start or a restart).
    pub(crate) fn route_midi(&mut self, ctx: Option<&egui::Context>) {
        let Some(m) = &self.midi_in else { return };
        if let Ok(mut r) = m.shared.route.lock() {
            r.live = self.audio.as_ref().map(|a| a.live.clone());
            r.position = self.audio.as_ref().map(|a| a.position.clone());
            if let Some(ctx) = ctx {
                r.repaint = Some(ctx.clone());
            }
        }
    }

    /// The channel a MIDI keyboard plays: the focused piano roll's, else the
    /// rack selection, else the last roll's (FL: the selected channel).
    pub(crate) fn midi_target(&self) -> u64 {
        if let Some(super::Tab::Roll { instrument }) = &self.focus {
            return *instrument;
        }
        self.rack_sel
            .last()
            .copied()
            .filter(|id| self.song.instrument(*id).is_some())
            .unwrap_or(self.instrument)
    }

    /// Once per frame: follow the selected channel, light keys, record notes,
    /// and keep MIDI output / Clock Master synced with transport.
    pub(crate) fn midi_frame(&mut self) {
        let target = self.midi_target();
        if let Some(m) = &mut self.midi_in {
            m.shared.target.store(target, Ordering::Relaxed);
            let events = m.poll(&self.settings.midi);
            if events.iter().any(|e| matches!(e.event, Event::On { .. })) {
                self.input_arrived();
            }
            let recording = self.rec.on && self.is_playing();
            if !recording && !self.midi_rec.is_empty() {
                self.midi_rec.clear();
            }
            if !recording {
                self.midi_take = false;
            }
            for inc in events {
                self.on_midi(target, inc.event, recording);
            }
        }
        let playing = self.is_playing();
        let at_start = self
            .audio
            .as_ref()
            .is_some_and(|a| a.position.load(Ordering::Relaxed) == 0);
        let master = self.settings.midi.clock == crate::settings::MidiClockMode::Master;
        let tempo = self.song.tempo;
        if let Some(mo) = &mut self.midi_out {
            let prefs = &self.settings.midi;
            mo.poll_refresh(|p| prefs.is_out_on(p));
            mo.sync_transport(master, playing, at_start, tempo);
        }
    }

    /// A note from a MIDI keyboard or the typing keyboard: light the key, record it.
    /// A controller either finishes a pending link or drives its bound control.
    fn on_midi(&mut self, target: u64, event: Event, recording: bool) {
        match event {
            Event::On {
                pitch,
                velocity,
                frame,
            } => {
                self.midi_keys[pitch as usize] = true;
                if recording {
                    let at = self.frame_tick(frame);
                    self.midi_rec.insert(pitch, (target, at, velocity));
                }
            }
            Event::Off { pitch, frame } => {
                self.midi_keys[pitch as usize] = false;
                if let Some((inst, start, velocity)) = self.midi_rec.remove(&pitch) {
                    let end = self.frame_tick(frame);
                    self.record_note(inst, pitch, velocity, start, end);
                }
            }
            Event::Control { channel, cc, value } => {
                if cc > 127 {
                    return;
                }
                let v = value.min(127) as u8;
                if self.midi_learn.is_some() {
                    self.finish_midi_learn(channel, cc);
                } else if self.multilink.is_some() {
                    self.multilink_cc(channel, cc);
                } else {
                    self.apply_midi_cc(channel, cc, v);
                }
            }
            Event::ClockStart => {
                let (start, _) = self.loop_bounds();
                if let Some(a) = &self.audio {
                    a.count_in.store(0, Ordering::Relaxed);
                    a.position.store(start, Ordering::Relaxed);
                    a.playing.store(true, Ordering::Relaxed);
                }
            }
            Event::ClockContinue => {
                if let Some(a) = &self.audio {
                    a.playing.store(true, Ordering::Relaxed);
                }
            }
            Event::ClockStop => {
                if let Some(a) = &self.audio {
                    a.playing.store(false, Ordering::Relaxed);
                }
            }
            Event::ClockTempo(bpm) => {
                if (self.song.tempo - bpm).abs() >= 0.05 {
                    self.song.tempo = bpm;
                    self.publish();
                }
            }
        }
    }

    /// Pick up link requests from knob menus, and Esc while armed.
    pub(crate) fn midi_learn_frame(&mut self, ctx: &egui::Context) {
        if let Some(target) = kit::take_learn_request(ctx) {
            self.start_midi_learn(target);
        }
        if (self.midi_learn.is_some() || self.multilink.is_some())
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            if self.midi_learn.is_some() {
                self.cancel_midi_learn();
            } else {
                self.cancel_multilink();
            }
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                });
            });
        }
    }

    pub(crate) fn start_midi_learn(&mut self, target: keyframexl::automation::Target) {
        self.multilink = None;
        self.midi_learn = Some(target);
        self.say(
            Tone::Wait,
            format!(
                "Move a knob on your controller to link {}. Esc cancels.",
                self.auto_label(target)
            ),
        );
    }

    /// Esc while armed: forget the pending link.
    pub(crate) fn cancel_midi_learn(&mut self) {
        self.midi_learn = None;
        self.say(Tone::Quiet, "Controller link cancelled.");
    }

    fn finish_midi_learn(&mut self, channel: u8, cc: u8) {
        let Some(target) = self.midi_learn.take() else {
            return;
        };
        self.settings.midi.bind(channel, cc, target);
        self.save_settings();
        self.midi_cc_last = None;
        self.say(
            Tone::Done,
            format!(
                "Linked CH {channel} CC {cc} to {}.",
                self.auto_label(target)
            ),
        );
    }

    /// A bound CC moves its control like a knob edit: song controls publish,
    /// plugin parameters go through the host. One undo step per gesture
    /// (moves within 0.6 s join, like the Parameters page).
    pub(crate) fn apply_midi_cc(&mut self, channel: u8, cc: u8, v: u8) {
        let Some(binding) = self.settings.midi.binding_for_cc(channel, cc).copied() else {
            return;
        };
        if !binding.target.exists(&self.song) {
            return;
        }
        let (lo, hi) = binding.target.range(&self.song);
        // Volume follows the fader taper, so a MIDI fader halfway up sounds halfway.
        let v = binding
            .target
            .from_unit(binding.value_for(v, 0.0, 1.0), lo, hi);
        let now = std::time::Instant::now();
        let fresh = self
            .midi_cc_last
            .is_none_or(|t| now.duration_since(t).as_millis() > 600);
        if fresh {
            if matches!(
                binding.target,
                keyframexl::automation::Target::PluginParam { .. }
            ) {
                self.collect_plugin_states();
            }
            self.checkpoint();
        }
        self.midi_cc_last = Some(now);
        if self.apply_target_value(binding.target, v) {
            self.on_target_tweaked(binding.target);
        }
    }

    /// FL's typing keyboard to piano (Ctrl+T): two octaves on the letter rows,
    /// Z-row from C3, Q-row from C4. The keys stop being shortcuts meanwhile.
    pub(crate) fn typing_piano(&mut self, ctx: &egui::Context) {
        use egui::Key::*;
        const KEYS: [(egui::Key, u8); 29] = [
            (Z, 0),
            (S, 1),
            (X, 2),
            (D, 3),
            (C, 4),
            (V, 5),
            (G, 6),
            (B, 7),
            (H, 8),
            (N, 9),
            (J, 10),
            (M, 11),
            (Comma, 12),
            (Q, 12),
            (Num2, 13),
            (W, 14),
            (Num3, 15),
            (E, 16),
            (R, 17),
            (Num5, 18),
            (T, 19),
            (Num6, 20),
            (Y, 21),
            (Num7, 22),
            (U, 23),
            (I, 24),
            (Num9, 25),
            (O, 26),
            (Num0, 27),
        ];
        let mut hits = Vec::new();
        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if !modifiers.command && !modifiers.alt => {
                    match KEYS.iter().find(|(k, _)| k == key) {
                        Some((_, off)) => {
                            if !repeat {
                                hits.push((48 + off, *pressed));
                            }
                            false
                        }
                        None => true,
                    }
                }
                _ => true,
            })
        });
        if hits.is_empty() {
            return;
        }
        if hits.iter().any(|(_, down)| *down) {
            self.input_arrived();
        }
        let target = self.midi_target();
        let recording = self.rec.on && self.is_playing();
        let velocity = self.velocity;
        for (pitch, down) in hits {
            let bytes = if down {
                [0x90, pitch, velocity]
            } else {
                [0x80, pitch, 0]
            };
            match &self.midi_in {
                // Same path as a MIDI keyboard: transpose, sustain, recording.
                Some(m) => {
                    m.shared.target.store(target, Ordering::Relaxed);
                    if let Some(ev) = m.shared.handle(&bytes) {
                        self.on_midi(target, ev, recording);
                    }
                }
                None => {
                    let Some(a) = &self.audio else { continue };
                    let id = (1u64 << 41) | pitch as u64;
                    a.live(if down {
                        crate::audio::LiveCmd::On {
                            id,
                            inst: target,
                            pitch,
                            velocity,
                            ch: 0,
                            lyric: 0,
                        }
                    } else {
                        crate::audio::LiveCmd::Off { id }
                    });
                    self.midi_keys[pitch as usize] = down;
                }
            }
        }
    }

    pub(crate) fn toggle_typing_piano(&mut self) {
        self.settings.general.typing_keyboard = !self.settings.general.typing_keyboard;
        self.save_settings();
        let text = if self.settings.general.typing_keyboard {
            "Typing keyboard plays notes: Z-row from C3, Q-row from C4. Ctrl+T turns it off."
        } else {
            "Typing keyboard off: letter keys are shortcuts again."
        };
        self.say(Tone::Quiet, text);
    }

    pub(crate) fn toggle_metronome(&mut self) {
        self.settings.general.metronome = !self.settings.general.metronome;
        self.save_settings();
        if let Some(a) = &self.audio {
            a.metronome
                .store(self.settings.general.metronome, Ordering::Relaxed);
        }
        self.say(
            Tone::Quiet,
            if self.settings.general.metronome {
                "Metronome on."
            } else {
                "Metronome off."
            },
        );
    }

    /// Song position (frames) as an absolute tick: takes outgrow one loop,
    /// so the pattern grows instead of wrapping every pass into bar 1.
    fn frame_tick(&self, frame: u64) -> Tick {
        self.frame_tick_at(frame)
    }

    fn record_note(&mut self, inst: u64, pitch: u8, velocity: u8, start: Tick, end: Tick) {
        let mut start = start.max(0);
        let mut length = (end - start).max(STEP / 4);
        if self.settings.midi.quantize_record {
            let g = self.snap.grid().max(1);
            start = (start + g / 2) / g * g;
            length = ((length + g / 2) / g * g).max(g);
        }
        length = length.max(STEP / 4);
        if start >= MAX_BARS * BAR {
            self.say(
                Tone::Bad,
                "Pattern is at its longest; extra notes were dropped.",
            );
            return;
        }
        if !self.midi_take {
            // One undo step per take.
            self.checkpoint();
            self.midi_take = true;
        } else {
            self.edits += 1;
        }
        // The take keeps playing past the loop: grow the pattern in whole
        // bars (FL) instead of wrapping every pass into bar 1.
        let need = start + length;
        if need > self.pattern().length {
            self.pattern_mut().length = (((need + BAR - 1) / BAR) * BAR).min(MAX_BARS * BAR);
        }
        let plen = self.pattern().length.max(1);
        let length = length.min((plen - start).max(STEP / 4));
        let pattern = self.selected;
        self.song
            .add_note(pattern, Note::new(inst, start, length, pitch, velocity));
        self.publish();
    }

    pub(crate) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let Some(ui_state) = &self.settings_ui else {
            return;
        };
        let mut page = ui_state.page;
        let (outputs, inputs) = (ui_state.outputs.clone(), ui_state.inputs.clone());
        let mut s = self.settings.clone();
        let mut close = false;
        let mut restart = false;
        let mut panic = false;
        let mut rescan = false;
        // FL: one window, sections down the left, the page on the right.
        let size = ui.max_rect().size();
        ui.set_min_size(size);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if let Some(p) = nav(ui, page, size.y) {
                page = p;
            }
            ui.vertical(|ui| {
                ui.set_width(size.x - NAV_W);
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 22,
                        right: 22,
                        top: 16,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(page.label())
                                .font(t::medium(20.0))
                                .color(t::TEXT),
                        );
                        ui.label(
                            RichText::new(page.blurb())
                                .font(t::sans(13.0))
                                .color(t::MUTED),
                        );
                    });
                let body_h = (ui.available_height() - 52.0).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt(("settings-body", page as u8))
                    .auto_shrink(false)
                    .max_height(body_h)
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 22,
                                right: 22,
                                top: 0,
                                bottom: 12,
                            })
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                                let mut check_updates_now = false;
                                match page {
                                    Page::Audio => {
                                        restart = self.audio_page(ui, &mut s, &outputs, &inputs)
                                    }
                                    Page::Midi => (panic, rescan) = self.midi_page(ui, &mut s),
                                    Page::Plugins => self.plugins_page(ui, &mut s),
                                    Page::General => {
                                        check_updates_now =
                                            general_page(ui, &mut s, self.updater.is_checking());
                                    }
                                }
                                if check_updates_now {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(egui::Id::new("settings-check-updates"), true)
                                    });
                                }
                            });
                    });
                // Footer: a seam, then Close on the right.
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 52.0), egui::Sense::hover());
                ui.painter().hline(
                    bar.x_range(),
                    bar.top(),
                    egui::Stroke::new(1.0_f32, t::EDGE),
                );
                let mut foot = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar.shrink2(vec2(22.0, 10.0)))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                if kit::button(&mut foot, "Close", kit::Kind::Normal, true).clicked() {
                    close = true;
                }
                foot.label(
                    RichText::new("Changes apply and save at once.")
                        .font(t::sans(12.0))
                        .color(t::FAINT),
                );
            });
        });
        if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_ui = None;
            self.win(Cmd::Close(Tab::Settings));
        }
        if let Some(st) = &mut self.settings_ui {
            st.page = page;
        }
        let wine_changed = s.plugins.wine != self.settings.plugins.wine
            || s.plugins.dxvk != self.settings.plugins.dxvk;
        if wine_changed {
            crate::plugins::set_dxvk(s.plugins.dxvk);
            crate::plugins::set_wine(s.plugins.wine.clone());
        }
        // A new Wine reaches plugins through new plugin processes.
        let bridge_changed = s.plugins.bridged != self.settings.plugins.bridged || wine_changed;
        if s.general.ui_scale != self.settings.general.ui_scale {
            ui.ctx().set_zoom_factor(
                s.general
                    .ui_scale
                    .clamp(crate::settings::UI_SCALE_MIN, crate::settings::UI_SCALE_MAX),
            );
        }
        if s != self.settings {
            let audio_changed = s.audio != self.settings.audio;
            let midi_out_changed = s.midi.out_enabled != self.settings.midi.out_enabled
                || s.midi.out_disabled != self.settings.midi.out_disabled;
            self.settings = s;
            if let Some(m) = &self.midi_in {
                m.set_prefs(&self.settings.midi);
            }
            if midi_out_changed && let Some(mo) = &mut self.midi_out {
                let prefs = &self.settings.midi;
                mo.refresh(|p| prefs.is_out_on(p));
            }
            self.save_settings();
            if audio_changed {
                restart = true;
            }
        }
        if ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(egui::Id::new("settings-check-updates")))
            .unwrap_or(false)
        {
            self.check_for_updates(ui.ctx());
        }
        if restart {
            self.restart_audio();
            if self.audio.is_none() {
                // A failed manual restart is not left silent: the watch keeps trying.
                let id = egui::Id::new("audio-recovery");
                let mut rec = ui
                    .ctx()
                    .data(|d| d.get_temp::<crate::audio::AudioRecovery>(id))
                    .unwrap_or_default();
                rec.reconnected(ui.input(|i| i.time), false, None);
                ui.ctx().data_mut(|d| d.insert_temp(id, rec));
            }
        }
        if bridge_changed && self.plugins.host.is_some() {
            self.restart_plugin_host(ui.ctx());
        }
        if panic {
            self.panic();
        }
        if rescan {
            if let Some(m) = &mut self.midi_in {
                m.retry();
                m.refresh(&self.settings.midi);
            }
            if let Some(mo) = &mut self.midi_out {
                mo.retry();
                let prefs = &self.settings.midi;
                mo.refresh(|p| prefs.is_out_on(p));
            }
        }
    }

    /// Stop every sounding live note (FL: MIDI panic).
    pub(crate) fn panic(&mut self) {
        if let Some(m) = &self.midi_in {
            m.shared.panic();
        }
        if let Some(mo) = &self.midi_out {
            mo.panic();
        }
        self.midi_keys = [false; 128];
        self.release_note();
        self.say(Tone::Quiet, "All notes off.");
    }

    fn audio_page(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Settings,
        outputs: &[String],
        inputs: &[String],
    ) -> bool {
        let mut restart = false;
        card(ui, "output", |ui| {
            row(ui, "Device", None, |ui| {
                let shown = s
                    .audio
                    .output
                    .clone()
                    .unwrap_or_else(|| "System default".into());
                kit::dropdown(ui, "audio-out", &shown, 380.0, |ui| {
                    if ui
                        .selectable_label(s.audio.output.is_none(), "System default")
                        .clicked()
                    {
                        s.audio.output = None;
                    }
                    for d in outputs {
                        if ui
                            .selectable_label(s.audio.output.as_ref() == Some(d), d)
                            .clicked()
                        {
                            s.audio.output = Some(d.clone());
                        }
                    }
                });
            });
            row(ui, "Sample rate", None, |ui| {
                let names: Vec<String> = RATES
                    .iter()
                    .map(|r| format!("{:.1} kHz", *r as f64 / 1000.0))
                    .collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let (pick, _) = kit::chips(
                    ui,
                    "audio-rate",
                    &names,
                    RATES.iter().position(|r| *r == s.audio.rate).unwrap_or(1),
                    None,
                );
                if let Some(i) = pick {
                    s.audio.rate = RATES[i];
                }
            });
            row(
                ui,
                "Buffer",
                Some(
                    "Auto asks for 256 frames (≈5 ms) so MIDI keys answer fast; raise it if you hear clicks.",
                ),
                |ui| {
                    let mut names: Vec<String> = vec!["Auto".into()];
                    names.extend(BUFFERS.iter().map(|b| b.to_string()));
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    let sel = s
                        .audio
                        .buffer
                        .and_then(|b| BUFFERS.iter().position(|x| *x == b))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let (pick, _) = kit::chips(ui, "audio-buffer", &names, sel, None);
                    if let Some(i) = pick {
                        s.audio.buffer = if i == 0 { None } else { Some(BUFFERS[i - 1]) };
                    }
                },
            );
            row(ui, "Running", None, |ui| {
                ui.vertical(|ui| {
                    let rec = ui
                        .ctx()
                        .data(|d| d.get_temp::<crate::audio::AudioRecovery>(egui::Id::new("audio-recovery")));
                    match &self.audio {
                        Some(a) => {
                            let r = &a.running;
                            let buf = r
                                .buffer
                                .map(|b| format!("{b} frames"))
                                .unwrap_or_else(|| "device buffer".into());
                            let load = a.load.load(Ordering::Relaxed) as f64 / 10.0;
                            let overruns = a.overruns.load(Ordering::Relaxed);
                            ui.horizontal(|ui| {
                                badge(ui, r.backend.label(), t::TEXT);
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} Hz · {buf}",
                                        r.device, r.rate
                                    ))
                                    .font(t::sans(13.0))
                                    .color(t::TEXT),
                                );
                            });
                            ui.label(
                                RichText::new(format!(
                                    "Latency ≈{:.1} ms · CPU {load:.0}% · Overruns {overruns}",
                                    r.latency_ms()
                                ))
                                .font(t::mono(13.0))
                                .color(if overruns > 0 { t::WAIT } else { t::MUTED }),
                            );
                            if r.rate != s.audio.rate {
                                ui.label(
                                    RichText::new(format!(
                                        "The device does not offer {} Hz; running at {} Hz.",
                                        s.audio.rate, r.rate
                                    ))
                                    .font(t::sans(12.0))
                                    .color(t::WAIT),
                                );
                            }
                            if r.fallback {
                                ui.label(
                                    RichText::new(format!(
                                        "{} is not available; playing through {}. It reconnects when the device returns.",
                                        s.audio.output.as_deref().unwrap_or("The chosen output"),
                                        r.device
                                    ))
                                    .font(t::sans(12.0))
                                    .color(t::WAIT),
                                );
                            }
                            if a.failed.load(Ordering::Relaxed) {
                                let why = a.last_error.lock().ok().and_then(|e| e.clone());
                                ui.label(
                                    RichText::new(format!(
                                        "Stream error: {}. Reconnecting.",
                                        why.unwrap_or_else(|| "unknown".into())
                                    ))
                                    .font(t::sans(12.0))
                                    .color(t::BAD),
                                );
                            }
                        }
                        None => {
                            ui.label(
                                RichText::new("No audio output.")
                                    .font(t::sans(13.0))
                                    .color(t::BAD),
                            );
                            if let Some(rec) = rec.as_ref().filter(|r| r.armed && r.recovering()) {
                                let now = ui.input(|i| i.time);
                                let next = rec.retry_at.map_or(0.0, |t| (t - now).max(0.0));
                                ui.label(
                                    RichText::new(format!(
                                        "Reconnecting: attempt {}, next in {next:.1} s.",
                                        rec.attempts + 1
                                    ))
                                    .font(t::sans(12.0))
                                    .color(t::WAIT),
                                );
                                if let Some(e) = &rec.last_error {
                                    ui.label(
                                        RichText::new(format!("Last error: {e}"))
                                            .font(t::sans(12.0))
                                            .color(t::BAD),
                                    );
                                }
                            }
                        }
                    }
                    if kit::tool_button(ui, "Restart audio", kit::Kind::Ghost, true).clicked() {
                        restart = true;
                    }
                });
            });
        });
        card(ui, "input (recording)", |ui| {
            row(ui, "Device", None, |ui| {
                let shown = s
                    .audio
                    .input
                    .clone()
                    .unwrap_or_else(|| "System default".into());
                kit::dropdown(ui, "audio-in", &shown, 380.0, |ui| {
                    if ui
                        .selectable_label(s.audio.input.is_none(), "System default")
                        .clicked()
                    {
                        s.audio.input = None;
                    }
                    for d in inputs {
                        if ui
                            .selectable_label(s.audio.input.as_ref() == Some(d), d)
                            .clicked()
                        {
                            s.audio.input = Some(d.clone());
                        }
                    }
                });
            });
            row(
                ui,
                "Record input",
                Some("A take per pass while Record is on."),
                |ui| {
                    let mut on = self.rec.input;
                    if kit_switch(ui, &mut on) {
                        self.rec.input = !on;
                        self.toggle_input();
                    }
                },
            );
        });
        restart
    }

    fn midi_page(&mut self, ui: &mut egui::Ui, s: &mut Settings) -> (bool, bool) {
        let (mut panic, mut rescan) = (false, false);
        card(ui, "input devices", |ui| {
            let ports = self
                .midi_in
                .as_ref()
                .map(|m| m.ports.clone())
                .unwrap_or_default();
            if self.midi_in.is_none() {
                ui.label(
                    RichText::new("MIDI input is off in this run.")
                        .font(t::sans(13.0))
                        .color(t::FAINT),
                );
            } else if ports.is_empty() {
                ui.label(
                    RichText::new(
                        "No MIDI inputs found. Plug in a keyboard; it connects by itself.",
                    )
                    .font(t::sans(13.0))
                    .color(t::FAINT),
                );
            }
            for p in &ports {
                ui.horizontal(|ui| {
                    let mut on = s.midi.is_on(&p.name);
                    if kit_switch(ui, &mut on) {
                        s.midi.set_on(&p.name, on);
                    }
                    ui.label(RichText::new(&p.name).font(t::sans(13.0)).color(t::TEXT));
                    let (text, color) = match (&p.error, p.connected) {
                        (Some(e), _) => (e.clone(), t::BAD),
                        (None, true) => ("connected".into(), t::OK),
                        (None, false) => ("off".into(), t::FAINT),
                    };
                    ui.label(RichText::new(text).font(t::sans(12.0)).color(color));
                });
            }
            let lit = self
                .midi_in
                .as_ref()
                .and_then(|m| m.last_activity)
                .is_some_and(|t| t.elapsed().as_secs_f64() < 0.3);
            ui.horizontal(|ui| {
                if kit::tool_button(ui, "Rescan", kit::Kind::Ghost, true).clicked() {
                    rescan = true;
                }
                if kit::tool_button(ui, "Panic (all notes off)", kit::Kind::Ghost, true).clicked() {
                    panic = true;
                }
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                kit::led(ui.painter(), r.center(), 4.0, lit.then_some(t::OK));
                let from = self
                    .midi_in
                    .as_ref()
                    .and_then(|m| m.last_port.clone())
                    .map(|p| format!("activity · last: {p}"))
                    .unwrap_or_else(|| "activity".into());
                ui.label(RichText::new(from).font(t::sans(12.0)).color(t::FAINT));
            });
            ui.label(RichText::new("Keys play the selected channel: the focused piano roll, else the rack selection.").font(t::sans(12.0)).color(t::FAINT));
        });
        card(ui, "output devices & clock sync", |ui| {
            let out_ports = self
                .midi_out
                .as_ref()
                .map(|m| m.ports.clone())
                .unwrap_or_default();
            if self.midi_out.is_none() {
                ui.label(
                    RichText::new("MIDI output is off in this run.")
                        .font(t::sans(13.0))
                        .color(t::FAINT),
                );
            } else if out_ports.is_empty() {
                ui.label(
                    RichText::new("No MIDI outputs found. Connect hardware gear or a virtual port, then Rescan.")
                        .font(t::sans(13.0))
                        .color(t::FAINT),
                );
            }
            for p in &out_ports {
                ui.horizontal(|ui| {
                    let mut on = s.midi.is_out_on(&p.name);
                    if kit_switch(ui, &mut on) {
                        s.midi.set_out_on(&p.name, on);
                    }
                    ui.label(RichText::new(&p.name).font(t::sans(13.0)).color(t::TEXT));
                    let (text, color) = match (&p.error, p.connected) {
                        (Some(e), _) => (e.clone(), t::BAD),
                        (None, true) => ("connected".into(), t::OK),
                        (None, false) => ("off".into(), t::FAINT),
                    };
                    ui.label(RichText::new(text).font(t::sans(12.0)).color(color));
                });
            }
            row(
                ui,
                "MIDI Clock",
                Some(
                    "Master sends 24 PPQN clock + Start/Stop to enabled outputs. Slave follows incoming clock.",
                ),
                |ui| {
                    use crate::settings::MidiClockMode;
                    let labels: Vec<&str> = MidiClockMode::ALL.iter().map(|m| m.label()).collect();
                    let cur = MidiClockMode::ALL
                        .iter()
                        .position(|m| *m == s.midi.clock)
                        .unwrap_or(0);
                    let (pick, _) = kit::chips(ui, "midi-clock-mode", &labels, cur, None);
                    if let Some(i) = pick {
                        s.midi.clock = MidiClockMode::ALL[i];
                    }
                },
            );
            if s.midi.clock == crate::settings::MidiClockMode::Master
                && let Some(mo) = &self.midi_out
            {
                let open = mo.connected_count();
                let ticks = mo.ticks();
                let info = if open == 0 {
                    "Enable at least one output port above to send clock.".to_string()
                } else {
                    format!("Sending to {open} port(s) · {ticks} pulses sent")
                };
                ui.label(RichText::new(info).font(t::sans(12.0)).color(if open == 0 {
                    t::WAIT
                } else {
                    t::FAINT
                }));
            }
        });
        card(ui, "notes in", |ui| {
            row(ui, "Channel", None, |ui| {
                let mut names: Vec<String> = vec!["Omni".into()];
                names.extend((1..=16).map(|c| c.to_string()));
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                kit::dropdown(ui, "midi-ch", names[s.midi.channel as usize], 90.0, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui
                            .selectable_label(s.midi.channel as usize == i, *n)
                            .clicked()
                        {
                            s.midi.channel = i as u8;
                        }
                    }
                });
            });
            row(ui, "Velocity", None, |ui| {
                ui.vertical(|ui| {
                    let curves: Vec<&str> = VelocityCurve::ALL.iter().map(|c| c.label()).collect();
                    let (pick, _) = kit::chips(
                        ui,
                        "midi-vel",
                        &curves,
                        VelocityCurve::ALL
                            .iter()
                            .position(|c| *c == s.midi.velocity)
                            .unwrap_or(0),
                        None,
                    );
                    if let Some(i) = pick {
                        s.midi.velocity = VelocityCurve::ALL[i];
                    }
                    if s.midi.velocity == VelocityCurve::Fixed {
                        let mut v = s.midi.fixed_velocity as i32;
                        ui.add(egui::Slider::new(&mut v, 1..=127).text("level"));
                        s.midi.fixed_velocity = v as u8;
                    }
                });
            });
            row(ui, "Transpose", None, |ui| {
                for (text, d) in [("−12", -12i8), ("−1", -1), ("+1", 1), ("+12", 12)] {
                    if kit::tool_button(ui, text, kit::Kind::Ghost, true).clicked() {
                        s.midi.transpose = (s.midi.transpose + d).clamp(-48, 48);
                    }
                }
                ui.label(
                    RichText::new(format!("{:+} semitones", s.midi.transpose))
                        .font(t::sans(13.0))
                        .color(t::TEXT),
                );
                if s.midi.transpose != 0
                    && kit::tool_button(ui, "Reset", kit::Kind::Ghost, true).clicked()
                {
                    s.midi.transpose = 0;
                }
            });
            row(ui, "Sustain pedal", Some("CC 64 holds notes."), |ui| {
                kit_switch(ui, &mut s.midi.sustain);
            });
        });
        card(ui, "recording", |ui| {
            row(
                ui,
                "Quantize",
                Some("Recorded notes snap to the grid."),
                |ui| {
                    kit_switch(ui, &mut s.midi.quantize_record);
                },
            );
            ui.label(RichText::new("Record (R) + Play writes what you play into the selected pattern, one undo step per pass.").font(t::sans(12.0)).color(t::FAINT));
        });
        card(ui, "controller links", |ui| {
            if s.midi.bindings.is_empty() {
                ui.label(
                    RichText::new(
                        "No links yet. Right-click a knob → Link to controller…, then move a knob.",
                    )
                    .font(t::sans(13.0))
                    .color(t::FAINT),
                );
            } else {
                let mut remove: Option<keyframexl::automation::Target> = None;
                for b in s.midi.bindings.clone() {
                    let label = self.auto_label(b.target);
                    row(ui, &label, None, |ui| {
                        ui.label(
                            RichText::new(format!("CH {} · CC {}", b.channel, b.cc))
                                .font(t::mono(12.0))
                                .color(t::MUTED),
                        );
                        if kit::tool_button(ui, "×", kit::Kind::Ghost, true)
                            .on_hover_text("Forget this link")
                            .clicked()
                        {
                            remove = Some(b.target);
                        }
                    });
                }
                if let Some(t) = remove {
                    s.midi.unbind_target(t);
                }
            }
            ui.label(
                RichText::new(
                    "A knob moves its control over the full range. One gesture is one undo step.",
                )
                .font(t::sans(12.0))
                .color(t::FAINT),
            );
        });
        (panic, rescan)
    }
}

fn general_page(ui: &mut egui::Ui, s: &mut Settings, checking_updates: bool) -> bool {
    let mut check_now = false;
    card(ui, "interface", |ui| {
        row(
            ui,
            "Size",
            Some("Ctrl+= and Ctrl+− change it, Ctrl+0 resets."),
            |ui| {
                let shown = format!("{:.0}%", s.general.ui_scale * 100.0);
                kit::dropdown(ui, "gen-ui-scale", &shown, 110.0, |ui| {
                    for v in crate::settings::UI_SCALES {
                        if ui
                            .selectable_label(
                                (v - s.general.ui_scale).abs() < 0.001,
                                format!("{:.0}%", v * 100.0),
                            )
                            .clicked()
                        {
                            s.general.ui_scale = v;
                        }
                    }
                });
                let native = ui.ctx().native_pixels_per_point().unwrap_or(1.0);
                ui.label(
                    RichText::new(format!("system scale {:.0}%", native * 100.0))
                        .font(t::sans(12.0))
                        .color(t::FAINT),
                );
            },
        );
        row(
            ui,
            "Deck monitor",
            Some("Master bus oscilloscope or spectrum in the top transport bar."),
            |ui| {
                use crate::settings::DeckMonitor;
                let labels: Vec<&str> = DeckMonitor::ALL.iter().map(|m| m.label()).collect();
                let cur = DeckMonitor::ALL
                    .iter()
                    .position(|m| *m == s.general.deck_monitor)
                    .unwrap_or(1);
                let (pick, _) = kit::chips(ui, "gen-deck-monitor", &labels, cur, None);
                if let Some(i) = pick {
                    s.general.deck_monitor = DeckMonitor::ALL[i];
                }
            },
        );
        row(
            ui,
            "X11 window",
            Some("Plugin editors open inside our windows, like FL. Restart to apply."),
            |ui| {
                kit_switch(ui, &mut s.general.x11);
            },
        );
        let tr = crate::i18n::strings(s.general.language);
        row(
            ui,
            tr.setting_language,
            Some(tr.setting_language_hint),
            |ui| {
                let names: Vec<&str> = crate::i18n::Language::ALL
                    .iter()
                    .map(|l| l.label())
                    .collect();
                let cur = crate::i18n::Language::ALL
                    .iter()
                    .position(|l| *l == s.general.language)
                    .unwrap_or(0);
                let (pick, _) = kit::chips(ui, "gen-lang", &names, cur, None);
                if let Some(i) = pick {
                    s.general.language = crate::i18n::Language::ALL[i];
                }
            },
        );
        row(
            ui,
            "System title bar",
            Some("The desktop's title bar instead of the menu row with the window buttons."),
            |ui| {
                kit_switch(ui, &mut s.general.system_title_bar);
            },
        );
    });
    card(ui, "projects", |ui| {
        let tr = crate::i18n::strings(s.general.language);
        row(
            ui,
            tr.setting_show_welcome,
            Some(tr.setting_show_welcome_hint),
            |ui| {
                kit_switch(ui, &mut s.general.show_welcome);
            },
        );
        row(ui, "Autosave", None, |ui| {
            let mins = [0u32, 1, 2, 5, 10];
            let names = ["Off", "1 min", "2 min", "5 min", "10 min"];
            let (pick, _) = kit::chips(
                ui,
                "gen-autosave",
                &names,
                mins.iter()
                    .position(|m| *m == s.general.autosave_minutes)
                    .unwrap_or(2),
                None,
            );
            if let Some(i) = pick {
                s.general.autosave_minutes = mins[i];
            }
        });
        row(
            ui,
            "Starter sounds",
            Some("New projects load the packs and .dwp presets."),
            |ui| {
                kit_switch(ui, &mut s.general.default_kit);
            },
        );
        row(
            ui,
            "Ask on close",
            Some("Offer to save unsaved changes."),
            |ui| {
                kit_switch(ui, &mut s.general.confirm_close);
            },
        );
    });
    card(ui, "updates", |ui| {
        row(
            ui,
            "Check automatically",
            Some(
                "Checks GitHub Releases at most once per day on startup. Set KFXL_NO_UPDATE_CHECK=1 to disable.",
            ),
            |ui| {
                kit_switch(ui, &mut s.general.check_updates);
            },
        );
        let tr = crate::i18n::strings(s.general.language);
        row(ui, tr.update_nightly, Some(tr.update_nightly_hint), |ui| {
            kit_switch(ui, &mut s.general.nightly_updates);
        });
        row(ui, "Current version", None, |ui| {
            ui.label(
                RichText::new(format!("v{}", crate::updater::CURRENT_VERSION))
                    .font(t::mono(12.5))
                    .color(t::TEXT),
            );
            if kit::tool_button(
                ui,
                if checking_updates {
                    "Checking…"
                } else {
                    "Check now"
                },
                kit::Kind::Ghost,
                !checking_updates,
            )
            .clicked()
            {
                check_now = true;
            }
            if let Some(ver) = s.general.skipped_version.clone() {
                if kit::tool_button(
                    ui,
                    &format!("Clear skipped (v{ver})"),
                    kit::Kind::Ghost,
                    true,
                )
                .clicked()
                {
                    s.general.skipped_version = None;
                }
            }
        });
    });
    check_now
}

/// Width of the section list on the left.
const NAV_W: f32 = 168.0;

/// Section list: a sunken column, the open section raised with a brand bar.
fn nav(ui: &mut egui::Ui, page: Page, height: f32) -> Option<Page> {
    let (col, _) = ui.allocate_exact_size(vec2(NAV_W, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        col,
        egui::CornerRadius {
            nw: 0,
            sw: 10,
            ne: 0,
            se: 0,
        },
        t::SIDE,
    );
    p.vline(
        col.right(),
        col.y_range(),
        egui::Stroke::new(1.0_f32, t::EDGE),
    );
    let mut pick = None;
    for (i, sec) in Page::ALL.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            col.left_top() + vec2(10.0, 14.0 + i as f32 * 38.0),
            vec2(NAV_W - 20.0, 32.0),
        );
        let resp = ui.interact(r, ui.id().with(("settings-nav", i)), egui::Sense::click());
        let on = *sec == page;
        let fill = if on {
            t::CTL_TOP
        } else if resp.hovered() {
            t::RAISED
        } else {
            t::SIDE
        };
        p.rect_filled(r, 7.0, fill);
        if on {
            p.rect_filled(
                egui::Rect::from_min_size(r.left_top() + vec2(0.0, 8.0), vec2(3.0, 16.0)),
                1.5,
                t::BRAND,
            );
        }
        p.text(
            r.left_center() + vec2(14.0, 0.0),
            Align2::LEFT_CENTER,
            sec.label(),
            t::sans(14.0),
            if on { t::TEXT } else { t::MUTED },
        );
        if resp.clicked() {
            pick = Some(*sec);
        }
    }
    pick
}

/// `PipeWire · device · 48000 Hz · 256 frames · 5.3 ms`
fn audio_summary(r: &crate::audio::Running) -> String {
    let buf = r
        .buffer
        .map(|b| format!("{b} frames"))
        .unwrap_or_else(|| "default buffer".into());
    format!(
        "{} · {} · {} Hz · {buf} · {:.1} ms",
        r.backend.label(),
        r.device,
        r.rate,
        r.latency_ms()
    )
}

/// A small sunken label (backend name).
fn badge(ui: &mut egui::Ui, text: &str, ink: egui::Color32) {
    egui::Frame::new()
        .fill(t::SUNKEN)
        .stroke(egui::Stroke::new(1.0_f32, t::EDGE))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(t::mono(12.0)).color(ink));
        });
}

/// A group of rows under a small caps heading, on a raised panel.
pub(crate) fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    let r = ui
        .allocate_exact_size(vec2(ui.available_width(), 16.0), egui::Sense::hover())
        .0;
    kit::caps(
        ui.painter(),
        r.left_center() + vec2(2.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        11.0,
        t::FAINT,
    );
    egui::Frame::new()
        .fill(t::SURFACE)
        .stroke(egui::Stroke::new(1.0_f32, t::EDGE))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            body(ui);
        });
}

/// One setting: its name on the left, the control on the right, an optional
/// note under both.
pub(crate) fn row(
    ui: &mut egui::Ui,
    name: &str,
    note: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(150.0);
            ui.add_space(4.0);
            ui.label(RichText::new(name).font(t::sans(13.0)).color(t::TEXT));
        });
        ui.horizontal_wrapped(|ui| control(ui));
    });
    if let Some(n) = note {
        ui.add_space(-4.0);
        ui.label(RichText::new(n).font(t::sans(11.5)).color(t::FAINT));
    }
}

/// An on/off switch (TE-style pill). Returns true when flipped.
pub(crate) fn kit_switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 20.0), egui::Sense::click());
    let k = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let p = ui.painter();
    p.rect_filled(r, 10.0, if *on { t::BRAND_FILL } else { t::CTL_BOTTOM });
    p.rect_stroke(
        r,
        10.0,
        egui::Stroke::new(1.0_f32, if resp.hovered() { t::STRONG } else { t::EDGE }),
        egui::StrokeKind::Inside,
    );
    let x = r.left() + 10.0 + k * (r.width() - 20.0);
    p.circle_filled(
        egui::pos2(x, r.center().y),
        7.0,
        if *on { t::ON_BRAND } else { t::MUTED },
    );
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

#[cfg(test)]
mod midi_rec_tests {
    use super::*;
    use keyframexl::sequence::Song;

    #[test]
    fn recorded_take_grows_the_pattern_past_the_loop() {
        let mut app = App::with_song(Song::default(), None);
        assert_eq!(app.pattern().length, BAR);
        // A note played on the second pass lands in bar 2, not wrapped into bar 1.
        app.record_note(4, 60, 100, BAR + 120, BAR + 240);
        assert_eq!(app.pattern().length, 2 * BAR);
        assert!(app.song.validate().is_ok());
        // One take is one undo step.
        let history = app.history.len();
        app.record_note(4, 62, 90, BAR + 360, BAR + 480);
        assert_eq!(app.history.len(), history, "same take, no new undo step");
        assert!(app.undo());
        assert!(
            app.pattern().notes.is_empty(),
            "undo takes the whole take back"
        );
        assert_eq!(app.pattern().length, BAR);
        // frame_tick does not wrap into the first loop.
        let two_bars = (2.0 * BAR as f64 / STEP as f64 * app.frames_per_step()) as u64;
        assert_eq!(app.frame_tick(two_bars), 2 * BAR);
    }

    #[test]
    fn controller_learn_binds_and_drives_controls() {
        use keyframexl::automation::Target;
        let mut app = App::with_song(Song::default(), None);
        // Arming learn: the next CC binds, device-agnostic on channel + number.
        app.midi_learn = Some(Target::MixerPan { track: 1 });
        app.on_midi(
            4,
            Event::Control {
                channel: 3,
                cc: 74,
                value: 100,
            },
            false,
        );
        assert_eq!(app.midi_learn, None);
        assert_eq!(
            app.settings.midi.binding_for_cc(3, 74).map(|b| b.target),
            Some(Target::MixerPan { track: 1 })
        );
        // A bound CC maps 0..127 over the target's range.
        app.apply_midi_cc(3, 74, 0);
        assert_eq!(app.song.mixer[1].pan, -1.0);
        app.apply_midi_cc(3, 74, 127);
        assert_eq!(app.song.mixer[1].pan, 1.0);
        app.settings
            .midi
            .bind(1, 7, Target::MixerVolume { track: 0 });
        app.apply_midi_cc(1, 7, 127);
        assert!((app.song.mixer[0].volume - keyframexl::automation::VOLUME_MAX).abs() < 1e-6);
        app.apply_midi_cc(1, 7, 0);
        assert_eq!(app.song.mixer[0].volume, 0.0);
        // Esc cancels an armed link.
        app.midi_learn = Some(Target::ChannelVolume { inst: 4 });
        app.cancel_midi_learn();
        assert_eq!(app.midi_learn, None);
        assert!(
            app.settings
                .midi
                .binding_for_target(Target::ChannelVolume { inst: 4 })
                .is_none()
        );
        assert!(app.song.validate().is_ok());
    }

    #[test]
    fn controller_gesture_is_one_undo_step() {
        use keyframexl::automation::Target;
        let mut app = App::with_song(Song::default(), None);
        app.settings.midi.bind(1, 74, Target::MixerPan { track: 1 });
        let base = app.history.len();
        app.apply_midi_cc(1, 74, 0);
        assert_eq!(app.history.len(), base + 1);
        app.apply_midi_cc(1, 74, 127);
        assert_eq!(
            app.history.len(),
            base + 1,
            "moves within 0.6 s join one step"
        );
        assert_eq!(app.song.mixer[1].pan, 1.0);
        app.midi_cc_last = Some(std::time::Instant::now() - std::time::Duration::from_millis(700));
        app.apply_midi_cc(1, 74, 0);
        assert_eq!(
            app.history.len(),
            base + 2,
            "a later move starts a new step"
        );
        assert!(app.undo());
        assert_eq!(
            app.song.mixer[1].pan, 1.0,
            "undo takes the last gesture back"
        );
        assert!(app.undo());
        assert_eq!(app.song.mixer[1].pan, 0.0, "and the first gesture too");
    }
}
