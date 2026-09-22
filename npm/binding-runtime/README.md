# @extendedresearch/binding-runtime

The TypeScript half of a package's napi-rs binding.

## What it is

A native Node addon can throw, but it cannot easily hand JavaScript a typed
error: Node-API carries a fixed set of statuses and nothing a library can
extend. So the native half — built with the Rust crate
`extendedresearch-napi` — reports a failure as its constant's name, `": "`,
then a sentence:

```text
EXAMPLE_ERR_TRUNCATED: the name did not fit
```

These modules turn that report into your error class, wrap a native class so
every method and getter reports the same way, and turn each enumeration the
library reports into a frozen contract.

| Import | What you get |
|---|---|
| `@extendedresearch/binding-runtime/errors` | `AbiError`, `createErrors`, `SEPARATOR` |
| `@extendedresearch/binding-runtime/harden` | `hardenClass`, `hardenFunction` |
| `@extendedresearch/binding-runtime/enums` | `contract`, `sharedPrefix`, `shortName` |

Each module imports nothing, so you can take one without the other two. There
is no root export: import the subpath you need.

## Install

The package is published `--access restricted` in the `@extendedresearch`
organisation, so installing it by name needs a credential the organisation
issued — `npm login`, or an `.npmrc` naming a read token for the
`@extendedresearch` scope. Without one the registry answers 404, the answer a
restricted package gives a reader who cannot see it.

```bash
npm install @extendedresearch/binding-runtime@0.1.1
```

An exact version, not a range: the native half is a Rust crate your manifest
pins by `rev` to one foundation commit, and this half is published from that
commit's release. Install the version whose release the `rev` you pin belongs
to, so both halves come from one commit.

The same tarball stays attached to each foundation release, and that URL needs
no credential, so you can install it by download URL instead — and a package
that already did needs no change:

```bash
npm install https://github.com/extendedresearch/foundation/releases/download/v0.1.1/extendedresearch-binding-runtime-0.1.1.tgz
```

npm installs a tarball given as an `http://` or `https://` URL
([`npm install <tarball url>`](https://docs.npmjs.com/cli/v10/commands/npm-install)).
The two are the same file: `release.yml` publishes the asset it attaches, and
the release's `SHA256SUMS` covers it.

The tarball carries compiled JavaScript and declarations, `dist/*.js` and
`dist/*.d.ts`, not TypeScript sources: Node refuses to strip types from `.ts`
files under `node_modules`
([Node.js: type stripping in dependencies](https://nodejs.org/api/typescript.html#type-stripping-in-dependencies)).

To build it from this repository instead:

```bash
npm ci
npm run build     # tsc: src/*.ts to dist/*.js and dist/*.d.ts
npm pack
```

## Use

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

Then wrap the addon's classes once, at import, so every call reports the same
way:

```ts
import { hardenClass } from "@extendedresearch/binding-runtime/harden";
import { Stream } from "#native";

hardenClass(Stream, errors.translate);
```

Each module's header comment states its rules, and
`docs/conventions/error-tokens.md` in foundation states the token grammar.

## Guarantees

- **Branch on `error.code`, never on `error.message`.** The code is the C ABI
  constant's own name — `EXAMPLE_ERR_TIMEOUT` — so it is a value the
  specification names rather than a sentence somebody can reword.
- **A failure this build has no name for is still a failure.**
  `<PREFIX>_ERR_BINDING` is something the binding could not do and
  `<PREFIX>_ERR_UNKNOWN` is a status this build has no name for; both are added
  to the code set for you, and both raise your class.
- **Anything that is not the package's passes through untouched.** A token
  outside the reported set, or a message with no `": "`, is returned as it
  arrived — so a napi-rs failure of its own, such as an argument of the wrong
  type, is not relabelled as yours.
- **Hardening walks the prototype rather than listing the methods.** A method
  added to the Rust tomorrow is wrapped tomorrow with no edit here, which the
  hand-written alternative cannot promise: a method added to the addon and not
  to a list would report differently from every other method.
- **Hardening does not change the shape the declarations describe.** A getter
  is wrapped as a getter and a method as a method, and a method that answers a
  promise has its rejection translated as well as its throw, without making a
  synchronous call asynchronous.
- **A binding that loops over the library's table cannot produce a subset of
  it.** `contract` takes the members the native half read out of the library,
  not values transcribed by hand, and returns `byName`, `nameOf` and `members`
  all frozen.
- **`enums` applies the same short-name rule `extendedresearch-pyo3` applies to
  a Python `IntEnum`**, derived from the member names rather than written down,
  because a family's C constant prefix and its contract prefix can differ.
- **Each module importing nothing is checked, not asserted.**
  `scripts/release-checks.py assets` fails on a compiled module containing a
  static import, a re-export, a dynamic import or a `require`.
- **The published tarball is checked on every pull request.** The
  `release-assets` job runs `scripts/build-release-assets.sh`, which fails on an
  archive holding a file it should not or missing one it should, installs the
  tarball into a scratch project and imports each export from `node_modules`,
  and type-checks a TypeScript consumer against the installed declarations.

## Limits

- **ESM only.** The package is `"type": "module"` and ships no CommonJS build,
  so `require()` does not reach it. A CommonJS consumer uses a dynamic
  `import()`.
- **No root export.** `import … from "@extendedresearch/binding-runtime"`
  resolves to nothing; name one of the three subpaths.
- **`hardenClass` does not wrap a constructor.** napi-rs builds the instance
  there, and replacing it would mean replacing the class — so a failure thrown
  while constructing arrives as the native half spelled it, untranslated.
- **`hardenClass` reaches one prototype's own properties, once.** A member
  inherited from a base class, a static method, a property the constructor
  assigns to the instance, and a property that is not configurable are all left
  as they are; so is anything added to the prototype after the call.
- **Translation is a string match.** The token has to be in the set
  `statusCodes()` and `bindingCodes()` reported, so a code your header gained
  and your native build did not report arrives untranslated, as an ordinary
  `Error`.
- **`contract` throws on colliding short names.** A table whose full names are
  distinct can still collide through the digit fallback — `RATE_50HZ` keeps its
  full spelling beside a member whose short name is already `RATE_50HZ` — and
  that is an exception at import, not a compile error.
- **These modules do nothing on their own.** There is no native code here and
  no runtime dependency; `errors` needs a native half that reports the token
  protocol, and `enums` needs one that answers `EnumMember[]` from the
  library's own table.

## Versioning

This is 0.1.1. Pre-1.0: a later 0.x release can change any name or signature.
The compiled output targets ES2022 and `NodeNext` module resolution.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
