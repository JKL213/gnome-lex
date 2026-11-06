// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Parser für Dokumente nach der DTD `byrecht-norm` (BAYERN.RECHT,
//! gesetze-bayern.de). Das Portal liefert unter `/Content/Zip/<Dokument-ID>`
//! ein ZIP-Archiv mit `bayportalnorm/<ID>.xml`; dieses Modul übersetzt es in
//! dieselben Strukturen wie der gii-norm-Parser ([`super::xml`]), sodass
//! Datenbank und Ansicht keinen Unterschied kennen.
//!
//! Aufbau der Datei: `kopf` (Metadaten) und `rumpf` mit verschachtelten
//! `gliederung`-Elementen (`gliederung.titel`) und `einzelnorm`-Elementen
//! (`para.nr`, `para.titel`, `jurAbsatz` → `absatz.nr` + `absatz.text` mit
//! `p`, `ul`/`li`/`symbol`, `table`, `fn.call`). Anhänge stehen in `annex`.

use super::xml::{
    collapse_ws, flush_spans, normalize_ws, parse_dom, push_span, trim_spans, Element, Node,
    ParsedLaw, ParsedLawMeta, ParsedNorm, ParsedUnit, XmlError,
};
use crate::model::text::{Block, Cell, Footnote, ListItem, Span, Style};

/// Wurzelelement des Formats.
pub const ROOT: &str = "byrecht-norm";

/// Ist das Dokument ein `byrecht-norm`-Dokument?
pub fn is_byrecht(xml: &str) -> bool {
    let head = &xml[..xml.len().min(2048)];
    head.contains("<byrecht-norm")
}

/// Liest nur die Metadaten (für den Aktualisierungsvergleich).
///
/// `fallback_abbrev` wird als Kürzel verwendet, wenn das Dokument keine
/// amtliche Abkürzung nennt (z. B. die Verfassung).
pub fn parse_meta(xml: &str, fallback_abbrev: &str) -> Result<ParsedLawMeta, XmlError> {
    // Nur Kopf und Normzitat betrachten, nicht die ganze Datei.
    let end = xml
        .find("<gliederung ")
        .or_else(|| xml.find("<einzelnorm "))
        .unwrap_or(xml.len());
    let mut head = xml[..end].to_owned();
    // Offene Elemente (rumpf, byrecht-norm) schließen.
    if head.contains("<rumpf>") && !head.contains("</rumpf>") {
        head.push_str("</rumpf>");
    }
    head.push_str("</byrecht-norm>");
    let dom = parse_dom(&head)?;
    Ok(law_meta(&dom, fallback_abbrev))
}

/// Parst ein vollständiges Gesetz.
pub fn parse_law(xml: &str, fallback_abbrev: &str) -> Result<ParsedLaw, XmlError> {
    let dom = parse_dom(xml)?;
    if dom.name != ROOT {
        return Err(XmlError::Invalid(format!(
            "Wurzelelement {} statt {ROOT}",
            dom.name
        )));
    }
    let meta = law_meta(&dom, fallback_abbrev);
    let mut law = ParsedLaw {
        meta,
        ..Default::default()
    };
    let doknr = law.meta.doknr.clone();

    let rumpf = dom
        .child("rumpf")
        .ok_or_else(|| XmlError::Invalid("kein rumpf".into()))?;

    // Rahmennorm (ohne Bezeichnung) mit Einleitungssatz und Änderungsverlauf,
    // wie bei gesetze-im-internet.de; die Ansicht blendet sie aus.
    let mut frame_blocks = Vec::new();
    if let Some(intro) = rumpf
        .child("einleitungssatz")
        .and_then(|e| e.child("einleitungssatz.text"))
    {
        frame_blocks.extend(content_blocks(intro));
    }
    let mut frame_footnotes = Vec::new();
    if let Some(history) = rumpf.child("aenderungsverlauf") {
        frame_footnotes.extend(content_blocks(history));
    }
    law.norms.push(ParsedNorm {
        doknr: doknr.clone(),
        enbez: None,
        titel: Some(law.meta.title.clone()),
        unit: None,
        blocks: frame_blocks,
        footnotes: Vec::new(),
        fussnoten: frame_footnotes,
    });

    let mut ctx = Context {
        doknr: &doknr,
        law: &mut law,
        norm_counter: 0,
        footnote_counter: 0,
    };
    walk_body(rumpf, None, &mut ctx, &mut Vec::new());

    // Anhänge als eigene Normen hinter dem Rumpf.
    for annex in dom.elements().filter(|e| e.name == "annex") {
        let body = annex.child("annex.koerper").unwrap_or(annex);
        let number = body
            .child("annex.nummer")
            .map(|e| normalize_ws(&e.text()))
            .unwrap_or_default();
        let title = body
            .child("annex.titel")
            .map(|e| normalize_ws(&e.text()))
            .filter(|t| !t.is_empty());
        let enbez = if number.is_empty() {
            "Anlage".to_owned()
        } else {
            number
        };
        let mut blocks = Vec::new();
        let mut footnotes = Vec::new();
        for text in annex.elements().filter(|e| e.name == "annex.text") {
            let mut collector = BlockCollector::new(&mut ctx.footnote_counter);
            collector.content(text);
            blocks.extend(collector.blocks);
            footnotes.extend(collector.footnotes);
        }
        ctx.norm_counter += 1;
        let norm_doknr = format!("{}#A{}", ctx.doknr, ctx.norm_counter);
        ctx.law.norms.push(ParsedNorm {
            doknr: norm_doknr,
            enbez: Some(enbez),
            titel: title,
            unit: None,
            blocks,
            footnotes,
            fussnoten: Vec::new(),
        });
    }
    Ok(law)
}

struct Context<'a> {
    doknr: &'a str,
    law: &'a mut ParsedLaw,
    norm_counter: usize,
    footnote_counter: usize,
}

/// Läuft rekursiv durch `gliederung`- und `einzelnorm`-Elemente.
/// `path` enthält die laufenden Nummern je Ebene (für die Kennzahl).
fn walk_body(parent: &Element, unit: Option<usize>, ctx: &mut Context<'_>, path: &mut Vec<usize>) {
    let mut child_no = 0;
    for e in parent.elements() {
        match e.name.as_str() {
            "gliederung" => {
                child_no += 1;
                path.push(child_no);
                let kennzahl: String = path.iter().map(|n| format!("{n:03}")).collect();
                let (bez, titel) = unit_labels(e.child("gliederung.titel"));
                let depth = path.len() - 1;
                ctx.law.units.push(ParsedUnit {
                    doknr: e
                        .attr("gliederungsid")
                        .map(|id| format!("{}#{id}", ctx.doknr))
                        .unwrap_or_default(),
                    kennzahl,
                    bez,
                    titel,
                    parent: unit,
                    depth,
                });
                let index = ctx.law.units.len() - 1;
                walk_body(e, Some(index), ctx, path);
                path.pop();
            }
            "einzelnorm" => {
                ctx.norm_counter += 1;
                let norm = parse_norm(e, unit, ctx);
                ctx.law.norms.push(norm);
            }
            _ => {}
        }
    }
}

/// „Erster Teil ⏎ Wesen und Aufgaben“ → („Erster Teil“, Some(„Wesen und Aufgaben“)).
fn unit_labels(title: Option<&Element>) -> (String, Option<String>) {
    let Some(title) = title else {
        return (String::new(), None);
    };
    let text = title.text();
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let bez = lines.next().unwrap_or_default().to_owned();
    let rest: Vec<&str> = lines.collect();
    let titel = normalize_ws(&rest.join(" "));
    (bez, if titel.is_empty() { None } else { Some(titel) })
}

fn parse_norm(e: &Element, unit: Option<usize>, ctx: &mut Context<'_>) -> ParsedNorm {
    let mut collector = BlockCollector::new(&mut ctx.footnote_counter);
    // Bezeichnung ohne eingebettete Fußnoten („Art. 35<fn.call>…“).
    let enbez_spans = e
        .child("para.nr")
        .map(|p| collector.inline(p))
        .unwrap_or_default();
    let enbez = normalize_ws(&spans_plain_text(&enbez_spans));
    let titel = e.child("para.titel").map(|t| {
        let spans = collector.inline(t);
        normalize_ws(&spans_plain_text(&spans))
    });
    let titel = titel.filter(|t| !t.is_empty());
    let enbez = if enbez.is_empty() {
        titel
            .as_deref()
            .map(|t| t.trim_matches(|c| c == '[' || c == ']').to_owned())
            .filter(|t| !t.is_empty())
    } else {
        Some(enbez)
    };

    for absatz in e.elements().filter(|a| a.name == "jurAbsatz") {
        let nr = absatz
            .child("absatz.nr")
            .map(|n| normalize_ws(&n.text()))
            .unwrap_or_default();
        let Some(text) = absatz.child("absatz.text") else {
            continue;
        };
        let start = collector.blocks.len();
        collector.content(text);
        if !nr.is_empty() {
            prefix_first_paragraph(&mut collector.blocks[start..], &nr);
        }
    }
    let doknr = format!(
        "{}#{}",
        ctx.doknr,
        e.attr("einzelnormid")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("P{}", ctx.norm_counter))
    );
    ParsedNorm {
        doknr,
        enbez,
        titel,
        unit,
        blocks: collector.blocks,
        footnotes: collector.footnotes,
        fussnoten: Vec::new(),
    }
}

/// Stellt die Absatznummer „(1) “ dem ersten Absatzblock voran (so wie die
/// gii-norm-Texte sie tragen); fehlt ein Absatzblock, entsteht einer.
fn prefix_first_paragraph(blocks: &mut [Block], nr: &str) {
    for block in blocks.iter_mut() {
        if let Block::Paragraph { spans } = block {
            match spans.first_mut() {
                Some(first) if first.style.is_plain() && first.footnote.is_none() => {
                    first.text = format!("{nr} {}", first.text);
                }
                _ => spans.insert(0, Span::plain(format!("{nr} "))),
            }
            return;
        }
    }
}

fn spans_plain_text(spans: &[Span]) -> String {
    spans
        .iter()
        .filter(|s| s.footnote.is_none())
        .map(|s| s.text.as_str())
        .collect()
}

/// Sammelt Blöcke und Fußnoten aus `absatz.text`-artigen Inhalten.
struct BlockCollector<'a> {
    blocks: Vec<Block>,
    footnotes: Vec<Footnote>,
    footnote_counter: &'a mut usize,
}

impl<'a> BlockCollector<'a> {
    fn new(footnote_counter: &'a mut usize) -> Self {
        Self {
            blocks: Vec::new(),
            footnotes: Vec::new(),
            footnote_counter,
        }
    }

    /// Blockinhalt (`p`, `ul`, `table`, Text) in `self.blocks`.
    fn content(&mut self, e: &Element) {
        let mut blocks = Vec::new();
        self.collect_blocks(e, &mut blocks);
        self.blocks.extend(blocks);
    }

    fn collect_blocks(&mut self, e: &Element, blocks: &mut Vec<Block>) {
        let mut spans: Vec<Span> = Vec::new();
        for node in &e.children {
            match node {
                Node::Text(t) => push_span(&mut spans, t, Style::default(), None),
                Node::Element(c) => match c.name.as_str() {
                    "p" => {
                        flush_spans(&mut spans, blocks);
                        let mut inner = Vec::new();
                        self.collect_inline(c, Style::default(), &mut inner, blocks);
                        flush_spans(&mut inner, blocks);
                    }
                    "ul" | "ol" => {
                        flush_spans(&mut spans, blocks);
                        let list = self.list_block(c);
                        blocks.push(list);
                    }
                    "table" => {
                        flush_spans(&mut spans, blocks);
                        let table = self.table_block(c);
                        blocks.push(table);
                    }
                    "pre" => {
                        flush_spans(&mut spans, blocks);
                        blocks.push(Block::Pre {
                            text: c.text().trim_matches('\n').to_owned(),
                        });
                    }
                    "br" => push_span(&mut spans, "\n", Style::default(), None),
                    _ => self.collect_inline(c, Style::default(), &mut spans, blocks),
                },
            }
        }
        flush_spans(&mut spans, blocks);
    }

    /// Inline-Inhalt ohne Blockelemente (Bezeichner, Titel, Zellen).
    fn inline(&mut self, e: &Element) -> Vec<Span> {
        let mut spans = Vec::new();
        let mut blocks = Vec::new();
        self.collect_inline(e, Style::default(), &mut spans, &mut blocks);
        let mut all: Vec<Span> = blocks
            .into_iter()
            .filter_map(|b| match b {
                Block::Paragraph { spans } => Some(spans),
                _ => None,
            })
            .flatten()
            .collect();
        all.append(&mut spans);
        trim_spans(&mut all);
        all
    }

    fn collect_inline(
        &mut self,
        e: &Element,
        style: Style,
        spans: &mut Vec<Span>,
        blocks: &mut Vec<Block>,
    ) {
        for node in &e.children {
            match node {
                Node::Text(t) => push_span(spans, t, style, None),
                Node::Element(c) => match c.name.as_str() {
                    "br" => push_span(spans, "\n", style, None),
                    "satz.nr" => {
                        let text = collapse_ws(&c.text());
                        let text = text.trim();
                        if !text.is_empty() {
                            // Satznummern stehen hochgestellt vor dem Satz.
                            spans.push(Span {
                                text: text.to_owned(),
                                style: Style {
                                    sup: true,
                                    small: true,
                                    ..style
                                },
                                footnote: None,
                            });
                        }
                    }
                    "sup" => self.collect_inline(c, Style { sup: true, ..style }, spans, blocks),
                    "sub" => self.collect_inline(c, Style { sub: true, ..style }, spans, blocks),
                    "b" | "strong" => self.collect_inline(
                        c,
                        Style {
                            bold: true,
                            ..style
                        },
                        spans,
                        blocks,
                    ),
                    "i" | "em" => self.collect_inline(
                        c,
                        Style {
                            italic: true,
                            ..style
                        },
                        spans,
                        blocks,
                    ),
                    "u" => self.collect_inline(
                        c,
                        Style {
                            underline: true,
                            ..style
                        },
                        spans,
                        blocks,
                    ),
                    "span" => {
                        let class = c.attr("class").unwrap_or_default();
                        let inner = Style {
                            italic: style.italic || class.split_whitespace().any(|k| k == "i"),
                            bold: style.bold || class.split_whitespace().any(|k| k == "b"),
                            ..style
                        };
                        self.collect_inline(c, inner, spans, blocks);
                    }
                    "fn.call" => {
                        let mark = c
                            .child("fn.text")
                            .map(|t| normalize_ws(&t.text()))
                            .filter(|t| !t.is_empty())
                            .unwrap_or_else(|| "*".to_owned());
                        *self.footnote_counter += 1;
                        let id = format!("F{}", self.footnote_counter);
                        let mut fn_blocks = Vec::new();
                        if let Some(def) = c.child("fn.def") {
                            self.collect_blocks(def, &mut fn_blocks);
                        }
                        self.footnotes.push(Footnote {
                            id: id.clone(),
                            mark: mark.clone(),
                            blocks: fn_blocks,
                        });
                        spans.push(Span {
                            text: mark,
                            style: Style { sup: true, ..style },
                            footnote: Some(id),
                        });
                    }
                    "p" => {
                        // Absatz innerhalb von Inline-Kontext (z. B. in `li` ohne
                        // Listenbehandlung): als Zeilenumbruch behandeln.
                        if !spans.is_empty() {
                            push_span(spans, "\n", style, None);
                        }
                        self.collect_inline(c, style, spans, blocks);
                    }
                    "ul" | "ol" => {
                        flush_spans(spans, blocks);
                        let list = self.list_block(c);
                        blocks.push(list);
                    }
                    "table" => {
                        flush_spans(spans, blocks);
                        let table = self.table_block(c);
                        blocks.push(table);
                    }
                    // verweis.norm, v.norm, v.abk, a, …: nur der Text zählt.
                    _ => self.collect_inline(c, style, spans, blocks),
                },
            }
        }
    }

    fn list_block(&mut self, ul: &Element) -> Block {
        let mut items = Vec::new();
        let mut kind = String::from("arabic");
        for li in ul.elements().filter(|e| e.name == "li") {
            let mut label = Vec::new();
            let mut blocks = Vec::new();
            let mut spans: Vec<Span> = Vec::new();
            for node in &li.children {
                match node {
                    Node::Text(t) => push_span(&mut spans, t, Style::default(), None),
                    Node::Element(c) => match c.name.as_str() {
                        "symbol" => {
                            if label.is_empty() {
                                label = self.inline(c);
                            }
                        }
                        "p" => {
                            flush_spans(&mut spans, &mut blocks);
                            let mut inner = Vec::new();
                            self.collect_inline(c, Style::default(), &mut inner, &mut blocks);
                            flush_spans(&mut inner, &mut blocks);
                        }
                        "ul" | "ol" => {
                            flush_spans(&mut spans, &mut blocks);
                            let list = self.list_block(c);
                            blocks.push(list);
                        }
                        "table" => {
                            flush_spans(&mut spans, &mut blocks);
                            let table = self.table_block(c);
                            blocks.push(table);
                        }
                        _ => self.collect_inline(c, Style::default(), &mut spans, &mut blocks),
                    },
                }
            }
            flush_spans(&mut spans, &mut blocks);
            if items.is_empty() {
                kind = list_kind(&spans_plain_text(&label)).to_owned();
            }
            items.push(ListItem { label, blocks });
        }
        Block::List { kind, items }
    }

    fn table_block(&mut self, table: &Element) -> Block {
        let mut rows = Vec::new();
        let mut header_rows = 0;
        let mut row_elements: Vec<(&Element, bool)> = Vec::new();
        for section in table.elements() {
            match section.name.as_str() {
                "thead" => row_elements.extend(
                    section
                        .elements()
                        .filter(|e| e.name == "tr")
                        .map(|r| (r, true)),
                ),
                "tbody" | "tfoot" => row_elements.extend(
                    section
                        .elements()
                        .filter(|e| e.name == "tr")
                        .map(|r| (r, false)),
                ),
                "tr" => row_elements.push((section, false)),
                _ => {}
            }
        }
        for (tr, is_header) in row_elements {
            let cells: Vec<Cell> = tr
                .elements()
                .filter(|e| e.name == "td" || e.name == "th")
                .map(|cell| {
                    let span_attr = |name: &str| {
                        cell.attr(name)
                            .and_then(|v| v.trim().parse::<u32>().ok())
                            .filter(|v| *v >= 1)
                            .unwrap_or(1)
                    };
                    let mut blocks = Vec::new();
                    self.collect_blocks(cell, &mut blocks);
                    Cell {
                        blocks,
                        colspan: span_attr("colspan"),
                        rowspan: span_attr("rowspan"),
                    }
                })
                .collect();
            if is_header {
                header_rows += 1;
            }
            rows.push(cells);
        }
        Block::Table {
            title: None,
            header_rows,
            rows,
        }
    }
}

/// Listenart anhand des ersten Bezeichners („1.“, „a)“, „aa)“, „–“).
fn list_kind(label: &str) -> &'static str {
    let label = label.trim();
    if label.is_empty() {
        "none"
    } else if label.starts_with(|c: char| c.is_ascii_digit()) {
        "arabic"
    } else if label.starts_with(|c: char| c.is_ascii_alphabetic()) {
        "alpha"
    } else {
        "Dash"
    }
}

/// Blöcke eines Elements ohne Fußnotenzähler (Einleitungssatz, Änderungsverlauf).
fn content_blocks(e: &Element) -> Vec<Block> {
    let mut counter = 0;
    let mut collector = BlockCollector::new(&mut counter);
    collector.content(e);
    collector.blocks
}

fn law_meta(dom: &Element, fallback_abbrev: &str) -> ParsedLawMeta {
    let kopf = dom.child("kopf");
    let unabh = kopf.and_then(|k| k.child("angaben.versunabh"));
    let abh = kopf.and_then(|k| k.child("angaben.versabh"));
    let text_of = |e: Option<&Element>, name: &str| {
        e.and_then(|e| e.child(name))
            .map(|c| normalize_ws(&c.text()))
            .filter(|t| !t.is_empty())
    };
    let doknr = unabh
        .and_then(|u| u.child("dokumentation"))
        .and_then(|d| d.attr("dokid"))
        .unwrap_or_default()
        .to_owned();
    let abbrev = text_of(abh, "amtlicheAbk").unwrap_or_else(|| fallback_abbrev.to_owned());
    let titelangaben = abh.and_then(|a| a.child("titelangaben"));
    let title_lines: Vec<String> = titelangaben
        .map(|t| {
            t.text()
                .lines()
                .map(normalize_ws)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let title = title_lines
        .first()
        .cloned()
        .or_else(|| text_of(abh, "kurzbezeichnung"))
        .unwrap_or_else(|| abbrev.clone());
    let neufassung = title_lines
        .iter()
        .find(|l| l.starts_with("in der Fassung"))
        .cloned();
    let fundstelle = unabh
        .and_then(|u| u.child("ausfertigung"))
        .and_then(|a| a.child("fundstelle.GVBl"))
        .and_then(|f| {
            let jahr = text_of(Some(f), "jahr")?;
            let seite = text_of(Some(f), "seite")?.replace("S=", "S. ");
            Some(format!("GVBl. {jahr} {seite}"))
        });
    let bayrs = text_of(abh, "gliederungsNr.BayRS");
    let fundstelle = match (fundstelle, bayrs) {
        (Some(f), Some(b)) => Some(format!("{f}; {b}")),
        (Some(f), None) => Some(f),
        (None, b) => b,
    };
    let stand = dom
        .child("rumpf")
        .and_then(|r| r.child("normzitat"))
        .map(|n| normalize_ws(&n.text()))
        .and_then(|zitat| {
            zitat
                .find("zuletzt ")
                .map(|i| zitat[i..].to_owned())
                .or(Some(zitat))
        })
        .filter(|s| !s.is_empty());
    ParsedLawMeta {
        doknr,
        builddate: dom.attr("builddate").unwrap_or_default().to_owned(),
        jurabk: abbrev.clone(),
        amtabk: Some(abbrev),
        title,
        ausfertigung_datum: text_of(
            unabh.and_then(|u| u.child("ausfertigung")),
            "ausfertigungsdatum",
        ),
        fundstelle,
        stand,
        neufassung,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::text::spans_text;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<byrecht-norm builddate="06.11.2026_02:19">
  <kopf>
    <angaben.versunabh>
      <dokumentation doktyp="gesetz" dokid="BayVwVfG" ersatz="BayVwVfG" />
      <ausfertigung><ausfertigungsdatum>1976-12-23</ausfertigungsdatum>
        <fundstelle.GVBl><jahr>1976</jahr><seite>S=544</seite></fundstelle.GVBl></ausfertigung>
    </angaben.versunabh>
    <angaben.versabh version.id="p">
      <gliederungsNr.BayRS>BayRS 2010-1-I</gliederungsNr.BayRS>
      <titelangaben>Bayerisches Verwaltungsverfahrensgesetz<br /> (<amtlicheAbk>BayVwVfG</amtlicheAbk>)<br />Vom 23. Dezember 1976</titelangaben>
      <amtlicheAbk>BayVwVfG</amtlicheAbk>
    </angaben.versabh>
  </kopf>
  <rumpf>
    <aenderungsverlauf><p>Änderungen</p><ul><li><symbol id="x">1.</symbol><p>§ 1 ÄndG vom 23.7.1985</p></li></ul></aenderungsverlauf>
    <normzitat>Bayerisches Verwaltungsverfahrensgesetz (BayVwVfG), das zuletzt durch § 1 des Gesetzes vom 9. Dezember 2024 (GVBl. S. 570) geändert worden ist</normzitat>
    <einleitungssatz version="p"><einleitungssatz.text><p>Der Landtag hat beschlossen:</p></einleitungssatz.text></einleitungssatz>
    <gliederung gliederungsid="G_1" version="p">
      <gliederung.titel><p>
        Erster Teil 
        Anwendungsbereich
      </p></gliederung.titel>
      <gliederung gliederungsid="G_2" version="p">
        <gliederung.titel><p>Abschnitt I Zuständigkeit</p></gliederung.titel>
        <einzelnorm version="p" einzelnormid="P_1"><para.nr>Art. 1</para.nr><para.titel> Anwendungsbereich </para.titel>
          <jurAbsatz><absatz.nr>(1)</absatz.nr><absatz.text>
            <p><satz.nr id="xx">1</satz.nr>Dieses Gesetz gilt<fn.call><fn.text>1)</fn.text><fn.def><p>Amtl. Anm.</p></fn.def></fn.call> für Behörden. <satz.nr id="xx">2</satz.nr>Es gilt nicht für</p>
            <ul><li><symbol id="x">1.</symbol><p>Verfahren nach <verweis.norm>Art. <v.norm ersatz="BayVwVfG">2</v.norm></verweis.norm>,</p></li>
                <li><symbol id="x">2.</symbol><p>Verfahren im Ausland.</p>
                    <ul><li><symbol id="x">a)</symbol><p>Unterpunkt</p></li></ul></li></ul>
            <p>Nachsatz.</p>
          </absatz.text></jurAbsatz>
          <jurAbsatz><absatz.nr>(2)</absatz.nr><absatz.text><p>Zweiter Absatz.</p></absatz.text></jurAbsatz>
        </einzelnorm>
        <einzelnorm version="p" einzelnormid="P_2"><para.nr>Art. 2</para.nr><para.titel><span class="i"> (aufgehoben)</span></para.titel><jurAbsatz id="xxx"><absatz.text /></jurAbsatz></einzelnorm>
      </gliederung>
    </gliederung>
    <einzelnorm version="p" einzelnormid="P_3"><para.nr>Art. 3</para.nr><jurAbsatz><absatz.text>
      <p>Tabelle:</p>
      <table rules="none"><colgroup><col /><col /></colgroup><thead><tr><th colspan="2" rowspan="1"><p>Kopf</p></th></tr></thead>
        <tbody><tr><td colspan="1" rowspan="1"><p>a</p></td><td colspan="1" rowspan="1"><p>b</p></td></tr></tbody></table>
    </absatz.text></jurAbsatz></einzelnorm>
  </rumpf>
  <annex version="p" annexid="ANL_1">
    <annex.koerper version="p"><annex.nummer int="1">Anlage 1</annex.nummer><annex.titel>Muster</annex.titel></annex.koerper>
    <annex.text><p>Anlagentext.</p></annex.text>
  </annex>
</byrecht-norm>"#;

    #[test]
    fn meta_is_read_from_head() {
        assert!(is_byrecht(SAMPLE));
        let meta = parse_meta(SAMPLE, "X").unwrap();
        assert_eq!(meta.doknr, "BayVwVfG");
        assert_eq!(meta.jurabk, "BayVwVfG");
        assert_eq!(meta.title, "Bayerisches Verwaltungsverfahrensgesetz");
        assert_eq!(meta.builddate, "06.11.2026_02:19");
        assert_eq!(meta.ausfertigung_datum.as_deref(), Some("1976-12-23"));
        assert_eq!(
            meta.fundstelle.as_deref(),
            Some("GVBl. 1976 S. 544; BayRS 2010-1-I")
        );
        assert_eq!(
            meta.stand.as_deref(),
            Some("zuletzt durch § 1 des Gesetzes vom 9. Dezember 2024 (GVBl. S. 570) geändert worden ist")
        );
        assert!(meta.neufassung.is_none());
    }

    #[test]
    fn fallback_abbrev_when_missing() {
        let xml = SAMPLE.replace("<amtlicheAbk>BayVwVfG</amtlicheAbk>\n", "<amtlicheAbk />\n");
        let meta = parse_meta(&xml, "BV").unwrap();
        assert_eq!(meta.jurabk, "BV");
    }

    #[test]
    fn units_norms_and_blocks() {
        let law = parse_law(SAMPLE, "X").unwrap();
        assert_eq!(law.units.len(), 2);
        assert_eq!(law.units[0].bez, "Erster Teil");
        assert_eq!(law.units[0].titel.as_deref(), Some("Anwendungsbereich"));
        assert_eq!(law.units[0].kennzahl, "001");
        assert_eq!(law.units[1].bez, "Abschnitt I Zuständigkeit");
        assert_eq!(law.units[1].titel, None);
        assert_eq!(law.units[1].parent, Some(0));
        assert_eq!(law.units[1].depth, 1);
        assert_eq!(law.units[1].kennzahl, "001001");

        // Rahmennorm, Art. 1, Art. 2, Art. 3, Anlage 1
        assert_eq!(law.norms.len(), 5);
        let frame = &law.norms[0];
        assert_eq!(frame.enbez, None);
        assert_eq!(
            spans_text(match &frame.blocks[0] {
                Block::Paragraph { spans } => spans,
                other => panic!("{other:?}"),
            }),
            "Der Landtag hat beschlossen:"
        );
        assert!(matches!(frame.fussnoten.get(1), Some(Block::List { .. })));

        let art1 = &law.norms[1];
        assert_eq!(art1.enbez.as_deref(), Some("Art. 1"));
        assert_eq!(art1.titel.as_deref(), Some("Anwendungsbereich"));
        assert_eq!(art1.unit, Some(1));
        let kinds: Vec<&str> = art1
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph { .. } => "P",
                Block::List { .. } => "L",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, ["P", "L", "P", "P"]);
        let Block::Paragraph { spans } = &art1.blocks[0] else {
            panic!()
        };
        assert_eq!(spans[0].text, "(1) ");
        assert!(spans[1].style.sup && spans[1].text == "1");
        assert_eq!(
            spans_text(spans),
            "(1) 1Dieses Gesetz gilt1) für Behörden. 2Es gilt nicht für"
        );
        let fn_span = spans.iter().find(|s| s.footnote.is_some()).unwrap();
        assert_eq!(fn_span.footnote.as_deref(), Some("F1"));
        assert_eq!(art1.footnotes.len(), 1);
        assert_eq!(art1.footnotes[0].mark, "1)");
        let Block::List { kind, items } = &art1.blocks[1] else {
            panic!()
        };
        assert_eq!(kind, "arabic");
        assert_eq!(items.len(), 2);
        assert_eq!(spans_text(&items[0].label), "1.");
        assert_eq!(
            spans_text(match &items[0].blocks[0] {
                Block::Paragraph { spans } => spans,
                other => panic!("{other:?}"),
            }),
            "Verfahren nach Art. 2,"
        );
        assert!(
            matches!(items[1].blocks.get(1), Some(Block::List { kind, .. }) if kind == "alpha")
        );
        let Block::Paragraph { spans } = &art1.blocks[3] else {
            panic!()
        };
        assert_eq!(spans_text(spans), "(2) Zweiter Absatz.");

        let art2 = &law.norms[2];
        assert_eq!(art2.titel.as_deref(), Some("(aufgehoben)"));
        assert!(art2.blocks.is_empty());

        let art3 = &law.norms[3];
        assert_eq!(art3.unit, None);
        let Block::Table {
            header_rows, rows, ..
        } = &art3.blocks[1]
        else {
            panic!("{:?}", art3.blocks)
        };
        assert_eq!(*header_rows, 1);
        assert_eq!(rows[0][0].colspan, 2);
        assert_eq!(rows[1].len(), 2);

        let annex = &law.norms[4];
        assert_eq!(annex.enbez.as_deref(), Some("Anlage 1"));
        assert_eq!(annex.titel.as_deref(), Some("Muster"));
    }

    #[test]
    fn preamble_without_number_uses_title() {
        let xml = SAMPLE.replace(
            "<para.nr>Art. 3</para.nr>",
            "<para.nr /><para.titel>[Präambel]</para.titel>",
        );
        let law = parse_law(&xml, "X").unwrap();
        assert_eq!(law.norms[3].enbez.as_deref(), Some("Präambel"));
    }

    /// Import eines echten Dokuments (Pfad in `GESETZE_BYRECHT_XML`), nur mit
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn parses_real_document() {
        let Ok(path) = std::env::var("GESETZE_BYRECHT_XML") else {
            return;
        };
        let xml = std::fs::read_to_string(path).unwrap();
        let law = parse_law(&xml, "X").unwrap();
        assert!(law.norms.len() > 10, "nur {} Normen", law.norms.len());
        assert!(law.units.len() > 1);
        assert!(law.norms.iter().skip(1).all(|n| n.enbez.is_some()));
    }
}
