.PHONY: check-fast check-esp-idf check-all

CHIP ?= esp32c6

# Default host build/test + both feature-gate checks from CLAUDE.md + clippy.
# This is the full local verification gate short of the esp-idf Docker check.
check-fast:
	cargo build
	cargo build --bin bambino-cli --features cli
	cargo test
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
