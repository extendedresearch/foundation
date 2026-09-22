# How `plugins` should arrive: five packages, not one

Status: **proposed**, 2026-09-22. A recommendation, not a decision — the
repository is another owner's. Measured from its tree on 2026-09-22.

---

## 1. What is actually there

Larger than "twenty directories across three tiers": **31 Rust crates in 4
workspaces, 21 publishable npm packages**, plus a 17,594-line root test suite
and two tool directories with no manifest at all.

```bash
find . -name Cargo.toml -not -path "./node_modules/*" -not -path "./target/*"  # 31 + 1 workspace root
cargo metadata --format-version 1 --no-deps --frozen                           # 28 workspace members
```

The largest single units are TypeScript, not Rust: a shell at 29,529 lines, an
export surface at 24,869, a drawing framework at 18,572.

---

## 2. The three facts the graph actually shows

Everything below follows from these. All three were read from `cargo metadata`
and from a parse of every import specifier in 549 TypeScript files, not from the
manifests alone.

### The Rust graph is bipartite, and the cut is already there

`base-providers` has **five in-edges and zero in-repo out-edges**. Above it, the
schema crates — one grammar crate, four Base schema crates, six modalities, four
measurements — carry **zero `ranvier` and zero `ca3`**. Below it, six devices and
four measurements carry `ranvier-{node,session,observe}`.

That is not a coincidence, it is a property the repository already tests:
`tools/conformance/tests/independence.rs` asserts *"a plugin must be usable with
`ca3` alone, with `ranvier` alone, or with neither."*

**This is the boundary worth drawing**, by foundation decision 0001's own test: does the
split let a consumer avoid depending on something it does not want? Somebody who
wants the EEG vocabulary should not acquire a streaming runtime with it.

### One crate is the sink of everything and depends on nothing

`standard-common`: **21 in-edges, zero out-edges**, two third-party dependencies
(`prost`, `prost-types`), 8,663 lines. It contains a 3,628-line well-formedness
checker whose own header argues for living beside the grammar.

It is the strongest standalone candidate in the repository and it is not close.

### The TypeScript graph has two hubs that do not touch each other

`Dashboard` is the sink of twelve view packages plus the test suite; `Workbench`
has out-degree zero and two in-edges. Neither depends on the other — and
`Marks` exists, by its own documentation, *precisely because* they do not.

Both graphs are acyclic, in normal and dev modes.

---

## 3. The criterion the graph cannot supply

The dependency graph says where a split is **possible**. It does not say where a
split is **valuable**, and for this repository the deciding question is not
internal coherence — it is:

> Can somebody outside this project add a device integration in an afternoon,
> without reading the monorepo?

That reframes one of the five. The graph says the seven devices are leaves with
in-degree zero, so bundling them costs nothing structurally. **The community
criterion says the opposite: a device is the extension point, so it must be a
package boundary whether or not the graph forces one.**

If devices ship as one bundled package, a contributor's device has nowhere to
live except inside it — which means a pull request against the monorepo, a
review by the owners, and a release cadence they do not control. If each device
is its own package against a published trait, their device lives in their
repository and depends on ours. That is the difference between an ecosystem and
a codebase with contributors.

**So the extension points are package boundaries by definition, and the first
job of the decomposition is to make the contributor's dependency set small and
obvious.** For a device author that set is: the provider trait, the one or two
modality vocabularies they emit, and the transports they use. Nothing else —
no drawing framework, no shell, no export surface, no other device.

Measured against today's tree, that is already nearly true:

```
device-screen   -> base-providers, modality-av, modality-av-audio + 3 runtime crates
device-muse     -> base-providers, base-clock, base-devicestate,
                   modality-imu, modality-neuro + 5 runtime crates
device-openxr   -> nothing at all
```

No device depends on another device, on the surface, or on conformance. The
tree already has the shape; the packaging has not caught up with it.

---

## 4. The recommendation

**Six packages and a template.**

| # | Package | What it holds | Why it is a boundary |
|---|---|---|---|
| 1 | **standard** | The grammar crate and the well-formedness checker | Sink of 21 edges, zero in-repo dependencies, two third-party. Nothing else in the repository is this separable |
| 2 | **vocabularies** | The four Base schema units, six modalities, four measurements — both languages | Carries zero `ranvier` and zero `ca3`. This is the half a consumer can take without a runtime |
| 3 | **providers** | `base-providers` alone — the trait a device implements | **The contributor-facing interface.** 2,238 lines, zero in-repo dependencies. Everything a device author must understand, and nothing else. It is small because it should be |
| 3b | **devices** | The seven first-party integrations, **each its own package** | Not bundled. A device is the extension point, so each one is both a published package and a worked example a contributor can read end to end |
| 3c | **recording** | `Recording/Ca3` | The only unit naming both siblings, with one dev-only in-repo edge. Already free-standing on the graph |
| 4 | **surface** | `Dashboard`, `Workbench`, `Export`, `Viewer`, `Marks` | ~80,000 TypeScript lines with their own two-hub shape and their own third-party graph (React, `@xyflow/react`) |
| 5 | **conformance** | `tools/conformance`, `tools/fixture` | Must see all of the above — it enforces the tier rules, the reachability of the TypeScript build, and the asset gates. A checker is not a participant. **And it has to be runnable by an outsider against their own device**, which it is not today |
| — | **the device template** | A scaffold, not a package | §5 |

### Why not one package

The bipartite cut is real and it is the whole point of foundation decision 0001. One
package means a consumer wanting a gaze vocabulary acquires seven device
integrations, a streaming runtime, a recording bridge and 80,000 lines of React.

### Why devices are split and vocabularies are not

Both are "many small units the graph says could be bundled". They go opposite
ways because the criterion is not symmetry, it is **who adds the next one**.

Nobody outside this project adds a modality vocabulary casually — it is a
schema change with conformance consequences, and the fourteen existing ones
share a build pipeline and a release cadence. Somebody outside this project
should add a device every week. One is an extension point; the other is a
component.

### Why not twenty-five

Because **the split has to buy a consumer something**. Fourteen vocabulary
crates each depend on exactly `{standard, base-core}` and `prost`; nothing wants
one of them without the others' infrastructure, and there is no edge between any
two of them to cut. Splitting them fourteen ways costs fourteen manifests,
fourteen READMEs and fourteen version numbers, and buys nobody anything.

Two of them are 47 and 99 lines of Rust. A package that small is a module.

---

## 4. The one thing that blocks this, and it is not small

**`CONTRACT_VERSION` is defined in `Dashboard/` and compiled into twelve
vocabulary packages, with a hard throw on mismatch.**

```
Dashboard/Plots/version.ts:23    CONTRACT_VERSION = 2
Dashboard/Stream/index.ts:40     LANE_CONTRACT_VERSION = 1
Dashboard/Stream/registry.ts:155,182,256   throw on mismatch
```

It crosses the boundary between **surface** and **vocabularies** — the two
packages that otherwise have the least to do with each other — and it makes them
one release unit. A bump is a coordinated thirteen-package release, and skew is a
runtime refusal rather than a degraded render.

**Three ways out**, and this is the decision the decomposition rests on:

1. **Move the constant to a neutral package.** `Marks` already exists for exactly
   this reason — a thing both hubs can depend on without depending on each
   other. This is the cheapest and it is what the repository's own reasoning
   points at.
2. **Invert it**: a vocabulary declares the contract it satisfies, and the
   framework checks. The arrow then runs the way the dependency already does.
3. **Accept that surface and vocabularies are one release unit** and merge
   packages 2 and 4. Honest, and it gives up the largest separation available.

I would take (1).

---

## 5. The template, and what "an afternoon" actually requires

A package boundary lets a contributor's device live outside the monorepo. It
does not make writing one easy. The target — **a working device integration in
an afternoon, by somebody who has not read this repository** — needs five
things, and the repository already has three of them built for other purposes.

| What the contributor needs | State today |
|---|---|
| **One command that scaffolds a device crate** — manifest, trait stub, proto wiring, a test, a CI job | Does not exist |
| **A trait small enough to read in one sitting** | `base-providers`, 2,238 lines. Needs a README that is a tutorial, not a reference |
| **A way to run their device with no hardware** | `tools/fixture` — 5,766 lines, already built, currently internal-only |
| **A conformance check they can run against their own crate** | `tools/conformance` — 7,916 lines, already built, **depends on 13 in-repo crates**, so an outsider cannot run it without vendoring the tree |
| **A worked example that is real** | Seven of them, and `device-openxr` is the smallest at 1,819 lines |

**The two that exist but are internal-only are the interesting ones.** A
conformance suite an outsider cannot run is a conformance suite that only
polices insiders — and this project already solved that problem once:
`extendedresearch-conformance` is a pip-installable development tool that runs a
binding's driver over a cases file and compares the languages, configured by a
TOML file in the consuming repository. The same shape works here. A device
author installs the checker, points it at their crate, and gets the same verdict
CI gives a first-party device.

**The scaffold should generate a device that passes conformance on the first
run, emitting a fixture stream.** Not a skeleton that compiles — one that works,
that the contributor then edits into their hardware. The difference decides
whether the first hour is spent making something run or making something build.

**The seven first-party devices become the examples**, which is the second
reason not to bundle them: a bundled package is something to read through, seven
separate ones are seven things to read *one of*.

### What this costs, stated honestly

`tools/conformance` depending on 13 in-repo crates is the real work. It has to
become a checker that reads a declaration and a built artefact, rather than one
that imports every crate it checks. That is a substantial rewrite of a
7,916-line tool and it is the single largest item implied by the community goal.

Nothing else here is large. The scaffold is a template repository or a
`cargo generate` source; the trait's README is a day.

---

## 6. A second coupling, less structural but worth naming

**Deep subpath imports are the norm.** Of 470 `@extendedresearch/*` import
sites, the great majority address an internal path — `dashboard/plots/target`,
`workbench/shell/AppShell`. `Export → workbench` is seven sites and **all seven**
are deep; `Viewer → dashboard` is thirty and **all thirty** are deep.

A manifest records these as a dependency on a package. They are actually a
dependency on that package's internal file layout, which it is free to change in
a patch release. This is the TypeScript hole `docs/specs/boundary-enforcement-plan.md`
§3 describes, and `plugins` is where it is worst.

Splitting into packages does not fix it and does not make it worse. But package
boundaries are the moment an `exports` map has to be written down, so it is the
cheapest moment to fix it.

---

## 7. Four things that do not fit anywhere, found on the way

**`device-openxr` is not a plugin.** Zero in-repo dependencies, zero `ranvier`,
zero `ca3`, declares no vocabulary and publishes nothing. It is a 1,819-line
OpenXR runtime prober living in a plugin repository. It belongs in **runtime**
by elimination, not by argument.

**`Base/Providers` is an undocumented fork.** Its own `SPEC.md:234` says *"ranvier
does not carry this package's shape."* The streaming runtime's provider crate
exists and carries the **identical** `description` string. The divergence is real — plugins'
copy has input ports that ranvier's lacks (`pub struct Inputs`, absent from
ranvier) — so the copy is *ahead*, and a sentence in its specification is false.
Settle this before the merge: upstream the input ports and depend, or declare the
fork with vectors holding the two honest.

**Three units have no consumer in the repository at all.** `Marks` (1,093
lines), `Base/Session` (4,193) and the `recording-ca3` npm package (727).
Whatever decides their packaging, it is not the internal graph.

**`tools/viewshot` reaches two levels above the repository root**, into an
application's `node_modules`, for a platform `esbuild` binary and a wasm
package — declared in no manifest and exercised by no CI job. It is the only
seam in the repository that nothing anchors, and the application it borrows
from is out of the migration entirely.

---

## 8. What this costs

**One Cargo workspace and one lockfile** today, whose own header argues that
resolving once *"is what keeps two plugins from compiling two revisions of the
same dependency."* Five packages in one monorepo workspace keeps that property;
five separate workspaces would lose it. Keep one workspace.

**The root build pipeline is order-dependent** — generated code must be copied
before `tsc -b`, or a clean build fails where an incremental one passes. That
pipeline spans every npm package and would need to survive the split.

**A Rust test polices the JavaScript build.** `tools/conformance`'s `reachable`
test exists because *"break `dist/test/*.test.js` and a checker living in
`test/` stops running."* Any split that leaves conformance behind loses the tier
check, the reachability check and the asset gates — which is the argument for
package 5 rather than folding it into another.

---

## 9. What I need from the owner

1. **The `CONTRACT_VERSION` resolution** (§4). The decomposition rests on it.
2. **The `Base/Providers` fork disposition** (§6) — upstream, or declare.
3. **Whether five is the right grain**, or whether `vocabularies` should split
   further along the eye-tracking measurement chain, which is the one place
   inside it with real internal edges.

Everything else in this document is a measurement, not a choice.
