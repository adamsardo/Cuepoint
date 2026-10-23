//! File and page layout ("File Header" and "Table Pages" in the Deep
//! Symmetry export analysis).
//!
//! Page 0 holds the file header. Each table is a linked list of pages: an
//! index page first, then data pages. A data page starts with a 0x28-byte
//! header, rows fill its heap upwards from 0x28, and the row index grows
//! down from the end of the page in groups of up to 16 row offsets.

use std::ops::Range;

use super::row::Row;

pub(super) const PAGE_SIZE: usize = 4096;
/// Common page header (0x20 bytes) plus the data page header (8 bytes).
const DATA_HEADER: usize = 0x28;
const HEAP_SIZE: usize = PAGE_SIZE - DATA_HEADER;
const ROWS_PER_GROUP: usize = 16;
/// 16 row offsets, the presence flags and the transaction flags.
const GROUP_SIZE: usize = 0x24;
/// Rows a page may count: `num_rows` is an 11-bit field.
const MAX_ROWS_PER_PAGE: usize = 0x7ff;

const FLAGS_DATA: u8 = 0x24;
const FLAGS_INDEX: u8 = 0x64;

/// Empty slots in an index page. Index entries run from 0x3c to 20 bytes
/// short of the page end; 0x3ec is also the magic value at 0x24.
const INDEX_ENTRIES: usize = 0x3ec;
const INDEX_EMPTY_ENTRY: u32 = 0x1fff_fff8;
/// "None" in index page fields that hold a page index.
const INDEX_NO_PAGE: u32 = 0x03ff_ffff;

/// A table to write: its page type and rows in order.
pub(super) struct Table {
    pub kind: u32,
    pub rows: Vec<Row>,
}

/// Where a table's pages go.
struct Placement {
    index_page: u32,
    /// Page number and the rows it holds.
    data_pages: Vec<(u32, Range<usize>)>,
    /// Where the table grows next. rekordbox reserves a blank page inside
    /// the file for an empty table and points past the end of the file for
    /// one with rows; we do the same.
    empty_candidate: u32,
}

/// Lays out and serialises the whole file.
pub(super) fn write_file(tables: &[Table]) -> Vec<u8> {
    let mut next_page = 1;
    let mut take_page = || {
        let page = next_page;
        next_page += 1;
        page
    };
    let mut placements: Vec<Placement> = tables
        .iter()
        .map(|table| {
            let index_page = take_page();
            let data_pages: Vec<_> = paginate(&table.rows)
                .into_iter()
                .map(|rows| (take_page(), rows))
                .collect();
            let empty_candidate = if data_pages.is_empty() {
                take_page()
            } else {
                0
            };
            Placement {
                index_page,
                data_pages,
                empty_candidate,
            }
        })
        .collect();
    let file_pages = next_page;
    let mut past_end = file_pages;
    for placement in placements.iter_mut().filter(|p| !p.data_pages.is_empty()) {
        placement.empty_candidate = past_end;
        past_end += 1;
    }

    let mut file = vec![0; page_offset(file_pages)];
    let mut sequence = 1;
    for (table, placement) in tables.iter().zip(&placements) {
        let first_data = placement.data_pages.first().map(|&(page, _)| page);
        write_index_page(
            page_mut(&mut file, placement.index_page),
            placement.index_page,
            table.kind,
            first_data.unwrap_or(placement.empty_candidate),
            first_data,
        );
        for (i, (page, rows)) in placement.data_pages.iter().enumerate() {
            sequence += 1;
            let next = placement
                .data_pages
                .get(i + 1)
                .map_or(placement.empty_candidate, |&(page, _)| page);
            let header = PageHeader {
                index: *page,
                kind: table.kind,
                next,
                sequence,
            };
            write_data_page(
                page_mut(&mut file, *page),
                &header,
                &table.rows[rows.clone()],
            );
        }
    }

    // File header.
    let header = &mut file[..PAGE_SIZE];
    put_u32(header, 0x04, PAGE_SIZE as u32);
    put_u32(
        header,
        0x08,
        u32::try_from(tables.len()).expect("20 tables"),
    );
    // next_unused_page: past every page in use, empty candidates included.
    put_u32(header, 0x0c, past_end);
    // Unknown; 5 in every rekordbox export seen.
    put_u32(header, 0x10, 5);
    // The next sequence number, one past the last page written.
    put_u32(header, 0x14, sequence + 1);
    for (i, (table, placement)) in tables.iter().zip(&placements).enumerate() {
        let at = 0x1c + 16 * i;
        let last = placement
            .data_pages
            .last()
            .map_or(placement.index_page, |&(page, _)| page);
        put_u32(header, at, table.kind);
        put_u32(header, at + 4, placement.empty_candidate);
        put_u32(header, at + 8, placement.index_page);
        put_u32(header, at + 12, last);
    }
    file
}

/// Bytes of row index for `rows` rows: two per offset and four per group
/// (presence and transaction flags). An unfilled last group only uses the
/// slots nearest the end of the page, so this is also what rekordbox
/// subtracts when it reports `free_size`.
fn row_index_size(rows: usize) -> usize {
    2 * rows + 4 * rows.div_ceil(ROWS_PER_GROUP)
}

/// Splits rows into pages, filling each page until the next row and its
/// index slot would no longer fit between the heap and the row index.
fn paginate(rows: &[Row]) -> Vec<Range<usize>> {
    let mut pages = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (i, row) in rows.iter().enumerate() {
        assert!(
            row.bytes.len() + row_index_size(1) <= HEAP_SIZE,
            "a row must fit in an empty page"
        );
        let count = i - start + 1;
        let fits = used + row.bytes.len() + row_index_size(count) <= HEAP_SIZE
            && count <= MAX_ROWS_PER_PAGE;
        if !fits {
            pages.push(start..i);
            start = i;
            used = 0;
        }
        used += row.bytes.len();
    }
    if start < rows.len() {
        pages.push(start..rows.len());
    }
    pages
}

struct PageHeader {
    index: u32,
    kind: u32,
    next: u32,
    sequence: u32,
}

/// The common page header: zero, page index, type, next page, sequence and
/// an unknown zero `u32`, then fields the callers fill.
fn write_common_header(page: &mut [u8], header: &PageHeader) {
    put_u32(page, 0x04, header.index);
    put_u32(page, 0x08, header.kind);
    put_u32(page, 0x0c, header.next);
    put_u32(page, 0x10, header.sequence);
}

/// An index page as rekordbox writes it for a table with no deleted rows:
/// no rows, flags 0x64 and an index header whose entries are all empty.
fn write_index_page(page: &mut [u8], index: u32, kind: u32, next: u32, first_data: Option<u32>) {
    let header = PageHeader {
        index,
        kind,
        next,
        // Index pages are the first written; data pages count on from here.
        sequence: 1,
    };
    write_common_header(page, &header);
    page[0x1b] = FLAGS_INDEX;
    // Index header: two unknowns (0x1fff when the index is empty), the
    // magic 0x03ec, next_offset 0, the page index again, the first data
    // page (or none), another 0x03ffffff and a zero.
    put_u16(page, 0x20, 0x1fff);
    put_u16(page, 0x22, 0x1fff);
    put_u16(page, 0x24, INDEX_ENTRIES as u16);
    put_u16(page, 0x26, 0);
    put_u32(page, 0x28, index);
    put_u32(page, 0x2c, first_data.unwrap_or(INDEX_NO_PAGE));
    put_u32(page, 0x30, INDEX_NO_PAGE);
    put_u32(page, 0x34, 0);
    // num_entries 0 and first_empty "none".
    put_u16(page, 0x38, 0);
    put_u16(page, 0x3a, 0x1fff);
    for slot in 0..INDEX_ENTRIES {
        put_u32(page, 0x3c + 4 * slot, INDEX_EMPTY_ENTRY);
    }
}

fn write_data_page(page: &mut [u8], header: &PageHeader, rows: &[Row]) {
    write_common_header(page, header);
    let count = rows.len();
    let mut used = 0;
    for (i, row) in rows.iter().enumerate() {
        let at = DATA_HEADER + used;
        page[at..at + row.bytes.len()].copy_from_slice(&row.bytes);
        if row.has_index_shift {
            put_u16(
                page,
                at + 2,
                u16::try_from(i * 0x20).expect("at most 2047 rows"),
            );
        }
        // Offsets count from the start of the heap. Group g ends 0x24 * g
        // bytes before the page end: transaction flags in its last two
        // bytes, presence flags before them, then offsets 0 to 15 going
        // backwards.
        let (group, slot) = (i / ROWS_PER_GROUP, i % ROWS_PER_GROUP);
        let group_end = PAGE_SIZE - GROUP_SIZE * group;
        put_u16(page, group_end - 6 - 2 * slot, used as u16);
        used += row.bytes.len();
    }
    for group in 0..count.div_ceil(ROWS_PER_GROUP) {
        let group_end = PAGE_SIZE - GROUP_SIZE * group;
        let in_group = (count - ROWS_PER_GROUP * group).min(ROWS_PER_GROUP);
        let present = ((1u32 << in_group) - 1) as u16;
        // rekordbox adds rows one transaction at a time, so the last row is
        // the one the last transaction touched.
        let touched = if group == (count - 1) / ROWS_PER_GROUP {
            1 << ((count - 1) % ROWS_PER_GROUP)
        } else {
            0
        };
        put_u16(page, group_end - 4, present);
        put_u16(page, group_end - 2, touched);
    }

    // Bytes 0x18..0x1b pack num_row_offsets (13 bits) and num_rows
    // (11 bits), least significant first. Nothing is deleted, so both are
    // the row count.
    let counts = (count | count << 13) as u32;
    page[0x18..0x1b].copy_from_slice(&counts.to_le_bytes()[..3]);
    page[0x1b] = FLAGS_DATA;
    put_u16(
        page,
        0x1c,
        (HEAP_SIZE - used - row_index_size(count)) as u16,
    );
    put_u16(page, 0x1e, used as u16);
    // transaction_row_count and transaction_row_index: one row, the last.
    put_u16(page, 0x20, 1);
    put_u16(page, 0x22, (count - 1) as u16);
}

fn page_offset(page: u32) -> usize {
    page as usize * PAGE_SIZE
}

fn page_mut(file: &mut [u8], page: u32) -> &mut [u8] {
    let start = page_offset(page);
    &mut file[start..start + PAGE_SIZE]
}

fn put_u16(buf: &mut [u8], at: usize, value: u16) {
    buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
