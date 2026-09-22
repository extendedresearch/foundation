# Timing and uncertainty — specification

Status: **proposed**, 2026-09-22. Not implemented. Numbered requirements are
normative and are cited by `timing-architecture.md` and
`timing-integration.md`.

**MUST**, **MUST NOT**, **MAY** and **SHOULD** carry their usual force. Every
requirement has an identifier of the form `R<n>`; identifiers are never reused
or renumbered, and a withdrawn requirement is marked withdrawn in place.

---

## 0. Scope

This specifies how a measurement's timing uncertainty is represented, composed,
recorded and refused. It covers two crates:

| Crate | Owns |
|---|---|
| `extendedresearch-clock` | Clock domains, readings, bounds, one-way fits, calendar anchors, drift detection, timer quanta. Exists today; this specification extends it and removes nothing |
| `extendedresearch-metrology` | Chains, terms, provenance, calibration, budgets, requirements, the record schema. New |

**Out of scope.** Acquiring a platform clock reading (each consumer's own
platform layer), the physical apparatus of a calibration, statistical inference
beyond the composition rules below, and any transport or storage mechanism
other than the canonical record encoding in §12.

**This is a general instrument-timing system, not a shared part of three
programs.** Every rule below is derived from what a measurement chain does and
what a defensible claim requires, not from what `ca3`, `ranvier` or `eres`
happen to do. Those repositories adopt it because it is the more complete
answer; where one of them already reached the same answer, that is convergence
and is noted as such, and where one differs, the rule here governs. A program
with no connection to this project takes the same crates and gets the same
guarantees.

**The design serves every fidelity, and mandates none.** A session that knows
nothing about its display latency is representable, and produces a total
uncertainty that is unbounded rather than absent. A session with a photodiode
calibration and a hardware trigger is representable by the same types. Nothing
in this specification requires a particular instrument, a particular clock
source, or a particular level of rigour — §11 is how a caller imposes one on
itself.

---

## 1. The claim this exists to support

Every research or clinical use of this software reduces to a claim of the form:

> Event A and event B, observed by different instruments, were separated by
> *t* ± *u*.

Three consequences shape everything below.

**The deliverable is an interval, not a timestamp.** No one publishes a clock
reading. A system organised around producing good timestamps and leaving the
subtraction to the caller has shipped the easy half.

**Without *u*, *t* is not evidence.** A 450 ms reaction time means one thing if
*u* is 2 ms and nothing at all if *u* is 60 ms and unstated.

**A wrong *t* that looks right is the failure that matters.** Timing defects do
not crash. They produce numbers that pass every plausibility check, get
analysed and are wrong. The system's first obligation is to refuse rather than
guess; its second is to make every number it emits reconstructable from the
record.

---

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Event** | A physical occurrence: photons leaving a panel, a contact closing, a neuron firing |
| **Chain** | The ordered sequence of stages an event traverses to become a recorded number |
| **Link** | One stage of a chain |
| **Stamp** | The act of reading a clock for an event, at a named position in the chain |
| **Basis** | The chain position at which a stamp was taken |
| **Term** | One named contribution to uncertainty, covering a span of one chain |
| **Bias** | Worst-case asymmetric interval: how far the true value may sit from the stated one |
| **Dispersion** | Trial-to-trial spread, expressed distributionally |
| **Correction** | A signed integer applied to a value, derived from a calibration |
| **Provenance** | How a term's numbers were arrived at |
| **Budget** | The set of terms accounting for one interval, plus its composed total |
| **Domain** | One host timeline, identified by clock source, host, boot and suspend behaviour |
| **Reading** | A clock value on a named domain, with a bound and a basis |
| **Fit** | An estimated mapping from one clock onto another |
| **Calibration** | A record of an externally measured latency, scoped to conditions |
| **Requirements** | A caller-declared floor that intervals are checked against |

---

## 3. Two currencies, never mixed

A worst-case interval and a standard deviation compose by different arithmetic.
Widths of a worst-case interval add linearly; standard deviations of
independent variables add in quadrature. Putting both in one field makes the
composition unsound in whichever direction it is wrong: treating an SD as a
bound understates the extreme, and treating a bound as an SD overstates the
typical by roughly a factor of √3 for a uniform term.

- **R1** A bias MUST be a pair of unsigned nanosecond widths `(early_ns,
  late_ns)` asserting that the true instant lies in `[value − early_ns, value +
  late_ns]`. It MUST NOT hold a standard deviation, a symmetric half-width, or
  a confidence interval.
- **R2** A dispersion, where present, MUST be a separate field carrying a
  standard deviation in nanoseconds together with the distribution kind it was
  derived under.
- **R3** Biases compose by addition, saturating at `UNBOUNDED` (§4). For the
  difference of two stamps the widths add **crosswise**: for `later.since(earlier)`
  with bounds `(e_a, l_a)` and `(e_b, l_b)`, the result's low width is
  `e_a + l_b` and its high width is `l_a + e_b`. The later event happening early
  and the earlier one happening late both shrink the interval.
- **R4** Dispersions compose in quadrature **only** between terms that declare
  independence of one another. Terms declaring correlation add linearly. Where
  the correlation is not declared, the composer MUST NOT combine them: it
  reports them per-term and marks the combined dispersion unknown.
- **R5** A calibration yields three outputs, and they never merge: a
  **correction** (signed nanoseconds applied to the value), a **residual bias**
  (what the calibration could not remove), and optionally a **dispersion**.
  A measured latency MUST NOT be expressed by widening a bound.
- **R6** A zero bias is a positive claim that the term contributes nothing. The
  absence of knowledge is `UNBOUNDED` on both sides, never zero.

*Worked example.* A photodiode measures a panel's output lag as 18.2 ms with a
0.4 ms standard deviation over 200 repetitions, and a residual worst case of
±0.9 ms about the mean. That is `correction = −18_200_000`, `residual bias =
(900_000, 900_000)`, `dispersion = 400_000 ns, Gaussian`. It is not a bound of
18.2 ms, and it is not a bound of 0.4 ms.

---

## 4. Unknown, and how it poisons

- **R7** `UNBOUNDED` is `u64::MAX` and means "no bound is asserted". It is
  already this crate's sentinel and MUST keep that value.
- **R8** If any term contributing to a side of a total is unbounded on that
  side, the total MUST be unbounded on that side.
- **R9** A budget MUST retain, and MUST expose separately, the sum of its
  bounded terms and the list of its unbounded ones. It MUST NOT present that
  partial sum as the total. The distinction the caller needs is between
  "12 ms" and "12 ms of known terms, plus an unmeasured display latency".
- **R10** `Provenance::Unknown` implies an unbounded bias on both sides. A term
  whose provenance is unknown cannot carry a finite bound.
- **R11** A chain link covered by no term MUST make the budget unbounded, by
  the same rule as R8. A budget that silently accounts for only the links
  someone remembered to describe is the failure this specification exists to
  prevent.

---

## 5. Chains, links, basis and spans

An interval's uncertainty is a property of the **path** each event travelled.
Making the path explicit is what lets terms compose correctly, lets overlapping
corrections be detected, and tells a researcher where effort pays.

- **R12** A chain is an ordered, finite sequence of links, from the physical
  event to the recorded number. Position 0 is the physical event.
- **R13** Each link carries a stable integer kind. The integers are a wire
  contract: never renumbered, never reused, appended on extension.
- **R14** A stamp carries a `Basis` naming the chain position at which the
  number was taken. **Links after that position contribute nothing to that
  stamp's uncertainty.** A hardware timestamp does not shrink a term; it
  shortens the chain, and the record MUST show the shorter chain rather than a
  smaller number.
- **R15** `Basis` integers are already a wire contract in
  `extendedresearch-clock` and MUST keep their current values. Values 7 to 10
  are marked provisional in the source and MUST be confirmed or changed before
  the first record schema version is frozen (§12).
- **R16** A **span** is a closed range of link positions within one named
  chain. Coverage MUST be expressed as a span. A kind tag alone is insufficient:
  two calibrations can both describe "device delay" for different devices, and a
  trigger alignment covers a contiguous run of links rather than one.

*Why this is the highest-value engineering.* Cutting links out of a chain beats
estimating them. In descending order of what each removes: a hardware trigger to
the instrument removes everything between stimulus and instrument; a photodiode
replaces all display-output uncertainty with a measurement; kernel socket or
evdev timestamps remove scheduling and userspace delay; PTP with hardware
timestamping removes cross-host clock estimation; audio loopback removes output
buffer uncertainty. A design offering an excellent software clock and no path to
any of these has optimised the smallest term.

---

## 6. Terms and provenance

- **R17** A term MUST carry: the span it covers, a bias, an optional
  dispersion, an optional correction, and a provenance.
- **R18** A term MUST be fixed-size and `Copy`. Free text MUST NOT appear in a
  term; it lives in a side table of the record, addressed by identifier. This
  keeps a term usable in a hot path and representable across the C ABI without
  a second design.
- **R19** `Provenance` MUST distinguish at least: `Measured` (naming a
  calibration identifier), `Specified` (naming a document identifier — a vendor
  claim, unverified), `Estimated` (naming a versioned estimator identifier and
  the inputs it ran on), `Bounded` (naming a stored argument identifier — a
  reasoned worst case), and `Unknown`.
- **R20** A measured 18.2 ms display latency and a datasheet's "typical 16 ms"
  are different epistemic objects. They MUST NOT flatten to the same
  representation. They age differently, they compose with different confidence,
  and a reviewer will ask which you had.
- **R21** Every identifier a provenance names MUST resolve within the record
  that carries the term (§12).
- **R22** An `Estimated` term MUST name the estimator **version**. Two
  implementations differing in the last nanosecond produce timestamps that look
  comparable and are not.

---

## 7. Calibration

The dominant terms — transduction, buffering, instrument-internal filtering —
are not measurable from software at any clock precision. They are measured with
external instrumentation or they are unknown, and "unknown" must be
representable (R10).

- **R23** A calibration MUST carry: the span it covers, its subject device, its
  value as (correction, residual bias, optional dispersion), the method, the
  apparatus by name and model, when it was performed, an optional expiry, the
  conditions it was performed under, and the number of repetitions.
- **R24** Conditions MUST be recorded as structured key/value pairs, not prose.
  A display latency measured at 1920×1080 and 60 Hz says nothing about the same
  panel at 144 Hz.
- **R25** Applying a calibration outside its recorded conditions MUST be an
  error. It MUST NOT be silently skipped, and it MUST NOT be silently applied.
- **R26** A calibration with an expiry MUST NOT be applied past it. A
  calibration with no expiry is a claim that nothing changed, which is a claim
  nobody checked; the record MUST make the absence of an expiry visible rather
  than treating it as permanence.
- **R27** Two calibrations whose spans intersect MUST NOT both apply to one
  budget. The composer refuses rather than summing. A trigger-based alignment
  already contains the device delay; applying both corrects twice, by exactly
  the size of the term the calibration exists to capture.
- **R28** A refusal under R25, R26 or R27 MUST name the calibrations involved
  and the overlapping or violated span.

---

## 8. Composition

- **R29** A budget composes the terms of one interval: the terms covering the
  chain of the earlier stamp and those covering the chain of the later one.
- **R30** Composition MUST be deterministic: a documented, stable ordering over
  terms, so that two runs over the same inputs produce bit-identical totals.
- **R31** The composer MUST detect and refuse span overlap (R27), and MUST
  detect chain links covered by no term and mark the total unbounded (R11).
- **R32** Corrections apply to the value before bias composition. The order is:
  resolve each stamp's value with its corrections applied, subtract, then
  compose biases crosswise (R3).
- **R33** A budget MUST report, per side: the composed total, the partial sum of
  bounded terms, and the identities of unbounded terms (R9).
- **R34** A budget SHOULD report which single term contributes the largest
  bounded width, since that is the number that tells a researcher where to
  spend effort.

---

## 9. The clock

The clock is one term in the budget and nearly the smallest. It earns its place
by making every other term comparable: each is measured on some clock, and if
two instruments disagree about elapsed time then no calibration composes.

- **R35** One timeline per host, unambiguously identified. A domain's identity
  MUST include the clock source read and its suspend behaviour, not only the
  host and boot. Two readings whose domains are not byte-equal MUST NOT
  subtract. *(Implemented.)*
- **R36** A direct clock read's bound MUST derive from the clock's resolution
  and its rounding direction. A clock whose resolution cannot be established
  yields `Bound::UNKNOWN`. *(Implemented.)*
- **R37** A clock's quantum is a floor under every interval derived from it and
  MUST appear as a term. Uniform quantisation of width *w* has worst-case
  ±*w*/2 and standard deviation *w*/√12; these are different columns (R1, R2)
  and both belong in the record. A 15.6 ms quantised source contributes about
  4.5 ms of standard deviation on its own, larger than most effects measured
  with it.
- **R38** A detected discontinuity ends the domain. Readings taken across it
  MUST NOT subtract. *(Implemented.)*
- **R39** Mapping a reading from one domain onto another MUST produce a
  reading whose bound includes the fit's bias (R40), its residual spread as
  dispersion (R41) and its extrapolation penalty (R42).
- **R40** A one-way fit cannot separate a constant transport delay from a clock
  offset, so its offset bias is `UNBOUNDED` below and `0` above: the mapping
  never places an event too early, and nothing bounds how late.
  *(Implemented — `Fit::bias_low_ns`, `Fit::bias_high_ns`.)* Narrowing it
  requires a round trip, or a marker the source records in its own timeline, or
  a calibration; each of those MUST enter as its own term with its own
  provenance.
- **R41** A fit's residual spread is **precision, not accuracy**: it is the
  variance the fit removed and says nothing about how far the line sits from
  truth. It MUST enter a budget as dispersion (R2), never as bias.
  *(Field exists; the rule is new.)*
- **R42** **A mapping MUST propagate its uncertainty.** Today
  `Line::map_to_reference` returns a bare integer, discarding the offset bias,
  the residual spread and the skew uncertainty the fit computed. The new layer
  MUST offer a mapping that returns a bounded reading. The extrapolation
  penalty grows with distance from the fit's origin: `skew_uncertainty_ppb ×
  |source_ns − reference_source_ns| / 10⁹`, saturating.
- **R43** Cross-host subtraction MUST be representable. It produces a term
  whose bias is either bounded — where PTP with hardware timestamping, a round
  trip, or a calibration supplies a bound — or `Unknown`, which poisons the
  total by R8. Refusing cross-host subtraction structurally is **not**
  permitted: the primary claim in §1 is about different instruments, which are
  routinely different hosts. A caller that needs the stricter behaviour gets it
  from §11, as policy.

---

## 10. Sample clocks

An instrument streaming at a nominal rate has its own oscillator, drifting
against the host. The interval a researcher wants routinely spans "sample *n*
on device *D*" to "a host event at *t*". This is the dominant practical case
for both recording and streaming, and it is a rate mapping rather than an
offset.

- **R44** A sample clock MUST map a sample index to a source-clock instant
  through its nominal rate, and MUST record the nominal rate it used.
- **R45** The device's own sampling quantum MUST appear as a term, by R37.
- **R46** Mapping a sample index onto the host clock goes through the fit and
  carries R39 to R42 in full.
- **R47** Extrapolating beyond the fit window MUST widen the bound by R42, and
  beyond a caller-stated limit MUST refuse rather than extrapolate further.
- **R48** A stream's nominal rate implies its tolerance: one sample period,
  `10¹² / nominal_rate_millihz` nanoseconds. A stream declaring no rate uses
  the default tolerance, and the record MUST show that this is what happened.
  *(Implemented — `tolerance_for_rate`.)*

---

## 11. Requirements, and refusal as policy

Different settings demand different floors. A pilot study tolerates unknowns; a
clinical assessment does not. Neither floor belongs in the type system.

- **R49** The default policy is permissive. With no requirements declared,
  nothing is refused and every scenario is representable, including one in which
  nothing is known.
- **R50** Requirements are opt-in and declared per session. They MAY constrain:
  a maximum total uncertainty, whether unknown terms are forbidden, which term
  kinds require a calibration, a maximum calibration age, and which devices
  require a hardware stamp.
- **R51** Requirements MUST be checked at two points. **At configuration**:
  can this rig, with these calibrations, meet this floor at all? **At each
  interval**: does this one meet it? The first catches a misconfigured session
  before a participant sits down; the second catches a calibration expiring
  mid-session.
- **R52** A violation MUST be a structured value naming the requirement, the
  term or calibration responsible, and the margin by which it failed. It MUST
  NOT be a panic and MUST NOT be a log line.
- **R53** The failure MUST be available while the participant is still in the
  chair, not in the statistics three months later.
- **R54** Named profiles MAY be shipped as presets. They MUST NOT be defaults.
  A profile is an example of a floor someone chose, not a claim about what
  research requires.

---

## 12. The record

A recorded interval must be re-derivable from the file alone, years later, by
someone who has none of this software.

- **R55** Foundation defines one canonical, versioned schema. Every consumer
  encodes the same thing, so a recording written by one is re-derivable by
  another.
- **R56** **Derived values MUST NOT be stored as facts.** A mapped timestamp is
  a function of raw data and a mapping. Store both; compute the derived value on
  read. A file storing only the mapped value cannot be re-analysed under a
  corrected calibration, and a calibration will be corrected.
- **R57** The record MUST carry: domains, calendar anchors, chains and their
  links, calibrations, estimator identities and versions, the observations each
  fit was built from, the fits, terms, budgets, the requirements in force, and
  every refusal that fired.
- **R58** Every identifier used anywhere in the record MUST resolve within it.
- **R59** Every integer enumeration crossing the schema MUST have stable values
  (R13, R15).
- **R60** A reader encountering an unknown enumeration integer MUST preserve it
  rather than dropping the record that carries it.
- **R61** The schema MUST be versioned, and the version MUST appear in every
  record.
- **R62** Conformance vectors are normative. A second implementation is correct
  when it reproduces them, and the vectors MUST be language-free JSON, as the
  22 the clock crate already carries are.

---

## 13. Reproducibility

- **R63** Arithmetic MUST be integer throughout, with one documented exception:
  the root-mean-square accumulations in the fit, in IEEE-754 binary64, in
  observation order, truncated to whole nanoseconds. Another implementation
  reproduces them by the same operations in the same order. *(Implemented.)*
- **R64** Wherever floating point is unavoidable, the ordering MUST be
  documented and vectored.
- **R65** Estimators MUST be versioned, so that "which arithmetic produced
  this" is answerable from the record.
- **R66** **No magnitude table ships as a default.** Every latency figure in
  any design document is the shape of a typical budget, not a measurement of
  anybody's hardware. Shipping one as a default value would put an unmeasured
  number into a record that claims provenance. A term with no measurement is
  `Unknown` (R10).

---

## 14. What this specification does not require

It does not require a minimum fidelity. It does not require PTP, a photodiode, a
trigger box, or a hardware timestamp. It does not require a calibration to
exist. It does not perform statistical inference beyond the composition rules in
§3 and §8, and in particular it does not convert a bound into a confidence
interval or assume a distribution that no term declared.

What it requires is that whatever you did know is stated, whatever you did not
is marked unknown, and the difference between the two survives into the file.

---

## 15. The standard to hold an implementation to

A researcher should be able to answer, from the file alone:

- What was the interval, and what is its total uncertainty?
- Which term dominates, and how was each arrived at?
- Which calibrations were applied, when were they made, and under what
  conditions?
- Which clock was read, and did it survive a suspend?
- What arithmetic produced the derived values, and can I re-derive them?
- What did the system refuse to report, and why?

A system answering all six produces measurements that survive review. A system
answering none can produce identical-looking numbers, which is why the
distinction has to be structural rather than documentary.

---

## Provenance of this document

**Derived from** `foundation-timing-first-principles.md` (drafted 2026-09-22),
whose load-bearing claims — the interval as deliverable, the clock as a small
term, unknown poisoning the total, hardware stamps removing links, bias and
variance never merging, derived values never stored — are adopted here.

**Departures from that draft**, each argued at the requirement: bias and
dispersion are separate fields with separate composition (R1–R4, resolving a
contradiction between its §3.1 and §3.2); a calibration yields a correction
rather than a wider bound (R5); coverage is a span rather than a kind (R16);
cross-host subtraction is represented rather than refused (R43); the sample-clock
mapping is in scope (§10, absent from the draft); the record schema is specified
(§12, absent); and refusal is opt-in policy rather than a structural floor
(R49, R54).

**Verified against the tree**, not recalled: `Bound` is worst-case by
construction and documents why; `Interval` composes crosswise and saturates to
`UNBOUNDED`; `Basis` enumerates chain positions and marks values 7–10
provisional; `Fit` separates `residual_spread_ns` from `bias_low_ns`/
`bias_high_ns` and sets the latter to `UNBOUNDED`/`0` for one-way data;
`Line::map_to_reference` returns a bare `u64`; 22 JSON vectors exist.

**Not verified:** every magnitude in §9's quantisation example and §5's ordering
of mechanisms are domain knowledge, not measurements of your hardware. R66 is
the rule that keeps them out of the code.
