//! GSettings-Zugriff mit Rückfallebenen: Wird das Schema nicht in der
//! Standardquelle gefunden (z. B. `cargo run` ohne Installation oder ein
//! Windows-Build), wird es aus dem Build-Verzeichnis bzw. relativ zur
//! ausführbaren Datei geladen.

use std::path::PathBuf;

use gtk::gio;

use crate::config::{BASE_SCHEMA_ID, GSCHEMA_DIR};

/// Liefert die Einstellungen der Anwendung.
pub fn settings() -> gio::Settings {
    if let Some(source) = gio::SettingsSchemaSource::default() {
        if let Some(schema) = source.lookup(BASE_SCHEMA_ID, true) {
            return gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
        }
    }
    for dir in candidate_schema_dirs() {
        if !dir.join("gschemas.compiled").is_file() {
            continue;
        }
        let parent = gio::SettingsSchemaSource::default();
        match gio::SettingsSchemaSource::from_directory(&dir, parent.as_ref(), true) {
            Ok(source) => {
                if let Some(schema) = source.lookup(BASE_SCHEMA_ID, true) {
                    log::info!("GSettings-Schema aus {} geladen", dir.display());
                    return gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
                }
            }
            Err(err) => log::warn!("Schema-Verzeichnis {} unbrauchbar: {err}", dir.display()),
        }
    }
    panic!(
        "GSettings-Schema {BASE_SCHEMA_ID} nicht gefunden. Bitte `glib-compile-schemas` \
         ausführen oder GSETTINGS_SCHEMA_DIR setzen."
    );
}

fn candidate_schema_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(env_dir) = std::env::var("GSETTINGS_SCHEMA_DIR") {
        dirs.push(PathBuf::from(env_dir));
    }
    dirs.push(PathBuf::from(GSCHEMA_DIR));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            dirs.push(prefix.join("share").join("glib-2.0").join("schemas"));
        }
    }
    dirs
}
