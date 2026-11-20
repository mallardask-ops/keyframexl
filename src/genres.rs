//! Genre starter projects (docs/research/0.2.0/C4-C5.md §2): tempo, swing,
//! a drum kit from `kits`, parts on Tide presets, and a small mix with a
//! reverb return. Each builds a valid song of 4–8 bars that loops in
//! pattern mode and sits twice on the playlist.

use crate::forge::ForgeSound;
use crate::fx::{FxKind, FxSlot};
use crate::kits::Kit;
use crate::sequence::{BAR, Clip, InstrumentKind, Note, STEP, Send, Song, Tick};
use crate::synth::SynthPatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    LoFi,
    Trap,
    House,
    Synthwave,
    Chiptune,
    DrumAndBass,
    Ambient,
}

impl Genre {
    pub const ALL: [Genre; 7] = [
        Genre::LoFi,
        Genre::Trap,
        Genre::House,
        Genre::Synthwave,
        Genre::Chiptune,
        Genre::DrumAndBass,
        Genre::Ambient,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Genre::LoFi => "Lo-fi hip-hop",
            Genre::Trap => "Trap",
            Genre::House => "House",
            Genre::Synthwave => "Synthwave",
            Genre::Chiptune => "Chiptune",
            Genre::DrumAndBass => "Drum & bass",
            Genre::Ambient => "Ambient",
        }
    }

    /// One line for the Welcome card.
    pub fn blurb(self) -> &'static str {
        match self {
            Genre::LoFi => "Swung drums, Rhodes chords, warm bass",
            Genre::Trap => "Hat rolls, deep 808, bell chords",
            Genre::House => "Four on the floor, offbeat bass and stabs",
            Genre::Synthwave => "Driving drums, octave bass, arp and pads",
            Genre::Chiptune => "Console drums, square lead and fast arps",
            Genre::DrumAndBass => "Two-step breaks, sub and Reese",
            Genre::Ambient => "No drums: pads, drone, bells and air",
        }
    }

    pub fn tempo(self) -> f64 {
        match self {
            Genre::LoFi => 82.0,
            Genre::Trap => 140.0,
            Genre::House => 124.0,
            Genre::Synthwave => 104.0,
            Genre::Chiptune => 150.0,
            Genre::DrumAndBass => 174.0,
            Genre::Ambient => 64.0,
        }
    }

    pub fn build(self) -> Song {
        let mut b = Builder::new(self);
        match self {
            Genre::LoFi => lofi(&mut b),
            Genre::Trap => trap(&mut b),
            Genre::House => house(&mut b),
            Genre::Synthwave => synthwave(&mut b),
            Genre::Chiptune => chiptune(&mut b),
            Genre::DrumAndBass => dnb(&mut b),
            Genre::Ambient => ambient(&mut b),
        }
        b.finish()
    }
}

/// Mixer inserts every template uses.
const DRUMS: usize = 1;
const BASS: usize = 2;
const KEYS: usize = 3;
const LEAD: usize = 4;
const REVERB: usize = 5;

struct Builder {
    song: Song,
    bars: i64,
}

impl Builder {
    fn new(genre: Genre) -> Self {
        let mut song = Song::default();
        song.instruments.clear();
        song.tempo = genre.tempo();
        song.info.title = genre.title().into();
        song.info.genre = genre.title().into();
        let mut b = Builder { song, bars: 4 };
        b.insert(DRUMS, "Drums", vec![comp(-16.0, 3.0, 15.0, 120.0)]);
        b.insert(BASS, "Bass", vec![comp(-20.0, 4.0, 20.0, 150.0)]);
        b.insert(KEYS, "Keys", vec![]);
        b.insert(LEAD, "Lead", vec![]);
        b.insert(REVERB, "Reverb", vec![reverb(70.0, 100.0, 0.0)]);
        b.song.mixer[0].fx = vec![FxSlot::new(FxKind::Limiter)];
        b
    }

    fn insert(&mut self, at: usize, name: &str, fx: Vec<FxSlot>) {
        let t = &mut self.song.mixer[at];
        t.name = name.into();
        t.fx = fx;
    }

    fn send(&mut self, from: usize, level: f32) {
        self.song.mixer[from].sends.push(Send {
            to: REVERB as u16,
            level,
        });
    }

    /// A channel of `kit`'s piece `id`, its hits from `grid` (see `hits`).
    fn drum(&mut self, kit: Kit, id: &str, grid: &str) -> u64 {
        let piece = kit.piece(id).expect("kit piece");
        let ch = self.song.add_channel(
            piece.name,
            InstrumentKind::Forge {
                sound: ForgeSound::new(piece.patch),
            },
        );
        self.route(ch, DRUMS);
        let bars = grid.split('|').count() as i64;
        for rep in 0..(self.bars / bars).max(1) {
            for (t, vel, len) in hits(grid) {
                self.note(ch, rep * bars * BAR + t, len, crate::forge::ROOT_KEY, vel);
            }
        }
        ch
    }

    /// A Tide channel on the factory preset called `preset`.
    fn tide(&mut self, name: &str, preset: &str, insert: usize) -> u64 {
        let i = SynthPatch::PRESETS
            .iter()
            .position(|p| *p == preset)
            .unwrap_or_else(|| panic!("no Tide preset {preset}"));
        let ch = self.song.add_channel(
            name,
            InstrumentKind::Synth {
                patch: SynthPatch::factory(i),
            },
        );
        self.route(ch, insert);
        ch
    }

    fn route(&mut self, ch: u64, insert: usize) {
        if let Some(i) = self.song.instruments.iter_mut().find(|i| i.id == ch) {
            i.mixer = insert as u16;
        }
    }

    fn note(&mut self, ch: u64, at: Tick, len: Tick, pitch: u8, vel: u8) {
        self.song
            .add_note(1, Note::new(ch, at, len.max(STEP / 4), pitch, vel));
    }

    /// `chord` held from `bar`'s step `step` for `len` steps.
    fn chord(&mut self, ch: u64, bar: i64, step: i64, len: i64, chord: &[u8], vel: u8) {
        for &p in chord {
            self.note(ch, bar * BAR + step * STEP, len * STEP, p, vel);
        }
    }

    fn finish(mut self) -> Song {
        let len = self.bars * BAR;
        std::sync::Arc::make_mut(&mut self.song.patterns[0]).length = len;
        self.song.clips = vec![Clip::new(1, 0, 0), Clip::new(1, len, 0)];
        debug_assert!(self.song.validate().is_ok(), "{:?}", self.song.validate());
        self.song
    }
}

/// Hits of a drum grid: one character a 16th step, bars split by `|`.
/// `X` accent (120), `x` hit (100), `o` ghost (45), `-` a long hit (two
/// steps), `r` a 1/32 roll (two hits in the step), `.` rest.
/// Returns (tick, velocity, length).
fn hits(grid: &str) -> Vec<(Tick, u8, Tick)> {
    let mut out = Vec::new();
    for (bar, row) in grid.split('|').enumerate() {
        for (step, ch) in row.trim().chars().enumerate() {
            let at = bar as Tick * BAR + step as Tick * STEP;
            match ch {
                'X' => out.push((at, 120, STEP)),
                'x' => out.push((at, 100, STEP)),
                'o' => out.push((at, 45, STEP)),
                '-' => out.push((at, 100, 2 * STEP)),
                'r' => {
                    out.push((at, 90, STEP / 2));
                    out.push((at + STEP / 2, 75, STEP / 2));
                }
                _ => {}
            }
        }
    }
    out
}

fn comp(threshold: f32, ratio: f32, attack: f32, release: f32) -> FxSlot {
    let mut s = FxSlot::new(FxKind::Compressor);
    s.params[0] = threshold;
    s.params[1] = ratio;
    s.params[2] = attack;
    s.params[3] = release;
    s
}

fn reverb(size: f32, wet: f32, dry: f32) -> FxSlot {
    let mut s = FxSlot::new(FxKind::Reverb);
    s.params[0] = size;
    s.params[4] = wet;
    s.params[5] = dry;
    s
}

fn chorus(depth: f32, mix: f32) -> FxSlot {
    let mut s = FxSlot::new(FxKind::Chorus);
    s.params[1] = depth;
    s.params[3] = mix;
    s
}

fn delay(steps: f32, feedback: f32, wet: f32) -> FxSlot {
    let mut s = FxSlot::new(FxKind::Delay);
    s.params[0] = steps;
    s.params[1] = feedback;
    s.params[4] = wet;
    s
}

// ---- The genres

fn lofi(b: &mut Builder) {
    b.song.swing = 0.12;
    b.drum(Kit::Core, "kick", "X......x..x.....|X.....x...x..o..");
    b.drum(Kit::Core, "snare", "....X.......X...|....X.......X..o");
    b.drum(Kit::Core, "hat_closed", "x.o.x.o.x.o.x.o.|x.o.x.o.x.o.x.oo");
    b.drum(Kit::Core, "hat_open", "..............-.|..............-.");
    b.drum(Kit::Core, "rim", "..o.......o.....|....o.....o.....");
    let keys = b.tide("Keys", "Rhodes Warm", KEYS);
    let bass = b.tide("Bass", "Bass", BASS);
    b.song.mixer[KEYS].fx = vec![chorus(35.0, 30.0)];
    b.send(KEYS, 0.25);
    b.send(DRUMS, 0.12);
    let chords: [&[u8]; 4] = [
        &[53, 57, 60, 64],
        &[50, 53, 57, 60],
        &[46, 50, 53, 57],
        &[48, 52, 55, 58],
    ];
    let roots = [41u8, 38, 34, 36];
    for (bar, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = bar as i64;
        b.chord(keys, bar, 0, 6, c, 80);
        b.chord(keys, bar, 10, 3, c, 70);
        b.note(bass, bar * BAR, 7 * STEP, r, 100);
        b.note(bass, bar * BAR + 10 * STEP, 3 * STEP, r + 7, 90);
        b.note(bass, bar * BAR + 14 * STEP, 2 * STEP, r + 12, 85);
    }
}

fn trap(b: &mut Builder) {
    b.drum(Kit::Core, "kick", "X.....x...X.....|X.....x...X.....");
    b.drum(Kit::Core, "clap", "........X.......|........X.......");
    b.drum(Kit::Core, "hat_closed", "x.x.x.x.x.x.x.rr|x.x.x.x.x.x.rrrr");
    let sub = b.tide("808", "808 Boom", BASS);
    let bell = b.tide("Bells", "Glass Bell", KEYS);
    b.send(KEYS, 0.2);
    let chords: [&[u8]; 4] = [
        &[60, 63, 67, 70],
        &[56, 60, 63, 67],
        &[51, 55, 58],
        &[58, 62, 65],
    ];
    let roots = [36u8, 32, 39, 34];
    for (bar, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = bar as i64;
        b.chord(bell, bar, 0, 4, c, 70);
        b.note(sub, bar * BAR, 6 * STEP, r, 110);
        b.note(sub, bar * BAR + 6 * STEP, 4 * STEP, r, 100);
        b.note(sub, bar * BAR + 10 * STEP, 6 * STEP, r - 3, 100);
    }
}

fn house(b: &mut Builder) {
    b.song.swing = 0.06;
    b.drum(Kit::Core, "kick_909", "X...X...X...X...");
    b.drum(Kit::Core, "clap", "....X.......X...");
    b.drum(Kit::Core, "hat_closed", "x.x.x.x.x.x.x.x.");
    b.drum(Kit::Core, "hat_open", "..x...x...x...x.");
    let bass = b.tide("Bass", "Pluck Bass", BASS);
    let keys = b.tide("Stabs", "House Stab", KEYS);
    b.send(KEYS, 0.2);
    let chords: [&[u8]; 4] = [
        &[57, 60, 64, 67],
        &[53, 57, 60, 64],
        &[48, 52, 55, 59],
        &[55, 59, 62, 65],
    ];
    let roots = [45u8, 41, 48, 43];
    for (bar, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = bar as i64;
        for step in [2, 6, 10, 14] {
            b.note(bass, bar * BAR + step * STEP, 2 * STEP, r - 12, 100);
        }
        b.chord(keys, bar, 2, 2, c, 85);
        b.chord(keys, bar, 10, 2, c, 75);
    }
}

fn synthwave(b: &mut Builder) {
    b.bars = 8;
    b.drum(Kit::Core, "kick", "X.......X.......|X.......X.....x.");
    b.drum(Kit::Core, "snare_808", "....X.......X...|....X.......X...");
    b.drum(Kit::Core, "hat_closed", "x.x.x.x.x.x.x.x.|x.x.x.x.x.x.x.xx");
    let bass = b.tide("Bass", "Pluck Bass", BASS);
    let pad = b.tide("Pad", "Warm Analog", KEYS);
    let arp = b.tide("Arp", "Arp Pluck", LEAD);
    b.song.mixer[LEAD].fx = vec![delay(3.0, 35.0, 25.0)];
    b.send(KEYS, 0.3);
    b.send(DRUMS, 0.15);
    let chords: [&[u8]; 4] = [&[57, 60, 64], &[53, 57, 60], &[48, 52, 55], &[55, 59, 62]];
    let roots = [33u8, 29, 36, 31];
    for (k, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = 2 * k as i64;
        b.chord(pad, bar, 0, 32, c, 70);
        for step in 0..32 {
            let at = bar * BAR + step * STEP;
            if step % 2 == 0 {
                let p = if step % 4 == 0 { r } else { r + 12 };
                b.note(bass, at, STEP, p, 100);
            }
            let tone = c[step as usize % c.len()] + if step % 8 >= 4 { 12 } else { 0 };
            b.note(arp, at, STEP, tone, 80);
        }
    }
}

fn chiptune(b: &mut Builder) {
    b.drum(Kit::Chip, "kick", "X.....X.X.......|X.....X.X.......");
    b.drum(Kit::Chip, "snare", "....X.......X...|....X.......X...");
    b.drum(Kit::Chip, "hat_closed", "x.x.x.x.x.x.x.x.|x.x.x.x.x.x.x.x.");
    let lead = b.tide("Lead", "NES Square", LEAD);
    let arp = b.tide("Arp", "Chip Arp", KEYS);
    let bass = b.tide("Bass", "Triangle Bass", BASS);
    let chords: [&[u8]; 4] = [&[60, 64, 67], &[57, 60, 64], &[53, 57, 60], &[55, 59, 62]];
    let roots = [48u8, 45, 41, 43];
    let melody = [72u8, 76, 79, 84, 83, 79, 76, 74];
    for (bar, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = bar as i64;
        for (k, p) in melody.iter().enumerate() {
            let p = if bar % 2 == 1 { p - 3 } else { *p };
            b.note(lead, bar * BAR + 2 * k as i64 * STEP, 2 * STEP, p, 90);
        }
        for step in 0..16 {
            b.note(
                arp,
                bar * BAR + step * STEP,
                STEP,
                c[step as usize % 3] + 12,
                70,
            );
        }
        b.note(bass, bar * BAR, 8 * STEP, r - 12, 100);
        b.note(bass, bar * BAR + 8 * STEP, 8 * STEP, r - 12, 100);
    }
}

fn dnb(b: &mut Builder) {
    b.song.swing = 0.1;
    b.drum(Kit::Core, "kick", "X.........X.....|X.........X.....");
    b.drum(Kit::Core, "snare", "....X.......X...|....X.......X...");
    b.drum(Kit::Core, "hat_closed", "x.x.x.x.x.x.x.x.|x.x.x.x.x.x.x.x.");
    b.drum(Kit::Core, "snare_808", "..o...o...o...o.|..o...o...o..rr.");
    let sub = b.tide("Sub", "Pure Sub", BASS);
    let reese = b.tide("Reese", "Reese", LEAD);
    let pad = b.tide("Atmos", "Sweep Pad", KEYS);
    b.send(KEYS, 0.35);
    let chords: [&[u8]; 4] = [
        &[56, 60, 63, 67],
        &[49, 53, 56, 60],
        &[56, 60, 63],
        &[51, 55, 58],
    ];
    let roots = [29u8, 25, 32, 27];
    for (bar, (c, r)) in chords.iter().zip(roots).enumerate() {
        let bar = bar as i64;
        b.note(sub, bar * BAR, 14 * STEP, r + 12, 110);
        b.note(reese, bar * BAR, 16 * STEP, r + 24, 80);
        b.chord(pad, bar, 0, 16, c, 60);
    }
}

fn ambient(b: &mut Builder) {
    b.bars = 8;
    let pad = b.tide("Pad", "Glass Pad", KEYS);
    let drone = b.tide("Drone", "Dark Drone", BASS);
    let bell = b.tide("Bells", "Tubular", LEAD);
    b.send(KEYS, 0.5);
    b.send(LEAD, 0.6);
    b.send(BASS, 0.2);
    b.song.mixer[REVERB].fx = vec![reverb(100.0, 100.0, 0.0)];
    let chords: [&[u8]; 4] = [
        &[50, 53, 57, 64],
        &[46, 50, 53, 57],
        &[43, 46, 50, 53],
        &[45, 50, 52, 55],
    ];
    let roots = [38u8, 34, 31, 33];
    let bells = [Some(74u8), None, Some(81), None];
    for (k, ((c, r), bl)) in chords.iter().zip(roots).zip(bells).enumerate() {
        let bar = 2 * k as i64;
        b.chord(pad, bar, 0, 32, c, 70);
        b.note(drone, bar * BAR, 32 * STEP, r, 80);
        if let Some(p) = bl {
            b.note(bell, bar * BAR + 8 * STEP, 8 * STEP, p, 60);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_read_as_written() {
        let h = hits("X.o-|r...");
        assert_eq!(h[0], (0, 120, STEP));
        assert_eq!(h[1], (2 * STEP, 45, STEP));
        assert_eq!(h[2], (3 * STEP, 100, 2 * STEP));
        assert_eq!(h[3], (BAR, 90, STEP / 2));
        assert_eq!(h[4], (BAR + STEP / 2, 75, STEP / 2));
    }

    #[test]
    fn every_genre_builds_a_valid_song_that_sounds() {
        for g in Genre::ALL {
            let song = g.build();
            song.validate().unwrap_or_else(|e| panic!("{g:?}: {e}"));
            assert_eq!(song.tempo, g.tempo());
            assert!(song.instruments.len() >= 3, "{g:?}");
            assert!(song.patterns[0].notes.len() > 8, "{g:?}");
            let audio = crate::engine::render(&song, 1, false, 22_050);
            let peak = audio
                .iter()
                .map(|f| f[0].abs().max(f[1].abs()))
                .fold(0.0, f32::max);
            assert!(peak > 0.05, "{g:?} is silent ({peak})");
            assert!(
                audio.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
                "{g:?}"
            );
        }
    }
}
