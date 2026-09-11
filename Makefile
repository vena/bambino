.PHONY: check-fast check-docs check-esp-idf check-all docs install-hooks

CHIP ?= esp32c6

# Default host build/test + both feature-gate checks from CLAUDE.md + clippy.
# This is the full local verification gate short of the esp-idf Docker check.
#
# `cargo fmt --check` runs first: it is the cheapest check here, and formatting
# drift is otherwise invisible. The PostToolUse rustfmt hook in
# .claude/settings.local.json formats one file per edit, and rustfmt recurses
# through `mod` declarations, so a leaf-module edit never reaches the rest of
# the crate. Before this check existed the tree drifted to 270 diff sites
# unnoticed, and the first src/lib.rs edit triggered a crate-wide pass that
# rewrote 41 untouched files into an unrelated commit. Use `cargo fmt` (not a
# bare `rustfmt <file>`) to fix a failure -- it covers the CLI binary and
# tests/, which a pass rooted at src/lib.rs cannot reach.
check-fast:
	cargo fmt --check
	cargo build
	cargo build --bin bambino-cli --features cli
	cargo test
	cargo test --bin bambino-cli --features cli
	cargo build --no-default-features --features alloc --lib
	cargo check --no-default-features --features embassy --lib
	cargo clippy
	cargo clippy --bin bambino-cli --features cli
	scripts/check-rules-globs.sh
	scripts/check-doc-latex.sh

# Wraps scripts/check-esp-idf.sh. Not run by check-fast/check-all's CI job on
# every push — see .github/workflows/esp-idf.yml for why (path-filtered, and
# the Docker volume caching that makes repeat local runs fast doesn't survive
# GitHub's ephemeral hosted runners).
check-esp-idf:
	scripts/check-esp-idf.sh $(CHIP)

# Intra-doc link gate. Broken `[...]` links compile, test, and clippy clean, so
# before this target nothing rejected them: a single session accumulated 15
# (13 `crate::PrinterClient::foo` links for a type that is not re-exported at
# the crate root, plus a `QuirkStrategy` link left dead by the rename to
# `ModelQuirks` that had survived every gate indefinitely). For a published
# crate the failure mode is a docs.rs page whose links go nowhere.
#
# Deliberately NOT part of check-fast, and invoked from .github/workflows/ci.yml
# as its own step: rustdoc does not reuse `cargo build` artifacts, and the
# pre-commit hook already runs check-fast on every commit. It is cheap (~9s warm
# after a src/ touch, vs ~10min for check-fast), so folding it in would be
# affordable -- the separation is about keeping a class of problem that never
# blocks anything at commit time off the commit path, not about the 9s.
#
# Default features on purpose, NOT `--features embassy` as the `docs` target
# below uses. Two reasons: that combo (tokio+std+embassy) is one nothing else
# builds, so it costs ~205s cold instead of ~9s; and two rustdoc invocations for
# the same crate+features share one fingerprint slot, so matching `docs`' flags
# would make each run invalidate the other's cached rustdoc output. Different
# feature sets keep them in separate slots. `cargo doc` writes HTML plus an
# index and never touches target/doc/bambino.json (verified by md5 before/after),
# which is the only target/doc artifact the `docs` target consumes -- so the two
# cannot clobber each other.
#
# The gap this leaves: cfg-gated backends (io/embassy.rs, io/esp_idf.rs) are
# invisible to a default-features doc run, so their intra-doc links stay
# ungated. Every warning actually observed so far was in default-feature code.
#
# -D warnings makes rustdoc's warnings fail the build rather than scroll past.
# The tree is clean at 0 warnings as of this target landing. If a future
# `#[doc(hidden)]` or cfg-gated item makes it noisy, the escape hatch is
# `--document-private-items` or a targeted `#[allow]`, not dropping the check.
check-docs:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

check-all: check-fast check-docs check-esp-idf

install-hooks:
	scripts/install-hooks.sh

# LLM-facing API reference, one markdown file per module (per-crate, no
# transitive deps), via cargo-docs-md (github.com/consistent-milk12/docs-md).
# Combines three passes since cfg-gated platform code (io/embassy.rs,
# io/esp_idf.rs) is otherwise invisible to a default-features-only doc run.
# `embassy` builds fine on host, so it shares the tokio/std rustdoc pass;
# `esp-idf` needs the esp-idf-sys Docker toolchain (scripts/doc-esp-idf.sh,
# mirroring scripts/check-esp-idf.sh) and is merged in as a second, no-clobber
# pass so it only adds esp-idf-exclusive files without overwriting the richer
# shared ones. Run manually when the public API changes; not wired into a git
# hook (post-commit can't include its own output in the triggering commit, and
# every commit would pay the rebuild cost regardless of relevance).
#
# scripts/postprocess-docs.py runs five passes over the merged output: it
# deletes blanket-impl noise cargo-docs-md doesn't filter on its own, undoes
# the paragraph split --full-method-docs introduces between every source line
# of a doc comment, fills in trait *declaration* method bodies from the
# rustdoc JSON (--full-method-docs only reaches methods inside impl blocks),
# resolves the intra-doc links inside prose into real relative links, and
# strips link fragments naming an anchor the target page doesn't have --
# see that script's docstring. Both JSON files are passed because each doc
# pass sees a different cfg-gated slice of the crate.
#
# WHAT ENDS UP IN docs/: item signatures, links, type structure, and the prose
# bodies of /// doc comments. The prose was missing until #143; a doc-comment
# edit now changes the output, so regen after prose edits too, not only after
# the API *shape* changes.
docs:
	@docker info >/dev/null 2>&1 || { echo "ERROR: docker unreachable — the esp-idf doc pass (scripts/doc-esp-idf.sh) needs it. A host-only regen silently DELETES docs/io/esp_idf/*: the esp-idf pass is what creates those files, and this target starts with 'rm -rf docs'. If that has already happened, revert docs/ rather than committing it. Start Docker and retry." >&2; exit 1; }
	rm -rf docs
	RUSTC_BOOTSTRAP=1 cargo rustdoc --features embassy --lib -- -Z unstable-options --output-format json
	cargo docs-md --path target/doc/bambino.json -o docs --format nested --full-method-docs
	scripts/doc-esp-idf.sh $(CHIP)
	rm -rf target/doc-md-esp-idf
	cargo docs-md --path target/esp-idf-doc-$(CHIP).json -o target/doc-md-esp-idf --format nested --full-method-docs
	rsync -a --ignore-existing target/doc-md-esp-idf/ docs/
	rm -rf target/doc-md-esp-idf
	rm -rf host-target
	scripts/postprocess-docs.py docs target/doc/bambino.json target/esp-idf-doc-$(CHIP).json
	@echo
	@git diff --quiet -- docs && echo "make docs: no changes to docs/." || true
