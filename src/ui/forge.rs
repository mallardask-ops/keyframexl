//! Forge editor: build a game sound from layers. Top to bottom it reads as
//! the job goes: pick a start (recipe or saved sound), hear it, turn the four
//! character knobs, grow relatives with Mutate, then open the layers, draw
//! their pitch, volume and filter lines, add effects, and export the files.

use super::synth::{Edit, panel, pct, secs, signed};
use super::{App, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::forge::{
    self, Curve, CurvePoint, FilterMode, ForgePatch, ForgeSound, Layer, MAX_LAYERS, NoiseColor,
    PITCH_RANGE, Rendered, Source, Wave, export, mutate,
    recipes::{Recipe, Style},
};
use keyframexl::fx::{FxKind, FxSlot};
use keyframexl::sequence::InstrumentKind;
use std::sync::{Arc, mpsc};

/// Mutate makes this many relatives.
const IDEAS: usize = 8;

/// Effects that suit short game sounds, in the add menu's order.
const FX_MENU: [FxKind; 14] = [
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

#[derive(Clone, Copy, Default, PartialEq)]
enum Line {
    #[default]
    Pitch,
    Volume,
    Filter,
}

#[derive(Clone)]
struct Idea {
    patch: ForgePatch,
    sound: Arc<Rendered>,
}

/// One window's view state (egui memory, not the project).
#[derive(Clone)]
struct View {
    layer: usize,
    line: Line,
    ideas: Vec<Idea>,
    /// The idea last heard (its tile is lit).
    heard: Option<usize>,
    seed: u64,
    count: f64,
    ogg: bool,
    rate_44: bool,
    normalize: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            layer: 0,
            line: Line::Pitch,
            ideas: Vec::new(),
            heard: None,
            seed: 1,
            count: 5.0,
            ogg: false,
            rate_44: false,
            normalize: true,
        }
    }
}

/// Where a bounced channel note goes.
#[derive(Clone, Copy, PartialEq)]
enum Into {
    NewLayer,
    Layer(usize),
}

type BounceResult = Result<(u32, Vec<[f32; 2]>), String>;

/// Forge work on worker threads, so knobs never wait for a render.
#[derive(Default)]
pub(crate) struct Jobs {
    /// (channel, patch version, audio).
    renders: Vec<(u64, u64, mpsc::Receiver<Rendered>)>,
    /// Mutate: (channel, ideas).
    ideas: Option<(u64, mpsc::Receiver<Vec<Idea>>)>,
    /// A channel note being bounced: (Forge channel, target, what, audio).
    bounce: Option<(u64, Into, forge::Bounce, mpsc::Receiver<BounceResult>)>,
    /// Play this channel's sound as soon as its render lands.
    play_when_ready: Option<u64>,
}

impl Jobs {
    fn busy(&self) -> bool {
        !self.renders.is_empty() || self.ideas.is_some() || self.bounce.is_some()
    }
}

fn hz(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

fn cutoff_hz(u: f32) -> f64 {
    20.0 * 1000f64.powf(u.clamp(0.0, 1.0) as f64)
}

/// A knob on an f32 field.
#[allow(clippy::too_many_arguments)]
fn knob(
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
fn chips<T: Copy + PartialEq>(
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
fn wave(p: &egui::Painter, r: Rect, frames: &[[f32; 2]], color: Color32) {
    p.line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0_f32, t::SURFACE),
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

fn source_kind(s: &Source) -> usize {
    match s {
        Source::Osc { .. } => 0,
        Source::Noise { .. } => 1,
        Source::Sample { .. } => 2,
        Source::Synth { .. } => 3,
    }
}

impl App {
    /// FL: Add → channel → Forge. Starts from the Jump recipe.
    pub(crate) fn add_forge(&mut self) -> u64 {
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
        self.say(
            Tone::Quiet,
            format!("{name} added. Pick a recipe, press Play, turn the knobs. Notes play it too: C5 as made."),
        );
        id
    }

    /// A recipe in the project's game style.
    fn recipe(&self, r: Recipe) -> ForgePatch {
        let mut p = r.patch();
        if let Some(s) = self.song.game_style {
            s.apply(&mut p);
        }
        p
    }

    /// Hear a rendered sound now, without touching the project.
    fn audition(&mut self, r: &Rendered) {
        self.previewing = None;
        self.preview_rx = None;
        if let Some(a) = &self.audio {
            a.preview_done
                .store(false, std::sync::atomic::Ordering::Relaxed);
            a.preview.store(Some(Arc::new(crate::audio::Preview {
                frames: Arc::new(r.frames.clone()),
                rate: r.rate,
            })));
        }
    }

    fn forge_sound_mut(&mut self, id: u64) -> Option<&mut ForgeSound> {
        match self.song.instruments.iter_mut().find(|i| i.id == id).map(|i| &mut i.kind) {
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
                self.audition(&r);
            }
            _ => {}
        }
    }

    /// Every frame: take finished work, start renders for stale sounds.
    pub(super) fn poll_forge(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.forge_jobs.renders.retain(|(inst, ver, rx)| match rx.try_recv() {
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
            if self.forge_sound_mut(inst).is_some_and(|s| s.install(ver, r.clone())) {
                changed = true;
                if self.forge_jobs.play_when_ready == Some(inst) {
                    self.forge_jobs.play_when_ready = None;
                    self.audition(&r);
                }
            }
        }
        if changed {
            self.publish();
        }
        // One render per channel at a time: a drag renders back to back, never piles up.
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
        if self.forge_jobs.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    pub(super) fn forge_editor(&mut self, ui: &mut Ui, id: u64, sound: &ForgeSound) {
        let ctx = ui.ctx().clone();
        let view_key = egui::Id::new(("forge-view", id));
        let mut view: View = ctx.data_mut(|d| d.get_temp(view_key)).unwrap_or_default();
        if let Some((cid, rx)) = &self.forge_jobs.ideas
            && *cid == id
            && let Ok(ideas) = rx.try_recv()
        {
            view.ideas = ideas;
            view.heard = None;
            self.forge_jobs.ideas = None;
        }
        let growing = self.forge_jobs.ideas.as_ref().is_some_and(|j| j.0 == id);
        let bouncing = self.forge_jobs.bounce.as_ref().filter(|j| j.0 == id).map(|j| (j.1, j.2));
        let channels: Vec<(u64, String)> = self
            .song
            .instruments
            .iter()
            .filter(|i| !matches!(i.kind, InstrumentKind::Forge { .. }))
            .map(|i| (i.id, i.name.clone()))
            .collect();
        let game_style = self.song.game_style;
        let mut style_pick: Option<Option<Style>> = None;
        let mut mutate_now = false;
        let mut bounce: Option<(Into, forge::Bounce)> = None;
        let mut p = sound.patch.clone();
        let mut ed = Edit::default();
        let mut play = false;
        let mut adopt: Option<ForgePatch> = None;
        let mut hear: Option<Arc<Rendered>> = None;
        let mut save = false;
        let mut export_now = false;
        let mut load_sample: Option<usize> = None;
        view.layer = view.layer.min(p.layers.len().saturating_sub(1));

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                // Start, hear, style.
                ui.horizontal_wrapped(|ui| {
                    play = kit::button_sized(ui, vec2(92.0, kit::CONTROL_H), kit::Kind::Primary, true, "Play", |p, face, ink| {
                        kit::icon_play(p, face.center() - vec2(22.0, 0.0), 11.0, ink);
                        p.text(face.center() + vec2(8.0, 0.0), Align2::CENTER_CENTER, "PLAY", t::mono_medium(13.0), ink);
                    })
                    .on_hover_text("Hear the sound. A note on this channel plays it too (C5 as made)")
                        .clicked();
                    let user = cached_user_sounds(ui.ctx());
                    egui::ComboBox::from_id_salt(("forge-start", id))
                        .width(170.0)
                        .selected_text(format!("Start from: {}", p.name))
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            for r in Recipe::ALL {
                                if ui.selectable_label(false, r.label()).clicked() {
                                    let mut np = r.patch();
                                    if let Some(s) = game_style {
                                        s.apply(&mut np);
                                    }
                                    adopt = Some(np);
                                }
                            }
                            if !user.is_empty() {
                                ui.separator();
                                ui.label(egui::RichText::new("My sounds").font(t::sans(11.5)).color(t::FAINT));
                                for u in user.iter() {
                                    if ui.selectable_label(false, &u.name).clicked() {
                                        adopt = Some(u.clone());
                                    }
                                }
                            }
                        });
                    save = ui
                        .button("Save to My sounds")
                        .on_hover_text("Keep this sound: it joins Start from, in Presets/Forge")
                        .clicked();
                });
                ui.horizontal_wrapped(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(84.0, 20.0), Sense::hover());
                    kit::caps(ui.painter(), r.left_center(), Align2::LEFT_CENTER, "GAME STYLE", 10.0, t::MUTED);
                    let names = ["Any", "Retro", "Cartoon", "Heavy", "Sci-fi"];
                    let at = game_style.map_or(0, |s| Style::ALL.iter().position(|x| *x == s).unwrap_or(0) + 1);
                    let (pick, r) = kit::chips(ui, &format!("forge-game-style-{id}"), &names, at, None);
                    ui.interact(r, ui.id().with("game-style-hint"), Sense::hover())
                        .on_hover_text("The whole project: every recipe in Start from and every new Forge channel begins in this style");
                    if let Some(i) = pick
                        && i != at
                    {
                        style_pick = Some(if i == 0 { None } else { Some(Style::ALL[i - 1]) });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(84.0, 20.0), Sense::hover());
                    kit::caps(ui.painter(), r.left_center(), Align2::LEFT_CENTER, "MAKE IT", 10.0, t::MUTED);
                    for s in Style::ALL {
                        let hint = match s {
                            Style::Retro => "Chip waves, chip noise, fewer bits",
                            Style::Cartoon => "Round waves, bouncier sweeps, no grit",
                            Style::Heavy => "Lower, bigger, driven, with a sub layer",
                            Style::SciFi => "Wobble, phaser, ring modulation, room",
                        };
                        if ui.button(s.label()).on_hover_text(hint).clicked() {
                            s.apply(&mut p);
                            ed.started = true;
                            ed.changed = true;
                        }
                    }
                });

                panel(ui, "sound", t::ENC_WHITE, |ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::click());
                    let painter = ui.painter();
                    let well = kit::sunken(painter, r, 6.0).shrink(6.0);
                    wave(painter, well, &sound.rendered.frames, t::ENC_WHITE);
                    let db = 20.0 * sound.rendered.peak.max(1e-6).log10();
                    kit::caps(
                        painter,
                        well.right_top() + vec2(-2.0, 2.0),
                        Align2::RIGHT_TOP,
                        &format!("{} · PEAK {db:.1} DB", secs(sound.rendered.seconds())),
                        10.0,
                        t::FAINT,
                    );
                    if sound.stale() {
                        kit::caps(painter, well.left_top() + vec2(2.0, 2.0), Align2::LEFT_TOP, "RENDERING", 10.0, t::WAIT);
                    }
                    if resp.on_hover_text("Click to hear it").clicked() {
                        play = true;
                    }
                });

                panel(ui, "character", t::ENC_WHITE, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let c = t::ENC_WHITE;
                        let hints = [
                            "Small and high .. huge, low and long",
                            "Lazy, soft, shallow sweeps .. wild, loud, deep sweeps",
                            "Clean .. driven, then crushed to fewer bits",
                            "Dry .. a cave (reverb)",
                        ];
                        for i in 0..4 {
                            let mut v = p.macros.get(i);
                            let def = forge::Macros::default().get(i) as f64;
                            knob(ui, &mut ed, &forge::Macros::NAMES[i].to_uppercase(), c, &mut v, (0.0, 1.0, def), false, &pct, hints[i]);
                            p.macros.set(i, v);
                        }
                        knob(ui, &mut ed, "PITCH", c, &mut p.transpose, (-24.0, 24.0, 0.0), false, &|v| format!("{v:+.1} st"), "The whole sound up or down, in semitones");
                        knob(ui, &mut ed, "OUTPUT", c, &mut p.gain_db, (-36.0, 12.0, 0.0), false, &|v| format!("{v:+.1} dB"), "Level before the safety limiter");
                    });
                });

                panel(ui, "ideas", t::ENC_GREEN, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let label = if growing { "Growing" } else { "Mutate" };
                        mutate_now = kit::button(ui, label, kit::Kind::Normal, !growing)
                            .on_hover_text("Eight relatives of this sound: different waves, sweeps and knobs")
                            .clicked();
                        if !view.ideas.is_empty() && ui.button("Clear").clicked() {
                            view.ideas.clear();
                            view.heard = None;
                        }
                    });
                    if view.ideas.is_empty() {
                        ui.label(egui::RichText::new("Mutate grows eight relatives of this sound. Click one to hear it, double-click to keep it.").color(t::MUTED));
                        return;
                    }
                    let cols = if ui.available_width() >= 560.0 { 4 } else { 2 };
                    let w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
                    for row in 0..view.ideas.len().div_ceil(cols) {
                        ui.horizontal(|ui| {
                            for c in 0..cols {
                                let i = row * cols + c;
                                let Some(idea) = view.ideas.get(i) else { break };
                                let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::click());
                                let painter = ui.painter();
                                let lit = view.heard == Some(i);
                                let well = kit::sunken(painter, r, 6.0).shrink(4.0);
                                if lit || resp.hovered() {
                                    painter.rect_stroke(r, 6.0, Stroke::new(1.0_f32, if lit { t::ENC_GREEN } else { t::STRONG }), egui::StrokeKind::Inside);
                                }
                                wave(painter, well, &idea.sound.frames, if lit { t::ENC_GREEN } else { t::MUTED });
                                kit::caps(painter, well.left_top() + vec2(2.0, 1.0), Align2::LEFT_TOP, &format!("{:02}", i + 1), 10.0, t::FAINT);
                                let resp = resp.on_hover_text("Click to hear · double-click to use it");
                                if resp.double_clicked() {
                                    adopt = Some(idea.patch.clone());
                                } else if resp.clicked() {
                                    hear = Some(idea.sound.clone());
                                    view.heard = Some(i);
                                }
                            }
                        });
                    }
                    if let Some(i) = view.heard
                        && let Some(idea) = view.ideas.get(i)
                        && kit::button(ui, &format!("Use {:02}", i + 1), kit::Kind::Normal, true)
                            .on_hover_text("Replace the sound with this one (Ctrl+Z undoes)")
                            .clicked()
                    {
                        adopt = Some(idea.patch.clone());
                    }
                });

                let jobs = Bouncing { now: bouncing, channels: &channels };
                layers_panel(ui, &mut ed, id, &mut p, &mut view, &mut load_sample, &jobs, &mut bounce);
                if let Some(l) = p.layers.get_mut(view.layer) {
                    let color = t::ENC_BLUE;
                    panel(ui, &format!("layer {:02} · {}", view.layer + 1, l.name), color, |ui| {
                        layer_editor(ui, &mut ed, id, l, &mut view, &mut load_sample, &jobs, &mut bounce);
                    });
                }
                panel(ui, "master effects", t::ENC_WHITE, |ui| {
                    fx_list(ui, &mut ed, &format!("forge-master-{id}"), &mut p.fx);
                });
                panel(ui, "export for game", t::ENC_WHITE, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        // View settings, not the sound: no undo step.
                        let mut scratch = Edit::default();
                        scratch.knob(ui, "FILES", t::ENC_WHITE, &mut view.count, (1.0, 20.0, 5.0), false, &|v| format!("{:.0}", v.round()), "The sound, then variations of it: a little different each, so repeats never sound robotic");
                        view.count = view.count.round().clamp(1.0, 20.0);
                        ui.vertical(|ui| {
                            if let (Some(i), _) = kit::chips(ui, &format!("forge-fmt-{id}"), &["WAV", "OGG"], view.ogg as usize, None) {
                                view.ogg = i == 1;
                            }
                            if let (Some(i), _) = kit::chips(ui, &format!("forge-rate-{id}"), &["48 kHz", "44.1 kHz"], view.rate_44 as usize, None) {
                                view.rate_44 = i == 1;
                            }
                        });
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                super::settings_ui::kit_switch(ui, &mut view.normalize);
                                ui.label("Normalize").on_hover_text("Loudest peak at −1 dB");
                            });
                            export_now = kit::button(ui, "Export…", kit::Kind::Normal, true)
                                .on_hover_text("Choose a folder: files are named like jump_01.wav, jump_02.wav")
                                .clicked();
                        });
                    });
                });
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
        }
        if let Some(style) = style_pick {
            self.checkpoint();
            self.song.game_style = style;
            self.publish();
            let what = style.map_or("any style".to_string(), |s| format!("the {} style", s.label()));
            self.say(Tone::Quiet, format!("Recipes and new Forge channels now start in {what}. MAKE IT turns this sound too."));
        }
        if mutate_now {
            view.seed = view.seed.wrapping_add(1);
            let (tx, rx) = mpsc::channel();
            let (base, seed) = (p.clone(), view.seed.wrapping_mul(7919));
            std::thread::spawn(move || {
                let ideas = mutate::variations(&base, IDEAS, seed, mutate::MUTATION)
                    .into_iter()
                    .map(|patch| {
                        let sound = Arc::new(forge::render::render(&patch));
                        Idea { patch, sound }
                    })
                    .collect();
                let _ = tx.send(ideas);
            });
            self.forge_jobs.ideas = Some((id, rx));
        }
        if let Some((into, b)) = bounce {
            self.forge_bounce(id, into, b);
        }
        if let Some(i) = load_sample {
            self.forge_load_sample(id, i);
        }
        if play {
            self.play_forge(id);
        }
        if let Some(r) = hear {
            self.audition(&r);
        }
        if save {
            self.forge_save(&ctx, id);
        }
        if export_now {
            self.forge_export(id, &view);
        }
        ctx.data_mut(|d| d.insert_temp(view_key, view));
    }

    /// Render one note of a project channel (plugin, sampler, synth) on a
    /// worker thread, the way a song export does, for Forge channel `forge`.
    fn forge_bounce(&mut self, forge: u64, into: Into, b: forge::Bounce) {
        use keyframexl::sequence::{Note, PPQ, Pattern};
        if self.forge_jobs.bounce.is_some() {
            return;
        }
        let mut song = self.song.clone();
        let pid = song.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut pat = Pattern::new(pid, "Forge bounce");
        let ticks = ((b.hold as f64 * song.tempo / 60.0) * PPQ as f64).round().max(1.0) as i64;
        pat.notes.push(Note::new(b.channel, 0, ticks, b.key, 100));
        song.patterns.push(pat);
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
        // The live output leaves the plugins to the render, as in an export.
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
            let _ = tx.send(out.map(|f| (rate, f)).ok_or_else(|| "the render stopped".to_string()));
        });
        self.forge_jobs.bounce = Some((forge, into, b, rx));
    }

    /// A finished bounce becomes a layer (one undo step).
    fn poll_bounce(&mut self) {
        let Some((forge_id, into, b, rx)) = &self.forge_jobs.bounce else { return };
        let (forge_id, into, b) = (*forge_id, *into, *b);
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("the render stopped".into()),
        };
        self.forge_jobs.bounce = None;
        let name = format!("{} {}", self.instrument_name(b.channel), super::roll::note_name(b.key));
        let (rate, mut frames) = match result {
            Ok(v) => v,
            Err(e) => {
                self.say(Tone::Bad, format!("Could not bounce {name}: {e}."));
                return;
            }
        };
        // From the first sound, at most as long as a layer may be.
        let first = frames.iter().position(|f| f[0].abs().max(f[1].abs()) > 1e-4);
        let Some(first) = first else {
            self.say(Tone::Bad, format!("{name} played silence. Load a sound into that channel, or pick another note."));
            return;
        };
        frames.drain(..first);
        frames.truncate((forge::MAX_SECONDS * rate as f32) as usize);
        let Some(mut p) = self.forge_patch(forge_id) else { return };
        let length = (frames.len() as f32 / rate.max(1) as f32).clamp(0.01, forge::MAX_SECONDS);
        let source = Source::Sample {
            name: name.clone(),
            rate,
            frames: Arc::new(frames),
            reverse: false,
            from: Some(b),
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
                self.say(Tone::Bad, "Six layers is the most: delete one, then bounce again.");
                return;
            }
        }
        self.checkpoint();
        self.commit_forge(forge_id, p);
        self.say(Tone::Quiet, format!("{name} bounced into the sound. Ctrl+Z undoes."));
    }

    /// Live check: `KFXL_CAPTURE_FORGE_BOUNCE=out.wav` bounces C5 of the last
    /// plugin channel into the first Forge channel once that plugin has loaded,
    /// then writes the new layer's audio to `out.wav`.
    pub(super) fn capture_forge_bounce(&mut self) {
        use std::sync::atomic::{AtomicU8, Ordering};
        // 0 waiting for the plugin, 1 bouncing, 2 written.
        static STATE: AtomicU8 = AtomicU8::new(0);
        let Ok(out) = std::env::var("KFXL_CAPTURE_FORGE_BOUNCE") else { return };
        started();
        let forge_id = self.song.instruments.iter().find(|i| matches!(i.kind, InstrumentKind::Forge { .. })).map(|i| i.id);
        let plugin = self.song.instruments.iter().rev().find_map(|i| i.plugin().map(|p| (i.id, p.id)));
        let (Some(forge_id), Some((channel, pid))) = (forge_id, plugin) else { return };
        match STATE.load(Ordering::Relaxed) {
            // A plugin under Wine reads its samples for a while after it loads
            // (KFXL_CAPTURE_FORGE_BOUNCE_AFTER seconds from start, default 25).
            0 if self.plugins.live.get(&pid).is_some_and(|l| l.proc.is_some())
                && started().elapsed().as_secs_f64()
                    >= std::env::var("KFXL_CAPTURE_FORGE_BOUNCE_AFTER").ok().and_then(|v| v.parse().ok()).unwrap_or(25.0) =>
            {
                STATE.store(1, Ordering::Relaxed);
                self.forge_bounce(forge_id, Into::NewLayer, forge::Bounce { channel, key: 60, hold: 0.6 });
            }
            1 if self.forge_jobs.bounce.is_none() => {
                STATE.store(2, Ordering::Relaxed);
                let p = self.forge_patch(forge_id);
                let frames = p.as_ref().and_then(|p| p.layers.iter().rev().find_map(|l| match &l.source {
                    Source::Sample { frames, rate, from: Some(_), .. } => Some((*rate, frames.clone())),
                    _ => None,
                }));
                match frames {
                    Some((rate, f)) => {
                        let r = keyframexl::render::write_wav_at(std::path::Path::new(&out), &f, keyframexl::render::Format::Wav32Float, false, rate);
                        eprintln!("forge bounce: {} frames at {rate} Hz -> {out} {r:?}", f.len());
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
        let Some(p) = self.forge_patch(id) else { return };
        match forge::save_user(&p) {
            Ok(path) => {
                ctx.data_mut(|d| d.remove::<Arc<Vec<ForgePatch>>>(egui::Id::new("forge-user")));
                self.say(Tone::Quiet, format!("Saved {} to {}. It is in Start from now.", p.name, path.display()));
            }
            Err(e) => self.say(Tone::Bad, format!("Could not save the sound: {e}")),
        }
    }

    fn forge_export(&mut self, id: u64, view: &View) {
        let Some(p) = self.forge_patch(id) else { return };
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Export sounds into this folder")
            .pick_folder()
        else {
            return;
        };
        let opt = export::Options {
            count: view.count as usize,
            format: if view.ogg { export::FileFormat::Ogg } else { export::FileFormat::Wav },
            rate: if view.rate_44 { 44_100 } else { 48_000 },
            normalize: view.normalize,
            stem: p.name.clone(),
            seed: view.seed.wrapping_add(id),
        };
        match export::export(&p, &dir, &opt) {
            Ok(paths) => {
                let first = paths.first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.say(Tone::Quiet, format!("Exported {} files ({first} …) to {}.", paths.len(), dir.display()));
            }
            Err(e) => self.say(Tone::Bad, format!("Export stopped: {e}. Files written before it are kept.")),
        }
    }

    /// Pick an audio file for layer `layer` of channel `id`.
    fn forge_load_sample(&mut self, id: u64, layer: usize) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "flac", "ogg"])
            .set_title("Sample for this layer")
            .pick_file()
        else {
            return;
        };
        let (rate, mut frames) = match crate::import::read_audio(&path) {
            Ok(v) => v,
            Err(e) => {
                self.say(Tone::Bad, format!("Could not read the sample: {e}"));
                return;
            }
        };
        frames.truncate((forge::MAX_SECONDS * rate as f32) as usize);
        let Some(mut p) = self.forge_patch(id) else { return };
        let Some(l) = p.layers.get_mut(layer) else { return };
        let name = super::instrument::file_name(&path.to_string_lossy());
        l.length = (frames.len() as f32 / rate.max(1) as f32).clamp(0.01, forge::MAX_SECONDS - l.start);
        l.source = Source::Sample {
            name: name.clone(),
            rate,
            frames: Arc::new(frames),
            reverse: false,
            from: None,
        };
        l.name = name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string());
        l.pitch = Curve::flat(0.0);
        self.checkpoint();
        self.commit_forge(id, p);
        self.say(Tone::Quiet, format!("{name} loaded into the layer. Ctrl+Z undoes."));
    }
}

/// When the program started (first asked).
fn started() -> std::time::Instant {
    static AT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *AT.get_or_init(std::time::Instant::now)
}

/// "My sounds", read once and kept until one is saved.
fn cached_user_sounds(ctx: &egui::Context) -> Arc<Vec<ForgePatch>> {
    let key = egui::Id::new("forge-user");
    if let Some(v) = ctx.data(|d| d.get_temp::<Arc<Vec<ForgePatch>>>(key)) {
        return v;
    }
    let v = Arc::new(forge::user_sounds());
    ctx.data_mut(|d| d.insert_temp(key, v.clone()));
    v
}

/// What the layer panels need to know about bouncing: the job running now
/// and the channels a note can come from.
struct Bouncing<'a> {
    now: Option<(Into, forge::Bounce)>,
    channels: &'a [(u64, String)],
}

impl Bouncing<'_> {
    fn name(&self, channel: u64) -> &str {
        self.channels
            .iter()
            .find(|c| c.0 == channel)
            .map_or("a deleted channel", |c| c.1.as_str())
    }
}

#[allow(clippy::too_many_arguments)]
fn layers_panel(
    ui: &mut Ui,
    ed: &mut Edit,
    id: u64,
    p: &mut ForgePatch,
    view: &mut View,
    load_sample: &mut Option<usize>,
    jobs: &Bouncing,
    bounce: &mut Option<(Into, forge::Bounce)>,
) {
    panel(ui, "layers", t::ENC_BLUE, |ui| {
        let solo_any = p.layers.iter().any(|l| l.solo);
        let mut remove = None;
        for (i, l) in p.layers.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.push_id(("forge-layer", id, i), |ui| {
                    if super::settings_ui::kit_switch(ui, &mut l.enabled) {
                        ed.started = true;
                        ed.changed = true;
                    }
                    let solo = egui::Button::new(egui::RichText::new("S").font(t::mono(12.0)).color(if l.solo { t::WAIT } else { t::MUTED }))
                        .min_size(vec2(24.0, 22.0));
                    if ui.add(solo).on_hover_text("Solo: hear only soloed layers").clicked() {
                        l.solo = !l.solo;
                        ed.started = true;
                        ed.changed = true;
                    }
                    let audible = if solo_any { l.solo } else { l.enabled };
                    let text = format!("{:02}  {}", i + 1, l.name);
                    let label = egui::RichText::new(text).font(t::sans(14.0)).color(if audible { t::TEXT } else { t::MUTED });
                    if ui.selectable_label(view.layer == i, label).on_hover_text("Edit this layer below").clicked() {
                        view.layer = i;
                    }
                    let what = match &l.source {
                        Source::Osc { wave, .. } => wave.label().to_string(),
                        Source::Noise { color, .. } => format!("{} noise", color.label()),
                        Source::Sample { name, .. } => name.clone(),
                        Source::Synth { .. } => "Tide synth".into(),
                    };
                    ui.label(egui::RichText::new(what).font(t::sans(12.5)).color(t::FAINT));
                    if ui.small_button("×").on_hover_text("Delete this layer").clicked() {
                        remove = Some(i);
                    }
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
            let what = match into {
                Into::NewLayer => "a new layer".to_string(),
                Into::Layer(i) => format!("layer {:02}", i + 1),
            };
            ui.label(egui::RichText::new(format!("Bouncing {} · {} into {what}…", jobs.name(b.channel), super::roll::note_name(b.key))).color(t::WAIT));
        }
        if p.layers.is_empty() {
            ui.label(egui::RichText::new("No layers: the sound is silent. Add one, or pick a recipe in Start from.").color(t::MUTED));
        }
        ui.horizontal(|ui| {
            let full = p.layers.len() >= MAX_LAYERS;
            ui.add_enabled_ui(!full, |ui| {
                ui.menu_button("+ Add layer", |ui| {
                    kit::menu_style(ui);
                    let new = if ui.button("Oscillator").clicked() {
                        Some(Layer::new("Tone", Source::osc(Wave::Square, 440.0), 0.3))
                    } else if ui.button("Noise").clicked() {
                        Some(Layer::new("Noise", Source::noise(NoiseColor::White, 4000.0), 0.3))
                    } else if ui.button("Sample…").clicked() {
                        let mut l = Layer::new("Sample", Source::osc(Wave::Sine, 440.0), 0.3);
                        l.amp = Curve::flat(1.0);
                        *load_sample = Some(p.layers.len());
                        Some(l)
                    } else if !jobs.channels.is_empty()
                        && let Some(ch) = ui
                            .menu_button("Note from a channel", |ui| {
                                kit::menu_style(ui);
                                let mut pick = None;
                                for (cid, name) in jobs.channels {
                                    if ui.button(name).on_hover_text("One C5 of this channel through its mixer insert: a VST plugin, the sampler, a synth").clicked() {
                                        pick = Some(*cid);
                                    }
                                }
                                pick
                            })
                            .inner
                            .flatten()
                    {
                        *bounce = Some((Into::NewLayer, forge::Bounce { channel: ch, key: 60, hold: 0.5 }));
                        ui.close_menu();
                        None
                    } else if ui.button("Synth note (KeyFrame Tide)").clicked() {
                        Some(Layer::new("Synth", Source::Synth { patch: Box::new(keyframexl::synth::SynthPatch::factory(2)), key: 60 }, 0.5))
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
                .on_hover_text(if full { "Six layers is the most" } else { "Add a source to the sound" });
                if !p.layers.is_empty() && ui.button("Duplicate").on_hover_text("Copy the selected layer").clicked() {
                    let mut l = p.layers[view.layer].clone();
                    l.name = format!("{} copy", l.name);
                    p.layers.insert(view.layer + 1, l);
                    view.layer += 1;
                    ed.started = true;
                    ed.changed = true;
                }
            });
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn layer_editor(
    ui: &mut Ui,
    ed: &mut Edit,
    id: u64,
    l: &mut Layer,
    view: &mut View,
    load_sample: &mut Option<usize>,
    jobs: &Bouncing,
    bounce: &mut Option<(Into, forge::Bounce)>,
) {
    let c = t::ENC_BLUE;
    let salt = format!("forge-{id}-{}", view.layer);
    let mut name = l.name.clone();
    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0).hint_text("Layer name"));
    if r.changed() {
        if kit::first_edit(ui, &r) {
            ed.started = true;
        }
        l.name = name;
        ed.changed = true;
    }
    kit::end_edit(ui, &r);

    // Source: what the layer plays.
    let kinds = ["Oscillator", "Noise", "Sample", "Synth"];
    let at = source_kind(&l.source);
    if let (Some(k), _) = kit::chips(ui, &format!("{salt}-src"), &kinds, at, None)
        && k != at
    {
        match k {
            0 => l.source = Source::osc(Wave::Square, 440.0),
            1 => l.source = Source::noise(NoiseColor::White, 4000.0),
            2 => *load_sample = Some(view.layer),
            _ => {
                l.source = Source::Synth {
                    patch: Box::new(keyframexl::synth::SynthPatch::factory(2)),
                    key: 60,
                }
            }
        }
        if k != 2 {
            ed.started = true;
            ed.changed = true;
        }
    }
    match &mut l.source {
        Source::Osc { wave, hz: f, vibrato_hz, vibrato } => {
            chips(ui, ed, &format!("{salt}-wave"), &Wave::ALL, &Wave::ALL.map(Wave::label), wave);
            ui.horizontal_wrapped(|ui| {
                knob(ui, ed, "PITCH", c, f, (20.0, 8000.0, 440.0), true, &hz, "Frequency where the pitch line is at 0");
                knob(ui, ed, "VIBRATO", c, vibrato, (0.0, 4.0, 0.0), false, &|v| format!("{v:.2} st"), "How far the pitch wobbles");
                knob(ui, ed, "RATE", c, vibrato_hz, (0.0, 30.0, 6.0), false, &|v| format!("{v:.1} Hz"), "How fast it wobbles");
            });
        }
        Source::Noise { color, hz: f } => {
            chips(ui, ed, &format!("{salt}-noise"), &NoiseColor::ALL, &NoiseColor::ALL.map(NoiseColor::label), color);
            if matches!(color, NoiseColor::Digital | NoiseColor::Crackle) {
                let hint = if *color == NoiseColor::Digital { "Clock of the chip noise: low is gritty, high is hiss" } else { "Pops per second" };
                ui.horizontal_wrapped(|ui| {
                    knob(ui, ed, "RATE", c, f, (10.0, 48_000.0, 4000.0), true, &hz, hint);
                });
            }
        }
        Source::Sample { name, reverse, from, .. } => {
            if let Some(b) = from {
                // Bounced from a channel: which one, the note, how long it is held.
                let mut nb = *b;
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt(format!("{salt}-from"))
                        .width(170.0)
                        .selected_text(jobs.name(nb.channel))
                        .show_ui(ui, |ui| {
                            kit::menu_style(ui);
                            for (cid, n) in jobs.channels {
                                ui.selectable_value(&mut nb.channel, *cid, n);
                            }
                        });
                    let mut k = nb.key as f64;
                    let mut scratch = Edit::default();
                    scratch.knob(ui, "NOTE", c, &mut k, (24.0, 96.0, 60.0), false, &|v| super::roll::note_name(v.round() as u8), "The note the channel plays");
                    nb.key = k.round().clamp(0.0, 127.0) as u8;
                    let mut h = nb.hold as f64;
                    scratch.knob(ui, "HOLD", c, &mut h, (0.02, 4.0, 0.5), true, &|v| secs(v), "How long the key is held; the release rings on");
                    nb.hold = h as f32;
                    let busy = jobs.now.is_some();
                    if kit::button(ui, "Bounce", kit::Kind::Normal, !busy && jobs.channels.iter().any(|c| c.0 == nb.channel))
                        .on_hover_text("Render the note again with these settings (and the channel's current sound)")
                        .clicked()
                    {
                        *bounce = Some((Into::Layer(view.layer), nb));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name.as_str()).font(t::sans(14.0)));
                if ui.button("Replace…").clicked() {
                    *load_sample = Some(view.layer);
                }
                super::settings_ui::kit_switch(ui, reverse).then(|| {
                    ed.started = true;
                    ed.changed = true;
                });
                ui.label("Reverse");
            });
        }
        Source::Synth { patch, key } => {
            ui.horizontal(|ui| {
                let names = keyframexl::synth::SynthPatch::PRESETS;
                let now = (0..names.len()).find(|&i| **patch == keyframexl::synth::SynthPatch::factory(i));
                egui::ComboBox::from_id_salt(format!("{salt}-tide"))
                    .width(160.0)
                    .selected_text(now.map_or("Edited patch", |i| names[i]))
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
                ed.knob(ui, "NOTE", c, &mut k, (24.0, 96.0, 60.0), false, &|v| super::roll::note_name(v.round() as u8), "The synth note this layer plays");
                *key = k.round().clamp(0.0, 127.0) as u8;
            });
        }
    }

    // Timing and level.
    ui.horizontal_wrapped(|ui| {
        knob(ui, ed, "START", c, &mut l.start, (0.0, 2.0, 0.0), false, &|v| secs(v), "When the layer comes in");
        knob(ui, ed, "LENGTH", c, &mut l.length, (0.01, forge::MAX_SECONDS as f64, 0.3), true, &|v| secs(v), "How long it lasts; its lines stretch with it");
        knob(ui, ed, "GAIN", c, &mut l.gain, (0.0, 2.0, 1.0), false, &pct, "Layer level");
        knob(ui, ed, "PAN", c, &mut l.pan, (-1.0, 1.0, 0.0), false, &signed, "Left .. right");
    });

    // The lines.
    let lines = [Line::Pitch, Line::Volume, Line::Filter];
    let at = lines.iter().position(|v| *v == view.line).unwrap_or(0);
    if let (Some(i), _) = kit::chips(ui, &format!("{salt}-line"), &["Pitch", "Volume", "Filter"], at, None) {
        view.line = lines[i];
    }
    let length = l.length as f64;
    match view.line {
        Line::Pitch => curve_editor(ui, ed, &format!("{salt}-pitch"), &mut l.pitch, Line::Pitch, t::ENC_BLUE, length),
        Line::Volume => curve_editor(ui, ed, &format!("{salt}-amp"), &mut l.amp, Line::Volume, t::ENC_GREEN, length),
        Line::Filter => {
            chips(ui, ed, &format!("{salt}-fmode"), &FilterMode::ALL, &FilterMode::ALL.map(FilterMode::label), &mut l.filter);
            if l.filter != FilterMode::Off {
                curve_editor(ui, ed, &format!("{salt}-cut"), &mut l.cutoff, Line::Filter, t::ENC_ORANGE, length);
                ui.horizontal_wrapped(|ui| {
                    knob(ui, ed, "RESO", t::ENC_ORANGE, &mut l.resonance, (0.0, 1.0, 0.0), false, &pct, "Ring at the cutoff: a whistle on sweeps");
                });
            }
        }
    }

    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    kit::caps(ui.painter(), r.left_center(), Align2::LEFT_CENTER, "LAYER EFFECTS", 10.0, t::MUTED);
    fx_list(ui, ed, &format!("{salt}-fx"), &mut l.fx);
}

/// A drawn line: drag points, double-click to add or remove one, drag the
/// line between two points to bend it. Pitch snaps to semitones (Alt: free).
fn curve_editor(ui: &mut Ui, ed: &mut Edit, salt: &str, c: &mut Curve, kind: Line, color: Color32, length: f64) {
    let (lo, hi) = match kind {
        Line::Pitch => {
            let most = c.points.iter().map(|p| p.v.abs()).fold(0.0, f32::max);
            let span = ((most / 12.0).ceil() * 12.0).clamp(12.0, PITCH_RANGE);
            (-span, span)
        }
        _ => (0.0, 1.0),
    };
    let (area, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let r = kit::sunken(&painter, area, 6.0).shrink(10.0);
    let x = |t: f32| r.left() + t * r.width();
    let y = |v: f32| r.bottom() - (v - lo) / (hi - lo) * r.height();
    let to_t = |px: f32| ((px - r.left()) / r.width()).clamp(0.0, 1.0);
    let to_v = |py: f32| (lo + (r.bottom() - py) / r.height() * (hi - lo)).clamp(lo, hi);
    // Guides: octaves for pitch, decades for the filter, halves for volume.
    let guide = Stroke::new(1.0_f32, t::SURFACE);
    match kind {
        Line::Pitch => {
            let mut v = lo;
            while v <= hi + 0.1 {
                let s = if v == 0.0 { Stroke::new(1.0_f32, t::LINE) } else { guide };
                painter.line_segment([pos2(r.left(), y(v)), pos2(r.right(), y(v))], s);
                kit::caps(&painter, pos2(r.left() + 2.0, y(v) - 1.0), Align2::LEFT_BOTTOM, &format!("{v:+.0}"), 9.0, t::FAINT);
                v += 12.0;
            }
        }
        Line::Filter => {
            for (u, label) in [(1.0f32 / 3.0, "200 HZ"), (2.0 / 3.0, "2 KHZ")] {
                painter.line_segment([pos2(r.left(), y(u)), pos2(r.right(), y(u))], guide);
                kit::caps(&painter, pos2(r.left() + 2.0, y(u) - 1.0), Align2::LEFT_BOTTOM, label, 9.0, t::FAINT);
            }
        }
        Line::Volume => {
            painter.line_segment([pos2(r.left(), y(0.5)), pos2(r.right(), y(0.5))], guide);
        }
    }
    let pts: Vec<_> = (0..=240)
        .map(|i| {
            let tt = i as f32 / 240.0;
            pos2(x(tt), y(c.at(tt).clamp(lo, hi)))
        })
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, color)));

    let near = |c: &Curve, pos: egui::Pos2| {
        c.points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, pos2(x(p.t), y(p.v.clamp(lo, hi))).distance(pos)))
            .filter(|(_, d)| *d <= 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    #[derive(Clone, Copy)]
    enum Grab {
        Point(usize),
        Bend(usize),
    }
    let grab_id = ui.id().with((salt, "grab"));
    let hover = resp.hover_pos();
    let over = hover.and_then(|h| near(c, h));
    // Where the button went down, not where the drag was recognised.
    if resp.drag_started()
        && let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
    {
        let g = match near(c, pos) {
            Some(i) => Some(Grab::Point(i)),
            None => {
                let tt = to_t(pos.x);
                let i = c.points.partition_point(|p| p.t <= tt);
                (i > 0 && i < c.points.len()).then_some(Grab::Bend(i - 1))
            }
        };
        ui.data_mut(|d| d.insert_temp(grab_id, g));
        ed.started |= g.is_some();
    }
    let grab: Option<Grab> = ui.data(|d| d.get_temp(grab_id)).flatten();
    if resp.dragged()
        && let Some(g) = grab
        && let Some(pos) = resp.interact_pointer_pos()
    {
        match g {
            Grab::Point(i) if i < c.points.len() => {
                let min = if i > 0 { c.points[i - 1].t } else { 0.0 };
                let max = c.points.get(i + 1).map_or(1.0, |p| p.t);
                let mut v = to_v(pos.y);
                if kind == Line::Pitch && !ui.input(|i| i.modifiers.alt) {
                    v = v.round();
                }
                c.points[i].t = to_t(pos.x).clamp(min, max);
                c.points[i].v = v;
            }
            Grab::Bend(i) if i + 1 < c.points.len() => {
                let dy = resp.drag_delta().y / 60.0;
                let rising = c.points[i + 1].v >= c.points[i].v;
                let p = &mut c.points[i];
                p.tension = (p.tension + if rising { dy } else { -dy }).clamp(-1.0, 1.0);
            }
            _ => {}
        }
        ed.changed = true;
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<Grab>>(grab_id));
    }
    if resp.double_clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        match near(c, pos) {
            Some(i) if c.points.len() > 2 => {
                c.points.remove(i);
            }
            Some(_) => {}
            None => {
                let mut v = to_v(pos.y);
                if kind == Line::Pitch {
                    v = v.round();
                }
                let tt = to_t(pos.x);
                let at = c.points.partition_point(|p| p.t <= tt);
                c.points.insert(at, CurvePoint { t: tt, v, tension: 0.0 });
            }
        }
        ed.started = true;
        ed.changed = true;
    }
    let value = |v: f32| match kind {
        Line::Pitch => format!("{v:+.1} st"),
        Line::Volume => pct(v as f64),
        Line::Filter => hz(cutoff_hz(v)),
    };
    for (i, p) in c.points.iter().enumerate() {
        let at = pos2(x(p.t), y(p.v.clamp(lo, hi)));
        let lit = over == Some(i) || matches!(grab, Some(Grab::Point(g)) if g == i);
        painter.circle_filled(at, if lit { 5.5 } else { 4.0 }, color);
        painter.circle_stroke(at, if lit { 5.5 } else { 4.0 }, Stroke::new(1.0_f32, t::EDGE));
        if lit {
            kit::caps(&painter, at + vec2(8.0, -8.0), Align2::LEFT_BOTTOM, &format!("{} · {}", secs(p.t as f64 * length), value(p.v)), 10.0, t::TEXT);
        }
    }
    resp.on_hover_text(match kind {
        Line::Pitch => "Drag points · double-click adds or removes one · drag the line to bend it · Alt: off the semitones",
        _ => "Drag points · double-click adds or removes one · drag the line to bend it",
    });
}

/// A chain of effects: switch, name, every knob, remove; an add menu below.
fn fx_list(ui: &mut Ui, ed: &mut Edit, salt: &str, fx: &mut Vec<FxSlot>) {
    let mut remove = None;
    for (i, slot) in fx.iter_mut().enumerate() {
        ui.push_id((salt, i), |ui| {
            ui.horizontal_wrapped(|ui| {
                if super::settings_ui::kit_switch(ui, &mut slot.enabled) {
                    ed.started = true;
                    ed.changed = true;
                }
                let (r, _) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::hover());
                kit::caps(ui.painter(), r.left_center(), Align2::LEFT_CENTER, slot.kind.label(), 10.0, if slot.enabled { t::TEXT } else { t::MUTED });
                let specs = slot.kind.params();
                for (n, spec) in specs.iter().enumerate() {
                    let mut v = slot.params[n] as f64;
                    let fmt = |x: f64| keyframexl::fx::format(spec, x as f32);
                    ui.add_enabled_ui(slot.enabled, |ui| {
                        ed.knob(ui, &spec.name.to_uppercase(), t::ENC_WHITE, &mut v, (spec.min as f64, spec.max as f64, spec.default as f64), spec.log, &fmt, spec.name);
                    });
                    slot.params[n] = v as f32;
                }
                if ui.small_button("×").on_hover_text("Remove this effect").clicked() {
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
        .selected_text(egui::RichText::new("+ Add effect").font(t::sans(12.5)).color(t::FAINT))
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};

    /// Run one frame of the curve editor; returns the drawing rect it used.
    fn frame(ctx: &egui::Context, c: &mut Curve, kind: Line, ed: &mut Edit, events: Vec<Event>) -> Rect {
        // Each frame 50 ms later, so clicks far apart in the test are not one double-click.
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
                curve_editor(ui, ed, "t", c, kind, t::ENC_BLUE, 1.0);
            });
        });
        area
    }

    fn press(pos: Pos2, down: bool) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed: down, modifiers: Modifiers::default() }
    }

    fn drag(ctx: &egui::Context, c: &mut Curve, kind: Line, a: Pos2, b: Pos2) -> Edit {
        let mut ed = Edit::default();
        frame(ctx, c, kind, &mut ed, vec![Event::PointerMoved(a)]);
        frame(ctx, c, kind, &mut ed, vec![press(a, true)]);
        frame(ctx, c, kind, &mut ed, vec![Event::PointerMoved(a + (b - a) * 0.5)]);
        frame(ctx, c, kind, &mut ed, vec![Event::PointerMoved(b)]);
        frame(ctx, c, kind, &mut ed, vec![press(b, false)]);
        ed
    }

    #[test]
    fn dragging_a_point_moves_it_and_pitch_snaps_to_semitones() {
        let ctx = egui::Context::default();
        let mut c = Curve::new(&[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)]);
        let r = frame(&ctx, &mut c, Line::Pitch, &mut Edit::default(), vec![]);
        // ±12 shown: 7.3 semitones up, half way along (the first point stays before the second).
        let to = pos2(r.left() + r.width() * 0.5, r.center().y - r.height() * 7.3 / 24.0);
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
        assert!((c.points[1].t - 0.25).abs() < 0.02 && (c.points[1].v - 0.9).abs() < 0.03, "{:?}", c.points);
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
        // Up bows a rising line up: it rises fast first.
        assert!(c.points[0].tension < -0.3, "{:?}", c.points);
        assert!(c.at(0.25) > 0.4);
    }

    #[test]
    fn a_forge_channel_opens_its_window() {
        let mut app = App::with_song(keyframexl::sequence::Song::default(), None);
        let id = app.add_forge();
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut tab = super::super::Tab::Instrument { id };
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1366.0, 768.0))),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.tab_ui(ui, &mut tab));
            });
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

    /// Poll like the frame loop does until nothing is in flight (5 s at most).
    fn settle(app: &mut App) {
        let ctx = egui::Context::default();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.poll_forge(&ctx);
            let stale = app.song.instruments.iter().any(|i| matches!(&i.kind, InstrumentKind::Forge { sound } if sound.stale()));
            if !app.forge_jobs.busy() && !stale {
                return;
            }
            assert!(std::time::Instant::now() < until, "forge work never finished");
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
        assert!(s.stale() && Arc::ptr_eq(&s.rendered, &before), "the edit returns before rendering");
        // A second edit while the first renders: only the last one is installed.
        app.poll_forge(&egui::Context::default());
        app.commit_forge(id, Recipe::Hit.patch());
        settle(&mut app);
        let s = sound(&app, id);
        assert!(!s.stale());
        assert_eq!(*s.rendered, forge::render::render(&Recipe::Hit.patch()));
        // Undo brings the Jump back, and its audio with it.
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
    fn a_channel_note_bounces_into_a_layer() {
        let mut app = app();
        let tide = app.song.add_channel("Tide", InstrumentKind::Synth { patch: keyframexl::synth::SynthPatch::factory(2) });
        let id = app.add_forge();
        let layers = sound(&app, id).patch.layers.len();
        let b = forge::Bounce { channel: tide, key: 60, hold: 0.3 };
        app.forge_bounce(id, Into::NewLayer, b);
        settle(&mut app);
        let s = sound(&app, id);
        assert_eq!(s.patch.layers.len(), layers + 1);
        let Source::Sample { frames, from, .. } = &s.patch.layers[layers].source else {
            panic!("not a sample layer");
        };
        assert_eq!(*from, Some(b));
        assert!(frames.len() > 4800, "the note and its release: {}", frames.len());
        assert!(frames[0][0].abs() + frames[0][1].abs() > 0.0, "starts on the sound");
        assert!(s.rendered.peak > 0.05);
        // Bounce again into the same layer, an octave up and shorter.
        app.forge_bounce(id, Into::Layer(layers), forge::Bounce { key: 72, hold: 0.1, ..b });
        settle(&mut app);
        let s2 = sound(&app, id);
        assert_eq!(s2.patch.layers.len(), layers + 1);
        assert!(matches!(&s2.patch.layers[layers].source, Source::Sample { from: Some(f), .. } if f.key == 72));
    }
}
