//! Beat grids: where the beats and bars of a track fall.
//!
//! A grid is a list of tempo markers, matching rekordbox's model (XML
//! `TEMPO` elements: `Inizio` start time, `Bpm`, `Battito` beat in bar).
//! Most tracks have one marker; variable-tempo tracks have several. Times
//! are in seconds so a grid is independent of any sample rate.

/// Beats per bar. rekordbox grids are always 4/4.
pub const BEATS_PER_BAR: u32 = 4;

/// Tempos a grid edit may produce, as rekordbox allows.
pub const EDIT_BPM_RANGE: std::ops::RangeInclusive<f64> = 20.0..=999.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMarker {
    /// Time of this marker's beat, seconds from the start of the track.
    pub time: f64,
    pub bpm: f64,
    /// Position of this beat within its bar, 1..=4 (1 is the downbeat).
    pub beat_in_bar: u32,
}

/// A gap between markers within this many beats of a whole number counts
/// as that whole number (marker times are stored rounded).
const WHOLE_BEAT_TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatGrid {
    /// Sorted by time; never empty for a usable grid.
    markers: Vec<TempoMarker>,
    /// Whole-beat index of each marker, counting the first marker's beat as
    /// 0. Every marker is itself a beat, so a marker that lands part-way
    /// through the previous tempo's beat shortens (or stretches) that beat.
    bases: Vec<f64>,
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
        markers.dedup_by(|b, a| b.time == a.time);
        for marker in &mut markers {
            marker.beat_in_bar = marker.beat_in_bar.clamp(1, BEATS_PER_BAR);
        }
        let mut bases = Vec::with_capacity(markers.len());
        let mut base = 0.0;
        for (i, marker) in markers.iter().enumerate() {
            bases.push(base);
            if let Some(next) = markers.get(i + 1) {
                let span = (next.time - marker.time) * marker.bpm / 60.0;
                let whole = span.round();
                let beats = if (span - whole).abs() < WHOLE_BEAT_TOLERANCE {
                    whole
                } else {
                    span.ceil()
                };
                base += beats.max(1.0);
            }
        }
        Self { markers, bases }
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

    /// For segment `i` with a following marker: the local index and time of
    /// its last full-length beat, after which the beat is resized to end
    /// exactly on the next marker.
    fn last_full_beat(&self, i: usize) -> Option<(f64, f64, f64)> {
        let (marker, next) = (self.markers.get(i)?, self.markers.get(i + 1)?);
        let last = self.bases[i + 1] - self.bases[i] - 1.0;
        let time = marker.time + last * 60.0 / marker.bpm;
        Some((last, time, next.time))
    }

    /// Fractional beat position at `time`; whole numbers are beats and each
    /// marker falls on one. Before the first marker the first tempo is
    /// extrapolated (negative beats).
    pub fn beat_at(&self, time: f64) -> Option<f64> {
        let i = self.segment(time);
        let marker = self.markers.get(i)?;
        let mut local = (time - marker.time) * marker.bpm / 60.0;
        if let Some((last, last_time, next_time)) = self.last_full_beat(i)
            && local > last
        {
            local = last + (time - last_time) / (next_time - last_time);
        }
        Some(self.bases[i] + local)
    }

    /// Time of fractional beat position `beat`, the inverse of `beat_at`.
    pub fn time_of_beat(&self, beat: f64) -> Option<f64> {
        let i = self.bases.partition_point(|&b| b <= beat).saturating_sub(1);
        let marker = self.markers.get(i)?;
        let local = beat - self.bases[i];
        if let Some((last, last_time, next_time)) = self.last_full_beat(i)
            && local > last
        {
            return Some(last_time + (local - last) * (next_time - last_time));
        }
        Some(marker.time + local * 60.0 / marker.bpm)
    }

    /// Whether the grid has markers and every tempo is in [`EDIT_BPM_RANGE`],
    /// so an edit that produced it can be applied.
    pub fn tempos_in_edit_range(&self) -> bool {
        !self.is_empty() && self.markers.iter().all(|m| EDIT_BPM_RANGE.contains(&m.bpm))
    }

    pub fn bpm_at(&self, time: f64) -> Option<f64> {
        self.markers.get(self.segment(time)).map(|m| m.bpm)
    }

    /// Whether whole beat `beat` (as counted by `beat_at`) is a downbeat,
    /// using the bar position of the marker that governs it.
    pub fn is_downbeat(&self, beat: i64) -> bool {
        self.beat_in_bar(beat) == Some(1)
    }

    /// Position 1..=4 of whole beat `beat` within its bar, counted from the
    /// bar position of the marker that governs it.
    pub fn beat_in_bar(&self, beat: i64) -> Option<u32> {
        let i = self
            .bases
            .partition_point(|&b| b <= beat as f64)
            .saturating_sub(1);
        let marker = self.markers.get(i)?;
        let since_marker = beat - self.bases[i] as i64;
        let bar = i64::from(BEATS_PER_BAR);
        Some((since_marker + i64::from(marker.beat_in_bar) - 1).rem_euclid(bar) as u32 + 1)
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

/// Grid edits, as the GRID EDIT panel applies them. Each returns a new
/// grid and leaves an empty grid empty.
impl BeatGrid {
    /// Every marker moved by `seconds` (positive is later).
    pub fn shifted(&self, seconds: f64) -> Self {
        self.map_markers(|m| TempoMarker {
            time: m.time + seconds,
            ..m
        })
    }

    /// Every tempo multiplied by `factor` (2.0 doubles, 0.5 halves); the
    /// markers stay where they are.
    pub fn tempo_scaled(&self, factor: f64) -> Self {
        self.map_markers(|m| TempoMarker {
            bpm: m.bpm * factor,
            ..m
        })
    }

    /// Each beat lengthened by `seconds` (negative shortens), keeping the
    /// first marker in place: the grid widens or narrows. Unchanged if that
    /// would make any beat zero or negative in length.
    pub fn stretched(&self, seconds: f64) -> Self {
        if self.markers.iter().any(|m| 60.0 / m.bpm + seconds <= 0.0) {
            return self.clone();
        }
        // Each marker moves by `seconds` per beat before it, so every
        // segment keeps its whole number of beats whatever its tempo.
        let markers = self
            .markers
            .iter()
            .zip(&self.bases)
            .map(|(m, &base)| TempoMarker {
                time: m.time + base * seconds,
                bpm: 60.0 / (60.0 / m.bpm + seconds),
                ..*m
            })
            .collect();
        Self::from_markers(markers)
    }

    /// A constant grid at `bpm` that keeps this grid's first beat.
    pub fn with_constant_tempo(&self, bpm: f64) -> Self {
        match self.markers.first() {
            Some(first) if bpm > 0.0 => Self::constant(bpm, first.time, first.beat_in_bar),
            _ => self.clone(),
        }
    }

    fn map_markers(&self, f: impl Fn(TempoMarker) -> TempoMarker) -> Self {
        Self::from_markers(self.markers.iter().copied().map(f).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_edits() {
        let grid = BeatGrid::constant(120.0, 0.25, 1);
        assert_eq!(grid.shifted(0.01).markers()[0].time, 0.26);
        assert_eq!(grid.tempo_scaled(2.0).markers()[0].bpm, 240.0);
        let wide = grid.stretched(0.01);
        assert!((wide.markers()[0].bpm - 60.0 / 0.51).abs() < 1e-9);
        assert_eq!(wide.markers()[0].time, 0.25);
        let tapped = grid.with_constant_tempo(126.0);
        assert_eq!(
            (tapped.markers()[0].time, tapped.markers()[0].bpm),
            (0.25, 126.0)
        );
        assert!(BeatGrid::default().shifted(1.0).is_empty());
        // Edits past the tempo range are flagged; a stretch that would
        // make a beat zero-length does nothing.
        assert!(grid.tempos_in_edit_range());
        assert!(!grid.tempo_scaled(16.0).tempos_in_edit_range());
        assert!(!grid.tempo_scaled(1.0 / 16.0).tempos_in_edit_range());
        assert_eq!(grid.stretched(-0.5), grid);
    }

    #[test]
    fn stretching_keeps_whole_beats_across_tempos() {
        // 16 beats at 120 BPM, 8 at 60 BPM, then a final marker.
        let grid = BeatGrid::from_markers(
            [(0.0, 120.0), (8.0, 60.0), (16.0, 60.0)]
                .map(|(time, bpm)| TempoMarker {
                    time,
                    bpm,
                    beat_in_bar: 1,
                })
                .to_vec(),
        );
        let wide = grid.stretched(0.0005);
        let times: Vec<f64> = wide.markers().iter().map(|m| m.time).collect();
        assert!((times[1] - (8.0 + 16.0 * 0.0005)).abs() < 1e-9);
        assert!((times[2] - (16.0 + 24.0 * 0.0005)).abs() < 1e-9);
        // No beat is inserted: marker beats and downbeats stay put.
        assert_eq!(wide.beat_at(times[2]), Some(24.0));
        assert!(wide.is_downbeat(24));
    }

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

    #[test]
    fn each_marker_is_a_beat_with_its_own_bar_position() {
        // The second marker lands a fifth of a beat after beat 20 at 120 BPM.
        let grid = BeatGrid::from_markers(vec![
            TempoMarker {
                time: 0.0,
                bpm: 120.0,
                beat_in_bar: 1,
            },
            TempoMarker {
                time: 10.1,
                bpm: 60.0,
                beat_in_bar: 3,
            },
        ]);
        assert_eq!(grid.beat_at(10.0), Some(20.0));
        assert_eq!(grid.beat_at(10.1), Some(21.0));
        assert_eq!(grid.time_of_beat(21.0), Some(10.1));
        assert!((grid.time_of_beat(22.0).unwrap() - 11.1).abs() < 1e-9);
        let beats: Vec<f64> = grid.beats_between(9.9, 12.2).iter().map(|b| b.0).collect();
        assert_eq!(beats.len(), 4);
        assert!((beats[1] - 10.1).abs() < 1e-9);
        // Beat 20 is a downbeat of the first marker's bars; the second
        // marker's beat is 3rd in its bar, so 23 is the next downbeat.
        assert!(grid.is_downbeat(20));
        assert!(!grid.is_downbeat(21) && !grid.is_downbeat(22));
        assert!(grid.is_downbeat(23));
        assert_eq!(grid.beat_in_bar(20), Some(1));
        assert_eq!(
            grid.beat_in_bar(21),
            Some(3),
            "marker keeps its own bar position"
        );
        // Positions stay monotonic through the shortened beat.
        assert!(grid.beat_at(10.05).unwrap() > 20.0 && grid.beat_at(10.05).unwrap() < 21.0);
    }
}
