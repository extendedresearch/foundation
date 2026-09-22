# extendedresearch-napi

Error tokens, `BigInt` conversion and `#[napi]` exports for a package's
napi-rs binding.

## What it is

A library behind a C ABI is bound once per language, and every binding has to
answer the same question: this call failed with code `-17` — what does a caller
of *this* language catch? For Node, the answer is in two halves. This crate is
the native half; `@extendedresearch/binding-runtime` on npm is the TypeScript
half that turns what it reports into an error class.

Your binding calls the package's safe Rust core directly: it holds no raw
handle and writes no `unsafe`. What it takes from here is the translation of
what the core answered into what Node sees.

| | |
|---|---|
| `Tokens` | The error token protocol for one package: an `AbiError` becomes a `napi::Error` whose message is `"<PREFIX>_ERR_X: sentence"` |
| `Tokens::to_u64`, `from_u64` | `BigInt` to `u64` and back, refusing a value that would not survive |
| `status_exports!`, `abi_version_exports!`, `enumeration_exports!` | The `#[napi]` functions a binding exports, expanded **in your crate** |

It depends on `extendedresearch-abi` and on napi, and on nothing else.

## Install

```toml
[dependencies]
extendedresearch-napi = { git = "https://github.com/extendedresearch/foundation", rev = "<40-character commit>" }
napi = { version = "3", default-features = false, features = ["napi6"] }
napi-derive = "3"
```

A `rev` rather than a branch or a tag: a lockfile records the commit, and only
an edit to the manifest moves it. The repository is public, so resolving it
needs no credential. Nothing is published to crates.io.

**Name napi and napi-derive in your own manifest**, at the `3` series and with
the same features this crate enables. The macros here expand to `#[napi]` items
in your crate, and the code napi-derive generates refers to both crates by
those names.

The TypeScript half is on the npm registry, and the same tarball is attached to
each foundation release. Install the version whose release the commit you pin
here belongs to, so that both halves come from one commit;
`npm/binding-runtime/README.md` has both commands.

## Use

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

A failure reaches JavaScript with the message
`EXAMPLE_ERR_TRUNCATED: <sentence>`. The npm package
`@extendedresearch/binding-runtime`, built from `npm/binding-runtime` in this
repository, turns it into your error class:

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

## Guarantees

- **The token is a value your specification names, not a sentence somebody can
  reword.** Every failure crosses as the constant's name, `": "`, then the
  error's `Display`. A boundary name gains your prefix through
  `extendedresearch_abi::codes::token` (`ERR_NULL` becomes `EXAMPLE_ERR_NULL`);
  a domain name already carries it and is left alone. The pyo3 and .NET layers
  apply the same step, so one failure reads the same in all three.
- **The code travels in the message because it cannot travel anywhere else.**
  napi-rs carries a JavaScript error's `code` in its `status`, and `status` is
  Node-API's own fixed set. `napi::Error<String>` widens it for a synchronous
  function, but `#[napi] async fn` requires `Error<Status>`, so a channel that
  works for both is the message.
- **`statusCodes()` reports the header's constants and nothing more.** A
  boundary code your header does not declare is not in the list you pass and so
  is not reported; a code foundation has no name for is skipped; an entry in
  the domain table that is not a domain code is skipped, so you can pass a
  table listing every status without naming a boundary code twice.
- **`to_u64` refuses what `get_u64` would corrupt.** napi-rs's own `get_u64`
  truncates a value wider than 64 bits and reads a negative value's magnitude —
  either is a number the core receives wrong and reports nothing about. This
  refuses both, naming the argument in the refusal.
- **The exports register, because they are compiled in your crate.** napi-rs
  registers exports from static constructors, and a linker may drop an
  unreferenced one from an rlib; whether a `#[napi]` item compiled in a
  dependency reaches the consumer's module has not been checked. The macros
  sidestep the question rather than relying on an answer.
- **The token protocol has one written form on each side.** `SEPARATOR` here
  and `SEPARATOR` in `@extendedresearch/binding-runtime/errors` are the same
  string, and `docs/conventions/error-tokens.md` states the grammar in full.
  `tests/tokens.rs` holds 11 tests over the protocol and the `BigInt`
  conversion with no Node involved, and
  `crates/napi-testaddon/test/addon.test.mjs` runs the macros' exports and the
  TypeScript half against a real addon:

  ```bash
  cargo test -p extendedresearch-napi
  cargo build -p extendedresearch-napi-testaddon
  node --test crates/napi-testaddon/test/addon.test.mjs   # Node 22.18 or later
  ```

## Limits

- **Nothing enforces the napi version lockstep.** `napi-sys` declares no
  `links` value, so a consumer whose napi requirement resolves to a different
  major builds a second copy beside this crate's and the two disagree about
  every type that crosses between them — a `napi::Error` from here is not the
  consumer's. There is no resolution error and no warning; the check is yours
  to run:

  ```bash
  cargo tree -i napi   # in your crate: exactly one version
  ```
- **Invoke `status_exports!` and `enumeration_exports!` once per crate.** Each
  defines a `#[napi(object)]` type — `StatusCode` and `EnumMember` — so a
  second invocation defines it twice and the crate does not compile. List every
  enumeration in one `enumeration_exports!` block.
- **Every package failure crosses with the same napi status.** `Tokens` builds
  each error with `Status::GenericFailure`, so a JavaScript caller cannot
  distinguish failures by anything napi-rs carries; the token in the message is
  the whole of the distinction.
- **Translation is by string match against a set.** The TypeScript half splits
  on the first `": "` and looks the token up in what `statusCodes()` and
  `bindingCodes()` reported. A token outside that set is left alone — which is
  what lets a napi-rs failure of its own pass through as itself, and also means
  a code your header gained but your build did not report arrives untranslated.
- **`BigInt` conversion covers `u64` and nothing else.** There is no signed
  equivalent and no wider one; a value that does not fit an unsigned 64 bits
  has no helper here.
- **This crate holds no TypeScript.** The error classes, the class hardening
  and the frozen enumeration contracts are a separate package, published to a
  separate registry, and installed separately. Keeping the two halves on one
  commit is a rule, not a build failure.
- **This crate is for a binding that calls a safe Rust core.** It holds no
  handle type, no pointer reading and no `unsafe`, so it does nothing for a
  Node binding that reaches the library through its C ABI.

## Versioning

This is 0.1.1, consumed as a git dependency pinned by `rev`. Pre-1.0: a minor
release may change any name, code or signature. The minimum supported Rust
version is 1.85, and the napi series is `3` — the pin
`docs/conventions/toolchain-pins.md` states.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
