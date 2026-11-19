//! Musical scales for the piano roll: which rows to highlight, where a new
//! note snaps (FL: Scale highlighting / "Snap to scale").

/// A scale as the semitones above its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Major,
    Minor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Major,
        Kind::Minor,
        Kind::HarmonicMinor,
        Kind::MelodicMinor,
        Kind::Dorian,
        Kind::Phrygian,
        Kind::Lydian,
        Kind::Mixolydian,
        Kind::Locrian,
        Kind::MajorPentatonic,
        Kind::MinorPentatonic,
        Kind::Blues,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Major => "Major",
            Kind::Minor => "Minor",
            Kind::HarmonicMinor => "Harmonic minor",
            Kind::MelodicMinor => "Melodic minor",
            Kind::Dorian => "Dorian",
            Kind::Phrygian => "Phrygian",
            Kind::Lydian => "Lydian",
            Kind::Mixolydian => "Mixolydian",
            Kind::Locrian => "Locrian",
            Kind::MajorPentatonic => "Major pentatonic",
            Kind::MinorPentatonic => "Minor pentatonic",
            Kind::Blues => "Blues",
        }
    }

    pub fn steps(self) -> &'static [u8] {
        match self {
            Kind::Major => &[0, 2, 4, 5, 7, 9, 11],
            Kind::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Kind::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Kind::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Kind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Kind::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Kind::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Kind::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Kind::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Kind::MajorPentatonic => &[0, 2, 4, 7, 9],
            Kind::MinorPentatonic => &[0, 3, 5, 7, 10],
            Kind::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

/// A key: root pitch class (0 = C) and scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Scale {
    pub root: u8,
    pub kind: Kind,
}

pub const ROOTS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

impl Scale {
    pub fn contains(self, pitch: u8) -> bool {
        let class = (pitch as i32 - self.root as i32).rem_euclid(12) as u8;
        self.kind.steps().contains(&class)
    }

    pub fn is_root(self, pitch: u8) -> bool {
        pitch % 12 == self.root % 12
    }

    /// The nearest pitch in the scale; a tie goes down (FL keeps the lower note).
    pub fn snap(self, pitch: u8) -> u8 {
        if self.contains(pitch) {
            return pitch;
        }
        for d in 1..=6u8 {
            if let Some(p) = pitch.checked_sub(d)
                && self.contains(p)
            {
                return p;
            }
            if let Some(p) = pitch.checked_add(d)
                && p <= 127
                && self.contains(p)
            {
                return p;
            }
        }
        pitch
    }

    pub fn name(self) -> String {
        format!("{} {}", ROOTS[(self.root % 12) as usize], self.kind.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_major_is_the_white_keys() {
        let s = Scale {
            root: 0,
            kind: Kind::Major,
        };
        let white: Vec<u8> = (60..72).filter(|p| s.contains(*p)).collect();
        assert_eq!(white, vec![60, 62, 64, 65, 67, 69, 71]);
    }

    #[test]
    fn a_minor_relative_to_c_major_and_roots() {
        let am = Scale {
            root: 9,
            kind: Kind::Minor,
        };
        let c = Scale {
            root: 0,
            kind: Kind::Major,
        };
        assert!((0..128u8).all(|p| am.contains(p) == c.contains(p)));
        assert!(am.is_root(57) && am.is_root(69) && !am.is_root(60));
    }

    #[test]
    fn snap_goes_to_the_nearest_and_down_on_a_tie() {
        let c = Scale {
            root: 0,
            kind: Kind::Major,
        };
        assert_eq!(c.snap(61), 60, "C# between C and D: tie goes down");
        assert_eq!(c.snap(66), 65, "F# between F and G: down");
        assert_eq!(c.snap(64), 64);
        let pent = Scale {
            root: 0,
            kind: Kind::MinorPentatonic,
        };
        assert_eq!(pent.snap(61), 60);
        assert_eq!(pent.snap(62), 63, "D is closer to Eb than to C");
        assert_eq!(pent.snap(127), 127.min(pent.snap(127)));
        assert!(pent.contains(pent.snap(0)));
    }
}
