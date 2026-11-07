// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! A bug in the plugin should cost one call, not the server. Everything the server or a
//! script calls into runs inside `catch`: a panic stops there, goes to the server log
//! with the place it happened, and the call returns a fallback - `JSON_ERR_INTERNAL` for a
//! native that returns a code, 0 for the rest - so the script carries on.

use crate::state::{self, Level};
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

thread_local! {
    /// What the last panic on this thread said, and where.
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
    /// How many panics `catch` has stopped on this thread.
    static CAUGHT: Cell<usize> = const { Cell::new(0) };
}

/// How many panics `catch` has stopped on this thread. The tests read it to see that
/// none happened.
#[cfg(test)]
pub fn caught() -> usize {
    CAUGHT.with(Cell::get)
}

/// Replaces the default panic message on stderr with one kept for the log. Called once,
/// when the server loads the component.
pub fn install_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let what = message(info.payload());
            let place = info
                .location()
                .map(|l| format!(" at {}:{}", l.file(), l.line()))
                .unwrap_or_default();
            LAST.with(|last| *last.borrow_mut() = Some(format!("{what}{place}")));
        }));
    });
}

fn message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "no message".to_owned()
    }
}

/// Runs `f`, and if it panics, logs the panic and returns `fallback` instead.
pub fn catch<R>(fallback: R, f: impl FnOnce() -> R) -> R {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            CAUGHT.with(|c| c.set(c.get() + 1));
            let what = LAST
                .with(|last| last.borrow_mut().take())
                .unwrap_or_else(|| message(&*payload));
            state::try_log(
                Level::Error,
                &format!(
                    "a call failed inside the plugin and was stopped there: {what}. The script goes on; please report this."
                ),
            );
            fallback
        }
    }
}
