//! Enumerations a binding loops over rather than transcribes.
//!
//! An enumerated value crosses the boundary as a named `int32_t` constant, and
//! beside the constants a library exports three functions, so a binding builds
//! its table from the library's rather than from its author's memory:
//!
//! ```c
//! int32_t thing_count(uint32_t *out_count);
//! int32_t thing_at(uint32_t index, int32_t *out_value);
//! int32_t thing_name(int32_t value, char *destination, uint64_t capacity, uint64_t *out_len);
//! ```
//!
//! **A binding that loops cannot produce a subset.** A binding that transcribes
//! constants by hand can, and the failure is silent: two clients written by
//! hand against one contract each declared four of its six refusal reasons and
//! stopped at the same missing value, so a refusal for that reason read as
//! no reason at all. `_count` and `_at` let a binding generate its constants from
//! the library when it loads; `_name` gives a report the contract's own words.
//!
//! # Keep an exhaustive `match` beside the table
//!
//! A table is a list, and a list does not fail the build when the contract
//! gains a value. Write the conversion from the contract's own type to the ABI
//! value as a `match` with no wildcard arm, and build the table's values from
//! it: a new value in the contract is then a compile error in the library,
//! rather than a value every binding above it silently lacks. The library's
//! test compares [`conformance::enumeration`](crate::conformance::enumeration)'s
//! answer against the contract in both directions.
//!
//! # What this is not for
//!
//! A per-handle collection — the bindings a joined node holds, say — is a
//! `_count(handle, …)` and `_at(handle, index, …)` over data, not an
//! enumeration of constants. [`crate::borrow`] and [`crate::buffer`] cover it.
//!
//! # The rules
//!
//! Past the end is [`ERR_RANGE`]. A value the table does not hold has no name,
//! and asking for one is [`ERR_RANGE`] too. A null out-parameter is [`ERR_NULL`],
//! and is checked before the index. `_name` follows [`crate::buffer`]'s shape.
//!
//! [`Enumeration`] holds the table. The raw-pointer forms a library's exported
//! functions call are [`borrow::enumeration_count`], [`borrow::enumeration_at`]
//! and [`borrow::enumeration_name`], and
//! [`conformance::enumeration`](crate::conformance::enumeration) checks all three.
//!
//! [`borrow::enumeration_count`]: crate::borrow::enumeration_count
//! [`borrow::enumeration_at`]: crate::borrow::enumeration_at
//! [`borrow::enumeration_name`]: crate::borrow::enumeration_name

use std::mem::MaybeUninit;

use crate::codes::{ERR_NULL, ERR_RANGE, OK};

/// One enumeration: each value with the contract's name for it, in the order
/// `_at` answers them.
///
/// ```
/// use extendedresearch_abi::enumeration::Enumeration;
///
/// pub const STOP_UNSPECIFIED: i32 = 0;
/// pub const STOP_PEER_FAILED: i32 = 2;
///
/// static STOP_REASONS: Enumeration = Enumeration::new(&[
///     (STOP_UNSPECIFIED, "STOP_REASON_UNSPECIFIED"),
///     (STOP_PEER_FAILED, "STOP_REASON_PEER_FAILED"),
/// ]);
///
/// assert_eq!(STOP_REASONS.name_of(2), Some("STOP_REASON_PEER_FAILED"));
/// assert_eq!(STOP_REASONS.name_of(1), None);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Enumeration {
    entries: &'static [(i32, &'static str)],
}

impl Enumeration {
    /// A table of `(value, name)` pairs.
    ///
    /// # Panics
    ///
    /// When the table is empty, or two entries share a value or a name. In a
    /// `static` or `const` that is a compile error rather than a panic:
    ///
    /// ```compile_fail
    /// use extendedresearch_abi::enumeration::Enumeration;
    /// static TWICE: Enumeration = Enumeration::new(&[(0, "A"), (0, "B")]);
    /// ```
    #[must_use]
    pub const fn new(entries: &'static [(i32, &'static str)]) -> Self {
        assert!(
            !entries.is_empty(),
            "an enumeration must hold at least one value"
        );
        let mut rest = entries;
        while let Some(((value, name), later)) = rest.split_first() {
            let mut others = later;
            while let Some(((other_value, other_name), after)) = others.split_first() {
                assert!(*value != *other_value, "two entries share a value");
                assert!(
                    !same_bytes(name.as_bytes(), other_name.as_bytes()),
                    "two entries share a name"
                );
                others = after;
            }
            rest = later;
        }
        Self { entries }
    }

    /// Every `(value, name)` pair, in `_at` order.
    #[must_use]
    pub const fn entries(&self) -> &'static [(i32, &'static str)] {
        self.entries
    }

    /// The contract's name for `value`, or `None` when the table does not hold
    /// it.
    #[must_use]
    pub fn name_of(&self, value: i32) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(candidate, _)| *candidate == value)
            .map(|(_, name)| *name)
    }

    /// `_count`: how many values the table holds.
    #[must_use]
    pub fn count(&self, out_count: Option<&mut MaybeUninit<u32>>) -> i32 {
        let Some(slot) = out_count else {
            return ERR_NULL;
        };
        let Ok(count) = u32::try_from(self.entries.len()) else {
            return ERR_RANGE;
        };
        slot.write(count);
        OK
    }

    /// `_at`: the value at `index`, or [`ERR_RANGE`] past the end.
    #[must_use]
    pub fn at(&self, index: u32, out_value: Option<&mut MaybeUninit<i32>>) -> i32 {
        let Some(slot) = out_value else {
            return ERR_NULL;
        };
        let Some((value, _)) = usize::try_from(index)
            .ok()
            .and_then(|index| self.entries.get(index))
        else {
            return ERR_RANGE;
        };
        slot.write(*value);
        OK
    }
}

const fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let (mut a, mut b) = (a, b);
    while let (Some((x, a_rest)), Some((y, b_rest))) = (a.split_first(), b.split_first()) {
        if *x != *y {
            return false;
        }
        a = a_rest;
        b = b_rest;
    }
    true
}
