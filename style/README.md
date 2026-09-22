# extendedresearch-style

The style guide every repository in this project writes to, the configuration
that enforces it, and the CI action that runs it against a change.

`STYLE.md` is the guide. Everything else here exists to enforce part of it, and
`STYLE.md` §9 is honest about which parts nothing enforces.

## What is here

| Path | What it is |
|---|---|
| `STYLE.md` | The guide. Rules are numbered `S<n>` and are cited by every config file |
| `configs/editorconfig` | Copied to a repository root as `.editorconfig`. Line endings, indentation, line length, and the C# naming and nullable rules |
| `configs/rustfmt.toml` | Copied to a repository root. Every setting is stable; the file says which nightly-only options were left out and why |
| `configs/clippy.toml` | Thresholds the workspace lint table compares against |
| `configs/ruff.toml` | Python format and lint, replacing black, isort and flake8 |
| `configs/prettierrc.json` | Copied as `.prettierrc.json` |
| `configs/eslint.config.mjs` | Flat config, type-aware. Needs `@eslint/js` and `typescript-eslint` in the repository's own lockfile |
| `configs/Directory.Build.props` | Nullable on, warnings as errors, analyzers on, XML docs generated |
| `scripts/check-style.sh` | Runs every check against the files a change touched. Soft by default |
| `scripts/check-configs.py` | Fails when a repository's copy of a config drifts from foundation's |
| `../.github/actions/style-check` | The composite action a repository's CI calls |

## Adopting it in a repository

Copy the configs for the languages that repository actually has:

```bash
cp <foundation>/style/configs/editorconfig          .editorconfig
cp <foundation>/style/configs/rustfmt.toml          rustfmt.toml
cp <foundation>/style/configs/clippy.toml           clippy.toml
cp <foundation>/style/configs/ruff.toml             ruff.toml
cp <foundation>/style/configs/prettierrc.json       .prettierrc.json
cp <foundation>/style/configs/eslint.config.mjs     eslint.config.mjs
cp <foundation>/style/configs/Directory.Build.props Directory.Build.props
```

A language the repository does not have needs no copy, and
`check-configs.py` reports that as `absent` rather than as a failure.

**Why copies rather than a package to depend on.** `rustfmt.toml` and
`clippy.toml` have no `extends` mechanism, so at least two of these have to be
copies whatever the others do. One distribution mechanism that works for all
seven beats two that each work for some — and a copy that nothing compares is a
copy that drifts, which is why `check-configs.py` exists and runs in CI. It is
the rule foundation already holds its `LICENSE` and `NOTICE` copies to.

## Running it

```bash
bash style/scripts/check-style.sh --base origin/main --mode soft
python style/scripts/check-configs.py
```

In a workflow:

```yaml
- uses: extendedresearch/foundation/.github/actions/style-check@<commit>
  with:
    base: origin/${{ github.base_ref }}
    mode: soft
```

## Soft, and how to leave it

`mode: soft` reports findings as annotations and passes. Nothing in an existing
tree has to change, and a contributor is never asked to fix code they did not
write.

`mode: strict` fails the job. Move a repository to strict **one language at a
time**, when that language's existing code is already clean under the config —
which you find out by running the check over the whole tree once, not by
switching and watching CI go red.

## What it will not catch

`STYLE.md` §9 has the full table. The three worth repeating:

- **The cross-language identity in S1.** Each linter checks its own casing.
  Nothing checks that `sample_rate`, `sampleRate` and `SampleRate` are the same
  concept, or that one of them was not renamed on its own. An identifier census
  across the four languages would catch it; it does not exist.
- **Import grouping (S13).** rustfmt's `group_imports` is nightly-only, so on
  stable it formats nothing. Enforcing it means a nightly rustfmt in its own
  job.
- **A Python stub against its extension (S26).** Nothing compares a `.pyi` with
  the module it describes.

Each is a real gap, listed rather than implied, because a report of only
failures cannot distinguish a rule that passed from one nothing looked at.
