//! The TypeScript half, carried as text so a package can vendor it.
//!
//! npm cannot install a subdirectory of a git repository, so a package does
//! not depend on these files; it commits a copy beside its own TypeScript and
//! calls [`assert_vendored`] from a Rust test. The copy is then the one `tsc`
//! compiles and ships, and the test goes red the day it differs from the
//! version this crate carries at the pinned commit.
//!
//! Each file imports nothing, so a package can vendor any subset.

use std::path::Path;

/// `ts/errors.ts`: the error base class and the factory that turns the native
/// half's `"<TOKEN>: sentence"` reports into the package's error classes.
pub const ERRORS: &str = include_str!("../ts/errors.ts");

/// `ts/harden.ts`: wraps every method and getter of a native class, and any
/// free function, so a failure arrives translated.
pub const HARDEN: &str = include_str!("../ts/harden.ts");

/// `ts/enums.ts`: frozen enumeration contracts from `EnumMember[]`, with the
/// short-name rule `extendedresearch-pyo3` applies.
pub const ENUMS: &str = include_str!("../ts/enums.ts");

/// Every file, by the name a package commits it under.
pub const FILES: &[(&str, &str)] = &[
    ("errors.ts", ERRORS),
    ("harden.ts", HARDEN),
    ("enums.ts", ENUMS),
];

/// Panic when a package's vendored copy of any of [`FILES`] in `directory`
/// differs from this crate's, or is missing.
///
/// Line endings are normalised first, so a checkout's `eol` setting is not
/// drift. Call it from a test in the package:
///
/// ```text
/// #[test]
/// fn vendored_typescript_matches_foundation() {
///     extendedresearch_napi::typescript::assert_vendored(
///         std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ts/vendor"),
///     );
/// }
/// ```
///
/// # Panics
///
/// Naming every file that is missing or differs, and the command that fixes
/// it.
pub fn assert_vendored(directory: impl AsRef<Path>) {
    let directory = directory.as_ref();
    let drifted: Vec<String> = FILES
        .iter()
        .filter_map(|(name, expected)| {
            let path = directory.join(name);
            match std::fs::read_to_string(&path) {
                Ok(found) if normalise(&found) == normalise(expected) => None,
                Ok(_) => Some(format!("{} differs", path.display())),
                Err(error) => Some(format!("{} cannot be read: {error}", path.display())),
            }
        })
        .collect();
    assert!(
        drifted.is_empty(),
        "the vendored TypeScript has drifted from extendedresearch-napi at the pinned commit:\n  {}\n\
         Copy crates/napi/ts/ from that commit of foundation over the vendored files.",
        drifted.join("\n  "),
    );
}

fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n")
}
