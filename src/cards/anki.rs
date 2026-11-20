// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Liest Anki-Kartenpakete (`.apkg`).
//!
//! Ein Paket ist ein Zip-Archiv mit einer SQLite-Sammlung und den Medien.
//! Unterstützt werden alle drei verbreiteten Fassungen: `collection.anki2`,
//! `collection.anki21` (Notiztypen und Stapel als JSON in der Tabelle `col`)
//! und `collection.anki21b` (Zstandard-komprimiert, eigene Tabellen, Vorlagen
//! als Protobuf). Aus Notizen und Vorlagen entstehen fertige Vorder- und
//! Rückseiten; der Lernstand des Pakets wird nicht übernommen.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gettextrs::gettext;
use rusqlite::{Connection, OpenFlags};
use ruzstd::decoding::StreamingDecoder;

use super::html::image_sources;
use super::template::{self, Context};
use super::DECK_SEPARATOR;

/// Trenner der Felder einer Notiz und (in neuen Sammlungen) der Stapelnamen.
const UNIT_SEPARATOR: char = '\u{1f}';
/// Obergrenze für entpackte Sammlungen und Medien (Schutz vor Zip-Bomben).
const MAX_UNPACKED: u64 = 1 << 30;

#[derive(Debug)]
pub enum AnkiError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Db(rusqlite::Error),
    /// Die Datei ist kein lesbares Kartenpaket.
    Format(String),
}

impl std::fmt::Display for AnkiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Zip(e) => write!(f, "{e}"),
            Self::Db(e) => write!(f, "{e}"),
            Self::Format(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AnkiError {}

impl From<std::io::Error> for AnkiError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<zip::result::ZipError> for AnkiError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

impl From<rusqlite::Error> for AnkiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

type Result<T> = std::result::Result<T, AnkiError>;

/// Eine fertig erzeugte Karte eines Pakets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCard {
    /// Karten-ID des Pakets; an ihr wird die Karte beim erneuten Import
    /// wiedererkannt.
    pub anki_id: i64,
    pub front: String,
    pub back: String,
    pub tags: String,
}

/// Ein Stapel eines Pakets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiDeck {
    /// Vollständiger Name, Ebenen durch „::“ getrennt.
    pub name: String,
    pub cards: Vec<AnkiCard>,
}

impl AnkiDeck {
    /// Dateinamen aller Bilder, die die Karten des Stapels einbinden.
    pub fn media_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for card in &self.cards {
            names.extend(image_sources(&card.front));
            names.extend(image_sources(&card.back));
        }
        names
    }
}

/// Liest alle Stapel eines Pakets. Stapel ohne Karten entfallen.
pub fn read_package(path: &Path) -> Result<Vec<AnkiDeck>> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let collection = read_collection(&mut archive)?;
    // SQLite braucht eine Datei; sie liegt nur für die Dauer des Lesens vor.
    let temp = TempFile::create(&collection)?;
    drop(collection);
    let conn = Connection::open_with_flags(
        &temp.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // Neue Sammlungen sortieren Namen mit einer eigenen Vergleichsfunktion.
    conn.create_collation("unicase", |a, b| a.to_lowercase().cmp(&b.to_lowercase()))?;
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    read_decks(&conn, &fallback)
}

/// Entpackt die genannten Mediendateien des Pakets nach `dest`. Liefert die
/// Zahl der geschriebenen Dateien; fehlende Medien werden übergangen.
pub fn extract_media(path: &Path, names: &HashSet<String>, dest: &Path) -> Result<usize> {
    if names.is_empty() {
        return Ok(0);
    }
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let (index, compressed) = read_media_index(&mut archive)?;
    std::fs::create_dir_all(dest)?;
    let mut written = 0;
    for (entry, name) in index {
        // Nur der reine Dateiname zählt; Pfadanteile könnten aus dem
        // Zielordner herausführen.
        let Some(file_name) = Path::new(&name).file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_name != name || !names.contains(&name) {
            continue;
        }
        let Ok(file) = archive.by_name(&entry) else {
            continue;
        };
        let mut data = Vec::new();
        file.take(MAX_UNPACKED).read_to_end(&mut data)?;
        if compressed {
            data = zstd_decode(&data)?;
        }
        File::create(dest.join(file_name))?.write_all(&data)?;
        written += 1;
    }
    Ok(written)
}

/// Sammlung des Pakets als SQLite-Abbild (neueste vorhandene Fassung).
fn read_collection<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Vec<u8>> {
    for (name, compressed) in [
        ("collection.anki21b", true),
        ("collection.anki21", false),
        ("collection.anki2", false),
    ] {
        let Ok(file) = archive.by_name(name) else {
            continue;
        };
        let mut data = Vec::new();
        file.take(MAX_UNPACKED).read_to_end(&mut data)?;
        return if compressed {
            zstd_decode(&data)
        } else {
            Ok(data)
        };
    }
    Err(AnkiError::Format(gettext(
        "Das Archiv enthält keine Kartensammlung (collection.anki2).",
    )))
}

/// Zuordnung „Name im Archiv → Dateiname“ der Medien und die Angabe, ob die
/// Dateien Zstandard-komprimiert sind (neues Paketformat).
fn read_media_index<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<(Vec<(String, String)>, bool)> {
    let mut data = Vec::new();
    match archive.by_name("media") {
        Ok(file) => {
            file.take(MAX_UNPACKED).read_to_end(&mut data)?;
        }
        Err(_) => return Ok((Vec::new(), false)),
    }
    // Altes Format: JSON-Objekt {"0": "bild.png", …}.
    if let Ok(map) = serde_json::from_slice::<BTreeMap<String, String>>(&data) {
        return Ok((map.into_iter().collect(), false));
    }
    // Neues Format: komprimierte Protobuf-Liste; der Platz in der Liste ist
    // der Name im Archiv.
    let decoded = zstd_decode(&data)?;
    let mut index = Vec::new();
    for (field, value) in ProtoReader::new(&decoded) {
        let (1, ProtoValue::Bytes(entry)) = (field, value) else {
            continue;
        };
        let mut name = String::new();
        let mut legacy = None;
        for (field, value) in ProtoReader::new(entry) {
            match (field, value) {
                (1, ProtoValue::Bytes(b)) => name = String::from_utf8_lossy(b).into_owned(),
                (255, ProtoValue::Varint(n)) => legacy = Some(n),
                _ => {}
            }
        }
        let entry_name = legacy.unwrap_or(index.len() as u64).to_string();
        index.push((entry_name, name));
    }
    Ok((index, true))
}

/// Entpackt Zstandard-Daten (auch mehrere aneinandergehängte Rahmen).
fn zstd_decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut source = data;
    let mut out = Vec::new();
    while !source.is_empty() {
        let mut decoder = StreamingDecoder::new(&mut source)
            .map_err(|e| AnkiError::Format(format!("Zstandard: {e}")))?;
        decoder.read_to_end(&mut out)?;
        if out.len() as u64 > MAX_UNPACKED {
            return Err(AnkiError::Format(gettext("Das Paket ist zu groß.")));
        }
    }
    Ok(out)
}

/// Temporäre Datei, die beim Verwerfen gelöscht wird.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn create(data: &[u8]) -> Result<Self> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "gesetze-anki-{}-{}.sqlite",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        File::create(&path)?.write_all(data)?;
        Ok(Self { path })
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ----------------------------------------------------------------------
// Sammlung
// ----------------------------------------------------------------------

struct CardTemplate {
    name: String,
    question: String,
    answer: String,
}

struct Notetype {
    name: String,
    cloze: bool,
    fields: Vec<String>,
    templates: Vec<CardTemplate>,
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?
        .exists([name])?)
}

fn read_decks(conn: &Connection, fallback_name: &str) -> Result<Vec<AnkiDeck>> {
    if !table_exists(conn, "notes")? || !table_exists(conn, "cards")? {
        return Err(AnkiError::Format(gettext(
            "Die Sammlung enthält keine Karten.",
        )));
    }
    let modern = table_exists(conn, "notetypes")?;
    let (notetypes, deck_names) = if modern {
        (modern_notetypes(conn)?, modern_deck_names(conn)?)
    } else {
        legacy_metadata(conn)?
    };
    let mut decks: BTreeMap<i64, AnkiDeck> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT c.id, c.did, c.odid, c.ord, n.mid, n.flds, n.tags
         FROM cards c JOIN notes n ON n.id = c.nid
         ORDER BY n.id, c.ord",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let anki_id: i64 = row.get(0)?;
        let did: i64 = row.get(1)?;
        let odid: i64 = row.get(2)?;
        let ord: i64 = row.get(3)?;
        let mid: i64 = row.get(4)?;
        let flds: String = row.get(5)?;
        let tags: String = row.get(6)?;
        let Some(notetype) = notetypes.get(&mid) else {
            continue;
        };
        // Karten in Auswahlstapeln gehören zu ihrem Heimatstapel.
        let deck_id = if odid != 0 { odid } else { did };
        let deck_name = match deck_names.get(&deck_id).map(String::as_str) {
            // Der namenlose Standardstapel heißt wie die Datei.
            None | Some("Default") | Some("Standard") | Some("") => fallback_name,
            Some(name) => name,
        };
        let ord = u16::try_from(ord).unwrap_or(0);
        let Some((front, back)) = render_card(notetype, ord, &flds, tags.trim(), deck_name) else {
            continue;
        };
        decks
            .entry(deck_id)
            .or_insert_with(|| AnkiDeck {
                name: deck_name.to_owned(),
                cards: Vec::new(),
            })
            .cards
            .push(AnkiCard {
                anki_id,
                front,
                back,
                tags: tags.trim().to_owned(),
            });
    }
    let mut decks: Vec<AnkiDeck> = decks.into_values().collect();
    decks.sort_by_key(|d| d.name.to_lowercase());
    Ok(decks)
}

/// Erzeugt Vorder- und Rückseite einer Karte aus Notiz und Vorlage.
fn render_card(
    notetype: &Notetype,
    ord: u16,
    flds: &str,
    tags: &str,
    deck: &str,
) -> Option<(String, String)> {
    // Lückentexte haben eine Vorlage für alle Karten.
    let index = if notetype.cloze { 0 } else { usize::from(ord) };
    let template = notetype.templates.get(index)?;
    let mut values = flds.split(UNIT_SEPARATOR);
    let fields: Vec<(String, String)> = notetype
        .fields
        .iter()
        .map(|name| (name.clone(), values.next().unwrap_or_default().to_owned()))
        .collect();
    let mut ctx = Context {
        fields: &fields,
        ord,
        cloze: notetype.cloze,
        answer: false,
        front_side: "",
        tags,
        deck,
        card: &template.name,
        notetype: &notetype.name,
    };
    let front = template::render(&template.question, &ctx);
    ctx.answer = true;
    let back = template::render_back(&template.answer, &ctx);
    Some((front, back))
}

/// Notiztypen und Stapelnamen alter Sammlungen (JSON in der Tabelle `col`).
fn legacy_metadata(conn: &Connection) -> Result<(HashMap<i64, Notetype>, HashMap<i64, String>)> {
    let (models, decks): (String, String) =
        conn.query_row("SELECT models, decks FROM col", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let models: serde_json::Value = serde_json::from_str(&models)
        .map_err(|e| AnkiError::Format(format!("Notiztypen unlesbar: {e}")))?;
    let decks: serde_json::Value = serde_json::from_str(&decks)
        .map_err(|e| AnkiError::Format(format!("Stapel unlesbar: {e}")))?;
    let mut notetypes = HashMap::new();
    for (id, model) in models.as_object().into_iter().flatten() {
        let Ok(id) = id.parse::<i64>() else {
            continue;
        };
        let text = |value: &serde_json::Value, key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let ordered = |key: &str| {
            let mut items: Vec<&serde_json::Value> = model
                .get(key)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().collect())
                .unwrap_or_default();
            items.sort_by_key(|v| v.get("ord").and_then(|o| o.as_i64()).unwrap_or(0));
            items
        };
        notetypes.insert(
            id,
            Notetype {
                name: text(model, "name"),
                cloze: model.get("type").and_then(|t| t.as_i64()) == Some(1),
                fields: ordered("flds").iter().map(|f| text(f, "name")).collect(),
                templates: ordered("tmpls")
                    .iter()
                    .map(|t| CardTemplate {
                        name: text(t, "name"),
                        question: text(t, "qfmt"),
                        answer: text(t, "afmt"),
                    })
                    .collect(),
            },
        );
    }
    let mut names = HashMap::new();
    for (id, deck) in decks.as_object().into_iter().flatten() {
        if let (Ok(id), Some(name)) = (id.parse::<i64>(), deck.get("name").and_then(|n| n.as_str()))
        {
            names.insert(id, name.to_owned());
        }
    }
    Ok((notetypes, names))
}

/// Notiztypen neuer Sammlungen (Tabellen `notetypes`, `fields`, `templates`).
fn modern_notetypes(conn: &Connection) -> Result<HashMap<i64, Notetype>> {
    let mut notetypes = HashMap::new();
    let mut stmt = conn.prepare("SELECT id, name, config FROM notetypes")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let config: Vec<u8> = row.get(2)?;
        // Feld 1 der Konfiguration: Art (0 = normal, 1 = Lückentext).
        let cloze = ProtoReader::new(&config)
            .any(|(field, value)| field == 1 && value == ProtoValue::Varint(1));
        notetypes.insert(
            row.get::<_, i64>(0)?,
            Notetype {
                name: row.get(1)?,
                cloze,
                fields: Vec::new(),
                templates: Vec::new(),
            },
        );
    }
    let mut stmt = conn.prepare("SELECT ntid, name FROM fields ORDER BY ntid, ord")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if let Some(notetype) = notetypes.get_mut(&row.get::<_, i64>(0)?) {
            notetype.fields.push(row.get(1)?);
        }
    }
    let mut stmt = conn.prepare("SELECT ntid, name, config FROM templates ORDER BY ntid, ord")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let config: Vec<u8> = row.get(2)?;
        let mut template = CardTemplate {
            name: row.get(1)?,
            question: String::new(),
            answer: String::new(),
        };
        // Felder 1 und 2 der Konfiguration: Frage- und Antwortvorlage.
        for (field, value) in ProtoReader::new(&config) {
            match (field, value) {
                (1, ProtoValue::Bytes(b)) => {
                    template.question = String::from_utf8_lossy(b).into_owned()
                }
                (2, ProtoValue::Bytes(b)) => {
                    template.answer = String::from_utf8_lossy(b).into_owned()
                }
                _ => {}
            }
        }
        if let Some(notetype) = notetypes.get_mut(&row.get::<_, i64>(0)?) {
            notetype.templates.push(template);
        }
    }
    Ok(notetypes)
}

fn modern_deck_names(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut names = HashMap::new();
    let mut stmt = conn.prepare("SELECT id, name FROM decks")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        names.insert(
            row.get::<_, i64>(0)?,
            name.replace(UNIT_SEPARATOR, DECK_SEPARATOR),
        );
    }
    Ok(names)
}

// ----------------------------------------------------------------------
// Protobuf (nur Lesen der wenigen benötigten Felder)
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtoValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed(u64),
}

/// Liefert nacheinander (Feldnummer, Wert) einer Protobuf-Nachricht; bei
/// beschädigten Daten endet die Folge.
struct ProtoReader<'a> {
    data: &'a [u8],
}

impl<'a> ProtoReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = self.data.split_first()?;
            self.data = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        if len > self.data.len() {
            return None;
        }
        let (head, rest) = self.data.split_at(len);
        self.data = rest;
        Some(head)
    }
}

impl<'a> Iterator for ProtoReader<'a> {
    type Item = (u64, ProtoValue<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.varint()?;
        let field = key >> 3;
        let value = match key & 7 {
            0 => ProtoValue::Varint(self.varint()?),
            1 => ProtoValue::Fixed(u64::from_le_bytes(self.take(8)?.try_into().ok()?)),
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                ProtoValue::Bytes(self.take(len)?)
            }
            5 => ProtoValue::Fixed(u64::from(u32::from_le_bytes(
                self.take(4)?.try_into().ok()?,
            ))),
            _ => return None,
        };
        Some((field, value))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ruzstd::encoding::{compress_to_vec, CompressionLevel};

    /// Verzeichnis für Testdateien, das beim Verwerfen gelöscht wird.
    pub(crate) struct TestDir(pub PathBuf);

    impl TestDir {
        pub(crate) fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "gesetze-test-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).expect("Testverzeichnis");
            Self(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const NOTES_AND_CARDS: &str = "
        CREATE TABLE notes (id INTEGER PRIMARY KEY, guid TEXT, mid INTEGER, mod INTEGER,
            usn INTEGER, tags TEXT, flds TEXT, sfld TEXT, csum INTEGER, flags INTEGER, data TEXT);
        CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER, did INTEGER, ord INTEGER,
            mod INTEGER, usn INTEGER, type INTEGER, queue INTEGER, due INTEGER, ivl INTEGER,
            factor INTEGER, reps INTEGER, lapses INTEGER, left INTEGER, odue INTEGER,
            odid INTEGER, flags INTEGER, data TEXT);
        INSERT INTO notes VALUES
            (10, 'a', 1, 0, 0, ' bgb ', 'Was regelt § 433 BGB?\u{1f}Den <b>Kaufvertrag</b><img src=\"schema.png\">', '', 0, 0, ''),
            (11, 'b', 2, 0, 0, '', 'Der {{c1::Verkäufer}} schuldet {{c2::Übergabe}}.\u{1f}', '', 0, 0, '');
        INSERT INTO cards VALUES
            (100, 10, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, ''),
            (101, 10, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, ''),
            (102, 11, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, ''),
            (103, 11, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, '');
    ";

    fn legacy_collection(dir: &Path) -> Vec<u8> {
        let path = dir.join("legacy.sqlite");
        let conn = Connection::open(&path).expect("Sammlung");
        conn.execute_batch(NOTES_AND_CARDS).expect("Schema");
        let models = serde_json::json!({
            "1": {"name": "Einfach (beide Richtungen)", "type": 0,
                  "flds": [{"name": "Rückseite", "ord": 1}, {"name": "Vorderseite", "ord": 0}],
                  "tmpls": [
                      {"name": "Karte 1", "ord": 0, "qfmt": "{{Vorderseite}}",
                       "afmt": "{{FrontSide}}<hr id=answer>{{Rückseite}}"},
                      {"name": "Karte 2", "ord": 1, "qfmt": "{{Rückseite}}",
                       "afmt": "{{FrontSide}}<hr id=answer>{{Vorderseite}}"}]},
            "2": {"name": "Lückentext", "type": 1,
                  "flds": [{"name": "Text", "ord": 0}, {"name": "Extra", "ord": 1}],
                  "tmpls": [{"name": "Lücke", "ord": 0, "qfmt": "{{cloze:Text}}",
                             "afmt": "{{cloze:Text}}<br>{{Extra}}"}]},
        });
        let decks = serde_json::json!({
            "1": {"name": "Default"},
            "5": {"name": "Schuldrecht BT::Kaufrecht"},
            "6": {"name": "Schuldrecht BT::Lücken"},
        });
        conn.execute_batch("CREATE TABLE col (id INTEGER PRIMARY KEY, models TEXT, decks TEXT)")
            .expect("col");
        conn.execute(
            "INSERT INTO col VALUES (1, ?1, ?2)",
            [models.to_string(), decks.to_string()],
        )
        .expect("col füllen");
        drop(conn);
        std::fs::read(&path).expect("Sammlung lesen")
    }

    fn proto_string(field: u8, value: &str) -> Vec<u8> {
        let mut out = vec![field << 3 | 2, value.len() as u8];
        out.extend_from_slice(value.as_bytes());
        out
    }

    fn modern_collection(dir: &Path) -> Vec<u8> {
        let path = dir.join("modern.sqlite");
        let conn = Connection::open(&path).expect("Sammlung");
        conn.create_collation("unicase", |a, b| a.to_lowercase().cmp(&b.to_lowercase()))
            .expect("Sortierung");
        conn.execute_batch(NOTES_AND_CARDS).expect("Schema");
        conn.execute_batch(
            "CREATE TABLE notetypes (id INTEGER PRIMARY KEY, name TEXT COLLATE unicase,
                 mtime_secs INTEGER, usn INTEGER, config BLOB);
             CREATE TABLE fields (ntid INTEGER, ord INTEGER, name TEXT COLLATE unicase, config BLOB);
             CREATE TABLE templates (ntid INTEGER, ord INTEGER, name TEXT COLLATE unicase,
                 mtime_secs INTEGER, usn INTEGER, config BLOB);
             CREATE TABLE decks (id INTEGER PRIMARY KEY, name TEXT COLLATE unicase,
                 mtime_secs INTEGER, usn INTEGER, common BLOB, kind BLOB);
             CREATE UNIQUE INDEX idx_decks_name ON decks (name);",
        )
        .expect("neue Tabellen");
        let mut q1 = proto_string(1, "{{Vorderseite}}");
        q1.extend(proto_string(2, "{{FrontSide}}<hr id=answer>{{Rückseite}}"));
        let mut q2 = proto_string(1, "{{Rückseite}}");
        q2.extend(proto_string(
            2,
            "{{FrontSide}}<hr id=answer>{{Vorderseite}}",
        ));
        let mut cloze = proto_string(1, "{{cloze:Text}}");
        cloze.extend(proto_string(2, "{{cloze:Text}}<br>{{Extra}}"));
        conn.execute(
            "INSERT INTO notetypes VALUES (1, 'Einfach', 0, 0, ?1), (2, 'Lückentext', 0, 0, ?2)",
            rusqlite::params![vec![8u8, 0], vec![8u8, 1]],
        )
        .expect("Notiztypen");
        conn.execute_batch(
            "INSERT INTO fields VALUES (1, 0, 'Vorderseite', x''), (1, 1, 'Rückseite', x''),
                 (2, 0, 'Text', x''), (2, 1, 'Extra', x'');",
        )
        .expect("Felder");
        conn.execute(
            "INSERT INTO templates VALUES (1, 0, 'Karte 1', 0, 0, ?1), (1, 1, 'Karte 2', 0, 0, ?2),
                 (2, 0, 'Lücke', 0, 0, ?3)",
            rusqlite::params![q1, q2, cloze],
        )
        .expect("Vorlagen");
        conn.execute(
            "INSERT INTO decks VALUES (1, 'Default', 0, 0, x'', x''),
                 (5, ?1, 0, 0, x'', x''), (6, ?2, 0, 0, x'', x'')",
            [
                "Schuldrecht BT\u{1f}Kaufrecht",
                "Schuldrecht BT\u{1f}Lücken",
            ],
        )
        .expect("Stapel");
        drop(conn);
        std::fs::read(&path).expect("Sammlung lesen")
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).expect("Zip anlegen"));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).expect("Eintrag");
            zip.write_all(data).expect("Daten");
        }
        zip.finish().expect("Zip abschließen");
    }

    /// Schreibt ein Paket im alten Format und liefert seinen Pfad.
    pub(crate) fn legacy_package(dir: &Path) -> PathBuf {
        let path = dir.join("Zivilrecht.apkg");
        let collection = legacy_collection(dir);
        write_zip(
            &path,
            &[
                ("collection.anki2", &collection),
                ("media", br#"{"0": "schema.png", "1": "../boese.png"}"#),
                ("0", b"PNG-Daten"),
                ("1", b"unerwuenscht"),
            ],
        );
        path
    }

    fn check_decks(decks: &[AnkiDeck]) {
        let names: Vec<&str> = decks.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            ["Schuldrecht BT::Kaufrecht", "Schuldrecht BT::Lücken"]
        );
        let kauf = &decks[0];
        assert_eq!(kauf.cards.len(), 2);
        assert_eq!(kauf.cards[0].anki_id, 100);
        assert_eq!(kauf.cards[0].front, "Was regelt § 433 BGB?");
        assert_eq!(
            kauf.cards[0].back,
            "Den <b>Kaufvertrag</b><img src=\"schema.png\">"
        );
        assert_eq!(kauf.cards[0].tags, "bgb");
        // Zweite Karte der Notiz: umgekehrte Richtung.
        assert_eq!(kauf.cards[1].back, "Was regelt § 433 BGB?");
        assert_eq!(kauf.media_names(), HashSet::from(["schema.png".to_owned()]));
        // Lückentext: Karte 103 liegt in einem Auswahlstapel und gehört
        // über „odid“ zum Stapel „Lücken“.
        let cloze = &decks[1];
        assert_eq!(cloze.cards.len(), 2);
        assert_eq!(
            cloze.cards[0].front,
            "Der <span class=\"cloze\">[…]</span> schuldet Übergabe."
        );
        assert_eq!(
            cloze.cards[1].back,
            "Der Verkäufer schuldet <span class=\"cloze\">Übergabe</span>.<br>"
        );
    }

    #[test]
    fn reads_legacy_package_with_media() {
        let dir = TestDir::new("anki-alt");
        let path = legacy_package(&dir.0);
        let decks = read_package(&path).expect("Paket");
        check_decks(&decks);
        let media = dir.0.join("medien");
        let names = HashSet::from(["schema.png".to_owned(), "../boese.png".to_owned()]);
        assert_eq!(extract_media(&path, &names, &media).expect("Medien"), 1);
        assert_eq!(
            std::fs::read(media.join("schema.png")).expect("Bild"),
            b"PNG-Daten"
        );
        assert!(!dir.0.join("boese.png").exists());
    }

    #[test]
    fn reads_compressed_package_with_new_schema() {
        let dir = TestDir::new("anki-neu");
        let path = dir.0.join("Zivilrecht.apkg");
        let collection = compress_to_vec(
            modern_collection(&dir.0).as_slice(),
            CompressionLevel::Fastest,
        );
        // Medienliste: ein Eintrag mit Feld 1 = Name.
        let entry = proto_string(1, "schema.png");
        let mut list = vec![1 << 3 | 2, entry.len() as u8];
        list.extend(entry);
        let media = compress_to_vec(list.as_slice(), CompressionLevel::Fastest);
        let image = compress_to_vec(&b"PNG-Daten"[..], CompressionLevel::Fastest);
        write_zip(
            &path,
            &[
                ("collection.anki2", b"Platzhalter"),
                ("collection.anki21b", &collection),
                ("media", &media),
                ("0", &image),
                ("meta", &[8, 3]),
            ],
        );
        let decks = read_package(&path).expect("Paket");
        check_decks(&decks);
        let dest = dir.0.join("medien");
        let names = HashSet::from(["schema.png".to_owned()]);
        assert_eq!(extract_media(&path, &names, &dest).expect("Medien"), 1);
        assert_eq!(
            std::fs::read(dest.join("schema.png")).expect("Bild"),
            b"PNG-Daten"
        );
    }

    #[test]
    fn rejects_other_archives() {
        let dir = TestDir::new("anki-falsch");
        let path = dir.0.join("leer.apkg");
        write_zip(&path, &[("liesmich.txt", b"kein Paket")]);
        assert!(matches!(read_package(&path), Err(AnkiError::Format(_))));
        let text = dir.0.join("text.apkg");
        std::fs::write(&text, "kein Zip").expect("Datei");
        assert!(matches!(read_package(&text), Err(AnkiError::Zip(_))));
    }

    #[test]
    fn proto_reader_stops_on_damaged_data() {
        let fields: Vec<_> = ProtoReader::new(&[8, 150, 1, 18, 2, b'o', b'k']).collect();
        assert_eq!(
            fields,
            [(1, ProtoValue::Varint(150)), (2, ProtoValue::Bytes(b"ok"))]
        );
        // Längenangabe über das Ende hinaus.
        assert_eq!(ProtoReader::new(&[18, 9, b'x']).count(), 0);
    }
}
