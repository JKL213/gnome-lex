---
mode: agent
description: "Übersetzungen in po/de.po vervollständigen"
---
Aktualisiere die Übersetzungen:

1. Erzeuge die POT-Datei neu: `xgettext` über alle Dateien aus `po/POTFILES`
   (Rust mit `--language=Rust -k_ -kgettext`, Blueprint mit `--language=C -k_`,
   Desktop-/Metainfo-/Schema-Dateien ohne Sprachangabe, jeweils mit `-j` anhängen).
2. Führe `msgmerge --update po/de.po po/gesetze.pot` aus.
3. Fülle alle leeren `msgstr` in `po/de.po` mit deutschen Übersetzungen.
   Die Quelltexte sind bereits deutsch; übernimm sie unverändert, korrigiere
   nur Tippfehler. Mnemonics (`_`) und Platzhalter (`{}`, `%s`) beibehalten.
4. Prüfe mit `msgfmt --check -o /dev/null po/de.po`.

Ändere keine Quelldateien außerhalb von `po/`.
