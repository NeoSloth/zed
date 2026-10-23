# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · Français · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Bienvenue dans Zed, un éditeur de code multi-utilisateurs hautes performances, créé par les auteurs d'[Atom](https://github.com/atom/atom) et de [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## À propos de ce fork

Ceci est un fork public de [zed-industries/zed](https://github.com/zed-industries/zed). Les modifications fonctionnelles nécessaires à la compilation localisée de Zed ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) sont portées sur la branche `l10n`.

- **Modifications portées** (les détails et la procédure de suivi sont dans [FORK-NOTES.md](FORK-NOTES.md), en japonais)
  - Réglages du balayage (polling) de surveillance des fichiers — ajoute `file_watcher_use_polling` / `file_watcher_poll_ms` ; le polling est choisi automatiquement uniquement pour les partages réseau, FUSE et 9P/WSL drvfs
  - Repli pour la restauration de session — quand la cible de restauration est absente, on se rabat sur le dernier espace de travail ouvert ; répond au problème des onglets non restaurés quand plusieurs instances se disputent une même identifiant de session
  - Les onglets survivent à la coupure du partage — quand SMB ou un autre volume est temporairement injoignable, le worktree reste ouvert et le texte intact, affiché barré avec un ✕ rouge jusqu'au retour du partage
- **Tags instantanés (snapshot)** — un tag stable d'upstream plus les modifications du fork en un seul commit sont découpés en `v<version>-l10n.<N>` ; le `config/project.toml` de zed-i18n (`zed_ref` / `zed_commit`) épingle toujours l'instantané le plus récent
- **Compilations** — la CI d'upstream suppose l'organisation zed-industries, aussi tous les jobs de publication de ce dépôt sont sautés sur les forks (conçu ainsi). Les compilations localisées et leur distribution GitHub ont lieu dans NeoSloth/zed-i18n ; le résultat s'installe comme une application non signée sous le même nom que Zed officiel

---

### Installation

Sur macOS, Linux et Windows, vous pouvez [télécharger Zed directement](https://zed.dev/download) ou installer Zed via votre gestionnaire de paquets local ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Les autres plateformes ne sont pas encore disponibles :

- Web ([discussion de suivi](https://github.com/zed-industries/zed/discussions/26195))

### Développer Zed

- [Compiler Zed pour macOS](./docs/src/development/macos.md)
- [Compiler Zed pour Linux](./docs/src/development/linux.md)
- [Compiler Zed pour Windows](./docs/src/development/windows.md)

### Contribuer

Consultez [CONTRIBUTING.md](./CONTRIBUTING.md) pour découvrir comment contribuer à Zed.

Et aussi... Zed Industries recrute ! Consultez leur page [emplois](https://zed.dev/jobs) pour les postes ouverts.

### Licence

Le code source de Zed est sous licence principalement GPL-3.0-or-later, avec des composants Apache-2.0 là où c'est indiqué.

Les informations de licence des dépendances tierces doivent être correctement fournies pour que la CI passe.

Nous utilisons [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) pour nous conformer automatiquement aux licences open source. Si la CI échoue, vérifiez ce qui suit :

- Affichez-vous une erreur `no license specified` pour une crate que vous avez créée ? Si oui, ajoutez `publish = false` sous `[package]` dans le Cargo.toml de votre crate.
- L'erreur `failed to satisfy license requirements` concerne-t-elle une dépendance ? Si oui, déterminez d'abord quelle licence a le projet et si ce système suffit à respecter les exigences de cette licence. En cas de doute, demandez à un avocat. Une fois vérifié que ce système est acceptable, ajoutez l'identifiant SPDX de la licence dans le tableau `accepted` de `script/licenses/zed-licenses.toml`.
- `cargo-about` ne parvient pas à trouver la licence d'une dépendance ? Si oui, ajoutez un champ clarification à la fin de `script/licenses/zed-licenses.toml`, comme indiqué dans le [livre cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Soutien financier

Zed est développé par **Zed Industries, Inc.**, une entreprise à but lucratif.

Si vous souhaitez soutenir financièrement le projet, vous pouvez le faire via GitHub Sponsors. Les contributions vont directement à Zed Industries et sont utilisées comme revenus généraux de l'entreprise. Aucun avantage ni droit n'est associé au parrainage.
