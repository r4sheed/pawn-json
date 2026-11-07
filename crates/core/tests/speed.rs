// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Speed checks, for `cargo test --release`. Each times a piece of the core against
//! serde_json doing the same work in the same process, so a bound holds on a slow
//! machine as well as a fast one. The bounds leave room for a noisy machine; what they
//! catch is a change that makes reading or saving a good deal slower. A debug build skips
//! them, since its numbers say nothing about the library that ships.

use pawn_json_core::dom::Doc;
use pawn_json_core::file::{self, Saver};
use pawn_json_core::write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// The component allocates through mimalloc, and parsing is mostly allocation.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Held by each check, so they run one at a time and don't slow each other down.
static ALONE: Mutex<()> = Mutex::new(());

/// 2000 rows of the kind a server keeps in a file: ids, names, positions, flags.
fn rows() -> String {
    let row = |i: usize| {
        format!(
            "{{\"id\":{i},\"name\":\"House {i}\",\"owner\":\"Player_{}\",\"x\":{:.4},\"y\":{:.4},\"z\":13.5,\"a\":{}.25,\"price\":{},\"locked\":{},\"interior\":{},\"world\":0,\"tags\":[\"a\",\"b\"]}}",
            i % 97,
            1000.0 + i as f64 * 1.25,
            -1500.0 - i as f64 * 0.75,
            i % 360,
            50000 + i * 10,
            i.is_multiple_of(2),
            i % 18,
        )
    };
    format!("[{}]", (0..2000).map(row).collect::<Vec<_>>().join(","))
}

/// 24000 coordinates, where printing floats is nearly all the work.
fn floats() -> String {
    let xs: Vec<String> = (0..24000)
        .map(|i| format!("{:.4}", 1000.0 + i as f64 * 1.25))
        .collect();
    format!("[{}]", xs.join(","))
}

/// The fastest of `runs` timings: the run least disturbed by whatever else the machine
/// was doing.
fn best(runs: usize, mut f: impl FnMut()) -> Duration {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join("pawn-json-speed");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
#[cfg_attr(debug_assertions, ignore = "speed checks need --release")]
fn parsing_is_no_slower_than_serde_json() {
    let _alone = ALONE.lock().unwrap_or_else(|e| e.into_inner());
    let text = rows();
    let ours = best(21, || drop(Doc::parse_slice(text.as_bytes()).unwrap()));
    let theirs = best(21, || {
        drop(serde_json::from_slice::<serde_json::Value>(text.as_bytes()).unwrap())
    });
    println!(
        "parse: core {:.2} ms, serde_json {:.2} ms",
        ms(ours),
        ms(theirs)
    );
    // Measured at 0.65 to 0.75 of serde_json's time.
    assert!(
        ours.as_secs_f64() <= theirs.as_secs_f64() * 1.1,
        "parsing took {:.2} ms where serde_json took {:.2} ms",
        ms(ours),
        ms(theirs)
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore = "speed checks need --release")]
fn writing_is_about_as_fast_as_serde_json() {
    let _alone = ALONE.lock().unwrap_or_else(|e| e.into_inner());
    // Rows were measured at 0.85 to 1.35 of serde_json's time and floats alone at about
    // 1.4; with ryu printing the floats they took 2 and 3 times as long.
    for (name, text, bound) in [("rows", rows(), 1.6), ("floats", floats(), 2.2)] {
        let doc = Doc::parse(&text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        for pretty in [false, true] {
            let ours = best(21, || drop(write::to_string(&doc, doc.root(), pretty)));
            let theirs = best(21, || {
                drop(if pretty {
                    serde_json::to_string_pretty(&value)
                } else {
                    serde_json::to_string(&value)
                })
            });
            println!(
                "write {name} (pretty {pretty}): core {:.2} ms, serde_json {:.2} ms",
                ms(ours),
                ms(theirs)
            );
            assert!(
                ours.as_secs_f64() <= theirs.as_secs_f64() * bound,
                "writing {name} (pretty {pretty}) took {:.2} ms where serde_json took {:.2} ms",
                ms(ours),
                ms(theirs)
            );
        }
    }
}

/// A save returns before the disk has confirmed the write. Where the disk confirms almost
/// at once there is nothing to tell apart, and the check says so and passes.
#[test]
#[cfg_attr(debug_assertions, ignore = "speed checks need --release")]
fn a_save_returns_before_the_disk_is_done() {
    let _alone = ALONE.lock().unwrap_or_else(|e| e.into_inner());
    let doc = Doc::parse(&rows()).unwrap();
    let path = scratch("speed-save.json");
    let plain = scratch("speed-plain.json");
    let text = write::to_string(&doc, doc.root(), false);
    let unsynced = best(15, || std::fs::write(&plain, text.as_bytes()).unwrap());
    let written = best(15, || drop(write::to_string(&doc, doc.root(), false)));
    let synced = best(15, || file::save(&doc, doc.root(), &path, false).unwrap());
    let wait = synced.saturating_sub(written + unsynced);
    let mut saver = Saver::default();
    let mut queued = Duration::MAX;
    for _ in 0..15 {
        let start = Instant::now();
        saver.save(&doc, doc.root(), &path, false).unwrap();
        queued = queued.min(start.elapsed());
        // Waited for outside the timing, so the saves don't pile up.
        saver.flush();
    }
    println!(
        "save: in place {:.2} ms (of which the disk's wait {:.2} ms), queued {:.2} ms",
        ms(synced),
        ms(wait),
        ms(queued)
    );
    if wait < Duration::from_micros(500) {
        println!("the disk here confirms writes too fast to tell the two apart");
        return;
    }
    assert!(
        queued + wait / 2 <= synced,
        "a queued save took {:.2} ms, a save in place {:.2} ms, of which the disk's wait {:.2} ms",
        ms(queued),
        ms(synced),
        ms(wait)
    );
}
