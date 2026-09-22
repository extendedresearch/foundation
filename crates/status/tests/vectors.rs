//! The shared-rule vector for the error-token grammar, run against this
//! crate's implementation of it.
//!
//! The grammar is written twice: `codes::token` here, and `AbiErrors.NameOf`
//! in `dotnet/Interop/AbiErrors.cs`, which `dotnet/Interop.Tests` runs against
//! the same file. Neither can call the other — the C# is compiled into a
//! consumer's own assembly from source, with no Rust anywhere near it — so the
//! rule is registered as a `[[shared_rule]]` in `ecosystem/PACKAGES.toml` and
//! this file is what holds the two to one answer.
//!
//! `crates/napi` and `crates/pyo3` both call `codes::token` rather than
//! spelling the grammar again, so they are the same implementation and not a
//! third.
//!
//! A row is never edited to match an implementation. When one fails, either the
//! implementation is wrong or the rule `docs/conventions/error-tokens.md`
//! states changed.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "support/reader.rs"]
mod reader;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use extendedresearch_status::codes::{self, OK};
use reader::{Value, read};

const VECTOR: &str = "0001-an-error-token-is-the-header-spelling-of-a-constant";

/// The token for a code: the name comes from the shared boundary table or from
/// what the package registered, and the same prefix step applies to either.
///
/// This is the join `AbiErrors.NameOf` performs in C#. It is written here in
/// the test rather than in the crate because the crate never sees a package's
/// registration — a package's own error type answers its name, and `token` is
/// the one step over it.
fn name_of(prefix: &str, code: i32, domain: &Value) -> String {
    if code == OK {
        return codes::token(prefix, "OK");
    }
    if let Some(boundary) = codes::name(code) {
        return codes::token(prefix, boundary);
    }
    let registered = domain
        .members()
        .iter()
        .find(|(at, _)| at.parse::<i32>() == Ok(code));
    match registered {
        Some((_, name)) => codes::token(prefix, name.text()),
        None => codes::token(prefix, "ERR_UNKNOWN"),
    }
}

fn observe(name: &str, input: &Value) -> String {
    let prefix = input.text_at("prefix");
    match input.text_at("call") {
        "token" => codes::token(prefix, input.text_at("name")),
        "name_of" => {
            let code: i32 = input.text_at("code").parse().expect("a decimal i32");
            name_of(prefix, code, input.at("domain"))
        }
        other => panic!("{name}: no runner for call {other:?}"),
    }
}

#[test]
fn every_row_of_the_shared_vector_passes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vectors")
        .join(format!("{VECTOR}.json"));
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let vector = read(&text);

    assert_eq!(vector.text_at("shared_rule_vector"), "0", "format version");
    assert_eq!(vector.text_at("id"), VECTOR, "id equals the file name");
    assert!(!vector.text_at("about").is_empty(), "about");
    assert_eq!(vector.text_at("function"), "error_token");

    let mut failures = Vec::new();
    let mut checked = 0;
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    for (name, row) in vector.at("rows").members() {
        assert!(
            seen.insert(name, ()).is_none(),
            "row {name} appears more than once"
        );
        let want = row.at("expect").text();
        let got = observe(name, row.at("input"));
        checked += 1;
        if got != want {
            failures.push(format!("{name}: expected {want}, got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} vector rows failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guards against a runner that silently reads nothing.
    assert!(checked >= 20, "only {checked} rows checked");
}
