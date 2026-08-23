.PHONY: check-fast check-esp-idf check-all docs install-hooks

CHIP ?= esp32c6

# Default host build/test + both feature-gate checks from CLAUDE.md + clippy.
# This is the full local verification gate short of the esp-idf Docker check.
check-fast:
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
# scripts/strip-doc-noise.py deletes blanket-impl noise cargo-docs-md doesn't
# filter on its own (AsTaggedExplicit/AsTaggedImplicit/StructuralPartialEq/
# StructuralEq showing up on every type) — see that script's docstring.
#
# WHAT ENDS UP IN docs/: item signatures, links, and type structure — NOT the
# prose bodies of /// doc comments. Verified 2026-08-23: rewriting several
# paragraphs of TlsConnector::peer_chain_der's doc comment produced an empty
# `git status`, while the commit that added the method itself changed 42 lines
# across four docs/io/* files. So a regen is only worth running when the API
# *shape* changes (items added/removed/renamed, signatures changed); a
# prose-only doc-comment edit provably cannot change the output and does not
# need this multi-minute Docker pass. Losing the prose is a real limitation of
# the current pipeline rather than a deliberate choice — see issue #143.
docs:
	@docker info >/dev/null 2>&1 || { echo "ERROR: docker unreachable — the esp-idf doc pass (scripts/doc-esp-idf.sh) needs it. A host-only regen silently DELETES docs/io/esp_idf/*: the esp-idf pass is what creates those files, and this target starts with 'rm -rf docs'. If that has already happened, revert docs/ rather than committing it. Start Docker and retry." >&2; exit 1; }
	rm -rf docs
	RUSTC_BOOTSTRAP=1 cargo rustdoc --features embassy --lib -- -Z unstable-options --output-format json
	cargo docs-md --path target/doc/bambino.json -o docs --format nested
	scripts/doc-esp-idf.sh $(CHIP)
	rm -rf target/doc-md-esp-idf
	cargo docs-md --path target/esp-idf-doc-$(CHIP).json -o target/doc-md-esp-idf --format nested
	rsync -a --ignore-existing target/doc-md-esp-idf/ docs/
	rm -rf target/doc-md-esp-idf
	rm -rf host-target
	scripts/strip-doc-noise.py docs
	@echo
	@echo "make docs: regenerated signatures/links only — /// prose bodies are not emitted (issue #143)."
	@git diff --quiet -- docs && echo "make docs: no changes to docs/ — expected if this edit only touched doc-comment prose." || true
