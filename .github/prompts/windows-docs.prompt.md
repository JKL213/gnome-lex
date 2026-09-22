---
mode: agent
description: "docs/WINDOWS.md für den MSYS2-Build schreiben"
---
Schreibe `docs/WINDOWS.md` (deutsch) mit einer Schritt-für-Schritt-Anleitung
für den Build unter Windows mit MSYS2 (UCRT64):

1. Benötigte pacman-Pakete: `mingw-w64-ucrt-x86_64-{rust,gtk4,libadwaita,libsoup3,pkgconf,gettext,meson,ninja,python-gobject,blueprint-compiler,desktop-file-utils,appstream}` — prüfe die Paketnamen gegen https://packages.msys2.org und korrigiere sie.
2. `cargo build --release` in der UCRT64-Shell; Hinweis auf `PKG_CONFIG_PATH`
   und `GSETTINGS_SCHEMA_DIR`, falls das Schema nicht gefunden wird
   (siehe `src/settings.rs` für die Suchreihenfolge).
3. Alternativ `meson setup _build --prefix=<pfad> && meson install -C _build`.
4. Paketierung: welche DLLs, Icons (`share/icons/Adwaita`), GSettings-Schemas,
   `share/glib-2.0/schemas/gschemas.compiled` und `share/locale` neben die
   EXE gehören; `glib-compile-schemas` nach der Installation.
5. Bekannte Einschränkungen (kein Portal, Farbschema über `AdwStyleManager`,
   Datenverzeichnis `%LOCALAPPDATA%\gesetze` über `glib::user_data_dir()`).

Keine Codeänderungen. Verlinke die Datei aus `README.md`, falls noch nicht geschehen.
