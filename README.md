# Gesetze

GNOME-Anwendung zum Lesen und Annotieren deutscher Bundesgesetze,
zunächst für das Bürgerliche Gesetzbuch (BGB). Daten stammen von
[gesetze-im-internet.de](https://www.gesetze-im-internet.de/).

## Bauen

Nur Cargo (Entwicklung):

```sh
cargo build
cargo run
```

Mit Meson (Installation, Übersetzungen, Tests):

```sh
meson setup _build -Dprofile=development
meson compile -C _build
meson test -C _build
meson install -C _build
```

Flatpak:

```sh
flatpak-builder --user --install --force-clean _flatpak build-aux/org.gnomelex.Gesetze.Devel.json
flatpak run org.gnomelex.Gesetze.Devel
```

Voraussetzungen: Rust stable, GTK 4.22, libadwaita 1.9, libsoup 3.6,
blueprint-compiler 0.20, Meson 1.0. Windows-Build: siehe
[docs/WINDOWS.md](docs/WINDOWS.md).

## Struktur

| Modul        | Aufgabe                                                   |
|--------------|-----------------------------------------------------------|
| `importer`   | Download, Entpacken und Parsen der gii-norm-XML-Dateien  |
| `db`         | SQLite (rusqlite, FTS5): Gesetze, Normen, Annotationen   |
| `model`      | GObject-Datenobjekte und das Block-Modell des Normtexts  |
| `refs`       | Erkennung von Normverweisen im Text                       |
| `widgets`    | Eigene Widgets (Gliederung, Normansicht, Seitenleisten)  |
| `window`     | Hauptfenster mit Tabs                                     |
| `application`| AdwApplication, Aktionen, Dialoge                         |
