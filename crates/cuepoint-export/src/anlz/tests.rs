//! Checks the written files against the documented layout with a small
//! independent reader. (rekordcrate parses ANLZ through binrw's `BinRead`
//! trait, which this crate cannot name without a direct `binrw`
//! dev-dependency.)

use super::waveform::{DETAIL_ENTRIES_PER_SECOND, Peaks, mono_entry};
use super::*;
use cuepoint_analysis::ColourWaveform;

const DEVICE_PATH: &str = "/Contents/Künstler/Album/Track №1.mp3";
const DURATION: f64 = 30.0;
const BPM: f64 = 128.0;
const FIRST_BEAT: f64 = 0.1;

struct Section<'a> {
    tag: [u8; 4],
    header: &'a [u8],
    content: &'a [u8],
}

impl Section<'_> {
    fn tag(&self) -> &str {
        std::str::from_utf8(&self.tag).unwrap()
    }

    fn header_word(&self, index: usize) -> u32 {
        be32(&self.header[4 * index..])
    }
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes[..4].try_into().unwrap())
}

fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes[..2].try_into().unwrap())
}

/// Splits a file into sections, checking the `PMAI` header and every
/// section envelope.
fn sections(file: &[u8]) -> Vec<Section<'_>> {
    assert_eq!(&file[..4], b"PMAI");
    let len_header = be32(&file[4..]) as usize;
    assert_eq!(len_header, 0x1c);
    assert_eq!(be32(&file[8..]) as usize, file.len(), "len_file");
    let mut out = Vec::new();
    let mut at = len_header;
    while at < file.len() {
        let len_header = be32(&file[at + 4..]) as usize;
        let len_tag = be32(&file[at + 8..]) as usize;
        assert!(12 <= len_header && len_header <= len_tag);
        assert!(at + len_tag <= file.len(), "section overruns the file");
        out.push(Section {
            tag: file[at..at + 4].try_into().unwrap(),
            header: &file[at + 12..at + len_header],
            content: &file[at + len_header..at + len_tag],
        });
        at += len_tag;
    }
    out
}

fn find<'a>(sections: &'a [Section<'a>], tag: &str) -> &'a Section<'a> {
    sections.iter().find(|s| s.tag() == tag).unwrap()
}

fn synthetic_waveform() -> ColourWaveform {
    let len = (DURATION * 150.0) as usize;
    let ramp = |i: usize, step: usize| ((i * step) % 256) as u8;
    ColourWaveform {
        points_per_second: 150,
        low: (0..len).map(|i| ramp(i, 1)).collect(),
        mid: (0..len).map(|i| ramp(i, 3)).collect(),
        high: (0..len).map(|i| ramp(i, 7)).collect(),
    }
}

fn with_input<T>(f: impl FnOnce(&AnalysisInput) -> T) -> T {
    let grid = BeatGrid::constant(BPM, FIRST_BEAT, 1);
    let waveform = synthetic_waveform();
    f(&AnalysisInput {
        device_path: DEVICE_PATH,
        grid: &grid,
        duration_secs: DURATION,
        waveform: &waveform,
    })
}

fn path_of(section: &Section) -> String {
    assert_eq!(section.header_word(0) as usize, section.content.len());
    let units: Vec<u16> = section.content.chunks_exact(2).map(be16).collect();
    assert_eq!(units.last(), Some(&0), "trailing NUL");
    String::from_utf16(&units[..units.len() - 1]).unwrap()
}

/// Entry bytes, entry count and data of a `PWV3`/`PWV4`/`PWV5` section.
fn entries<'a>(section: &'a Section<'a>, entry_bytes: u32) -> (usize, &'a [u8]) {
    assert_eq!(section.header.len(), 12);
    assert_eq!(section.header_word(0), entry_bytes);
    let count = section.header_word(1) as usize;
    assert_eq!(section.content.len(), count * entry_bytes as usize);
    (count, section.content)
}

fn assert_empty_cue_lists(sections: &[Section], tag: &str) {
    let lists: Vec<&Section> = sections.iter().filter(|s| s.tag() == tag).collect();
    let types: Vec<u32> = lists.iter().map(|s| s.header_word(0)).collect();
    assert_eq!(types, [HOT_CUES, MEMORY_CUES], "{tag} list types");
    for list in lists {
        let count_at = if tag == "PCOB" { 6 } else { 4 };
        assert_eq!(be16(&list.header[count_at..]), 0, "{tag} cue count");
        assert!(list.content.is_empty());
    }
}

#[test]
fn dat_holds_documented_sections() {
    let file = with_input(write_dat);
    let sections = sections(&file);
    let tags: Vec<&str> = sections.iter().map(Section::tag).collect();
    assert_eq!(
        tags,
        ["PPTH", "PVBR", "PQTZ", "PWAV", "PWV2", "PCOB", "PCOB"]
    );
    assert_eq!(path_of(find(&sections, "PPTH")), DEVICE_PATH);

    let vbr = find(&sections, "PVBR");
    assert_eq!(vbr.header.len() + vbr.content.len(), 4 + 4 * 401);

    let preview = find(&sections, "PWAV");
    assert_eq!(preview.header_word(0), 400);
    assert_eq!(preview.content.len(), 400);
    let tiny = find(&sections, "PWV2");
    assert_eq!(tiny.header_word(0), 100);
    assert_eq!(tiny.content.len(), 100);
    assert!(tiny.content.iter().all(|&b| b <= 15), "PWV2 uses 4 bits");

    assert_empty_cue_lists(&sections, "PCOB");
}

#[test]
fn beat_grid_matches_grid() {
    let file = with_input(write_dat);
    let sections = sections(&file);
    let grid = find(&sections, "PQTZ");
    assert_eq!(grid.header.len(), 12);
    let count = grid.header_word(2) as usize;
    assert_eq!(grid.content.len(), 8 * count);

    // Beats at 0.1 s + k × 60/128 s up to 30 s: k = 0..=63.
    let beat_len = 60.0 / BPM;
    let expected = ((DURATION - FIRST_BEAT) / beat_len).floor() as usize + 1;
    assert_eq!(count, expected);
    assert_eq!(count, 64);
    for (k, beat) in grid.content.chunks_exact(8).enumerate() {
        assert_eq!(be16(beat), (k % 4) as u16 + 1, "beat {k} in bar");
        assert_eq!(be16(&beat[2..]), 12_800, "beat {k} tempo");
        let ms = ((FIRST_BEAT + k as f64 * beat_len) * 1000.0).round() as u32;
        assert_eq!(be32(&beat[4..]), ms, "beat {k} time");
    }
}

#[test]
fn ext_holds_documented_sections() {
    let file = with_input(write_ext);
    let sections = sections(&file);
    let tags: Vec<&str> = sections.iter().map(Section::tag).collect();
    assert_eq!(
        tags,
        [
            "PPTH", "PWV3", "PCOB", "PCOB", "PCO2", "PCO2", "PWV5", "PWV4"
        ]
    );
    assert_eq!(path_of(find(&sections, "PPTH")), DEVICE_PATH);

    let detail_len = (DURATION * f64::from(DETAIL_ENTRIES_PER_SECOND)).ceil() as usize;
    let (count, _) = entries(find(&sections, "PWV3"), 1);
    assert_eq!(count, detail_len);
    let (count, _) = entries(find(&sections, "PWV5"), 2);
    assert_eq!(count, detail_len);
    let (count, _) = entries(find(&sections, "PWV4"), 6);
    assert_eq!(count, 1200);

    assert_empty_cue_lists(&sections, "PCOB");
    assert_empty_cue_lists(&sections, "PCO2");
}

#[test]
fn waveforms_follow_the_input() {
    let waveform = synthetic_waveform();
    let peaks = |i: usize| Peaks {
        low: waveform.low[i],
        mid: waveform.mid[i],
        high: waveform.high[i],
    };
    let ext = with_input(write_ext);
    let ext_sections = sections(&ext);

    // PWV3 is one PWAV-style byte per input point.
    let (_, detail) = entries(find(&ext_sections, "PWV3"), 1);
    for (i, &byte) in detail.iter().enumerate() {
        assert_eq!(byte, mono_entry(peaks(i)));
    }

    // PWV5 heights track the loudest band, and colour sits in bits 15 to 7.
    let (_, colour) = entries(find(&ext_sections, "PWV5"), 2);
    for (i, entry) in colour.chunks_exact(2).enumerate() {
        let packed = be16(entry);
        assert_eq!(packed & 0b11, 0, "unused low bits");
        let loudest = u32::from(peaks(i).loudest());
        assert_eq!(u32::from((packed >> 2) & 0x1F), (loudest * 31 + 127) / 255);
    }

    // Each PWAV column is the tallest of the 11.25 points it covers.
    let dat = with_input(write_dat);
    let dat_sections = sections(&dat);
    let preview = find(&dat_sections, "PWAV").content;
    for (column, &byte) in preview.iter().enumerate() {
        let span = column * detail.len() / 400..(column + 1) * detail.len() / 400;
        let tallest = detail[span].iter().map(|b| b & 0x1F).max().unwrap();
        assert_eq!(byte & 0x1F, tallest, "column {column}");
    }
}

#[test]
fn empty_input_still_writes_valid_files() {
    let grid = BeatGrid::default();
    let waveform = ColourWaveform::default();
    let input = AnalysisInput {
        device_path: "",
        grid: &grid,
        duration_secs: 0.0,
        waveform: &waveform,
    };
    let dat = write_dat(&input);
    let dat_sections = sections(&dat);
    assert_eq!(find(&dat_sections, "PQTZ").header_word(2), 0);
    assert_eq!(find(&dat_sections, "PWAV").content, [0; 400]);
    assert_eq!(path_of(find(&dat_sections, "PPTH")), "");
    let ext = write_ext(&input);
    let ext_sections = sections(&ext);
    assert_eq!(entries(find(&ext_sections, "PWV5"), 2).0, 0);
}

#[test]
fn analysis_path_is_stable_and_distinct() {
    let a = analysis_path("/Contents/A/One.mp3");
    assert_eq!(a, analysis_path("/Contents/A/One.mp3"));
    assert_ne!(a, analysis_path("/Contents/A/Two.mp3"));
    assert_ne!(a, analysis_path("/Contents/a/One.mp3"));
    // `/PIONEER/USBANLZ/` + `Pxxx` + `/` + 8 hex digits + `/ANLZ0000.DAT`.
    let rest = a.strip_prefix("/PIONEER/USBANLZ/P").unwrap();
    let (outer, rest) = rest.split_at(3);
    let (inner, file) = rest[1..].split_at(8);
    assert!(
        outer
            .chars()
            .chain(inner.chars())
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
    );
    assert_eq!(file, "/ANLZ0000.DAT");

    let mut seen = std::collections::HashSet::new();
    for i in 0..10_000 {
        assert!(seen.insert(analysis_path(&format!("/Contents/Track {i}.mp3"))));
    }
}
