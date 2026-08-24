#!/usr/bin/env python3
"""Post-process the markdown cargo-docs-md generates for docs/.

Five passes, run in order over every `*.md` under the docs directory:

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

4. resolve_prose_links -- turn the rustdoc intra-doc links inside doc-comment
   prose into real relative markdown links. cargo-docs-md resolves the links
   it generates itself (signature types, implementor lists) but passes prose
   links through verbatim, so `[`Foo::bar`]` renders as literal brackets and
   `[text](crate::Foo)` as a dead href -- 182 of them across the tree.
   Members resolve to their owning type's anchor, since only some members
   carry an anchor of their own; anything unresolvable is demoted to a plain
   code span rather than left as broken markup.

5. drop_dangling_anchors -- strip link fragments naming an anchor the target
   page doesn't have. cargo-docs-md writes `mod/index.md#mod` for every
   module link, but a module page is headed "# Module `mod`" and carries no
   such anchor. This runs tree-wide after every page has its final headings.

Passes 3 and 4 need the same rustdoc JSON that fed cargo-docs-md; pass one
JSON path per doc pass the Makefile runs (host + esp-idf), since each sees a
different cfg-gated slice of the crate.
"""

import json
import os
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


MODULE_LEVEL_KINDS = frozenset(
    {
        "module",
        "struct",
        "enum",
        "trait",
        "function",
        "constant",
        "static",
        "type_alias",
        "macro",
        "union",
        "trait_alias",
    }
)


def _slug(name: str) -> str:
    return name.lower().replace("_", "-")


def build_item_index(json_paths: list[Path]) -> dict[str, tuple[str, str]]:
    """Map a rust path (and its unambiguous short forms) -> (docs file, anchor).

    cargo-docs-md derives an item's anchor from its name by lowercasing and
    turning `_` into `-`, and puts every item of a module in that module's
    `index.md`. Confirmed against 277 of the 278 links the tool resolves on
    its own; the lone exception is a generic impl heading, which nothing
    links to by name.
    """
    index: dict[str, tuple[str, str]] = {}
    ambiguous: set[str] = set()

    def register(key: str, target: tuple[str, str]) -> None:
        if key in ambiguous:
            return
        if index.get(key, target) != target:
            del index[key]
            ambiguous.add(key)
            return
        index[key] = target

    for json_path in json_paths:
        data = json.loads(json_path.read_text())
        for entry in data.get("paths", {}).values():
            path = entry.get("path") or []
            kind = entry.get("kind")
            if len(path) < 2 or entry.get("crate_id") != 0 or kind not in MODULE_LEVEL_KINDS:
                # Members (variants, methods, assoc items) carry their owning
                # type as a path segment, which would read as a directory.
                # _resolve reaches them through the owning type instead.
                continue
            if kind == "module":
                # A module page is headed "# Module `x`", so it has no "#x"
                # anchor to aim at — the file itself is the target.
                target = ("/".join(path[1:]) + "/index.md", "")
            else:
                parents = path[1:-1]
                file = "/".join([*parents, "index.md"]) if parents else "index.md"
                target = (file, _slug(path[-1]))
            qualified = "::".join(path[1:])
            register(qualified, target)
            register(f"crate::{qualified}", target)
            register("::".join(path), target)
            register(path[-1], target)
    return index


def _resolve(ref: str, index: dict[str, tuple[str, str]], enclosing: str | None):
    """Resolve a rust path from doc prose to a (file, anchor) pair, or None.

    A member path (`Type::method`, `Enum::Variant`) falls back to its owning
    type's anchor: members are rendered as bullets under the type, and only
    some of them carry an anchor of their own, so the type's heading is the
    one target that is always right.
    """
    ref = ref.strip().removesuffix("()").removesuffix("!")
    # rustdoc disambiguators (`enum@Error`, `fn@connect`) name the item kind,
    # which the path index already knows.
    ref = ref.rpartition("@")[2]
    if ref.startswith("Self::") and enclosing:
        ref = f"{enclosing}::{ref[len('Self::'):]}"
    for prefix in ("crate::", "self::", "super::"):
        if ref.startswith(prefix) and ref not in index:
            ref = ref[len(prefix):]
    while ref:
        if ref in index:
            return index[ref]
        if "::" not in ref:
            return None
        ref = ref.rsplit("::", 1)[0]
    return None


def _href(source: Path, docs_dir: Path, file: str, anchor: str) -> str:
    """Build the link cargo-docs-md would have written, relative to `source`."""
    target = docs_dir / file
    suffix = f"#{anchor}" if anchor else ""
    if target.resolve() == source.resolve():
        return suffix or "#"
    rel = os.path.relpath(target, source.parent)
    return f"{rel}{suffix}"


# `[`Type::method`]` with no target of its own — rustdoc shorthand the tool
# passes through verbatim, which markdown renders as literal brackets.
BARE_REF_RE = re.compile(r"\[`([A-Za-z_][A-Za-z0-9_:<>()@!]*)`\](?![(:])")
# `[text](crate::path::Item)` — an inline link whose href is a rust path.
RUST_HREF_RE = re.compile(
    r"\[([^\]]+)\]\(((?:crate|self|super|Self)::[A-Za-z0-9_:]+|"
    r"::[a-z][A-Za-z0-9_:]*|[A-Z][A-Za-z0-9_]*::[A-Za-z0-9_:]+)\)"
)
# `### `TypeName<..>`` — the type whose section we are in, for `Self::`.
TYPE_HEADING_RE = re.compile(r"^#{2,3} `([A-Za-z_][A-Za-z0-9_]*)")


def resolve_prose_links(
    lines: list[str], source: Path, docs_dir: Path, index: dict[str, tuple[str, str]]
) -> list[str]:
    """Turn rustdoc intra-doc links in prose into real relative markdown links.

    cargo-docs-md resolves the links it generates itself (signature types,
    implementor lists) but passes prose links through as written, so every
    `[`Foo::bar`]` in a doc comment renders as literal brackets and every
    `[text](crate::Foo)` as a dead href. Anything that cannot be resolved is
    demoted to a plain code span rather than left as broken markup.
    """
    out: list[str] = []
    enclosing: str | None = None
    in_fence = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            out.append(line)
            continue
        if in_fence:
            out.append(line)
            continue

        heading = TYPE_HEADING_RE.match(line)
        if heading:
            enclosing = heading.group(1)

        def bare(match: re.Match[str]) -> str:
            ref = match.group(1)
            # rustdoc shows `enum@Error` as plain `Error`; so should we.
            shown = ref.rpartition("@")[2]
            hit = _resolve(ref, index, enclosing)
            if hit is None:
                return f"`{shown}`"
            return f"[`{shown}`]({_href(source, docs_dir, *hit)})"

        def inline(match: re.Match[str]) -> str:
            text, ref = match.group(1), match.group(2)
            hit = _resolve(ref, index, enclosing)
            if hit is None:
                return text if text.startswith("`") else f"`{text}`"
            return f"[{text}]({_href(source, docs_dir, *hit)})"

        line = RUST_HREF_RE.sub(inline, line)
        line = BARE_REF_RE.sub(bare, line)
        out.append(line)
    return out


ANCHOR_ID_RE = re.compile(r'<span id="([^"]+)"')
HEADING_RE = re.compile(r"^#{1,6} (.+?)\s*$", re.M)
LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)\s]+)\)")


def collect_anchors(text: str) -> set[str]:
    """Every fragment the generated page can actually be linked to."""
    anchors = set(ANCHOR_ID_RE.findall(text))
    for heading in HEADING_RE.findall(text):
        # cargo-docs-md slugs a heading by taking the backticked item name up
        # to any generic parameter list, lowercasing, and kebab-casing it.
        name = heading.strip("`").split("<")[0].strip()
        anchors.add(re.sub(r"[^a-z0-9-]", "", name.lower().replace("_", "-")))
    return anchors


def drop_dangling_anchors(text: str, source: Path, docs_dir: Path,
                          anchors: dict[Path, set[str]]) -> str:
    """Strip link fragments that name no anchor on the target page.

    cargo-docs-md writes `mod/index.md#mod` for every module link, but a
    module page is headed "# Module `mod`" and carries no such anchor. Where
    a file part survives the link still resolves, so only the fragment is
    dropped; a same-page link with nowhere to land becomes plain text.
    """

    def fix(match: re.Match[str]) -> str:
        text_, href = match.group(1), match.group(2)
        if href.startswith(("http://", "https://", "mailto:")):
            return match.group(0)
        file, sep, anchor = href.partition("#")
        if not sep or not anchor:
            return match.group(0)
        target = source.resolve() if not file else (source.parent / file).resolve()
        if target not in anchors or anchor in anchors[target]:
            return match.group(0)
        if file:
            return f"[{text_}]({file})"
        # The tool lists a submodule in the page's own contents and quick
        # reference as if it had a section there; it does not, but its page
        # is a subdirectory of this one.
        for name in (anchor, anchor.replace("-", "_")):
            page = source.parent / name / "index.md"
            if page.resolve() in anchors:
                return f"[{text_}]({name}/index.md)"
        return text_

    return LINK_RE.sub(fix, text)


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <docs-dir> <rustdoc.json> [<rustdoc.json>...]",
              file=sys.stderr)
        return 1

    docs_dir = Path(sys.argv[1])
    json_paths = [Path(p) for p in sys.argv[2:]]
    trait_docs = collect_trait_docs(json_paths)
    item_index = build_item_index(json_paths)

    pages = sorted(docs_dir.rglob("*.md"))
    rewritten: dict[Path, str] = {}
    for path in pages:
        lines = path.read_text().splitlines()
        lines = strip_noise(lines)
        lines = unwrap_doc_blocks(lines)
        lines = expand_trait_method_docs(lines, trait_docs)
        lines = resolve_prose_links(lines, path, docs_dir, item_index)
        rewritten[path] = "\n".join(lines) + "\n"

    # Anchors can only be checked once every page has its final headings.
    anchors = {path.resolve(): collect_anchors(text) for path, text in rewritten.items()}

    changed = 0
    for path in pages:
        updated = drop_dangling_anchors(rewritten[path], path, docs_dir, anchors)
        if updated != path.read_text():
            path.write_text(updated)
            changed += 1

    print(f"postprocess-docs: rewrote {changed} file(s) "
          f"({len(trait_docs)} trait method doc bodies available)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
