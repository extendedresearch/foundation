// The shared TypeScript, run against an addon that consumes the shared macros.
//
//     cargo build -p extendedresearch-napi-testaddon
//     node --test crates/napi-testaddon/test/
//
// The modules are `npm/binding-runtime/src/`, the sources of
// `@extendedresearch/binding-runtime`. Node imports the `.ts` files directly by
// stripping their types (Node 22.18 and later), so this needs no compiler and
// no npm install. The compiled package is checked separately:
// `scripts/build-release-assets.sh` installs the tarball into a scratch project
// and runs it from `node_modules`. The addon is read from `target/debug` unless
// `EXTENDEDRESEARCH_TESTADDON` names the built library.

import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { AbiError, SEPARATOR, createErrors } from "../../../npm/binding-runtime/src/errors.ts";
import { hardenClass, hardenFunction } from "../../../npm/binding-runtime/src/harden.ts";
import { contract, sharedPrefix, shortName } from "../../../npm/binding-runtime/src/enums.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));

function builtLibrary() {
  if (process.env.EXTENDEDRESEARCH_TESTADDON) {
    return process.env.EXTENDEDRESEARCH_TESTADDON;
  }
  const stem = "extendedresearch_napi_testaddon";
  const file =
    process.platform === "win32"
      ? `${stem}.dll`
      : process.platform === "darwin"
        ? `lib${stem}.dylib`
        : `lib${stem}.so`;
  return join(ROOT, "target", "debug", file);
}

// `require` loads a native addon only from a `.node` file.
const loadable = join(mkdtempSync(join(tmpdir(), "er-napi-")), "addon.node");
copyFileSync(builtLibrary(), loadable);
const native = createRequire(import.meta.url)(loadable);

class TestError extends AbiError {}
class FullError extends TestError {}

const errors = createErrors({
  prefix: "TEST",
  codes: native.statusCodes().map((one) => one.name),
  base: TestError,
  subclasses: [["TEST_ERR_FULL", FullError]],
});

function thrown(body) {
  try {
    body();
  } catch (caught) {
    return caught;
  }
  assert.fail("nothing was thrown");
}

test("the status table is OK, the boundary codes, then the domain codes", () => {
  assert.deepEqual(
    native.statusCodes().map((one) => [one.name, one.value]),
    [
      ["TEST_OK", 0],
      ["TEST_ERR_NULL", -1],
      ["TEST_ERR_RANGE", -2],
      ["TEST_ERR_UTF8", -3],
      ["TEST_ERR_PANIC", -4],
      ["TEST_ERR_STATE", -5],
      ["TEST_ERR_FULL", -16],
    ],
  );
  assert.deepEqual(native.bindingCodes(), ["TEST_ERR_BINDING", "TEST_ERR_UNKNOWN"]);
  assert.ok(errors.codes.has("TEST_ERR_BINDING"));
  assert.ok(errors.codes.has("TEST_ERR_UNKNOWN"));
});

test("the version exports are registered and agree", () => {
  assert.equal(native.abiVersion(), 3);
  assert.equal(native.expectedAbiVersion(), native.abiVersion());
});

test("the native half reports a token, the separator and a sentence", () => {
  const raw = thrown(() => native.fail("utf8"));
  assert.equal(raw.message, `TEST_ERR_UTF8${SEPARATOR}the text was not UTF-8`);
  assert.ok(!(raw instanceof AbiError));
});

test("translate raises the mapped class, or the base, with the code", () => {
  const full = errors.translate(thrown(() => native.fail("full")));
  assert.ok(full instanceof FullError);
  assert.ok(full instanceof TestError);
  assert.equal(full.code, "TEST_ERR_FULL");
  assert.equal(full.message, "the counter is full at 3");
  assert.equal(full.name, "FullError");
  assert.ok(full.cause instanceof Error);

  const state = errors.translate(thrown(() => native.fail("state")));
  assert.ok(state instanceof TestError);
  assert.ok(!(state instanceof FullError));
  assert.equal(state.code, "TEST_ERR_STATE");

  const binding = errors.translate(thrown(() => native.bindingFailure()));
  assert.equal(binding.code, "TEST_ERR_BINDING");
});

test("an error that is not the package's passes through untouched", () => {
  const wrongType = thrown(() => native.roundTrip("not a bigint"));
  assert.equal(errors.translate(wrongType), wrongType);
  const foreign = new Error("OTHER_ERR_X: something else");
  assert.equal(errors.translate(foreign), foreign);
  assert.equal(errors.translate("a string"), "a string");
});

test("guard and guardAsync translate", async () => {
  assert.throws(() => errors.guard(() => native.fail("full")), FullError);
  await assert.rejects(
    errors.guardAsync(async () => native.fail("full")),
    FullError,
  );
  assert.equal(errors.guard(() => 7), 7);
});

test("every u64 survives a round trip and nothing wider does", () => {
  for (const value of [0n, 1n, 2n ** 32n, 2n ** 64n - 1n]) {
    assert.equal(native.roundTrip(value), value);
  }
  for (const value of [-1n, 2n ** 64n, -(2n ** 70n)]) {
    const failure = errors.translate(thrown(() => native.roundTrip(value)));
    assert.equal(failure.code, "TEST_ERR_BINDING", String(value));
  }
});

test("hardenClass translates methods and getters, and leaves the constructor", () => {
  hardenClass(native.Counter, errors.translate);
  const counter = new native.Counter();
  assert.equal(counter.increment(), 1);
  counter.increment();
  counter.increment();
  assert.throws(() => counter.increment(), FullError);
  const closed = thrown(() => counter.closed);
  assert.ok(closed instanceof TestError);
  assert.equal(closed.code, "TEST_ERR_STATE");
});

test("hardenFunction translates a throw and a rejection", async () => {
  const fail = hardenFunction(native.fail, errors.translate);
  assert.throws(() => fail("full"), FullError);
  assert.equal(fail("nothing"), undefined);

  const later = hardenFunction(async () => native.fail("full"), errors.translate);
  await assert.rejects(later(), FullError);
});

test("an enumeration becomes a frozen contract with short names", () => {
  const origin = contract(native.origins());
  assert.deepEqual({ ...origin.byName }, { UNSPECIFIED: 0, RAW: 1, DERIVED: 2 });
  assert.equal(origin.nameOf[1], "ORIGIN_RAW");
  assert.equal(origin.members.length, 3);
  assert.ok(Object.isFrozen(origin.byName));
  assert.ok(Object.isFrozen(origin.members));

  const rate = contract(native.rates());
  assert.deepEqual({ ...rate.byName }, { UNSPECIFIED: 0, RATE_50HZ: 50 });
});

test("the short-name rule matches extendedresearch-pyo3's cases", () => {
  assert.equal(sharedPrefix(["ORIGIN_UNSPECIFIED", "ORIGIN_RAW"]), "ORIGIN_");
  assert.equal(
    sharedPrefix(["REFUSE_REASON_PORTS", "REFUSE_REASON_PROTOCOL"]),
    "REFUSE_REASON_",
  );
  assert.equal(sharedPrefix(["ALPHA", "BETA"]), "");
  assert.equal(sharedPrefix(["AB_X", "AC_Y"]), "");
  assert.equal(sharedPrefix([]), "");
  assert.equal(sharedPrefix(["STAMP_UNDATED"]), "STAMP_");
  assert.equal(shortName("ORIGIN_RAW", "ORIGIN_"), "RAW");
  assert.equal(shortName("RATE_50HZ", "RATE_"), "RATE_50HZ");
  assert.equal(shortName("RATE_", "RATE_"), "RATE_");
});

test("two members sharing a short name are refused", () => {
  assert.throws(
    () =>
      contract([
        { value: 0, name: "RATE_RATE_50HZ" },
        { value: 1, name: "RATE_50HZ" },
      ]),
    /share the short name/,
  );
});
