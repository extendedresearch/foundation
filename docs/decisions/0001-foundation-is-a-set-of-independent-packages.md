# 0001 — Foundation is a set of independent packages, not one shared library

- **Status:** Proposed, 2026-09-22.
- **Implementation status:** **Not implemented.** Nothing in the tree has moved.
  `docs/specs/package-restructuring-plan.md` is the plan this record authorises.

---

## Context

Foundation was built as the shared half of three products. Its admission rule
says so:

> A crate lands here when two packages need the same runtime code. A crate with
> one consumer belongs in that consumer's repository.

That rule serves `ranvier`, `ca3` and `eres`. The intent now is different: these
should be building blocks that stand on their own merits, usable by anyone
building a scientific measurement system — and independent of one another in the
same way the three products are independent of one another.

Three things in the tree prevent that today. Each was checked rather than
recalled.

**Everything versions in lockstep.** `python scripts/release-checks.py tree`
fails unless all 15 version strings are identical — every crate, the wheel, the
npm package, the `.nupkg`, and every internal path dependency's requirement. One
tag produces one release carrying four assets. Someone who wants only the clock
gets a version that moved because the .NET interop changed.

**The admission rule is the opposite of the goal.** It admits code because two
of our products need it. The goal admits code because it is foundational to
scientific measurement, whether or not any of our products use it yet.

**Nothing is packaged for a stranger.** The crates are not on crates.io, the
wheel is not on PyPI, the `.nupkg` is not on nuget.org, and the npm package is
restricted. Every install instruction names a git rev in a private repository.

---

## Decision

**Foundation is a set of independently useful packages.** Each one is a building
block someone can adopt without adopting the others, and without adopting
ExtendedResearch.

Four parts to that, and one deliberate deferral.

### 1. The admission criterion changes

A package belongs in foundation when it is **foundational to scientific
measurement software and coherent on its own** — when its documentation makes
sense to a reader who has never heard of `ranvier`.

The old two-consumer rule is withdrawn. It was a proxy for "is this general",
and it is a bad one in both directions: it admits accidental overlap between two
of our products, and it refuses something genuinely general that only one of
them happens to need yet. `extendedresearch-clock` would have been refused under
it when it landed, and `eres` not depending on it today is now irrelevant rather
than disqualifying.

What the old rule got right survives as a separate test: **a package whose only
plausible consumer is one of our products belongs in that product.** The
question is generality, not consumer count.

### 2. Boundaries are drawn by dependency direction, not by subject

A package boundary exists where it changes what a consumer must depend on.
Splitting a crate that every consumer takes whole adds a manifest line and buys
nothing.

The test that matters: **does this boundary let a consumer avoid depending on
something it does not want?** Two applications of it:

- A **safe core** implementing an error trait should not depend on the crate
  whose purpose is dereferencing raw pointers. Today it does, because
  `AbiError` and `mod borrow` are in one crate.
- A program that wants **timing accounting** should not need this project's
  clock, and a program that wants the **clock** should not need a budget model
  or an allocator.

### 3. Coupled things stay coupled, and say so

Not everything separates. `abi`, `pyo3`, `napi`, `binding-runtime` and `Interop`
share one error-code numbering and one token grammar; changing a boundary code
moves all five. They are one product with five language faces. Versioning them
independently would be a fiction, and the plan keeps them together and states
the coupling rather than hiding it behind five version numbers that always
change at once.

### 4. Names and boundaries are settled before publication, not after

A package boundary and a package name are cheap to change now and expensive
after anyone depends on them. Both are decided as part of this restructuring,
while the only consumers are repositories we control and they pin by commit.

### The deferral: versioning and release machinery wait for v1

**Lockstep versioning, the single tag and the four-asset release stay for now.**
They are what makes pre-v1 development cheap: one number to bump, one check that
everything agrees, one release to cut. Per-package versions, per-package tags
(`clock-v0.2.0`) and per-package release assets are what publication needs, and
publication is a v1 concern.

So `release-checks.py`'s version equality check survives unchanged, and the work
in it is not wasted.

**What keeps the deferral from rotting.** A deferred boundary is only real if
something holds it in the meantime:

- Every package's `Cargo.toml` declares its dependencies explicitly. A package
  that does not depend on another cannot use it, lockstep versioning or not.
- Every package carries a README that stands alone, written for a reader who has
  never heard of this project. A README that cannot be written without
  explaining `ranvier` is a package boundary in the wrong place.
- The dependency graph is acyclic and shallow, and the plan states it. A new
  edge between packages is a reviewed change.

Those three are enforceable today and cost nothing. Independent version numbers
add nothing to them pre-v1.

---

## Consequences

**Now:** packages are split and named as
`docs/specs/package-restructuring-plan.md` describes; the admission criterion in
`CONTEXT.md` is rewritten; each package gets a standalone README.

**Not now:** per-package versions, per-package tags, per-package releases,
registry publishing, and any rename driven by public presentation rather than by
what the package does.

**At v1:** the version-equality check becomes a per-package check, the tag
scheme gains a package prefix, `release.yml` builds per-package assets, and the
packages go to crates.io, PyPI, nuget.org and npm. The trigger is the decision
to publish, not a date.

**For the consuming repositories:** almost nothing. They pin by git rev, and a
rev is version-agnostic, so a split that keeps module paths stable costs them an
import line rather than a version bump. The plan quantifies this per package.

---

## What this record does not decide

Whether the `extendedresearch-` prefix survives publication. It is a namespace,
which is ordinary practice, and it is not what makes a name unhelpful — the
second half is. That is settled per package in the plan.

Whether foundation's packages are `no_std`. It matters for instrument firmware
and it is decided per package, in the plan, not globally here.
