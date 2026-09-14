/**
 * Making every native call raise the package's error.
 *
 * Part of `@extendedresearch/binding-runtime`, imported as
 * `@extendedresearch/binding-runtime/harden`. The source is
 * `npm/binding-runtime/src/harden.ts` in foundation.
 *
 * The native half reports failures as a constant's name and a sentence
 * (`errors.ts` says why). Something has to turn each of those into the
 * package's error, and doing it by hand once per method is the version that
 * goes stale: a method added to the Rust and not to a list would report
 * differently from every other method.
 *
 * **So the wrapping walks the prototype rather than listing the methods.** It
 * runs once, at import, over the classes and functions the addon exports. A
 * method added tomorrow is wrapped tomorrow with no edit here.
 *
 * A getter is wrapped as a getter and a method as a method, because a getter
 * rewritten as a method would change the public shape the generated
 * declarations describe. A method that answers a promise has its rejection
 * translated as well as its throw.
 *
 * This module imports nothing. Pass the `translate` from `createErrors`.
 */

/** Turns what the native half threw into the package's error. */
export type Translate = (thrown: unknown) => unknown;

/** Anything with a prototype whose members are worth wrapping. */
export type Constructor = { prototype: object; name: string };

/** Translate a rejection as well as a throw, without making a sync call async. */
function mapped(answer: unknown, translate: Translate): unknown {
  if (
    typeof answer === "object" &&
    answer !== null &&
    typeof (answer as PromiseLike<unknown>).then === "function"
  ) {
    return (answer as Promise<unknown>).then(undefined, (thrown: unknown) => {
      throw translate(thrown);
    });
  }
  return answer;
}

/** Wrap one function so what it refuses arrives translated. */
function wrap<T extends (...args: never[]) => unknown>(
  body: T,
  translate: Translate,
): T {
  return function wrapped(this: unknown, ...args: never[]): unknown {
    try {
      return mapped(body.apply(this, args), translate);
    } catch (thrown) {
      throw translate(thrown);
    }
  } as T;
}

/**
 * Wrap every method and getter on a native class, in place.
 *
 * `constructor` is skipped: napi-rs builds the instance there, and replacing it
 * would mean replacing the class. A property that is not configurable is left
 * as it is.
 */
export function hardenClass(subject: Constructor, translate: Translate): void {
  const prototype = subject.prototype;
  for (const name of Object.getOwnPropertyNames(prototype)) {
    if (name === "constructor") continue;
    const described = Object.getOwnPropertyDescriptor(prototype, name);
    if (described === undefined || described.configurable !== true) continue;

    if (typeof described.value === "function") {
      Object.defineProperty(prototype, name, {
        ...described,
        value: wrap(described.value as (...args: never[]) => unknown, translate),
      });
      continue;
    }
    if (typeof described.get === "function") {
      Object.defineProperty(prototype, name, {
        ...described,
        get: wrap(described.get as () => unknown, translate),
        ...(typeof described.set === "function"
          ? { set: wrap(described.set as (value: never) => void, translate) }
          : {}),
      });
    }
  }
}

/** Wrap a free function so what it refuses arrives translated. */
export function hardenFunction<T extends (...args: never[]) => unknown>(
  body: T,
  translate: Translate,
): T {
  return wrap(body, translate);
}
