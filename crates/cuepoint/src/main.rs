//! Cuepoint desktop app.
//!
//! Usage: `cuepoint [LIBRARY]`, where LIBRARY is a rekordbox XML export or a
//! rekordbox 6/7 `master.db`; or `cuepoint export LIBRARY DEVICE_ROOT` to
//! export the collection to a USB/SD device without opening a window. The master.db key is read from
//! `CUEPOINT_MASTERDB_KEY`, or asked for in the app.

mod analysis_cache;
mod export;
mod player;

use std::path::{Path, PathBuf};
use std::time::Duration;

use cuepoint_library::Library;
use cuepoint_ui::{Browser, BrowserAction, ExportView, Mode};
use player::Player;

const KEY_ENV: &str = "CUEPOINT_MASTERDB_KEY";

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "export") {
        std::process::exit(match args.as_slice() {
            [_, library, dest] => export_cli(Path::new(library), Path::new(dest)),
            _ => {
                eprintln!("usage: cuepoint export LIBRARY DEVICE_ROOT");
                2
            }
        });
    }
    let path = args.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cuepoint")
            // rekordbox's window on a 14" MacBook Pro. The app draws its own
            // 28 pt title bar, as rekordbox's is, under a transparent one.
            .with_inner_size([1512.0, 949.0])
            .with_min_inner_size([1024.0, 628.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "Cuepoint",
        options,
        Box::new(move |cc| {
            cuepoint_ui::theme::install(&cc.egui_ctx);
            let mut app = CuepointApp::new();
            if let Some(path) = path {
                app.open(path, std::env::var(KEY_ENV).ok());
            }
            app.apply_dev_state();
            Ok(Box::new(app))
        }),
    )
}

struct CuepointApp {
    player: Player,
    library: Library,
    /// Where the current library came from.
    loaded_from: Option<PathBuf>,
    browser: Browser,
    mode: Mode,
    export_view: ExportView,
    artwork: cuepoint_ui::ArtworkCache,
    previews: cuepoint_ui::PreviewCache,
    /// Pending `CUEPOINT_DEV_SEEK`, applied once Player A has its track.
    dev_seek: Option<f64>,
    /// egui time of the last input event, to keep redrawing briefly after.
    last_input: f64,
    status: Status,
    key_prompt: Option<KeyPrompt>,
    export: Option<RunningExport>,
}

struct RunningExport {
    job: export::ExportJob,
    status: String,
}

#[derive(Default)]
enum Status {
    #[default]
    Empty,
    Loaded,
    Info(String),
    Error(String),
    /// A refused or failed track load, shown until it expires; then the
    /// footer goes back to the list summary.
    LoadError(String, std::time::Instant),
}

/// How long a load error stays in the footer.
const LOAD_ERROR_SECONDS: u64 = 6;

struct KeyPrompt {
    path: PathBuf,
    key: String,
}

impl CuepointApp {
    fn new() -> Self {
        Self {
            player: Player::new(cache_dir().map(|d| d.join("analysis"))),
            library: Library::default(),
            loaded_from: None,
            browser: Browser::default(),
            mode: Mode::Export,
            export_view: ExportView::default(),
            artwork: cuepoint_ui::ArtworkCache::new(),
            previews: preview_cache(),
            dev_seek: None,
            last_input: 0.0,
            status: Status::default(),
            key_prompt: None,
            export: None,
        }
    }

    /// Development aid for side-by-side comparison with rekordbox:
    /// `CUEPOINT_DEV_PLAYLIST` opens a playlist by name, `CUEPOINT_DEV_LOAD`
    /// loads the track with that title to Player A and `CUEPOINT_DEV_SEEK`
    /// seeks it to a time in seconds once loaded; `CUEPOINT_DEV_TAB=grid`
    /// opens the GRID tab (or `info`/`memory` the side tabs, `artwork` the
    /// artwork list view).
    fn apply_dev_state(&mut self) {
        match std::env::var("CUEPOINT_DEV_TAB").as_deref() {
            Ok("grid") => self.export_view.show_grid_edit(),
            Ok("info") => self.export_view.show_side_tab(cuepoint_ui::SideTab::Info),
            Ok("memory") => self.export_view.show_side_tab(cuepoint_ui::SideTab::Memory),
            Ok("artwork") => self.browser.set_view(cuepoint_ui::ListView::Artwork),
            _ => {}
        }
        if let Ok(name) = std::env::var("CUEPOINT_DEV_PLAYLIST") {
            self.browser.select_playlist_named(&self.library, &name);
        }
        if let Ok(title) = std::env::var("CUEPOINT_DEV_LOAD")
            && let Some(index) = self.library.tracks().iter().position(|t| t.title == title)
        {
            self.browser.select_track(index);
            self.browser_action(BrowserAction::Load {
                track: index,
                deck: Some(0),
            });
            self.dev_seek = std::env::var("CUEPOINT_DEV_SEEK")
                .ok()
                .and_then(|s| s.parse().ok());
        }
    }

    fn start_export(&mut self) {
        if self.library.tracks().is_empty() || self.export.is_some() {
            return;
        }
        let Some(root) = rfd::FileDialog::new()
            .set_title("Choose the USB or SD card to export to")
            .pick_folder()
        else {
            return;
        };
        self.export = Some(RunningExport {
            job: export::start(&self.library, self.browser.source(), root),
            status: "Starting export…".into(),
        });
    }

    fn poll_export(&mut self) {
        let Some(running) = &mut self.export else {
            return;
        };
        while let Ok(update) = running.job.updates.try_recv() {
            match update {
                export::Update::Status(text) => running.status = text,
                export::Update::Finished(result) => {
                    self.status = match result {
                        Ok(report) => Status::Info(format!(
                            "Exported {} tracks and {} playlists ({} audio files already on the device).",
                            report.tracks, report.playlists, report.already_present
                        )),
                        Err(error) => Status::Error(error),
                    };
                    self.export = None;
                    return;
                }
            }
        }
    }

    fn export_window(&mut self, ui: &mut egui::Ui) {
        let Some(running) = &self.export else { return };
        let mut cancel = false;
        egui::Window::new("Exporting to device")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&running.status);
                });
                cancel = ui.button("Cancel").clicked();
            });
        if cancel {
            running.job.cancel();
        }
    }

    fn browser_action(&mut self, action: BrowserAction) {
        match action {
            BrowserAction::Load { track, deck } => {
                let Some(track) = self.library.tracks().get(track).cloned() else {
                    return;
                };
                // Export mode shows only Player A, so that is the free deck.
                let free = match self.mode {
                    Mode::Export => Some(0),
                    Mode::Performance => self.player.free_deck(),
                };
                let Some(deck) = deck.or(free) else {
                    self.load_error("Both decks are playing.".into());
                    return;
                };
                if let Err(message) = self.player.load(deck, track) {
                    self.load_error(message);
                }
            }
        }
    }

    fn load_error(&mut self, message: String) {
        self.status = Status::LoadError(message, std::time::Instant::now());
    }

    /// Expires load errors, and moves Player A's load message to the footer
    /// in Export mode while a track is loaded: the player shows a message
    /// only when it is empty, and a failed load leaves the old track up.
    fn update_status(&mut self) {
        if let Status::LoadError(_, at) = &self.status
            && at.elapsed().as_secs() >= LOAD_ERROR_SECONDS
        {
            self.status = Status::Loaded;
        }
        let deck = &mut self.player.decks[0];
        if self.mode == Mode::Export
            && deck.loaded.is_some()
            && deck.loading.is_none()
            && let Some(message) = deck.message.take()
        {
            self.load_error(message);
        }
    }

    fn open(&mut self, path: PathBuf, key: Option<String>) {
        let result = if is_xml(&path) {
            cuepoint_library::xml::open(&path)
        } else {
            match key {
                Some(key) => cuepoint_library::master_db::open(&path, &key),
                None => {
                    self.key_prompt = Some(KeyPrompt {
                        path,
                        key: String::new(),
                    });
                    return;
                }
            }
        };
        match result {
            Ok(library) => {
                self.library = library;
                self.loaded_from = Some(path);
                self.browser.reset();
                self.status = Status::Loaded;
            }
            Err(error) => self.status = Status::Error(format!("{}: {error}", path.display())),
        }
    }

    /// Library and export commands, shown from the window's context menu.
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Open rekordbox XML…").clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("rekordbox XML", &["xml"])
                .pick_file()
            {
                self.open(path, None);
            }
        }
        if ui.button("Open rekordbox master.db…").clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("rekordbox database", &["db"])
                .pick_file()
            {
                self.open(path, std::env::var(KEY_ENV).ok());
            }
        }
        ui.separator();
        let label = match self.browser.source() {
            cuepoint_ui::Source::Collection => "Export collection to device…",
            cuepoint_ui::Source::Playlist(_) => "Export playlist to device…",
        };
        let enabled = !self.library.tracks().is_empty() && self.export.is_none();
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            ui.close();
            self.start_export();
        }
    }

    /// The footer's right-hand text: the list summary, or a status message,
    /// after a warning when there is no audio output.
    fn footer_text(&self) -> String {
        let text = match &self.status {
            Status::Empty => {
                "No library open. Right-click here to open a rekordbox XML or master.db.".into()
            }
            Status::Loaded => self.browser.summary(&self.library),
            Status::Info(message) | Status::Error(message) | Status::LoadError(message, _) => {
                message.clone()
            }
        };
        if self.player.output_is_silent() {
            format!("{} · {text}", self.player.output_description())
        } else {
            text
        }
    }

    fn key_window(&mut self, ui: &mut egui::Ui) {
        let Some(prompt) = &mut self.key_prompt else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new("master.db key")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(format!(
                    "Enter the database key for {}",
                    prompt.path.display()
                ));
                ui.weak(format!("Tip: set {KEY_ENV} to skip this prompt."));
                let field = ui.add(egui::TextEdit::singleline(&mut prompt.key).password(true));
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                ui.horizontal(|ui| {
                    submit |= ui.button("Open").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.key_prompt = None;
        } else if submit && let Some(prompt) = self.key_prompt.take() {
            self.open(prompt.path, Some(prompt.key));
        }
    }
}

impl eframe::App for CuepointApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_started = std::time::Instant::now();
        self.draw(ui);
        if std::env::var_os("CUEPOINT_DEV_TIMING").is_some()
            && frame_started.elapsed().as_millis() > 8
        {
            eprintln!(
                "[timing] frame built in {:.1} ms",
                frame_started.elapsed().as_secs_f64() * 1e3
            );
        }
    }
}

impl CuepointApp {
    fn draw(&mut self, ui: &mut egui::Ui) {
        self.player.poll_loads();
        self.update_status();
        self.poll_export();
        self.artwork.poll(ui.ctx());
        self.previews.poll();
        let state = self.player.state();
        if state.decks[0].loaded
            && let Some(seconds) = self.dev_seek.take()
        {
            self.player.seek(0, seconds * f64::from(state.sample_rate));
        }
        let window = ui.max_rect();
        let title = egui::Rect::from_min_size(
            window.min,
            egui::vec2(window.width(), cuepoint_ui::TITLE_BAR),
        );
        cuepoint_ui::title_bar(ui, title, "Cuepoint");
        let full = egui::Rect::from_min_max(egui::pos2(window.left(), title.bottom()), window.max);
        let top =
            egui::Rect::from_min_size(full.min, egui::vec2(full.width(), cuepoint_ui::TOP_BAR));
        let footer = egui::Rect::from_min_max(
            egui::pos2(full.left(), full.bottom() - cuepoint_ui::FOOTER),
            full.max,
        );
        let clock = chrono::Local::now().format("%-I:%M").to_string();
        if let Some(mode) = cuepoint_ui::top_bar(ui, top, self.mode, &clock) {
            self.mode = mode;
        }
        let browser_top = match self.mode {
            Mode::Export => {
                let player = egui::Rect::from_min_size(
                    egui::pos2(full.left(), top.bottom()),
                    egui::vec2(full.width(), cuepoint_ui::PLAYER),
                );
                self.player.show_export(
                    &mut self.export_view,
                    ui,
                    player,
                    &state,
                    &mut self.artwork,
                );
                player.bottom()
            }
            Mode::Performance => {
                let decks = egui::Rect::from_min_size(
                    egui::pos2(full.left(), top.bottom()),
                    egui::vec2(full.width(), 330.0),
                );
                ui.painter()
                    .rect_filled(decks, 0.0, cuepoint_ui::theme::PANEL);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(decks.shrink(6.0)));
                self.player.show_detail_waveforms(&mut child, &state);
                child.add_space(4.0);
                child.columns(3, |columns| {
                    self.player.show_deck(&mut columns[0], 0, &state);
                    self.player.show_mixer(&mut columns[1], &state);
                    self.player.show_deck(&mut columns[2], 1, &state);
                });
                decks.bottom()
            }
        };
        let divider = egui::Rect::from_min_size(
            egui::pos2(full.left(), browser_top),
            egui::vec2(full.width(), cuepoint_ui::DIVIDER),
        );
        ui.painter()
            .rect_filled(divider, 0.0, cuepoint_ui::theme::BLACK);
        let browser = egui::Rect::from_min_max(
            egui::pos2(full.left(), divider.bottom()),
            egui::pos2(full.right(), footer.top()),
        );
        let loaded = self
            .player
            .loaded_id(0)
            .and_then(|id| self.library.track_index(id));
        self.browser.set_two_decks(self.mode == Mode::Performance);
        if let Some(action) = self.browser.show(
            ui,
            browser,
            &self.library,
            loaded,
            &mut self.artwork,
            &mut self.previews,
        ) {
            self.browser_action(action);
        }
        // Space plays and pauses Player A unless text is being typed.
        if self.mode == Mode::Export
            && ui.ctx().memory(|m| m.focused().is_none())
            && ui.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.player.play_pause(0);
        }
        let footer_text = self.footer_text();
        cuepoint_ui::footer(ui, footer, &footer_text);
        let footer_response =
            ui.interact(footer, ui.scope_id().with("footer"), egui::Sense::click());
        footer_response.context_menu(|ui| self.file_menu(ui));
        self.key_window(ui);
        self.export_window(ui);
        // Redraw at frame rate only while something moves or is arriving:
        // playback, loads, an export, artwork and previews, and briefly after
        // input so engine responses show. Otherwise once a second, for the
        // clock.
        let now = ui.input(|i| i.time);
        if ui.input(|i| !i.events.is_empty() || i.pointer.any_down()) {
            self.last_input = now;
        }
        let busy = self.player.is_busy(&state)
            || self.export.is_some()
            || self.dev_seek.is_some()
            || self.artwork.is_pending()
            || self.previews.is_pending()
            || now - self.last_input < 0.5;
        let wait = if busy { 16 } else { 1000 };
        ui.ctx().request_repaint_after(Duration::from_millis(wait));
    }
}

/// `cuepoint export LIBRARY DEVICE_ROOT`: exports the whole collection
/// without opening a window. Returns the process exit code.
fn export_cli(library: &Path, dest: &Path) -> i32 {
    let opened = if is_xml(library) {
        cuepoint_library::xml::open(library)
    } else {
        let Ok(key) = std::env::var(KEY_ENV) else {
            eprintln!("set {KEY_ENV} to open a master.db");
            return 2;
        };
        cuepoint_library::master_db::open(library, &key)
    };
    let library = match opened {
        Ok(library) => library,
        Err(error) => {
            eprintln!("{}: {error}", library.display());
            return 1;
        }
    };
    let job = export::start(&library, &cuepoint_ui::Source::Collection, dest.to_owned());
    for update in job.updates.iter() {
        match update {
            export::Update::Status(text) => eprintln!("{text}"),
            export::Update::Finished(Ok(report)) => {
                println!(
                    "Exported {} tracks and {} playlists to {} ({} already present).",
                    report.tracks,
                    report.playlists,
                    dest.display(),
                    report.already_present
                );
                return 0;
            }
            export::Update::Finished(Err(error)) => {
                eprintln!("{error}");
                return 1;
            }
        }
    }
    1
}

/// Track list previews, made from Cuepoint's own 3-band waveform and kept
/// in the user's cache folder.
fn preview_cache() -> cuepoint_ui::PreviewCache {
    // The waveform works at any rate, so the audio is not resampled.
    let worker = |path: &Path| {
        let pcm = cuepoint_engine::decode_native(path).ok()?;
        let bands = cuepoint_analysis::colour_waveform(pcm.samples(), pcm.sample_rate());
        Some(cuepoint_ui::preview_from_bands(
            &bands.low,
            &bands.mid,
            &bands.high,
        ))
    };
    let threads =
        std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(2).clamp(1, 4));
    cuepoint_ui::PreviewCache::new(
        std::sync::Arc::new(worker),
        cache_dir().map(|d| d.join("previews")),
        threads,
    )
}

/// `~/Library/Caches/Cuepoint` on macOS, `$XDG_CACHE_HOME/cuepoint` (or
/// `~/.cache/cuepoint`) elsewhere.
fn cache_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Caches/Cuepoint"));
    }
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".cache")))
        .map(|d| d.join("cuepoint"))
}

fn is_xml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
}
