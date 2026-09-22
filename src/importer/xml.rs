//! Parser für Dokumente nach der DTD `gii-norm` (gesetze-im-internet.de).
//!
//! Die Datei wird in einen einfachen DOM gelesen und anschließend in das
//! Block-Modell aus [`crate::model::text`] übersetzt.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::model::text::{spans_text, Block, Cell, Footnote, ListItem, Span, Style};

#[derive(Debug)]
pub enum XmlError {
    Xml(String),
    Invalid(String),
}

impl From<quick_xml::Error> for XmlError {
    fn from(e: quick_xml::Error) -> Self {
        XmlError::Xml(e.to_string())
    }
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlError::Xml(s) => write!(f, "XML-Fehler: {s}"),
            XmlError::Invalid(s) => write!(f, "Ungültiges Dokument: {s}"),
        }
    }
}
impl std::error::Error for XmlError {}

// ---------------------------------------------------------------------------
// DOM
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// Gesamter Textinhalt (rekursiv, ohne Auszeichnung).
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for child in &self.children {
            match child {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) if e.name == "BR" => out.push('\n'),
                Node::Element(e) => e.collect_text(out),
            }
        }
    }
}

/// Liest ein XML-Dokument in einen DOM. Externe DTDs werden nicht aufgelöst.
pub fn parse_dom(xml: &str) -> Result<Element, XmlError> {
    let mut reader = Reader::from_str(xml);
    let config = reader.config_mut();
    config.trim_text(false);
    config.expand_empty_elements = false;

    let mut stack: Vec<Element> = vec![Element {
        name: "#root".into(),
        ..Default::default()
    }];

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let elem = start_element(&e)?;
                stack.push(elem);
            }
            Event::Empty(e) => {
                let elem = start_element(&e)?;
                stack
                    .last_mut()
                    .expect("stack")
                    .children
                    .push(Node::Element(elem));
            }
            Event::End(_) => {
                let elem = stack.pop().ok_or_else(|| XmlError::Invalid("Ende ohne Anfang".into()))?;
                stack
                    .last_mut()
                    .ok_or_else(|| XmlError::Invalid("Wurzel geschlossen".into()))?
                    .children
                    .push(Node::Element(elem));
            }
            Event::Text(t) => {
                let text = t.decode()?.into_owned();
                let text = quick_xml::escape::unescape(&text)
                    .map(|c| c.into_owned())
                    .unwrap_or(text);
                push_text(stack.last_mut().expect("stack"), &text);
            }
            Event::CData(t) => {
                let text = String::from_utf8_lossy(&t).into_owned();
                push_text(stack.last_mut().expect("stack"), &text);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop().ok_or_else(|| XmlError::Invalid("leer".into()))?;
    if !stack.is_empty() {
        return Err(XmlError::Invalid("Elemente nicht geschlossen".into()));
    }
    root.elements()
        .next()
        .cloned()
        .ok_or_else(|| XmlError::Invalid("kein Wurzelelement".into()))
        .map(|e| {
            root.children.clear();
            e
        })
}

fn start_element(e: &quick_xml::events::BytesStart<'_>) -> Result<Element, XmlError> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut attrs = HashMap::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|e| XmlError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = attr
            .unescape_value()
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(&attr.value).into_owned());
        attrs.insert(key, value);
    }
    Ok(Element {
        name,
        attrs,
        children: Vec::new(),
    })
}

fn push_text(parent: &mut Element, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Node::Text(last)) = parent.children.last_mut() {
        last.push_str(text);
    } else {
        parent.children.push(Node::Text(text.to_owned()));
    }
}

// ---------------------------------------------------------------------------
// Fachliche Struktur
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLawMeta {
    pub doknr: String,
    pub builddate: String,
    pub jurabk: String,
    pub amtabk: Option<String>,
    pub title: String,
    pub ausfertigung_datum: Option<String>,
    pub fundstelle: Option<String>,
    pub stand: Option<String>,
    pub neufassung: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedUnit {
    pub doknr: String,
    pub kennzahl: String,
    pub bez: String,
    pub titel: Option<String>,
    /// Index der übergeordneten Einheit in `ParsedLaw::units`.
    pub parent: Option<usize>,
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedNorm {
    pub doknr: String,
    pub enbez: Option<String>,
    pub titel: Option<String>,
    /// Index der umgebenden Gliederungseinheit in `ParsedLaw::units`.
    pub unit: Option<usize>,
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub fussnoten: Vec<Block>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedLaw {
    pub meta: ParsedLawMeta,
    pub units: Vec<ParsedUnit>,
    /// Normen in Dokumentreihenfolge (einschließlich Rahmennorm und Inhaltsübersicht).
    pub norms: Vec<ParsedNorm>,
}

/// Liest nur die Metadaten (für den Aktualisierungsvergleich).
pub fn parse_meta(xml: &str) -> Result<ParsedLawMeta, XmlError> {
    // Nur den ersten <norm>-Block betrachten, um nicht die ganze Datei zu parsen.
    let end = xml.find("</norm>").map(|i| i + "</norm>".len()).unwrap_or(xml.len());
    let head = &xml[..end];
    let head = format!("{head}</dokumente>");
    let dom = parse_dom(&head)?;
    let first = dom
        .child("norm")
        .ok_or_else(|| XmlError::Invalid("keine Norm".into()))?;
    Ok(law_meta(&dom, first))
}

/// Parst ein vollständiges Gesetz.
pub fn parse_law(xml: &str) -> Result<ParsedLaw, XmlError> {
    let dom = parse_dom(xml)?;
    if dom.name != "dokumente" {
        return Err(XmlError::Invalid(format!(
            "Wurzelelement {} statt dokumente",
            dom.name
        )));
    }
    let norms: Vec<&Element> = dom.elements().filter(|e| e.name == "norm").collect();
    let first = norms
        .first()
        .ok_or_else(|| XmlError::Invalid("keine Norm".into()))?;
    let meta = law_meta(&dom, first);

    let mut law = ParsedLaw {
        meta,
        ..Default::default()
    };
    // Stapel offener Gliederungseinheiten: (Index, Kennzahl)
    let mut open: Vec<(usize, String)> = Vec::new();

    for norm in norms {
        let md = norm
            .child("metadaten")
            .ok_or_else(|| XmlError::Invalid("norm ohne metadaten".into()))?;
        let doknr = norm.attr("doknr").unwrap_or_default().to_owned();
        let textdaten = norm.child("textdaten");

        if let Some(unit) = md.child("gliederungseinheit") {
            let kennzahl = unit
                .child("gliederungskennzahl")
                .map(|e| e.text().trim().to_owned())
                .unwrap_or_default();
            while let Some((_, k)) = open.last() {
                if kennzahl.len() > k.len() && kennzahl.starts_with(k.as_str()) {
                    break;
                }
                open.pop();
            }
            let parent = open.last().map(|(i, _)| *i);
            let depth = open.len();
            let bez = unit
                .child("gliederungsbez")
                .map(|e| e.text().trim().to_owned())
                .unwrap_or_default();
            let titel = unit
                .child("gliederungstitel")
                .map(|e| normalize_ws(&e.text()))
                .filter(|t| !t.is_empty());
            law.units.push(ParsedUnit {
                doknr: doknr.clone(),
                kennzahl: kennzahl.clone(),
                bez,
                titel,
                parent,
                depth,
            });
            open.push((law.units.len() - 1, kennzahl));
            // Gliederungsnormen tragen in der Regel keinen eigenen Text.
            let has_text = textdaten
                .and_then(|t| t.child("text"))
                .map(|t| !t.text().trim().is_empty())
                .unwrap_or(false);
            if !has_text {
                continue;
            }
        }

        let enbez = md
            .child("enbez")
            .map(|e| normalize_ws(&e.text()))
            .filter(|s| !s.is_empty());
        let titel = md
            .child("titel")
            .map(|e| normalize_ws(&e.text()))
            .filter(|s| !s.is_empty());

        let mut blocks = Vec::new();
        let mut footnotes = Vec::new();
        let mut fussnoten = Vec::new();
        if let Some(td) = textdaten {
            if let Some(text) = td.child("text") {
                if let Some(content) = text.child("Content") {
                    blocks = content_blocks(content);
                }
                if let Some(toc) = text.child("TOC") {
                    blocks.extend(toc_blocks(toc));
                }
                if let Some(fns) = text.child("Footnotes") {
                    footnotes = parse_footnotes(fns);
                }
            }
            if let Some(fn_section) = td.child("fussnoten") {
                if let Some(content) = fn_section.child("Content") {
                    fussnoten = content_blocks(content);
                }
                if let Some(fns) = fn_section.child("Footnotes") {
                    footnotes.extend(parse_footnotes(fns));
                }
            }
        }

        law.norms.push(ParsedNorm {
            doknr,
            enbez,
            titel,
            unit: open.last().map(|(i, _)| *i),
            blocks,
            footnotes,
            fussnoten,
        });
    }
    Ok(law)
}

fn law_meta(dom: &Element, first: &Element) -> ParsedLawMeta {
    let md = first.child("metadaten");
    let get = |name: &str| md.and_then(|m| m.child(name)).map(|e| normalize_ws(&e.text()));
    let mut meta = ParsedLawMeta {
        doknr: dom
            .attr("doknr")
            .or(first.attr("doknr"))
            .unwrap_or_default()
            .to_owned(),
        builddate: dom
            .attr("builddate")
            .or(first.attr("builddate"))
            .unwrap_or_default()
            .to_owned(),
        jurabk: get("jurabk").unwrap_or_default(),
        amtabk: get("amtabk"),
        title: get("langue").or_else(|| get("kurzue")).unwrap_or_default(),
        ausfertigung_datum: get("ausfertigung-datum"),
        fundstelle: None,
        stand: None,
        neufassung: None,
    };
    if let Some(md) = md {
        if let Some(fs) = md.child("fundstelle") {
            let periodikum = fs.child("periodikum").map(|e| e.text()).unwrap_or_default();
            let zit = fs.child("zitstelle").map(|e| e.text()).unwrap_or_default();
            meta.fundstelle = Some(normalize_ws(&format!("{periodikum} {zit}")));
        }
        for stand in md.elements().filter(|e| e.name == "standangabe") {
            let typ = stand.child("standtyp").map(|e| e.text()).unwrap_or_default();
            let kommentar = stand
                .child("standkommentar")
                .map(|e| normalize_ws(&e.text()))
                .unwrap_or_default();
            match typ.trim() {
                "Stand" => meta.stand = Some(kommentar),
                "Neuf" => meta.neufassung = Some(kommentar),
                _ => {}
            }
        }
    }
    meta
}

// ---------------------------------------------------------------------------
// Blöcke
// ---------------------------------------------------------------------------

fn content_blocks(content: &Element) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut pending: Vec<Span> = Vec::new();
    for node in &content.children {
        match node {
            Node::Text(t) => {
                if !t.trim().is_empty() {
                    pending.push(Span::plain(normalize_ws(t)));
                }
            }
            Node::Element(e) => match e.name.as_str() {
                "P" => {
                    flush(&mut pending, &mut blocks);
                    blocks.extend(paragraph_blocks(e));
                }
                "BR" => {}
                "table" => {
                    flush(&mut pending, &mut blocks);
                    blocks.push(table_block(e));
                }
                "TOC" => {
                    flush(&mut pending, &mut blocks);
                    blocks.extend(toc_blocks(e));
                }
                "Revision" => {
                    flush(&mut pending, &mut blocks);
                    blocks.extend(content_blocks(e));
                }
                "Title" | "Subtitle" => {
                    flush(&mut pending, &mut blocks);
                    blocks.push(Block::Heading {
                        level: 1,
                        spans: inline_spans(e, Style::default()),
                    });
                }
                "kommentar" => {
                    flush(&mut pending, &mut blocks);
                    blocks.push(Block::Paragraph {
                        spans: vec![Span {
                            text: normalize_ws(&e.text()),
                            style: Style {
                                italic: true,
                                ..Default::default()
                            },
                            footnote: None,
                        }],
                    });
                }
                "AttArea" | "FnArea" => {}
                _ => {
                    flush(&mut pending, &mut blocks);
                    blocks.extend(paragraph_blocks(e));
                }
            },
        }
    }
    flush(&mut pending, &mut blocks);
    blocks
}

fn flush(pending: &mut Vec<Span>, blocks: &mut Vec<Block>) {
    if !pending.is_empty() {
        blocks.push(Block::Paragraph {
            spans: std::mem::take(pending),
        });
    }
}

/// Ein `<P>` (oder `<LA>`) kann neben Text auch Listen, Tabellen und
/// vorformatierte Abschnitte enthalten; daraus entstehen mehrere Blöcke.
/// Hängen Text und Liste zusammen („… folgender Richtlinien: <DL>“),
/// bleibt die Reihenfolge erhalten.
fn paragraph_blocks(p: &Element) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    collect_paragraph(p, Style::default(), &mut spans, &mut blocks);
    trim_spans(&mut spans);
    if !spans.is_empty() || blocks.is_empty() {
        blocks.insert(0, Block::Paragraph { spans });
    }
    // Wurde die Liste mitten im Absatz eingefügt, steht der Einleitungssatz
    // an erster Stelle; nachfolgende Blöcke folgen in Dokumentreihenfolge.
    blocks
}

fn collect_paragraph(e: &Element, style: Style, spans: &mut Vec<Span>, blocks: &mut Vec<Block>) {
    for node in &e.children {
        match node {
            Node::Text(t) => push_span(spans, t, style, None),
            Node::Element(c) => match c.name.as_str() {
                "BR" => push_span(spans, "\n", style, None),
                "B" => collect_paragraph(c, Style { bold: true, ..style }, spans, blocks),
                "I" => collect_paragraph(c, Style { italic: true, ..style }, spans, blocks),
                "U" => collect_paragraph(c, Style { underline: true, ..style }, spans, blocks),
                "SUP" => collect_paragraph(c, Style { sup: true, ..style }, spans, blocks),
                "SUB" => collect_paragraph(c, Style { sub: true, ..style }, spans, blocks),
                "small" => collect_paragraph(c, Style { small: true, ..style }, spans, blocks),
                "FnR" => {
                    if let Some(id) = c.attr("ID") {
                        push_span(spans, "*", Style { sup: true, ..style }, Some(id.to_owned()));
                    }
                }
                "FnArea" => {
                    for r in c.elements().filter(|r| r.name == "FnR") {
                        if let Some(id) = r.attr("ID") {
                            push_span(spans, "*", Style { sup: true, ..style }, Some(id.to_owned()));
                        }
                    }
                }
                "DL" => blocks.push(list_block(c)),
                "table" => blocks.push(table_block(c)),
                "pre" => blocks.push(Block::Pre {
                    text: c.text().trim_matches('\n').to_owned(),
                }),
                "Revision" => blocks.extend(content_blocks(c)),
                "Split" | "IMG" | "FILE" | "QuoteL" | "QuoteR" | "ABWFORMAT" | "Accolade"
                | "AttR" => {}
                "kommentar" => push_span(
                    spans,
                    &c.text(),
                    Style {
                        italic: true,
                        ..style
                    },
                    None,
                ),
                // NB, noindex, Citation, SP, F, FNA, LA, P, Ident, Title …
                _ => collect_paragraph(c, style, spans, blocks),
            },
        }
    }
}

fn push_span(spans: &mut Vec<Span>, text: &str, style: Style, footnote: Option<String>) {
    let text = collapse_ws(text);
    if text.is_empty() {
        return;
    }
    if footnote.is_none() {
        if let Some(last) = spans.last_mut() {
            if last.style == style && last.footnote.is_none() {
                last.text.push_str(&text);
                return;
            }
        }
    }
    spans.push(Span {
        text,
        style,
        footnote,
    });
}

/// Fasst Whitespace zusammen, behält aber Zeilenumbrüche aus `<BR/>`.
fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for c in text.chars() {
        if c == '\n' {
            out.push('\n');
            prev_space = true;
        } else if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

fn trim_spans(spans: &mut Vec<Span>) {
    if let Some(first) = spans.first_mut() {
        let t = first.text.trim_start().to_owned();
        first.text = t;
    }
    if let Some(last) = spans.last_mut() {
        let t = last.text.trim_end().to_owned();
        last.text = t;
    }
    spans.retain(|s| !s.text.is_empty() || s.footnote.is_some());
}

/// Inline-Inhalt ohne Blockelemente (Titel, Bezeichner, Zellen).
fn inline_spans(e: &Element, style: Style) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut blocks = Vec::new();
    collect_paragraph(e, style, &mut spans, &mut blocks);
    trim_spans(&mut spans);
    spans
}

fn list_block(dl: &Element) -> Block {
    let kind = dl.attr("Type").unwrap_or("arabic").to_owned();
    let mut items = Vec::new();
    let mut label: Option<Vec<Span>> = None;
    for e in dl.elements() {
        match e.name.as_str() {
            "DT" => {
                if let Some(l) = label.take() {
                    items.push(ListItem {
                        label: l,
                        blocks: Vec::new(),
                    });
                }
                label = Some(inline_spans(e, Style::default()));
            }
            "DD" => {
                let mut blocks = Vec::new();
                for la in e.elements() {
                    match la.name.as_str() {
                        "LA" => blocks.extend(paragraph_blocks(la)),
                        "Revision" => blocks.extend(content_blocks(la)),
                        _ => {}
                    }
                }
                items.push(ListItem {
                    label: label.take().unwrap_or_default(),
                    blocks,
                });
            }
            _ => {}
        }
    }
    if let Some(l) = label.take() {
        items.push(ListItem {
            label: l,
            blocks: Vec::new(),
        });
    }
    Block::List { kind, items }
}

fn table_block(table: &Element) -> Block {
    let title = table
        .child("Title")
        .map(|t| normalize_ws(&t.text()))
        .filter(|t| !t.is_empty());
    let mut rows = Vec::new();
    let mut header_rows = 0;
    for tgroup in table.elements().filter(|e| e.name == "tgroup") {
        let cols: Vec<String> = tgroup
            .elements()
            .filter(|e| e.name == "colspec")
            .map(|c| c.attr("colname").unwrap_or_default().to_owned())
            .collect();
        for section in tgroup.elements() {
            match section.name.as_str() {
                "thead" => {
                    for row in section.elements().filter(|e| e.name == "row") {
                        rows.push(table_row(row, &cols));
                        header_rows += 1;
                    }
                }
                "tbody" | "tfoot" => {
                    for row in section.elements().filter(|e| e.name == "row") {
                        rows.push(table_row(row, &cols));
                    }
                }
                _ => {}
            }
        }
    }
    Block::Table {
        title,
        header_rows,
        rows,
    }
}

fn table_row(row: &Element, cols: &[String]) -> Vec<Cell> {
    row.elements()
        .filter(|e| e.name == "entry")
        .map(|entry| {
            let colspan = match (entry.attr("namest"), entry.attr("nameend")) {
                (Some(a), Some(b)) => {
                    let ia = cols.iter().position(|c| c == a);
                    let ib = cols.iter().position(|c| c == b);
                    match (ia, ib) {
                        (Some(ia), Some(ib)) if ib >= ia => (ib - ia + 1) as u32,
                        _ => 1,
                    }
                }
                _ => 1,
            };
            let rowspan = entry
                .attr("morerows")
                .and_then(|m| m.trim().parse::<u32>().ok())
                .map(|m| m + 1)
                .unwrap_or(1);
            let mut blocks = Vec::new();
            let has_block_children = entry
                .elements()
                .any(|e| matches!(e.name.as_str(), "P" | "DL" | "table" | "Title" | "Ident"));
            if has_block_children {
                blocks = content_blocks(entry);
            } else {
                let spans = inline_spans(entry, Style::default());
                if !spans.is_empty() {
                    blocks.push(Block::Paragraph { spans });
                }
            }
            Cell {
                blocks,
                colspan,
                rowspan,
            }
        })
        .collect()
}

fn toc_blocks(toc: &Element) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut ident: Option<Vec<Span>> = None;
    for e in toc.elements() {
        match e.name.as_str() {
            "Ident" => {
                if let Some(i) = ident.take() {
                    blocks.push(Block::Heading {
                        level: toc_level(e),
                        spans: i,
                    });
                }
                ident = Some(inline_spans(e, Style::default()));
            }
            "Title" => {
                let mut spans = ident.take().unwrap_or_default();
                if !spans.is_empty() {
                    spans.push(Span::plain(" "));
                }
                spans.extend(inline_spans(e, Style::default()));
                blocks.push(Block::Heading {
                    level: toc_level(e),
                    spans,
                });
            }
            "P" => blocks.extend(paragraph_blocks(e)),
            "table" => blocks.push(table_block(e)),
            _ => {}
        }
    }
    if let Some(i) = ident.take() {
        blocks.push(Block::Heading { level: 1, spans: i });
    }
    blocks
}

fn toc_level(e: &Element) -> u8 {
    // Class="S5" (Buch) … "S1" (Kapitel); ohne Angabe die niedrigste Ebene.
    e.attr("Class")
        .and_then(|c| c.strip_prefix('S'))
        .and_then(|n| n.parse::<u8>().ok())
        .unwrap_or(1)
}

fn parse_footnotes(fns: &Element) -> Vec<Footnote> {
    fns.elements()
        .filter(|e| e.name == "Footnote")
        .map(|f| {
            let mark = f.attr("FnZ").unwrap_or("*").to_owned();
            Footnote {
                id: f.attr("ID").unwrap_or_default().to_owned(),
                mark,
                blocks: paragraph_blocks(f),
            }
        })
        .collect()
}

/// Whitespace zusammenfassen und trimmen (für Metadaten).
pub fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[allow(dead_code)]
pub fn spans_to_text(spans: &[Span]) -> String {
    spans_text(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::text::flatten_blocks;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?><!DOCTYPE dokumente SYSTEM "https://www.gesetze-im-internet.de/dtd/1.01/gii-norm.dtd">
<dokumente builddate="20260910215507" doknr="BJNR001950896"><norm builddate="20260910215507" doknr="BJNR001950896"><metadaten><jurabk>BGB</jurabk><amtabk>BGB</amtabk><ausfertigung-datum manuell="ja">1896-08-18</ausfertigung-datum><fundstelle typ="amtlich"><periodikum>RGBl</periodikum><zitstelle>1896, 195</zitstelle></fundstelle><langue>Bürgerliches Gesetzbuch</langue><standangabe checked="ja"><standtyp>Neuf</standtyp><standkommentar>Neugefasst durch Bek. v. 2.1.2002 I 42</standkommentar></standangabe><standangabe checked="ja"><standtyp>Stand</standtyp><standkommentar>zuletzt geändert durch Art. 6 G v. 23.7.2026 I Nr. 226</standkommentar></standangabe></metadaten><textdaten><text format="XML"><Content><P>Dieses Gesetz dient der Umsetzung: <DL Font="normal" Type="arabic"><DT>1.</DT><DD Font="normal"><LA Size="normal">Richtlinie A,</LA></DD><DT>2.</DT><DD Font="normal"><LA Size="normal">Richtlinie B.</LA></DD></DL></P></Content></text><fussnoten><Content><P><BR/> <pre xml:space="preserve">(+++ Textnachweis Geltung ab: 1.1.1980 +++)<BR/></pre></P></Content></fussnoten></textdaten></norm>
<norm doknr="BJNR001950896BJNE262685360"><metadaten><jurabk>BGB</jurabk><enbez>Inhaltsübersicht</enbez></metadaten><textdaten><text format="XML"><TOC><Ident Class="S5">Buch 1<BR/><BR/></Ident><Title Align="auto" Class="S5">Allgemeiner Teil<BR/><BR/><BR/></Title></TOC></text><fussnoten/></textdaten></norm>
<norm doknr="BJNR001950896BJNG000102377"><metadaten><jurabk>BGB</jurabk><gliederungseinheit><gliederungskennzahl>010</gliederungskennzahl><gliederungsbez>Buch 1</gliederungsbez><gliederungstitel>Allgemeiner Teil</gliederungstitel></gliederungseinheit></metadaten><textdaten><text format="XML"><Content><P/></Content></text><fussnoten/></textdaten></norm>
<norm doknr="BJNR001950896BJNG000202377"><metadaten><jurabk>BGB</jurabk><gliederungseinheit><gliederungskennzahl>010010</gliederungskennzahl><gliederungsbez>Abschnitt 1</gliederungsbez><gliederungstitel>Personen</gliederungstitel></gliederungseinheit></metadaten><textdaten><text format="XML"><Content><P/></Content></text><fussnoten/></textdaten></norm>
<norm doknr="BJNR001950896BJNE000102377"><metadaten><jurabk>BGB</jurabk><enbez>§ 1</enbez><titel format="XML">Beginn der Rechtsfähigkeit</titel></metadaten><textdaten><text format="XML"><Content><P>Die Rechtsfähigkeit des Menschen beginnt mit der Vollendung der Geburt.</P></Content></text><fussnoten/></textdaten></norm>
<norm doknr="BJNR001950896BJNG000302377"><metadaten><jurabk>BGB</jurabk><gliederungseinheit><gliederungskennzahl>020</gliederungskennzahl><gliederungsbez>Buch 2</gliederungsbez><gliederungstitel>Recht der Schuldverhältnisse</gliederungstitel></gliederungseinheit></metadaten><textdaten><text format="XML"><Content><P/></Content></text><fussnoten/></textdaten></norm>
<norm doknr="BJNR001950896BJNE244701377"><metadaten><jurabk>BGB</jurabk><enbez>§ 14</enbez><titel format="XML">Unternehmer</titel></metadaten><textdaten><text format="XML"><Content><P><FnR ID="F1"/></P><P>(1) Unternehmer ist eine <I>natürliche</I> Person.</P><P>(2) Eine rechtsfähige Personengesellschaft.</P><table><tgroup cols="2"><colspec colname="c1"/><colspec colname="c2"/><thead><row><entry>Kopf A</entry><entry>Kopf B</entry></row></thead><tbody><row><entry namest="c1" nameend="c2">Breit</entry></row></tbody></tgroup></table></Content><Footnotes><Footnote FnZ="*" ID="F1"><B>Amtlicher Hinweis:</B><BR/>Dient der Umsetzung.</Footnote></Footnotes></text><fussnoten/></textdaten></norm>
</dokumente>"#;

    #[test]
    fn parses_meta() {
        let meta = parse_meta(SAMPLE).unwrap();
        assert_eq!(meta.jurabk, "BGB");
        assert_eq!(meta.title, "Bürgerliches Gesetzbuch");
        assert_eq!(meta.builddate, "20260910215507");
        assert_eq!(meta.doknr, "BJNR001950896");
        assert_eq!(meta.fundstelle.as_deref(), Some("RGBl 1896, 195"));
        assert_eq!(
            meta.stand.as_deref(),
            Some("zuletzt geändert durch Art. 6 G v. 23.7.2026 I Nr. 226")
        );
        assert_eq!(
            meta.neufassung.as_deref(),
            Some("Neugefasst durch Bek. v. 2.1.2002 I 42")
        );
        assert_eq!(meta.ausfertigung_datum.as_deref(), Some("1896-08-18"));
    }

    #[test]
    fn builds_unit_tree_and_assigns_norms() {
        let law = parse_law(SAMPLE).unwrap();
        assert_eq!(law.units.len(), 3);
        assert_eq!(law.units[0].bez, "Buch 1");
        assert_eq!(law.units[0].parent, None);
        assert_eq!(law.units[1].bez, "Abschnitt 1");
        assert_eq!(law.units[1].parent, Some(0));
        assert_eq!(law.units[1].depth, 1);
        assert_eq!(law.units[2].bez, "Buch 2");
        assert_eq!(law.units[2].parent, None);

        let names: Vec<_> = law.norms.iter().map(|n| n.enbez.clone()).collect();
        assert_eq!(
            names,
            vec![None, Some("Inhaltsübersicht".into()), Some("§ 1".into()), Some("§ 14".into())]
        );
        let p1 = law.norms.iter().find(|n| n.enbez.as_deref() == Some("§ 1")).unwrap();
        assert_eq!(p1.unit, Some(1));
        assert_eq!(p1.titel.as_deref(), Some("Beginn der Rechtsfähigkeit"));
        let p14 = law.norms.iter().find(|n| n.enbez.as_deref() == Some("§ 14")).unwrap();
        assert_eq!(p14.unit, Some(2));
    }

    #[test]
    fn parses_paragraphs_styles_footnotes_and_tables() {
        let law = parse_law(SAMPLE).unwrap();
        let p14 = law.norms.iter().find(|n| n.enbez.as_deref() == Some("§ 14")).unwrap();
        assert_eq!(p14.blocks.len(), 4);
        match &p14.blocks[0] {
            Block::Paragraph { spans } => {
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].footnote.as_deref(), Some("F1"));
            }
            other => panic!("unexpected {other:?}"),
        }
        match &p14.blocks[1] {
            Block::Paragraph { spans } => {
                assert_eq!(spans[0].text, "(1) Unternehmer ist eine ");
                assert!(spans[1].style.italic);
                assert_eq!(spans[1].text, "natürliche");
                assert_eq!(spans[2].text, " Person.");
            }
            other => panic!("unexpected {other:?}"),
        }
        match &p14.blocks[3] {
            Block::Table {
                header_rows, rows, ..
            } => {
                assert_eq!(*header_rows, 1);
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[1][0].colspan, 2);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(p14.footnotes.len(), 1);
        assert_eq!(p14.footnotes[0].mark, "*");
        let fl = flatten_blocks(&p14.footnotes[0].blocks);
        assert_eq!(fl.text(), "Amtlicher Hinweis:\nDient der Umsetzung.\n");
    }

    #[test]
    fn parses_list_inside_paragraph_and_pre_in_fussnoten() {
        let law = parse_law(SAMPLE).unwrap();
        let root = &law.norms[0];
        assert_eq!(root.blocks.len(), 2);
        match &root.blocks[0] {
            Block::Paragraph { spans } => {
                assert_eq!(spans[0].text, "Dieses Gesetz dient der Umsetzung:")
            }
            other => panic!("unexpected {other:?}"),
        }
        match &root.blocks[1] {
            Block::List { kind, items } => {
                assert_eq!(kind, "arabic");
                assert_eq!(items.len(), 2);
                assert_eq!(spans_text(&items[1].label), "2.");
                assert_eq!(
                    items[1].blocks,
                    vec![Block::Paragraph {
                        spans: vec![Span::plain("Richtlinie B.")]
                    }]
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(root
            .fussnoten
            .iter()
            .any(|b| matches!(b, Block::Pre { text } if text.contains("Textnachweis"))));
    }

    #[test]
    fn parses_toc_headings() {
        let law = parse_law(SAMPLE).unwrap();
        let toc = &law.norms[1];
        assert_eq!(
            toc.blocks,
            vec![Block::Heading {
                level: 5,
                spans: vec![Span::plain("Buch 1 Allgemeiner Teil")]
            }]
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_law("<foo/>").is_err());
        assert!(parse_law("<dokumente><norm>").is_err());
    }

    #[test]
    fn dom_unescapes_entities() {
        let dom = parse_dom("<a x=\"1&amp;2\">Ein &quot;Zitat&quot; &amp; mehr<BR/>Z</a>").unwrap();
        assert_eq!(dom.attr("x"), Some("1&2"));
        assert_eq!(dom.text(), "Ein \"Zitat\" & mehr\nZ");
    }
}
