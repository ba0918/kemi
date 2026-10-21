# Project Context

## What this is

kemi は、変更をブラウザで読むためのローカルなレビュー道具。単一バイナリを
GitHub Releases で配り、`mise use -g github:ba0918/kemi` でインストールする。
人間がページで行コメントと suggestion を付け、「承認」か「変更要求」でレビューを
終えると、コメントが 1 つの JSON として実行ターミナル（通常はレビューを頼んだ
エージェント）へ返る。元データは manifest / コミット範囲 / worktree / staged /
`--live`（動いているページと作業ツリー）の 5 モードで与える。名前は「閲する（けみする）」から。

## Stack and layout

Rust の workspace。ドメインは純粋関数、HTTP は axum、フロントはビルドなしの
ネイティブ ESM + CSS で、release ビルドではバイナリに埋め込む。

| Path | 内容 |
|---|---|
| `docs/README.md` | 文書の入口。各ディレクトリの役割といまの文書 |
| `docs/spec/kemi.md` | 承認済みの本体の仕様の索引。契約（CLI、JSON、性能条件）の正典は、そこから辿る責務ごとのファイル |
| `CONTEXT.md` | 用語集。二通りに読める語の読みの正典 |
| `src/main.rs` | CLI の引数の振り分けと、確かめる順。配線は `src/` の `input.rs`（入力からソース）・`review.rs`（起動・サーブ・終わり方）・`resume.rs`（復元）・`results.rs`（結果ファイル） |
| `src/report.rs` | stderr の契約の行、stdout への書き込み、終了コード |
| `crates/kemi-core/src/domain/` | 純粋なドメイン（整列、コメント、digest、ノイズ判定） |
| `crates/kemi-core/src/source/` | git / manifest の読み取りアダプタ |
| `crates/kemi-server/` | HTTP と SSE（axum） |
| `crates/kemi-webview/` | 資産の埋め込み |
| `web/` | ESM と CSS のソース。`tsc --checkJs` は型検査のみ（層は下の節） |
| `skills/kemi/SKILL.md` | エージェント向けの kemi の使い方。仕様から派生した配布物で、契約の正典ではない |
| `scripts/` | フィクスチャ生成、起動時間の計測、ブラウザ自動化（`test-*.mjs`）、`--live` の検査に使う試験用の開発サーバ（`live-dev-server.mjs`） |
| `scripts/lib/` | ブラウザ検査の共通の部品（agent-browser の呼び出し、kemi の起動と止め方と API、検査が落ちたときの後始末、待ち、フィクスチャ、PNG） |
| `scripts/live/` | `scripts/test-live.mjs` の場面。領域ごとのファイルと、2 つ以上の領域の場面が使う補助（`common.mjs`） |

### `web/assets/` の層

ページの JS は 4 層で、依存は上から下の一方向だけ。

| 層 | 中身 |
|---|---|
| `app.js` | 結線と起動だけ。すべてを import してよい唯一のモジュール |
| `features/` | 状態を変え、`api.js` を呼び、描き直す。`features/explanation/` は `explanation` の一部（親の `explanation.js` が import し、親を呼ぶときは `actions` を通す） |
| `views/` | 要素を作る。`api.js` を除く leaf と、ほかの `views/` を import してよい（循環させない）。`api.js`・`features/`・`app.js` は import しない。`views/diagrams/` は図の view で、`diagrams/` を使う |
| leaf | `model.js`（純粋）、`conversation-model.js`（会話パネルとエージェントとの往復の純粋な読み）、`api.js`（通信）、`dom.js`（要素と道具）、`state.js`（状態と派生の読み）、`storage.js`（localStorage）、`actions.js`（下から上を呼ぶ入れ物）、`live-model.js`（`--live` のページの見方の純粋な読みと、書いているページへのコメントの場所の扱い）、`live-diff.js`（`--live` の差分の純粋な処理。要素の記述を比べて変化の一覧を作る）、`explanation-model.js`（説明ビューの純粋な読み）、`diagrams/`（shoryo から取り込んだ図の描画の束。DOM を使う。上流とそろえるため、中身を kemi の層に合わせて書き換えない） |

- 下から上への呼び出し（view のボタンが feature を呼ぶ、前の feature が後ろの feature を
  呼ぶ）は `actions.js` を通す。中身は `app.js` が起動時に `bindActions` で 1 回だけ入れる。
- `features/` の中の順番は
   `horizontal-scroll → display → navigation → files → rendered → parts → theme → comments → agent → units → conversation → narrow → submit → explanation → live` で、
  自分より前の feature だけを直接 import してよい。後ろのものは `actions` を通す。
- `features/live/` は `live` の部品で、親の `features/live.js`（起動と結線、見方・側・表示幅・ページの切り替え、
  ページからの知らせの振り分け、帯と比べる相手の描画）が import する。部品の順は
  `state → channel → stage → snapshots → mock → changes → compose` で（state は共有の状態、channel はページとの
  往復、stage は枠の置き方、changes は変化の一覧と印、compose はページへのコメントを書く）、部品は自分より前の
  部品だけを import し、親は import しない。
  `features/live/state.js` は画面を開いている間の状態（`live`）と骨組みの要素（`shell`）を持ち、ほかの部品と
  親が読み書きする。部品から親の関数（描き直し、見方・側・表示幅・ページの切り替え）を呼ぶときは
  `state.js` の `liveActions` を通す。中身は親が `startLive` の最初に `bindLiveActions` で 1 回だけ入れる。
- `state.js` は `storage.js` と `model.js` を import してよい。`diagrams/` の中のファイルは互いに import してよい。
  ほかの leaf 同士は import しない。
- `app.js` と、`dom.js`（`document` を引く）・`state.js`（`localStorage` を読む）を除き、
  読み込んだだけで走る文（`addEventListener` や起動の呼び出し）は置かない。
- `web/index.html` は全モジュールを `modulepreload` で先に取りに行かせる（ビルド無しの
  ESM は依存を 1 段ずつ発見するため。実測で読み込み完了が約 2 割短くなった）。
  モジュールを足したり改名したりしたら、その一覧も直す。ズレても壊れず、ブラウザの
  コンソールに警告が出るだけ。
- 上の規則（features の順番と `features/<name>/` の下から親を import しないこと、`features/live/` の部品の順、
  views が import しないもの、leaf の許可表）と `modulepreload` の一覧は `web/assets/layers.test.js`
  （`node --test web`）が import 文から確かめる。features の順番と部品の順は上の行をそのまま読む。
  JSDoc の `import("…")` は型だけの参照で読み込みを生まないので数えない。動的な `import(…)` は、下の例外
  （`app.js` が `--live` のモジュールを読む所）だけであることも確かめる。`index.html` の stylesheet の順（部品の `parts.css`・`explanation.css` の後に
  `style.css`）も確かめる。詳細度の同じ規則は後に読んだ方が勝つので、この順が見た目を決める。
  読み込んだだけで走る文の規則は確かめない。
- 例外は `--live` のページの見方（`features/live.js` と `features/live/`・`views/live.js`（骨組み・上部バー・書く欄）・
  `views/live-changes.js`（ページのツリーと変化の一覧）・`live-model.js`・`live-diff.js`・
  `live.css`）。`--live` でないレビューはページ用のファイルを読み込まない約束（`R-VERIFY`）
  なので、`modulepreload` に載せず、`app.js` が `--live` のレビューでだけ動的に `import` する。
  ファイルを分けるときもパスに `live` を含める（`--live` でないレビューで読み込んだ URL を、パスの
  `live` で確かめているため）。
- `web/live/page.js` は、中継したページに差し込むスクリプト。レビュー画面の層の外で、開発中の
  ページの中で動き、レビュー画面とは `postMessage` だけで話す。中継のポートの `/__kemi/page.js`
  で配る。要素の記述・スナップショット・変化の印に加え、ページへのコメントの場所（指している要素と
  文書の座標）を決め、場所の描き込みと、その周りの画像を作る。押す・描く操作そのものはレビュー
  画面が枠の上に重ねた層（`features/live/compose.js`）で受ける。
- `--live` のページへのコメントの印とページのツリーの数は、`views/conversation.js` が会話を描くたびに
  `actions.pageCommentsChanged` で合わせる。`--live` でないレビューでは何もしない。

## Commands

| Purpose | Command |
|---|---|
| Install（利用者） | `mise use -g github:ba0918/kemi`（リリース後） |
| Build | `cargo build --release` |
| Test | `cargo test` / `node --test web` |
| スキルの context 読み取り補助（Python 3.9+、標準ライブラリのみ） | `python3 -m unittest discover -s tests -p 'test_context_summary.py'` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` / `cargo fmt --all --check` / `npx tsc -p web --noEmit` |
| スキルの形式 | `scripts/check-skill-format.sh`（uv が要る。skills-ref はハッシュ付きの `scripts/skills-ref-requirements.txt` から入れる） |
| スキルの frontmatter（ASCII だけ・項目は 3 つ） | `scripts/check-skill-frontmatter.sh` |
| ドメインの純粋さ | `scripts/check-domain-purity.sh` |
| Run locally | `cargo run -- --worktree` |
| Fixture | `scripts/gen-fixture.sh <dir> --files N --lines M [--commits K] [--ignored-dirs D] [--changed C]`（`--ignored-dirs` は git が無視するディレクトリの数、`--changed` は未コミットの変更を付けるファイルの数で既定は全部） |
| Measure | `scripts/measure-startup.sh <fixture> <target/release/kemi>` / `scripts/measure-range.sh <fixture> <target/release/kemi> <commit\|file\|busy>` |
| Browser（実バイナリ） | `node scripts/test-rendered-view.mjs <target/release/kemi>` / `node scripts/test-narrow-screen.mjs <target/release/kemi>` / `node scripts/test-agent-channel.mjs <target/release/kemi>`（agent-browser をローカルに入れて実行） |
| Browser（コメント札のレイアウト） | `node scripts/test-comment-chip-layout.mjs <target/release/kemi>`（agent-browser が要る） |
| Browser（偽サーバ） | `node scripts/test-horizontal-scroll.mjs`（agent-browser が要る。`web/` は自前の HTTP サーバから配るので引数は無い） |
| Browser（`--live`） | `node scripts/test-live.mjs <target/release/kemi> [<場面>...]`（agent-browser が要る。場面は `scripts/live/` の場面の関数の名前で、並べた順に走る。省くと全部。`<場面>:<値>` は値を受け取る場面にだけ渡せる） |
| Browser（説明の見方） | `node scripts/test-explanation-view.mjs <target/release/kemi> [--parts-only \| --navigation \| --live-navigation \| --range-navigation]`（どれか 1 つで、その節だけ: 復元した会話の parts、説明の見方、`--live` の説明、コミット範囲の参照） / `node scripts/test-explanation-selection.mjs <target/release/kemi>`（agent-browser が要る） |
| Release plan | `dist plan`（cargo-dist をローカルに入れて実行） |

## Conventions specific to this project

- `docs/spec/` の仕様が契約の正典（本体の入口は `kemi.md` の索引）。振る舞いを変えるときは該当節 ID（例 `R-SUBMIT`）を
  引用し、実装より先に仕様を直す。
- 用語は `CONTEXT.md` の読みに従う（例: コメントを「注釈」と呼ばない）。
- コードとコミットは日本語、UI の文言と CLI の出力は英語、docs は日本語、`README.md` と `skills/` は英語、
  `README-ja.md` は日本語。両 README は構成と内容をそろえ、冒頭に相互リンクを置く。
- CLI と JSON の契約を変えるコミットで、`skills/kemi/SKILL.md` と README も同時に直す。
  一致を確かめる機械検査は無いので、レビューで見る。
- 利用者に届けたいスキルの直しは、文面だけの変更でもリリース（`v<semver>` のタグ）で出す。
  `gh skill` が配るのはタグの内容で、ブランチへのコミットだけでは利用者に届かない。
- フロントにビルド工程を入れない。型は JSDoc で書き、生成物をコミットしない。
- ルール `ba0918-gui-structure` は適用しない（`AGENTS.md` のルーティング表から外している）。
  あの規約は、画面の頂点の部品だけが状態を持ち、各部品へ表示用データを渡して描かせる部品の木を
  前提にしている。`web/assets/` は共有の `state.js` を各層が直接読み、`features/` が状態を変えて
  描き直す構成（上の「`web/assets/` の層」）で、前提が合わない。ルーティング表を作り直すときも
  この行は足さない。
- 新しい依存は `R-DEPS` のライセンス範囲に限り、C 依存（oniguruma 等）を持ち込まない
  （musl 静的リンクのため）。

## Constraints

- 性能: 10,000 ファイルの一覧が 1 秒未満 / 行データは表示時に計算 / 50 万行でも
  スクロール可 / 10,000 行 or 1 MB 超はハイライト off / digest は 30,000 ファイルで
  100 KB 未満。
- 配布: Linux x86_64・aarch64（musl 静的）、macOS x86_64・arm64、Windows
  x86_64・arm64（msvc）。
- セキュリティ: 既定では 127.0.0.1 にだけバインドし、`--bind` で明示したときだけ
  LAN に公開する（公開時は警告を出す）。URL トークン、Origin / Host 検証。kemi は
  git の状態を書き換えない（suggestion の適用はエージェントが行う）。静的 HTML は
  書き出さない。
- セッション（未 submit のレビュー）は結果ファイルと同じ根の `sessions/` に保存し、
  `kemi --resume` で当時の差分のまま復元できる（`R-SESSION`）。submit の結果は
  stdout の JSON で返し、同じ JSON を結果ファイル（unix は
  `$XDG_STATE_HOME/kemi/results/`、Windows は `%LOCALAPPDATA%\kemi\results\`。
  `R-RESULT`）にも残す
- 描画表示（`R-RENDER`）: Markdown / CSV / TSV は片側 10,000 行 or 1 MB 超で描画しない。
  画像は片側 5 MB 超で並べない。描画は表示時に計算し、起動時には行わない
- 狭い画面（幅 720px 未満、`R-NARROW`）で変わる規則は 1 つのメディアクエリ（CSS と
  `matchMedia`）に集め、広い画面で見えている要素と操作は変えない

## Glossary

正典は `CONTEXT.md`。特に kemi / レビュー / manifest / グループ / コメント /
suggestion / 適用 / quote / outdated / submit / verdict / digest / ノイズ /
focus / 重要 / 更新バッジ / グループ単位 / 表示モード / 変更ブロック / 由来 /
結果ファイル / セッション / 保留 / 復元 / 描画表示 / ソース表示 / ブロック / 狭い画面。
