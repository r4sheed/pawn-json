// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Natives for the pawn-json 1.x names that `include/json.inc` keeps and that no new
//! native already covers: the variadic builders, which take ownership of their nodes the
//! way 1.x did, the 1.x reads that hand out a node, and 1.x's garbage-collection
//! switches. They return the same error codes as every other native.
//!
//! 1.x scripts tend to reuse one variable for every read in a loop. 1.x got away with
//! that by freeing its copies on read; here, a read that hands out a node re-points the
//! output instead when it already holds a view the script hasn't kept, so the loop
//! doesn't leak a handle per pass.

use crate::amx::{Amx, Cell};
use crate::natives::{Call, run};
use crate::state::{Level, State};
use pawn_json_core::dom::{Doc, NodeId, Val};
use pawn_json_core::error::{Error, OK};
use pawn_json_core::pool::Pool;

/// Reads the `i`th variadic argument. Pawn passes every variadic argument by address, so
/// this goes through the pointer.
fn var(c: &Call, i: usize) -> Result<Cell, Error> {
    c.args.read(i).ok_or(Error::Address)
}

/// `Node:JSON_Object({_, Node}:...)`. With no arguments it makes an empty object; given
/// key, node, key, node... it is 1.x's builder and takes ownership of the nodes. A failure
/// returns 0 rather than an error code, since a code could be mistaken for a node.
pub extern "C" fn object(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 0, |c, st| {
        let n = c.args.count();
        if n % 2 != 0 {
            st.log(
                Level::Warning,
                "JSON_Object: the arguments are not key and node pairs",
            );
            return Ok(0);
        }
        let mut doc = Doc::new(Val::empty_object());
        let mut taken = Vec::with_capacity(n / 2);
        for pair in 0..n / 2 {
            let key = pawn_json_core::text::to_string(
                c.args.string(2 * pair).ok_or(Error::Address)?,
                c.cp,
            );
            let h = var(c, 2 * pair + 1)?;
            let Some(r) = st.pool.resolve(h) else {
                st.log(Level::Warning, "JSON_Object: a node passed is gone");
                return Ok(0);
            };
            let id = doc.copy_from(st.pool.doc(r.doc), r.node);
            let root = doc.root();
            doc.obj_put(root, &key, id).expect("an object");
            taken.push(h);
        }
        Ok(made(c, st, doc, &taken))
    })
}

/// `Node:JSON_Array(Node:...)`. With no arguments it makes an empty array; otherwise it is
/// 1.x's builder and takes ownership of the nodes.
pub extern "C" fn array(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 0, |c, st| {
        let mut doc = Doc::new(Val::Arr(Vec::new()));
        let mut taken = Vec::with_capacity(c.args.count());
        for i in 0..c.args.count() {
            let h = var(c, i)?;
            let Some(r) = st.pool.resolve(h) else {
                st.log(Level::Warning, "JSON_Array: a node passed is gone");
                return Ok(0);
            };
            let id = doc.copy_from(st.pool.doc(r.doc), r.node);
            let root = doc.root();
            doc.arr_push_node(root, id).expect("an array");
            taken.push(h);
        }
        Ok(made(c, st, doc, &taken))
    })
}

/// `Node:JSON_Append(Node:a, Node:b)`. Merges two objects, or joins two arrays, into a new
/// node. As in 1.x, both inputs are consumed whether or not it succeeds.
pub extern "C" fn append(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let (a, b) = (c.args.cell(0), c.args.cell(1));
        let doc = match (st.pool.resolve(a), st.pool.resolve(b)) {
            (Some(x), Some(y)) => join(&st.pool, (x.doc, x.node), (y.doc, y.node)),
            _ => None,
        };
        let Some(doc) = doc else {
            st.pool.release(a);
            st.pool.release(b);
            return Ok(0);
        };
        Ok(made(c, st, doc, &[a, b]))
    })
}

fn join(pool: &Pool, x: (u32, NodeId), y: (u32, NodeId)) -> Option<Doc> {
    let (dx, dy) = (pool.doc(x.0), pool.doc(y.0));
    match (dx.val(x.1), dy.val(y.1)) {
        (Val::Obj(a), Val::Obj(b)) => {
            let mut doc = Doc::new(Val::empty_object());
            let root = doc.root();
            for (from, map) in [(dx, a), (dy, b)] {
                for (k, &v) in map {
                    let id = doc.copy_from(from, v);
                    doc.obj_put(root, k, id).expect("an object");
                }
            }
            Some(doc)
        }
        (Val::Arr(a), Val::Arr(b)) => {
            let mut doc = Doc::new(Val::Arr(Vec::with_capacity(a.len() + b.len())));
            let root = doc.root();
            for (from, items) in [(dx, a), (dy, b)] {
                for &v in items {
                    let id = doc.copy_from(from, v);
                    doc.arr_push_node(root, id).expect("an array");
                }
            }
            Some(doc)
        }
        _ => None,
    }
}

/// Hands out the new document's root and releases the nodes it was built from. Returns 0
/// when the pool has run out of handles - the inputs are released either way, as 1.x
/// always took them.
fn made(c: &Call, st: &mut State, doc: Doc, taken: &[Cell]) -> Cell {
    for &t in taken {
        st.pool.release(t);
    }
    let h = st.pool.root(doc, c.owner);
    if h == 0 {
        st.tell_spent();
    }
    h
}

/// Writes a view of `id` into the output argument `out`. If `out` already holds a view
/// the script hasn't kept, that handle is re-pointed rather than a new one made.
fn view_into(c: &Call, st: &mut State, out: usize, d: u32, id: NodeId) -> Result<Cell, Error> {
    let held = c.args.read(out).ok_or(Error::Address)?;
    let h = st.pool.walk(held, d, id, c.owner);
    if h == 0 {
        st.tell_spent();
        return Err(Error::Node);
    }
    if !c.args.write(out, h) {
        return Err(Error::Address);
    }
    Ok(OK)
}

fn member(c: &Call, st: &State, node: Cell, key: usize) -> Result<(u32, NodeId), Error> {
    let key = c.args.string(key).ok_or(Error::Address)?;
    let r = st.pool.resolve(node).ok_or(Error::Node)?;
    let Val::Obj(m) = st.pool.doc(r.doc).val(r.node) else {
        return Err(Error::Container);
    };
    let id = pawn_json_core::text::with_text(key, c.cp, |k| m.get(k).copied()).ok_or(Error::Key)?;
    Ok((r.doc, id))
}

/// `JSON_GetObject(Node:node, const key[], &Node:output)`. Despite the name, it hands out
/// the value at `key` whatever its type.
pub extern "C" fn get_object(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let (d, id) = member(c, st, c.args.cell(0), 1)?;
        view_into(c, st, 2, d, id)
    })
}

/// `JSON_GetArray(Node:node, const key[], &Node:output)`. Hands out the array at `key`, or
/// fails with a type error if the value there is something else.
pub extern "C" fn get_array(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let (d, id) = member(c, st, c.args.cell(0), 1)?;
        if !matches!(st.pool.doc(d).val(id), Val::Arr(_)) {
            return Err(Error::Type);
        }
        view_into(c, st, 2, d, id)
    })
}

fn element(st: &State, node: Cell, index: Cell) -> Result<(u32, NodeId), Error> {
    let r = st.pool.resolve(node).ok_or(Error::Node)?;
    let Val::Arr(v) = st.pool.doc(r.doc).val(r.node) else {
        return Err(Error::Container);
    };
    let id = usize::try_from(index)
        .ok()
        .and_then(|i| v.get(i))
        .ok_or(Error::Key)?;
    Ok((r.doc, *id))
}

/// `JSON_ArrayObject(Node:node, index, &Node:output)`. Hands out the element at `index`,
/// whatever its type.
pub extern "C" fn array_object(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let (d, id) = element(st, c.args.cell(0), c.args.cell(1))?;
        view_into(c, st, 2, d, id)
    })
}

/// `JSON_ArrayIterate(Node:node, &index, &Node:output)`. The index is stepped before the
/// read, so a loop starts it at -1, and `JSON_ERR_KEY` means the end was reached. The
/// handle from the previous step is re-pointed at the next element, unless the script
/// kept it, in which case it is left alone and a new one is made.
pub extern "C" fn array_iterate(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let index = c.args.read(1).ok_or(Error::Address)?.wrapping_add(1);
        if !c.args.write(1, index) {
            return Err(Error::Address);
        }
        let (d, id) = element(st, c.args.cell(0), index)?;
        view_into(c, st, 2, d, id)
    })
}

/// `JSON_ArrayRemove(Node:node, const key[], Node:input)`. Removes every element of the
/// array at `key` that equals `input`'s value, and consumes `input`.
pub extern "C" fn array_remove(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let input = c.args.cell(2);
        let want = st.pool.resolve(input).ok_or(Error::Node)?;
        let at = match member(c, st, c.args.cell(0), 1) {
            Ok(at) => at,
            Err(e) => {
                st.pool.release(input);
                return Err(e);
            }
        };
        let Val::Arr(items) = st.pool.doc(at.0).val(at.1) else {
            st.pool.release(input);
            return Err(Error::Type);
        };
        let gone: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|&(_, &e)| Doc::same(st.pool.doc(at.0), e, st.pool.doc(want.doc), want.node))
            .map(|(i, _)| i)
            .collect();
        let doc = st.pool.doc_mut(at.0);
        for i in gone.into_iter().rev() {
            doc.arr_remove(at.1, i).expect("an index of the array");
        }
        st.pool.release(input);
        Ok(OK)
    })
}

/// `JSON_ToggleGC(Node:node, bool:toggle)`. Turning collection off stops the node being
/// freed when its scope ends, just as `JSON_Keep` does; turning it on undoes that.
pub extern "C" fn toggle_gc(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        if st.pool.set_kept(c.args.cell(0), c.args.cell(1) == 0) {
            Ok(OK)
        } else {
            Err(Error::Node)
        }
    })
}

/// `JSON_Cleanup(Node:node, auto = false)`. Frees the node. With `auto` set, a node whose
/// collection was turned off is left alone.
pub extern "C" fn cleanup(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let h = c.args.cell(0);
        if st.pool.owner(h).is_none() {
            return Err(Error::Node);
        }
        if c.args.cell(1) == 0 || !st.pool.kept(h) {
            st.pool.release(h);
        }
        Ok(OK)
    })
}
