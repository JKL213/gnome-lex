// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Normansicht: ein `GtkTextView` mit Text-Tags für Überschriften,
//! Absatznummern, Listen, Fußnoten und Auszeichnungen. Tabellen werden als
//! `GtkGrid` an einem `GtkTextChildAnchor` eingebettet.
//!
//! Die Ansicht zeigt fortlaufend: Beim Scrollen ans Ende fordert sie über
//! [`LexNormView::connect_need_more`] die nächste Norm an und hängt sie als
//! weiteren Abschnitt an; beim Scrollen über den Anfang hinaus die vorherige.
//! Welche Norm gerade oben sichtbar ist, meldet
//! [`LexNormView::connect_visible_changed`].
//!
//! Der Normtext wird mit [`flatten_blocks`] linearisiert – derselben
//! Funktion, mit der der Importer die Absatztexte in die Datenbank schreibt.
//! Dadurch stimmen Zeichenoffsets im Puffer (ab [`LexNormView::content_start`]
//! plus Blockanfang) mit den Offsets der Annotationen überein.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashSet;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use glib::translate::IntoGlib;
use gtk::{glib, pango, CompositeTemplate};

use crate::model::text::{
    flatten_blocks, paragraph_label, spans_text, Block, Flattened, Seg, SegTag, TableData,
};
use crate::model::{Annotation, AnnotationKind, Norm, NormInfo, UnitInfo, HIGHLIGHT_COLORS};
use crate::refs::{find_references, NormRef};

/// Einzug je Listenebene in Pixeln.
const LIST_INDENT: i32 = 36;
/// Nachladen beginnt, sobald weniger als so viele Seiten Text unter dem
/// sichtbaren Bereich liegen.
const PRELOAD_PAGES: f64 = 1.5;
/// Ab so vielen Zeilen wird eine Tabelle nicht mehr als Raster gebaut.
const MAX_GRID_ROWS: usize = 150;
/// Tiefste Listenebene, für die eigene Tags angelegt werden.
const MAX_LIST_DEPTH: u8 = 5;
/// Standardschriftgröße in Punkt (entspricht dem GSettings-Standard).
const DEFAULT_FONT_SIZE: i32 = 12;

/// Ziel eines klickbaren Bereichs im Text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Name der Textmarke der Fußnote.
    Footnote(String),
    /// Verweis auf eine andere Norm („§ 280 Abs. 1“, „Art. 229 EGBGB“).
    Reference(NormRef),
}

/// Alles, was die Ansicht für einen Normabschnitt braucht.
#[derive(Debug, Clone)]
pub struct NormPage {
    pub norm: Norm,
    /// Gliederungspfad (Buch, Abschnitt, …) der Norm, Wurzel zuerst.
    pub units: Vec<UnitInfo>,
    /// Kürzel des Gesetzes (für die Verweiserkennung).
    pub law_abbrev: String,
    /// Eigene Notiz (Schema) zur Norm.
    pub note: Option<String>,
    /// Im Text verankerte Markierungen und Notizen.
    pub annotations: Vec<Annotation>,
}

/// Änderungswunsch an Annotationen, den die Ansicht nach außen meldet.
/// Bei `Create` ist `law` noch leer; der Tab ergänzt den Slug.
#[derive(Debug, Clone)]
pub enum AnnotationEvent {
    Create(Annotation),
    Update(Annotation),
    Delete(i64),
}

/// Eine im Text verankerte Annotation mit ihren Marken: `start`/`end`
/// umfassen den Wortlaut; bei Notizen zusätzlich den bearbeitbaren
/// Inline-Block (`block_start`/`block_end`) hinter dem Absatz.
pub struct Anchored {
    annotation: Annotation,
    start: gtk::TextMark,
    end: gtk::TextMark,
    block_start: Option<gtk::TextMark>,
    block_end: Option<gtk::TextMark>,
}

/// Präfix eines Inline-Notizblocks (nicht bearbeitbar).
const NOTE_GLYPH: &str = "✎ ";
/// Verzögerung, nach der Änderungen an Inline-Notizen gemeldet werden.
const NOTE_EDIT_DELAY_MS: u64 = 700;

/// Ein im Puffer dargestellter Normabschnitt.
pub struct Section {
    norm: NormInfo,
    /// IDs des Gliederungspfads (für die Überschriften beim Nachladen).
    unit_ids: Vec<i64>,
    /// Marke am Abschnittsanfang (bleibt beim Einfügen davor stehen).
    start: gtk::TextMark,
    /// Bereich der Notiz zwischen Überschrift und Normtext.
    note_start: gtk::TextMark,
    note_end: gtk::TextMark,
    /// Zeichenoffset, an dem der Normtext (Block 0) beginnt – relativ
    /// zum Abschnittsanfang.
    content_offset: i32,
    /// Marke am Anfang jedes Blocks (Rechtsgravitation: Inline-Notizen am
    /// Blockende rutschen davor) und Länge des Blocktexts in Zeichen.
    block_marks: Vec<gtk::TextMark>,
    block_lens: Vec<i32>,
    /// Ende des Normtexts (Linksgravitation, vor den Fußnoten).
    content_end: gtk::TextMark,
}

type AnnotationCallback = Box<dyn Fn(&LexNormView, AnnotationEvent)>;
type NeedMoreCallback = Box<dyn Fn(&LexNormView, i64, i64, u64)>;
type VisibleChangedCallback = Box<dyn Fn(&LexNormView, NormInfo)>;
type NavigateCallback = Box<dyn Fn(&LexNormView, NormRef, bool)>;

#[derive(Debug, Clone)]
pub struct LinkRange {
    start: i32,
    end: i32,
    target: LinkTarget,
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/norm_view.ui")]
    pub struct LexNormView {
        #[template_child]
        pub scrolled: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
        pub tags: OnceCell<gtk::TextTagTable>,
        /// Abschnitte in Pufferreihenfolge.
        pub sections: RefCell<Vec<Section>>,
        pub font_size: Cell<i32>,
        pub links: RefCell<Vec<LinkRange>>,
        pub table_labels: RefCell<Vec<gtk::Label>>,
        pub hovering_link: Cell<bool>,
        /// Zählt jedes `set_norm`; Nachladeantworten älterer Generationen
        /// werden verworfen.
        pub generation: Cell<u64>,
        pub loading_next: Cell<bool>,
        pub loading_prev: Cell<bool>,
        pub end_next: Cell<bool>,
        pub end_prev: Cell<bool>,
        /// Zuletzt gemeldete oben sichtbare Norm (0 = keine).
        pub visible_norm: Cell<i64>,
        /// Während `render_section` erfasste Tag-Bereiche (Offset von, bis,
        /// Tags), um sie beim Voranstellen sauber neu anzuwenden: Text, der
        /// vor einem Tag-Anfang eingefügt wird, erbt sonst dessen Tags.
        pub pending_tags: RefCell<Vec<(i32, i32, Vec<gtk::TextTag>)>>,
        pub on_need_more: RefCell<Option<NeedMoreCallback>>,
        pub on_visible_changed: RefCell<Option<VisibleChangedCallback>>,
        pub on_navigate: RefCell<Option<NavigateCallback>>,
        pub on_annotation: RefCell<Option<AnnotationCallback>>,
        pub anchored: RefCell<Vec<Anchored>>,
        /// Während die Ansicht selbst in den Puffer schreibt.
        pub rendering: Cell<bool>,
        pub note_edit_timer: RefCell<Option<glib::SourceId>>,
        /// Popover für Auswahl bzw. angeklickte Markierung.
        pub selection_popover: OnceCell<gtk::Popover>,
        pub remove_button: OnceCell<gtk::Button>,
        pub note_button: OnceCell<gtk::Widget>,
        /// Annotation, auf die sich das Popover bezieht (None = Textauswahl).
        pub popover_target: Cell<Option<i64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexNormView {
        const NAME: &'static str = "LexNormView";
        type Type = super::LexNormView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LexNormView {
        fn constructed(&self) {
            self.parent_constructed();
            self.font_size.set(DEFAULT_FONT_SIZE);
            let obj = self.obj();
            self.tags.set(obj.build_tag_table()).ok();
            obj.setup_controllers();
            obj.setup_selection_popover();
            obj.reset_buffer();
        }
    }

    impl WidgetImpl for LexNormView {}
    impl BinImpl for LexNormView {}
}

glib::wrapper! {
    pub struct LexNormView(ObjectSubclass<imp::LexNormView>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexNormView {
    fn default() -> Self {
        Self::new()
    }
}

impl LexNormView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Zeigt eine Norm an (oder leert die Ansicht) und beginnt damit einen
    /// neuen fortlaufenden Lesefluss.
    pub fn set_norm(&self, page: Option<NormPage>) {
        let imp = self.imp();
        imp.generation.set(imp.generation.get().wrapping_add(1));
        imp.loading_next.set(false);
        imp.loading_prev.set(false);
        imp.end_next.set(false);
        imp.end_prev.set(false);
        self.reset_buffer();
        match page {
            Some(page) => {
                imp.visible_norm.set(page.norm.info.id);
                self.render_section(page, false);
                self.scroll_to_top();
                self.schedule_edge_check();
            }
            None => imp.visible_norm.set(0),
        }
    }

    /// Hängt die nächste Norm als Abschnitt an (Antwort auf `need_more`).
    pub fn append_norm(&self, page: NormPage, generation: u64) {
        let imp = self.imp();
        if imp.generation.get() != generation {
            return;
        }
        imp.loading_next.set(false);
        self.render_section(page, false);
        self.schedule_edge_check();
    }

    /// Stellt die vorherige Norm als Abschnitt voran; GTK hält dabei den
    /// sichtbaren Text an seiner Position.
    pub fn prepend_norm(&self, page: NormPage, generation: u64) {
        let imp = self.imp();
        if imp.generation.get() != generation {
            return;
        }
        imp.loading_prev.set(false);
        self.render_section(page, true);
        self.schedule_edge_check();
    }

    /// In Richtung `direction` (-1/+1) gibt es keine weitere Norm.
    pub fn set_end_reached(&self, direction: i64, generation: u64) {
        let imp = self.imp();
        if imp.generation.get() != generation {
            return;
        }
        if direction < 0 {
            imp.loading_prev.set(false);
            imp.end_prev.set(true);
        } else {
            imp.loading_next.set(false);
            imp.end_next.set(true);
        }
    }

    /// Wird gerufen, wenn weitere Normen gebraucht werden:
    /// (Richtung -1/+1, ID der Norm am Rand, Generation).
    pub fn connect_need_more(&self, f: impl Fn(&Self, i64, i64, u64) + 'static) {
        *self.imp().on_need_more.borrow_mut() = Some(Box::new(f));
    }

    /// Wird gerufen, wenn eine andere Norm oben im Sichtbereich liegt.
    pub fn connect_visible_changed(&self, f: impl Fn(&Self, NormInfo) + 'static) {
        *self.imp().on_visible_changed.borrow_mut() = Some(Box::new(f));
    }

    /// Wird gerufen, wenn ein Verweis angeklickt wurde (`true` = neuer Tab).
    pub fn connect_navigate(&self, f: impl Fn(&Self, NormRef, bool) + 'static) {
        *self.imp().on_navigate.borrow_mut() = Some(Box::new(f));
    }

    /// Wird gerufen, wenn der Leser eine Markierung oder Notiz anlegt,
    /// ändert oder löscht; die Persistenz übernimmt das Fenster.
    pub fn connect_annotation(&self, f: impl Fn(&Self, AnnotationEvent) + 'static) {
        *self.imp().on_annotation.borrow_mut() = Some(Box::new(f));
    }

    fn emit_annotation(&self, event: AnnotationEvent) {
        if let Some(cb) = self.imp().on_annotation.borrow().as_ref() {
            cb(self, event);
        }
    }

    /// Zeigt eine (gespeicherte) Annotation im passenden Abschnitt an;
    /// eine vorhandene mit derselben ID wird ersetzt. Stimmt bei einer
    /// Notiz der Text im Puffer bereits (eigene Bearbeitung), bleibt der
    /// Block stehen.
    pub fn apply_annotation(&self, law_id: i64, annotation: &Annotation) {
        let imp = self.imp();
        let unchanged = imp.anchored.borrow().iter().any(|a| {
            a.annotation.id == annotation.id
                && a.annotation.color == annotation.color
                && a.annotation.kind == annotation.kind
                && self.inline_note_text(a).as_deref()
                    == Some(annotation.note.as_deref().unwrap_or(""))
        });
        if unchanged {
            for a in imp.anchored.borrow_mut().iter_mut() {
                if a.annotation.id == annotation.id {
                    a.annotation = annotation.clone();
                }
            }
            return;
        }
        self.remove_annotation(annotation.id);
        let index = imp.sections.borrow().iter().position(|s| {
            s.norm.law_id == law_id && s.norm.enbez.as_deref() == Some(annotation.norm.as_str())
        });
        let Some(index) = index else {
            return;
        };
        let fresh = annotation.kind == AnnotationKind::Note
            && annotation.note.as_deref().unwrap_or("").is_empty();
        self.place_annotation(index, annotation);
        if fresh {
            self.focus_note(annotation.id);
        }
    }

    /// Entfernt die Darstellung einer Annotation (Tags, Inline-Block, Marken).
    pub fn remove_annotation(&self, id: i64) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let position = imp
            .anchored
            .borrow()
            .iter()
            .position(|a| a.annotation.id == id);
        let Some(index) = position else {
            return;
        };
        let item = imp.anchored.borrow_mut().remove(index);
        let from = buffer.iter_at_mark(&item.start);
        let to = buffer.iter_at_mark(&item.end);
        for (name, _, _, _) in HIGHLIGHT_COLORS {
            for prefix in ["hl-", "note-"] {
                if let Some(tag) = self.tag(&format!("{prefix}{name}")) {
                    buffer.remove_tag(&tag, &from, &to);
                }
            }
        }
        buffer.delete_mark(&item.start);
        buffer.delete_mark(&item.end);
        if let (Some(block_start), Some(block_end)) = (item.block_start, item.block_end) {
            let a = buffer.iter_at_mark(&block_start).offset();
            let b = buffer.iter_at_mark(&block_end).offset();
            imp.rendering.set(true);
            buffer.delete(&mut buffer.iter_at_offset(a), &mut buffer.iter_at_offset(b));
            imp.rendering.set(false);
            self.shift_links(a, a - b);
            buffer.delete_mark(&block_start);
            buffer.delete_mark(&block_end);
        }
    }

    /// Verschiebt gespeicherte Verweis-Offsets ab `from` um `delta`.
    fn shift_links(&self, from: i32, delta: i32) {
        if delta == 0 {
            return;
        }
        for link in self.imp().links.borrow_mut().iter_mut() {
            if link.start >= from {
                link.start += delta;
                link.end += delta;
            }
        }
    }

    /// Ersetzt die Notiz (Schema) eines dargestellten Abschnitts.
    pub fn set_note(&self, law_id: i64, enbez: &str, text: Option<&str>) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let found = imp
            .sections
            .borrow()
            .iter()
            .position(|s| s.norm.law_id == law_id && s.norm.enbez.as_deref() == Some(enbez));
        let Some(index) = found else {
            return;
        };
        let (note_start, note_end) = {
            let sections = imp.sections.borrow();
            (
                sections[index].note_start.clone(),
                sections[index].note_end.clone(),
            )
        };
        let from = buffer.iter_at_mark(&note_start).offset();
        let old_len = buffer.iter_at_mark(&note_end).offset() - from;
        buffer.delete(
            &mut buffer.iter_at_mark(&note_start),
            &mut buffer.iter_at_mark(&note_end),
        );
        imp.pending_tags.borrow_mut().clear();
        let at = buffer.create_mark(None, &buffer.iter_at_mark(&note_start), false);
        self.insert_note(&buffer, &at, text);
        let to = buffer.iter_at_mark(&at).offset();
        buffer.remove_all_tags(&buffer.iter_at_offset(from), &buffer.iter_at_offset(to));
        for (a, b, tags) in imp.pending_tags.borrow().iter() {
            for tag in tags {
                buffer.apply_tag(tag, &buffer.iter_at_offset(*a), &buffer.iter_at_offset(*b));
            }
        }
        imp.pending_tags.borrow_mut().clear();
        if let Some(base) = self.tag("base") {
            buffer.apply_tag(
                &base,
                &buffer.iter_at_offset(from),
                &buffer.iter_at_offset(to),
            );
        }
        buffer.move_mark(&note_end, &buffer.iter_at_mark(&at));
        buffer.delete_mark(&at);
        let delta = (to - from) - old_len;
        if delta != 0 {
            imp.sections.borrow_mut()[index].content_offset += delta;
            for link in imp.links.borrow_mut().iter_mut() {
                if link.start >= from {
                    link.start += delta;
                    link.end += delta;
                }
            }
        }
    }

    /// ID der oben sichtbaren Norm.
    pub fn norm_id(&self) -> Option<i64> {
        Some(self.imp().visible_norm.get()).filter(|id| *id != 0)
    }

    /// Zeichenoffset im Puffer, an dem der Text der ersten Norm beginnt.
    pub fn content_start(&self) -> i32 {
        let imp = self.imp();
        let sections = imp.sections.borrow();
        let Some(first) = sections.first() else {
            return 0;
        };
        let buffer = imp.text_view.buffer();
        buffer.iter_at_mark(&first.start).offset() + first.content_offset
    }

    /// Schriftgröße in Punkt; wirkt sofort auf Text und Tabellen.
    pub fn set_font_size(&self, points: i32) {
        let imp = self.imp();
        let points = points.clamp(6, 72);
        if imp.font_size.get() == points {
            return;
        }
        imp.font_size.set(points);
        if let Some(base) = self.tag("base") {
            base.set_size_points(points as f64);
        }
        let attrs = font_attrs(points);
        for label in imp.table_labels.borrow().iter() {
            label.set_attributes(Some(&attrs));
        }
    }

    pub fn font_size(&self) -> i32 {
        self.imp().font_size.get()
    }

    fn scroll_to_top(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mut start = buffer.start_iter();
        buffer.place_cursor(&start);
        imp.text_view
            .scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
        imp.scrolled.vadjustment().set_value(0.0);
    }

    fn tag(&self, name: &str) -> Option<gtk::TextTag> {
        self.imp().tags.get().and_then(|t| t.lookup(name))
    }

    // ------------------------------------------------------------------
    // Tags
    // ------------------------------------------------------------------

    fn build_tag_table(&self) -> gtk::TextTagTable {
        let table = gtk::TextTagTable::new();
        let add = |tag: gtk::TextTag| {
            table.add(&tag);
        };
        add(gtk::TextTag::builder()
            .name("base")
            .size_points(self.font_size() as f64)
            .build());
        add(gtk::TextTag::builder()
            .name("norm-title")
            .weight(pango::Weight::Bold.into_glib())
            .scale(1.4)
            .pixels_below_lines(14)
            .build());
        // Trennlinie zwischen zwei Normen: eine winzige Zeile mit
        // Absatzhintergrund, davor und danach je eine leere Abstandszeile
        // (der Absatzhintergrund würde sonst auch den Abstand füllen).
        let rule = gtk::TextTag::builder()
            .name("norm-rule")
            .scale(0.12)
            .build();
        rule.set_paragraph_background_rgba(Some(&gtk::gdk::RGBA::new(0.5, 0.5, 0.5, 0.45)));
        add(rule);
        add(gtk::TextTag::builder()
            .name("rule-gap")
            .scale(0.3)
            .pixels_above_lines(22)
            .build());
        // Weggefallene Normen: kursiv und abgeblendet.
        add(gtk::TextTag::builder()
            .name("repealed")
            .style(pango::Style::Italic)
            .foreground("#8c8c8c")
            .build());
        // Gliederungsüberschriften (Buch, Abschnitt, Titel) über der Norm.
        add(gtk::TextTag::builder()
            .name("unit-heading")
            .weight(pango::Weight::Bold.into_glib())
            .scale(0.8)
            .foreground("#8c8c8c")
            .pixels_below_lines(0)
            .build());
        // Eigene Notiz (Schema) unter der Überschrift.
        add(gtk::TextTag::builder()
            .name("note-heading")
            .weight(pango::Weight::Bold.into_glib())
            .scale(0.8)
            .letter_spacing(pango::SCALE / 2)
            .pixels_above_lines(4)
            .build());
        add(gtk::TextTag::builder()
            .name("user-note")
            .left_margin(36)
            .right_margin(36)
            .pixels_above_lines(2)
            .pixels_below_lines(2)
            .build());
        let note_background = gtk::TextTag::builder().name("note-block").build();
        let reference = gtk::TextTag::builder().name("reference").build();
        add(gtk::TextTag::builder()
            .name("heading1")
            .weight(pango::Weight::Bold.into_glib())
            .scale(1.15)
            .pixels_above_lines(10)
            .build());
        add(gtk::TextTag::builder()
            .name("heading2")
            .weight(pango::Weight::Bold.into_glib())
            .scale(1.05)
            .pixels_above_lines(8)
            .build());
        add(gtk::TextTag::builder()
            .name("heading3")
            .weight(pango::Weight::Bold.into_glib())
            .pixels_above_lines(6)
            .build());
        add(gtk::TextTag::builder()
            .name("section-heading")
            .weight(pango::Weight::Bold.into_glib())
            .pixels_above_lines(24)
            .pixels_below_lines(6)
            .build());
        add(gtk::TextTag::builder()
            .name("para-number")
            .weight(pango::Weight::Bold.into_glib())
            .build());
        add(gtk::TextTag::builder()
            .name("bold")
            .weight(pango::Weight::Bold.into_glib())
            .build());
        add(gtk::TextTag::builder()
            .name("italic")
            .style(pango::Style::Italic)
            .build());
        add(gtk::TextTag::builder()
            .name("underline")
            .underline(pango::Underline::Single)
            .build());
        add(gtk::TextTag::builder()
            .name("sup")
            .rise(5 * pango::SCALE)
            .scale(0.75)
            .build());
        add(gtk::TextTag::builder()
            .name("sub")
            .rise(-3 * pango::SCALE)
            .scale(0.75)
            .build());
        add(gtk::TextTag::builder().name("small").scale(0.85).build());
        add(gtk::TextTag::builder()
            .name("pre")
            .family("monospace")
            .wrap_mode(gtk::WrapMode::None)
            .build());
        add(gtk::TextTag::builder()
            .name("list-label")
            .weight(pango::Weight::Medium.into_glib())
            .build());
        add(gtk::TextTag::builder()
            .name("footnote")
            .scale(0.9)
            .pixels_below_lines(2)
            .build());
        let footnote_ref = gtk::TextTag::builder()
            .name("footnote-ref")
            .weight(pango::Weight::Bold.into_glib())
            .build();
        let style_manager = adw::StyleManager::default();
        let apply_accent = |sm: &adw::StyleManager,
                            footnote_ref: &gtk::TextTag,
                            reference: &gtk::TextTag,
                            note_heading: &gtk::TextTag,
                            note_background: &gtk::TextTag| {
            let accent = sm.accent_color_rgba();
            footnote_ref.set_foreground_rgba(Some(&accent));
            reference.set_foreground_rgba(Some(&accent));
            note_heading.set_foreground_rgba(Some(&accent));
            let mut tint = accent;
            tint.set_alpha(0.1);
            note_background.set_paragraph_background_rgba(Some(&tint));
        };
        let note_heading = table.lookup("note-heading").expect("note-heading");
        apply_accent(
            &style_manager,
            &footnote_ref,
            &reference,
            &note_heading,
            &note_background,
        );
        style_manager.connect_accent_color_rgba_notify(glib::clone!(
            #[weak]
            footnote_ref,
            #[weak]
            reference,
            #[weak]
            note_heading,
            #[weak]
            note_background,
            move |sm| apply_accent(
                sm,
                &footnote_ref,
                &reference,
                &note_heading,
                &note_background
            )
        ));
        add(footnote_ref);
        add(reference);
        add(note_background);

        // Markierungsfarben (hell/dunkel) und Unterstreichung angehefteter Notizen.
        for (name, _, light, dark) in HIGHLIGHT_COLORS {
            let highlight = gtk::TextTag::builder()
                .name(format!("hl-{name}").as_str())
                .build();
            let note = gtk::TextTag::builder()
                .name(format!("note-{name}").as_str())
                .underline(pango::Underline::Single)
                .build();
            // Bearbeitbarer Inline-Block der Notiz, farbig hinterlegt.
            let block = gtk::TextTag::builder()
                .name(format!("inline-note-{name}").as_str())
                .editable(true)
                .left_margin(36)
                .right_margin(36)
                .pixels_above_lines(3)
                .pixels_below_lines(3)
                .build();
            set_highlight_colors(
                &highlight,
                &note,
                &block,
                light,
                dark,
                style_manager.is_dark(),
            );
            style_manager.connect_dark_notify(glib::clone!(
                #[weak]
                highlight,
                #[weak]
                note,
                #[weak]
                block,
                move |sm| set_highlight_colors(
                    &highlight,
                    &note,
                    &block,
                    light,
                    dark,
                    sm.is_dark()
                )
            ));
            add(highlight);
            add(note);
            add(block);
        }
        add(gtk::TextTag::builder()
            .name("note-glyph")
            .editable(false)
            .weight(pango::Weight::Bold.into_glib())
            .build());
        add(gtk::TextTag::builder()
            .name("note-body")
            .editable(true)
            .build());

        for depth in 0..=MAX_LIST_DEPTH {
            let mut tabs = pango::TabArray::new(1, true);
            tabs.set_tab(0, pango::TabAlign::Left, LIST_INDENT);
            let name = list_tag_name(depth);
            // Unterebenen (Nummern, Buchstaben) kursiv, damit sie sich von
            // den Absätzen „(1)“, „(2)“ abheben.
            add(gtk::TextTag::builder()
                .name(name.as_str())
                .left_margin(24 + (depth as i32 + 1) * LIST_INDENT)
                .indent(-LIST_INDENT)
                .tabs(&tabs)
                .style(pango::Style::Italic)
                .pixels_below_lines(2)
                .build());
        }
        table
    }

    fn tags_for(&self, seg_tags: &[SegTag], extra: &[&str]) -> Vec<gtk::TextTag> {
        let mut names: Vec<String> = extra.iter().map(|s| (*s).to_owned()).collect();
        for t in seg_tags {
            match t {
                SegTag::Bold => names.push("bold".into()),
                SegTag::Italic => names.push("italic".into()),
                SegTag::Underline => names.push("underline".into()),
                SegTag::Sup => names.push("sup".into()),
                SegTag::Sub => names.push("sub".into()),
                SegTag::Small => names.push("small".into()),
                SegTag::Pre => names.push("pre".into()),
                SegTag::Heading(level) => names.push(heading_tag_name(*level).into()),
                SegTag::ListLine(depth) => names.push(list_tag_name(*depth)),
                SegTag::ListLabel => names.push("list-label".into()),
                SegTag::FootnoteRef(_) => names.push("footnote-ref".into()),
                SegTag::Table(_) => {}
            }
        }
        names.iter().filter_map(|n| self.tag(n)).collect()
    }

    // ------------------------------------------------------------------
    // Aufbau des Puffers
    // ------------------------------------------------------------------

    /// Leert den Puffer samt Abschnitten und Kind-Widgets.
    fn reset_buffer(&self) {
        let imp = self.imp();
        let Some(table) = imp.tags.get() else {
            return;
        };
        imp.sections.borrow_mut().clear();
        imp.links.borrow_mut().clear();
        imp.table_labels.borrow_mut().clear();
        imp.anchored.borrow_mut().clear();
        if let Some(popover) = imp.selection_popover.get() {
            popover.popdown();
        }
        imp.text_view
            .set_buffer(Some(&gtk::TextBuffer::new(Some(table))));
    }

    /// Rendert eine Norm als Abschnitt am Ende (oder am Anfang) des Puffers.
    fn render_section(&self, page: NormPage, at_start: bool) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let NormPage {
            norm,
            units,
            law_abbrev,
            note,
            annotations,
        } = page;
        let is_first = imp.sections.borrow().is_empty();
        let old_chars = buffer.char_count();
        let old_links = imp.links.borrow().len();
        let norm_id = norm.info.id;
        imp.pending_tags.borrow_mut().clear();

        // Einfügemarke mit Rechtsgravitation: wandert hinter eingefügten Text.
        let at = if at_start {
            buffer.create_mark(None, &buffer.start_iter(), false)
        } else {
            buffer.create_mark(None, &buffer.end_iter(), false)
        };
        // Abschnittsanfang mit Linksgravitation: bleibt vor späterem Text.
        let start = buffer.create_mark(None, &buffer.iter_at_mark(&at), true);
        let section_offset = buffer.iter_at_mark(&at).offset();

        // Gliederungsüberschriften, soweit sie sich vom Nachbarn unterscheiden.
        let neighbor_units: Vec<i64> = {
            let sections = imp.sections.borrow();
            let neighbor = if at_start {
                sections.first()
            } else {
                sections.last()
            };
            neighbor.map(|s| s.unit_ids.clone()).unwrap_or_default()
        };
        let first_new = units
            .iter()
            .position(|u| !neighbor_units.contains(&u.id))
            .unwrap_or(units.len());
        // Trennlinie vor einem angehängten Abschnitt.
        if !is_first && !at_start {
            self.insert_rule(&buffer, &at);
        }
        for unit in &units[first_new..] {
            let tags = self.tags_for(&[], &["unit-heading"]);
            let line = match &unit.titel {
                Some(t) => format!("{} · {t}\n", unit.bez),
                None => format!("{}\n", unit.bez),
            };
            self.insert_text(&buffer, &at, &line, &tags);
        }

        // Überschrift
        let title = norm.info.display_name();
        let title_tags = self.tags_for(&[], &["norm-title"]);
        self.insert_text(&buffer, &at, &format!("{title}\n"), &title_tags);

        // Notiz (Schema) zwischen Überschrift und Text
        let note_start = buffer.create_mark(None, &buffer.iter_at_mark(&at), true);
        let note_end = buffer.create_mark(None, &buffer.iter_at_mark(&at), false);
        self.insert_note(&buffer, &at, note.as_deref());
        let content_offset = buffer.iter_at_mark(&at).offset() - section_offset;

        // Normtext
        let flat = flatten_blocks(&norm.blocks);
        self.insert_flattened(&buffer, &at, norm_id, &flat, &[]);
        self.mark_references(&buffer, &flat, &law_abbrev, section_offset + content_offset);
        let base = section_offset + content_offset;
        let content_len = flat.char_len() as i32;
        let starts: Vec<i32> = flat.block_starts.iter().map(|b| *b as i32).collect();
        let block_marks: Vec<gtk::TextMark> = starts
            .iter()
            .map(|b| buffer.create_mark(None, &buffer.iter_at_offset(base + b), false))
            .collect();
        let block_lens: Vec<i32> = starts
            .iter()
            .enumerate()
            .map(|(i, b)| starts.get(i + 1).copied().unwrap_or(content_len) - b)
            .collect();
        let content_end = buffer.create_mark(None, &buffer.iter_at_mark(&at), true);

        // Fußnoten (aus dem Text referenziert) und amtliche Hinweise
        if !norm.footnotes.is_empty() || !norm.fussnoten.is_empty() {
            let heading = self.tags_for(&[], &["section-heading"]);
            self.insert_text(
                &buffer,
                &at,
                &format!("{}\n", gettext("Fußnoten")),
                &heading,
            );
            for footnote in &norm.footnotes {
                let iter = buffer.iter_at_mark(&at);
                buffer.create_mark(
                    Some(footnote_mark_name(norm_id, &footnote.id).as_str()),
                    &iter,
                    true,
                );
                let tags = self.tags_for(&[], &["footnote", "bold"]);
                self.insert_text(&buffer, &at, &format!("{} ", footnote.mark), &tags);
                let flat = flatten_blocks(&footnote.blocks);
                self.insert_flattened(&buffer, &at, norm_id, &flat, &["footnote"]);
            }
            if !norm.fussnoten.is_empty() {
                let flat = flatten_blocks(&norm.fussnoten);
                self.insert_flattened(&buffer, &at, norm_id, &flat, &["footnote"]);
            }
        }
        // Beim Voranstellen trennt eine Linie vom bisherigen Anfang.
        if at_start {
            self.insert_rule(&buffer, &at);
        }
        // Weggefallene Normen kursiv und abgeblendet.
        if is_repealed(norm.info.titel.as_deref()) {
            let tags = self.tags_for(&[], &["repealed"]);
            let from = buffer.iter_at_mark(&start).offset();
            let to = buffer.iter_at_mark(&at).offset();
            self.imp()
                .pending_tags
                .borrow_mut()
                .push((from, to, tags.clone()));
            for tag in &tags {
                buffer.apply_tag(
                    tag,
                    &buffer.iter_at_offset(from),
                    &buffer.iter_at_offset(to),
                );
            }
        }

        // Beim Voranstellen hat der Text die Tags des bisherigen Anfangs
        // geerbt: alles entfernen und die erfassten Bereiche neu anwenden.
        if at_start {
            buffer.remove_all_tags(&buffer.iter_at_mark(&start), &buffer.iter_at_mark(&at));
            for (from, to, tags) in imp.pending_tags.borrow().iter() {
                let a = buffer.iter_at_offset(*from);
                let b = buffer.iter_at_offset(*to);
                for tag in tags {
                    buffer.apply_tag(tag, &a, &b);
                }
            }
        }
        imp.pending_tags.borrow_mut().clear();
        self.mark_paragraph_numbers(
            &buffer,
            &norm.blocks,
            &flat,
            section_offset + content_offset,
        );
        if let Some(base) = self.tag("base") {
            buffer.apply_tag(
                &base,
                &buffer.iter_at_mark(&start),
                &buffer.iter_at_mark(&at),
            );
        }
        // Die Startmarke des bisherigen ersten Abschnitts (Linksgravitation)
        // ist bei Offset 0 geblieben – hinter den neuen Text verschieben.
        if at_start {
            if let Some(first) = imp.sections.borrow().first() {
                buffer.move_mark(&first.start, &buffer.iter_at_mark(&at));
            }
        }
        buffer.delete_mark(&at);

        // Beim Voranstellen verschieben sich die bisherigen Verweise um die
        // eingefügte Zeichenzahl; die neuen liegen bereits richtig (ab 0).
        if at_start {
            let added = buffer.char_count() - old_chars;
            for link in imp.links.borrow_mut().iter_mut().take(old_links) {
                link.start += added;
                link.end += added;
            }
        }

        let section = Section {
            norm: norm.info,
            unit_ids: units.iter().map(|u| u.id).collect(),
            start,
            note_start,
            note_end,
            content_offset,
            block_marks,
            block_lens,
            content_end,
        };
        let index = {
            let mut sections = imp.sections.borrow_mut();
            if at_start {
                sections.insert(0, section);
                0
            } else {
                sections.push(section);
                sections.len() - 1
            }
        };
        // Markierungen und angeheftete Notizen dieser Norm.
        for annotation in &annotations {
            self.place_annotation(index, annotation);
        }
    }

    /// Fügt eine Trennlinie (eigene Zeile mit Absatzhintergrund) ein.
    fn insert_rule(&self, buffer: &gtk::TextBuffer, at: &gtk::TextMark) {
        let gap = self.tags_for(&[], &["rule-gap"]);
        let rule = self.tags_for(&[], &["norm-rule"]);
        self.insert_text(buffer, at, "\n", &gap);
        self.insert_text(buffer, at, "\u{2009}\n", &rule);
        self.insert_text(buffer, at, "\n", &gap);
    }

    /// Fügt die Notiz (Schema) als hervorgehobenen Block ein.
    fn insert_note(&self, buffer: &gtk::TextBuffer, at: &gtk::TextMark, note: Option<&str>) {
        let Some(text) = note.map(str::trim).filter(|t| !t.is_empty()) else {
            return;
        };
        let heading = self.tags_for(&[], &["note-heading", "note-block"]);
        self.insert_text(
            buffer,
            at,
            &format!("{}\n", gettext("Schema / Notiz")),
            &heading,
        );
        let body = self.tags_for(&[], &["user-note", "note-block"]);
        self.insert_text(buffer, at, &format!("{text}\n"), &body);
    }

    /// Markiert erkannte Normverweise im soeben eingefügten Text.
    fn mark_references(
        &self,
        buffer: &gtk::TextBuffer,
        flat: &Flattened,
        law_abbrev: &str,
        base: i32,
    ) {
        let Some(tag) = self.tag("reference") else {
            return;
        };
        let text = flat.text();
        for reference in find_references(&text, law_abbrev) {
            if reference.target.norm.is_none() {
                continue;
            }
            let from = base + reference.start as i32;
            let to = base + reference.end as i32;
            buffer.apply_tag(
                &tag,
                &buffer.iter_at_offset(from),
                &buffer.iter_at_offset(to),
            );
            self.imp().links.borrow_mut().push(LinkRange {
                start: from,
                end: to,
                target: LinkTarget::Reference(reference.target),
            });
        }
    }

    /// Fügt Text mit Tags an der Einfügemarke ein.
    fn insert_text(
        &self,
        buffer: &gtk::TextBuffer,
        at: &gtk::TextMark,
        text: &str,
        tags: &[gtk::TextTag],
    ) {
        let mut iter = buffer.iter_at_mark(at);
        let from = iter.offset();
        let refs: Vec<&gtk::TextTag> = tags.iter().collect();
        buffer.insert_with_tags(&mut iter, text, &refs);
        self.imp()
            .pending_tags
            .borrow_mut()
            .push((from, iter.offset(), tags.to_vec()));
    }

    /// Fügt linearisierte Segmente an der Einfügemarke ein; Tabellen werden
    /// als Kind-Widget an einem Anker eingebettet (belegt wie der Platzhalter
    /// genau ein Zeichen).
    fn insert_flattened(
        &self,
        buffer: &gtk::TextBuffer,
        at: &gtk::TextMark,
        norm_id: i64,
        flat: &Flattened,
        extra: &[&str],
    ) {
        let imp = self.imp();
        for seg in &flat.segs {
            let mut iter = buffer.iter_at_mark(at);
            if let Some(index) = seg.tags.iter().find_map(|t| match t {
                SegTag::Table(i) => Some(*i),
                _ => None,
            }) {
                if let Some(data) = flat.tables.get(index) {
                    let anchor = buffer.create_child_anchor(&mut iter);
                    let widget = self.build_table(data);
                    imp.text_view.add_child_at_anchor(&widget, &anchor);
                }
                continue;
            }
            let start = iter.offset();
            let tags = self.tags_for(&seg.tags, extra);
            let refs: Vec<&gtk::TextTag> = tags.iter().collect();
            buffer.insert_with_tags(&mut iter, &seg.text, &refs);
            imp.pending_tags
                .borrow_mut()
                .push((start, iter.offset(), tags.clone()));
            if let Some(id) = seg.tags.iter().find_map(|t| match t {
                SegTag::FootnoteRef(id) => Some(id.clone()),
                _ => None,
            }) {
                imp.links.borrow_mut().push(LinkRange {
                    start,
                    end: iter.offset(),
                    target: LinkTarget::Footnote(footnote_mark_name(norm_id, &id)),
                });
            }
        }
    }

    /// Hebt Absatznummern wie „(1)“ am Anfang von Absätzen hervor.
    fn mark_paragraph_numbers(
        &self,
        buffer: &gtk::TextBuffer,
        blocks: &[Block],
        flat: &Flattened,
        content_start: i32,
    ) {
        let Some(tag) = self.tag("para-number") else {
            return;
        };
        for (block, start) in blocks.iter().zip(&flat.block_starts) {
            let Block::Paragraph { spans } = block else {
                continue;
            };
            let text = spans_text(spans);
            let Some((offset, len)) = paragraph_number_range(&text) else {
                continue;
            };
            let from = content_start + *start as i32 + offset as i32;
            let a = buffer.iter_at_offset(from);
            let b = buffer.iter_at_offset(from + len as i32);
            buffer.apply_tag(&tag, &a, &b);
        }
    }

    /// Baut eine Tabelle als `GtkGrid` (mit optionalem Titel darüber).
    /// Sehr große Tabellen werden als ein einziges Label mit einer Zeile
    /// je Tabellenzeile gesetzt, weil ein Raster aus tausenden umbrechenden
    /// Labels die Oberfläche minutenlang blockiert.
    fn build_table(&self, data: &TableData) -> gtk::Widget {
        let imp = self.imp();
        let attrs = font_attrs(self.font_size());
        if data.rows.len() > MAX_GRID_ROWS {
            let label = gtk::Label::builder()
                .use_markup(true)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .xalign(0.0)
                .max_width_chars(80)
                .selectable(false)
                .css_classes(["norm-table-plain"])
                .build();
            label.set_markup(&table_as_lines(data));
            label.set_attributes(Some(&attrs));
            imp.table_labels.borrow_mut().push(label.clone());
            return label.upcast();
        }
        let grid = gtk::Grid::builder()
            .css_classes(["norm-table"])
            .column_homogeneous(false)
            .build();
        let mut occupied: HashSet<(usize, usize)> = HashSet::new();
        for (r, row) in data.rows.iter().enumerate() {
            let mut c = 0usize;
            for cell in row {
                while occupied.contains(&(r, c)) {
                    c += 1;
                }
                let label = gtk::Label::builder()
                    .use_markup(true)
                    .wrap(true)
                    .wrap_mode(pango::WrapMode::WordChar)
                    .xalign(0.0)
                    .yalign(0.0)
                    .max_width_chars(40)
                    .selectable(false)
                    .build();
                label.set_markup(&cell_markup(&cell.segs));
                label.set_attributes(Some(&attrs));
                if r < data.header_rows {
                    label.add_css_class("table-header");
                }
                let colspan = cell.colspan.max(1) as usize;
                let rowspan = cell.rowspan.max(1) as usize;
                grid.attach(&label, c as i32, r as i32, colspan as i32, rowspan as i32);
                for dr in 0..rowspan {
                    for dc in 0..colspan {
                        occupied.insert((r + dr, c + dc));
                    }
                }
                imp.table_labels.borrow_mut().push(label);
                c += colspan;
            }
        }
        match &data.title {
            Some(title) if !title.trim().is_empty() => {
                let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
                let label = gtk::Label::builder()
                    .label(title.trim())
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(["heading"])
                    .build();
                label.set_attributes(Some(&attrs));
                imp.table_labels.borrow_mut().push(label.clone());
                boxed.append(&label);
                boxed.append(&grid);
                boxed.upcast()
            }
            _ => grid.upcast(),
        }
    }

    // ------------------------------------------------------------------
    // Klick auf Fußnotenverweise
    // ------------------------------------------------------------------

    fn setup_controllers(&self) {
        let imp = self.imp();
        // Bearbeitung in Notizblöcken: Änderungen melden, Verweise verschieben,
        // Cursor nur dort zeigen.
        imp.text_view.connect_buffer_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |tv| view.watch_buffer(&tv.buffer())
        ));
        let click = gtk::GestureClick::builder().button(1).build();
        click.connect_released(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, n_press, x, y| {
                if n_press != 1 {
                    return;
                }
                if let Some(link) = view.link_at(x, y) {
                    let new_tab = gesture
                        .current_event_state()
                        .contains(gtk::gdk::ModifierType::CONTROL_MASK);
                    view.follow_link(&link, new_tab);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    return;
                }
                view.on_click_released(x, y);
            }
        ));
        imp.text_view.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, x, y| {
                let over = view.link_at(x, y).is_some();
                let imp = view.imp();
                if imp.hovering_link.replace(over) != over {
                    imp.text_view
                        .set_cursor_from_name(Some(if over { "pointer" } else { "text" }));
                }
            }
        ));
        imp.text_view.add_controller(motion);

        // Fortlaufendes Lesen: Position überwachen und am Rand nachladen.
        let adjustment = imp.scrolled.vadjustment();
        adjustment.connect_value_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.update_visible_norm();
                view.check_edges();
            }
        ));
        // Bei `changed` (Höhe ändert sich während GTK Zeilen validiert) nur
        // nachladen; die sichtbare Norm wäre in dieser Phase unzuverlässig.
        adjustment.connect_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.check_edges()
        ));
        // Am oberen Rand gibt es kein `value-changed` mehr; ein Scrollrad
        // nach oben lädt dann die vorherige Norm.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, dy| {
                if dy < 0.0 && view.imp().scrolled.vadjustment().value() <= 0.0 {
                    view.request_previous();
                }
                glib::Propagation::Proceed
            }
        ));
        imp.scrolled.add_controller(scroll);
    }

    /// Verbindet die Signale eines (neuen) Puffers.
    fn watch_buffer(&self, buffer: &gtk::TextBuffer) {
        buffer.connect_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.on_buffer_changed()
        ));
        buffer.connect_insert_text(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, iter, text| {
                if !view.imp().rendering.get() {
                    view.shift_links(iter.offset(), text.chars().count() as i32);
                }
            }
        ));
        buffer.connect_delete_range(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, start, end| {
                if !view.imp().rendering.get() {
                    view.shift_links(end.offset(), start.offset() - end.offset());
                }
            }
        ));
        buffer.connect_cursor_position_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.update_cursor_visibility()
        ));
    }

    /// Nur für Tests: Auswahl setzen und wie über das Popover markieren
    /// („hl <farbe>“), eine Notiz anheften („note“) oder markieren („select a b“).
    pub fn test_command(&self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let buffer = self.imp().text_view.buffer();
        match parts.as_slice() {
            ["select", a, b] => {
                if let (Ok(a), Ok(b)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    buffer.select_range(&buffer.iter_at_offset(a), &buffer.iter_at_offset(b));
                    let end = buffer.iter_at_offset(b);
                    if let Some(popover) = self.imp().selection_popover.get() {
                        self.popup_at(popover, &end);
                    }
                }
            }
            ["hl", color] => {
                let color = HIGHLIGHT_COLORS
                    .iter()
                    .map(|c| c.0)
                    .find(|c| c == color)
                    .unwrap_or("yellow");
                self.on_color_chosen(AnnotationKind::Highlight, color);
            }
            ["note"] => self.on_color_chosen(AnnotationKind::Note, "yellow"),
            ["type", rest @ ..] => {
                let buffer = self.imp().text_view.buffer();
                buffer.insert_at_cursor(&rest.join(" "));
            }
            _ => log::warn!("unbekannter Testbefehl: {command}"),
        }
    }

    /// Nur für Tests: scrollt zu einem Anteil der Gesamthöhe („bottom“ = 1.0)
    /// oder fordert mit einem negativen Wert die vorherige Norm an.
    pub fn scroll_to_fraction(&self, fraction: f64) {
        let adjustment = self.imp().scrolled.vadjustment();
        if fraction < 0.0 {
            self.request_previous();
            return;
        }
        let target = (adjustment.upper() - adjustment.page_size()) * fraction.min(1.0);
        adjustment.set_value(target.max(0.0));
    }

    fn schedule_edge_check(&self) {
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move || {
                view.update_visible_norm();
                view.check_edges();
            }
        ));
        // Nach dem Layout (GTK validiert nachgeladene Zeilen verzögert).
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || view.update_visible_norm()
            ),
        );
    }

    // ------------------------------------------------------------------
    // Markierungen und angeheftete Notizen
    // ------------------------------------------------------------------

    /// Wendet eine Annotation im Abschnitt `section` an: Wortlaut taggen
    /// und bei Notizen den bearbeitbaren Block hinter dem Absatz einfügen.
    fn place_annotation(&self, section: usize, annotation: &Annotation) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        if annotation.orphaned
            || !matches!(
                annotation.kind,
                AnnotationKind::Highlight | AnnotationKind::Note
            )
        {
            return;
        }
        let index = annotation.paragraph as usize;
        let (block_start, block_len, block_end_pos) = {
            let sections = imp.sections.borrow();
            let Some(sec) = sections.get(section) else {
                return;
            };
            if index == 0 || index > sec.block_marks.len() {
                return;
            }
            let start = buffer.iter_at_mark(&sec.block_marks[index - 1]).offset();
            let end_pos = match sec.block_marks.get(index) {
                Some(next) => buffer.iter_at_mark(next).offset(),
                None => buffer.iter_at_mark(&sec.content_end).offset(),
            };
            (start, sec.block_lens[index - 1], end_pos)
        };
        let from = block_start + (annotation.start as i32).clamp(0, block_len);
        let to = block_start + (annotation.end as i32).clamp(0, block_len);
        let to = to.max(from);
        let color = annotation.color.as_deref().unwrap_or("yellow");
        let prefix = match annotation.kind {
            AnnotationKind::Highlight => "hl-",
            _ => "note-",
        };
        if let Some(tag) = self.tag(&format!("{prefix}{color}")) {
            buffer.apply_tag(
                &tag,
                &buffer.iter_at_offset(from),
                &buffer.iter_at_offset(to),
            );
        }
        let start = buffer.create_mark(None, &buffer.iter_at_offset(from), true);
        let end = buffer.create_mark(None, &buffer.iter_at_offset(to), false);
        let mut block_marks = None;
        if annotation.kind == AnnotationKind::Note {
            // Hinter vorhandene Notizblöcke desselben Absatzes einreihen.
            let mut insert_at = block_end_pos;
            for other in imp.anchored.borrow().iter() {
                if other.annotation.norm == annotation.norm
                    && other.annotation.paragraph == annotation.paragraph
                {
                    if let Some(end_mark) = &other.block_end {
                        insert_at = insert_at.max(buffer.iter_at_mark(end_mark).offset());
                    }
                }
            }
            let (bs, be) = self.insert_note_block(
                &buffer,
                insert_at,
                color,
                annotation.note.as_deref().unwrap_or(""),
            );
            block_marks = Some((bs, be));
        }
        let (block_start_mark, block_end_mark) = match block_marks {
            Some((a, b)) => (Some(a), Some(b)),
            None => (None, None),
        };
        imp.anchored.borrow_mut().push(Anchored {
            annotation: annotation.clone(),
            start,
            end,
            block_start: block_start_mark,
            block_end: block_end_mark,
        });
    }

    /// Fügt einen Inline-Notizblock („✎ Text⏎“) an `at` ein und liefert
    /// seine Marken (Anfang links-, Ende rechtsgravitativ).
    fn insert_note_block(
        &self,
        buffer: &gtk::TextBuffer,
        at: i32,
        color: &str,
        text: &str,
    ) -> (gtk::TextMark, gtk::TextMark) {
        let imp = self.imp();
        imp.rendering.set(true);
        let block_start = buffer.create_mark(None, &buffer.iter_at_offset(at), true);
        let mut iter = buffer.iter_at_offset(at);
        let glyph_tags = self.tags_for(&[], &["note-glyph", &format!("inline-note-{color}")]);
        let refs: Vec<&gtk::TextTag> = glyph_tags.iter().collect();
        buffer.insert_with_tags(&mut iter, NOTE_GLYPH, &refs);
        let body_tags = self.tags_for(&[], &["note-body", &format!("inline-note-{color}")]);
        let refs: Vec<&gtk::TextTag> = body_tags.iter().collect();
        buffer.insert_with_tags(&mut iter, &format!("{text}\n"), &refs);
        let block_end = buffer.create_mark(None, &iter, false);
        // Text vor einem Tag-Anfang erbt sonst dessen Tags (siehe render_section).
        let from = buffer.iter_at_offset(at);
        buffer.remove_all_tags(&from, &iter);
        let glyph_end = buffer.iter_at_offset(at + NOTE_GLYPH.chars().count() as i32);
        for tag in &glyph_tags {
            buffer.apply_tag(tag, &from, &glyph_end);
        }
        for tag in &body_tags {
            buffer.apply_tag(tag, &glyph_end, &iter);
        }
        if let Some(base) = self.tag("base") {
            buffer.apply_tag(&base, &from, &iter);
        }
        imp.rendering.set(false);
        let added = iter.offset() - at;
        // Verweise hinter dem Block verschieben (nicht die davor).
        for link in imp.links.borrow_mut().iter_mut() {
            if link.start >= at {
                link.start += added;
                link.end += added;
            }
        }
        (block_start, block_end)
    }

    /// Text eines Inline-Notizblocks ohne Präfix und abschließenden Umbruch.
    fn inline_note_text(&self, item: &Anchored) -> Option<String> {
        let (Some(bs), Some(be)) = (&item.block_start, &item.block_end) else {
            return None;
        };
        let buffer = self.imp().text_view.buffer();
        let text = buffer
            .text(&buffer.iter_at_mark(bs), &buffer.iter_at_mark(be), false)
            .to_string();
        let body = text.strip_prefix(NOTE_GLYPH).unwrap_or(&text);
        Some(body.trim_end_matches('\n').to_owned())
    }

    /// Setzt den Cursor in einen Notizblock und gibt der Ansicht den Fokus.
    fn focus_note(&self, id: i64) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let position = imp
            .anchored
            .borrow()
            .iter()
            .find(|a| a.annotation.id == id)
            .and_then(|a| {
                a.block_end
                    .as_ref()
                    .map(|m| buffer.iter_at_mark(m).offset() - 1)
            });
        if let Some(pos) = position {
            buffer.place_cursor(&buffer.iter_at_offset(pos));
            imp.text_view.grab_focus();
            self.update_cursor_visibility();
        }
    }

    /// Der Cursor ist nur in bearbeitbaren Notizblöcken sichtbar.
    fn update_cursor_visibility(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let offset = buffer.iter_at_mark(&buffer.get_insert()).offset();
        let inside = self.note_block_at(offset).is_some();
        imp.text_view.set_cursor_visible(inside);
    }

    /// ID der Notiz, deren Inline-Block den Offset enthält.
    fn note_block_at(&self, offset: i32) -> Option<i64> {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        imp.anchored
            .borrow()
            .iter()
            .find(|a| match (&a.block_start, &a.block_end) {
                (Some(bs), Some(be)) => {
                    buffer.iter_at_mark(bs).offset() <= offset
                        && offset < buffer.iter_at_mark(be).offset()
                }
                _ => false,
            })
            .map(|a| a.annotation.id)
    }

    /// Nach Änderungen im Puffer: geänderte Notizblöcke verzögert melden.
    fn on_buffer_changed(&self) {
        let imp = self.imp();
        if imp.rendering.get() {
            return;
        }
        if let Some(id) = imp.note_edit_timer.borrow_mut().take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(NOTE_EDIT_DELAY_MS),
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    view.imp().note_edit_timer.borrow_mut().take();
                    view.flush_note_edits();
                }
            ),
        );
        *imp.note_edit_timer.borrow_mut() = Some(id);
    }

    /// Meldet alle Notizblöcke, deren Text vom gespeicherten abweicht.
    pub fn flush_note_edits(&self) {
        let imp = self.imp();
        let mut updates = Vec::new();
        for item in imp.anchored.borrow().iter() {
            let Some(text) = self.inline_note_text(item) else {
                continue;
            };
            if item.annotation.note.as_deref().unwrap_or("") != text {
                let mut updated = item.annotation.clone();
                updated.note = Some(text);
                updates.push(updated);
            }
        }
        for updated in updates {
            for item in imp.anchored.borrow_mut().iter_mut() {
                if item.annotation.id == updated.id {
                    item.annotation.note = updated.note.clone();
                }
            }
            self.emit_annotation(AnnotationEvent::Update(updated));
        }
    }

    /// Baut das Popover für die Textauswahl: Farben für Markierung und
    /// Notiz, Entfernen.
    fn setup_selection_popover(&self) {
        let imp = self.imp();
        let grid = gtk::Grid::builder()
            .row_spacing(4)
            .column_spacing(4)
            .build();
        let highlight_label = gtk::Label::builder()
            .label(gettext("Markieren"))
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        let note_label = gtk::Label::builder()
            .label(gettext("Notiz"))
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        grid.attach(&highlight_label, 0, 0, 1, 1);
        grid.attach(&note_label, 0, 1, 1, 1);
        for (column, (name, label, _, _)) in HIGHLIGHT_COLORS.iter().enumerate() {
            for (row, kind) in [(0, AnnotationKind::Highlight), (1, AnnotationKind::Note)] {
                let swatch = gtk::Box::builder()
                    .css_classes(["highlight-swatch", name])
                    .build();
                let button = gtk::Button::builder()
                    .child(&swatch)
                    .tooltip_text(*label)
                    .css_classes(["flat", "circular"])
                    .build();
                button.connect_clicked(glib::clone!(
                    #[weak(rename_to = view)]
                    self,
                    move |_| view.on_color_chosen(kind, name)
                ));
                grid.attach(&button, column as i32 + 1, row, 1, 1);
            }
        }
        let remove_button = gtk::Button::builder()
            .label(gettext("Entfernen"))
            .css_classes(["flat", "destructive-action"])
            .visible(false)
            .build();
        remove_button.connect_clicked(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.on_remove_clicked()
        ));
        grid.attach(&remove_button, 0, 2, 6, 1);
        let popover = gtk::Popover::builder()
            .child(&grid)
            .autohide(true)
            .has_arrow(true)
            .build();
        popover.set_parent(&*imp.text_view);
        imp.selection_popover.set(popover).ok();
        imp.remove_button.set(remove_button).ok();
        imp.note_button.set(note_label.upcast::<gtk::Widget>()).ok();
    }

    /// Klick ohne Verweis: Popover für eine Auswahl, eine getroffene
    /// Markierung oder das Symbol eines Notizblocks.
    fn on_click_released(&self, x: f64, y: f64) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        self.update_cursor_visibility();
        let (Some(popover), Some(remove_button), Some(note_label)) = (
            imp.selection_popover.get(),
            imp.remove_button.get(),
            imp.note_button.get(),
        ) else {
            return;
        };
        if let Some((start, end)) = buffer.selection_bounds() {
            if start.offset() == end.offset() || self.note_block_at(start.offset()).is_some() {
                popover.popdown();
                return;
            }
            imp.popover_target.set(None);
            let overlapping = self
                .annotation_at(end.offset() - 1, Some(AnnotationKind::Highlight))
                .or_else(|| self.annotation_at(start.offset(), Some(AnnotationKind::Highlight)));
            remove_button.set_visible(overlapping.is_some());
            note_label.set_visible(true);
            self.set_note_row_visible(true);
            self.popup_at(popover, &end);
            return;
        }
        let (bx, by) =
            imp.text_view
                .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let Some(iter) = imp.text_view.iter_at_location(bx, by) else {
            popover.popdown();
            return;
        };
        let offset = iter.offset();
        // Symbol eines Notizblocks: Farbe ändern oder Notiz löschen.
        let glyph_hit = self.note_block_at(offset).filter(|id| {
            imp.anchored.borrow().iter().any(|a| {
                a.annotation.id == *id
                    && a.block_start.as_ref().is_some_and(|m| {
                        let bs = buffer.iter_at_mark(m).offset();
                        offset < bs + NOTE_GLYPH.chars().count() as i32
                    })
            })
        });
        let target =
            glyph_hit.or_else(|| self.annotation_at(offset, Some(AnnotationKind::Highlight)));
        match target {
            Some(id) => {
                imp.popover_target.set(Some(id));
                remove_button.set_visible(true);
                let is_note = glyph_hit.is_some();
                self.set_note_row_visible(is_note);
                self.set_highlight_row_visible(!is_note);
                self.popup_at(popover, &iter);
            }
            None => popover.popdown(),
        }
    }

    /// Zeilen des Popovers ein-/ausblenden (Markieren = Zeile 0, Notiz = Zeile 1).
    fn set_note_row_visible(&self, visible: bool) {
        self.set_popover_row_visible(1, visible);
    }

    fn set_highlight_row_visible(&self, visible: bool) {
        self.set_popover_row_visible(0, visible);
    }

    fn set_popover_row_visible(&self, row: i32, visible: bool) {
        let Some(popover) = self.imp().selection_popover.get() else {
            return;
        };
        let Some(grid) = popover.child().and_downcast::<gtk::Grid>() else {
            return;
        };
        for column in 0..6 {
            if let Some(child) = grid.child_at(column, row) {
                child.set_visible(visible);
            }
        }
    }

    fn popup_at(&self, popover: &gtk::Popover, iter: &gtk::TextIter) {
        let imp = self.imp();
        // Standard: beide Zeilen sichtbar, wenn nicht gezielt ausgeblendet.
        if imp.popover_target.get().is_none() {
            self.set_highlight_row_visible(true);
        }
        let location = imp.text_view.iter_location(iter);
        let (wx, wy) = imp.text_view.buffer_to_window_coords(
            gtk::TextWindowType::Widget,
            location.x(),
            location.y(),
        );
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            wx,
            wy,
            1,
            location.height(),
        )));
        popover.popup();
    }

    /// ID der Annotation (gewünschter Art) an einem Pufferoffset (Wortlaut).
    fn annotation_at(&self, offset: i32, kind: Option<AnnotationKind>) -> Option<i64> {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        imp.anchored
            .borrow()
            .iter()
            .find(|a| {
                kind.is_none_or(|k| a.annotation.kind == k)
                    && buffer.iter_at_mark(&a.start).offset() <= offset
                    && offset < buffer.iter_at_mark(&a.end).offset()
            })
            .map(|a| a.annotation.id)
    }

    /// Farbe gewählt: neue Markierung/Notiz aus der Auswahl oder Farbwechsel
    /// einer bestehenden Annotation.
    fn on_color_chosen(&self, kind: AnnotationKind, color: &'static str) {
        let imp = self.imp();
        if let Some(popover) = imp.selection_popover.get() {
            popover.popdown();
        }
        if let Some(id) = imp.popover_target.get() {
            let updated = imp
                .anchored
                .borrow()
                .iter()
                .find(|a| a.annotation.id == id)
                .map(|a| {
                    let mut annotation = a.annotation.clone();
                    annotation.color = Some(color.to_owned());
                    annotation
                });
            if let Some(annotation) = updated {
                self.emit_annotation(AnnotationEvent::Update(annotation));
            }
            return;
        }
        if let Some(mut annotation) = self.annotation_from_selection(kind, color) {
            if kind == AnnotationKind::Note {
                annotation.note = Some(String::new());
            }
            self.emit_annotation(AnnotationEvent::Create(annotation));
        }
    }

    fn on_remove_clicked(&self) {
        let imp = self.imp();
        if let Some(popover) = imp.selection_popover.get() {
            popover.popdown();
        }
        let buffer = imp.text_view.buffer();
        let id = imp.popover_target.get().or_else(|| {
            let (start, end) = buffer.selection_bounds()?;
            self.annotation_at(end.offset() - 1, Some(AnnotationKind::Highlight))
                .or_else(|| self.annotation_at(start.offset(), Some(AnnotationKind::Highlight)))
        });
        if let Some(id) = id {
            self.emit_annotation(AnnotationEvent::Delete(id));
        }
    }

    /// Baut aus der Textauswahl eine Annotation mit Absatz, Offsets und
    /// Wortlaut (auf den Absatz begrenzt, in dem die Auswahl beginnt).
    fn annotation_from_selection(&self, kind: AnnotationKind, color: &str) -> Option<Annotation> {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let (sel_start, sel_end) = buffer.selection_bounds()?;
        let sel_start = sel_start.offset();
        let sel_end = sel_end.offset();
        let sections = imp.sections.borrow();
        let section = sections
            .iter()
            .rev()
            .find(|s| buffer.iter_at_mark(&s.start).offset() <= sel_start)?;
        let index = section
            .block_marks
            .iter()
            .rposition(|m| buffer.iter_at_mark(m).offset() <= sel_start)?;
        let block_start = buffer.iter_at_mark(&section.block_marks[index]).offset();
        let block_len = section.block_lens[index];
        let rel = sel_start - block_start;
        if rel < 0 || rel >= block_len {
            return None;
        }
        let mut end_rel = (sel_end - block_start).min(block_len);
        // Abschließenden Zeilenumbruch nicht mitnehmen.
        while end_rel > rel {
            let ch = buffer.iter_at_offset(block_start + end_rel - 1).char();
            if ch == '\n' {
                end_rel -= 1;
            } else {
                break;
            }
        }
        if end_rel <= rel {
            return None;
        }
        let quote = buffer
            .text(
                &buffer.iter_at_offset(block_start + rel),
                &buffer.iter_at_offset(block_start + end_rel),
                false,
            )
            .to_string();
        let caret = buffer.iter_at_offset(block_start + rel);
        buffer.select_range(&caret, &caret);
        Some(Annotation {
            id: 0,
            law: String::new(),
            norm: section.norm.enbez.clone()?,
            paragraph: index as i64 + 1,
            start: rel as i64,
            end: end_rel as i64,
            quote,
            kind,
            color: Some(color.to_owned()),
            note: None,
            target: None,
            created: String::new(),
            modified: String::new(),
            orphaned: false,
        })
    }

    fn check_edges(&self) {
        let imp = self.imp();
        if imp.end_next.get() || imp.loading_next.get() {
            return;
        }
        let adjustment = imp.scrolled.vadjustment();
        let page = adjustment.page_size();
        if page <= 0.0 {
            return;
        }
        let remaining = adjustment.upper() - (adjustment.value() + page);
        if remaining > page * PRELOAD_PAGES {
            return;
        }
        let last = imp.sections.borrow().last().map(|s| s.norm.id);
        let Some(last) = last else {
            return;
        };
        imp.loading_next.set(true);
        let generation = imp.generation.get();
        if let Some(cb) = imp.on_need_more.borrow().as_ref() {
            cb(self, 1, last, generation);
        }
    }

    fn request_previous(&self) {
        let imp = self.imp();
        if imp.end_prev.get() || imp.loading_prev.get() {
            return;
        }
        let first = imp.sections.borrow().first().map(|s| s.norm.id);
        let Some(first) = first else {
            return;
        };
        imp.loading_prev.set(true);
        let generation = imp.generation.get();
        if let Some(cb) = imp.on_need_more.borrow().as_ref() {
            cb(self, -1, first, generation);
        }
    }

    /// Ermittelt den Abschnitt am oberen Rand des Sichtbereichs und meldet
    /// einen Wechsel. Die Zeile am Rand liefert GTK über `line_at_y`; der
    /// Vergleich läuft dann über Zeichenoffsets, die unabhängig vom
    /// Layout-Stand gültig sind.
    fn update_visible_norm(&self) {
        let imp = self.imp();
        let adjustment = imp.scrolled.vadjustment();
        // Die Norm, die am oberen Rand (plus etwas Luft) beginnt bzw. läuft.
        let y = (adjustment.value() + 32.0) as i32;
        let (line_iter, _) = imp.text_view.line_at_y(y);
        let edge = line_iter.offset();
        let buffer = imp.text_view.buffer();
        let mut current: Option<NormInfo> = None;
        for section in imp.sections.borrow().iter() {
            let start = buffer.iter_at_mark(&section.start).offset();
            if start <= edge || current.is_none() {
                current = Some(section.norm.clone());
            } else {
                break;
            }
        }
        let Some(info) = current else {
            return;
        };
        if imp.visible_norm.get() == info.id {
            return;
        }
        imp.visible_norm.set(info.id);
        if let Some(cb) = imp.on_visible_changed.borrow().as_ref() {
            cb(self, info);
        }
    }

    /// Verweis unter der Zeigerposition (Widget-Koordinaten).
    fn link_at(&self, x: f64, y: f64) -> Option<LinkTarget> {
        let imp = self.imp();
        let (bx, by) =
            imp.text_view
                .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let iter = imp.text_view.iter_at_location(bx, by)?;
        let offset = iter.offset();
        imp.links
            .borrow()
            .iter()
            .find(|l| l.start <= offset && offset < l.end)
            .map(|l| l.target.clone())
    }

    fn follow_link(&self, target: &LinkTarget, new_tab: bool) {
        let imp = self.imp();
        match target {
            LinkTarget::Footnote(mark_name) => {
                let buffer = imp.text_view.buffer();
                if let Some(mark) = buffer.mark(mark_name) {
                    imp.text_view.scroll_to_mark(&mark, 0.1, true, 0.0, 0.0);
                }
            }
            LinkTarget::Reference(norm_ref) => {
                if let Some(cb) = imp.on_navigate.borrow().as_ref() {
                    cb(self, norm_ref.clone(), new_tab);
                }
            }
        }
    }
}

// ----------------------------------------------------------------------
// Hilfsfunktionen (ohne Widgets, testbar)
// ----------------------------------------------------------------------

fn list_tag_name(depth: u8) -> String {
    format!("list{}", depth.min(MAX_LIST_DEPTH))
}

fn heading_tag_name(level: u8) -> &'static str {
    match level {
        0 | 1 => "heading1",
        2 => "heading2",
        _ => "heading3",
    }
}

/// Setzt die Farben einer Markierung (hell/dunkel) und der Notiz-Unterstreichung.
fn set_highlight_colors(
    highlight: &gtk::TextTag,
    note: &gtk::TextTag,
    block: &gtk::TextTag,
    light: &str,
    dark: &str,
    is_dark: bool,
) {
    let hex = if is_dark { dark } else { light };
    if let Ok(rgba) = gtk::gdk::RGBA::parse(hex) {
        highlight.set_background_rgba(Some(&rgba));
        let mut tint = rgba;
        tint.set_alpha(0.18);
        note.set_background_rgba(Some(&tint));
        let mut line = rgba;
        line.set_alpha(1.0);
        note.set_underline_rgba(Some(&line));
        let mut block_tint = rgba;
        block_tint.set_alpha(0.22);
        block.set_paragraph_background_rgba(Some(&block_tint));
    }
}

/// Weggefallene Normen tragen als Titel nur „(weggefallen)“.
fn is_repealed(title: Option<&str>) -> bool {
    title
        .map(|t| t.trim().trim_matches(|c| c == '(' || c == ')') == "weggefallen")
        .unwrap_or(false)
}

fn footnote_mark_name(norm_id: i64, id: &str) -> String {
    format!("footnote:{norm_id}:{id}")
}

fn font_attrs(points: i32) -> pango::AttrList {
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrSize::new(points * pango::SCALE));
    attrs
}

/// Zeichenoffset und -länge einer Absatznummer wie „(1)“ am Absatzanfang.
fn paragraph_number_range(text: &str) -> Option<(usize, usize)> {
    let label = paragraph_label(text)?;
    let leading = text.chars().take_while(|c| c.is_whitespace()).count();
    Some((leading, label.chars().count()))
}

/// Pango-Markup einer großen Tabelle als Zeilen; Zellen durch Trennstriche.
fn table_as_lines(data: &TableData) -> String {
    let mut out = String::new();
    if let Some(title) = data
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        out.push_str(&format!("<b>{}</b>\n", glib::markup_escape_text(title)));
    }
    for (r, row) in data.rows.iter().enumerate() {
        let cells: Vec<String> = row.iter().map(|c| cell_markup(&c.segs)).collect();
        let line = cells.join("  │  ");
        if r < data.header_rows {
            out.push_str(&format!("<b>{line}</b>"));
        } else {
            out.push_str(&line);
        }
        out.push('\n');
    }
    out.trim_end_matches('\n').to_owned()
}

/// Pango-Markup für den Inhalt einer Tabellenzelle.
fn cell_markup(segs: &[Seg]) -> String {
    let mut out = String::new();
    for seg in segs {
        if seg.tags.iter().any(|t| matches!(t, SegTag::Table(_))) {
            continue;
        }
        let escaped = glib::markup_escape_text(&seg.text);
        let mut open = String::new();
        let mut close = String::new();
        let mut wrap = |tag: &str| {
            open.push('<');
            open.push_str(tag);
            open.push('>');
            close.insert_str(0, &format!("</{tag}>"));
        };
        for t in &seg.tags {
            match t {
                SegTag::Bold | SegTag::ListLabel => wrap("b"),
                SegTag::Italic => wrap("i"),
                SegTag::Underline => wrap("u"),
                SegTag::Sup | SegTag::FootnoteRef(_) => wrap("sup"),
                SegTag::Sub => wrap("sub"),
                SegTag::Small => wrap("small"),
                SegTag::Pre => wrap("tt"),
                SegTag::Heading(_) => wrap("b"),
                SegTag::ListLine(_) | SegTag::Table(_) => {}
            }
        }
        out.push_str(&open);
        out.push_str(escaped.as_str());
        out.push_str(&close);
    }
    // Der abschließende Zeilenumbruch des letzten Blocks entfällt.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repealed_titles() {
        assert!(is_repealed(Some("(weggefallen)")));
        assert!(!is_repealed(Some("Sachmangel")));
        assert!(!is_repealed(None));
    }

    #[test]
    fn paragraph_number_ranges() {
        assert_eq!(paragraph_number_range("(1) Text"), Some((0, 3)));
        assert_eq!(paragraph_number_range("  (2a) Text"), Some((2, 4)));
        assert_eq!(paragraph_number_range("Text"), None);
    }

    #[test]
    fn cell_markup_escapes_and_styles() {
        let segs = vec![
            Seg {
                text: "a < b ".into(),
                tags: vec![],
            },
            Seg {
                text: "fett".into(),
                tags: vec![SegTag::Bold, SegTag::Italic],
            },
            Seg {
                text: "\n".into(),
                tags: vec![],
            },
        ];
        assert_eq!(cell_markup(&segs), "a &lt; b <b><i>fett</i></b>");
    }

    #[test]
    fn cell_markup_skips_nested_tables() {
        let segs = vec![Seg {
            text: "\u{FFFC}".into(),
            tags: vec![SegTag::Table(0)],
        }];
        assert_eq!(cell_markup(&segs), "");
    }

    #[test]
    fn tag_names() {
        assert_eq!(list_tag_name(0), "list0");
        assert_eq!(list_tag_name(99), format!("list{MAX_LIST_DEPTH}"));
        assert_eq!(heading_tag_name(1), "heading1");
        assert_eq!(heading_tag_name(7), "heading3");
    }
}
