#!/usr/bin/env python3
"""Every repository's copy of a shared style configuration equals foundation's.

    python style/scripts/check-configs.py            # in any repository
    python style/scripts/check-configs.py --source <path to foundation>

Some of these tools have no `extends` mechanism — `rustfmt.toml` and
`clippy.toml` in particular — so a shared configuration reaches a repository as
a copy. A copy nothing compares is a copy that drifts, which is how four
repositories end up with four formatters that agree about nothing.

This is the rule foundation already holds its `LICENSE` and `NOTICE` copies to,
applied to configuration. Every row is printed, passing rows included: a report
of only failures cannot distinguish a file that matched from one that was never
looked at.

Line endings are normalised before comparison, so a checkout's `eol` setting is
not drift. Standard library only.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# source name in style/configs  ->  where a repository keeps its copy
COPIES: dict[str, str] = {
    "editorconfig": ".editorconfig",
    "rustfmt.toml": "rustfmt.toml",
    "clippy.toml": "clippy.toml",
    "ruff.toml": "ruff.toml",
    "prettierrc.json": ".prettierrc.json",
    "eslint.config.mjs": "eslint.config.mjs",
    "Directory.Build.props": "Directory.Build.props",
}

# A repository that has no code in a language needs no copy of that language's
# configuration, and its absence is reported as `absent` rather than as drift.
OPTIONAL = set(COPIES)


def normalise(text: str) -> str:
    """Line endings and a trailing newline are not drift."""
    return text.replace("\r\n", "\n").replace("\r", "\n").rstrip("\n") + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=None,
        help="path to a foundation checkout; defaults to this repository",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path.cwd(),
        help="the repository to check; defaults to the working directory",
    )
    args = parser.parse_args()

    source_root = args.source if args.source is not None else args.root
    configs = source_root / "style" / "configs"
    if not configs.is_dir():
        print(f"no style/configs under {source_root}", file=sys.stderr)
        print("pass --source with a path to a foundation checkout", file=sys.stderr)
        return 2

    rows: list[tuple[str, str, str]] = []
    failed = 0

    for name, destination in sorted(COPIES.items()):
        origin = configs / name
        copy = args.root / destination

        if not origin.is_file():
            rows.append((destination, "ERROR", f"style/configs/{name} is missing"))
            failed += 1
            continue

        # Checking foundation against itself compares a file with its own source.
        if origin.resolve() == copy.resolve():
            rows.append((destination, "source", "this is the source copy"))
            continue

        if not copy.is_file():
            verdict = "absent" if name in OPTIONAL else "MISSING"
            if verdict == "MISSING":
                failed += 1
            rows.append((destination, verdict, "no copy in this repository"))
            continue

        want = normalise(origin.read_text(encoding="utf-8"))
        have = normalise(copy.read_text(encoding="utf-8"))
        if want == have:
            rows.append((destination, "equal", f"matches style/configs/{name}"))
        else:
            rows.append((destination, "DRIFTED", f"differs from style/configs/{name}"))
            failed += 1

    width = max(len(r[0]) for r in rows)
    print(f"{'file':<{width}}  {'verdict':<8}  detail")
    print(f"{'-' * width}  {'-' * 8}  {'-' * 40}")
    for destination, verdict, detail in rows:
        print(f"{destination:<{width}}  {verdict:<8}  {detail}")
    print()

    if failed:
        print(f"{failed} configuration copy/copies need attention")
        print("copy the file from style/configs/, or change it in foundation and re-copy")
        return 1

    print(f"{len(rows)} configuration file(s) checked, none drifted")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
