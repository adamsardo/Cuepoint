//! End to end: export a small library to a temporary "device" and read the
//! result back with rekordcrate, an independent reader (test-only).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use binrw::BinRead;
use cuepoint_analysis::ColourWaveform;
use cuepoint_core::{BeatGrid, MusicalKey};
use cuepoint_export::device::{PreparedTrack, write_device};
use cuepoint_export::{ExportPlaylist, ExportTrack, FileType};
use rekordcrate::anlz::{ANLZ, Content};
use rekordcrate::pdb::{Header, PageType, Row};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cuepoint-export-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn prepared(dir: &Path, n: usize, title: &str, artist: &str, bpm: f64) -> PreparedTrack {
    let source = dir.join(format!("source-{n}.mp3"));
    std::fs::write(&source, vec![n as u8; 1000 + n]).unwrap();
    let points = 30 * 150;
    PreparedTrack {
        row: ExportTrack {
            title: title.into(),
            artist: artist.into(),
            album: "Demo".into(),
            genre: "Techno".into(),
            key: Some(MusicalKey::new(9, true)),
            bpm_centi: (bpm * 100.0) as u32,
            duration_secs: 30,
            sample_rate: 44_100,
            bit_rate_kbps: 320,
            file_type: FileType::Mp3,
            date_added: "2026-10-22".into(),
            ..ExportTrack::default()
        },
        source,
        grid: BeatGrid::constant(bpm, 0.05, 1),
        waveform: ColourWaveform {
            points_per_second: 150,
            low: vec![200; points],
            mid: vec![100; points],
            high: vec![50; points],
        },
        duration_secs: 30.0,
    }
}

fn pdb_rows(bytes: &[u8]) -> Vec<(PageType, Vec<Row>)> {
    let mut reader = Cursor::new(bytes);
    let header = Header::read(&mut reader).expect("rekordcrate reads the header");
    header
        .tables
        .iter()
        .map(|table| {
            let pages = header
                .read_pages(
                    &mut reader,
                    binrw::Endian::NATIVE,
                    (&table.first_page, &table.last_page),
                )
                .expect("rekordcrate reads every page");
            let rows = pages
                .iter()
                .flat_map(|page| {
                    page.row_groups
                        .iter()
                        .flat_map(|g| g.present_rows())
                        .collect::<Vec<_>>()
                })
                .collect();
            (table.page_type, rows)
        })
        .collect()
}

#[test]
fn exported_device_reads_back_with_rekordcrate() {
    let dir = temp_dir("roundtrip");
    let root = dir.join("USB");
    let tracks = vec![
        prepared(&dir, 1, "Strings", "Rhythim Is Rhythim", 125.0),
        prepared(&dir, 2, "Café Δ", "Ivo Paskal", 128.0),
        prepared(&dir, 3, "Third", "Rhythim Is Rhythim", 174.0),
    ];
    let playlists = vec![
        ExportPlaylist {
            id: 1,
            parent_id: 0,
            name: "Gigs".into(),
            is_folder: true,
            sort_order: 0,
            entries: vec![],
        },
        ExportPlaylist {
            id: 2,
            parent_id: 1,
            name: "Friday".into(),
            is_folder: false,
            sort_order: 0,
            entries: vec![3, 1],
        },
    ];
    let report = write_device(&root, tracks, playlists, &mut |_| true).unwrap();
    assert_eq!(
        (report.tracks, report.playlists, report.already_present),
        (3, 2, 0)
    );

    // Audio copied under Contents with the documented layout.
    let copied = root.join("Contents/Ivo Paskal/Demo/source-2.mp3");
    assert_eq!(std::fs::read(&copied).unwrap(), vec![2u8; 1002]);

    let pdb = std::fs::read(root.join("PIONEER/rekordbox/export.pdb")).unwrap();
    let tables = pdb_rows(&pdb);
    let rows_of =
        |kind: PageType| -> &Vec<Row> { &tables.iter().find(|(k, _)| *k == kind).unwrap().1 };
    let tracks = rows_of(PageType::Tracks);
    assert_eq!(tracks.len(), 3);
    let dump = format!("{tracks:?}");
    for text in [
        "Strings",
        "Café Δ",
        "/Contents/Ivo Paskal/Demo/source-2.mp3",
        "/PIONEER/USBANLZ/",
    ] {
        assert!(dump.contains(text), "track rows lack {text:?}");
    }
    assert_eq!(
        rows_of(PageType::Artists).len(),
        2,
        "shared artist stored once"
    );
    assert_eq!(rows_of(PageType::PlaylistTree).len(), 2);
    assert_eq!(rows_of(PageType::PlaylistEntries).len(), 2);

    // Every track's analysis files exist where its row points, and parse.
    for path in std::fs::read_dir(root.join("PIONEER/USBANLZ")).unwrap() {
        for folder in std::fs::read_dir(path.unwrap().path()).unwrap() {
            let folder = folder.unwrap().path();
            for ext in ["DAT", "EXT"] {
                let bytes = std::fs::read(folder.join(format!("ANLZ0000.{ext}"))).unwrap();
                let anlz = ANLZ::read(&mut Cursor::new(&bytes)).expect("rekordcrate reads ANLZ");
                let has_path = anlz
                    .sections
                    .iter()
                    .any(|s| matches!(s.content, Content::Path(_)));
                assert!(has_path, "{ext} has a path section");
                if ext == "DAT" {
                    let beats = anlz.sections.iter().find_map(|s| match &s.content {
                        Content::BeatGrid(g) => Some(g.beats.len()),
                        _ => None,
                    });
                    assert!(beats.unwrap() > 50, "beat grid written");
                }
            }
        }
    }

    // Re-exporting keeps audio already on the device.
    let again = vec![prepared(&dir, 1, "Strings", "Rhythim Is Rhythim", 125.0)];
    let report = write_device(&root, again, vec![], &mut |_| true).unwrap();
    assert_eq!(report.already_present, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Every file under `dir`, with its contents.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn cancelled_re_export_leaves_the_previous_export_intact() {
    let dir = temp_dir("cancel-reexport");
    let root = dir.join("USB");
    let first = vec![prepared(&dir, 1, "First", "Same Artist", 125.0)];
    write_device(&root, first, vec![], &mut |_| true).unwrap();
    let before = snapshot(&root);

    // A different track whose audio lands on the same device path.
    let mut other = prepared(&dir, 2, "Other", "Same Artist", 140.0);
    let renamed = dir.join("other").join("source-1.mp3");
    std::fs::create_dir_all(renamed.parent().unwrap()).unwrap();
    std::fs::rename(&other.source, &renamed).unwrap();
    other.source = renamed;
    let result = write_device(&root, vec![other], vec![], &mut |p| {
        p != cuepoint_export::device::Progress::WritingLibrary
    });
    assert!(matches!(
        result,
        Err(cuepoint_export::device::ExportError::Cancelled)
    ));
    assert_eq!(snapshot(&root), before, "old files kept, nothing left over");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancelling_stops_the_export() {
    let dir = temp_dir("cancel");
    let tracks = vec![prepared(&dir, 1, "A", "B", 120.0)];
    let result = write_device(&dir.join("USB"), tracks, vec![], &mut |_| false);
    assert!(matches!(
        result,
        Err(cuepoint_export::device::ExportError::Cancelled)
    ));
    std::fs::remove_dir_all(dir).unwrap();
}
