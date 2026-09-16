//! Clock readings that carry the domain they were taken on and a worst-case
//! bound on when the event they stamp happened, and the arithmetic over them.
//!
//! # The contract
//!
//! | | |
//! |---|---|
//! | **Units** | Every instant and width is `u64` nanoseconds, and every signed difference `i64`. No public surface takes or returns a float except [`ns_from_ms`], the one conversion a browser face makes |
//! | **A bound is a worst case** | A reading `r` with [`Bound`] `(early_ns, late_ns)` asserts the event happened in `[r - early_ns, r + late_ns]`. Never a standard deviation. [`UNBOUNDED`] is "no bound known", and width addition saturates to it |
//! | **Subtraction needs one domain** | Two readings subtract if and only if their [`Domain`]s' `host_id` and `host_clock_epoch` are byte-equal. [`Reading::since`] refuses otherwise |
//! | **A discontinuity ends the domain** | [`DriftCheck`] ends the domain when `wall - monotonic` moves past a [`DriftThreshold`]; the next domain's epoch carries `.e<n>`, so no interval crosses the jump |
//! | **The anchor states its error** | [`Anchor::from_brackets`] keeps the narrowest bracket, and its error comes from the span and both clocks' resolutions; [`wall_at`] maps a reading to the calendar through it |
//!
//! # What is here, and what is not
//!
//! This is the target-independent core: data and arithmetic, no I/O, no
//! platform call, and no `cfg`. It compiles unchanged for every host target
//! and for `wasm32-unknown-unknown`. The faces that read a real clock — a
//! native `host` module and a browser `browser` module — implement [`Clock`]
//! over these types, and are not written yet.
//!
//! The field names follow the `clock.v1` schema, so a [`Domain`], [`Anchor`],
//! [`Basis`] or [`SuspendBehaviour`] maps onto a recording's declaration field
//! by field. Several choices here are provisional until confirmed: the crate
//! name, [`Basis`] values 7 to 10, the browser `host_id` and epoch format
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
mod quantum;
mod reading;
mod units;

pub use anchor::{Anchor, WallError, WallTime, wall_at};
pub use bound::{Basis, Bound, Interval, Rounding, bound_for_read};
pub use clock::{Clock, SuppliedClock};
pub use domain::{Domain, DomainId, SuspendBehaviour, doc_epoch_from, host_clock_epoch_from};
pub use drift::{Discontinuity, DriftCheck, DriftThreshold};
pub use quantum::{
    QUANTUM_FLOOR_NS, QUANTUM_MAX_DIVISOR, QUANTUM_MIN_DELTAS, QUANTUM_SLACK_NS, QUANTUM_TOL_DEN,
    quantum_from_deltas,
};
pub use reading::{Reading, SinceError};
pub use units::{MsError, UNBOUNDED, ns_from_ms};
