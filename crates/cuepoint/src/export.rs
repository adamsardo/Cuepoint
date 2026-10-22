//! Background export of the collection or a playlist to a USB/SD device.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use cuepoint_analysis::AnalysisSettings;
use cuepoint_core::{MusicalKey, PlaylistKind, PlaylistNode, Track, TrackId};
use cuepoint_export::device::{ExportReport, PreparedTrack, Progress, write_device};
use cuepoint_export::{ExportPlaylist, ExportTrack, FileType};
use cuepoint_library::Library;
use cuepoint_ui::Source;

/// Rate tracks are decoded at for analysis.
const ANALYSIS_RATE: u32 = 44_100;

pub enum Update {
    Status(String),
    Finished(Result<ExportReport, String>),
}

pub struct ExportJob {
    pub updates: Receiver<Update>,
    cancel: Arc<AtomicBool>,
}

impl ExportJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What to export: the tracks in order, and the playlist tree whose
/// entries refer to those tracks by 1-based position.
fn select(library: &Library, source: &Source) -> (Vec<Track>, Vec<ExportPlaylist>) {
    match source {
        Source::Collection => {
            let tracks = library.tracks().to_vec();
            let positions: HashMap<&TrackId, u32> = tracks
                .iter()
                .enumerate()
                .map(|(i, t)| (&t.id, i as u32 + 1))
                .collect();
            let mut playlists = Vec::new();
            flatten(library.playlists(), 0, &positions, &mut playlists);
            (tracks, playlists)
        }
        Source::Playlist(id) => {
            let Some(node) = library.find_playlist(id) else {
                return (Vec::new(), Vec::new());
            };
            let tracks: Vec<Track> = library.playlist_tracks(node).cloned().collect();
            let playlist = ExportPlaylist {
                id: 1,
                parent_id: 0,
                name: node.name.clone(),
                is_folder: false,
                sort_order: 0,
                entries: (1..=tracks.len() as u32).collect(),
            };
            (tracks, vec![playlist])
        }
    }
}

/// Depth-first copy of the tree. Smart playlists export empty until rule
/// evaluation exists.
fn flatten(
    nodes: &[PlaylistNode],
    parent: u32,
    positions: &HashMap<&TrackId, u32>,
    out: &mut Vec<ExportPlaylist>,
) {
    for (order, node) in nodes.iter().enumerate() {
        let id = out.len() as u32 + 1;
        out.push(ExportPlaylist {
            id,
            parent_id: parent,
            name: node.name.clone(),
            is_folder: node.kind == PlaylistKind::Folder,
            sort_order: order as u32,
            entries: node
                .tracks
                .iter()
                .filter_map(|t| positions.get(t).copied())
                .collect(),
        });
        flatten(&node.children, id, positions, out);
    }
}

/// rekordbox colour label index for a track colour, from its name or the
/// RGB values rekordbox XML uses.
fn colour_index(track: &Track) -> u8 {
    let Some(colour) = &track.colour else {
        return 0;
    };
    const LABELS: [(&str, u32); 8] = [
        ("pink", 0xFF007F),
        ("red", 0xFF0000),
        ("orange", 0xFFA500),
        ("yellow", 0xFFFF00),
        ("green", 0x00FF00),
        ("aqua", 0x25FDE9),
        ("blue", 0x0000FF),
        ("purple", 0x660099),
    ];
    LABELS
        .iter()
        .position(|(name, rgb)| colour.name.eq_ignore_ascii_case(name) || colour.rgb == Some(*rgb))
        .map_or(0, |i| i as u8 + 1)
}

fn prepare(track: &Track) -> Result<PreparedTrack, String> {
    let source = track.location.clone().ok_or("no file location")?;
    let pcm = cuepoint_engine::decode_file(&source, ANALYSIS_RATE).map_err(|e| e.to_string())?;
    let analysis =
        cuepoint_analysis::analyze(pcm.samples(), ANALYSIS_RATE, &AnalysisSettings::default());
    let grid = if track.beatgrid.is_empty() {
        analysis.grid
    } else {
        track.beatgrid.clone()
    };
    let bpm = grid
        .markers()
        .first()
        .map(|m| m.bpm)
        .or(track.bpm.map(|b| b.as_f64()));
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let row = ExportTrack {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        genre: track.genre.clone(),
        label: track.label.clone(),
        remixer: track.remixer.clone(),
        comment: track.comment.clone(),
        key: MusicalKey::parse(&track.key).or(analysis.key),
        bpm_centi: bpm.map_or(0, |b| (b * 100.0).round() as u32),
        duration_secs: pcm.duration_secs().round() as u32,
        year: track.year.unwrap_or(0),
        rating: track.rating.stars(),
        colour: colour_index(track),
        sample_rate: track.sample_rate.unwrap_or(ANALYSIS_RATE),
        bit_rate_kbps: track.bit_rate.unwrap_or(0),
        bit_depth: 16,
        file_type: FileType::from_extension(extension),
        date_added: track.date_added.clone(),
        ..ExportTrack::default()
    };
    Ok(PreparedTrack {
        row,
        source,
        grid,
        waveform: analysis.waveform,
        duration_secs: pcm.duration_secs(),
    })
}

/// Starts exporting `source` from `library` to the device mounted at `root`.
pub fn start(library: &Library, source: &Source, root: PathBuf) -> ExportJob {
    let (tracks, playlists) = select(library, source);
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelled = cancel.clone();
    std::thread::Builder::new()
        .name("cuepoint-export".into())
        .spawn(move || {
            let total = tracks.len();
            let mut prepared = Vec::with_capacity(total);
            let mut kept = Vec::with_capacity(total);
            let mut skipped = Vec::new();
            for (index, track) in tracks.iter().enumerate() {
                if cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(Update::Finished(Err("Export cancelled.".into())));
                    return;
                }
                let _ = tx.send(Update::Status(format!(
                    "Analysing {} of {total}: {}",
                    index + 1,
                    track.title
                )));
                match prepare(track) {
                    Ok(p) => {
                        kept.push(index as u32 + 1);
                        prepared.push(p);
                    }
                    Err(e) => skipped.push(format!("{}: {e}", track.title)),
                }
            }
            // Re-point playlist entries at the tracks that made it.
            let renumber: HashMap<u32, u32> = kept
                .iter()
                .enumerate()
                .map(|(new, &old)| (old, new as u32 + 1))
                .collect();
            let playlists = playlists
                .into_iter()
                .map(|mut p| {
                    p.entries = p
                        .entries
                        .iter()
                        .filter_map(|e| renumber.get(e).copied())
                        .collect();
                    p
                })
                .collect();
            let mut on_progress = |progress: Progress| {
                let text = match progress {
                    Progress::Track { done, total } => format!("Copying {} of {total}", done + 1),
                    Progress::WritingLibrary => "Writing export.pdb".into(),
                };
                let _ = tx.send(Update::Status(text));
                !cancelled.load(Ordering::Relaxed)
            };
            let result = write_device(&root, prepared, playlists, &mut on_progress)
                .map_err(|e| e.to_string())
                .and_then(|report| {
                    if skipped.is_empty() {
                        Ok(report)
                    } else {
                        Err(format!(
                            "Exported {} tracks; skipped {}: {}",
                            report.tracks,
                            skipped.len(),
                            skipped.join("; ")
                        ))
                    }
                });
            let _ = tx.send(Update::Finished(result));
        })
        .expect("spawn export thread");
    ExportJob {
        updates: rx,
        cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuepoint_core::TrackColour;

    #[test]
    fn colour_labels_map_by_name_or_rgb() {
        let mut track = Track::default();
        assert_eq!(colour_index(&track), 0);
        track.colour = Some(TrackColour {
            name: "Aqua".into(),
            rgb: None,
        });
        assert_eq!(colour_index(&track), 6);
        track.colour = Some(TrackColour {
            name: "#660099".into(),
            rgb: Some(0x660099),
        });
        assert_eq!(colour_index(&track), 8);
    }

    #[test]
    fn collection_export_keeps_the_playlist_tree() {
        let tracks = vec![
            Track {
                id: TrackId("a".into()),
                ..Track::default()
            },
            Track {
                id: TrackId("b".into()),
                ..Track::default()
            },
        ];
        let mut folder = PlaylistNode::new("1", "Gigs", PlaylistKind::Folder);
        let mut list = PlaylistNode::new("2", "Friday", PlaylistKind::Playlist);
        list.tracks = vec![
            TrackId("b".into()),
            TrackId("missing".into()),
            TrackId("a".into()),
        ];
        folder.children.push(list);
        let library = Library::new(tracks, vec![folder]);
        let (tracks, playlists) = select(&library, &Source::Collection);
        assert_eq!(tracks.len(), 2);
        assert_eq!(playlists.len(), 2);
        assert!(playlists[0].is_folder && playlists[0].parent_id == 0);
        assert_eq!(
            (playlists[1].parent_id, playlists[1].entries.clone()),
            (1, vec![2, 1])
        );
    }
}
