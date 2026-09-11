//! The token protocol and the BigInt conversion, with no Node involved.
//!
//! Nothing here calls Node-API: `napi::Error` built with a status and a reason
//! and `BigInt` are plain Rust values. The Node side of the protocol is
//! `crates/napi-testaddon/test/addon.test.mjs`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fmt;

use extendedresearch_abi::codes::{self, AbiError, DOMAIN_FLOOR};
use extendedresearch_napi::{Report, SEPARATOR, Tokens, from_u64};
use napi::Status;
use napi::bindgen_prelude::BigInt;

static TOKENS: Tokens = Tokens::new("THING");

struct Failure {
    code: i32,
    name: &'static str,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("it went wrong")
    }
}

impl AbiError for Failure {
    fn code(&self) -> i32 {
        self.code
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

#[test]
fn a_boundary_name_gains_the_prefix_and_a_domain_name_keeps_its_own() {
    assert_eq!(TOKENS.token("ERR_NULL"), "THING_ERR_NULL");
    assert_eq!(TOKENS.token("OK"), "THING_OK");
    assert_eq!(TOKENS.token("THING_ERR_FULL"), "THING_ERR_FULL");
    // A name that merely starts with the same letters is not prefixed.
    assert_eq!(TOKENS.token("THINGS_ERR_X"), "THING_THINGS_ERR_X");
}

#[test]
fn an_error_s_message_is_the_token_the_separator_and_the_sentence() {
    let boundary = TOKENS.error(&Failure {
        code: codes::ERR_UTF8,
        name: "ERR_UTF8",
    });
    assert_eq!(boundary.reason, "THING_ERR_UTF8: it went wrong");
    assert_eq!(boundary.status, Status::GenericFailure);

    let domain = TOKENS.error(&Failure {
        code: DOMAIN_FLOOR,
        name: "THING_ERR_FULL",
    });
    assert_eq!(domain.reason, "THING_ERR_FULL: it went wrong");
    assert_eq!(SEPARATOR, ": ");
}

#[test]
fn binding_and_unknown_tokens_are_derived_from_the_prefix() {
    assert_eq!(
        TOKENS.binding_codes(),
        ["THING_ERR_BINDING", "THING_ERR_UNKNOWN"]
    );
    assert_eq!(
        TOKENS.binding_failure("no such argument").reason,
        "THING_ERR_BINDING: no such argument"
    );
}

fn owned(rows: &[(&str, i32)]) -> Vec<(String, i32)> {
    rows.iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect()
}

#[test]
fn the_status_table_is_ok_then_the_declared_boundary_codes_then_domain() {
    // `ERR_STATE` is left out, as a package whose header omits it would.
    let table = TOKENS.status_table(
        &[
            codes::ERR_NULL,
            codes::ERR_RANGE,
            codes::ERR_UTF8,
            codes::ERR_PANIC,
        ],
        &[(DOMAIN_FLOOR, "THING_ERR_FULL")],
    );
    assert_eq!(
        table,
        owned(&[
            ("THING_OK", codes::OK),
            ("THING_ERR_NULL", codes::ERR_NULL),
            ("THING_ERR_RANGE", codes::ERR_RANGE),
            ("THING_ERR_UTF8", codes::ERR_UTF8),
            ("THING_ERR_PANIC", codes::ERR_PANIC),
            ("THING_ERR_FULL", DOMAIN_FLOOR),
        ])
    );
}

#[test]
fn a_whole_status_table_names_each_code_once_and_an_unnamed_code_not_at_all() {
    let whole = [
        (codes::OK, "THING_OK"),
        (codes::ERR_NULL, "THING_ERR_NULL"),
        (DOMAIN_FLOOR, "THING_ERR_FULL"),
    ];
    let table = TOKENS.status_table(&[codes::ERR_NULL, -9], &whole);
    assert_eq!(
        table,
        owned(&[
            ("THING_OK", codes::OK),
            ("THING_ERR_NULL", codes::ERR_NULL),
            ("THING_ERR_FULL", DOMAIN_FLOOR),
        ])
    );
}

#[test]
fn report_converts_only_the_error_side() {
    let fine: Result<u8, Failure> = Ok(1);
    assert_eq!(fine.report(&TOKENS).unwrap(), 1);
    let failed: Result<u8, Failure> = Err(Failure {
        code: codes::ERR_STATE,
        name: "ERR_STATE",
    });
    assert_eq!(
        failed.report(&TOKENS).unwrap_err().reason,
        "THING_ERR_STATE: it went wrong"
    );
}

fn big(sign_bit: bool, words: &[u64]) -> BigInt {
    BigInt {
        sign_bit,
        words: words.to_vec(),
    }
}

#[test]
fn every_u64_survives_a_round_trip() {
    for value in [0, 1, 255, u64::from(u32::MAX) + 1, u64::MAX] {
        assert_eq!(TOKENS.to_u64(&from_u64(value), "value").unwrap(), value);
    }
}

#[test]
fn zero_with_no_words_is_zero() {
    // V8 reports `0n` with an empty word list; napi-rs's own `get_u64`
    // indexes the first word.
    assert_eq!(TOKENS.to_u64(&big(false, &[]), "value").unwrap(), 0);
}

#[test]
fn a_negative_value_is_refused() {
    let err = TOKENS.to_u64(&big(true, &[5]), "offset").unwrap_err();
    assert!(
        err.reason
            .starts_with("THING_ERR_BINDING: offset is negative")
    );
}

#[test]
fn a_value_wider_than_64_bits_is_refused() {
    let err = TOKENS.to_u64(&big(false, &[0, 1]), "offset").unwrap_err();
    assert!(
        err.reason
            .starts_with("THING_ERR_BINDING: offset does not fit")
    );
}

#[test]
fn high_zero_words_do_not_count_as_width() {
    assert_eq!(TOKENS.to_u64(&big(false, &[9, 0, 0]), "value").unwrap(), 9);
    // A negative zero with padding words is still zero.
    assert_eq!(TOKENS.to_u64(&big(true, &[0]), "value").unwrap(), 0);
}
