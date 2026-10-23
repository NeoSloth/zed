# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · Deutsch · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Willkommen bei Zed — einem leistungsstarken Code-Editor mit gemeinsamer Arbeit mehrerer Nutzer, geschaffen von den Machern von [Atom](https://github.com/atom/atom) und [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Über diesen Fork

Dies ist ein öffentlicher Fork von [zed-industries/zed](https://github.com/zed-industries/zed). Funktionsänderungen, die der lokalisierte Zed-Build ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) benötigt, werden auf dem Branch `l10n` geführt.

- **Getragene Änderungen** (Details und das Verfahren zum Nachziehen neuer Upstream-Releases stehen in [FORK-NOTES.md](FORK-NOTES.md), auf Japanisch)
  - Polling-Einstellungen für die Dateiüberwachung — ergänzt `file_watcher_use_polling` / `file_watcher_poll_ms`; Polling wird automatisch nur für Netzwerkfreigaben, FUSE und 9P/WSL drvfs gewählt
  - Fallback bei der Sitzungswiederherstellung — wenn das Wiederherstellungsziel fehlt, wird auf den zuletzt geöffneten Workspace ausgewichen; adressiert das Problem, dass Tabs nicht wiederhergestellt werden, wenn mehrere Instanzen um eine Session-ID konkurrieren
  - Tabs überstehen Trennungen der Freigabe — wenn SMB oder ein anderes Volume vorübergehend nicht erreichbar ist, bleibt die Worktree offen und der Text erhalten, dargestellt mit Durchstreichung und rotem ✕, bis die Freigabe wieder erreichbar ist
- **Snapshot-Tags** — ein stabiler Upstream-Tag plus die Fork-Änderungen als ein einzelner Commit werden als `v<version>-l10n.<N>` markiert; die `config/project.toml` von zed-i18n (`zed_ref` / `zed_commit`) pinnt stets den neuesten Snapshot
- **Builds** — die Upstream-CI setzt die Organisation zed-industries voraus, daher werden alle Release-Jobs in diesem Repository auf Forks übersprungen (by design). Lokalisierte Builds und deren GitHub-Distribution laufen in NeoSloth/zed-i18n; das Ergebnis installiert sich als unsignierte App mit eigener Marke „Zed i18n“, getrennt vom offiziellen Zed

---

### Installation

Auf macOS, Linux und Windows kannst du Zed [direkt herunterladen](https://zed.dev/download) oder über deinen lokalen Paketmanager installieren ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Andere Plattformen sind noch nicht verfügbar:

- Web ([laufende Diskussion](https://github.com/zed-industries/zed/discussions/26195))

### Zed entwickeln

- [Zed für macOS bauen](./docs/src/development/macos.md)
- [Zed für Linux bauen](./docs/src/development/linux.md)
- [Zed für Windows bauen](./docs/src/development/windows.md)

### Mitwirken

Möglichkeiten, zu Zed beizutragen, findest du in [CONTRIBUTING.md](./CONTRIBUTING.md).

Zed Industries stellt ein! Offene Stellen gibt es auf der [Jobs-Seite](https://zed.dev/jobs) von Zed Industries.

### Lizenzierung

Der Zed-Quellcode wird überwiegend unter GPL-3.0-or-later lizenziert, mit Apache-2.0-Komponenten dort, wo entsprechend gekennzeichnet.

Lizenzinformationen für Drittanbieter-Abhängigkeiten müssen korrekt angegeben sein, damit die CI besteht.

Wir verwenden [`cargo-about`](https://github.com/EmbarkStudios/cargo-about), um Open-Source-Lizenzen automatisch einzuhalten. Wenn die CI fehlschlägt, prüfe Folgendes:

- Erscheint ein Fehler `no license specified` für eine von dir erstellte Crate? Falls ja, füge `publish = false` unter `[package]` in der Cargo.toml deiner Crate hinzu.
- Handelt es sich bei dem Fehler `failed to satisfy license requirements` um eine Abhängigkeit? Falls ja, bestimme zunächst, welche Lizenz das Projekt hat und ob dieses System ausreicht, um die Anforderungen dieser Lizenz einzuhalten. Im Zweifel frage eine Anwältin oder einen Anwalt. Sobald du bestätigt hast, dass dieses System akzeptabel ist, füge die SPDX-Kennung der Lizenz dem Array `accepted` in `script/licenses/zed-licenses.toml` hinzu.
- Kann `cargo-about` die Lizenz einer Abhängigkeit nicht finden? Falls ja, füge am Ende von `script/licenses/zed-licenses.toml` ein Feld `clarification` hinzu, wie im [cargo-about-Buch](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration) beschrieben.

## Sponsoring

Zed wird von **Zed Industries, Inc.** entwickelt, einem gewinnorientierten Unternehmen.

Wenn du das Projekt finanziell unterstützen möchtest, kannst du das über GitHub Sponsors tun. Sponsorings gehen direkt an Zed Industries und werden als allgemeines Unternehmenseinkommen verwendet. Mit dem Sponsoring sind keine Vorteile oder Ansprüche verbunden.
