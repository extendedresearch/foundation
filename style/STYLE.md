# Code style

The conventions every repository in this project writes to, and the tools that
enforce them. Rules are numbered `S<n>`; identifiers are never reused, and a
withdrawn rule is marked withdrawn in place.

**Enforcement is soft by default.** The checks run on the lines a change
touches, not on the tree, so adopting them costs no sweeping rewrite and a
contributor is never asked to fix code they did not write. §9 says which rules a
tool decides and which a reviewer does, and how to move a repository to strict.

---

## 0. Why these particular rules

Four languages describe the same objects here: a Rust core, a Python extension,
a TypeScript binding and a C# assembly. The same concept is named four times, in
four casings, and a reader has to recognise it each time. Most of what follows
falls out of one requirement — **a name must survive translation between
languages without a human deciding anything** — and the rest is the ordinary
practice of each language's community.

Where a language's own convention and cross-language coherence disagree, this
guide says which wins and why. It never leaves the conflict unstated.

---

## 1. One concept, one name

- **S1** A concept has one canonical identifier, written in `snake_case`. Every
  language spells that identifier by a mechanical transform, and no language
  invents a different word for it.

| Context | Transform | Example |
|---|---|---|
| Rust function, variable, module | `snake_case` | `sample_rate` |
| Rust type, trait, enum variant | `UpperCamelCase` | `SampleRate` |
| Rust constant, static | `SCREAMING_SNAKE_CASE` | `SAMPLE_RATE` |
| Python function, variable, module | `snake_case` | `sample_rate` |
| Python class | `UpperCamelCase` | `SampleRate` |
| Python constant, enum member | `SCREAMING_SNAKE_CASE` | `SAMPLE_RATE` |
| TypeScript value, property, method | `camelCase` | `sampleRate` |
| TypeScript type, interface, class | `UpperCamelCase` | `SampleRate` |
| C# public member, type | `PascalCase` | `SampleRate` |
| C# private field | `_camelCase` | `_sampleRate` |
| C# local, parameter | `camelCase` | `sampleRate` |
| C ABI symbol | `<prefix>_snake_case` | `example_sample_rate` |

- **S2** **Acronyms are words.** `HttpClient`, `abiVersion`, `AbiVersion`,
  `parse_url`, never `HTTPClient`, `ABIVersion` or `parseURL`. In
  `SCREAMING_SNAKE_CASE` every letter is already uppercase, so the rule does not
  arise.

  *This deviates from Microsoft's C# guideline*, which keeps two-letter acronyms
  uppercase (`IOStream`, `DBConnection`). The carve-out is dropped because a name
  crossing the ABI must transform mechanically in both directions, and a rule
  with an exception at two letters cannot: `io_stream` → `IOStream` → `io_stream`
  needs a dictionary, while `io_stream` → `IoStream` → `io_stream` needs none.
  Rust's RFC 430, TypeScript practice and Python's PEP 8 already agree on words.

- **S3** **A quantity carries its unit in its name.** `timeout_ns`,
  `rate_millihz`, `capacity_bytes`, `skew_ppb`. A bare `timeout` is a defect: a
  caller reading it has to find the definition to know whether it is seconds or
  milliseconds, and the two are a thousand-fold apart in a system whose whole
  subject is time.

- **S4** **A boolean reads as an assertion.** Prefix with `is_`, `has_`, `can_`
  or `should_` (`isReady`, `HasExpired`, `can_resolve`). A boolean named for the
  thing rather than the claim — `ready`, `expiry` — reads as a value at the call
  site.

- **S5** **Conversions use the same three verbs in every language**, with the
  meanings Rust's API guidelines give them: `as_` is a cheap borrowed view,
  `to_` allocates or computes, `into_` consumes the receiver. A C# or TypeScript
  method converting a value uses `To<Thing>` / `to<Thing>` for the allocating
  case and a property for the borrowed one.

- **S6** **No `get_` prefix on an accessor.** `fn rate()`, `get rate()`,
  `Rate { get; }`. Reserve `get_` for an operation that genuinely fetches from
  somewhere.

- **S7** **A collection is plural; a count is `_count`; an index is `_index`.**
  `observations`, `observation_count`, `observation_index`.

- **S8** **Abbreviations are spelled out** except these, which are established
  enough to read as words: `abi`, `api`, `cpu`, `db`, `id`, `io`, `max`, `min`,
  `ns`, `ms`, `ppb`, `rx`, `tx`, `utc`, `uuid`. Adding to this list is a change
  to this file.

- **S9** **An error type is named for what failed, suffixed by the language's
  convention**: `ParseError` in Rust, TypeScript and Python; `ParseException` in
  C#. Not `ErrorParse`, not `ParseFailure`, not a bare `Error` outside a crate
  root.

---

## 2. Rust

- **S10** `rustfmt` decides all formatting. The configuration is
  `style/configs/rustfmt.toml`; a repository copies it and the copy is checked
  for drift (§9).
- **S11** The workspace lint table is foundation's, repeated verbatim by every
  repository: `unsafe_code = "deny"`, `missing_docs = "warn"`,
  `unwrap_used`/`expect_used`/`indexing_slicing` warned,
  `undocumented_unsafe_blocks` warned. CI passes `-D warnings`, so a warning
  fails a build.
- **S12** Every public item carries a doc comment (`missing_docs` enforces it).
  The first line is a sentence naming what the item is, not what it does
  internally.
- **S13** Imports are grouped `std`, external crates, then this crate, separated
  by a blank line, each group alphabetised. **No tool enforces this.** rustfmt's
  `group_imports` and `imports_granularity` are unstable, so on the stable
  channel they format nothing and warn once per invocation; the shared config
  leaves them out and says why. Enforcing this mechanically means a nightly
  rustfmt in a job of its own.
- **S14** Prefer `impl Trait` in argument position for a single-use bound;
  prefer a named generic when the bound appears twice.
- **S15** A function returning `Result` names its error type; `Box<dyn Error>`
  does not appear in a public signature.

## 3. TypeScript

- **S16** Prettier decides formatting; ESLint with `typescript-eslint` decides
  the rest. Type-aware rules are on, which is why the config needs a
  `tsconfig.json` path.
- **S17** No `I` prefix on an interface, no `T` prefix on a type parameter
  beyond a single descriptive word (`TValue`, not `T` when two parameters
  appear).
- **S18** `type` for a union or an alias, `interface` for an object contract
  something implements.
- **S19** No `any`. Where a value is genuinely unconstrained, `unknown` plus a
  narrowing check. `eslint` enforces this and it is the one rule that most often
  needs a justified `// eslint-disable-next-line` — which requires the reason on
  the same line.
- **S20** Every exported symbol carries a TSDoc comment.
- **S21** A module imports nothing it does not use, and a module intended to be
  independently importable imports nothing at all. Foundation's
  `@extendedresearch/binding-runtime` is checked for this mechanically.

## 4. Python

- **S22** `ruff format` decides formatting; `ruff check` decides lint. Neither
  `black` nor `isort` nor `flake8` appears in any repository.
- **S23** `mypy --strict` on everything that ships. A binding's `.pyi` stub is
  part of what ships.
- **S24** Type annotations on every public function signature.
- **S25** Docstrings follow PEP 257; the first line is a sentence ending in a
  full stop.
- **S26** A `.pyi` stub must agree with the extension it describes. This is the
  one rule with no tool behind it today — see §9.

## 5. C#

- **S27** `dotnet format` decides formatting, driven by `.editorconfig`.
- **S28** Nullable reference types are enabled, and warnings are errors.
- **S29** `_camelCase` for private fields, `PascalCase` for everything public,
  `camelCase` for locals and parameters (S1).
- **S30** An `async` method ends in `Async`, except an event handler.
- **S31** A public type carries an XML doc comment.
- **S32** No Hungarian notation, and no `m_` prefix.

## 6. Everything else

- **S33** Shell scripts are `bash`, start `set -euo pipefail`, and pass
  `shellcheck`.
- **S34** YAML is two-space indented, and every workflow pins third-party
  actions by commit where the workflow reads a secret.
- **S35** Markdown wraps at 80 columns. **No tool enforces this**, and prettier
  is deliberately not pointed at Markdown: its `proseWrap` would reflow every
  hand-wrapped document in the tree, which is churn rather than consistency.
  Prose style for shipped documents is governed separately by each repository's
  documentation conventions; this file governs only mechanics.
- **S36** Line length is 100 columns for code in every language, which is
  `rustfmt`'s default and is set explicitly for the other three.
- **S37** Files end with a newline, use UTF-8 without a BOM, and use LF in the
  repository. `.gitattributes` and `.editorconfig` both state this.

---

## 7. Comments

- **S38** A comment explains **why**, not what. A comment restating the code is
  deleted rather than updated.
- **S39** A comment stating a fact a command could check names the command
  beside it, so a reader can re-derive it instead of trusting it.
- **S40** `TODO` carries an owner and a tracking reference, or it is not
  committed. A bare `TODO` is invisible to everyone but its author.

---

## 8. Errors

- **S41** An error message is a sentence naming what was attempted, what
  happened, and what the caller can do. It does not end in a full stop when it
  will be composed into another message, and this project's ABI error tokens
  govern the composed form.
- **S42** An error carries the data a caller needs to branch, not only a string.
- **S43** No error is logged and returned. One or the other.

---

## 9. What a tool decides, and what a reviewer does

| Rule | Enforced by | Mode |
|---|---|---|
| S10, S11, S12, S14, S15 | `rustfmt`, `clippy` | mechanical |
| S13 | nothing — the settings are nightly-only | review |
| S16–S21 | `prettier`, `eslint` | mechanical |
| S22–S25 | `ruff`, `mypy` | mechanical |
| S27–S32 | `dotnet format`, analyzers | mechanical |
| S33 | `shellcheck` | mechanical |
| S36, S37 | `.editorconfig`, formatters | mechanical |
| S35 | nothing — prettier is kept off Markdown on purpose | review |
| S1–S9 | **partly** — each linter enforces its own casing; the cross-language identity in S1 is not checked by anything | review |
| S26 | nothing today | review |
| S38–S43 | nothing | review |

**The honest part of this table is the last three rows.** A convention checked
only by human intention is not enforced, and saying so is more useful than
restating it. Two of them are worth building a check for later: an identifier
census across the four languages for S1, and a stub-versus-extension comparison
for S26. Neither exists.

**Soft mode** runs every mechanical check against the files a change touches and
reports findings as annotations without failing the build. **Strict mode** fails
it. A repository moves from soft to strict per language, when that language's
existing code is clean — not all at once.

---

## 10. What this file does not govern

`docs/conventions/` is normative where it speaks, and this guide defers to it
rather than restating it:

| Convention | Governs |
|---|---|
| `rustfmt.md` | The two edition lines in `rustfmt.toml`, and why the edition is named twice |
| `error-tokens.md` | The grammar of an error token crossing into a managed language — S41 defers to it entirely |
| `c-library-naming.md` | What a package's C library artefact is called |
| `toolchain-pins.md` | Which compiler and dependency series a repository is on |
| `workspace-layout.md` | Where a crate goes |
| `ci.md` | How a repository's jobs are split |

A rule here that contradicts one of those is a defect in this file.

Each repository's prose and documentation conventions are separate again: this
guide governs mechanics — casing, formatting, line length — and says nothing
about voice.

---

## 11. Changing a rule

A rule changes by editing this file, in a pull request, with the reason in the
body. Configuration follows the rule rather than leading it: a setting in
`style/configs/` that no rule here states is a setting nobody agreed to.
