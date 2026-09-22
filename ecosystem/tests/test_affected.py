"""`ecosystem/affected.py` against synthetic package graphs and synthetic diffs.

    python -m unittest discover -s ecosystem/tests -v

Each closure test builds a declaration in memory and calls `select` directly, so
what is under test is the closure rather than git. The end-to-end tests build a
real throwaway repository — `git init`, a base commit, a branch, a change — and
run the script as a subprocess against it, because the diff range, the base-ref
fallback and the exit code are only real when git is real.

Every assertion names the packages it expects **and** the packages it expects to
be absent. A test that only checks the selected set passes when the script
selects everything, which is the one answer that is never wrong and never
useful.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import affected  # noqa: E402

SCRIPT = HERE.parent / "affected.py"


def declare(*packages: dict) -> dict:
    """Build a declaration holding exactly these packages."""
    return {"package": list(packages)}


def pkg(name: str, path: str, depends: list[str] | None = None, kind: str = "rust") -> dict:
    """Build one package entry, in the shape `PACKAGES.toml` gives."""
    return {"name": name, "path": path, "kind": kind, "depends": depends or []}


# A diamond with a shared root, which is the shape foundation actually has:
#
#   status  <- abi <- pyo3
#     ^        ^
#     |        |
#     +------ napi          clock (an island)  tools (tooling)
GRAPH = declare(
    pkg("status", "crates/status"),
    pkg("abi", "crates/abi", ["status"]),
    pkg("pyo3", "crates/pyo3", ["abi", "status"]),
    pkg("napi", "crates/napi", ["status"]),
    pkg("clock", "crates/clock"),
    pkg("runtime", "npm/runtime"),
    pkg("tools", "toolbox", kind="tooling"),
)
EVERY = {"status", "abi", "pyo3", "napi", "clock", "runtime", "tools"}


def chosen(matrix: dict) -> set[str]:
    """Return the selected names."""
    return {e["name"] for e in matrix["packages"] if e["selected"]}


def reasons(matrix: dict) -> dict[str, str]:
    """Return every package's reason, selected or not."""
    return {e["name"]: e["reason"] for e in matrix["packages"]}


class Closure(unittest.TestCase):
    """The selection rules, against a synthetic graph."""

    def assert_matrix_complete(self, matrix: dict) -> None:
        """Every declared package has a row. A missing row is an unconsidered package."""
        self.assertEqual({e["name"] for e in matrix["packages"]}, EVERY)

    def test_a_leaf_change_selects_one_package(self) -> None:
        # `pyo3` is a leaf: nothing depends on it, so the closure adds nothing.
        matrix = affected.select(GRAPH, ["crates/pyo3/src/lib.rs"], None)
        self.assert_matrix_complete(matrix)
        self.assertEqual(chosen(matrix), {"pyo3"})
        self.assertEqual(reasons(matrix)["pyo3"], affected.DIRECT)
        for name in EVERY - {"pyo3"}:
            self.assertEqual(reasons(matrix)[name], affected.NONE, name)

    def test_a_package_with_no_dependents_selects_only_itself(self) -> None:
        # `clock` is an island: no dependencies and no dependents.
        matrix = affected.select(GRAPH, ["crates/clock/src/lib.rs"], None)
        self.assertEqual(chosen(matrix), {"clock"})
        self.assertNotIn("status", chosen(matrix))

    def test_a_status_shaped_change_selects_everything_downstream(self) -> None:
        # The transitive case: status -> abi -> pyo3, and status -> napi.
        # `pyo3` is two edges away and must still be selected.
        matrix = affected.select(GRAPH, ["crates/status/src/codes.rs"], None)
        self.assertEqual(chosen(matrix), {"status", "abi", "pyo3", "napi"})
        self.assertEqual(reasons(matrix)["status"], affected.DIRECT)
        self.assertEqual(reasons(matrix)["abi"], affected.REVERSE)
        self.assertEqual(reasons(matrix)["pyo3"], affected.REVERSE)
        self.assertEqual(reasons(matrix)["napi"], affected.REVERSE)
        # And nothing that does not depend on it.
        self.assertEqual(reasons(matrix)["clock"], affected.NONE)
        self.assertEqual(reasons(matrix)["runtime"], affected.NONE)
        self.assertEqual(reasons(matrix)["tools"], affected.NONE)

    def test_the_closure_does_not_run_forwards(self) -> None:
        # `pyo3` depends on `abi`; changing `pyo3` must not select `abi`.
        # Selecting a change's *dependencies* is the mistake that makes the
        # answer look conservative while testing the wrong half of the graph.
        matrix = affected.select(GRAPH, ["crates/pyo3/src/lib.rs"], None)
        self.assertNotIn("abi", chosen(matrix))
        self.assertNotIn("status", chosen(matrix))

    def test_a_global_file_selects_all(self) -> None:
        for path in (".github/workflows/ci.yml", "scripts/release.sh", "Cargo.toml", "README.md"):
            with self.subTest(path=path):
                matrix = affected.select(GRAPH, [path], None)
                self.assertEqual(chosen(matrix), EVERY)
                self.assertEqual(set(reasons(matrix).values()), {affected.GLOBAL})

    def test_a_file_owned_by_nobody_is_reported_as_an_orphan(self) -> None:
        matrix = affected.select(GRAPH, ["docs/specs/plan.md"], None)
        self.assertEqual(matrix["files"], [{"path": "docs/specs/plan.md", "package": None}])
        self.assertEqual(chosen(matrix), EVERY)

    def test_a_tooling_change_selects_all(self) -> None:
        matrix = affected.select(GRAPH, ["toolbox/check.py"], None)
        self.assertEqual(chosen(matrix), EVERY)
        self.assertEqual(set(reasons(matrix).values()), {affected.TOOLING})

    def test_a_global_file_beside_a_leaf_change_still_selects_all(self) -> None:
        matrix = affected.select(GRAPH, ["crates/pyo3/src/lib.rs", "scripts/x.sh"], None)
        self.assertEqual(chosen(matrix), EVERY)

    def test_ownership_matches_whole_components(self) -> None:
        # `dotnet/Interop.Package` must not own `dotnet/Interop.PackageTest`.
        # A raw string prefix would, and the file would be attributed to a
        # package it has nothing to do with — narrowing, not widening.
        graph = declare(pkg("interop", "dotnet/Interop.Package", kind="dotnet"))
        matrix = affected.select(graph, ["dotnet/Interop.PackageTest/T.cs"], None)
        self.assertEqual(matrix["files"][0]["package"], None)
        self.assertEqual(chosen(matrix), {"interop"})  # via the global rule
        self.assertEqual(reasons(matrix)["interop"], affected.GLOBAL)

    def test_the_longest_declared_path_wins(self) -> None:
        graph = declare(
            pkg("outer", "packages"),
            pkg("inner", "packages/inner"),
        )
        matrix = affected.select(graph, ["packages/inner/src/a.rs"], None)
        self.assertEqual(matrix["files"][0]["package"], "inner")
        self.assertEqual(chosen(matrix), {"inner"})

    def test_a_directory_itself_counts_as_owned(self) -> None:
        graph = declare(pkg("only", "crates/only"))
        matrix = affected.select(graph, ["crates/only"], None)
        self.assertEqual(matrix["files"][0]["package"], "only")

    def test_an_undeclared_dependency_name_is_refused(self) -> None:
        graph = declare(pkg("a", "a", ["ghost"]))
        with self.assertRaises(SystemExit) as caught:
            affected.select(graph, ["a/x.rs"], None)
        self.assertIn("ghost", str(caught.exception))

    def test_a_cycle_terminates(self) -> None:
        # `check.py boundaries` refuses a cycle, so one should never reach here.
        # If one ever does, the closure must still stop rather than hang.
        graph = declare(pkg("a", "a", ["b"]), pkg("b", "b", ["a"]))
        matrix = affected.select(graph, ["a/x.rs"], None)
        self.assertEqual(chosen(matrix), {"a", "b"})

    def test_a_widened_base_selects_everything_with_its_reason(self) -> None:
        matrix = affected.select(GRAPH, [], (affected.GLOBAL, "base ref missing"))
        self.assertEqual(chosen(matrix), EVERY)
        self.assertEqual(matrix["base_widened_everything"], "base ref missing")


class Ownership(unittest.TestCase):
    """The path-prefix predicate on its own."""

    def test_owns(self) -> None:
        cases = [
            ("crates/abi", "crates/abi/src/lib.rs", True),
            ("crates/abi", "crates/abi", True),
            ("crates/abi", "crates/abi-testlib/src/lib.rs", False),
            ("crates/abi", "crates/abistuff", False),
            ("dotnet/Interop.Package", "dotnet/Interop.PackageTest/T.cs", False),
            ("dotnet/Interop.Package", "dotnet/Interop.Package/P.csproj", True),
            ("style", "style/configs/ruff.toml", True),
            ("style", "styles/other", False),
            ("ecosystem", ".github/workflows/ci.yml", False),
        ]
        for prefix, path, want in cases:
            with self.subTest(prefix=prefix, path=path):
                self.assertEqual(affected.owns(prefix, path), want)


DECLARATION = """\
schema = 1

[[package]]
name = "root-pkg"
kind = "rust"
path = "crates/root"
depends = []

[[package]]
name = "mid-pkg"
kind = "rust"
path = "crates/mid"
depends = ["root-pkg"]

[[package]]
name = "leaf-pkg"
kind = "rust"
path = "crates/leaf"
depends = ["mid-pkg"]

[[package]]
name = "island-pkg"
kind = "rust"
path = "crates/island"
depends = []
"""
EVERY_REAL = {"root-pkg", "mid-pkg", "leaf-pkg", "island-pkg"}


class EndToEnd(unittest.TestCase):
    """The script as a subprocess, against a real git repository."""

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="er-affected-"))
        self.addCleanup(self._remove)
        self.git("init", "--initial-branch", "base")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "test")
        self.write("ecosystem/PACKAGES.toml", DECLARATION)
        for name in ("root", "mid", "leaf", "island"):
            self.write(f"crates/{name}/src/lib.rs", "// placeholder\n")
        self.write("README.md", "# fixture\n")
        self.git("add", "-A")
        self.git("commit", "-m", "base")
        self.git("checkout", "-b", "work")

    def _remove(self) -> None:
        # Windows keeps .git/objects read-only, so rmtree needs the bit cleared.
        import shutil
        import stat

        def force(func, path, _exc):  # noqa: ANN001, ANN202
            Path(path).chmod(stat.S_IWRITE)
            func(path)

        shutil.rmtree(self.dir, onerror=force)

    def git(self, *args: str) -> None:
        """Run git in the fixture repository, failing the test if it fails."""
        subprocess.run(["git", *args], cwd=self.dir, check=True, capture_output=True, text=True)

    def write(self, relative: str, text: str) -> None:
        """Write a file in the fixture repository, creating its directories."""
        target = self.dir / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        """Run `affected.py --root <fixture>` and return the completed process."""
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.dir), *args],
            cwd=self.dir,
            capture_output=True,
            text=True,
            check=False,
        )

    def commit(self, relative: str, text: str) -> None:
        """Change one file and commit it."""
        self.write(relative, text)
        self.git("add", "-A")
        self.git("commit", "-m", f"touch {relative}")

    def test_plain_output_is_names_one_per_line(self) -> None:
        self.commit("crates/root/src/lib.rs", "// changed\n")
        done = self.run_script("--base", "base")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(sorted(done.stdout.split()), sorted(["root-pkg", "mid-pkg", "leaf-pkg"]))
        self.assertNotIn("island-pkg", done.stdout)

    def test_json_output_carries_the_selected_set(self) -> None:
        self.commit("crates/leaf/src/lib.rs", "// changed\n")
        done = self.run_script("--base", "base", "--json")
        self.assertEqual(done.returncode, 0, done.stderr)
        payload = json.loads(done.stdout)
        self.assertEqual(payload["selected"], ["leaf-pkg"])
        self.assertEqual(payload["base"], "base")
        self.assertEqual(payload["changed_file_count"], 1)
        self.assertIsNone(payload["widened_everything"])

    def test_json_with_explain_carries_the_whole_matrix(self) -> None:
        self.commit("crates/mid/src/lib.rs", "// changed\n")
        done = self.run_script("--base", "base", "--json", "--explain")
        self.assertEqual(done.returncode, 0, done.stderr)
        payload = json.loads(done.stdout)
        self.assertEqual({p["name"] for p in payload["packages"]}, EVERY_REAL)
        by_name = {p["name"]: p for p in payload["packages"]}
        self.assertFalse(by_name["island-pkg"]["selected"])
        self.assertFalse(by_name["root-pkg"]["selected"])
        self.assertEqual(by_name["leaf-pkg"]["reason"], "reverse")

    def test_explain_prints_a_row_for_every_package_and_every_file(self) -> None:
        self.commit("crates/root/src/lib.rs", "// changed\n")
        done = self.run_script("--base", "base", "--explain")
        self.assertEqual(done.returncode, 0, done.stderr)
        for name in EVERY_REAL:
            self.assertIn(name, done.stdout)
        self.assertIn("crates/root/src/lib.rs", done.stdout)
        self.assertIn("not selected", done.stdout)
        self.assertIn("3 of 4 package(s) selected", done.stdout)

    def test_a_deletion_selects_the_package_it_left(self) -> None:
        # `check-style.sh` filters to ACMR; this must not, because a deleted
        # file is exactly the kind of change a suite has something to say about.
        (self.dir / "crates" / "island" / "src" / "lib.rs").unlink()
        self.git("add", "-A")
        self.git("commit", "-m", "delete")
        done = self.run_script("--base", "base")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("island-pkg", done.stdout.split())

    def test_a_rename_selects_both_ends(self) -> None:
        (self.dir / "crates" / "island" / "src" / "lib.rs").unlink()
        self.write("crates/leaf/src/moved.rs", "// placeholder\n")
        self.git("add", "-A")
        self.git("commit", "-m", "move")
        done = self.run_script("--base", "base")
        names = set(done.stdout.split())
        self.assertIn("island-pkg", names)
        self.assertIn("leaf-pkg", names)

    def test_an_unresolvable_base_selects_everything(self) -> None:
        self.commit("crates/leaf/src/lib.rs", "// changed\n")
        done = self.run_script("--base", "origin/does-not-exist")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(set(done.stdout.split()), EVERY_REAL)
        self.assertIn("is not in this checkout", done.stderr)

    def test_an_empty_diff_selects_everything(self) -> None:
        done = self.run_script("--base", "work")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(set(done.stdout.split()), EVERY_REAL)
        self.assertIn("no file differs", done.stderr)

    def test_a_root_file_selects_everything(self) -> None:
        self.commit("README.md", "# changed\n")
        done = self.run_script("--base", "base", "--json")
        payload = json.loads(done.stdout)
        self.assertEqual(set(payload["selected"]), EVERY_REAL)

    def test_a_root_without_a_declaration_exits_two(self) -> None:
        empty = Path(tempfile.mkdtemp(prefix="er-affected-empty-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(empty, ignore_errors=True))
        done = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(empty)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(done.returncode, 2)
        self.assertIn("PACKAGES.toml", done.stderr)


class AgainstThisRepository(unittest.TestCase):
    """The real declaration, so a package added without a path fails here."""

    def test_every_declared_package_has_a_path_that_exists(self) -> None:
        root = SCRIPT.resolve().parent.parent
        declaration = affected.load(root / "ecosystem" / "PACKAGES.toml")
        missing = [p["name"] for p in declaration["package"] if not (root / p["path"]).is_dir()]
        self.assertEqual(missing, [], f"declared path(s) absent from the tree: {missing}")

    def test_a_status_change_selects_every_rust_consumer(self) -> None:
        root = SCRIPT.resolve().parent.parent
        declaration = affected.load(root / "ecosystem" / "PACKAGES.toml")
        matrix = affected.select(declaration, ["crates/status/src/lib.rs"], None)
        got = chosen(matrix)
        for name in (
            "extendedresearch-status",
            "extendedresearch-abi",
            "extendedresearch-pyo3",
            "extendedresearch-napi",
            "extendedresearch-abi-testlib",
            "extendedresearch-napi-testaddon",
        ):
            self.assertIn(name, got, name)
        # And the packages with no path to status stay out.
        for name in (
            "extendedresearch-clock",
            "extendedresearch-metrology",
            "extendedresearch-style",
        ):
            self.assertNotIn(name, got, name)


if __name__ == "__main__":
    unittest.main()
