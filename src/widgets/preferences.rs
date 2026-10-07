// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Einstellungsdialog (`Adw.PreferencesDialog`): Farbschema, Schriftgröße
//! und Aktualisierungsprüfung, direkt an GSettings gebunden.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, pango, CompositeTemplate};

use crate::settings;

/// Schriftfamilien, die der Reihe nach versucht werden, wenn keine
/// eigene eingestellt ist.
pub const DEFAULT_READING_FONTS: &str =
    "\"Public Sans\", \"Source Sans 3\", \"Inter\", \"Inter Variable\", \"Adwaita Sans\", sans-serif";

/// Werte des GSettings-Schlüssels `color-scheme` in Reihenfolge des Auswahlfelds.
const COLOR_SCHEMES: [&str; 3] = ["default", "light", "dark"];

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/preferences.ui")]
    pub struct LexPreferences {
        #[template_child]
        pub color_scheme_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub font_size_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub font_button: TemplateChild<gtk::FontDialogButton>,
        #[template_child]
        pub font_reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub check_updates_row: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexPreferences {
        const NAME: &'static str = "LexPreferences";
        type Type = super::LexPreferences;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexPreferences {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().bind_settings();
        }
    }

    impl WidgetImpl for LexPreferences {}
    impl AdwDialogImpl for LexPreferences {}
    impl PreferencesDialogImpl for LexPreferences {}
}

glib::wrapper! {
    pub struct LexPreferences(ObjectSubclass<imp::LexPreferences>)
        @extends gtk::Widget, adw::Dialog, adw::PreferencesDialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexPreferences {
    fn default() -> Self {
        Self::new()
    }
}

impl LexPreferences {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn bind_settings(&self) {
        let imp = self.imp();
        let settings = settings::settings();
        settings
            .bind("font-size", &*imp.font_size_row, "value")
            .build();
        settings
            .bind("check-updates", &*imp.check_updates_row, "active")
            .build();
        // Schriftfamilie: Schaltfläche zeigt die Einstellung, Änderung speichert.
        let family = settings.string("reading-font");
        if !family.is_empty() {
            imp.font_button
                .set_font_desc(&pango::FontDescription::from_string(&family));
        }
        imp.font_button.connect_font_desc_notify(glib::clone!(
            #[strong]
            settings,
            move |button| {
                let family = button
                    .font_desc()
                    .and_then(|d| d.family().map(|f| f.to_string()))
                    .unwrap_or_default();
                if let Err(err) = settings.set_string("reading-font", &family) {
                    log::warn!("reading-font konnte nicht gespeichert werden: {err}");
                }
            }
        ));
        imp.font_reset_button.connect_clicked(glib::clone!(
            #[strong]
            settings,
            #[weak(rename_to = button)]
            imp.font_button,
            move |_| {
                settings.reset("reading-font");
                button.set_font_desc(&pango::FontDescription::from_string("Public Sans"));
            }
        ));
        let current = settings.string("color-scheme");
        let index = COLOR_SCHEMES
            .iter()
            .position(|s| *s == current.as_str())
            .unwrap_or(0);
        imp.color_scheme_row.set_selected(index as u32);
        imp.color_scheme_row.connect_selected_notify(move |row| {
            let value = COLOR_SCHEMES
                .get(row.selected() as usize)
                .copied()
                .unwrap_or("default");
            if let Err(err) = settings.set_string("color-scheme", value) {
                log::warn!("color-scheme konnte nicht gespeichert werden: {err}");
            }
        });
    }
}

/// Setzt die Schriftfamilie des Lesetexts per CSS und folgt der Einstellung.
pub fn apply_reading_font(settings: &gio::Settings) {
    let provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
    let apply = move |settings: &gio::Settings| {
        let family = settings.string("reading-font");
        let stack = if family.is_empty() {
            DEFAULT_READING_FONTS.to_owned()
        } else {
            format!("\"{}\", {DEFAULT_READING_FONTS}", family.replace('"', ""))
        };
        provider.load_from_string(&format!(
            ".norm-text, .norm-text label, .note-editor {{ font-family: {stack}; }}"
        ));
    };
    apply(settings);
    settings.connect_changed(Some("reading-font"), move |settings, _| apply(settings));
}

/// Wendet das in GSettings gewählte Farbschema an und folgt Änderungen.
pub fn apply_color_scheme(settings: &gio::Settings) {
    let apply = |settings: &gio::Settings| {
        let scheme = match settings.string("color-scheme").as_str() {
            "light" => adw::ColorScheme::ForceLight,
            "dark" => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
    };
    apply(settings);
    settings.connect_changed(Some("color-scheme"), move |settings, _| apply(settings));
}
