// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Hauptfenster: Erststart mit Download-Angebot, Fortschrittsanzeige während
//! des Imports und Prüfung auf neue Gesetzesfassungen.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib, CompositeTemplate};

use crate::db::Database;
use crate::importer::{self, ImportError, ImportReport, Progress, UpdateCheck, DEFAULT_SLUG};
use crate::model::LawInfo;
use crate::settings;

/// Mindestabstand zwischen zwei automatischen Aktualisierungsprüfungen.
const UPDATE_CHECK_INTERVAL_HOURS: i64 = 24;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/window.ui")]
    pub struct LexWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub empty_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub busy_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub ready_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub version_label: TemplateChild<gtk::Label>,
        pub settings: OnceCell<gio::Settings>,
        /// Läuft gerade ein Download, Import oder eine Prüfung?
        pub busy: Cell<bool>,
        /// Die installierte Fassung des Standardgesetzes, falls vorhanden.
        pub law: RefCell<Option<LawInfo>>,
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
            self.busy_page
                .set_paintable(Some(&adw::SpinnerPaintable::new(Some(&*self.busy_page))));
            obj.setup_actions();
            obj.load_state();
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
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl LexWindow {
    pub fn new(app: &impl IsA<gtk::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn settings(&self) -> &gio::Settings {
        self.imp().settings.get().expect("settings")
    }

    fn db_path() -> PathBuf {
        Database::default_path()
    }

    // ------------------------------------------------------------------
    // Aktionen und Zustand
    // ------------------------------------------------------------------

    fn setup_actions(&self) {
        let download = gio::ActionEntry::builder("download")
            .activate(|win: &Self, _, _| win.start_download())
            .build();
        let check = gio::ActionEntry::builder("check-updates")
            .activate(|win: &Self, _, _| win.start_update_check(true))
            .build();
        self.add_action_entries([download, check]);
        self.update_actions();
    }

    fn update_actions(&self) {
        let imp = self.imp();
        let busy = imp.busy.get();
        let installed = imp.law.borrow().is_some();
        for (name, enabled) in [("download", !busy), ("check-updates", !busy && installed)] {
            if let Some(action) = self.lookup_action(name) {
                if let Some(simple) = action.downcast_ref::<gio::SimpleAction>() {
                    simple.set_enabled(enabled);
                }
            }
        }
    }

    fn set_busy(&self, busy: bool, title: &str) {
        let imp = self.imp();
        imp.busy.set(busy);
        if busy {
            imp.busy_page.set_title(title);
            imp.busy_page.set_description(None);
            imp.stack.set_visible_child_name("busy");
        }
        self.update_actions();
    }

    fn set_law(&self, law: Option<LawInfo>) {
        *self.imp().law.borrow_mut() = law;
        self.update_actions();
    }

    /// Zeigt je nach Zustand die Erststart-Seite oder die Übersicht der
    /// installierten Fassung.
    fn show_state(&self) {
        let imp = self.imp();
        if imp.busy.get() {
            return;
        }
        match imp.law.borrow().as_ref() {
            Some(law) => {
                imp.ready_page.set_title(&law.title);
                imp.ready_page.set_description(Some(&describe_law(law)));
                imp.version_label.set_label(&describe_import(law));
                imp.stack.set_visible_child_name("ready");
            }
            None => imp.stack.set_visible_child_name("empty"),
        }
    }

    fn show_progress(&self, progress: Progress) {
        let text = match progress {
            Progress::Downloading { received, total } => match total {
                Some(total) if total > 0 => {
                    let percent = (received * 100 / total).min(100);
                    format!(
                        "{} {percent} % ({} von {})",
                        gettext("Lade herunter …"),
                        glib::format_size(received),
                        glib::format_size(total)
                    )
                }
                _ => format!(
                    "{} {}",
                    gettext("Lade herunter …"),
                    glib::format_size(received)
                ),
            },
            Progress::Importing => gettext("Verarbeite Gesetzestext und schreibe die Datenbank …"),
        };
        self.imp().busy_page.set_description(Some(&text));
    }

    fn toast(&self, message: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(message));
    }

    fn toast_error(&self, context: &str, err: &ImportError) {
        log::warn!("{context}: {err}");
        let toast = adw::Toast::builder()
            .title(format!("{context}: {err}"))
            .timeout(0)
            .priority(adw::ToastPriority::High)
            .build();
        self.imp().toast_overlay.add_toast(toast);
    }

    fn record_update_check(&self) {
        let now = chrono::Local::now().to_rfc3339();
        if let Err(err) = self.settings().set_string("last-update-check", &now) {
            log::warn!("last-update-check konnte nicht gespeichert werden: {err}");
        }
    }

    // ------------------------------------------------------------------
    // Abläufe
    // ------------------------------------------------------------------

    /// Liest beim Start die installierte Fassung aus der Datenbank und
    /// stößt gegebenenfalls die automatische Aktualisierungsprüfung an.
    fn load_state(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                match gio::spawn_blocking(move || importer::installed_law(&path, DEFAULT_SLUG))
                    .await
                {
                    Ok(Ok(law)) => win.set_law(law),
                    Ok(Err(err)) => {
                        win.toast_error(&gettext("Datenbank konnte nicht geöffnet werden"), &err)
                    }
                    Err(_) => win.toast_error(
                        &gettext("Datenbank konnte nicht geöffnet werden"),
                        &ImportError::Cancelled,
                    ),
                }
                win.show_state();
                if win.should_check_updates_on_start() {
                    log::info!("Automatische Aktualisierungsprüfung beim Start");
                    win.start_update_check(false);
                }
            }
        ));
    }

    /// Automatische Prüfung nur, wenn ein Gesetz installiert ist, die
    /// Einstellung aktiv ist und die letzte Prüfung lange genug zurückliegt.
    fn should_check_updates_on_start(&self) -> bool {
        if self.imp().law.borrow().is_none() {
            return false;
        }
        let settings = self.settings();
        if !settings.boolean("check-updates") {
            return false;
        }
        let last = settings.string("last-update-check");
        update_check_due(&last, chrono::Local::now(), UPDATE_CHECK_INTERVAL_HOURS)
    }

    /// Lädt das Standardgesetz herunter und importiert es (Erstimport oder
    /// Aktualisierung).
    fn start_download(&self) {
        if self.imp().busy.get() {
            return;
        }
        self.set_busy(true, &gettext("Lade herunter …"));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    importer::install_law(path.clone(), DEFAULT_SLUG, |p| win.show_progress(p))
                        .await;
                match result {
                    Ok(report) => {
                        log::info!(
                            "Import abgeschlossen: {} Normen, {} Einheiten, Stand {:?}, Build {}",
                            report.norms,
                            report.units,
                            report.stand,
                            report.builddate
                        );
                        win.record_update_check();
                        let law = gio::spawn_blocking(move || {
                            importer::installed_law(&path, DEFAULT_SLUG)
                        })
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .flatten();
                        win.set_law(law);
                        win.toast(&describe_report(&report));
                    }
                    Err(err) => win.toast_error(&gettext("Download fehlgeschlagen"), &err),
                }
                win.set_busy(false, "");
                win.show_state();
            }
        ));
    }

    /// Prüft, ob eine neuere Fassung vorliegt. Bei `manual` wird auch das
    /// Ergebnis „aktuell“ und ein Fehler als Toast gemeldet.
    fn start_update_check(&self, manual: bool) {
        if self.imp().busy.get() {
            return;
        }
        self.set_busy(true, &gettext("Prüfe auf Aktualisierung …"));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    importer::check_update(path, DEFAULT_SLUG, |p| win.show_progress(p)).await;
                match result {
                    Ok(UpdateCheck::NotInstalled) => win.set_law(None),
                    Ok(UpdateCheck::UpToDate(law)) => {
                        log::info!("Aktualisierungsprüfung: Fassung ist aktuell");
                        win.record_update_check();
                        win.set_law(Some(law));
                        if manual {
                            win.toast(&gettext("Das BGB ist auf dem neuesten Stand."));
                        }
                    }
                    Ok(UpdateCheck::Available { installed, remote }) => {
                        log::info!(
                            "Aktualisierungsprüfung: neue Fassung (Stand {:?}, Build {})",
                            remote.stand,
                            remote.builddate
                        );
                        win.record_update_check();
                        win.set_law(Some(installed));
                        let stand = remote
                            .stand
                            .clone()
                            .unwrap_or_else(|| format_builddate(&remote.builddate));
                        let toast = adw::Toast::builder()
                            .title(format!("{} {stand}", gettext("Neue Fassung des BGB:")))
                            .button_label(gettext("Aktualisieren"))
                            .action_name("win.download")
                            .timeout(0)
                            .priority(adw::ToastPriority::High)
                            .build();
                        win.imp().toast_overlay.add_toast(toast);
                    }
                    Err(err) => {
                        if manual {
                            win.toast_error(
                                &gettext("Aktualisierungsprüfung fehlgeschlagen"),
                                &err,
                            );
                        } else {
                            log::warn!("Automatische Aktualisierungsprüfung fehlgeschlagen: {err}");
                        }
                    }
                }
                win.set_busy(false, "");
                win.show_state();
            }
        ));
    }
}

// ----------------------------------------------------------------------
// Hilfsfunktionen (ohne GTK, testbar)
// ----------------------------------------------------------------------

/// Beschreibung der installierten Fassung für die Übersichtsseite.
fn describe_law(law: &LawInfo) -> String {
    let mut parts = Vec::new();
    if let Some(stand) = &law.stand {
        parts.push(format!("{} {stand}", gettext("Stand:")));
    }
    if let Some(neufassung) = &law.neufassung {
        parts.push(neufassung.clone());
    }
    if let Some(fundstelle) = &law.fundstelle {
        parts.push(format!("{} {fundstelle}", gettext("Fundstelle:")));
    }
    parts.join("\n")
}

/// Zeile mit Dokumentdatum und Importzeitpunkt.
fn describe_import(law: &LawInfo) -> String {
    format!(
        "{} {} · {} {}",
        gettext("Dokument vom"),
        format_builddate(&law.builddate),
        gettext("importiert am"),
        format_timestamp(&law.imported_at)
    )
}

/// Meldung nach einem erfolgreichen Import.
fn describe_report(report: &ImportReport) -> String {
    let norms = report.norms;
    let mut text = format!(
        "{} {}",
        report.title,
        ngettext(
            "importiert: {} Norm.",
            "importiert: {} Normen.",
            norms as u32
        )
        .replace("{}", &norms.to_string())
    );
    let r = &report.reanchored;
    if r.moved > 0 || r.orphaned > 0 || r.recovered > 0 {
        text.push(' ');
        text.push_str(
            &gettext("Annotationen: {moved} verschoben, {orphaned} verwaist, {recovered} wiedergefunden.")
                .replace("{moved}", &r.moved.to_string())
                .replace("{orphaned}", &r.orphaned.to_string())
                .replace("{recovered}", &r.recovered.to_string()),
        );
    }
    text
}

/// Wandelt das Build-Datum der XML-Datei (`JJJJMMTThhmmss`) in `TT.MM.JJJJ` um.
fn format_builddate(builddate: &str) -> String {
    match chrono::NaiveDateTime::parse_from_str(builddate, "%Y%m%d%H%M%S") {
        Ok(dt) => dt.format("%d.%m.%Y").to_string(),
        Err(_) => builddate.to_owned(),
    }
}

/// Formatiert einen RFC-3339-Zeitstempel als lokales Datum mit Uhrzeit.
fn format_timestamp(ts: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(ts) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%d.%m.%Y %H:%M")
            .to_string(),
        Err(_) => ts.to_owned(),
    }
}

/// Ist die nächste automatische Prüfung fällig? Ein leerer oder
/// unlesbarer Zeitstempel zählt als „noch nie geprüft“.
fn update_check_due(last: &str, now: chrono::DateTime<chrono::Local>, interval_hours: i64) -> bool {
    match chrono::DateTime::parse_from_rfc3339(last) {
        Ok(last) => now.signed_duration_since(last) >= chrono::Duration::hours(interval_hours),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builddate_formatting() {
        assert_eq!(format_builddate("20260910215507"), "10.09.2026");
        assert_eq!(format_builddate("kaputt"), "kaputt");
        assert_eq!(format_builddate(""), "");
    }

    #[test]
    fn update_check_interval() {
        let now = chrono::Local::now();
        assert!(update_check_due("", now, 24));
        assert!(update_check_due("gestern", now, 24));
        let recent = (now - chrono::Duration::hours(1)).to_rfc3339();
        assert!(!update_check_due(&recent, now, 24));
        let old = (now - chrono::Duration::hours(25)).to_rfc3339();
        assert!(update_check_due(&old, now, 24));
        let future = (now + chrono::Duration::hours(5)).to_rfc3339();
        assert!(!update_check_due(&future, now, 24));
    }
}
