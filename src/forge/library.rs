//! The recipe catalogue past the first eleven (`Recipe::Lib`): game sounds by
//! category, from docs/research/0.2.0/C3.md. Each has a target peak for its
//! category, so a set exported "as made" keeps a UI click quieter than a
//! shotgun; and recipes that a style's extra layers would spoil (UI ticks,
//! a heartbeat) take only the light part of a style.

use super::{
    Curve, CurvePreset, FilterMode, ForgePatch, Layer, Macros, ModalMaterial, NoiseColor, Source,
    Wave,
};
use crate::fx::{FxKind, FxSlot};

/// What a recipe is for (menus group by it; export levels follow it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Ui,
    Steps,
    Actions,
    Weapons,
    Pickups,
    Magic,
    Damage,
    Doors,
    Ambience,
    Creatures,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Ui,
        Category::Steps,
        Category::Actions,
        Category::Weapons,
        Category::Pickups,
        Category::Magic,
        Category::Damage,
        Category::Doors,
        Category::Ambience,
        Category::Creatures,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Ui => "UI",
            Category::Steps => "Footsteps",
            Category::Actions => "Actions",
            Category::Weapons => "Weapons & combat",
            Category::Pickups => "Pickups",
            Category::Magic => "Magic",
            Category::Damage => "Damage",
            Category::Doors => "Doors & mechanisms",
            Category::Ambience => "Ambience",
            Category::Creatures => "Creatures & sci-fi",
        }
    }

    /// The peak a recipe of this category is made at: interface sounds sit
    /// well under hits and explosions, as in a game's mix.
    pub fn peak(self) -> f32 {
        match self {
            Category::Ui => 0.35,
            Category::Steps => 0.45,
            Category::Ambience => 0.45,
            Category::Pickups => 0.55,
            Category::Actions => 0.6,
            Category::Magic => 0.7,
            Category::Damage => 0.7,
            Category::Doors => 0.7,
            Category::Creatures => 0.7,
            Category::Weapons => 0.89,
        }
    }
}

pub struct RecipeDef {
    pub name: &'static str,
    pub stem: &'static str,
    pub category: Category,
    /// Styles add only their light part (waves, macros), no extra layers or
    /// effects: a UI tick must not grow a sub, a heartbeat no vibrato.
    pub light_style: bool,
    pub build: fn() -> ForgePatch,
}

const fn def(
    name: &'static str,
    stem: &'static str,
    category: Category,
    light_style: bool,
    build: fn() -> ForgePatch,
) -> RecipeDef {
    RecipeDef {
        name,
        stem,
        category,
        light_style,
        build,
    }
}

use Category::*;

pub const RECIPES: [RecipeDef; 40] = [
    def("Hover", "ui_hover", Ui, true, ui_hover),
    def("Confirm", "ui_confirm", Ui, true, ui_confirm),
    def("Back", "ui_back", Ui, true, ui_back),
    def("Error", "ui_error", Ui, true, ui_error),
    def("Toggle", "ui_toggle", Ui, true, ui_toggle),
    def("Step dirt", "step_dirt", Steps, false, step_dirt),
    def("Step grass", "step_grass", Steps, false, step_grass),
    def("Step gravel", "step_gravel", Steps, false, step_gravel),
    def("Step wood", "step_wood", Steps, false, step_wood),
    def("Step metal", "step_metal", Steps, false, step_metal),
    def(
        "Step concrete",
        "step_concrete",
        Steps,
        false,
        step_concrete,
    ),
    def("Step snow", "step_snow", Steps, false, step_snow),
    def("Swing", "swing", Weapons, false, swing),
    def("Clash", "clash", Weapons, false, clash),
    def("Pistol", "pistol", Weapons, false, pistol),
    def("Shotgun", "shotgun", Weapons, false, shotgun),
    def("Reload", "reload", Weapons, false, reload),
    def("Charge", "charge", Magic, false, charge),
    def("Fireball", "fireball", Magic, false, fireball),
    def("Ice", "ice", Magic, false, ice),
    def("Heal", "heal", Magic, false, heal),
    def("Curse", "curse", Magic, false, curse),
    def("Gem", "gem", Pickups, false, gem),
    def("Health", "health", Pickups, false, health),
    def("Level up", "level_up", Pickups, false, level_up),
    def("Hurt", "hurt", Damage, false, hurt),
    def("Death", "death", Damage, false, death),
    def("Heartbeat", "heartbeat", Damage, true, heartbeat),
    def("Door creak", "door_creak", Doors, false, door_creak),
    def("Door slam", "door_slam", Doors, false, door_slam),
    def("Gate", "gate", Doors, false, gate),
    def("Lever", "lever", Doors, false, lever),
    def("Wind", "wind", Ambience, true, wind),
    def("Rain", "rain", Ambience, true, rain),
    def("Fire", "fire", Ambience, false, fire),
    def("Cave", "cave", Ambience, false, cave),
    def("Machine hum", "machine_hum", Ambience, false, machine_hum),
    def("Growl", "growl", Creatures, false, growl),
    def("Slime", "slime", Creatures, false, slime),
    def("Siren", "siren", Creatures, false, siren),
];

// ---- Notation helpers (C3.md §3.1).

fn o(name: &str, wave: Wave, hz: f32, len: f32) -> Layer {
    Layer::new(name, Source::osc(wave, hz), len)
}
fn fm(
    name: &str,
    wave: Wave,
    hz: f32,
    fm_ratio: f32,
    fm_index: f32,
    unison: u8,
    detune: f32,
    len: f32,
) -> Layer {
    Layer::new(
        name,
        Source::Osc {
            wave,
            hz,
            vibrato_hz: 0.0,
            vibrato: 0.0,
            fm_ratio,
            fm_index,
            unison,
            detune,
            shape: 0.0,
        },
        len,
    )
}
fn m(
    name: &str,
    material: ModalMaterial,
    hz: f32,
    damping: f32,
    inharmonicity: f32,
    strike: f32,
    len: f32,
) -> Layer {
    Layer::new(
        name,
        Source::Modal {
            material,
            hz,
            damping,
            inharmonicity,
            strike,
        },
        len,
    )
    .with_amp(Curve::flat(1.0))
}
fn gr(
    name: &str,
    hz: f32,
    density: f32,
    grain_ms: f32,
    scatter: f32,
    spread: f32,
    spray: f32,
    len: f32,
) -> Layer {
    Layer::new(
        name,
        Source::Grain {
            hz,
            density,
            grain_ms,
            scatter,
            spread,
            spray,
            sample: None,
        },
        len,
    )
    .with_amp(Curve::flat(1.0))
}
fn n(name: &str, color: NoiseColor, hz: f32, len: f32) -> Layer {
    Layer::new(name, Source::noise(color, hz), len)
}
fn ns(name: &str, color: NoiseColor, hz: f32, width: f32, burst: f32, len: f32) -> Layer {
    Layer::new(
        name,
        Source::Noise {
            color,
            hz,
            stereo_width: width,
            burst,
        },
        len,
    )
}
/// A curve from `(t, value, tension)` points.
fn c(points: &[(f32, f32, f32)]) -> Curve {
    Curve::new(points)
}
fn dec(k: f32) -> Curve {
    c(&[(0.0, 1.0, k), (1.0, 0.0, 0.0)])
}
fn flat(v: f32) -> Curve {
    Curve::flat(v)
}
/// Evenly spaced straight segments through `values` (a "→" filter sweep).
fn sweep(values: &[f32]) -> Curve {
    let last = (values.len().max(2) - 1) as f32;
    let pts: Vec<(f32, f32, f32)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f32 / last, *v, 0.0))
        .collect();
    c(&pts)
}

trait L {
    fn g(self, gain: f32) -> Self;
    fn st(self, start: f32) -> Self;
    fn pan(self, pan: f32) -> Self;
    fn p(self, curve: Curve) -> Self;
    fn a(self, curve: Curve) -> Self;
    fn f(self, mode: FilterMode, cutoff: Curve, res: f32) -> Self;
}

impl L for Layer {
    fn g(self, gain: f32) -> Self {
        self.with_gain(gain)
    }
    fn st(self, start: f32) -> Self {
        self.with_start(start)
    }
    fn pan(self, pan: f32) -> Self {
        self.with_pan(pan)
    }
    fn p(self, curve: Curve) -> Self {
        self.with_pitch(curve)
    }
    fn a(self, curve: Curve) -> Self {
        self.with_amp(curve)
    }
    fn f(self, mode: FilterMode, cutoff: Curve, res: f32) -> Self {
        self.with_filter(mode, cutoff, res)
    }
}

use FilterMode::{BandPass as BP, HighPass as HP, LowPass as LP};
use NoiseColor::{Brown as Br, Crackle as Cr, Pink as Pk, White as Wh};
use Wave::{Pulse25 as P25, Saw, Sine as Sin, Square as Sq, Triangle as Tri};

fn fx(kind: FxKind, set: &[(usize, f32)]) -> FxSlot {
    let mut s = FxSlot::new(kind);
    for &(i, v) in set {
        s.params[i] = v;
    }
    s
}
fn dly(steps: f32, fb: f32, wet: f32) -> FxSlot {
    fx(FxKind::Delay, &[(0, steps), (1, fb), (4, wet)])
}
fn rev(size: f32, damp: f32, wet: f32) -> FxSlot {
    fx(FxKind::Reverb, &[(0, size), (1, damp), (4, wet)])
}
fn drv(db: f32, tone: f32, out: f32, mix: f32) -> FxSlot {
    fx(FxKind::Drive, &[(0, db), (1, tone), (2, out), (3, mix)])
}
fn cho(depth: f32, rate: f32, mix: f32) -> FxSlot {
    fx(FxKind::Chorus, &[(1, depth), (2, rate), (3, mix)])
}
fn pha(rate: f32, depth: f32, fb: f32) -> FxSlot {
    fx(FxKind::Phaser, &[(0, rate), (1, depth), (2, fb)])
}
fn ring(hz: f32, mix: f32) -> FxSlot {
    fx(FxKind::RingMod, &[(0, hz), (1, mix)])
}
fn comp(thr: f32, ratio: f32, atk: f32, rel: f32) -> FxSlot {
    fx(
        FxKind::Compressor,
        &[(0, thr), (1, ratio), (2, atk), (3, rel)],
    )
}
fn trans(atk: f32, sus: f32) -> FxSlot {
    fx(FxKind::Transient, &[(0, atk), (1, sus)])
}
fn trem(hz: f32, depth: f32) -> FxSlot {
    fx(FxKind::Tremolo, &[(0, hz), (1, depth)])
}

fn sound(layers: Vec<Layer>, fx: Vec<FxSlot>, m: (f32, f32, f32, f32), seed: u64) -> ForgePatch {
    ForgePatch {
        layers,
        fx,
        macros: Macros {
            size: m.0,
            energy: m.1,
            dirt: m.2,
            space: m.3,
        },
        seed,
        ..ForgePatch::blank()
    }
}

fn looped(layers: Vec<Layer>, fx: Vec<FxSlot>, m: (f32, f32, f32, f32), seed: u64) -> ForgePatch {
    ForgePatch {
        seamless_loop: true,
        ..sound(layers, fx, m, seed)
    }
}

// ---- UI

fn ui_hover() -> ForgePatch {
    sound(
        vec![
            m("Tick", ModalMaterial::Glass, 2200.0, 0.85, 0.15, 0.65, 0.04).g(0.65),
            ns("Air", Wh, 8000.0, 0.35, 0.0, 0.014)
                .g(0.18)
                .f(HP, flat(0.82), 0.1),
        ],
        vec![],
        (0.45, 0.4, 0.0, 0.0),
        11,
    )
}
fn ui_confirm() -> ForgePatch {
    sound(
        vec![
            m("Chime", ModalMaterial::Glass, 880.0, 0.55, 0.1, 0.55, 0.14).g(0.75),
            m("Ring", ModalMaterial::Glass, 1320.0, 0.45, 0.12, 0.65, 0.22)
                .st(0.055)
                .g(0.8),
        ],
        vec![],
        (0.5, 0.5, 0.0, 0.08),
        12,
    )
}
fn ui_back() -> ForgePatch {
    sound(
        vec![
            m("Tap", ModalMaterial::Wood, 640.0, 0.65, 0.2, 0.55, 0.1).g(0.8),
            o("Drop", Tri, 440.0, 0.12)
                .st(0.06)
                .g(0.65)
                .p(c(&[(0.0, 0.0, 0.0), (1.0, -7.0, 0.0)]))
                .a(dec(-0.3)),
        ],
        vec![],
        (0.5, 0.45, 0.0, 0.06),
        13,
    )
}
fn ui_error() -> ForgePatch {
    let env = || c(&[(0.0, 0.9, 0.0), (0.15, 1.0, 0.0), (1.0, 0.0, 0.0)]);
    sound(
        vec![
            fm("Buzz", Saw, 165.0, 1.41, 1.2, 2, 0.35, 0.22)
                .g(0.85)
                .a(env())
                .f(LP, flat(0.55), 0.35),
            m("Thud", ModalMaterial::Membrane, 110.0, 0.72, 0.3, 0.7, 0.16).g(0.55),
        ],
        vec![drv(6.0, 5000.0, -4.0, 55.0)],
        (0.5, 0.55, 0.1, 0.0),
        14,
    )
}
fn ui_toggle() -> ForgePatch {
    sound(
        vec![
            m(
                "Switch",
                ModalMaterial::Wood,
                1280.0,
                0.88,
                0.25,
                0.92,
                0.035,
            )
            .g(0.9),
            ns("Edge", Wh, 8000.0, 0.2, 0.0, 0.012)
                .g(0.22)
                .f(HP, flat(0.82), 0.0),
        ],
        vec![],
        (0.48, 0.45, 0.0, 0.0),
        15,
    )
}

// ---- Footsteps

fn step_dirt() -> ForgePatch {
    sound(
        vec![
            ns("Scuff", Pk, 4000.0, 0.45, 0.15, 0.13)
                .g(1.2)
                .a(dec(-0.6))
                .f(BP, flat(0.5), 0.25),
            m("Thud", ModalMaterial::Membrane, 82.0, 0.75, 0.25, 0.5, 0.12).g(0.8),
            gr("Grit", 1100.0, 55.0, 14.0, 0.5, 0.4, 0.75, 0.09)
                .g(0.45)
                .a(dec(-0.4)),
        ],
        vec![],
        (0.5, 0.5, 0.0, 0.05),
        21,
    )
}
fn step_grass() -> ForgePatch {
    sound(
        vec![
            ns("Swish", Pk, 4000.0, 0.55, 0.0, 0.18)
                .g(1.1)
                .a(c(&[(0.0, 0.0, 0.0), (0.12, 0.85, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, flat(0.56), 0.15),
            gr("Blades", 1600.0, 48.0, 16.0, 0.45, 0.5, 0.7, 0.14)
                .g(0.42)
                .a(dec(-0.5)),
            m(
                "Soft thud",
                ModalMaterial::Membrane,
                76.0,
                0.82,
                0.2,
                0.35,
                0.1,
            )
            .g(0.55),
        ],
        vec![],
        (0.5, 0.5, 0.0, 0.05),
        22,
    )
}
fn step_gravel() -> ForgePatch {
    sound(
        vec![
            gr("Debris", 1350.0, 82.0, 12.0, 0.68, 0.65, 0.85, 0.18)
                .g(1.1)
                .a(c(&[(0.0, 0.95, -0.35), (0.45, 0.65, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, flat(0.64), 0.3),
            m("Stone", ModalMaterial::Stone, 420.0, 0.78, 0.55, 0.82, 0.1).g(0.7),
            gr("Scatter", 2400.0, 60.0, 10.0, 0.7, 0.7, 0.9, 0.09)
                .st(0.05)
                .g(0.5)
                .a(dec(-0.35)),
        ],
        vec![],
        (0.5, 0.55, 0.0, 0.05),
        23,
    )
}
fn step_wood() -> ForgePatch {
    sound(
        vec![
            m("Plank", ModalMaterial::Wood, 195.0, 0.62, 0.28, 0.72, 0.18).g(1.0),
            ns("Sole", Pk, 4000.0, 0.3, 0.0, 0.08)
                .g(0.5)
                .a(dec(-0.55))
                .f(BP, flat(0.42), 0.45),
            m("Heel", ModalMaterial::Wood, 340.0, 0.75, 0.35, 0.85, 0.09)
                .st(0.015)
                .g(0.45),
        ],
        vec![],
        (0.5, 0.5, 0.0, 0.05),
        24,
    )
}
fn step_metal() -> ForgePatch {
    sound(
        vec![
            m("Grate", ModalMaterial::Metal, 460.0, 0.58, 0.48, 0.86, 0.28).g(0.9),
            m("Clank", ModalMaterial::Metal, 740.0, 0.68, 0.55, 0.92, 0.16)
                .st(0.008)
                .g(0.55),
            ns("Scuff", Wh, 8000.0, 0.35, 0.0, 0.055)
                .g(0.32)
                .a(dec(-0.5))
                .f(HP, flat(0.78), 0.0),
        ],
        vec![],
        (0.5, 0.5, 0.0, 0.08),
        25,
    )
}
fn step_concrete() -> ForgePatch {
    sound(
        vec![
            m("Slab", ModalMaterial::Stone, 285.0, 0.82, 0.45, 0.88, 0.1).g(1.0),
            ns("Tap", Wh, 8000.0, 0.25, 0.1, 0.055)
                .g(0.75)
                .a(dec(-0.7))
                .f(HP, flat(0.68), 0.1),
            gr("Grit", 2100.0, 45.0, 9.0, 0.55, 0.35, 0.7, 0.05)
                .g(0.35)
                .a(dec(-0.35)),
        ],
        vec![],
        (0.48, 0.5, 0.0, 0.04),
        26,
    )
}
fn step_snow() -> ForgePatch {
    sound(
        vec![
            ns("Compress", Br, 4000.0, 0.5, 0.2, 0.22)
                .g(0.95)
                .a(c(&[(0.0, 0.0, 0.0), (0.2, 1.0, -0.2), (1.0, 0.0, 0.0)]))
                .f(LP, flat(0.45), 0.1),
            gr("Squeak", 950.0, 52.0, 18.0, 0.45, 0.55, 0.65, 0.2)
                .g(0.55)
                .a(c(&[(0.0, 0.0, 0.0), (0.25, 0.75, 0.0), (1.0, 0.0, 0.0)])),
        ],
        vec![],
        (0.5, 0.45, 0.0, 0.05),
        27,
    )
}

// ---- Weapons & combat

fn swing() -> ForgePatch {
    sound(
        vec![
            ns("Air", Pk, 8000.0, 0.8, 0.0, 0.26)
                .g(1.1)
                .a(c(&[(0.0, 0.0, 0.0), (0.35, 1.0, -0.2), (1.0, 0.0, 0.0)]))
                .f(BP, sweep(&[0.45, 0.78, 0.42]), 0.35)
                .with_pan_curve(CurvePreset::Ramp.build(-0.75, 0.75, 0.0)),
            fm("Edge", Saw, 210.0, 1.5, 0.6, 2, 0.25, 0.22)
                .g(0.32)
                .p(c(&[(0.0, -5.0, 0.0), (1.0, 7.0, 0.0)]))
                .a(c(&[(0.0, 0.0, 0.0), (0.3, 0.3, 0.0), (1.0, 0.0, 0.0)]))
                .f(LP, flat(0.45), 0.15),
        ],
        vec![],
        (0.5, 0.55, 0.0, 0.06),
        31,
    )
}
fn clash() -> ForgePatch {
    sound(
        vec![
            m("Blade", ModalMaterial::Metal, 920.0, 0.32, 0.55, 0.96, 0.55).g(0.9),
            m(
                "Blade 2",
                ModalMaterial::Metal,
                1410.0,
                0.38,
                0.68,
                0.92,
                0.45,
            )
            .st(0.006)
            .g(0.65),
            gr("Sparks", 3200.0, 75.0, 10.0, 0.75, 0.8, 0.85, 0.14)
                .g(0.45)
                .a(dec(-0.6)),
        ],
        vec![rev(45.0, 50.0, 22.0)],
        (0.5, 0.58, 0.0, 0.18),
        32,
    )
}
fn pistol() -> ForgePatch {
    sound(
        vec![
            m("Bolt", ModalMaterial::Metal, 760.0, 0.75, 0.45, 0.95, 0.08).g(0.65),
            ns("Blast", Wh, 8000.0, 0.45, 0.1, 0.22)
                .g(1.0)
                .a(dec(-0.8))
                .f(LP, sweep(&[0.92, 0.42]), 0.15),
            fm("Body", Sin, 92.0, 2.0, 1.1, 1, 0.0, 0.17)
                .g(1.1)
                .p(c(&[(0.0, 16.0, -0.7), (1.0, -8.0, 0.0)]))
                .a(dec(-0.3)),
            gr("Tail", 900.0, 42.0, 16.0, 0.55, 0.65, 0.8, 0.38)
                .st(0.02)
                .g(0.35)
                .a(dec(-0.55)),
        ],
        vec![drv(12.0, 6000.0, -6.0, 75.0), rev(25.0, 50.0, 12.0)],
        (0.5, 0.6, 0.2, 0.1),
        33,
    )
}
fn shotgun() -> ForgePatch {
    sound(
        vec![
            m("Action", ModalMaterial::Metal, 540.0, 0.72, 0.55, 0.96, 0.1).g(0.65),
            ns("Blast", Wh, 8000.0, 0.65, 0.15, 0.36)
                .g(0.95)
                .a(dec(-0.55))
                .f(LP, sweep(&[0.95, 0.35]), 0.1),
            ns("Boom", Br, 4000.0, 0.55, 0.0, 0.62)
                .g(1.2)
                .a(c(&[(0.0, 0.0, 0.0), (0.02, 1.0, -0.4), (1.0, 0.0, 0.0)]))
                .f(LP, sweep(&[0.55, 0.3]), 0.15),
            fm("Sub", Sin, 48.0, 2.0, 0.9, 1, 0.0, 0.5)
                .g(1.0)
                .p(c(&[(0.0, 10.0, -0.5), (1.0, -6.0, 0.0)]))
                .a(dec(-0.2)),
            gr("Debris", 780.0, 48.0, 18.0, 0.6, 0.75, 0.85, 0.5)
                .st(0.05)
                .g(0.45)
                .a(dec(-0.45))
                .f(HP, flat(0.45), 0.0),
        ],
        vec![comp(-18.0, 4.0, 10.0, 120.0), rev(55.0, 50.0, 20.0)],
        (0.55, 0.6, 0.25, 0.2),
        34,
    )
}
fn reload() -> ForgePatch {
    sound(
        vec![
            m(
                "Release",
                ModalMaterial::Metal,
                1550.0,
                0.85,
                0.45,
                0.95,
                0.045,
            )
            .g(0.85),
            gr("Slide", 2400.0, 90.0, 9.0, 0.5, 0.3, 0.6, 0.045)
                .st(0.07)
                .g(0.6)
                .a(dec(-0.35))
                .f(BP, flat(0.72), 0.4),
            m("Lock", ModalMaterial::Metal, 980.0, 0.78, 0.5, 0.92, 0.06)
                .st(0.15)
                .g(0.85),
            m("Body", ModalMaterial::Wood, 260.0, 0.78, 0.25, 0.65, 0.08)
                .st(0.165)
                .g(0.45),
        ],
        vec![trans(20.0, 0.0), rev(20.0, 50.0, 8.0)],
        (0.48, 0.5, 0.0, 0.05),
        35,
    )
}

// ---- Magic

fn charge() -> ForgePatch {
    sound(
        vec![
            o("Rise", Saw, 110.0, 0.8)
                .g(0.7)
                .p(c(&[(0.0, -12.0, 0.4), (1.0, 24.0, 0.0)]))
                .a(c(&[
                    (0.0, 0.0, 0.0),
                    (0.2, 0.7, 0.0),
                    (0.8, 0.8, 0.0),
                    (1.0, 0.0, 0.0),
                ]))
                .f(LP, sweep(&[0.3, 0.85]), 0.5),
            n("Air", Wh, 8000.0, 0.8)
                .g(0.3)
                .a(c(&[(0.0, 0.0, 0.0), (0.5, 0.6, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, sweep(&[0.4, 0.85]), 0.4),
            o("Sparkle", Sin, 1760.0, 0.8).g(0.2).pan(0.3).a(c(&[
                (0.0, 0.0, 0.0),
                (0.4, 0.35, 0.0),
                (1.0, 0.0, 0.0),
            ])),
        ],
        vec![pha(0.6, 70.0, 35.0), rev(55.0, 50.0, 25.0)],
        (0.5, 0.55, 0.0, 0.35),
        41,
    )
}
fn fireball() -> ForgePatch {
    sound(
        vec![
            ns("Roar", Br, 4000.0, 0.65, 0.15, 0.55)
                .g(1.0)
                .a(dec(-0.4))
                .f(LP, sweep(&[0.7, 0.4]), 0.1),
            gr("Embers", 720.0, 62.0, 18.0, 0.6, 0.75, 0.8, 0.62)
                .g(0.65)
                .a(c(&[(0.0, 0.0, 0.0), (0.1, 1.0, 0.0), (1.0, 0.0, 0.0)])),
            ns("Whoosh", Wh, 8000.0, 0.8, 0.0, 0.42)
                .g(0.45)
                .a(c(&[(0.0, 0.0, 0.0), (0.15, 0.85, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, sweep(&[0.45, 0.82]), 0.3)
                .with_pan_curve(CurvePreset::Ramp.build(-0.6, 0.6, 0.0)),
        ],
        vec![drv(8.0, 5000.0, -4.0, 50.0), rev(45.0, 50.0, 25.0)],
        (0.5, 0.6, 0.15, 0.2),
        42,
    )
}
fn ice() -> ForgePatch {
    sound(
        vec![
            m(
                "Crystal",
                ModalMaterial::Glass,
                1380.0,
                0.35,
                0.25,
                0.8,
                0.7,
            )
            .g(0.7),
            m(
                "Shard",
                ModalMaterial::Glass,
                2093.0,
                0.42,
                0.35,
                0.88,
                0.55,
            )
            .st(0.04)
            .g(0.55),
            gr("Frost", 2600.0, 70.0, 14.0, 0.55, 0.8, 0.75, 0.52)
                .g(0.55)
                .a(c(&[(0.0, 0.0, 0.0), (0.04, 1.0, -0.35), (1.0, 0.0, 0.0)]))
                .f(HP, flat(0.72), 0.0),
        ],
        vec![cho(40.0, 0.4, 45.0), rev(60.0, 50.0, 28.0)],
        (0.5, 0.5, 0.0, 0.32),
        43,
    )
}
fn heal() -> ForgePatch {
    let swell = || {
        c(&[
            (0.0, 0.0, 0.0),
            (0.2, 0.6, 0.0),
            (0.8, 0.5, 0.0),
            (1.0, 0.0, 0.0),
        ])
    };
    sound(
        vec![
            o("Root", Sin, 523.0, 0.9).g(0.5).a(swell()),
            o("Third", Sin, 659.0, 0.8)
                .st(0.1)
                .g(0.4)
                .pan(-0.2)
                .a(swell()),
            o("Fifth", Tri, 784.0, 0.7)
                .st(0.2)
                .g(0.3)
                .pan(0.2)
                .a(swell()),
            n("Air", Pk, 4000.0, 0.9)
                .g(0.15)
                .f(HP, flat(0.6), 0.0)
                .a(c(&[(0.0, 0.0, 0.0), (0.3, 0.4, 0.0), (1.0, 0.0, 0.0)])),
        ],
        vec![cho(35.0, 0.5, 40.0), rev(60.0, 50.0, 30.0)],
        (0.5, 0.45, 0.0, 0.4),
        44,
    )
}
fn curse() -> ForgePatch {
    let env = || c(&[(0.0, 0.0, 0.0), (0.1, 0.8, 0.0), (1.0, 0.0, 0.0)]);
    let fall = || c(&[(0.0, 0.0, 0.0), (1.0, -12.0, 0.0)]);
    sound(
        vec![
            o("Drone", Saw, 55.0, 1.2)
                .g(0.9)
                .p(fall())
                .a(env())
                .f(LP, flat(0.35), 0.4),
            o("Beat", Saw, 58.3, 1.2)
                .g(0.6)
                .pan(0.3)
                .p(fall())
                .a(env())
                .f(LP, flat(0.35), 0.4),
            n("Whisper", Br, 4000.0, 1.0)
                .g(0.4)
                .a(c(&[(0.0, 0.0, 0.0), (0.3, 0.5, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, flat(0.4), 0.3),
        ],
        vec![
            ring(42.0, 35.0),
            pha(0.4, 70.0, 40.0),
            drv(6.0, 3000.0, -4.0, 40.0),
            rev(65.0, 50.0, 30.0),
        ],
        (0.55, 0.55, 0.2, 0.4),
        45,
    )
}

// ---- Pickups

fn gem() -> ForgePatch {
    sound(
        vec![
            m("One", ModalMaterial::Glass, 1047.0, 0.42, 0.1, 0.75, 0.22).g(0.65),
            m("Two", ModalMaterial::Glass, 1319.0, 0.42, 0.1, 0.78, 0.26)
                .st(0.065)
                .g(0.65),
            m(
                "Three",
                ModalMaterial::Glass,
                1568.0,
                0.35,
                0.12,
                0.82,
                0.42,
            )
            .st(0.13)
            .g(0.75),
        ],
        vec![dly(1.0, 25.0, 16.0), rev(30.0, 50.0, 12.0)],
        (0.5, 0.5, 0.0, 0.15),
        51,
    )
}
fn health() -> ForgePatch {
    sound(
        vec![
            o("Low", Sin, 440.0, 0.3).g(0.5).a(c(&[
                (0.0, 0.0, 0.0),
                (0.15, 0.7, 0.0),
                (1.0, 0.0, 0.0),
            ])),
            o("High", Sin, 660.0, 0.35).st(0.12).g(0.4).a(dec(-0.2)),
            n("Air", Pk, 4000.0, 0.4)
                .g(0.2)
                .f(HP, flat(0.55), 0.0)
                .a(c(&[(0.0, 0.0, 0.0), (0.3, 0.4, 0.0), (1.0, 0.0, 0.0)])),
        ],
        vec![cho(35.0, 0.5, 35.0), rev(45.0, 50.0, 20.0)],
        (0.5, 0.45, 0.0, 0.25),
        52,
    )
}
fn level_up() -> ForgePatch {
    sound(
        vec![
            o("C", P25, 523.0, 0.18)
                .g(0.6)
                .a(c(&[(0.0, 0.8, 0.0), (1.0, 0.3, 0.0)])),
            o("E", P25, 659.0, 0.2)
                .st(0.12)
                .g(0.6)
                .a(c(&[(0.0, 0.8, 0.0), (1.0, 0.3, 0.0)])),
            o("G", P25, 784.0, 0.25)
                .st(0.24)
                .g(0.6)
                .a(c(&[(0.0, 0.8, 0.0), (1.0, 0.3, 0.0)])),
            o("C high", P25, 1047.0, 0.6).st(0.36).g(0.7).a(dec(-0.3)),
        ],
        vec![dly(1.0, 30.0, 20.0), rev(45.0, 50.0, 20.0)],
        (0.5, 0.6, 0.0, 0.2),
        53,
    )
}

// ---- Damage

fn hurt() -> ForgePatch {
    sound(
        vec![
            o("Body", Saw, 220.0, 0.18)
                .g(0.8)
                .p(c(&[(0.0, 0.0, 0.0), (1.0, -9.0, 0.0)]))
                .a(dec(-0.4))
                .f(LP, sweep(&[0.6, 0.4]), 0.2),
            n("Hit", Pk, 4000.0, 0.12)
                .g(0.6)
                .a(dec(-0.5))
                .f(HP, flat(0.5), 0.0),
        ],
        vec![drv(4.0, 4000.0, -3.0, 40.0), trans(25.0, 0.0)],
        (0.5, 0.55, 0.1, 0.05),
        61,
    )
}
fn death() -> ForgePatch {
    sound(
        vec![
            o("Fall", Saw, 180.0, 1.1)
                .g(0.8)
                .p(c(&[(0.0, 3.0, -0.3), (1.0, -24.0, 0.0)]))
                .a(c(&[(0.0, 0.9, 0.0), (0.6, 0.7, 0.0), (1.0, 0.0, 0.0)]))
                .f(LP, sweep(&[0.7, 0.3]), 0.3),
            n("Rumble", Br, 4000.0, 1.0)
                .g(0.6)
                .a(c(&[(0.0, 0.0, 0.0), (0.1, 0.8, 0.0), (1.0, 0.0, 0.0)]))
                .f(LP, sweep(&[0.5, 0.25]), 0.1),
            o("Sub", Sin, 45.0, 1.0)
                .g(0.7)
                .p(c(&[(0.0, 7.0, -0.4), (1.0, -12.0, 0.0)]))
                .a(dec(-0.2)),
        ],
        vec![pha(0.3, 60.0, 30.0), rev(65.0, 50.0, 35.0)],
        (0.55, 0.55, 0.15, 0.35),
        62,
    )
}
fn heartbeat() -> ForgePatch {
    let thump = || c(&[(0.0, 8.0, -0.6), (1.0, -2.0, 0.0)]);
    sound(
        vec![
            o("Lub", Sin, 50.0, 0.22).g(1.1).p(thump()).a(dec(-0.4)),
            o("Dub", Sin, 50.0, 0.2)
                .st(0.18)
                .g(0.8)
                .p(thump())
                .a(dec(-0.5)),
            n("Chest", Br, 4000.0, 0.4)
                .g(0.3)
                .a(c(&[(0.0, 0.0, 0.0), (0.05, 0.5, 0.0), (1.0, 0.0, 0.0)]))
                .f(LP, flat(0.3), 0.0),
        ],
        vec![],
        (0.5, 0.4, 0.0, 0.05),
        63,
    )
}

// ---- Doors & mechanisms

fn door_creak() -> ForgePatch {
    sound(
        vec![
            o("Creak", Saw, 110.0, 0.9)
                .g(0.8)
                .p(c(&[
                    (0.0, 0.0, 0.0),
                    (0.15, 2.0, -0.4),
                    (0.3, 1.0, 0.0),
                    (0.45, 3.0, -0.3),
                    (0.6, 2.0, 0.0),
                    (0.75, 4.0, -0.2),
                    (1.0, 3.0, 0.0),
                ]))
                .a(c(&[
                    (0.0, 0.0, 0.0),
                    (0.1, 0.5, 0.0),
                    (0.8, 0.4, 0.0),
                    (1.0, 0.0, 0.0),
                ]))
                .f(BP, flat(0.35), 0.7),
            n("Wood", Pk, 4000.0, 0.5)
                .g(0.4)
                .a(dec(-0.3))
                .f(BP, flat(0.4), 0.5),
            o("Thud", Sin, 70.0, 0.15)
                .st(0.8)
                .g(0.5)
                .p(c(&[(0.0, 6.0, -0.5), (1.0, 0.0, 0.0)]))
                .a(dec(-0.3)),
        ],
        vec![rev(35.0, 50.0, 15.0)],
        (0.5, 0.45, 0.0, 0.1),
        71,
    )
}
fn door_slam() -> ForgePatch {
    sound(
        vec![
            m(
                "Wood slam",
                ModalMaterial::Wood,
                165.0,
                0.58,
                0.35,
                0.92,
                0.4,
            )
            .g(1.0),
            m(
                "Frame",
                ModalMaterial::Membrane,
                72.0,
                0.52,
                0.25,
                0.78,
                0.35,
            )
            .g(0.9),
            ns("Air burst", Wh, 8000.0, 0.5, 0.0, 0.22)
                .g(0.75)
                .a(dec(-0.65))
                .f(LP, sweep(&[0.8, 0.35]), 0.1),
        ],
        vec![trans(30.0, 0.0), rev(50.0, 50.0, 22.0)],
        (0.55, 0.55, 0.1, 0.2),
        72,
    )
}
fn gate() -> ForgePatch {
    sound(
        vec![
            gr("Chains", 1250.0, 58.0, 16.0, 0.65, 0.65, 0.75, 1.2)
                .g(0.8)
                .a(c(&[(0.0, 0.0, 0.0), (0.1, 1.0, -0.25), (1.0, 0.0, 0.0)]))
                .f(BP, flat(0.65), 0.45),
            m("Iron", ModalMaterial::Metal, 210.0, 0.35, 0.6, 0.72, 1.05).g(0.75),
            m("Latch", ModalMaterial::Metal, 480.0, 0.65, 0.5, 0.88, 0.25)
                .st(0.92)
                .g(0.65),
        ],
        vec![rev(55.0, 50.0, 24.0)],
        (0.55, 0.55, 0.1, 0.25),
        73,
    )
}
fn lever() -> ForgePatch {
    sound(
        vec![
            m("Click", ModalMaterial::Metal, 1420.0, 0.85, 0.4, 0.95, 0.04).g(0.8),
            m("Clunk", ModalMaterial::Wood, 150.0, 0.68, 0.3, 0.8, 0.14).g(0.85),
            m(
                "Spring",
                ModalMaterial::Metal,
                520.0,
                0.45,
                0.55,
                0.65,
                0.24,
            )
            .st(0.05)
            .g(0.4),
        ],
        vec![trans(25.0, 0.0)],
        (0.48, 0.5, 0.0, 0.05),
        74,
    )
}

// ---- Ambience (seamless loops out of the box)

fn wind() -> ForgePatch {
    looped(
        vec![
            ns("Gust", Br, 4000.0, 0.85, 0.0, 6.0)
                .g(0.95)
                .a(c(&[
                    (0.0, 0.55, 0.0),
                    (0.28, 0.88, -0.2),
                    (0.62, 0.52, 0.0),
                    (0.85, 0.82, -0.2),
                    (1.0, 0.55, 0.0),
                ]))
                .f(BP, sweep(&[0.34, 0.52, 0.36, 0.48, 0.34]), 0.28),
            ns("Whistle", Pk, 4000.0, 0.9, 0.0, 6.0)
                .g(0.32)
                .a(c(&[
                    (0.0, 0.2, 0.0),
                    (0.4, 0.4, 0.0),
                    (0.75, 0.18, 0.0),
                    (1.0, 0.2, 0.0),
                ]))
                .f(BP, sweep(&[0.64, 0.74, 0.64]), 0.58),
            ns("Air", Wh, 8000.0, 0.8, 0.0, 6.0)
                .g(0.2)
                .a(flat(0.28))
                .f(LP, flat(0.48), 0.0),
        ],
        vec![rev(70.0, 50.0, 28.0)],
        (0.5, 0.45, 0.0, 0.45),
        81,
    )
}
fn rain() -> ForgePatch {
    looped(
        vec![
            ns("Hiss", Wh, 8000.0, 0.9, 0.0, 6.0)
                .g(0.78)
                .a(flat(0.6))
                .f(HP, flat(0.55), 0.0),
            ns("Body", Pk, 4000.0, 0.85, 0.0, 6.0)
                .g(0.62)
                .a(flat(0.5))
                .f(BP, flat(0.5), 0.2),
            gr("Drops", 1850.0, 64.0, 14.0, 0.6, 0.9, 0.85, 6.0).g(0.52),
        ],
        vec![rev(50.0, 50.0, 20.0)],
        (0.5, 0.45, 0.0, 0.32),
        82,
    )
}
fn fire() -> ForgePatch {
    looped(
        vec![
            ns("Roar", Br, 4000.0, 0.75, 0.12, 5.0)
                .g(1.0)
                .a(c(&[(0.0, 0.68, 0.0), (0.5, 0.82, 0.0), (1.0, 0.68, 0.0)]))
                .f(LP, flat(0.4), 0.2),
            gr("Crackles", 1400.0, 55.0, 11.0, 0.65, 0.75, 0.88, 5.0).g(0.72),
            ns("Sparks", Cr, 120.0, 0.7, 0.35, 5.0)
                .g(0.45)
                .a(flat(0.55)),
        ],
        vec![rev(35.0, 50.0, 12.0)],
        (0.5, 0.45, 0.0, 0.18),
        83,
    )
}
fn cave() -> ForgePatch {
    looped(
        vec![
            fm("Drone", Sin, 55.0, 1.5, 0.25, 2, 0.22, 6.0)
                .g(0.55)
                .a(c(&[(0.0, 0.35, 0.0), (0.5, 0.45, 0.0), (1.0, 0.35, 0.0)])),
            m("Drip", ModalMaterial::Glass, 1380.0, 0.48, 0.08, 0.45, 0.28)
                .st(1.1)
                .g(0.42),
            m(
                "Drip 2",
                ModalMaterial::Glass,
                1080.0,
                0.48,
                0.08,
                0.45,
                0.3,
            )
            .st(3.4)
            .g(0.35),
            ns("Air", Br, 4000.0, 0.85, 0.0, 6.0)
                .g(0.6)
                .a(flat(0.42))
                .f(LP, flat(0.35), 0.0),
        ],
        vec![dly(3.0, 20.0, 15.0), rev(90.0, 50.0, 42.0)],
        (0.5, 0.4, 0.0, 0.55),
        84,
    )
}
fn machine_hum() -> ForgePatch {
    looped(
        vec![
            fm("Turbine", Saw, 50.0, 2.0, 0.45, 3, 0.18, 4.0)
                .g(0.8)
                .a(flat(0.7))
                .f(LP, flat(0.36), 0.4),
            fm("Harmonic", Saw, 100.0, 1.5, 0.3, 2, 0.2, 4.0)
                .g(0.42)
                .a(flat(0.65))
                .f(LP, flat(0.42), 0.25),
            gr("Rattle", 420.0, 24.0, 14.0, 0.3, 0.45, 0.55, 4.0).g(0.28),
        ],
        vec![trem(6.0, 18.0), rev(25.0, 50.0, 10.0)],
        (0.5, 0.5, 0.1, 0.1),
        85,
    )
}

// ---- Creatures & sci-fi

fn growl() -> ForgePatch {
    sound(
        vec![
            o("Growl", Saw, 70.0, 0.9)
                .g(1.0)
                .p(c(&[(0.0, -3.0, 0.0), (0.5, 2.0, 0.3), (1.0, -7.0, 0.0)]))
                .a(c(&[(0.0, 0.0, 0.0), (0.1, 0.9, 0.0), (1.0, 0.0, 0.0)]))
                .f(LP, sweep(&[0.45, 0.6, 0.3]), 0.35),
            o("Sub", Sin, 40.0, 0.9).g(0.6).a(dec(-0.2)),
            n("Breath", Br, 4000.0, 0.8)
                .g(0.4)
                .a(c(&[(0.0, 0.0, 0.0), (0.2, 0.6, 0.0), (1.0, 0.0, 0.0)]))
                .f(BP, flat(0.4), 0.3),
        ],
        vec![
            drv(10.0, 4000.0, -5.0, 60.0),
            ring(38.0, 25.0),
            rev(45.0, 50.0, 20.0),
        ],
        (0.55, 0.6, 0.2, 0.2),
        91,
    )
}
fn slime() -> ForgePatch {
    sound(
        vec![
            n("Squelch", Pk, 4000.0, 0.3)
                .g(0.9)
                .a(c(&[(0.0, 0.0, 0.0), (0.1, 1.0, -0.3), (1.0, 0.0, 0.0)]))
                .f(BP, sweep(&[0.5, 0.3, 0.7]), 0.5),
            o("Blorp", Sin, 200.0, 0.25)
                .g(0.5)
                .p(c(&[(0.0, 7.0, -0.5), (1.0, -10.0, 0.0)]))
                .a(dec(-0.4)),
            n("Bubbles", Cr, 900.0, 0.3).g(0.3).a(dec(-0.3)),
        ],
        vec![cho(50.0, 0.3, 40.0), rev(30.0, 50.0, 15.0)],
        (0.5, 0.55, 0.05, 0.15),
        92,
    )
}
fn siren() -> ForgePatch {
    sound(
        vec![
            o("Tone", Sq, 880.0, 0.9)
                .g(0.6)
                .p(c(&[
                    (0.0, 0.0, 0.0),
                    (0.25, 12.0, 0.0),
                    (0.5, 0.0, 0.0),
                    (0.75, 12.0, 0.0),
                    (1.0, 0.0, 0.0),
                ]))
                .a(c(&[
                    (0.0, 0.0, 0.0),
                    (0.05, 0.7, 0.0),
                    (0.95, 0.7, 0.0),
                    (1.0, 0.0, 0.0),
                ])),
            o("Sub", Sin, 55.0, 0.9).g(0.3).a(flat(0.5)),
        ],
        vec![
            ring(120.0, 20.0),
            dly(1.0, 30.0, 20.0),
            rev(40.0, 50.0, 20.0),
        ],
        (0.5, 0.55, 0.0, 0.25),
        93,
    )
}
