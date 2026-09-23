// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! SQLite-Datenhaltung: Gesetze, Gliederung, Normen, Absätze, Volltextindex
//! (FTS5) und Annotationen.
// Wird ab den folgenden Stufen von der Oberfläche genutzt.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::importer::xml::ParsedLaw;
use crate::model::text::{block_search_text, flatten_blocks, paragraph_label, Block, Footnote};
use crate::model::{Annotation, AnnotationKind, LawInfo, LinkTarget, Norm, NormInfo, UnitInfo};

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS laws (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    jurabk TEXT NOT NULL,
    amtabk TEXT,
    title TEXT NOT NULL,
    doknr TEXT NOT NULL,
    builddate TEXT NOT NULL,
    stand TEXT,
    neufassung TEXT,
    ausfertigung_datum TEXT,
    fundstelle TEXT,
    imported_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS units (
    id INTEGER PRIMARY KEY,
    law_id INTEGER NOT NULL REFERENCES laws(id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES units(id) ON DELETE CASCADE,
    doknr TEXT,
    kennzahl TEXT NOT NULL,
    bez TEXT NOT NULL,
    titel TEXT,
    depth INTEGER NOT NULL,
    position INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS units_law ON units(law_id, position);
CREATE TABLE IF NOT EXISTS norms (
    id INTEGER PRIMARY KEY,
    law_id INTEGER NOT NULL REFERENCES laws(id) ON DELETE CASCADE,
    unit_id INTEGER REFERENCES units(id) ON DELETE SET NULL,
    doknr TEXT NOT NULL,
    enbez TEXT,
    titel TEXT,
    position INTEGER NOT NULL,
    blocks TEXT NOT NULL,
    footnotes TEXT NOT NULL,
    fussnoten TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS norms_law ON norms(law_id, position);
CREATE INDEX IF NOT EXISTS norms_enbez ON norms(law_id, enbez);
CREATE TABLE IF NOT EXISTS paragraphs (
    id INTEGER PRIMARY KEY,
    norm_id INTEGER NOT NULL REFERENCES norms(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    label TEXT,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS paragraphs_norm ON paragraphs(norm_id, idx);
CREATE VIRTUAL TABLE IF NOT EXISTS norms_fts USING fts5(
    enbez, titel, body, norm_id UNINDEXED, law_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TABLE IF NOT EXISTS annotations (
    id INTEGER PRIMARY KEY,
    law TEXT NOT NULL,
    norm TEXT NOT NULL,
    paragraph INTEGER NOT NULL DEFAULT 0,
    start INTEGER NOT NULL DEFAULT 0,
    end INTEGER NOT NULL DEFAULT 0,
    quote TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    color TEXT,
    note TEXT,
    target TEXT,
    created TEXT NOT NULL,
    modified TEXT NOT NULL,
    orphaned INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS annotations_norm ON annotations(law, norm);
"#;

/// Treffer der Volltextsuche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub norm: NormInfo,
    pub snippet: String,
}

/// Ergebnis der Neuverankerung von Annotationen nach einem Update.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReanchorReport {
    pub unchanged: usize,
    pub moved: usize,
    pub orphaned: usize,
    pub recovered: usize,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Standardpfad: `<user_data_dir>/gesetze/gesetze.db`.
    pub fn default_path() -> PathBuf {
        glib::user_data_dir().join("gesetze").join("gesetze.db")
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&Self::default_path())
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(format!("{}: {e}", parent.display())),
                )
            })?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(SCHEMA)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    // ------------------------------------------------------------------
    // Gesetze
    // ------------------------------------------------------------------

    pub fn laws(&self) -> Result<Vec<LawInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, slug, jurabk, amtabk, title, doknr, builddate, stand, neufassung,
                    ausfertigung_datum, fundstelle, imported_at FROM laws ORDER BY title",
        )?;
        let rows = stmt.query_map([], row_to_law)?;
        rows.collect()
    }

    pub fn law_by_slug(&self, slug: &str) -> Result<Option<LawInfo>> {
        self.conn
            .query_row(
                "SELECT id, slug, jurabk, amtabk, title, doknr, builddate, stand, neufassung,
                        ausfertigung_datum, fundstelle, imported_at FROM laws WHERE slug = ?1",
                [slug],
                row_to_law,
            )
            .optional()
    }

    pub fn law_by_id(&self, id: i64) -> Result<Option<LawInfo>> {
        self.conn
            .query_row(
                "SELECT id, slug, jurabk, amtabk, title, doknr, builddate, stand, neufassung,
                        ausfertigung_datum, fundstelle, imported_at FROM laws WHERE id = ?1",
                [id],
                row_to_law,
            )
            .optional()
    }

    pub fn law_by_abbrev(&self, abbrev: &str) -> Result<Option<LawInfo>> {
        self.conn
            .query_row(
                "SELECT id, slug, jurabk, amtabk, title, doknr, builddate, stand, neufassung,
                        ausfertigung_datum, fundstelle, imported_at FROM laws
                 WHERE jurabk = ?1 COLLATE NOCASE OR amtabk = ?1 COLLATE NOCASE OR slug = ?1 COLLATE NOCASE",
                [abbrev],
                row_to_law,
            )
            .optional()
    }

    /// Ersetzt ein Gesetz vollständig durch den geparsten Stand und liefert
    /// die Gesetzes-ID. Annotationen bleiben erhalten (sie sind über die
    /// Normbezeichnung verankert) und werden anschließend neu verankert.
    pub fn replace_law(&mut self, slug: &str, parsed: &ParsedLaw) -> Result<i64> {
        let tx = self.conn.transaction()?;
        if let Some(old_id) = tx
            .query_row("SELECT id FROM laws WHERE slug = ?1", [slug], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
        {
            tx.execute("DELETE FROM norms_fts WHERE law_id = ?1", [old_id])?;
            tx.execute("DELETE FROM laws WHERE id = ?1", [old_id])?;
        }
        let m = &parsed.meta;
        tx.execute(
            "INSERT INTO laws (slug, jurabk, amtabk, title, doknr, builddate, stand, neufassung,
                               ausfertigung_datum, fundstelle, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                slug,
                m.jurabk,
                m.amtabk,
                m.title,
                m.doknr,
                m.builddate,
                m.stand,
                m.neufassung,
                m.ausfertigung_datum,
                m.fundstelle,
                Annotation::now(),
            ],
        )?;
        let law_id = tx.last_insert_rowid();

        let mut unit_ids: Vec<i64> = Vec::with_capacity(parsed.units.len());
        {
            let mut stmt = tx.prepare(
                "INSERT INTO units (law_id, parent_id, doknr, kennzahl, bez, titel, depth, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (i, u) in parsed.units.iter().enumerate() {
                let parent_id = u.parent.map(|p| unit_ids[p]);
                stmt.execute(params![
                    law_id,
                    parent_id,
                    u.doknr,
                    u.kennzahl,
                    u.bez,
                    u.titel,
                    u.depth as i64,
                    i as i64
                ])?;
                unit_ids.push(tx.last_insert_rowid());
            }
        }
        {
            let mut norm_stmt = tx.prepare(
                "INSERT INTO norms (law_id, unit_id, doknr, enbez, titel, position, blocks, footnotes, fussnoten)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            let mut para_stmt = tx.prepare(
                "INSERT INTO paragraphs (norm_id, idx, label, text) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut fts_stmt = tx.prepare(
                "INSERT INTO norms_fts (enbez, titel, body, norm_id, law_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (i, n) in parsed.norms.iter().enumerate() {
                let blocks = serde_json::to_string(&n.blocks).unwrap_or_else(|_| "[]".into());
                let footnotes = serde_json::to_string(&n.footnotes).unwrap_or_else(|_| "[]".into());
                let fussnoten = serde_json::to_string(&n.fussnoten).unwrap_or_else(|_| "[]".into());
                norm_stmt.execute(params![
                    law_id,
                    n.unit.map(|u| unit_ids[u]),
                    n.doknr,
                    n.enbez,
                    n.titel,
                    i as i64,
                    blocks,
                    footnotes,
                    fussnoten
                ])?;
                let norm_id = tx.last_insert_rowid();
                let mut body = String::new();
                for (idx, block) in n.blocks.iter().enumerate() {
                    let text = flatten_blocks(std::slice::from_ref(block)).text();
                    para_stmt.execute(params![
                        norm_id,
                        idx as i64 + 1,
                        paragraph_label(&text),
                        text
                    ])?;
                    body.push_str(&block_search_text(block));
                }
                fts_stmt.execute(params![n.enbez, n.titel, body, norm_id, law_id])?;
            }
        }
        tx.commit()?;
        Ok(law_id)
    }

    pub fn delete_law(&mut self, slug: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        if let Some(old_id) = tx
            .query_row("SELECT id FROM laws WHERE slug = ?1", [slug], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
        {
            tx.execute("DELETE FROM norms_fts WHERE law_id = ?1", [old_id])?;
            tx.execute("DELETE FROM laws WHERE id = ?1", [old_id])?;
        }
        tx.commit()
    }

    // ------------------------------------------------------------------
    // Gliederung und Normen
    // ------------------------------------------------------------------

    pub fn units(&self, law_id: i64) -> Result<Vec<UnitInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, law_id, parent_id, kennzahl, bez, titel, depth, position
             FROM units WHERE law_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([law_id], |r| {
            Ok(UnitInfo {
                id: r.get(0)?,
                law_id: r.get(1)?,
                parent_id: r.get(2)?,
                kennzahl: r.get(3)?,
                bez: r.get(4)?,
                titel: r.get(5)?,
                depth: r.get(6)?,
                position: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn norm_infos(&self, law_id: i64) -> Result<Vec<NormInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, law_id, unit_id, doknr, enbez, titel, position
             FROM norms WHERE law_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([law_id], row_to_norm_info)?;
        rows.collect()
    }

    pub fn norm_info(&self, id: i64) -> Result<Option<NormInfo>> {
        self.conn
            .query_row(
                "SELECT id, law_id, unit_id, doknr, enbez, titel, position FROM norms WHERE id = ?1",
                [id],
                row_to_norm_info,
            )
            .optional()
    }

    pub fn norm(&self, id: i64) -> Result<Option<Norm>> {
        self.conn
            .query_row(
                "SELECT id, law_id, unit_id, doknr, enbez, titel, position, blocks, footnotes, fussnoten
                 FROM norms WHERE id = ?1",
                [id],
                row_to_norm,
            )
            .optional()
    }

    pub fn norm_by_enbez(&self, law_id: i64, enbez: &str) -> Result<Option<Norm>> {
        self.conn
            .query_row(
                "SELECT id, law_id, unit_id, doknr, enbez, titel, position, blocks, footnotes, fussnoten
                 FROM norms WHERE law_id = ?1 AND enbez = ?2 COLLATE NOCASE",
                params![law_id, enbez],
                row_to_norm,
            )
            .optional()
    }

    pub fn norm_id_by_enbez(&self, law_id: i64, enbez: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM norms WHERE law_id = ?1 AND enbez = ?2 COLLATE NOCASE",
                params![law_id, enbez],
                |r| r.get(0),
            )
            .optional()
    }

    /// Erste Norm eines Gesetzes mit „§“-Bezeichnung (Einstieg beim Öffnen).
    pub fn first_norm_id(&self, law_id: i64) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM norms WHERE law_id = ?1 AND enbez LIKE '§%' ORDER BY position LIMIT 1",
                [law_id],
                |r| r.get(0),
            )
            .optional()
    }

    /// Vorherige (`-1`) oder nächste (`+1`) Norm in Dokumentreihenfolge.
    pub fn neighbor_norm(&self, id: i64, direction: i64) -> Result<Option<i64>> {
        let sql = if direction < 0 {
            "SELECT n2.id FROM norms n1 JOIN norms n2 ON n2.law_id = n1.law_id
             WHERE n1.id = ?1 AND n2.position < n1.position ORDER BY n2.position DESC LIMIT 1"
        } else {
            "SELECT n2.id FROM norms n1 JOIN norms n2 ON n2.law_id = n1.law_id
             WHERE n1.id = ?1 AND n2.position > n1.position ORDER BY n2.position ASC LIMIT 1"
        };
        self.conn.query_row(sql, [id], |r| r.get(0)).optional()
    }

    /// Absatztexte einer Norm (1-basiert, in Reihenfolge).
    pub fn paragraphs(&self, norm_id: i64) -> Result<Vec<(i64, Option<String>, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT idx, label, text FROM paragraphs WHERE norm_id = ?1 ORDER BY idx")?;
        let rows = stmt.query_map([norm_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    }

    // ------------------------------------------------------------------
    // Suche
    // ------------------------------------------------------------------

    /// Volltextsuche. Die Eingabe wird in Präfix-Terme zerlegt, damit auch
    /// Wortanfänge treffen; FTS-Sonderzeichen werden neutralisiert.
    pub fn search(&self, law_id: Option<i64>, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let Some(fts_query) = build_fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.law_id, n.unit_id, n.doknr, n.enbez, n.titel, n.position,
                    snippet(norms_fts, 2, '\u{1}', '\u{2}', '…', 14)
             FROM norms_fts JOIN norms n ON n.id = norms_fts.norm_id
             WHERE norms_fts MATCH ?1 AND (?2 IS NULL OR norms_fts.law_id = ?2)
             ORDER BY bm25(norms_fts, 5.0, 3.0, 1.0) LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![fts_query, law_id, limit as i64], |r| {
            Ok(SearchHit {
                norm: row_to_norm_info(r)?,
                snippet: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    // ------------------------------------------------------------------
    // Annotationen
    // ------------------------------------------------------------------

    pub fn annotations_for_norm(&self, law: &str, norm: &str) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, law, norm, paragraph, start, end, quote, kind, color, note, target,
                    created, modified, orphaned
             FROM annotations WHERE law = ?1 AND norm = ?2 ORDER BY paragraph, start",
        )?;
        let rows = stmt.query_map(params![law, norm], row_to_annotation)?;
        rows.collect()
    }

    pub fn annotations_for_law(&self, law: &str) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.law, a.norm, a.paragraph, a.start, a.end, a.quote, a.kind, a.color,
                    a.note, a.target, a.created, a.modified, a.orphaned
             FROM annotations a
             LEFT JOIN laws l ON l.slug = a.law
             LEFT JOIN norms n ON n.law_id = l.id AND n.enbez = a.norm
             WHERE a.law = ?1 ORDER BY COALESCE(n.position, 1000000), a.paragraph, a.start",
        )?;
        let rows = stmt.query_map([law], row_to_annotation)?;
        rows.collect()
    }

    pub fn all_annotations(&self) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, law, norm, paragraph, start, end, quote, kind, color, note, target,
                    created, modified, orphaned FROM annotations ORDER BY law, norm, paragraph, start",
        )?;
        let rows = stmt.query_map([], row_to_annotation)?;
        rows.collect()
    }

    pub fn annotation(&self, id: i64) -> Result<Option<Annotation>> {
        self.conn
            .query_row(
                "SELECT id, law, norm, paragraph, start, end, quote, kind, color, note, target,
                        created, modified, orphaned FROM annotations WHERE id = ?1",
                [id],
                row_to_annotation,
            )
            .optional()
    }

    pub fn insert_annotation(&self, a: &Annotation) -> Result<i64> {
        let now = Annotation::now();
        let created = if a.created.is_empty() {
            now.clone()
        } else {
            a.created.clone()
        };
        self.conn.execute(
            "INSERT INTO annotations (law, norm, paragraph, start, end, quote, kind, color, note,
                                      target, created, modified, orphaned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                a.law,
                a.norm,
                a.paragraph,
                a.start,
                a.end,
                a.quote,
                a.kind.as_str(),
                a.color,
                a.note,
                a.target
                    .as_ref()
                    .map(|t| serde_json::to_string(t).unwrap_or_default()),
                created,
                now,
                a.orphaned as i64,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_annotation(&self, a: &Annotation) -> Result<()> {
        self.conn.execute(
            "UPDATE annotations SET law = ?2, norm = ?3, paragraph = ?4, start = ?5, end = ?6,
                    quote = ?7, kind = ?8, color = ?9, note = ?10, target = ?11, modified = ?12,
                    orphaned = ?13 WHERE id = ?1",
            params![
                a.id,
                a.law,
                a.norm,
                a.paragraph,
                a.start,
                a.end,
                a.quote,
                a.kind.as_str(),
                a.color,
                a.note,
                a.target
                    .as_ref()
                    .map(|t| serde_json::to_string(t).unwrap_or_default()),
                Annotation::now(),
                a.orphaned as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_annotation(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM annotations WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Exportiert alle Annotationen als JSON.
    pub fn export_annotations_json(&self) -> Result<String> {
        let all = self.all_annotations()?;
        Ok(serde_json::to_string_pretty(&AnnotationExport {
            format: "org.gnomelex.Gesetze.annotations".into(),
            version: 1,
            exported: Annotation::now(),
            annotations: all,
        })
        .unwrap_or_default())
    }

    /// Importiert Annotationen aus JSON; identische Einträge (gleicher Anker
    /// und Inhalt) werden übersprungen. Liefert die Zahl neuer Einträge.
    pub fn import_annotations_json(&mut self, json: &str) -> std::result::Result<usize, String> {
        let export: AnnotationExport = serde_json::from_str(json)
            .or_else(|_| {
                serde_json::from_str::<Vec<Annotation>>(json).map(|annotations| AnnotationExport {
                    format: String::new(),
                    version: 1,
                    exported: String::new(),
                    annotations,
                })
            })
            .map_err(|e| format!("Ungültiges JSON: {e}"))?;
        let existing = self.all_annotations().map_err(|e| e.to_string())?;
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        let mut count = 0;
        for a in export.annotations {
            let duplicate = existing.iter().any(|e| {
                e.law == a.law
                    && e.norm == a.norm
                    && e.paragraph == a.paragraph
                    && e.start == a.start
                    && e.end == a.end
                    && e.kind == a.kind
                    && e.note == a.note
                    && e.color == a.color
            });
            if duplicate {
                continue;
            }
            insert_annotation_tx(&tx, &a).map_err(|e| e.to_string())?;
            count += 1;
        }
        tx.commit().map_err(|e| e.to_string())?;
        if count > 0 {
            // Importierte Anker gegen den aktuellen Text prüfen.
            let _ = self.reanchor_all();
        }
        Ok(count)
    }

    /// Verankert alle Annotationen eines Gesetzes neu, z. B. nach einem Update.
    pub fn reanchor_law(&mut self, law_slug: &str) -> Result<ReanchorReport> {
        let Some(law) = self.law_by_slug(law_slug)? else {
            return Ok(ReanchorReport::default());
        };
        let annotations = self.annotations_for_law(law_slug)?;
        let mut report = ReanchorReport::default();
        for mut a in annotations {
            let was_orphaned = a.orphaned;
            let outcome = self.reanchor_one(law.id, &mut a)?;
            match outcome {
                Anchor::Unchanged => {
                    if was_orphaned {
                        report.recovered += 1;
                        a.orphaned = false;
                        self.update_annotation(&a)?;
                    } else {
                        report.unchanged += 1;
                    }
                }
                Anchor::Moved => {
                    if was_orphaned {
                        report.recovered += 1;
                    } else {
                        report.moved += 1;
                    }
                    a.orphaned = false;
                    self.update_annotation(&a)?;
                }
                Anchor::Orphaned => {
                    report.orphaned += 1;
                    if !was_orphaned {
                        a.orphaned = true;
                        self.update_annotation(&a)?;
                    }
                }
            }
        }
        Ok(report)
    }

    pub fn reanchor_all(&mut self) -> Result<ReanchorReport> {
        let mut total = ReanchorReport::default();
        for law in self.laws()? {
            let r = self.reanchor_law(&law.slug)?;
            total.unchanged += r.unchanged;
            total.moved += r.moved;
            total.orphaned += r.orphaned;
            total.recovered += r.recovered;
        }
        Ok(total)
    }

    fn reanchor_one(&self, law_id: i64, a: &mut Annotation) -> Result<Anchor> {
        let Some(norm_id) = self.norm_id_by_enbez(law_id, &a.norm)? else {
            return Ok(Anchor::Orphaned);
        };
        // Lesezeichen und Anker ohne Wortlaut hängen nur an der Norm.
        if a.quote.is_empty() {
            return Ok(Anchor::Unchanged);
        }
        let paragraphs = self.paragraphs(norm_id)?;
        let quote = a.quote.as_str();

        let text_of = |idx: i64| paragraphs.iter().find(|p| p.0 == idx).map(|p| p.2.as_str());

        if let Some(text) = text_of(a.paragraph) {
            if char_slice(text, a.start, a.end) == Some(quote) {
                return Ok(Anchor::Unchanged);
            }
            if let Some(pos) = find_char_pos(text, quote) {
                a.start = pos as i64;
                a.end = (pos + quote.chars().count()) as i64;
                return Ok(Anchor::Moved);
            }
        }
        for (idx, _, text) in &paragraphs {
            if let Some(pos) = find_char_pos(text, quote) {
                a.paragraph = *idx;
                a.start = pos as i64;
                a.end = (pos + quote.chars().count()) as i64;
                return Ok(Anchor::Moved);
            }
        }
        Ok(Anchor::Orphaned)
    }
}

enum Anchor {
    Unchanged,
    Moved,
    Orphaned,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AnnotationExport {
    #[serde(default)]
    format: String,
    #[serde(default)]
    version: u32,
    #[serde(default)]
    exported: String,
    annotations: Vec<Annotation>,
}

fn insert_annotation_tx(tx: &Transaction<'_>, a: &Annotation) -> Result<()> {
    let now = Annotation::now();
    tx.execute(
        "INSERT INTO annotations (law, norm, paragraph, start, end, quote, kind, color, note,
                                  target, created, modified, orphaned)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            a.law,
            a.norm,
            a.paragraph,
            a.start,
            a.end,
            a.quote,
            a.kind.as_str(),
            a.color,
            a.note,
            a.target
                .as_ref()
                .map(|t| serde_json::to_string(t).unwrap_or_default()),
            if a.created.is_empty() {
                now.clone()
            } else {
                a.created.clone()
            },
            now,
            a.orphaned as i64,
        ],
    )?;
    Ok(())
}

fn char_slice(text: &str, start: i64, end: i64) -> Option<&str> {
    if start < 0 || end < start {
        return None;
    }
    let mut indices = text.char_indices().map(|(i, _)| i);
    let s = indices.by_ref().nth(start as usize)?;
    let len = (end - start) as usize;
    let e = if len == 0 {
        s
    } else {
        indices.nth(len - 1).unwrap_or(text.len())
    };
    text.get(s..e)
}

fn find_char_pos(text: &str, needle: &str) -> Option<usize> {
    let byte = text.find(needle)?;
    Some(text[..byte].chars().count())
}

/// Baut eine FTS5-Abfrage aus freier Eingabe: jeder Term als Präfixsuche,
/// Anführungszeichen als Phrase.
pub fn build_fts_query(input: &str) -> Option<String> {
    let mut terms = Vec::new();
    let mut in_phrase = false;
    let mut phrase = String::new();
    for token in input.split_whitespace() {
        let mut t = token.to_string();
        if !in_phrase && t.starts_with('"') {
            in_phrase = true;
            t.remove(0);
            phrase.clear();
        }
        if in_phrase {
            let closing = t.ends_with('"');
            if closing {
                t.pop();
            }
            let clean: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
            if !clean.is_empty() {
                if !phrase.is_empty() {
                    phrase.push(' ');
                }
                phrase.push_str(&clean);
            }
            if closing {
                in_phrase = false;
                if !phrase.is_empty() {
                    terms.push(format!("\"{phrase}\""));
                }
            }
            continue;
        }
        let clean: String = t
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '§')
            .collect();
        if clean.is_empty() {
            continue;
        }
        terms.push(format!("\"{clean}\"*"));
    }
    if in_phrase && !phrase.is_empty() {
        terms.push(format!("\"{phrase}\""));
    }
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn row_to_law(r: &rusqlite::Row<'_>) -> Result<LawInfo> {
    Ok(LawInfo {
        id: r.get(0)?,
        slug: r.get(1)?,
        jurabk: r.get(2)?,
        amtabk: r.get(3)?,
        title: r.get(4)?,
        doknr: r.get(5)?,
        builddate: r.get(6)?,
        stand: r.get(7)?,
        neufassung: r.get(8)?,
        ausfertigung_datum: r.get(9)?,
        fundstelle: r.get(10)?,
        imported_at: r.get(11)?,
    })
}

fn row_to_norm_info(r: &rusqlite::Row<'_>) -> Result<NormInfo> {
    Ok(NormInfo {
        id: r.get(0)?,
        law_id: r.get(1)?,
        unit_id: r.get(2)?,
        doknr: r.get(3)?,
        enbez: r.get(4)?,
        titel: r.get(5)?,
        position: r.get(6)?,
    })
}

fn row_to_norm(r: &rusqlite::Row<'_>) -> Result<Norm> {
    let info = row_to_norm_info(r)?;
    let blocks: String = r.get(7)?;
    let footnotes: String = r.get(8)?;
    let fussnoten: String = r.get(9)?;
    Ok(Norm {
        info,
        blocks: serde_json::from_str::<Vec<Block>>(&blocks).unwrap_or_default(),
        footnotes: serde_json::from_str::<Vec<Footnote>>(&footnotes).unwrap_or_default(),
        fussnoten: serde_json::from_str::<Vec<Block>>(&fussnoten).unwrap_or_default(),
    })
}

fn row_to_annotation(r: &rusqlite::Row<'_>) -> Result<Annotation> {
    let kind: String = r.get(7)?;
    let target: Option<String> = r.get(10)?;
    Ok(Annotation {
        id: r.get(0)?,
        law: r.get(1)?,
        norm: r.get(2)?,
        paragraph: r.get(3)?,
        start: r.get(4)?,
        end: r.get(5)?,
        quote: r.get(6)?,
        kind: AnnotationKind::parse(&kind).unwrap_or(AnnotationKind::Note),
        color: r.get(8)?,
        note: r.get(9)?,
        target: target.and_then(|t| serde_json::from_str::<LinkTarget>(&t).ok()),
        created: r.get(11)?,
        modified: r.get(12)?,
        orphaned: r.get::<_, i64>(13)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::xml::{ParsedLawMeta, ParsedNorm, ParsedUnit};
    use crate::model::text::Span;

    fn sample_law(text_433: &str) -> ParsedLaw {
        ParsedLaw {
            meta: ParsedLawMeta {
                doknr: "BJNR001950896".into(),
                builddate: "20260910".into(),
                jurabk: "BGB".into(),
                amtabk: Some("BGB".into()),
                title: "Bürgerliches Gesetzbuch".into(),
                stand: Some("Stand A".into()),
                ..Default::default()
            },
            units: vec![
                ParsedUnit {
                    kennzahl: "010".into(),
                    bez: "Buch 1".into(),
                    titel: Some("Allgemeiner Teil".into()),
                    ..Default::default()
                },
                ParsedUnit {
                    kennzahl: "010010".into(),
                    bez: "Abschnitt 1".into(),
                    parent: Some(0),
                    depth: 1,
                    ..Default::default()
                },
            ],
            norms: vec![
                ParsedNorm {
                    doknr: "X1".into(),
                    enbez: Some("§ 1".into()),
                    titel: Some("Beginn der Rechtsfähigkeit".into()),
                    unit: Some(1),
                    blocks: vec![Block::Paragraph {
                        spans: vec![Span::plain("Die Rechtsfähigkeit beginnt mit der Geburt.")],
                    }],
                    ..Default::default()
                },
                ParsedNorm {
                    doknr: "X433".into(),
                    enbez: Some("§ 433".into()),
                    titel: Some("Vertragstypische Pflichten".into()),
                    unit: Some(1),
                    blocks: vec![
                        Block::Paragraph {
                            spans: vec![Span::plain("(1) Der Veräußerer verpflichtet sich.")],
                        },
                        Block::Paragraph {
                            spans: vec![Span::plain(text_433)],
                        },
                    ],
                    ..Default::default()
                },
            ],
        }
    }

    #[test]
    fn import_and_query() {
        let mut db = Database::open_in_memory().unwrap();
        let law_id = db
            .replace_law("bgb", &sample_law("(2) Der Käufer zahlt."))
            .unwrap();
        let law = db.law_by_slug("bgb").unwrap().unwrap();
        assert_eq!(law.id, law_id);
        assert_eq!(law.jurabk, "BGB");
        assert!(db.law_by_abbrev("bgb").unwrap().is_some());

        let units = db.units(law_id).unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[1].parent_id, Some(units[0].id));

        let norms = db.norm_infos(law_id).unwrap();
        assert_eq!(norms.len(), 2);
        assert_eq!(norms[1].display_name(), "§ 433 Vertragstypische Pflichten");

        let n = db.norm_by_enbez(law_id, "§ 433").unwrap().unwrap();
        assert_eq!(n.blocks.len(), 2);
        let paras = db.paragraphs(n.info.id).unwrap();
        assert_eq!(paras[0].1.as_deref(), Some("(1)"));
        assert_eq!(paras[1].2, "(2) Der Käufer zahlt.\n");

        assert_eq!(db.neighbor_norm(n.info.id, -1).unwrap(), Some(norms[0].id));
        assert_eq!(db.neighbor_norm(n.info.id, 1).unwrap(), None);
        assert_eq!(db.first_norm_id(law_id).unwrap(), Some(norms[0].id));
    }

    #[test]
    fn fulltext_search() {
        let mut db = Database::open_in_memory().unwrap();
        let law_id = db
            .replace_law("bgb", &sample_law("(2) Der Käufer zahlt."))
            .unwrap();
        let hits = db.search(Some(law_id), "käufer", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].norm.enbez.as_deref(), Some("§ 433"));
        assert!(hits[0].snippet.contains("\u{1}Käufer\u{2}"));
        let hits = db.search(Some(law_id), "rechtsf", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(db.search(Some(law_id), "", 10).unwrap().is_empty());
        assert!(
            db.search(Some(law_id), "\"Veräußerer verpflichtet\"", 10)
                .unwrap()
                .len()
                == 1
        );
        assert!(
            db.search(Some(law_id), "verpflichtet )(", 10)
                .unwrap()
                .len()
                == 1
        );
    }

    #[test]
    fn annotations_roundtrip_and_reanchor() {
        let mut db = Database::open_in_memory().unwrap();
        db.replace_law("bgb", &sample_law("(2) Der Käufer zahlt."))
            .unwrap();
        let a = Annotation {
            id: 0,
            law: "bgb".into(),
            norm: "§ 433".into(),
            paragraph: 2,
            start: 8,
            end: 14,
            quote: "Käufer".into(),
            kind: AnnotationKind::Highlight,
            color: Some("yellow".into()),
            note: None,
            target: None,
            created: String::new(),
            modified: String::new(),
            orphaned: false,
        };
        let id = db.insert_annotation(&a).unwrap();
        let bookmark = Annotation {
            kind: AnnotationKind::Bookmark,
            quote: String::new(),
            start: 0,
            end: 0,
            paragraph: 0,
            color: None,
            ..a.clone()
        };
        db.insert_annotation(&bookmark).unwrap();
        assert_eq!(db.annotations_for_norm("bgb", "§ 433").unwrap().len(), 2);

        // Text verschiebt sich: Wortlaut wird wiedergefunden.
        db.replace_law("bgb", &sample_law("(2) Nun zahlt der Käufer."))
            .unwrap();
        let report = db.reanchor_law("bgb").unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(report.unchanged, 1);
        let moved = db.annotation(id).unwrap().unwrap();
        assert_eq!(moved.start, 18);
        assert_eq!(moved.end, 24);
        assert!(!moved.orphaned);

        // Wortlaut verschwindet: verwaist.
        db.replace_law("bgb", &sample_law("(2) Der Erwerber zahlt."))
            .unwrap();
        let report = db.reanchor_law("bgb").unwrap();
        assert_eq!(report.orphaned, 1);
        assert!(db.annotation(id).unwrap().unwrap().orphaned);

        // Wortlaut kehrt zurück: wiederhergestellt.
        db.replace_law("bgb", &sample_law("(2) Der Käufer zahlt."))
            .unwrap();
        let report = db.reanchor_law("bgb").unwrap();
        assert_eq!(report.recovered, 1);

        // Export / Import
        let json = db.export_annotations_json().unwrap();
        assert!(json.contains("\"kind\": \"highlight\""));
        assert_eq!(db.import_annotations_json(&json).unwrap(), 0);
        db.delete_annotation(id).unwrap();
        assert_eq!(db.import_annotations_json(&json).unwrap(), 1);
        assert_eq!(db.all_annotations().unwrap().len(), 2);
        assert!(db.import_annotations_json("kaputt").is_err());
    }

    #[test]
    fn fts_query_building() {
        assert_eq!(build_fts_query("  "), None);
        assert_eq!(
            build_fts_query("Kauf vertrag"),
            Some("\"Kauf\"* AND \"vertrag\"*".into())
        );
        assert_eq!(
            build_fts_query("\"guter Glaube\""),
            Some("\"guter Glaube\"".into())
        );
        assert_eq!(
            build_fts_query("a) OR (b"),
            Some("\"a\"* AND \"OR\"* AND \"b\"*".into())
        );
    }

    #[test]
    fn char_slice_handles_umlauts() {
        assert_eq!(char_slice("Käufer zahlt", 0, 6), Some("Käufer"));
        assert_eq!(char_slice("Käufer zahlt", 7, 12), Some("zahlt"));
        assert_eq!(char_slice("abc", 2, 5), Some("c"));
        assert_eq!(char_slice("abc", 5, 6), None);
        assert_eq!(find_char_pos("Käufer zahlt", "zahlt"), Some(7));
    }
}
