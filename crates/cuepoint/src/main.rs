//! Cuepoint desktop app.
//!
//! Usage: `cuepoint [LIBRARY]`, where LIBRARY is a rekordbox XML export or a
//! rekordbox 6/7 `master.db`. The master.db key is read from
//! `CUEPOINT_MASTERDB_KEY`, or asked for in the app.

use std::path::{Path, PathBuf};

use cuepoint_library::Library;
use cuepoint_ui::Browser;

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
            let mut app = CuepointApp::default();
            if let Some(path) = path {
                app.open(path, std::env::var(KEY_ENV).ok());
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Default)]
struct CuepointApp {
    library: Library,
    /// Where the current library came from.
    loaded_from: Option<PathBuf>,
    browser: Browser,
    status: Status,
    key_prompt: Option<KeyPrompt>,
}

#[derive(Default)]
enum Status {
    #[default]
    Empty,
    Loaded,
    Error(String),
}

struct KeyPrompt {
    path: PathBuf,
    key: String,
}

impl CuepointApp {
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
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
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
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.browser.show(ui, &self.library));
        self.key_window(ui);
    }
}

fn is_xml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
}
