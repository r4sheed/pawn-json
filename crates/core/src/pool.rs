// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

use crate::dom::{Doc, NodeId};

pub type Cell = i32;

const INDEX_BITS: u32 = 20;
const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;
const GENERATIONS: u16 = 1 << 11;

/// Who made a handle: the component's id for the script, and the AMX frame of the Pawn
/// function that called the native. The core only stores this; the component uses the
/// frame to free a handle when that function's scope ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Owner {
    pub script: u32,
    pub frame: Cell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref {
    pub doc: u32,
    pub node: NodeId,
}

/// How far a `JSON_Iter` walk has got: the container it is walking and the index it is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walk {
    pub container: NodeId,
    pub index: u32,
}

struct Live {
    doc: u32,
    node: NodeId,
    node_generation: u32,
    owner: Owner,
    kept: bool,
    view: bool,
    walk: Option<Walk>,
}

struct Slot {
    generation: u16,
    live: Option<Live>,
}

struct DocSlot {
    doc: Doc,
    handles: u32,
}

/// The handles the scripts hold. A handle is `index | generation << 20`, which is always
/// positive, so 0 can mean "no node" everywhere. Once a slot has used up its 2048
/// generations it is retired, so a stale handle never names another node. The price is a
/// limit of 2^31 handles made per server run.
pub struct Pool {
    slots: Vec<Slot>,
    free: Vec<u32>,
    docs: Vec<Option<DocSlot>>,
    doc_free: Vec<u32>,
    limit: u32,
    live: usize,
}

impl Default for Pool {
    fn default() -> Pool {
        Pool::with_limit(INDEX_MASK)
    }
}

impl Pool {
    pub fn with_limit(limit: u32) -> Pool {
        Pool {
            // Slot 0 is never handed out, since its first handle would be 0.
            slots: vec![Slot {
                generation: GENERATIONS,
                live: None,
            }],
            free: Vec::new(),
            docs: Vec::new(),
            doc_free: Vec::new(),
            limit: limit.min(INDEX_MASK),
            live: 0,
        }
    }

    fn split(h: Cell) -> Option<(usize, u16)> {
        if h <= 0 {
            return None;
        }
        let h = h as u32;
        Some(((h & INDEX_MASK) as usize, (h >> INDEX_BITS) as u16))
    }

    fn slot(&self, h: Cell) -> Option<&Live> {
        let (index, generation) = Self::split(h)?;
        let s = self.slots.get(index)?;
        if s.generation != generation {
            return None;
        }
        s.live.as_ref()
    }

    fn slot_mut(&mut self, h: Cell) -> Option<&mut Live> {
        let (index, generation) = Self::split(h)?;
        let s = self.slots.get_mut(index)?;
        if s.generation != generation {
            return None;
        }
        s.live.as_mut()
    }

    fn take_slot(&mut self, live: Live) -> Cell {
        let index = match self.free.pop() {
            Some(i) => i,
            None if self.slots.len() as u32 <= self.limit => {
                self.slots.push(Slot {
                    generation: 0,
                    live: None,
                });
                self.slots.len() as u32 - 1
            }
            None => return 0,
        };
        self.doc_slot_mut(live.doc).handles += 1;
        let s = &mut self.slots[index as usize];
        s.live = Some(live);
        self.live += 1;
        (((s.generation as u32) << INDEX_BITS) | index) as Cell
    }

    fn doc_slot_mut(&mut self, d: u32) -> &mut DocSlot {
        self.docs[d as usize]
            .as_mut()
            .expect("a handle's document is live")
    }

    fn drop_handle_of(&mut self, d: u32) {
        let ds = self.doc_slot_mut(d);
        ds.handles -= 1;
        if ds.handles == 0 {
            self.docs[d as usize] = None;
            self.doc_free.push(d);
        }
    }

    /// Stores a new document and returns a handle to its root, or 0 if the pool is full.
    pub fn root(&mut self, doc: Doc, owner: Owner) -> Cell {
        let node = doc.root();
        let node_generation = doc.generation(node);
        let slot = DocSlot { doc, handles: 0 };
        let d = match self.doc_free.pop() {
            Some(d) => {
                self.docs[d as usize] = Some(slot);
                d
            }
            None => {
                self.docs.push(Some(slot));
                self.docs.len() as u32 - 1
            }
        };
        let h = self.take_slot(Live {
            doc: d,
            node,
            node_generation,
            owner,
            kept: false,
            view: false,
            walk: None,
        });
        if h == 0 {
            self.docs[d as usize] = None;
            self.doc_free.push(d);
        }
        h
    }

    /// Makes a handle onto `node` in a document that some live handle already holds.
    /// Nothing is copied.
    pub fn view(&mut self, doc: u32, node: NodeId, owner: Owner) -> Cell {
        let node_generation = self.doc(doc).generation(node);
        self.take_slot(Live {
            doc,
            node,
            node_generation,
            owner,
            kept: false,
            view: true,
            walk: None,
        })
    }

    /// One step of a walk. If `el` is a view the script has not kept, it is re-pointed at
    /// `node`; otherwise a new view is made, so an element the script kept is never taken
    /// away from it.
    pub fn walk(&mut self, el: Cell, doc: u32, node: NodeId, owner: Owner) -> Cell {
        self.walk_with(el, doc, node, owner, None)
    }

    /// Like `walk`, but also records the walk's position in the handle it returns.
    pub fn walk_with(
        &mut self,
        el: Cell,
        doc: u32,
        node: NodeId,
        owner: Owner,
        walk: Option<Walk>,
    ) -> Cell {
        let h = self.walk_without_position(el, doc, node, owner);
        if let Some(l) = self.slot_mut(h) {
            l.walk = walk;
        }
        h
    }

    /// The walk position stored in `h`, along with the document being walked.
    pub fn walk_state(&self, h: Cell) -> Option<(u32, Walk)> {
        let l = self.slot(h)?;
        Some((l.doc, l.walk?))
    }

    /// Whether `h` is a view the script has not kept, which a walk is free to re-point or
    /// release.
    pub fn is_unkept_view(&self, h: Cell) -> bool {
        self.slot(h).is_some_and(|l| l.view && !l.kept)
    }

    fn walk_without_position(&mut self, el: Cell, doc: u32, node: NodeId, owner: Owner) -> Cell {
        let node_generation = self.doc(doc).generation(node);
        let old_doc = match self.slot_mut(el) {
            Some(l) if l.view && !l.kept => {
                let old = l.doc;
                *l = Live {
                    doc,
                    node,
                    node_generation,
                    owner,
                    kept: false,
                    view: true,
                    walk: None,
                };
                old
            }
            _ => return self.view(doc, node, owner),
        };
        if old_doc != doc {
            self.doc_slot_mut(doc).handles += 1;
            self.drop_handle_of(old_doc);
        }
        el
    }

    /// The node a handle names, if both the handle and the node are still live.
    pub fn resolve(&self, h: Cell) -> Option<Ref> {
        let l = self.slot(h)?;
        self.doc(l.doc).live(l.node, l.node_generation)?;
        Some(Ref {
            doc: l.doc,
            node: l.node,
        })
    }

    pub fn get(&self, h: Cell) -> Option<(&Doc, NodeId)> {
        let r = self.resolve(h)?;
        Some((self.doc(r.doc), r.node))
    }

    pub fn get_mut(&mut self, h: Cell) -> Option<(&mut Doc, NodeId)> {
        let r = self.resolve(h)?;
        Some((self.doc_mut(r.doc), r.node))
    }

    pub fn doc(&self, d: u32) -> &Doc {
        &self.docs[d as usize]
            .as_ref()
            .expect("a handle's document is live")
            .doc
    }

    pub fn doc_mut(&mut self, d: u32) -> &mut Doc {
        &mut self.doc_slot_mut(d).doc
    }

    /// Borrows two different documents at once, the first to write to and the second to
    /// read from.
    pub fn doc_pair(&mut self, dst: u32, src: u32) -> (&mut Doc, &Doc) {
        assert_ne!(dst, src);
        let (lo, hi) = (dst.min(src) as usize, dst.max(src) as usize);
        let (a, b) = self.docs.split_at_mut(hi);
        let (low, high) = (&mut a[lo], &mut b[0]);
        let (low, high) = (
            &mut low.as_mut().expect("live").doc,
            &mut high.as_mut().expect("live").doc,
        );
        if (dst as usize) < (src as usize) {
            (low, high)
        } else {
            (high, low)
        }
    }

    /// Releases the handle, and its document too if this was the last handle into it.
    /// Returns false if the handle is not live. A handle whose node has been removed is
    /// still a live handle, and releasing it works.
    pub fn release(&mut self, h: Cell) -> bool {
        if self.slot(h).is_none() {
            return false;
        }
        let index = (h as u32 & INDEX_MASK) as usize;
        let s = &mut self.slots[index];
        let live = s.live.take().expect("checked above");
        s.generation += 1;
        if s.generation < GENERATIONS {
            self.free.push(index as u32);
        }
        self.live -= 1;
        self.drop_handle_of(live.doc);
        true
    }

    /// Releases every handle `script` owns, kept ones too, for when the script unloads.
    pub fn release_script(&mut self, script: u32) -> usize {
        let mut n = 0;
        for index in 1..self.slots.len() {
            let s = &self.slots[index];
            if s.live.as_ref().is_some_and(|l| l.owner.script == script) {
                let h = (((s.generation as u32) << INDEX_BITS) | index as u32) as Cell;
                self.release(h);
                n += 1;
            }
        }
        n
    }

    pub fn set_kept(&mut self, h: Cell, kept: bool) -> bool {
        self.slot_mut(h).map(|l| l.kept = kept).is_some()
    }

    pub fn kept(&self, h: Cell) -> bool {
        self.slot(h).is_some_and(|l| l.kept)
    }

    pub fn owner(&self, h: Cell) -> Option<Owner> {
        self.slot(h).map(|l| l.owner)
    }

    pub fn set_owner(&mut self, h: Cell, owner: Owner) -> bool {
        self.slot_mut(h).map(|l| l.owner = owner).is_some()
    }

    pub fn live_handles(&self) -> usize {
        self.live
    }

    /// How many handles point into document `d`.
    pub fn doc_handles(&self, d: u32) -> u32 {
        self.docs[d as usize].as_ref().map_or(0, |ds| ds.handles)
    }

    pub fn live_documents(&self) -> usize {
        self.docs.len() - self.doc_free.len()
    }

    pub fn spent(&self) -> bool {
        self.free.is_empty() && self.slots.len() as u32 > self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Val;
    use crate::write::to_string;

    const ME: Owner = Owner {
        script: 1,
        frame: 100,
    };

    fn parse(pool: &mut Pool, text: &str) -> Cell {
        pool.root(Doc::parse(text).unwrap(), ME)
    }

    fn child(pool: &mut Pool, h: Cell, key: &str) -> Cell {
        let r = pool.resolve(h).unwrap();
        let Val::Obj(m) = pool.doc(r.doc).val(r.node) else {
            panic!()
        };
        let c = m[key];
        pool.view(r.doc, c, ME)
    }

    fn element(pool: &mut Pool, h: Cell, i: usize) -> (u32, NodeId) {
        let r = pool.resolve(h).unwrap();
        let Val::Arr(v) = pool.doc(r.doc).val(r.node) else {
            panic!()
        };
        (r.doc, v[i])
    }

    #[test]
    fn no_handle_is_zero_or_negative() {
        let mut pool = Pool::default();
        let mut held = Vec::new();
        for i in 0..1_000_000 {
            let h = pool.root(Doc::new(Val::Null), ME);
            assert!(h > 0, "handle {h} at {i}");
            held.push(h);
            if held.len() == 1000 {
                for h in held.drain(..) {
                    assert!(pool.release(h));
                }
            }
        }
        assert!(((((GENERATIONS - 1) as u32) << INDEX_BITS) | INDEX_MASK) as Cell > 0);
        assert!(pool.resolve(0).is_none() && pool.resolve(-5).is_none());
    }

    #[test]
    fn a_freed_handle_answers_node_after_its_slot_is_reused() {
        let mut pool = Pool::default();
        let old = parse(&mut pool, "1");
        assert!(pool.release(old));
        let new = parse(&mut pool, "2");
        assert_eq!(old as u32 & INDEX_MASK, new as u32 & INDEX_MASK);
        assert_ne!(old, new);
        assert!(pool.resolve(old).is_none());
        assert!(!pool.release(old));
        let (doc, node) = pool.get(new).unwrap();
        assert_eq!(doc.val(node), &Val::Int(2));
    }

    #[test]
    fn a_spent_slot_is_retired() {
        let mut pool = Pool::default();
        let h = parse(&mut pool, "0");
        let first = h as u32 & INDEX_MASK;
        pool.release(h);
        let mut seen = Vec::new();
        for _ in 1..GENERATIONS {
            let h = parse(&mut pool, "0");
            seen.push(h);
            pool.release(h);
        }
        assert!(seen.iter().all(|&h| h as u32 & INDEX_MASK == first));
        for _ in 0..10_000 {
            let h = parse(&mut pool, "0");
            assert_ne!(h as u32 & INDEX_MASK, first);
            pool.release(h);
        }
    }

    #[test]
    fn a_view_of_a_removed_child_answers_node() {
        let mut pool = Pool::default();
        let root = parse(&mut pool, r#"{"a":{"b":1},"c":[1,2]}"#);
        let a = child(&mut pool, root, "a");
        let c = child(&mut pool, root, "c");
        let (doc, node) = pool.get_mut(root).unwrap();
        doc.obj_remove(node, "a").unwrap();
        assert!(pool.resolve(a).is_none());
        let (doc, node) = pool.get_mut(root).unwrap();
        doc.obj_set(node, "c", Val::Int(3)).unwrap();
        assert!(pool.resolve(c).is_none());
        let c2 = child(&mut pool, root, "c");
        let (doc, node) = pool.get(c2).unwrap();
        assert_eq!(doc.val(node), &Val::Int(3));
        assert!(pool.release(a), "a dead node's handle is still a handle");
    }

    #[test]
    fn a_write_through_a_child_changes_the_parent() {
        let mut pool = Pool::default();
        let root = parse(&mut pool, r#"{"pos":{"x":1}}"#);
        let pos = child(&mut pool, root, "pos");
        let (doc, node) = pool.get_mut(pos).unwrap();
        doc.obj_set(node, "y", Val::from_f32(2.5)).unwrap();
        let (doc, node) = pool.get(root).unwrap();
        assert_eq!(to_string(doc, node, false), r#"{"pos":{"x":1,"y":2.5}}"#);
    }

    #[test]
    fn a_view_survives_its_parent_growing() {
        let mut pool = Pool::default();
        let root = parse(&mut pool, "[7]");
        let (d, n) = element(&mut pool, root, 0);
        let first = pool.view(d, n, ME);
        for i in 0..10_000 {
            let (doc, node) = pool.get_mut(root).unwrap();
            doc.arr_push(node, Val::Int(i)).unwrap();
        }
        let (doc, node) = pool.get(first).unwrap();
        assert_eq!(doc.val(node), &Val::Int(7));
        let (doc, node) = pool.get_mut(root).unwrap();
        doc.arr_remove(node, 5).unwrap();
        let (doc, node) = pool.get(first).unwrap();
        assert_eq!(doc.val(node), &Val::Int(7));
    }

    #[test]
    fn a_document_lives_while_a_view_into_it_lives() {
        let mut pool = Pool::default();
        let root = parse(&mut pool, r#"{"a":"x"}"#);
        let a = child(&mut pool, root, "a");
        assert!(pool.release(root));
        assert_eq!(pool.live_documents(), 1);
        let (doc, node) = pool.get(a).unwrap();
        assert_eq!(doc.val(node), &Val::Str("x".into()));
        assert!(pool.release(a));
        assert_eq!((pool.live_documents(), pool.live_handles()), (0, 0));
    }

    #[test]
    fn no_read_frees() {
        let mut pool = Pool::default();
        let root = parse(&mut pool, r#"{"a":1,"b":[1,2]}"#);
        let a = child(&mut pool, root, "a");
        for _ in 0..100 {
            assert!(pool.get(a).is_some() && pool.resolve(root).is_some());
            let _ = (pool.kept(a), pool.owner(a));
        }
        assert_eq!(pool.live_handles(), 2);
        assert!(pool.get(a).is_some());
    }

    #[test]
    fn the_walk_repoints_its_handle_and_leaves_a_kept_one() {
        let mut pool = Pool::default();
        let arr = parse(&mut pool, "[10,20,30]");
        let value = |pool: &Pool, h| {
            let (doc, node) = pool.get(h).unwrap();
            doc.val(node).clone()
        };
        let (d, n0) = element(&mut pool, arr, 0);
        let el = pool.walk(0, d, n0, ME);
        assert_eq!(value(&pool, el), Val::Int(10));
        let (_, n1) = element(&mut pool, arr, 1);
        assert_eq!(pool.walk(el, d, n1, ME), el);
        assert_eq!(pool.live_handles(), 2);
        pool.set_kept(el, true);
        let (_, n2) = element(&mut pool, arr, 2);
        let next = pool.walk(el, d, n2, ME);
        assert_ne!(next, el);
        assert_eq!(value(&pool, el), Val::Int(20));
        assert_eq!(value(&pool, next), Val::Int(30));
        assert_ne!(pool.walk(arr, d, n2, ME), arr, "a root is never re-pointed");
    }

    #[test]
    fn a_spent_pool_answers_zero() {
        let mut pool = Pool::with_limit(3);
        let hs: Vec<Cell> = (0..3).map(|_| parse(&mut pool, "0")).collect();
        assert!(hs.iter().all(|&h| h > 0));
        assert_eq!(parse(&mut pool, "0"), 0);
        assert!(pool.spent());
        assert_eq!(pool.live_documents(), 3);
        pool.release(hs[0]);
        assert!(parse(&mut pool, "0") > 0);
    }

    #[test]
    fn a_script_s_unload_frees_what_it_owns() {
        let mut pool = Pool::default();
        let mine = parse(&mut pool, "[1]");
        pool.set_kept(mine, true);
        let theirs = pool.root(
            Doc::new(Val::Null),
            Owner {
                script: 2,
                frame: 100,
            },
        );
        assert_eq!(pool.release_script(1), 1);
        assert!(pool.resolve(mine).is_none() && pool.resolve(theirs).is_some());
    }
}
