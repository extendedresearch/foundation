# The name of a package's C library artefact

**A package's C library artefact is `<package>_abi`.** The crate that holds the
`extern "C"` adapter sets its `[lib] name` to the package name with `_abi`
appended, and nothing else in the workspace answers to that name:

```toml
# crates/example-abi/Cargo.toml
[lib]
name = "example_abi"
crate-type = ["cdylib", "staticlib"]
```

The loader sees `libexample_abi.so`, `libexample_abi.dylib` or
`example_abi.dll`, and every binding resolves the library under that name.

## Why the suffix is there

A `[lib] name` equal to a binary target's name in the same workspace is a
collision in one directory rather than two. Both link steps write their debug
information to `target/<profile>/deps/<name>.pdb`, because the output name is
derived from the target name and the two targets share it.

Cargo sees this coming and warns:

```text
warning: output filename collision.
The lib target `example` in package `example v0.0.0` has the same output
filename as the bin target `example` in package `example v0.0.0`.
Colliding filename is: .../target/debug/deps/example.pdb
```

The warning has been open since 2018 as [rust-lang/cargo#6313][collision], so
it is not a state a future Cargo release resolves for you. On Windows the
warning is followed by a hard failure, because MSVC's linker does not share a
PDB between two link invocations:

```text
LINK : fatal error LNK1201: error writing to program database
```

The failure is order-dependent: it appears when both targets link in one
invocation of `cargo build` and can vanish when only one of them is rebuilt.
That makes it a fault that reaches CI from a clean tree and not the machine of
whoever introduced it, which is the argument for a naming rule rather than a
warning somebody can choose to read.

A separate name for the library target takes the collision away rather than
sequencing around it: `example_abi.pdb` and `example.pdb` are two files.

[collision]: https://github.com/rust-lang/cargo/issues/6313

## What does not move

The suffix belongs to the artefact and to nothing else:

| | |
|---|---|
| The header filename | `example.h`, not `example_abi.h`. It is the package's contract, and a C caller includes the package, not one of its build products |
| The symbol prefix | `example_stream_create`, not `example_abi_stream_create`. Every exported symbol keeps the package's own prefix |
| The constant prefix | `EXAMPLE_ERR_NULL`, `EXAMPLE_ABI_VERSION`. See [error-tokens.md](error-tokens.md) |
| The package name | `example`. The suffix is on the library target inside it |

A binding therefore knows two spellings: the library's file name, which it
passes to its loader, and the package's prefix, which every symbol and constant
it reads begins with. Moving the suffix into the second would rename the whole
C contract to work around a name collision in a build directory.

## Checking it

The library target's name, from the manifest rather than from memory:

```bash
cargo metadata --no-deps --format-version 1 \
  | python -c 'import json,sys; [print(t["name"]) for p in json.load(sys.stdin)["packages"] for t in p["targets"] if "cdylib" in t["kind"] or "staticlib" in t["kind"]]'
# example_abi
```

And that the header and the symbols did not follow it:

```bash
grep -c '_abi_' include/example.h
# 0
```
