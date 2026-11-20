// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Karteikarten-Modus: Übersicht der Stapel, Import von Anki-Paketen und
//! die Lernansicht. Gelernt wird nach dem Fächerprinzip: Die oberste Karte
//! des Stapels wird umgedreht und als gewusst oder nicht gewusst bewertet;
//! der Karteikasten darunter zeigt, wie sich die Karten auf die Fächer
//! verteilen.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gdk, gio, glib, CompositeTemplate};

use super::index_card::Face;
use super::{LexCardStack, LexLeitnerBox};
use crate::cards::import::{self, ImportSummary};
use crate::cards::{self, html, Card, DeckInfo, Rating, Session, SessionMode, PAPER_COLORS};
use crate::db::Database;
use crate::refs::NormRef;

type NavigateCallback = Box<dyn Fn(&NormRef)>;
type MessageCallback = Box<dyn Fn(&str)>;
type DueCallback = Box<dyn Fn(u32)>;
/// Aktionen ohne Parameter und solche mit der ID eines Stapels.
type SimpleHandler = fn(&LexFlashcards);
type DeckHandler = fn(&LexFlashcards, i64);

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/flashcards.ui")]
    pub struct LexFlashcards {
        #[template_child]
        pub nav: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub decks_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub busy_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub summary_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub deck_flow: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub study_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub study_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub progress_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub card_stack: TemplateChild<LexCardStack>,
        #[template_child]
        pub buttons_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub leitner: TemplateChild<LexLeitnerBox>,
        #[template_child]
        pub done_page: TemplateChild<adw::StatusPage>,
        pub actions: OnceCell<gio::SimpleActionGroup>,
        /// Alle Stapel in der Reihenfolge der Übersicht.
        pub decks: RefCell<Vec<DeckInfo>>,
        /// Stapel und Sitzung der Lernansicht.
        pub deck: RefCell<Option<DeckInfo>>,
        pub session: RefCell<Option<Session>>,
        /// Zeigt die oberste Karte ihre Rückseite?
        pub flipped: Cell<bool>,
        pub busy: Cell<bool>,
        pub on_navigate: RefCell<Option<NavigateCallback>>,
        pub on_message: RefCell<Option<MessageCallback>>,
        pub on_due: RefCell<Option<DueCallback>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexFlashcards {
        const NAME: &'static str = "LexFlashcards";
        type Type = super::LexFlashcards;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            LexCardStack::ensure_type();
            LexLeitnerBox::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexFlashcards {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.busy_page
                .set_paintable(Some(&adw::SpinnerPaintable::new(Some(&*self.busy_page))));
            obj.setup_actions();
            obj.setup_widgets();
            obj.setup_keys();
            obj.setup_drop();
            obj.update_study_ui();
        }
    }

    impl WidgetImpl for LexFlashcards {}
    impl BinImpl for LexFlashcards {}
}

glib::wrapper! {
    pub struct LexFlashcards(ObjectSubclass<imp::LexFlashcards>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexFlashcards {
    fn default() -> Self {
        Self::new()
    }
}

impl LexFlashcards {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Rückruf für Klicks auf Normverweise im Kartentext.
    pub fn connect_navigate(&self, f: impl Fn(&NormRef) + 'static) {
        self.imp().on_navigate.replace(Some(Box::new(f)));
    }

    /// Rückruf für Meldungen, die das Fenster als Toast zeigt.
    pub fn connect_message(&self, f: impl Fn(&str) + 'static) {
        self.imp().on_message.replace(Some(Box::new(f)));
    }

    /// Rückruf mit der Zahl heute fälliger Karten über alle Stapel.
    pub fn connect_due_changed(&self, f: impl Fn(u32) + 'static) {
        self.imp().on_due.replace(Some(Box::new(f)));
    }

    fn message(&self, text: &str) {
        if let Some(callback) = self.imp().on_message.borrow().as_ref() {
            callback(text);
        }
    }

    /// Gibt der Lernansicht den Tastaturfokus, damit Leertaste und
    /// Pfeiltasten wirken.
    pub fn focus_study(&self) {
        if self.is_studying() {
            self.imp().card_stack.grab_focus();
        }
    }

    fn is_studying(&self) -> bool {
        let imp = self.imp();
        imp.nav
            .visible_page()
            .and_then(|p| p.tag())
            .is_some_and(|t| t == "study")
            && imp.study_stack.visible_child_name().as_deref() == Some("card")
    }

    // ------------------------------------------------------------------
    // Einrichtung
    // ------------------------------------------------------------------

    fn setup_actions(&self) {
        let group = gio::SimpleActionGroup::new();
        let simple: [(&str, SimpleHandler); 10] = [
            ("import", Self::choose_files),
            ("flip", Self::flip),
            ("known", |this| this.rate(Rating::Known)),
            ("unknown", |this| this.rate(Rating::Unknown)),
            ("next", |this| this.step(true)),
            ("previous", |this| this.step(false)),
            ("back", |this| {
                this.imp().nav.pop();
            }),
            ("learn-current", |this| this.restart(SessionMode::Learn)),
            ("browse-current", |this| this.restart(SessionMode::Browse)),
            ("reset-current", |this| {
                if let Some(id) = this.current_deck_id() {
                    this.confirm_reset(id);
                }
            }),
        ];
        for (name, handler) in simple {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| handler(&this)
            ));
            group.add_action(&action);
        }
        let with_deck: [(&str, DeckHandler); 4] = [
            ("learn", |this, id| {
                this.start_session(id, SessionMode::Learn)
            }),
            ("browse", |this, id| {
                this.start_session(id, SessionMode::Browse)
            }),
            ("reset", Self::confirm_reset),
            ("delete", Self::confirm_delete),
        ];
        for (name, handler) in with_deck {
            let action = gio::SimpleAction::new(name, Some(&i64::static_variant_type()));
            action.connect_activate(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, param| {
                    if let Some(id) = param.and_then(|p| p.get::<i64>()) {
                        handler(&this, id);
                    }
                }
            ));
            group.add_action(&action);
        }
        // Papierfarbe eines Stapels: Parameter „<Stapel-ID>:<Farbe>“.
        let color = gio::SimpleAction::new("color", Some(&String::static_variant_type()));
        color.connect_activate(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_, param| {
                let spec = param.and_then(|p| p.get::<String>()).unwrap_or_default();
                if let Some((id, color)) = spec.split_once(':') {
                    if let Ok(id) = id.parse::<i64>() {
                        this.set_deck_color(id, color);
                    }
                }
            }
        ));
        group.add_action(&color);
        self.insert_action_group("cards", Some(&group));
        self.imp().actions.set(group).ok();
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        let action = self
            .imp()
            .actions
            .get()
            .and_then(|group| group.lookup_action(name))
            .and_downcast::<gio::SimpleAction>();
        if let Some(action) = action {
            action.set_enabled(enabled);
        }
    }

    fn setup_widgets(&self) {
        let imp = self.imp();
        imp.card_stack.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| this.flip()
        ));
        for card in imp.card_stack.cards() {
            card.connect_link(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, target| {
                    if let Some(callback) = this.imp().on_navigate.borrow().as_ref() {
                        callback(target);
                    }
                }
            ));
        }
        imp.deck_flow.connect_child_activated(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_, child| {
                let deck = this
                    .imp()
                    .decks
                    .borrow()
                    .get(child.index().max(0) as usize)
                    .cloned();
                if let Some(deck) = deck {
                    this.start_session(deck.id, SessionMode::Learn);
                }
            }
        ));
        // Zurück in der Übersicht: Sitzung beenden und Zähler neu lesen.
        imp.nav.connect_popped(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_, page| {
                if page.tag().is_some_and(|t| t == "study") {
                    this.imp().session.replace(None);
                    this.imp().deck.replace(None);
                    this.refresh();
                    // Der Fokus gehört auf die Stapel, nicht auf die
                    // Schaltflächen der Kopfleiste.
                    this.imp().deck_flow.grab_focus();
                }
            }
        ));
    }

    /// Tastatur in der Lernansicht: Leertaste dreht um, Pfeile und Ziffern
    /// bewerten bzw. blättern.
    fn setup_keys(&self) {
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, modifiers| {
                let blocking = gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SUPER_MASK;
                if !this.is_studying() || modifiers.intersects(blocking) {
                    return glib::Propagation::Proceed;
                }
                let browsing = this
                    .imp()
                    .session
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.mode() == SessionMode::Browse);
                match key {
                    gdk::Key::space
                    | gdk::Key::Return
                    | gdk::Key::KP_Enter
                    | gdk::Key::Up
                    | gdk::Key::Down => this.flip(),
                    gdk::Key::Left | gdk::Key::_1 | gdk::Key::KP_1 => {
                        if browsing {
                            this.step(false)
                        } else {
                            this.rate(Rating::Unknown)
                        }
                    }
                    gdk::Key::Right | gdk::Key::_2 | gdk::Key::KP_2 => {
                        if browsing {
                            this.step(true)
                        } else {
                            this.rate(Rating::Known)
                        }
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.add_controller(controller);
    }

    /// Pakete lassen sich auch per Ziehen und Ablegen importieren.
    fn setup_drop(&self) {
        let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        target.connect_drop(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                let Ok(list) = value.get::<gdk::FileList>() else {
                    return false;
                };
                let paths: Vec<PathBuf> = list
                    .files()
                    .iter()
                    .filter_map(|f| f.path())
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("apkg"))
                    })
                    .collect();
                if paths.is_empty() {
                    this.message(&gettext(
                        "Nur Anki-Kartenpakete (.apkg) lassen sich importieren.",
                    ));
                    return false;
                }
                this.import_files(paths);
                true
            }
        ));
        self.add_controller(target);
    }

    // ------------------------------------------------------------------
    // Stapelübersicht
    // ------------------------------------------------------------------

    /// Liest die Stapel aus der Datenbank und baut die Übersicht neu auf.
    pub fn refresh(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let path = Database::default_path();
                let today = cards::today();
                let result =
                    gio::spawn_blocking(move || Database::open(&path)?.card_decks(today)).await;
                match result {
                    Ok(Ok(decks)) => this.set_decks(decks),
                    Ok(Err(err)) => {
                        log::warn!("Kartenstapel konnten nicht gelesen werden: {err}");
                        this.message(&gettext("Die Kartenstapel konnten nicht gelesen werden."));
                    }
                    Err(_) => log::warn!("Lesen der Kartenstapel abgebrochen"),
                }
            }
        ));
    }

    fn set_decks(&self, decks: Vec<DeckInfo>) {
        let imp = self.imp();
        while let Some(child) = imp.deck_flow.first_child() {
            imp.deck_flow.remove(&child);
        }
        for deck in &decks {
            imp.deck_flow.append(&self.build_tile(deck));
        }
        let due: u32 = decks.iter().map(|d| d.due).sum();
        let count = decks.len() as u32;
        let stacks = ngettext("{n} Stapel", "{n} Stapel", count).replace("{n}", &count.to_string());
        let due_text = if due == 0 {
            gettext("heute ist nichts fällig")
        } else {
            ngettext("{n} Karte heute fällig", "{n} Karten heute fällig", due)
                .replace("{n}", &due.to_string())
        };
        imp.summary_label
            .set_label(&format!("{stacks} · {due_text}"));
        let empty = decks.is_empty();
        imp.decks.replace(decks);
        if !imp.busy.get() {
            imp.decks_stack
                .set_visible_child_name(if empty { "empty" } else { "decks" });
        }
        if let Some(callback) = imp.on_due.borrow().as_ref() {
            callback(due);
        }
    }

    /// Kachel eines Stapels: der Stapel selbst mit Aufschrift, darunter
    /// Kartenzahl und Fälligkeit, rechts oben das Menü.
    fn build_tile(&self, deck: &DeckInfo) -> gtk::Widget {
        let stack = LexCardStack::new();
        stack.set_mini(true);
        stack.set_paper(&deck.color);
        stack.set_depth(deck.total.saturating_sub(1));
        stack.set_can_target(false);
        let face = Face::title(&deck.parent().unwrap_or_default(), deck.title());
        stack.show(|card| card.set_face(&face));
        let menu = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text(gettext("Stapel"))
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .menu_model(&deck_menu(deck))
            .css_classes(["flat", "circular", "deck-menu"])
            .build();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&stack));
        overlay.add_overlay(&menu);
        let total =
            ngettext("{n} Karte", "{n} Karten", deck.total).replace("{n}", &deck.total.to_string());
        let status = if deck.due > 0 {
            format!(
                "<b>{}</b> · {total}",
                gettext("{n} fällig").replace("{n}", &deck.due.to_string())
            )
        } else {
            format!("{total} · {}", gettext("nichts fällig"))
        };
        let label = gtk::Label::builder()
            .use_markup(true)
            .label(status)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .css_classes(["deck-status", "numeric"])
            .build();
        if deck.due > 0 {
            label.add_css_class("due");
        }
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 0);
        tile.add_css_class("deck-tile");
        tile.append(&overlay);
        tile.append(&label);
        tile.set_tooltip_text(Some(&format!(
            "{}\n{}",
            deck.name.replace(cards::DECK_SEPARATOR, " › "),
            gettext("Quelle: {file}").replace("{file}", &deck.source)
        )));
        tile.upcast()
    }

    fn deck_by_id(&self, id: i64) -> Option<DeckInfo> {
        self.imp()
            .decks
            .borrow()
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }

    fn current_deck_id(&self) -> Option<i64> {
        self.imp().deck.borrow().as_ref().map(|d| d.id)
    }

    // ------------------------------------------------------------------
    // Import
    // ------------------------------------------------------------------

    fn choose_files(&self) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Anki-Kartenpakete (*.apkg)")));
        filter.add_suffix("apkg");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Anki-Paket importieren"))
            .accept_label(gettext("Importieren"))
            .filters(&filters)
            .modal(true)
            .build();
        let window = self.root().and_downcast::<gtk::Window>();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                match dialog.open_multiple_future(window.as_ref()).await {
                    Ok(files) => {
                        let paths = files
                            .iter::<gio::File>()
                            .flatten()
                            .filter_map(|f| f.path())
                            .collect();
                        this.import_files(paths);
                    }
                    Err(err) if err.matches(gtk::DialogError::Dismissed) => {}
                    Err(err) => log::warn!("Dateiauswahl fehlgeschlagen: {err}"),
                }
            }
        ));
    }

    fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        imp.busy.set(busy);
        self.set_action_enabled("import", !busy);
        if busy {
            imp.decks_stack.set_visible_child_name("busy");
        }
    }

    /// Importiert die Pakete nacheinander im Hintergrund.
    fn import_files(&self, paths: Vec<PathBuf>) {
        if self.imp().busy.get() || paths.is_empty() {
            return;
        }
        self.set_busy(true);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let today = cards::today();
                let mut total = ImportSummary::default();
                let mut errors = Vec::new();
                for package in paths {
                    let name = package
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let db = Database::default_path();
                    let media = import::media_root();
                    let result = gio::spawn_blocking(move || {
                        import::import_package(&db, &package, &media, today)
                    })
                    .await;
                    match result {
                        Ok(Ok(summary)) => {
                            total.decks += summary.decks;
                            total.added += summary.added;
                            total.updated += summary.updated;
                            total.removed += summary.removed;
                        }
                        Ok(Err(err)) => errors.push(format!("{name}: {err}")),
                        Err(_) => errors.push(name),
                    }
                }
                this.set_busy(false);
                if total.decks > 0 {
                    this.message(&describe_import(&total));
                }
                for error in errors {
                    log::warn!("Kartenimport fehlgeschlagen: {error}");
                    this.message(
                        &gettext("Import fehlgeschlagen – {error}").replace("{error}", &error),
                    );
                }
                this.refresh();
            }
        ));
    }

    // ------------------------------------------------------------------
    // Lernansicht
    // ------------------------------------------------------------------

    /// Lädt die Karten eines Stapels und öffnet die Lernansicht.
    fn start_session(&self, deck_id: i64, mode: SessionMode) {
        let Some(deck) = self.deck_by_id(deck_id) else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let path = Database::default_path();
                let today = cards::today();
                let result = gio::spawn_blocking(move || {
                    let db = Database::open(&path)?;
                    match mode {
                        SessionMode::Learn => db.due_cards(deck_id, today),
                        SessionMode::Browse => db.deck_cards(deck_id),
                    }
                })
                .await;
                match result {
                    Ok(Ok(cards)) => this.begin(deck, Session::new(mode, cards)),
                    _ => this.message(&gettext("Die Karten konnten nicht geladen werden.")),
                }
            }
        ));
    }

    /// Startet für den offenen Stapel eine neue Sitzung.
    fn restart(&self, mode: SessionMode) {
        let Some(id) = self.current_deck_id() else {
            return;
        };
        // Zähler des Stapels aus der laufenden Sitzung übernehmen.
        let current = self.imp().deck.borrow().clone();
        if let Some(current) = current {
            let mut decks = self.imp().decks.borrow_mut();
            if let Some(deck) = decks.iter_mut().find(|d| d.id == id) {
                *deck = current;
            }
        }
        self.start_session(id, mode);
    }

    fn begin(&self, deck: DeckInfo, session: Session) {
        let imp = self.imp();
        imp.card_stack.set_paper(&deck.color);
        imp.leitner.set_paper(&deck.color);
        imp.leitner.set_counts(deck.boxes);
        imp.deck.replace(Some(deck));
        imp.flipped.set(false);
        let first = session.current().cloned();
        imp.session.replace(Some(session));
        match first {
            Some(card) => {
                let face = self.face(&card, false);
                imp.card_stack.show(|c| c.set_face(&face));
                imp.study_stack.set_visible_child_name("card");
            }
            None => self.show_done(),
        }
        self.update_study_ui();
        let on_study = imp
            .nav
            .visible_page()
            .and_then(|p| p.tag())
            .is_some_and(|t| t == "study");
        if !on_study {
            imp.nav.push_by_tag("study");
        }
        self.focus_study();
    }

    /// Beschriftung einer Kartenseite.
    fn face(&self, card: &Card, back: bool) -> Face {
        let deck = self.imp().deck.borrow();
        Face {
            caption: deck
                .as_ref()
                .map(|d| d.title().to_owned())
                .unwrap_or_default(),
            corner: if back {
                gettext("Antwort")
            } else {
                gettext("Frage")
            },
            content: html::render(if back { &card.back } else { &card.front }),
            media_dir: deck
                .as_ref()
                .map(|d| import::media_dir(&import::media_root(), d.id)),
        }
    }

    /// Titel, Fortschritt, Stapelhöhe und Schaltflächen passend zur Sitzung.
    fn update_study_ui(&self) {
        let imp = self.imp();
        let session = imp.session.borrow();
        let deck = imp.deck.borrow();
        let (Some(session), Some(deck)) = (session.as_ref(), deck.as_ref()) else {
            for name in ["flip", "known", "unknown", "next", "previous"] {
                self.set_action_enabled(name, false);
            }
            return;
        };
        let learning = session.mode() == SessionMode::Learn;
        let remaining = session.remaining() as u32;
        imp.study_title.set_title(deck.title());
        imp.study_title
            .set_subtitle(&if learning && remaining == 0 {
                gettext("erledigt")
            } else if learning {
                ngettext("noch {n} Karte", "noch {n} Karten", remaining)
                    .replace("{n}", &remaining.to_string())
            } else {
                gettext("Durchblättern")
            });
        let progress = match session.current() {
            Some(card) => gettext("Karte {i} von {n} · Fach {box}")
                .replace("{i}", &session.position().to_string())
                .replace("{n}", &session.total().to_string())
                .replace("{box}", &card.box_no.to_string()),
            None => String::new(),
        };
        imp.progress_label.set_label(&progress);
        imp.card_stack.set_depth(remaining.saturating_sub(1));
        let flipped = imp.flipped.get();
        let has_card = session.current().is_some();
        imp.buttons_stack.set_visible_child_name(if !learning {
            "browse"
        } else if flipped {
            "rate"
        } else {
            "flip"
        });
        self.set_action_enabled("flip", has_card);
        self.set_action_enabled("known", learning && flipped && has_card);
        self.set_action_enabled("unknown", learning && flipped && has_card);
        self.set_action_enabled("next", !learning && session.remaining() > 1);
        self.set_action_enabled("previous", !learning && session.position() > 1);
    }

    /// Dreht die oberste Karte um.
    fn flip(&self) {
        let imp = self.imp();
        if !self.is_studying() || imp.card_stack.is_animating() {
            return;
        }
        let card = imp
            .session
            .borrow()
            .as_ref()
            .and_then(|s| s.current().cloned());
        let Some(card) = card else {
            return;
        };
        let flipped = !imp.flipped.get();
        imp.flipped.set(flipped);
        let face = self.face(&card, flipped);
        imp.card_stack.flip(move |c| c.set_face(&face));
        self.update_study_ui();
    }

    /// Bewertet die umgedrehte Karte; sie fliegt nach rechts (gewusst) oder
    /// links (nicht gewusst) vom Stapel.
    fn rate(&self, rating: Rating) {
        let imp = self.imp();
        if !self.is_studying() || !imp.flipped.get() {
            return;
        }
        let today = cards::today();
        let outcome = {
            let mut session = imp.session.borrow_mut();
            match session.as_mut() {
                Some(s) if s.mode() == SessionMode::Learn => s.rate(rating, today),
                _ => None,
            }
        };
        let Some(outcome) = outcome else {
            return;
        };
        // Zähler des Stapels mitführen, damit der Karteikasten sofort stimmt.
        if let Some(deck) = imp.deck.borrow_mut().as_mut() {
            let old = usize::from(outcome.old_box.clamp(1, cards::BOX_COUNT as u8)) - 1;
            let new = usize::from(outcome.new_box.clamp(1, cards::BOX_COUNT as u8)) - 1;
            deck.boxes[old] = deck.boxes[old].saturating_sub(1);
            deck.boxes[new] += 1;
            if rating == Rating::Known {
                deck.due = deck.due.saturating_sub(1);
            }
            imp.leitner.set_counts(deck.boxes);
        }
        imp.leitner.flash(outcome.new_box);
        let path = Database::default_path();
        glib::spawn_future_local(async move {
            let result =
                gio::spawn_blocking(move || Database::open(&path)?.apply_rating(&outcome, today))
                    .await;
            if !matches!(result, Ok(Ok(()))) {
                log::warn!("Bewertung konnte nicht gespeichert werden");
            }
        });
        imp.flipped.set(false);
        let next = imp
            .session
            .borrow()
            .as_ref()
            .and_then(|s| s.current().cloned());
        let next_face = next.map(|card| self.face(&card, false));
        let finished = next_face.is_none();
        self.update_study_ui();
        let direction = if rating == Rating::Known { 1.0 } else { -1.0 };
        imp.card_stack.fly_out(
            direction,
            next_face.map(|face| {
                Box::new(move |c: &super::LexIndexCard| c.set_face(&face))
                    as Box<dyn FnOnce(&super::LexIndexCard)>
            }),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                move || {
                    if finished {
                        this.show_done();
                    }
                }
            ),
        );
    }

    /// Blättermodus: zur nächsten oder vorherigen Karte.
    fn step(&self, forward: bool) {
        let imp = self.imp();
        if !self.is_studying() {
            return;
        }
        let moved = {
            let mut session = imp.session.borrow_mut();
            match session.as_mut() {
                Some(s) if s.mode() == SessionMode::Browse => {
                    if forward {
                        s.next()
                    } else {
                        s.previous()
                    }
                }
                _ => false,
            }
        };
        if !moved {
            return;
        }
        imp.flipped.set(false);
        let card = imp
            .session
            .borrow()
            .as_ref()
            .and_then(|s| s.current().cloned());
        let Some(card) = card else {
            return;
        };
        let face = self.face(&card, false);
        self.update_study_ui();
        if forward {
            imp.card_stack
                .fly_out(-1.0, Some(Box::new(move |c| c.set_face(&face))), || {});
        } else {
            imp.card_stack.fly_in(-1.0, move |c| c.set_face(&face));
        }
    }

    /// Abschlussseite einer Lernsitzung.
    fn show_done(&self) {
        let imp = self.imp();
        let (total, known, unknown) = imp
            .session
            .borrow()
            .as_ref()
            .map(|s| (s.total(), s.known() as u32, s.unknown() as u32))
            .unwrap_or_default();
        if total == 0 {
            imp.done_page.set_title(&gettext("Nichts fällig"));
            imp.done_page.set_description(Some(&gettext(
                "Alle Karten dieses Stapels sind für heute erledigt. Durchblättern lassen sie sich jederzeit.",
            )));
        } else {
            let mut text = ngettext("{n} Karte gewusst", "{n} Karten gewusst", known)
                .replace("{n}", &known.to_string());
            if unknown > 0 {
                text.push_str(" · ");
                text.push_str(
                    &ngettext("{n} Wiederholung", "{n} Wiederholungen", unknown)
                        .replace("{n}", &unknown.to_string()),
                );
            }
            imp.done_page.set_title(&gettext("Für heute geschafft"));
            imp.done_page.set_description(Some(&text));
        }
        imp.study_stack.set_visible_child_name("done");
    }

    // ------------------------------------------------------------------
    // Stapel verwalten
    // ------------------------------------------------------------------

    fn confirm_reset(&self, deck_id: i64) {
        let Some(deck) = self.deck_by_id(deck_id) else {
            return;
        };
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Lernstand zurücksetzen?")),
            Some(
                &gettext("Alle Karten des Stapels „{name}“ wandern zurück in Fach 1.")
                    .replace("{name}", deck.title()),
            ),
        );
        dialog.add_responses(&[
            ("cancel", &gettext("_Abbrechen")),
            ("reset", &gettext("_Zurücksetzen")),
        ]);
        dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(
            Some("reset"),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| this.reset_deck(deck_id)
            ),
        );
        dialog.present(Some(self));
    }

    fn reset_deck(&self, deck_id: i64) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let path = Database::default_path();
                let today = cards::today();
                let result = gio::spawn_blocking(move || {
                    let db = Database::open(&path)?;
                    db.reset_deck(deck_id, today)?;
                    db.card_decks(today)
                })
                .await;
                match result {
                    Ok(Ok(decks)) => {
                        this.set_decks(decks);
                        if this.current_deck_id() == Some(deck_id) {
                            this.start_session(deck_id, SessionMode::Learn);
                        }
                    }
                    _ => this.message(&gettext("Der Lernstand konnte nicht zurückgesetzt werden.")),
                }
            }
        ));
    }

    fn confirm_delete(&self, deck_id: i64) {
        let Some(deck) = self.deck_by_id(deck_id) else {
            return;
        };
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Stapel löschen?")),
            Some(
                &ngettext(
                    "„{name}“ mit {n} Karte und dem Lernstand wird entfernt.",
                    "„{name}“ mit {n} Karten und dem Lernstand wird entfernt.",
                    deck.total,
                )
                .replace("{name}", deck.title())
                .replace("{n}", &deck.total.to_string()),
            ),
        );
        dialog.add_responses(&[
            ("cancel", &gettext("_Abbrechen")),
            ("delete", &gettext("_Löschen")),
        ]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(
            Some("delete"),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| this.delete_deck(deck_id)
            ),
        );
        dialog.present(Some(self));
    }

    fn delete_deck(&self, deck_id: i64) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let path = Database::default_path();
                let media = import::media_root();
                let result =
                    gio::spawn_blocking(move || import::remove_deck(&path, &media, deck_id)).await;
                if !matches!(result, Ok(Ok(()))) {
                    this.message(&gettext("Der Stapel konnte nicht gelöscht werden."));
                }
                if this.current_deck_id() == Some(deck_id) {
                    this.imp().nav.pop();
                }
                this.refresh();
            }
        ));
    }

    fn set_deck_color(&self, deck_id: i64, color: &str) {
        if !PAPER_COLORS.contains(&color) {
            return;
        }
        let color = color.to_owned();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            async move {
                let path = Database::default_path();
                let result = gio::spawn_blocking(move || {
                    Database::open(&path)?.set_deck_color(deck_id, &color)
                })
                .await;
                if !matches!(result, Ok(Ok(()))) {
                    log::warn!("Papierfarbe konnte nicht gespeichert werden");
                }
                this.refresh();
            }
        ));
    }

    /// Nur für Tests per D-Bus: „import <Pfad>“, „open <Nr> [browse]“,
    /// „flip“, „known“, „unknown“, „next“, „prev“, „back“, „reset <Nr>“,
    /// „delete <Nr>“ (beide ohne Rückfrage), „confirm-delete <Nr>“.
    pub fn test_command(&self, command: &str) {
        let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
        match name {
            "import" => self.import_files(vec![PathBuf::from(arg.trim())]),
            "open" => {
                let mut parts = arg.split_whitespace();
                let index = parts
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
                let mode = if parts.next() == Some("browse") {
                    SessionMode::Browse
                } else {
                    SessionMode::Learn
                };
                let id = self.imp().decks.borrow().get(index).map(|d| d.id);
                if let Some(id) = id {
                    self.start_session(id, mode);
                }
            }
            "flip" => self.flip(),
            "known" => self.rate(Rating::Known),
            "unknown" => self.rate(Rating::Unknown),
            "next" => self.step(true),
            "prev" => self.step(false),
            "back" => {
                self.imp().nav.pop();
            }
            // Ohne Rückfrage, damit Tests keinen Dialog bedienen müssen.
            "reset" | "delete" | "confirm-delete" => {
                let index = arg.trim().parse::<usize>().unwrap_or(0);
                let id = self.imp().decks.borrow().get(index).map(|d| d.id);
                match (name, id) {
                    ("reset", Some(id)) => self.reset_deck(id),
                    ("delete", Some(id)) => self.delete_deck(id),
                    ("confirm-delete", Some(id)) => self.confirm_delete(id),
                    _ => {}
                }
            }
            other => log::warn!("Unbekannter Testbefehl: {other}"),
        }
    }
}

/// Bezeichnung einer Papierfarbe für das Menü.
fn paper_color_label(color: &str) -> String {
    match color {
        "yellow" => gettext("Gelb"),
        "green" => gettext("Grün"),
        "blue" => gettext("Blau"),
        "pink" => gettext("Rosa"),
        _ => gettext("Weiß"),
    }
}

/// Menü einer Stapelkachel.
fn deck_menu(deck: &DeckInfo) -> gio::Menu {
    let item = |label: &str, action: &str, target: glib::Variant| {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some(action), Some(&target));
        item
    };
    let id = deck.id.to_variant();
    let study = gio::Menu::new();
    study.append_item(&item(
        &gettext("Fällige Karten _lernen"),
        "cards.learn",
        id.clone(),
    ));
    study.append_item(&item(
        &gettext("Alle Karten _durchblättern"),
        "cards.browse",
        id.clone(),
    ));
    let colors = gio::Menu::new();
    for color in PAPER_COLORS {
        colors.append_item(&item(
            &paper_color_label(color),
            "cards.color",
            format!("{}:{color}", deck.id).to_variant(),
        ));
    }
    let manage = gio::Menu::new();
    manage.append_submenu(Some(&gettext("_Papierfarbe")), &colors);
    manage.append_item(&item(
        &gettext("Lernstand _zurücksetzen"),
        "cards.reset",
        id.clone(),
    ));
    manage.append_item(&item(&gettext("Stapel _löschen …"), "cards.delete", id));
    let menu = gio::Menu::new();
    menu.append_section(None, &study);
    menu.append_section(None, &manage);
    menu
}

/// Meldung nach einem Import.
fn describe_import(summary: &ImportSummary) -> String {
    let decks = summary.decks as u32;
    let mut text = ngettext("{n} Stapel importiert", "{n} Stapel importiert", decks)
        .replace("{n}", &decks.to_string());
    let mut parts = Vec::new();
    if summary.added > 0 {
        parts.push(
            ngettext("{n} neue Karte", "{n} neue Karten", summary.added as u32)
                .replace("{n}", &summary.added.to_string()),
        );
    }
    if summary.updated > 0 {
        parts.push(gettext("{n} aktualisiert").replace("{n}", &summary.updated.to_string()));
    }
    if summary.removed > 0 {
        parts.push(gettext("{n} entfernt").replace("{n}", &summary.removed.to_string()));
    }
    if !parts.is_empty() {
        text.push_str(": ");
        text.push_str(&parts.join(", "));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_message_lists_only_what_happened() {
        let summary = ImportSummary {
            decks: 2,
            added: 40,
            updated: 0,
            removed: 0,
        };
        assert_eq!(
            describe_import(&summary),
            "2 Stapel importiert: 40 neue Karten"
        );
        let update = ImportSummary {
            decks: 1,
            added: 1,
            updated: 12,
            removed: 3,
        };
        assert_eq!(
            describe_import(&update),
            "1 Stapel importiert: 1 neue Karte, 12 aktualisiert, 3 entfernt"
        );
    }
}
