// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! pawn-json as an open.mp component. The C++ shell in `cpp/component.cpp` talks to the
//! server, and this crate turns each native call into a call on `pawn-json-core`.

mod amx;
mod compat;
mod lifetime;
mod natives;
mod state;

use std::ffi::c_void;

/// Everything the plugin allocates goes through mimalloc. Parsing a document is mostly
/// allocation, and the system heap made it noticeably slower. Nothing allocated here is
/// ever freed outside the plugin, so the choice of allocator stays private to it.
#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// The functions the C++ shell calls into. This must match `PjApi` in
/// `cpp/component.cpp` field for field.
#[repr(C)]
struct Api {
    natives: extern "C" fn(*mut i32) -> *const natives::NativeInfo,
    script_loaded: extern "C" fn(*mut amx::Amx),
    script_unloaded: extern "C" fn(*mut amx::Amx),
    set_codepage: extern "C" fn(*const u8, usize) -> bool,
    set_logger: extern "C" fn(*mut c_void, state::LogFn),
}

extern "C" fn script_loaded(amx: *mut amx::Amx) {
    state::with(|st| st.loaded(amx));
}

extern "C" fn script_unloaded(amx: *mut amx::Amx) {
    state::with(|st| st.unloaded(amx));
}

extern "C" fn set_codepage(label: *const u8, len: usize) -> bool {
    let label = unsafe { std::slice::from_raw_parts(label, len) };
    let cp = std::str::from_utf8(label)
        .ok()
        .and_then(pawn_json_core::text::Codepage::from_label);
    if let Some(cp) = cp {
        state::with(|st| st.set_default_codepage(cp));
    }
    cp.is_some()
}

extern "C" fn set_logger(ctx: *mut c_void, log: state::LogFn) {
    state::with(|st| st.set_logger(ctx, log));
}

static API: Api = Api {
    natives: natives::natives,
    script_loaded,
    script_unloaded,
    set_codepage,
    set_logger,
};

unsafe extern "C" {
    fn pj_component(api: *const Api) -> *mut c_void;
}

/// The only symbol the library exports. open.mp calls it once to get the component.
#[unsafe(no_mangle)]
pub extern "C" fn ComponentEntryPoint() -> *mut c_void {
    unsafe { pj_component(&API) }
}

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: tests::Counting = tests::Counting;

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Counts the allocations a thread makes while it is inside `allocations`, so a test
    /// can check that a read allocates nothing.
    pub struct Counting;

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

    pub fn allocations(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(Some(0)));
        f();
        COUNT.with(|c| c.replace(None)).unwrap()
    }

    #[test]
    fn the_include_declares_every_registered_native() {
        let inc = include_str!("../../../include/json.inc");
        // The name a `native` line registers. A line like `native Foo(...) = Bar;`
        // declares Foo for Pawn but registers Bar, so the name after the `=` wins.
        let registers = |l: &'static str| -> Option<&'static str> {
            let l = l.strip_prefix("native ")?;
            if let Some((_, alias)) = l.rsplit_once(") = ") {
                return Some(alias.trim_end_matches(';').trim());
            }
            l.split_once('(')
                .map(|(head, _)| head.rsplit(':').next().unwrap())
        };
        let mut declared: Vec<&str> = inc.lines().filter_map(registers).collect();
        declared.sort();
        declared.dedup();
        let registered: Vec<&str> = super::natives::names().collect();
        for name in &registered {
            assert!(
                declared.contains(name),
                "{name} is registered but not in include/json.inc"
            );
        }
        for name in &declared {
            assert!(
                registered.contains(name),
                "{name} is declared but not registered"
            );
        }
    }
}
