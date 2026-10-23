//! The Export screen: top bar, the single preview player ("PLAYER A") with
//! its overview and zoomed waveforms, the cue side panel and the footer.
//! Geometry follows rekordbox 7.2.19's Export layout on macOS, in points;
//! comments give the measured position of each element.

use cuepoint_core::{BeatGrid, Cue};
use cuepoint_engine::{Command, DeckState};
use egui::{Align, Rect, Sense, Ui, pos2, vec2};

use crate::artwork::ArtworkCache;
use crate::deck::DeckView;
use crate::paint;
use crate::theme::{self, wave};

/// Heights of the fixed bands, top to bottom.
pub const TITLE_BAR: f32 = 28.0;
pub const TOP_BAR: f32 = 28.0;
pub const PLAYER: f32 = 256.0;
pub const DIVIDER: f32 = 3.0;
pub const FOOTER: f32 = 28.0;

/// The player's left button column and right cue panel widths.
const LEFT_COLUMN: f32 = 80.0;
const SIDE_PANEL: f32 = 212.0;

/// Zoomed waveform scale: rekordbox's default shows a 132 BPM beat as
/// 27.6 pt, i.e. 60.7 pt per second.
const DEFAULT_ZOOM: f32 = 60.7;
const ZOOM_STEP: f32 = 1.5;

/// Beat-jump sizes offered under the jump buttons.
const JUMP_SIZES: [(f64, &str); 6] = [
    (1.0, "1Beat"),
    (2.0, "2Beats"),
    (4.0, "4Beats"),
    (8.0, "8Beats"),
    (16.0, "16Beats"),
    (32.0, "32Beats"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Export,
    Performance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LowerTab {
    CueLoop,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideTab {
    Memory,
    HotCue,
    Info,
}

/// UI state of the Export screen that is not engine state.
pub struct ExportView {
    /// Zoomed waveform scale in points per second of audio.
    zoom: f32,
    jump: usize,
    lower_tab: LowerTab,
    side_tab: SideTab,
    beat_loop: u32,
    quantize: bool,
    /// Grid edits on the loaded track, for undo and redo.
    grid_undo: Vec<cuepoint_core::BeatGrid>,
    grid_redo: Vec<cuepoint_core::BeatGrid>,
    /// Recent TAP times, in egui seconds.
    taps: Vec<f64>,
    /// The track the undo history belongs to.
    grid_track: Option<cuepoint_core::TrackId>,
}

impl Default for ExportView {
    fn default() -> Self {
        Self {
            zoom: DEFAULT_ZOOM,
            jump: 2,
            lower_tab: LowerTab::CueLoop,
            side_tab: SideTab::HotCue,
            beat_loop: 4,
            quantize: true,
            grid_undo: Vec::new(),
            grid_redo: Vec::new(),
            taps: Vec::new(),
            grid_track: None,
        }
    }
}

/// Clicks on a painted rect.
fn click(ui: &Ui, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug) -> egui::Response {
    ui.interact(rect, ui.scope_id().with(("export", id)), Sense::click())
}

/// The window title bar: drag to move, double-click to zoom. The traffic
/// lights are the system's, drawn over it.
pub fn title_bar(ui: &mut Ui, rect: Rect, title: &str) {
    let p = ui.painter_at(rect);
    paint::fill(&p, rect, theme::TITLE_BAR);
    paint::text(
        &p,
        rect.center().x,
        rect.top() + 8.5,
        Align::Center,
        title,
        theme::bold(13.0),
        theme::TITLE_TEXT,
    );
    let response = ui.interact(
        rect,
        ui.scope_id().with("title_bar"),
        Sense::click_and_drag(),
    );
    if response.double_clicked() {
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// The bar across the top of the window. Returns a newly chosen mode.
pub fn top_bar(ui: &mut Ui, rect: Rect, mode: Mode, clock: &str) -> Option<Mode> {
    let p = ui.painter_at(rect);
    paint::fill(&p, rect, theme::BAR);
    let (x, y) = (rect.left(), rect.top());
    // Mode menu: "EXPORT" at x 16.5, cap top 10, chevron at 116.
    let label = match mode {
        Mode::Export => "EXPORT",
        Mode::Performance => "PERFORMANCE",
    };
    paint::text(
        &p,
        x + 16.5,
        y + 10.0,
        Align::Min,
        label,
        theme::bold(theme::BODY_SIZE),
        theme::TEXT,
    );
    paint::chevron_down(&p, pos2(x + 116.0, y + 14.0), 9.0, theme::TEXT);
    let menu_rect = Rect::from_min_max(pos2(x, y), pos2(x + 125.0, rect.bottom()));
    let mut chosen = None;
    let response = click(ui, menu_rect, "mode");
    egui::Popup::menu(&response).show(|ui| {
        for (option, text) in [(Mode::Export, "EXPORT"), (Mode::Performance, "PERFORMANCE")] {
            if ui.selectable_label(mode == option, text).clicked() {
                chosen = Some(option);
            }
        }
    });
    // Layout toggle: a small window outline at 136-153, chevron at 170.
    let layout = Rect::from_min_size(pos2(x + 136.0, y + 9.5), vec2(17.0, 9.0));
    paint::outline(&p, layout, 0.0, theme::TEXT);
    paint::fill(
        &p,
        Rect::from_min_size(layout.min + vec2(3.0, 3.0), vec2(4.0, 3.0)),
        theme::TEXT,
    );
    paint::chevron_down(&p, pos2(x + 170.5, y + 14.0), 9.0, theme::TEXT);
    // Recording button, 20 x 20 at x 310.
    let record = Rect::from_min_size(pos2(x + 310.0, y + 4.0), vec2(20.0, 20.0));
    paint::rounded(&p, record, 3.0, theme::BUTTON);
    paint::record(&p, record.center(), theme::TEXT);

    // Right-hand group, anchored to the right edge.
    let r = rect.right();
    paint::info(&p, pos2(r - 367.0, y + 14.0), theme::TEXT, theme::BAR);
    let badge = Rect::from_min_max(pos2(r - 338.0, y + 5.0), pos2(r - 291.0, y + 23.0));
    paint::rounded(&p, badge, 2.0, theme::BADGE_BG);
    paint::text_centred(&p, badge, "Free", theme::bold(12.5), theme::BADGE_TEXT);
    paint::gear(&p, pos2(r - 261.0, y + 13.5), 7.5, theme::TEXT);
    paint::arc(
        &p,
        pos2(r - 224.0, y + 14.0),
        7.0,
        -3.4,
        1.1,
        2.4,
        theme::ACCENT,
    );
    // Master level meters (left/right), then the output level.
    for dy in [9.0, 14.5] {
        let meter = Rect::from_min_size(pos2(r - 205.0, y + dy), vec2(80.0, 2.5));
        paint::fill(&p, meter, theme::TAB_OFF);
    }
    let level = Rect::from_min_size(pos2(r - 105.0, y + 12.5), vec2(35.0, 2.5));
    paint::fill(&p, level, theme::TAB_OFF);
    paint::fill(
        &p,
        Rect::from_min_size(level.min, vec2(3.0, 2.5)),
        theme::ACCENT,
    );
    paint::text(
        &p,
        r - 26.0,
        y + 9.5,
        Align::Max,
        clock,
        theme::bold(theme::BODY_SIZE),
        theme::TEXT,
    );
    chosen
}

/// The footer: wordmark on the left, the track list summary on the right.
pub fn footer(ui: &mut Ui, rect: Rect, summary: &str) {
    let p = ui.painter_at(rect);
    paint::fill(&p, rect, theme::BLACK);
    let mark = pos2(rect.left() + 16.0, rect.top() + 13.0);
    p.circle_stroke(
        mark + vec2(5.0, 0.0),
        5.5,
        egui::Stroke::new(2.0, theme::TEXT),
    );
    p.circle_filled(mark + vec2(5.0, 0.0), 2.0, theme::TEXT);
    paint::text(
        &p,
        rect.left() + 30.0,
        rect.top() + 7.5,
        Align::Min,
        "cuepoint",
        theme::bold(15.5),
        theme::TEXT,
    );
    paint::text(
        &p,
        rect.right() - 34.5,
        rect.top() + 12.0,
        Align::Max,
        summary,
        theme::bold(11.6),
        theme::TEXT_DIM,
    );
}

impl ExportView {
    /// Opens a tab of the cue side panel.
    pub fn show_side_tab(&mut self, tab: SideTab) {
        self.side_tab = tab;
    }

    /// Opens the GRID tab under the player.
    pub fn show_grid_edit(&mut self) {
        self.lower_tab = LowerTab::Grid;
    }

    /// Draws the player band (left buttons, waveforms, side panel).
    #[allow(clippy::too_many_arguments)] // one deck's view, state and outputs
    pub fn player(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        deck: &mut DeckView,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
        artwork: &mut ArtworkCache,
    ) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::BLACK);
        let left = Rect::from_min_max(
            pos2(rect.left(), rect.top() + 1.0),
            pos2(rect.left() + LEFT_COLUMN, rect.bottom()),
        );
        let side = Rect::from_min_max(
            pos2(rect.right() - SIDE_PANEL + 3.0, rect.top()),
            pos2(rect.right() - 3.0, rect.bottom()),
        );
        let main = Rect::from_min_max(
            pos2(left.right(), rect.top()),
            pos2(side.left() - 3.0, rect.bottom()),
        );
        self.left_column(ui, left, deck, state, commands);
        self.info_row(ui, main, deck, state, sample_rate, artwork);
        self.overview(ui, main, deck, state, sample_rate, commands);
        self.zoomed(ui, main, deck, state, sample_rate);
        self.lower_strip(ui, main, deck, state, sample_rate, commands);
        self.side_panel(ui, side, deck, state, sample_rate);
    }

    fn left_column(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        deck: &mut DeckView,
        state: &DeckState,
        commands: &mut Vec<Command>,
    ) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::PANEL);
        let (x, y) = (rect.left(), rect.top() - 1.0);
        let index = deck.index;
        let gridded = state.bpm.is_some();
        let beats = JUMP_SIZES[self.jump].0;
        // Beat jump (top row) and fine move (second row) buttons, 26 x 26.
        let buttons = [
            (12.0, 22.0, "jump_back"),
            (42.0, 22.0, "jump_fwd"),
            (12.0, 68.0, "move_back"),
            (42.0, 68.0, "move_fwd"),
        ];
        for (bx, by, name) in buttons {
            let b = Rect::from_min_size(pos2(x + bx, y + by), vec2(26.0, 26.0));
            paint::rounded(&p, b, 2.0, theme::BUTTON);
            match name {
                "jump_back" => paint::jump_arrow(&p, b.center(), true, theme::TEXT),
                "jump_fwd" => paint::jump_arrow(&p, b.center(), false, theme::TEXT),
                "move_back" => paint::chevron_side(&p, b.center(), 11.0, false, theme::TEXT),
                _ => paint::chevron_side(&p, b.center(), 11.0, true, theme::TEXT),
            }
            if click(ui, b, (name, index)).clicked() && gridded {
                let jump = match name {
                    "jump_back" => -beats,
                    "jump_fwd" => beats,
                    "move_back" => -1.0,
                    _ => 1.0,
                };
                commands.push(Command::BeatJump {
                    deck: index,
                    beats: jump,
                });
            }
        }
        // Jump size selector, 56 x 26 at y 98.
        let size = Rect::from_min_size(pos2(x + 12.0, y + 98.0), vec2(56.0, 26.0));
        paint::rounded(&p, size, 2.0, theme::BUTTON);
        paint::text(
            &p,
            size.center().x,
            y + 103.5,
            Align::Center,
            JUMP_SIZES[self.jump].1,
            theme::bold(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        paint::chevron_down(&p, pos2(size.center().x, y + 118.5), 7.0, theme::TEXT);
        let response = click(ui, size, ("jump_size", index));
        egui::Popup::menu(&response).show(|ui| {
            for (i, (_, label)) in JUMP_SIZES.iter().enumerate() {
                if ui.selectable_label(self.jump == i, *label).clicked() {
                    self.jump = i;
                }
            }
        });
        // CUE: a 38 pt ring centred at (40, 164).
        let cue_centre = pos2(x + 40.0, y + 164.0);
        p.circle_filled(cue_centre, 19.0, theme::BUTTON);
        p.circle_stroke(cue_centre, 18.6, egui::Stroke::new(1.5, theme::CUE_RING));
        paint::text_centred(
            &p,
            Rect::from_center_size(cue_centre, vec2(38.0, 38.0)),
            "CUE",
            theme::bold(theme::BODY_SIZE),
            theme::TEXT,
        );
        let cue = ui.interact(
            Rect::from_center_size(cue_centre, vec2(38.0, 38.0)),
            ui.scope_id().with(("export_cue", index)),
            Sense::click_and_drag(),
        );
        let held = cue.is_pointer_button_down_on();
        if held != deck.cue_held {
            commands.push(if held {
                Command::CueDown { deck: index }
            } else {
                Command::CueUp { deck: index }
            });
            deck.cue_held = held;
        }
        // Play/pause: green ring centred at (40, 214).
        let play_centre = pos2(x + 40.0, y + 214.0);
        let ring = if state.playing {
            theme::wave::MID
        } else {
            theme::PLAY_RING
        };
        p.circle_stroke(play_centre, 18.6, egui::Stroke::new(1.5, ring));
        if state.playing {
            for dx in [-3.5, 3.5] {
                paint::fill(
                    &p,
                    Rect::from_center_size(play_centre + vec2(dx, 0.0), vec2(3.0, 11.0)),
                    theme::TEXT,
                );
            }
        } else {
            // rekordbox keeps the glyph grey while paused.
            let glyph = theme::PLAY_GLYPH;
            paint::play_triangle(&p, play_centre + vec2(1.5, 0.0), 11.0, true, glyph);
        }
        if click(
            ui,
            Rect::from_center_size(play_centre, vec2(38.0, 38.0)),
            ("play", index),
        )
        .clicked()
        {
            commands.push(Command::PlayPause { deck: index });
        }
    }

    fn info_row(
        &self,
        ui: &Ui,
        main: Rect,
        deck: &DeckView,
        state: &DeckState,
        sample_rate: u32,
        artwork: &mut ArtworkCache,
    ) {
        let p = ui.painter_at(main);
        let (x, y, r) = (main.left(), main.top(), main.right());
        // Artwork placeholder, 48 x 48 at (95, 39) in window points.
        let art = Rect::from_min_size(pos2(x + 15.0, y + 11.0), vec2(48.0, 48.0));
        let Some(loaded) = &deck.loaded else {
            if let Some(message) = &deck.message {
                paint::text(
                    &p,
                    x + 76.5,
                    y + 6.0,
                    Align::Min,
                    message,
                    theme::regular(13.0),
                    theme::TEXT_DIM,
                );
            }
            return;
        };
        match loaded
            .track
            .location
            .as_deref()
            .and_then(|path| artwork.get(path))
        {
            Some(texture) => {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                p.image(texture.id(), art, uv, egui::Color32::WHITE);
            }
            None => {
                paint::fill(&p, art, theme::BUTTON);
                paint::note(&p, art.center(), theme::TEXT_OFF);
            }
        }
        let track = &loaded.track;
        let rate = f64::from(sample_rate.max(1));
        let elapsed = state.position / rate;
        let remaining = (state.frames as f64 - state.position).max(0.0) / rate;
        // Right-anchored readouts: separators at r-288, r-131, r-75.
        let sep = |sx: f32| {
            paint::line(
                &p,
                pos2(sx, y + 2.0),
                pos2(sx, y + 19.0),
                1.0,
                theme::TEXT_OFF,
            );
        };
        let artist_sep = x + 76.0 + (r - 288.0 - (x + 76.0)) * 0.675;
        sep(artist_sep);
        sep(r - 288.5);
        sep(r - 131.5);
        sep(r - 75.5);
        paint::text_clipped(
            &p,
            x + 76.5,
            y + 5.0,
            artist_sep - x - 86.0,
            Align::Min,
            &track.title,
            theme::bold(16.0),
            theme::TEXT,
        );
        paint::text_clipped(
            &p,
            artist_sep + 17.0,
            y + 6.0,
            r - 288.5 - artist_sep - 26.0,
            Align::Min,
            &track.artist,
            theme::regular(13.0),
            theme::TEXT_KEY,
        );
        // rekordbox's clock digits are wider than plain Arial at this size.
        let big = theme::regular(16.0);
        let small = theme::regular(10.5);
        let clock_at = |left: f32, seconds: f64, sign: &str, colour| {
            let (whole, tenth) = clock(seconds);
            let text = format!("{sign}{whole}");
            let rect = paint::text_spaced(
                &p,
                left,
                y + 4.5,
                Align::Min,
                &text,
                big.clone(),
                colour,
                0.75,
            );
            paint::text(
                &p,
                rect.right(),
                y + 9.5,
                Align::Min,
                &format!(".{tenth}"),
                small.clone(),
                colour,
            );
        };
        clock_at(r - 268.0, remaining, "-", theme::TEXT);
        clock_at(r - 199.5, elapsed, "", theme::TEXT_KEY);
        let key = loaded.key.map(|k| k.camelot()).unwrap_or_default();
        paint::text(
            &p,
            r - 103.0,
            y + 6.0,
            Align::Center,
            &key,
            theme::regular(12.5),
            theme::TEXT,
        );
        let bpm = state
            .bpm
            .or(track.bpm.map(|b| b.as_f64() * state.tempo))
            .map_or("--.--".to_owned(), |b| format!("{b:.2}"));
        paint::text(
            &p,
            r - 19.0,
            y + 6.0,
            Align::Max,
            &bpm,
            theme::regular(13.0),
            theme::TEXT,
        );
    }

    /// Full-track overview with its rail, at y 22-56 of the player.
    fn overview(
        &self,
        ui: &Ui,
        main: Rect,
        deck: &DeckView,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
    ) {
        let area = Rect::from_min_max(
            pos2(main.left() + 83.0, main.top() + 22.0),
            pos2(main.right() - 10.5, main.top() + 60.0),
        );
        let p = ui.painter_at(area.expand(8.0));
        let base = main.top() + 50.5;
        let Some(loaded) = &deck.loaded else { return };
        let frames = state.frames as f64;
        if frames <= 0.0 {
            return;
        }
        let x_of = |frame: f64| area.left() + (frame / frames) as f32 * area.width();
        let playhead = x_of(state.position);
        // Rail: two grey lines under the waveform, light where played.
        paint::fill(
            &p,
            Rect::from_x_y_ranges(area.left()..=area.right(), base + 3.5..=base + 4.5),
            wave::RAIL,
        );
        paint::fill(
            &p,
            Rect::from_x_y_ranges(area.left()..=area.right(), base + 5.5..=base + 6.0),
            wave::RAIL_DARK,
        );
        paint::fill(
            &p,
            Rect::from_x_y_ranges(area.left()..=playhead, base + 3.0..=base + 5.5),
            wave::RAIL_PLAYED,
        );
        if let Some(bands) = &loaded.bands
            && bands.points_per_second > 0
            && !bands.low.is_empty()
        {
            // 1200 bins, as rekordbox's 3-band overview, each a stack of
            // layers from the baseline sized from the bin's mean band levels
            // (heights measured against rekordbox's drawing).
            const BINS: usize = 1200;
            let points = bands.low.len();
            let bin_width = area.width() / BINS as f32;
            for bin in 0..BINS {
                let start = bin * points / BINS;
                let end = ((bin + 1) * points / BINS).max(start + 1).min(points);
                let (l, m, h) = bands.means(start, end);
                let low = (0.082 * l).max(1.0);
                let mid = low + 0.0352 * m + 0.9;
                let high = mid + 0.072 * h;
                let left = paint::snap(area.left() + bin as f32 * bin_width);
                let right = paint::snap(area.left() + (bin + 1) as f32 * bin_width).max(left + 0.5);
                for (top, colour) in [
                    (high, wave::HIGH),
                    (mid, wave::OVERVIEW_MID),
                    (low, wave::OVERVIEW_LOW),
                ] {
                    let height = paint::snap(top.min(30.0));
                    paint::fill(
                        &p,
                        Rect::from_min_max(pos2(left, base - height), pos2(right, base)),
                        colour,
                    );
                }
            }
        }
        // Memory cues (and the deck's cue point) as orange triangles under
        // the waveform; hot cues as lettered boxes above it.
        let rate = f64::from(sample_rate.max(1));
        let mut triangles: Vec<f32> = loaded
            .track
            .cues
            .iter()
            .filter(|c| c.hot_cue.is_none())
            .map(|c| x_of(c.start * rate))
            .collect();
        triangles.push(x_of(state.cue));
        for cue in triangles {
            paint::triangle(
                &p,
                [
                    pos2(cue, base - 1.0),
                    pos2(cue - 6.0, base + 8.5),
                    pos2(cue + 6.0, base + 8.5),
                ],
                theme::CUE_ORANGE,
            );
        }
        for cue in &loaded.track.cues {
            if let Some(letter) = cue.slot_letter() {
                hot_cue_box(
                    &p,
                    pos2(x_of(cue.start * rate) - 5.5, main.top() + 21.0),
                    letter,
                    cue,
                );
            }
        }
        paint::line(
            &p,
            pos2(playhead, area.top() - 1.0),
            pos2(playhead, base + 10.0),
            1.0,
            theme::TEXT_DIM,
        );
        let response = ui.interact(
            area,
            ui.scope_id().with(("overview", deck.index)),
            Sense::click_and_drag(),
        );
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let fraction = ((pointer.x - area.left()) / area.width()).clamp(0.0, 1.0);
            commands.push(Command::Seek {
                deck: deck.index,
                frame: f64::from(fraction) * frames,
            });
        }
    }

    /// Zoomed 3-band waveform, centred on the playhead, y 82-206.
    fn zoomed(
        &mut self,
        ui: &Ui,
        main: Rect,
        deck: &DeckView,
        state: &DeckState,
        sample_rate: u32,
    ) {
        let area = Rect::from_min_max(
            pos2(main.left(), main.top() + 70.0),
            pos2(main.right(), main.top() + 208.0),
        );
        let p = ui.painter_at(area);
        let (x, y) = (main.left(), main.top());
        // Zoom buttons at x 94 (window), y 112 / 135 / 162.
        let plus = pos2(x + 13.5, y + 112.5);
        let minus = pos2(x + 13.5, y + 162.5);
        paint::magnifier(&p, plus, 5.5, Some(true), theme::TEXT);
        paint::text(
            &p,
            x + 6.5,
            y + 134.5,
            Align::Min,
            "RST",
            theme::regular(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        paint::magnifier(&p, minus, 5.5, Some(false), theme::TEXT);
        if click(
            ui,
            Rect::from_center_size(plus, vec2(18.0, 18.0)),
            "zoom_in",
        )
        .clicked()
        {
            self.zoom = (self.zoom * ZOOM_STEP).min(DEFAULT_ZOOM * 16.0);
        }
        if click(
            ui,
            Rect::from_min_size(pos2(x + 4.0, y + 130.0), vec2(20.0, 12.0)),
            "zoom_reset",
        )
        .clicked()
        {
            self.zoom = DEFAULT_ZOOM;
        }
        if click(
            ui,
            Rect::from_center_size(minus, vec2(18.0, 18.0)),
            "zoom_out",
        )
        .clicked()
        {
            self.zoom = (self.zoom / ZOOM_STEP).max(DEFAULT_ZOOM / 8.0);
        }

        let Some(loaded) = &deck.loaded else { return };
        if !state.loaded {
            return;
        }
        let wave_rect = Rect::from_min_max(pos2(x + 3.0, y + 87.0), pos2(main.right(), y + 201.0));
        let wp = ui.painter_at(wave_rect);
        let centre_x = paint::snap((wave_rect.left() + wave_rect.right()) / 2.0);
        let centre_y = y + 144.0;
        let rate = f64::from(sample_rate.max(1));
        let now = state.position / rate;
        let pts_per_sec = f64::from(self.zoom);
        let t_of = |px: f32| now + f64::from(px - centre_x) / pts_per_sec;
        let x_of = |t: f64| centre_x + ((t - now) * pts_per_sec) as f32;
        let (start, end) = (t_of(wave_rect.left()), t_of(wave_rect.right()));

        // Beat lines behind the waveform; bars are white.
        let beats = loaded.grid.beats_between(start.max(0.0), end);
        for &(time, downbeat) in &beats {
            let bx = paint::snap(x_of(time));
            let colour = if downbeat {
                theme::TEXT
            } else {
                theme::BEAT_LINE
            };
            paint::line(&wp, pos2(bx, y + 87.0), pos2(bx, y + 201.0), 1.0, colour);
        }
        if let Some(bands) = &loaded.bands
            && bands.points_per_second > 0
        {
            let pps = f64::from(bands.points_per_second);
            let half = 53.0;
            let columns = (wave_rect.width() * 2.0) as usize;
            for column in 0..columns {
                let cx = wave_rect.left() + column as f32 * 0.5;
                let t0 = t_of(cx);
                if t0 < 0.0 {
                    continue;
                }
                let first = (t0 * pps) as usize;
                if first >= bands.low.len() {
                    break;
                }
                let last = ((t_of(cx + 0.5) * pps) as usize).max(first + 1);
                let (l, m, h) = bands.peaks(first, last);
                let amps = [
                    (f32::from(l) / 255.0 * half, 0b001u8),
                    (f32::from(m) / 255.0 * half, 0b010),
                    (f32::from(h) / 255.0 * half, 0b100),
                ];
                detail_column(&wp, cx, centre_y, amps);
            }
        }
        // Grid markers: small triangles at both ends of each beat line.
        for &(time, downbeat) in &beats {
            let bx = paint::snap(x_of(time)) + 0.25;
            let colour = if downbeat {
                theme::BAR_LINE
            } else {
                theme::BEAT_LINE
            };
            let w = if downbeat { 3.5 } else { 2.0 };
            paint::triangle(
                &wp,
                [
                    pos2(bx - w, y + 87.0),
                    pos2(bx + w, y + 87.0),
                    pos2(bx, y + 92.5),
                ],
                colour,
            );
            paint::triangle(
                &wp,
                [
                    pos2(bx - w, y + 201.0),
                    pos2(bx + w, y + 201.0),
                    pos2(bx, y + 195.5),
                ],
                colour,
            );
        }
        // Cue point and memory cues: orange triangles under the waveform;
        // hot cues: lettered boxes over the bar markers.
        let mut triangles: Vec<f32> = loaded
            .track
            .cues
            .iter()
            .filter(|c| c.hot_cue.is_none())
            .map(|c| x_of(c.start))
            .collect();
        triangles.push(x_of(state.cue / rate));
        for cue_x in triangles {
            paint::triangle(
                &p,
                [
                    pos2(cue_x, y + 195.0),
                    pos2(cue_x - 7.0, y + 206.0),
                    pos2(cue_x + 7.0, y + 206.0),
                ],
                theme::CUE_ORANGE,
            );
        }
        for cue in &loaded.track.cues {
            if let Some(letter) = cue.slot_letter() {
                hot_cue_box(&p, pos2(x_of(cue.start) - 5.5, y + 77.0), letter, cue);
            }
        }
        // Playhead, with the bar.beat position to its left.
        paint::line(
            &p,
            pos2(centre_x, y + 70.5),
            pos2(centre_x, y + 206.0),
            1.0,
            theme::TEXT_DIM,
        );
        if let Some(beat) = state.beat {
            let label = bar_beat(&loaded.grid, beat);
            paint::text(
                &p,
                centre_x - 2.0,
                y + 73.5,
                Align::Max,
                &label,
                theme::bold(13.0),
                theme::BARS_TEXT,
            );
        }
    }

    fn lower_strip(
        &mut self,
        ui: &Ui,
        main: Rect,
        deck: &mut DeckView,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
    ) {
        let strip = Rect::from_min_max(pos2(main.left(), main.bottom() - 48.0), main.max);
        let p = ui.painter_at(strip);
        let (x, y) = (strip.left(), strip.top());
        // Tabs: CUE/LOOP over GRID, 80 wide; the selected one joins the strip.
        let tabs = [
            (
                LowerTab::CueLoop,
                "CUE/LOOP",
                Rect::from_min_size(pos2(x, y), vec2(83.0, 24.0)),
            ),
            (
                LowerTab::Grid,
                "GRID",
                Rect::from_min_size(pos2(x, y + 24.0), vec2(83.0, 24.0)),
            ),
        ];
        paint::fill(
            &p,
            Rect::from_min_max(pos2(x + 83.0, y), strip.max),
            theme::PANEL,
        );
        for (tab, label, rect) in tabs {
            let fill_rect = Rect::from_min_max(pos2(rect.left() + 3.0, rect.top()), rect.max);
            paint::fill(&p, rect, theme::BLACK);
            if self.lower_tab == tab {
                paint::fill(&p, fill_rect, theme::PANEL);
            }
            paint::text(
                &p,
                fill_rect.center().x,
                rect.top() + 8.5,
                Align::Center,
                label,
                theme::bold(theme::BODY_SIZE * 0.85),
                theme::TEXT,
            );
            if click(ui, rect, ("lower_tab", label)).clicked() {
                self.lower_tab = tab;
            }
        }
        let row = y + 10.0;
        match self.lower_tab {
            LowerTab::CueLoop => {
                self.cue_loop_controls(ui, &p, strip, deck, state, sample_rate, commands)
            }
            LowerTab::Grid => {
                self.grid_edit_controls(ui, &p, strip, deck, state, sample_rate, commands)
            }
        }
        let small = |p: &egui::Painter, rect: Rect| paint::rounded(p, rect, 2.0, theme::BUTTON);
        // Quantize and the menu, anchored to the right edge.
        let q = Rect::from_min_size(pos2(strip.right() - 94.0, row + 4.0), vec2(33.0, 19.5));
        if self.quantize {
            paint::rounded(&p, q, 2.0, theme::ACCENT);
        } else {
            paint::outline(&p, q, 2.0, theme::OUTLINE);
        }
        paint::text_centred(
            &p,
            q,
            "Q",
            theme::bold(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        if click(ui, q, "quantize").clicked() {
            self.quantize = !self.quantize;
        }
        let menu = Rect::from_min_size(pos2(strip.right() - 50.5, row + 4.0), vec2(20.0, 20.0));
        small(&p, menu);
        paint::hamburger(&p, menu.center(), 13.0, theme::TEXT);
    }

    /// CUE/LOOP: hot cue pads, memory cue navigation and beat loops.
    #[allow(clippy::too_many_arguments)]
    fn cue_loop_controls(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        strip: Rect,
        deck: &DeckView,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
    ) {
        let (x, y) = (strip.left(), strip.top());
        let row = y + 10.0;
        // The control group is centred between the tabs and the right edge;
        // in a 1220 pt player it starts 244 pt after the tabs.
        let group_width = 649.0;
        let gx = ((x + 83.0 + strip.right()) / 2.0 - group_width / 2.0).round();
        let rate = f64::from(sample_rate.max(1));
        let cues: &[Cue] = deck.loaded.as_ref().map_or(&[], |l| &l.track.cues);
        for (i, letter) in ["A", "B", "C", "D", "E", "F", "G", "H"].iter().enumerate() {
            let b = Rect::from_min_size(pos2(gx + i as f32 * 33.0, row), vec2(28.0, 28.0));
            paint::rounded(p, b, 2.0, theme::BUTTON_DARK);
            let cue = cues.iter().find(|c| c.hot_cue == Some(i as u8));
            if let Some(cue) = cue {
                let square = Rect::from_center_size(b.center(), vec2(15.0, 15.0));
                paint::fill(p, square, theme::cue_colour(cue));
                paint::text_centred(p, square, letter, theme::regular(12.5), theme::CUE_LETTER);
            } else {
                paint::text_centred(p, b, letter, theme::regular(12.5), theme::TEXT_OFF);
            }
            if let Some(cue) = cue
                && click(ui, b, ("hot_cue", i)).clicked()
            {
                commands.push(Command::Seek {
                    deck: deck.index,
                    frame: cue.start * rate,
                });
            }
        }
        let now = state.position / rate;
        let memory_cues: Vec<f64> = cues
            .iter()
            .filter(|c| c.hot_cue.is_none())
            .map(|c| c.start)
            .collect();
        let mut bx = gx + 289.0;
        let small = |p: &egui::Painter, rect: Rect| paint::rounded(p, rect, 2.0, theme::BUTTON);
        let memory = Rect::from_min_size(pos2(bx, row + 4.0), vec2(60.0, 20.0));
        small(p, memory);
        paint::text_centred(
            p,
            memory,
            "MEMORY",
            theme::bold(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        bx += 64.0;
        for (i, glyph) in ["prev", "next", "delete"].iter().enumerate() {
            let b = Rect::from_min_size(pos2(bx + i as f32 * 25.5, row + 4.0), vec2(20.0, 20.0));
            small(p, b);
            if click(ui, b, ("memory", i)).clicked() {
                let target = match *glyph {
                    "prev" => memory_cues
                        .iter()
                        .copied()
                        .filter(|&t| t < now - 0.05)
                        .reduce(f64::max),
                    "next" => memory_cues
                        .iter()
                        .copied()
                        .filter(|&t| t > now + 0.05)
                        .reduce(f64::min),
                    _ => None,
                };
                if let Some(t) = target {
                    commands.push(Command::Seek {
                        deck: deck.index,
                        frame: t * rate,
                    });
                }
            }
            match *glyph {
                "prev" => paint::play_triangle(p, b.center(), 9.0, false, theme::TEXT),
                "next" => paint::play_triangle(p, b.center(), 9.0, true, theme::TEXT),
                _ => paint::cross(p, b.center(), 9.0, 1.4, theme::TEXT),
            }
        }
        bx += 101.0;
        let au = Rect::from_min_size(pos2(bx, row + 4.0), vec2(36.5, 20.0));
        paint::rounded(p, au, 2.0, theme::ACCENT);
        paint::text_centred(
            p,
            au,
            "AU",
            theme::bold(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        let ma = Rect::from_min_size(pos2(bx + 39.5, row + 4.0), vec2(36.5, 20.0));
        paint::outline(p, ma, 2.0, theme::OUTLINE);
        paint::text_centred(
            p,
            ma,
            "MA",
            theme::bold(theme::BODY_SIZE * 0.85),
            theme::TEXT,
        );
        bx += 81.0;
        let prev = Rect::from_min_size(pos2(bx, row + 4.0), vec2(20.0, 20.0));
        let field = Rect::from_min_size(pos2(bx + 22.0, row + 4.0), vec2(70.0, 20.0));
        let next = Rect::from_min_size(pos2(bx + 94.0, row + 4.0), vec2(20.0, 20.0));
        small(p, prev);
        small(p, field);
        small(p, next);
        paint::chevron_side(p, prev.center(), 10.0, false, theme::TEXT);
        paint::chevron_side(p, next.center(), 10.0, true, theme::TEXT);
        paint::text_centred(
            p,
            field,
            &self.beat_loop.to_string(),
            theme::regular(12.5),
            theme::TEXT,
        );
        if click(ui, prev, "loop_shorter").clicked() && self.beat_loop > 1 {
            self.beat_loop /= 2;
        }
        if click(ui, next, "loop_longer").clicked() && self.beat_loop < 512 {
            self.beat_loop *= 2;
        }
    }

    /// GRID EDIT: tempo, tap, grid shift and stretch, tempo doubling and
    /// halving, undo and redo. PHRASE EDIT is shown but not implemented.
    #[allow(clippy::too_many_arguments)]
    fn grid_edit_controls(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        strip: Rect,
        deck: &mut DeckView,
        state: &DeckState,
        sample_rate: u32,
        commands: &mut Vec<Command>,
    ) {
        let (x, y) = (strip.left(), strip.top());
        let track = deck.loaded.as_ref().map(|l| l.track.id.clone());
        if track != self.grid_track {
            self.grid_undo.clear();
            self.grid_redo.clear();
            self.taps.clear();
            self.grid_track = track;
        }
        // Positions in a 1220 pt player, from x 173 (window) onwards.
        let gx = x + 93.0;
        let label = theme::bold(theme::BODY_SIZE * 0.85);
        paint::text(
            p,
            gx,
            y + 3.0,
            Align::Min,
            "GRID EDIT",
            label.clone(),
            theme::TEXT,
        );
        let row = y + 12.0;
        let button = |dx: f32, w: f32| Rect::from_min_size(pos2(gx + dx, row), vec2(w, 24.0));
        let rate = f64::from(sample_rate.max(1));
        let now = state.position / rate;
        let grid = deck.loaded.as_ref().map(|l| l.grid.clone());
        let has_grid = grid.as_ref().is_some_and(|g| !g.is_empty());
        let mut edit: Option<cuepoint_core::BeatGrid> = None;

        // Downbeat marker button.
        let marker = button(0.0, 24.0);
        paint::rounded(p, marker, 2.0, theme::BUTTON);
        paint::fill(
            p,
            Rect::from_min_size(marker.center() + vec2(-1.0, -6.0), vec2(2.0, 4.0)),
            theme::TEXT,
        );
        paint::fill(
            p,
            Rect::from_min_size(marker.center() + vec2(-1.0, -2.0), vec2(2.0, 8.0)),
            theme::BAR_LINE,
        );
        // Tempo field and TAP.
        let field = button(34.0, 50.0);
        paint::rounded(p, field, 2.0, theme::BUTTON_DARK);
        let bpm = grid
            .as_ref()
            .and_then(|g| g.bpm_at(now))
            .map_or("--.--".to_owned(), |b| format!("{b:.2}"));
        paint::text(
            p,
            field.right() - 9.0,
            row + 7.5,
            Align::Max,
            &bpm,
            theme::regular(12.0),
            theme::TEXT,
        );
        let tap = button(88.0, 22.0);
        paint::text_centred(
            p,
            tap,
            "TAP",
            theme::bold(theme::BODY_SIZE),
            theme::TEXT_OFF,
        );
        if has_grid && click(ui, tap, "grid_tap").clicked() {
            let t = ui.input(|i| i.time);
            if self.taps.last().is_some_and(|&last| t - last > 2.0) {
                self.taps.clear();
            }
            self.taps.push(t);
            if self.taps.len() >= 4 {
                let span = self.taps[self.taps.len() - 1] - self.taps[0];
                let tapped = 60.0 * (self.taps.len() - 1) as f64 / span;
                edit = grid
                    .as_ref()
                    .map(|g| g.with_constant_tempo((tapped * 100.0).round() / 100.0));
            }
        }
        // Shift, stretch, double and halve.
        let tools: [(f32, &str); 8] = [
            (123.0, "shift_left"),
            (152.0, "shift_right"),
            (186.0, "widen"),
            (215.0, "narrow"),
            (249.0, "double"),
            (278.0, "halve"),
            (307.0, "unused_a"),
            (341.0, "unused_b"),
        ];
        for (dx, name) in tools {
            let b = button(dx, 24.0);
            paint::rounded(p, b, 2.0, theme::BUTTON);
            let c = b.center();
            let enabled = has_grid && !name.starts_with("unused");
            let colour = if enabled {
                theme::TEXT
            } else {
                theme::TEXT_OFF
            };
            let bars = |p: &egui::Painter, centre: egui::Pos2, colour| {
                for dx in [-2.5, 0.0, 2.5] {
                    paint::line(
                        p,
                        centre + vec2(dx, -5.0),
                        centre + vec2(dx, 5.0),
                        1.0,
                        theme::TEXT_DIM,
                    );
                }
                let _ = colour;
            };
            match name {
                "shift_left" => {
                    paint::play_triangle(p, c + vec2(-6.0, 0.0), 6.0, false, colour);
                    bars(p, c + vec2(2.0, 0.0), colour);
                }
                "shift_right" => {
                    bars(p, c + vec2(-2.0, 0.0), colour);
                    paint::play_triangle(p, c + vec2(6.0, 0.0), 6.0, true, colour);
                }
                "widen" => {
                    paint::play_triangle(p, c + vec2(-7.0, 0.0), 6.0, false, colour);
                    bars(p, c, colour);
                    paint::play_triangle(p, c + vec2(7.0, 0.0), 6.0, true, colour);
                }
                "narrow" => {
                    paint::play_triangle(p, c + vec2(-7.0, 0.0), 6.0, true, colour);
                    bars(p, c, colour);
                    paint::play_triangle(p, c + vec2(7.0, 0.0), 6.0, false, colour);
                }
                "double" => {
                    bars(p, c + vec2(-5.0, 0.0), colour);
                    paint::text(
                        p,
                        c.x + 0.5,
                        c.y - 3.5,
                        Align::Min,
                        "x2",
                        theme::bold(10.0),
                        colour,
                    );
                }
                "halve" => {
                    bars(p, c + vec2(-6.5, 0.0), colour);
                    paint::text(
                        p,
                        c.x - 2.5,
                        c.y - 3.0,
                        Align::Min,
                        "x",
                        theme::bold(11.0),
                        colour,
                    );
                    let fx = c.x + 7.0;
                    paint::text(
                        p,
                        fx,
                        c.y - 8.5,
                        Align::Center,
                        "1",
                        theme::bold(8.0),
                        colour,
                    );
                    paint::line(p, pos2(fx - 3.0, c.y), pos2(fx + 3.0, c.y), 1.0, colour);
                    paint::text(
                        p,
                        fx,
                        c.y + 2.0,
                        Align::Center,
                        "2",
                        theme::bold(8.0),
                        colour,
                    );
                }
                _ => bars(p, c, colour),
            }
            if enabled && click(ui, b, ("grid_tool", name)).clicked() {
                // One waveform point (1/150 s) per shift; half a
                // millisecond per beat per stretch.
                edit = grid.as_ref().map(|g| match name {
                    "shift_left" => g.shifted(-1.0 / 150.0),
                    "shift_right" => g.shifted(1.0 / 150.0),
                    "narrow" => g.stretched(-0.0005),
                    "widen" => g.stretched(0.0005),
                    "double" => g.tempo_scaled(2.0),
                    _ => g.tempo_scaled(0.5),
                });
            }
        }
        // Undo and redo.
        let undo = button(376.0, 20.0);
        let redo = button(405.0, 20.0);
        let undo_on = !self.grid_undo.is_empty();
        let redo_on = !self.grid_redo.is_empty();
        paint::curved_arrow(
            p,
            undo.center(),
            true,
            if undo_on {
                theme::TEXT
            } else {
                theme::TEXT_OFF
            },
        );
        paint::curved_arrow(
            p,
            redo.center(),
            false,
            if redo_on {
                theme::TEXT
            } else {
                theme::TEXT_OFF
            },
        );
        if undo_on
            && click(ui, undo, "grid_undo").clicked()
            && let (Some(previous), Some(current)) = (self.grid_undo.pop(), grid.clone())
        {
            self.grid_redo.push(current);
            self.apply_grid(deck, previous, commands);
        } else if redo_on
            && click(ui, redo, "grid_redo").clicked()
            && let (Some(next), Some(current)) = (self.grid_redo.pop(), grid.clone())
        {
            self.grid_undo.push(current);
            self.apply_grid(deck, next, commands);
        }
        // Metronome, click and analysis lock buttons.
        for (dx, kind) in [(431.5, 0), (460.5, 1), (489.5, 2)] {
            let b = button(dx, 24.0);
            paint::rounded(p, b, 2.0, theme::BUTTON);
            let c = b.center();
            match kind {
                0 => {
                    paint::outline(
                        p,
                        Rect::from_center_size(c, vec2(9.0, 12.0)),
                        1.0,
                        theme::TEXT,
                    );
                    paint::line(
                        p,
                        c + vec2(-3.0, -5.0),
                        c + vec2(3.0, 5.0),
                        1.4,
                        theme::TEXT,
                    );
                }
                1 => {
                    paint::fill(
                        p,
                        Rect::from_center_size(c + vec2(-3.0, 1.0), vec2(1.5, 5.0)),
                        theme::ACCENT,
                    );
                    paint::fill(
                        p,
                        Rect::from_center_size(c + vec2(0.0, 0.0), vec2(1.5, 8.0)),
                        theme::ACCENT,
                    );
                    paint::fill(
                        p,
                        Rect::from_center_size(c + vec2(3.0, 0.0), vec2(1.5, 12.0)),
                        theme::TEXT,
                    );
                }
                _ => paint::open_lock(p, c + vec2(0.0, 1.0), theme::TEXT),
            }
        }
        // Phrase editing, shown disabled.
        let px = x + 800.0;
        paint::text(
            p,
            px,
            y + 3.0,
            Align::Min,
            "PHRASE EDIT",
            label.clone(),
            theme::TEXT_DIM,
        );
        let cut = Rect::from_min_size(pos2(px, row), vec2(50.0, 24.0));
        let clear = Rect::from_min_size(pos2(px + 67.0, row), vec2(50.0, 24.0));
        for (b, text) in [(cut, "CUT"), (clear, "CLEAR")] {
            paint::rounded(p, b, 2.0, theme::PHRASE_BUTTON);
            paint::text_centred(p, b, text, label.clone(), theme::TEXT_OFF);
        }

        if let (Some(new), Some(current)) = (edit, grid) {
            self.grid_undo.push(current);
            self.grid_redo.clear();
            self.apply_grid(deck, new, commands);
        }
    }

    /// Puts `grid` on the loaded track and the engine's deck.
    fn apply_grid(
        &mut self,
        deck: &mut DeckView,
        grid: cuepoint_core::BeatGrid,
        commands: &mut Vec<Command>,
    ) {
        if let Some(loaded) = &mut deck.loaded {
            loaded.grid = grid.clone();
            commands.push(Command::SetGrid {
                deck: deck.index,
                grid: Some(std::sync::Arc::new(grid)),
            });
        }
    }

    fn side_panel(
        &mut self,
        ui: &Ui,
        rect: Rect,
        deck: &DeckView,
        state: &DeckState,
        sample_rate: u32,
    ) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::PANEL);
        let (x, y) = (rect.left(), rect.top());
        // Eight rows of 26 pt with 1 pt gaps, from y 5.
        let list = Rect::from_min_max(pos2(x + 8.0, y + 5.0), pos2(rect.right() - 8.0, y + 221.0));
        let cues: &[Cue] = deck.loaded.as_ref().map_or(&[], |l| &l.track.cues);
        let entry = |p: &egui::Painter, row: Rect, cue: &Cue, letter: Option<char>| {
            let square =
                Rect::from_min_size(pos2(row.left() + 10.0, row.top() + 7.5), vec2(11.0, 11.0));
            paint::fill(p, square, theme::cue_colour(cue));
            if let Some(letter) = letter {
                paint::text_centred(
                    p,
                    square,
                    &letter.to_string(),
                    theme::regular(10.5),
                    theme::CUE_LETTER,
                );
            }
            let (whole, _) = clock(cue.start);
            paint::text(
                p,
                row.left() + 31.5,
                row.top() + 9.0,
                Align::Min,
                &whole,
                theme::regular(theme::BODY_SIZE),
                theme::TEXT,
            );
            paint::text_clipped(
                p,
                row.left() + 67.0,
                row.top() + 9.0,
                row.width() - 95.0,
                Align::Min,
                &cue.name,
                theme::regular(theme::BODY_SIZE),
                theme::TEXT,
            );
            paint::cross(
                p,
                pos2(row.right() - 17.0, row.center().y),
                8.0,
                1.3,
                theme::TEXT,
            );
        };
        let memory: Vec<&Cue> = cues.iter().filter(|c| c.hot_cue.is_none()).collect();
        match self.side_tab {
            // Eight 26 pt rows, A to H.
            SideTab::HotCue => {
                for i in 0..8 {
                    let row = Rect::from_min_size(
                        pos2(list.left(), list.top() + i as f32 * 27.0),
                        vec2(list.width(), 26.0),
                    );
                    paint::fill(&p, row, theme::BAR);
                    let letter = char::from(b'A' + i as u8);
                    match cues.iter().find(|c| c.hot_cue == Some(i as u8)) {
                        Some(cue) => entry(&p, row, cue, Some(letter)),
                        None => {
                            let text = letter.to_string();
                            let font = theme::regular(theme::BODY_SIZE);
                            paint::text(
                                &p,
                                row.left() + 13.0,
                                row.top() + 10.0,
                                Align::Min,
                                &text,
                                font,
                                theme::TEXT_OFF,
                            );
                        }
                    }
                }
            }
            // Ten 21 pt rows of memory cues, each with its delete cross.
            SideTab::Memory => {
                for i in 0..10 {
                    let row = Rect::from_min_size(
                        pos2(list.left(), y + 6.0 + i as f32 * 22.0),
                        vec2(list.width(), 21.0),
                    );
                    paint::fill(&p, row, theme::BAR);
                    match memory.get(i) {
                        Some(cue) => memory_entry(&p, row, cue),
                        None => paint::cross(
                            &p,
                            pos2(row.right() - 10.0, row.center().y),
                            7.0,
                            1.0,
                            theme::TEXT_OFF,
                        ),
                    }
                }
            }
            SideTab::Info => {
                if let Some(loaded) = &deck.loaded {
                    let area = Rect::from_min_max(
                        pos2(list.left(), y + 6.0),
                        pos2(list.right(), y + 225.0),
                    );
                    paint::fill(&p, area, theme::BAR);
                    info_panel(&p, area, &loaded.track);
                    let bar = Rect::from_min_max(
                        pos2(area.right() - 4.0, area.top()),
                        pos2(area.right() - 1.5, area.bottom() - 25.0),
                    );
                    paint::rounded(&p, bar, 1.0, theme::BEAT_LINE);
                }
            }
        }
        let _ = (state, sample_rate);
        let tabs = [
            (SideTab::Memory, "MEMORY", 8.0),
            (SideTab::HotCue, "HOT CUE", 72.0),
            (SideTab::Info, "INFO", 136.0),
        ];
        for (tab, label, tx) in tabs {
            let b = Rect::from_min_size(pos2(x + tx, y + 229.5), vec2(61.5, 18.5));
            if self.side_tab == tab {
                paint::rounded(&p, b, 2.0, theme::ACCENT);
            } else {
                paint::outline(&p, b, 2.0, theme::OUTLINE);
            }
            paint::text_centred(
                &p,
                b,
                label,
                theme::bold(theme::BODY_SIZE * 0.85),
                theme::TEXT,
            );
            if click(ui, b, ("side_tab", label)).clicked() {
                self.side_tab = tab;
            }
        }
    }
}

/// A memory cue row: orange marker, time and label.
fn memory_entry(p: &egui::Painter, row: Rect, cue: &Cue) {
    let c = pos2(row.left() + 15.5, row.center().y);
    paint::triangle(
        p,
        [c + vec2(0.0, -4.0), c + vec2(-5.0, 4.0), c + vec2(5.0, 4.0)],
        theme::CUE_ORANGE,
    );
    let (whole, _) = clock(cue.start);
    let font = theme::regular(theme::BODY_SIZE);
    paint::text(
        p,
        row.left() + 31.5,
        row.top() + 7.0,
        Align::Min,
        &whole,
        font.clone(),
        theme::TEXT,
    );
    paint::text_clipped(
        p,
        row.left() + 67.0,
        row.top() + 7.0,
        row.width() - 95.0,
        Align::Min,
        &cue.name,
        font,
        theme::TEXT,
    );
    paint::cross(
        p,
        pos2(row.right() - 10.0, row.center().y),
        8.0,
        1.3,
        theme::TEXT,
    );
}

/// INFO: rating, comment and file details of the loaded track.
fn info_panel(p: &egui::Painter, area: Rect, track: &cuepoint_core::Track) {
    let (x, y) = (area.left() + 10.0, area.top() - 6.0);
    for i in 0..5u8 {
        let c = pos2(x + 5.75 + f32::from(i) * 16.0, y + 21.5);
        if i < track.rating.stars() {
            paint::star(p, c, 5.5, theme::TEXT);
        } else {
            paint::star(p, c, 6.0, theme::STAR_OFF);
            paint::star(p, c, 5.0, theme::BAR);
        }
    }
    for (sy, right) in [(37.5, 170.0), (68.5, 170.0), (116.5, 162.0)] {
        paint::fill(
            p,
            Rect::from_min_size(pos2(x, y + sy), vec2(right, 1.0)),
            theme::SEPARATOR,
        );
    }
    // Comment bubble and text.
    let bubble = Rect::from_min_size(pos2(x, y + 78.0), vec2(12.0, 7.5));
    paint::rounded(p, bubble, 2.0, theme::TEXT_DIM);
    paint::triangle(
        p,
        [
            bubble.left_bottom() + vec2(2.0, 0.0),
            bubble.left_bottom() + vec2(6.0, 0.0),
            bubble.left_bottom() + vec2(2.0, 3.0),
        ],
        theme::TEXT_DIM,
    );
    let font = theme::regular(theme::BODY_SIZE);
    paint::text_clipped(
        p,
        x,
        y + 97.5,
        area.width() - 20.0,
        Align::Min,
        &track.comment,
        font.clone(),
        theme::TEXT_DIM,
    );
    let size = track
        .file_size
        .map(|b| format!("{:.1} MB", b as f64 / 1_048_576.0));
    let rate = track.sample_rate.map(|r| format!("{r}Hz"));
    let bits = track.bit_rate.map(|b| format!("{b}kbps"));
    let lines = [
        Some(track.kind.clone()).filter(|k| !k.is_empty()),
        size,
        rate,
        bits,
    ];
    for (i, line) in lines.iter().flatten().enumerate() {
        paint::text(
            p,
            x,
            y + 130.0 + i as f32 * 16.0,
            Align::Min,
            line,
            font.clone(),
            theme::TEXT_DIM,
        );
    }
}

/// The 11 pt lettered box marking a hot cue on the waveforms.
fn hot_cue_box(p: &egui::Painter, top_left: egui::Pos2, letter: char, cue: &Cue) {
    let square = Rect::from_min_size(top_left, vec2(11.0, 11.0));
    paint::fill(p, square, theme::cue_colour(cue));
    paint::text_centred(
        p,
        square,
        &letter.to_string(),
        theme::regular(10.5),
        theme::CUE_LETTER,
    );
}

/// Colours one 0.5 pt column of the zoomed waveform. Each pixel's colour
/// depends on which bands reach it, so overlapping bands blend.
fn detail_column(p: &egui::Painter, x: f32, centre: f32, mut amps: [(f32, u8); 3]) {
    amps.sort_by(|a, b| b.0.total_cmp(&a.0));
    let colour_of = |mask: u8| match mask {
        0b001 => wave::LOW,
        0b010 => wave::MID,
        0b100 => wave::HIGH,
        0b011 => wave::LOW_MID,
        0b110 => wave::MID_HIGH,
        0b101 => wave::LOW_HIGH,
        _ => wave::ALL,
    };
    // From the outside in: the tallest band alone, then the top two, then all.
    let mut mask = 0u8;
    for i in 0..3 {
        mask |= amps[i].1;
        let outer = amps[i].0;
        let inner = if i < 2 { amps[i + 1].0 } else { 0.0 };
        if outer - inner < 0.25 {
            continue;
        }
        let colour = colour_of(mask);
        let top = Rect::from_min_max(pos2(x, centre - outer), pos2(x + 0.5, centre - inner));
        let bottom = Rect::from_min_max(pos2(x, centre + inner), pos2(x + 0.5, centre + outer));
        paint::fill(p, top, colour);
        paint::fill(p, bottom, colour);
    }
}

/// `mm:ss` and tenths, as rekordbox's player shows them.
fn clock(seconds: f64) -> (String, u64) {
    let tenths = (seconds.max(0.0) * 10.0).floor() as u64;
    (
        format!("{:02}:{:02}", tenths / 600, (tenths / 10) % 60),
        tenths % 10,
    )
}

/// "bar.beatBars": the bar counted from the grid's first downbeat (bar 1)
/// and the beat within it, e.g. `3.4Bars`.
fn bar_beat(grid: &BeatGrid, beat: f64) -> String {
    let index = beat.floor() as i64;
    let in_bar = grid.beat_in_bar(index).unwrap_or(1);
    // Beats before the first downbeat belong to bar 0.
    let first_downbeat = (0..4).find(|&b| grid.is_downbeat(b)).unwrap_or(0);
    let bar = (index - first_downbeat).div_euclid(4) + 1;
    format!("{bar}.{in_bar}Bars")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_pads_minutes() {
        assert_eq!(clock(229.45), ("03:49".to_owned(), 4));
        assert_eq!(clock(5.19), ("00:05".to_owned(), 1));
    }

    #[test]
    fn bars_count_from_the_first_downbeat() {
        let grid = BeatGrid::constant(132.0, 0.0, 1);
        assert_eq!(bar_beat(&grid, 0.0), "1.1Bars");
        assert_eq!(bar_beat(&grid, 11.2), "3.4Bars");
    }
}
