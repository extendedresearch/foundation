"""The runner against three fixture bindings, conforming and broken.

    python -m unittest discover -s python/conformance/tests

Each test copies `fixtures/` into a temporary directory, writes a
`conformance.toml` naming three languages that all run `fixtures/driver.py`,
and runs the suite in-process. A test that expects a failure checks that the
failing row is the one it broke, so a suite that failed for another reason does
not pass it.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
PACKAGE = HERE.parent
sys.path.insert(0, str(PACKAGE / "src"))

from extendedresearch_conformance import config as config_module  # noqa: E402
from extendedresearch_conformance.header import Header  # noqa: E402
from extendedresearch_conformance.run import main  # noqa: E402

CONFIG = """\
header = "fixture.h"
prefix = "FIX"
cases = "cases.json"

[[language]]
name = "alpha"
command = ["{python}", "driver.py", "--language", "{language}", "--out", "{out}", "--cases", "{cases}"]

[[language]]
name = "beta"
command = ["{python}", "driver.py", "--language", "{language}", "--out", "{out}", "--cases", "{cases}"]

[[language]]
name = "gamma"
command = ["{python}", "{root}/driver.py", "--language", "gamma", "--out", "{out}", "--cases", "{cases}"]
env = { FIXTURE_SEEN_ROOT = "{root}" }
"""


class Suite(unittest.TestCase):
    """One temporary copy of the fixtures per test."""

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="er-conformance-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        for name in ("fixture.h", "cases.json", "driver.py"):
            shutil.copy(HERE / "fixtures" / name, self.dir / name)
        (self.dir / "conformance.toml").write_text(CONFIG, encoding="utf-8")

    def run_suite(self, fault: str = "", *extra: str) -> tuple[int, str]:
        output = io.StringIO()
        with mock.patch.dict(os.environ, {"FIXTURE_FAULT": fault}):
            with contextlib.redirect_stdout(output):
                status = main(["--config", str(self.dir / "conformance.toml"), *extra])
        return status, output.getvalue()

    def edit_cases(self, edit) -> None:
        path = self.dir / "cases.json"
        table = json.loads(path.read_text(encoding="utf-8"))
        edit(table)
        path.write_text(json.dumps(table), encoding="utf-8")

    def row(self, output: str, case: str, observation: str) -> str:
        for line in output.splitlines():
            if line.startswith(case + " ") and f"  {observation} " in line + " ":
                return line
        self.fail(f"no row {case} / {observation} in:\n{output}")


class Conforming(Suite):
    def test_three_conforming_bindings_pass_and_every_row_is_printed(self) -> None:
        status, output = self.run_suite()
        self.assertEqual(status, 0, output)
        header = output.splitlines()[0].split()
        self.assertEqual(header[:5], ["case", "observation", "alpha", "beta", "gamma"])
        self.assertIn("OK   agree + ABI", self.row(output, "color", "member count"))
        self.assertIn("OK   agree", self.row(output, "color", "the bindings agree"))
        self.assertIn("OK   agree + ABI", self.row(output, "version", "linked"))
        self.assertIn("OK   positive", self.row(output, "version", "clock"))
        self.assertIn("OK   agree + ABI", self.row(output, "statuses", "busy and gone stay two codes"))
        self.assertIn("OK   agree", self.row(output, "statuses", "no cancellation status exists yet"))
        self.assertIn("OK   agree", self.row(output, "error_busy", "every binding landed on one outcome"))
        self.assertIn("across fixture alpha, fixture beta, fixture gamma; 0 failed", output)
        self.assertNotIn("FAIL", output)

    def test_the_legacy_status_rule_forms_are_read(self) -> None:
        def legacy(table: dict) -> None:
            case = table["cases"][0]
            case["distinct"] = ["FIX_ERR_BUSY", "FIX_ERR_GONE"]
            case["no_status_matching"] = {"pattern": "CANCEL", "why": "Recorded."}
            del case["absent"]

        self.edit_cases(legacy)
        status, output = self.run_suite()
        self.assertEqual(status, 0, output)
        self.row(output, "statuses", "FIX_ERR_BUSY, FIX_ERR_GONE stay distinct")
        self.row(output, "statuses", "no status matches /CANCEL/")

    def test_recorded_observations_are_compared_without_running_drivers(self) -> None:
        for language in ("alpha", "beta", "gamma"):
            observations = self.dir / f"observations-{language}.json"
            with mock.patch.object(
                sys, "argv",
                ["driver.py", "--language", language, "--out", str(observations),
                 "--cases", str(self.dir / "cases.json")],
            ):
                namespace: dict = {"__name__": "fixture"}
                exec((self.dir / "driver.py").read_text(encoding="utf-8"), namespace)
                self.assertEqual(namespace["main"](), 0)
        (self.dir / "driver.py").write_text("raise SystemExit(9)\n", encoding="utf-8")
        status, output = self.run_suite("", "--observations", str(self.dir))
        self.assertEqual(status, 0, output)


class Failing(Suite):
    def test_a_missing_family_member_is_a_disagreement(self) -> None:
        status, output = self.run_suite("beta:drop_member")
        self.assertEqual(status, 1)
        self.assertIn("FAIL disagree", self.row(output, "color", "beta members match the header"))
        self.assertIn("missing ['COLOR_BLUE']", self.row(output, "color", "beta members match the header"))
        self.assertIn("FAIL disagree", self.row(output, "color", "the bindings agree"))
        self.assertIn("OK   agree + ABI", self.row(output, "color", "alpha members match the header"))

    def test_one_binding_answering_differently_is_a_disagreement(self) -> None:
        status, output = self.run_suite("gamma:greeting")
        self.assertEqual(status, 1)
        self.assertIn("FAIL disagree", self.row(output, "version", "greeting"))

    def test_a_binding_without_a_status_table_is_undeclared(self) -> None:
        status, output = self.run_suite("alpha:no_statuses")
        self.assertEqual(status, 1)
        self.assertIn("FAIL undeclared", self.row(output, "statuses", "alpha enumerates the header's statuses"))

    def test_a_declared_absence_passes(self) -> None:
        self.edit_cases(lambda table: table["cases"][0].update(declared="Alpha has no table. Said here."))
        status, output = self.run_suite("alpha:no_statuses")
        self.assertEqual(status, 0, output)
        self.assertIn("OK   declared", self.row(output, "statuses", "alpha enumerates the header's statuses"))

    def test_a_read_only_some_bindings_offer_is_undeclared(self) -> None:
        self.edit_cases(lambda table: table["cases"][1]["reads"]["greeting"].update(gamma=None))
        status, output = self.run_suite()
        self.assertEqual(status, 1)
        self.assertIn("FAIL undeclared", self.row(output, "version", "greeting"))

    def test_failures_that_look_alike_are_caught(self) -> None:
        status, output = self.run_suite("alpha:same_error")
        self.assertEqual(status, 1)
        self.assertIn("FAIL expectation", self.row(output, "error_gone", "alpha raises"))
        self.assertIn("FAIL disagree", self.row(output, "error.*", "alpha keeps the failures apart"))
        self.assertIn("OK   agree", self.row(output, "error.*", "beta keeps the failures apart"))

    def test_a_status_the_absent_rule_forbids_is_caught(self) -> None:
        header = self.dir / "fixture.h"
        header.write_text(
            header.read_text(encoding="utf-8").replace(
                "#define FIX_ERR_GONE -17", "#define FIX_ERR_GONE -17\n#define FIX_ERR_CANCELLED -18"
            ),
            encoding="utf-8",
        )
        status, output = self.run_suite()
        self.assertEqual(status, 1)
        line = self.row(output, "statuses", "no cancellation status exists yet")
        self.assertIn("FAIL disagree", line)
        self.assertIn("FIX_ERR_CANCELLED", line)

    def test_codes_that_must_stay_distinct_but_collide_are_caught(self) -> None:
        header = self.dir / "fixture.h"
        header.write_text(
            header.read_text(encoding="utf-8").replace("FIX_ERR_GONE -17", "FIX_ERR_GONE -16"),
            encoding="utf-8",
        )
        status, output = self.run_suite()
        self.assertEqual(status, 1)
        self.assertIn("FAIL disagree", self.row(output, "statuses", "busy and gone stay two codes"))

    def test_a_driver_that_crashes_is_reported_not_skipped(self) -> None:
        status, output = self.run_suite("beta:crash")
        self.assertEqual(status, 1)
        line = self.row(output, "(driver)", "beta ran")
        self.assertIn("FAIL driver", line)
        self.assertIn("exited 3", line)

    def test_a_case_a_driver_never_recorded_is_reported(self) -> None:
        status, output = self.run_suite("gamma:forget_case")
        self.assertEqual(status, 1)
        self.assertIn("FAIL driver", self.row(output, "version", "gamma driver"))

    def test_an_empty_corpus_fails(self) -> None:
        self.edit_cases(lambda table: table.update(cases=[], families=[]))
        status, output = self.run_suite()
        self.assertEqual(status, 1)
        self.assertIn("declares no cases and no families", output)


class Configuration(unittest.TestCase):
    def write(self, text: str) -> Path:
        directory = Path(tempfile.mkdtemp(prefix="er-conformance-config-"))
        self.addCleanup(shutil.rmtree, directory, True)
        path = directory / "conformance.toml"
        path.write_text(text, encoding="utf-8")
        return path

    def test_paths_resolve_against_the_file_and_the_version_name_defaults(self) -> None:
        path = self.write(CONFIG)
        loaded = config_module.load(path)
        self.assertEqual(loaded.header, path.parent / "fixture.h")
        self.assertEqual(loaded.abi_version, "FIX_ABI_VERSION")
        self.assertEqual([one.name for one in loaded.languages], ["alpha", "beta", "gamma"])
        self.assertEqual(loaded.languages[2].env, {"FIXTURE_SEEN_ROOT": "{root}"})

    def test_a_missing_key_or_a_duplicate_language_is_an_error(self) -> None:
        with self.assertRaises(config_module.ConfigError):
            config_module.load(self.write('prefix = "FIX"\ncases = "c.json"\n'))
        duplicate = CONFIG.replace('name = "beta"', 'name = "alpha"')
        with self.assertRaises(config_module.ConfigError):
            config_module.load(self.write(duplicate))
        status = main(["--config", str(self.write('header = "h"\n'))])
        self.assertEqual(status, 2)

    def test_placeholders_are_replaced_and_other_braces_are_not(self) -> None:
        self.assertEqual(
            config_module.expand("{python} --x={out} {unknown}", {"python": "py", "out": "o.json"}),
            "py --x=o.json {unknown}",
        )


class Headers(unittest.TestCase):
    def test_only_the_prefix_s_constants_are_read(self) -> None:
        header = Header(HERE / "fixtures" / "fixture.h", "FIX")
        self.assertNotIn("OTHER_ERR_NULL", header.constants())
        self.assertEqual(header.value("FIX_ABI_VERSION"), 3)
        self.assertEqual(
            header.family("FIX_COLOR_", "COLOR_"),
            {"COLOR_RED": 0, "COLOR_GREEN": 1, "COLOR_BLUE": 7},
        )
        self.assertEqual(len(header.statuses()), 8)
        self.assertNotIn("FIX_ABI_VERSION", header.statuses())

    def test_a_stale_prefix_fails_loudly(self) -> None:
        header = Header(HERE / "fixtures" / "fixture.h", "FIX")
        with self.assertRaises(AssertionError):
            header.family("FIX_COLOUR_", "COLOUR_")
        with self.assertRaises(AssertionError):
            Header(HERE / "fixtures" / "fixture.h", "NOPE").constants()


class License(unittest.TestCase):
    def test_the_package_ships_the_repository_s_license_and_notice(self) -> None:
        root = PACKAGE.parent.parent
        for name in ("LICENSE", "NOTICE"):
            copy = (PACKAGE / name).read_text(encoding="utf-8").replace("\r\n", "\n")
            original = root / name
            if original.exists():
                self.assertEqual(copy, original.read_text(encoding="utf-8").replace("\r\n", "\n"), name)


if __name__ == "__main__":
    unittest.main()
