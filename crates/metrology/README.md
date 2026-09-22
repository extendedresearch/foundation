# extendedresearch-metrology

Chains, terms, provenance and budgets: what a measured interval carries so that
its uncertainty is stated rather than assumed.

## What it is

A timestamp difference is not a measurement until something says how wrong it
might be. This crate is the accounting that makes it one.

You describe the **chain** each event travelled — the ordered stages between the
physical event and the recorded number — and write a **term** for each stage:
its worst-case interval, its trial-to-trial spread, any correction applied to
the value, and how each of those was arrived at. The composer adds them up.

What it adds up to is a `Total`, and `Total` is a sum type. A budget with a
stage nobody measured does not report a smaller number; it reports that there is
no total, names the terms responsible, and keeps the sum of everything that was
known as a separate figure. The distinction between "12 ms" and "12 ms of known
terms, plus an unmeasured display latency" is the one this crate exists to keep.

This is step 1 of the timing model: the core uncertainty types and the composer.
Calibrations with conditions and expiry, the uncertainty-carrying clock mapping,
sample clocks, requirements and the record schema are later steps.

One dependency, `extendedresearch-clock`, for the `UNBOUNDED` sentinel. No
platform calls, no `unsafe`, and no floating point anywhere — including the
quadrature sum, which accumulates squares in `u128` and takes an exact integer
square root.

## Install

```toml
[dependencies]
extendedresearch-metrology = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it.

## Use

```rust
use extendedresearch_metrology::{
    ArgumentId, Bias, Chain, ChainId, Composer, Link, LinkKind, Provenance, Span, Term, Total,
};

let argued = Provenance::Bounded { argument: ArgumentId(1) };

// A response: a contact closes, travels a wire, and the host stamps it.
let response = Chain::new(ChainId(1), [
    Link::of(LinkKind::Transduction),
    Link::of(LinkKind::Transport),
    Link::of(LinkKind::HostStamp),
]);
// A stimulus: the program submits a frame and the panel emits light.
let stimulus = Chain::new(ChainId(2), [
    Link::of(LinkKind::ApplicationSubmit),
    Link::of(LinkKind::Emission),
]);

let mut composer = Composer::new(response, stimulus);
composer
    // One stamp covering the contact, the wire and the read: a span, not a position.
    .term(Term::new(Span { chain: ChainId(1), from: 0, to: 2 }, Bias::symmetric(200_000), argued))
    .term(Term::new(Span::at(ChainId(2), 0), Bias::symmetric(100_000), argued))
    // Nobody measured the panel, so nothing is claimed about it.
    .term(Term::unknown(Span::at(ChainId(2), 1)))
    .stamps(1_450_000_000, 1_000_000_000);

let budget = composer.compose().unwrap();

assert_eq!(budget.interval_ns, Some(450_000_000));
assert!(matches!(budget.total, Total::Unbounded { .. }));
assert_eq!(budget.total.early_ns(), None);              // there is no total
assert_eq!(budget.total.partial_sums().early_ns, 300_000); // and this is not one
assert_eq!(budget.total.unbounded_terms(), &[2]);       // this term is why
```

Measure the panel and the same budget answers a number. A calibration is three
things that never merge — a correction, a residual bias, and a dispersion:

```rust
use extendedresearch_metrology::{
    CalibrationId, Correction, Dispersion, DistributionKind, Provenance,
};

let measured = Provenance::Measured { calibration: CalibrationId(1) };

// A photodiode: 18.2 ms lag, ±0.9 ms residual, 0.4 ms SD over 200 repetitions.
let panel = Term::new(Span::at(ChainId(2), 1), Bias::symmetric(900_000), measured)
    .with_correction(Correction::new(-18_200_000, measured))
    .with_dispersion(Dispersion::new(400_000, DistributionKind::Gaussian));
```

Not a bound of 18.2 ms, and not a bound of 0.4 ms.

## Guarantees

- **A bias and a dispersion never merge.** A worst-case interval and a standard
  deviation compose by different arithmetic, so they are separate fields.
  Reading a standard deviation as a bound understates the extreme; reading a
  bound as a standard deviation overstates the typical by roughly √3 for a
  uniform term.
- **Absence of knowledge is `UNBOUNDED`, not zero.** A zero bias is a positive
  claim that a stage contributes nothing. `Bias` has no `Default` for this
  reason, and a term whose bias provenance is unknown composes as unbounded
  whatever its bias field holds — enforced where the number is used, not only
  where it is written, because the fields are public.
- **One unknown term is never a finite total, in any composition order, through
  any public API.** `tests/properties.rs` checks this over generated term sets
  rather than over three examples, and enumerates every accessor that answers a
  width.
- **A link no term covers makes the total unbounded**, and the uncovered runs
  are named. A budget that accounts for only the stages someone remembered to
  describe composes cleanly and is wrong by however much the rest contribute.
- **Two corrections over one link are refused**, with both terms, the
  intersecting positions and the calibrations named. Two biases over one link
  are not refused: bounding a stage twice is over-conservative rather than
  wrong.
- **Composition is deterministic.** Terms compose in a documented order, so two
  runs over the same inputs produce equal budgets whatever order they were
  submitted in.
- **Integer arithmetic throughout**, with no documented exception. Widths
  saturate at `UNBOUNDED`, and a sum that reaches it is reported as no bound
  rather than as a very wide one.
- **The enumeration integers are a wire contract.** `LinkKind`, `Provenance`,
  `Correlation` and `DistributionKind` are written to a record as integers, and
  a value is never renumbered or reused. Zero means "nothing was declared" in
  all of them, so a truncated record cannot read as a measurement.
- **Twelve language-free JSON conformance vectors** in `vectors/`, run by
  `tests/vectors.rs`. A second implementation is correct when it reproduces
  them. Each row states the whole budget, so a row cannot pass on the field it
  is about while getting another wrong.

## Limits

- **The composer does not know what a stage is.** It checks that every position
  of both chains is covered by something; it cannot check that the term covering
  a position describes the right thing. A chain that names the wrong stages
  composes to a confident total.
- **A `Bounded` term is only as good as its argument.** The bound on a
  round-trip is `±min_rtt / 2` *if the path is symmetric*, and nothing in the
  observations reveals asymmetry — ordinary on radio, and on anything whose
  uplink and downlink differ. The argument the term names has to state the
  assumption, not the arithmetic; nothing here can check that it does.
- **A correlation group's second half is a claim nobody verifies.** Declaring a
  group says the terms inside it are correlated *and* independent of everything
  outside. The composer takes both halves on trust. `Undeclared` is the default
  precisely because the alternative flatters the measurement.
- **No calibrations yet.** Conditions, expiry and the refusals that go with them
  (R25, R26) are step 5. Today a calibration enters a budget as a term whose
  provenance names it, and nothing checks that it was applied in the conditions
  it was measured under.
- **Two stamps need two chains.** Composing both endpoints from one chain is
  refused rather than guessed at, so one physical path traversed twice is
  registered under two identifiers with the terms stated for each.
- **No record schema and no C ABI projection.** Nothing here is serialised yet,
  and the enumeration integers are frozen when the schema is (step 7), not
  before.
- **No magnitude ships as a default.** Every latency figure in this crate's
  documentation is the shape of a budget, not a measurement of anybody's
  hardware. A stage with no measurement is `Unknown`.

## Versioning

Pre-1.0. A minor release may change any name, code or signature. The
enumeration integers and the conformance vectors are the parts treated as a
contract, and even those are provisional until the record schema freezes them;
everything else is still moving.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
