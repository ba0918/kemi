# 描画表示（R-RENDER）を実装する

## Goal

Markdown と CSV / TSV をファイルヘッダの切り替えで描画した見た目のまま差分として読め、
描画したブロックからそのままコメントを付けられ、画像は旧と新を並べて比べられる。
起動時間と表示中の応答は今と同じ速さのまま。

## Specification

`docs/spec/kemi.md`（描画表示の改訂 `1170a0a`、画像の判定と枠の層の修正 `56400a0`）。
見出しは `R-RENDER 描画表示`、`R-VIEW 差分の表示`、`R-NAV 変更間の移動`、
`R-COMMENT コメントと suggestion`、`R-SERVE 配信モデル`、`R-SESSION セッションの保存と復元`、
`R-LIVE ライブリロード`、`R-INPUT-3 未コミット（worktree）`、`R-UNIT グループ単位の切り替え`、
`R-DEPS 依存`、`R-DIST 名称と配布`、`R-VERIFY 検証`、`作らないもの`、`委譲`。用語は
`CONTEXT.md` の「描画表示 / ソース表示」「ブロック」「変更ブロック」「表示モード」。

## Approach and why

- **描画とブロックの計算は kemi-core の純粋なモジュールに置き、HTML は kemi 自身が
  ブロック単位で組み立てる。** ox-content は Markdown の構文木（`ox_content_parser::parse`
  が返す `ox_content_ast::Node`。各ノードが元テキストのバイト範囲 `Span` を持つ）と
  ノード単位の HTML 描画（`ox_content_renderer` の `HtmlRenderHooks` / `render_nodes`）を
  提供する（計画時に ox-content v3.2.8 のソースで確認済み。`ox_content_ast` は
  `ox_content_parser` から再輸出されないので、依存として直接足す。同じ ox-content の
  一族で、`R-DEPS` の D5 の範囲内）。ブロックの行レンジ、
  変更の印、重ねるか上下に並べるかの判定は仕様が kemi の契約として決めたものなので、
  描画器の出す属性（`data-source-span`）には依存せず、構文木の `Span` から kemi が
  計算する。エスケープした生 HTML の塊もブロックにする必要があり、描画器はそこに
  属性を出さないので、この方針でしか契約を満たせない。
- **旧側と新側の両方を構文木にし、行の整列で対応づける。** 差分の元は既存の
  `domain/diff.rs` の `align`（行）と `diff_segments`（語）で、ソース表示と同じ整列を
  使う。新側のブロック列を骨格にし、消した範囲と重ねられない書き換えの旧のブロックを
  整列で決まる位置に差し込む。削除ファイルは旧側の構文木だけを描画する。
- **重ねたブロックのインラインは kemi が描画する。** 描画器は消した語と足した語の印を
  出せないので、文のブロックのインライン子ノードを「書式付きのテキストの並び」に
  ほどき、テキスト全体を `diff_segments` で語単位に比べ、印を書式の中に入れて HTML に
  する。旧と新で書式の種類と順番が同じときだけ重ねる、という仕様の条件が、この
  ほどき方を成立させている。
- **front matter は構文木にかける前に切り離し、行レンジに行数を足し戻す。** 切り離した
  後のテキストで得た `Span` は切り離し後のオフセットなので、front matter の行数だけ
  ずらしてから行に写す。
- **コード塊のハイライトは関数の注入で受け取る。** syntect は `kemi-server` の
  `highlight.rs` にあり、既存の `Highlighter::highlight` はパスの拡張子から構文を引いて
  スタイルの列を返し、`render_line` が HTML にする。core の描画は「言語名と本文を受けて
  行ごとの HTML を返す」関数を引数に取り、サーバ側の `highlight.rs` に、フェンスの
  言語名で構文を引いて `render_line` まで通すアダプタを足して渡す。ファイルの
  ハイライト設定（有効・無効、`dark`）に従うだけで、別の切り替えは持たない。
- **相対パス画像の参照も core は純粋に扱う。** core は、その側のレビュー対象ファイルの
  パスの一覧と「リポジトリを読めるか」（git の入力モードなら真、manifest と復元なら偽）を
  引数に受け、描画の時点で分かる枠（リポジトリの外に出るパス、画像でない拡張子、
  レビュー対象に一致せずリポジトリも読めないもの）は `src` を出さずに枠にし、レビュー
  対象に一致するものは「そのファイルと側」の参照、リポジトリを読めるものは「正規化した
  パスと側」の参照としてブロックに残す。参照を URL にするのはサーバで、前者は
  レビュー対象画像のエンドポイント（復元でも写しから出る）、後者は `ReviewSource` に
  足す「側と版を指定してリポジトリのファイルを読む」メソッド（既定 `None`。git の
  ソースだけが入力モードごとの版で実装する）に向ける。symlink・上限超え・存在しない
  ファイルはそのエンドポイントが 404 を返し、ページが同じ枠に差し替える。これで
  仕様の表（manifest は出さない、復元は写しにあるものだけ）と枠の 2 段がそのまま
  実装に写る。
- **描画は新しいエンドポイントで、表示時に計算する。** 既存の `api/file` は行データの
  契約のまま触らず、描画表示の HTML とブロックの一覧は別の `GET` で返す（URL の形は
  D7）。起動と `api/review` では計算しない（`R-SERVE`）。事前に判定できる描画不可の
  うち行数とバイト数は、`api/file` が既に持つメタデータ（行数とサイズ）だけで決めて
  内容を余分に読まない。表の `"` の対応は `api/file` が行データを作るときに一緒に見る。
- **表（CSV / TSV）は依存を足さず、core の小さな分割関数で行を表の行にする。** 行 =
  表の行なので、行の整列がそのままブロックの整列になる。
- **レビュー対象の画像は kemi が読んだバイト列（`ReviewSource::content`）をそのまま返す。**
  入力モードを問わず、復元でも写しから動く。写しの上限（20 MB）で復元できなくなる
  セッションが増えうるのは `R-SESSION` の既存の契約のままで、この計画では変えない。
- **フロントは新しい feature と view を足し、既存の行の描画には手を入れない。** 描画
  表示はファイル単位の別の表示で、行の仮想化と折りたたみを通らない。feature は
  `features/` の順番で `files` の後、`comments` の前に置く（`api.js` を呼び、状態を変える
  が、コメントの入口は `actions` 経由で後ろの `comments` を呼ぶ）。ファイルごとの
  切り替えは `state.js` の「ページを開いている間だけの状態」に置き、`storage.js` にも
  サーバのセッション状態にも書かない。
- **README は同じ流れで直し、スキルは触らない。** `R-DIST` がスキルの同時更新を求めるのは
  CLI と JSON の契約を変えるときで、この計画はどちらも変えない。

## 速度の条件

利用者の条件として、速度を落とさないことを絶対とする。仕様の数値（`R-SERVE` の起動
1 秒、`R-UNIT` の `busy` 100 ms）に加えて、この計画では次を守る。

- 描画は表示時にだけ計算し、起動・`api/review`・`api/file` の経路で描画器を呼ばない。
  `api/file` に載せる描画不可の事前判定は、既に持っている行数とサイズだけで決める。
- 上限付近（片側 10,000 行）の Markdown で描画エンドポイントの応答が 500 ms を超えたら、
  次の Step に進む前にチューニングする。この 500 ms はこの計画の停止条件で、仕様の
  契約ではない（仕様の数値は起動 1 秒と `busy` 100 ms だけ）。チューニングの例は、
  構文木を 1 回だけ作る、語の差分を文のブロックだけにかける、HTML の組み立てで文字列の
  再割り当てを避ける、など。チューニングしても超えるなら止めて相談する。
- `scripts/measure-startup.sh` と `scripts/measure-range.sh <fixture> <bin> file` /
  `busy` を実装の前後で同じフィクスチャで測り、後の値が前の値を上回らない（測定の
  ばらつきの範囲を超えて悪化しない）ことを確かめる。実装前の値は Step 1 で、依存を
  足す前の release ビルドで測って記録する。実装後は Step 9 で測る。release ビルドは
  `web/` を埋め込むので、Step 9 の計測前に release を作り直す。

## Scope of change

- `crates/kemi-core/src/` の新しい描画モジュール（Markdown の構文木からブロックへ、
  front matter、URL の判定、重ねる判定、表の分割、画像の判定、相対パスの正規化）と
  そのテスト
- `crates/kemi-core/src/source/mod.rs`（`ReviewSource` の新しいメソッド）、
  `crates/kemi-core/src/source/git.rs`（相対パス画像の読み取り）とそのテスト
- `crates/kemi-core/Cargo.toml`（`ox_content_ast`、`ox_content_parser`、
  `ox_content_renderer`）、`Cargo.lock`
- `crates/kemi-server/src/api.rs`（描画と画像のエンドポイント、`api/file` の描画表示の
  情報）、`crates/kemi-server/src/highlight.rs`（言語名で引くアダプタ）、
  `crates/kemi-server/src/lib.rs`、`crates/kemi-server/tests/server.rs`
- `web/assets/` の新しい feature と view、`api.js`（描画と画像の取得）、`state.js`、
  `model.js`、`model.test.js`、`actions.js`、`app.js`、`views/file-header.js`、
  `views/nav.js`、`features/display.js`（止まる場所の反映）、`features/navigation.js`、
  `features/comments.js`（ブロックからの入口）、`style.css`、`web/index.html`
  （`modulepreload` の一覧）
- `scripts/` の新しいブラウザ自動化スクリプト
- `PROJECT.md`（`features/` の順番の行）、`README.md`

これ以外は触らない。`skills/`、`src/`（CLI）、`crates/kemi-core/src/session/`、
`deny.toml` は変更しない。`api/file` の JSON の既存フィールドは変えない（この計画の
取り決め。フロントの既存の経路を壊さないため）。

## Step order and prerequisites

Step 1 → Step 2 → Step 3 → Step 4 → Step 5 → Step 6 → Step 7 → Step 8 → Step 9 → Step 10。
Step 6（表）は Step 2 の描画モジュールと Step 5 のページに依存する。Step 7（画像）は
Step 3 のエンドポイントの作りと Step 5 のページに依存する。Step 6 と Step 7 は互いに
独立で入れ替えてよい。Step 8 は Step 7 の画像エンドポイントに依存する。Step 9 は
Step 6〜8 がそろってから。Step 5 まででも Markdown の描画表示として使える形にする
ため、画像を後ろに置く。

## Verification map

| 仕様の見出し | 確かめる Step |
|---|---|
| `R-RENDER 描画表示` | Step 2〜9（対象、Markdown、変更の見せ方、表、画像、コメント、移動、上限と失敗、ライブリロードと復元）。押してからの描画の失敗の画面はサイクル終了時の人による確認 |
| `R-VIEW 差分の表示` | Step 5（ヘッダの切り替え。取得中の無効は既存の仕組みのまま）、Step 7（バイナリの例外） |
| `R-NAV 変更間の移動` | Step 4（止まる場所の純粋関数）、Step 5（帯と `n` / `p`）。画像を飛ばすのは既存の規則のまま |
| `R-COMMENT コメントと suggestion` | Step 5（ブロックからの行コメントが submit JSON に入る、side、suggestion） |
| `R-SERVE 配信モデル` | Step 3（表示時に計算、token 配下）、Step 7 と Step 8（画像の応答ヘッダ） |
| `R-SESSION セッションの保存と復元` | Step 9（復元した Markdown の描画表示、写しからの画像、切り替えがソースに戻る） |
| `R-LIVE ライブリロード` | Step 5（読み込み直しで描画表示を保つ・戻す） |
| `R-INPUT-3 未コミット（worktree）` | Step 7（untracked の上限超え）、Step 8（作業ツリーの版） |
| `R-UNIT グループ単位の切り替え` | Step 9（`busy` の 100 ms を落とさないことの確認。単位の振る舞いは変えない） |
| `R-DEPS 依存` | Step 1（`cargo deny check licenses`、`cargo tree`。計画時にも通した: licenses ok、C 依存なし） |
| `R-DIST 名称と配布` | Step 10（README。UI の文言は英語） |
| `R-VERIFY 検証` | 各 Step の Shown by。Rust を触る Step は `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --check` も通す |

仕様の「描画側のコメントの操作感は人が確認する」は、Step の中ではなく、サイクルの
終わりに利用者が実際のリポジトリの Markdown で確かめる受け入れとする。

## Left to the implementer

- 新しいモジュール・型・関数・feature・view の名前と内部構造。
- 描画エンドポイントと画像エンドポイントの URL の形と JSON の形（D7）。
- `data-kemi-block` 以外の HTML の構造、クラス名、消した語と足した語の印の表し方。
  CSS の値（色は既存の役割の変数を使う）。
- 描画結果のキャッシュの有無（D16）。
- 切り替えのキーの割り当て（D15。既存のキーと衝突しないもの。既存のキーは README の
  "Keys in the page" の表にある）。
- ハイライトを注入する関数の形（トレイトかクロージャか）。
- 相対パス画像を読むメソッドの名前と引数の形（側と版の渡し方）、パス正規化の実装。
- 構文木のインライン子ノードを「書式付きのテキストの並び」にほどく内部表現。
- ツールチップとヘッダに出す描画不可の理由の文言（契約でない。英語）。

## Stop conditions

- `cargo deny check licenses` が失敗する、または `cargo tree` に C 依存が現れるとき
  （計画時は通ったが、版が動いたときに備える）。
- ox-content の構文木が、仕様がブロックに数える種類（表の行、脚注の定義、リスト項目、
  生 HTML、水平線）のどれかに `Span` を持たないと分かったとき。
- `HtmlRenderHooks` でコード塊の描画を差し替えられない、またはインラインの子ノードを
  再描画できる形で取り出せないと分かったとき。近似で済ませず相談する。
- 「速度の条件」を、チューニングしても満たせないとき。
- 作業ツリーやインデックスの読み取りで symlink を辿らずに読む方法が、対象 OS で揃わない
  とき。
- `agent-browser` がこの環境で使えないとき（止めて相談する。人による確認への切り替えは
  相談の上で決め、切り替えた条件は UNVERIFIED として記録する）。
- サーバや e2e のテストのヘルパの変更が、この計画のファイル範囲の外へ広がるとき。
- 既存の `api/file` の JSON の既存フィールドを変えないと描画表示の情報を載せられない
  とき。

## Out of scope

- バージョンと `CHANGELOG.md`（リリース時にまとめる）。
- `skills/kemi/` の変更（契約が変わらない）。
- `.rst` / AsciiDoc / GeoJSON、画像のスワイプと重ね合わせ、`.mdx` / front matter の中身 /
  数式の解釈、表のフィールド内の改行、外部画像のプロキシ、複数ブロックの範囲選択
  （`作らないもの` P15〜P21）。
- 由来の行を描画表示に出すこと。
- 描画結果をセッションに保存すること（描画は表示時に計算する）。
- 写しの上限の変更。

---

## Step 1 — ox-content を依存に足す

Purpose: Markdown の構文木と描画器を kemi-core から使えるようにする。Specification:
`docs/spec/kemi.md#R-DEPS 依存`。
Prerequisites: なし。
May change: `crates/kemi-core/Cargo.toml`、`Cargo.lock`。
Done when: 依存を足す前の起動と応答の計測値が記録され、`ox_content_ast`、
`ox_content_parser`、`ox_content_renderer`（いずれも 3.2.x）が kemi-core の依存にあり、
ライセンスの検査が通り、依存の木に C 依存が無く、ビルドが通る。
Shown by: check — 順に、依存を足す前に `cargo build --release` と
`scripts/gen-fixture.sh` の標準フィクスチャで `scripts/measure-startup.sh` と
`scripts/measure-range.sh <fixture> <bin> file` / `busy` を測って値を記録し、依存を足して
`cargo deny check licenses`、`cargo tree -p kemi-core -e features`（oniguruma 系と
`cc` / `libc` を使う crate が現れない）、`cargo build`。
Left to the implementer: なし。
Stop and hand back if: どれかのコマンドが失敗するとき。

## Step 2 — Markdown から変更の印の付いたブロック列を作る純粋関数

Purpose: 旧と新の Markdown（またはどちらか片方）を受け取り、仕様の変更の見せ方に従う
ブロック列（HTML、side、行レンジ、印、画像の参照）を返す純粋関数を作る。
Specification: `docs/spec/kemi.md#R-RENDER 描画表示`（対象と切り替え、Markdown の描画、
変更の見せ方、上限と失敗）、`CONTEXT.md#ブロック`。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/` の新しい描画モジュール（`lib.rs` への登録を含む）と
そのテスト。
Done when: 次がそのまま観察できる。
- 対象の判定は拡張子（大小文字を区別しない）で、改名は新側のパス、削除は旧側のパス。
- GFM の表・取り消し線・タスクリスト・自動リンク・脚注が描画される。front matter は
  切り離されて無地の塊のブロックになり（閉じが無ければ front matter でない）、生 HTML は
  エスケープされた塊のブロックになる。
- `href` / `src` は前後の空白と制御文字を除いてから判定され、大小文字を区別しない
  `http` / `https`、`//` で始まらない相対パス、`#` 始まりだけが通る。外部リンクは
  新しいタブと `rel="noopener noreferrer"` を持ち、相対リンクは `<a>` でなくテキストで
  `title` にパスを持つ。外部 URL の画像は `src` がそのまま出る。
- ブロックは段落・見出し・コード塊・表の行・水平線・脚注の定義・front matter の塊・
  生 HTML の塊・中に段落を持たないリスト項目で、リスト・引用・段落を持つリスト項目は
  入れ物、表は行が単位。すべてのブロックが `data-kemi-block`（`<side>:<start>-<end>`）を
  持つ。
- 新側の文書が骨格で、消した範囲と重ねられない書き換えの旧のブロックが整列で決まる
  位置に旧側のレンジで差し込まれる。削除ファイルは旧側の文書全体で全ブロックが削除の
  印、追加だけのファイルは全ブロックが追加の印。
- 構造（ブロックの種類とインラインの種類・順番）が同じ文のブロックの書き換えは 1 つの
  ブロックに重なり、消した語と足した語の両方の印を持ち、語の印は旧新のテキスト全体の
  語差分で、レンジは新側。構造が違うもの、コード塊、表の行は旧新 2 ブロックで上下。
- コード塊は注入されたハイライト関数を言語名で呼ぶ（関数が無ければ無地）。
- 相対パス画像は Markdown のディレクトリ基準（先頭 `/` は根）で正規化され、リポジトリの
  外に出るもの、画像でない拡張子、レビュー対象に一致せずリポジトリを読めない入力
  （manifest と復元）のものは `src` の無い枠、その側のレビュー対象ファイルに一致する
  ものは「そのファイルと側」の参照、それ以外は「正規化したパスと側」の参照として
  ブロックに残る。
- 片側が 10,000 行超または 1 MB 超の入力は描画不可の結果になる。
Shown by: test — `cargo test -p kemi-core` に、上の各項目を 1 つずつ確かめるテストを
足す: 生 HTML と `<img onerror>` / `<iframe>` が要素や属性として出ない、前後に空白の
付いた `javascript:` と `data:` と `//host` が無効、`#anchor` が通る、外部リンクの
`rel` と新しいタブ、相対リンクの `<a>` 無しと `title`、外部画像の `src` が不変、GFM の
5 要素、front matter の塊の行レンジ（閉じ無しは塊が出ない）、CRLF と複数バイト文字の
行レンジ、入れ子のリストと引用のブロック数と行レンジ、構造の同じ段落の 1 語の変更が
1 ブロックに両方の印、リンクを平文にした段落が 2 ブロック、消したブロックの差し込み
位置と旧側のレンジ、削除ファイルの全ブロックが削除の印、追加だけのファイルの全ブロックが
追加の印、`a.txt` → `b.md` の改名と `.md` の削除の対象判定、`../` と画像でない拡張子と
リポジトリを読めない入力の枠に `src` が無い、レビュー対象に一致する相対パスがその
ファイルの参照になる、ハイライト関数を渡さないコード塊が無地、上限超えの描画不可。
Left to the implementer: モジュールの分け方、ブロックの内部表現、インラインをほどく
表現、印の表し方、HTML のクラス名。
Stop and hand back if: 構文木に仕様のブロック種別の `Span` が無いとき。インラインの
子ノードを再描画できる形で取り出せないとき。

## Step 3 — 描画エンドポイントと `api/file` の描画表示の情報

Purpose: 描画表示の HTML とブロックの一覧を表示時に配り、切り替えを出すかどうかと
事前に分かる描画不可を `api/file` で知らせる。Specification:
`docs/spec/kemi.md#R-SERVE 配信モデル`、`docs/spec/kemi.md#R-RENDER 描画表示`（対象と
切り替え、Markdown の描画、上限と失敗）、`docs/spec/kemi.md#R-VIEW 差分の表示`
（ハイライトの設定）。
Prerequisites: Step 2。
May change: `crates/kemi-server/src/api.rs`、`crates/kemi-server/src/highlight.rs`、
`crates/kemi-server/src/lib.rs`、`crates/kemi-server/tests/server.rs`。
Done when: token 配下の `GET` で、描画表示の対象ファイルの HTML とブロック一覧が返り、
コード塊はそのファイルのハイライト設定（`highlight=on|off` と `dark`）に従って
ハイライトされ、相対パス画像の参照は URL になっている（URL の先は Step 8）。
`api/file` の応答に、そのファイルが描画表示の対象か（この Step では Markdown。表は
Step 6、画像は Step 7 で足す）と、事前に分かる描画不可（Markdown と表の行数・バイト数の
上限。画像には当てない）とその理由が載り、それを決めるのに内容を余分に読まない。描画の
純粋関数が失敗を返したときは失敗の応答になり、ソース表示の `api/file` は影響を
受けない。起動と `api/review` は描画を計算しない。
Shown by: test — `crates/kemi-server/tests/server.rs` の `FakeSource` を使い、描画の
取得が token 無しで拒否される、`api/review` までの内容の読み取りが 0 のまま（既存の
`content_reads_zero_during_startup_and_review` の対象に描画の経路が入る）、10,001 行の
Markdown で `api/file` が描画不可を返す、画像の `api/file` には行数・バイト数の描画不可が
付かない、`highlight=off` と `dark=1` で描画の応答のコード塊が変わる、描画の関数の
失敗を注入すると失敗の応答になる、を確かめるテストを足す。
Left to the implementer: エンドポイントの URL と JSON の形（D7）、キャッシュ（D16）、
ハイライトの注入の形、描画の失敗をテストから注入する形。
Stop and hand back if: 既存の `api/file` の JSON の既存フィールドを変えないと描画表示の
情報を載せられないとき。

## Step 4 — 描画表示の止まる場所とコメントの置き場の純粋関数

Purpose: ブロック列とコメントから、`n` / `p` の止まる場所、帯の印、各コメントを置く
ブロックを決める純粋関数を `model.js` に足す。Specification:
`docs/spec/kemi.md#R-NAV 変更間の移動`、`docs/spec/kemi.md#R-RENDER 描画表示`
（コメント、移動と帯）。
Prerequisites: Step 2 の `data-kemi-block` の値の書式（フロントはこれを読む）。
May change: `web/assets/model.js`、`web/assets/model.test.js`。
Done when: 変更の印の付いたブロックの先頭とコメントの吹き出しの位置が止まる場所に
なり、印の無いブロックは止まる場所にならない。コメントは行レンジに重なる最初の
ブロックに置かれ、重ならないものは直前のブロック、直前が無ければ文書の先頭に置かれる。
帯の印は同じ位置。既存の `navStops` / `rulerMarks` の呼び出し側は変わらない。
Shown by: test — `node --test web` に、止まる場所（印あり・印なし・コメント）と
コメントの置き場（重なる、複数に重なる、重ならない、先頭より前）を 1 つずつ確かめる
テストを足す。
Left to the implementer: 関数の名前と、既存の `navStops` を拡張するか別の関数にするか。
Stop and hand back if: なし。

## Step 5 — ページの描画表示（切り替え、ブロック、コメント、移動、ライブリロード）

Purpose: ファイルヘッダの切り替えとキー操作で描画表示を出し、ブロックからコメントを
付け、`n` / `p` と帯が描画表示で動き、読み込み直しで状態を保つ。Specification:
`docs/spec/kemi.md#R-RENDER 描画表示`（対象と切り替え、コメント、移動と帯、上限と失敗、
ライブリロードと復元）、`docs/spec/kemi.md#R-VIEW 差分の表示`、
`docs/spec/kemi.md#R-COMMENT コメントと suggestion`、`docs/spec/kemi.md#R-NAV 変更間の移動`、
`docs/spec/kemi.md#R-LIVE ライブリロード`、`docs/spec/kemi.md#委譲`（D15）。
Prerequisites: Step 3、Step 4。
May change: `web/assets/` の新しい feature（`features/` の順番で `files` の後、`comments` の
前）と view、`api.js`、`state.js`、`actions.js`、`app.js`、`views/file-header.js`、
`views/nav.js`、`features/display.js`、`features/navigation.js`、`features/comments.js`、
`style.css`、`web/index.html`、`PROJECT.md`（`features/` の順番の行）、`scripts/` の
新しいブラウザ自動化スクリプト。
Done when: 既定はソース表示（`api/file` が既定を描画表示と言うファイル、つまり SVG は
描画表示）で、`api/file` が切り替えありと言ったファイルのヘッダに
"Rendered" / "Source" の切り替えがあり、キー操作でも切り替わり、取得が終わるまで無効で、
描画不可のファイルでは無効で理由がツールチップに出る。押してから描画に失敗した
ファイルはソース表示のままヘッダに理由が出る。描画表示は表示モードに関係なく 1 段で、
行の仮想化と折りたたみを持たず文書全体を載せ、由来の行を出さず、すべてのブロックが
`data-kemi-block` を持つ。ブロックにホバーすると `+` が出て、押すとそのブロックの
行レンジへの行コメントの入力欄が元の行を見せて開く。side は新側にあるブロック（重ねた
ブロックを含む）が新側、消したブロックが旧側で、新側なら suggestion も書ける。吹き出しは
Step 4 の置き場に出る。`n` / `p` と帯が Step 4 の止まる場所で動く。ファイルごとの
切り替えはページを開いている間だけ残り、再読込で消え、サーバのセッション状態には
送られない。読み込み直しで描画表示は保たれ、描画できなくなったファイルはソース表示に
戻って切り替えが無効になる。`npx tsc -p web --noEmit` が通り、`web/index.html` の
`modulepreload` に新しいモジュールが入っている。
Shown by: check — `scripts/` に、`scripts/gen-fixture.sh` で作ったリポジトリに Markdown を
足して実際の `kemi` バイナリ（debug ビルド。パスはスクリプトの引数）を `--port 0
--no-open` で起動し、`agent-browser` でページを操作するスクリプトを足す。1 回目の起動で
(1) 描画表示の `data-kemi-block` の数と値が文書全体のブロックと一致する（仮想化と
折りたたみが無い）、(2) 10,001 行の Markdown で切り替えが無効、(3) worktree のファイルを
外部から変えて更新バッジから読み込み直しても描画表示のまま、上限を超えるように変えた
ファイルはソース表示に戻って切り替えが無効、(4) 2 列表示にしても描画表示が 1 段で
由来の行が無い、を確かめ、2 回目の起動で (5) 新側のブロックから付けたコメントと消した
ブロックから付けたコメントが、ページから submit した stdout の JSON に行コメントとして
入り、前者は `side: new` で `quote` がそのブロックの元の行、後者は `side: old` で
`suggestion` が `null`、を確かめる。続けて `npx tsc -p web --noEmit` と
`node --test web`。取得中の無効は既存のヘッダの無効化の仕組みに乗り、押してからの
描画の失敗は実際のバイナリでは起こせないので、どちらもサイクル終了時の人による確認に
含める。
Left to the implementer: view の分け方、キーの割り当て（D15）、CSS の値、理由の文言。
Stop and hand back if: `agent-browser` が使えないとき。既存の `features/` の順番の
規則（前の feature だけを直接 import）で描画表示の feature を置けないとき。

## Step 6 — 表（CSV / TSV）の描画表示

Purpose: `.csv` / `.tsv` を表として描画し、行の追加・削除・書き換えに色を付ける。
Specification: `docs/spec/kemi.md#R-RENDER 描画表示`（対象と切り替え、表（CSV / TSV）、
上限と失敗）、`docs/spec/kemi.md#作らないもの`（P19）。
Prerequisites: Step 2、Step 5。
May change: `crates/kemi-core/src/` の描画モジュール（表の分割とブロック化）とその
テスト、`crates/kemi-server/src/api.rs`（表の描画不可の事前判定と切り替えありを
`api/file` に載せる）、`crates/kemi-server/tests/server.rs`。画面は Step 9 で足す。
Done when: 区切りが拡張子で決まり、`"` で囲んだフィールドと `""` が解釈され、1 行目が
`<th>` になり、足した行・消した行が印を持ち、書き換えは旧の行を削除の印・新の行を
追加の印で上下に並び、列数の不揃いはそのまま出て、各行が `data-kemi-block` を持つ。
`"` の対応が取れない行があるファイルは、`api/file` が描画不可として返す（Step 5 の
ページが切り替えを無効にする）。
Shown by: test — `cargo test -p kemi-core` に、`"a,b"` が 1 フィールド、`""` が `"`、
`.tsv` のタブ区切り、対応の取れない `"` で描画不可、書き換えの行が旧新 2 ブロック、
見出し行の `<th>`、を確かめるテストを足し、`cargo test -p kemi-server` に、対応の
取れない `"` を含む `.csv` で `api/file` が描画不可を返すテストを足す。
Left to the implementer: 分割関数の置き場と表の HTML の構造。
Stop and hand back if: なし。

## Step 7 — レビュー対象の画像を並べる

Purpose: 画像ファイルを描画表示にし、旧と新を並べ、バイト列を安全に配る。
Specification: `docs/spec/kemi.md#R-RENDER 描画表示`（対象と切り替え、画像、コメント）、
`docs/spec/kemi.md#R-SERVE 配信モデル`（`api/image` の応答ヘッダ）、
`docs/spec/kemi.md#R-VIEW 差分の表示`（バイナリの例外）、
`docs/spec/kemi.md#R-NAV 変更間の移動`（画像を飛ばす）、
`docs/spec/kemi.md#R-INPUT-3 未コミット（worktree）`。
Prerequisites: Step 3、Step 5。
May change: `crates/kemi-core/src/` の描画モジュール（画像の判定）とそのテスト、
`crates/kemi-server/src/api.rs`（画像のエンドポイント、`api/file` の画像の情報）、
`crates/kemi-server/tests/server.rs`。画面は Step 9 で足す。
Done when: 拡張子が画像で内容がバイナリ判定のファイルと `.svg` のファイルが画像に
なり、拡張子が画像でも内容がテキストのもの（LFS のポインタ、空ファイル）は今までの
テキストの差分のまま。manifest の文字列にも同じ判定がかかる。SVG 以外の画像は切り替え
無しで描画表示、SVG は切り替えありで既定が描画表示。旧と新の `<img>` が 2 列で左右・
1 列で上下に並び、それぞれにバイト数が付き、追加・削除は片側だけ、改名でバイト列が
同じなら 1 枚と "unchanged"、読み込みに失敗した側はバイト数だけの枠（画面は Step 9）。
片側 5 MB 超と untracked の上限超え（内容が無い）は今までのバイト数の増減だけ。画像の
エンドポイントは token 配下の `GET` で、拡張子から決めた `Content-Type`、
`X-Content-Type-Options: nosniff`、`Content-Security-Policy: sandbox` を付け、レビュー
対象でない id は 404。画像へのコメントはファイル全体のコメントだけで、`n` / `p` が
画像を飛ばすのは既存の `R-NAV` の規則のままで変えない。
Shown by: test — `cargo test -p kemi-core` に画像の判定（バイナリの `.png`、テキストの
`.png`、`.svg`）のテストを足し、`crates/kemi-server/tests/server.rs` に、画像の応答に
3 つのヘッダが付く、token 無しと未知の id で拒否される、5 MB 超と untracked の上限超えで
`api/file` がバイト数だけを返す、`api/file` が SVG は切り替えあり・既定が描画表示で、
他の画像は切り替え無しと返す、を確かめるテストを足す。画面は Step 9 で確かめる。
Left to the implementer: エンドポイントの URL の形（D7）、画像の枠の HTML。
Stop and hand back if: `Content-Security-Policy: sandbox` を付けた応答が `<img>` で
描画されないブラウザがあると分かったとき（仕様の前提が崩れる）。

## Step 8 — Markdown の相対パス画像

Purpose: Markdown の中の相対パス画像を、その画像を含むブロックの側の版から読んで出す。
Specification: `docs/spec/kemi.md#R-RENDER 描画表示`（Markdown の描画の表と枠の層）、
`docs/spec/kemi.md#R-SERVE 配信モデル`（相対パス画像の条件と応答ヘッダ）、
`docs/spec/kemi.md#R-SESSION セッションの保存と復元`（復元では写しにあるものだけ）。
Prerequisites: Step 2、Step 7。
May change: `crates/kemi-core/src/source/mod.rs`（`ReviewSource` の新しいメソッド、
既定は `None`）、`crates/kemi-core/src/source/git.rs` とそのテスト、
`crates/kemi-server/src/api.rs`、`crates/kemi-server/tests/server.rs`。
Done when: 描画の応答で、正規化したパスがその側のレビュー対象ファイルに一致する画像は
レビュー対象画像のエンドポイントの URL になり（復元でも写しから出る）、一致しない
ものは相対パス画像のエンドポイントの URL になる。相対パス画像のエンドポイントは token
配下の `GET` で、入力モードごとの版（worktree は作業ツリー / `HEAD`、staged は
インデックス / `HEAD`、最終形は `to` / `from`、コミットごとはそのコミット / 親）から
読み、symlink・5 MB 超・存在しないファイル・manifest・復元では 404 を返し、200 には
Step 7 と同じ 3 つのヘッダが付く。
Shown by: test — `cargo test` の一時的な git リポジトリ（既存の git テストと同じ作り）で、
worktree の新側のブロックと `HEAD` の旧側のブロック、staged のインデックスと `HEAD`、
`--from` / `--to` の最終形の `to` と `from`、コミットごとのそのコミットと親、でそれぞれ
違うバイト列が返る、symlink と 5 MB 超と存在しないファイルが 404、200 に 3 つの
ヘッダ、を確かめるテストと、`crates/kemi-server/tests/server.rs` で、レビュー対象に
一致する相対パスがレビュー対象画像の URL になり、凍結ソース相当（メソッドが `None`）
では一致しないものが `src` の無い枠になるテストを足す（URL は描画した HTML の `src`
から取り、形には依存しない）。
Left to the implementer: メソッドの名前と引数、パス正規化の実装、2 つのエンドポイントを
1 つのハンドラにするかどうか。
Stop and hand back if: symlink を辿らない読み取りが対象 OS で揃わないとき。コミットごとの
単位で「その親」を引けないファイル（最初のコミットなど）の扱いが仕様から導けないとき。

## Step 9 — 画面の総仕上げと速度の確認

Purpose: 表・画像・復元を含むページの振る舞いをブラウザ自動化で確かめ、速度が落ちて
いないことを測る。Specification: `docs/spec/kemi.md#R-RENDER 描画表示`（表、画像、
ライブリロードと復元）、`docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-SERVE 配信モデル`、`docs/spec/kemi.md#R-UNIT グループ単位の切り替え`、
`docs/spec/kemi.md#R-VERIFY 検証`。
Prerequisites: Step 6、Step 7、Step 8。
May change: `web/assets/` の描画表示の feature / view、`views/file-header.js`、
`style.css`（表と画像の画面）、`scripts/` の Step 5 のブラウザ自動化スクリプト（項目の
追加）。
Done when: `.csv` / `.tsv` のヘッダに切り替えがあり、描画表示で 1 行目が `<th>` で
書き換えの行が上下に並ぶ。`.svg` は既定で描画表示で切り替えがあり、他の画像は
切り替え無しで、旧と新が 2 列で左右・1 列で上下に並び、バイト数が付き、改名で同じ
バイト列なら 1 枚と "unchanged"、読み込みに失敗した側はバイト数だけの枠になる。
相対パス画像のうち `src` の取得が 404 になるものは枠に差し替わる。保留した
セッションを `kemi --resume` で開くと Markdown を描画表示にでき、相対パス画像は
レビュー対象のものだけ出て他は枠で、ファイルの切り替えはソース表示から始まる。
起動と `file` / `busy` の計測値が Step 1 の記録を上回らず、10,000 行の Markdown の
描画エンドポイントの応答時間が記録されている。
Shown by: check — Step 5 のスクリプトに、`.csv` の切り替えと `<th>`、`.svg` と `.png` の
切り替えの有無と既定、画像の並び（2 列と 1 列）と "unchanged"、壊れた画像の枠、
symlink を参照する相対パス画像の 404 と枠、保留（SIGTERM）→ `kemi --resume <id>`
（別の起動）での描画表示・相対パス画像・切り替えの既定、の項目を足して通す。続けて
release を作り直し、`scripts/measure-startup.sh` と `scripts/measure-range.sh <fixture>
<bin> file` / `busy` を Step 1 と同じフィクスチャで測り、10,000 行の Markdown で描画
エンドポイントの応答時間を測って、Step 1 の記録と並べて記録する。
Left to the implementer: 計測の記録の置き場（サイクルの報告に含める）、表と画像の
画面の HTML の構造。
Stop and hand back if: `agent-browser` が使えないとき。計測が「速度の条件」を超え、
チューニングしても縮まないとき。

## Step 10 — README

Purpose: 利用者向けに描画表示とキー操作を README に書く。Specification:
`docs/spec/kemi.md#R-DIST 名称と配布`。
Prerequisites: Step 5〜9。
May change: `README.md`。
Done when: README のページの説明に描画表示（Markdown / CSV / TSV / SVG の切り替え、
画像の並び、ブロックからのコメント、相対パス画像）が英語で書かれ、"Keys in the page" の
表に切り替えのキーが入っている。`skills/kemi/` は変えない。
Shown by: artifact — `README.md`。
Left to the implementer: 言い回しと置き場所。
Stop and hand back if: 契約と食い違わない書き方が見つからないとき。
