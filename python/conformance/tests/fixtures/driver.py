"""A fixture driver standing in for one language's binding.

    python driver.py --language alpha --out observations.json --cases cases.json

It writes the observations a conforming binding of `fixture.h` would. The
`FIXTURE_FAULT` environment variable, `<language>:<fault>`, makes one language
misbehave in one way, so the self-test can see each failure reported.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

STATUSES = {
    "FIX_OK": 0,
    "FIX_ERR_NULL": -1,
    "FIX_ERR_RANGE": -2,
    "FIX_ERR_UTF8": -3,
    "FIX_ERR_PANIC": -4,
    "FIX_ERR_STATE": -5,
    "FIX_ERR_BUSY": -16,
    "FIX_ERR_GONE": -17,
}

COLORS = {"COLOR_RED": 0, "COLOR_GREEN": 1, "COLOR_BLUE": 7}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--language", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cases", type=Path, required=True)
    arguments = parser.parse_args()

    fault_language, _, fault = os.environ.get("FIXTURE_FAULT", "").partition(":")
    fault = fault if fault_language == arguments.language else ""
    if fault == "crash":
        return 3

    colors = dict(COLORS)
    if fault == "drop_member":
        colors.pop("COLOR_BLUE")
    members = {**colors, **{name.removeprefix("COLOR_"): value for name, value in colors.items()}}

    table = json.loads(arguments.cases.read_text(encoding="utf-8"))
    cases: dict[str, dict[str, object]] = {}
    for case in table["cases"]:
        kind = case.get("kind", "scenario")
        if kind == "statuses":
            cases[case["id"]] = {"statuses": None if fault == "no_statuses" else STATUSES}
        elif kind == "errors":
            code = "FIX_ERR_BUSY" if case["id"] == "error_busy" else "FIX_ERR_GONE"
            kind_name = "BusyError" if case["id"] == "error_busy" else "GoneError"
            if fault == "same_error":
                code, kind_name = "FIX_ERR_BUSY", "BusyError"
            cases[case["id"]] = {
                "error": {"type": kind_name, "mro": [kind_name, "FixError", "Exception"], "code": code}
            }
        else:
            cases[case["id"]] = {
                "read": {
                    "linked": "3",
                    "greeting": "hi" if fault != "greeting" else "hello",
                    "clock": "12345",
                }
            }
        if fault == "forget_case" and case["id"] == "version":
            del cases[case["id"]]

    observations = {
        "runtime": f"fixture {arguments.language}",
        "families": {"color": {"all": members}},
        "cases": cases,
    }
    arguments.out.write_text(json.dumps(observations), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
