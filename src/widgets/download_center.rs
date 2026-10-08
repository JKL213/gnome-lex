// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Download-Center: listet alle bekannten Gesetzesquellen, getrennt nach
//! installiert und verfügbar, mit Stand-Vermerk und einer Schaltfläche, die
//! `win.download` mit dem Slug auslöst. Das Fenster ruft nach jedem Import
//! [`LexDownloadCenter::refresh`] auf.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};

use crate::importer::SOURCES;
use crate::model::LawInfo;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/download_center.ui")]
    pub struct LexDownloadCenter {
        #[template_child]
        pub installed_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub available_group: TemplateChild<adw::PreferencesGroup>,
        /// Eingefügte Zeilen, damit `refresh` sie wieder entfernen kann.
        pub rows: RefCell<Vec<(adw::PreferencesGroup, adw::ActionRow)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexDownloadCenter {
        const NAME: &'static str = "LexDownloadCenter";
        type Type = super::LexDownloadCenter;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexDownloadCenter {}
    impl WidgetImpl for LexDownloadCenter {}
    impl AdwDialogImpl for LexDownloadCenter {}
}

glib::wrapper! {
    pub struct LexDownloadCenter(ObjectSubclass<imp::LexDownloadCenter>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexDownloadCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl LexDownloadCenter {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Baut die Zeilen anhand der installierten Gesetze neu auf.
    pub fn refresh(&self, laws: &[LawInfo]) {
        let imp = self.imp();
        for (group, row) in imp.rows.borrow_mut().drain(..) {
            group.remove(&row);
        }
        let mut rows = Vec::new();
        for source in SOURCES {
            let installed = laws.iter().find(|l| l.slug == source.slug);
            let (subtitle, button_label) = match installed {
                Some(law) => (installed_subtitle(law), gettext("Aktualisieren")),
                None => (
                    gettext("{abbrev} · nicht installiert").replace("{abbrev}", source.abbrev),
                    gettext("Herunterladen"),
                ),
            };
            let button = gtk::Button::builder()
                .label(button_label)
                .valign(gtk::Align::Center)
                .action_name("win.download")
                .tooltip_text(
                    gettext("{abbrev} von gesetze-im-internet.de laden")
                        .replace("{abbrev}", source.abbrev),
                )
                .build();
            button.set_action_target_value(Some(&source.slug.to_variant()));
            if installed.is_none() {
                button.add_css_class("suggested-action");
            }
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(source.name))
                .subtitle(glib::markup_escape_text(&subtitle))
                .build();
            row.add_suffix(&button);
            let group = if installed.is_some() {
                &*imp.installed_group
            } else {
                &*imp.available_group
            };
            group.add(&row);
            rows.push((group.clone(), row));
        }
        imp.installed_group.set_visible(
            laws.iter()
                .any(|l| SOURCES.iter().any(|s| s.slug == l.slug)),
        );
        *imp.rows.borrow_mut() = rows;
    }
}

/// Untertitel einer installierten Fassung: Kürzel, Stand und Importdatum.
fn installed_subtitle(law: &LawInfo) -> String {
    let mut parts = vec![law.jurabk.clone()];
    if let Some(stand) = &law.stand {
        parts.push(stand.clone());
    }
    if let Some(date) = law.imported_at.get(..10) {
        parts.push(gettext("importiert am {date}").replace("{date}", &format_iso_date(date)));
    }
    parts.join(" · ")
}

/// `JJJJ-MM-TT` → `TT.MM.JJJJ`, sonst unverändert.
fn format_iso_date(date: &str) -> String {
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => d.format("%d.%m.%Y").to_string(),
        Err(_) => date.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_lists_abbrev_stand_and_date() {
        let law = LawInfo {
            jurabk: "ZPO".into(),
            stand: Some("zuletzt geändert …".into()),
            imported_at: "2026-10-07T12:39:24".into(),
            ..Default::default()
        };
        assert_eq!(
            installed_subtitle(&law),
            "ZPO · zuletzt geändert … · importiert am 07.10.2026"
        );
        assert_eq!(format_iso_date("kaputt"), "kaputt");
    }
}
