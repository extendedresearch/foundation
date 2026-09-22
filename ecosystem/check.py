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
from pathlib import Path

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
    cycles_ok = cycles.report()

    # Each result is computed before the `and`, because `and` short-circuits
    # and a short-circuited table is a table that never printed. A check whose
    # later rows disappear as soon as an earlier one fails cannot tell a reader
    # whether those rows were clean or were skipped.
    namespaces_ok = check_namespaces(packages)
    seams_ok = check_seams(packages)
    return graph_ok and cycles_ok and namespaces_ok and seams_ok


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


def check_namespaces(packages: list[dict]) -> bool:
    table = Table("Every declared namespace is used by a .cs file in this tree")
    declared_any = False
    for package in packages:
        namespaces = package.get("namespaces", [])
        if not namespaces:
            continue
        declared_any = True
        for namespace in namespaces:
            users = namespace_users(namespace)
            label = f"{package['name']} :: {namespace}"
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


def check_seams(packages: list[dict]) -> bool:
    table = Table("Every declared seam reaches outside the package that declares it")
    for package in packages:
        seams = package.get("seams", [])
        if not seams:
            table.add(package["name"], "ok", "no seam declared")
            continue
        for index, seam in enumerate(seams):
            label = f"{package['name']}[{index}]"
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
    return identity_ok and table_ok and refused_ok


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
