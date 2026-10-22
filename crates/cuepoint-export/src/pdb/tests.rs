//! Round trips through a small reader written separately from the writer,
//! straight from the documented layout (Deep Symmetry's export analysis and
//! crate-digger's `rekordbox_pdb.ksy`). It also checks the page
//! bookkeeping a strict reader would rely on.

use super::*;
use crate::model::{ExportPlaylist, ExportTrack, FileType};
use cuepoint_core::MusicalKey;

const PAGE: usize = 4096;

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Decodes the DeviceSQL string at `at`.
fn string_at(bytes: &[u8], at: usize) -> String {
    let kind = bytes[at];
    if kind & 1 == 1 {
        let len = usize::from(kind >> 1) - 1;
        return String::from_utf8(bytes[at + 1..at + 1 + len].to_vec()).unwrap();
    }
    let len = usize::from(u16_at(bytes, at + 1)) - 4;
    assert_eq!(bytes[at + 3], 0, "pad byte");
    assert_eq!(at % 4, 0, "long strings start on a 4-byte boundary");
    let body = &bytes[at + 4..at + 4 + len];
    match kind {
        0x40 => {
            assert!(body.is_ascii());
            String::from_utf8(body.to_vec()).unwrap()
        }
        0x90 => {
            let units: Vec<u16> = body
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16(&units).unwrap()
        }
        _ => panic!("unknown string kind {kind:#x}"),
    }
}

/// A present row: the page holding it and its start within the page.
struct RowRef<'a> {
    page: &'a [u8],
    start: usize,
}

impl RowRef<'_> {
    fn u8(&self, at: usize) -> u8 {
        self.page[self.start + at]
    }
    fn u16(&self, at: usize) -> u16 {
        u16_at(self.page, self.start + at)
    }
    fn u32(&self, at: usize) -> u32 {
        u32_at(self.page, self.start + at)
    }
    /// The string at an offset from the row start.
    fn string(&self, offset: usize) -> String {
        string_at(self.page, self.start + offset)
    }
}

struct Pdb<'a> {
    bytes: &'a [u8],
    tables: Vec<[u32; 4]>,
}

impl<'a> Pdb<'a> {
    /// Parses the file header and checks every table's page chain.
    fn parse(bytes: &'a [u8]) -> Self {
        assert_eq!(u32_at(bytes, 0), 0);
        assert_eq!(u32_at(bytes, 4), PAGE as u32);
        assert_eq!(bytes.len() % PAGE, 0);
        let file_pages = (bytes.len() / PAGE) as u32;
        let count = u32_at(bytes, 8) as usize;
        let next_unused = u32_at(bytes, 0x0c);
        assert!(next_unused >= file_pages);
        assert_eq!(u32_at(bytes, 0x18), 0);
        let tables: Vec<[u32; 4]> = (0..count)
            .map(|i| {
                let at = 0x1c + 16 * i;
                std::array::from_fn(|field| u32_at(bytes, at + 4 * field))
            })
            .collect();
        let pdb = Self { bytes, tables };
        let mut seen = vec![false; file_pages as usize];
        for &[kind, empty_candidate, first, last] in &pdb.tables {
            assert!(empty_candidate < next_unused);
            let chain = pdb.chain(kind);
            assert_eq!(chain.first(), Some(&first));
            assert_eq!(chain.last(), Some(&last));
            let after_last = u32_at(pdb.page(last), 0x0c);
            assert_eq!(after_last, empty_candidate);
            for (i, &page) in chain.iter().enumerate() {
                assert!(
                    !std::mem::replace(&mut seen[page as usize], true),
                    "page {page} reused"
                );
                let flags = pdb.page(page)[0x1b];
                assert_eq!(flags, if i == 0 { 0x64 } else { 0x24 });
            }
        }
        pdb
    }

    fn page(&self, index: u32) -> &'a [u8] {
        let start = index as usize * PAGE;
        &self.bytes[start..start + PAGE]
    }

    fn table(&self, kind: u32) -> [u32; 4] {
        *self.tables.iter().find(|table| table[0] == kind).unwrap()
    }

    /// Page indices of a table, from `first_page` to `last_page`.
    fn chain(&self, kind: u32) -> Vec<u32> {
        let [_, _, first, last] = self.table(kind);
        let mut pages = vec![first];
        let mut current = first;
        while current != last {
            current = u32_at(self.page(current), 0x0c);
            pages.push(current);
        }
        for &index in &pages {
            let page = self.page(index);
            assert_eq!(u32_at(page, 0x04), index);
            assert_eq!(u32_at(page, 0x08), kind);
        }
        pages
    }

    /// Present rows of a table, in order, checking each data page's counts.
    fn rows(&self, kind: u32) -> Vec<RowRef<'a>> {
        let mut rows = Vec::new();
        for index in self.chain(kind) {
            let page = self.page(index);
            if page[0x1b] & 0x40 != 0 {
                continue;
            }
            let counts =
                u32::from(page[0x18]) | u32::from(page[0x19]) << 8 | u32::from(page[0x1a]) << 16;
            let offsets = (counts & 0x1fff) as usize;
            let present_rows = (counts >> 13) as usize;
            assert!(offsets > 0);
            let used = usize::from(u16_at(page, 0x1e));
            let free = usize::from(u16_at(page, 0x1c));
            let groups = offsets.div_ceil(16);
            assert_eq!(free + used + 2 * offsets + 4 * groups, PAGE - 0x28);
            let mut present = 0;
            for i in 0..offsets {
                let group_end = PAGE - 0x24 * (i / 16);
                let flags = u16_at(page, group_end - 4);
                if flags >> (i % 16) & 1 == 1 {
                    present += 1;
                    let offset = usize::from(u16_at(page, group_end - 6 - 2 * (i % 16)));
                    assert!(offset < used);
                    rows.push(RowRef {
                        page,
                        start: 0x28 + offset,
                    });
                }
            }
            assert_eq!(present, present_rows);
        }
        rows
    }

    /// `(id, name)` of genre, label and history playlist rows.
    fn names(&self, kind: u32) -> Vec<(u32, String)> {
        let rows = self.rows(kind);
        rows.iter().map(|row| (row.u32(0), row.string(4))).collect()
    }
}

/// What a track row says, read back.
#[derive(Debug, PartialEq)]
struct TrackRead {
    id: u32,
    title: String,
    artist: u32,
    remixer: u32,
    album: u32,
    genre: u32,
    label: u32,
    key: u32,
    colour: u8,
    rating: u8,
    bpm_centi: u32,
    duration: u16,
    year: u16,
    sample_rate: u32,
    bit_rate: u32,
    bit_depth: u16,
    file_size: u32,
    file_type: u16,
    file_path: String,
    file_name: String,
    analysis_path: String,
    date_added: String,
    comment: String,
}

fn read_track(row: &RowRef<'_>) -> TrackRead {
    assert_eq!(row.u16(0), 0x24);
    let string = |index: usize| row.string(usize::from(row.u16(0x5e + 2 * index)));
    TrackRead {
        id: row.u32(0x48),
        title: string(17),
        artist: row.u32(0x44),
        remixer: row.u32(0x2c),
        album: row.u32(0x40),
        genre: row.u32(0x3c),
        label: row.u32(0x28),
        key: row.u32(0x20),
        colour: row.u8(0x58),
        rating: row.u8(0x59),
        bpm_centi: row.u32(0x38),
        duration: row.u16(0x54),
        year: row.u16(0x50),
        sample_rate: row.u32(0x08),
        bit_rate: row.u32(0x30),
        bit_depth: row.u16(0x52),
        file_size: row.u32(0x10),
        file_type: row.u16(0x5a),
        file_path: string(20),
        file_name: string(19),
        analysis_path: string(14),
        date_added: string(10),
        comment: string(16),
    }
}

/// The name of an artist or album row, through its near or far offset.
fn subtype_name(row: &RowRef<'_>, near_at: usize, far_at: usize) -> String {
    let offset = if row.u16(0) & 0x04 == 0 {
        usize::from(row.u8(near_at))
    } else {
        usize::from(row.u16(far_at))
    };
    row.string(offset)
}

fn track(id: u32) -> ExportTrack {
    ExportTrack {
        id,
        title: format!("Track {id}"),
        bpm_centi: 12_000 + id,
        duration_secs: 300,
        sample_rate: 44_100,
        bit_rate_kbps: 320,
        bit_depth: 16,
        file_size: 9_000_000,
        file_type: FileType::Mp3,
        device_path: format!("/Contents/Artist/Album/Track {id}.mp3"),
        analysis_path: format!("/PIONEER/USBANLZ/P000/{id:08X}/ANLZ0000.DAT"),
        date_added: "2026-10-22".into(),
        ..ExportTrack::default()
    }
}

fn sample_library() -> DeviceLibrary {
    let am = MusicalKey::new(9, true);
    let long_title = "An Extremely Long Extended Club Mix Title ".repeat(4);
    let tracks = vec![
        ExportTrack {
            title: "Café Δ".into(),
            artist: "Alpha".into(),
            remixer: "Beta".into(),
            album: "First".into(),
            genre: "House".into(),
            label: "Label One".into(),
            comment: "warm-up, then peak".into(),
            key: Some(am),
            year: 2024,
            rating: 4,
            colour: 3,
            ..track(1)
        },
        ExportTrack {
            title: long_title.clone(),
            artist: "Alpha".into(),
            album: "First".into(),
            genre: "House".into(),
            key: Some(MusicalKey::new(6, false)),
            file_type: FileType::Flac,
            bit_depth: 24,
            sample_rate: 96_000,
            colour: 8,
            device_path: format!("/Contents/Alpha/First/{long_title}.flac"),
            ..track(2)
        },
        // Every optional field empty: ids 0 and empty strings.
        ExportTrack {
            title: String::new(),
            artist: "Gamma".into(),
            file_type: FileType::Unknown,
            date_added: String::new(),
            analysis_path: String::new(),
            bpm_centi: 0,
            ..track(3)
        },
        ExportTrack {
            title: "Ünïcödé ".repeat(20),
            artist: "Beta".into(),
            album: "Second Δ".into(),
            genre: "Techno".into(),
            label: "Label One".into(),
            key: Some(am),
            colour: 9, // out of range: no colour
            rating: 7, // clamped to 5
            file_type: FileType::Wav,
            ..track(4)
        },
    ];
    let playlist =
        |id, parent_id, name: &str, is_folder, sort_order, entries: Vec<u32>| ExportPlaylist {
            id,
            parent_id,
            name: name.into(),
            is_folder,
            sort_order,
            entries,
        };
    DeviceLibrary {
        tracks,
        playlists: vec![
            playlist(1, 0, "Sets", true, 0, vec![]),
            playlist(2, 1, "Friday ♫", false, 0, vec![4, 1, 2]),
            playlist(3, 1, "Deep", true, 1, vec![]),
            playlist(4, 3, "Inner", false, 0, vec![3]),
            playlist(5, 0, "All", false, 1, vec![1, 2, 3, 4]),
        ],
    }
}

#[test]
fn every_table_reads_back() {
    let library = sample_library();
    let bytes = write_pdb(&library);
    let pdb = Pdb::parse(&bytes);
    let kinds: Vec<u32> = pdb.tables.iter().map(|table| table[0]).collect();
    assert_eq!(kinds, (0..20).collect::<Vec<_>>());

    let tracks: Vec<TrackRead> = pdb.rows(kind::TRACKS).iter().map(read_track).collect();
    assert_eq!(tracks.len(), 4);
    for (read, track) in tracks.iter().zip(&library.tracks) {
        assert_eq!(read.id, track.id);
        assert_eq!(read.title, track.title);
        assert_eq!(read.bpm_centi, track.bpm_centi);
        assert_eq!(u32::from(read.duration), track.duration_secs);
        assert_eq!(read.year, track.year);
        assert_eq!(read.sample_rate, track.sample_rate);
        assert_eq!(read.bit_rate, track.bit_rate_kbps);
        assert_eq!(read.bit_depth, track.bit_depth);
        assert_eq!(u64::from(read.file_size), track.file_size);
        assert_eq!(read.file_path, track.device_path);
        assert_eq!(
            read.file_name,
            track.device_path.rsplit('/').next().unwrap()
        );
        assert_eq!(read.analysis_path, track.analysis_path);
        assert_eq!(read.date_added, track.date_added);
        assert_eq!(read.comment, track.comment);
    }
    let links: Vec<_> = tracks
        .iter()
        .map(|t| (t.artist, t.remixer, t.album, t.genre, t.label, t.key))
        .collect();
    assert_eq!(
        links,
        [
            (1, 2, 1, 1, 1, 1),
            (1, 0, 1, 1, 0, 2),
            (3, 0, 0, 0, 0, 0),
            (2, 0, 2, 2, 1, 1),
        ]
    );
    let small: Vec<_> = tracks
        .iter()
        .map(|t| (t.colour, t.rating, t.file_type))
        .collect();
    assert_eq!(
        small,
        [(3, 4, 0x01), (8, 0, 0x05), (0, 0, 0x00), (0, 5, 0x0b)]
    );

    let artists: Vec<_> = pdb
        .rows(kind::ARTISTS)
        .iter()
        .map(|row| (row.u32(4), subtype_name(row, 9, 0x0a)))
        .collect();
    assert_eq!(
        artists,
        [(1, "Alpha".into()), (2, "Beta".into()), (3, "Gamma".into())]
    );
    let albums: Vec<_> = pdb
        .rows(kind::ALBUMS)
        .iter()
        .map(|row| (row.u32(0x0c), row.u32(0x08), subtype_name(row, 0x15, 0x16)))
        .collect();
    assert_eq!(albums, [(1, 1, "First".into()), (2, 2, "Second Δ".into())]);
    assert_eq!(
        pdb.names(kind::GENRES),
        [(1, "House".into()), (2, "Techno".into())]
    );
    assert_eq!(pdb.names(kind::LABELS), [(1, "Label One".into())]);
    let keys: Vec<_> = pdb
        .rows(kind::KEYS)
        .iter()
        .map(|row| (row.u32(0), row.u32(4), row.string(8)))
        .collect();
    assert_eq!(keys, [(1, 1, "Am".into()), (2, 2, "F#".into())]);
    let colours: Vec<_> = pdb
        .rows(kind::COLOURS)
        .iter()
        .map(|row| (row.u16(5), row.string(8)))
        .collect();
    let expected: Vec<_> = (1..).zip(COLOURS.map(String::from)).collect();
    assert_eq!(colours, expected);

    let tree: Vec<_> = pdb
        .rows(kind::PLAYLIST_TREE)
        .iter()
        .map(|row| {
            (
                row.u32(0x0c),
                row.u32(0),
                row.u32(8),
                row.u32(0x10) != 0,
                row.string(0x14),
            )
        })
        .collect();
    let expected: Vec<_> = library
        .playlists
        .iter()
        .map(|p| (p.id, p.parent_id, p.sort_order, p.is_folder, p.name.clone()))
        .collect();
    assert_eq!(tree, expected);
    let entries: Vec<_> = pdb
        .rows(kind::PLAYLIST_ENTRIES)
        .iter()
        .map(|row| [row.u32(0), row.u32(4), row.u32(8)])
        .collect();
    assert_eq!(
        entries,
        [
            [1, 4, 2],
            [2, 1, 2],
            [3, 2, 2],
            [1, 3, 4],
            [1, 1, 5],
            [2, 2, 5],
            [3, 3, 5],
            [4, 4, 5],
        ]
    );

    let columns: Vec<_> = pdb
        .rows(kind::COLUMNS)
        .iter()
        .map(|row| (row.u16(0), row.u16(2), row.string(4)))
        .collect();
    assert_eq!(columns.len(), 27);
    assert_eq!(columns[0], (1, 0x80, "\u{fffa}GENRE\u{fffb}".into()));
    assert_eq!(pdb.rows(kind::MENU).len(), MENU.len());
    assert_eq!(pdb.rows(kind::MENU_SORT).len(), MENU_SORT.len());
    for empty in [9, 10, 11, 12, 13, 14, 15, 19] {
        assert!(pdb.rows(empty).is_empty(), "table {empty}");
    }
}

#[test]
fn rows_carry_rekordbox_constants() {
    let bytes = write_pdb(&sample_library());
    let pdb = Pdb::parse(&bytes);
    let tracks = pdb.rows(kind::TRACKS);
    for (i, row) in tracks.iter().enumerate() {
        // index_shift is 0x20 per row within its page.
        assert_eq!(usize::from(row.u16(2)), 0x20 * i);
        assert_eq!(row.u32(4), 0x000c_0700);
        assert_eq!(row.u16(0x56), 0x29);
        assert_eq!(row.u16(0x5c), 3);
        let autoload = row.string(usize::from(row.u16(0x5e + 2 * 7)));
        assert_eq!(autoload, "ON");
    }
    let index_page = pdb.page(pdb.table(kind::TRACKS)[2]);
    assert_eq!(u16_at(index_page, 0x24), 0x3ec);
    assert_eq!(u32_at(index_page, 0x3c), 0x1fff_fff8);
    // An empty table still has its index page and a reserved blank page.
    let [_, candidate, first, last] = pdb.table(kind::ARTWORK);
    assert_eq!(first, last);
    assert_eq!(candidate, first + 1);
    assert!(pdb.page(candidate).iter().all(|&b| b == 0));
}

#[test]
fn multi_page_tables() {
    let tracks: Vec<ExportTrack> = (1..=2000)
        .map(|id| ExportTrack {
            artist: format!("Artist {}", id % 700),
            // Albums are keyed by name and artist; id % 1400 fixes both.
            album: format!("Album {}", id % 1400),
            genre: format!("Genre {}", id % 40),
            comment: "c".repeat(60),
            ..track(id)
        })
        .collect();
    let library = DeviceLibrary {
        playlists: vec![ExportPlaylist {
            id: 1,
            name: "Everything".into(),
            entries: tracks.iter().rev().map(|t| t.id).collect(),
            ..ExportPlaylist::default()
        }],
        tracks,
    };
    let bytes = write_pdb(&library);
    let pdb = Pdb::parse(&bytes);
    // Index page plus several data pages.
    assert!(pdb.chain(kind::TRACKS).len() > 10);
    assert!(pdb.chain(kind::PLAYLIST_ENTRIES).len() > 3);
    assert!(pdb.chain(kind::ARTISTS).len() > 2);

    let read: Vec<TrackRead> = pdb.rows(kind::TRACKS).iter().map(read_track).collect();
    assert_eq!(read.len(), 2000);
    for (read, track) in read.iter().zip(&library.tracks) {
        assert_eq!((read.id, &read.title), (track.id, &track.title));
        assert_eq!(read.file_path, track.device_path);
    }
    let artists = pdb.rows(kind::ARTISTS);
    assert_eq!(artists.len(), 700);
    for (row, id) in artists.iter().zip(1..) {
        assert_eq!(row.u32(4), id);
    }
    assert_eq!(pdb.rows(kind::ALBUMS).len(), 1400);
    let entries: Vec<_> = pdb
        .rows(kind::PLAYLIST_ENTRIES)
        .iter()
        .map(|row| (row.u32(0), row.u32(4)))
        .collect();
    let expected: Vec<_> = (1..).zip((1..=2000).rev()).collect();
    assert_eq!(entries, expected);
}

#[test]
fn output_is_deterministic() {
    let library = sample_library();
    assert_eq!(write_pdb(&library), write_pdb(&library));
}

#[test]
fn empty_library_has_every_table() {
    let bytes = write_pdb(&DeviceLibrary::default());
    let pdb = Pdb::parse(&bytes);
    assert_eq!(pdb.tables.len(), 20);
    assert!(pdb.rows(kind::TRACKS).is_empty());
    assert_eq!(pdb.rows(kind::COLOURS).len(), 8);
    assert_eq!(pdb.rows(kind::COLUMNS).len(), 27);
}
