//! Scrolling detail waveform: 3-band colour waveform centred on the
//! playhead, with beat and bar lines (spec 05-ui, section 2.6).
//!
//! The view spans a fixed amount of real time, so at matched tempo two
//! synced decks show their beat lines at the same spacing.

use cuepoint_engine::DeckState;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::deck::LoadedTrack;

/// Seconds of playback shown either side of the playhead.
const HALF_SPAN_SECS: f64 = 4.0;
const LOW_COLOUR: Color32 = Color32::from_rgb(30, 90, 230);
const MID_COLOUR: Color32 = Color32::from_rgb(240, 150, 40);
const HIGH_COLOUR: Color32 = Color32::from_rgb(245, 245, 245);
const BEAT_COLOUR: Color32 = Color32::from_rgba_premultiplied(140, 140, 140, 140);
const BAR_COLOUR: Color32 = Color32::from_rgb(230, 40, 40);
const PLAYHEAD_COLOUR: Color32 = Color32::from_rgb(255, 255, 255);
const CUE_COLOUR: Color32 = Color32::from_rgb(255, 150, 30);
const LOOP_COLOUR: Color32 = Color32::from_rgba_premultiplied(40, 120, 40, 70);

/// Per-band peak amplitudes, 0..=255, at a fixed rate.
#[derive(Debug, Clone, Default)]
pub struct BandWaveform {
    pub points_per_second: u32,
    pub low: Vec<u8>,
    pub mid: Vec<u8>,
    pub high: Vec<u8>,
}

impl BandWaveform {
    /// Mean value of each band over points `start..end`, 0..=255.
    pub(crate) fn means(&self, start: usize, end: usize) -> (f32, f32, f32) {
        let end = end
            .min(self.low.len())
            .min(self.mid.len())
            .min(self.high.len());
        let start = start.min(end);
        let count = (end - start).max(1) as f32;
        let mean =
            |band: &[u8]| band[start..end].iter().map(|&v| f32::from(v)).sum::<f32>() / count;
        (mean(&self.low), mean(&self.mid), mean(&self.high))
    }

    /// Largest value of each band over points `start..end`.
    pub(crate) fn peaks(&self, start: usize, end: usize) -> (u8, u8, u8) {
        let end = end
            .min(self.low.len())
            .min(self.mid.len())
            .min(self.high.len());
        let start = start.min(end);
        let max = |band: &[u8]| band[start..end].iter().copied().max().unwrap_or(0);
        (max(&self.low), max(&self.mid), max(&self.high))
    }
}

/// Draws one deck's detail waveform at full available width.
pub fn detail_waveform(
    ui: &mut Ui,
    height: f32,
    loaded: Option<&LoadedTrack>,
    state: &DeckState,
    sample_rate: u32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_gray(12));
    let (Some(loaded), true) = (loaded, state.loaded) else {
        return;
    };
    let rate = f64::from(sample_rate.max(1));
    let now = state.position / rate;
    let half = HALF_SPAN_SECS * state.tempo.max(0.01);
    let (start, end) = (now - half, now + half);
    let secs_per_px = (end - start) / f64::from(rect.width().max(1.0));
    let x_of = |t: f64| rect.left() + ((t - start) / secs_per_px) as f32;
    let mid_y = rect.center().y;
    let half_height = rect.height() * 0.5 - 1.0;

    if let (Some(a), Some(b)) = (state.loop_in, state.loop_out) {
        let (a, b) = (x_of(a / rate), x_of(b / rate));
        painter.rect_filled(
            Rect::from_x_y_ranges(a..=b, rect.y_range()),
            0.0,
            LOOP_COLOUR,
        );
    }

    if let Some(bands) = &loaded.bands
        && bands.points_per_second > 0
    {
        let pps = f64::from(bands.points_per_second);
        for column in 0..rect.width() as usize {
            let t0 = start + column as f64 * secs_per_px;
            if t0 < 0.0 {
                continue;
            }
            let first = (t0 * pps) as usize;
            let last = (((t0 + secs_per_px) * pps) as usize).max(first + 1);
            if first >= bands.low.len() {
                break;
            }
            let (low, mid, high) = bands.peaks(first, last);
            let x = rect.left() + column as f32 + 0.5;
            // Low band at the back, then mid, then high on top.
            for (value, colour, weight) in [
                (low, LOW_COLOUR, 1.0),
                (mid, MID_COLOUR, 0.8),
                (high, HIGH_COLOUR, 0.55),
            ] {
                let h = f32::from(value) / 255.0 * half_height * weight;
                if h >= 0.5 {
                    painter.line_segment(
                        [Pos2::new(x, mid_y - h), Pos2::new(x, mid_y + h)],
                        Stroke::new(1.0, colour),
                    );
                }
            }
        }
    }

    for (time, downbeat) in loaded.grid.beats_between(start, end) {
        let x = x_of(time);
        let stroke = if downbeat {
            Stroke::new(1.5, BAR_COLOUR)
        } else {
            Stroke::new(1.0, BEAT_COLOUR)
        };
        // Ticks at the top and bottom edges; bars reach further in.
        let tick = rect.height() * if downbeat { 0.3 } else { 0.15 };
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.top() + tick)],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom() - tick),
                Pos2::new(x, rect.bottom()),
            ],
            stroke,
        );
    }

    let cue_x = x_of(state.cue / rate);
    if rect.x_range().contains(cue_x) {
        painter.line_segment(
            [
                Pos2::new(cue_x, rect.top()),
                Pos2::new(cue_x, rect.bottom()),
            ],
            Stroke::new(1.5, CUE_COLOUR),
        );
    }
    let centre = rect.center().x;
    painter.line_segment(
        [
            Pos2::new(centre, rect.top()),
            Pos2::new(centre, rect.bottom()),
        ],
        Stroke::new(2.0, PLAYHEAD_COLOUR),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_clamp_to_available_points() {
        let w = BandWaveform {
            points_per_second: 150,
            low: vec![1, 9, 3],
            mid: vec![4, 5, 6],
            high: vec![7, 2, 0],
        };
        assert_eq!(w.peaks(0, 2), (9, 5, 7));
        assert_eq!(w.peaks(2, 99), (3, 6, 0));
        assert_eq!(w.peaks(5, 9), (0, 0, 0));
    }
}
