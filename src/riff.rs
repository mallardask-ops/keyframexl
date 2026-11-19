//! Piano-roll riff generation. Times in the processing chain are relative to the phrase start.
use crate::{
    note_ops,
    scale::{Kind, Scale},
    sequence::{BAR, Note, PPQ, STEP, Tick},
};

pub const STAGES: [&str; 8] = [
    "Progression",
    "Chords",
    "Arpeggio",
    "Mirror",
    "Levels",
    "Articulation",
    "Groove",
    "Fit",
];
pub const PROGRESSIONS: [&str; 5] = [
    "I · V · vi · IV",
    "i · VI · III · VII",
    "ii · V · I · vi",
    "I · IV · V · I",
    "Seeded walk",
];
pub const RATES: [(&str, Tick); 5] = [
    ("1/4", PPQ),
    ("1/8", PPQ / 2),
    ("1/8 triplet", PPQ / 3),
    ("1/16", STEP),
    ("1/32", STEP / 2),
];
pub const RHYTHMS: [&str; 4] = ["Straight", "Tresillo", "Offbeat", "Gallop"];
pub const DIRECTIONS: [&str; 4] = ["Up", "Down", "Up / down", "Random"];

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Recipe {
    pub enabled: [bool; 8],
    pub stage_seeds: [u32; 8],
    pub grid: usize,
    pub snap: u8,
    pub groove_end: usize,
    pub bars: u8,
    pub seed: u32,
    pub scale: Scale,
    pub progression: usize,
    pub changes: u8,
    pub voices: u8,
    pub inversion: u8,
    pub rhythm: usize,
    pub rate: usize,
    pub direction: usize,
    pub octaves: u8,
    pub continuous: bool,
    pub reverse: bool,
    pub invert: bool,
    pub velocity: u8,
    pub pan: u8,
    pub release: u8,
    pub fine: u8,
    pub mod_x: u8,
    pub mod_y: u8,
    pub gate: u8,
    pub variation: u8,
    pub swing: u8,
    pub jitter: u8,
    pub bottom: u8,
    pub top: u8,
}
impl Default for Recipe {
    fn default() -> Self {
        Self {
            enabled: [true, true, true, false, true, true, false, true],
            stage_seeds: [0; 8],
            grid: 3,
            snap: 80,
            groove_end: 0,
            bars: 4,
            seed: 1,
            scale: Scale {
                root: 0,
                kind: Kind::Minor,
            },
            progression: 1,
            changes: 1,
            voices: 3,
            inversion: 0,
            rhythm: 0,
            rate: 1,
            direction: 2,
            octaves: 1,
            continuous: false,
            reverse: true,
            invert: false,
            velocity: 12,
            pan: 7,
            release: 0,
            fine: 0,
            mod_x: 0,
            mod_y: 0,
            gate: 85,
            variation: 8,
            swing: 24,
            jitter: 3,
            bottom: 48,
            top: 83,
        }
    }
}

struct Random(u64);
impl Random {
    fn new(seed: u32, stage: u64) -> Self {
        Self((u64::from(seed) << 8) | stage)
    }
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn offset(&mut self, amount: u8) -> i32 {
        (self.next() % (2 * u32::from(amount) + 1)) as i32 - i32::from(amount)
    }
}

impl Recipe {
    pub fn roll_dice(&mut self) {
        let mut r = Random::new(self.seed, 91);
        self.seed = r.next();
        self.progression = (r.next() % 5) as usize;
        self.changes = 1 + (r.next() % 2) as u8;
        self.voices = 2 + (r.next() % 3) as u8;
        self.inversion = (r.next() % u32::from(self.voices)) as u8;
        self.rhythm = (r.next() % 4) as usize;
        self.direction = (r.next() % 4) as usize;
        self.rate = (r.next() % 4) as usize;
        self.gate = 55 + (r.next() % 46) as u8;
        self.swing = (r.next() % 50) as u8;
    }
    pub fn vary_stage(&mut self, tab: usize) {
        self.stage_seeds[tab] = self.stage_seeds[tab].wrapping_add(1);
        let mut r = Random::new(
            self.seed.wrapping_add(self.stage_seeds[tab]),
            tab as u64 + 10,
        );
        match tab {
            0 => {
                self.progression = r.next() as usize % 5;
                self.changes = 1 + (r.next() % 4) as u8;
            }
            1 => {
                self.voices = 2 + (r.next() % 4) as u8;
                self.inversion = (r.next() % u32::from(self.voices)) as u8;
            }
            2 => {
                self.rhythm = r.next() as usize % 4;
                self.direction = r.next() as usize % 4;
                self.rate = r.next() as usize % 5;
                self.octaves = 1 + (r.next() % 3) as u8;
            }
            3 => {
                let axes = 1 + r.next() % 3;
                self.invert = axes & 1 != 0;
                self.reverse = axes & 2 != 0;
            }
            4 => {
                self.velocity = 4 + (r.next() % 32) as u8;
                self.pan = (r.next() % 50) as u8;
            }
            5 => {
                self.gate = 35 + (r.next() % 91) as u8;
                self.variation = (r.next() % 32) as u8;
            }
            6 => {
                self.swing = (r.next() % 66) as u8;
                self.jitter = (r.next() % 20) as u8;
                self.grid = r.next() as usize % 5;
            }
            _ => {
                self.bottom = 36 + (r.next() % 25) as u8;
                self.top = self.bottom + 12 + (r.next() % 25) as u8;
            }
        }
    }

    pub fn valid(&self) -> bool {
        (1..=16).contains(&self.bars)
            && self.scale.root < 12
            && self.progression < PROGRESSIONS.len()
            && (1..=4).contains(&self.changes)
            && (2..=5).contains(&self.voices)
            && self.inversion < self.voices
            && self.rhythm < RHYTHMS.len()
            && self.rate < RATES.len()
            && self.direction < DIRECTIONS.len()
            && (1..=3).contains(&self.octaves)
            && self.velocity <= 64
            && [self.pan, self.fine, self.release, self.mod_x, self.mod_y]
                .iter()
                .all(|v| *v <= 100)
            && (10..=180).contains(&self.gate)
            && self.variation <= 60
            && self.swing <= 75
            && self.grid < RATES.len()
            && self.snap <= 100
            && self.groove_end < 3
            && self.jitter <= 50
            && self.top <= 127
            && self.top as i16 - self.bottom as i16 >= 11
    }

    pub fn reset_stage(&mut self, tab: usize) {
        let d = Self::default();
        self.stage_seeds[tab] = 0;
        match tab {
            0 => {
                (self.progression, self.changes) = (d.progression, d.changes);
            }
            1 => {
                (self.voices, self.inversion) = (d.voices, d.inversion);
            }
            2 => {
                (
                    self.rhythm,
                    self.direction,
                    self.rate,
                    self.octaves,
                    self.continuous,
                ) = (d.rhythm, d.direction, d.rate, d.octaves, d.continuous);
            }
            3 => {
                (self.reverse, self.invert) = (d.reverse, d.invert);
            }
            4 => {
                (
                    self.velocity,
                    self.pan,
                    self.release,
                    self.fine,
                    self.mod_x,
                    self.mod_y,
                ) = (d.velocity, d.pan, 0, 0, 0, 0);
            }
            5 => {
                (self.gate, self.variation) = (d.gate, d.variation);
            }
            6 => {
                (
                    self.swing,
                    self.jitter,
                    self.grid,
                    self.snap,
                    self.groove_end,
                ) = (d.swing, d.jitter, d.grid, d.snap, d.groove_end);
            }
            _ => {
                (self.scale, self.bottom, self.top) = (d.scale, d.bottom, d.top);
            }
        }
    }

    pub fn length(&self) -> Tick {
        Tick::from(self.bars) * BAR
    }

    pub fn generate(
        &self,
        inst: u64,
        source: Option<&[Note]>,
        span: Tick,
        through: usize,
    ) -> Vec<Note> {
        let active = |i: usize| i <= through && self.enabled[i];
        let scale = self.scale;
        let steps = scale.kind.steps();
        let degree = |n: usize| (12 * (n / steps.len()) + steps[n % steps.len()] as usize) as i16;
        let mut notes = if let Some(score) = source {
            score.to_vec()
        } else if active(0) {
            let stride = BAR / Tick::from(self.changes);
            let mut r = Random::new(self.seed.wrapping_add(self.stage_seeds[0]), 1);
            let templates = [[0, 4, 5, 3], [0, 5, 2, 6], [1, 4, 0, 5], [0, 3, 4, 0]];
            (0..span)
                .step_by(stride as usize)
                .enumerate()
                .map(|(i, t)| {
                    let d = if self.progression == 4 {
                        if i == 0 {
                            0
                        } else {
                            (r.next() as usize) % steps.len()
                        }
                    } else {
                        templates[self.progression][i % 4]
                    };
                    Note::new(
                        inst,
                        t,
                        stride.min(span - t),
                        (48 + scale.root as i16 + degree(d)) as u8,
                        96,
                    )
                })
                .collect()
        } else {
            vec![Note::new(inst, 0, span, 48 + scale.root, 96)]
        };
        if active(1) {
            notes = notes
                .into_iter()
                .flat_map(|root| {
                    let pc = (i16::from(scale.snap(root.pitch)) - i16::from(scale.root))
                        .rem_euclid(12) as u8;
                    let d = steps.iter().position(|&s| s == pc).unwrap_or(0);
                    (0..self.voices).map(move |v| {
                        let shift = degree(d + 2 * v as usize) - degree(d)
                            + if v < self.inversion.min(self.voices - 1) {
                                12
                            } else {
                                0
                            };
                        Note {
                            pitch: (i16::from(root.pitch) + shift).min(127) as u8,
                            ..root
                        }
                    })
                })
                .collect();
        }
        if active(2) {
            let rate = RATES[self.rate].1;
            let mut r = Random::new(self.seed.wrapping_add(self.stage_seeds[2]), 3);
            notes.sort_by_key(|n| (n.start, n.pitch));
            let mut melody = Vec::new();
            for chord in notes.chunk_by(|a, b| a.start == b.start) {
                let count = chord.len() * self.octaves as usize;
                let end = chord.iter().map(Note::end).max().unwrap();
                for (j, t) in (chord[0].start..end).step_by(rate as usize).enumerate() {
                    let step = if self.continuous {
                        (t / rate) as usize
                    } else {
                        j
                    };
                    let mask = [0xffu8, 0x49, 0xaa, 0xdd][self.rhythm];
                    if mask & (1 << (step % 8)) == 0 {
                        continue;
                    }
                    let index = match self.direction {
                        1 => count - 1 - step % count,
                        2 if count > 1 => {
                            let period = 2 * (count - 1);
                            let pos = step % period;
                            pos.min(period - pos)
                        }
                        3 => r.next() as usize % count,
                        _ => step % count,
                    };
                    let n = chord[index % chord.len()];
                    melody.push(Note {
                        start: t,
                        length: rate.min(end - t),
                        pitch: (n.pitch as usize + 12 * (index / chord.len())).min(127) as u8,
                        ..n
                    });
                }
            }
            notes = melody;
        }
        if active(3) {
            if self.reverse {
                note_ops::flip(&mut notes, false);
            }
            if self.invert {
                note_ops::flip(&mut notes, true);
            }
        }
        let mut levels = Random::new(self.seed.wrapping_add(self.stage_seeds[4]), 5);
        let mut articulation = Random::new(self.seed.wrapping_add(self.stage_seeds[5]), 6);
        let mut groove = Random::new(self.seed.wrapping_add(self.stage_seeds[6]), 7);
        for n in &mut notes {
            if active(4) {
                n.velocity = (n.velocity as i32 + levels.offset(self.velocity)).clamp(1, 127) as u8;
                n.pan = (n.pan + levels.offset(self.pan) as f32 / 100.).clamp(-1., 1.);
                n.release = (n.release + levels.offset(self.release) as f32 / 100.).clamp(0., 1.);
                n.fine = (n.fine + levels.offset(self.fine) as f32).clamp(-100., 100.);
                n.mod_x = (n.mod_x + levels.offset(self.mod_x) as f32 / 100.).clamp(0., 1.);
                n.mod_y = (n.mod_y + levels.offset(self.mod_y) as f32 / 100.).clamp(0., 1.);
            }
            if active(5) {
                n.length = (n.length
                    * i64::from(self.gate as i32 + articulation.offset(self.variation))
                    / 100)
                    .max(1);
            }
            if active(6) {
                let grid = RATES[self.grid].1;
                let end = n.end();
                let nearest = ((n.start + grid / 2) / grid) * grid;
                let swing = (nearest / grid % 2) * grid * Tick::from(self.swing) / 100;
                n.start = (n.start
                    + (nearest - n.start) * Tick::from(self.snap) / 100
                    + swing
                    + i64::from(groove.offset(self.jitter)))
                .clamp(0, span - 1);
                if self.groove_end == 1 {
                    n.length = (end - n.start).max(1);
                }
                if self.groove_end == 2 {
                    let to = ((end + grid / 2) / grid) * grid;
                    n.length = (end + (to - end) * Tick::from(self.snap) / 100 - n.start).max(1);
                }
            }
            if active(7) {
                let mut pitch = i16::from(n.pitch);
                while pitch > self.top as i16 {
                    pitch -= 12;
                }
                while pitch < (self.bottom as i16) {
                    pitch += 12;
                }
                let p = pitch.clamp(self.bottom as i16, self.top as i16) as u8;
                n.pitch = (self.bottom..=self.top)
                    .filter(|v| scale.contains(*v))
                    .min_by_key(|v| v.abs_diff(p))
                    .unwrap_or(p);
            }
            n.length = n.length.clamp(1, span - n.start);
        }
        notes.sort_by_key(|n| (n.start, n.pitch));
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_bypass_and_seeded_recipes_reproduce_the_score() {
        let mut r = Recipe::default();
        let n = Note::new(4, 0, BAR, 60, 96);
        r.enabled = [false; 8];
        assert_eq!(r.generate(4, Some(&[n]), BAR, 7), vec![n]);
        r.enabled = [true; 8];
        r.rhythm = 3;
        r.invert = true;
        r.pan = 90;
        for seed in 0..30 {
            r.seed = seed;
            r.roll_dice();
            assert!(r.valid());
            let saved = serde_json::to_string(&r).unwrap();
            let copy: Recipe = serde_json::from_str(&saved).unwrap();
            let notes = r.generate(4, None, r.length(), 7);
            assert_eq!(notes, copy.generate(4, None, r.length(), 7));
            assert!(!notes.is_empty());
            assert!(notes.iter().all(|n| n.start >= 0
                && n.length > 0
                && n.end() <= r.length()
                && r.scale.contains(n.pitch)
                && (r.bottom..=r.top).contains(&n.pitch)));
        }
    }
    #[test]
    fn each_stage_can_be_auditioned_independently() {
        let r = Recipe::default();
        let roots = r.generate(4, None, r.length(), 0);
        let chords = r.generate(4, None, r.length(), 1);
        let arp = r.generate(4, None, r.length(), 2);
        assert_eq!(roots.len(), 4);
        assert_eq!(chords.len(), 12);
        assert_eq!(arp.len(), 32);
        for stage in 3..8 {
            let mut r = Recipe::default();
            r.enabled = [false; 8];
            r.enabled[stage] = true;
            r.scale.root = 1;
            r.pan = 50;
            r.jitter = 32;
            r.invert = true;
            let score = [
                Note::new(4, 0, STEP * 2, 60, 99),
                Note::new(4, STEP * 3, STEP, 66, 85),
            ];
            assert_ne!(r.generate(4, Some(&score), BAR, stage), score);
        }
    }
}
