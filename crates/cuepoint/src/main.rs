//! Cuepoint desktop app.
//!
//! Usage: `cuepoint [LIBRARY]`, where LIBRARY is a rekordbox XML export or a
//! rekordbox 6/7 `master.db`. The master.db key is read from
//! `CUEPOINT_MASTERDB_KEY`, or asked for in the app.

mod export;
mod player;

use std::path::{Path, PathBuf};
use std::time::Duration;

use cuepoint_library::Library;
use cuepoint_ui::{Browser, BrowserAction};
use player::Player;

const KEY_ENV: &str = "CUEPOINT_MASTERDB_KEY";

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cuepoint")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Cuepoint",
        options,
        Box::new(move |_cc| {
            let mut app = CuepointApp::new();
            if let Some(path) = path {
                app.open(path, std::env::var(KEY_ENV).ok());
            }
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
}

struct KeyPrompt {
    path: PathBuf,
    key: String,
}

impl CuepointApp {
    fn new() -> Self {
        Self {
            player: Player::new(),
            library: Library::default(),
            loaded_from: None,
            browser: Browser::default(),
            status: Status::default(),
            key_prompt: None,
            export: None,
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
                let Some(deck) = deck.or_else(|| self.player.free_deck()) else {
                    self.status = Status::Error("Both decks are playing.".into());
                    return;
                };
                if let Err(message) = self.player.load(deck, track) {
                    self.status = Status::Error(message);
                }
            }
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

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
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
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let output = self.player.output_description();
            if self.player.output_is_silent() {
                ui.colored_label(ui.visuals().warn_fg_color, output);
            } else {
                ui.weak(format!("Audio: {output}"));
            }
            ui.separator();
            self.library_status(ui);
        });
    }

    fn library_status(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.status {
            Status::Empty => {
                ui.weak("No library open. File > Open rekordbox XML or master.db.");
            }
            Status::Loaded => {
                let from = self.loaded_from.as_deref().map(Path::display);
                ui.label(format!(
                    "{} tracks, showing {} · {}",
                    self.library.tracks().len(),
                    self.browser.row_count(),
                    from.map(|p| p.to_string()).unwrap_or_default()
                ));
            }
            Status::Info(message) => {
                ui.label(message);
            }
            Status::Error(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
        });
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
        self.player.poll_loads();
        self.poll_export();
        let state = self.player.state();
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::top("decks").show(ui, |ui| {
            self.player.show_detail_waveforms(ui, &state);
            ui.add_space(4.0);
            ui.columns(3, |columns| {
                self.player.show_deck(&mut columns[0], 0, &state);
                self.player.show_mixer(&mut columns[1], &state);
                self.player.show_deck(&mut columns[2], 1, &state);
            });
        });
        let action = egui::CentralPanel::default()
            .show(ui, |ui| self.browser.show(ui, &self.library))
            .inner;
        if let Some(action) = action {
            self.browser_action(action);
        }
        self.key_window(ui);
        self.export_window(ui);
        // Keep playheads and meters moving.
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn is_xml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
}
