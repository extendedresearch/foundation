# foundation

Shared Rust code that ranvier, ca3 and eres depend on. One crate today:
`extendedresearch-abi`, the conventions every C ABI in the ecosystem obeys.

## What is here

| Path | What it is |
|---|---|
| `crates/abi` | `extendedresearch-abi`. Error codes, the only module that dereferences a caller's pointer, the measure-then-copy buffer shape, the panic guard, the table behind an enumeration's `_count`/`_at`/`_name`, the calling side a Rust binding uses, and a conformance kit a library runs against its own exports. It exports no `extern "C"` symbol; each library still writes its own functions, handles, header and version constant. The crate docs in `crates/abi/src/lib.rs` state every convention and are the reference for them |
| `Cargo.toml` | The workspace. `[workspace.lints]` repeats the levels ranvier, ca3 and eres set, plus `undocumented_unsafe_blocks` |
| `.github/workflows/ci.yml` | Format, lints, tests on the current stable and on the MSRV, and Miri over the pointer-handling tests |
| `.github/workflows/release.yml` | Publishes `extendedresearch-abi` to crates.io when a `v*` tag naming the manifest's version is pushed, after running all of `ci.yml`. Needs a `CARGO_REGISTRY_TOKEN` secret |

## How the packages reach it

**By a crates.io release, and no other way.** Each package refuses a
dependency that resolves through a sibling checkout, over git, or with a
credential, so a path or git dependency on this repository would break the
package's own rule. That is why no member sets `publish = false`, and why a
crate here is public once released.

**A crate lands here when two packages need the same runtime code.** A crate
with one consumer belongs in that consumer's repository.

## What is built, and what is not

Built: `crates/abi`, with its tests passing on stable, on 1.85, and under Miri.

Not built, or not decided:

- **No package depends on this yet.** ranvier has its own `crates/abi` with a
  different numbering; ca3 and eres have none.

  | Code | here | ranvier |
  |---|---|---|
  | NULL | -1 | -1 |
  | RANGE | -2 | -4 |
  | UTF8 | -3 | -2 |
  | PANIC | -4 | -8 |
  | STATE | -5 | none |
  | library-specific | -16 and below | -3, -5, -6, -9, -10, -11 |

  Which side moves is open.
- **Nothing is published yet, and nothing is versioned.** Every crate stays at
  `0.0.0` until a package can use it end to end, and no version moves before
  then. No code, name or signature is frozen until that first version is
  complete. The crates are published to crates.io and the repository is
  public.
- **Where shared tooling lives** — the lint table, the decision-record checker,
  the step that fails CI when zero tests ran — is undecided. None of it is here.
- **ca3's decision 0004 §1** allows a crates.io dependency but not "a
  dependency on another repository in this ecosystem". Read literally, that
  excludes this crate for ca3.

## Conventions that cause bugs when broken

- **`unsafe` is written in `crates/abi/src/borrow.rs` and nowhere else in
  `src/`.** The workspace denies `unsafe_code`; `lib.rs` allows it on
  `mod borrow` alone; `tests/discipline.rs` fails on a second `allow`.
- **Every function that takes a raw pointer is `unsafe fn`.** A safe public
  function that dereferences a raw pointer is unsound, because safe code can
  pass any address.
- **A caller's buffer and out-parameters are `MaybeUninit`.** A C caller passes
  uninitialised memory, and a `&mut [u8]` over it is undefined behaviour whether
  or not it is read. Miri checks this; an ordinary test run cannot.
- **Codes `-1` to `-15` are this crate's to add, and a library numbers its own
  from `-16` down.** A code added to the boundary range by a library collides
  with the next one added here.
- **`*out_len` never counts a string's terminator, and `capacity` always must.**
- **The MSRV is 1.85**, the lowest of the three packages (eres requires 1.88),
  so a crate here builds wherever any of them does.
- **Each crate carries its own copy of `LICENSE` and `NOTICE`.** A crate is
  packaged from its own directory, so the root files do not reach a user who
  downloads it, and Apache-2.0 requires `NOTICE` to travel with a
  redistribution. `tests/license.rs` fails when a copy drifts from the root.

## Checks

```bash
cargo test --workspace
cargo +1.85 test --workspace
cargo clippy --workspace --all-targets -- -D warnings
# Miri cannot read files, so the discipline test and doctests are left out.
cargo +nightly miri test -p extendedresearch-abi --lib --test borrow --test buffer --test conformance --test enumeration --test binding
```
