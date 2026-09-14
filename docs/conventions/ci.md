# CI

## How the jobs are split

**One job per compiled graph.** Two steps belong in the same job when they need
the same toolchain and the same set of crates compiled; they belong in
different jobs when either differs.

The split is not about how long a job takes or how the results read. It is
about what each step has to build before it can start. Two steps in one job
share a `target/` directory, so the second reuses everything the first
compiled. The same two steps in two jobs compile the graph twice, on two
runners, and the second runner's cache is warm only if somebody configured it
to be.

So `cargo fmt`, `cargo clippy --all-targets`, `cargo doc` and `cargo test
--workspace` are one job: one stable toolchain, one graph, and clippy has
already built what the test run needs. A run on a different toolchain is a
different job, because it shares nothing — a separate `target/` directory and a
separate compile of every dependency.

The cost of the rule is that a formatting failure and a test failure arrive in
the same red job rather than two, so the summary says less about which failed.
The step names inside the job say it instead, and the trade buys one compile of
the workspace per toolchain rather than four.

## How the jobs are named

**A job's name must be true the first time the job fails, before anybody opens
it.** A reader sees the name in a pull request's check list and nowhere else
until they click.

That gives one rule: **one claim per name.** A name that joins two claims is
half wrong whenever the job fails, and a name that is half wrong is read as
noise. `lint-and-test` failing says either that the code is unformatted or that
a test broke, and the reader has to open the job to find out which — which is
exactly the work the name was supposed to save.

A name that states the job's subject rather than its steps is true in both
cases:

| Job | What it is | True when it fails |
|---|---|---|
| `check` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo doc`, and the suite, on the OS matrix | Something in the tree does not check out |
| `header` | Regenerate the C header with the pinned cbindgen and compare it with the committed one | The header drifted |
| `audit` | `cargo audit` against the RustSec database | An advisory matches a dependency |

`check` carries the matrix in its display name — `check (ubuntu-latest)` — so
the row says which platform, and the platform is the only thing the matrix
varies.

Windows and macOS are in that matrix beside Linux because every consumer loads
a native library on all three, and pointer width, allocator behaviour, symbol
resolution and library naming are exactly what differs between them. A binding
bug that only Windows has is the ordinary kind.

## Triggers

```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
```

`branches: [main]` on both. Without it on `push`, every branch a contributor
pushes runs the full matrix a second time alongside the pull request's run, for
the same commit. Without it on `pull_request`, a pull request between two
topic branches runs the matrix for work that is not yet headed anywhere.

What the filter gives up is a check on a pull request into a long-lived release
branch. Add that branch to both lists when one exists; do not drop the filter,
because the set of branches CI runs on is then whatever anybody happens to
push.

## Every job sets `RUSTFLAGS`

```yaml
env:
  RUSTFLAGS: -D warnings
  CARGO_TERM_COLOR: always
```

**In every job, identically.** `RUSTFLAGS` is part of Cargo's fingerprint for
every compiled unit, so a job that sets it and a job that does not cannot share
a single compiled artefact: whichever runs second rebuilds the whole graph,
including every dependency. Two jobs that differ in `RUSTFLAGS` by accident
look like a slow cache rather than like a misconfiguration, which is why this
is worth stating rather than leaving to each job.

`-D warnings` there also covers what a command-line flag does not. `cargo
clippy -- -D warnings` passes the flag to the final crate only; a warning in a
build script, or in a workspace member compiled as a dependency of the one
being linted, is emitted and not denied.

`RUSTDOCFLAGS: -D warnings` is separate and goes on the `cargo doc` step:
rustdoc does not read `RUSTFLAGS`, and a broken intra-doc link is a rustdoc
warning.

## What no job does

**No job takes a credential**, and no job checks out another repository.

Every dependency of a build here resolves from a public registry — crates.io,
nuget.org, the npm registry, PyPI — or from a public git repository pinned by
commit, so there is nothing for a token to unlock. A
workflow that asks for one has either added a private dependency — which is a
decision to make deliberately, not a secret to add quietly — or is reaching
into another repository's tree.

Reaching into another repository is the thing to refuse hardest, because it
inverts the dependency direction without changing a manifest. A job that checks
out a consumer and builds it against this tree makes this repository's CI fail
for a change made in the consumer, and makes the consumer's behaviour part of
this repository's contract without anything recording that.

What a package needs from here it takes as a pinned dependency, and finds out
about a breaking change by moving that pin. A composite action is shared the
same way — `<owner>/<repo>/.github/actions/<name>@<commit>` — which needs no
checkout and no credential.

**A release is the one write, and it is a workflow of its own.** foundation's
`release.yml` runs only on a version tag, with `permissions: contents: write`,
and creates the GitHub Release with the workflow's own `GITHUB_TOKEN`; it reads
no other secret. The build it runs is `scripts/build-release-assets.sh`, which
the read-only `release-assets` job in `ci.yml` runs on every pull request, so
the token is never what first exercises the packaging.

`permissions: contents: read` at the top of the workflow states it, and
`concurrency` with `cancel-in-progress: true` is safe here for the same reason
every job is read-only: an obsolete run has nothing left to finish.

## Counting a suite

A suite that did not run is not a suite that passed. `cargo test` exits 0 with
nothing to run, `node --test` exits 0 over a file whose tests were all deleted,
and a test that became `#[ignore]`d lands in a column nothing reads. So a step
that runs a suite also reads the count out of the log and fails on zero, and on
any skipped or ignored test.

**Extract the count with `awk`, never with `grep | awk` or a `grep` chain.**

```bash
set -euo pipefail
dotnet test dotnet/Interop.Tests 2>&1 | tee dotnet-test.log
passed=$(awk 'match($0, /Passed: +[0-9]+/) { s = substr($0, RSTART, RLENGTH); gsub(/[^0-9]/, "", s); print s }' dotnet-test.log | tail -1)
if [ "${passed:-0}" -eq 0 ]; then
  echo "::error::The .NET suite ran no test."
  exit 1
fi
```

Not:

```bash
passed=$(grep -oE 'Passed: +[0-9]+' dotnet-test.log | grep -oE '[0-9]+' | tail -1)
```

**`grep` exits 1 when it matches nothing.** Under `set -o pipefail` that status
becomes the pipeline's, and under `set -e` the shell exits at the assignment.
So the one case the guard exists for — no result line in the log at all, the
suite that did not run — kills the script before the `if` that was going to
report it. The job still fails, and it fails with no message, at a line that
looks like a broken command rather than an empty suite. Whoever reads it starts
debugging the extraction.

`awk` prints nothing and exits 0 when no line matches, so `${passed:-0}` is `0`
and the `if` reports what actually happened.

The same shape, in the flavours each runner needs:

```bash
# node --test prints a TAP summary
pass=$(awk '/^# pass / {print $3}' node-test.log)
skipped=$(awk '/^# skipped / {print $3}' node-test.log)
todo=$(awk '/^# todo / {print $3}' node-test.log)
if [ "${pass:-0}" -eq 0 ] || [ "${skipped:-0}" -ne 0 ] || [ "${todo:-0}" -ne 0 ]; then
  echo "::error::The Node suite ran no test, or skipped one."
  exit 1
fi
```

```bash
# unittest: `OK` alone, because `OK (skipped=N)` is a test that did not run
if ! grep -qE '^Ran [1-9][0-9]* tests? ' unittest.log || ! grep -qx 'OK' unittest.log; then
  echo "::error::The conformance self-test ran no test, or skipped one."
  exit 1
fi
```

A `grep` inside `if !` is fine: the `!` consumes the status, so `set -e` never
sees it. It is the `$(...)` assignment that is fatal, and that is where `awk`
belongs.

For `cargo test`, the check is the composite action
`.github/actions/prove-tests-ran`, which fails a job when the log shows no
result line, zero passed tests, or any ignored test. It is written once so that
a consuming package refers to it by commit rather than carrying the same shell
inline and letting the two drift.

## `audit`

```yaml
audit:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v7
    - run: cargo install cargo-audit --locked
    - run: cargo audit
```

`cargo audit` reads `Cargo.lock` and matches every resolved version against the
[RustSec advisory database][rustsec]. It compiles nothing, which is why it is
its own job rather than a step in `check`: it needs no toolchain beyond the one
that builds `cargo-audit`, and it has an answer for a lockfile that no platform
in the matrix would change.

It is also the one job here whose result can change with no commit, because the
database moves. That is the point of running it on `push` to `main` and not
only on pull requests: an advisory published after a merge is found on the next
push rather than the next time somebody happens to open a pull request.

[rustsec]: https://rustsec.org/
