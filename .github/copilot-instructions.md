# Copilot-Anweisungen für dieses Repository

Lies zuerst `AGENTS.md`; dort stehen die verbindlichen Regeln. Kurzfassung:

- Sprache: Rust stable, GTK 4 (`gtk4` 0.11), libadwaita (`libadwaita` 0.9,
  Feature `v1_9`), glib/gio 0.22. UI nur als Blueprint (`data/ui/*.blp`).
- Kein WebKit, kein HTML, keine Web-Technologien. Kein reqwest (Netzwerk
  läuft über `soup3`). Keine Linux-spezifischen Pfade oder APIs.
- Kein Branding, keine Firmen- oder Personennamen. App-ID `org.gnomelex.Gesetze`.
- Deutsch für Kommentare, Doc-Kommentare, UI-Strings und Commit-Nachrichten.
  UI-Strings immer über `gettext` bzw. `_()` in Blueprint.
- Jede Änderung muss `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  und `cargo test` bestehen. Neue Parserfälle in `src/importer` oder `src/refs`
  bekommen einen Unit-Test.
- Nichts blockiert den Hauptthread: blockierende Arbeit in `gio::spawn_blocking`,
  asynchrone Arbeit über `glib::MainContext::spawn_local` oder `glib::spawn_future_local`.
- Neue Blueprint-Dateien in `data/gesetze.gresource.xml`, `data/meson.build`
  und `po/POTFILES` eintragen; neue Rust-Dateien mit UI-Strings in `po/POTFILES`.
- `README.md` niemals bearbeiten, auch nicht zum Verlinken neuer Dokumente.
- Halte dich an den Umfang der gestellten Aufgabe. Keine neuen Stufen des
  Entwicklungsplans beginnen; die sind der Hauptentwicklung vorbehalten.

## Lokaler Build

`libsoup3-devel` kann fehlen. Dann vor Cargo-Befehlen setzen:

```sh
export PKG_CONFIG_PATH=$HOME/.cache/gnome-lex/pc
```

Der Shim liegt dort (siehe `AGENTS.md`). Der Flatpak-Build braucht ihn nicht.
