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
# no transitive deps). Combines three passes since cfg-gated platform code
# (io/embassy.rs, io/esp_idf.rs) is otherwise invisible to a default-features-only
# `cargo doc-md` run — confirmed 2026-07-06 by a missed `missing_docs` gap on
# `EspIdfTimer::new()` that no default-feature-only check could have caught.
# `embassy` builds fine on host, so it's folded into the same rustdoc pass as the
# default tokio/std build (`cargo doc-md` has no `--features` flag itself, hence
# going through `cargo rustdoc ... --output-format json` + `cargo doc-md --json`
# instead of the plain auto-generating form). `esp-idf` needs the esp-idf-sys
# Docker toolchain (scripts/doc-esp-idf.sh, mirroring scripts/check-esp-idf.sh)
# and is merged in as a second, no-clobber pass so it only adds esp-idf-exclusive
# files (e.g. io/esp_idf.md) without overwriting the richer tokio+embassy version
# of shared files (e.g. io.md) with an esp-idf-only rebuild of the same module.
# cargo-doc-md nests output under an extra <crate-name>/ dir + writes a useless
# single-crate index.md — flatten both away since we only ever document this one
# crate. Run manually when the public API actually changes; not wired into a git
# hook (see CLAUDE.md's cargo-doc-md discussion for why: post-commit can't
# include its own output in the commit that triggered it, and every commit would
# pay the rebuild cost regardless of whether the change touched the public API).
# cargo-doc-md (github.com/Crazytieguy/cargo-doc-md v0.11.0) groups trait impls
# by iterating a HashMap with no sort before printing (converter.rs
# collect_impls_for_type) -- "Trait Implementations:" and "Traits:" derive-list
# order is genuinely random per run, confirmed 2026-07-09. scripts/sort-docs.py
# canonicalizes both alphabetically after generation so `git diff` on docs/
# only shows real API changes, not shuffled sections.
docs:
	rm -rf docs
	RUSTC_BOOTSTRAP=1 cargo rustdoc --features embassy --lib -- -Z unstable-options --output-format json
	cargo doc-md --json target/doc/bambino.json -o docs
	scripts/doc-esp-idf.sh $(CHIP)
	rm -rf target/doc-md-esp-idf
	cargo doc-md --json target/esp-idf-doc-$(CHIP).json -o target/doc-md-esp-idf
	rsync -a --ignore-existing target/doc-md-esp-idf/bambino/ docs/bambino/
	rm -rf target/doc-md-esp-idf
	rm -f docs/index.md
	mv docs/bambino/* docs/
	rmdir docs/bambino
	scripts/sort-docs.py docs
