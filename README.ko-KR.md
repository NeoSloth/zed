# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · 한국어 · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Zed를 환영합니다. Zed는 [Atom](https://github.com/atom/atom)과 [Tree-sitter](https://github.com/tree-sitter/tree-sitter)의 제작자들이 만든 고성능 멀티플레이어 코드 에디터입니다.

---

## 이 포크에 대하여

이 저장소는 [zed-industries/zed](https://github.com/zed-industries/zed)의 공개 포크입니다. 현지화된 Zed 빌드([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n))에 필요한 기능 변경은 `l10n` 브랜치에 담겨 있습니다.

- **담긴 변경 사항** (자세한 내용과 상류 추적 절차는 [FORK-NOTES.md](FORK-NOTES.md)에 일본어로 작성되어 있습니다)
  - 파일 감시 폴링 설정 — `file_watcher_use_polling` / `file_watcher_poll_ms`를 추가했습니다. 네트워크 공유, FUSE, 9P/WSL drvfs에만 자동으로 폴링을 선택합니다
  - 세션 복원 대체 동작 — 복원 대상이 없을 때 마지막으로 열었던 워크스페이스로 대체하며, 여러 인스턴스가 하나의 세션 ID를 놓고 경쟁해 탭이 복원되지 않는 문제를 다룹니다
  - 공유 연결 끊김 시 탭 유지 — SMB 등 볼륨에 일시적으로 도달할 수 없어도 worktree를 닫지 않고 본문 내용을 보존하며, 공유가 돌아올 때까지 취소선과 빨간 ✕를 표시합니다
- **스냅샷 태그** — 본가의 안정 태그에 이 포크의 변경 1커밋을 더해 `v<버전>-l10n.<N>`로 태그합니다. zed-i18n의 `config/project.toml`(`zed_ref` / `zed_commit`)이 항상 최신 스냅샷을 고정합니다
- **빌드** — 본가 CI는 zed-industries 조직을 전제로 하므로 이 저장소의 모든 릴리스 잡은 포크에서 스킵됩니다(정상 동작). 현지화된 빌드와 GitHub 배포는 NeoSloth/zed-i18n에서 수행되며, 산출물은 서명되지 않은 'Zed i18n' 자체 브랜드 앱으로, 공식 Zed와 구분되게 설치됩니다

---

### 설치

macOS, Linux, Windows에서는 Zed를 [직접 다운로드](https://zed.dev/download)하거나 로컬 패키지 관리자([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers))를 통해 설치할 수 있습니다.

기타 플랫폼은 아직 사용할 수 없습니다:

- Web([추적 중인 토론](https://github.com/zed-industries/zed/discussions/26195))

### Zed 개발하기

- [macOS용으로 Zed 빌드하기](./docs/src/development/macos.md)
- [Linux용으로 Zed 빌드하기](./docs/src/development/linux.md)
- [Windows용으로 Zed 빌드하기](./docs/src/development/windows.md)

### 기여하기

Zed에 기여할 수 있는 방법은 [CONTRIBUTING.md](./CONTRIBUTING.md)를 참고하세요.

그리고... Zed Industries는 채용 중입니다! 공개 직무는 해당 [채용 페이지](https://zed.dev/jobs)에서 확인하세요.

### 라이선스

Zed 소스 코드는 주로 GPL-3.0-or-later로 라이선스되며, 표시된 부분에 Apache-2.0 구성 요소가 포함됩니다.

서드파티 의존성의 라이선스 정보는 CI가 통과하도록 올바르게 제공되어야 합니다.

오픈소스 라이선스 준수를 자동화하기 위해 [`cargo-about`](https://github.com/EmbarkStudios/cargo-about)을 사용합니다. CI가 실패한다면 다음을 확인하세요:

- 직접 만든 crate에 대해 `no license specified` 오류가 표시되나요? 그렇다면 해당 crate의 Cargo.toml의 `[package]` 아래에 `publish = false`를 추가하세요.
- 의존성에 대해 `failed to satisfy license requirements` 오류인가요? 그렇다면 먼저 프로젝트가 어떤 라이선스를 갖고 있는지, 그리고 이 체계가 해당 라이선스의 요구 사항을 준수하기에 충분한지 판단하세요. 불확실하다면 변호사에게 문의하세요. 이 체계가 허용 가능함을 확인했다면 해당 라이선스의 SPDX 식별자를 `script/licenses/zed-licenses.toml`의 `accepted` 배열에 추가하세요.
- `cargo-about`가 의존성의 라이선스를 찾지 못하나요? 그렇다면 [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration)에 명시된 대로 `script/licenses/zed-licenses.toml` 맨 끝에 clarification 필드를 추가하세요.

## 후원

Zed는 영리 기업 **Zed Industries, Inc.**가 개발하고 있습니다.

프로젝트를 금전적으로 지원하고 싶다면 GitHub Sponsors를 통해 할 수 있습니다. 후원은 Zed Industries에 직접 전달되어 회사의 일반적인 수입으로 사용됩니다. 후원에 수반되는 특전이나 권한은 없습니다.
