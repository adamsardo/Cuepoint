//! Library browser in rekordbox's Export layout: source icon rail, playlist
//! tree, and the track list with its header bar (spec 05-ui, section 2.7).
//! Everything is painted at measured positions; rows are virtualised.

use std::collections::HashSet;

use cuepoint_core::{PlaylistKind, PlaylistNode, Track};
use cuepoint_library::{Library, SortColumn, matches_query, sort_rows};
use egui::{Align, Rect, Sense, Ui, pos2, vec2};

use crate::artwork::ArtworkCache;
use crate::paint;
use crate::preview::{PREVIEW_BINS, PreviewCache};
use crate::theme;

/// Tree and track list rows.
const ROW: f32 = 15.0;
/// Left icon rail and tree panel edges, and the gap before the list.
const RAIL_RIGHT: f32 = 37.0;
const TREE_RIGHT: f32 = 354.0;
const LIST_LEFT: f32 = 359.0;
/// Scrollbar and right rail widths.
const SCROLLBAR: f32 = 12.0;
const RIGHT_RAIL: f32 = 25.0;
/// Header bar, then a 1 pt line, then the column header.
const HEADER_BAR: f32 = 26.0;
const COLUMN_HEADER: f32 = 15.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content {
    Status,
    Number,
    Preview,
    Artwork,
    Text(SortColumn),
}

#[derive(Debug, Clone, Copy)]
struct Column {
    title: &'static str,
    width: f32,
    content: Content,
    right_aligned: bool,
}

/// rekordbox's default Export columns, widths including the 1 pt divider.
const COLUMNS: [Column; 11] = [
    Column {
        title: "",
        width: 29.0,
        content: Content::Status,
        right_aligned: false,
    },
    Column {
        title: "",
        width: 30.0,
        content: Content::Status,
        right_aligned: false,
    },
    Column {
        title: "#",
        width: 60.0,
        content: Content::Number,
        right_aligned: false,
    },
    Column {
        title: "Preview",
        width: 200.0,
        content: Content::Preview,
        right_aligned: false,
    },
    Column {
        title: "Artwork",
        width: 80.0,
        content: Content::Artwork,
        right_aligned: false,
    },
    Column {
        title: "Track Title",
        width: 246.0,
        content: Content::Text(SortColumn::Title),
        right_aligned: false,
    },
    Column {
        title: "Artist",
        width: 128.0,
        content: Content::Text(SortColumn::Artist),
        right_aligned: false,
    },
    Column {
        title: "Genre",
        width: 128.0,
        content: Content::Text(SortColumn::Genre),
        right_aligned: false,
    },
    Column {
        title: "BPM",
        width: 80.0,
        content: Content::Text(SortColumn::Bpm),
        right_aligned: true,
    },
    Column {
        title: "Time",
        width: 80.0,
        content: Content::Text(SortColumn::Duration),
        right_aligned: true,
    },
    Column {
        title: "Key",
        width: 55.0,
        content: Content::Text(SortColumn::Key),
        right_aligned: false,
    },
];

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
    /// Folder ids the user collapsed (everything starts expanded).
    collapsed: HashSet<String>,
    playlists_open: bool,
    /// Track list scroll position, in points.
    scroll: f32,
    tree_scroll: f32,
    /// Row the context menu was opened on.
    context_row: Option<usize>,
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
            collapsed: HashSet::new(),
            playlists_open: true,
            scroll: 0.0,
            tree_scroll: 0.0,
            context_row: None,
        }
    }
}

impl Browser {
    /// Call after replacing the library being shown.
    pub fn reset(&mut self) {
        self.source = Source::Collection;
        self.selected = None;
        self.scroll = 0.0;
        self.generation += 1;
    }

    /// Shows the first playlist named `name`; false if there is none.
    pub fn select_playlist_named(&mut self, library: &Library, name: &str) -> bool {
        fn find<'a>(nodes: &'a [PlaylistNode], name: &str) -> Option<&'a PlaylistNode> {
            nodes.iter().find_map(|n| {
                if n.kind != PlaylistKind::Folder && n.name == name {
                    Some(n)
                } else {
                    find(&n.children, name)
                }
            })
        }
        let Some(node) = find(library.playlists(), name) else {
            return false;
        };
        self.select_source(Source::Playlist(node.id.clone()));
        true
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// "100 Tracks, 8 hours 59 minutes, 1.2 GB" for the rows shown.
    pub fn summary(&self, library: &Library) -> String {
        let tracks = library.tracks();
        let rows = self.rows.iter().map(|&i| &tracks[i]);
        let seconds: u64 = rows
            .clone()
            .filter_map(|t| t.duration)
            .map(|d| d.as_secs())
            .sum();
        let bytes: u64 = rows.filter_map(|t| t.file_size).sum();
        let (hours, minutes) = (seconds / 3600, (seconds / 60) % 60);
        let size = if bytes >= 1 << 30 {
            format!("{:.1} GB", bytes as f64 / f64::from(1u32 << 30))
        } else {
            format!("{:.1} MB", bytes as f64 / f64::from(1u32 << 20))
        };
        let duration = if hours > 0 {
            format!("{hours} hours {minutes} minutes")
        } else {
            format!("{minutes} minutes")
        };
        format!("{} Tracks, {duration}, {size}", self.rows.len())
    }

    /// Draws the browser into `rect`; returns what the user asked to do
    /// with a track. `loaded` marks the track on Player A.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        library: &Library,
        loaded: Option<usize>,
        artwork: &mut ArtworkCache,
        previews: &mut PreviewCache,
    ) -> Option<BrowserAction> {
        self.refresh(library);
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::BLACK);
        let rail = Rect::from_min_max(
            pos2(rect.left() + 2.0, rect.top()),
            pos2(rect.left() + RAIL_RIGHT, rect.bottom()),
        );
        let tree = Rect::from_min_max(
            pos2(rail.right(), rect.top()),
            pos2(rect.left() + TREE_RIGHT, rect.bottom()),
        );
        let right_rail = Rect::from_min_max(pos2(rect.right() - RIGHT_RAIL, rect.top()), rect.max);
        let list = Rect::from_min_max(
            pos2(rect.left() + LIST_LEFT, rect.top()),
            pos2(right_rail.left(), rect.bottom()),
        );
        self.icon_rail(ui, rail);
        self.tree(ui, tree, library);
        self.right_rail(ui, right_rail);
        self.track_list(ui, list, library, loaded, artwork, previews)
    }

    fn icon_rail(&self, ui: &Ui, rect: Rect) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::PANEL);
        let cx = rect.left() + 17.5;
        // Collapse arrow at the top.
        paint::chevron_side(
            &p,
            pos2(rect.left() + 12.5, rect.top() + 10.5),
            9.0,
            true,
            theme::TEXT_OFF,
        );
        // Source icons on a 20 pt pitch from y 33.5; the list is selected.
        for i in 0..15 {
            let c = pos2(cx, rect.top() + 33.5 + i as f32 * 20.0);
            let icon = Rect::from_center_size(c, vec2(16.0, 16.0));
            let selected = i == 1;
            let colour = if (11..=12).contains(&i) {
                theme::TEXT_OFF
            } else {
                theme::TEXT
            };
            if selected {
                paint::fill(
                    &p,
                    Rect::from_min_size(pos2(rect.left() - 2.0, icon.top()), vec2(2.0, 16.0)),
                    theme::ACCENT,
                );
                paint::rounded(&p, icon.expand(1.0), 2.0, theme::ROW_SELECTED);
            }
            paint::outline(&p, icon, 2.0, colour);
            match i {
                0 => {
                    // Bookmark ribbon.
                    paint::fill(
                        &p,
                        Rect::from_min_max(c + vec2(-3.5, -5.5), c + vec2(3.5, 2.0)),
                        colour,
                    );
                    paint::triangle(
                        &p,
                        [
                            c + vec2(-3.5, 2.0),
                            c + vec2(0.0, -0.5),
                            c + vec2(-3.5, 5.5),
                        ],
                        colour,
                    );
                    paint::triangle(
                        &p,
                        [c + vec2(3.5, 2.0), c + vec2(0.0, -0.5), c + vec2(3.5, 5.5)],
                        colour,
                    );
                }
                1 => paint::list_icon(&p, icon, true, colour),
                2 => {
                    paint::line(&p, c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5), 1.5, colour);
                    p.circle_filled(c + vec2(-3.5, 3.5), 2.0, colour);
                    p.circle_filled(c + vec2(3.5, -3.5), 2.0, colour);
                }
                3 => {
                    // Explorer: three crossing orbits.
                    for a in [0.0f32, 1.047, 2.094] {
                        let d = egui::Vec2::angled(a) * 5.5;
                        paint::line(&p, c - d, c + d, 1.2, colour);
                    }
                    p.circle_filled(c, 1.6, colour);
                }
                4..=8 => paint::note(&p, c + vec2(-0.5, 0.0), colour),
                9 => {
                    // A computer: screen and stand.
                    paint::outline(
                        &p,
                        Rect::from_center_size(c + vec2(0.0, -1.0), vec2(10.0, 7.0)),
                        0.0,
                        colour,
                    );
                    paint::fill(
                        &p,
                        Rect::from_center_size(c + vec2(0.0, 4.0), vec2(8.0, 1.2)),
                        colour,
                    );
                }
                10 => paint::outline(
                    &p,
                    Rect::from_center_size(c + vec2(0.0, 1.0), vec2(6.0, 9.0)),
                    1.0,
                    colour,
                ),
                11 | 12 => paint::lock(&p, c, colour),
                13 => paint::arc(&p, c, 4.5, -2.5, 3.6, 1.4, colour),
                14 => {
                    p.circle_stroke(c, 4.5, egui::Stroke::new(1.4, colour));
                    p.circle_filled(c, 2.5, colour);
                }
                _ => {
                    p.circle_filled(c, 3.5, colour);
                }
            }
        }
    }

    fn right_rail(&self, ui: &Ui, rect: Rect) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::PANEL);
        let b = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 3.0), vec2(17.0, 20.0));
        paint::rounded(&p, b, 1.0, theme::BUTTON);
        paint::list_icon(&p, b.shrink2(vec2(1.0, 3.0)), true, theme::TEXT);
    }

    fn tree(&mut self, ui: &mut Ui, rect: Rect, library: &Library) {
        let p = ui.painter_at(rect);
        paint::fill(&p, rect, theme::PANEL);
        let (x, y) = (rect.left(), rect.top());
        // Window toggle at the top.
        let window = Rect::from_min_size(pos2(x + 14.5, y + 2.5), vec2(15.0, 15.0));
        paint::outline(&p, window, 2.0, theme::TEXT);
        paint::fill(
            &p,
            Rect::from_min_size(window.min + vec2(2.0, 2.0), vec2(11.0, 3.0)),
            theme::TEXT,
        );
        // Tree View / Column View tabs, 100 wide, y 22-46.
        let search = Rect::from_min_size(pos2(x + 1.5, y + 23.0), vec2(20.0, 22.0));
        paint::outline(&p, search, 0.0, theme::OUTLINE);
        paint::magnifier(
            &p,
            search.center() + vec2(-1.0, 1.0),
            4.5,
            None,
            theme::TEXT,
        );
        let tree_tab = Rect::from_min_size(pos2(x + 26.0, y + 22.0), vec2(100.0, 24.0));
        let column_tab = Rect::from_min_size(pos2(x + 126.0, y + 22.0), vec2(100.0, 24.0));
        paint::fill(&p, tree_tab, theme::ACCENT);
        paint::fill(&p, column_tab, theme::TAB_OFF);
        paint::text_centred(&p, tree_tab, "Tree View", theme::regular(12.5), theme::TEXT);
        paint::lock(
            &p,
            pos2(column_tab.left() + 11.0, column_tab.center().y),
            theme::TEXT_DIM,
        );
        paint::text(
            &p,
            column_tab.left() + 20.0,
            y + 29.0,
            Align::Min,
            "Column View",
            theme::regular(12.5),
            theme::TEXT_DIM,
        );

        // Rows.
        let body = Rect::from_min_max(pos2(x, y + 46.0), rect.max);
        let mut rows = Vec::new();
        rows.push(TreeRow {
            depth: 0,
            kind: RowKind::AllTracks,
            label: "All Tracks".into(),
            count: Some(library.tracks().len()),
        });
        rows.push(TreeRow {
            depth: 0,
            kind: RowKind::Root,
            label: "Playlists".into(),
            count: None,
        });
        if self.playlists_open {
            for node in library.playlists() {
                self.flatten(node, 1, &mut rows);
            }
        }
        let visible = (body.height() / ROW).ceil() as usize + 1;
        let max_scroll = (rows.len() as f32 * ROW - body.height()).max(0.0);
        let response = ui.interact(body, ui.scope_id().with("tree_body"), Sense::click());
        if response.hovered() {
            self.tree_scroll -= ui.input(|i| i.smooth_scroll_delta.y);
        }
        self.tree_scroll = self.tree_scroll.clamp(0.0, max_scroll);
        let first = (self.tree_scroll / ROW) as usize;
        let bp = ui.painter_at(body);
        let mut clicked = None;
        for (i, row) in rows.iter().enumerate().skip(first).take(visible) {
            let top = body.top() + i as f32 * ROW - self.tree_scroll;
            let row_rect = Rect::from_min_size(pos2(x, top), vec2(rect.width() - 12.0, ROW));
            let selected = match (&row.kind, &self.source) {
                (RowKind::AllTracks, Source::Collection) => true,
                (RowKind::Playlist(id), Source::Playlist(current)) => id == current,
                _ => false,
            };
            if selected {
                paint::fill(&bp, row_rect, theme::ROW_SELECTED);
            }
            let cap = top + 3.0;
            // Each level indents 20 pt; depth 1 is the first level under
            // "Playlists".
            let indent = x + (row.depth.max(1) - 1) as f32 * 20.0;
            match &row.kind {
                RowKind::AllTracks => {
                    paint::note(&bp, pos2(x + 30.0, top + 7.0), theme::TEXT_DIM);
                    paint::text(
                        &bp,
                        x + 43.0,
                        cap,
                        Align::Min,
                        &row.label,
                        theme::body(),
                        theme::TEXT,
                    );
                }
                RowKind::Root => {
                    paint::disclosure(
                        &bp,
                        pos2(x + 14.5, top + 7.5),
                        7.0,
                        self.playlists_open,
                        theme::TEXT,
                    );
                    paint::text(
                        &bp,
                        x + 28.5,
                        cap,
                        Align::Min,
                        &row.label,
                        theme::body(),
                        theme::TEXT,
                    );
                }
                RowKind::Folder(id) => {
                    let open = !self.collapsed.contains(id);
                    paint::disclosure(&bp, pos2(indent + 27.5, top + 7.5), 7.0, open, theme::TEXT);
                    paint::folder(
                        &bp,
                        Rect::from_min_size(pos2(indent + 45.0, top + 3.0), vec2(12.0, 9.0)),
                        theme::TEXT_DIM,
                    );
                    paint::text(
                        &bp,
                        indent + 63.0,
                        cap,
                        Align::Min,
                        &row.label,
                        theme::body(),
                        theme::TEXT,
                    );
                }
                RowKind::Playlist(_) | RowKind::Smart(_) => {
                    paint::playlist(
                        &bp,
                        Rect::from_min_size(pos2(indent + 46.5, top + 2.5), vec2(9.0, 10.0)),
                        theme::TEXT,
                    );
                    paint::text_clipped(
                        &bp,
                        indent + 63.5,
                        cap,
                        x + 239.0 - indent - 63.5,
                        Align::Min,
                        &row.label,
                        theme::body(),
                        theme::TEXT,
                    );
                }
            }
            if let Some(count) = row.count {
                paint::text(
                    &bp,
                    x + 275.0,
                    cap,
                    Align::Max,
                    &count.to_string(),
                    theme::body(),
                    theme::TEXT,
                );
            }
            if selected && matches!(row.kind, RowKind::Playlist(_)) {
                paint::plus_circle(&bp, pos2(x + 294.0, top + 7.5), theme::TEXT_DIM);
            }
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
                && row_rect.contains(pointer)
            {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            match &rows[i].kind {
                RowKind::AllTracks => self.select_source(Source::Collection),
                RowKind::Root => self.playlists_open = !self.playlists_open,
                RowKind::Folder(id) => {
                    if !self.collapsed.remove(id) {
                        self.collapsed.insert(id.clone());
                    }
                }
                RowKind::Playlist(id) | RowKind::Smart(id) => {
                    self.select_source(Source::Playlist(id.clone()))
                }
            }
        }
    }

    fn select_source(&mut self, source: Source) {
        if self.source != source {
            self.source = source;
            self.scroll = 0.0;
        }
    }

    fn flatten(&self, node: &PlaylistNode, depth: usize, rows: &mut Vec<TreeRow>) {
        let kind = match node.kind {
            PlaylistKind::Folder => RowKind::Folder(node.id.clone()),
            PlaylistKind::Playlist => RowKind::Playlist(node.id.clone()),
            PlaylistKind::Smart => RowKind::Smart(node.id.clone()),
        };
        let count = (node.kind != PlaylistKind::Folder).then_some(node.tracks.len());
        rows.push(TreeRow {
            depth,
            kind,
            label: node.name.clone(),
            count,
        });
        if node.kind == PlaylistKind::Folder && !self.collapsed.contains(&node.id) {
            for child in &node.children {
                self.flatten(child, depth + 1, rows);
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

    fn source_title(&self, library: &Library) -> String {
        match &self.source {
            Source::Collection => format!("All Tracks ({} Tracks)", self.rows.len()),
            Source::Playlist(id) => {
                let name = library.find_playlist(id).map_or("", |n| n.name.as_str());
                format!("{name} ({} Tracks)", self.rows.len())
            }
        }
    }

    fn track_list(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        library: &Library,
        loaded: Option<usize>,
        artwork: &mut ArtworkCache,
        previews: &mut PreviewCache,
    ) -> Option<BrowserAction> {
        let p = ui.painter_at(rect);
        let (x, y) = (rect.left(), rect.top());
        let list_right = rect.right() - SCROLLBAR;
        // Header bar: source title, sort chevrons, player, views, search.
        let bar = Rect::from_min_max(rect.min, pos2(rect.right(), y + HEADER_BAR));
        paint::fill(&p, bar, theme::PANEL);
        paint::text(
            &p,
            x + 10.0,
            y + 9.0,
            Align::Min,
            &self.source_title(library),
            theme::body(),
            theme::TEXT,
        );
        paint::chevron_up(&p, pos2(x + 289.5, y + 9.0), 9.0, theme::TEXT_OFF);
        paint::chevron_down(&p, pos2(x + 289.5, y + 17.0), 9.0, theme::TEXT_OFF);
        // Offsets from the list's right edge (the right rail starts there).
        let r = rect.right();
        let player = Rect::from_min_size(pos2(r - 505.0, y + 4.0), vec2(63.0, 19.0));
        let chooser = Rect::from_min_size(pos2(r - 441.0, y + 4.0), vec2(16.0, 19.0));
        paint::rounded(&p, player, 2.0, theme::BUTTON);
        paint::rounded(&p, chooser, 2.0, theme::BUTTON);
        paint::text_centred(&p, player, "PLAYER A", theme::bold(10.3), theme::TEXT);
        paint::chevron_down(&p, chooser.center(), 8.0, theme::TEXT);
        for (i, ix) in [r - 403.5, r - 365.5, r - 306.5, r - 268.5]
            .into_iter()
            .enumerate()
        {
            let icon = Rect::from_min_size(pos2(ix, y + 6.5), vec2(15.0, 15.0));
            let colour = if i == 0 {
                theme::ACCENT
            } else {
                theme::OUTLINE
            };
            paint::outline(&p, icon, 1.5, colour);
            match i {
                0 | 1 => paint::list_icon(&p, icon, i == 1, theme::TEXT),
                2 => paint::columns_icon(&p, icon, 3, theme::TEXT),
                _ => {
                    paint::fill(
                        &p,
                        Rect::from_min_size(icon.min + vec2(3.0, 3.0), vec2(3.0, 9.0)),
                        theme::TEXT,
                    );
                    paint::fill(
                        &p,
                        Rect::from_min_size(icon.min + vec2(7.0, 3.0), vec2(5.0, 9.0)),
                        theme::TEXT,
                    );
                }
            }
        }
        let search = Rect::from_min_max(pos2(r - 232.0, y + 3.0), pos2(r - 20.0, y + 24.0));
        paint::fill(&p, search, theme::BLACK);
        paint::magnifier(
            &p,
            pos2(search.left() + 11.0, search.center().y - 0.5),
            4.5,
            None,
            theme::TEXT_OFF,
        );
        paint::triangle(
            &p,
            [
                pos2(search.left() + 21.0, y + 12.0),
                pos2(search.left() + 26.0, y + 12.0),
                pos2(search.left() + 23.5, y + 15.0),
            ],
            theme::TEXT_OFF,
        );
        let field = Rect::from_min_max(
            pos2(search.left() + 36.0, search.top() + 2.0),
            pos2(search.right() - 4.0, search.bottom() - 2.0),
        );
        let edit = ui.put(
            field,
            egui::TextEdit::singleline(&mut self.query)
                .frame(egui::Frame::NONE)
                .font(theme::regular(12.0))
                .text_color(theme::TEXT)
                .margin(egui::Margin::ZERO),
        );
        if self.query.is_empty() && !edit.has_focus() {
            paint::text(
                &p,
                field.left() + 2.0,
                y + 7.5,
                Align::Min,
                "Search within this track list",
                theme::regular(12.0),
                theme::TEXT_HINT,
            );
        }

        // Column header.
        let header = Rect::from_min_max(
            pos2(x, bar.bottom() + 1.0),
            pos2(list_right, bar.bottom() + 1.0 + COLUMN_HEADER - 1.0),
        );
        paint::fill(
            &p,
            Rect::from_min_max(pos2(x, bar.bottom()), pos2(rect.right(), header.bottom())),
            theme::BLACK,
        );
        paint::fill(&p, header, theme::PANEL);
        let mut cx = x;
        let mut clicked_column = None;
        for (i, column) in COLUMNS.iter().enumerate() {
            let last = i == COLUMNS.len() - 1;
            let width = if last { list_right - cx } else { column.width };
            let cell =
                Rect::from_min_size(pos2(cx, header.top()), vec2(width - 1.0, header.height()));
            paint::fill(
                &p,
                Rect::from_min_size(
                    pos2(cx + width - 1.0, header.top()),
                    vec2(1.0, header.height()),
                ),
                theme::BLACK,
            );
            let sorted = match (column.content, self.sort) {
                (Content::Number, None) => Some(true),
                (Content::Text(c), Some((s, asc))) if c == s => Some(asc),
                _ => None,
            };
            let colour = if sorted.is_some() {
                theme::ACCENT
            } else {
                theme::TEXT
            };
            if i == 0 {
                paint::funnel(&p, pos2(cell.center().x, cell.center().y), theme::TEXT_DIM);
            } else {
                paint::text(
                    &p,
                    cx + 7.0,
                    header.top() + 2.0,
                    Align::Min,
                    column.title,
                    theme::body(),
                    colour,
                );
            }
            if let Some(ascending) = sorted {
                paint::sort_arrow(
                    &p,
                    pos2(cx + width - 11.5, cell.center().y),
                    ascending,
                    theme::ACCENT,
                );
            }
            if matches!(column.content, Content::Text(_) | Content::Number)
                && ui
                    .interact(cell, ui.scope_id().with(("column", i)), Sense::click())
                    .clicked()
            {
                clicked_column = Some(column.content);
            }
            cx += width;
        }
        if let Some(content) = clicked_column {
            self.sort = match (content, self.sort) {
                (Content::Number, _) => None,
                (Content::Text(c), Some((s, true))) if c == s => Some((c, false)),
                (Content::Text(c), _) => Some((c, true)),
                _ => self.sort,
            };
        }

        // Rows.
        let body = Rect::from_min_max(
            pos2(x, header.bottom()),
            pos2(list_right, rect.bottom() - 12.0),
        );
        let response = ui.interact(body, ui.scope_id().with("track_rows"), Sense::click());
        let max_scroll = (self.rows.len() as f32 * ROW - body.height()).max(0.0);
        if response.hovered() {
            self.scroll -= ui.input(|i| i.smooth_scroll_delta.y);
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let bp = ui.painter_at(body);
        // Stripes continue below the last row.
        let first = (self.scroll / ROW) as usize;
        let visible = (body.height() / ROW).ceil() as usize + 1;
        let tracks = library.tracks();
        let row_at = |pos: egui::Pos2| -> Option<usize> {
            let index = ((pos.y - body.top() + self.scroll) / ROW) as usize;
            (index < self.rows.len()).then_some(index)
        };
        for n in first..first + visible {
            let top = body.top() + n as f32 * ROW - self.scroll;
            let row_rect = Rect::from_min_size(pos2(x, top), vec2(list_right - x, ROW));
            let stripe = if n % 2 == 0 {
                theme::ROW_EVEN
            } else {
                theme::ROW_ODD
            };
            let Some(&track_index) = self.rows.get(n) else {
                paint::fill(&bp, row_rect, stripe);
                continue;
            };
            let selected = self.selected == Some(track_index);
            paint::fill(
                &bp,
                row_rect,
                if selected {
                    theme::ROW_SELECTED
                } else {
                    stripe
                },
            );
            let track = &tracks[track_index];
            let mut cx = x;
            for (i, column) in COLUMNS.iter().enumerate() {
                let last = i == COLUMNS.len() - 1;
                let width = if last { list_right - cx } else { column.width };
                if !last {
                    paint::fill(
                        &bp,
                        Rect::from_min_size(pos2(cx + width - 1.0, top), vec2(1.0, ROW)),
                        theme::BLACK,
                    );
                }
                let cap = top + 3.0;
                let cell_clip = Rect::from_min_size(pos2(cx, top), vec2(width - 1.0, ROW));
                let cp = bp.with_clip_rect(bp.clip_rect().intersect(cell_clip));
                match column.content {
                    Content::Status if i == 1 => {
                        status_cell(&cp, cx, top, loaded == Some(track_index))
                    }
                    Content::Preview => {
                        // The preview cell is black on dark rows.
                        if n % 2 == 0 && !selected {
                            paint::fill(&cp, cell_clip, theme::BLACK);
                        }
                        if let Some(path) = track.location.as_deref()
                            && let Some(preview) = previews.get(path)
                        {
                            draw_preview(&cp, cx + 30.0, cx + width - 5.0, top, preview, track);
                        }
                    }
                    Content::Status | Content::Artwork => {}
                    Content::Number => {
                        paint::text(
                            &cp,
                            cx + 6.0,
                            cap,
                            Align::Min,
                            &(n + 1).to_string(),
                            theme::body(),
                            theme::TEXT,
                        );
                    }
                    Content::Text(sort) => {
                        let text = cell_text(track, sort);
                        let (tx, align) = if column.right_aligned {
                            (cx + width - 6.5, Align::Max)
                        } else {
                            (cx + 5.5, Align::Min)
                        };
                        paint::text(&cp, tx, cap, align, &text, theme::body(), theme::TEXT);
                    }
                }
                cx += width;
            }
        }
        // Artwork: each cover is drawn at full column width from its row's
        // top and runs down into the rows below until the next cover, as
        // rekordbox draws it. Start a few rows up so their overflow shows.
        let art_left = x + COLUMNS[..4].iter().map(|c| c.width).sum::<f32>();
        let art_width = COLUMNS[4].width - 1.0;
        let art_clip = Rect::from_x_y_ranges(art_left..=art_left + art_width, body.y_range());
        let ap = bp.with_clip_rect(bp.clip_rect().intersect(art_clip));
        for n in first.saturating_sub(5)..(first + visible).min(self.rows.len()) {
            let top = body.top() + n as f32 * ROW - self.scroll;
            let Some(path) = tracks[self.rows[n]].location.as_deref() else {
                continue;
            };
            let cover = Rect::from_min_size(pos2(art_left, top), vec2(art_width, art_width));
            if let Some(texture) = artwork.get(path) {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                ap.image(texture.id(), cover, uv, egui::Color32::WHITE);
            } else if artwork.is_missing(path) {
                paint::fill(&ap, cover, theme::BUTTON_DARK);
                ap.circle_filled(
                    cover.center() + vec2(0.0, 6.0),
                    art_width * 0.46,
                    theme::ROW_ODD,
                );
                ap.circle_filled(cover.center() + vec2(0.0, 6.0), 4.0, theme::BUTTON_DARK);
            }
        }
        if response.clicked()
            && let Some(row) = response.interact_pointer_pos().and_then(row_at)
        {
            self.selected = Some(self.rows[row]);
        }
        let mut action = None;
        if response.double_clicked()
            && let Some(row) = response.interact_pointer_pos().and_then(row_at)
        {
            action = Some(BrowserAction::Load {
                track: self.rows[row],
                deck: Some(0),
            });
        }
        if response.secondary_clicked() {
            self.context_row = response
                .interact_pointer_pos()
                .and_then(row_at)
                .map(|r| self.rows[r]);
            if let Some(track) = self.context_row {
                self.selected = Some(track);
            }
        }
        if let Some(track_index) = self.context_row {
            response.context_menu(|ui| {
                for deck in 0..2 {
                    let label = if deck == 0 {
                        "Load to Player A (Deck 1)".to_owned()
                    } else {
                        format!("Load to Deck {}", deck + 1)
                    };
                    if ui.button(label).clicked() {
                        action = Some(BrowserAction::Load {
                            track: track_index,
                            deck: Some(deck),
                        });
                        ui.close();
                    }
                }
            });
        }

        // Scrollbar: a narrow rounded thumb in a black track.
        let track_rect = Rect::from_min_max(
            pos2(list_right, header.bottom()),
            pos2(rect.right(), rect.bottom() - 12.0),
        );
        paint::fill(&p, track_rect, theme::BLACK);
        let content = self.rows.len() as f32 * ROW;
        if content > body.height() {
            let length = (body.height() / content * track_rect.height()).max(20.0);
            let top = track_rect.top() + self.scroll / max_scroll * (track_rect.height() - length);
            let thumb = Rect::from_min_size(pos2(track_rect.left() + 4.0, top), vec2(4.5, length));
            paint::rounded(&p, thumb, 2.25, theme::TAB_OFF);
        }
        // Horizontal scrollbar strip at the bottom.
        let bottom = Rect::from_min_max(pos2(x, rect.bottom() - 12.0), rect.max);
        paint::fill(&p, bottom, theme::BLACK);
        paint::rounded(
            &p,
            Rect::from_min_max(
                pos2(x + 1.0, bottom.top() + 2.0),
                pos2(list_right - 1.0, bottom.top() + 8.0),
            ),
            3.0,
            theme::TAB_OFF,
        );
        action
    }
}

/// The analysis status icon (red, star, green and blue marks) followed by
/// the cue indicator, or a play mark on the loaded track.
fn status_cell(p: &egui::Painter, x: f32, top: f32, loaded: bool) {
    paint::fill(
        p,
        Rect::from_min_size(pos2(x + 0.5, top + 4.5), vec2(2.0, 2.0)),
        theme::STATUS_RED,
    );
    paint::fill(
        p,
        Rect::from_min_size(pos2(x + 0.5, top + 11.0), vec2(15.5, 1.5)),
        theme::STATUS_BLUE,
    );
    paint::fill(
        p,
        Rect::from_min_size(pos2(x + 0.5, top + 12.5), vec2(15.5, 1.0)),
        theme::STATUS_CYAN,
    );
    paint::star(p, pos2(x + 7.5, top + 7.0), 5.4, theme::BAR);
    paint::star(p, pos2(x + 7.5, top + 7.0), 4.4, theme::TEXT);
    paint::line(
        p,
        pos2(x + 11.5, top + 4.5),
        pos2(x + 14.0, top + 4.5),
        1.0,
        theme::STATUS_GREEN,
    );
    if loaded {
        paint::play_triangle(p, pos2(x + 20.0, top + 7.5), 8.0, true, theme::TEXT_DIM);
    } else {
        paint::line(
            p,
            pos2(x + 15.75, top + 2.0),
            pos2(x + 15.75, top + 9.0),
            1.0,
            theme::STATUS_GREY,
        );
        paint::text(
            p,
            x + 13.5,
            top + 4.0,
            Align::Min,
            "CUE",
            theme::regular(12.0),
            theme::STATUS_GREY,
        );
    }
}

/// A row's preview: the 3-band overview in miniature, stacked from a
/// baseline near the row's bottom, with its hot cues as small badges.
fn draw_preview(
    p: &egui::Painter,
    left: f32,
    right: f32,
    top: f32,
    preview: &[[u8; 3]],
    track: &Track,
) {
    let base = top + 13.0;
    let width = right - left;
    let bin_width = width / PREVIEW_BINS as f32;
    for (i, &[l, m, h]) in preview.iter().enumerate() {
        let x0 = paint::snap(left + i as f32 * bin_width);
        let x1 = paint::snap(left + (i + 1) as f32 * bin_width).max(x0 + 0.5);
        let low = (0.03 * f32::from(l)).max(0.5);
        let mid = low + 0.013 * f32::from(m) + 0.4;
        let high = mid + 0.035 * f32::from(h) + 0.3;
        for (height, colour) in [
            (high, theme::wave::HIGH),
            (mid, theme::wave::OVERVIEW_MID),
            (low, theme::wave::OVERVIEW_LOW),
        ] {
            let height = paint::snap(height.min(11.0));
            paint::fill(
                p,
                Rect::from_min_max(pos2(x0, base - height), pos2(x1, base)),
                colour,
            );
        }
    }
    let Some(duration) = track.duration.map(|d| d.as_secs_f64()).filter(|&d| d > 0.0) else {
        return;
    };
    for cue in &track.cues {
        if let Some(letter) = cue.slot_letter() {
            let x = left + (cue.start / duration) as f32 * width;
            let badge = Rect::from_min_size(pos2(x - 3.5, top + 1.5), vec2(7.5, 7.5));
            paint::fill(p, badge, theme::cue_colour(cue));
            paint::text_centred(
                p,
                badge,
                &letter.to_string(),
                theme::bold(7.0),
                theme::CUE_LETTER,
            );
        }
    }
}

struct TreeRow {
    depth: usize,
    kind: RowKind,
    label: String,
    count: Option<usize>,
}

enum RowKind {
    AllTracks,
    Root,
    Folder(String),
    Playlist(String),
    Smart(String),
}

fn cell_text(track: &Track, column: SortColumn) -> String {
    match column {
        SortColumn::Title => track.title.clone(),
        SortColumn::Artist => track.artist.clone(),
        SortColumn::Album => track.album.clone(),
        SortColumn::Genre => track.genre.clone(),
        SortColumn::Bpm => track
            .bpm
            .map(|b| format!("{:.2}", b.as_f64()))
            .unwrap_or_default(),
        // rekordbox's default key display is Camelot-style ("8A").
        SortColumn::Key => cuepoint_core::MusicalKey::parse(&track.key)
            .map_or_else(|| track.key.clone(), |k| k.camelot().to_string()),
        SortColumn::Duration => track
            .duration
            .map(|d| format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60))
            .unwrap_or_default(),
        SortColumn::Rating => "★".repeat(usize::from(track.rating.stars())),
        SortColumn::DateAdded => track.date_added.clone(),
    }
}
