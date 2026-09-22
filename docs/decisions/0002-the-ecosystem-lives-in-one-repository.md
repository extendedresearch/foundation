# 0002 — The ecosystem lives in one repository, once the boundaries are enforced without it

- **Status:** Proposed, 2026-09-22.
- **Implementation status:** **Not implemented.** No repository has moved.
  `docs/specs/monorepo-migration-plan.md` is the plan this record authorises,
  and §3 below is a gate rather than a checklist — the move does not start until
  every item passes.

---

## Context

Five repositories hold one ecosystem: `foundation` and the packages in it,
`ranvier`, `ca3`, `eres`, and `plugins`. They are developed together, released
against each other, and broken by each other.

**What separation costs today**, each checked rather than recalled:

- **Two versions of foundation are live across three consumers.** `ranvier` and
  `ca3` pin `55b2fce`; `eres` pins `a56dfe6`. `CONTEXT.md` already names this as
  the hazard it is: two packages pinning different revs put two copies of every
  crate in one build, and Cargo treats them as unrelated types.
- **No change is tested against the ecosystem before it merges.** A foundation
  change is validated when each consumer gets around to bumping a rev, which is
  after the fact and one repository at a time.
- **Cross-cutting changes cannot be atomic.** The `extendedresearch-status`
  split ships re-exports for no reason other than that four repositories cannot
  move in one commit. The re-exports are scaffolding for a constraint the
  repository layout imposes.
- **The duplication check covers one repository out of five.** A consumer
  vendoring a copy of a foundation file is invisible from here, which is exactly
  the drift the check exists to prevent.
- **Machinery exists only to cross the seam.** `ranvier`'s
  `fetch-foundation-nuget.sh` downloads a `.nupkg` from a release and verifies it
  against that release's `SHA256SUMS`. Every line of it is the cost of the
  boundary, not of the problem.

**What separation buys today**, and this is the part that decides the
sequencing: **repository boundaries are silently enforcing package boundaries.**
`ranvier` cannot accidentally depend on `ca3` because it cannot see it. That
enforcement is free, invisible, and would disappear on the day of the move.

---

## Decision

**Move `ranvier`, `ca3`, `eres` and `plugins` into this repository, after — and
only after — the enforcement that repository separation currently provides for
free exists as a check.**

This does not contradict decision 0001. That record is about **package**
independence, not repository count: the boundary is the package, not the
directory it lives in. A repository holding many independently publishable
packages is ordinary, and the packages still publish separately at v1. What a
monorepo removes is the accidental enforcement, which is why §3 is a gate.

### Decision records are per package, with a repository-level tier above them

Numbering makes this urgent rather than cosmetic. Across the four repositories
there are **169 decision records, every set starting at 0001**:

```bash
for r in ranvier ca3 eres plugins; do ls $r/docs/decisions/*.md | wc -l; done
# 54, 12, 98, 5 — highest numbers 0053, 0012, 0097, 0005
```

Renumbering into one sequence would break every citation in every one of them,
and citations reach into code: `ranvier`'s `Cargo.toml` cites "decision 0049" in
a comment explaining its dependency policy.

So there are two tiers:

- **Package-level** — `<package>/docs/decisions/NNNN-*.md`. Numbering is local
  to the package. `ranvier/docs/decisions/0049-*.md` keeps its number and every
  existing citation keeps resolving.
- **Repository-level** — `docs/decisions/NNNN-*.md` at the root, for decisions
  about how packages relate. This record and 0001 are the first two.

**The rule for which tier a decision belongs to:** if it would still make sense
to somebody who took only that package and nothing else, it is package-level.
If it is about how packages relate to each other, it is repository-level.

This also follows from 0001 without the merge: a package that must carry a
README standing on its own should carry the design history explaining why it is
shaped that way, and that history should travel with the package if it is ever
extracted.

---

## The gate

The move does not begin until all five hold. Each is a check, not an intention.

1. **Cross-package source enforcement exists.** `ecosystem/check.py` reads
   manifests today. It must also refuse a source-level import that no manifest
   declares, per language. This is the wall that repository separation is
   providing now, and nothing else replaces it.
2. **Every repository conforms to the package standard** — an entry in
   `PACKAGES.toml`, a README with the standard headings, the shared style
   configuration, and its decision records under its own directory.
3. **Affected-package detection exists in CI.** A monorepo that runs everything
   on every change gets slow, and a slow suite is one people learn to skip. This
   is a prerequisite, not a later optimisation.
4. **The workspace question is settled** (migration plan §2). Four root
   workspaces exist today and the answer is not obvious.
5. **The repository is renamed.** "foundation" names a set of building blocks,
   not an ecosystem containing four products. A redirect is cheap now and every
   stale link is expensive after publication.

---

## Consequences

**Deleted, not moved:** `fetch-foundation-nuget.sh` and its hash verification,
git-rev pinning in four manifests, the release-asset round trip for internal
consumption, and the re-export scaffolding that exists only so repositories can
migrate at different speeds.

**Gained:** every change tested against the whole ecosystem before it merges;
atomic cross-cutting changes; one resolution of every shared dependency, so the
pyo3 and napi lockstep that `CONTEXT.md` describes as a convention becomes
structural; and a duplication check that covers five repositories instead of
one.

**Harder:** visibility becomes all-or-nothing. Foundation could be open-sourced
on its own today; after the move that is one decision for everything, at v1.
CI blast radius grows, which item 3 of the gate exists to contain.

**Unchanged:** decision 0001 still governs. Packages remain independently
useful, independently documented, and independently published at v1.

---

## Alternatives considered

**Stay separate and automate the version skew** — a bot that bumps each
consumer's rev when foundation moves. Rejected: it fixes the symptom that is
easiest to see and not the one that matters. The goal is testing a change
against the ecosystem *before* it merges, and no amount of downstream bumping
provides that.

**Move the products and leave `plugins` out**, since nothing depends on it.
Rejected as a permanent shape, kept as the migration order: `plugins` and `eres`
are leaves and make the safest pilots.

**One flat decision-record sequence, renumbering on merge.** Rejected — 169
records and their citations, including citations from code comments.
