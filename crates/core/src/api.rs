// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! The logic behind every native, working on the pool and on slices of cells. The
//! component turns AMX addresses into slices and calls these, and the tests and the
//! benchmark call them the same way. Most functions return an `error` code; the ones that
//! make a node return the node, or 0.

use crate::dom::{Doc, NodeId, Val};
use crate::error::{Error, OK, Type, code};
use crate::file::Saver;
use crate::pool::{Cell, Owner, Pool};
use crate::text::{self, Codepage};
use crate::write;
use std::collections::HashMap;

fn found(pool: &Pool, h: Cell) -> Result<(u32, NodeId), Error> {
    let r = pool.resolve(h).ok_or(Error::Node)?;
    Ok((r.doc, r.node))
}

fn member(pool: &Pool, cp: Codepage, h: Cell, key: &[Cell]) -> Result<(u32, NodeId), Error> {
    let (d, node) = found(pool, h)?;
    let Val::Obj(m) = pool.doc(d).val(node) else {
        return Err(Error::Container);
    };
    let id = text::with_text(key, cp, |k| m.get(k).copied()).ok_or(Error::Key)?;
    Ok((d, id))
}

fn element(pool: &Pool, h: Cell, index: Cell) -> Result<(u32, NodeId), Error> {
    let (d, node) = found(pool, h)?;
    let Val::Arr(v) = pool.doc(d).val(node) else {
        return Err(Error::Container);
    };
    let id = usize::try_from(index)
        .ok()
        .and_then(|i| v.get(i))
        .ok_or(Error::Key)?;
    Ok((d, *id))
}

fn val(pool: &Pool, at: (u32, NodeId)) -> &Val {
    pool.doc(at.0).val(at.1)
}

pub fn type_of_val(v: &Val) -> Type {
    match v {
        Val::Null => Type::Null,
        Val::Bool(_) => Type::Bool,
        Val::Int(_) => Type::Int,
        Val::Float(_) => Type::Float,
        Val::Str(_) => Type::String,
        Val::Arr(_) => Type::Array,
        Val::Obj(_) => Type::Object,
    }
}

pub fn read_int(v: &Val, out: &mut Cell) -> Result<(), Error> {
    match v {
        Val::Int(i) => {
            *out = Cell::try_from(*i).map_err(|_| Error::Range)?;
            Ok(())
        }
        _ => Err(Error::Type),
    }
}

pub fn read_float(v: &Val, out: &mut Cell) -> Result<(), Error> {
    let f = match v {
        Val::Int(i) => *i as f32,
        Val::Float(f) => *f as f32,
        _ => return Err(Error::Type),
    };
    *out = f.to_bits() as Cell;
    Ok(())
}

pub fn read_bool(v: &Val, out: &mut Cell) -> Result<(), Error> {
    match v {
        Val::Bool(b) => {
            *out = *b as Cell;
            Ok(())
        }
        _ => Err(Error::Type),
    }
}

pub fn read_string(v: &Val, cp: Codepage, out: &mut [Cell]) -> Result<(), Error> {
    match v {
        Val::Str(s) => text::put(out, s, cp),
        _ => Err(Error::Type),
    }
}

// Parsing.

pub fn parse_doc(input: &[Cell], cp: Codepage) -> Result<Doc, crate::dom::ParseError> {
    Doc::parse(&text::to_string(input, cp))
}

/// `JSON_Parse(const text[], &Node:out)`. `out` comes in holding what the variable held,
/// which the new node replaces (see `overwrite`), and is left at 0 if anything fails.
pub fn parse(pool: &mut Pool, cp: Codepage, owner: Owner, input: &[Cell], out: &mut Cell) -> Cell {
    let old = std::mem::take(out);
    let code = match parse_doc(input, cp) {
        Ok(doc) => {
            *out = pool.root(doc, owner);
            if *out == 0 { Error::Node as Cell } else { OK }
        }
        Err(_) => Error::Syntax as Cell,
    };
    overwrite(pool, owner, old);
    code
}

/// A node written into a variable replaces what the variable held. If that was a handle
/// of the same script that nothing keeps, it is released, so a variable that is parsed
/// or read into over and over doesn't leave a node behind each time. A copy of the old
/// handle in another variable stops working, which is why kept handles are left alone.
pub fn overwrite(pool: &mut Pool, owner: Owner, old: Cell) {
    if pool.owner(old).is_some_and(|o| o.script == owner.script) && !pool.kept(old) {
        pool.release(old);
    }
}

// Reading an object's values by key.

pub fn get_int(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell], out: &mut Cell) -> Cell {
    code(member(pool, cp, node, key).and_then(|at| read_int(val(pool, at), out)))
}

pub fn get_float(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell], out: &mut Cell) -> Cell {
    code(member(pool, cp, node, key).and_then(|at| read_float(val(pool, at), out)))
}

pub fn get_bool(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell], out: &mut Cell) -> Cell {
    code(member(pool, cp, node, key).and_then(|at| read_bool(val(pool, at), out)))
}

pub fn get_string(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell], out: &mut [Cell]) -> Cell {
    code(member(pool, cp, node, key).and_then(|at| read_string(val(pool, at), cp, out)))
}

/// `JSON_GetNode(Node:node, const key[], &Node:child)`. The child is a view into the same
/// document; nothing is copied.
pub fn get_node(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    node: Cell,
    key: &[Cell],
    out: &mut Cell,
) -> Cell {
    code(member(pool, cp, node, key).and_then(|(d, id)| view(pool, owner, d, id, out)))
}

/// Hands out a view of `id` in `out`, which comes in holding what the variable held. A
/// view this script made and didn't keep is re-pointed, as a walk's element is; anything
/// else the variable held is replaced as `overwrite` says. So a read into the same
/// variable in a loop uses one handle, not one per pass.
fn view(pool: &mut Pool, owner: Owner, d: u32, id: NodeId, out: &mut Cell) -> Result<(), Error> {
    let old = *out;
    match pool.walk(old, d, id, owner) {
        0 => Err(Error::Node),
        h => {
            if h != old {
                overwrite(pool, owner, old);
            }
            *out = h;
            Ok(())
        }
    }
}

/// `JSON_TypeOf(Node:node, const key[] = "")`. The type of the value under `key`, or of
/// the node itself when the key is empty, and `Type::Missing` when there is nothing there.
pub fn type_of(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell]) -> Cell {
    let at = if text::is_empty(key) {
        found(pool, node)
    } else {
        member(pool, cp, node, key)
    };
    match at {
        Ok(at) => type_of_val(val(pool, at)) as Cell,
        Err(_) => Type::Missing as Cell,
    }
}

pub fn has_key(pool: &Pool, cp: Codepage, node: Cell, key: &[Cell]) -> bool {
    member(pool, cp, node, key).is_ok()
}

/// `JSON_Count(Node:node, &count)`. Counts an array's elements or an object's keys.
pub fn count(pool: &Pool, node: Cell, out: &mut Cell) -> Cell {
    code(found(pool, node).and_then(|at| {
        *out = match val(pool, at) {
            Val::Arr(v) => v.len(),
            Val::Obj(m) => m.len(),
            _ => return Err(Error::Container),
        } as Cell;
        Ok(())
    }))
}

/// `JSON_KeyAt(Node:obj, index, key[], size)`. The key at `index`, counting in the order
/// the keys appear in the file.
pub fn key_at(pool: &Pool, cp: Codepage, node: Cell, index: Cell, out: &mut [Cell]) -> Cell {
    code(found(pool, node).and_then(|at| {
        let Val::Obj(m) = val(pool, at) else {
            return Err(Error::Container);
        };
        let (k, _) = usize::try_from(index)
            .ok()
            .and_then(|i| m.get_index(i))
            .ok_or(Error::Key)?;
        text::put(out, k, cp)
    }))
}

// Reading by RFC 6901 path, so a value several levels down takes one call instead of a
// `JSON_GetNode` for each level.

fn at_path(pool: &Pool, cp: Codepage, h: Cell, path: &[Cell]) -> Result<(u32, NodeId), Error> {
    let (d, node) = found(pool, h)?;
    let id = text::with_text(path, cp, |p| crate::path::walk(pool.doc(d), node, p))?;
    Ok((d, id))
}

pub fn path_int(pool: &Pool, cp: Codepage, node: Cell, path: &[Cell], out: &mut Cell) -> Cell {
    code(at_path(pool, cp, node, path).and_then(|at| read_int(val(pool, at), out)))
}

pub fn path_float(pool: &Pool, cp: Codepage, node: Cell, path: &[Cell], out: &mut Cell) -> Cell {
    code(at_path(pool, cp, node, path).and_then(|at| read_float(val(pool, at), out)))
}

pub fn path_bool(pool: &Pool, cp: Codepage, node: Cell, path: &[Cell], out: &mut Cell) -> Cell {
    code(at_path(pool, cp, node, path).and_then(|at| read_bool(val(pool, at), out)))
}

pub fn path_string(pool: &Pool, cp: Codepage, node: Cell, path: &[Cell], out: &mut [Cell]) -> Cell {
    code(at_path(pool, cp, node, path).and_then(|at| read_string(val(pool, at), cp, out)))
}

pub fn path_node(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    node: Cell,
    path: &[Cell],
    out: &mut Cell,
) -> Cell {
    code(at_path(pool, cp, node, path).and_then(|(d, id)| view(pool, owner, d, id, out)))
}

// Reading an array's elements by index. Apart from `element_node`, these read the value
// directly and make no handle for the element.

pub fn element_node(pool: &mut Pool, owner: Owner, arr: Cell, index: Cell, out: &mut Cell) -> Cell {
    code(element(pool, arr, index).and_then(|(d, id)| view(pool, owner, d, id, out)))
}

pub fn element_int(pool: &Pool, arr: Cell, index: Cell, out: &mut Cell) -> Cell {
    code(element(pool, arr, index).and_then(|at| read_int(val(pool, at), out)))
}

pub fn element_float(pool: &Pool, arr: Cell, index: Cell, out: &mut Cell) -> Cell {
    code(element(pool, arr, index).and_then(|at| read_float(val(pool, at), out)))
}

pub fn element_bool(pool: &Pool, arr: Cell, index: Cell, out: &mut Cell) -> Cell {
    code(element(pool, arr, index).and_then(|at| read_bool(val(pool, at), out)))
}

pub fn element_string(pool: &Pool, cp: Codepage, arr: Cell, index: Cell, out: &mut [Cell]) -> Cell {
    code(element(pool, arr, index).and_then(|at| read_string(val(pool, at), cp, out)))
}

// Reading a node's own value. None of these frees the node, the way reading one did in
// pawn-json 1.x.

pub fn value_int(pool: &Pool, node: Cell, out: &mut Cell) -> Cell {
    code(found(pool, node).and_then(|at| read_int(val(pool, at), out)))
}

pub fn value_float(pool: &Pool, node: Cell, out: &mut Cell) -> Cell {
    code(found(pool, node).and_then(|at| read_float(val(pool, at), out)))
}

pub fn value_bool(pool: &Pool, node: Cell, out: &mut Cell) -> Cell {
    code(found(pool, node).and_then(|at| read_bool(val(pool, at), out)))
}

pub fn value_string(pool: &Pool, cp: Codepage, node: Cell, out: &mut [Cell]) -> Cell {
    code(found(pool, node).and_then(|at| read_string(val(pool, at), cp, out)))
}

// Walking a container with a cursor.

/// `JSON_Next(Node:container, &cursor, &Node:el)`. Points `el` at the next element or
/// value and advances `cursor`. `el` is re-pointed in place unless the script kept it.
/// Returns `Error::Key` once the cursor is past the end.
pub fn next(
    pool: &mut Pool,
    owner: Owner,
    container: Cell,
    cursor: &mut Cell,
    el: &mut Cell,
) -> Cell {
    code(found(pool, container).and_then(|(d, node)| {
        let i = usize::try_from(*cursor).map_err(|_| Error::Key)?;
        let id = match pool.doc(d).val(node) {
            Val::Arr(v) => v.get(i).copied(),
            Val::Obj(m) => m.get_index(i).map(|(_, &id)| id),
            _ => return Err(Error::Container),
        }
        .ok_or(Error::Key)?;
        match pool.walk(*el, d, id, owner) {
            0 => return Err(Error::Node),
            h => *el = h,
        }
        *cursor += 1;
        Ok(())
    }))
}

/// `JSON_NextKey(Node:obj, &cursor, key[], size)`. The cursor still moves on when the key
/// had to be cut to fit the buffer, so a loop never sticks on a long key.
pub fn next_key(pool: &Pool, cp: Codepage, obj: Cell, cursor: &mut Cell, out: &mut [Cell]) -> Cell {
    let r = key_at(pool, cp, obj, *cursor, out);
    if r == OK || r == Error::Buffer as Cell {
        *cursor += 1;
    }
    r
}

/// `JSON_NextEntry(Node:obj, &cursor, key[], size, &Node:val)`. As with `next_key`, a key
/// cut to fit still counts as a step.
pub fn next_entry(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    obj: Cell,
    cursor: &mut Cell,
    key: &mut [Cell],
    el: &mut Cell,
) -> Cell {
    let at = *cursor;
    let r = key_at(pool, cp, obj, at, key);
    if r != OK && r != Error::Buffer as Cell {
        return r;
    }
    let mut c = at;
    match next(pool, owner, obj, &mut c, el) {
        OK => {
            *cursor = c;
            r
        }
        e => e,
    }
}

// Constructors. Each returns the new node, or 0 if it could not make one.

pub fn object(pool: &mut Pool, owner: Owner) -> Cell {
    pool.root(Doc::new(Val::empty_object()), owner)
}

pub fn array(pool: &mut Pool, owner: Owner) -> Cell {
    pool.root(Doc::new(Val::Arr(Vec::new())), owner)
}

/// `JSON_Copy(Node:node)`. Copies the node's value out into a document of its own.
pub fn copy(pool: &mut Pool, owner: Owner, node: Cell) -> Cell {
    match found(pool, node) {
        Ok((d, id)) => {
            let doc = pool.doc(d).clone_subtree(id);
            pool.root(doc, owner)
        }
        Err(_) => 0,
    }
}

// Turning a native's cell arguments into values.

pub fn int_val(c: Cell) -> Val {
    Val::Int(c as i64)
}

pub fn float_val(c: Cell) -> Val {
    Val::from_f32(f32::from_bits(c as u32))
}

pub fn bool_val(c: Cell) -> Val {
    Val::Bool(c != 0)
}

pub fn string_val(cells: &[Cell], cp: Codepage) -> Val {
    Val::Str(text::to_string(cells, cp).into_boxed_str())
}

// Writes. Overwriting or removing a value frees its nodes, so any handle still pointing
// at them gets `Error::Node` from then on.

fn object_at(pool: &Pool, h: Cell) -> Result<(u32, NodeId), Error> {
    let at = found(pool, h)?;
    match val(pool, at) {
        Val::Obj(_) => Ok(at),
        _ => Err(Error::Container),
    }
}

fn array_at(pool: &Pool, h: Cell) -> Result<(u32, NodeId), Error> {
    let at = found(pool, h)?;
    match val(pool, at) {
        Val::Arr(_) => Ok(at),
        _ => Err(Error::Container),
    }
}

/// Copies the value behind the handle `value` into document `dst` and returns the new
/// node. Two documents never share a node, and a node set into its own tree is copied
/// out first, so no cycle can form. If `value` is a document's root and its only handle,
/// it is used up, meaning released after the copy, so a temporary such as
/// `JSON_SetNode(o, "list", JSON_Array())` leaks nothing, just as in 1.x. A view, or a
/// document that other handles still hold, is left as it was.
fn take_in(pool: &mut Pool, dst: u32, value: Cell) -> Result<NodeId, Error> {
    let (s, id) = found(pool, value)?;
    if s == dst {
        let tmp = pool.doc(s).clone_subtree(id);
        return Ok(pool.doc_mut(dst).copy_from(&tmp, tmp.root()));
    }
    let used_up = id == pool.doc(s).root() && pool.doc_handles(s) == 1;
    let (to, from) = pool.doc_pair(dst, s);
    let new = to.copy_from(from, id);
    if used_up {
        pool.release(value);
    }
    Ok(new)
}

/// `JSON_Set*(Node:node, const key[], value)`, for every kind of value but a node.
pub fn set(pool: &mut Pool, cp: Codepage, node: Cell, key: &[Cell], v: Val) -> Cell {
    code(object_at(pool, node).and_then(|(d, n)| {
        let doc = pool.doc_mut(d);
        text::with_text(key, cp, |k| doc.obj_set(n, k, v).map(|_| ()))
    }))
}

/// `JSON_SetNode(Node:node, const key[], Node:value)`. The value goes in the way
/// `take_in` describes.
pub fn set_node(pool: &mut Pool, cp: Codepage, node: Cell, key: &[Cell], value: Cell) -> Cell {
    code(object_at(pool, node).and_then(|(d, n)| {
        let id = take_in(pool, d, value)?;
        let doc = pool.doc_mut(d);
        text::with_text(key, cp, |k| doc.obj_put(n, k, id))
    }))
}

/// `JSON_Remove(Node:node, const key[])`.
pub fn remove(pool: &mut Pool, cp: Codepage, node: Cell, key: &[Cell]) -> Cell {
    code(object_at(pool, node).and_then(|(d, n)| {
        let doc = pool.doc_mut(d);
        text::with_text(key, cp, |k| doc.obj_remove(n, k))
    }))
}

/// `JSON_Append*(Node:array, value)`.
pub fn append(pool: &mut Pool, arr: Cell, v: Val) -> Cell {
    code(array_at(pool, arr).and_then(|(d, n)| pool.doc_mut(d).arr_push(n, v).map(|_| ())))
}

pub fn append_node(pool: &mut Pool, arr: Cell, value: Cell) -> Cell {
    code(array_at(pool, arr).and_then(|(d, n)| {
        let id = take_in(pool, d, value)?;
        pool.doc_mut(d).arr_push_node(n, id)
    }))
}

/// Checks that `index` falls inside an array of `len` elements, or returns `Error::Key`.
fn index_in(index: Cell, len: usize) -> Result<usize, Error> {
    usize::try_from(index)
        .ok()
        .filter(|&i| i < len)
        .ok_or(Error::Key)
}

fn array_len(pool: &Pool, at: (u32, NodeId)) -> usize {
    match val(pool, at) {
        Val::Arr(items) => items.len(),
        _ => 0,
    }
}

/// `JSON_ArraySet*(Node:array, index, value)`. This replaces an element and never grows
/// the array, so an index past the end returns `Error::Key`.
pub fn array_set(pool: &mut Pool, arr: Cell, index: Cell, v: Val) -> Cell {
    code(array_at(pool, arr).and_then(|(d, n)| {
        let i = index_in(index, array_len(pool, (d, n)))?;
        let doc = pool.doc_mut(d);
        let id = doc.alloc(v);
        doc.arr_put(n, i, id)
    }))
}

pub fn array_set_node(pool: &mut Pool, arr: Cell, index: Cell, value: Cell) -> Cell {
    code(array_at(pool, arr).and_then(|(d, n)| {
        let i = index_in(index, array_len(pool, (d, n)))?;
        let id = take_in(pool, d, value)?;
        pool.doc_mut(d).arr_put(n, i, id)
    }))
}

/// `JSON_ArrayRemove(Node:array, index)`. The elements after it move down one place.
pub fn array_remove(pool: &mut Pool, arr: Cell, index: Cell) -> Cell {
    code(array_at(pool, arr).and_then(|(d, n)| {
        let i = index_in(index, array_len(pool, (d, n)))?;
        pool.doc_mut(d).arr_remove(n, i)
    }))
}

// Files. Each returns its code and, when something went wrong, a line for the server log.

/// `JSON_ParseFile(const path[], &Node:node)`. Saves still being finished are waited for
/// first, so a file read straight after `JSON_SaveFile` holds what was saved, however its
/// path is spelled.
pub fn parse_file(
    pool: &mut Pool,
    saver: &Saver,
    cp: Codepage,
    owner: Owner,
    path: &[Cell],
    out: &mut Cell,
) -> (Cell, Option<String>) {
    let old = std::mem::take(out);
    saver.flush();
    let path = text::to_string(path, cp);
    let result = match crate::file::load(&path) {
        Ok(doc) => match pool.root(doc, owner) {
            0 => (Error::Node as Cell, None),
            h => {
                *out = h;
                (OK, None)
            }
        },
        Err(crate::file::LoadError::Io(e)) => (Error::File as Cell, Some(format!("{path}: {e}"))),
        Err(crate::file::LoadError::Syntax(e)) => (
            Error::Syntax as Cell,
            Some(format!("{path}: {}", e.message)),
        ),
    };
    overwrite(pool, owner, old);
    result
}

/// `JSON_SaveFile(Node:node, const path[], bool:pretty)`. Returns once the text is in a
/// temporary file beside `path`; `saver` finishes the rest on its thread.
pub fn save_file(
    pool: &Pool,
    saver: &mut Saver,
    cp: Codepage,
    node: Cell,
    path: &[Cell],
    pretty: bool,
) -> (Cell, Option<String>) {
    let (d, id) = match found(pool, node) {
        Ok(at) => at,
        Err(e) => return (e as Cell, None),
    };
    let path = text::to_string(path, cp);
    match saver.save(pool.doc(d), id, &path, pretty) {
        Ok(()) => (OK, None),
        Err(e) => (Error::File as Cell, Some(format!("{path}: {e}"))),
    }
}

// Packing and unpacking by format. These return the code too, and for a bad format a
// line for the server log saying where it went wrong.

fn format_failure(f: crate::format::Failure, format: &str) -> (Cell, Option<String>) {
    let why = (f.error == Error::Format)
        .then(|| format!("the format \"{format}\" is wrong at character {}", f.at + 1));
    (f.error as Cell, why)
}

/// `JSON_Unpack(Node:node, const format[], ...)`.
pub fn unpack<A: crate::format::Args>(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    node: Cell,
    format: &[Cell],
    args: &mut A,
) -> (Cell, Option<String>) {
    text::with_text(format, cp, |f| {
        match crate::format::unpack(pool, cp, owner, node, f, args) {
            Ok(()) => (OK, None),
            Err(e) => format_failure(e, f),
        }
    })
}

/// `Node:JSON_Pack(const format[], ...)`. Returns the new node, or 0 along with the error
/// code.
pub fn pack<A: crate::format::Args>(
    pool: &mut Pool,
    cp: Codepage,
    owner: Owner,
    format: &[Cell],
    args: &A,
) -> (Cell, Cell, Option<String>) {
    text::with_text(format, cp, |f| {
        match crate::format::pack(pool, cp, owner, f, args) {
            Ok(h) => (h, OK, None),
            Err(e) => {
                let (code, why) = format_failure(e, f);
                (0, code, why)
            }
        }
    })
}

// Iteration, as in `while (JSON_Iter(list, row))`. A loop's position belongs to its
// element variable: the plugin keeps it, keyed by the script and the variable's address,
// so the script has no index to keep, and nothing done to the node in the variable -
// releasing it, or a copy of it going out of scope - can send the loop back to the
// start. A loop carries on while the variable still holds what the loop last put in it;
// anything else, such as the 0 of a new variable, starts a new loop. When a loop runs
// out, its element is released and the variable set back to 0.
//
// A loop goes as far as the container's length when it began. What it adds on the way
// isn't visited, so a loop that appends to its own array still ends; what it removes
// ends it early.
//
// A loop inside another that reuses the outer loop's variable is stacked on top of it.
// When the inner loop runs out, or the outer one is stepped again after a `break`, the
// variable holds the outer loop's element again and the outer loop carries on.

/// The loops in progress, keyed by script and element variable. Each variable has a stack,
/// innermost loop last.
#[derive(Default)]
pub struct Walks {
    loops: HashMap<(u32, Cell), Vec<Walk>>,
}

#[derive(Clone, Copy)]
struct Walk {
    /// What the loop last put in the variable.
    handle: Cell,
    doc: u32,
    container: NodeId,
    generation: u32,
    /// The index of that element, and where the loop stops.
    index: u32,
    end: u32,
}

impl Walks {
    /// Forgets the loops of the variables in `cells` cells from address `at`, the
    /// variables of a scope that is ending, and releases the elements the loops made.
    pub fn forget(&mut self, pool: &mut Pool, script: u32, at: Cell, cells: usize) {
        if self.loops.is_empty() {
            return;
        }
        let end = at as i64 + (cells * size_of::<Cell>()) as i64;
        self.loops.retain(|&(s, var), stack| {
            let gone = s == script && (at as i64..end).contains(&(var as i64));
            if gone {
                for w in stack.drain(..) {
                    release_element(pool, script, w.handle);
                }
            }
            !gone
        });
    }

    /// Forgets every loop of a script that is unloading. Its handles go with it.
    pub fn forget_script(&mut self, script: u32) {
        self.loops.retain(|&(s, _), _| s != script);
    }

    /// How many loops are in progress.
    pub fn len(&self) -> usize {
        self.loops.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }
}

/// Releases an element a loop made, unless the script kept it or it isn't the script's.
fn release_element(pool: &mut Pool, script: u32, h: Cell) {
    if pool.is_unkept_view(h) && pool.owner(h).is_some_and(|o| o.script == script) {
        pool.release(h);
    }
}

/// One step of the loop over `container` whose element variable is at address `var` and
/// holds `el`. Returns the index and node of the next element, now in `el`, or `None`
/// once the loop is done. `objects_only` is for `JSON_IterEntry`.
fn advance(
    pool: &mut Pool,
    walks: &mut Walks,
    owner: Owner,
    var: Cell,
    container: Cell,
    el: &mut Cell,
    objects_only: bool,
) -> Option<(usize, NodeId)> {
    let key = (owner.script, var);
    let holds_top = walks
        .loops
        .get(&key)
        .and_then(|s| s.last())
        .is_some_and(|w| w.handle == *el);
    // Walking a container into the variable that holds it would lose the container on
    // the first step, so that is refused and the variable left alone. When the variable
    // holds an outer loop's element, the inner loop gets a handle of its own and the
    // element stays, so walking it is fine.
    if container == *el && container != 0 && !holds_top {
        return None;
    }
    let Ok((d, node)) = found(pool, container) else {
        // The container is gone. If the variable holds what its loop last put there,
        // that loop is over.
        if holds_top {
            finish(pool, walks, key, el);
        }
        return None;
    };
    let generation = pool.doc(d).generation(node);
    let len = match pool.doc(d).val(node) {
        Val::Obj(m) => m.len(),
        Val::Arr(v) if !objects_only => v.len(),
        _ => 0,
    };
    let stack = walks.loops.entry(key).or_default();
    let this = |w: &Walk| (w.doc, w.container, w.generation) == (d, node, generation);
    let mut fresh_view = false;
    match stack.iter().rposition(this) {
        Some(k) if holds_top => {
            // This loop is under way. Loops stacked on it were left with `break`.
            if k + 1 < stack.len() {
                for w in stack.drain(k + 1..) {
                    release_element(pool, owner.script, w.handle);
                }
                *el = stack[k].handle;
            }
        }
        _ => {
            if holds_top {
                // A loop inside another, on the outer loop's variable: the outer
                // element stays as it is, for when this loop is done.
                fresh_view = true;
            } else {
                // The variable holds something else, so any loop it had is over.
                for w in stack.drain(..) {
                    if w.handle != *el {
                        release_element(pool, owner.script, w.handle);
                    }
                }
            }
            stack.push(Walk {
                handle: 0,
                doc: d,
                container: node,
                generation,
                index: u32::MAX,
                end: len as u32,
            });
        }
    }
    let w = *stack.last().expect("a loop was just found or pushed");
    let at = w.index.wrapping_add(1);
    if at >= w.end || at as usize >= len {
        finish(pool, walks, key, el);
        return None;
    }
    let id = match pool.doc(d).val(node) {
        Val::Arr(v) => v[at as usize],
        Val::Obj(m) => {
            *m.get_index(at as usize)
                .expect("an index below the length")
                .1
        }
        _ => unreachable!("only a container has a length"),
    };
    let old = *el;
    let h = if fresh_view {
        pool.view(d, id, owner)
    } else {
        pool.walk(old, d, id, owner)
    };
    if h == 0 {
        finish(pool, walks, key, el);
        return None;
    }
    if h != old && !fresh_view {
        overwrite(pool, owner, old);
    }
    *el = h;
    let top = walks
        .loops
        .get_mut(&key)
        .and_then(|s| s.last_mut())
        .expect("the loop is still there");
    top.handle = h;
    top.index = at;
    Some((at as usize, id))
}

/// Ends the innermost loop of a variable: its element is released, and the variable
/// gets back the element of the loop under it, or 0.
fn finish(pool: &mut Pool, walks: &mut Walks, key: (u32, Cell), el: &mut Cell) {
    let Some(stack) = walks.loops.get_mut(&key) else {
        *el = 0;
        return;
    };
    if let Some(w) = stack.pop()
        && w.handle != 0
    {
        release_element(pool, key.0, w.handle);
    }
    *el = stack.last().map_or(0, |w| w.handle);
    if stack.is_empty() {
        walks.loops.remove(&key);
    }
}

/// `JSON_Iter(Node:container, &Node:element)`. Steps to the next element of an array or
/// the next value of an object, and returns false once there are no more. `var` is the
/// address of the element variable.
pub fn iter(
    pool: &mut Pool,
    walks: &mut Walks,
    owner: Owner,
    var: Cell,
    container: Cell,
    el: &mut Cell,
) -> bool {
    advance(pool, walks, owner, var, container, el, false).is_some()
}

/// `JSON_IterEntry(Node:object, key[], &Node:value, size)`. Steps to the next key and its
/// value. A key longer than the buffer is cut to fit, and the walk carries on.
#[allow(clippy::too_many_arguments)]
pub fn iter_entry(
    pool: &mut Pool,
    walks: &mut Walks,
    cp: Codepage,
    owner: Owner,
    var: Cell,
    obj: Cell,
    key: &mut [Cell],
    el: &mut Cell,
) -> bool {
    let Some((at, _)) = advance(pool, walks, owner, var, obj, el, true) else {
        return false;
    };
    let (d, node) = found(pool, obj).expect("the object was just walked");
    if let Val::Obj(m) = pool.doc(d).val(node) {
        let (k, _) = m.get_index(at).expect("an index below the length");
        let _ = text::put(key, k, cp);
    }
    true
}

// Writing JSON text.

/// `JSON_Stringify(Node:node, buf[], size, bool:pretty)`. Writes the node out as JSON
/// text in the script's code page.
pub fn stringify(pool: &Pool, cp: Codepage, node: Cell, pretty: bool, out: &mut [Cell]) -> Cell {
    code(
        found(pool, node)
            .and_then(|(d, id)| text::put(out, &write::to_string(pool.doc(d), id, pretty), cp)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: Owner = Owner {
        script: 1,
        frame: 100,
    };

    fn s(text: &str) -> Vec<Cell> {
        text.bytes().map(|b| b as Cell).chain([0]).collect()
    }

    fn back(cells: &[Cell]) -> String {
        text::to_string(cells, Codepage::DEFAULT)
    }

    /// The address of a test's element variable. As in Pawn, each loop variable has its
    /// own.
    fn var(name: &str) -> Cell {
        match name {
            "row" => 400,
            "cell" => 404,
            "val" => 408,
            "el" => 412,
            "list" => 416,
            _ => panic!("no address for {name}"),
        }
    }

    fn root(pool: &mut Pool, json: &str) -> Cell {
        let mut out = 0;
        assert_eq!(parse(pool, Codepage::DEFAULT, ME, &s(json), &mut out), OK);
        out
    }

    const CP: Codepage = Codepage::DEFAULT;
    const ROW: &str = r#"{"model":411,"x":2165.9163,"name":"Infernus","on":true,"none":null,"big":4294967296,"pos":[1,2.5,"three"]}"#;

    #[test]
    fn typed_gets_read_their_kind_and_refuse_another() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        let mut v = 0;
        assert_eq!(get_int(&pool, CP, r, &s("model"), &mut v), OK);
        assert_eq!(v, 411);
        assert_eq!(get_float(&pool, CP, r, &s("x"), &mut v), OK);
        assert_eq!(f32::from_bits(v as u32), 2165.9163);
        assert_eq!(get_float(&pool, CP, r, &s("model"), &mut v), OK);
        assert_eq!(f32::from_bits(v as u32), 411.0);
        assert_eq!(get_bool(&pool, CP, r, &s("on"), &mut v), OK);
        assert_eq!(v, 1);
        let mut buf = [0; 32];
        assert_eq!(get_string(&pool, CP, r, &s("name"), &mut buf), OK);
        assert_eq!(back(&buf), "Infernus");
        v = -7;
        assert_eq!(
            get_int(&pool, CP, r, &s("name"), &mut v),
            Error::Type as Cell
        );
        assert_eq!(get_int(&pool, CP, r, &s("x"), &mut v), Error::Type as Cell);
        assert_eq!(
            get_int(&pool, CP, r, &s("nope"), &mut v),
            Error::Key as Cell
        );
        assert_eq!(v, -7, "a failed read leaves the variable");
        let mut pos = 0;
        assert_eq!(get_node(&mut pool, CP, ME, r, &s("pos"), &mut pos), OK);
        assert_eq!(
            get_int(&pool, CP, pos, &s("x"), &mut v),
            Error::Container as Cell
        );
    }

    #[test]
    fn the_type_of_a_missing_key_is_missing() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        assert_eq!(type_of(&pool, CP, r, &s("nope")), Type::Missing as Cell);
        assert_eq!(type_of(&pool, CP, r, &s("model")), Type::Int as Cell);
        assert_eq!(type_of(&pool, CP, r, &s("x")), Type::Float as Cell);
        assert_eq!(type_of(&pool, CP, r, &s("none")), Type::Null as Cell);
        assert_eq!(type_of(&pool, CP, r, &s("")), Type::Object as Cell);
        assert_ne!(Type::Missing as Cell, Type::Int as Cell);
    }

    #[test]
    fn the_type_of_a_dead_node_is_missing() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        pool.release(r);
        assert_eq!(type_of(&pool, CP, r, &s("")), Type::Missing as Cell);
        assert_eq!(type_of(&pool, CP, r, &s("model")), Type::Missing as Cell);
        assert_eq!(type_of(&pool, CP, 0, &s("")), Type::Missing as Cell);
    }

    #[test]
    fn a_short_buffer_answers_buffer_and_holds_the_cut_string() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        let mut buf = [-1; 5];
        assert_eq!(
            get_string(&pool, CP, r, &s("name"), &mut buf),
            Error::Buffer as Cell
        );
        assert_eq!(back(&buf), "Infe");
        let mut buf = [-1; 9];
        assert_eq!(get_string(&pool, CP, r, &s("name"), &mut buf), OK);
        let mut buf = [-1; 8];
        assert_eq!(
            stringify(&pool, CP, r, false, &mut buf),
            Error::Buffer as Cell
        );
        assert_eq!(back(&buf), "{\"model");
    }

    #[test]
    fn an_int_past_32_bits_answers_range_and_leaves_the_variable() {
        let mut pool = Pool::default();
        let r = root(
            &mut pool,
            r#"{"a":2147483648,"b":-2147483649,"c":1099511627776,"max":2147483647,"min":-2147483648}"#,
        );
        for key in ["a", "b", "c"] {
            let mut v = 42;
            assert_eq!(
                get_int(&pool, CP, r, &s(key), &mut v),
                Error::Range as Cell,
                "{key}"
            );
            assert_eq!(v, 42);
        }
        let mut v = 0;
        assert_eq!(get_int(&pool, CP, r, &s("max"), &mut v), OK);
        assert_eq!(v, i32::MAX);
        assert_eq!(get_int(&pool, CP, r, &s("min"), &mut v), OK);
        assert_eq!(v, i32::MIN);
    }

    #[test]
    fn every_read_answers_node_for_a_dead_handle() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        pool.release(r);
        let (mut v, mut buf, k) = (0, [0; 8], s("model"));
        let node = Error::Node as Cell;
        assert_eq!(get_int(&pool, CP, r, &k, &mut v), node);
        assert_eq!(get_float(&pool, CP, r, &k, &mut v), node);
        assert_eq!(get_bool(&pool, CP, r, &k, &mut v), node);
        assert_eq!(get_string(&pool, CP, r, &k, &mut buf), node);
        assert_eq!(get_node(&mut pool, CP, ME, r, &k, &mut v), node);
        assert_eq!(count(&pool, r, &mut v), node);
        assert_eq!(key_at(&pool, CP, r, 0, &mut buf), node);
        assert_eq!(element_node(&mut pool, ME, r, 0, &mut v), node);
        assert_eq!(element_int(&pool, r, 0, &mut v), node);
        assert_eq!(value_int(&pool, r, &mut v), node);
        assert_eq!(value_string(&pool, CP, r, &mut buf), node);
        assert_eq!(stringify(&pool, CP, r, false, &mut buf), node);
        assert_eq!(next(&mut pool, ME, r, &mut 0, &mut v), node);
        assert!(!has_key(&pool, CP, r, &k));
    }

    #[test]
    fn elements_and_values_read_and_never_free() {
        let mut pool = Pool::default();
        let r = root(&mut pool, ROW);
        let (mut pos, mut v, mut n) = (0, 0, 0);
        get_node(&mut pool, CP, ME, r, &s("pos"), &mut pos);
        assert_eq!(count(&pool, pos, &mut n), OK);
        assert_eq!(n, 3);
        assert_eq!(element_int(&pool, pos, 0, &mut v), OK);
        assert_eq!(v, 1);
        assert_eq!(element_float(&pool, pos, 1, &mut v), OK);
        assert_eq!(f32::from_bits(v as u32), 2.5);
        let mut buf = [0; 8];
        assert_eq!(element_string(&pool, CP, pos, 2, &mut buf), OK);
        assert_eq!(element_int(&pool, pos, 3, &mut v), Error::Key as Cell);
        assert_eq!(element_int(&pool, pos, -1, &mut v), Error::Key as Cell);
        let mut el = 0;
        assert_eq!(element_node(&mut pool, ME, pos, 0, &mut el), OK);
        for _ in 0..3 {
            assert_eq!(value_int(&pool, el, &mut v), OK);
        }
        assert_eq!(type_of(&pool, CP, el, &s("")), Type::Int as Cell);
        assert_eq!(pool.live_handles(), 3);
    }

    #[test]
    fn keys_and_walks_go_in_file_order() {
        let mut pool = Pool::default();
        let r = root(&mut pool, r#"{"z":1,"a":2,"m":3}"#);
        let mut buf = [0; 8];
        assert_eq!(key_at(&pool, CP, r, 1, &mut buf), OK);
        assert_eq!(back(&buf), "a");
        assert_eq!(key_at(&pool, CP, r, 3, &mut buf), Error::Key as Cell);
        let (mut cursor, mut keys) = (0, vec![]);
        while next_key(&pool, CP, r, &mut cursor, &mut buf) == OK {
            keys.push(back(&buf));
        }
        assert_eq!(keys, ["z", "a", "m"]);
        let (mut cursor, mut el, mut sum, mut v) = (0, 0, 0, 0);
        while next_entry(&mut pool, CP, ME, r, &mut cursor, &mut buf, &mut el) == OK {
            value_int(&pool, el, &mut v);
            sum = sum * 10 + v;
        }
        assert_eq!(sum, 123);
        assert_eq!(pool.live_handles(), 2, "the walk re-pointed one handle");
    }

    fn dump(pool: &Pool, h: Cell) -> String {
        let (doc, node) = pool.get(h).unwrap();
        write::to_string(doc, node, false)
    }

    #[test]
    fn sets_appends_and_removes_change_the_tree() {
        let mut pool = Pool::default();
        let o = object(&mut pool, ME);
        assert_eq!(set(&mut pool, CP, o, &s("model"), int_val(411)), OK);
        assert_eq!(
            set(
                &mut pool,
                CP,
                o,
                &s("x"),
                float_val(2165.9163f32.to_bits() as Cell)
            ),
            OK
        );
        assert_eq!(set(&mut pool, CP, o, &s("on"), bool_val(1)), OK);
        assert_eq!(
            set(&mut pool, CP, o, &s("name"), string_val(&s("Infernus"), CP)),
            OK
        );
        assert_eq!(set(&mut pool, CP, o, &s("none"), Val::Null), OK);
        let a = array(&mut pool, ME);
        assert_eq!(append(&mut pool, a, int_val(1)), OK);
        assert_eq!(append(&mut pool, a, int_val(2)), OK);
        assert_eq!(set_node(&mut pool, CP, o, &s("list"), a), OK);
        assert_eq!(
            dump(&pool, o),
            r#"{"model":411,"x":2165.9163,"on":true,"name":"Infernus","none":null,"list":[1,2]}"#
        );
        assert_eq!(remove(&mut pool, CP, o, &s("on")), OK);
        assert_eq!(remove(&mut pool, CP, o, &s("on")), Error::Key as Cell);
        assert_eq!(set(&mut pool, CP, o, &s("model"), int_val(560)), OK);
        assert_eq!(
            dump(&pool, o),
            r#"{"model":560,"x":2165.9163,"name":"Infernus","none":null,"list":[1,2]}"#
        );
        assert_eq!(
            append(&mut pool, a, int_val(3)),
            Error::Node as Cell,
            "the array went in, used up"
        );
        assert_eq!(append(&mut pool, o, int_val(3)), Error::Container as Cell);
        let live = array(&mut pool, ME);
        assert_eq!(
            set(&mut pool, CP, live, &s("k"), int_val(3)),
            Error::Container as Cell
        );
    }

    #[test]
    fn array_sets_and_removes_go_by_index() {
        let mut pool = Pool::default();
        let a = root(&mut pool, r#"[10,20,30]"#);
        let mut el = 0;
        assert_eq!(element_node(&mut pool, ME, a, 2, &mut el), OK);
        assert_eq!(array_set(&mut pool, a, 1, string_val(&s("x"), CP)), OK);
        assert_eq!(array_set(&mut pool, a, 3, int_val(0)), Error::Key as Cell);
        assert_eq!(array_set(&mut pool, a, -1, int_val(0)), Error::Key as Cell);
        assert_eq!(array_remove(&mut pool, a, 0), OK);
        assert_eq!(array_remove(&mut pool, a, 5), Error::Key as Cell);
        assert_eq!(dump(&pool, a), r#"["x",30]"#);
        let mut v = 0;
        assert_eq!(
            value_int(&pool, el, &mut v),
            OK,
            "a view keeps its element as others move"
        );
        assert_eq!(v, 30);
        assert_eq!(array_set(&mut pool, a, 1, int_val(31)), OK);
        assert_eq!(
            value_int(&pool, el, &mut v),
            Error::Node as Cell,
            "an overwritten element is gone"
        );
        assert_eq!(
            array_set_node(&mut pool, a, 0, a),
            OK,
            "an array set into itself is a copy"
        );
        assert_eq!(dump(&pool, a), r#"[["x",31],31]"#);
    }

    #[test]
    fn a_temporary_is_used_up_and_a_shared_one_copied() {
        let mut pool = Pool::default();
        let o = object(&mut pool, ME);
        let temp = array(&mut pool, ME);
        assert_eq!(set_node(&mut pool, CP, o, &s("a"), temp), OK);
        assert_eq!(pool.live_handles(), 1, "the temporary leaked nothing");
        let src = root(&mut pool, r#"{"k":[1]}"#);
        let mut k = 0;
        assert_eq!(get_node(&mut pool, CP, ME, src, &s("k"), &mut k), OK);
        assert_eq!(set_node(&mut pool, CP, o, &s("b"), k), OK);
        assert_eq!(set_node(&mut pool, CP, o, &s("c"), src), OK);
        assert_eq!(
            append(&mut pool, k, int_val(2)),
            OK,
            "a view is never used up"
        );
        assert_eq!(
            dump(&pool, src),
            r#"{"k":[1,2]}"#,
            "a document with other handles stays"
        );
        assert_eq!(dump(&pool, o), r#"{"a":[],"b":[1],"c":{"k":[1]}}"#);
        assert_eq!(append_node(&mut pool, o, o), Error::Container as Cell);
        assert_eq!(
            set_node(&mut pool, CP, o, &s("me"), o),
            OK,
            "a node set into itself is copied"
        );
        assert!(pool.resolve(o).is_some());
    }

    #[test]
    fn a_node_set_into_its_own_tree_is_copied_first() {
        let mut pool = Pool::default();
        let o = root(&mut pool, r#"{"a":{"b":1}}"#);
        let mut a = 0;
        assert_eq!(get_node(&mut pool, CP, ME, o, &s("a"), &mut a), OK);
        assert_eq!(set_node(&mut pool, CP, a, &s("self"), o), OK);
        assert_eq!(dump(&pool, o), r#"{"a":{"b":1,"self":{"a":{"b":1}}}}"#);
    }

    #[test]
    fn a_copy_is_a_document_of_its_own() {
        let mut pool = Pool::default();
        let o = root(&mut pool, r#"{"a":[1]}"#);
        let mut a = 0;
        get_node(&mut pool, CP, ME, o, &s("a"), &mut a);
        let c = copy(&mut pool, ME, a);
        assert_eq!(append(&mut pool, c, int_val(2)), OK);
        assert_eq!(
            (dump(&pool, c), dump(&pool, o)),
            ("[1,2]".into(), r#"{"a":[1]}"#.into())
        );
        pool.release(o);
        pool.release(a);
        assert_eq!(copy(&mut pool, ME, o), 0);
    }

    #[test]
    fn files_save_and_parse_back() {
        let dir = std::env::temp_dir().join("pawn-json-core-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("api.json").to_string_lossy().into_owned();
        let mut pool = Pool::default();
        let mut saver = Saver::default();
        let o = root(&mut pool, r#"{"z":1,"a":[true,null]}"#);
        assert_eq!(
            save_file(&pool, &mut saver, CP, o, &s(&path), true),
            (OK, None)
        );
        let mut back = 0;
        assert_eq!(
            parse_file(&mut pool, &saver, CP, ME, &s(&path), &mut back),
            (OK, None)
        );
        assert_eq!(dump(&pool, back), r#"{"z":1,"a":[true,null]}"#);
        let (code, why) = parse_file(
            &mut pool,
            &saver,
            CP,
            ME,
            &s(&format!("{path}.none")),
            &mut back,
        );
        assert_eq!((code, back), (Error::File as Cell, 0));
        assert!(why.unwrap().contains("api.json.none"));
    }

    // The mistakes a script makes most, held to what they must do: nothing piles up, and
    // nothing runs forever.

    #[test]
    fn parsing_into_one_variable_over_and_over_keeps_one_document() {
        let mut pool = Pool::default();
        let mut n = 0;
        for _ in 0..1000 {
            assert_eq!(parse(&mut pool, CP, ME, &s("{\"a\":[1,2,3]}"), &mut n), OK);
        }
        assert_eq!(pool.live_handles(), 1);
        // A failed parse empties the variable, and what it held goes too.
        assert_eq!(
            parse(&mut pool, CP, ME, &s("{"), &mut n),
            Error::Syntax as Cell
        );
        assert_eq!((n, pool.live_handles()), (0, 0));
    }

    #[test]
    fn reading_children_into_one_variable_keeps_one_handle() {
        let mut pool = Pool::default();
        let r = root(
            &mut pool,
            r#"{"list":[{"pos":[1]},{"pos":[2]}],"a":{"b":{}}}"#,
        );
        let mut v = 0;
        for i in 0..500 {
            assert_eq!(get_node(&mut pool, CP, ME, r, &s("a"), &mut v), OK);
            assert_eq!(path_node(&mut pool, CP, ME, r, &s("/a/b"), &mut v), OK);
            let mut list = 0;
            assert_eq!(get_node(&mut pool, CP, ME, r, &s("list"), &mut list), OK);
            assert_eq!(element_node(&mut pool, ME, list, i % 2, &mut v), OK);
            pool.release(list);
        }
        assert_eq!(pool.live_handles(), 2);
        // Reading a child into the variable that holds its parent: the parent's handle
        // goes, and the document stays for the child.
        let mut p = root(&mut pool, r#"{"c":{"x":7}}"#);
        assert_eq!(get_node(&mut pool, CP, ME, p, &s("c"), &mut p), OK);
        let mut x = 0;
        assert_eq!(get_int(&pool, CP, p, &s("x"), &mut x), OK);
        assert_eq!((x, pool.live_handles()), (7, 3));
    }

    #[test]
    fn a_kept_handle_or_another_script_s_is_never_taken_from_a_variable() {
        let mut pool = Pool::default();
        let other = Owner {
            script: 2,
            frame: 100,
        };
        let r = root(&mut pool, r#"{"a":1,"b":2}"#);
        let mut kept = 0;
        get_node(&mut pool, CP, ME, r, &s("a"), &mut kept);
        pool.set_kept(kept, true);
        let mut v = kept;
        assert_eq!(get_node(&mut pool, CP, ME, r, &s("b"), &mut v), OK);
        assert_ne!(v, kept);
        assert_eq!(dump(&pool, kept), "1");
        let mut theirs = 0;
        get_node(&mut pool, CP, other, r, &s("a"), &mut theirs);
        let mut mine = theirs;
        assert_eq!(get_node(&mut pool, CP, ME, r, &s("b"), &mut mine), OK);
        assert_ne!(mine, theirs);
        assert_eq!(dump(&pool, theirs), "1");
        let mut parsed = theirs;
        assert_eq!(parse(&mut pool, CP, ME, &s("[]"), &mut parsed), OK);
        assert_eq!(dump(&pool, theirs), "1");
    }

    #[test]
    fn a_walk_ends_even_when_the_loop_adds_to_what_it_walks() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let list = root(&mut pool, "[1,2,3]");
        let (mut el, mut steps) = (0, 0);
        while iter(&mut pool, &mut walks, ME, var("el"), list, &mut el) {
            append(&mut pool, list, int_val(0));
            steps += 1;
            assert!(steps <= 3, "the walk did not stop");
        }
        assert_eq!(steps, 3);
        let obj = root(&mut pool, r#"{"a":1}"#);
        let (mut el, mut steps) = (0, 0);
        while iter(&mut pool, &mut walks, ME, var("el"), obj, &mut el) {
            set(&mut pool, CP, obj, &s(&format!("k{steps}")), int_val(0));
            steps += 1;
            assert!(steps <= 1, "the walk did not stop");
        }
        let mut key = [0; 8];
        let (mut el, mut steps) = (0, 0);
        while iter_entry(
            &mut pool,
            &mut walks,
            CP,
            ME,
            var("el"),
            obj,
            &mut key,
            &mut el,
        ) {
            set(&mut pool, CP, obj, &s(&format!("e{steps}")), int_val(0));
            steps += 1;
            assert!(steps <= 2, "the walk did not stop");
        }
        assert_eq!(steps, 2);
        // A walk over an array that shrinks ends early instead of reading past it.
        let list = root(&mut pool, "[1,2,3,4,5]");
        let (mut el, mut steps) = (0, 0);
        while iter(&mut pool, &mut walks, ME, var("el"), list, &mut el) {
            array_remove(&mut pool, list, 0);
            steps += 1;
        }
        assert_eq!(steps, 3);
    }

    #[test]
    fn a_container_is_not_walked_into_its_own_variable() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let mut list = root(&mut pool, "[[1],[2]]");
        let held = list;
        assert!(!iter(
            &mut pool,
            &mut walks,
            ME,
            var("list"),
            list,
            &mut list
        ));
        assert_eq!(list, held);
        assert_eq!(dump(&pool, list), "[[1],[2]]");
    }

    /// A read straight after a save must see it, even when the thread has not got to it
    /// yet and even when the path is spelled differently.
    #[test]
    fn a_file_read_right_after_a_save_holds_what_was_saved() {
        let dir = std::env::temp_dir().join("pawn-json-core-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir
            .join("read-after-save.json")
            .to_string_lossy()
            .into_owned();
        let other = path.replace("read-after-save.json", "./read-after-save.json");
        let mut pool = Pool::default();
        let mut saver = Saver::default();
        for i in 0..50 {
            let o = root(&mut pool, &format!(r#"{{"n":{i}}}"#));
            assert_eq!(
                save_file(&pool, &mut saver, CP, o, &s(&path), false),
                (OK, None)
            );
            let mut back = 0;
            let spelled = if i % 2 == 0 { &path } else { &other };
            assert_eq!(
                parse_file(&mut pool, &saver, CP, ME, &s(spelled), &mut back),
                (OK, None)
            );
            assert_eq!(dump(&pool, back), format!(r#"{{"n":{i}}}"#));
            pool.release(back);
            pool.release(o);
        }
    }

    #[test]
    fn a_path_reads_deep_in_one_call() {
        let mut pool = Pool::default();
        let r = root(
            &mut pool,
            r#"{"vehicles":[{"model":400},{"model":401},{"model":402},{"model":411,"name":"Infernus"}]}"#,
        );
        let mut v = 0;
        assert_eq!(path_int(&pool, CP, r, &s("/vehicles/3/model"), &mut v), OK);
        assert_eq!(v, 411);
        let mut buf = [0; 16];
        assert_eq!(
            path_string(&pool, CP, r, &s("/vehicles/3/name"), &mut buf),
            OK
        );
        assert_eq!(back(&buf), "Infernus");
        assert_eq!(
            path_int(&pool, CP, r, &s("/vehicles/4/model"), &mut v),
            Error::Key as Cell
        );
        assert_eq!(
            path_int(&pool, CP, r, &s("vehicles"), &mut v),
            Error::Path as Cell
        );
        let mut third = 0;
        assert_eq!(
            path_node(&mut pool, CP, ME, r, &s("/vehicles/2"), &mut third),
            OK
        );
        assert_eq!(get_int(&pool, CP, third, &s("model"), &mut v), OK);
        assert_eq!(v, 402);
        assert_eq!(
            path_int(&pool, CP, third, &s(""), &mut v),
            Error::Type as Cell,
            "the empty pointer is the node"
        );
    }

    #[test]
    fn an_element_read_makes_no_slot() {
        let mut pool = Pool::default();
        let a = root(&mut pool, "[5,6.5,true]");
        let before = pool.live_handles();
        let mut v = 0;
        assert_eq!(element_int(&pool, a, 0, &mut v), OK);
        assert_eq!(element_float(&pool, a, 1, &mut v), OK);
        assert_eq!(element_bool(&pool, a, 2, &mut v), OK);
        assert_eq!(pool.live_handles(), before);
    }

    #[test]
    fn has_key_answers_for_present_absent_and_not_an_object() {
        let mut pool = Pool::default();
        let r = root(&mut pool, r#"{"a":null,"b":[1]}"#);
        assert!(
            has_key(&pool, CP, r, &s("a")),
            "a null value is still a key"
        );
        assert!(!has_key(&pool, CP, r, &s("c")));
        let mut b = 0;
        get_node(&mut pool, CP, ME, r, &s("b"), &mut b);
        assert!(!has_key(&pool, CP, b, &s("0")), "an array has no keys");
        pool.release(r);
        assert!(
            !has_key(&pool, CP, b, &s("x")),
            "its root released, the view still reads: an array, no keys"
        );
        assert!(!has_key(&pool, CP, r, &s("a")), "a dead node has none");
    }

    #[test]
    fn iter_walks_an_array_and_ends_clean() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let a = root(&mut pool, "[10,20,30]");
        let (mut row, mut sum, mut v) = (0, 0, 0);
        while iter(&mut pool, &mut walks, ME, var("row"), a, &mut row) {
            value_int(&pool, row, &mut v);
            sum = sum * 100 + v;
            assert_eq!(pool.live_handles(), 2, "one handle for the whole walk");
        }
        assert_eq!((sum, row, pool.live_handles()), (102030, 0, 1));
        let mut steps = 0;
        while iter(&mut pool, &mut walks, ME, var("row"), a, &mut row) {
            steps += 1;
        }
        assert_eq!(steps, 3, "the same variable walks again from the start");
    }

    #[test]
    fn iter_walks_an_object_s_values_and_an_empty_one_not_at_all() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let o = root(&mut pool, r#"{"a":1,"b":2}"#);
        let (mut val, mut sum, mut v) = (0, 0, 0);
        while iter(&mut pool, &mut walks, ME, var("val"), o, &mut val) {
            value_int(&pool, val, &mut v);
            sum += v;
        }
        assert_eq!(sum, 3);
        let e = root(&mut pool, "[]");
        assert!(!iter(&mut pool, &mut walks, ME, var("val"), e, &mut val));
        assert!(
            !iter(&mut pool, &mut walks, ME, var("val"), 0, &mut val),
            "no container, no walk"
        );
        let n = root(&mut pool, "5");
        assert!(!iter(&mut pool, &mut walks, ME, var("val"), n, &mut val));
    }

    #[test]
    fn a_kept_element_stays_and_the_walk_goes_on() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let a = root(&mut pool, "[1,2,3]");
        let (mut row, mut kept, mut seen, mut v) = (0, 0, vec![], 0);
        while iter(&mut pool, &mut walks, ME, var("row"), a, &mut row) {
            value_int(&pool, row, &mut v);
            seen.push(v);
            if v == 1 {
                pool.set_kept(row, true);
                kept = row;
            }
        }
        assert_eq!(seen, [1, 2, 3]);
        assert_eq!(value_int(&pool, kept, &mut v), OK);
        assert_eq!(v, 1);
        assert_eq!(pool.live_handles(), 2, "the root and the kept one");
    }

    #[test]
    fn iters_nest() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let grid = root(&mut pool, "[[1,2],[3]]");
        let (mut row, mut cell, mut sum, mut v) = (0, 0, 0, 0);
        while iter(&mut pool, &mut walks, ME, var("row"), grid, &mut row) {
            while iter(&mut pool, &mut walks, ME, var("cell"), row, &mut cell) {
                value_int(&pool, cell, &mut v);
                sum += v;
            }
        }
        assert_eq!((sum, pool.live_handles()), (6, 1));
    }

    #[test]
    fn a_loop_goes_on_when_its_element_is_released() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let a = root(&mut pool, "[1,2,3]");
        let (mut row, mut seen, mut v) = (0, vec![], 0);
        while iter(&mut pool, &mut walks, ME, var("row"), a, &mut row) {
            value_int(&pool, row, &mut v);
            seen.push(v);
            // As JSON_Release(row) does, or a copy of row in a block's own variable
            // when the block ends.
            pool.release(row);
            assert!(seen.len() <= 3, "the loop started over");
        }
        assert_eq!(seen, [1, 2, 3]);
        assert_eq!((row, pool.live_handles(), walks.len()), (0, 1, 0));
    }

    #[test]
    fn nested_loops_can_share_one_variable() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let grid = root(
            &mut pool,
            r#"[{"n":10,"items":[1,2]},{"n":20,"items":[3]}]"#,
        );
        let (mut row, mut items, mut sum, mut v, mut outer) = (0, 0, 0, 0, 0);
        while iter(&mut pool, &mut walks, ME, var("row"), grid, &mut row) {
            outer += 1;
            assert!(outer <= 2, "the outer loop started over");
            get_node(&mut pool, CP, ME, row, &s("items"), &mut items);
            while iter(&mut pool, &mut walks, ME, var("row"), items, &mut row) {
                value_int(&pool, row, &mut v);
                sum += v;
            }
            // Back on the outer element once the inner loop is done.
            assert_eq!(get_int(&pool, CP, row, &s("n"), &mut v), OK);
            sum += v;
        }
        assert_eq!((outer, sum, row, walks.len()), (2, 36, 0, 0));
        pool.release(items);
        assert_eq!(pool.live_handles(), 1);
    }

    #[test]
    fn an_outer_loop_goes_on_after_a_break_from_the_inner_one() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let grid = root(&mut pool, "[[1,2,3],[4,5,6]]");
        let (mut row, mut seen, mut v, mut outer) = (0, vec![], 0, 0);
        while iter(&mut pool, &mut walks, ME, var("row"), grid, &mut row) {
            outer += 1;
            assert!(outer <= 2, "the outer loop started over");
            let inner = row;
            // The container is the outer element, which isn't in the variable while the
            // inner loop runs; a copy of it is.
            let mut first = true;
            while iter(&mut pool, &mut walks, ME, var("row"), inner, &mut row) {
                value_int(&pool, row, &mut v);
                seen.push(v);
                if !first {
                    break;
                }
                first = false;
            }
        }
        assert_eq!(seen, [1, 2, 4, 5]);
        assert_eq!((row, walks.len(), pool.live_handles()), (0, 0, 1));
    }

    #[test]
    fn the_end_of_a_scope_forgets_its_loops() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let grid = root(&mut pool, "[[1,2],[3,4]]");
        let (mut row, mut cell) = (0, 0);
        assert!(iter(&mut pool, &mut walks, ME, var("row"), grid, &mut row));
        let inner = row;
        assert!(iter(&mut pool, &mut walks, ME, var("row"), inner, &mut row));
        assert!(iter(
            &mut pool,
            &mut walks,
            ME,
            var("cell"),
            grid,
            &mut cell
        ));
        assert_eq!((walks.len(), pool.live_handles()), (3, 4));
        // A scope with only `cell` in it ends, then one with `row`.
        walks.forget(&mut pool, ME.script, var("cell"), 1);
        assert_eq!((walks.len(), pool.live_handles()), (2, 3));
        walks.forget(&mut pool, ME.script, var("row") - 4, 2);
        assert_eq!((walks.len(), pool.live_handles()), (0, 1));
    }

    #[test]
    fn iter_entry_hands_out_keys_in_order() {
        let mut pool = Pool::default();
        let mut walks = Walks::default();
        let o = root(&mut pool, r#"{"zeta":1,"alpha":2}"#);
        let (mut val, mut key, mut keys) = (0, [0; 8], vec![]);
        while iter_entry(
            &mut pool,
            &mut walks,
            CP,
            ME,
            var("val"),
            o,
            &mut key,
            &mut val,
        ) {
            keys.push(back(&key));
        }
        assert_eq!(keys, ["zeta", "alpha"]);
        assert_eq!(val, 0);
    }

    #[test]
    fn a_bad_text_answers_syntax_and_node_zero() {
        let mut pool = Pool::default();
        let mut out = 99;
        assert_eq!(
            parse(&mut pool, CP, ME, &s("{\"a\":}"), &mut out),
            Error::Syntax as Cell
        );
        assert_eq!(out, 0);
    }

    #[test]
    fn stringify_writes_in_the_code_page() {
        let mut pool = Pool::default();
        let cp = Codepage::from_label("windows-1250").unwrap();
        let mut r = 0;
        let input: Vec<Cell> = b"{\"n\":\"\xF5\"}"
            .iter()
            .map(|&b| b as Cell)
            .chain([0])
            .collect();
        assert_eq!(parse(&mut pool, cp, ME, &input, &mut r), OK);
        let mut buf = [0; 16];
        assert_eq!(stringify(&pool, cp, r, false, &mut buf), OK);
        assert_eq!(&buf[..9], &input[..9]);
        assert_eq!(stringify(&pool, CP, r, false, &mut buf), OK);
        assert_eq!(buf[6], b'?' as Cell);
    }
}
