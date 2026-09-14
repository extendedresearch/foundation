#!/usr/bin/env python3
"""The checks a foundation release makes on the tree and on what it built.

    python scripts/release-checks.py tree                  # before building
    python scripts/release-checks.py tree --tag v0.1.0     # ... on a tag
    python scripts/release-checks.py version               # the version, alone
    python scripts/release-checks.py assets <dir> <version>

`scripts/build-release-assets.sh` runs all of them. Each check prints every row
it compared, passing rows included, so a green run shows what was checked and
not only that nothing failed. Standard library only; `tomllib` is why the floor
is Python 3.11.

# tree

**Every version is the same version.** A release has one number, and a crate,
a wheel, a tarball and a .nupkg that disagree about it are four releases under
one tag. The rows are each workspace crate and each versioned requirement on an
internal path dependency (both from `cargo metadata`), the conformance runner's
`pyproject.toml` and `__version__`, the npm package's manifest and lockfile, the
NuGet project, and the exact version the NuGet consumer test restores. Given a
tag, the tag must be `v<major>.<minor>.<patch>` and name that version.

**Every copy of LICENSE and NOTICE is the root's.** Each asset is built from its
own directory, so the root files reach a user only as a copy, and Apache-2.0
requires NOTICE to travel with a redistribution. The directories an asset or a
crate is built from must each carry both; any other tracked copy is compared
too. Line endings are normalised, so a checkout's `eol` setting is not drift.

# assets

**Each archive holds exactly the files it should.** The npm tarball's `dist/`
is derived from `src/*.ts` and the .nupkg's content files from
`dotnet/Interop/*.cs`, so a new source file needs no edit here, and a file left
over from an earlier build fails the run instead of shipping. The wheel and the
sdist are checked for the files that must be there and for nothing outside the
package, because their metadata files are setuptools' to choose.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tarfile
import tomllib
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NPM = ROOT / "npm" / "binding-runtime"
INTEROP = ROOT / "dotnet" / "Interop"
PACKAGE_PROJECT = ROOT / "dotnet" / "Interop.Package" / "Interop.Package.csproj"
PACKAGE_TEST_PROJECT = ROOT / "dotnet" / "Interop.PackageTest" / "Interop.PackageTest.csproj"
CONFORMANCE = ROOT / "python" / "conformance"

NPM_NAME = "@extendedresearch/binding-runtime"
NUGET_ID = "ExtendedResearch.Interop"
PYTHON_DIST = "extendedresearch_conformance"

LICENCE_COPIES = (
    "crates/abi",
    "crates/napi",
    "crates/pyo3",
    "dotnet/Interop.Package",
    "npm/binding-runtime",
    "python/conformance",
)

# The parts NuGet writes into every package beside the package's own files.
NUPKG_PARTS = {"[Content_Types].xml", "_rels/.rels"}
NUPKG_PARTS_PREFIX = "package/services/metadata/core-properties/"
NUPKG_CONTENT = f"contentFiles/cs/any/{NUGET_ID}/"


class Table:
    """Rows of `(subject, finding, passed)`, printed with the passing rows."""

    def __init__(self, title: str) -> None:
        self.title = title
        self.rows: list[tuple[str, str, bool]] = []

    def add(self, subject: str, finding: str, passed: bool) -> None:
        self.rows.append((subject, finding, passed))

    def report(self) -> bool:
        print(self.title)
        if not self.rows:
            print("  FAIL  nothing was compared")
            print()
            return False
        width = max(len(subject) for subject, _, _ in self.rows)
        for subject, finding, passed in self.rows:
            print(f"  {'pass' if passed else 'FAIL'}  {subject.ljust(width)}  {finding}")
        failed = sum(1 for _, _, passed in self.rows if not passed)
        print(f"  {len(self.rows) - failed} passed, {failed} failed")
        print()
        return failed == 0


def local_name(element: ET.Element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def normalised(data: bytes) -> str:
    return data.decode("utf-8-sig").replace("\r\n", "\n")


def read_normalised(path: Path) -> str | None:
    try:
        return normalised(path.read_bytes())
    except OSError:
        return None


def show(title: str, members: dict[str, int]) -> None:
    print(title)
    for name in sorted(members):
        print(f"  {members[name]:>9}  {name}")
    print()


def exact(members: set[str], expected: set[str], table: Table) -> None:
    """A row per expected file, and a failing row per file nobody expected."""
    for name in sorted(expected):
        table.add(name, "present" if name in members else "missing", name in members)
    for name in sorted(members - expected):
        table.add(name, "not expected", False)


# -- tree ---------------------------------------------------------------------


def declared_versions() -> list[tuple[str, str]]:
    """Every place the tree states the release version, as `(where, version)`."""
    found: list[tuple[str, str]] = []

    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    members = set(metadata["workspace_members"])
    for package in sorted(metadata["packages"], key=lambda one: one["name"]):
        if package["id"] not in members:
            continue
        found.append((f"crate {package['name']}", package["version"]))
        for dependency in package["dependencies"]:
            # A path dependency with no `version` has the requirement `*`; one
            # that states a version must state this release's.
            if dependency.get("path") is None or dependency["req"] == "*":
                continue
            found.append(
                (
                    f"crate {package['name']}: requirement on {dependency['name']}",
                    dependency["req"].lstrip("^="),
                )
            )

    pyproject = tomllib.loads((CONFORMANCE / "pyproject.toml").read_text(encoding="utf-8"))
    found.append(("python/conformance/pyproject.toml", pyproject["project"]["version"]))
    init = (CONFORMANCE / "src" / PYTHON_DIST / "__init__.py").read_text(encoding="utf-8")
    match = re.search(r'^__version__ = "([^"]*)"$', init, re.MULTILINE)
    found.append((f"{PYTHON_DIST}.__version__", match.group(1) if match else "(absent)"))

    manifest = json.loads((NPM / "package.json").read_text(encoding="utf-8"))
    found.append(("npm/binding-runtime/package.json", manifest["version"]))
    lock = json.loads((NPM / "package-lock.json").read_text(encoding="utf-8"))
    found.append(("npm/binding-runtime/package-lock.json", lock["version"]))
    found.append(
        ("npm/binding-runtime/package-lock.json: root package", lock["packages"][""]["version"])
    )

    versions = [
        element.text or ""
        for element in ET.parse(PACKAGE_PROJECT).getroot().iter()
        if local_name(element) == "Version"
    ]
    found.append(
        (
            "dotnet/Interop.Package: Version",
            versions[0] if len(versions) == 1 else f"({len(versions)} Version elements)",
        )
    )
    references = [
        element.get("Version", "")
        for element in ET.parse(PACKAGE_TEST_PROJECT).getroot().iter()
        if local_name(element) == "PackageReference" and element.get("Include") == NUGET_ID
    ]
    if len(references) != 1:
        restored = f"({len(references)} references to {NUGET_ID})"
    elif re.fullmatch(r"\[[^,\]]+\]", references[0]):
        restored = references[0][1:-1]
    else:
        restored = f"{references[0]} (not an exact [version])"
    found.append(("dotnet/Interop.PackageTest: restores", restored))
    return found


def check_versions(tag: str | None) -> bool:
    table = Table("Versions" + (f": every one equal to the tag {tag}" if tag else ": every one equal"))
    found = declared_versions()
    expected = found[0][1]
    if tag is not None:
        match = re.fullmatch(r"v(\d+\.\d+\.\d+)", tag)
        table.add(
            f"tag {tag}",
            f"names {match.group(1)}" if match else "is not v<major>.<minor>.<patch>",
            match is not None,
        )
        if match:
            expected = match.group(1)
    for where, version in found:
        passed = version == expected
        table.add(where, version if passed else f"{version}, expected {expected}", passed)
    return table.report()


def check_licences() -> bool:
    table = Table("LICENSE and NOTICE: every copy equal to the root's")
    tracked = subprocess.run(
        ["git", "ls-files", "-z", "--", "*LICENSE", "*NOTICE"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split("\0")
    directories = set(LICENCE_COPIES)
    directories.update(Path(path).parent.as_posix() for path in tracked if "/" in path)
    for directory in sorted(directories):
        for name in ("LICENSE", "NOTICE"):
            root = read_normalised(ROOT / name)
            copy = read_normalised(ROOT / directory / name)
            subject = f"{directory}/{name}"
            if copy is None:
                table.add(subject, "missing", False)
            else:
                same = root is not None and copy == root
                table.add(subject, "equal" if same else "differs; copy the root file over it", same)
    return table.report()


# -- assets -------------------------------------------------------------------


def check_npm(directory: Path, version: str) -> bool:
    tarball = directory / f"extendedresearch-binding-runtime-{version}.tgz"
    table = Table(f"npm tarball {tarball.name}")
    if not tarball.is_file():
        table.add(tarball.name, "missing", False)
        return table.report()
    stems = sorted(path.stem for path in (NPM / "src").glob("*.ts"))
    with tarfile.open(tarball, "r:gz") as archive:
        members = {member.name: member.size for member in archive.getmembers() if member.isfile()}
        show(f"Contents of {tarball.name}", members)
        expected = {"package/package.json", "package/README.md", "package/LICENSE", "package/NOTICE"}
        expected.update(f"package/dist/{stem}{suffix}" for stem in stems for suffix in (".js", ".d.ts"))
        exact(set(members), expected, table)

        if "package/package.json" in members:
            packed = json.loads(archive.extractfile("package/package.json").read())
            table.add("package.json name", packed.get("name", ""), packed.get("name") == NPM_NAME)
            table.add("package.json version", packed.get("version", ""), packed.get("version") == version)
        for stem in stems:
            name = f"package/dist/{stem}.js"
            if name not in members:
                continue
            script = normalised(archive.extractfile(name).read())
            # A static import or re-export, a dynamic import, or a require.
            imports = re.findall(
                r"^\s*import[\s{*\"']|^\s*export\s[^;]*\bfrom\s*[\"']|\bimport\s*\(|\brequire\s*\(",
                script,
                re.MULTILINE,
            )
            table.add(f"dist/{stem}.js imports", "nothing" if not imports else f"{len(imports)}", not imports)
    return table.report()


def check_nuget(directory: Path, version: str) -> bool:
    package = directory / f"{NUGET_ID}.{version}.nupkg"
    table = Table(f"NuGet package {package.name}")
    if not package.is_file():
        table.add(package.name, "missing", False)
        return table.report()
    sources = sorted(INTEROP.glob("*.cs"))
    nuspec = f"{NUGET_ID}.nuspec"
    with zipfile.ZipFile(package) as archive:
        members = {info.filename: info.file_size for info in archive.infolist() if not info.is_dir()}
        show(f"Contents of {package.name}", members)
        own = {
            name
            for name in members
            if name not in NUPKG_PARTS and not name.startswith(NUPKG_PARTS_PREFIX)
        }
        expected = {nuspec, "README.md", "LICENSE", "NOTICE"}
        expected.update(NUPKG_CONTENT + source.name for source in sources)
        exact(own, expected, table)

        for source in sources:
            name = NUPKG_CONTENT + source.name
            if name in members:
                same = normalised(archive.read(name)) == normalised(source.read_bytes())
                table.add(f"{name} content", "equal to dotnet/Interop" if same else "differs", same)

        if nuspec in members:
            metadata = ET.fromstring(archive.read(nuspec))

            def text(tag: str) -> str:
                values = [element.text or "" for element in metadata.iter() if local_name(element) == tag]
                return values[0] if len(values) == 1 else f"({len(values)} <{tag}> elements)"

            table.add("nuspec id", text("id"), text("id") == NUGET_ID)
            table.add("nuspec version", text("version"), text("version") == version)
            table.add(
                "nuspec developmentDependency",
                text("developmentDependency"),
                text("developmentDependency") == "true",
            )
            dependencies = [element for element in metadata.iter() if local_name(element) == "dependency"]
            table.add("nuspec dependencies", str(len(dependencies)), not dependencies)
            content_files = [
                files
                for element in metadata.iter()
                if local_name(element) == "contentFiles"
                for files in element
                if local_name(files) == "files"
            ]
            table.add("nuspec contentFiles entries", str(len(content_files)), bool(content_files))
            for files in content_files:
                action = files.get("buildAction", "")
                table.add(f"contentFiles {files.get('include')}", f"buildAction {action}", action == "Compile")
    return table.report()


def check_wheel(directory: Path, version: str) -> bool:
    wheel = directory / f"{PYTHON_DIST}-{version}-py3-none-any.whl"
    table = Table(f"Wheel {wheel.name}")
    if not wheel.is_file():
        table.add(wheel.name, "missing", False)
        return table.report()
    info = f"{PYTHON_DIST}-{version}.dist-info/"
    modules = sorted(path.name for path in (CONFORMANCE / "src" / PYTHON_DIST).glob("*.py"))
    with zipfile.ZipFile(wheel) as archive:
        members = {item.filename: item.file_size for item in archive.infolist() if not item.is_dir()}
        show(f"Contents of {wheel.name}", members)
        required = {f"{PYTHON_DIST}/{module}" for module in modules}
        required.update(info + name for name in ("METADATA", "licenses/LICENSE", "licenses/NOTICE"))
        for name in sorted(required):
            table.add(name, "present" if name in members else "missing", name in members)
        for name in sorted(members):
            if not name.startswith((f"{PYTHON_DIST}/", info)):
                table.add(name, "outside the package and its metadata", False)
        if info + "METADATA" in members:
            stated = re.search(r"^Version: (.+)$", normalised(archive.read(info + "METADATA")), re.MULTILINE)
            found = stated.group(1).strip() if stated else "(absent)"
            table.add("METADATA Version", found, found == version)
    return table.report()


def check_sdist(directory: Path, version: str) -> bool:
    sdist = directory / f"{PYTHON_DIST}-{version}.tar.gz"
    table = Table(f"Source distribution {sdist.name}")
    if not sdist.is_file():
        table.add(sdist.name, "missing", False)
        return table.report()
    top = f"{PYTHON_DIST}-{version}/"
    modules = sorted(path.name for path in (CONFORMANCE / "src" / PYTHON_DIST).glob("*.py"))
    with tarfile.open(sdist, "r:gz") as archive:
        members = {member.name: member.size for member in archive.getmembers() if member.isfile()}
        show(f"Contents of {sdist.name}", members)
        required = {top + name for name in ("PKG-INFO", "pyproject.toml", "README.md", "LICENSE", "NOTICE")}
        required.update(f"{top}src/{PYTHON_DIST}/{module}" for module in modules)
        for name in sorted(required):
            table.add(name, "present" if name in members else "missing", name in members)
        for name in sorted(members):
            if not name.startswith(top):
                table.add(name, f"outside {top}", False)
    return table.report()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="The checks a foundation release makes.")
    commands = parser.add_subparsers(dest="command", required=True)
    tree = commands.add_parser("tree", help="versions and licence copies, before building")
    tree.add_argument("--tag", help="the release tag, v<major>.<minor>.<patch>")
    commands.add_parser("version", help="print the release version, with no newline")
    assets = commands.add_parser("assets", help="what each built archive holds")
    assets.add_argument("directory", type=Path)
    assets.add_argument("version")
    args = parser.parse_args(argv)

    if args.command == "version":
        # No newline: on Windows a text-mode newline is `\r\n`, and a shell's
        # `$(...)` strips only the `\n`.
        sys.stdout.write(json.loads((NPM / "package.json").read_text(encoding="utf-8"))["version"])
        return 0
    if args.command == "tree":
        results = [check_versions(args.tag), check_licences()]
    else:
        results = [
            check(args.directory, args.version)
            for check in (check_npm, check_nuget, check_wheel, check_sdist)
        ]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
