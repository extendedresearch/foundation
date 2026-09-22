# 0003 — Packages are named for what they do, and contracts are renamed separately

- **Status:** Proposed, 2026-09-22.
- **Implementation status:** **Not implemented.** No package has been renamed.

---

## Context

`ranvier` and `ca3` are coined names — a node of Ranvier, a hippocampal
region. They are evocative inside neuroscience and say nothing about function:
one streams typed data between processes, the other is a recording container
format. Decision 0001 makes these packages building blocks for anyone building a
scientific measurement system, and a stranger searching for a recording format
does not search for a hippocampal region.

**Coined names are not the problem in general.** Kafka, Redis, Tokio and Pandas
are coined and do fine. The argument here is narrower and rests on two things
specific to this ecosystem:

1. **The namespace already carries the identity.** Every package is
   `extendedresearch-*`. Inside a namespace that already says who made it, the
   second half is free to say what it does, and spending it on a second identity
   is spending it twice.
2. **Discovery in this niche is by function.** Somebody looking for this works
   from "record synchronised instrument data with timestamps", not from a name
   they have never heard.

**The timing will not get better.** Nothing is published to crates.io, PyPI or
nuget.org; the one npm package is restricted and is not one of these. The
repositories are about to move, so paths churn regardless, and 169 decision
records are being audited, so their text is being touched anyway. After v1 and
publication, a crates.io name cannot be renamed at all — you publish a new crate
and deprecate the old one, and every external link rots.

---

## Decision

**Rename the packages for what they do, during the move. Do not rename the
contracts at the same time, and in some cases do not rename them at all.**

The second half is the part that makes this tractable, because the cost is not
evenly distributed. Three categories, and they are not the same decision:

### A. Names, which are cheap now

Crate names, directory names, module paths, repository names, documentation.
These are a mechanical rename plus the churn of every path that mentions them.
Expensive in diff size, cheap in risk, and entirely reversible before
publication.

### B. Contracts, which are expensive and independent

Each of these is a published surface that something outside the package
depends on. Renaming them is a **breaking change to a consumer**, not a rename:

- **C ABI symbol prefixes** — `ranvier_*`, `ca3_*`. Every `extern "C"` export.
  A C or .NET caller links against these by name.
- **Error tokens** — `RANVIER_ERR_*`, `CA3_ERR_*`. `docs/conventions/error-tokens.md`
  makes these a wire contract, and every binding in three languages reports the
  header's spelling.
- **The C library artefact name** — `<package>_abi` by
  `docs/conventions/c-library-naming.md`, so the artefact renames with the
  package or the convention breaks.
- **Environment variables, CLI names, configuration keys.**

### C. Format identity, which should probably not be renamed

**A file format is not a package.** `.ca3` names a format that files on disk
already carry; the package that reads it is a different thing and can be renamed
without touching it. Parquet files are read by Arrow; nobody renamed the files.

**Decided: `.ca3` stays.** The container format keeps its name, its extension
and any magic bytes; the package that implements it is renamed for what it does.
Files already written stay readable and nothing about the format moves.

**If a rename would invalidate a file that already exists, that is the signal it
is a format decision and not a naming one**, and it needs its own record.

---

## What follows

**A is done during the move.** The directories are moving anyway, so the rename
rides along and costs one extra pass over paths rather than a second migration.

**B is decided per contract, and deliberately, after A.** A symbol prefix or an
error token may keep the old spelling indefinitely — a name a consumer links
against is not required to match the package that produces it, only to be
stable and documented. Changing them is a separate change with its own
consumers to notify.

**C needs its own record before anything touches it**, and the question it
answers is "does a file written yesterday still open", not "is this name
searchable".

---

## Naming criteria, not names

The names themselves are the owner's call. The criteria this record asserts:

- **The second half names the function**, since the namespace names the maker.
- **It survives translation.** `style/STYLE.md` S1 requires one canonical
  identifier per concept, transformed mechanically per language. A name that
  reads badly as a C symbol prefix or a .NET namespace is the wrong name.
- **It does not collide inside the ecosystem.** Which is already a live problem:
  `ranvier` has a local function named `status` that maps to foundation's
  `check`, while foundation's own `status` converts in the opposite direction.
  After a merge, one file reading `status(...)` means two different things
  depending on which package it is in. That is the S1 failure the style guide
  exists to catch, and the rename is the moment to catch it.
- **It is not a second identity.** If the name needs a paragraph of etymology to
  explain, it is category A spent on category B's job.

---

## Consequences

**Every citation of a package name changes**, including in the 169 decision
records being audited. Since the audit is already reading all of them, the
rename list should be an output of that audit rather than a separate pass.

**The rename must land in one commit per package**, not spread across a
migration, so that `git log --follow` and a revert both work.

**Decision 0001's names are affected too.** `extendedresearch-abi` was kept in
the restructuring plan on the grounds that renaming a 595-call-site surface for
a presentational gain was not worth it. That reasoning holds less well if
everything else is renaming in the same window — worth reopening as part of A,
with the same "cheap now, impossible later" argument that justifies this record.
