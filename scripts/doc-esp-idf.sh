#!/usr/bin/env bash
# Generates rustdoc JSON for the `esp-idf` feature target inside the matching
# `espressif/idf-rust` Docker image — same reason as check-esp-idf.sh:
# esp-idf-sys's build script needs Python/cmake/ninja/the ESP-IDF SDK, unavailable
# on host. Writes target/esp-idf-doc-<chip>.json (host-side, real filesystem, not
# the container's cached target/ volume) for `make docs` to convert to markdown on
# the host via `cargo docs-md --path` — cargo-docs-md itself is never installed
# inside the container, only rustdoc's JSON output is generated there.
#
# Usage: scripts/doc-esp-idf.sh [chip]   (default esp32c6)
#
# Not wired into CI — run manually (via `make docs`) before landing changes to
# src/io/esp_idf.rs or anything gated on #[cfg(feature = "esp-idf")].
set -euo pipefail

CHIP="${1:-esp32c6}"
IMAGE="espressif/idf-rust:${CHIP}_latest"

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

# Same named volumes as check-esp-idf.sh (shared cache across both scripts —
# doc generation and the compile check use the same dependency graph and target
# triple, so there's no reason to duplicate the cargo registry/rust-src/target
# cache under a separate name).
CARGO_REGISTRY_VOLUME="bambino-esp-idf-cargo-registry"
CARGO_GIT_VOLUME="bambino-esp-idf-cargo-git"
RUSTUP_VOLUME="bambino-esp-idf-rustup"
TARGET_VOLUME="bambino-esp-idf-target-${CHIP}"

docker run --rm \
  -v "${CARGO_REGISTRY_VOLUME}:/home/esp/.cargo/registry" \
  -v "${CARGO_GIT_VOLUME}:/home/esp/.cargo/git" \
  -v "${RUSTUP_VOLUME}:/home/esp/.rustup" \
  -v "${TARGET_VOLUME}:/workspace/target" \
  --user root \
  "${IMAGE}" \
  chown -R esp:esp /home/esp/.cargo /home/esp/.rustup /workspace/target

OUT_NAME="esp-idf-doc-${CHIP}.json"

# `/workspace/target` is the named volume above (survives --rm, not visible on the
# host) — rustdoc's JSON output lands there first, then gets copied to
# `/workspace/host-target`, a second bind mount pointing at the host's real
# `target/` directory, so the Makefile can read it back out afterward.
echo "== cargo rustdoc --target ${TARGET} --no-default-features --features esp-idf --lib -- --output-format json (chip=${CHIP}) =="
docker run --rm \
  -v "$(pwd):/workspace" \
  -v "$(pwd)/target:/workspace/host-target" \
  -v "${CARGO_REGISTRY_VOLUME}:/home/esp/.cargo/registry" \
  -v "${CARGO_GIT_VOLUME}:/home/esp/.cargo/git" \
  -v "${RUSTUP_VOLUME}:/home/esp/.rustup" \
  -v "${TARGET_VOLUME}:/workspace/target" \
  -w /workspace \
  -e "IDF_TARGET=${CHIP}" \
  -e "MCU=${CHIP}" \
  -e "RUSTC_BOOTSTRAP=1" \
  "${IMAGE}" \
  bash -lc "source \"\$HOME/export-esp.sh\" 2>/dev/null || true; rustup component add rust-src; cargo rustdoc -Z build-std=std,panic_abort --target ${TARGET} --no-default-features --features esp-idf --lib -- -Z unstable-options --output-format json && cp target/${TARGET}/doc/bambino.json /workspace/host-target/${OUT_NAME}"

echo "JSON written to target/${OUT_NAME}"
