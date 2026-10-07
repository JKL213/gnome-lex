#!/bin/sh
# Startet broadwayd und die App in der SDK-Sandbox (Testumgebung, headless)
# auf einem eigenen Session-Bus (bus.sock), damit eine parallel laufende
# Entwicklungsinstanz den D-Bus-Namen nicht blockiert.
here=/home/janhk/SVN/gnome-lex/gnome-lex
exec flatpak run --command=sh --no-documents-portal --no-a11y-bus --no-session-bus \
  --filesystem=home --share=network --share=ipc \
  --env=DBUS_SESSION_BUS_ADDRESS=unix:path=/home/janhk/.cache/gnome-lex/bus.sock \
  --env=PATH=/usr/lib/sdk/rust-stable/bin:/usr/bin:/bin \
  --env=XDG_DATA_HOME=/home/janhk/.cache/gnome-lex/testdata \
  --env=GSETTINGS_BACKEND=keyfile --env=XDG_CONFIG_HOME=/home/janhk/.cache/gnome-lex/testdata/config \
  --env=RUST_LOG=info \
  --env=GDK_BACKEND=broadway --env=BROADWAY_DISPLAY=:7 \
  org.gnome.Sdk//51 -c 'gtk4-broadwayd :7 & sleep 1; exec '"$here"'/target-sdk/debug/gesetze'
