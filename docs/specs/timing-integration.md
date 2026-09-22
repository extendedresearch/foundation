# Timing and uncertainty — integration

Status: **proposed**, 2026-09-22. What `ca3`, `ranvier` and `eres` each need in
order to refactor against `timing-and-uncertainty.md` while the implementation
proceeds here. `R<n>` cites that document; `timing-architecture.md` holds the
type sketches and the implementation order.

---

## 0. The promise that makes parallel work possible

**Step 0 is a no-op.** Every public item the three repositories use today keeps
its name, signature and semantics. The new version compiles against unchanged
consumer code, and every new capability is opt-in one call site at a time.

The single additive change to an existing type is a `chain: ChainId` field on
`Reading`, constructed by `SuppliedClock::at`. A caller with no chain passes
`ChainId::UNSPECIFIED` and behaves exactly as today.

So: take the version bump first, refactor afterwards, in whatever order suits
each repository.

---

## 1. Where ca3's implementation and this specification converge

Every rule in the specification is derived there, from what a measurement chain
does. Several land on answers ca3 already reached independently. That is
convergence, not inheritance: the rule governs because it is derived, and ca3's
code is evidence the derivation is workable rather than the reason for it.

Listing them matters for one practical purpose — these are the places ca3
refactors by moving code rather than by rewriting it.

| Converges with ca3's | Lands in | Why the rule exists |
|---|---|---|
| `ClockDomain`, `DomainKind` (`crates/container/src/records.rs`) | `extendedresearch-clock` (reconcile with `Domain`) | ranvier needs the same identity rules; two definitions of a domain is how two files disagree about whether they may subtract |
| `Quality`, `SyncMethod` | `extendedresearch-metrology` as `Provenance` + `Method` (R19, R23) | The same distinction, generalised past clock sync to every term |
| `BIAS_UNBOUNDED` | `extendedresearch-clock::UNBOUNDED` (exists) | Already the same sentinel; ca3 re-exports the crate's estimator, so the constant should follow |
| `ClockDomain::map_to_reference` / `inverse_map` (`records.rs:1324`, `:1363`) | `extendedresearch-metrology::Mapping::map` (R42) | Both consumers wrap `Line` identically; the wrapper is where the uncertainty should be reattached |
| "Derived time is an export, not a fact of record" (`sync.rs` header) | R56 | Already ca3's position; this specification adopts it verbatim for every consumer |
| its rule that one estimator is normative | R22, R65 | The same rule, expressed as estimator versioning |
| its rule against double correction | R27 | ca3 states the rule; R27 makes it structural, via spans |

**Where the two differ, this specification governs**, because its rules are
derived rather than adopted. §5 lists the differences I know of, with the
derivation for each, so a reviewer can attack the reasoning rather than the
provenance. Mapping them onto ca3's own section numbering belongs in ca3's tree:
foundation is tier 1 and does not cite a consuming repository's specification.

---

## 2. ranvier

### 2.1 What it uses today

One site: `crates/api/src/data.rs:29`, `:436-465`.

```rust
window: SlidingWindow,
line:   Option<Line>,
// …
self.window.push(Observation { source_ns, receipt_ns });
// …
self.line.map(|line| line.map_to_reference(source_ns))   // :465
```

### 2.2 The one defect to fix

**Line 465 is the silent-uncertainty-loss site.** It returns `Option<u64>`: a
mapped timestamp with no bound, no basis, and no indication that it is an
estimate whose offset bias is unbounded below (R40). A subscriber receiving that
number cannot tell it from a direct clock read.

The replacement:

```rust
// before
pub fn map(&self, source_ns: u64) -> Option<u64>

// after
pub fn map(&self, source_ns: u64) -> Option<MappedReading>
//   .reading  — ns as before, plus Bound and Basis
//   .terms    — offset bias (R40), residual spread as dispersion (R41),
//               extrapolation penalty (R42)
```

`MappedReading::reading.ns` equals what line 465 returns today, bit for bit —
the differential test in `timing-architecture.md` §9 pins this. Nothing about
the value changes; what changes is that the uncertainty travels with it.

### 2.3 What to do, in order

1. **Now, no dependency on foundation work.** Decide what ranvier's chains are:
   for each stream kind, the ordered links from the physical event to the stamp
   ranvier records. This is the input to `Chain` (R12) and nothing in foundation
   can guess it. A first pass naming the links per transport is enough.
2. **Now.** Audit every place a timestamp crosses ranvier's public surface and
   record whether it is a direct read, a platform event timestamp, or a mapped
   estimate. That set becomes the `Basis` assignment (R14), and a stamp whose
   basis you cannot name is a finding.
3. **After foundation step 1** (terms and composition): adopt `Term` and
   `Budget` for the streams whose chains you named.
4. **After foundation step 3** (the mapping fix): replace `data.rs:465` as
   above. This is the change that most improves what a ranvier subscriber can
   claim.
5. **After foundation step 4** (sample clocks): move stream rate handling onto
   `SampleClock`, with the extrapolation limit (R47) rather than unbounded
   extrapolation past the window.

### 2.4 The cross-host question, settled

Ranvier streams between hosts by design. **Nothing in this specification
forbids that subtraction** (R43). Two readings from different domains produce an
interval carrying a `clock_offset` term whose bias is bounded where PTP with
hardware timestamping or a round trip supplies a bound, and `Unknown` otherwise
— which makes the total unbounded (R8) without making the operation
unavailable. A deployment that wants the stricter behaviour declares it as a
requirement (R50), and gets a structured violation rather than a compile error.

---

## 3. ca3

### 3.1 What it uses today

| Site | Item |
|---|---|
| `crates/container/src/records.rs:33` | `Line` |
| `crates/container/src/records.rs:1177` | re-exports `SKEW_NOT_ESTIMATED` |
| `crates/container/src/sync.rs:42` | `fit_one_way` |
| `crates/container/src/sync.rs:53,56,69,75` | re-exports `MIN_OBSERVATIONS_FOR_SKEW`, `Observation`, `DEFAULT_TOLERANCE_NS`, `tolerance_for_rate` |

### 3.2 What ca3 already gets right, and should keep

- **Offline fitting from the recorded observations.** `sync.rs` fits after the
  session from `(source_timestamp_ns, record_ts_ns)` pairs already in the file,
  so a mapping can be refitted later with a better estimator over the same
  bytes. That is R56 and R65, implemented before this document existed.
- **`BIAS_UNBOUNDED` reported rather than a placeholder.** `sync.rs`'s header
  states the limit as a limit. That is R40.
- **One estimator, cited as normative.** That is R22.

None of this changes, because the derivation arrives at the same place. Where
that happens, ca3 refactors by moving code rather than rewriting it.

### 3.3 What to do, in order

1. **Now.** Name ca3's chains (R12), per recorded device kind. Same task as
   ranvier's, different set.
2. **Now.** For each `Quality` and `SyncMethod` value ca3 records, write down
   the `Provenance` variant it maps to (R19). Where a value maps to none, that is
   a gap in either the enumeration or the specification, and it is worth finding
   before the integers freeze.
3. **After foundation step 1.** Replace ca3's own uncertainty accounting with
   `Term` and `Budget`. ca3's four-term error model is the direct ancestor of
   R17; the change is that coverage becomes a span (R16) so overlap is refused
   structurally rather than by a written rule.
4. **After foundation step 3.** Have `ClockDomain::map_to_reference`
   (`records.rs:1324`) delegate to `Mapping::map` and return the bound.
   `inverse_map` (`:1363`) follows.
5. **After foundation step 7.** Reconcile ca3's container encoding with the
   canonical record field set (R55). ca3 keeps its own physical encoding; what
   must agree is the field set, the integers and the arithmetic.

### 3.4 The one thing to decide early

ca3 owns the recording format, so ca3 is where a wrong integer is expensive.
`Basis` values 7–10 are marked provisional in foundation's source and freeze at
step 7 (R15). If ca3 has already written records carrying any of them, say so
before the freeze.

---

## 4. eres

eres has no dependency on `extendedresearch-clock` today — confirmed by grep
across its manifests, lockfiles and sources. Its runtime is not built yet.

**What this means for eres:** nothing is required now, and the sequencing above
does not block on it. When the runtime lands, take the crates at whatever
version exists then; the model will be settled and the integers frozen, which is
the better moment to arrive.

**What eres should do before then:** the same step 1 as the others — name the
chains for whatever it will time. Doing that while the model is still moving is
the cheapest way to find a link kind (R13) the enumeration is missing, and eres
is the consumer whose requirements are least represented in the design so far.

---

## 5. Where this differs from ca3's current implementation

Each row is decided here, on the derivation in the middle column. They are
listed so ca3 knows what changes under it, not as open questions.

| # | ca3 today | This specification | Why |
|---|---|---|---|
| 1 | Uncertainty is a bound | Bias and dispersion are separate fields, composed differently (R1–R4) | Worst-case widths add linearly; standard deviations add in quadrature. One field cannot serve both without being wrong in one direction |
| 2 | A calibration widens a bound | A calibration yields a correction plus a residual bias (R5) | A measured 18.2 ms lag is a shift, not an uncertainty. `Bound`'s own documentation already says a calibrated value never goes in either field |
| 3 | Double-correction refused by a written rule | Refused structurally, by span intersection (R27) | A rule in prose is enforced by whoever remembers it |
| 4 | Error terms are named kinds | Coverage is a span of a named chain (R16) | Two devices can both contribute "device delay", and a trigger alignment covers a run of links rather than one |
| 5 | Mapped values reported as integers | Mapping returns a bounded reading (R42) | The fit computes four uncertainty figures and the mapping currently discards all four |
| — | Derived time is an export | Adopted unchanged (R56) | ca3's position, generalised |

---

## 6. What foundation needs back from you

1. **Chain definitions** from ca3 and ranvier (§2.3 step 1, §3.3 step 1). These
   are the input to `LinkKind`'s value set, and the set freezes at step 7.
2. **`Basis` 7–10 confirmed or changed** (R15), from whoever owns the recording
   format — ca3.
3. **A decision on each row of §5**, before step 1 lands.
4. **The extrapolation limit policy** (R47): caller-stated with no default is
   the proposal, because a default would put an unmeasured number into a record
   that claims provenance (R66).
5. **The correlation default** (R4): `Undeclared`, which refuses to combine
   dispersions, is the proposal. `Independent` would produce a smaller number
   and would be wrong whenever two terms share a cause — common for two terms on
   one device.
