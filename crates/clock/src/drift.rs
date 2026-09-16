//! The drift check: a suspend or a wall step ends the domain.

use crate::anchor::{Anchor, anchor_error};
use crate::bound::{Bound, Interval, Rounding, bound_for_read};
use crate::domain::{Domain, DomainId, next_epoch};
use crate::reading::{Reading, SinceError};
use crate::units::widths;

/// The domain ended between two observations.
///
/// No interval can be computed across the jump: `before` is on the ending
/// domain and `after` on `next_domain`, so `after.since(before)` is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discontinuity {
    /// The last reading before the jump, on the domain that is ending.
    pub before: Reading,
    /// The first reading after the jump, re-tagged onto `next_domain`.
    pub after: Reading,
    /// The observed change in `wall - monotonic` across the pair, signed.
    /// Positive when the wall clock advanced more than the monotonic clock, as
    /// across a suspend the monotonic clock did not count. Its bound is built
    /// from both observations' anchor errors.
    pub offset_change: Interval,
    /// The domain readings after the jump belong to: the same `host_id`, and
    /// `host_clock_epoch` with its suffix replaced by `.e<n+1>`. Its anchor is
    /// the observation that fired.
    pub next_domain: Domain,
}

/// When the drift check fires: `|Δoffset| > floor_ns + ⌊rate_ppm · Δt / 10^6⌋`.
///
/// `floor_ns` covers read noise, which is the same for every pair, and
/// `rate_ppm` covers the rate difference between the wall and monotonic
/// clocks, which grows with the time between observations. The values are per
/// face and measured; this type fixes only their meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriftThreshold {
    /// The part of the threshold that does not depend on `Δt`.
    pub floor_ns: u64,
    /// The part that grows with `Δt`, in parts per million.
    pub rate_ppm: u32,
}

/// One bracketed `(monotonic, wall)` observation, as the check keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pair {
    monotonic_ns: u64,
    offset: i128,
    read_span_ns: u64,
    wall_resolution_ns: Option<u64>,
    wall_rounding: Rounding,
    /// `None` for the anchor passed to [`DriftCheck::start`], which is not a
    /// reading.
    reading: Option<Reading>,
}

impl Pair {
    fn error(&self, monotonic: Bound) -> Bound {
        anchor_error(
            self.read_span_ns,
            monotonic,
            bound_for_read(self.wall_resolution_ns, self.wall_rounding),
        )
    }
}

/// Watches `wall - monotonic` and ends the domain when it moves by more than
/// the threshold.
///
/// On a clock that stops during a suspend, a suspend moves that offset by the
/// suspended time. The check cannot tell a suspend from a wall-clock step, and
/// does not try: either ends the domain. A false split is the chosen failure,
/// because it makes an interval unsubtractable rather than wrong.
///
/// The comparison is pair to pair, strict, and on the central value of the
/// offset change: each observation against the previous one, the first against
/// the anchor passed to [`DriftCheck::start`]. `read_span_ns` and the
/// resolutions do not widen the comparison; they widen `offset_change`'s bound,
/// and a face accounts for read noise in the `floor_ns` it chooses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriftCheck {
    domain: Domain,
    id: DomainId,
    threshold: DriftThreshold,
    wall_source: &'static str,
    previous: Pair,
}

impl DriftCheck {
    /// Starts the check on `domain`, with `anchor` as the first previous pair.
    ///
    /// It takes a [`Domain`] rather than a [`Clock`](crate::Clock), so supplied
    /// data can drive it.
    pub fn start(domain: &Domain, anchor: Anchor, threshold: DriftThreshold) -> Self {
        DriftCheck {
            domain: domain.clone(),
            id: domain.id(),
            threshold,
            wall_source: domain.anchor.map_or(anchor.wall_source, |a| a.wall_source),
            previous: Pair {
                monotonic_ns: anchor.monotonic_ns,
                offset: i128::from(anchor.wall_unix_ns) - i128::from(anchor.monotonic_ns),
                read_span_ns: anchor.read_span_ns,
                wall_resolution_ns: anchor.wall_resolution_ns,
                wall_rounding: anchor.wall_rounding,
                reading: None,
            },
        }
    }

    /// Feeds one fresh bracketed observation.
    ///
    /// `monotonic` is the bracket midpoint on the current domain, bounded as
    /// `bound_for_read` bounds that clock; `wall_unix_ns`, `read_span_ns`,
    /// `wall_resolution_ns` and `wall_rounding` describe the wall read and the
    /// bracket, as in [`Anchor`].
    ///
    /// With `offset = wall_unix_ns - monotonic` and `Δt` the monotonic time
    /// since the previous pair, the check fires when
    /// `|offset - previous offset| > floor_ns + ⌊rate_ppm · Δt / 10^6⌋`.
    ///
    /// `Ok(None)` when it does not fire; the observation becomes the previous
    /// pair. On a fire, `Ok(Some(d))`, and the check continues on
    /// `d.next_domain` with this observation as the previous pair; the caller
    /// swaps its own domain for `d.next_domain`. In the record:
    ///
    /// - `before` is the previous pair's reading. When the first observation
    ///   fires, the previous pair is the start anchor, and `before` is its
    ///   `monotonic_ns` on the ending domain, bounded by the anchor's error.
    /// - `after` is `monotonic` re-tagged onto `next_domain`, with its bound.
    /// - `offset_change.ns` is the change in offset. Its `below_ns` is this
    ///   observation's anchor early error plus the previous pair's late error,
    ///   and `above_ns` the reverse. Both errors take their monotonic leg from
    ///   the bound `monotonic` carries, and their wall leg from each pair's own
    ///   resolution and rounding.
    /// - `next_domain.anchor` is this observation, with the ending domain's
    ///   wall source.
    ///
    /// # Errors
    ///
    /// [`SinceError::DomainMismatch`] when `monotonic` is not on the current
    /// domain, checked first. [`SinceError::Overflow`] when the offset change
    /// does not fit `i64`, or when a fire would need an epoch suffix past
    /// `.e18446744073709551615`. A refused observation changes nothing.
    pub fn observe(
        &mut self,
        monotonic: Reading,
        wall_unix_ns: u64,
        read_span_ns: u64,
        wall_resolution_ns: Option<u64>,
        wall_rounding: Rounding,
    ) -> Result<Option<Discontinuity>, SinceError> {
        if monotonic.domain() != self.id {
            return Err(SinceError::DomainMismatch);
        }
        let offset = i128::from(wall_unix_ns) - i128::from(monotonic.as_ns());
        let change = offset - self.previous.offset;
        let change_ns = i64::try_from(change).map_err(|_| SinceError::Overflow)?;
        let dt = monotonic.as_ns().abs_diff(self.previous.monotonic_ns);
        let limit = u128::from(self.threshold.floor_ns)
            + u128::from(self.threshold.rate_ppm) * u128::from(dt) / 1_000_000;
        let mut pair = Pair {
            monotonic_ns: monotonic.as_ns(),
            offset,
            read_span_ns,
            wall_resolution_ns,
            wall_rounding,
            reading: Some(monotonic),
        };
        if change.unsigned_abs() <= limit {
            self.previous = pair;
            return Ok(None);
        }

        let epoch = next_epoch(&self.domain.host_clock_epoch).ok_or(SinceError::Overflow)?;
        let leg = monotonic.bound();
        let error_now = pair.error(leg);
        let error_before = self.previous.error(leg);
        let offset_change = Interval {
            ns: change_ns,
            below_ns: widths(error_now.early_ns, error_before.late_ns),
            above_ns: widths(error_now.late_ns, error_before.early_ns),
        };
        let before = self
            .previous
            .reading
            .unwrap_or_else(|| Reading::new(self.previous.monotonic_ns, self.id, error_before));
        let next_domain = Domain {
            host_clock_epoch: epoch,
            anchor: Some(Anchor {
                monotonic_ns: monotonic.as_ns(),
                wall_unix_ns,
                wall_source: self.wall_source,
                read_span_ns,
                wall_resolution_ns,
                wall_rounding,
            }),
            ..self.domain.clone()
        };
        let next_id = next_domain.id();
        let after = Reading::new(monotonic.as_ns(), next_id, leg);
        pair.reading = Some(after);
        self.previous = pair;
        self.domain = next_domain.clone();
        self.id = next_id;
        Ok(Some(Discontinuity {
            before,
            after,
            offset_change,
            next_domain,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::bound::Basis;
    use crate::domain::SuspendBehaviour;
    use crate::units::UNBOUNDED;

    const W: u64 = 1_700_000_000_000_000_000;
    const FLOOR: Rounding = Rounding::FloorOnly;

    fn domain(epoch: &str) -> Domain {
        Domain {
            host_id: "fixture-host".to_owned(),
            host_clock_epoch: epoch.to_owned(),
            monotonic_source: "CLOCK_MONOTONIC",
            suspend: SuspendBehaviour::Excluded,
            resolution_ns: Some(100),
            anchor: None,
        }
    }

    fn anchor() -> Anchor {
        Anchor {
            monotonic_ns: 1_000_000_000,
            wall_unix_ns: W,
            wall_source: "CLOCK_REALTIME",
            read_span_ns: 2000,
            wall_resolution_ns: Some(100),
            wall_rounding: FLOOR,
        }
    }

    const THRESHOLD: DriftThreshold = DriftThreshold {
        floor_ns: 50_000_000,
        rate_ppm: 500,
    };

    fn read(check: &DriftCheck, ns: u64) -> Reading {
        Reading::new(ns, check.id, bound_for_read(Some(100), FLOOR))
    }

    #[test]
    fn pair_to_pair_not_against_the_anchor() {
        let mut check = DriftCheck::start(&domain("boot.6553f0ff"), anchor(), THRESHOLD);
        let r = read(&check, 2_000_000_000);
        assert_eq!(
            check.observe(r, W + 1_030_000_000, 2000, Some(100), FLOOR),
            Ok(None)
        );
        let r = read(&check, 3_000_000_000);
        assert_eq!(
            check.observe(r, W + 2_060_000_000, 2000, Some(1000), FLOOR),
            Ok(None)
        );
    }

    #[test]
    fn a_suspend_ends_the_domain() {
        let start = domain("boot.6553f0ff");
        let mut check = DriftCheck::start(&start, anchor(), THRESHOLD);
        let r1 = read(&check, 3_000_000_000);
        // 2 s of monotonic, 2.02 s of wall: 20 ms, under 50 ms + 1 ms.
        assert_eq!(
            check.observe(r1, W + 2_020_000_000, 2000, Some(1000), FLOOR),
            Ok(None)
        );
        let r2 = read(&check, 3_100_000_000);
        let d = check
            .observe(r2, W + 12_120_000_000, 3001, Some(1000), FLOOR)
            .unwrap()
            .unwrap();
        assert_eq!(d.before, r1);
        assert_eq!(d.after.as_ns(), 3_100_000_000);
        assert_eq!(d.after.bound(), r2.bound());
        assert_eq!(d.next_domain.host_clock_epoch, "boot.6553f0ff.e1");
        assert_eq!(d.next_domain.host_id, start.host_id);
        assert_eq!(d.next_domain.monotonic_source, start.monotonic_source);
        assert_eq!(d.after.domain(), d.next_domain.id());
        assert_eq!(d.after.since(d.before), Err(SinceError::DomainMismatch));
        // now: (1501 + 100 + 0, 1500 + 0 + 1000); before: (1000 + 100, 1000 + 1000).
        assert_eq!(
            d.offset_change,
            Interval {
                ns: 10_000_000_000,
                below_ns: 1601 + 2000,
                above_ns: 2500 + 1100
            }
        );
        assert_eq!(
            d.next_domain.anchor,
            Some(Anchor {
                monotonic_ns: 3_100_000_000,
                wall_unix_ns: W + 12_120_000_000,
                wall_source: "CLOCK_REALTIME",
                read_span_ns: 3001,
                wall_resolution_ns: Some(1000),
                wall_rounding: FLOOR,
            })
        );
        // The check continues on the next domain.
        assert_eq!(
            check.observe(r2, W, 0, None, FLOOR),
            Err(SinceError::DomainMismatch)
        );
        let r3 = Reading::new(3_200_000_000, d.next_domain.id(), r2.bound());
        assert_eq!(
            check.observe(r3, W + 12_220_000_000, 2000, Some(100), FLOOR),
            Ok(None)
        );
    }

    #[test]
    fn exactly_at_threshold_does_not_fire_and_the_rate_term_counts() {
        let mut check = DriftCheck::start(&domain("boot.1"), anchor(), THRESHOLD);
        // Δt = 1 s: limit 50.5 ms; change exactly 50.5 ms.
        let r = read(&check, 2_000_000_000);
        assert_eq!(
            check.observe(r, W + 1_050_500_000, 0, Some(1), FLOOR),
            Ok(None)
        );
        // Δt = 100 s: limit 100 ms; change 60 ms.
        let r = read(&check, 102_000_000_000);
        assert_eq!(
            check.observe(r, W + 101_110_500_000, 0, Some(1), FLOOR),
            Ok(None)
        );
        // One more nanosecond over a 1 s pair fires.
        let r = read(&check, 103_000_000_000);
        let fired = check.observe(r, W + 102_161_000_001, 0, Some(1), FLOOR);
        assert!(matches!(fired, Ok(Some(_))), "{fired:?}");
    }

    #[test]
    fn a_step_back_fires_and_replaces_the_suffix() {
        let mut check = DriftCheck::start(&domain("boot.1.e9"), anchor(), THRESHOLD);
        let r = read(&check, 2_000_000_000);
        let d = check
            .observe(r, W - 1_000_000_000, 0, Some(1), FLOOR)
            .unwrap()
            .unwrap();
        assert_eq!(d.offset_change.ns, -2_000_000_000);
        assert_eq!(d.next_domain.host_clock_epoch, "boot.1.e10");
    }

    #[test]
    fn the_first_observation_firing_bounds_before_by_the_anchor() {
        let mut check = DriftCheck::start(&domain("doc.00000000000000ab"), anchor(), THRESHOLD);
        let r = read(&check, 1_100_000_000);
        let d = check
            .observe(r, W + 5_100_000_000, 2000, Some(100), FLOOR)
            .unwrap()
            .unwrap();
        assert_eq!(d.before.as_ns(), 1_000_000_000);
        assert_eq!(d.before.domain(), domain("doc.00000000000000ab").id());
        // Anchor error: span 2000, mono leg (0, 100), wall (0, 100).
        assert_eq!(
            d.before.bound(),
            Bound {
                early_ns: 1100,
                late_ns: 1100,
                basis: Basis::ClockRead
            }
        );
        assert_eq!(d.next_domain.host_id, "fixture-host");
        assert_eq!(d.next_domain.host_clock_epoch, "doc.00000000000000ab.e1");
    }

    #[test]
    fn an_unknown_leg_makes_before_unknown() {
        let mut check = DriftCheck::start(&domain("boot.1"), anchor(), THRESHOLD);
        let r = Reading::new(1_100_000_000, check.id, Bound::UNKNOWN);
        let d = check
            .observe(r, W + 5_100_000_000, 2000, Some(100), FLOOR)
            .unwrap()
            .unwrap();
        assert_eq!(d.before.bound(), Bound::UNKNOWN);
        assert_eq!(
            (d.offset_change.below_ns, d.offset_change.above_ns),
            (UNBOUNDED, UNBOUNDED)
        );
    }

    #[test]
    fn the_wall_source_is_the_ending_domains() {
        let mut anchored = domain("boot.1");
        anchored.anchor = Some(Anchor {
            wall_source: "GetSystemTimePreciseAsFileTime",
            ..anchor()
        });
        let mut check = DriftCheck::start(&anchored, anchor(), THRESHOLD);
        let r = read(&check, 1_100_000_000);
        let d = check
            .observe(r, W + 5_100_000_000, 0, None, Rounding::TwoSided)
            .unwrap()
            .unwrap();
        let next = d.next_domain.anchor.unwrap();
        assert_eq!(next.wall_source, "GetSystemTimePreciseAsFileTime");
        assert_eq!(
            (next.wall_resolution_ns, next.wall_rounding),
            (None, Rounding::TwoSided)
        );
    }

    #[test]
    fn refusals_change_nothing() {
        let start = domain("boot.1");
        let mut check = DriftCheck::start(
            &start,
            Anchor {
                monotonic_ns: u64::MAX,
                wall_unix_ns: 1,
                ..anchor()
            },
            THRESHOLD,
        );
        let before = check.clone();
        let wrong = Reading::new(0, domain("boot.2").id(), Bound::UNKNOWN);
        assert_eq!(
            check.observe(wrong, u64::MAX, 0, None, FLOOR),
            Err(SinceError::DomainMismatch)
        );
        let r = read(&check, 0);
        assert_eq!(
            check.observe(r, u64::MAX, 0, None, FLOOR),
            Err(SinceError::Overflow)
        );
        assert_eq!(check, before);

        let mut last =
            DriftCheck::start(&domain("boot.1.e18446744073709551615"), anchor(), THRESHOLD);
        let snapshot = last.clone();
        let r = read(&last, 2_000_000_000);
        assert_eq!(
            last.observe(r, W + 9_000_000_000, 0, None, FLOOR),
            Err(SinceError::Overflow)
        );
        assert_eq!(last, snapshot);
    }

    #[test]
    fn extreme_rate_and_gap_do_not_overflow() {
        let threshold = DriftThreshold {
            floor_ns: u64::MAX,
            rate_ppm: u32::MAX,
        };
        let mut check = DriftCheck::start(
            &domain("boot.1"),
            Anchor {
                monotonic_ns: 0,
                wall_unix_ns: 0,
                ..anchor()
            },
            threshold,
        );
        // The largest change that fits i64, over the largest Δt and rate.
        let r = read(&check, i64::MAX.unsigned_abs());
        assert_eq!(check.observe(r, 0, 0, None, FLOOR), Ok(None));
    }
}
