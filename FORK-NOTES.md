# このフォークの運用メモ

`https://github.com/zed-industries/zed` の公開フォーク（`NeoSloth/zed`）です。日本語化ビルドで必要になる変更を `l10n` ブランチに乗せ、本家では**安定リリースのタグだけ**を追います。プレリリース（`v1.25.0-pre` など）と `main` は追いません。

## リモート構成

| remote | 内容 |
| --- | --- |
| `origin` | `https://github.com/NeoSloth/zed`。公開フォーク。`l10n` とスナップショットタグを置く |
| `upstream` | `https://github.com/zed-industries/zed`。本家。`main` と全タグを読む |

ビルドは同じアカウントの `zed-i18n` リポジトリで行い、`config/project.toml` の pin（`zed_repository` / `zed_ref` / `zed_commit`）がこのリポジトリのタグを指します。`zed_ref` がないと `zed_version` のタグ名のまま clone されるので、フォーク側の変更は拾われません。

## 本家の新規リリースへの追従

順序は `zed-industries/zed` → この `l10n` → `zed-i18n` の `upstream`（LI-NA/zed-i18n）のマージの順にします。LI-NA も同じ Zed の版に短い間で追随するので、後から追うと翻訳工程と docs の作業が重複し、`catalog/`・`manifest/` で衝突します（衝突したら上流のファイルを取って、pin 済みソースに対して `extract` を実行し直す）。

1. `git fetch upstream --tags`
2. `git merge-tree --write-tree --name-only l10n v<新>` — 載せ直したときに何が衝突するかを数えておく
3. `git rebase --onto v<新> v<旧> l10n` — 自分の変更の列を新しい安定タグの上に載せ直す。列が小さいうちはこれが一番速く、本家のコミットも署名込みでそのまま残ります。衝突したら 1 件ずつ解決してください
4. `git tag -a v<新>-l10n.<N> -m "…"` — 本家リリースとローカル変更の組み合わせのスナップショット
5. `git push -f origin l10n v<新>-l10n.<N>` — 載せ直しなので force push になります

変更の列が大きくなって衝突の扱いが重くなったら、3 を `git merge v<新>`（マージコミットを作る形）に置き換えてください。衝突解決を 1 回で済ませられる代わりに、履歴にマージが増えます。
6. `zed-i18n` 側で `config/project.toml` の `zed_version`（本家の版名）／`zed_ref`（4 で作ったタグ）／`zed_commit`（その SHA）を書き換えて `fetch-zed` → `extract` → 翻訳工程 → ビルド

`extract` は pin を変えたときは必ず実行してください。UI 文言が増えていなくても `manifest/ui-strings.json` が抱えている byte offset がズレます。ズレたままビルドすると `occurrence byte span is stale` で止まりますが、`validate` はキーとプレースホルダしか見ないので通ってしまいます。手作業で登録した文字列（保存時の確認文 2 件）は `extract` では再現されないので、新しいソースの byte 範囲を確認したうえで再登録が必要です。

名前についての注意: `l10n` を接尾辞として混ぜたタグ（`v1.25.0-l10n.1`）は `zed_ref` 側にだけ使い、`zed_version` は本家そのままの `v1.25.0` にします。版名に接尾辞を入れると .deb のバージョン文字列やアプリのバージョン表記が崩れます。

### 衝突が起きやすい場所

`l10n` が触っているのは本家でも編集の多い領域です。`crates/worktree/src/worktree.rs` の走査状態、`crates/project/src/buffer_store.rs` の buffer 対応、`crates/editor/src/items.rs` のタブ表示、`crates/workspace/src/pane.rs` の保存導線なので、バージョンが離れた追従では衝突解決を前提に見てください。`v1.24.3 → v1.24.4` は本家側 2 コミット 3 ファイルで衝突ゼロでした。

## これまで乗せた変更

### 1. ファイル監視のポーリング設定（`66700ea`）

`file_watcher_use_polling` と `file_watcher_poll_ms` を追加しました。既定では自動判定で、ボリュームのファイルシステム名（macOS）や `GetDriveTypeW`（Windows）からネットワーク共有・FUSE・9P/WSL drvfs だけをポーリングへ選び、ローカルディスクは OS の監視を使います。設定を持たない環境では従来どおりの動作です。

### 2. セッション復元の退避（`2ec82b6`）

復元対象のワークスペースが見つからないとき、空のウィンドウを開く代わりに最後に開いたワークスペースへ退避するようにしました。Zed を同時に複数起動したときにセッション ID を奪い合ってタブが復元されない問題（upstream issue #65166 と同型）への対処です。

### 3. 共有が切れたときのタブ保持（`de0ebbc`）

SMB などのネットワークボリュームが一時的に到達不能になっても、その上のファイルの worktree を閉じなくなりました。タブは閉じずに本文を保ち、取り消し線と赤い ✕ を表示します。共有が戻ればネットワーク上の内容を正として読み直します。未保存の編集がある場合は本家既存の競合導線に乗せて、保存時に選ばせます。到達不能なまま保存しようとすると「このファイルを含むネットワーク共有が切断されています。このまま保存しますか？」を確認します。Zed を再起動した場合も、ディスクに見えないファイルの内容は保持して `DiskState::Deleted` として復元するので、空の保存で共有側を潰しません。ローカルファイルが本当に削除されたときの既存動作は変えていません。

## 履歴

変更は本家の安定タグ 1 つの上にまとめ 1 コミットでのせています。作業の途中経過は履歴に残しません。追従のたびに、列ごと新しいタグの上へ載せ直します（`git rebase --onto`）。

- `v1.24.4-l10n.1` … 1〜3 を本家 v1.24.4 のタグコミットの上に載せたもの。ここが `zed-i18n` の pin が指すスナップショットです
- docs（このファイル） … その後の別コミット。ビルドのソースには影響しません

1.24.3 に乗せたスナップショットの系列は破棄しました。同じ内容は `zed-i18n` の `patches/v1.24.3-*.patch` から再現できます。コミットの作者と committer は GitHub の noreply アドレス（`62577569+NeoSloth@users.noreply.github.com`）で統一していて、これはマシンの global 設定も同じ値になっています。

## このリポジトリでの運用メモ

- `README.md` 冒頭の `> [!IMPORTANT]` は、変更をレビューした人間だけが消すマーカーです（CLAUDE.md の規約）。`l10n` では意図的に未コミットのままにしています。
- `cargo test -p worktree` の `test_file_scan_exclusions` と `test_fs_events_in_exclusions` は、この macOS では `.DS_Store` のため変更と無関係に失敗します。
- clippy は `cargo clippy` ではなく `./script/clippy` を使います。
- 追従後の型チェックは `cargo check -p language_models -p project -p worktree -p editor -p fs -p workspace` で足ります。ビルドそのものは `zed-i18n` の README に書いた `build-shard` コマンドで行い、`/Applications/Zed.app` を置き換えます。
