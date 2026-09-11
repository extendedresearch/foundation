//! The C# sources of `ExtendedResearch.Interop`, carried as text so a package
//! can vendor them.
//!
//! `ExtendedResearch.Interop` is what a package's .NET binding shares with every
//! other package's: a `SafeHandle` base whose release calls the library's
//! `_destroy`, the mapping from a status code to an exception, measure-then-copy
//! reading of text and bytes, a native library resolver, the ABI version check,
//! and reading an enumeration's `_count`/`_at`/`_name` into a list. Every type
//! is `internal`; a package compiles the files into its own assembly, which
//! builds for `netstandard2.1` (Unity's Mono and IL2CPP, through `[DllImport]`)
//! and `net8.0`.
//!
//! NuGet cannot install from a git repository, so a package commits a copy of
//! [`FILES`] and asserts it with [`assert_vendored`]:
//!
//! ```text
//! #[test]
//! fn vendored_csharp_matches_foundation() {
//!     extendedresearch_interop_sources::assert_vendored(
//!         std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dotnet/Interop"),
//!     );
//! }
//! ```
//!
//! The sources are `dotnet/Interop/*.cs` in foundation; `dotnet/Interop.Build`
//! compiles them for both targets and `dotnet/Interop.Tests` runs them against
//! `crates/abi-testlib`.

use std::path::Path;

/// Every file, as `(path relative to dotnet/Interop, contents)`.
pub const FILES: &[(&str, &str)] = &[
    (
        "AbiBuffer.cs",
        include_str!("../../../dotnet/Interop/AbiBuffer.cs"),
    ),
    (
        "AbiCodes.cs",
        include_str!("../../../dotnet/Interop/AbiCodes.cs"),
    ),
    (
        "AbiEnumeration.cs",
        include_str!("../../../dotnet/Interop/AbiEnumeration.cs"),
    ),
    (
        "AbiErrors.cs",
        include_str!("../../../dotnet/Interop/AbiErrors.cs"),
    ),
    (
        "AbiException.cs",
        include_str!("../../../dotnet/Interop/AbiException.cs"),
    ),
    (
        "AbiHandle.cs",
        include_str!("../../../dotnet/Interop/AbiHandle.cs"),
    ),
    (
        "AbiLibrary.cs",
        include_str!("../../../dotnet/Interop/AbiLibrary.cs"),
    ),
];

/// Panic when a package's vendored copy of any of [`FILES`] under `directory`
/// differs from this crate's, or is missing.
///
/// Line endings are normalised first, so a checkout's `eol` setting is not
/// drift.
///
/// # Panics
///
/// Naming every file that is missing or differs, and what fixes it.
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
        "the vendored C# has drifted from extendedresearch-interop-sources at the pinned commit:\n  {}\n\
         Copy dotnet/Interop/ from that commit of foundation over the vendored files.",
        drifted.join("\n  "),
    );
}

fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n")
}
