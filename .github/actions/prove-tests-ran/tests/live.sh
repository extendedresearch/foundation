#!/usr/bin/env bash
# Run `prove-tests-ran.sh` against what the cargo on this machine prints.
#
#     bash .github/actions/prove-tests-ran/tests/live.sh
#
# **The logs beside this file cannot notice cargo changing.** They are captured
# output, so a cargo release that moves the fields of libtest's summary line
# leaves them, and `run.sh`, exactly as they were, while the script reads the
# wrong field out of every real log. This runs `cargo test` on `live/`, a crate
# with two tests that pass and one that is ignored, and requires the script to
# read exactly `passed=2 failed=0 ignored=1` from that output: refused as it
# stands, accepted with FAIL_ON_IGNORED=false.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script="$here/../prove-tests-ran.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo --version
set +e
# No `--locked`: foundation commits no lockfile, and `live/` has no dependency
# to resolve.
cargo test --manifest-path "$here/live/Cargo.toml" --target-dir "$work/target" > "$work/cargo-test.log" 2>&1
status=$?
set -e
if [ "$status" -ne 0 ]; then
  cat "$work/cargo-test.log"
  echo "cargo test on live/ exited $status, so there is no log to read"
  exit 1
fi

failures=0
# check NAME EXPECT(accept|refuse) FAIL_ON_IGNORED
check() {
  local name=$1 expect=$2 fail_on_ignored=$3 out status verdict=ok
  set +e
  out=$(LOG="$work/cargo-test.log" FAIL_ON_IGNORED="$fail_on_ignored" bash "$script" 2>&1)
  status=$?
  set -e
  if [ "$expect" = accept ] && [ "$status" -ne 0 ]; then verdict=FAILED; fi
  if [ "$expect" = refuse ] && [ "$status" -eq 0 ]; then verdict=FAILED; fi
  case "$out" in
    *"passed=2 failed=0 ignored=1"*) ;;
    *) verdict=FAILED ;;
  esac
  if [ "$verdict" = FAILED ]; then failures=$((failures + 1)); fi
  printf '%-6s  %-26s  %-6s  exit %d  %s\n' "$verdict" "$name" "$expect" "$status" "$(printf '%s\n' "$out" | head -n 1)"
}

check "this cargo's output" refuse true
check "the same, ignored allowed" accept false

echo "2 cases, $failures failed"
if [ "$failures" -ne 0 ]; then
  grep '^test result:' "$work/cargo-test.log"
  exit 1
fi
