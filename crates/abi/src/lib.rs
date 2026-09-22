//! The conventions every C ABI in this ecosystem obeys, as code.
//!
//! A library that exposes a C boundary — a runtime, a container reader, a
//! validator — writes its own `extern "C"` functions. What it does not write is
//! the part every one of those functions shares: how a caller's pointer is read,
//! how a variable-length answer is copied out, which codes a failure answers,
//! and what happens to a panic. That part is here, once, so a binding author
//! learns it once and it cannot drift between libraries.
//!
//! # The conventions
//!
//! Each is stated in full in the module that implements it. The summary is what
//! a binding author needs before reading any one library's header.
//!
//! | | |
//! |---|---|
//! | **Handles** | Opaque pointers. The header declares a tag and never a field, so nothing above can compute a size or an offset. [`borrow::into_handle`] hands one out; [`borrow::reclaim`] takes it back |
//! | **Ownership** | The library allocates and the library frees. Every `_create` has a `_destroy`; every `_destroy` is null-tolerant, non-blocking, and callable from any thread, because .NET runs them on a finalizer thread in an order nothing controls. [`guard::contain`] keeps a panic inside one from crossing |
//! | **Errors** | `int32_t` return codes, values through out-parameters, zero success, negative failure. No `errno`-style global. [`codes`] splits the negative range between the boundary and the library |
//! | **Variable-length data** | Copied into a buffer the caller allocated. A null destination measures. `*out_len` never counts a string's terminator and `capacity` always must. See [`buffer`] |
//! | **Strings** | UTF-8, null-terminated. In: borrowed for the call, and not-UTF-8 is [`codes::ERR_UTF8`] rather than a lossy conversion. Out: the buffer shape above |
//! | **Panics** | Caught at the boundary by [`guard::guard`] and answered as [`codes::ERR_PANIC`], because an unwind into C aborts a process that belongs to somebody else |
//! | **Enumerations** | Named `int32_t` constants, never a C `enum`, for codes and parameters alike, because the width of a C enum is implementation-defined. Beside each set, `_count`, `_at` and `_name` functions, so a binding loops over the library's values rather than transcribing them. See [`enumeration`] |
//! | **Threading** | Distinct handles from distinct threads. One handle from one thread at a time unless the library documents an exception. A `_destroy` races nothing |
//! | **Version** | Each library exports a `uint32_t` ABI version, compared for **equality** against the value in the header the binding was built with. The conventions are an ownership contract, not a feature set a later version is a superset of |
//!
//! # What a library still writes
//!
//! Its own `extern "C"` functions, its own handle types, its own codes at and
//! below [`codes::DOMAIN_FLOOR`], its own version constant, and its own header.
//! This crate exports no symbol, so linking it changes nothing a C caller sees.
//!
//! # Where `unsafe` is
//!
//! [`borrow`] is the only module that writes `unsafe`, and the compiler holds
//! that rather than a reviewer: the workspace manifest denies `unsafe_code` and
//! the one `allow` sits on `mod borrow` below. `tests/discipline.rs` fails if
//! a second `allow` appears.
//!
//! A library consuming this crate still writes `unsafe` at each call into
//! [`borrow`], because each call is where a pointer from C is asserted to be what
//! the caller said. What changes is that every such block wraps one call with a
//! stated contract, rather than an open-coded dereference.
//!
//! # Checking a library against the conventions
//!
//! [`conformance`] drives a library's exported functions through raw pointers,
//! the way a binding does, and panics with the rule that was broken. A library
//! calls it from its own tests.
//!
//! # Where each binding meets a package
//!
//! A package's core is safe Rust whose error type implements
//! [`codes::AbiError`]. Its Python and Node bindings call that core directly
//! and convert the error with `extendedresearch-pyo3` and
//! `extendedresearch-napi`; no pointer and no handle crosses between them. A
//! thin C adapter per package serves .NET and C callers: it is the only code
//! that calls [`borrow`], one documented call per `unsafe` block, and
//! [`codes::status`] turns each core `Result` into the code it answers.
//!
//! [`binding`] is the other side of that C adapter as Rust sees it: what the
//! package's C-ABI tests and conformance drivers do to read an answer and a
//! code. It needs no `unsafe`.

pub mod binding;
#[allow(unsafe_code)]
pub mod borrow;
pub mod buffer;
pub mod conformance;
pub mod enumeration;

/// The codes and the [`AbiError`](codes::AbiError) trait, re-exported from
/// `extendedresearch-status`.
///
/// They moved to a crate with no dependencies and no `unsafe`, so a package's
/// safe core can implement [`codes::AbiError`] without depending on the crate
/// that dereferences a caller's pointer. This path keeps working;
/// `extendedresearch_status::codes` is the same module and is where new code
/// should reach for it.
pub use extendedresearch_status::codes;

/// The panic guard, re-exported from `extendedresearch-status`.
///
/// It moved with [`codes`], which is all it needs. There it sits behind that
/// crate's default-on `std` feature, which this crate enables, so
/// [`guard::guard`] and [`guard::contain`] are always available here.
pub use extendedresearch_status::guard;

// The README's example is the crates.io page's first code a reader sees; this
// runs it as a doctest so it cannot drift from the crate.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExample;
