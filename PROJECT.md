# Project Context

## What this is

kemi は、変更をブラウザで読むためのローカルなレビュー道具。単一バイナリを
GitHub Releases で配り、`mise use -g github:ba0918/kemi` でインストールする。
人間がページで行コメントと suggestion を付け、「承認」か「変更要求」でレビューを
終えると、コメントが 1 つの JSON として実行ターミナル（通常はレビューを頼んだ
エージェント）へ返る。元データは manifest / コミット範囲 / worktree / staged の
4 モードで与える。名前は「閲する（けみする）」から。

## Stack and layout

Rust の workspace。ドメインは純粋関数、HTTP は axum、フロントはビルドなしの
ネイティブ ESM + CSS で、release ビルドではバイナリに埋め込む。

| Path | 内容 |
|---|---|
| `docs/spec/kemi.md` | 承認済みの仕様。契約（CLI、JSON、性能条件）の正典 |
| `CONTEXT.md` | 用語集。二通りに読める語の読みの正典 |
| `src/main.rs` | CLI と配線、stdout / stderr、終了コード |
| `crates/kemi-core/src/domain/` | 純粋なドメイン（整列、コメント、digest、ノイズ判定） |
| `crates/kemi-core/src/source/` | git / manifest の読み取りアダプタ |
| `crates/kemi-server/` | HTTP と SSE（axum） |
| `crates/kemi-webview/` | 資産の埋め込み |
| `web/` | ESM と CSS のソース。`tsc --checkJs` は型検査のみ（層は下の節） |
| `scripts/` | フィクスチャ生成と起動時間の計測 |

### `web/assets/` の層

ページの JS は 4 層で、依存は上から下の一方向だけ。

| 層 | 中身 |
|---|---|
| `app.js` | 結線と起動だけ。すべてを import してよい唯一のモジュール |
| `features/` | 状態を変え、`api.js` を呼び、描き直す |
| `views/` | 要素を作る。`api.js` を除く leaf と、ほかの `views/` を import してよい（循環させない）。`api.js`・`features/`・`app.js` は import しない |
| leaf | `model.js`（純粋）、`api.js`（通信）、`dom.js`（要素と道具）、`state.js`（状態と派生の読み）、`storage.js`（localStorage）、`actions.js`（下から上を呼ぶ入れ物） |

- 下から上への呼び出し（view のボタンが feature を呼ぶ、前の feature が後ろの feature を
  呼ぶ）は `actions.js` を通す。中身は `app.js` が起動時に `bindActions` で 1 回だけ入れる。
- `features/` の中の順番は
  `display → navigation → files → theme → comments → units → comment-list → submit` で、
  自分より前の feature だけを直接 import してよい。後ろのものは `actions` を通す。
- `state.js` は `storage.js` と `model.js` を import してよい。ほかの leaf 同士は import しない。
- `app.js` と、`dom.js`（`document` を引く）・`state.js`（`localStorage` を読む）を除き、
  読み込んだだけで走る文（`addEventListener` や起動の呼び出し）は置かない。

## Commands

| Purpose | Command |
|---|---|
| Install（利用者） | `mise use -g github:ba0918/kemi`（リリース後） |
| Build | `cargo build --release` |
| Test | `cargo test` / `node --test web` |
| Lint | `cargo clippy -- -D warnings` / `cargo fmt --check` / `npx tsc -p web --noEmit` |
| Run locally | `cargo run -- --worktree` |
| Fixture | `scripts/gen-fixture.sh <dir> --files N --lines M [--commits K]` |
| Measure | `scripts/measure-startup.sh <fixture> <target/release/kemi>` / `scripts/measure-range.sh <fixture> <target/release/kemi> <commit\|file\|busy>` |
| Release plan | `dist plan`（cargo-dist をローカルに入れて実行） |

## Conventions specific to this project

- `docs/spec/kemi.md` が契約の正典。振る舞いを変えるときは該当節 ID（例 `R-SUBMIT`）を
  引用し、実装より先に仕様を直す。
- 用語は `CONTEXT.md` の読みに従う（例: コメントを「注釈」と呼ばない）。
- コードとコミットは日本語、UI 文言と docs も日本語、README は英語。
- フロントにビルド工程を入れない。型は JSDoc で書き、生成物をコミットしない。
- 新しい依存は `R-DEPS` のライセンス範囲に限り、C 依存（oniguruma 等）を持ち込まない
  （musl 静的リンクのため）。

## Constraints

- 性能: 10,000 ファイルの一覧が 1 秒未満 / 行データは表示時に計算 / 50 万行でも
  スクロール可 / 10,000 行 or 1 MB 超はハイライト off / digest は 30,000 ファイルで
  100 KB 未満。
- 配布: Linux x86_64・aarch64（musl 静的）と macOS x86_64・arm64。Windows は対象外。
- セキュリティ: 127.0.0.1 のみ、URL トークン、Origin / Host 検証。kemi は git の状態を
  書き換えない（suggestion の適用はエージェントが行う）。静的 HTML は書き出さない。
- セッションはメモリのみで、プロセス終了で消える。submit の結果は stdout の JSON で
  返し、同じ JSON を結果ファイル（`$XDG_STATE_HOME/kemi/results/`）にも残す
  （`R-RESULT`）。

## Glossary

正典は `CONTEXT.md`。特に kemi / レビュー / manifest / グループ / コメント /
suggestion / 適用 / quote / outdated / submit / verdict / digest / ノイズ /
focus / 重要 / 更新バッジ / グループ単位 / 表示モード / 変更ブロック / 由来 /
結果ファイル。
