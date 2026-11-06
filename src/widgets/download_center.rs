// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Download-Center: listet alle bekannten Gesetzesquellen – installierte
//! Fassungen, Bundesrecht und das Landesrecht je Land (mit stilisiertem
//! Wappen) – mit Stand-Vermerk und einer Schaltfläche, die `win.download`
//! mit dem Slug auslöst. Länder ohne maschinenlesbaren Export erhalten
//! einen Verweis auf ihr Rechtsportal. Das Fenster ruft nach jedem Import
//! [`LexDownloadCenter::refresh`] auf.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib, CompositeTemplate};

use crate::importer::{Source, SOURCES};
use crate::model::states::{Jurisdiction, State};
use crate::model::LawInfo;

/// Pixelgröße der Wappen in den Zeilen.
const EMBLEM_SIZE: i32 = 24;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/download_center.ui")]
    pub struct LexDownloadCenter {
        #[template_child]
        pub installed_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub federal_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub state_group: TemplateChild<adw::PreferencesGroup>,
        /// Eingefügte Zeilen, damit `refresh` sie wieder entfernen kann.
        pub rows: RefCell<Vec<(adw::PreferencesGroup, gtk::Widget)>>,
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
        let mut rows: Vec<(adw::PreferencesGroup, gtk::Widget)> = Vec::new();
        let mut add = |group: &adw::PreferencesGroup, row: &gtk::Widget| {
            group.add(row);
            rows.push((group.clone(), row.clone()));
        };

        // Installiert: alle Quellen mit vorhandener Fassung, in Listenreihenfolge.
        let mut any_installed = false;
        for source in SOURCES {
            if let Some(law) = laws.iter().find(|l| l.slug == source.slug) {
                any_installed = true;
                add(
                    &imp.installed_group,
                    source_row(source, Some(law)).upcast_ref(),
                );
            }
        }
        imp.installed_group.set_visible(any_installed);

        // Bundesrecht: noch nicht installierte Quellen.
        for source in Source::of(Jurisdiction::Bund) {
            if !laws.iter().any(|l| l.slug == source.slug) {
                add(&imp.federal_group, source_row(source, None).upcast_ref());
            }
        }

        // Landesrecht: je Land ein Aufklapp-Eintrag mit Wappen oder ein
        // Verweis auf das Portal.
        for state in State::ALL {
            let sources: Vec<&Source> = Source::of(Jurisdiction::Land(state)).collect();
            if sources.is_empty() {
                add(&imp.state_group, portal_row(state).upcast_ref());
                continue;
            }
            let installed = sources
                .iter()
                .filter(|s| laws.iter().any(|l| l.slug == s.slug))
                .count();
            let expander = adw::ExpanderRow::builder()
                .title(glib::markup_escape_text(state.name()))
                .subtitle(glib::markup_escape_text(&state_subtitle(
                    sources.len(),
                    installed,
                    sources[0].site_name(),
                )))
                .build();
            expander.add_prefix(&emblem(Jurisdiction::Land(state)));
            for source in sources {
                let law = laws.iter().find(|l| l.slug == source.slug);
                expander.add_row(&source_row(source, law));
            }
            add(&imp.state_group, expander.upcast_ref());
        }
        *imp.rows.borrow_mut() = rows;
    }
}

/// Stilisiertes Wappen des Gesetzgebers als Zeilenpräfix.
fn emblem(jurisdiction: Jurisdiction) -> gtk::Image {
    let image = gtk::Image::builder()
        .icon_name(jurisdiction.icon_name())
        .pixel_size(EMBLEM_SIZE)
        .valign(gtk::Align::Center)
        .build();
    image.add_css_class("law-emblem");
    image
}

/// Zeile für eine Gesetzesquelle mit Schaltfläche „Herunterladen“ bzw.
/// „Aktualisieren“ (`win.download` mit dem Slug als Ziel).
fn source_row(source: &Source, installed: Option<&LawInfo>) -> adw::ActionRow {
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
            gettext("{abbrev} von {site} laden")
                .replace("{abbrev}", source.abbrev)
                .replace("{site}", source.site_name()),
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
    row.add_prefix(&emblem(source.jurisdiction));
    row.add_suffix(&button);
    row
}

/// Zeile für ein Land ohne maschinenlesbare Quelle: öffnet das Portal.
fn portal_row(state: State) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(state.name()))
        .subtitle(glib::markup_escape_text(
            &gettext("{portal} · kein Import möglich, öffnet das Portal im Browser")
                .replace("{portal}", state.portal_name()),
        ))
        .activatable(true)
        .build();
    row.add_prefix(&emblem(Jurisdiction::Land(state)));
    let button = gtk::Button::builder()
        .label(gettext("Portal öffnen"))
        .valign(gtk::Align::Center)
        .tooltip_text(state.portal_url())
        .build();
    let url = state.portal_url();
    button.connect_clicked(move |button| open_portal(button, url));
    row.add_suffix(&button);
    row.connect_activated(move |row| open_portal(row, url));
    row
}

/// Öffnet die Adresse im Standardbrowser.
fn open_portal(widget: &impl IsA<gtk::Widget>, url: &str) {
    let parent = widget.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    gtk::UriLauncher::new(url).launch(parent.as_ref(), gio::Cancellable::NONE, move |result| {
        if let Err(err) = result {
            log::warn!("Portal konnte nicht geöffnet werden: {err}");
        }
    });
}

/// Untertitel eines Landes: Anzahl der Gesetze, davon installiert, Quelle.
fn state_subtitle(total: usize, installed: usize, site: &str) -> String {
    let count =
        ngettext("{n} Gesetz", "{n} Gesetze", total as u32).replace("{n}", &total.to_string());
    let mut parts = vec![count];
    if installed > 0 {
        parts.push(gettext("{n} installiert").replace("{n}", &installed.to_string()));
    }
    parts.push(site.to_owned());
    parts.join(" · ")
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

    #[test]
    fn state_subtitle_counts() {
        assert_eq!(state_subtitle(1, 0, "x"), "1 Gesetz · x");
        assert_eq!(
            state_subtitle(14, 2, "gesetze-bayern.de"),
            "14 Gesetze · 2 installiert · gesetze-bayern.de"
        );
    }
}
