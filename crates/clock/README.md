# extendedresearch-clock

Clock readings that carry their domain and a worst-case bound, and the
arithmetic over them.

## What it is

A timestamp on its own cannot say whether it may be subtracted from another
one, or how wrong it might be. This crate makes both explicit.

A `Reading` carries the clock it came from, and two readings subtract only when
their domains are byte-equal — so a value from a machine that suspended, or from
a different clock source on the same machine, refuses to produce an interval
rather than producing a wrong one. Every reading carries a `Bound`: a worst-case
asymmetric window around it, whose absence is `UNBOUNDED` rather than zero.

On top of that: fitting a source clock onto a host clock from one-way
observations, mapping between the two in integers, a calendar anchor, a drift
check that ends a domain when the clock stepped, and the quantum of a timer.

No dependencies. Checked for `wasm32-unknown-unknown` on every change.

## Install

```toml
[dependencies]
extendedresearch-clock = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it.

## Use

```rust
use extendedresearch_clock::{bound_for_read, Domain, Reading, Rounding, SuppliedClock};

// A domain names the clock, the host, the boot and what the clock does across
// a suspend. Two readings subtract only when these are byte-equal.
let clock = SuppliedClock::new(domain);

// A read's bound comes from the clock's resolution and which way it rounds.
// A clock whose resolution you cannot establish gives Bound::UNKNOWN.
let bound = bound_for_read(Some(100_000), Rounding::FloorOnly);
let start = clock.at(now_ns(), bound);
let end = clock.at(later_ns(), bound);

// The widths add crosswise, because the interval is a difference: the later
// event happening early and the earlier one happening late both shrink it.
let elapsed = end.since(start)?;
println!("{} ns, within -{}/+{}", elapsed.ns, elapsed.below_ns, elapsed.above_ns);
```

Fitting a device's clock onto the host's, from `(source, receipt)` pairs:

```rust
use extendedresearch_clock::{fit_one_way, Observation};

let fit = fit_one_way(&observations)?;
let host_ns = fit.line().map_to_reference(source_ns);
```

## Guarantees

- **Integer arithmetic throughout**, with one documented exception: the two
  root-mean-square accumulations in a fit, in IEEE-754 binary64, in observation
  order, truncated to whole nanoseconds. Another implementation reproduces them
  by the same operations in the same order.
- **Readings from different domains do not subtract.** Not "should not" — the
  operation returns an error.
- **A bound is a worst case, never a standard deviation.** Widths of a
  difference add linearly, which holds only for worst cases.
- **Absence of knowledge is `UNBOUNDED`, not zero.** A zero bound is a positive
  claim that a term contributes nothing.
- **`Basis` integers are a wire contract.** A basis is written to a record as
  its integer, and a value is never renumbered or reused.
- **22 language-free JSON conformance vectors** in `vectors/`, run by
  `tests/vectors.rs`. A second implementation is correct when it reproduces
  them.

## Limits

- **A one-way fit cannot separate a constant transport delay from a clock
  offset.** A source five milliseconds away with a perfect clock produces the
  same observations as one beside the host whose clock is five milliseconds
  behind. Every `Fit` therefore reports `bias_low_ns = UNBOUNDED` and
  `bias_high_ns = 0`. That is the measured answer for the method, not a
  placeholder: bounding it needs a round trip, or a marker the source records in
  its own timeline.
- **The mapping returns a bare integer.** `Line::map_to_reference` discards the
  fit's bias, residual spread and skew uncertainty. Propagating them is the job
  of a layer above this crate.
- **No platform calls.** This crate does not read a clock; you supply the
  reading and state its domain and resolution. What your platform's monotonic
  clock does across a suspend is something you have to know and declare.
- **No cross-host synchronisation.** Fitting is one source onto one host.

## Versioning

Pre-1.0. A minor release may change any name, code or signature. Error codes,
`Basis` integers and the conformance vectors are the parts treated as a
contract; everything else is still moving.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
