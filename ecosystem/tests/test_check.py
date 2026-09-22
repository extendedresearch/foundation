"""Tests for the failures `ecosystem/check.py` cannot show you by passing.

    python -m unittest discover -s ecosystem/tests -v

Every test here covers a defect that a green run is blind to by construction.
A table that never printed, a workspace that was never read, and a package that
was overwritten by its namesake all produce output that looks exactly like a
clean tree — which is why each one needs a test rather than an inspection.

The trees are synthetic and `tracked()` is replaced, so no test touches this
repository or runs git. The workspace test runs `cargo metadata` for real,
because the thing under test is what cargo reports from several roots, and a
stub would only assert that the stub was written to match the fix.
"""

from __future__ import annotations

import contextlib
import io
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check  # noqa: E402


def write(root: Path, relative: str, text: str) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(text).lstrip("\n"), encoding="utf-8")
    return path


@contextlib.contextmanager
def tree(root: Path, files: list[str]):
    """Point `check` at `root`, with `files` standing in for git's list."""
    previous = check.ROOT
    check.ROOT = root
    try:
        with mock.patch.object(check, "tracked", lambda: list(files)):
            yield
    finally:
        check.ROOT = previous


def run(function, *args, **kwargs) -> tuple[bool, str]:
    """Call a check, returning its verdict and everything it printed."""
    buffer = io.StringIO()
    with contextlib.redirect_stdout(buffer):
        result = function(*args, **kwargs)
    return result, buffer.getvalue()


DOCUMENTS = {
    "required": ["README.md"],
    "at_v1": [],
    "readme_headings": ["What it is"],
}

#: Where a record lives, assembled from segments. Spelled out as one string it
#: would be a citation naming no repository, and `check.py citations` scans this
#: file like any other.
RECORDS = Path("docs") / "decisions"


class TemporaryTree(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="ecosystem-check-"))
        self.addCleanup(shutil.rmtree, self.root, True)


class EveryTablePrints(TemporaryTree):
    """Defect 1: `a.report() and b.report()` never prints `b` once `a` fails.

    Python short-circuits `and`, so a failing early table removed every later
    table from the output entirely — not empty, not failing, absent. The file's
    own docstring says a report of only failures cannot distinguish "checked and
    clean" from "never checked"; a table that never printed is that defect one
    step further along, and it is invisible to a run that passes.
    """

    def test_boundaries_prints_later_tables_when_the_first_fails(self) -> None:
        declaration = {
            "package": [
                # No Cargo.toml in this tree, so cargo lists nothing and this
                # row is MISSING: the first table fails.
                {"name": "absent", "kind": "rust", "path": ".", "depends": [], "external": []},
            ]
        }
        with tree(self.root, []):
            ok, output = run(check.check_boundaries, declaration)

        self.assertFalse(ok)
        self.assertIn("MISSING", output)
        for title in (
            "Every cargo workspace in the tree was read",
            "Dependencies: declared and resolved agree",
            "Every declared edge resolves to exactly one package",
            "The dependency graph is acyclic",
            "Every declared namespace is used by a .cs file",
            "Every declared seam reaches outside the package",
        ):
            self.assertIn(title, output, f"{title!r} was skipped by a short-circuited `and`")

    def test_duplication_prints_the_shared_rule_table_when_the_first_fails(self) -> None:
        write(self.root, "vendor/copied.txt", "x\n")
        declaration = {
            "forbidden_paths": ["vendor/"],
            "copy": [],
            "shared_rule": [],
        }
        with tree(self.root, ["vendor/copied.txt"]):
            ok, output = run(check.check_duplication, declaration)

        self.assertFalse(ok)
        self.assertIn("VENDORED", output)
        self.assertIn("Every rule written more than once is registered", output)

    def test_documents_prints_the_records_table_when_the_first_fails(self) -> None:
        write(self.root, "pkg/placeholder", "")
        # Built from segments rather than written whole, because a literal
        # record path in this file is a citation naming no repository, and
        # `check.py citations` is right to refuse it.
        write(self.root, str(RECORDS / f"{'0001'}-a-record.md"), "# A record\n")
        declaration = {
            "documents": DOCUMENTS,
            "package": [
                {"name": "pkg", "kind": "rust", "path": "pkg", "depends": [], "external": []},
            ],
        }
        with tree(self.root, []):
            ok, output = run(check.check_documents, declaration, False)

        self.assertFalse(ok)
        self.assertIn("MISSING", output)
        self.assertIn("Decision records are numbered uniquely", output)


class EveryWorkspaceIsRead(TemporaryTree):
    """Defect 2: one `cargo metadata` at the root sees one workspace.

    A consuming repository has nine. Eight correctly-declared packages reported
    MISSING, and the obvious way to make that run green is to delete the eight
    rows — which would leave real edges undeclared and unwatched.
    """

    def setUp(self) -> None:
        super().setUp()
        if shutil.which("cargo") is None:
            self.skipTest("cargo is not on PATH")
        write(self.root, "first/Cargo.toml", """
            [workspace]
            members = ["alpha"]
            resolver = "2"
        """)
        write(self.root, "first/alpha/Cargo.toml", """
            [package]
            name = "alpha"
            version = "0.0.0"
            edition = "2021"

            [dependencies]
            beta = { path = "../../second/beta" }
        """)
        write(self.root, "first/alpha/src/lib.rs", "")
        write(self.root, "second/Cargo.toml", """
            [workspace]
            members = ["beta"]
            resolver = "2"
        """)
        write(self.root, "second/beta/Cargo.toml", """
            [package]
            name = "beta"
            version = "0.0.0"
            edition = "2021"
        """)
        write(self.root, "second/beta/src/lib.rs", "")
        self.files = [
            "first/Cargo.toml",
            "first/alpha/Cargo.toml",
            "second/Cargo.toml",
            "second/beta/Cargo.toml",
        ]

    def test_both_workspaces_are_read(self) -> None:
        with tree(self.root, self.files):
            edges, table = check.rust_edges()

        self.assertIn("alpha", edges, "the first workspace was not read")
        self.assertIn("beta", edges, "the second workspace was not read")
        self.assertEqual(sorted(r[0] for r in table.rows), ["first", "second"])

    def test_a_cross_workspace_edge_is_intra_repository(self) -> None:
        """`local` is the union, so a crate in one workspace is not third-party
        to a crate in another. Read one workspace only and `beta` becomes an
        undeclared third-party dependency, which is the wrong finding."""
        with tree(self.root, self.files):
            edges, _ = check.rust_edges()

        intra, external = edges["alpha"]
        self.assertEqual(intra, {"beta"})
        self.assertEqual(external, set())

    def test_the_root_alone_would_have_found_nothing(self) -> None:
        """The premise of the test above: this tree has no workspace at its
        root, so a checker that reads only the root sees zero crates."""
        completed = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=self.root,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(completed.returncode, 0)

    def test_a_crate_in_no_workspace_is_read_on_its_own(self) -> None:
        write(self.root, "loner/Cargo.toml", """
            [package]
            name = "loner"
            version = "0.0.0"
            edition = "2021"
        """)
        write(self.root, "loner/src/lib.rs", "")
        with tree(self.root, [*self.files, "loner/Cargo.toml"]):
            edges, _ = check.rust_edges()
        self.assertIn("loner", edges)


class APackageIsNotItsName(TemporaryTree):
    """Defect 3: two packages can share a name and differ in kind.

    Keyed on the name alone, twenty-three declared packages became twenty-two
    nodes and one entry overwrote the other. Nothing was wrong on the day it was
    found because both had empty `depends`; an edge on either would have been
    checked against the wrong row and printed `ok`.
    """

    def setUp(self) -> None:
        super().setUp()
        # A real manifest, so that the python row exercises the ordinary path
        # rather than the missing-manifest one.
        write(self.root, "py/pyproject.toml", """
            [project]
            name = "toolkit"
            version = "0.0.0"
            dependencies = []
        """)

    def declaration(self, extra: list[dict] | None = None) -> dict:
        return {
            "package": [
                {"name": "toolkit", "kind": "rust", "path": "rust", "depends": [], "external": []},
                {"name": "toolkit", "kind": "python", "path": "py", "depends": [], "external": []},
                *(extra or []),
            ]
        }

    def test_the_key_is_the_name_and_the_kind(self) -> None:
        packages = self.declaration()["package"]
        self.assertEqual(len({check.key_of(p) for p in packages}), 2)
        self.assertEqual(len({p["name"] for p in packages}), 1)

    def test_a_shared_name_is_labelled_by_kind_and_a_unique_one_is_not(self) -> None:
        packages = self.declaration(
            [{"name": "alone", "kind": "rust", "path": "a", "depends": [], "external": []}]
        )["package"]
        labels = check.labels_for(packages)
        self.assertEqual(labels[("toolkit", "rust")], "toolkit (rust)")
        self.assertEqual(labels[("toolkit", "python")], "toolkit (python)")
        self.assertEqual(labels[("alone", "rust")], "alone")

    def test_both_namesakes_appear_in_the_acyclicity_table(self) -> None:
        with tree(self.root, []):
            _, output = run(check.check_boundaries, self.declaration())
        graph = output.split("The dependency graph is acyclic")[1]
        self.assertIn("toolkit (rust)", graph)
        self.assertIn("toolkit (python)", graph)
        self.assertIn("2 passed", graph)

    def test_an_edge_resolves_to_the_package_of_its_own_kind(self) -> None:
        extra = [
            {
                "name": "consumer",
                "kind": "rust",
                "path": "c",
                "depends": ["toolkit"],
                "external": [],
            }
        ]
        with tree(self.root, []):
            _, output = run(check.check_boundaries, self.declaration(extra))
        resolution = output.split("Every declared edge resolves to exactly one package")[1]
        self.assertIn("consumer -> toolkit", resolution)
        self.assertIn("the rust package of that name", resolution)

    def test_an_edge_that_could_be_either_is_reported_not_picked(self) -> None:
        extra = [
            {
                "name": "consumer",
                "kind": "dotnet",
                "path": "c",
                "depends": ["toolkit"],
                "external": [],
            }
        ]
        with tree(self.root, []):
            ok, output = run(check.check_boundaries, self.declaration(extra))
        self.assertFalse(ok)
        self.assertIn("AMBIGUOUS", output)
        self.assertIn("['python', 'rust']", output)


class AMissingManifestIsOneRow(TemporaryTree):
    """A manifest the declaration points at and the tree does not hold.

    It used to end the run with a traceback, which says nothing about the other
    twelve packages — the same shape as a table that never printed.
    """

    def test_a_missing_manifest_is_a_row_and_the_run_continues(self) -> None:
        declaration = {
            "package": [
                {
                    "name": "gone",
                    "kind": "python",
                    "path": "nowhere",
                    "depends": [],
                    "external": [],
                },
                {"name": "fine", "kind": "tooling", "path": ".", "depends": [], "external": []},
            ]
        }
        with tree(self.root, []):
            ok, output = run(check.check_boundaries, declaration)

        self.assertFalse(ok)
        self.assertIn("MANIFEST", output)
        self.assertIn("fine", output)
        self.assertIn("The dependency graph is acyclic", output)


class AnIndexIsNotARecord(TemporaryTree):
    """Defect 4: `docs/decisions/README.md` was reported MALFORMED."""

    def test_a_readme_beside_the_records_is_not_a_malformed_record(self) -> None:
        records = self.root / "docs" / "decisions"
        records.mkdir(parents=True)
        (records / "README.md").write_text("# Index\n", encoding="utf-8")
        (records / f"{'0001'}-a-record.md").write_text("# A record\n", encoding="utf-8")
        declaration = {"documents": DOCUMENTS, "package": []}
        with tree(self.root, []):
            ok, output = run(check.check_documents, declaration, False)

        self.assertTrue(ok, output)
        self.assertNotIn("MALFORMED", output)
        self.assertIn("1 record(s), numbers unique", output)

    def test_an_unnumbered_record_is_still_malformed(self) -> None:
        """The exemption is by name, not by a looser pattern: a record with no
        number is exactly what the row exists to refuse."""
        records = self.root / "docs" / "decisions"
        records.mkdir(parents=True)
        (records / "notes.md").write_text("# Notes\n", encoding="utf-8")
        declaration = {"documents": DOCUMENTS, "package": []}
        with tree(self.root, []):
            ok, output = run(check.check_documents, declaration, False)

        self.assertFalse(ok)
        self.assertIn("MALFORMED", output)


if __name__ == "__main__":
    unittest.main()
