# Error tokens

A failure that crosses into a managed language arrives as a message with a
machine-readable token in front of it:

```text
EXAMPLE_ERR_TRUNCATED: the record ended before its declared length
```

The token, `": "`, then the sentence. The token is the C header's own spelling
of the constant; the sentence is the error's `Display` and is for a person. A
caller branches on the token and never on the sentence, because the sentence is
prose somebody may reword and the token is a name the header declares.

## Why the token is in the message

The obvious place for it is a field on the error object, and in two of the three
bindings it ends up there. It travels in the message because of the third.

napi-rs carries a JavaScript error's `code` in its `status`, and `status` is
Node-API's own fixed set of values. `napi::Error<String>` widens it for a
synchronous function, but `#[napi] async fn` requires `Error<Status>`. So there
is no field on an asynchronous napi error that a package's constant name fits
into. The message is the one channel every binding has.

Putting the token in the message in every binding, rather than in a field where
a field exists, keeps one grammar: a package's failure reads the same in
Python, in Node and in .NET, and a person reading a log from one can look the
token up in the header the same way.

## The grammar

```text
<PREFIX>_ERR_<NAME>: <sentence>
```

`<PREFIX>` is the prefix every constant of the package begins with — `EXAMPLE`
for a package whose header declares `EXAMPLE_ERR_NULL` and
`EXAMPLE_ABI_VERSION`.

There is one step between what a package's error type answers and what the
token says, and it exists because boundary codes and domain codes answer
differently:

| Kind | `AbiError::name` answers | Token |
|---|---|---|
| Boundary — `ERR_NULL`, `ERR_RANGE`, `ERR_UTF8`, `ERR_PANIC`, `ERR_STATE` | the unprefixed name, `ERR_NULL` | `EXAMPLE_ERR_NULL` |
| Domain — the package's own, at or below `DOMAIN_FLOOR` | the full declared name, `EXAMPLE_ERR_TRUNCATED` | `EXAMPLE_ERR_TRUNCATED` |

The boundary names are unprefixed at the source because they are shared: every
library on these conventions answers `ERR_NULL` for the same failure, and a
crate that knew one package's prefix could not serve another. The header,
though, declares them prefixed, because a C header has one flat namespace and
two libraries in one process both declaring `ERR_NULL` would collide.

`extendedresearch_status::codes::token` is that step, and it is idempotent: a name
that already carries the prefix and an underscore comes back unchanged, so
applying it to either kind is correct and applying it twice changes nothing.

```rust
use extendedresearch_status::codes::token;

assert_eq!(token("EXAMPLE", "ERR_NULL"), "EXAMPLE_ERR_NULL");
assert_eq!(token("EXAMPLE", "EXAMPLE_ERR_TRUNCATED"), "EXAMPLE_ERR_TRUNCATED");
// A name that merely starts with the same letters is not the prefix.
assert_eq!(token("EXAMPLE", "EXAMPLE0_ERR_X"), "EXAMPLE_EXAMPLE0_ERR_X");
```

**Every binding layer applies it, and a layer that skipped it would report a
name no header declares** — which is worse than reporting nothing, because it
sends a reader to `grep` a header for a constant that is not in it.

| Layer | Where the prefix lives |
|---|---|
| Rust, the boundary itself | `extendedresearch_status::codes::token`, re-exported as `extendedresearch_abi::codes::token` |
| Node | `extendedresearch_napi::Tokens`, constructed with the prefix; `Tokens::token` is `codes::token` |
| Python | the required `prefix` on the family `extendedresearch_pyo3::exceptions!` generates, read as `ExceptionFamily::PREFIX` |
| .NET | the `prefix` an `AbiErrors` is constructed with; `AbiErrors.NameOf` applies it |

## The two tokens no header declares

Two failures are the binding's rather than the package's, and they are named
from the prefix alone:

| Token | When |
|---|---|
| `EXAMPLE_ERR_BINDING` | The binding could not do its job: an answer that was not UTF-8, a buffer that kept growing, an ABI version mismatch. The library did not fail — the code path between it and the caller did |
| `EXAMPLE_ERR_UNKNOWN` | The library answered a code this build of the binding has no name for. Still a failure, still catchable, and the number is in the sentence |

Both are in the set a caller matches against, and neither is in the header.
`Tokens::binding_token` and `Tokens::unknown_token` in the Node layer, and
`AbiErrors.BindingCode` and `AbiErrors.UnknownCode` in .NET, are where they are
built.

`EXAMPLE_ERR_BINDING` being distinct from every library code is what lets a
caller tell "the thing I asked for could not be done" from "the binding broke
on the way back". Folding it into a library code would be a lie about where the
failure was, and folding it into an untyped error would put it outside the set
a caller catches for.

## What a caller does

**Split on the first `": "`, and look the token up in the package's frozen
table.**

```ts
const at = thrown.message.indexOf(": ");
if (at < 0) return thrown;                       // not this grammar
const code = thrown.message.slice(0, at);
if (!codes.has(code)) return thrown;             // not this package's
const sentence = thrown.message.slice(at + 2);
```

Three things about that shape are load-bearing:

**The first separator, not the last and not every one.** A sentence contains
`": "` as often as prose does. Splitting on the first occurrence makes the token
the only part of the message whose boundaries are fixed.

**A frozen table, built from what the library reported, not from a pattern.**
The set comes from `statusCodes()` and `bindingCodes()` — the package's own
exports, read when the binding loads. Matching `^[A-Z0-9_]+: ` instead would
relabel any error whose message happens to start with a capitalised word and a
colon, including errors from libraries that have nothing to do with this
package.

**A token outside the table passes through untouched.** An argument of the
wrong type thrown by napi-rs, a `TypeError` from the runtime, an error from
another library in the same process — none of those are this package's failure,
and wrapping them in the package's error class would claim they were. The
translation is a relabelling of what it recognises and a pass-through of
everything else.

Building the table from the library's own exports, rather than transcribing the
constants into the binding, is the same argument as the enumeration trio in
`extendedresearch_abi::enumeration`: a binding that loops cannot produce a
subset, and a binding that transcribes can, silently.
