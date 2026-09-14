//! The boundary codes and names `dotnet/Interop/AbiCodes.cs` states are
//! `extendedresearch-abi`'s.
//!
//! The C# is the .NET form of `codes`, written by hand, so nothing but this
//! comparison notices a value or a name that differs. It sits in this crate,
//! the library the .NET tests call, because it reads a file from outside its
//! own directory and this crate is never packaged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::path::Path;

use extendedresearch_abi::codes;

fn abi_codes_cs() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dotnet/Interop/AbiCodes.cs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
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
    for line in abi_codes_cs().lines().map(str::trim) {
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
    for line in abi_codes_cs().lines().map(str::trim) {
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
