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
//! | **Ownership** | The library allocates and the library frees. Every `_create` has a `_destroy`; every `_destroy` is null-tolerant, non-blocking, and callable from any thread, because .NET runs them on a finalizer thread in an order nothing controls |
//! | **Errors** | `int32_t` return codes, values through out-parameters, zero success, negative failure. No `errno`-style global. [`codes`] splits the negative range between the boundary and the library |
//! | **Variable-length data** | Copied into a buffer the caller allocated. A null destination measures. `*out_len` never counts a string's terminator and `capacity` always must. See [`buffer`] |
//! | **Strings** | UTF-8, null-terminated. In: borrowed for the call, and not-UTF-8 is [`codes::ERR_UTF8`] rather than a lossy conversion. Out: the buffer shape above |
//! | **Panics** | Caught at the boundary by [`guard::guard`] and answered as [`codes::ERR_PANIC`], because an unwind into C aborts a process that belongs to somebody else |
//! | **Enumerations** | Named `int32_t` constants, never a C `enum`, for codes and parameters alike. The width of a C enum is implementation-defined |
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

#[allow(unsafe_code)]
pub mod borrow;
pub mod buffer;
pub mod codes;
pub mod conformance;
pub mod guard;
