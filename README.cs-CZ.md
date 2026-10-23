# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

Čeština · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Vítejte u Zed — vysoce výkonného editoru kódu se společnou prací několika uživatelů od tvůrců [Atomu](https://github.com/atom/atom) a [Tree-sitteru](https://github.com/tree-sitter/tree-sitter).

---

## O tomto forku

Toto je veřejný fork [zed-industries/zed](https://github.com/zed-industries/zed). Změny funkcí potřebné pro lokalizované sestavení Zedu ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) jsou udržovány na větvi `l10n`.

- **Změny v tomto forku** (podrobnosti a postup sledování upstreamu jsou v [FORK-NOTES.md](FORK-NOTES.md), v japonštině)
  - Nastavení pollingu sledování souborů — přidává `file_watcher_use_polling` / `file_watcher_poll_ms`; polling je volen automaticky pouze pro síťové podíly, FUSE a 9P/WSL drvfs
  - Fallback obnovování relace — když cíl obnovy chybí, vrátí se k naposledy otevřenému workspace; řeší případ, kdy souběžné instance soutěží o jedinou session id a taby se neobnoví
  - Taby přečkají odpojení sdíleného svazku — když je SMB nebo jiný svazek dočasně nedostupný, worktree zůstává otevřená a text zachován; zobrazuje se přeškrtnutí s červeným ✕, dokud sdílení není zpět
- **Snapshot tagy** — stabilní tag upstreamu plus změny forku jako jeden commit jsou označeny tagem `v<version>-l10n.<N>`; `config/project.toml` v zed-i18n (`zed_ref` / `zed_commit`) vždy fixuje nejnovější snapshot
- **Sestavení** — upstream CI předpokládá organizaci zed-industries, proto jsou všechny release joby v tomto repozitáři na forku přeskočeny (záměr). Lokalizuovaná sestavení a jejich distribuce na GitHub probíhají v NeoSloth/zed-i18n; výsledek se instaluje jako nepodepsaná aplikace s vlastní značkou Zed i18n, odděleně od oficiálního Zedu

---

### Instalace

Na macOS, Linuxu a Windows si můžete [stáhnout Zed přímo](https://zed.dev/download) nebo nainstalovat Zed přes lokálního správce balíčků ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Ostatní platformy zatím nejsou k dispozici:

- Web ([sledovaná diskuze](https://github.com/zed-industries/zed/discussions/26195))

### Vývoj Zedu

- [Sestavení Zedu pro macOS](./docs/src/development/macos.md)
- [Sestavení Zedu pro Linux](./docs/src/development/linux.md)
- [Sestavení Zedu pro Windows](./docs/src/development/windows.md)

### Přispívání

Možnosti, jak přispívat do Zedu, najdete v [CONTRIBUTING.md](./CONTRIBUTING.md).

Společnost Zed Industries hledá posily! Otevřené pozice najdete na jejich stránce [jobs](https://zed.dev/jobs).

### Licencování

Zdrojový kód Zedu je licencován primárně pod GPL-3.0-or-later, s komponentami Apache-2.0 tam, kde je to označeno.

Informace o licencích závislostí třetích stran musí být správně zadány, aby prošel CI.

K automatickému plnění podmínek open source licencí používáme [`cargo-about`](https://github.com/EmbarkStudios/cargo-about). Pokud CI selhává, zkontrolujte následující:

- Zobrazuje se chyba `no license specified` pro crate, který jste vytvořili? Pokud ano, přidejte `publish = false` pod `[package]` v Cargo.toml vašeho crate.
- Jde o chybu `failed to satisfy license requirements` u závislosti? Pokud ano, nejprve zjistěte, jakou licenci projekt má a zda tento systém postačuje k splnění požadavků dané licence. Pokud si nejste jisti, zeptejte se právníka. Až ověříte, že tento systém je akceptovatelný, přidejte SPDX identifikátor licence do pole `accepted` v `script/licenses/zed-licenses.toml`.
- Nemůže `cargo-about` u závislosti najít licenci? Pokud ano, přidejte na konec `script/licenses/zed-licenses.toml` pole clarification, jak je uvedeno v [knize cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponzorování

Zed vyvíjí společnost **Zed Industries, Inc.**, zisková firma.

Chcete-li projekt finančně podpořit, můžete tak učinit přes GitHub Sponsors. Sponzorské dary směřují přímo do Zed Industries a jsou použity jako obecný příjem firmy. Se sponzorováním nejsou spojeny žádné výhody ani nároky.
