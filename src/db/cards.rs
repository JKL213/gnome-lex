// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Karteikarten in der Datenbank: Stapel, Karten und ihr Lernstand.

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension};

use super::{Database, Result};
use crate::cards::anki::AnkiCard;
use crate::cards::{date_key, Card, DeckInfo, RatingOutcome, BOX_COUNT, PAPER_COLORS};

pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS card_decks (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL,
    color TEXT NOT NULL,
    imported_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cards (
    id INTEGER PRIMARY KEY,
    deck_id INTEGER NOT NULL REFERENCES card_decks(id) ON DELETE CASCADE,
    anki_id INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '',
    box INTEGER NOT NULL DEFAULT 1,
    due TEXT NOT NULL,
    reviews INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    last_review TEXT
);
CREATE INDEX IF NOT EXISTS cards_deck ON cards(deck_id, position);
CREATE INDEX IF NOT EXISTS cards_due ON cards(deck_id, due);
"#;

/// Ergebnis des Imports eines Stapels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckImport {
    pub deck_id: i64,
    /// Neu angelegte Karten.
    pub added: usize,
    /// Bereits vorhandene Karten, deren Text aktualisiert wurde (der
    /// Lernstand bleibt erhalten).
    pub updated: usize,
    /// Karten, die das Paket nicht mehr enthält.
    pub removed: usize,
}

const CARD_COLUMNS: &str = "id, deck_id, front, back, tags, box, due, reviews, lapses";

fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<Card> {
    Ok(Card {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        front: row.get(2)?,
        back: row.get(3)?,
        tags: row.get(4)?,
        box_no: row.get(5)?,
        due: row.get(6)?,
        reviews: row.get(7)?,
        lapses: row.get(8)?,
    })
}

impl Database {
    /// Legt einen Stapel an oder aktualisiert den gleichnamigen: Karten
    /// werden an ihrer Paket-ID wiedererkannt und behalten ihren Lernstand,
    /// neue Karten beginnen im ersten Fach und sind sofort fällig.
    pub fn import_deck(
        &mut self,
        name: &str,
        source: &str,
        cards: &[AnkiCard],
        today: NaiveDate,
    ) -> Result<DeckImport> {
        let tx = self.conn.transaction()?;
        let now = chrono::Local::now().to_rfc3339();
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM card_decks WHERE name = ?1", [name], |r| {
                r.get(0)
            })
            .optional()?;
        let deck_id = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE card_decks SET source = ?2, imported_at = ?3 WHERE id = ?1",
                    params![id, source, now],
                )?;
                id
            }
            None => {
                // Farben reihum, damit benachbarte Stapel sich unterscheiden.
                let count: i64 =
                    tx.query_row("SELECT COUNT(*) FROM card_decks", [], |r| r.get(0))?;
                let color = PAPER_COLORS[count as usize % PAPER_COLORS.len()];
                tx.execute(
                    "INSERT INTO card_decks (name, source, color, imported_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![name, source, color, now],
                )?;
                tx.last_insert_rowid()
            }
        };
        let mut report = DeckImport {
            deck_id,
            ..Default::default()
        };
        let due = date_key(today);
        {
            // Erst alle Karten als „nicht mehr im Paket“ vormerken …
            tx.execute(
                "UPDATE cards SET position = -1 WHERE deck_id = ?1",
                [deck_id],
            )?;
            let mut update = tx.prepare(
                "UPDATE cards SET position = ?3, front = ?4, back = ?5, tags = ?6
                 WHERE deck_id = ?1 AND anki_id = ?2 AND position = -1",
            )?;
            let mut insert = tx.prepare(
                "INSERT INTO cards (deck_id, anki_id, position, front, back, tags, box, due)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
            )?;
            for (position, card) in cards.iter().enumerate() {
                let position = position as i64;
                let changed = update.execute(params![
                    deck_id,
                    card.anki_id,
                    position,
                    card.front,
                    card.back,
                    card.tags
                ])?;
                if changed > 0 {
                    report.updated += 1;
                } else {
                    insert.execute(params![
                        deck_id,
                        card.anki_id,
                        position,
                        card.front,
                        card.back,
                        card.tags,
                        due
                    ])?;
                    report.added += 1;
                }
            }
        }
        // … was dann noch vorgemerkt ist, gibt es im Paket nicht mehr.
        report.removed = tx.execute(
            "DELETE FROM cards WHERE deck_id = ?1 AND position = -1",
            [deck_id],
        )?;
        tx.commit()?;
        Ok(report)
    }

    /// Alle Stapel mit Kartenzahl, Fälligkeit und Verteilung auf die Fächer.
    pub fn card_decks(&self, today: NaiveDate) -> Result<Vec<DeckInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, color, imported_at FROM card_decks
             ORDER BY name COLLATE NOCASE",
        )?;
        let mut decks: Vec<DeckInfo> = stmt
            .query_map([], |row| {
                Ok(DeckInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source: row.get(2)?,
                    color: row.get(3)?,
                    imported_at: row.get(4)?,
                    total: 0,
                    due: 0,
                    boxes: [0; BOX_COUNT],
                })
            })?
            .collect::<Result<_>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT deck_id, box, COUNT(*), SUM(due <= ?1) FROM cards GROUP BY deck_id, box",
        )?;
        let mut rows = stmt.query([date_key(today)])?;
        while let Some(row) = rows.next()? {
            let deck_id: i64 = row.get(0)?;
            let box_no: i64 = row.get(1)?;
            let count: u32 = row.get(2)?;
            let due: u32 = row.get(3)?;
            if let Some(deck) = decks.iter_mut().find(|d| d.id == deck_id) {
                let index = (box_no.clamp(1, BOX_COUNT as i64) - 1) as usize;
                deck.boxes[index] += count;
                deck.total += count;
                deck.due += due;
            }
        }
        Ok(decks)
    }

    #[cfg(test)]
    pub fn card_deck(&self, id: i64, today: NaiveDate) -> Result<Option<DeckInfo>> {
        Ok(self.card_decks(today)?.into_iter().find(|d| d.id == id))
    }

    /// Heute fällige Karten eines Stapels: niedrige Fächer zuerst, innerhalb
    /// eines Fachs in der Reihenfolge des Stapels.
    pub fn due_cards(&self, deck_id: i64, today: NaiveDate) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CARD_COLUMNS} FROM cards WHERE deck_id = ?1 AND due <= ?2
             ORDER BY box, position"
        ))?;
        let rows = stmt.query_map(params![deck_id, date_key(today)], row_to_card)?;
        rows.collect()
    }

    /// Alle Karten eines Stapels in ihrer Reihenfolge.
    pub fn deck_cards(&self, deck_id: i64) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CARD_COLUMNS} FROM cards WHERE deck_id = ?1 ORDER BY position"
        ))?;
        let rows = stmt.query_map([deck_id], row_to_card)?;
        rows.collect()
    }

    /// Schreibt das Ergebnis einer Bewertung: neues Fach und Fälligkeit.
    pub fn apply_rating(&self, outcome: &RatingOutcome, today: NaiveDate) -> Result<()> {
        let lapse = i64::from(outcome.rating == crate::cards::Rating::Unknown);
        self.conn.execute(
            "UPDATE cards SET box = ?2, due = ?3, reviews = reviews + 1,
                    lapses = lapses + ?4, last_review = ?5
             WHERE id = ?1",
            params![
                outcome.card_id,
                outcome.new_box,
                date_key(outcome.due),
                lapse,
                date_key(today)
            ],
        )?;
        Ok(())
    }

    /// Legt alle Karten eines Stapels zurück ins erste Fach.
    pub fn reset_deck(&self, deck_id: i64, today: NaiveDate) -> Result<()> {
        self.conn.execute(
            "UPDATE cards SET box = 1, due = ?2, reviews = 0, lapses = 0, last_review = NULL
             WHERE deck_id = ?1",
            params![deck_id, date_key(today)],
        )?;
        Ok(())
    }

    pub fn set_deck_color(&self, deck_id: i64, color: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE card_decks SET color = ?2 WHERE id = ?1",
            params![deck_id, color],
        )?;
        Ok(())
    }

    /// Löscht einen Stapel samt Karten.
    pub fn delete_deck(&self, deck_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM card_decks WHERE id = ?1", [deck_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{schedule, Rating};

    fn anki(id: i64, front: &str) -> AnkiCard {
        AnkiCard {
            anki_id: id,
            front: front.into(),
            back: format!("Antwort auf {front}"),
            tags: String::new(),
        }
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, d).expect("Datum")
    }

    #[test]
    fn import_counts_and_due_cards() {
        let mut db = Database::open_in_memory().expect("Datenbank");
        let cards = [anki(1, "A"), anki(2, "B"), anki(3, "C")];
        let report = db
            .import_deck("BGB AT::Anfechtung", "bgb.apkg", &cards, day(1))
            .expect("Import");
        assert_eq!((report.added, report.updated, report.removed), (3, 0, 0));
        let decks = db.card_decks(day(1)).expect("Stapel");
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].title(), "Anfechtung");
        assert_eq!((decks[0].total, decks[0].due), (3, 3));
        assert_eq!(decks[0].boxes, [3, 0, 0, 0, 0]);
        assert_eq!(decks[0].color, "white");
        let due = db.due_cards(report.deck_id, day(1)).expect("fällig");
        assert_eq!(
            due.iter().map(|c| c.front.as_str()).collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
    }

    #[test]
    fn rating_moves_cards_between_boxes() {
        let mut db = Database::open_in_memory().expect("Datenbank");
        let deck = db
            .import_deck("Stapel", "s.apkg", &[anki(1, "A"), anki(2, "B")], day(1))
            .expect("Import")
            .deck_id;
        let cards = db.due_cards(deck, day(1)).expect("fällig");
        let rate = |db: &Database, card: &Card, rating, today| {
            let (new_box, due) = schedule(card.box_no, rating, today);
            let outcome = RatingOutcome {
                card_id: card.id,
                rating,
                old_box: card.box_no,
                new_box,
                due,
            };
            db.apply_rating(&outcome, today).expect("Bewertung");
        };
        rate(&db, &cards[0], Rating::Known, day(1));
        rate(&db, &cards[1], Rating::Unknown, day(1));
        let info = db
            .card_deck(deck, day(1))
            .expect("Stapel")
            .expect("vorhanden");
        assert_eq!(info.boxes, [1, 1, 0, 0, 0]);
        // Die gewusste Karte ist erst in zwei Tagen wieder fällig.
        assert_eq!(info.due, 1);
        assert_eq!(
            db.card_deck(deck, day(3)).expect("Stapel").expect("da").due,
            2
        );
        let all = db.deck_cards(deck).expect("Karten");
        assert_eq!((all[0].box_no, all[0].reviews, all[0].lapses), (2, 1, 0));
        assert_eq!((all[1].box_no, all[1].reviews, all[1].lapses), (1, 1, 1));
        db.reset_deck(deck, day(4)).expect("Zurücksetzen");
        let info = db.card_deck(deck, day(4)).expect("Stapel").expect("da");
        assert_eq!((info.boxes, info.due), ([2, 0, 0, 0, 0], 2));
    }

    #[test]
    fn reimport_keeps_progress_and_drops_missing_cards() {
        let mut db = Database::open_in_memory().expect("Datenbank");
        let first = [anki(1, "A"), anki(2, "B"), anki(3, "C")];
        let deck = db
            .import_deck("Stapel", "s.apkg", &first, day(1))
            .expect("Import")
            .deck_id;
        let card = db.deck_cards(deck).expect("Karten").remove(0);
        let (new_box, due) = schedule(card.box_no, Rating::Known, day(1));
        db.apply_rating(
            &RatingOutcome {
                card_id: card.id,
                rating: Rating::Known,
                old_box: 1,
                new_box,
                due,
            },
            day(1),
        )
        .expect("Bewertung");
        // Neues Paket: Karte 1 mit geändertem Text, Karte 2 fehlt, Karte 4 neu.
        let second = [anki(4, "D"), anki(1, "A neu"), anki(3, "C")];
        let report = db
            .import_deck("Stapel", "s2.apkg", &second, day(2))
            .expect("Import");
        assert_eq!(report.deck_id, deck);
        assert_eq!((report.added, report.updated, report.removed), (1, 2, 1));
        let cards = db.deck_cards(deck).expect("Karten");
        assert_eq!(
            cards.iter().map(|c| c.front.as_str()).collect::<Vec<_>>(),
            ["D", "A neu", "C"]
        );
        assert_eq!(cards[1].box_no, 2);
        assert_eq!(db.card_decks(day(2)).expect("Stapel")[0].source, "s2.apkg");
    }

    #[test]
    fn decks_get_alternating_colors_and_can_be_deleted() {
        let mut db = Database::open_in_memory().expect("Datenbank");
        let a = db
            .import_deck("A", "a.apkg", &[anki(1, "A")], day(1))
            .expect("A");
        let b = db
            .import_deck("B", "a.apkg", &[anki(2, "B")], day(1))
            .expect("B");
        let decks = db.card_decks(day(1)).expect("Stapel");
        assert_eq!(
            (decks[0].color.as_str(), decks[1].color.as_str()),
            ("white", "yellow")
        );
        db.set_deck_color(a.deck_id, "pink").expect("Farbe");
        db.delete_deck(b.deck_id).expect("Löschen");
        let decks = db.card_decks(day(1)).expect("Stapel");
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].color, "pink");
        assert!(db.deck_cards(b.deck_id).expect("Karten").is_empty());
    }
}
