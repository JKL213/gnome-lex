// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! `AdwApplication`-Subklasse: Aktionen, Tastenkürzel, Dialoge.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::{APP_ID, VERSION};
use crate::widgets::{
    preferences::{apply_color_scheme, apply_reading_font},
    LexPreferences,
};
use crate::window::LexWindow;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LexApplication;

    #[glib::object_subclass]
    impl ObjectSubclass for LexApplication {
        const NAME: &'static str = "LexApplication";
        type Type = super::LexApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for LexApplication {}

    impl ApplicationImpl for LexApplication {
        fn activate(&self) {
            let app = self.obj();
            if let Some(window) = app.active_window() {
                window.present();
                return;
            }
            let window = LexWindow::new(&*app);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();
            let app = self.obj();
            app.setup_actions();
            app.setup_accels();
            app.load_css();
            let settings = crate::settings::settings();
            apply_color_scheme(&settings);
            apply_reading_font(&settings);
        }
    }

    impl GtkApplicationImpl for LexApplication {}
    impl AdwApplicationImpl for LexApplication {}
}

glib::wrapper! {
    pub struct LexApplication(ObjectSubclass<imp::LexApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for LexApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl LexApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }

    fn setup_actions(&self) {
        // Beim Beenden über die Aktion die Tabs sichern (kein close-request).
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                for window in app.windows() {
                    if let Some(window) = window.downcast_ref::<LexWindow>() {
                        window.save_state_before_quit();
                    }
                }
                app.quit()
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        let preferences = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                LexPreferences::new().present(app.active_window().as_ref());
            })
            .build();
        let shortcuts = gio::ActionEntry::builder("shortcuts")
            .activate(|app: &Self, _, _| app.show_shortcuts())
            .build();
        // Norm im aktiven Fenster anzeigen (Parameter: Bezeichnung wie „§ 433“).
        let show_norm = gio::ActionEntry::builder("show-norm")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|app: &Self, _, param| {
                let Some(enbez) = param.and_then(|p| p.get::<String>()) else {
                    return;
                };
                if let Some(window) = app.active_window().and_downcast::<LexWindow>() {
                    window.show_norm_by_enbez(&enbez);
                }
            })
            .build();
        // Fensteraktion im aktiven Fenster auslösen („download bgb“,
        // „new-tab“); ein Wort nach dem Namen wird als String-Parameter
        // übergeben. Nur für Tests per D-Bus, da GTK das Fensterobjekt
        // derzeit nicht exportiert.
        let window_action = gio::ActionEntry::builder("window-action")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|app: &Self, _, param| {
                let Some(spec) = param.and_then(|p| p.get::<String>()) else {
                    return;
                };
                let (name, arg) = match spec.split_once(' ') {
                    Some((n, a)) => (n.to_owned(), Some(a.trim().to_variant())),
                    None => (spec.clone(), None),
                };
                if let Some(window) = app.active_window().and_downcast::<LexWindow>() {
                    ActionGroupExt::activate_action(&window, &name, arg.as_ref());
                }
            })
            .build();
        self.add_action_entries([
            quit,
            about,
            preferences,
            shortcuts,
            show_norm,
            window_action,
        ]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.set_accels_for_action("app.shortcuts", &["<Control>question"]);
        self.set_accels_for_action("win.toggle-notes", &["<Control><Shift>n"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.split", &["<Control><Shift>d"]);
        self.set_accels_for_action("win.switch-pane", &["F6"]);
        self.set_accels_for_action("win.toggle-favorite", &["<Control>d"]);
        // „/“ wird zusätzlich im Fenster abgefangen (nur außerhalb von Eingabefeldern).
        self.set_accels_for_action("win.quick-search", &["<Control>k"]);
        self.set_accels_for_action("win.prev-norm", &["<Alt>Page_Up"]);
        self.set_accels_for_action("win.next-norm", &["<Alt>Page_Down"]);
        self.set_accels_for_action(
            "win.zoom-in",
            &["<Control>plus", "<Control>equal", "<Control>KP_Add"],
        );
        self.set_accels_for_action("win.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]);
        self.set_accels_for_action("win.zoom-reset", &["<Control>0", "<Control>KP_0"]);
    }

    fn load_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/org/gnomelex/Gesetze/style.css");
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    /// Übersicht der Tastenkürzel (`Adw.ShortcutsDialog`).
    fn show_shortcuts(&self) {
        let dialog = adw::ShortcutsDialog::new();
        let sections: [(&str, &[(&str, &str)]); 4] = [
            (
                &gettext("Navigation"),
                &[
                    (&gettext("Schnellsuche"), "<Control>k slash"),
                    (&gettext("Vorherige Norm"), "<Alt>Page_Up"),
                    (&gettext("Nächste Norm"), "<Alt>Page_Down"),
                    (&gettext("Favorit setzen oder entfernen"), "<Control>d"),
                    (
                        &gettext("Notiz / Schema ein- oder ausblenden"),
                        "<Control><Shift>n",
                    ),
                ],
            ),
            (
                &gettext("Ansicht"),
                &[
                    (&gettext("Geteilte Ansicht"), "<Control><Shift>d"),
                    (&gettext("Zwischen den Ansichten wechseln"), "F6"),
                    (&gettext("Fenster schließen"), "<Control>w"),
                ],
            ),
            (
                &gettext("Schrift"),
                &[
                    (&gettext("Vergrößern"), "<Control>plus"),
                    (&gettext("Verkleinern"), "<Control>minus"),
                    (&gettext("Zurücksetzen"), "<Control>0"),
                ],
            ),
            (
                &gettext("Allgemein"),
                &[
                    (&gettext("Einstellungen"), "<Control>comma"),
                    (&gettext("Tastenkürzel"), "<Control>question"),
                    (&gettext("Beenden"), "<Control>q"),
                ],
            ),
        ];
        for (title, items) in sections {
            let section = adw::ShortcutsSection::new(Some(title));
            for (name, accel) in items {
                section.add(adw::ShortcutsItem::new(name, accel));
            }
            dialog.add(section);
        }
        dialog.present(self.active_window().as_ref());
    }

    fn show_about(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name(gettext("Gesetze"))
            .application_icon(APP_ID)
            .developer_name("Gnome Lex")
            .version(VERSION)
            .license_type(gtk::License::Lgpl30)
            .comments(gettext(
                "Deutsche Bundesgesetze lesen und annotieren. Datenquelle: gesetze-im-internet.de",
            ))
            .website("https://github.com/gnome-lex/gnome-lex")
            .issue_url("https://github.com/gnome-lex/gnome-lex/issues")
            .build();
        dialog.present(self.active_window().as_ref());
    }
}
