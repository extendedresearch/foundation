//! Properties of the composer and of the clock mapping, checked over generated
//! inputs with a seeded generator and no property-testing dependency.
//!
//! # The one that matters
//!
//! [`an_unknown_term_never_produces_a_finite_total`] is the negative test this
//! crate exists to pass. A budget holding one unknown term must never produce a
//! finite total — in any composition order, through any public API. It is
//! written over generated term sets rather than as three examples, because
//! three examples check three orders and the failure mode is an order nobody
//! wrote down: an accumulator that reaches the sentinel and then has a finite
//! width added to it, a fold whose identity is zero, a `max` that discards an
//! unbounded operand, an accessor that falls back to the partial sum when the
//! total is missing. Each of those passes a hand-written example and fails on
//! some permutation of some term set.
//!
//! So the test enumerates **every** public way to read a width out of a budget,
//! shuffles the terms into fresh orders, and asserts that none of them yields a
//! number on a poisoned side.
//!
//! # The differential one
//!
//! [`a_mapping_reports_exactly_what_the_line_computes`] is the property that
//! keeps the new layer honest: for every input `Line::map_to_reference` accepts,
//! `Mapping::map(..).reading.ns` equals it bit for bit. `Mapping` changes what is
//! *reported* and never what is *computed*, and the way that rule breaks is a
//! re-derivation of the arithmetic that agrees on the cases someone wrote down —
//! an instant at the origin, a positive skew, a small offset — and disagrees at
//! a saturating addition, at the clamp to zero, or where a source instant at or
//! above 2⁶³ reinterprets as negative. So it is generated over those edges
//! rather than exampled.
//!
//! # The rest
//!
//! Composition is order-independent, adding a term never narrows a total, and a
//! bounded total equals an independent re-derivation of the sum in `u128`. The
//! last one is the check that the saturating arithmetic is not quietly
//! wrapping: `u128` cannot overflow at these magnitudes, so the two agree
//! unless the `u64` path is wrong.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use extendedresearch_clock::{Domain, Fit, Line, SKEW_NOT_ESTIMATED, SuspendBehaviour};
use extendedresearch_metrology::{
    ArgumentId, Bias, Budget, CalibrationId, Chain, ChainId, Composer, Correction, Correlation,
    Dispersion, DistributionKind, DocumentId, EstimatorId, InputsId, Link, LinkKind, Mapping,
    MappingProvenance, Provenance, Span, Term, Total, UNBOUNDED,
};

/// SplitMix64: small, seeded, and the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[low, high]`.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        low + self.next() % (high - low + 1)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }

    /// Fisher-Yates, so every permutation is reachable.
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            let other = (self.next() % (index as u64 + 1)) as usize;
            items.swap(index, other);
        }
    }
}

const LATER: ChainId = ChainId(1);
const EARLIER: ChainId = ChainId(2);

fn chain(id: ChainId, links: usize) -> Chain {
    Chain::new(id, vec![Link::of(LinkKind::Transport); links])
}

/// A provenance that is not `Unknown`, so a generated bias means what it says.
fn known_provenance(rng: &mut Rng) -> Provenance {
    match rng.range(0, 3) {
        0 => Provenance::Measured {
            calibration: CalibrationId(1),
        },
        1 => Provenance::Specified {
            document: DocumentId(2),
        },
        2 => Provenance::Estimated {
            estimator: EstimatorId(3),
            inputs: InputsId(4),
        },
        _ => Provenance::Bounded {
            argument: ArgumentId(5),
        },
    }
}

/// A width that is finite most of the time and `UNBOUNDED` sometimes, so both
/// the bounded and the poisoned paths are exercised.
fn width(rng: &mut Rng, allow_unbounded: bool) -> u64 {
    if allow_unbounded && rng.range(0, 5) == 0 {
        UNBOUNDED
    } else {
        rng.pick(&[0, 1, 7, 1_000, 65_537, 1_000_000_000, 1 << 40])
    }
}

/// A term set that covers both chains completely: each chain's positions are
/// cut into runs, and each run gets one term. Terms never carry a correction,
/// so no generated set trips the overlap refusal.
fn covering_terms(
    rng: &mut Rng,
    later_links: u16,
    earlier_links: u16,
    unbounded: bool,
) -> Vec<Term> {
    let mut terms = Vec::new();
    for (id, links) in [(LATER, later_links), (EARLIER, earlier_links)] {
        let mut from = 0u16;
        while from < links {
            let to = from + u16::try_from(rng.range(0, u64::from(links - from - 1))).expect("u16");
            let mut term = Term::new(
                Span {
                    chain: id,
                    from,
                    to,
                },
                Bias::new(width(rng, unbounded), width(rng, unbounded)),
                known_provenance(rng),
            );
            if rng.range(0, 2) == 0 {
                term = term
                    .with_dispersion(Dispersion::new(
                        rng.range(0, 100_000),
                        rng.pick(&[
                            DistributionKind::Unspecified,
                            DistributionKind::Gaussian,
                            DistributionKind::Uniform,
                        ]),
                    ))
                    .with_correlation(
                        rng.pick(&[Correlation::Undeclared, Correlation::Independent]),
                    );
            }
            terms.push(term);
            from = to + 1;
        }
    }
    terms
}

fn compose(terms: &[Term], later_links: u16, earlier_links: u16) -> Budget {
    let mut composer = Composer::new(
        chain(LATER, usize::from(later_links)),
        chain(EARLIER, usize::from(earlier_links)),
    );
    composer.terms(terms.iter().copied());
    composer.compose().expect("a covering term set composes")
}

/// Every public way to read a width out of a budget, as `(name, early, late)`
/// with `None` where the API declines to give a number.
///
/// A new accessor that answers a width belongs here. The point of listing them
/// is that "through any public API" is checked rather than asserted.
fn every_width(budget: &Budget) -> Vec<(&'static str, Option<u64>, Option<u64>)> {
    let mut readings = vec![(
        "Total::early_ns / late_ns",
        budget.total.early_ns(),
        budget.total.late_ns(),
    )];
    match &budget.total {
        Total::Bounded { early_ns, late_ns } => {
            readings.push(("Total::Bounded fields", Some(*early_ns), Some(*late_ns)));
        }
        Total::Unbounded {
            known_early_ns,
            known_late_ns,
            early_unbounded,
            late_unbounded,
            ..
        } => {
            // The partial sums are only a total on a side nothing poisoned.
            readings.push((
                "Total::Unbounded known fields",
                (!early_unbounded).then_some(*known_early_ns),
                (!late_unbounded).then_some(*known_late_ns),
            ));
        }
    }
    readings
}

// ---- the negative property ---------------------------------------------------

/// A budget containing one unknown term never produces a finite total, in any
/// composition order, through any public API (R8, R10, architecture §9).
#[test]
fn an_unknown_term_never_produces_a_finite_total() {
    let mut rng = Rng(0x5EED_1234_ABCD_0001);
    let mut cases = 0;
    let mut poisoned_sides = 0;
    for _ in 0..3_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let mut terms = covering_terms(&mut rng, later_links, earlier_links, false);

        // Poison exactly one term, in one of the three ways a term can be
        // unknown: the honest one, the one that lies in its bias field, and the
        // one that is unbounded on a single side.
        let victim = (rng.next() as usize) % terms.len();
        let (early_poisoned, late_poisoned) = match rng.range(0, 3) {
            0 => {
                terms[victim] = Term::unknown(terms[victim].covers);
                (true, true)
            }
            1 => {
                // A finite bias under an unknown provenance is still unknown.
                terms[victim] = Term::new(terms[victim].covers, Bias::NONE, Provenance::Unknown);
                (true, true)
            }
            2 => {
                terms[victim].bias = Bias::new(UNBOUNDED, terms[victim].bias.late_ns);
                (
                    terms[victim].covers.chain == LATER,
                    terms[victim].covers.chain == EARLIER,
                )
            }
            _ => {
                terms[victim].bias = Bias::new(terms[victim].bias.early_ns, UNBOUNDED);
                (
                    terms[victim].covers.chain == EARLIER,
                    terms[victim].covers.chain == LATER,
                )
            }
        };

        // Every order, not one: a rule that holds only for the order the terms
        // happened to arrive in is not a rule.
        for _ in 0..6 {
            rng.shuffle(&mut terms);
            let budget = compose(&terms, later_links, earlier_links);
            cases += 1;

            assert!(
                !budget.total.is_bounded(),
                "a budget with an unknown term reported a bounded total: {budget:?}"
            );
            for (api, early, late) in every_width(&budget) {
                if early_poisoned {
                    assert_eq!(early, None, "{api} answered an early width for {budget:?}");
                }
                if late_poisoned {
                    assert_eq!(late, None, "{api} answered a late width for {budget:?}");
                }
            }
            // The term is named, so a reader can see which one it was (R33).
            assert!(
                !budget.total.unbounded_terms().is_empty(),
                "the unbounded term was not named: {budget:?}"
            );
            if early_poisoned {
                assert!(!budget.unbounded_early_terms().is_empty());
                poisoned_sides += 1;
            }
            if late_poisoned {
                assert!(!budget.unbounded_late_terms().is_empty());
                poisoned_sides += 1;
            }
        }
    }
    assert!(cases >= 18_000, "only {cases} compositions checked");
    assert!(
        poisoned_sides >= 18_000,
        "only {poisoned_sides} sides checked"
    );
}

/// A link no term covers is the same poison, checked the same way: removing any
/// one term from a covering set leaves a total that is unbounded on both sides
/// (R11).
#[test]
fn a_link_no_term_covers_never_produces_a_finite_total() {
    let mut rng = Rng(0x5EED_1234_ABCD_0002);
    let mut cases = 0;
    for _ in 0..2_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let mut terms = covering_terms(&mut rng, later_links, earlier_links, false);
        let dropped = (rng.next() as usize) % terms.len();
        let hole = terms.remove(dropped).covers;

        rng.shuffle(&mut terms);
        let budget = compose(&terms, later_links, earlier_links);
        cases += 1;

        assert!(!budget.total.is_bounded(), "{budget:?}");
        assert_eq!(budget.total.early_ns(), None, "{budget:?}");
        assert_eq!(budget.total.late_ns(), None, "{budget:?}");
        // The hole is reported, and every position of it is inside a reported
        // run. Runs are maximal, so a run may be wider than the dropped span.
        for position in hole.from..=hole.to {
            assert!(
                budget
                    .total
                    .uncovered_links()
                    .iter()
                    .any(|run| run.chain == hole.chain && run.contains(position)),
                "position {position} of {hole:?} was not reported uncovered: {budget:?}"
            );
        }
    }
    assert!(cases >= 2_000, "only {cases} compositions checked");
}

// ---- the positive properties --------------------------------------------------

/// Composition is independent of the order terms were submitted in (R30).
#[test]
fn the_submission_order_never_changes_the_budget() {
    let mut rng = Rng(0x5EED_1234_ABCD_0003);
    let mut cases = 0;
    for _ in 0..2_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let mut terms = covering_terms(&mut rng, later_links, earlier_links, true);
        let reference = compose(&terms, later_links, earlier_links);
        for _ in 0..4 {
            rng.shuffle(&mut terms);
            assert_eq!(
                compose(&terms, later_links, earlier_links),
                reference,
                "a permutation composed differently"
            );
            cases += 1;
        }
    }
    assert!(cases >= 8_000, "only {cases} permutations checked");
}

/// Adding a term never narrows a total, on either side (R3's addition is
/// monotone, and saturation only widens).
#[test]
fn adding_a_term_never_narrows_a_total() {
    let mut rng = Rng(0x5EED_1234_ABCD_0004);
    let mut cases = 0;
    for _ in 0..3_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let terms = covering_terms(&mut rng, later_links, earlier_links, true);
        let before = compose(&terms, later_links, earlier_links);

        // An extra term over an already-covered span, carrying no correction so
        // it cannot trip the overlap refusal.
        let host = terms[(rng.next() as usize) % terms.len()];
        let mut extra = terms.clone();
        extra.push(Term::new(
            host.covers,
            Bias::new(width(&mut rng, true), width(&mut rng, true)),
            known_provenance(&mut rng),
        ));
        let after = compose(&extra, later_links, earlier_links);
        cases += 1;

        for (before_side, after_side) in [
            (before.total.early_ns(), after.total.early_ns()),
            (before.total.late_ns(), after.total.late_ns()),
        ] {
            match (before_side, after_side) {
                // Bounded before, bounded after: never smaller.
                (Some(was), Some(now)) => assert!(now >= was, "{was} narrowed to {now}"),
                // Bounded before, unbounded after: widened to no bound at all.
                (Some(_), None) => {}
                // Unbounded before: it cannot become bounded by adding a term.
                (None, now) => assert_eq!(now, None, "an unbounded side became {now:?}"),
            }
        }
    }
    assert!(cases >= 3_000, "only {cases} additions checked");
}

/// A bounded total equals an independent re-derivation of the crosswise sum in
/// `u128`, which cannot overflow at these magnitudes.
#[test]
fn a_bounded_total_is_the_crosswise_sum_of_its_parts() {
    let mut rng = Rng(0x5EED_1234_ABCD_0005);
    let mut bounded = 0;
    for _ in 0..4_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let terms = covering_terms(&mut rng, later_links, earlier_links, true);
        let budget = compose(&terms, later_links, earlier_links);
        let Total::Bounded { early_ns, late_ns } = budget.total else {
            continue;
        };
        bounded += 1;

        let (mut early, mut late) = (0u128, 0u128);
        for term in &budget.terms {
            let bias = term.effective_bias();
            if term.covers.chain == LATER {
                early += u128::from(bias.early_ns);
                late += u128::from(bias.late_ns);
            } else {
                early += u128::from(bias.late_ns);
                late += u128::from(bias.early_ns);
            }
        }
        assert_eq!(u128::from(early_ns), early, "early side");
        assert_eq!(u128::from(late_ns), late, "late side");
        // A bounded total names no unbounded term and no uncovered link.
        assert!(budget.total.unbounded_terms().is_empty());
        assert!(budget.total.uncovered_links().is_empty());
        // And the partial sums are the total, which is the one case where they
        // may be read as one.
        assert_eq!(budget.total.partial_sums(), Bias::new(early_ns, late_ns));
    }
    assert!(bounded >= 500, "only {bounded} bounded budgets seen");
}

/// A correction moves the value and never a width (R5, R32).
#[test]
fn a_correction_moves_the_value_and_never_a_width() {
    let mut rng = Rng(0x5EED_1234_ABCD_0006);
    let mut cases = 0;
    for _ in 0..2_000 {
        let later_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let earlier_links = u16::try_from(rng.range(1, 6)).expect("u16");
        let terms = covering_terms(&mut rng, later_links, earlier_links, true);
        let (later_ns, earlier_ns) = (rng.range(0, 1 << 40), rng.range(0, 1 << 40));

        let plain = {
            let mut composer = Composer::new(
                chain(LATER, usize::from(later_links)),
                chain(EARLIER, usize::from(earlier_links)),
            );
            composer
                .terms(terms.iter().copied())
                .stamps(later_ns, earlier_ns);
            composer.compose().expect("composes")
        };

        // Correct exactly one term, which is always allowed: no generated term
        // carries a correction, so no two corrections can intersect.
        let victim = (rng.next() as usize) % terms.len();
        let delta = i64::try_from(rng.range(0, 1 << 30)).expect("i64")
            * if rng.range(0, 1) == 0 { -1 } else { 1 };
        let mut corrected_terms = terms.clone();
        corrected_terms[victim] = corrected_terms[victim].with_correction(Correction::new(
            delta,
            Provenance::Measured {
                calibration: CalibrationId(7),
            },
        ));
        let corrected = {
            let mut composer = Composer::new(
                chain(LATER, usize::from(later_links)),
                chain(EARLIER, usize::from(earlier_links)),
            );
            composer
                .terms(corrected_terms.iter().copied())
                .stamps(later_ns, earlier_ns);
            composer.compose().expect("composes")
        };
        cases += 1;

        assert_eq!(plain.total, corrected.total, "a correction changed a width");
        let sign = if terms[victim].covers.chain == LATER {
            1i128
        } else {
            -1
        };
        assert_eq!(
            i128::from(corrected.interval_ns.expect("a value")),
            i128::from(plain.interval_ns.expect("a value")) + sign * i128::from(delta),
            "a correction did not move the value by itself"
        );
    }
    assert!(cases >= 2_000, "only {cases} corrections checked");
}

// ---- the differential property ------------------------------------------------

/// Instants that sit on the edges the mapping arithmetic can break at, plus a
/// random draw so the set is not only the edges.
fn instants(rng: &mut Rng) -> Vec<u64> {
    let mut every = vec![
        0,
        1,
        1_000_000_000,
        i64::MAX.unsigned_abs(), // the largest source that reads as positive
        i64::MAX.unsigned_abs() + 1, // the first one that reinterprets as negative
        u64::MAX - 1,
        u64::MAX,
    ];
    for _ in 0..5 {
        every.push(rng.next());
        every.push(rng.range(0, 1 << 42));
    }
    every
}

/// A line whose three fields are drawn from the edges of their own ranges.
fn line(rng: &mut Rng) -> Line {
    Line {
        reference_source_ns: rng.pick(&[
            0,
            1,
            1_000_000_000,
            i64::MAX.unsigned_abs(),
            i64::MAX.unsigned_abs() + 1,
            u64::MAX,
        ]),
        offset_ns: rng.pick(&[
            0,
            1,
            -1,
            5_000_000,
            -5_000_000,
            i64::MAX,
            i64::MIN,
            i64::MAX / 2,
        ]),
        skew_ppb: rng.pick(&[0, 1, -1, 100, -100, 1_000_000_000, -1_000_000_000, i64::MAX]),
    }
}

/// The fit a line is wrapped in, with the four uncertainty figures drawn
/// separately: they never touch the value, and the differential property is
/// that they do not.
fn fit(rng: &mut Rng, line: Line) -> Fit {
    Fit {
        reference_source_ns: line.reference_source_ns,
        offset_ns: line.offset_ns,
        skew_ppb: line.skew_ppb,
        skew_uncertainty_ppb: rng.pick(&[0, 1, 2_000, 1_000_000, u64::MAX - 1, SKEW_NOT_ESTIMATED]),
        residual_spread_ns: rng.pick(&[0, 1, 400_000, UNBOUNDED]),
        observation_count: rng.range(1, 10_000),
        bias_low_ns: UNBOUNDED,
        bias_high_ns: 0,
    }
}

fn reference_domain() -> extendedresearch_clock::DomainId {
    Domain {
        host_id: "property-host".to_owned(),
        host_clock_epoch: "boot.1".to_owned(),
        monotonic_source: "CLOCK_BOOTTIME",
        suspend: SuspendBehaviour::Included,
        resolution_ns: None,
        anchor: None,
    }
    .id()
}

fn mapping_provenance() -> MappingProvenance {
    MappingProvenance {
        estimator: EstimatorId(1),
        inputs: InputsId(2),
        offset_argument: ArgumentId(3),
        residual_argument: ArgumentId(4),
    }
}

/// For every input `Line::map_to_reference` accepts, `Mapping::map` reports the
/// same value, bit for bit (architecture §9).
///
/// The new layer changes what is reported, never what is computed.
#[test]
fn a_mapping_reports_exactly_what_the_line_computes() {
    let mut rng = Rng(0x5EED_1234_ABCD_0007);
    let mut cases = 0;
    for _ in 0..2_000 {
        let line = line(&mut rng);
        let fit = fit(&mut rng, line);
        let mapping = Mapping::new(
            fit,
            Span::at(ChainId(1), 0),
            reference_domain(),
            mapping_provenance(),
        );
        // The mapping maps with the fit's own line, and that line is the one
        // the differential is against.
        assert_eq!(mapping.line(), line);
        for source_ns in instants(&mut rng) {
            let mapped = mapping.map(source_ns).expect("a well-formed span maps");
            assert_eq!(
                mapped.reading.as_ns(),
                line.map_to_reference(source_ns),
                "line {line:?} at {source_ns}"
            );
            cases += 1;
        }
    }
    assert!(cases >= 30_000, "only {cases} instants checked");
}

/// The three terms say what the fit says, over the mapping's own span, at every
/// instant (R40, R41, R42).
#[test]
fn the_three_terms_are_the_fits_four_figures_and_nothing_else() {
    let mut rng = Rng(0x5EED_1234_ABCD_0008);
    let mut cases = 0;
    let mut unestimated = 0;
    let covers = Span::at(ChainId(1), 0);
    for _ in 0..2_000 {
        let line = line(&mut rng);
        let fit = fit(&mut rng, line);
        let mapping = Mapping::new(fit, covers, reference_domain(), mapping_provenance());
        for source_ns in instants(&mut rng) {
            let mapped = mapping.map(source_ns).expect("maps");
            cases += 1;

            for term in mapped.terms {
                assert_eq!(term.covers, covers);
            }
            // R40: the fit's own bound. One term corrects and two do not, so
            // the three together cannot trip the overlap refusal (R27).
            assert_eq!(
                mapped.offset().bias,
                Bias::new(fit.bias_low_ns, fit.bias_high_ns)
            );
            assert!(mapped.offset().corrects());
            assert!(!mapped.residual().corrects());
            assert!(!mapped.extrapolation().corrects());
            // R41: precision, never accuracy.
            assert_eq!(mapped.residual().bias, Bias::NONE);
            assert_eq!(
                mapped.residual().dispersion.map(|d| d.sd_ns),
                Some(fit.residual_spread_ns)
            );
            // R42: the penalty, re-derived in u128 where it cannot overflow.
            let penalty = mapped.extrapolation().effective_bias();
            if fit.skew_uncertainty_ppb == SKEW_NOT_ESTIMATED {
                unestimated += 1;
                assert_eq!(penalty, Bias::UNKNOWN);
                // Which leaves the whole reading unknown, as it must.
                assert!(mapped.reading.bound().is_unknown());
            } else {
                let distance = u128::from(source_ns.abs_diff(fit.reference_source_ns));
                let wanted = u64::try_from(
                    u128::from(fit.skew_uncertainty_ppb).saturating_mul(distance) / 1_000_000_000,
                )
                .unwrap_or(UNBOUNDED);
                assert_eq!(penalty, Bias::symmetric(wanted), "at {source_ns}");
                // Nothing bounds how late a one-way mapping places an event.
                assert_eq!(mapped.reading.bound().early_ns, UNBOUNDED);
            }
        }
    }
    assert!(cases >= 30_000, "only {cases} instants checked");
    assert!(
        unestimated >= 1_000,
        "only {unestimated} unestimated-rate cases seen"
    );
}

/// The penalty never shrinks as the instant moves away from the fit's origin
/// (R42), so extrapolating further is never reported as more certain.
#[test]
fn the_penalty_never_shrinks_with_distance() {
    let mut rng = Rng(0x5EED_1234_ABCD_0009);
    let mut cases = 0;
    for _ in 0..4_000 {
        let line = Line {
            reference_source_ns: rng.range(0, 1 << 40),
            offset_ns: 0,
            skew_ppb: 0,
        };
        let fit = Fit {
            skew_uncertainty_ppb: rng.pick(&[0, 1, 2_000, 1_000_000, u64::MAX - 1]),
            ..fit(&mut rng, line)
        };
        let mapping = Mapping::new(
            fit,
            Span::at(ChainId(1), 0),
            reference_domain(),
            mapping_provenance(),
        );
        // Both draws are taken past the origin, so their distances are ordered.
        let origin = fit.reference_source_ns;
        let near = origin.saturating_add(rng.range(0, 1 << 40));
        let far = near.saturating_add(rng.range(0, 1 << 40));
        assert!(
            mapping.extrapolation_penalty_ns(far) >= mapping.extrapolation_penalty_ns(near),
            "{fit:?} shrank between {near} and {far}"
        );
        cases += 1;
    }
    assert!(cases >= 4_000, "only {cases} pairs checked");
}
