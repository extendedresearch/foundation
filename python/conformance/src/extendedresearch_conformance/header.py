"""What the contract declares, read out of the package's C header.

The header is a machine-readable statement of the contract that no binding
produced. A comparison of the bindings against each other says they agree; a
comparison of each against the header says they agree *with the contract*, and
those are different claims.

Nothing here enumerates a member. Every function scans for a pattern built from
the configured prefix, so a value added to the header is picked up with no
edit. The one thing written down is each family's prefix, and that lives in the
cases file beside the family, so a renamed family fails loudly — an empty match
set — rather than narrowing the check to nothing.
"""

from __future__ import annotations

import re
from pathlib import Path


class Header:
    """The integer constants one package's header defines."""

    def __init__(self, path: Path, prefix: str) -> None:
        self.path = path
        self.prefix = prefix
        #: `#define PREFIX_SOMETHING -4`, and nothing else in a header looks like it.
        self._define = re.compile(
            rf"^#define (?P<name>{re.escape(prefix)}_[A-Z0-9_]+) (?P<value>-?\d+)$"
        )
        #: A status: `PREFIX_OK` or any `PREFIX_ERR_*`. Not every constant with
        #: the prefix, which would sweep in the ABI version and every
        #: enumeration member.
        self._status = re.compile(rf"^{re.escape(prefix)}_(OK|ERR_[A-Z0-9_]+)$")
        self._constants: dict[str, int] | None = None

    def constants(self) -> dict[str, int]:
        """Every `#define` with the prefix that names an integer."""
        if self._constants is None:
            found: dict[str, int] = {}
            for line in self.path.read_text(encoding="utf-8").splitlines():
                matched = self._define.match(line.strip())
                if matched:
                    found[matched.group("name")] = int(matched.group("value"))
            if not found:
                raise AssertionError(
                    f"{self.path} defines no {self.prefix}_ constant; the path or the prefix is wrong"
                )
            self._constants = found
        return self._constants

    def family(self, header_prefix: str, contract_prefix: str) -> dict[str, int]:
        """One enumeration, as the contract spells its members.

        `header_prefix` is what the C constants share (`EXAMPLE_REFUSE_REASON_`);
        `contract_prefix` is what the library's `_name` functions answer
        (`REFUSE_REASON_`). Both are named because they can differ: a header
        may spell a member `EXAMPLE_ASSERTED_BY_OS_USER` while the contract
        calls it `ASSERTION_METHOD_OS_USER`, so neither prefix can be derived
        from the other.
        """
        members = {
            contract_prefix + name[len(header_prefix) :]: value
            for name, value in self.constants().items()
            if name.startswith(header_prefix)
        }
        if not members:
            raise AssertionError(
                f"no constant in {self.path.name} begins {header_prefix!r}; the family "
                "was renamed, or this prefix is stale"
            )
        return members

    def statuses(self) -> dict[str, int]:
        """Every status the ABI can answer, by the header's own name for it."""
        found = {
            name: value
            for name, value in self.constants().items()
            if self._status.match(name)
        }
        if not found:
            raise AssertionError(f"{self.path.name} defines no statuses; the scan is wrong")
        return found

    def value(self, name: str) -> int:
        """One named constant."""
        constants = self.constants()
        if name not in constants:
            raise AssertionError(f"{self.path.name} does not define {name}")
        return constants[name]
