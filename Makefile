.PHONY: check-fast check-esp-idf check-all docs install-hooks

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

check-all: check-fast check-esp-idf

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
