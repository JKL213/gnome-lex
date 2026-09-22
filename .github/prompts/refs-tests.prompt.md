---
mode: agent
description: "Weitere Testfälle für den Verweisparser (src/refs) ergänzen"
---
Ergänze `src/refs/mod.rs` um Unit-Tests für Zitierweisen, die noch nicht
abgedeckt sind, z. B.:

- „§ 1 Abs. 1 Satz 1 Halbsatz 2“
- „§§ 145 ff.“ und „§ 812 ff.“
- „§ 355 Absatz 2 Satz 2 Nummer 1 Buchstabe a“
- „nach den §§ 987 bis 993“
- „Artikel 3 Absatz 1 des Grundgesetzes“
- Verweise am Zeilenanfang und am Textende
- Text mit Umlauten vor dem Verweis (Zeichenoffsets prüfen)

Schlagen Tests fehl, korrigiere den Parser so minimal wie möglich und
begründe die Änderung im Doc-Kommentar. Alle bestehenden Tests müssen weiter
bestehen. `cargo test refs`, `cargo clippy --all-targets -- -D warnings`
und `cargo fmt --check` müssen grün sein.
