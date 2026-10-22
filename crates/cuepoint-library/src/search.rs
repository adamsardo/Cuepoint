use std::cmp::Ordering;

use cuepoint_core::Track;

/// True when every whitespace-separated term of `query` appears, ignoring
/// case, in at least one of the track's text fields. An empty query matches.
pub fn matches_query(track: &Track, query: &str) -> bool {
    let fields = [
        &track.title,
        &track.artist,
        &track.album,
        &track.genre,
        &track.label,
        &track.remixer,
        &track.comment,
        &track.key,
    ];
    query.split_whitespace().all(|term| {
        let term = term.to_lowercase();
        fields
            .iter()
            .any(|field| field.to_lowercase().contains(&term))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortColumn {
    Title,
    Artist,
    Album,
    Genre,
    Bpm,
    Key,
    Duration,
    Rating,
    DateAdded,
}

impl SortColumn {
    pub const ALL: [SortColumn; 9] = [
        SortColumn::Title,
        SortColumn::Artist,
        SortColumn::Album,
        SortColumn::Genre,
        SortColumn::Bpm,
        SortColumn::Key,
        SortColumn::Duration,
        SortColumn::Rating,
        SortColumn::DateAdded,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortColumn::Title => "Title",
            SortColumn::Artist => "Artist",
            SortColumn::Album => "Album",
            SortColumn::Genre => "Genre",
            SortColumn::Bpm => "BPM",
            SortColumn::Key => "Key",
            SortColumn::Duration => "Time",
            SortColumn::Rating => "Rating",
            SortColumn::DateAdded => "Date Added",
        }
    }

    pub fn compare(self, a: &Track, b: &Track) -> Ordering {
        let text = |x: &str, y: &str| x.to_lowercase().cmp(&y.to_lowercase());
        match self {
            SortColumn::Title => text(&a.title, &b.title),
            SortColumn::Artist => text(&a.artist, &b.artist),
            SortColumn::Album => text(&a.album, &b.album),
            SortColumn::Genre => text(&a.genre, &b.genre),
            SortColumn::Bpm => a.bpm.cmp(&b.bpm),
            SortColumn::Key => text(&a.key, &b.key),
            SortColumn::Duration => a.duration.cmp(&b.duration),
            SortColumn::Rating => a.rating.cmp(&b.rating),
            SortColumn::DateAdded => a.date_added.cmp(&b.date_added),
        }
    }
}

/// Stable sort, so equal keys keep their previous (e.g. playlist) order.
pub fn sort_tracks(tracks: &mut [&Track], column: SortColumn, ascending: bool) {
    tracks.sort_by(|a, b| {
        let order = column.compare(a, b);
        if ascending { order } else { order.reverse() }
    });
}

/// [`sort_tracks`] for a list of indices into `tracks`.
pub fn sort_rows(rows: &mut [usize], tracks: &[Track], column: SortColumn, ascending: bool) {
    rows.sort_by(|&a, &b| {
        let order = column.compare(&tracks[a], &tracks[b]);
        if ascending { order } else { order.reverse() }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuepoint_core::{Bpm, TrackId};

    fn track(id: &str, title: &str, artist: &str, bpm: u32) -> Track {
        Track {
            id: TrackId(id.into()),
            title: title.into(),
            artist: artist.into(),
            bpm: Some(Bpm::from_centi(bpm)),
            ..Track::default()
        }
    }

    #[test]
    fn query_needs_every_term_in_some_field() {
        let t = track("1", "Strings of Life", "Rhythim Is Rhythim", 12500);
        assert!(matches_query(&t, ""));
        assert!(matches_query(&t, "strings rhythim"));
        assert!(!matches_query(&t, "strings derrick"));
    }

    #[test]
    fn sorts_by_bpm_both_ways() {
        let (a, b, c) = (
            track("1", "A", "", 12800),
            track("2", "B", "", 9000),
            track("3", "C", "", 17400),
        );
        let mut list = vec![&a, &b, &c];
        sort_tracks(&mut list, SortColumn::Bpm, true);
        assert_eq!(
            list.iter().map(|t| t.id.0.as_str()).collect::<Vec<_>>(),
            ["2", "1", "3"]
        );
        sort_tracks(&mut list, SortColumn::Bpm, false);
        assert_eq!(
            list.iter().map(|t| t.id.0.as_str()).collect::<Vec<_>>(),
            ["3", "1", "2"]
        );
    }
}
