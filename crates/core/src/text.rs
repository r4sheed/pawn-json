// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

use crate::error::Error;
use crate::pool::Cell;
use encoding_rs::{EncoderResult, Encoding, WINDOWS_1252};

/// The code page a script's strings are in: a single-byte encoding, picked by its WHATWG
/// label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codepage(&'static Encoding);

impl Codepage {
    pub const DEFAULT: Codepage = Codepage(WINDOWS_1252);

    pub fn from_label(label: &str) -> Option<Codepage> {
        let e = Encoding::for_label(label.trim().as_bytes())?;
        e.is_single_byte().then_some(Codepage(e))
    }

    pub fn name(&self) -> &'static str {
        self.0.name()
    }
}

impl Default for Codepage {
    fn default() -> Codepage {
        Codepage::DEFAULT
    }
}

/// Pawn packs a string four characters to a cell, the first in the top byte, while an
/// unpacked string has one character per cell. This tells them apart the way
/// `amx_StrLen` does: a packed string's first cell has bits set above the low 24.
fn is_packed(cells: &[Cell]) -> bool {
    cells.first().is_some_and(|&c| c as u32 > 0x00FF_FFFF)
}

/// True for a string with no characters.
pub fn is_empty(cells: &[Cell]) -> bool {
    cells.first().is_none_or(|&c| c == 0)
}

/// Appends the string's raw bytes to `out`, stopping at its terminator or at the end of
/// the slice.
pub fn raw_bytes(cells: &[Cell], out: &mut Vec<u8>) {
    if is_packed(cells) {
        for &c in cells {
            for b in (c as u32).to_be_bytes() {
                if b == 0 {
                    return;
                }
                out.push(b);
            }
        }
    } else {
        let n = cells
            .iter()
            .position(|&c| c as u8 == 0)
            .unwrap_or(cells.len());
        out.extend(cells[..n].iter().map(|&c| c as u8));
    }
}

pub const STACK_KEY: usize = 256;

/// Hands `f` the string as UTF-8. Every key and path a read looks up comes through here,
/// so a string of up to `STACK_KEY` bytes stays on the stack: ASCII is passed as it
/// stands, and anything else is decoded into a second stack buffer. A longer string is
/// decoded on the heap.
pub fn with_text<R>(cells: &[Cell], cp: Codepage, f: impl FnOnce(&str) -> R) -> R {
    let mut raw = [0u8; STACK_KEY];
    let mut n = 0;
    let mut high = 0u8;
    if is_packed(cells) {
        'cells: for &c in cells {
            for b in (c as u32).to_be_bytes() {
                if b == 0 {
                    break 'cells;
                }
                if n == STACK_KEY {
                    return f(&to_string(cells, cp));
                }
                raw[n] = b;
                high |= b;
                n += 1;
            }
        }
    } else {
        for &c in cells {
            let b = c as u8;
            if b == 0 {
                break;
            }
            if n == STACK_KEY {
                return f(&to_string(cells, cp));
            }
            raw[n] = b;
            high |= b;
            n += 1;
        }
    }
    let raw = &raw[..n];
    if high < 0x80 {
        return f(std::str::from_utf8(raw).expect("ASCII"));
    }
    // One byte of a single-byte code page decodes to at most three bytes of UTF-8.
    let mut utf8 = [0u8; STACK_KEY * 3];
    let mut decoder = cp.0.new_decoder_without_bom_handling();
    let (_, _, written, _) = decoder.decode_to_utf8(raw, &mut utf8, true);
    f(std::str::from_utf8(&utf8[..written]).expect("the decoder writes UTF-8"))
}

/// Decodes the string into a `String`, for a value the document will keep.
pub fn to_string(cells: &[Cell], cp: Codepage) -> String {
    let mut raw = Vec::new();
    raw_bytes(cells, &mut raw);
    if raw.is_ascii() {
        return String::from_utf8(raw).expect("ASCII");
    }
    cp.0.decode_without_bom_handling(&raw).0.into_owned()
}

/// Writes `s` into a Pawn buffer as an unpacked, terminated string in the code page. A
/// character the code page has no byte for becomes `?`, not the `&#NNN;` entity
/// encoding_rs would write by default. If the buffer is too short, the string is cut and
/// still terminated, and the call returns `Error::Buffer`.
pub fn put(out: &mut [Cell], s: &str, cp: Codepage) -> Result<(), Error> {
    let Some(room) = out.len().checked_sub(1) else {
        return Err(Error::Buffer);
    };
    let mut n = 0;
    let mut cut = false;
    if s.is_ascii() {
        for b in s.bytes() {
            if n == room {
                cut = true;
                break;
            }
            out[n] = b as Cell;
            n += 1;
        }
    } else {
        let mut encoder = cp.0.new_encoder();
        let mut src = s;
        let mut chunk = [0u8; 256];
        'outer: loop {
            let (result, read, written) =
                encoder.encode_from_utf8_without_replacement(src, &mut chunk, true);
            for &b in &chunk[..written] {
                if n == room {
                    cut = true;
                    break 'outer;
                }
                out[n] = b as Cell;
                n += 1;
            }
            src = &src[read..];
            match result {
                EncoderResult::InputEmpty => break,
                EncoderResult::OutputFull => {}
                EncoderResult::Unmappable(_) => {
                    if n == room {
                        cut = true;
                        break;
                    }
                    out[n] = b'?' as Cell;
                    n += 1;
                }
            }
        }
    }
    out[n] = 0;
    if cut { Err(Error::Buffer) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(bytes: &[u8]) -> Vec<Cell> {
        bytes.iter().map(|&b| b as Cell).chain([0]).collect()
    }

    fn put_bytes(s: &str, cp: Codepage) -> Vec<u8> {
        let mut out = vec![-1; 64];
        put(&mut out, s, cp).unwrap();
        out.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect()
    }

    const HUNGARIAN: &str = "\u{e1}\u{e9}\u{ed}\u{f3}\u{f6}\u{151}\u{fa}\u{fc}\u{171}\u{c1}\u{c9}\u{cd}\u{d3}\u{d6}\u{150}\u{da}\u{dc}\u{170}";

    #[test]
    fn a_character_the_code_page_lacks_arrives_as_a_question_mark() {
        let w1250 = Codepage::from_label("windows-1250").unwrap();
        assert_eq!(put_bytes("o\u{151}o", Codepage::DEFAULT), b"o?o");
        assert_eq!(put_bytes("\u{416}x", w1250), b"?x");
    }

    #[test]
    fn hungarian_round_trips_through_windows_1250() {
        let cp = Codepage::from_label("windows-1250").unwrap();
        let bytes = put_bytes(HUNGARIAN, cp);
        assert_eq!(
            bytes,
            [
                0xE1, 0xE9, 0xED, 0xF3, 0xF6, 0xF5, 0xFA, 0xFC, 0xFB, 0xC1, 0xC9, 0xCD, 0xD3, 0xD6,
                0xD5, 0xDA, 0xDC, 0xDB
            ]
        );
        assert_eq!(with_text(&cells(&bytes), cp, |s| s.to_owned()), HUNGARIAN);
        assert_eq!(to_string(&cells(&bytes), cp), HUNGARIAN);
    }

    #[test]
    fn no_output_holds_an_entity() {
        let sample = "a\u{416}\u{4e2d}\u{1f600}\u{151}\u{20ac}\u{2014}z";
        for label in ["windows-1252", "windows-1250", "windows-1251", "iso-8859-2"] {
            let out = put_bytes(sample, Codepage::from_label(label).unwrap());
            assert!(
                !out.contains(&b'&') && !out.contains(&b'#'),
                "{label}: {out:?}"
            );
            assert_eq!(out.first(), Some(&b'a'));
            assert_eq!(out.last(), Some(&b'z'));
        }
    }

    #[test]
    fn a_short_buffer_gets_the_cut_string_and_an_error() {
        let mut out = [-1; 4];
        assert_eq!(
            put(&mut out, "abcdef", Codepage::DEFAULT),
            Err(Error::Buffer)
        );
        assert_eq!(out, [b'a' as Cell, b'b' as Cell, b'c' as Cell, 0]);
        let mut out = [-1; 3];
        assert_eq!(
            put(&mut out, "\u{e1}\u{e9}\u{ed}", Codepage::DEFAULT),
            Err(Error::Buffer)
        );
        assert_eq!(out, [0xE1, 0xE9, 0]);
        let mut out = [-1; 4];
        assert_eq!(put(&mut out, "abc", Codepage::DEFAULT), Ok(()));
        assert_eq!(put(&mut [], "", Codepage::DEFAULT), Err(Error::Buffer));
    }

    #[test]
    fn a_packed_string_reads_as_an_unpacked_one() {
        let packed = [
            i32::from_be_bytes(*b"hell"),
            i32::from_be_bytes([b'o', 0xE1, 0, 0]),
        ];
        let unpacked = cells(b"hello\xE1");
        let a = with_text(&packed, Codepage::DEFAULT, |s| s.to_owned());
        let b = with_text(&unpacked, Codepage::DEFAULT, |s| s.to_owned());
        assert_eq!(a, "hello\u{e1}");
        assert_eq!(a, b);
    }

    #[test]
    fn a_key_past_the_stack_buffer_still_reads() {
        let long = "k".repeat(STACK_KEY + 10);
        assert_eq!(
            with_text(&cells(long.as_bytes()), Codepage::DEFAULT, |s| s.len()),
            long.len()
        );
    }

    #[test]
    fn only_single_byte_code_pages_are_taken() {
        assert!(Codepage::from_label("utf-8").is_none());
        assert!(Codepage::from_label("nonsense").is_none());
        assert_eq!(
            Codepage::from_label(" Windows-1250 ").unwrap().name(),
            "windows-1250"
        );
    }
}
