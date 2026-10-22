//! rekordbox 6/7 `master.db` reader.
//!
//! The file is SQLite encrypted with SQLCipher 4 defaults; the caller
//! supplies the key. Table and column meanings follow
//! `docs/spec/subsystems/01-library-database.md` sections 3.1 to 3.3.
//! Every synced table carries `rb_local_deleted`; rows with it set are in the
//! trash or awaiting sync deletion and are skipped.
//!
//! The database is opened read-only. Writing back is a later milestone and
//! must respect the sync bookkeeping columns (section 3.2).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cuepoint_core::{Bpm, PlaylistKind, PlaylistNode, Rating, Track, TrackColour, TrackId};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, Row};

use crate::{Error, Library, Result};

/// `djmdPlaylist.ParentID` of top-level nodes.
const ROOT_PARENT: &str = "root";

pub fn open(path: &Path, key: &str) -> Result<Library> {
    if !path.exists() {
        return Err(Error::Io {
            path: path.to_owned(),
            source: std::io::ErrorKind::NotFound.into(),
        });
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "key", key)?;
    // SQLCipher only checks the key on first read.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| Error::WrongKey)?;
    read(&conn)
}

fn read(conn: &Connection) -> Result<Library> {
    let tracks = read_tracks(conn)?;
    let playlists = read_playlists(conn)?;
    Ok(Library::new(tracks, playlists))
}

fn read_tracks(conn: &Connection) -> Result<Vec<Track>> {
    let mut statement = conn.prepare(
        "SELECT c.ID, c.Title, artist.Name, album.Name, genre.Name, label.Name, remixer.Name,
                c.Commnt, k.ScaleName, c.BPM, c.Length, c.Rating, colour.Commnt, colour.ColorCode,
                c.ReleaseYear, c.DJPlayCount, c.BitRate, c.SampleRate, c.FileSize, c.FileType,
                c.StockDate, c.created_at, c.FolderPath
         FROM djmdContent c
         LEFT JOIN djmdArtist artist ON artist.ID = c.ArtistID
         LEFT JOIN djmdAlbum album ON album.ID = c.AlbumID
         LEFT JOIN djmdGenre genre ON genre.ID = c.GenreID
         LEFT JOIN djmdLabel label ON label.ID = c.LabelID
         LEFT JOIN djmdArtist remixer ON remixer.ID = c.RemixerID
         LEFT JOIN djmdKey k ON k.ID = c.KeyID
         LEFT JOIN djmdColor colour ON colour.ID = c.ColorID
         WHERE c.rb_local_deleted = 0
         ORDER BY CAST(c.ID AS INTEGER)",
    )?;
    let rows = statement.query_map([], |row| {
        let field = Fields(row);
        Ok(Track {
            id: TrackId(field.text(0)),
            title: field.text(1),
            artist: field.text(2),
            album: field.text(3),
            genre: field.text(4),
            label: field.text(5),
            remixer: field.text(6),
            comment: field.text(7),
            key: field.text(8),
            bpm: field
                .int(9)
                .filter(|&b| b > 0)
                .map(|b| Bpm::from_centi(b as u32)),
            duration: field
                .int(10)
                .filter(|&s| s > 0)
                .map(|s| Duration::from_secs(s as u64)),
            rating: Rating::new(field.int(11).unwrap_or(0).clamp(0, 5) as u8),
            colour: Some(field.text(12))
                .filter(|name| !name.is_empty())
                .map(|name| TrackColour {
                    name,
                    rgb: field.int(13).filter(|&c| c > 0).map(|c| c as u32),
                }),
            year: field.int(14).filter(|&y| y > 0).map(|y| y as u16),
            play_count: field.int(15).unwrap_or(0).max(0) as u32,
            bit_rate: field.int(16).filter(|&r| r > 0).map(|r| r as u32),
            sample_rate: field.int(17).filter(|&r| r > 0).map(|r| r as u32),
            file_size: field.int(18).filter(|&s| s > 0).map(|s| s as u64),
            kind: field.int(19).map(file_kind).unwrap_or_default(),
            date_added: date_added(&field.text(20), &field.text(21)),
            location: Some(field.text(22))
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            // master.db keeps grids in the ANLZ files under AnalysisDataPath,
            // not in the database; reading those comes with device export.
            beatgrid: Default::default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn read_playlists(conn: &Connection) -> Result<Vec<PlaylistNode>> {
    struct Raw {
        id: String,
        parent: String,
        node: PlaylistNode,
    }

    let mut statement = conn.prepare(
        "SELECT ID, Name, Attribute, ParentID, SmartList FROM djmdPlaylist
         WHERE rb_local_deleted = 0 ORDER BY Seq, CAST(ID AS INTEGER)",
    )?;
    let raw = statement
        .query_map([], |row| {
            let field = Fields(row);
            let id = field.text(0);
            // Attribute: 0 playlist, 1 folder, 4 smart. Bit 0x80 is a sync
            // flag (inference, spec Q-list) and is masked off.
            let kind = match field.int(2).unwrap_or(0) & 0x7F {
                1 => PlaylistKind::Folder,
                4 => PlaylistKind::Smart,
                _ => PlaylistKind::Playlist,
            };
            let mut node = PlaylistNode::new(id.clone(), field.text(1), kind);
            if kind == PlaylistKind::Smart {
                node.smart_rules = Some(field.text(4)).filter(|rules| !rules.is_empty());
            }
            Ok(Raw {
                id,
                parent: field.text(3),
                node,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut members: HashMap<String, Vec<TrackId>> = HashMap::new();
    let mut statement = conn.prepare(
        "SELECT PlaylistID, ContentID FROM djmdSongPlaylist
         WHERE rb_local_deleted = 0 ORDER BY PlaylistID, TrackNo",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let field = Fields(row);
        members
            .entry(field.text(0))
            .or_default()
            .push(TrackId(field.text(1)));
    }

    // Children are listed under their parent in Seq order; attach bottom-up.
    let mut children: HashMap<String, Vec<Raw>> = HashMap::new();
    for item in raw {
        children.entry(item.parent.clone()).or_default().push(item);
    }
    fn build(
        parent: &str,
        children: &mut HashMap<String, Vec<Raw>>,
        members: &mut HashMap<String, Vec<TrackId>>,
    ) -> Vec<PlaylistNode> {
        let Some(items) = children.remove(parent) else {
            return Vec::new();
        };
        items
            .into_iter()
            .map(|mut item| {
                item.node.tracks = members.remove(&item.id).unwrap_or_default();
                item.node.children = build(&item.id, children, members);
                item.node
            })
            .collect()
    }
    Ok(build(ROOT_PARENT, &mut children, &mut members))
}

/// Reads columns whatever SQLite storage class they hold; rekordbox mixes
/// text and integers for ids and numbers across versions.
struct Fields<'a, 'b>(&'a Row<'b>);

impl Fields<'_, '_> {
    fn value(&self, index: usize) -> Value {
        self.0.get(index).unwrap_or(Value::Null)
    }

    fn text(&self, index: usize) -> String {
        match self.value(index) {
            Value::Text(text) => text,
            Value::Integer(n) => n.to_string(),
            Value::Real(n) => n.to_string(),
            Value::Null | Value::Blob(_) => String::new(),
        }
    }

    fn int(&self, index: usize) -> Option<i64> {
        match self.value(index) {
            Value::Integer(n) => Some(n),
            Value::Real(n) => Some(n.round() as i64),
            Value::Text(text) => text.trim().parse().ok(),
            Value::Null | Value::Blob(_) => None,
        }
    }
}

/// `djmdContent.FileType` codes, per prior art (pyrekordbox).
fn file_kind(code: i64) -> String {
    match code {
        1 => "MP3 File".into(),
        4 => "M4A File".into(),
        5 => "FLAC File".into(),
        11 => "WAV File".into(),
        12 => "AIFF File".into(),
        other => format!("Type {other}"),
    }
}

/// `StockDate` is the date added (`YYYY-MM-DD`); fall back to the row's
/// creation timestamp.
fn date_added(stock_date: &str, created_at: &str) -> String {
    let source = if stock_date.is_empty() {
        created_at
    } else {
        stock_date
    };
    source.get(..10).unwrap_or(source).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "cuepoint-test-key";

    /// The subset of the spec's schema this reader touches, plus the sync
    /// columns it filters on.
    const SCHEMA: &str = "
        CREATE TABLE djmdContent (ID VARCHAR(255) PRIMARY KEY, FolderPath VARCHAR(255), Title VARCHAR(255),
            ArtistID VARCHAR(255), AlbumID VARCHAR(255), GenreID VARCHAR(255), BPM INTEGER, Length INTEGER,
            BitRate INTEGER, Commnt TEXT, FileType INTEGER, Rating INTEGER, ReleaseYear INTEGER,
            RemixerID VARCHAR(255), LabelID VARCHAR(255), KeyID VARCHAR(255), StockDate VARCHAR(255),
            ColorID VARCHAR(255), DJPlayCount INTEGER, FileSize INTEGER, SampleRate INTEGER,
            rb_local_deleted TINYINT(1) DEFAULT 0, created_at DATETIME NOT NULL);
        CREATE TABLE djmdArtist (ID VARCHAR(255) PRIMARY KEY, Name VARCHAR(255), rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdAlbum (ID VARCHAR(255) PRIMARY KEY, Name VARCHAR(255), rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdGenre (ID VARCHAR(255) PRIMARY KEY, Name VARCHAR(255), rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdLabel (ID VARCHAR(255) PRIMARY KEY, Name VARCHAR(255), rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdKey (ID VARCHAR(255) PRIMARY KEY, ScaleName VARCHAR(255), Seq INTEGER, rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdColor (ID VARCHAR(255) PRIMARY KEY, ColorCode INTEGER, SortKey INTEGER, Commnt VARCHAR(255), rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdPlaylist (ID VARCHAR(255) PRIMARY KEY, Seq INTEGER, Name VARCHAR(255), Attribute INTEGER,
            ParentID VARCHAR(255), SmartList TEXT, rb_local_deleted TINYINT(1) DEFAULT 0);
        CREATE TABLE djmdSongPlaylist (ID VARCHAR(255) PRIMARY KEY, PlaylistID VARCHAR(255), ContentID VARCHAR(255),
            TrackNo INTEGER, rb_local_deleted TINYINT(1) DEFAULT 0);

        INSERT INTO djmdArtist (ID, Name) VALUES ('1', 'Rhythim Is Rhythim'), ('2', 'Aphex Twin');
        INSERT INTO djmdAlbum (ID, Name) VALUES ('1', 'Innovator');
        INSERT INTO djmdGenre (ID, Name) VALUES ('1', 'Techno');
        INSERT INTO djmdKey (ID, ScaleName, Seq) VALUES ('1', '8A', 1);
        INSERT INTO djmdColor (ID, ColorCode, SortKey, Commnt) VALUES ('1', 16711935, 1, 'Pink');
        INSERT INTO djmdContent (ID, FolderPath, Title, ArtistID, AlbumID, GenreID, BPM, Length, BitRate, FileType,
            Rating, KeyID, StockDate, ColorID, DJPlayCount, created_at, rb_local_deleted) VALUES
            ('10', '/Music/strings.mp3', 'Strings of Life', '1', '1', '1', 12500, 402, 320, 1, 4, '1', '2024-03-01', '1', 7, '2024-03-01 10:00:00', 0),
            ('9', '/Music/window.flac', 'Windowlicker', '2', NULL, NULL, '12730', 366, NULL, 5, 0, NULL, NULL, NULL, 0, '2023-01-02 09:00:00', 0),
            ('11', '/Music/deleted.mp3', 'In The Trash', '2', NULL, NULL, 12000, 300, NULL, 1, 0, NULL, NULL, NULL, 0, '2023-01-02 09:00:00', 1);
        INSERT INTO djmdPlaylist (ID, Seq, Name, Attribute, ParentID, SmartList) VALUES
            ('100', 1, 'Gigs', 1, 'root', NULL),
            ('101', 2, 'Warm Up', 0, 'root', NULL),
            ('102', 1, 'Fabric', 128, '100', NULL),
            ('103', 3, 'Fast Techno', 4, 'root', '<NODE Id=\"103\" LogicalOperator=\"1\"/>');
        INSERT INTO djmdSongPlaylist (ID, PlaylistID, ContentID, TrackNo, rb_local_deleted) VALUES
            ('1', '101', '10', 2, 0), ('2', '101', '9', 1, 0), ('3', '101', '11', 3, 1), ('4', '102', '10', 1, 0);
    ";

    fn fixture(dir: &Path) -> PathBuf {
        let path = dir.join("master.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "key", KEY).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        path
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cuepoint-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_tracks_and_skips_deleted() {
        let dir = temp_dir("master-db-tracks");
        let library = open(&fixture(&dir), KEY).unwrap();
        let titles: Vec<_> = library.tracks().iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Windowlicker", "Strings of Life"]);

        let t = library.track(&TrackId("10".into())).unwrap();
        assert_eq!(t.artist, "Rhythim Is Rhythim");
        assert_eq!(t.album, "Innovator");
        assert_eq!(t.bpm, Some(Bpm::from_centi(12500)));
        assert_eq!(t.key, "8A");
        assert_eq!(t.rating.stars(), 4);
        assert_eq!(t.colour.as_ref().map(|c| c.name.as_str()), Some("Pink"));
        assert_eq!(t.kind, "MP3 File");
        assert_eq!(t.date_added, "2024-03-01");
        assert_eq!(t.location.as_deref(), Some(Path::new("/Music/strings.mp3")));

        // BPM stored as text still parses; date falls back to created_at.
        let w = library.track(&TrackId("9".into())).unwrap();
        assert_eq!(w.bpm, Some(Bpm::from_centi(12730)));
        assert_eq!(w.date_added, "2023-01-02");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn builds_playlist_tree() {
        let dir = temp_dir("master-db-playlists");
        let library = open(&fixture(&dir), KEY).unwrap();
        let top: Vec<_> = library
            .playlists()
            .iter()
            .map(|n| (n.name.as_str(), n.kind))
            .collect();
        assert_eq!(
            top,
            [
                ("Gigs", PlaylistKind::Folder),
                ("Warm Up", PlaylistKind::Playlist),
                ("Fast Techno", PlaylistKind::Smart)
            ]
        );
        let warm_up = library.find_playlist("101").unwrap();
        let order: Vec<_> = library
            .playlist_tracks(warm_up)
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(order, ["Windowlicker", "Strings of Life"]);
        assert_eq!(
            library.find_playlist("102").unwrap().kind,
            PlaylistKind::Playlist
        );
        assert!(library.find_playlist("103").unwrap().smart_rules.is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wrong_key_is_reported() {
        let dir = temp_dir("master-db-key");
        let path = fixture(&dir);
        assert!(matches!(open(&path, "not-the-key"), Err(Error::WrongKey)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
