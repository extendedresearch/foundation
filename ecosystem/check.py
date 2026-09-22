#!/usr/bin/env python3
"""The checks that hold this repository's package boundaries.

    python ecosystem/check.py boundaries     # the dependency graph is what PACKAGES.toml says
    python ecosystem/check.py duplication    # no copy that is not declared, no vendored tree
    python ecosystem/check.py documents      # every package carries the standard set
    python ecosystem/check.py all            # all three
    python ecosystem/check.py all --strict   # soft findings fail too

Every check prints every row it compared, passing rows included. A report of
only failures cannot distinguish "checked and clean" from "never checked", and
a declaration nobody checks is a comment.

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
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECLARATION = ROOT / "ecosystem" / "PACKAGES.toml"


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


# ---------------------------------------------------------------------------
# boundaries
# ---------------------------------------------------------------------------


def rust_edges() -> dict[str, tuple[set[str], set[str]]]:
    """Each Rust package's (intra-repository, third-party) normal dependencies.

    Read from `cargo metadata --no-deps`, which is the manifests as cargo
    resolves them rather than as a regex reads them. Development and build
    dependencies are excluded: they do not reach a consumer.
    """
    raw = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    meta = json.loads(raw)
    local = {p["name"] for p in meta["packages"]}
    edges: dict[str, tuple[set[str], set[str]]] = {}
    for package in meta["packages"]:
        intra: set[str] = set()
        external: set[str] = set()
        for dependency in package["dependencies"]:
            if dependency["kind"] is not None:  # dev or build
                continue
            (intra if dependency["name"] in local else external).add(dependency["name"])
        edges[package["name"]] = (intra, external)
    return edges


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

    table = Table("Dependencies: declared and resolved agree, in both directions")
    rust = rust_edges()

    for package in packages:
        name, kind = package["name"], package["kind"]
        path = ROOT / package["path"]
        want_intra = set(package.get("depends", []))
        want_external = set(package.get("external", []))

        if kind == "rust":
            if name not in rust:
                table.add(name, "MISSING", "declared here, and cargo metadata does not list it")
                continue
            have_intra, have_external = rust[name]
        elif kind == "npm":
            have_external, have_dev = npm_edges(path)
            have_intra = have_external & declared_names
            have_external -= have_intra
            want_dev = set(package.get("external_dev", []))
            if have_dev != want_dev:
                table.add(f"{name} (dev)", "DRIFT", f"resolved {sorted(have_dev)}, declared {sorted(want_dev)}")
            else:
                table.add(f"{name} (dev)", "ok", f"{sorted(have_dev) or 'none'}")
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
            table.add(name, "DRIFT", "; ".join(problems))
        else:
            inside = sorted(want_intra) or "nothing"
            outside = sorted(want_external) or "nothing"
            table.add(name, "ok", f"depends on {inside}; third-party {outside}")

    graph_ok = table.report()

    cycles = Table("The dependency graph is acyclic")
    edges = {p["name"]: set(p.get("depends", [])) for p in packages}
    state: dict[str, int] = defaultdict(int)  # 0 unseen, 1 on the stack, 2 done
    found: list[str] = []

    def walk(node: str, stack: list[str]) -> None:
        if state[node] == 1:
            found.append(" -> ".join(stack[stack.index(node) :] + [node]))
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
        depth_of: dict[str, int] = {}

        def depth(node: str) -> int:
            if node not in depth_of:
                depth_of[node] = 1 + max((depth(n) for n in edges.get(node, ())), default=0)
            return depth_of[node]

        for node in sorted(edges):
            cycles.add(node, "ok", f"depth {depth(node)}")
    return graph_ok and cycles.report()


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

    return paths_ok and copies_ok and rules.report()


# ---------------------------------------------------------------------------
# documents
# ---------------------------------------------------------------------------


def check_documents(declaration: dict, strict: bool) -> bool:
    spec = declaration["documents"]
    required = spec["required"]
    at_v1 = spec.get("at_v1", [])
    headings = spec.get("readme_headings", [])

    table = Table("Every published package carries the standard set")
    for package in declaration["package"]:
        if package.get("fixture") or package["kind"] == "tooling":
            continue
        path = ROOT / package["path"]
        missing = [d for d in required if not (path / d).is_file()]
        pending = [d for d in at_v1 if not (path / d).is_file()]
        if missing:
            table.add(package["name"], "MISSING", f"no {', '.join(missing)}")
        elif pending:
            table.add(package["name"], "ok", f"{len(required)} present; pending at v1: {', '.join(pending)}")
        else:
            table.add(package["name"], "ok", f"{len(required) + len(at_v1)} present")
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
            order.add(package["name"], "ok", "every standard heading, in order")
        else:
            absent = [h for h in headings if h not in found]
            order.add(package["name"], "SOFT", f"missing {absent}" if absent else "out of order")

    order_ok = order.report(soft=set() if strict else {"SOFT"})

    # Decision records are per package, numbered locally, so that a package
    # carries the history explaining its shape and two packages never collide.
    # Repository-level records — about how packages relate — live at the root.
    records = Table("Decision records are numbered uniquely within their package")
    seen_any = False
    for package in [*declaration["package"], {"name": "(repository)", "path": "."}]:
        directory = ROOT / package["path"] / "docs" / "decisions"
        if not directory.is_dir():
            continue
        seen_any = True
        numbers: dict[str, list[str]] = defaultdict(list)
        malformed: list[str] = []
        for record in sorted(directory.glob("*.md")):
            match = re.match(r"^(\d{4})-[a-z0-9]+(-[a-z0-9]+)*\.md$", record.name)
            if match:
                numbers[match.group(1)].append(record.name)
            else:
                malformed.append(record.name)
        collisions = {n: f for n, f in numbers.items() if len(f) > 1}
        if malformed:
            records.add(package["name"], "MALFORMED", f"not NNNN-kebab-case.md: {malformed[:3]}")
        elif collisions:
            records.add(package["name"], "COLLISION", f"number reused: {sorted(collisions)}")
        else:
            records.add(package["name"], "ok", f"{len(numbers)} record(s), numbers unique")
    if not seen_any:
        records.add("(none)", "ok", "no package carries decision records yet")

    return documents_ok and order_ok and records.report()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("check", choices=["boundaries", "duplication", "documents", "all"])
    parser.add_argument("--strict", action="store_true", help="soft findings fail too")
    args = parser.parse_args()

    declaration = load()
    results = []
    if args.check in {"boundaries", "all"}:
        results.append(check_boundaries(declaration))
    if args.check in {"duplication", "all"}:
        results.append(check_duplication(declaration))
    if args.check in {"documents", "all"}:
        results.append(check_documents(declaration, args.strict))

    if all(results):
        print("ecosystem: every declared boundary holds")
        return 0
    print("ecosystem: findings above")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
