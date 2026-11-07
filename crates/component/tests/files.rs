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

/// The plugin catches a panic at every entry point so a bug costs one call, not the
/// server. That needs unwinding: with `panic = "abort"` the process would die before
/// the catch.
#[test]
fn the_release_build_unwinds() {
    let manifest = include_str!("../../../Cargo.toml");
    let release = manifest
        .split("[profile.release]")
        .nth(1)
        .expect("a release profile")
        .split(
            "
[",
        )
        .next()
        .unwrap();
    assert!(
        !release.contains("panic"),
        "the release profile sets a panic strategy:{release}"
    );
}

/// The codes the include declares are the codes the natives return.
#[test]
fn the_include_s_codes_are_the_plugin_s() {
    let inc = include_str!("../../../include/json.inc");
    let declared: Vec<(String, i32)> = inc
        .lines()
        .filter_map(|l| l.strip_prefix("const JSON_ERR_"))
        .map(|l| {
            let (name, value) = l.trim_end_matches(';').split_once(" = ").unwrap();
            (name.to_owned(), value.parse().unwrap())
        })
        .collect();
    use pawn_json_core::error::Error::*;
    let ours = [
        ("NODE", Node),
        ("CONTAINER", Container),
        ("KEY", Key),
        ("TYPE", Type),
        ("BUFFER", Buffer),
        ("RANGE", Range),
        ("SYNTAX", Syntax),
        ("FILE", File),
        ("PATH", Path),
        ("FORMAT", Format),
        ("ADDRESS", Address),
        ("INTERNAL", Internal),
    ];
    assert_eq!(declared.len(), ours.len(), "{declared:?}");
    for (name, e) in ours {
        assert!(
            declared.contains(&(name.to_owned(), e as i32)),
            "JSON_ERR_{name} is not {} in the include",
            e as i32
        );
    }
}
