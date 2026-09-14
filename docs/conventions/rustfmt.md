# rustfmt.toml

**A repository carries one `rustfmt.toml` at its root, and that file names the
edition twice:**

```toml
edition = "2024"
style_edition = "2024"
```

Both values match the `edition` in `[workspace.package]`. Change one and you
change all three in the same commit.

## Why the edition is written down at all

`cargo fmt` finds the manifest, reads `edition` out of it, and passes
`--edition` to every rustfmt invocation it makes. For that path, the
`rustfmt.toml` changes nothing.

**rustfmt invoked directly reads no manifest.** It is given a file, not a
package, and nothing in its arguments says which edition that file belongs to.
Its default is edition 2015 — the value rustfmt was written with, kept for
compatibility — and there is no warning when it falls back to it.

Three ways that path gets taken, none of them exotic:

| | |
|---|---|
| Format-on-save | Every editor integration that is not routed through `cargo fmt` runs `rustfmt` on the open buffer |
| `rustfmt src/lib.rs` | Formatting one file by hand |
| A pre-commit hook | The ones that pass changed paths run the binary, not the Cargo subcommand |

Under edition 2015, rustfmt parses edition-2024 syntax as a syntax error. A let
chain is the case that bites first:

```rust
if let Some(rest) = name.strip_prefix(prefix)
    && rest.starts_with('_')
{
    return name.to_owned();
}
```

That is valid in edition 2024 and is not valid in edition 2015, where `let` is
not an expression. rustfmt parses the file, fails, and — this is the part that
makes it a convention rather than a nuisance — **leaves the file unchanged**.
Formatting a file is not supposed to be destructive, so a parse failure means
rustfmt writes nothing.

So format-on-save appears to work and does nothing, for one file, for as long
as nobody looks. The divergence is found by `cargo fmt --all --check` in CI,
which does read the manifest, gets the edition right, parses the file, and
reports a diff the author's editor has been silently declining to apply.

`style_edition` is the second value because it is a separate knob from
`edition`. `edition` tells rustfmt which grammar to parse; `style_edition`
selects which edition's *formatting rules* to apply, and it defaults to the
value of `edition` only when `edition` is known. Writing both keeps a directly
invoked rustfmt from parsing as 2024 and formatting as 2015.

## Why it sits at the repository root

**rustfmt walks up from the file it is formatting** until it finds a
`rustfmt.toml` or `.rustfmt.toml`, and stops at the first one. It does not look
in the current working directory, and it does not consult a workspace.

The root is therefore the only location every source file in the repository can
reach. A `rustfmt.toml` inside a crate covers that crate and shadows the root's
for it — which is a way to give one crate different rules, and a way to
accidentally give one crate edition 2015 by writing a file there that omits
`edition`.

A package under `tools/`, excluded from the root workspace (see
[workspace-layout.md](workspace-layout.md)), still finds the root
`rustfmt.toml` by the same upward walk, because exclusion is a Cargo notion and
rustfmt is walking directories. It does not inherit `[workspace.package]`, so
its manifest names its own edition; the formatting configuration it shares.

## Checking it

The pins, from the files rather than from memory:

```bash
grep -E '^(edition|style_edition) ' rustfmt.toml
grep -E '^edition ' Cargo.toml
```

And that a directly invoked rustfmt agrees with `cargo fmt`, which is the case
the root file exists for:

```bash
rustfmt --check $(git ls-files '*.rs')    # no manifest is read; the config is
cargo fmt --all --check                   # the manifest is read
```

Both quiet, or both reporting the same diff. `rustfmt --check` quiet while
`cargo fmt --all --check` reports a diff is the failure this file prevents:
the direct invocation parsed nothing.
