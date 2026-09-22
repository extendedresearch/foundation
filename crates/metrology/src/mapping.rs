//! Mapping a source clock's instant onto a reference clock **with the fit's
//! uncertainty still attached**.
//!
//! # The defect this module exists to fix
//!
//! `extendedresearch_clock::Fit` computes four uncertainty figures —
//! `bias_low_ns`, `bias_high_ns`, `residual_spread_ns` and
//! `skew_uncertainty_ppb` — and `Line::map_to_reference` returns a bare `u64`,
//! discarding all four. A consumer that emits that integer has emitted an
//! estimate that is indistinguishable from a direct clock read. Nothing crashes.
//! The number is analysed, and whoever analyses it has no way to know that its
//! offset bias is unbounded below, that the fit's residual spread is precision
//! rather than accuracy, or that the instant sits an hour outside the window the
//! rate was estimated over.
//!
//! [`Mapping::map`] is the same arithmetic with the uncertainty reported. It
//! calls `Line::map_to_reference` for the value, so the number does not change:
//! what changes is that a [`Reading`] carries it, with a bound, beside the three
//! terms that produced the bound.
//!
//! # Three terms over one span
//!
//! A mapping is one stage of a chain — [`LinkKind::ClockMapping`](crate::LinkKind::ClockMapping)
//! — and the three things a one-way fit can say about it are different kinds of
//! claim, so they are three terms rather than one widened number.
//!
//! | Term | What it is | Where it goes |
//! |---|---|---|
//! | **offset bias** | One-way data cannot separate a constant transport delay from a clock offset. The fitted offset is at most the true offset plus the smallest delay, never less (R40) | A bias of `(UNBOUNDED, 0)`, and the fitted offset as a [`Correction`] |
//! | **residual spread** | The variance the fit removed. It says nothing about how far the line sits from the truth (R41) | A [`Dispersion`], and a bias of zero |
//! | **extrapolation penalty** | The rate estimate's uncertainty, grown by the distance from the fit's origin (R42) | A symmetric bias, or [`Bias::UNKNOWN`] where no rate was estimated |
//!
//! # The offset term carries two provenances, and needs to
//!
//! R17. The fitted offset is a **correction** an estimator computed over stored
//! observations, and the residual bound is an **argument** about transport delay
//! — that it is never negative, so the mapping never places an event too early
//! and nothing bounds how late. One is [`Provenance::Estimated`] and the other
//! is [`Provenance::Bounded`], over one span, in one term. Splitting them into
//! two terms would trip the correction-overlap refusal (R27) and make the fit
//! unrepresentable.
//!
//! # An unestimated rate poisons the total, which is the right answer
//!
//! Where `skew_uncertainty_ppb` is `SKEW_NOT_ESTIMATED` — a window too short, or
//! two envelope points spanning no source time — the extrapolation term is
//! [`Term::unknown`]. It makes any budget containing it unbounded (R8), and that
//! is correct: a mapping whose rate nobody could estimate places an instant at a
//! distance nobody can bound. A zero penalty there would read as "measured, and
//! the clocks agree".
//!
//! # What this module does not do
//!
//! It does not refuse a distant extrapolation. R47's caller-stated limit is a
//! sample clock's, where the fit window and the stream are both in hand; here
//! the penalty grows without a ceiling and the caller reads it. It does not map
//! backwards: `Line::inverse_map` has no uncertainty layer yet.

use std::fmt;

use extendedresearch_clock::{
    Basis, Bound, DomainId, Fit, Line, Reading, SKEW_NOT_ESTIMATED, UNBOUNDED,
};

use crate::chain::Span;
use crate::ids::{ArgumentId, EstimatorId, InputsId};
use crate::term::{Bias, Correction, Dispersion, DistributionKind, Provenance, Term};

/// Nanoseconds per second, which is also the parts-per-billion divisor.
const BILLION: i128 = 1_000_000_000;

/// The identifiers the three terms name, each resolving in the record that
/// carries them (R21).
///
/// Two estimator identifiers would be wrong and one argument identifier would be
/// too few: the offset bound and the zero width of the residual term rest on
/// **different** assumptions, and R19a asks each stored argument to state its
/// assumption rather than its arithmetic. The field documentation below says
/// what each argument has to state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MappingProvenance {
    /// The estimator that produced the fit, **including its version** (R22,
    /// R65).
    pub estimator: EstimatorId,
    /// The observations it ran on, so the fit is reproducible from the record.
    pub inputs: InputsId,
    /// The stored argument the offset bound rests on.
    ///
    /// It has to state the assumption: *a transport delay is never negative, so
    /// the fitted offset is at most the true offset plus the smallest delay*.
    /// That is what makes `(UNBOUNDED, 0)` the measured answer for one-way data
    /// rather than a placeholder — and what tells a reader that a round trip, a
    /// marker the source records in its own timeline, or a calibration is what
    /// narrows it.
    pub offset_argument: ArgumentId,
    /// The stored argument the residual term's **zero bias** rests on.
    ///
    /// It has to state the assumption: *a fit's residual spread is the variance
    /// the fit removed, so it bounds nothing about where the line sits and
    /// contributes no worst-case width*. A recorder that instead widened the
    /// bound by the residual spread would be reporting precision as accuracy,
    /// which R41 forbids and which flatters nothing — it is simply a different
    /// claim.
    pub residual_argument: ArgumentId,
}

/// A source instant mapped onto a reference clock, with the three terms that
/// bound it.
///
/// [`MappedReading::reading`] is what a consumer emits, and its `ns` is bit for
/// bit what `Line::map_to_reference` returns for the same instant. Its
/// [`Bound`] is the three terms' biases added along the chain, so a consumer
/// that ignores the terms entirely still cannot mistake the value for a direct
/// clock read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedReading {
    /// The mapped instant on the reference domain, bounded.
    pub reading: Reading,
    /// The offset term, the residual term and the extrapolation term, in that
    /// order, all covering the mapping's span.
    pub terms: [Term; 3],
}

impl MappedReading {
    /// The offset term: the fitted offset as a correction, and the bound the
    /// one-way argument supports (R40).
    pub const fn offset(&self) -> Term {
        let [offset, _, _] = self.terms;
        offset
    }

    /// The residual term: the fit's residual spread as a dispersion, and no
    /// width (R41).
    pub const fn residual(&self) -> Term {
        let [_, residual, _] = self.terms;
        residual
    }

    /// The extrapolation term: the rate uncertainty grown by the distance from
    /// the fit's origin, or unknown where no rate was estimated (R42).
    pub const fn extrapolation(&self) -> Term {
        let [_, _, extrapolation] = self.terms;
        extrapolation
    }
}

/// Why a mapping refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MapError {
    /// The span the terms would cover runs backwards, so it covers nothing.
    ///
    /// Emitting three terms over a span that covers no position would leave the
    /// mapping's own link uncovered and the budget unbounded for a reason that
    /// has nothing to do with the fit — a refusal here says what actually
    /// happened.
    MalformedSpan {
        /// The span.
        span: Span,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::MalformedSpan { span } => write!(
                f,
                "the mapping covers {}:{}-{}, which runs backwards",
                span.chain.0, span.from, span.to
            ),
        }
    }
}

impl std::error::Error for MapError {}

/// A fitted clock mapping, and the identifiers its terms name.
///
/// Holds the [`Fit`] rather than a [`Line`], because three of the four numbers
/// the terms are built from — `bias_low_ns`, `bias_high_ns`,
/// `residual_spread_ns` and `skew_uncertainty_ppb` — live on the fit and not on
/// the line. A caller holding only a line has, by construction, already thrown
/// the uncertainty away.
///
/// ```
/// use extendedresearch_clock::{Domain, Observation, SuspendBehaviour, fit_one_way};
/// use extendedresearch_metrology::{
///     ArgumentId, ChainId, EstimatorId, InputsId, Mapping, MappingProvenance, Span,
/// };
///
/// let domain = Domain {
///     host_id: "host-1".to_owned(),
///     host_clock_epoch: "boot.1".to_owned(),
///     monotonic_source: "CLOCK_BOOTTIME",
///     suspend: SuspendBehaviour::Included,
///     resolution_ns: Some(1),
///     anchor: None,
/// };
/// // A source one microsecond behind, sampled once a second, running 100 ppb fast.
/// let observations: Vec<Observation> = (0..16u64)
///     .map(|i| Observation {
///         source_ns: 1_000_000_000 + i * 1_000_000_000,
///         receipt_ns: 1_000_000_000 + i * 1_000_000_100 + 1_000,
///     })
///     .collect();
/// let fit = fit_one_way(&observations).expect("a fit");
///
/// let mapping = Mapping::new(
///     fit,
///     Span::at(ChainId(1), 3),
///     domain.id(),
///     MappingProvenance {
///         estimator: EstimatorId(1),
///         inputs: InputsId(2),
///         offset_argument: ArgumentId(3),
///         residual_argument: ArgumentId(4),
///     },
/// );
///
/// let mapped = mapping.map(2_000_000_000)?;
/// // The value is the line's, unchanged.
/// assert_eq!(mapped.reading.as_ns(), fit.line().map_to_reference(2_000_000_000));
/// // And it is no longer mistakable for a clock read: nothing bounds how late.
/// assert_eq!(mapped.reading.bound().early_ns, extendedresearch_metrology::UNBOUNDED);
/// assert_eq!(mapped.offset().bias.early_ns, extendedresearch_metrology::UNBOUNDED);
/// assert_eq!(mapped.offset().bias.late_ns, 0);
/// # Ok::<(), extendedresearch_metrology::MapError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    fit: Fit,
    covers: Span,
    reference_domain: DomainId,
    provenance: MappingProvenance,
}

impl Mapping {
    /// A mapping through `fit`, whose terms cover `covers` and whose readings
    /// are on `reference_domain`.
    ///
    /// `covers` is the span of the chain the mapping is a stage of — one
    /// position for an ordinary [`ClockMapping`](crate::LinkKind::ClockMapping)
    /// link, a run where one term accounts for the mapping and the stages
    /// beside it (R17b).
    pub const fn new(
        fit: Fit,
        covers: Span,
        reference_domain: DomainId,
        provenance: MappingProvenance,
    ) -> Mapping {
        Mapping {
            fit,
            covers,
            reference_domain,
            provenance,
        }
    }

    /// The fit this mapping reports through.
    pub const fn fit(&self) -> Fit {
        self.fit
    }

    /// The span the three terms cover.
    pub const fn covers(&self) -> Span {
        self.covers
    }

    /// The reference domain a mapped reading is on.
    pub const fn reference_domain(&self) -> DomainId {
        self.reference_domain
    }

    /// The identifiers the three terms name.
    pub const fn provenance(&self) -> MappingProvenance {
        self.provenance
    }

    /// The line the value is computed with: `Fit::line`.
    pub fn line(&self) -> Line {
        self.fit.line()
    }

    /// The extrapolation penalty at `source_ns`, in nanoseconds (R42).
    ///
    /// `skew_uncertainty_ppb × |source_ns − reference_source_ns| / 10⁹`,
    /// computed in `u128` and saturating at [`UNBOUNDED`].
    ///
    /// [`UNBOUNDED`] where `skew_uncertainty_ppb` is
    /// [`SKEW_NOT_ESTIMATED`](extendedresearch_clock::SKEW_NOT_ESTIMATED) — not
    /// zero. A window too short to estimate a rate has not measured a rate of
    /// zero, and the two spellings must not be the same number.
    ///
    /// ```
    /// # use extendedresearch_clock::{Fit, SKEW_NOT_ESTIMATED, UNBOUNDED};
    /// # use extendedresearch_metrology::{ArgumentId, ChainId, EstimatorId, InputsId, Mapping, MappingProvenance, Span};
    /// # let provenance = MappingProvenance {
    /// #     estimator: EstimatorId(1), inputs: InputsId(2),
    /// #     offset_argument: ArgumentId(3), residual_argument: ArgumentId(4),
    /// # };
    /// # let domain = extendedresearch_clock::Domain {
    /// #     host_id: "h".to_owned(), host_clock_epoch: "boot.1".to_owned(),
    /// #     monotonic_source: "CLOCK_BOOTTIME",
    /// #     suspend: extendedresearch_clock::SuspendBehaviour::Included,
    /// #     resolution_ns: None, anchor: None,
    /// # }.id();
    /// let fit = Fit {
    ///     reference_source_ns: 1_000_000_000,
    ///     offset_ns: 0,
    ///     skew_ppb: 0,
    ///     skew_uncertainty_ppb: 2_000,      // 2 ppm
    ///     residual_spread_ns: 0,
    ///     observation_count: 16,
    ///     bias_low_ns: UNBOUNDED,
    ///     bias_high_ns: 0,
    /// };
    /// let mapping = Mapping::new(fit, Span::at(ChainId(1), 0), domain, provenance);
    /// // Ten seconds past the origin at 2 ppm is 20 µs.
    /// assert_eq!(mapping.extrapolation_penalty_ns(11_000_000_000), 20_000);
    /// // At the origin itself, nothing.
    /// assert_eq!(mapping.extrapolation_penalty_ns(1_000_000_000), 0);
    ///
    /// let unestimated = Fit { skew_uncertainty_ppb: SKEW_NOT_ESTIMATED, ..fit };
    /// let mapping = Mapping::new(unestimated, Span::at(ChainId(1), 0), domain, provenance);
    /// assert_eq!(mapping.extrapolation_penalty_ns(1_000_000_000), UNBOUNDED);
    /// ```
    pub fn extrapolation_penalty_ns(&self, source_ns: u64) -> u64 {
        if self.fit.skew_uncertainty_ppb == SKEW_NOT_ESTIMATED {
            return UNBOUNDED;
        }
        let distance = source_ns.abs_diff(self.fit.reference_source_ns);
        let grown = u128::from(self.fit.skew_uncertainty_ppb).saturating_mul(u128::from(distance));
        u64::try_from(grown / 1_000_000_000).unwrap_or(UNBOUNDED)
    }

    /// The adjustment the line applies at `source_ns`: the fitted offset plus
    /// the drift over the distance from the origin, saturating.
    ///
    /// The same arithmetic `Line::map_to_reference` documents, stated here as
    /// the correction so that the record says what was added rather than only
    /// what came out. The two differ where the line saturates or where its
    /// result is clamped at zero, and the mapped value is always the line's.
    fn applied_ns(&self, source_ns: u64) -> i64 {
        let line = self.line();
        let elapsed = (source_ns as i64).saturating_sub(line.reference_source_ns as i64);
        let drift = (i128::from(elapsed) * i128::from(line.skew_ppb) / BILLION) as i64;
        line.offset_ns.saturating_add(drift)
    }

    /// Map a source instant onto the reference clock, carrying the fit's
    /// uncertainty (R39 to R42).
    ///
    /// The value is `Line::map_to_reference(source_ns)`, called rather than
    /// re-derived: this layer changes what is *reported* and never what is
    /// *computed*, and `tests/properties.rs` holds that as a differential
    /// property over generated lines and instants rather than as examples.
    ///
    /// The reading's [`Bound`] is the three terms' biases added along the chain
    /// and its [`Basis`] is [`Basis::Unspecified`]: a mapped instant was not
    /// taken at any chain position — it is a function of a stamp and a fit — and
    /// no `Basis` value names a mapping. What says it is one is the chain, which
    /// carries a [`ClockMapping`](crate::LinkKind::ClockMapping) link, and the
    /// bound, which is unbounded below.
    ///
    /// # Errors
    ///
    /// [`MapError::MalformedSpan`] when the span the terms would cover runs
    /// backwards.
    pub fn map(&self, source_ns: u64) -> Result<MappedReading, MapError> {
        let covers = self.covers;
        if !covers.is_well_formed() {
            return Err(MapError::MalformedSpan { span: covers });
        }
        let estimated = Provenance::Estimated {
            estimator: self.provenance.estimator,
            inputs: self.provenance.inputs,
        };

        // R40. The one-way bound, with the fitted offset as a correction whose
        // provenance is the estimator's rather than the argument's (R17).
        let offset = Term::new(
            covers,
            Bias::new(self.fit.bias_low_ns, self.fit.bias_high_ns),
            Provenance::Bounded {
                argument: self.provenance.offset_argument,
            },
        )
        .with_correction(Correction::new(self.applied_ns(source_ns), estimated));

        // R41. Precision, not accuracy: a dispersion, and no width. The
        // distribution is unspecified because a root mean square of residuals
        // about a fitted line is not a distribution anybody declared, and
        // calling it Gaussian would licence a coverage factor nobody derived.
        let residual = Term::new(
            covers,
            Bias::NONE,
            Provenance::Bounded {
                argument: self.provenance.residual_argument,
            },
        )
        .with_dispersion(Dispersion::new(
            self.fit.residual_spread_ns,
            DistributionKind::Unspecified,
        ));

        // R42. The rate uncertainty grown by the distance, or nothing claimed
        // at all where no rate was estimated.
        let penalty = self.extrapolation_penalty_ns(source_ns);
        let extrapolation = if penalty == UNBOUNDED {
            Term::unknown(covers)
        } else {
            Term::new(covers, Bias::symmetric(penalty), estimated)
        };

        let bias = offset
            .effective_bias()
            .saturating_add(residual.effective_bias())
            .saturating_add(extrapolation.effective_bias());

        Ok(MappedReading {
            reading: Reading::new(
                self.line().map_to_reference(source_ns),
                self.reference_domain,
                Bound {
                    early_ns: bias.early_ns,
                    late_ns: bias.late_ns,
                    basis: Basis::Unspecified,
                },
            ),
            terms: [offset, residual, extrapolation],
        })
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
    use crate::budget::{Composer, Total};
    use crate::chain::{Chain, Link, LinkKind};
    use crate::ids::ChainId;
    use extendedresearch_clock::{Domain, SuspendBehaviour};

    const COVERS: Span = Span::at(ChainId(1), 0);

    fn domain() -> DomainId {
        Domain {
            host_id: "fixture-host".to_owned(),
            host_clock_epoch: "boot.1".to_owned(),
            monotonic_source: "CLOCK_BOOTTIME",
            suspend: SuspendBehaviour::Included,
            resolution_ns: None,
            anchor: None,
        }
        .id()
    }

    fn provenance() -> MappingProvenance {
        MappingProvenance {
            estimator: EstimatorId(1),
            inputs: InputsId(2),
            offset_argument: ArgumentId(3),
            residual_argument: ArgumentId(4),
        }
    }

    fn fit(skew_ppb: i64, skew_uncertainty_ppb: u64, residual_spread_ns: u64) -> Fit {
        Fit {
            reference_source_ns: 1_000_000_000,
            offset_ns: 5_000_000,
            skew_ppb,
            skew_uncertainty_ppb,
            residual_spread_ns,
            observation_count: 16,
            bias_low_ns: UNBOUNDED,
            bias_high_ns: 0,
        }
    }

    fn mapping(fit: Fit) -> Mapping {
        Mapping::new(fit, COVERS, domain(), provenance())
    }

    #[test]
    fn the_value_is_the_lines_and_nothing_else() {
        let m = mapping(fit(100, 2_000, 30_000));
        for source_ns in [0, 1, 1_000_000_000, 11_000_000_000, u64::MAX] {
            assert_eq!(
                m.map(source_ns).expect("maps").reading.as_ns(),
                m.line().map_to_reference(source_ns),
                "{source_ns}"
            );
        }
    }

    #[test]
    fn the_offset_term_is_bounded_one_way_and_corrected_by_an_estimate() {
        let mapped = mapping(fit(100, 2_000, 30_000))
            .map(11_000_000_000)
            .expect("maps");
        let offset = mapped.offset();
        assert_eq!(offset.bias, Bias::new(UNBOUNDED, 0));
        assert_eq!(offset.bias_provenance.kind(), 4); // Bounded
        // Ten seconds past the origin at 100 ppb is 1 000 ns of drift.
        assert_eq!(offset.correction_ns(), 5_000_000 + 1_000);
        assert_eq!(
            offset.correction.map(|c| c.provenance.kind()),
            Some(3) // Estimated
        );
        // And the correction is exactly what the value moved by.
        assert_eq!(
            mapped.reading.as_ns(),
            11_000_000_000 + 5_000_000 + 1_000_u64
        );
    }

    #[test]
    fn the_residual_spread_is_a_dispersion_and_never_a_width() {
        let mapped = mapping(fit(0, 2_000, 400_000))
            .map(1_000_000_000)
            .expect("maps");
        let residual = mapped.residual();
        assert_eq!(residual.bias, Bias::NONE);
        assert_eq!(
            residual.dispersion,
            Some(Dispersion::new(400_000, DistributionKind::Unspecified))
        );
        // Widening the bound by the spread would be reporting precision as
        // accuracy, which is the merge R41 forbids.
        assert_eq!(mapped.reading.bound().late_ns, 0);
    }

    #[test]
    fn the_extrapolation_penalty_grows_with_the_distance() {
        let m = mapping(fit(0, 2_000, 0));
        let at_origin = m.map(1_000_000_000).expect("maps");
        assert_eq!(at_origin.extrapolation().bias, Bias::symmetric(0));
        let ten_seconds_out = m.map(11_000_000_000).expect("maps");
        assert_eq!(
            ten_seconds_out.extrapolation().bias,
            Bias::symmetric(20_000)
        );
        // Symmetric, and on both sides of the reading's bound.
        assert_eq!(ten_seconds_out.reading.bound().late_ns, 20_000);
        assert_eq!(ten_seconds_out.reading.bound().early_ns, UNBOUNDED);
        // Behind the origin is the same distance.
        assert_eq!(
            m.map(0).expect("maps").extrapolation().bias,
            Bias::symmetric(2_000)
        );
    }

    #[test]
    fn the_penalty_saturates_rather_than_wrapping() {
        let m = mapping(fit(0, u64::MAX - 1, 0));
        assert_eq!(m.extrapolation_penalty_ns(u64::MAX), UNBOUNDED);
    }

    #[test]
    fn an_unestimated_rate_is_unknown_and_not_zero() {
        let mapped = mapping(fit(0, SKEW_NOT_ESTIMATED, 0))
            .map(1_000_000_000)
            .expect("maps");
        let extrapolation = mapped.extrapolation();
        assert_eq!(extrapolation.bias, Bias::UNKNOWN);
        assert!(extrapolation.bias_provenance.is_unknown());
        assert_eq!(mapped.reading.bound(), Bound::UNKNOWN);
        // Which is exactly what a budget has to refuse to total.
        assert!(mapped.reading.bound().is_unknown());
    }

    #[test]
    fn the_three_terms_compose_without_tripping_the_overlap_refusal() {
        // Three terms over one span, one of them correcting: accepted (R27).
        let mapped = mapping(fit(0, 2_000, 400_000))
            .map(11_000_000_000)
            .expect("maps");
        let later = Chain::new(ChainId(1), [Link::of(LinkKind::ClockMapping)]);
        let earlier = Chain::new(ChainId(2), [Link::of(LinkKind::Emission)]);
        let mut composer = Composer::new(later, earlier);
        composer
            .terms(mapped.terms)
            .term(Term::new(
                Span::at(ChainId(2), 0),
                Bias::NONE,
                Provenance::Bounded {
                    argument: ArgumentId(9),
                },
            ))
            .stamps(mapped.reading.as_ns(), 0);
        let budget = composer.compose().expect("composes");
        // Nothing bounds how late the source event was, and 20 µs of
        // extrapolation bounds how early.
        assert!(matches!(budget.total, Total::Unbounded { .. }));
        assert_eq!(budget.total.early_ns(), None);
        assert_eq!(budget.total.late_ns(), Some(20_000));
    }

    #[test]
    fn a_span_that_covers_nothing_is_refused() {
        let backwards = Span {
            chain: ChainId(1),
            from: 5,
            to: 4,
        };
        let m = Mapping::new(fit(0, 0, 0), backwards, domain(), provenance());
        assert_eq!(
            m.map(1_000_000_000),
            Err(MapError::MalformedSpan { span: backwards })
        );
        assert!(
            m.map(1_000_000_000)
                .expect_err("refuses")
                .to_string()
                .contains("1:5-4")
        );
    }

    #[test]
    fn the_accessors_answer_the_three_terms_in_order() {
        let mapped = mapping(fit(0, 2_000, 400_000))
            .map(1_000_000_000)
            .expect("maps");
        assert_eq!(mapped.offset(), mapped.terms[0]);
        assert_eq!(mapped.residual(), mapped.terms[1]);
        assert_eq!(mapped.extrapolation(), mapped.terms[2]);
        // And all three cover the mapping's own span.
        for term in mapped.terms {
            assert_eq!(term.covers, COVERS);
        }
    }

    #[test]
    fn the_mapping_reports_what_it_was_built_from() {
        let m = mapping(fit(100, 2_000, 30_000));
        assert_eq!(m.fit(), fit(100, 2_000, 30_000));
        assert_eq!(m.covers(), COVERS);
        assert_eq!(m.reference_domain(), domain());
        assert_eq!(m.provenance(), provenance());
        assert_eq!(m.line(), fit(100, 2_000, 30_000).line());
    }
}
