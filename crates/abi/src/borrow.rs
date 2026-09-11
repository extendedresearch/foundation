//! The only place a caller's pointer is dereferenced.
//!
//! Every function here that takes a raw pointer is `unsafe fn`, and says in its
//! `# Safety` section what the caller of the `extern "C"` function promised. A
//! library calls these from inside its own exported functions, one `unsafe`
//! block per call, and the block is where that promise is asserted.
//!
//! # Why these are `unsafe fn` and not safe functions with a documented contract
//!
//! Inside one crate, a safe `fn` that dereferences a raw pointer is a judgement
//! call. Exported from a crate other code depends on, it is unsound: any safe
//! caller could pass `0x1 as *const T`. So the contract is in the type.
//!
//! # Why the caller's buffers are `MaybeUninit`
//!
//! A C caller passes `malloc`'d memory, and a .NET caller may pass a buffer it
//! has not cleared. Making a `&mut [u8]` over uninitialised memory violates
//! `slice::from_raw_parts_mut`'s documented contract whether or not anything
//! reads it, so a destination is `&mut [MaybeUninit<u8>]` and an out-parameter is
//! `&mut MaybeUninit<T>`. Both are only ever written.
//!
//! # What none of this can check
//!
//! **A handle that was already destroyed.** A pointer to freed memory is not
//! distinguishable from a live one at any cost this boundary can pay, and a
//! magic number would catch the easy half and give false confidence about the
//! rest. It is the one error left to the binding, which is why every binding is
//! expected to own its handles in whatever its language does automatically —
//! `SafeHandle` in .NET, reference counting in Python.

use std::ffi::{CStr, c_char};
use std::mem::MaybeUninit;

use crate::buffer;
use crate::codes::{ERR_NULL, ERR_RANGE, ERR_UTF8};
use crate::enumeration::Enumeration;

/// Hand a value across the boundary as an opaque handle.
///
/// This is what every `_create` does with the object it built. The pointer is
/// only ever given back to [`handle`], [`handle_mut`] or [`reclaim`].
#[must_use]
pub fn into_handle<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

/// A shared reference to what a handle points at, or `None` for null.
///
/// # Safety
///
/// `handle` must be null, or a pointer [`into_handle`] returned that has not been
/// passed to [`reclaim`], and nothing may hold an exclusive reference to it for
/// `'a`.
#[must_use]
pub unsafe fn handle<'a, T>(handle: *const T) -> Option<&'a T> {
    // SAFETY: the caller's promise above. Given it, the pointer came from
    // `Box::into_raw`, so it is aligned, initialised and valid for `T`.
    unsafe { handle.as_ref() }
}

/// An exclusive reference to what a handle points at, or `None` for null.
///
/// # Safety
///
/// As [`handle`], and additionally no other reference to the object may exist
/// for `'a` — which is the ABI's threading rule, and the caller's to hold.
#[must_use]
pub unsafe fn handle_mut<'a, T>(handle: *mut T) -> Option<&'a mut T> {
    // SAFETY: the caller's promise above, including exclusivity.
    unsafe { handle.as_mut() }
}

/// Take ownership back from a handle, so dropping it frees what it owns.
///
/// This is what every `_destroy` does. Null is `None`, so destroying null is a
/// no-op — which is what makes a `_destroy` safe to call from a finalizer that
/// already cleared its field. Destroying the same non-null handle twice is the
/// undetectable case in the module header.
///
/// # Safety
///
/// `handle` must be null, or a pointer [`into_handle`] returned for this `T` that
/// has not already been reclaimed, and nothing else may be using it.
#[must_use]
pub unsafe fn reclaim<T>(handle: *mut T) -> Option<Box<T>> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: pairs with the `Box::into_raw` in `into_handle`, once, by the
    // caller's promise.
    Some(unsafe { Box::from_raw(handle) })
}

/// An out-parameter the library writes, or `None` for null.
///
/// The slot is `MaybeUninit` because a caller's `uint64_t len;` is
/// uninitialised until the library writes it.
///
/// # Safety
///
/// `slot` must be null, or valid and aligned for writing one `T` for `'a`, with
/// nothing else accessing it.
#[must_use]
pub unsafe fn out<'a, T>(slot: *mut T) -> Option<&'a mut MaybeUninit<T>> {
    // SAFETY: `MaybeUninit<T>` has the layout of `T`, and validity for a write is
    // the caller's promise above.
    unsafe { slot.cast::<MaybeUninit<T>>().as_mut() }
}

/// Read a caller's string.
///
/// `Ok(None)` is null, which the library decides the meaning of — an absent
/// optional argument, or [`ERR_NULL`] where one is required.
///
/// # Errors
///
/// [`ERR_UTF8`] when the bytes are not UTF-8. A name that silently became a
/// different string would bind to the wrong thing and report nothing.
///
/// # Safety
///
/// `value` must be null, or a null-terminated string that stays valid and
/// unmodified for `'a`.
pub unsafe fn text<'a>(value: *const c_char) -> Result<Option<&'a str>, i32> {
    if value.is_null() {
        return Ok(None);
    }
    // SAFETY: null-terminated and stable for `'a` is the caller's promise.
    let raw = unsafe { CStr::from_ptr(value) };
    raw.to_str().map(Some).map_err(|_| ERR_UTF8)
}

/// Read a caller's array — a byte run, or an array of handles.
///
/// A zero length is an empty slice whatever the pointer is: a binding that
/// passes the empty slice its language gives it should not have to
/// special-case the pointer inside it.
///
/// # Errors
///
/// [`ERR_NULL`] for a null pointer with a non-zero length. [`ERR_RANGE`] for a
/// length this platform cannot address.
///
/// # Safety
///
/// `values` must be null, or aligned and valid for reading `len` initialised
/// `T`s, unmodified for `'a`.
pub unsafe fn slice<'a, T>(values: *const T, len: u64) -> Result<&'a [T], i32> {
    if len == 0 {
        return Ok(&[]);
    }
    if values.is_null() {
        return Err(ERR_NULL);
    }
    let len = usize::try_from(len).map_err(|_| ERR_RANGE)?;
    // SAFETY: valid for `len` elements is the caller's promise, and an
    // allocation that valid is no larger than `isize::MAX` bytes.
    Ok(unsafe { std::slice::from_raw_parts(values, len) })
}

/// `_count` for an [`Enumeration`]: how many values it holds.
///
/// # Safety
///
/// `out_count` must be null, or valid for writing one `u32`.
#[must_use]
pub unsafe fn enumeration_count(table: &Enumeration, out_count: *mut u32) -> i32 {
    // SAFETY: the caller's promise above, passed on unchanged.
    let out_count = unsafe { out(out_count) };
    table.count(out_count)
}

/// `_at` for an [`Enumeration`]: the value at `index`, or [`ERR_RANGE`] past the
/// end.
///
/// # Safety
///
/// `out_value` must be null, or valid for writing one `i32`.
#[must_use]
pub unsafe fn enumeration_at(table: &Enumeration, index: u32, out_value: *mut i32) -> i32 {
    // SAFETY: the caller's promise above, passed on unchanged.
    let out_value = unsafe { out(out_value) };
    table.at(index, out_value)
}

/// `_name` for an [`Enumeration`]: the contract's name for `value`, in the
/// measure-then-copy shape, or [`ERR_RANGE`] for a value the table does not
/// hold.
///
/// # Safety
///
/// As [`fill_text`].
#[must_use]
pub unsafe fn enumeration_name(
    table: &Enumeration,
    value: i32,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    let Some(name) = table.name_of(value) else {
        return ERR_RANGE;
    };
    // SAFETY: the caller's promise above, passed on unchanged.
    unsafe { fill_text(name, destination, capacity, out_len) }
}

/// Answer a byte run into a caller's buffer, or measure it.
///
/// The raw-pointer form of [`buffer::fill_bytes`]: the signature a library's
/// exported function already has, so the whole answer is one call.
///
/// # Safety
///
/// `destination` must be null or valid for writing `capacity` bytes, and
/// `out_len` null or valid for writing one `u64`, neither accessed by anything
/// else during the call.
#[must_use]
pub unsafe fn fill_bytes(
    value: &[u8],
    destination: *mut u8,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    // SAFETY: both halves of the caller's promise above, passed on unchanged.
    let (destination, out_len) =
        unsafe { (span(destination, capacity, value.len()), out(out_len)) };
    buffer::fill_bytes(value, destination, out_len)
}

/// Answer text into a caller's buffer, null-terminated, or measure it.
///
/// The raw-pointer form of [`buffer::fill_text`]. `*out_len` never counts the
/// terminator and `capacity` always must.
///
/// # Safety
///
/// As [`fill_bytes`].
#[must_use]
pub unsafe fn fill_text(
    value: &str,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    let wanted = value.len().saturating_add(1);
    // SAFETY: both halves of the caller's promise above, passed on unchanged.
    let (destination, out_len) = unsafe {
        (
            span(destination.cast::<u8>(), capacity, wanted),
            out(out_len),
        )
    };
    buffer::fill_text(value, destination, out_len)
}

/// The part of a caller's buffer an answer can touch: `wanted` bytes, or all of
/// `capacity` when that is fewer.
///
/// Never wider than the answer, so no reference is made over bytes the library
/// has no reason to reach.
///
/// # Safety
///
/// `destination` must be null or valid for writing `capacity` bytes, with
/// nothing else accessing them for `'a`.
unsafe fn span<'a>(
    destination: *mut u8,
    capacity: u64,
    wanted: usize,
) -> Option<&'a mut [MaybeUninit<u8>]> {
    if destination.is_null() {
        return None;
    }
    // A capacity this platform cannot address is at least `wanted`.
    let len = usize::try_from(capacity).map_or(wanted, |capacity| capacity.min(wanted));
    // SAFETY: `len <= capacity`, and `capacity` writable bytes is the caller's
    // promise. `MaybeUninit<u8>` asks nothing of the bytes' current contents.
    Some(unsafe { std::slice::from_raw_parts_mut(destination.cast::<MaybeUninit<u8>>(), len) })
}
