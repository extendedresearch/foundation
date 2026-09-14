# foundation

Shared code that ranvier, ca3 and eres depend on: the C ABI conventions, and
the binding layers each package's Python, Node and .NET bindings share.

## The layered architecture

Each package has a **safe Rust core** whose error type implements
`extendedresearch_abi::codes::AbiError`. Above it:

- **Python (PyO3) and Node (napi-rs) bindings call the core directly.** No raw
  handle and no `unsafe`. They convert the core's errors and enumerations with
  `extendedresearch-pyo3` and `extendedresearch-napi`.
- **A thin C-ABI adapter crate per package serves .NET and C callers.** It is
  the only code in a package that writes `unsafe`, and each block is one
  documented call into `extendedresearch_abi::borrow`. `codes::status` turns
  each core `Result` into the status it answers. The .NET binding reads that
  adapter with `ExtendedResearch.Interop`.

Error codes use this crate's numbering: `0` is success, `-1` to `-15` are the
boundary's (`codes.rs`), and each library numbers its own from `-16` down.

## What is here

| Path | What it is |
|---|---|
| `crates/abi` | `extendedresearch-abi`. Error codes and the `AbiError` trait, the only module that dereferences a caller's pointer, the measure-then-copy buffer shape, the panic guard, the table behind an enumeration's `_count`/`_at`/`_name`, the calling side C-ABI tests use, and a conformance kit a library runs against its own exports and error codes. It exports no `extern "C"` symbol. The crate docs in `crates/abi/src/lib.rs` state every convention and are the reference for them |
| `crates/pyo3` | `extendedresearch-pyo3`. `exceptions!` (a package's exception hierarchy, expanded in the consumer), `AbiError` to `PyErr`, and `IntEnum` from an `Enumeration` |
| `crates/napi` | `extendedresearch-napi`. The `"<PREFIX>_ERR_X: sentence"` error token protocol (`Tokens`), `BigInt` to `u64` refusing what does not fit, macros that expand to `#[napi]` exports in the consumer, and `ts/` — `errors.ts`, `harden.ts`, `enums.ts` — carried as `typescript::{ERRORS, HARDEN, ENUMS}` |
| `crates/interop-sources` | `extendedresearch-interop-sources`. The C# in `dotnet/Interop/` carried as `FILES`, with `assert_vendored` |
| `dotnet/Interop` | `ExtendedResearch.Interop`: internal C# a package compiles into its own assembly for `netstandard2.1` and `net8.0`. `AbiHandle` (a `SafeHandle` over `_destroy`), `AbiErrors` and the exception types, `AbiBuffer` (measure-then-copy), `AbiLibrary` (a `DllImport` resolver on net8.0), `AbiEnumeration`, `AbiCodes` |
| `dotnet/Interop.Build` | Compiles `dotnet/Interop` for both targets, C# 8, warnings as errors |
| `dotnet/Interop.Tests` | xUnit on net8.0, P/Invoking `crates/abi-testlib` |
| `crates/abi-testlib` | `publish = false`. A cdylib shaped like a package — a safe core and a C adapter over it — for the .NET tests and for Miri |
| `crates/napi-testaddon` | `publish = false`. A Node addon consuming `extendedresearch-napi`'s macros; `test/addon.test.mjs` loads it and runs `crates/napi/ts/` against it |
| `python/conformance` | `extendedresearch-conformance`, a pip-installable development tool: runs every language binding's driver over one cases file and compares each with the C header and with every other binding. Configured per repository by `conformance.toml`; standard library only |
| `.github/actions/prove-tests-ran` | A composite action that fails a job when a `cargo test` log shows no result line, zero passed tests, or ignored tests |
| `docs/conventions` | The rules a consuming package cites rather than copying from a sibling: the C library artefact's name, the toolchain pins, the workspace layout, the CI job split, the rustfmt edition, and the error token grammar. Each states its rule and the failure it prevents |
| `Cargo.toml` | The workspace. `[workspace.lints]` is the table a consuming package repeats, plus `undocumented_unsafe_blocks` |
| `.github/workflows/ci.yml` | Format, lints, docs and every suite on Linux, Windows and macOS; the Rust tests on the MSRV; Miri over the pointer-handling tests |

## How the packages reach it

**Rust: as a git dependency on this public repository, pinned to a commit:**

```toml
extendedresearch-abi = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

The repository is public, so resolving it needs no credential and no secret in
any package's CI. A `rev` is as fixed as a registry version: the lockfile
records the commit, and only an edit to the manifest moves it. A branch or a
tag would move under an unchanged manifest at the next `cargo update`, so
neither is used.

**This is the one dependency from outside its own tree that a core package
takes.** A package that checks that nothing in its `Cargo.lock` resolves over
git relaxes that check to "nothing resolves over git except this repository,
pinned to a commit":

```bash
grep 'source = "git' Cargo.lock \
  | grep -cvE '^source = "git\+https://github\.com/extendedresearch/foundation\?rev=[0-9a-f]{40}#[0-9a-f]{40}"$'
# 0
```

**TypeScript and C#: vendored, with a drift test.** npm cannot install a
subdirectory of a git repository and NuGet cannot read one, so a package commits
a copy of `crates/napi/ts/*.ts` or `dotnet/Interop/*.cs` into its own binding
and calls `extendedresearch_napi::typescript::assert_vendored(dir)` or
`extendedresearch_interop_sources::assert_vendored(dir)` from a Rust test. The
copy then differs from the constant at the pinned commit only as a red test.

**The conformance runner: pip, from a git subdirectory:**

```bash
pip install "extendedresearch-conformance @ git+https://github.com/extendedresearch/foundation@<commit>#subdirectory=python/conformance"
```

**The CI action: by commit**, as
`extendedresearch/foundation/.github/actions/prove-tests-ran@<commit>`.

**Two packages pinning different commits put two copies of each crate in one
build**, and Cargo treats them as different crates. Nothing from here crosses a
package boundary today — codes are plain `i32`, the helpers are functions, and
the exception and export macros expand in each consumer — so the copies
coexist. A package that exposes one of these crates' types in its own public
API ties every package built beside it to the same commit.

**crates.io is for when the API is settled.** Until then nothing is published
and no version is spent: a package moves by changing its `rev`. A crate with a
git dependency cannot itself be published to crates.io, and no core package is
today.

**A crate lands here when two packages need the same runtime code.** A crate
with one consumer belongs in that consumer's repository. `abi-testlib` and
`napi-testaddon` are the exception: they are test fixtures for the crates here,
and `publish = false`.

## What is built, and what is not

Built, with the check that shows it beside each:

- `crates/abi`, `crates/pyo3`, `crates/napi`, `crates/interop-sources`,
  `crates/abi-testlib`: `cargo test --workspace`, on stable and on 1.85.
- The napi macros registering in a real consumer, and the TypeScript running
  against it: `node --test crates/napi-testaddon/test/addon.test.mjs`.
- `ExtendedResearch.Interop` compiling for `netstandard2.1` and `net8.0`
  (`dotnet build dotnet/Interop.Build`) and running against a Rust library on
  net8.0 (`dotnet test dotnet/Interop.Tests`).
- The conformance runner: `python -m unittest discover -s python/conformance/tests`.

Not built, or not decided:

- **Nothing is published, and nothing is versioned.** Every crate stays at
  `0.0.0`, and packages pin a commit rather than a version. No code, name or
  signature is frozen until the first version is complete.
- **`extendedresearch-interop-sources` cannot be packaged for crates.io.** It
  reads `dotnet/Interop/` from outside its crate directory, which a git
  dependency resolves and `cargo package` does not.
- **Where the rest of the shared tooling lives** — the lint table, the
  decision-record checker — is undecided. The zero-tests step and the
  conformance runner are here.
- **No enumeration of a library's statuses crosses the C ABI.** A .NET
  binding writes its domain codes down and checks them against the header in a
  test. An enumeration trio over the status table (`_status_count`, `_at`,
  `_name`, the shape `Enumeration` already serves) would let it read them, and
  is not decided.

## Conventions that cause bugs when broken

- **`unsafe` is written in two places.** `crates/abi/src/borrow.rs`, allowed on
  `mod borrow` alone; and `crates/abi-testlib`, a test fixture whose exports
  need `#[unsafe(no_mangle)]`, allowed at its crate root. The workspace denies
  `unsafe_code` everywhere else, and `crates/abi/tests/discipline.rs` fails on a
  second `allow` inside `crates/abi`:

  ```bash
  grep -rn 'allow(unsafe_code)' crates/*/src
  # crates/abi-testlib/src/lib.rs:27:#![allow(unsafe_code)]
  # crates/abi/src/lib.rs:66:#[allow(unsafe_code)]
  ```
- **Every function that takes a raw pointer is `unsafe fn`**, including a C
  adapter's `extern "C"` exports. A safe public function that dereferences a raw
  pointer is unsound, because safe code can pass any address.
- **A caller's buffer and out-parameters are `MaybeUninit`.** A C caller passes
  uninitialised memory, and a `&mut [u8]` over it is undefined behaviour whether
  or not it is read. Miri checks this; an ordinary test run cannot.
- **Codes `-1` to `-15` are this crate's to add, and a library numbers its own
  from `-16` down.** A code added to the boundary range by a library collides
  with the next one added here. `conformance::error_codes` checks a library's
  table.
- **`AbiError::name` is unprefixed for boundary codes, and every binding
  reports the header's spelling.** A boundary error answers `ERR_NULL`; a
  domain error answers its full name, `EXAMPLE_ERR_TRUNCATED`. `codes::token`
  adds the package prefix to the first kind (`EXAMPLE_ERR_NULL`) and leaves the
  second alone, and all three layers apply it: napi's `Tokens`, the pyo3
  family's required `prefix`, and `AbiErrors` in .NET. A layer that skipped it
  would report a name no header declares. `docs/conventions/error-tokens.md`
  states the grammar in full.
- **A package's `statusCodes()` reports the boundary codes its header
  declares**, which it passes to `status_exports!`; foundation naming a code
  does not put it in a package's table.
- **pyo3 and napi move in lockstep with every consumer.** `pyo3-ffi` declares
  `links = "python"`, so a consumer on a different pyo3 series fails to resolve.
  `napi-sys` declares no `links`, so a consumer on a different napi series
  builds a second copy silently, and a `napi::Error` from here is not the
  consumer's. Consumers require `pyo3 = "0.29"` and `napi = "3"`,
  `napi-derive = "3"`, the series `docs/conventions/toolchain-pins.md` pins;
  `cargo tree -i pyo3-ffi` and `cargo tree -i napi` in the consumer each list
  one version.
- **Exception classes and `#[napi]` exports are created in the consumer**, by
  `extendedresearch_pyo3::exceptions!` and the `extendedresearch_napi` export
  macros. An exception class created in a shared crate is one per extension
  module linking it; whether a `#[napi]` item in a dependency registers with the
  consumer's module is unverified.
- **A vendored TypeScript or C# file is edited here, never in the package.**
  The package's drift test fails on any difference from the pinned commit, so
  an edit made there is undone by the next update or blocks it.
- **`*out_len` never counts a string's terminator, and `capacity` always must.**
- **The MSRV is 1.85, because the workspace is on edition 2024 and 1.85 is the
  release that stabilised it.** Nothing here asks for a later compiler, so the
  floor is the edition's and not a dependency's: pyo3 0.29 declares 1.83 and
  napi 3 declares 1.82, both below it. A consumer on a newer toolchain is
  unaffected; a consumer on an older one could not compile an edition-2024
  crate at all. `docs/conventions/toolchain-pins.md` carries the pin and the
  `msrv` job in `ci.yml` is what holds it.
- **Each published crate and `python/conformance` carry their own copy of
  `LICENSE` and `NOTICE`.** A package is built from its own directory, so the
  root files do not reach a user who downloads it, and Apache-2.0 requires
  `NOTICE` to travel with a redistribution. Each crate's `tests/license.rs` and
  the conformance self-test fail when a copy drifts from the root.

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo +1.85 test --workspace
# Miri cannot read files, so the discipline test and doctests are left out.
cargo +nightly miri test -p extendedresearch-abi --lib --test borrow --test buffer --test conformance --test enumeration --test binding
cargo +nightly miri test -p extendedresearch-abi-testlib --test conformance

cargo build -p extendedresearch-napi-testaddon
node --test crates/napi-testaddon/test/addon.test.mjs     # Node 22.18 or later

python -m unittest discover -s python/conformance/tests   # Python 3.11 or later

dotnet build dotnet/Interop.Build
cargo build -p extendedresearch-abi-testlib
dotnet test dotnet/Interop.Tests
```

`crates/napi/tsconfig.json` type-checks `crates/napi/ts/` with `tsc -p crates/napi`.
TypeScript is not a dependency of this repository, so CI does not run it; a
consuming package's own `tsc` compiles its vendored copy.
