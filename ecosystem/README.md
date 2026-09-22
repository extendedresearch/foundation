# extendedresearch-ecosystem

The checks that hold a set of packages modular: a declared dependency graph, a
refusal of undeclared copies, one standard document set, and decision citations
that resolve.

## What it is

Foundation decision record 0001 says this repository is a set of independently
useful packages. That is a claim about the dependency graph, and a claim nothing
checks decays — an unwanted edge arrives as one manifest line and is invisible
from then on.

`PACKAGES.toml` declares the shape: every package, every dependency it is
allowed inside the repository, every third-party dependency, every file that is
deliberately a copy, and the documents each package carries. `check.py` compares
that declaration against what the manifests actually resolve and what the tree
actually contains.

Four checks, plus a fifth that does not pass yet and says so:

| Check | Refuses |
|---|---|
| `boundaries` | A dependency that exists and is not declared, a declaration with no dependency behind it, an undeclared third-party dependency, a cycle, a declared namespace no `.cs` file uses, and a declared seam that reaches nowhere outside its own package |
| `duplication` | Two byte-identical tracked files that no `[[copy]]` declares, and any tracked file under `vendor/`, `third_party/`, `node_modules/` or `target/` |
| `documents` | A package missing `README.md`, `LICENSE` or `NOTICE`, and (softly) a README whose headings are not the standard set in order |
| `citations` | A reference to a decision record that does not name the repository whose record it is, and a reference naming this repository that resolves to no file |
| `comparator.py` | An import the source makes and the declaration does not allow — and, today, a language with no extractor behind it |

`citations` exists because a bare `decision NNNN` cannot be resolved by anyone
who was not there when it was written. The number alone does not say which of
five record sets to look in, and the September 2026 audit found a repository
where the bare numbers resolve to a *sibling's* records while that repository
has no such number — a reader following one gets a confident answer to the
wrong question. Re-derive the count in any checkout with
`git grep -nE '[Dd]ecisions? [0-9]{4}'`.

`ecosystem/comparator.py` compares what the source actually imports against
`PACKAGES.toml`, per `docs/specs/boundary-enforcement-plan.md` §2: one extractor
per language, each emitting `(package, resolved_target, kind)`, and one
comparator holding the rule. **The comparator exists and no extractor does**, so
every run fails with the language named. It is deliberately not part of `all`:
wiring a check that cannot pass into CI teaches everyone to ignore CI.

`affected.py` reads the same declaration to answer a different question: given a
diff, which packages must be tested. It is gate item 3 of foundation decision 0002 — four
repositories merge into this one, and a monorepo that runs every test on every
change gets slow enough that people learn to skip the suite. It does not
re-derive the graph; `check.py boundaries` is what holds the declaration honest,
so a second derivation would be a second thing to be wrong.

Nothing is gated on its answer. The `affected` CI job computes and publishes the
selected set while every other job runs unconditionally, so its answers can be
compared against what actually runs before anything is switched off. The comment
above that job states what would have to hold first.

## Install

Nothing to install. Python 3.11 or later, standard library only; the
`boundaries` check shells out to `cargo metadata`.

## Use

```bash
python -m unittest discover -s ecosystem/tests   # the checks' own tests
python ecosystem/check.py all
python ecosystem/check.py boundaries
python ecosystem/check.py citations
python ecosystem/check.py all --strict            # soft findings fail too
python ecosystem/check.py all --root ../elsewhere # any repository, against its own declaration
python ecosystem/comparator.py                    # fails: no extractor exists
```

CI runs the tests and then `all`, in the `ecosystem` job, on every change. It
does not run the comparator.

`ecosystem/tests/` covers what a passing run cannot show: a table skipped by a
short-circuited `and`, a workspace that was never read, two packages collapsed
into one by a shared name, and an index file counted as a malformed record.
Each of those produces output that reads exactly like a clean tree.

`citations` needs to know which repository it is looking at, so that it can tell
a citation it should resolve from one it should leave to the repository that
owns it. That name comes from the `repository` key in `PACKAGES.toml`, and
`--repo` overrides it. A name the check does not recognise fails the run rather
than filing every citation as a sibling's — which would pass an entire tree
without resolving anything in it.

```bash
python ecosystem/affected.py                       # names, one per line
python ecosystem/affected.py --base origin/main    # the ref to diff against
python ecosystem/affected.py --json                # for a workflow to consume
python ecosystem/affected.py --explain             # every package and why

python -m unittest discover -s ecosystem/tests     # the detection's own suite
```

CI runs `check.py all`, the tier check and the affected-detection suite in the
`ecosystem` job on every change, and computes the affected set in the `affected`
job.

Adding a dependency between two packages means editing `PACKAGES.toml` in the
same change. That is the point: the edge becomes something a reviewer sees.

## Guarantees

- **Both directions.** An undeclared edge fails, and so does a declared edge
  with nothing behind it — so the file cannot drift into describing a graph that
  no longer exists.
- **Read from the manifests, not from prose.** The Rust graph comes from
  `cargo metadata --no-deps`, which is what cargo resolves rather than what a
  regex reads. Development and build dependencies are excluded, because they do
  not reach a consumer.
- **Every row is printed, passing rows included.** A report of only failures
  cannot distinguish "checked and clean" from "never checked".
- **Every table is printed, including the ones after a failure.** `and`
  short-circuits, so combining verdicts with it stops the output at the first
  failure and the later tables are absent rather than empty. Each report runs
  before the verdicts are combined, and `ecosystem/tests/` asserts it — a
  passing run cannot show you a table that would have been skipped.
- **Every cargo workspace in the tree is read.** One `cargo metadata` at the
  root sees one workspace. A repository with nine had eight of them invisible,
  and the obvious way to make that run green is to delete the true rows.
- **A package is keyed by name *and* kind.** A Rust crate and a Python
  distribution can carry the same name and both be right; keyed on the name
  alone, one replaces the other and an edge is checked against the wrong row.
- **Line endings are not drift.** Files are hashed with `\r\n` normalised.
- **A citation in a binary file is reported, not parsed.** A citation reaches a
  compiled protobuf descriptor as a source comment carried through codegen.
  Deciding whether such a match is qualified would mean guessing at an encoding
  and at where the surrounding bytes begin. The row says a citation is in there
  and stops.
- **A language with no extractor fails the comparator.** A comparator reporting
  nothing over a language it never looked at is indistinguishable from one that
  looked and found nothing, and the second is the claim a reader takes away.

## Limits

- **It cannot detect the same rule written twice in two languages.** Hashing
  finds copied bytes; it does not find the enumeration short-name rule
  implemented once in Rust and once in TypeScript, which is a real case in this
  repository and which the two implementations disagree on. `[[shared_rule]]`
  is the declaration for that class: it requires one set of language-free
  vectors and a test per implementation consuming them. Registering a rule is
  cheap; writing the vectors is the work, and neither has been done yet.
- **It cannot see a type leaking across a boundary.** A package exposing
  another package's type in its own public API couples every consumer to both,
  and nothing here notices. `cargo public-api` over rustdoc JSON could; it is
  not wired up.
- **It checks this repository only.** A consuming repository vendoring a copy
  of a foundation file is invisible from here. The script is runnable from any
  repository, and no consumer runs it yet.
- **The declared graph is not compared with the plan.** `PACKAGES.toml` and
  `docs/specs/package-restructuring-plan.md` can disagree, and nothing notices.
- **No extractor exists, so nothing compares source against declaration.**
  `boundaries` reads manifests, and a manifest says what a package *may* depend
  on rather than what its source imports. Repository separation is what closes
  that gap today — a package cannot import from a sibling it cannot see — and
  the merge removes it. The comparator and the intermediate form are built; the
  four extractors are not.
- **`citations` sees citations that say "decision".** `foundation 0001`, a bare
  `0001 §4`, and a record referred to by title rather than number are all
  invisible to it. So is a citation to a *sibling's* record: the check confirms
  it names a repository and stops, because this checkout does not contain that
  repository's records to resolve against.
- **`citations` knows five repository names, and so does
  `docs/docs.tools/tier_vocabulary.py`.** Two lists, one fact. A sixth
  repository changes both, and nothing notices if only one changes. This is the
  shape `[[shared_rule]]` exists for, and registering it means writing vectors
  first.
- **The `namespaces` check runs in one direction.** A namespace declared by a
  package and used by nothing is reported. A `.cs` file declaring a namespace no
  package claims is not, because no package but `ExtendedResearch.Interop`
  declares the field yet and every test project's namespace would be a finding
  on the day it was switched on.
- **A `process` seam is not looked for in the source.** Nothing about a child
  process is checkable from the declaration alone; the `reason` is the review
  surface, and that it is present is what the check enforces.
- **No package declares a seam, so the seam rules have nothing real to run
  against.** They were exercised against a throwaway declaration, not against
  this tree. The check prints a row per package saying "no seam declared", which
  is the honest version of a table that would otherwise be empty.
- **Both directions holds for edges, not for packages.** A package declared
  here and absent from the tree fails; a crate in the tree that this file does
  not declare passes unnoticed. Workspace discovery makes that visible for the
  first time — foundation has two cargo workspaces, and the second holds
  `prove-tests-ran-live`, a CI-action fixture that `PACKAGES.toml` does not
  declare. Closing that direction means deciding what a fixture crate outside
  the package set is, which is a declaration change rather than a checker one.
- **`comparator.py --edges` reads a file, not a tree.** It exists so the
  comparison rules can be exercised before any extractor is written. A handmade
  file is a claim about the source, not evidence from it, and the run prints
  that the edges came from a file.

- **Affected detection sees manifest edges and nothing else.** Two edges in
  this repository exist only in a test harness and so are invisible to it:
  `crates/napi-testaddon`'s Node test loads `npm/binding-runtime/src/`, and
  `dotnet/Interop.Tests` runs against a library built from `crates/abi-testlib`.
  Behavioural coupling with no edge at all — two packages that must agree on a
  wire format or a status code's meaning — is invisible for the same reason.
  `affected.py`'s module docstring carries the full list.
- **A package's sources can sit outside its declared `path`.**
  `ExtendedResearch.Interop` is declared at `dotnet/Interop.Package` and ships
  `dotnet/Interop/*.cs`. Affected detection reads that as a file owned by no
  package and widens to everything, which is safe and means the narrow answer
  for that package has never been exercised.

## Versioning

Pre-1.0, and versioned with the rest of the repository. The `schema` key in
`PACKAGES.toml` is `2`; a change to the declaration format increments it.
Schema 2 adds `repository` at the top level and `namespaces` and `seams` on a
package. All three are additive: a file written for schema 1 still loads.

## Licence

Apache-2.0. See the repository root's `LICENSE` and `NOTICE`.
