/**
 * Failures, as `Error` subclasses carrying a machine-readable code.
 *
 * Vendored from `extendedresearch-napi` (`crates/napi/ts/errors.ts` in
 * foundation). Edit it there; the package's drift test fails when this copy
 * differs.
 *
 * **Branch on `error.code`, never on `error.message`.** The code is the C ABI
 * constant's own name — `EXAMPLE_ERR_TIMEOUT`, `EXAMPLE_ERR_TRUNCATED` — so it
 * is a value the specification names rather than a sentence somebody can
 * reword.
 *
 * # The protocol
 *
 * The native half cannot give a thrown error a custom `code` from an
 * asynchronous function, so it reports one in the message instead:
 *
 * ```text
 * EXAMPLE_ERR_TIMEOUT: the operation ran out of time
 * ```
 *
 * The constant's name, `": "`, then the sentence. {@link createErrors} splits on
 * the first `": "`, looks the token up in the set of codes the native half
 * reported (`statusCodes()` and `bindingCodes()`), and raises the package's
 * class for it. A token outside that set is not the package's, so a napi-rs
 * failure of its own — an argument of the wrong type, say — passes through as
 * itself.
 *
 * This file imports nothing, so a package can vendor it on its own.
 */

/** The separator between the token and the sentence. */
export const SEPARATOR = ": ";

/** A failure from a package, carrying the package's constant name. */
export class AbiError extends Error {
  /**
   * The constant's own name — `EXAMPLE_ERR_TIMEOUT`, and so on.
   *
   * `<PREFIX>_ERR_BINDING` for something the binding could not do, and
   * `<PREFIX>_ERR_UNKNOWN` for a status this build has no name for. An
   * unrecognised status is still a failure.
   */
  readonly code: string;

  constructor(code: string, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = new.target.name;
    this.code = code;
  }
}

/** A class {@link createErrors} can raise. */
export type ErrorClass = new (
  code: string,
  message: string,
  options?: ErrorOptions,
) => AbiError;

/** What {@link createErrors} needs to know about one package. */
export interface ErrorsOptions {
  /** The prefix every constant of the package begins with, such as `EXAMPLE`. */
  readonly prefix: string;
  /**
   * Every token the native half can report. Pass
   * `statusCodes().map((one) => one.name)`; the binding and unknown tokens are
   * added for you.
   */
  readonly codes: Iterable<string>;
  /** The package's base class. `AbiError` when omitted. */
  readonly base?: ErrorClass;
  /** Which class a token raises. A token not listed raises `base`. */
  readonly subclasses?: Iterable<readonly [string, ErrorClass]>;
}

/** The translation for one package. */
export interface Errors {
  /** Every token this build recognises. */
  readonly codes: ReadonlySet<string>;
  /** `<PREFIX>_ERR_BINDING`. */
  readonly bindingCode: string;
  /** `<PREFIX>_ERR_UNKNOWN`. */
  readonly unknownCode: string;
  /**
   * Turn whatever the native half threw into the package's error. Anything
   * that is not the package's is returned untouched.
   */
  translate(thrown: unknown): unknown;
  /** Run a synchronous call, raising the package's error for a failure. */
  guard<T>(body: () => T): T;
  /** Run an asynchronous call, raising the package's error for a failure. */
  guardAsync<T>(body: () => Promise<T>): Promise<T>;
}

/** Build the translation for one package. */
export function createErrors(options: ErrorsOptions): Errors {
  const base: ErrorClass = options.base ?? AbiError;
  const bindingCode = `${options.prefix}_ERR_BINDING`;
  const unknownCode = `${options.prefix}_ERR_UNKNOWN`;
  const codes: ReadonlySet<string> = new Set([
    ...options.codes,
    bindingCode,
    unknownCode,
  ]);
  const subclasses: ReadonlyMap<string, ErrorClass> = new Map(
    options.subclasses ?? [],
  );

  function translate(thrown: unknown): unknown {
    if (thrown instanceof AbiError) return thrown;
    if (!(thrown instanceof Error)) return thrown;

    const at = thrown.message.indexOf(SEPARATOR);
    if (at < 0) return thrown;

    const code = thrown.message.slice(0, at);
    if (!codes.has(code)) return thrown;

    const sentence = thrown.message.slice(at + SEPARATOR.length);
    const Subclass = subclasses.get(code) ?? base;
    return new Subclass(code, sentence, { cause: thrown });
  }

  return Object.freeze({
    codes,
    bindingCode,
    unknownCode,
    translate,
    guard<T>(body: () => T): T {
      try {
        return body();
      } catch (thrown) {
        throw translate(thrown);
      }
    },
    async guardAsync<T>(body: () => Promise<T>): Promise<T> {
      try {
        return await body();
      } catch (thrown) {
        throw translate(thrown);
      }
    },
  });
}
