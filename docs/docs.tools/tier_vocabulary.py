#!/usr/bin/env python3
"""The vocabulary the tier checker matches, and the one file exempt from it.

`check-tier.py` holds the machinery — the corpus, the floors, the report. This
file holds every string that machinery looks for, and it is the only file in
the tree that is allowed to contain them.

# Why the denylist is the one file that exempts itself

A checker of this shape has a structural problem: the list of forbidden text is
made of forbidden text. Scanning it reports a finding on every rule, every run,
for the rest of the repository's life — a permanent wall of noise that trains
everyone to read the report as "the usual ones plus maybe something real",
which is the same as not reading it.

**The exemption is by exact repository-relative path and never by glob.** A
glob is the wrong instrument here for a reason worth stating: `docs.tools/*.py`
would exempt every tool that ever lands beside this one, and `*vocabulary*`
would exempt a file someone names `app_vocabulary.py` next year. Neither
exemption would be *noticed*, because an exempt file produces no output at all.
An exact path exempts exactly one file and fails closed: rename this file and
the checker immediately reports it against every rule it declares, which is
loud, immediate, and correct.

The exemption covers the marker count as well as the findings. This file spells
the marker in order to define it, and a definition is not an excuse.

# Word boundaries are written out, not `\\b`

Every identifier rule here uses `(?<![A-Za-z0-9_])X(?![A-Za-z0-9])` rather than
`\\bX\\b`. Both halves differ from `\\b` and both differences were measured.

**The lookahead omits `_` deliberately.** `_` is a word character, so `\\beres\\b`
does not match `eres_stream_open` and `\\bca3\\b` does not match `CA3_ERR_NULL`
— the two spellings a foreign symbol is most likely to arrive in. Dropping `_`
from the lookahead is what makes an identifier prefix a match.

**The lookbehind keeps `_`, and keeps letters.** That is what stops `eres` from
matching inside `theres`, `spheres`, `adheres` and `interferes`. Measured on
this org's trees: in ranvier, a raw substring search for `eres` returns 17 hits
and the bounded pattern returns 12, so five of seventeen were English words. A
rule with a 29% false-positive rate on its first run does not survive contact
with a reviewer.

    git -C <repo> grep -oIE 'eres' | wc -l                       # raw
    git -C <repo> grep -oIE '(^|[^A-Za-z0-9_])eres([^A-Za-z0-9]|$)' | wc -l

# The two words that are matched only as repository names

`plugins` and `ecosystem` are ordinary English nouns as well as repository
names, and both are used as ordinary English nouns throughout these trees.
Banning the bare noun makes the report worthless in the specific way described
above.

**`plugins`.** A single protobuf container fixture in ca3
(`crates/cli/tests/fixtures/container.ca3`) carries the word 78 times inside
upstream text about protobuf code-generation plugins, which are not this
organisation's repository:

    git -C ca3 grep -c 'plugins'

That fixture happens to be dropped earlier by the NUL-byte test — it is a
binary container and the first 8 KiB contain NULs — so the 78 do not reach the
report today. The rule is still written this way, because a fixture regenerated
as text tomorrow puts them back, and because the same argument holds for the
prose files that do reach the report.

**`ecosystem`.** This one is not hypothetical and is not hiding in a fixture.
"every library in the ecosystem", "the boundary codes every library in the
ecosystem answers", "the conventions every C ABI in the ecosystem obeys" — the
bare noun appears 38 times in 29 files in ranvier, 10 in ca3, 16 in eres, and
essentially none of it names the repository. The repository is named in a
distinguishable way every time it is named: backticked, hyphenated into
`extendedresearch-ecosystem`, or followed by the word "repository".

So both words get a repository-form rule that fails the run, and a bare-noun
rule that is **advisory**: it is counted, printed in the matrix on every run,
and never fails anything. The count is there so the decision to treat the noun
as noise stays visible and can be revisited against a number rather than
against a memory.

    ADVISORY is not a skip. A skipped rule is invisible; an advisory rule is
    printed beside the rules that can fail, with its hits and its file count.

# `Muse`, `Neon` and `Mocopi`: two case-sensitive branches, and one gamble

    (?<![A-Za-z0-9_])(?:(?:Muse|Neon|Mocopi|mocopi)(?![a-z])
                      |(?:MUSE|NEON|MOCOPI)(?![A-Za-z0-9]))

**Capitalised**, with a lowercase-letter lookahead rather than the usual
alphanumeric one, so `Museum`, `amuse`, `muses` and `Neonatal` do not match
while `Muse-1A2B` and `Neon's` do.

**Shouted**, with the ordinary boundary, because that is how these names arrive
in fixture identifiers and a case-sensitive rule without this branch does not
see them. It was not seeing them: ca3 carried `serial: "NEON-0042"` and
`clock_domain: "device.NEON-0042"` in ten places across `crates/container`,
every one of them invisible to the capitalised branch while the package rule
was correctly flagging `pupil-labs-neon` three lines away. The two branches
take different lookaheads on purpose — `NEON-0042` has to match and `NEONATAL`
must not, and only `(?![A-Za-z0-9])` gives both.

Surveyed before the branch was added, over all five repositories:

    git -C <repo> grep -nIE '(^|[^A-Za-z0-9_])(MUSE|NEON|MOCOPI)([^a-z]|$)'

`MUSE` and `MOCOPI`: **zero hits anywhere**, so the acronym collision that
would make `MUSE` expensive is not populated today. `NEON`: the tier-2 hits are
all `device.NEON-0042` fixture identifiers, which is the case this branch
exists for.

**The residual risk, stated plainly: lowercase `neon` is a Rust crate name and
an ARM instruction set, capitalised `Neon` is an eye tracker, and only the case
separates them.** The crate half is live in these trees now — ranvier has
`neon.local` as an example hostname and `runtime.node("neon")` in tests, ca3
has `node: "neon"` and `kind: "pupil-labs-neon"` in fixtures. All are lowercase
and every one would be a false positive under a case-insensitive rule.

The ARM half was surveyed rather than assumed:

    git -C <repo> grep -nIE 'target_feature[^\\n]*neon|aarch64_feature_detected
                             |core::arch::aarch64|std::arch::aarch64'

**Zero hits in all five repositories.** It is also safe by construction where
it matters: the ARM feature string is lowercase by specification, so
`target_feature = "neon"` and `is_aarch64_feature_detected!("neon")` cannot
fire. What *would* fire is prose about it — a comment saying "the NEON path is
faster" — and there is no pattern that separates that from a tracker serial.
A repository that starts writing SIMD prose will need a marker per line or a
narrowing here, and this paragraph is the warning rather than a fix. The same
holds for an unrelated constant: plugins has `const NEON: &[u8]` naming an AAC
fixture, which would be a false positive if plugins were tier 2. It is tier 3,
where this rule does not apply, and that is luck rather than design.

`pupil-labs-neon` and `pupillabs-neon` are matched by the package rule instead,
which is case-insensitive because a package name is not ambiguous.

# What each rule carries, and why the canaries are plural

Every rule carries `canaries`: strings the rule must match, one per alternative
that matters. `check-tier.py` tests all of them before it reads a single file
and fails the run if any one stops matching.

This is the floor that matters most in a file like this one. A regex edited
into matching nothing does not error, does not warn, and does not look any
different from a rule that is simply satisfied — it reports a clean tree with
total confidence. A canary turns "this rule found nothing" into two
distinguishable claims: the rule ran and found nothing, or the rule is broken.

**Plural, because a rule is usually an alternation and one canary proves
nothing about the other branches.** The hardware rule above is the case that
forced it: a single canary on `Muse` passes while the shouted branch matches
nothing at all, which is precisely the failure that branch was added to fix.
Each branch that can independently break carries its own.
"""

import re
from dataclasses import dataclass

#: The per-line escape hatch, and the whole of it. A line carrying this marker
#: followed by at least `MIN_REASON` characters of prose drops every finding on
#: **that line** and nothing else.
#:
#: Spelled here rather than in `check-tier.py` on purpose: this file is exempt
#: from the scan by exact path, so the literal can appear. `check-tier.py`
#: builds every message that mentions the marker out of this constant, so the
#: checker's own source never carries a marker that would count against the
#: ceiling.
MARKER = "tier-historical:"

#: Characters of reason required after the marker. A bare marker is a finding
#: in its own right — see `check-tier.py`.
#:
#: Twelve is short enough that a real reason always clears it ("the 2026-08
#: rename", "quoted from the spec") and long enough that `ok`, `-`, `see above`
#: and `historical` do not. It is a speed bump, not a proof of anything: the
#: thing that actually holds the line is that the marker count is ratcheted.
MIN_REASON = 12

#: The one file exempt from the scan, as a repository-relative POSIX path.
#: Exact, never a glob. See the module docstring.
SELF_EXEMPT = "docs/docs.tools/tier_vocabulary.py"

#: Repository names by tier. The checker refuses to run against a name that is
#: not here, because inferring "no rules apply" from "I do not know this repo"
#: is the first anti-vacuity floor wearing a disguise.
TIER1 = ("foundation",)
TIER2 = ("ranvier", "ca3", "eres")
TIER3 = ("plugins",)

#: The ratchet, per repository. See `check-tier.py` for why the ratchet is over
#: markers rather than over findings.
MARKER_CEILING = {
    # Set on 2026-09-12, to the count each tree carried when the sweep that
    # introduced the marker finished. Every one of them is a line in a dated
    # record or a denylist that has to spell what it refuses; none is in a
    # specification, and `spec/` carries no marker in any repository.
    #
    # **A ceiling falls in the commit that lowers it and rises only in the
    # commit that adds the marker.** Raising one here without a marker beside
    # it in the same diff is the move this table exists to make visible.
    "foundation": 0,
    "ranvier": 58,
    "ca3": 1,
    "eres": 1,
    "plugins": 95,
}


def word(body: str) -> str:
    """`body` as a whole token, with the boundaries argued in the docstring.

    The lookahead omits `_` so an identifier prefix matches; the lookbehind
    keeps letters and digits so an English word ending in the token does not.
    """
    return rf"(?<![A-Za-z0-9_])(?:{body})(?![A-Za-z0-9])"


@dataclass(frozen=True)
class Rule:
    """One thing the checker refuses, and the proof that it still refuses it."""

    #: Stable identifier, printed in the matrix and in every finding.
    name: str
    #: What a reader should do about a hit, in one sentence.
    why: str
    #: Applied to each line of each scanned file.
    pattern: re.Pattern[str]
    #: Strings this rule must match, one per alternative that matters.
    #: Checked before the walk; any canary that stops matching fails the run.
    #: A tuple rather than a single string because a rule is usually an
    #: alternation, and a canary for one branch proves nothing about the
    #: others - an edit that broke the shouted spelling while leaving the
    #: capitalised one intact would pass a one-canary floor and report a clean
    #: tree with total confidence. See the docstring.
    canaries: tuple[str, ...]
    #: Printed in the matrix, never fails the run. Reserved for the two
    #: repository names that are also ordinary English nouns.
    advisory: bool = False
    #: When set, the rule applies only to files whose repository-relative path
    #: matches. Used by the manifest rule, where the same text is a dependency
    #: in one file and a sentence in another.
    paths: re.Pattern[str] | None = None


# --------------------------------------------------------------------------
#  Tier 1 — foundation may NAME the tier-2 repositories and may not REFERENCE
#  them. The line between the two is the whole of this section: "ranvier pins
#  napi 3" is a fact about a sibling and costs nothing; a symbol path, a
#  constant, a file path, a section citation or a manifest entry is a coupling
#  that makes foundation unbuildable without the sibling it names.
# --------------------------------------------------------------------------

_T2 = "ranvier|ca3|eres"
_T2_UPPER = "RANVIER|CA3|ERES"

#: Manifests, where a repository name on the left of an `=` or a `:` is a
#: dependency rather than a sentence.
MANIFEST_PATHS = re.compile(
    r"(?:^|/)(?:Cargo\.toml|package\.json|pyproject\.toml|[^/]+\.csproj)$"
)

#: Applications, matched by name wherever they appear. Case-insensitive: a
#: brand written in prose does not keep its capitals reliably, and unlike
#: `Neon` there is no lowercase homonym to protect.
_APPS = r"RayTx|VisualField|ca3-viewer|eres-ide|ranvier-flow"


def _tier1_rules() -> list[Rule]:
    return [
        Rule(
            name="t1-symbol-path",
            why="foundation names a symbol inside a tier-2 crate; foundation "
            "compiles without those crates and must keep doing so",
            pattern=re.compile(rf"(?<![A-Za-z0-9_])(?:{_T2})::[A-Za-z_]"),
            canaries=("ca3::BOUNDARY_CODES", "ranvier::Stream", "eres::check"),
        ),
        Rule(
            name="t1-foreign-symbol",
            why="foundation names a tier-2 C or interop symbol; the shared "
            "layer describes the convention, it does not call the callers",
            # The lookbehind admits `.`, so a C# interop call through a
            # `Native` class is caught by the same rule as a bare symbol.
            pattern=re.compile(
                rf"(?<![A-Za-z0-9_])(?:{_T2})_[a-z][A-Za-z0-9_]*(?![A-Za-z0-9])"
            ),
            canaries=(
                "Native.ranvier_stream_destroy(handle)",
                "ca3_open(path)",
                "eres_stream_open",
            ),
        ),
        Rule(
            name="t1-foreign-constant",
            why="foundation names a constant a tier-2 package declares; "
            "foundation defines how such a name is spelled, not what it is",
            pattern=re.compile(
                rf"(?<![A-Za-z0-9_])(?:{_T2_UPPER})_[A-Z][A-Z0-9_]*(?![A-Za-z0-9])"
            ),
            canaries=("CA3_ERR_TRUNCATED", "RANVIER_ERR_NULL", "ERES_ABI_VERSION"),
        ),
        Rule(
            name="t1-foreign-path",
            why="foundation cites a file or directory inside a tier-2 "
            "repository; a path is a reference that rots silently",
            pattern=re.compile(
                rf"(?<![A-Za-z0-9_])(?:{_T2})/"
                r"(?:"
                # A directory citation, restricted to the segments these trees
                # actually use, so a GitHub URL ending at the repository root
                # stays a naming rather than a citation.
                r"(?:crates|bindings|docs|src|tests|tools|conformance|proto|"
                r"spec|python|dotnet|examples)(?![A-Za-z0-9])"
                r"|"
                # A file citation: any depth, ending in something with an
                # extension, with an optional line number.
                r"(?:[A-Za-z0-9_.-]+/)*[A-Za-z0-9_-]+\.[A-Za-z0-9]+(?::\d+)?"
                r")"
            ),
            canaries=(
                # One per branch: a file path, a file path with a line
                # number, and a bare directory citation.
                "ranvier/crates/runtime/node/src/ports.rs",
                "ca3/docs/decisions/0010-the-c-abi.md:237",
                "see eres/spec for the grammar",
            ),
        ),
        Rule(
            name="t1-spec-citation",
            why="foundation cites a numbered section of a tier-2 specification; "
            "the section number is a promise that repository never made here",
            # The repository name has to be adjacent to the section mark. Every
            # tree here cites its OWN records as `decision 0036 §4` and the
            # licence as `Apache-2.0 §4(d)`; neither has a repository name in
            # front of the mark, and neither is a cross-tier reference.
            pattern=re.compile(rf"(?<![A-Za-z0-9_])(?:{_T2})(?:'s)?`?\s*§\s*\d"),
            canaries=("ranvier §4 says", "ca3's §2 exception", "`eres` § 12"),
        ),
        Rule(
            name="t1-manifest-dependency",
            why="foundation takes a build-time dependency on a tier-2 package, "
            "which inverts the dependency direction the tiers exist to fix",
            pattern=re.compile(
                r"(?:"
                # Cargo / pyproject: a key on the left of `=`.
                rf"^\s*[\"']?(?:{_T2})[A-Za-z0-9_-]*[\"']?\s*="
                r"|"
                # package.json / csproj: a quoted specifier before a `:`.
                rf"[\"'][@A-Za-z0-9_./-]*(?<![A-Za-z0-9_])(?:{_T2})"
                r"[A-Za-z0-9_-]*[\"']\s*:"
                r")"
            ),
            canaries=(
                'ca3-api = { path = "../api" }',
                '  "@extendedresearch/ranvier": "^1.0.0",',
            ),
            paths=MANIFEST_PATHS,
        ),
        Rule(
            name="t1-application",
            why="foundation names an application; the shared layer is two "
            "tiers below anything that ships to a person",
            pattern=re.compile(word(_APPS), re.IGNORECASE),
            canaries=("ca3-viewer", "RayTx", "VisualField", "eres-ide", "ranvier-flow"),
        ),
    ]


# --------------------------------------------------------------------------
#  Tier 2 — ranvier, ca3 and eres may name and reference foundation and
#  themselves, and nothing else. A BARE MENTION of anything else is the
#  violation; there is no naming/referencing distinction at this tier, because
#  three sibling libraries that describe each other converge into one library
#  with three checkouts.
# --------------------------------------------------------------------------


def _tier2_rules(repo: str) -> list[Rule]:
    foreign = [name for name in TIER2 if name != repo]
    alternation = "|".join(foreign)
    return [
        Rule(
            name="t2-sibling-repo",
            why=f"{repo} mentions a sibling core repository; a sibling may be "
            "depended on through foundation or not at all",
            pattern=re.compile(word(alternation), re.IGNORECASE),
            canaries=tuple(foreign),
        ),
        Rule(
            name="t2-application",
            why=f"{repo} names an application; a library that knows its "
            "consumers by name has already been shaped by them",
            pattern=re.compile(word(_APPS), re.IGNORECASE),
            canaries=(
                "ca3-viewer's build",
                "RayTx",
                "VisualField",
                "eres-ide",
                "ranvier-flow",
            ),
        ),
        Rule(
            name="t2-plugins-repo",
            why=f"{repo} names the plugins repository, which sits a tier above",
            # Repository-naming forms only. See the docstring for the 78-hit
            # fixture that makes the bare noun unusable as a rule.
            pattern=re.compile(
                r"(?:"
                r"`plugins`"
                r"|(?<![A-Za-z0-9_/.-])plugins/"
                r"|(?<![A-Za-z0-9_])plugins(?![A-Za-z0-9])['’]?s?\s+"
                r"(?:repo|repository|repositories)"
                r")"
            ),
            canaries=(
                "`plugins` each run",
                "plugins/AV/Codecs",
                "the plugins repository",
                "plugins repo",
            ),
        ),
        Rule(
            name="t2-ecosystem-repo",
            why=f"{repo} names the ecosystem repository",
            pattern=re.compile(
                r"(?:"
                r"`ecosystem`"
                r"|extendedresearch-ecosystem"
                r"|(?<![A-Za-z0-9_/.-])ecosystem/"
                r"|(?<![A-Za-z0-9_])ecosystem(?![A-Za-z0-9])['’]?s?\s+"
                r"(?:repo|repository|repositories)"
                r")"
            ),
            canaries=(
                "`ecosystem`'s workflow",
                "`extendedresearch-ecosystem` decisions 0014",
                "ecosystem/docs",
                "the ecosystem repository",
            ),
        ),
        Rule(
            name="t2-plugin-package",
            why=f"{repo} names a plugin package; the package list is a tier-3 "
            "fact and changes without this repository hearing about it",
            pattern=re.compile(
                word(
                    r"pupillabs-neon|pupil-labs-neon|interaxon-muse|sony-mocopi"
                    r"|ranvier-package-(?:\*|[A-Za-z0-9-]+)"
                ),
                re.IGNORECASE,
            ),
            canaries=(
                "`ranvier-package-muse`'s six-second sweep",
                "ranvier-package-* repositories",
                'runtime.node("pupillabs-neon")',
                'kind: "pupil-labs-neon"',
                "interaxon-muse",
                "sony-mocopi",
            ),
        ),
        Rule(
            name="t2-plugin-hardware",
            why=f"{repo} uses plugin hardware as an example; an example is how "
            "a specific device ends up shaping a general interface",
            # Two case-sensitive branches with different lookaheads. See the
            # docstring: `neon` the crate and `Neon` the tracker are separated
            # by nothing but case, and the shouted spelling is how these names
            # arrive in fixture identifiers.
            pattern=re.compile(
                r"(?<![A-Za-z0-9_])(?:"
                # Capitalised: `(?![a-z])` so `Museum`, `Neonatal` and `muses`
                # do not match while `Muse-1A2B` and `Neon's` do.
                r"(?:Muse|Neon|Mocopi|mocopi)(?![a-z])"
                r"|"
                # Shouted: the ordinary boundary, so `NEON-0042` matches and
                # `NEONATAL` does not.
                r"(?:MUSE|NEON|MOCOPI)(?![A-Za-z0-9])"
                r")"
            ),
            canaries=(
                "Two Muse headbands are two nodes",
                'a Neon\'s address',
                "`ranvier-package-mocopi` hangs",
                'source_clock_domain: "device.NEON-0042"',
                'serial: "MUSE-1A2B"',
                'kind: "MOCOPI"',
            ),
        ),
        Rule(
            name="t2-plugins-noun",
            why="advisory only: the bare English noun, counted so the decision "
            "to ignore it stays visible",
            pattern=re.compile(word("plugins"), re.IGNORECASE),
            canaries=("protobuf plugins",),
            advisory=True,
        ),
        Rule(
            name="t2-ecosystem-noun",
            why="advisory only: the bare English noun, counted so the decision "
            "to ignore it stays visible",
            pattern=re.compile(word("ecosystem"), re.IGNORECASE),
            canaries=("every library in the ecosystem",),
            advisory=True,
        ),
    ]


# --------------------------------------------------------------------------
#  Tier 3 — plugins may reference tiers 1 and 2 and must never reference a
#  specific application. No application is named literally in this tree today,
#  so every rule below is oblique: the references that survive are the ones
#  that describe an application without naming it, and those are exactly the
#  ones a name-based rule cannot see.
# --------------------------------------------------------------------------


def _tier3_rules() -> list[Rule]:
    return [
        Rule(
            name="t3-application",
            why="plugins names an application",
            pattern=re.compile(word(_APPS), re.IGNORECASE),
            canaries=("RayTx", "VisualField", "ca3-viewer", "eres-ide", "ranvier-flow"),
        ),
        Rule(
            name="t3-oblique-application",
            why="plugins describes a fixed set of applications; 'the two' is a "
            "count that was true once and is a dependency either way",
            pattern=re.compile(
                r"(?<![A-Za-z0-9_])(?:"
                r"the two applications"
                r"|both applications"
                r"|each application['’]s"
                r"|the applications this (?:replaces|comes from)"
                r")(?![A-Za-z0-9])",
                re.IGNORECASE,
            ),
            canaries=(
                "what the two applications already had",
                "Both applications divided by full scale",
                "each application's own idea of a recording",
                "the applications this replaces",
            ),
        ),
        Rule(
            name="t3-oblique-consumer",
            why="plugins describes a particular consumer; a definite article "
            "and a role is a name with the name filed off",
            pattern=re.compile(
                r"(?<![A-Za-z0-9_])(?:"
                r"the external consumer"
                r"|the consumer['’]s"
                r"|an? dashboard consumer"
                r"|an? node-graph consumer"
                r")(?![A-Za-z0-9])",
                re.IGNORECASE,
            ),
            canaries=(
                "the external consumer's checkout",
                "The consumer's channel lookup",
                "a dashboard consumer | 166 | 0 |",
                "a node-graph consumer is unmeasured",
            ),
        ),
        Rule(
            name="t3-application-shape",
            why="plugins assumes the shape of the program that loads it — a "
            "desktop shell with a main process and a renderer",
            pattern=re.compile(
                r"(?:"
                r"(?<![A-Za-z0-9_])app-level(?![A-Za-z0-9])"
                r"|(?<![A-Za-z0-9_])renderer bundles?(?![A-Za-z0-9])"
                r"|(?<![A-Za-z0-9_])the main process(?![A-Za-z0-9])"
                r")",
                re.IGNORECASE,
            ),
            canaries=(
                "its app-level scenario suite",
                "main and renderer bundles are built separately",
                "a renderer bundle",
                "as the main process hands it over",
            ),
        ),
        Rule(
            name="t3-electron",
            why="plugins names the host runtime of a particular application; "
            "a plugin that knows it is in a desktop shell is not portable",
            # Case-sensitive with a lowercase lookahead, which is what keeps
            # the Apache-2.0 licence text ("electronic mailing lists") and
            # "Interaxon's electronics" out of the report.
            pattern=re.compile(r"(?<![A-Za-z0-9_])Electron(?![a-z])"),
            canaries=("Electron does not gate audio",),
        ),
        Rule(
            name="t3-application-path",
            why="plugins cites a path inside an application checkout",
            pattern=re.compile(r"(?<![A-Za-z0-9_./-])apps/[A-Za-z0-9_.-]+"),
            canaries=("apps/RayTx-VisualField-V2/tests", "cd apps/viewer && npm test"),
        ),
    ]


def rules_for(repo: str) -> list[Rule]:
    """Every rule that applies to `repo`, or `[]` for a name with no tier.

    An empty list is returned rather than raised, because the caller's first
    floor is "this repository has no rules" and a floor that is bypassed by an
    exception raised somewhere else is not a floor.
    """
    if repo in TIER1:
        return _tier1_rules()
    if repo in TIER2:
        return _tier2_rules(repo)
    if repo in TIER3:
        return _tier3_rules()
    return []


def tier_of(repo: str) -> int | None:
    if repo in TIER1:
        return 1
    if repo in TIER2:
        return 2
    if repo in TIER3:
        return 3
    return None
