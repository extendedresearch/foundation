//! What an `int32_t` means when it crosses a C boundary.
//!
//! A Rust library with a C ABI answers a status code from every fallible
//! function, and four layers above it have to agree on what that integer says:
//! the library's own core, its C adapter, each language binding, and the header
//! a C caller reads. This crate is the one place that agreement is written
//! down.
//!
//! | | |
//! |---|---|
//! | [`codes`] | Zero is success, negative is failure. `-1` down to one above [`codes::DOMAIN_FLOOR`] are the boundary's; [`codes::DOMAIN_FLOOR`] and below are the library's. [`codes::AbiError`] is what a library's error type implements; [`codes::status`] turns a `Result` into a code and [`codes::check`] turns a code back into a `Result` |
//! | [`guard`] | A panic answers [`codes::ERR_PANIC`] instead of unwinding into C, including inside a `_destroy` that has no code to answer with. Behind the default-on `std` feature |
//!
//! # What this crate does not hold
//!
//! No pointer is dereferenced here, no symbol is exported, and nothing is
//! `unsafe`. The mechanics of a boundary — reading a caller's pointer, the
//! measure-then-copy buffer shape, the table behind an enumeration's `_count`
//! and `_at`, and a conformance kit — are `extendedresearch-abi`, which depends
//! on this crate. A package's safe core depends on this one alone.
//!
//! # `no_std`
//!
//! Default features off, the crate is `#![no_std]`: the codes, the predicates,
//! [`codes::name`], [`codes::describe`], [`codes::AbiError`],
//! [`codes::status`] and [`codes::check`] need nothing from the operating
//! system. [`guard`] is the exception and is what the `std` feature turns on.
//!
//! [`codes::token`] allocates a `String`, so the crate links `alloc` whether or
//! not `std` is on.
//!
//! ```bash
//! cargo check -p extendedresearch-status --no-default-features
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod codes;
#[cfg(feature = "std")]
pub mod guard;

// The README's example is the first code a reader of the package page sees;
// this runs it as a doctest so it cannot drift from the crate.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExample;
