---
mode: agent
description: "Importer gegen weitere gii-norm-Konstrukte testen"
---
Ergänze `src/importer/xml.rs` um Tests für Konstrukte der DTD `gii-norm`,
die im BGB selten oder gar nicht vorkommen, damit später weitere Gesetze
importiert werden können:

- `<table>` mit `<thead>`, `morerows`, `namest`/`nameend`
- `<DL Type="alpha">` mit verschachtelten `<DL>` in `<LA>`
- `<Revision>` innerhalb von `<Content>` und `<DD>`
- `<Citation>`, `<SP>`, `<small>`, `<SUP>`, `<SUB>`, `<U>`
- `<kommentar>` im Text und in `<fussnoten>`
- Normen ohne `<textdaten>` und Gliederungseinheiten mit eigenem Text
- Entity-Referenzen (`&amp;`, `&#8203;`) und CDATA

Jeder Test verwendet ein kleines Inline-XML wie `SAMPLE`. Korrigiere den
Parser nur, wenn ein Test einen echten Fehler zeigt. Qualitätsläufe:
`cargo test importer`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.
