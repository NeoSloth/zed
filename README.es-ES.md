# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · Español · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · [Türkçe](README.tr-TR.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Bienvenido a Zed, un editor de código multipersona de alto rendimiento creado por los autores de [Atom](https://github.com/atom/atom) y [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Acerca de este fork

Esto es un fork público de [zed-industries/zed](https://github.com/zed-industries/zed). Los cambios de funcionalidades que requiere la compilación localizada de Zed ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) se mantienen en la rama `l10n`.

- **Cambios incluidos** (los detalles y el procedimiento de seguimiento están en [FORK-NOTES.md](FORK-NOTES.md), en japonés)
  - Ajustes de sondeo del vigilante de archivos — añade `file_watcher_use_polling` / `file_watcher_poll_ms`; el sondeo se selecciona automáticamente solo para recursos compartidos de red, FUSE y 9P/WSL drvfs
  - Respaldo en la restauración de sesiones — cuando falta el destino de restauración, recurre al último espacio de trabajo abierto; aborda el problema de que las pestañas no se restauran cuando varias instancias compiten por una misma id de sesión
  - Las pestañas sobreviven a las desconexiones del recurso compartido — cuando SMB u otro volumen queda temporalmente inalcanzable, el worktree permanece abierto con el texto intacto, mostrado tachado con una ✕ roja hasta que el recurso vuelve
- **Etiquetas de snapshot** — una etiqueta estable de upstream más los cambios del fork como un solo commit se corta como `v<versión>-l10n.<N>`; el `config/project.toml` de zed-i18n (`zed_ref` / `zed_commit`) siempre fija el snapshot más reciente
- **Compilaciones** — la CI de upstream asume la organización zed-industries, por lo que todos los trabajos de release de este repositorio se omiten en los forks (por diseño). Las compilaciones localizadas y su distribución en GitHub se realizan en NeoSloth/zed-i18n; el resultado se instala como una aplicación sin firmar con el mismo nombre que Zed oficial

---

### Instalación

En macOS, Linux y Windows puedes [descargar Zed directamente](https://zed.dev/download) o instalar Zed mediante el gestor de paquetes local ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Otras plataformas aún no están disponibles:

- Web ([hilo de seguimiento](https://github.com/zed-industries/zed/discussions/26195))

### Desarrollar Zed

- [Compilar Zed para macOS](./docs/src/development/macos.md)
- [Compilar Zed para Linux](./docs/src/development/linux.md)
- [Compilar Zed para Windows](./docs/src/development/windows.md)

### Contribuir

Consulta [CONTRIBUTING.md](./CONTRIBUTING.md) para ver formas de contribuir a Zed.

Además... Zed Industries está contratando. Consulta su página de [empleo](https://zed.dev/jobs) para ver las posiciones disponibles.

### Licencias

El código fuente de Zed está licenciado principalmente bajo GPL-3.0-or-later, con componentes Apache-2.0 donde está indicado.

La información de licencias de las dependencias de terceros debe proporcionarse correctamente para que la CI pase.

Usamos [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) para cumplir automáticamente con las licencias de código abierto. Si la CI está fallando, revisa lo siguiente:

- ¿Aparece un error `no license specified` para una crate que has creado? Si es así, añade `publish = false` bajo `[package]` en el Cargo.toml de tu crate.
- ¿El error es `failed to satisfy license requirements` para una dependencia? Si es así, determina primero qué licencia tiene el proyecto y si este sistema es suficiente para cumplir los requisitos de esa licencia. Si no estás seguro, consulta a un abogado. Una vez verificado que este sistema es aceptable, añade el identificador SPDX de la licencia al array `accepted` en `script/licenses/zed-licenses.toml`.
- ¿`cargo-about` no encuentra la licencia de una dependencia? Si es así, añade un campo clarification al final de `script/licenses/zed-licenses.toml`, según lo especificado en el [libro de cargo-about](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Patrocinio

Zed es desarrollado por **Zed Industries, Inc.**, una empresa con fines de lucro.

Si deseas apoyar económicamente el proyecto, puedes hacerlo a través de GitHub Sponsors. Los patrocinios van directamente a Zed Industries y se usan como ingresos generales de la empresa. No hay ventajas ni derechos asociados con el patrocinio.
