//! Tokens: BMS dark (bms-hub-design-kit tokens.json) + the Teenage Engineering
//! colour code for encoders and pattern addresses. Nothing outside this file
//! may name a colour or a font file.

use eframe::egui::{self, Color32 as C, FontFamily, FontId, Stroke, Vec2};
use std::sync::Arc;

// Surfaces, darkest first.
pub const EDGE: C = C::from_rgb(4, 5, 5);
pub const SUNKEN: C = C::from_rgb(12, 13, 14);
pub const SIDE: C = C::from_rgb(15, 16, 17);
pub const BG: C = C::from_rgb(20, 21, 22);
pub const SURFACE: C = C::from_rgb(26, 27, 29);
pub const PANEL_BOTTOM: C = C::from_rgb(32, 34, 37);
pub const PANEL_TOP: C = C::from_rgb(34, 36, 39);
pub const RAISED: C = C::from_rgb(34, 35, 38);
pub const SELECTION: C = C::from_rgb(42, 44, 47);
pub const CTL_BOTTOM: C = C::from_rgb(43, 45, 48);
pub const CTL_TOP: C = C::from_rgb(50, 52, 56);
pub const CTL_HOVER: C = C::from_rgb(67, 71, 76);

// Lines.
pub const LINE: C = C::from_rgb(45, 47, 50);
#[allow(dead_code)] // reserved token
pub const INPUT_LINE: C = C::from_rgb(52, 55, 59);
pub const STRONG: C = C::from_rgb(106, 109, 114);
/// White 12 %, black 60 %, black 80 % (premultiplied).
pub const HI: C = C::from_rgba_premultiplied(18, 18, 18, 18);
pub const LOW: C = C::from_black_alpha(153);
pub const SHADOW: C = C::from_black_alpha(204);

// Ink.
pub const TEXT: C = C::from_rgb(233, 234, 235);
pub const MUTED: C = C::from_rgb(163, 166, 171);
pub const FAINT: C = C::from_rgb(160, 163, 168);
pub const FOCUS: C = TEXT;

// Brand: the one hero colour (Play, an enabled step, an own note).
pub const BRAND: C = C::from_rgb(240, 88, 76);
pub const BRAND_TOP: C = C::from_rgb(229, 57, 43);
pub const BRAND_FILL: C = C::from_rgb(214, 52, 38);
pub const BRAND_GLOW: C = C::from_rgb(240, 78, 63);
pub const BRAND_EDGE: C = C::from_rgb(110, 20, 13);
pub const BRAND_SOFT: C = C::from_rgb(43, 26, 24);
pub const ON_BRAND: C = C::from_rgb(255, 255, 255);

// Semantic.
pub const OK: C = C::from_rgb(95, 207, 143);
pub const WAIT: C = C::from_rgb(229, 182, 78);
pub const WAIT_SOFT: C = C::from_rgb(42, 35, 22);
pub const BAD: C = C::from_rgb(242, 118, 107);
#[allow(dead_code)] // reserved token
pub const BAD_SOFT: C = C::from_rgb(46, 27, 25);

// TE colour code: four encoders, each colour is an address, never a status.
pub const ENC_BLUE: C = C::from_rgb(92, 154, 255);
pub const ENC_GREEN: C = C::from_rgb(95, 207, 143);
pub const ENC_WHITE: C = C::from_rgb(214, 216, 219);
#[allow(dead_code)] // reserved token
pub const ENC_ORANGE: C = C::from_rgb(255, 138, 61);

/// Pattern address colours, assigned by pattern id.
pub const PATTERN_CODES: [C; 5] = [ENC_WHITE, ENC_BLUE, ENC_GREEN, WAIT, ENC_ORANGE];
pub fn pattern_code(id: u64) -> C {
    PATTERN_CODES[(id.saturating_sub(1) as usize) % PATTERN_CODES.len()]
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
pub const COLOR_PALETTE: [C; 10] = [
    ENC_WHITE, ENC_BLUE, ENC_GREEN, WAIT, ENC_ORANGE, HUE_PINK, HUE_PURPLE, HUE_CYAN, HUE_LIME,
    HUE_GOLD,
];

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

// Segment display ink.
pub const LCD_ON: C = TEXT;
pub const LCD_OFF: C = C::from_rgb(27, 29, 31);

// Piano keys.
pub const KEY_WHITE: C = C::from_rgb(196, 199, 203);
pub const KEY_WHITE_EDGE: C = C::from_rgb(150, 153, 158);
pub const KEY_BLACK: C = C::from_rgb(14, 15, 16);
pub const KEY_INK: C = C::from_rgb(40, 42, 45);
pub const ROW_BLACK: C = C::from_rgb(16, 17, 18);
pub const ROW_WHITE: C = C::from_rgb(22, 23, 25);
/// Piano roll with a scale on: the root rows, and rows outside the scale.
pub const ROW_ROOT: C = C::from_rgb(40, 33, 31);
pub const ROW_OUT: C = C::from_rgb(11, 12, 13);

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

/// A system font that has kana, found through fontconfig (`fc-match`):
/// (file bytes, face index in a collection).
pub fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", "sans:lang=ja"])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines();
    let file = lines.next()?.trim();
    let index = lines
        .next()
        .and_then(|i| i.trim().parse().ok())
        .unwrap_or(0);
    let bytes = std::fs::read(file).ok()?;
    (bytes.len() > 1000).then_some((bytes, index))
}

pub fn setup(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions(None));
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL_TOP;
    v.window_stroke = Stroke::new(1.5_f32, EDGE);
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 10.into();
    v.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: SHADOW,
    };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = SUNKEN;
    v.faint_bg_color = SURFACE;
    v.override_text_color = None;
    v.selection.bg_fill = SELECTION;
    v.selection.stroke = Stroke::new(1.0_f32, TEXT);
    v.hyperlink_color = ENC_BLUE;
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
    v.widgets.noninteractive.bg_fill = PANEL_TOP;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    v.widgets.inactive.bg_fill = CTL_BOTTOM;
    v.widgets.inactive.weak_bg_fill = CTL_BOTTOM;
    v.widgets.inactive.bg_stroke = Stroke::new(1.5_f32, EDGE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.bg_fill = CTL_TOP;
    v.widgets.hovered.weak_bg_fill = CTL_TOP;
    v.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, EDGE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.active.bg_fill = CTL_BOTTOM;
    v.widgets.active.weak_bg_fill = CTL_BOTTOM;
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, EDGE);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
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
    ctx.set_style(style);
}
