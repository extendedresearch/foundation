//! The clock a face implements, and the clock whose readings a caller supplies.

use crate::bound::{Basis, Bound};
use crate::domain::{Domain, DomainId};
use crate::reading::Reading;

/// A clock that can be read now.
///
/// A trait rather than a free `now_ns()`, because a browser has no ambient
/// clock, and because an ambient default is how a consumer ends up reading a
/// clock it never chose. Nothing in this crate implements it yet: the native
/// and browser faces do.
pub trait Clock {
    /// The domain every reading of this clock is on.
    fn domain(&self) -> &Domain;

    /// A reading of this clock now.
    ///
    /// The basis is the caller's to state, because a field nobody has to fill
    /// goes unfilled. The bound starts at the face's
    /// [`bound_for_read`](crate::bound_for_read) and the caller widens it.
    /// Infallible: a failed platform read returns `ns = 0` with
    /// [`Bound::UNKNOWN`], which carries [`Basis::Unspecified`] rather than
    /// the basis passed.
    fn now(&self, basis: Basis) -> Reading;
}

/// A clock whose readings the caller supplies: tests, converters, replay, and
/// a binding that reads the clock on the far side of a language boundary.
///
/// It deliberately does not implement [`Clock`]. It has no "now", and a
/// `now()` that returned a made-up value would be the silent default this
/// crate exists to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedClock {
    id: DomainId,
}

impl SuppliedClock {
    /// A clock on `domain`.
    pub fn new(domain: Domain) -> Self {
        SuppliedClock { id: domain.id() }
    }

    /// A reading of `ns`, bounded by `bound`, on this clock's domain.
    pub fn at(&self, ns: u64, bound: Bound) -> Reading {
        Reading::new(ns, self.id, bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SuspendBehaviour;

    fn domain(epoch: &str) -> Domain {
        Domain {
            host_id: "fixture-host".to_owned(),
            host_clock_epoch: epoch.to_owned(),
            monotonic_source: "CLOCK_BOOTTIME",
            suspend: SuspendBehaviour::Included,
            resolution_ns: Some(1),
            anchor: None,
        }
    }

    #[test]
    fn supplied_readings_are_on_the_supplied_domain() {
        let d = domain("boot.1");
        let clock = SuppliedClock::new(d.clone());
        let bound = Bound {
            early_ns: 1,
            late_ns: 2,
            basis: Basis::FileRead,
        };
        let r = clock.at(42, bound);
        assert_eq!((r.as_ns(), r.domain(), r.bound()), (42, d.id(), bound));
        let other = SuppliedClock::new(domain("boot.2")).at(41, bound);
        assert!(r.since(other).is_err());
        assert_eq!(r.since(clock.at(40, bound)).map(|i| i.ns), Ok(2));
    }

    struct Fixed(Domain);

    impl Clock for Fixed {
        fn domain(&self) -> &Domain {
            &self.0
        }
        fn now(&self, _basis: Basis) -> Reading {
            Reading::new(0, self.0.id(), Bound::UNKNOWN)
        }
    }

    #[test]
    fn a_clock_is_object_safe() {
        let clock: Box<dyn Clock> = Box::new(Fixed(domain("boot.1")));
        let r = clock.now(Basis::ClockRead);
        assert_eq!(r.domain(), clock.domain().id());
        assert!(r.bound().is_unknown());
    }
}
