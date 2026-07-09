#!/usr/bin/env python3
"""Strip blanket-impl noise cargo-docs-md doesn't filter on its own.

cargo-docs-md (github.com/consistent-milk12/docs-md) only excludes a fixed
allowlist of blanket impls (From/Into/TryFrom/TryInto/Any/Borrow/BorrowMut/
ToOwned) instead of checking rustdoc's own `blanket_impl` JSON field the way
cargo-doc-md does -- so it doesn't know about `AsTaggedExplicit`/
`AsTaggedImplicit` (a `impl<T: Sized + 'a> ... for T` blanket impl pulled in
transitively via asn1-rs/x509-parser), which shows up on every single
documented type in the crate. It also leaves in `StructuralPartialEq`/
`StructuralEq`, compiler-internal derive markers with no real content.
Confirmed 2026-07-09: ~550 lines of this across the generated docs, none of
it conveying any information a reader needs.

Deletes each matching heading line (bare `impl ... for Type` markers with no
method body) plus one following blank line, if present.
"""

import re
import sys
from pathlib import Path

NOISE_TRAITS = (
    "AsTaggedExplicit",
    "AsTaggedImplicit",
    "StructuralPartialEq",
    "StructuralEq",
)

NOISE_RE = re.compile(
    r"^#+ `impl(?:<[^>]*>)? (?:" + "|".join(NOISE_TRAITS) + r")(?:<[^>]*>)? for .*`$"
)


def process(path: Path) -> bool:
    lines = path.read_text().splitlines(keepends=True)
    out: list[str] = []
    changed = False
    i = 0
    while i < len(lines):
        if NOISE_RE.match(lines[i].rstrip("\n")):
            changed = True
            i += 1
            if i < len(lines) and lines[i].strip() == "":
                i += 1
            continue
        out.append(lines[i])
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
    print(f"strip-doc-noise: cleaned {changed_count} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
