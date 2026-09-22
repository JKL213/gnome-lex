---
applyTo: "src/**/*.rs"
---
# Rust-Konventionen

- GObject-Subklassen nach dem Muster in `src/model/objects.rs` und `src/window.rs`:
  `mod imp` mit `#[glib::object_subclass]`, außen `glib::wrapper!`.
  Pro `#[derive(glib::Properties)]` ein eigenes `mod`, sonst kollidiert
  `DerivedPropertiesEnum`.
- Composite Templates: `#[derive(CompositeTemplate)]` mit
  `#[template(resource = "/org/gnomelex/Gesetze/ui/<name>.ui")]`,
  `klass.bind_template()` in `class_init`, `obj.init_template()` in `instance_init`.
- Fehler als `enum` mit `Display`- und `Error`-Impl (siehe `ImportError`),
  keine zusätzlichen Fehler-Crates einführen.
- Datenbankzugriff nur über `crate::db::Database`; SQL bleibt in `src/db`.
- Zeichenoffsets sind Unicode-Skalare (wie `GtkTextBuffer`), nie Byteoffsets.
- Tests als `#[cfg(test)] mod tests` in derselben Datei.
- `cargo fmt` vor dem Abschluss; keine `#[allow]` ohne Begründung im Kommentar.
