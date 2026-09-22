# extendedresearch-conformance

Run every language binding of a package through one set of cases, and compare
each binding with the package's C header, with what each case declared, and
with every other binding.

## What it is

A library with a C ABI and three or four language bindings has one contract and
several independent readings of it. Testing each binding on its own says each
one is self-consistent; it does not say they agree, and it does not say any of
them agrees with the header. This tool makes both comparisons.

Each binding gets a thin driver that walks the cases file and writes down what
its binding did. This tool runs the drivers, makes every comparison, and prints
the full matrix, passing rows included, because a report of only the
disagreements cannot be told apart from one that never ran.

It depends on nothing outside the Python standard library; `tomllib` is why the
floor is 3.11.

## Install

```bash
pip install https://github.com/extendedresearch/foundation/releases/download/v0.1.1/extendedresearch_conformance-0.1.1-py3-none-any.whl
```

pip installs from a local or remote archive, and the release also carries the
sdist. The release's `SHA256SUMS` lists the wheel's checksum.

## Use

Put a `conformance.toml` in your repository:

```toml
header = "crates/abi/include/example.h"
prefix = "EXAMPLE"
cases = "conformance/bindings/cases.json"
# abi_version = "EXAMPLE_ABI_VERSION"   # the default is <prefix>_ABI_VERSION

[[language]]
name = "python"
command = ["{python}", "conformance/bindings/driver.py", "--out", "{out}", "--cases", "{cases}"]

[[language]]
name = "javascript"
command = ["node", "conformance/bindings/driver.mjs", "--out", "{out}", "--cases", "{cases}"]
env = { EXAMPLE_PYTHON = "{python}" }
```

`src/extendedresearch_conformance/config.py` lists the placeholders. Paths are
relative to the file, and drivers run with its directory as their working
directory.

Then run it:

```bash
extendedresearch-conformance --config conformance.toml
# or: python -m extendedresearch_conformance --config conformance.toml
```

Exit status 0 means every row held; 1 means a row failed, a driver could not
run, or the corpus declared no cases or no families; 2 means the configuration
could not be read. `--observations DIR` compares `observations-<language>.json`
files already in `DIR` instead of running the drivers, and `--cases FILE` reads
a cases file other than the configured one.

The cases format, the observation format and every verdict are described in
`src/extendedresearch_conformance/run.py`. Rules about the status table as a
whole — codes that must stay distinct, statuses that must not exist yet — are
case data (`distinct`, `absent`), not code.

## Guarantees

- **Every row is printed, including every row that held.** A report of only the
  failures cannot distinguish "checked and clean" from "never checked".
- **A difference nobody wrote down is a finding even when it is harmless.** A
  family, a status table or a read that some bindings offer and others do not
  fails unless the case carries a `declared` note saying why. The alternative
  is a suite that quietly normalises whatever it meets.
- **An empty corpus fails.** A cases file declaring no cases or no families,
  and a run that checked no row at all, exit 1 and say so — because zero
  failures over an empty corpus prints exactly like zero failures over a full
  one.
- **A driver that could not run is reported, not skipped.** A driver that fails
  to start, exits non-zero, or writes no readable observations becomes a `FAIL
  driver` row, and so does a case a driver never recorded.
- **The member count is asserted as well as the member set.** A header scan
  that matched nothing would agree with a binding that reported nothing, and
  every "the bindings agree" row would then pass vacuously; the count against
  the header is what stops that.
- **Different failures must stay distinguishable in each language.** Every
  error case's `(type, code)` identity is compared within each binding, so two
  failures that collapse into one exception are a finding.
- **The header scan enumerates nothing.** Every pattern is built from the
  configured prefix, so a constant added to the header is picked up with no
  edit here. The one thing written down is each family's prefix, and it lives
  in the cases file beside the family, so a renamed family fails loudly with an
  empty match set rather than narrowing the check to nothing.
- **The tool is tested against deliberately broken bindings.** The self-test
  runs the suite over three fixture bindings with one fault injected per test,
  and each test checks that the row it broke is the row that failed — so a
  suite that failed for some other reason does not pass it:

  ```bash
  python -m unittest discover -s python/conformance/tests   # 20 tests
  ```

## Limits

- **The header reader understands one line shape.** It matches
  `#define <PREFIX>_NAME <integer>`, with a single space and a decimal value. A
  constant declared as a C `enum`, written in hexadecimal, computed by a macro,
  or spaced differently is invisible to it, and a header with no matching line
  at all is an error rather than an empty result.
- **It compares what the drivers wrote down, not what the bindings do.** You
  write one driver per binding; a call no driver records is a call nothing
  compares, and a driver that records the wrong thing is believed.
- **One broken driver stops the whole matrix.** When any driver fails, the run
  reports the driver rows and makes no comparison at all — so a single
  environment problem hides every other finding until it is fixed.
- **Identity across languages is compared as text.** An error's type name, code
  and inheritance chain are strings the driver wrote; two languages' classes
  are never the same object, and the comparison is only as good as the names.
- **It reads the header, not the compiled library.** The header is the contract
  of record here. A header that disagrees with the binary a binding actually
  loaded is a disagreement nothing in this tool can see.
- **It runs the commands the configuration names**, as subprocesses in the
  configuration's directory, inheriting the environment. It is a development
  tool for your own repository, not a sandbox for a cases file you were sent.
- **Drivers run one at a time**, each to completion before the next starts.
- **Only the configuration is guarded by exit status 2.** A cases file that is
  not readable JSON raises rather than exiting with a status.
- **It compares values, and nothing else.** Timing, memory and concurrent
  behaviour are outside what a driver records and outside what this compares.

## Versioning

This is 0.1.1. Pre-1.0: nothing is frozen, and a later 0.x release can change
any name, option or output format. Python 3.11 or later.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
