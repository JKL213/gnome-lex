// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Schnellsuche: ein Dialog mit Eingabefeld und Trefferliste. Die Eingabe
//! wird nicht nach Gesetz oder Paragrafnummer unterschieden
//! („433“, „zpo 253“, „§ 55a bgb“, „Kaufvertrag“); Eingabe springt zum
//! ersten bzw. ausgewählten Treffer.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::db::{Database, QuickHit};

/// Höchstzahl angezeigter Treffer.
const LIMIT: usize = 30;

/// Aufruf beim Sprung: Norm-ID und ob ein neuer Tab gewünscht ist.
type JumpCallback = Box<dyn Fn(i64, bool)>;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/quick_search.ui")]
    pub struct LexQuickSearch {
        #[template_child]
        pub entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub results: TemplateChild<gtk::ListBox>,
        /// Norm-IDs der Zeilen in `results`, gleiche Reihenfolge.
        pub norm_ids: RefCell<Vec<i64>>,
        pub preferred_law: Cell<Option<i64>>,
        pub serial: Cell<u64>,
        pub on_jump: RefCell<Option<JumpCallback>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexQuickSearch {
        const NAME: &'static str = "LexQuickSearch";
        type Type = super::LexQuickSearch;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexQuickSearch {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for LexQuickSearch {}
    impl AdwDialogImpl for LexQuickSearch {}
}

glib::wrapper! {
    pub struct LexQuickSearch(ObjectSubclass<imp::LexQuickSearch>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexQuickSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl LexQuickSearch {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Gesetz, dessen Treffer bei gleicher Güte vorn stehen.
    pub fn set_preferred_law(&self, law_id: Option<i64>) {
        self.imp().preferred_law.set(law_id);
    }

    /// Belegt das Eingabefeld vor und sucht sofort.
    pub fn set_query(&self, query: &str) {
        self.imp().entry.set_text(query);
    }

    pub fn connect_jump(&self, f: impl Fn(i64, bool) + 'static) {
        *self.imp().on_jump.borrow_mut() = Some(Box::new(f));
    }

    fn setup(&self) {
        let imp = self.imp();
        self.set_focus(Some(&*imp.entry));
        imp.entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |entry| dialog.run_search(entry.text().as_str())
        ));
        imp.entry.connect_activate(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| dialog.jump_selected(false)
        ));
        imp.results.connect_row_activated(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_, row| {
                dialog.imp().results.select_row(Some(row));
                dialog.jump_selected(false);
            }
        ));
        // Pfeiltasten und Strg+Eingabe werden vor dem Eingabefeld abgefangen.
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, state| {
                let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
                match key {
                    gdk::Key::Down => {
                        dialog.move_selection(1);
                        glib::Propagation::Stop
                    }
                    gdk::Key::Up => {
                        dialog.move_selection(-1);
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter if ctrl => {
                        dialog.jump_selected(true);
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            }
        ));
        imp.entry.add_controller(keys);
    }

    fn move_selection(&self, delta: i32) {
        let imp = self.imp();
        let count = imp.norm_ids.borrow().len() as i32;
        if count == 0 {
            return;
        }
        let current = imp
            .results
            .selected_row()
            .map(|r| r.index())
            .unwrap_or(if delta > 0 { -1 } else { 0 });
        let next = (current + delta).clamp(0, count - 1);
        if let Some(row) = imp.results.row_at_index(next) {
            imp.results.select_row(Some(&row));
            row.grab_focus();
            imp.entry.grab_focus();
        }
    }

    /// Springt zum ausgewählten (sonst ersten) Treffer und schließt den Dialog.
    fn jump_selected(&self, new_tab: bool) {
        let imp = self.imp();
        let index = imp
            .results
            .selected_row()
            .map(|r| r.index())
            .unwrap_or(0)
            .max(0) as usize;
        let Some(norm_id) = imp.norm_ids.borrow().get(index).copied() else {
            return;
        };
        if let Some(cb) = imp.on_jump.borrow().as_ref() {
            cb(norm_id, new_tab);
        }
        self.close();
    }

    fn run_search(&self, input: &str) {
        let imp = self.imp();
        let serial = imp.serial.get().wrapping_add(1);
        imp.serial.set(serial);
        let input = input.trim().to_owned();
        if input.is_empty() {
            self.show_hits(&[]);
            return;
        }
        let preferred = imp.preferred_law.get();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let path = Database::default_path();
                let result =
                    gio::spawn_blocking(move || -> Result<Vec<QuickHit>, rusqlite::Error> {
                        Database::open(&path)?.quick_search(&input, preferred, LIMIT)
                    })
                    .await;
                if dialog.imp().serial.get() != serial {
                    return;
                }
                match result {
                    Ok(Ok(hits)) => dialog.show_hits(&hits),
                    Ok(Err(err)) => log::warn!("Schnellsuche fehlgeschlagen: {err}"),
                    Err(_) => log::warn!("Schnellsuche abgebrochen"),
                }
            }
        ));
    }

    fn show_hits(&self, hits: &[QuickHit]) {
        let imp = self.imp();
        imp.results.remove_all();
        let mut ids = Vec::with_capacity(hits.len());
        for hit in hits {
            let (title, subtitle) = hit_labels(hit);
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&title))
                .subtitle(glib::markup_escape_text(&subtitle))
                .activatable(true)
                .build();
            imp.results.append(&row);
            ids.push(hit.norm.id);
        }
        *imp.norm_ids.borrow_mut() = ids;
        if let Some(first) = imp.results.row_at_index(0) {
            imp.results.select_row(Some(&first));
        }
    }
}

/// Zeilenbeschriftung eines Treffers: „§ 433 BGB“ und der Normtitel.
fn hit_labels(hit: &QuickHit) -> (String, String) {
    match &hit.norm.enbez {
        Some(enbez) => (
            format!("{enbez} {}", hit.law_abbrev),
            hit.norm.titel.clone().unwrap_or_default(),
        ),
        None => (
            hit.norm
                .titel
                .clone()
                .unwrap_or_else(|| hit.norm.doknr.clone()),
            hit.law_abbrev.clone(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NormInfo;

    #[test]
    fn labels() {
        let hit = QuickHit {
            norm: NormInfo {
                enbez: Some("§ 253".into()),
                titel: Some("Klageschrift".into()),
                ..Default::default()
            },
            law_slug: "zpo".into(),
            law_abbrev: "ZPO".into(),
        };
        assert_eq!(
            hit_labels(&hit),
            ("§ 253 ZPO".into(), "Klageschrift".into())
        );
    }
}
