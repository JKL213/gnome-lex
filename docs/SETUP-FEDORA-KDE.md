# Entwicklungsumgebung unter Fedora KDE

Die App bleibt eine GNOME-Anwendung (GTK 4 + libadwaita). Unter KDE Plasma
läuft sie ohne Anpassungen; nötig sind nur die GNOME-Bibliotheken und
-Werkzeuge sowie die Flatpak-Laufzeiten.

## 1. Systempakete

```sh
sudo dnf install rust cargo clippy rustfmt \
  gtk4-devel libadwaita-devel libsoup3-devel glib2-devel gettext-devel \
  meson ninja-build blueprint-compiler desktop-file-utils appstream \
  flatpak flatpak-builder gdb git ccache \
  adwaita-icon-theme adwaita-fonts-all xdg-desktop-portal-gtk
```

Hinweise:

- Rust muss mindestens Version 1.92 sein (`rust-version` in `Cargo.toml`).
  Ist die Fedora-Version älter, `rustup` verwenden.
- Mit installiertem `libsoup3-devel` ist der pkg-config-Shim aus
  `AGENTS.md` überflüssig. Die Einträge in `.vscode/settings.json` und
  `.vscode/tasks.json` (`PKG_CONFIG_PATH=$HOME/.cache/gnome-lex/pc`)
  stören nicht, weil das Verzeichnis dann einfach leer bzw. nicht vorhanden ist.
- `xdg-desktop-portal-gtk` neben `xdg-desktop-portal-kde` sorgt dafür,
  dass GTK-Dateidialoge und das Farbschema (hell/dunkel) unter Plasma
  korrekt funktionieren. libadwaita übernimmt das Plasma-Farbschema über
  das Settings-Portal.

## 2. Flatpak-Laufzeiten

Systemweit installieren (so ist es auf dem bisherigen Entwicklungsrechner
eingerichtet; `flatpak-builder --user` nutzt systemweite Laufzeiten):

```sh
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//51 org.gnome.Sdk//51 \
  org.freedesktop.Sdk.Extension.rust-stable//26.08
```

`flatpak-builder --install-deps-from=flathub` zusammen mit `--user`
schlägt fehl, wenn Flathub nur systemweit eingerichtet ist. Einfach ohne
diese Option bauen.

## 3. VS Code

Erweiterungen aus `.vscode/extensions.json` installieren:

```sh
code --install-extension rust-lang.rust-analyzer
code --install-extension ms-vscode.cpptools
code --install-extension github.copilot
code --install-extension github.copilot-chat
code --install-extension bodil.blueprint-gtk
code --install-extension mesonbuild.mesonbuild
```

Für „Run and Debug“ (Flatpak-Variante mit gdb in der Sandbox) sind
`rust-analyzer` und `cpptools` Pflicht; die Konfigurationen liegen in
`.vscode/launch.json`, das Sandbox-Skript ist `build-aux/flatpak-run.sh`.

## 4. Erster Build

```sh
cargo build && cargo test
cargo clippy --all-targets -- -D warnings && cargo fmt --check
meson setup _build -Dprofile=development && meson compile -C _build && meson test -C _build
flatpak-builder --user --force-clean --ccache _flatpak build-aux/org.gnomelex.Gesetze.Devel.json
```

Danach in VS Code „Flatpak: Gesetze debuggen (ohne Neubau)“ starten.

## 5. Beim Übernehmen des Repos beachten

- Die lokalen Copilot-Prompts unter `.github/prompts/` sind per
  `.gitignore` ausgeschlossen und müssen bei Bedarf separat kopiert werden.
- `CLAUDE.md` im Wurzelverzeichnis enthält die Übergabe für Claude Code
  (Arbeitsregeln, Stand, nächste Stufe); `AGENTS.md` die verbindlichen
  Regeln für alle Agenten.
- Unter KDE gibt es keinen GNOME-Shell-Symbolic-Icon-Cache; nach
  `meson install` ggf. `gtk4-update-icon-cache` laufen lassen (macht
  `gnome.post_install` bereits).
