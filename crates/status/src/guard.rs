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
/// use extendedresearch_status::{codes, guard::guard};
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

/// Run the body of an exported function that answers nothing — a `_destroy` —
/// and keep a panic inside it from crossing.
///
/// Answers whether the body finished. A `_destroy` has no channel to report a
/// panic through, so the panic is swallowed; the alternative is an abort of the
/// host, from a finalizer thread, at a time no test reproduces. Dropping a
/// handle's contents runs arbitrary `Drop` code, which is where such a panic
/// comes from.
///
/// ```
/// use extendedresearch_status::guard;
///
/// struct Thing;
///
/// // The body of a library's `extern "C" fn thing_destroy(thing: *mut Thing)`,
/// // once the raw pointer has been turned back into the box that owns it —
/// // which is `extendedresearch_abi::borrow::reclaim`, and the null handle a
/// // `_destroy` tolerates is the `None`.
/// fn destroy(thing: Option<Box<Thing>>) -> bool {
///     guard::contain(|| drop(thing))
/// }
///
/// assert!(destroy(Some(Box::new(Thing))));
/// assert!(destroy(None));
/// assert!(!guard::contain(|| panic!("a Drop that panics")));
/// ```
pub fn contain<F>(body: F) -> bool
where
    F: FnOnce(),
{
    catch_unwind(AssertUnwindSafe(body)).is_ok()
}
