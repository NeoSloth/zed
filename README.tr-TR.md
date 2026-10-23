# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

[Čeština](README.cs-CZ.md) · [Deutsch](README.de-DE.md) · [English](README.md) · [Español](README.es-ES.md) · [Français](README.fr-FR.md) · [Italiano](README.it-IT.md) · [日本語](README.ja-JP.md) · [한국어](README.ko-KR.md) · [Polski](README.pl-PL.md) · [Português](README.pt-BR.md) · [Русский](README.ru-RU.md) · Türkçe · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md)

Zed'e hoş geldiniz: [Atom](https://github.com/atom/atom) ve [Tree-sitter](https://github.com/tree-sitter/tree-sitter)'in yaratıcılarından gelen, birden çok kullanıcının ortak çalışmasını destekleyen yüksek performanslı bir kod editörü.

---

## Bu fork hakkında

Bu, [zed-industries/zed](https://github.com/zed-industries/zed) deposunun herkese açık bir forkudur. Yerelleştirilmiş Zed derlemesinin ([NeoSloth/zed-i18n](https://github.com/NeoSloth/zed-i18n)) gerektirdiği işlev değişiklikleri `l10n` dalında taşınır.

- **Taşınan değişiklikler** (ayrıntılar ve upstream takibi prosedürü [FORK-NOTES.md](FORK-NOTES.md) dosyasında, Japonca yazılmıştır)
  - Dosya izleyici yoklama (polling) ayarları — `file_watcher_use_polling` / `file_watcher_poll_ms` ekler; polling yalnızca ağ paylaşımları, FUSE ve 9P/WSL drvfs için otomatik seçilir
  - Oturum geri yükleme yedek yolu — geri yüklenecek hedef bulunamadığında son açılan workspace'e geri dönülür; birden çok örneğin tek bir oturum kimliği için yarışması nedeniyle sekmelerin geri yüklenememesi sorununu ele alır
  - Paylaşım bağlantısı koptuğunda sekmeler korunur — SMB veya başka bir birim geçici olarak ulaşılamaz olduğunda worktree kapanmaz ve metin yerinde kalır; paylaşım geri gelene kadar üstü çizili ve kırmızı bir ✕ ile gösterilir
- **Anlık görüntü (snapshot) etiketleri** — kararlı bir upstream etiketi ile fork'un değişiklikleri tek commit olarak `v<sürüm>-l10n.<N>` biçiminde etiketlenir; zed-i18n'in `config/project.toml` dosyası (`zed_ref` / `zed_commit`) her zaman en yeni anlık görüntüyü sabitler
- **Derlemeler** — upstream CI, zed-industries organizasyonunu varsayar; bu nedenle bu depodaki tüm sürüm işleri forklarda atlanır (tasarım gereği). Yerelleştirilmiş derlemeler ve bunların GitHub üzerinden dağıtımı NeoSloth/zed-i18n tarafında yürür; çıktı, resmi Zed'den ayrı, kendi "Zed i18n" markasına sahip imzasız bir uygulama olarak kurulur

---

### Kurulum

macOS, Linux ve Windows'ta Zed'i [doğrudan indirebilir](https://zed.dev/download) veya yerel paket yöneticiniz üzerinden kurabilirsiniz ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Diğer platformlar henüz kullanılamıyor:

- Web ([takip tartışması](https://github.com/zed-industries/zed/discussions/26195))

### Zed geliştirmek

- [Zed'i macOS için derlemek](./docs/src/development/macos.md)
- [Zed'i Linux için derlemek](./docs/src/development/linux.md)
- [Zed'i Windows için derlemek](./docs/src/development/windows.md)

### Katkıda bulunmak

Zed'e nasıl katkıda bulunabileceğinizi görmek için [CONTRIBUTING.md](./CONTRIBUTING.md) dosyasına bakın.

Bu arada... Zed Industries işe alım yapıyor! Açık pozisyonlar için şirketin [iş](https://zed.dev/jobs) sayfasına göz atın.

### Lisanslama

Zed kaynak kodu ağırlıklı olarak GPL-3.0-or-later ile lisanslanmıştır; işaretli yerlerde Apache-2.0 bileşenleri bulunur.

Üçüncü taraf bağımlılıkların lisans bilgileri, CI'nin geçmesi için doğru biçimde sağlanmalıdır.

Açık kaynak lisanslarına otomatik uymak için [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) kullanıyoruz. CI başarısız oluyorsa şunları kontrol edin:

- Oluşturduğunuz bir crate için `no license specified` hatası mı görüyorsunuz? Öyleyse crate'inizin Cargo.toml dosyasında `[package]` altına `publish = false` ekleyin.
- Hata bir bağımlılık için `failed to satisfy license requirements` mı? Önce projenin hangi lisansa sahip olduğunu ve bu sistemin bu lisansın gerekliliklerine uymak için yeterli olup olmadığını belirleyin. Emin değilseniz bir avukata danışın. Bu sistemin kabul edilebilir olduğunu doğruladıktan sonra, lisansın SPDX tanımlayıcısını `script/licenses/zed-licenses.toml` içindeki `accepted` dizisine ekleyin.
- `cargo-about` bir bağımlılığın lisansını bulamıyor mu? Öyleyse [cargo-about kitabında](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration) belirtildiği şekilde `script/licenses/zed-licenses.toml` dosyasının sonuna bir clarification alanı ekleyin.

## Sponsorluk

Zed, kâr amacı güden **Zed Industries, Inc.** tarafından geliştirilmektedir.

Projeye maddi olarak destek vermek isterseniz GitHub Sponsors üzerinden bunu yapabilirsiniz. Sponsorluklar doğrudan Zed Industries'e gider ve şirketin genel geliri olarak kullanılır. Sponsorlukla ilişkili hiçbir ayrıcalık veya hak yoktur.
