# extendedresearch-pyo3

Python exceptions and `IntEnum`s for a package's PyO3 binding, built from the
package's `AbiError` and foundation `Enumeration`s.

Your binding calls the package's safe Rust core directly. This crate turns what
the core answers into Python:

```rust
use extendedresearch_pyo3::{Raise, exceptions, int_enum};
use pyo3::prelude::*;

exceptions! {
    /// Every exception `_thing` raises.
    pub family ThingExceptions in _thing;
    prefix "THING";
    base ThingError: "Anything thing refused.";
    panic PanicError: "A panic was caught; thing's state is unknown.";
    exception RefusedError(ThingError): "Thing refused the request.";
    domain thing::THING_ERR_REFUSED => RefusedError;
}

#[pyfunction]
fn open(path: &str) -> PyResult<u64> {
    thing::open(path).raise::<ThingExceptions>()
}

#[pymodule]
fn _thing(module: &Bound<'_, PyModule>) -> PyResult<()> {
    ThingExceptions::register(module)?;
    module.add("Origin", int_enum(module.py(), "Origin", "thing", "…", &thing::ORIGINS)?)?;
    module.add_function(wrap_pyfunction!(open, module)?)?;
    Ok(())
}
```

`exceptions!` expands in your crate, so each extension module owns one copy of
its exception classes. `prefix` is what your header's constants begin with:
a boundary failure's message reads `THING_ERR_RANGE: …`, as it does from your
Node and .NET bindings. The crate documentation lists which code raises which
exception and gives the short-name rule `int_enum` applies.

## pyo3 version

`pyo3-ffi` declares `links = "python"`, so a build holds one pyo3. Your pyo3
requirement must resolve to the same version as this crate's: the `0.29`
series, with `abi3-py311`. Enable `extension-module` in your own `cdylib`.
`cargo tree -i pyo3-ffi` in your crate lists exactly one version.

## Status

0.1.0, consumed as a git dependency pinned by `rev`; nothing is frozen.
Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
