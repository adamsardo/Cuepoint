use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::BeatGrid;

/// Identifier of a track within one library. rekordbox uses decimal strings
/// in `master.db` and integers in XML; both are kept as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(pub String);

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tempo in hundredths of a beat per minute, the precision rekordbox stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bpm {
    centi: u32,
}

impl Bpm {
    pub fn from_centi(centi: u32) -> Self {
        Self { centi }
    }

    /// Rounds to the nearest hundredth. Returns `None` for non-finite or
    /// non-positive values.
    pub fn from_f64(bpm: f64) -> Option<Self> {
        (bpm.is_finite() && bpm > 0.0).then(|| Self::from_centi((bpm * 100.0).round() as u32))
    }

    pub fn centi(self) -> u32 {
        self.centi
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.centi) / 100.0
    }
}

impl fmt::Display for Bpm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.centi / 100, self.centi % 100)
    }
}

/// Star rating, 0 to 5.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rating(u8);

impl Rating {
    /// Clamps to 0..=5.
    pub fn new(stars: u8) -> Self {
        Self(stars.min(5))
    }

    pub fn stars(self) -> u8 {
        self.0
    }
}

/// One of the colour labels a user can give a track.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackColour {
    pub name: String,
    /// `0xRRGGBB`, when the source records it.
    pub rgb: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: TrackId,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub comment: String,
    /// Musical key as the source names it, e.g. `8A` or `Am`.
    pub key: String,
    pub bpm: Option<Bpm>,
    pub duration: Option<Duration>,
    pub rating: Rating,
    pub colour: Option<TrackColour>,
    pub year: Option<u16>,
    pub play_count: u32,
    pub bit_rate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub file_size: Option<u64>,
    /// File type as the source names it, e.g. `MP3 File`.
    pub kind: String,
    /// `YYYY-MM-DD` when known.
    pub date_added: String,
    pub location: Option<PathBuf>,
    /// Grid from the source library, empty when it has none.
    pub beatgrid: BeatGrid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_round_trips_hundredths() {
        let bpm = Bpm::from_f64(127.996).unwrap();
        assert_eq!(bpm.centi(), 12800);
        assert_eq!(bpm.to_string(), "128.00");
        assert_eq!(Bpm::from_centi(9050).to_string(), "90.50");
        assert_eq!(Bpm::from_f64(0.0), None);
        assert_eq!(Bpm::from_f64(f64::NAN), None);
    }

    #[test]
    fn rating_clamps() {
        assert_eq!(Rating::new(9).stars(), 5);
    }
}
