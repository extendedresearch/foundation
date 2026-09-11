//! A panic stops at the boundary.
//!
//! **An unwind out of an `extern "C"` function aborts the process**, and the
//! process belongs to somebody else — a Python interpreter with unsaved work, a
//! Unity editor mid-session. A code the binding raises as an exception is worse
//! than no panic and much better than killing the host.
//!
//! The library's state after one is not recoverable, and each library says so
//! in its documentation; this buys an orderly report, not a working library.
//!
//! # A build with `panic = "abort"` catches nothing
//!
//! Under that profile a panic aborts before [`guard`] can see it. A library
//! that wants [`ERR_PANIC`] to mean anything builds its C artefact with the
//! default `panic = "unwind"`.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::codes::ERR_PANIC;

/// Run the body of an exported function, turning a panic into [`ERR_PANIC`].
///
/// ```
/// use extendedresearch_abi::{codes, guard::guard};
///
/// assert_eq!(guard(|| codes::OK), codes::OK);
/// assert_eq!(guard(|| panic!("a bug")), codes::ERR_PANIC);
/// ```
///
/// `AssertUnwindSafe` is sound here because nothing observes the state the body
/// left behind except through a later call, and [`ERR_PANIC`] already tells the
/// caller that state is unknown.
#[must_use]
pub fn guard<F>(body: F) -> i32
where
    F: FnOnce() -> i32,
{
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(ERR_PANIC)
}
