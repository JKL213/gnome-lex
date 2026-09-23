// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! `AdwApplication`-Subklasse: Aktionen, Tastenkürzel, Dialoge.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::{APP_ID, VERSION};
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
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| app.quit())
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([quit, about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
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
