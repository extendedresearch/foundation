#!/usr/bin/env bash
# The check `action.yml` runs, kept in a file so `tests/run.sh` can run it
# against logs it must refuse and logs it must accept.
#
#     LOG=cargo-test.log FAIL_ON_IGNORED=true bash prove-tests-ran.sh
#
# Reads the captured output of `cargo test` and exits 1 when the log does not
# exist, when no `test result:` line was printed, when the `passed` counts sum
# to zero, or when the `ignored` counts sum to more than zero and
# FAIL_ON_IGNORED is `true` (the default).
#
# The counts are read by field position in libtest's summary line,
# `test result: ok. N passed; N failed; N ignored; N measured; N filtered out`,
# so `tests/` pins that line as today's cargo prints it.
set -euo pipefail

LOG="${LOG:?LOG names the cargo test log}"
FAIL_ON_IGNORED="${FAIL_ON_IGNORED:-true}"

if [ ! -f "$LOG" ]; then
  echo "::error::$LOG does not exist, so there is no evidence any test ran."
  exit 1
fi
read -r passed failed ignored <<<"$(
  awk '/^test result:/ {p+=$4; f+=$6; i+=$8} END {print p+0, f+0, i+0}' "$LOG"
)"
echo "passed=$passed failed=$failed ignored=$ignored"
if ! grep -q '^test result:' "$LOG"; then
  echo "::error::The run printed no test-result line at all."
  exit 1
fi
if [ "$passed" -eq 0 ]; then
  echo "::error::Zero tests ran. cargo test exits 0 when it has nothing to run."
  exit 1
fi
case "$FAIL_ON_IGNORED" in
  true)
    if [ "$ignored" -ne 0 ]; then
      echo "::error::$ignored test(s) are #[ignore]d, so they did not run and could not fail. Say why in the diff and change this step in the same commit."
      exit 1
    fi
    ;;
  false) ;;
  *)
    echo "::error::fail-on-ignored is '$FAIL_ON_IGNORED'; it takes true or false."
    exit 1
    ;;
esac
