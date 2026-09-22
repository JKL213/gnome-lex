//! Block-Modell des Normtexts und dessen Linearisierung in Textsegmente.
//!
//! Der Importer speichert Blöcke als JSON in SQLite; die Normansicht wandelt
//! sie in Segmente für einen `GtkTextBuffer` um. Damit Zeichenoffsets von
//! Annotationen in Datenbank und Ansicht übereinstimmen, verwenden beide
//! dieselbe Funktion [`flatten_blocks`].

use serde::{Deserialize, Serialize};

/// Zeichen, das im Text eine eingebettete Tabelle vertritt (Object Replacement Character).
pub const TABLE_PLACEHOLDER: char = '\u{FFFC}';

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sup: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub small: bool,
}

impl Style {
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }
}

/// Ein Textlauf mit einheitlicher Auszeichnung.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "Style::is_plain")]
    pub style: Style,
    /// ID der referenzierten Fußnote (`<FnR ID="…"/>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footnote: Option<String>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub label: Vec<Span>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub blocks: Vec<Block>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub colspan: u32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub rowspan: u32,
}

fn one() -> u32 {
    1
}
fn is_one(v: &u32) -> bool {
    *v == 1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Block {
    /// Absatz (`<P>`) oder Listenabsatz (`<LA>`).
    Paragraph { spans: Vec<Span> },
    /// Aufzählung (`<DL>`); `kind` entspricht dem Attribut `Type`.
    List { kind: String, items: Vec<ListItem> },
    /// CALS-Tabelle.
    Table {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        header_rows: usize,
        rows: Vec<Vec<Cell>>,
    },
    /// Vorformatierter Text (`<pre>`).
    Pre { text: String },
    /// Zwischenüberschrift (`<Ident>`/`<Title>` in Inhaltsübersichten).
    Heading { level: u8, spans: Vec<Span> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footnote {
    pub id: String,
    /// Fußnotenzeichen, z. B. „*“ oder „1“.
    pub mark: String,
    pub blocks: Vec<Block>,
}

/// Auszeichnung eines Segments in der linearisierten Form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegTag {
    Bold,
    Italic,
    Underline,
    Sup,
    Sub,
    Small,
    Pre,
    /// Überschrift mit Ebene.
    Heading(u8),
    /// Zeile eines Listeneintrags mit Verschachtelungstiefe (0 = oberste Liste).
    ListLine(u8),
    /// Bezeichner eines Listeneintrags („1.“, „a)“).
    ListLabel,
    /// Verweis auf eine Fußnote (ID).
    FootnoteRef(String),
    /// Platzhalter für die Tabelle mit diesem Index in [`Flattened::tables`].
    Table(usize),
}

/// Ein Textstück mit Auszeichnungen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seg {
    pub text: String,
    pub tags: Vec<SegTag>,
}

/// Tabelle in einer für `GtkGrid` geeigneten Form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableData {
    pub title: Option<String>,
    pub header_rows: usize,
    pub rows: Vec<Vec<TableCell>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub segs: Vec<Seg>,
    pub colspan: u32,
    pub rowspan: u32,
}

/// Ergebnis der Linearisierung.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flattened {
    pub segs: Vec<Seg>,
    pub tables: Vec<TableData>,
    /// Zeichenoffset (in Unicode-Skalaren), an dem jeder Block beginnt.
    pub block_starts: Vec<usize>,
}

impl Flattened {
    /// Reiner Text ohne Auszeichnung.
    pub fn text(&self) -> String {
        self.segs.iter().map(|s| s.text.as_str()).collect()
    }

    pub fn char_len(&self) -> usize {
        self.segs.iter().map(|s| s.text.chars().count()).sum()
    }
}

struct Flattener {
    out: Flattened,
    len: usize,
}

impl Flattener {
    fn push(&mut self, text: &str, tags: Vec<SegTag>) {
        if text.is_empty() {
            return;
        }
        self.len += text.chars().count();
        if let Some(last) = self.out.segs.last_mut() {
            if last.tags == tags {
                last.text.push_str(text);
                return;
            }
        }
        self.out.segs.push(Seg {
            text: text.to_owned(),
            tags,
        });
    }

    fn spans(&mut self, spans: &[Span], base: &[SegTag]) {
        for span in spans {
            let mut tags = base.to_vec();
            let s = span.style;
            if s.bold {
                tags.push(SegTag::Bold);
            }
            if s.italic {
                tags.push(SegTag::Italic);
            }
            if s.underline {
                tags.push(SegTag::Underline);
            }
            if s.sup {
                tags.push(SegTag::Sup);
            }
            if s.sub {
                tags.push(SegTag::Sub);
            }
            if s.small {
                tags.push(SegTag::Small);
            }
            if let Some(id) = &span.footnote {
                tags.push(SegTag::FootnoteRef(id.clone()));
            }
            self.push(&span.text, tags);
        }
    }

    fn block(&mut self, block: &Block, depth: u8) {
        match block {
            Block::Paragraph { spans } => {
                let base: Vec<SegTag> = if depth > 0 {
                    vec![SegTag::ListLine(depth - 1)]
                } else {
                    vec![]
                };
                self.spans(spans, &base);
                self.push("\n", base);
            }
            Block::List { items, .. } => {
                for item in items {
                    let line = vec![SegTag::ListLine(depth)];
                    let mut label_tags = line.clone();
                    label_tags.push(SegTag::ListLabel);
                    self.spans(&item.label, &label_tags);
                    self.push("\t", line.clone());
                    // Der erste Absatz folgt in derselben Zeile wie der Bezeichner.
                    let mut first = true;
                    for b in &item.blocks {
                        match b {
                            Block::Paragraph { spans } if first => {
                                self.spans(spans, &line);
                                self.push("\n", line.clone());
                            }
                            _ => {
                                if first {
                                    self.push("\n", line.clone());
                                }
                                self.block(b, depth + 1);
                            }
                        }
                        first = false;
                    }
                    if first {
                        self.push("\n", line);
                    }
                }
            }
            Block::Table {
                title,
                header_rows,
                rows,
            } => {
                let index = self.out.tables.len();
                let data = TableData {
                    title: title.clone(),
                    header_rows: *header_rows,
                    rows: rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|cell| TableCell {
                                    segs: flatten_blocks(&cell.blocks).segs,
                                    colspan: cell.colspan,
                                    rowspan: cell.rowspan,
                                })
                                .collect()
                        })
                        .collect(),
                };
                self.out.tables.push(data);
                let mut s = String::new();
                s.push(TABLE_PLACEHOLDER);
                self.push(&s, vec![SegTag::Table(index)]);
                self.push("\n", vec![]);
            }
            Block::Pre { text } => {
                self.push(text, vec![SegTag::Pre]);
                if !text.ends_with('\n') {
                    self.push("\n", vec![SegTag::Pre]);
                }
            }
            Block::Heading { level, spans } => {
                self.spans(spans, &[SegTag::Heading(*level)]);
                self.push("\n", vec![SegTag::Heading(*level)]);
            }
        }
    }
}

/// Linearisiert eine Blockfolge. Jeder Block endet mit einem Zeilenumbruch.
pub fn flatten_blocks(blocks: &[Block]) -> Flattened {
    let mut f = Flattener {
        out: Flattened::default(),
        len: 0,
    };
    for block in blocks {
        f.out.block_starts.push(f.len);
        f.block(block, 0);
    }
    f.out
}

/// Reiner Text einer Spanfolge.
pub fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Text eines Blocks einschließlich Tabelleninhalten (für die Volltextsuche).
pub fn block_search_text(block: &Block) -> String {
    let mut out = String::new();
    collect_search_text(block, &mut out);
    out
}

fn collect_search_text(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph { spans } | Block::Heading { spans, .. } => {
            out.push_str(&spans_text(spans));
            out.push('\n');
        }
        Block::List { items, .. } => {
            for item in items {
                out.push_str(&spans_text(&item.label));
                out.push(' ');
                for b in &item.blocks {
                    collect_search_text(b, out);
                }
            }
        }
        Block::Table { title, rows, .. } => {
            if let Some(t) = title {
                out.push_str(t);
                out.push('\n');
            }
            for row in rows {
                for cell in row {
                    for b in &cell.blocks {
                        collect_search_text(b, out);
                    }
                }
            }
        }
        Block::Pre { text } => {
            out.push_str(text);
            out.push('\n');
        }
    }
}

/// Liest eine Absatznummer wie „(1)“ am Anfang eines Absatzes.
pub fn paragraph_label(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('(')?;
    let end = rest.find(')')?;
    let inner = &rest[..end];
    if inner.is_empty() || inner.len() > 4 {
        return None;
    }
    if inner.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(format!("({inner})"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Block {
        Block::Paragraph {
            spans: vec![Span::plain(text)],
        }
    }

    #[test]
    fn flatten_paragraphs_records_block_starts() {
        let f = flatten_blocks(&[p("(1) Erster."), p("(2) Zweiter.")]);
        assert_eq!(f.text(), "(1) Erster.\n(2) Zweiter.\n");
        assert_eq!(f.block_starts, vec![0, 12]);
        assert_eq!(f.char_len(), 24);
    }

    #[test]
    fn flatten_list_puts_label_and_first_paragraph_on_one_line() {
        let list = Block::List {
            kind: "arabic".into(),
            items: vec![
                ListItem {
                    label: vec![Span::plain("1.")],
                    blocks: vec![p("eins")],
                },
                ListItem {
                    label: vec![Span::plain("2.")],
                    blocks: vec![
                        p("zwei"),
                        Block::List {
                            kind: "alpha".into(),
                            items: vec![ListItem {
                                label: vec![Span::plain("a)")],
                                blocks: vec![p("zwei-a")],
                            }],
                        },
                    ],
                },
            ],
        };
        let f = flatten_blocks(&[p("Intro:"), list]);
        assert_eq!(f.text(), "Intro:\n1.\teins\n2.\tzwei\na)\tzwei-a\n");
        let label = f.segs.iter().find(|s| s.text == "1.").unwrap();
        assert!(label.tags.contains(&SegTag::ListLabel));
        let nested = f.segs.iter().find(|s| s.text == "zwei-a").unwrap();
        assert!(nested.tags.contains(&SegTag::ListLine(1)));
    }

    #[test]
    fn flatten_table_uses_placeholder_and_collects_table() {
        let table = Block::Table {
            title: None,
            header_rows: 1,
            rows: vec![vec![
                Cell {
                    blocks: vec![p("A")],
                    colspan: 1,
                    rowspan: 1,
                },
                Cell {
                    blocks: vec![p("B")],
                    colspan: 2,
                    rowspan: 1,
                },
            ]],
        };
        let f = flatten_blocks(&[table]);
        assert_eq!(f.text(), format!("{TABLE_PLACEHOLDER}\n"));
        assert_eq!(f.tables.len(), 1);
        assert_eq!(f.tables[0].rows[0][1].colspan, 2);
        assert_eq!(f.tables[0].rows[0][0].segs[0].text, "A\n");
        assert!(block_search_text(&Block::Table {
            title: Some("T".into()),
            header_rows: 0,
            rows: vec![]
        })
        .contains("T"));
    }

    #[test]
    fn styles_become_tags_and_merge_adjacent_plain_text() {
        let block = Block::Paragraph {
            spans: vec![
                Span::plain("a"),
                Span::plain("b"),
                Span {
                    text: "c".into(),
                    style: Style {
                        bold: true,
                        ..Default::default()
                    },
                    footnote: None,
                },
                Span {
                    text: "1".into(),
                    style: Style::default(),
                    footnote: Some("fn1".into()),
                },
            ],
        };
        let f = flatten_blocks(&[block]);
        assert_eq!(f.segs[0].text, "ab");
        assert_eq!(f.segs[1].tags, vec![SegTag::Bold]);
        assert_eq!(f.segs[2].tags, vec![SegTag::FootnoteRef("fn1".into())]);
    }

    #[test]
    fn paragraph_labels() {
        assert_eq!(paragraph_label("(1) Text"), Some("(1)".into()));
        assert_eq!(paragraph_label("(2a) Text"), Some("(2a)".into()));
        assert_eq!(paragraph_label("Text (1)"), None);
        assert_eq!(paragraph_label("(Nicht) Text"), None);
        assert_eq!(paragraph_label(""), None);
    }

    #[test]
    fn block_json_roundtrip() {
        let blocks = vec![p("x"), Block::Pre { text: "y".into() }];
        let json = serde_json::to_string(&blocks).unwrap();
        let back: Vec<Block> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, blocks);
        assert!(json.contains("\"t\":\"paragraph\""));
    }
}
