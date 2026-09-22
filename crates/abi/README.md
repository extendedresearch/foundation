# extendedresearch-abi

The conventions a Rust library's C ABI follows, so that its Python,
JavaScript and .NET bindings sit on one boundary and read it the same way.

## What it is

A library that exposes a C boundary writes its own `extern "C"` functions.
This crate holds the part those functions share: how a caller's pointer is
read, how a variable-length answer is copied out, which codes a failure
answers, what happens to a panic, how an enumeration is exposed, and how a
binding written in Rust reads all of that back. It exports no symbol, so
depending on it changes nothing a C caller sees.

| Module | For | What it does |
|---|---|---|
| `codes` | both sides | `int32_t` codes: zero is success, `-1` to `-15` are boundary failures any library has, and `-16` and below are the library's own. `AbiError` is the trait a package's error type implements so every binding reads its code and name; `status` turns a core `Result` into the code a C adapter answers |
| `borrow` | the library | Every read of a caller's pointer: handles, out-parameters, strings, arrays, and the copy-out calls. The only module that writes `unsafe` |
| `buffer` | the library | Measure-then-copy: a null destination measures, a short buffer answers `ERR_RANGE` with the size it needed |
| `guard` | the library | A panic answers `ERR_PANIC` instead of unwinding into C, including inside a `_destroy` |
| `enumeration` | the library | The table behind `_count`, `_at` and `_name`, so a binding loops over values instead of transcribing them |
| `binding` | C-ABI tests and conformance drivers | Reading a measure-then-copy answer from Rust, re-measuring if it grew between calls, and naming a failure code. The Python and Node bindings call the package's Rust core instead |
| `conformance` | the library's tests | Drives the library's exports through raw pointers, the way a binding does, and panics naming the rule a function broke. Checks the library's table of domain codes, and its error values against that table |

No dependencies. The crate documentation in `src/lib.rs` states each rule with
its reason, and is the reference for them.

## Install

```toml
[dependencies]
extendedresearch-abi = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it. The repository is public, so resolving it
needs no credential. Nothing is published to crates.io.

## Use

An exported function, and a binding reading it:

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

That example is compiled and run as a doctest — `src/lib.rs` includes this file
under `#[cfg(doctest)]` — so it cannot drift from the crate.

## Guarantees

- **Handles are opaque pointers.** The library allocates and the library frees;
  every `_destroy` accepts null, does not block, and can run on any thread,
  because .NET runs them on a finalizer thread in an order nothing controls.
- **Every fallible function answers `int32_t`** and delivers its value through
  an out-parameter. Zero is success and every negative value is a failure,
  including one a caller has never heard of — which is what lets a library add
  a code without every binding knowing it first. There is no `errno`-style
  global.
- **The negative range is split, once, for every library.** `-1` to `-15` are
  the boundary's (`ERR_NULL`, `ERR_RANGE`, `ERR_UTF8`, `ERR_PANIC`,
  `ERR_STATE`, and headroom); `DOMAIN_FLOOR` is `-16` and a library numbers its
  own from there down. A binding translates the boundary codes once and reuses
  that translation against every library.
- **`status` never lets an error read as success.** An `AbiError` whose `code`
  is not negative answers `ERR_STATE` rather than the value it gave.
- **Enumerated values and codes are named `int32_t` constants, never a C
  `enum`.** The width of a C enum is implementation-defined, and a binding that
  guessed 32 bits against a library that compiled something else would misread
  every code.
- **Text is UTF-8 and null-terminated. `*out_len` never counts the terminator;
  `capacity` always must.** Written once here because it is two rules that
  differ by one byte, and getting it wrong yields a truncated last character
  rather than an error.
- **No pointer into the library's memory is ever returned**, so *is this
  borrowed or owned* cannot be asked and there is no `_string_free`.
- **A panic answers `ERR_PANIC` instead of crossing into C**, including inside
  a `_destroy`, where `guard::contain` swallows it because a `_destroy` has no
  channel to report through.
- **Each library exports a `uint32_t` ABI version, compared for equality** with
  the header the binding was built against. The conventions are an ownership
  contract, not a feature set a later version is a superset of.
- **`unsafe` is confined to `borrow` by the compiler, not by review.** The
  workspace denies `unsafe_code`, one `allow` sits on `mod borrow`, and
  `tests/discipline.rs` fails if a second one appears:

  ```bash
  grep -rn 'allow(unsafe_code)' crates/abi/src
  # crates/abi/src/lib.rs:66:#[allow(unsafe_code)]
  ```
- **Every function that takes a raw pointer is `unsafe fn`**, because a safe
  public function that dereferences one is unsound: safe code can pass any
  address.
- **A caller's buffers and out-parameters are `MaybeUninit`.** A C caller
  passes uninitialised memory, and a `&mut [u8]` over it is undefined behaviour
  whether or not anything reads it. CI runs Miri over the pointer-handling
  tests; an ordinary test run cannot see this.
- **It exports no symbol.** Nothing in `src/` carries `#[unsafe(no_mangle)]`,
  so linking this crate changes nothing a C caller sees:

  ```bash
  grep -rn 'no_mangle' crates/abi/src   # no output
  ```

## Limits

- **A handle that was already destroyed cannot be detected.** A pointer to
  freed memory is not distinguishable from a live one at any cost this boundary
  can pay, and a magic number would catch the easy half while giving false
  confidence about the rest. It is the one error left to the binding, which is
  why every binding owns its handles in whatever its language does
  automatically — `SafeHandle` in .NET, reference counting in Python.
- **`panic = "abort"` catches nothing.** Under that profile a panic aborts
  before `guard` can see it, and `ERR_PANIC` becomes a code no build answers. A
  library that wants it to mean anything builds its C artefact with the default
  `panic = "unwind"`.
- **`ERR_PANIC` buys an orderly report, not a working library.** The library's
  state after a caught panic is not recoverable, and each library says so in
  its own documentation.
- **This crate writes no export, no header and no binding.** A library still
  writes its own `extern "C"` functions, its own handle types, its own codes at
  and below `DOMAIN_FLOOR`, its own version constant, and its own header.
  Nothing here generates any of them.
- **Threading is stated, not enforced.** Distinct handles from distinct
  threads, and one handle from one thread at a time unless the library
  documents an exception, is the caller's promise. `borrow::handle_mut` asserts
  it in a `# Safety` section; no code checks it.
- **`binding::text` and `binding::bytes` give up after `ATTEMPTS`.** An answer
  that is larger on each of the four copies than when it was last measured
  answers `ReadError::Unstable` rather than looping.
- **`conformance::errors` checks the values it is handed.** Pass one value of
  every variant your error type has; nothing verifies that you did, so a
  variant left out of the call is a variant left unchecked.
- **Nothing here compares a library against its C header.** `error_codes`
  checks a declared table against the numbering rules, not against what the
  header says. Reading the header, and comparing every binding with it, is
  `extendedresearch-conformance` in `python/conformance`.
- **No enumeration of a library's statuses crosses the C ABI.** `Enumeration`
  serves a library's parameter enumerations; a binding still writes the status
  table down and checks it against the header in a test of its own.

## Versioning

This is 0.1.1. Pre-1.0: a minor release may change any name, code or signature.
The minimum supported Rust version is 1.85, which is the release that
stabilised edition 2024 — nothing here asks for a later compiler.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
