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

## Install

Nothing to install. Python 3.11 or later, standard library only; the
`boundaries` check shells out to `cargo metadata`.

## Use

```bash
python ecosystem/check.py all
python ecosystem/check.py boundaries
python ecosystem/check.py all --strict   # soft findings fail too
```

CI runs `all` in the `ecosystem` job on every change.

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

## Versioning

Pre-1.0, and versioned with the rest of the repository. The `schema` key in
`PACKAGES.toml` is `1`; a change to the declaration format increments it.

## Licence

Apache-2.0. See the repository root's `LICENSE` and `NOTICE`.
