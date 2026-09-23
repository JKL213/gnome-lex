// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Erkennung von Normverweisen im Gesetzestext.
//!
//! Erkannt werden u. a. „§ 280 Abs. 1“, „§§ 434 bis 437“, „§§ 28, 31a Abs. 1
//! Satz 2“, „Artikel 229 EGBGB“, „Art. 229 § 34 BGBEG“ sowie „Absatz 3“ als
//! Verweis innerhalb derselben Norm. Offsets sind Zeichenoffsets (Unicode-
//! Skalare), wie sie `GtkTextBuffer` verwendet.
// Wird ab den folgenden Stufen von der Oberfläche genutzt.
#![allow(dead_code)]

use std::sync::OnceLock;

use regex::Regex;

/// Auf welches Gesetz sich ein Verweis bezieht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawRef {
    /// Dasselbe Gesetz wie der Text, in dem der Verweis steht.
    Same,
    /// Ein anderes Gesetz, bezeichnet durch seine Abkürzung (z. B. „EGBGB“).
    Abbrev(String),
    /// Ein anderes, nicht per Abkürzung benanntes Gesetz („des Gesetzes über …“).
    Unknown,
}

/// Ziel eines Verweises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormRef {
    pub law: LawRef,
    /// Bezeichnung der Norm, z. B. „§ 280“ oder „Art. 229“; `None` = dieselbe Norm.
    pub norm: Option<String>,
    /// Bei Artikeln mit innerer Paragraphenzählung („Art. 229 § 34“).
    pub sub_section: Option<String>,
    pub paragraph: Option<String>,
    pub sentence: Option<String>,
    pub number: Option<String>,
}

impl NormRef {
    /// Kurzform für Anzeige und Tag-Namen.
    pub fn label(&self) -> String {
        let mut s = String::new();
        if let Some(n) = &self.norm {
            s.push_str(n);
        } else {
            s.push_str("diese Norm");
        }
        if let Some(sub) = &self.sub_section {
            s.push(' ');
            s.push_str(sub);
        }
        if let Some(p) = &self.paragraph {
            s.push_str(&format!(" Abs. {p}"));
        }
        if let Some(n) = &self.sentence {
            s.push_str(&format!(" Satz {n}"));
        }
        if let Some(n) = &self.number {
            s.push_str(&format!(" Nr. {n}"));
        }
        if let LawRef::Abbrev(a) = &self.law {
            s.push(' ');
            s.push_str(a);
        }
        s
    }

    /// Kandidaten für die `enbez`-Suche in der Datenbank, in Präferenzreihenfolge.
    pub fn enbez_candidates(&self) -> Vec<String> {
        let Some(norm) = &self.norm else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(sub) = &self.sub_section {
            out.push(format!("{norm} {sub}"));
            if let Some(stripped) = norm.strip_prefix("Art. ") {
                out.push(format!("Art {stripped} {sub}"));
            }
        }
        out.push(norm.clone());
        if let Some(stripped) = norm.strip_prefix("Art. ") {
            out.push(format!("Art {stripped}"));
            out.push(format!("Artikel {stripped}"));
        }
        out
    }
}

/// Ein erkannter Verweis mit seiner Position im Text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Zeichenoffset des ersten Zeichens.
    pub start: usize,
    /// Zeichenoffset hinter dem letzten Zeichen.
    pub end: usize,
    pub target: NormRef,
}

fn sign_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"§§?|\bArtikel\b|\bArt\.").expect("regex"))
}

fn abs_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\bAbs(?:atz|ätze|\.)").expect("regex"))
}

/// Erkennt „Absatz 3“ / „Abs. 2 Satz 1“ ohne vorangestelltes „§“.
fn bare_abs_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bAbs(?:atz|\.)\s*(\d+[a-z]?)").expect("regex"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Qualifier {
    Paragraph,
    Sentence,
    Number,
    Other,
}

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn skip_ws(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.text.len() - trimmed.len();
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Liest eine Norm- oder Absatznummer wie „280“, „31a“, „312g“.
    fn number(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let mut end = digits;
        let suffix = rest[digits..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase())
            .count();
        if suffix <= 2 {
            // „31a“, aber nicht „5und“ (Suffix muss vom nächsten Wort getrennt sein).
            let after = rest[digits + suffix..].chars().next();
            let boundary = after.is_none_or(|c| !c.is_alphanumeric());
            if boundary {
                end += suffix;
            } else if suffix > 0 {
                return None;
            }
        } else {
            return None;
        }
        let after = rest[end..].chars().next();
        if after.is_some_and(|c| c.is_alphanumeric()) {
            return None;
        }
        let s = &rest[..end];
        self.pos += end;
        Some(s)
    }

    fn qualifier(&mut self) -> Option<Qualifier> {
        let rest = self.rest();
        let table: &[(&str, Qualifier)] = &[
            ("Absätze", Qualifier::Paragraph),
            ("Absatz", Qualifier::Paragraph),
            ("Abs.", Qualifier::Paragraph),
            ("Sätze", Qualifier::Sentence),
            ("Satz", Qualifier::Sentence),
            ("S.", Qualifier::Sentence),
            ("Nummern", Qualifier::Number),
            ("Nummer", Qualifier::Number),
            ("Nrn.", Qualifier::Number),
            ("Nr.", Qualifier::Number),
            ("Halbsatz", Qualifier::Other),
            ("Buchstabe", Qualifier::Other),
            ("Buchst.", Qualifier::Other),
        ];
        for (word, q) in table {
            if let Some(after) = rest.strip_prefix(word) {
                let boundary = after.chars().next().is_none_or(|c| !c.is_alphabetic());
                if boundary {
                    self.pos += word.len();
                    return Some(*q);
                }
            }
        }
        None
    }
}

/// Liest „Zahl“, „Zahl und Zahl“, „Zahl, Zahl“, „Zahl bis Zahl“ für einen
/// Qualifier; gibt den ersten Wert zurück und bewegt den Cursor hinter alles.
fn qualifier_values(c: &mut Cursor<'_>) -> Option<String> {
    c.skip_ws();
    let first = c.number()?.to_owned();
    loop {
        let save = c.pos;
        c.skip_ws();
        let connector =
            c.eat(",") || c.eat("und") || c.eat("oder") || c.eat("bis") || c.eat("sowie");
        if !connector {
            c.pos = save;
            break;
        }
        c.skip_ws();
        if c.number().is_none() {
            c.pos = save;
            break;
        }
    }
    Some(first)
}

/// Parst die Qualifier („Abs. 1 Satz 2 Nr. 3“) nach einer Normnummer.
fn qualifiers(c: &mut Cursor<'_>, target: &mut NormRef) {
    loop {
        let save = c.pos;
        c.skip_ws();
        let Some(q) = c.qualifier() else {
            c.pos = save;
            return;
        };
        let Some(value) = qualifier_values(c) else {
            c.pos = save;
            return;
        };
        match q {
            Qualifier::Paragraph => target.paragraph.get_or_insert(value),
            Qualifier::Sentence => target.sentence.get_or_insert(value),
            Qualifier::Number => target.number.get_or_insert(value),
            Qualifier::Other => continue,
        };
    }
}

/// Erkennt eine Gesetzesabkürzung wie „EGBGB“, „VVG“, „KAGB“, „StGB“.
fn is_law_abbrev(token: &str) -> bool {
    let len = token.chars().count();
    if !(2..=12).contains(&len) {
        return false;
    }
    if !token.chars().next().is_some_and(|c| c.is_uppercase()) {
        return false;
    }
    if !token.chars().all(|c| c.is_alphabetic()) {
        return false;
    }
    let upper = token.chars().filter(|c| c.is_uppercase()).count();
    const STOP: &[&str] = &[
        "Abs",
        "Satz",
        "Nr",
        "Nummer",
        "Absatz",
        "Halbsatz",
        "Buchstabe",
    ];
    upper >= 2 && !STOP.contains(&token)
}

/// Prüft, ob nach `pos` eine Gesetzesangabe folgt; liefert deren Ende und Wert.
fn law_suffix(text: &str, pos: usize, current_law: &str) -> Option<(usize, LawRef)> {
    let mut c = Cursor { text, pos };
    c.skip_ws();
    if c.pos == pos {
        return None;
    }
    // „des Gesetzes über …“, „der Verordnung …“ → unbekanntes Gesetz
    let save = c.pos;
    let article = c.eat("des") || c.eat("der");
    if article {
        c.skip_ws();
        if c.rest().starts_with("Gesetzes")
            || c.rest().starts_with("Verordnung")
            || c.rest().starts_with("Bürgerlichen")
        {
            if c.rest().starts_with("Bürgerlichen Gesetzbuchs") {
                let end = c.pos + "Bürgerlichen Gesetzbuchs".len();
                let law = if current_law.eq_ignore_ascii_case("BGB") {
                    LawRef::Same
                } else {
                    LawRef::Abbrev("BGB".into())
                };
                return Some((end, law));
            }
            return Some((save, LawRef::Unknown));
        }
    }
    let token: String = c
        .rest()
        .chars()
        .take_while(|ch| ch.is_alphabetic())
        .collect();
    if !is_law_abbrev(&token) {
        return None;
    }
    let end = c.pos + token.len();
    if text[end..]
        .chars()
        .next()
        .is_some_and(|ch| ch.is_alphanumeric())
    {
        return None;
    }
    let law = if token.eq_ignore_ascii_case(current_law) {
        LawRef::Same
    } else {
        LawRef::Abbrev(token)
    };
    Some((end, law))
}

/// Findet alle Normverweise in `text`. `current_law` ist die Abkürzung des
/// Gesetzes, zu dem der Text gehört (z. B. „BGB“).
pub fn find_references(text: &str, current_law: &str) -> Vec<Reference> {
    let mut raw: Vec<(usize, usize, NormRef)> = Vec::new();
    let mut consumed_until = 0usize;

    for m in sign_regex().find_iter(text) {
        if m.start() < consumed_until {
            continue;
        }
        let sign = m.as_str();
        let is_article = sign.starts_with("Art");
        let mut c = Cursor { text, pos: m.end() };
        let mut items: Vec<(usize, usize, NormRef)> = Vec::new();

        loop {
            c.skip_ws();
            let start = c.pos;
            let Some(num) = c.number() else { break };
            let norm = if is_article {
                format!("Art. {num}")
            } else {
                format!("§ {num}")
            };
            let mut target = NormRef {
                law: LawRef::Same,
                norm: Some(norm),
                sub_section: None,
                paragraph: None,
                sentence: None,
                number: None,
            };
            // „Art. 229 § 34“
            if is_article {
                let save = c.pos;
                c.skip_ws();
                if c.eat("§") {
                    c.skip_ws();
                    if let Some(sub) = c.number() {
                        target.sub_section = Some(format!("§ {sub}"));
                    } else {
                        c.pos = save;
                    }
                } else {
                    c.pos = save;
                }
            }
            let before_qual = c.pos;
            qualifiers(&mut c, &mut target);
            let had_qualifier = c.pos != before_qual;
            items.push((start, c.pos, target));

            // Weitere Normen derselben Aufzählung: „, 31a“, „ und 38“, „ bis 437“
            let save = c.pos;
            c.skip_ws();
            let connector =
                c.eat(",") || c.eat("und") || c.eat("oder") || c.eat("bis") || c.eat("sowie");
            if !connector {
                c.pos = save;
                break;
            }
            c.skip_ws();
            // Nach einem Qualifier gehören nachfolgende Zahlen bereits zum
            // Qualifier (wurden dort konsumiert); hier zählt nur eine echte
            // Normnummer, ggf. mit erneutem „§“.
            let _ = c.eat("§§") || c.eat("§");
            c.skip_ws();
            let peek = Cursor { text, pos: c.pos }.number().is_some();
            if !peek || (had_qualifier && sign == "§") {
                c.pos = save;
                break;
            }
        }

        if items.is_empty() {
            continue;
        }
        let end = items.last().map(|i| i.1).unwrap_or(c.pos);
        let mut law = LawRef::Same;
        let mut consumed = end;
        if let Some((law_end, l)) = law_suffix(text, end, current_law) {
            law = l;
            consumed = law_end;
        }
        for (s, e, mut t) in items {
            t.law = law.clone();
            raw.push((s, e, t));
        }
        consumed_until = consumed;
    }

    // Alleinstehende Absatzverweise („Absatz 3“, „Abs. 2 Satz 1“) innerhalb derselben Norm.
    let mut occupied: Vec<(usize, usize)> = raw.iter().map(|r| (r.0, r.1)).collect();
    occupied.sort_unstable();
    for m in bare_abs_regex().find_iter(text) {
        if occupied.iter().any(|(s, e)| m.start() < *e && m.end() > *s) {
            continue;
        }
        // Nicht, wenn direkt ein Gesetz folgt („Absatz 2 VVG“ ist unüblich) oder
        // eine Normnummer vorausgeht („§ 3 Abs. 2“ wurde oben bereits erfasst).
        let mut c = Cursor {
            text,
            pos: m.start(),
        };
        let mut target = NormRef {
            law: LawRef::Same,
            norm: None,
            sub_section: None,
            paragraph: None,
            sentence: None,
            number: None,
        };
        qualifiers(&mut c, &mut target);
        if target.paragraph.is_some() {
            raw.push((m.start(), c.pos, target));
        }
    }
    let _ = abs_regex();

    raw.sort_by_key(|r| r.0);
    let offsets = CharOffsets::new(text);
    raw.into_iter()
        .map(|(s, e, target)| Reference {
            start: offsets.char_at(s),
            end: offsets.char_at(e),
            target,
        })
        .collect()
}

/// Umrechnung von Byte- in Zeichenoffsets.
struct CharOffsets {
    byte_to_char: Vec<usize>,
}

impl CharOffsets {
    fn new(text: &str) -> Self {
        let mut byte_to_char = vec![0; text.len() + 1];
        let mut ci = 0;
        for (bi, ch) in text.char_indices() {
            for k in 0..ch.len_utf8() {
                byte_to_char[bi + k] = ci;
            }
            ci += 1;
        }
        byte_to_char[text.len()] = ci;
        Self { byte_to_char }
    }

    fn char_at(&self, byte: usize) -> usize {
        self.byte_to_char[byte.min(self.byte_to_char.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> Vec<Reference> {
        find_references(text, "BGB")
    }

    fn slice(text: &str, r: &Reference) -> String {
        text.chars().skip(r.start).take(r.end - r.start).collect()
    }

    #[test]
    fn simple_paragraph_with_absatz() {
        let t = "Nach § 280 Abs. 1 haftet der Schuldner.";
        let r = refs(t);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(t, &r[0]), "280 Abs. 1");
        assert_eq!(r[0].target.norm.as_deref(), Some("§ 280"));
        assert_eq!(r[0].target.paragraph.as_deref(), Some("1"));
        assert_eq!(r[0].target.law, LawRef::Same);
    }

    #[test]
    fn range_of_paragraphs() {
        let t = "Die §§ 434 bis 437 gelten entsprechend.";
        let r = refs(t);
        assert_eq!(r.len(), 2);
        assert_eq!(slice(t, &r[0]), "434");
        assert_eq!(slice(t, &r[1]), "437");
        assert_eq!(r[1].target.norm.as_deref(), Some("§ 437"));
    }

    #[test]
    fn enumeration_with_qualifiers() {
        let t = "der §§ 28, 31a Abs. 1 Satz 2 sowie der §§ 32, 33 und 38 finden";
        let r = refs(t);
        let labels: Vec<_> = r.iter().map(|x| x.target.label()).collect();
        assert_eq!(
            labels,
            vec!["§ 28", "§ 31a Abs. 1 Satz 2", "§ 32", "§ 33", "§ 38"]
        );
        assert_eq!(slice(t, &r[1]), "31a Abs. 1 Satz 2");
    }

    #[test]
    fn absatz_enumeration_stays_with_norm() {
        let t = "gemäß § 312 Abs. 1, 2 und 4 Satz 1 ist";
        let r = refs(t);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(t, &r[0]), "312 Abs. 1, 2 und 4 Satz 1");
        assert_eq!(r[0].target.paragraph.as_deref(), Some("1"));
        assert_eq!(r[0].target.sentence.as_deref(), Some("1"));
    }

    #[test]
    fn article_with_law_abbrev() {
        let t = "vgl. Artikel 229 EGBGB und Art. 229 § 34 BGBEG.";
        let r = refs(t);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].target.norm.as_deref(), Some("Art. 229"));
        assert_eq!(r[0].target.law, LawRef::Abbrev("EGBGB".into()));
        assert_eq!(r[1].target.sub_section.as_deref(), Some("§ 34"));
        assert_eq!(r[1].target.law, LawRef::Abbrev("BGBEG".into()));
        assert_eq!(slice(t, &r[1]), "229 § 34");
        assert!(r[1]
            .target
            .enbez_candidates()
            .contains(&"Art 229 § 34".to_string()));
    }

    #[test]
    fn other_law_by_abbrev_and_unknown_law() {
        let t = "nach § 8 Abs. 5 VVG oder § 185a des Gesetzes über das Verfahren";
        let r = refs(t);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].target.law, LawRef::Abbrev("VVG".into()));
        assert_eq!(r[1].target.law, LawRef::Unknown);
        assert_eq!(r[1].target.norm.as_deref(), Some("§ 185a"));
    }

    #[test]
    fn same_law_abbrev_is_same() {
        let r = find_references("siehe § 433 BGB", "BGB");
        assert_eq!(r[0].target.law, LawRef::Same);
        let r = find_references("siehe § 433 BGB", "HGB");
        assert_eq!(r[0].target.law, LawRef::Abbrev("BGB".into()));
    }

    #[test]
    fn bare_absatz_refers_to_same_norm() {
        let t = "Eine Beziehung nach Absatz 3 oder 4 besteht, wenn Absatz 3 Satz 1 gilt.";
        let r = refs(t);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].target.norm, None);
        assert_eq!(r[0].target.paragraph.as_deref(), Some("3"));
        assert_eq!(slice(t, &r[0]), "Absatz 3 oder 4");
        assert_eq!(r[1].target.sentence.as_deref(), Some("1"));
    }

    #[test]
    fn ignores_signs_without_number_and_umlauts_keep_char_offsets() {
        assert!(refs("Der § allein und §§ ohne Zahl.").is_empty());
        let t = "Über § 1 hinaus";
        let r = refs(t);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(t, &r[0]), "1");
        assert_eq!(r[0].start, 7);
    }

    #[test]
    fn comma_after_single_paragraph_with_qualifier_does_not_continue() {
        let t = "§ 26 Absatz 2 Satz 1, des § 27 Absatz 1 und 3";
        let r = refs(t);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].target.norm.as_deref(), Some("§ 26"));
        assert_eq!(r[1].target.norm.as_deref(), Some("§ 27"));
        assert_eq!(slice(t, &r[1]), "27 Absatz 1 und 3");
    }

    #[test]
    fn suffix_letters_are_part_of_number() {
        let t = "§ 312g Abs. 2 Nr. 1 und § 1631b";
        let r = refs(t);
        assert_eq!(r[0].target.norm.as_deref(), Some("§ 312g"));
        assert_eq!(r[0].target.number.as_deref(), Some("1"));
        assert_eq!(r[1].target.norm.as_deref(), Some("§ 1631b"));
    }
}
