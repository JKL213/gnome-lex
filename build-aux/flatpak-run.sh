#!/bin/sh
# Führt ein Kommando in der fertig gebauten Flatpak-Umgebung (_flatpak) aus.
#
# Statt `flatpak-builder --run` wird `flatpak build` direkt benutzt, weil
# flatpak-builder dafür rofiles-fuse braucht. Das schlägt fehl, wenn der
# Aufrufer (z. B. der Debug-Adapter von VS Code) fusermount nicht mit
# setuid ausführen darf.
#
# Aufruf: build-aux/flatpak-run.sh <kommandozeile…>
#
# Die Argumente werden zu einer Kommandozeile zusammengesetzt und in der
# Sandbox von `sh -c` ausgewertet. Das ist nötig, weil VS Code (cppdbg)
# "gdb --interpreter=mi" als ein einziges Argument übergibt.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BUILD_DIR="${FLATPAK_BUILD_DIR:-_flatpak}"

if [ ! -f "$BUILD_DIR/metadata" ] || [ ! -d "$BUILD_DIR/files" ]; then
    echo "Kein Flatpak-Build unter $BUILD_DIR. Zuerst die Aufgabe „flatpak: build“ ausführen." >&2
    exit 1
fi

if [ "$#" -eq 0 ]; then
    set -- sh
fi

CMDLINE="$*"
# Umgebungsvariablen für Anzeige und D-Bus in die Sandbox durchreichen.
set --
for var in WAYLAND_DISPLAY DISPLAY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS \
           XDG_SESSION_TYPE LANG LC_ALL RUST_BACKTRACE G_MESSAGES_DEBUG GTK_DEBUG; do
    eval "val=\${$var:-}"
    if [ -n "$val" ]; then
        set -- "$@" "--env=$var=$val"
    fi
done

# --nofilesystem=host: `flatpak build` bindet sonst das ganze Host-Dateisystem
# ein, und bwrap scheitert an autofs-Einhängepunkten wie /mnt/nas.
exec flatpak build \
    --with-appdir \
    --nofilesystem=host \
    --share=network --share=ipc \
    --socket=wayland --socket=fallback-x11 --socket=session-bus \
    --device=dri \
    --talk-name=org.freedesktop.portal.Desktop \
    --talk-name=org.a11y.Bus \
    "$@" \
    "$BUILD_DIR" sh -c "$CMDLINE"
