//! Hauptfenster.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use crate::settings;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/window.ui")]
    pub struct LexWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub status_page: TemplateChild<adw::StatusPage>,
        pub settings: std::cell::OnceCell<gio::Settings>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexWindow {
        const NAME: &'static str = "LexWindow";
        type Type = super::LexWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            let settings = settings::settings();
            settings
                .bind("window-width", &*obj, "default-width")
                .build();
            settings
                .bind("window-height", &*obj, "default-height")
                .build();
            settings
                .bind("window-maximized", &*obj, "maximized")
                .build();
            self.settings.set(settings).ok();
            if crate::config::is_development() {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for LexWindow {}
    impl WindowImpl for LexWindow {}
    impl ApplicationWindowImpl for LexWindow {}
    impl AdwApplicationWindowImpl for LexWindow {}
}

glib::wrapper! {
    pub struct LexWindow(ObjectSubclass<imp::LexWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Root, gtk::Native, gtk::ShortcutManager;
}

impl LexWindow {
    pub fn new(app: &impl IsA<gtk::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn settings(&self) -> &gio::Settings {
        self.imp().settings.get().expect("settings")
    }
}
