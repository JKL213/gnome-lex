---
mode: agent
description: "App-Icon nach GNOME-Richtlinien überarbeiten"
---
Überarbeite `data/icons/hicolor/scalable/apps/org.gnomelex.Gesetze.svg` und
das Symbolic-Icon nach den GNOME-Icon-Richtlinien (128×128-Raster, Perspektive
und Farbpalette der GNOME-HIG, keine Schrift als `<text>`, sondern Pfade).
Motiv: Gesetzbuch mit Paragraphenzeichen. Keine Marken, keine Namen.
Das Symbolic-Icon (16×16) muss einfarbig sein und mit `class="…"`-freiem
`fill="#2e3436"` arbeiten. Prüfe beide Dateien mit `rsvg-convert` oder
`gtk4-icon-browser`, falls verfügbar. Keine weiteren Dateien ändern.
