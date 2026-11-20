//! Tide's factory presets from index 18 on, by category (`SynthPatch::PRESETS`
//! holds the names, `SynthPatch::CATEGORIES` the category of each). Recipes
//! follow docs/research/0.2.0/C1.md; pitch drops use a positive Filter Env →
//! Pitch A depth (the envelope rises to 1 and falls, so the pitch falls).

use crate::fx::FxKind;
use crate::synth::{
    FX_KINDS, Filter, Lfo, LfoDivision, LfoShape, ModRoute, ModSource, ModTarget, Oscillator,
    Spectral, SynthPatch, Table, Warp,
};

/// A patch under construction: short setters in the recipe notation.
struct P(SynthPatch);

impl P {
    fn new() -> Self {
        P(SynthPatch::default())
    }
    fn table(mut self, t: Table) -> Self {
        self.0.table = t;
        self
    }
    fn pos(mut self, v: f64) -> Self {
        self.0.position = v;
        self
    }
    fn lvl(mut self, v: f64) -> Self {
        self.0.level = v;
        self
    }
    fn warp(mut self, w: Warp, amount: f64) -> Self {
        self.0.warp = w;
        self.0.warp_amount = amount;
        self
    }
    fn b(mut self, f: impl FnOnce(&mut Oscillator)) -> Self {
        f(&mut self.0.osc_b);
        self
    }
    /// Oscillator C: level, partials, tilt, odd, shimmer.
    fn sp(mut self, level: f64, partials: u8, tilt: f64, odd: f64, shimmer: f64) -> Self {
        self.0.spectral = Spectral {
            level,
            partials,
            tilt,
            odd,
            shimmer,
            ..self.0.spectral
        };
        self
    }
    fn sp_coarse(mut self, v: f64) -> Self {
        self.0.spectral.coarse = v;
        self
    }
    fn sub(mut self, v: f64) -> Self {
        self.0.sub = v;
        self
    }
    fn noise(mut self, v: f64) -> Self {
        self.0.noise = v;
        self
    }
    fn uni(mut self, n: u8, detune: f64, spread: f64) -> Self {
        self.0.unison = n;
        self.0.detune = detune;
        self.0.spread = spread;
        self
    }
    fn filter(mut self, f: Filter, cutoff: f64) -> Self {
        self.0.filter = f;
        self.0.cutoff = cutoff;
        self
    }
    fn cut(mut self, v: f64) -> Self {
        self.0.cutoff = v;
        self
    }
    fn res(mut self, v: f64) -> Self {
        self.0.resonance = v;
        self
    }
    fn ea(mut self, v: f64) -> Self {
        self.0.env_amount = v;
        self
    }
    fn glide(mut self, v: f64) -> Self {
        self.0.glide = v;
        self
    }
    fn a(mut self, v: f64) -> Self {
        self.0.amp_env.attack = v;
        self
    }
    fn d(mut self, v: f64) -> Self {
        self.0.amp_env.decay = v;
        self
    }
    fn s(mut self, v: f64) -> Self {
        self.0.amp_env.sustain = v;
        self
    }
    fn r(mut self, v: f64) -> Self {
        self.0.amp_env.release = v;
        self
    }
    fn fa(mut self, v: f64) -> Self {
        self.0.filter_env.attack = v;
        self
    }
    fn fd(mut self, v: f64) -> Self {
        self.0.filter_env.decay = v;
        self
    }
    fn fs(mut self, v: f64) -> Self {
        self.0.filter_env.sustain = v;
        self
    }
    fn lfo(mut self, shape: LfoShape, rate: f64) -> Self {
        self.0.lfos[0] = Lfo {
            shape,
            rate,
            ..Lfo::default()
        };
        self
    }
    fn lfo_sync(mut self, shape: LfoShape, division: LfoDivision) -> Self {
        self.0.lfos[0] = Lfo {
            shape,
            sync: true,
            division,
            retrigger: true,
            ..Lfo::default()
        };
        self
    }
    fn route(mut self, source: ModSource, target: ModTarget, depth: f64) -> Self {
        self.0.mods.push(ModRoute {
            source,
            target,
            depth,
        });
        self
    }
    fn lfo_to(self, target: ModTarget, depth: f64) -> Self {
        self.route(ModSource::Lfo1, target, depth)
    }
    fn env_to(self, target: ModTarget, depth: f64) -> Self {
        self.route(ModSource::FilterEnv, target, depth)
    }
    fn fx(mut self, kind: FxKind, set: &[(usize, f32)]) -> Self {
        let mut slot = crate::fx::FxSlot::new(kind);
        for &(i, v) in set {
            slot.params[i] = v;
        }
        self.0.fx.push(slot);
        self
    }
    fn drive(self, db: f32, tone: f32, out: f32) -> Self {
        self.fx(FxKind::Drive, &[(0, db), (1, tone), (2, out)])
    }
    fn drive_mix(self, db: f32, tone: f32, out: f32, mix: f32) -> Self {
        self.fx(FxKind::Drive, &[(0, db), (1, tone), (2, out), (3, mix)])
    }
    fn chorus(self, depth: f32, mix: f32) -> Self {
        self.fx(FxKind::Chorus, &[(1, depth), (3, mix)])
    }
    fn delay(self, steps: f32, feedback: f32, wet: f32) -> Self {
        self.fx(FxKind::Delay, &[(0, steps), (1, feedback), (4, wet)])
    }
    fn rev(self, size: f32, damping: f32, wet: f32) -> Self {
        self.fx(FxKind::Reverb, &[(0, size), (1, damping), (4, wet)])
    }
    /// The patch, its effects in chain order.
    fn done(mut self) -> SynthPatch {
        self.0
            .fx
            .sort_by_key(|s| FX_KINDS.iter().position(|k| *k == s.kind));
        self.0
    }
}

use LfoShape::{SampleAndHold as SH, Sine, Triangle as Tri};
use ModTarget::*;
use Table::*;

/// Names and categories of the presets `extra` builds, in index order (the
/// source of `SynthPatch::PRESETS`/`CATEGORIES` from 18 on; a test keeps them equal).
#[cfg_attr(not(test), allow(dead_code))]
pub const EXTRA: [(&str, &str); 87] = [
    // Bass
    ("Pure Sub", "Bass"),
    ("808 Boom", "Bass"),
    ("Reese", "Bass"),
    ("Acid 303", "Bass"),
    ("FM Growl", "Bass"),
    ("Pluck Bass", "Bass"),
    ("Organ Bass", "Bass"),
    ("Funk Square", "Bass"),
    ("Click Sub", "Bass"),
    // Lead
    ("Saw Lead", "Lead"),
    ("Supersaw", "Lead"),
    ("Sync Lead", "Lead"),
    ("Square Lead", "Lead"),
    ("Brass Lead", "Lead"),
    ("Portamento Solo", "Lead"),
    ("Fifth Lead", "Lead"),
    ("PWM Lead", "Lead"),
    ("Screamer", "Lead"),
    ("Bell Lead", "Lead"),
    // Pad
    ("Warm Analog", "Pad"),
    ("String Machine", "Pad"),
    ("PWM Pad", "Pad"),
    ("Glass Pad", "Pad"),
    ("Choir Pad", "Pad"),
    ("Dark Drone", "Pad"),
    ("Air Layer", "Pad"),
    ("Shimmer Pad", "Pad"),
    ("Sweep Pad", "Pad"),
    ("Lo-fi Tape", "Pad"),
    // Keys
    ("Full Organ", "Keys"),
    ("E-Piano", "Keys"),
    ("Clav", "Keys"),
    ("Harpsichord", "Keys"),
    ("Rhodes Warm", "Keys"),
    ("Wurli", "Keys"),
    ("Toy Piano", "Keys"),
    // Pluck
    ("Glass Pluck", "Pluck"),
    ("Koto", "Pluck"),
    ("Marimba", "Pluck"),
    ("Trance Pluck", "Pluck"),
    ("Muted Pluck", "Pluck"),
    ("Bell Pluck", "Pluck"),
    ("House Stab", "Pluck"),
    // Bell
    ("Tubular", "Bell"),
    ("Glass Bell", "Bell"),
    ("FM Bell", "Bell"),
    ("Church Bell", "Bell"),
    ("Music Box", "Bell"),
    ("Cowbell", "Bell"),
    ("Handbell", "Bell"),
    ("Celesta", "Bell"),
    // Arp
    ("Arp Pluck", "Arp"),
    ("Arp Square", "Arp"),
    ("Arp Bell", "Arp"),
    ("Arp Bass", "Arp"),
    ("Arp Stab", "Arp"),
    ("Arp Chiptune", "Arp"),
    ("Arp Marimba", "Arp"),
    ("Arp Acid", "Arp"),
    ("Arp Organ", "Arp"),
    // FX
    ("Riser", "FX"),
    ("Downlifter", "FX"),
    ("Laser", "FX"),
    ("Wind", "FX"),
    ("Siren", "FX"),
    ("Robot", "FX"),
    ("Noise Sweep", "FX"),
    ("Alien Vowel", "FX"),
    ("Impact", "FX"),
    // Drum-synth
    ("Synth Kick 909", "Drum"),
    ("Synth Kick 808", "Drum"),
    ("Synth Snare", "Drum"),
    ("Synth Clap", "Drum"),
    ("Synth Closed Hat", "Drum"),
    ("Synth Open Hat", "Drum"),
    ("Synth Tom", "Drum"),
    ("Synth Rim", "Drum"),
    ("Synth Cowbell", "Drum"),
    // Chiptune
    ("NES Square", "Chiptune"),
    ("Pulse Wave", "Chiptune"),
    ("Triangle Bass", "Chiptune"),
    ("Noise Snare", "Chiptune"),
    ("Chip Arp", "Chiptune"),
    ("Game Over", "Chiptune"),
    ("Coin", "Chiptune"),
    ("Boss Bass", "Chiptune"),
    ("SID Lead", "Chiptune"),
];

/// Preset `i` of `EXTRA`.
pub fn extra(i: usize) -> SynthPatch {
    let p = P::new();
    match i {
        // ---- Bass
        0 => p.lvl(0.0).sub(1.0).cut(300.0).a(0.0).d(0.08).s(1.0).r(0.15),
        1 => p
            .sub(0.6)
            .cut(400.0)
            .env_to(PitchA, 0.5)
            .fd(0.05)
            .fs(0.0)
            .a(0.0)
            .d(1.2)
            .s(0.0)
            .r(0.5),
        2 => p
            .pos(1.0)
            .uni(7, 0.05, 0.6)
            .cut(900.0)
            .res(0.2)
            .a(0.01)
            .d(0.2)
            .s(1.0)
            .r(0.2),
        3 => p
            .pos(1.0)
            .cut(200.0)
            .res(0.7)
            .ea(0.45)
            .fd(0.18)
            .fs(0.0)
            .glide(0.06)
            .drive(12.0, 4000.0, -6.0),
        4 => p
            .pos(0.3)
            .b(|o| {
                o.position = 0.4;
                o.level = 0.5;
                o.coarse = 12.0;
                o.warp = Warp::FmSub;
                o.warp_amount = 0.6;
            })
            .cut(700.0)
            .res(0.3)
            .lfo_sync(Sine, LfoDivision::Eighth)
            .lfo_to(WarpB, 0.3),
        5 => p
            .pos(0.85)
            .sub(0.4)
            .cut(700.0)
            .ea(0.5)
            .fd(0.12)
            .fs(0.0)
            .a(0.0)
            .d(0.3)
            .s(0.2)
            .r(0.1),
        6 => p
            .table(Organ)
            .pos(0.3)
            .sub(0.5)
            .cut(1200.0)
            .a(0.002)
            .d(0.2)
            .s(0.7)
            .r(0.1),
        7 => p
            .table(SquarePulse)
            .pos(0.25)
            .cut(600.0)
            .ea(0.35)
            .fd(0.08)
            .fs(0.0)
            .a(0.0)
            .d(0.15)
            .s(0.3)
            .r(0.08),
        8 => p
            .lvl(0.45)
            .sub(0.7)
            .noise(0.08)
            .env_to(Noise, 0.8)
            .fd(0.04)
            .fs(0.0)
            .a(0.0)
            .d(0.35)
            .s(0.6)
            .r(0.1),
        // ---- Lead
        9 => p.pos(1.0).uni(3, 0.08, 0.5).cut(7000.0).glide(0.04),
        10 => p
            .pos(0.9)
            .uni(7, 0.1, 0.9)
            .cut(9000.0)
            .chorus(45.0, 40.0)
            .rev(60.0, 50.0, 30.0),
        11 => p
            .warp(Warp::Sync, 0.5)
            .pos(0.7)
            .cut(5000.0)
            .res(0.2)
            .lfo(Tri, 0.18)
            .lfo_to(ModTarget::Warp, 0.15),
        12 => p
            .table(SquarePulse)
            .pos(0.4)
            .cut(4000.0)
            .res(0.15)
            .glide(0.03)
            .delay(3.0, 45.0, 30.0),
        13 => p
            .pos(0.8)
            .cut(2500.0)
            .ea(0.35)
            .fa(0.06)
            .fd(0.4)
            .fs(0.6)
            .uni(2, 0.03, 0.3)
            .a(0.02),
        14 => p
            .pos(0.95)
            .glide(0.12)
            .lfo(Sine, 5.5)
            .lfo_to(PitchA, 0.008)
            .a(0.01),
        15 => p
            .pos(0.8)
            .b(|o| {
                o.position = 0.8;
                o.coarse = 7.0;
                o.level = 0.4;
            })
            .cut(6000.0)
            .uni(3, 0.05, 0.4),
        16 => p
            .warp(Warp::PwmMirror, 0.4)
            .lfo(Tri, 0.2)
            .lfo_to(ModTarget::Warp, 0.2)
            .uni(3, 0.06, 0.4),
        17 => p
            .warp(Warp::Sync, 0.8)
            .pos(0.8)
            .cut(3000.0)
            .res(0.5)
            .ea(0.3)
            .fd(0.3)
            .fs(0.2)
            .drive_mix(18.0, 6000.0, -8.0, 100.0),
        18 => p
            .sp(0.8, 24, 0.1, 0.3, 0.0)
            .sp_coarse(12.0)
            .cut(9000.0)
            .a(0.0)
            .d(1.2)
            .s(0.1)
            .r(0.6),
        // ---- Pad
        19 => p
            .pos(0.6)
            .uni(5, 0.12, 0.8)
            .cut(4500.0)
            .a(0.6)
            .r(1.8)
            .chorus(50.0, 50.0)
            .rev(85.0, 50.0, 35.0),
        20 => p
            .pos(0.75)
            .uni(7, 0.18, 0.9)
            .lfo(Tri, 0.15)
            .lfo_to(Position, 0.1)
            .a(0.5)
            .r(1.6)
            .chorus(60.0, 50.0)
            .rev(85.0, 50.0, 35.0),
        21 => p
            .warp(Warp::PwmMirror, 0.35)
            .lfo(Sine, 0.12)
            .lfo_to(ModTarget::Warp, 0.25)
            .uni(5, 0.1, 0.7)
            .cut(5000.0)
            .a(0.8)
            .r(2.0),
        22 => p
            .table(Vowel)
            .pos(0.3)
            .sp(0.5, 20, -0.2, 0.0, 0.4)
            .cut(6000.0)
            .a(1.0)
            .r(2.5),
        23 => p
            .table(Vowel)
            .pos(0.45)
            .lvl(0.55)
            .sp(0.35, 24, -0.2, -0.5, 0.5)
            .a(0.8)
            .r(2.0)
            .chorus(35.0, 40.0)
            .rev(90.0, 50.0, 40.0),
        24 => p
            .pos(0.2)
            .cut(700.0)
            .res(0.1)
            .uni(4, 0.06, 0.5)
            .a(2.0)
            .r(4.0)
            .rev(100.0, 60.0, 50.0),
        25 => p.pos(0.5).lvl(0.7).noise(0.12).cut(6000.0).a(1.5).r(3.0),
        26 => p
            .lvl(0.0)
            .sp(1.0, 24, 0.3, 0.0, 0.8)
            .cut(8000.0)
            .lfo(Sine, 0.07)
            .lfo_to(TiltC, 0.4)
            .a(1.2)
            .r(3.0),
        27 => p
            .pos(0.8)
            .cut(600.0)
            .ea(0.5)
            .fa(1.5)
            .fd(3.0)
            .fs(0.4)
            .uni(5, 0.1, 0.7)
            .a(0.4)
            .r(2.0)
            .rev(90.0, 50.0, 45.0),
        28 => p
            .pos(0.5)
            .a(0.4)
            .r(1.2)
            .drive_mix(8.0, 3000.0, -4.0, 40.0)
            .chorus(30.0, 30.0),
        // ---- Keys
        29 => p
            .table(Organ)
            .pos(0.55)
            .uni(2, 0.02, 0.3)
            .cut(6000.0)
            .a(0.005)
            .r(0.1)
            .drive_mix(8.0, 5000.0, -5.0, 60.0),
        30 => p
            .lvl(0.0)
            .sp(0.9, 6, -0.3, 0.5, 0.0)
            .b(|o| {
                o.coarse = 12.0;
                o.warp = Warp::FmSub;
                o.warp_amount = 0.25;
                o.level = 0.25;
            })
            .a(0.0)
            .d(1.2)
            .s(0.25)
            .r(0.4)
            .chorus(30.0, 35.0),
        31 => p
            .table(SquarePulse)
            .pos(0.2)
            .filter(Filter::HighPass, 300.0)
            .d(0.25)
            .s(0.1)
            .r(0.05)
            .ea(0.3)
            .fd(0.1)
            .fs(0.0)
            .drive(6.0, 4000.0, -3.0),
        32 => p
            .pos(0.95)
            .noise(0.15)
            .env_to(Noise, 0.9)
            .fd(0.03)
            .fs(0.0)
            .d(0.6)
            .s(0.0)
            .r(0.1)
            .filter(Filter::HighPass, 200.0),
        33 => p
            .lvl(0.0)
            .sp(0.9, 4, -0.5, -0.3, 0.0)
            .a(0.01)
            .d(1.5)
            .s(0.3)
            .r(0.5)
            .chorus(20.0, 50.0)
            .rev(40.0, 50.0, 15.0),
        34 => p
            .table(SquarePulse)
            .pos(0.5)
            .cut(2500.0)
            .a(0.005)
            .d(0.8)
            .s(0.2)
            .r(0.3)
            .drive(10.0, 3000.0, -5.0),
        35 => p
            .lvl(0.0)
            .sp(0.8, 3, 0.2, 0.8, 0.0)
            .sp_coarse(12.0)
            .d(0.4)
            .s(0.0)
            .r(0.2)
            .delay(2.0, 30.0, 20.0)
            .rev(50.0, 50.0, 20.0),
        // ---- Pluck
        36 => p
            .table(Vowel)
            .pos(0.6)
            .d(0.18)
            .s(0.0)
            .r(0.15)
            .delay(2.0, 40.0, 25.0)
            .rev(40.0, 50.0, 15.0),
        37 => p
            .pos(0.95)
            .filter(Filter::HighPass, 400.0)
            .d(0.35)
            .s(0.0)
            .r(0.15)
            .ea(0.3)
            .fd(0.12)
            .fs(0.0),
        38 => p
            .lvl(0.0)
            .sp(1.0, 3, -0.7, 0.3, 0.0)
            .d(0.5)
            .s(0.0)
            .r(0.2)
            .rev(30.0, 50.0, 10.0),
        39 => p
            .pos(1.0)
            .uni(5, 0.1, 0.7)
            .d(0.2)
            .s(0.0)
            .r(0.15)
            .delay(3.0, 45.0, 30.0)
            .rev(50.0, 50.0, 20.0),
        40 => p
            .pos(0.8)
            .cut(900.0)
            .d(0.1)
            .s(0.0)
            .r(0.08)
            .drive(8.0, 3000.0, -5.0),
        41 => p
            .lvl(0.0)
            .sp(0.8, 8, 0.2, -0.4, 0.2)
            .d(0.7)
            .s(0.0)
            .r(0.3)
            .rev(60.0, 50.0, 25.0),
        42 => p
            .table(Organ)
            .pos(0.8)
            .d(0.3)
            .s(0.2)
            .r(0.2)
            .chorus(25.0, 40.0),
        // ---- Bell
        43 => p
            .lvl(0.0)
            .sp(0.5, 16, 0.1, -0.3, 0.0)
            .a(0.0)
            .d(3.0)
            .s(0.0)
            .r(1.5)
            .rev(95.0, 50.0, 40.0),
        44 => p
            .lvl(0.0)
            .sp(0.9, 8, 0.4, -0.5, 0.3)
            .d(2.0)
            .s(0.0)
            .r(1.0)
            .chorus(25.0, 30.0)
            .rev(70.0, 50.0, 30.0),
        45 => p
            .pos(0.2)
            .b(|o| {
                o.coarse = 19.0;
                o.warp = Warp::FmSub;
                o.warp_amount = 0.7;
                o.level = 0.5;
            })
            .d(1.5)
            .s(0.0)
            .r(0.8)
            .rev(60.0, 50.0, 25.0),
        46 => p
            .lvl(0.0)
            .sub(0.2)
            .sp(0.45, 12, -0.4, -0.2, 0.0)
            .b(|o| {
                o.fine = 5.0;
                o.level = 0.3;
            })
            .d(4.0)
            .s(0.0)
            .r(2.0)
            .rev(100.0, 60.0, 45.0),
        47 => p
            .lvl(0.0)
            .sp(0.9, 5, 0.2, 0.7, 0.0)
            .d(0.8)
            .s(0.0)
            .r(0.4)
            .delay(3.0, 25.0, 20.0),
        48 => p
            .table(SquarePulse)
            .pos(0.1)
            .b(|o| {
                o.table = SquarePulse;
                o.position = 0.1;
                o.coarse = 7.0;
                o.fine = 20.0;
                o.level = 0.6;
            })
            .filter(Filter::HighPass, 500.0)
            .d(0.4)
            .s(0.0)
            .r(0.1)
            .drive(6.0, 5000.0, -4.0),
        49 => p
            .lvl(0.0)
            .sp(0.7, 10, 0.2, 0.5, 0.0)
            .lfo(Sine, 6.0)
            .lfo_to(PitchA, 0.002)
            .d(2.5)
            .s(0.0)
            .r(1.0)
            .rev(80.0, 50.0, 35.0),
        50 => p
            .lvl(0.0)
            .sp(0.8, 4, -0.2, 0.0, 0.0)
            .sp_coarse(12.0)
            .d(1.0)
            .s(0.0)
            .r(0.5)
            .rev(50.0, 50.0, 20.0),
        // ---- Arp
        51 => p
            .pos(0.9)
            .d(0.12)
            .s(0.0)
            .r(0.08)
            .cut(3000.0)
            .delay(1.0, 35.0, 25.0),
        52 => p
            .table(SquarePulse)
            .pos(0.35)
            .d(0.1)
            .s(0.0)
            .r(0.06)
            .delay(2.0, 40.0, 25.0),
        53 => p
            .lvl(0.0)
            .sp(0.8, 8, 0.2, 0.0, 0.0)
            .d(0.3)
            .s(0.0)
            .r(0.15)
            .delay(3.0, 35.0, 30.0),
        54 => p.pos(0.8).sub(0.4).d(0.15).s(0.0).r(0.08).cut(700.0),
        55 => p
            .table(Organ)
            .pos(0.7)
            .uni(3, 0.04, 0.4)
            .d(0.18)
            .s(0.1)
            .r(0.1)
            .chorus(30.0, 35.0),
        56 => p
            .table(SquarePulse)
            .pos(0.1)
            .cut(20000.0)
            .d(0.08)
            .s(0.0)
            .r(0.04)
            .glide(0.01),
        57 => p
            .lvl(0.0)
            .sp(1.0, 3, -0.6, 0.3, 0.0)
            .d(0.2)
            .s(0.0)
            .r(0.1)
            .rev(30.0, 50.0, 12.0),
        58 => p
            .pos(1.0)
            .cut(400.0)
            .res(0.6)
            .ea(0.4)
            .fd(0.1)
            .fs(0.0)
            .d(0.1)
            .s(0.0)
            .r(0.06)
            .drive(10.0, 5000.0, -5.0),
        59 => p
            .table(Organ)
            .pos(0.5)
            .d(0.1)
            .s(0.1)
            .r(0.08)
            .chorus(20.0, 30.0),
        // ---- FX
        60 => p
            .lvl(0.0)
            .noise(1.0)
            .filter(Filter::HighPass, 200.0)
            .ea(1.0)
            .fa(4.0)
            .fs(1.0)
            .a(4.0)
            .r(0.5),
        61 => p
            .pos(0.8)
            .cut(300.0)
            .ea(1.0)
            .env_to(PitchA, 0.25)
            .fa(0.01)
            .fd(2.0)
            .fs(0.0)
            .a(0.0)
            .d(2.0)
            .s(0.0)
            .r(1.0),
        62 => p
            .pos(0.9)
            .env_to(PitchA, 0.9)
            .fd(0.18)
            .fs(0.0)
            .d(0.2)
            .s(0.0)
            .r(0.1)
            .delay(2.0, 50.0, 40.0),
        63 => p
            .lvl(0.0)
            .noise(1.0)
            .filter(Filter::BandPass, 800.0)
            .res(0.4)
            .lfo(Sine, 0.1)
            .lfo_to(Cutoff, 0.3)
            .a(2.0)
            .r(2.0),
        64 => p
            .table(SquarePulse)
            .pos(0.3)
            .lfo(Tri, 1.5)
            .lfo_to(PitchA, 0.3)
            .a(0.05),
        65 => p
            .b(|o| {
                o.warp = Warp::FmSub;
                o.warp_amount = 0.9;
                o.level = 0.6;
            })
            .lfo(SH, 8.0)
            .lfo_to(ModTarget::Warp, 0.5)
            .filter(Filter::HighPass, 300.0),
        66 => p
            .lvl(0.0)
            .noise(1.0)
            .filter(Filter::HighPass, 2000.0)
            .lfo(Sine, 0.05)
            .lfo_to(Cutoff, 0.4)
            .a(0.5)
            .r(1.5)
            .rev(100.0, 60.0, 50.0),
        67 => p
            .table(Vowel)
            .pos(0.2)
            .lfo(SH, 4.0)
            .lfo_to(Position, 0.8)
            .delay(4.0, 45.0, 35.0)
            .rev(70.0, 50.0, 30.0),
        68 => p
            .lvl(0.0)
            .sub(0.6)
            .noise(0.2)
            .env_to(PitchA, 0.4)
            .fd(0.3)
            .fs(0.0)
            .d(2.0)
            .s(0.0)
            .r(0.5)
            .drive(8.0, 4000.0, -9.0)
            .rev(100.0, 60.0, 60.0),
        // ---- Drum-synth
        69 => p
            .a(0.0)
            .d(0.35)
            .s(0.0)
            .r(0.1)
            .env_to(PitchA, 0.35)
            .fd(0.04)
            .fs(0.0)
            .noise(0.05)
            .drive(8.0, 4000.0, -4.0),
        70 => p
            .lvl(0.2)
            .sub(0.8)
            .env_to(PitchA, 0.3)
            .fd(0.05)
            .fs(0.0)
            .a(0.0)
            .d(1.0)
            .s(0.0)
            .r(0.3),
        71 => p
            .pos(0.8)
            .lvl(0.6)
            .noise(0.6)
            .cut(5000.0)
            .ea(0.2)
            .fd(0.05)
            .fs(0.0)
            .a(0.0)
            .d(0.18)
            .s(0.0)
            .r(0.08),
        72 => p
            .lvl(0.0)
            .noise(0.9)
            .filter(Filter::HighPass, 1200.0)
            .a(0.005)
            .d(0.12)
            .s(0.0)
            .r(0.08)
            .delay(1.0, 30.0, 20.0),
        73 => p
            .lvl(0.0)
            .noise(1.0)
            .filter(Filter::HighPass, 7000.0)
            .a(0.0)
            .d(0.04)
            .s(0.0)
            .r(0.03),
        74 => p
            .lvl(0.0)
            .noise(1.0)
            .filter(Filter::HighPass, 6000.0)
            .a(0.0)
            .d(0.5)
            .s(0.0)
            .r(0.2),
        75 => p
            .pos(0.1)
            .sub(0.3)
            .env_to(PitchA, 0.15)
            .fd(0.06)
            .fs(0.0)
            .a(0.0)
            .d(0.4)
            .s(0.0)
            .r(0.15),
        76 => p
            .lvl(0.0)
            .noise(0.5)
            .filter(Filter::HighPass, 2000.0)
            .res(0.5)
            .a(0.0)
            .d(0.02)
            .s(0.0)
            .r(0.02),
        77 => p
            .table(SquarePulse)
            .pos(0.1)
            .b(|o| {
                o.table = SquarePulse;
                o.position = 0.1;
                o.coarse = 7.0;
                o.level = 0.5;
            })
            .filter(Filter::HighPass, 400.0)
            .a(0.0)
            .d(0.3)
            .s(0.0)
            .r(0.08),
        // ---- Chiptune
        78 => p
            .table(SquarePulse)
            .pos(0.15)
            .cut(20000.0)
            .a(0.0)
            .d(0.1)
            .s(0.8)
            .r(0.05)
            .glide(0.02),
        79 => p
            .table(SquarePulse)
            .pos(0.05)
            .filter(Filter::HighPass, 100.0),
        80 => p.pos(0.25).sub(0.3).cut(1000.0),
        81 => p
            .lvl(0.0)
            .noise(0.8)
            .filter(Filter::HighPass, 1500.0)
            .a(0.0)
            .d(0.1)
            .s(0.0)
            .r(0.05),
        82 => p
            .table(SquarePulse)
            .pos(0.2)
            .d(0.06)
            .s(0.0)
            .r(0.04)
            .delay(1.0, 30.0, 20.0),
        83 => p
            .pos(0.4)
            .cut(4000.0)
            .ea(-0.6)
            .fa(0.01)
            .fd(0.6)
            .fs(0.0)
            .d(0.7)
            .s(0.0)
            .r(0.2),
        // Coin: the pitch climbs to the note (a negative drop).
        84 => p
            .table(SquarePulse)
            .pos(0.5)
            .env_to(PitchA, -0.5)
            .fd(0.06)
            .fs(0.0)
            .d(0.08)
            .s(0.0)
            .r(0.06),
        85 => p
            .table(SquarePulse)
            .pos(0.3)
            .sub(0.5)
            .drive(6.0, 4000.0, -4.0),
        86 => p
            .warp(Warp::PwmMirror, 0.5)
            .lfo(Tri, 3.0)
            .lfo_to(ModTarget::Warp, 0.2)
            .cut(8000.0),
        _ => p,
    }
    .done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::{BAR, InstrumentKind, Note, STEP, Song};

    #[test]
    fn names_and_categories_line_up() {
        assert_eq!(SynthPatch::PRESETS.len(), 18 + EXTRA.len());
        for (k, (name, cat)) in EXTRA.iter().enumerate() {
            assert_eq!(SynthPatch::PRESETS[18 + k], *name);
            assert_eq!(SynthPatch::CATEGORIES[18 + k], *cat);
        }
        let mut seen = std::collections::HashSet::new();
        for (name, cat) in SynthPatch::PRESETS.iter().zip(SynthPatch::CATEGORIES) {
            assert!(seen.insert(*name), "duplicate preset name {name}");
            assert!(SynthPatch::CATEGORY_ORDER.contains(&cat), "{name}: {cat}");
        }
    }

    /// A held note, a rest, a chord, a high note: the engine's own render.
    fn song_for(i: usize) -> Song {
        let mut song = Song::default();
        song.instruments.clear();
        let id = song.add_channel(
            "Test",
            InstrumentKind::Synth {
                patch: SynthPatch::factory(i),
            },
        );
        std::sync::Arc::make_mut(&mut song.patterns[0]).length = 4 * BAR;
        let cat = SynthPatch::CATEGORIES[i];
        let key = if cat == "Bass" { 48 } else { 60 };
        song.add_note(1, Note::new(id, 0, BAR, key, 100));
        for (k, p) in [60u8, 64, 67].iter().enumerate() {
            song.add_note(
                1,
                Note::new(id, 2 * BAR + k as i64 * 2 * STEP, 8 * STEP, *p, 100),
            );
        }
        song.add_note(1, Note::new(id, 3 * BAR, 4 * STEP, 96, 100));
        song
    }

    #[test]
    fn every_factory_preset_is_audible_clean_and_bounded() {
        let mut bad = Vec::new();
        for i in 0..SynthPatch::PRESETS.len() {
            let name = SynthPatch::PRESETS[i];
            let p = SynthPatch::factory(i);
            p.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p, SynthPatch::factory_all()[i]);
            let song = song_for(i);
            let f = crate::engine::render(&song, 1, false, 48_000);
            let peak = f
                .iter()
                .map(|s| s[0].abs().max(s[1].abs()))
                .fold(0.0, f32::max);
            let rms = (f.iter().map(|s| (s[0] * s[0]) as f64).sum::<f64>() / f.len() as f64).sqrt();
            let dc = f.iter().map(|s| (s[0] + s[1]) as f64).sum::<f64>() / (2 * f.len()) as f64;
            let mut why = Vec::new();
            if peak < 0.02 {
                why.push(format!("silent {peak:.3}"));
            }
            if peak > 0.98 {
                why.push(format!("clips {peak:.3}"));
            }
            if !f.iter().all(|s| s[0].is_finite() && s[1].is_finite()) {
                why.push("NaN".into());
            }
            if f.iter().any(|s| s[0].is_subnormal() || s[1].is_subnormal()) {
                why.push("subnormal".into());
            }
            if dc.abs() >= 5e-3 {
                why.push(format!("DC {dc:.4}"));
            }
            if p.spread == 0.0 && p.fx.is_empty() && !f.iter().all(|s| s[0] == s[1]) {
                why.push("mono patch not centred".into());
            }
            if f != crate::engine::render(&song, 1, false, 48_000) {
                why.push("not deterministic".into());
            }
            if std::env::var_os("KFXL_PRESET_REPORT").is_some() {
                eprintln!(
                    "{i:3} {name:18} peak {peak:.3} rms {:.1} dB",
                    20.0 * rms.log10()
                );
            }
            if !why.is_empty() {
                bad.push(format!("{i} {name}: {}", why.join(", ")));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }
}
