//! The calling side: what Rust code does to use a library through its C ABI.
//!
//! **The Python and Node bindings do not use this.** They call a package's
//! safe Rust core directly and convert its [`AbiError`](crate::codes::AbiError)
//! with `extendedresearch-pyo3` and `extendedresearch-napi`, so no pointer
//! crosses between them and the core. What calls the C ABI from Rust is a
//! package's tests of its own C adapter, and the drivers that exercise that
//! adapter the way a .NET or C caller would. For every variable-length answer
//! each one measures, allocates and copies, and for every failure it reads a
//! code; these are the shared form of those helpers.
//!
//! The .NET form of the same reading is `ExtendedResearch.Interop`, carried by
//! `extendedresearch-interop-sources`.
//!
//! **One case neither of those handled: an answer that grows between the
//! measuring call and the copying one.** A name changed by another thread in
//! between is refused with [`ERR_RANGE`], and the refusal carries the new size
//! (see [`crate::buffer`]), so [`text`] and [`bytes`] measure again rather than
//! fail, up to [`ATTEMPTS`] times.
//!
//! Nothing here holds a pointer past a call or allocates on the library's
//! behalf, so none of it needs `unsafe`. [`text`] and [`bytes`] take a closure,
//! which is also where a handle is bound: `|d, c, l| stream_name(stream, d, c, l)`.
//!
//! ```
//! use std::ffi::c_char;
//! use extendedresearch_abi::{binding, borrow};
//!
//! // Stands in for a library's `extern "C" fn thing_name(...)`.
//! fn thing_name(destination: *mut c_char, capacity: u64, out_len: *mut u64) -> i32 {
//!     // SAFETY: `binding::text` passes a null or a buffer of `capacity` bytes,
//!     // and a valid `u64`.
//!     unsafe { borrow::fill_text("tracker", destination, capacity, out_len) }
//! }
//!
//! assert_eq!(binding::text(thing_name).unwrap(), "tracker");
//! ```

use std::ffi::c_char;
use std::fmt;

use crate::codes::{ERR_RANGE, OK, describe, name};

/// How many times [`text`] and [`bytes`] copy an answer that keeps growing
/// before answering [`ReadError::Unstable`].
pub const ATTEMPTS: usize = 4;

/// Why reading an answer failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The library answered this failure code.
    Code(i32),
    /// The library reported a length this platform cannot allocate, or wrote
    /// more than it measured. A conforming library does neither.
    Length(u64),
    /// The library answered text that is not UTF-8. A conforming library never
    /// does.
    NotUtf8,
    /// The answer was larger on every one of [`ATTEMPTS`] copies than when it
    /// was last measured.
    Unstable,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(code) => match (name(*code), describe(*code)) {
                (Some(name), Some(meaning)) => write!(f, "{name} ({code}): {meaning}"),
                _ => write!(f, "the library answered {code}"),
            },
            Self::Length(len) => write!(
                f,
                "the library reported a length of {len}, which does not fit what it measured or what this platform can allocate"
            ),
            Self::NotUtf8 => f.write_str("the library answered text that is not UTF-8"),
            Self::Unstable => write!(
                f,
                "the answer grew on each of {ATTEMPTS} attempts to copy it"
            ),
        }
    }
}

impl std::error::Error for ReadError {}

/// `Ok` for [`OK`], and the code otherwise.
///
/// Every non-zero code is a failure to a caller, including one it has never
/// heard of — which is what lets a library add a code without every binding
/// knowing it first.
///
/// # Errors
///
/// The code itself, when it is not [`OK`].
pub fn check(code: i32) -> Result<(), i32> {
    if code == OK { Ok(()) } else { Err(code) }
}

/// Read text a library answers in the measure-then-copy shape.
///
/// # Errors
///
/// [`ReadError::Code`] for a failure the library answered, and the other
/// variants for an answer a conforming library would not give.
pub fn text<F>(mut call: F) -> Result<String, ReadError>
where
    F: FnMut(*mut c_char, u64, *mut u64) -> i32,
{
    let bytes = read(
        |destination, capacity, out_len| call(destination.cast::<c_char>(), capacity, out_len),
        1,
    )?;
    String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)
}

/// Read a byte run a library answers in the measure-then-copy shape.
///
/// # Errors
///
/// As [`text`], without [`ReadError::NotUtf8`].
pub fn bytes<F>(call: F) -> Result<Vec<u8>, ReadError>
where
    F: FnMut(*mut u8, u64, *mut u64) -> i32,
{
    read(call, 0)
}

/// Measure, allocate, copy — and measure again from the refusal if the answer
/// grew. `terminator` is the room past `*out_len` the shape needs: one byte for
/// text, none for a byte run.
fn read<F>(mut call: F, terminator: u64) -> Result<Vec<u8>, ReadError>
where
    F: FnMut(*mut u8, u64, *mut u64) -> i32,
{
    let mut needed = 0u64;
    check(call(std::ptr::null_mut(), 0, &raw mut needed)).map_err(ReadError::Code)?;

    for _ in 0..ATTEMPTS {
        let capacity = needed
            .checked_add(terminator)
            .ok_or(ReadError::Length(needed))?;
        let size = usize::try_from(capacity).map_err(|_| ReadError::Length(needed))?;
        if size == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; size];
        let mut written = 0u64;
        match call(buffer.as_mut_ptr(), capacity, &raw mut written) {
            OK => {
                if written > needed {
                    return Err(ReadError::Length(written));
                }
                // `written <= needed < size`, so this fits.
                buffer.truncate(usize::try_from(written).map_err(|_| ReadError::Length(written))?);
                return Ok(buffer);
            }
            ERR_RANGE => needed = written,
            other => return Err(ReadError::Code(other)),
        }
    }
    Err(ReadError::Unstable)
}
