# extendedresearch-pyo3

Python exceptions and `IntEnum`s for a package's PyO3 binding, built from the
package's `AbiError` and foundation `Enumeration`s.

## What it is

A library behind a C ABI is bound once per language, and every binding has to
answer the same question: this call failed with code `-17` — what does a caller
of *this* language catch? For Python, this crate is that answer, written once.

Your binding calls the package's safe Rust core directly: it holds no raw
handle and writes no `unsafe`. What it takes from here is the translation of
what the core answered into Python.

| | |
|---|---|
| `exceptions!` | Creates the package's exception hierarchy **in your crate**: a base error, a `PanicError` beneath it, any domain exceptions, and the table from domain code to exception |
| `error`, `code_error`, `Raise` | Turn an `AbiError` into the `PyErr` your family names for it |
| `int_enum` | Builds a Python `IntEnum` from a foundation `Enumeration`, under the short-name rule below |

It depends on `extendedresearch-abi` and on pyo3, and on nothing else.

## Install

```toml
[dependencies]
extendedresearch-pyo3 = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
pyo3 = { version = "0.29", features = ["abi3-py311", "extension-module"] }
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it. The repository is public, so resolving it
needs no credential. Nothing is published to crates.io.

Name pyo3 in your own manifest, at the `0.29` series with `abi3-py311`, for
your `#[pymodule]`. `extension-module` belongs to your `cdylib` and is not
enabled here: enabling it in this crate would stop its own tests from linking
an interpreter.

## Use

```rust
use extendedresearch_pyo3::{Raise, exceptions, int_enum};
use pyo3::prelude::*;

exceptions! {
    /// Every exception `_thing` raises.
    pub family ThingExceptions in _thing;
    prefix "THING";
    base ThingError: "Anything thing refused.";
    panic PanicError: "A panic was caught; thing's state is unknown.";
    exception RefusedError(ThingError): "Thing refused the request.";
    domain thing::THING_ERR_REFUSED => RefusedError;
}

#[pyfunction]
fn open(path: &str) -> PyResult<u64> {
    thing::open(path).raise::<ThingExceptions>()
}

#[pymodule]
fn _thing(module: &Bound<'_, PyModule>) -> PyResult<()> {
    ThingExceptions::register(module)?;
    module.add("Origin", int_enum(module.py(), "Origin", "thing", "…", &thing::ORIGINS)?)?;
    module.add_function(wrap_pyfunction!(open, module)?)?;
    Ok(())
}
```

`prefix` is what your header's constants begin with. A boundary failure's
message reads `THING_ERR_RANGE: …`, as it does from your Node and .NET
bindings. The crate documentation lists which code raises which exception and
states the short-name rule in full.

## Guarantees

- **Each extension module owns exactly one copy of its exception classes.**
  `create_exception!` puts a class in a `static` in the crate that expands it,
  so a class created in a shared crate would be one static per module linking
  it, and two packages loaded into one interpreter would each raise their own
  class of the same name — `except` on one missing the other. `exceptions!`
  expands in your binding, which is what prevents that.
- **Every message leads with the constant's name as your header spells it.**
  A boundary name gains your `prefix` through
  `extendedresearch_abi::codes::token` (`ERR_UTF8` becomes `THING_ERR_UTF8`); a
  domain name already carries it and is left alone. The Node and .NET layers
  apply the same step, so one failure reads the same in all three.
- **`prefix` is required**, because the unprefixed name is the one no header
  declares.
- **`PanicError` subclasses the base error**, so a caller catching "anything
  this package refused" still catches a panic.
- **A pyo3 mismatch is a resolution error, not a second copy.** `pyo3-ffi`
  declares `links = "python"` and Cargo permits one package per `links` value
  in a build, so a consumer on another minor series fails to resolve rather
  than building two pyo3s that disagree about every type. `cargo tree -i
  pyo3-ffi` in your crate lists exactly one version.
- **The short-name prefix is derived from the member names, not written down.**
  `ORIGIN_UNSPECIFIED` and `ORIGIN_RAW` share `ORIGIN_`, which is stripped; the
  contract's full name stays bound as an alias, so `Origin.RAW` and
  `Origin.ORIGIN_RAW` are the same member. Deriving it matters because a
  family's C constant prefix and its contract prefix can differ, and a prefix
  written into the binding would then be wrong for one of them.
- **The conversions are tested against a real interpreter.** `tests/python.rs`
  holds 12 tests that run under an embedded CPython — `isinstance`,
  `issubclass` and `IntEnum` lookups are Python's own answers, not assertions
  about what pyo3 should have built:

  ```bash
  cargo test -p extendedresearch-pyo3
  ```

## Limits

- **A `PyErr` carries no code attribute.** The code travels as the first token
  of the message and nothing else; a caller that wants to branch on the exact
  constant either catches a type you mapped or reads the token out of
  `str(error)`. The Node side puts the token on `error.code` as a field, and
  this side has no equivalent.
- **Only three boundary codes get a distinct type.** `ERR_UTF8` raises
  `ValueError`, `ERR_RANGE` raises `IndexError`, `ERR_PANIC` raises your
  `PanicError`. `ERR_NULL`, `ERR_STATE`, any other boundary code, and any
  domain code your family does not map all raise your base error, so they are
  not distinguishable by `except` alone.
- **Your pyo3 requirement is not yours to choose.** It must resolve to the
  series this crate resolves to — `0.29` today, the pin
  `docs/conventions/toolchain-pins.md` states. Moving this crate's pyo3 moves
  every consumer's on the same commit.
- **`abi3-py311` sets the floor at CPython 3.11.** An extension built against
  this crate does not load on an earlier interpreter.
- **`int_enum` goes one way.** It builds a Python class from a static table at
  module init; nothing here reads a Python value back into a Rust enum, and
  nothing re-reads the table if it changes after the module loaded.
- **The short-name rule has an escape hatch that is not uniform.** A short name
  that would be empty, or would start with a digit (`RATE_50HZ` becoming
  `50HZ`, which is not an identifier), keeps the contract's full spelling. So
  one family can hold both short and full names.
- **`int_enum` refuses a table whose short names collide**, raising
  `ValueError` at module init rather than failing at compile time. The refusal
  is `member_names`', not `IntEnum`'s: the members are handed over as a mapping,
  so a repeated key overwrites the earlier entry and the class would come out
  one member short with nothing raised. `@extendedresearch/binding-runtime`
  refuses the same table, and
  `vectors/0001-enumeration-short-names-strip-one-shared-prefix.json` is what
  holds the two to one answer.
- **This crate is for a binding that calls a safe Rust core.** It holds no
  handle type, no pointer reading and no `unsafe`, so it does nothing for a
  Python binding that reaches the library through its C ABI with `ctypes` or
  `cffi`.

## Versioning

This is 0.1.1, consumed as a git dependency pinned by `rev`. Pre-1.0: a minor
release may change any name, code or signature. The minimum supported Rust
version is 1.85.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
