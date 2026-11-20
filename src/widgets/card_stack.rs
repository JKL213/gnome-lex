// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Ein Stapel Karteikarten, plastisch gezeichnet: unter der obersten Karte
//! liegen die übrigen leicht versetzt, die Höhe des Stapels folgt der Zahl
//! der Karten. Die oberste Karte lässt sich umdrehen (Drehung um die
//! Hochachse) und fliegt nach der Bewertung zur Seite vom Stapel. Alle
//! Bewegungen sind flache Transformationen, die jeder Renderer beherrscht.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use super::index_card::{paper_rgba, LexIndexCard, A6_RATIO};

/// Dauer des Umdrehens und des Wegfliegens in Millisekunden.
const FLIP_MS: u32 = 420;
const FLY_MS: u32 = 300;
/// Blickwinkel in Grad, unter dem die Karte beim Umdrehen gesehen wird.
const FLIP_PITCH: f32 = 26.0;
/// Größte Breite der Karte in voller Größe und in der Übersicht.
const MAX_CARD_WIDTH: f32 = 680.0;
const MAX_MINI_WIDTH: f32 = 208.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Motion {
    #[default]
    None,
    /// Die oberste Karte dreht sich um; bei der Hälfte wechselt die Seite.
    Flip,
    /// Die bisher oberste Karte fliegt in Richtung des Vorzeichens davon.
    FlyOut(f32),
    /// Eine Karte kommt aus Richtung des Vorzeichens auf den Stapel zurück.
    FlyIn(f32),
}

type CardCallback = Box<dyn FnOnce(&LexIndexCard)>;
type ClickCallback = Box<dyn Fn(&LexCardStack)>;

/// Zahl der sichtbaren Lagen unter der obersten Karte: Die ersten Karten
/// zählen einzeln, danach wächst der Stapel nur noch langsam.
pub fn pile_layers(depth: u32, mini: bool) -> u32 {
    let (single, max) = if mini { (4, 9) } else { (6, 16) };
    if depth <= single {
        depth
    } else {
        (single + f64::from(depth - single).sqrt().ceil() as u32).min(max)
    }
}

/// Kleine, feste Unregelmäßigkeit je Lage (−1 … 1), damit der Stapel nicht
/// wie gedruckt aussieht.
fn jitter(layer: u32, salt: u32) -> f32 {
    let hash = (layer.wrapping_mul(2_654_435_761) ^ salt.wrapping_mul(40_503)) >> 8;
    (hash % 201) as f32 / 100.0 - 1.0
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LexCardStack {
        pub cards: [LexIndexCard; 2],
        /// Index der obersten Karte in `cards`; die andere dient als
        /// „Geisterkarte“ für Bewegungen.
        pub top: Cell<usize>,
        pub motion: Cell<Motion>,
        pub progress: Cell<f64>,
        pub half: RefCell<Option<CardCallback>>,
        pub done: RefCell<Option<Box<dyn FnOnce()>>>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
        /// Karten unter der obersten.
        pub depth: Cell<u32>,
        pub color: RefCell<String>,
        pub mini: Cell<bool>,
        /// Es liegt keine Karte mehr oben (Stapel abgearbeitet).
        pub empty: Cell<bool>,
        /// Lage der obersten Karte: x, y, Breite, Höhe.
        pub card_rect: Cell<(f32, f32, f32, f32)>,
        pub on_click: RefCell<Option<ClickCallback>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexCardStack {
        const NAME: &'static str = "LexCardStack";
        type Type = super::LexCardStack;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("cardstack");
        }
    }

    impl ObjectImpl for LexCardStack {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.color.replace("white".into());
            for card in &self.cards {
                card.set_parent(&*obj);
            }
            self.cards[1].set_child_visible(false);
            let click = gtk::GestureClick::builder().button(1).build();
            click.connect_released(glib::clone!(
                #[weak]
                obj,
                move |_, _, x, y| obj.on_released(x, y)
            ));
            obj.add_controller(click);
        }

        fn dispose(&self) {
            if let Some(animation) = self.animation.take() {
                animation.reset();
            }
            for card in &self.cards {
                card.unparent();
            }
        }
    }

    impl WidgetImpl for LexCardStack {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let mini = self.mini.get();
            let (min_w, nat_w) = if mini { (170, 232) } else { (260, 740) };
            match orientation {
                gtk::Orientation::Horizontal => (min_w, nat_w, -1, -1),
                _ => {
                    let height = |w: i32| (f64::from(w) / A6_RATIO).round() as i32;
                    let extra = if mini { 30 } else { 64 };
                    let natural = height(nat_w) + extra;
                    // Kacheln der Übersicht behalten ihre Höhe, auch wenn die
                    // Liste gescrollt werden muss.
                    let minimum = if mini {
                        natural
                    } else {
                        height(min_w) + extra / 2
                    };
                    (minimum, natural, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let mini = self.mini.get();
            // Rand für Schatten und für die Lagen unter der obersten Karte.
            let (margin_x, margin_top, margin_bottom) = if mini {
                (12.0, 8.0, 22.0)
            } else {
                (24.0, 14.0, 50.0)
            };
            let avail_w = (width as f32 - 2.0 * margin_x).max(40.0);
            let avail_h = (height as f32 - margin_top - margin_bottom).max(30.0);
            let max_w = if mini { MAX_MINI_WIDTH } else { MAX_CARD_WIDTH };
            let w = avail_w.min(avail_h * A6_RATIO as f32).min(max_w).floor();
            let h = (w / A6_RATIO as f32).floor();
            let x = ((width as f32 - w) / 2.0).floor();
            let y = (margin_top + (avail_h - h) / 2.0).floor();
            self.card_rect.set((x, y, w, h));
            for card in &self.cards {
                let transform = gsk::Transform::new().translate(&graphene::Point::new(x, y));
                card.allocate(w as i32, h as i32, -1, Some(transform));
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let top = &self.cards[self.top.get()];
            let ghost = &self.cards[1 - self.top.get()];
            let progress = self.progress.get() as f32;
            obj.draw_pile(snapshot);
            match self.motion.get() {
                Motion::None => {
                    if !self.empty.get() {
                        obj.snapshot_child(top, snapshot);
                    }
                }
                Motion::Flip => {
                    snapshot.save();
                    obj.flip_transform(snapshot, progress);
                    obj.snapshot_child(top, snapshot);
                    snapshot.restore();
                }
                Motion::FlyOut(direction) => {
                    if !self.empty.get() {
                        obj.snapshot_child(top, snapshot);
                    }
                    obj.snapshot_flying(snapshot, ghost, direction, progress);
                }
                Motion::FlyIn(direction) => {
                    obj.snapshot_child(ghost, snapshot);
                    obj.snapshot_flying(snapshot, top, direction, 1.0 - progress);
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct LexCardStack(ObjectSubclass<imp::LexCardStack>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexCardStack {
    fn default() -> Self {
        Self::new()
    }
}

impl LexCardStack {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Oberste Karte des Stapels.
    pub fn card(&self) -> &LexIndexCard {
        let imp = self.imp();
        &imp.cards[imp.top.get()]
    }

    /// Beide Kartenwidgets (für Einstellungen, die jede Karte betreffen).
    pub fn cards(&self) -> &[LexIndexCard; 2] {
        &self.imp().cards
    }

    /// Verkleinerte Darstellung für die Stapelübersicht.
    pub fn set_mini(&self, mini: bool) {
        self.imp().mini.set(mini);
        for card in &self.imp().cards {
            card.set_mini(mini);
        }
        self.queue_resize();
    }

    pub fn set_paper(&self, color: &str) {
        self.imp().color.replace(color.to_owned());
        for card in &self.imp().cards {
            card.set_paper(color);
        }
        self.queue_draw();
    }

    /// Zahl der Karten unter der obersten.
    pub fn set_depth(&self, depth: u32) {
        if self.imp().depth.replace(depth) != depth {
            self.queue_draw();
        }
    }

    /// Rückruf für einen Klick auf die oberste Karte.
    pub fn connect_clicked(&self, f: impl Fn(&Self) + 'static) {
        self.imp().on_click.replace(Some(Box::new(f)));
        self.set_cursor_from_name(Some("pointer"));
    }

    pub fn is_animating(&self) -> bool {
        self.imp().motion.get() != Motion::None
    }

    /// Legt ohne Bewegung eine Karte oben auf den Stapel.
    pub fn show(&self, prepare: impl FnOnce(&LexIndexCard)) {
        self.finish_animation();
        self.imp().empty.set(false);
        prepare(self.card());
        self.card().set_child_visible(true);
        self.queue_draw();
    }

    /// Dreht die oberste Karte um; `swap` beschriftet sie in dem Moment, in
    /// dem sie auf der Kante steht.
    pub fn flip(&self, swap: impl FnOnce(&LexIndexCard) + 'static) {
        self.finish_animation();
        self.imp().half.replace(Some(Box::new(swap)));
        self.start(Motion::Flip, FLIP_MS, adw::Easing::EaseInOutSine);
    }

    /// Die oberste Karte fliegt zur Seite (`direction` < 0 links, > 0
    /// rechts). `next` beschriftet die Karte, die darunter zum Vorschein
    /// kommt; ohne sie ist der Stapel danach leer. `done` läuft am Ende.
    pub fn fly_out(
        &self,
        direction: f32,
        next: Option<CardCallback>,
        done: impl FnOnce() + 'static,
    ) {
        self.finish_animation();
        let imp = self.imp();
        imp.top.set(1 - imp.top.get());
        match next {
            Some(prepare) => {
                prepare(self.card());
                imp.empty.set(false);
            }
            None => imp.empty.set(true),
        }
        imp.done.replace(Some(Box::new(done)));
        self.start(Motion::FlyOut(direction), FLY_MS, adw::Easing::EaseInCubic);
    }

    /// Eine Karte kommt von der Seite zurück auf den Stapel (Zurückblättern).
    pub fn fly_in(&self, direction: f32, prepare: impl FnOnce(&LexIndexCard)) {
        self.finish_animation();
        let imp = self.imp();
        imp.top.set(1 - imp.top.get());
        imp.empty.set(false);
        prepare(self.card());
        self.start(Motion::FlyIn(direction), FLY_MS, adw::Easing::EaseOutCubic);
    }

    fn start(&self, motion: Motion, duration: u32, easing: adw::Easing) {
        let imp = self.imp();
        imp.motion.set(motion);
        imp.progress.set(0.0);
        for card in &imp.cards {
            card.set_child_visible(true);
        }
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            move |value| stack.on_progress(value)
        ));
        let animation = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(0.0)
            .value_to(1.0)
            .duration(duration)
            .easing(easing)
            .target(&target)
            .build();
        animation.connect_done(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            move |_| stack.on_done()
        ));
        imp.animation.replace(Some(animation.clone()));
        animation.play();
    }

    fn on_progress(&self, value: f64) {
        let imp = self.imp();
        imp.progress.set(value);
        if value >= 0.5 && imp.motion.get() == Motion::Flip {
            if let Some(swap) = imp.half.take() {
                swap(self.card());
            }
        }
        self.queue_draw();
    }

    fn on_done(&self) {
        let imp = self.imp();
        if imp.motion.get() == Motion::None {
            return;
        }
        // Auch ohne Animationen (Systemeinstellung) wechselt die Seite.
        if let Some(swap) = imp.half.take() {
            swap(self.card());
        }
        imp.motion.set(Motion::None);
        imp.progress.set(0.0);
        imp.animation.take();
        imp.cards[1 - imp.top.get()].set_child_visible(false);
        self.card().set_child_visible(!imp.empty.get());
        self.queue_draw();
        if let Some(done) = imp.done.take() {
            done();
        }
    }

    /// Bringt eine laufende Bewegung sofort zu Ende.
    fn finish_animation(&self) {
        let animation = self.imp().animation.borrow().clone();
        if let Some(animation) = animation {
            animation.skip();
        }
        self.on_done();
    }

    fn on_released(&self, x: f64, y: f64) {
        let imp = self.imp();
        let (cx, cy, w, h) = imp.card_rect.get();
        let inside = x as f32 >= cx && x as f32 <= cx + w && y as f32 >= cy && y as f32 <= cy + h;
        if !inside || imp.empty.get() {
            return;
        }
        // Erst nach dem Klick entscheiden: Ein Verweis im Text meldet sich
        // über sein eigenes Signal und dreht die Karte nicht um.
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            move || {
                if stack.card().link_just_clicked() {
                    return;
                }
                if let Some(callback) = stack.imp().on_click.borrow().as_ref() {
                    callback(&stack);
                }
            }
        ));
    }

    /// Drehung der obersten Karte um ihre Hochachse. Die Karte wird dabei
    /// schmaler und kippt leicht, als sähe man von schräg oben auf sie; das
    /// kommt ohne räumliche Transformation aus und sieht deshalb mit jedem
    /// Renderer gleich aus.
    fn flip_transform(&self, snapshot: &gtk::Snapshot, progress: f32) {
        let (x, y, w, h) = self.imp().card_rect.get();
        let center = graphene::Point::new(x + w / 2.0, y + h / 2.0);
        // 0° → 90° mit der alten Seite, −90° → 0° mit der neuen.
        let angle = if progress < 0.5 {
            180.0 * progress
        } else {
            180.0 * progress - 180.0
        }
        .to_radians();
        // Blickwinkel von oben: am größten, wenn die Karte auf der Kante steht.
        let pitch = FLIP_PITCH.to_radians() * angle.sin().abs();
        let lift = 1.0 + 0.06 * (std::f32::consts::PI * progress).sin();
        let shear = (-angle.sin() * pitch.sin() / pitch.cos())
            .atan()
            .to_degrees();
        let transform = gsk::Transform::new()
            .translate(&center)
            .scale(lift * angle.cos().abs().max(0.015), lift * pitch.cos())
            .skew(0.0, shear)
            .translate(&graphene::Point::new(-center.x(), -center.y()));
        snapshot.transform(Some(&transform));
    }

    /// Zeichnet eine Karte auf ihrem Weg vom Stapel (`progress` 0 = liegt
    /// auf, 1 = weg).
    fn snapshot_flying(
        &self,
        snapshot: &gtk::Snapshot,
        card: &LexIndexCard,
        direction: f32,
        progress: f32,
    ) {
        let (x, y, w, h) = self.imp().card_rect.get();
        let center = graphene::Point::new(x + w / 2.0, y + h / 2.0);
        snapshot.save();
        snapshot.translate(&graphene::Point::new(
            direction * progress * w * 0.75,
            -progress * h * 0.10,
        ));
        snapshot.translate(&center);
        snapshot.rotate(direction * progress * 14.0);
        snapshot.translate(&graphene::Point::new(-center.x(), -center.y()));
        snapshot.push_opacity(f64::from(1.0 - progress * progress));
        self.snapshot_child(card, snapshot);
        snapshot.pop();
        snapshot.restore();
    }

    /// Die Karten unter der obersten: von unten nach oben, jede Lage ein
    /// Stück höher, leicht verschoben und verdreht.
    fn draw_pile(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let mini = imp.mini.get();
        let (x, y, w, h) = imp.card_rect.get();
        if w <= 0.0 {
            return;
        }
        let layers = pile_layers(imp.depth.get(), mini);
        let (step, radius) = if mini { (1.4, 3.5) } else { (2.2, 5.0) };
        let color = imp.color.borrow();
        let edge = gdk::RGBA::new(0.0, 0.0, 0.0, 0.20);
        let center = graphene::Point::new(x + w / 2.0, y + h / 2.0);
        let has_top = !imp.empty.get() || imp.motion.get() != Motion::None;
        if layers == 0 && !has_top {
            return;
        }
        // Schatten des ganzen Stapels auf der Unterlage; er wächst mit der Höhe.
        let base = graphene::Rect::new(x, y + layers as f32 * step, w, h);
        let lift = 1.0 + layers as f32 / 6.0;
        snapshot.append_outset_shadow(
            &gsk::RoundedRect::from_rect(base, radius),
            &gdk::RGBA::new(0.0, 0.0, 0.0, if mini { 0.20 } else { 0.24 }),
            0.0,
            if mini { 2.0 } else { 4.0 } * lift.min(2.5),
            0.0,
            if mini { 5.0 } else { 9.0 } * lift.min(2.5),
        );
        for layer in (1..=layers).rev() {
            let dx = jitter(layer, 1) * if mini { 0.8 } else { 1.6 };
            // Nur einzelne Karten liegen sichtbar schief.
            let angle = if layer % 3 == 0 {
                jitter(layer, 2) * if mini { 0.9 } else { 0.7 }
            } else {
                jitter(layer, 3) * 0.15
            };
            let rect = graphene::Rect::new(x + dx, y + layer as f32 * step, w, h);
            let outline = gsk::RoundedRect::from_rect(rect, radius);
            snapshot.save();
            snapshot.translate(&center);
            snapshot.rotate(angle);
            snapshot.translate(&graphene::Point::new(-center.x(), -center.y()));
            snapshot.push_rounded_clip(&outline);
            let shade = (0.03 + 0.012 * layer as f32).min(0.16);
            snapshot.append_color(&paper_rgba(&color, shade), &rect);
            snapshot.pop();
            snapshot.append_border(&outline, &[1.0; 4], &[edge; 4]);
            snapshot.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pile_grows_with_the_number_of_cards() {
        assert_eq!(pile_layers(0, false), 0);
        assert_eq!(pile_layers(3, false), 3);
        assert_eq!(pile_layers(6, false), 6);
        assert!(pile_layers(30, false) > pile_layers(10, false));
        assert_eq!(pile_layers(5000, false), 16);
        assert_eq!(pile_layers(5000, true), 9);
        // Die Unregelmäßigkeit bleibt im Rahmen und ist für jede Lage fest.
        for layer in 1..40 {
            let value = jitter(layer, 1);
            assert!((-1.0..=1.0).contains(&value));
            assert_eq!(value, jitter(layer, 1));
        }
    }
}
