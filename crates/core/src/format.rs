// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Packing and unpacking by format string, in the spirit of sscanf and of Jansson's
//! `json_pack` / `json_unpack`: `{model:i, x:f, name:s[32], pos:[f,f,f]}`. The format is
//! there to carry the types, because a Pawn macro cannot tell a string from a number:
//! both are untagged.
//!
//! The grammar, with spaces allowed between tokens:
//!   value  = object | array | leaf
//!   object = "{" [ key ":" value { "," key ":" value } ] "}" [ "?" ]
//!   array  = "[" [ value { "," value } ] "]" [ "?" ]
//!   leaf   = ( "i" | "f" | "b" | "s" [ "[" size "]" ] | "n" | "o" ) [ "?" ]
//! A key is any run of characters other than `:,{}[]`, with the spaces around it
//! trimmed. Every leaf except `n` takes one argument: unpack writes to it (`s[size]` is a
//! buffer of `size` cells, and `o` gets a view) and pack reads from it. A `?` after a
//! value means a missing key or element leaves its variables as they are instead of
//! failing; pack has no use for that and refuses it. The format is parsed only once,
//! walking the document in step with it.

use crate::dom::{Doc, NodeId, Val};
use crate::error::Error;
use crate::pool::{Cell, Owner, Pool};
use crate::text::{self, Codepage};

/// The native's variadic arguments, which the component reads from the AMX. Pawn passes
/// every variadic argument by address, so each one points at a cell or a string.
pub trait Args {
    fn count(&self) -> usize;
    /// The cell that argument `i` points at.
    fn cell(&self, i: usize) -> Option<Cell>;
    /// The string that argument `i` points at.
    fn string(&self, i: usize) -> Option<&[Cell]>;
    /// Writes to the cell that argument `i` points at, or returns false if it cannot.
    fn set(&mut self, i: usize, v: Cell) -> bool;
    /// Writes `s` into the buffer of `size` cells that argument `i` points at.
    fn set_string(&mut self, i: usize, size: usize, s: &str, cp: Codepage) -> Result<(), Error>;
}

/// What went wrong, and at which byte of the format, so the log can point at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub error: Error,
    pub at: usize,
}

struct Fmt<'a> {
    s: &'a [u8],
    i: usize,
}

#[derive(Clone, Copy)]
struct Leaf {
    kind: u8,
    size: usize,
    optional: bool,
}

impl<'a> Fmt<'a> {
    fn fail<T>(&self, error: Error) -> Result<T, Failure> {
        Err(Failure { error, at: self.i })
    }

    fn peek(&mut self) -> Option<u8> {
        while self.s.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), Failure> {
        if self.eat(c) {
            Ok(())
        } else {
            self.fail(Error::Format)
        }
    }

    fn key(&mut self) -> Result<&'a str, Failure> {
        self.peek();
        let start = self.i;
        while self.s.get(self.i).is_some_and(|b| !b":,{}[]".contains(b)) {
            self.i += 1;
        }
        let key = std::str::from_utf8(&self.s[start..self.i])
            .map(str::trim)
            .unwrap_or("");
        if key.is_empty() {
            return self.fail(Error::Format);
        }
        Ok(key)
    }

    fn leaf(&mut self) -> Result<Leaf, Failure> {
        let kind = match self.peek() {
            Some(k @ (b'i' | b'f' | b'b' | b's' | b'n' | b'o')) => k,
            _ => return self.fail(Error::Format),
        };
        self.i += 1;
        let mut size = 0;
        if kind == b's' && self.eat(b'[') {
            self.peek();
            let start = self.i;
            while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
            size = std::str::from_utf8(&self.s[start..self.i])
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if size == 0 {
                return self.fail(Error::Format);
            }
            self.expect(b']')?;
        }
        let optional = self.eat(b'?');
        Ok(Leaf {
            kind,
            size,
            optional,
        })
    }

    fn done(&mut self) -> Result<(), Failure> {
        if self.peek().is_some() {
            self.fail(Error::Format)
        } else {
            Ok(())
        }
    }
}

struct Unpack<'p, 'a, A: Args> {
    pool: &'p mut Pool,
    doc: u32,
    cp: Codepage,
    owner: Owner,
    args: &'a mut A,
    next: usize,
}

impl<A: Args> Unpack<'_, '_, A> {
    fn arg(&mut self, f: &Fmt) -> Result<usize, Failure> {
        if self.next >= self.args.count() {
            return f.fail(Error::Format);
        }
        self.next += 1;
        Ok(self.next - 1)
    }

    /// Matches one value of the format against `node`. A `node` of `None` means the value
    /// is missing from the document: the format is still read and its arguments counted,
    /// but their variables are left alone.
    fn value(&mut self, f: &mut Fmt, node: Option<NodeId>) -> Result<(), Failure> {
        match f.peek() {
            Some(b'{') => {
                f.i += 1;
                let map = match node.map(|n| self.pool.doc(self.doc).val(n)) {
                    Some(Val::Obj(_)) | None => node,
                    Some(_) => return f.fail(Error::Type),
                };
                if !f.eat(b'}') {
                    loop {
                        let key = f.key()?;
                        f.expect(b':')?;
                        let child = map.and_then(|m| match self.pool.doc(self.doc).val(m) {
                            Val::Obj(o) => o.get(key).copied(),
                            _ => None,
                        });
                        let at = f.i;
                        self.value(f, child)?;
                        if map.is_some() && child.is_none() && !self.optional_at(f, at) {
                            return Err(Failure {
                                error: Error::Key,
                                at,
                            });
                        }
                        if !f.eat(b',') {
                            break;
                        }
                    }
                    f.expect(b'}')?;
                }
                f.eat(b'?');
                Ok(())
            }
            Some(b'[') => {
                f.i += 1;
                let items = match node.map(|n| self.pool.doc(self.doc).val(n)) {
                    Some(Val::Arr(_)) | None => node,
                    Some(_) => return f.fail(Error::Type),
                };
                let mut index = 0;
                if !f.eat(b']') {
                    loop {
                        let child = items.and_then(|a| match self.pool.doc(self.doc).val(a) {
                            Val::Arr(v) => v.get(index).copied(),
                            _ => None,
                        });
                        let at = f.i;
                        self.value(f, child)?;
                        if items.is_some() && child.is_none() && !self.optional_at(f, at) {
                            return Err(Failure {
                                error: Error::Key,
                                at,
                            });
                        }
                        index += 1;
                        if !f.eat(b',') {
                            break;
                        }
                    }
                    f.expect(b']')?;
                }
                f.eat(b'?');
                Ok(())
            }
            _ => {
                let leaf = f.leaf()?;
                if leaf.kind == b's' && leaf.size == 0 {
                    return f.fail(Error::Format);
                }
                let i = if leaf.kind == b'n' {
                    usize::MAX
                } else {
                    self.arg(f)?
                };
                let Some(n) = node else { return Ok(()) };
                self.read(f, leaf, n, i)
            }
        }
    }

    /// Whether the value that started at offset `at` ended with a `?`.
    fn optional_at(&self, f: &Fmt, at: usize) -> bool {
        let end = f.i;
        f.s[at..end].iter().rev().find(|b| !b.is_ascii_whitespace()) == Some(&b'?')
    }

    fn read(&mut self, f: &Fmt, leaf: Leaf, n: NodeId, i: usize) -> Result<(), Failure> {
        let fail = |error| Failure { error, at: f.i };
        let v = self.pool.doc(self.doc).val(n);
        let mut c = 0;
        match leaf.kind {
            b'i' => crate::api::read_int(v, &mut c).map_err(fail)?,
            b'f' => crate::api::read_float(v, &mut c).map_err(fail)?,
            b'b' => crate::api::read_bool(v, &mut c).map_err(fail)?,
            b'n' => {
                return if matches!(v, Val::Null) {
                    Ok(())
                } else {
                    Err(fail(Error::Type))
                };
            }
            b's' => {
                let Val::Str(s) = v else {
                    return Err(fail(Error::Type));
                };
                return self.args.set_string(i, leaf.size, s, self.cp).map_err(fail);
            }
            _ => match self.pool.view(self.doc, n, self.owner) {
                0 => return Err(fail(Error::Node)),
                h => c = h,
            },
        }
        if self.args.set(i, c) {
            Ok(())
        } else {
            Err(fail(Error::Address))
        }
    }
}

/// `JSON_Unpack(Node:node, const format[], ...)`. Reads the format's values into the
/// arguments and stops at the first failure, so every variable after it is left as it was.
pub fn unpack<A: Args>(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    node: Cell,
    format: &str,
    args: &mut A,
) -> Result<(), Failure> {
    let start = Failure {
        error: Error::Node,
        at: 0,
    };
    let r = pool.resolve(node).ok_or(start)?;
    let mut f = Fmt {
        s: format.as_bytes(),
        i: 0,
    };
    let mut u = Unpack {
        pool,
        doc: r.doc,
        cp,
        owner,
        args,
        next: 0,
    };
    u.value(&mut f, Some(r.node))?;
    f.done()?;
    if u.next != u.args.count() {
        return f.fail(Error::Format);
    }
    Ok(())
}

struct Pack<'p, 'a, A: Args> {
    pool: &'p Pool,
    cp: Codepage,
    args: &'a A,
    next: usize,
    used_up: Vec<Cell>,
}

impl<A: Args> Pack<'_, '_, A> {
    fn value(&mut self, f: &mut Fmt, doc: &mut Doc) -> Result<NodeId, Failure> {
        match f.peek() {
            Some(b'{') => {
                f.i += 1;
                let obj = doc.alloc(Val::empty_object());
                if !f.eat(b'}') {
                    loop {
                        let key = f.key()?;
                        f.expect(b':')?;
                        let child = self.value(f, doc)?;
                        doc.obj_put(obj, key, child)
                            .map_err(|error| Failure { error, at: f.i })?;
                        if !f.eat(b',') {
                            break;
                        }
                    }
                    f.expect(b'}')?;
                }
                self.no_optional(f)?;
                Ok(obj)
            }
            Some(b'[') => {
                f.i += 1;
                let arr = doc.alloc(Val::Arr(Vec::new()));
                if !f.eat(b']') {
                    loop {
                        let child = self.value(f, doc)?;
                        doc.arr_push_node(arr, child)
                            .map_err(|error| Failure { error, at: f.i })?;
                        if !f.eat(b',') {
                            break;
                        }
                    }
                    f.expect(b']')?;
                }
                self.no_optional(f)?;
                Ok(arr)
            }
            _ => {
                let leaf = f.leaf()?;
                if leaf.optional {
                    return f.fail(Error::Format);
                }
                if leaf.kind == b'n' {
                    return Ok(doc.alloc(Val::Null));
                }
                if self.next >= self.args.count() {
                    return f.fail(Error::Format);
                }
                let i = self.next;
                self.next += 1;
                let fail = Failure {
                    error: Error::Address,
                    at: f.i,
                };
                let cell = || self.args.cell(i).ok_or(fail);
                let v = match leaf.kind {
                    b'i' => crate::api::int_val(cell()?),
                    b'f' => crate::api::float_val(cell()?),
                    b'b' => crate::api::bool_val(cell()?),
                    b's' => Val::Str(
                        text::to_string(self.args.string(i).ok_or(fail)?, self.cp).into_boxed_str(),
                    ),
                    _ => {
                        let h = cell()?;
                        let r = self.pool.resolve(h).ok_or(Failure {
                            error: Error::Node,
                            at: f.i,
                        })?;
                        let src = self.pool.doc(r.doc);
                        if r.node == src.root() && self.pool.doc_handles(r.doc) == 1 {
                            self.used_up.push(h);
                        }
                        return Ok(doc.copy_from(src, r.node));
                    }
                };
                Ok(doc.alloc(v))
            }
        }
    }

    fn no_optional(&self, f: &mut Fmt) -> Result<(), Failure> {
        if f.eat(b'?') {
            f.fail(Error::Format)
        } else {
            Ok(())
        }
    }
}

/// `Node:JSON_Pack(const format[], ...)`. Builds a new document from the arguments. A
/// node passed for `o` is copied in, and used up if it was its document's only handle,
/// the same rule `JSON_SetNode` follows.
pub fn pack<A: Args>(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    format: &str,
    args: &A,
) -> Result<Cell, Failure> {
    let mut f = Fmt {
        s: format.as_bytes(),
        i: 0,
    };
    let mut doc = Doc::new(Val::Null);
    let mut p = Pack {
        pool,
        cp,
        args,
        next: 0,
        used_up: Vec::new(),
    };
    let top = p.value(&mut f, &mut doc)?;
    f.done()?;
    if p.next != args.count() {
        return f.fail(Error::Format);
    }
    let used_up = std::mem::take(&mut p.used_up);
    doc.reroot(top);
    let h = pool.root(doc, owner);
    if h == 0 {
        return Err(Failure {
            error: Error::Node,
            at: 0,
        });
    }
    for u in used_up {
        pool.release(u);
    }
    Ok(h)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::write::to_string;

    const ME: Owner = Owner {
        script: 1,
        frame: 100,
    };
    const CP: Codepage = Codepage::DEFAULT;

    /// Stand-in arguments for the tests, each one a buffer the format reads or writes.
    pub struct Fake(pub Vec<Vec<Cell>>);

    impl Args for Fake {
        fn count(&self) -> usize {
            self.0.len()
        }
        fn cell(&self, i: usize) -> Option<Cell> {
            self.0.get(i)?.first().copied()
        }
        fn string(&self, i: usize) -> Option<&[Cell]> {
            self.0.get(i).map(|v| &v[..])
        }
        fn set(&mut self, i: usize, v: Cell) -> bool {
            match self.0.get_mut(i).and_then(|a| a.first_mut()) {
                Some(c) => {
                    *c = v;
                    true
                }
                None => false,
            }
        }
        fn set_string(
            &mut self,
            i: usize,
            size: usize,
            s: &str,
            cp: Codepage,
        ) -> Result<(), Error> {
            let buf = self.0.get_mut(i).ok_or(Error::Address)?;
            let n = size.min(buf.len());
            text::put(&mut buf[..n], s, cp)
        }
    }

    fn s(text: &str) -> Vec<Cell> {
        text.bytes().map(|b| b as Cell).chain([0]).collect()
    }

    fn back(cells: &[Cell]) -> String {
        text::to_string(cells, CP)
    }

    fn parse(pool: &mut Pool, json: &str) -> Cell {
        pool.root(Doc::parse(json).unwrap(), ME)
    }

    fn float(c: Cell) -> f32 {
        f32::from_bits(c as u32)
    }

    const ROW: &str = r#"{"model": 549, "x": -1655.78, "y": 1315.25, "z": 8.7917, "angle": 315.0, "color1": -1, "color2": -1, "paintjob": 3, "respawn": 0, "source": "both", "note": "sfe_stream3.ipl, random"}"#;
    const ROW_FORMAT: &str = "{model:i, x:f, y:f, z:f, angle:f, color1:i, color2:i, paintjob:i, respawn:i, source:s[8], note:s[32]}";

    #[test]
    fn unpack_reads_a_placed_vehicle_row_in_one_call() {
        let mut pool = Pool::default();
        let row = parse(&mut pool, ROW);
        let mut args = Fake(vec![vec![0]; 9]);
        args.0.push(vec![0; 8]);
        args.0.push(vec![0; 32]);
        assert_eq!(
            unpack(&mut pool, CP, ME, row, ROW_FORMAT, &mut args),
            Ok(())
        );
        let a = &args.0;
        assert_eq!(a[0][0], 549);
        assert_eq!(
            (
                float(a[1][0]),
                float(a[2][0]),
                float(a[3][0]),
                float(a[4][0])
            ),
            (-1655.78, 1315.25, 8.7917, 315.0)
        );
        assert_eq!((a[5][0], a[6][0], a[7][0], a[8][0]), (-1, -1, 3, 0));
        assert_eq!(
            (back(&a[9]), back(&a[10])),
            ("both".into(), "sfe_stream3.ipl, random".into())
        );
    }

    #[test]
    fn pack_builds_what_unpack_reads() {
        let mut pool = Pool::default();
        let inner = parse(&mut pool, r#"{"k":true}"#);
        let args = Fake(vec![
            s("Zsofi"),
            vec![30],
            vec![2.5f32.to_bits() as Cell],
            vec![(-1.0f32).to_bits() as Cell],
            vec![1],
            vec![inner],
        ]);
        let h = pack(
            &mut pool,
            CP,
            ME,
            "{name:s, age:i, pos:[f, f], on:b, none:n, more:o}",
            &args,
        )
        .unwrap();
        let (doc, node) = pool.get(h).unwrap();
        assert_eq!(
            to_string(doc, node, false),
            r#"{"name":"Zsofi","age":30,"pos":[2.5,-1.0],"on":true,"none":null,"more":{"k":true}}"#
        );
        assert!(
            pool.resolve(inner).is_none(),
            "the lone node given for o was used up"
        );
        let mut out = Fake(vec![vec![0; 8], vec![0], vec![0], vec![0], vec![0]]);
        assert_eq!(
            unpack(
                &mut pool,
                CP,
                ME,
                h,
                "{name:s[8], age:i, pos:[f, f], none:n, more:{k:b}}",
                &mut out
            ),
            Ok(())
        );
        assert_eq!(
            (
                back(&out.0[0]),
                out.0[1][0],
                float(out.0[2][0]),
                float(out.0[3][0])
            ),
            ("Zsofi".into(), 30, 2.5, -1.0)
        );
    }

    #[test]
    fn an_unpack_into_a_short_buffer_answers_buffer() {
        let mut pool = Pool::default();
        let row = parse(&mut pool, ROW);
        let mut args = Fake(vec![vec![0; 4], vec![-7]]);
        let r = unpack(&mut pool, CP, ME, row, "{source:s[4], model:i}", &mut args);
        assert_eq!(r.map_err(|f| f.error), Err(Error::Buffer));
        assert_eq!(back(&args.0[0]), "bot");
        assert_eq!(args.0[1][0], -7, "the unpack stopped at the first failure");
    }

    #[test]
    fn a_missing_key_answers_key_unless_marked_optional() {
        let mut pool = Pool::default();
        let row = parse(&mut pool, r#"{"a":1,"list":[1]}"#);
        let mut args = Fake(vec![vec![9], vec![9]]);
        assert_eq!(
            unpack(&mut pool, CP, ME, row, "{b:i?, a:i}", &mut args),
            Ok(())
        );
        assert_eq!(
            (args.0[0][0], args.0[1][0]),
            (9, 1),
            "the optional one was left"
        );
        let r = unpack(&mut pool, CP, ME, row, "{b:i, a:i}", &mut args);
        assert_eq!(r.map(|_| ()).map_err(|f| f.error), Err(Error::Key));
        let mut three = Fake(vec![vec![0], vec![5]]);
        assert_eq!(
            unpack(&mut pool, CP, ME, row, "{list:[i, i?]}", &mut three),
            Ok(())
        );
        assert_eq!((three.0[0][0], three.0[1][0]), (1, 5));
        let mut nested = Fake(vec![vec![4], vec![4], vec![0]]);
        assert_eq!(
            unpack(&mut pool, CP, ME, row, "{pos:[f, f]?, a:i}", &mut nested),
            Ok(())
        );
        assert_eq!(
            (nested.0[0][0], nested.0[1][0], nested.0[2][0]),
            (4, 4, 1),
            "the missing pair was left"
        );
        assert_eq!(
            unpack(&mut pool, CP, ME, row, "{pos:[f, f], a:i}", &mut nested).map_err(|f| f.error),
            Err(Error::Key)
        );
    }

    #[test]
    fn the_grammar() {
        let mut pool = Pool::default();
        let row = parse(&mut pool, r#"{"a":1,"b":"x","c":[1,2],"d":null}"#);
        let mut one = Fake(vec![vec![0]]);
        let err = |pool: &mut Pool, fmt: &str, args: &mut Fake| {
            unpack(pool, CP, ME, row, fmt, args).map_err(|f| f.error)
        };
        assert_eq!(err(&mut pool, "  {  a : i  }  ", &mut one), Ok(()));
        assert_eq!(err(&mut pool, "{a:i, d:n}", &mut one), Ok(()));
        for bad in [
            "{a:i", "{a i}", "{a:x}", "{:i}", "{a:i}}", "{b:s}", "{b:s[]}", "{b:s[0]}", "{b:s[4}",
            "{a:i,}",
        ] {
            assert_eq!(
                err(&mut pool, bad, &mut Fake(vec![vec![0; 4]])),
                Err(Error::Format),
                "{bad}"
            );
        }
        assert_eq!(
            err(&mut pool, "{a:i, c:[i,i]}", &mut one),
            Err(Error::Format),
            "more values than arguments"
        );
        assert_eq!(
            err(&mut pool, "{a:i}", &mut Fake(vec![vec![0], vec![0]])),
            Err(Error::Format),
            "more arguments than values"
        );
        assert_eq!(err(&mut pool, "{b:i}", &mut one), Err(Error::Type));
        assert_eq!(err(&mut pool, "{a:[i]}", &mut one), Err(Error::Type));
        assert_eq!(err(&mut pool, "{d:i}", &mut one), Err(Error::Type));
        let f = unpack(&mut pool, CP, ME, row, "{a:i, q:?}", &mut one).unwrap_err();
        assert_eq!((f.error, f.at), (Error::Format, 8));
        assert!(
            pack(&mut pool, CP, ME, "{a:i?}", &Fake(vec![vec![1]])).is_err(),
            "pack refuses ?"
        );
        assert!(pack(&mut pool, CP, ME, "[i, i]", &Fake(vec![vec![1]])).is_err());
        let h = pack(&mut pool, CP, ME, "i", &Fake(vec![vec![7]])).unwrap();
        let (doc, node) = pool.get(h).unwrap();
        assert_eq!(to_string(doc, node, false), "7");
    }

    #[test]
    fn an_unpacked_node_is_a_view() {
        let mut pool = Pool::default();
        let row = parse(&mut pool, r#"{"pos":[1,2]}"#);
        let mut args = Fake(vec![vec![0]]);
        assert_eq!(unpack(&mut pool, CP, ME, row, "{pos:o}", &mut args), Ok(()));
        let (doc, node) = pool.get_mut(args.0[0][0]).unwrap();
        doc.arr_push(node, Val::Int(3)).unwrap();
        let (doc, node) = pool.get(row).unwrap();
        assert_eq!(to_string(doc, node, false), r#"{"pos":[1,2,3]}"#);
    }
}
