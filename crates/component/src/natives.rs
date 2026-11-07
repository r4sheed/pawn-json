// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! The natives. Each one turns its arguments into slices and cells, calls the core's api,
//! and writes back whatever the api returned.

use crate::amx::{Amx, Args, Cell};
use crate::compat;
use crate::lifetime;
use crate::state::{self, Level, State};
use pawn_json_core::api;
use pawn_json_core::error::{Error, OK};
use pawn_json_core::pool::Owner;
use pawn_json_core::text::Codepage;
use std::ffi::c_char;

pub type Native = extern "C" fn(*mut Amx, *const Cell) -> Cell;

#[repr(C)]
pub struct NativeInfo {
    pub name: *const c_char,
    pub func: Native,
}

pub(crate) struct Call<'a> {
    pub args: &'a Args,
    pub owner: Owner,
    pub cp: Codepage,
}

/// Runs a native's body with its arguments and the calling script. If the call passed
/// fewer than `arity` arguments, or an address points outside the script, the native
/// returns `JSON_ERR_ADDRESS`.
pub(crate) fn run(
    amx: *mut Amx,
    params: *const Cell,
    arity: usize,
    body: impl FnOnce(&Call, &mut State) -> Result<Cell, Error>,
) -> Cell {
    let args = unsafe { Args::new(amx, params) };
    if args.count() < arity {
        return Error::Address as Cell;
    }
    state::with(|st| {
        let script = st.script(args.amx());
        let call = Call {
            args: &args,
            owner: Owner {
                script: script.id,
                frame: args.frame(),
            },
            cp: script.cp,
        };
        body(&call, st).unwrap_or_else(|e| e as Cell)
    })
}

/// `JSON_Parse(const text[], &Node:node)`
extern "C" fn json_parse(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let text = c.args.string(0).ok_or(Error::Address)?;
        let held = c.args.read(1).ok_or(Error::Address)?;
        let (code, node) = match api::parse_doc(text, c.cp) {
            Ok(doc) => match st.pool.root(doc, c.owner) {
                0 => {
                    st.tell_spent();
                    (Error::Node as Cell, 0)
                }
                h => (OK, h),
            },
            Err(e) => {
                st.log(Level::Warning, &format!("JSON_Parse: {}", e.message));
                (Error::Syntax as Cell, 0)
            }
        };
        if !c.args.write(1, node) {
            st.pool.release(node);
            return Err(Error::Address);
        }
        api::overwrite(&mut st.pool, c.owner, held);
        Ok(code)
    })
}

/// `JSON_GetInt(Node:node, const key[], &value)`
extern "C" fn json_get_int(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let key = c.args.string(1).ok_or(Error::Address)?;
        let mut v = 0;
        let code = api::get_int(&st.pool, c.cp, c.args.cell(0), key, &mut v);
        if code == OK && !c.args.write(2, v) {
            return Err(Error::Address);
        }
        Ok(code)
    })
}

/// `JSON@Free(const Node:nodes[], len)`, which the include's `operator~` calls when a
/// scope ends.
extern "C" fn json_free(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let len = usize::try_from(c.args.cell(1)).map_err(|_| Error::Address)?;
        let handles = c.args.cells(0, len).ok_or(Error::Address)?;
        let scope = c.args.data_cell(c.args.frame()).ok_or(Error::Address)?;
        let freed = lifetime::free_scope(&mut st.pool, c.owner.script, scope, handles);
        // The loops these variables ran die with them.
        st.walks
            .forget(&mut st.pool, c.owner.script, c.args.cell(0), len);
        Ok(freed as Cell)
    })
}

/// `JSON_Keep(Node:node)`. Stops the node being freed when its scope ends, so it lives
/// until `JSON_Release`.
extern "C" fn json_keep(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        Ok(if st.pool.set_kept(c.args.cell(0), true) {
            OK
        } else {
            Error::Node as Cell
        })
    })
}

/// `JSON_Release(Node:node)`. Frees the handle straight away, kept or not.
extern "C" fn json_release(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        Ok(if st.pool.release(c.args.cell(0)) {
            OK
        } else {
            Error::Node as Cell
        })
    })
}

/// `Node:JSON_Up(Node:node)`. Writing `return JSON_Up(node);` hands the node to the
/// caller.
extern "C" fn json_up(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        let h = c.args.cell(0);
        let caller = c.args.data_cell(c.args.frame()).ok_or(Error::Address)?;
        lifetime::hand_up(&mut st.pool, h, c.owner.script, c.owner.frame, caller);
        Ok(h)
    })
}

/// `JSON_CountNodes()`. The number of live handles, across every script.
extern "C" fn json_count_nodes(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 0, |_, st| Ok(st.pool.live_handles() as Cell))
}

fn string<'a>(c: &'a Call, i: usize) -> Result<&'a [Cell], Error> {
    c.args.string(i).ok_or(Error::Address)
}

fn put(c: &Call, i: usize, v: Cell) -> Result<(), Error> {
    if c.args.write(i, v) {
        Ok(())
    } else {
        Err(Error::Address)
    }
}

/// Passes a constructor's new handle through. A 0 may mean the pool has run out of
/// handles, and `tell_spent` logs that if so.
fn made(st: &mut State, h: Cell) -> Cell {
    if h == 0 {
        st.tell_spent();
    }
    h
}

fn overlaps(input: &[Cell], p: *mut Cell, n: usize) -> bool {
    let (a, b) = (
        input.as_ptr() as usize,
        input.as_ptr() as usize + size_of_val(input),
    );
    let (x, y) = (p as usize, p as usize + n * size_of::<Cell>());
    a < y && x < b
}

/// Runs `f` with the string argument at `input` (empty if `None`) and the output buffer
/// at `buf`, whose length is the argument at `size`. If the input and the buffer overlap,
/// the input is copied first so that writing the output can't corrupt it.
fn with_buffer(
    c: &Call,
    input: Option<usize>,
    buf: usize,
    size: usize,
    f: impl FnOnce(&[Cell], &mut [Cell]) -> Cell,
) -> Result<Cell, Error> {
    let len = usize::try_from(c.args.cell(size)).map_err(|_| Error::Address)?;
    let (p, n) = c.args.buffer(buf, len).ok_or(Error::Address)?;
    let input: &[Cell] = match input {
        Some(i) => string(c, i)?,
        None => &[],
    };
    let owned: Vec<Cell>;
    let input = if overlaps(input, p, n) {
        owned = input.to_vec();
        &owned[..]
    } else {
        input
    };
    let out = unsafe { std::slice::from_raw_parts_mut(p, n) };
    Ok(f(input, out))
}

/// Defines a native that reads the value at a key into a reference:
/// `(Node:node, const key[], &value)`.
macro_rules! get_by_key {
    ($name:ident, $read:path) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, 3, |c, st| {
                let mut v = 0;
                let code = $read(&st.pool, c.cp, c.args.cell(0), string(c, 1)?, &mut v);
                if code == OK {
                    put(c, 2, v)?;
                }
                Ok(code)
            })
        }
    };
}

/// Defines a native that reads a node's own value: `(Node:node, &value)`.
macro_rules! value_of {
    ($name:ident, $read:path) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, 2, |c, st| {
                let mut v = 0;
                let code = $read(&st.pool, c.args.cell(0), &mut v);
                if code == OK {
                    put(c, 1, v)?;
                }
                Ok(code)
            })
        }
    };
}

/// Defines a native that sets the value at a key: `(Node:node, const key[], value)`.
macro_rules! set_by_key {
    ($name:ident, $arity:expr, |$c:ident| $val:expr) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, $arity, |$c, st| {
                let v = $val;
                Ok(api::set(
                    &mut st.pool,
                    $c.cp,
                    $c.args.cell(0),
                    string($c, 1)?,
                    v,
                ))
            })
        }
    };
}

/// Defines a native that appends a value to an array: `(Node:array, value)`.
macro_rules! append {
    ($name:ident, $arity:expr, |$c:ident| $val:expr) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, $arity, |$c, st| {
                let v = $val;
                Ok(api::append(&mut st.pool, $c.args.cell(0), v))
            })
        }
    };
}

/// Defines a native that sets an array element: `(Node:array, index, value)`.
macro_rules! set_at {
    ($name:ident, $arity:expr, |$c:ident| $val:expr) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, $arity, |$c, st| {
                let v = $val;
                Ok(api::array_set(
                    &mut st.pool,
                    $c.args.cell(0),
                    $c.args.cell(1),
                    v,
                ))
            })
        }
    };
}

/// `Node:JSON_Copy(Node:node)`
extern "C" fn json_copy(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        let h = api::copy(&mut st.pool, c.owner, c.args.cell(0));
        Ok(if h == 0 && st.pool.resolve(c.args.cell(0)).is_some() {
            made(st, h)
        } else {
            h
        })
    })
}

/// `JSON_ParseFile(const path[], &Node:node)`
extern "C" fn json_parse_file(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let mut node = c.args.read(1).ok_or(Error::Address)?;
        let (code, why) = api::parse_file(
            &mut st.pool,
            &st.saver,
            c.cp,
            c.owner,
            string(c, 0)?,
            &mut node,
        );
        if let Some(why) = why {
            st.log(Level::Warning, &format!("JSON_ParseFile: {why}"));
        }
        if !c.args.write(1, node) {
            st.pool.release(node);
            return Err(Error::Address);
        }
        Ok(code)
    })
}

/// `JSON_SaveFile(const path[], Node:node, bool:pretty = false)`. The path comes first to
/// keep 1.x's argument order. The wait for the disk happens on the saver's thread; a
/// failure there is logged on the next server tick.
extern "C" fn json_save_file(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let (code, why) = api::save_file(
            &st.pool,
            &mut st.saver,
            c.cp,
            c.args.cell(1),
            string(c, 0)?,
            c.args.cell(2) != 0,
        );
        if let Some(why) = why {
            st.log(Level::Warning, &format!("JSON_SaveFile: {why}"));
        }
        Ok(code)
    })
}

/// `JSON_Flush()`. Waits until every save made so far is on the disk under its own name.
/// Answers `JSON_ERR_FILE` if one of them could not be finished; the server log says
/// which.
extern "C" fn json_flush(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 0, |_, st| {
        st.saver.flush();
        Ok(if st.report_failed_saves() {
            Error::File as Cell
        } else {
            OK
        })
    })
}

/// `JSON_Stringify(Node:node, output[], size = sizeof (output), bool:pretty = false)`
extern "C" fn json_stringify(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        with_buffer(c, None, 1, 2, |_, out| {
            api::stringify(&st.pool, c.cp, c.args.cell(0), c.args.cell(3) != 0, out)
        })
    })
}

/// `bool:JSON_SetCodepage(const name[])`. Sets the calling script's code page from its
/// WHATWG label, returning false for a label it doesn't support.
extern "C" fn json_set_codepage(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        let label = pawn_json_core::text::to_string(string(c, 0)?, c.cp);
        Ok(match Codepage::from_label(&label) {
            Some(cp) => {
                st.set_script_codepage(c.args.amx(), cp);
                1
            }
            None => 0,
        })
    })
}

get_by_key!(json_get_float, api::get_float);
get_by_key!(json_get_bool, api::get_bool);

/// `JSON_GetString(Node:node, const key[], output[], size = sizeof (output))`
extern "C" fn json_get_string(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        with_buffer(c, Some(1), 2, 3, |key, out| {
            api::get_string(&st.pool, c.cp, c.args.cell(0), key, out)
        })
    })
}

/// `JSON_GetNode(Node:node, const key[], &Node:child)`
extern "C" fn json_get_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let mut child = c.args.read(2).ok_or(Error::Address)?;
        let code = api::get_node(
            &mut st.pool,
            c.cp,
            c.owner,
            c.args.cell(0),
            string(c, 1)?,
            &mut child,
        );
        if code == OK && !c.args.write(2, child) {
            st.pool.release(child);
            return Err(Error::Address);
        }
        Ok(code)
    })
}

/// `JSON_TypeOf(Node:node, const key[] = "")`
extern "C" fn json_type_of(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        Ok(api::type_of(&st.pool, c.cp, c.args.cell(0), string(c, 1)?))
    })
}

/// `JSON_Count(Node:node, &count)`
extern "C" fn json_count(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let mut n = 0;
        let code = api::count(&st.pool, c.args.cell(0), &mut n);
        if code == OK {
            put(c, 1, n)?;
        }
        Ok(code)
    })
}

/// `JSON_KeyAt(Node:node, index, output[], size = sizeof (output))`
extern "C" fn json_key_at(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        with_buffer(c, None, 2, 3, |_, out| {
            api::key_at(&st.pool, c.cp, c.args.cell(0), c.args.cell(1), out)
        })
    })
}

value_of!(json_value_int, api::value_int);
value_of!(json_value_float, api::value_float);
value_of!(json_value_bool, api::value_bool);

/// `JSON_ValueString(Node:node, output[], size = sizeof (output))`
extern "C" fn json_value_string(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        with_buffer(c, None, 1, 2, |_, out| {
            api::value_string(&st.pool, c.cp, c.args.cell(0), out)
        })
    })
}

/// `JSON_Next(Node:container, &cursor, &Node:element)`
extern "C" fn json_next(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let mut cursor = c.args.read(1).ok_or(Error::Address)?;
        let mut el = c.args.read(2).ok_or(Error::Address)?;
        let code = api::next(&mut st.pool, c.owner, c.args.cell(0), &mut cursor, &mut el);
        put(c, 1, cursor)?;
        put(c, 2, el)?;
        Ok(code)
    })
}

/// `JSON_NextKey(Node:object, &cursor, key[], size = sizeof (key))`
extern "C" fn json_next_key(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        let mut cursor = c.args.read(1).ok_or(Error::Address)?;
        let code = with_buffer(c, None, 2, 3, |_, out| {
            api::next_key(&st.pool, c.cp, c.args.cell(0), &mut cursor, out)
        })?;
        put(c, 1, cursor)?;
        Ok(code)
    })
}

/// `JSON_NextEntry(Node:object, &cursor, &Node:value, key[], size = sizeof (key))`
extern "C" fn json_next_entry(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 5, |c, st| {
        let mut cursor = c.args.read(1).ok_or(Error::Address)?;
        let mut el = c.args.read(2).ok_or(Error::Address)?;
        let code = with_buffer(c, None, 3, 4, |_, out| {
            api::next_entry(
                &mut st.pool,
                c.cp,
                c.owner,
                c.args.cell(0),
                &mut cursor,
                out,
                &mut el,
            )
        })?;
        put(c, 1, cursor)?;
        put(c, 2, el)?;
        Ok(code)
    })
}

set_by_key!(json_set_int, 3, |c| api::int_val(c.args.cell(2)));
set_by_key!(json_set_float, 3, |c| api::float_val(c.args.cell(2)));
set_by_key!(json_set_bool, 3, |c| api::bool_val(c.args.cell(2)));
set_by_key!(json_set_string, 3, |c| api::string_val(string(c, 2)?, c.cp));
set_by_key!(json_set_null, 2, |c| pawn_json_core::dom::Val::Null);

/// `JSON_SetNode(Node:node, const key[], Node:value)`. Stores a copy of `value`, so the
/// script's handle to it is untouched.
extern "C" fn json_set_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        Ok(api::set_node(
            &mut st.pool,
            c.cp,
            c.args.cell(0),
            string(c, 1)?,
            c.args.cell(2),
        ))
    })
}

/// `JSON_Remove(Node:node, const key[])`
extern "C" fn json_remove(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        Ok(api::remove(
            &mut st.pool,
            c.cp,
            c.args.cell(0),
            string(c, 1)?,
        ))
    })
}

append!(json_append_int, 2, |c| api::int_val(c.args.cell(1)));
append!(json_append_float, 2, |c| api::float_val(c.args.cell(1)));
append!(json_append_bool, 2, |c| api::bool_val(c.args.cell(1)));
append!(json_append_string, 2, |c| api::string_val(
    string(c, 1)?,
    c.cp
));
append!(json_append_null, 1, |c| pawn_json_core::dom::Val::Null);

/// `JSON_AppendNode(Node:array, Node:value)`. Appends a copy of `value`.
extern "C" fn json_append_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        Ok(api::append_node(
            &mut st.pool,
            c.args.cell(0),
            c.args.cell(1),
        ))
    })
}

set_at!(json_array_set_int, 3, |c| api::int_val(c.args.cell(2)));
set_at!(json_array_set_float, 3, |c| api::float_val(c.args.cell(2)));
set_at!(json_array_set_bool, 3, |c| api::bool_val(c.args.cell(2)));
set_at!(json_array_set_string, 3, |c| api::string_val(
    string(c, 2)?,
    c.cp
));
set_at!(json_array_set_null, 2, |c| pawn_json_core::dom::Val::Null);

/// `JSON_ArraySetNode(Node:array, index, Node:value)`. Stores a copy of `value`.
extern "C" fn json_array_set_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        Ok(api::array_set_node(
            &mut st.pool,
            c.args.cell(0),
            c.args.cell(1),
            c.args.cell(2),
        ))
    })
}

/// `JSON_ArrayRemoveAt(Node:array, index)`
extern "C" fn json_array_remove(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        Ok(api::array_remove(
            &mut st.pool,
            c.args.cell(0),
            c.args.cell(1),
        ))
    })
}

/// Defines a native that reads an array element into a reference:
/// `(Node:array, index, &value)`. No handle is made.
macro_rules! get_at {
    ($name:ident, $read:path) => {
        extern "C" fn $name(amx: *mut Amx, params: *const Cell) -> Cell {
            run(amx, params, 3, |c, st| {
                let mut v = 0;
                let code = $read(&st.pool, c.args.cell(0), c.args.cell(1), &mut v);
                if code == OK {
                    put(c, 2, v)?;
                }
                Ok(code)
            })
        }
    };
}

get_by_key!(json_path_get_int, api::path_int);
get_by_key!(json_path_get_float, api::path_float);
get_by_key!(json_path_get_bool, api::path_bool);

/// `JSON_PathGetString(Node:node, const path[], output[], size = sizeof (output))`
extern "C" fn json_path_get_string(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        with_buffer(c, Some(1), 2, 3, |path, out| {
            api::path_string(&st.pool, c.cp, c.args.cell(0), path, out)
        })
    })
}

/// `JSON_PathGetNode(Node:node, const path[], &Node:child)`
extern "C" fn json_path_get_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let mut child = c.args.read(2).ok_or(Error::Address)?;
        let code = api::path_node(
            &mut st.pool,
            c.cp,
            c.owner,
            c.args.cell(0),
            string(c, 1)?,
            &mut child,
        );
        if code == OK && !c.args.write(2, child) {
            st.pool.release(child);
            return Err(Error::Address);
        }
        Ok(code)
    })
}

get_at!(json_array_get_int, api::element_int);
get_at!(json_array_get_float, api::element_float);
get_at!(json_array_get_bool, api::element_bool);

/// `JSON_ArrayGetString(Node:array, index, output[], size = sizeof (output))`
extern "C" fn json_array_get_string(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        with_buffer(c, None, 2, 3, |_, out| {
            api::element_string(&st.pool, c.cp, c.args.cell(0), c.args.cell(1), out)
        })
    })
}

/// `JSON_ArrayGetNode(Node:array, index, &Node:element)`. The element comes back as a
/// view into the array, not a copy.
extern "C" fn json_array_get_node(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let mut el = c.args.read(2).ok_or(Error::Address)?;
        let code = api::element_node(
            &mut st.pool,
            c.owner,
            c.args.cell(0),
            c.args.cell(1),
            &mut el,
        );
        if code == OK && !c.args.write(2, el) {
            st.pool.release(el);
            return Err(Error::Address);
        }
        Ok(code)
    })
}

/// `bool:JSON_HasKey(Node:node, const key[])`
extern "C" fn json_has_key(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        Ok(api::has_key(&st.pool, c.cp, c.args.cell(0), string(c, 1)?) as Cell)
    })
}

/// A variadic native's arguments from `first` on, in the shape the format code reads and
/// writes them. Pawn passes each one by address.
struct VarArgs<'a> {
    args: &'a Args,
    first: usize,
}

impl pawn_json_core::format::Args for VarArgs<'_> {
    fn count(&self) -> usize {
        self.args.count().saturating_sub(self.first)
    }
    fn cell(&self, i: usize) -> Option<Cell> {
        self.args.read(self.first + i)
    }
    fn string(&self, i: usize) -> Option<&[Cell]> {
        self.args.string(self.first + i)
    }
    fn set(&mut self, i: usize, v: Cell) -> bool {
        self.args.write(self.first + i, v)
    }
    fn set_string(&mut self, i: usize, size: usize, s: &str, cp: Codepage) -> Result<(), Error> {
        let (p, n) = self
            .args
            .buffer(self.first + i, size)
            .ok_or(Error::Address)?;
        pawn_json_core::text::put(unsafe { std::slice::from_raw_parts_mut(p, n) }, s, cp)
    }
}

/// `JSON_Unpack(Node:node, const format[], {Float, bool, Node, _}:...)`. The format is
/// copied up front, so writing an output can't change it while it is still being read.
extern "C" fn json_unpack(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let format = string(c, 1)?.to_vec();
        let mut args = VarArgs {
            args: c.args,
            first: 2,
        };
        let (code, why) = api::unpack(
            &mut st.pool,
            c.cp,
            c.owner,
            c.args.cell(0),
            &format,
            &mut args,
        );
        if let Some(why) = why {
            st.log(Level::Warning, &format!("JSON_Unpack: {why}"));
        }
        Ok(code)
    })
}

/// `Node:JSON_Pack(const format[], {Float, bool, Node, _}:...)`
extern "C" fn json_pack(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 1, |c, st| {
        let args = VarArgs {
            args: c.args,
            first: 1,
        };
        let (h, _, why) = api::pack(&mut st.pool, c.cp, c.owner, string(c, 0)?, &args);
        if let Some(why) = why {
            st.log(Level::Warning, &format!("JSON_Pack: {why}"));
        }
        Ok(made(st, h))
    })
}

/// `JSON_GetAs(Node:node, const key[], kind, size, {Float, bool, Node, _}:...)`, which the
/// `JSON_Get` macro expands to. `kind` comes from the include's `JSON@KIND`, worked out
/// from the variable's tag and size at compile time.
extern "C" fn json_get_as(amx: *mut Amx, params: *const Cell) -> Cell {
    const INT: Cell = 0;
    const FLOAT: Cell = 1;
    const BOOL: Cell = 2;
    const NODE: Cell = 4;
    const STRING: Cell = 8;
    run(amx, params, 5, |c, st| {
        let node = c.args.cell(0);
        let mut v = 0;
        let code = match c.args.cell(2) {
            STRING => {
                return with_buffer(c, Some(1), 4, 3, |key, out| {
                    api::get_string(&st.pool, c.cp, node, key, out)
                });
            }
            INT => api::get_int(&st.pool, c.cp, node, string(c, 1)?, &mut v),
            FLOAT => api::get_float(&st.pool, c.cp, node, string(c, 1)?, &mut v),
            BOOL => api::get_bool(&st.pool, c.cp, node, string(c, 1)?, &mut v),
            NODE => api::get_node(&mut st.pool, c.cp, c.owner, node, string(c, 1)?, &mut v),
            _ => Error::Type as Cell,
        };
        if code == OK && !c.args.write(4, v) {
            return Err(Error::Address);
        }
        Ok(code)
    })
}

/// `bool:JSON_Iter(Node:container, &Node:element)`
extern "C" fn json_iter(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 2, |c, st| {
        let mut el = c.args.read(1).ok_or(Error::Address)?;
        let more = api::iter(
            &mut st.pool,
            &mut st.walks,
            c.owner,
            c.args.cell(1),
            c.args.cell(0),
            &mut el,
        );
        put(c, 1, el)?;
        Ok(more as Cell)
    })
}

/// `bool:JSON_IterEntry(Node:object, key[], &Node:value, size = sizeof (key))`
extern "C" fn json_iter_entry(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 4, |c, st| {
        let mut el = c.args.read(2).ok_or(Error::Address)?;
        let more = with_buffer(c, None, 1, 3, |_, out| {
            api::iter_entry(
                &mut st.pool,
                &mut st.walks,
                c.cp,
                c.owner,
                c.args.cell(2),
                c.args.cell(0),
                out,
                &mut el,
            ) as Cell
        })?;
        put(c, 2, el)?;
        Ok(more)
    })
}

/// `bool:JSON_IterKey(Node:object, key[], size = sizeof (key))`. This loop has no handle
/// to remember its position in, so the plugin remembers it instead, keyed by the address
/// of the key buffer. An empty buffer starts a new loop, and the buffer is emptied again
/// when the keys run out.
extern "C" fn json_iter_key(amx: *mut Amx, params: *const Cell) -> Cell {
    run(amx, params, 3, |c, st| {
        let slot = (c.owner.script, c.args.cell(1));
        let size = usize::try_from(c.args.cell(2)).map_err(|_| Error::Address)?;
        let (p, n) = c.args.buffer(1, size).ok_or(Error::Address)?;
        let out = unsafe { std::slice::from_raw_parts_mut(p, n) };
        let fresh = out[0] == 0;
        let key = st.pool.resolve(c.args.cell(0)).and_then(|r| {
            let doc = st.pool.doc(r.doc);
            let generation = doc.generation(r.node);
            let pawn_json_core::dom::Val::Obj(m) = doc.val(r.node) else {
                return None;
            };
            // As with JSON_Iter, the walk stops at the key count it began with, so a loop
            // that adds keys still ends.
            let (at, end) = match st.key_walks.get(&slot) {
                Some(&(d, node, g, i, end))
                    if !fresh && (d, node, g) == (r.doc, r.node, generation) =>
                {
                    (i + 1, end)
                }
                _ => (0, m.len() as u32),
            };
            if at >= end {
                return None;
            }
            m.get_index(at as usize)
                .map(|(k, _)| (k.clone(), (r.doc, r.node, generation, at, end)))
        });
        match key {
            Some((k, walk)) => {
                let _ = pawn_json_core::text::put(out, &k, c.cp);
                st.key_walks.insert(slot, walk);
                Ok(1)
            }
            None => {
                st.key_walks.remove(&slot);
                out[0] = 0;
                Ok(0)
            }
        }
    })
}

struct Table(&'static [NativeInfo]);

// Safe to share across threads: the table is never written, and its names are static C
// strings.
unsafe impl Sync for Table {}

macro_rules! table {
    ($($name:literal => $func:path),* $(,)?) => {
        Table(&[$(NativeInfo { name: $name.as_ptr(), func: $func }),*])
    };
}

static NATIVES: Table = table![
    c"JSON_Parse" => json_parse,
    c"JSON_GetInt" => json_get_int,
    c"JSON@Free" => json_free,
    c"JSON_Keep" => json_keep,
    c"JSON_Release" => json_release,
    c"JSON_Up" => json_up,
    c"JSON_CountNodes" => json_count_nodes,
    c"JSON_Object" => compat::object,
    c"JSON_Array" => compat::array,
    c"JSON_Copy" => json_copy,
    c"JSON_ParseFile" => json_parse_file,
    c"JSON_SaveFile" => json_save_file,
    c"JSON_Flush" => json_flush,
    c"JSON_Stringify" => json_stringify,
    c"JSON_SetCodepage" => json_set_codepage,
    c"JSON_GetFloat" => json_get_float,
    c"JSON_GetBool" => json_get_bool,
    c"JSON_GetString" => json_get_string,
    c"JSON_GetNode" => json_get_node,
    c"JSON_TypeOf" => json_type_of,
    c"JSON_Count" => json_count,
    c"JSON_KeyAt" => json_key_at,
    c"JSON_ValueInt" => json_value_int,
    c"JSON_ValueFloat" => json_value_float,
    c"JSON_ValueBool" => json_value_bool,
    c"JSON_ValueString" => json_value_string,
    c"JSON_Next" => json_next,
    c"JSON_NextKey" => json_next_key,
    c"JSON_NextEntry" => json_next_entry,
    c"JSON_SetInt" => json_set_int,
    c"JSON_SetFloat" => json_set_float,
    c"JSON_SetBool" => json_set_bool,
    c"JSON_SetString" => json_set_string,
    c"JSON_SetNull" => json_set_null,
    c"JSON_SetNode" => json_set_node,
    c"JSON_Remove" => json_remove,
    c"JSON_AppendInt" => json_append_int,
    c"JSON_AppendFloat" => json_append_float,
    c"JSON_AppendBool" => json_append_bool,
    c"JSON_AppendString" => json_append_string,
    c"JSON_AppendNull" => json_append_null,
    c"JSON_AppendNode" => json_append_node,
    c"JSON_ArraySetInt" => json_array_set_int,
    c"JSON_ArraySetFloat" => json_array_set_float,
    c"JSON_ArraySetBool" => json_array_set_bool,
    c"JSON_ArraySetString" => json_array_set_string,
    c"JSON_ArraySetNull" => json_array_set_null,
    c"JSON_ArraySetNode" => json_array_set_node,
    c"JSON_ArrayRemoveAt" => json_array_remove,
    c"JSON_PathGetInt" => json_path_get_int,
    c"JSON_PathGetFloat" => json_path_get_float,
    c"JSON_PathGetBool" => json_path_get_bool,
    c"JSON_PathGetString" => json_path_get_string,
    c"JSON_PathGetNode" => json_path_get_node,
    c"JSON_ArrayGetInt" => json_array_get_int,
    c"JSON_ArrayGetFloat" => json_array_get_float,
    c"JSON_ArrayGetBool" => json_array_get_bool,
    c"JSON_ArrayGetString" => json_array_get_string,
    c"JSON_ArrayGetNode" => json_array_get_node,
    c"JSON_HasKey" => json_has_key,
    c"JSON_Unpack" => json_unpack,
    c"JSON_Pack" => json_pack,
    c"JSON_GetAs" => json_get_as,
    c"JSON_Iter" => json_iter,
    c"JSON_IterKey" => json_iter_key,
    c"JSON_IterEntry" => json_iter_entry,
    c"JSON_Append" => compat::append,
    c"JSON_GetObject" => compat::get_object,
    c"JSON_GetArray" => compat::get_array,
    c"JSON_ArrayObject" => compat::array_object,
    c"JSON_ArrayIterate" => compat::array_iterate,
    c"JSON_ArrayRemove" => compat::array_remove,
    c"JSON_ToggleGC" => compat::toggle_gc,
    c"JSON_Cleanup" => compat::cleanup,
];

pub extern "C" fn natives(count: *mut i32) -> *const NativeInfo {
    unsafe { *count = NATIVES.0.len() as i32 };
    NATIVES.0.as_ptr()
}

#[cfg(test)]
pub fn names() -> impl Iterator<Item = &'static str> {
    NATIVES.0.iter().map(|n| {
        unsafe { std::ffi::CStr::from_ptr(n.name) }
            .to_str()
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amx::fake::Script;

    #[test]
    fn parse_and_get_int_run_through_the_amx() {
        let mut s = Script::new(256);
        let text = s.string(br#"{"model":411,"big":4294967296}"#);
        let node = s.put(&[-1]);
        assert_eq!(s.call(json_parse, &[text, node]), OK);
        let h = s.cell_at(node);
        assert!(h > 0);
        let (key, big, v) = (s.string(b"model"), s.string(b"big"), s.put(&[-7]));
        assert_eq!(s.call(json_get_int, &[h, key, v]), OK);
        assert_eq!(s.cell_at(v), 411);
        assert_eq!(s.call(json_get_int, &[h, big, v]), Error::Range as Cell);
        assert_eq!(s.cell_at(v), 411, "a failed read leaves the variable");
    }

    #[test]
    fn a_bad_address_or_a_missing_argument_answers_address() {
        let mut s = Script::new(64);
        let text = s.string(b"{}");
        assert_eq!(s.call(json_parse, &[text, 1 << 20]), Error::Address as Cell);
        assert_eq!(s.call(json_parse, &[text]), Error::Address as Cell);
        assert_eq!(s.call(json_parse, &[-8, 4]), Error::Address as Cell);
        let out = s.put(&[5]);
        let bad = s.string(b"{");
        assert_eq!(s.call(json_parse, &[bad, out]), Error::Syntax as Cell);
        assert_eq!(s.cell_at(out), 0);
    }
}
