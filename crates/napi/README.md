# extendedresearch-napi

Error tokens, `BigInt` conversion and `#[napi]` exports for a package's napi-rs
binding.

Your binding calls the package's safe Rust core directly. This crate turns what
the core answers into what Node sees:

```rust
use extendedresearch_napi::{Report, Tokens};
use napi_derive::napi;

static TOKENS: Tokens = Tokens::new("EXAMPLE");

extendedresearch_napi::status_exports!(TOKENS, example::BOUNDARY_CODES, example::ERROR_CODES);
extendedresearch_napi::abi_version_exports!(example::abi_version(), example::ABI_VERSION);
extendedresearch_napi::enumeration_exports! {
    /// How a container was compressed.
    compressions => example::COMPRESSIONS;
}

#[napi]
pub fn open(path: String) -> napi::Result<u32> {
    example::open(&path).report(&TOKENS)
}
```

`BOUNDARY_CODES` lists the boundary codes your header declares (`ERR_NULL`,
`ERR_RANGE`, …), so `statusCodes()` reports exactly the header's constants.

## The TypeScript half

A failure reaches JavaScript with the message `EXAMPLE_ERR_TRUNCATED: <sentence>`.
The npm package `@extendedresearch/binding-runtime`, built from
`npm/binding-runtime` in foundation, turns it into your error class:

```ts
import { statusCodes } from "#native";
import { AbiError, createErrors } from "@extendedresearch/binding-runtime/errors";

export class ExampleError extends AbiError {}
export const errors = createErrors({
  prefix: "EXAMPLE",
  codes: statusCodes().map((one) => one.name),
  base: ExampleError,
});
```

The package is attached to foundation's releases, not published to the npm
registry. Install it from the release whose tagged commit you pin this crate
to, so both halves come from one commit; `npm/binding-runtime/README.md` has the
command.

## napi version

Require napi `3` and napi-derive `3`, the series
`docs/conventions/toolchain-pins.md` pins, with the same features this crate
enables. `napi-sys` declares no `links` value, so a different series builds a
second copy beside this crate's and the types do not match. `cargo tree -i napi`
in your crate lists exactly one version. Name napi and napi-derive in your own
manifest: the macros expand to `#[napi]` items in your crate.

## Status

0.1.0, consumed as a git dependency pinned by `rev`; nothing is frozen.
Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
