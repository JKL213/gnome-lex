#!/usr/bin/env python3
"""Erzeugt po/gesetze.pot aus allen Quellen.

xgettext übernimmt Desktop-Datei, Metainfo (ITS) und Blueprint-Dateien;
die Rust-Quellen liest dieses Skript selbst, weil der Rust-Modus von
xgettext Aufrufe wie `&gettext("…")` und mehrzeilige Literale übergeht.
Erkannt werden gettext("…") und ngettext("…", "…", n), auch über
Zeilenumbrüche hinweg. Danach: `msgmerge -U po/<lang>.po po/gesetze.pot`.
"""
import glob
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)

STRING = r'"((?:[^"\\]|\\.)*)"'
CALL = re.compile(r'\b(ngettext|gettext)\s*\(\s*' + STRING + r'(?:\s*,\s*' + STRING + r')?', re.S)


def unescape(lit: str) -> str:
    return lit.encode("utf-8").decode("unicode_escape").encode("latin-1").decode("utf-8")


def escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def rust_pot(path: str) -> str:
    entries = {}
    for src in sorted(glob.glob("src/**/*.rs", recursive=True)):
        text = open(src, encoding="utf-8").read()
        for m in CALL.finditer(text):
            kind, a, b = m.group(1), unescape(m.group(2)), m.group(3)
            line = text.count("\n", 0, m.start()) + 1
            key = (a, unescape(b) if (kind == "ngettext" and b) else None)
            entries.setdefault(key, []).append(f"{src}:{line}")
    out = ['msgid ""', 'msgstr ""', '"Content-Type: text/plain; charset=UTF-8\\n"', ""]
    for (msgid, plural), refs in entries.items():
        out.append("#: " + " ".join(refs))
        out.append(f'msgid "{escape(msgid)}"')
        if plural is not None:
            out.append(f'msgid_plural "{escape(plural)}"')
            out.append('msgstr[0] ""')
            out.append('msgstr[1] ""')
        else:
            out.append('msgstr ""')
        out.append("")
    open(path, "w", encoding="utf-8").write("\n".join(out))
    return path


def run(args):
    subprocess.run(args, check=True)


with tempfile.TemporaryDirectory() as tmp:
    parts = []
    p = os.path.join(tmp, "desktop.pot")
    run(["xgettext", "--from-code=UTF-8", "--language=Desktop", "-k", "-kName", "-kComment",
         "-kKeywords", "-o", p, "data/org.gnomelex.Gesetze.desktop.in"])
    parts.append(p)
    p = os.path.join(tmp, "metainfo.pot")
    run(["xgettext", "--from-code=UTF-8", "--its=/usr/share/gettext/its/metainfo.its", "-o", p,
         "data/org.gnomelex.Gesetze.metainfo.xml.in"])
    parts.append(p)
    p = os.path.join(tmp, "blp.pot")
    run(["xgettext", "--from-code=UTF-8", "--language=C", "--keyword=_", "--keyword=C_:1c,2",
         "-o", p] + sorted(glob.glob("data/ui/*.blp")))
    parts.append(p)
    parts.append(rust_pot(os.path.join(tmp, "rs.pot")))
    run(["msgcat", "--use-first", "-o", "po/gesetze.pot"] + parts)

pot = open("po/gesetze.pot", encoding="utf-8").read()
pot = pot.replace("Project-Id-Version: PACKAGE VERSION", "Project-Id-Version: gesetze")
pot = pot.replace("charset=CHARSET", "charset=UTF-8")
open("po/gesetze.pot", "w", encoding="utf-8").write(pot)
print("po/gesetze.pot:", pot.count("\nmsgid "), "Einträge")
