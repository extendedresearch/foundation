#!/usr/bin/env bash
# Run every style check this repository has a tool for, scoped to the files a
# change touched.
#
#   bash style/scripts/check-style.sh [--base <ref>] [--mode soft|strict]
#
# **Scoped to changed files, not to the tree.** Adopting this guide costs no
# sweeping rewrite, and a contributor is never asked to fix code they did not
# write. `--base` names what to diff against; CI passes the pull request's base.
#
# **Every row is printed, including the ones that pass.** A report of only
# failures cannot distinguish "checked and clean" from "never checked", and a
# tool that is not installed is a third state that has to be visible rather than
# read as a pass.
#
# **Soft mode reports and exits 0. Strict mode exits non-zero on any failure.**
# A repository moves to strict one language at a time, when that language's
# existing code is already clean.
set -euo pipefail

BASE="${BASE:-origin/main}"
MODE="soft"

while [ $# -gt 0 ]; do
  case "$1" in
    --base)  BASE="$2"; shift 2 ;;
    --mode)  MODE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$MODE" in soft|strict) ;; *) echo "--mode must be soft or strict" >&2; exit 2 ;; esac

# `...` is the merge base, so a change is judged against where it branched
# rather than against whatever main has since become.
if ! git rev-parse --verify --quiet "$BASE" >/dev/null; then
  echo "::warning::base ref '$BASE' is not in this checkout; falling back to HEAD~1"
  BASE="HEAD~1"
fi
mapfile -t CHANGED < <(git diff --name-only --diff-filter=ACMR "$BASE...HEAD")

if [ ${#CHANGED[@]} -eq 0 ]; then
  echo "no files added, copied, modified or renamed against $BASE; nothing to check"
  exit 0
fi

echo "checking ${#CHANGED[@]} changed file(s) against $BASE, mode=$MODE"
echo

FAILED=0
ROWS=()

# filter <extension-regex> -> the changed files matching it, one per line
filter() { printf '%s\n' "${CHANGED[@]}" | grep -E "$1" || true; }

# row <check> <verdict> <detail>
row() { ROWS+=("$(printf '%-22s %-12s %s' "$1" "$2" "$3")"); }

# run <check-name> <files...> -- <command...>
#
# Records `skipped (no files)` when nothing changed for that language and
# `skipped (tool absent)` when the tool is not installed. Neither is a pass.
run() {
  local name="$1"; shift
  local files="$1"; shift
  local tool="$1"
  if [ -z "$files" ]; then row "$name" "skipped" "no changed files"; return; fi
  if ! command -v "$tool" >/dev/null 2>&1; then
    row "$name" "skipped" "$tool is not installed"; return
  fi
  local count; count=$(printf '%s' "$files" | grep -c '' || true)
  if "$@" $files >/tmp/style-$$.log 2>&1; then
    row "$name" "pass" "$count file(s)"
  else
    row "$name" "FAIL" "$count file(s)"
    FAILED=1
    echo "--- $name ---"
    sed -n '1,60p' /tmp/style-$$.log
    echo
  fi
  rm -f /tmp/style-$$.log
}

RUST=$(filter '\.rs$')
PY=$(filter '\.py$')
TS=$(filter '\.(ts|mts|cts|tsx)$')
# Markdown is deliberately absent: this project's prose is hand-wrapped, and
# prettier's `proseWrap` would reflow every document in the tree to no benefit.
# S35 (markdown wraps at 80) is a review rule for that reason.
WEB=$(filter '\.(ts|mts|cts|tsx|js|mjs|cjs|json|ya?ml)$')
CS=$(filter '\.cs$')
SH=$(filter '\.sh$')

# S10: rustfmt takes a file list, so this is genuinely per-file.
run "rust format"   "$RUST" rustfmt --check --edition 2024

# S11: clippy is crate-scoped and cannot be pointed at a file list. It runs over
# the workspace and its findings are reported whole; in soft mode that is an
# annotation rather than a gate, which is why it is safe to run unscoped.
if [ -n "$RUST" ] && command -v cargo >/dev/null 2>&1; then
  if cargo clippy --workspace --all-targets --quiet -- -D warnings >/tmp/clippy-$$.log 2>&1; then
    row "rust lint" "pass" "workspace (not file-scoped)"
  else
    row "rust lint" "FAIL" "workspace (not file-scoped)"
    FAILED=1
    echo "--- rust lint ---"; sed -n '1,60p' /tmp/clippy-$$.log; echo
  fi
  rm -f /tmp/clippy-$$.log
elif [ -n "$RUST" ]; then
  row "rust lint" "skipped" "cargo is not installed"
else
  row "rust lint" "skipped" "no changed files"
fi

run "python format" "$PY"  ruff format --check
run "python lint"   "$PY"  ruff check
run "python types"  "$PY"  mypy --strict
run "web format"    "$WEB" npx --no-install prettier --check
run "typescript"    "$TS"  npx --no-install eslint
run "shell"         "$SH"  shellcheck

# S27: dotnet format wants a project or solution and an --include list.
if [ -n "$CS" ]; then
  if command -v dotnet >/dev/null 2>&1; then
    includes=$(printf '%s ' $CS)
    if dotnet format --verify-no-changes --include $includes >/tmp/fmt-$$.log 2>&1; then
      row "csharp format" "pass" "$(printf '%s' "$CS" | grep -c '') file(s)"
    else
      row "csharp format" "FAIL" "$(printf '%s' "$CS" | grep -c '') file(s)"
      FAILED=1
      echo "--- csharp format ---"; sed -n '1,60p' /tmp/fmt-$$.log; echo
    fi
    rm -f /tmp/fmt-$$.log
  else
    row "csharp format" "skipped" "dotnet is not installed"
  fi
else
  row "csharp format" "skipped" "no changed files"
fi

# Configuration drift: a repository's copy of a shared config must equal
# foundation's, the same rule the licence copies are held to.
if command -v python >/dev/null 2>&1; then
  if python style/scripts/check-configs.py >/tmp/cfg-$$.log 2>&1; then
    row "config drift" "pass" "every copy equal"
  else
    row "config drift" "FAIL" "see below"
    FAILED=1
    echo "--- config drift ---"; cat /tmp/cfg-$$.log; echo
  fi
  rm -f /tmp/cfg-$$.log
else
  row "config drift" "skipped" "python is not installed"
fi

printf '%-22s %-12s %s\n' "check" "verdict" "detail"
printf '%-22s %-12s %s\n' "----------------------" "------------" "------------------------"
for r in "${ROWS[@]}"; do echo "$r"; done
echo

if [ "$FAILED" -eq 0 ]; then
  echo "style: clean"
  exit 0
fi

if [ "$MODE" = "strict" ]; then
  echo "style: findings above, and mode is strict"
  exit 1
fi

echo "::warning::style findings above. Mode is soft, so this does not fail the build."
exit 0
