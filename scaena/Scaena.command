#!/usr/bin/env bash
# Double-click to launch the Scaena browser GUI.
cd "$(dirname "$0")"
BIN="$HOME/.local/bin/scaena"
[ -x "$BIN" ] || BIN="./target/release/scaena"
[ -x "$BIN" ] || { echo "building…"; cargo build --release -q && BIN="./target/release/scaena"; }
exec "$BIN" serve --open
