#!/usr/bin/env python3
"""Given a diff, the packages that must be tested.

    python ecosystem/affected.py                          # names, one per line
    python ecosystem/affected.py --base origin/main       # the ref to diff against
    python ecosystem/affected.py --json                   # for a workflow to consume
    python ecosystem/affected.py --explain                # every package and why

Gate item 3 of `docs/decisions/0002-the-ecosystem-lives-in-one-repository.md`.
Four repositories are about to merge into this one. A monorepo that runs every
test on every change gets slow, and a slow suite is one people learn to skip —
at which point the ecosystem-wide testing that justified the merge is gone. So
this exists before the merge rather than after it.

Standard library only; `tomllib` is why the floor is Python 3.11.

# The declaration is the source of truth, and this script does not re-derive it

The package set and every intra-repository edge are read from
`ecosystem/PACKAGES.toml` and from nothing else. `ecosystem/check.py boundaries`
already compares that declaration against what the manifests resolve, in both
directions, so a second derivation here would be a second thing to keep right
and a second thing to be wrong. What this script must not do is invent an edge
the declaration does not carry: an edge that exists in a manifest and not in the
declaration is `check.py`'s failure to report, not this script's to work around.

`Table` below is a copy of the four-line printer in `ecosystem/check.py` rather
than an import of it. The original's verdicts are pass/fail and its report
returns whether anything failed; here every row is an outcome and none of them
is a failure, so the shared part is too small to be worth coupling two scripts
over. Stated rather than left to be noticed.

# Every row is printed, selected and not selected alike

`--explain` prints the full matrix: every changed file with its owning package,
then every package with whether it was selected and why. A report of only the
selected set cannot distinguish "not affected" from "not considered", which is
the distinction the whole thing rests on — the answer is only trustworthy if it
can be seen to have considered everything.

# Selection, and why it is wider than it strictly has to be

Getting this wrong by testing too little is the failure that matters: a skipped
suite is a defect that ships, while a redundant suite costs a few runner
minutes. So every judgement call below widens. Each is a deliberate cost.

**A changed file selects its owning package.** Ownership is by path prefix, on
whole path components — `dotnet/Interop.PackageTest/x.cs` is not owned by the
package at `dotnet/Interop.Package`, though one string is a prefix of the other.
The longest matching declared path wins, so a package nested inside another's
directory would still take its own files.

**Reverse dependencies are closed over transitively.** A change to
`extendedresearch-status` selects everything that depends on it, and everything
that depends on those, to a fixed point.

**A file owned by no package selects everything.** `.github/`, `scripts/`,
`docs/`, the root manifest, anything at the root. A workflow, a shared script or
a root `Cargo.toml` can change any package's result, and nothing in the
declaration records that. This is also the bucket that catches a package whose
sources live outside its declared `path` — `dotnet/Interop/*.cs` ship inside
`ExtendedResearch.Interop`, which is declared at `dotnet/Interop.Package`, so a
change to those sources selects everything rather than one package. Wider than
necessary, and correct.

**A file in a package declared `kind = "tooling"` selects everything.** `style/`
and `ecosystem/` hold the checks that run against every package. The declaration
records no edge from a package to the tooling that checks it — such an edge
would be a lie about the build graph — so nothing else would widen here.

**An unresolvable base ref selects everything.** `style/scripts/check-style.sh`
falls back to `HEAD~1` in this case, which is right for a style check and wrong
here: a wrong base narrows the diff and narrowing is the direction that loses
tests. A shallow clone, a fork, or a first push on a new branch all produce it.

**An empty diff selects everything.** Nothing genuinely changing is rare;
a base ref that resolves to `HEAD` is not, and the two are indistinguishable
from here. Selecting nothing on the more common cause is not a trade worth
making for a run that has nothing to test anyway.

**Renames are read as a delete and an add** (`--no-renames`), so a file moving
out of a package selects that package as well as the one it moved into.
Deletions count: `git diff --name-only` is left unfiltered, unlike
`check-style.sh`'s `--diff-filter=ACMR`, because a style check has nothing to
say about a file that is gone while a test suite very much does.

# What it cannot see

**Edges that exist only in a test harness.** `crates/napi-testaddon`'s Node test
loads `npm/binding-runtime/src/`, and `dotnet/Interop.Tests` runs against a
library built from `crates/abi-testlib`. Neither is a manifest dependency, so
neither is in the declaration, so neither is here. A change to
`@extendedresearch/binding-runtime` does not select `extendedresearch-napi-testaddon`.
Declaring those edges is the fix; inventing them in this script is not.

**Behavioural coupling with no edge at all.** Two packages that must agree on a
wire format, a status code's meaning or a generated header have no dependency
between them and will not select each other.

**Anything uncommitted.** The diff is `<base>...HEAD`, the same range
`check-style.sh` uses, so working-tree edits are invisible until committed.

**Whether a selected package's suite is the right suite.** This answers which
packages a change reaches, not what to run for each one.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECLARATION = ROOT / "ecosystem" / "PACKAGES.toml"

# The reasons a package can be selected, and the order they are considered in.
# The first that applies is the one reported, so a package that is both a direct
# hit and a reverse dependency reads as the direct hit.
GLOBAL = "global"
TOOLING = "tooling"
DIRECT = "direct"
REVERSE = "reverse"
NONE = "none"


class Table:
    """A named block of rows, printed whole.

    A copy of `ecosystem/check.py`'s printer, not an import of it; see the
    module docstring for why.
    """

    def __init__(self, title: str) -> None:
        self.title = title
        self.rows: list[tuple[str, str, str]] = []

    def add(self, what: str, verdict: str, detail: str = "") -> None:
        """Record one row."""
        self.rows.append((what, verdict, detail))

    def report(self) -> None:
        """Print every row, and a count of them."""
        print(self.title)
        if not self.rows:
            print("  (nothing to report)\n")
            return
        what_width = max(len(r[0]) for r in self.rows)
        verdict_width = max(len(r[1]) for r in self.rows)
        for what, verdict, detail in self.rows:
            print(f"  {verdict:<{verdict_width}}  {what:<{what_width}}  {detail}")
        print(f"  {len(self.rows)} row(s)\n")


def load(declaration_path: Path) -> dict:
    """Read the package declaration."""
    with declaration_path.open("rb") as handle:
        return tomllib.load(handle)


def owns(package_path: str, changed: str) -> bool:
    """Whether `changed` lies inside `package_path`, matched on whole components.

    `dotnet/Interop.Package` does not own `dotnet/Interop.PackageTest/x.cs`,
    though one string is a prefix of the other.
    """
    prefix = package_path.rstrip("/")
    return changed == prefix or changed.startswith(prefix + "/")


def changed_files(base: str, root: Path) -> tuple[list[str], str | None]:
    """Return the paths differing between `base` and HEAD, and why the base failed if it did.

    `<base>...HEAD` is the merge base, so a change is judged against where it
    branched rather than against whatever main has since become — the same range
    `style/scripts/check-style.sh` uses. Unfiltered and `--no-renames`: see the
    module docstring.
    """
    probe = subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", base],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if probe.returncode != 0:
        return [], f"base ref {base!r} is not in this checkout"

    diff = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", f"{base}...HEAD"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if diff.returncode != 0:
        detail = diff.stderr.strip().splitlines()
        return [], f"git diff against {base!r} failed: {detail[0] if detail else 'no message'}"
    return sorted(p for p in diff.stdout.split("\0") if p), None


def select(declaration: dict, changed: list[str], widen_all: tuple[str, str] | None) -> dict:
    """Decide which packages a change reaches, and record why for every package.

    Returns the whole matrix: `packages` in declaration order, each with
    `selected`, `reason` and `detail`, plus the file-to-package mapping and the
    global trigger if there was one.
    """
    packages = declaration["package"]
    names = [p["name"] for p in packages]
    known = set(names)
    paths = {p["name"]: p["path"] for p in packages}
    tooling = {p["name"] for p in packages if p.get("kind") == "tooling"}

    depends = {p["name"]: list(p.get("depends", [])) for p in packages}
    unknown = sorted(
        {f"{name} -> {dep}" for name, deps in depends.items() for dep in deps if dep not in known}
    )
    if unknown:
        raise SystemExit(
            "ecosystem/PACKAGES.toml declares a dependency on a package it does not "
            f"declare: {unknown}. The closure cannot be computed honestly against a "
            "declaration that names something absent."
        )

    # dependents[x] = every package with a declared edge to x.
    dependents: dict[str, list[str]] = {name: [] for name in names}
    for name, deps in depends.items():
        for dep in deps:
            dependents[dep].append(name)

    # Map every changed file to its owning package: the longest declared path
    # that contains it, on whole components.
    ownership: list[tuple[str, str | None]] = []
    for path in changed:
        owner = None
        for name in names:
            if owns(paths[name], path) and (owner is None or len(paths[name]) > len(paths[owner])):
                owner = name
        ownership.append((path, owner))

    reason: dict[str, str] = dict.fromkeys(names, NONE)
    detail: dict[str, str] = dict.fromkeys(names, "no changed file reaches it")

    # A global trigger selects everything and stops there: there is nothing more
    # specific to say once every package is in.
    global_trigger = widen_all
    if global_trigger is None:
        orphans = [path for path, owner in ownership if owner is None]
        hit_tooling = [
            (path, owner) for path, owner in ownership if owner is not None and owner in tooling
        ]
        if orphans:
            global_trigger = (GLOBAL, f"{orphans[0]} is owned by no package")
        elif hit_tooling:
            path, owner = hit_tooling[0]
            global_trigger = (
                TOOLING,
                f"{path} is in {owner}, which is tooling every package is checked by",
            )

    if global_trigger is not None:
        kind, message = global_trigger
        for name in names:
            reason[name] = kind
            detail[name] = message
        return {
            "base_widened_everything": message,
            "files": [{"path": p, "package": o} for p, o in ownership],
            "packages": [
                {"name": n, "selected": True, "reason": reason[n], "detail": detail[n]}
                for n in names
            ],
        }

    for path, owner in ownership:
        if owner is not None and reason[owner] != DIRECT:
            reason[owner] = DIRECT
            detail[owner] = f"changed file {path}"

    # Close over reverse dependencies to a fixed point, recording which package
    # pulled each one in — the first to reach it, which is the shortest path.
    frontier = [n for n in names if reason[n] == DIRECT]
    while frontier:
        nxt: list[str] = []
        for node in frontier:
            for consumer in dependents[node]:
                if reason[consumer] == NONE:
                    reason[consumer] = REVERSE
                    detail[consumer] = f"depends on {node}"
                    nxt.append(consumer)
        frontier = nxt

    return {
        "base_widened_everything": None,
        "files": [{"path": p, "package": o} for p, o in ownership],
        "packages": [
            {
                "name": n,
                "selected": reason[n] != NONE,
                "reason": reason[n],
                "detail": detail[n],
            }
            for n in names
        ],
    }


def explain(base: str, matrix: dict) -> None:
    """Print the full matrix: every changed file, then every package."""
    files = Table(f"Changed files against {base}, and the package each belongs to")
    for entry in matrix["files"]:
        owner = entry["package"]
        # ASCII only in printed output: a Windows console's default code page
        # cannot encode an em-dash, and a report that mojibakes is a report
        # somebody stops reading.
        files.add(
            entry["path"],
            "owned" if owner else "ORPHAN",
            owner or "no package - selects everything",
        )
    if not matrix["files"]:
        files.add("(no files differ)", "none", "selecting everything rather than nothing")
    files.report()

    packages = Table("Every package, selected or not, and why")
    phrase = {
        DIRECT: "direct hit",
        REVERSE: "reverse dependency",
        GLOBAL: "global file",
        TOOLING: "tooling change",
        NONE: "not affected",
    }
    for entry in matrix["packages"]:
        verdict = "selected" if entry["selected"] else "not selected"
        packages.add(entry["name"], verdict, f"{phrase[entry['reason']]}: {entry['detail']}")
    packages.report()

    chosen = sum(1 for e in matrix["packages"] if e["selected"])
    total = len(matrix["packages"])
    print(f"affected: {chosen} of {total} package(s) selected against {base}")


def main() -> int:
    """Parse the arguments, compute the affected set, print it."""
    parser = argparse.ArgumentParser(description="The packages a diff requires testing.")
    parser.add_argument(
        "--base",
        default="origin/main",
        help="the ref to diff against, as style/scripts/check-style.sh takes one",
    )
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    parser.add_argument(
        "--explain",
        action="store_true",
        help="the full matrix: every package, selected or not, and why",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help=(
            "the repository to read; defaults to this script's own. "
            "It must carry its own ecosystem/PACKAGES.toml, since a declaration "
            "describes one repository's packages and never another's."
        ),
    )
    args = parser.parse_args()

    root = args.root.resolve() if args.root is not None else ROOT
    declaration_path = root / "ecosystem" / "PACKAGES.toml"
    if not declaration_path.is_file():
        print(f"no ecosystem/PACKAGES.toml under {root}", file=sys.stderr)
        print(
            "a repository declares its own packages; copy the format from foundation's",
            file=sys.stderr,
        )
        return 2

    changed, base_problem = changed_files(args.base, root)

    widen_all: tuple[str, str] | None = None
    if base_problem is not None:
        widen_all = (
            GLOBAL,
            f"{base_problem}; selecting everything rather than trusting a narrower diff",
        )
    elif not changed:
        widen_all = (
            GLOBAL,
            f"no file differs from {args.base}, which is more often a base ref that "
            "resolved to HEAD than a change with nothing in it; selecting everything",
        )
    if widen_all is not None:
        print(f"::warning::{widen_all[1]}", file=sys.stderr)

    matrix = select(load(declaration_path), changed, widen_all)
    selected = [e["name"] for e in matrix["packages"] if e["selected"]]

    if args.json:
        payload: dict = {
            "base": args.base,
            "changed_file_count": len(changed),
            "selected": selected,
            "widened_everything": matrix["base_widened_everything"],
        }
        if args.explain:
            payload["files"] = matrix["files"]
            payload["packages"] = matrix["packages"]
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    if args.explain:
        explain(args.base, matrix)
        return 0

    for name in selected:
        print(name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
