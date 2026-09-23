.PHONY: check-commit check-fast check-docs check-esp-idf check-embassy-probe check-all docs install-hooks

CHIP ?= esp32c6

# Fast pre-commit subset (measured 54s, vs check-fast's 492s). This is what the
# pre-commit hook runs; scripts/hooks/pre-push runs the full check-fast before
# anything reaches main, so nothing below is *dropped* -- it moves from
# once-per-commit to once-per-push (#296).
#
# The split is measured, not guessed. Per-step timing of check-fast in the
# realistic commit case (deps warm, bambino invalidated in every feature slot,
# which is what any source edit does) totalled 492s, distributed as:
#
#   cargo test                     206s   (--lib 44s, --test integration 62s, --doc 60s)
#   cargo test embassy-host --test  71s
#   cargo test --bin bambino-cli    49s
#   cargo test embassy-host --lib    41s
#   cargo clippy --bin bambino-cli   29s
#   cargo clippy                     21s
#   cargo check embassy no-default   20s
#   cargo build --bin bambino-cli    19s
#   cargo check alloc                19s
#   cargo build                      10s
#   cargo check embassy+std           9s
#   fmt + the two scripts            <1s
#
# Two things that table settled, both contradicting the plausible guesses:
#   - The six-feature-set breadth is NOT the cost. The four cargo checks are 47s
#     combined, under 10% of the gate. The four *test* legs are 366s, 74%. So
#     narrowing feature coverage would have bought almost nothing while giving up
#     the one guarantee the matrix exists for.
#   - No step is redundant. Measured: `cargo build` after `cargo test` still costs
#     its full ~10s, and `cargo build --bin` after `cargo test --bin` costs 29s --
#     the cfg(test) and non-test builds are separate compilations, so neither pair
#     collapses. Don't "simplify" either pair into each other. (check-fast's
#     clippy has since become one `--all-targets --features cli` call, 77s
#     against the 49s pair measured above -- a coverage gain at a cost, #297.)
#
# What is here and why: fmt and the two scripts are free. `cargo clippy` type-checks
# the whole default-feature lib, so it catches a compile error without a separate
# `cargo build` (clippy cannot reuse build's fingerprints anyway -- different rustc
# wrapper). `cargo test --lib` is the cheapest leg that actually executes code.
# Deliberately absent: doctests and integration tests (60s + 62s), the CLI legs,
# and the feature matrix -- all real checks, all left to pre-push, none of them the
# thing a just-edited line usually breaks.
check-commit:
	cargo fmt --check
	scripts/check-rules-globs.sh
	scripts/check-doc-latex.sh
	cargo clippy
	cargo test --lib

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
#
# Order matters: the three ~0s checks run first so a trivial failure fails fast
# rather than after ~10min of cargo. The two scripts used to run last, behind the
# whole cargo sequence.
#
# The alloc gate is `cargo check`, not `cargo build`: it is a compile gate for
# the no_std feature combo, nothing consumes the artifact, and skipping codegen
# costs 9s instead of 15s. The embassy gate beside it has always been a `check`
# for the same reason.
#
# There are two embassy checks, and the second is not redundant. The
# `--no-default-features` one is the bare-metal shipping target. The plain
# `--features embassy` one is default features (std + tokio) *plus* embassy — the
# combination Cargo's feature unification forces when two crates in one dependency
# graph use bambino differently, and what a consumer gets from
# `cargo add bambino --features embassy` without `--no-default-features`. That
# combination was broken until `extern crate alloc` stopped being gated on
# `not(feature = "std")` (see the comment at that declaration in src/lib.rs), with
# no consumer-side workaround, and nothing in the gate would have caught it.
#
# Clippy is one `--all-targets --features cli` call, not the plain `cargo clippy`
# plus `--bin bambino-cli` pair it replaced (#297): neither of those lints test
# code, since a normal build cfg's `#[cfg(test)]` out and never builds tests/.
# `cli` only adds dependencies (no `cfg(feature = "cli")` outside src/bin/), so
# the lib lints the same either way. It stays out of check-commit: 77s against
# the 49s pair, too much for the per-commit budget above.
check-fast:
	cargo fmt --check
	scripts/check-rules-globs.sh
	scripts/check-doc-latex.sh
	cargo build
	cargo build --bin bambino-cli --features cli
	cargo test
	cargo test --bin bambino-cli --features cli
	cargo check --no-default-features --features alloc --lib
	cargo check --no-default-features --features embassy --lib
	cargo check --features embassy --lib
	$(MAKE) test-embassy-host
	cargo clippy --all-targets --features cli

# Runs the embassy backend's code on the host, which the `cargo check`es above cannot: a check
# proves io/embassy.rs compiles, never that it behaves. It runs in the `embassy` + `std`
# configuration that the `cargo check --features embassy --lib` line above now gates — not a
# shipping target, but a combination consumers reach through feature unification, so it is
# checked on its own merits. EmbassyTlsConnector needs only mbedtls-rs and an AsyncIo stream,
# not embassy-net's executor or an ESP32, which is what makes host execution possible at all.
#
# Runs `--lib` as well as `--test`: the unit-test surface used to import `crate::io::tokio` under
# a bare `cfg(test)` and so failed to build at all without the tokio feature (#291). Those tests
# now use `src/test_support.rs`'s platform-agnostic doubles, and the handful that genuinely need a
# real wall clock are gated on `feature = "tokio"` — so `--lib` here is what keeps that property
# from silently regressing, and is the only gate that executes the crate's `pub(crate)` no_std
# paths at all.
#
# Kept inside check-fast rather than check-all: it guards a backend nobody can test on
# hardware here, so the cheap host-side half should not be the part that gets skipped.
test-embassy-host:
	cargo test --no-default-features --features "embassy,std" --lib
	cargo test --no-default-features --features "embassy,std" --test embassy_tls_version_test

# Wraps scripts/check-esp-idf.sh. Not run by check-fast/check-all's CI job on
# every push — see .github/workflows/esp-idf.yml for why (path-filtered, and
# the Docker volume caching that makes repeat local runs fast doesn't survive
# GitHub's ephemeral hosted runners).
check-esp-idf:
	scripts/check-esp-idf.sh $(CHIP)

# Compiles embassy-hw-probe/ for bare-metal RISC-V. Unlike check-esp-idf this needs no
# Docker and no SDK — just `rustup target add riscv32imac-unknown-none-elf` — because the
# embassy backend is no_std and brings its own stack.
#
# `cargo build`, not `cargo check`: the one failure this catches that a check cannot is a
# link error. MbedTLS's X.509 code references `memchr`, which the bare-metal sysroot does not
# provide, and the whole crate compiles cleanly before the linker says so (hence the
# `tinyrlibc` dependency in the probe's Cargo.toml).
#
# Placeholder credentials: the probe's build.rs requires the .env keys, and a gate that only
# runs for whoever has a printer on their desk is not a gate. Process env beats the file, so
# this works whether or not a real .env exists and never reads one.
#
# Not in check-fast, same reasoning as check-esp-idf: this builds a whole bare-metal stack
# (esp-hal, esp-radio, MbedTLS) for a harness no library change can break silently — the
# library half is already covered by check-fast's two embassy gates.
check-embassy-probe:
	cd embassy-hw-probe && \
		PROBE_WIFI_SSID=ci PROBE_WIFI_PASS=ci PROBE_PRINTER_IP=127.0.0.1 \
		PROBE_SERIAL=ci PROBE_ACCESS_CODE=00000000 \
		cargo build --release

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
