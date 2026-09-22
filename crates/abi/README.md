# extendedresearch-abi

The conventions a Rust library's C ABI follows, so that its Python,
JavaScript and .NET bindings sit on one boundary and read it the same way.

A library that exposes a C boundary writes its own `extern "C"` functions.
This crate holds the part those functions share: how a caller's pointer is
read, how a variable-length answer is copied out, how an enumeration is
exposed, and how a binding written in Rust reads all of that back. It exports
no symbol, so depending on it changes nothing a C caller sees.

The vocabulary of the `int32_t` itself — the codes, the `AbiError` trait and the
panic guard — is `extendedresearch-status`, a crate with no dependencies and no
`unsafe`, so a package's safe core can implement `AbiError` without depending on
the crate that dereferences a caller's pointer. This crate depends on it and
re-exports `codes` and `guard`, so both paths work.

## What you get

| Module | For | What it does |
|---|---|---|
| `codes` | both sides | Re-exported from `extendedresearch-status`. `int32_t` codes: zero is success, `-1` to `-15` are boundary failures any library has, and `-16` and below are the library's own. `AbiError` is the trait a package's error type implements so every binding reads its code and name; `status` turns a core `Result` into the code a C adapter answers, and `check` turns one back into a `Result` |
| `borrow` | the library | Every read of a caller's pointer: handles, out-parameters, strings, arrays, and the copy-out calls. The only module that writes `unsafe` |
| `buffer` | the library | Measure-then-copy: a null destination measures, a short buffer answers `ERR_RANGE` with the size it needed |
| `guard` | the library | Re-exported from `extendedresearch-status`. A panic answers `ERR_PANIC` instead of unwinding into C, including inside a `_destroy` |
| `enumeration` | the library | The table behind `_count`, `_at` and `_name`, so a binding loops over values instead of transcribing them |
| `binding` | C-ABI tests and conformance drivers | Reading a measure-then-copy answer from Rust, re-measuring if it grew between calls, and naming a failure code. The Python and Node bindings call the package's Rust core instead |
| `conformance` | the library's tests | Drives the library's exports through raw pointers, the way a binding does, and panics naming the rule a function broke. Checks the library's table of domain codes, and its error values against that table |

## An exported function, and a binding reading it

```rust
use std::ffi::c_char;
use extendedresearch_abi::{binding, borrow, codes, conformance, guard};

pub struct Stream {
    name: String,
}

/// `int32_t stream_name(const stream_t *stream, char *destination,
///                      uint64_t capacity, uint64_t *out_len);`
pub extern "C" fn stream_name(
    stream: *const Stream,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the header says `stream` is null or a live handle.
        let Some(stream) = (unsafe { borrow::handle(stream) }) else {
            return codes::ERR_NULL;
        };
        // SAFETY: the header says what `destination`, `capacity` and `out_len` may be.
        unsafe { borrow::fill_text(&stream.name, destination, capacity, out_len) }
    })
}

let stream = borrow::into_handle(Stream { name: "tracker".to_owned() });

// What a PyO3 or napi-rs binding does with it:
let name = binding::text(|d, c, l| stream_name(stream, d, c, l)).unwrap();
assert_eq!(name, "tracker");

// What the library's own tests do with it: every edge of the shape, not only
// the ordinary path.
assert_eq!(conformance::text_answer(|d, c, l| stream_name(stream, d, c, l)), "tracker");

// SAFETY: `stream` came from `into_handle` and is destroyed once.
guard::contain(|| drop(unsafe { borrow::reclaim(stream) }));
```

## The rules, in one place

- Handles are opaque pointers. The library allocates and the library frees;
  every `_destroy` accepts null and can run on any thread.
- Every fallible function answers `int32_t` and delivers its value through an
  out-parameter. There is no `errno`-style global.
- Enumerated values and codes are named `int32_t` constants, never a C `enum`.
- Text is UTF-8 and null-terminated. `*out_len` never counts the terminator;
  `capacity` always must.
- No pointer into the library's memory is ever returned.
- Each library exports a `uint32_t` ABI version, and a binding compares it for
  equality with the header it was built against.

The crate documentation states each rule with its reason.

## Using it

Depend on this repository, pinned to a commit:

```toml
[dependencies]
extendedresearch-abi = { git = "https://github.com/extendedresearch/foundation", rev = "<commit>" }
```

The repository is public, so this needs no credential. Nothing is published to
crates.io yet.

## Status

This is 0.1.1, and nothing in it is frozen: a name, a code or a signature can
change in the next version. The minimum supported Rust version is 1.85.

Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
