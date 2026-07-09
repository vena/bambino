#!/usr/bin/env python3
"""Canonicalize nondeterministic ordering in cargo-doc-md output.

cargo-doc-md (github.com/Crazytieguy/cargo-doc-md) collects trait impls by
iterating a rustdoc-types HashMap<Id, Item> with no sort before printing
(converter.rs collect_impls_for_type + the three render call sites) -- see
"Trait Implementations:" and "Traits:" derive-list order flip between runs
with identical source, discovered 2026-07-09. This is an upstream bug, not
fixable from this repo; run as a post-process step in `make docs` instead.

Sorts, per module .md file:
  - "**Traits:** A, B, C" derive lists, alphabetically
  - "**Trait Implementations:**" bullet blocks, alphabetically by trait name
"""

import re
import sys
from pathlib import Path

TRAITS_PREFIX = "**Traits:** "
IMPLS_HEADER = "**Trait Implementations:**"


def sort_traits_line(line: str) -> str:
    rest = line[len(TRAITS_PREFIX) :].rstrip("\n")
    items = sorted(x.strip() for x in rest.split(","))
    return TRAITS_PREFIX + ", ".join(items) + "\n"


def trait_name(entry_lines: list[str]) -> str:
    m = re.match(r"- \*\*(.+?)\*\*", entry_lines[0])
    return m.group(1) if m else entry_lines[0]


def process(path: Path) -> bool:
    lines = path.read_text().splitlines(keepends=True)
    out: list[str] = []
    i = 0
    changed = False

    while i < len(lines):
        line = lines[i]

        if line.startswith(TRAITS_PREFIX):
            new_line = sort_traits_line(line)
            changed |= new_line != line
            out.append(new_line)
            i += 1
            continue

        if line.strip() == IMPLS_HEADER:
            out.append(line)
            i += 1
            if i < len(lines) and lines[i].strip() == "":
                out.append(lines[i])
                i += 1

            entries: list[list[str]] = []
            while i < len(lines) and lines[i].strip() != "":
                if lines[i].startswith("- **"):
                    entry_lines = [lines[i]]
                    i += 1
                    while (
                        i < len(lines)
                        and lines[i].startswith("  ")
                        and not lines[i].startswith("- **")
                    ):
                        entry_lines.append(lines[i])
                        i += 1
                    entries.append(entry_lines)
                else:
                    # Unexpected shape -- keep as-is rather than risk mangling it.
                    entries.append([lines[i]])
                    i += 1

            sorted_entries = sorted(entries, key=trait_name)
            changed |= sorted_entries != entries
            for entry_lines in sorted_entries:
                out.extend(entry_lines)
            continue

        out.append(line)
        i += 1

    if changed:
        path.write_text("".join(out))
    return changed


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <docs-dir>", file=sys.stderr)
        return 1

    docs_dir = Path(sys.argv[1])
    changed_count = sum(process(p) for p in sorted(docs_dir.rglob("*.md")))
    print(f"sort-docs: normalized {changed_count} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
