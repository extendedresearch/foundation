//! The carried C# is every file in `dotnet/Interop`, its codes are
//! foundation's, and the drift check catches drift.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use extendedresearch_abi::codes;
use extendedresearch_interop_sources::{FILES, assert_vendored};

fn sources_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dotnet/Interop")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "extendedresearch-interop-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn source(name: &str) -> &'static str {
    FILES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, text)| *text)
        .unwrap()
}

#[test]
fn every_cs_file_is_carried_and_nothing_else_is() {
    let mut on_disk: Vec<String> = std::fs::read_dir(sources_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".cs"))
        .collect();
    on_disk.sort();
    let carried: Vec<String> = FILES.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(on_disk, carried, "FILES is kept in sorted order");
}

/// `ErrNull` to `ERR_NULL`, `DomainFloor` to `DOMAIN_FLOOR`.
fn screaming(camel: &str) -> String {
    let mut out = String::new();
    for (at, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

#[test]
fn every_csharp_code_is_foundation_s() {
    let mut found: Vec<(String, i32)> = Vec::new();
    for line in source("AbiCodes.cs").lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("public const int ") else {
            continue;
        };
        let (name, value) = rest.trim_end_matches(';').split_once(" = ").unwrap();
        found.push((screaming(name), value.parse().unwrap()));
    }
    let expected: Vec<(String, i32)> = [
        ("OK", codes::OK),
        ("ERR_NULL", codes::ERR_NULL),
        ("ERR_RANGE", codes::ERR_RANGE),
        ("ERR_UTF8", codes::ERR_UTF8),
        ("ERR_PANIC", codes::ERR_PANIC),
        ("ERR_STATE", codes::ERR_STATE),
        ("DOMAIN_FLOOR", codes::DOMAIN_FLOOR),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect();
    assert_eq!(found, expected);
}

#[test]
fn every_csharp_boundary_name_is_foundation_s() {
    let mut named = 0;
    for line in source("AbiCodes.cs").lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("case ") else {
            continue;
        };
        let (constant, answer) = rest.split_once(": return \"").unwrap();
        let name = answer.trim_end_matches("\";");
        let code = match constant {
            "ErrNull" => codes::ERR_NULL,
            "ErrRange" => codes::ERR_RANGE,
            "ErrUtf8" => codes::ERR_UTF8,
            "ErrPanic" => codes::ERR_PANIC,
            "ErrState" => codes::ERR_STATE,
            other => {
                panic!("AbiCodes.BoundaryName names {other}, which foundation has no code for")
            }
        };
        assert_eq!(codes::name(code), Some(name), "{constant}");
        named += 1;
    }
    let boundary = (codes::DOMAIN_FLOOR + 1..0)
        .filter(|code| codes::name(*code).is_some())
        .count();
    assert_eq!(
        named, boundary,
        "every boundary code foundation names has a C# name"
    );
}

#[test]
fn the_repository_s_own_directory_passes() {
    assert_vendored(sources_dir());
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
fn an_edited_or_missing_copy_fails() {
    let dir = scratch("edited");
    for (name, contents) in FILES.iter().skip(1) {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    std::fs::write(dir.join("AbiHandle.cs"), "// edited\n").unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| assert_vendored(&dir)));
    let message = *outcome.unwrap_err().downcast::<String>().unwrap();
    assert!(message.contains("AbiHandle.cs differs"), "{message}");
    assert!(
        message.contains(&format!(
            "{} cannot be read",
            dir.join(FILES[0].0).display()
        )),
        "{message}"
    );
    assert!(!message.contains("AbiErrors.cs"), "{message}");
}
