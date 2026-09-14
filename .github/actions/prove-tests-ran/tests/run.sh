#!/usr/bin/env bash
# Run `prove-tests-ran.sh` against every log in this directory and print one
# row per case, the passing ones included.
#
#     bash .github/actions/prove-tests-ran/tests/run.sh
#
# **A guard that never refuses anything looks exactly like one that works**:
# every real log CI hands the action is one it should accept. These cases are
# the ones it must refuse, beside two it must accept, and each also names a line
# the script must print, so a refusal for the wrong reason fails too.
#
# The logs are real `cargo test` output, captured with cargo 1.97.1 from a
# scratch crate: a library with no unit tests beside an integration test with
# two (`sums-across-binaries.log`), an integration test with one passing and one
# `#[ignore]`d test (`one-ignored.log`), the library alone
# (`zero-passed.log`), and an integration test that does not compile
# (`no-result-line.log`). Only the build paths were rewritten, to the shape a
# Linux runner prints. These logs catch an edit that breaks the script; being
# captured, they cannot notice cargo changing its summary line, which is what
# `live.sh` is for.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script="$here/../prove-tests-ran.sh"
cases=0
failures=0

# check NAME EXPECT(accept|refuse) LOG FAIL_ON_IGNORED MUST_PRINT
check() {
  local name=$1 expect=$2 log=$3 fail_on_ignored=$4 must_print=$5 out status verdict
  cases=$((cases + 1))
  set +e
  out=$(LOG="$log" FAIL_ON_IGNORED="$fail_on_ignored" bash "$script" 2>&1)
  status=$?
  set -e
  verdict=ok
  if [ "$expect" = accept ] && [ "$status" -ne 0 ]; then verdict=FAILED; fi
  if [ "$expect" = refuse ] && [ "$status" -eq 0 ]; then verdict=FAILED; fi
  case "$out" in
    *"$must_print"*) ;;
    *) verdict=FAILED ;;
  esac
  if [ "$verdict" = FAILED ]; then failures=$((failures + 1)); fi
  # The script's last line, with the annotation prefix removed so a refusal
  # this run expects is not reported to GitHub as an error.
  printf '%-6s  %-34s  %-6s  exit %d  %s\n' "$verdict" "$name" "$expect" "$status" "$(printf '%s\n' "$out" | tail -n 1 | sed 's/^::error:://')"
}

check "sums every binary's counts" accept "$here/sums-across-binaries.log" true "passed=2 failed=0 ignored=0"
check "zero passed" refuse "$here/zero-passed.log" true "Zero tests ran"
check "an ignored test" refuse "$here/one-ignored.log" true "1 test(s) are #[ignore]d"
check "an ignored test, allowed" accept "$here/one-ignored.log" false "passed=1 failed=0 ignored=1"
check "no test-result line" refuse "$here/no-result-line.log" true "no test-result line"
check "no log" refuse "$here/does-not-exist.log" true "does not exist"
check "fail-on-ignored neither value" refuse "$here/sums-across-binaries.log" maybe "takes true or false"

echo "$cases cases, $failures failed"
if [ "$cases" -eq 0 ] || [ "$failures" -ne 0 ]; then
  exit 1
fi
