# extendedresearch-interop-sources

The C# sources of `ExtendedResearch.Interop`, carried as Rust string constants
so a package can vendor them into its .NET binding.

`ExtendedResearch.Interop` is the part of a .NET binding every package writes
the same way: a `SafeHandle` base whose release calls the library's `_destroy`,
status-to-exception mapping, measure-then-copy reading of text and bytes, a
native library resolver, the ABI version check, and reading an enumeration's
`_count`/`_at`/`_name` into a list. Every type is `internal`. Compile the files
into your own assembly, which builds for `netstandard2.1` and `net8.0`.

NuGet cannot install from a git repository, so commit a copy of the files and
check it from a Rust test:

```rust
#[test]
fn vendored_csharp_matches_foundation() {
    extendedresearch_interop_sources::assert_vendored(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dotnet/Interop"),
    );
}
```

The test fails when your copy differs from the one at the commit you pin.

The sources live at `dotnet/Interop/` in foundation. This crate reads them
from there, so it resolves as a git dependency and cannot be packaged for
crates.io as it stands.

## Status

0.0.0, consumed as a git dependency pinned by `rev`; nothing is frozen.
Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
