# Gesetze (gnome-lex) – Übergabe an die nächste Claude-Code-Instanz

Diese Datei fasst den Stand und alle Vereinbarungen aus der bisherigen
Sitzung zusammen. Lies zusätzlich `AGENTS.md` (verbindliche Regeln).
Die Zielplattform bleibt **GNOME (GTK 4 + libadwaita)**, auch wenn die
Entwicklung unter Fedora KDE weitergeht. Keine Qt/KDE-Abhängigkeiten.
Einrichtung der Entwicklungsumgebung unter KDE: `docs/SETUP-FEDORA-KDE.md`.

---

## 1. Verbindliche Arbeitsregeln (vom Auftraggeber festgelegt)

1. **Vor jeder neuen Entwicklungsstufe fragen.** Eine Stufe wird erst auf
   ausdrückliches Kommando begonnen. Innerhalb einer laufenden Stufe darf
   selbstständig gearbeitet werden, bis sie lauffähig abgeschlossen ist.
2. **`README.md` nicht bearbeiten** (gilt für Claude und Copilot).
3. **Kein Branding, keine Firmen- oder Personennamen** in Code, Metadaten,
   UI oder Doku. App-ID ist `org.gnomelex.Gesetze` (Devel-Variante
   `org.gnomelex.Gesetze.Devel`), Entwicklername „Gnome Lex“,
   Platzhalter-URLs `https://github.com/gnome-lex/gnome-lex`.
4. **Commits nur auf Anweisung.** Bisher gibt es keinen Commit; alle Dateien
   sind mit `git add` vorgemerkt (Branch `main`).
5. **`.github/prompts/` bleibt lokal** (in `.gitignore`), die Dateien werden
   nicht committet, dürfen aber weiter genutzt und ergänzt werden.
6. GitHub Copilot bekommt nur kleine, klar abgegrenzte Aufgaben (Tests,
   Übersetzungen, Doku, Icons), nie Entwicklungsstufen.
7. Deutsch für Kommentare, UI-Strings, Doku und Commit-Nachrichten.

---

## 2. Ursprüngliche Aufgabenstellung (Kurzfassung, vollständig gültig)

GNOME-native Desktop-App zum Lesen und Annotieren deutscher Bundesgesetze,
zunächst nur BGB.

**Harte Vorgaben:** nur GTK 4 + libadwaita (aktuell), kein WebKit/HTML/
Electron; Rust stable mit gtk4, libadwaita, glib, gio; UI in Blueprint
(.blp) als Composite Templates über GResource; Build über Meson, das Cargo
aufruft, `cargo build` allein muss ebenfalls funktionieren (build.rs
kompiliert Blueprint, GResource, Schema); Ziel Linux-Flatpak
(org.gnome.Platform, rust-stable), sekundär Windows/MSYS2, daher keine
Linux-Pfade (glib::user_data_dir, GSettings mit Fallback); GNOME HIG;
gettext-rs vorbereitet; `cargo clippy -- -D warnings` und `cargo fmt --check`
müssen bestehen.

**Architektur:** eigene Widgets/Datenobjekte als GObject-Subklassen;
gio::ListStore, GtkListView, GtkTreeListModel (Gliederung als Baum); nichts
blockiert den Hauptthread (gio::spawn_blocking, MainContext::spawn_local);
Netzwerk über soup3; rusqlite „bundled“ (FTS5); quick-xml; zip; serde_json;
Module importer, db, model, widgets, window, application (zusätzlich refs
für den Verweisparser).

**Datenquelle:** https://www.gesetze-im-internet.de/bgb/xml.zip (DTD
gii-norm). Importer schreibt Gliederung, Normen, Absätze, Fußnoten,
Tabellen, Stand-Vermerk nach SQLite; FTS5-Volltextsuche; Update-Funktion
vergleicht den Stand-Vermerk.

**Libadwaita:** AdwApplicationWindow + AdwNavigationSplitView (Gliederung
links, Text rechts); AdwTabView/TabBar/TabOverview (mehrere Tabs desselben
Gesetzes an verschiedenen Stellen); optional geteilte Ansicht (GtkPaned)
pro Tab; AdwBreakpoint bis Smartphone-Breite; AdwOverlaySplitView rechts
für Notizen/Lesezeichen; ToastOverlay, AlertDialog, PreferencesDialog,
AboutDialog; AdwStyleManager (hell/dunkel, Akzentfarbe); GtkSearchBar mit
Live-Suche; Kürzel über GtkShortcutController plus Übersicht im Hilfemenü;
GSettings für Fenster, offene Tabs, Schriftgröße mit Wiederherstellung.

**Text:** GtkTextView mit TextTags (Überschriften, Absatznummern, Fußnoten,
Hervorhebungen); Tabellen als GtkGrid über GtkTextChildAnchor; Tabs
desselben Gesetzes teilen sich, wo sinnvoll, den GtkTextBuffer.

**Verweise:** Parser erkennt „§ 280 Abs. 1“, „§§ 434 bis 437“,
„Artikel 229 EGBGB“ usw. als klickbare Tags; Klick öffnet, Strg+Klick in
neuem Tab, Hover zeigt GtkPopover-Vorschau; manuelle Verweise zwischen
beliebigen Textstellen.

**Annotationen:** Markierungen (mehrere Farben), Notizen, Lesezeichen;
sichtbar im Text (farbige Tags, Symbol per TextChildAnchor, Notiztext in
Popover und Seitenleiste); Speicherung in SQLite, verankert über Norm-ID,
Absatz, Zeichenoffset und Wortlaut, damit sie nach Updates wiedergefunden
oder als verwaist gemeldet werden; JSON-Export/-Import.

**Stufen:** 1 Gerüst + leeres Fenster · 2 XML-Importer mit Tests ·
3 Gliederung und Normanzeige · 4 Tabs und geteilte Ansicht · 5 Suche ·
6 Verweiserkennung und Navigation · 7 Annotationen · 8 Windows-Build
dokumentieren und testen. Nach jeder Stufe muss die App lauffähig sein;
Unit-Tests für Importer und Verweisparser sind Pflicht.

---

## 3. Stand: Stufe 2 abgeschlossen, Stufe 3 noch nicht begonnen

Alles Folgende ist gebaut und geprüft (cargo build/test/clippy/fmt grün,
Stufe 1 zusätzlich mit Meson-Tests und Flatpak-Build; Stufe 2 wurde über
`build-aux/sdk-cargo.sh` in der GNOME-SDK-Sandbox geprüft, siehe unten):

- **Gerüst:** `Cargo.toml`, `build.rs` (Blueprint → OUT_DIR, GResource,
  glib-compile-schemas, Export von APP_ID/VERSION/PROFILE/LOCALEDIR/
  PKGDATADIR/GSCHEMA_DIR als rustc-env), `meson.build` + `src/meson.build`
  + `data/meson.build` + `po/meson.build`, `meson_options.txt` (profile),
  Flatpak-Manifest `build-aux/org.gnomelex.Gesetze.Devel.json`
  (org.gnome.Platform//51, rust-stable, `--share=network` im Build),
  Desktop-/Metainfo-Datei, GSettings-Schema, Icons, `data/style.css`,
  `po/` mit `gesetze.pot`, `de.po`, `LINGUAS`, `POTFILES`.
- **Rust-Module (fertig, getestet, per `#![allow(dead_code)]` bis zur
  UI-Anbindung stillgelegt):**
  - `src/model/text.rs`: Block-Modell (Paragraph, List, Table, Pre,
    Heading; Span mit Style und Fußnoten-ID) plus `flatten_blocks()`, das
    Blöcke in Segmente mit Tags linearisiert. **Wichtig:** Importer (für
    Absatztexte in der DB) und spätere Normansicht (für den TextBuffer)
    müssen dieselbe Funktion nutzen, damit Annotations-Offsets übereinstimmen.
    Tabellen werden im Text durch U+FFFC vertreten.
  - `src/model/mod.rs`: LawInfo, UnitInfo, NormInfo, Norm, Annotation
    (+Kind, LinkTarget), HIGHLIGHT_COLORS. `src/model/objects.rs`:
    GObjects OutlineItem, SearchResultObject, AnnotationObject (je ein
    eigenes `mod imp_*`, weil mehrere `glib::Properties` in einem Modul
    kollidieren).
  - `src/importer/xml.rs`: eigener DOM über quick-xml 0.42 (Text und
    `Event::GeneralRef` für Entities getrennt behandelt), `parse_meta()`,
    `parse_law()` → ParsedLaw {meta, units (Baum über Kennzahl-Präfix),
    norms}. Gliederungsnormen (doknr …BJNG…) tragen die Einheit, §-Normen
    folgen in Dokumentreihenfolge. Tests mit Inline-XML.
  - `src/importer/mod.rs` (Stufe 2 fertig): `Source`/`SOURCES`/`DEFAULT_SLUG`
    (bgb), ImportError (+Network/UnknownSource/Cancelled), `extract_xml()`
    (zip), `import_xml(db_path, slug, xml)` (blockierend), `download(url,
    progress)` (soup3 `send_future` + `read_bytes_future` in 64-KiB-Blöcken,
    User-Agent `gesetze/<version>`, 60 s Timeout), `install_law(db_path,
    slug, progress)` (Download im Hauptkontext, Entpacken/Parsen/DB in
    `gio::spawn_blocking`), `check_update(db_path, slug, progress)` →
    `UpdateCheck::{NotInstalled, UpToDate, Available}`, `installed_law()`,
    `version_differs(installed, remote)` (Stand-Vermerk, sonst Build-Datum).
    Fortschritt über `Progress::{Downloading{received,total}, Importing}`.
  - `src/db/mod.rs`: Schema (laws, units, norms mit Blöcken als JSON,
    paragraphs, norms_fts FTS5 unicode61, annotations), `replace_law()`,
    Gliederungs-/Norm-Abfragen, `neighbor_norm()`, `search()` mit
    `build_fts_query()` (Präfixterme, Phrasen), Annotations-CRUD, JSON-
    Export/Import, `reanchor_law()` (unverändert/verschoben/verwaist/
    wiederhergestellt anhand Wortlaut). DB-Pfad:
    `glib::user_data_dir()/gesetze/gesetze.db`.
  - `src/refs/mod.rs`: `find_references(text, current_law)` → Reference
    {start, end (Zeichenoffsets), target: NormRef {law: Same|Abbrev|Unknown,
    norm, sub_section, paragraph, sentence, number}}; erkennt §, §§ mit
    Aufzählungen/Bereichen, Qualifier (Abs./Satz/Nr./Halbsatz/Buchstabe),
    Artikel mit innerem §, Gesetzesabkürzungen, alleinstehende „Absatz n“.
  - `src/settings.rs`: GSettings mit Fallback-Suche (GSETTINGS_SCHEMA_DIR,
    Build-OUT_DIR, `<exe>/../share/glib-2.0/schemas`).
  - `src/application.rs` (Aktionen quit/about, CSS laden, AboutDialog),
    `src/window.rs` (Stufe 2): Fenstergeometrie an GSettings gebunden;
    `GtkStack` mit drei Seiten „empty“ (StatusPage mit Schaltfläche „BGB
    herunterladen“), „busy“ (StatusPage mit `AdwSpinnerPaintable`,
    Fortschritt in Prozent und Bytes) und „ready“ (Titel, Stand,
    Neufassung, Fundstelle, Dokumentdatum, Importzeitpunkt). Fensteraktionen
    `win.download` (Erstimport und Aktualisierung, gleicher Ablauf) und
    `win.check-updates`; beide im Hauptmenü, während laufender Vorgänge
    deaktiviert. Beim Start: installierte Fassung per `spawn_blocking`
    lesen; wenn `check-updates` aktiv und `last-update-check` älter als
    24 h, stille Prüfung (Fehler nur ins Log). Neue Fassung → Toast mit
    Schaltfläche „Aktualisieren“ (Aktion `win.download`); nach Import Toast
    mit Normenzahl und ggf. Reanchor-Bericht (verschoben/verwaist/
    wiedergefunden). `last-update-check` wird als RFC 3339 gespeichert.
    Hilfsfunktionen `format_builddate`, `update_check_due` mit Tests.
    `data/ui/window.blp` entsprechend. Übrige `.blp` unter `data/ui/` sind
    Stubs (nur `using`-Zeilen), bereits in GResource/Meson/POTFILES
    eingetragen: law_tab, norm_view, outline_row, annotation_row,
    search_row, preferences, shortcuts.
- **Tooling:** `.vscode/` (settings, tasks, launch, extensions; Tasks
  `sdk: cargo build|run|qualität` für die SDK-Sandbox),
  `build-aux/sdk-cargo.sh` (führt Cargo in `org.gnome.Sdk//51` mit
  rust-stable aus, Ziel `target-sdk/`, in `.gitignore`; nötig, wenn der Host
  keine gtk4-/libadwaita-Entwicklungspakete oder nur glib < 2.88 hat, wie
  Fedora 43), `build-aux/flatpak-run.sh` (startet Sandbox per `flatpak build
  --with-appdir` auf `_flatpak`, reicht Display-/D-Bus-Variablen durch,
  fasst Argumente zu einer `sh -c`-Kommandozeile zusammen, weil cppdbg
  „gdb --interpreter=mi“ als ein Argument übergibt), Copilot-Konfiguration
  (`.github/copilot-instructions.md`, `.github/instructions/`,
  `.github/agents/kleinaufgaben.agent.md`, lokale `.github/prompts/`).
- **Icons:** vom Auftraggeber geliefertes Logo (blaues Quadrat, weißes §)
  als `org.gnomelex.Gesetze.svg`; Symbolic-Icon nutzt denselben Pfad.

### Bekannte Entscheidungen und Fallstricke

- Crates: gtk4 0.11 (Feature `gnome_50`, `blueprint`), libadwaita 0.9
  (`v1_9`), glib/gio 0.22, soup3 0.9, rusqlite 0.40 bundled, quick-xml
  0.42, zip 8, gettext-rs 0.8 (`gettext-system`), regex, chrono, log/env_logger.
- `setlocale` aus gettext-rs ist `unsafe` (mit Begründung gekapselt).
- Die Sandbox unter `flatpak build` hat die rust-stable-Erweiterung nicht
  eingehängt; die gdb-Zeile für Rust-Pretty-Printer in `launch.json` ist
  daher mit `ignoreFailures` versehen.
- Wenn `libsoup3-devel` fehlt: pkg-config-Shim unter `~/.cache/gnome-lex/pc`
  (`libsoup-3.0.pc` + Symlink `libsoup-3.0.so` → `/usr/lib64/libsoup-3.0.so.0`),
  `PKG_CONFIG_PATH` darauf setzen. Auf einem System mit installiertem
  `libsoup3-devel` ist das überflüssig; `.vscode/settings.json` und
  `tasks.json` setzen die Variable trotzdem (harmlos).
- `flatpak-builder --install-deps-from=flathub` scheitert mit `--user`,
  wenn Flathub nur systemweit eingerichtet ist; ohne die Option bauen.
- `flatpak run` der SDK-Sandbox blieb auf dem Entwicklungsrechner gelegentlich
  vor dem Start hängen (Wartezeit auf eine Pipe; vermutlich Dokumenten-Portal
  und Autofs-Einhängungen unter `/mnt`). `build-aux/sdk-cargo.sh` setzt daher
  `--no-documents-portal --no-a11y-bus`; seitdem keine Hänger.
- Test des Imports ohne Klicken (Stufe 2 so geprüft): App aus
  `target-sdk/debug/gesetze` per `flatpak run … --own-name=org.gnomelex.Gesetze.Devel
  --env=XDG_DATA_HOME=<Verzeichnis unter $HOME> --env=GSETTINGS_BACKEND=keyfile
  --env=RUST_LOG=info org.gnome.Sdk//51` starten (ein Verzeichnis unter
  `/tmp` ist in der Sandbox nicht sichtbar!), dann Fensteraktionen über
  D-Bus auslösen: `gdbus call --session --dest org.gnomelex.Gesetze.Devel
  --object-path /org/gnomelex/Gesetze/Devel/window/1 --method
  org.gtk.Actions.Activate download "[]" "{}"` (ebenso `check-updates`,
  `DescribeAll` zeigt den Aktiviert-Zustand). Ergebnis: 2558 Normen,
  289 Einheiten, 5673 Absätze, DB ca. 7 MB, Download + Import ca. 3 s.

---

## 4. Nächster Schritt: Stufe 3 (erst nach Kommando)

Geplant: Gliederung als `GtkTreeListModel` über `OutlineItem` in einem
`GtkListView` (linke Seite des `AdwNavigationSplitView`), Normansicht mit
`GtkTextView` und TextTags (Überschriften, Absatznummern, Fußnoten),
Tabellen als `GtkGrid` an `GtkTextChildAnchor`, gemeinsamer Buffer pro
Norm; **`flatten_blocks()` aus `src/model/text.rs` für den Buffer nutzen**,
damit Annotations-Offsets zur DB passen. Die „ready“-Seite des Fensters
wird dann durch die eigentliche Ansicht ersetzt; `win.download` und
`win.check-updates` bleiben im Menü. Offene Punkte aus Stufe 2: ein
Einstellungsdialog (Stub `preferences.blp`) mit Schalter für
`check-updates` fehlt noch; die Prüfung lädt das ganze Archiv (~470 KB),
ein HEAD-Vergleich über ETag/Last-Modified wäre eine mögliche Optimierung.
