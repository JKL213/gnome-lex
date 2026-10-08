#!/bin/sh
# Startet rust-analyzer in der GNOME-SDK-Sandbox (org.gnome.Sdk//51 mit
# rust-stable-Erweiterung). Der Host (Fedora 43) hat weder passende gtk4-/
# libadwaita-/glib-Entwicklungspakete noch rust-src und clippy; in der
# Sandbox stimmen Werkzeugkette, Standardbibliothek, Proc-Macro-Server und
# Systembibliotheken überein.
#
# Wird in `.vscode/settings.json` als `rust-analyzer.server.path`
# eingetragen. Das Sprachserver-Protokoll läuft über stdin/stdout.
#
# Der Build landet wie bei `sdk-cargo.sh` unter `target-sdk/`
# (rust-analyzer nutzt dort wegen `cargo.targetDir` ein eigenes
# Unterverzeichnis und sperrt laufende Builds nicht).
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
runtime=${GNOME_SDK:-org.gnome.Sdk//51}
exec flatpak run \
  --command=/usr/lib/sdk/rust-stable/bin/rust-analyzer \
  --no-documents-portal --no-a11y-bus \
  --filesystem=home \
  --share=network \
  --env=PATH=/usr/lib/sdk/rust-stable/bin:/usr/bin:/bin \
  --env=CARGO_TARGET_DIR="$here/target-sdk" \
  "$runtime" "$@"
