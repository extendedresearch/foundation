//! The carried TypeScript is the file on disk, and the drift check catches
//! drift.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use extendedresearch_napi::typescript::{FILES, assert_vendored};

fn ts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ts")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "extendedresearch-napi-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_file_in_ts_is_carried_and_nothing_else_is() {
    let mut on_disk: Vec<String> = std::fs::read_dir(ts_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut carried: Vec<String> = FILES.iter().map(|(name, _)| (*name).to_owned()).collect();
    carried.sort();
    assert_eq!(on_disk, carried);
}

#[test]
fn the_crate_s_own_directory_passes() {
    assert_vendored(ts_dir());
}

#[test]
fn a_copy_with_other_line_endings_passes() {
    let dir = scratch("crlf");
    for (name, contents) in FILES {
        std::fs::write(dir.join(name), contents.replace('\n', "\r\n")).unwrap();
    }
    assert_vendored(&dir);
}

#[test]
fn an_edited_copy_fails() {
    let dir = scratch("edited");
    for (name, contents) in FILES {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    std::fs::write(dir.join("enums.ts"), "export {};\n").unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| assert_vendored(&dir)));
    let message = *outcome.unwrap_err().downcast::<String>().unwrap();
    assert!(message.contains("enums.ts differs"), "{message}");
    assert!(!message.contains("errors.ts"), "{message}");
}

#[test]
fn a_missing_copy_fails() {
    let dir = scratch("missing");
    let outcome = catch_unwind(AssertUnwindSafe(|| assert_vendored(&dir)));
    let message = *outcome.unwrap_err().downcast::<String>().unwrap();
    for (name, _) in FILES {
        assert!(message.contains(name), "{message}");
    }
}
