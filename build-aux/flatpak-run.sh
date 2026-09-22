#!/bin/sh
# Führt ein Kommando in der fertig gebauten Flatpak-Umgebung (_flatpak) aus.
# Räumt vorher hängengebliebene rofiles-fuse-Mounts auf, die nach einem
# abgebrochenen Lauf "Permission denied" verursachen.
#
# Aufruf: build-aux/flatpak-run.sh <kommando> [argumente…]
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MANIFEST="build-aux/org.gnomelex.Gesetze.Devel.json"
BUILD_DIR="${FLATPAK_BUILD_DIR:-_flatpak}"

if [ -d .flatpak-builder/rofiles ]; then
    for d in .flatpak-builder/rofiles/rofiles-*; do
        [ -d "$d" ] || continue
        if mountpoint -q "$d" 2>/dev/null; then
            fusermount3 -u "$d" 2>/dev/null || fusermount -u "$d" 2>/dev/null || true
        fi
        rmdir "$d" 2>/dev/null || true
    done
    rm -f .flatpak-builder/rofiles/rofiles-*-lock
fi

if [ ! -d "$BUILD_DIR/files" ]; then
    echo "Kein Flatpak-Build unter $BUILD_DIR. Zuerst die Aufgabe „flatpak: build“ ausführen." >&2
    exit 1
fi

exec flatpak-builder --run "$BUILD_DIR" "$MANIFEST" "$@"
