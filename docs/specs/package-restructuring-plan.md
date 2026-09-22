# Package restructuring plan

Status: **in progress**, 2026-09-22. Authorised by
`foundation/docs/decisions/0001-foundation-is-a-set-of-independent-packages.md`. Step 1 of
section 4 has landed: `crates/status` exists, `codes` and `guard` live in it,
and `check` sits beside `status`. Steps 2 to 7 have not.

---

## 1. The package set

| Package | Owns | Depends on | Standalone? |
|---|---|---|---|
| `extendedresearch-status` | The vocabulary of an integer crossing a C boundary: the boundary codes, `DOMAIN_FLOOR`, `is_boundary`, `is_domain`, `name`, `describe`, `token`, the `AbiError` trait, `status` (outbound) and `check` (inbound), plus `guard` | nothing | yes |
| `extendedresearch-abi` | The mechanics of the boundary: `borrow`, `buffer`, `enumeration`, `binding` (the calling side), `conformance` | `status` | yes |
| `extendedresearch-pyo3` | PyO3 bindings over a core that implements `AbiError` | `status` | yes |
| `extendedresearch-napi` | napi-rs bindings over the same | `status` | yes |
| `@extendedresearch/binding-runtime` | The TypeScript half of a napi binding | — | yes |
| `ExtendedResearch.Interop` | The C# half | — | yes |
| `extendedresearch-clock` | Clock domains, readings, bounds, fits, anchors, drift, quanta | nothing | yes |
| `extendedresearch-metrology` | Chains, terms, provenance, calibration, budgets, requirements, the record | `clock` | yes |
| `extendedresearch-conformance` | The cross-language driver comparator | — | yes |
| `extendedresearch-style` | The style guide, its configuration, and the check | — | yes |
| CI actions | `prove-tests-ran`, `style-check` | — | yes |

**The binding kit is one versioned unit.** `status`, `abi`, `pyo3`, `napi`,
`binding-runtime` and `Interop` share one error-code numbering and one token
grammar; a change to a boundary code moves all six. They are separate *packages*
because they have different dependents, and one *release unit* because they have
one wire contract.

`clock`, `metrology`, `conformance`, `style` and the actions are independent of
that unit and of each other, except `metrology → clock`.

---

## 2. Splitting `status` out of `abi`

### The proposal on the table

`ranvier` proposed an `extendedresearch-status` package keeping `codes` as the
module name inside it, holding the boundary codes, the predicates, `name`,
`describe`, `token`, the `AbiError` trait, `status`, and `check` moved out of
`binding` — with `guard` folded in as a judgement call. Its argument: `status`
and `check` are inverses of one conversion and sit in different modules, and
`check` is filed under `binding`, which is the *calling* side — full of
measure-then-copy, retries and `ReadError`. An adapter wanting `check` has to
import from a module that has nothing else to do with it.

### What checks out

Verified, not taken on trust:

```bash
grep -n "pub fn status\|pub fn check" crates/abi/src/*.rs
#   crates/abi/src/binding.rs:95:pub fn check(code: i32) -> Result<(), i32>
#   crates/abi/src/codes.rs:242:pub fn status<T, E, F>(result: Result<T, E>, deliver: F) -> i32
```

The cohesion argument is correct: two inverse conversions, two modules, and the
one holding `check` is otherwise about reading buffers.

### The stronger argument, which the proposal did not make

Every module in the crate except `borrow` already depends only on `codes`:

```bash
grep -n "^use crate::" crates/abi/src/{binding,buffer,enumeration,guard}.rs
#   all four: use crate::codes::{...}      # and nothing else from the crate
```

And the real `unsafe` is 27 lines in `borrow.rs` and nowhere else in `src/` —
every other hit is prose in a doc comment.

So `codes` is the leaf everything imports, and `borrow` is a leaf nothing
imports. **The consequence today is that every package's safe core depends on
the crate that dereferences raw pointers**, because `AbiError` and `mod borrow`
ship together. That is the boundary worth drawing, and it is drawn exactly where
the `unsafe` is:

- **`status`** — no `unsafe`, no pointers, no `std` requirement except `guard`.
  What a safe core depends on.
- **`abi`** — the pointer mechanics. What a C adapter depends on.

### Where this plan differs from the proposal

**Two packages, not more.** The proposal raised splitting `borrow` out as well,
and worried about the resulting diamond. Two answers:

1. **A diamond is not a hazard.** Cargo resolves a diamond to one copy whenever
   the requirements are compatible. The hazard this project actually has is
   already documented and is different: two packages pinning *different revs* of
   this repository get two copies of every crate, which Cargo treats as
   unrelated types. That is about rev pinning, not about graph shape.
2. **Splitting `borrow` buys nothing and costs the most call sites.** `borrow`
   is what a C adapter uses; a C adapter also uses `buffer` and `enumeration`.
   No consumer wants one without the others. Keeping them together means the 595
   `borrow::` sites in `ranvier` do not move at all.

```bash
git -C ranvier grep -c "borrow::" -- '*.rs' | awk -F: '{s+=$2} END {print s}'   # 595
```

**`guard` folds in, behind a feature.** It is the same vocabulary — containing a
panic is how a Rust failure becomes `ERR_PANIC` — and it depends only on
`codes`. But it needs `std::panic::catch_unwind`, so folding it in unconditionally
would make `status` require `std`. It goes in behind a default-on `std` feature,
which keeps `status` usable from instrument firmware and `no_std` targets without
costing any current consumer anything.

**`status` is `no_std` + `alloc`-free in its default path.** It is integers, a
trait and two conversions; nothing in it needs to allocate. That is a property
worth having in a package meant to be foundational, and it is free today.

### What moves, per repository

| Repository | Moves | Does not move |
|---|---|---|
| foundation | `codes.rs` and `guard.rs` to the new crate; `check` from `binding.rs` to beside `status` | Everything else |
| `ranvier` | Imports in the files that use `codes::` or a local `check`; its two copies in `bindings/{node,python}/src/status.rs` are deleted | 595 `borrow::` sites |
| `ca3` | The same, in `crates/ffi`, `bindings/node`, `bindings/python` | Its `borrow::` sites |
| `eres` | The same | — |

Keeping `codes` as the module name is what makes this an import change rather
than a rename: `use extendedresearch_status::codes;` leaves every `codes::OK` as
it was.

---

## 3. What the other packages need

**`clock`** is already standalone: no dependencies, `wasm32` clean. It needs a
README written for someone who has never heard of this project, and a decision
on `no_std` — `fit.rs` uses `VecDeque`, so the sliding window needs `alloc`
while the rest does not.

**`metrology`** is new and lands with the boundary already right.

**`conformance`** is standalone in substance: it runs each binding's driver over
one cases file and compares them, configured by a `conformance.toml`. What ties
it here is presentation, not code.

**`style`** has no code dependency on anything.

**Naming.** The `extendedresearch-` prefix stays as a namespace — ordinary
practice, and not what makes a name unhelpful. What matters is that the second
half names what the package does rather than where it sits in our architecture.
`clock`, `metrology`, `status`, `style`, `conformance` pass that test. `abi` is
the weak one: it names a boundary, not a capability. It is kept because the
alternative is renaming the 595-site surface for a presentational gain, and
because `abi` is at least accurate. Revisit at publication, not before.

---

## 4. Order

Each step leaves the tree green, and no step depends on a later one.

| # | Step | Consumer impact |
|---|---|---|
| 1 | **Done.** Create `crates/status`; move `codes.rs` and `guard.rs`; move `check` beside `status`; `extendedresearch-abi` re-exports both modules so nothing breaks | none — re-exports keep every path working |
| 2 | **Done**, with step 1. Point `pyo3`, `napi` and `abi-testlib` at `status` directly | none |
| 3 | Consumers switch their imports to `extendedresearch_status` at their own pace | one import line per file |
| 4 | Remove the re-exports from `abi` once no consumer uses them | a compile error naming the fix, on the version that removes them |
| 5 | Land `metrology` (timing plan step 0) | none — additive |
| 6 | Standalone README for each package; rewrite the admission rule in `CONTEXT.md` | none |
| 7 | **Deferred to v1:** per-package versions, `<package>-v<x.y.z>` tags, per-package release assets, registry publishing | a version scheme to adopt |

Step 1's re-export is what makes this safe: the split lands without a
coordinated change across four repositories, and step 4 happens when the
consumers are ready rather than when foundation is.

---

## 5. What holds the boundaries while versioning is deferred

Lockstep versioning cannot tell you whether a boundary is real. These can, and
all three are enforceable now:

1. **Manifests are explicit.** A package that does not declare a dependency
   cannot use it. `cargo tree -p extendedresearch-status` showing no edges is
   the check.
2. **Every package README stands alone.** A reader who has never heard of this
   project can tell what the package is for and use it. A README that needs to
   explain `ranvier` is a boundary in the wrong place.
3. **The graph is acyclic and shallow.** `status ← abi`, `status ← pyo3`,
   `status ← napi`, `clock ← metrology`. Nothing else. A new edge is a reviewed
   change.

A fourth would be worth building and does not exist: a check that the declared
graph in this document matches what the manifests resolve. Saying so is more
useful than implying the document is enforced.

---

## 6. Open

**`clock` and `no_std`.** Free for most of the crate, costs the sliding window
an `alloc` feature. Worth doing when the crate is split for firmware use, which
nothing needs yet.

**`conformance` as a package name.** `extendedresearch-conformance` describes a
test runner for cross-language bindings. Someone searching for that would not
find it by name. Low priority — it is a development tool, not a library.

**Whether `binding` and `conformance` belong in `abi` or in a third package.**
Both are test-side: one is the calling side for a package's own C-ABI tests, the
other is the kit a library runs against its own exports. Neither is used by
production adapter code. Splitting them would let a shipped adapter depend on
less. It is the next boundary to consider and it is not part of this plan.
