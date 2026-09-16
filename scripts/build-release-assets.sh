#!/usr/bin/env bash
# Build every asset a foundation release attaches, check each one, and write
# SHA256SUMS over them.
#
#     bash scripts/build-release-assets.sh <out-dir>           # any commit
#     bash scripts/build-release-assets.sh <out-dir> v0.1.0    # a release tag
#
# For version X, <out-dir> receives:
#
#     extendedresearch-binding-runtime-X.tgz            npm/binding-runtime
#     ExtendedResearch.Interop.X.nupkg                  dotnet/Interop.Package
#     extendedresearch_conformance-X-py3-none-any.whl   python/conformance
#     extendedresearch_conformance-X.tar.gz             python/conformance
#     SHA256SUMS
#
# `ci.yml` runs this on every pull request without a tag, and `release.yml`
# runs it on a tag and attaches what it writes. It needs cargo, Node and npm, a
# .NET SDK that builds net8.0, and Python 3.11 or later with `build` installed;
# set PYTHON when that interpreter is not `python`.
#
# The steps, and what each one fails on:
#
# 1. `scripts/release-checks.py tree`: a version that differs from another, or
#    from the tag; an npm manifest `npm publish` would refuse or would publish
#    restricted; a LICENSE or NOTICE copy that differs from the root's.
# 2. Build the npm tarball, the .nupkg, the wheel and the sdist.
# 3. `scripts/release-checks.py assets`: an archive missing a file, holding one
#    nobody expected, or stating the wrong name or version.
# 4. Install the tarball into a scratch project and import each export from
#    `node_modules`, then type-check a TypeScript consumer against it.
# 5. Restore the .nupkg from a local folder source and build
#    `dotnet/Interop.PackageTest` against it for netstandard2.1 and net8.0.
#
# **Nothing reaches <out-dir> until every step has passed**, and <out-dir> must
# be empty or absent, so SHA256SUMS covers exactly what this run built. The
# work happens in a new temporary directory, which is left in place and printed
# so a failed run can be read. The NuGet packages folder is a new one inside it:
# NuGet takes a package whose id and version are already in its packages folder
# from there without reading any source, so a folder that had seen this version
# before would build the consumer against that copy instead of the new one.
set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <out-dir> [v<major>.<minor>.<patch>]" >&2
  exit 2
fi
out=$1
tag=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
python=${PYTHON:-python}

# A native Windows program started from Git Bash wants a Windows path; where
# cygpath does not exist the path passes through unchanged.
native() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$1"
  else
    printf '%s\n' "$1"
  fi
}

checks=$(native "$root/scripts/release-checks.py")

if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
  echo "error: $out is not empty, and SHA256SUMS would cover what is already there" >&2
  exit 1
fi

echo "== The tree"
if [ -n "$tag" ]; then
  "$python" "$checks" tree --tag "$tag"
else
  "$python" "$checks" tree
fi
version=$("$python" "$checks" version | tr -d '\r')

work=$(mktemp -d)
echo "Working in $work"
built="$work/built"
mkdir -p "$built" "$work/nuget-feed" "$work/nuget-packages" "$work/npm-consumer"

tarball="extendedresearch-binding-runtime-$version.tgz"
nupkg="ExtendedResearch.Interop.$version.nupkg"
wheel="extendedresearch_conformance-$version-py3-none-any.whl"
sdist="extendedresearch_conformance-$version.tar.gz"

echo "== npm: @extendedresearch/binding-runtime $version"
(
  cd "$root/npm/binding-runtime"
  npm ci --no-audit --no-fund
  npm run build
  npm pack --pack-destination "$(native "$built")"
)

echo "== NuGet: ExtendedResearch.Interop $version"
dotnet pack "$(native "$root/dotnet/Interop.Package")" \
  --configuration Release --output "$(native "$built")"

echo "== Python: extendedresearch-conformance $version"
"$python" -m build --outdir "$(native "$built")" "$(native "$root/python/conformance")"

echo "== What each archive holds"
"$python" "$checks" assets "$(native "$built")" "$version"

echo "== npm: a consumer installs the tarball and imports each export"
cp "$root"/npm/binding-runtime/test/consumer/* "$work/npm-consumer/"
(
  cd "$work/npm-consumer"
  npm install --no-audit --no-fund --no-save "$(native "$built/$tarball")"
  node consumer.mjs
  node "$(native "$root/npm/binding-runtime/node_modules/typescript/bin/tsc")" -p tsconfig.json
  echo "consumer.ts type-checks against the installed declarations"
)

echo "== NuGet: a consumer restores the package from a local folder source"
cp "$built/$nupkg" "$work/nuget-feed/"
dotnet build "$(native "$root/dotnet/Interop.PackageTest")" \
  --configuration Release --no-incremental \
  "-p:RestoreSources=$(native "$work/nuget-feed")" \
  "-p:RestorePackagesPath=$(native "$work/nuget-packages")" \
  -p:RestoreForce=true

echo "== Release assets in $out"
mkdir -p "$out"
assets=("$tarball" "$nupkg" "$wheel" "$sdist")
for asset in "${assets[@]}"; do
  cp "$built/$asset" "$out/"
done
(
  cd "$out"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${assets[@]}" >SHA256SUMS
  else
    shasum -a 256 "${assets[@]}" >SHA256SUMS
  fi
  for file in "${assets[@]}" SHA256SUMS; do
    printf '%10d  %s\n' "$(wc -c <"$file")" "$file"
  done
  echo
  cat SHA256SUMS
)
