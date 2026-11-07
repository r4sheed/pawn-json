// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! RFC 6901 JSON Pointers, such as `/vehicles/3/model`. Each `/` starts a token, in which
//! `~1` stands for `/` and `~0` for `~`. An array index is decimal with no leading zero,
//! and `-`, which names the element past the end, is refused since there is nothing there
//! to read. A token is used where it lies, or unescaped into a stack buffer if it has a
//! `~` in it; only an escaped token over 256 bytes goes to the heap.

use crate::dom::{Doc, NodeId, Val};
use crate::error::Error;

const STACK_TOKEN: usize = 256;

/// Follows `pointer` from `start` and returns the node it names.
pub fn walk(doc: &Doc, start: NodeId, pointer: &str) -> Result<NodeId, Error> {
    if pointer.is_empty() {
        return Ok(start);
    }
    let rest = pointer.strip_prefix('/').ok_or(Error::Path)?;
    let mut at = start;
    for raw in rest.split('/') {
        at = if raw.contains('~') {
            let mut buf = [0u8; STACK_TOKEN];
            match unescape(raw, &mut buf) {
                Some(token) => step(doc, at, token)?,
                None if raw.len() > STACK_TOKEN => step(doc, at, &unescape_owned(raw)?)?,
                None => return Err(Error::Path),
            }
        } else {
            step(doc, at, raw)?
        };
    }
    Ok(at)
}

fn step(doc: &Doc, at: NodeId, token: &str) -> Result<NodeId, Error> {
    match doc.val(at) {
        Val::Obj(m) => m.get(token).copied().ok_or(Error::Key),
        Val::Arr(v) => {
            let i = index(token)?;
            v.get(i).copied().ok_or(Error::Key)
        }
        _ => Err(Error::Container),
    }
}

fn index(token: &str) -> Result<usize, Error> {
    let digits = !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit());
    if !digits || (token.len() > 1 && token.starts_with('0')) {
        return Err(Error::Path);
    }
    token.parse().map_err(|_| Error::Key)
}

/// Unescapes the token into `buf`. Returns None for a bad escape or for a token too long
/// for `buf`; the caller tells the two apart by the token's length.
fn unescape<'b>(raw: &str, buf: &'b mut [u8; STACK_TOKEN]) -> Option<&'b str> {
    let mut n = 0;
    let mut bytes = raw.bytes();
    while let Some(b) = bytes.next() {
        let out = if b == b'~' {
            match bytes.next() {
                Some(b'0') => b'~',
                Some(b'1') => b'/',
                _ => return None,
            }
        } else {
            b
        };
        *buf.get_mut(n)? = out;
        n += 1;
    }
    std::str::from_utf8(&buf[..n]).ok()
}

fn unescape_owned(raw: &str) -> Result<String, Error> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        out.push(match c {
            '~' => match chars.next() {
                Some('0') => '~',
                Some('1') => '/',
                _ => return Err(Error::Path),
            },
            c => c,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The example document from RFC 6901, section 5.
    const RFC: &str = r#"{"foo":["bar","baz"],"":0,"a/b":1,"c%d":2,"e^f":3,"g|h":4,"i\\j":5,"k\"l":6," ":7,"m~n":8}"#;

    fn at<'d>(doc: &'d Doc, pointer: &str) -> Result<&'d Val, Error> {
        walk(doc, doc.root(), pointer).map(|id| doc.val(id))
    }

    #[test]
    fn rfc6901_section_5() {
        let doc = Doc::parse(RFC).unwrap();
        assert!(matches!(at(&doc, ""), Ok(Val::Obj(_))));
        assert!(matches!(at(&doc, "/foo"), Ok(Val::Arr(v)) if v.len() == 2));
        assert_eq!(at(&doc, "/foo/0"), Ok(&Val::Str("bar".into())));
        let rows = [
            ("/", 0),
            ("/a~1b", 1),
            ("/c%d", 2),
            ("/e^f", 3),
            ("/g|h", 4),
            ("/i\\j", 5),
            ("/k\"l", 6),
            ("/ ", 7),
            ("/m~0n", 8),
        ];
        for (pointer, want) in rows {
            assert_eq!(at(&doc, pointer), Ok(&Val::Int(want)), "{pointer}");
        }
    }

    #[test]
    fn a_bad_pointer_answers_path() {
        let doc = Doc::parse(r#"{"a":{"b":[10,20]},"~":1}"#).unwrap();
        assert_eq!(at(&doc, "a/b"), Err(Error::Path), "no leading slash");
        assert_eq!(at(&doc, "/a/b/01"), Err(Error::Path), "a leading zero");
        assert_eq!(
            at(&doc, "/a/b/-"),
            Err(Error::Path),
            "the element past the end"
        );
        assert_eq!(at(&doc, "/a/b/x"), Err(Error::Path));
        assert_eq!(at(&doc, "/~2"), Err(Error::Path), "a bad escape");
        assert_eq!(at(&doc, "/~"), Err(Error::Path));
        assert_eq!(at(&doc, "/~0"), Ok(&Val::Int(1)));
        assert_eq!(at(&doc, "/a/b/2"), Err(Error::Key));
        assert_eq!(at(&doc, "/a/c"), Err(Error::Key));
        assert_eq!(at(&doc, "/a/b/0/x"), Err(Error::Container));
        assert_eq!(at(&doc, "/a/b/1"), Ok(&Val::Int(20)));
    }

    #[test]
    fn a_long_escaped_token_still_reads() {
        let key = format!("{}/{}", "k".repeat(300), "v");
        let doc = Doc::parse(&format!(r#"{{"{key}":5}}"#)).unwrap();
        assert_eq!(
            at(&doc, &format!("/{}", key.replace('/', "~1"))),
            Ok(&Val::Int(5))
        );
    }
}
