// Type-checks against the installed tarball's declarations, the way a binding's
// own `tsc` does: through the `types` condition of each export.

import {
  AbiError,
  SEPARATOR,
  createErrors,
  type Errors,
} from "@extendedresearch/binding-runtime/errors";
import { hardenFunction, type Translate } from "@extendedresearch/binding-runtime/harden";
import { contract, type Contract } from "@extendedresearch/binding-runtime/enums";

class ExampleError extends AbiError {}

const errors: Errors = createErrors({
  prefix: "EXAMPLE",
  codes: ["EXAMPLE_ERR_NULL"],
  base: ExampleError,
});

const translate: Translate = errors.translate;

export const fail: (reason: string) => void = hardenFunction((reason: string): void => {
  throw new Error(`EXAMPLE_ERR_NULL${SEPARATOR}${reason}`);
}, translate);

export const origin: Contract = contract([
  { value: 0, name: "ORIGIN_UNSPECIFIED" },
  { value: 1, name: "ORIGIN_RAW" },
]);
