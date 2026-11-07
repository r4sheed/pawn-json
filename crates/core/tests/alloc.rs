// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Checks that reads do not touch the heap. The global allocator counts allocations, but
//! only on a thread that has asked it to, so other tests running at the same time do not
//! throw the count off.

use pawn_json_core::dom::{Doc, Val};
use pawn_json_core::pool::{Owner, Pool};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNT.try_with(|c| c.set(c.get().map(|n| n + 1)));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(Some(0)));
    f();
    COUNT.with(|c| c.replace(None)).unwrap()
}

const ME: Owner = Owner {
    script: 1,
    frame: 100,
};

#[test]
fn reading_a_child_copies_nothing() {
    let mut pool = Pool::default();
    let root = pool.root(
        Doc::parse(r#"{"row":{"name":"a long string value","x":1.5}}"#).unwrap(),
        ME,
    );
    let r = pool.resolve(root).unwrap();
    let Val::Obj(m) = pool.doc(r.doc).val(r.node) else {
        panic!()
    };
    let row = m["row"];
    // One view up front, so the pool's tables have already grown before the count starts.
    let warm = pool.view(r.doc, row, ME);
    pool.release(warm);

    let n = allocations(|| {
        let v = pool.view(r.doc, row, ME);
        let (doc, node) = pool.get(v).unwrap();
        let Val::Obj(cols) = doc.val(node) else {
            panic!()
        };
        assert!(matches!(doc.val(cols["name"]), Val::Str(s) if &**s == "a long string value"));
        pool.release(v);
    });
    assert_eq!(n, 0);
}

fn cells(bytes: &[u8]) -> Vec<i32> {
    bytes.iter().map(|&b| b as i32).chain([0]).collect()
}

#[test]
fn a_key_reads_with_no_allocation() {
    use pawn_json_core::text::{Codepage, with_text};
    let cp = Codepage::from_label("windows-1250").unwrap();
    let ascii = cells(b"vehicle_model");
    let latin2 = cells(b"sz\xE1m_\xF5");
    assert_eq!(
        allocations(|| assert_eq!(with_text(&ascii, cp, |k| k.len()), 13)),
        0
    );
    assert_eq!(
        allocations(|| assert!(with_text(&latin2, cp, |k| k == "sz\u{e1}m_\u{151}"))),
        0
    );
}

#[test]
fn a_typed_get_allocates_nothing() {
    use pawn_json_core::{api, text::Codepage};
    let mut pool = Pool::default();
    let cp = Codepage::DEFAULT;
    let mut row = 0;
    let text = cells(br#"{"model":411,"name":"a string column","x":1.5}"#);
    assert_eq!(api::parse(&mut pool, cp, ME, &text, &mut row), 0);
    let (model, name, x) = (cells(b"model"), cells(b"name"), cells(b"x"));
    let mut buf = [0; 32];
    let n = allocations(|| {
        let mut v = 0;
        assert_eq!(api::get_int(&pool, cp, row, &model, &mut v), 0);
        assert_eq!(api::get_float(&pool, cp, row, &x, &mut v), 0);
        assert_eq!(api::get_string(&pool, cp, row, &name, &mut buf), 0);
    });
    assert_eq!(n, 0);
}

#[test]
fn a_path_read_allocates_nothing() {
    use pawn_json_core::{api, text::Codepage};
    let mut pool = Pool::default();
    let cp = Codepage::DEFAULT;
    let mut root = 0;
    let text = cells(br#"{"vehicles":[{"model":400},{"m/odel":411}]}"#);
    assert_eq!(api::parse(&mut pool, cp, ME, &text, &mut root), 0);
    let (plain, escaped) = (cells(b"/vehicles/0/model"), cells(b"/vehicles/1/m~1odel"));
    let n = allocations(|| {
        let mut v = 0;
        assert_eq!(api::path_int(&pool, cp, root, &plain, &mut v), 0);
        assert_eq!(v, 400);
        assert_eq!(api::path_int(&pool, cp, root, &escaped, &mut v), 0);
        assert_eq!(v, 411);
    });
    assert_eq!(n, 0);
}
