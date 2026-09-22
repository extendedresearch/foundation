//! The shared-rule vector for the enumeration short-name rule, run against
//! this crate's implementation of it.
//!
//! The same rule is written in TypeScript in
//! `npm/binding-runtime/src/enums.ts`, and
//! `crates/napi-testaddon/test/addon.test.mjs` runs that one against the same
//! file. Neither implementation depends on the other, which is why the rule is
//! registered as a `[[shared_rule]]` in `ecosystem/PACKAGES.toml`: the
//! duplication is the design, and the vector is what keeps it honest.
//!
//! A row is never edited to match an implementation. When one fails, either the
//! implementation is wrong or the rule the vector states changed. Every row's
//! result is collected and the test fails once with all of them.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "support/reader.rs"]
mod reader;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use extendedresearch_abi::enumeration::Enumeration;
use extendedresearch_pyo3::{member_names, shared_prefix, short_name};
use reader::{Value, read};

const VECTOR: &str = "0001-enumeration-short-names-strip-one-shared-prefix";

/// `Enumeration` holds `&'static str`, and a vector's names are read at run
/// time, so a row's table outlives the row.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn names_of(input: &Value) -> Vec<&'static str> {
    input
        .at("names")
        .list()
        .iter()
        .map(|one| leak(one.text()))
        .collect()
}

fn table_of(input: &Value) -> Enumeration {
    let entries: Vec<(i32, &'static str)> = input
        .at("members")
        .list()
        .iter()
        .map(|one| {
            let value: i32 = one.text_at("value").parse().expect("a decimal i32");
            (value, leak(one.text_at("name")))
        })
        .collect();
    Enumeration::new(Box::leak(entries.into_boxed_slice()))
}

/// What this crate answers for one row, flattened to `row` or `row.field` the
/// way the expectations are.
fn observe(name: &str, input: &Value, out: &mut BTreeMap<String, String>) {
    match input.text_at("call") {
        // The vector holds the prefix as text because TypeScript computes a
        // string and this crate computes a byte count. Both are the same run of
        // bytes off the front of the first name.
        "shared_prefix" => {
            let names = names_of(input);
            let strip = shared_prefix(&names);
            let prefix = names.first().map_or("", |first| &first[..strip]);
            out.insert(name.to_owned(), prefix.to_owned());
        }
        "short_name" => {
            let member = input.text_at("name");
            let prefix = input.text_at("prefix");
            out.insert(name.to_owned(), short_name(member, prefix.len()).to_owned());
        }
        "short_names" => {
            let table = table_of(input);
            match member_names(&table) {
                Err(_) => {
                    out.insert(name.to_owned(), "refused:short_name_collision".to_owned());
                }
                Ok(bound) => {
                    // The short name is the first name bound to a member's
                    // value; the contract's own name follows it as an alias
                    // when the two differ.
                    for (value, member) in table.entries() {
                        let short = bound
                            .iter()
                            .find(|(_, bound)| bound == value)
                            .map_or("<unbound>", |(short, _)| *short);
                        out.insert(format!("{name}.{member}"), short.to_owned());
                    }
                }
            }
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
    assert_eq!(vector.text_at("function"), "enumeration_short_names");

    let mut want: BTreeMap<String, String> = BTreeMap::new();
    let mut got: BTreeMap<String, String> = BTreeMap::new();
    for (name, row) in vector.at("rows").members() {
        match row.at("expect") {
            Value::Text(one) => {
                want.insert(name.clone(), one.clone());
            }
            expect => {
                let members = expect.members();
                assert!(!members.is_empty(), "row {name} expects an empty object");
                for (field, value) in members {
                    want.insert(format!("{name}.{field}"), value.text().to_owned());
                }
            }
        }
        observe(name, row.at("input"), &mut got);
    }

    let mut failures = Vec::new();
    for (key, value) in &want {
        match got.get(key) {
            Some(one) if one == value => {}
            Some(one) => failures.push(format!("{key}: expected {value}, got {one}")),
            None => failures.push(format!("{key}: expected {value}, got nothing")),
        }
    }
    for key in got.keys().filter(|k| !want.contains_key(*k)) {
        failures.push(format!("{key}: produced a value no row expects"));
    }
    assert!(
        failures.is_empty(),
        "{} of {} vector rows failed:\n{}",
        failures.len(),
        want.len(),
        failures.join("\n")
    );
    // Guards against a runner that silently reads nothing.
    assert!(want.len() >= 20, "only {} rows checked", want.len());
}
