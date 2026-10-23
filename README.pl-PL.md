# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · Polski · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Witamy w Zed — wysokowydajnym edytorze kodu ze wspólną pracą wielu użytkowników, stworzonym przez autorów [Atomu](https://github.com/atom/atom) i [Tree-sittera](https://github.com/tree-sitter/tree-sitter).

---

## O tym forku

To jest publiczny fork [zed-industries/zed](https://github.com/zed-industries/zed). Zmiany funkcji wymagane przez lokalizowany build Zeda ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) są utrzymywane na gałęzi `l10n`.

- **Naniesione zmiany** (szczegóły i procedura aktualizacji do nowych wydań upstreamu znajdują się w [FORK-NOTES.md](FORK-NOTES.md), w języku japońskim)
  - Ustawienia odpytywania obserwatora plików — dodaje `file_watcher_use_polling` / `file_watcher_poll_ms`; odpytywanie jest wybierane automatycznie tylko dla udziałów sieciowych, FUSE i 9P/WSL drvfs
  - Zastępowanie przy przywracaniu sesji — gdy cel przywrócenia nie istnieje, następuje zastąpienie ostatnio otwartym workspace; rozwiązuje problem nieprzywracanych kart, gdy wiele instancji rywalizuje o jedną id sesji
  - Karty przetrwają rozłączenie udziału — gdy SMB lub inny wolumin jest chwilowo niedostępny, worktree pozostaje otwarty z zachowaną treścią, wyświetlany przekreślony z czerwonym ✕ aż do powrotu udziału
- **Tagi snapshotów** — stabilny tag upstreamu plus zmiany forku jako jeden commit są oznaczane tagiem `v<wersja>-l10n.<N>`; `config/project.toml` zed-i18n (`zed_ref` / `zed_commit`) zawsze przypina najnowszy snapshot
- **Buildy** — CI upstreamu zakłada organizację zed-industries, dlatego wszystkie joby release w tym repozytorium są pomijane na forkach (z założenia). Lokalizowane buildy i ich dystrybucja na GitHubie odbywają się w NeoSloth/zed-i18n; wynik instaluje się jako niesygnowana aplikacja pod tą samą nazwą co oficjalny Zed

---

### Instalacja

Na macOS, Linuksie i Windowsie możesz [pobrać Zed bezpośrednio](https://zed.dev/download) lub zainstalować Zed przez lokalny menedżer pakietów ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Pozostałe platformy nie są jeszcze dostępne:

- Web ([śledzona dyskusja](https://github.com/zed-industries/zed/discussions/26195))

### Rozwój Zeda

- [Budowanie Zeda dla macOS](./docs/src/development/macos.md)
- [Budowanie Zeda dla Linuksa](./docs/src/development/linux.md)
- [Budowanie Zeda dla Windows](./docs/src/development/windows.md)

### Kontrybuowanie

Możliwości wnoszenia wkładu w Zed znajdziesz w [CONTRIBUTING.md](./CONTRIBUTING.md).

A poza tym... Zed Industries rekrutuje! Sprawdź ich stronę [praca](https://zed.dev/jobs) pod kątem otwartych stanowisk.

### Licencjonowanie

Kod źródłowy Zeda jest licencjonowany przede wszystkim na GPL-3.0-or-later, z komponentami Apache-2.0 w oznaczonych miejscach.

Informacje o licencjach zależności stron trzecich muszą być prawidłowo podane, aby CI przechodziło.

Używamy [`cargo-about`](https://github.com/EmbarkStudios/cargo-about), aby automatycznie dochować licencji open source. Jeśli CI zawodzi, sprawdź następujące rzeczy:

- Czy pojawia się błąd `no license specified` dla crate, który utworzyłeś? Jeśli tak, dodaj `publish = false` pod `[package]` w Cargo.toml swojego crate.
- Czy błąd `failed to satisfy license requirements` dotyczy zależności? Jeśli tak, ustal najpierw, jaką licencję ma projekt i czy ten system jest wystarczający, aby spełnić wymagania tej licencji. W razie wątpliwości zapytaj prawnika. Po zweryfikowaniu, że ten system jest akceptowalny, dodaj identyfikator SPDX licencji do tablicy `accepted` w `script/licenses/zed-licenses.toml`.
- Czy `cargo-about` nie znajduje licencji zależności? Jeśli tak, dodaj pole clarification na końcu `script/licenses/zed-licenses.toml`, zgodnie z opisem w [książce cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorowanie

Zed jest rozwijany przez **Zed Industries, Inc.**, firmę nastawioną na zysk.

Jeśli chcesz wesprzeć projekt finansowo, możesz to zrobić przez GitHub Sponsors. Darowizny sponsora trafiają bezpośrednio do Zed Industries i są używane jako ogólne przychody firmy. Ze sponsorowaniem nie wiążą się żadne przywileje ani uprawnienia.
