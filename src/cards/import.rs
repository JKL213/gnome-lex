// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Import eines Kartenpakets in die Datenbank samt Bildern. Die Funktionen
//! blockieren und laufen deshalb nie im Hauptthread.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use gettextrs::gettext;

use super::anki::{self, AnkiError};
use crate::db::Database;

/// Zusammenfassung eines Imports über alle Stapel des Pakets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub decks: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

/// Ordner, unter dem die Bilder aller Stapel liegen.
pub fn media_root() -> PathBuf {
    glib::user_data_dir().join("gesetze").join("karten-medien")
}

/// Ordner mit den Bildern eines Stapels.
pub fn media_dir(root: &Path, deck_id: i64) -> PathBuf {
    root.join(deck_id.to_string())
}

/// Liest ein Paket und legt seine Stapel an bzw. aktualisiert sie; die von
/// den Karten eingebundenen Bilder werden in den Medienordner entpackt.
pub fn import_package(
    db_path: &Path,
    package: &Path,
    media_root: &Path,
    today: NaiveDate,
) -> Result<ImportSummary, AnkiError> {
    let decks = anki::read_package(package)?;
    if decks.is_empty() {
        return Err(AnkiError::Format(gettext(
            "Das Paket enthält keine Karten.",
        )));
    }
    let source = package
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut db = Database::open(db_path)?;
    let mut summary = ImportSummary::default();
    for deck in &decks {
        let report = db.import_deck(&deck.name, &source, &deck.cards, today)?;
        summary.decks += 1;
        summary.added += report.added;
        summary.updated += report.updated;
        summary.removed += report.removed;
        let names = deck.media_names();
        if let Err(err) =
            anki::extract_media(package, &names, &media_dir(media_root, report.deck_id))
        {
            // Fehlende Bilder verhindern den Import nicht.
            log::warn!("Bilder des Stapels {} nicht entpackt: {err}", deck.name);
        }
    }
    Ok(summary)
}

/// Löscht einen Stapel und seine Bilder.
pub fn remove_deck(db_path: &Path, media_root: &Path, deck_id: i64) -> Result<(), AnkiError> {
    Database::open(db_path)?.delete_deck(deck_id)?;
    let dir = media_dir(media_root, deck_id);
    if dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::anki::tests::{legacy_package, TestDir};

    #[test]
    fn imports_package_with_media_and_removes_it_again() {
        let dir = TestDir::new("import");
        let package = legacy_package(&dir.0);
        let db_path = dir.0.join("karten.db");
        let media = dir.0.join("medien");
        let today = NaiveDate::from_ymd_opt(2026, 4, 1).expect("Datum");
        let summary = import_package(&db_path, &package, &media, today).expect("Import");
        assert_eq!(
            summary,
            ImportSummary {
                decks: 2,
                added: 4,
                updated: 0,
                removed: 0
            }
        );
        let db = Database::open(&db_path).expect("Datenbank");
        let decks = db.card_decks(today).expect("Stapel");
        assert_eq!(decks[0].name, "Schuldrecht BT::Kaufrecht");
        assert_eq!(decks[0].source, "Zivilrecht.apkg");
        assert!(media_dir(&media, decks[0].id).join("schema.png").is_file());
        drop(db);
        // Zweiter Import desselben Pakets: nichts Neues, nichts verloren.
        let again = import_package(&db_path, &package, &media, today).expect("Import");
        assert_eq!((again.added, again.updated, again.removed), (0, 4, 0));
        remove_deck(&db_path, &media, decks[0].id).expect("Löschen");
        assert!(!media_dir(&media, decks[0].id).exists());
        let db = Database::open(&db_path).expect("Datenbank");
        assert_eq!(db.card_decks(today).expect("Stapel").len(), 1);
    }
}
