//! Measure, then copy — the one shape every variable-length answer uses.
//!
//! ```c
//! int32_t f(..., uint8_t *destination, uint64_t capacity, uint64_t *out_len);
//! ```
//!
//! A null `destination` **measures**: `*out_len` is set to what is needed and
//! nothing is written. A `capacity` that is too small answers
//! [`ERR_RANGE`] with `*out_len` still set, so a caller that guessed low
//! allocates exactly and retries. A null `out_len` is [`ERR_NULL`], because an
//! answer whose length the caller cannot learn is not one it can use.
//!
//! # The rule that is easy to get wrong in each language separately
//!
//! **`*out_len` never counts a string's null terminator, and `capacity` always
//! must.** Measure, allocate `*out_len + 1`, call again. Byte runs have no
//! terminator, so `capacity >= *out_len` is enough.
//!
//! Written once here because it is two rules that differ by one byte, and a
//! binding author who gets it wrong gets a truncated last character rather than
//! an error — which is the kind of defect that reaches a user.
//!
//! # No pointer into the library's memory is ever returned
//!
//! So the question *is this borrowed or owned* cannot be asked, and there is no
//! `_string_free` because nothing needs freeing. It costs less than it sounds:
//! a .NET caller passes a managed array and the marshaller pins it, so the write
//! lands directly in the caller's final destination with no second copy.
//!
//! # This module is the safe half
//!
//! The functions here take views the caller's pointers have already been turned
//! into. A library's exported function holds raw pointers, and calls
//! [`borrow::fill_bytes`](crate::borrow::fill_bytes) or
//! [`borrow::fill_text`](crate::borrow::fill_text), which make the views and
//! call these.

use std::mem::MaybeUninit;

use crate::codes::{ERR_NULL, ERR_RANGE, OK};

/// Report how many bytes an answer needs, without writing any.
///
/// The measuring half of the shape, for a library that can compute a length more
/// cheaply than the content.
#[must_use]
pub fn measure(needed: usize, out_len: Option<&mut MaybeUninit<u64>>) -> i32 {
    let Some(slot) = out_len else {
        return ERR_NULL;
    };
    slot.write(needed as u64);
    OK
}

/// Copy a byte run into the caller's buffer, or measure it.
///
/// `destination` of `None` measures. A destination shorter than `bytes`
/// answers [`ERR_RANGE`] with `out_len` still written, and writes nothing.
///
/// Byte runs carry no terminator, so `capacity >= len` is sufficient.
#[must_use]
pub fn fill_bytes(
    bytes: &[u8],
    destination: Option<&mut [MaybeUninit<u8>]>,
    out_len: Option<&mut MaybeUninit<u64>>,
) -> i32 {
    let status = measure(bytes.len(), out_len);
    if status != OK {
        return status;
    }
    let Some(destination) = destination else {
        return OK;
    };
    let Some(body) = destination.get_mut(..bytes.len()) else {
        return ERR_RANGE;
    };
    copy(body, bytes);
    OK
}

/// Copy text into the caller's buffer as UTF-8 with a null terminator, or
/// measure it.
///
/// `out_len` is the length **without** the terminator; the capacity required is
/// `out_len + 1`. A caller therefore measures, allocates `*out_len + 1`, and
/// calls again — which is the sentence a binding author has to get right.
#[must_use]
pub fn fill_text(
    text: &str,
    destination: Option<&mut [MaybeUninit<u8>]>,
    out_len: Option<&mut MaybeUninit<u64>>,
) -> i32 {
    let bytes = text.as_bytes();
    let status = measure(bytes.len(), out_len); // never counts the terminator
    if status != OK {
        return status;
    }
    let Some(destination) = destination else {
        return OK;
    };
    // Capacity always must count the terminator.
    let Some((terminator, body)) = destination
        .get_mut(..=bytes.len())
        .and_then(<[_]>::split_last_mut)
    else {
        return ERR_RANGE;
    };
    copy(body, bytes);
    terminator.write(0);
    OK
}

fn copy(destination: &mut [MaybeUninit<u8>], source: &[u8]) {
    for (slot, byte) in destination.iter_mut().zip(source) {
        slot.write(*byte);
    }
}
