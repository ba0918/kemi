# Changelog

kemi の版ごとの変更。版の正典はルート `Cargo.toml` の `version`（`docs/spec/kemi.md` の
`R-DIST`）。タグは `v<semver>`。読者は、前の版から上げる人。

## [Unreleased]

（なし）

## [0.1.5] - 2026-09-14

### 変更

- Windows（`x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`）を配布ターゲットに
  追加した。GitHub Releases の `kemi-<target>.zip` を解いて `kemi.exe` を PATH に置くと
  使える。結果ファイルは `%LOCALAPPDATA%\kemi\results\` に置き、`--no-open` を付けなければ
  ブラウザを自動で開く。Linux と macOS の使い方と、`--version` を除く CLI・JSON の契約は
  変わらない。

## [0.1.4] - 2026-09-14

### 修正

- ページ上部の meta を開くボタンと、開いた箱の読み上げ用の名前（`aria-label`）に日本語が
  残っていたのを英語にした。
- 件数の表示（コメント・suggestion・未読のファイル・展開する行）と、digest で省かれた
  グループの題が、1 件のときも複数形になっていた（"1 comments" など）のを単数形にした。

## [0.1.3] - 2026-09-14

### 変更

- UI の文言と CLI の出力を英語にした。ヘルプ、エラーメッセージ、stderr の運用メッセージ、
  ページのボタン・ラベル・説明文・エラー表示、レビューの既定の title（"Working tree
  changes" / "Staged changes" / "Review of changes"）が英語になる。JSON の形と終了コードは
  変わらない。開発ドキュメント（仕様・用語集）とコードコメントは日本語のまま。同梱の
  スキルと README も英語のボタン文言に合わせて更新した。`gh skill install` で入れ直すと
  反映される。

## [0.1.2] - 2026-09-13

### 変更

- 上部バー: manifest の自由記述の `meta` をバーに並べるのをやめ、題の右の小さな「i」を
  押すと題の下に label と value の一覧が開く形にした。meta が無ければ「i」は出ない。
  manifest の回で上部バーが 2 行になっていたのが 1 行に戻り、差分を読む領域が広がる。
- 承認対象のフッターをやめた。manifest の `approval` の `identity` は人が照合できる値では
  ないので画面には出さず、結果の JSON の `approval` にそのまま返すだけにする（JSON の形は
  変わらない）。同梱のスキル `skills/kemi/SKILL.md` は、承認後にその `identity` と実際に
  コミットするバイト列を突き合わせるよう書き換えた。`gh skill install` で入れ直すと反映される。

## [0.1.1] - 2026-09-13

最初のリリース。

### 提供するもの

- 変更をブラウザで読み、行ごとのコメントと suggestion を付けて、「承認」か「変更要求」で
  終えると、コメントが 1 つの JSON として起動したターミナルの stdout に返る。JSON の形と
  終了コード（0 承認 / 1 変更要求 / 2 エラー / 130 中断）は `docs/spec/kemi.md` の
  `R-SUBMIT` が契約。
- 入力は 4 つ: manifest（旧 diff-review と互換の JSON）、コミット範囲（`--from`、`--to` の
  既定は `HEAD`）、`--worktree`、`--staged`。
- コミット範囲は「最終形」（各ファイルを 1 回だけ。既定）と「コミットごと」の 2 つの
  見せ方を持ち、ページで切り替えられる（`--group-by` は起動時の表示だけを選ぶ）。最終形では
  各変更ブロックに、それを作ったコミット（由来）が出る。
- ページ: 1 列 / 2 列の表示、折返し、ファイルの木、重要のみと変更量順の絞り込み、変更間の
  移動（n / p）と位置の帯、ファイルごとの「見た」と進捗、ライブリロードの更新バッジ、
  テーマ（light / dark / solarized）。
- 送った結果は結果ファイルにも残り、`kemi --result` で取り直せる。`--digest` はページを
  出さずにレビューの地図を JSON で出す。
- エージェント向けの使い方の文書（Agent Skills 形式の `skills/kemi/SKILL.md`）を同梱。
  `gh skill install ba0918/kemi kemi` で入る。

### インストール

- GitHub Releases に Linux（x86_64 / aarch64、musl 静的）と macOS（x86_64 / arm64）の
  アーカイブを置く。`mise use -g github:ba0918/kemi` で入る。

[Unreleased]: https://github.com/ba0918/kemi/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/ba0918/kemi/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ba0918/kemi/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ba0918/kemi/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ba0918/kemi/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ba0918/kemi/releases/tag/v0.1.1
