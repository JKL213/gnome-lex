//! GObject-Datenobjekte für Listen (werden in den folgenden Stufen ausgebaut).

use gtk::glib;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::OutlineItem)]
    pub struct OutlineItem {
        #[property(get, set)]
        pub id: Cell<i64>,
        #[property(get, set)]
        pub is_unit: Cell<bool>,
        #[property(get, set)]
        pub label: RefCell<String>,
        #[property(get, set)]
        pub title: RefCell<String>,
        #[property(get, set)]
        pub depth: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OutlineItem {
        const NAME: &'static str = "LexOutlineItem";
        type Type = super::OutlineItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for OutlineItem {}

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::SearchResultObject)]
    pub struct SearchResultObject {
        #[property(get, set)]
        pub norm_id: Cell<i64>,
        #[property(get, set)]
        pub title: RefCell<String>,
        #[property(get, set)]
        pub snippet: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchResultObject {
        const NAME: &'static str = "LexSearchResultObject";
        type Type = super::SearchResultObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SearchResultObject {}

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::AnnotationObject)]
    pub struct AnnotationObject {
        #[property(get, set)]
        pub id: Cell<i64>,
        #[property(get, set)]
        pub kind: RefCell<String>,
        #[property(get, set)]
        pub norm: RefCell<String>,
        #[property(get, set)]
        pub quote: RefCell<String>,
        #[property(get, set)]
        pub note: RefCell<String>,
        #[property(get, set)]
        pub color: RefCell<String>,
        #[property(get, set)]
        pub orphaned: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AnnotationObject {
        const NAME: &'static str = "LexAnnotationObject";
        type Type = super::AnnotationObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for AnnotationObject {}
}

glib::wrapper! {
    pub struct OutlineItem(ObjectSubclass<imp::OutlineItem>);
}

impl OutlineItem {
    pub fn new(id: i64, is_unit: bool, label: &str, title: &str, depth: u32) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("is-unit", is_unit)
            .property("label", label)
            .property("title", title)
            .property("depth", depth)
            .build()
    }
}

glib::wrapper! {
    pub struct SearchResultObject(ObjectSubclass<imp::SearchResultObject>);
}

impl SearchResultObject {
    pub fn new(norm_id: i64, title: &str, snippet: &str) -> Self {
        glib::Object::builder()
            .property("norm-id", norm_id)
            .property("title", title)
            .property("snippet", snippet)
            .build()
    }
}

glib::wrapper! {
    pub struct AnnotationObject(ObjectSubclass<imp::AnnotationObject>);
}

impl AnnotationObject {
    pub fn from_annotation(a: &crate::model::Annotation) -> Self {
        glib::Object::builder()
            .property("id", a.id)
            .property("kind", a.kind.as_str())
            .property("norm", a.norm.as_str())
            .property("quote", a.quote.as_str())
            .property("note", a.note.clone().unwrap_or_default())
            .property("color", a.color.clone().unwrap_or_default())
            .property("orphaned", a.orphaned)
            .build()
    }
}
