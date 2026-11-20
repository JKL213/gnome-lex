// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Wandelt das HTML einer Karteikarte in Blöcke um, die ohne Web-Technik
//! darstellbar sind: Text als Pango-Markup, Bilder und Trennlinien.
//!
//! Erkannt werden die auf Karten üblichen Auszeichnungen (fett, kursiv,
//! unterstrichen, hoch-/tiefgestellt, Farben, Listen, einfache Tabellen).
//! Normzitate im Text werden zu Verweisen (`<a href="ref:N">`), auch in der
//! unter Studierenden üblichen Kurzform „§ 433 I 1 BGB“.

use std::sync::OnceLock;

use regex::Regex;

use crate::refs::{find_references, LawRef, NormRef};

/// Ein darstellbarer Abschnitt einer Kartenseite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Text als Pango-Markup.
    Text(String),
    /// Bild; der Wert ist der Dateiname im Medienordner des Stapels.
    Image(String),
    /// Waagerechte Trennlinie.
    Rule,
}

/// Ergebnis der Umwandlung: Blöcke und die Ziele der Verweise, auf die das
/// Markup mit `ref:<Index>` zeigt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub blocks: Vec<Block>,
    pub links: Vec<NormRef>,
}

impl Rendered {
    /// Enthält die Seite gar nichts Darstellbares?
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// Maskiert Text für HTML und Pango-Markup.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Reiner Text ohne Auszeichnung (Blockgrenzen werden zu Leerzeichen).
pub fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    for token in tokenize(html) {
        match token {
            Token::Text(t) => out.push_str(&decode_entities(t)),
            // Nur Blockgrenzen trennen Wörter; „<b>An</b>spruch“ bleibt ein Wort.
            Token::Open { name, .. } | Token::Close(name) => {
                if BLOCK_ELEMENTS.contains(&name.as_str())
                    || matches!(name.as_str(), "br" | "hr" | "td" | "th" | "img")
                {
                    out.push(' ');
                }
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Dateinamen aller eingebundenen Bilder (ohne Netzadressen).
pub fn image_sources(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in tokenize(html) {
        if let Token::Open { name, attrs, .. } = token {
            if name == "img" {
                if let Some(src) = attr(&attrs, "src").and_then(local_media_name) {
                    if !out.contains(&src) {
                        out.push(src);
                    }
                }
            }
        }
    }
    out
}

/// Wandelt HTML in darstellbare Blöcke um.
pub fn render(html: &str) -> Rendered {
    let mut b = Builder::default();
    for token in tokenize(html) {
        match token {
            Token::Text(text) => b.text(text),
            Token::Open { name, attrs, .. } => b.open(&name, &attrs),
            Token::Close(name) => b.close(&name),
        }
    }
    b.flush();
    Rendered {
        blocks: b.blocks,
        links: b.links,
    }
}

// ----------------------------------------------------------------------
// Zerlegung
// ----------------------------------------------------------------------

/// Attribute eines Elements als (Name, Wert).
type Attributes = Vec<(String, String)>;

enum Token<'a> {
    Text(&'a str),
    Open { name: String, attrs: Attributes },
    Close(String),
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Elemente, deren Inhalt nicht zum Text gehört.
const RAW_ELEMENTS: [&str; 4] = ["style", "script", "head", "title"];

fn tokenize(html: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut text_start = 0;
    let bytes = html.as_bytes();
    while pos < html.len() {
        if bytes[pos] != b'<' {
            pos += 1;
            continue;
        }
        let rest = &html[pos..];
        let (token, len) = if rest.starts_with("<!--") {
            (None, rest.find("-->").map_or(rest.len(), |e| e + 3))
        } else if let Some(after) = rest.strip_prefix("</") {
            let Some(end) = after.find('>') else {
                break;
            };
            let name = after[..end].trim().to_ascii_lowercase();
            (Some(Token::Close(name)), end + 3)
        } else if rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '!') {
            match parse_tag(rest) {
                Some((name, attrs, len)) => {
                    let mut len = len;
                    // Inhalt von <style>, <script> usw. überspringen.
                    if RAW_ELEMENTS.contains(&name.as_str()) {
                        let lower = rest[len..].to_ascii_lowercase();
                        let close = format!("</{name}");
                        len += lower.find(&close).unwrap_or(lower.len());
                    }
                    (Some(Token::Open { name, attrs }), len)
                }
                None => break,
            }
        } else {
            // Einzelnes „<“ im Text.
            pos += 1;
            continue;
        };
        if text_start < pos {
            tokens.push(Token::Text(&html[text_start..pos]));
        }
        tokens.extend(token);
        pos += len;
        text_start = pos;
    }
    if text_start < html.len() {
        tokens.push(Token::Text(&html[text_start..]));
    }
    tokens
}

/// Liest ein öffnendes Tag ab „<“; liefert Name, Attribute und Länge.
fn parse_tag(text: &str) -> Option<(String, Attributes, usize)> {
    let mut chars = text.char_indices().skip(1).peekable();
    let mut name = String::new();
    while let Some((_, c)) = chars.peek().copied() {
        if c.is_whitespace() || c == '>' || c == '/' {
            break;
        }
        name.push(c.to_ascii_lowercase());
        chars.next();
    }
    let mut attrs = Vec::new();
    loop {
        while chars
            .peek()
            .is_some_and(|(_, c)| c.is_whitespace() || *c == '/')
        {
            chars.next();
        }
        let (i, c) = chars.peek().copied()?;
        if c == '>' {
            return Some((name, attrs, i + 1));
        }
        let mut key = String::new();
        while let Some((_, c)) = chars.peek().copied() {
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
                break;
            }
            key.push(c.to_ascii_lowercase());
            chars.next();
        }
        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek().is_some_and(|(_, c)| *c == '=') {
            chars.next();
            while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                chars.next();
            }
            match chars.peek().copied() {
                Some((_, quote)) if quote == '"' || quote == '\'' => {
                    chars.next();
                    for (_, c) in chars.by_ref() {
                        if c == quote {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some((_, c)) = chars.peek().copied() {
                        if c.is_whitespace() || c == '>' {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }
        if key.is_empty() {
            // Unlesbares Zeichen überspringen, damit die Schleife endet.
            chars.next();
        } else {
            attrs.push((key, decode_entities(&value)));
        }
    }
}

// ----------------------------------------------------------------------
// Zeichenreferenzen
// ----------------------------------------------------------------------

const ENTITIES: &[(&str, &str)] = &[
    ("nbsp", "\u{a0}"),
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("sect", "§"),
    ("para", "¶"),
    ("auml", "ä"),
    ("ouml", "ö"),
    ("uuml", "ü"),
    ("Auml", "Ä"),
    ("Ouml", "Ö"),
    ("Uuml", "Ü"),
    ("szlig", "ß"),
    ("eacute", "é"),
    ("egrave", "è"),
    ("agrave", "à"),
    ("ndash", "–"),
    ("mdash", "—"),
    ("hellip", "…"),
    ("bdquo", "„"),
    ("ldquo", "“"),
    ("rdquo", "”"),
    ("lsquo", "‘"),
    ("rsquo", "’"),
    ("sbquo", "‚"),
    ("laquo", "«"),
    ("raquo", "»"),
    ("rarr", "→"),
    ("larr", "←"),
    ("harr", "↔"),
    ("rArr", "⇒"),
    ("lArr", "⇐"),
    ("hArr", "⇔"),
    ("uarr", "↑"),
    ("darr", "↓"),
    ("bull", "•"),
    ("middot", "·"),
    ("times", "×"),
    ("divide", "÷"),
    ("plusmn", "±"),
    ("ne", "≠"),
    ("le", "≤"),
    ("ge", "≥"),
    ("deg", "°"),
    ("euro", "€"),
    ("copy", "©"),
    ("reg", "®"),
    ("frac12", "½"),
    ("shy", ""),
    ("thinsp", "\u{2009}"),
    ("ensp", "\u{2002}"),
    ("emsp", "\u{2003}"),
    ("zwnj", ""),
    ("check", "✓"),
    ("cross", "✗"),
];

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .char_indices()
            .take(12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i);
        let decoded = end.and_then(|end| {
            let name = &after[..end];
            if let Some(num) = name.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse().ok(),
                };
                code.and_then(char::from_u32).map(|c| (c.to_string(), end))
            } else {
                ENTITIES
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| ((*v).to_owned(), end))
            }
        });
        match decoded {
            Some((value, end)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

// ----------------------------------------------------------------------
// Farben und Stilangaben
// ----------------------------------------------------------------------

const NAMED_COLORS: &[(&str, (u8, u8, u8))] = &[
    ("black", (0, 0, 0)),
    ("white", (255, 255, 255)),
    ("red", (224, 27, 36)),
    ("darkred", (165, 29, 45)),
    ("maroon", (128, 0, 0)),
    ("green", (38, 162, 105)),
    ("darkgreen", (0, 100, 0)),
    ("lime", (0, 255, 0)),
    ("olive", (128, 128, 0)),
    ("blue", (26, 95, 180)),
    ("darkblue", (0, 0, 139)),
    ("navy", (0, 0, 128)),
    ("teal", (0, 128, 128)),
    ("aqua", (0, 255, 255)),
    ("cyan", (0, 255, 255)),
    ("yellow", (255, 255, 0)),
    ("orange", (255, 165, 0)),
    ("brown", (165, 42, 42)),
    ("purple", (128, 0, 128)),
    ("magenta", (255, 0, 255)),
    ("fuchsia", (255, 0, 255)),
    ("pink", (255, 192, 203)),
    ("gray", (128, 128, 128)),
    ("grey", (128, 128, 128)),
    ("silver", (192, 192, 192)),
];

/// Liest `#rgb`, `#rrggbb`, `rgb(r, g, b)` und gängige Farbnamen.
fn parse_color(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
        return match hex.len() {
            3 | 4 => Some((digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17)),
            6 | 8 => Some((digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            _ => None,
        };
    }
    if let Some(args) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
    {
        let mut parts = args
            .trim_end_matches(')')
            .split([',', ' ', '/'])
            .filter(|p| !p.is_empty())
            .map(|p| {
                p.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|v| v.clamp(0.0, 255.0) as u8)
            });
        return Some((parts.next()??, parts.next()??, parts.next()??));
    }
    NAMED_COLORS
        .iter()
        .find(|(n, _)| *n == value)
        .map(|(_, c)| *c)
}

/// Wahrgenommene Helligkeit (0 = schwarz, 1 = weiß).
fn luminance((r, g, b): (u8, u8, u8)) -> f32 {
    (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Farbe hervorgehobener Lücken (Tinte).
const CLOZE_COLOR: &str = "#1a5fb4";

/// Pango-Markup (öffnend, schließend) für ein Element samt Stilangaben.
fn element_markup(name: &str, attrs: &[(String, String)]) -> (String, String) {
    let mut tags: Vec<&str> = Vec::new();
    let mut span: Vec<String> = Vec::new();
    match name {
        "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" => tags.push("b"),
        "i" | "em" | "cite" | "dfn" | "var" => tags.push("i"),
        "u" | "ins" => tags.push("u"),
        "s" | "strike" | "del" => tags.push("s"),
        "sub" => tags.push("sub"),
        "sup" => tags.push("sup"),
        "small" => tags.push("small"),
        "big" => tags.push("big"),
        "code" | "tt" | "kbd" | "samp" | "pre" => tags.push("tt"),
        "mark" => span.push("background=\"#f9f06b\"".into()),
        _ => {}
    }
    if attr(attrs, "class").is_some_and(|c| c.split_whitespace().any(|c| c == "cloze")) {
        tags.push("b");
        span.push(format!("foreground=\"{CLOZE_COLOR}\""));
    }
    let mut foreground = attr(attrs, "color").and_then(parse_color);
    let mut background = None;
    for declaration in attr(attrs, "style").unwrap_or_default().split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        match property.trim().to_ascii_lowercase().as_str() {
            "color" => foreground = parse_color(&value).or(foreground),
            "background" | "background-color" => background = parse_color(&value),
            "font-weight" => {
                let heavy = value.parse::<u32>().is_ok_and(|w| w >= 600);
                if heavy || value.starts_with("bold") {
                    tags.push("b");
                }
            }
            "font-style" if value.starts_with("italic") || value.starts_with("oblique") => {
                tags.push("i")
            }
            "text-decoration" | "text-decoration-line" => {
                if value.contains("underline") {
                    tags.push("u");
                }
                if value.contains("line-through") {
                    tags.push("s");
                }
            }
            _ => {}
        }
    }
    // Sehr helle Schrift (Karten für dunkle Oberflächen) wäre auf Papier
    // unsichtbar, sehr dunkle Hinterlegung machte die Tinte unlesbar.
    if let Some(color) = foreground.filter(|c| luminance(*c) < 0.72) {
        span.push(format!("foreground=\"{}\"", hex(color)));
    }
    if let Some(color) = background.filter(|c| luminance(*c) > 0.55) {
        span.push(format!("background=\"{}\"", hex(color)));
    }
    tags.dedup();
    let mut open = String::new();
    let mut close = String::new();
    if !span.is_empty() {
        open.push_str(&format!("<span {}>", span.join(" ")));
    }
    for tag in &tags {
        open.push_str(&format!("<{tag}>"));
    }
    for tag in tags.iter().rev() {
        close.push_str(&format!("</{tag}>"));
    }
    if !span.is_empty() {
        close.push_str("</span>");
    }
    (open, close)
}

/// Dateiname eines lokalen Bildes; Netzadressen und eingebettete Daten
/// werden nicht geladen.
fn local_media_name(src: &str) -> Option<String> {
    let src = src.trim();
    if src.is_empty() || src.contains("://") || src.starts_with("data:") {
        return None;
    }
    let decoded = percent_decode(src);
    let name = decoded.rsplit(['/', '\\']).next().unwrap_or_default();
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_owned())
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let value = text
                .get(i + 1..i + 3)
                .and_then(|digits| u8::from_str_radix(digits, 16).ok());
            if let Some(value) = value {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ----------------------------------------------------------------------
// Aufbau der Blöcke
// ----------------------------------------------------------------------

const BLOCK_ELEMENTS: [&str; 21] = [
    "div",
    "p",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "center",
    "section",
    "article",
    "dl",
    "dt",
    "dd",
];
const VOID_ELEMENTS: [&str; 8] = ["br", "img", "hr", "input", "meta", "link", "wbr", "col"];

/// Ein offenes Element mit dem Markup, das es umschließt.
struct Open {
    name: String,
    open: String,
    close: String,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    links: Vec<NormRef>,
    /// Markup des Blocks, an dem gerade geschrieben wird.
    cur: String,
    /// Enthält `cur` schon sichtbaren Text?
    has_text: bool,
    /// Zeilenumbrüche am Ende von `cur`.
    newlines: u8,
    /// Zwischen zwei Wörtern steht noch ein Leerzeichen aus.
    pending_space: bool,
    stack: Vec<Open>,
    /// Je offener Liste: Zähler (nummerierte Liste) oder `None` (Aufzählung).
    lists: Vec<Option<u32>>,
    /// Zellen der laufenden Tabellenzeile.
    cells: u32,
}

impl Builder {
    fn open(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "br" => self.line_break(),
            "hr" => {
                self.flush();
                self.blocks.push(Block::Rule);
            }
            "img" => {
                if let Some(src) = attr(attrs, "src").and_then(local_media_name) {
                    self.flush();
                    self.blocks.push(Block::Image(src));
                }
            }
            "p" | "blockquote" | "table" => self.ensure_breaks(2),
            "ul" | "ol" => {
                self.ensure_breaks(1);
                self.lists.push((name == "ol").then_some(0));
            }
            "li" => {
                self.ensure_breaks(1);
                let depth = self.lists.len().saturating_sub(1);
                let bullet = match self.lists.last_mut() {
                    Some(Some(counter)) => {
                        *counter += 1;
                        format!("{counter}. ")
                    }
                    _ => "• ".to_owned(),
                };
                self.raw_text(&format!("{}{bullet}", "\u{2003}".repeat(depth)));
            }
            "tr" => {
                self.ensure_breaks(1);
                self.cells = 0;
            }
            "td" | "th" => {
                if self.cells > 0 {
                    self.raw_text("  ·  ");
                }
                self.cells += 1;
            }
            _ if BLOCK_ELEMENTS.contains(&name) => self.ensure_breaks(1),
            _ => {}
        }
        if VOID_ELEMENTS.contains(&name) || RAW_ELEMENTS.contains(&name) {
            return;
        }
        let (open, close) = element_markup(name, attrs);
        // Ein ausstehendes Leerzeichen gehört vor die Auszeichnung.
        if self.pending_space && self.has_text && self.newlines == 0 {
            self.cur.push(' ');
        }
        self.pending_space = false;
        self.cur.push_str(&open);
        self.stack.push(Open {
            name: name.to_owned(),
            open,
            close,
        });
    }

    fn close(&mut self, name: &str) {
        // Schließt alles bis einschließlich des passenden Elements; ein
        // schließendes Tag ohne Gegenstück wird übergangen.
        if let Some(index) = self.stack.iter().rposition(|o| o.name == name) {
            while self.stack.len() > index {
                if let Some(open) = self.stack.pop() {
                    self.cur.push_str(&open.close);
                }
            }
        }
        match name {
            "p" | "blockquote" | "table" => self.ensure_breaks(2),
            "ul" | "ol" => {
                self.lists.pop();
                self.ensure_breaks(1);
            }
            _ if BLOCK_ELEMENTS.contains(&name) => self.ensure_breaks(1),
            _ => {}
        }
    }

    /// Erzwungener Umbruch (`<br>`); mehr als eine Leerzeile entsteht nicht.
    fn line_break(&mut self) {
        if self.has_text && self.newlines < 2 {
            self.cur.push('\n');
            self.newlines += 1;
        }
        self.pending_space = false;
    }

    /// Sorgt für mindestens `count` Umbrüche vor dem folgenden Text.
    fn ensure_breaks(&mut self, count: u8) {
        while self.has_text && self.newlines < count {
            self.cur.push('\n');
            self.newlines += 1;
        }
        self.pending_space = false;
    }

    /// Fügt Text ein, der nicht weiter untersucht wird (Aufzählungszeichen).
    fn raw_text(&mut self, text: &str) {
        self.cur.push_str(&escape(text));
        self.has_text = true;
        self.newlines = 0;
        self.pending_space = false;
    }

    fn in_pre(&self) -> bool {
        self.stack.iter().any(|o| o.name == "pre")
    }

    fn text(&mut self, raw: &str) {
        let entities = decode_entities(raw);
        let decoded = sound_regex().replace_all(&entities, "");
        let mut piece = String::new();
        if self.in_pre() {
            piece.push_str(&decoded);
        } else {
            // Leerraum zusammenziehen; am Zeilenanfang entfällt er ganz.
            for c in decoded.chars() {
                if c.is_whitespace() && c != '\u{a0}' {
                    self.pending_space = true;
                    continue;
                }
                let at_line_start = piece.is_empty() && (!self.has_text || self.newlines > 0);
                if self.pending_space && !at_line_start {
                    piece.push(' ');
                }
                self.pending_space = false;
                piece.push(c);
            }
        }
        if piece.is_empty() {
            return;
        }
        let linked = self.link_references(&piece);
        self.cur.push_str(&linked);
        self.has_text = true;
        self.newlines = 0;
    }

    /// Maskiert `text` und macht Normzitate zu Verweisen.
    fn link_references(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for citation in find_citations(text) {
            if citation.start < last {
                continue;
            }
            out.push_str(&escape(&text[last..citation.start]));
            out.push_str(&format!("<a href=\"ref:{}\">", self.links.len()));
            // „§“ und Nummer bleiben am Zeilenende beisammen.
            let label = text[citation.start..citation.end]
                .replacen("§ ", "§\u{a0}", 1)
                .replacen("Art. ", "Art.\u{a0}", 1);
            out.push_str(&escape(&label));
            out.push_str("</a>");
            self.links.push(citation.target);
            last = citation.end;
        }
        out.push_str(&escape(&text[last..]));
        out
    }

    /// Schließt den laufenden Textblock ab; offene Auszeichnungen werden
    /// geschlossen und im nächsten Block wieder geöffnet.
    fn flush(&mut self) {
        if self.has_text {
            let mut markup = std::mem::take(&mut self.cur);
            let trimmed = markup.trim_end_matches('\n').len();
            markup.truncate(trimmed);
            for open in self.stack.iter().rev() {
                markup.push_str(&open.close);
            }
            self.blocks.push(Block::Text(markup));
        }
        self.cur.clear();
        for open in &self.stack {
            self.cur.push_str(&open.open);
        }
        self.has_text = false;
        self.newlines = 0;
        self.pending_space = false;
    }
}

fn sound_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[sound:[^\]]*\]").expect("regex"))
}

// ----------------------------------------------------------------------
// Normzitate
// ----------------------------------------------------------------------

/// Ein Normzitat im Text (Bytepositionen).
struct Citation {
    start: usize,
    end: usize,
    target: NormRef,
}

/// Findet Normzitate samt Kurzform („§ 433 I 1 BGB“): Der Verweisparser
/// liefert die Norm, [`citation_tail`] ergänzt Absatz, Satz und Gesetz.
fn find_citations(text: &str) -> Vec<Citation> {
    let byte_at: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let mut out = Vec::new();
    for reference in find_references(text, "") {
        let mut target = reference.target;
        // „Absatz 3“ ohne Norm hat auf einer Karte kein Ziel.
        if target.norm.is_none() || target.law == LawRef::Unknown {
            continue;
        }
        let (Some(start), Some(mut end)) = (
            byte_at.get(reference.start).copied(),
            byte_at.get(reference.end).copied(),
        ) else {
            continue;
        };
        if target.law == LawRef::Same {
            let tail = citation_tail(&text[end..]);
            if target.paragraph.is_none() {
                target.paragraph = tail.paragraph;
                if target.sentence.is_none() {
                    target.sentence = tail.sentence;
                }
                end += tail.len;
            }
            if let Some(law) = tail.law {
                target.law = LawRef::Abbrev(law);
            }
        }
        out.push(Citation {
            start: sign_start(text, start),
            end,
            target,
        });
    }
    out
}

/// Zieht ein unmittelbar vorangestelltes „§“/„§§“/„Art.“ in den Verweis.
fn sign_start(text: &str, start: usize) -> usize {
    let before = text[..start].trim_end();
    for sign in ["§§", "§", "Artikel", "Art."] {
        if before.ends_with(sign) {
            return before.len() - sign.len();
        }
    }
    start
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Tail {
    paragraph: Option<String>,
    sentence: Option<String>,
    law: Option<String>,
    /// Länge von Absatz- und Satzangabe direkt hinter der Norm.
    len: usize,
}

/// Wert einer römischen Zahl bis 39 („I“ … „XXXIX“), wie sie Absätze zählt.
fn roman_value(token: &str) -> Option<u32> {
    if token.is_empty() || !token.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) {
        return None;
    }
    let digit = |c| match c {
        'I' => 1,
        'V' => 5,
        _ => 10,
    };
    let values: Vec<u32> = token.chars().map(digit).collect();
    let mut total = 0;
    for (i, value) in values.iter().enumerate() {
        if values.get(i + 1).is_some_and(|next| next > value) {
            total -= *value as i32;
        } else {
            total += *value as i32;
        }
    }
    let total = u32::try_from(total).ok().filter(|t| (1..40).contains(t))?;
    // Nur die übliche Schreibweise gilt („IIII“ oder „VX“ sind keine Zahlen).
    (to_roman(total) == token).then_some(total)
}

fn to_roman(mut n: u32) -> String {
    let mut out = String::new();
    for (value, sign) in [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
        while n >= value {
            out.push_str(sign);
            n -= value;
        }
    }
    out
}

fn looks_like_law(token: &str) -> bool {
    let len = token.chars().count();
    (2..=12).contains(&len)
        && token.chars().all(char::is_alphabetic)
        && token.chars().next().is_some_and(char::is_uppercase)
        && token.chars().filter(|c| c.is_uppercase()).count() >= 2
        && roman_value(token).is_none()
}

/// Liest, was in der Kurzzitierweise auf die Norm folgt: römischer Absatz,
/// Satznummer, weitere Angaben („Nr. 2“, „Alt. 1“, „analog“) und schließlich
/// das Gesetzeskürzel.
fn citation_tail(rest: &str) -> Tail {
    const FILLERS: [&str; 16] = [
        "Abs.", "S.", "Nr.", "Alt.", "Var.", "Hs.", "Fall", "lit.", "f.", "ff.", "analog", "a.F.",
        "n.F.", "i.V.m.", "bis", "und",
    ];
    let mut tail = Tail::default();
    let mut pos = 0;
    let mut count = 0;
    let mut direct = true;
    while count < 14 {
        let trimmed = rest[pos..].trim_start_matches([' ', '\u{a0}']);
        let offset = rest.len() - trimmed.len();
        let word_len = trimmed
            .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ')')
            .unwrap_or(trimmed.len());
        if word_len == 0 {
            // Komma zwischen Aufzählungsgliedern („§§ 280 I, III, 281 BGB“).
            if trimmed.starts_with(',') {
                pos = offset + 1;
                direct = false;
                count += 1;
                continue;
            }
            break;
        }
        let word = &trimmed[..word_len];
        let bare = word.trim_end_matches('.');
        if looks_like_law(bare) {
            tail.law = Some(bare.to_owned());
            break;
        }
        let roman = roman_value(word);
        let number = word.chars().all(|c| c.is_ascii_digit());
        if direct && tail.paragraph.is_none() && tail.sentence.is_none() && roman.is_some() {
            tail.paragraph = roman.map(|v| v.to_string());
            tail.len = offset + word_len;
        } else if direct && tail.paragraph.is_some() && tail.sentence.is_none() && number {
            tail.sentence = Some(word.to_owned());
            tail.len = offset + word_len;
        } else if roman.is_some()
            || number
            || FILLERS.contains(&word)
            || (!bare.is_empty() && bare.chars().all(|c| c.is_ascii_digit()))
            || (bare.chars().count() == 1 && bare.chars().all(|c| c.is_ascii_lowercase()))
            || word.starts_with('§')
        {
            direct = false;
        } else {
            break;
        }
        pos = offset + word_len;
        count += 1;
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_blocks(html: &str) -> Vec<String> {
        render(html)
            .blocks
            .into_iter()
            .map(|b| match b {
                Block::Text(t) => t,
                Block::Image(name) => format!("[Bild {name}]"),
                Block::Rule => "---".into(),
            })
            .collect()
    }

    #[test]
    fn inline_formatting_becomes_pango_markup() {
        assert_eq!(
            text_blocks("Ein <b>fetter</b> und <em>kursiver</em> Text mit H<sub>2</sub>O"),
            ["Ein <b>fetter</b> und <i>kursiver</i> Text mit H<sub>2</sub>O"]
        );
    }

    #[test]
    fn whitespace_and_breaks_are_normalised() {
        assert_eq!(
            text_blocks("<div>  Erste   Zeile </div>\n<div>Zweite<br>Dritte</div><div><br></div>"),
            ["Erste Zeile\nZweite\nDritte"]
        );
        assert_eq!(
            text_blocks("<p>Absatz eins</p><p>Absatz zwei</p>"),
            ["Absatz eins\n\nAbsatz zwei"]
        );
        assert!(render("<div><br></div> \n ").is_empty());
    }

    #[test]
    fn entities_and_special_characters() {
        assert_eq!(
            text_blocks("&euro; 1 &amp; &lt;Tag&gt; &#8222;Zitat&#x201C; A&nbsp;B"),
            ["€ 1 &amp; &lt;Tag&gt; „Zitat“ A\u{a0}B"]
        );
        assert_eq!(strip_tags("<b>A</b><br>B &amp; C"), "A B & C");
    }

    #[test]
    fn lists_and_tables() {
        assert_eq!(
            text_blocks(
                "Voraussetzungen:<ol><li>Schuldverhältnis</li><li>Pflichtverletzung</li></ol>"
            ),
            ["Voraussetzungen:\n1. Schuldverhältnis\n2. Pflichtverletzung"]
        );
        assert_eq!(
            text_blocks("<ul><li>A<ul><li>B</li></ul></li></ul>"),
            ["• A\n\u{2003}• B"]
        );
        assert_eq!(
            text_blocks("<table><tr><th>Norm</th><td>Inhalt</td></tr></table>"),
            ["<b>Norm</b>  ·  Inhalt"]
        );
    }

    #[test]
    fn styles_are_translated_and_scripts_dropped() {
        assert_eq!(
            text_blocks("<style>.card{color:red}</style><span style=\"color: #c00; font-weight: bold\">wichtig</span>"),
            ["<span foreground=\"#cc0000\"><b>wichtig</b></span>"]
        );
        // Weiße Schrift wäre auf der Karte unsichtbar.
        assert_eq!(text_blocks("<font color=\"#ffffff\">hell</font>"), ["hell"]);
        assert_eq!(
            text_blocks("<span class=\"cloze\">[…]</span>"),
            ["<span foreground=\"#1a5fb4\"><b>[…]</b></span>"]
        );
    }

    #[test]
    fn images_and_rules_split_the_text() {
        assert_eq!(
            text_blocks("<b>Schema<img src=\"pr%C3%BCfung.png\">weiter</b><hr>Ende [sound:a.mp3]"),
            [
                "<b>Schema</b>",
                "[Bild prüfung.png]",
                "<b>weiter</b>",
                "---",
                "Ende"
            ]
        );
        assert_eq!(
            image_sources(
                "<img src='a.png'><img src=\"https://example.org/b.png\"><img src=a.png>"
            ),
            ["a.png"]
        );
    }

    #[test]
    fn unbalanced_markup_stays_well_formed() {
        assert_eq!(
            text_blocks("<b>fett <i>beides</b> Rest</i>"),
            ["<b>fett <i>beides</i></b> Rest"]
        );
        assert_eq!(text_blocks("a < b und c > d"), ["a &lt; b und c &gt; d"]);
    }

    #[test]
    fn citations_become_links() {
        let rendered = render("Anspruch aus § 433 Abs. 2 BGB, vgl. § 311 ZPO.");
        assert_eq!(
            rendered.blocks,
            [Block::Text(
                "Anspruch aus <a href=\"ref:0\">§\u{a0}433 Abs. 2</a> BGB, vgl. <a href=\"ref:1\">§\u{a0}311</a> ZPO."
                    .into()
            )]
        );
        assert_eq!(rendered.links[0].norm.as_deref(), Some("§ 433"));
        assert_eq!(rendered.links[0].paragraph.as_deref(), Some("2"));
        assert_eq!(rendered.links[0].law, LawRef::Abbrev("BGB".into()));
        assert_eq!(rendered.links[1].law, LawRef::Abbrev("ZPO".into()));
    }

    #[test]
    fn short_citations_with_roman_paragraphs() {
        let rendered = render("§ 812 I 1 Alt. 1 BGB und §§ 280 I, III, 281 BGB; § 985");
        let Block::Text(markup) = &rendered.blocks[0] else {
            panic!("Textblock erwartet");
        };
        assert_eq!(
            markup,
            "<a href=\"ref:0\">§\u{a0}812 I 1</a> Alt. 1 BGB und <a href=\"ref:1\">§§\u{a0}280 I</a>, III, 281 BGB; \
             <a href=\"ref:2\">§\u{a0}985</a>"
        );
        let first = &rendered.links[0];
        assert_eq!(
            (first.paragraph.as_deref(), first.sentence.as_deref()),
            (Some("1"), Some("1"))
        );
        assert_eq!(first.law, LawRef::Abbrev("BGB".into()));
        assert_eq!(rendered.links[1].paragraph.as_deref(), Some("1"));
        assert_eq!(rendered.links[1].law, LawRef::Abbrev("BGB".into()));
        // Ohne Kürzel bleibt das Gesetz offen.
        assert_eq!(rendered.links[2].law, LawRef::Same);
    }

    #[test]
    fn roman_numerals() {
        assert_eq!(roman_value("IV"), Some(4));
        assert_eq!(roman_value("XIX"), Some(19));
        assert_eq!(roman_value("IIII"), None);
        assert_eq!(roman_value("BGB"), None);
        assert!(looks_like_law("StGB"));
        assert!(!looks_like_law("II"));
        assert!(!looks_like_law("Satz"));
    }
}
