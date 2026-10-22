//! Beat grids: where the beats and bars of a track fall.
//!
//! A grid is a list of tempo markers, matching rekordbox's model (XML
//! `TEMPO` elements: `Inizio` start time, `Bpm`, `Battito` beat in bar).
//! Most tracks have one marker; variable-tempo tracks have several. Times
//! are in seconds so a grid is independent of any sample rate.

/// Beats per bar. rekordbox grids are always 4/4.
pub const BEATS_PER_BAR: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMarker {
    /// Time of this marker's beat, seconds from the start of the track.
    pub time: f64,
    pub bpm: f64,
    /// Position of this beat within its bar, 1..=4 (1 is the downbeat).
    pub beat_in_bar: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatGrid {
    /// Sorted by time; never empty for a usable grid.
    markers: Vec<TempoMarker>,
}

impl BeatGrid {
    /// A constant-tempo grid with a beat at `first_beat` seconds.
    pub fn constant(bpm: f64, first_beat: f64, beat_in_bar: u32) -> Self {
        Self::from_markers(vec![TempoMarker {
            time: first_beat,
            bpm,
            beat_in_bar,
        }])
    }

    /// Markers with non-positive or non-finite tempo are dropped; the rest
    /// are sorted by time.
    pub fn from_markers(mut markers: Vec<TempoMarker>) -> Self {
        markers.retain(|m| m.bpm.is_finite() && m.bpm > 0.0 && m.time.is_finite());
        markers.sort_by(|a, b| a.time.total_cmp(&b.time));
        for marker in &mut markers {
            marker.beat_in_bar = marker.beat_in_bar.clamp(1, BEATS_PER_BAR);
        }
        Self { markers }
    }

    pub fn markers(&self) -> &[TempoMarker] {
        &self.markers
    }

    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }

    /// Index of the marker governing `time`.
    fn segment(&self, time: f64) -> usize {
        self.markers
            .partition_point(|m| m.time <= time)
            .saturating_sub(1)
    }

    /// Beat count at the start of each marker, counting the first marker's
    /// beat as 0.
    fn beats_at_markers(&self) -> impl Iterator<Item = f64> + '_ {
        let mut beats = 0.0;
        self.markers.iter().enumerate().map(move |(i, m)| {
            let here = beats;
            if let Some(next) = self.markers.get(i + 1) {
                beats += (next.time - m.time) * m.bpm / 60.0;
            }
            here
        })
    }

    /// Fractional beat position at `time`; whole numbers are beats. Before
    /// the first marker the first tempo is extrapolated (negative beats).
    pub fn beat_at(&self, time: f64) -> Option<f64> {
        let i = self.segment(time);
        let marker = self.markers.get(i)?;
        let base = self.beats_at_markers().nth(i)?;
        Some(base + (time - marker.time) * marker.bpm / 60.0)
    }

    /// Time of fractional beat position `beat`, the inverse of `beat_at`.
    pub fn time_of_beat(&self, beat: f64) -> Option<f64> {
        let starts: Vec<f64> = self.beats_at_markers().collect();
        let i = starts.partition_point(|&b| b <= beat).saturating_sub(1);
        let marker = self.markers.get(i)?;
        Some(marker.time + (beat - starts[i]) * 60.0 / marker.bpm)
    }

    pub fn bpm_at(&self, time: f64) -> Option<f64> {
        self.markers.get(self.segment(time)).map(|m| m.bpm)
    }

    /// Whether whole beat `beat` (as counted by `beat_at`) is a downbeat.
    pub fn is_downbeat(&self, beat: i64) -> bool {
        let Some(first) = self.markers.first() else {
            return false;
        };
        let bar = i64::from(BEATS_PER_BAR);
        (beat + i64::from(first.beat_in_bar) - 1).rem_euclid(bar) == 0
    }

    /// Times of every whole beat in `start..end` seconds, with a downbeat
    /// flag, for drawing grid lines.
    pub fn beats_between(&self, start: f64, end: f64) -> Vec<(f64, bool)> {
        let (Some(first), Some(last)) = (self.beat_at(start), self.beat_at(end)) else {
            return Vec::new();
        };
        ((first.ceil() as i64)..=(last.floor() as i64))
            .filter_map(|beat| Some((self.time_of_beat(beat as f64)?, self.is_downbeat(beat))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_grid_maps_time_and_beats() {
        let grid = BeatGrid::constant(120.0, 0.25, 1);
        assert_eq!(grid.beat_at(0.25), Some(0.0));
        assert_eq!(grid.beat_at(1.25), Some(2.0));
        assert_eq!(grid.beat_at(0.0), Some(-0.5));
        assert_eq!(grid.time_of_beat(4.0), Some(2.25));
        assert!(grid.is_downbeat(0) && grid.is_downbeat(4) && !grid.is_downbeat(2));
        assert!(grid.is_downbeat(-4));
        let beats = grid.beats_between(0.0, 2.3);
        assert_eq!(beats.len(), 5);
        assert_eq!(beats[0], (0.25, true));
    }

    #[test]
    fn beat_in_bar_shifts_downbeats() {
        // The marker's beat is the 3rd of its bar, so beat 2 is the downbeat.
        let grid = BeatGrid::constant(128.0, 0.0, 3);
        assert!(!grid.is_downbeat(0));
        assert!(grid.is_downbeat(2));
    }

    #[test]
    fn variable_tempo_segments_chain() {
        let grid = BeatGrid::from_markers(vec![
            TempoMarker {
                time: 10.0,
                bpm: 60.0,
                beat_in_bar: 1,
            },
            TempoMarker {
                time: 0.0,
                bpm: 120.0,
                beat_in_bar: 1,
            },
        ]);
        // 0..10 s at 120 BPM is 20 beats; then 1 beat per second.
        assert_eq!(grid.beat_at(10.0), Some(20.0));
        assert_eq!(grid.beat_at(12.0), Some(22.0));
        assert_eq!(grid.time_of_beat(21.0), Some(11.0));
        assert_eq!(grid.time_of_beat(10.0), Some(5.0));
        assert_eq!(grid.bpm_at(11.0), Some(60.0));
    }

    #[test]
    fn rejects_bad_markers() {
        let grid = BeatGrid::from_markers(vec![TempoMarker {
            time: 0.0,
            bpm: 0.0,
            beat_in_bar: 9,
        }]);
        assert!(grid.is_empty());
        assert_eq!(grid.beat_at(1.0), None);
    }
}
