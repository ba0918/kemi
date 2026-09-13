# UI の文言と既定 title を英語にする

## Goal

kemi の UI の文言（文章、ボタン、説明文、状態表示）とレビューの既定 title が英語になる。CLI の出力と Web UI のエラー表示は既に英語化済み。開発ドキュメント（仕様・CONTEXT.md・PROJECT.md・コードコメント）は日本語のまま残る。

## Specification

`docs/spec/kemi.md` — 実装で参照する節は R-DIST、R-INPUT。該当ステップに引用する。

## Approach and why

- 既に実装済み（このブランチの先行コミット）: CLI の出力、エラー（kemi-core / ApiError / ServerError）、Web UI のエラー表示・失敗通知の英語化。
- 残りは Web UI の通常文言（ボタン・ラベル・説明文・状態表示・モード・見出し・進行中表示・空状態）と、`crates/kemi-core/` の既定 title（"Working tree changes" / "Staged changes" / "Review of changes"）。
- Web UI の通常文言の翻訳は仕様で固定しない（ブレインストームで実装委譲が決定済み）。既定 title は仕様 R-INPUT が値を固定しているので、その値に合わせる。
- e2e は CLI の出力に日本語の文字が含まれないことを、kemi-core の単体テストは既定 title の値を検証する。

## Scope of change

変更してよいファイル:

- `web/index.html`（ページ本体のボタン・ナビ・tooltip・aria-label・`lang`）
- `web/assets/`（フロントのすべての文言: ボタン・ラベル・説明文・状態表示・モード・見出し・進行中表示・空状態）
- `web/*.test.js`（フロントの検証文字列）
- `crates/kemi-core/src/source/`（既定 title: mod.rs、manifest.rs、git.rs）
- `tests/e2e.rs`（既定 title の検証が無いことを確認済み。変更は不要だが、必要なら追随してよい）
- `README.md` / `skills/kemi/SKILL.md`（ボタン文言の英語化に伴う「承認 / 変更要求」への参照の更新のみ）

変更しないもの:

- `docs/`（仕様・CONTEXT.md・PROJECT.md・計画）— 日本語のまま
- コードコメント（`//` / `///`）— 日本語のまま
- JSON の I/F（フィールド名、verdict の値）
- 入力データ（コミットメッセージ・コメント・manifest の本文）— 変えない

## Step order and prerequisites

Step 1 → 2 → 3。各ステップの前提はそのステップに書く。

## Verification map

- Step 1: R-DIST の言語ポリシー（UI の文言は英語）と反例（UI の文言に日本語の文言が残る）
- Step 2: R-INPUT の既定 title（"Working tree changes" など）と成功条件
- Step 3: R-DIST の成功条件（全テストが通り、README / スキルが整合）

## Left to the implementer

- 英語の具体文言（Web UI の通常文言の翻訳）— 仕様は文言を固定しない
- テストの検証文字列の更新方法
- コミットの分割（1 関心: UI の文言と既定 title の英語化）

## Stop conditions

- docs（仕様・CONTEXT.md・コードコメント）を英語にしなければ仕様に合わないと判断する場合
- JSON の I/F や入力データ（コミットメッセージ・コメント・manifest の本文）を変えなければならないと判断する場合
- Web UI の文言の英語化が、フロントのテスト（`node --test web`）で安定して GREEN にならない場合

## Test command

プロジェクトの規約（PROJECT.md）が定める次のコマンドを使う:

- テスト: `cargo test`、`node --test web`
- lint: `cargo clippy -- -D warnings`、`npx tsc -p web --noEmit`
- 整形: `cargo fmt --check`

## Out of scope

- `docs/` の英語化（日本語のまま）
- JSON の I/F
- 入力データの変更
- README / skills の文言変更

---

## Step 1 — Web UI の通常文言を英語にする

Purpose: Web UI のボタン・ラベル・説明文・状態表示・モード・見出し・進行中表示・空状態を英語にする。
Specification: `docs/spec/kemi.md`#R-DIST（言語ポリシー・反例）。
Prerequisites: なし（CLI とエラー表示の英語化は実装済み）。
May change: `web/index.html`、`web/assets/`（features/、views/、model.js、state.js、app.js）、`web/*.test.js`。
Done when: エラー表示を除く Web UI のすべての文言（ボタン・ラベル・説明文・状態表示・モード・見出し・進行中表示・空状態・テーマ表示）が英語になっている。`web/index.html` のボタン・ナビ・tooltip・aria-label と `lang` 属性も英語になっている。`rg` で `web/`（index.html を含む）の UI 文言（文字列リテラルと HTML の属性値）に日本語の文字が含まれないことを確認できる。仕様 R-SEEN が固定する「閲」の印は対象外（そのツールチップ「すべて見た」は英語にする）。コードコメントとテストの入力データ（コミット subject など）は日本語のままでよい。
Shown by: test — `node --test web`（フロントのテスト）が GREEN。静的検査: `rg -n '[ぁ-んァ-ヶ一-龠]' web/index.html web/assets/` が、コードコメントと入力データ・「閲」の印以外で一致しないこと。
Left to the implementer: 英語の具体文言。model.js の見出し（「新側」「旧側」「ファイル全体」など）と state.js のモード（「最終形」「コミットごと」）の訳し方。
Stop and hand back if: コードコメントや入力データを英語にしなければ日本語が消えないと気付く（コメントと入力データは対象外）。

## Step 2 — 既定 title を英語にする

Purpose: `crates/kemi-core/` の既定 title を仕様 R-INPUT が固定する英語にする。
Specification: `docs/spec/kemi.md`#R-INPUT（既定 title: "Working tree changes" / "Staged changes" / "Review of changes"）。
Prerequisites: なし。
May change: `crates/kemi-core/src/source/`（mod.rs、manifest.rs、git.rs）。
Done when: worktree の既定 title が "Working tree changes"、staged が "Staged changes"、manifest の既定が "Review of changes" になっている。既定 title を検証する kemi-core の単体テスト（git.rs の "作業ツリーの変更"、manifest.rs の "変更のレビュー"）が新しい値に合わせて更新され、通る。
Shown by: test — `cargo test`（kemi-core の単体テスト）が GREEN。
Left to the implementer: テストの検証文字列（"作業ツリーの変更" → "Working tree changes" など）の更新。
Stop and hand back if: 仕様 R-INPUT が固定する title の値と、実装すべき英語が一致しないと判断する場合。

## Step 3 — 全体検証と配布物の整合確認

Purpose: 全テストと lint が通り、README / スキルがボタン文言の英語化と整合していることを確認する。
Specification: `docs/spec/kemi.md`#R-DIST（成功条件）。
Prerequisites: Step 1・Step 2。
May change: `README.md` / `skills/kemi/SKILL.md`（「承認 / 変更要求」への参照を新しい英語のボタン文言に合わせる。そのほかの整合に矛盾が見つかった場合も含む）。
Done when: `cargo test`、`node --test web`、`cargo clippy -- -D warnings`、`cargo fmt --check`、`npx tsc -p web --noEmit` がすべて成功する。README と `skills/kemi/SKILL.md` の「承認 / 変更要求」への参照が新しい英語のボタン文言と整合している。
Shown by: check — 上のコマンドを順に実行し、すべて成功することを確認。README / スキルの整合は git diff のレビューで確認する（機械検査は無い）。
Left to the implementer: なし。
Stop and hand back if: 英語化と無関係のテスト失敗が起きる、または README / スキルに、ボタン文言の参照以外の直すべき不整合が見つかる（変更対象の判断が要る）。