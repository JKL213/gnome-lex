#!/bin/sh
# Führt Cargo-Befehle in der GNOME-SDK-Sandbox aus (org.gnome.Sdk//51 mit
# rust-stable-Erweiterung). Nützlich, wenn auf dem Host gtk4-devel,
# libadwaita-devel oder eine passende glib-Version fehlen.
#
# Beispiele:
#   build-aux/sdk-cargo.sh build
#   build-aux/sdk-cargo.sh clippy -- -D warnings
#   build-aux/sdk-cargo.sh test
#
# Der Build landet in `target-sdk/`, damit er sich nicht mit einem nativen
# `target/` vermischt. Das Cargo-Registry-Verzeichnis des Hosts wird geteilt.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
runtime=${GNOME_SDK:-org.gnome.Sdk//51}
exec flatpak run \
  --command=sh \
  --no-documents-portal --no-a11y-bus \
  --filesystem=home \
  --share=network \
  --socket=wayland --socket=fallback-x11 --share=ipc --device=dri \
  --env=PATH=/usr/lib/sdk/rust-stable/bin:/usr/bin:/bin \
  --env=CARGO_TARGET_DIR="$here/target-sdk" \
  "$runtime" -c 'cd "$1" && shift && exec cargo "$@"' sh "$here" "$@"
