#![doc = include_str!("../README.md")]

mod application;
mod config;
mod db;
mod importer;
mod model;
mod refs;
mod settings;
mod widgets;
mod window;

use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::application::LexApplication;
use crate::config::{GETTEXT_PACKAGE, RESOURCE_PREFIX};

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // SAFETY: wird vor dem Start weiterer Threads einmalig aufgerufen.
    unsafe {
        setlocale(LocaleCategory::LcAll, "");
    }
    bindtextdomain(GETTEXT_PACKAGE, locale_dir()).expect("bindtextdomain");
    bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8").expect("bind_textdomain_codeset");
    textdomain(GETTEXT_PACKAGE).expect("textdomain");

    glib::set_application_name("Gesetze");

    gio::resources_register_include!("gesetze.gresource").expect("GResource laden");

    let app = LexApplication::new();
    app.set_resource_base_path(Some(RESOURCE_PREFIX));
    app.run()
}

/// Verzeichnis der Übersetzungen: Meson-Wert, sonst relativ zur ausführbaren
/// Datei (`<prefix>/bin/../share/locale`), damit Windows-Builds ohne feste
/// Pfade auskommen.
fn locale_dir() -> std::path::PathBuf {
    let configured = std::path::PathBuf::from(config::LOCALEDIR);
    if configured.is_dir() {
        return configured;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
        })
        .map(|prefix| prefix.join("share").join("locale"))
        .unwrap_or(configured)
}
