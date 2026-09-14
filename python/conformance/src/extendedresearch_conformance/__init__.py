"""Behavioural conformance across every language binding of one package.

A package's bindings each get a thin driver that walks a shared cases file and
writes down what its binding did. This tool runs every driver, then compares
each binding against the package's C header, against what each case declared,
and against every other binding, and prints the full matrix, passing rows
included.

`run` is the entry point; `config` reads the consuming repository's
`conformance.toml`; `header` reads the constants the header defines.
"""

__version__ = "0.1.0"
