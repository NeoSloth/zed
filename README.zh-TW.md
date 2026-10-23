# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · 繁體中文

歡迎使用 Zed——由 [Atom](https://github.com/atom/atom) 與 [Tree-sitter](https://github.com/tree-sitter/tree-sitter) 的創造者打造的高效能、支援多人協作的程式碼編輯器。

---

## 關於本 fork

本倉庫是 [zed-industries/zed](https://github.com/zed-industries/zed) 的公開 fork。本地化 Zed 組建（[NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)）所需的功能改動位於 `l10n` 分支。

- **已納入的改動**（詳細說明與跟進上游發布的流程見 [FORK-NOTES.md](FORK-NOTES.md)，為日文）
  - 檔案監視輪詢設定 —— 新增 `file_watcher_use_polling` / `file_watcher_poll_ms`，僅自動對網路共用、FUSE、9P/WSL drvfs 選用輪詢
  - 工作階段復原退避 —— 找不到復原目標時退避到最近開啟的工作區，處理多個執行個體爭用同一工作階段 ID 導致標籤頁無法復原的問題
  - 共用中斷時保留標籤頁 —— SMB 等磁碟區暫時不可達時不會關閉 worktree 並保留正文內容，在共用恢復前顯示刪除線和紅色 ✕
- **快照標籤** —— 將上游穩定標籤加上本 fork 的改動（單一 commit）標記為 `v<版本>-l10n.<N>`；zed-i18n 的 `config/project.toml`（`zed_ref` / `zed_commit`）始終固定指向最新的快照
- **組建** —— 上游 CI 以 zed-industries 組織為前提，因此本倉庫的所有 release job 在 fork 上都會被跳過（屬正常設計）。本地化組建及其在 GitHub 上的發行在 NeoSloth/zed-i18n 一側進行；產物未經程式碼簽名，以獨立的 "Zed i18n" 品牌安裝（與官方 Zed 區分）

---

### 安裝

在 macOS、Linux 和 Windows 上，您可以[直接下載 Zed](https://zed.dev/download)，或透過本地套件管理員安裝 Zed（[macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)）。

其他平台尚不可用：

- Web（[跟進中的討論](https://github.com/zed-industries/zed/discussions/26195)）

### 開發 Zed

- [為 macOS 組建 Zed](./docs/src/development/macos.md)
- [為 Linux 組建 Zed](./docs/src/development/linux.md)
- [為 Windows 組建 Zed](./docs/src/development/windows.md)

### 參與貢獻

關於為 Zed 貢獻的方式，請參閱 [CONTRIBUTING.md](./CONTRIBUTING.md)。

另外……Zed Industries 正在徵才！開放職缺請查看其[徵才頁面](https://zed.dev/jobs)。

### 授權

Zed 原始程式碼主要採用 GPL-3.0-or-later 授權，在標註處包含 Apache-2.0 元件。

第三方相依性的授權資訊必須正確提供，CI 才能通過。

我們使用 [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) 自動符合開源授權要求。如果 CI 失敗，請檢查以下內容：

- 是否為您自己建立的 crate 顯示了 `no license specified` 錯誤？如果是，請在該 crate 的 Cargo.toml 中的 `[package]` 下新增 `publish = false`。
- 針對某個相依性的錯誤是否為 `failed to satisfy license requirements`？如果是，請先確定該專案採用的授權，並判斷本系統是否足以符合該授權的要求。如有不確定，請諮詢律師。確認本系統可接受後，將該授權的 SPDX 識別碼加入到 `script/licenses/zed-licenses.toml` 的 `accepted` 陣列中。
- `cargo-about` 是否找不到某個相依性的授權？如果是，請在 `script/licenses/zed-licenses.toml` 末尾新增 clarification 欄位，具體如 [cargo-about 手冊](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration)所述。

## 贊助

Zed 由營利性公司 **Zed Industries, Inc.** 開發。

如果您願意為該專案提供資金支援，可以透過 GitHub Sponsors 進行贊助。贊助款項直接歸屬 Zed Industries，用作公司的總體收入。贊助不附帶任何特權或權益。
