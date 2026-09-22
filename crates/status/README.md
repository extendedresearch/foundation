# extendedresearch-status

The codes an `int32_t` C boundary answers, the trait a library's error type
implements behind them, and the guard that stops a panic from crossing.

## What it is

A Rust library with a C ABI returns an `int32_t` from every fallible function,
and four pieces of code have to agree on what that integer says: the library's
core, its C adapter, each language binding, and the header a C caller reads.
Nothing in the type system connects those four. This crate is the one place the
agreement is written down.

The split it draws is the whole point. `0` is success. `-1` down to `-15` are
failures any boundary has — a null handle, a buffer too small, text that is not
UTF-8, a caught panic — so a binding translates that range once and reuses the
translation against every library that follows the convention. `-16` and below
are yours: a refusal, a protocol violation, a timeout mean nothing without your
domain, and a shared crate that tried to name them would be guessing. Without
the split, every library numbers from `-1` and a binding has to know which
library it is talking to before it can read a code.

`AbiError` is what your error type implements so that your Python, Node and C
layers read the same two facts from it — the code, and the name of the constant
that code is. `status` turns a `Result` into the code your C adapter returns;
`check` turns a returned code back into a `Result`. `guard` turns a panic into
`ERR_PANIC`, because unwinding out of an `extern "C"` function is undefined and
the process belongs to somebody else.

No dependencies, no `unsafe`, and no exported symbol — so linking this changes
nothing a C caller sees.

## Install

```toml
[dependencies]
extendedresearch-status = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it.

Without the panic guard, which is the only part that needs `std`:

```toml
extendedresearch-status = { git = "...", rev = "...", default-features = false }
```

## Use

Your error type answers a code and that code's name:

```rust
use std::fmt;

use extendedresearch_status::codes::{self, AbiError, DOMAIN_FLOOR};

// Your own codes start at the floor and count down. Everything above it is the
// boundary's, and stays the boundary's.
pub const STORE_ERR_FULL: i32 = DOMAIN_FLOOR;

#[derive(Debug)]
pub enum StoreError {
    NotUtf8,
    Full,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str("the key was not UTF-8"),
            Self::Full => f.write_str("the store is full"),
        }
    }
}

impl AbiError for StoreError {
    fn code(&self) -> i32 {
        match self {
            Self::NotUtf8 => codes::ERR_UTF8,
            Self::Full => STORE_ERR_FULL,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            // Unprefixed for a boundary code: every library shares that name.
            Self::NotUtf8 => "ERR_UTF8",
            // Your own name, spelled as your header spells it.
            Self::Full => "STORE_ERR_FULL",
        }
    }
}
```

An exported function answers a status; Rust calling it reads the status back:

```rust
use extendedresearch_status::{codes, guard};
# use std::fmt;
# use extendedresearch_status::codes::AbiError;
# struct Full;
# impl fmt::Display for Full {
#     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("full") }
# }
# impl AbiError for Full {
#     fn code(&self) -> i32 { codes::DOMAIN_FLOOR }
#     fn name(&self) -> &'static str { "STORE_ERR_FULL" }
# }

fn len() -> Result<u64, Full> {
    Ok(3)
}

// The body of `int32_t store_len(const store_t *store, uint64_t *out_len);`,
// with the pointer work left out. `guard` is outermost, so a panic anywhere
// inside answers ERR_PANIC instead of unwinding into C.
let mut out_len = 0u64;
let rc = guard::guard(|| {
    codes::status(len(), |value| {
        out_len = value;
        codes::OK
    })
});

// And what Rust on the calling side does with the answer.
assert_eq!(codes::check(rc), Ok(()));
assert_eq!(out_len, 3);
assert_eq!(codes::check(codes::ERR_RANGE), Err(codes::ERR_RANGE));
```

One failure reads the same in every language because every layer reports the
header's spelling of the name, which `token` produces:

```rust
use extendedresearch_status::codes::token;

assert_eq!(token("STORE", "ERR_UTF8"), "STORE_ERR_UTF8");
// A name that already carries the prefix is unchanged, so applying the step
// twice changes nothing.
assert_eq!(token("STORE", "STORE_ERR_FULL"), "STORE_ERR_FULL");
```

## Guarantees

- **Zero is the only non-negative answer.** `if (rc < 0)` is the whole of a
  caller's check, and a code it has never heard of still reads as a failure —
  which is what lets you add a code without every binding knowing it first.
- **Nothing is ever added between `-1` and `DOMAIN_FLOOR`'s `-16`.** The gap
  below `ERR_STATE` is headroom for codes this crate has not needed yet, so a
  binding's translation of the boundary range stays correct as this crate grows,
  and your `-16` never collides with one added here.
- **`status` never answers success for an error.** An `AbiError` whose `code()`
  is zero or positive answers `ERR_STATE`, so a defect in your error type
  cannot read as success to `if (rc < 0)`.
- **`check` is the inverse of `status`** for every code `status` can answer,
  including domain codes and codes this crate has no name for. The unit tests
  round-trip both directions over the whole set.
- **`token` is idempotent**, which is what lets any layer apply it without
  first asking whether an earlier one already did.
- **No dependencies.** `cargo tree -p extendedresearch-status` prints the crate
  and nothing under it.
- **No `unsafe`.** The workspace denies `unsafe_code` and this crate allows it
  nowhere.
- **`#![no_std]` with `--no-default-features`**, so the same codes are usable
  from instrument firmware as from a desktop binding.

## Limits

- **`guard` needs `std`**, because containing a panic is
  `std::panic::catch_unwind`. It is behind the default-on `std` feature;
  turning that off leaves you everything else.
- **A C artefact built with `panic = "abort"` catches nothing.** The process is
  gone before `guard` runs, so `ERR_PANIC` means something only under the
  default `panic = "unwind"`.
- **A caught panic buys an orderly report, not a working library.** The state
  the panic left behind is not recoverable, and `ERR_PANIC` is the signal to
  stop using the object rather than to retry.
- **`token` allocates a `String`**, so the crate links `alloc` whether or not
  `std` is on. Nothing else here allocates.
- **This crate cannot name your codes.** `name` and `describe` answer `None`
  for anything at or below `DOMAIN_FLOOR`, and for `OK`. Your header and your
  own table are where those live.
- **Nothing here touches a pointer or exports a symbol.** The mechanics of a C
  boundary — reading a caller's pointer, the measure-then-copy buffer shape,
  the table behind an enumeration's `_count` and `_at`, and a conformance kit
  that drives your exports the way a binding does — are
  `extendedresearch-abi`, which depends on this crate.

## Versioning

Pre-1.0. A minor release may change any name, code or signature. The boundary
code values and the `DOMAIN_FLOOR` split are the part treated as a wire
contract, because a C header, a C# constant and a TypeScript token elsewhere
all repeat these numbers; they are not renumbered or reused. The minimum
supported Rust version is 1.85, which is the release that stabilised edition
2024.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
