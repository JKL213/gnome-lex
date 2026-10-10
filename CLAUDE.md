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

## 3. Stand: Leseansicht ohne Tabs, Verweisindex, Annotationen im Kern (siehe unten)

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
- **Stufe 3 (Gliederung und Normanzeige), im Arbeitsbaum fertig, noch nicht
  committet:**
  - `src/widgets/outline.rs` (`Outline`): `GtkTreeListModel` über
    `OutlineItem` (Einheiten mit Kindern, §-Normen als Blätter), `set_data()`,
    `reveal_norm()` (klappt Pfad auf, liefert Position), `item_at()`,
    `SingleSelection`. `src/widgets/outline_row.rs` (`LexOutlineRow`,
    `outline_row.blp`): `TreeExpander` + Bezeichner/Titel.
  - `src/widgets/norm_view.rs` (`LexNormView`, `norm_view.blp`): `Adw.Bin`
    mit `ScrolledWindow` → `Adw.ClampScrollable` → `GtkTextView`; eigene
    `TextTagTable` (Überschriften h1–h3, Absatznummern fett, Listenzeilen mit
    Einzug je Tiefe, Listenbezeichner, pre, footnote, footnote-ref in
    Akzentfarbe, bold/italic/underline/sup/sub/small); Text kommt über
    `flatten_blocks()` (dieselben Offsets wie in der DB), Tabellen als
    `GtkGrid` an `GtkTextChildAnchor` (`build_table`), Fußnoten unten mit
    Marken (`footnote_mark_name`), Fußnotenverweise als Tag. `set_norm()`,
    `norm_id()`, `content_start()`, `set_font_size()` (CSS-Provider).
  - `src/window.rs`: Stack-Seite „law“ mit `AdwNavigationSplitView`
    (Sidebar: Gliederungs-`ListView`; Content: HeaderBar mit vor/zurück und
    `Adw.WindowTitle`, darunter `LexNormView`), `AdwBreakpoint` klappt die
    Sidebar ein. Fensteraktionen `download`, `check-updates`, `prev-norm`,
    `next-norm` (`neighbor_norm()`), `zoom-in/-out/-reset` (GSettings
    `font-size`), `show-norm(s)` (Bezeichnung wie „§ 433“). Laden per
    `spawn_blocking` mit `load_serial` gegen veraltete Antworten; nach
    Import bleibt die aktuelle Norm über ihre Bezeichnung geöffnet.
    Kürzel in `src/application.rs` (`set_accels_for_action`): Strg+Q, Strg+W,
    Alt+Bild↑/↓, Strg+Plus/Minus/0. App-Aktion `app.show-norm(s)` reicht an
    das aktive Fenster durch (für Tests per D-Bus, siehe Fallstricke).
  - Geprüft in der SDK-Sandbox (fmt, clippy `-D warnings`, 44 Tests) und
    per Broadway-Screenshots (§ 14 mit Fußnote, § 55a mit Liste, § 187).
- **Importer-Korrektur (29.09.2026):** Text hinter einer im `<P>`
  eingebetteten Liste/Tabelle/`<pre>` wurde bisher an den Einleitungssatz
  gehängt (§ 55a Abs. 1 zeigte den Nachsatz vor der Aufzählung).
  `collect_paragraph` schließt jetzt vor jedem Blockelement die
  gesammelten Spans als eigenen Absatzblock ab (`flush_spans`), Reihenfolge
  Einleitung – Liste – Nachsatz bleibt erhalten; der Nachsatz wird ein
  eigener DB-Absatz ohne Nummer. Test `keeps_text_after_embedded_list_in_order`
  (Inline-XML) und ignorierter Realdaten-Test `imports_real_bgb_keeps_list_order`
  (`GESETZE_BGB_XML=<Pfad zur BJNR001950896.xml> build-aux/sdk-cargo.sh
  test -- --ignored`, Variable exportieren; Datei liegt lokal unter
  `~/.cache/gnome-lex/bgb.xml`). **Bestehende Datenbanken müssen neu
  importiert werden** (`win.download`), sonst bleibt die alte Reihenfolge.
- **Prüfung am 07.10.2026 (Leseansicht lauffähig):** Build, fmt, clippy
  `-D warnings` und 45 Tests grün; Erstimport (2558 Normen) und Anzeige von
  § 14, § 55a, § 433, „Titel 1“ sowie Schmalbreite per Broadway bestätigt
  (Screenshots unter `~/.cache/gnome-lex/shots/`). Dabei behoben:
  - Importer: Gliederungsnormen mit eigenem Text (fünf Titel/Untertitel im
    BGB mit „Amtlichem Hinweis“ als Fußnote) erschienen in der Gliederung
    als Blatt mit Roh-Doknr. Sie erhalten jetzt Bezeichnung/Titel der
    Einheit (`unit_labels` in `parse_law`, Test
    `unit_norm_with_footnote_gets_unit_labels`). DB wurde neu importiert.
  - Fenster: `show_norm_by_enbez` wechselt bei eingeklappter Seitenleiste
    jetzt wie der Gliederungsklick zur Inhaltsseite (`reveal_content`).
  - Test-Hook `app.window-action(s)`: löst eine parameterlose Fensteraktion
    im aktiven Fenster aus, z. B. Erstimport ohne Klicken:
    `gdbus call --session --dest org.gnomelex.Gesetze.Devel --object-path
    /org/gnomelex/Gesetze/Devel --method org.gtk.Actions.Activate
    window-action "[<'download'>]" "{}"`.
  - `build-aux/broadway-run.sh`: startet `gtk4-broadwayd :7` und die App aus
    `target-sdk/debug/` in der SDK-Sandbox mit Testdaten unter
    `~/.cache/gnome-lex/testdata` (XDG_DATA_HOME, XDG_CONFIG_HOME mit
    Keyfile-GSettings, `--own-name`). Mit `nohup … &` starten; Screenshots:
    `chromium-browser --headless --no-sandbox --screenshot=x.png
    --window-size=1200,800 --virtual-time-budget=8000 http://127.0.0.1:8087/`.
    Achtung: `pkill -f` mit dem Programmpfad trifft auch die eigene Shell;
    `pkill -x gesetze; pkill -x gtk4-broadwayd` verwenden.
- **Schriften und Feinschliff (07.10.2026, Auftrag):** `data/style.css`
  setzt die Oberfläche auf Inter (Obsidian-Standardschrift; Fallback
  „Inter Variable“, Adwaita Sans, Cantarell) und den Lesetext (`.norm-text`
  samt Tabellen-Labels) auf „Anthropic Sans“ mit Fallback auf Inter. Beide
  Schriften werden nicht mitgeliefert: Inter (OFL) liegt zum Testen unter
  `~/.local/share/fonts/inter/`, Anthropic Sans ist proprietär und war auf
  dem Entwicklungsrechner nicht installiert (Ausnahme von Regel 3 auf
  ausdrücklichen Wunsch). Außerdem: Lesespalte enger (Clamp 780/620),
  mehr Zeilen-/Absatzabstand (`pixels-inside-wrap`), Blättern-Schaltflächen
  als `linked`-Box, Hauptmenü auf der Inhaltsseite nur bei eingeklappter
  Seitenleiste, Gliederungszeilen mit Tabellenziffern und Mindestbreite
  für Bezeichner (`.norm-label`).
- **Stufe 4 (07.10.2026, auf Kommando) – Tabs, geteilte Ansicht, mehrere
  Gesetze (ZPO), Schnellsuche.** Geprüft: fmt, clippy `-D warnings`,
  49 Tests, Broadway-Screenshots (`~/.cache/gnome-lex/shots/s*.png`,
  `n*.png`, `d*.png`): zwei Tabs, geteilte Ansicht BGB § 433 / ZPO § 253,
  Schmalbreite (Teilung untereinander), Schnellsuche als Dialog bzw.
  Bottom Sheet, Wiederherstellung der Tabs nach Neustart.
  - `src/widgets/law_tab.rs` + `law_tab.blp` (`LexLawTab`, `Adw.Bin` mit
    `GtkPaned`, zwei `LexNormView` mit Beschriftung `.pane-caption`):
    Eigenschaften `title`, `subtitle`, `split`, `active-pane`; Klick in
    eine Ansicht macht sie aktiv (Capture-Geste); `set_split()` kopiert
    beim Einschalten die Norm in die zweite Ansicht; `show_norm_in_pane()`
    lädt Norm + Gesetz per `spawn_blocking` (Serial je Ansicht);
    `set_vertical()` für Schmalbreite; `state()` → `TabState {panes:
    [{law, norm}], active}` (serde, JSON in GSettings `open-tabs`);
    `panes_showing(slug)` zum Nachladen nach Import; Callback
    `connect_norm_changed`. Jede Ansicht kann eine Norm eines beliebigen
    Gesetzes zeigen; Tab-Titel „§ 433 BGB“, Untertitel = Normtitel.
  - `src/window.rs` neu: `laws: Vec<LawInfo>` statt eines Gesetzes;
    `GtkDropDown law_dropdown` in der Sidebar-Kopfleiste wählt das Gesetz
    der Gliederung (`outline_slug`/`outline_law`, `sync_outline()` folgt
    der aktiven Ansicht und wechselt das Gesetz, `pending_reveal`,
    `outline_serial`). Inhaltsseite: `Adw.TabOverview` → `Adw.ToolbarView`
    mit HeaderBar (vor/zurück, Suche, `Adw.WindowTitle`, neuer Tab,
    Teilung als `ToggleButton` an `win.split`, `Adw.TabButton`), `Adw.TabBar`
    (autohide), `Adw.TabView`. Aktionen: `download(s)` (Slug), `check-updates`
    (alle installierten Gesetze nacheinander, Toast je neuer Fassung mit
    Ziel-Slug), `prev-/next-norm`, `zoom-*`, `show-norm(s)` (nutzt die
    Schnellsuche: „§ 433“, „253 zpo“), `new-tab`, `close-tab`, `next-/prev-tab`,
    `tab-overview`, `split` (stateful), `switch-pane`, `open-in-new-tab(x)`,
    `quick-search`, `quick-search-query(s)` (nur Tests). Kürzel in
    `application.rs`: Strg+T/W, Strg+Bild↑/↓ bzw. Strg+Tab, Strg+Umschalt+O,
    Strg+Umschalt+D (Teilung), F6 (Ansicht wechseln), Strg+K; „/“ über
    `ShortcutController` (Capture) im Fenster, nur wenn kein Eingabefeld
    den Fokus hat. Persistenz: `save_tabs()` bei `close-request` und
    `app.quit`, `restore_tabs()` beim Start (unbekannte Bezeichnungen
    überspringen, sonst erste Norm). Nach Import: `reload_after_import()`
    lädt Gliederung und betroffene Ansichten über die Bezeichnung nach
    (Norm-IDs ändern sich beim Import!). Breakpoint blendet bei
    Schmalbreite Such-, Teilungs- und Neuer-Tab-Schaltfläche aus und stellt
    Tabs auf vertikale Teilung. Erststart-Seite: eine Schaltfläche je
    Quelle (`setup_download_buttons`).
  - `src/widgets/outline_row.rs`: Mittelklick oder Strg+Klick auf eine
    Norm → `win.open-in-new-tab`.
  - **ZPO:** zweite Quelle in `SOURCES` (`zpo`, Zivilprozessordnung,
    `https://www.gesetze-im-internet.de/zpo/xml.zip`; 1095 Normen, 108
    Einheiten). Menüeinträge „BGB/ZPO herunterladen oder aktualisieren“ mit
    `target`. `DEFAULT_SLUG` nur noch für Tests, `installed_law()` entfernt.
  - **Schnellsuche:** `Database::quick_search(input, preferred_law, limit)`
    + `parse_quick_query()` in `db/mod.rs` (Kürzel, „§“/„Art.“, Nummer mit
    Buchstabe, Stichwörter → Bezeichnungs-Präfix-Suche mit exaktem Treffer
    zuerst, sonst FTS mit Titelgewicht, sonst erste Norm des Gesetzes;
    Tests `quick_query_parsing`, `quick_search_across_laws`).
    `src/widgets/quick_search.rs` + `quick_search.blp` (`LexQuickSearch`,
    `Adw.Dialog` mit `SearchEntry` als Titel-Widget, `ListBox` mit
    `Adw.ActionRow`s, Pfeiltasten/Strg+Eingabe per Capture-Key-Controller,
    `connect_jump(norm_id, new_tab)`).
  - Test-Hook `app.window-action(s)` nimmt jetzt ein Argument: `"download
    zpo"`, `"split"`, `"new-tab"`, `"quick-search-query 55a"`.
  - **Broadway-Screenshots:** ohne verbundenen Browser zeichnet GTK nicht
    neu; deshalb jede Aufnahme zweimal hintereinander machen (die zweite
    zeigt den aktuellen Stand), siehe `shot()`-Schleife in dieser Sitzung.
  - Nicht umgesetzt aus dem Plan: gemeinsamer `GtkTextBuffer` je Norm über
    Tabs (jede Ansicht hat eigenen Buffer; für Annotationen später prüfen),
    Scrollposition in `open-tabs`. `po/gesetze.pot` wurde nicht
    nachgezogen (neue Strings in window.blp, quick_search.blp, window.rs,
    law_tab.rs, quick_search.rs).
- **Nachbesserungen nach Rückmeldung (07.10.2026):** fmt, clippy
  `-D warnings`, 52 Tests grün; Broadway-Screenshots `g*.png`, `h*.png`.
  - **Hänger bei „Inhaltsübersicht“ (ZPO):** Die Übersicht ist dort eine
    Tabelle mit 1423 Zeilen, die als `GtkGrid` mit ~2800 umbrechenden
    Labels gebaut wurde (CPU-Last, minutenlanger Stillstand). Importer:
    `toc_blocks` und – für Inhaltsübersichten im `<Content>` – `tables_to_lines`
    wandeln solche Tabellen in Zeilen (Gliederungszeilen als Überschrift,
    `is_unit_label`). Ansicht: Tabellen mit mehr als `MAX_GRID_ROWS` (150)
    Zeilen werden als ein Label (`table_as_lines`, `.norm-table-plain`)
    gesetzt. Tests `toc_table_becomes_lines`. **Betroffene Gesetze neu
    importieren** (ZPO wurde in der Testdaten-DB neu importiert).
  - **Weggefallene Normen:** `strip_repealed_marker` entfernt „(XXXX)“ aus
    der Bezeichnung („§§ 15 bis 20“); Gliederungszeilen mit Titel
    „(weggefallen)“ sind abgeblendet (`.outline-row.repealed`). Test
    `repealed_marker_is_stripped`. Erfordert Neuimport.
  - **Download-Center** (`src/widgets/download_center.rs`,
    `download_center.blp`, `LexDownloadCenter`, `Adw.Dialog` mit
    `PreferencesPage`): Gruppen „Installiert“/„Verfügbar“, je Quelle eine
    `ActionRow` mit Stand/Importdatum und Schaltfläche an `win.download`
    (Slug als Ziel); Kopfleiste mit `win.check-updates`. `refresh(laws)`
    wird vom Fenster nach Importen gerufen (`download_center: WeakRef`).
    Aktion `win.download-center` im Hauptmenü („Gesetze herunterladen …“)
    und auf der Erststart-Seite („Weitere Gesetze …“); die einzelnen
    Menüeinträge je Gesetz sind entfallen. `Source` hat jetzt `abbrev`,
    die URL kommt aus `Source::url()`; `SOURCES` umfasst 33 Gesetze mit
    geprüften Slugs (u. a. `bgbeg`, `owig_1968`, `gkg_2004`, `vvg_2008`,
    `ao_1977`, `ustg_1980`, `sgb_5`).
  - **Favoriten:** Lesezeichen als Annotation `kind = bookmark` (law =
    Slug, norm = Bezeichnung, note = Normtitel, quote leer → bleibt bei
    Updates an der Norm verankert). Stern-`ToggleButton` in der Kopfleiste
    und Menüeintrag an `win.toggle-favorite` (stateful, Strg+D). Sidebar
    ist jetzt ein `Adw.ViewStack` („Gliederung“/„Favoriten“) mit
    `Adw.ViewSwitcherBar` unten; Favoritenliste als `ListBox` mit
    `ActionRow`s (Titel „§ 433 BGB“, Untertitel Normtitel, Papierkorb zum
    Entfernen, Klick öffnet im aktiven Tab; verwaiste Einträge abgeblendet).
    Fenster: `favorites`, `load_favorites`, `set_favorites`,
    `toggle_favorite`, `remove_favorite`, `open_favorite`, `favorite_id`.
  - **Fortlaufendes Lesen (wie gesetze.io):** `LexNormView` verwaltet
    Abschnitte (`Section {norm, start: TextMark, content_offset}`) in einem
    Puffer. Beim Scrollen nahe ans Ende (`PRELOAD_PAGES` = 1,5 Seiten)
    ruft `check_edges` den Callback `connect_need_more(direction, edge_id,
    generation)`; am oberen Rand löst ein Scrollrad nach oben
    (`EventControllerScroll`) `request_previous` aus. `append_norm`/
    `prepend_norm`/`set_end_reached` tragen die Generation (jedes
    `set_norm` erhöht sie, alte Antworten werden verworfen). Beim
    Voranstellen: GTK hält den sichtbaren Text an Ort und Stelle; Tags
    werden über `pending_tags` neu angewendet (vor einem Tag-Anfang
    eingefügter Text erbt sonst die Tags), Verweis-Offsets verschoben und
    die Startmarke des bisherigen ersten Abschnitts hinter den neuen Text
    bewegt (Linksgravitation!). `update_visible_norm` bestimmt die Norm
    am oberen Rand über `line_at_y` + Offsets und meldet sie per
    `connect_visible_changed`; `LexLawTab` (`setup_continuous_reading`,
    `load_neighbor`, `on_visible_norm_changed`) aktualisiert Ansichtsinfo,
    Titel, Beschriftung und löst `norm_changed` aus, sodass Kopfleiste und
    Gliederung folgen. Fußnotenmarken heißen `footnote:<norm_id>:<id>`.
  - Test-Hooks: `window-action "scroll 1"` (Ende), `"scroll 0"`,
    `"scroll -1"` (vorherige Norm anfordern), `"sidebar-page favorites"`.
    `build-aux/broadway-shot.sh <name>` nimmt bis zu drei Screenshots und
    behält den größten (Broadway liefert nach Neuverbindung manchmal ein
    leeres Bild). Broadway selbst stürzt gelegentlich mit
    `broadway-output.c: append_node_ops: assertion failed` ab (Backend-Bug
    bei Browser-Neuverbindungen, nicht die App) – dann einfach neu starten.
- **Zweite Rückmeldungsrunde (07.10.2026):** fmt, clippy `-D warnings`,
  54 Tests grün; Screenshots `i*.png`, `j*.png`, `k*.png`.
  - **Symbole:** Auf KDE (Breeze-Theme ohne Adwaita-Fallback) fehlten die
    symbolischen Icons. 25 Adwaita-Icons liegen jetzt unter
    `data/icons/scalable/actions/*-symbolic.svg` und in der GResource;
    GTK findet sie automatisch über den Ressourcenpfad der App-ID. Neue
    Icons dort ablegen und in `data/gesetze.gresource.xml` eintragen
    (Quelle: `/usr/share/icons/Adwaita/symbolic/` im SDK).
  - **Gliederung:** Rahmennorm (ohne Bezeichnung) und „Inhaltsübersicht“
    werden ausgeblendet (`is_readable_norm` in `window.rs`); `neighbor_norm`
    und `quick_search` überspringen sie ebenfalls.
  - **Gliederungsüberschriften im Text:** `Database::unit_path(unit_id)`;
    die Ansicht bekommt je Abschnitt `NormPage {norm, units, law_abbrev,
    note}` und setzt die Pfadzeilen (Tag `unit-heading`) über die
    Überschrift, beim Nachladen nur die gegenüber dem Nachbarn neuen Ebenen
    (`Section::unit_ids`).
  - **Klickbare Verweise (Kern von Stufe 6):** `mark_references` lässt
    `refs::find_references` über den Normtext laufen, markiert Treffer mit
    Tag `reference` (Akzentfarbe) und legt `LinkTarget::Reference(NormRef)`
    an. Klick → `connect_navigate` (Strg+Klick = neuer Tab) → Tab →
    `LexWindow::navigate_reference`: Gesetz über Slug (Same) oder
    `law_by_abbrev`, Bezeichnung über `NormRef::enbez_candidates`; nicht
    installiertes Gesetz → Toast mit Download-Schaltfläche
    (`offer_law_download`). Noch ohne Hover-Vorschau.
  - **Schema / Notiz je Norm:** Annotation `kind = note`, Absatz 0, leerer
    Wortlaut (`Database::norm_note`, `set_norm_note`). Rechtes Panel
    (`Adw.OverlaySplitView notes_split`, `TextView notes_view`) über
    Stern-Nachbar `win.toggle-notes` (Strg+Umschalt+N); speichert 800 ms
    nach der letzten Änderung und bei Normwechsel/Beenden (`flush_note`,
    synchroner DB-Zugriff). Die Notiz erscheint im Text zwischen
    Überschrift und Absatz 1 als Block (Tags `note-heading`, `user-note`,
    `note-block` mit Akzent-Tönung); `LexNormView::set_note` ersetzt den
    Block zwischen den Marken `note_start`/`note_end` und korrigiert
    `content_offset` und Verweis-Offsets.
  - **Verlauf:** `HistoryEntry {law, abbrev, norm, title}` (max. 60, keine
    Dubletten), `MenuButton history_button` mit Popover-Liste (neueste
    zuerst) und „Verlauf leeren“; persistiert in GSettings `history` (as,
    JSON) beim Beenden. Aufzeichnung in `record_history` bei jedem
    Normwechsel der aktiven Ansicht.
  - **GNOME-Dialoge:** `src/widgets/preferences.rs` + `preferences.blp`
    (`LexPreferences`, `Adw.PreferencesDialog`: Farbschema als `ComboRow`,
    Schriftgröße als `SpinRow` an `font-size`, `SwitchRow` an
    `check-updates`; `apply_color_scheme` setzt den `AdwStyleManager` beim
    Start und bei Änderung). `app.preferences` (Strg+,), `app.shortcuts`
    (Strg+?) mit `Adw.ShortcutsDialog` aus Code (`show_shortcuts`).
    Hauptmenü endet GNOME-typisch mit Einstellungen, Tastenkürzel, Info.
  - **Sichtbare Norm beim Scrollen:** nur noch bei `value-changed` und
    150 ms nach dem Nachladen ermittelt (über `line_at_y` + Offsets).
    Während `changed`-Signalen (GTK validiert Zeilen) lieferte `line_at_y`
    falsche Zeilen, was Verlauf und Notizziel verfälschte.
  - Test-Hooks: `window-action "set-note <Text>"` (`\n` für Zeilenumbruch),
    `"popup-history"`, App-Aktionen `preferences`, `shortcuts`.
- **Dritte Rückmeldungsrunde (07.10.2026) – Beginn von Stufe 7
  (Annotationen):** fmt, clippy `-D warnings`, 55 Tests grün; Screenshots
  `l*.png`, `m*.png`.
  - **Schrift:** GSettings `reading-font` (s, leer = Standardstapel
    `DEFAULT_READING_FONTS`: Public Sans → Source Sans 3 → Inter →
    Adwaita Sans). `apply_reading_font` (preferences.rs) hält einen eigenen
    `CssProvider` für `.norm-text`, `.norm-text label`, `.note-editor` und
    folgt der Einstellung. Einstellungsdialog: `FontDialogButton`
    (`level: family`) plus Zurücksetzen-Schaltfläche. Zum Testen liegen
    Public Sans (OFL) und Source Sans 3 (OFL) unter `~/.local/share/fonts/`;
    für das Flatpak müssten sie als Module gebündelt werden. „Anthropic
    Sans“ steht nicht mehr im Code (Regel 3), kann aber als eigene Schrift
    gewählt werden, wenn installiert.
  - **Textbild:** Listenebenen (`list0`–`list5`) kursiv; weggefallene Normen
    (`is_repealed` auf dem Titel) komplett mit Tag `repealed` (kursiv,
    grau); zwischen Normen eine Trennlinie (`insert_rule`: Abstandszeile
    `rule-gap`, winzige Zeile `norm-rule` mit Absatzhintergrund,
    Abstandszeile). `update_visible_norm` nimmt jetzt die Norm am oberen
    Rand (+32 px) statt bei 30 % der Seite.
  - **Markierungen (Highlights):** Textauswahl per Maus → `on_click_released`
    zeigt ein `GtkPopover` am Auswahlende (Capture der Geste nach Verweisen):
    fünf Farbfelder (`HIGHLIGHT_COLORS`), „Notiz anheften“, „Entfernen“
    (wenn die Auswahl eine Markierung berührt). Klick auf eine bestehende
    Markierung → dasselbe Popover zum Umfärben/Entfernen
    (`popover_target`). `annotation_from_selection` rechnet Pufferoffsets
    in Absatz (`Section::block_starts`, = DB-Tabelle `paragraphs`, idx =
    Block + 1) und Offsets im Absatz um, begrenzt auf den Startabsatz, und
    speichert den Wortlaut (`quote`). Tags `hl-<farbe>` (hell/dunkel über
    `set_highlight_colors`, folgt `StyleManager::dark`).
  - **Angeheftete Notizen:** Annotation `kind = note` mit Absatz ≥ 1 und
    Wortlaut; Darstellung als farbige Unterstreichung + Tönung (Tag
    `note-<farbe>`) und einem Marker (`gtk::Button.note-marker.<farbe>`)
    als Overlay am Ende des Wortlauts (`add_overlay`/`move_overlay`,
    Pufferkoordinaten; `reposition_markers` nach `changed`, Schriftgröße
    und Nachladen). Klick auf den Marker → Popover (`open_note_editor`) mit
    Wortlaut, Farbfeldern, `TextView` (`.note-editor`), „Löschen“/„Fertig“;
    beim Schließen `AnnotationEvent::Update`/`Delete`. Neue Notiz öffnet den
    Editor 200 ms nach dem Anlegen. **Overlay-Kinder hängen an einem
    internen `GtkTextViewChild`; `gtk_text_view_remove` kennt sie nicht →
    `remove_marker` versteckt sie nur** (kleines Leck je entfernter Notiz).
  - **Datenfluss:** `NormPage.annotations` (aus `annotations_for_norm`,
    Absatz > 0, nicht verwaist) → `place_annotation` beim Rendern; die
    Ansicht meldet `connect_annotation(AnnotationEvent)` → Tab ergänzt den
    Slug (`connect_annotation`) → `LexWindow::on_annotation_event` schreibt
    per `insert_/update_/delete_annotation` und spiegelt per
    `tab.apply_annotation(law_id, &a)` / `remove_annotation(id)` in alle
    Tabs und beide Ansichten. Reanchoring nach Updates läuft über den
    vorhandenen `reanchor_law` (Wortlaut).
  - **Testumgebung:** Eine parallel laufende Entwicklungsinstanz (VS-Code-
    Debugger, `/app/bin/gesetze` in `ptrace_stop`) hielt den D-Bus-Namen,
    sodass die Testinstanz nicht registrieren konnte. `build-aux/broadway-run.sh`
    nutzt deshalb einen eigenen Bus: vorher
    `dbus-daemon --session --address=unix:path=$HOME/.cache/gnome-lex/bus.sock
    --nofork --nopidfile &` starten, die App mit `--no-session-bus` und
    `DBUS_SESSION_BUS_ADDRESS` auf diesen Socket; `gdbus`-Aufrufe vom Host
    mit derselben Variable. Test-Hooks: `window-action "view-test select
    230 300"`, `"view-test hl green"`, `"view-test note"`.
  - Offen aus Stufe 7: Annotationsliste in der Seitenleiste, JSON-Export/
    -Import in der Oberfläche, Markieren per Tastatur, Hover-Vorschau.
- **Vierte Rückmeldungsrunde (07.10.2026):** fmt, clippy `-D warnings`,
  55 Tests grün; Screenshots `n*.png`.
  - **Schriftwechsel wirkte nicht:** `settings::settings()` erzeugte je
    Aufruf eine neue `gio::Settings`-Instanz; `connect_changed`-Handler
    (Lesefont, Farbschema) starben mit der kurzlebigen Kopie aus
    `startup`. Jetzt eine Instanz pro Thread (`thread_local` + `OnceCell`).
  - **Hänger beim Schließen:** headless nicht reproduzierbar (Broadway hat
    ohne Browser keinen Frame-Takt). Hauptverdacht waren die Overlay-Marker
    (`move_overlay` in `changed`-Handlern → Neulayout-Schleife); sie sind
    mit dem Umbau der Notizen entfallen. Test-Hook `window-action
    close-window` (ruft `win.close()`); Schließen mit Notizen und offenem
    Panel beendet die Testinstanz sauber.
  - **Notizen jetzt inline:** Eine angeheftete Notiz ist ein bearbeitbarer
    Block („✎ Text⏎“, Tags `note-glyph` nicht editierbar, `note-body` +
    `inline-note-<farbe>` editierbar mit Absatzhintergrund) direkt hinter
    dem Absatz, auf den sie zeigt; der Wortlaut bleibt farbig unterstrichen.
    Die `GtkTextView` bleibt `editable=false`, nur die Blöcke sind über
    Tag-`editable` beschreibbar; der Cursor ist nur dort sichtbar
    (`update_cursor_visibility`). Tippen → `buffer.changed` →
    `on_buffer_changed` (700 ms) → `flush_note_edits` → `Update`; `insert-text`/
    `delete-range` verschieben Verweis-Offsets (`shift_links`), während die
    Ansicht selbst schreibt, ist `rendering` gesetzt. Blöcke pro Absatz
    reihen sich hintereinander (`place_annotation` sucht das letzte
    `block_end` derselben Norm/Absatz). Positionen laufen über Marken:
    `Section::block_marks` (Rechtsgravitation je Blockanfang),
    `block_lens`, `content_end` (Linksgravitation, vor den Fußnoten).
    Auswahl-Popover: zwei Reihen Farbfelder („Markieren“/„Notiz“) und
    „Entfernen“; Klick auf das ✎-Symbol eines Blocks → Farbe/Löschen.
    `apply_annotation` lässt einen Block stehen, wenn Text und Farbe schon
    übereinstimmen (sonst würde die eigene Eingabe neu gesetzt).
    `save_state_before_quit` ruft `tab.flush_note_edits()`.
  - Test-Hooks: `view-test select a b`, `view-test hl <farbe>`,
    `view-test note`, `view-test type <Text>` (fügt am Cursor ein).
- **Fünfte Rückmeldungsrunde (07.10.2026):** fmt, clippy `-D warnings`,
  55 Tests grün; Screenshots `o*.png`.
  - **Normtext verschwand beim Speichern der Schema-Notiz:** `note_end`
    wurde mit Rechtsgravitation *vor* dem Einfügen des Normtexts angelegt
    und wanderte hinter den gesamten Text; `set_note` löschte dann alles
    dazwischen. Jetzt wird die Marke nach `insert_note` mit
    Linksgravitation gesetzt. Gleiche Korrektur für `Anchored::end` und
    `block_end` (sonst wuchsen Markierungen bzw. Notizblöcke, wenn direkt
    dahinter eingefügt wurde). **Regel:** Endmarken immer Linksgravitation
    und erst nach dem Einfügen anlegen; Anfangsmarken Linksgravitation vor
    dem Einfügen; Einfügemarken Rechtsgravitation.
  - **Absturz beim Schließen über die Fensterschaltfläche:** Popovers, die
    per `set_parent` am TextView hängen (Auswahl-Popover, Kontextmenü),
    müssen in `ObjectImpl::dispose` abgehängt werden (`unparent`), sonst
    „finalized with children left“ (Critical → unter dem Debugger Abbruch).
    Zusätzlich `popdown` in `WidgetImpl::unroot`.
  - **Kontextmenü (Rechtsklick):** `GestureClick` (Taste 3, Capture, Claimed)
    unterdrückt das GTK-Standardmenü; `show_context_menu` ermittelt
    `MenuContext {highlight, note, link, has_selection}`, wählt ohne Auswahl
    das Wort unter dem Zeiger und baut per `build_context_menu` ein
    `gio::Menu`: Verweis öffnen / in neuem Tab, Notizfarbe / Notiz löschen,
    Markierungsfarbe / Markierung entfernen, Markieren (Farben), Notiz
    hinzufügen (Farben), Kopieren, Auswahl in der Schnellsuche, Favorit,
    Schema-Panel, Norm in neuem Tab. Aktionen in der Gruppe `lex` am
    TextView (`highlight(s)`, `note(s)`, `recolor(s)`, `remove`, `copy`,
    `search-selection`, `open-link`, `open-link-tab`); `win.*`-Aktionen
    werden durchgereicht. `PopoverMenu` mit `NESTED`-Untermenüs. Unter
    Broadway ist das Menü nicht sichtbar (`gdk_monitor_get_geometry`
    fehlt), die Aktionen sind per `view-test action lex.note green` geprüft.
  - Test-Hooks: `view-test menu <x> <y>`, `view-test action <aktion> [param]`.
- **Tippen in Inline-Notizen (07.10.2026):** Bei `editable=false` reicht
  der `GtkTextView` Tastendrücke nicht an die Eingabemethode weiter, Tag-
  Editierbarkeit greift dann nie. Deshalb ist der TextView jetzt
  `editable: true`; das Tag `base` (unterste Priorität, über jedem
  Abschnitt) hat `editable = false` und sperrt den Normtext, die später
  angelegten Notiz-Tags geben ihre Blöcke frei. Interaktives Einfügen in
  Notizblöcken angenommen, im Normtext abgelehnt (Hooks `view-test itype
  <Text>`, `view-test cursor <offset>`).
- **Umbau 08.10.2026 (Auftrag): Tabs entfernt, Verweis-Pfeile, Aufräumen.**
  fmt, clippy `-D warnings`, 56 Tests grün; Screenshots `q*.png`.
  - **Tabsystem entfernt:** `AdwTabView`/`TabBar`/`TabOverview` und die
    Aktionen `new-tab`, `close-tab`, `next-/prev-tab`, `tab-overview`,
    `open-in-new-tab` sind weg. Die Inhaltsseite ist ein `Adw.ToolbarView`
    mit Kopfleiste und genau einer Leseansicht. `LexLawTab` heißt jetzt
    **`LexReader`** (`src/widgets/reader.rs`, `data/ui/reader.blp`,
    `ReaderState`); das Fenster greift über `reader()` darauf zu. Alles,
    was früher „in neuem Tab“ öffnete (Strg+Klick in Gliederung und auf
    Verweise, Strg+Eingabe in der Schnellsuche, Kontextmenü), öffnet jetzt
    in der **zweiten Ansicht** (`LexReader::show_in_other_pane`, Aktion
    `win.open-in-other-pane(x)`; schaltet die Teilung ein). Strg+W schließt
    das Fenster (`window.close`). Persistenz: GSettings `reader-state` (s,
    JSON `ReaderState`) statt `open-tabs`/`active-tab`
    (`save_reader_state`/`restore_reader_state`).
  - **Verweis-Pfeile:** Tabelle `norm_refs (from_norm_id, law_abbrev,
    enbez)` wird in `replace_law` aus `find_references` über den Normtext
    gefüllt (Same → eigenes Kürzel; erster `enbez_candidates`-Eintrag;
    Dubletten je Norm entfernt; `ON DELETE CASCADE`). Abfragen
    `outgoing_refs(norm_id)` und `incoming_refs(abbrev, enbez, limit)`
    (lesbare Normen, eigenes Gesetz zuerst). `load_page` baut daraus
    `NormPage.outgoing`/`incoming` als `RefChip {label, target: NormRef}`;
    die Ansicht setzt unter die Überschrift je eine Zeile „→ § 434 · § 437“
    (wohin die Norm zeigt) und „← § 453 · § 475 …“ (wer auf sie zeigt),
    Tag `ref-chip` (klein, Akzent), klickbar wie Verweise im Text, maximal
    `MAX_REF_CHIPS` (12) plus „+n weitere“. Test `reference_index`.
    **Bestehende Datenbanken neu importieren**, sonst bleibt `norm_refs`
    leer (Testdaten-DB ist neu importiert).
  - **Aufräumen:** `#![allow(dead_code)]` aus `db`, `model`, `refs`
    entfernt; gelöscht: `Database::open_default`, `delete_law`, `norm_info`;
    `open_in_memory`, `norm_by_enbez`, `annotation` nur noch `#[cfg(test)]`;
    gezielt mit Begründung erlaubt: `search`/`SearchHit` (Stufe 5),
    `export_/import_annotations_json`, `AnnotationExport`,
    `insert_annotation_tx`, `reanchor_all` (JSON-Export/-Import der
    Oberfläche steht aus). Kontextmenü ohne „Norm in neuem Tab“,
    Kürzeldialog ohne Tab-Einträge.
- **Verweise und Tooltips (09.10.2026):** Die klickbare Fläche eines
  Verweises schließt jetzt das vorangestellte „§“/„§§“/„Art.“ ein
  (`sign_prefix_start`, Test `link_includes_sign`). Trefferprüfung über
  `char_offset_at` (`iter_at_position`, Zeichen unter dem Zeiger statt
  nächster Cursorposition, kein Treffer rechts vom Zeilenende); beim
  Überfahren wird der Verweis unterstrichen (Tag `link-hover`) und ein
  Tooltip zeigt Ziel und Strg+Klick-Hinweis. Weitere Tooltips: Farbfelder
  („Gelb markieren“/„Notiz in Gelb anheften“), Entfernen, vor/zurück mit
  Kürzel, Verlauf, Download-Schaltflächen. Abgeschnittene Oberkante der
  „7“ (Bildschirm mit Skalierung 1,25) unter Broadway nicht reproduzierbar.
- **Scrollfehler und Artefakte (09.10.2026):** Nur statisch geprüft (fmt,
  clippy `-D warnings` mit Stub-pkg-config, kein GTK im Container).
  - **Sprung beim Hochscrollen:** GTK hält die oberste Zeile über eine Marke
    mit Linksgravitation; am Pufferanfang blieb sie vor der vorangestellten
    Norm, die Ansicht sprang an deren Anfang und jeder weitere Radschritt
    lud die nächste. `prepend_norm` hält jetzt den bisherigen Anfang per
    `scroll_to_mark` oben, wenn die erste Zeile sichtbar war.
  - **Doppelt verschobene Verweise:** `render_section` (Voranstellen) und
    `set_note` setzten `rendering` nicht; die `insert-text`/`delete-range`-
    Handler verschoben die Verweis-Offsets zusätzlich zur eigenen
    Korrektur. Folge: Klicks, Tooltips und Hover trafen nach dem
    Hochscrollen oder Speichern der Schema-Notiz falsche Stellen. Beide
    setzen `rendering` jetzt (mit Wiederherstellung des Vorzustands).
  - **Hängende Unterstreichungen:** `link-hover` wurde über veraltete
    Offsets entfernt und blieb stehen. Entfernen jetzt über den ganzen
    Puffer; Pufferänderungen markieren den Zustand nur als veraltet
    (`invalidate_hover`, keine Tag-Änderung aus Signalhandlern heraus).
  - **Sprung zeigte die Folgenorm:** Direkt nach `set_norm` ist das Layout
    des neuen Puffers unvollständig (unvermessene Zeilen); `line_at_y`
    lieferte eine Zeile der nachgeladenen Folgenorm, Kopfleiste, Gliederung
    und Verlauf sprangen auf sie, und Neuvermessung/Nachladen verschoben
    die Position. `pin_to_top` hält jetzt bis zu `JUMP_PIN_MS` (800 ms) den
    Anfang oben und unterdrückt `update_visible_norm`; Scrollen, Klick
    (Capture an der `ScrolledWindow`) oder Taste beenden das sofort
    (`release_pin`). `update_visible_norm` und `prepend_norm` rechnen über
    `window_to_buffer_coords` statt über den Adjustment-Wert (der Rand der
    Textansicht wurde vorher mitgezählt).
- **Sprungleiste (10.10.2026, Auftrag):** Der Such-Knopf und der Dialog
  `LexQuickSearch` sind ersetzt durch `LexJumpBar` (`src/widgets/jump_bar.rs`,
  `data/ui/jump_bar.blp`) als `title-widget` der Inhaltskopfleiste: `Entry`
  in `Adw.Clamp` (560), Platzhalter „Springen … (Super+K)“ kursiv (CSS
  `entry.jump-entry > text > placeholder`), rechts im leeren Feld die
  aktuelle Norm (`set_context`, ersetzt `norm_title`). Darunter ein
  `GtkPopover` ohne Autohide (nimmt den Fokus nicht, am Bin geparentet,
  `present()` in `size_allocate`, `unparent` in `dispose`) mit Trefferliste
  und Vorschau (erste ~700 Zeichen aus `Database::paragraphs`, Cache je
  Norm). Inline-Vervollständigung: nach Weitertippen wird der beste Treffer
  ergänzt und der Rest markiert (`complete`, Schreibweisen „§ 433 BGB“,
  „433 BGB“, „BGB 433“, Titel); Tab übernimmt, Pfeile/Bild wählen,
  Eingabe springt, Strg+Eingabe zweite Ansicht, Escape leert bzw. gibt den
  Fokus an die Leseansicht zurück. Kürzel Super+K, Strg+K, „/“
  (`win.quick-search`), `quick-search-query` füllt die Leiste. Breakpoint
  setzt `jump_bar.compact` (ohne Vorschau und Normhinweis). Nur statisch
  geprüft (clippy `-D warnings` mit Stub-pkg-config, fmt, reine Funktionen
  separat getestet); Sichtprüfung unter GNOME steht aus.
- **Tooling:** `.vscode/` (settings, tasks, launch, extensions; Tasks
  `sdk: cargo build|run|qualität` für die SDK-Sandbox),
  `build-aux/sdk-cargo.sh` (führt Cargo in `org.gnome.Sdk//51` mit
  rust-stable aus, Ziel `target-sdk/`, in `.gitignore`; nötig, wenn der Host
  keine gtk4-/libadwaita-Entwicklungspakete oder nur glib < 2.88 hat, wie
  Fedora 43), `build-aux/sdk-rust-analyzer.sh` (startet rust-analyzer in
  derselben Sandbox; in `.vscode/settings.json` als
  `rust-analyzer.server.path` eingetragen, weil dem Host rust-src, clippy
  und passende GTK-Pakete fehlen), `build-aux/flatpak-run.sh` (startet Sandbox per `flatpak build
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
- **Seit Stufe 3 exportiert GTK das Fensterobjekt nicht mehr auf D-Bus**
  (`/org/gnomelex/Gesetze/Devel/window/1` fehlt, `win.*` sind per `gdbus`
  nicht erreichbar; Ursache noch nicht geklärt). Für Tests deshalb
  App-Aktionen nutzen: `gdbus call … --object-path /org/gnomelex/Gesetze/Devel
  --method org.gtk.Actions.Activate show-norm "[<'§ 433'>]" "{}"`. Ein
  Erstimport ohne Klicken geht derzeit nur über die Schaltfläche der
  „empty“-Seite oder über den ignorierten Realdaten-Test (siehe oben).
- Headless-Sichtprüfung: `gtk4-broadwayd :7 &` und die App mit
  `GDK_BACKEND=broadway BROADWAY_DISPLAY=:7` in derselben SDK-Sandbox
  starten, dann `http://127.0.0.1:8087/` mit einem Browser (auch headless
  mit `--screenshot`) aufnehmen. Screenshots vom 23.09. liegen unter
  `~/Downloads/gnome-lex-shots/`.
- Die Claude-Sitzung vom 23.09.2026 brach um 19:22 Uhr während eines langen
  Sammelbefehls (Build + App-Neustart + Chrome-Screenshots) ab. Lange
  Befehle besser in einzelne Aufrufe zerlegen und Prozesse im Hintergrund
  mit `nohup` starten.

---

## 4. Nächster Schritt: Stufe 5 – Suche (erst nach Kommando)

Stufe 4 ist lauffähig abgeschlossen, Stufe 6 (Verweise) und Stufe 7
(Markierungen, Notizen, Favoriten) sind im Kern vorgezogen (siehe
Abschnitt 3). Vor Stufe 5: Commit nur auf Anweisung (alles mit `git add`
vorgemerkt).

**Stufe 5 (Volltextsuche):** Hinweis: Die Verweiserkennung (Stufe 6) ist
bereits als klickbare Verweise im Text umgesetzt; offen aus Stufe 6 sind
Hover-Vorschau (`GtkPopover`) und manuelle Verweise.

**Stufe 5 im Detail:** `GtkSearchBar` mit Live-Suche über
`Database::search()` (FTS5, `build_fts_query`), Trefferliste mit Snippets
(`SearchResultObject`, `search_row.blp`), Treffer im aktiven Tab öffnen,
Suchbegriff im Text hervorheben. Die Schnellsuche (Strg+K, „/“) bleibt
als Sprungfunktion bestehen.

**Weiter offen (Reste, nicht Teil von Stufe 5):**
- Fortlaufendes Lesen: Abschnitte werden nie wieder entladen (Speicher
  wächst beim Durchscrollen eines ganzen Gesetzes); `prev-/next-norm`
  laden neu statt zum Nachbarabschnitt zu springen; der ViewSwitcher der
  Seitenleiste wird bei programmatischem Wechsel nicht hervorgehoben.
- Update-Prüfung per HEAD (ETag/Last-Modified) statt Vollarchiv.
- Fehlender D-Bus-Export des Fensters (siehe Fallstricke).
- `po/gesetze.pot` nachziehen; Annotationsliste in der Seitenleiste,
  JSON-Export/-Import in der Oberfläche.
