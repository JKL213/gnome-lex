// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Die Leseansicht: eine Normansicht, auf Wunsch geteilt in zwei Ansichten
//! (links/rechts bzw. oben/unten bei Schmalbreite). Jede Ansicht („Pane“)
//! kann eine Norm eines beliebigen installierten Gesetzes zeigen. Die
//! aktive Ansicht bestimmt Titel, Blättern und das Ziel von Klicks in der
//! Gliederung.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use crate::db::Database;
use crate::model::{Annotation, AnnotationKind, LawInfo, NormInfo};
use crate::refs::{LawRef, NormRef};
use crate::widgets::norm_view::{AnnotationEvent, NormPage, RefChip};
use crate::widgets::LexNormView;

/// Was eine Ansicht gerade zeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneInfo {
    pub norm: NormInfo,
    pub law_slug: String,
    pub law_abbrev: String,
}

/// Gespeicherter Zustand der Leseansicht (GSettings `reader-state`, JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReaderState {
    /// Eine oder zwei Ansichten; die zweite bedeutet geteilte Ansicht.
    pub panes: Vec<PaneRef>,
    #[serde(default)]
    pub active: u32,
}

/// Verweis auf eine Norm über Gesetz und Bezeichnung (stabil über Updates).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaneRef {
    pub law: String,
    pub norm: String,
}

type NormChangedCallback = Box<dyn Fn(&LexReader, u32)>;
type NavigateCallback = Box<dyn Fn(&LexReader, u32, NormRef, bool)>;
type AnnotationCallback = Box<dyn Fn(&LexReader, u32, AnnotationEvent)>;

/// Liest Norm, Gesetz, Gliederungspfad und Notiz für eine Ansicht.
fn load_page(db: &Database, norm_id: i64) -> Result<Option<(NormPage, LawInfo)>, rusqlite::Error> {
    let Some(norm) = db.norm(norm_id)? else {
        return Ok(None);
    };
    let Some(law) = db.law_by_id(norm.info.law_id)? else {
        return Ok(None);
    };
    let units = db.unit_path(norm.info.unit_id)?;
    let (note, annotations) = match norm.info.enbez.as_deref() {
        Some(enbez) => {
            let all = db.annotations_for_norm(&law.slug, enbez)?;
            let note = all
                .iter()
                .find(|a| a.kind == AnnotationKind::Note && a.paragraph == 0 && a.quote.is_empty())
                .and_then(|a| a.note.clone());
            let anchored = all
                .into_iter()
                .filter(|a| a.paragraph > 0 && !a.orphaned)
                .collect();
            (note, anchored)
        }
        None => (None, Vec::new()),
    };
    let chip = |abbrev: &str, enbez: &str| RefChip {
        label: if abbrev.eq_ignore_ascii_case(&law.jurabk) {
            enbez.to_owned()
        } else {
            format!("{enbez} {abbrev}")
        },
        target: NormRef {
            law: if abbrev.eq_ignore_ascii_case(&law.jurabk) {
                LawRef::Same
            } else {
                LawRef::Abbrev(abbrev.to_owned())
            },
            norm: Some(enbez.to_owned()),
            sub_section: None,
            paragraph: None,
            sentence: None,
            number: None,
        },
    };
    let outgoing = db
        .outgoing_refs(norm.info.id)?
        .iter()
        .map(|(abbrev, enbez)| chip(abbrev, enbez))
        .collect();
    let incoming = match norm.info.enbez.as_deref() {
        Some(enbez) => db
            .incoming_refs(&law.jurabk, enbez, 40)?
            .iter()
            .filter(|(info, _)| info.id != norm.info.id)
            .filter_map(|(info, abbrev)| info.enbez.as_deref().map(|e| chip(abbrev, e)))
            .collect(),
        None => Vec::new(),
    };
    Ok(Some((
        NormPage {
            norm,
            units,
            law_abbrev: law.jurabk.clone(),
            note,
            annotations,
            outgoing,
            incoming,
        },
        law,
    )))
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate, glib::Properties)]
    #[template(resource = "/org/gnomelex/Gesetze/ui/reader.ui")]
    #[properties(wrapper_type = super::LexReader)]
    pub struct LexReader {
        #[template_child]
        pub paned: TemplateChild<gtk::Paned>,
        #[template_child]
        pub pane0: TemplateChild<gtk::Box>,
        #[template_child]
        pub pane1: TemplateChild<gtk::Box>,
        #[template_child]
        pub caption0: TemplateChild<gtk::Label>,
        #[template_child]
        pub caption1: TemplateChild<gtk::Label>,
        #[template_child]
        pub view0: TemplateChild<LexNormView>,
        #[template_child]
        pub view1: TemplateChild<LexNormView>,
        /// Titel der aktiven Ansicht, z. B. „§ 433 BGB“.
        #[property(get)]
        pub title: RefCell<String>,
        /// Untertitel der aktiven Ansicht (Titel der Norm).
        #[property(get)]
        pub subtitle: RefCell<String>,
        /// Geteilte Ansicht aktiv?
        #[property(get)]
        pub split: Cell<bool>,
        /// Aktive Ansicht (0 oder 1).
        #[property(get)]
        pub active_pane: Cell<u32>,
        pub panes: [RefCell<Option<PaneInfo>>; 2],
        /// Laufende Nummern der letzten Normanfragen je Ansicht.
        pub serial: [Cell<u64>; 2],
        pub on_norm_changed: RefCell<Option<NormChangedCallback>>,
        pub on_navigate: RefCell<Option<NavigateCallback>>,
        pub on_annotation: RefCell<Option<AnnotationCallback>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexReader {
        const NAME: &'static str = "LexReader";
        type Type = super::LexReader;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            LexNormView::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for LexReader {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gestures();
            obj.setup_continuous_reading();
        }
    }

    impl WidgetImpl for LexReader {}
    impl BinImpl for LexReader {}
}

glib::wrapper! {
    pub struct LexReader(ObjectSubclass<imp::LexReader>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexReader {
    fn default() -> Self {
        Self::new()
    }
}

impl LexReader {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Wird aufgerufen, wenn sich die Norm einer Ansicht oder die aktive
    /// Ansicht ändert (Parameter: Index der Ansicht).
    pub fn connect_norm_changed(&self, f: impl Fn(&Self, u32) + 'static) {
        *self.imp().on_norm_changed.borrow_mut() = Some(Box::new(f));
    }

    /// Wird aufgerufen, wenn in einer Ansicht ein Verweis angeklickt wurde
    /// (Ansicht, Ziel, zweite Ansicht gewünscht).
    pub fn connect_navigate(&self, f: impl Fn(&Self, u32, NormRef, bool) + 'static) {
        *self.imp().on_navigate.borrow_mut() = Some(Box::new(f));
    }

    /// Wird aufgerufen, wenn der Leser eine Markierung oder Notiz anlegt,
    /// ändert oder löscht (Ansicht, Ereignis mit ergänztem Gesetzes-Slug).
    pub fn connect_annotation(&self, f: impl Fn(&Self, u32, AnnotationEvent) + 'static) {
        *self.imp().on_annotation.borrow_mut() = Some(Box::new(f));
    }

    /// Zeigt eine gespeicherte Annotation in beiden Ansichten.
    pub fn apply_annotation(&self, law_id: i64, annotation: &Annotation) {
        let imp = self.imp();
        imp.view0.apply_annotation(law_id, annotation);
        imp.view1.apply_annotation(law_id, annotation);
    }

    pub fn remove_annotation(&self, id: i64) {
        let imp = self.imp();
        imp.view0.remove_annotation(id);
        imp.view1.remove_annotation(id);
    }

    /// Noch nicht gemeldete Änderungen an Inline-Notizen sofort melden.
    pub fn flush_note_edits(&self) {
        let imp = self.imp();
        imp.view0.flush_note_edits();
        imp.view1.flush_note_edits();
    }

    fn emit_norm_changed(&self, pane: u32) {
        if let Some(cb) = self.imp().on_norm_changed.borrow().as_ref() {
            cb(self, pane);
        }
    }

    /// Ein Klick in eine Ansicht macht sie zur aktiven.
    fn setup_gestures(&self) {
        let imp = self.imp();
        for (index, pane) in [(0u32, &*imp.pane0), (1u32, &*imp.pane1)] {
            let gesture = gtk::GestureClick::builder()
                .button(0)
                .propagation_phase(gtk::PropagationPhase::Capture)
                .build();
            gesture.connect_pressed(glib::clone!(
                #[weak(rename_to = tab)]
                self,
                move |_, _, _, _| tab.set_active_pane(index)
            ));
            pane.add_controller(gesture);
        }
    }

    /// Verbindet beide Ansichten mit dem Nachladen benachbarter Normen und
    /// der Meldung der oben sichtbaren Norm.
    fn setup_continuous_reading(&self) {
        let imp = self.imp();
        for (index, view) in [(0u32, &*imp.view0), (1u32, &*imp.view1)] {
            view.connect_need_more(glib::clone!(
                #[weak(rename_to = tab)]
                self,
                move |view, direction, edge_id, generation| {
                    tab.load_neighbor(view, index, direction, edge_id, generation)
                }
            ));
            view.connect_visible_changed(glib::clone!(
                #[weak(rename_to = tab)]
                self,
                move |_, info| tab.on_visible_norm_changed(index, info)
            ));
            view.connect_navigate(glib::clone!(
                #[weak(rename_to = tab)]
                self,
                move |_, target, new_tab| {
                    tab.set_active_pane(index);
                    if let Some(cb) = tab.imp().on_navigate.borrow().as_ref() {
                        cb(&tab, index, target, new_tab);
                    }
                }
            ));
            view.connect_annotation(glib::clone!(
                #[weak(rename_to = tab)]
                self,
                move |_, event| {
                    let event = match event {
                        AnnotationEvent::Create(mut a) => {
                            let Some(info) = tab.pane_info(index) else {
                                return;
                            };
                            a.law = info.law_slug;
                            AnnotationEvent::Create(a)
                        }
                        other => other,
                    };
                    if let Some(cb) = tab.imp().on_annotation.borrow().as_ref() {
                        cb(&tab, index, event);
                    }
                }
            ));
        }
    }

    /// Aktualisiert die Notiz einer Norm in beiden Ansichten.
    pub fn set_note(&self, law_id: i64, enbez: &str, text: Option<&str>) {
        let imp = self.imp();
        imp.view0.set_note(law_id, enbez, text);
        imp.view1.set_note(law_id, enbez, text);
    }

    /// Lädt die Nachbarnorm von `edge_id` und hängt sie an die Ansicht an
    /// bzw. stellt sie voran; ohne Nachbarn wird das Ende gemeldet.
    fn load_neighbor(
        &self,
        view: &LexNormView,
        _pane: u32,
        direction: i64,
        edge_id: i64,
        generation: u64,
    ) {
        glib::spawn_future_local(glib::clone!(
            #[weak]
            view,
            async move {
                let path = Database::default_path();
                let result =
                    gio::spawn_blocking(move || -> Result<Option<NormPage>, rusqlite::Error> {
                        let db = Database::open(&path)?;
                        match db.neighbor_norm(edge_id, direction)? {
                            Some(id) => Ok(load_page(&db, id)?.map(|(page, _)| page)),
                            None => Ok(None),
                        }
                    })
                    .await;
                match result {
                    Ok(Ok(Some(page))) if direction < 0 => view.prepend_norm(page, generation),
                    Ok(Ok(Some(page))) => view.append_norm(page, generation),
                    Ok(Ok(None)) => view.set_end_reached(direction, generation),
                    Ok(Err(err)) => {
                        log::warn!("Nachbarnorm konnte nicht geladen werden: {err}");
                        view.set_end_reached(direction, generation);
                    }
                    Err(_) => view.set_end_reached(direction, generation),
                }
            }
        ));
    }

    /// Beim Scrollen liegt eine andere Norm oben: Ansichtsinfo, Titel und
    /// Beschriftung nachführen (das Gesetz bleibt dasselbe).
    fn on_visible_norm_changed(&self, pane: u32, info: NormInfo) {
        let imp = self.imp();
        {
            let mut slot = imp.panes[pane as usize].borrow_mut();
            match slot.as_mut() {
                Some(current) if current.norm.law_id == info.law_id => current.norm = info,
                _ => return,
            }
        }
        self.update_captions();
        if pane == self.active_pane() {
            self.update_title();
        }
        self.emit_norm_changed(pane);
    }

    /// Nur für Tests: Befehl an die aktive Ansicht (siehe `LexNormView::test_command`).
    pub fn test_command(&self, command: &str) {
        let imp = self.imp();
        let view = if self.active_pane() == 0 {
            &imp.view0
        } else {
            &imp.view1
        };
        view.test_command(command);
    }

    /// Nur für Tests: scrollt die aktive Ansicht (siehe `LexNormView::scroll_to_fraction`).
    pub fn scroll_active(&self, fraction: f64) {
        let imp = self.imp();
        let view = if self.active_pane() == 0 {
            &imp.view0
        } else {
            &imp.view1
        };
        view.scroll_to_fraction(fraction);
    }

    pub fn set_font_size(&self, points: i32) {
        let imp = self.imp();
        imp.view0.set_font_size(points);
        imp.view1.set_font_size(points);
    }

    /// Ansichten untereinander (Schmalbreite) statt nebeneinander.
    pub fn set_vertical(&self, vertical: bool) {
        self.imp().paned.set_orientation(if vertical {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
    }

    pub fn pane_info(&self, pane: u32) -> Option<PaneInfo> {
        self.imp().panes.get(pane as usize)?.borrow().clone()
    }

    pub fn active_info(&self) -> Option<PaneInfo> {
        self.pane_info(self.active_pane())
    }

    pub fn active_norm_id(&self) -> Option<i64> {
        self.active_info().map(|p| p.norm.id)
    }

    pub fn set_active_pane(&self, pane: u32) {
        let imp = self.imp();
        let pane = if imp.split.get() { pane.min(1) } else { 0 };
        if imp.active_pane.get() == pane {
            return;
        }
        imp.active_pane.set(pane);
        self.update_captions();
        self.update_title();
        self.notify_active_pane();
        self.emit_norm_changed(pane);
    }

    /// Wechselt zwischen den beiden Ansichten (nur bei geteilter Ansicht).
    pub fn switch_pane(&self) {
        self.set_active_pane(1 - self.active_pane().min(1));
    }

    /// Schaltet die geteilte Ansicht ein oder aus. Beim Einschalten zeigt
    /// die zweite Ansicht zunächst dieselbe Norm und wird aktiv.
    pub fn set_split(&self, split: bool) {
        let imp = self.imp();
        if imp.split.get() == split {
            return;
        }
        imp.split.set(split);
        imp.pane1.set_visible(split);
        imp.caption0.set_visible(split);
        imp.caption1.set_visible(split);
        if split {
            if imp.panes[1].borrow().is_none() {
                if let Some(id) = self.pane_info(0).map(|p| p.norm.id) {
                    self.show_norm_in_pane(id, 1);
                }
            }
            self.notify_split();
            self.set_active_pane(1);
        } else {
            *imp.panes[1].borrow_mut() = None;
            imp.view1.set_norm(None);
            self.notify_split();
            self.set_active_pane(0);
        }
    }

    pub fn toggle_split(&self) {
        self.set_split(!self.split());
    }

    /// Zeigt eine Norm in der jeweils anderen Ansicht (schaltet die Teilung
    /// bei Bedarf ein) und macht diese aktiv – Ersatz für „in neuem Tab“.
    pub fn show_in_other_pane(&self, norm_id: i64) {
        if !self.split() {
            self.set_split(true);
        }
        let other = 1 - self.active_pane().min(1);
        self.show_norm_in_pane(norm_id, other);
        self.set_active_pane(other);
    }

    /// Zeigt eine Norm in der aktiven Ansicht.
    pub fn show_norm(&self, norm_id: i64) {
        self.show_norm_in_pane(norm_id, self.active_pane());
    }

    /// Lädt eine Norm samt Gesetz aus der Datenbank und zeigt sie in der
    /// Ansicht `pane`.
    pub fn show_norm_in_pane(&self, norm_id: i64, pane: u32) {
        let imp = self.imp();
        let pane = pane.min(1);
        let serial = imp.serial[pane as usize].get().wrapping_add(1);
        imp.serial[pane as usize].set(serial);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = tab)]
            self,
            async move {
                let path = Database::default_path();
                let result = gio::spawn_blocking(move || {
                    let db = Database::open(&path)?;
                    load_page(&db, norm_id)
                })
                .await;
                if tab.imp().serial[pane as usize].get() != serial {
                    return;
                }
                match result {
                    Ok(Ok(Some((page, law)))) => tab.present_in_pane(pane, page, law),
                    Ok(Ok(None)) => log::warn!("Norm {norm_id} nicht gefunden"),
                    Ok(Err(err)) => log::warn!("Norm {norm_id} konnte nicht geladen werden: {err}"),
                    Err(_) => log::warn!("Laden der Norm {norm_id} abgebrochen"),
                }
            }
        ));
    }

    fn present_in_pane(&self, pane: u32, page: NormPage, law: LawInfo) {
        let imp = self.imp();
        let info = PaneInfo {
            norm: page.norm.info.clone(),
            law_slug: law.slug,
            law_abbrev: law.jurabk,
        };
        *imp.panes[pane as usize].borrow_mut() = Some(info);
        let view = if pane == 0 { &imp.view0 } else { &imp.view1 };
        view.set_norm(Some(page));
        self.update_captions();
        if pane == self.active_pane() {
            self.update_title();
        }
        self.emit_norm_changed(pane);
    }

    fn update_captions(&self) {
        let imp = self.imp();
        let active = imp.active_pane.get();
        for (index, caption) in [(0u32, &*imp.caption0), (1u32, &*imp.caption1)] {
            let text = self
                .pane_info(index)
                .map(|p| pane_caption(&p))
                .unwrap_or_default();
            caption.set_label(&text);
            if index == active {
                caption.add_css_class("active");
            } else {
                caption.remove_css_class("active");
            }
        }
    }

    fn update_title(&self) {
        let imp = self.imp();
        let (title, subtitle) = match self.active_info() {
            Some(info) => pane_title(&info),
            None => (String::new(), String::new()),
        };
        let changed_title = *imp.title.borrow() != title;
        let changed_subtitle = *imp.subtitle.borrow() != subtitle;
        *imp.title.borrow_mut() = title;
        *imp.subtitle.borrow_mut() = subtitle;
        if changed_title {
            self.notify_title();
        }
        if changed_subtitle {
            self.notify_subtitle();
        }
    }

    /// Zustand für die Wiederherstellung beim nächsten Start.
    pub fn state(&self) -> ReaderState {
        let mut panes = Vec::new();
        for index in 0..=1u32 {
            if index == 1 && !self.split() {
                break;
            }
            if let Some(info) = self.pane_info(index) {
                if let Some(enbez) = info.norm.enbez.clone() {
                    panes.push(PaneRef {
                        law: info.law_slug,
                        norm: enbez,
                    });
                }
            }
        }
        ReaderState {
            panes,
            active: self.active_pane(),
        }
    }

    /// Ansichten, die eine Norm des Gesetzes `slug` zeigen, mit deren
    /// Bezeichnung – zum Neuladen nach einem Import.
    pub fn panes_showing(&self, slug: &str) -> Vec<(u32, String)> {
        (0..=1u32)
            .filter_map(|index| {
                let info = self.pane_info(index)?;
                (info.law_slug == slug).then_some((index, info.norm.enbez?))
            })
            .collect()
    }
}

/// Titel und Untertitel für eine Ansicht.
fn pane_title(info: &PaneInfo) -> (String, String) {
    let title = match &info.norm.enbez {
        Some(enbez) => format!("{enbez} {}", info.law_abbrev),
        None => info
            .norm
            .titel
            .clone()
            .unwrap_or_else(|| info.norm.doknr.clone()),
    };
    let subtitle = match &info.norm.enbez {
        Some(_) => info.norm.titel.clone().unwrap_or_default(),
        None => info.law_abbrev.clone(),
    };
    (title, subtitle)
}

/// Beschriftung über einer Ansicht in der geteilten Darstellung.
fn pane_caption(info: &PaneInfo) -> String {
    let (title, subtitle) = pane_title(info);
    if subtitle.is_empty() {
        title
    } else {
        format!("{title} – {subtitle}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(enbez: Option<&str>, titel: Option<&str>) -> PaneInfo {
        PaneInfo {
            norm: NormInfo {
                enbez: enbez.map(str::to_owned),
                titel: titel.map(str::to_owned),
                doknr: "DOK".into(),
                ..Default::default()
            },
            law_slug: "bgb".into(),
            law_abbrev: "BGB".into(),
        }
    }

    #[test]
    fn titles_and_captions() {
        let i = info(Some("§ 433"), Some("Pflichten"));
        assert_eq!(pane_title(&i), ("§ 433 BGB".into(), "Pflichten".into()));
        assert_eq!(pane_caption(&i), "§ 433 BGB – Pflichten");
        let i = info(None, Some("Inhaltsübersicht"));
        assert_eq!(pane_title(&i), ("Inhaltsübersicht".into(), "BGB".into()));
        let i = info(None, None);
        assert_eq!(pane_title(&i).0, "DOK");
    }

    #[test]
    fn tab_state_roundtrip() {
        let state = ReaderState {
            panes: vec![
                PaneRef {
                    law: "bgb".into(),
                    norm: "§ 433".into(),
                },
                PaneRef {
                    law: "zpo".into(),
                    norm: "§ 253".into(),
                },
            ],
            active: 1,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<ReaderState>(&json).unwrap(), state);
        let old: ReaderState = serde_json::from_str(r#"{"panes":[]}"#).unwrap();
        assert_eq!(old.active, 0);
    }
}
