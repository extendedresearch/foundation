//! Composing the terms of one interval into a total — or into the statement
//! that there is no total, and why.
//!
//! # The failure this module exists to prevent
//!
//! A budget that silently accounts for only the stages someone remembered to
//! describe. It composes cleanly, it produces a plausible number, and the
//! number is missing however much the undescribed stages contribute. Nothing
//! crashes; the figure is analysed and it is wrong.
//!
//! Three rules, together, are what stop that:
//!
//! - **Unknown poisons.** A term unbounded on a side makes the total unbounded
//!   on that side (R8), and a [`crate::Provenance::Unknown`] bias provenance is
//!   unbounded on both whatever the term's `bias` field says (R10).
//! - **A link no term covers is an unknown term.** The composer walks every
//!   position of both chains and reports the ones nothing accounts for (R11).
//! - **The partial sum is kept, and is never the total.** [`Total`] is a sum
//!   type, so a caller that wants the number has to acknowledge which case it
//!   is in (R9). The distinction a researcher needs is between "12 ms" and
//!   "12 ms of known terms, plus an unmeasured display latency".
//!
//! # Crosswise, because a budget is for a difference
//!
//! R3, R29. A budget accounts for one interval, so it has two endpoints: the
//! chain the later stamp travelled and the chain the earlier one travelled.
//! The later chain's terms put their early width on the interval's early side;
//! the earlier chain's put their **late** width there. The later event
//! happening early and the earlier one happening late both shrink the interval,
//! so those are the two widths that belong together. Adding side by side
//! instead produces a number that looks right and is wrong by however
//! asymmetric the two chains were.
//!
//! # What overlap refusal does and does not refuse
//!
//! R27, R31. The hazard is **correcting twice**: a trigger-based alignment
//! already contains the device delay, and applying both moves the value by
//! exactly the size of the term the second one exists to capture. So at most
//! one term covering a given position may carry a [`Correction`], and two that
//! do are refused with both terms and the intersecting positions named.
//!
//! Two *biases* over one position are not refused. Bounding a stage twice is
//! over-conservative, not wrong, and forbidding it would make a real case
//! unrepresentable: two arguments about one transport, or a stage covered by
//! both a device-wide bound and a link-specific one.
//!
//! # The order is documented, so two runs agree
//!
//! R30. The composer sorts the submitted terms into [`Term`]'s own ordering —
//! lexicographic over `(covers, bias, bias_provenance, correction, dispersion,
//! correlation)` — and the budget holds them in it. Every index this module
//! reports is an index into that order.
//!
//! The arithmetic does not need the ordering: saturating addition on `u64` is
//! commutative and associative, so the sums are the same whatever order they
//! run in. What the ordering fixes is the *reported* indices, the term lists
//! and the uncovered-link lists, so that two runs over the same inputs in
//! different submission orders produce budgets that compare equal.
//!
//! # Corrections first, then biases
//!
//! R32. A correction moves the value; a bias describes how far the value may be
//! from the truth. The order is: apply each endpoint's corrections to its
//! stamp, subtract, then compose the biases crosswise. Doing it the other way
//! round would let a correction change a width, which is the merge R5 forbids.

use std::collections::BTreeMap;
use std::fmt;

use extendedresearch_clock::UNBOUNDED;

use crate::chain::{Chain, Span};
use crate::ids::{CalibrationId, ChainId, GroupId};
use crate::term::{Bias, Correction, Correlation, Dispersion, DistributionKind, Term};

/// Which of an interval's two stamps a chain belongs to.
///
/// The endpoint decides which side of the total a term's widths land on (R3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    /// The chain the later stamp travelled; widths land side by side.
    Later,
    /// The chain the earlier stamp travelled; widths land crosswise.
    Earlier,
}

/// The composed uncertainty of one interval, per side.
///
/// A sum type rather than a pair of saturating integers: a caller that wants
/// the number has to acknowledge which case it is in, which is R9 expressed in
/// the type system rather than in a doc comment.
///
/// [`Total::Unbounded`] is the answer whenever **either** side is unbounded, so
/// it carries which. A side may be unbounded because a term is unbounded on it,
/// because a link no term covers poisons both sides (R11), or because the sum
/// of finite widths saturated — a bound as wide as `u64::MAX` nanoseconds is
/// not one anybody can act on, and reporting it as a total would be a lie of a
/// different kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Total {
    /// Both sides are bounded, and these are the widths.
    Bounded {
        /// The true interval may be up to this much smaller.
        early_ns: u64,
        /// The true interval may be up to this much larger.
        late_ns: u64,
    },
    /// At least one side has no bound.
    Unbounded {
        /// The sum of the bounded contributions to the early side. **Not the
        /// total** (R9).
        known_early_ns: u64,
        /// The sum of the bounded contributions to the late side. **Not the
        /// total** (R9).
        known_late_ns: u64,
        /// True when nothing bounds the early side.
        early_unbounded: bool,
        /// True when nothing bounds the late side.
        late_unbounded: bool,
        /// Indices, in the budget's term order, of the terms unbounded on at
        /// least one side (R33).
        unbounded_terms: Vec<usize>,
        /// Runs of chain positions no term covered (R11). Empty when every
        /// link was covered and the unboundedness came from the terms alone.
        uncovered_links: Vec<Span>,
    },
}

impl Total {
    /// The early width, or `None` when nothing bounds that side.
    ///
    /// `None` rather than the partial sum: R9 forbids presenting the sum of the
    /// bounded terms as the total, and an accessor that returned it here would
    /// be exactly that. The partial sums are [`Total::partial_sums`].
    pub fn early_ns(&self) -> Option<u64> {
        match self {
            Total::Bounded { early_ns, .. } => Some(*early_ns),
            Total::Unbounded {
                early_unbounded: true,
                ..
            } => None,
            Total::Unbounded { known_early_ns, .. } => Some(*known_early_ns),
        }
    }

    /// The late width, or `None` when nothing bounds that side.
    pub fn late_ns(&self) -> Option<u64> {
        match self {
            Total::Bounded { late_ns, .. } => Some(*late_ns),
            Total::Unbounded {
                late_unbounded: true,
                ..
            } => None,
            Total::Unbounded { known_late_ns, .. } => Some(*known_late_ns),
        }
    }

    /// True only when both sides are bounded.
    pub fn is_bounded(&self) -> bool {
        matches!(self, Total::Bounded { .. })
    }

    /// The sum of the bounded contributions on each side.
    ///
    /// **This is not the total** and must not be reported as one (R9). For a
    /// bounded total the two coincide; for an unbounded one the difference
    /// between them is the whole point — "12 ms of known terms" is a different
    /// statement from "12 ms".
    pub fn partial_sums(&self) -> Bias {
        match self {
            Total::Bounded { early_ns, late_ns } => Bias::new(*early_ns, *late_ns),
            Total::Unbounded {
                known_early_ns,
                known_late_ns,
                ..
            } => Bias::new(*known_early_ns, *known_late_ns),
        }
    }

    /// The indices of the terms unbounded on at least one side, empty for a
    /// bounded total.
    pub fn unbounded_terms(&self) -> &[usize] {
        match self {
            Total::Bounded { .. } => &[],
            Total::Unbounded {
                unbounded_terms, ..
            } => unbounded_terms,
        }
    }

    /// The runs of chain positions no term covered, empty for a bounded total.
    pub fn uncovered_links(&self) -> &[Span] {
        match self {
            Total::Bounded { .. } => &[],
            Total::Unbounded {
                uncovered_links, ..
            } => uncovered_links,
        }
    }
}

/// What the terms' dispersions combine to, or why they do not combine.
///
/// R4. Dispersions compose in quadrature **only** between terms that declare
/// independence of one another; terms declaring correlation add linearly; and
/// where the correlation is not declared the composer does not combine them at
/// all. It reports them per term and marks the combination undeclared, which is
/// this type's third case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombinedDispersion {
    /// No term carried a dispersion.
    None,
    /// Every term carrying a dispersion said how it relates to the others, and
    /// this is the combination.
    ///
    /// The kind is [`DistributionKind::Gaussian`] when every contributor was
    /// Gaussian — a sum of Gaussians is Gaussian — and
    /// [`DistributionKind::Unspecified`] otherwise. A sum of two uniforms is
    /// not uniform, and saying it was would licence a coverage factor nobody
    /// derived.
    Known(Dispersion),
    /// At least one term carrying a dispersion left its correlation
    /// [`Undeclared`](crate::Correlation::Undeclared), so nothing was combined.
    ///
    /// The per-term dispersions are still in [`Budget::terms`]; these are the
    /// indices of the ones that stopped the combination.
    Undeclared {
        /// Indices, in the budget's term order, of the undeclared contributors.
        terms: Vec<usize>,
    },
}

/// The terms accounting for one interval, and what they compose to.
///
/// Owns its terms. This is the slow tier: one of these per interval, or per
/// stream epoch, never one per sample. A per-sample structure carries a
/// [`ChainId`] and nothing more (architecture §2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The terms, in the composer's documented order (R30).
    pub terms: Vec<Term>,
    /// What they compose to, per side.
    pub total: Total,
    /// The index of the single term contributing the largest bounded width
    /// (R34), or `None` when no term contributes one.
    ///
    /// This is the number that tells a researcher where spending money pays.
    /// `None` when every term is unbounded, or when every bounded contribution
    /// is zero — an arbitrary pick among zeroes would point at nothing.
    pub largest_bounded: Option<usize>,
    /// What the terms' dispersions combine to, or why they do not.
    pub dispersion: CombinedDispersion,
    /// The corrected interval, when the composer was given the two stamps.
    ///
    /// Corrections applied to each stamp, then subtracted (R32). The widths are
    /// in [`Budget::total`] and never here: a value and its uncertainty are two
    /// things, and merging them is what R1 forbids.
    pub interval_ns: Option<i64>,
    /// The chain the later stamp travelled.
    pub later_chain: ChainId,
    /// The chain the earlier stamp travelled.
    pub earlier_chain: ChainId,
}

impl Budget {
    /// Which endpoint a term belongs to, by the chain its span names.
    pub fn endpoint_of(&self, term: &Term) -> Endpoint {
        if term.covers.chain == self.later_chain {
            Endpoint::Later
        } else {
            Endpoint::Earlier
        }
    }

    /// What a term contributes to each side: `(early, late)`, with `None` where
    /// the contribution is unbounded.
    ///
    /// A later-endpoint term contributes `(early_ns, late_ns)`; an
    /// earlier-endpoint term contributes `(late_ns, early_ns)`, which is the
    /// crosswise rule (R3) seen one term at a time.
    pub fn contribution_of(&self, term: &Term) -> (Option<u64>, Option<u64>) {
        let bias = term.effective_bias();
        let finite = |width: u64| (width != UNBOUNDED).then_some(width);
        match self.endpoint_of(term) {
            Endpoint::Later => (finite(bias.early_ns), finite(bias.late_ns)),
            Endpoint::Earlier => (finite(bias.late_ns), finite(bias.early_ns)),
        }
    }

    /// The indices of the terms that leave the early side unbounded (R33).
    pub fn unbounded_early_terms(&self) -> Vec<usize> {
        self.unbounded_on(|(early, _)| early.is_none())
    }

    /// The indices of the terms that leave the late side unbounded (R33).
    pub fn unbounded_late_terms(&self) -> Vec<usize> {
        self.unbounded_on(|(_, late)| late.is_none())
    }

    fn unbounded_on(&self, wanted: impl Fn((Option<u64>, Option<u64>)) -> bool) -> Vec<usize> {
        self.terms
            .iter()
            .enumerate()
            .filter(|(_, term)| wanted(self.contribution_of(term)))
            .map(|(index, _)| index)
            .collect()
    }
}

/// Why a composition was refused.
///
/// Every refusal names what it refused over: the term indices, in the order
/// [`Composer::ordered_terms`] reports, and the span or chain involved (R28).
/// A refusal is a value, never a panic and never a log line (R52).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompositionError {
    /// Both endpoints named the same chain.
    ///
    /// Two stamps are two traversals. Even on one physical path they are two
    /// events, and composing them from one chain would leave the composer
    /// guessing whether each term counts once or twice. Register the path under
    /// two identifiers and state the terms for each, which is what the record
    /// has to show anyway.
    SameChainForBothEndpoints {
        /// The chain both endpoints named.
        chain: ChainId,
    },
    /// A chain described no links.
    ///
    /// A chain with no stages is not a chain with nothing to account for; it is
    /// a chain nobody described, and reporting it as fully covered is the R11
    /// failure.
    EmptyChain {
        /// The chain.
        chain: ChainId,
    },
    /// A chain has more links than a position can address.
    ChainTooLong {
        /// The chain.
        chain: ChainId,
        /// How many links it described.
        links: usize,
    },
    /// A term's span has `to` below `from`, so it covers nothing.
    MalformedSpan {
        /// The term's index in the composer's order.
        term: usize,
        /// The span.
        span: Span,
    },
    /// A term's span names a chain neither endpoint declared.
    UnknownChain {
        /// The term's index in the composer's order.
        term: usize,
        /// The chain it named.
        chain: ChainId,
    },
    /// A term's span reaches past the last position of its chain.
    SpanPastChain {
        /// The term's index in the composer's order.
        term: usize,
        /// The span.
        span: Span,
        /// The chain's last position.
        last_position: u16,
    },
    /// Two terms carrying a correction cover intersecting positions (R27).
    ///
    /// A trigger-based alignment already contains the device delay; applying
    /// both corrects twice, by exactly the size of the term the second one
    /// exists to capture. Two biases over one span are not this and are not
    /// refused.
    SpanOverlap {
        /// The first term's index in the composer's order.
        first: usize,
        /// The second term's index in the composer's order.
        second: usize,
        /// The positions both cover.
        at: Span,
        /// The calibrations involved, where each correction named one (R28).
        calibrations: [Option<CalibrationId>; 2],
    },
    /// The corrected interval does not fit `i64` nanoseconds.
    Overflow,
}

impl fmt::Display for CompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositionError::SameChainForBothEndpoints { chain } => write!(
                f,
                "both endpoints named chain {}; two stamps are two chains",
                chain.0
            ),
            CompositionError::EmptyChain { chain } => {
                write!(f, "chain {} describes no links", chain.0)
            }
            CompositionError::ChainTooLong { chain, links } => write!(
                f,
                "chain {} describes {links} links, more than a position can address",
                chain.0
            ),
            CompositionError::MalformedSpan { term, span } => write!(
                f,
                "term {term} covers {}:{}-{}, which runs backwards",
                span.chain.0, span.from, span.to
            ),
            CompositionError::UnknownChain { term, chain } => write!(
                f,
                "term {term} covers chain {}, which neither endpoint declared",
                chain.0
            ),
            CompositionError::SpanPastChain {
                term,
                span,
                last_position,
            } => write!(
                f,
                "term {term} covers {}:{}-{}, past the chain's last position {last_position}",
                span.chain.0, span.from, span.to
            ),
            CompositionError::SpanOverlap {
                first,
                second,
                at,
                calibrations,
            } => {
                write!(
                    f,
                    "terms {first} and {second} both correct the value over {}:{}-{}",
                    at.chain.0, at.from, at.to
                )?;
                match calibrations {
                    [Some(a), Some(b)] => write!(f, " (calibrations {} and {})", a.0, b.0),
                    [Some(a), None] | [None, Some(a)] => write!(f, " (calibration {})", a.0),
                    [None, None] => Ok(()),
                }
            }
            CompositionError::Overflow => {
                f.write_str("the corrected interval does not fit i64 nanoseconds")
            }
        }
    }
}

impl std::error::Error for CompositionError {}

/// Builds one interval's budget from its two chains and the terms covering
/// them.
///
/// ```
/// use extendedresearch_metrology::{
///     ArgumentId, Bias, Chain, ChainId, Composer, Link, LinkKind, Provenance, Span, Term,
/// };
///
/// let later = Chain::new(ChainId(1), [Link::of(LinkKind::HostStamp)]);
/// let earlier = Chain::new(ChainId(2), [Link::of(LinkKind::Emission)]);
///
/// // The later stamp's chain is bounded; the earlier one's is not described.
/// let mut composer = Composer::new(later, earlier);
/// composer.term(Term::new(
///     Span::at(ChainId(1), 0),
///     Bias::symmetric(100_000),
///     Provenance::Bounded { argument: ArgumentId(1) },
/// ));
///
/// // Chain 2 position 0 is covered by no term, so there is no total.
/// let budget = composer.compose()?;
/// assert!(!budget.total.is_bounded());
/// assert_eq!(budget.total.early_ns(), None);
/// assert_eq!(budget.total.partial_sums().early_ns, 100_000);
/// # Ok::<(), extendedresearch_metrology::CompositionError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composer {
    later: Chain,
    earlier: Chain,
    terms: Vec<Term>,
    stamps: Option<(u64, u64)>,
}

impl Composer {
    /// A budget for the difference of two stamps: the chain the later one
    /// travelled, and the chain the earlier one travelled (R29).
    pub fn new(later: Chain, earlier: Chain) -> Composer {
        Composer {
            later,
            earlier,
            terms: Vec::new(),
            stamps: None,
        }
    }

    /// Adds a term. Its span names which chain, and so which endpoint, it
    /// belongs to.
    pub fn term(&mut self, term: Term) -> &mut Composer {
        self.terms.push(term);
        self
    }

    /// Adds several terms.
    pub fn terms(&mut self, terms: impl IntoIterator<Item = Term>) -> &mut Composer {
        self.terms.extend(terms);
        self
    }

    /// The two raw stamp values, so the budget can report the corrected
    /// interval (R32).
    ///
    /// Raw, never corrected: the corrections live in the terms, and applying
    /// them here is the composer's job. A caller that stores the corrected
    /// value and throws the raw one away has stored a derived value as a fact,
    /// which R56 forbids for exactly the reason that a calibration will later
    /// be corrected.
    pub fn stamps(&mut self, later_ns: u64, earlier_ns: u64) -> &mut Composer {
        self.stamps = Some((later_ns, earlier_ns));
        self
    }

    /// The submitted terms in the order [`Composer::compose`] uses, so that the
    /// indices a refusal names can be resolved.
    pub fn ordered_terms(&self) -> Vec<Term> {
        let mut terms = self.terms.clone();
        terms.sort_unstable();
        terms
    }

    /// Composes the terms into a [`Budget`].
    ///
    /// # Errors
    ///
    /// [`CompositionError::SameChainForBothEndpoints`] when one chain was given
    /// for both endpoints; [`CompositionError::EmptyChain`] or
    /// [`CompositionError::ChainTooLong`] for a chain that cannot be walked;
    /// [`CompositionError::MalformedSpan`], [`CompositionError::UnknownChain`]
    /// or [`CompositionError::SpanPastChain`] for a span that does not describe
    /// positions of a declared chain; [`CompositionError::SpanOverlap`] when two
    /// terms carrying a correction cover the same position (R27, R31); and
    /// [`CompositionError::Overflow`] when the corrected interval does not fit
    /// `i64`.
    ///
    /// A link no term covers is **not** an error: it is an unbounded total with
    /// the uncovered runs named (R11).
    pub fn compose(&self) -> Result<Budget, CompositionError> {
        if self.later.id == self.earlier.id {
            return Err(CompositionError::SameChainForBothEndpoints {
                chain: self.later.id,
            });
        }
        let last = [&self.later, &self.earlier].map(|chain| match chain.last_position() {
            None => Err(CompositionError::EmptyChain { chain: chain.id }),
            Some(_) if chain.links.len() > usize::from(u16::MAX) + 1 => {
                Err(CompositionError::ChainTooLong {
                    chain: chain.id,
                    links: chain.links.len(),
                })
            }
            Some(position) => Ok(position),
        });
        let [later_last, earlier_last] = [last[0]?, last[1]?];

        let terms = self.ordered_terms();
        let last_of = |chain: ChainId| {
            if chain == self.later.id {
                Some(later_last)
            } else if chain == self.earlier.id {
                Some(earlier_last)
            } else {
                None
            }
        };

        for (index, term) in terms.iter().enumerate() {
            let span = term.covers;
            if !span.is_well_formed() {
                return Err(CompositionError::MalformedSpan { term: index, span });
            }
            let Some(last_position) = last_of(span.chain) else {
                return Err(CompositionError::UnknownChain {
                    term: index,
                    chain: span.chain,
                });
            };
            if span.to > last_position {
                return Err(CompositionError::SpanPastChain {
                    term: index,
                    span,
                    last_position,
                });
            }
        }

        overlap_refusal(&terms)?;

        let uncovered_links = uncovered(
            &terms,
            [(&self.later, later_last), (&self.earlier, earlier_last)],
        );

        let shell = Budget {
            terms: Vec::new(),
            total: Total::Bounded {
                early_ns: 0,
                late_ns: 0,
            },
            largest_bounded: None,
            dispersion: CombinedDispersion::None,
            interval_ns: None,
            later_chain: self.later.id,
            earlier_chain: self.earlier.id,
        };

        let mut partial = Bias::NONE;
        let mut early_unbounded = false;
        let mut late_unbounded = false;
        let mut unbounded_terms = Vec::new();
        for (index, term) in terms.iter().enumerate() {
            let (early, late) = shell.contribution_of(term);
            match early {
                Some(width) => partial.early_ns = partial.early_ns.saturating_add(width),
                None => early_unbounded = true,
            }
            match late {
                Some(width) => partial.late_ns = partial.late_ns.saturating_add(width),
                None => late_unbounded = true,
            }
            if early.is_none() || late.is_none() {
                unbounded_terms.push(index);
            }
        }
        // A sum of finite widths that reached the sentinel is not a bound a
        // caller can act on, and reporting it as one would be a lie of its own.
        early_unbounded |= partial.early_ns == UNBOUNDED;
        late_unbounded |= partial.late_ns == UNBOUNDED;
        // R11, by way of R8: a position nothing accounts for is an unknown
        // term, and an unknown term is unbounded on both sides.
        if !uncovered_links.is_empty() {
            early_unbounded = true;
            late_unbounded = true;
        }

        let total = if early_unbounded || late_unbounded {
            Total::Unbounded {
                known_early_ns: partial.early_ns,
                known_late_ns: partial.late_ns,
                early_unbounded,
                late_unbounded,
                unbounded_terms,
                uncovered_links,
            }
        } else {
            Total::Bounded {
                early_ns: partial.early_ns,
                late_ns: partial.late_ns,
            }
        };

        let largest_bounded = terms
            .iter()
            .enumerate()
            .filter_map(|(index, term)| {
                let (early, late) = shell.contribution_of(term);
                let width = early.unwrap_or(0).max(late.unwrap_or(0));
                (width > 0).then_some((width, index))
            })
            // The first term of its size, in the documented order, is the one
            // to name; `max_by_key` would answer the last.
            .fold(None, |best: Option<(u64, usize)>, candidate| match best {
                Some(current) if current.0 >= candidate.0 => Some(current),
                _ => Some(candidate),
            })
            .map(|(_, index)| index);

        let interval_ns = self.corrected_interval(&terms)?;

        Ok(Budget {
            dispersion: combine_dispersions(&terms),
            terms,
            total,
            largest_bounded,
            interval_ns,
            ..shell
        })
    }

    /// Each stamp with its endpoint's corrections applied, then subtracted
    /// (R32).
    fn corrected_interval(&self, terms: &[Term]) -> Result<Option<i64>, CompositionError> {
        let Some((later_ns, earlier_ns)) = self.stamps else {
            return Ok(None);
        };
        let corrected = |ns: u64, chain: ChainId| {
            terms
                .iter()
                .filter(|term| term.covers.chain == chain)
                .try_fold(i128::from(ns), |value, term| {
                    value.checked_add(i128::from(term.correction_ns()))
                })
        };
        let (Some(later), Some(earlier)) = (
            corrected(later_ns, self.later.id),
            corrected(earlier_ns, self.earlier.id),
        ) else {
            return Err(CompositionError::Overflow);
        };
        later
            .checked_sub(earlier)
            .and_then(|difference| i64::try_from(difference).ok())
            .map(Some)
            .ok_or(CompositionError::Overflow)
    }
}

/// Refuses two terms that both carry a correction and cover a common position
/// (R27, R31).
///
/// Only corrections. Two biases over one position are over-conservative rather
/// than wrong, and refusing them would make a one-way fit — an estimated offset
/// and an argued residual over one span — unrepresentable.
fn overlap_refusal(terms: &[Term]) -> Result<(), CompositionError> {
    let correcting: Vec<(usize, &Term)> = terms
        .iter()
        .enumerate()
        .filter(|(_, term)| term.corrects())
        .collect();
    for (position, &(first, one)) in correcting.iter().enumerate() {
        for &(second, other) in correcting.iter().skip(position + 1) {
            if let Some(at) = one.covers.intersection(other.covers) {
                let named = |term: &Term| {
                    term.correction
                        .and_then(|correction: Correction| correction.provenance.calibration())
                };
                return Err(CompositionError::SpanOverlap {
                    first,
                    second,
                    at,
                    calibrations: [named(one), named(other)],
                });
            }
        }
    }
    Ok(())
}

/// The runs of positions no term covers, over both chains in ascending chain
/// identifier (R11).
///
/// A term covering several positions covers every one of them: a recorder that
/// takes one stamp for [`HostReceive`](crate::LinkKind::HostReceive) and
/// [`HostQueue`](crate::LinkKind::HostQueue) together leaves neither uncovered.
fn uncovered(terms: &[Term], chains: [(&Chain, u16); 2]) -> Vec<Span> {
    let mut chains = chains;
    chains.sort_by_key(|(chain, _)| chain.id);
    let mut runs = Vec::new();
    for (chain, last) in chains {
        let mut covered = vec![false; usize::from(last) + 1];
        for term in terms.iter().filter(|term| term.covers.chain == chain.id) {
            for position in term.covers.from..=term.covers.to {
                if let Some(flag) = covered.get_mut(usize::from(position)) {
                    *flag = true;
                }
            }
        }
        let mut start: Option<u16> = None;
        for position in 0..=last {
            match (covered.get(usize::from(position)), start) {
                (Some(false), None) => start = Some(position),
                (Some(true), Some(from)) => {
                    runs.push(Span {
                        chain: chain.id,
                        from,
                        to: position - 1,
                    });
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(from) = start {
            runs.push(Span {
                chain: chain.id,
                from,
                to: last,
            });
        }
    }
    runs
}

/// Combines the terms' dispersions, or reports that they do not combine (R4).
fn combine_dispersions(terms: &[Term]) -> CombinedDispersion {
    let contributors: Vec<(usize, Dispersion, Correlation)> = terms
        .iter()
        .enumerate()
        .filter_map(|(index, term)| {
            term.dispersion
                .map(|dispersion| (index, dispersion, term.correlation))
        })
        .collect();
    if contributors.is_empty() {
        return CombinedDispersion::None;
    }
    let undeclared: Vec<usize> = contributors
        .iter()
        .filter(|(_, _, correlation)| correlation.is_undeclared())
        .map(|(index, _, _)| *index)
        .collect();
    if !undeclared.is_empty() {
        return CombinedDispersion::Undeclared { terms: undeclared };
    }

    // Correlated terms add linearly inside their group; each group's sum then
    // enters the quadrature beside every independent term.
    let mut groups: BTreeMap<GroupId, u64> = BTreeMap::new();
    let mut independent: Vec<u64> = Vec::new();
    for (_, dispersion, correlation) in &contributors {
        match correlation.group() {
            Some(group) => {
                let sum = groups.entry(group).or_insert(0);
                *sum = sum.saturating_add(dispersion.sd_ns);
            }
            None => independent.push(dispersion.sd_ns),
        }
    }
    let sum_of_squares = groups
        .values()
        .copied()
        .chain(independent)
        .fold(0u128, |sum, sd| {
            let sd = u128::from(sd);
            sum.saturating_add(sd.saturating_mul(sd))
        });
    // Exact integer square root; no floating point enters the composer.
    let sd_ns = u64::try_from(sum_of_squares.isqrt()).unwrap_or(UNBOUNDED);
    let all_gaussian = contributors
        .iter()
        .all(|(_, dispersion, _)| dispersion.kind == DistributionKind::Gaussian);
    let kind = if all_gaussian {
        DistributionKind::Gaussian
    } else {
        DistributionKind::Unspecified
    };
    CombinedDispersion::Known(Dispersion::new(sd_ns, kind))
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
    use crate::chain::{Link, LinkKind};
    use crate::ids::{ArgumentId, EstimatorId, InputsId};
    use crate::term::Provenance;

    const LATER: ChainId = ChainId(1);
    const EARLIER: ChainId = ChainId(2);

    fn argued() -> Provenance {
        Provenance::Bounded {
            argument: ArgumentId(1),
        }
    }

    fn chain(id: ChainId, links: usize) -> Chain {
        Chain::new(id, vec![Link::of(LinkKind::Transport); links])
    }

    fn bounded(chain: ChainId, from: u16, to: u16, bias: Bias) -> Term {
        Term::new(Span { chain, from, to }, bias, argued())
    }

    fn composer(later_links: usize, earlier_links: usize) -> Composer {
        Composer::new(chain(LATER, later_links), chain(EARLIER, earlier_links))
    }

    #[test]
    fn widths_compose_crosswise_over_the_two_endpoints() {
        let mut c = composer(1, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(10, 20)));
        c.term(bounded(EARLIER, 0, 0, Bias::new(3, 4)));
        let budget = c.compose().expect("composes");
        assert_eq!(
            budget.total,
            Total::Bounded {
                early_ns: 10 + 4,
                late_ns: 20 + 3
            }
        );
    }

    #[test]
    fn an_unknown_term_leaves_no_total_and_keeps_the_partial_sum() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(10, 20)));
        c.term(Term::unknown(Span::at(LATER, 1)));
        c.term(bounded(EARLIER, 0, 0, Bias::new(3, 4)));
        let budget = c.compose().expect("composes");
        assert_eq!(budget.total.early_ns(), None);
        assert_eq!(budget.total.late_ns(), None);
        assert_eq!(budget.total.partial_sums(), Bias::new(14, 23));
        assert_eq!(budget.total.unbounded_terms(), &[1]);
        assert!(budget.total.uncovered_links().is_empty());
    }

    #[test]
    fn a_link_no_term_covers_makes_the_total_unbounded() {
        let mut c = composer(3, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(10, 20)));
        c.term(bounded(EARLIER, 0, 0, Bias::new(3, 4)));
        let budget = c.compose().expect("composes");
        assert_eq!(
            budget.total.uncovered_links(),
            &[Span {
                chain: LATER,
                from: 1,
                to: 2
            }]
        );
        assert_eq!(budget.total.early_ns(), None);
        assert_eq!(budget.total.partial_sums(), Bias::new(14, 23));
    }

    #[test]
    fn one_term_may_cover_several_links() {
        let mut c = composer(3, 1);
        // One stamp covering HostReceive and HostQueue together.
        c.term(bounded(LATER, 0, 2, Bias::new(10, 20)));
        c.term(bounded(EARLIER, 0, 0, Bias::new(3, 4)));
        let budget = c.compose().expect("composes");
        assert!(budget.total.uncovered_links().is_empty());
        assert_eq!(
            budget.total,
            Total::Bounded {
                early_ns: 14,
                late_ns: 23
            }
        );
    }

    #[test]
    fn two_corrections_over_one_position_are_refused() {
        let mut c = composer(3, 1);
        c.term(bounded(LATER, 0, 1, Bias::NONE).with_correction(Correction::new(-5, argued())));
        c.term(bounded(LATER, 1, 2, Bias::NONE).with_correction(Correction::new(-7, argued())));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        let error = c.compose().expect_err("refuses");
        assert!(matches!(
            error,
            CompositionError::SpanOverlap {
                at: Span {
                    chain: LATER,
                    from: 1,
                    to: 1
                },
                ..
            }
        ));
    }

    #[test]
    fn two_overlapping_terms_with_one_correction_are_accepted() {
        let mut c = composer(1, 1);
        c.term(
            bounded(LATER, 0, 0, Bias::new(1, 1)).with_correction(Correction::new(-5, argued())),
        );
        c.term(bounded(LATER, 0, 0, Bias::new(2, 2)));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        let budget = c.compose().expect("composes");
        assert_eq!(
            budget.total,
            Total::Bounded {
                early_ns: 3,
                late_ns: 3
            }
        );
    }

    #[test]
    fn a_one_way_fit_is_one_term_the_composer_accepts() {
        // Estimated offset and argued residual over one span: the case a single
        // provenance per term could not carry.
        let mut c = composer(1, 1);
        c.term(
            bounded(LATER, 0, 0, Bias::new(UNBOUNDED, 0)).with_correction(Correction::new(
                -4_200,
                Provenance::Estimated {
                    estimator: EstimatorId(1),
                    inputs: InputsId(2),
                },
            )),
        );
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        c.stamps(1_000_000, 0);
        let budget = c.compose().expect("composes");
        assert_eq!(budget.interval_ns, Some(1_000_000 - 4_200));
        assert_eq!(budget.total.early_ns(), None);
        assert_eq!(budget.total.late_ns(), Some(0));
    }

    #[test]
    fn a_zero_correction_is_a_claim_and_an_absent_one_is_not() {
        let mut with_zero = composer(1, 1);
        with_zero
            .term(bounded(LATER, 0, 0, Bias::NONE).with_correction(Correction::new(0, argued())));
        with_zero
            .term(bounded(EARLIER, 0, 0, Bias::NONE).with_correction(Correction::new(0, argued())));
        let mut without = composer(1, 1);
        without.term(bounded(LATER, 0, 0, Bias::NONE));
        without.term(bounded(EARLIER, 0, 0, Bias::NONE));
        // The same interval, and different terms in the record.
        assert_eq!(
            with_zero.compose().expect("composes").total,
            without.compose().expect("composes").total
        );
        assert_ne!(
            with_zero.compose().expect("composes").terms,
            without.compose().expect("composes").terms
        );
        // And the zero one is a correction, so a second over the same span is
        // refused.
        with_zero
            .term(bounded(LATER, 0, 0, Bias::NONE).with_correction(Correction::new(0, argued())));
        assert!(matches!(
            with_zero.compose().expect_err("refuses"),
            CompositionError::SpanOverlap { .. }
        ));
    }

    #[test]
    fn corrections_apply_before_the_biases_compose() {
        let mut c = composer(1, 1);
        c.term(
            bounded(LATER, 0, 0, Bias::new(10, 20))
                .with_correction(Correction::new(-18_200_000, argued())),
        );
        c.term(
            bounded(EARLIER, 0, 0, Bias::new(3, 4)).with_correction(Correction::new(500, argued())),
        );
        c.stamps(1_000_000_000, 250_000_000);
        let budget = c.compose().expect("composes");
        assert_eq!(
            budget.interval_ns,
            Some(1_000_000_000 - 18_200_000 - (250_000_000 + 500))
        );
        // The correction moved the value and left both widths alone.
        assert_eq!(
            budget.total,
            Total::Bounded {
                early_ns: 14,
                late_ns: 23
            }
        );
    }

    #[test]
    fn the_submission_order_does_not_change_the_budget() {
        let one = bounded(LATER, 0, 0, Bias::new(10, 20));
        let two = bounded(LATER, 1, 1, Bias::new(1, 2));
        let three = bounded(EARLIER, 0, 0, Bias::new(3, 4));
        let mut forward = composer(2, 1);
        forward.terms([one, two, three]);
        let mut backward = composer(2, 1);
        backward.terms([three, two, one]);
        assert_eq!(forward.compose(), backward.compose());
    }

    #[test]
    fn dispersions_need_a_declaration_before_they_combine() {
        let mut c = composer(1, 1);
        c.term(
            bounded(LATER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(300, DistributionKind::Gaussian)),
        );
        c.term(
            bounded(EARLIER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(400, DistributionKind::Gaussian)),
        );
        assert_eq!(
            c.compose().expect("composes").dispersion,
            CombinedDispersion::Undeclared { terms: vec![0, 1] }
        );
    }

    #[test]
    fn declared_independence_composes_in_quadrature() {
        let mut c = composer(1, 1);
        c.term(
            bounded(LATER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(300, DistributionKind::Gaussian))
                .with_correlation(Correlation::Independent),
        );
        c.term(
            bounded(EARLIER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(400, DistributionKind::Gaussian))
                .with_correlation(Correlation::Independent),
        );
        assert_eq!(
            c.compose().expect("composes").dispersion,
            CombinedDispersion::Known(Dispersion::new(500, DistributionKind::Gaussian))
        );
    }

    #[test]
    fn a_group_adds_linearly_before_it_enters_the_quadrature() {
        let mut c = composer(2, 1);
        let group = Correlation::CorrelatedWith(GroupId(1));
        c.term(
            bounded(LATER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(150, DistributionKind::Gaussian))
                .with_correlation(group),
        );
        c.term(
            bounded(LATER, 1, 1, Bias::NONE)
                .with_dispersion(Dispersion::new(150, DistributionKind::Gaussian))
                .with_correlation(group),
        );
        c.term(
            bounded(EARLIER, 0, 0, Bias::NONE)
                .with_dispersion(Dispersion::new(400, DistributionKind::Uniform))
                .with_correlation(Correlation::Independent),
        );
        // 150 + 150 linearly, then sqrt(300^2 + 400^2); one contributor is not
        // Gaussian, so the combination declares no distribution.
        assert_eq!(
            c.compose().expect("composes").dispersion,
            CombinedDispersion::Known(Dispersion::new(500, DistributionKind::Unspecified))
        );
    }

    #[test]
    fn the_largest_bounded_term_is_the_first_of_its_size() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(10, 20)));
        c.term(bounded(LATER, 1, 1, Bias::new(20, 5)));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        let budget = c.compose().expect("composes");
        assert_eq!(budget.largest_bounded, Some(0));
        assert_eq!(budget.terms[0].covers.from, 0);
    }

    #[test]
    fn a_budget_of_zero_widths_names_no_largest_term() {
        let mut c = composer(1, 1);
        c.term(bounded(LATER, 0, 0, Bias::NONE));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        assert_eq!(c.compose().expect("composes").largest_bounded, None);
    }

    #[test]
    fn a_saturated_partial_sum_is_not_a_total() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(1 << 63, 0)));
        c.term(bounded(LATER, 1, 1, Bias::new(1 << 63, 0)));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        let budget = c.compose().expect("composes");
        assert_eq!(budget.total.early_ns(), None);
        assert_eq!(budget.total.late_ns(), Some(0));
        assert_eq!(budget.total.partial_sums().early_ns, UNBOUNDED);
        assert!(budget.total.unbounded_terms().is_empty());
    }

    #[test]
    fn a_chain_used_for_both_endpoints_is_refused() {
        let error = Composer::new(chain(LATER, 1), chain(LATER, 1))
            .compose()
            .expect_err("refuses");
        assert_eq!(
            error,
            CompositionError::SameChainForBothEndpoints { chain: LATER }
        );
    }

    #[test]
    fn a_chain_with_no_links_is_refused() {
        let error = Composer::new(Chain::new(LATER, []), chain(EARLIER, 1))
            .compose()
            .expect_err("refuses");
        assert_eq!(error, CompositionError::EmptyChain { chain: LATER });
    }

    #[test]
    fn a_span_that_describes_nothing_is_refused() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 1, 0, Bias::NONE));
        assert!(matches!(
            c.compose().expect_err("refuses"),
            CompositionError::MalformedSpan { .. }
        ));
    }

    #[test]
    fn a_span_on_an_undeclared_chain_is_refused() {
        let mut c = composer(1, 1);
        c.term(bounded(ChainId(9), 0, 0, Bias::NONE));
        assert_eq!(
            c.compose().expect_err("refuses"),
            CompositionError::UnknownChain {
                term: 0,
                chain: ChainId(9)
            }
        );
    }

    #[test]
    fn a_span_past_the_end_of_its_chain_is_refused() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 0, 5, Bias::NONE));
        assert!(matches!(
            c.compose().expect_err("refuses"),
            CompositionError::SpanPastChain {
                last_position: 1,
                ..
            }
        ));
    }

    #[test]
    fn an_interval_that_does_not_fit_is_refused() {
        let mut c = composer(1, 1);
        c.term(bounded(LATER, 0, 0, Bias::NONE));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        c.stamps(u64::MAX, 0);
        assert_eq!(
            c.compose().expect_err("refuses"),
            CompositionError::Overflow
        );
    }

    #[test]
    fn every_refusal_says_what_it_refused_over() {
        let measured = |id| Provenance::Measured {
            calibration: CalibrationId(id),
        };
        let mut c = composer(3, 1);
        c.term(bounded(LATER, 0, 1, Bias::NONE).with_correction(Correction::new(-5, measured(4))));
        c.term(bounded(LATER, 1, 2, Bias::NONE).with_correction(Correction::new(-7, measured(9))));
        let message = c.compose().expect_err("refuses").to_string();
        assert!(message.contains("1:1-1"), "{message}");
        assert!(message.contains("calibrations 4 and 9"), "{message}");
    }

    #[test]
    fn the_per_side_unbounded_terms_are_reported_separately() {
        let mut c = composer(2, 1);
        c.term(bounded(LATER, 0, 0, Bias::new(UNBOUNDED, 5)));
        c.term(bounded(LATER, 1, 1, Bias::new(5, UNBOUNDED)));
        c.term(bounded(EARLIER, 0, 0, Bias::NONE));
        let budget = c.compose().expect("composes");
        assert_eq!(budget.unbounded_early_terms(), vec![0]);
        assert_eq!(budget.unbounded_late_terms(), vec![1]);
        assert_eq!(budget.total.unbounded_terms(), &[0, 1]);
    }
}
