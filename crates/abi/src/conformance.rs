//! Checks a library runs against its own exported functions.
//!
//! Each function here drives one exported function the way a binding does —
//! through raw pointers, with buffers a binding would allocate — and panics
//! naming the rule that was broken. A library calls them from its tests:
//!
//! ```
//! use std::ffi::c_char;
//! use extendedresearch_abi::{borrow, conformance};
//!
//! // Stands in for a library's `extern "C" fn thing_name(...)`.
//! fn thing_name(destination: *mut c_char, capacity: u64, out_len: *mut u64) -> i32 {
//!     // SAFETY: `conformance` passes a null or a buffer of `capacity` bytes, and
//!     // a null or a valid `u64`.
//!     unsafe { borrow::fill_text("tracker", destination, capacity, out_len) }
//! }
//!
//! assert_eq!(conformance::text_answer(thing_name), "tracker");
//! ```
//!
//! A check that only ran the ordinary path would pass a function that counted
//! the terminator in `*out_len`, or wrote one byte past `capacity`. These run
//! the edges, which is where each of those defects lives.

use std::ffi::c_char;

use crate::codes::{ERR_NULL, ERR_RANGE, OK, name};

/// A byte no conforming function writes on its own, so a write past the answer
/// shows.
const SENTINEL: u8 = 0xA5;

/// Check a function that answers text in the measure-then-copy shape, and
/// return the text it answered.
///
/// # Panics
///
/// When the function breaks any of these, each of which is a rule in
/// [`crate::buffer`]:
///
/// - A null destination measures, answers [`OK`], and sets `*out_len`.
/// - A null `out_len` is [`ERR_NULL`].
/// - A capacity of exactly `*out_len` is [`ERR_RANGE`], still sets `*out_len`,
///   and writes nothing — because the terminator does not fit.
/// - A capacity of `*out_len + 1` answers [`OK`], writes the text and a
///   terminator, and writes nothing past them.
/// - The text is UTF-8 and contains no null before the terminator.
pub fn text_answer<F>(mut read: F) -> String
where
    F: FnMut(*mut c_char, u64, *mut u64) -> i32,
{
    let needed = measured(|destination, capacity, out_len| {
        read(destination.cast::<c_char>(), capacity, out_len)
    });
    let len = to_usize(needed);

    let mut exact = vec![SENTINEL; len];
    let mut reported = u64::MAX;
    expect(
        read(
            exact.as_mut_ptr().cast::<c_char>(),
            needed,
            &raw mut reported,
        ),
        ERR_RANGE,
        "a capacity of exactly *out_len leaves no room for the terminator",
    );
    assert_eq!(
        reported, needed,
        "a refusal for capacity must still report what was needed"
    );
    assert!(
        exact.iter().all(|byte| *byte == SENTINEL),
        "a refusal for capacity wrote into the caller's buffer"
    );

    // One byte of room past what the answer needs, which must stay untouched.
    let mut room = vec![SENTINEL; len + 2];
    let mut written = u64::MAX;
    expect(
        read(
            room.as_mut_ptr().cast::<c_char>(),
            needed + 1,
            &raw mut written,
        ),
        OK,
        "*out_len + 1 must be enough for text",
    );
    assert_eq!(written, needed, "*out_len changed between two calls");
    let Some((body, rest)) = room.split_at_checked(len) else {
        unreachable!("room is longer than len");
    };
    assert_eq!(
        rest,
        [0, SENTINEL],
        "text must be null-terminated, and nothing written past the terminator"
    );
    assert!(
        !body.contains(&0),
        "the text contains a null, so a C caller reading to the first null gets less than *out_len says"
    );
    match std::str::from_utf8(body) {
        Ok(text) => text.to_owned(),
        Err(error) => panic!("the text answered is not UTF-8: {error}"),
    }
}

/// Check a function that answers a byte run in the measure-then-copy shape, and
/// return the bytes it answered.
///
/// # Panics
///
/// As [`text_answer`], except that a byte run has no terminator: a capacity of
/// exactly `*out_len` must answer [`OK`], and one short of it [`ERR_RANGE`].
pub fn bytes_answer<F>(mut read: F) -> Vec<u8>
where
    F: FnMut(*mut u8, u64, *mut u64) -> i32,
{
    let needed = measured(&mut read);
    let len = to_usize(needed);

    if let Some(short) = needed.checked_sub(1) {
        let mut buffer = vec![SENTINEL; len];
        let mut reported = u64::MAX;
        expect(
            read(buffer.as_mut_ptr(), short, &raw mut reported),
            ERR_RANGE,
            "a capacity one short of *out_len must be refused",
        );
        assert_eq!(
            reported, needed,
            "a refusal for capacity must still report what was needed"
        );
        assert!(
            buffer.iter().all(|byte| *byte == SENTINEL),
            "a refusal for capacity wrote into the caller's buffer"
        );
    }

    let mut room = vec![SENTINEL; len + 1];
    let mut written = u64::MAX;
    expect(
        read(room.as_mut_ptr(), needed, &raw mut written),
        OK,
        "a capacity of exactly *out_len must be enough for a byte run",
    );
    assert_eq!(written, needed, "*out_len changed between two calls");
    assert_eq!(
        room.pop(),
        Some(SENTINEL),
        "a byte run was written past its length"
    );
    room
}

/// The measuring and null-`out_len` checks both shapes share.
fn measured<F>(mut read: F) -> u64
where
    F: FnMut(*mut u8, u64, *mut u64) -> i32,
{
    let mut needed = u64::MAX;
    expect(
        read(std::ptr::null_mut(), 0, &raw mut needed),
        OK,
        "a null destination must measure",
    );
    assert_ne!(needed, u64::MAX, "measuring did not set *out_len");

    let mut unused = [SENTINEL; 1];
    expect(
        read(unused.as_mut_ptr(), 1, std::ptr::null_mut()),
        ERR_NULL,
        "a null out_len must be refused",
    );
    needed
}

fn expect(actual: i32, expected: i32, rule: &str) {
    assert!(
        actual == expected,
        "{rule}: answered {actual} ({}), expected {expected} ({})",
        describe(actual),
        describe(expected),
    );
}

fn describe(code: i32) -> &'static str {
    match code {
        OK => "OK",
        other => name(other).unwrap_or("not a boundary code"),
    }
}

fn to_usize(len: u64) -> usize {
    match usize::try_from(len) {
        Ok(len) => len,
        Err(_) => panic!("*out_len of {len} is not addressable on this platform"),
    }
}
