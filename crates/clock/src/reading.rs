//! A reading: nanoseconds, the domain they are on, and the bound on the event.

use std::fmt;

use crate::bound::{Bound, Interval};
use crate::domain::DomainId;
use crate::units::widths;

/// A monotonic clock reading, the domain it was taken on, and a bound on when
/// the event it stamps happened.
///
/// The three are inseparable on purpose. A bare `u64` is how two readings from
/// unrelated clocks get subtracted, and a reading with no bound is how an error
/// of zero gets assumed. When nothing is known the bound is
/// [`Bound::UNKNOWN`], which is explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    ns: u64,
    domain: DomainId,
    bound: Bound,
}

/// Why [`Reading::since`] refused to subtract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinceError {
    /// The two readings are on different domains.
    DomainMismatch,
    /// `later - earlier` does not fit `i64`.
    Overflow,
}

impl fmt::Display for SinceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SinceError::DomainMismatch => "the readings are on different clock domains",
            SinceError::Overflow => "the difference does not fit i64 nanoseconds",
        })
    }
}

impl std::error::Error for SinceError {}

impl Reading {
    /// A reading of `ns` on `domain`, bounded by `bound`.
    pub fn new(ns: u64, domain: DomainId, bound: Bound) -> Self {
        Reading { ns, domain, bound }
    }

    /// The reading in nanoseconds on its domain's clock.
    pub fn as_ns(self) -> u64 {
        self.ns
    }

    /// The domain the reading was taken on.
    pub fn domain(self) -> DomainId {
        self.domain
    }

    /// The bound on when the stamped event happened.
    pub fn bound(self) -> Bound {
        self.bound
    }

    /// The same reading with a bound at least this wide on each side.
    ///
    /// Each side becomes the larger of the current width and the one passed,
    /// so widening never narrows. The basis is kept.
    pub fn widen(self, early_ns: u64, late_ns: u64) -> Self {
        Reading {
            bound: Bound {
                early_ns: self.bound.early_ns.max(early_ns),
                late_ns: self.bound.late_ns.max(late_ns),
                basis: self.bound.basis,
            },
            ..self
        }
    }

    /// The interval from `earlier` to `self`.
    ///
    /// `ns` is `self - earlier`; `below_ns` is `self`'s early side plus
    /// `earlier`'s late side, and `above_ns` is `self`'s late side plus
    /// `earlier`'s early side, each saturating to
    /// [`UNBOUNDED`](crate::UNBOUNDED).
    ///
    /// # Errors
    ///
    /// [`SinceError::DomainMismatch`] when the domains differ, checked first,
    /// and [`SinceError::Overflow`] when the difference does not fit `i64`.
    pub fn since(self, earlier: Reading) -> Result<Interval, SinceError> {
        if self.domain != earlier.domain {
            return Err(SinceError::DomainMismatch);
        }
        let ns = i64::try_from(i128::from(self.ns) - i128::from(earlier.ns))
            .map_err(|_| SinceError::Overflow)?;
        Ok(Interval {
            ns,
            below_ns: widths(self.bound.early_ns, earlier.bound.late_ns),
            above_ns: widths(self.bound.late_ns, earlier.bound.early_ns),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bound::Basis;
    use crate::domain::{Domain, SuspendBehaviour};
    use crate::units::UNBOUNDED;

    fn id(epoch: &str) -> DomainId {
        Domain {
            host_id: "fixture-host".to_owned(),
            host_clock_epoch: epoch.to_owned(),
            monotonic_source: "CLOCK_BOOTTIME",
            suspend: SuspendBehaviour::Included,
            resolution_ns: None,
            anchor: None,
        }
        .id()
    }

    fn bound(early_ns: u64, late_ns: u64) -> Bound {
        Bound {
            early_ns,
            late_ns,
            basis: Basis::ClockRead,
        }
    }

    #[test]
    fn accessors_return_what_was_built() {
        let d = id("boot.1");
        let r = Reading::new(42, d, bound(1, 2));
        assert_eq!(r.as_ns(), 42);
        assert_eq!(r.domain(), d);
        assert_eq!(r.bound(), bound(1, 2));
    }

    #[test]
    fn widths_add_crosswise() {
        let d = id("boot.1");
        let later = Reading::new(1_250_000_000, d, bound(100_000, 100_000));
        let earlier = Reading::new(1_000_000_000, d, bound(0, 1_000_000));
        assert_eq!(
            later.since(earlier),
            Ok(Interval {
                ns: 250_000_000,
                below_ns: 1_100_000,
                above_ns: 100_000
            })
        );
        assert_eq!(
            earlier.since(later),
            Ok(Interval {
                ns: -250_000_000,
                below_ns: 100_000,
                above_ns: 1_100_000
            })
        );
    }

    #[test]
    fn a_wide_sum_saturates() {
        let d = id("boot.1");
        let a = Reading::new(3, d, bound(1 << 63, 0));
        let b = Reading::new(2, d, bound(0, 1 << 63));
        let i = a.since(b);
        assert_eq!(
            i,
            Ok(Interval {
                ns: 1,
                below_ns: UNBOUNDED,
                above_ns: 0
            })
        );
    }

    #[test]
    fn refuses_across_domains() {
        let a = Reading::new(2, id("boot.1"), bound(0, 0));
        let b = Reading::new(1, id("boot.1.e1"), bound(0, 0));
        assert_eq!(a.since(b), Err(SinceError::DomainMismatch));
        // Mismatch is reported before overflow.
        let far = Reading::new(u64::MAX, id("boot.2"), bound(0, 0));
        assert_eq!(far.since(b), Err(SinceError::DomainMismatch));
    }

    #[test]
    fn the_i64_edges() {
        let d = id("boot.1");
        let at = |ns| Reading::new(ns, d, bound(0, 0));
        let i64_max = i64::MAX.unsigned_abs();
        assert_eq!(at(i64_max).since(at(0)).map(|i| i.ns), Ok(i64::MAX));
        assert_eq!(at(i64_max + 1).since(at(0)), Err(SinceError::Overflow));
        assert_eq!(at(0).since(at(i64_max + 1)).map(|i| i.ns), Ok(i64::MIN));
        assert_eq!(at(0).since(at(u64::MAX)), Err(SinceError::Overflow));
        assert_eq!(at(u64::MAX).since(at(u64::MAX)).map(|i| i.ns), Ok(0));
    }

    #[test]
    fn widen_never_narrows() {
        let r = Reading::new(5, id("boot.1"), bound(10, 20));
        assert_eq!(r.widen(0, 0), r);
        assert_eq!(r.widen(15, 5).bound(), bound(15, 20));
        assert_eq!(r.widen(5, 30).bound(), bound(10, 30));
        assert_eq!(
            r.widen(UNBOUNDED, UNBOUNDED).bound(),
            bound(UNBOUNDED, UNBOUNDED)
        );
        let unknown = Reading::new(0, id("boot.1"), Bound::UNKNOWN);
        assert_eq!(unknown.widen(1, 1).bound(), Bound::UNKNOWN);
        assert_eq!(r.widen(1, 1).as_ns(), 5);
    }

    #[test]
    fn errors_display() {
        assert!(SinceError::DomainMismatch.to_string().contains("domain"));
        assert!(SinceError::Overflow.to_string().contains("i64"));
    }
}
