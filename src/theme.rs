//! Tokens: BMS dark (bms-hub-design-kit tokens.json), its light counterpart,
//! and the Teenage Engineering colour code for encoders and pattern addresses. Nothing outside this file
//! may name a colour or a font file.

use eframe::egui::{self, Color32 as C, FontFamily, FontId, Stroke, Vec2};
use std::sync::Arc;

// Theme-following colours: one field per token, so a palette that forgets
// one does not build. Read them with the accessors below (`t::text()`).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // reserved tokens: input_line, bad_soft
pub struct Palette {
    pub edge: C,
    pub sunken: C,
    pub side: C,
    pub bg: C,
    pub surface: C,
    pub panel_bottom: C,
    pub panel_top: C,
    pub raised: C,
    pub selection: C,
    pub ctl_bottom: C,
    pub ctl_top: C,
    pub ctl_hover: C,
    pub line: C,
    pub input_line: C,
    pub strong: C,
    pub hi: C,
    pub low: C,
    pub shadow: C,
    pub text: C,
    pub muted: C,
    pub faint: C,
    pub focus: C,
    pub brand: C,
    pub brand_top: C,
    pub brand_fill: C,
    pub brand_glow: C,
    pub brand_edge: C,
    pub brand_soft: C,
    pub on_brand: C,
    pub ok: C,
    pub wait: C,
    pub wait_soft: C,
    pub bad: C,
    pub bad_soft: C,
    pub enc_blue: C,
    pub enc_green: C,
    pub enc_white: C,
    pub enc_orange: C,
    pub lcd_on: C,
    pub lcd_off: C,
    pub key_white: C,
    pub key_white_edge: C,
    pub key_black: C,
    pub key_ink: C,
    pub row_black: C,
    pub row_white: C,
    pub row_root: C,
    pub row_out: C,
}

/// BMS dark (the program's own look).
pub static DARK: Palette = Palette {
    edge: C::from_rgb(4, 5, 5),
    sunken: C::from_rgb(12, 13, 14),
    side: C::from_rgb(15, 16, 17),
    bg: C::from_rgb(20, 21, 22),
    surface: C::from_rgb(26, 27, 29),
    panel_bottom: C::from_rgb(32, 34, 37),
    panel_top: C::from_rgb(34, 36, 39),
    raised: C::from_rgb(34, 35, 38),
    selection: C::from_rgb(42, 44, 47),
    ctl_bottom: C::from_rgb(43, 45, 48),
    ctl_top: C::from_rgb(50, 52, 56),
    ctl_hover: C::from_rgb(67, 71, 76),
    line: C::from_rgb(45, 47, 50),
    input_line: C::from_rgb(52, 55, 59),
    strong: C::from_rgb(106, 109, 114),
    hi: C::from_rgba_premultiplied(18, 18, 18, 18),
    low: C::from_black_alpha(153),
    shadow: C::from_black_alpha(204),
    text: C::from_rgb(233, 234, 235),
    muted: C::from_rgb(163, 166, 171),
    faint: C::from_rgb(160, 163, 168),
    focus: C::from_rgb(233, 234, 235),
    brand: C::from_rgb(240, 88, 76),
    // White on it: 4.58:1 (was 4.24 at 229, 57, 43).
    brand_top: C::from_rgb(222, 50, 37),
    brand_fill: C::from_rgb(214, 52, 38),
    brand_glow: C::from_rgb(240, 78, 63),
    brand_edge: C::from_rgb(110, 20, 13),
    brand_soft: C::from_rgb(43, 26, 24),
    on_brand: C::from_rgb(255, 255, 255),
    ok: C::from_rgb(95, 207, 143),
    wait: C::from_rgb(229, 182, 78),
    wait_soft: C::from_rgb(42, 35, 22),
    bad: C::from_rgb(242, 118, 107),
    bad_soft: C::from_rgb(46, 27, 25),
    enc_blue: C::from_rgb(92, 154, 255),
    enc_green: C::from_rgb(95, 207, 143),
    enc_white: C::from_rgb(214, 216, 219),
    enc_orange: C::from_rgb(255, 138, 61),
    lcd_on: C::from_rgb(233, 234, 235),
    lcd_off: C::from_rgb(27, 29, 31),
    key_white: C::from_rgb(196, 199, 203),
    key_white_edge: C::from_rgb(150, 153, 158),
    key_black: C::from_rgb(14, 15, 16),
    key_ink: C::from_rgb(40, 42, 45),
    row_black: C::from_rgb(16, 17, 18),
    row_white: C::from_rgb(22, 23, 25),
    row_root: C::from_rgb(40, 33, 31),
    row_out: C::from_rgb(11, 12, 13),
};

/// Light: the same hierarchy as the dark one, a cool light grey (OP-1
/// field), depth carried by the edge, one darker red; all text ≥ 4.5:1.
pub static LIGHT: Palette = Palette {
    edge: C::from_rgb(0x9A, 0xA1, 0xA9),
    sunken: C::from_rgb(0xD6, 0xDA, 0xDE),
    side: C::from_rgb(0xE1, 0xE4, 0xE8),
    bg: C::from_rgb(0xE8, 0xEA, 0xED),
    surface: C::from_rgb(0xF0, 0xF2, 0xF4),
    panel_bottom: C::from_rgb(0xF2, 0xF4, 0xF6),
    panel_top: C::from_rgb(0xF8, 0xF9, 0xFB),
    raised: C::from_rgb(0xEF, 0xF1, 0xF3),
    selection: C::from_rgb(0xCF, 0xD5, 0xDC),
    ctl_bottom: C::from_rgb(0xED, 0xEF, 0xF2),
    ctl_top: C::from_rgb(0xFA, 0xFB, 0xFC),
    ctl_hover: C::from_rgb(0xFF, 0xFF, 0xFF),
    line: C::from_rgb(0xC6, 0xCB, 0xD1),
    input_line: C::from_rgb(0xAD, 0xB4, 0xBB),
    strong: C::from_rgb(0x6B, 0x72, 0x7A),
    hi: C::from_rgba_premultiplied(178, 178, 178, 178),
    low: C::from_black_alpha(31),
    shadow: C::from_black_alpha(61),
    text: C::from_rgb(0x16, 0x18, 0x1B),
    muted: C::from_rgb(0x4A, 0x50, 0x58),
    faint: C::from_rgb(0x56, 0x5D, 0x65),
    focus: C::from_rgb(0x16, 0x18, 0x1B),
    brand: C::from_rgb(0xBE, 0x2F, 0x1E),
    brand_top: C::from_rgb(0xC6, 0x3A, 0x28),
    brand_fill: C::from_rgb(0xB2, 0x2B, 0x1C),
    brand_glow: C::from_rgb(0xD2, 0x43, 0x2C),
    brand_edge: C::from_rgb(0x7A, 0x19, 0x11),
    brand_soft: C::from_rgb(0xF7, 0xE3, 0xDF),
    on_brand: C::from_rgb(255, 255, 255),
    ok: C::from_rgb(0x17, 0x78, 0x4A),
    wait: C::from_rgb(0x8A, 0x5A, 0x0E),
    wait_soft: C::from_rgb(0xFB, 0xF0, 0xD9),
    bad: C::from_rgb(0xB0, 0x2A, 0x20),
    bad_soft: C::from_rgb(0xFA, 0xE5, 0xE2),
    enc_blue: C::from_rgb(0x2F, 0x6F, 0xD0),
    enc_green: C::from_rgb(0x17, 0x78, 0x4A),
    enc_white: C::from_rgb(0x6E, 0x75, 0x7D),
    enc_orange: C::from_rgb(0xC2, 0x5E, 0x12),
    lcd_on: C::from_rgb(0x1A, 0x1D, 0x21),
    lcd_off: C::from_rgb(0xC2, 0xC7, 0xCD),
    key_white: C::from_rgb(0xFA, 0xFA, 0xF8),
    key_white_edge: C::from_rgb(0xC9, 0xCB, 0xC6),
    key_black: C::from_rgb(0x1B, 0x1D, 0x20),
    key_ink: C::from_rgb(0x3A, 0x3D, 0x42),
    row_black: C::from_rgb(0xE5, 0xE8, 0xEB),
    row_white: C::from_rgb(0xF4, 0xF5, 0xF7),
    row_root: C::from_rgb(0xEF, 0xE6, 0xE2),
    row_out: C::from_rgb(0xDE, 0xE1, 0xE4),
};

/// Which palette is in use (the interface's theme).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

static ACTIVE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The palette in use.
#[inline]
pub fn p() -> &'static Palette {
    if ACTIVE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        &LIGHT
    } else {
        &DARK
    }
}

pub fn theme() -> Theme {
    if ACTIVE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        Theme::Light
    } else {
        Theme::Dark
    }
}

fn set_theme(theme: Theme) {
    ACTIVE.store(
        (theme == Theme::Light) as u8,
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// The desktop's light/dark preference (xdg-desktop-portal on Linux, Registry
/// `AppsUseLightTheme` on Windows). Asked in the background at most every 30 s;
/// `None` until known or when the desktop has no preference.
pub fn system_theme() -> Option<Theme> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static STATE: Mutex<(Option<Instant>, Option<Theme>)> = Mutex::new((None, None));
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if st.0.is_none_or(|at| at.elapsed() > Duration::from_secs(30)) {
        st.0 = Some(Instant::now());
        std::thread::spawn(|| {
            let found = crate::platform::query_system_theme();
            STATE.lock().unwrap_or_else(|e| e.into_inner()).1 = found;
        });
    }
    st.1
}

/// gdbus prints `(<uint32 1>,)`: 1 dark, 2 light, 0 no preference.
#[allow(dead_code)]
pub(crate) fn parse_color_scheme(s: &str) -> Option<Theme> {
    match s.split("uint32 ").nth(1)?.chars().next()? {
        '1' => Some(Theme::Dark),
        '2' => Some(Theme::Light),
        _ => None,
    }
}

/// The theme for a setting: `KFXL_THEME=dark|light|system` first (captures),
/// then the setting; System falls back to dark.
pub fn resolve(mode: crate::settings::ThemeMode) -> Theme {
    use crate::settings::ThemeMode;
    let mode = match std::env::var("KFXL_THEME").as_deref() {
        Ok("light") => ThemeMode::Light,
        Ok("dark") => ThemeMode::Dark,
        Ok("system") => ThemeMode::System,
        _ => mode,
    };
    match mode {
        ThemeMode::Dark => Theme::Dark,
        ThemeMode::Light => Theme::Light,
        ThemeMode::System => system_theme().unwrap_or(Theme::Dark),
    }
}

/// Faceplates (Tide, Sampler) are hardware: dark in both themes. While the
/// guard lives, the palette and the `Ui`'s visuals are the dark ones.
pub struct Faceplate(u8);

impl Faceplate {
    pub fn enter(ui: &mut egui::Ui) -> Self {
        let was = ACTIVE.swap(0, std::sync::atomic::Ordering::Relaxed);
        if was != 0 {
            let ctx = ui.ctx().clone();
            ui.style_mut().visuals = style(&ctx, Theme::Dark).visuals;
        }
        Self(was)
    }
}

impl Drop for Faceplate {
    fn drop(&mut self) {
        ACTIVE.store(self.0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[inline]
pub fn edge() -> C {
    p().edge
}
#[inline]
pub fn sunken() -> C {
    p().sunken
}
#[inline]
pub fn side() -> C {
    p().side
}
#[inline]
pub fn bg() -> C {
    p().bg
}
#[inline]
pub fn surface() -> C {
    p().surface
}
#[inline]
pub fn panel_bottom() -> C {
    p().panel_bottom
}
#[inline]
pub fn panel_top() -> C {
    p().panel_top
}
#[inline]
pub fn raised() -> C {
    p().raised
}
#[inline]
pub fn selection() -> C {
    p().selection
}
#[inline]
pub fn ctl_bottom() -> C {
    p().ctl_bottom
}
#[inline]
pub fn ctl_top() -> C {
    p().ctl_top
}
#[inline]
pub fn ctl_hover() -> C {
    p().ctl_hover
}
#[inline]
pub fn line() -> C {
    p().line
}
#[inline]
#[allow(dead_code)]
pub fn input_line() -> C {
    p().input_line
}
#[inline]
pub fn strong() -> C {
    p().strong
}
#[inline]
pub fn hi() -> C {
    p().hi
}
#[inline]
pub fn low() -> C {
    p().low
}
#[inline]
pub fn shadow() -> C {
    p().shadow
}
#[inline]
pub fn text() -> C {
    p().text
}
#[inline]
pub fn muted() -> C {
    p().muted
}
#[inline]
pub fn faint() -> C {
    p().faint
}
#[inline]
pub fn focus() -> C {
    p().focus
}
#[inline]
pub fn brand() -> C {
    p().brand
}
#[inline]
pub fn brand_top() -> C {
    p().brand_top
}
#[inline]
pub fn brand_fill() -> C {
    p().brand_fill
}
#[inline]
pub fn brand_glow() -> C {
    p().brand_glow
}
#[inline]
pub fn brand_edge() -> C {
    p().brand_edge
}
#[inline]
pub fn brand_soft() -> C {
    p().brand_soft
}
#[inline]
pub fn on_brand() -> C {
    p().on_brand
}
#[inline]
pub fn ok() -> C {
    p().ok
}
#[inline]
pub fn wait() -> C {
    p().wait
}
#[inline]
pub fn wait_soft() -> C {
    p().wait_soft
}
#[inline]
pub fn bad() -> C {
    p().bad
}
#[inline]
#[allow(dead_code)]
pub fn bad_soft() -> C {
    p().bad_soft
}
#[inline]
pub fn enc_blue() -> C {
    p().enc_blue
}
#[inline]
pub fn enc_green() -> C {
    p().enc_green
}
#[inline]
pub fn enc_white() -> C {
    p().enc_white
}
#[inline]
pub fn enc_orange() -> C {
    p().enc_orange
}
#[inline]
pub fn lcd_on() -> C {
    p().lcd_on
}
#[inline]
pub fn lcd_off() -> C {
    p().lcd_off
}
#[inline]
pub fn key_white() -> C {
    p().key_white
}
#[inline]
pub fn key_white_edge() -> C {
    p().key_white_edge
}
#[inline]
pub fn key_black() -> C {
    p().key_black
}
#[inline]
pub fn key_ink() -> C {
    p().key_ink
}
#[inline]
pub fn row_black() -> C {
    p().row_black
}
#[inline]
pub fn row_white() -> C {
    p().row_white
}
#[inline]
pub fn row_root() -> C {
    p().row_root
}
#[inline]
pub fn row_out() -> C {
    p().row_out
}

/// Pattern address colours, assigned by pattern id.
pub fn pattern_codes() -> [C; 5] {
    [enc_white(), enc_blue(), enc_green(), wait(), enc_orange()]
}
pub fn pattern_code(id: u64) -> C {
    let codes = pattern_codes();
    codes[(id.saturating_sub(1) as usize) % codes.len()]
}

/// A user-picked colour as 0xRRGGBB.
pub fn color_from_u32(v: u32) -> C {
    C::from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    )
}

pub fn u32_from_color(c: C) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | (c.b() as u32)
}

/// Pattern colour: the user's pick, else the address colour by id.
pub fn pattern_color(id: u64, custom: Option<u32>) -> C {
    custom
        .map(color_from_u32)
        .unwrap_or_else(|| pattern_code(id))
}

// Secondary hues shared by note colours and the track/pattern palette.
pub const HUE_PINK: C = C::from_rgb(218, 110, 153);
pub const HUE_PURPLE: C = C::from_rgb(158, 111, 190);
pub const HUE_CYAN: C = C::from_rgb(91, 188, 198);
pub const HUE_LIME: C = C::from_rgb(161, 190, 91);
pub const HUE_GOLD: C = C::from_rgb(201, 164, 83);
pub const HUE_BROWN: C = C::from_rgb(159, 116, 86);
pub const HUE_ROSE: C = C::from_rgb(194, 126, 123);
pub const HUE_TEAL: C = C::from_rgb(78, 155, 145);
pub const HUE_INDIGO: C = C::from_rgb(105, 112, 185);
pub const HUE_GREY: C = C::from_rgb(145, 150, 155);

/// Palette offered for tracks and patterns (FL colour names reuse the note names).
pub fn color_palette() -> [C; 10] {
    [
        enc_white(),
        enc_blue(),
        enc_green(),
        wait(),
        enc_orange(),
        HUE_PINK,
        HUE_PURPLE,
        HUE_CYAN,
        HUE_LIME,
        HUE_GOLD,
    ]
}

// KeyFrame Tide faceplate tokens (abyssal slate + tidal cyan oscilloscope).
pub const TIDE_CHASSIS: C = C::from_rgb(18, 23, 28);
pub const TIDE_DECK: C = C::from_rgb(25, 32, 39);
pub const TIDE_DECK_HI: C = C::from_rgb(33, 42, 51);
pub const TIDE_WELL: C = C::from_rgb(9, 13, 18);
pub const TIDE_LINE: C = C::from_rgb(42, 54, 66);
pub const TIDE_CYAN: C = C::from_rgb(78, 205, 235);
pub const TIDE_DEEP: C = C::from_rgb(32, 102, 128);
pub const TIDE_CORAL: C = C::from_rgb(245, 146, 86);
pub const TIDE_MINT: C = C::from_rgb(94, 214, 162);
pub const TIDE_MIST: C = C::from_rgb(158, 177, 194);

// DWP Sampler faceplate tokens (warm studio charcoal + brass/tape splice).
pub const SMP_CHASSIS: C = C::from_rgb(24, 22, 20);
pub const SMP_DECK: C = C::from_rgb(33, 30, 27);
pub const SMP_DECK_HI: C = C::from_rgb(44, 40, 36);
pub const SMP_WELL: C = C::from_rgb(14, 12, 10);
pub const SMP_LINE: C = C::from_rgb(56, 51, 45);
pub const SMP_GOLD: C = C::from_rgb(235, 182, 78);
pub const SMP_LOOP: C = C::from_rgb(232, 114, 82);
pub const SMP_START: C = C::from_rgb(104, 208, 151);
pub const SMP_FILTER2: C = C::from_rgb(104, 174, 240);
pub const SMP_SAND: C = C::from_rgb(180, 170, 158);

// Type: IBM Plex only. 400/500/600, no 700, no italics.
pub fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("plex-medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("plex-semibold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
#[allow(dead_code)]
pub fn mono_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("plex-mono-medium".into()))
}

/// The interface fonts. Kana and kanji (lyrics, voicebank names) come from a
/// system font when given: bundling a CJK face would add many megabytes.
pub fn font_definitions(cjk: Option<(Vec<u8>, u32)>) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let files: [(&str, &'static [u8]); 5] = [
        (
            "plex",
            include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
        ),
        (
            "plex-medium",
            include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
        ),
        (
            "plex-semibold",
            include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
        ),
        (
            "plex-mono",
            include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
        ),
        (
            "plex-mono-medium",
            include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
        ),
    ];
    for (name, bytes) in files {
        fonts
            .font_data
            .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    }
    for name in ["plex-medium", "plex-semibold", "plex-mono-medium"] {
        // Fall back to the regular face for glyphs a weight lacks.
        let base = if name.contains("mono") {
            "plex-mono"
        } else {
            "plex"
        };
        fonts.families.insert(
            FontFamily::Name(name.into()),
            vec![name.into(), base.into()],
        );
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("proportional family")
        .insert(0, "plex".into());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .expect("monospace family")
        .insert(0, "plex-mono".into());

    if let Some((bytes, index)) = cjk {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        fonts.font_data.insert("cjk".into(), Arc::new(data));
        for family in fonts.families.values_mut() {
            family.push("cjk".into());
        }
    }
    fonts
}

/// A system font that has kana (fontconfig on Linux, `%WINDIR%\Fonts` on Windows):
/// (file bytes, face index in a collection).
pub fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    crate::platform::find_cjk_font()
}

pub fn setup(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions(None));
    apply(ctx, theme());
}

/// Switch the interface to `theme`: the palette, and egui's style for it
/// (at the start of a frame, before any panel draws).
pub fn apply(ctx: &egui::Context, theme: Theme) {
    set_theme(theme);
    let egui_theme = match theme {
        Theme::Dark => egui::Theme::Dark,
        Theme::Light => egui::Theme::Light,
    };
    ctx.set_theme(egui_theme);
    ctx.set_style_of(egui_theme, style(ctx, theme));
}

fn style(ctx: &egui::Context, theme: Theme) -> egui::Style {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    *v = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    v.panel_fill = bg();
    v.window_fill = panel_top();
    v.window_stroke = Stroke::new(1.5_f32, edge());
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 10.into();
    v.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: shadow(),
    };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = sunken();
    v.faint_bg_color = surface();
    v.override_text_color = None;
    v.selection.bg_fill = selection();
    v.selection.stroke = Stroke::new(1.0_f32, text());
    v.hyperlink_color = enc_blue();
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 7.into();
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = panel_top();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, line());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, muted());
    v.widgets.inactive.bg_fill = ctl_bottom();
    v.widgets.inactive.weak_bg_fill = ctl_bottom();
    v.widgets.inactive.bg_stroke = Stroke::new(1.5_f32, edge());
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text());
    v.widgets.hovered.bg_fill = ctl_top();
    v.widgets.hovered.weak_bg_fill = ctl_top();
    v.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, edge());
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, text());
    v.widgets.active.bg_fill = ctl_bottom();
    v.widgets.active.weak_bg_fill = ctl_bottom();
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, edge());
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, text());
    v.widgets.open = v.widgets.hovered;

    // Standard density: 26 px controls, menus and popups as compact lists
    // (FL's menus are text rows, not stacks of big buttons).
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.menu_margin = 4.into();
    style.spacing.combo_height = 360.0;
    use egui::TextStyle as S;
    style.text_styles.insert(S::Body, sans(14.0));
    style.text_styles.insert(S::Button, sans(14.0));
    style.text_styles.insert(S::Small, sans(12.0));
    style.text_styles.insert(S::Monospace, mono(13.0));
    style.text_styles.insert(S::Heading, semibold(20.0));
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance contrast.
    fn contrast(a: C, b: C) -> f32 {
        let lum = |c: C| {
            let ch = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_reads_in_both_themes() {
        for p in [&DARK, &LIGHT] {
            for (ink, name) in [(p.text, "text"), (p.muted, "muted"), (p.faint, "faint")] {
                for (bg, where_) in [(p.bg, "bg"), (p.surface, "surface"), (p.panel_top, "panel")] {
                    let c = contrast(ink, bg);
                    assert!(c >= 4.5, "{name} on {where_}: {c:.2}");
                }
            }
            assert!(contrast(p.on_brand, p.brand_top) >= 4.5);
            assert!(contrast(p.on_brand, p.brand_fill) >= 4.5);
            assert!(contrast(p.brand, p.bg) >= 4.5);
            assert!(contrast(p.bad, p.bg) >= 4.5);
        }
    }

    #[test]
    fn portal_answers_are_read() {
        assert_eq!(parse_color_scheme("(<uint32 1>,)\n"), Some(Theme::Dark));
        assert_eq!(parse_color_scheme("(<uint32 2>,)"), Some(Theme::Light));
        assert_eq!(parse_color_scheme("(<uint32 0>,)"), None);
        assert_eq!(parse_color_scheme(""), None);
    }
}
