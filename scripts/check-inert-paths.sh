#!/usr/bin/env bash
# Reads newline-separated repo-relative paths on stdin. Exits 0 if EVERY path is
# known-inert (no cargo target reads it), 1 if any path could affect a build.
#
# Shared by scripts/hooks/pre-commit and scripts/hooks/pre-push so the two gates
# cannot drift apart. It lived inline in pre-commit until pre-push needed the
# same decision (#296); two copies of a fail-safe allowlist is precisely the kind
# of duplicate that goes stale silently.
#
# This is an ALLOWLIST of inert paths, not a denylist of code paths. Anything
# unrecognized is treated as code, so a new file type fails safe (runs the gate)
# rather than slipping through unchecked. Two deliberate exclusions:
#   - tests/mocks/*.json|.ndjson are NOT inert -- tests deserialize them, so a
#     fixture edit can fail the suite.
#   - Cargo.toml/Cargo.lock/build.rs/*.rs/Makefile/.cargo/ are obviously not
#     inert and are covered by falling through to the catch-all.
# MODEL_MATRIX.csv is inert: it is referenced only in doc comments, never via
# include_str!/include_bytes!.
#
# Note scripts/ itself is NOT inert: it holds the gate scripts the hooks invoke.
set -euo pipefail

while IFS= read -r path; do
    [ -z "$path" ] && continue
    case "$path" in
        *.md|LICENSE|.gitignore|MODEL_MATRIX.csv) ;;
        .claude/*|docs/*|reference/*|.github/*) ;;
        *)
            exit 1
            ;;
    esac
done

exit 0
