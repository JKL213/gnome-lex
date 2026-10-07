// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Hauptfenster: Erststart mit Download-Angebot, Fortschrittsanzeige während
//! des Imports, Prüfung auf neue Gesetzesfassungen, Gliederung (Seitenleiste
//! mit Gesetzesauswahl), die Leseansicht mit optionaler Teilung und die
//! Schnellsuche.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::db::Database;
use crate::importer::SOURCES;
use crate::importer::{self, ImportError, ImportReport, Progress, UpdateCheck};
use crate::model::{Annotation, AnnotationKind, LawInfo, NormInfo, UnitInfo};
use crate::refs::{LawRef, NormRef};
use crate::settings;
use crate::widgets::{
    norm_view::AnnotationEvent, reader::PaneInfo, LexDownloadCenter, LexOutlineRow, LexQuickSearch,
    LexReader, Outline, ReaderState,
};

/// Mindestabstand zwischen zwei automatischen Aktualisierungsprüfungen.
const UPDATE_CHECK_INTERVAL_HOURS: i64 = 24;
/// Höchstzahl der Einträge im Verlauf.
const HISTORY_LIMIT: usize = 60;
/// Verzögerung, nach der eine geänderte Notiz gespeichert wird.
const NOTE_SAVE_DELAY_MS: u64 = 800;

/// Ein Eintrag im Verlauf der gelesenen Normen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub law: String,
    pub abbrev: String,
    pub norm: String,
    #[serde(default)]
    pub title: String,
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/window.ui")]
    pub struct LexWindow {
        #[template_child]
        pub breakpoint: TemplateChild<adw::Breakpoint>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub empty_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub download_buttons: TemplateChild<gtk::Box>,
        #[template_child]
        pub busy_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub law_dropdown: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub outline_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub sidebar_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub favorites_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub favorite_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub norm_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub reader: TemplateChild<LexReader>,
        #[template_child]
        pub notes_split: TemplateChild<adw::OverlaySplitView>,
        #[template_child]
        pub notes_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub notes_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub notes_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub history_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub history_popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub history_list: TemplateChild<gtk::ListBox>,
        pub settings: OnceCell<gio::Settings>,
        pub outline: OnceCell<Outline>,
        /// Läuft gerade ein Download, Import oder eine Prüfung?
        pub busy: Cell<bool>,
        /// Alle installierten Gesetze (Reihenfolge wie im Auswahlfeld).
        pub laws: RefCell<Vec<LawInfo>>,
        /// Gesetz, dessen Gliederung gezeigt wird (Slug), und seine ID.
        pub outline_slug: RefCell<String>,
        pub outline_law: Cell<i64>,
        /// Norm, die nach dem nächsten Laden der Gliederung markiert wird.
        pub pending_reveal: Cell<Option<i64>>,
        /// Schmale Darstellung (Seitenleiste eingeklappt, Teilung untereinander).
        pub narrow: Cell<bool>,
        /// Unterdrückt die Reaktion auf programmatische Auswahl im Dropdown.
        pub syncing_dropdown: Cell<bool>,
        /// Laufende Nummer der Gliederungsanfragen gegen veraltete Antworten.
        pub outline_serial: Cell<u64>,
        /// Favoriten (Lesezeichen auf Normen) aus der Datenbank.
        pub favorites: RefCell<Vec<Annotation>>,
        /// Geöffnetes Download-Center, um es nach Importen zu aktualisieren.
        pub download_center: RefCell<Option<glib::WeakRef<LexDownloadCenter>>>,
        /// Norm, zu der das Notizpanel gerade gehört: (Slug, Bezeichnung, Gesetzes-ID).
        pub note_target: RefCell<Option<(String, String, i64)>>,
        /// Das Panel wird gerade befüllt (Änderungssignal ignorieren).
        pub note_loading: Cell<bool>,
        pub note_dirty: Cell<bool>,
        pub note_timer: RefCell<Option<glib::SourceId>>,
        /// Verlauf der gelesenen Normen, neueste zuletzt.
        pub history: RefCell<Vec<HistoryEntry>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexWindow {
        const NAME: &'static str = "LexWindow";
        type Type = super::LexWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            LexReader::ensure_type();
            LexOutlineRow::ensure_type();
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
            obj.setup_download_buttons();
            obj.setup_outline();
            obj.setup_reader();
            obj.setup_font_size();
            obj.setup_breakpoint();
            obj.setup_actions();
            obj.setup_slash_shortcut();
            obj.setup_favorites();
            obj.setup_notes();
            obj.setup_history();
            obj.load_state();
        }
    }

    impl WidgetImpl for LexWindow {}

    impl WindowImpl for LexWindow {
        fn close_request(&self) -> glib::Propagation {
            self.obj().save_state_before_quit();
            self.parent_close_request()
        }
    }

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
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(slug) = param.and_then(|p| p.get::<String>()) {
                    win.start_download(&slug);
                }
            })
            .build();
        let check = gio::ActionEntry::builder("check-updates")
            .activate(|win: &Self, _, _| win.start_update_check(true))
            .build();
        let prev = gio::ActionEntry::builder("prev-norm")
            .activate(|win: &Self, _, _| win.show_neighbor(-1))
            .build();
        let next = gio::ActionEntry::builder("next-norm")
            .activate(|win: &Self, _, _| win.show_neighbor(1))
            .build();
        let zoom_in = gio::ActionEntry::builder("zoom-in")
            .activate(|win: &Self, _, _| win.change_font_size(1))
            .build();
        let zoom_out = gio::ActionEntry::builder("zoom-out")
            .activate(|win: &Self, _, _| win.change_font_size(-1))
            .build();
        let zoom_reset = gio::ActionEntry::builder("zoom-reset")
            .activate(|win: &Self, _, _| win.reset_font_size())
            .build();
        // Norm über ihre Bezeichnung („§ 433“) in der aktiven Ansicht anzeigen.
        let show = gio::ActionEntry::builder("show-norm")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(enbez) = param.and_then(|p| p.get::<String>()) {
                    win.show_norm_by_enbez(&enbez);
                }
            })
            .build();
        let split = gio::ActionEntry::builder("split")
            .state(false.to_variant())
            .activate(|win: &Self, action, _| {
                let reader = win.reader();
                reader.toggle_split();
                action.set_state(&reader.split().to_variant());
            })
            .build();
        let switch_pane = gio::ActionEntry::builder("switch-pane")
            .activate(|win: &Self, _, _| win.reader().switch_pane())
            .build();
        let open_other = gio::ActionEntry::builder("open-in-other-pane")
            .parameter_type(Some(&i64::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(id) = param.and_then(|p| p.get::<i64>()) {
                    win.reader().show_in_other_pane(id);
                }
            })
            .build();
        let quick = gio::ActionEntry::builder("quick-search")
            .activate(|win: &Self, _, _| win.open_quick_search())
            .build();
        // Nur für Tests: Seite der Seitenleiste („outline“/„favorites“).
        let sidebar_page = gio::ActionEntry::builder("sidebar-page")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(name) = param.and_then(|p| p.get::<String>()) {
                    win.imp().sidebar_stack.set_visible_child_name(&name);
                }
            })
            .build();
        // Nur für Tests: aktive Ansicht scrollen („1“ = Ende, „-1“ = vorherige Norm).
        let scroll = gio::ActionEntry::builder("scroll")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                let fraction = param
                    .and_then(|p| p.get::<String>())
                    .and_then(|s| s.trim().parse::<f64>().ok());
                if let Some(fraction) = fraction {
                    win.reader().scroll_active(fraction);
                }
            })
            .build();
        // Nur für Tests: Notiztext setzen bzw. Verlaufs-Popover öffnen.
        let set_note = gio::ActionEntry::builder("set-note")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(text) = param.and_then(|p| p.get::<String>()) {
                    win.imp()
                        .notes_view
                        .buffer()
                        .set_text(&text.replace("\\n", "\n"));
                }
            })
            .build();
        let view_test = gio::ActionEntry::builder("view-test")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(cmd) = param.and_then(|p| p.get::<String>()) {
                    win.reader().test_command(&cmd);
                }
            })
            .build();
        let close_window = gio::ActionEntry::builder("close-window")
            .activate(|win: &Self, _, _| win.close())
            .build();
        let popup_history = gio::ActionEntry::builder("popup-history")
            .activate(|win: &Self, _, _| win.imp().history_button.popup())
            .build();
        let center = gio::ActionEntry::builder("download-center")
            .activate(|win: &Self, _, _| win.open_download_center())
            .build();
        let favorite = gio::ActionEntry::builder("toggle-favorite")
            .state(false.to_variant())
            .activate(|win: &Self, _, _| win.toggle_favorite())
            .build();
        let notes = gio::ActionEntry::builder("toggle-notes")
            .state(false.to_variant())
            .activate(|win: &Self, _, _| {
                let split = &win.imp().notes_split;
                split.set_show_sidebar(!split.shows_sidebar());
            })
            .build();
        let clear_history = gio::ActionEntry::builder("clear-history")
            .activate(|win: &Self, _, _| win.clear_history())
            .build();
        // Nur für Tests: Schnellsuche mit vorbelegter Eingabe öffnen.
        let quick_query = gio::ActionEntry::builder("quick-search-query")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(q) = param.and_then(|p| p.get::<String>()) {
                    win.open_quick_search_with(Some(&q));
                }
            })
            .build();
        self.add_action_entries([
            download,
            check,
            prev,
            next,
            zoom_in,
            zoom_out,
            zoom_reset,
            show,
            split,
            switch_pane,
            open_other,
            quick,
            quick_query,
            sidebar_page,
            scroll,
            set_note,
            view_test,
            close_window,
            popup_history,
            center,
            favorite,
            notes,
            clear_history,
        ]);
        self.update_actions();
    }

    fn update_actions(&self) {
        let imp = self.imp();
        let busy = imp.busy.get();
        let installed = !imp.laws.borrow().is_empty();
        let reader = self.reader();
        let showing = installed && !busy && reader.active_norm_id().is_some();
        let split = reader.split();
        for (name, enabled) in [
            ("download", !busy),
            ("check-updates", !busy && installed),
            ("prev-norm", showing),
            ("next-norm", showing),
            ("show-norm", installed && !busy),
            ("split", installed),
            ("switch-pane", split),
            ("open-in-other-pane", installed && !busy),
            ("quick-search", installed && !busy),
            ("quick-search-query", installed && !busy),
            ("download-center", !busy),
            ("toggle-favorite", showing),
            ("toggle-notes", installed),
            ("clear-history", !imp.history.borrow().is_empty()),
        ] {
            if let Some(action) = self.lookup_action(name) {
                if let Some(simple) = action.downcast_ref::<gio::SimpleAction>() {
                    simple.set_enabled(enabled);
                }
            }
        }
        if let Some(action) = self
            .lookup_action("split")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_state(&split.to_variant());
        }
        let favorite = reader
            .active_info()
            .is_some_and(|p| self.favorite_id(&p).is_some());
        if let Some(action) = self
            .lookup_action("toggle-favorite")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_state(&favorite.to_variant());
        }
        imp.favorite_button.set_icon_name(if favorite {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        });
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

    /// Zeigt je nach Zustand die Erststart-Seite oder die Leseansicht.
    fn show_state(&self) {
        let imp = self.imp();
        if imp.busy.get() {
            return;
        }
        if imp.laws.borrow().is_empty() {
            imp.stack.set_visible_child_name("empty");
        } else {
            imp.stack.set_visible_child_name("law");
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

    /// Erststart-Seite: BGB direkt, alle weiteren Gesetze über das Download-Center.
    fn setup_download_buttons(&self) {
        let imp = self.imp();
        let bgb = gtk::Button::builder()
            .label(gettext("BGB herunterladen"))
            .halign(gtk::Align::Center)
            .action_name("win.download")
            .css_classes(["pill", "suggested-action"])
            .build();
        bgb.set_action_target_value(Some(&"bgb".to_variant()));
        imp.download_buttons.append(&bgb);
        let more = gtk::Button::builder()
            .label(gettext("Weitere Gesetze …"))
            .halign(gtk::Align::Center)
            .action_name("win.download-center")
            .css_classes(["pill"])
            .build();
        imp.download_buttons.append(&more);
    }

    fn open_download_center(&self) {
        let imp = self.imp();
        if let Some(open) = imp
            .download_center
            .borrow()
            .as_ref()
            .and_then(|w| w.upgrade())
        {
            open.present(Some(self));
            return;
        }
        let dialog = LexDownloadCenter::new();
        dialog.refresh(&imp.laws.borrow());
        *imp.download_center.borrow_mut() = Some(dialog.downgrade());
        dialog.present(Some(self));
    }

    /// Bei Schmalbreite liegen die Ansichten eines geteilten Tabs untereinander.
    fn setup_breakpoint(&self) {
        let imp = self.imp();
        imp.breakpoint.connect_apply(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.set_narrow(true)
        ));
        imp.breakpoint.connect_unapply(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.set_narrow(false)
        ));
    }

    fn set_narrow(&self, narrow: bool) {
        self.imp().narrow.set(narrow);
        self.reader().set_vertical(narrow);
    }

    /// „/“ öffnet die Schnellsuche, solange kein Eingabefeld den Fokus hat.
    fn setup_slash_shortcut(&self) {
        let controller = gtk::ShortcutController::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let action = gtk::CallbackAction::new(|widget, _| {
            let Some(win) = widget.downcast_ref::<LexWindow>() else {
                return glib::Propagation::Proceed;
            };
            let focus = GtkWindowExt::focus(win);
            let in_entry = focus.as_ref().is_some_and(|f| {
                f.is::<gtk::Text>()
                    || f.is::<gtk::Editable>()
                    || f.downcast_ref::<gtk::TextView>()
                        .is_some_and(|tv| tv.is_editable())
            });
            if in_entry {
                return glib::Propagation::Proceed;
            }
            if win
                .lookup_action("quick-search")
                .is_some_and(|a| a.is_enabled())
            {
                win.open_quick_search();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let trigger = gtk::KeyvalTrigger::new(gdk::Key::slash, gdk::ModifierType::empty());
        controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
        self.add_controller(controller);
    }

    // ------------------------------------------------------------------
    // Abläufe
    // ------------------------------------------------------------------

    /// Liest beim Start die installierten Gesetze, stellt die Tabs wieder
    /// her und stößt gegebenenfalls die Aktualisierungsprüfung an.
    fn load_state(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                match gio::spawn_blocking(move || -> Result<Vec<LawInfo>, ImportError> {
                    Ok(Database::open(&path)?.laws()?)
                })
                .await
                {
                    Ok(Ok(laws)) => win.set_laws(laws),
                    Ok(Err(err)) => {
                        win.toast_error(&gettext("Datenbank konnte nicht geöffnet werden"), &err)
                    }
                    Err(_) => win.toast_error(
                        &gettext("Datenbank konnte nicht geöffnet werden"),
                        &ImportError::Cancelled,
                    ),
                }
                win.show_state();
                win.load_favorites();
                if !win.imp().laws.borrow().is_empty() {
                    win.restore_reader_state();
                }
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
        if self.imp().laws.borrow().is_empty() {
            return false;
        }
        let settings = self.settings();
        if !settings.boolean("check-updates") {
            return false;
        }
        let last = settings.string("last-update-check");
        update_check_due(&last, chrono::Local::now(), UPDATE_CHECK_INTERVAL_HOURS)
    }

    /// Lädt das Gesetz `slug` herunter und importiert es (Erstimport oder
    /// Aktualisierung).
    fn start_download(&self, slug: &str) {
        if self.imp().busy.get() {
            return;
        }
        let Some(source) = importer::Source::by_slug(slug) else {
            self.toast_error(
                &gettext("Download fehlgeschlagen"),
                &ImportError::UnknownSource(slug.to_owned()),
            );
            return;
        };
        self.set_busy(
            true,
            &gettext("Lade {name} herunter …").replace("{name}", source.name),
        );
        let slug = slug.to_owned();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    importer::install_law(path.clone(), &slug, |p| win.show_progress(p)).await;
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
                        let laws = gio::spawn_blocking(move || Database::open(&path)?.laws())
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .unwrap_or_default();
                        win.toast(&describe_report(&report));
                        win.set_busy(false, "");
                        win.set_laws(laws);
                        win.show_state();
                        win.reload_after_import(&slug, report.law_id);
                        return;
                    }
                    Err(err) => win.toast_error(&gettext("Download fehlgeschlagen"), &err),
                }
                win.set_busy(false, "");
                win.show_state();
            }
        ));
    }

    /// Prüft alle installierten Gesetze nacheinander auf neuere Fassungen.
    /// Bei `manual` wird auch „aktuell“ und ein Fehler als Toast gemeldet.
    fn start_update_check(&self, manual: bool) {
        if self.imp().busy.get() {
            return;
        }
        let laws: Vec<LawInfo> = self.imp().laws.borrow().clone();
        if laws.is_empty() {
            return;
        }
        self.set_busy(true, &gettext("Prüfe auf Aktualisierung …"));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let mut up_to_date = Vec::new();
                for law in laws {
                    let path = Self::db_path();
                    let result =
                        importer::check_update(path, &law.slug, |p| win.show_progress(p)).await;
                    match result {
                        Ok(UpdateCheck::NotInstalled) => {}
                        Ok(UpdateCheck::UpToDate(law)) => {
                            log::info!("Aktualisierungsprüfung {}: aktuell", law.jurabk);
                            win.record_update_check();
                            up_to_date.push(law.jurabk);
                        }
                        Ok(UpdateCheck::Available { installed, remote }) => {
                            log::info!(
                                "Aktualisierungsprüfung {}: neue Fassung (Stand {:?}, Build {})",
                                installed.jurabk,
                                remote.stand,
                                remote.builddate
                            );
                            win.record_update_check();
                            let stand = remote
                                .stand
                                .clone()
                                .unwrap_or_else(|| format_builddate(&remote.builddate));
                            let toast = adw::Toast::builder()
                                .title(
                                    gettext("Neue Fassung: {law}, {stand}")
                                        .replace("{law}", &installed.jurabk)
                                        .replace("{stand}", &stand),
                                )
                                .button_label(gettext("Aktualisieren"))
                                .action_name("win.download")
                                .timeout(0)
                                .priority(adw::ToastPriority::High)
                                .build();
                            toast.set_action_target_value(Some(&installed.slug.to_variant()));
                            win.imp().toast_overlay.add_toast(toast);
                        }
                        Err(err) => {
                            if manual {
                                win.toast_error(
                                    &gettext("Aktualisierungsprüfung fehlgeschlagen"),
                                    &err,
                                );
                            } else {
                                log::warn!(
                                    "Automatische Aktualisierungsprüfung {} fehlgeschlagen: {err}",
                                    law.jurabk
                                );
                            }
                        }
                    }
                }
                if manual && !up_to_date.is_empty() {
                    win.toast(
                        &gettext("Auf dem neuesten Stand: {laws}")
                            .replace("{laws}", &up_to_date.join(", ")),
                    );
                }
                win.set_busy(false, "");
                win.show_state();
            }
        ));
    }

    // ------------------------------------------------------------------
    // Gesetze und Gliederung
    // ------------------------------------------------------------------

    fn outline(&self) -> &Outline {
        self.imp().outline.get().expect("outline")
    }

    /// Übernimmt die Liste installierter Gesetze in Auswahlfeld und Zustand.
    /// Die Gliederung bleibt beim bisherigen Gesetz, falls es noch existiert.
    fn set_laws(&self, laws: Vec<LawInfo>) {
        let imp = self.imp();
        let names: Vec<String> = laws.iter().map(|l| l.jurabk.clone()).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let previous = imp.outline_slug.borrow().clone();
        *imp.laws.borrow_mut() = laws;
        imp.syncing_dropdown.set(true);
        imp.law_dropdown
            .set_model(Some(&gtk::StringList::new(&name_refs)));
        let index = imp
            .laws
            .borrow()
            .iter()
            .position(|l| l.slug == previous)
            .unwrap_or(0) as u32;
        imp.law_dropdown.set_selected(index);
        imp.syncing_dropdown.set(false);
        if let Some(center) = imp
            .download_center
            .borrow()
            .as_ref()
            .and_then(|w| w.upgrade())
        {
            center.refresh(&imp.laws.borrow());
        }
        let chosen = imp.laws.borrow().get(index as usize).cloned();
        match chosen {
            Some(law) if law.slug != previous || law.id != imp.outline_law.get() => {
                self.load_outline(&law, None)
            }
            Some(_) => {}
            None => {
                *imp.outline_slug.borrow_mut() = String::new();
                imp.outline_law.set(0);
                self.outline().set_data(&[], &[]);
            }
        }
        self.update_actions();
    }

    fn setup_outline(&self) {
        let imp = self.imp();
        let outline = Outline::new();
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
                item.set_child(Some(&LexOutlineRow::new()));
            }
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let row = item.item().and_downcast::<gtk::TreeListRow>();
            if let Some(child) = item.child().and_downcast::<LexOutlineRow>() {
                child.bind(row.as_ref());
            }
        });
        factory.connect_unbind(|_, item| {
            if let Some(child) = item
                .downcast_ref::<gtk::ListItem>()
                .and_then(|i| i.child())
                .and_downcast::<LexOutlineRow>()
            {
                child.bind(None);
            }
        });
        imp.outline_view.set_factory(Some(&factory));
        imp.outline_view.set_model(Some(outline.selection()));
        imp.outline_view.connect_activate(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, position| win.activate_outline_row(position)
        ));
        imp.outline.set(outline).ok();
        imp.law_dropdown.connect_selected_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |dropdown| {
                let imp = win.imp();
                if imp.syncing_dropdown.get() {
                    return;
                }
                let law = imp.laws.borrow().get(dropdown.selected() as usize).cloned();
                if let Some(law) = law {
                    // Zeigt die Norm der aktiven Ansicht, falls sie zu diesem Gesetz gehört.
                    let reveal = win
                        .reader()
                        .active_info()
                        .filter(|p| p.law_slug == law.slug)
                        .map(|p| p.norm.id);
                    win.load_outline(&law, reveal);
                }
            }
        ));
    }

    /// Liest Gliederung und Normenliste eines Gesetzes aus der Datenbank und
    /// markiert anschließend `reveal` (oder die bereits vorgemerkte Norm).
    fn load_outline(&self, law: &LawInfo, reveal: Option<i64>) {
        let imp = self.imp();
        *imp.outline_slug.borrow_mut() = law.slug.clone();
        imp.outline_law.set(law.id);
        if reveal.is_some() {
            imp.pending_reveal.set(reveal);
        }
        let serial = imp.outline_serial.get().wrapping_add(1);
        imp.outline_serial.set(serial);
        let law_id = law.id;
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    gio::spawn_blocking(move || -> Result<OutlineData, rusqlite::Error> {
                        let db = Database::open(&path)?;
                        Ok(OutlineData {
                            units: db.units(law_id)?,
                            norms: db.norm_infos(law_id)?,
                        })
                    })
                    .await;
                if win.imp().outline_serial.get() != serial {
                    return;
                }
                match result {
                    Ok(Ok(data)) => {
                        let norms: Vec<NormInfo> =
                            data.norms.into_iter().filter(is_readable_norm).collect();
                        win.outline().set_data(&data.units, &norms);
                        let pending = win.imp().pending_reveal.take();
                        let target = pending.or_else(|| {
                            win.reader()
                                .active_info()
                                .filter(|p| p.norm.law_id == law_id)
                                .map(|p| p.norm.id)
                        });
                        if let Some(id) = target {
                            win.reveal_in_outline(id);
                        }
                    }
                    Ok(Err(err)) => win.toast_error(
                        &gettext("Gliederung konnte nicht geladen werden"),
                        &ImportError::Db(err),
                    ),
                    Err(_) => win.toast_error(
                        &gettext("Gliederung konnte nicht geladen werden"),
                        &ImportError::Cancelled,
                    ),
                }
            }
        ));
    }

    /// Markiert eine Norm in der Gliederung (klappt den Pfad auf).
    fn reveal_in_outline(&self, norm_id: i64) {
        let imp = self.imp();
        if let Some(position) = self.outline().reveal_norm(norm_id) {
            self.outline().selection().set_selected(position);
            imp.outline_view
                .scroll_to(position, gtk::ListScrollFlags::NONE, None);
        } else {
            self.outline()
                .selection()
                .set_selected(gtk::INVALID_LIST_POSITION);
        }
    }

    /// Lässt die Gliederung der Norm der aktiven Ansicht folgen, notfalls
    /// mit Wechsel des Gesetzes im Auswahlfeld.
    fn sync_outline(&self, info: &PaneInfo) {
        let imp = self.imp();
        if info.law_slug == *imp.outline_slug.borrow() && info.norm.law_id == imp.outline_law.get()
        {
            self.reveal_in_outline(info.norm.id);
            return;
        }
        let found = imp
            .laws
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, l)| l.slug == info.law_slug)
            .map(|(i, l)| (i as u32, l.clone()));
        if let Some((index, law)) = found {
            imp.syncing_dropdown.set(true);
            imp.law_dropdown.set_selected(index);
            imp.syncing_dropdown.set(false);
            self.load_outline(&law, Some(info.norm.id));
        }
    }

    /// Klick oder Eingabetaste auf eine Zeile der Gliederung: Einheiten
    /// auf- bzw. zuklappen, Normen in der aktiven Ansicht anzeigen.
    fn activate_outline_row(&self, position: u32) {
        let Some((row, item)) = self.outline().item_at(position) else {
            return;
        };
        if item.is_unit() {
            row.set_expanded(!row.is_expanded());
            return;
        }
        self.show_norm_in_active(item.id());
        self.reveal_content();
    }

    /// Bei eingeklappter Seitenleiste (Schmalbreite) zur Inhaltsseite wechseln.
    fn reveal_content(&self) {
        let imp = self.imp();
        if imp.split_view.is_collapsed() {
            imp.split_view.set_show_content(true);
        }
    }

    fn setup_font_size(&self) {
        let settings = self.settings();
        settings.connect_changed(
            Some("font-size"),
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |settings, _| {
                    win.reader().set_font_size(settings.int("font-size"));
                }
            ),
        );
    }

    fn change_font_size(&self, delta: i32) {
        let settings = self.settings();
        let size = (settings.int("font-size") + delta).clamp(8, 40);
        if let Err(err) = settings.set_int("font-size", size) {
            log::warn!("font-size konnte nicht gespeichert werden: {err}");
        }
    }

    fn reset_font_size(&self) {
        self.settings().reset("font-size");
    }

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------

    fn reader(&self) -> &LexReader {
        &self.imp().reader
    }

    /// Verbindet die Leseansicht mit Kopfleiste, Gliederung, Verlauf, Notizen.
    fn setup_reader(&self) {
        let reader = self.reader();
        reader.set_font_size(self.settings().int("font-size"));
        reader.connect_title_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_header()
        ));
        reader.connect_subtitle_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_header()
        ));
        reader.connect_split_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_actions()
        ));
        reader.connect_norm_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |reader, _| {
                win.update_header();
                let info = reader.active_info();
                if let Some(info) = &info {
                    win.sync_outline(info);
                    win.record_history(info);
                }
                win.sync_notes_panel(info.as_ref());
                win.update_actions();
            }
        ));
        reader.connect_navigate(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, pane, target, other_pane| win.navigate_reference(pane, target, other_pane)
        ));
        reader.connect_annotation(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, pane, event| win.on_annotation_event(pane, event)
        ));
    }

    /// Klick auf einen Verweis im Text: Gesetz und Norm auflösen und
    /// anzeigen (Strg+Klick in der zweiten Ansicht).
    fn navigate_reference(&self, pane: u32, target: NormRef, other_pane: bool) {
        let Some(info) = self.reader().pane_info(pane) else {
            return;
        };
        let law_key = match &target.law {
            LawRef::Same => LawKey::Slug(info.law_slug.clone()),
            LawRef::Abbrev(a) => LawKey::Abbrev(a.clone()),
            LawRef::Unknown => return,
        };
        let candidates = target.enbez_candidates();
        if candidates.is_empty() {
            return;
        }
        let label = target.label();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let key = law_key.clone();
                let result = gio::spawn_blocking(move || -> Result<RefLookup, rusqlite::Error> {
                    let db = Database::open(&path)?;
                    let law = match &key {
                        LawKey::Slug(slug) => db.law_by_slug(slug)?,
                        LawKey::Abbrev(abbrev) => db.law_by_abbrev(abbrev)?,
                    };
                    let Some(law) = law else {
                        return Ok(RefLookup::LawMissing);
                    };
                    for candidate in &candidates {
                        if let Some(id) = db.norm_id_by_enbez(law.id, candidate)? {
                            return Ok(RefLookup::Found(id));
                        }
                    }
                    Ok(RefLookup::NormMissing)
                })
                .await;
                match result {
                    Ok(Ok(RefLookup::Found(id))) => {
                        if other_pane {
                            win.reader().show_in_other_pane(id);
                        } else {
                            win.reader().show_norm_in_pane(id, pane);
                        }
                    }
                    Ok(Ok(RefLookup::NormMissing)) => win.toast(
                        &gettext("Verweis {label} wurde nicht gefunden.")
                            .replace("{label}", &label),
                    ),
                    Ok(Ok(RefLookup::LawMissing)) => win.offer_law_download(&law_key),
                    _ => log::warn!("Verweis {label} konnte nicht aufgelöst werden"),
                }
            }
        ));
    }

    /// Toast für ein nicht installiertes Gesetz, mit Download-Angebot, wenn
    /// eine passende Quelle bekannt ist.
    fn offer_law_download(&self, key: &LawKey) {
        let abbrev = match key {
            LawKey::Abbrev(a) => a.clone(),
            LawKey::Slug(s) => s.to_uppercase(),
        };
        let source = SOURCES.iter().find(|s| {
            s.abbrev.eq_ignore_ascii_case(&abbrev) || s.slug.eq_ignore_ascii_case(&abbrev)
        });
        let toast = adw::Toast::builder()
            .title(gettext("{law} ist nicht installiert.").replace("{law}", &abbrev))
            .timeout(6)
            .build();
        if let Some(source) = source {
            toast.set_button_label(Some(&gettext("Herunterladen")));
            toast.set_action_name(Some("win.download"));
            toast.set_action_target_value(Some(&source.slug.to_variant()));
        }
        self.imp().toast_overlay.add_toast(toast);
    }

    // ------------------------------------------------------------------
    // Markierungen und angeheftete Notizen (Persistenz)
    // ------------------------------------------------------------------

    /// Speichert eine Annotation und spiegelt sie in beide Ansichten.
    fn on_annotation_event(&self, pane: u32, event: AnnotationEvent) {
        let Some(law_id) = self.reader().pane_info(pane).map(|p| p.norm.law_id) else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let job = event.clone();
                let result =
                    gio::spawn_blocking(move || -> Result<AnnotationEvent, rusqlite::Error> {
                        let db = Database::open(&path)?;
                        Ok(match job {
                            AnnotationEvent::Create(mut a) => {
                                a.id = db.insert_annotation(&a)?;
                                AnnotationEvent::Create(a)
                            }
                            AnnotationEvent::Update(a) => {
                                db.update_annotation(&a)?;
                                AnnotationEvent::Update(a)
                            }
                            AnnotationEvent::Delete(id) => {
                                db.delete_annotation(id)?;
                                AnnotationEvent::Delete(id)
                            }
                        })
                    })
                    .await;
                match result {
                    Ok(Ok(AnnotationEvent::Create(a))) | Ok(Ok(AnnotationEvent::Update(a))) => {
                        win.reader().apply_annotation(law_id, &a);
                    }
                    Ok(Ok(AnnotationEvent::Delete(id))) => win.reader().remove_annotation(id),
                    Ok(Err(err)) => win.toast_error(
                        &gettext("Annotation konnte nicht gespeichert werden"),
                        &ImportError::Db(err),
                    ),
                    Err(_) => {}
                }
            }
        ));
    }

    // ------------------------------------------------------------------
    // Verlauf
    // ------------------------------------------------------------------

    fn setup_history(&self) {
        let imp = self.imp();
        let placeholder = gtk::Label::builder()
            .label(gettext("Noch keine gelesenen Normen."))
            .margin_top(12)
            .margin_bottom(12)
            .css_classes(["dim-label"])
            .build();
        imp.history_list.set_placeholder(Some(&placeholder));
        imp.history_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| {
                let imp = win.imp();
                imp.history_popover.popdown();
                let index = row.index().max(0) as usize;
                let entry = {
                    let history = imp.history.borrow();
                    // Die Liste zeigt die neuesten Einträge zuerst.
                    history
                        .len()
                        .checked_sub(index + 1)
                        .and_then(|i| history.get(i).cloned())
                };
                if let Some(entry) = entry {
                    win.open_favorite(&entry.law, &entry.norm);
                }
            }
        ));
        let stored: Vec<HistoryEntry> = self
            .settings()
            .strv("history")
            .iter()
            .filter_map(|s| serde_json::from_str(s.as_str()).ok())
            .collect();
        *imp.history.borrow_mut() = stored;
        self.rebuild_history_list();
    }

    /// Merkt sich eine gelesene Norm (keine direkten Wiederholungen).
    fn record_history(&self, info: &PaneInfo) {
        let Some(enbez) = info.norm.enbez.clone() else {
            return;
        };
        let entry = HistoryEntry {
            law: info.law_slug.clone(),
            abbrev: info.law_abbrev.clone(),
            norm: enbez,
            title: info.norm.titel.clone().unwrap_or_default(),
        };
        {
            let mut history = self.imp().history.borrow_mut();
            if history.last() == Some(&entry) {
                return;
            }
            history.retain(|e| e.law != entry.law || e.norm != entry.norm);
            history.push(entry);
            let excess = history.len().saturating_sub(HISTORY_LIMIT);
            if excess > 0 {
                history.drain(..excess);
            }
        }
        self.rebuild_history_list();
    }

    fn rebuild_history_list(&self) {
        let imp = self.imp();
        imp.history_list.remove_all();
        for entry in imp.history.borrow().iter().rev() {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&format!(
                    "{} {}",
                    entry.norm, entry.abbrev
                )))
                .subtitle(glib::markup_escape_text(&entry.title))
                .activatable(true)
                .build();
            imp.history_list.append(&row);
        }
        self.update_actions();
    }

    fn clear_history(&self) {
        self.imp().history.borrow_mut().clear();
        self.rebuild_history_list();
    }

    fn save_history(&self) {
        let entries: Vec<String> = self
            .imp()
            .history
            .borrow()
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .collect();
        let refs: Vec<&str> = entries.iter().map(String::as_str).collect();
        if let Err(err) = self.settings().set_strv("history", refs.as_slice()) {
            log::warn!("history konnte nicht gespeichert werden: {err}");
        }
    }

    /// Vor dem Beenden: Tabs, Verlauf und eine offene Notiz sichern.
    pub fn save_state_before_quit(&self) {
        self.flush_note();
        self.reader().flush_note_edits();
        self.save_history();
        self.save_reader_state();
    }

    // ------------------------------------------------------------------
    // Notizen (Schema) zur Norm
    // ------------------------------------------------------------------

    fn setup_notes(&self) {
        let imp = self.imp();
        imp.notes_view.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let imp = win.imp();
                if imp.note_loading.get() {
                    return;
                }
                imp.note_dirty.set(true);
                if let Some(id) = imp.note_timer.borrow_mut().take() {
                    id.remove();
                }
                let id = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(NOTE_SAVE_DELAY_MS),
                    glib::clone!(
                        #[weak]
                        win,
                        move || {
                            win.imp().note_timer.borrow_mut().take();
                            win.flush_note();
                        }
                    ),
                );
                *imp.note_timer.borrow_mut() = Some(id);
            }
        ));
        imp.notes_split.connect_show_sidebar_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |split| {
                let shown = split.shows_sidebar();
                if let Some(action) = win
                    .lookup_action("toggle-notes")
                    .and_downcast::<gio::SimpleAction>()
                {
                    action.set_state(&shown.to_variant());
                }
                if shown {
                    let info = win.reader().active_info();
                    win.sync_notes_panel(info.as_ref());
                    win.imp().notes_view.grab_focus();
                } else {
                    win.flush_note();
                }
            }
        ));
    }

    /// Lädt die Notiz der aktiven Norm ins Panel (nur bei Wechsel der Norm).
    fn sync_notes_panel(&self, info: Option<&PaneInfo>) {
        let imp = self.imp();
        let target = info.and_then(|p| {
            p.norm
                .enbez
                .clone()
                .map(|e| (p.law_slug.clone(), e, p.norm.law_id))
        });
        if *imp.note_target.borrow() == target {
            return;
        }
        self.flush_note();
        *imp.note_target.borrow_mut() = target.clone();
        let Some((slug, enbez, _)) = target else {
            imp.notes_title.set_subtitle("");
            self.set_note_text("");
            imp.notes_view.set_sensitive(false);
            return;
        };
        imp.notes_view.set_sensitive(true);
        imp.notes_title.set_subtitle(&format!(
            "{enbez} {}",
            info.map(|p| p.law_abbrev.as_str()).unwrap_or("")
        ));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let (lookup_slug, lookup_enbez) = (slug.clone(), enbez.clone());
                let result = gio::spawn_blocking(move || {
                    Database::open(&path)?.norm_note(&lookup_slug, &lookup_enbez)
                })
                .await;
                let still_current = win
                    .imp()
                    .note_target
                    .borrow()
                    .as_ref()
                    .is_some_and(|(s, e, _)| *s == slug && *e == enbez);
                if !still_current {
                    return;
                }
                let text = match result {
                    Ok(Ok(Some(a))) => a.note.unwrap_or_default(),
                    _ => String::new(),
                };
                win.set_note_text(&text);
            }
        ));
    }

    fn set_note_text(&self, text: &str) {
        let imp = self.imp();
        imp.note_loading.set(true);
        imp.notes_view.buffer().set_text(text);
        imp.note_loading.set(false);
        imp.note_dirty.set(false);
    }

    /// Speichert eine geänderte Notiz sofort und aktualisiert die Anzeige
    /// in allen Tabs.
    fn flush_note(&self) {
        let imp = self.imp();
        if let Some(id) = imp.note_timer.borrow_mut().take() {
            id.remove();
        }
        if !imp.note_dirty.get() {
            return;
        }
        imp.note_dirty.set(false);
        let Some((slug, enbez, law_id)) = imp.note_target.borrow().clone() else {
            return;
        };
        let buffer = imp.notes_view.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        // Kurzer synchroner Schreibzugriff, damit die Reihenfolge der
        // Speichervorgänge sicher ist (auch beim Beenden).
        match Database::open(&Self::db_path()).and_then(|db| db.set_norm_note(&slug, &enbez, &text))
        {
            Ok(()) => {
                let shown = (!text.trim().is_empty()).then_some(text.trim_end());
                self.reader().set_note(law_id, &enbez, shown);
            }
            Err(err) => self.toast_error(
                &gettext("Notiz konnte nicht gespeichert werden"),
                &ImportError::Db(err),
            ),
        }
    }

    fn update_header(&self) {
        let imp = self.imp();
        let reader = self.reader();
        imp.norm_title.set_title(&reader.title());
        imp.norm_title.set_subtitle(&reader.subtitle());
    }

    /// Zeigt eine Norm in der aktiven Ansicht.
    pub fn show_norm_in_active(&self, norm_id: i64) {
        self.reader().show_norm(norm_id);
    }

    /// Öffnet die erste Norm des Gliederungsgesetzes.
    fn open_first_norm(&self) {
        let law_id = self.imp().outline_law.get();
        if law_id == 0 {
            return;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    gio::spawn_blocking(move || Database::open(&path)?.first_norm_id(law_id)).await;
                if let Ok(Ok(Some(id))) = result {
                    win.reader().show_norm(id);
                }
            }
        ));
    }

    /// Zeigt den ersten Treffer der Schnellsuche für `query` („§ 433“,
    /// „253 zpo“) in der aktiven Ansicht; bevorzugt wird das Gesetz der aktiven Ansicht.
    pub fn show_norm_by_enbez(&self, query: &str) {
        let preferred = self.preferred_law();
        let query = query.trim().to_owned();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let lookup = query.clone();
                let result = gio::spawn_blocking(move || {
                    Database::open(&path)?.quick_search(&lookup, preferred, 1)
                })
                .await;
                match result {
                    Ok(Ok(hits)) if !hits.is_empty() => {
                        win.show_norm_in_active(hits[0].norm.id);
                        win.reveal_content();
                    }
                    Ok(Ok(_)) => win.toast(
                        &gettext("Norm {enbez} wurde nicht gefunden.").replace("{enbez}", &query),
                    ),
                    _ => log::warn!("Norm {query} konnte nicht gesucht werden"),
                }
            }
        ));
    }

    /// Gesetz der aktiven Ansicht, sonst das der Gliederung.
    fn preferred_law(&self) -> Option<i64> {
        self.reader()
            .active_info()
            .map(|p| p.norm.law_id)
            .or_else(|| Some(self.imp().outline_law.get()).filter(|id| *id != 0))
    }

    /// Vorherige (`-1`) oder nächste (`+1`) Norm in Dokumentreihenfolge.
    fn show_neighbor(&self, direction: i64) {
        let Some(current) = self.reader().active_norm_id() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    gio::spawn_blocking(move || -> Result<Option<i64>, rusqlite::Error> {
                        Database::open(&path)?.neighbor_norm(current, direction)
                    })
                    .await;
                if let Ok(Ok(Some(id))) = result {
                    win.reader().show_norm(id);
                }
            }
        ));
    }

    /// Nach einem Import sind die Norm-IDs des Gesetzes neu: Ansichten mit
    /// diesem Gesetz laden ihre Norm über die Bezeichnung nach, die
    /// Gliederung wird neu gelesen. Ohne Norm wird die erste geöffnet.
    fn reload_after_import(&self, slug: &str, law_id: i64) {
        let imp = self.imp();
        let law = imp.laws.borrow().iter().find(|l| l.slug == slug).cloned();
        let Some(law) = law else {
            return;
        };
        if *imp.outline_slug.borrow() == slug {
            self.load_outline(&law, None);
        }
        let reader = self.reader().clone();
        if reader.active_norm_id().is_none() {
            self.open_first_norm();
            return;
        }
        for (pane, enbez) in reader.panes_showing(slug) {
            let reader = reader.clone();
            glib::spawn_future_local(async move {
                let path = Self::db_path();
                let lookup = enbez.clone();
                let result = gio::spawn_blocking(move || {
                    Database::open(&path)?.norm_id_by_enbez(law_id, &lookup)
                })
                .await;
                match result {
                    Ok(Ok(Some(id))) => reader.show_norm_in_pane(id, pane),
                    _ => log::info!("{enbez} nach dem Import nicht mehr gefunden"),
                }
            });
        }
    }

    // ------------------------------------------------------------------
    // Favoriten (Lesezeichen auf Normen, Annotationen der Art „bookmark“)
    // ------------------------------------------------------------------

    fn setup_favorites(&self) {
        let imp = self.imp();
        let placeholder = gtk::Label::builder()
            .label(gettext(
                "Noch keine Favoriten. Stern in der Kopfleiste oder Strg+D.",
            ))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .margin_top(24)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["dim-label"])
            .build();
        imp.favorites_list.set_placeholder(Some(&placeholder));
        imp.favorites_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| {
                let index = row.index().max(0) as usize;
                let target = win
                    .imp()
                    .favorites
                    .borrow()
                    .get(index)
                    .map(|a| (a.law.clone(), a.norm.clone()));
                if let Some((law, norm)) = target {
                    win.open_favorite(&law, &norm);
                }
            }
        ));
    }

    /// ID des Favoriten für eine Ansicht, falls vorhanden.
    fn favorite_id(&self, info: &PaneInfo) -> Option<i64> {
        let enbez = info.norm.enbez.as_deref()?;
        self.imp()
            .favorites
            .borrow()
            .iter()
            .find(|a| a.law == info.law_slug && a.norm == enbez)
            .map(|a| a.id)
    }

    fn load_favorites(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    gio::spawn_blocking(move || Database::open(&path)?.all_annotations()).await;
                if let Ok(Ok(all)) = result {
                    let favorites = all
                        .into_iter()
                        .filter(|a| a.kind == AnnotationKind::Bookmark)
                        .collect();
                    win.set_favorites(favorites);
                }
            }
        ));
    }

    fn set_favorites(&self, favorites: Vec<Annotation>) {
        let imp = self.imp();
        imp.favorites_list.remove_all();
        for fav in &favorites {
            let abbrev = imp
                .laws
                .borrow()
                .iter()
                .find(|l| l.slug == fav.law)
                .map(|l| l.jurabk.clone())
                .unwrap_or_else(|| fav.law.to_uppercase());
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&format!("{} {abbrev}", fav.norm)))
                .subtitle(glib::markup_escape_text(fav.note.as_deref().unwrap_or("")))
                .activatable(!fav.orphaned)
                .build();
            if fav.orphaned {
                row.set_subtitle(&gettext("Nicht mehr vorhanden"));
                row.add_css_class("dim-label");
            }
            let remove = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(gettext("Favorit entfernen"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let id = fav.id;
            remove.connect_clicked(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| win.remove_favorite(id)
            ));
            row.add_suffix(&remove);
            imp.favorites_list.append(&row);
        }
        *imp.favorites.borrow_mut() = favorites;
        self.update_actions();
    }

    /// Stern: Favorit der aktiven Ansicht anlegen oder entfernen.
    fn toggle_favorite(&self) {
        let Some(info) = self.reader().active_info() else {
            return;
        };
        if let Some(id) = self.favorite_id(&info) {
            self.remove_favorite(id);
            return;
        }
        let Some(enbez) = info.norm.enbez.clone() else {
            return;
        };
        let annotation = Annotation {
            id: 0,
            law: info.law_slug.clone(),
            norm: enbez,
            paragraph: 0,
            start: 0,
            end: 0,
            quote: String::new(),
            kind: AnnotationKind::Bookmark,
            color: None,
            note: info.norm.titel.clone(),
            target: None,
            created: String::new(),
            modified: String::new(),
            orphaned: false,
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result = gio::spawn_blocking(move || {
                    Database::open(&path)?.insert_annotation(&annotation)
                })
                .await;
                match result {
                    Ok(Ok(_)) => win.load_favorites(),
                    Ok(Err(err)) => win.toast_error(
                        &gettext("Favorit konnte nicht gespeichert werden"),
                        &ImportError::Db(err),
                    ),
                    Err(_) => {}
                }
            }
        ));
    }

    fn remove_favorite(&self, id: i64) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let result =
                    gio::spawn_blocking(move || Database::open(&path)?.delete_annotation(id)).await;
                match result {
                    Ok(Ok(())) => win.load_favorites(),
                    Ok(Err(err)) => win.toast_error(
                        &gettext("Favorit konnte nicht entfernt werden"),
                        &ImportError::Db(err),
                    ),
                    Err(_) => {}
                }
            }
        ));
    }

    /// Öffnet einen Favoriten (Gesetz-Slug und Bezeichnung) in der aktiven Ansicht.
    fn open_favorite(&self, law_slug: &str, enbez: &str) {
        let law_slug = law_slug.to_owned();
        let enbez = enbez.to_owned();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let lookup = enbez.clone();
                let result =
                    gio::spawn_blocking(move || -> Result<Option<i64>, rusqlite::Error> {
                        let db = Database::open(&path)?;
                        match db.law_by_slug(&law_slug)? {
                            Some(law) => db.norm_id_by_enbez(law.id, &lookup),
                            None => Ok(None),
                        }
                    })
                    .await;
                match result {
                    Ok(Ok(Some(id))) => {
                        win.show_norm_in_active(id);
                        win.reveal_content();
                    }
                    Ok(Ok(None)) => win.toast(
                        &gettext("Norm {enbez} wurde nicht gefunden.").replace("{enbez}", &enbez),
                    ),
                    _ => {}
                }
            }
        ));
    }

    // ------------------------------------------------------------------
    // Schnellsuche
    // ------------------------------------------------------------------

    fn open_quick_search(&self) {
        self.open_quick_search_with(None);
    }

    /// Öffnet die Schnellsuche, optional mit vorbelegter Eingabe (Tests).
    fn open_quick_search_with(&self, query: Option<&str>) {
        let dialog = LexQuickSearch::new();
        dialog.set_preferred_law(self.preferred_law());
        dialog.connect_jump(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |norm_id, other_pane| {
                if other_pane {
                    win.reader().show_in_other_pane(norm_id);
                } else {
                    win.show_norm_in_active(norm_id);
                }
                win.reveal_content();
            }
        ));
        dialog.present(Some(self));
        if let Some(query) = query {
            dialog.set_query(query);
        }
    }

    // ------------------------------------------------------------------
    // Persistenz der Tabs
    // ------------------------------------------------------------------

    pub fn save_reader_state(&self) {
        if self.imp().laws.borrow().is_empty() {
            // Ohne installiertes Gesetz den alten Stand nicht überschreiben.
            return;
        }
        let state = self.reader().state();
        if state.panes.is_empty() {
            return;
        }
        let json = serde_json::to_string(&state).unwrap_or_default();
        if let Err(err) = self.settings().set_string("reader-state", &json) {
            log::warn!("reader-state konnte nicht gespeichert werden: {err}");
        }
    }

    /// Stellt die zuletzt gelesenen Normen wieder her; unbekannte
    /// Bezeichnungen werden still übersprungen, sonst die erste Norm.
    fn restore_reader_state(&self) {
        let state: Option<ReaderState> =
            serde_json::from_str(self.settings().string("reader-state").as_str()).ok();
        let Some(state) = state.filter(|s| !s.panes.is_empty()) else {
            self.open_first_norm();
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let path = Self::db_path();
                let panes = state.panes.clone();
                let resolved =
                    gio::spawn_blocking(move || -> Result<Vec<Option<i64>>, rusqlite::Error> {
                        let db = Database::open(&path)?;
                        let mut ids = Vec::new();
                        for pane in &panes {
                            let id = match db.law_by_slug(&pane.law)? {
                                Some(law) => db.norm_id_by_enbez(law.id, &pane.norm)?,
                                None => None,
                            };
                            ids.push(id);
                        }
                        Ok(ids)
                    })
                    .await;
                let ids = match resolved {
                    Ok(Ok(ids)) => ids,
                    _ => Vec::new(),
                };
                let reader = win.reader();
                match ids.first() {
                    Some(Some(first)) => reader.show_norm_in_pane(*first, 0),
                    _ => {
                        win.open_first_norm();
                        return;
                    }
                }
                if let Some(Some(second)) = ids.get(1) {
                    reader.set_split(true);
                    reader.show_norm_in_pane(*second, 1);
                    reader.set_active_pane(state.active);
                }
            }
        ));
    }
}

/// Gesetz eines Verweises: eigenes Gesetz (Slug) oder fremdes Kürzel.
#[derive(Debug, Clone)]
enum LawKey {
    Slug(String),
    Abbrev(String),
}

/// Ergebnis der Verweisauflösung.
enum RefLookup {
    Found(i64),
    NormMissing,
    LawMissing,
}

/// Rahmennorm (ohne Bezeichnung) und Inhaltsübersicht bleiben außen vor.
fn is_readable_norm(norm: &NormInfo) -> bool {
    norm.enbez
        .as_deref()
        .is_some_and(|e| e != "Inhaltsübersicht")
}

/// Aus der Datenbank gelesene Gliederungsdaten.
struct OutlineData {
    units: Vec<UnitInfo>,
    norms: Vec<NormInfo>,
}

// ----------------------------------------------------------------------
// Hilfsfunktionen (ohne GTK, testbar)
// ----------------------------------------------------------------------

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
    fn readable_norms() {
        let mut n = NormInfo::default();
        assert!(!is_readable_norm(&n));
        n.enbez = Some("Inhaltsübersicht".into());
        assert!(!is_readable_norm(&n));
        n.enbez = Some("§ 1".into());
        assert!(is_readable_norm(&n));
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
