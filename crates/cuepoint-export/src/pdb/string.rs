//! DeviceSQL strings ("DeviceSQL Strings" in the Deep Symmetry export
//! analysis, `device_sql_string` in crate-digger's `rekordbox_pdb.ksy`).
//!
//! The first byte says how the rest is stored:
//!
//! - odd: a short ASCII string; the byte is `(len + 1) * 2 + 1`, so it holds
//!   at most 126 bytes of text and the empty string is the single byte `03`;
//! - `0x40`: long ASCII, then a `u16` length covering the 4-byte header and
//!   the text, then a zero pad byte;
//! - `0x90`: UTF-16LE with the same 4-byte header, the length counting bytes.

/// Most text a short ASCII string holds: its length byte keeps
/// `len + 1` in 7 bits and the low bit flags the short form.
const MAX_SHORT_ASCII: usize = 126;
const LONG_ASCII: u8 = 0x40;
const LONG_UTF16LE: u8 = 0x90;

/// Longest string we store, in UTF-16 code units. Longer text is cut at a
/// character boundary. It bounds every row well inside one page (a track row
/// with all its strings at this limit is under 3 KiB, a page heap is 4056
/// bytes), and no player path or display field is longer anyway.
pub(super) const MAX_UNITS: usize = 255;

/// The part of `text` we store.
pub(super) fn clamp(text: &str) -> &str {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        units += c.len_utf16();
        if units > MAX_UNITS {
            return &text[..at];
        }
    }
    text
}

/// Whether `text` is stored in one of the long forms. rekordbox starts those
/// on a 4-byte boundary within the row (every `0x40` and `0x90` string in
/// real exports is aligned so), while short strings follow on directly.
pub(super) fn is_long(text: &str) -> bool {
    let text = clamp(text);
    !text.is_ascii() || text.len() > MAX_SHORT_ASCII
}

/// Appends `text`, clamped to [`MAX_UNITS`], in the form rekordbox uses.
pub(super) fn encode(text: &str, out: &mut Vec<u8>) {
    let text = clamp(text);
    if !text.is_ascii() {
        let units: Vec<u16> = text.encode_utf16().collect();
        long_header(out, LONG_UTF16LE, units.len() * 2);
        out.extend(units.iter().flat_map(|unit| unit.to_le_bytes()));
    } else if text.len() > MAX_SHORT_ASCII {
        long_header(out, LONG_ASCII, text.len());
        out.extend_from_slice(text.as_bytes());
    } else {
        let len = u8::try_from(text.len()).expect("short strings hold at most 126 bytes");
        out.push(((len + 1) << 1) | 1);
        out.extend_from_slice(text.as_bytes());
    }
}

fn long_header(out: &mut Vec<u8>, kind: u8, body_len: usize) {
    let len = u16::try_from(body_len + 4).expect("clamped strings are far below 64 KiB");
    out.push(kind);
    out.extend_from_slice(&len.to_le_bytes());
    out.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        encode(text, &mut out);
        out
    }

    #[test]
    fn short_ascii() {
        assert_eq!(encoded(""), [0x03]);
        // As in the colour table rekordbox writes.
        assert_eq!(encoded("Pink"), [0x0b, b'P', b'i', b'n', b'k']);
        let longest = "a".repeat(126);
        assert_eq!(encoded(&longest)[0], 0xff);
        assert!(!is_long(&longest));
    }

    #[test]
    fn long_ascii() {
        let text = "b".repeat(127);
        let bytes = encoded(&text);
        assert_eq!(bytes[..4], [0x40, 131, 0, 0]);
        assert_eq!(&bytes[4..], text.as_bytes());
        assert!(is_long(&text));
    }

    #[test]
    fn utf16() {
        let bytes = encoded("Café");
        assert_eq!(bytes[..4], [0x90, 12, 0, 0]);
        assert_eq!(&bytes[4..], [b'C', 0, b'a', 0, b'f', 0, 0xe9, 0]);
        // A character outside the BMP takes a surrogate pair.
        assert_eq!(encoded("\u{1f3b5}")[..4], [0x90, 8, 0, 0]);
    }

    #[test]
    fn clamps_at_character_boundaries() {
        let ascii = "x".repeat(MAX_UNITS + 10);
        assert_eq!(clamp(&ascii).len(), MAX_UNITS);
        let pairs = "\u{1f3b5}".repeat(200);
        let kept = clamp(&pairs);
        assert_eq!(kept.chars().count(), MAX_UNITS / 2);
        assert_eq!(clamp("Café Δ"), "Café Δ");
    }
}
