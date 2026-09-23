// SPDX-FileCopyrightText: 2026 Jan-Henrik Koch
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Import von Gesetzen: Download des XML-Archivs von gesetze-im-internet.de,
//! Entpacken, Parsen und Schreiben in die Datenbank sowie die Prüfung auf
//! neue Fassungen.
//!
//! Der Download läuft asynchron über `soup3` im Hauptkontext, die
//! blockierenden Schritte (Entpacken, Parsen, Datenbank) laufen in
//! [`gio::spawn_blocking`]. Die Oberfläche bekommt den Fortschritt über
//! einen Callback ([`Progress`]).

pub mod xml;

use std::io::Read;
use std::path::{Path, PathBuf};

use gtk::{gio, glib};
use soup::prelude::*;

use crate::config::VERSION;
use crate::db::Database;
use crate::model::LawInfo;
use xml::ParsedLawMeta;

/// Eine bekannte Gesetzesquelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// Interner Schlüssel (z. B. `bgb`), zugleich `slug` in der Datenbank.
    pub slug: &'static str,
    /// Anzeigename.
    pub name: &'static str,
    /// Adresse des XML-Archivs.
    pub url: &'static str,
}

impl Source {
    /// Liefert die Quelle zu einem Slug.
    pub fn by_slug(slug: &str) -> Option<&'static Source> {
        SOURCES.iter().find(|s| s.slug == slug)
    }
}

/// Bekannte Gesetzesquellen.
pub const SOURCES: &[Source] = &[Source {
    slug: "bgb",
    name: "Bürgerliches Gesetzbuch",
    url: "https://www.gesetze-im-internet.de/bgb/xml.zip",
}];

/// Slug des Gesetzes, das beim ersten Start angeboten wird.
pub const DEFAULT_SLUG: &str = "bgb";

/// Größe der Leseblöcke beim Download.
const CHUNK_SIZE: usize = 64 * 1024;

/// Zeitlimit je Netzwerkoperation in Sekunden.
const TIMEOUT_SECONDS: u32 = 60;

#[derive(Debug)]
pub enum ImportError {
    Zip(String),
    Xml(xml::XmlError),
    Db(rusqlite::Error),
    /// Netzwerkfehler (Verbindung, HTTP-Status, Zeitüberschreitung).
    Network(String),
    /// Der Slug ist keiner bekannten Quelle zugeordnet.
    UnknownSource(String),
    /// Der Hintergrundthread ist abgebrochen.
    Cancelled,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Zip(s) => write!(f, "ZIP-Fehler: {s}"),
            ImportError::Xml(e) => write!(f, "{e}"),
            ImportError::Db(e) => write!(f, "Datenbankfehler: {e}"),
            ImportError::Network(s) => write!(f, "Netzwerkfehler: {s}"),
            ImportError::UnknownSource(s) => write!(f, "Unbekannte Gesetzesquelle: {s}"),
            ImportError::Cancelled => write!(f, "Der Import wurde abgebrochen"),
        }
    }
}
impl std::error::Error for ImportError {}

impl From<glib::Error> for ImportError {
    fn from(e: glib::Error) -> Self {
        ImportError::Network(e.to_string())
    }
}

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
    pub title: String,
    pub norms: usize,
    pub units: usize,
    pub stand: Option<String>,
    pub builddate: String,
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
        title: parsed.meta.title.clone(),
        norms: parsed.norms.len(),
        units: parsed.units.len(),
        stand: parsed.meta.stand.clone(),
        builddate: parsed.meta.builddate.clone(),
        reanchored,
    })
}

// ---------------------------------------------------------------------------
// Download und Aktualisierung
// ---------------------------------------------------------------------------

/// Fortschrittsmeldung während eines Imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Download läuft; `total` ist bekannt, wenn der Server `Content-Length` sendet.
    Downloading { received: u64, total: Option<u64> },
    /// Entpacken, Parsen und Schreiben in die Datenbank.
    Importing,
}

/// Lädt eine Datei vollständig herunter und meldet den Fortschritt.
///
/// Muss im GLib-Hauptkontext ausgeführt werden; blockiert nicht.
pub async fn download(
    url: &str,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, ImportError> {
    let session = soup::Session::new();
    // Ein abschließendes Leerzeichen lässt libsoup seine eigene Kennung anhängen.
    session.set_user_agent(&format!("gesetze/{VERSION} "));
    session.set_timeout(TIMEOUT_SECONDS);
    let msg = soup::Message::new("GET", url).map_err(|e| ImportError::Network(e.to_string()))?;
    let stream = session.send_future(&msg, glib::Priority::DEFAULT).await?;
    let status = msg.status_code();
    if !(200..300).contains(&status) {
        let reason = msg.reason_phrase().unwrap_or_default();
        return Err(ImportError::Network(format!("HTTP {status} {reason}")));
    }
    let total = msg
        .response_headers()
        .map(|h| h.content_length())
        .filter(|len| *len > 0)
        .map(|len| len as u64);
    let mut buf = Vec::with_capacity(total.unwrap_or(0) as usize);
    progress(0, total);
    loop {
        let bytes = stream
            .read_bytes_future(CHUNK_SIZE, glib::Priority::DEFAULT)
            .await?;
        if bytes.is_empty() {
            break;
        }
        buf.extend_from_slice(&bytes);
        progress(buf.len() as u64, total);
    }
    if let Err(err) = stream.close_future(glib::Priority::DEFAULT).await {
        log::debug!("Antwortstrom konnte nicht geschlossen werden: {err}");
    }
    Ok(buf)
}

/// Lädt das Gesetz `slug` herunter und importiert es in die Datenbank unter
/// `db_path`. Ersetzt eine vorhandene Fassung und verankert Annotationen neu.
pub async fn install_law(
    db_path: PathBuf,
    slug: &str,
    mut progress: impl FnMut(Progress),
) -> Result<ImportReport, ImportError> {
    let source = Source::by_slug(slug).ok_or_else(|| ImportError::UnknownSource(slug.into()))?;
    let bytes = download(source.url, |received, total| {
        progress(Progress::Downloading { received, total })
    })
    .await?;
    progress(Progress::Importing);
    let slug = slug.to_owned();
    run_blocking(move || {
        let xml = extract_xml(&bytes)?;
        import_xml(&db_path, &slug, &xml)
    })
    .await
}

/// Ergebnis einer Aktualisierungsprüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    /// Das Gesetz ist noch nicht in der Datenbank.
    NotInstalled,
    /// Die installierte Fassung entspricht der auf dem Server.
    UpToDate(LawInfo),
    /// Auf dem Server liegt eine andere Fassung.
    Available {
        installed: LawInfo,
        remote: Box<ParsedLawMeta>,
    },
}

/// Prüft, ob auf dem Server eine neuere Fassung des Gesetzes `slug` liegt.
///
/// Lädt das Archiv herunter und vergleicht Stand-Vermerk und Build-Datum
/// der Metadaten mit der Datenbank (siehe [`version_differs`]).
pub async fn check_update(
    db_path: PathBuf,
    slug: &str,
    mut progress: impl FnMut(Progress),
) -> Result<UpdateCheck, ImportError> {
    let source = Source::by_slug(slug).ok_or_else(|| ImportError::UnknownSource(slug.into()))?;
    let slug_owned = slug.to_owned();
    let path = db_path.clone();
    let installed = run_blocking(move || -> Result<Option<LawInfo>, ImportError> {
        Ok(Database::open(&path)?.law_by_slug(&slug_owned)?)
    })
    .await?;
    let Some(installed) = installed else {
        return Ok(UpdateCheck::NotInstalled);
    };
    let bytes = download(source.url, |received, total| {
        progress(Progress::Downloading { received, total })
    })
    .await?;
    progress(Progress::Importing);
    let remote = run_blocking(move || -> Result<ParsedLawMeta, ImportError> {
        let xml = extract_xml(&bytes)?;
        Ok(xml::parse_meta(&xml)?)
    })
    .await?;
    if version_differs(&installed, &remote) {
        Ok(UpdateCheck::Available {
            installed,
            remote: Box::new(remote),
        })
    } else {
        Ok(UpdateCheck::UpToDate(installed))
    }
}

/// Liest die installierte Fassung eines Gesetzes (blockierend, aber schnell).
pub fn installed_law(db_path: &Path, slug: &str) -> Result<Option<LawInfo>, ImportError> {
    Ok(Database::open(db_path)?.law_by_slug(slug)?)
}

/// Ist die Fassung auf dem Server eine andere als die installierte?
///
/// Maßgeblich ist der Stand-Vermerk („zuletzt geändert durch …“); fehlt er
/// oder ist er gleich, entscheidet das Build-Datum des Dokuments.
pub fn version_differs(installed: &LawInfo, remote: &ParsedLawMeta) -> bool {
    match (&installed.stand, &remote.stand) {
        (Some(a), Some(b)) if a != b => true,
        _ => !remote.builddate.is_empty() && installed.builddate != remote.builddate,
    }
}

/// Führt eine blockierende Funktion in einem Hintergrundthread aus.
async fn run_blocking<T, F>(func: F) -> Result<T, ImportError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ImportError> + Send + 'static,
{
    gio::spawn_blocking(func)
        .await
        .map_err(|_| ImportError::Cancelled)?
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

    #[test]
    fn source_lookup() {
        let bgb = Source::by_slug("bgb").unwrap();
        assert_eq!(bgb.name, "Bürgerliches Gesetzbuch");
        assert!(bgb.url.ends_with("/bgb/xml.zip"));
        assert!(Source::by_slug("hgb").is_none());
        assert!(Source::by_slug(DEFAULT_SLUG).is_some());
    }

    fn installed(stand: Option<&str>, builddate: &str) -> LawInfo {
        LawInfo {
            stand: stand.map(str::to_owned),
            builddate: builddate.to_owned(),
            ..LawInfo::default()
        }
    }

    fn remote(stand: Option<&str>, builddate: &str) -> ParsedLawMeta {
        ParsedLawMeta {
            stand: stand.map(str::to_owned),
            builddate: builddate.to_owned(),
            ..ParsedLawMeta::default()
        }
    }

    #[test]
    fn version_comparison_uses_stand_then_builddate() {
        let alt = "zuletzt geändert durch Art. 1 G v. 1.1.2026 I Nr. 1";
        let neu = "zuletzt geändert durch Art. 6 G v. 23.7.2026 I Nr. 226";
        // Gleicher Stand, gleiches Build-Datum: aktuell.
        assert!(!version_differs(
            &installed(Some(alt), "20260101000000"),
            &remote(Some(alt), "20260101000000")
        ));
        // Anderer Stand: neue Fassung, auch bei gleichem Build-Datum.
        assert!(version_differs(
            &installed(Some(alt), "20260101000000"),
            &remote(Some(neu), "20260101000000")
        ));
        // Gleicher Stand, aber neu erzeugtes Dokument: als geändert werten.
        assert!(version_differs(
            &installed(Some(alt), "20260101000000"),
            &remote(Some(alt), "20260910215507")
        ));
        // Ohne Stand-Vermerk entscheidet allein das Build-Datum.
        assert!(!version_differs(
            &installed(None, "20260101000000"),
            &remote(None, "20260101000000")
        ));
        assert!(version_differs(
            &installed(None, "20260101000000"),
            &remote(Some(neu), "20260910215507")
        ));
        // Fehlt das Build-Datum auf dem Server, gilt die Fassung als aktuell.
        assert!(!version_differs(
            &installed(Some(alt), "20260101000000"),
            &remote(None, "")
        ));
    }

    #[test]
    fn meta_from_zip_round_trip() {
        let xml = concat!(
            "<dokumente builddate=\"20260910215507\" doknr=\"BJNR001950896\">",
            "<norm builddate=\"20260910215507\" doknr=\"BJNR001950896\"><metadaten>",
            "<jurabk>BGB</jurabk><langue>Bürgerliches Gesetzbuch</langue>",
            "<standangabe><standtyp>Stand</standtyp>",
            "<standkommentar>zuletzt geändert durch Art. 6 G v. 23.7.2026 I Nr. 226</standkommentar>",
            "</standangabe></metadaten></norm></dokumente>"
        );
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("BJNR001950896.xml", opts).unwrap();
            w.write_all(xml.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        let extracted = extract_xml(buf.get_ref()).unwrap();
        let meta = xml::parse_meta(&extracted).unwrap();
        assert_eq!(meta.builddate, "20260910215507");
        assert_eq!(meta.title, "Bürgerliches Gesetzbuch");
        assert_eq!(
            meta.stand.as_deref(),
            Some("zuletzt geändert durch Art. 6 G v. 23.7.2026 I Nr. 226")
        );
        let law = installed(meta.stand.as_deref(), &meta.builddate);
        assert!(!version_differs(&law, &meta));
    }
}
