# CLI の出力を英語にする

## Goal

kemi の CLI 出力（ヘルプ、エラーメッセージ、stderr の運用メッセージとライブ表示）と、Web UI のエラー表示が英語になり、Web UI の通常文言とレビューの既定 title は日本語のまま残る。

## Specification

`docs/spec/kemi.md` — 実装で参照する節は R-DIST、R-INPUT-6、R-COMMENT、R-RESULT。該当ステップに引用する。

## Approach and why

- 変更範囲は CLI 出力とエラーメッセージの文字列。翻訳の具体文言は仕様で固定しない（ブレインストームで実装委譲が決定済み）。
- エラーメッセージは kemi が作るすべてのエラー（入力の読み取り、manifest の検証、git の失敗、focus の検証、API のエラー）を含み、Web UI のエラー表示も英語になる（仕様 R-DIST で承認済み）。そのため `crates/kemi-core/` のエラー型と `crates/kemi-server/` の API エラーも変更対象。
- e2e の CLI 出力検証を「日本語の文字が含まれないこと」の検査に書き換える。英文字列を固定しないため、文言の言い換えでテストが壊れない（仕様の成功条件が定めた方式）。
- テスト（e2e）を先に書き換えて RED を確認し、実装で GREEN にする TDD の順序で進める。

## Scope of change

変更してよいファイル:

- `src/main.rs`（CLI のヘルプ・エラーメッセージ・運用メッセージ）
- `crates/kemi-core/src/source/`（`SourceError` の Display: mod.rs、manifest.rs、git.rs）
- `crates/kemi-core/src/domain/focus.rs`（`FocusError` の Display）
- `crates/kemi-core/src/domain/digest.rs`（省略グループのタイトル「…ほか N グループ」）
- `crates/kemi-server/src/api.rs`（stderr のライブ表示と `ApiError` のメッセージ）
- `crates/kemi-server/src/lib.rs`（`ServerError::Stopped` の既定文言）
- `tests/e2e.rs`（CLI 出力の検証）

変更しないもの（対象外）:

- `web/` — Web UI の通常文言（ボタン・ラベル・説明文）は日本語のまま。エラー表示はサーバが返す英語のメッセージを使うため、フロント側は変更しない
- 既定 title（「作業ツリーの変更」「ステージ済みの変更」「変更のレビュー」）— Web の見出しにも表示されるため日本語のまま
- JSON の I/F — フィールド名と verdict の値は既に英語。コミットメッセージ・コメント・manifest の本文（入力データ）は変えない
- `README.md` / `skills/kemi/SKILL.md` — 英語。日本語の CLI メッセージへの言及は確認済みで無い

## Step order and prerequisites

Step 1 → 2 → 3 → 4 → 5。各ステップの前提はそのステップに書く。

## Verification map

- Step 1・5: R-DIST の成功条件「e2e テストが、CLI のヘルプと、使い方の誤りで出力されるエラーメッセージに、日本語の文字が含まれないことを検証する」
- Step 2・3・4: R-DIST の要件「CLI の出力（ヘルプ、エラーメッセージ、stderr の運用メッセージとライブ表示）は英語にする」「エラーメッセージには kemi が作るすべてのエラーを含み、Web UI のエラー表示も英語になる」と反例「CLI の出力に日本語の文言が残る」
- Step 2: R-INPUT-6（ヘルプと使い方の誤りの終了コード 2、`kemi: <url>` の行を stderr に出す契約）
- Step 4: R-COMMENT（人間向けのライブ表示を stderr に出す。文言は契約としない）、R-RESULT（保存先を stderr に出す、保存失敗は警告のみ）

## Left to the implementer

- 英語の具体文言（ヘルプ、エラーメッセージ、ライブ表示、digest の省略タイトルの翻訳）— 仕様は文言を固定しない
- e2e の検証の具体的な書き方（どの起動操作で何を検査し、日本語の文字をどう判定するか）
- コミットの分割（1 関心: CLI 出力とエラーメッセージの英語化）

## Stop conditions

- 既定 title や Web UI の通常文言を英語にしなければ仕様に合わないと判断する場合
- JSON のキーやフィールド名、manifest / コミットメッセージの中身を変えなければならないと判断する場合
- e2e の「日本語が含まれない」検証が、現状の出力で安定して RED にならない場合

## Test command

プロジェクトの規約（PROJECT.md）が定める次のコマンドを使う:

- テスト: `cargo test`
- lint: `cargo clippy -- -D warnings`
- 整形: `cargo fmt --check`

## Out of scope

- Web UI の通常文言（`web/`）
- 既定 title（`crates/kemi-core/` のソース側）
- JSON の I/F
- README / skills の文言変更

---

## Step 1 — e2e の CLI 出力検証を「日本語が含まれない」に書き換える

Purpose: CLI 出力とエラーの検証を、英語の特定文言ではなく日本語不在の検査にする。
Specification: `docs/spec/kemi.md`#R-DIST（成功条件）。
Prerequisites: なし。
May change: `tests/e2e.rs`。
Done when: 次の経路の検証が「出力に日本語の文字が含まれないこと」を検査する形になっている — (a) 入力モード無しで起動したときのヘルプ、(b) 使い方の誤りのエラー（`--out` など）、(c) 存在しない manifest を渡したときのエラー（kemi-core の経路）、(d) コメント追加時のライブ表示。
Shown by: test — `cargo test --test e2e` で該当テストが RED（現状の日本語出力では日本語不在の検査が失敗する）。
Left to the implementer: 日本語の文字の判定方法（ひらがな・カタカナ・漢字の Unicode 範囲の検査など）、検証する起動操作と検証の書き方。
Stop and hand back if: 日本語不在の検証が現状の出力でも通ってしまう（ヘルプやエラーに日本語が残っていない）。

## Step 2 — src/main.rs の CLI 出力を英語にする

Purpose: CLI のヘルプ、エラーメッセージ、運用メッセージを英語にする。
Specification: `docs/spec/kemi.md`#R-DIST（言語ポリシー・反例）、#R-INPUT-6（ヘルプと使い方の誤りの終了コード 2、`kemi: <url>` の行）。
Prerequisites: Step 1。
May change: `src/main.rs`。
Done when: `USAGE` 定数、`parse_args` と `validate_result_flags` のエラーメッセージ、`fail()` に渡すメッセージ、stderr の運用メッセージ（結果ファイルの保存先など）が英語になっている。`kemi: <url>` の行と `kemi:` プレフィックスは維持する。JSON のキーは変えない。
Shown by: test — `cargo test --test e2e` で Step 1 の (a) ヘルプと (b) 使い方の誤りの検証が GREEN になる。(c) と (d) は後続ステップまで RED のまま。
Left to the implementer: 英語の具体文言。USAGE の構成（現行の「使い方 / 共通のフラグ / --result と一緒に使うフラグ」の区切りを英語で保つかどうか）。
Stop and hand back if: kemi-core や kemi-server の変更なしにヘルプやエラーが英語にならないと気付く。

## Step 3 — kemi-core のエラー文言と digest の省略タイトルを英語にする

Purpose: `SourceError` / `FocusError` の Display と digest の省略タイトルを英語にする。
Specification: `docs/spec/kemi.md`#R-DIST（エラーメッセージは kemi が作るすべてのエラーを含み英語）。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/source/mod.rs`、`crates/kemi-core/src/source/manifest.rs`、`crates/kemi-core/src/source/git.rs`、`crates/kemi-core/src/domain/focus.rs`、`crates/kemi-core/src/domain/digest.rs`。
Done when: `SourceError` と `FocusError` の Display が英語になり、digest の省略タイトル（「…ほか N グループ」）が英語になっている。既定 title（「作業ツリーの変更」など）と、テスト内のデータ文字列は変えない。
Shown by: test — `cargo test --test e2e` で Step 1 の (c) 存在しない manifest の検証が GREEN になる。`cargo test`（kemi-core の単体テスト）も通る。
Left to the implementer: 英語の具体文言。
Stop and hand back if: 既定 title や Web UI の通常文言と共通の文字列を変えなければならないと気付く。

## Step 4 — kemi-server のライブ表示と API エラーを英語にする

Purpose: stderr のライブ表示と `ApiError` のメッセージ、`ServerError::Stopped` の既定文言を英語にする。
Specification: `docs/spec/kemi.md`#R-DIST（エラーメッセージとライブ表示は英語）、#R-COMMENT（ライブ表示）、#R-RESULT（保存先・保存失敗の警告）。
Prerequisites: Step 1。
May change: `crates/kemi-server/src/api.rs`、`crates/kemi-server/src/lib.rs`。
Done when: `eprintln!` のライブ表示（由来の失敗・コメント追加・結果保存の失敗）と、コメント追加表示内の「ファイル全体」、`ApiError` のメッセージ（not_found / bad_request / forbidden / internal 経由）、`ServerError::Stopped` の既定文言が英語になっている。
Shown by: test — `cargo test --test e2e` で Step 1 の (d) ライブ表示の検証が GREEN になる。`cargo test` も通る。
Left to the implementer: 英語の具体文言。
Stop and hand back if: `web/` の変更なしに API エラーが英語にならないと気付く（フロントは変更しない）。

## Step 5 — 全体検証と配布物の整合確認

Purpose: 全テストと lint が通り、README / スキルが変更後の契約と整合していることを確認する。
Specification: `docs/spec/kemi.md`#R-DIST（成功条件）。
Prerequisites: Step 2・Step 3・Step 4。
May change: `README.md` / `skills/kemi/SKILL.md`（整合に矛盾が見つかった場合のみ）。
Done when: `cargo test` がすべて通り、`cargo clippy -- -D warnings` と `cargo fmt --check` が成功する。README と `skills/kemi/SKILL.md` に、日本語の CLI メッセージや英語化された文言への不整合が無い。
Shown by: check — `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check` の順に実行し、すべて成功することを確認。README / スキルの整合は git diff のレビューで確認する（機械検査は無い）。
Left to the implementer: なし。
Stop and hand back if: 英語化と無関係のテスト失敗が起きる、または README / スキルに直すべき不整合が見つかる（変更対象の判断が要る）。