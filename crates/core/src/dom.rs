// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

use crate::error::Error;
use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub type NodeId = u32;
pub type Map = IndexMap<Box<str>, NodeId, FxBuildHasher>;

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Box<str>),
    Arr(Vec<NodeId>),
    Obj(Map),
}

impl Val {
    /// Stores a Pawn float as the shortest decimal that reads back as the same f32, so
    /// 2165.9163 is written as 2165.9163 and not as 2165.916259765625.
    pub fn from_f32(x: f32) -> Val {
        if !x.is_finite() {
            return Val::Float(x as f64);
        }
        let mut b = zmij::Buffer::new();
        Val::Float(b.format_finite(x).parse().unwrap_or(x as f64))
    }

    pub fn empty_object() -> Val {
        Val::Obj(Map::default())
    }
}

pub struct Node {
    pub generation: u32,
    pub val: Val,
}

/// One JSON document. Every node lives in a single arena and containers hold their
/// children by index, so a node's index stays valid while its parent grows. Freeing a
/// node bumps its generation, which is how a handle onto it finds out it is gone.
pub struct Doc {
    nodes: Vec<Node>,
    free: Vec<NodeId>,
    root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> ParseError {
        ParseError {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        }
    }
}

impl Doc {
    pub fn new(val: Val) -> Doc {
        let mut doc = Doc {
            nodes: Vec::new(),
            free: Vec::new(),
            root: 0,
        };
        doc.root = doc.alloc(val);
        doc
    }

    pub fn parse(text: &str) -> Result<Doc, ParseError> {
        let mut de = serde_json::Deserializer::from_str(text);
        Self::parse_from(&mut de)
    }

    pub fn parse_slice(bytes: &[u8]) -> Result<Doc, ParseError> {
        let mut de = serde_json::Deserializer::from_slice(bytes);
        Self::parse_from(&mut de)
    }

    fn parse_from<'de, R: serde_json::de::Read<'de>>(
        de: &mut serde_json::Deserializer<R>,
    ) -> Result<Doc, ParseError> {
        let mut b = Building {
            doc: Doc {
                nodes: Vec::new(),
                free: Vec::new(),
                root: 0,
            },
            ids: Vec::new(),
            entries: Vec::new(),
        };
        b.doc.root = Seed(&mut b).deserialize(&mut *de)?;
        de.end()?;
        Ok(b.doc)
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn generation(&self, id: NodeId) -> u32 {
        self.nodes[id as usize].generation
    }

    /// The value of a node the caller already knows is live; the generation is not checked.
    pub fn val(&self, id: NodeId) -> &Val {
        &self.nodes[id as usize].val
    }

    pub fn val_mut(&mut self, id: NodeId) -> &mut Val {
        &mut self.nodes[id as usize].val
    }

    /// The value of `id` if it is still at `generation`, that is, if it has not been freed
    /// since that generation was read.
    pub fn live(&self, id: NodeId, generation: u32) -> Option<&Val> {
        match self.nodes.get(id as usize) {
            Some(n) if n.generation == generation => Some(&n.val),
            _ => None,
        }
    }

    pub fn live_mut(&mut self, id: NodeId, generation: u32) -> Option<&mut Val> {
        match self.nodes.get_mut(id as usize) {
            Some(n) if n.generation == generation => Some(&mut n.val),
            _ => None,
        }
    }

    pub fn alloc(&mut self, val: Val) -> NodeId {
        if let Some(id) = self.free.pop() {
            self.nodes[id as usize].val = val;
            id
        } else {
            self.nodes.push(Node { generation: 0, val });
            (self.nodes.len() - 1) as NodeId
        }
    }

    /// Frees `id` and everything under it. A node whose generation counter has run out is
    /// retired rather than reused, so a stale handle can never end up naming a newer node.
    pub fn free_subtree(&mut self, id: NodeId) {
        // Most values freed are a number or a string, which need no stack at all, and an
        // array's own list of children serves as one.
        let mut stack = match self.free_node(id) {
            Val::Arr(v) => v,
            Val::Obj(m) => m.into_values().collect(),
            _ => return,
        };
        while let Some(n) = stack.pop() {
            match self.free_node(n) {
                Val::Arr(v) => stack.extend(v),
                Val::Obj(m) => stack.extend(m.into_values()),
                _ => {}
            }
        }
    }

    /// Frees the one node `id` and returns the value it held, children and all.
    fn free_node(&mut self, id: NodeId) -> Val {
        let node = &mut self.nodes[id as usize];
        let val = std::mem::replace(&mut node.val, Val::Null);
        node.generation = node.generation.wrapping_add(1);
        if node.generation != u32::MAX {
            self.free.push(id);
        }
        val
    }

    /// Makes `id` the root and frees the old one, which must not hold any children, since
    /// they would be freed along with it.
    pub fn reroot(&mut self, id: NodeId) {
        let old = std::mem::replace(&mut self.root, id);
        if old != id {
            self.free_subtree(old);
        }
    }

    /// Copies the subtree at `id` in `src` into this document and returns the new node. It
    /// keeps its own stack instead of recursing: the parser caps how deep a document can
    /// nest, but a tree built up through natives can be any depth.
    pub fn copy_from(&mut self, src: &Doc, id: NodeId) -> NodeId {
        let top = self.alloc(Val::Null);
        let mut stack = vec![(id, top)];
        while let Some((s, d)) = stack.pop() {
            let val = match src.val(s) {
                Val::Arr(v) => Val::Arr(
                    v.iter()
                        .map(|&c| {
                            let n = self.alloc(Val::Null);
                            stack.push((c, n));
                            n
                        })
                        .collect(),
                ),
                Val::Obj(m) => {
                    let mut out = Map::with_capacity_and_hasher(m.len(), FxBuildHasher);
                    for (k, &c) in m {
                        let n = self.alloc(Val::Null);
                        stack.push((c, n));
                        out.insert(k.clone(), n);
                    }
                    Val::Obj(out)
                }
                v => v.clone(),
            };
            self.nodes[d as usize].val = val;
        }
        top
    }

    /// Copies the subtree at `id` out into a document of its own.
    pub fn clone_subtree(&self, id: NodeId) -> Doc {
        let mut doc = Doc {
            nodes: Vec::new(),
            free: Vec::new(),
            root: 0,
        };
        doc.root = doc.copy_from(self, id);
        doc
    }

    /// Stores the already allocated node `id` under `key`, freeing whatever was there.
    pub fn obj_put(&mut self, obj: NodeId, key: &str, id: NodeId) -> Result<(), Error> {
        let Val::Obj(m) = &mut self.nodes[obj as usize].val else {
            return Err(Error::Container);
        };
        let old = match m.get_mut(key) {
            Some(slot) => Some(std::mem::replace(slot, id)),
            None => {
                m.insert(Box::from(key), id);
                None
            }
        };
        if let Some(old) = old {
            self.free_subtree(old);
        }
        Ok(())
    }

    pub fn obj_set(&mut self, obj: NodeId, key: &str, val: Val) -> Result<NodeId, Error> {
        if !matches!(self.val(obj), Val::Obj(_)) {
            return Err(Error::Container);
        }
        let id = self.alloc(val);
        self.obj_put(obj, key, id)?;
        Ok(id)
    }

    /// Removes `key` and frees its value. The keys after it shift down rather than the last
    /// one being swapped in, so the order is kept.
    pub fn obj_remove(&mut self, obj: NodeId, key: &str) -> Result<(), Error> {
        let Val::Obj(m) = &mut self.nodes[obj as usize].val else {
            return Err(Error::Container);
        };
        let id = m.shift_remove(key).ok_or(Error::Key)?;
        self.free_subtree(id);
        Ok(())
    }

    pub fn arr_push(&mut self, arr: NodeId, val: Val) -> Result<NodeId, Error> {
        if !matches!(self.val(arr), Val::Arr(_)) {
            return Err(Error::Container);
        }
        let id = self.alloc(val);
        self.arr_push_node(arr, id)?;
        Ok(id)
    }

    pub fn arr_push_node(&mut self, arr: NodeId, id: NodeId) -> Result<(), Error> {
        let Val::Arr(v) = &mut self.nodes[arr as usize].val else {
            return Err(Error::Container);
        };
        v.push(id);
        Ok(())
    }

    /// Stores the already allocated node `id` at `index`, freeing whatever was there.
    pub fn arr_put(&mut self, arr: NodeId, index: usize, id: NodeId) -> Result<(), Error> {
        let Val::Arr(v) = &mut self.nodes[arr as usize].val else {
            return Err(Error::Container);
        };
        let slot = v.get_mut(index).ok_or(Error::Key)?;
        let old = std::mem::replace(slot, id);
        self.free_subtree(old);
        Ok(())
    }

    pub fn arr_remove(&mut self, arr: NodeId, index: usize) -> Result<(), Error> {
        let Val::Arr(v) = &mut self.nodes[arr as usize].val else {
            return Err(Error::Container);
        };
        if index >= v.len() {
            return Err(Error::Key);
        }
        let id = v.remove(index);
        self.free_subtree(id);
        Ok(())
    }

    /// Whether node `x` of `a` and node `y` of `b` hold the same value. An object's keys
    /// may come in any order, but numbers must be of the same kind, so 1 and 1.0 differ.
    pub fn same(a: &Doc, x: NodeId, b: &Doc, y: NodeId) -> bool {
        let mut stack = vec![(x, y)];
        while let Some((x, y)) = stack.pop() {
            match (a.val(x), b.val(y)) {
                (Val::Arr(p), Val::Arr(q)) => {
                    if p.len() != q.len() {
                        return false;
                    }
                    stack.extend(p.iter().copied().zip(q.iter().copied()));
                }
                (Val::Obj(p), Val::Obj(q)) => {
                    if p.len() != q.len() {
                        return false;
                    }
                    for (k, &v) in p {
                        match q.get(k) {
                            Some(&w) => stack.push((v, w)),
                            None => return false,
                        }
                    }
                }
                (Val::Arr(_) | Val::Obj(_), _) | (_, Val::Arr(_) | Val::Obj(_)) => return false,
                (p, q) => {
                    if p != q {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn live_nodes(&self) -> usize {
        self.nodes.len() - self.free.len() - self.retired()
    }

    fn retired(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.generation == u32::MAX)
            .count()
    }
}

struct Key(Box<str>);

impl<'de> de::Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Key, D::Error> {
        struct KeyVisitor;
        impl Visitor<'_> for KeyVisitor {
            type Value = Key;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a key")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Key, E> {
                Ok(Key(Box::from(v)))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Key, E> {
                Ok(Key(v.into_boxed_str()))
            }
        }
        d.deserialize_str(KeyVisitor)
    }
}

/// A document while it is being parsed. An array's elements and an object's entries
/// are gathered on these two stacks, which every level of the document shares, until
/// their container closes. Only then is its list or map made, once and at its final
/// size, where growing it entry by entry would allocate and move it several times over.
struct Building {
    doc: Doc,
    ids: Vec<NodeId>,
    entries: Vec<(Box<str>, NodeId)>,
}

struct Seed<'a>(&'a mut Building);

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = NodeId;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<NodeId, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = NodeId;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Bool(v)))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Int(v)))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<NodeId, E> {
        let val = match i64::try_from(v) {
            Ok(v) => Val::Int(v),
            Err(_) => Val::Float(v as f64),
        };
        Ok(self.0.doc.alloc(val))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Float(v)))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Str(Box::from(v))))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Str(v.into_boxed_str())))
    }
    fn visit_unit<E: de::Error>(self) -> Result<NodeId, E> {
        Ok(self.0.doc.alloc(Val::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<NodeId, A::Error> {
        let b = self.0;
        let start = b.ids.len();
        while let Some(id) = a.next_element_seed(Seed(&mut *b))? {
            b.ids.push(id);
        }
        let v: Vec<NodeId> = b.ids.drain(start..).collect();
        Ok(b.doc.alloc(Val::Arr(v)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<NodeId, A::Error> {
        let b = self.0;
        let start = b.entries.len();
        while let Some(Key(k)) = a.next_key()? {
            let id = a.next_value_seed(Seed(&mut *b))?;
            b.entries.push((k, id));
        }
        let mut m = Map::with_capacity_and_hasher(b.entries.len() - start, FxBuildHasher);
        for (k, id) in b.entries.drain(start..) {
            // A key written twice keeps its first place and takes its last value.
            if let Some(old) = m.insert(k, id) {
                b.doc.free_subtree(old);
            }
        }
        Ok(b.doc.alloc(Val::Obj(m)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(doc: &Doc) -> Vec<&str> {
        match doc.val(doc.root()) {
            Val::Obj(m) => m.keys().map(|k| &**k).collect(),
            _ => vec![],
        }
    }

    /// A Pawn float has to land on the same f64 it always did, or a value saved and read
    /// back would change. Checked against ryu, which `from_f32` used before, on a sweep of
    /// f32 bit patterns. Every finite f32 was compared once when the switch was made.
    #[test]
    fn a_pawn_float_is_stored_as_it_always_was() {
        let mut bits = 0u32;
        loop {
            let x = f32::from_bits(bits);
            let Val::Float(got) = Val::from_f32(x) else {
                panic!()
            };
            let want: f64 = if x.is_finite() {
                ryu::Buffer::new().format_finite(x).parse().unwrap()
            } else {
                x as f64
            };
            assert!(
                got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
                "{x:e}"
            );
            match bits.checked_add(9_973) {
                Some(next) => bits = next,
                None => break,
            }
        }
    }

    #[test]
    fn an_object_keeps_its_keys_in_file_order() {
        let doc = Doc::parse(r#"{"zeta":1,"alpha":2,"mid":{"b":1,"a":2},"beta":3}"#).unwrap();
        assert_eq!(keys(&doc), ["zeta", "alpha", "mid", "beta"]);
    }

    #[test]
    fn a_repeated_key_keeps_its_place_and_takes_the_last_value() {
        let doc = Doc::parse(r#"{"a":{"x":1},"b":2,"a":3}"#).unwrap();
        assert_eq!(keys(&doc), ["a", "b"]);
        let Val::Obj(m) = doc.val(doc.root()) else {
            panic!()
        };
        assert_eq!(doc.val(m["a"]), &Val::Int(3));
        assert_eq!(doc.live_nodes(), 3);
    }

    #[test]
    fn bad_json_answers_its_line_and_column() {
        let e = Doc::parse("{\n  \"a\": 1,\n}").err().unwrap();
        assert_eq!((e.line, e.column), (3, 1));
        let e = Doc::parse("[1] x").err().unwrap();
        assert_eq!((e.line, e.column), (1, 5));
        assert!(Doc::parse("").is_err());
    }

    #[test]
    fn a_nest_of_64_is_read_and_one_past_128_is_refused() {
        let deep = |n| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(Doc::parse(&deep(64)).is_ok());
        assert!(Doc::parse(&deep(129)).is_err());
    }

    #[test]
    fn numbers_keep_their_kind() {
        let doc = Doc::parse(r#"[1, -9223372036854775808, 18446744073709551615, 1.5, 5.0, 1e3]"#)
            .unwrap();
        let Val::Arr(v) = doc.val(doc.root()) else {
            panic!()
        };
        let vals: Vec<&Val> = v.iter().map(|&i| doc.val(i)).collect();
        assert_eq!(vals[0], &Val::Int(1));
        assert_eq!(vals[1], &Val::Int(i64::MIN));
        assert_eq!(vals[2], &Val::Float(18446744073709551615.0));
        assert_eq!(vals[3], &Val::Float(1.5));
        assert_eq!(vals[4], &Val::Float(5.0));
        assert_eq!(vals[5], &Val::Float(1000.0));
    }

    #[test]
    fn a_freed_subtree_steps_its_generations_and_is_reused() {
        let mut doc = Doc::parse(r#"{"a":[1,2,3]}"#).unwrap();
        let Val::Obj(m) = doc.val(doc.root()) else {
            panic!()
        };
        let a = m["a"];
        let g = doc.generation(a);
        doc.free_subtree(a);
        assert!(doc.live(a, g).is_none());
        assert_eq!(doc.live_nodes(), 1);
        let n = doc.alloc(Val::Null);
        assert_ne!(doc.generation(n), 0);
    }

    #[test]
    fn same_compares_values_not_layout() {
        let a = Doc::parse(r#"[{"x":1,"y":[true,"s"]},2]"#).unwrap();
        let b = Doc::parse(r#"{"k":{"y":[true,"s"],"x":1}}"#).unwrap();
        let Val::Arr(v) = a.val(a.root()) else {
            panic!()
        };
        let Val::Obj(m) = b.val(b.root()) else {
            panic!()
        };
        assert!(Doc::same(&a, v[0], &b, m["k"]), "keys in another order");
        assert!(!Doc::same(&a, v[1], &b, m["k"]));
        let c = Doc::parse(r#"[1, 1.0]"#).unwrap();
        let Val::Arr(w) = c.val(c.root()) else {
            panic!()
        };
        assert!(!Doc::same(&c, w[0], &c, w[1]), "a number's kind as parsed");
    }

    #[test]
    fn a_spent_node_is_retired() {
        let mut doc = Doc::new(Val::Null);
        let n = doc.alloc(Val::Null);
        doc.nodes[n as usize].generation = u32::MAX - 1;
        doc.free_subtree(n);
        assert_ne!(doc.alloc(Val::Null), n);
    }
}
