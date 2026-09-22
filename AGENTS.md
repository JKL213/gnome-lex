# AGENTS.md

Hinweise für KI-Agenten und Mitwirkende, die an diesem Repository arbeiten.

## Projekt

GNOME-Desktop-Anwendung zum Lesen und Annotieren deutscher Bundesgesetze
(zunächst BGB). Rust, GTK 4, libadwaita, Blueprint, Meson, Flatpak.
App-ID: `org.gnomelex.Gesetze`. Binärname: `gesetze`.

## Harte Regeln

- Nur GTK 4 und libadwaita. Kein WebKitGTK, kein HTML, keine Web-Technologien.
- UI ausschließlich als Blueprint (`data/ui/*.blp`), eingebunden über
  `#[derive(CompositeTemplate)]` und GResource.
- Kein blockierender Code im Hauptthread: Netzwerk über `soup3` (async),
  Import über `gio::spawn_blocking` bzw. `glib::MainContext::spawn_local`.
- Keine Linux-spezifischen Pfade: `glib::user_data_dir()`, GSettings mit
  Fallback (`src/settings.rs`). Windows über MSYS2 muss möglich bleiben.
- Datenhaltung: `rusqlite` (bundled, FTS5). XML: `quick-xml`. Zip: `zip`.
- Kein Branding, keine Firmen- oder Personennamen im Code, in Metadaten
  oder in der UI.
- Texte in der UI über `gettext` (`gettextrs`) übersetzbar halten; neue
  Quelldateien mit UI-Strings in `po/POTFILES` eintragen.
- GNOME Human Interface Guidelines beachten.

## Qualitätssicherung

Vor jedem Abschluss müssen durchlaufen:

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build
```

Unit-Tests für `importer` und `refs` sind Pflicht; neue Parserfälle
bekommen einen Test.

## Struktur

| Pfad             | Inhalt                                                  |
|------------------|---------------------------------------------------------|
| `src/importer/`  | Download, Entpacken, gii-norm-XML-Parser                |
| `src/db/`        | SQLite-Schema, Abfragen, Annotationen, Neuverankerung   |
| `src/model/`     | Block-Modell des Normtexts, GObject-Datenobjekte        |
| `src/refs/`      | Erkennung von Normverweisen                             |
| `src/widgets/`   | Eigene Widgets                                          |
| `src/window.rs`  | Hauptfenster                                            |
| `src/application.rs` | AdwApplication, Aktionen, Dialoge                   |
| `data/ui/`       | Blueprint-Dateien                                       |
| `data/`          | Schema, Desktop-/Metainfo-Datei, Icons, CSS             |
| `build-aux/`     | Flatpak-Manifest, Hilfsskripte                          |
| `docs/`          | Windows-Build und weitere Dokumentation                 |

## Bauen

`cargo build` allein reicht (build.rs kompiliert Blueprint, GResource,
Schema). Meson: `meson setup _build -Dprofile=development && meson compile -C _build`.
Flatpak: `flatpak-builder --user --install --force-clean _flatpak build-aux/org.gnomelex.Gesetze.Devel.json`.

Fehlt `libsoup3-devel` lokal, kann für die Entwicklung ein pkg-config-Shim
über `PKG_CONFIG_PATH` gesetzt werden; der Flatpak-Build braucht das nicht.

## Vorgehen

Die Entwicklung läuft in nummerierten Stufen (siehe Aufgabenbeschreibung):
Gerüst, Importer, Gliederung/Normanzeige, Tabs/geteilte Ansicht, Suche,
Verweise, Annotationen, Windows-Build. Nach jeder Stufe muss die App
lauffähig sein. **Eine neue Stufe wird erst auf ausdrückliche Anweisung
begonnen.**
