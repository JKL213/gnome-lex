// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod application;
mod config;
mod db;
mod hints;
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

    // Gewählte Oberflächensprache (Einstellung „language“) vor der
    // Locale-Initialisierung über LANGUAGE setzen; gettext wertet die
    // Variable mit Vorrang vor LC_MESSAGES aus – außer in der Locale „C“.
    // SAFETY: wird vor dem Start weiterer Threads einmalig aufgerufen.
    unsafe {
        let preferred = settings::preferred_language();
        if let Some(lang) = &preferred {
            std::env::set_var("LANGUAGE", lang);
        }
        setlocale(LocaleCategory::LcAll, "");
        if let Some(lang) = &preferred {
            ensure_messages_locale(lang);
        }
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

/// In der Locale „C“ (auch „C.UTF-8“) ignoriert gettext die Variable
/// LANGUAGE. Läuft die App ohne konfigurierte Systemsprache, wird für
/// LC_MESSAGES eine installierte UTF-8-Locale gewählt – bevorzugt eine der
/// gewünschten Sprache –, damit die Übersetzung greift.
///
/// # Safety
/// Nur vor dem Start weiterer Threads aufrufen (setlocale ist nicht threadsicher).
unsafe fn ensure_messages_locale(lang: &str) {
    // Erneut aus der Umgebung setzen liefert den Namen der aktiven Locale.
    let current = setlocale(LocaleCategory::LcMessages, "")
        .map(|c| String::from_utf8_lossy(&c).to_string())
        .unwrap_or_default();
    let is_c = matches!(current.as_str(), "" | "C" | "POSIX" | "C.UTF-8" | "C.utf8");
    if !is_c {
        return;
    }
    let region = match lang {
        "de" => "de_DE",
        "en" => "en_US",
        "ru" => "ru_RU",
        other => other,
    };
    let candidates = [
        format!("{region}.UTF-8"),
        format!("{region}.utf8"),
        "en_US.UTF-8".to_owned(),
        "de_DE.UTF-8".to_owned(),
        "C.UTF-8".to_owned(),
    ];
    for candidate in &candidates {
        if setlocale(LocaleCategory::LcMessages, candidate.as_str()).is_some() {
            log::debug!("LC_MESSAGES auf {candidate} gesetzt (Sprache {lang})");
            return;
        }
    }
    log::warn!("Keine passende Locale für die Sprache {lang} gefunden");
}
