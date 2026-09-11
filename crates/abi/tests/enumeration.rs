//! An enumeration's three functions, driven the way a binding drives them, and
//! the conformance kit refusing each way they can be wrong.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
// Calling an `unsafe fn` is what a library's exported function does, and each
// call here passes pointers the test owns or that the kit passes in.
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use extendedresearch_abi::borrow::{
    enumeration_at, enumeration_count, enumeration_name, fill_text,
};
use extendedresearch_abi::codes::{ERR_NULL, ERR_RANGE, OK};
use extendedresearch_abi::conformance;
use extendedresearch_abi::enumeration::Enumeration;

const THING_A: i32 = 0;
const THING_B: i32 = 1;
const THING_C: i32 = 5;

static THINGS: Enumeration = Enumeration::new(&[
    (THING_A, "THING_A"),
    (THING_B, "THING_B"),
    (THING_C, "THING_C"),
]);

extern "C" fn thing_count(out_count: *mut u32) -> i32 {
    unsafe { enumeration_count(&THINGS, out_count) }
}

extern "C" fn thing_at(index: u32, out_value: *mut i32) -> i32 {
    unsafe { enumeration_at(&THINGS, index, out_value) }
}

extern "C" fn thing_name(
    value: i32,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    unsafe { enumeration_name(&THINGS, value, destination, capacity, out_len) }
}

// The kit takes closures, and Rust implements the closure traits only for
// Rust-ABI functions, so each export is wrapped once.
fn count(out_count: *mut u32) -> i32 {
    thing_count(out_count)
}

fn at(index: u32, out_value: *mut i32) -> i32 {
    thing_at(index, out_value)
}

fn name(value: i32, destination: *mut c_char, capacity: u64, out_len: *mut u64) -> i32 {
    thing_name(value, destination, capacity, out_len)
}

#[test]
fn the_three_functions_pass_the_conformance_kit() {
    let entries = conformance::enumeration(count, at, name);
    assert_eq!(
        entries,
        [
            (THING_A, "THING_A".to_owned()),
            (THING_B, "THING_B".to_owned()),
            (THING_C, "THING_C".to_owned()),
        ],
        "the kit answers every value with its name, in `_at` order"
    );
}

#[test]
fn past_the_end_and_an_unknown_value_are_range_errors() {
    let mut value = 0;
    assert_eq!(thing_at(3, &raw mut value), ERR_RANGE);
    let mut needed = 0u64;
    assert_eq!(
        thing_name(2, std::ptr::null_mut(), 0, &raw mut needed),
        ERR_RANGE
    );
}

#[test]
fn a_null_out_parameter_is_refused_before_the_index_is_looked_at() {
    // Past the end *and* null: null wins, as it does in ranvier.
    assert_eq!(thing_at(99, std::ptr::null_mut()), ERR_NULL);
    assert_eq!(thing_count(std::ptr::null_mut()), ERR_NULL);
}

#[test]
fn a_table_that_repeats_a_value_or_a_name_or_holds_nothing_is_refused() {
    assert!(catch_unwind(|| Enumeration::new(&[(0, "A"), (0, "B")])).is_err());
    assert!(catch_unwind(|| Enumeration::new(&[(0, "A"), (1, "A")])).is_err());
    assert!(catch_unwind(|| Enumeration::new(&[])).is_err());
    assert!(catch_unwind(|| Enumeration::new(&[(0, "A"), (1, "AB")])).is_ok());
}

#[test]
fn names_and_entries_read_back() {
    assert_eq!(THINGS.name_of(THING_C), Some("THING_C"));
    assert_eq!(THINGS.name_of(4), None);
    assert_eq!(THINGS.entries().len(), 3);
}

// The kit refusing each defect it claims to catch.

fn refuses<C, A, N>(count: C, at: A, name: N) -> bool
where
    C: FnMut(*mut u32) -> i32,
    A: FnMut(u32, *mut i32) -> i32,
    N: FnMut(i32, *mut c_char, u64, *mut u64) -> i32,
{
    catch_unwind(AssertUnwindSafe(|| {
        conformance::enumeration(count, at, name)
    }))
    .is_err()
}

#[test]
fn an_at_that_answers_past_the_end_is_caught() {
    let lenient_at = |index: u32, out: *mut i32| {
        let answer = thing_at(index, out);
        if answer == ERR_RANGE { OK } else { answer }
    };
    assert!(refuses(count, lenient_at, name));
}

#[test]
fn a_name_for_a_value_not_enumerated_is_caught() {
    let inventive_name = |value: i32, d: *mut c_char, c: u64, l: *mut u64| {
        if THINGS.name_of(value).is_some() {
            thing_name(value, d, c, l)
        } else {
            unsafe { fill_text("INVENTED", d, c, l) }
        }
    };
    assert!(refuses(count, at, inventive_name));
}

#[test]
fn a_count_that_accepts_null_is_caught() {
    let lenient_count = |out: *mut u32| if out.is_null() { OK } else { thing_count(out) };
    assert!(refuses(lenient_count, at, name));
}

#[test]
fn an_enumeration_that_repeats_a_value_is_caught() {
    // Bypasses `Enumeration::new`, which would refuse this table, to stand in
    // for a hand-written library.
    let repeating_at = |index: u32, out: *mut i32| {
        let answer = thing_at(index, out);
        if answer == OK && index == 1 {
            unsafe { *out = THING_A };
        }
        answer
    };
    assert!(refuses(count, repeating_at, name));
}

#[test]
fn an_empty_enumeration_is_caught() {
    let empty_count = |out: *mut u32| {
        if out.is_null() {
            return ERR_NULL;
        }
        unsafe { *out = 0 };
        OK
    };
    assert!(refuses(empty_count, at, name));
}
