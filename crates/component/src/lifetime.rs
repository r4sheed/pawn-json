// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! A node lives until the scope that holds it ends. Each handle records where it was
//! made: the script, and the frame of the Pawn function whose native call made it.
//!
//! The include's `operator~` runs in a frame of its own, and the saved frame at `[frm]`
//! is the function whose scope is ending. The AMX stack grows down, so any frame at or
//! below that one belongs to the ending function or to a callee that has already
//! returned, and a handle made from such a frame is freed. That gives the behaviour you
//! would expect: a node passed down to a helper, or to a hook through CallLocalFunction,
//! outlives the call because its maker sits higher up the stack, while a node made in the
//! helper dies with the helper, or with the caller it was returned to.

use pawn_json_core::pool::{Cell, Pool};

/// Applies the `operator~` rule to the handles in the ending scope's variables, and
/// returns how many it freed. Kept handles and other scripts' handles are left alone.
pub fn free_scope(pool: &mut Pool, script: u32, scope: Cell, handles: &[Cell]) -> usize {
    let mut freed = 0;
    for &h in handles {
        let mine = pool
            .owner(h)
            .is_some_and(|o| o.script == script && o.frame <= scope);
        if mine && !pool.kept(h) {
            pool.release(h);
            freed += 1;
        }
    }
    freed
}

/// Backs `JSON_Up`. A node the calling function made is handed to that function's caller,
/// so the scope that ends in between won't free it. A node made further up the stack
/// already outlives the call, so it stays with its maker.
pub fn hand_up(pool: &mut Pool, h: Cell, script: u32, frame: Cell, caller: Cell) {
    if pool
        .owner(h)
        .is_some_and(|o| o.script == script && o.frame == frame)
    {
        pool.set_owner(
            h,
            pawn_json_core::pool::Owner {
                script,
                frame: caller,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pawn_json_core::dom::{Doc, Val};
    use pawn_json_core::pool::Owner;

    // The stack grows down: main at 1000 calls helper at 900, which calls inner at 800.
    const MAIN: Cell = 1000;
    const HELPER: Cell = 900;
    const INNER: Cell = 800;

    fn make(pool: &mut Pool, frame: Cell) -> Cell {
        pool.root(Doc::new(Val::Null), Owner { script: 1, frame })
    }

    fn live(pool: &Pool, h: Cell) -> bool {
        pool.resolve(h).is_some()
    }

    #[test]
    fn a_helper_s_end_leaves_its_argument_and_frees_its_own() {
        let mut pool = Pool::default();
        let arg = make(&mut pool, MAIN);
        let own = make(&mut pool, HELPER);
        assert_eq!(free_scope(&mut pool, 1, HELPER, &[arg, own]), 1);
        assert!(live(&pool, arg) && !live(&pool, own));
        assert_eq!(free_scope(&mut pool, 1, MAIN, &[arg]), 1);
        assert!(!live(&pool, arg));
    }

    #[test]
    fn a_node_a_returned_callee_made_dies_with_the_caller_s_scope() {
        let mut pool = Pool::default();
        let returned = make(&mut pool, INNER);
        assert_eq!(
            free_scope(&mut pool, 1, MAIN, &[returned]),
            1,
            "1.x's helper that returns a node, unchanged"
        );
    }

    #[test]
    fn a_kept_node_and_another_script_s_are_left() {
        let mut pool = Pool::default();
        let kept = make(&mut pool, MAIN);
        pool.set_kept(kept, true);
        let theirs = pool.root(
            Doc::new(Val::Null),
            Owner {
                script: 2,
                frame: MAIN,
            },
        );
        assert_eq!(
            free_scope(&mut pool, 1, MAIN, &[kept, theirs, 0, -1, 12345]),
            0
        );
        assert!(live(&pool, kept) && live(&pool, theirs));
    }

    #[test]
    fn up_moves_a_node_one_frame_and_only_from_its_maker() {
        let mut pool = Pool::default();
        let made = make(&mut pool, INNER);
        hand_up(&mut pool, made, 1, INNER, HELPER);
        assert_eq!(pool.owner(made).unwrap().frame, HELPER);
        hand_up(&mut pool, made, 1, HELPER, MAIN);
        assert_eq!(
            free_scope(&mut pool, 1, HELPER, &[made]),
            0,
            "handed past the helper's scope"
        );
        assert_eq!(pool.owner(made).unwrap().frame, MAIN);

        let from_above = make(&mut pool, MAIN);
        hand_up(&mut pool, from_above, 1, INNER, HELPER);
        assert_eq!(
            pool.owner(from_above).unwrap().frame,
            MAIN,
            "a node from above stays with its maker"
        );
    }
}
