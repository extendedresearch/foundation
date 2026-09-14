"""The consuming repository's `conformance.toml`.

Everything that differs between packages is here rather than in code: where
the header is, the prefix its constants share, where the cases are, and how to
run each language's driver.

```toml
header = "crates/abi/include/example.h"
prefix = "EXAMPLE"
cases = "conformance/bindings/cases.json"
# abi_version = "EXAMPLE_ABI_VERSION"   # the default is <prefix>_ABI_VERSION

[[language]]
name = "python"
command = ["{python}", "conformance/bindings/driver.py", "--out", "{out}", "--cases", "{cases}"]

[[language]]
name = "javascript"
command = ["node", "conformance/bindings/driver.mjs", "--out", "{out}", "--cases", "{cases}"]
env = { EXAMPLE_PYTHON = "{python}" }
```

Paths are relative to the file's own directory, and every driver runs with that
directory as its working directory. In `command` and `env` values these
placeholders are replaced:

| Placeholder | Becomes |
|---|---|
| `{python}` | the interpreter running this tool |
| `{out}` | the file the driver writes its observations to |
| `{cases}` | the cases file |
| `{root}` | the directory holding `conformance.toml` |
| `{language}` | the language's `name` |
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


class ConfigError(ValueError):
    """The configuration file is missing a key or has one of the wrong type."""


@dataclass(frozen=True)
class Language:
    """One binding, and how to run its driver."""

    name: str
    command: tuple[str, ...]
    env: dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class Config:
    """A consuming repository's configuration, with paths resolved."""

    root: Path
    header: Path
    prefix: str
    cases: Path
    abi_version: str
    languages: tuple[Language, ...]


def expand(text: str, values: dict[str, str]) -> str:
    """Replace each `{name}` placeholder in `text`, and nothing else.

    Plain replacement rather than `str.format`, so an argument that contains
    braces for its own reasons passes through unchanged.
    """
    for name, value in values.items():
        text = text.replace("{" + name + "}", value)
    return text


def _string(data: dict[str, Any], key: str, where: str) -> str:
    value = data.get(key)
    if not isinstance(value, str) or not value:
        raise ConfigError(f"{where}: `{key}` must be a non-empty string")
    return value


def _language(entry: Any, at: int, where: str) -> Language:
    if not isinstance(entry, dict):
        raise ConfigError(f"{where}: [[language]] {at} is not a table")
    name = _string(entry, "name", f"{where}: [[language]] {at}")
    command = entry.get("command")
    if (
        not isinstance(command, list)
        or not command
        or not all(isinstance(part, str) for part in command)
    ):
        raise ConfigError(f"{where}: language {name}: `command` must be a non-empty list of strings")
    env = entry.get("env", {})
    if not isinstance(env, dict) or not all(
        isinstance(key, str) and isinstance(value, str) for key, value in env.items()
    ):
        raise ConfigError(f"{where}: language {name}: `env` must be a table of strings")
    return Language(name=name, command=tuple(command), env=dict(env))


def load(path: Path) -> Config:
    """Read and check a `conformance.toml`."""
    where = str(path)
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise ConfigError(f"{where}: {error}") from error
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"{where}: {error}") from error

    root = path.resolve().parent
    prefix = _string(data, "prefix", where)
    entries = data.get("language")
    if not isinstance(entries, list) or not entries:
        raise ConfigError(f"{where}: at least one [[language]] is required")
    languages = tuple(_language(entry, at, where) for at, entry in enumerate(entries))
    names = [language.name for language in languages]
    if len(set(names)) != len(names):
        raise ConfigError(f"{where}: two languages share a name: {names}")

    abi_version = data.get("abi_version", f"{prefix}_ABI_VERSION")
    if not isinstance(abi_version, str):
        raise ConfigError(f"{where}: `abi_version` must be a string")

    return Config(
        root=root,
        header=root / _string(data, "header", where),
        prefix=prefix,
        cases=root / _string(data, "cases", where),
        abi_version=abi_version,
        languages=languages,
    )
