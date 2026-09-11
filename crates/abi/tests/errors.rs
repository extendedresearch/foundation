//! The error-code checks catch each defect they claim to.
//!
//! As in `tests/conformance.rs`: a check that passed everything would be
//! indistinguishable from one that checked nothing, so each rule has a table or
//! an error type here that breaks exactly it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use extendedresearch_abi::codes::{self, AbiError, DOMAIN_FLOOR};
use extendedresearch_abi::conformance::{error_codes, errors};

const THING_ERR_REFUSED: i32 = DOMAIN_FLOOR;
const THING_ERR_FULL: i32 = DOMAIN_FLOOR - 1;

const DECLARED: &[(i32, &str)] = &[
    (THING_ERR_REFUSED, "THING_ERR_REFUSED"),
    (THING_ERR_FULL, "THING_ERR_FULL"),
];

/// An error type whose code and name are whatever the test says.
struct Error {
    code: i32,
    name: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.code)
    }
}

impl AbiError for Error {
    fn code(&self) -> i32 {
        self.code
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

fn error(code: i32, name: &'static str) -> Error {
    Error { code, name }
}

fn panics(body: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(body)).is_err()
}

#[test]
fn a_correct_table_passes_and_comes_back() {
    assert_eq!(error_codes(DECLARED), DECLARED);
}

#[test]
fn an_empty_table_passes() {
    // A package with no failures of its own is allowed; it answers boundary
    // codes only.
    assert!(error_codes(&[]).is_empty());
}

#[test]
fn a_code_above_the_floor_is_caught() {
    assert!(panics(|| {
        error_codes(&[(DOMAIN_FLOOR + 1, "THING_ERR_HIGH")]);
    }));
    assert!(panics(|| {
        error_codes(&[(codes::OK, "THING_OK")]);
    }));
    assert!(panics(|| {
        error_codes(&[(3, "THING_ERR_POSITIVE")]);
    }));
}

#[test]
fn two_entries_sharing_a_code_are_caught() {
    assert!(panics(|| {
        error_codes(&[(THING_ERR_FULL, "A"), (THING_ERR_FULL, "B")]);
    }));
}

#[test]
fn two_entries_sharing_a_name_are_caught() {
    assert!(panics(|| {
        error_codes(&[(THING_ERR_FULL, "A"), (THING_ERR_REFUSED, "A")]);
    }));
}

#[test]
fn an_empty_name_is_caught() {
    assert!(panics(|| {
        error_codes(&[(THING_ERR_FULL, "")]);
    }));
}

#[test]
fn a_boundary_name_on_a_domain_code_is_caught() {
    assert!(panics(|| {
        error_codes(&[(THING_ERR_FULL, "ERR_NULL")]);
    }));
}

#[test]
fn correct_error_values_pass() {
    errors(
        [
            error(codes::ERR_NULL, "ERR_NULL"),
            error(codes::ERR_PANIC, "ERR_PANIC"),
            error(THING_ERR_REFUSED, "THING_ERR_REFUSED"),
            error(THING_ERR_FULL, "THING_ERR_FULL"),
        ],
        DECLARED,
    );
}

#[test]
fn an_error_that_reads_as_success_is_caught() {
    assert!(panics(|| errors([error(codes::OK, "THING_OK")], DECLARED)));
    assert!(panics(|| errors([error(1, "THING_ONE")], DECLARED)));
}

#[test]
fn a_boundary_code_under_another_name_is_caught() {
    assert!(panics(|| errors(
        [error(codes::ERR_NULL, "ERR_UTF8")],
        DECLARED
    )));
    assert!(panics(|| {
        errors([error(codes::ERR_NULL, "THING_ERR_NULL")], DECLARED);
    }));
}

#[test]
fn a_domain_code_the_table_lacks_is_caught() {
    assert!(panics(|| {
        errors([error(DOMAIN_FLOOR - 9, "THING_ERR_UNDECLARED")], DECLARED);
    }));
}

#[test]
fn a_domain_code_under_the_wrong_name_is_caught() {
    assert!(panics(|| {
        errors([error(THING_ERR_FULL, "THING_ERR_REFUSED")], DECLARED);
    }));
}

#[test]
fn an_unused_boundary_code_in_the_reserved_gap_is_not_domain() {
    // `-6` to `-15` carry no name yet. An error answering one is refused
    // rather than passed, because `codes::name` has nothing to match.
    assert!(panics(|| errors([error(-6, "ERR_SOMETHING")], DECLARED)));
}
