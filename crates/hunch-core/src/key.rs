use std::fmt;

const PITCH_CLASS_COUNT: u8 = 12;
const SHARP_NAMES: [&str; PITCH_CLASS_COUNT as usize] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// One of the 12 pitch classes, 0 = C through 11 = B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PitchClass(u8);

impl PitchClass {
    /// Returns `None` if `index` is not in `0..12`.
    pub fn new(index: u8) -> Option<Self> {
        (index < PITCH_CLASS_COUNT).then_some(Self(index))
    }

    pub fn index(self) -> u8 {
        self.0
    }
}

impl fmt::Display for PitchClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SHARP_NAMES[usize::from(self.0)])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Major,
    Minor,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Major => f.write_str("major"),
            Mode::Minor => f.write_str("minor"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub tonic: PitchClass,
    pub mode: Mode,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.tonic, self.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    #[test]
    fn displays_keys_with_sharps() {
        assert_eq!(key(0, Mode::Major).to_string(), "C major");
        assert_eq!(key(6, Mode::Major).to_string(), "F# major");
        assert_eq!(key(3, Mode::Minor).to_string(), "D# minor");
        assert_eq!(key(11, Mode::Minor).to_string(), "B minor");
    }

    #[test]
    fn rejects_out_of_range_pitch_class() {
        assert!(PitchClass::new(11).is_some());
        assert!(PitchClass::new(12).is_none());
    }
}
