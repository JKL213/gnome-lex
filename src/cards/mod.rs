// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Karteikarten: Datenmodell und Lernkartei nach dem Fächerprinzip.
//!
//! Jede Karte liegt in einem von fünf Fächern. Eine gewusste Karte wandert
//! ein Fach weiter und wird nach dem Abstand dieses Fachs wieder fällig; eine
//! nicht gewusste Karte fällt in das erste Fach zurück und kommt in derselben
//! Sitzung erneut an die Reihe.

pub mod anki;
pub mod html;
pub mod import;
pub mod template;

use std::collections::VecDeque;

use chrono::{Duration, NaiveDate};

/// Zahl der Fächer der Lernkartei.
pub const BOX_COUNT: usize = 5;
/// Wiederholungsabstand je Fach in Tagen (Fach 1 täglich … Fach 5 monatlich).
pub const BOX_INTERVALS: [i64; BOX_COUNT] = [1, 2, 5, 10, 30];
/// Papierfarben der Stapel, wie sie als Karteikarten im Handel üblich sind.
pub const PAPER_COLORS: [&str; 5] = ["white", "yellow", "green", "blue", "pink"];
/// Trenner zwischen über- und untergeordnetem Stapel im Namen.
pub const DECK_SEPARATOR: &str = "::";

/// Ein Kartenstapel mit seinen Zählern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInfo {
    pub id: i64,
    /// Vollständiger Name, übergeordnete Stapel durch „::“ getrennt.
    pub name: String,
    /// Dateiname des Pakets, aus dem der Stapel stammt.
    pub source: String,
    /// Papierfarbe (ein Wert aus [`PAPER_COLORS`]).
    pub color: String,
    pub imported_at: String,
    pub total: u32,
    /// Heute fällige Karten.
    pub due: u32,
    /// Karten je Fach.
    pub boxes: [u32; BOX_COUNT],
}

impl DeckInfo {
    /// Eigener Titel des Stapels (letzter Bestandteil des Namens).
    pub fn title(&self) -> &str {
        deck_title(&self.name)
    }

    /// Übergeordnete Stapel als lesbarer Pfad („BGB AT › Willenserklärung“).
    pub fn parent(&self) -> Option<String> {
        deck_parent(&self.name)
    }
}

/// Letzter Bestandteil eines Stapelnamens.
pub fn deck_title(name: &str) -> &str {
    name.rsplit(DECK_SEPARATOR).next().unwrap_or(name).trim()
}

/// Übergeordnete Bestandteile eines Stapelnamens, mit „›“ verbunden.
pub fn deck_parent(name: &str) -> Option<String> {
    let (parent, _) = name.rsplit_once(DECK_SEPARATOR)?;
    let parts: Vec<&str> = parent
        .split(DECK_SEPARATOR)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" › "))
}

/// Eine Karteikarte: Vorder- und Rückseite als (bereits aus der Vorlage
/// erzeugtes) HTML sowie der Lernstand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: i64,
    pub deck_id: i64,
    pub front: String,
    pub back: String,
    pub tags: String,
    /// Fach 1 bis [`BOX_COUNT`].
    pub box_no: u8,
    /// Fälligkeit als `JJJJ-MM-TT`.
    pub due: String,
    pub reviews: u32,
    pub lapses: u32,
}

/// Selbsteinschätzung nach dem Umdrehen der Karte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Known,
    Unknown,
}

/// Neues Fach und neue Fälligkeit einer Karte nach der Bewertung.
pub fn schedule(box_no: u8, rating: Rating, today: NaiveDate) -> (u8, NaiveDate) {
    match rating {
        Rating::Unknown => (1, today),
        Rating::Known => {
            let next = (box_no.max(1) as usize + 1).min(BOX_COUNT);
            (next as u8, today + Duration::days(BOX_INTERVALS[next - 1]))
        }
    }
}

/// Heutiges Datum in Ortszeit.
pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Datum als Schlüssel für die Datenbank (`JJJJ-MM-TT`, sortierbar).
pub fn date_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Art einer Sitzung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    /// Fällige Karten abfragen und bewerten.
    Learn,
    /// Alle Karten ohne Bewertung durchblättern.
    Browse,
}

/// Ergebnis einer Bewertung, das in die Datenbank geschrieben wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatingOutcome {
    pub card_id: i64,
    pub rating: Rating,
    pub old_box: u8,
    pub new_box: u8,
    pub due: NaiveDate,
}

/// Eine Lern- oder Blättersitzung über einen Stapel. Die aktuelle Karte ist
/// die oberste des Stapels.
#[derive(Debug, Clone)]
pub struct Session {
    mode: SessionMode,
    queue: VecDeque<Card>,
    /// Bereits durchgeblätterte Karten (nur im Blättermodus, für „zurück“).
    seen: Vec<Card>,
    total: usize,
    known: usize,
    unknown: usize,
}

impl Session {
    pub fn new(mode: SessionMode, cards: Vec<Card>) -> Self {
        Self {
            mode,
            total: cards.len(),
            queue: cards.into(),
            seen: Vec::new(),
            known: 0,
            unknown: 0,
        }
    }

    pub fn mode(&self) -> SessionMode {
        self.mode
    }

    /// Oberste Karte des Stapels.
    pub fn current(&self) -> Option<&Card> {
        self.queue.front()
    }

    /// Karten, die noch auf dem Stapel liegen (einschließlich der obersten).
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    /// Zahl der Karten zu Beginn der Sitzung.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Gewusste Karten dieser Sitzung.
    pub fn known(&self) -> usize {
        self.known
    }

    /// Wie oft in dieser Sitzung „nicht gewusst“ gewählt wurde.
    pub fn unknown(&self) -> usize {
        self.unknown
    }

    /// Nummer der aktuellen Karte (ab 1) für die Fortschrittsanzeige.
    pub fn position(&self) -> usize {
        match self.mode {
            SessionMode::Learn => (self.known + 1).min(self.total.max(1)),
            SessionMode::Browse => self.seen.len() + 1,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty()
    }

    /// Bewertet die oberste Karte: Gewusste Karten verlassen den Stapel,
    /// nicht gewusste wandern ans Ende und kommen erneut an die Reihe.
    pub fn rate(&mut self, rating: Rating, today: NaiveDate) -> Option<RatingOutcome> {
        let mut card = self.queue.pop_front()?;
        let old_box = card.box_no;
        let (new_box, due) = schedule(old_box, rating, today);
        let outcome = RatingOutcome {
            card_id: card.id,
            rating,
            old_box,
            new_box,
            due,
        };
        match rating {
            Rating::Known => self.known += 1,
            Rating::Unknown => {
                self.unknown += 1;
                card.box_no = new_box;
                card.due = date_key(due);
                self.queue.push_back(card);
            }
        }
        Some(outcome)
    }

    /// Blättermodus: nächste Karte. Liefert `false` am Ende des Stapels.
    pub fn next(&mut self) -> bool {
        if self.queue.len() <= 1 {
            return false;
        }
        if let Some(card) = self.queue.pop_front() {
            self.seen.push(card);
        }
        true
    }

    /// Blättermodus: vorherige Karte zurück auf den Stapel legen.
    pub fn previous(&mut self) -> bool {
        match self.seen.pop() {
            Some(card) => {
                self.queue.push_front(card);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: i64, box_no: u8) -> Card {
        Card {
            id,
            deck_id: 1,
            front: format!("Frage {id}"),
            back: format!("Antwort {id}"),
            tags: String::new(),
            box_no,
            due: "2026-01-01".into(),
            reviews: 0,
            lapses: 0,
        }
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, d).expect("Datum")
    }

    #[test]
    fn known_card_moves_one_box_up() {
        assert_eq!(schedule(1, Rating::Known, day(1)), (2, day(3)));
        assert_eq!(schedule(2, Rating::Known, day(1)), (3, day(6)));
        assert_eq!(schedule(4, Rating::Known, day(1)), (5, day(31)));
        // Das letzte Fach wird nicht verlassen.
        assert_eq!(schedule(5, Rating::Known, day(1)), (5, day(31)));
    }

    #[test]
    fn unknown_card_falls_back_to_first_box() {
        assert_eq!(schedule(4, Rating::Unknown, day(7)), (1, day(7)));
        assert_eq!(schedule(1, Rating::Unknown, day(7)), (1, day(7)));
    }

    #[test]
    fn unknown_cards_return_within_the_session() {
        let mut session = Session::new(SessionMode::Learn, vec![card(1, 3), card(2, 1)]);
        assert_eq!(session.remaining(), 2);
        let outcome = session.rate(Rating::Unknown, day(1)).expect("Karte");
        assert_eq!((outcome.card_id, outcome.new_box), (1, 1));
        // Karte 1 liegt jetzt hinten, Karte 2 oben.
        assert_eq!(session.current().map(|c| c.id), Some(2));
        assert_eq!(session.remaining(), 2);
        session.rate(Rating::Known, day(1));
        let again = session.rate(Rating::Known, day(1)).expect("Karte");
        assert_eq!((again.card_id, again.old_box, again.new_box), (1, 1, 2));
        assert!(session.is_finished());
        assert_eq!((session.known(), session.unknown()), (2, 1));
    }

    #[test]
    fn browsing_goes_back_and_forth() {
        let mut session = Session::new(
            SessionMode::Browse,
            vec![card(1, 1), card(2, 1), card(3, 1)],
        );
        assert_eq!(session.position(), 1);
        assert!(session.next());
        assert!(session.next());
        assert_eq!(session.current().map(|c| c.id), Some(3));
        assert_eq!(session.position(), 3);
        assert!(!session.next());
        assert!(session.previous());
        assert_eq!(session.current().map(|c| c.id), Some(2));
        assert!(session.previous());
        assert!(!session.previous());
    }

    #[test]
    fn deck_names_are_split() {
        assert_eq!(deck_title("BGB AT::Willenserklärung"), "Willenserklärung");
        assert_eq!(
            deck_parent("BGB AT::Willenserklärung"),
            Some("BGB AT".into())
        );
        assert_eq!(
            deck_parent("Zivilrecht::BGB AT::Anfechtung"),
            Some("Zivilrecht › BGB AT".into())
        );
        assert_eq!(deck_title("Schuldrecht"), "Schuldrecht");
        assert_eq!(deck_parent("Schuldrecht"), None);
    }
}
