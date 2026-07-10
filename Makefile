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
docs:
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
