// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Eine Karteikarte im Format A6 quer: Papier in der Farbe des Stapels,
//! rote Kopflinie, blaue Lineatur, darauf der Text einer Kartenseite.
//!
//! Die Karte ist ein Gegenstand und bleibt auch im dunklen Farbschema
//! papierhell mit dunkler Tinte. Die Lineatur richtet sich nach dem Text:
//! Jede Textzeile ist genau einen Zeilenabstand hoch, die Linien werden
//! relativ zum Textanfang gezeichnet und laufen beim Blättern mit.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

use crate::cards::html::{Block, Rendered};
use crate::refs::NormRef;

/// Seitenverhältnis von DIN A6 quer (148 × 105 mm).
pub const A6_RATIO: f64 = 148.0 / 105.0;

/// Maße einer Kartengröße in Pixeln.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    /// Abstand der Linien; zugleich die Zeilenhöhe des Texts (siehe CSS).
    pitch: i32,
    /// Höhe des Kopfbereichs über der roten Linie.
    header: i32,
    /// Seitlicher Rand des Texts.
    pad: i32,
    /// Lage der Linie unterhalb der Oberkante einer Textzeile.
    rule_offset: f32,
    radius: f32,
}

/// Lernkarte in voller Größe.
const REGULAR: Metrics = Metrics {
    pitch: 28,
    header: 44,
    pad: 26,
    rule_offset: 23.5,
    radius: 5.0,
};
/// Verkleinerte Karte für die Stapelübersicht.
const MINI: Metrics = Metrics {
    pitch: 18,
    header: 24,
    pad: 12,
    rule_offset: 15.5,
    radius: 3.5,
};

/// Papierfarbe eines Stapels als (Rot, Grün, Blau).
pub fn paper_rgb(color: &str) -> (f32, f32, f32) {
    match color {
        "yellow" => (0.985, 0.945, 0.720),
        "green" => (0.850, 0.935, 0.810),
        "blue" => (0.835, 0.905, 0.965),
        "pink" => (0.975, 0.855, 0.885),
        _ => (0.988, 0.980, 0.955),
    }
}

/// Papierfarbe, um `shade` abgedunkelt (0 = unverändert).
pub fn paper_rgba(color: &str, shade: f32) -> gdk::RGBA {
    let (r, g, b) = paper_rgb(color);
    let f = 1.0 - shade.clamp(0.0, 1.0);
    gdk::RGBA::new(r * f, g * f, b * f, 1.0)
}

/// Inhalt einer Kartenseite.
#[derive(Debug, Clone, Default)]
pub struct Face {
    /// Beschriftung oben links (Name des Stapels).
    pub caption: String,
    /// Vermerk oben rechts („Frage“, „Antwort“).
    pub corner: String,
    pub content: Rendered,
    /// Ordner mit den Bildern des Stapels.
    pub media_dir: Option<PathBuf>,
}

impl Face {
    /// Kartenseite mit einer einzigen, mittig gesetzten Aufschrift.
    pub fn title(caption: &str, title: &str) -> Self {
        Self {
            caption: caption.to_owned(),
            content: Rendered {
                blocks: vec![Block::Text(format!(
                    "<b>{}</b>",
                    glib::markup_escape_text(title)
                ))],
                links: Vec::new(),
            },
            ..Default::default()
        }
    }
}

/// Kurze Texte ohne Aufzählung stehen mittig auf der Karte, alles andere
/// beginnt oben links wie von Hand geschrieben.
fn is_centered(content: &Rendered) -> bool {
    let mut chars = 0;
    let mut lines = 0;
    for block in &content.blocks {
        match block {
            Block::Text(markup) => {
                chars += markup.chars().count();
                lines += 1 + markup.matches('\n').count();
                if markup.contains("• ") || markup.contains("\n1. ") || markup.starts_with("1. ")
                {
                    return false;
                }
            }
            Block::Image(_) | Block::Rule => return false,
        }
    }
    chars <= 260 && lines <= 4
}

/// Prüft Markup vor der Übergabe an das Label; `<a>` kennt nur das Label
/// selbst, deshalb wird es für die Prüfung durch `<span>` ersetzt.
fn markup_is_valid(markup: &str) -> bool {
    let probe = markup.replace("</a>", "</span>");
    let mut checked = String::with_capacity(probe.len());
    let mut rest = probe.as_str();
    while let Some(start) = rest.find("<a href=\"") {
        checked.push_str(&rest[..start]);
        checked.push_str("<span>");
        match rest[start..].find('>') {
            Some(end) => rest = &rest[start + end + 1..],
            None => return false,
        }
    }
    checked.push_str(rest);
    pango::parse_markup(&checked, '\0').is_ok()
}

type LinkCallback = Box<dyn Fn(&LexIndexCard, &NormRef)>;

mod imp {
    use super::*;

    pub struct LexIndexCard {
        pub caption: gtk::Label,
        pub corner: gtk::Label,
        pub scroller: gtk::ScrolledWindow,
        pub paper: ruled::RuledPaper,
        pub color: RefCell<String>,
        pub mini: Cell<bool>,
        pub links: RefCell<Vec<NormRef>>,
        pub on_link: RefCell<Option<LinkCallback>>,
        /// Zeitpunkt des letzten Klicks auf einen Verweis (monoton, µs).
        pub link_clicked_at: Cell<i64>,
    }

    impl Default for LexIndexCard {
        fn default() -> Self {
            Self {
                caption: gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(pango::EllipsizeMode::End)
                    .css_classes(["card-caption"])
                    .build(),
                corner: gtk::Label::builder()
                    .xalign(1.0)
                    .css_classes(["card-caption", "card-corner"])
                    .build(),
                scroller: gtk::ScrolledWindow::builder()
                    .hscrollbar_policy(gtk::PolicyType::Never)
                    .vscrollbar_policy(gtk::PolicyType::Automatic)
                    .build(),
                paper: ruled::RuledPaper::new(),
                color: RefCell::new("white".into()),
                mini: Cell::new(false),
                links: RefCell::default(),
                on_link: RefCell::default(),
                link_clicked_at: Cell::new(0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexIndexCard {
        const NAME: &'static str = "LexIndexCard";
        type Type = super::LexIndexCard;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("indexcard");
        }
    }

    impl ObjectImpl for LexIndexCard {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.scroller.set_child(Some(&self.paper));
            self.caption.set_parent(&*obj);
            self.corner.set_parent(&*obj);
            self.scroller.set_parent(&*obj);
            obj.apply_metrics();
        }

        fn dispose(&self) {
            self.caption.unparent();
            self.corner.unparent();
            self.scroller.unparent();
        }
    }

    impl WidgetImpl for LexIndexCard {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            // Die Größe gibt der Stapel vor; die Karte füllt, was sie bekommt.
            let (min, nat) = if self.mini.get() {
                (96, 200)
            } else {
                (220, 560)
            };
            match orientation {
                gtk::Orientation::Horizontal => (min, nat, -1, -1),
                _ => {
                    let scale = |v: i32| (f64::from(v) / A6_RATIO).round() as i32;
                    (scale(min), scale(nat), -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let m = self.obj().metrics();
            let place = |widget: &gtk::Widget, x: i32, y: i32, w: i32, h: i32| {
                let transform =
                    gsk::Transform::new().translate(&graphene::Point::new(x as f32, y as f32));
                widget.allocate(w.max(1), h.max(1), -1, Some(transform));
            };
            let inner = (width - 2 * m.pad).max(1);
            let (_, corner_w, _, _) = self.corner.measure(gtk::Orientation::Horizontal, -1);
            let corner_w = corner_w.min(inner / 2);
            let (_, label_h, _, _) = self.caption.measure(gtk::Orientation::Vertical, -1);
            // Die Beschriftung steht knapp über der roten Linie.
            let label_y = (m.header - label_h - m.header / 9).max(0);
            place(
                self.caption.upcast_ref(),
                m.pad,
                label_y,
                inner - corner_w - 8,
                label_h,
            );
            place(
                self.corner.upcast_ref(),
                width - m.pad - corner_w,
                label_y,
                corner_w,
                label_h,
            );
            let top = m.header + 2;
            let bottom = (m.radius as i32).max(3);
            place(
                self.scroller.upcast_ref(),
                0,
                top,
                width,
                height - top - bottom,
            );
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let m = obj.metrics();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let rect = graphene::Rect::new(0.0, 0.0, w, h);
            let outline = gsk::RoundedRect::from_rect(rect, m.radius);
            let color = self.color.borrow();
            // Schlagschatten: die Karte liegt auf, sie schwebt nicht.
            snapshot.append_outset_shadow(
                &outline,
                &gdk::RGBA::new(0.0, 0.0, 0.0, 0.22),
                0.0,
                1.5,
                0.0,
                if self.mini.get() { 3.0 } else { 5.0 },
            );
            snapshot.push_rounded_clip(&outline);
            // Papier mit leichtem Verlauf von oben (Licht) nach unten.
            snapshot.append_linear_gradient(
                &rect,
                &graphene::Point::new(0.0, 0.0),
                &graphene::Point::new(0.0, h),
                &[
                    gsk::ColorStop::new(0.0, paper_rgba(&color, 0.0)),
                    gsk::ColorStop::new(1.0, paper_rgba(&color, 0.035)),
                ],
            );
            // Rote Kopflinie, wie auf linierten Karteikarten üblich.
            let header = m.header as f32;
            snapshot.append_color(
                &gdk::RGBA::new(0.78, 0.17, 0.22, 0.62),
                &graphene::Rect::new(0.0, header, w, if self.mini.get() { 1.0 } else { 1.5 }),
            );
            obj.snapshot_child(&self.caption, snapshot);
            obj.snapshot_child(&self.corner, snapshot);
            obj.snapshot_child(&self.scroller, snapshot);
            snapshot.pop();
            // Feine Kante, damit sich gleichfarbige Karten voneinander abheben.
            let edge = gdk::RGBA::new(0.0, 0.0, 0.0, 0.16);
            snapshot.append_border(&outline, &[1.0; 4], &[edge; 4]);
        }
    }
}

glib::wrapper! {
    pub struct LexIndexCard(ObjectSubclass<imp::LexIndexCard>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexIndexCard {
    fn default() -> Self {
        Self::new()
    }
}

impl LexIndexCard {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn metrics(&self) -> Metrics {
        if self.imp().mini.get() {
            MINI
        } else {
            REGULAR
        }
    }

    fn apply_metrics(&self) {
        let m = self.metrics();
        self.imp()
            .paper
            .set_ruling(m.pitch, m.rule_offset, m.pad, m.pitch / 4);
        if self.imp().mini.get() {
            self.add_css_class("mini");
        } else {
            self.remove_css_class("mini");
        }
        self.queue_resize();
    }

    /// Verkleinerte Darstellung für die Stapelübersicht.
    pub fn set_mini(&self, mini: bool) {
        if self.imp().mini.replace(mini) != mini {
            self.apply_metrics();
        }
    }

    /// Papierfarbe (ein Wert aus [`crate::cards::PAPER_COLORS`]).
    pub fn set_paper(&self, color: &str) {
        if *self.imp().color.borrow() != color {
            self.imp().color.replace(color.to_owned());
            self.queue_draw();
        }
    }

    /// Rückruf für Klicks auf Normverweise im Kartentext.
    pub fn connect_link(&self, f: impl Fn(&Self, &NormRef) + 'static) {
        self.imp().on_link.replace(Some(Box::new(f)));
    }

    /// Wurde in den letzten Augenblicken ein Verweis angeklickt? Der Stapel
    /// dreht die Karte dann nicht um.
    pub fn link_just_clicked(&self) -> bool {
        glib::monotonic_time() - self.imp().link_clicked_at.get() < 400_000
    }

    /// Beschriftet die Karte mit einer Seite.
    pub fn set_face(&self, face: &Face) {
        let imp = self.imp();
        let m = self.metrics();
        imp.caption.set_label(&face.caption);
        imp.corner.set_label(&face.corner);
        imp.links.replace(face.content.links.clone());
        let centered = is_centered(&face.content);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for block in &face.content.blocks {
            match block {
                Block::Text(markup) => content.append(&self.text_label(markup, centered)),
                Block::Image(name) => {
                    content.append(&image_widget(face.media_dir.as_deref(), name, m.pitch))
                }
                Block::Rule => {
                    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
                    rule.add_css_class("card-rule");
                    rule.set_margin_top(m.pitch / 2 - 1);
                    rule.set_margin_bottom(m.pitch - m.pitch / 2);
                    content.append(&rule);
                }
            }
        }
        imp.paper.set_content(&content, centered);
        imp.scroller.vadjustment().set_value(0.0);
    }

    fn text_label(&self, markup: &str, centered: bool) -> gtk::Label {
        let label = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .xalign(if centered { 0.5 } else { 0.0 })
            .justify(if centered {
                gtk::Justification::Center
            } else {
                gtk::Justification::Left
            })
            .css_classes(["card-text"])
            .build();
        if markup_is_valid(markup) {
            label.set_markup(markup);
        } else {
            log::warn!("Kartentext mit ungültigem Markup wird ohne Auszeichnung gezeigt");
            label.set_text(&crate::cards::html::strip_tags(markup));
        }
        label.connect_activate_link(glib::clone!(
            #[weak(rename_to = card)]
            self,
            #[upgrade_or]
            glib::Propagation::Stop,
            move |_, uri| {
                let imp = card.imp();
                imp.link_clicked_at.set(glib::monotonic_time());
                let target = uri
                    .strip_prefix("ref:")
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| imp.links.borrow().get(n).cloned());
                if let Some(target) = target {
                    if let Some(callback) = imp.on_link.borrow().as_ref() {
                        callback(&card, &target);
                    }
                }
                glib::Propagation::Stop
            }
        ));
        label
    }
}

/// Bild aus dem Medienordner; seine Höhe ist ein Vielfaches des
/// Zeilenabstands, damit der folgende Text wieder auf den Linien steht.
fn image_widget(media_dir: Option<&std::path::Path>, name: &str, pitch: i32) -> gtk::Widget {
    let texture = media_dir
        .map(|dir| dir.join(name))
        .filter(|path| path.is_file())
        .and_then(|path| gdk::Texture::from_filename(path).ok());
    match texture {
        Some(texture) => {
            let rows = (texture.height() + pitch - 1) / pitch;
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_content_fit(gtk::ContentFit::ScaleDown);
            picture.set_can_shrink(true);
            picture.set_halign(gtk::Align::Center);
            picture.set_size_request(-1, rows.clamp(2, 9) * pitch);
            picture.add_css_class("card-image");
            picture.upcast()
        }
        None => gtk::Label::builder()
            .label(format!("[{name}]"))
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::Middle)
            .css_classes(["card-text", "card-missing"])
            .build()
            .upcast(),
    }
}

/// Die linierte Schreibfläche: ordnet ihren Inhalt an und zeichnet die
/// Linien dazu passend.
mod ruled {
    use super::*;

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct RuledPaper {
            pub content: RefCell<Option<gtk::Widget>>,
            pub centered: Cell<bool>,
            pub pitch: Cell<i32>,
            pub rule_offset: Cell<f32>,
            pub pad: Cell<i32>,
            pub top: Cell<i32>,
            /// Oberkante des Inhalts nach der letzten Anordnung.
            pub origin: Cell<i32>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for RuledPaper {
            const NAME: &'static str = "LexRuledPaper";
            type Type = super::RuledPaper;
            type ParentType = gtk::Widget;
        }

        impl ObjectImpl for RuledPaper {
            fn dispose(&self) {
                if let Some(content) = self.content.take() {
                    content.unparent();
                }
            }
        }

        impl WidgetImpl for RuledPaper {
            fn request_mode(&self) -> gtk::SizeRequestMode {
                gtk::SizeRequestMode::HeightForWidth
            }

            fn measure(
                &self,
                orientation: gtk::Orientation,
                for_size: i32,
            ) -> (i32, i32, i32, i32) {
                let Some(content) = self.content.borrow().clone() else {
                    return (0, 0, -1, -1);
                };
                let pad = 2 * self.pad.get();
                match orientation {
                    gtk::Orientation::Horizontal => {
                        let (min, nat, _, _) = content.measure(orientation, -1);
                        (min + pad, nat + pad, -1, -1)
                    }
                    _ => {
                        let width = if for_size < 0 {
                            -1
                        } else {
                            (for_size - pad).max(1)
                        };
                        let (min, nat, _, _) = content.measure(orientation, width);
                        let extra = self.top.get() + self.pitch.get() / 2;
                        (min + extra, nat + extra, -1, -1)
                    }
                }
            }

            fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
                let Some(content) = self.content.borrow().clone() else {
                    return;
                };
                let pad = self.pad.get();
                let inner = (width - 2 * pad).max(1);
                let (min_h, nat_h, _, _) = content.measure(gtk::Orientation::Vertical, inner);
                let content_h = nat_h.max(min_h);
                let top = self.top.get();
                // Kurzer Text steht mittig, aber nie über dem oberen Rand.
                let y = if self.centered.get() && content_h + top < height {
                    ((height - content_h) / 2 - self.pitch.get() / 3).max(top)
                } else {
                    top
                };
                self.origin.set(y);
                let transform =
                    gsk::Transform::new().translate(&graphene::Point::new(pad as f32, y as f32));
                content.allocate(inner, content_h, -1, Some(transform));
            }

            fn snapshot(&self, snapshot: &gtk::Snapshot) {
                let obj = self.obj();
                let pitch = self.pitch.get().max(4) as f32;
                let (w, h) = (obj.width() as f32, obj.height() as f32);
                let line = gdk::RGBA::new(0.16, 0.40, 0.78, 0.30);
                // Erste Linie oberhalb des Sichtbaren, die zum Text passt.
                let first = self.origin.get() as f32 + self.rule_offset.get();
                let mut y = first - (first / pitch).floor() * pitch;
                while y < h {
                    snapshot.append_color(&line, &graphene::Rect::new(0.0, y.round(), w, 1.0));
                    y += pitch;
                }
                if let Some(content) = self.content.borrow().as_ref() {
                    obj.snapshot_child(content, snapshot);
                }
            }
        }
    }

    glib::wrapper! {
        pub struct RuledPaper(ObjectSubclass<imp::RuledPaper>)
            @extends gtk::Widget,
            @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
    }

    impl RuledPaper {
        pub fn new() -> Self {
            glib::Object::new()
        }

        /// Linienabstand, Lage der Linie in der Zeile, Seitenrand, oberer Rand.
        pub fn set_ruling(&self, pitch: i32, rule_offset: f32, pad: i32, top: i32) {
            let imp = self.imp();
            imp.pitch.set(pitch);
            imp.rule_offset.set(rule_offset);
            imp.pad.set(pad);
            imp.top.set(top);
            self.queue_resize();
        }

        pub fn set_content(&self, content: &impl IsA<gtk::Widget>, centered: bool) {
            let imp = self.imp();
            if let Some(old) = imp.content.take() {
                old.unparent();
            }
            content.set_parent(self);
            imp.content.replace(Some(content.clone().upcast()));
            imp.centered.set(centered);
            self.queue_resize();
        }
    }
}
