# extendedresearch-ecosystem

The checks that hold a set of packages modular: a declared dependency graph, a
refusal of undeclared copies, and one standard document set.

## What it is

Decision record 0001 says foundation is a set of independently useful packages.
That is a claim about the dependency graph, and a claim nothing checks decays —
an unwanted edge arrives as one manifest line and is invisible from then on.

`PACKAGES.toml` declares the shape: every package, every dependency it is
allowed inside the repository, every third-party dependency, every file that is
deliberately a copy, and the documents each package carries. `check.py` compares
that declaration against what the manifests actually resolve and what the tree
actually contains.

Three checks:

| Check | Refuses |
|---|---|
| `boundaries` | A dependency that exists and is not declared, a declaration with no dependency behind it, an undeclared third-party dependency, and a cycle |
| `duplication` | Two byte-identical tracked files that no `[[copy]]` declares, and any tracked file under `vendor/`, `third_party/`, `node_modules/` or `target/` |
| `documents` | A package missing `README.md`, `LICENSE` or `NOTICE`, and (softly) a README whose headings are not the standard set in order |

`affected.py` reads the same declaration to answer a different question: given a
diff, which packages must be tested. It is gate item 3 of decision 0002 — four
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
python ecosystem/check.py all
python ecosystem/check.py boundaries
python ecosystem/check.py all --strict   # soft findings fail too
```

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
- **Line endings are not drift.** Files are hashed with `\r\n` normalised.

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
`PACKAGES.toml` is `1`; a change to the declaration format increments it.

## Licence

Apache-2.0. See the repository root's `LICENSE` and `NOTICE`.
