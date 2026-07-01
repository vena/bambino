#!/usr/bin/env bash
# Compiles the `esp-idf` feature target inside the matching `espressif/idf-rust`
# Docker image — the only way to actually exercise `esp-idf-sys`'s build script
# (needs Python/cmake/ninja/the ESP-IDF SDK, and for Xtensa chips a forked Rust
# toolchain) without installing that stack on the host. See CLAUDE.md.
#
# Usage:
#   scripts/check-esp-idf.sh                # esp32c6 (RISC-V, default)
#   scripts/check-esp-idf.sh esp32           # classic ESP32 (Xtensa)
#   scripts/check-esp-idf.sh esp32s3         # Xtensa
#   scripts/check-esp-idf.sh esp32c3         # RISC-V
#
# Not wired into CI — run manually before landing changes to src/io/esp_idf.rs
# or anything gated on #[cfg(feature = "esp-idf")].
set -euo pipefail

CHIP="${1:-esp32c6}"
IMAGE="espressif/idf-rust:${CHIP}_latest"

# esp-idf std targets are all tier-3 — no prebuilt std, `-Z build-std` compiles
# it from source on every run. Confirmed against docs.espressif.com/projects/rust
# for the RISC-V triples; Xtensa triples are believed to follow the same
# pattern but have NOT been run through this script — verify before trusting.
case "${CHIP}" in
  esp32)   TARGET="xtensa-esp32-espidf" ;;
  esp32s2) TARGET="xtensa-esp32s2-espidf" ;;
  esp32s3) TARGET="xtensa-esp32s3-espidf" ;;
  esp32c2) TARGET="riscv32imc-esp-espidf" ;;
  esp32c3) TARGET="riscv32imc-esp-espidf" ;;
  esp32c6) TARGET="riscv32imac-esp-espidf" ;;
  esp32h2) TARGET="riscv32imac-esp-espidf" ;;
  *)
    echo "unknown chip '${CHIP}' — no known target triple mapping, add one to this script" >&2
    exit 1
    ;;
esac

echo "== pulling ${IMAGE} (skips if already present) =="
docker pull "${IMAGE}"

# `docker run --rm` throws away the container filesystem every run — without
# persistent volumes, that means re-downloading every crate, recompiling `std`
# from source, and reinstalling `rust-src` from scratch each time. Named
# volumes survive `--rm` (only the container is removed, not the volume), so
# only the first run per chip pays the full cost. Kept separate from the
# bind-mounted repo (and per-chip, since target triples differ) so this never
# collides with the host's own `target/` used by the default tokio build.
CARGO_REGISTRY_VOLUME="bambino-esp-idf-cargo-registry"
CARGO_GIT_VOLUME="bambino-esp-idf-cargo-git"
RUSTUP_VOLUME="bambino-esp-idf-rustup"
TARGET_VOLUME="bambino-esp-idf-target-${CHIP}"

# Docker auto-creates named volumes as root-owned on first use, but the image runs
# as non-root `esp` (uid 1000) — so a brand-new volume is unwritable by the build
# until it's chowned once. Safe to run every time: a no-op on already-owned volumes.
docker run --rm \
  -v "${CARGO_REGISTRY_VOLUME}:/home/esp/.cargo/registry" \
  -v "${CARGO_GIT_VOLUME}:/home/esp/.cargo/git" \
  -v "${RUSTUP_VOLUME}:/home/esp/.rustup" \
  -v "${TARGET_VOLUME}:/workspace/target" \
  --user root \
  "${IMAGE}" \
  chown -R esp:esp /home/esp/.cargo /home/esp/.rustup /workspace/target

echo "== cargo check --target ${TARGET} --no-default-features --features esp-idf --lib (chip=${CHIP}) =="
docker run --rm \
  -v "$(pwd):/workspace" \
  -v "${CARGO_REGISTRY_VOLUME}:/home/esp/.cargo/registry" \
  -v "${CARGO_GIT_VOLUME}:/home/esp/.cargo/git" \
  -v "${RUSTUP_VOLUME}:/home/esp/.rustup" \
  -v "${TARGET_VOLUME}:/workspace/target" \
  -w /workspace \
  -e "IDF_TARGET=${CHIP}" \
  -e "MCU=${CHIP}" \
  "${IMAGE}" \
  bash -lc "source \"\$HOME/export-esp.sh\" 2>/dev/null || true; rustup component add rust-src; cargo check -Z build-std=std,panic_abort --target ${TARGET} --no-default-features --features esp-idf --lib"
