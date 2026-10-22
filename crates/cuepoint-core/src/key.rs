//! Musical keys and their DJ notations.

use std::fmt;

const NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// One of the 24 major and minor keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicalKey {
    /// Pitch class of the tonic, 0 = C .. 11 = B.
    tonic: u8,
    minor: bool,
}

impl MusicalKey {
    pub fn new(tonic: u8, minor: bool) -> Self {
        Self {
            tonic: tonic % 12,
            minor,
        }
    }

    pub fn tonic(self) -> u8 {
        self.tonic
    }

    pub fn is_minor(self) -> bool {
        self.minor
    }

    /// Camelot wheel notation, e.g. `8A` for A minor, `8B` for C major.
    pub fn camelot(self) -> String {
        // Each step round the wheel is a fifth (7 semitones); C major is 8B
        // and a minor key shares its relative major's number.
        let major_tonic = if self.minor {
            (self.tonic + 3) % 12
        } else {
            self.tonic
        };
        let number = (u32::from(major_tonic) * 7 % 12 + 7) % 12 + 1;
        format!("{number}{}", if self.minor { 'A' } else { 'B' })
    }

    /// Classic name, e.g. `Am`, `F#`.
    pub fn name(self) -> String {
        format!(
            "{}{}",
            NAMES[usize::from(self.tonic)],
            if self.minor { "m" } else { "" }
        )
    }

    /// Parses Camelot (`8A`), Open Key (`1m`, `1d`) or classic names
    /// (`Am`, `A minor`, `F#`, `Gbmin`, `Bb`).
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.is_empty() {
            return None;
        }
        let (digits, rest): (String, String) = t.chars().partition(|c| c.is_ascii_digit());
        if !digits.is_empty() && t.starts_with(|c: char| c.is_ascii_digit()) {
            let number: u8 = digits.parse().ok().filter(|n| (1..=12).contains(n))?;
            let letter = rest.to_ascii_lowercase();
            return match letter.as_str() {
                // Open Key 1d = C major = 8B; 1m = A minor = 8A.
                "a" | "b" => Self::from_camelot(number, letter == "a"),
                "m" | "d" => Self::from_camelot((number + 6) % 12 + 1, letter == "m"),
                _ => None,
            };
        }
        let mut chars = t.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        let natural = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest = chars.as_str();
        let (shift, quality) = match rest.chars().next() {
            Some(c @ ('#' | '♯')) => (1, &rest[c.len_utf8()..]),
            Some(c @ ('b' | '♭')) => (11, &rest[c.len_utf8()..]),
            _ => (0, rest),
        };
        let quality = quality.trim().to_ascii_lowercase();
        let minor = match quality.as_str() {
            "" | "maj" | "major" => false,
            "m" | "min" | "minor" => true,
            _ => return None,
        };
        Some(Self::new((natural + shift) % 12, minor))
    }

    fn from_camelot(number: u8, minor: bool) -> Option<Self> {
        (0..12)
            .map(|tonic| Self::new(tonic, minor))
            .find(|key| key.camelot() == format!("{number}{}", if minor { 'A' } else { 'B' }))
    }
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camelot_wheel() {
        let cases = [
            (MusicalKey::new(9, true), "8A"),  // A minor
            (MusicalKey::new(0, false), "8B"), // C major
            (MusicalKey::new(11, false), "1B"),
            (MusicalKey::new(8, true), "1A"), // G#/Ab minor
            (MusicalKey::new(7, false), "9B"),
            (MusicalKey::new(4, true), "9A"),
            (MusicalKey::new(5, false), "7B"),
            (MusicalKey::new(6, true), "11A"),
            (MusicalKey::new(4, false), "12B"),
        ];
        for (key, camelot) in cases {
            assert_eq!(key.camelot(), camelot, "{key}");
        }
    }

    #[test]
    fn parses_common_notations() {
        let am = MusicalKey::new(9, true);
        for text in ["8A", "8a", "Am", "A minor", "Amin", "1m"] {
            assert_eq!(MusicalKey::parse(text), Some(am), "{text}");
        }
        assert_eq!(MusicalKey::parse("Bb"), Some(MusicalKey::new(10, false)));
        assert_eq!(MusicalKey::parse("F#m"), Some(MusicalKey::new(6, true)));
        assert_eq!(MusicalKey::parse("Ebm"), Some(MusicalKey::new(3, true)));
        assert_eq!(MusicalKey::parse("1d"), Some(MusicalKey::new(0, false)));
        assert_eq!(MusicalKey::parse("13A"), None);
        assert_eq!(MusicalKey::parse("H"), None);
        assert_eq!(MusicalKey::parse(""), None);
    }

    #[test]
    fn every_key_round_trips_through_camelot() {
        for tonic in 0..12 {
            for minor in [false, true] {
                let key = MusicalKey::new(tonic, minor);
                assert_eq!(MusicalKey::parse(&key.camelot()), Some(key));
                assert_eq!(MusicalKey::parse(&key.name()), Some(key));
            }
        }
    }
}
