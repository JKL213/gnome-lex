// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Kartenvorlagen aus Anki-Paketen: Feldersetzung, Bedingungen und
//! Lückentexte.
//!
//! Unterstützt werden `{{Feld}}`, Filter wie `{{text:Feld}}`,
//! `{{cloze:Feld}}` und `{{hint:Feld}}`, die Bedingungen `{{#Feld}}`,
//! `{{^Feld}}`, `{{/Feld}}` sowie die Sonderfelder `FrontSide`, `Tags`,
//! `Type`, `Deck`, `Subdeck` und `Card`. Eingabefelder (`type:`) und
//! Sprachausgabe entfallen ersatzlos.

use super::html::{escape, strip_tags};

/// Alles, was eine Vorlage über Notiz und Karte wissen muss.
#[derive(Debug, Clone, Default)]
pub struct Context<'a> {
    /// Feldnamen und -inhalte (HTML) der Notiz.
    pub fields: &'a [(String, String)],
    /// Laufende Nummer der Karte innerhalb der Notiz (bei Lückentexten
    /// Lückennummer − 1).
    pub ord: u16,
    /// Notiztyp „Lückentext“.
    pub cloze: bool,
    /// Wird die Antwortseite erzeugt?
    pub answer: bool,
    /// Bereits erzeugte Vorderseite für `{{FrontSide}}`.
    pub front_side: &'a str,
    pub tags: &'a str,
    pub deck: &'a str,
    pub card: &'a str,
    pub notetype: &'a str,
}

/// Füllt eine Vorlage mit den Feldern der Notiz.
pub fn render(template: &str, ctx: &Context) -> String {
    let mut out = String::new();
    // Je offener Bedingung: ist ihr Inhalt sichtbar?
    let mut sections: Vec<bool> = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let active = sections.iter().all(|s| *s);
        if active {
            out.push_str(&rest[..start]);
        }
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // Unvollständige Klammer: wörtlich übernehmen.
            if active {
                out.push_str(&rest[start..]);
            }
            rest = "";
            break;
        };
        let tag = after[..end].trim_matches(|c: char| c == '{' || c.is_whitespace());
        rest = &after[end + 2..];
        // „{{{Feld}}}“ hinterlässt eine dritte schließende Klammer.
        if let Some(stripped) = rest.strip_prefix('}') {
            if after[..end].starts_with('{') {
                rest = stripped;
            }
        }
        if let Some(name) = tag.strip_prefix('#') {
            sections.push(is_truthy(name.trim(), ctx));
        } else if let Some(name) = tag.strip_prefix('^') {
            sections.push(!is_truthy(name.trim(), ctx));
        } else if tag.starts_with('/') {
            sections.pop();
        } else if active {
            out.push_str(&field_value(tag, ctx));
        }
    }
    if sections.iter().all(|s| *s) {
        out.push_str(rest);
    }
    out
}

/// Rückseite einer Karte: Die Antwortvorlage wiederholt meist die Frage
/// (`{{FrontSide}}`) und trennt sie mit `<hr id=answer>` ab. Auf einer
/// Karteikarte steht die Frage vorn, also bleibt nur der Teil dahinter.
pub fn render_back(template: &str, ctx: &Context) -> String {
    let rendered = render(template, ctx);
    match answer_separator(&rendered) {
        Some(end) => rendered[end..].to_owned(),
        None => rendered,
    }
}

/// Byteposition hinter `<hr id=answer>` (in beliebiger Schreibweise).
fn answer_separator(html: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<hr") {
        let start = from + pos;
        let end = start + lower[start..].find('>')? + 1;
        let tag: String = lower[start..end]
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '"' && *c != '\'')
            .collect();
        if tag.contains("id=answer") {
            return Some(end);
        }
        from = end;
    }
    None
}

fn field<'a>(name: &str, ctx: &'a Context) -> Option<&'a str> {
    ctx.fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Nummer aus „c1“, „c12“ (Bedingung in Lückentext-Vorlagen).
fn cloze_number(name: &str) -> Option<u16> {
    name.strip_prefix('c')
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .and_then(|n| n.parse().ok())
}

fn is_truthy(name: &str, ctx: &Context) -> bool {
    if let Some(value) = field(name, ctx) {
        return !is_blank(value);
    }
    if ctx.cloze {
        if let Some(n) = cloze_number(name) {
            return n == ctx.ord + 1;
        }
    }
    match name {
        "Tags" => !ctx.tags.trim().is_empty(),
        _ => false,
    }
}

/// Leer im Sinne der Vorlagen: nur Leerraum, Umbrüche oder leere Absätze.
fn is_blank(value: &str) -> bool {
    strip_tags(value)
        .chars()
        .all(|c| c.is_whitespace() || c == '\u{a0}')
        && !value.to_ascii_lowercase().contains("<img")
}

fn field_value(tag: &str, ctx: &Context) -> String {
    if tag.starts_with("tts ") || tag.starts_with("tts-voices") {
        return String::new();
    }
    let mut parts: Vec<&str> = tag.split(':').map(str::trim).collect();
    let name = parts.pop().unwrap_or_default();
    // Eingabefelder gibt es auf Papier nicht.
    if parts.contains(&"type") {
        return String::new();
    }
    let mut value = match field(name, ctx) {
        Some(v) => v.to_owned(),
        None => match name {
            "FrontSide" => ctx.front_side.to_owned(),
            "Tags" => escape(ctx.tags.trim()),
            "Type" => escape(ctx.notetype),
            "Deck" => escape(ctx.deck),
            "Subdeck" => escape(super::deck_title(ctx.deck)),
            "Card" => escape(ctx.card),
            _ => String::new(),
        },
    };
    // Filter stehen vor dem Feldnamen und werden von rechts nach links angewandt.
    for filter in parts.iter().rev() {
        value = match *filter {
            "text" => escape(&strip_tags(&value)),
            "cloze" | "cloze-only" => render_cloze(&value, ctx.ord + 1, ctx.answer),
            "hint" if !ctx.answer => String::new(),
            _ => value,
        };
    }
    value
}

/// Ersetzt Lücken der Form `{{c1::Antwort::Hinweis}}`. Die Lücke der Karte
/// erscheint auf der Vorderseite als „[…]“ (oder mit ihrem Hinweis), auf der
/// Rückseite hervorgehoben; alle übrigen Lücken werden ausgeschrieben.
pub fn render_cloze(text: &str, number: u16, answer: bool) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        if let Some((n, body_start)) = cloze_open(rest) {
            if let Some((content_end, close_end)) = cloze_close(rest, body_start) {
                let (body, hint) = split_hint(&rest[body_start..content_end]);
                let inner = render_cloze(body, number, answer);
                if n != number {
                    out.push_str(&inner);
                } else if answer {
                    out.push_str("<span class=\"cloze\">");
                    out.push_str(&inner);
                    out.push_str("</span>");
                } else {
                    out.push_str("<span class=\"cloze\">[");
                    out.push_str(hint.filter(|h| !h.is_empty()).unwrap_or("…"));
                    out.push_str("]</span>");
                }
                pos += close_end;
                continue;
            }
        }
        let ch = rest.chars().next().expect("Zeichen");
        out.push(ch);
        pos += ch.len_utf8();
    }
    out
}

/// Erkennt `{{cN::` am Anfang von `text`; liefert die Nummer und den Beginn
/// des Inhalts.
fn cloze_open(text: &str) -> Option<(u16, usize)> {
    let rest = text.strip_prefix("{{c")?;
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || !rest[digits..].starts_with("::") {
        return None;
    }
    let number = rest[..digits].parse().ok()?;
    Some((number, 3 + digits + 2))
}

/// Sucht das zu einer Lücke gehörende `}}` (verschachtelte Lücken zählen
/// mit). Liefert Ende des Inhalts und Position hinter der Klammer.
fn cloze_close(text: &str, body_start: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut pos = body_start;
    while pos < text.len() {
        let rest = &text[pos..];
        if let Some((_, inner)) = cloze_open(rest) {
            depth += 1;
            pos += inner;
        } else if rest.starts_with("}}") {
            if depth == 0 {
                return Some((pos, pos + 2));
            }
            depth -= 1;
            pos += 2;
        } else {
            pos += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

/// Trennt den Hinweis (`Antwort::Hinweis`) ab, ohne in verschachtelte
/// Lücken hineinzugreifen.
fn split_hint(body: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    let mut pos = 0;
    while pos < body.len() {
        let rest = &body[pos..];
        if let Some((_, inner)) = cloze_open(rest) {
            depth += 1;
            pos += inner;
        } else if rest.starts_with("}}") {
            depth = depth.saturating_sub(1);
            pos += 2;
        } else if depth == 0 && rest.starts_with("::") {
            return (&body[..pos], Some(&body[pos + 2..]));
        } else {
            pos += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    (body, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn replaces_fields_and_special_names() {
        let f = fields(&[
            ("Front", "Was ist ein <b>Anspruch</b>?"),
            ("Back", "§ 194 BGB"),
        ]);
        let ctx = Context {
            fields: &f,
            deck: "BGB AT::Verjährung",
            tags: "bgb at",
            ..Default::default()
        };
        assert_eq!(render("{{Front}}", &ctx), "Was ist ein <b>Anspruch</b>?");
        assert_eq!(render("{{text:Front}}", &ctx), "Was ist ein Anspruch?");
        assert_eq!(
            render("{{Subdeck}} / {{Tags}}", &ctx),
            "Verjährung / bgb at"
        );
        assert_eq!(render("[{{Unbekannt}}]", &ctx), "[]");
        assert_eq!(render("{{type:Back}}x", &ctx), "x");
    }

    #[test]
    fn conditionals_depend_on_field_content() {
        let f = fields(&[
            ("Front", "Frage"),
            ("Extra", "<div><br></div>"),
            ("Quelle", "BGH"),
        ]);
        let ctx = Context {
            fields: &f,
            ..Default::default()
        };
        let template = "{{Front}}{{#Extra}} ({{Extra}}){{/Extra}}{{^Extra}} –{{/Extra}}\
                        {{#Quelle}} [{{Quelle}}]{{/Quelle}}";
        assert_eq!(render(template, &ctx), "Frage – [BGH]");
        // Verschachtelt: innere Bedingung bleibt unsichtbar, wenn die äußere es ist.
        let nested = "{{#Extra}}a{{#Quelle}}b{{/Quelle}}c{{/Extra}}d";
        assert_eq!(render(nested, &ctx), "d");
    }

    #[test]
    fn back_side_drops_the_repeated_question() {
        let f = fields(&[("Front", "Frage"), ("Back", "Antwort")]);
        let ctx = Context {
            fields: &f,
            answer: true,
            front_side: "Frage",
            ..Default::default()
        };
        let template = "{{FrontSide}}\n\n<hr id=answer>\n\n{{Back}}";
        assert_eq!(render_back(template, &ctx).trim(), "Antwort");
        assert_eq!(
            render_back("{{FrontSide}}<hr id=\"answer\" />{{Back}}", &ctx),
            "Antwort"
        );
        // Ohne Trenner bleibt die Rückseite, wie die Vorlage sie vorsieht.
        assert_eq!(render_back("{{Back}}<hr>Ende", &ctx), "Antwort<hr>Ende");
    }

    #[test]
    fn cloze_question_and_answer() {
        let text = "Der {{c1::Kaufvertrag}} verpflichtet zur {{c2::Übergabe::Pflicht}}.";
        assert_eq!(
            render_cloze(text, 1, false),
            "Der <span class=\"cloze\">[…]</span> verpflichtet zur Übergabe."
        );
        assert_eq!(
            render_cloze(text, 2, false),
            "Der Kaufvertrag verpflichtet zur <span class=\"cloze\">[Pflicht]</span>."
        );
        assert_eq!(
            render_cloze(text, 2, true),
            "Der Kaufvertrag verpflichtet zur <span class=\"cloze\">Übergabe</span>."
        );
    }

    #[test]
    fn nested_cloze() {
        let text = "{{c1::Anspruch aus {{c2::§ 433 II}} BGB}}";
        assert_eq!(
            render_cloze(text, 2, false),
            "Anspruch aus <span class=\"cloze\">[…]</span> BGB"
        );
        assert_eq!(
            render_cloze(text, 1, true),
            "<span class=\"cloze\">Anspruch aus § 433 II BGB</span>"
        );
    }

    #[test]
    fn cloze_template_uses_card_number() {
        let f = fields(&[("Text", "{{c1::A}} und {{c2::B}}"), ("Extra", "")]);
        let ctx = Context {
            fields: &f,
            ord: 1,
            cloze: true,
            ..Default::default()
        };
        assert_eq!(
            render("{{cloze:Text}}{{#c2}}!{{/c2}}{{#c1}}?{{/c1}}", &ctx),
            "A und <span class=\"cloze\">[…]</span>!"
        );
    }
}
