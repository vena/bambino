.PHONY: check-fast check-esp-idf check-all docs

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

# Wraps scripts/check-esp-idf.sh. Not run by check-fast/check-all's CI job on
# every push — see .github/workflows/esp-idf.yml for why (path-filtered, and
# the Docker volume caching that makes repeat local runs fast doesn't survive
# GitHub's ephemeral hosted runners).
check-esp-idf:
	scripts/check-esp-idf.sh $(CHIP)

check-all: check-fast check-esp-idf

# LLM-facing API reference, one markdown file per top-level module (per-crate,
# no transitive deps). cargo-doc-md nests output under an extra <crate-name>/
# dir + writes a useless single-crate index.md — flatten both away since we
# only ever document this one crate. Run manually when the public API
# actually changes; not wired into a git hook (see CLAUDE.md's cargo-doc-md
# discussion for why: post-commit can't include its own output in the commit
# that triggered it, and every commit would pay the rebuild cost regardless
# of whether the change touched the public API).
docs:
	rm -rf docs
	cargo doc-md --no-deps -o docs
	rm -f docs/index.md
	mv docs/bambino/* docs/
	rmdir docs/bambino
