//! One named contribution to an interval's uncertainty: what it covers, how
//! wide it is, how it spreads, what it corrects, and how each of those was
//! arrived at.
//!
//! # Two currencies, never mixed
//!
//! A worst-case interval and a standard deviation compose by different
//! arithmetic. Widths of a worst case add linearly; standard deviations of
//! independent variables add in quadrature. Putting both in one field makes the
//! composition unsound in whichever direction it is wrong: reading a standard
//! deviation as a bound understates the extreme, and reading a bound as a
//! standard deviation overstates the typical by roughly √3 for a uniform term.
//!
//! So [`Bias`] and [`Dispersion`] are separate fields with separate
//! composition, and neither can be spelled in the other's units (R1, R2).
//!
//! # A calibration yields three things, and they never merge
//!
//! A photodiode measuring a panel's output lag at 18.2 ms with a 0.4 ms
//! standard deviation over 200 repetitions, and a residual worst case of
//! ±0.9 ms about the mean, is
//!
//! ```text
//! correction = Some(-18_200_000 ns, Measured { calibration })
//! bias       = (900_000, 900_000)
//! dispersion = 400_000 ns, Gaussian
//! ```
//!
//! It is not a bound of 18.2 ms, and it is not a bound of 0.4 ms. A measured
//! latency is never expressed by widening a bound (R5).
//!
//! # The bias and the correction have separate provenance
//!
//! One provenance per term cannot represent a case the model has to carry. A
//! one-way clock fit produces **two** epistemic objects over one span: the
//! fitted offset, which is a correction an estimator computed, and a residual
//! bound of `(UNBOUNDED, 0)`, which rests on the argument that a transport
//! delay is never negative. One is [`Provenance::Estimated`] and the other is
//! [`Provenance::Bounded`], over the same span, in the same term.
//!
//! Splitting them into two terms would be worse: the composer refuses two
//! correcting terms over one span (R27), and the fit would become
//! unrepresentable. So [`Term::bias_provenance`] says how the width was arrived
//! at, and [`Correction::provenance`] says how the adjustment was.
//!
//! # `None` and `Some(0)` are different claims
//!
//! [`Term::correction`] is an `Option`. `None` is "no correction was applied";
//! `Some(Correction { ns: 0, .. })` is a positive claim that the right
//! adjustment is zero, and it names who says so. The distinction is R6's, one
//! field over: a zero is a claim, and the absence of one is not.
//!
//! # Zero is a claim; absence is `UNBOUNDED`
//!
//! R6. A zero bias asserts that the stage contributes nothing. The absence of
//! knowledge is [`UNBOUNDED`] on both sides. This is why [`Bias`] has no
//! `Default`: a derived one would be `(0, 0)`, which would turn every field a
//! caller forgot to fill into a positive claim that the stage is perfect.
//!
//! # Free text does not live here
//!
//! R18. A term is `Copy` and fixed-size, and every identifier in it is an
//! integer into the record's side tables. That is what lets the same type serve
//! a hot path, a serialised record and a C ABI projection without a second
//! design. See [`crate::ids`].

use extendedresearch_clock::UNBOUNDED;

use crate::chain::Span;
use crate::ids::{ArgumentId, CalibrationId, DocumentId, EstimatorId, GroupId, InputsId};

/// A worst-case asymmetric interval: how far the true instant may sit from the
/// stated one.
///
/// A value `v` with bias `(early_ns, late_ns)` asserts that the truth lies in
/// `[v − early_ns, v + late_ns]`. It is never a standard deviation, a symmetric
/// half-width, or a confidence interval (R1) — widths of a difference add
/// linearly, and that holds only for worst cases.
///
/// Both widths saturate at [`UNBOUNDED`] under addition, so a sum that passes
/// `u64::MAX` reports that no bound is asserted rather than wrapping to a small
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bias {
    /// How much earlier than the stated value the truth may be.
    pub early_ns: u64,
    /// How much later than the stated value the truth may be.
    pub late_ns: u64,
}

impl Bias {
    /// Nothing is known on either side.
    ///
    /// This, never `(0, 0)`, is what a stage nobody measured contributes (R6).
    pub const UNKNOWN: Bias = Bias {
        early_ns: UNBOUNDED,
        late_ns: UNBOUNDED,
    };

    /// The stage contributes nothing, on either side.
    ///
    /// A positive claim, and one a reviewer may ask you to defend. It is the
    /// right value for a term that exists only to carry a dispersion or a
    /// correction.
    pub const NONE: Bias = Bias {
        early_ns: 0,
        late_ns: 0,
    };

    /// A bias of these two widths.
    pub const fn new(early_ns: u64, late_ns: u64) -> Bias {
        Bias { early_ns, late_ns }
    }

    /// A bias of the same width on both sides.
    pub const fn symmetric(width_ns: u64) -> Bias {
        Bias::new(width_ns, width_ns)
    }

    /// True when no bound is asserted on the early side.
    pub const fn early_is_unbounded(self) -> bool {
        self.early_ns == UNBOUNDED
    }

    /// True when no bound is asserted on the late side.
    pub const fn late_is_unbounded(self) -> bool {
        self.late_ns == UNBOUNDED
    }

    /// True only when both sides are [`UNBOUNDED`].
    ///
    /// One unbounded side is not unknown: a mapping whose event may be
    /// arbitrarily late but never early is a real and common shape — it is what
    /// a one-way fit produces — and reading it as "nothing is known" would
    /// discard the half that is.
    pub const fn is_unknown(self) -> bool {
        self.early_is_unbounded() && self.late_is_unbounded()
    }

    /// Adds two biases side by side, each side saturating at [`UNBOUNDED`].
    ///
    /// This is composition **along** one chain: two stages in series each add
    /// their own early width to the early side and their own late width to the
    /// late side. Composition **across** a difference is crosswise instead —
    /// see [`Bias::crosswise`].
    pub const fn saturating_add(self, other: Bias) -> Bias {
        Bias {
            early_ns: self.early_ns.saturating_add(other.early_ns),
            late_ns: self.late_ns.saturating_add(other.late_ns),
        }
    }

    /// The bias of `self − earlier`, with the widths added crosswise (R3).
    ///
    /// For `later.since(earlier)` with biases `(e_a, l_a)` and `(e_b, l_b)`,
    /// the difference's early width is `e_a + l_b` and its late width is
    /// `l_a + e_b`. The later event happening early and the earlier one
    /// happening late both shrink the interval, so those two widths are the
    /// ones that belong on the same side. Adding them side by side instead
    /// produces a number that looks right and is wrong by however asymmetric
    /// the two stamps were.
    pub const fn crosswise(self, earlier: Bias) -> Bias {
        Bias {
            early_ns: self.early_ns.saturating_add(earlier.late_ns),
            late_ns: self.late_ns.saturating_add(earlier.early_ns),
        }
    }
}

/// The distribution a [`Dispersion`] was derived under.
///
/// The kind is not decoration: it is what says whether a standard deviation may
/// be turned into a coverage interval later, and by what factor. A composer
/// that combined two dispersions and reported the result as Gaussian because
/// both inputs were would be right; one that reported a sum of two uniforms as
/// uniform would not, which is why [`crate::CombinedDispersion`] reports
/// [`DistributionKind::Unspecified`] unless every contributor was Gaussian.
///
/// The integers are a wire contract on the same terms as [`crate::LinkKind`]'s:
/// never renumbered, never reused, appended on extension. Zero is the absence
/// of a declaration, so a zeroed field decodes as "not stated" rather than as a
/// distribution nobody claimed.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DistributionKind {
    /// No distribution was declared.
    Unspecified = 0,
    /// Normal.
    Gaussian = 1,
    /// Uniform over a width — what quantisation produces, whose standard
    /// deviation is the width over √12.
    Uniform = 2,
}

impl DistributionKind {
    /// The integer this kind is written to a record as.
    pub const fn kind(self) -> u16 {
        self as u16
    }
}

/// Trial-to-trial spread, expressed distributionally.
///
/// Separate from [`Bias`] and composed by different arithmetic (R2, R4). A
/// dispersion is precision, not accuracy: it says how much the value moves
/// between repetitions, and nothing at all about how far the whole set sits
/// from the truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dispersion {
    /// The standard deviation, in nanoseconds.
    pub sd_ns: u64,
    /// The distribution it was derived under.
    pub kind: DistributionKind,
}

impl Dispersion {
    /// A dispersion of this standard deviation and kind.
    pub const fn new(sd_ns: u64, kind: DistributionKind) -> Dispersion {
        Dispersion { sd_ns, kind }
    }
}

/// A signed adjustment applied to a value, and how it was arrived at.
///
/// Separate from the term's [`Bias`] and separately attributed, because the two
/// routinely come from different places. A one-way clock fit corrects by an
/// offset an estimator computed and bounds by an argument about transport
/// delay; a photodiode calibration corrects and bounds from the same
/// measurement. Both are one term.
///
/// A correction moves the value. Two of them over one link move it twice, by
/// exactly the size of the term the second one exists to capture, which is why
/// the composer refuses intersecting corrections (R27).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Correction {
    /// The signed nanoseconds added to the value.
    pub ns: i64,
    /// How the adjustment was arrived at.
    pub provenance: Provenance,
}

impl Correction {
    /// A correction of this many nanoseconds, from this provenance.
    pub const fn new(ns: i64, provenance: Provenance) -> Correction {
        Correction { ns, provenance }
    }
}

/// What a term declares about its relationship to the other terms of a budget.
///
/// # Why the default refuses
///
/// [`Correlation::Undeclared`] is the default, and the composer will not
/// combine dispersions across it: it reports them per term and marks the
/// combined dispersion undeclared (R4).
///
/// Defaulting to [`Correlation::Independent`] would compose in quadrature and
/// produce a *smaller* total, which is wrong whenever two terms share a cause —
/// routine for two terms on one device, where one temperature drift or one
/// oscillator moves both. A default that is wrong in the direction of
/// flattering the measurement is exactly the failure this crate exists to
/// prevent. The cost is that a caller wanting the smaller number states
/// independence explicitly, which is the claim it was making anyway.
///
/// # What a group declares
///
/// [`Correlation::CorrelatedWith`] is a claim with two halves: this term is
/// correlated with every other term in the group, **and** independent of every
/// term outside it. Terms inside a group add linearly; the group's sum then
/// enters the quadrature with everything else. A caller unwilling to make the
/// second half uses `Undeclared`, and gets no combined dispersion rather than a
/// wrong one.
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Correlation {
    /// Nothing was declared, so nothing is combined.
    #[default]
    Undeclared = 0,
    /// Independent of every other term in the budget.
    Independent = 1,
    /// Correlated with the other members of this group, independent of
    /// everything outside it.
    CorrelatedWith(GroupId) = 2,
}

impl Correlation {
    /// The integer this declaration is written to a record as.
    pub const fn kind(self) -> u16 {
        match self {
            Correlation::Undeclared => 0,
            Correlation::Independent => 1,
            Correlation::CorrelatedWith(_) => 2,
        }
    }

    /// The group, where one was named.
    pub const fn group(self) -> Option<GroupId> {
        match self {
            Correlation::CorrelatedWith(group) => Some(group),
            _ => None,
        }
    }

    /// True when the term said nothing, so nothing combines across it (R4).
    pub const fn is_undeclared(self) -> bool {
        matches!(self, Correlation::Undeclared)
    }
}

/// How a number was arrived at.
///
/// R19, R20. A measured 18.2 ms display latency and a datasheet's "typical
/// 16 ms" are different epistemic objects and do not flatten to one
/// representation. They age differently, they compose with different
/// confidence, and a reviewer will ask which you had.
///
/// A term carries two of these: one for its width
/// ([`Term::bias_provenance`]) and one for its adjustment
/// ([`Correction::provenance`]). They are routinely different.
///
/// Every variant but [`Provenance::Unknown`] names an identifier that resolves
/// in the record carrying the term (R21).
///
/// # The integers
///
/// A wire contract, as [`crate::LinkKind`]'s are. `Unknown` is **zero**, so
/// that a field a decoder never filled reads as the absence of knowledge rather
/// than as a measurement. Getting that the other way round would let a
/// truncated record claim provenance it never had.
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Provenance {
    /// No knowledge. As a bias provenance it implies an unbounded bias on both
    /// sides (R10).
    #[default]
    Unknown = 0,
    /// Externally measured, by the calibration this names.
    Measured {
        /// The calibration.
        calibration: CalibrationId,
    } = 1,
    /// Claimed by a document — a vendor figure, unverified.
    Specified {
        /// The document.
        document: DocumentId,
    } = 2,
    /// Computed, by a **versioned** estimator over stated inputs (R22, R65).
    Estimated {
        /// The estimator, including its version.
        estimator: EstimatorId,
        /// What it ran on.
        inputs: InputsId,
    } = 3,
    /// A reasoned worst case, resting on a stored argument.
    ///
    /// **The argument must state its assumption, not its arithmetic.** This is
    /// the one place in the model that can produce a confidently wrong number,
    /// and the mechanism is always the same: a bound that is conditional gets
    /// recorded as though it were unconditional, because the condition lived in
    /// someone's head rather than in the record.
    ///
    /// The motivating case is a round-trip bound of `±min_rtt / 2`. The
    /// arithmetic is correct and the bound is tight — *if the path is
    /// symmetric*. Asymmetric paths are ordinary: radio, and anything whose
    /// uplink and downlink differ. Nothing in the observations reveals the
    /// asymmetry, so the fit cannot detect it and the number looks exactly as
    /// good as a sound one. An argument recorded as "half the minimum
    /// round-trip time" leaves a reader no way to know the claim was
    /// conditional; one recorded as "the path is symmetric, so half the minimum
    /// round-trip bounds the one-way delay" leaves them a condition to check
    /// against their rig.
    ///
    /// A worst case nobody wrote down is not `Bounded`; it is
    /// [`Provenance::Unknown`].
    Bounded {
        /// The argument, which states the assumption the bound rests on.
        argument: ArgumentId,
    } = 4,
}

impl Provenance {
    /// The integer this provenance is written to a record as.
    pub const fn kind(self) -> u16 {
        match self {
            Provenance::Unknown => 0,
            Provenance::Measured { .. } => 1,
            Provenance::Specified { .. } => 2,
            Provenance::Estimated { .. } => 3,
            Provenance::Bounded { .. } => 4,
        }
    }

    /// The identifier this provenance names, where it names one.
    ///
    /// [`Provenance::Estimated`] names two; this answers the estimator, and
    /// [`Provenance::inputs`] answers the other. The pair is what the C ABI
    /// projection carries as `kind: u16` plus `id: u32`.
    pub const fn id(self) -> Option<u32> {
        match self {
            Provenance::Unknown => None,
            Provenance::Measured { calibration } => Some(calibration.0),
            Provenance::Specified { document } => Some(document.0),
            Provenance::Estimated { estimator, .. } => Some(estimator.0),
            Provenance::Bounded { argument } => Some(argument.0),
        }
    }

    /// The inputs identifier, which only [`Provenance::Estimated`] carries.
    pub const fn inputs(self) -> Option<InputsId> {
        match self {
            Provenance::Estimated { inputs, .. } => Some(inputs),
            _ => None,
        }
    }

    /// The calibration identifier, which only [`Provenance::Measured`] carries.
    pub const fn calibration(self) -> Option<CalibrationId> {
        match self {
            Provenance::Measured { calibration } => Some(calibration),
            _ => None,
        }
    }

    /// True for [`Provenance::Unknown`], whose bias is unbounded on both sides
    /// whatever the term's `bias` field says (R10).
    pub const fn is_unknown(self) -> bool {
        matches!(self, Provenance::Unknown)
    }
}

/// One named contribution to the uncertainty of one interval.
///
/// `Copy` and fixed-size, with every identifier an integer (R18).
///
/// # The fields are public, and the composer does not trust them
///
/// A caller can write `Term { bias_provenance: Provenance::Unknown, bias:
/// Bias::NONE, .. }` with a struct literal. R10 says a term whose provenance is
/// unknown cannot carry a finite bound, so the composer reads
/// [`Term::effective_bias`] rather than `bias` — enforcing the rule where the
/// number is used rather than only where it is written. A rule enforced only at
/// construction is a rule a struct literal walks past.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Term {
    /// The span of one chain this term accounts for.
    ///
    /// A span, not a position: a recorder taking one stamp that covers both
    /// [`HostReceive`](crate::LinkKind::HostReceive) and
    /// [`HostQueue`](crate::LinkKind::HostQueue) attributes one term to two
    /// links, and a trigger alignment covers a contiguous run of them.
    pub covers: Span,
    /// The worst-case interval this stage contributes.
    pub bias: Bias,
    /// How the width was arrived at.
    pub bias_provenance: Provenance,
    /// The adjustment applied to the value, where one was applied (R5).
    ///
    /// `None` is "no correction"; `Some(Correction { ns: 0, .. })` is a claim
    /// that the right adjustment is zero, and names who says so.
    pub correction: Option<Correction>,
    /// The trial-to-trial spread, where one is known.
    pub dispersion: Option<Dispersion>,
    /// What this term declares about its relationship to the others.
    pub correlation: Correlation,
}

impl Term {
    /// A term covering this span about which nothing is known.
    ///
    /// The bias is [`Bias::UNKNOWN`] and the bias provenance is
    /// [`Provenance::Unknown`], which is what an undescribed stage contributes
    /// and what poisons a total (R8, R10).
    pub const fn unknown(covers: Span) -> Term {
        Term {
            covers,
            bias: Bias::UNKNOWN,
            bias_provenance: Provenance::Unknown,
            correction: None,
            dispersion: None,
            correlation: Correlation::Undeclared,
        }
    }

    /// A term covering this span with this bias and bias provenance, no
    /// correction, no dispersion and an undeclared correlation.
    ///
    /// Passing [`Provenance::Unknown`] here gives a term whose effective bias
    /// is [`Bias::UNKNOWN`] whatever `bias` said, by R10.
    pub const fn new(covers: Span, bias: Bias, bias_provenance: Provenance) -> Term {
        Term {
            covers,
            bias,
            bias_provenance,
            correction: None,
            dispersion: None,
            correlation: Correlation::Undeclared,
        }
    }

    /// The same term, carrying this correction.
    #[must_use]
    pub const fn with_correction(mut self, correction: Correction) -> Term {
        self.correction = Some(correction);
        self
    }

    /// The same term, carrying this dispersion.
    #[must_use]
    pub const fn with_dispersion(mut self, dispersion: Dispersion) -> Term {
        self.dispersion = Some(dispersion);
        self
    }

    /// The same term, declaring this correlation.
    #[must_use]
    pub const fn with_correlation(mut self, correlation: Correlation) -> Term {
        self.correlation = correlation;
        self
    }

    /// The bias the composer uses: `bias`, unless the bias provenance is
    /// [`Provenance::Unknown`], in which case [`Bias::UNKNOWN`] (R10).
    ///
    /// The one place the rule is applied, so that no public path composes a
    /// finite width out of a term that admits it knows nothing.
    pub const fn effective_bias(self) -> Bias {
        if self.bias_provenance.is_unknown() {
            Bias::UNKNOWN
        } else {
            self.bias
        }
    }

    /// The nanoseconds this term adds to its endpoint's value, `0` when it
    /// carries no correction.
    pub const fn correction_ns(self) -> i64 {
        match self.correction {
            Some(correction) => correction.ns,
            None => 0,
        }
    }

    /// True when this term moves the value.
    ///
    /// Two of these over intersecting spans are refused (R27): a trigger-based
    /// alignment already contains the device delay, and applying both corrects
    /// twice. Two *biases* over one span are not this — a clock mapping's
    /// residual bound and its estimated offset live in one term, and a stage
    /// bounded twice by two arguments is over-conservative rather than wrong.
    pub const fn corrects(self) -> bool {
        self.correction.is_some()
    }
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
    use crate::ids::ChainId;

    fn span() -> Span {
        Span::at(ChainId(1), 0)
    }

    fn argued() -> Provenance {
        Provenance::Bounded {
            argument: ArgumentId(1),
        }
    }

    #[test]
    fn widths_add_side_by_side_along_a_chain() {
        let a = Bias::new(10, 20);
        let b = Bias::new(3, 4);
        assert_eq!(a.saturating_add(b), Bias::new(13, 24));
    }

    #[test]
    fn widths_add_crosswise_across_a_difference() {
        let later = Bias::new(10, 20);
        let earlier = Bias::new(3, 4);
        assert_eq!(later.crosswise(earlier), Bias::new(14, 23));
    }

    #[test]
    fn addition_saturates_at_unbounded() {
        assert_eq!(
            Bias::new(UNBOUNDED, 0).saturating_add(Bias::new(1, 1)),
            Bias::new(UNBOUNDED, 1)
        );
        assert_eq!(
            Bias::new(1 << 63, 0).saturating_add(Bias::new(1 << 63, 0)),
            Bias::new(UNBOUNDED, 0)
        );
        assert_eq!(
            Bias::new(0, UNBOUNDED).crosswise(Bias::new(0, 5)),
            Bias::new(5, UNBOUNDED)
        );
    }

    #[test]
    fn unknown_needs_both_sides() {
        assert!(Bias::UNKNOWN.is_unknown());
        assert!(!Bias::new(UNBOUNDED, 0).is_unknown());
        assert!(Bias::new(UNBOUNDED, 0).early_is_unbounded());
        assert!(!Bias::NONE.is_unknown());
    }

    #[test]
    fn an_unknown_bias_provenance_overrides_a_finite_bias() {
        let lying = Term {
            covers: span(),
            bias: Bias::NONE,
            bias_provenance: Provenance::Unknown,
            correction: None,
            dispersion: None,
            correlation: Correlation::Undeclared,
        };
        assert_eq!(lying.bias, Bias::NONE);
        assert_eq!(lying.effective_bias(), Bias::UNKNOWN);
    }

    #[test]
    fn a_one_way_fit_is_one_term_with_two_provenances() {
        // The fitted offset is an estimate; the residual bound rests on an
        // argument about transport delay. One span, one term, two provenances.
        let fit =
            Term::new(span(), Bias::new(UNBOUNDED, 0), argued()).with_correction(Correction::new(
                -4_200,
                Provenance::Estimated {
                    estimator: EstimatorId(1),
                    inputs: InputsId(2),
                },
            ));
        assert_eq!(fit.bias_provenance.kind(), 4);
        assert_eq!(fit.correction.map(|c| c.provenance.kind()), Some(3));
        assert_eq!(fit.correction_ns(), -4_200);
    }

    #[test]
    fn no_correction_and_a_zero_correction_are_different_claims() {
        let none = Term::new(span(), Bias::NONE, argued());
        let zero = none.with_correction(Correction::new(0, argued()));
        assert_ne!(none, zero);
        assert!(!none.corrects());
        assert!(zero.corrects());
        assert_eq!(none.correction_ns(), zero.correction_ns());
    }

    #[test]
    fn the_enumeration_integers_are_the_wire_contract() {
        assert_eq!(Provenance::Unknown.kind(), 0);
        assert_eq!(
            Provenance::Measured {
                calibration: CalibrationId(9)
            }
            .kind(),
            1
        );
        assert_eq!(
            Provenance::Specified {
                document: DocumentId(9)
            }
            .kind(),
            2
        );
        assert_eq!(
            Provenance::Estimated {
                estimator: EstimatorId(3),
                inputs: InputsId(4)
            }
            .kind(),
            3
        );
        assert_eq!(
            Provenance::Bounded {
                argument: ArgumentId(9)
            }
            .kind(),
            4
        );
        assert_eq!(Correlation::Undeclared.kind(), 0);
        assert_eq!(Correlation::Independent.kind(), 1);
        assert_eq!(Correlation::CorrelatedWith(GroupId(2)).kind(), 2);
        assert_eq!(DistributionKind::Unspecified.kind(), 0);
        assert_eq!(DistributionKind::Gaussian.kind(), 1);
        assert_eq!(DistributionKind::Uniform.kind(), 2);
    }

    #[test]
    fn a_provenance_answers_the_identifier_it_names() {
        let estimated = Provenance::Estimated {
            estimator: EstimatorId(3),
            inputs: InputsId(4),
        };
        assert_eq!(estimated.id(), Some(3));
        assert_eq!(estimated.inputs(), Some(InputsId(4)));
        assert_eq!(estimated.calibration(), None);
        assert_eq!(Provenance::Unknown.id(), None);
        assert_eq!(
            Provenance::Measured {
                calibration: CalibrationId(7)
            }
            .calibration(),
            Some(CalibrationId(7))
        );
    }

    #[test]
    fn the_default_correlation_is_undeclared() {
        assert_eq!(Correlation::default(), Correlation::Undeclared);
        assert!(Correlation::default().is_undeclared());
        assert_eq!(Correlation::default().group(), None);
        assert_eq!(
            Correlation::CorrelatedWith(GroupId(5)).group(),
            Some(GroupId(5))
        );
    }
}
