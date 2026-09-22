//! Import von Gesetzen: Entpacken der XML-Datei und Schreiben in die Datenbank.
//! Download und Aktualisierungsprüfung folgen in einer späteren Stufe.
// Wird ab den folgenden Stufen von der Oberfläche genutzt.
#![allow(dead_code)]

pub mod xml;

use std::io::Read;
use std::path::Path;

use crate::db::Database;

/// Bekannte Gesetzesquellen (Slug, Anzeigename, Download-URL).
pub const SOURCES: &[(&str, &str, &str)] = &[(
    "bgb",
    "Bürgerliches Gesetzbuch",
    "https://www.gesetze-im-internet.de/bgb/xml.zip",
)];

#[derive(Debug)]
pub enum ImportError {
    Zip(String),
    Xml(xml::XmlError),
    Db(rusqlite::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Zip(s) => write!(f, "ZIP-Fehler: {s}"),
            ImportError::Xml(e) => write!(f, "{e}"),
            ImportError::Db(e) => write!(f, "Datenbankfehler: {e}"),
        }
    }
}
impl std::error::Error for ImportError {}

impl From<xml::XmlError> for ImportError {
    fn from(e: xml::XmlError) -> Self {
        ImportError::Xml(e)
    }
}
impl From<rusqlite::Error> for ImportError {
    fn from(e: rusqlite::Error) -> Self {
        ImportError::Db(e)
    }
}

/// Liest die erste XML-Datei aus einem ZIP-Archiv.
pub fn extract_xml(zip_bytes: &[u8]) -> Result<String, ImportError> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| ImportError::Zip(e.to_string()))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ImportError::Zip(e.to_string()))?;
        if file.name().to_ascii_lowercase().ends_with(".xml") {
            let mut buf = String::new();
            file.read_to_string(&mut buf)
                .map_err(|e| ImportError::Zip(e.to_string()))?;
            return Ok(buf);
        }
    }
    Err(ImportError::Zip("keine XML-Datei im Archiv".into()))
}

/// Ergebnis eines Imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub law_id: i64,
    pub norms: usize,
    pub units: usize,
    pub stand: Option<String>,
    pub reanchored: crate::db::ReanchorReport,
}

/// Parst das XML und schreibt das Gesetz in die Datenbank unter `db_path`.
/// Blockierend; für den Aufruf aus der Oberfläche in `gio::spawn_blocking` kapseln.
pub fn import_xml(db_path: &Path, slug: &str, xml: &str) -> Result<ImportReport, ImportError> {
    let parsed = xml::parse_law(xml)?;
    let mut db = Database::open(db_path)?;
    let law_id = db.replace_law(slug, &parsed)?;
    let reanchored = db.reanchor_law(slug)?;
    Ok(ImportReport {
        law_id,
        norms: parsed.norms.len(),
        units: parsed.units.len(),
        stand: parsed.meta.stand.clone(),
        reanchored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_xml_from_zip() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("readme.txt", opts).unwrap();
            w.write_all(b"hi").unwrap();
            w.start_file("BJNR001950896.xml", opts).unwrap();
            w.write_all("<dokumente doknr=\"X\"/>".as_bytes()).unwrap();
            w.finish().unwrap();
        }
        let xml = extract_xml(buf.get_ref()).unwrap();
        assert!(xml.starts_with("<dokumente"));
        assert!(extract_xml(b"nicht zip").is_err());
    }
}
