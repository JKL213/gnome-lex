<img width="1280" height="640" alt="github-social-preview_2" src="https://github.com/user-attachments/assets/2f20a0e1-e228-49f1-b5fb-7bd3f7013f49" />


# gnome-lex

gnome-lex ist eine mit dem GTK-Toolkit für GNOME-Desktopumgebungen zusammengestellte App zum Lesen deutscher Gesetze. 
Sie befindet sich in reger Entwicklung und ist seit August 2026 in einer äußerst frühen ausführbaren Version. (Für die drei deutschen Juristen die tatsächlich mal mit GNOME arbeiten).

Als Besonderheit fügt sich gnome-lex nahtlos in bestehende GNOME-Desktops ein und erreicht somit einen schönen Look & Feel ohne nervigen Electron-/Browser-Overhead. Die Gesetzestexte werden in XML-Form von gesetze-im-internet.de bezogen.

gnome-lex entstand aus meinem Bedarf heraus, eine GNOME-native App für Gesetzestexte im Repetitorium zu haben, und ist stark auf meine eigenen Bedürfnisse adaptiert. PRs, falls jemals welche kommen sollten, nehme ich aber natürlich immer gerne an. 

gnome-lex ist [freie](https://www.gnu.org/philosophy/free-sw) Software, und folgt den Prinzipien des [Debian-Projekts](https://www.debian.org/intro/philosophy) für freie Software. Es ist juristisch unter der GNU LGPL v3 lizensiert und kann von jedermann bearbeitet und verändert werden. 



## Bauen
WARNUNG! gnome-lex wurde von AI-Agents mitentwickelt. Den größten Teil des Adwaita/GTK-Displaystacks habe ich jedoch selbst implementiert. Da sich die Meinungen bei AI-Code spalten, sei es jedem selbst überlassen, wie er dieses Projekt verwenden möchte. Es werden jedoch keine ungetesteten Builds publiziert. Jede Zeile Code wird von mir entweder selbst geschrieben oder selbst geprüft. 

Im Umkehrschluss ist es natürlich erlaubt, eigene PRs mit KI zu generieren oder zu beschreiben, das ist mir im Rahmen dieses Projektes egal. 



Nur Cargo (Entwicklung):

```sh
cargo build
cargo run
```

Mit Meson (Installation, Übersetzungen, Tests):

```sh
meson setup _build -Dprofile=development
meson compile -C _build
meson test -C _build
meson install -C _build
```

Flatpak:

```sh
flatpak-builder --user --install --force-clean _flatpak build-aux/org.gnomelex.Gesetze.Devel.json
flatpak run org.gnomelex.Gesetze.Devel
```

Voraussetzungen: Rust stable, GTK 4.22, libadwaita 1.9, libsoup 3.6,
blueprint-compiler 0.20, Meson 1.0. Windows-Build: siehe
[docs/WINDOWS.md](docs/WINDOWS.md).
   |
