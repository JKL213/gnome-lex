---
applyTo: "data/ui/**/*.blp"
---
# Blueprint-Konventionen

- Kopf immer `using Gtk 4.0;` und `using Adw 1;`.
- Eigene Widgets als `template $LexName : Adw.Bin { … }` bzw. passender Elterntyp;
  Klassennamen mit Präfix `Lex`.
- Sichtbare Strings mit `_("…")` übersetzbar machen; Tooltips nicht vergessen.
- GNOME HIG: Abstände 6/12/18/24, `Adw.HeaderBar`, `Adw.ToolbarView`,
  `Adw.StatusPage` für leere Zustände, Symbolic-Icons (`*-symbolic`).
- Neue Datei in `data/gesetze.gresource.xml` (als `ui/<name>.ui`),
  `data/meson.build` (`blueprint_sources`) und `po/POTFILES` eintragen.
- Validieren mit `blueprint-compiler compile data/ui/<name>.blp`.
