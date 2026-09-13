# Changelog

kemi の版ごとの変更。版の正典はルート `Cargo.toml` の `version`（`docs/spec/kemi.md` の
`R-DIST`）。タグは `v<semver>`。読者は、前の版から上げる人。

## [Unreleased]

（なし）

## [0.1.0] - 2026-09-13

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

[Unreleased]: https://github.com/ba0918/kemi/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ba0918/kemi/releases/tag/v0.1.0
