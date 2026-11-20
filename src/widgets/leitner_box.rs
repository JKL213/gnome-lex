// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Der Karteikasten von oben gesehen: ein Holzkasten mit fünf Fächern, in
//! denen die Karten hochkant stehen. Je Fach zeigt eine Reihe von
//! Kartenkanten, wie voll es ist; darunter stehen Fachnummer und Kartenzahl.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

use super::index_card::paper_rgba;
use crate::cards::{BOX_COUNT, BOX_INTERVALS};

/// Höhe des Kastens und der Beschriftung darunter.
const BOX_HEIGHT: f32 = 62.0;
const LABEL_HEIGHT: f32 = 22.0;
const MAX_WIDTH: f32 = 520.0;
/// Höchstzahl gezeichneter Kartenkanten je Fach.
const MAX_EDGES: u32 = 13;

/// Zahl der Kartenkanten für `count` Karten: Wenige Karten zählen einzeln,
/// volle Fächer wachsen langsamer.
pub fn edge_count(count: u32) -> u32 {
    if count <= 5 {
        count
    } else {
        (5 + f64::from(count - 5).sqrt().ceil() as u32).min(MAX_EDGES)
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LexLeitnerBox {
        pub counts: Cell<[u32; BOX_COUNT]>,
        pub color: RefCell<String>,
        /// Fach (ab 1), das gerade aufleuchtet, und die Stärke des Leuchtens.
        pub flash_box: Cell<u8>,
        pub flash: Cell<f64>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LexLeitnerBox {
        const NAME: &'static str = "LexLeitnerBox";
        type Type = super::LexLeitnerBox;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("leitnerbox");
        }
    }

    impl ObjectImpl for LexLeitnerBox {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.color.replace("white".into());
            obj.set_has_tooltip(true);
            obj.connect_query_tooltip(|obj, x, _, _, tooltip| {
                let Some(index) = obj.compartment_at(x as f32) else {
                    return false;
                };
                let count = obj.imp().counts.get()[index];
                let days = BOX_INTERVALS[index];
                let cards =
                    ngettext("{n} Karte", "{n} Karten", count).replace("{n}", &count.to_string());
                let interval = if days == 1 {
                    gettext("Wiederholung täglich")
                } else {
                    gettext("Wiederholung alle {n} Tage").replace("{n}", &days.to_string())
                };
                tooltip.set_text(Some(&format!(
                    "{} · {cards}\n{interval}",
                    gettext("Fach {n}").replace("{n}", &(index + 1).to_string())
                )));
                true
            });
        }

        fn dispose(&self) {
            if let Some(animation) = self.animation.take() {
                animation.reset();
            }
        }
    }

    impl WidgetImpl for LexLeitnerBox {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => (240, MAX_WIDTH as i32, -1, -1),
                _ => {
                    let height = (BOX_HEIGHT + LABEL_HEIGHT + 10.0) as i32;
                    (height, height, -1, -1)
                }
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct LexLeitnerBox(ObjectSubclass<imp::LexLeitnerBox>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LexLeitnerBox {
    fn default() -> Self {
        Self::new()
    }
}

impl LexLeitnerBox {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Kartenzahl je Fach.
    pub fn set_counts(&self, counts: [u32; BOX_COUNT]) {
        self.imp().counts.set(counts);
        self.queue_draw();
    }

    /// Papierfarbe der Karten im Kasten.
    pub fn set_paper(&self, color: &str) {
        self.imp().color.replace(color.to_owned());
        self.queue_draw();
    }

    /// Lässt ein Fach (ab 1) kurz aufleuchten, in das eine Karte gewandert ist.
    pub fn flash(&self, box_no: u8) {
        let imp = self.imp();
        if let Some(old) = imp.animation.take() {
            old.reset();
        }
        imp.flash_box.set(box_no);
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |value| {
                widget.imp().flash.set(value);
                widget.queue_draw();
            }
        ));
        let animation = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(1.0)
            .value_to(0.0)
            .duration(900)
            .easing(adw::Easing::EaseOutCubic)
            .target(&target)
            .build();
        imp.animation.replace(Some(animation.clone()));
        animation.play();
    }

    /// Lage des Kastens: x, Breite.
    fn box_span(&self) -> (f32, f32) {
        let width = self.width() as f32;
        let w = width.min(MAX_WIDTH);
        (((width - w) / 2.0).floor(), w)
    }

    /// Fach unter der waagerechten Position `x`.
    fn compartment_at(&self, x: f32) -> Option<usize> {
        let (start, w) = self.box_span();
        if x < start || x >= start + w {
            return None;
        }
        Some((((x - start) / w * BOX_COUNT as f32) as usize).min(BOX_COUNT - 1))
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let (x, w) = self.box_span();
        let y = 2.0;
        let wall = 7.0;
        let divider = 5.0;
        let outer = graphene::Rect::new(x, y, w, BOX_HEIGHT);
        let outline = gsk::RoundedRect::from_rect(outer, 7.0);
        // Schatten und Korpus aus hellem Holz.
        snapshot.append_outset_shadow(
            &outline,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.22),
            0.0,
            2.0,
            0.0,
            6.0,
        );
        snapshot.push_rounded_clip(&outline);
        snapshot.append_linear_gradient(
            &outer,
            &graphene::Point::new(0.0, y),
            &graphene::Point::new(0.0, y + BOX_HEIGHT),
            &[
                gsk::ColorStop::new(0.0, gdk::RGBA::new(0.84, 0.70, 0.50, 1.0)),
                gsk::ColorStop::new(1.0, gdk::RGBA::new(0.72, 0.57, 0.38, 1.0)),
            ],
        );
        // Innenraum: dunkler, weil er im Schatten der Wände liegt.
        let inner =
            graphene::Rect::new(x + wall, y + wall, w - 2.0 * wall, BOX_HEIGHT - 2.0 * wall);
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(inner, 2.5));
        snapshot.append_linear_gradient(
            &inner,
            &graphene::Point::new(0.0, inner.y()),
            &graphene::Point::new(0.0, inner.y() + inner.height()),
            &[
                gsk::ColorStop::new(0.0, gdk::RGBA::new(0.36, 0.27, 0.17, 1.0)),
                gsk::ColorStop::new(0.35, gdk::RGBA::new(0.47, 0.36, 0.23, 1.0)),
                gsk::ColorStop::new(1.0, gdk::RGBA::new(0.52, 0.40, 0.26, 1.0)),
            ],
        );
        let slot_w = (inner.width() - divider * (BOX_COUNT as f32 - 1.0)) / BOX_COUNT as f32;
        let counts = imp.counts.get();
        let color = imp.color.borrow();
        for (index, count) in counts.iter().enumerate() {
            let slot_x = inner.x() + index as f32 * (slot_w + divider);
            // Trennwand zum nächsten Fach.
            if index + 1 < BOX_COUNT {
                snapshot.append_color(
                    &gdk::RGBA::new(0.80, 0.66, 0.46, 1.0),
                    &graphene::Rect::new(slot_x + slot_w, inner.y(), divider, inner.height()),
                );
            }
            // Kartenkanten von hinten nach vorn.
            let edges = edge_count(*count);
            let gap = 3.4;
            let card_w = slot_w - 10.0;
            let first = inner.y() + inner.height() - 5.0 - edges as f32 * gap;
            for edge in 0..edges {
                let edge_y = first + edge as f32 * gap + gap;
                let shift = ((edge * 7 + index as u32 * 3) % 5) as f32 * 0.5 - 1.0;
                let rect = graphene::Rect::new(slot_x + 5.0 + shift, edge_y, card_w, 2.0);
                snapshot.append_color(
                    &gdk::RGBA::new(0.0, 0.0, 0.0, 0.28),
                    &rect.offset_r(0.0, 1.0),
                );
                snapshot.append_color(&paper_rgba(&color, 0.02), &rect);
            }
            // Aufleuchten des Fachs, in das gerade eine Karte gewandert ist.
            if usize::from(imp.flash_box.get()) == index + 1 && imp.flash.get() > 0.01 {
                let accent = adw::StyleManager::default().accent_color_rgba();
                let glow = gdk::RGBA::new(
                    accent.red(),
                    accent.green(),
                    accent.blue(),
                    0.42 * imp.flash.get() as f32,
                );
                snapshot.append_color(
                    &glow,
                    &graphene::Rect::new(slot_x, inner.y(), slot_w, inner.height()),
                );
            }
        }
        snapshot.pop();
        snapshot.pop();
        snapshot.append_border(
            &outline,
            &[1.0; 4],
            &[gdk::RGBA::new(0.0, 0.0, 0.0, 0.25); 4],
        );
        // Beschriftung: Fachnummer und Kartenzahl in der Schriftfarbe der
        // Oberfläche.
        let fg = self.color();
        let dim = gdk::RGBA::new(fg.red(), fg.green(), fg.blue(), fg.alpha() * 0.6);
        for (index, count) in counts.iter().enumerate() {
            let slot_x = x + wall + index as f32 * (slot_w + divider);
            let layout = self.create_pango_layout(None);
            layout.set_markup(&format!(
                "<span size=\"small\">{} <b>{count}</b></span>",
                glib::markup_escape_text(
                    &gettext("Fach {n}").replace("{n}", &(index + 1).to_string())
                )
            ));
            layout.set_width((slot_w * pango::SCALE as f32) as i32);
            layout.set_alignment(pango::Alignment::Center);
            layout.set_ellipsize(pango::EllipsizeMode::End);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(slot_x, y + BOX_HEIGHT + 6.0));
            snapshot.append_layout(&layout, if *count > 0 { &fg } else { &dim });
            snapshot.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_reflect_the_fill_level() {
        assert_eq!(edge_count(0), 0);
        assert_eq!(edge_count(4), 4);
        assert!(edge_count(40) > edge_count(12));
        assert_eq!(edge_count(10_000), MAX_EDGES);
    }
}
