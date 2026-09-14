//! Checks a library runs against its own exported functions, and against its
//! own error codes.
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
//!
//! **Each check takes closures, so pass an `extern "C"` function wrapped in
//! one:** `|d, c, l| thing_name(d, c, l)`. Rust implements the closure traits
//! only for Rust-ABI functions, and a call bound to a handle —
//! `|d, c, l| stream_name(stream, d, c, l)` — needs a closure anyway.

use std::ffi::c_char;

use crate::codes::{
    self, AbiError, DOMAIN_FLOOR, ERR_NULL, ERR_RANGE, OK, is_boundary, is_domain, name,
};

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

/// Check an enumeration's `_count`, `_at` and `_name`, and return every value
/// with its name, in `_at` order.
///
/// A library compares what this returns against its own contract, in both
/// directions — a value the contract declares and the enumeration lacks, and a
/// value the enumeration invents — because only the library knows its contract.
///
/// # Panics
///
/// When any of these fails, each a rule in [`crate::enumeration`]:
///
/// - `_count` answers [`OK`] and at least one value; a null `out_count` is
///   [`ERR_NULL`].
/// - `_at` answers [`OK`] below the count and [`ERR_RANGE`] at it; a null
///   `out_value` is [`ERR_NULL`].
/// - `_name` passes [`text_answer`] for every enumerated value, and answers
///   [`ERR_RANGE`] for a value that is not enumerated.
/// - No value, and no name, is enumerated twice.
pub fn enumeration<C, A, N>(mut count: C, mut at: A, mut name: N) -> Vec<(i32, String)>
where
    C: FnMut(*mut u32) -> i32,
    A: FnMut(u32, *mut i32) -> i32,
    N: FnMut(i32, *mut c_char, u64, *mut u64) -> i32,
{
    let mut total = 0u32;
    expect(count(&raw mut total), OK, "_count must succeed");
    assert!(
        total > 0,
        "_count answered zero; an enumeration with no values enumerates nothing"
    );
    expect(
        count(std::ptr::null_mut()),
        ERR_NULL,
        "a null out_count must be refused",
    );

    let mut entries: Vec<(i32, String)> = Vec::new();
    for index in 0..total {
        let mut value = 0i32;
        expect(
            at(index, &raw mut value),
            OK,
            "_at must succeed below the count",
        );
        let text = text_answer(|destination, capacity, out_len| {
            name(value, destination, capacity, out_len)
        });
        if let Some((_, first)) = entries.iter().find(|(seen, _)| *seen == value) {
            panic!("the value {value} is enumerated twice, as {first} and as {text}");
        }
        assert!(
            entries.iter().all(|(_, seen)| *seen != text),
            "the name {text} is enumerated twice"
        );
        entries.push((value, text));
    }

    let mut past = 0i32;
    expect(
        at(total, &raw mut past),
        ERR_RANGE,
        "_at at the count must be refused",
    );
    expect(
        at(0, std::ptr::null_mut()),
        ERR_NULL,
        "a null out_value must be refused",
    );

    let unknown = [i32::MIN, i32::MAX, -9_999, 9_999]
        .into_iter()
        .find(|candidate| entries.iter().all(|(value, _)| value != candidate));
    if let Some(unknown) = unknown {
        let mut needed = 0u64;
        expect(
            name(unknown, std::ptr::null_mut(), 0, &raw mut needed),
            ERR_RANGE,
            "_name for a value that is not enumerated must be refused",
        );
    }
    entries
}

/// Check that a `_destroy` accepts null, which is what lets a finalizer call it
/// on a field it already cleared.
///
/// A `_destroy` that dereferences null faults the process rather than
/// returning, so this has nothing to assert beyond returning at all.
pub fn destroy_accepts_null<T, F>(destroy: F)
where
    F: FnOnce(*mut T),
{
    destroy(std::ptr::null_mut());
}

/// Check a package's table of its own error codes, and return it.
///
/// `declared` is every domain code the package's header defines, with the
/// constant's name: `&[(EXAMPLE_ERR_TRUNCATED, "EXAMPLE_ERR_TRUNCATED")]`. The
/// boundary codes are this crate's and do not belong in it.
///
/// # Panics
///
/// When any of these fails, each a rule in [`crate::codes`]:
///
/// - Every code is at or below [`DOMAIN_FLOOR`], so it cannot collide with a
///   boundary code this crate adds later.
/// - No two entries share a code, and no two share a name.
/// - Every name is non-empty and is not a boundary code's name.
pub fn error_codes<'a>(declared: &'a [(i32, &'a str)]) -> &'a [(i32, &'a str)] {
    for (at, (code, name)) in declared.iter().enumerate() {
        assert!(
            is_domain(*code),
            "{name} is {code}, above DOMAIN_FLOOR ({DOMAIN_FLOOR}); a package numbers its own codes from {DOMAIN_FLOOR} down",
        );
        assert!(!name.is_empty(), "the code {code} has an empty name");
        let boundary = (DOMAIN_FLOOR + 1..0).any(|b| codes::name(b) == Some(*name));
        assert!(
            !boundary,
            "{name} is a boundary code's name and cannot name the domain code {code}"
        );
        for (other_code, other_name) in declared.iter().skip(at + 1) {
            assert!(
                code != other_code,
                "{name} and {other_name} are both {code}"
            );
            assert!(
                name != other_name,
                "{name} is declared twice, as {code} and as {other_code}"
            );
        }
    }
    declared
}

/// Check a package's error values against its declared codes.
///
/// Pass one value of every variant the error type has; `declared` is the table
/// [`error_codes`] accepted.
///
/// # Panics
///
/// When a value breaks any of these:
///
/// - Its code is negative.
/// - A boundary code carries the name [`codes::name`] gives it.
/// - A domain code is in `declared`, under the same name.
pub fn errors<E, I>(values: I, declared: &[(i32, &str)])
where
    E: AbiError,
    I: IntoIterator<Item = E>,
{
    for value in values {
        let (code, name) = (value.code(), value.name());
        assert!(code < 0, "`{value}` answers {code}, which reads as success");
        if is_boundary(code) {
            assert_eq!(
                codes::name(code),
                Some(name),
                "`{value}` answers the boundary code {code} under the name {name}"
            );
            continue;
        }
        match declared.iter().find(|(known, _)| *known == code) {
            Some((_, known)) => assert_eq!(
                *known, name,
                "`{value}` answers {code} as {name}, and the table names {code} {known}"
            ),
            None => panic!(
                "`{value}` answers {name} ({code}), which the package's table does not declare"
            ),
        }
    }
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
