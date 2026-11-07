// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

use crate::dom::{Doc, NodeId, Val};

enum Frame<'a> {
    Arr(std::slice::Iter<'a, NodeId>, bool),
    Obj(indexmap::map::Iter<'a, Box<str>, NodeId>, bool),
}

enum Step<'a> {
    Child(NodeId, Option<&'a str>, bool),
    Close(char),
}

/// Writes the subtree at `id` as JSON, compact or in serde_json's pretty layout, with the
/// keys in the object's own order. Like `Doc::copy_from` it keeps its own stack instead
/// of recursing, since a tree built up through natives can be any depth.
pub fn write(doc: &Doc, id: NodeId, pretty: bool, out: &mut String) {
    let mut stack: Vec<Frame> = Vec::new();
    let mut next = Some(id);
    loop {
        if let Some(n) = next.take() {
            match doc.val(n) {
                Val::Arr(v) if v.is_empty() => out.push_str("[]"),
                Val::Obj(m) if m.is_empty() => out.push_str("{}"),
                Val::Arr(v) => {
                    out.push('[');
                    stack.push(Frame::Arr(v.iter(), true));
                }
                Val::Obj(m) => {
                    out.push('{');
                    stack.push(Frame::Obj(m.iter(), true));
                }
                v => scalar(v, out),
            }
        }
        let step = match stack.last_mut() {
            None => break,
            Some(Frame::Arr(it, first)) => match it.next() {
                Some(&c) => Step::Child(c, None, std::mem::replace(first, false)),
                None => Step::Close(']'),
            },
            Some(Frame::Obj(it, first)) => match it.next() {
                Some((k, &c)) => Step::Child(c, Some(k), std::mem::replace(first, false)),
                None => Step::Close('}'),
            },
        };
        match step {
            Step::Child(c, key, first) => {
                if !first {
                    out.push(',');
                }
                newline(pretty, stack.len(), out);
                if let Some(k) = key {
                    string(k, out);
                    out.push_str(if pretty { ": " } else { ":" });
                }
                next = Some(c);
            }
            Step::Close(c) => {
                stack.pop();
                newline(pretty, stack.len(), out);
                out.push(c);
            }
        }
    }
}

pub fn to_string(doc: &Doc, id: NodeId, pretty: bool) -> String {
    let mut out = String::new();
    write(doc, id, pretty, &mut out);
    out
}

fn newline(pretty: bool, depth: usize, out: &mut String) {
    if pretty {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
}

fn scalar(v: &Val, out: &mut String) {
    match v {
        Val::Null => out.push_str("null"),
        Val::Bool(true) => out.push_str("true"),
        Val::Bool(false) => out.push_str("false"),
        Val::Int(i) => out.push_str(itoa::Buffer::new().format(*i)),
        Val::Float(f) if f.is_finite() => out.push_str(ryu::Buffer::new().format_finite(*f)),
        Val::Float(_) => out.push_str("null"),
        Val::Str(s) => string(s, out),
        Val::Arr(_) | Val::Obj(_) => unreachable!(),
    }
}

pub fn string(s: &str, out: &mut String) {
    out.push('"');
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x08 => "\\b",
            0x0c => "\\f",
            0..=0x1f => "",
            _ => continue,
        };
        out.push_str(&s[start..i]);
        if esc.is_empty() {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push_str("\\u00");
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        } else {
            out.push_str(esc);
        }
        start = i + 1;
    }
    out.push_str(&s[start..]);
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn array_of(vals: Vec<Val>) -> Doc {
        let mut doc = Doc::new(Val::Null);
        let ids = vals.into_iter().map(|v| doc.alloc(v)).collect();
        *doc.val_mut(doc.root()) = Val::Arr(ids);
        doc
    }

    fn compact(doc: &Doc) -> String {
        to_string(doc, doc.root(), false)
    }

    #[test]
    fn a_float_from_pawn_is_written_short() {
        let xs = [2165.9163f32, 0.1, 1e-7, -0.0, 5.0, -1720.25, 3.4028235e38];
        let doc = array_of(xs.iter().map(|&x| Val::from_f32(x)).collect());
        assert_eq!(
            compact(&doc),
            "[2165.9163,0.1,1e-7,-0.0,5.0,-1720.25,3.4028235e38]"
        );
        for (i, x) in xs.iter().enumerate() {
            let Val::Arr(v) = doc.val(doc.root()) else {
                panic!()
            };
            let Val::Float(f) = doc.val(v[i]) else {
                panic!()
            };
            assert_eq!(*f as f32, *x);
        }
    }

    #[test]
    fn a_float_from_text_keeps_its_f64() {
        let doc = Doc::parse("[2165.916259765625,0.30000000000000004]").unwrap();
        assert_eq!(compact(&doc), "[2165.916259765625,0.30000000000000004]");
    }

    #[test]
    fn nan_and_infinity_are_written_null() {
        let doc = array_of(vec![
            Val::from_f32(f32::NAN),
            Val::from_f32(f32::INFINITY),
            Val::Float(f64::NEG_INFINITY),
        ]);
        assert_eq!(compact(&doc), "[null,null,null]");
    }

    #[test]
    fn a_document_round_trips() {
        let compact_text = include_str!("../tests/fixtures/compact.json").trim_end();
        let doc = Doc::parse(compact_text).unwrap();
        assert_eq!(compact(&doc), compact_text);

        let pretty_text = include_str!("../tests/fixtures/pretty.json").trim_end();
        let doc = Doc::parse(pretty_text).unwrap();
        assert_eq!(to_string(&doc, doc.root(), true), pretty_text);
        assert_eq!(compact(&doc), compact_text);

        let theirs: serde_json::Value = serde_json::from_str(compact_text).unwrap();
        let ours: serde_json::Value = serde_json::from_str(&compact(&doc)).unwrap();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn escapes_are_read_and_written_back() {
        let text = include_str!("../tests/fixtures/escapes.json").trim_end();
        let doc = Doc::parse(text).unwrap();
        let Val::Obj(m) = doc.val(doc.root()) else {
            panic!()
        };
        assert_eq!(
            doc.val(m["q"]),
            &Val::Str("a\"b\\c/d\n\t\u{1}\u{e1}\u{1f600}".into())
        );
        assert_eq!(doc.val(m["k\u{e9}y"]), &Val::Int(1));
        assert_eq!(
            compact(&doc),
            "{\"q\":\"a\\\"b\\\\c/d\\n\\t\\u0001\u{e1}\u{1f600}\",\"k\u{e9}y\":1}"
        );
    }

    #[test]
    fn a_deep_tree_built_by_hand_is_written_without_recursion() {
        let mut doc = Doc::new(Val::Arr(vec![]));
        let mut inner = doc.alloc(Val::Arr(vec![]));
        for _ in 0..200_000 {
            inner = doc.alloc(Val::Arr(vec![inner]));
        }
        *doc.val_mut(doc.root()) = Val::Arr(vec![inner]);
        let s = compact(&doc);
        assert_eq!(s.len(), 2 * 200_002);
        let copy = doc.clone_subtree(doc.root());
        assert_eq!(compact(&copy), s);
    }
}
