#!/usr/bin/env bash
# Install Scaena in-place: build release, symlink `scaena` + `scaena-mcp` into ~/.local/bin.
set -euo pipefail
cd "$(dirname "$0")"

BIN_DIR="${SCAENA_BIN:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"

echo "building release…"
cargo build --release -q

for b in scaena scaena-mcp; do
  ln -sf "$(pwd)/target/release/$b" "$BIN_DIR/$b"
  echo "  linked $BIN_DIR/$b -> target/release/$b"
done

echo "done. ensure $BIN_DIR is on PATH, then: scaena version"
