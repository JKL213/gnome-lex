// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Zeile des Gliederungsbaums: `GtkTreeExpander` mit Bezeichnung
//! („Buch 1“, „§ 433“) und Titel.

use std::cell::Cell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, CompositeTemplate};

use crate::model::OutlineItem;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/outline_row.ui")]
    pub struct LexOutlineRow {
        #[template_child]
        pub expander: TemplateChild<gtk::TreeExpander>,
        #[template_child]
        pub label_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        /// ID des gebundenen Objekts (0 = keins) und ob es eine Einheit ist.
        pub item_id: Cell<i64>,
        pub is_unit: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexOutlineRow {
        const NAME: &'static str = "LexOutlineRow";
        type Type = super::LexOutlineRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexOutlineRow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_gesture();
        }
    }
    impl WidgetImpl for LexOutlineRow {}
    impl BoxImpl for LexOutlineRow {}
}

glib::wrapper! {
    pub struct LexOutlineRow(ObjectSubclass<imp::LexOutlineRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for LexOutlineRow {
    fn default() -> Self {
        Self::new()
    }
}

impl LexOutlineRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Mittelklick oder Strg+Klick auf eine Norm öffnet sie in einem neuen
    /// Ansicht (`win.open-in-other-pane`); der normale Klick bleibt der Liste.
    fn setup_gesture(&self) {
        let gesture = gtk::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        gesture.connect_pressed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |gesture, _, _, _| {
                let imp = row.imp();
                let id = imp.item_id.get();
                if imp.is_unit.get() || id == 0 {
                    return;
                }
                let button = gesture.current_button();
                let ctrl = gesture
                    .current_event_state()
                    .contains(gdk::ModifierType::CONTROL_MASK);
                if button == 2 || (button == 1 && ctrl) {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    if let Err(err) =
                        row.activate_action("win.open-in-other-pane", Some(&id.to_variant()))
                    {
                        log::warn!("open-in-other-pane nicht erreichbar: {err}");
                    }
                }
            }
        ));
        self.add_controller(gesture);
    }

    /// Verbindet die Zeile mit einer Baumzeile (oder löst sie bei `None`).
    pub fn bind(&self, row: Option<&gtk::TreeListRow>) {
        let imp = self.imp();
        imp.expander.set_list_row(row);
        let item = row.and_then(|r| r.item()).and_downcast::<OutlineItem>();
        imp.item_id.set(item.as_ref().map(|i| i.id()).unwrap_or(0));
        imp.is_unit.set(item.as_ref().is_some_and(|i| i.is_unit()));
        match item {
            Some(item) => {
                let is_unit = item.is_unit();
                let label = item.label();
                let title = item.title();
                imp.label_label.set_label(&label);
                imp.title_label.set_label(&title);
                imp.title_label.set_visible(!title.is_empty());
                if is_unit {
                    imp.label_label.add_css_class("unit");
                } else {
                    imp.label_label.remove_css_class("unit");
                }
                if is_repealed(&title) {
                    self.add_css_class("repealed");
                } else {
                    self.remove_css_class("repealed");
                }
                let full = if title.is_empty() {
                    label
                } else {
                    format!("{label} {title}")
                };
                self.set_tooltip_text(Some(&full));
            }
            None => {
                imp.label_label.set_label("");
                imp.title_label.set_label("");
                self.remove_css_class("repealed");
                self.set_tooltip_text(None);
            }
        }
    }
}

/// Weggefallene Normen tragen als Titel nur „(weggefallen)“.
fn is_repealed(title: &str) -> bool {
    title.trim().trim_matches(|c| c == '(' || c == ')') == "weggefallen"
}
