#!/usr/bin/env python3
"""Refuse text that crosses an architecture tier, in whichever repository it is.

    python docs/docs.tools/check-tier.py                      # this checkout
    python docs/docs.tools/check-tier.py --root ../ranvier    # another one
    python docs/docs.tools/check-tier.py --root ../eres --repo eres

The organisation is three tiers deep, and the tiers are the reason any of these
repositories can be built, tested or released on their own:

- **Tier 1, foundation.** The C ABI conventions and the shared binding glue. It
  may *name* the tier-2 repositories — "napi 3, the series ranvier pins" is a
  fact about a sibling and costs nothing — and it may not *reference* them. A
  symbol path, a foreign constant, a foreign interop symbol, a file path into a
  sibling, a numbered section of a sibling's specification, or a manifest entry
  are all references, and every one of them makes the bottom of the stack
  unbuildable without something above it.

- **Tier 2, ranvier and ca3 and eres.** Each may name and reference foundation
  and itself, and nothing else. Here there is no naming/referencing
  distinction, and the absence is the point: three sibling libraries that
  describe each other in prose converge, one paragraph at a time, into one
  library with three checkouts. A bare mention of a sibling, of the tier-3
  repository, of the ecosystem repository, of an application, of a plugin
  package, or of a piece of plugin hardware used as an example, is a finding.

- **Tier 3, plugins.** May reference tiers 1 and 2 freely, and must never
  reference a specific application.

# The tier-3 rules are oblique because the tier-3 violations are

No application is named literally in the plugins tree today, and a checker
built on names alone would therefore report it clean. What is actually there is
a set of descriptions that identify an application without naming one: "the two
applications", "both applications", "each application's", "the external
consumer", "the consumer's", "a dashboard consumer", "a node-graph consumer",
"app-level", "renderer bundle", "the main process", the name of a desktop
runtime, and paths into an application checkout.

Those are worse than a name, not better. A name is greppable and a reader knows
what it costs. "The two applications" is a count that was true on the day it
was written, is load-bearing in the sentence around it, and survives every
sweep anyone runs — which is how a shared library ends up with a fixed idea of
who consumes it and no way to notice.

# Why the corpus is git's list and not a directory walk

`git ls-files -t --cached --others --exclude-standard -z` is the set that
matters: tracked files, plus untracked files git would add. An ignored file
cannot reach history, so failing a build over it is a false positive that gets
the check switched off; a file that stops being ignored appears here the moment
it does.

A directory walk answers a different question — what is on this disk — and
those answers differ by whole trees: build output, dependency caches, and
second checkouts nested inside the first. Pruning them by name is a losing
game, because the list of names is never finished.

**An untracked file under a dot-directory is dropped; a tracked one never is.**
A dot-directory git tracks nothing in holds editor state and tool caches.
`.github/` holds tracked files and is therefore read, without anyone adding it
to an allowlist — and so is any dot-directory on the day its first file is
committed. The workflow is exactly where a cross-tier reference hides, so this
is not a detail.

# Every file type, and no extension allowlist

The pruning here is by *kind* — lockfiles, known binary suffixes, and a NUL
test on the first 8 KiB — and never by "which extensions are worth reading".

An extension allowlist is the standard way to build this check and it has a
standard failure, which is that stale cross-tier references do not live in
code. They live in prose and in doc comments: a module header explaining what
used to be true, a decision record quoting a sibling's rule, a comment
describing what a consumer does with the output. A list of extensions is
written by someone thinking about source files, and the material that matters
is in the file types nobody thought to add.

Lockfiles are pruned for the opposite reason: a resolved dependency graph names
every transitive package, is machine-written, and cannot be edited in response
to a finding. Prune it or turn the check off; there is no third option.

# The marker, and why the ratchet is over markers rather than over findings

Some text has to cross a tier in order to explain something — a decision record
that says why the boundary was drawn where it was cannot always avoid the words
on the other side of it. The escape hatch is a per-line marker (spelled in
`tier_vocabulary.MARKER`) followed by at least `MIN_REASON` characters of
prose. It drops findings on **that line** and no other. A bare marker with no
reason is itself a finding, because a marker whose reason is "-" is a comment
character with extra steps.

**A fenced code block is scanned like any other prose.** A sibling's name
inside an example command is a reference the same as one in a sentence — it is
copied, run, and pasted into other documents more often than the prose around
it — so nothing here treats a fence as exempt. What differs inside a fence is
where the marker can *go*: a shell continuation ends in a backslash, and a
trailing comment after a backslash breaks the command. There is nowhere on that
line to put it.

So there are exactly two exemptions that reach a line the marker is not on, and
both exist for the same reason: **a marker that broke the thing it annotates
would be a worse outcome than the violation it excuses.**

**A continued command.** A comment-only line carrying a valid marker, inside a
fence, excuses the contiguous command that immediately follows it — from the
next line up to and including the first line that does not end in a backslash,
and no further. Six lines in the tree need it today, all of them recorded
commands in dated plan and baseline documents, one of them a three-line command
whose offending text runs across all three.

**A fence in a format with no line-comment syntax.** A `json` block cannot
carry a comment on any line at all: an attempt to put one there makes the block
invalid JSON. So a marker line immediately above the fence — separated only by
blank lines, since a marker written as an HTML comment needs one to render —
excuses every line inside that block, bounded by the closing delimiter. Only an
info string in `COMMENTLESS_FENCES` is eligible; an unknown or empty one is
not, because an unlabelled fence would otherwise become a region one line above
it could excuse wholesale. One record needs this today.

The bound is the whole of the design in both cases, and in both it is a
delimiter already in the file rather than a distance. "A marker anywhere
nearby" would let one comment excuse an entire region, and a fence is exactly
the kind of region that grows a line at a time with nobody re-reading the
comment above it. Neither exemption reaches a second command or a second block,
neither survives the closing delimiter, and neither reaches backwards. Every
use is printed by site in the matrix, under its own count, so growth in them is
visible without going looking.

**No directory is ever excluded, decision records included.** Excluding the
records is the obvious move and it is wrong twice: it blinds the check to the
staleness that accumulates fastest in exactly the place people copy old text
into, and it means a record written tomorrow can cross a tier freely on the day
it is written. A record that needs the words says so on the line that needs
them, and pays one marker for it.

**The ratchet is over the number of markers, not over the number of findings.**
A findings ratchet is satisfied by removing a crossing from a CI comment while
adding one to a normative specification: the number goes down, the tree gets
worse, and nothing in the report can tell the difference — the two events are
the same integer. A marker ratchet makes a different and checkable claim: *the
number of lines this repository has excused only falls.* Adding a marker is a
diff that says so, in a file, with a reason on the line, under review.

# Finding nothing is not passing

**A checker of this shape fails by finding nothing to check**, and every way it
can do that looks identical to a clean tree from the outside. Five floors, each
failing loudly:

1. **No rules for this repository.** An unknown name gets an empty rule set,
   which passes every file. This is the failure the other four are variations
   of.
2. **A rule's canary stopped matching.** Every rule carries a string it must
   match, and all of them are tested before the walk. A regex edited into
   matching nothing does not error and does not warn; it reports a clean tree
   with total confidence, and it is indistinguishable from a rule that is
   simply satisfied. The canary is what makes those two claims different.
3. **The operating system refused a file.** Not a decision the checker made — a
   binary skip is a decision, a lockfile skip is a decision, this is the
   absence of one. Reported before the empty-walk floor, because a run in which
   every file was refused also scans nothing, and the empty-walk message points
   at a broken path, which is the wrong diagnosis.
4. **Zero files scanned.** A wrong root, an over-broad prune, or a target that
   is not a git repository.
5. **The marker ceiling was exceeded.** The ratchet above.

# The matrix prints on every run, pass or fail

The report is a full matrix — files scanned, files clean, every rule with its
hit count, its affected-file count and a verdict, and the marker count against
the ceiling — and it prints when the run passes.

A failures-only report has a property worth stating out loud: **it shrinks to
nothing when the walk breaks, and nothing is also what a clean tree looks
like.** The five floors above exist because that shape of failure is the one
this class of tool actually has, and printing the matrix is the same argument
carried into the output. A reader who sees "1,204 files, 14 rules, all clean"
has been told something. A reader who sees an empty report has been told
nothing, twice.

Two rules print in the matrix and never fail the run, marked ADVISORY: the two
repository names that are also ordinary English nouns. `tier_vocabulary.py`
carries the measurements behind that decision. An advisory rule is not a
skipped rule — it is counted and printed beside the rules that can fail, so the
choice stays visible and can be revisited against a number.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tier_vocabulary import (  # noqa: E402
    MARKER,
    MARKER_CEILING,
    MIN_REASON,
    SELF_EXEMPT,
    Rule,
    rules_for,
    tier_of,
)

ROOT = Path(__file__).resolve().parents[2]

#: Path components that are never a repository's own content: build output,
#: dependencies, caches, and generated trees. Dropped wherever they sit.
#: Dot-directories are a separate rule, applied in `files_in_tree`, because
#: some of them are the repository's own content and some are not.
PRUNED = {
    ".git", "target", "node_modules", "dist", "build", "out", "pkg",
    "vendor", "third_party", "generated", "__pycache__", ".venv",
    ".pytest_cache",
}

#: Machine-written dependency graphs. They name every transitive package,
#: nobody edits them in response to a finding, and a checker that fails on one
#: is a checker that gets switched off.
LOCKFILES = {
    "Cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml",
    "uv.lock", "poetry.lock",
}

#: A first pass only. The NUL test below is what actually decides, because an
#: unknown binary format has an extension nobody listed.
BINARY_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".icns", ".bmp",
    ".pdf", ".zip", ".gz", ".xz", ".bz2", ".7z", ".tar",
    ".wasm", ".so", ".dylib", ".dll", ".exe", ".lib", ".a", ".o", ".obj",
    ".pdb", ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".mp4", ".mp3", ".wav", ".ogg", ".webm", ".flac", ".bin", ".dat",
}

#: Large enough that no prose file in these trees approaches it, small enough
#: that a stray fixture does not get read into memory. A text file over this is
#: reported rather than skipped, for the same reason the floors exist.
MAX_BYTES = 16 * 1024 * 1024


class Unreadable(Exception):
    """The operating system refused this file, and nothing here chose that.

    Three of the four things that happen to a file in the walk are decisions: a
    binary is skipped because matching prose rules against it is meaningless, a
    lockfile is skipped because it is machine-written, and a text file is read.
    This is the fourth, and it is not a decision — the check did not run and the
    checker does not know what the file held. Returning `None` here would make
    it indistinguishable from the binary skip, which is the third floor.
    """

    def __init__(self, path: Path, cause: OSError) -> None:
        self.path = path
        self.cause = cause
        super().__init__(f"{cause.strerror or cause} (errno {cause.errno})")


class NotACheckout(Exception):
    """`--root` is not a git checkout, or git could not be run at all."""

    def __init__(self, root: Path, cause: BaseException) -> None:
        self.root = root
        super().__init__(f"{root}: {cause}")


def openable(path: Path) -> Path:
    """`path`, with the Windows long-path prefix when it is needed.

    Several trees here are checked out under a deep root, and decision-record
    filenames in them run past 160 characters. The sum crosses Windows'
    259-character ceiling, at which point the file cannot be opened at all —
    and a file that cannot be opened is a file no rule covers. The prefix costs
    nothing elsewhere: it is applied only on Windows, only to absolute paths,
    and only when the path is long enough to be at risk.
    """
    if sys.platform != "win32":
        return path
    text = str(path)
    if text.startswith("\\\\?\\") or not path.is_absolute() or len(text) < 240:
        return path
    return Path("\\\\?\\" + text)


def files_in_tree(root: Path):
    """Every file that is in the repository, or could be added to it.

    See the module docstring for why this is git's list rather than a walk, and
    for the dot-directory rule. `-t` tags each name, and `?` is git's tag for a
    file it does not track.

    Every name git returns is yielded, including one that will not resolve to a
    readable file. Filtering here on `is_file()` would be a silent drop: it
    swallows the `OSError` a refused path raises and answers `False`, so the
    file leaves the walk before anything has decided anything about it.
    """
    try:
        done = subprocess.run(
            ["git", "ls-files", "-t", "--cached", "--others",
             "--exclude-standard", "-z"],
            cwd=root, capture_output=True, text=True, check=True,
        )
    except (OSError, subprocess.CalledProcessError) as cause:
        # A root that is not a checkout, or a git that is not installed.
        # Raised rather than swallowed: an empty list here would arrive at the
        # fourth floor with a message about pruning, which sends the reader to
        # the wrong place. `NotACheckout` names what actually happened.
        raise NotACheckout(root, cause) from cause
    for entry in done.stdout.split("\0"):
        if not entry:
            continue
        tag, name = entry[:1], entry[2:]
        parts = Path(name).parts
        if not parts:
            continue
        if any(part in PRUNED for part in parts):
            continue
        if parts[-1] in LOCKFILES:
            continue
        if tag == "?" and any(part.startswith(".") for part in parts[:-1]):
            continue
        yield name


def size_of(path: Path) -> int:
    try:
        return openable(path).stat().st_size
    except OSError as cause:
        raise Unreadable(path, cause) from cause


def readable(path: Path) -> str | None:
    """The file's text, or `None` when this checker decided not to scan it.

    `None` means *a decision was made*; `Unreadable` means nobody made one. The
    third floor rests entirely on that distinction.
    """
    if path.suffix.lower() in BINARY_SUFFIXES:
        return None
    if size_of(path) > MAX_BYTES:
        return None
    try:
        raw = openable(path).read_bytes()
    except OSError as cause:
        raise Unreadable(path, cause) from cause
    if b"\x00" in raw[:8192]:
        return None
    return raw.decode("utf-8", errors="replace")


def marker_state(line: str) -> tuple[bool, bool]:
    """`(carries a marker, the reason is long enough)` for one line.

    The reason is everything after the marker to the end of the line, stripped.
    Two answers rather than one, because the bare marker is a finding of its own
    and must not also silence the findings on its line — an excuse with no
    reason on it excuses nothing.
    """
    at = line.find(MARKER)
    if at < 0:
        return False, False
    reason = line[at + len(MARKER):].strip()
    return True, len(reason) >= MIN_REASON


class Tally:
    """Per-rule counts, kept for the matrix rather than for the verdict."""

    def __init__(self, rule: Rule) -> None:
        self.rule = rule
        self.hits = 0
        self.files: set[str] = set()
        self.excused = 0

    def record(self, where: str, count: int) -> None:
        self.hits += count
        self.files.add(where)


#: The opening or closing delimiter of a fenced code block, with the info
#: string an opening delimiter may carry.
FENCE = re.compile(r"^\s*(?:```|~~~)\s*([A-Za-z0-9_+.-]*)")

#: A line that is nothing but a comment, in the syntaxes these fences hold.
#: The marker has to sit on one of these for the fenced-command exemption to
#: apply, because that is the placement that leaves the command runnable.
COMMENT_ONLY = re.compile(r"^\s*(?:#|//|<!--|--\s)")

#: Fence info strings naming a format with no line-comment syntax, where the
#: marker cannot go inside the block at all. `json` is the case that exists.
#:
#: **An unknown or empty info string is not eligible**, deliberately. The
#: eligible set is an allowlist because the failure modes are not symmetric:
#: leaving a comment-bearing format out costs one marker per line, and letting
#: an unlabelled fence in would turn every unlabelled block in the tree into
#: something one line above it can excuse wholesale.
COMMENTLESS_FENCES = {"json"}


def fence_blocks(lines: list[str]) -> list[tuple[int, int, str]]:
    """`(opening index, closing index, info string)` for each fenced block.

    A block left unclosed at the end of the file closes at the last line, so
    an unbalanced fence cannot make the rest of the file unreachable.
    """
    blocks: list[tuple[int, int, str]] = []
    opened: int | None = None
    info = ""
    for index, line in enumerate(lines):
        match = FENCE.match(line)
        if not match:
            continue
        if opened is None:
            opened, info = index, match.group(1).lower()
        else:
            blocks.append((opened, index, info))
            opened, info = None, ""
    if opened is not None:
        blocks.append((opened, len(lines), info))
    return blocks


def fence_map(lines: list[str]) -> list[bool]:
    """Which lines sit *inside* a fenced code block, delimiters excluded."""
    inside = [False] * len(lines)
    for opening, closing, _ in fence_blocks(lines):
        for at in range(opening + 1, min(closing, len(lines))):
            inside[at] = True
    return inside


def excuse_map(lines: list[str]) -> tuple[list[bool], dict[int, tuple[str, int]]]:
    """Which lines a marker excuses, and which markers reach beyond their own.

    Returns `(excused per line, {marker line index: (kind, lines covered)})`.

    **The rule is per line, and there are exactly two exceptions to it.**
    A marker with a reason excuses its own line and nothing else. That is the
    whole discipline: the cost of crossing a tier is one marker per line, which
    is what stops a single excuse from covering a region nobody re-reads. Both
    exceptions exist for the same reason, which is that the marker cannot
    physically go on the line — **a marker that broke the thing it annotates
    would be a worse outcome than the violation it excuses** — and both are
    bounded by a delimiter that is already in the file rather than by a
    distance.

    **`command`: a continued command inside a fence.** A shell continuation
    ends in a backslash, and appending a comment after a backslash breaks the
    command. So a comment-only line carrying a valid marker, inside a fence,
    excuses the contiguous command that immediately follows it — from the next
    line up to and including the first line that does not end in a backslash.
    Six lines in the tree need this today, one of them a three-line command
    whose offending text runs across all three.

    **`block`: a fence in a format with no line-comment syntax.** A `json`
    block cannot carry a comment on any line; an attempt to put one there makes
    the block invalid JSON, which is a worse defect than the reference. So a
    marker line immediately above the fence — separated only by blank lines,
    because a marker written as an HTML comment needs one to render — excuses
    every line *inside* that block, bounded by the closing delimiter. Only an
    info string in `COMMENTLESS_FENCES` is eligible; an unknown or empty one is
    not.

    Neither exemption reaches backwards, neither survives the closing
    delimiter, and neither can reach a second block or a second command. The
    bound is the point: "a marker anywhere nearby" would let one comment excuse
    a whole region, and a fence is exactly the kind of region that grows a line
    at a time with nobody re-reading the comment above it. Every use is printed
    by site in the matrix, under its own count, so growth is visible without
    anyone going looking for it.
    """
    inside = fence_map(lines)
    blocks = {opening: (closing, info) for opening, closing, info in fence_blocks(lines)}
    excused = [False] * len(lines)
    covers: dict[int, tuple[str, int]] = {}

    for index, line in enumerate(lines):
        marked, reasoned = marker_state(line)
        if not (marked and reasoned):
            continue
        excused[index] = True

        # `command`: inside a fence, on a comment-only line.
        if inside[index] and COMMENT_ONLY.match(line):
            span = 0
            at = index + 1
            while at < len(lines) and inside[at]:
                excused[at] = True
                span += 1
                if not lines[at].rstrip().endswith("\\"):
                    break
                at += 1
            if span:
                covers[index] = ("command", span)
            continue

        # `block`: above a fence whose format cannot carry a comment. Blank
        # lines between the marker and the fence are skipped and nothing else
        # is, so a paragraph of prose in between makes the marker ineligible.
        at = index + 1
        while at < len(lines) and not lines[at].strip():
            at += 1
        if at not in blocks:
            continue
        closing, info = blocks[at]
        if info not in COMMENTLESS_FENCES:
            continue
        span = 0
        for within in range(at + 1, min(closing, len(lines))):
            excused[within] = True
            span += 1
        if span:
            covers[index] = (f"{info} block", span)

    return excused, covers


def scan_file(
    name: str, text: str, rules: list[Rule], tallies: dict[str, Tally]
) -> tuple[list[str], list[str], list[str], list[str]]:
    """One file against every rule, line by line.

    Returns `(findings, excused lines, bare-marker lines, fenced-command
    markers)`. `tallies` is updated in place.

    Factored out of `main` so the marker's behaviours can be exercised without
    a checkout to put files in: a marked line with a reason drops its findings
    and is counted once against the ratchet, a marked line without one is a
    finding and silences nothing, an unmarked line behaves normally, and a
    marker above a fenced command covers that command and stops.

    A rule matched twice on one line counts twice and reports once — the report
    names the line, and the line is the unit a person edits.
    """
    lines = text.splitlines()
    excused, covers = excuse_map(lines)

    findings: list[str] = []
    marked_lines: list[str] = []
    bare_markers: list[str] = []
    fenced_markers: list[str] = []

    for index, line in enumerate(lines):
        number = index + 1
        marked, reasoned = marker_state(line)
        if marked:
            if reasoned:
                marked_lines.append(f"{name}:{number}")
                if index in covers:
                    kind, span = covers[index]
                    fenced_markers.append(
                        f"{name}:{number} ({kind}, {span} line(s))"
                    )
            else:
                bare_markers.append(f"{name}:{number}")

        for rule in rules:
            if rule.paths is not None and not rule.paths.search(name):
                continue
            found = rule.pattern.findall(line)
            if not found:
                continue
            tally = tallies[rule.name]
            # An advisory rule is never excused, because it never accuses: its
            # count is a measurement of the tree, and a marker that changed it
            # would make the measurement a function of the excuses.
            if excused[index] and not rule.advisory:
                tally.excused += len(found)
                continue
            tally.record(name, len(found))
            if not rule.advisory:
                matched = rule.pattern.search(line)
                findings.append(
                    f"{name}:{number}: [{rule.name}] "
                    f"{matched.group(0)!r} in {line.strip()[:110]!r}"
                )

    return findings, marked_lines, bare_markers, fenced_markers


def check_canaries(rules: list[Rule]) -> list[str]:
    """The second floor. Every rule must still match every string it names.

    Every canary of every rule, not one per rule: a rule is usually an
    alternation, and a canary that passes on one branch says nothing about the
    others. A rule with no canaries at all is itself a failure, because an
    empty loop passes and that is the shape of vacuity this floor exists for.
    """
    broken: list[str] = []
    for rule in rules:
        if not rule.canaries:
            broken.append(f"{rule.name}: declares no canary at all")
            continue
        broken.extend(
            f"{rule.name}: its canary {canary!r} no longer matches"
            for canary in rule.canaries
            if not rule.pattern.search(canary)
        )
    return broken


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument(
        "--root", default=str(ROOT),
        help="the checkout to scan; defaults to the one holding this script",
    )
    parser.add_argument(
        "--repo",
        help="the repository's tier name; defaults to the root's directory name",
    )
    args = parser.parse_args()

    root = Path(args.root).resolve()
    repo = args.repo or root.name
    tier = tier_of(repo)
    rules = rules_for(repo)

    # The first floor. An unknown repository gets an empty rule set, which
    # passes every file in the tree — the exact shape of a clean run. Every
    # other floor here is a variation on this one.
    if not rules:
        print(
            f"tier check FAILED: no rules are defined for '{repo}'.\n"
            f"  Root: {root}\n"
            "  A repository with no rules passes everything, which reads\n"
            "  exactly like a clean tree. Either the --repo name is wrong, or\n"
            "  the repository is new and belongs in a tier in\n"
            "  docs/docs.tools/tier_vocabulary.py.",
            file=sys.stderr,
        )
        return 1

    # The second floor, before a single file is read. See the docstring.
    broken = check_canaries(rules)
    if broken:
        print(
            f"tier check FAILED: {len(broken)} rule(s) match nothing.\n"
            "  A rule that cannot match its own canary reports a clean tree\n"
            "  with total confidence, and looks no different from a rule that\n"
            "  is satisfied. Fix the pattern or fix the canary, in the same\n"
            "  diff, and say which one was wrong:\n"
            + "".join(f"    {one}\n" for one in broken),
            file=sys.stderr,
        )
        return 1

    ceiling = MARKER_CEILING.get(repo)
    if ceiling is None:
        print(
            f"tier check FAILED: '{repo}' has rules but no marker ceiling.\n"
            "  An absent ceiling is an unbounded one. Add the repository to\n"
            "  MARKER_CEILING in docs/docs.tools/tier_vocabulary.py.",
            file=sys.stderr,
        )
        return 1

    tallies = {rule.name: Tally(rule) for rule in rules}
    findings: list[str] = []
    marker_lines: list[str] = []
    bare_markers: list[str] = []
    fenced_markers: list[str] = []
    refused: list[str] = []
    skipped_large: list[str] = []
    scanned = 0
    clean = 0

    try:
        corpus = list(files_in_tree(root))
    except NotACheckout as cause:
        # Named rather than counted. An empty corpus would land on the fourth
        # floor, whose message is about pruning and a wrong root — true, but
        # not the truth here, and a message that sends a reader to the wrong
        # place costs more than no message.
        print(
            f"tier check FAILED: {cause}\n"
            "  --root must be a git checkout: the corpus is git's list of what\n"
            "  is tracked or addable, not a walk of the directory.",
            file=sys.stderr,
        )
        return 1

    for name in corpus:
        path = root / name

        # Exact path, never a glob. The denylist is the one file that has to
        # contain what it forbids, and the exemption covers its markers as well
        # as its findings: it spells the marker in order to define it.
        if name == SELF_EXEMPT:
            continue

        try:
            if (
                path.suffix.lower() not in BINARY_SUFFIXES
                and size_of(path) > MAX_BYTES
            ):
                skipped_large.append(f"{name} ({size_of(path):,} bytes)")
            text = readable(path)
        except Unreadable as refusal:
            refused.append(f"{name}: {refusal}")
            continue

        if text is None:
            continue

        scanned += 1
        here, marked, bare, fenced = scan_file(name, text, rules, tallies)
        findings.extend(here)
        marker_lines.extend(marked)
        bare_markers.extend(bare)
        fenced_markers.extend(fenced)
        if not here:
            clean += 1

    # The third floor, checked before the fourth deliberately. A run in which
    # every file was refused also scans nothing, and the fourth floor's message
    # points at a wrong root — the wrong diagnosis, and the one that sends a
    # reader looking in the wrong place.
    if refused:
        print(
            f"tier check FAILED: {len(refused)} file(s) the operating system\n"
            "  would not open. A file the checker cannot read is a file no rule\n"
            "  covers, and skipping it silently would leave it out of the\n"
            "  scanned count and exit 0. The long-path prefix has already been\n"
            "  tried, so a path over 259 characters is not the explanation; a\n"
            "  lock, a permission, or a file removed after git listed it are:\n"
            + "".join(f"    {one}\n" for one in refused),
            file=sys.stderr,
        )
        return 1

    # The fourth floor.
    if scanned == 0:
        print(
            f"tier check FAILED: no files were scanned under {root}.\n"
            "  A run that reads nothing cannot tell a clean tree from a broken\n"
            "  walk, so it is a failure rather than a pass. Check that --root\n"
            "  is a git checkout and that the prune list is not swallowing it.",
            file=sys.stderr,
        )
        return 1

    if skipped_large:
        print(
            "tier check FAILED: text file(s) too large to scan.\n"
            "  A silent skip reads exactly like a clean pass. Split the file,\n"
            "  or raise MAX_BYTES deliberately and say why:\n"
            + "".join(f"    {one}\n" for one in skipped_large),
            file=sys.stderr,
        )
        return 1

    # ---------------------------------------------------------------- matrix
    # Printed on every run. See the docstring: a failures-only report shrinks
    # to nothing when the walk breaks, and nothing is what a clean tree looks
    # like too.
    enforced = [rule for rule in rules if not rule.advisory]
    advisory = [rule for rule in rules if rule.advisory]
    violated = [r for r in enforced if tallies[r.name].hits]

    print(f"tier check - {repo} (tier {tier})")
    print(f"  root            {root}")
    print(f"  files scanned   {scanned}")
    print(f"  files clean     {clean}")
    print(f"  files with hits {scanned - clean}")
    print()
    print(f"  {'rule':<26} {'hits':>6} {'files':>6} {'excused':>8}  verdict")
    print(f"  {'-' * 26} {'-' * 6} {'-' * 6} {'-' * 8}  {'-' * 8}")
    for rule in enforced:
        tally = tallies[rule.name]
        verdict = "VIOLATED" if tally.hits else "clean"
        print(
            f"  {rule.name:<26} {tally.hits:>6} {len(tally.files):>6} "
            f"{tally.excused:>8}  {verdict}"
        )
    for rule in advisory:
        tally = tallies[rule.name]
        print(
            f"  {rule.name:<26} {tally.hits:>6} {len(tally.files):>6} "
            f"{'-':>8}  ADVISORY (never fails)"
        )
    print()
    print(
        f"  markers         {len(marker_lines)} of {ceiling} allowed"
        + (
            f" ({len(fenced_markers)} reaching into a fence)"
            if fenced_markers
            else ""
        )
        + ("  EXCEEDED" if len(marker_lines) > ceiling else "")
    )
    # Printed by site, not just by count. The fenced-command exemption is the
    # one place a marker reaches a line it is not on, so its use is listed
    # every run and its growth is visible without anyone going looking.
    for one in fenced_markers:
        print(f"                    {one}")
    if bare_markers:
        print(f"  markers with no reason  {len(bare_markers)}")
    print()

    failed = False

    if bare_markers:
        print(
            f"tier check FAILED: {len(bare_markers)} marker(s) with no reason.\n"
            f"  The marker takes at least {MIN_REASON} characters of prose after\n"
            "  it, on the same line. A marker with no reason is a comment\n"
            "  character with extra steps, and it silences nothing:\n"
            + "".join(f"    {one}\n" for one in bare_markers),
            file=sys.stderr,
        )
        failed = True

    # The fifth floor.
    if len(marker_lines) > ceiling:
        print(
            f"tier check FAILED: {len(marker_lines)} excused line(s) against a\n"
            f"  ceiling of {ceiling}. The ratchet is over markers rather than\n"
            "  over findings, and the claim it makes is that the number of\n"
            "  lines this repository has excused only falls. Raising it is a\n"
            "  diff that says so:\n"
            + "".join(f"    {one}\n" for one in marker_lines),
            file=sys.stderr,
        )
        failed = True

    if findings:
        print(
            f"tier check FAILED: {len(findings)} finding(s) across "
            f"{len(violated)} rule(s).",
            file=sys.stderr,
        )
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        failed = True

    if failed:
        return 1

    print(
        f"tier check passed - {scanned} file(s) against {len(enforced)} rule(s)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
