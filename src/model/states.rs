// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Die 16 Länder: Kürzel, Name, stilisiertes Wappen-Icon und das amtliche
//! Landesrechtsportal. Gesetze des Bundes und der Länder werden über
//! [`Jurisdiction`] unterschieden.

/// Bundesland.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    BadenWuerttemberg,
    Bayern,
    Berlin,
    Brandenburg,
    Bremen,
    Hamburg,
    Hessen,
    MecklenburgVorpommern,
    Niedersachsen,
    NordrheinWestfalen,
    RheinlandPfalz,
    Saarland,
    Sachsen,
    SachsenAnhalt,
    SchleswigHolstein,
    Thueringen,
}

/// Gesetzgeber: Bund oder ein Land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Jurisdiction {
    Bund,
    Land(State),
}

impl Jurisdiction {
    /// Name des Icons (stilisiertes Wappen) in der GResource.
    pub fn icon_name(&self) -> String {
        match self {
            Jurisdiction::Bund => "lex-wappen-bund".to_owned(),
            Jurisdiction::Land(state) => state.icon_name(),
        }
    }
}

impl State {
    /// Alle Länder in alphabetischer Reihenfolge.
    pub const ALL: [State; 16] = [
        State::BadenWuerttemberg,
        State::Bayern,
        State::Berlin,
        State::Brandenburg,
        State::Bremen,
        State::Hamburg,
        State::Hessen,
        State::MecklenburgVorpommern,
        State::Niedersachsen,
        State::NordrheinWestfalen,
        State::RheinlandPfalz,
        State::Saarland,
        State::Sachsen,
        State::SachsenAnhalt,
        State::SchleswigHolstein,
        State::Thueringen,
    ];

    /// Amtliches Länderkürzel (ISO 3166-2:DE ohne Präfix).
    pub fn code(&self) -> &'static str {
        match self {
            State::BadenWuerttemberg => "BW",
            State::Bayern => "BY",
            State::Berlin => "BE",
            State::Brandenburg => "BB",
            State::Bremen => "HB",
            State::Hamburg => "HH",
            State::Hessen => "HE",
            State::MecklenburgVorpommern => "MV",
            State::Niedersachsen => "NI",
            State::NordrheinWestfalen => "NW",
            State::RheinlandPfalz => "RP",
            State::Saarland => "SL",
            State::Sachsen => "SN",
            State::SachsenAnhalt => "ST",
            State::SchleswigHolstein => "SH",
            State::Thueringen => "TH",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            State::BadenWuerttemberg => "Baden-Württemberg",
            State::Bayern => "Bayern",
            State::Berlin => "Berlin",
            State::Brandenburg => "Brandenburg",
            State::Bremen => "Bremen",
            State::Hamburg => "Hamburg",
            State::Hessen => "Hessen",
            State::MecklenburgVorpommern => "Mecklenburg-Vorpommern",
            State::Niedersachsen => "Niedersachsen",
            State::NordrheinWestfalen => "Nordrhein-Westfalen",
            State::RheinlandPfalz => "Rheinland-Pfalz",
            State::Saarland => "Saarland",
            State::Sachsen => "Sachsen",
            State::SachsenAnhalt => "Sachsen-Anhalt",
            State::SchleswigHolstein => "Schleswig-Holstein",
            State::Thueringen => "Thüringen",
        }
    }

    /// Icon-Name des stilisierten Landeswappens (`lex-wappen-<kürzel>`).
    pub fn icon_name(&self) -> String {
        format!("lex-wappen-{}", self.code().to_ascii_lowercase())
    }

    /// Name des amtlichen Landesrechtsportals.
    pub fn portal_name(&self) -> &'static str {
        match self {
            State::BadenWuerttemberg => "Landesrecht BW",
            State::Bayern => "BAYERN.RECHT",
            State::Berlin => "Berliner Vorschriften- und Rechtsprechungsdatenbank",
            State::Brandenburg => "BRAVORS",
            State::Bremen => "Transparenzportal Bremen",
            State::Hamburg => "Landesrecht Hamburg",
            State::Hessen => "Hessenrecht",
            State::MecklenburgVorpommern => "Landesrecht Mecklenburg-Vorpommern",
            State::Niedersachsen => "NI-VORIS",
            State::NordrheinWestfalen => "recht.nrw.de",
            State::RheinlandPfalz => "Landesrecht Rheinland-Pfalz",
            State::Saarland => "Recht Saarland",
            State::Sachsen => "REVOSax",
            State::SachsenAnhalt => "Landesrecht Sachsen-Anhalt",
            State::SchleswigHolstein => "Landesvorschriften Schleswig-Holstein",
            State::Thueringen => "Landesrecht Thüringen",
        }
    }

    /// Startseite des amtlichen Landesrechtsportals.
    pub fn portal_url(&self) -> &'static str {
        match self {
            State::BadenWuerttemberg => "https://www.landesrecht-bw.de/",
            State::Bayern => "https://www.gesetze-bayern.de/",
            State::Berlin => "https://gesetze.berlin.de/",
            State::Brandenburg => "https://bravors.brandenburg.de/",
            State::Bremen => "https://www.transparenz.bremen.de/",
            State::Hamburg => "https://www.landesrecht-hamburg.de/",
            State::Hessen => "https://www.rv.hessenrecht.hessen.de/",
            State::MecklenburgVorpommern => "https://www.landesrecht-mv.de/",
            State::Niedersachsen => "https://voris.wolterskluwer-online.de/",
            State::NordrheinWestfalen => "https://recht.nrw.de/",
            State::RheinlandPfalz => "https://landesrecht.rlp.de/",
            State::Saarland => "https://recht.saarland.de/",
            State::Sachsen => "https://www.revosax.sachsen.de/",
            State::SachsenAnhalt => "https://www.landesrecht.sachsen-anhalt.de/",
            State::SchleswigHolstein => "https://www.gesetze-rechtsprechung.sh.juris.de/",
            State::Thueringen => "https://landesrecht.thueringen.de/",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_icons_follow_them() {
        let mut codes: Vec<&str> = State::ALL.iter().map(State::code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 16);
        assert_eq!(State::Bayern.icon_name(), "lex-wappen-by");
        assert_eq!(Jurisdiction::Bund.icon_name(), "lex-wappen-bund");
        assert!(State::ALL
            .iter()
            .all(|s| s.portal_url().starts_with("https://")));
    }
}
