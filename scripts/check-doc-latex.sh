#!/usr/bin/env bash
# Rejects LaTeX \text{} in tracked markdown and Rust doc comments.
#
# GitHub's markdown renderer (KaTeX) fails on \text{} containing an escaped
# underscore -- `$\text{ams\_id}$` renders as the literal error string
# "'_' allowed only in math mode" instead of the formula. Every "formula" in
# this repo is a protocol identifier or integer arithmetic, so the fix is
# always to write it as a code span or fenced block, never to repair the
# LaTeX. This check enforces that decision.
#
# It matches `\text{` specifically, NOT `$...$`. A `$...$` matcher would fire
# on shell snippets ($PATH), Make variables ($(CHIP)), G-code samples, and
# backticked identifiers like `$nozzle_count` in src/quirks/models/h2.rs --
# all legitimate, all rendering fine. `\text{` has no such collisions.
#
# Scans `git ls-files` so untracked build output (.embuild/, target/) is
# excluded automatically; vendored ESP-IDF markdown does use LaTeX and is not
# ours to fix.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Test the captured output, not the exit status: xargs may split the file list
# into several grep invocations and returns 123 if ANY of them found nothing,
# which would mask a real hit found by one of the other batches. -H forces the
# filename even when a batch happens to hold a single file.
hits="$(git ls-files -z -- '*.md' '*.rs' | xargs -0 grep -Hn '\\text{' || true)"

if [ -n "$hits" ]; then
    printf 'ERROR: LaTeX \\text{} found in docs -- GitHub'"'"'s KaTeX renderer breaks on it.\n' >&2
    echo "Rewrite the expression as a code span or a fenced block:" >&2
    echo "$hits" >&2
    exit 1
fi
