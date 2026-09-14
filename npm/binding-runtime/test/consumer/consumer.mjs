// Loads @extendedresearch/binding-runtime from node_modules by its package
// name, the way a binding does, so the `exports` map resolves every import and
// the compiled JavaScript is what runs.
//
// `scripts/build-release-assets.sh` copies this directory into a scratch
// project, installs the freshly packed tarball there, and runs this file.

import assert from "node:assert/strict";

import { AbiError, SEPARATOR, createErrors } from "@extendedresearch/binding-runtime/errors";
import { hardenClass, hardenFunction } from "@extendedresearch/binding-runtime/harden";
import { contract, sharedPrefix, shortName } from "@extendedresearch/binding-runtime/enums";

class ExampleError extends AbiError {}

const errors = createErrors({
  prefix: "EXAMPLE",
  codes: ["EXAMPLE_ERR_NULL"],
  base: ExampleError,
});

const failure = errors.translate(new Error(`EXAMPLE_ERR_NULL${SEPARATOR}a handle was null`));
assert.ok(failure instanceof ExampleError);
assert.equal(failure.code, "EXAMPLE_ERR_NULL");
assert.equal(failure.message, "a handle was null");

const fail = hardenFunction(() => {
  throw new Error(`EXAMPLE_ERR_NULL${SEPARATOR}a handle was null`);
}, errors.translate);
assert.throws(fail, ExampleError);
assert.equal(typeof hardenClass, "function");

const origin = contract([
  { value: 0, name: "ORIGIN_UNSPECIFIED" },
  { value: 1, name: "ORIGIN_RAW" },
]);
assert.deepEqual({ ...origin.byName }, { UNSPECIFIED: 0, RAW: 1 });
assert.equal(sharedPrefix(["ORIGIN_UNSPECIFIED", "ORIGIN_RAW"]), "ORIGIN_");
assert.equal(shortName("RATE_50HZ", "RATE_"), "RATE_50HZ");

// No root export: a binding names the module it takes.
await assert.rejects(import("@extendedresearch/binding-runtime"), {
  code: "ERR_PACKAGE_PATH_NOT_EXPORTED",
});

console.log("consumer.mjs: errors, harden and enums resolve and run from node_modules");
