#!/bin/sh
# Bündelt die Cargo-Abhängigkeiten für `meson dist`.
set -eu
DIST="$1"
SOURCE_ROOT="$2"
cd "$SOURCE_ROOT"
mkdir -p "$DIST/.cargo"
cargo vendor "$DIST/vendor" > "$DIST/.cargo/config.toml"
