//! What a measured interval has to carry to be evidence: the stages the event
//! travelled, the terms accounting for each, how every number was arrived at,
//! and what they compose to — or that they compose to nothing, because
//! something is unknown.
//!
//! # The claim this exists to support
//!
//! Every research or clinical use of timing software reduces to a claim of the
//! form *event A and event B, observed by different instruments, were separated
//! by t ± u*. Three things follow.
//!
//! **The deliverable is an interval, not a timestamp.** Nobody publishes a
//! clock reading. A system that produces good timestamps and leaves the
//! subtraction to the caller has shipped the easy half.
//!
//! **Without *u*, *t* is not evidence.** A 450 ms reaction time means one thing
//! if *u* is 2 ms and nothing at all if *u* is 60 ms and unstated.
//!
//! **A wrong *t* that looks right is the failure that matters.** Timing defects
//! do not crash. They produce numbers that pass every plausibility check, get
//! analysed, and are wrong. So this crate's first obligation is to refuse
//! rather than guess, and its second is to make every number it emits
//! reconstructable.
//!
//! # The contract
//!
//! | | |
//! |---|---|
//! | **Two currencies, never mixed** | [`Bias`] is a worst-case asymmetric interval; [`Dispersion`] is a standard deviation with the distribution it came from. They are separate fields and compose by different arithmetic (R1, R2, R4) |
//! | **Zero is a claim; absence is `UNBOUNDED`** | A zero bias asserts the stage contributes nothing. Not knowing is [`UNBOUNDED`] on both sides, never zero (R6). [`Bias`] has no `Default` for this reason |
//! | **Unknown poisons the total** | A term unbounded on a side makes the total unbounded on that side, and no accessor returns a partial sum where a total is asked for (R8, R9) |
//! | **A link nothing covers is unknown** | The composer walks every position of both chains; a run no term covers makes the total unbounded and is named in it (R11) |
//! | **Coverage is a span** | A term covers a closed range of one chain's positions, not a kind tag — two devices can both have a "device delay", and one stamp can cover several links (R16) |
//! | **Correcting twice is refused** | At most one term covering a position may carry a [`Correction`]; two are refused with both terms and the intersecting positions named (R27, R28, R31) |
//! | **Corrections move the value, biases describe it** | Corrections apply to each stamp, then the difference is taken, then the biases compose crosswise (R3, R32) |
//! | **Deterministic** | Terms compose in [`Term`]'s own order, so two runs over the same inputs produce equal budgets whatever order they were submitted in (R30) |
//! | **Integers throughout** | Including the quadrature sum, which accumulates squares in `u128` and takes an exact integer square root. No floating point enters this crate (R63) |
//!
//! # What is here, and what is not
//!
//! This is step 1 of the implementation order in `timing-architecture.md`: the
//! core uncertainty model.
//!
//! - chains and coverage: [`Chain`], [`Link`], [`LinkKind`], [`Span`];
//! - terms: [`Bias`], [`Dispersion`], [`Correction`], [`Correlation`],
//!   [`Provenance`], [`Term`];
//! - composition: [`Composer`], [`Budget`], [`Total`], [`CombinedDispersion`],
//!   [`CompositionError`].
//!
//! Not here yet: calibrations and their conditions and expiry (step 5), the
//! uncertainty-carrying clock mapping (step 3), sample clocks (step 4),
//! requirements and profiles (step 6), the record schema (step 7), and the C
//! ABI projection (step 8).
//!
//! Nothing here reads a clock or touches a platform. The one dependency is
//! `extendedresearch-clock`, for [`UNBOUNDED`], which is defined once and not
//! redefined here.
//!
//! # A term carries two provenances
//!
//! One would not be enough. A one-way clock fit produces a **correction** an
//! estimator computed and a **residual bound** resting on an argument about
//! transport delay, over one span, in one term. [`Term::bias_provenance`] says
//! how the width was arrived at and [`Correction::provenance`] says how the
//! adjustment was; splitting them into two terms would trip the correction
//! overlap refusal and make the fit unrepresentable.
//!
//! # Conformance
//!
//! `vectors/` beside this crate holds language-free JSON vectors, one expected
//! answer per row, for every composition rule. `tests/vectors.rs` runs them
//! against this crate; any other implementation runs the same files (R62).
//!
//! No `unsafe` is written in this crate.
//!
//! # Example
//!
//! ```
//! use extendedresearch_metrology::{
//!     ArgumentId, Bias, Chain, ChainId, Composer, Link, LinkKind, Provenance, Span, Term, Total,
//! };
//!
//! let argued = Provenance::Bounded { argument: ArgumentId(1) };
//!
//! // The response: a contact closes, travels a wire, and the host stamps it.
//! let response = Chain::new(
//!     ChainId(1),
//!     [
//!         Link::of(LinkKind::Transduction),
//!         Link::of(LinkKind::Transport),
//!         Link::of(LinkKind::HostStamp),
//!     ],
//! );
//! // The stimulus: the program submits a frame and the panel emits light.
//! let stimulus = Chain::new(
//!     ChainId(2),
//!     [Link::of(LinkKind::ApplicationSubmit), Link::of(LinkKind::Emission)],
//! );
//!
//! let mut composer = Composer::new(response, stimulus);
//! composer
//!     // One stamp covering the contact, the wire and the read.
//!     .term(Term::new(Span { chain: ChainId(1), from: 0, to: 2 }, Bias::symmetric(200_000), argued))
//!     .term(Term::new(Span::at(ChainId(2), 0), Bias::symmetric(100_000), argued))
//!     // Nobody measured the panel, so nothing is claimed about it.
//!     .term(Term::unknown(Span::at(ChainId(2), 1)))
//!     .stamps(1_450_000_000, 1_000_000_000);
//!
//! let budget = composer.compose()?;
//! assert_eq!(budget.interval_ns, Some(450_000_000));
//! // 450 ms, and no total: the panel's contribution is unmeasured.
//! assert!(matches!(budget.total, Total::Unbounded { .. }));
//! assert_eq!(budget.total.early_ns(), None);
//! // What is known is kept, and is not the total.
//! assert_eq!(budget.total.partial_sums().early_ns, 300_000);
//! assert_eq!(budget.total.unbounded_terms(), &[2]);
//! # Ok::<(), extendedresearch_metrology::CompositionError>(())
//! ```

mod budget;
mod chain;
mod ids;
mod term;

pub use budget::{Budget, CombinedDispersion, Composer, CompositionError, Endpoint, Total};
pub use chain::{Chain, Link, LinkKind, Span};
pub use ids::{
    ArgumentId, CalibrationId, ChainId, DeviceId, DocumentId, EstimatorId, GroupId, InputsId,
};
pub use term::{Bias, Correction, Correlation, Dispersion, DistributionKind, Provenance, Term};

/// No bound is known in this direction: `u64::MAX`, re-exported from
/// `extendedresearch-clock`.
///
/// One sentinel across both crates, defined once. A second definition here
/// would be the same number today and a silently different one the day either
/// crate changed it, and every bound in every record would be wrong in a way no
/// test compares.
pub use extendedresearch_clock::UNBOUNDED;
