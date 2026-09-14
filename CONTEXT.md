# foundation

Shared code that ranvier, ca3 and eres depend on: the C ABI conventions, and
the binding layers each package's Python, Node and .NET bindings share.

## The layered architecture

Each package has a **safe Rust core** whose error type implements
`extendedresearch_abi::codes::AbiError`. Above it:

- **Python (PyO3) and Node (napi-rs) bindings call the core directly.** No raw
  handle and no `unsafe`. They convert the core's errors and enumerations with
  `extendedresearch-pyo3` and `extendedresearch-napi`, and a Node binding's
  TypeScript raises typed errors with `@extendedresearch/binding-runtime`.
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
| `crates/napi` | `extendedresearch-napi`. The `"<PREFIX>_ERR_X: sentence"` error token protocol (`Tokens`), `BigInt` to `u64` refusing what does not fit, and macros that expand to `#[napi]` exports in the consumer |
| `npm/binding-runtime` | `@extendedresearch/binding-runtime`, the TypeScript half of a Node binding: `errors` (error classes from the token protocol), `harden` (wraps a native class's methods and getters), `enums` (frozen enumeration contracts). `src/*.ts` compiles to `dist/` with `npm run build`; `test/consumer/` is the project the release script installs the tarball into |
| `dotnet/Interop` | The sources of `ExtendedResearch.Interop`: internal C# a package compiles into its own assembly for `netstandard2.1` and `net8.0`. `AbiHandle` (a `SafeHandle` over `_destroy`), `AbiErrors` and the exception types, `AbiBuffer` (measure-then-copy), `AbiLibrary` (a `DllImport` resolver on net8.0), `AbiEnumeration`, `AbiCodes` |
| `dotnet/Interop.Package` | Packs `dotnet/Interop/*.cs` as the NuGet source package `ExtendedResearch.Interop`: content files under `contentFiles/cs/any/` with the build action `Compile`, no assembly, marked `developmentDependency` |
| `dotnet/Interop.PackageTest` | A consumer that restores the `.nupkg` from a local folder source and compiles it for both targets, C# 8, warnings as errors |
| `dotnet/Interop.Build` | Compiles `dotnet/Interop` for both targets, C# 8, warnings as errors |
| `dotnet/Interop.Tests` | xUnit on net8.0, P/Invoking `crates/abi-testlib` |
| `crates/abi-testlib` | `publish = false`. A cdylib shaped like a package — a safe core and a C adapter over it — for the .NET tests and for Miri. `tests/interop_codes.rs` compares `dotnet/Interop/AbiCodes.cs` with `extendedresearch_abi::codes` |
| `crates/napi-testaddon` | `publish = false`. A Node addon consuming `extendedresearch-napi`'s macros; `test/addon.test.mjs` loads it and runs `npm/binding-runtime/src/` against it |
| `python/conformance` | `extendedresearch-conformance`, a pip-installable development tool: runs every language binding's driver over one cases file and compares each with the C header and with every other binding. Configured per repository by `conformance.toml`; standard library only |
| `scripts/build-release-assets.sh` | Builds every release asset into one directory, checks each, and writes `SHA256SUMS`. `scripts/release-checks.py` holds the checks: versions, licence copies, and each archive's contents |
| `.github/actions/prove-tests-ran` | A composite action that fails a job when a `cargo test` log shows no result line, zero passed tests, or ignored tests. The check is `prove-tests-ran.sh`; `tests/` holds real `cargo test` logs it must refuse or accept, and `tests/run.sh` runs them |
| `docs/conventions` | The rules a consuming package cites rather than copying from a sibling: the C library artefact's name, the toolchain pins, the workspace layout, the CI job split, the rustfmt edition, and the error token grammar. Each states its rule and the failure it prevents |
| `Cargo.toml` | The workspace. `[workspace.lints]` is the table a consuming package repeats, plus `undocumented_unsafe_blocks` |
| `.github/workflows/ci.yml` | Format, lints, docs and every suite on Linux, Windows and macOS; the Rust tests on the MSRV; Miri over the pointer-handling tests; every release asset built and checked (`release-assets`) |
| `.github/workflows/release.yml` | On a `v*.*.*` tag: the release script with the tag, then a GitHub Release for the tag with the assets attached, using only the workflow's `GITHUB_TOKEN` |

## How the packages reach it

Every foundation version is a tag, `v<major>.<minor>.<patch>`, and a GitHub
Release for that tag carrying four assets and `SHA256SUMS`. A package takes the
Rust crates by the commit the tag points to and everything else from the
release, and moves to a new version by changing all of them together.

**Rust: as a git dependency on this public repository, pinned to the commit the
release tag points to:**

```toml
extendedresearch-abi = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

```bash
git rev-parse 'v0.1.0^{commit}'    # the commit a tag points to
```

The repository is public, so resolving it needs no credential and no secret in
any package's CI. A `rev` is as fixed as a registry version: the lockfile
records the commit, and only an edit to the manifest moves it. A branch or a
tag would move under an unchanged manifest at the next `cargo update`, so
neither is used — the tag names the commit, and the manifest holds the commit.

**This is the one git dependency a core package's Rust build takes.** A package
that checks that nothing in its `Cargo.lock` resolves over git relaxes that
check to "nothing resolves over git except this repository, pinned to a
commit":

```bash
grep 'source = "git' Cargo.lock \
  | grep -cvE '^source = "git\+https://github\.com/extendedresearch/foundation\?rev=[0-9a-f]{40}#[0-9a-f]{40}"$'
# 0
```

**TypeScript: the npm tarball, from the release's download URL:**

```bash
npm install https://github.com/extendedresearch/foundation/releases/download/v0.1.0/extendedresearch-binding-runtime-0.1.0.tgz
```

npm installs a tarball given as an `http://` or `https://` URL
([`npm install <tarball url>`](https://docs.npmjs.com/cli/v10/commands/npm-install)).
The tarball carries compiled `dist/*.js` and `dist/*.d.ts`, not TypeScript:
Node refuses to strip types from `.ts` files under `node_modules`
([Node.js: type stripping in dependencies](https://nodejs.org/api/typescript.html#type-stripping-in-dependencies)).

**C#: the `.nupkg`, through a local folder source.** A package downloads
`ExtendedResearch.Interop.<version>.nupkg` from the release into a folder in its
own repository and names that folder in its `nuget.config`, with a package
source mapping that takes `ExtendedResearch.Interop` from that folder alone.
NuGet reads a folder of packages as a source
([local feeds](https://learn.microsoft.com/en-us/nuget/hosting-packages/local-feeds)).
`dotnet/Interop.Package/README.md` has the `nuget.config` and the
`PackageReference`. The package holds the `.cs` files as `Compile` content
files and no assembly, so each package still compiles them into its own
assembly as `internal` types.

**The conformance runner: the wheel, from the release's download URL:**

```bash
pip install https://github.com/extendedresearch/foundation/releases/download/v0.1.0/extendedresearch_conformance-0.1.0-py3-none-any.whl
```

pip installs from a local or remote archive
([`pip install`](https://pip.pypa.io/en/stable/cli/pip_install/)). The release
also carries the sdist.

**The CI action: by commit**, as
`extendedresearch/foundation/.github/actions/prove-tests-ran@<commit>`.

**Two packages pinning different commits put two copies of each crate in one
build**, and Cargo treats them as different crates. Nothing from here crosses a
package boundary today — codes are plain `i32`, the helpers are functions, and
the exception and export macros expand in each consumer — so the copies
coexist. A package that exposes one of these crates' types in its own public
API ties every package built beside it to the same commit.

**No package registry yet.** The crates are not on crates.io, the tarball is
not on the npm registry, the `.nupkg` is not on nuget.org, and the wheel is not
on PyPI: a release is a GitHub Release and nothing else. A crate with a git
dependency cannot itself be published to crates.io, and no core package is
today.

**A crate lands here when two packages need the same runtime code.** A crate
with one consumer belongs in that consumer's repository. `abi-testlib` and
`napi-testaddon` are the exception: they are test fixtures for the crates here,
and `publish = false`.

## What is built, and what is not

Built, with the check that shows it beside each:

- `crates/abi`, `crates/pyo3`, `crates/napi`, `crates/abi-testlib`:
  `cargo test --workspace`, on stable and on 1.85.
- The napi macros registering in a real consumer, and the TypeScript sources
  running against it: `node --test crates/napi-testaddon/test/addon.test.mjs`.
- `ExtendedResearch.Interop` compiling for `netstandard2.1` and `net8.0`
  (`dotnet build dotnet/Interop.Build`) and running against a Rust library on
  net8.0 (`dotnet test dotnet/Interop.Tests`).
- The conformance runner: `python -m unittest discover -s python/conformance/tests`.
- The zero-tests action refusing what it should:
  `bash .github/actions/prove-tests-ran/tests/run.sh` runs its script against
  real `cargo test` logs on each CI platform, and job `check` on Linux feeds
  `action.yml` itself a log with zero passed tests and fails unless it refuses.
  Those logs are captured, so they cannot notice cargo changing its summary
  line; `bash .github/actions/prove-tests-ran/tests/live.sh`, in the same job on
  Linux, runs `cargo test` on a crate with two passing tests and one ignored and
  fails unless the script reads exactly that from the runner's cargo.
- **Every release asset, on every pull request**: the `release-assets` job runs
  `bash scripts/build-release-assets.sh <empty directory>`. It fails unless
  every version agrees, and every copy of `LICENSE` and `NOTICE` equals the
  root's. It builds the npm tarball, the `.nupkg`, the wheel and the sdist, and
  fails on an archive holding a file it should not or missing one it should. It
  installs the tarball into a scratch project and imports each export from
  `node_modules`, type-checks a TypeScript consumer against the installed
  declarations, and builds `dotnet/Interop.PackageTest` from the `.nupkg`
  through a local folder source. Then it writes `SHA256SUMS`.
- **Publishing as GitHub Release assets**: `release.yml`, on a push of a
  `v*.*.*` tag, runs the same script with the tag — which also fails unless
  every version equals it — and creates the release with
  `gh release create "$GITHUB_REF_NAME" <assets> --verify-tag`. It reads no
  secret beyond the workflow's `GITHUB_TOKEN`.

Not built, or not decided:

- **`release.yml` has not run.** No tag has been pushed; the first release is
  its first run on GitHub. The build inside it is the one `release-assets` runs
  on every pull request, and the `gh release create` step is the part no pull
  request exercises.
- **A release tag is never moved.** A package pins its `rev` to the commit a
  tag points to and downloads the assets from that tag's release; a tag moved
  to another commit leaves one version naming two trees, and assets built from
  one beside a `rev` that names the other. A mistake is corrected with the next
  version. Nothing in this repository enforces it. `gh release create --help`
  describes release immutability, a repository setting under which a published
  release's tag and assets cannot be modified or deleted; whether this
  repository enables it is not recorded here.
- **0.1.0 is the first version, and nothing is frozen.** A later 0.x release can
  change any name, code or signature.
- **No registry is decided.** Publishing to crates.io, the npm registry,
  nuget.org or PyPI is a separate decision from this release process.
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
- **The TypeScript and the C# are edited here, and a package takes them only as
  a release asset.** A copy committed into a package is a file no release check
  reads and no version names. Each module in `npm/binding-runtime/src/` imports
  nothing, so a binding can take one without the others, and
  `scripts/release-checks.py assets` fails on a compiled module that imports
  anything.
- **Every version in the tree is the release's.** Each crate and the `version`
  on each internal path dependency, `python/conformance/pyproject.toml` and
  `__version__`, `npm/binding-runtime/package.json` and its lockfile,
  `dotnet/Interop.Package`'s `<Version>`, and the exact version
  `dotnet/Interop.PackageTest` restores. `python scripts/release-checks.py tree`
  lists every one and fails when any differs; with `--tag` it also fails unless
  each equals the tag.
- **`*out_len` never counts a string's terminator, and `capacity` always must.**
- **The MSRV is 1.85, because the workspace is on edition 2024 and 1.85 is the
  release that stabilised it.** Nothing here asks for a later compiler, so the
  floor is the edition's and not a dependency's: pyo3 0.29 declares 1.83 and
  napi 3 declares 1.82, both below it. A consumer on a newer toolchain is
  unaffected; a consumer on an older one could not compile an edition-2024
  crate at all. `docs/conventions/toolchain-pins.md` carries the pin and the
  `msrv` job in `ci.yml` is what holds it.
- **Each published crate, `python/conformance`, `npm/binding-runtime` and
  `dotnet/Interop.Package` carry their own copy of `LICENSE` and `NOTICE`.** A
  package is built from its own directory, so the root files do not reach a
  user who downloads it, and Apache-2.0 requires `NOTICE` to travel with a
  redistribution. Each crate's `tests/license.rs` and the conformance self-test
  fail when their copy drifts from the root, and
  `python scripts/release-checks.py tree` fails when any tracked copy does, or
  when one of those directories has none.

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

bash .github/actions/prove-tests-ran/tests/run.sh         # the zero-tests check refuses what it should
bash .github/actions/prove-tests-ran/tests/live.sh        # ...and reads this cargo's summary line

dotnet build dotnet/Interop.Build
cargo build -p extendedresearch-abi-testlib
dotnet test dotnet/Interop.Tests

python scripts/release-checks.py tree                     # versions and licence copies
bash scripts/build-release-assets.sh "$(mktemp -d)/assets" # needs `pip install build`

python docs/docs.tools/check-tier.py --root . --repo foundation
```

`npm run build` in `npm/binding-runtime` type-checks and compiles `src/` with
the exact `typescript` its lockfile pins; the release script runs it.
