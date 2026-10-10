// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Sprungleiste in der Mitte der Kopfleiste. Die Eingabe wird nicht nach
//! Gesetz oder Paragrafnummer unterschieden („433“, „zpo 253“,
//! „§ 55a bgb“, „Kaufvertrag“). Darunter erscheint eine Trefferliste mit
//! Vorschau des ausgewählten Treffers; der beste Treffer wird inline
//! vervollständigt (markierter Rest, Tab übernimmt ihn). Eingabe springt
//! zum ausgewählten Treffer, Strg+Eingabe öffnet ihn in der zweiten Ansicht.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::db::{Database, QuickHit};

/// Höchstzahl angezeigter Treffer.
const LIMIT: usize = 30;
/// Länge der Vorschau in Zeichen.
const PREVIEW_CHARS: usize = 700;
/// Breite der Trefferliste und der Vorschau.
const LIST_WIDTH: i32 = 340;
const PREVIEW_WIDTH: i32 = 360;
/// Platz für Suchsymbol und Innenabstand links vom Platzhalter.
const ICON_SPACE: i32 = 36;
/// Mindestabstand zwischen Platzhalter und Normhinweis.
const CONTEXT_GAP: i32 = 18;
/// Schmaler als das lohnt sich der Normhinweis nicht mehr.
const CONTEXT_MIN_WIDTH: i32 = 72;

/// Aufruf beim Sprung: Norm-ID und ob die zweite Ansicht gewünscht ist.
type JumpCallback = Box<dyn Fn(i64, bool)>;
/// Aufruf, wenn die Leiste den Fokus abgibt (Escape oder Sprung).
type DoneCallback = Box<dyn Fn()>;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate, glib::Properties)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/jump_bar.ui")]
    #[properties(wrapper_type = super::LexJumpBar)]
    pub struct LexJumpBar {
        #[template_child]
        pub entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub context_label: TemplateChild<gtk::Label>,
        /// Schmale Darstellung: Treffer ohne Vorschau, kein Normhinweis.
        #[property(get, set = Self::set_compact)]
        pub compact: Cell<bool>,
        pub popover: OnceCell<gtk::Popover>,
        pub results: OnceCell<gtk::ListBox>,
        pub preview: OnceCell<gtk::Box>,
        pub preview_title: OnceCell<gtk::Label>,
        pub preview_subtitle: OnceCell<gtk::Label>,
        pub preview_body: OnceCell<gtk::Label>,
        /// Treffer in der Reihenfolge der Zeilen in `results`.
        pub hits: RefCell<Vec<QuickHit>>,
        /// Bereits geladene Vorschautexte je Norm-ID.
        pub previews: RefCell<HashMap<i64, String>>,
        pub preferred_law: Cell<Option<i64>>,
        pub serial: Cell<u64>,
        pub preview_serial: Cell<u64>,
        /// Zuletzt vom Benutzer getippter Text (ohne Vervollständigung).
        pub typed: RefCell<String>,
        /// Die Leiste schreibt gerade selbst ins Eingabefeld.
        pub completing: Cell<bool>,
        pub on_jump: RefCell<Option<JumpCallback>>,
        pub on_done: RefCell<Option<DoneCallback>>,
    }

    impl LexJumpBar {
        fn set_compact(&self, compact: bool) {
            self.compact.set(compact);
            if let Some(preview) = self.preview.get() {
                preview.set_visible(!compact && !self.hits.borrow().is_empty());
            }
            self.obj().update_context_visibility();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexJumpBar {
        const NAME: &'static str = "LexJumpBar";
        type Type = super::LexJumpBar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for LexJumpBar {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.build_popover();
            obj.setup_entry();
            obj.setup_context_position();
        }

        fn dispose(&self) {
            // Das Popover hängt direkt an der Leiste und muss vor dem
            // Zerstören abgehängt werden („finalized with children left“).
            if let Some(popover) = self.popover.get() {
                popover.unparent();
            }
        }
    }

    impl WidgetImpl for LexJumpBar {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if let Some(popover) = self.popover.get() {
                popover.present();
            }
        }

        fn unroot(&self) {
            if let Some(popover) = self.popover.get() {
                popover.popdown();
            }
            self.parent_unroot();
        }
    }

    impl BinImpl for LexJumpBar {}
}

glib::wrapper! {
    pub struct LexJumpBar(ObjectSubclass<imp::LexJumpBar>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexJumpBar {
    fn default() -> Self {
        Self::new()
    }
}

impl LexJumpBar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Gesetz, dessen Treffer bei gleicher Güte vorn stehen.
    pub fn set_preferred_law(&self, law_id: Option<i64>) {
        self.imp().preferred_law.set(law_id);
    }

    /// Hinweis auf die gerade gelesene Norm, rechts im leeren Feld.
    pub fn set_context(&self, title: &str, subtitle: &str) {
        let label = &self.imp().context_label;
        let text = match (title.is_empty(), subtitle.is_empty()) {
            (true, _) => String::new(),
            (false, true) => title.to_owned(),
            (false, false) => format!("{title} · {subtitle}"),
        };
        label.set_text(&text);
        label.set_tooltip_text(Some(&text).filter(|t| !t.is_empty()).map(|t| t.as_str()));
        self.update_context_visibility();
    }

    /// Setzt den Fokus in die Leiste und markiert vorhandenen Text.
    pub fn activate_search(&self) {
        let entry = &self.imp().entry;
        entry.grab_focus();
        entry.select_region(0, -1);
        if !entry.text().is_empty() {
            self.run_search(entry.text().as_str(), false);
        }
    }

    /// Belegt die Leiste vor und sucht sofort (Kontextmenü, Tests).
    pub fn set_query(&self, query: &str) {
        let imp = self.imp();
        imp.entry.grab_focus();
        self.set_text_silently(query);
        imp.entry.set_position(-1);
        *imp.typed.borrow_mut() = query.to_owned();
        self.run_search(query, false);
    }

    pub fn connect_jump(&self, f: impl Fn(i64, bool) + 'static) {
        *self.imp().on_jump.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_done(&self, f: impl Fn() + 'static) {
        *self.imp().on_done.borrow_mut() = Some(Box::new(f));
    }

    // ------------------------------------------------------------------
    // Aufbau
    // ------------------------------------------------------------------

    fn build_popover(&self) {
        let imp = self.imp();
        let results = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .can_focus(false)
            .css_classes(["navigation-sidebar"])
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .width_request(LIST_WIDTH)
            .child(&results)
            .build();

        let preview_title = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();
        let preview_subtitle = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let preview_body = gtk::Label::builder()
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .vexpand(true)
            .css_classes(["jump-preview-body"])
            .build();
        let preview = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(PREVIEW_WIDTH)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(14)
            .margin_end(14)
            .visible(false)
            .css_classes(["jump-preview"])
            .build();
        preview.append(&preview_title);
        preview.append(&preview_subtitle);
        preview.append(&preview_body);

        let columns = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        columns.append(&scrolled);
        columns.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        columns.append(&preview);
        // Trennlinie nur zusammen mit der Vorschau.
        if let Some(separator) = preview.prev_sibling() {
            preview
                .bind_property("visible", &separator, "visible")
                .sync_create()
                .build();
        }

        let hint = gtk::Label::builder()
            .label(gettext(
                "Eingabe öffnet · Strg+Eingabe in der zweiten Ansicht · Tab vervollständigt",
            ))
            .wrap(true)
            .xalign(0.0)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["dim-label", "caption"])
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&columns);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&hint);

        // Ohne Autohide nimmt das Popover dem Eingabefeld nicht den Fokus.
        let popover = gtk::Popover::builder()
            .autohide(false)
            .has_arrow(false)
            .can_focus(false)
            .position(gtk::PositionType::Bottom)
            .child(&content)
            .css_classes(["jump-popover"])
            .build();
        popover.set_parent(self);

        results.connect_row_activated(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |list, row| {
                list.select_row(Some(row));
                bar.jump_selected(false);
            }
        ));
        results.connect_row_selected(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_, row| bar.show_preview(row.map(|r| r.index()))
        ));

        imp.popover.set(popover).ok();
        imp.results.set(results).ok();
        imp.preview.set(preview).ok();
        imp.preview_title.set(preview_title).ok();
        imp.preview_subtitle.set(preview_subtitle).ok();
        imp.preview_body.set(preview_body).ok();
    }

    /// Der Normhinweis darf nur den Platz rechts vom Platzhalter belegen:
    /// Er wird auf die freie Breite gekürzt und bei zu wenig Platz
    /// ausgeblendet, damit beide Texte nicht ineinanderlaufen.
    fn setup_context_position(&self) {
        let imp = self.imp();
        let entry = imp.entry.clone();
        imp.overlay
            .connect_get_child_position(move |overlay, child| {
                let width = overlay.width();
                let height = overlay.height();
                let placeholder = entry.placeholder_text().unwrap_or_default();
                // Platzhalter kursiv setzen wie im CSS, sonst ist er zu schmal.
                let layout = entry.create_pango_layout(Some(&placeholder));
                let mut font = layout.context().font_description().unwrap_or_default();
                font.set_style(gtk::pango::Style::Italic);
                layout.set_font_description(Some(&font));
                let (placeholder_width, _) = layout.pixel_size();
                let free = width - ICON_SPACE - placeholder_width - CONTEXT_GAP;

                let (min, natural, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
                let child_width = natural.min(free).max(min);
                child.set_child_visible(free >= CONTEXT_MIN_WIDTH.max(min));
                let (_, child_height, _, _) =
                    child.measure(gtk::Orientation::Vertical, child_width);
                let child_height = child_height.min(height);
                Some(gdk::Rectangle::new(
                    width - child_width,
                    (height - child_height) / 2,
                    child_width,
                    child_height,
                ))
            });
    }

    fn setup_entry(&self) {
        let imp = self.imp();
        imp.entry.connect_changed(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |entry| bar.on_changed(entry)
        ));
        imp.entry.connect_activate(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| bar.jump_selected(false)
        ));

        // Pfeiltasten, Tab, Escape und Strg+Eingabe vor dem Eingabefeld abfangen.
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, state| bar.on_key(key, state)
        ));
        imp.entry.add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| {
                bar.update_context_visibility();
                if !bar.imp().hits.borrow().is_empty() {
                    bar.popup();
                }
            }
        ));
        focus.connect_leave(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            move |_| {
                bar.update_context_visibility();
                // Kurz warten: Ein Klick in die Trefferliste soll noch ankommen.
                glib::timeout_add_local_once(
                    Duration::from_millis(150),
                    glib::clone!(
                        #[weak]
                        bar,
                        move || {
                            bar.update_context_visibility();
                            if !bar.entry_focused() {
                                bar.popdown();
                            }
                        }
                    ),
                );
            }
        ));
        imp.entry.add_controller(focus);
    }

    // ------------------------------------------------------------------
    // Eingabe
    // ------------------------------------------------------------------

    fn on_key(&self, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let imp = self.imp();
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        match key {
            gdk::Key::Down => {
                self.move_selection(1);
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                self.move_selection(-1);
                glib::Propagation::Stop
            }
            gdk::Key::Page_Down => {
                self.move_selection(8);
                glib::Propagation::Stop
            }
            gdk::Key::Page_Up => {
                self.move_selection(-8);
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter if ctrl => {
                self.jump_selected(true);
                glib::Propagation::Stop
            }
            gdk::Key::Tab if !imp.hits.borrow().is_empty() => {
                self.accept_completion();
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if imp.entry.text().is_empty() || !self.is_popped_up() {
                    self.finish();
                } else {
                    self.clear();
                }
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    }

    fn on_changed(&self, entry: &gtk::Entry) {
        let imp = self.imp();
        self.update_context_visibility();
        if imp.completing.get() {
            return;
        }
        let text = entry.text().to_string();
        // Nur beim Weitertippen am Ende vervollständigen, nicht beim Löschen.
        let previous = imp.typed.replace(text.clone());
        let appended =
            text.chars().count() > previous.chars().count() && text.starts_with(previous.as_str());
        self.run_search(&text, appended);
    }

    /// Schreibt Text, ohne eine neue Suche auszulösen.
    fn set_text_silently(&self, text: &str) {
        let imp = self.imp();
        imp.completing.set(true);
        imp.entry.set_text(text);
        imp.completing.set(false);
    }

    /// Übernimmt die Vervollständigung bzw. den ausgewählten Treffer.
    fn accept_completion(&self) {
        let imp = self.imp();
        let text = imp.entry.text().to_string();
        let hit_text = self
            .selected_index()
            .and_then(|i| imp.hits.borrow().get(i).map(completion_text));
        let accepted = match hit_text {
            // Ausgewählter Treffer passt nicht zur Eingabe: seinen Namen einsetzen.
            Some(full) if imp.entry.selection_bounds().is_none() => full,
            _ => text,
        };
        self.set_text_silently(&accepted);
        imp.entry.set_position(-1);
        *imp.typed.borrow_mut() = accepted.clone();
        self.run_search(&accepted, false);
    }

    fn move_selection(&self, delta: i32) {
        let imp = self.imp();
        let count = imp.hits.borrow().len() as i32;
        let Some(results) = imp.results.get() else {
            return;
        };
        if count == 0 {
            return;
        }
        if !self.is_popped_up() {
            self.popup();
        }
        let current = self
            .selected_index()
            .map(|i| i as i32)
            .unwrap_or(if delta > 0 { -1 } else { 0 });
        let next = (current + delta).clamp(0, count - 1);
        if let Some(row) = results.row_at_index(next) {
            results.select_row(Some(&row));
            scroll_row_into_view(results, &row);
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.imp()
            .results
            .get()?
            .selected_row()
            .and_then(|r| usize::try_from(r.index()).ok())
    }

    /// Springt zum ausgewählten (sonst ersten) Treffer.
    fn jump_selected(&self, other_pane: bool) {
        let imp = self.imp();
        let index = self.selected_index().unwrap_or(0);
        let Some(norm_id) = imp.hits.borrow().get(index).map(|h| h.norm.id) else {
            return;
        };
        if let Some(cb) = imp.on_jump.borrow().as_ref() {
            cb(norm_id, other_pane);
        }
        self.clear();
        self.finish();
    }

    /// Leert die Leiste und schließt die Trefferliste.
    fn clear(&self) {
        let imp = self.imp();
        imp.serial.set(imp.serial.get().wrapping_add(1));
        self.set_text_silently("");
        imp.typed.borrow_mut().clear();
        self.show_hits(&[]);
    }

    /// Gibt den Fokus zurück an die Leseansicht.
    fn finish(&self) {
        self.popdown();
        if let Some(cb) = self.imp().on_done.borrow().as_ref() {
            cb();
        }
    }

    // ------------------------------------------------------------------
    // Suche und Anzeige
    // ------------------------------------------------------------------

    fn run_search(&self, input: &str, complete: bool) {
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
            #[weak(rename_to = bar)]
            self,
            async move {
                let path = Database::default_path();
                let query = input.clone();
                let result =
                    gio::spawn_blocking(move || -> Result<Vec<QuickHit>, rusqlite::Error> {
                        Database::open(&path)?.quick_search(&query, preferred, LIMIT)
                    })
                    .await;
                if bar.imp().serial.get() != serial {
                    return;
                }
                match result {
                    Ok(Ok(hits)) => {
                        bar.show_hits(&hits);
                        if complete {
                            bar.complete_inline();
                        }
                    }
                    Ok(Err(err)) => log::warn!("Sprungsuche fehlgeschlagen: {err}"),
                    Err(_) => log::warn!("Sprungsuche abgebrochen"),
                }
            }
        ));
    }

    /// Ergänzt die Eingabe um den Rest des besten Treffers und markiert ihn,
    /// sodass Weitertippen ihn ersetzt.
    fn complete_inline(&self) {
        let imp = self.imp();
        let typed = imp.typed.borrow().clone();
        // Cursor muss am Ende stehen (beim Ändern mitten im Text nichts ergänzen).
        let at_end = imp.entry.position() == typed.chars().count() as i32;
        if imp.entry.text().as_str() != typed || !at_end || !self.entry_focused() {
            return;
        }
        let Some(completed) = imp
            .hits
            .borrow()
            .first()
            .and_then(|hit| complete(&typed, hit))
        else {
            return;
        };
        let start = typed.chars().count() as i32;
        self.set_text_silently(&completed);
        imp.entry.select_region(start, -1);
    }

    fn show_hits(&self, hits: &[QuickHit]) {
        let imp = self.imp();
        let Some(results) = imp.results.get() else {
            return;
        };
        results.remove_all();
        for hit in hits {
            let (title, subtitle) = hit_labels(hit);
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&title))
                .subtitle(glib::markup_escape_text(&subtitle))
                .subtitle_lines(1)
                .activatable(true)
                .focusable(false)
                .build();
            results.append(&row);
        }
        *imp.hits.borrow_mut() = hits.to_vec();
        if let Some(preview) = imp.preview.get() {
            preview.set_visible(!imp.compact.get() && !hits.is_empty());
        }
        if let Some(first) = results.row_at_index(0) {
            results.select_row(Some(&first));
        }
        if hits.is_empty() {
            self.popdown();
        } else if self.entry_focused() {
            self.popup();
        }
    }

    fn show_preview(&self, index: Option<i32>) {
        let imp = self.imp();
        let hit = index
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| imp.hits.borrow().get(i).cloned());
        let (Some(title), Some(subtitle), Some(body)) = (
            imp.preview_title.get(),
            imp.preview_subtitle.get(),
            imp.preview_body.get(),
        ) else {
            return;
        };
        let Some(hit) = hit else {
            title.set_text("");
            subtitle.set_text("");
            body.set_text("");
            return;
        };
        let (t, s) = hit_labels(&hit);
        title.set_text(&t);
        subtitle.set_text(&s);
        subtitle.set_visible(!s.is_empty());

        let norm_id = hit.norm.id;
        if let Some(text) = imp.previews.borrow().get(&norm_id) {
            body.set_text(text);
            return;
        }
        body.set_text("");
        if imp.compact.get() {
            return;
        }
        let serial = imp.preview_serial.get().wrapping_add(1);
        imp.preview_serial.set(serial);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            async move {
                let path = Database::default_path();
                let result =
                    gio::spawn_blocking(move || Database::open(&path)?.paragraphs(norm_id)).await;
                let imp = bar.imp();
                let text = match result {
                    Ok(Ok(paragraphs)) => preview_text(&paragraphs, PREVIEW_CHARS),
                    _ => return,
                };
                imp.previews.borrow_mut().insert(norm_id, text.clone());
                if imp.preview_serial.get() == serial {
                    if let Some(body) = imp.preview_body.get() {
                        body.set_text(&text);
                    }
                }
            }
        ));
    }

    fn popup(&self) {
        let imp = self.imp();
        if let Some(popover) = imp.popover.get() {
            if !popover.is_visible() {
                popover.popup();
            }
        }
    }

    fn popdown(&self) {
        if let Some(popover) = self.imp().popover.get() {
            popover.popdown();
        }
    }

    /// Den Fokus hält das innere `GtkText`, nicht das `GtkEntry` selbst.
    fn entry_focused(&self) -> bool {
        self.imp()
            .entry
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN)
    }

    fn is_popped_up(&self) -> bool {
        self.imp().popover.get().is_some_and(|p| p.is_visible())
    }

    /// Der Hinweis auf die aktuelle Norm erscheint nur im leeren, nicht
    /// fokussierten Feld und nicht in schmaler Darstellung.
    fn update_context_visibility(&self) {
        let imp = self.imp();
        let label = &imp.context_label;
        label.set_visible(
            !imp.compact.get()
                && !self.entry_focused()
                && imp.entry.text().is_empty()
                && !label.text().is_empty(),
        );
    }
}

/// Scrollt die Trefferliste so, dass `row` sichtbar ist.
fn scroll_row_into_view(list: &gtk::ListBox, row: &gtk::ListBoxRow) {
    let Some(scrolled) = list
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
    else {
        return;
    };
    let Some(bounds) = row.compute_bounds(list) else {
        return;
    };
    let adj = scrolled.vadjustment();
    let top = f64::from(bounds.y());
    let bottom = top + f64::from(bounds.height());
    if top < adj.value() {
        adj.set_value(top);
    } else if bottom > adj.value() + adj.page_size() {
        adj.set_value(bottom - adj.page_size());
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

/// Text, mit dem Tab einen Treffer übernimmt: „§ 433 BGB“.
fn completion_text(hit: &QuickHit) -> String {
    hit_labels(hit).0
}

/// Mögliche Schreibweisen eines Treffers, deren Anfang die Eingabe sein kann.
fn completion_candidates(hit: &QuickHit) -> Vec<String> {
    let abbrev = &hit.law_abbrev;
    let mut out = Vec::new();
    if let Some(enbez) = &hit.norm.enbez {
        out.push(format!("{enbez} {abbrev}"));
        out.push(format!("{abbrev} {enbez}"));
        // Bezeichnung ohne Zeichen: „433 BGB“, „BGB 433“.
        let bare = enbez
            .trim_start_matches(['§', ' '])
            .trim_start_matches("Art")
            .trim_start_matches(['.', ' ']);
        if bare != enbez.as_str() && !bare.is_empty() {
            out.push(format!("{bare} {abbrev}"));
            out.push(format!("{abbrev} {bare}"));
        }
    }
    if let Some(titel) = &hit.norm.titel {
        out.push(titel.clone());
    }
    out
}

/// Vervollständigt `typed` mit der ersten Schreibweise von `hit`, die mit
/// der Eingabe beginnt (ohne Rücksicht auf Groß-/Kleinschreibung). Die
/// getippten Zeichen bleiben unverändert.
fn complete(typed: &str, hit: &QuickHit) -> Option<String> {
    let typed_len = typed.chars().count();
    if typed_len == 0 {
        return None;
    }
    let typed_lower = typed.to_lowercase();
    completion_candidates(hit)
        .into_iter()
        .find_map(|candidate| {
            let head: String = candidate.chars().take(typed_len).collect();
            let longer = candidate.chars().count() > typed_len;
            (longer && head.to_lowercase() == typed_lower).then(|| {
                let rest: String = candidate.chars().skip(typed_len).collect();
                format!("{typed}{rest}")
            })
        })
}

/// Vorschautext aus den Absätzen einer Norm, auf `max` Zeichen gekürzt.
/// Tabellen (U+FFFC) werden durch einen Hinweis ersetzt.
fn preview_text(paragraphs: &[(i64, Option<String>, String)], max: usize) -> String {
    let table = gettext("[Tabelle]");
    let mut out = String::new();
    for (_, label, text) in paragraphs {
        let text = text.replace('\u{FFFC}', &table);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        if let Some(label) = label.as_deref().filter(|l| !text.starts_with(l)) {
            out.push_str(label);
            out.push(' ');
        }
        out.push_str(text);
        if out.chars().count() >= max {
            break;
        }
    }
    if out.chars().count() > max {
        let cut: String = out.chars().take(max).collect();
        let cut = cut
            .rsplit_once(char::is_whitespace)
            .map(|(head, _)| head)
            .unwrap_or(&cut);
        return format!("{} …", cut.trim_end());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NormInfo;

    fn hit(enbez: &str, titel: &str, abbrev: &str) -> QuickHit {
        QuickHit {
            norm: NormInfo {
                enbez: Some(enbez.into()),
                titel: Some(titel.into()),
                ..Default::default()
            },
            law_slug: abbrev.to_lowercase(),
            law_abbrev: abbrev.into(),
        }
    }

    #[test]
    fn labels() {
        assert_eq!(
            hit_labels(&hit("§ 253", "Klageschrift", "ZPO")),
            ("§ 253 ZPO".into(), "Klageschrift".into())
        );
    }

    #[test]
    fn inline_completion() {
        let kauf = hit(
            "§ 433",
            "Vertragstypische Pflichten beim Kaufvertrag",
            "BGB",
        );
        assert_eq!(complete("43", &kauf).as_deref(), Some("433 BGB"));
        assert_eq!(complete("§ 43", &kauf).as_deref(), Some("§ 433 BGB"));
        assert_eq!(complete("bgb 4", &kauf).as_deref(), Some("bgb 433"));
        assert_eq!(
            complete("vertragsty", &kauf).as_deref(),
            Some("vertragstypische Pflichten beim Kaufvertrag")
        );
        // Vollständige Eingabe oder fremder Anfang: nichts ergänzen.
        assert_eq!(complete("433 BGB", &kauf), None);
        assert_eq!(complete("kauf", &kauf), None);
        assert_eq!(complete("", &kauf), None);

        let art = hit("Art 229", "Weitere Überleitungsvorschriften", "BGBEG");
        assert_eq!(complete("229", &art).as_deref(), Some("229 BGBEG"));
        assert_eq!(complete("Art 2", &art).as_deref(), Some("Art 229 BGBEG"));
    }

    #[test]
    fn preview_is_shortened() {
        let paragraphs = vec![
            (
                1,
                Some("(1)".to_owned()),
                "(1) Durch den Kaufvertrag wird der Verkäufer verpflichtet.".to_owned(),
            ),
            (2, None, "Tabelle: \u{FFFC}".to_owned()),
        ];
        let full = preview_text(&paragraphs, 500);
        assert_eq!(
            full,
            "(1) Durch den Kaufvertrag wird der Verkäufer verpflichtet.\nTabelle: [Tabelle]"
        );
        let short = preview_text(&paragraphs, 20);
        assert_eq!(short, "(1) Durch den …");
    }
}
