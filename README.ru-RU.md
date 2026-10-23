# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · Русский · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Добро пожаловать в Zed — высокопроизводительный редактор кода с совместной работой нескольких пользователей от создателей [Atom](https://github.com/atom/atom) и [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Об этом форке

Это публичный форк [zed-industries/zed](https://github.com/zed-industries/zed). Изменения функциональности, необходимые локализованной сборке Zed ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)), содержатся в ветке `l10n`.

- **Внесённые изменения** (подробности и порядок отслеживания апстримных релизов — в [FORK-NOTES.md](FORK-NOTES.md), на японском языке)
  - Настройки опроса наблюдателя файлов — добавляют `file_watcher_use_polling` / `file_watcher_poll_ms`; опрос автоматически выбирается только для сетевых шар, FUSE и 9P/WSL drvfs
  - Отступной путь при восстановлении сессии — если целевая область восстановления отсутствует, выполняется возврат к последнему открытому workspace; решает проблему с невосстанавливаемыми вкладками, когда несколько экземпляров спорят за один session id
  - Вкладки выживают при отключении шары — если SMB или другой том временно недоступен, worktree остаётся открытым, содержимое сохраняется, показывается зачёркнутым с красным ✕, пока шара не вернётся
- **Теги-снапшоты** — стабильный тег апстрима плюс изменения форка одним коммитом вырезаются как `v<версия>-l10n.<N>`; `config/project.toml` в zed-i18n (`zed_ref` / `zed_commit`) всегда пинит новейший снапшот
- **Сборки** — апстримный CI предполагает организацию zed-industries, поэтому все релизные джобы в этом репозитории в форках пропускаются (по замыслу). Локализованные сборки и их дистрибуция на GitHub выполняются в NeoSloth/zed-i18n; результат устанавливается как неподписанное приложение под тем же именем, что и официальный Zed

---

### Установка

На macOS, Linux и Windows вы можете [скачать Zed напрямую](https://zed.dev/download) или установить Zed через локальный пакетный менеджер ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Другие платформы пока недоступны:

- Web ([отслеживающее обсуждение](https://github.com/zed-industries/zed/discussions/26195))

### Разработка Zed

- [Сборка Zed для macOS](./docs/src/development/macos.md)
- [Сборка Zed для Linux](./docs/src/development/linux.md)
- [Сборка Zed для Windows](./docs/src/development/windows.md)

### Участие

Способы внести вклад в Zed описаны в [CONTRIBUTING.md](./CONTRIBUTING.md).

И ещё... Zed Industries нанимает! Открытые вакансии смотрите на их странице [вакансий](https://zed.dev/jobs).

### Лицензирование

Исходный код Zed лицензирован преимущественно под GPL-3.0-or-later, с компонентами Apache-2.0 в помеченных местах.

Информация о лицензиях сторонних зависимостей должна быть корректно предоставлена, чтобы CI проходил.

Мы используем [`cargo-about`](https://github.com/EmbarkStudios/cargo-about), чтобы автоматически соблюдать лицензии с открытым исходным кодом. Если CI падает, проверьте следующее:

- Появляется ли ошибка `no license specified` для созданной вами crate? Если да, добавьте `publish = false` под `[package]` в Cargo.toml вашей crate.
- Ошибка `failed to satisfy license requirements` касается зависимости? Если да, сначала определите, какая лицензия у проекта, и достаточно ли этой системы для соблюдения требований этой лицензии. Если не уверены — спросите юриста. Убедившись, что эта система приемлема, добавьте SPDX-идентификатор лицензии в массив `accepted` в `script/licenses/zed-licenses.toml`.
- `cargo-about` не может найти лицензию зависимости? Если да, добавьте поле clarification в конец `script/licenses/zed-licenses.toml`, как указано в [книге cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Спонсорство

Zed разрабатывается **Zed Industries, Inc.**, коммерческой компанией.

Если вы хотите поддержать проект финансово, вы можете сделать это через GitHub Sponsors. Спонсорские взносы поступают напрямую в Zed Industries и используются как общий доход компании. Спонсорство не связано с какими-либо привилегиями или правами.
