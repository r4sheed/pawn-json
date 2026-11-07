// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Checks on what the repository's other files say, read as plain text.

#[test]
fn the_include_says_the_crate_version() {
    let inc = include_str!("../../../include/json.inc");
    let want = format!("#define JSON_VERSION \"{}\"", env!("CARGO_PKG_VERSION"));
    assert!(
        inc.lines().any(|l| l.trim() == want),
        "include/json.inc lacks: {want}"
    );
}

#[test]
fn the_lock_file_has_no_git_source() {
    let lock = include_str!("../../../Cargo.lock");
    let git: Vec<&str> = lock.lines().filter(|l| l.contains("git+")).collect();
    assert!(git.is_empty(), "git sources in Cargo.lock: {git:?}");
}
