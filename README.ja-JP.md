# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · 日本語 · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Zed へようこそ。Zed は [Atom](https://github.com/atom/atom) と [Tree-sitter](https://github.com/tree-sitter/tree-sitter) の製作者たちによる、高性能なマルチプレイヤーコードエディタです。

---

## このフォークについて

これは [zed-industries/zed](https://github.com/zed-industries/zed) の公開フォークです。ローカライズされた Zed ビルド（[NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)）で必要になる機能変更を `l10n` ブランチに乗せています。

- **乗せている変更**（詳細と本家リリースへの追従手順は [FORK-NOTES.md](FORK-NOTES.md)）
  - ファイル監視のポーリング設定 — `file_watcher_use_polling` / `file_watcher_poll_ms` を追加し、ネットワーク共有・FUSE・9P/WSL drvfs だけを自動でポーリングへ選ぶ
  - セッション復元の退避 — 復元先が見つからないとき最後に開いたワークスペースへ退避し、同時起動でセッション ID を奪い合ってタブが復元されない問題を扱う
  - 共有切断時のタブ保持 — SMB などが一時的に到達不能になっても worktree を閉じずに本文を保ち、復帰まで取り消し線と赤い ✕ を表示する
- **スナップショットタグ** — 本家の安定タグ + このフォークの変更 1 コミットを `v<版>-l10n.<N>` として切る。zed-i18n の `config/project.toml`（`zed_ref` / `zed_commit`）が常に最新のスナップショットを指す
- **ビルドについて** — 本家 CI は zed-industries 組織前提のため、このリポジトリでは全リリースジョブがフォークでスキップになる（仕様）。ローカライズ済みビルドとその GitHub 配布は NeoSloth/zed-i18n で行われ、成果物はコード署名なし・公式 Zed と同じアプリ名義でインストールされる

---

### インストール

macOS、Linux、Windows では [Zed を直接ダウンロード](https://zed.dev/download)するか、ローカルのパッケージマネージャー経由でインストールできます（[macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)）。

その他のプラットフォームはまだ利用できません:

- Web（[追跡中の議論](https://github.com/zed-industries/zed/discussions/26195)）

### Zed の開発

- [macOS 向けに Zed をビルドする](./docs/src/development/macos.md)
- [Linux 向けに Zed をビルドする](./docs/src/development/linux.md)
- [Windows 向けに Zed をビルドする](./docs/src/development/windows.md)

### コントリビュート

Zed への貢献の方法については [CONTRIBUTING.md](./CONTRIBUTING.md) を参照してください。

それから…Zed Industries 社は採用を行っています！募集している職種は同社の[求人ページ](https://zed.dev/jobs)でご確認ください。

### ライセンス

Zed のソースコードは主に GPL-3.0-or-later でライセンスされており、Apache-2.0 のコンポーネントはマークされている箇所に含まれます。

サードパーティ依存関係のライセンス情報は、CI が通るために正しく提供されている必要があります。

オープンソースライセンスへの準拠を自動化するために [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) を使用しています。CI が失敗している場合は次を確認してください:

- 自作の crate に対して `no license specified` エラーが出ていますか？その場合、その crate の Cargo.toml の `[package]` 下に `publish = false` を追加してください。
- 依存関係に対して `failed to satisfy license requirements` エラーですか？その場合、まずプロジェクトのライセンスが何かを特定し、そのライセンスの要求事項に対してこの仕組みで十分に対応できるかを判断してください。不確かな場合は弁護士に相談してください。この仕組みが受け入れられることを確認したら、そのライセンスの SPDX 識別子を `script/licenses/zed-licenses.toml` の `accepted` 配列に追加してください。
- `cargo-about` が依存関係のライセンスを見つけられませんか？その場合、[cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration) に指定されているとおり、`script/licenses/zed-licenses.toml` の末尾に clarification フィールドを追加してください。

## スポンサーシップ

Zed は営利企業である **Zed Industries, Inc.** によって開発されています。

プロジェクトを経済的に支援したい場合は、GitHub Sponsors 経由で支援できます。スポンサーシップは Zed Industries に直接渡され、会社の一般的な収益として使われます。スポンサーシップに付随する特典や権利はありません。
