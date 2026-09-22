# GNOMELex oder Gnome-Lex

GNOMELex ist eine mit dem GTK-Toolkit für GNOME-Desktopumgebungen zusammengestellte App zum Lesen deutscher Gesetze. 
Sie befindet sich in reger Entwicklung und befindet sich in einer äußerst frühen ausführbaren Version.

Als Besonderheit fügt sich GNOMELex nahtlos in bestehende GNOME-Desktops ein und erreicht somit einen schönen Look & Feel ohne nervigen Electron-/Browser-Overhead.

GNOMELex entstand aus meinem Bedarf heraus, eine GNOME-native App für Gesetzestexte im Repetitorium zu haben, und ist stark auf meine eigenen Bedürfnisse adaptiert. PRs, falls jemals welche kommen sollten, nehme ich aber natürlich immer gerne an. 

GNOMELex bezieht seine Gesetzestexte als XML von
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
