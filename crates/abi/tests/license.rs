//! The crate ships its own LICENSE and NOTICE, and they match the repository's.
//!
//! A crate is packaged from its own directory, so a file at the repository root
//! does not reach a user who downloads it. Apache-2.0 §4(d) requires a
//! redistribution to carry the NOTICE file, so a copy of each lives beside this
//! crate's manifest, and this test fails when a copy drifts from the root.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fs;
use std::path::{Path, PathBuf};

/// Contents with line endings normalised, so a checkout's `eol` setting is not
/// read as drift.
fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.replace("\r\n", "\n"))
}

fn ships_and_matches(name: &str) {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let copy = read(&crate_dir.join(name)).unwrap_or_else(|| {
        panic!("crates/abi/{name} is missing, so the published crate would ship without it")
    });
    // A packaged crate has no repository root above it, and the copy is then
    // all there is to check.
    if let Some(root) = read(&crate_dir.join("../..").join(name)) {
        assert_eq!(
            copy, root,
            "crates/abi/{name} differs from the repository's {name}; copy the root file over it"
        );
    }
}

#[test]
fn the_license_ships_with_the_crate() {
    ships_and_matches("LICENSE");
}

#[test]
fn the_notice_ships_with_the_crate() {
    ships_and_matches("NOTICE");
}
