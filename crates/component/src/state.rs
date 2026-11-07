// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! What the component keeps between native calls: the handle pool, the saver, each
//! script's id and code page, and the server's logger. open.mp runs every script on its main thread and
//! nothing else calls in here, so a thread-local is all the state needs.

use crate::amx::{Amx, Cell};
use pawn_json_core::api::Walks;
use pawn_json_core::dom::NodeId;
use pawn_json_core::file::Saver;
use pawn_json_core::pool::Pool;
use pawn_json_core::text::Codepage;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

pub type LogFn = extern "C" fn(ctx: *mut c_void, level: i32, msg: *const u8, len: usize);

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Level {
    Warning = 2,
    Error = 3,
}

#[derive(Clone, Copy)]
pub struct Script {
    pub id: u32,
    pub cp: Codepage,
}

/// How far a `JSON_IterKey` loop has got: the object's document, node and generation, the
/// index of the last key it handed out, and the key count when the loop began.
pub type KeyWalk = (u32, NodeId, u32, u32, u32);

pub struct State {
    pub pool: Pool,
    pub saver: Saver,
    /// The `JSON_Iter` and `JSON_IterEntry` loops in progress.
    pub walks: Walks,
    /// The `JSON_IterKey` loops in progress, keyed by script and key buffer address.
    pub key_walks: HashMap<(u32, Cell), KeyWalk>,
    scripts: HashMap<usize, Script>,
    next_id: u32,
    default_cp: Codepage,
    logger: Option<(*mut c_void, LogFn)>,
    told_spent: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        pool: Pool::default(),
        saver: Saver::default(),
        walks: Walks::default(),
        key_walks: HashMap::new(),
        scripts: HashMap::new(),
        next_id: 1,
        default_cp: Codepage::DEFAULT,
        logger: None,
        told_spent: false,
    });
}

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Logs a line if the state can be reached right now, and quietly doesn't if it can't,
/// so reporting a failure can never fail in turn.
pub fn try_log(level: Level, msg: &str) {
    let _ = STATE.try_with(|s| {
        if let Ok(st) = s.try_borrow() {
            st.log(level, msg);
        }
    });
}

impl State {
    /// Looks up the calling script. A script whose load was never announced is registered
    /// on its first call.
    pub fn script(&mut self, amx: *mut Amx) -> Script {
        let (next, cp) = (self.next_id, self.default_cp);
        let s = *self
            .scripts
            .entry(amx as usize)
            .or_insert(Script { id: next, cp });
        if s.id == next {
            self.next_id += 1;
        }
        s
    }

    pub fn loaded(&mut self, amx: *mut Amx) {
        self.script(amx);
    }

    /// Frees every handle the script owns and forgets its loops.
    pub fn unloaded(&mut self, amx: *mut Amx) {
        if let Some(s) = self.scripts.remove(&(amx as usize)) {
            self.pool.release_script(s.id);
            self.walks.forget_script(s.id);
            self.key_walks.retain(|&(script, _), _| script != s.id);
        }
    }

    pub fn set_script_codepage(&mut self, amx: *mut Amx, cp: Codepage) {
        self.script(amx);
        if let Some(s) = self.scripts.get_mut(&(amx as usize)) {
            s.cp = cp;
        }
    }

    pub fn set_default_codepage(&mut self, cp: Codepage) {
        self.default_cp = cp;
    }

    pub fn set_logger(&mut self, ctx: *mut c_void, log: LogFn) {
        self.logger = Some((ctx, log));
    }

    pub fn log(&self, level: Level, msg: &str) {
        if let Some((ctx, log)) = self.logger {
            log(ctx, level as i32, msg.as_ptr(), msg.len());
        }
    }

    /// Logs each save that failed on the saver's thread since the last call, and says
    /// whether there was one.
    pub fn report_failed_saves(&self) -> bool {
        let failures = self.saver.take_failures();
        for why in &failures {
            self.log(Level::Warning, &format!("JSON_SaveFile: {why}"));
        }
        !failures.is_empty()
    }

    /// Logs it if a loop variable ran out of steps on this call.
    pub fn report_stopped_loop(&mut self, native: &str) {
        if self.walks.take_stopped() {
            self.log(
                Level::Error,
                &format!(
                    "{native}: a loop variable kept starting over on the same container, more than {} steps in one server tick, so its loop was stopped: it was going round for ever. Look for an inner loop over the same container on the same variable, or the variable set to 0 inside the loop.",
                    pawn_json_core::api::STEP_BUDGET
                ),
            );
        }
    }

    /// Logs that the pool has run out of handles, but only the first time. That means the
    /// scripts hold a million nodes at once, or this run has made 2^31 of them.
    pub fn tell_spent(&mut self) {
        if !self.told_spent && self.pool.spent() {
            self.told_spent = true;
            self.log(Level::Error, "no handle is free, so a new node is answered 0: the scripts hold a million nodes, or this run has made 2^31");
        }
    }
}
