//! Welcome screen: the hub shown at startup (and from `HELP → Welcome`).
//!
//! Recent projects and quick actions on the left; starting points on the
//! right (project templates, KeyFrame Tide factory presets, Forge recipes).
//! `show_welcome` owns no app state: it draws, and returns what was picked.
//! The caller acts on the returned [`WelcomeAction`], so the whole screen runs
//! in a headless egui context for tests.

use super::kit;
use crate::theme as t;
use eframe::egui::{
    self, Align, Align2, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2,
    vec2,
};
use keyframexl::{
    forge::{ForgeSound, recipes::Recipe},
    fx::{FxKind, FxSlot},
    sequence::{BAR, InstrumentKind, Note, STEP, STEP_PITCH, Song, default_tempo},
    synth::SynthPatch,
    vocal::{Voice, install},
};
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ copy

/// Every label on the screen, so the caller can hand in translated text.
#[derive(Clone, Debug, PartialEq)]
pub struct WelcomeCopy {
    pub window_title: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub close_hint: &'static str,
    pub update_badge: &'static str,
    pub update_hint: &'static str,
    pub recent_heading: &'static str,
    pub recent_empty: &'static str,
    pub recent_missing: &'static str,
    pub recent_missing_hint: &'static str,
    pub btn_new: &'static str,
    pub btn_open: &'static str,
    pub btn_open_empty: &'static str,
    pub btn_import_bundle: &'static str,
    pub btn_import_flp: &'static str,
    pub btn_settings: &'static str,
    pub templates_heading: &'static str,
    pub tab_templates: &'static str,
    pub tab_tide: &'static str,
    pub tab_forge: &'static str,
    pub search_hint: &'static str,
    pub no_match: &'static str,
    pub show_on_startup: &'static str,
    pub continue_btn: &'static str,
    pub template_titles: [&'static str; 6],
    pub template_descs: [&'static str; 6],
}

impl WelcomeCopy {
    pub fn english() -> Self {
        Self {
            window_title: "Welcome to KeyFrameXL",
            title: "KeyFrameXL",
            subtitle: "Pattern-based studio for beats, synths, voices and game sounds",
            close_hint: "Close (Esc)",
            update_badge: "Update available",
            update_hint: "A newer release is on GitHub. Click to see what changed.",
            recent_heading: "Recent projects",
            recent_empty: "No recent projects yet. Open a .kfxl file or start from a template.",
            recent_missing: "Missing",
            recent_missing_hint: "This file was moved or deleted",
            btn_new: "New project",
            btn_open: "Open .kfxl…",
            btn_open_empty: "Open a project…",
            btn_import_bundle: "Import bundle (.zip)…",
            btn_import_flp: "Import FL Studio (.flp)…",
            btn_settings: "Audio & MIDI settings (F10)",
            templates_heading: "Start something new",
            tab_templates: "Templates",
            tab_tide: "Tide presets",
            tab_forge: "Forge SFX",
            search_hint: "Search by name",
            no_match: "Nothing matches that search.",
            show_on_startup: "Show welcome screen on startup",
            continue_btn: "Continue to workspace",
            template_titles: [
                "Empty beat",
                "Demo beat",
                "Tide studio",
                "Forge SFX bench",
                "Vocal session",
                "Blank sampler",
            ],
            template_descs: [
                "Kick, snare, hat and a sampler bass on an empty pattern.",
                "The same rack with a groove written in. Press Space.",
                "Beat rack plus a Soft Pad and a Bass on their own inserts.",
                "One game-sound channel with a master limiter, ready to design.",
                "Beat rack plus a singing channel with EQ, compressor and reverb.",
                "A single sampler channel and nothing else.",
            ],
        }
    }

    pub fn for_language(lang: crate::i18n::Language) -> Self {
        let s = crate::i18n::strings(lang);
        match lang {
            crate::i18n::Language::English => Self::english(),
            crate::i18n::Language::Ukrainian => Self {
                window_title: s.welcome_window_title,
                title: "KeyFrameXL",
                subtitle: s.welcome_subtitle,
                close_hint: "Закрити (Esc)",
                update_badge: s.welcome_update_badge,
                update_hint: "На GitHub є новіший реліз. Натисніть, щоб переглянути зміни.",
                recent_heading: s.welcome_recent_heading,
                recent_empty: s.welcome_recent_empty,
                recent_missing: s.welcome_recent_missing,
                recent_missing_hint: "Цей файл переміщено або видалено",
                btn_new: s.welcome_new_project,
                btn_open: s.welcome_open_project,
                btn_open_empty: s.welcome_open_project,
                btn_import_bundle: s.welcome_import_bundle,
                btn_import_flp: s.welcome_import_flp,
                btn_settings: s.welcome_settings,
                templates_heading: s.welcome_templates_heading,
                tab_templates: s.welcome_tab_templates,
                tab_tide: s.welcome_tab_tide,
                tab_forge: s.welcome_tab_forge,
                search_hint: s.welcome_search_presets,
                no_match: "Нічого не знайдено за цим запитом.",
                show_on_startup: s.welcome_show_on_startup,
                continue_btn: s.welcome_continue,
                template_titles: [
                    s.tpl_beat_title,
                    s.tpl_demo_title,
                    s.tpl_tide_title,
                    s.tpl_forge_title,
                    s.tpl_vocal_title,
                    s.tpl_blank_title,
                ],
                template_descs: [
                    s.tpl_beat_desc,
                    s.tpl_demo_desc,
                    s.tpl_tide_desc,
                    s.tpl_forge_desc,
                    s.tpl_vocal_desc,
                    s.tpl_blank_desc,
                ],
            },
        }
    }
}

impl Default for WelcomeCopy {
    fn default() -> Self {
        Self::english()
    }
}

// ------------------------------------------------------------- templates

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateKind {
    EmptyBeat,
    DemoBeat,
    TideStudio,
    ForgeSfx,
    VocalSession,
    BlankMinimal,
}

impl TemplateKind {
    pub const ALL: [TemplateKind; 6] = [
        TemplateKind::EmptyBeat,
        TemplateKind::DemoBeat,
        TemplateKind::TideStudio,
        TemplateKind::ForgeSfx,
        TemplateKind::VocalSession,
        TemplateKind::BlankMinimal,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Channel count and tempo, as the card prints them.
    pub fn tag(self) -> &'static str {
        match self {
            TemplateKind::EmptyBeat => "4 CH · 120 BPM",
            TemplateKind::DemoBeat => "4 CH · 120 BPM",
            TemplateKind::TideStudio => "6 CH · 120 BPM",
            TemplateKind::ForgeSfx => "1 CH · 120 BPM",
            TemplateKind::VocalSession => "5 CH · 120 BPM",
            TemplateKind::BlankMinimal => "1 CH · 120 BPM",
        }
    }

    /// Address colour of the card's stripe.
    fn accent(self) -> egui::Color32 {
        match self {
            TemplateKind::EmptyBeat => t::BRAND,
            TemplateKind::DemoBeat => t::ENC_BLUE,
            TemplateKind::TideStudio => t::ENC_GREEN,
            TemplateKind::ForgeSfx => t::ENC_ORANGE,
            TemplateKind::VocalSession => t::WAIT,
            TemplateKind::BlankMinimal => t::ENC_WHITE,
        }
    }
}

/// A project with no channels: only the first empty pattern and the mixer.
fn bare_song() -> Song {
    let mut song = Song::default();
    song.instruments.clear();
    song.tempo = default_tempo();
    song
}

/// Route a channel to mixer insert `insert` and name that insert.
fn route(song: &mut Song, channel: u64, insert: usize, name: &str, fx: &[FxKind]) {
    if let Some(i) = song.instruments.iter_mut().find(|i| i.id == channel) {
        i.mixer = insert as u16;
    }
    let track = &mut song.mixer[insert];
    track.name = name.into();
    track.fx = fx.iter().map(|k| FxSlot::new(*k)).collect();
}

fn tide_channel(song: &mut Song, name: &str, preset: usize) -> u64 {
    song.add_channel(
        name,
        InstrumentKind::Synth {
            patch: SynthPatch::factory(preset),
        },
    )
}

fn forge_channel(song: &mut Song, recipe: Recipe) -> u64 {
    song.add_channel(
        recipe.label(),
        InstrumentKind::Forge {
            sound: ForgeSound::new(recipe.patch()),
        },
    )
}

/// The first installed voicebank (Teto first), or an empty voice to pick later.
fn default_voice() -> Voice {
    let banks = install::installed();
    let pick = banks
        .iter()
        .find(|b| b.name.contains("テト") || b.name.to_lowercase().contains("teto"))
        .or(banks.first());
    match pick {
        Some(b) => Voice::new(&b.path.to_string_lossy(), &b.name),
        None => Voice::new("", ""),
    }
}

pub fn build_template(kind: TemplateKind) -> Song {
    let song = match kind {
        TemplateKind::EmptyBeat => Song::default(),
        TemplateKind::DemoBeat => Song::demo(),
        TemplateKind::TideStudio => {
            let mut song = Song::default();
            let pad = tide_channel(&mut song, "Soft Pad", 1);
            let bass = tide_channel(&mut song, "Bass", 3);
            route(&mut song, pad, 5, "Pad", &[FxKind::Reverb]);
            route(&mut song, bass, 6, "Bass", &[FxKind::Delay]);
            for pitch in [60, 63, 67] {
                song.add_note(1, Note::new(pad, 0, BAR, pitch, 90));
            }
            song.add_note(1, Note::new(bass, 0, BAR, 36, 100));
            song
        }
        TemplateKind::ForgeSfx => {
            let mut song = bare_song();
            let id = forge_channel(&mut song, Recipe::Jump);
            song.mixer[0].fx = vec![FxSlot::new(FxKind::Limiter)];
            for step in [0, 8] {
                song.add_note(1, Note::new(id, step * STEP, STEP, STEP_PITCH, 100));
            }
            song
        }
        TemplateKind::VocalSession => {
            let mut song = Song::default();
            let id = song.add_channel(
                "Vocal",
                InstrumentKind::Vocal {
                    voice: default_voice(),
                },
            );
            route(
                &mut song,
                id,
                5,
                "Vocal",
                &[FxKind::ParamEq, FxKind::Compressor, FxKind::Reverb],
            );
            song
        }
        TemplateKind::BlankMinimal => {
            let mut song = bare_song();
            song.add_sampler("Sampler 1");
            song
        }
    };
    debug_assert!(song.validate().is_ok());
    song
}

/// A clean project whose only channel is KeyFrame Tide on factory preset
/// `preset_idx`, with four notes to hear it by.
pub fn build_tide_preset_song(preset_idx: usize) -> Song {
    let idx = preset_idx.min(SynthPatch::PRESETS.len() - 1);
    let name = SynthPatch::PRESETS[idx];
    let mut song = bare_song();
    let id = tide_channel(&mut song, name, idx);
    for (step, pitch) in [(0, 60), (4, 63), (8, 67), (12, 72)] {
        song.add_note(1, Note::new(id, step * STEP, 3 * STEP, pitch, 100));
    }
    song.info.title = name.into();
    debug_assert!(song.validate().is_ok());
    song
}

/// Forge recipe names, in the order `build_forge_recipe_song` takes.
pub fn forge_recipe_names() -> Vec<&'static str> {
    Recipe::ALL.iter().map(|r| r.label()).collect()
}

/// A clean project whose only channel is a Forge sound on recipe `recipe_idx`.
pub fn build_forge_recipe_song(recipe_idx: usize) -> Song {
    let recipe = Recipe::ALL[recipe_idx.min(Recipe::ALL.len() - 1)];
    let mut song = bare_song();
    let id = forge_channel(&mut song, recipe);
    song.mixer[0].fx = vec![FxSlot::new(FxKind::Limiter)];
    song.add_note(1, Note::new(id, 0, STEP, STEP_PITCH, 100));
    song.info.title = recipe.label().into();
    debug_assert!(song.validate().is_ok());
    song
}

// ----------------------------------------------------------------- state

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WelcomeTab {
    #[default]
    Templates,
    Tide,
    Forge,
}

#[derive(Clone, Debug, Default)]
pub struct WelcomeState {
    pub tab: WelcomeTab,
    pub query: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WelcomeAction {
    NewProject,
    OpenProject,
    OpenRecent(PathBuf),
    ImportBundle,
    ImportFlp,
    OpenSettings,
    OpenUpdater,
    LoadTemplate(TemplateKind),
    LoadTidePreset(usize),
    LoadForgeRecipe(usize),
    Close,
}

// ---------------------------------------------------------------- layout

const MODAL_W: f32 = 820.0;
const MODAL_H: f32 = 540.0;
const HEADER_H: f32 = 64.0;
const FOOTER_H: f32 = 48.0;
const LEFT_W: f32 = 310.0;
const PAD: f32 = 16.0;
const GAP: f32 = 10.0;
const ROW_H: f32 = 44.0;
const ACTION_H: f32 = kit::TOOL_H;
const ACTION_GAP: f32 = 6.0;
/// New project (34) + gap + four action rows.
const ACTIONS_H: f32 = kit::CONTROL_H + GAP + 4.0 * ACTION_H + 3.0 * ACTION_GAP;
const CARD_H: f32 = 96.0;
const LIST_ROW_H: f32 = 34.0;

pub fn show_welcome(
    ctx: &egui::Context,
    state: &mut WelcomeState,
    copy: &WelcomeCopy,
    recent: &[PathBuf],
    show_on_startup: &mut bool,
    update_version: Option<&str>,
) -> Option<WelcomeAction> {
    let mut action: Option<WelcomeAction> = None;
    let screen = ctx.screen_rect();
    let size = vec2(
        MODAL_W.min((screen.width() - 32.0).max(480.0)),
        MODAL_H.min((screen.height() - 32.0).max(380.0)),
    );
    let frame = egui::Frame::new()
        .fill(t::BG)
        .stroke(Stroke::new(1.5_f32, t::EDGE))
        .corner_radius(10.0);
    egui::Modal::new(egui::Id::new("kfxl-welcome"))
        .backdrop_color(t::SHADOW.gamma_multiply(0.7))
        .frame(frame)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let (modal, r) = ui.allocate_exact_size(size, Sense::hover());
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Other, true, copy.window_title)
            });
            header(ui, modal, copy, update_version, &mut action);
            let body_top = modal.top() + HEADER_H;
            let body_bottom = modal.bottom() - FOOTER_H;
            let left = Rect::from_min_max(
                pos2(modal.left(), body_top),
                pos2(modal.left() + LEFT_W, body_bottom),
            );
            let right = Rect::from_min_max(
                pos2(left.right(), body_top),
                pos2(modal.right(), body_bottom),
            );
            left_column(ui, left, copy, recent, &mut action);
            right_column(ui, right, state, copy, &mut action);
            footer(ui, modal, copy, show_on_startup, &mut action);
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        action.get_or_insert(WelcomeAction::Close);
    }
    action
}

fn top_corners(r: u8) -> CornerRadius {
    CornerRadius {
        nw: r,
        ne: r,
        sw: 0,
        se: 0,
    }
}

fn header(
    ui: &mut Ui,
    modal: Rect,
    copy: &WelcomeCopy,
    update_version: Option<&str>,
    action: &mut Option<WelcomeAction>,
) {
    let p = ui.painter().clone();
    let h = Rect::from_min_size(modal.min, vec2(modal.width(), HEADER_H));
    p.rect_filled(h.shrink2(vec2(1.0, 0.0)), top_corners(9), t::PANEL_TOP);
    p.line_segment(
        [
            pos2(h.left(), h.bottom() - 0.5),
            pos2(h.right(), h.bottom() - 0.5),
        ],
        Stroke::new(1.0_f32, t::LINE),
    );
    kit::icon_brand(
        &p,
        Rect::from_center_size(pos2(h.left() + PAD + 20.0, h.center().y), vec2(40.0, 40.0)),
    );
    let x = h.left() + PAD + 40.0 + 14.0;
    let title_w = kit::text_width(&p, copy.title, t::semibold(20.0));
    p.text(
        pos2(x, h.top() + 11.0),
        Align2::LEFT_TOP,
        copy.title,
        t::semibold(20.0),
        t::TEXT,
    );
    p.text(
        pos2(x + title_w + 10.0, h.top() + 33.0),
        Align2::LEFT_BOTTOM,
        format!("v{}", env!("CARGO_PKG_VERSION")),
        t::mono(12.0),
        t::MUTED,
    );
    p.text(
        pos2(x, h.top() + 40.0),
        Align2::LEFT_TOP,
        copy.subtitle,
        t::sans(12.0),
        t::MUTED,
    );

    let close =
        Rect::from_center_size(pos2(h.right() - PAD - 14.0, h.center().y), vec2(28.0, 28.0));
    let r = ui.interact(close, ui.id().with("welcome-close"), Sense::click());
    kit::paint_button(
        ui,
        close,
        &r,
        kit::Kind::Ghost,
        true,
        "Close",
        |p, face, ink| {
            p.text(
                face.center() - vec2(0.0, 1.0),
                Align2::CENTER_CENTER,
                "×",
                t::sans(20.0),
                ink,
            );
        },
    );
    if kit::tip(r.clone(), copy.close_hint).clicked() {
        action.get_or_insert(WelcomeAction::Close);
    }

    if let Some(v) = update_version {
        let label = format!("{} · v{v}", copy.update_badge);
        let font = t::mono(12.0);
        let w = kit::text_width(&p, &label, font.clone()) + 20.0;
        let pill = Rect::from_center_size(
            pos2(close.left() - GAP - w / 2.0, h.center().y),
            vec2(w, 24.0),
        );
        let r = ui.interact(pill, ui.id().with("welcome-update"), Sense::click());
        let hover = kit::hover_amount(ui, &r);
        p.rect_filled(pill, 6.0, kit::mix(t::WAIT_SOFT, t::SELECTION, hover));
        p.rect_stroke(
            pill,
            6.0,
            Stroke::new(1.0_f32, t::WAIT),
            egui::StrokeKind::Inside,
        );
        p.text(pill.center(), Align2::CENTER_CENTER, label, font, t::WAIT);
        if kit::show_focus(ui, &r) {
            kit::focus_ring(&p, pill, 6.0);
        }
        if kit::tip(r.clone(), copy.update_hint).clicked() {
            action.get_or_insert(WelcomeAction::OpenUpdater);
        }
    }
}

fn footer(
    ui: &mut Ui,
    modal: Rect,
    copy: &WelcomeCopy,
    show_on_startup: &mut bool,
    action: &mut Option<WelcomeAction>,
) {
    let p = ui.painter().clone();
    let f = Rect::from_min_max(pos2(modal.left(), modal.bottom() - FOOTER_H), modal.max);
    p.rect_filled(
        f.shrink2(vec2(1.0, 0.0)),
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 9,
            se: 9,
        },
        t::PANEL_BOTTOM,
    );
    p.line_segment(
        [
            pos2(f.left(), f.top() + 0.5),
            pos2(f.right(), f.top() + 0.5),
        ],
        Stroke::new(1.0_f32, t::LINE),
    );

    // Checkbox: sunken box, a check when on.
    let bx = Rect::from_center_size(pos2(f.left() + PAD + 9.0, f.center().y), vec2(18.0, 18.0));
    let label_w = kit::text_width(&p, copy.show_on_startup, t::sans(13.0));
    let hit = Rect::from_min_max(
        pos2(bx.left() - 2.0, f.top() + 6.0),
        pos2(bx.right() + 10.0 + label_w + 4.0, f.bottom() - 6.0),
    );
    let r = ui.interact(hit, ui.id().with("welcome-startup"), Sense::click());
    r.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            true,
            *show_on_startup,
            copy.show_on_startup,
        )
    });
    kit::sunken(&p, bx, 4.0);
    if *show_on_startup {
        let c = bx.center();
        let st = Stroke::new(1.8_f32, t::TEXT);
        p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.5, 3.0)], st);
        p.line_segment([c + vec2(-1.5, 3.0), c + vec2(4.5, -3.5)], st);
    }
    p.text(
        pos2(bx.right() + 10.0, f.center().y),
        Align2::LEFT_CENTER,
        copy.show_on_startup,
        t::sans(13.0),
        if r.hovered() { t::TEXT } else { t::MUTED },
    );
    if kit::show_focus(ui, &r) {
        kit::focus_ring(&p, bx, 4.0);
    }
    if r.clicked() {
        *show_on_startup = !*show_on_startup;
    }

    // Continue: a plain button, the one red action lives on the left.
    let font = t::sans(14.0);
    let w = kit::text_width(&p, copy.continue_btn, font.clone()) + 32.0;
    let rect = Rect::from_center_size(
        pos2(f.right() - PAD - w / 2.0, f.center().y),
        vec2(w, kit::CONTROL_H),
    );
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    if kit::button(&mut child, copy.continue_btn, kit::Kind::Normal, true).clicked() {
        action.get_or_insert(WelcomeAction::Close);
    }
}

// ------------------------------------------------------------ left column

fn left_column(
    ui: &mut Ui,
    col: Rect,
    copy: &WelcomeCopy,
    recent: &[PathBuf],
    action: &mut Option<WelcomeAction>,
) {
    let p = ui.painter().clone();
    p.rect_filled(
        col.shrink2(vec2(1.0, 0.0)),
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 9,
            se: 0,
        },
        t::SIDE,
    );
    p.line_segment(
        [
            pos2(col.right() - 0.5, col.top()),
            pos2(col.right() - 0.5, col.bottom()),
        ],
        Stroke::new(1.0_f32, t::LINE),
    );
    let inner = col.shrink(PAD);
    kit::caps(
        &p,
        pos2(inner.left(), inner.top() + 9.0),
        Align2::LEFT_CENTER,
        copy.recent_heading,
        12.0,
        t::MUTED,
    );

    // Quick actions pinned to the bottom.
    let actions = Rect::from_min_max(pos2(inner.left(), inner.bottom() - ACTIONS_H), inner.max);
    let mut aui = ui.new_child(UiBuilder::new().max_rect(actions));
    aui.spacing_mut().item_spacing = vec2(0.0, ACTION_GAP);
    if brand_button(&mut aui, copy.btn_new, inner.width()).clicked() {
        action.get_or_insert(WelcomeAction::NewProject);
    }
    aui.add_space(GAP - ACTION_GAP);
    let rows = [
        (copy.btn_open, WelcomeAction::OpenProject),
        (copy.btn_import_bundle, WelcomeAction::ImportBundle),
        (copy.btn_import_flp, WelcomeAction::ImportFlp),
        (copy.btn_settings, WelcomeAction::OpenSettings),
    ];
    for (label, act) in rows {
        let r = kit::button_sized(
            &mut aui,
            vec2(inner.width(), ACTION_H),
            kit::Kind::Normal,
            true,
            label,
            |p, face, ink| {
                p.text(
                    pos2(face.left() + 12.0, face.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    t::sans(13.0),
                    ink,
                );
            },
        );
        if r.clicked() {
            action.get_or_insert(act);
        }
    }

    let list = Rect::from_min_max(
        pos2(inner.left(), inner.top() + 26.0),
        pos2(inner.right(), actions.top() - GAP),
    );
    let mut lui = ui.new_child(
        UiBuilder::new()
            .max_rect(list)
            .layout(Layout::top_down(Align::Min)),
    );
    if recent.is_empty() {
        lui.spacing_mut().item_spacing = vec2(0.0, 10.0);
        lui.label(
            RichText::new(copy.recent_empty)
                .font(t::sans(13.0))
                .color(t::MUTED),
        );
        if kit::button(&mut lui, copy.btn_open_empty, kit::Kind::Normal, true).clicked() {
            action.get_or_insert(WelcomeAction::OpenProject);
        }
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("welcome-recent")
        .auto_shrink([false, false])
        .show(&mut lui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            for (i, path) in recent.iter().enumerate() {
                if recent_row(ui, i, path, copy) {
                    action.get_or_insert(WelcomeAction::OpenRecent(path.clone()));
                }
            }
        });
}

/// The big red button of the screen: one per viewport.
fn brand_button(ui: &mut Ui, label: &str, width: f32) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(width, kit::CONTROL_H), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let p = ui.painter();
    let hover = kit::hover_amount(ui, &r);
    let rect = rect.shrink(0.5);
    let face = kit::raised(
        p,
        rect,
        kit::RADIUS,
        kit::mix(t::BRAND_TOP, t::BRAND_GLOW, hover),
        kit::mix(t::BRAND_FILL, t::BRAND_TOP, hover),
        t::BRAND_EDGE,
        r.is_pointer_button_down_on(),
        kit::DEPTH,
    );
    p.text(
        face.center(),
        Align2::CENTER_CENTER,
        label,
        t::medium(14.0),
        t::ON_BRAND,
    );
    if kit::show_focus(ui, &r) {
        kit::focus_ring(p, rect, kit::RADIUS);
    }
    r
}

/// Home folder shortened to `~`.
fn short_dir(path: &Path) -> String {
    let dir = path.parent().unwrap_or(path);
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = dir.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        };
    }
    dir.display().to_string()
}

fn recent_row(ui: &mut Ui, i: usize, path: &Path, copy: &WelcomeCopy) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let missing = !path.exists();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &stem));
    let _ = i;
    let p = ui.painter();
    if r.hovered() || r.has_focus() {
        p.rect_filled(rect, 6.0, t::SELECTION);
    }
    let badge_w = if missing {
        kit::text_width(p, &copy.recent_missing.to_uppercase(), t::mono(12.0)) + 16.0
    } else {
        0.0
    };
    let text_w = rect.width() - 24.0 - badge_w;
    let name = kit::elide(p, &stem, t::medium(13.0), text_w);
    p.text(
        pos2(rect.left() + 12.0, rect.top() + 7.0),
        Align2::LEFT_TOP,
        name,
        t::medium(13.0),
        if missing { t::MUTED } else { t::TEXT },
    );
    let dir = kit::elide(p, &short_dir(path), t::mono(11.0), text_w);
    p.text(
        pos2(rect.left() + 12.0, rect.top() + 25.0),
        Align2::LEFT_TOP,
        dir,
        t::mono(11.0),
        t::FAINT,
    );
    if missing {
        kit::caps(
            p,
            pos2(rect.right() - 10.0, rect.top() + 14.0),
            Align2::RIGHT_CENTER,
            copy.recent_missing,
            12.0,
            t::BAD,
        );
    }
    if kit::show_focus(ui, &r) {
        kit::focus_ring(ui.painter(), rect, 6.0);
    }
    if missing {
        kit::tip(r.clone(), copy.recent_missing_hint);
    } else {
        kit::tip(r.clone(), path.display().to_string());
    }
    r.clicked()
}

// ----------------------------------------------------------- right column

fn right_column(
    ui: &mut Ui,
    col: Rect,
    state: &mut WelcomeState,
    copy: &WelcomeCopy,
    action: &mut Option<WelcomeAction>,
) {
    let p = ui.painter().clone();
    let inner = col.shrink(PAD);
    kit::caps(
        &p,
        pos2(inner.left(), inner.top() + 9.0),
        Align2::LEFT_CENTER,
        copy.templates_heading,
        12.0,
        t::MUTED,
    );
    let mut rui = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(inner.left(), inner.top() + 26.0),
                inner.max,
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    rui.spacing_mut().item_spacing = vec2(GAP, GAP);

    let tide_label = format!("{} ({})", copy.tab_tide, SynthPatch::PRESETS.len());
    let tabs = [copy.tab_templates, tide_label.as_str(), copy.tab_forge];
    let selected = match state.tab {
        WelcomeTab::Templates => 0,
        WelcomeTab::Tide => 1,
        WelcomeTab::Forge => 2,
    };
    let (clicked, _) = kit::chips(&mut rui, "welcome-tabs", &tabs, selected, None);
    if let Some(i) = clicked {
        let next = [WelcomeTab::Templates, WelcomeTab::Tide, WelcomeTab::Forge][i];
        if next != state.tab {
            state.tab = next;
            state.query.clear();
        }
    }

    if state.tab == WelcomeTab::Templates {
        egui::ScrollArea::vertical()
            .id_salt("welcome-templates")
            .auto_shrink([false, false])
            .show(&mut rui, |ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                let w = ((ui.available_width() - GAP) / 2.0).floor() - 2.0;
                for pair in TemplateKind::ALL.chunks(2) {
                    ui.horizontal(|ui| {
                        for kind in pair {
                            if template_card(ui, *kind, copy, w) {
                                action.get_or_insert(WelcomeAction::LoadTemplate(*kind));
                            }
                        }
                    });
                }
            });
        return;
    }

    search_field(&mut rui, &mut state.query, copy);
    let q = state.query.trim().to_lowercase();
    let names: Vec<&'static str> = if state.tab == WelcomeTab::Tide {
        SynthPatch::PRESETS.to_vec()
    } else {
        forge_recipe_names()
    };
    let items: Vec<(usize, &str)> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q))
        .map(|(i, n)| (i, *n))
        .collect();
    let picked = egui::ScrollArea::vertical()
        .id_salt(("welcome-list", state.tab == WelcomeTab::Tide))
        .auto_shrink([false, false])
        .show(&mut rui, |ui| {
            if items.is_empty() {
                ui.label(
                    RichText::new(copy.no_match)
                        .font(t::sans(13.0))
                        .color(t::MUTED),
                );
                return None;
            }
            ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
            let w = ((ui.available_width() - GAP) / 2.0).floor() - 2.0;
            let mut picked = None;
            for pair in items.chunks(2) {
                ui.horizontal(|ui| {
                    for (idx, name) in pair {
                        if list_row(ui, *idx, name, w) {
                            picked = Some(*idx);
                        }
                    }
                });
            }
            picked
        })
        .inner;
    if let Some(idx) = picked {
        action.get_or_insert(if state.tab == WelcomeTab::Tide {
            WelcomeAction::LoadTidePreset(idx)
        } else {
            WelcomeAction::LoadForgeRecipe(idx)
        });
    }
}

fn search_field(ui: &mut Ui, query: &mut String, copy: &WelcomeCopy) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let p = ui.painter().clone();
    kit::sunken(&p, rect, kit::RADIUS);
    ui.put(
        rect.shrink2(vec2(10.0, 2.0)),
        egui::TextEdit::singleline(query)
            .id(egui::Id::new("welcome-search"))
            .frame(false)
            .font(t::sans(13.0))
            .hint_text(
                RichText::new(copy.search_hint)
                    .font(t::sans(13.0))
                    .color(t::FAINT),
            )
            .vertical_align(Align::Center)
            .desired_width(rect.width() - 20.0),
    );
}

fn template_card(ui: &mut Ui, kind: TemplateKind, copy: &WelcomeCopy, w: f32) -> bool {
    let i = kind.index();
    let title = copy.template_titles[i];
    let (rect, r) = ui.allocate_exact_size(vec2(w, CARD_H), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    let hover = kit::hover_amount(ui, &r);
    let down = r.is_pointer_button_down_on();
    let p = ui.painter();
    let outer = rect.shrink(0.5);
    let face = kit::control(p, outer, kit::RADIUS, hover, down);
    let stripe = Rect::from_min_max(
        pos2(face.left() + 8.0, face.top() + 10.0),
        pos2(face.left() + 12.0, face.bottom() - 10.0),
    );
    p.rect_filled(stripe, 2.0, kind.accent());
    let x = face.left() + 24.0;
    let text_w = face.right() - x - 12.0;
    let dy = if down { 1.0 } else { 0.0 };
    p.text(
        pos2(x, face.top() + 12.0 + dy),
        Align2::LEFT_TOP,
        kit::elide(p, title, t::medium(14.0), text_w),
        t::medium(14.0),
        t::TEXT,
    );
    let desc = p.layout(
        copy.template_descs[i].to_string(),
        t::sans(12.0),
        t::MUTED,
        text_w,
    );
    p.galley(pos2(x, face.top() + 34.0 + dy), desc, t::MUTED);
    kit::caps(
        p,
        pos2(x, face.bottom() - 10.0 + dy),
        Align2::LEFT_BOTTOM,
        kind.tag(),
        12.0,
        t::FAINT,
    );
    if kit::show_focus(ui, &r) {
        kit::focus_ring(ui.painter(), outer, kit::RADIUS);
    }
    r.clicked()
}

fn list_row(ui: &mut Ui, idx: usize, name: &str, w: f32) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(w, LIST_ROW_H), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let hover = kit::hover_amount(ui, &r);
    let down = r.is_pointer_button_down_on();
    let p = ui.painter();
    let outer = rect.shrink(0.5);
    let face = kit::control(p, outer, kit::RADIUS, hover, down);
    let stripe = Rect::from_min_max(
        pos2(face.left() + 7.0, face.top() + 8.0),
        pos2(face.left() + 10.0, face.bottom() - 8.0),
    );
    p.rect_filled(stripe, 1.5, t::pattern_code(idx as u64));
    let cy = face.center().y + if down { 1.0 } else { 0.0 };
    p.text(
        pos2(face.left() + 20.0, cy),
        Align2::LEFT_CENTER,
        format!("{:02}", idx + 1),
        t::mono(11.0),
        t::FAINT,
    );
    let name_x = face.left() + 46.0;
    p.text(
        pos2(name_x, cy),
        Align2::LEFT_CENTER,
        kit::elide(p, name, t::sans(13.0), face.right() - name_x - 10.0),
        t::sans(13.0),
        t::TEXT,
    );
    if kit::show_focus(ui, &r) {
        kit::focus_ring(ui.painter(), outer, kit::RADIUS);
    }
    r.clicked()
}

// ----------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Vec2};
    use keyframexl::sequence::InstrumentKind as K;

    fn kinds(song: &Song) -> Vec<&'static str> {
        song.instruments
            .iter()
            .map(|i| match i.kind {
                K::Drum { .. } => "drum",
                K::Sampler { .. } => "sampler",
                K::Synth { .. } => "synth",
                K::Vocal { .. } => "vocal",
                K::Plugin { .. } => "plugin",
                K::Forge { .. } => "forge",
            })
            .collect()
    }

    #[test]
    fn every_template_builds_a_valid_song() {
        for kind in TemplateKind::ALL {
            let song = build_template(kind);
            assert_eq!(song.validate(), Ok(()), "{kind:?}");
            assert_eq!(song.tempo, 120.0, "{kind:?}");
            let k = kinds(&song);
            match kind {
                TemplateKind::EmptyBeat => {
                    assert_eq!(k, ["drum", "drum", "drum", "sampler"]);
                    assert!(song.patterns[0].notes.is_empty());
                }
                TemplateKind::DemoBeat => {
                    assert_eq!(k.len(), 4);
                    assert!(!song.patterns[0].notes.is_empty());
                }
                TemplateKind::TideStudio => {
                    assert_eq!(k.len(), 6);
                    assert_eq!(k.iter().filter(|x| **x == "synth").count(), 2);
                    assert_eq!(song.instruments[4].mixer, 5);
                    assert_eq!(song.instruments[5].mixer, 6);
                    assert_eq!(song.mixer[5].fx[0].kind, FxKind::Reverb);
                    assert_eq!(song.mixer[6].fx[0].kind, FxKind::Delay);
                }
                TemplateKind::ForgeSfx => {
                    assert_eq!(k, ["forge"]);
                    assert_eq!(song.mixer[0].fx[0].kind, FxKind::Limiter);
                    assert!(!song.patterns[0].notes.is_empty());
                }
                TemplateKind::VocalSession => {
                    assert_eq!(k.len(), 5);
                    assert_eq!(k[4], "vocal");
                    assert_eq!(song.instruments[4].mixer, 5);
                    let fx: Vec<FxKind> = song.mixer[5].fx.iter().map(|s| s.kind).collect();
                    assert_eq!(fx, [FxKind::ParamEq, FxKind::Compressor, FxKind::Reverb]);
                }
                TemplateKind::BlankMinimal => assert_eq!(k, ["sampler"]),
            }
        }
    }

    #[test]
    fn copy_covers_every_template_and_tags_match_the_songs() {
        let copy = WelcomeCopy::english();
        for (i, kind) in TemplateKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), i);
            assert!(!copy.template_titles[i].is_empty());
            assert!(!copy.template_descs[i].is_empty());
            let n = build_template(*kind).instruments.len();
            assert!(kind.tag().starts_with(&format!("{n} CH")), "{kind:?}");
        }
    }

    #[test]
    fn every_tide_preset_starts_a_valid_project() {
        assert_eq!(SynthPatch::PRESETS.len(), 18);
        for i in 0..SynthPatch::PRESETS.len() {
            let song = build_tide_preset_song(i);
            assert_eq!(song.validate(), Ok(()), "preset {i}");
            assert_eq!(song.instruments.len(), 1);
            assert_eq!(song.instruments[0].name, SynthPatch::PRESETS[i]);
            let K::Synth { patch } = &song.instruments[0].kind else {
                panic!("preset {i} is not a synth");
            };
            assert_eq!(*patch, SynthPatch::factory(i));
            assert_eq!(song.info.title, SynthPatch::PRESETS[i]);
        }
    }

    #[test]
    fn every_forge_recipe_starts_a_valid_project() {
        let names = forge_recipe_names();
        assert_eq!(names.len(), Recipe::ALL.len());
        for (i, name) in names.iter().enumerate() {
            let song = build_forge_recipe_song(i);
            assert_eq!(song.validate(), Ok(()), "recipe {name}");
            assert_eq!(kinds(&song), ["forge"]);
            assert_eq!(song.instruments[0].name, *name);
        }
    }

    fn input(events: Vec<Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1366.0, 768.0))),
            events,
            ..Default::default()
        }
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut WelcomeState,
        recent: &[PathBuf],
        events: Vec<Event>,
        update: Option<&str>,
    ) -> Option<WelcomeAction> {
        let copy = WelcomeCopy::english();
        let mut startup = true;
        let mut out = None;
        let _ = ctx.run(input(events), |ctx| {
            out = show_welcome(ctx, state, &copy, recent, &mut startup, update);
        });
        out
    }

    #[test]
    fn renders_every_tab_with_and_without_recent_projects() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let here = std::env::current_dir().unwrap().join("Cargo.toml");
        let populated = vec![
            here,
            PathBuf::from("/nonexistent/folder/lost song.kfxl"),
            dirs::home_dir().unwrap_or_default().join("beat.kfxl"),
        ];
        for tab in [WelcomeTab::Templates, WelcomeTab::Tide, WelcomeTab::Forge] {
            for recent in [&[][..], &populated[..]] {
                for update in [None, Some("9.9.9")] {
                    let mut state = WelcomeState {
                        tab,
                        query: String::new(),
                    };
                    for _ in 0..3 {
                        assert_eq!(frame(&ctx, &mut state, recent, vec![], update), None);
                    }
                    // A search with no match, and one with matches.
                    state.query = "zzzz".into();
                    assert_eq!(frame(&ctx, &mut state, recent, vec![], update), None);
                    state.query = "e".into();
                    assert_eq!(frame(&ctx, &mut state, recent, vec![], update), None);
                }
            }
        }
    }

    #[test]
    fn small_windows_do_not_panic() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let copy = WelcomeCopy::english();
        let mut state = WelcomeState::default();
        let mut startup = false;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 400.0))),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            show_welcome(ctx, &mut state, &copy, &[], &mut startup, None);
        });
    }

    #[test]
    fn escape_closes() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut state = WelcomeState::default();
        frame(&ctx, &mut state, &[], vec![], None);
        let esc = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        };
        assert_eq!(
            frame(&ctx, &mut state, &[], vec![esc], None),
            Some(WelcomeAction::Close)
        );
    }

    #[test]
    fn clicking_the_first_template_card_loads_it() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut state = WelcomeState::default();
        for _ in 0..2 {
            frame(&ctx, &mut state, &[], vec![], None);
        }
        // First card: right column's top-left, under the heading and tab strip.
        let modal_left = (1366.0 - MODAL_W) / 2.0;
        let modal_top = (768.0 - MODAL_H) / 2.0;
        let at = pos2(
            modal_left + LEFT_W + PAD + 80.0,
            modal_top + HEADER_H + PAD + 26.0 + kit::CONTROL_H + GAP + 30.0,
        );
        let mv = Event::PointerMoved(at);
        let btn = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(&ctx, &mut state, &[], vec![mv], None);
        frame(&ctx, &mut state, &[], vec![btn(true)], None);
        let got = frame(&ctx, &mut state, &[], vec![btn(false)], None);
        assert_eq!(
            got,
            Some(WelcomeAction::LoadTemplate(TemplateKind::EmptyBeat))
        );
    }

    #[test]
    fn short_dir_uses_the_tilde() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(short_dir(&home.join("a.kfxl")), "~");
            assert_eq!(short_dir(&home.join("Music/a.kfxl")), "~/Music");
        }
        assert_eq!(short_dir(Path::new("/srv/x/a.kfxl")), "/srv/x");
    }
}
