# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · 简体中文 · [繁體中文](README.zh-TW.md)

欢迎使用 Zed——由 [Atom](https://github.com/atom/atom) 与 [Tree-sitter](https://github.com/tree-sitter/tree-sitter) 的创造者打造的高性能、支持多人协作的代码编辑器。

---

## 关于本 fork

本仓库是 [zed-industries/zed](https://github.com/zed-industries/zed) 的公开 fork。本地化 Zed 构建（[NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)）所需的功能改动位于 `l10n` 分支。

- **包含的改动**（详细说明与跟进上游发布的流程见 [FORK-NOTES.md](FORK-NOTES.md)，为日文）
  - 文件监视轮询设置 —— 新增 `file_watcher_use_polling` / `file_watcher_poll_ms`，仅自动对网络共享、FUSE、9P/WSL drvfs 选用轮询
  - 会话恢复回退 —— 找不到恢复目标时回退到最近打开的工作区，处理多个实例争用同一会话 ID 导致标签页无法恢复的问题
  - 共享断开时保留标签页 —— SMB 等卷暂时不可达时不会关闭 worktree 并保留正文内容，在共享恢复前显示删除线和红色 ✕
- **快照标签** —— 将上游稳定标签加上本 fork 的改动（单个 commit）标记为 `v<版本>-l10n.<N>`；zed-i18n 的 `config/project.toml`（`zed_ref` / `zed_commit`）始终固定指向最新的快照
- **构建** —— 上游 CI 以 zed-industries 组织为前提，因此本仓库的所有 release job 在 fork 上都会被跳过（属正常设计）。本地化构建及其在 GitHub 上的发行在 NeoSloth/zed-i18n 一侧进行；产物未经代码签名，以独立的 "Zed i18n" 品牌安装（与官方 Zed 相区分）

---

### 安装

在 macOS、Linux 和 Windows 上，您可以[直接下载 Zed](https://zed.dev/download)，或通过本地包管理器安装 Zed（[macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)）。

其他平台尚不可用：

- Web（[跟进中的讨论](https://github.com/zed-industries/zed/discussions/26195)）

### 开发 Zed

- [为 macOS 构建 Zed](./docs/src/development/macos.md)
- [为 Linux 构建 Zed](./docs/src/development/linux.md)
- [为 Windows 构建 Zed](./docs/src/development/windows.md)

### 参与贡献

有关为 Zed 贡献的方式，请参阅 [CONTRIBUTING.md](./CONTRIBUTING.md)。

另外……Zed Industries 正在招聘！开放职位请查看其[招聘页面](https://zed.dev/jobs)。

### 许可证

Zed 源代码主要采用 GPL-3.0-or-later 许可，在标注处包含 Apache-2.0 组件。

第三方依赖的许可证信息必须正确提供，CI 才能通过。

我们使用 [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) 自动满足开源许可证要求。如果 CI 失败，请检查以下内容：

- 是否为您自己创建的 crate 显示了 `no license specified` 错误？如果是，请在该 crate 的 Cargo.toml 中的 `[package]` 下添加 `publish = false`。
- 针对某个依赖的错误是否为 `failed to satisfy license requirements`？如果是，请先确定该项目采用的许可证，并判断本系统是否足以满足该许可证的要求。如有不确定，请咨询律师。确认本系统可接受后，将该许可证的 SPDX 标识符添加到 `script/licenses/zed-licenses.toml` 的 `accepted` 数组中。
- `cargo-about` 是否找不到某个依赖的许可证？如果是，请在 `script/licenses/zed-licenses.toml` 末尾添加 clarification 字段，具体如 [cargo-about 手册](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration)所述。

## 赞助

Zed 由营利性公司 **Zed Industries, Inc.** 开发。

如果您愿意为该项目提供资金支持，可以通过 GitHub Sponsors 进行赞助。赞助款项直接归属 Zed Industries，用作公司的总体收入。赞助不附带任何特权或权益。
