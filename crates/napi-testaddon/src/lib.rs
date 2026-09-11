//! A consumer of `extendedresearch-napi`, built to be loaded by Node in
//! foundation's tests and nowhere else.
//!
//! It stands in for a package binding: a small "core" with an error type that
//! implements [`AbiError`] and two enumerations, and the `#[napi]` surface a
//! binding writes over it with the shared macros and [`Tokens`].

use std::fmt;

use extendedresearch_abi::codes::{self, AbiError, DOMAIN_FLOOR};
use extendedresearch_abi::enumeration::Enumeration;
use extendedresearch_napi::{Report, Tokens, from_u64};
use napi::bindgen_prelude::BigInt;
use napi_derive::napi;

static TOKENS: Tokens = Tokens::new("TEST");

/// The one domain code.
pub const TEST_ERR_FULL: i32 = DOMAIN_FLOOR;

/// Every domain code, as a package declares them.
pub const ERROR_CODES: &[(i32, &str)] = &[(TEST_ERR_FULL, "TEST_ERR_FULL")];

/// An enumeration whose names share a prefix.
pub static ORIGINS: Enumeration = Enumeration::new(&[
    (0, "ORIGIN_UNSPECIFIED"),
    (1, "ORIGIN_RAW"),
    (2, "ORIGIN_DERIVED"),
]);

/// An enumeration with a member whose short name would start with a digit.
pub static RATES: Enumeration = Enumeration::new(&[(0, "RATE_UNSPECIFIED"), (50, "RATE_50HZ")]);

extendedresearch_napi::status_exports!(TOKENS, ERROR_CODES);
extendedresearch_napi::abi_version_exports!(3, 3);
extendedresearch_napi::enumeration_exports! {
    /// What a stream is for.
    origins => ORIGINS;
    /// A sampling rate.
    rates => RATES;
}

/// The core's error type.
enum CoreError {
    Utf8,
    State,
    Full(u32),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8 => f.write_str("the text was not UTF-8"),
            Self::State => f.write_str("the counter is closed"),
            Self::Full(at) => write!(f, "the counter is full at {at}"),
        }
    }
}

impl AbiError for CoreError {
    fn code(&self) -> i32 {
        match self {
            Self::Utf8 => codes::ERR_UTF8,
            Self::State => codes::ERR_STATE,
            Self::Full(_) => TEST_ERR_FULL,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Utf8 => "ERR_UTF8",
            Self::State => "ERR_STATE",
            Self::Full(_) => "TEST_ERR_FULL",
        }
    }
}

/// Fail with the core error named by `which`, or succeed for anything else.
#[napi]
pub fn fail(which: String) -> napi::Result<()> {
    let outcome = match which.as_str() {
        "utf8" => Err(CoreError::Utf8),
        "state" => Err(CoreError::State),
        "full" => Err(CoreError::Full(3)),
        _ => Ok(()),
    };
    outcome.report(&TOKENS)
}

/// Fail as the binding rather than the core.
#[napi]
pub fn binding_failure() -> napi::Result<()> {
    Err(TOKENS.binding_failure("the binding could not do that"))
}

/// A `BigInt` through `u64` and back.
#[napi]
pub fn round_trip(value: BigInt) -> napi::Result<BigInt> {
    Ok(from_u64(TOKENS.to_u64(&value, "value")?))
}

/// A class, so the prototype walk in `harden.ts` has methods and a getter.
#[napi]
pub struct Counter {
    count: u32,
}

#[napi]
impl Counter {
    /// A counter at zero.
    #[napi(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// Count one, failing once the count reaches three.
    #[napi]
    pub fn increment(&mut self) -> napi::Result<u32> {
        if self.count >= 3 {
            return Err(CoreError::Full(self.count)).report(&TOKENS);
        }
        self.count += 1;
        Ok(self.count)
    }

    /// Always fails, so a getter's translation is visible.
    #[napi(getter)]
    pub fn closed(&self) -> napi::Result<u32> {
        Err(CoreError::State).report(&TOKENS)
    }
}
