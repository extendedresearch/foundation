//! The test library passes the conformance kit, and its adapter answers the
//! codes its core's errors carry.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::ffi::CString;

use extendedresearch_abi::codes::{ERR_NULL, ERR_PANIC, ERR_STATE, ERR_UTF8, OK};
use extendedresearch_abi::{binding, conformance};
use extendedresearch_abi_testlib::{
    COLORS, Counter, CounterError, ERROR_CODES, TESTLIB_ERR_EMPTY_NAME, TESTLIB_ERR_FULL,
    testlib_color_at, testlib_color_count, testlib_color_name, testlib_counter_close,
    testlib_counter_create, testlib_counter_destroy, testlib_counter_increment,
    testlib_counter_name, testlib_counter_name_bytes, testlib_destroyed_count, testlib_panic,
};

fn create(name: &str, limit: u32) -> *mut Counter {
    let name = CString::new(name).unwrap();
    let mut counter = std::ptr::null_mut();
    assert_eq!(
        unsafe { testlib_counter_create(name.as_ptr(), limit, &raw mut counter) },
        OK
    );
    counter
}

#[test]
fn the_error_table_and_every_error_value_conform() {
    conformance::error_codes(ERROR_CODES);
    conformance::errors(
        [
            CounterError::EmptyName,
            CounterError::Full(1),
            CounterError::Closed,
        ],
        ERROR_CODES,
    );
}

#[test]
fn the_name_conforms_as_text_and_as_bytes() {
    let counter = create("Zähler", 1);
    assert_eq!(
        conformance::text_answer(|d, c, l| unsafe { testlib_counter_name(counter, d, c, l) }),
        "Zähler"
    );
    assert_eq!(
        conformance::bytes_answer(|d, c, l| unsafe {
            testlib_counter_name_bytes(counter, d, c, l)
        }),
        "Zähler".as_bytes()
    );
    assert_eq!(
        binding::text(|d, c, l| unsafe { testlib_counter_name(counter, d, c, l) }).unwrap(),
        "Zähler"
    );
    unsafe { testlib_counter_destroy(counter) };
}

#[test]
fn the_colours_conform_and_match_the_table() {
    let entries = conformance::enumeration(
        |c| unsafe { testlib_color_count(c) },
        |i, v| unsafe { testlib_color_at(i, v) },
        |value, d, c, l| unsafe { testlib_color_name(value, d, c, l) },
    );
    let expected: Vec<(i32, String)> = COLORS
        .entries()
        .iter()
        .map(|(value, name)| (*value, (*name).to_owned()))
        .collect();
    assert_eq!(entries, expected);
}

#[test]
fn destroy_accepts_null_and_counts_what_it_frees() {
    conformance::destroy_accepts_null(|counter| unsafe { testlib_counter_destroy(counter) });
    let before = testlib_destroyed_count();
    unsafe { testlib_counter_destroy(create("one", 1)) };
    assert!(testlib_destroyed_count() > before);
}

#[test]
fn create_answers_boundary_and_domain_codes() {
    let mut counter = std::ptr::null_mut();
    assert_eq!(
        unsafe { testlib_counter_create(std::ptr::null(), 1, &raw mut counter) },
        ERR_NULL
    );
    let not_utf8 = [0xFF_u8, 0];
    assert_eq!(
        unsafe { testlib_counter_create(not_utf8.as_ptr().cast(), 1, &raw mut counter) },
        ERR_UTF8
    );
    let empty = CString::new("").unwrap();
    assert_eq!(
        unsafe { testlib_counter_create(empty.as_ptr(), 1, &raw mut counter) },
        TESTLIB_ERR_EMPTY_NAME
    );
    let name = CString::new("x").unwrap();
    assert_eq!(
        unsafe { testlib_counter_create(name.as_ptr(), 1, std::ptr::null_mut()) },
        ERR_NULL
    );
}

#[test]
fn increment_answers_full_and_then_state() {
    let counter = create("limited", 2);
    let mut value = 0u32;
    for expected in 1..=2 {
        assert_eq!(
            unsafe { testlib_counter_increment(counter, &raw mut value) },
            OK
        );
        assert_eq!(value, expected);
    }
    assert_eq!(
        unsafe { testlib_counter_increment(counter, &raw mut value) },
        TESTLIB_ERR_FULL
    );
    assert_eq!(unsafe { testlib_counter_close(counter) }, OK);
    assert_eq!(
        unsafe { testlib_counter_increment(counter, &raw mut value) },
        ERR_STATE
    );
    assert_eq!(
        unsafe { testlib_counter_increment(counter, std::ptr::null_mut()) },
        ERR_NULL
    );
    unsafe { testlib_counter_destroy(counter) };
}

#[test]
fn a_panic_is_answered_not_unwound() {
    assert_eq!(testlib_panic(), ERR_PANIC);
}
