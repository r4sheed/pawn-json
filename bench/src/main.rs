// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// How fast does a script read its JSON? This reads every .json and .jsonl file under a
// folder the way a Pawn loader would - parse, then every value by key, arrays by length
// and index - once through a model of pawn-json 1.x's natives and once through this
// plugin's core. The AMX calls themselves are left out of both, so the numbers are the
// plugin's own work.
//
//     cargo build --release --target i686-pc-windows-msvc -p jsonbench
//     target/i686-pc-windows-msvc/release/jsonbench.exe <folder> [runs]

// The 1.x model has all of 1.x's natives, including some this benchmark never calls.
#![allow(dead_code)]

use encoding_rs::WINDOWS_1251;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

// The component ships with mimalloc and 1.x used the system allocator, so each model gets
// the allocator it really runs with. Frees go back to whichever heap the pointer came
// from.
struct Switch;

thread_local! {
    static ON_MIMALLOC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn on_mimalloc() -> bool {
    ON_MIMALLOC.try_with(|c| c.get()).unwrap_or(false)
}

fn owned_by_mimalloc(p: *mut u8) -> bool {
    unsafe { libmimalloc_sys::mi_is_in_heap_region(p as *const _) }
}

unsafe impl std::alloc::GlobalAlloc for Switch {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        unsafe {
            if on_mimalloc() {
                mimalloc::MiMalloc.alloc(l)
            } else {
                std::alloc::System.alloc(l)
            }
        }
    }
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {
        unsafe {
            if on_mimalloc() {
                mimalloc::MiMalloc.alloc_zeroed(l)
            } else {
                std::alloc::System.alloc_zeroed(l)
            }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        unsafe {
            if owned_by_mimalloc(p) {
                mimalloc::MiMalloc.dealloc(p, l)
            } else {
                std::alloc::System.dealloc(p, l)
            }
        }
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, size: usize) -> *mut u8 {
        unsafe {
            if owned_by_mimalloc(p) {
                mimalloc::MiMalloc.realloc(p, l, size)
            } else {
                std::alloc::System.realloc(p, l, size)
            }
        }
    }
}

#[global_allocator]
static ALLOCATOR: Switch = Switch;

struct Mimalloc;

impl Mimalloc {
    fn on() -> Mimalloc {
        ON_MIMALLOC.with(|c| c.set(true));
        Mimalloc
    }
}

impl Drop for Mimalloc {
    fn drop(&mut self) {
        ON_MIMALLOC.with(|c| c.set(false));
    }
}

const NUMBER: i32 = 0;
const BOOLEAN: i32 = 1;
const STRING: i32 = 2;
const OBJECT: i32 = 3;
const ARRAY: i32 = 4;
const NULL: i32 = 5;

fn cells(s: &str) -> Vec<i32> {
    let mut v: Vec<i32> = WINDOWS_1251.encode(s).0.iter().map(|&b| b as i32).collect();
    v.push(0);
    v
}

fn strlen(c: &[i32]) -> usize {
    c.iter().position(|&x| x == 0).unwrap_or(c.len())
}

// A model of pawn-json 1.x's natives, written to behave the way they did: every read
// clones the node first, handles live in a HashMap, and strings go through samp-rs's
// Windows-1251 decoding.
mod cur {
    use super::*;

    pub fn key(c: &[i32]) -> String {
        let len = strlen(c);
        let mut bytes = Vec::with_capacity(len);
        for &x in &c[..len] {
            bytes.push(x as u8);
        }
        WINDOWS_1251.decode(&bytes).0.into_owned()
    }

    pub fn put(buf: &mut [i32], s: &str) -> bool {
        let b = WINDOWS_1251.encode(s).0;
        if b.len() >= buf.len() {
            return false;
        }
        for (i, &x) in b.iter().enumerate() {
            buf[i] = x as i32;
        }
        buf[b.len()] = 0;
        true
    }

    #[derive(Clone)]
    pub struct Item {
        pub value: Value,
        pub gc: bool,
    }

    #[derive(Default)]
    pub struct Pool {
        pub active: HashMap<i32, Item>,
        pub current: i32,
    }

    impl Pool {
        pub fn alloc(&mut self, v: Value) -> i32 {
            self.current += 1;
            self.active
                .insert(self.current, Item { value: v, gc: true });
            self.current
        }
        pub fn get(&mut self, id: i32) -> Option<&mut Value> {
            self.active.get_mut(&id).map(|i| &mut i.value)
        }
        pub fn take(&mut self, id: i32) -> Option<Value> {
            let i = self.active.get(&id)?.clone();
            if i.gc {
                self.active.remove(&id);
            }
            Some(i.value)
        }
        pub fn has(&self, id: i32) -> bool {
            self.active.contains_key(&id)
        }
        pub fn collect(&mut self, id: i32) {
            if let Some(i) = self.active.get(&id) {
                if i.gc {
                    self.active.remove(&id);
                }
            }
        }
        pub fn collect_force(&mut self, id: i32) {
            self.active.remove(&id);
        }
    }

    fn t(v: &Value) -> i32 {
        match v {
            Value::Null => NULL,
            Value::Bool(_) => BOOLEAN,
            Value::Number(_) => NUMBER,
            Value::String(_) => STRING,
            Value::Array(_) => ARRAY,
            Value::Object(_) => OBJECT,
        }
    }

    pub fn parse(p: &mut Pool, input: &[i32], out: &mut i32) -> i32 {
        match serde_json::from_str::<Value>(&key(input)) {
            Ok(v) => {
                *out = p.alloc(v);
                0
            }
            Err(_) => 1,
        }
    }

    pub fn parse_text(p: &mut Pool, text: &str, out: &mut i32) -> i32 {
        match serde_json::from_str::<Value>(text) {
            Ok(v) => {
                *out = p.alloc(v);
                0
            }
            Err(_) => 3,
        }
    }

    pub fn keys(p: &mut Pool, node: i32, index: i32, out: &mut [i32]) -> i32 {
        let v = match p.get(node) {
            Some(v) => v,
            None => return 1,
        };
        if !v.is_object() {
            return 1;
        }
        let mut vec: Vec<String> = vec![];
        for k in v.as_object().unwrap().keys() {
            vec.push(k.to_string());
        }
        if index as usize >= vec.len() {
            return 1;
        }
        put(out, &vec[index as usize]);
        0
    }

    pub fn get_type(p: &mut Pool, node: i32, k: &[i32]) -> i32 {
        let v = match p.get(node) {
            Some(v) => v,
            None => return 0,
        };
        if !v.is_object() {
            return 0;
        }
        if v.get(key(k)).is_none() {
            return 0;
        }
        t(&v[key(k)])
    }

    macro_rules! cloned_get {
        ($p:expr, $node:expr, $k:expr) => {{
            let v: Value = match $p.get($node) {
                Some(v) => v.clone(),
                None => return 1,
            };
            let v = match v.as_object() {
                Some(v) => v,
                None => return 1,
            };
            match v.get(&key($k)) {
                Some(v) => v.clone(),
                None => return 2,
            }
        }};
    }

    pub fn get_string(p: &mut Pool, node: i32, k: &[i32], out: &mut [i32]) -> i32 {
        let v = cloned_get!(p, node, k);
        let v = match v.as_str() {
            Some(v) => v,
            None => return 3,
        };
        put(out, v);
        0
    }

    pub fn get_int(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        let v = cloned_get!(p, node, k);
        match v.as_i64() {
            Some(v) => *out = v as i32,
            None => return 3,
        }
        0
    }

    pub fn get_float(p: &mut Pool, node: i32, k: &[i32], out: &mut f32) -> i32 {
        let v = cloned_get!(p, node, k);
        match v.as_f64() {
            Some(v) => *out = v as f32,
            None => return 3,
        }
        0
    }

    pub fn get_bool(p: &mut Pool, node: i32, k: &[i32], out: &mut bool) -> i32 {
        let v = cloned_get!(p, node, k);
        match v.as_bool() {
            Some(v) => *out = v,
            None => return 3,
        }
        0
    }

    pub fn get_object(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        let v: Value = match p.get(node) {
            Some(v) => v.clone(),
            None => return 1,
        };
        let v = match v.as_object() {
            Some(v) => v,
            None => return 2,
        };
        let v = match v.get(&key(k)) {
            Some(v) => v.clone(),
            None => return 3,
        };
        *out = p.alloc(v);
        0
    }

    pub fn get_array(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        let v = cloned_get!(p, node, k);
        if v.as_array().is_none() {
            return 3;
        }
        *out = p.alloc(v);
        0
    }

    pub fn array_length(p: &mut Pool, node: i32, len: &mut i32) -> i32 {
        let v: Value = match p.get(node) {
            Some(v) => v.clone(),
            None => return 1,
        };
        match v.as_array() {
            Some(v) => *len = v.len() as i32,
            None => return 1,
        }
        0
    }

    pub fn array_object(p: &mut Pool, node: i32, index: i32, out: &mut i32) -> i32 {
        let v: Value = match p.get(node) {
            Some(v) => v.clone(),
            None => return 1,
        };
        let v = match v.as_array() {
            Some(v) => v,
            None => return 1,
        };
        let v = match v.get(index as usize) {
            Some(v) => v.clone(),
            None => return 2,
        };
        *out = p.alloc(v);
        0
    }

    pub fn array_iterate(p: &mut Pool, node: i32, index: &mut i32, out: &mut i32) -> i32 {
        let v: Value = match p.get(node) {
            Some(v) => v.clone(),
            None => return 1,
        };
        *index += 1;
        let v = match v.as_array() {
            Some(v) => v,
            None => return 1,
        };
        if p.has(*out) {
            p.collect_force(*out);
        }
        let v = match v.get(*index as usize) {
            Some(v) => v.clone(),
            None => return 2,
        };
        *out = p.alloc(v);
        0
    }

    pub fn get_node_string(p: &mut Pool, node: i32, out: &mut [i32]) -> i32 {
        let v = match p.take(node) {
            Some(v) => v,
            None => return 1,
        };
        match v.as_str() {
            Some(s) => {
                put(out, s);
                0
            }
            None => 1,
        }
    }

    pub fn get_node_int(p: &mut Pool, node: i32, out: &mut i32) -> i32 {
        let v = match p.take(node) {
            Some(v) => v,
            None => return 1,
        };
        match v.as_i64() {
            Some(x) => {
                *out = x as i32;
                0
            }
            None => 1,
        }
    }

    pub fn get_node_float(p: &mut Pool, node: i32, out: &mut f32) -> i32 {
        let v = match p.take(node) {
            Some(v) => v,
            None => return 1,
        };
        match v.as_f64() {
            Some(x) => {
                *out = x as f32;
                0
            }
            None => 1,
        }
    }

    pub fn get_node_bool(p: &mut Pool, node: i32, out: &mut bool) -> i32 {
        let v = match p.take(node) {
            Some(v) => v,
            None => return 1,
        };
        match v.as_bool() {
            Some(x) => {
                *out = x;
                0
            }
            None => 1,
        }
    }

    pub fn cleanup(p: &mut Pool, node: i32, auto: bool) {
        if auto {
            p.collect(node)
        } else {
            p.collect_force(node)
        }
    }
}

// This plugin's core, wrapped in the old natives' signatures so both models can be driven
// by exactly the same calls.
mod new {
    use super::*;
    use pawn_json_core::api;
    use pawn_json_core::dom::Doc;
    use pawn_json_core::error::Type;
    use pawn_json_core::pool::{self as core_pool, Owner};
    use pawn_json_core::text::Codepage;

    const ME: Owner = Owner {
        script: 1,
        frame: 1,
    };

    pub struct Pool {
        pub p: core_pool::Pool,
        pub cp: Codepage,
    }

    impl Default for Pool {
        fn default() -> Self {
            Pool {
                p: core_pool::Pool::default(),
                cp: Codepage::DEFAULT,
            }
        }
    }

    impl Pool {
        pub fn live(&self) -> usize {
            self.p.live_handles()
        }
    }

    fn old_type(t: i32) -> i32 {
        match t {
            x if x == Type::Bool as i32 => BOOLEAN,
            x if x == Type::Int as i32 || x == Type::Float as i32 => NUMBER,
            x if x == Type::String as i32 => STRING,
            x if x == Type::Array as i32 => ARRAY,
            x if x == Type::Object as i32 => OBJECT,
            _ => NULL,
        }
    }

    fn float(r: i32, c: i32, out: &mut f32) -> i32 {
        if r == 0 {
            *out = f32::from_bits(c as u32);
        }
        r
    }

    pub fn parse(p: &mut Pool, input: &[i32], out: &mut i32) -> i32 {
        api::parse(&mut p.p, p.cp, ME, input, out)
    }

    pub fn parse_text(p: &mut Pool, text: &str, out: &mut i32) -> i32 {
        match Doc::parse(text) {
            Ok(doc) => {
                *out = p.p.root(doc, ME);
                0
            }
            Err(_) => 3,
        }
    }

    pub fn keys(p: &mut Pool, node: i32, index: i32, out: &mut [i32]) -> i32 {
        api::key_at(&p.p, p.cp, node, index, out)
    }

    pub fn get_type(p: &mut Pool, node: i32, k: &[i32]) -> i32 {
        old_type(api::type_of(&p.p, p.cp, node, k))
    }

    pub fn get_string(p: &mut Pool, node: i32, k: &[i32], out: &mut [i32]) -> i32 {
        api::get_string(&p.p, p.cp, node, k, out)
    }

    pub fn get_int(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        api::get_int(&p.p, p.cp, node, k, out)
    }

    pub fn get_float(p: &mut Pool, node: i32, k: &[i32], out: &mut f32) -> i32 {
        let mut c = 0;
        float(api::get_float(&p.p, p.cp, node, k, &mut c), c, out)
    }

    pub fn get_bool(p: &mut Pool, node: i32, k: &[i32], out: &mut bool) -> i32 {
        let mut c = 0;
        let r = api::get_bool(&p.p, p.cp, node, k, &mut c);
        *out = c != 0;
        r
    }

    pub fn get_object(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        api::get_node(&mut p.p, p.cp, ME, node, k, out)
    }

    pub fn get_array(p: &mut Pool, node: i32, k: &[i32], out: &mut i32) -> i32 {
        if api::type_of(&p.p, p.cp, node, k) != Type::Array as i32 {
            return 3;
        }
        api::get_node(&mut p.p, p.cp, ME, node, k, out)
    }

    pub fn array_length(p: &mut Pool, node: i32, len: &mut i32) -> i32 {
        api::count(&p.p, node, len)
    }

    pub fn array_object(p: &mut Pool, node: i32, index: i32, out: &mut i32) -> i32 {
        api::element_node(&mut p.p, ME, node, index, out)
    }

    pub fn array_iterate(p: &mut Pool, node: i32, index: &mut i32, out: &mut i32) -> i32 {
        let mut cursor = *index + 1;
        *index += 1;
        api::next(&mut p.p, ME, node, &mut cursor, out)
    }

    pub fn get_node_string(p: &mut Pool, node: i32, out: &mut [i32]) -> i32 {
        api::value_string(&p.p, p.cp, node, out)
    }

    pub fn get_node_int(p: &mut Pool, node: i32, out: &mut i32) -> i32 {
        api::value_int(&p.p, node, out)
    }

    pub fn get_node_float(p: &mut Pool, node: i32, out: &mut f32) -> i32 {
        let mut c = 0;
        float(api::value_float(&p.p, node, &mut c), c, out)
    }

    pub fn get_node_bool(p: &mut Pool, node: i32, out: &mut bool) -> i32 {
        let mut c = 0;
        let r = api::value_bool(&p.p, node, &mut c);
        *out = c != 0;
        r
    }

    pub fn cleanup(p: &mut Pool, node: i32) {
        p.p.release(node);
    }
}

// How a typical loader reads a row: each column by name, arrays by length and index, and
// each element's value with GetNode*. The plan is worked out once from serde_json so the
// timed loops only make the calls.
enum Op {
    Int(Vec<i32>),
    Float(Vec<i32>),
    Bool(Vec<i32>),
    Str(Vec<i32>),
    Arr(Vec<i32>, Vec<El>),
    Obj(Vec<i32>, Vec<Op>),
}

enum El {
    Int,
    Float,
    Bool,
    Str,
    Arr(Vec<El>),
    Obj(Vec<Op>),
    Skip,
}

fn plan_obj(m: &serde_json::Map<String, Value>) -> Vec<Op> {
    m.iter()
        .filter_map(|(k, v)| {
            let k = cells(k);
            Some(match v {
                Value::Number(n) if n.is_i64() => Op::Int(k),
                Value::Number(_) => Op::Float(k),
                Value::Bool(_) => Op::Bool(k),
                Value::String(_) => Op::Str(k),
                Value::Array(a) => Op::Arr(k, a.iter().map(plan_el).collect()),
                Value::Object(o) => Op::Obj(k, plan_obj(o)),
                Value::Null => return None,
            })
        })
        .collect()
}

fn plan_el(v: &Value) -> El {
    match v {
        Value::Number(n) if n.is_i64() => El::Int,
        Value::Number(_) => El::Float,
        Value::Bool(_) => El::Bool,
        Value::String(_) => El::Str,
        Value::Array(a) => El::Arr(a.iter().map(plan_el).collect()),
        Value::Object(o) => El::Obj(plan_obj(o)),
        Value::Null => El::Skip,
    }
}

macro_rules! define_walk {
    ($m:ident, $obj:ident, $el:ident, $cleanup:expr) => {
        fn $obj(p: &mut $m::Pool, node: i32, ops: &[Op], acc: &mut u64) {
            let mut buf = [0i32; 256];
            for op in ops {
                match op {
                    Op::Int(k) => {
                        let mut v = 0;
                        $m::get_int(p, node, k, &mut v);
                        *acc = acc.wrapping_add(v as u64);
                    }
                    Op::Float(k) => {
                        let mut v = 0.0;
                        $m::get_float(p, node, k, &mut v);
                        *acc = acc.wrapping_add(v as u64);
                    }
                    Op::Bool(k) => {
                        let mut v = false;
                        $m::get_bool(p, node, k, &mut v);
                        *acc += v as u64;
                    }
                    Op::Str(k) => {
                        $m::get_string(p, node, k, &mut buf);
                        *acc += strlen(&buf) as u64;
                    }
                    Op::Arr(k, els) => {
                        let mut list = 0;
                        if $m::get_array(p, node, k, &mut list) == 0 {
                            let mut len = 0;
                            $m::array_length(p, list, &mut len);
                            for i in 0..len {
                                let mut e = 0;
                                $m::array_object(p, list, i, &mut e);
                                $el(p, e, &els[i as usize], acc);
                                $cleanup(p, e);
                            }
                        }
                        $cleanup(p, list);
                    }
                    Op::Obj(k, inner) => {
                        let mut o = 0;
                        if $m::get_object(p, node, k, &mut o) == 0 {
                            $obj(p, o, inner, acc);
                        }
                        $cleanup(p, o);
                    }
                }
            }
        }

        fn $el(p: &mut $m::Pool, e: i32, op: &El, acc: &mut u64) {
            let mut buf = [0i32; 256];
            match op {
                El::Int => {
                    let mut v = 0;
                    $m::get_node_int(p, e, &mut v);
                    *acc = acc.wrapping_add(v as u64);
                }
                El::Float => {
                    let mut v = 0.0;
                    $m::get_node_float(p, e, &mut v);
                    *acc = acc.wrapping_add(v as u64);
                }
                El::Bool => {
                    let mut v = false;
                    $m::get_node_bool(p, e, &mut v);
                    *acc += v as u64;
                }
                El::Str => {
                    $m::get_node_string(p, e, &mut buf);
                    *acc += strlen(&buf) as u64;
                }
                El::Arr(els) => {
                    let mut len = 0;
                    $m::array_length(p, e, &mut len);
                    for i in 0..len {
                        let mut x = 0;
                        $m::array_object(p, e, i, &mut x);
                        $el(p, x, &els[i as usize], acc);
                        $cleanup(p, x);
                    }
                }
                El::Obj(inner) => $obj(p, e, inner, acc),
                El::Skip => {}
            }
        }
    };
}

define_walk!(cur, cur_obj, cur_el, |p: &mut cur::Pool, n| cur::cleanup(
    p, n, true
));
define_walk!(new, new_obj, new_el, |p: &mut new::Pool, n| new::cleanup(
    p, n
));

struct Rows {
    lines: Vec<Vec<i32>>,
    plans: Vec<Vec<Op>>,
    texts: Vec<String>,
}

fn load_rows(path: &std::path::Path) -> Rows {
    let text = std::fs::read_to_string(path).unwrap();
    let mut rows = Rows {
        lines: vec![],
        plans: vec![],
        texts: vec![],
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(line) {
            rows.lines.push(cells(line));
            rows.plans.push(plan_obj(&m));
            rows.texts.push(line.to_owned());
        }
    }
    rows
}

fn run_rows_cur(rows: &Rows) -> u64 {
    let mut p = cur::Pool::default();
    let mut acc = 0u64;
    for (line, plan) in rows.lines.iter().zip(&rows.plans) {
        let mut row = 0;
        if cur::parse(&mut p, line, &mut row) == 0 {
            cur_obj(&mut p, row, plan, &mut acc);
        }
        cur::cleanup(&mut p, row, true);
    }
    assert!(p.active.is_empty());
    acc
}

fn run_rows_new(rows: &Rows) -> u64 {
    let _heap = Mimalloc::on();
    let mut p = new::Pool::default();
    let mut acc = 0u64;
    for (line, plan) in rows.lines.iter().zip(&rows.plans) {
        let mut row = 0;
        if new::parse(&mut p, line, &mut row) == 0 {
            new_obj(&mut p, row, plan, &mut acc);
        }
        new::cleanup(&mut p, row);
    }
    assert_eq!(p.live(), 0);
    acc
}

fn bench(name: &str, iters: usize, mut f: impl FnMut() -> u64) -> f64 {
    let mut times = Vec::with_capacity(iters);
    let mut check = 0;
    for _ in 0..iters {
        let t = Instant::now();
        check = std::hint::black_box(f());
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = times[times.len() / 2];
    println!("  {:<44} {:>10.3} ms   check {}", name, med, check);
    med
}

// What the core should hand back for `s` in windows-1252, with `?` for anything the code
// page can't hold.
fn expected(s: &str) -> Vec<i32> {
    let mut out = vec![];
    for ch in s.chars() {
        let mut b = [0u8; 4];
        let (bytes, _, unmappable) = encoding_rs::WINDOWS_1252.encode(ch.encode_utf8(&mut b));
        if unmappable {
            out.push(b'?' as i32);
        } else {
            out.extend(bytes.iter().map(|&x| x as i32));
        }
    }
    out
}

fn got(buf: &[i32]) -> &[i32] {
    &buf[..strlen(buf)]
}

fn cells_cp(s: &str) -> Vec<i32> {
    let mut v = expected(s);
    v.push(0);
    v
}

// Before timing anything, check every value the core reads against serde_json. The check
// sums alone can't prove it, because 1.x writes `&#NNN;` entities where the core writes
// plain bytes.
fn verify_obj(p: &mut new::Pool, node: i32, m: &serde_json::Map<String, Value>) -> usize {
    let mut n = 0;
    for (k, v) in m {
        let key = cells_cp(k);
        let mut child = 0;
        assert_eq!(new::get_object(p, node, &key, &mut child), 0, "key {k}");
        n += verify_el(p, child, v);
        new::cleanup(p, child);
    }
    n
}

fn verify_el(p: &mut new::Pool, el: i32, v: &Value) -> usize {
    let mut buf = vec![0i32; 8192];
    match v {
        Value::String(s) => {
            assert_eq!(new::get_node_string(p, el, &mut buf), 0);
            assert_eq!(got(&buf), &expected(s)[..], "string {s}");
        }
        Value::Number(x) if x.is_i64() => {
            let mut i = 0;
            let want = x.as_i64().unwrap();
            let r = new::get_node_int(p, el, &mut i);
            if let Ok(w) = i32::try_from(want) {
                assert_eq!((r, i), (0, w));
            } else {
                assert_eq!(r, pawn_json_core::error::Error::Range as i32);
            }
        }
        Value::Number(x) => {
            let mut f = 0.0;
            assert_eq!(new::get_node_float(p, el, &mut f), 0);
            assert_eq!(f, x.as_f64().unwrap() as f32);
        }
        Value::Bool(b) => {
            let mut x = false;
            assert_eq!(new::get_node_bool(p, el, &mut x), 0);
            assert_eq!(x, *b);
        }
        Value::Null => {}
        Value::Array(a) => {
            let mut len = 0;
            assert_eq!(new::array_length(p, el, &mut len), 0);
            assert_eq!(len as usize, a.len());
            let mut n = 0;
            for (i, e) in a.iter().enumerate() {
                let mut x = 0;
                assert_eq!(new::array_object(p, el, i as i32, &mut x), 0);
                n += verify_el(p, x, e);
                new::cleanup(p, x);
            }
            return n + 1;
        }
        Value::Object(m) => return verify_obj(p, el, m) + 1,
    }
    1
}

fn verify(texts: &[&str]) -> usize {
    let _heap = Mimalloc::on();
    let mut p = new::Pool::default();
    let mut n = 0;
    for text in texts {
        let v: Value = serde_json::from_str(text).unwrap();
        let mut root = 0;
        assert_eq!(new::parse_text(&mut p, text, &mut root), 0);
        n += verify_el(&mut p, root, &v);
        new::cleanup(&mut p, root);
    }
    assert_eq!(p.live(), 0);
    n
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "json" || x == "jsonl") {
            out.push(p);
        }
    }
}

// A .json file is one document and a .jsonl file is one document per line. Either way,
// each object is read column by column.
fn load(path: &std::path::Path) -> Rows {
    if path.extension().is_some_and(|x| x == "jsonl") {
        return load_rows(path);
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut rows = Rows {
        lines: vec![],
        plans: vec![],
        texts: vec![],
    };
    if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&text) {
        rows.lines.push(cells(&text));
        rows.plans.push(plan_obj(&m));
        rows.texts.push(text);
    }
    rows
}

fn main() {
    let mut args = std::env::args().skip(1);
    let folder = std::path::PathBuf::from(args.next().expect("usage: jsonbench <folder> [runs]"));
    let runs: usize = args.next().map_or(5, |x| x.parse().unwrap());

    let mut files = vec![];
    collect(&folder, &mut files);
    files.sort();
    let (mut old, mut core, mut documents, mut values) = (0.0, 0.0, 0, 0);
    for f in &files {
        let rows = load(f);
        if rows.lines.is_empty() {
            continue;
        }
        documents += rows.lines.len();
        values += verify(&rows.texts.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let name = f
            .strip_prefix(&folder)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        println!(" {name}, {} documents", rows.lines.len());
        old += bench("pawn-json 1.x", runs, || run_rows_cur(&rows));
        core += bench("this plugin", runs.max(20), || run_rows_new(&rows));
    }
    println!(
        "{} files, {} documents, {} values read back right: pawn-json 1.x {:.2} ms, this plugin {:.2} ms ({:.1}x)",
        files.len(),
        documents,
        values,
        old,
        core,
        old / core
    );
}
