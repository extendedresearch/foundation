//! `borrow` stays the only module that may write `unsafe`.
//!
//! The compiler holds this through `unsafe_code = "deny"` and a single
//! `#[allow(unsafe_code)]` on `mod borrow`. What the compiler cannot see is
//! somebody adding a second `allow`, or deleting the denial, so this test reads
//! the source.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fs;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Lines that are code rather than comment, so documentation that quotes a
/// pattern does not read as a match. The comment marker differs by language,
/// and a `#` must not be stripped from Rust, where it starts an attribute.
fn code_lines<'a>(text: &'a str, comment: &'a str) -> impl Iterator<Item = &'a str> {
    text.lines()
        .map(str::trim)
        .filter(move |line| !line.starts_with(comment))
}

fn toml_lines(text: &str) -> impl Iterator<Item = &str> {
    code_lines(text, "#")
}

fn denies_unsafe_code(manifest: &str) -> bool {
    toml_lines(manifest).any(|line| line.replace(' ', "") == "unsafe_code=\"deny\"")
}

fn inherits_workspace_lints(manifest: &str) -> bool {
    let mut lines = toml_lines(manifest).filter(|line| !line.is_empty());
    lines.any(|line| line == "[lints]")
        && lines.next().map(|line| line.replace(' ', "")) == Some("workspace=true".to_owned())
}

#[test]
fn unsafe_code_is_denied_for_the_crate() {
    let manifest = read(&crate_dir().join("Cargo.toml"));
    // A packaged crate carries the workspace's lints written into its own
    // manifest, so either form holds the property.
    if denies_unsafe_code(&manifest) {
        return;
    }
    assert!(
        inherits_workspace_lints(&manifest),
        "the crate must either deny `unsafe_code` itself or take `[lints] workspace = true`"
    );
    let workspace = read(&crate_dir().join("../../Cargo.toml"));
    assert!(
        denies_unsafe_code(&workspace),
        "the workspace must keep `unsafe_code = \"deny\"` under [workspace.lints.rust]"
    );
}

#[test]
fn only_borrow_is_allowed_unsafe() {
    let mut allows = Vec::new();
    for entry in fs::read_dir(crate_dir().join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = read(&path);
        let lines: Vec<&str> = code_lines(&text, "//").collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("allow(unsafe_code)") {
                let next = lines.get(i + 1).copied().unwrap_or_default();
                allows.push(format!(
                    "{}: {line} / {next}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    assert_eq!(
        allows,
        ["lib.rs: #[allow(unsafe_code)] / pub mod borrow;"],
        "exactly one allow(unsafe_code), on `mod borrow` in lib.rs"
    );
}
