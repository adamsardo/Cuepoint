//! rekordbox XML (`DJ_PLAYLISTS`) reader.
//!
//! Layout, per AlphaTheta's public "rekordbox XML format" document and
//! `docs/spec/subsystems/01-library-database.md` section 3.6:
//! `COLLECTION/TRACK` holds one element per track (attributes are the track
//! fields, with `TEMPO` grid markers and `POSITION_MARK` cues as children), and
//! `PLAYLISTS/NODE` is a tree whose root node is named `ROOT`. A node with
//! `Type="0"` is a folder; `Type="1"` is a playlist whose `TRACK Key=...`
//! children reference tracks by `TrackID` (`KeyType="0"`) or by `Location`
//! (`KeyType="1"`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cuepoint_core::{
    BeatGrid, Bpm, Cue, PlaylistKind, PlaylistNode, Rating, TempoMarker, Track, TrackColour,
    TrackId,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::{Error, Library, Result};

pub fn open(path: &Path) -> Result<Library> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    parse(&decode_text(&bytes)?)
}

/// Decodes UTF-8 (with or without BOM) or BOM-marked UTF-16, which
/// rekordbox's own importer also accepts.
fn decode_text(bytes: &[u8]) -> Result<String> {
    let utf16 = |rest: &[u8], from: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().copied().map(from).collect();
        String::from_utf16(&units).map_err(|_| Error::Xml("invalid UTF-16 text".into()))
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()),
        [0xFF, 0xFE, rest @ ..] => return utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => return utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8(bytes.to_vec()),
    }
    .map_err(|_| Error::Xml("invalid UTF-8 text".into()))
}

pub fn parse(text: &str) -> Result<Library> {
    let mut reader = Reader::from_str(text);
    let mut tracks = Vec::new();
    let mut location_ids = HashMap::new();
    // Open NODE elements, innermost last, plus each playlist's key type.
    let mut stack: Vec<(PlaylistNode, KeyType)> = Vec::new();
    let mut roots = Vec::new();
    let mut in_collection = false;
    // TEMPO markers of the collection track being read.
    let mut markers = Vec::new();
    let mut in_playlists = false;
    let mut node_count = 0usize;
    // quick-xml reports EOF without checking that every element closed, so
    // track nesting to reject truncated files instead of loading part of them.
    let mut depth = 0usize;
    let mut saw_root = false;

    loop {
        let event = reader.read_event().map_err(|e| Error::Xml(e.to_string()))?;
        if let Event::Start(e) | Event::Empty(e) = &event
            && depth == 0
        {
            if saw_root || e.name().as_ref() != "DJ_PLAYLISTS" {
                return Err(Error::Xml("root element is not DJ_PLAYLISTS".into()));
            }
            saw_root = true;
        }
        let (element, is_empty) = match &event {
            Event::Start(e) => {
                depth += 1;
                (e, false)
            }
            Event::Empty(e) => (e, true),
            Event::End(e) => {
                depth -= 1;
                match e.name().as_ref() {
                    "COLLECTION" => in_collection = false,
                    "TRACK" if in_collection => finish_grid(&mut tracks, &mut markers),
                    "PLAYLISTS" => in_playlists = false,
                    "NODE" => close_node(&mut stack, &mut roots),
                    _ => {}
                }
                continue;
            }
            Event::Eof if saw_root && depth == 0 => break,
            Event::Eof => {
                return Err(Error::Xml(
                    "file ends before the document is complete (truncated export?)".into(),
                ));
            }
            _ => continue,
        };
        let attrs = Attrs::read(element)?;
        match element.name().as_ref() {
            "COLLECTION" => in_collection = !is_empty,
            "PLAYLISTS" => in_playlists = !is_empty,
            "TEMPO" if in_collection => markers.extend(tempo_marker(&attrs)),
            "POSITION_MARK" if in_collection => {
                if let (Some(track), Some(cue)) = (tracks.last_mut(), position_mark(&attrs)) {
                    track.cues.push(cue);
                }
            }
            "TRACK" if in_collection && stack.is_empty() => {
                let track = track_from(&attrs);
                if let Some(location) = attrs.get("Location") {
                    location_ids.insert(location.to_owned(), track.id.clone());
                }
                tracks.push(track);
                if is_empty {
                    finish_grid(&mut tracks, &mut markers);
                }
            }
            "NODE" if in_playlists => {
                node_count += 1;
                let kind = match attrs.get("Type") {
                    Some("0") => PlaylistKind::Folder,
                    _ => PlaylistKind::Playlist,
                };
                let key_type = match attrs.get("KeyType") {
                    Some("1") => KeyType::Location,
                    _ => KeyType::TrackId,
                };
                // XML has no node ids; a running number is stable per file.
                let node = PlaylistNode::new(
                    node_count.to_string(),
                    attrs.get("Name").unwrap_or(""),
                    kind,
                );
                stack.push((node, key_type));
                if is_empty {
                    close_node(&mut stack, &mut roots);
                }
            }
            "TRACK" if in_playlists => {
                if let (Some((node, key_type)), Some(key)) = (stack.last_mut(), attrs.get("Key")) {
                    let id = match key_type {
                        KeyType::TrackId => Some(TrackId(key.to_owned())),
                        KeyType::Location => location_ids.get(key).cloned(),
                    };
                    node.tracks.extend(id);
                }
            }
            _ => {}
        }
    }

    // The tree's single root is named ROOT; show its children at top level.
    let playlists = match roots.as_slice() {
        [root] if root.kind == PlaylistKind::Folder && root.name == "ROOT" => {
            roots.remove(0).children
        }
        _ => roots,
    };
    Ok(Library::new(tracks, playlists))
}

/// `TEMPO Inizio="0.025" Bpm="128.00" Metro="4/4" Battito="1"`: a grid
/// marker at `Inizio` seconds whose beat is number `Battito` in its bar.
fn tempo_marker(attrs: &Attrs) -> Option<TempoMarker> {
    Some(TempoMarker {
        time: attrs.number("Inizio")?,
        bpm: attrs.number("Bpm")?,
        beat_in_bar: attrs.number("Battito").unwrap_or(1),
    })
}

/// `POSITION_MARK Name="" Type="0" Start="0.049" Num="0" Red="255" ...`:
/// `Num` is the hot cue slot (0 = A) or -1 for a memory cue; `Type` 4 is a
/// loop ending at `End`. Fade and load markers (types 1 to 3) are skipped.
fn position_mark(attrs: &Attrs) -> Option<Cue> {
    let kind: u8 = attrs.number("Type").unwrap_or(0);
    if !matches!(kind, 0 | 4) {
        return None;
    }
    let slot: i32 = attrs.number("Num").unwrap_or(-1);
    let colour = match (
        attrs.number::<u32>("Red"),
        attrs.number::<u32>("Green"),
        attrs.number::<u32>("Blue"),
    ) {
        (Some(r), Some(g), Some(b)) => Some((r.min(255) << 16) | (g.min(255) << 8) | b.min(255)),
        _ => None,
    };
    Some(Cue {
        hot_cue: u8::try_from(slot).ok().filter(|&s| s < 16),
        start: attrs.number("Start")?,
        loop_end: if kind == 4 { attrs.number("End") } else { None },
        name: attrs.get("Name").unwrap_or("").to_owned(),
        colour,
    })
}

fn finish_grid(tracks: &mut [Track], markers: &mut Vec<TempoMarker>) {
    if let Some(track) = tracks.last_mut()
        && !markers.is_empty()
    {
        track.beatgrid = BeatGrid::from_markers(std::mem::take(markers));
    }
    markers.clear();
}

#[derive(Debug, Clone, Copy)]
enum KeyType {
    TrackId,
    Location,
}

fn close_node(stack: &mut Vec<(PlaylistNode, KeyType)>, roots: &mut Vec<PlaylistNode>) {
    if let Some((node, _)) = stack.pop() {
        match stack.last_mut() {
            Some((parent, _)) => parent.children.push(node),
            None => roots.push(node),
        }
    }
}

struct Attrs(HashMap<String, String>);

impl Attrs {
    fn read(element: &BytesStart<'_>) -> Result<Self> {
        let mut map = HashMap::new();
        for attr in element.attributes() {
            let attr = attr.map_err(|e| Error::Xml(e.to_string()))?;
            let value = attr
                .normalized_value(XmlVersion::Explicit1_0)
                .map_err(|e| Error::Xml(e.to_string()))?;
            map.insert(attr.key.as_ref().to_owned(), value.into_owned());
        }
        Ok(Self(map))
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    fn text(&self, key: &str) -> String {
        self.get(key).unwrap_or_default().to_owned()
    }

    fn number<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key)?.trim().parse().ok()
    }
}

fn track_from(attrs: &Attrs) -> Track {
    let location = attrs.get("Location");
    // rekordbox exports streaming tracks (Beatport, TIDAL and so on) with a
    // service catalogue path instead of a file, such as
    // `file://localhost/v4/catalog/tracks/27134064/`; they have no file here.
    let streaming = location.is_some_and(|l| l.contains("localhost/v4/catalog/"));
    Track {
        id: TrackId(attrs.text("TrackID")),
        title: attrs.text("Name"),
        artist: attrs.text("Artist"),
        album: attrs.text("Album"),
        genre: attrs.text("Genre"),
        label: attrs.text("Label"),
        remixer: attrs.text("Remixer"),
        comment: attrs.text("Comments"),
        key: attrs.text("Tonality"),
        bpm: attrs.number("AverageBpm").and_then(Bpm::from_f64),
        duration: attrs.number("TotalTime").map(Duration::from_secs),
        // XML stores stars on a 0-255 scale in steps of 51.
        rating: Rating::new(
            attrs
                .number::<u16>("Rating")
                .map_or(0, |r| ((r + 25) / 51) as u8),
        ),
        colour: attrs.get("Colour").and_then(parse_colour),
        year: attrs.number("Year").filter(|&y| y > 0),
        play_count: attrs.number("PlayCount").unwrap_or(0),
        bit_rate: attrs.number("BitRate").filter(|&r| r > 0),
        sample_rate: attrs.number("SampleRate").filter(|&r| r > 0),
        file_size: attrs.number("Size").filter(|&s| s > 0),
        kind: if streaming {
            STREAMING_KIND.into()
        } else {
            attrs.text("Kind")
        },
        date_added: attrs.text("DateAdded"),
        location: location.filter(|_| !streaming).and_then(location_to_path),
        // Filled from the TRACK's TEMPO children by `finish_grid`.
        beatgrid: BeatGrid::default(),
        // Filled from the TRACK's POSITION_MARK children.
        cues: Vec::new(),
    }
}

/// `Track::kind` of a streaming-only track.
pub const STREAMING_KIND: &str = "Streaming";

fn parse_colour(value: &str) -> Option<TrackColour> {
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(TrackColour {
        name: format!("#{rgb:06X}"),
        rgb: Some(rgb),
    })
}

/// `file://localhost/Users/a%20b/x.mp3` -> `/Users/a b/x.mp3`;
/// `file://localhost/C:/Music/x.mp3` -> `C:/Music/x.mp3`.
fn location_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri
        .strip_prefix("file://localhost")
        .or_else(|| uri.strip_prefix("file://"))?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?;
    let bytes = decoded.as_bytes();
    let path = if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        &decoded[1..]
    } else {
        &decoded[..]
    };
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="7.2.19" Company="AlphaTheta"/>
  <COLLECTION Entries="3">
    <TRACK TrackID="11" Name="Strings &amp; Life" Artist="Rhythim Is Rhythim" Album="Innovator"
      Genre="Techno" Kind="MP3 File" Size="9876543" TotalTime="402" Year="1987" AverageBpm="125.00"
      DateAdded="2024-03-01" BitRate="320" SampleRate="44100" Comments="classic" PlayCount="7"
      Rating="204" Location="file://localhost/Users/dj/Music/Strings%20of%20Life.mp3"
      Remixer="" Tonality="8A" Label="Transmat" Colour="0xFF007F">
      <TEMPO Inizio="0.025" Bpm="125.00" Metro="4/4" Battito="1"/>
      <POSITION_MARK Name="" Type="0" Start="0.025" Num="-1"/>
      <POSITION_MARK Name="1.1Bars" Type="0" Start="0.025" Num="0" Red="255" Green="55" Blue="111"/>
      <POSITION_MARK Name="Drop" Type="4" Start="64.025" End="71.705" Num="2" Red="40" Green="226" Blue="20"/>
      <POSITION_MARK Name="" Type="3" Start="0.000" Num="-1"/>
    </TRACK>
    <TRACK TrackID="12" Name="Windowlicker" Artist="Aphex Twin" TotalTime="366" AverageBpm="127.30"
      Location="file://localhost/C:/Music/windowlicker.flac" Rating="0"/>
    <TRACK TrackID="13" Name="No Tempo" Artist="Unknown" AverageBpm="0.00" Rating="255"
      Location="file://localhost/Users/dj/Music/ambient.wav"/>
    <TRACK TrackID="14" Name="Streamed" Kind="Unknown Format" Size="0"
      Location="file://localhost/v4/catalog/tracks/27134064/"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="2">
      <NODE Name="Warm Up" Type="1" KeyType="0" Entries="2">
        <TRACK Key="12"/>
        <TRACK Key="11"/>
      </NODE>
      <NODE Type="0" Name="Gigs" Count="2">
        <NODE Name="By Location" Type="1" KeyType="1" Entries="2">
          <TRACK Key="file://localhost/Users/dj/Music/ambient.wav"/>
          <TRACK Key="file://localhost/Users/dj/Music/missing.mp3"/>
        </NODE>
        <NODE Name="Empty" Type="1" KeyType="0" Entries="0"/>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>
"#;

    #[test]
    fn reads_collection_fields() {
        let library = parse(SAMPLE).unwrap();
        assert_eq!(library.tracks().len(), 4);
        let t = library.track(&TrackId("11".into())).unwrap();
        assert_eq!(t.title, "Strings & Life");
        assert_eq!(t.bpm, Some(Bpm::from_centi(12500)));
        assert_eq!(t.duration, Some(Duration::from_secs(402)));
        assert_eq!(t.rating.stars(), 4);
        assert_eq!(t.key, "8A");
        assert_eq!(t.colour.as_ref().and_then(|c| c.rgb), Some(0xFF007F));
        assert_eq!(
            t.location.as_deref(),
            Some(Path::new("/Users/dj/Music/Strings of Life.mp3"))
        );
        assert_eq!(t.year, Some(1987));
        assert_eq!(t.file_size, Some(9876543));
        let marker = t.beatgrid.markers()[0];
        assert_eq!(
            (marker.time, marker.bpm, marker.beat_in_bar),
            (0.025, 125.0, 1)
        );

        // A memory cue, hot cue A and a looping hot cue C; the load
        // marker (Type 3) is skipped.
        assert_eq!(t.cues.len(), 3);
        assert_eq!((t.cues[0].hot_cue, t.cues[0].start), (None, 0.025));
        assert_eq!(t.cues[1].slot_letter(), Some('A'));
        assert_eq!(
            (t.cues[1].name.as_str(), t.cues[1].colour),
            ("1.1Bars", Some(0xFF376F))
        );
        assert_eq!(t.cues[2].slot_letter(), Some('C'));
        assert_eq!(t.cues[2].loop_end, Some(71.705));

        let streamed = library.track(&TrackId("14".into())).unwrap();
        assert_eq!(
            (streamed.location.as_ref(), streamed.kind.as_str()),
            (None, STREAMING_KIND)
        );

        let windows = library.track(&TrackId("12".into())).unwrap();
        assert_eq!(
            windows.location.as_deref(),
            Some(Path::new("C:/Music/windowlicker.flac"))
        );
        let untimed = library.track(&TrackId("13".into())).unwrap();
        assert_eq!(untimed.bpm, None);
        assert_eq!(untimed.rating.stars(), 5);
        assert!(windows.beatgrid.is_empty() && untimed.beatgrid.is_empty());
    }

    #[test]
    fn builds_playlist_tree_under_root() {
        let library = parse(SAMPLE).unwrap();
        let names: Vec<_> = library
            .playlists()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, ["Warm Up", "Gigs"]);

        let warm_up = &library.playlists()[0];
        let order: Vec<_> = library
            .playlist_tracks(warm_up)
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(order, ["Windowlicker", "Strings & Life"]);

        let gigs = &library.playlists()[1];
        assert_eq!(gigs.kind, PlaylistKind::Folder);
        assert_eq!(gigs.children.len(), 2);
        // Location keys resolve; the missing file is dropped.
        let by_location = &gigs.children[0];
        assert_eq!(by_location.tracks, [TrackId("13".into())]);
        assert!(gigs.children[1].tracks.is_empty());
        assert!(library.find_playlist(&by_location.id).is_some());
    }

    #[test]
    fn rejects_truncated_or_foreign_documents() {
        // Cut inside the playlist tree, then between collection tracks.
        for cut in ["<TRACK Key=\"11\"/>", "<TRACK TrackID=\"12\""] {
            let end = SAMPLE.find(cut).unwrap() + cut.len();
            let truncated = SAMPLE[..end].rsplit_once('\n').unwrap().0;
            assert!(
                matches!(parse(truncated), Err(Error::Xml(_))),
                "accepted cut at {cut}"
            );
        }
        assert!(matches!(parse(""), Err(Error::Xml(_))));
        assert!(matches!(parse("<playlist/>"), Err(Error::Xml(_))));
    }

    #[test]
    fn decodes_utf16_with_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(SAMPLE.encode_utf16().flat_map(u16::to_le_bytes));
        let text = decode_text(&bytes).unwrap();
        // The declaration still says UTF-8; quick-xml reads the decoded str.
        assert_eq!(parse(&text).unwrap().tracks().len(), 4);
    }
}
