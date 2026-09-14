# extendedresearch-conformance

Run every language binding of a package through one set of cases, and compare
each binding with the package's C header, with what each case declared, and
with every other binding.

Each binding gets a thin driver that walks the cases file and writes down what
its binding did. This tool runs the drivers, makes every comparison, and prints
the full matrix, passing rows included, because a report of only the
disagreements cannot be told apart from one that never ran.

## Install

It depends on nothing outside the Python standard library (3.11 or later).
Install the wheel attached to a foundation release:

```bash
pip install https://github.com/extendedresearch/foundation/releases/download/v0.1.0/extendedresearch_conformance-0.1.0-py3-none-any.whl
```

The release's `SHA256SUMS` lists the wheel's checksum.

## Configure

Put a `conformance.toml` in your repository:

```toml
header = "crates/abi/include/example.h"
prefix = "EXAMPLE"
cases = "conformance/bindings/cases.json"

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

## Run

```bash
extendedresearch-conformance --config conformance.toml
# or: python -m extendedresearch_conformance --config conformance.toml
```

Exit status 0 means every row held; 1 means a row failed, a driver could not
run, or the cases file was empty; 2 means the configuration could not be read.
`--observations DIR` compares `observations-<language>.json` files already in
`DIR` instead of running the drivers.

The cases format, the observation format and every verdict are described in
`src/extendedresearch_conformance/run.py`. Rules about the status table as a
whole — codes that must stay distinct, statuses that must not exist yet — are
case data (`distinct`, `absent`), not code.

## Test

```bash
python -m unittest discover -s python/conformance/tests
```

The self-test runs three fixture drivers, conforming and with one fault each,
and checks that each fault fails the row it should.

## Status

0.1.0, nothing frozen. Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
