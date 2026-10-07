// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Gliederung eines Gesetzes als Baum (`GtkTreeListModel` über
//! [`OutlineItem`]): Gliederungseinheiten sind aufklappbare Knoten, Normen
//! die Blätter darunter. Normen ohne Einheit (Rahmennorm, Inhaltsübersicht)
//! stehen auf der obersten Ebene.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::model::{NormInfo, OutlineItem, UnitInfo};

/// Kindobjekte je Gliederungseinheit (zuerst direkte Normen, dann Untereinheiten).
type Children = HashMap<i64, Vec<OutlineItem>>;

/// Baum-Modell der Gliederung samt Auswahl.
pub struct Outline {
    root: gio::ListStore,
    tree: gtk::TreeListModel,
    selection: gtk::SingleSelection,
    children: Rc<RefCell<Children>>,
    /// Einheit → übergeordnete Einheit.
    unit_parent: RefCell<HashMap<i64, Option<i64>>>,
    /// Norm → umgebende Einheit.
    norm_unit: RefCell<HashMap<i64, Option<i64>>>,
}

impl Default for Outline {
    fn default() -> Self {
        Self::new()
    }
}

impl Outline {
    pub fn new() -> Self {
        let root = gio::ListStore::new::<OutlineItem>();
        let children: Rc<RefCell<Children>> = Rc::new(RefCell::new(HashMap::new()));
        let tree = gtk::TreeListModel::new(
            root.clone(),
            false,
            false,
            glib::clone!(
                #[strong]
                children,
                move |obj| {
                    let item = obj.downcast_ref::<OutlineItem>()?;
                    if !item.is_unit() {
                        return None;
                    }
                    let map = children.borrow();
                    let kids = map.get(&item.id())?;
                    if kids.is_empty() {
                        return None;
                    }
                    let store = gio::ListStore::new::<OutlineItem>();
                    for k in kids {
                        store.append(k);
                    }
                    Some(store.upcast())
                }
            ),
        );
        let selection = gtk::SingleSelection::builder()
            .model(&tree)
            .autoselect(false)
            .can_unselect(true)
            .build();
        Self {
            root,
            tree,
            selection,
            children,
            unit_parent: RefCell::new(HashMap::new()),
            norm_unit: RefCell::new(HashMap::new()),
        }
    }

    pub fn selection(&self) -> &gtk::SingleSelection {
        &self.selection
    }

    /// Ersetzt den Inhalt des Baums.
    pub fn set_data(&self, units: &[UnitInfo], norms: &[NormInfo]) {
        let mut children: Children = HashMap::new();
        let mut root_items: Vec<OutlineItem> = Vec::new();
        let mut norm_unit = HashMap::with_capacity(norms.len());
        let mut unit_parent = HashMap::with_capacity(units.len());

        for n in norms {
            let (label, title) = norm_labels(n);
            let item = OutlineItem::new(n.id, false, &label, &title, 0);
            norm_unit.insert(n.id, n.unit_id);
            match n.unit_id {
                Some(u) => children.entry(u).or_default().push(item),
                None => root_items.push(item),
            }
        }
        for u in units {
            let item = OutlineItem::new(
                u.id,
                true,
                &u.bez,
                u.titel.as_deref().unwrap_or(""),
                u.depth as u32,
            );
            unit_parent.insert(u.id, u.parent_id);
            match u.parent_id {
                Some(p) => children.entry(p).or_default().push(item),
                None => root_items.push(item),
            }
        }

        *self.children.borrow_mut() = children;
        *self.unit_parent.borrow_mut() = unit_parent;
        *self.norm_unit.borrow_mut() = norm_unit;
        self.selection.set_selected(gtk::INVALID_LIST_POSITION);
        self.root.remove_all();
        self.root.splice(0, 0, &root_items);
    }

    /// Klappt alle Einheiten über der Norm auf und liefert die Position der
    /// Norm im flachen Modell.
    pub fn reveal_norm(&self, norm_id: i64) -> Option<u32> {
        let path = {
            let norm_unit = self.norm_unit.borrow();
            let unit_parent = self.unit_parent.borrow();
            unit_path(&unit_parent, *norm_unit.get(&norm_id)?)
        };
        let mut row: Option<gtk::TreeListRow> = None;
        for unit_id in path {
            let model = self.model_below(row.as_ref())?;
            let index = find_index(&model, unit_id, true)?;
            let child = match &row {
                None => self.tree.child_row(index)?,
                Some(r) => r.child_row(index)?,
            };
            row = Some(child);
        }
        let model = self.model_below(row.as_ref())?;
        let index = find_index(&model, norm_id, false)?;
        let leaf = match &row {
            None => self.tree.child_row(index)?,
            Some(r) => r.child_row(index)?,
        };
        Some(leaf.position())
    }

    /// Kindmodell einer Zeile (klappt sie dafür auf) bzw. das Wurzelmodell.
    fn model_below(&self, row: Option<&gtk::TreeListRow>) -> Option<gio::ListModel> {
        match row {
            None => Some(self.root.clone().upcast()),
            Some(r) => {
                r.set_expanded(true);
                r.children()
            }
        }
    }

    /// Das Objekt an einer Position des flachen Modells.
    pub fn item_at(&self, position: u32) -> Option<(gtk::TreeListRow, OutlineItem)> {
        let row = self.tree.row(position)?;
        let item = row.item().and_downcast::<OutlineItem>()?;
        Some((row, item))
    }
}

/// Bezeichnung und Titel einer Norm für die Gliederung.
fn norm_labels(n: &NormInfo) -> (String, String) {
    match (&n.enbez, &n.titel) {
        (Some(e), Some(t)) => (e.clone(), t.clone()),
        (Some(e), None) => (e.clone(), String::new()),
        (None, Some(t)) => (t.clone(), String::new()),
        (None, None) => (n.doknr.clone(), String::new()),
    }
}

/// Einheiten von der obersten Ebene bis zur umgebenden Einheit einer Norm.
fn unit_path(unit_parent: &HashMap<i64, Option<i64>>, unit: Option<i64>) -> Vec<i64> {
    let mut path = Vec::new();
    let mut current = unit;
    while let Some(id) = current {
        if path.contains(&id) {
            break; // Schutz vor zyklischen Daten
        }
        path.push(id);
        current = unit_parent.get(&id).copied().flatten();
    }
    path.reverse();
    path
}

/// Index eines Objekts mit passender ID in einem Listenmodell.
fn find_index(model: &gio::ListModel, id: i64, is_unit: bool) -> Option<u32> {
    (0..model.n_items()).find(|&i| {
        model
            .item(i)
            .and_downcast::<OutlineItem>()
            .is_some_and(|item| item.id() == id && item.is_unit() == is_unit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_path_walks_to_root() {
        let mut parents = HashMap::new();
        parents.insert(1, None);
        parents.insert(2, Some(1));
        parents.insert(3, Some(2));
        assert_eq!(unit_path(&parents, Some(3)), vec![1, 2, 3]);
        assert_eq!(unit_path(&parents, Some(1)), vec![1]);
        assert!(unit_path(&parents, None).is_empty());
    }

    #[test]
    fn unit_path_survives_cycles() {
        let mut parents = HashMap::new();
        parents.insert(1, Some(2));
        parents.insert(2, Some(1));
        assert_eq!(unit_path(&parents, Some(1)), vec![2, 1]);
    }

    #[test]
    fn norm_labels_fall_back_to_title_and_doknr() {
        let n = NormInfo {
            enbez: Some("§ 1".into()),
            titel: Some("Beginn".into()),
            ..Default::default()
        };
        assert_eq!(norm_labels(&n), ("§ 1".into(), "Beginn".into()));
        let n = NormInfo {
            titel: Some("Inhaltsübersicht".into()),
            ..Default::default()
        };
        assert_eq!(norm_labels(&n), ("Inhaltsübersicht".into(), String::new()));
        let n = NormInfo {
            doknr: "BJNR".into(),
            ..Default::default()
        };
        assert_eq!(norm_labels(&n).0, "BJNR");
    }
}
