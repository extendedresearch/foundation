# @extendedresearch/binding-runtime

The TypeScript half of a package's napi-rs binding. The native half, built with
the Rust crate `extendedresearch-napi`, reports a failure as
`EXAMPLE_ERR_TRUNCATED: <sentence>`. These modules turn that report into your
error class, wrap a native class so every method and getter reports the same
way, and turn each enumeration the library reports into a frozen contract.

| Import | What you get |
|---|---|
| `@extendedresearch/binding-runtime/errors` | `AbiError`, `createErrors`, `SEPARATOR` |
| `@extendedresearch/binding-runtime/harden` | `hardenClass`, `hardenFunction` |
| `@extendedresearch/binding-runtime/enums` | `contract`, `sharedPrefix`, `shortName` |

Each module imports nothing, so you can take one without the other two. There
is no root export: import the subpath you need.

## Install

The package is attached to each foundation release and is not on the npm
registry. Install the tarball from the release's download URL:

```bash
npm install https://github.com/extendedresearch/foundation/releases/download/v0.1.1/extendedresearch-binding-runtime-0.1.1.tgz
```

npm installs a tarball given as an `http://` or `https://` URL
([`npm install <tarball url>`](https://docs.npmjs.com/cli/v10/commands/npm-install)).
The release's `SHA256SUMS` covers the tarball.

Take the tarball from the release whose tagged commit your `rev` of
`extendedresearch-napi` pins, so the native half and this half come from one
commit.

The tarball carries compiled JavaScript and declarations, `dist/*.js` and
`dist/*.d.ts`, not TypeScript sources: Node refuses to strip types from `.ts`
files under `node_modules`
([Node.js: type stripping in dependencies](https://nodejs.org/api/typescript.html#type-stripping-in-dependencies)).

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

Each module's header comment states its rules, and
`docs/conventions/error-tokens.md` in foundation states the token grammar.

## Build

```bash
npm ci
npm run build     # tsc: src/*.ts to dist/*.js and dist/*.d.ts
npm pack
```

`scripts/build-release-assets.sh` in foundation runs these, checks the
tarball's contents, and installs it into a scratch project to import each
export.

## Status

0.1.1. A later 0.x release can change any name or signature. Licensed under
Apache-2.0. See `LICENSE` and `NOTICE`.
