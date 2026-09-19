# 狭い画面（R-NARROW）を実装する

## Goal

幅 720px 未満の狭い画面で kemi のページを開いても、ツリーが本文を押しつぶさず、1 列で
折り返して読め、行番号のタップでコメントを付けて submit できる。広い画面の見た目と操作は
変わらない。

## Specification

`docs/spec/kemi.md`（狭い画面の改訂 `3428a80`）。見出しは `R-NARROW 狭い画面`、
`R-VIEW 差分の表示`、`R-COMMENT コメントと suggestion`、`R-NAV 変更間の移動`、
`R-RENDER 描画表示`、`R-SERVE 配信モデル`（localStorage の用途）、`R-VERIFY 検証`、
`作らないもの`（P22〜P26）。用語は `CONTEXT.md` の「狭い画面」「広い画面」
「引き出し / シート」。

## Approach and why

- **幅の判定は 1 箇所。** `style.css` の 1 つのメディアクエリ（`max-width: 719.98px`）と、
  同じ文字列を `matchMedia` で見る JS の 1 箇所だけが幅を見る。`prefers-color-scheme` の
  既存の `matchMedia`（`app.js`、`features/theme.js`）は幅を見ないので対象外。
- **狭い画面専用の要素は `web/index.html` に常に置き、広い画面では CSS で隠す。** ツリーを
  開く操作、「…」のメニュー、2 段目のバー、シートの閉じる操作がそれ。JS は幅で要素を
  足し引きしない。
- **1 列と折返しの強制は「実効値」で行い、広い画面の記憶に触れない。** 既存の `setMode` は
  localStorage の `kemi-mode` を書く（`features/display.js` と `storage.js`）ので、狭い画面
  ではそれを呼ばない。描画に使う表示モードを「狭い画面なら 1 列、そうでなければ
  `state.mode`」と派生させ、折返しも「狭い画面なら狭い画面用の値（`state` にだけ持つ）、
  そうでなければ `state.wrap`」と派生させる。広い画面に戻れば派生が元の値に戻るので、
  仕様の「覚えている表示モードに戻る」「広い画面の既定は変えない」が構造で満たされる。
  幅をまたぐときの作り直しは、既存の `recomputeDisplay(anchor)` の経路（上端の行を残す）を
  通す。
- **タップの選択は純粋関数で決め、既存の選択の入れ物に流し込む。** 次の選択を返す関数を
  `model.js` に置き、狭い画面では `views/diff-rows.js` の行番号の `mousedown` /
  `mouseenter`（`startSelection` / `extendSelection`）を使わず、`click` からこの関数で
  `state` の選択を置き換える。`+`（`line-add-btn`）はいま「コメントできる全行に作り、
  ホバーした行だけ CSS で見せる」作りなので、狭い画面では「選択中の範囲の最後の行」に
  view が印を付け、その行の `+` だけを CSS で見せる。
- **上部バーの 2 段は既存の要素を CSS の grid で並べ替え、足りない操作だけ足す。**
  `title` を押すとシートが開く操作、ツリーを開く操作、「…」のメニューは新しい要素。
  `subtitle` / `meta` / 表示の操作のアイコンは既存のまま、狭い画面では置き場（シート、
  メニュー）が変わる。メニューの中の文字ラベルはメニューの要素にだけ付け、広い画面の
  アイコンには付けない（`R-VIEW` の「文字ラベルを持たない」を保つ）。
- **引き出しは既存のツリーの要素をそのまま重ねる。** 中身は変えず、CSS で位置と表示を
  変える。ファイル選択で閉じるのは、既存のファイル選択の経路（`features/files.js`）から
  `actions` 経由で閉じる。
- **サーバは触らない。** 変わるのは `web/` と README だけで、起動時間と応答は影響を
  受けない。

## Scope of change

- `web/index.html`（狭い画面専用の要素、`modulepreload` の一覧）
- `web/assets/style.css`（メディアクエリの 1 ブロック。広い画面の規則は変えない）
- `web/assets/state.js`（幅の状態、狭い画面の折返し、引き出しとシートの開閉、実効値）
- `web/assets/model.js`、`web/assets/model.test.js`（タップの選択、実効値の純粋関数）
- `web/assets/features/` の新しい feature（幅の変化、引き出し、シート、メニュー）、
  `features/display.js`（実効値で描画、幅をまたいだときの作り直し）、
  `features/files.js`（ファイル選択で引き出しを閉じる）、`features/navigation.js`
  （`n` / `p` で引き出しを開かない）、`features/comments.js`（狭い画面のタップの選択）
- `web/assets/views/header.js`、`views/file-header.js`、`views/tree.js`、
  `views/diff-rows.js`（タップの入口と最後の行の印）、`views/rendered.js`（ブロックの
  タップ）、`views/comment-list.js`、`views/overlay.js`（シート）、`actions.js`、`app.js`
- `scripts/` の新しいブラウザ自動化スクリプト（幅 390px と 1280px）
- `PROJECT.md`（`features/` の順番の行）、`README.md`

これ以外は触らない。`crates/`、`src/`、`skills/` は変更しない。広い画面のスタイル
（メディアクエリの外）は変えない。

## Step order and prerequisites

Step 1（純粋関数）と Step 2（配置）は互いに独立で、どちらからでもよい。Step 3（上部バーと
シート）は Step 2 の幅の状態に依存する。Step 4（コメントの入口）は Step 1 と Step 2 に
依存する。Step 5（README）は Step 2〜4 の後。

## Verification map

| 仕様の見出し | 確かめる Step |
|---|---|
| `R-NARROW 狭い画面` | Step 1（タップの選択）、Step 2（広い画面を変えない、配置、幅またぎ）、Step 3（上部バー、シート）、Step 4（コメントの入口） |
| `R-VIEW 差分の表示` | Step 2 と Step 3（広い画面で上部バーとヘッダの成功条件がそのまま成り立つ） |
| `R-COMMENT コメントと suggestion` | Step 4（タップで付けたコメントが submit JSON に入る） |
| `R-NAV 変更間の移動` | Step 2（`n` / `p` で引き出しを開かない） |
| `R-RENDER 描画表示` | Step 2（画像が上下に並ぶ）、Step 4（ブロックのタップ） |
| `R-SERVE 配信モデル` | Step 2（localStorage の既存の鍵を狭い画面で書き換えない） |
| `R-VERIFY 検証` | 各 Step の Shown by。`npx tsc -p web --noEmit` と `node --test web` は Step 1〜4 で通す |

仕様の成功条件のうち「幅で変わる規則が 1 つのメディアクエリに集まっている」は
サイクルの差分レビューで、「実機のスマホで読む・コメントする・submit する」はサイクルの
終わりに利用者が確かめる受け入れとする。

## Left to the implementer

- 新しい feature / view / 関数 / CSS クラスの名前と、feature の置き場（`display` より後で、
  前の feature だけを直接 import する規則を守る）。
- 引き出し・シート・メニューの見た目（幅、影、開閉の動き）。色は既存の役割の変数を使う。
- ツリーを開く操作と「…」のアイコン（モノクロの SVG。既存のアイコンの作りに合わせる）。
- 2 段目が入りきらないときの「数字だけ」「3 段目」の切り替えを CSS だけで行うか JS で
  測るか。どちらでも観察される結果は同じ。
- 実効値の派生をどこに置くか（`state.js` の読み取り関数か `model.js` の純粋関数か）。

## Stop conditions

- `agent-browser` の `set viewport` で `matchMedia` の変化が発火しないとき。
- 幅で変わる規則を 1 つのメディアクエリに集められない箇所が出たとき（例: 既存の
  インライン style が幅を決めている）。
- 広い画面のスタイルを変えないと狭い画面の配置が作れないとき。
- タップの選択を既存の選択状態に流し込むと、広い画面のドラッグの振る舞いが変わってしまう
  とき。
- 2 段目の上部バーで、送信ボタンとグループ単位の切り替えの文言を変えないと 390px に
  収まらないとき（仕様は文言を変えないと定める）。

## Out of scope

- バージョンと `CHANGELOG.md`（リリース時にまとめる）。
- `skills/kemi/` の変更（契約が変わらない）。
- ジェスチャ、PWA、UA 判定、ピンチズームの禁止、狭い画面専用の設定（P22〜P26）。
- サーバと CLI の変更。
- 広い画面の見た目の変更。

---

## Step 1 — タップの選択と実効値の純粋関数

Purpose: いまの選択とタップした行から次の選択を返す関数と、幅の状態から描画に使う
表示モードと折返しを返す関数を `model.js` に足す。Specification:
`docs/spec/kemi.md#R-NARROW 狭い画面`（配置、コメントの入口）、
`docs/spec/kemi.md#R-COMMENT コメントと suggestion`。
Prerequisites: なし。
May change: `web/assets/model.js`、`web/assets/model.test.js`。
Done when: 次の 6 つの入力に対する返り値が `R-NARROW` の「コメントの入口」の規則と
一致する: (1) 選択なしでタップ、(2) 同じ側で表示中の連続した別の行、(3) 反対側の行、
(4) 折りたたみをまたぐ行、(5) 範囲があるときの 3 つ目の行、(6) 同じ行の再タップ。
実効値の関数が、狭い画面では 1 列と狭い画面用の折返し、広い画面では `state` の表示モードと
折返しを返す。
Shown by: test — `node --test web` に、上の 6 つと実効値の 2 つ（狭い / 広い）を 1 つずつ
確かめるテストを足す。
Left to the implementer: 関数の名前、表示中の行の渡し方。
Stop and hand back if: なし。

## Step 2 — 狭い画面の配置

Purpose: 幅 720px 未満でツリーを引き出しに、1 列と折返しを既定に、吹き出しを全幅にし、
幅をまたいでも再読込なしで切り替わり、広い画面は変えない。Specification:
`docs/spec/kemi.md#R-NARROW 狭い画面`（広い画面を変えない、配置）、
`docs/spec/kemi.md#R-VIEW 差分の表示`、`docs/spec/kemi.md#R-NAV 変更間の移動`、
`docs/spec/kemi.md#R-SERVE 配信モデル`、`docs/spec/kemi.md#R-RENDER 描画表示`。
Prerequisites: Step 1 の実効値の関数。
May change: `web/index.html`、`web/assets/style.css`（メディアクエリの 1 ブロック）、
`state.js`、`features/` の新しい feature、`features/display.js`、`features/files.js`、
`features/navigation.js`、`views/tree.js`、`views/file-header.js`、`views/diff-rows.js`
（吹き出しの位置）、`actions.js`、`app.js`、`PROJECT.md`、`scripts/` の新しいブラウザ
自動化スクリプト。
Done when: 幅 390px を開いて次が観察できる。ツリーが見えず本文が全幅で、上部バーの操作で
引き出しが開き、その中身は広い画面のツリーと同じ要素で、ファイルを選ぶか外を押すと
閉じる。行は 1 列で折り返され、2 列への切り替えが見えない。折返しを切り替えると別の
ファイルへ移っても保たれ、再読込すると既定（オン）に戻り、localStorage の内容は切り替えの前後で
同じ（`R-SERVE` の用途の外に何も書かない）。ファイルヘッダは 2 行で追従し、長いパスは
先頭が省略記号で切れて末尾が見える。吹き出しと入力欄の左端が本文の左端に揃い、本文の幅
は約 72 文字を超えない。描画表示の画像が上下に並ぶ。`n` / `p` でファイルをまたいでも
引き出しは開かない。帯・由来の行・見たのチェック・キー操作は広い画面と同じ要素で動く。
幅 1280px で 2 列を選んでから 390px にすると 1 列になり、1280px に戻すと 2 列のまま。390px で引き出しを開き、選択と下書きを作ってから 1280px に
すると、引き出しは閉じ、選択と下書きは残り、上端に見えていた行が同じ位置にある。
Shown by: check — `scripts/` に、`scripts/test-rendered-view.mjs` と同じ作り（実際の
`kemi` バイナリと `agent-browser`。`set viewport 390 844` と `set viewport 1280 800`）の
スクリプトを足し、順に (1) 390px でツリーが見えず、引き出しが開き、中の要素の数が
1280px のツリーと同じで、ファイル選択で閉じ、外を押しても閉じる、(2) 390px で
`data-kemi-row` の行が 1 列で折り返され、2 列の切り替えが見えない、(3) 390px で折返しを
切り替えて別のファイルへ移っても保たれ、再読込で戻り、localStorage の内容が切り替えの
前後で同じ、(4) 390px でヘッダが 2 行で、長いパスの先頭が省略記号、(5) 390px で吹き出しの
左端が本文の左端と一致し、画像が column で並ぶ、(6) 390px で `n` を押してファイルを
またいでも引き出しが開かない、(7) 1280px で 2 列 → 390px で 1 列 → 1280px で 2 列、
(8) 390px で引き出しを開き、範囲を選んで下書きを書き、1280px にすると引き出しが閉じて
選択と下書きが残り、上端の行が同じ、を確かめる。続けて `npx tsc -p web --noEmit` と
`node --test web`。
Left to the implementer: feature の置き場、CSS の値、引き出しの見た目、実効値の置き場。
Stop and hand back if: `set viewport` で `matchMedia` が発火しないとき。広い画面のスタイルを
変えないと配置が作れないとき。

## Step 3 — 狭い画面の上部バーとシート

Purpose: 幅 720px 未満で上部バーを 2 段にし、表示の操作を「…」のメニューに、`subtitle` /
`meta` とコメント一覧をシートにし、広い画面の上部バーは変えない。Specification:
`docs/spec/kemi.md#R-NARROW 狭い画面`（配置）、`docs/spec/kemi.md#R-VIEW 差分の表示`
（上部バーの成功条件）、`docs/spec/kemi.md#R-SUBMIT 送信と契約`、
`docs/spec/kemi.md#R-UNIT グループ単位の切り替え`。
Prerequisites: Step 2。
May change: `web/index.html`、`web/assets/style.css`（同じメディアクエリのブロック）、
`state.js`、Step 2 の feature、`views/header.js`、`views/comment-list.js`、
`views/overlay.js`、`actions.js`、`app.js`、`scripts/` の Step 2 のスクリプト。
Done when: 幅 390px で、上部バーが 2 段で、1 段目に `kemi`・`title`・ツリーを開く操作・
コメント一覧の入口・更新バッジ、2 段目にグループ単位の切り替え（コミット範囲のとき）・
見たの進捗・"Approve" と "Request changes" が見える。「…」を押すと折返し・重要のみ・
変更量順・テーマがこの順で文字ラベル付きに出る。2 段目が入りきらない幅では見たの進捗が
数字だけになり、それでも入らなければ 3 段目に折り返して、送信ボタンとグループ単位の
切り替えの文言は変わらない。`title` を押すと `subtitle` と `meta` のシートが開いて閉じ
られ、コメント一覧はシートで開いて閉じられる。幅 1280px では、文字ラベルを持つ上部バーの
操作がグループ単位の切り替え・送信のボタン・更新バッジ・コメント一覧の入口だけで、
`subtitle` と `meta` が上部に出て、狭い画面専用の操作が見えない。
Shown by: check — Step 2 のスクリプトに (9) 390px の上部バーの 1 段目と 2 段目の要素、
(10) 「…」のメニューの 4 項目の順と文字ラベル、(11) 320px で見たの進捗が数字だけになり
送信ボタンの文言が変わらない、(12) `title` のシートとコメント一覧のシートの開閉、
(13) 1280px で文字ラベルを持つ上部バーの操作が 4 種類だけで、`subtitle` と `meta` が
上部に見え、狭い画面専用の操作が見えない、を足して通す。続けて `npx tsc -p web --noEmit`
と `node --test web`。
Left to the implementer: シートとメニューの見た目、2 段目の折り返しの実装、アイコン。
Stop and hand back if: 送信ボタンの文言を変えないと 390px に収まらないとき。

## Step 4 — 狭い画面でのコメントの入口

Purpose: 行番号とブロックのタップで `+` を出し、コメントを付けられるようにする。
Specification: `docs/spec/kemi.md#R-NARROW 狭い画面`（コメントの入口）、
`docs/spec/kemi.md#R-COMMENT コメントと suggestion`、`docs/spec/kemi.md#R-RENDER 描画表示`。
Prerequisites: Step 1、Step 2。
May change: `views/diff-rows.js`、`views/rendered.js`、`features/comments.js`、Step 2 の
feature、`style.css`（同じメディアクエリのブロック）、`scripts/` の Step 2 のスクリプト。
Done when: 幅 390px で、行番号のタップで `+` がその行にだけ出て、同じ側の別の行のタップで
範囲になり `+` は範囲の最後の行にだけ出て、反対側の行のタップは新しい 1 行の選択になり、
行番号以外を押すと解除され、`+` から開いた入力を取り消すと解除される。行番号の押し下げと
ホバーでは選択が始まらない。描画表示のブロックはタップで `+` が出る。付けたコメントは
submit の JSON に行コメントとして入る。幅 1280px では行番号の押し下げとホバーで範囲が
作れ、ホバーした行に `+` が出る。
Shown by: check — Step 2 のスクリプトに (14) 390px で行番号のタップ → `+` が 1 つ、
2 つ目のタップで範囲になり `+` が最後の行に 1 つ、反対側のタップで 1 行に戻る、行番号
以外を押すと選択が消える、(15) 390px で `mousedown` と `mouseenter` を送っても選択が
始まらない、(16) 390px で描画表示のブロックのタップで `+`、(17) タップで付けた範囲
コメントが submit の stdout の JSON に入り `quote` が対応する行、(18) 1280px で押し下げと
ホバーで範囲が作れる、を足して通す。続けて `npx tsc -p web --noEmit` と `node --test web`。
Left to the implementer: 選択状態への流し込み方、最後の行の印の付け方。
Stop and hand back if: 広い画面のドラッグの振る舞いが変わってしまうとき。

## Step 5 — README

Purpose: 狭い画面で使えることを README に書く。Specification:
`docs/spec/kemi.md#R-NARROW 狭い画面`、`docs/spec/kemi.md#R-DIST 名称と配布`。
Prerequisites: Step 2〜4。
May change: `README.md`。
Done when: README の "Exposing kemi to other devices" の節に、幅 720px 未満ではツリーが
引き出しになり 1 列で折り返され、行番号のタップでコメントできることが英語で書かれている。
Shown by: external — 人が README を読み、`R-NARROW` の配置とコメントの入口の記述と
食い違わないことを確かめる。
Left to the implementer: 言い回し。
Stop and hand back if: なし。
