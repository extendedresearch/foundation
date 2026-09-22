# Timing and uncertainty — architecture

Status: **proposed**, 2026-09-22. Companion to `timing-and-uncertainty.md`,
which holds the numbered requirements this document implements. Integration
steps for the consuming repositories are in `timing-integration.md`.

---

## 1. Two crates, and the seam between them

```
extendedresearch-clock          extendedresearch-metrology
────────────────────────        ──────────────────────────
Domain, DomainId                Chain, Link, LinkKind, Span
SuspendBehaviour                Term, Bias, Dispersion, Correction
Reading, Bound, Basis           Provenance, EstimatorId, CalibrationId
Interval                        Calibration, Conditions, Method
Anchor, WallTime                Budget, Composer
Observation, Line, Fit          Requirements, Profile, Violation
SlidingWindow, Subsample        Record, schema encode/decode
DriftCheck, Discontinuity       SampleClock
Quantum
                                depends on ─────────► clock
no dependencies                 no dependencies beyond clock
wasm32 clean                    wasm32 clean
```

**Why two.** The clock crate is read in a hot path, has no dependencies, and is
checked for `wasm32-unknown-unknown` in CI. The metrology crate owns
allocation, a schema and a policy engine. A program that wants research-grade
timing accounting without this project's clock takes metrology and supplies its
own readings; a program that wants a correct host clock and no budget takes
clock alone. Keeping them separate is what makes either sentence true.

**What moves.** Nothing. Every public item in `extendedresearch-clock` keeps its
name, signature and semantics (§7). The metrology crate is additive.

**What the seam is.** Metrology depends on clock for `Reading`, `Bound`,
`Basis`, `Domain`, `Fit` and `Line`. Clock never depends on metrology and never
learns what a term is.

---

## 2. The two-tier hot path

The first-principles draft asks for allocation-free reads at tens of thousands
per second *and* makes the public result a struct owning a list of terms per
interval. Both cannot hold. The resolution is that the two live at different
rates.

| Tier | What it is | Rate | Allocation |
|---|---|---|---|
| **Stamp** | `Reading`: domain id, `u64` nanoseconds, `Bound`, `Basis`, `ChainId` | per sample, 10⁴–10⁶/s | none; `Copy`, 32 bytes |
| **Budget** | `Budget`: the terms accounting for an interval | per interval, or per stream epoch | owns a `Vec<Term>` |

A stamp references its chain by a 32-bit identifier. The chain, its links, and
every term covering them live once per stream — not once per sample. A
1 kHz EEG stream recording for an hour produces 3.6 million stamps and one chain
description.

```rust
// clock: unchanged shape, one added field
#[derive(Clone, Copy)]
pub struct Reading {
    pub domain: DomainId,
    pub ns: u64,
    pub bound: Bound,      // early_ns, late_ns, basis
    pub chain: ChainId,    // new: which chain this stamp travelled
}
```

`ChainId` is a plain `u32` in the clock crate with no interpretation; metrology
gives it meaning. That keeps the clock crate free of the chain model while
letting a stamp carry the one field that makes its budget reconstructable.

**Rule.** A per-sample structure MUST NOT own a heap allocation, and MUST NOT
grow when a term is added to its chain. If satisfying a requirement would put a
`Vec` on a stamp, the design is wrong.

---

## 3. Core types

Sketches, not final signatures. Every integer enumeration is `#[repr(u16)]`,
`#[non_exhaustive]`, with values frozen at the first schema version (R13, R59).

```rust
// ---- chains -------------------------------------------------------------
pub struct Chain { pub id: ChainId, pub links: Vec<Link> }   // ordered, 0 = the event
pub struct Link  { pub kind: LinkKind, pub device: Option<DeviceId> }

/// Derived from what a measurement chain does, not from any one instrument.
/// An acquisition chain runs 1..10 in order; a stimulus chain runs 11..16; 17
/// and 18 apply to either. A chain uses the links it has and omits the rest.
#[repr(u16)] pub enum LinkKind {
    // ---- acquisition: a physical event becomes a number ----
    Transduction       = 1,   // the physical quantity becomes a signal
    AnalogConditioning = 2,   // amplification and analogue filtering
    Quantisation       = 3,   // sample-and-hold, ADC aperture
    DeviceFilter       = 4,   // on-device digital filtering; group delay
    DeviceBuffer       = 5,   // on-device accumulation before transmission
    DeviceStamp        = 6,   // the instrument applies its own timestamp
    Transport          = 7,   // wire, radio, or bus
    HostReceive        = 8,   // interrupt, driver, kernel
    HostQueue          = 9,   // scheduling and userspace wakeup
    HostStamp          = 10,  // the application reads a clock

    // ---- stimulus: a command becomes a physical event ----
    ApplicationSubmit  = 11,  // the program issues the command
    FrameworkQueue     = 12,  // graphics or audio API queueing
    Compositor         = 13,  // OS composition or mixing
    OutputBuffer       = 14,  // device-side buffer before conversion
    Conversion         = 15,  // DAC, scanout
    Emission           = 16,  // panel response, transducer rise

    // ---- either ----
    ClockMapping       = 17,  // an estimated mapping onto another timeline
    Serialisation      = 18,  // a record write that can delay or reorder a stamp
}

/// A closed range of link positions in one chain. Coverage is a span (R16).
#[derive(Clone, Copy)] pub struct Span { pub chain: ChainId, pub from: u16, pub to: u16 }

// ---- terms --------------------------------------------------------------
#[derive(Clone, Copy)] pub struct Bias { pub early_ns: u64, pub late_ns: u64 }
#[derive(Clone, Copy)] pub struct Dispersion { pub sd_ns: u64, pub kind: DistributionKind }

#[derive(Clone, Copy)] pub struct Term {
    pub covers: Span,
    pub bias: Bias,
    pub dispersion: Option<Dispersion>,
    pub correction_ns: i64,            // 0 when none (R5)
    pub provenance: Provenance,
    pub correlation: Correlation,      // Independent | CorrelatedWith(GroupId) | Undeclared
}

#[derive(Clone, Copy)] pub enum Provenance {
    Measured  { calibration: CalibrationId },
    Specified { document: DocumentId },
    Estimated { estimator: EstimatorId, inputs: InputsId },
    Bounded   { argument: ArgumentId },
    Unknown,
}
```

`Term` is `Copy` and fixed-size: every identifier is an integer into a side
table of the record (R18). That is what makes the same type usable in a hot
path, in a serialised record, and across the C ABI without a second design.

```rust
// ---- calibration --------------------------------------------------------
pub struct Calibration {
    pub id: CalibrationId,
    pub covers: Span,
    pub subject: DeviceId,
    pub correction_ns: i64,
    pub residual: Bias,
    pub dispersion: Option<Dispersion>,
    pub method: Method,                 // Photodiode | Loopback | Trigger | ...
    pub apparatus: StringId,
    pub performed: WallTime,
    pub valid_until: Option<WallTime>,
    pub conditions: Vec<(StringId, StringId)>,
    pub repetitions: u32,
}

// ---- budget -------------------------------------------------------------
pub struct Budget {
    pub terms: Vec<Term>,
    pub total: Total,
    pub largest_bounded: Option<usize>,  // index into terms (R34)
}

pub enum Total {
    Bounded   { early_ns: u64, late_ns: u64 },
    Unbounded {
        known_early_ns: u64,             // partial sum, never the total (R9)
        known_late_ns:  u64,
        unbounded_terms: Vec<usize>,
        uncovered_links: Vec<Span>,      // links no term covered (R11)
    },
}
```

`Total` is a sum type rather than a pair of saturating integers. A caller that
wants the number has to acknowledge which case it is in — which is the whole
point of R9, expressed in the type system rather than in a doc comment.

---

## 4. The mapping fix

The concrete defect in today's crate: `Fit` computes `bias_low_ns`,
`bias_high_ns`, `residual_spread_ns` and `skew_uncertainty_ppb`, and
`Line::map_to_reference(source_ns) -> u64` discards all four. A caller mapping a
source timestamp onto the host clock gets an integer with no indication that it
is an estimate.

```rust
// metrology
pub struct MappedReading { pub reading: Reading, pub terms: [Term; 3] }

impl Mapping {
    /// Map a source instant onto the host clock, carrying the fit's uncertainty.
    ///
    /// Emits three terms over the same span:
    ///   offset bias        — from Fit::bias_low_ns / bias_high_ns   (R40)
    ///   residual spread    — as dispersion, never bias              (R41)
    ///   extrapolation      — skew_uncertainty_ppb × distance / 1e9  (R42)
    pub fn map(&self, source_ns: u64) -> Result<MappedReading, MapError>;
}
```

The extrapolation penalty is
`skew_uncertainty_ppb × |source_ns − reference_source_ns| / 10⁹`, saturating,
and applied to both sides. Where `skew_uncertainty_ppb == SKEW_NOT_ESTIMATED`
the penalty is `UNBOUNDED`, which poisons the total by R8 — the correct answer
for a window too short to estimate a rate.

`Line::map_to_reference` stays exactly as it is, and becomes the documented
inner arithmetic that `Mapping::map` wraps. Nothing that compiles today stops
compiling.

---

## 5. Answering the ABI question concretely

Designing for the C ABI does not constrain the Rust. It adds a projection.

| Rust side | ABI side | Mechanism that already exists |
|---|---|---|
| `Budget` owning `Vec<Term>` | opaque handle | `AbiHandle` over `_destroy` |
| `budget.terms` | `_term_count` / `_term_at(i, *out)` | the enumeration trio (`Enumeration`, `AbiEnumeration`) |
| `Term` (fixed-size, `Copy`) | a flat `struct` of integers | plain `#[repr(C)]`, no pointer chasing |
| `Provenance` | `kind: u16` + `id: u32` | stable integers (R59) |
| `apparatus: StringId` | `_string(id, buf, cap, *out_len)` | measure-then-copy (`AbiBuffer`, `buffer.rs`) |
| nested list (budget → term → inputs) | a second trio keyed by the outer index | trios nest |
| `Total` sum type | `_total_kind()` + typed accessors | the same shape as `Result` over the boundary |

**Borrowed data and nested `Vec` are unaffected.** They live behind the handle.
What crosses is an index and a copy, which is the convention
`extendedresearch-abi` already documents for every other type in this project.

The real costs, stated plainly:

1. **Every integer is frozen forever.** `LinkKind`, `Method`,
   `DistributionKind`, `Provenance`'s discriminants and `Basis` become a wire
   contract on the day the first record is written. This is why R15 asks you to
   confirm `Basis` 7–10 *before* the schema freezes, not after.
2. **Two surfaces to keep in step.** The Rust API and the ABI projection need a
   test that walks every term of a budget through both and compares — the same
   shape as `crates/napi-testaddon`, which loads a real addon and runs the
   TypeScript against it.
3. **Strings become ids.** A caller wanting the apparatus name makes a second
   call. That is already true of every string in this project's C ABI.

The projection lands in the implementation order at step 7 (§8), after the model
is settled, because freezing integers before the model is settled is how you get
a reserved-for-future-use field.

---

## 6. The record

One canonical schema (R55), versioned, with language-free JSON conformance
vectors alongside the 22 the clock crate carries.

```
record
├── schema_version: u32
├── strings:       [String]                  // every StringId resolves here
├── devices:       [Device]
├── domains:       [Domain]                  // clock source, host, boot, suspend behaviour
├── anchors:       [Anchor]                  // calendar mapping with its own error
├── chains:        [Chain]
├── estimators:    [{ id, name, version }]   // R22, R65
├── calibrations:  [Calibration]
├── observations:  [[Observation]]           // the raw pairs each fit was built from
├── fits:          [{ id, estimator, inputs, Fit fields }]
├── stamps:        [Reading]                 // raw, never mapped (R56)
├── budgets:       [Budget]
├── requirements:  [Requirements]
└── violations:    [Violation]
```

**Stamps are raw.** A mapped timestamp is a function of a stamp and a fit; the
record stores both and a reader computes the mapping (R56). A recording whose
mapped values were baked in cannot be re-analysed when a calibration is
corrected, and a calibration will be corrected.

**Encoding.** The schema above is the logical model. The physical encoding is
per-consumer — ca3's container and ranvier's stream carry the same fields with
the same integers, and the vectors are what proves they agree. What foundation
owns is the field set, the integers, the composition arithmetic and the
vectors; what it does not own is whether ca3 writes them as MCAP channels or
ranvier as protobuf.

---

## 7. Migration: step 0 is a no-op

Every item the consumers use today keeps its exact signature:

| Item | Used by |
|---|---|
| `Observation`, `Line`, `Fit`, `fit_one_way` | ca3, ranvier |
| `SlidingWindow`, `Subsample` | ranvier, ca3 |
| `SKEW_NOT_ESTIMATED`, `MIN_OBSERVATIONS_FOR_SKEW`, `DEFAULT_TOLERANCE_NS`, `tolerance_for_rate` | ca3 |
| `Bound`, `Basis`, `Interval`, `bound_for_read` | both, on adoption |
| `Domain`, `DomainId`, `SuspendBehaviour`, `Anchor`, `WallTime`, `DriftCheck` | both, on adoption |

The one additive change to an existing type is `Reading.chain: ChainId` (§2).
It is constructed by `SuppliedClock::at`, so a caller that ignores chains passes
`ChainId::UNSPECIFIED` and behaves exactly as today.

**A consumer can therefore take the new version, change nothing, and still
compile.** Everything else is opt-in, one call site at a time. That is the
property that lets ca3 and ranvier refactor while implementation continues here.

---

## 8. Implementation order

Ordered by how much each improves the defensibility of a recorded number, with
the constraint that nothing already working may regress.

| # | What | Why here |
|---|---|---|
| 0 | `extendedresearch-metrology` skeleton; `ChainId` added to `Reading` | Both consumers compile against the new version unchanged |
| 1 | `Bias`, `Dispersion`, `Term`, `Provenance`, `Span`, `Total`, the composer, overlap refusal, uncovered-link detection | Until an interval carries its decomposition, every improvement below is invisible and every regression silent |
| 2 | `Chain`, `Link`, `LinkKind`; `Basis` reconciled to chain positions; confirm `Basis` 7–10 | Coverage and overlap are meaningless without the chain |
| 3 | `Mapping::map` — the fix in §4 | Closes a live hole where uncertainty is silently discarded |
| 4 | `SampleClock`: index → source → host, with the extrapolation limit | The dominant practical case for both consumers |
| 5 | `Calibration`, `Conditions`, expiry, overlap refusal | Lets the dominant terms enter a budget at all |
| 6 | `Requirements`, `Profile`, `Violation`; configuration and per-interval checks | Turns the model into a contract a session can declare |
| 7 | Record schema, encode/decode, conformance vectors | Freezes the integers, once the model has stopped moving |
| 8 | C ABI projection (§5) | After the integers are frozen |
| 9 | Hardware stamp paths | Mostly consumer-side; foundation supplies the `Basis` values and the chain shapes |

Steps 1–4 are what ca3 and ranvier need in order to start refactoring. Steps 5–6
are what a clinical or publication claim needs. Steps 7–8 are what a second
implementation and a non-Rust caller need.

---

## 9. Testing

**Conformance vectors are normative (R62).** Every composition rule gets one:
crosswise addition, saturation, unknown poisoning, partial-sum retention,
overlap refusal, uncovered-link detection, quadrature versus linear dispersion,
extrapolation penalty, calibration condition mismatch, expiry.

**Property tests** for the arithmetic: composition is associative and
commutative over terms; adding a term never narrows a total; a bounded total
never exceeds the sum of its parts; mapping forward then back lands within the
documented one-nanosecond window.

**Differential tests** against the existing crate: for every input the old
`Line::map_to_reference` accepts, `Mapping::map(..).reading.ns` equals it
exactly. The new layer changes what is *reported*, never what is *computed*.

**Miri** over any pointer handling the ABI projection introduces, as
`crates/abi` already runs.

**The negative test that matters most:** a budget with one unknown term must
never produce a finite total, in any composition order, through any public API.
Write it as a property test over generated term sets, not as three examples.

---

## 10. Decisions, and the one question left

These were open when this document was drafted. They are decided here, on the
derivation given, rather than referred to a consumer.

**`LinkKind`'s value set** (§3) is derived from the two directions a measurement
chain can run — a physical event becoming a number, and a command becoming a
physical event — plus the two links that apply to either. It is not an
enumeration of what any consumer happens to have. A chain omits the links it
does not have; a chain needing a link the set lacks is a reviewed addition, and
the integers append.

**Extrapolation limit** (R47): **caller-stated, no default, refuse when
absent.** A default would put a number nobody measured into a record that claims
provenance, which R66 forbids. A caller who does not know how far it may
extrapolate does not get a silently chosen answer; it gets a refusal naming the
missing policy.

**Correlation declaration** (R4): **`Undeclared` is the default, and it refuses
to combine dispersions.** Defaulting to `Independent` would compose in
quadrature and produce a smaller total, which is wrong whenever two terms share
a cause — routine for two terms on one device, where a single temperature drift
or a single clock moves both. A default that is wrong in the direction of
flattering the measurement is the failure this project exists to avoid. The cost
is that a caller wanting the smaller number states independence explicitly,
which is the claim it was making anyway.

**The one question left is factual, not a design choice.** `Basis` values 7–10
are marked provisional in `bound.rs` and freeze at step 7 (R15). If any records
already written carry them, the freeze has to preserve those values rather than
renumber. That is a question about deployed data, and only whoever wrote those
records can answer it.
