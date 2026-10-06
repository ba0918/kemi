# --live ②: 差分と変化の一覧

## Goal

`--live` で比べる相手がスナップショットのとき、動いているページとスナップショットを要素ごとに対応させ、
変わったところに印を付け、ページのツリーの表示中のページの下に変化の一覧と変化の数を出す。
ページが HMR や保存で変わったら、一覧と印も今のページに合わせて変わる。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（変化を並べること、ページのツリーの変化の一覧と変化の数、狭い画面）
  - [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)（2 MB の上限、`sandbox` の枠）
  - [委譲](../spec/live.md#委譲)（DL2 要素の対応づけ、DL3 スナップショットの中の形）、[未決](../spec/live.md#未決)（UL1）
- [比べる相手 仕様](../spec/live-compare.md): [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)
  （変化の一覧と印はスナップショットのときだけ。計画①の Step 8 が後回しにした「モックのときは変化の一覧が出ない」の部分）
- [kemi 仕様](../spec/kemi.md): [R-NARROW](../spec/kemi.md#r-narrow-狭い画面)、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)
  （動いているページはその場で変わってよい例外）、[R-VERIFY](../spec/kemi.md#r-verify-検証)（`--live` でないレビューはページ用の
  ファイルを読み込まない。動いているページの検査は試験用の開発サーバとブラウザ自動化）。
- 見た目の参照: [docs/design/ui-mock-live.html](../design/ui-mock-live.html)（案 A。左の列のページのツリーの下に変化の一覧、主な変化が上、
  ずれただけは畳む。印は赤・緑・薄い紫の枠）。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md` の「スナップショット」「比べる相手」「見る対象」「ページのツリー」。

## 4 つの計画の中の位置

`--live` は 4 つの計画に分けた。① 中継と見方とスナップショット（main にマージ済み）、**② この計画**、
③ ページへのコメント（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)）、④ 保留と復元の残り。
スナップショットは ④ までメモリにだけあり、レビューを終えると消える。したがって、この計画より前の形で取った
スナップショットを読む互換は要らない。

## Approach and why

### 今の作り（調べた事実）

- スナップショットは `web/live/page.js` の `captureSnapshot` が、動いているページの中で DOM を写して作る 1 つの HTML。
  レビュー画面の `web/assets/features/live.js` の `capture` が `postMessage` で頼み、`crates/kemi-server/src/api/snapshot.rs` の
  `take_snapshot` が `crates/kemi-server/src/live/mod.rs` の `Snapshot`（id・page・width・kind・html）としてメモリに持つ。
  2 MB の上限は HTML のバイト数で、サーバ（`SNAPSHOT_LIMIT`。要求の本文は `SNAPSHOT_BODY_LIMIT` = 8 MB まで）とレビュー画面
  （`features/live.js` の `SNAPSHOT_LIMIT`）の両方で見ている。
- スナップショットはレビュー画面の中の `sandbox=""` の iframe（`srcdoc`）に出す。`allow-same-origin` が無いので不透明なオリジンになり、
  レビュー画面からはその DOM も計算済みのスタイルも読めない。
- 動いているページは中継のオリジンで動き、レビュー画面とは `page.js` との `postMessage` だけで話す（R-PAGE-PROXY）。
- 比べる相手の選び方は `web/assets/live-model.js` の `chooseReference`、ページのツリーは `buildPageTree` と
  `web/assets/views/live.js` の `renderPageTree`。
- ページの見方のファイル（`features/live.js`・`views/live.js`・`live-model.js`・`live.css`）は `modulepreload` に載せず、
  `app.js` が `--live` のレビューでだけ動的に `import` する（`PROJECT.md` の「`web/assets/` の層」）。
- 試験用の開発サーバ `scripts/live-dev-server.mjs` には `/siblings.html`（`?extra=1` で途中に 1 つ足す）があるが、クエリが違うと
  別のページ（`pageKey` はパスとクエリ）になるので、同じページの前後の比較には使えない。ボタンの背景色を変えるページは無い。
  HMR に当たる知らせは `style.css` の差し替えだけを扱う。
- `showSnapshot` は、比べる相手のスナップショットの id が変わったときだけ枠を作り直して `srcdoc` を入れる。作り直すと枠の
  スクロールは先頭に戻る。並べる表示では、スナップショットの枠は自分の中でスクロールし、その位置はレビュー画面から読めない。
- 検査 `test-live.mjs` の「`--live` でないレビューはページ用のファイルを読み込まない」は、パスに `live` を含む URL を数える。

### 両側の「要素の記述」を作り、レビュー画面で比べる

レビュー画面はどちらの側の DOM も読めないので、要素ごとに比べるには両側の要素を書き出した記述が要る。

- **スナップショットの側**: `page.js` が写すときに、同じ DOM から要素の記述（以下「記述」）も作り、HTML と一緒に返す。記述は
  スナップショットに付けてサーバのメモリに持ち、スナップショットを取り出すときに一緒に返す。写した時点の本物のページから取るので、
  スナップショットを後から描き直して測るより、その時点のページに忠実。これは DL3（スナップショットの中の形）の範囲の決定。
- **動いているページの側**: レビュー画面が `postMessage` で頼むと、`page.js` が今のページの記述を返す。
- **比べる**: レビュー画面が、2 つの記述を新しい純粋なモジュールで比べ、変化の一覧を作る。純粋にするのは、対応づけと分類の規則を
  `node --test` で確かめるため。

記述に入れるもの（最低限）: 要素を対応づける手がかり（タグ、`id`、親の中の位置など）、その要素自身の文字、位置と大きさ、
見た目のスタイルのうち少なくとも `color`・`background-color`・`font-size`・`border-radius`
（R-PAGE-DIFF の例と、成功条件のボタンの背景色）。ほかの見た目のプロパティと、空白だけの文字の差の扱いなどの線引きは、
仕様が「試作で決める」としているので実装者が決める。一覧には主な変化の前後の値（色なら前の色と後の色、文字なら前と後の文字）を出す。
1 つの要素が見た目と文字の両方で変わることがある（表し方は実装者が決める）。

位置はスクロールに依存しない文書の座標で持つ（表示中の範囲を基準にすると、スクロールしただけで多くの要素がずれただけになる）。
開いている shadow root の中の要素も対象にする（スナップショットも shadow DOM を写しているため）。閉じた shadow root は読めないので、
その持ち主の要素までを対象にする。

### 2 MB の上限と記述

[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット) の「1 つ 2 MB まで」は、仕様の定義どおり DOM と CSS の記録、
つまり HTML に掛ける（今と同じ）。記述に別の上限は作らない（仕様に無い受け入れの境目を計画が決めないため）。記述が大きすぎて
要求の本文の上限（8 MB）を超えたら、今ある「Not recorded: …」の出し方でそのまま断られる。

### 印をどちらの側に付けるか

- 今のページにある要素の変化（主な変化、増えた、ずれただけ）は、見る対象（動いているページ）の側に印を付ける。採用した画面モックと同じ。
  ずれただけの印は控えめにする（R-PAGE-DIFF）。
- 消えた要素は今のページに無いので、比べる相手（スナップショット）の側に印を付ける。スナップショットの枠はスクリプトが動かず
  中を触れない。スクリプトは足さない（R-PAGE-SNAPSHOT の `sandbox` の約束は変わらない）。方法は 2 つ考えられ、どちらにするかは
  実装者が決める。(a) `page.js` が写すときに写しの側の要素へ番号の属性を付けておき（原本の要素には付けない）、印の `<style>` を足した
  HTML を出す。shadow root の中に効かせるには、その中にも `<style>` が要る。(b) スナップショットの枠を中身の高さで描いて外側で
  スクロールさせ（重ねて透かす表示が今そうしている）、レビュー画面が記述の位置を使って枠の上に印を重ねる。
  どちらでも、印を付け直したときに比べる相手のスクロール位置を変えない（[R-LIVE](../spec/kemi.md#r-live-ライブリロード) の
  「スクロール位置は変えない」）。今の `showSnapshot` のように枠を作り直すとスクロールが先頭に戻るので、(a) は作り直さずに印を替える
  工夫が要る。
- 印の描き方（動いているページの要素に枠を重ねる層を `page.js` が足すか、レビュー画面が枠の上に重ねるか）は実装者が決める。ただし、
  kemi が足した印は、記述にも、後から取るスナップショットの HTML にも入れない（入れると、印そのものが変化として出る）。印を付け直しても
  ページの変化の見張りが反応しない（反応すると、知らせ → 計算し直し → 印の付け直し、が繰り返す）。

### 変化を計算し直すきっかけ

比べる相手が変わったとき（選択、新しいスナップショット、モックの割り当てと解除）、ページを移ったとき、表示幅を変えたとき、
動いているページが変わったとき。最後のものは、`page.js` がページの変化（DOM の変化、読み込み、大きさの変化）を見張り、
間を置いてまとめてレビュー画面に知らせる。これが無いと、HMR の後に一覧と印が古いまま残る。動き続けるページ（アニメーションなど）で
計算が続かないように間引く方法は実装者が決める。

一覧と印がページに合わせてその場で変わるのは、[R-LIVE](../spec/kemi.md#r-live-ライブリロード) の例外（動いているページはその場で
変わる）から読んだ決定。仕様は一覧が自動で追うとは書いていないが、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方) は
「変わったところに必ず印を付け」るとしており、ページだけが変わって印が古いままだと、印が今のページの別の要素を指す。
この読みは承認のときに利用者に確かめる（違うなら、更新バッジを押して計算し直す形に変える）。

### 一覧の置き場所

ページのツリーの、表示中のページの下に出す（[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)、画面モックの案 A）。
変化の数（主とずれの内訳）もそこに出す。変化が 0 のときも、数 0 と変化が無いことを出す（画面モックの「変化なし」。比べた結果が
出ていることと、比べていないことを見分けるため）。表示中でないページには変化の数を出さない。ずれただけは既定で畳む。同じページを
表示している間は、一覧が描き直されても開いた状態を保ち、ページを移ったら畳む。保存はしない。
狭い画面では、ツリーと同じ引き出しに入る（今のページのツリーが引き出しに入る作りのまま）。比べる相手がモックのとき、
または記録されていないときは、一覧も印も出さない。

### 新しいモジュールの置き場所

比べる純粋な処理は、新しい leaf（例: `web/assets/live-diff.js`）に置く。`live-model.js` と同じく、ほかの leaf を import せず、
`modulepreload` に載せず、`--live` のレビューでだけ読み込まれる（`features/live.js` から import する）。ファイル名には `live` を含める
（上の検査がパスの `live` で数えるため）。`PROJECT.md` の層の表と、`modulepreload` に載せない例外の列挙の両方に足す。

## Scope of change

- ページ: `web/assets/` の下（新しい leaf とそのテスト、`features/live.js`、`views/live.js`、`live-model.js`、`live.css`）、`web/live/page.js`
- Rust: `crates/kemi-server/src/live/mod.rs`（`Snapshot` に記述を足す）、`crates/kemi-server/src/api/snapshot.rs`、
  そのテスト（`crates/kemi-server/tests/` か同じファイルのテスト）
- 検査: `scripts/test-live.mjs`、`scripts/live-dev-server.mjs`
- 文書: `PROJECT.md`（層の表）、`CHANGELOG.md`（Unreleased）、`README.md` と `skills/kemi/`（`--live` の説明に変化の一覧を足す必要が
  あるときだけ。CLI と JSON の契約は変えない）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/` は変えない。`web/index.html` の `modulepreload` は変えない（新しい leaf は載せない）。

## Step order and prerequisites

Step 1 → 2 → 3 → 4 の順。各ステップは前のステップが終わっていることを前提にする。
Step 1 で比べる規則を固めてから、Step 2 で記述を作って一覧を出し、Step 3 で印を付ける。一覧の検査は記述と比べる処理の両方が
要るので Step 2 にまとめる。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。WSL の負荷のため）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `CARGO_BUILD_JOBS=4 cargo test --workspace`、`node --test web`
3. ブラウザの検査があるステップ: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   `node scripts/test-live.mjs target/release/kemi` と、既存の 4 本（`test-agent-channel.mjs`・`test-narrow-screen.mjs`・
   `test-rendered-view.mjs`・`test-horizontal-scroll.mjs`）を 1 本ずつ回す。

---

## Step 1 — 2 つの記述を比べて変化の一覧を作る純粋な処理

Purpose: 要素の対応づけと、主な変化・ずれただけへの分け方を、画面と切り離して確かめられる形で作る。
Specification: [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)、[委譲](../spec/live.md#委譲) の DL2。
Prerequisites: なし。
May change: 新しい leaf（例: `web/assets/live-diff.js`）とそのテスト（例: `web/assets/live-diff.test.js`）、`PROJECT.md`（層の表と、`modulepreload` に載せない例外の列挙）。
Done when: 2 つの記述（前と今）を受け取り、変化の一覧を返す関数がある。一覧の各項目は、種類（主な変化のうち見た目・文字・
増えた・消えた、またはずれただけ）、どの要素か（一覧に出せる手がかり）、主な変化なら前後の値を持つ。対応づけは
[R-PAGE-DIFF](../spec/live.md#r-page-diff-差分) の 2 つの約束を守る。位置と大きさだけが変わった要素はずれただけに入り、主な変化には入らない。
Shown by: test — RED → GREEN → REFACTOR、`node --test web`。
- 兄弟の途中に 1 つ要素が入ると、その要素だけが増えたになり、後ろの兄弟は増えた・消えたにならない（位置が変われば、ずれただけ）。
- 兄弟の途中の要素が 1 つ消えると、その要素だけが消えたになる。
- `id` を持つ要素は、並びが変わっても `id` で対応する。
- 背景色だけが変わった要素が主な変化に入り、前後の色の値を持つ。
- 1 つの要素が大きくなり、下の要素の位置だけが変わると、下の要素はずれただけに入り、主な変化に並ばない（仕様の反例）。
Left to the implementer: 対応づけの方法（DL2。兄弟の並びの最長共通部分列など）、記述の正確な形、1 つの要素の複数の変化の表し方、
関数と型の名前（ファイル名には `live` を含める）。
Stop and hand back if: 2 つの約束の両方を満たす対応づけが作れない場合（例: `id` で対応させると兄弟の並びの約束と食い違う並びがある）。

## Step 2 — 両側の記述を作り、ページのツリーに変化の一覧と数を出す

Purpose: スナップショットと動いているページから記述を作り、比べた結果を一覧として画面に出す。
Specification: [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)
（ページのツリーの変化の一覧と数、狭い画面）、[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（スナップショットのときだけ）、
[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)（2 MB）、DL3、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 1。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live-model.js`、`web/assets/live.css`、
Step 1 の leaf、`crates/kemi-server/src/live/mod.rs`、`crates/kemi-server/src/api/snapshot.rs` とそのテスト、`scripts/live-dev-server.mjs`、
`scripts/test-live.mjs`。
Done when:
- `page.js` が、写すときに HTML と記述を返し、頼まれたら今のページの記述を返す。ページが変わったら、間を置いてまとめて知らせる。
- サーバがスナップショットに記述を付けて持ち、取り出すときに返す。2 MB の判定は今どおり HTML だけに掛ける。
- 比べる相手がスナップショットのとき、ページのツリーの表示中のページの下に、変化の数（主とずれの内訳）と変化の一覧が出る。
  主な変化が上で前後の値を持ち、ずれただけは既定で畳む。表示中でないページには変化の数を出さない。
  変化が 0 のときは、数 0 と変化が無いことが出る。ずれただけを開いた状態は、同じページの間は描き直しても保つ。
- 比べる相手がモックのとき、記録されていないときは、一覧も数も出ない。
- 計算し直すきっかけは「Approach and why」の「変化を計算し直すきっかけ」のとおりで、HMR やファイルの保存の後に一覧が今のページに合う。
- 試験用の開発サーバに、同じ URL のまま中身を変えられるページを足す: 兄弟の途中に要素を 1 つ足す・足したものを戻す、ボタンの背景色を
  変える。ボタンの背景色は、ページを読み込み直さずに変わる方法（今の `style.css` の HMR の知らせで CSS を差し替える）で変える。
  兄弟の足し・戻しの方法は問わない。クエリを変える方法は別のページになるので使えない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分) の成功条件 2 つ: 兄弟の途中に要素を 1 つ足すと、足した要素だけが増えたになり、
  後ろの要素は増えた・消えたにならない。ボタンの背景色を変えると、そのボタンが主な変化に入り、色の前後の値が出る。
  どちらも、スナップショットを取った後に同じ URL の中身を変え、レビュー画面も枠も検査の側から読み込み直さずに一覧が変わることで
  確かめる。ボタンの背景色は HMR の CSS の差し替えで変え、読み込みを伴わないページの変化をレビュー画面が拾うことも確かめる。
- [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の成功条件のうち変化の一覧の部分: モックを割り当てたページでは変化の一覧が出ず、
  割り当てを外すと最後に渡した時点のスナップショットに戻り、変化の一覧が出る（変化が 0 でも、数 0 の一覧が出ることで見分けられる）。
- 2 つのページにスナップショットがあるとき、変化の数は表示中のページにだけ出る。
- 幅 390px で開くと、変化の一覧が引き出しの中にある。
Left to the implementer: 記述の形と送り方（DL3）、ページの変化の見張り方と間引き方、一覧の文言（今の英語の流儀に合わせる）と見た目の細部
（画面モックの案 A に倣う）、見た目のプロパティの範囲（最低限の 4 つを含む）、試験用のページの作り。
Stop and hand back if: 記述を付けると、今まで取れていた大きさのページで要求の本文の上限（8 MB）を超えて取れなくなる場合（記述の形を
小さくしても避けられないなら、上限の扱いは仕様の判断になるので報告する）。動いているページの変化を見張ると、試験用のページの操作が
目に見えて重くなる場合。

## Step 3 — 変わったところに印を付ける

Purpose: 一覧に出た変化を、ページの上でも見つけられるようにする。
Specification: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（変わったところに必ず印を付ける）、
[R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)（ずれただけの印は控えめ）、[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)
（スナップショットの枠でスクリプトを動かさない）、[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（重ねて透かす表示）。
Prerequisites: Step 2。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live.css`、Step 1 の leaf、
`scripts/test-live.mjs`、`scripts/live-dev-server.mjs`。
Done when:
- 主な変化と増えた要素に、動いているページの側で印が付く。ずれただけの要素の印は、主な変化より控えめ。
- 消えた要素に、スナップショットの側で印が付く。スナップショットの枠には今どおりスクリプトを入れない（印は `<style>` で付ける）。
- 一覧が変わると印も変わる（印だけが古いまま残らない）。kemi の印が記述にも後から取るスナップショットの HTML にも入らず、
  印を付けても変化が増えない。印を付け直してもページの変化の見張りが反応しない。
- 印を付け直しても、比べる相手の枠のスクロール位置が変わらない。
- 並べる表示でも重ねて透かす表示でも印が見える。比べる相手がモックのとき、記録されていないときは印を出さない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- ボタンの背景色を変えたとき、そのボタンに主な変化の印が付き、その下の変わっていない要素には付かない。
- 兄弟の途中に要素を足したとき、足した要素に増えたの印が付き、後ろの兄弟には主な変化の印が付かない。
- 兄弟の途中の要素を消したとき（足した状態でスナップショットを取り、同じ URL のまま元に戻す）、スナップショットの側のその要素に消えたの印が付く。
- 印を付けた後も変化の数が変わらない。印を付けた後に手でスナップショットを取り、それと比べると変化が 0 になる。
- 重ねて透かす表示に切り替えても、同じ要素に同じ印がある。
- 比べる相手の枠をスクロールしてからページを変えて印が付け直されても、枠のスクロール位置が変わらない。
印があるかは、テストが観測できる方法で確かめる（中継したページの中で要素を調べる、画面の画素を調べる、など）。合格は「その要素の位置に
その種類の印がある」こと。色合いや線の太さは人の確認。
Left to the implementer: 印の描き方（動いているページの側は、ページの中に重ねる層を足すか、レビュー画面が枠の上に重ねるか。
スナップショットの側は「印をどちらの側に付けるか」の (a) か (b)）と見た目（画面モックの赤・緑・薄い紫の枠に倣う）、番号の属性の名前。
Stop and hand back if: 印をスナップショットの側に付けるために、スナップショットの枠の `sandbox` を緩める必要が出た場合。
(a) と (b) のどちらでも、印を付け直したときに比べる相手のスクロール位置を保てない場合。

## Step 4 — 文書を直し、速さを測る

Purpose: 利用者向けの文書をこの計画の範囲に合わせ、変化の計算の時間を UL1 の材料として報告する。
Specification: [未決](../spec/live.md#未決) の UL1、[R-VERIFY](../spec/kemi.md#r-verify-検証)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)
（README とスキルの中身の範囲）。
Prerequisites: Step 3。
May change: `CHANGELOG.md`、`README.md`、`skills/kemi/`、`PROJECT.md`、`scripts/live-dev-server.mjs`（計測用の大きなページを足すなら）。
Done when: CHANGELOG の Unreleased に変化の一覧と印が書いてある。README とスキルは、`--live` の説明が比べる相手と見比べ方に触れている
ならそこに変化の一覧を足し、触れていなければ変えない。要素の多いページ（数千要素）で、記述を作る時間と比べる時間を測り、数字を報告する。
Shown by: check — 次の順に実行する。
1. 各ステップの終わりの検査のすべて
2. `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`（スキルを変えたときだけ）
3. 要素の多い試験用のページで、記述を作る時間（動いているページの側と写す側）と比べる時間を測り、要素数と一緒に報告する
Left to the implementer: 計測用のページの規模と作り、測り方。
Stop and hand back if: 計測で、ページの操作や一覧の更新が目に見えて遅い場合。数字を報告し、上限や間引きを仕様で
決めるかの判断を仰ぐ。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-DIFF（対応づけの 2 つの約束、主とずれの分け方） | Step 1（純粋な処理）、Step 2（成功条件 2 つ） |
| R-PAGE-VIEW（変化の一覧と数、表示中のページだけ、狭い画面） | Step 2 |
| R-PAGE-VIEW（変わったところに必ず印） | Step 3 |
| R-PAGE-REF（一覧と印はスナップショットのときだけ） | Step 2（一覧）、Step 3（印） |
| R-PAGE-SNAPSHOT（2 MB、スクリプトを動かさない枠） | Step 2（2 MB）、Step 3（枠） |
| R-LIVE（動いているページはその場で変わってよい。スクロール位置は変えない） | Step 2（HMR の後に一覧が変わる）、Step 3（印を付け直しても比べる相手のスクロールが保たれる） |
| R-VERIFY（ページ用のファイルを読み込まない） | 各ステップの終わりの `test-live.mjs`（①で足した、`--live` でないレビューが読み込む URL を数える検査） |
| UL1 の材料 | Step 4 |

## Left to the implementer

- 要素の対応づけの方法（DL2）、記述の形と送り方（DL3）。
- 見た目のプロパティの範囲と、主とずれの線引きの細部（最低限 `color`・`background-color`・`font-size`・`border-radius` を含み、
  [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分) の約束を守る）。
- ページの変化の見張り方と間引き方。
- 印の描き方と見た目、一覧の文言と見た目の細部（画面モックの案 A に倣う）。
- モジュールの分け方と名前（`PROJECT.md` の層と features の順の規則を守る）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、保存先、エラーの扱い、上限）を決めないと進めない場合。
- スナップショットの枠の `sandbox` を緩める、または中継したページとレビュー画面が `postMessage` 以外で話す必要が出た場合。
- 今のモード（manifest・コミット範囲・worktree・staged）か ① の検査を、仕様を変えずに書き換える必要が出た場合。
- 変化の計算や見張りで、ページの操作が目に見えて遅くなった場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 一覧の項目を押してその要素へ移る・選ぶこと、一覧から「場所に足す」こと（画面モックにあるが、コメントの場所は ③）。
- 変化の一覧をエージェントに渡すこと（仕様に無い）。
- 表示中でないページの変化の数（仕様が出さないと決めている）。
- スナップショットと記述の保存（④）。
- `--live` 独自の性能の数字を決めること（UL1。この計画は測って報告するだけ）。
