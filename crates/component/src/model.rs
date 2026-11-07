// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Random scripts. Each case is a run of native calls, chosen and combined at random,
//! made the way the abstract machine makes them, by a script with two nested functions
//! of four `Node:` variables each. Some steps are whole loops whose bodies do the things
//! scripts get wrong: release the element, copy it into a variable that goes out of
//! scope, add to what is being walked, reuse the variable for an inner loop, leave early.
//!
//! After every step:
//! - no call has panicked, and every code is one the include declares;
//! - a handle that was released answers JSON_ERR_NODE, for good;
//! - a loop has not run past the length its container had when it began;
//! - a node written out reads back as the same text.
//!
//! At the end both functions' scopes end, as `operator~` ends them. Then the only nodes
//! left may be the ones the script kept, and the ones it lost by assigning over a
//! variable by hand, which no plugin can see. Once those are released too, nothing at
//! all may be left: anything else is a leak.
//!
//! A failing case is cut down to the shortest run that still fails and printed.
//! `PROPTEST_CASES=20000 cargo test -p pawn-json model` runs a longer search.

use crate::amx::fake::Script;
use crate::amx::{Amx, Cell};
use crate::natives::find;
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};

const OK: Cell = 0;
const ERR_NODE: Cell = 1;
const LAST_CODE: Cell = 11;

// Where things live in the script's data, in cells.
const CELLS: usize = 12_000;
const HEAP: usize = 1;
const OUT: usize = 3_000;
const OUT_CELLS: usize = 2_000;
const KEY: usize = 5_100;
const KEY_CELLS: usize = 16;
const TEMP: usize = 5_200;
const B_VARS: usize = 6_000;
const A_VARS: usize = 7_000;
const SCOPE: usize = 8_000;
// The two functions' frames. The stack grows down, so the inner one is lower.
const A_FRAME: Cell = 7_500 * 4;
const B_FRAME: Cell = 6_500 * 4;

const TEXTS: &[&str] = &[
    r#"{"a":1,"b":[1,2,3],"c":{"d":[{"e":1},{"e":2}]}}"#,
    r#"[1,[2,3],{"a":[4]},[]]"#,
    "[]",
    "{}",
    "5",
    "\"s\"",
    "{",
    "[[[[1]]]]",
    r#"{"a":{"a":{"a":{}}}}"#,
    r#"[{"items":[1,2],"n":1},{"items":[3],"n":2}]"#,
];
const KEYS: &[&str] = &["a", "b", "c", "d", "e", "items", "n", "x", ""];
const PATHS: &[&str] = &[
    "/a", "/b/1", "/c/d/0/e", "/1/0", "", "a", "/9", "/b/-", "/0/items",
];
const INDICES: &[Cell] = &[-1, 0, 1, 2, 5, Cell::MAX];
/// Formats for JSON_Unpack, each with the kind of variable it fills, in order: an int
/// (`i`), the node variable (`o`), or a string buffer (`s`).
const FORMATS: &[(&str, &str)] = &[
    ("{a:i}", "i"),
    ("{b:o}", "o"),
    ("{c:o?, a:i?}", "oi"),
    ("[i,o]", "io"),
    ("{a:s[4]}", "s"),
    ("{x:i}", "i"),
];

#[derive(Debug, Clone, Copy)]
struct Var(u8);

#[derive(Debug, Clone)]
enum Body {
    Nothing,
    Read,
    ReleaseElement,
    CleanupElement,
    CopyIntoBlockVariable,
    AddToContainer,
    SetKeyOnContainer,
    RemoveFirst,
    InnerLoopOnSameVariable(u8),
    Break,
}

#[derive(Debug, Clone)]
enum Op {
    Parse(Var, u8),
    ParseFile(Var, u8),
    Save(Var, u8),
    Flush,
    Object(Var),
    Array(Var),
    Copy(Var, Var),
    Assign(Var, Var),
    Garbage(Var, Cell),
    GetNode(Var, u8, Var),
    PathNode(Var, u8, Var),
    ElementNode(Var, u8, Var),
    GetObject(Var, u8, Var),
    GetAs(Var, u8, Var),
    ArrayObject(Var, u8, Var),
    Unpack(Var, u8, Var),
    SetInt(Var, u8, Cell),
    SetNode(Var, u8, Var),
    AppendInt(Var, Cell),
    AppendNode(Var, Var),
    ArraySetNode(Var, u8, Var),
    Remove(Var, u8),
    ArrayRemove(Var, u8),
    Release(Var),
    Cleanup(Var),
    Keep(Var),
    ToggleGcOff(Var),
    Build(u8, Var, Var),
    Step(Var, Var),
    StepEntry(Var, Var),
    StepKey(Var),
    Loop(Var, Var, Vec<Body>),
    Stringify(Var),
    Enter,
    Leave,
}

fn var() -> impl Strategy<Value = Var> {
    (0u8..8).prop_map(Var)
}

fn body() -> impl Strategy<Value = Body> {
    prop_oneof![
        3 => Just(Body::Nothing),
        2 => Just(Body::Read),
        1 => Just(Body::ReleaseElement),
        1 => Just(Body::CleanupElement),
        1 => Just(Body::CopyIntoBlockVariable),
        1 => Just(Body::AddToContainer),
        1 => Just(Body::SetKeyOnContainer),
        1 => Just(Body::RemoveFirst),
        1 => (0u8..9).prop_map(Body::InnerLoopOnSameVariable),
        1 => Just(Body::Break),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    let t = 0u8..TEXTS.len() as u8;
    let k = || 0u8..KEYS.len() as u8;
    let i = || 0u8..INDICES.len() as u8;
    prop_oneof![
        6 => (var(), t).prop_map(|(v, t)| Op::Parse(v, t)),
        1 => (var(), 0u8..3).prop_map(|(v, f)| Op::ParseFile(v, f)),
        1 => (var(), 0u8..3).prop_map(|(v, f)| Op::Save(v, f)),
        1 => Just(Op::Flush),
        1 => var().prop_map(Op::Object),
        1 => var().prop_map(Op::Array),
        1 => (var(), var()).prop_map(|(a, b)| Op::Copy(a, b)),
        2 => (var(), var()).prop_map(|(a, b)| Op::Assign(a, b)),
        1 => (var(), any::<Cell>()).prop_map(|(v, x)| Op::Garbage(v, x)),
        4 => (var(), k(), var()).prop_map(|(a, k, b)| Op::GetNode(a, k, b)),
        2 => (var(), 0u8..PATHS.len() as u8, var()).prop_map(|(a, p, b)| Op::PathNode(a, p, b)),
        2 => (var(), i(), var()).prop_map(|(a, i, b)| Op::ElementNode(a, i, b)),
        1 => (var(), k(), var()).prop_map(|(a, k, b)| Op::GetObject(a, k, b)),
        1 => (var(), k(), var()).prop_map(|(a, k, b)| Op::GetAs(a, k, b)),
        1 => (var(), i(), var()).prop_map(|(a, i, b)| Op::ArrayObject(a, i, b)),
        1 => (var(), 0u8..FORMATS.len() as u8, var()).prop_map(|(a, f, b)| Op::Unpack(a, f, b)),
        2 => (var(), k(), any::<Cell>()).prop_map(|(v, k, x)| Op::SetInt(v, k, x)),
        2 => (var(), k(), var()).prop_map(|(a, k, b)| Op::SetNode(a, k, b)),
        2 => (var(), any::<Cell>()).prop_map(|(v, x)| Op::AppendInt(v, x)),
        2 => (var(), var()).prop_map(|(a, b)| Op::AppendNode(a, b)),
        1 => (var(), i(), var()).prop_map(|(a, i, b)| Op::ArraySetNode(a, i, b)),
        1 => (var(), k()).prop_map(|(v, k)| Op::Remove(v, k)),
        1 => (var(), i()).prop_map(|(v, i)| Op::ArrayRemove(v, i)),
        2 => var().prop_map(Op::Release),
        1 => var().prop_map(Op::Cleanup),
        1 => var().prop_map(Op::Keep),
        1 => var().prop_map(Op::ToggleGcOff),
        1 => (k(), var(), var()).prop_map(|(k, a, b)| Op::Build(k, a, b)),
        3 => (var(), var()).prop_map(|(a, b)| Op::Step(a, b)),
        1 => (var(), var()).prop_map(|(a, b)| Op::StepEntry(a, b)),
        1 => var().prop_map(Op::StepKey),
        4 => (var(), var(), prop::collection::vec(body(), 1..4))
            .prop_map(|(a, b, body)| Op::Loop(a, b, body)),
        2 => var().prop_map(Op::Stringify),
        1 => Just(Op::Enter),
        1 => Just(Op::Leave),
    ]
}

struct Model {
    s: Script,
    in_b: bool,
    kept: HashSet<Cell>,
    lost: HashSet<Cell>,
    dead: HashSet<Cell>,
    /// Variables whose loop was left half done, by `break` or by a single step, with the
    /// element they held then. A loop started on such a variable is nested in that one
    /// (see `api::Walks`), and if it walks the same container, which loop a step belongs
    /// to can't be told. Such loops must end, but may visit an element again.
    left: HashMap<usize, Cell>,
    dir: std::path::PathBuf,
}

impl Model {
    fn new() -> Model {
        let mut s = Script::new(CELLS);
        s.amx.frm = A_FRAME;
        let dir = std::env::temp_dir().join("pawn-json-model");
        std::fs::create_dir_all(&dir).unwrap();
        Model {
            s,
            in_b: false,
            kept: HashSet::new(),
            lost: HashSet::new(),
            dead: HashSet::new(),
            left: HashMap::new(),
            dir,
        }
    }

    fn amx(&mut self) -> *mut Amx {
        &mut *self.s.amx
    }

    /// The cell a variable lives in. The inner function's variables only exist while it
    /// runs; until then they stand for the outer one's.
    fn cell(&self, v: Var) -> usize {
        match v.0 {
            n @ 4..8 if self.in_b => B_VARS + n as usize - 4,
            n => A_VARS + (n % 4) as usize,
        }
    }

    fn addr(&self, v: Var) -> Cell {
        (self.cell(v) * 4) as Cell
    }

    fn get(&self, v: Var) -> Cell {
        self.s.data[self.cell(v)]
    }

    /// A Pawn assignment, `v = value`. What `v` held is lost to the script unless it is
    /// held somewhere else too.
    fn assign(&mut self, v: Var, value: Cell) {
        let old = self.get(v);
        if old != 0 && old != value {
            self.lost.insert(old);
        }
        let c = self.cell(v);
        self.s.data[c] = value;
    }

    fn text(&mut self, s: &str) -> Cell {
        self.s.string(s.as_bytes())
    }

    fn call(&mut self, name: &str, args: &[Cell]) -> Cell {
        self.s.call(find(name), args)
    }

    /// A call that returns a code: it must be one the include declares.
    fn code(&mut self, name: &str, args: &[Cell]) -> Cell {
        let r = self.call(name, args);
        assert!((0..=LAST_CODE).contains(&r), "{name} returned {r}");
        r
    }

    fn path(&self, file: u8) -> String {
        self.dir
            .join(format!("model-{file}.json"))
            .to_string_lossy()
            .into_owned()
    }

    fn release(&mut self, h: Cell) {
        if self.code("JSON_Release", &[h]) == OK {
            self.dead.insert(h);
            self.kept.remove(&h);
        }
    }

    fn count(&mut self, h: Cell) -> Option<Cell> {
        let out = (OUT * 4) as Cell;
        (self.code("JSON_Count", &[h, out]) == OK).then(|| self.s.data[OUT])
    }

    /// Ends a scope the way `operator~` does: the plugin is handed the variables, while
    /// the saved frame says whose scope it is.
    fn end_scope(&mut self, first: usize, cells: usize, frame: Cell) {
        let saved = self.s.amx.frm;
        self.s.data[SCOPE] = frame;
        self.s.amx.frm = (SCOPE * 4) as Cell;
        let r = self.call("JSON@Free", &[(first * 4) as Cell, cells as Cell]);
        assert!(r >= 0, "JSON@Free returned {r}");
        self.s.amx.frm = saved;
    }

    fn run(&mut self, op: &Op) {
        self.s.rewind(HEAP);
        match *op {
            Op::Parse(v, t) => {
                let text = self.text(TEXTS[t as usize]);
                self.code("JSON_Parse", &[text, self.addr(v)]);
            }
            Op::ParseFile(v, f) => {
                let path = self.path(f);
                let p = self.text(&path);
                self.code("JSON_ParseFile", &[p, self.addr(v)]);
            }
            Op::Save(v, f) => {
                let path = self.path(f);
                let p = self.text(&path);
                self.code("JSON_SaveFile", &[p, self.get(v), 0]);
            }
            Op::Flush => {
                self.code("JSON_Flush", &[]);
            }
            Op::Object(v) => {
                let h = self.call("JSON_Object", &[]);
                self.assign(v, h);
            }
            Op::Array(v) => {
                let h = self.call("JSON_Array", &[]);
                self.assign(v, h);
            }
            Op::Copy(from, to) => {
                let h = self.call("JSON_Copy", &[self.get(from)]);
                self.assign(to, h);
            }
            Op::Assign(from, to) => {
                let h = self.get(from);
                self.assign(to, h);
            }
            Op::Garbage(v, x) => self.assign(v, x),
            Op::GetNode(from, k, to) => {
                let key = self.text(KEYS[k as usize]);
                self.code("JSON_GetNode", &[self.get(from), key, self.addr(to)]);
            }
            Op::PathNode(from, p, to) => {
                let path = self.text(PATHS[p as usize]);
                self.code("JSON_PathGetNode", &[self.get(from), path, self.addr(to)]);
            }
            Op::ElementNode(from, i, to) => {
                let args = [self.get(from), INDICES[i as usize], self.addr(to)];
                self.code("JSON_ArrayGetNode", &args);
            }
            Op::GetObject(from, k, to) => {
                let key = self.text(KEYS[k as usize]);
                self.code("JSON_GetObject", &[self.get(from), key, self.addr(to)]);
            }
            Op::GetAs(from, k, to) => {
                // JSON_Get(node, key, Node:var) as the include's macro spells it.
                let key = self.text(KEYS[k as usize]);
                let args = [self.get(from), key, 4, 1, self.addr(to)];
                self.code("JSON_GetAs", &args);
            }
            Op::ArrayObject(from, i, to) => {
                let args = [self.get(from), INDICES[i as usize], self.addr(to)];
                self.code("JSON_ArrayObject", &args);
            }
            Op::Unpack(from, f, to) => {
                let (format, kinds) = FORMATS[f as usize];
                let format = self.text(format);
                let mut args = vec![self.get(from), format];
                for kind in kinds.chars() {
                    args.push(match kind {
                        'i' => self.s.put(&[0]),
                        's' => self.s.put(&[0; 4]),
                        _ => self.addr(to),
                    });
                }
                self.code("JSON_Unpack", &args);
            }
            Op::SetInt(on, k, x) => {
                let key = self.text(KEYS[k as usize]);
                self.code("JSON_SetInt", &[self.get(on), key, x]);
            }
            Op::SetNode(on, k, from) => {
                let key = self.text(KEYS[k as usize]);
                self.code("JSON_SetNode", &[self.get(on), key, self.get(from)]);
            }
            Op::AppendInt(on, x) => {
                self.code("JSON_AppendInt", &[self.get(on), x]);
            }
            Op::AppendNode(on, from) => {
                self.code("JSON_AppendNode", &[self.get(on), self.get(from)]);
            }
            Op::ArraySetNode(on, i, from) => {
                let args = [self.get(on), INDICES[i as usize], self.get(from)];
                self.code("JSON_ArraySetNode", &args);
            }
            Op::Remove(on, k) => {
                let key = self.text(KEYS[k as usize]);
                self.code("JSON_Remove", &[self.get(on), key]);
            }
            Op::ArrayRemove(on, i) => {
                self.code("JSON_ArrayRemoveAt", &[self.get(on), INDICES[i as usize]]);
            }
            Op::Release(v) => {
                let h = self.get(v);
                self.release(h);
            }
            Op::Cleanup(v) => {
                let h = self.get(v);
                if self.code("JSON_Cleanup", &[h, 0]) == OK {
                    self.dead.insert(h);
                    self.kept.remove(&h);
                }
            }
            Op::Keep(v) => {
                let h = self.get(v);
                if self.code("JSON_Keep", &[h]) == OK {
                    self.kept.insert(h);
                }
            }
            Op::ToggleGcOff(v) => {
                let h = self.get(v);
                if self.code("JSON_ToggleGC", &[h, 0]) == OK {
                    self.kept.insert(h);
                }
            }
            Op::Build(k, from, to) => {
                // 1.x's builder, JSON_Object("key", node): it takes the node.
                let key = self.text(KEYS[k as usize]);
                let held = self.s.put(&[self.get(from)]);
                let h = self.call("JSON_Object", &[key, held]);
                self.assign(to, h);
            }
            Op::Step(c, el) => {
                if self.call("JSON_Iter", &[self.get(c), self.addr(el)]) != 0 {
                    self.left.insert(self.cell(el), self.get(el));
                }
            }
            Op::StepEntry(c, el) => {
                let key = (KEY * 4) as Cell;
                let args = [self.get(c), key, self.addr(el), KEY_CELLS as Cell];
                if self.call("JSON_IterEntry", &args) != 0 {
                    self.left.insert(self.cell(el), self.get(el));
                }
            }
            Op::StepKey(c) => {
                let key = (KEY * 4) as Cell;
                self.call("JSON_IterKey", &[self.get(c), key, KEY_CELLS as Cell]);
            }
            Op::Loop(c, el, ref body) => {
                let nested_in_a_left_loop = self.left.get(&self.cell(el)) == Some(&self.get(el));
                self.walk(c, el, body, 0, nested_in_a_left_loop);
            }
            Op::Stringify(v) => self.round_trip(v),
            Op::Enter => {
                if !self.in_b {
                    self.in_b = true;
                    self.s.data[B_VARS..B_VARS + 4].fill(0);
                    self.s.amx.frm = B_FRAME;
                }
            }
            Op::Leave => self.leave(),
        }
    }

    fn leave(&mut self) {
        if self.in_b {
            self.end_scope(B_VARS, 4, B_FRAME);
            self.in_b = false;
            self.s.amx.frm = A_FRAME;
        }
    }

    /// A whole `while (JSON_Iter(c, el))` loop. It may take at most as many steps as the
    /// container had elements when it began. When `c` and `el` are one variable, the
    /// container changes with every step, as the script asked; such a loop goes down the
    /// tree and must still end, which a limit far above any of these documents' size
    /// checks.
    fn walk(&mut self, c: Var, el: Var, body: &[Body], depth: u32, relaxed: bool) {
        let container = self.get(c);
        let limit = if relaxed || self.cell(c) == self.cell(el) {
            1_000
        } else {
            self.count(container).unwrap_or(0)
        };
        let mut steps = 0;
        'walk: while self.call("JSON_Iter", &[self.get(c), self.addr(el)]) != 0 {
            steps += 1;
            assert!(
                steps <= limit,
                "a loop over {limit} elements took step {steps}"
            );
            for b in body {
                let e = self.get(el);
                match *b {
                    Body::Nothing => {}
                    Body::Read => {
                        let key = self.text("");
                        self.call("JSON_TypeOf", &[e, key]);
                    }
                    Body::ReleaseElement => self.release(e),
                    Body::CleanupElement => {
                        if self.code("JSON_Cleanup", &[e, 0]) == OK {
                            self.dead.insert(e);
                        }
                    }
                    Body::CopyIntoBlockVariable => {
                        // `new Node:item = row;`, then the block ends.
                        self.s.data[TEMP] = e;
                        let frame = if self.in_b { B_FRAME } else { A_FRAME };
                        self.end_scope(TEMP, 1, frame);
                        self.s.data[TEMP] = 0;
                    }
                    Body::AddToContainer => {
                        self.code("JSON_AppendInt", &[self.get(c), 7]);
                    }
                    Body::SetKeyOnContainer => {
                        let key = self.text(&format!("k{steps}"));
                        self.code("JSON_SetInt", &[self.get(c), key, 7]);
                    }
                    Body::RemoveFirst => {
                        self.code("JSON_ArrayRemoveAt", &[self.get(c), 0]);
                    }
                    Body::InnerLoopOnSameVariable(k) if depth < 2 => {
                        // The container comes from the element, through a variable of its
                        // own, and the inner loop reuses the outer loop's variable.
                        let inner = (1..4)
                            .map(Var)
                            .find(|v| {
                                self.cell(*v) != self.cell(c) && self.cell(*v) != self.cell(el)
                            })
                            .expect("three variables to choose from");
                        let key = self.text(KEYS[k as usize]);
                        // Only a child of the element: a variable left holding something
                        // else could hold the very container walked above, which is the
                        // case `api::Walks` stops by its budget, not by its length.
                        if self.code("JSON_GetNode", &[e, key, self.addr(inner)]) == OK {
                            self.walk(inner, el, &[Body::Read], depth + 1, relaxed);
                        }
                    }
                    Body::InnerLoopOnSameVariable(_) => {}
                    Body::Break => {
                        self.left.insert(self.cell(el), e);
                        break 'walk;
                    }
                }
            }
        }
    }

    /// What a node writes out must read back as the same text.
    fn round_trip(&mut self, v: Var) {
        let out = (OUT * 4) as Cell;
        let args = [self.get(v), out, OUT_CELLS as Cell, 0];
        if self.code("JSON_Stringify", &args) != OK {
            return;
        }
        let first: Vec<Cell> = self.s.data[OUT..OUT + OUT_CELLS].to_vec();
        let text = self.s.put(&first);
        let back = self.s.put(&[0]);
        assert_eq!(
            self.code("JSON_Parse", &[text, back]),
            OK,
            "it wrote JSON it can't read"
        );
        let h = self.s.cell_at(back);
        assert_eq!(
            self.code("JSON_Stringify", &[h, out, OUT_CELLS as Cell, 0]),
            OK
        );
        assert_eq!(
            &self.s.data[OUT..OUT + OUT_CELLS],
            &first[..],
            "the text changed on the way back"
        );
        self.release(h);
    }

    /// The checks that hold after every step.
    fn check(&mut self) {
        let dead: Vec<Cell> = self.dead.iter().copied().take(64).collect();
        for h in dead {
            let out = (OUT * 4) as Cell;
            assert_eq!(
                self.call("JSON_Count", &[h, out]),
                ERR_NODE,
                "released handle {h} came back"
            );
        }
    }

    /// Ends both scopes, releases what the script kept or lost, and returns what is left.
    fn finish(&mut self) -> Cell {
        self.leave();
        self.end_scope(A_VARS, 4, A_FRAME);
        let held: Vec<Cell> = self.kept.union(&self.lost).copied().collect();
        for h in held {
            self.call("JSON_Release", &[h]);
        }
        self.code("JSON_Flush", &[]);
        self.call("JSON_CountNodes", &[])
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        let amx = self.amx();
        crate::state::with(|st| st.unloaded(amx));
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        max_shrink_iters: 20_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_scripts_leak_nothing_and_never_hang(ops in prop::collection::vec(op(), 1..48)) {
        let caught = crate::guard::caught();
        let mut m = Model::new();
        assert_eq!(m.call("JSON_CountNodes", &[]), 0, "an earlier case left nodes behind");
        for op in &ops {
            m.run(op);
            m.check();
        }
        let left = m.finish();
        prop_assert_eq!(left, 0, "nodes left after every scope ended");
        prop_assert_eq!(crate::guard::caught(), caught, "a call panicked");
    }
}
