# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · Italiano · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Benvenuto in Zed, un editor di codice multiutente ad alte prestazioni creato dagli autori di [Atom](https://github.com/atom/atom) e [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Informazioni su questo fork

Questo è un fork pubblico di [zed-industries/zed](https://github.com/zed-industries/zed). Le modifiche funzionali richieste dalla build localizzata di Zed ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) sono mantenute sul ramo `l10n`.

- **Modifiche incluse** (i dettagli e la procedura di allineamento sono in [FORK-NOTES.md](FORK-NOTES.md), in giapponese)
  - Impostazioni del polling dell'osservatore di file — aggiunge `file_watcher_use_polling` / `file_watcher_poll_ms`; il polling è scelto automaticamente solo per condivisioni di rete, FUSE e 9P/WSL drvfs
  - Fallback per il ripristino della sessione — quando la destinazione da ripristinare non c'è, ripiega sull'ultimo workspace aperto; affronta il problema delle schede non ripristinate quando più istanze si contendono un'unica id di sessione
  - Le schede sopravvivono alla disconnessione della condivisione — quando SMB o un altro volume è temporaneamente irraggiungibile, il worktree resta aperto con il testo integro, mostrato barrato con una ✕ rossa finché la condivisione non torna
- **Tag di snapshot** — un tag stabile di upstream più le modifiche del fork come un singolo commit vengono etichettati come `v<versione>-l10n.<N>`; il `config/project.toml` di zed-i18n (`zed_ref` / `zed_commit`) fissa sempre lo snapshot più recente
- **Build** — la CI di upstream presuppone l'organizzazione zed-industries, quindi tutti i job di release di questo repository vengono saltati nei fork (per progettazione). Le build localizzate e la loro distribuzione su GitHub avvengono in NeoSloth/zed-i18n; il risultato si installa come app non firmata con lo stesso nome di Zed ufficiale

---

### Installazione

Su macOS, Linux e Windows puoi [scaricare Zed direttamente](https://zed.dev/download) oppure installare Zed tramite il gestore di pacchetti locale ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Altre piattaforme non sono ancora disponibili:

- Web ([discussione di tracciamento](https://github.com/zed-industries/zed/discussions/26195))

### Sviluppare Zed

- [Compilare Zed per macOS](./docs/src/development/macos.md)
- [Compilare Zed per Linux](./docs/src/development/linux.md)
- [Compilare Zed per Windows](./docs/src/development/windows.md)

### Contribuire

Vedi [CONTRIBUTING.md](./CONTRIBUTING.md) per i modi in cui puoi contribuire a Zed.

E poi... Zed Industries sta assumendo! Consulta la loro pagina [lavoro](https://zed.dev/jobs) per le posizioni aperte.

### Licenze

Il codice sorgente di Zed è concesso in licenza principalmente con GPL-3.0-or-later, con componenti Apache-2.0 dove indicato.

Le informazioni di licenza delle dipendenze di terze parti devono essere fornite correttamente affinché la CI passi.

Usiamo [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) per rispettare automaticamente le licenze open source. Se la CI fallisce, controlla quanto segue:

- Vedi un errore `no license specified` per una crate che hai creato? In tal caso, aggiungi `publish = false` sotto `[package]` nel Cargo.toml della tua crate.
- L'errore `failed to satisfy license requirements` riguarda una dipendenza? In tal caso, determina prima quale licenza ha il progetto e se questo sistema è sufficiente a rispettare i requisiti di tale licenza. In caso di dubbio, chiedi a un avvocato. Una volta verificato che questo sistema è accettabile, aggiungi l'identificatore SPDX della licenza all'array `accepted` in `script/licenses/zed-licenses.toml`.
- `cargo-about` non riesce a trovare la licenza di una dipendenza? In tal caso, aggiungi un campo clarification alla fine di `script/licenses/zed-licenses.toml`, come specificato nel [libro di cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sostegno

Zed è sviluppato da **Zed Industries, Inc.**, un'azienda a scopo di lucro.

Se desideri sostenere economicamente il progetto, puoi farlo tramite GitHub Sponsors. Le sponsorizzazioni vanno direttamente a Zed Industries e sono usate come entrate generali dell'azienda. Non ci sono vantaggi o diritti associati alla sponsorizzazione.
