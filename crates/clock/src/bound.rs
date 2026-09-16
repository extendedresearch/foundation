//! What a bound is, where a reading was taken, and the bound of a direct read.

use crate::units::UNBOUNDED;

/// A worst-case interval around a reading.
///
/// A reading `r` with bound `(early_ns, late_ns)` asserts that the true event
/// time lies in `[r - early_ns, r + late_ns]`. It is never a standard
/// deviation, a symmetric half-width or a confidence interval: widths of a
/// difference add linearly (see [`Interval`]), which holds only for worst
/// cases. A calibrated value, such as a photodiode's measured lag, never goes
/// in either field; it travels as a separate record beside the reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bound {
    /// How much earlier than the reading the event may have happened.
    pub early_ns: u64,
    /// How much later than the reading the event may have happened.
    pub late_ns: u64,
    /// Why the bound has this width, and where the reading was taken.
    pub basis: Basis,
}

impl Bound {
    /// Nothing is known: both sides [`UNBOUNDED`], basis
    /// [`Basis::Unspecified`]. A zero bound is a claim; this is its absence.
    pub const UNKNOWN: Bound = Bound {
        early_ns: UNBOUNDED,
        late_ns: UNBOUNDED,
        basis: Basis::Unspecified,
    };

    /// True only when both sides are [`UNBOUNDED`]. The basis is not consulted,
    /// and one unbounded side is not unknown.
    pub fn is_unknown(self) -> bool {
        self.early_ns == UNBOUNDED && self.late_ns == UNBOUNDED
    }
}

/// Which way a clock rounds a true instant to the value it returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// The clock floors to its resolution: the true instant is in
    /// `[r, r + res)`.
    FloorOnly,
    /// The clock floors, then may add one resolution: the true instant is in
    /// `[r - res, r + res)`.
    TwoSided,
}

/// The bound of a direct clock read, set by the clock's rounding direction.
///
/// `Some(q)` with [`Rounding::FloorOnly`] is `(0, q)`, and with
/// [`Rounding::TwoSided`] is `(q, q)`, both with basis [`Basis::ClockRead`].
/// `None` is [`Bound::UNKNOWN`]. A face that cannot establish that its clock
/// floors passes `TwoSided`, whose bound contains the floor-only one.
///
/// ```
/// use extendedresearch_clock::{bound_for_read, Basis, Bound, Rounding};
/// let b = bound_for_read(Some(100_000), Rounding::FloorOnly);
/// assert_eq!((b.early_ns, b.late_ns, b.basis), (0, 100_000, Basis::ClockRead));
/// assert_eq!(bound_for_read(None, Rounding::TwoSided), Bound::UNKNOWN);
/// ```
pub fn bound_for_read(resolution_ns: Option<u64>, rounding: Rounding) -> Bound {
    match resolution_ns {
        None => Bound::UNKNOWN,
        Some(q) => Bound {
            early_ns: match rounding {
                Rounding::FloorOnly => 0,
                Rounding::TwoSided => q,
            },
            late_ns: q,
            basis: Basis::ClockRead,
        },
    }
}

/// Where in the path a reading was taken, which decides the shape of its bound.
///
/// Values 0 to 6 are the `clock.v1` `RxStampPoint` values, value for value, so
/// a reading's basis becomes a recording's receive-stamp point by copying the
/// integer. Values 7 to 10 are provisional: they are proposed as additions to
/// that enumeration, and hold the same integers here until that lands.
/// The set is closed at each release; a new basis is a reviewed change.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Basis {
    /// Not stated.
    Unspecified = 0,
    /// Stamped after a blocking read returned.
    AfterReadReturned = 1,
    /// Stamped inside a platform callback.
    InPlatformCallback = 2,
    /// Stamped by the kernel on the socket.
    KernelSocketTimestamp = 3,
    /// Stamped at an interrupt edge.
    InterruptEdge = 4,
    /// Stamped inside a browser event handler.
    BrowserEventHandler = 5,
    /// Stamped when the data was read from a file.
    FileRead = 6,
    /// A stimulus quantised to a display refresh; the reading names the frame.
    DisplayFrame = 7,
    /// A sound scheduled into an audio output buffer.
    AudioOutputBuffer = 8,
    /// A direct read of this domain's clock, bounded by [`bound_for_read`].
    ClockRead = 9,
    /// An occurrence time the platform supplies with an event, on this
    /// domain's clock.
    PlatformEventTimestamp = 10,
}

/// The difference between two readings on one domain, with its bound.
///
/// For `later.since(earlier)` with bounds `(ea, la)` and `(eb, lb)`, `ns` is
/// `later - earlier`, `below_ns` is `ea + lb` and `above_ns` is `la + eb`,
/// each sum saturating to [`UNBOUNDED`]. The true interval lies in
/// `[ns - below_ns, ns + above_ns]`. The widths add crosswise because the
/// interval is a difference: the later event happening early and the earlier
/// one happening late both shrink it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    /// The difference of the two readings, in nanoseconds.
    pub ns: i64,
    /// The true interval may be up to this much smaller than `ns`.
    pub below_ns: u64,
    /// The true interval may be up to this much larger than `ns`.
    pub above_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_needs_both_sides() {
        assert!(Bound::UNKNOWN.is_unknown());
        let one = |early_ns, late_ns, basis| Bound {
            early_ns,
            late_ns,
            basis,
        };
        assert!(one(UNBOUNDED, UNBOUNDED, Basis::ClockRead).is_unknown());
        assert!(!one(UNBOUNDED, 0, Basis::Unspecified).is_unknown());
        assert!(!one(0, UNBOUNDED, Basis::Unspecified).is_unknown());
        assert!(!one(0, 0, Basis::Unspecified).is_unknown());
    }

    #[test]
    fn a_read_is_bounded_by_its_rounding() {
        let floor = bound_for_read(Some(100_000), Rounding::FloorOnly);
        assert_eq!(
            floor,
            Bound {
                early_ns: 0,
                late_ns: 100_000,
                basis: Basis::ClockRead
            }
        );
        let two = bound_for_read(Some(100_000), Rounding::TwoSided);
        assert_eq!(
            two,
            Bound {
                early_ns: 100_000,
                late_ns: 100_000,
                basis: Basis::ClockRead
            }
        );
        assert_eq!(bound_for_read(None, Rounding::FloorOnly), Bound::UNKNOWN);
        assert_eq!(bound_for_read(None, Rounding::TwoSided), Bound::UNKNOWN);
        assert_eq!(
            bound_for_read(Some(0), Rounding::TwoSided),
            Bound {
                early_ns: 0,
                late_ns: 0,
                basis: Basis::ClockRead
            }
        );
    }

    #[test]
    fn basis_integers_are_the_receive_stamp_points() {
        let all = [
            (Basis::Unspecified, 0),
            (Basis::AfterReadReturned, 1),
            (Basis::InPlatformCallback, 2),
            (Basis::KernelSocketTimestamp, 3),
            (Basis::InterruptEdge, 4),
            (Basis::BrowserEventHandler, 5),
            (Basis::FileRead, 6),
            (Basis::DisplayFrame, 7),
            (Basis::AudioOutputBuffer, 8),
            (Basis::ClockRead, 9),
            (Basis::PlatformEventTimestamp, 10),
        ];
        for (basis, value) in all {
            assert_eq!(basis as u8, value, "{basis:?}");
        }
    }
}
