//! The codes a caller checks, and the range a library numbers its own in.
//!
//! # Zero is success, negative is failure
//!
//! `if (rc < 0)` is the whole of a caller's check, and a code it has never
//! heard of still reads as a failure. That is what lets a library add a code
//! without every binding needing to know it first.
//!
//! # Named `i32` constants, not a C `enum`
//!
//! The width of a C enum is implementation-defined. A binding that declared one
//! as 32 bits against a library that compiled it as something else would
//! misread every code — including the ones that mean "your pointer was null".
//! So every code here, and every enumerated *parameter* a library declares
//! beside them, is an `int32_t` with constants next to it.
//!
//! # The split this module exists to draw
//!
//! **Codes from `-1` down to one above [`DOMAIN_FLOOR`] belong to the
//! boundary.** They are the
//! failures any library has, whatever it does: a null handle, a buffer too
//! small, text that is not UTF-8, a panic that must not cross. A binding can
//! translate all of them once, in one place, and reuse that translation against
//! every library in this ecosystem.
//!
//! **Codes at and below [`DOMAIN_FLOOR`] belong to the library.** A refusal, a
//! protocol violation, a timeout — these mean nothing without the domain, and
//! a shared crate that tried to name them would be guessing.
//!
//! The split is what makes a binding's error translation reusable. Without it
//! every library numbers from `-1` and a binding has to know which library it
//! is talking to before it can read a code, which is the situation this
//! ecosystem was in when this crate was written.

/// The call succeeded.
///
/// Zero, and it is the only non-negative value any fallible function answers.
pub const OK: i32 = 0;

/// A required handle or out-pointer was null.
///
/// Null where one is required is this rather than a crash. A handle that was
/// *already destroyed* is undefined and is the one error a boundary cannot
/// detect — see [`crate::borrow`].
pub const ERR_NULL: i32 = -1;

/// A caller's buffer was too small, or an index or length was past what the call
/// can reach.
///
/// For a buffer, answered with `*out_len` still set to what was needed, so a
/// caller that guessed low allocates exactly and retries. See [`crate::buffer`].
pub const ERR_RANGE: i32 = -2;

/// Text crossing the boundary was not valid UTF-8.
///
/// A refusal rather than a lossy conversion: a name that silently became a
/// different string is worse than a call that failed.
pub const ERR_UTF8: i32 = -3;

/// A Rust panic was caught at the boundary and not allowed to cross.
///
/// Unwinding into C is undefined. A library catches its own panics and answers
/// this, so a bug is a failed call rather than a corrupted process.
pub const ERR_PANIC: i32 = -4;

/// The call was well-formed but the object is in the wrong state for it.
///
/// Reading from a closed subscription, writing to a finished document. Distinct
/// from [`ERR_NULL`] because the handle was valid, and from a domain refusal
/// because no domain rule was consulted.
pub const ERR_STATE: i32 = -5;

/// The most negative code the boundary reserves for itself.
///
/// A library numbers its own codes at this value and below. Nothing here will
/// ever be added between `-1` and this, so a binding's translation of the
/// boundary codes stays correct as this crate grows.
///
/// The gap between [`ERR_STATE`] and this is deliberate headroom.
pub const DOMAIN_FLOOR: i32 = -16;

/// Whether a code is one this crate defines.
///
/// A binding uses it to decide whether to translate a code itself or hand it to
/// the library-specific table.
#[must_use]
pub const fn is_boundary(code: i32) -> bool {
    code < 0 && code > DOMAIN_FLOOR
}

/// Whether a code belongs to the library rather than the boundary.
#[must_use]
pub const fn is_domain(code: i32) -> bool {
    code <= DOMAIN_FLOOR
}

/// The name of a boundary code, for a binding's error message.
///
/// `None` for [`OK`] and for any domain code, because neither is this crate's
/// to name.
#[must_use]
pub const fn name(code: i32) -> Option<&'static str> {
    match code {
        ERR_NULL => Some("ERR_NULL"),
        ERR_RANGE => Some("ERR_RANGE"),
        ERR_UTF8 => Some("ERR_UTF8"),
        ERR_PANIC => Some("ERR_PANIC"),
        ERR_STATE => Some("ERR_STATE"),
        _ => None,
    }
}

/// A constant's name as a package's header spells it.
///
/// A boundary name, as [`name`] answers it, gains the package's prefix:
/// `ERR_NULL` becomes `EXAMPLE_ERR_NULL`. A name that already carries the
/// prefix and an underscore, as a domain name does, comes back unchanged. So
/// the step applies to any name [`AbiError::name`] answers, and applying it
/// twice changes nothing. Every binding layer spells a failure with it.
///
/// ```
/// use extendedresearch_abi::codes::token;
///
/// assert_eq!(token("EXAMPLE", "ERR_NULL"), "EXAMPLE_ERR_NULL");
/// assert_eq!(token("EXAMPLE", "EXAMPLE_ERR_TRUNCATED"), "EXAMPLE_ERR_TRUNCATED");
/// assert_eq!(token("EXAMPLE", "OK"), "EXAMPLE_OK");
/// // A name that merely starts with the same letters is not the prefix.
/// assert_eq!(token("EXAMPLE", "EXAMPLE0_ERR_X"), "EXAMPLE_EXAMPLE0_ERR_X");
/// ```
#[must_use]
pub fn token(prefix: &str, name: &str) -> String {
    match name.strip_prefix(prefix) {
        Some(rest) if rest.starts_with('_') => name.to_owned(),
        _ => format!("{prefix}_{name}"),
    }
}

/// The contract a package's own error type implements, so every binding layer
/// above it reads one shape.
///
/// A package's core is safe Rust that answers `Result<T, E>`. Its Python and
/// Node bindings call the core directly and turn `E` into their language's
/// error; its C adapter turns `E` into the `int32_t` a C caller reads. All
/// three need the same two facts from `E`, and this trait is where they come
/// from:
///
/// - [`code`](Self::code) is the value the C ABI answers. A boundary failure
///   answers this module's constant ([`ERR_NULL`], [`ERR_UTF8`], …). A domain
///   failure answers the package's own constant, at or below [`DOMAIN_FLOOR`].
///   It is always negative: an error that answered zero would read as success.
/// - [`name`](Self::name) is that constant's name: `ERR_NULL` for a boundary
///   code (the name [`name`] answers, which every library shares), and the
///   package's own name, such as `EXAMPLE_ERR_TRUNCATED`, for a domain code.
///   [`token`] turns either into the header's spelling (`EXAMPLE_ERR_NULL`),
///   and every binding layer reports that.
///
/// `Display` is the sentence a person reads. It says what happened; the code
/// and the name say which failure it was, and a caller branches on those.
///
/// [`conformance::error_codes`](crate::conformance::error_codes) checks a
/// package's declared domain codes, and
/// [`conformance::errors`](crate::conformance::errors) checks its error values
/// against them.
///
/// ```
/// use std::fmt;
/// use extendedresearch_abi::codes::{self, AbiError};
///
/// pub const THING_ERR_REFUSED: i32 = -16;
///
/// #[derive(Debug)]
/// pub enum ThingError {
///     NotUtf8,
///     Refused(String),
/// }
///
/// impl fmt::Display for ThingError {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         match self {
///             Self::NotUtf8 => f.write_str("the name was not UTF-8"),
///             Self::Refused(why) => write!(f, "refused: {why}"),
///         }
///     }
/// }
///
/// impl AbiError for ThingError {
///     fn code(&self) -> i32 {
///         match self {
///             Self::NotUtf8 => codes::ERR_UTF8,
///             Self::Refused(_) => THING_ERR_REFUSED,
///         }
///     }
///     fn name(&self) -> &'static str {
///         match self {
///             Self::NotUtf8 => "ERR_UTF8",
///             Self::Refused(_) => "THING_ERR_REFUSED",
///         }
///     }
/// }
///
/// assert_eq!(ThingError::Refused("full".into()).code(), -16);
/// ```
pub trait AbiError: std::fmt::Display {
    /// The negative `int32_t` the C ABI answers for this failure.
    fn code(&self) -> i32;

    /// The name of the constant [`code`](Self::code) is: unprefixed for a
    /// boundary code, the package's full name for a domain code. [`token`]
    /// gives the header's spelling of either.
    fn name(&self) -> &'static str;
}

/// Turn a core call's `Result` into the status a C adapter answers.
///
/// `Ok` hands the value to `deliver`, which writes it to the caller's
/// out-parameter and answers a status of its own — [`OK`], or the code a
/// copy-out call answered. `Err` answers the error's [`AbiError::code`].
///
/// ```
/// use extendedresearch_abi::codes::{self, AbiError};
/// # use std::fmt;
/// # struct Full;
/// # impl fmt::Display for Full {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("full") }
/// # }
/// # impl AbiError for Full {
/// #     fn code(&self) -> i32 { -16 }
/// #     fn name(&self) -> &'static str { "THING_ERR_FULL" }
/// # }
///
/// let mut slot = 0u32;
/// assert_eq!(codes::status(Ok::<u32, Full>(7), |v| { slot = v; codes::OK }), codes::OK);
/// assert_eq!(slot, 7);
/// assert_eq!(codes::status(Err::<u32, Full>(Full), |_| codes::OK), -16);
/// ```
///
/// **An error whose code is not negative answers [`ERR_STATE`]**, so a defect
/// in an error type cannot read as success to `if (rc < 0)`.
/// [`conformance::errors`](crate::conformance::errors) is the test that finds
/// such a type before a caller does.
#[must_use]
pub fn status<T, E, F>(result: Result<T, E>, deliver: F) -> i32
where
    E: AbiError,
    F: FnOnce(T) -> i32,
{
    match result {
        Ok(value) => deliver(value),
        Err(error) => match error.code() {
            code if code < 0 => code,
            _ => ERR_STATE,
        },
    }
}

/// What a boundary code means, as a sentence a binding can put in an error.
///
/// `None` for [`OK`] and for any domain code, for the reason [`name`] gives.
#[must_use]
pub const fn describe(code: i32) -> Option<&'static str> {
    match code {
        ERR_NULL => Some("a required handle or out-pointer was null"),
        ERR_RANGE => {
            Some("a buffer was too small, or an index or length was past what the call can reach")
        }
        ERR_UTF8 => Some("text passed to the library was not valid UTF-8"),
        ERR_PANIC => Some(
            "the library panicked and caught it; its state is unknown and it should not be used further",
        ),
        ERR_STATE => Some("the object is in the wrong state for this call"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_the_only_non_negative() {
        assert_eq!(OK, 0);
        for code in [ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC, ERR_STATE] {
            assert!(code < 0, "{code} should read as a failure to `rc < 0`");
        }
    }

    #[test]
    fn every_boundary_code_is_inside_the_reserved_range() {
        for code in [ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC, ERR_STATE] {
            assert!(
                is_boundary(code),
                "{code} is outside the range a binding translates as a boundary failure",
            );
            assert!(!is_domain(code));
        }
    }

    #[test]
    fn the_floor_itself_belongs_to_the_library() {
        // The first code a library may take is the floor, not one past it.
        assert!(is_domain(DOMAIN_FLOOR));
        assert!(!is_boundary(DOMAIN_FLOOR));
        assert!(is_domain(DOMAIN_FLOOR - 1));
    }

    #[test]
    fn success_is_neither() {
        assert!(!is_boundary(OK));
        assert!(!is_domain(OK));
        assert_eq!(name(OK), None);
    }

    #[test]
    fn every_boundary_code_can_be_named_and_no_domain_code_can() {
        for code in [ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC, ERR_STATE] {
            assert!(name(code).is_some(), "{code} has no name to report");
        }
        assert_eq!(name(DOMAIN_FLOOR), None);
        assert_eq!(name(-999), None);
    }

    #[test]
    fn every_boundary_code_is_described_and_nothing_else_is() {
        for code in [ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC, ERR_STATE] {
            assert!(describe(code).is_some(), "{code} has no description");
        }
        assert_eq!(describe(OK), None);
        assert_eq!(describe(DOMAIN_FLOOR), None);
    }

    struct Failing(i32);

    impl std::fmt::Display for Failing {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "failed with {}", self.0)
        }
    }

    impl AbiError for Failing {
        fn code(&self) -> i32 {
            self.0
        }
        fn name(&self) -> &'static str {
            "TEST_ERR"
        }
    }

    #[test]
    fn status_delivers_a_value_and_answers_what_delivery_answered() {
        let mut slot = None;
        let answered = status(Ok::<_, Failing>(5u8), |value| {
            slot = Some(value);
            ERR_RANGE
        });
        assert_eq!(slot, Some(5));
        assert_eq!(answered, ERR_RANGE, "a copy-out refusal must pass through");
    }

    #[test]
    fn status_answers_an_error_s_own_code() {
        assert_eq!(status(Err::<(), _>(Failing(ERR_UTF8)), |()| OK), ERR_UTF8);
        assert_eq!(status(Err::<(), _>(Failing(-40)), |()| OK), -40);
    }

    #[test]
    fn status_never_answers_success_for_an_error() {
        for code in [OK, 1, i32::MAX] {
            assert_eq!(status(Err::<(), _>(Failing(code)), |()| OK), ERR_STATE);
        }
    }

    #[test]
    fn the_codes_are_distinct() {
        let all = [ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC, ERR_STATE];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "two boundary codes share a value");
            }
        }
    }
}
