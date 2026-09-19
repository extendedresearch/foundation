//! Clock readings that carry the domain they were taken on and a worst-case
//! bound on when the event they stamp happened, and the arithmetic over them.
//!
//! # The contract
//!
//! | | |
//! |---|---|
//! | **Units** | Every instant and width is `u64` nanoseconds, and every signed difference `i64`. No public surface takes or returns a float except [`ns_from_ms`], the one conversion a browser face makes. Inside, [`fit_one_way`]'s two spreads are root mean squares in binary64 in a stated order, and nothing else is floating point |
//! | **A bound is a worst case** | A reading `r` with [`Bound`] `(early_ns, late_ns)` asserts the event happened in `[r - early_ns, r + late_ns]`. Never a standard deviation. [`UNBOUNDED`] is "no bound known", and width addition saturates to it |
//! | **Subtraction needs one domain** | Two readings subtract if and only if their [`Domain`]s' `host_id` and `host_clock_epoch` are byte-equal. [`Reading::since`] refuses otherwise |
//! | **A discontinuity ends the domain** | [`DriftCheck`] ends the domain when `wall - monotonic` moves past a [`DriftThreshold`]; the next domain's epoch carries `.e<n>`, so no interval crosses the jump |
//! | **The anchor states its error** | [`Anchor::from_brackets`] keeps the narrowest bracket, and its error comes from the span and both clocks' resolutions; [`wall_at`] maps a reading to the calendar through it |
//! | **A one-way fit states its bias** | [`fit_one_way`] fits a source clock onto a host clock from `(source, receipt)` pairs by min filter and lower-envelope skew. Its bias is [`UNBOUNDED`] below and `0` above, because one-way data cannot separate a constant delay from an offset; [`Line`] maps through the fit in integers |
//!
//! # What is here, and what is not
//!
//! This is the target-independent core: data and arithmetic, no I/O, no
//! platform call, and no `cfg`:
//!
//! - readings, bounds and intervals: [`Reading`], [`Bound`], [`Interval`],
//!   [`bound_for_read`];
//! - domains and their epochs: [`Domain`], [`host_clock_epoch_from`],
//!   [`doc_epoch_from`];
//! - the anchor to the calendar: [`Anchor`], [`wall_at`];
//! - a clock's quantum: [`quantum_from_deltas`];
//! - the discontinuity check: [`DriftCheck`];
//! - a source clock fitted onto a host clock from one-way observations:
//!   [`fit_one_way`] over a slice, [`SlidingWindow`] for a live stream,
//!   [`Subsample`] for a whole session, and the [`Line`] a [`Fit`] maps
//!   through, forward and inverse.
//!
//! It compiles unchanged for every host target
//! and for `wasm32-unknown-unknown`. The faces that read a real clock — a
//! native `host` module and a browser `browser` module — implement [`Clock`]
//! over these types, and are not written yet.
//!
//! This crate defines the clock vocabulary: the field names, and the integers
//! of [`Basis`] and [`SuspendBehaviour`], are its own and part of its contract.
//! Several choices here are provisional until confirmed: the crate name,
//! [`Basis`] values 7 to 10, the browser `host_id` and epoch format
//! ([`doc_epoch_from`]), and [`QUANTUM_TOL_DEN`].
//!
//! # Conformance
//!
//! `vectors/` beside this crate holds language-free JSON vectors, one expected
//! answer per row, for every pure function here. `tests/vectors.rs` runs them
//! against this crate; any other implementation runs the same files.
//!
//! No `unsafe` is written in this crate.

mod anchor;
mod bound;
mod clock;
mod domain;
mod drift;
mod fit;
mod quantum;
mod reading;
mod units;

pub use anchor::{Anchor, WallError, WallTime, wall_at};
pub use bound::{Basis, Bound, Interval, Rounding, bound_for_read};
pub use clock::{Clock, SuppliedClock};
pub use domain::{Domain, DomainId, SuspendBehaviour, doc_epoch_from, host_clock_epoch_from};
pub use drift::{Discontinuity, DriftCheck, DriftThreshold};
pub use fit::{
    DEFAULT_TOLERANCE_NS, DEFAULT_WINDOW_CAPACITY, DEFAULT_WINDOW_NS, Fit, Line,
    MIN_OBSERVATIONS_FOR_SKEW, Observation, SKEW_NOT_ESTIMATED, SlidingWindow, Subsample,
    fit_one_way, tolerance_for_rate,
};
pub use quantum::{
    QUANTUM_FLOOR_NS, QUANTUM_MAX_DIVISOR, QUANTUM_MIN_DELTAS, QUANTUM_SLACK_NS, QUANTUM_TOL_DEN,
    quantum_from_deltas,
};
pub use reading::{Reading, SinceError};
pub use units::{MsError, UNBOUNDED, ns_from_ms};
