#!/usr/bin/env python3
"""Post-process the markdown cargo-docs-md generates for docs/.

Three passes, run in order over every `*.md` under the docs directory:

1. strip_noise -- delete blanket-impl noise cargo-docs-md doesn't filter on
   its own. It only excludes a fixed allowlist of blanket impls (From/Into/
   TryFrom/TryInto/Any/Borrow/BorrowMut/ToOwned) instead of checking
   rustdoc's own `blanket_impl` JSON field the way cargo-doc-md does -- so it
   doesn't know about `AsTaggedExplicit`/`AsTaggedImplicit` (a
   `impl<T: Sized + 'a> ... for T` blanket impl pulled in transitively via
   asn1-rs/x509-parser), which shows up on every single documented type in
   the crate. It also leaves in `StructuralPartialEq`/`StructuralEq`,
   compiler-internal derive markers with no real content. Confirmed
   2026-07-09: ~550 lines of this across the generated docs, none of it
   conveying any information a reader needs.

2. unwrap_doc_blocks -- undo the blank line cargo-docs-md inserts between
   every *source* line of an item's doc comment under `--full-method-docs`.
   Each `///` line is emitted as its own markdown paragraph, so a
   hard-wrapped sentence becomes several, a `- ` list becomes several
   one-item lists, and a fenced example gets blank lines between every line
   of code. The two shapes are distinguishable: a wrap emits a genuinely
   empty separator line, while a blank `///` line in the source emits a
   whitespace-only line at the block's indent. Dropping the former and
   blanking the latter reproduces the original doc comment verbatim.

3. expand_trait_method_docs -- fill in the doc bodies of trait *declaration*
   methods from the rustdoc JSON. `--full-method-docs` only reaches methods
   inside impl blocks; under a trait's own `#### Required Methods` /
   `#### Provided Methods` the tool still emits the first source *line*
   alone, which truncates mid-sentence ("... or `None` if it"). 25 trait
   methods in this crate carry multi-line prose, including the ones whose
   whole point is a caveat the signature can't express
   (`TlsConnector::peer_chain_der`, `TimerProvider::now_millis`).

Pass 3 needs the same rustdoc JSON that fed cargo-docs-md; pass one JSON
path per doc pass the Makefile runs (host + esp-idf), since each sees a
different cfg-gated slice of the crate.
"""

import json
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

# `### `TlsConnector<RawStream: AsyncIo>`` -> TlsConnector
TRAIT_HEADING_RE = re.compile(r"^### `([A-Za-z_][A-Za-z0-9_]*)[<`]")
# Any heading at the trait's level or above ends the trait's section.
SECTION_END_RE = re.compile(r"^#{1,3} ")
METHOD_LIST_RE = re.compile(r"^#### (?:Required|Provided) Methods\s*$")
# `- `fn peer_chain_der(&self, ...) -> Option<Vec<Vec<u8>>>` -- [`links`]`
METHOD_BULLET_RE = re.compile(r"^- `(?:async |unsafe |const )*fn ([A-Za-z_][A-Za-z0-9_]*)")
# Bullets that introduce an item's own doc body, as opposed to the module-prose
# and table-of-contents bullets that are also followed by indented lines. Only
# the former carry the per-source-line paragraph splitting pass 2 undoes;
# joining a hard-wrapped TOC entry to the next one would corrupt it.
ITEM_BULLET_RE = re.compile(r"^- (?:<span id=\"|`)")


def strip_noise(lines: list[str]) -> list[str]:
    """Delete bare `impl Noise for Type` heading lines and one trailing blank."""
    out: list[str] = []
    i = 0
    while i < len(lines):
        if NOISE_RE.match(lines[i]):
            i += 1
            if i < len(lines) and not lines[i].strip():
                i += 1
            continue
        out.append(lines[i])
        i += 1
    return out


def _prev_content(lines: list[str], start: int) -> str:
    """Return the nearest non-blank line before `start`, or "" if there is none."""
    i = start - 1
    while i >= 0 and not lines[i].strip():
        i -= 1
    return lines[i] if i >= 0 else ""


def _doc_block_bounds(lines: list[str], start: int) -> int:
    """Return the index one past the doc block beginning at `start`.

    A doc block is a run of indented lines, allowing single blank lines
    between them (those are the separators pass 2 exists to remove).
    """
    end = start
    i = start
    while i < len(lines):
        if lines[i].startswith("  "):
            i += 1
            end = i
        elif not lines[i].strip() and i + 1 < len(lines) and lines[i + 1].startswith("  "):
            i += 1
        else:
            break
    return end


def unwrap_doc_blocks(lines: list[str]) -> list[str]:
    """Collapse cargo-docs-md's one-paragraph-per-source-line doc rendering."""
    out: list[str] = []
    i = 0
    while i < len(lines):
        if not lines[i].startswith("  ") or not ITEM_BULLET_RE.match(_prev_content(lines, i)):
            out.append(lines[i])
            i += 1
            continue

        end = _doc_block_bounds(lines, i)
        for line in lines[i:end]:
            if line == "":
                continue  # separator the tool inserted between two source lines
            # A whitespace-only line is a blank `///` line: a real paragraph
            # break, emitted without its trailing indent.
            out.append("" if not line.strip() else line)
        i = end
    return out


def collect_trait_docs(json_paths: list[Path]) -> dict[tuple[str, str], str]:
    """Map (trait name, method name) -> full doc body, from rustdoc JSON."""
    docs: dict[tuple[str, str], str] = {}
    for json_path in json_paths:
        index = json.loads(json_path.read_text())["index"]
        for item in index.values():
            trait = item.get("inner", {}).get("trait")
            if not trait or not item.get("name"):
                continue
            for member_id in trait.get("items", []):
                member = index.get(str(member_id))
                if not member or not member.get("name"):
                    continue
                body = (member.get("docs") or "").strip()
                if body:
                    docs.setdefault((item["name"], member["name"]), body)
    return docs


def _render(body: str) -> list[str]:
    """Indent a doc body to sit under its method bullet."""
    return [f"  {line}".rstrip() for line in body.split("\n")]


def expand_trait_method_docs(
    lines: list[str], trait_docs: dict[tuple[str, str], str]
) -> list[str]:
    out: list[str] = []
    trait_name: str | None = None
    in_method_list = False
    i = 0
    while i < len(lines):
        line = lines[i]

        heading = TRAIT_HEADING_RE.match(line)
        if heading:
            trait_name = heading.group(1)
            in_method_list = False
        elif SECTION_END_RE.match(line):
            trait_name = None
            in_method_list = False
        if METHOD_LIST_RE.match(line):
            in_method_list = True
        elif line.startswith("#### "):
            in_method_list = False

        out.append(line)
        i += 1

        method = METHOD_BULLET_RE.match(line) if in_method_list and trait_name else None
        body = trait_docs.get((trait_name, method.group(1))) if method else None
        if body is None:
            continue

        # Replace whatever truncated summary the tool emitted, blank line included.
        while i < len(lines) and not lines[i].strip():
            i += 1
        i = _doc_block_bounds(lines, i)
        out.append("")
        out.extend(_render(body))
        if i < len(lines) and lines[i].strip():
            out.append("")

    return out


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <docs-dir> <rustdoc.json> [<rustdoc.json>...]",
              file=sys.stderr)
        return 1

    docs_dir = Path(sys.argv[1])
    trait_docs = collect_trait_docs([Path(p) for p in sys.argv[2:]])

    changed = 0
    for path in sorted(docs_dir.rglob("*.md")):
        original = path.read_text()
        lines = original.splitlines()
        lines = strip_noise(lines)
        lines = unwrap_doc_blocks(lines)
        lines = expand_trait_method_docs(lines, trait_docs)
        updated = "\n".join(lines) + "\n"
        if updated != original:
            path.write_text(updated)
            changed += 1

    print(f"postprocess-docs: rewrote {changed} file(s) "
          f"({len(trait_docs)} trait method doc bodies available)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
