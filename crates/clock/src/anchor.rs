//! Where a domain's timeline meets the calendar, and the error of that meeting.

use std::fmt;

use crate::bound::{Basis, Bound, Rounding, bound_for_read};
use crate::domain::Domain;
use crate::reading::Reading;
use crate::units::widths;

/// A monotonic reading and a wall reading taken as close together as the
/// platform allows, taken as a whole or not at all.
///
/// The wall read `W` is bracketed by two monotonic reads `m1` and `m2`.
/// `read_span_ns` is `m2 - m1` and `monotonic_ns` is the midpoint
/// `m1 + ⌊(m2 - m1)/2⌋`, so the fields map one to one onto `clock.v1`'s wall
/// clock anchor. `wall_unix_ns` is never 0 in an anchor
/// [`Anchor::from_brackets`] builds, because 0 means no anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    /// Midpoint of the bracketing monotonic reads, floored.
    pub monotonic_ns: u64,
    /// The wall read, in nanoseconds since the Unix epoch.
    pub wall_unix_ns: u64,
    /// Bare identifier of the wall-clock call, such as `CLOCK_REALTIME`,
    /// `GetSystemTimePreciseAsFileTime` or `Date.now`.
    pub wall_source: &'static str,
    /// The second bracketing monotonic read minus the first.
    pub read_span_ns: u64,
    /// Resolution of the wall read. `None` means not known.
    pub wall_resolution_ns: Option<u64>,
    /// The wall read's rounding direction. Every wall source named above
    /// floors.
    pub wall_rounding: Rounding,
}

/// The error of a midpoint anchor with span `s`, placed on its wall reading.
///
/// With the monotonic leg bounded `(em, lm)` and the wall leg `(ew, lw)`, the
/// calendar time at the midpoint lies in
/// `[W - (⌈s/2⌉ + lm + ew), W + (⌊s/2⌋ + em + lw)]`, each sum saturating.
/// Either leg unknown makes the whole error [`Bound::UNKNOWN`]; otherwise the
/// basis is [`Basis::ClockRead`].
///
/// Derivation: the true instants satisfy `t1 ≤ tw ≤ t2`, with
/// `t1 ≥ m1 - em` and `t2 ≤ m2 + lm`, so `M - tw` lies in
/// `[-⌈s/2⌉ - lm, ⌊s/2⌋ + em]`; adding the wall leg's range gives the above.
/// It assumes no wall step and no rate difference inside the bracket.
pub(crate) fn anchor_error(read_span_ns: u64, monotonic: Bound, wall: Bound) -> Bound {
    if monotonic.is_unknown() || wall.is_unknown() {
        return Bound::UNKNOWN;
    }
    let half_up = read_span_ns / 2 + read_span_ns % 2;
    let half_down = read_span_ns / 2;
    Bound {
        early_ns: widths(widths(half_up, monotonic.late_ns), wall.early_ns),
        late_ns: widths(widths(half_down, monotonic.early_ns), wall.late_ns),
        basis: Basis::ClockRead,
    }
}

impl Anchor {
    /// The error of this anchor, given the bound of its monotonic leg.
    pub(crate) fn error(&self, monotonic: Bound) -> Bound {
        anchor_error(
            self.read_span_ns,
            monotonic,
            bound_for_read(self.wall_resolution_ns, self.wall_rounding),
        )
    }

    /// Builds an anchor from `(m1, wall_unix_ns, m2)` brackets and states its
    /// error.
    ///
    /// A bracket whose wall read is 0, or whose `m2` is less than its `m1`, is
    /// discarded. Of the rest, the one with the smallest `m2 - m1` is kept, the
    /// first on a tie. The anchor's `monotonic_ns` is that bracket's midpoint
    /// and the other fields are as passed.
    ///
    /// The [`Bound`] is the anchor's error, placed on `wall_unix_ns`: the
    /// calendar time at `monotonic_ns` lies in
    /// `[wall_unix_ns - early_ns, wall_unix_ns + late_ns]`. The monotonic leg is
    /// `bound_for_read(monotonic_resolution_ns, monotonic)` and the wall leg is
    /// `bound_for_read(wall_resolution_ns, wall_rounding)`; with legs `(em, lm)`
    /// and `(ew, lw)` and span `s`, early is `⌈s/2⌉ + lm + ew` and late is
    /// `⌊s/2⌋ + em + lw`, each saturating. A `None` resolution on either leg
    /// gives [`Bound::UNKNOWN`]; otherwise the basis is [`Basis::ClockRead`].
    /// A span of 0 is not an error of 0.
    ///
    /// ```
    /// use extendedresearch_clock::{Anchor, Rounding};
    /// let (anchor, error) = Anchor::from_brackets(
    ///     &[(999_999_000, 1_700_000_000_000_000_000, 1_000_001_001)],
    ///     Some(100), Rounding::FloorOnly,
    ///     "CLOCK_REALTIME", Some(100), Rounding::FloorOnly,
    /// ).unwrap();
    /// assert_eq!((anchor.monotonic_ns, anchor.read_span_ns), (1_000_000_000, 2001));
    /// assert_eq!((error.early_ns, error.late_ns), (1101, 1100));
    /// ```
    ///
    /// `None` when no bracket is left.
    pub fn from_brackets(
        brackets: &[(u64, u64, u64)],
        monotonic_resolution_ns: Option<u64>,
        monotonic: Rounding,
        wall_source: &'static str,
        wall_resolution_ns: Option<u64>,
        wall_rounding: Rounding,
    ) -> Option<(Anchor, Bound)> {
        let mut kept: Option<(u64, u64, u64)> = None;
        for &(m1, wall, m2) in brackets {
            if wall == 0 || m2 < m1 {
                continue;
            }
            let span = m2 - m1;
            // Strictly smaller replaces, so the first of equal spans stays.
            if kept.is_none_or(|(_, _, kept_span)| span < kept_span) {
                kept = Some((m1, wall, span));
            }
        }
        let (m1, wall_unix_ns, read_span_ns) = kept?;
        let anchor = Anchor {
            monotonic_ns: m1 + read_span_ns / 2,
            wall_unix_ns,
            wall_source,
            read_span_ns,
            wall_resolution_ns,
            wall_rounding,
        };
        let error = anchor.error(bound_for_read(monotonic_resolution_ns, monotonic));
        Some((anchor, error))
    }
}

/// A calendar time and a worst-case bound on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallTime {
    /// Nanoseconds since the Unix epoch.
    pub unix_ns: u64,
    /// How much earlier the event's calendar time may be.
    pub early_ns: u64,
    /// How much later the event's calendar time may be.
    pub late_ns: u64,
}

/// Why [`wall_at`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallError {
    /// The reading is not on the domain passed.
    DomainMismatch,
    /// The domain has no anchor.
    NoAnchor,
    /// The calendar time is before the Unix epoch or past `u64::MAX`.
    Overflow,
}

impl fmt::Display for WallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WallError::DomainMismatch => "the reading is not on this clock domain",
            WallError::NoAnchor => "the clock domain has no anchor",
            WallError::Overflow => "the calendar time does not fit u64 nanoseconds",
        })
    }
}

impl std::error::Error for WallError {}

/// The calendar time of the event `reading` stamps, through `domain.anchor`.
///
/// For a reading `r` bounded `(er, lr)` on a domain whose anchor has midpoint
/// `M` and wall read `W`, `unix_ns` is `W + (r - M)`, `early_ns` is `er` plus
/// the anchor's early error, and `late_ns` is `lr` plus the anchor's late
/// error. The widths add on the same side, because a mapped time is a sum and
/// not a difference. The anchor's monotonic leg is
/// `bound_for_read(domain.resolution_ns, monotonic)` and its wall leg is
/// `bound_for_read(anchor.wall_resolution_ns, anchor.wall_rounding)`.
///
/// The bound leaves out any wall step and any rate difference between the
/// wall and monotonic clocks between the anchor and the reading; both grow
/// with `|r - M|`, so use the most recent anchor.
///
/// # Errors
///
/// In this order: [`WallError::DomainMismatch`] when `reading` is not on
/// `domain`, [`WallError::NoAnchor`] when `domain.anchor` is `None`, and
/// [`WallError::Overflow`] when `unix_ns` is negative or does not fit `u64`.
pub fn wall_at(
    domain: &Domain,
    reading: Reading,
    monotonic: Rounding,
) -> Result<WallTime, WallError> {
    if reading.domain() != domain.id() {
        return Err(WallError::DomainMismatch);
    }
    let anchor = domain.anchor.ok_or(WallError::NoAnchor)?;
    let unix = i128::from(anchor.wall_unix_ns) + i128::from(reading.as_ns())
        - i128::from(anchor.monotonic_ns);
    let unix_ns = u64::try_from(unix).map_err(|_| WallError::Overflow)?;
    let error = anchor.error(bound_for_read(domain.resolution_ns, monotonic));
    let bound = reading.bound();
    Ok(WallTime {
        unix_ns,
        early_ns: widths(bound.early_ns, error.early_ns),
        late_ns: widths(bound.late_ns, error.late_ns),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::domain::SuspendBehaviour;
    use crate::units::UNBOUNDED;

    const W: u64 = 1_700_000_000_000_000_000;

    fn floor(q: Option<u64>) -> Bound {
        bound_for_read(q, Rounding::FloorOnly)
    }

    fn pair(b: Bound) -> (u64, u64) {
        (b.early_ns, b.late_ns)
    }

    #[test]
    fn the_error_uses_the_span_and_both_legs() {
        let two = |q| bound_for_read(Some(q), Rounding::TwoSided);
        assert_eq!(
            pair(anchor_error(2001, floor(Some(100)), floor(Some(100)))),
            (1101, 1100)
        );
        // macOS worked example: 42 ns mach tick, 1 µs wall, span 84 ns.
        assert_eq!(
            pair(anchor_error(84, floor(Some(42)), floor(Some(1000)))),
            (84, 1042)
        );
        // A two-sided monotonic leg adds q to the late side too.
        assert_eq!(
            pair(anchor_error(10, two(7), floor(Some(3)))),
            (5 + 7, 5 + 7 + 3)
        );
        assert_eq!(
            pair(anchor_error(10, floor(Some(7)), two(3))),
            (5 + 7 + 3, 5 + 3)
        );
        assert_eq!(
            pair(anchor_error(0, floor(Some(0)), floor(Some(0)))),
            (0, 0)
        );
        assert_eq!(
            anchor_error(0, floor(Some(0)), floor(Some(0))).basis,
            Basis::ClockRead
        );
        assert_eq!(anchor_error(1, floor(None), floor(Some(1))), Bound::UNKNOWN);
        assert_eq!(anchor_error(1, floor(Some(1)), floor(None)), Bound::UNKNOWN);
        assert_eq!(
            pair(anchor_error(2, floor(Some(u64::MAX - 1)), floor(Some(5)))),
            (UNBOUNDED, 6)
        );
    }

    #[test]
    fn the_narrowest_bracket_is_kept_and_the_first_on_a_tie() {
        let brackets = [
            (1_000_000_000, W, 1_000_003_000),
            (2_000_000_000, W + 1_000_000_000, 2_000_002_001),
            (3_000_000_000, W + 2_000_000_000, 3_000_002_001),
        ];
        let (anchor, _) = Anchor::from_brackets(
            &brackets,
            Some(100),
            Rounding::FloorOnly,
            "CLOCK_REALTIME",
            Some(100),
            Rounding::FloorOnly,
        )
        .unwrap();
        assert_eq!(anchor.monotonic_ns, 2_000_001_000);
        assert_eq!(anchor.wall_unix_ns, W + 1_000_000_000);
        assert_eq!(anchor.read_span_ns, 2001);
        assert_eq!(anchor.wall_source, "CLOCK_REALTIME");
        assert_eq!(anchor.wall_resolution_ns, Some(100));
        assert_eq!(anchor.wall_rounding, Rounding::FloorOnly);
    }

    #[test]
    fn invalid_brackets_are_discarded() {
        let build = |brackets: &[(u64, u64, u64)]| {
            Anchor::from_brackets(
                brackets,
                Some(1),
                Rounding::FloorOnly,
                "Date.now",
                Some(1_000_000),
                Rounding::TwoSided,
            )
        };
        assert_eq!(build(&[]), None);
        assert_eq!(build(&[(0, 0, 2), (3, 0, 3)]), None);
        assert_eq!(build(&[(5, W, 4)]), None);
        // A zero-span bracket with wall 0 loses to a wider valid one.
        let (anchor, error) = build(&[(10, 0, 10), (9, W, 5), (20, W, 30)]).unwrap();
        assert_eq!((anchor.monotonic_ns, anchor.read_span_ns), (25, 10));
        assert_eq!(anchor.wall_rounding, Rounding::TwoSided);
        // mono floor (0, 1); wall two-sided (1e6, 1e6).
        assert_eq!(pair(error), (5 + 1 + 1_000_000, 5 + 1_000_000));
    }

    #[test]
    fn an_unknown_resolution_is_an_unknown_error() {
        let (_, error) = Anchor::from_brackets(
            &[(0, 1, 2)],
            None,
            Rounding::FloorOnly,
            "CLOCK_REALTIME",
            Some(5),
            Rounding::FloorOnly,
        )
        .unwrap();
        assert_eq!(error, Bound::UNKNOWN);
        let (_, error) = Anchor::from_brackets(
            &[(0, 1, 2)],
            Some(u64::MAX - 1),
            Rounding::FloorOnly,
            "CLOCK_REALTIME",
            Some(5),
            Rounding::FloorOnly,
        )
        .unwrap();
        assert_eq!(
            (pair(error), error.basis),
            ((UNBOUNDED, 6), Basis::ClockRead)
        );
    }

    fn domain(anchor: Option<Anchor>, resolution_ns: Option<u64>) -> Domain {
        Domain {
            host_id: "fixture-host".to_owned(),
            host_clock_epoch: "boot.6553f0ff".to_owned(),
            monotonic_source: "CLOCK_BOOTTIME",
            suspend: SuspendBehaviour::Included,
            resolution_ns,
            anchor,
        }
    }

    fn anchor(wall_resolution_ns: Option<u64>, wall_rounding: Rounding) -> Anchor {
        Anchor {
            monotonic_ns: 1_000_000_000,
            wall_unix_ns: W,
            wall_source: "CLOCK_REALTIME",
            read_span_ns: 2001,
            wall_resolution_ns,
            wall_rounding,
        }
    }

    #[test]
    fn a_reading_maps_through_the_anchor_on_the_same_side() {
        let d = domain(Some(anchor(Some(1000), Rounding::FloorOnly)), Some(100));
        let r = Reading::new(
            3_000_000_000,
            d.id(),
            Bound {
                early_ns: 7,
                late_ns: 50,
                basis: Basis::ClockRead,
            },
        );
        // Anchor error: early ⌈2001/2⌉ + 100 + 0 = 1101; late 1000 + 0 + 1000 = 2000.
        assert_eq!(
            wall_at(&d, r, Rounding::FloorOnly),
            Ok(WallTime {
                unix_ns: W + 2_000_000_000,
                early_ns: 7 + 1101,
                late_ns: 50 + 2000
            })
        );
        // Two-sided monotonic: early gains nothing new, late gains q.
        assert_eq!(
            wall_at(&d, r, Rounding::TwoSided),
            Ok(WallTime {
                unix_ns: W + 2_000_000_000,
                early_ns: 7 + 1101,
                late_ns: 50 + 2100
            })
        );
        // A reading before the anchor maps before the wall read.
        let before = Reading::new(1, d.id(), Bound::UNKNOWN);
        let t = wall_at(&d, before, Rounding::FloorOnly).unwrap();
        assert_eq!(t.unix_ns, W - 999_999_999);
        assert_eq!((t.early_ns, t.late_ns), (UNBOUNDED, UNBOUNDED));
    }

    #[test]
    fn the_wall_leg_takes_the_anchors_rounding() {
        let d = domain(Some(anchor(Some(1000), Rounding::TwoSided)), Some(100));
        let r = Reading::new(1_000_000_000, d.id(), floor(Some(100)));
        let t = wall_at(&d, r, Rounding::FloorOnly).unwrap();
        // early: 0 + (1001 + 100 + 1000); late: 100 + (1000 + 0 + 1000).
        assert_eq!((t.unix_ns, t.early_ns, t.late_ns), (W, 2101, 2100));
    }

    #[test]
    fn unknown_legs_leave_the_mapping_unbounded() {
        let no_wall = domain(Some(anchor(None, Rounding::FloorOnly)), Some(100));
        let r = Reading::new(5, no_wall.id(), floor(Some(100)));
        let t = wall_at(&no_wall, r, Rounding::FloorOnly).unwrap();
        assert_eq!((t.early_ns, t.late_ns), (UNBOUNDED, UNBOUNDED));
        let no_mono = domain(Some(anchor(Some(1), Rounding::FloorOnly)), None);
        let r = Reading::new(5, no_mono.id(), floor(Some(100)));
        let t = wall_at(&no_mono, r, Rounding::FloorOnly).unwrap();
        assert_eq!((t.early_ns, t.late_ns), (UNBOUNDED, UNBOUNDED));
    }

    #[test]
    fn refusals_in_order() {
        let anchored = domain(Some(anchor(Some(1), Rounding::FloorOnly)), Some(1));
        let mut other = anchored.clone();
        other.host_clock_epoch.push_str(".e1");
        other.anchor = None;
        let r = Reading::new(u64::MAX, other.id(), Bound::UNKNOWN);
        assert_eq!(
            wall_at(&anchored, r, Rounding::FloorOnly),
            Err(WallError::DomainMismatch)
        );
        assert_eq!(
            wall_at(&other, r, Rounding::FloorOnly),
            Err(WallError::NoAnchor)
        );
        let past = Reading::new(u64::MAX, anchored.id(), Bound::UNKNOWN);
        assert_eq!(
            wall_at(&anchored, past, Rounding::FloorOnly),
            Err(WallError::Overflow)
        );
        let mut early = anchored.clone();
        early.anchor = Some(Anchor {
            wall_unix_ns: 10,
            ..anchor(Some(1), Rounding::FloorOnly)
        });
        let r = Reading::new(999_999_989, early.id(), Bound::UNKNOWN);
        assert_eq!(
            wall_at(&early, r, Rounding::FloorOnly),
            Err(WallError::Overflow)
        );
        let r = Reading::new(999_999_990, early.id(), Bound::UNKNOWN);
        assert_eq!(
            wall_at(&early, r, Rounding::FloorOnly).map(|t| t.unix_ns),
            Ok(0)
        );
    }

    #[test]
    fn errors_display() {
        assert!(WallError::DomainMismatch.to_string().contains("domain"));
        assert!(WallError::NoAnchor.to_string().contains("anchor"));
        assert!(WallError::Overflow.to_string().contains("u64"));
    }
}
