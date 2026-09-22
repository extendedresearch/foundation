//! Properties of the composer, checked over generated term sets with a seeded
//! generator and no property-testing dependency.
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
//! # The rest
//!
//! Composition is order-independent, adding a term never narrows a total, and a
//! bounded total equals an independent re-derivation of the sum in `u128`. The
//! last one is the check that the saturating arithmetic is not quietly
//! wrapping: `u128` cannot overflow at these magnitudes, so the two agree
//! unless the `u64` path is wrong.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use extendedresearch_metrology::{
    ArgumentId, Bias, Budget, CalibrationId, Chain, ChainId, Composer, Correction, Correlation,
    Dispersion, DistributionKind, DocumentId, EstimatorId, InputsId, Link, LinkKind, Provenance,
    Span, Term, Total, UNBOUNDED,
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
