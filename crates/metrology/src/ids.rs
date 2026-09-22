//! The integer identifiers a term names, and why a term names integers rather
//! than strings.
//!
//! A term is `Copy` and fixed-size (R18). That is not a micro-optimisation: it
//! is what lets one type serve a hot path taking a hundred thousand stamps a
//! second, a serialised record, and a C ABI projection, without a second
//! design for each. Free text in a term would put a heap allocation on every
//! copy of it and a pointer in every wire encoding.
//!
//! So a term says *which* calibration, document, estimator, argument or device
//! it came from, and the text lives once in the record's side tables, addressed
//! by the same integer (R21: every identifier a provenance names resolves
//! within the record that carries the term). The side tables are the record
//! schema's, which is a later step; what this module owns is the identifier
//! types, so that a `CalibrationId` cannot be passed where a `DocumentId` is
//! meant.
//!
//! The newtypes are not interchangeable and none of them is `u32` by
//! inference. Every one wraps a public `u32`, so a record decoder builds them
//! without a constructor.

/// Declares an identifier newtype over `u32`, with its documentation.
macro_rules! identifier {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(
            /// The identifier's value, which resolves in the record's side table.
            pub u32,
        );
    };
}

identifier! {
    /// Which chain a stamp travelled, as an integer a stamp can carry.
    ///
    /// The chain, its links and the terms covering them live once per stream;
    /// a stamp carries this and nothing more (architecture §2). Zero is
    /// [`ChainId::UNSPECIFIED`], the value a caller that has described no chain
    /// passes.
    ChainId
}

impl ChainId {
    /// No chain was named.
    ///
    /// A caller that describes no chain uses this and behaves as it did before
    /// chains existed. A composition over an unspecified chain still has to
    /// cover that chain's links, so this is not a way past R11: it is a way to
    /// pass a stamp through code that does not care.
    pub const UNSPECIFIED: ChainId = ChainId(0);
}

identifier! {
    /// Which device a link belongs to.
    ///
    /// Two calibrations can both describe "device delay" and mean different
    /// boxes, which is one of the reasons coverage is a span rather than a
    /// kind (R16).
    DeviceId
}

identifier! {
    /// Which calibration a [`Measured`](crate::Provenance::Measured) term came
    /// from.
    ///
    /// A measured 18.2 ms display latency and a datasheet's "typical 16 ms" are
    /// different epistemic objects, and R20 forbids flattening them to one
    /// representation. This identifier is half of what keeps them apart; the
    /// other half is that the two use different [`Provenance`](crate::Provenance)
    /// variants.
    CalibrationId
}

identifier! {
    /// Which document a [`Specified`](crate::Provenance::Specified) term was
    /// read out of — a vendor claim, unverified.
    DocumentId
}

identifier! {
    /// Which estimator produced an [`Estimated`](crate::Provenance::Estimated)
    /// term, **including its version**.
    ///
    /// R22: two implementations differing in the last nanosecond produce
    /// timestamps that look comparable and are not. The identifier resolves to
    /// a name *and a version* in the record's estimator table, so "which
    /// arithmetic produced this" is answerable from the file (R65).
    EstimatorId
}

identifier! {
    /// Which inputs an [`Estimated`](crate::Provenance::Estimated) term's
    /// estimator ran on.
    ///
    /// An estimate is reproducible only when both the arithmetic and what it
    /// ran on are recoverable, so the provenance names both.
    InputsId
}

identifier! {
    /// Which stored argument a [`Bounded`](crate::Provenance::Bounded) term
    /// rests on — a reasoned worst case someone wrote down.
    ///
    /// A worst case nobody wrote down is not `Bounded`; it is
    /// [`Unknown`](crate::Provenance::Unknown).
    ArgumentId
}

identifier! {
    /// Which correlation group a term declared itself part of.
    ///
    /// A group is a claim with two halves: the terms inside it are correlated
    /// with one another, and they are independent of every term outside it.
    /// See [`Correlation`](crate::Correlation) for why a caller unwilling to
    /// make the second half uses `Undeclared` instead.
    GroupId
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test that panics is reporting, not failing to handle"
)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_is_zero() {
        assert_eq!(ChainId::UNSPECIFIED, ChainId(0));
        assert_eq!(ChainId::default(), ChainId::UNSPECIFIED);
    }

    #[test]
    fn identifiers_order_by_their_value() {
        assert!(CalibrationId(1) < CalibrationId(2));
        assert_eq!(DocumentId(7).0, 7);
    }
}
