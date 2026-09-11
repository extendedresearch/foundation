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
