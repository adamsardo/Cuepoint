//! Library browser: playlist tree on the left, searchable and sortable
//! track table on the right (spec 05-ui, section 2.7).

use cuepoint_core::{PlaylistKind, PlaylistNode};
use cuepoint_library::{Library, SortColumn, matches_query, sort_rows};
use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

/// What the track table lists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Collection,
    Playlist(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAction {
    /// Load `Library::tracks()[track]` to a deck; `None` lets the app pick.
    Load { track: usize, deck: Option<usize> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewKey {
    source: Source,
    query: String,
    sort: Option<(SortColumn, bool)>,
    generation: u64,
}

pub struct Browser {
    source: Source,
    query: String,
    /// `None` keeps the source order (playlist order or collection order).
    sort: Option<(SortColumn, bool)>,
    /// Bumped whenever a new library is loaded, to invalidate the view.
    generation: u64,
    cached_key: Option<ViewKey>,
    /// Indices into `Library::tracks`, filtered and sorted.
    rows: Vec<usize>,
    /// Highlighted track, as an index into `Library::tracks`.
    selected: Option<usize>,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            source: Source::Collection,
            query: String::new(),
            sort: None,
            generation: 0,
            cached_key: None,
            rows: Vec::new(),
            selected: None,
        }
    }
}

impl Browser {
    /// Call after replacing the library being shown.
    pub fn reset(&mut self) {
        self.source = Source::Collection;
        self.selected = None;
        self.generation += 1;
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Draws the browser; returns what the user asked to do with a track.
    pub fn show(&mut self, ui: &mut Ui, library: &Library) -> Option<BrowserAction> {
        let mut action = None;
        egui::Panel::left("browser_tree")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.tree(ui, library));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("title, artist, album, key…"),
                );
                if !self.query.is_empty() && ui.small_button("✕").clicked() {
                    self.query.clear();
                }
            });
            ui.separator();
            self.refresh(library);
            action = self.table(ui, library);
        });
        action
    }

    fn tree(&mut self, ui: &mut Ui, library: &Library) {
        let label = format!("Collection ({})", library.tracks().len());
        if ui
            .selectable_label(self.source == Source::Collection, label)
            .clicked()
        {
            self.source = Source::Collection;
        }
        ui.separator();
        ui.label(RichText::new("Playlists").strong());
        for node in library.playlists() {
            self.tree_node(ui, node);
        }
    }

    fn tree_node(&mut self, ui: &mut Ui, node: &PlaylistNode) {
        match node.kind {
            PlaylistKind::Folder => {
                egui::CollapsingHeader::new(format!("📁 {}", node.name))
                    .id_salt(("playlist", &node.id))
                    .show(ui, |ui| {
                        for child in &node.children {
                            self.tree_node(ui, child);
                        }
                    });
            }
            PlaylistKind::Playlist | PlaylistKind::Smart => {
                let icon = if node.kind == PlaylistKind::Smart {
                    "⚙"
                } else {
                    "♫"
                };
                let selected = self.source == Source::Playlist(node.id.clone());
                let response = ui.selectable_label(selected, format!("{icon} {}", node.name));
                let response = if node.kind == PlaylistKind::Smart {
                    response.on_hover_text("Smart playlist: rule evaluation is not implemented yet")
                } else {
                    response
                };
                if response.clicked() {
                    self.source = Source::Playlist(node.id.clone());
                }
            }
        }
    }

    fn refresh(&mut self, library: &Library) {
        let key = ViewKey {
            source: self.source.clone(),
            query: self.query.clone(),
            sort: self.sort,
            generation: self.generation,
        };
        if self.cached_key.as_ref() == Some(&key) {
            return;
        }
        let mut rows: Vec<usize> = match &self.source {
            Source::Collection => (0..library.tracks().len()).collect(),
            Source::Playlist(id) => match library.find_playlist(id) {
                Some(node) => node
                    .tracks
                    .iter()
                    .filter_map(|id| library.track_index(id))
                    .collect(),
                None => Vec::new(),
            },
        };
        let tracks = library.tracks();
        rows.retain(|&index| matches_query(&tracks[index], &self.query));
        if let Some((column, ascending)) = self.sort {
            sort_rows(&mut rows, tracks, column, ascending);
        }
        self.rows = rows;
        self.cached_key = Some(key);
    }

    fn table(&mut self, ui: &mut Ui, library: &Library) -> Option<BrowserAction> {
        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let mut clicked_column = None;
        let mut action = None;
        // Selectable text would swallow the row's clicks and double-clicks.
        ui.style_mut().interaction.selectable_labels = false;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .column(Column::auto().at_least(36.0))
            .column(Column::initial(240.0).at_least(80.0).clip(true))
            .column(Column::initial(160.0).at_least(60.0).clip(true))
            .column(Column::initial(140.0).at_least(60.0).clip(true))
            .column(Column::initial(90.0).at_least(50.0).clip(true))
            .column(Column::auto().at_least(56.0))
            .column(Column::auto().at_least(40.0))
            .column(Column::auto().at_least(48.0))
            .column(Column::auto().at_least(64.0))
            .column(Column::remainder().at_least(104.0))
            .header(row_height, |mut header| {
                header.col(|ui| {
                    ui.strong("#");
                });
                for column in SortColumn::ALL {
                    header.col(|ui| {
                        let arrow = match self.sort {
                            Some((c, true)) if c == column => " ▲",
                            Some((c, false)) if c == column => " ▼",
                            _ => "",
                        };
                        if ui
                            .button(RichText::new(format!("{}{arrow}", column.label())).strong())
                            .clicked()
                        {
                            clicked_column = Some(column);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, self.rows.len(), |mut row| {
                    let number = row.index() + 1;
                    let track_index = self.rows[row.index()];
                    let track = &library.tracks()[track_index];
                    row.set_selected(self.selected == Some(track_index));
                    row.col(|ui| {
                        ui.weak(number.to_string());
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            // An invisible dot keeps uncoloured titles aligned.
                            let colour = track.colour.as_ref().and_then(|c| c.rgb).map_or(
                                Color32::TRANSPARENT,
                                |rgb| {
                                    Color32::from_rgb(
                                        (rgb >> 16) as u8,
                                        (rgb >> 8) as u8,
                                        rgb as u8,
                                    )
                                },
                            );
                            ui.label(RichText::new("●").color(colour));
                            ui.label(&track.title);
                        });
                    });
                    row.col(|ui| {
                        ui.label(&track.artist);
                    });
                    row.col(|ui| {
                        ui.label(&track.album);
                    });
                    row.col(|ui| {
                        ui.label(&track.genre);
                    });
                    row.col(|ui| {
                        ui.monospace(track.bpm.map(|b| b.to_string()).unwrap_or_default());
                    });
                    row.col(|ui| {
                        ui.label(&track.key);
                    });
                    row.col(|ui| {
                        let time = track
                            .duration
                            .map(|d| format!("{}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
                            .unwrap_or_default();
                        ui.monospace(time);
                    });
                    row.col(|ui| {
                        ui.label("★".repeat(usize::from(track.rating.stars())));
                    });
                    row.col(|ui| {
                        ui.label(&track.date_added);
                    });
                    let response = row.response();
                    if response.clicked() {
                        self.selected = Some(track_index);
                    }
                    if response.double_clicked() {
                        action = Some(BrowserAction::Load {
                            track: track_index,
                            deck: None,
                        });
                    }
                    response.context_menu(|ui| {
                        for deck in 0..2 {
                            if ui.button(format!("Load to Deck {}", deck + 1)).clicked() {
                                action = Some(BrowserAction::Load {
                                    track: track_index,
                                    deck: Some(deck),
                                });
                                ui.close();
                            }
                        }
                    });
                });
            });
        if let Some(column) = clicked_column {
            // Click cycles ascending, descending, then back to source order.
            self.sort = match self.sort {
                Some((c, true)) if c == column => Some((column, false)),
                Some((c, false)) if c == column => None,
                _ => Some((column, true)),
            };
        }
        action
    }
}
