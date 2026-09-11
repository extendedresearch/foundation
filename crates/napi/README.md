# extendedresearch-napi

Error tokens, `BigInt` conversion, `#[napi]` exports and TypeScript for a
package's napi-rs binding.

Your binding calls the package's safe Rust core directly. This crate turns what
the core answers into what Node sees:

```rust
use extendedresearch_napi::{Report, Tokens};
use napi_derive::napi;

static TOKENS: Tokens = Tokens::new("CA3");

extendedresearch_napi::status_exports!(TOKENS, ca3::ERROR_CODES);
extendedresearch_napi::abi_version_exports!(ca3::abi_version(), ca3::ABI_VERSION);
extendedresearch_napi::enumeration_exports! {
    /// How a container was compressed.
    compressions => ca3::COMPRESSIONS;
}

#[napi]
pub fn open(path: String) -> napi::Result<u32> {
    ca3::open(&path).report(&TOKENS)
}
```

A failure reaches JavaScript with the message `CA3_ERR_TRUNCATED: <sentence>`.
The TypeScript in `ts/` turns it into your error class:

```ts
import { statusCodes } from "#native";
import { AbiError, createErrors } from "./vendor/errors.js";

export class Ca3Error extends AbiError {}
export const errors = createErrors({
  prefix: "CA3",
  codes: statusCodes().map((one) => one.name),
  base: Ca3Error,
});
```

## Vendoring the TypeScript

npm cannot install a subdirectory of a git repository, so commit a copy of
`ts/errors.ts`, `ts/harden.ts` and `ts/enums.ts` beside your TypeScript and
check it from a Rust test:

```rust
#[test]
fn vendored_typescript_matches_foundation() {
    extendedresearch_napi::typescript::assert_vendored(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ts/vendor"),
    );
}
```

The test fails when your copy differs from the one at the commit you pin.

## napi version

Require napi `3` and napi-derive `3`, with the same features ranvier's binding
uses. `napi-sys` declares no `links` value, so a different series builds a
second copy beside this crate's and the types do not match. `cargo tree -i napi`
in your crate lists exactly one version. Name napi and napi-derive in your own
manifest: the macros expand to `#[napi]` items in your crate.

## Status

0.0.0, consumed as a git dependency pinned by `rev`; nothing is frozen.
Licensed under Apache-2.0. See `LICENSE` and `NOTICE`.
