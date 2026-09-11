//! The calling side, against functions that answer through `borrow` and
//! against ones that misbehave on purpose.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::cell::Cell;
use std::ffi::c_char;

use extendedresearch_abi::binding::{self, ATTEMPTS, ReadError};
use extendedresearch_abi::borrow::{fill_bytes, fill_text};
use extendedresearch_abi::codes::{ERR_NULL, ERR_STATE, OK};

fn answer_text(value: &str) -> impl FnMut(*mut c_char, u64, *mut u64) -> i32 + '_ {
    move |d, c, l| unsafe { fill_text(value, d, c, l) }
}

#[test]
fn text_and_bytes_read_what_the_library_answers() {
    assert_eq!(binding::text(answer_text("stream/α")).unwrap(), "stream/α");
    assert_eq!(binding::text(answer_text("")).unwrap(), "");
    let run = [0u8, 1, 0, 0xFF];
    assert_eq!(
        binding::bytes(|d, c, l| unsafe { fill_bytes(&run, d, c, l) }).unwrap(),
        run
    );
    assert!(
        binding::bytes(|d, c, l| unsafe { fill_bytes(&[], d, c, l) })
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_failure_code_is_passed_through_and_named() {
    let failure = binding::text(|_, _, _| ERR_STATE).unwrap_err();
    assert_eq!(failure, ReadError::Code(ERR_STATE));
    let message = failure.to_string();
    assert!(message.starts_with("ERR_STATE (-5):"), "{message}");

    // A code this crate does not name is still a failure, and still reported.
    assert_eq!(
        binding::bytes(|_, _, _| -42).unwrap_err().to_string(),
        "the library answered -42"
    );
    assert_eq!(binding::check(OK), Ok(()));
    assert_eq!(binding::check(ERR_NULL), Err(ERR_NULL));
}

#[test]
fn an_answer_that_grew_between_measuring_and_copying_is_measured_again() {
    // The first call measures "abc"; by the second the value is "abcdef", so the
    // copy is refused with the new size, and the third call succeeds.
    let calls = Cell::new(0);
    let growing = |d, c, l| {
        let value = if calls.get() == 0 { "abc" } else { "abcdef" };
        calls.set(calls.get() + 1);
        unsafe { fill_text(value, d, c, l) }
    };
    assert_eq!(binding::text(growing).unwrap(), "abcdef");
    assert_eq!(calls.get(), 3);
}

#[test]
fn an_answer_that_never_stops_growing_is_reported_rather_than_chased() {
    let calls = Cell::new(0usize);
    let text = "x".repeat(64);
    let always_growing = |d, c, l| {
        calls.set(calls.get() + 1);
        unsafe { fill_text(&text[..calls.get()], d, c, l) }
    };
    assert_eq!(binding::text(always_growing), Err(ReadError::Unstable));
    assert_eq!(
        calls.get(),
        ATTEMPTS + 1,
        "one measure, then ATTEMPTS copies"
    );
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    let raw = |d: *mut c_char, c, l| unsafe { fill_bytes(b"\xFF\xFE", d.cast::<u8>(), c, l) };
    assert_eq!(binding::text(raw), Err(ReadError::NotUtf8));
}

#[test]
fn a_library_that_writes_more_than_it_measured_is_refused() {
    let overclaiming = |d: *mut u8, _c, l: *mut u64| {
        unsafe { *l = if d.is_null() { 2 } else { 10 } };
        OK
    };
    assert_eq!(binding::bytes(overclaiming), Err(ReadError::Length(10)));
}
