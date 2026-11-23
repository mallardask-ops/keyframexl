//! Forge Studio editor: layered SFX workstation with interactive layer
//! timeline, modern procedural/sample sources, curves with one-click shapes,
//! live Round-Robin variations, searchable presets & layer blocks, and WYSIWYG
//! game / Sound Pack export.

mod curves;
mod inspector;
mod tabs;
mod timeline;

use crate::i18n::fill;
use crate::theme as t;
use crate::ui::synth::Edit;
use crate::ui::{App, Tone, kit};
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::forge::{
    self, Curve, ForgePatch, ForgeSound, GrainSample, Layer, MAX_LAYERS, ModalMaterial, Rendered,
    Source, export, library::Category, mutate, recipes::Recipe,
};
use keyframexl::fx::{FxKind, FxSlot};
use keyframexl::sampler::SamplePreset;
use keyframexl::sequence::InstrumentKind;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::Instant;

/// Effects that suit short game sounds, in the add menu's order.
pub(super) const FX_MENU: [FxKind; 14] = [
    FxKind::Bitcrusher,
    FxKind::Drive,
    FxKind::Wavefolder,
    FxKind::Squeeze,
    FxKind::RingMod,
    FxKind::Phaser,
    FxKind::Chorus,
    FxKind::Tremolo,
    FxKind::Filter,
    FxKind::Delay,
    FxKind::Reverb,
    FxKind::RoomReverb,
    FxKind::Compressor,
    FxKind::Transient,
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Design,
    Presets,
    Variations,
    Export,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PresetSub {
    #[default]
    Full,
    Blocks,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PresetFilter {
    #[default]
    All,
    Loops,
    Category(Category),
    User,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Line {
    #[default]
    Pitch,
    Volume,
    Filter,
    Pan,
    Morph,
}

#[derive(Clone)]
pub(super) struct VariationSlot {
    pub patch: ForgePatch,
    pub sound: Arc<Rendered>,
    pub pinned: bool,
    pub seed: u64,
}

/// One window's view state (egui memory, not the project).
#[derive(Clone)]
pub(super) struct View {
    pub tab: Tab,
    pub layer: usize,
    pub line: Line,
    pub rr_enabled: bool,
    pub rr_cursor: usize,
    pub auto_audition: bool,
    pub ab_slot_b: bool,
    pub ab_other: Option<ForgePatch>,
    pub preset_sub: PresetSub,
    pub preset_filter: PresetFilter,
    pub search: String,
    pub variations: Vec<VariationSlot>,
    pub heard: Option<usize>,
    pub seed: u64,
    pub count: f64,
    pub spread: f32,
    pub format_idx: usize,
    pub rate_44: bool,
    pub mono: bool,
    pub loudness_idx: usize,
    pub export_pack: bool,
    pub template: String,
    /// Pending auto-audition flag: set during drag/edit, triggers on pointer release.
    pub dirty_for_auto: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            tab: Tab::Design,
            layer: 0,
            line: Line::Pitch,
            rr_enabled: false,
            rr_cursor: 0,
            auto_audition: true,
            ab_slot_b: false,
            ab_other: None,
            preset_sub: PresetSub::Full,
            preset_filter: PresetFilter::All,
            search: String::new(),
            variations: Vec::new(),
            heard: None,
            seed: 1,
            count: 6.0,
            spread: 0.18,
            format_idx: 0,
            rate_44: false,
            mono: false,
            loudness_idx: 0,
            export_pack: false,
            template: "{name}_{nn}".into(),
            dirty_for_auto: false,
        }
    }
}

impl View {
    pub(super) fn file_format(&self) -> export::FileFormat {
        match self.format_idx {
            1 => export::FileFormat::Wav24,
            2 => export::FileFormat::Ogg,
            _ => export::FileFormat::Wav,
        }
    }

    pub(super) fn export_options(&self, stem: String, id: u64) -> export::Options {
        let loudness = match self.loudness_idx {
            1 => export::Loudness::PeakDb(-1.0),
            2 => export::Loudness::Lufs(-14.0),
            _ => export::Loudness::AsMixed,
        };
        export::Options {
            count: (self.count.round() as usize).clamp(1, 12),
            format: self.file_format(),
            rate: if self.rate_44 { 44_100 } else { 48_000 },
            normalize: self.loudness_idx == 1,
            loudness,
            mono: self.mono,
            stem,
            template: self.template.clone(),
            seed: self.seed.wrapping_add(id),
        }
    }
}

/// Where a bounced channel note goes.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Into {
    NewLayer,
    Layer(usize),
}

type BounceResult = Result<(u32, Vec<[f32; 2]>), String>;

/// Active audition playhead state for live visual sweep across waveform & timeline.
#[derive(Clone, Copy)]
struct Playhead {
    channel: u64,
    started: Instant,
    duration: f64,
    looping: bool,
}

/// Forge work on worker threads, so knobs never wait for a render.
#[derive(Default)]
pub(crate) struct Jobs {
    /// (channel, patch version, audio).
    renders: Vec<(u64, u64, mpsc::Receiver<Rendered>)>,
    /// Variation rack generation: (channel, slots).
    variations: Option<(u64, mpsc::Receiver<Vec<VariationSlot>>)>,
    /// A channel note being bounced: (Forge channel, target, what, audio).
    bounce: Option<(u64, Into, forge::Bounce, mpsc::Receiver<BounceResult>)>,
    /// Play this channel's sound as soon as its render lands.
    play_when_ready: Option<u64>,
    /// Active Forge audition playhead.
    playhead: Option<Playhead>,
}

impl Jobs {
    fn busy(&self) -> bool {
        !self.renders.is_empty()
            || self.variations.is_some()
            || self.bounce.is_some()
            || self.playhead.is_some()
    }
}

pub(super) fn hz(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

pub(super) fn cutoff_hz(u: f32) -> f64 {
    20.0 * 1000f64.powf(u.clamp(0.0, 1.0) as f64)
}

pub(super) fn source_color(s: &Source) -> Color32 {
    match s {
        Source::Osc { .. } => t::enc_blue(),
        Source::Modal { .. } => t::enc_orange(),
        Source::Grain { .. } => t::enc_green(),
        Source::Noise { .. } => t::enc_white(),
        Source::Sample { .. } => t::wait(),
        Source::Synth { .. } => t::enc_blue(),
    }
}

pub(super) fn source_kind(s: &Source) -> usize {
    match s {
        Source::Osc { .. } => 0,
        Source::Modal { .. } => 1,
        Source::Grain { .. } => 2,
        Source::Noise { .. } => 3,
        Source::Sample { .. } => 4,
        Source::Synth { .. } => 5,
    }
}

/// A knob on an f32 field.
#[allow(clippy::too_many_arguments)]
pub(super) fn knob(
    ui: &mut Ui,
    ed: &mut Edit,
    label: &str,
    color: Color32,
    v: &mut f32,
    range: (f64, f64, f64),
    log: bool,
    fmt: &dyn Fn(f64) -> String,
    hint: &str,
) {
    let mut x = *v as f64;
    ed.knob(ui, label, color, &mut x, range, log, fmt, hint);
    *v = x as f32;
}

/// A pick among a few options (chips): one undo step per pick.
pub(super) fn chips<T: Copy + PartialEq>(
    ui: &mut Ui,
    ed: &mut Edit,
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
        ed.started = true;
        ed.changed = true;
    }
}

/// Min/max columns of a sound in `r`.
pub(super) fn wave(p: &egui::Painter, r: Rect, frames: &[[f32; 2]], color: Color32) {
    p.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::surface()),
    );
    if frames.is_empty() {
        return;
    }
    let cols = r.width().max(1.0) as usize;
    let per = (frames.len() as f32 / cols as f32).max(1.0);
    for c in 0..cols {
        let a = (c as f32 * per) as usize;
        let b = (((c + 1) as f32 * per) as usize).min(frames.len());
        if a >= b {
            continue;
        }
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for f in &frames[a..b] {
            let v = (f[0] + f[1]) * 0.5;
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let x = r.left() + c as f32 + 0.5;
        let h = r.height() * 0.5;
        p.line_segment(
            [
                pos2(x, r.center().y - hi.min(1.0) * h),
                pos2(x, r.center().y - lo.max(-1.0) * h + 0.5),
            ],
            Stroke::new(1.0_f32, color),
        );
    }
}

/// What the layer panels need to know about bouncing: the job running now
/// and the channels a note can come from.
pub(super) struct Bouncing<'a> {
    pub now: Option<(Into, forge::Bounce)>,
    pub channels: &'a [(u64, String)],
    pub deleted: &'static str,
}

impl Bouncing<'_> {
    pub(super) fn name(&self, channel: u64) -> &str {
        self.channels
            .iter()
            .find(|c| c.0 == channel)
            .map_or(self.deleted, |c| c.1.as_str())
    }
}

/// A chain of effects: switch, name, every knob, remove; an add menu below.
pub(super) fn fx_list(ui: &mut Ui, ed: &mut Edit, salt: &str, fx: &mut Vec<FxSlot>) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let mut remove = None;
    for (i, slot) in fx.iter_mut().enumerate() {
        ui.push_id((salt, i), |ui| {
            ui.horizontal_wrapped(|ui| {
                if crate::ui::settings_ui::kit_switch(ui, &mut slot.enabled) {
                    ed.started = true;
                    ed.changed = true;
                }
                let (r, _) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::hover());
                kit::caps(
                    ui.painter(),
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    slot.kind.label(),
                    10.0,
                    if slot.enabled { t::text() } else { t::muted() },
                );
                let specs = slot.kind.params();
                for (n, spec) in specs.iter().enumerate() {
                    let mut v = slot.params[n] as f64;
                    let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
                    ui.add_enabled_ui(slot.enabled, |ui| {
                        ed.knob(
                            ui,
                            &spec.name.to_uppercase(),
                            t::enc_white(),
                            &mut v,
                            (spec.min as f64, spec.max as f64, spec.default as f64),
                            spec.log,
                            &fmt,
                            spec.name,
                        );
                    });
                    slot.params[n] = v as f32;
                }
                if ui
                    .small_button("×")
                    .on_hover_text(tr.forge_remove_fx_tip)
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        fx.remove(i);
        ed.started = true;
        ed.changed = true;
    }
    egui::ComboBox::from_id_salt((salt, "add"))
        .width(150.0)
        .selected_text(
            egui::RichText::new(tr.forge_add_fx)
                .font(t::sans(12.5))
                .color(t::faint()),
        )
        .show_ui(ui, |ui| {
            kit::menu_style(ui);
            for k in FX_MENU {
                if ui.selectable_label(false, k.label()).clicked() {
                    fx.push(FxSlot::new(k));
                    ed.started = true;
                    ed.changed = true;
                }
            }
        });
}

/// "My sounds", read once and kept until one is saved.
pub(super) fn cached_user_sounds(ctx: &egui::Context) -> Arc<Vec<ForgePatch>> {
    let key = egui::Id::new("forge-user");
    if let Some(v) = ctx.data(|d| d.get_temp::<Arc<Vec<ForgePatch>>>(key)) {
        return v;
    }
    let v = Arc::new(forge::user_sounds());
    ctx.data_mut(|d| d.insert_temp(key, v.clone()));
    v
}

/// Clean single-layer starting patch for `Init`.
fn init_patch() -> ForgePatch {
    let mut p = ForgePatch::blank();
    p.name = "Init".into();
    p.layers.push(
        Layer::new("Strike", Source::modal(ModalMaterial::Metal, 480.0), 0.35)
            .with_amp(Curve::flat(1.0)),
    );
    p
}

/// Build `count` variations, keeping any pinned slots from `existing`.
fn build_variations(
    base: &ForgePatch,
    existing: &[VariationSlot],
    count: usize,
    seed: u64,
    spread: f32,
) -> Vec<VariationSlot> {
    (0..count.clamp(1, 12))
        .map(|i| {
            if let Some(prev) = existing.get(i)
                && prev.pinned
            {
                return prev.clone();
            }
            let s = seed.wrapping_mul(7919).wrapping_add(i as u64 * 31 + 1);
            let patch = if i == 0 && existing.is_empty() {
                base.clone()
            } else {
                mutate::variation_at(base, i, s, spread)
            };
            let sound = Arc::new(forge::render::render(&patch));
            VariationSlot {
                patch,
                sound,
                pinned: false,
                seed: s,
            }
        })
        .collect()
}

impl App {
    /// FL: Add → channel → Forge. Starts from the Jump recipe.
    pub(crate) fn add_forge(&mut self) -> u64 {
        let tr = self.tr();
        self.checkpoint();
        let name = self.song.free_name("Forge");
        let patch = self.recipe(Recipe::Jump);
        let id = self.song.add_channel(
            &name,
            InstrumentKind::Forge {
                sound: ForgeSound::new(patch),
            },
        );
        self.publish();
        self.win(super::Cmd::Show(super::Tab::Instrument { id }));
        self.say(Tone::Quiet, fill(tr.forge_added, &[("name", &name)]));
        id
    }

    /// A recipe in the project's legacy game style (kept for `.kfxl` compatibility).
    fn recipe(&self, r: Recipe) -> ForgePatch {
        r.styled(self.song.game_style)
    }

    /// Hear a rendered sound now (looped when `r.seamless_loop` is true).
    fn audition(&mut self, channel: u64, r: &Rendered) {
        self.previewing = None;
        self.preview_rx = None;
        if let Some(a) = &self.audio {
            a.preview_done
                .store(false, std::sync::atomic::Ordering::Relaxed);
            a.preview.store(Some(Arc::new(crate::audio::Preview {
                frames: Arc::new(r.frames.clone()),
                rate: r.rate,
                looping: r.seamless_loop,
            })));
        }
        self.forge_jobs.playhead = Some(Playhead {
            channel,
            started: Instant::now(),
            duration: r.seconds().max(0.01),
            looping: r.seamless_loop,
        });
    }

    fn stop_audition(&mut self) {
        if let Some(a) = &self.audio {
            a.preview.store(None);
            a.preview_done
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.forge_jobs.playhead = None;
    }

    fn forge_sound_mut(&mut self, id: u64) -> Option<&mut ForgeSound> {
        match self
            .song
            .instruments
            .iter_mut()
            .find(|i| i.id == id)
            .map(|i| &mut i.kind)
        {
            Some(InstrumentKind::Forge { sound }) => Some(sound),
            _ => None,
        }
    }

    /// Put `p` into channel `id`. Its audio renders on a worker thread
    /// (`poll_forge`); the engine plays the previous audio meanwhile.
    fn commit_forge(&mut self, id: u64, p: ForgePatch) {
        if let Some(sound) = self.forge_sound_mut(id) {
            sound.edit(p);
        }
        self.publish();
    }

    /// Play channel `id`'s sound: now, or when its render lands.
    fn play_forge(&mut self, id: u64) {
        match self.song.instrument(id).map(|i| &i.kind) {
            Some(InstrumentKind::Forge { sound }) if sound.stale() => {
                self.forge_jobs.play_when_ready = Some(id);
            }
            Some(InstrumentKind::Forge { sound }) => {
                let r = sound.rendered.clone();
                self.audition(id, &r);
            }
            _ => {}
        }
    }

    /// Every frame: take finished work, start renders for stale sounds, advance playhead.
    pub(super) fn poll_forge(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.forge_jobs
            .renders
            .retain(|(inst, ver, rx)| match rx.try_recv() {
                Ok(r) => {
                    done.push((*inst, *ver, r));
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        let mut changed = false;
        for (inst, ver, r) in done {
            let r = Arc::new(r);
            if self
                .forge_sound_mut(inst)
                .is_some_and(|s| s.install(ver, r.clone()))
            {
                changed = true;
                if self.forge_jobs.play_when_ready == Some(inst) {
                    self.forge_jobs.play_when_ready = None;
                    self.audition(inst, &r);
                }
            }
        }
        if changed {
            self.publish();
        }
        for i in &self.song.instruments {
            if let InstrumentKind::Forge { sound } = &i.kind
                && sound.stale()
                && !self.forge_jobs.renders.iter().any(|j| j.0 == i.id)
            {
                let (tx, rx) = mpsc::channel();
                let patch = sound.patch.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(forge::render::render(&patch));
                });
                self.forge_jobs.renders.push((i.id, sound.version(), rx));
            }
        }
        self.poll_bounce();
        // Expire one-shot playhead once finished.
        if let Some(ph) = self.forge_jobs.playhead
            && !ph.looping
            && ph.started.elapsed().as_secs_f64() >= ph.duration
        {
            self.forge_jobs.playhead = None;
        }
        if self.forge_jobs.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        }
    }

    pub(super) fn forge_editor(&mut self, ui: &mut Ui, id: u64, sound: &ForgeSound) {
        let tr = self.tr();
        let ctx = ui.ctx().clone();
        let view_key = egui::Id::new(("forge-view", id));
        let mut view: View = ctx.data_mut(|d| d.get_temp(view_key)).unwrap_or_default();

        if let Some((cid, rx)) = &self.forge_jobs.variations
            && *cid == id
            && let Ok(slots) = rx.try_recv()
        {
            view.variations = slots;
            view.heard = None;
            view.rr_cursor = 0;
            self.forge_jobs.variations = None;
        }
        let growing = self
            .forge_jobs
            .variations
            .as_ref()
            .is_some_and(|j| j.0 == id);
        let bouncing = self
            .forge_jobs
            .bounce
            .as_ref()
            .filter(|j| j.0 == id)
            .map(|j| (j.1, j.2));
        let channels: Vec<(u64, String)> = self
            .song
            .instruments
            .iter()
            .filter(|i| !matches!(i.kind, InstrumentKind::Forge { .. }))
            .map(|i| (i.id, i.name.clone()))
            .collect();
        let forge_channel_count = self
            .song
            .instruments
            .iter()
            .filter(|i| matches!(i.kind, InstrumentKind::Forge { .. }))
            .count();

        let playhead_frac = self.forge_jobs.playhead.and_then(|ph| {
            if ph.channel != id {
                return None;
            }
            let elapsed = ph.started.elapsed().as_secs_f64();
            let frac = if ph.looping {
                (elapsed % ph.duration) / ph.duration
            } else {
                (elapsed / ph.duration).min(1.0)
            };
            Some(frac as f32)
        });
        let looping_now = self
            .forge_jobs
            .playhead
            .is_some_and(|ph| ph.channel == id && ph.looping);

        let mut p = sound.patch.clone();
        let mut ed = Edit::default();
        let mut play = false;
        let mut stop = false;
        let mut adopt: Option<ForgePatch> = None;
        let mut hear: Option<Arc<Rendered>> = None;
        let mut save = false;
        let mut to_playlist = false;
        let mut reroll_all = false;
        let mut reroll_one: Option<usize> = None;
        let mut export_channel = false;
        let mut export_pack = false;
        let mut load_sample: Option<usize> = None;
        let mut load_grain_sample: Option<usize> = None;
        let mut drop_file: Option<std::path::PathBuf> = None;
        let mut bounce: Option<(Into, forge::Bounce)> = None;
        view.layer = view.layer.min(p.layers.len().saturating_sub(1));

        // Spacebar auditions when no text box owns keyboard focus.
        if !ctx.wants_keyboard_input()
            && self.focus == Some(super::Tab::Instrument { id })
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            if looping_now {
                stop = true;
            } else {
                play = true;
            }
        }

        // Top mode tab bar + Transport / Quick Action deck.
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        ui.horizontal_wrapped(|ui| {
            // Hero Play / Stop button.
            if looping_now {
                stop = kit::button_sized(
                    ui,
                    vec2(88.0, kit::CONTROL_H),
                    kit::Kind::Primary,
                    true,
                    tr.forge_stop_caps,
                    |painter, face, ink| {
                        let sq =
                            Rect::from_center_size(face.center() - vec2(22.0, 0.0), vec2(9.0, 9.0));
                        painter.rect_filled(sq, 1.5, ink);
                        painter.text(
                            face.center() + vec2(8.0, 0.0),
                            Align2::CENTER_CENTER,
                            tr.forge_stop_caps,
                            t::mono_medium(12.5),
                            ink,
                        );
                    },
                )
                .clicked();
            } else {
                play = kit::button_sized(
                    ui,
                    vec2(88.0, kit::CONTROL_H),
                    kit::Kind::Primary,
                    true,
                    tr.forge_play,
                    |painter, face, ink| {
                        kit::icon_play(painter, face.center() - vec2(22.0, 0.0), 10.5, ink);
                        painter.text(
                            face.center() + vec2(8.0, 0.0),
                            Align2::CENTER_CENTER,
                            tr.forge_play_caps,
                            t::mono_medium(12.5),
                            ink,
                        );
                    },
                )
                .on_hover_text(tr.forge_play_tip)
                .clicked();
            }

            // Studio toggles: Loop (seamless_loop), RR (round-robin), Auto-audition, A/B compare.
            if toggle_chip(ui, tr.forge_loop_btn, p.seamless_loop, t::enc_green())
                .on_hover_text(tr.forge_loop_tip)
                .clicked()
            {
                p.seamless_loop = !p.seamless_loop;
                ed.started = true;
                ed.changed = true;
                if !p.seamless_loop && looping_now {
                    stop = true;
                }
            }
            let rr_lbl = if view.variations.is_empty() {
                tr.forge_rr_btn.to_string()
            } else {
                format!(
                    "{} {}/{}",
                    tr.forge_rr_btn,
                    (view.rr_cursor % view.variations.len()) + 1,
                    view.variations.len()
                )
            };
            if toggle_chip(ui, &rr_lbl, view.rr_enabled, t::enc_green())
                .on_hover_text(tr.forge_rr_tip)
                .clicked()
            {
                view.rr_enabled = !view.rr_enabled;
                if view.rr_enabled && view.variations.is_empty() {
                    reroll_all = true;
                }
            }
            if toggle_chip(ui, tr.forge_auto_btn, view.auto_audition, t::enc_blue())
                .on_hover_text(tr.forge_auto_tip)
                .clicked()
            {
                view.auto_audition = !view.auto_audition;
            }
            let ab_lbl = if view.ab_slot_b { "B" } else { "A" };
            if toggle_chip(ui, ab_lbl, view.ab_slot_b, t::wait())
                .on_hover_text(tr.forge_ab_tip)
                .clicked()
            {
                let prev = view.ab_other.replace(p.clone());
                view.ab_slot_b = !view.ab_slot_b;
                if let Some(other) = prev {
                    adopt = Some(other);
                }
            }

            ui.separator();

            // Primary navigation tabs: Design | Presets & Blocks | Variations | Export.
            let tabs_list = [Tab::Design, Tab::Presets, Tab::Variations, Tab::Export];
            let tab_idx = tabs_list.iter().position(|x| *x == view.tab).unwrap_or(0);
            let var_tab_lbl = if view.variations.is_empty() {
                tr.forge_tab_variations.to_string()
            } else {
                format!("{} ({})", tr.forge_tab_variations, view.variations.len())
            };
            if let (Some(i), _) = kit::chips(
                ui,
                &format!("forge-tabs-{id}"),
                &[
                    tr.forge_tab_design,
                    tr.forge_tab_presets,
                    var_tab_lbl.as_str(),
                    tr.forge_tab_export,
                ],
                tab_idx,
                None,
            ) {
                view.tab = tabs_list[i];
                if view.tab == Tab::Variations && view.variations.is_empty() && !growing {
                    reroll_all = true;
                }
            }

            // Quick utility actions: Init, Save, → Playlist.
            if ui
                .button(tr.forge_init_btn)
                .on_hover_text(tr.forge_init_tip)
                .clicked()
            {
                adopt = Some(init_patch());
            }
            if ui
                .button(tr.forge_save_my_sounds)
                .on_hover_text(tr.forge_save_my_sounds_tip)
                .clicked()
            {
                save = true;
            }
            if ui
                .button(tr.forge_to_playlist_btn)
                .on_hover_text(tr.forge_to_playlist_tip)
                .clicked()
            {
                to_playlist = true;
            }
        });

        ui.separator();

        // Main workspace content for the active tab.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                match view.tab {
                    Tab::Design => {
                        let jobs = Bouncing {
                            now: bouncing,
                            channels: &channels,
                            deleted: tr.forge_deleted_channel,
                        };
                        let tl = timeline::stage_panel(
                            ui,
                            &mut ed,
                            id,
                            sound,
                            &mut p,
                            &mut view,
                            &jobs,
                            playhead_frac,
                        );
                        play |= tl.play;
                        if tl.load_sample.is_some() {
                            load_sample = tl.load_sample;
                        }
                        if tl.drop_file.is_some() {
                            drop_file = tl.drop_file;
                        }
                        if tl.bounce.is_some() {
                            bounce = tl.bounce;
                        }

                        // Lower split: Selected Layer Inspector (left) + Curve Editor (right).
                        if let Some(l) = p.layers.get_mut(view.layer) {
                            let salt = format!("forge-{id}-{}", view.layer);
                            let ghost = &sound.rendered.frames;
                            let wide = ui.available_width() >= 640.0;
                            if wide {
                                ui.columns(2, |cols| {
                                    let insp = inspector::inspector_panel(
                                        &mut cols[0],
                                        &mut ed,
                                        id,
                                        l,
                                        &view,
                                        &jobs,
                                    );
                                    if insp.load_sample.is_some() {
                                        load_sample = insp.load_sample;
                                    }
                                    if insp.load_grain_sample.is_some() {
                                        load_grain_sample = insp.load_grain_sample;
                                    }
                                    if insp.bounce.is_some() {
                                        bounce = insp.bounce;
                                    }
                                    curves::curves_panel(
                                        &mut cols[1],
                                        &mut ed,
                                        &salt,
                                        l,
                                        &mut view,
                                        ghost,
                                    );
                                });
                            } else {
                                let insp =
                                    inspector::inspector_panel(ui, &mut ed, id, l, &view, &jobs);
                                if insp.load_sample.is_some() {
                                    load_sample = insp.load_sample;
                                }
                                if insp.load_grain_sample.is_some() {
                                    load_grain_sample = insp.load_grain_sample;
                                }
                                if insp.bounce.is_some() {
                                    bounce = insp.bounce;
                                }
                                curves::curves_panel(ui, &mut ed, &salt, l, &mut view, ghost);
                            }
                        }

                        tabs::master_strip(ui, &mut ed, id, &mut p);
                    }
                    Tab::Presets => {
                        let res = tabs::presets_tab(ui, id, &p, &mut view);
                        if let Some(np) = res.adopt {
                            adopt = Some(np);
                            view.tab = Tab::Design;
                        }
                        if let Some((bname, layer)) = res.append_layer
                            && p.layers.len() < MAX_LAYERS
                        {
                            p.layers.push(layer);
                            view.layer = p.layers.len() - 1;
                            view.tab = Tab::Design;
                            ed.started = true;
                            ed.changed = true;
                            self.say(Tone::Quiet, fill(tr.forge_block_added, &[("name", &bname)]));
                        }
                        if res.inspire {
                            view.seed = view.seed.wrapping_add(1);
                            adopt = Some(mutate::mutate(&p, view.seed.wrapping_mul(7919), 0.55));
                            view.tab = Tab::Design;
                        }
                    }
                    Tab::Variations => {
                        let res = tabs::variations_tab(ui, &p, &mut view, growing);
                        reroll_all |= res.reroll_all;
                        reroll_one = res.reroll_one;
                        if res.hear.is_some() {
                            hear = res.hear;
                        }
                        if let Some(np) = res.adopt {
                            adopt = Some(np);
                            view.tab = Tab::Design;
                        }
                    }
                    Tab::Export => {
                        let res = tabs::export_tab(ui, id, &p, &mut view, forge_channel_count);
                        export_channel = res.export_channel;
                        export_pack = res.export_pack;
                    }
                }
            });

        if let Some(np) = adopt {
            p = np;
            ed.started = true;
            ed.changed = true;
        }
        if ed.started {
            self.checkpoint();
        }
        if ed.changed {
            self.commit_forge(id, p.clone());
            if view.auto_audition {
                view.dirty_for_auto = true;
            }
        }
        // Trigger auto-audition once the pointer is released after an edit.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if view.dirty_for_auto && !pointer_down {
            view.dirty_for_auto = false;
            play = true;
        }

        if reroll_all {
            view.seed = view.seed.wrapping_add(1);
            let (tx, rx) = mpsc::channel();
            let (base, existing, count, seed, spread) = (
                p.clone(),
                view.variations.clone(),
                (view.count.round() as usize).clamp(1, 12),
                view.seed,
                view.spread,
            );
            std::thread::spawn(move || {
                let slots = build_variations(&base, &existing, count, seed, spread);
                let _ = tx.send(slots);
            });
            self.forge_jobs.variations = Some((id, rx));
        }
        if let Some(slot_idx) = reroll_one
            && let Some(slot) = view.variations.get_mut(slot_idx)
        {
            view.seed = view.seed.wrapping_add(1);
            let s = view
                .seed
                .wrapping_mul(7919)
                .wrapping_add(slot_idx as u64 + 1);
            let patch = mutate::variation_at(&p, slot_idx, s, view.spread);
            let sound_r = Arc::new(forge::render::render(&patch));
            slot.patch = patch;
            slot.sound = sound_r.clone();
            slot.seed = s;
            view.heard = Some(slot_idx);
            hear = Some(sound_r);
        }

        if let Some((into, b)) = bounce {
            self.forge_bounce(id, into, b);
        }
        if let Some(i) = load_sample {
            self.forge_load_sample(id, i);
        }
        if let Some(i) = load_grain_sample {
            self.forge_load_grain_sample(id, i);
        }
        if let Some(path) = drop_file {
            self.forge_drop_sample(id, &path);
        }
        if stop {
            self.stop_audition();
        } else if play {
            if view.rr_enabled && !view.variations.is_empty() {
                let idx = view.rr_cursor % view.variations.len();
                let r = view.variations[idx].sound.clone();
                view.heard = Some(idx);
                view.rr_cursor = (idx + 1) % view.variations.len();
                self.audition(id, &r);
            } else {
                self.play_forge(id);
            }
        }
        if let Some(r) = hear {
            self.audition(id, &r);
        }
        if save {
            self.forge_save(&ctx, id);
        }
        if to_playlist {
            self.forge_render_to_playlist(id);
        }
        if export_channel {
            self.forge_export(id, &view);
        }
        if export_pack {
            self.forge_export_pack(&view);
        }
        ctx.data_mut(|d| d.insert_temp(view_key, view));
    }

    /// Render the active Forge channel directly onto the Playlist as an audio clip.
    pub(super) fn forge_render_to_playlist(&mut self, id: u64) {
        let tr = self.tr();
        if let Some(sound) = self.forge_sound_mut(id) {
            sound.refresh();
        }
        let Some((name, rendered)) = self.song.instrument(id).and_then(|i| match &i.kind {
            InstrumentKind::Forge { sound } => Some((i.name.clone(), sound.rendered.clone())),
            _ => None,
        }) else {
            return;
        };
        if rendered.frames.is_empty() {
            return;
        }
        let preset = SamplePreset::from_sample(&name, rendered.rate, rendered.frames.clone());
        let track = self.next_free_audio_track();
        self.checkpoint();
        match self.song.add_audio_clip(&name, preset, 0, track) {
            Ok((_inst, pat_id, clip_idx)) => {
                self.select_pattern(pat_id);
                self.clip = Some(clip_idx);
                self.set_song_mode(true);
                self.publish();
                self.say(
                    Tone::Done,
                    fill(
                        tr.forge_playlist_added,
                        &[("name", &name), ("track", &(track + 1).to_string())],
                    ),
                );
            }
            Err(e) => {
                self.history.pop();
                self.say(Tone::Bad, e);
            }
        }
    }

    /// Render one note of a project channel (plugin, sampler, synth) on a
    /// worker thread, the way a song export does, for Forge channel `forge`.
    fn forge_bounce(&mut self, forge: u64, into: Into, b: forge::Bounce) {
        let tr = self.tr();
        let stopped = tr.forge_render_stopped;
        use keyframexl::sequence::{Note, PPQ, Pattern};
        if self.forge_jobs.bounce.is_some() {
            return;
        }
        let mut song = self.song.clone();
        let pid = song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut pat = Pattern::new(pid, "Forge bounce");
        let ticks = ((b.hold as f64 * song.tempo / 60.0) * PPQ as f64)
            .round()
            .max(1.0) as i64;
        pat.notes.push(Note::new(b.channel, 0, ticks, b.key, 100));
        song.patterns.push(pat.into());
        if let Some(i) = song.instruments.iter_mut().find(|i| i.id == b.channel) {
            i.muted = false;
        }
        let plugins = keyframexl::render::Plugins(
            self.plugins
                .live
                .iter()
                .filter_map(|(id, l)| Some((*id, l.proc.clone()?)))
                .collect(),
        );
        let hold = self
            .audio
            .as_ref()
            .filter(|_| !plugins.0.is_empty())
            .map(|a| a.hold_plugins.clone());
        if let Some(h) = &hold {
            h.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let rate = self.rate();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let out = keyframexl::render::render_range(
                &song,
                pid,
                false,
                None,
                keyframexl::render::Tail::Leave,
                rate,
                &plugins,
                &cancel,
                &mut |_| {},
            );
            if let Some(h) = hold {
                h.store(false, std::sync::atomic::Ordering::Relaxed);
            }
            let _ = tx.send(out.map(|f| (rate, f)).ok_or_else(|| stopped.to_string()));
        });
        self.forge_jobs.bounce = Some((forge, into, b, rx));
    }

    /// A finished bounce becomes a layer (one undo step).
    fn poll_bounce(&mut self) {
        let tr = self.tr();
        let Some((forge_id, into, b, rx)) = &self.forge_jobs.bounce else {
            return;
        };
        let (forge_id, into, b) = (*forge_id, *into, *b);
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(tr.forge_render_stopped.into()),
        };
        self.forge_jobs.bounce = None;
        let name = format!(
            "{} {}",
            self.instrument_name(b.channel),
            super::roll::note_name(b.key)
        );
        let (rate, mut frames) = match result {
            Ok(v) => v,
            Err(e) => {
                self.say(
                    Tone::Bad,
                    fill(tr.forge_bounce_err, &[("name", &name), ("err", &e)]),
                );
                return;
            }
        };
        let first = frames
            .iter()
            .position(|f| f[0].abs().max(f[1].abs()) > 1e-4);
        let Some(first) = first else {
            self.say(Tone::Bad, fill(tr.forge_bounce_silence, &[("name", &name)]));
            return;
        };
        frames.drain(..first);
        frames.truncate((forge::MAX_SECONDS * rate as f32) as usize);
        let Some(mut p) = self.forge_patch(forge_id) else {
            return;
        };
        let length = (frames.len() as f32 / rate.max(1) as f32).clamp(0.01, forge::MAX_SECONDS);
        let source = Source::Sample {
            name: name.clone(),
            rate,
            frames: Arc::new(frames),
            reverse: false,
            from: Some(b),
            trim_start: 0.0,
            trim_end: 1.0,
        };
        match into {
            Into::Layer(i) if i < p.layers.len() => {
                let l = &mut p.layers[i];
                l.source = source;
                l.length = length.min(forge::MAX_SECONDS - l.start);
            }
            _ if p.layers.len() < MAX_LAYERS => {
                let mut l = Layer::new(&name, source, length);
                l.amp = Curve::flat(1.0);
                p.layers.push(l);
            }
            _ => {
                self.say(Tone::Bad, tr.forge_layers_full_bounce);
                return;
            }
        }
        self.checkpoint();
        self.commit_forge(forge_id, p);
        self.say(Tone::Quiet, fill(tr.forge_bounced_ok, &[("name", &name)]));
    }

    /// Live check: `KFXL_CAPTURE_FORGE_BOUNCE=out.wav` bounces C5 of the last
    /// plugin channel into the first Forge channel once that plugin has loaded,
    /// then writes the new layer's audio to `out.wav`.
    pub(super) fn capture_forge_bounce(&mut self) {
        use std::sync::atomic::{AtomicU8, Ordering};
        static STATE: AtomicU8 = AtomicU8::new(0);
        let Ok(out) = std::env::var("KFXL_CAPTURE_FORGE_BOUNCE") else {
            return;
        };
        started();
        let forge_id = self
            .song
            .instruments
            .iter()
            .find(|i| matches!(i.kind, InstrumentKind::Forge { .. }))
            .map(|i| i.id);
        let plugin = self
            .song
            .instruments
            .iter()
            .rev()
            .find_map(|i| i.plugin().map(|p| (i.id, p.id)));
        let (Some(forge_id), Some((channel, pid))) = (forge_id, plugin) else {
            return;
        };
        match STATE.load(Ordering::Relaxed) {
            0 if self
                .plugins
                .live
                .get(&pid)
                .is_some_and(|l| l.proc.is_some())
                && started().elapsed().as_secs_f64()
                    >= std::env::var("KFXL_CAPTURE_FORGE_BOUNCE_AFTER")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(25.0) =>
            {
                STATE.store(1, Ordering::Relaxed);
                self.forge_bounce(
                    forge_id,
                    Into::NewLayer,
                    forge::Bounce {
                        channel,
                        key: 60,
                        hold: 0.6,
                    },
                );
            }
            1 if self.forge_jobs.bounce.is_none() => {
                STATE.store(2, Ordering::Relaxed);
                let p = self.forge_patch(forge_id);
                let frames = p.as_ref().and_then(|p| {
                    p.layers.iter().rev().find_map(|l| match &l.source {
                        Source::Sample {
                            frames,
                            rate,
                            from: Some(_),
                            ..
                        } => Some((*rate, frames.clone())),
                        _ => None,
                    })
                });
                match frames {
                    Some((rate, f)) => {
                        let r = keyframexl::render::write_wav_at(
                            Path::new(&out),
                            &f,
                            keyframexl::render::Format::Wav32Float,
                            false,
                            rate,
                        );
                        eprintln!(
                            "forge bounce: {} frames at {rate} Hz -> {out} {r:?}",
                            f.len()
                        );
                    }
                    None => eprintln!("forge bounce: no bounced layer"),
                }
            }
            _ => {}
        }
    }

    fn forge_patch(&self, id: u64) -> Option<ForgePatch> {
        match self.song.instrument(id).map(|i| &i.kind) {
            Some(InstrumentKind::Forge { sound }) => Some(sound.patch.clone()),
            _ => None,
        }
    }

    fn forge_save(&mut self, ctx: &egui::Context, id: u64) {
        let tr = self.tr();
        let Some(p) = self.forge_patch(id) else {
            return;
        };
        match forge::save_user(&p) {
            Ok(path) => {
                ctx.data_mut(|d| d.remove::<Arc<Vec<ForgePatch>>>(egui::Id::new("forge-user")));
                self.say(
                    Tone::Quiet,
                    fill(
                        tr.forge_saved_ok,
                        &[("name", &p.name), ("path", &path.display().to_string())],
                    ),
                );
            }
            Err(e) => self.say(
                Tone::Bad,
                fill(tr.forge_save_err, &[("err", &e.to_string())]),
            ),
        }
    }

    fn forge_export(&mut self, id: u64, view: &View) {
        let tr = self.tr();
        let Some(p) = self.forge_patch(id) else {
            return;
        };
        let Some(dir) = rfd::FileDialog::new()
            .set_title(tr.forge_export_dlg_title)
            .pick_folder()
        else {
            return;
        };
        let opt = view.export_options(p.name.clone(), id);
        let result = if view.variations.is_empty() {
            export::export(&p, &dir, &opt)
        } else {
            let approved: Vec<ForgePatch> =
                view.variations.iter().map(|v| v.patch.clone()).collect();
            export::export_patches(&approved, &dir, &opt)
        };
        match result {
            Ok(paths) => {
                let first = paths
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.say(
                    Tone::Done,
                    tr.forge_exported_files.fmt_with(
                        tr.lang,
                        paths.len(),
                        &[("first", &first), ("dir", &dir.display().to_string())],
                    ),
                );
            }
            Err(e) => self.say(
                Tone::Bad,
                fill(tr.forge_export_err, &[("err", &e.to_string())]),
            ),
        }
    }

    fn forge_export_pack(&mut self, view: &View) {
        let tr = self.tr();
        let opt = view.export_options("pack".into(), 0);
        let channels: Vec<(String, Vec<ForgePatch>)> = self
            .song
            .instruments
            .iter()
            .filter_map(|i| match &i.kind {
                InstrumentKind::Forge { sound } => {
                    let mut ch_opt = opt.clone();
                    ch_opt.stem = i.name.clone();
                    ch_opt.seed = view.seed.wrapping_add(i.id);
                    Some((i.name.clone(), export::patches(&sound.patch, &ch_opt)))
                }
                _ => None,
            })
            .collect();
        if channels.is_empty() {
            self.say(Tone::Bad, tr.forge_exp_pack_empty);
            return;
        }
        let Some(dir) = rfd::FileDialog::new()
            .set_title(tr.forge_export_dlg_title)
            .pick_folder()
        else {
            return;
        };
        match export::export_sound_pack(&channels, &dir, &opt) {
            Ok(paths) => {
                let first = paths
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.say(
                    Tone::Done,
                    tr.forge_exported_files.fmt_with(
                        tr.lang,
                        paths.len(),
                        &[("first", &first), ("dir", &dir.display().to_string())],
                    ),
                );
            }
            Err(e) => self.say(
                Tone::Bad,
                fill(tr.forge_export_err, &[("err", &e.to_string())]),
            ),
        }
    }

    /// Pick an audio file for layer `layer` of channel `id`.
    fn forge_load_sample(&mut self, id: u64, layer: usize) {
        let tr = self.tr();
        let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.forge_sample_filter, &["wav", "flac", "ogg"])
            .set_title(tr.forge_sample_dlg_title)
            .pick_file()
        else {
            return;
        };
        self.forge_load_sample_path(id, Some(layer), &path);
    }

    /// Drop an audio file from the Files browser onto the Forge timeline as a new layer.
    fn forge_drop_sample(&mut self, id: u64, path: &Path) {
        self.forge_load_sample_path(id, None, path);
    }

    fn forge_load_sample_path(&mut self, id: u64, layer: Option<usize>, path: &Path) {
        let tr = self.tr();
        let (rate, mut frames) = match crate::import::read_audio(path) {
            Ok(v) => v,
            Err(e) => {
                self.say(
                    Tone::Bad,
                    fill(tr.forge_sample_read_err, &[("err", &e.to_string())]),
                );
                return;
            }
        };
        frames.truncate((forge::MAX_SECONDS * rate as f32) as usize);
        let Some(mut p) = self.forge_patch(id) else {
            return;
        };
        let name = super::instrument::file_name(&path.to_string_lossy());
        let short = name
            .rsplit_once('.')
            .map_or(name.clone(), |(s, _)| s.to_string());
        let src = Source::Sample {
            name: name.clone(),
            rate,
            frames: Arc::new(frames),
            reverse: false,
            from: None,
            trim_start: 0.0,
            trim_end: 1.0,
        };
        let len = match &src {
            Source::Sample { frames, .. } => {
                (frames.len() as f32 / rate.max(1) as f32).clamp(0.01, forge::MAX_SECONDS)
            }
            _ => 0.3,
        };
        match layer {
            Some(idx) if idx < p.layers.len() => {
                let l = &mut p.layers[idx];
                l.length = len.min(forge::MAX_SECONDS - l.start);
                l.source = src;
                l.name = short;
                l.pitch = Curve::flat(0.0);
            }
            _ if p.layers.len() < MAX_LAYERS => {
                let mut l = Layer::new(&short, src, len);
                l.amp = Curve::flat(1.0);
                l.pitch = Curve::flat(0.0);
                p.layers.push(l);
            }
            _ => {
                self.say(Tone::Bad, tr.forge_layers_full_bounce);
                return;
            }
        }
        self.checkpoint();
        self.commit_forge(id, p);
        self.say(
            Tone::Quiet,
            fill(tr.forge_sample_loaded, &[("name", &name)]),
        );
    }

    /// Pick an audio file as a sample cloud for a `Source::Grain` layer.
    fn forge_load_grain_sample(&mut self, id: u64, layer: usize) {
        let tr = self.tr();
        let Some(path) = rfd::FileDialog::new()
            .add_filter(tr.forge_sample_filter, &["wav", "flac", "ogg"])
            .set_title(tr.forge_sample_dlg_title)
            .pick_file()
        else {
            return;
        };
        let (rate, mut frames) = match crate::import::read_audio(&path) {
            Ok(v) => v,
            Err(e) => {
                self.say(
                    Tone::Bad,
                    fill(tr.forge_sample_read_err, &[("err", &e.to_string())]),
                );
                return;
            }
        };
        frames.truncate((forge::MAX_SECONDS * rate as f32) as usize);
        let Some(mut p) = self.forge_patch(id) else {
            return;
        };
        let Some(l) = p.layers.get_mut(layer) else {
            return;
        };
        let name = super::instrument::file_name(&path.to_string_lossy());
        if let Source::Grain { sample, .. } = &mut l.source {
            *sample = Some(GrainSample {
                name: name.clone(),
                rate,
                frames: Arc::new(frames),
            });
            self.checkpoint();
            self.commit_forge(id, p);
            self.say(
                Tone::Quiet,
                fill(tr.forge_sample_loaded, &[("name", &name)]),
            );
        }
    }
}

/// Compact hardware toggle chip with an address-colored LED dot.
fn toggle_chip(ui: &mut Ui, label: &str, on: bool, dot_color: Color32) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), t::sans(12.0), t::text());
    let size = vec2(galley.size().x + 24.0, 26.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if on {
        t::sunken()
    } else if resp.hovered() {
        t::ctl_hover()
    } else {
        t::raised()
    };
    let stroke = Stroke::new(1.0_f32, if on { t::strong() } else { t::line() });
    painter.rect(r, 6.0, fill, stroke, egui::StrokeKind::Inside);
    painter.circle_filled(
        pos2(r.left() + 9.0, r.center().y),
        3.0,
        if on { dot_color } else { t::faint() },
    );
    painter.galley(
        pos2(r.left() + 17.0, r.center().y - galley.size().y * 0.5),
        galley,
        if on { t::text() } else { t::muted() },
    );
    resp
}

/// When the program started (first asked).
fn started() -> Instant {
    static AT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *AT.get_or_init(Instant::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
    use keyframexl::forge::recipes::Style;

    /// Run one frame of the curve editor; returns the drawing rect it used.
    fn frame(
        ctx: &egui::Context,
        c: &mut Curve,
        kind: Line,
        ed: &mut Edit,
        events: Vec<Event>,
    ) -> Rect {
        let now = ctx.input(|i| i.time) + 0.05;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 400.0))),
            time: Some(now),
            events,
            ..Default::default()
        };
        let mut area = Rect::NOTHING;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let top = ui.cursor().min;
                area = Rect::from_min_size(top, vec2(ui.available_width(), 150.0)).shrink(10.0);
                curves::curve_editor(ui, ed, "t", c, kind, t::enc_blue(), 1.0, &[]);
            });
        });
        area
    }

    fn press(pos: Pos2, down: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: down,
            modifiers: Modifiers::default(),
        }
    }

    fn drag(ctx: &egui::Context, c: &mut Curve, kind: Line, a: Pos2, b: Pos2) -> Edit {
        let mut ed = Edit::default();
        frame(ctx, c, kind, &mut ed, vec![Event::PointerMoved(a)]);
        frame(ctx, c, kind, &mut ed, vec![press(a, true)]);
        frame(
            ctx,
            c,
            kind,
            &mut ed,
            vec![Event::PointerMoved(a + (b - a) * 0.5)],
        );
        frame(ctx, c, kind, &mut ed, vec![Event::PointerMoved(b)]);
        frame(ctx, c, kind, &mut ed, vec![press(b, false)]);
        ed
    }

    #[test]
    fn dragging_a_point_moves_it_and_pitch_snaps_to_semitones() {
        let ctx = egui::Context::default();
        let mut c = Curve::new(&[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)]);
        let r = frame(&ctx, &mut c, Line::Pitch, &mut Edit::default(), vec![]);
        let to = pos2(
            r.left() + r.width() * 0.5,
            r.center().y - r.height() * 7.3 / 24.0,
        );
        let ed = drag(&ctx, &mut c, Line::Pitch, r.left_center(), to);
        assert!(ed.started && ed.changed);
        assert!((c.points[0].t - 0.5).abs() < 0.02, "{:?}", c.points);
        assert_eq!(c.points[0].v, 7.0);
    }

    #[test]
    fn double_click_adds_and_removes_points() {
        let ctx = egui::Context::default();
        let mut c = Curve::new(&[(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)]);
        let r = frame(&ctx, &mut c, Line::Volume, &mut Edit::default(), vec![]);
        let at = pos2(r.left() + r.width() * 0.25, r.top() + r.height() * 0.1);
        let dbl = |c: &mut Curve, p: Pos2| {
            let mut ed = Edit::default();
            for _ in 0..12 {
                frame(&ctx, c, Line::Volume, &mut ed, vec![]);
            }
            frame(&ctx, c, Line::Volume, &mut ed, vec![Event::PointerMoved(p)]);
            for _ in 0..2 {
                frame(&ctx, c, Line::Volume, &mut ed, vec![press(p, true)]);
                frame(&ctx, c, Line::Volume, &mut ed, vec![press(p, false)]);
            }
            ed
        };
        let ed = dbl(&mut c, at);
        assert!(ed.started);
        assert_eq!(c.points.len(), 3);
        assert!(
            (c.points[1].t - 0.25).abs() < 0.02 && (c.points[1].v - 0.9).abs() < 0.03,
            "{:?}",
            c.points
        );
        dbl(&mut c, at);
        assert_eq!(c.points.len(), 2, "double-click on a point removes it");
        dbl(&mut c, r.left_top());
        assert_eq!(c.points.len(), 2, "two points always stay");
    }

    #[test]
    fn dragging_the_line_bends_it() {
        let ctx = egui::Context::default();
        let mut c = Curve::new(&[(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)]);
        let r = frame(&ctx, &mut c, Line::Volume, &mut Edit::default(), vec![]);
        let mid = pos2(r.center().x, r.center().y);
        drag(&ctx, &mut c, Line::Volume, mid, mid - vec2(0.0, 40.0));
        assert!(c.points[0].tension < -0.3, "{:?}", c.points);
        assert!(c.at(0.25) > 0.4);
    }

    #[test]
    fn a_forge_channel_opens_its_window_and_renders_all_tabs() {
        let mut app = App::with_song(keyframexl::sequence::Song::default(), None);
        let id = app.add_forge();
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut tab = super::super::Tab::Instrument { id };
        for mode in [Tab::Design, Tab::Presets, Tab::Variations, Tab::Export] {
            let view_key = egui::Id::new(("forge-view", id));
            ctx.data_mut(|d| {
                let mut v: View = d.get_temp(view_key).unwrap_or_default();
                v.tab = mode;
                d.insert_temp(view_key, v);
            });
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1180.0, 680.0))),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
                });
            }
        }
        let Some(InstrumentKind::Forge { sound }) = app.song.instrument(id).map(|i| &i.kind) else {
            panic!("no forge channel");
        };
        assert_eq!(sound.patch, Recipe::Jump.patch());
        assert!(sound.rendered.peak > 0.05);
    }

    fn app() -> App {
        App::with_song(keyframexl::sequence::Song::default(), None)
    }

    fn sound(app: &App, id: u64) -> ForgeSound {
        match app.song.instrument(id).map(|i| &i.kind) {
            Some(InstrumentKind::Forge { sound }) => sound.clone(),
            _ => panic!("no forge channel"),
        }
    }

    fn settle(app: &mut App) {
        let ctx = egui::Context::default();
        let until = Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.poll_forge(&ctx);
            let stale = app
                .song
                .instruments
                .iter()
                .any(|i| matches!(&i.kind, InstrumentKind::Forge { sound } if sound.stale()));
            if !app.forge_jobs.busy() && !stale {
                return;
            }
            assert!(Instant::now() < until, "forge work never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn edits_render_in_the_background() {
        let mut app = app();
        let id = app.add_forge();
        let before = sound(&app, id).rendered.clone();
        app.checkpoint();
        app.commit_forge(id, Recipe::Laser.patch());
        let s = sound(&app, id);
        assert!(
            s.stale() && Arc::ptr_eq(&s.rendered, &before),
            "the edit returns before rendering"
        );
        app.poll_forge(&egui::Context::default());
        app.commit_forge(id, Recipe::Hit.patch());
        settle(&mut app);
        let s = sound(&app, id);
        assert!(!s.stale());
        assert_eq!(*s.rendered, forge::render::render(&Recipe::Hit.patch()));
        app.undo();
        settle(&mut app);
        assert_eq!(*sound(&app, id).rendered, *before);
    }

    #[test]
    fn the_game_style_shapes_new_sounds() {
        let mut app = app();
        app.song.game_style = Some(Style::Retro);
        let id = app.add_forge();
        let mut want = Recipe::Jump.patch();
        Style::Retro.apply(&mut want);
        want.tidy();
        assert_eq!(sound(&app, id).patch, want);
    }

    #[test]
    fn render_to_playlist_adds_an_audio_clip() {
        let mut app = app();
        let id = app.add_forge();
        let clips_before = app.song.clips.len();
        app.forge_render_to_playlist(id);
        assert_eq!(app.song.clips.len(), clips_before + 1);
    }

    #[test]
    fn build_variations_preserves_pinned_slots_and_locked_layers() {
        let mut base = Recipe::Hit.patch();
        base.layers[0].locked = true;
        let first = build_variations(&base, &[], 4, 1, 0.25);
        assert_eq!(first.len(), 4);
        assert_eq!(first[1].patch.layers[0], base.layers[0]);

        let mut pinned = first.clone();
        pinned[2].pinned = true;
        let rerolled = build_variations(&base, &pinned, 4, 2, 0.25);
        assert_eq!(
            rerolled[2].patch, first[2].patch,
            "pinned slot must survive reroll"
        );
        assert_ne!(
            rerolled[1].patch, first[1].patch,
            "unpinned slot must reroll"
        );
    }

    #[test]
    fn a_channel_note_bounces_into_a_layer() {
        let mut app = app();
        let tide = app.song.add_channel(
            "Tide",
            InstrumentKind::Synth {
                patch: keyframexl::synth::SynthPatch::factory(2),
            },
        );
        let id = app.add_forge();
        let layers = sound(&app, id).patch.layers.len();
        let b = forge::Bounce {
            channel: tide,
            key: 60,
            hold: 0.3,
        };
        app.forge_bounce(id, Into::NewLayer, b);
        settle(&mut app);
        let s = sound(&app, id);
        assert_eq!(s.patch.layers.len(), layers + 1);
        let Source::Sample { frames, from, .. } = &s.patch.layers[layers].source else {
            panic!("not a sample layer");
        };
        assert!(!frames.is_empty());
        assert_eq!(*from, Some(b));
        let b2 = forge::Bounce { key: 72, ..b };
        app.forge_bounce(id, Into::Layer(layers), b2);
        settle(&mut app);
        let s2 = sound(&app, id);
        assert_eq!(s2.patch.layers.len(), layers + 1);
        assert!(
            matches!(&s2.patch.layers[layers].source, Source::Sample { from: Some(f), .. } if f.key == 72)
        );
    }
}
