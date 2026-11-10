// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Randspalte der Normansicht: ein schlichtes Widget fester Breite, das
//! seinen Inhalt über einen Rückruf in den `GtkSnapshot` zeichnet. Die
//! Normansicht setzt es als linkes Gutter ihres `GtkTextView` ein und malt
//! darin die Hinweispfeile mit der Nummer der Anschlussnorm.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

type DrawCallback = Box<dyn Fn(&LexHintGutter, &gtk::Snapshot)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LexHintGutter {
        pub width: Cell<i32>,
        pub draw: RefCell<Option<DrawCallback>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexHintGutter {
        const NAME: &'static str = "LexHintGutter";
        type Type = super::LexHintGutter;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("hint-gutter");
        }
    }

    impl ObjectImpl for LexHintGutter {}

    impl WidgetImpl for LexHintGutter {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let width = self.width.get();
                    (width, width, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(draw) = self.draw.borrow().as_ref() {
                draw(&self.obj(), snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct LexHintGutter(ObjectSubclass<imp::LexHintGutter>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexHintGutter {
    fn default() -> Self {
        Self::new()
    }
}

impl LexHintGutter {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Legt die Breite der Spalte in Pixeln fest.
    pub fn set_gutter_width(&self, width: i32) {
        let width = width.max(0);
        if self.imp().width.replace(width) != width {
            self.queue_resize();
        }
    }

    /// Setzt den Rückruf, der den Inhalt zeichnet.
    pub fn set_draw(&self, draw: impl Fn(&Self, &gtk::Snapshot) + 'static) {
        self.imp().draw.replace(Some(Box::new(draw)));
        self.queue_draw();
    }
}
