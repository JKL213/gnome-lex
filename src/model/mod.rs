// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Datenmodell: das Block-Modell des Normtexts (serialisierbar, ohne GTK)
//! sowie GObject-Subklassen für Listen und Bäume in der Oberfläche.
// Wird ab den folgenden Stufen von der Oberfläche genutzt.
#![allow(dead_code)]

pub mod objects;
pub mod text;

#[allow(unused_imports)]
pub use objects::{AnnotationObject, OutlineItem, SearchResultObject};
#[allow(unused_imports)]
pub use text::{Block, Cell, Flattened, Footnote, ListItem, Seg, SegTag, Span, Style, TableData};

/// Metadaten eines Gesetzes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LawInfo {
    pub id: i64,
    pub slug: String,
    pub jurabk: String,
    pub amtabk: Option<String>,
    pub title: String,
    pub doknr: String,
    pub builddate: String,
    pub stand: Option<String>,
    pub neufassung: Option<String>,
    pub ausfertigung_datum: Option<String>,
    pub fundstelle: Option<String>,
    pub imported_at: String,
}

/// Eine Gliederungseinheit (Buch, Abschnitt, Titel, …).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitInfo {
    pub id: i64,
    pub law_id: i64,
    pub parent_id: Option<i64>,
    pub kennzahl: String,
    pub bez: String,
    pub titel: Option<String>,
    pub depth: i64,
    pub position: i64,
}

/// Kurzinformation zu einer Norm für Listen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormInfo {
    pub id: i64,
    pub law_id: i64,
    pub unit_id: Option<i64>,
    pub doknr: String,
    pub enbez: Option<String>,
    pub titel: Option<String>,
    pub position: i64,
}

impl NormInfo {
    /// Anzeigename, z. B. „§ 433 Vertragstypische Pflichten beim Kaufvertrag“.
    pub fn display_name(&self) -> String {
        match (&self.enbez, &self.titel) {
            (Some(e), Some(t)) => format!("{e} {t}"),
            (Some(e), None) => e.clone(),
            (None, Some(t)) => t.clone(),
            (None, None) => self.doknr.clone(),
        }
    }
}

/// Vollständige Norm mit Inhalt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Norm {
    pub info: NormInfo,
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub fussnoten: Vec<Block>,
}

/// Art einer Annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Highlight,
    Note,
    Bookmark,
    Link,
}

impl AnnotationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
            Self::Bookmark => "bookmark",
            Self::Link => "link",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "highlight" => Some(Self::Highlight),
            "note" => Some(Self::Note),
            "bookmark" => Some(Self::Bookmark),
            "link" => Some(Self::Link),
            _ => None,
        }
    }
}

/// Zielangabe eines manuellen Verweises.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkTarget {
    pub law: String,
    pub norm: String,
    #[serde(default)]
    pub paragraph: i64,
    #[serde(default)]
    pub offset: i64,
}

/// Eine Annotation (Markierung, Notiz, Lesezeichen oder manueller Verweis),
/// verankert über Gesetz, Norm-Bezeichnung, Absatz, Zeichenoffsets und Wortlaut.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    #[serde(default)]
    pub id: i64,
    pub law: String,
    pub norm: String,
    #[serde(default)]
    pub paragraph: i64,
    #[serde(default)]
    pub start: i64,
    #[serde(default)]
    pub end: i64,
    #[serde(default)]
    pub quote: String,
    pub kind: AnnotationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<LinkTarget>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub modified: String,
    #[serde(default)]
    pub orphaned: bool,
}

impl Annotation {
    pub fn now() -> String {
        chrono::Local::now().to_rfc3339()
    }
}

/// Verfügbare Markierungsfarben (Name, Anzeigename, RGBA hell, RGBA dunkel).
pub const HIGHLIGHT_COLORS: &[(&str, &str, &str, &str)] = &[
    ("yellow", "Gelb", "#f6d32d80", "#e5a50a80"),
    ("green", "Grün", "#8ff0a480", "#26a26980"),
    ("blue", "Blau", "#99c1f180", "#1c71d880"),
    ("pink", "Rosa", "#f4a6c780", "#c061cb80"),
    ("orange", "Orange", "#ffbe6f80", "#e6610080"),
];
