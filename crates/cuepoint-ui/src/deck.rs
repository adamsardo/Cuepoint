//! Deck panel: track info, overview waveform, transport, loops and tempo
//! (spec 05-ui, section 2.4). Emits engine commands; never touches audio.

use cuepoint_core::{BeatGrid, MusicalKey, Track};
use cuepoint_engine::{Command, DeckState};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::waveform::BandWaveform;

const WAVEFORM_HEIGHT: f32 = 64.0;
const CUE_COLOUR: Color32 = Color32::from_rgb(255, 150, 30);
const LOOP_COLOUR: Color32 = Color32::from_rgba_premultiplied(40, 120, 40, 90);
const WAVE_COLOUR: Color32 = Color32::from_rgb(70, 140, 220);
const PLAYED_COLOUR: Color32 = Color32::from_rgb(45, 75, 110);

/// Tempo slider ranges a DJ can pick, as ± fractions.
const TEMPO_RANGES: [(f64, &str); 4] =
    [(0.06, "±6%"), (0.10, "±10%"), (0.16, "±16%"), (1.0, "WIDE")];

/// What the UI keeps about a loaded track.
#[derive(Debug, Clone)]
pub struct LoadedTrack {
    pub track: Track,
    /// Peak envelope across the whole track, 0..=1 per bin.
    pub overview: Vec<f32>,
    /// 3-band detail waveform from analysis.
    pub bands: Option<BandWaveform>,
    /// Grid in use: the library's when it has one, else analysis'.
    pub grid: BeatGrid,
    pub key: Option<MusicalKey>,
}

/// UI-side state of one deck.
#[derive(Debug, Clone)]
pub struct DeckView {
    pub index: usize,
    pub loaded: Option<LoadedTrack>,
    /// Shown after a load failed or was refused.
    pub message: Option<String>,
    /// Title of the track being decoded; the deck shows as empty meanwhile.
    pub loading: Option<String>,
    /// Tempo slider position as a fraction, e.g. 0.02 = +2%.
    pub tempo: f64,
    pub tempo_range: f64,
    pub(crate) cue_held: bool,
    /// Tracks put on this deck so far, so a view can tell a reload of the
    /// same track from the load it already has.
    pub loads: u64,
}

impl DeckView {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            loaded: None,
            message: None,
            loading: None,
            tempo: 0.0,
            tempo_range: TEMPO_RANGES[0].0,
            cue_held: false,
            loads: 0,
        }
    }

    /// Puts `track` on the deck as a new load.
    pub fn set_loaded(&mut self, track: LoadedTrack) {
        self.loaded = Some(track);
        self.loads += 1;
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
    ) {
        let deck = self.index;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("DECK {}", deck + 1))
                    .strong()
                    .monospace(),
            );
            match (&self.loaded, &self.message) {
                _ if self.loading.is_some() => {
                    ui.weak(format!(
                        "Loading {}…",
                        self.loading.as_deref().unwrap_or_default()
                    ));
                }
                (_, Some(message)) => {
                    ui.weak(message);
                }
                (Some(loaded), None) => {
                    ui.label(RichText::new(&loaded.track.title).strong());
                    ui.weak(&loaded.track.artist);
                }
                (None, None) => {
                    ui.weak("Empty. Right-click a track and load it here.");
                }
            }
        });
        ui.horizontal(|ui| {
            let library_bpm = self
                .loaded
                .as_ref()
                .and_then(|l| l.track.bpm)
                .map(|b| b.as_f64() * state.tempo);
            let bpm = state
                .bpm
                .or(library_bpm)
                .map_or("--.--".to_owned(), |b| format!("{b:.2}"));
            ui.label(RichText::new(bpm).size(22.0).monospace().strong());
            ui.label(RichText::new("BPM").small());
            if let Some(key) = self.loaded.as_ref().and_then(|l| l.key) {
                ui.separator();
                ui.label(RichText::new(key.camelot()).monospace().strong())
                    .on_hover_text(key.name());
            }
            ui.separator();
            ui.label(RichText::new(format!("{:+.2}%", (state.tempo - 1.0) * 100.0)).monospace());
            ui.separator();
            let rate = f64::from(sample_rate.max(1));
            let elapsed = state.position / rate;
            let remaining = (state.frames as f64 - state.position).max(0.0) / rate;
            ui.label(RichText::new(clock(elapsed)).monospace().size(16.0));
            ui.label(
                RichText::new(format!("-{}", clock(remaining)))
                    .monospace()
                    .size(16.0)
                    .weak(),
            );
        });
        self.waveform(ui, state, commands);
        ui.horizontal(|ui| {
            let size = Vec2::new(64.0, 36.0);
            let cue = ui.add_sized(
                size,
                egui::Button::new(RichText::new("CUE").strong()).sense(Sense::click_and_drag()),
            );
            let held = cue.is_pointer_button_down_on();
            if held != self.cue_held {
                commands.push(if held {
                    Command::CueDown { deck }
                } else {
                    Command::CueUp { deck }
                });
                self.cue_held = held;
            }
            let play_label = if state.playing { "⏸" } else { "▶" };
            if ui
                .add_sized(
                    size,
                    egui::Button::new(RichText::new(play_label).size(18.0)).selected(state.playing),
                )
                .clicked()
            {
                commands.push(Command::PlayPause { deck });
            }
            ui.separator();
            let small = Vec2::new(56.0, 36.0);
            let armed = state.loop_in.is_some() && !state.loop_active;
            if ui
                .add_sized(small, egui::Button::new("IN").selected(armed))
                .clicked()
            {
                commands.push(Command::LoopIn { deck });
            }
            if ui.add_sized(small, egui::Button::new("OUT")).clicked() {
                commands.push(Command::LoopOut { deck });
            }
            let reloop = if state.loop_active { "EXIT" } else { "RELOOP" };
            if ui
                .add_sized(
                    Vec2::new(72.0, 36.0),
                    egui::Button::new(reloop).selected(state.loop_active),
                )
                .clicked()
            {
                commands.push(Command::ReloopExit { deck });
            }
        });
        ui.horizontal(|ui| {
            let gridded = state.bpm.is_some();
            let sync = ui
                // Always allow switching sync off, even on an ungridded track.
                .add_enabled(
                    gridded || state.sync,
                    egui::Button::new("SYNC").selected(state.sync),
                )
                .on_disabled_hover_text("Needs a beat grid");
            if sync.clicked() {
                commands.push(Command::SetSync {
                    deck,
                    on: !state.sync,
                });
            }
            let master =
                ui.add_enabled(gridded, egui::Button::new("MASTER").selected(state.master));
            if master.clicked() {
                commands.push(Command::SetSyncMaster { deck });
            }
            for (label, beats) in [("◀4", -4.0), ("4▶", 4.0)] {
                let jump = ui
                    .add_enabled(gridded, egui::Button::new(label))
                    .on_hover_text("Beat jump");
                if jump.clicked() {
                    commands.push(Command::BeatJump { deck, beats });
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("TEMPO");
            // A synced follower's tempo is set by the master.
            // Only a gridded follower has its tempo set by the master.
            let following = state.sync && !state.master && state.bpm.is_some();
            if following {
                self.tempo = state.tempo - 1.0;
                self.tempo_range = TEMPO_RANGES
                    .iter()
                    .map(|r| r.0)
                    .find(|&r| self.tempo.abs() <= r)
                    .unwrap_or(1.0);
            }
            let range = self.tempo_range;
            let slider = egui::Slider::new(&mut self.tempo, -range..=range)
                .show_value(false)
                .smart_aim(false);
            let changed = ui
                .add_enabled_ui(!following, |ui| {
                    ui.add_sized(Vec2::new(ui.available_width() - 150.0, 18.0), slider)
                })
                .inner
                .changed();
            if ui.small_button("0").on_hover_text("Reset tempo").clicked() || changed {
                if !changed {
                    self.tempo = 0.0;
                }
                commands.push(Command::SetTempo {
                    deck,
                    ratio: 1.0 + self.tempo,
                });
            }
            egui::ComboBox::from_id_salt(("tempo_range", deck))
                .width(64.0)
                .selected_text(
                    TEMPO_RANGES
                        .iter()
                        .find(|r| r.0 == self.tempo_range)
                        .map_or("", |r| r.1),
                )
                .show_ui(ui, |ui| {
                    for (value, label) in TEMPO_RANGES {
                        if ui
                            .selectable_value(&mut self.tempo_range, value, label)
                            .changed()
                        {
                            self.tempo = self.tempo.clamp(-value, value);
                            commands.push(Command::SetTempo {
                                deck,
                                ratio: 1.0 + self.tempo,
                            });
                        }
                    }
                });
        });
    }

    fn waveform(&self, ui: &mut Ui, state: &DeckState, commands: &mut Vec<Command>) {
        let size = Vec2::new(ui.available_width(), WAVEFORM_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
        let frames = state.frames as f64;
        if !state.loaded || frames <= 0.0 {
            return;
        }
        let x_of = |frame: f64| rect.left() + (frame / frames) as f32 * rect.width();
        if let (Some(start), Some(stop)) = (state.loop_in, state.loop_out) {
            let loop_rect = Rect::from_x_y_ranges(x_of(start)..=x_of(stop), rect.y_range());
            painter.rect_filled(loop_rect, 0.0, LOOP_COLOUR);
        }
        let playhead_x = x_of(state.position);
        if let Some(loaded) = &self.loaded
            && !loaded.overview.is_empty()
        {
            let bins = loaded.overview.len();
            let columns = rect.width().max(1.0) as usize;
            let mid = rect.center().y;
            for column in 0..columns {
                let start = column * bins / columns;
                let end = ((column + 1) * bins / columns).max(start + 1).min(bins);
                let peak = loaded.overview[start..end]
                    .iter()
                    .copied()
                    .fold(0.0, f32::max)
                    .min(1.0);
                let x = rect.left() + column as f32 + 0.5;
                let half = peak * (rect.height() * 0.5 - 2.0);
                let colour = if x < playhead_x {
                    PLAYED_COLOUR
                } else {
                    WAVE_COLOUR
                };
                painter.line_segment(
                    [Pos2::new(x, mid - half), Pos2::new(x, mid + half)],
                    Stroke::new(1.0, colour),
                );
            }
        }
        let cue_x = x_of(state.cue);
        painter.line_segment(
            [
                Pos2::new(cue_x, rect.top()),
                Pos2::new(cue_x, rect.bottom()),
            ],
            Stroke::new(1.5, CUE_COLOUR),
        );
        painter.line_segment(
            [
                Pos2::new(playhead_x, rect.top()),
                Pos2::new(playhead_x, rect.bottom()),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let fraction = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            commands.push(Command::Seek {
                deck: self.index,
                frame: f64::from(fraction) * frames,
            });
        }
    }
}

/// `m:ss.t`
fn clock(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).floor() as u64;
    format!("{}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::clock;

    #[test]
    fn clock_format() {
        assert_eq!(clock(0.0), "0:00.0");
        assert_eq!(clock(61.25), "1:01.2");
        assert_eq!(clock(-3.0), "0:00.0");
    }
}
