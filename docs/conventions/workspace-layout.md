# Workspace layout

One workspace holds the library crates and the bindings. `tools/` sits outside
it, and each package under `tools/` is a workspace of its own.

```toml
# Cargo.toml, at the repository root
[workspace]
resolver = "3"
members = [
    "crates/example",
    "crates/example-abi",
    "bindings/python",
    "bindings/javascript",
]
exclude = ["tools"]
```

## `tools/` is excluded, deliberately

A development tool is not part of the artefact the repository ships. It pulls a
dependency tree the library does not — an argument parser, a template engine, a
`git2` — and a member of the root workspace shares one `Cargo.lock` and one
feature resolution with every other member. So a tool's dependency ends up
constraining the library's, `cargo test --workspace` compiles the tool before it
runs a test, and a `cargo update` for the tool touches the lockfile the library
publishes against.

`exclude = ["tools"]` ends all of that. What it costs is that nothing under
`tools/` is reached by a command run from the root, and that has to be made up
for explicitly.

### Each package under `tools/` carries an empty `[workspace]` table

```toml
# tools/whatever/Cargo.toml
[workspace]

[package]
name = "whatever"
```

Without it, Cargo walks up from the package looking for a workspace root,
finds the repository's, and reports that the package is neither a member nor
excluded — or, worse, silently joins one if the exclusion is later edited. The
empty table declares the package its own workspace root and stops the walk.

Each such package therefore has its own `Cargo.lock`, and that lockfile is
committed like any other.

### And is reached by `--manifest-path`

Every command that runs over the root workspace runs again, once per detached
package, pointed at its manifest. Format and lint included — an excluded
package is invisible to `cargo fmt --all` and `cargo clippy --workspace`, and
"the tool was never formatted" is exactly the kind of thing that is noticed
only when someone finally formats it in an unrelated change:

```bash
for manifest in tools/*/Cargo.toml; do
  cargo fmt --manifest-path "$manifest" --all --check
  cargo clippy --manifest-path "$manifest" --all-targets -- -D warnings
  cargo test --manifest-path "$manifest"
done
```

### A detached package inherits nothing, so it copies

`edition`, `rust-version`, `license` and `repository` from
`[workspace.package]`, and the levels in `[workspace.lints]`, reach members of
the root workspace. A package under `tools/` is not a member, so
`edition.workspace = true` there does not resolve, and — the part that is easy
to miss — `[lints] workspace = true` resolves against the package's *own* empty
workspace table and applies nothing.

Write the values out in the detached manifest:

```toml
[package]
edition = "2024"
rust-version = "1.85"

[lints.rust]
unsafe_code = "deny"
missing_docs = "warn"

[lints.clippy]
unwrap_used = "warn"
expect_used = "warn"
indexing_slicing = "warn"
```

These are copies, and copies drift. The pins they copy are in
[toolchain-pins.md](toolchain-pins.md); a change there is a change in every
detached manifest in the same commit.

## `bindings/*` is written out, not globbed

`members` lists each binding by path. `crates/*` may be globbed, because every
directory under it is a Rust crate; `bindings/*` may not, because a binding
need not have any Rust in it.

A pure-TypeScript binding, a .NET binding that is a `.csproj` and a folder of
C#, a Python binding that is a wheel built from an existing `cdylib` — each is
a directory under `bindings/` with no `Cargo.toml`. A glob that matches a
directory without a manifest is a workspace error:

```text
error: failed to load manifest for workspace member `.../bindings/dotnet`
```

The error arrives when the binding is added, which sounds harmless, except that
what it stops is every Cargo command in the repository at once, for a reason
unrelated to the change. Listing the Rust bindings by name means adding a
non-Rust binding changes no manifest.

The cost is that adding a Rust binding needs an edit to the root manifest, and
forgetting it means the binding is not built. `cargo test --workspace` passing
while a directory is untested is the failure to guard against; a CI job that
lists the workspace members it expects is what catches it:

```bash
cargo metadata --no-deps --format-version 1 \
  | python -c 'import json,sys; [print(p["name"]) for p in json.load(sys.stdin)["packages"]]' \
  | sort
```
