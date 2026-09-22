# Source-level boundary enforcement

Status: **proposed**, 2026-09-22. This is gate item 1 of
`docs/decisions/0002-the-ecosystem-lives-in-one-repository.md`, and nothing
moves until it exists. Not implemented.

The per-language analysis here was proposed by the `ranvier` session and is
adopted with the reasoning kept, because the reasoning is what makes the limits
in §4 honest.

---

## 1. What this replaces

`ecosystem/check.py boundaries` reads manifests. A manifest says what a package
*may* depend on; it does not say what the source actually imports. The gap
between those two is invisible today for a reason that will not last:
**repository separation is enforcing package boundaries for free.** `ranvier`
cannot import from `ca3` because it cannot see it.

On the day the repositories merge, that wall is gone and the only thing between
a package and an import across a boundary is a check. This is that check.

---

## 2. Architecture: four extractors, one comparator

Do not look for one analysis that spans four languages. Each language already
has a tool that resolves imports, and none of them needs a new parser.

```
rust/extract.py   ─┐
ts/extract.mjs     ├─→  (package, resolved_target, kind)  ─→  compare against
python/extract.py  │         one intermediate form             PACKAGES.toml
dotnet/extract.py ─┘
```

The language knowledge lives in the extractors, which are individually small.
The rule is written once, in the comparator. A fifth language later is a fifth
extractor and no change to the rule.

`kind` distinguishes a normal import from a development or test one, because a
test importing across a boundary is a different finding from a shipped module
doing it.

---

## 3. Per language

### Rust — mostly self-enforcing, one real gap

An undeclared dependency does not compile. Rust enforces this already, so an
extractor adds little for the ordinary case.

**The real risk is a path dependency reaching outside the declared graph** — a
manifest-level fact, and one `ecosystem/check.py` can already see. The extractor
is therefore the lowest-value of the four and should be built last.

*Covered by:* the existing manifest check, plus `cargo metadata`'s resolved
paths.

### TypeScript — the widest hole

A **deep import** satisfies the manifest while crossing a boundary the package
never exported:

```ts
import { thing } from "@scope/pkg/dist/internal/thing";   // manifest: satisfied
```

`@scope/pkg` is a declared dependency, so nothing in a manifest check objects.
But `dist/internal/thing` is not in that package's `exports` map, which means
the package never offered it and is free to delete it in a patch release.

**So the TypeScript extractor compares resolved specifiers against each
package's `exports` map, not against its `dependencies`.** That is the
difference between checking that a package is allowed and checking that the
*entry point* is.

*Uses:* `tsc --traceResolution`, or a module-graph walk, for resolved
specifiers.

### Python — straightforward, with one stated limit

An AST walk over `import` and `from` statements, resolved with `importlib`.

**A dynamic import defeats static analysis**, and that is a limit to state
rather than a gap to imply coverage over. A module that builds a name and calls
`importlib.import_module` on it is invisible to this check, and the report must
say so rather than counting the file as clean.

### C# — genuinely not checkable by import analysis

`ExtendedResearch.Interop` ships C# **source** as content files, and each
consumer compiles them into its own assembly as `internal` types. At source
level those types are indistinguishable from the consumer's own. There is no
import to analyse, because there was no import — the code is simply present.

**The namespace has to be the signal.** Checking .NET honestly needs a declared
map from namespace to owning package:

```toml
[[package]]
name = "ExtendedResearch.Interop"
kind = "dotnet"
namespaces = ["ExtendedResearch.Interop"]   # new field
```

**This changes `PACKAGES.toml`'s schema, not just the checker**, which is why it
is decided now rather than when the extractor is written. A schema field added
after four repositories have written declarations is four edits; added now it is
one.

---

## 3a. What a manifest cannot see, beyond imports

Import analysis is the smaller half. Evidence from the `plugins` session, whose
audit found three shapes no import graph contains — and one of them is a live
defect right now.

### Runtime seams

A package can depend on a sibling with no manifest edge and no import:

- **A filesystem path into a sibling working directory**, read at run time. A
  code generator reaching `../<sibling>/proto` to compile a grammar leaves
  `cargo metadata` clean while the repository is not self-contained.
- **Spawning a sibling's binary as a child process.** A dependency with no entry
  in any manifest in any language.

Both are greppable: a relative path escaping the package root, and an invocation
of a known sibling binary. Crude, and it catches exactly what defeats everything
else.

### Forks, which are not edges at all

One package carries a near-verbatim fork of another's crate — 663 code lines
each, two differing — and names it in no manifest. There is no edge to declare
and none to catch. The fork is also **ahead** of the original, carrying support
the original lacks.

**This defeats the duplication check as built.** `ecosystem/check.py duplication`
hashes files and finds byte-identical pairs; two files differing in two lines
hash differently and pass. Catching a diverged fork needs similarity rather than
equality — a different tool, with a threshold to tune and false positives to
answer for.

### Rules split across repositories, with nothing binding the pieces

The sharpest case, and a defect today rather than a hypothesis.
Suspend-behaviour classification is written in three places: this repository
owns the enumeration and the literal set, one consumer owns the platform table,
and another owns the function joining them.

**The drift runs both ways, which is what makes it the worked example.** Each
side knows a literal the other does not:

| Literal | the consumer's tables | vector 0017 |
|---|---|---|
| `CLOCK_BOOTTIME` | includes suspend | yes |
| `CLOCK_MONOTONIC_RAW` | includes suspend | yes |
| `QueryPerformanceCounter` | includes suspend | yes |
| `CLOCK_MONOTONIC` | excludes suspend | yes |
| `performance.now` | **absent** | yes |
| `QueryUnbiasedInterruptTime` | excludes suspend | **absent** |

Four shared, one each way. In one direction the defect is live: a browser face
emits `performance.now`, and the consumer's classifier answers `Unclassified`.
In the other it is latent — a face emitting `QueryUnbiasedInterruptTime` would
fail this repository's check while the consumer classifies it correctly. Checked
and not currently live: the platform table emits `QueryPerformanceCounter` and
`CLOCK_BOOTTIME`, and names `QueryUnbiasedInterruptTime` only in a doc comment
describing what it does *not* use.

The two sides are also different shapes. This repository asks "is this a literal
a face may emit"; the consumer asks "does this call include suspend or exclude
it", and answers with two tables. **A similarity check would never find this**,
because the code does not resemble itself across the boundary — it is one rule
split into adjacent questions, and nothing holds the answers together.

The vector exists and its own description says every face's live row checks
against it. Nothing runs that consumer against it. **Three repositories each
wrote down a piece and none wrote down the join.**

One posture worth preserving into the shared version: the consumer's classifier
matches exactly and reports a near miss as unknown rather than guessing, on the
grounds that a producer who wrote something almost right is a producer whose
string nobody has checked. A fuzzy match would hide exactly the drift above.

### The distinction that makes this hard: sanctioned duplication

Not every copy is a defect, and a check flagging them all would be wrong.
Vector `0018` states that two implementations exist to be held to one answer
**without either depending on the other** — there the duplication is the design
and the vector is what keeps it honest.

So the check cannot ask "is this duplicated". It has to ask "is this duplication
declared, and is there a vector holding the copies together". That is what
`[[shared_rule]]` in `ecosystem/PACKAGES.toml` is for. It has **zero
registrations today** while at least three cases are known: the enumeration
short-name rule, the error-token grammar, and this suspend-behaviour split.

**Registering them is the work, and writing the vectors is most of it.** A
`[[shared_rule]]` without vectors is a comment.

---

## 4. What this will not catch

Stated here so that a green run is not read as a guarantee it cannot give:

- **A dynamic import in Python**, per §3.
- **Reflection in C#**, for the same reason, plus everything §3's C# note
  covers.
- **A type leaking through a public API.** A package exposing another package's
  type in its own signature couples every consumer to both. That is a different
  analysis — `cargo public-api` over rustdoc JSON for Rust, and nothing off the
  shelf for the others — and it is not this check.
- **A build script or macro that reaches across a boundary at compile time.**

---

## 5. Order

0. **Register the three known shared rules and write their vectors** (§3a).
   Ahead of everything else: one of them is a live defect, the declaration
   already exists and is empty, and the vectors are what every later check leans
   on. It needs no merge and no schema change.
1. **The `namespaces` field in `PACKAGES.toml`**, before any repository writes
   its declaration. Schema first, because it is the only part that gets more
   expensive with time. A `seams` field belongs in the same pass, declaring the
   filesystem paths and child processes a package may reach.
2. **The comparator and the intermediate form**, with no extractors. It fails
   loudly with "no extractor for this language", which is honest and is a
   working skeleton.
3. **TypeScript extractor** — the widest hole, so the most value per line.
4. **Python extractor** — straightforward, with the dynamic-import limit
   reported.
5. **C# extractor** — needs step 1 and is the one whose honesty depends most on
   the declaration being right.
6. **Rust extractor** — last, because the compiler already covers the ordinary
   case.

The gate in decision 0002 is satisfied when steps 1 to 5 are done. Rust can
follow the merge, since an undeclared Rust dependency fails to compile whether
or not this check exists.
