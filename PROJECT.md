# Project Context

## What this is

kemi は、変更をブラウザで読むためのローカルなレビュー道具。単一バイナリを
GitHub Releases で配り、`mise use -g github:ba0918/kemi` でインストールする。
人間がページで行コメントと suggestion を付け、「承認」か「変更要求」でレビューを
終えると、注釈が 1 つの JSON として実行ターミナル（通常はレビューを頼んだ
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
| `web/` | ESM と CSS のソース。`tsc --checkJs` は型検査のみ |
| `scripts/` | フィクスチャ生成と起動時間の計測 |

## Commands

| Purpose | Command |
|---|---|
| Install（利用者） | `mise use -g github:ba0918/kemi`（リリース後） |
| Build | `cargo build --release` |
| Test | `cargo test` / `node --test web` |
| Lint | `cargo clippy -- -D warnings` / `cargo fmt --check` / `npx tsc -p web --noEmit` |
| Run locally | `cargo run -- --worktree` |
| Fixture | `scripts/gen-fixture.sh <dir> --files N --lines M [--commits K]` |
| Measure | `scripts/measure-startup.sh <fixture> <target/release/kemi>` |
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
- セッションはメモリのみで、プロセス終了で消える。submit の stdout JSON が唯一の出口。

## Glossary

正典は `CONTEXT.md`。特に kemi / レビュー / manifest / グループ / コメント /
suggestion / 適用 / quote / outdated / submit / verdict / digest / ノイズ /
focus / 重要 / 更新バッジ。
