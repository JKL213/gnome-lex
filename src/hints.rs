// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Kuratierte Randhinweise: Zu den wichtigsten Normen des BGB und der ZPO
//! nennt die Tabelle Anschlussnormen samt Begründung. Die Leseansicht
//! zeigt sie als Pfeile in der Randspalte links neben dem Normtext.
//!
//! Richtung: `Up` (↗) führt zurück zur Grundlage (Voraussetzung,
//! Definition, Ausgangsnorm), `Down` (↘) weiter zur Anschlussnorm
//! (Rechtsfolge, nächster Prüfungsschritt, Ausnahme, Rechtsbehelf).
//!
//! Die Begründungen sind wie Normtitel juristischer Inhalt in deutscher
//! Sprache und werden bewusst nicht übersetzt.

use crate::refs::{LawRef, NormRef};

/// Richtung des Pfeils.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// ↗ Grundlage: Voraussetzung, Definition, Ausgangsnorm.
    Up,
    /// ↘ Anschluss: Rechtsfolge, nächster Schritt, Ausnahme, Rechtsbehelf.
    Down,
}

/// Ein Randhinweis auf eine Anschlussnorm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    pub direction: Direction,
    /// Kürzel des Zielgesetzes; leer = dasselbe Gesetz.
    pub law: &'static str,
    /// Bezeichnung der Zielnorm, z. B. „§ 437“.
    pub norm: &'static str,
    /// Warum die Zielnorm einschlägig ist.
    pub reason: &'static str,
}

impl Hint {
    /// Anzeigename des Ziels („§ 437“ bzw. „§ 253 ZPO“).
    pub fn label(&self) -> String {
        if self.law.is_empty() {
            self.norm.to_owned()
        } else {
            format!("{} {}", self.norm, self.law)
        }
    }

    /// Navigationsziel für die Leseansicht.
    pub fn target(&self) -> NormRef {
        NormRef {
            law: if self.law.is_empty() {
                LawRef::Same
            } else {
                LawRef::Abbrev(self.law.to_owned())
            },
            norm: Some(self.norm.to_owned()),
            sub_section: None,
            paragraph: None,
            sentence: None,
            number: None,
        }
    }
}

/// ↗ auf eine Norm desselben Gesetzes.
const fn u(norm: &'static str, reason: &'static str) -> Hint {
    Hint {
        direction: Direction::Up,
        law: "",
        norm,
        reason,
    }
}

/// ↘ auf eine Norm desselben Gesetzes.
const fn d(norm: &'static str, reason: &'static str) -> Hint {
    Hint {
        direction: Direction::Down,
        law: "",
        norm,
        reason,
    }
}

/// ↘ auf eine Norm eines anderen Gesetzes.
const fn dx(law: &'static str, norm: &'static str, reason: &'static str) -> Hint {
    Hint {
        direction: Direction::Down,
        law,
        norm,
        reason,
    }
}

type Table = &'static [(&'static str, &'static [Hint])];

/// Tabelle eines Gesetzes, falls für sein Kürzel Hinweise hinterlegt sind.
fn table_for(law_abbrev: &str) -> Option<Table> {
    match law_abbrev {
        "BGB" => Some(BGB),
        "ZPO" => Some(ZPO),
        _ => None,
    }
}

/// Gibt es für das Gesetz überhaupt Randhinweise? Danach richtet sich, ob
/// die Leseansicht die Randspalte einblendet.
pub fn has_hints(law_abbrev: &str) -> bool {
    table_for(law_abbrev).is_some()
}

/// Liefert die Randhinweise einer Norm (leer, wenn keine hinterlegt sind).
pub fn hints_for(law_abbrev: &str, enbez: &str) -> &'static [Hint] {
    let Some(table) = table_for(law_abbrev) else {
        return &[];
    };
    table
        .iter()
        .find(|(norm, _)| *norm == enbez)
        .map(|(_, hints)| *hints)
        .unwrap_or(&[])
}

#[rustfmt::skip]
const BGB: Table = &[
    // --- Allgemeiner Teil ---------------------------------------------
    ("§ 13", &[
        d("§ 14", "Gegenbegriff: Unternehmer"),
        d("§ 312", "Anwendungsbereich der Verbraucherverträge"),
        d("§ 474", "Verbrauchsgüterkauf setzt Verbraucher als Käufer voraus"),
    ]),
    ("§ 14", &[
        u("§ 13", "Gegenbegriff: Verbraucher"),
        d("§ 310", "Abs. 3: erweiterte AGB-Kontrolle bei Verbraucherverträgen"),
    ]),
    ("§ 104", &[
        d("§ 105", "Rechtsfolge: Nichtigkeit der Willenserklärung"),
        d("§ 106", "Abgrenzung: beschränkte Geschäftsfähigkeit ab sieben Jahren"),
    ]),
    ("§ 105", &[
        u("§ 104", "Wer geschäftsunfähig ist"),
        d("§ 105a", "Ausnahme: Geschäfte des täglichen Lebens"),
        d("§ 131", "Zugang von Erklärungen gegenüber nicht voll Geschäftsfähigen"),
    ]),
    ("§ 106", &[
        d("§ 107", "Grundsatz: Einwilligung des gesetzlichen Vertreters"),
        d("§ 108", "Vertragsschluss ohne Einwilligung"),
    ]),
    ("§ 107", &[
        d("§ 108", "Fehlt die Einwilligung: schwebende Unwirksamkeit des Vertrags"),
        d("§ 110", "Ausnahme: Bewirken mit eigenen Mitteln („Taschengeld“)"),
        d("§ 111", "Einseitige Rechtsgeschäfte ohne Einwilligung sind unwirksam"),
    ]),
    ("§ 108", &[
        u("§ 107", "Einwilligungsbedürftigkeit als Voraussetzung"),
        d("§ 109", "Widerrufsrecht des anderen Teils während der Schwebezeit"),
        d("§ 184", "Genehmigung wirkt auf den Vertragsschluss zurück"),
    ]),
    ("§ 119", &[
        d("§ 121", "Anfechtungsfrist: unverzüglich"),
        d("§ 143", "Anfechtung erfolgt durch Erklärung gegenüber dem Anfechtungsgegner"),
        d("§ 142", "Rechtsfolge: Nichtigkeit von Anfang an"),
        d("§ 122", "Preis der Anfechtung: Ersatz des Vertrauensschadens"),
    ]),
    ("§ 120", &[
        u("§ 119", "Gleichstellung mit dem Erklärungsirrtum"),
        d("§ 121", "Anfechtungsfrist: unverzüglich"),
        d("§ 122", "Ersatz des Vertrauensschadens"),
    ]),
    ("§ 121", &[
        u("§ 119", "Anfechtungsgrund, für den die Frist gilt"),
        d("§ 143", "Form der Ausübung: Anfechtungserklärung"),
    ]),
    ("§ 122", &[
        u("§ 119", "Anfechtung wegen Irrtums als Haftungsgrund"),
        u("§ 118", "Ebenso bei nichtiger Scherzerklärung"),
    ]),
    ("§ 123", &[
        d("§ 124", "Anfechtungsfrist: ein Jahr"),
        d("§ 143", "Anfechtung erfolgt durch Erklärung gegenüber dem Anfechtungsgegner"),
        d("§ 142", "Rechtsfolge: Nichtigkeit von Anfang an"),
    ]),
    ("§ 124", &[
        u("§ 123", "Anfechtungsgrund, für den die Jahresfrist gilt"),
    ]),
    ("§ 125", &[
        u("§ 126", "Was die gesetzliche Schriftform verlangt"),
        d("§ 311b", "Wichtigster Fall: Grundstücksgeschäfte, mit Heilung in Abs. 1 Satz 2"),
        d("§ 242", "Ausnahmsweise Durchbrechung der Formnichtigkeit nach Treu und Glauben"),
    ]),
    ("§ 126", &[
        d("§ 125", "Rechtsfolge des Formmangels: Nichtigkeit"),
        d("§ 126a", "Ersetzung durch die elektronische Form"),
        d("§ 126b", "Abgrenzung: Textform"),
    ]),
    ("§ 130", &[
        d("§ 131", "Zugang gegenüber nicht voll Geschäftsfähigen"),
        d("§ 145", "Mit Zugang wird der Antrag bindend"),
    ]),
    ("§ 133", &[
        d("§ 157", "Ergänzend: Auslegung von Verträgen nach Treu und Glauben"),
    ]),
    ("§ 134", &[
        d("§ 139", "Reichweite der Nichtigkeit bei Teilverstoß"),
        d("§ 817", "Satz 2: Kondiktionssperre bei der Rückabwicklung"),
    ]),
    ("§ 138", &[
        d("§ 139", "Reichweite der Nichtigkeit"),
        d("§ 817", "Satz 2: Kondiktionssperre bei der Rückabwicklung"),
        d("§ 826", "Deliktische Parallele: sittenwidrige Schädigung"),
    ]),
    ("§ 139", &[
        d("§ 140", "Umdeutung des nichtigen Geschäfts"),
    ]),
    ("§ 142", &[
        u("§ 119", "Anfechtungsgrund Irrtum"),
        u("§ 123", "Anfechtungsgrund Täuschung oder Drohung"),
        u("§ 143", "Erforderliche Anfechtungserklärung"),
        d("§ 812", "Rückabwicklung des Geleisteten nach Bereicherungsrecht"),
    ]),
    ("§ 143", &[
        u("§ 119", "Anfechtungsgrund Irrtum"),
        u("§ 123", "Anfechtungsgrund Täuschung oder Drohung"),
        d("§ 142", "Wirkung der Anfechtung"),
    ]),
    ("§ 145", &[
        d("§ 146", "Erlöschen des Antrags"),
        d("§ 147", "Annahmefrist"),
        d("§ 150", "Verspätete oder abändernde Annahme"),
    ]),
    ("§ 147", &[
        u("§ 145", "Bindung an den Antrag"),
        d("§ 148", "Vom Antragenden bestimmte Annahmefrist"),
        d("§ 150", "Verspätete Annahme gilt als neuer Antrag"),
    ]),
    ("§ 150", &[
        u("§ 147", "Wann die Annahme verspätet ist"),
        d("§ 151", "Annahme ohne Erklärung gegenüber dem Antragenden"),
    ]),
    ("§ 157", &[
        u("§ 133", "Auslegung der einzelnen Willenserklärung"),
        d("§ 242", "Treu und Glauben bei der Leistungsbewirkung"),
    ]),
    ("§ 164", &[
        d("§ 166", "Willensmängel und Wissenszurechnung"),
        d("§ 167", "Erteilung der Vollmacht"),
        d("§ 177", "Handeln ohne Vertretungsmacht"),
        d("§ 181", "Grenze: Insichgeschäft"),
    ]),
    ("§ 167", &[
        u("§ 164", "Vertretungsmacht als Voraussetzung der Stellvertretung"),
        d("§ 168", "Erlöschen der Vollmacht"),
        d("§ 170", "Rechtsscheintatbestände der §§ 170 bis 172"),
    ]),
    ("§ 177", &[
        u("§ 164", "Fehlende Vertretungsmacht als Ausgangspunkt"),
        d("§ 178", "Widerrufsrecht des anderen Teils"),
        d("§ 179", "Haftung des Vertreters ohne Vertretungsmacht"),
        d("§ 184", "Genehmigung wirkt zurück"),
    ]),
    ("§ 179", &[
        u("§ 177", "Verweigerte Genehmigung als Voraussetzung"),
    ]),
    ("§ 181", &[
        u("§ 164", "Beschränkung der Vertretungsmacht"),
        d("§ 177", "Folge des Verstoßes: schwebende Unwirksamkeit"),
    ]),
    ("§ 182", &[
        d("§ 183", "Vorherige Zustimmung: Einwilligung"),
        d("§ 184", "Nachträgliche Zustimmung: Genehmigung"),
    ]),
    ("§ 185", &[
        u("§ 182", "Allgemeine Regeln der Zustimmung"),
        d("§ 816", "Ausgleich: Herausgabe des Erlangten durch den Nichtberechtigten"),
        d("§ 932", "Ohne Zustimmung nur gutgläubiger Erwerb"),
    ]),
    ("§ 187", &[
        d("§ 188", "Fristende"),
        d("§ 193", "Fristende an Sonn- und Feiertagen sowie Sonnabenden"),
    ]),
    ("§ 188", &[
        u("§ 187", "Fristbeginn"),
        d("§ 193", "Fristende an Sonn- und Feiertagen sowie Sonnabenden"),
    ]),
    ("§ 194", &[
        d("§ 195", "Regelmäßige Verjährungsfrist"),
        d("§ 199", "Beginn der regelmäßigen Verjährung"),
        d("§ 214", "Wirkung: Leistungsverweigerungsrecht"),
    ]),
    ("§ 195", &[
        u("§ 194", "Gegenstand der Verjährung"),
        d("§ 199", "Fristbeginn und Höchstfristen"),
        d("§ 197", "Sonderfall: dreißigjährige Frist"),
    ]),
    ("§ 199", &[
        u("§ 195", "Dauer der regelmäßigen Frist"),
        d("§ 203", "Hemmung durch Verhandlungen"),
        d("§ 204", "Hemmung durch Rechtsverfolgung"),
        d("§ 212", "Neubeginn der Verjährung"),
    ]),
    ("§ 203", &[
        u("§ 199", "Laufende Verjährungsfrist"),
        d("§ 209", "Wirkung der Hemmung"),
    ]),
    ("§ 204", &[
        d("§ 209", "Wirkung der Hemmung"),
        dx("ZPO", "§ 253", "Klageerhebung als Hemmungstatbestand"),
        dx("ZPO", "§ 167", "Rückwirkung der Zustellung auf den Eingang"),
    ]),
    ("§ 214", &[
        u("§ 194", "Verjährter Anspruch als Voraussetzung"),
        d("§ 215", "Aufrechnung und Zurückbehaltung trotz Verjährung"),
        d("§ 216", "Fortbestand gesicherter Rechte"),
    ]),
    // --- Allgemeines Schuldrecht --------------------------------------
    ("§ 241", &[
        d("§ 242", "Inhalt der Leistungspflicht nach Treu und Glauben"),
        d("§ 280", "Rechtsfolge der Pflichtverletzung"),
        d("§ 311", "Abs. 2: Schutzpflichten schon im vorvertraglichen Schuldverhältnis"),
    ]),
    ("§ 242", &[
        u("§ 241", "Schuldverhältnis als Anknüpfungspunkt"),
        d("§ 313", "Kodifizierte Fallgruppe: Störung der Geschäftsgrundlage"),
    ]),
    ("§ 249", &[
        u("§ 280", "Vertraglicher Haftungsgrund"),
        u("§ 823", "Deliktischer Haftungsgrund"),
        d("§ 251", "Geldersatz, wenn Herstellung unmöglich oder unverhältnismäßig"),
        d("§ 253", "Immaterieller Schaden"),
        d("§ 254", "Kürzung wegen Mitverschuldens"),
    ]),
    ("§ 251", &[
        u("§ 249", "Vorrang der Naturalrestitution"),
    ]),
    ("§ 252", &[
        u("§ 249", "Umfang des zu ersetzenden Schadens"),
    ]),
    ("§ 253", &[
        u("§ 249", "Grundsatz der Naturalrestitution"),
        u("§ 823", "Verletzung der in Abs. 2 genannten Rechtsgüter"),
    ]),
    ("§ 254", &[
        u("§ 249", "Schadensersatzanspruch, der gekürzt wird"),
        d("§ 278", "Abs. 2 Satz 2: Zurechnung des Verschuldens von Hilfspersonen"),
    ]),
    ("§ 271", &[
        d("§ 286", "Fälligkeit als Voraussetzung des Verzugs"),
    ]),
    ("§ 273", &[
        d("§ 274", "Wirkung: Verurteilung Zug um Zug"),
        d("§ 320", "Sonderfall im gegenseitigen Vertrag"),
    ]),
    ("§ 275", &[
        d("§ 326", "Schicksal der Gegenleistung"),
        d("§ 283", "Schadensersatz statt der Leistung"),
        d("§ 311a", "Anfängliches Leistungshindernis"),
        d("§ 285", "Herausgabe des Ersatzes"),
    ]),
    ("§ 276", &[
        u("§ 280", "Vertretenmüssen nach Abs. 1 Satz 2"),
        d("§ 277", "Sorgfalt in eigenen Angelegenheiten"),
        d("§ 278", "Haftung für Erfüllungsgehilfen"),
    ]),
    ("§ 278", &[
        u("§ 276", "Maßstab des Vertretenmüssens"),
        d("§ 831", "Gegenstück im Deliktsrecht, dort mit Exkulpation"),
    ]),
    ("§ 280", &[
        u("§ 241", "Pflichten aus dem Schuldverhältnis"),
        u("§ 276", "Vertretenmüssen (Abs. 1 Satz 2)"),
        d("§ 281", "Schadensersatz statt der Leistung (Abs. 3)"),
        d("§ 286", "Verzögerungsschaden nur bei Verzug (Abs. 2)"),
        d("§ 249", "Art und Umfang des Schadensersatzes"),
    ]),
    ("§ 281", &[
        u("§ 280", "Grundtatbestand, auf den Abs. 3 verweist"),
        d("§ 323", "Rücktritt unter parallelen Voraussetzungen"),
        d("§ 284", "Alternativ: Ersatz vergeblicher Aufwendungen"),
    ]),
    ("§ 282", &[
        u("§ 280", "Grundtatbestand, auf den Abs. 3 verweist"),
        u("§ 241", "Verletzte Rücksichtnahmepflicht nach Abs. 2"),
    ]),
    ("§ 283", &[
        u("§ 275", "Ausschluss der Leistungspflicht"),
        u("§ 280", "Grundtatbestand, auf den Abs. 3 verweist"),
    ]),
    ("§ 284", &[
        u("§ 281", "Anspruch auf Schadensersatz statt der Leistung als Voraussetzung"),
    ]),
    ("§ 285", &[
        u("§ 275", "Ausschluss der Leistungspflicht"),
    ]),
    ("§ 286", &[
        u("§ 271", "Fälligkeit der Leistung"),
        u("§ 280", "Anspruchsgrundlage für den Verzögerungsschaden"),
        d("§ 287", "Verschärfte Haftung im Verzug"),
        d("§ 288", "Verzugszinsen"),
    ]),
    ("§ 288", &[
        u("§ 286", "Verzug als Voraussetzung"),
        d("§ 247", "Basiszinssatz als Bezugsgröße"),
        d("§ 291", "Prozesszinsen ab Rechtshängigkeit"),
    ]),
    ("§ 293", &[
        d("§ 294", "Tatsächliches Angebot"),
        d("§ 295", "Wörtliches Angebot"),
        d("§ 300", "Wirkungen: Haftungsmilderung und Gefahrübergang"),
        d("§ 326", "Abs. 2: Gegenleistung bleibt geschuldet"),
    ]),
    ("§ 305", &[
        d("§ 305b", "Vorrang der Individualabrede"),
        d("§ 305c", "Überraschende und mehrdeutige Klauseln"),
        d("§ 307", "Inhaltskontrolle"),
        d("§ 306", "Folgen der Nichteinbeziehung und Unwirksamkeit"),
    ]),
    ("§ 305c", &[
        u("§ 305", "Vorliegen und Einbeziehung von AGB"),
        d("§ 306", "Rechtsfolgen"),
    ]),
    ("§ 306", &[
        u("§ 305", "Scheitern der Einbeziehung"),
        u("§ 307", "Unwirksamkeit nach Inhaltskontrolle"),
    ]),
    ("§ 307", &[
        u("§ 305", "Vorliegen und Einbeziehung von AGB"),
        u("§ 309", "Vorab zu prüfen: Klauselverbote ohne Wertungsmöglichkeit"),
        u("§ 308", "Vorab zu prüfen: Klauselverbote mit Wertungsmöglichkeit"),
        d("§ 306", "Rechtsfolge der Unwirksamkeit"),
    ]),
    ("§ 308", &[
        u("§ 305", "Vorliegen und Einbeziehung von AGB"),
        d("§ 307", "Auffangtatbestand: Generalklausel"),
        d("§ 306", "Rechtsfolge der Unwirksamkeit"),
    ]),
    ("§ 309", &[
        u("§ 305", "Vorliegen und Einbeziehung von AGB"),
        d("§ 308", "Danach: Klauselverbote mit Wertungsmöglichkeit"),
        d("§ 306", "Rechtsfolge der Unwirksamkeit"),
    ]),
    ("§ 310", &[
        u("§ 305", "Begriff und Einbeziehung, deren Anwendungsbereich hier bestimmt wird"),
    ]),
    ("§ 311", &[
        d("§ 241", "Abs. 2: Inhalt der vorvertraglichen Pflichten"),
        d("§ 280", "Haftung aus Verschulden bei Vertragsverhandlungen"),
        d("§ 311b", "Formbedürftige Verträge"),
    ]),
    ("§ 311a", &[
        u("§ 275", "Leistungshindernis schon bei Vertragsschluss"),
        d("§ 284", "Wahlweise Aufwendungsersatz"),
    ]),
    ("§ 311b", &[
        d("§ 125", "Rechtsfolge des Formmangels"),
        d("§ 873", "Heilung durch Auflassung und Eintragung"),
        d("§ 925", "Auflassung"),
    ]),
    ("§ 312", &[
        d("§ 312b", "Außerhalb von Geschäftsräumen geschlossene Verträge"),
        d("§ 312c", "Fernabsatzverträge"),
        d("§ 312g", "Widerrufsrecht"),
    ]),
    ("§ 312b", &[
        u("§ 312", "Anwendungsbereich"),
        d("§ 312d", "Informationspflichten"),
        d("§ 312g", "Widerrufsrecht"),
    ]),
    ("§ 312c", &[
        u("§ 312", "Anwendungsbereich"),
        d("§ 312d", "Informationspflichten"),
        d("§ 312g", "Widerrufsrecht"),
    ]),
    ("§ 312g", &[
        u("§ 312b", "Außerhalb von Geschäftsräumen geschlossener Vertrag"),
        u("§ 312c", "Fernabsatzvertrag"),
        d("§ 355", "Ausübung und Wirkung des Widerrufs"),
        d("§ 356", "Beginn und Erlöschen der Widerrufsfrist"),
    ]),
    ("§ 313", &[
        u("§ 242", "Herleitung aus Treu und Glauben"),
        d("§ 314", "Bei Dauerschuldverhältnissen: Kündigung"),
        d("§ 346", "Abs. 3: Rückabwicklung nach Rücktritt"),
    ]),
    ("§ 314", &[
        u("§ 313", "Abgrenzung: Störung der Geschäftsgrundlage"),
        d("§ 543", "Sonderregel für die Miete"),
        d("§ 626", "Sonderregel für den Dienstvertrag"),
    ]),
    ("§ 320", &[
        u("§ 273", "Allgemeines Zurückbehaltungsrecht"),
        d("§ 322", "Wirkung: Verurteilung Zug um Zug"),
        d("§ 321", "Unsicherheitseinrede des Vorleistungspflichtigen"),
    ]),
    ("§ 323", &[
        u("§ 281", "Parallele Voraussetzungen beim Schadensersatz"),
        d("§ 349", "Rücktrittserklärung"),
        d("§ 346", "Rechtsfolgen des Rücktritts"),
        d("§ 325", "Schadensersatz bleibt neben dem Rücktritt möglich"),
    ]),
    ("§ 324", &[
        u("§ 241", "Verletzte Rücksichtnahmepflicht nach Abs. 2"),
        d("§ 346", "Rechtsfolgen des Rücktritts"),
    ]),
    ("§ 326", &[
        u("§ 275", "Ausschluss der Leistungspflicht"),
        d("§ 346", "Abs. 4: Rückforderung der bewirkten Gegenleistung"),
        d("§ 285", "Abs. 3: Gegenleistung bei Verlangen des Surrogats"),
    ]),
    ("§ 328", &[
        d("§ 333", "Zurückweisung des Rechts durch den Dritten"),
        d("§ 334", "Einwendungen des Schuldners gegenüber dem Dritten"),
    ]),
    ("§ 346", &[
        u("§ 323", "Gesetzliches Rücktrittsrecht"),
        u("§ 349", "Rücktrittserklärung"),
        d("§ 347", "Nutzungen und Verwendungen"),
        d("§ 348", "Erfüllung Zug um Zug"),
    ]),
    ("§ 355", &[
        u("§ 312g", "Widerrufsrecht bei besonderen Vertriebsformen"),
        d("§ 356", "Fristbeginn und Erlöschen"),
        d("§ 357", "Rechtsfolgen des Widerrufs"),
    ]),
    ("§ 356", &[
        u("§ 355", "Grundregel der vierzehntägigen Frist"),
        d("§ 357", "Rechtsfolgen des Widerrufs"),
    ]),
    ("§ 357", &[
        u("§ 355", "Wirksamer Widerruf"),
        d("§ 357a", "Wertersatz"),
    ]),
    ("§ 362", &[
        d("§ 363", "Beweislast nach Annahme als Erfüllung"),
        d("§ 364", "Leistung an Erfüllungs statt"),
        d("§ 366", "Tilgungsbestimmung bei mehreren Forderungen"),
    ]),
    ("§ 364", &[
        u("§ 362", "Grundfall der Erfüllung"),
    ]),
    ("§ 387", &[
        d("§ 388", "Aufrechnungserklärung"),
        d("§ 389", "Wirkung: Erlöschen mit Rückwirkung"),
        d("§ 390", "Keine Aufrechnung mit einredebehafteter Forderung"),
        d("§ 393", "Aufrechnungsverbot bei unerlaubter Handlung"),
    ]),
    ("§ 388", &[
        u("§ 387", "Aufrechnungslage"),
        d("§ 389", "Wirkung der Aufrechnung"),
    ]),
    ("§ 389", &[
        u("§ 387", "Aufrechnungslage"),
        u("§ 388", "Aufrechnungserklärung"),
    ]),
    ("§ 398", &[
        d("§ 399", "Ausschluss der Abtretung"),
        d("§ 401", "Übergang der Neben- und Vorzugsrechte"),
        d("§ 404", "Einwendungen des Schuldners"),
        d("§ 407", "Schutz des unwissenden Schuldners"),
    ]),
    ("§ 399", &[
        u("§ 398", "Grundsatz der freien Abtretbarkeit"),
        dx("HGB", "§ 354a", "Ausnahme im Handelsverkehr: Abtretung trotz Verbots wirksam"),
    ]),
    ("§ 404", &[
        u("§ 398", "Wirksame Abtretung"),
        d("§ 406", "Aufrechnung gegenüber dem neuen Gläubiger"),
    ]),
    ("§ 407", &[
        u("§ 398", "Wirksame Abtretung"),
        d("§ 408", "Mehrfache Abtretung"),
        d("§ 409", "Abtretungsanzeige"),
        d("§ 816", "Abs. 2: Ausgleich zwischen altem und neuem Gläubiger"),
    ]),
    ("§ 414", &[
        d("§ 415", "Schuldübernahme durch Vertrag mit dem Schuldner"),
        d("§ 417", "Einwendungen des Übernehmers"),
    ]),
    ("§ 421", &[
        d("§ 422", "Wirkung der Erfüllung für alle"),
        d("§ 425", "Einzelwirkung anderer Tatsachen"),
        d("§ 426", "Ausgleich im Innenverhältnis"),
    ]),
    ("§ 426", &[
        u("§ 421", "Gesamtschuld als Voraussetzung"),
        u("§ 840", "Gesamtschuld mehrerer Schädiger"),
    ]),
    // --- Besonderes Schuldrecht ---------------------------------------
    ("§ 433", &[
        d("§ 434", "Wann die Sache mangelfrei ist"),
        d("§ 437", "Rechte des Käufers bei Mängeln"),
        d("§ 446", "Gefahr- und Lastenübergang"),
        d("§ 474", "Sonderregeln für den Verbrauchsgüterkauf"),
    ]),
    ("§ 434", &[
        u("§ 433", "Pflicht zur mangelfreien Verschaffung (Abs. 1 Satz 2)"),
        d("§ 446", "Maßgeblicher Zeitpunkt: Gefahrübergang"),
        d("§ 437", "Rechtsfolgen des Mangels"),
        d("§ 442", "Ausschluss bei Kenntnis des Käufers"),
    ]),
    ("§ 435", &[
        u("§ 433", "Pflicht zur mangelfreien Verschaffung (Abs. 1 Satz 2)"),
        d("§ 437", "Rechtsfolgen des Mangels"),
    ]),
    ("§ 437", &[
        u("§ 434", "Sachmangel als Voraussetzung"),
        d("§ 439", "Nr. 1: Nacherfüllung, vorrangig"),
        d("§ 323", "Nr. 2: Rücktritt"),
        d("§ 441", "Nr. 2: Minderung"),
        d("§ 280", "Nr. 3: Schadensersatz"),
        d("§ 438", "Verjährung der Mängelansprüche"),
    ]),
    ("§ 438", &[
        u("§ 437", "Ansprüche, die verjähren"),
        d("§ 218", "Unwirksamkeit von Rücktritt und Minderung nach Verjährung"),
    ]),
    ("§ 439", &[
        u("§ 437", "Nacherfüllung als erstes Mängelrecht (Nr. 1)"),
        d("§ 440", "Wann die Fristsetzung entbehrlich ist"),
        d("§ 323", "Nach erfolgloser Frist: Rücktritt"),
    ]),
    ("§ 440", &[
        u("§ 439", "Nacherfüllung, die fehlschlägt oder verweigert wird"),
        u("§ 323", "Grundsatz der Fristsetzung"),
    ]),
    ("§ 441", &[
        u("§ 437", "Minderung als Mängelrecht (Nr. 2)"),
        u("§ 323", "Voraussetzungen wie beim Rücktritt („statt zurückzutreten“)"),
    ]),
    ("§ 442", &[
        u("§ 437", "Mängelrechte, die ausgeschlossen werden"),
    ]),
    ("§ 444", &[
        u("§ 437", "Mängelrechte, deren Ausschluss vereinbart wurde"),
        d("§ 476", "Strengere Grenze beim Verbrauchsgüterkauf"),
    ]),
    ("§ 446", &[
        u("§ 434", "Zeitpunkt für die Beurteilung der Mangelfreiheit"),
        d("§ 447", "Vorverlagerung beim Versendungskauf"),
        d("§ 326", "Preisgefahr ohne Übergabe"),
    ]),
    ("§ 447", &[
        u("§ 446", "Grundregel des Gefahrübergangs"),
        d("§ 475", "Abs. 2: Einschränkung beim Verbrauchsgüterkauf"),
    ]),
    ("§ 449", &[
        d("§ 158", "Aufschiebend bedingte Übereignung"),
        d("§ 929", "Dinglicher Vollzug"),
        d("§ 323", "Rücktritt bei Zahlungsverzug als Voraussetzung der Herausgabe"),
    ]),
    ("§ 474", &[
        u("§ 13", "Verbraucher als Käufer"),
        u("§ 14", "Unternehmer als Verkäufer"),
        d("§ 475", "Abweichungen vom allgemeinen Kaufrecht"),
        d("§ 476", "Grenzen abweichender Vereinbarungen"),
        d("§ 477", "Beweislastumkehr"),
    ]),
    ("§ 475", &[
        u("§ 474", "Verbrauchsgüterkauf"),
    ]),
    ("§ 476", &[
        u("§ 474", "Verbrauchsgüterkauf"),
        u("§ 437", "Mängelrechte, die nicht abbedungen werden können"),
    ]),
    ("§ 477", &[
        u("§ 474", "Verbrauchsgüterkauf"),
        u("§ 434", "Vermuteter Mangel bei Gefahrübergang"),
    ]),
    ("§ 488", &[
        d("§ 489", "Ordentliches Kündigungsrecht des Darlehensnehmers"),
        d("§ 490", "Außerordentliches Kündigungsrecht"),
        d("§ 491", "Sonderregeln für Verbraucherdarlehen"),
    ]),
    ("§ 491", &[
        u("§ 488", "Darlehensvertrag"),
        d("§ 492", "Schriftform und Pflichtangaben"),
        d("§ 495", "Widerrufsrecht"),
    ]),
    ("§ 495", &[
        u("§ 491", "Verbraucherdarlehensvertrag"),
        d("§ 355", "Ausübung und Wirkung des Widerrufs"),
        d("§ 356b", "Beginn der Widerrufsfrist"),
    ]),
    ("§ 516", &[
        d("§ 518", "Form des Schenkungsversprechens"),
        d("§ 528", "Rückforderung wegen Verarmung"),
        d("§ 530", "Widerruf wegen groben Undanks"),
    ]),
    ("§ 518", &[
        u("§ 516", "Schenkung"),
        d("§ 125", "Folge des Formmangels, vorbehaltlich der Heilung nach Abs. 2"),
    ]),
    ("§ 535", &[
        d("§ 536", "Minderung bei Mängeln"),
        d("§ 543", "Außerordentliche fristlose Kündigung"),
        d("§ 546", "Rückgabepflicht bei Vertragsende"),
        d("§ 549", "Sonderregeln für Wohnraum"),
    ]),
    ("§ 536", &[
        u("§ 535", "Pflicht zur Erhaltung des vertragsgemäßen Zustands"),
        d("§ 536a", "Schadens- und Aufwendungsersatz"),
        d("§ 536b", "Ausschluss bei Kenntnis des Mieters"),
        d("§ 536c", "Anzeigepflicht des Mieters"),
    ]),
    ("§ 536a", &[
        u("§ 536", "Mangel der Mietsache"),
    ]),
    ("§ 543", &[
        u("§ 535", "Mietverhältnis und seine Hauptpflichten"),
        d("§ 569", "Ergänzungen für Wohnraum, auch Schonfrist"),
        d("§ 546", "Rückgabe nach Beendigung"),
    ]),
    ("§ 546", &[
        u("§ 542", "Ende des Mietverhältnisses"),
        d("§ 546a", "Entschädigung bei verspäteter Rückgabe"),
    ]),
    ("§ 558", &[
        d("§ 558a", "Form und Begründung des Erhöhungsverlangens"),
        d("§ 558b", "Zustimmung und Klage"),
    ]),
    ("§ 566", &[
        u("§ 535", "Bestehendes Mietverhältnis über Wohnraum"),
        d("§ 566a", "Mietsicherheit beim Erwerber"),
    ]),
    ("§ 573", &[
        u("§ 542", "Ordentliche Kündigung als Beendigungsgrund"),
        d("§ 568", "Schriftform der Kündigung"),
        d("§ 573c", "Kündigungsfristen"),
        d("§ 574", "Widerspruch des Mieters (Sozialklausel)"),
    ]),
    ("§ 574", &[
        u("§ 573", "Wirksame ordentliche Kündigung"),
    ]),
    ("§ 611", &[
        d("§ 611a", "Sonderform: Arbeitsvertrag"),
        d("§ 615", "Vergütung bei Annahmeverzug"),
        d("§ 626", "Fristlose Kündigung aus wichtigem Grund"),
    ]),
    ("§ 611a", &[
        u("§ 611", "Dienstvertrag als Grundtyp"),
        d("§ 613a", "Betriebsübergang"),
        d("§ 622", "Kündigungsfristen"),
    ]),
    ("§ 615", &[
        u("§ 611", "Vergütungsanspruch"),
        u("§ 293", "Annahmeverzug des Dienstberechtigten"),
    ]),
    ("§ 620", &[
        d("§ 622", "Kündigungsfristen bei Arbeitsverhältnissen"),
        d("§ 623", "Schriftform der Kündigung"),
        d("§ 626", "Fristlose Kündigung"),
    ]),
    ("§ 622", &[
        u("§ 620", "Ordentliche Kündigung"),
        d("§ 623", "Schriftform der Kündigung"),
    ]),
    ("§ 623", &[
        u("§ 126", "Anforderungen der Schriftform"),
        d("§ 125", "Folge des Formmangels: Nichtigkeit"),
    ]),
    ("§ 626", &[
        u("§ 314", "Allgemeine Regel für Dauerschuldverhältnisse"),
        d("§ 623", "Schriftform bei Arbeitsverhältnissen"),
        d("§ 628", "Teilvergütung und Schadensersatz"),
    ]),
    ("§ 631", &[
        d("§ 632", "Vergütung"),
        d("§ 633", "Mangelfreiheit des Werks"),
        d("§ 640", "Abnahme"),
        d("§ 641", "Fälligkeit der Vergütung"),
    ]),
    ("§ 633", &[
        u("§ 631", "Pflicht zur Herstellung des versprochenen Werks"),
        d("§ 634", "Rechte des Bestellers bei Mängeln"),
    ]),
    ("§ 634", &[
        u("§ 633", "Mangel als Voraussetzung"),
        d("§ 635", "Nr. 1: Nacherfüllung"),
        d("§ 637", "Nr. 2: Selbstvornahme"),
        d("§ 280", "Nr. 4: Schadensersatz"),
        d("§ 634a", "Verjährung der Mängelansprüche"),
    ]),
    ("§ 635", &[
        u("§ 634", "Nacherfüllung als erstes Mängelrecht (Nr. 1)"),
    ]),
    ("§ 637", &[
        u("§ 634", "Selbstvornahme als Mängelrecht (Nr. 2)"),
    ]),
    ("§ 640", &[
        u("§ 631", "Herstellung des Werks"),
        d("§ 641", "Fälligkeit der Vergütung"),
        d("§ 644", "Gefahrübergang"),
        d("§ 634a", "Beginn der Verjährung"),
    ]),
    ("§ 641", &[
        u("§ 640", "Abnahme als Voraussetzung der Fälligkeit"),
    ]),
    ("§ 648", &[
        u("§ 631", "Werkvertrag"),
        d("§ 648a", "Kündigung aus wichtigem Grund"),
    ]),
    ("§ 662", &[
        d("§ 665", "Weisungen des Auftraggebers"),
        d("§ 667", "Herausgabepflicht"),
        d("§ 670", "Aufwendungsersatz"),
    ]),
    ("§ 667", &[
        u("§ 662", "Auftrag"),
        u("§ 681", "Verweisung aus der Geschäftsführung ohne Auftrag"),
    ]),
    ("§ 670", &[
        u("§ 662", "Auftrag"),
        u("§ 683", "Verweisung aus der berechtigten Geschäftsführung ohne Auftrag"),
    ]),
    ("§ 675", &[
        u("§ 662", "Auftragsrecht, das weitgehend entsprechend gilt"),
        d("§ 667", "Herausgabepflicht"),
        d("§ 670", "Aufwendungsersatz"),
    ]),
    ("§ 677", &[
        d("§ 683", "Aufwendungsersatz bei berechtigter Geschäftsführung"),
        d("§ 681", "Nebenpflichten des Geschäftsführers"),
        d("§ 678", "Haftung bei Übernahme gegen den Willen des Geschäftsherrn"),
        d("§ 687", "Abgrenzung: Eigengeschäftsführung"),
    ]),
    ("§ 683", &[
        u("§ 677", "Geschäftsführung ohne Auftrag"),
        d("§ 670", "Umfang des Aufwendungsersatzes"),
        d("§ 684", "Wenn die Voraussetzungen fehlen"),
    ]),
    ("§ 684", &[
        u("§ 683", "Fehlende Berechtigung der Geschäftsführung"),
        d("§ 812", "Herausgabe nach Bereicherungsrecht"),
    ]),
    ("§ 687", &[
        u("§ 677", "Fremdgeschäftsführungswille als Abgrenzungsmerkmal"),
    ]),
    ("§ 765", &[
        d("§ 766", "Schriftform der Bürgschaftserklärung"),
        d("§ 767", "Akzessorietät: Umfang der Bürgschaftsschuld"),
        d("§ 768", "Einreden des Bürgen"),
        d("§ 771", "Einrede der Vorausklage"),
    ]),
    ("§ 766", &[
        u("§ 765", "Bürgschaftsvertrag"),
        dx("HGB", "§ 350", "Formfreiheit für Kaufleute"),
    ]),
    ("§ 767", &[
        u("§ 765", "Bürgschaftsvertrag"),
    ]),
    ("§ 771", &[
        u("§ 765", "Bürgschaftsvertrag"),
        d("§ 773", "Ausschluss der Einrede"),
        dx("HGB", "§ 349", "Keine Einrede für Kaufleute"),
    ]),
    ("§ 774", &[
        u("§ 765", "Befriedigung des Gläubigers durch den Bürgen"),
        d("§ 412", "Gesetzlicher Forderungsübergang: Abtretungsrecht gilt entsprechend"),
    ]),
    ("§ 780", &[
        d("§ 781", "Schuldanerkenntnis"),
        d("§ 812", "Abs. 2: Kondiktion des Versprechens"),
    ]),
    ("§ 812", &[
        d("§ 814", "Ausschluss bei Kenntnis der Nichtschuld"),
        d("§ 817", "Verstoß gegen Gesetz oder gute Sitten"),
        d("§ 818", "Umfang des Anspruchs und Entreicherung"),
        d("§ 819", "Verschärfte Haftung"),
    ]),
    ("§ 814", &[
        u("§ 812", "Leistungskondiktion, die ausgeschlossen wird"),
    ]),
    ("§ 816", &[
        u("§ 932", "Wirksame Verfügung kraft guten Glaubens"),
        u("§ 185", "Wirksame Verfügung kraft Genehmigung"),
        d("§ 818", "Umfang des Anspruchs"),
    ]),
    ("§ 817", &[
        u("§ 812", "Leistungskondiktion"),
        u("§ 134", "Gesetzesverstoß"),
        u("§ 138", "Sittenverstoß"),
    ]),
    ("§ 818", &[
        u("§ 812", "Anspruchsgrund"),
        d("§ 819", "Kein Entreicherungseinwand bei Kenntnis"),
        d("§ 820", "Verschärfte Haftung bei ungewissem Erfolg"),
    ]),
    ("§ 819", &[
        u("§ 818", "Abs. 4: Haftung nach den allgemeinen Vorschriften"),
        d("§ 292", "Haftung wie nach Rechtshängigkeit"),
    ]),
    ("§ 822", &[
        u("§ 812", "Anspruch gegen den Erstempfänger"),
        u("§ 818", "Abs. 3: Entreicherung des Erstempfängers als Voraussetzung"),
    ]),
    ("§ 823", &[
        d("§ 249", "Art und Umfang des Schadensersatzes"),
        d("§ 253", "Schmerzensgeld"),
        d("§ 831", "Haftung für Verrichtungsgehilfen"),
        d("§ 840", "Mehrere Schädiger haften als Gesamtschuldner"),
        d("§ 1004", "Ergänzend: Beseitigung und Unterlassung"),
    ]),
    ("§ 826", &[
        u("§ 138", "Maßstab der Sittenwidrigkeit"),
        d("§ 249", "Art und Umfang des Schadensersatzes"),
    ]),
    ("§ 828", &[
        u("§ 823", "Haftungstatbestand, dessen Verschulden zu prüfen ist"),
        d("§ 829", "Billigkeitshaftung"),
        d("§ 832", "Haftung des Aufsichtspflichtigen"),
    ]),
    ("§ 830", &[
        u("§ 823", "Unerlaubte Handlung"),
        d("§ 840", "Gesamtschuldnerische Haftung"),
    ]),
    ("§ 831", &[
        u("§ 823", "Rechtswidrige unerlaubte Handlung des Gehilfen"),
        u("§ 278", "Abgrenzung: Erfüllungsgehilfe ohne Exkulpation"),
        d("§ 840", "Gesamtschuld mit dem Gehilfen"),
    ]),
    ("§ 832", &[
        u("§ 823", "Rechtswidrige Schädigung durch den Aufsichtsbedürftigen"),
    ]),
    ("§ 833", &[
        d("§ 834", "Haftung des Tieraufsehers"),
        d("§ 249", "Art und Umfang des Schadensersatzes"),
    ]),
    ("§ 839", &[
        dx("GG", "Art. 34", "Überleitung der Haftung auf den Staat"),
    ]),
    ("§ 840", &[
        u("§ 830", "Mittäter und Beteiligte"),
        d("§ 426", "Ausgleich unter den Gesamtschuldnern"),
    ]),
    // --- Sachenrecht --------------------------------------------------
    ("§ 854", &[
        d("§ 855", "Besitzdiener"),
        d("§ 868", "Mittelbarer Besitz"),
        d("§ 858", "Schutz gegen verbotene Eigenmacht"),
    ]),
    ("§ 858", &[
        d("§ 859", "Selbsthilfe des Besitzers"),
        d("§ 861", "Anspruch wegen Besitzentziehung"),
        d("§ 862", "Anspruch wegen Besitzstörung"),
    ]),
    ("§ 861", &[
        u("§ 858", "Verbotene Eigenmacht"),
        d("§ 863", "Beschränkung der Einwendungen"),
        d("§ 864", "Erlöschen nach einem Jahr"),
    ]),
    ("§ 868", &[
        u("§ 854", "Unmittelbarer Besitz des Besitzmittlers"),
        d("§ 930", "Übereignung durch Besitzkonstitut"),
        d("§ 931", "Übereignung durch Abtretung des Herausgabeanspruchs"),
    ]),
    ("§ 873", &[
        d("§ 925", "Form der Einigung bei Grundstücken: Auflassung"),
        d("§ 883", "Sicherung des Anspruchs durch Vormerkung"),
        d("§ 892", "Erwerb vom Nichtberechtigten"),
    ]),
    ("§ 883", &[
        d("§ 885", "Voraussetzungen der Eintragung"),
        d("§ 888", "Durchsetzung gegenüber dem Dritterwerber"),
    ]),
    ("§ 888", &[
        u("§ 883", "Relative Unwirksamkeit vormerkungswidriger Verfügungen"),
    ]),
    ("§ 891", &[
        d("§ 892", "Öffentlicher Glaube des Grundbuchs"),
    ]),
    ("§ 892", &[
        u("§ 873", "Erwerbstatbestand im Übrigen"),
        u("§ 891", "Vermutung der Richtigkeit des Grundbuchs"),
        d("§ 894", "Berichtigungsanspruch des wahren Berechtigten"),
        d("§ 816", "Schuldrechtlicher Ausgleich"),
    ]),
    ("§ 894", &[
        u("§ 892", "Gefahr des Rechtsverlusts durch unrichtiges Grundbuch"),
        d("§ 899", "Sicherung durch Widerspruch"),
    ]),
    ("§ 903", &[
        d("§ 985", "Herausgabeanspruch"),
        d("§ 1004", "Beseitigungs- und Unterlassungsanspruch"),
        d("§ 906", "Grenze: Duldung von Immissionen"),
    ]),
    ("§ 906", &[
        u("§ 903", "Befugnisse des Eigentümers"),
        u("§ 1004", "Duldungspflicht im Sinne von Abs. 2"),
    ]),
    ("§ 925", &[
        u("§ 873", "Einigung und Eintragung"),
        u("§ 311b", "Formbedürftiges Verpflichtungsgeschäft"),
    ]),
    ("§ 929", &[
        d("§ 930", "Übergabeersatz: Besitzkonstitut"),
        d("§ 931", "Übergabeersatz: Abtretung des Herausgabeanspruchs"),
        d("§ 932", "Erwerb vom Nichtberechtigten"),
    ]),
    ("§ 930", &[
        u("§ 929", "Einigung als Grundtatbestand"),
        u("§ 868", "Besitzmittlungsverhältnis"),
        d("§ 933", "Gutgläubiger Erwerb erst mit Übergabe"),
    ]),
    ("§ 931", &[
        u("§ 929", "Einigung als Grundtatbestand"),
        d("§ 934", "Gutgläubiger Erwerb"),
    ]),
    ("§ 932", &[
        u("§ 929", "Erwerbstatbestand im Übrigen"),
        d("§ 935", "Kein Erwerb abhandengekommener Sachen"),
        d("§ 936", "Erlöschen von Rechten Dritter"),
        d("§ 816", "Ausgleichsanspruch des früheren Eigentümers"),
    ]),
    ("§ 933", &[
        u("§ 930", "Übereignung durch Besitzkonstitut"),
        d("§ 935", "Kein Erwerb abhandengekommener Sachen"),
    ]),
    ("§ 934", &[
        u("§ 931", "Übereignung durch Abtretung des Herausgabeanspruchs"),
        d("§ 935", "Kein Erwerb abhandengekommener Sachen"),
    ]),
    ("§ 935", &[
        u("§ 932", "Gutgläubiger Erwerb, der hier ausgeschlossen wird"),
    ]),
    ("§ 946", &[
        d("§ 951", "Ausgleich für den Rechtsverlust"),
    ]),
    ("§ 950", &[
        d("§ 951", "Ausgleich für den Rechtsverlust"),
    ]),
    ("§ 951", &[
        u("§ 946", "Rechtsverlust durch Verbindung"),
        u("§ 950", "Rechtsverlust durch Verarbeitung"),
        d("§ 812", "Rechtsgrundverweisung auf das Bereicherungsrecht"),
    ]),
    ("§ 985", &[
        d("§ 986", "Einwendung: Recht zum Besitz"),
        d("§ 987", "Nutzungsherausgabe"),
        d("§ 989", "Schadensersatz"),
        d("§ 994", "Verwendungsersatz des Besitzers"),
    ]),
    ("§ 986", &[
        u("§ 985", "Herausgabeanspruch, dem das Besitzrecht entgegensteht"),
    ]),
    ("§ 987", &[
        u("§ 985", "Vindikationslage"),
        d("§ 990", "Gleichstellung des bösgläubigen Besitzers"),
        d("§ 993", "Sperrwirkung zugunsten des redlichen Besitzers"),
    ]),
    ("§ 989", &[
        u("§ 985", "Vindikationslage"),
        d("§ 990", "Gleichstellung des bösgläubigen Besitzers"),
        d("§ 992", "Deliktischer Besitzer"),
    ]),
    ("§ 990", &[
        u("§ 987", "Nutzungen ab Rechtshängigkeit"),
        u("§ 989", "Schadensersatz ab Rechtshängigkeit"),
        u("§ 932", "Abs. 2: Maßstab des guten Glaubens"),
    ]),
    ("§ 993", &[
        u("§ 987", "Haftung des verklagten Besitzers"),
    ]),
    ("§ 994", &[
        u("§ 985", "Vindikationslage"),
        d("§ 996", "Nützliche Verwendungen"),
        d("§ 1000", "Zurückbehaltungsrecht des Besitzers"),
        d("§ 1001", "Klage auf Verwendungsersatz"),
    ]),
    ("§ 1004", &[
        u("§ 903", "Eigentum als geschütztes Recht"),
        u("§ 985", "Abgrenzung: Entziehung oder Vorenthaltung des Besitzes"),
        d("§ 906", "Duldungspflicht bei Immissionen"),
    ]),
    ("§ 1113", &[
        d("§ 1147", "Befriedigung durch Zwangsvollstreckung"),
        d("§ 1153", "Akzessorietät bei der Übertragung"),
        d("§ 1163", "Eigentümerhypothek"),
    ]),
    ("§ 1147", &[
        u("§ 1113", "Hypothek als Verwertungsrecht"),
        dx("ZPO", "§ 794", "Duldungstitel, häufig vollstreckbare Urkunde (Abs. 1 Nr. 5)"),
        dx("ZVG", "§ 15", "Anordnung der Zwangsversteigerung"),
    ]),
    ("§ 1191", &[
        u("§ 1113", "Abgrenzung: akzessorische Hypothek"),
        d("§ 1192", "Anwendbare Vorschriften, Einreden aus dem Sicherungsvertrag"),
        d("§ 1147", "Befriedigung durch Zwangsvollstreckung"),
    ]),
    ("§ 1192", &[
        u("§ 1191", "Grundschuld"),
    ]),
    ("§ 1204", &[
        d("§ 1205", "Bestellung des Pfandrechts"),
        d("§ 1228", "Befriedigung durch Pfandverkauf"),
        d("§ 1252", "Erlöschen mit der Forderung"),
    ]),
    // --- Familienrecht ------------------------------------------------
    ("§ 1357", &[
        d("§ 421", "Mitverpflichtung des Ehegatten als Gesamtschuldner"),
    ]),
    ("§ 1363", &[
        d("§ 1365", "Verfügungsbeschränkung über das Vermögen im Ganzen"),
        d("§ 1371", "Ausgleich im Todesfall"),
        d("§ 1372", "Ausgleich in anderen Fällen"),
    ]),
    ("§ 1365", &[
        u("§ 1363", "Gesetzlicher Güterstand"),
        d("§ 1366", "Genehmigung von Verträgen"),
        d("§ 1368", "Geltendmachung der Unwirksamkeit durch den anderen Ehegatten"),
    ]),
    ("§ 1372", &[
        u("§ 1363", "Zugewinngemeinschaft"),
        d("§ 1373", "Begriff des Zugewinns"),
        d("§ 1378", "Ausgleichsforderung"),
    ]),
    ("§ 1378", &[
        u("§ 1373", "Zugewinn als Berechnungsgrundlage"),
    ]),
    ("§ 1564", &[
        d("§ 1565", "Scheitern der Ehe"),
    ]),
    ("§ 1565", &[
        u("§ 1564", "Scheidung durch richterliche Entscheidung"),
        d("§ 1566", "Vermutungen für das Scheitern"),
    ]),
    ("§ 1601", &[
        d("§ 1602", "Bedürftigkeit des Berechtigten"),
        d("§ 1603", "Leistungsfähigkeit des Verpflichteten"),
        d("§ 1610", "Maß des Unterhalts"),
    ]),
    ("§ 1626", &[
        d("§ 1629", "Vertretung des Kindes"),
    ]),
    ("§ 1629", &[
        u("§ 1626", "Elterliche Sorge"),
        u("§ 164", "Wirkung der Vertretung für und gegen das Kind"),
    ]),
    // --- Erbrecht -----------------------------------------------------
    ("§ 1922", &[
        d("§ 1924", "Gesetzliche Erbfolge"),
        d("§ 1937", "Gewillkürte Erbfolge"),
        d("§ 1942", "Anfall und Ausschlagung"),
        d("§ 1967", "Haftung für Nachlassverbindlichkeiten"),
    ]),
    ("§ 1924", &[
        u("§ 1922", "Erbfall"),
        d("§ 1925", "Erben zweiter Ordnung"),
        d("§ 1931", "Erbrecht des Ehegatten"),
    ]),
    ("§ 1931", &[
        u("§ 1924", "Verwandte, neben denen der Ehegatte erbt"),
        d("§ 1371", "Erhöhung des Erbteils bei Zugewinngemeinschaft"),
    ]),
    ("§ 1937", &[
        d("§ 2087", "Auslegung: Erbeinsetzung oder Vermächtnis"),
        d("§ 2247", "Eigenhändiges Testament"),
    ]),
    ("§ 1942", &[
        u("§ 1922", "Erbfall"),
        d("§ 1943", "Annahme der Erbschaft"),
        d("§ 1944", "Ausschlagungsfrist"),
    ]),
    ("§ 1944", &[
        u("§ 1942", "Ausschlagungsrecht"),
        d("§ 1945", "Form der Ausschlagung"),
        d("§ 1954", "Anfechtung von Annahme oder Ausschlagung"),
    ]),
    ("§ 1967", &[
        u("§ 1922", "Gesamtrechtsnachfolge"),
        d("§ 1975", "Beschränkung der Haftung auf den Nachlass"),
        d("§ 2058", "Gesamtschuldnerische Haftung der Miterben"),
    ]),
    ("§ 2032", &[
        d("§ 2033", "Verfügung über den Anteil"),
        d("§ 2038", "Gemeinschaftliche Verwaltung"),
        d("§ 2042", "Auseinandersetzung"),
    ]),
    ("§ 2247", &[
        u("§ 2229", "Testierfähigkeit"),
        d("§ 125", "Folge des Formmangels: Nichtigkeit"),
        d("§ 2253", "Widerruf des Testaments"),
    ]),
    ("§ 2303", &[
        d("§ 2311", "Wert des Nachlasses"),
        d("§ 2314", "Auskunftsanspruch gegen den Erben"),
        d("§ 2325", "Pflichtteilsergänzung bei Schenkungen"),
    ]),
    ("§ 2325", &[
        u("§ 2303", "Pflichtteilsberechtigung"),
        d("§ 2329", "Anspruch gegen den Beschenkten"),
    ]),
    ("§ 2353", &[
        d("§ 2365", "Vermutung der Richtigkeit"),
        d("§ 2366", "Öffentlicher Glaube des Erbscheins"),
    ]),
];

#[rustfmt::skip]
const ZPO: Table = &[
    // --- Zuständigkeit und Parteien -----------------------------------
    ("§ 1", &[
        dx("GVG", "§ 23", "Sachliche Zuständigkeit der Amtsgerichte"),
        dx("GVG", "§ 71", "Sachliche Zuständigkeit der Landgerichte"),
    ]),
    ("§ 12", &[
        d("§ 13", "Allgemeiner Gerichtsstand: Wohnsitz"),
        d("§ 17", "Allgemeiner Gerichtsstand juristischer Personen"),
        d("§ 29", "Besonderer Gerichtsstand des Erfüllungsorts"),
        d("§ 32", "Besonderer Gerichtsstand der unerlaubten Handlung"),
        d("§ 35", "Wahlrecht des Klägers"),
    ]),
    ("§ 13", &[
        u("§ 12", "Allgemeiner Gerichtsstand als Grundsatz"),
        dx("BGB", "§ 7", "Begriff des Wohnsitzes"),
    ]),
    ("§ 17", &[
        u("§ 12", "Allgemeiner Gerichtsstand als Grundsatz"),
    ]),
    ("§ 29", &[
        u("§ 12", "Neben dem allgemeinen Gerichtsstand"),
        dx("BGB", "§ 269", "Bestimmung des Erfüllungsorts"),
        d("§ 35", "Wahlrecht des Klägers"),
    ]),
    ("§ 32", &[
        u("§ 12", "Neben dem allgemeinen Gerichtsstand"),
        d("§ 35", "Wahlrecht des Klägers"),
    ]),
    ("§ 38", &[
        d("§ 39", "Zuständigkeit durch rügelose Einlassung"),
        d("§ 40", "Unzulässige Vereinbarungen"),
    ]),
    ("§ 39", &[
        u("§ 38", "Ausdrückliche Gerichtsstandsvereinbarung"),
        d("§ 504", "Vor dem Amtsgericht nur nach richterlichem Hinweis"),
    ]),
    ("§ 50", &[
        d("§ 51", "Prozessfähigkeit"),
        d("§ 56", "Prüfung von Amts wegen"),
    ]),
    ("§ 51", &[
        u("§ 50", "Parteifähigkeit"),
        d("§ 52", "Umfang der Prozessfähigkeit"),
        d("§ 56", "Prüfung von Amts wegen"),
    ]),
    ("§ 59", &[
        d("§ 60", "Streitgenossenschaft bei gleichartigen Ansprüchen"),
        d("§ 61", "Selbständigkeit der Streitgenossen"),
        d("§ 62", "Notwendige Streitgenossenschaft"),
    ]),
    ("§ 62", &[
        u("§ 59", "Einfache Streitgenossenschaft als Grundform"),
    ]),
    ("§ 66", &[
        d("§ 67", "Rechtsstellung des Nebenintervenienten"),
        d("§ 68", "Interventionswirkung"),
        d("§ 70", "Beitrittserklärung"),
    ]),
    ("§ 68", &[
        u("§ 66", "Beitritt als Nebenintervenient"),
        d("§ 74", "Erstreckung auf den Streitverkündungsempfänger"),
    ]),
    ("§ 72", &[
        d("§ 73", "Form der Streitverkündung"),
        d("§ 74", "Wirkung der Streitverkündung"),
    ]),
    ("§ 74", &[
        u("§ 72", "Zulässige Streitverkündung"),
        d("§ 68", "Interventionswirkung"),
        dx("BGB", "§ 204", "Abs. 1 Nr. 6: Hemmung der Verjährung"),
    ]),
    ("§ 78", &[
        d("§ 79", "Parteiprozess"),
        d("§ 85", "Zurechnung des Anwaltsverschuldens"),
    ]),
    ("§ 85", &[
        d("§ 233", "Bedeutung für die Wiedereinsetzung"),
    ]),
    // --- Kosten -------------------------------------------------------
    ("§ 91", &[
        d("§ 91a", "Kosten bei übereinstimmender Erledigung"),
        d("§ 92", "Teilweises Obsiegen"),
        d("§ 93", "Sofortiges Anerkenntnis"),
        d("§ 103", "Kostenfestsetzung"),
    ]),
    ("§ 91a", &[
        u("§ 91", "Grundsatz der Kostenlast"),
    ]),
    ("§ 92", &[
        u("§ 91", "Grundsatz der Kostenlast"),
    ]),
    ("§ 93", &[
        u("§ 91", "Grundsatz, von dem hier abgewichen wird"),
        u("§ 307", "Anerkenntnis"),
    ]),
    ("§ 97", &[
        u("§ 91", "Grundsatz der Kostenlast"),
    ]),
    ("§ 103", &[
        u("§ 91", "Kostengrundentscheidung"),
        d("§ 104", "Verfahren der Kostenfestsetzung"),
    ]),
    ("§ 114", &[
        d("§ 115", "Einsatz von Einkommen und Vermögen"),
        d("§ 117", "Antrag"),
        d("§ 121", "Beiordnung eines Rechtsanwalts"),
        d("§ 127", "Entscheidung und Rechtsmittel"),
    ]),
    // --- Verfahren im Allgemeinen -------------------------------------
    ("§ 128", &[
        d("§ 128a", "Videoverhandlung"),
    ]),
    ("§ 130", &[
        u("§ 129", "Vorbereitende Schriftsätze"),
        d("§ 130a", "Elektronisches Dokument"),
        d("§ 130d", "Nutzungspflicht für Rechtsanwälte"),
    ]),
    ("§ 130a", &[
        d("§ 130d", "Nutzungspflicht für Rechtsanwälte und Behörden"),
    ]),
    ("§ 130d", &[
        u("§ 130a", "Anforderungen an das elektronische Dokument"),
    ]),
    ("§ 138", &[
        d("§ 139", "Richterliche Hinweispflicht"),
        d("§ 288", "Abgrenzung: gerichtliches Geständnis"),
        d("§ 286", "Bestrittene Tatsachen bedürfen des Beweises"),
    ]),
    ("§ 139", &[
        u("§ 138", "Erklärungspflicht der Parteien"),
        d("§ 156", "Wiedereröffnung nach verspätetem Hinweis"),
    ]),
    ("§ 166", &[
        d("§ 167", "Rückwirkung der Zustellung"),
        d("§ 189", "Heilung von Zustellungsmängeln"),
    ]),
    ("§ 167", &[
        u("§ 166", "Zustellung"),
        u("§ 253", "Klageerhebung durch Zustellung"),
        dx("BGB", "§ 204", "Hemmung der Verjährung"),
    ]),
    ("§ 189", &[
        u("§ 166", "Zustellung"),
    ]),
    ("§ 222", &[
        dx("BGB", "§ 187", "Fristbeginn"),
        dx("BGB", "§ 188", "Fristende"),
    ]),
    ("§ 233", &[
        u("§ 85", "Abs. 2: Anwaltsverschulden steht eigenem gleich"),
        d("§ 234", "Wiedereinsetzungsfrist"),
        d("§ 236", "Antrag und Nachholung der Prozesshandlung"),
    ]),
    ("§ 234", &[
        u("§ 233", "Wiedereinsetzungsgrund"),
        d("§ 236", "Inhalt des Antrags"),
    ]),
    // --- Verfahren im ersten Rechtszug --------------------------------
    ("§ 253", &[
        u("§ 130", "Abs. 4: Inhalt der Schriftsätze"),
        d("§ 261", "Rechtshängigkeit"),
        d("§ 271", "Zustellung der Klageschrift"),
        d("§ 167", "Rückwirkung auf den Eingang"),
        dx("BGB", "§ 204", "Hemmung der Verjährung"),
    ]),
    ("§ 256", &[
        u("§ 253", "Allgemeine Anforderungen an die Klage"),
        d("§ 322", "Rechtskraft des Feststellungsurteils"),
    ]),
    ("§ 260", &[
        u("§ 253", "Klageerhebung"),
        d("§ 5", "Zusammenrechnung der Streitwerte"),
        d("§ 145", "Prozesstrennung"),
    ]),
    ("§ 261", &[
        u("§ 253", "Erhebung der Klage"),
        d("§ 262", "Materiellrechtliche Wirkungen"),
        d("§ 265", "Veräußerung der streitbefangenen Sache"),
    ]),
    ("§ 263", &[
        d("§ 264", "Keine Klageänderung"),
        d("§ 267", "Vermutete Einwilligung"),
    ]),
    ("§ 264", &[
        u("§ 263", "Grundsatz: Einwilligung oder Sachdienlichkeit"),
    ]),
    ("§ 265", &[
        d("§ 325", "Rechtskrafterstreckung auf den Rechtsnachfolger"),
        d("§ 727", "Titelumschreibung"),
    ]),
    ("§ 269", &[
        u("§ 253", "Erhobene Klage"),
        d("§ 91a", "Abgrenzung: Erledigungserklärung"),
    ]),
    ("§ 272", &[
        d("§ 275", "Früher erster Termin"),
        d("§ 276", "Schriftliches Vorverfahren"),
        d("§ 278", "Güteverhandlung"),
    ]),
    ("§ 276", &[
        u("§ 272", "Wahl der Verfahrensweise"),
        d("§ 277", "Klageerwiderung"),
        d("§ 331", "Abs. 3: Versäumnisurteil im schriftlichen Vorverfahren"),
    ]),
    ("§ 278", &[
        d("§ 278a", "Mediation"),
        d("§ 794", "Abs. 1 Nr. 1: Vergleich als Vollstreckungstitel"),
    ]),
    ("§ 282", &[
        d("§ 296", "Zurückweisung verspäteten Vorbringens"),
    ]),
    ("§ 284", &[
        d("§ 286", "Beweiswürdigung"),
        d("§ 355", "Unmittelbarkeit der Beweisaufnahme"),
    ]),
    ("§ 286", &[
        u("§ 138", "Beweisbedürftig ist nur Bestrittenes"),
        u("§ 284", "Beweisaufnahme"),
        d("§ 287", "Erleichterung bei Schaden und Forderungshöhe"),
    ]),
    ("§ 287", &[
        u("§ 286", "Regelbeweismaß"),
    ]),
    ("§ 288", &[
        u("§ 138", "Abgrenzung: Nichtbestreiten"),
        d("§ 290", "Widerruf des Geständnisses"),
    ]),
    ("§ 292", &[
        u("§ 286", "Freie Beweiswürdigung"),
    ]),
    ("§ 296", &[
        u("§ 282", "Prozessförderungspflicht"),
        u("§ 276", "Versäumte Erwiderungsfrist"),
        d("§ 531", "Fortwirkung in der Berufung"),
    ]),
    ("§ 300", &[
        d("§ 301", "Teilurteil"),
        d("§ 304", "Grundurteil"),
        d("§ 308", "Bindung an die Anträge"),
    ]),
    ("§ 301", &[
        u("§ 300", "Endurteil"),
    ]),
    ("§ 307", &[
        d("§ 93", "Kostenfolge bei sofortigem Anerkenntnis"),
        d("§ 708", "Nr. 1: vorläufig vollstreckbar ohne Sicherheit"),
    ]),
    ("§ 308", &[
        u("§ 253", "Antrag bestimmt den Streitgegenstand"),
    ]),
    ("§ 313", &[
        d("§ 313a", "Weglassen von Tatbestand und Gründen"),
        d("§ 313b", "Abgekürzte Form"),
    ]),
    ("§ 318", &[
        d("§ 319", "Berichtigung offenbarer Unrichtigkeiten"),
        d("§ 320", "Berichtigung des Tatbestands"),
        d("§ 321", "Ergänzung des Urteils"),
    ]),
    ("§ 319", &[
        u("§ 318", "Bindung des Gerichts"),
    ]),
    ("§ 320", &[
        u("§ 318", "Bindung des Gerichts"),
    ]),
    ("§ 321", &[
        u("§ 318", "Bindung des Gerichts"),
    ]),
    ("§ 321a", &[
        u("§ 511", "Subsidiär: nur wenn kein Rechtsmittel gegeben ist"),
    ]),
    ("§ 322", &[
        d("§ 323", "Durchbrechung: Abänderung"),
        d("§ 325", "Subjektive Grenzen der Rechtskraft"),
        d("§ 767", "Abs. 2: Präklusion bei der Vollstreckungsabwehrklage"),
    ]),
    ("§ 323", &[
        u("§ 322", "Materielle Rechtskraft"),
        d("§ 323a", "Abänderung von Vergleichen und Urkunden"),
    ]),
    ("§ 325", &[
        u("§ 322", "Materielle Rechtskraft"),
        u("§ 265", "Veräußerung während des Prozesses"),
        d("§ 727", "Vollstreckbare Ausfertigung für und gegen Rechtsnachfolger"),
    ]),
    ("§ 330", &[
        d("§ 338", "Einspruch"),
        d("§ 708", "Nr. 2: vorläufig vollstreckbar ohne Sicherheit"),
    ]),
    ("§ 331", &[
        u("§ 276", "Versäumte Verteidigungsanzeige"),
        d("§ 335", "Unzulässigkeit der Versäumnisentscheidung"),
        d("§ 337", "Vertagung von Amts wegen"),
        d("§ 338", "Einspruch"),
    ]),
    ("§ 338", &[
        u("§ 331", "Versäumnisurteil"),
        d("§ 339", "Einspruchsfrist"),
        d("§ 340", "Einspruchsschrift"),
        d("§ 342", "Wirkung des zulässigen Einspruchs"),
    ]),
    ("§ 339", &[
        u("§ 338", "Einspruch"),
        d("§ 233", "Wiedereinsetzung bei Versäumung der Notfrist"),
    ]),
    ("§ 342", &[
        u("§ 338", "Zulässiger Einspruch"),
        d("§ 343", "Entscheidung nach Einspruch"),
    ]),
    ("§ 345", &[
        u("§ 338", "Einspruch"),
        d("§ 514", "Abs. 2: eingeschränkte Berufung"),
    ]),
    ("§ 348", &[
        d("§ 348a", "Obligatorischer Einzelrichter"),
    ]),
    ("§ 355", &[
        u("§ 284", "Beweisaufnahme"),
    ]),
    ("§ 373", &[
        d("§ 377", "Ladung der Zeugen"),
        d("§ 383", "Zeugnisverweigerung aus persönlichen Gründen"),
        d("§ 384", "Zeugnisverweigerung aus sachlichen Gründen"),
    ]),
    ("§ 383", &[
        u("§ 373", "Zeugenbeweis"),
        d("§ 384", "Zeugnisverweigerung aus sachlichen Gründen"),
    ]),
    ("§ 402", &[
        d("§ 404", "Auswahl des Sachverständigen"),
        d("§ 406", "Ablehnung des Sachverständigen"),
        d("§ 411", "Schriftliches Gutachten"),
    ]),
    ("§ 415", &[
        d("§ 416", "Privaturkunden"),
        d("§ 418", "Öffentliche Urkunden mit anderem Inhalt"),
    ]),
    ("§ 416", &[
        u("§ 415", "Öffentliche Urkunden"),
        d("§ 440", "Beweis der Echtheit"),
    ]),
    ("§ 445", &[
        d("§ 447", "Vernehmung der beweispflichtigen Partei"),
        d("§ 448", "Vernehmung von Amts wegen"),
    ]),
    ("§ 485", &[
        d("§ 493", "Verwertung im Prozess"),
        d("§ 494a", "Frist zur Klageerhebung"),
    ]),
    // --- Rechtsmittel -------------------------------------------------
    ("§ 511", &[
        d("§ 513", "Berufungsgründe"),
        d("§ 517", "Berufungsfrist"),
        d("§ 519", "Berufungsschrift"),
        d("§ 520", "Berufungsbegründung"),
    ]),
    ("§ 513", &[
        u("§ 511", "Statthaftigkeit der Berufung"),
        d("§ 529", "Prüfungsumfang des Berufungsgerichts"),
        d("§ 546", "Begriff der Rechtsverletzung"),
    ]),
    ("§ 517", &[
        u("§ 511", "Statthaftigkeit der Berufung"),
        d("§ 233", "Wiedereinsetzung bei Versäumung der Notfrist"),
    ]),
    ("§ 519", &[
        u("§ 511", "Statthaftigkeit der Berufung"),
        d("§ 520", "Berufungsbegründung"),
    ]),
    ("§ 520", &[
        u("§ 519", "Einlegung der Berufung"),
        d("§ 522", "Zulässigkeitsprüfung und Zurückweisungsbeschluss"),
    ]),
    ("§ 522", &[
        u("§ 520", "Berufungsbegründung"),
    ]),
    ("§ 529", &[
        u("§ 513", "Berufungsgründe"),
        d("§ 531", "Neue Angriffs- und Verteidigungsmittel"),
    ]),
    ("§ 531", &[
        u("§ 296", "Zurückweisung im ersten Rechtszug"),
        u("§ 529", "Prüfungsumfang"),
    ]),
    ("§ 542", &[
        d("§ 543", "Zulassungsrevision"),
        d("§ 544", "Nichtzulassungsbeschwerde"),
        d("§ 545", "Revisionsgründe"),
    ]),
    ("§ 543", &[
        u("§ 542", "Statthaftigkeit der Revision"),
        d("§ 544", "Nichtzulassungsbeschwerde"),
    ]),
    ("§ 544", &[
        u("§ 543", "Zulassungsgründe"),
    ]),
    ("§ 545", &[
        u("§ 542", "Statthaftigkeit der Revision"),
        d("§ 546", "Begriff der Rechtsverletzung"),
        d("§ 547", "Absolute Revisionsgründe"),
    ]),
    ("§ 567", &[
        d("§ 569", "Frist und Form"),
        d("§ 572", "Abhilfe und Vorlage"),
        d("§ 574", "Rechtsbeschwerde"),
    ]),
    ("§ 569", &[
        u("§ 567", "Statthaftigkeit der sofortigen Beschwerde"),
    ]),
    ("§ 574", &[
        u("§ 567", "Sofortige Beschwerde"),
        d("§ 575", "Frist, Form und Begründung"),
    ]),
    // --- Besondere Verfahrensarten ------------------------------------
    ("§ 592", &[
        d("§ 595", "Beschränkung der Beweismittel"),
        d("§ 599", "Vorbehaltsurteil"),
        d("§ 600", "Nachverfahren"),
    ]),
    ("§ 599", &[
        u("§ 592", "Urkundenprozess"),
        d("§ 600", "Nachverfahren"),
        d("§ 708", "Nr. 4: vorläufig vollstreckbar ohne Sicherheit"),
    ]),
    ("§ 688", &[
        d("§ 690", "Mahnantrag"),
        d("§ 692", "Mahnbescheid"),
        d("§ 694", "Widerspruch"),
        d("§ 699", "Vollstreckungsbescheid"),
    ]),
    ("§ 694", &[
        u("§ 688", "Mahnverfahren"),
        d("§ 696", "Überleitung in das streitige Verfahren"),
    ]),
    ("§ 699", &[
        u("§ 688", "Mahnverfahren"),
        d("§ 700", "Einspruch gegen den Vollstreckungsbescheid"),
    ]),
    ("§ 700", &[
        u("§ 699", "Vollstreckungsbescheid"),
        d("§ 338", "Einspruch wie gegen ein Versäumnisurteil"),
        d("§ 339", "Einspruchsfrist"),
    ]),
    // --- Zwangsvollstreckung ------------------------------------------
    ("§ 704", &[
        d("§ 708", "Vorläufige Vollstreckbarkeit"),
        d("§ 724", "Vollstreckungsklausel"),
        d("§ 750", "Zustellung vor Beginn der Vollstreckung"),
        d("§ 794", "Weitere Vollstreckungstitel"),
    ]),
    ("§ 707", &[
        d("§ 719", "Einstellung bei Rechtsmittel und Einspruch"),
    ]),
    ("§ 708", &[
        u("§ 704", "Vollstreckung aus Endurteilen"),
        d("§ 709", "Im Übrigen: nur gegen Sicherheitsleistung"),
        d("§ 711", "Abwendungsbefugnis des Schuldners"),
        d("§ 713", "Unterbleiben von Schutzanordnungen"),
    ]),
    ("§ 709", &[
        u("§ 704", "Vollstreckung aus Endurteilen"),
        u("§ 708", "Vorrangig: Fälle ohne Sicherheitsleistung"),
        d("§ 108", "Art und Höhe der Sicherheit"),
    ]),
    ("§ 711", &[
        u("§ 708", "Fälle der Nummern 4 bis 11"),
        d("§ 713", "Unterbleiben von Schutzanordnungen"),
    ]),
    ("§ 717", &[
        u("§ 708", "Vollstreckung aus vorläufig vollstreckbarem Urteil"),
        u("§ 709", "Vollstreckung gegen Sicherheitsleistung"),
    ]),
    ("§ 719", &[
        u("§ 707", "Voraussetzungen der einstweiligen Einstellung"),
    ]),
    ("§ 724", &[
        u("§ 704", "Vollstreckungstitel"),
        d("§ 725", "Wortlaut der Klausel"),
        d("§ 726", "Titel mit Bedingung"),
        d("§ 727", "Klausel bei Rechtsnachfolge"),
    ]),
    ("§ 726", &[
        u("§ 724", "Einfache Klausel"),
        d("§ 731", "Klage auf Erteilung der Klausel"),
    ]),
    ("§ 727", &[
        u("§ 724", "Einfache Klausel"),
        u("§ 325", "Rechtskrafterstreckung"),
        d("§ 731", "Klage auf Erteilung der Klausel"),
    ]),
    ("§ 732", &[
        u("§ 724", "Erteilte Klausel"),
        d("§ 768", "Abgrenzung: Klage gegen die Klausel"),
    ]),
    ("§ 750", &[
        u("§ 704", "Titel"),
        u("§ 724", "Klausel"),
        d("§ 751", "Besondere Vollstreckungsvoraussetzungen"),
    ]),
    ("§ 753", &[
        d("§ 754", "Vollstreckungsauftrag"),
        d("§ 766", "Rechtsbehelf gegen das Vorgehen des Gerichtsvollziehers"),
    ]),
    ("§ 765a", &[
        u("§ 766", "Abgrenzung: Rüge von Verfahrensfehlern"),
    ]),
    ("§ 766", &[
        d("§ 793", "Sofortige Beschwerde gegen Entscheidungen"),
        d("§ 767", "Abgrenzung: materielle Einwendungen gegen den Anspruch"),
        d("§ 771", "Abgrenzung: Rechte Dritter"),
    ]),
    ("§ 767", &[
        u("§ 704", "Titel, dessen Vollstreckbarkeit beseitigt werden soll"),
        d("§ 769", "Einstweilige Anordnungen"),
        d("§ 797", "Abs. 4: keine Präklusion bei vollstreckbaren Urkunden"),
        d("§ 771", "Abgrenzung: Drittwiderspruchsklage"),
    ]),
    ("§ 768", &[
        u("§ 726", "Qualifizierte Klausel"),
        u("§ 732", "Abgrenzung: Klauselerinnerung"),
    ]),
    ("§ 769", &[
        u("§ 767", "Vollstreckungsabwehrklage"),
        u("§ 771", "Drittwiderspruchsklage (Abs. 3)"),
    ]),
    ("§ 771", &[
        u("§ 766", "Abgrenzung: Erinnerung bei Verfahrensfehlern"),
        d("§ 769", "Einstweilige Anordnungen (Abs. 3)"),
        d("§ 805", "Abgrenzung: Klage auf vorzugsweise Befriedigung"),
    ]),
    ("§ 793", &[
        u("§ 766", "Abgrenzung: Erinnerung gegen Vollstreckungsmaßnahmen"),
        d("§ 567", "Verfahren der sofortigen Beschwerde"),
    ]),
    ("§ 794", &[
        u("§ 704", "Endurteile als Grundfall"),
        d("§ 795", "Anwendung der allgemeinen Vorschriften"),
        d("§ 797", "Verfahren bei vollstreckbaren Urkunden"),
        d("§ 800", "Unterwerfung mit Wirkung gegen den jeweiligen Eigentümer"),
    ]),
    ("§ 795", &[
        u("§ 794", "Weitere Vollstreckungstitel"),
        d("§ 724", "Klauselerfordernis"),
    ]),
    ("§ 797", &[
        u("§ 794", "Vollstreckbare Urkunde (Abs. 1 Nr. 5)"),
    ]),
    ("§ 802c", &[
        d("§ 802f", "Verfahren zur Abnahme der Vermögensauskunft"),
        d("§ 802g", "Erzwingungshaft"),
    ]),
    ("§ 802g", &[
        u("§ 802c", "Pflicht zur Vermögensauskunft"),
    ]),
    ("§ 803", &[
        d("§ 804", "Pfändungspfandrecht"),
        d("§ 808", "Pfändung beweglicher Sachen"),
        d("§ 811", "Unpfändbare Sachen"),
        d("§ 829", "Pfändung von Geldforderungen"),
    ]),
    ("§ 804", &[
        u("§ 803", "Wirksame Pfändung"),
        d("§ 805", "Vorzugsweise Befriedigung besserrangiger Gläubiger"),
    ]),
    ("§ 805", &[
        u("§ 804", "Rangverhältnis der Pfandrechte"),
        u("§ 771", "Abgrenzung: Drittwiderspruchsklage"),
    ]),
    ("§ 808", &[
        u("§ 803", "Pfändung als Vollstreckungsakt"),
        d("§ 809", "Sachen im Gewahrsam Dritter"),
        d("§ 811", "Unpfändbare Sachen"),
        d("§ 814", "Verwertung durch Versteigerung"),
    ]),
    ("§ 809", &[
        u("§ 808", "Gewahrsam des Schuldners als Grundfall"),
        d("§ 771", "Schutz des Dritten bei eigenem Recht"),
    ]),
    ("§ 811", &[
        u("§ 808", "Sachpfändung"),
        d("§ 766", "Rechtsbehelf bei Verstoß"),
    ]),
    ("§ 814", &[
        u("§ 808", "Gepfändete Sache"),
        d("§ 817", "Zuschlag und Ablieferung"),
    ]),
    ("§ 828", &[
        d("§ 829", "Pfändungsbeschluss"),
    ]),
    ("§ 829", &[
        u("§ 828", "Zuständiges Vollstreckungsgericht"),
        d("§ 835", "Überweisung der Forderung"),
        d("§ 840", "Erklärungspflicht des Drittschuldners"),
        d("§ 850", "Pfändungsschutz für Arbeitseinkommen"),
    ]),
    ("§ 835", &[
        u("§ 829", "Wirksame Pfändung"),
        d("§ 836", "Wirkung der Überweisung"),
    ]),
    ("§ 840", &[
        u("§ 829", "Zustellung des Pfändungsbeschlusses"),
    ]),
    ("§ 850", &[
        d("§ 850c", "Pfändungsgrenzen"),
        d("§ 850d", "Erweiterte Pfändbarkeit bei Unterhaltsansprüchen"),
        d("§ 850k", "Pfändungsschutzkonto"),
    ]),
    ("§ 850c", &[
        u("§ 850", "Arbeitseinkommen"),
        d("§ 850d", "Ausnahme: Unterhaltsgläubiger"),
        d("§ 850f", "Änderung des unpfändbaren Betrags"),
    ]),
    ("§ 850k", &[
        u("§ 850c", "Pfändungsfreibeträge als Maßstab"),
        d("§ 899", "Pfändungsfreier Betrag auf dem Konto"),
    ]),
    ("§ 857", &[
        u("§ 829", "Entsprechende Anwendung der Forderungspfändung"),
    ]),
    ("§ 864", &[
        d("§ 866", "Arten der Immobiliarvollstreckung"),
        d("§ 867", "Zwangshypothek"),
        d("§ 869", "Zwangsversteigerung und Zwangsverwaltung"),
    ]),
    ("§ 866", &[
        u("§ 864", "Gegenstand der Immobiliarvollstreckung"),
        d("§ 867", "Zwangshypothek"),
        dx("ZVG", "§ 15", "Anordnung der Zwangsversteigerung"),
    ]),
    ("§ 867", &[
        u("§ 866", "Arten der Immobiliarvollstreckung"),
        dx("BGB", "§ 1147", "Befriedigung aus dem Grundstück"),
    ]),
    ("§ 869", &[
        dx("ZVG", "§ 1", "Regelung im Zwangsversteigerungsgesetz"),
    ]),
    ("§ 883", &[
        d("§ 885", "Herausgabe und Räumung von Grundstücken"),
        d("§ 886", "Sache im Gewahrsam eines Dritten"),
    ]),
    ("§ 885", &[
        d("§ 885a", "Beschränkter Vollstreckungsauftrag"),
        d("§ 721", "Räumungsfrist"),
        d("§ 765a", "Vollstreckungsschutz in Härtefällen"),
    ]),
    ("§ 887", &[
        d("§ 888", "Abgrenzung: nicht vertretbare Handlungen"),
        d("§ 891", "Verfahren und Anhörung"),
    ]),
    ("§ 888", &[
        u("§ 887", "Abgrenzung: vertretbare Handlungen"),
        d("§ 890", "Abgrenzung: Unterlassen und Dulden"),
    ]),
    ("§ 890", &[
        u("§ 888", "Abgrenzung: Vornahme einer Handlung"),
        d("§ 891", "Verfahren und Anhörung"),
    ]),
    ("§ 894", &[
        u("§ 704", "Rechtskraft des Urteils als Voraussetzung"),
    ]),
    // --- Arrest und einstweilige Verfügung ----------------------------
    ("§ 916", &[
        d("§ 917", "Arrestgrund"),
        d("§ 919", "Arrestgericht"),
        d("§ 920", "Arrestgesuch"),
        d("§ 935", "Abgrenzung: einstweilige Verfügung"),
    ]),
    ("§ 917", &[
        u("§ 916", "Arrestanspruch"),
    ]),
    ("§ 920", &[
        u("§ 916", "Arrestanspruch"),
        d("§ 294", "Glaubhaftmachung"),
        d("§ 922", "Entscheidung über das Gesuch"),
    ]),
    ("§ 922", &[
        u("§ 920", "Arrestgesuch"),
        d("§ 924", "Widerspruch gegen den Beschluss"),
        d("§ 926", "Anordnung der Klageerhebung"),
    ]),
    ("§ 924", &[
        u("§ 922", "Arrestbeschluss"),
        d("§ 925", "Entscheidung nach Widerspruch"),
    ]),
    ("§ 926", &[
        u("§ 922", "Arrestbefehl"),
    ]),
    ("§ 929", &[
        u("§ 922", "Arrestbefehl"),
        d("§ 930", "Vollziehung in bewegliches Vermögen"),
    ]),
    ("§ 935", &[
        u("§ 916", "Abgrenzung: Arrest wegen Geldforderungen"),
        d("§ 936", "Anwendung der Arrestvorschriften"),
        d("§ 938", "Inhalt der einstweiligen Verfügung"),
        d("§ 940", "Regelungsverfügung"),
    ]),
    ("§ 936", &[
        u("§ 935", "Einstweilige Verfügung"),
        d("§ 920", "Gesuch und Glaubhaftmachung"),
    ]),
    ("§ 940", &[
        u("§ 935", "Sicherungsverfügung"),
    ]),
    ("§ 945", &[
        u("§ 916", "Arrest"),
        u("§ 935", "Einstweilige Verfügung"),
        dx("BGB", "§ 249", "Art und Umfang des Schadensersatzes"),
    ]),
    ("§ 1029", &[
        d("§ 1032", "Einrede der Schiedsvereinbarung"),
    ]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_found_per_law() {
        let hints = hints_for("BGB", "§ 437");
        assert!(hints.iter().any(|h| h.norm == "§ 439"));
        assert!(hints_for("ZPO", "§ 437").is_empty());
        assert!(hints_for("StGB", "§ 242").is_empty());
        assert!(has_hints("BGB") && has_hints("ZPO") && !has_hints("StGB"));
        let cross = hints_for("BGB", "§ 204");
        let zpo = cross.iter().find(|h| h.law == "ZPO").expect("ZPO-Hinweis");
        assert_eq!(zpo.label(), "§ 253 ZPO");
        assert_eq!(zpo.target().law, LawRef::Abbrev("ZPO".into()));
    }

    /// Jede Norm steht nur einmal in der Tabelle, kein Hinweis zeigt auf
    /// die eigene Norm, und keine Norm trägt mehr Pfeile, als die Randspalte
    /// neben einer kurzen Norm sinnvoll zeigen kann.
    #[test]
    fn tables_are_consistent() {
        for (table, law) in [(BGB, "BGB"), (ZPO, "ZPO")] {
            let mut seen = std::collections::HashSet::new();
            for (norm, hints) in table {
                assert!(seen.insert(*norm), "{law} {norm} doppelt");
                assert!(!hints.is_empty(), "{law} {norm} ohne Hinweise");
                assert!(hints.len() <= 6, "{law} {norm} hat zu viele Hinweise");
                for hint in *hints {
                    assert!(
                        !(hint.law.is_empty() && hint.norm == *norm),
                        "{law} {norm} zeigt auf sich selbst"
                    );
                    assert!(hint.law != law, "{law} {norm}: eigenes Kürzel angegeben");
                    assert!(!hint.reason.is_empty());
                    assert!(hint.norm.starts_with("§ ") || hint.norm.starts_with("Art. "));
                }
            }
        }
    }

    /// Ziele im selben Gesetz, zu denen es selbst einen Eintrag gibt,
    /// sollen möglichst zurückverweisen; hier nur: Up-Pfeile stehen vor
    /// Down-Pfeilen, damit die Randspalte von oben nach unten lesbar ist.
    #[test]
    fn up_before_down() {
        for table in [BGB, ZPO] {
            for (norm, hints) in table {
                let first_down = hints
                    .iter()
                    .position(|h| h.direction == Direction::Down)
                    .unwrap_or(hints.len());
                assert!(
                    hints[first_down..]
                        .iter()
                        .all(|h| h.direction == Direction::Down),
                    "{norm}: ↗ nach ↘"
                );
            }
        }
    }
}
