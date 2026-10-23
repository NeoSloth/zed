# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · Português · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Bem-vindo ao Zed, um editor de código multiusuário de alto desempenho criado pelos autores do [Atom](https://github.com/atom/atom) e do [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Sobre este fork

Este é um fork público do [zed-industries/zed](https://github.com/zed-industries/zed). As alterações de funcionalidade exigidas pela compilação localizada do Zed ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) são mantidas no branch `l10n`.

- **Alterações incluídas** (os detalhes e o procedimento de acompanhamento estão em [FORK-NOTES.md](FORK-NOTES.md), em japonês)
  - Configurações de polling do observador de arquivos — adiciona `file_watcher_use_polling` / `file_watcher_poll_ms`; o polling é escolhido automaticamente apenas para compartilhamentos de rede, FUSE e 9P/WSL drvfs
  - Alternativa na restauração de sessão — quando o alvo de restauração não existe, recorre ao último workspace aberto; resolve o problema de abas que não são restauradas quando várias instâncias disputam um mesmo id de sessão
  - Abas sobrevivem à desconexão do compartilhamento — quando SMB ou outro volume fica temporariamente inacessível, o worktree permanece aberto com o texto intacto, exibido riscado com um ✕ vermelho até o compartilhamento voltar
- **Tags de snapshot** — uma tag estável do upstream mais as alterações do fork como um único commit são cortadas como `v<versão>-l10n.<N>`; o `config/project.toml` do zed-i18n (`zed_ref` / `zed_commit`) fixa sempre o snapshot mais recente
- **Compilações** — a CI do upstream assume a organização zed-industries, portanto todos os jobs de release deste repositório são pulados em forks (por design). As compilações localizadas e sua distribuição no GitHub ocorrem no NeoSloth/zed-i18n; o resultado é instalado como um app sem assinatura com a própria marca "Zed i18n", separada do Zed oficial

---

### Instalação

No macOS, Linux e Windows você pode [baixar o Zed diretamente](https://zed.dev/download) ou instalar o Zed pelo gerenciador de pacotes local ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Outras plataformas ainda não estão disponíveis:

- Web ([discussão de acompanhamento](https://github.com/zed-industries/zed/discussions/26195))

### Desenvolver o Zed

- [Compilar o Zed para macOS](./docs/src/development/macos.md)
- [Compilar o Zed para Linux](./docs/src/development/linux.md)
- [Compilar o Zed para Windows](./docs/src/development/windows.md)

### Contribuir

Consulte [CONTRIBUTING.md](./CONTRIBUTING.md) para ver como você pode contribuir com o Zed.

E também... a Zed Industries está contratando! Confira a página de [vagas](https://zed.dev/jobs) da empresa para posições abertas.

### Licenciamento

O código-fonte do Zed é licenciado principalmente sob a GPL-3.0-or-later, com componentes Apache-2.0 onde marcado.

As informações de licença de dependências de terceiros devem ser fornecidas corretamente para a CI passar.

Usamos o [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) para cumprir automaticamente as licenças open source. Se a CI estiver falhando, verifique o seguinte:

- Está aparecendo um erro `no license specified` para uma crate que você criou? Se sim, adicione `publish = false` sob `[package]` no Cargo.toml da sua crate.
- O erro `failed to satisfy license requirements` é de uma dependência? Se sim, primeiro determine qual licença o projeto tem e se este sistema é suficiente para cumprir os requisitos dessa licença. Se tiver dúvida, pergunte a um advogado. Depois de verificar que este sistema é aceitável, adicione o identificador SPDX da licença ao array `accepted` em `script/licenses/zed-licenses.toml`.
- O `cargo-about` não consegue encontrar a licença de uma dependência? Se sim, adicione um campo clarification no final de `script/licenses/zed-licenses.toml`, conforme especificado no [livro do cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Patrocínio

O Zed é desenvolvido pela **Zed Industries, Inc.**, uma empresa com fins lucrativos.

Se você gostaria de apoiar o projeto financeiramente, pode fazê-lo via GitHub Sponsors. Os patrocínios vão diretamente para a Zed Industries e são usados como receita geral da empresa. Não há regalias ou direitos associados ao patrocínio.
