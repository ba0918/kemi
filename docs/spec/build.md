# kemi 仕様: 構成と検証

[kemi 仕様](kemi.md) の一部。リポジトリの構成、依存、検証の出所。
用語の定義は `CONTEXT.md`、成功条件と反例の書き方は [kemi 仕様](kemi.md) の冒頭にある。

---

## R-WS 構成

```
Cargo.toml            # workspace + バイナリパッケージ kemi
rust-toolchain.toml   # Rust の版を固定
src/main.rs           # CLI 起動と配線、stdout/stderr、終了コード
crates/kemi-core/
  src/domain/         # 純粋なドメイン（差分整列、コメント、digest、ノイズ判定）
  src/source/         # git・manifest の読み取り（外部入力アダプタ）
crates/kemi-server/   # HTTP / SSE（axum）
crates/kemi-webview/  # HTML/CSS/JS 資産の埋め込み
web/
  package.json        # TypeScript（型チェックのみ）を固定
  tsconfig.json
  index.html
  assets/             # ビルド不要の ESM + CSS。app.js / features/ / views/ / leaf の
                      # 4 層（層の分け方は D11）
skills/
  kemi/SKILL.md       # エージェント向けの使い方（配布物。R-DIST）
scripts/
  gen-fixture.sh      # 検証用の合成リポジトリ生成
  measure-startup.sh  # 起動時間の計測
  measure-range.sh    # コミット範囲のグループ単位を作る時間の計測
```

- `kemi-core` の `domain` は純粋関数で、axum / hyper / tokio / `std::process` に
  依存しない（`source` モジュールはこの限りでない）。層は
  domain → source → server/cli の一方向。
- `kemi-server` は `kemi-core` に依存する。`kemi-webview` はどこにも依存しない。
  ルートのバイナリが 3 つを配線する。
- フロントはビルドなしのネイティブ ESM と CSS。型は JSDoc で書き、CI で
  `tsc --checkJs --noEmit` を通す（型チェックのみで生成物を出さない）。
  純粋なロジックは `node --test` でテストする。
- Node と Rust の版はリポジトリ内で固定する（`.mise.toml` / `rust-toolchain.toml`）。
- 資産は release ビルドでバイナリに埋め込み、開発時はディスクから配る。

**成功条件**

- `cargo build` だけで（Node 無しでも）バイナリができる。
- `rg 'use (axum|hyper|tokio|std::process)' crates/kemi-core/src/domain` が
  import 行に一致しない。
- `npx tsc -p web --noEmit` と `node --test web` が成功する。

**反例**

- ドメインが git コマンドや HTTP の型を直接参照する。
- フロントのビルドに Node が必須で、クリーンな `cargo build` が失敗する。

---

## R-DEPS 依存

調査済みの推奨スタックを使う（`D5`）。互換のため、同じ役割の別クレートに
差し替えても上の要件を満たすなら変更してよい。

- 差分整列: `similar`（単語単位の強調を含む）
- 構文ハイライト: `syntect` + `two-face`。`syntect` は `default-features = false` と
  `default-fancy` を使い、oniguruma（C 実装）に依存しない。計画時に
  `cargo tree -e features` で C 依存が無いことを確認する
- HTTP / SSE: `axum`
- 資産埋め込み: `rust-embed`
- ファイル監視: `notify`（debounce は固定の時刻で試せるよう自前で持つ）
- JSON: `serde` / `serde_json`
- 乱数（URL トークン）: `getrandom`（C 依存を持ち込まない）
- セッションに保存する内容の圧縮: `miniz_oxide`（pure Rust。C 依存を持ち込まない）
- 端末の選択画面: `inquire`（pure Rust）
- Markdown の構文木: `ox_content_ast` + `ox_content_parser` + `ox_content_allocator`
  （ox-content、MIT、pure Rust）。HTML はブロックごとの属性と語の印のために kemi が
  自分で書き、ox-content の描画器は使わない。計画時に `cargo deny check licenses` と
  `cargo tree` で依存の木を確認する。表（CSV / TSV）の分割は自前で持つ

許可する依存ライセンスは MIT / Apache-2.0 / BSD-3-Clause / ISC / Unicode-3.0 と
その互換（0BSD 等）に限る。

**成功条件**

- `cargo deny check licenses` が成功する。
- `cargo tree -e features` に oniguruma 系の C 依存が現れない。

**反例**

- GPL 系ライセンスのクレートが混入する。
- musl 静的リンクが C 依存で失敗する。

---

## R-VERIFY 検証

| 条件 | 出所 |
|---|---|
| Rust のユニット・統合テスト | `cargo test`（CI は ubuntu と windows。macOS は ubuntu で代表する） |
| 警告ゼロ | `cargo clippy --workspace --all-targets --locked -- -D warnings` / `cargo fmt --all --check` |
| 依存ライセンス | `cargo deny check licenses` |
| フロントの型 | `npx tsc -p web --noEmit` |
| フロントの純ロジック | `node --test web` |
| スキルの形式 | `agentskills validate ./skills/kemi`（skills-ref の公式バリデータ）と frontmatter の非 ASCII の検査。CI でも走らせる |
| スキルと契約の一致 | 人による確認（契約を変えるコミットのレビュー。`R-DIST`） |
| 規模の目標 | `scripts/gen-fixture.sh` と `scripts/measure-startup.sh` / `scripts/measure-range.sh` |
| マージや由来の検証 | `cargo test` の中で作る一時的な git リポジトリ（既存の git テストと同じ作り方） |
| セッションの保存と復元 | `cargo test`（一時ディレクトリを状態ディレクトリにして、保留・復元・掃除・権限・上限を確かめる） |
| 復元後の表示 | ブラウザ自動化（復元した URL を開き、開始時の内容が出ることと由来の劣化を確かめる） |
| 端末の選択画面 | 人による確認。`id` なしの一覧は `cargo test`（端末でない標準入出力でタブ区切りを確かめる） |
| 描画表示の変換 | `cargo test`（生 HTML の無効化、ブロックの印、オフセットから行への写像、front matter の切り離し、表の分割を純粋関数で確かめる）と、画像エンドポイントの統合テスト |
| 描画表示の画面 | ブラウザ自動化で `data-kemi-block` を数え、行レンジを照合する。復元した Markdown の描画表示と相対パス画像も同じ。描画側のコメントの操作感は人による確認 |
| 狭い画面 | ブラウザ自動化（幅 390px と 1280px を指定して開く）。実機のスマホでの見た目と操作は人による確認 |
| 表示と操作 | 人による確認。DOM 行数はブラウザ自動化（agent-browser 等）で `data-kemi-row` を数える |
| エージェントとの往復 | e2e（`kemi wait` / `kemi reply` を別プロセスで呼び、画面の操作はサーバの API で代える）と、画面はブラウザ自動化 |
| 動いているページ | 試験用の開発サーバ（ビルドの要らない静的ファイルと HMR 相当の通知を返す小さなサーバ）を相手に、ブラウザ自動化。操作感と見た目は人による確認（試作で詰める。[live.md](live.md)） |
| 配布 | リリースワークフローの成果物と、別環境での mise インストールと `gh skill install`。Windows は zip の `kemi.exe` が `--version` を出すことを Windows マシンを持つ人が確認する |

- `scripts/gen-fixture.sh <dir> --files N --lines M [--commits K] [--ignored-dirs D] [--changed C]` は、決定的な
  内容で git リポジトリを生成する（同じ引数なら同じ結果）。`--ignored-dirs` は git が無視するディレクトリを D 個作り、
  `--changed` は未コミットの変更を付ける追跡ファイルを先頭の C 個に絞る（省くと全部）。
- `scripts/measure-startup.sh <fixture> <kemi-bin>` は、release ビルドの kemi を
  `--port 0 --no-open` で起動し、stderr の `kemi: <url>` 行を待って
  `api/review` を取得するまでの時間を計測し、3 回の中央値を表示する。
- `scripts/measure-range.sh <fixture> <kemi-bin> <commit|file|busy>` は、release
  ビルドの kemi をフィクスチャの最初のコミットからの `--from` で起動して計測する。
  `commit` / `file` は `--group-by <commit|file>` で起動し、`api/review` を取得する
  までの時間の 3 回の中央値を表示する。`commit` の時間を、コミットごとの単位を作る
  時間とみなす（同じ処理のため。`R-UNIT`）。`busy` は既定（最終形）で起動し、
  `api/review` の直後から、裏でコミットごとの単位を作っている間に最初のファイルの
  `api/file` を 10 回取得し、最も遅い応答時間を表示する（`R-UNIT` の 100 ms）。
  このスクリプトで使う「範囲の標準フィクスチャ」は
  `scripts/gen-fixture.sh <dir> --files 100 --lines 20 --commits 201`（最初のコミットの
  後に 200 コミット）。
- 時間の上限（1 秒、および `R-UNIT` の 100 ms）は開発機でのスモーク基準とする。
  計測の対象は `scripts/gen-fixture.sh` が作るフィクスチャで、追加主体の worktree は
  対象外（許容する）。CI では余裕を持った閾値にし、マシン差を理由に失敗し
  続けないようにする。

- ライブ改訂の後も、上の時間の上限は変えない。往復の機能（状態の表示、渡す、返信）は
  全モードで読み込まれるが、この上限を守る。`--live` のコードの見方も `R-SERVE` の 1 秒を守る。

**成功条件**: 上のコマンドがすべて成功する。`--live` でないレビューのページで、ページ用の
ファイル（中継・スナップショット・案）が読み込まれない（ブラウザ自動化で読み込んだ URL を数える）。

**反例**: 目標の規模で 1 秒を大きく超える、またはテストが環境依存で
恒常的に不安定。

---

## 委譲

- D5 クレート選定（`R-DEPS` の範囲内）。
- D11 フロントの層の分け方（`web/assets/` の 4 層と依存の向き）。規則の置き場は
  `PROJECT.md`。どの分け方でも画面の振る舞いは変わらず、`R-WS` の成功条件（`tsc` と
  `node --test`）で検査する。
