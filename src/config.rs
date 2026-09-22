//! Zur Build-Zeit festgelegte Konstanten (siehe `build.rs` und `src/meson.build`).

pub const APP_ID: &str = env!("APP_ID");
pub const VERSION: &str = env!("VERSION");
pub const PROFILE: &str = env!("PROFILE");
pub const LOCALEDIR: &str = env!("LOCALEDIR");
pub const PKGDATADIR: &str = env!("PKGDATADIR");
pub const GSCHEMA_DIR: &str = env!("GSCHEMA_DIR");
pub const GETTEXT_PACKAGE: &str = "gesetze";
pub const RESOURCE_PREFIX: &str = "/org/gnomelex/Gesetze";
pub const BASE_SCHEMA_ID: &str = "org.gnomelex.Gesetze";

/// Die App-ID ohne `.Devel`-Suffix; das GSettings-Schema trägt immer die Basis-ID.
pub fn is_development() -> bool {
    PROFILE == "development"
}
