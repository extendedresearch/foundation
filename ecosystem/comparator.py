#!/usr/bin/env python3
"""What the source imports, compared against what PACKAGES.toml allows.

    python ecosystem/comparator.py                       # fails: no extractor exists
    python ecosystem/comparator.py --root ../elsewhere   # any repository
    python ecosystem/comparator.py --edges FILE          # rules, against a handmade form

`ecosystem/check.py boundaries` reads manifests. A manifest says what a package
*may* depend on; it does not say what the source actually imports, and until the
repositories merge that gap is invisible because repository separation is doing
the work for free — a package cannot import from a sibling it cannot see. On the
day the trees merge, the only thing between a package and an import across a
boundary is this.

The design is four extractors and one comparator, per
`docs/specs/boundary-enforcement-plan.md` §2. Each language already has a tool
that resolves its own imports, so the language knowledge lives in an extractor
and the rule is written once, here. A fifth language later is a fifth extractor
and no change to the rule.

**This file is the comparator and the intermediate form. No extractor exists
yet**, which is why running it fails.

# The intermediate form

One JSON object per line, one line per resolved import:

    {"package": "extendedresearch-abi", "target": "extendedresearch-status",
     "kind": "normal", "language": "rust", "source": "crates/abi/src/lib.rs:12"}

`package` is the declared package the importing file belongs to. `target` is
what the import resolved to, after the language's own resolution rules — a
sibling package's name when it is one, and the third-party package name when it
is not. `language` selects the extractor that produced the line. `source` is
`path:line`, so a finding names a line rather than a package.

**`kind` is one of `normal`, `dev`, `test`, `build`**, and it exists because a
test crossing a boundary is a different finding from a shipped module doing it.
Collapsing them would mean either failing every test fixture or excusing every
shipped import that happened to sit in a file named like a test.

# The rules

**1. A language with no extractor fails the run.** This is the rule the others
are written underneath. A comparator that reports "0 findings" over a language
it never looked at is indistinguishable from one that looked and found nothing,
and the second claim is the one a reader takes away. Today that is every
language, so the run fails — honestly, and loudly, with the language named.

**2. Every edge names a declared package.** An edge whose `package` is not in
PACKAGES.toml came from a tree the declaration does not describe, and no rule
below it can mean anything.

**3. Every edge's `kind` is one of the four.** An unrecognised kind would fall
through every branch below and be counted as checked.

**4. A `normal` edge to a sibling must appear in that package's `depends`.**
This is the rule the whole file exists for.

**5. A `dev`, `test` or `build` edge to a sibling is reported separately.**
`depends` is resolved from normal dependencies only, so there is no field that
declares a development edge to a sibling today. The finding names the shape
rather than guessing at a schema: the field is worth adding the first time an
extractor produces one of these, and not before, because a field invented now
would be designed against no example.

**6. An edge to something that is not a sibling must be declared third-party** —
in `external` for a `normal` edge, and in `external` or `external_dev` for the
others.

# What this does not check

**One direction only: source to declaration.** A declared dependency that
nothing imports is not reported here, because a package may depend on another to
re-export it and never name it in an import. The manifest side of that question
is already both-directions in `check.py boundaries`, which fails a declared edge
with no manifest entry behind it.

**Nothing about how an import was written.** Whether a specifier reached a
package's public entry point or a path the package never exported is the
TypeScript extractor's question, and it is answered against that package's
`exports` map rather than against `dependencies`. The comparator sees only what
the extractor resolved.

**Nothing dynamic.** A name built at run time and passed to an importing
function is invisible to every extractor in this design, in every language.

Standard library only; `tomllib` is why the floor is Python 3.11.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check
from check import Table

#: The kinds an extractor may emit. A test crossing a boundary is a different
#: finding from a shipped module doing it, and `build` is separate again because
#: a build script runs on the machine doing the building and reaches things no
#: consumer ever sees.
KINDS = ("normal", "dev", "test", "build")

#: Which extractor answers for a package's `kind` in PACKAGES.toml. `tooling` is
#: mapped rather than excluded: a tooling package's source is Python, and an
#: import across a boundary there is as real as one anywhere else. Excluding it
#: would be the "finding nothing is not passing" failure written into the map.
LANGUAGE_OF_KIND = {
    "rust": "rust",
    "npm": "typescript",
    "python": "python",
    "dotnet": "dotnet",
    "tooling": "python",
}

#: language -> the extractor that emits the intermediate form for it.
#:
#: **Empty, deliberately.** The plan builds these in order of value per line:
#: TypeScript first, because a deep import satisfies a manifest while crossing a
#: boundary; then Python; then C#, which needs the `namespaces` field; then
#: Rust, last, because an undeclared Rust dependency does not compile.
EXTRACTORS: dict[str, str] = {}


@dataclass(frozen=True)
class Edge:
    """One resolved import, as an extractor emits it."""

    package: str
    target: str
    kind: str
    language: str
    source: str

    @classmethod
    def from_json(cls, raw: dict) -> Edge:
        missing = [f for f in ("package", "target", "kind", "language", "source") if f not in raw]
        if missing:
            raise ValueError(f"edge is missing {missing}: {raw}")
        return cls(
            package=str(raw["package"]),
            target=str(raw["target"]),
            kind=str(raw["kind"]),
            language=str(raw["language"]),
            source=str(raw["source"]),
        )

    def as_json(self) -> str:
        return json.dumps(
            {
                "package": self.package,
                "target": self.target,
                "kind": self.kind,
                "language": self.language,
                "source": self.source,
            },
            sort_keys=True,
        )


def read_edges(path: Path) -> list[Edge]:
    edges = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        try:
            edges.append(Edge.from_json(json.loads(line)))
        except (ValueError, json.JSONDecodeError) as error:
            raise SystemExit(f"{path}:{number}: {error}") from error
    return edges


def check_coverage(declaration: dict) -> bool:
    """Rule 1. Every language in the declaration has an extractor, or the run fails."""
    table = Table("Every language in this repository has an extractor")
    wanted: dict[str, list[str]] = {}
    for package in declaration["package"]:
        language = LANGUAGE_OF_KIND.get(package["kind"])
        if language is None:
            table.add(package["name"], "KIND", f"{package['kind']!r} maps to no language")
            continue
        wanted.setdefault(language, []).append(package["name"])
    for language in sorted(wanted):
        packages = ", ".join(sorted(wanted[language]))
        if language in EXTRACTORS:
            table.add(language, "ok", f"{EXTRACTORS[language]}; {len(wanted[language])} package(s)")
        else:
            table.add(language, "MISSING", f"no extractor for {language}; {packages} unchecked")
    if not wanted:
        table.add("(no language)", "EMPTY", "no package in the declaration maps to one")
    return table.report()


def compare(declaration: dict, edges: list[Edge]) -> bool:
    """Rules 2 to 6. Every edge, against what the declaration allows.

    Packages are keyed by `(name, language)` rather than by name. A name is not
    unique — one repository declares a Rust crate and a Python distribution
    that are both correctly called `ca3` — and keying on the name alone would
    check an edge against the wrong package's `depends` and print `ok`. The
    edge carries its language, so the pair is available without guessing.
    """
    packages = {(p["name"], LANGUAGE_OF_KIND.get(p["kind"])): p for p in declaration["package"]}
    by_name: dict[str, list[dict]] = defaultdict(list)
    for package in declaration["package"]:
        by_name[package["name"]].append(package)

    def find(name: str, language: str) -> dict | None:
        """The package this name means, in this language, or uniquely."""
        if (name, language) in packages:
            return packages[(name, language)]
        return by_name[name][0] if len(by_name[name]) == 1 else None

    table = Table(f"Every resolved import is a declared one ({len(edges)} edge(s))")
    if not edges:
        table.add("(no edge)", "ok", "nothing was extracted; see the coverage table above")
        return table.report()

    for edge in sorted(edges, key=lambda e: (e.package, e.target, e.source)):
        label = f"{edge.source} -> {edge.target}"
        package = find(edge.package, edge.language)
        if package is None:
            detail = (
                f"{edge.package!r} is declared for several languages and none is {edge.language}"
                if by_name[edge.package]
                else f"{edge.package!r} is not a package in this declaration"
            )
            table.add(label, "UNKNOWN", detail)
            continue
        if edge.kind not in KINDS:
            table.add(label, "KIND", f"{edge.kind!r} is not one of {list(KINDS)}")
            continue

        shipped = edge.kind == "normal"
        if by_name[edge.target]:
            if edge.target in set(package.get("depends", [])):
                table.add(label, "ok", f"{edge.kind}; declared in {edge.package}'s depends")
            elif shipped:
                table.add(label, "UNDECLARED", f"a sibling not in {edge.package}'s depends")
            else:
                table.add(
                    label,
                    "DEV-EDGE",
                    f"a {edge.kind} import of a sibling; no field declares one today",
                )
        else:
            allowed = set(package.get("external", []))
            if not shipped:
                allowed |= set(package.get("external_dev", []))
            if edge.target in allowed:
                table.add(label, "ok", f"{edge.kind}; declared third-party")
            else:
                table.add(label, "UNDECLARED", f"third-party and not declared by {edge.package}")
    return table.report()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="the repository to check; defaults to this script's own",
    )
    parser.add_argument(
        "--edges",
        type=Path,
        default=None,
        help=(
            "read the intermediate form from a file instead of running "
            "extractors. This is a development input: a handmade file is a "
            "claim about the source, not evidence from it, and the run says so."
        ),
    )
    args = parser.parse_args()

    if args.root is not None and not check.set_root(args.root):
        print(f"no ecosystem/PACKAGES.toml under {args.root.resolve()}", file=sys.stderr)
        return 2

    declaration = check.load()
    covered = check_coverage(declaration)

    if args.edges is not None:
        edges = read_edges(args.edges)
        print(f"edges: {len(edges)} read from {args.edges}, which no extractor produced\n")
    else:
        # There is nothing to run. The coverage table above has already said so
        # per language; this is the same fact in the exit code.
        edges = []

    compared = compare(declaration, edges)

    if covered and compared:
        print("comparator: every resolved import is a declared one")
        return 0
    print("comparator: findings above")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
