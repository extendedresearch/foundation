/**
 * Enumerated contracts, from the members the library reports.
 *
 * Part of `@extendedresearch/binding-runtime`, imported as
 * `@extendedresearch/binding-runtime/enums`. The source is
 * `npm/binding-runtime/src/enums.ts` in foundation.
 *
 * **This module contains no discriminant.** A package's native half answers
 * each enumeration as `EnumMember[]`, read from the library's own table, and
 * {@link contract} turns that into the three shapes a caller wants. A binding
 * that loops over the library's table cannot produce a subset of it; a binding
 * that transcribes values can, and the failure is silent.
 *
 * # The short-name rule
 *
 * The same rule `extendedresearch-pyo3` applies to a Python `IntEnum`:
 *
 * 1. Find the longest prefix every name shares, cut back to just past its last
 *    underscore. `ORIGIN_UNSPECIFIED` and `ORIGIN_RAW` share `ORIGIN_`;
 *    `REFUSE_REASON_PORTS` and `REFUSE_REASON_PROTOCOL` share
 *    `REFUSE_REASON_`, not `REFUSE_REASON_P`.
 * 2. Strip it. A name that would then be empty, or start with a digit
 *    (`RATE_50HZ` would become `50HZ`), keeps its full spelling.
 *
 * The prefix is derived from the names rather than written down, because a
 * family's C constant prefix and its contract prefix can differ.
 *
 * This module imports nothing, so a binding can take it without the other two.
 */

/** One member, as a package's native half reports it. */
export interface EnumMember {
  /** The value that crosses the boundary. */
  readonly value: number;
  /** The contract's name for it. */
  readonly name: string;
}

/** A frozen `SHORT_NAME` to value map. */
export type Enumeration = Readonly<Record<string, number>>;

/** One enumerated contract, in the three shapes a caller wants it in. */
export interface Contract {
  /** `PORTS` to `4`, and so on. Frozen. */
  readonly byName: Enumeration;
  /** `4` to `REFUSE_REASON_PORTS` — the contract's own spelling. Frozen. */
  readonly nameOf: Readonly<Record<number, string>>;
  /** Every member, in the order the library reports them. Frozen. */
  readonly members: readonly EnumMember[];
}

/** The prefix every name shares, back to and including its last underscore. */
export function sharedPrefix(names: readonly string[]): string {
  if (names.length === 0) return "";
  let prefix = names[0] ?? "";
  for (const name of names) {
    let common = 0;
    while (
      common < prefix.length &&
      common < name.length &&
      prefix[common] === name[common]
    ) {
      common += 1;
    }
    prefix = prefix.slice(0, common);
  }
  const boundary = prefix.lastIndexOf("_");
  return boundary < 0 ? "" : prefix.slice(0, boundary + 1);
}

/**
 * One member's short name: `name` without `prefix`, or the whole name when
 * that leaves nothing or leaves a leading digit.
 */
export function shortName(name: string, prefix: string): string {
  const short = name.startsWith(prefix) ? name.slice(prefix.length) : name;
  if (short.length === 0 || /^[0-9]/.test(short)) return name;
  return short;
}

/**
 * Turn the members a library reported into the three shapes above.
 *
 * Throws when two members would share a short name, which a table whose names
 * are distinct can still produce through the fallback (`RATE_50HZ` kept whole
 * beside a member whose short name is `RATE_50HZ`).
 */
export function contract(members: readonly EnumMember[]): Contract {
  const prefix = sharedPrefix(members.map((one) => one.name));
  const byName: Record<string, number> = Object.create(null) as Record<
    string,
    number
  >;
  const nameOf: Record<number, string> = Object.create(null) as Record<
    number,
    string
  >;
  for (const member of members) {
    const short = shortName(member.name, prefix);
    if (short in byName) {
      throw new Error(
        `two members of one enumeration share the short name ${short}`,
      );
    }
    byName[short] = member.value;
    nameOf[member.value] = member.name;
  }
  return Object.freeze({
    byName: Object.freeze(byName),
    nameOf: Object.freeze(nameOf),
    members: Object.freeze(
      members.map((one) => Object.freeze({ value: one.value, name: one.name })),
    ),
  });
}
