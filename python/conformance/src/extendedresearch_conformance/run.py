"""Run a package's conformance cases across every language binding it has.

    extendedresearch-conformance --config conformance.toml

It runs each driver named in the configuration. Each walks the cases file and
writes down what its binding did; this module then makes every comparison: each
binding against the header, each against what its case declared, and every
binding against every other.

**The full matrix is printed, including every row that passes.** A report of
only the disagreements cannot be told apart from one that never ran.

Exit status is 0 when every row holds, and 1 otherwise — including when a
driver could not run and when the cases file is empty. A configuration error is
2.

# What counts as a failure

- Bindings answering different values for a read the case did not declare a
  difference for.
- A binding disagreeing with the header about what the contract declares.
- A binding failing an expectation the case wrote down.
- An asymmetry with no `declared` note: a family, a status table or a read that
  some bindings offer and others do not. **A difference nobody wrote down is a
  finding even when it is harmless**, because the alternative is a suite that
  quietly normalises whatever it meets.

# Status rules are case data

A `statuses` case may carry rules about the header's statuses as a whole:

```json
"distinct": [{"label": "timeout, network and address stay three codes",
              "codes": ["EXAMPLE_ERR_TIMEOUT", "EXAMPLE_ERR_NETWORK", "EXAMPLE_ERR_ADDRESS"]}],
"absent":   [{"label": "no cancellation status exists yet",
              "pattern": "CANCEL|CLOSED",
              "why": "A recorded gap, asserted so that closing it cannot pass silently."}]
```

A `distinct` group holds when every code is in the header and no two share a
value. An `absent` rule holds while no status matches its pattern. The older
forms — `distinct` as a plain list of names, and `no_status_matching` as one
`{pattern, why}` — are read as one group or rule each.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from extendedresearch_conformance.config import Config, ConfigError, expand, load
from extendedresearch_conformance.header import Header

#: A short label per verdict. `OK` prefixes everything that held.
AGREE = "OK   agree"
EXPECTED = "OK   expected"
DECLARED = "OK   declared"
POSITIVE = "OK   positive"
MATCHES_ABI = "OK   agree + ABI"
DISAGREE = "FAIL disagree"
WRONG = "FAIL expectation"
UNDECLARED = "FAIL undeclared"
BROKEN = "FAIL driver"


@dataclass(frozen=True)
class Row:
    """One comparison and its verdict."""

    case: str
    observation: str
    cells: tuple[str, ...]
    verdict: str
    note: str

    @property
    def failed(self) -> bool:
        return self.verdict.startswith("FAIL")


class Matrix:
    """Every row this run checked, and whether it held."""

    def __init__(self, languages: tuple[str, ...]) -> None:
        self.languages = languages
        self.rows: list[Row] = []

    def add(
        self,
        case: str,
        observation: str,
        values: dict[str, Any],
        verdict: str,
        note: str = "",
    ) -> None:
        cells = tuple(_render(values.get(language)) for language in self.languages)
        self.rows.append(Row(case, observation, cells, verdict, note))

    @property
    def failures(self) -> list[Row]:
        return [row for row in self.rows if row.failed]


def _render(value: Any) -> str:
    if value is None:
        return "—"
    if isinstance(value, bool):
        return "true" if value else "false"
    text = str(value)
    return text if len(text) <= 34 else text[:31] + "..."


def _first_sentence(text: str) -> str:
    stripped = text.replace("**", "")
    at = stripped.find(". ")
    return stripped if at < 0 else stripped[: at + 1]


def _only(language: str, value: Any) -> dict[str, Any]:
    return {language: value}


# -- running the drivers --------------------------------------------------


def run_drivers(
    config: Config, into: Path, cases: Path
) -> tuple[dict[str, dict[str, Any]], dict[str, str]]:
    """Run every driver; answer what each wrote, and why any could not."""
    seen: dict[str, dict[str, Any]] = {}
    broken: dict[str, str] = {}
    for language in config.languages:
        out = into / f"observations-{language.name}.json"
        values = {
            "python": sys.executable,
            "out": str(out),
            "cases": str(cases),
            "root": str(config.root),
            "language": language.name,
        }
        command = [expand(part, values) for part in language.command]
        env = {**os.environ, **{key: expand(value, values) for key, value in language.env.items()}}
        try:
            completed = subprocess.run(command, cwd=config.root, env=env, check=False)
        except OSError as error:
            broken[language.name] = f"could not start {command[0]}: {error}"
            continue
        if completed.returncode != 0:
            broken[language.name] = f"exited {completed.returncode}"
            continue
        try:
            seen[language.name] = json.loads(out.read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            broken[language.name] = f"wrote no readable observations: {error}"
    return seen, broken


# -- the enumerations -----------------------------------------------------


def check_family(
    matrix: Matrix, family: dict[str, Any], seen: dict[str, Any], header: Header
) -> None:
    """One enumeration, against the header and across the bindings."""
    languages = matrix.languages
    declared = header.family(family["header_prefix"], family["contract_prefix"])
    prefix = family["contract_prefix"]
    contract: dict[str, dict[str, int]] = {}
    short: dict[str, dict[str, int]] = {}

    for language in languages:
        observed = seen[language].get("families", {}).get(family["id"])
        if observed is None:
            continue
        every = observed["all"]
        contract[language] = {n: v for n, v in every.items() if n.startswith(prefix)}
        short[language] = {n: v for n, v in every.items() if not n.startswith(prefix)}

    exposing = [language for language in languages if language in contract]
    if not exposing:
        matrix.add(family["id"], "exposed by", {}, UNDECLARED, "no binding exposes this family")
        return

    if len(exposing) < len(languages):
        note = _first_sentence(family.get("declared", ""))
        matrix.add(
            family["id"],
            "exposed by",
            {language: "yes" for language in exposing},
            DECLARED if note else UNDECLARED,
            note or "some bindings do not expose it and the case declares no reason",
        )

    # The count, asserted as well as the members. A scan that matched nothing
    # agrees with a binding that reported nothing, and every "same set"
    # assertion below would then pass vacuously.
    matrix.add(
        family["id"],
        "member count",
        {language: len(contract[language]) or None for language in exposing},
        MATCHES_ABI
        if all(len(contract[language]) == len(declared) for language in exposing)
        else DISAGREE,
        f"the header declares {len(declared)}",
    )

    for language in exposing:
        matches = contract[language] == declared
        matrix.add(
            family["id"],
            f"{language} members match the header",
            _only(language, len(contract[language])),
            MATCHES_ABI if matches else DISAGREE,
            ""
            if matches
            else f"missing {sorted(set(declared) - set(contract[language]))}, "
            f"extra {sorted(set(contract[language]) - set(declared))}",
        )
        values = list(contract[language].values())
        matrix.add(
            family["id"],
            f"{language} values are distinct",
            _only(language, len(set(values))),
            AGREE if len(set(values)) == len(values) else DISAGREE,
        )
        matrix.add(
            family["id"],
            f"{language} binds a short name for every member",
            _only(language, len(short[language])),
            AGREE if len(short[language]) == len(contract[language]) else DISAGREE,
        )

    if len(exposing) >= 2:
        first = contract[exposing[0]]
        same = all(contract[language] == first for language in exposing)
        matrix.add(
            family["id"],
            "the bindings agree",
            {language: len(contract[language]) for language in exposing},
            AGREE if same else DISAGREE,
            "" if same else "the member sets differ",
        )


# -- the statuses ---------------------------------------------------------


def _distinct_groups(case: dict[str, Any]) -> list[dict[str, Any]]:
    groups = case.get("distinct", [])
    if groups and all(isinstance(one, str) for one in groups):
        return [{"label": f"{', '.join(groups)} stay distinct", "codes": groups}]
    return list(groups)


def _absent_rules(case: dict[str, Any]) -> list[dict[str, Any]]:
    rules = list(case.get("absent", []))
    legacy = case.get("no_status_matching")
    if legacy:
        rules.append({"label": f"no status matches /{legacy['pattern']}/", **legacy})
    return rules


def check_statuses(
    matrix: Matrix, case: dict[str, Any], seen: dict[str, Any], header: Header
) -> None:
    """The status table, and the case's rules about it."""
    declared = header.statuses()
    for language in matrix.languages:
        observed = seen[language]["cases"][case["id"]].get("statuses")
        if observed is None:
            matrix.add(
                case["id"],
                f"{language} enumerates the header's statuses",
                _only(language, "no"),
                DECLARED if case.get("declared") else UNDECLARED,
                _first_sentence(case.get("declared", "")),
            )
            continue
        matrix.add(
            case["id"],
            f"{language} matches the header",
            _only(language, len(observed)),
            MATCHES_ABI if observed == declared else DISAGREE,
            f"the header declares {len(declared)}",
        )

    everywhere = dict.fromkeys(matrix.languages)
    for group in _distinct_groups(case):
        values = [declared.get(name) for name in group["codes"]]
        rendered = ",".join(str(one) for one in values)
        missing = [name for name, value in zip(group["codes"], values) if value is None]
        matrix.add(
            case["id"],
            group.get("label", "these codes stay distinct"),
            {language: rendered for language in everywhere},
            MATCHES_ABI if not missing and len(set(values)) == len(values) else DISAGREE,
            f"the header lacks {missing}" if missing else "",
        )

    for rule in _absent_rules(case):
        pattern = rule["pattern"]
        matched = [name for name in declared if re.search(pattern, name)]
        matrix.add(
            case["id"],
            rule.get("label", f"no status matches /{pattern}/"),
            {language: len(matched) for language in everywhere},
            AGREE if not matched else DISAGREE,
            rule.get("note", f"a status matching /{pattern}/ would close this")
            if not matched
            else f"a status now matches: {matched}. {_first_sentence(rule.get('why', ''))}".strip(),
        )


# -- the scenarios --------------------------------------------------------


def check_reads(
    matrix: Matrix, case: dict[str, Any], seen: dict[str, Any], header: Header, config: Config
) -> None:
    """Each value the case reads, across the bindings that offer it."""
    languages = matrix.languages
    for name, entry in case["reads"].items():
        observed = {
            language: seen[language]["cases"][case["id"]].get("read", {}).get(name)
            for language in languages
        }
        offered = [language for language in languages if entry.get(language) is not None]
        shown = {language: observed[language] for language in offered}
        expect = entry.get("expect")
        note = _first_sentence(entry.get("declared", ""))

        if not offered:
            matrix.add(case["id"], name, {}, UNDECLARED, "no binding offers this read")
            continue

        if entry.get("abi") == "abi_version":
            wanted = str(header.value(config.abi_version))
            held = all(observed[one] == wanted for one in offered)
            matrix.add(
                case["id"], name, shown, MATCHES_ABI if held else DISAGREE,
                f"the header states {wanted}",
            )
            continue

        if entry.get("compare") == "positive":
            try:
                held = all(int(observed[one]) > 0 for one in offered)
            except (TypeError, ValueError):
                held = False
            matrix.add(case["id"], name, shown, POSITIVE if held else DISAGREE, note)
            continue

        if expect is not None:
            wrong = [one for one in offered if one in expect and observed[one] != expect[one]]
            matrix.add(
                case["id"], name, shown,
                WRONG if wrong else (DECLARED if note else EXPECTED),
                note or (f"expected {expect}" if wrong else ""),
            )
            continue

        same = all(observed[one] == observed[offered[0]] for one in offered)
        if not same:
            verdict = DISAGREE
        elif len(offered) < len(languages) and not note:
            verdict = UNDECLARED
            note = "some bindings do not offer this and the case declares no reason"
        else:
            verdict = DECLARED if note else AGREE
        matrix.add(case["id"], name, shown, verdict, note)


# -- the errors -----------------------------------------------------------


def _matches(observed: dict[str, Any] | None, expect: dict[str, Any] | None) -> bool:
    if observed is None or expect is None:
        return False
    return (
        observed["type"] == expect["type"]
        and observed["code"] == expect["code"]
        and all(one in observed["mro"] for one in expect["mro"])
    )


def _identity(record: dict[str, Any] | None) -> str | None:
    if record is None:
        return None
    return f"{record['type']}/{record['code']}" if record["code"] else record["type"]


def check_error(matrix: Matrix, case: dict[str, Any], seen: dict[str, Any]) -> None:
    """One failure, as each language surfaces it.

    A case lists one or more *paired* outcomes. Where more than one is listed
    the platform decides which arrives — and the pairing is the point: every
    binding must land on the same one.
    """
    languages = matrix.languages
    observed = {
        language: seen[language]["cases"][case["id"]].get("error") for language in languages
    }
    outcomes = case["outcomes"]

    for language in languages:
        record = observed[language]
        held = any(_matches(record, one.get(language)) for one in outcomes)
        listed = " or ".join(
            one[language]["type"] for one in outcomes if one.get(language) is not None
        )
        matrix.add(
            case["id"], f"{language} raises", _only(language, _identity(record)),
            EXPECTED if held else WRONG,
            "" if held else f"no declared outcome matches; the case lists {listed or 'nothing'}",
        )
        matrix.add(
            case["id"], f"{language} inherits",
            _only(language, "/".join(record["mro"]) if record else None),
            EXPECTED if record else WRONG,
            "" if record else "nothing was raised",
        )

    landed = [
        at
        for at, one in enumerate(outcomes)
        if all(_matches(observed[language], one.get(language)) for language in languages)
    ]
    matrix.add(
        case["id"], "every binding landed on one outcome",
        {language: _identity(observed[language]) for language in languages},
        AGREE if landed else DISAGREE,
        outcomes[landed[0]].get("why", "") if landed
        else "the bindings matched different outcomes, or none matched one",
    )


def check_error_distinctness(
    matrix: Matrix, cases: list[dict[str, Any]], seen: dict[str, Any]
) -> None:
    """Different failures must stay distinguishable in each language."""
    error_cases = [one for one in cases if one.get("kind") == "errors"]
    for language in matrix.languages:
        identities = []
        for case in error_cases:
            observed = seen[language]["cases"][case["id"]].get("error") or {}
            identities.append((observed.get("type"), observed.get("code")))
        distinct = len(set(identities)) == len(identities)
        matrix.add(
            "error.*", f"{language} keeps the failures apart",
            _only(language, len(set(identities))),
            AGREE if distinct else DISAGREE,
            f"{len(error_cases)} failures, {len(set(identities))} identities",
        )


# -- the report -----------------------------------------------------------


def report(matrix: Matrix) -> None:
    """Print every row."""
    header = ("case", "observation", *matrix.languages, "verdict")
    table = [(row.case, row.observation, *row.cells, row.verdict) for row in matrix.rows]
    widths = [
        max([len(header[at])] + [len(cells[at]) for cells in table]) for at in range(len(header))
    ]

    def line(cells: tuple[str, ...], note: str = "") -> str:
        body = "  ".join(cell.ljust(widths[at]) for at, cell in enumerate(cells))
        return f"{body}  {note}".rstrip()

    print(line(header, "note"))
    print("  ".join("-" * width for width in widths))
    for cells, row in zip(table, matrix.rows):
        print(line(cells, row.note))


def compare(
    config: Config, table: dict[str, Any], seen: dict[str, Any], broken: dict[str, str]
) -> Matrix:
    """Every comparison, into one matrix."""
    header = Header(config.header, config.prefix)
    matrix = Matrix(tuple(language.name for language in config.languages))

    for language, why in broken.items():
        matrix.add("(driver)", f"{language} ran", _only(language, why), BROKEN)
    if broken:
        return matrix

    # A driver that could not reach a state is a failure of the case, not a
    # reason to leave the row out. Neither is a case a driver never recorded.
    unusable: set[str] = set()
    for case in table["cases"]:
        for language in matrix.languages:
            record = seen[language].get("cases", {}).get(case["id"])
            failure = (
                "recorded nothing for this case" if record is None else record.get("driver_failed")
            )
            if failure:
                unusable.add(case["id"])
                matrix.add(case["id"], f"{language} driver", _only(language, failure), BROKEN)

    for family in table["families"]:
        check_family(matrix, family, seen, header)

    for case in table["cases"]:
        if case["id"] in unusable:
            continue
        kind = case.get("kind", "scenario")
        if kind == "statuses":
            check_statuses(matrix, case, seen, header)
        elif kind == "errors":
            check_error(matrix, case, seen)
        else:
            check_reads(matrix, case, seen, header, config)

    if not unusable:
        check_error_distinctness(matrix, table["cases"], seen)
    return matrix


def main(argv: list[str] | None = None) -> int:
    """Run the suite; answer the exit status."""
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--config", type=Path, default=Path("conformance.toml"))
    parser.add_argument("--cases", type=Path, help="a cases file other than the configured one")
    parser.add_argument(
        "--observations",
        type=Path,
        help="a directory holding observations-<language>.json already written, "
        "instead of running the drivers",
    )
    arguments = parser.parse_args(argv)

    try:
        config = load(arguments.config)
    except ConfigError as error:
        print(f"configuration: {error}", file=sys.stderr)
        return 2
    cases = arguments.cases.resolve() if arguments.cases else config.cases
    table = json.loads(cases.read_text(encoding="utf-8"))
    table.setdefault("families", [])
    table.setdefault("cases", [])

    if arguments.observations:
        seen = {
            language.name: json.loads(
                (arguments.observations / f"observations-{language.name}.json").read_text(
                    encoding="utf-8"
                )
            )
            for language in config.languages
        }
        broken: dict[str, str] = {}
    else:
        with tempfile.TemporaryDirectory() as into:
            seen, broken = run_drivers(config, Path(into), cases)

    matrix = compare(config, table, seen, broken)
    report(matrix)
    failures = matrix.failures
    runtimes = ", ".join(
        str(seen.get(language, {}).get("runtime", language)) for language in matrix.languages
    )
    print()
    print(f"{len(matrix.rows)} row(s) checked across {runtimes}; {len(failures)} failed")
    if failures:
        print()
        for row in failures:
            cells = " ".join(
                f"{language}={cell}" for language, cell in zip(matrix.languages, row.cells)
            )
            print(f"  {row.case} / {row.observation}: {row.verdict} — {cells} {row.note}".rstrip())
        return 1

    # An empty corpus reports zero failures exactly like a full one that held,
    # so the population guarded is the corpus, not only the matrix: the
    # distinctness rows come from the bindings, not the table, and would pass
    # an emptied table on their own.
    empty = [name for name in ("cases", "families") if not table[name]]
    if empty or not matrix.rows:
        print()
        if empty:
            print(
                f"the cases file declares no {' and no '.join(empty)}, so most of what this "
                "suite compares was never enumerated. A run over an empty corpus reports zero "
                "failures exactly like a run in which every row held."
            )
        if not matrix.rows:
            print("no row was checked at all, so the bindings were not compared.")
        return 1
    return 0


def entry() -> None:
    """The console script."""
    sys.exit(main())


if __name__ == "__main__":
    entry()
