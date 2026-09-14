//! The short-name rule, with no interpreter involved.
//!
//! The same rule is written in TypeScript in `npm/binding-runtime/src/enums.ts`, and
//! these cases are the ones that file's comment lists.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use extendedresearch_abi::enumeration::Enumeration;
use extendedresearch_pyo3::{member_names, shared_prefix, short_name};

#[test]
fn the_prefix_is_cut_back_to_an_underscore() {
    assert_eq!(shared_prefix(&["ORIGIN_UNSPECIFIED", "ORIGIN_RAW"]), 7);
    // `REFUSE_REASON_P` is shared by these two, and half a word is not a
    // prefix.
    assert_eq!(
        shared_prefix(&["REFUSE_REASON_PORTS", "REFUSE_REASON_PROTOCOL"]),
        14
    );
}

#[test]
fn names_with_nothing_in_common_strip_nothing() {
    assert_eq!(shared_prefix(&["ALPHA", "BETA"]), 0);
    assert_eq!(shared_prefix(&["AB_X", "AC_Y"]), 0);
    assert_eq!(shared_prefix::<&str>(&[]), 0);
}

#[test]
fn a_single_name_strips_to_its_last_underscore() {
    assert_eq!(shared_prefix(&["STAMP_UNDATED"]), 6);
    assert_eq!(shared_prefix(&["ONLY"]), 0);
}

#[test]
fn a_short_name_that_is_empty_or_starts_with_a_digit_falls_back() {
    assert_eq!(short_name("ORIGIN_RAW", 7), "RAW");
    assert_eq!(short_name("RATE_50HZ", 5), "RATE_50HZ");
    assert_eq!(short_name("RATE_", 5), "RATE_");
    assert_eq!(short_name("ANY", 99), "ANY");
}

#[test]
fn member_names_put_the_short_name_first_and_skip_a_redundant_alias() {
    static TABLE: Enumeration = Enumeration::new(&[(0, "ONE_A"), (1, "ONE_9")]);
    assert_eq!(
        member_names(&TABLE),
        [("A", 0), ("ONE_A", 0), ("ONE_9", 1)],
        "ONE_9 keeps its full name and is bound once"
    );
}
