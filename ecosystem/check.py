#!/usr/bin/env python3
"""The checks that hold this repository's package boundaries.

    python ecosystem/check.py boundaries     # the dependency graph is what PACKAGES.toml says
    python ecosystem/check.py duplication    # no copy that is not declared, no vendored tree
    python ecosystem/check.py documents      # every package carries the standard set
    python ecosystem/check.py citations      # every decision citation names its repository
    python ecosystem/check.py all            # all four
    python ecosystem/check.py all --strict   # soft findings fail too

Every check prints every row it compared, passing rows included. A report of
only failures cannot distinguish "checked and clean" from "never checked", and
a declaration nobody checks is a comment.

**Every table is reported before the results are combined.** `and`
short-circuits, so `a.report() and b.report()` stops printing at the first
failure and `b` is not empty and not failing — it is absent, which is what a
table that was never reached and a table that had nothing to say look like from
the outside. This file had that defect at three sites; `ecosystem/tests/` covers
it, because no passing run can.

`ecosystem/comparator.py` is the fifth check and is deliberately not part of
`all`: it compares what the source actually imports against this declaration,
and with no extractor written yet it fails on every language. Wiring a check
that cannot pass into CI would train everyone to ignore CI.

Standard library only; `tomllib` is why the floor is Python 3.11.

# boundaries

**Both directions.** An intra-repository dependency that exists and is not
declared fails, and a declared dependency that does not exist fails. The first
is the one that bites: an unwanted edge arrives as one manifest line and is
invisible afterwards.

**Third-party dependencies are declared too.** `extendedresearch-status` and
`extendedresearch-clock` having no dependencies is a property worth keeping,
and it is only kept if adding one fails a build.

**The graph must be acyclic**, and the cycle is named when it is not.

**Every cargo workspace in the tree is read, not just the root's.** One
`cargo metadata` at the repository root sees one workspace; a consuming
repository has nine, and against it eight correctly-declared packages reported
`MISSING`. The obvious way to make that run green is to delete the eight rows,
which leaves real edges undeclared and unwatched — a checker whose failure mode
is "delete the true rows" is worse than no checker.

**A package is keyed by its name and its kind, never by its name.** A name is
not unique: one repository declares a Rust crate and a Python distribution that
are both correctly called `ca3`. Keyed on the name alone, twenty-three declared
packages became twenty-two nodes and one silently replaced the other. A
`depends` entry names a package, so it is resolved to the package of the same
kind first and to a unique holder of the name second; an entry that could be
either is reported rather than picked.

**A declared namespace is owned by the package that declares it.** A
source-only .NET package compiles its files *into* the consumer, where its types
are `internal` to that assembly and indistinguishable from the consumer's own.
There is no import to read because there was no import, so the namespace is the
only attributable signal there is. What is checkable today is the direction that
rots first: a namespace declared here and used by no `.cs` file in the tree.

**A declared seam reaches outside its own package.** A package can depend on a
sibling with no manifest edge and no import at all — by reading a path into a
sibling working tree at run time, or by spawning a sibling's binary. Neither
appears in any manifest in any language. `seams` is where such a reach is
written down; an entry whose path resolves back inside the declaring package is
not a seam, and a seam with no reason is a line nobody can review.

# duplication

Every tracked text file is hashed with line endings normalised. Two files with
the same hash are a copy, and every copy must match a `[[copy]]` declaration
giving the reason it cannot be a dependency. A vendored tree is refused by path.

What this cannot do is detect the same rule written twice in two languages.
`[[shared_rule]]` is the declaration for that case: it requires one set of
language-free vectors and a test per implementation consuming them.

# documents

A reader who has used one package should know where to look in the next. The
required set is checked for presence, and a README's heading order is reported
against the standard — softly, until the existing READMEs are migrated.

Decision records are numbered uniquely within their package. A `README.md`
beside them is an index, not a record, and is exempted by name — not by
loosening the pattern, because the pattern is what stops a record being filed
under a number nobody can cite, and widening it would admit every unnumbered
file.

# citations

**Every reference to a decision record outside `docs/decisions/` names the
repository whose record it is**, and every reference naming *this* repository
resolves to a file that exists.

A bare `decision NNNN` is unresolvable, and no mechanical rewrite can fix it
afterwards: the number alone does not say which of five record sets to look in,
and the writer who knew is not there to ask. The September 2026 ecosystem audit
counted 269 lines written that way, and found that in one repository the bare
numbers resolve to a *sibling's* records while the repository doing the citing
has no such number — so a reader who follows one gets a confident answer to the
wrong question. Re-derive the count in any checkout with:

    git grep -nE '[Dd]ecisions? [0-9]{4}'

**A match inside a binary file is reported, not parsed.** A citation reaches a
compiled protobuf descriptor as a source comment carried through codegen.
Deciding whether such a match is qualified would mean guessing at an encoding
and at where the surrounding bytes begin, which is a judgement the parser would
be making instead of a person. The row says a citation is in there and stops.

**The records themselves are not scanned.** A record citing its neighbour by
number is resolvable — it is in the same directory — and requiring the
repository name there would be noise. Everything else in the tree is scanned,
code comments and specifications included, because those are where a citation
outlives the person who wrote it.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from collections import defaultdict
from pathlib import Path, PurePosixPath

# Set by `main` from `--root`, so one copy of this script checks any repository
# against that repository's own declaration. Every function reads them at call
# time.
ROOT = Path(__file__).resolve().parent.parent
DECLARATION = ROOT / "ecosystem" / "PACKAGES.toml"


def set_root(root: Path) -> bool:
    """Point every check at `root`. False if it carries no declaration.

    `ecosystem/comparator.py` calls this too, so that one definition of where
    the tree is serves both, and a second copy of it cannot drift.
    """
    global ROOT, DECLARATION
    ROOT = root.resolve()
    DECLARATION = ROOT / "ecosystem" / "PACKAGES.toml"
    return DECLARATION.is_file()


class Table:
    """A named block of rows, printed whole."""

    def __init__(self, title: str) -> None:
        self.title = title
        self.rows: list[tuple[str, str, str]] = []

    def add(self, what: str, verdict: str, detail: str = "") -> None:
        self.rows.append((what, verdict, detail))

    def report(self, soft: set[str] | None = None) -> bool:
        soft = soft or set()
        print(self.title)
        if not self.rows:
            print("  (nothing to check)\n")
            return True
        width = max(len(r[0]) for r in self.rows)
        failed = 0
        for what, verdict, detail in self.rows:
            bad = verdict.isupper() and verdict not in {"OK", "PASS"}
            if bad and verdict not in soft:
                failed += 1
            print(f"  {verdict.lower() if not bad else verdict:<9} {what:<{width}}  {detail}")
        passed = len(self.rows) - failed
        print(f"  {passed} passed, {failed} failed\n")
        return failed == 0


def load() -> dict:
    with DECLARATION.open("rb") as handle:
        return tomllib.load(handle)


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout
    return [p for p in out.split("\0") if p]


def key_of(package: dict) -> tuple[str, str]:
    """A package's identity: its name and its kind, never the name alone.

    A name is not unique across kinds. One repository in this organisation
    declares two packages called `ca3` — a Rust crate and a Python
    distribution, both correctly named — and a dict keyed on the name alone
    silently keeps one of them. Twenty-three declared packages became
    twenty-two nodes, and an edge added to either would have been checked
    against the other's row.
    """
    return (package["name"], package["kind"])


def labels_for(packages: list[dict]) -> dict[tuple[str, str], str]:
    """A printable label per package, disambiguated only where it has to be.

    A name shared by two kinds prints as `name (kind)`; every other name prints
    as itself, so a repository whose names happen to be unique reads the same
    as it did before the key changed.
    """
    seen: dict[str, int] = defaultdict(int)
    for package in packages:
        seen[package["name"]] += 1
    return {
        key_of(p): (p["name"] if seen[p["name"]] == 1 else f"{p['name']} ({p['kind']})")
        for p in packages
    }


# ---------------------------------------------------------------------------
# boundaries
# ---------------------------------------------------------------------------


def first_line(text: str | None) -> str:
    lines = (text or "").strip().splitlines()
    return lines[0] if lines else ""


def cargo_manifests() -> list[str]:
    """Every tracked `Cargo.toml`, as the directory holding it."""
    return sorted(
        PurePosixPath(p).parent.as_posix()
        for p in tracked()
        if PurePosixPath(p).name == "Cargo.toml"
    )


def opens_a_workspace(directory: str) -> bool:
    """True when this directory's `Cargo.toml` carries a `[workspace]` table.

    Anchored and exact, so `[workspace.dependencies]` and `[workspace.package]`
    in a *member*'s manifest are not mistaken for a workspace root.
    """
    try:
        text = (ROOT / directory / "Cargo.toml").read_text(encoding="utf-8")
    except OSError:
        return False
    return re.search(r"(?m)^\[workspace\]\s*$", text) is not None


def declares_a_package(directory: str) -> bool:
    try:
        text = (ROOT / directory / "Cargo.toml").read_text(encoding="utf-8")
    except OSError:
        return False
    return re.search(r"(?m)^\[package\]\s*$", text) is not None


def cargo_metadata(directory: str) -> dict:
    raw = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT / directory,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(raw)


def rust_edges() -> tuple[dict[str, tuple[set[str], set[str]]], Table]:
    """Each Rust package's (intra-repository, third-party) normal dependencies.

    Read from `cargo metadata --no-deps`, which is the manifests as cargo
    resolves them rather than as a regex reads them. Development and build
    dependencies are excluded: they do not reach a consumer.

    **Every workspace in the tree is read, not just the root's.** One
    `cargo metadata` at the repository root sees one workspace; a consuming
    repository has nine, and against it eight correctly-declared packages
    reported `MISSING`. The obvious way to make that run green is to delete the
    eight rows, which would leave real edges undeclared and unwatched — a
    checker whose failure mode is "delete the true rows" is worse than none.

    A crate in a workspace of its own that no `[workspace]` lists is read on its
    own afterwards, so a tree with no workspace at all is covered too.
    `local` is the union across every workspace, because a crate in one
    workspace depending on a crate in another is still an intra-repository edge.

    Returns the edges and the table saying which workspaces were read, so that a
    run states its own coverage rather than leaving it to be inferred.
    """
    table = Table("Every cargo workspace in the tree was read")
    directories = [d for d in cargo_manifests() if opens_a_workspace(d)]

    metadata: dict[str, dict] = {}
    failures: list[tuple[str, str]] = []
    for directory in directories:
        try:
            metadata[directory] = cargo_metadata(directory)
        except subprocess.CalledProcessError as error:
            failures.append((directory, first_line(error.stderr)))

    # A crate no workspace listed. Read on its own, so that a repository with no
    # `[workspace]` anywhere is covered rather than silently empty.
    seen: set[str] = {
        str(Path(p["manifest_path"]).resolve()) for m in metadata.values() for p in m["packages"]
    }
    for directory in cargo_manifests():
        if directory in metadata or not declares_a_package(directory):
            continue
        if str((ROOT / directory / "Cargo.toml").resolve()) in seen:
            continue
        try:
            metadata[directory] = cargo_metadata(directory)
        except subprocess.CalledProcessError as error:
            failures.append((directory, first_line(error.stderr)))
            continue
        seen |= {str(Path(p["manifest_path"]).resolve()) for p in metadata[directory]["packages"]}

    local: set[str] = {p["name"] for m in metadata.values() for p in m["packages"]}
    edges: dict[str, tuple[set[str], set[str]]] = {}
    manifest_of: dict[str, str] = {}
    collisions: list[tuple[str, str]] = []

    for directory in sorted(metadata):
        meta = metadata[directory]
        for package in meta["packages"]:
            intra: set[str] = set()
            external: set[str] = set()
            for dependency in package["dependencies"]:
                if dependency["kind"] is not None:  # dev or build
                    continue
                (intra if dependency["name"] in local else external).add(dependency["name"])
            manifest = str(Path(package["manifest_path"]).resolve())
            if package["name"] in manifest_of and manifest_of[package["name"]] != manifest:
                collisions.append((package["name"], manifest))
                continue
            manifest_of[package["name"]] = manifest
            edges[package["name"]] = (intra, external)
        table.add(directory, "ok", f"{len(meta['packages'])} crate(s)")

    for directory, message in failures:
        table.add(directory, "CARGO", message or "cargo metadata failed here")
    for name, manifest in collisions:
        table.add(name, "COLLISION", f"a second crate of this name at {manifest}")
    if not metadata and not failures:
        table.add("(none)", "ok", "no tracked Cargo.toml, so no Rust package to read")
    return edges, table


def npm_edges(path: Path) -> tuple[set[str], set[str]]:
    manifest = json.loads((path / "package.json").read_text(encoding="utf-8"))
    return set(manifest.get("dependencies", {})), set(manifest.get("devDependencies", {}))


def dotnet_edges(path: Path) -> set[str]:
    found: set[str] = set()
    for project in path.glob("*.csproj"):
        for match in re.finditer(
            r'<PackageReference\s+Include="([^"]+)"', project.read_text(encoding="utf-8")
        ):
            found.add(match.group(1))
    return found


def python_edges(path: Path) -> set[str]:
    with (path / "pyproject.toml").open("rb") as handle:
        manifest = tomllib.load(handle)
    return {re.split(r"[<>=!~\[ ]", d)[0] for d in manifest.get("project", {}).get("dependencies", [])}


def check_boundaries(declaration: dict) -> bool:
    packages = declaration["package"]
    declared_names = {p["name"] for p in packages}

    labels = labels_for(packages)

    rust, workspaces = rust_edges()
    workspaces_ok = workspaces.report()

    table = Table("Dependencies: declared and resolved agree, in both directions")

    for package in packages:
        name, kind = package["name"], package["kind"]
        path = ROOT / package["path"]
        want_intra = set(package.get("depends", []))
        want_external = set(package.get("external", []))

        label = labels[key_of(package)]

        # A manifest this declaration points at and the tree does not hold used
        # to end the run with a traceback, which reports nothing about the other
        # twelve packages. An unreadable manifest is a finding on one row.
        try:
            if kind == "rust":
                if name not in rust:
                    table.add(
                        label, "MISSING", "declared here, and cargo metadata does not list it"
                    )
                    continue
                have_intra, have_external = rust[name]
            elif kind == "npm":
                have_external, have_dev = npm_edges(path)
                have_intra = have_external & declared_names
                have_external -= have_intra
                want_dev = set(package.get("external_dev", []))
                if have_dev != want_dev:
                    table.add(
                        f"{label} (dev)",
                        "DRIFT",
                        f"resolved {sorted(have_dev)}, declared {sorted(want_dev)}",
                    )
                else:
                    table.add(f"{label} (dev)", "ok", f"{sorted(have_dev) or 'none'}")
            elif kind == "dotnet":
                have_external = dotnet_edges(path)
                have_intra = have_external & declared_names
                have_external -= have_intra
            elif kind == "python":
                have_external = python_edges(path)
                have_intra = have_external & declared_names
                have_external -= have_intra
            else:  # tooling: no manifest, so nothing may be declared either
                have_intra, have_external = set(), set()
        except (OSError, ValueError) as error:
            table.add(label, "MANIFEST", f"{package['path']}: {first_line(str(error))}")
            continue

        problems = []
        if have_intra - want_intra:
            problems.append(f"undeclared edge(s) {sorted(have_intra - want_intra)}")
        if want_intra - have_intra:
            problems.append(f"declared and absent {sorted(want_intra - have_intra)}")
        if have_external - want_external:
            problems.append(f"undeclared third-party {sorted(have_external - want_external)}")
        if want_external - have_external:
            problems.append(f"declared third-party absent {sorted(want_external - have_external)}")

        if problems:
            table.add(labels[key_of(package)], "DRIFT", "; ".join(problems))
        else:
            inside = sorted(want_intra) or "nothing"
            outside = sorted(want_external) or "nothing"
            table.add(labels[key_of(package)], "ok", f"depends on {inside}; third-party {outside}")

    graph_ok = table.report()

    # `depends` names a package; a name can belong to more than one. An entry is
    # resolved to the package of the same kind first, then to a unique holder of
    # the name, and an entry that could be either is reported rather than picked
    # — picking would check the edge against the wrong row and print `ok`.
    nodes = {key_of(p) for p in packages}
    by_name: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for package in packages:
        by_name[package["name"]].append(key_of(package))

    resolution = Table("Every declared edge resolves to exactly one package")
    edges: dict[tuple[str, str], set[tuple[str, str]]] = {}
    for package in packages:
        here = key_of(package)
        targets: set[tuple[str, str]] = set()
        for wanted in sorted(package.get("depends", [])):
            same_kind = (wanted, package["kind"])
            row = f"{labels[here]} -> {wanted}"
            if same_kind in nodes:
                targets.add(same_kind)
                resolution.add(row, "ok", f"the {package['kind']} package of that name")
            elif len(by_name.get(wanted, [])) == 1:
                target = by_name[wanted][0]
                targets.add(target)
                resolution.add(row, "ok", f"the only package of that name ({target[1]})")
            elif by_name.get(wanted):
                kinds = sorted(k for _, k in by_name[wanted])
                resolution.add(
                    row, "AMBIGUOUS", f"{wanted} is declared as {kinds}; the edge cannot say which"
                )
            else:
                resolution.add(row, "UNDECLARED", f"no package named {wanted} is declared here")
        edges[here] = targets
    if not resolution.rows:
        resolution.add(
            "(no edge)", "ok", f"{len(packages)} package(s), none declaring a dependency"
        )
    resolution_ok = resolution.report()

    cycles = Table("The dependency graph is acyclic")
    state: dict[tuple[str, str], int] = defaultdict(int)  # 0 unseen, 1 on the stack, 2 done
    found: list[str] = []

    def walk(node: tuple[str, str], stack: list[tuple[str, str]]) -> None:
        if state[node] == 1:
            cycle = stack[stack.index(node) :] + [node]
            found.append(" -> ".join(labels.get(n, n[0]) for n in cycle))
            return
        if state[node] == 2:
            return
        state[node] = 1
        for nxt in sorted(edges.get(node, ())):
            walk(nxt, stack + [nxt])
        state[node] = 2

    for node in sorted(edges):
        walk(node, [node])

    if found:
        for cycle in found:
            cycles.add(cycle, "CYCLE", "an edge here has to go")
    else:
        depth_of: dict[tuple[str, str], int] = {}

        def depth(node: tuple[str, str]) -> int:
            if node not in depth_of:
                depth_of[node] = 1 + max((depth(n) for n in edges.get(node, ())), default=0)
            return depth_of[node]

        for node in sorted(edges, key=lambda n: labels.get(n, n[0])):
            cycles.add(labels.get(node, node[0]), "ok", f"depth {depth(node)}")
    cycles_ok = cycles.report()

    namespaces_ok = check_namespaces(packages, labels)
    seams_ok = check_seams(packages, labels)

    # Every report is called before the results are combined. `and`
    # short-circuits, so `a.report() and b.report()` never prints `b` once `a`
    # has failed — and a table that never printed is not empty and not failing,
    # it is absent. This file's own docstring says a report of only failures
    # cannot distinguish "checked and clean" from "never checked"; a
    # short-circuited table is the same defect one step further along.
    return all([workspaces_ok, graph_ok, resolution_ok, cycles_ok, namespaces_ok, seams_ok])


# ---------------------------------------------------------------------------
# namespaces and seams: the two things a manifest cannot say
# ---------------------------------------------------------------------------

def namespace_users(namespace: str) -> list[str]:
    """Every tracked `.cs` file that declares exactly this namespace.

    Both spellings the language allows are matched: the block form
    `namespace X { ... }` and the file-scoped form `namespace X;`. The pattern
    is anchored at both ends, because `ExtendedResearch.Interop.Tests` starts
    with `ExtendedResearch.Interop` and is a different namespace, owned by a
    different package or by none.
    """
    pattern = re.compile(rf"^\s*namespace\s+{re.escape(namespace)}\s*[;{{]?\s*$", re.M)
    found = []
    for path in tracked():
        if not path.endswith(".cs"):
            continue
        try:
            text = (ROOT / path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if pattern.search(text):
            found.append(path)
    return found


def check_namespaces(packages: list[dict], labels: dict[tuple[str, str], str]) -> bool:
    table = Table("Every declared namespace is used by a .cs file in this tree")
    declared_any = False
    for package in packages:
        namespaces = package.get("namespaces", [])
        if not namespaces:
            continue
        declared_any = True
        for namespace in namespaces:
            users = namespace_users(namespace)
            label = f"{labels[key_of(package)]} :: {namespace}"
            if users:
                table.add(label, "ok", f"{len(users)} file(s), e.g. {users[0]}")
            else:
                table.add(label, "UNUSED", "declared here and no .cs file declares it")
    if not declared_any:
        # Printed rather than skipped. A package set in which nothing declares a
        # namespace and a package set in which the field was forgotten look
        # identical from the outside, and only one of them is fine.
        dotnet = [p["name"] for p in packages if p["kind"] == "dotnet"]
        table.add(
            "(none declared)",
            "ok" if not dotnet else "UNDECLARED",
            f"dotnet package(s) here: {dotnet or 'none'}",
        )
    return table.report()


#: The seam kinds this schema knows. A kind outside this set is a finding
#: rather than a thing to ignore: an unknown kind is checked by nothing, and
#: a declaration checked by nothing reads exactly like a declaration that
#: passed.
SEAM_KINDS = ("path", "process")


def check_seams(packages: list[dict], labels: dict[tuple[str, str], str]) -> bool:
    table = Table("Every declared seam reaches outside the package that declares it")
    for package in packages:
        seams = package.get("seams", [])
        if not seams:
            table.add(labels[key_of(package)], "ok", "no seam declared")
            continue
        for index, seam in enumerate(seams):
            label = f"{labels[key_of(package)]}[{index}]"
            kind = seam.get("kind")
            target = seam.get("target")
            reason = (seam.get("reason") or "").strip()
            if kind not in SEAM_KINDS:
                table.add(label, "KIND", f"{kind!r} is not one of {list(SEAM_KINDS)}")
                continue
            if not target:
                table.add(label, "TARGET", "a seam with no target declares nothing")
                continue
            if not reason:
                table.add(label, "REASON", f"{target!r} is declared with no reason to review")
                continue
            if kind == "process":
                # Nothing about a child process is checkable from the
                # declaration alone; the reason is the review surface, and it
                # is checked above. Stated here so the row is not read as
                # evidence the invocation was found.
                table.add(label, "ok", f"process {target!r}; not looked for in the source")
                continue
            inside = ROOT / package["path"]
            resolved = (inside / target).resolve()
            if resolved == inside.resolve() or inside.resolve() in resolved.parents:
                table.add(label, "INSIDE", f"{target!r} resolves inside {package['path']}")
            else:
                try:
                    shown = resolved.relative_to(ROOT).as_posix()
                except ValueError:
                    shown = resolved.as_posix()
                table.add(label, "ok", f"path {shown}; {reason.splitlines()[0]}")
    return table.report()


# ---------------------------------------------------------------------------
# duplication
# ---------------------------------------------------------------------------

BINARY = {".png", ".jpg", ".jpeg", ".gif", ".ico", ".wasm", ".node", ".nupkg", ".whl", ".tgz"}


def digest(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data[:8192]:
        return None
    return hashlib.sha256(data.replace(b"\r\n", b"\n").rstrip(b"\n")).hexdigest()


def check_duplication(declaration: dict) -> bool:
    copies = declaration.get("copy", [])
    forbidden = declaration["documents"].get("forbidden_paths", []) if "documents" in declaration else []
    forbidden = declaration.get("forbidden_paths", forbidden)

    paths = Table("No vendored tree")
    offenders = [p for p in tracked() if any(seg in f"{p}" for seg in forbidden)]
    if offenders:
        for path in offenders[:20]:
            paths.add(path, "VENDORED", "a tracked file under a forbidden path")
    else:
        for rule in forbidden:
            paths.add(rule, "ok", "no tracked file under it")
    paths_ok = paths.report()

    groups: dict[str, list[str]] = defaultdict(list)
    for path in tracked():
        if Path(path).suffix.lower() in BINARY:
            continue
        got = digest(ROOT / path)
        if got:
            groups[got].append(path)

    table = Table("Every byte-identical pair of files is a declared copy")
    duplicates = {h: members for h, members in groups.items() if len(members) > 1}

    def declared_for(members: list[str]) -> dict | None:
        for rule in copies:
            pattern = rule["pattern"]
            if all(
                fnmatch.fnmatch(m, pattern) or m == rule["source"] or Path(m).name == Path(pattern).name
                for m in members
            ):
                return rule
        return None

    if not duplicates:
        table.add("(no duplicates)", "ok", "every tracked text file is unique")
    for members in sorted(duplicates.values(), key=lambda m: m[0]):
        rule = declared_for(members)
        label = f"{Path(members[0]).name} x{len(members)}"
        if rule:
            table.add(label, "declared", rule["reason"].strip().split("\n")[0])
        else:
            table.add(label, "UNDECLARED", ", ".join(members[:4]))
    copies_ok = table.report()

    rules = Table("Every rule written more than once is registered with shared vectors")
    shared = declaration.get("shared_rule", [])
    if not shared:
        rules.add("(none registered)", "ok", "see PACKAGES.toml for the two that should be")
    for rule in shared:
        missing = [p for p in [rule["vectors"], *rule["implementations"]] if not (ROOT / p).exists()]
        if missing:
            rules.add(rule["name"], "MISSING", f"no such path: {missing}")
        else:
            rules.add(rule["name"], "ok", f"{len(rule['implementations'])} implementation(s)")

    # Reported before the results are combined; see `check_boundaries` for why.
    rules_ok = rules.report()
    return all([paths_ok, copies_ok, rules_ok])


# ---------------------------------------------------------------------------
# documents
# ---------------------------------------------------------------------------


def check_documents(declaration: dict, strict: bool) -> bool:
    spec = declaration["documents"]
    required = spec["required"]
    at_v1 = spec.get("at_v1", [])
    headings = spec.get("readme_headings", [])

    names = labels_for(declaration["package"])
    table = Table("Every published package carries the standard set")
    for package in declaration["package"]:
        if package.get("fixture") or package["kind"] == "tooling":
            continue
        path = ROOT / package["path"]
        label = names[key_of(package)]
        missing = [d for d in required if not (path / d).is_file()]
        pending = [d for d in at_v1 if not (path / d).is_file()]
        if missing:
            table.add(label, "MISSING", f"no {', '.join(missing)}")
        elif pending:
            table.add(label, "ok", f"{len(required)} present; pending at v1: {', '.join(pending)}")
        else:
            table.add(label, "ok", f"{len(required) + len(at_v1)} present")
    documents_ok = table.report()

    order = Table("README heading order matches the standard (soft until migrated)")
    for package in declaration["package"]:
        if package.get("fixture"):
            continue
        readme = ROOT / package["path"] / "README.md"
        if not readme.is_file():
            continue
        found = [
            m.group(1).strip()
            for m in re.finditer(r"^##\s+(.+?)\s*$", readme.read_text(encoding="utf-8"), re.M)
        ]
        wanted = [h for h in headings if h in found]
        if wanted == [h for h in found if h in headings] and len(wanted) == len(headings):
            order.add(names[key_of(package)], "ok", "every standard heading, in order")
        else:
            absent = [h for h in headings if h not in found]
            detail = f"missing {absent}" if absent else "out of order"
            order.add(names[key_of(package)], "SOFT", detail)

    order_ok = order.report(soft=set() if strict else {"SOFT"})

    # Decision records are per package, numbered locally, so that a package
    # carries the history explaining its shape and two packages never collide.
    # Repository-level records — about how packages relate — live at the root.
    records = Table("Decision records are numbered uniquely within their package")
    labels = labels_for(declaration["package"])
    seen_any = False
    for package in [*declaration["package"], {"name": "(repository)", "kind": "", "path": "."}]:
        directory = ROOT / package["path"] / "docs" / "decisions"
        if not directory.is_dir():
            continue
        seen_any = True
        label = labels.get(key_of(package), package["name"])
        numbers: dict[str, list[str]] = defaultdict(list)
        malformed: list[str] = []
        for record in sorted(directory.glob("*.md")):
            # An index is not a record. `README.md` is exempted by name rather
            # than by loosening the pattern, because the pattern is what stops a
            # record being filed under a number nobody can cite — widening it to
            # admit `README.md` would admit every other unnumbered file too, and
            # a record with no number is exactly the thing this row refuses.
            if record.name == "README.md":
                continue
            match = re.match(r"^(\d{4})-[a-z0-9]+(-[a-z0-9]+)*\.md$", record.name)
            if match:
                numbers[match.group(1)].append(record.name)
            else:
                malformed.append(record.name)
        collisions = {n: f for n, f in numbers.items() if len(f) > 1}
        if malformed:
            records.add(label, "MALFORMED", f"not NNNN-kebab-case.md: {malformed[:3]}")
        elif collisions:
            records.add(label, "COLLISION", f"number reused: {sorted(collisions)}")
        else:
            records.add(label, "ok", f"{len(numbers)} record(s), numbers unique")
    if not seen_any:
        records.add("(none)", "ok", "no package carries decision records yet")

    # Reported before the results are combined; see `check_boundaries` for why.
    records_ok = records.report()
    return all([documents_ok, order_ok, records_ok])


# ---------------------------------------------------------------------------
# citations
# ---------------------------------------------------------------------------

#: The repositories in this organisation. A citation is qualified when it names
#: one of these, and a word that is not one of them is not a qualification —
#: which is the point. `supersedes decision NNNN` has a word in front of the
#: number and is exactly as unresolvable as `decision NNNN` on its own, so a
#: check that accepted any preceding token would pass the lines it exists to
#: find.
#:
#: `docs/docs.tools/tier_vocabulary.py` carries these same names for a
#: different question — which tier may name which. Two lists, one fact: a sixth
#: repository changes both, and nothing here notices if only one changes.
KNOWN_REPOSITORIES = ("foundation", "ranvier", "ca3", "eres", "plugins")

#: Where records live, relative to a package root or to the repository root.
RECORD_DIRECTORY = "docs/decisions/"

#: A citation written as a path: any depth of directory, then the record
#: directory's last segment, then the number and slug. The prefix is captured
#: whole so that a repository name anywhere in it counts — `../<sibling>/docs/`
#: and a GitHub blob URL both name the repository, just not in the first
#: segment.
CITATION_PATH = re.compile(
    r"(?P<prefix>(?:[A-Za-z0-9_.-]+/)*)decisions/(?P<number>\d{4})-(?P<slug>[A-Za-z0-9._-]*)"
)

#: A citation written in prose. The optional leading token is what makes the
#: difference between a qualified citation and a bare one, so it is captured
#: even when it turns out to be an ordinary English word: the row says which
#: word was found, which is what tells a writer what to replace.
CITATION_PROSE = re.compile(
    r"(?:(?P<repository>[A-Za-z][A-Za-z0-9_-]*)(?:'s)?[ \t]+)?"
    r"[Dd]ecisions?[ \t]+(?:records?[ \t]+)?(?P<number>\d{4})"
)


def records_by_number() -> dict[str, list[str]]:
    """Every decision record in the tree, by its four-digit number.

    Numbers are unique within a package and not across them — `check.py
    documents` is what holds that — so one number can have several records. A
    prose citation resolves against all of them, which is a limit and not an
    oversight: `<repository> decision NNNN` does not say which package's record
    set it means, and inventing an answer would be worse than naming the
    ambiguity.
    """
    found: dict[str, list[str]] = defaultdict(list)
    for path in tracked():
        match = re.search(r"(?:^|/)docs/decisions/(\d{4})-[^/]*\.md$", path)
        if match:
            found[match.group(1)].append(path)
    return found


def citations_in(line: str) -> list[tuple[str, str | None, str, str | None]]:
    """Every citation on one line, as (text, repository, number, path).

    `repository` is None when nothing in front of the citation names one.
    `path` is the repository-relative path a path-shaped citation points at,
    and None for a prose one.
    """
    found: list[tuple[str, str | None, str, str | None]] = []
    for match in CITATION_PATH.finditer(line):
        segments = [s for s in match.group("prefix").split("/") if s and s not in {".", ".."}]
        named = [i for i, s in enumerate(segments) if s.lower() in KNOWN_REPOSITORIES]
        tail = f"decisions/{match.group('number')}-{match.group('slug')}"
        if named:
            after = segments[named[-1] + 1 :]
            # A citation written as a web URL names the repository and then
            # says how to browse it — `blob/main`, `tree/v1`, GitLab's `-/`.
            # Those segments are the host's, not the tree's, and leaving them
            # in makes a correctly qualified link resolve to nothing.
            while after and after[0] in {"blob", "tree", "raw", "blame", "-"}:
                after = after[2:] if after[0] != "-" else after[1:]
            relative = "/".join([*after, tail])
            owner = segments[named[-1]].lower()
            found.append((match.group(0), owner, match.group("number"), relative))
        else:
            found.append((match.group(0), None, match.group("number"), "/".join([*segments, tail])))
    for match in CITATION_PROSE.finditer(line):
        word = (match.group("repository") or "").lower()
        named_repository = word if word in KNOWN_REPOSITORIES else None
        found.append((match.group(0), named_repository, match.group("number"), None))
    return found


def check_citations(repository: str) -> bool:
    identity = Table("The repository this check resolves citations against")
    if repository not in KNOWN_REPOSITORIES:
        # The first floor, and the one the others are variations of. An unknown
        # name means no citation can be recognised as naming this repository,
        # so every one of them is filed as a sibling's and never resolved —
        # which reads, from the outside, exactly like a clean tree.
        identity.add(repository, "UNKNOWN", f"not one of {list(KNOWN_REPOSITORIES)}; pass --repo")
        identity.report()
        return False
    identity.add(repository, "ok", "a citation naming it is resolved against this tree")
    identity_ok = identity.report()

    records = records_by_number()
    scanned = [p for p in tracked() if RECORD_DIRECTORY not in p]

    table = Table(
        f"Every decision citation names its repository "
        f"({len(scanned)} file(s) scanned; the {len(tracked()) - len(scanned)} record(s) are not)"
    )
    refused = Table("Every file in the corpus could be read")
    refused_any = False

    for path in scanned:
        try:
            data = (ROOT / path).read_bytes()
        except OSError as error:
            refused.add(path, "REFUSED", f"{error}")
            refused_any = True
            continue
        if b"\0" in data[:8192]:
            hits = citations_in(data.decode("latin-1"))
            if hits:
                table.add(
                    path,
                    "BINARY",
                    f"{len(hits)} citation(s) in a file this check will not parse, "
                    f"first {hits[0][0]!r}; qualify it in the source it was generated from",
                )
            continue
        for number, line in enumerate(data.decode("utf-8", errors="replace").splitlines(), 1):
            for text, named, record, relative in citations_in(line):
                label = f"{path}:{number}"
                if named is None:
                    table.add(label, "BARE", f"{text!r} names no repository")
                elif named != repository:
                    table.add(label, "ok", f"{text!r} names {named}, checked where it lives")
                elif relative is not None:
                    if (ROOT / relative).is_file():
                        table.add(label, "ok", f"{text!r} resolves to {relative}")
                    else:
                        table.add(label, "MISSING", f"{text!r} has no file at {relative}")
                elif records.get(record):
                    table.add(label, "ok", f"{text!r} resolves to {', '.join(records[record])}")
                else:
                    table.add(
                        label,
                        "MISSING",
                        f"{text!r} names {repository} and no record {record} exists",
                    )

    if not scanned:
        # A wrong root, or a target that is not a git repository. Scanning
        # nothing and finding nothing to fix produce the same empty table.
        table.add("(nothing scanned)", "EMPTY", "no tracked file outside the record directory")
    elif not table.rows:
        table.add("(no citation)", "ok", f"{len(scanned)} file(s) carry none")

    table_ok = table.report()
    refused_ok = True
    if refused_any:
        refused_ok = refused.report()
    return all([identity_ok, table_ok, refused_ok])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "check", choices=["boundaries", "duplication", "documents", "citations", "all"]
    )
    parser.add_argument("--strict", action="store_true", help="soft findings fail too")
    parser.add_argument(
        "--repo",
        default=None,
        help=(
            "the repository's own name, used to decide which citations this "
            "tree can resolve. Defaults to the `repository` key in "
            "PACKAGES.toml, then to the root's directory name — which is wrong "
            "in a worktree, which is why the key exists."
        ),
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help=(
            "the repository to check; defaults to this script's own. "
            "It must carry its own ecosystem/PACKAGES.toml, since a declaration "
            "describes one repository's packages and never another's."
        ),
    )
    args = parser.parse_args()

    if args.root is not None and not set_root(args.root):
        print(f"no ecosystem/PACKAGES.toml under {ROOT}", file=sys.stderr)
        print("a repository declares its own packages; copy the format from foundation's", file=sys.stderr)
        return 2

    declaration = load()
    repository = args.repo or declaration.get("repository") or ROOT.name
    results = []
    if args.check in {"boundaries", "all"}:
        results.append(check_boundaries(declaration))
    if args.check in {"duplication", "all"}:
        results.append(check_duplication(declaration))
    if args.check in {"documents", "all"}:
        results.append(check_documents(declaration, args.strict))
    if args.check in {"citations", "all"}:
        results.append(check_citations(repository))

    if all(results):
        print("ecosystem: every declared boundary holds")
        return 0
    print("ecosystem: findings above")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
