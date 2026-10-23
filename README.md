> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · English · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Welcome to Zed, a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## About This Fork

This README is fully translated into the languages linked at the top of the page; the English text here is the canonical version.

This is a public fork of [zed-industries/zed](https://github.com/zed-industries/zed). Feature changes required by the localized Zed build ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) are carried on the `l10n` branch.

- **Changes carried** (details and the tracking procedure are in [FORK-NOTES.md](FORK-NOTES.md), written in Japanese)
  - File watcher polling settings — adds `file_watcher_use_polling` / `file_watcher_poll_ms`; polling is chosen automatically only for network shares, FUSE, and 9P/WSL drvfs
  - Session restore fallback — when the restore target is missing, falls back to the last opened workspace, addressing tabs not restoring when multiple instances race for one session id
  - Tabs survive share disconnects — when SMB or another volume is briefly unreachable, the worktree stays open with the text intact, shown struck through with a red ✕ until the share returns
- **Snapshot tags** — an upstream stable tag plus the fork's changes as a single commit are cut as `v<version>-l10n.<N>`; zed-i18n's `config/project.toml` (`zed_ref` / `zed_commit`) always pins the newest snapshot
- **Builds** — upstream CI assumes the zed-industries organization, so every release job in this repository is skipped on forks (by design). Localized builds and their GitHub distribution run in NeoSloth/zed-i18n; the output installs as an unsigned app under the same name as official Zed

---

### Installation

On macOS, Linux, and Windows you can [download Zed directly](https://zed.dev/download) or install Zed via your local package manager ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Other platforms are not yet available:

- Web ([tracking discussion](https://github.com/zed-industries/zed/discussions/26195))

### Developing Zed

- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Linux](./docs/src/development/linux.md)
- [Building Zed for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Zed.

Zed Industries is hiring — check their [jobs](https://zed.dev/jobs) page for open roles.

### Licensing

Zed source code is licensed primarily under GPL-3.0-or-later, with Apache-2.0 components where marked.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Zed is developed by **Zed Industries, Inc.**, a for-profit company.

If you’d like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.
