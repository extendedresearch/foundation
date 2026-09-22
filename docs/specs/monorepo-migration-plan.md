# Monorepo migration plan

Status: **proposed**, 2026-09-22. Authorised by
`docs/decisions/0002-the-ecosystem-lives-in-one-repository.md`, whose §3 is a
gate this plan does not start before. Nothing has moved.

---

## 1. Layout

Minimum churn: foundation's existing paths do not move, and the four products
arrive as top-level directories beside them.

```
/
├── crates/          status, abi, pyo3, napi, clock, metrology, fixtures
├── npm/             binding-runtime
├── dotnet/          Interop and its projects
├── python/          conformance
├── style/           the style guide, configs, checks
├── ecosystem/       PACKAGES.toml and check.py
├── docs/            repository-level decisions, specs, conventions
├── ranvier/         ─┐
├── ca3/              ├─ each keeps its own internal layout and its own
├── eres/             │  docs/decisions/, unrenumbered
└── plugins/         ─┘
```

**Why not reorganise into `packages/` and `products/` at the same time.** Every
path in every document, every workflow and every `include_str!` would move in
the same change as four repository merges. Two large changes at once produce a
failure nobody can bisect. The reorganisation is cheap later and can be its own
change.

**What this costs:** the root gains four directories and reads less tidily than
a curated layout would. That is the trade — legibility of the root against
bisectability of the migration.

---

## 2. The workspace question — the biggest open item

Four root Cargo workspaces exist today, one per repository. Three shapes are
possible and they differ in what they buy.

| Shape | Ecosystem-wide `cargo test` | One resolution of shared deps | Cost |
|---|---|---|---|
| **One workspace** | yes | yes — one `Cargo.lock` | every crate must agree on every shared dependency version, at once |
| **Several workspaces** | no — one command per workspace | no | keeps today's independence, and most of the benefit with it |
| **One workspace, `exclude` for stragglers** | for what is in it | for what is in it | a migration path rather than an end state |

**Recommendation: one workspace, reached through the third shape.**

The whole point of the move is testing a change against the ecosystem before it
merges, and separate workspaces do not provide that. One lockfile also makes
structural something `CONTEXT.md` currently states as a convention: *"pyo3 and
napi move in lockstep with every consumer"* — `pyo3-ffi` declares
`links = "python"`, so two series cannot resolve, while `napi-sys` declares no
`links` and a second copy builds silently. A single lockfile removes the silent
case.

**The cost, stated plainly:** every crate must agree on every shared dependency
at the moment of the merge. That is discoverable before committing to it:

```bash
# For each shared dependency, what does each repository resolve today?
for r in foundation ranvier ca3 eres plugins; do
  echo "== $r"; grep -rhn '^name = "\(pyo3\|napi\|serde\|tokio\)"' -A1 $r/Cargo.lock
done
```

Run that first. If the answer is "they already agree", one workspace is free and
the decision is made. If it is not, the disagreements are the real migration
work and they are worth knowing before any history is rewritten.

---

## 3. Order

Each phase leaves every repository working, and no phase depends on a later one.

### Phase 0 — the gate (decision 0002 §3)

Nothing moves until all five pass. Phases 1 and 2 are how items 1 and 2 get
done, so they run inside the gate rather than after it.

### Phase 1 — audit the decision records, then move them in place

Moving 169 records assumes they are still true, and they are not. The
restructuring invalidates some outright, several are ecosystem decisions that
happen to live in one repository, and four independent sets written without
reading each other will contradict each other on questions all four answered.

**The governing principle: supersede, never delete or edit.** A decision record
is a history of what was believed and why, not a configuration file. A record
the refactoring invalidated is marked superseded and names what replaced it. A
reader who finds a wrong decision should be able to see that it was wrong, when
that was noticed, and what replaced it — deleting it destroys exactly the
information that makes the set worth keeping.

Every record is classified into one of five, and **every record gets a row,
including the ones that need nothing** — a report of only the problems cannot
distinguish "checked and still true" from "never read":

| Class | What happens |
|---|---|
| **Live** | Still true. Moves to `<package>/docs/decisions/` unchanged, keeping its number |
| **Ecosystem** | Actually about how packages relate rather than about one package. Promoted to the repository-level set; the original is superseded by a pointer so its number keeps resolving |
| **Superseded** | The refactoring made it false. Marked superseded in place, naming its replacement. Not deleted |
| **Contradicted** | Conflicts with another repository's record on the same question. Needs adjudication before either moves — this is the class that only exists because nothing reads all four sets today |
| **Orphaned** | Still true, but about a package that left. Moves *to that package*, not to the repository tier and not to the bin |
| **Status drifted** | Its *Implementation status* is wrong |

Status drift is worth its own pass. A record claiming `Implemented` for
something that does not exist is worse than no record, because the next reader
plans around it.

### What the audit actually found

All 169 records were classified. The counts matter less than five things it
turned up, each verified by a command rather than read off a status line.

**Forty-four records are orphaned, not superseded.** An editor application left
`eres` for a repository of its own, taking 614 files with it, and the records
describing it stayed behind. They are not wrong; their subject moved. That
repository has 21 records of its own and re-decides some of the same ground, so
`eres` 0050–0091 and its 0001–0020 are about the same code under different
numbers. This is what the **Orphaned** class is for.

**The scope in §1 is incomplete.** Four more repositories sit under `apps/`
— one with 21 decision records, one with 1 — plus other Rust trees beside
`core/`. The migration names four repositories and the ecosystem has more. The
layout and the order both have to account for them before Phase 3.

**Decision citations have leaked into shipped artifacts.** One repository cites
**34 distinct decision numbers** of which 29 belong to other repositories, and
**269 of its 336 citation lines name no repository at all** — a bare "decision
0011" that resolves to a sibling's record while the citing repository has no
0011. Another carries a decision *filename* as a data field in a generated JSON
file, and a decision number is compiled into a protobuf descriptor that ships
inside recordings. **No mechanical renumber or rename can be trusted against
this**, which retires the last argument for renumbering and raises a new
requirement: a citation must name its repository.

**Status drift is the rule, not the exception, and it runs both ways.** One
record reads "Not implemented" while the schema is entirely its shape. Another's
own re-derivation command reports 7 of 15 where the tree now gives 35 of 69 —
and the test fails. Three records claim an ABI exports 126 functions over 12
handles where it exports 359 over 31. Two name a crate that exists on no branch.

**The contradictions are real and one of them is this repository's.** See §4a.

Two things the audit could not do, stated because a green-looking report should
not be read as coverage: it did not run any test suite, so every test count
quoted in a record is unverified; and it flagged structural claims rather than
snapshot counts, so roughly a dozen more records carry stale figures that were
deliberately not classified as drift.

Done before the merge, in four separate changes within the existing
repositories, so a collision cannot happen during the merge itself. **No
renumbering, ever** — 169 records and their citations, some in code comments.
Promotion to the ecosystem tier gives a record a new number in the root set and
leaves the original number in place as a superseded pointer, which is what keeps
a code comment citing "decision 0049" resolving.

### Phase 2 — conformance, in place

In each repository, independently, in any order:

- An entry in that repository's own copy of the package declaration.
- A README per package with the standard headings. `crates/clock/README.md` is
  the exemplar.
- The shared style configuration, adopted in soft mode.
- `ecosystem/check.py` running green there, including the source-level import
  check from gate item 1.

**This is where the real work is**, and none of it needs the merge. A repository
that cannot pass its own boundary check today would import its way across a
package boundary the week after the merge.

### Phase 3 — the merge, one repository at a time, history preserved

`plugins` first, then `eres`, then `ca3`, then `ranvier` — leaves before
dependents, smallest blast radius first.

```bash
git remote add plugins-origin <url>
git fetch plugins-origin
git subtree add --prefix=plugins plugins-origin main
```

`git subtree add` preserves the incoming history under the prefix, so
`git log --follow plugins/<path>` still reaches the original commits.

**After each one, before the next:** the full suite green, `ecosystem/check.py
all` green, and the newly arrived packages declared. A merge that has to be
undone is undone alone rather than alongside three others.

### Phase 4 — collapse the seams

Only once every repository is in. This is where the benefit is actually
collected, and it deletes more than it adds:

- Replace the four `git` dependencies on `extendedresearch-*` with path
  dependencies. The `rev` skew disappears by construction.
- Delete `ranvier/scripts/fetch-foundation-nuget.sh` and the `nuget.config`
  local-folder source; `dotnet/Interop` is now a path away.
- Delete the release-asset round trip for internal consumption. The assets still
  exist for external consumers at v1; nothing internal downloads them.
- Delete the re-export scaffolding in `extendedresearch-abi` that exists only so
  four repositories can migrate at different speeds.
- Unify CI into one workflow with affected-package detection.

### Phase 5 — rename the repository

`foundation` names a set of building blocks. GitHub redirects the old name, so
the cost is a redirect now against every stale link later.

---

## 4a. Contradictions found, and who owns each

Every row verified by running the command beside it, not by reading a record.

| Question | The disagreement | Owner |
|---|---|---|
| **What is the ecosystem's minimum Rust?** | `foundation` and `ranvier` declare 1.85; `ca3`, `eres` and `plugins` declare 1.88. `docs/conventions/toolchain-pins.md` states 1.85 as *the* pin, so this repository's own convention is wrong about three of five repositories. One of the 1.88s is measured — let-chains fail on 1.87 — so this is not a free choice about a number | **foundation** |
| **Is this repository one repository?** | One consumer's record says "This repository is one repository. Not a stage in a split", against decision 0002 | that consumer, after 0002 |
| **When does a package reach a public registry?** | One consumer's record requires registry publication to work; decision 0001 defers registry publishing to v1 | foundation 0001 governs |
| **May an export carry a figure no sample carried?** | Three documents in one consumer answer it three ways — never, never interpolate, and yes-if-labelled — and the permissive one is what ships | that consumer |
| **What does a time basis label mean?** | One consumer ships two unrelated vocabularies for one column family: `exact/ok/degraded/unmapped` and `exact/estimated/unchecked`. Both implemented. A reader sees two meanings of `exact` | that consumer |
| **How is the contribution boundary enforced?** | One repository removed its pre-commit and pre-push hooks and kept a CI job; nothing propagated that, and there are four or five independent copies of the checker with no shared implementation | ecosystem-level; unowned today |

**Two findings that are defects rather than disagreements**, both in this
repository's subject area:

- A consumer's calibration records are specified and unimplemented, which makes
  the rule forbidding double-correction **unimplementable**, and that rule is the
  one its own specification calls the failure mode the format exists to refuse.
  It is foundation R27 and R28, which that consumer will inherit.
- **A third clock-fit implementation exists** and no decision record covers it.
  Its own source says three implementations exist and a disagreement between
  them is a disagreement about when a recorded event happened. Foundation R42
  targets exactly the discarded-uncertainty defect in the one it names; whatever
  supersedes it supersedes all three.

---

## 4. What could go wrong, and what catches it

| Risk | What catches it |
|---|---|
| A product imports across a package boundary once the wall is gone | Gate item 1 — the source-level import check. **Nothing else does.** This is the single most important prerequisite |
| Shared dependency versions disagree | §2's command, run before committing to one workspace |
| CI becomes slow enough to skip | Gate item 3 — affected-package detection, before the move |
| A merge goes wrong | One repository at a time, full suite between each; `git subtree add` is a merge commit and is revertible |
| History is lost | `git subtree add` rather than a copy; verified with `git log --follow` on a file from each repository afterwards |
| Decision numbers collide | Phase 1, done in each repository before the merge |
| Visibility becomes all-or-nothing | Decided deliberately at v1, per decision 0002 |

---

## 5. Open

**One workspace or several** (§2) — run the resolution comparison first.

**Whether `plugins` belongs at all.** It builds against `ranvier` and `ca3` and
nothing depends on it. It is the safest pilot for exactly that reason, and it is
also the one whose absence would cost least. Worth asking whether it is a
product of this ecosystem or a consumer of it.

**What the repository is called** after Phase 5.

**Whether the `packages/` and `products/` reorganisation happens** after Phase
4, once the migration is no longer the thing that might need bisecting.
