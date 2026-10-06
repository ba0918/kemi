# --live ③: ページへのコメント

## Goal

`--live` の動いているページに、要素・矢印・ペンの場所を持つコメントを付けられるようにする。コメントは会話パネルに
スレッドとして並び、「エージェントに渡す」と `kemi wait` に場所の情報と、描き込みを重ねた画像のファイルの絶対パスが
渡る。submit の JSON の `page` にも同じ場所が入る。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（画像の作り方の段落を含む）
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（ページのツリーのコメントの数、コメントだけがあるページ、狭い画面で
    指で描くこととタップで選ぶこと）
  - [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（下の「保留と復元の分け方」の範囲だけ）
  - [却下](../spec/live.md#却下)（RL7 と、その但し書き）
- [比べる相手 仕様](../spec/live-compare.md): [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（コメントは見る対象の側だけ。
  重ねて透かしている間に要素を選ぶと見る対象の要素が場所になる。計画①の Step 9 が後回しにしたもの）
- [kemi 仕様](../spec/kemi.md): [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（`page` の形、ページへのコメントの各フィールド、
  `outdated` は常に `false`、submit の後は `image` が `null`）、[R-INPUT](../spec/kemi.md#r-input-入力)（`--live` のグループ `page` と
  題 "Page"）、[R-COMMENT](../spec/kemi.md#r-comment-コメントと-suggestion)（保存した後は場所を変えない）、
  [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)、
  [R-DIST](../spec/kemi.md#r-dist-名称と配布)（契約を変えるコミットでスキルと README も直す）、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
- [エージェントとのやりとり 仕様](../spec/agent-channel.md): [R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)
  （渡したページへのコメントの形、画像は `kemi wait` でだけ絶対パス）。
- 見た目の参照: [docs/design/ui-mock-live.html](../design/ui-mock-live.html)（案 A の「コメントを書く流れ」: 上の帯の道具
  「要素・矢印・ペン・操作」、場所の番号がページの上と書く欄の上に出る、× で外しても振り直さない、保存すると番号が控えめな印になり
  会話パネルのスレッドの札になる）。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md` の「コメント」「見る対象」「比べる相手」「ページのツリー」「会話パネル」。

## 4 つの計画の中の位置

`--live` は 4 つの計画に分けた。① 中継と見方とスナップショット、② 差分と変化の一覧（どちらも main にマージ済み）、
**③ この計画**、④ 保留と復元の残り。

### 保留と復元の分け方

この計画で作るもの: ページへのコメント（場所と画像のパスを含む）を `<id>.session` に保存して復元で戻すこと、コメントの画像を
`<id>.files/` に書くこと、submit を確定したとき、セッションを消すとき（掃除で消すとき、`--live` で会話が無くなって `<id>.session` を
消すときを含む）に `<id>.files/` も消すこと。強制終了などで `<id>.session` が無いまま残った `<id>.files/` の回収は ④（掃除の数え方と一緒）。
④ で作るもの: スナップショットとモックの割り当ての保存、`<id>.files/` の 20 MB の規則（古いものから消す順と、保存しなかったことの表示）、
掃除の大きさの数え方に `<id>.files/` を入れること。したがって ③ の段階では、削除したコメントの画像も `<id>.files/` に残り、
大きさの上限は掛からない（④ までの途中の状態）。

## Approach and why

### 今の作り（調べた事実）

- コメントは `crates/kemi-core/src/domain/review.rs` の `Comment` で、`file_id`・`path`・`side` が必須（`String` と `Side`）。
  `crates/kemi-server/src/session.rs` の `comment_json` が submit・`kemi wait`・画面への応答の 3 か所で共通に使われ、`page` は常に `null`。
- コメントの API（`crates/kemi-server/src/api/comments.rs`）の `Add` は `file_id` を必須とし、`find_file` でファイルを探す。
  追加のたびに `Notice::CommentAdded { path, side, lines }` を stderr に出す（この行は仕様の契約ではない）。
- submit の `build_submit_document`（`crates/kemi-server/src/api/submit.rs`）は全コメントで `find_file` を呼び、見つからなければ
  `outdated = true` にする。ファイルを持たないページへのコメントがこのまま通ると `outdated: true` になり、仕様と食い違う。
- `--live` のレビューのグループは、git の中では `worktree` の 1 つ、外では無し。`page` グループを作る箇所はまだ無い。
- セッションは `<id>.session` に `SessionState`（`CommentDto` を含む）を保存する。`CommentDto` の `file_id`・`path`・`side` は
  `String` で default が無い。形式の版は 3 で、版 3 はまだリリースしていない（リリース済みの最後は版 2）ので、版を上げずに
  項目を足してよい。`<id>.files/` を扱うコードは無く、`delete` も掃除（`cleanup`）も `remove_file` だけでディレクトリを消さない。
  サーバ（`SessionSink`）はセッションの置き場所のパスを知る口を持たない。
- 渡したコメントは `Channel` の events に渡した時点の写しとして入り、`<id>.session` にも保存される。
- 会話パネル（`web/assets/views/conversation.js`、`features/conversation.js`）・差分の中の札（`views/comment.js`）・書く欄
  （`features/comments.js`）は、行コメント（`path`・`start_line`・`group_id` と `path` の照合）を前提にしている。
- `web/live/page.js` が受ける頼みは `describe`・`marks`・`capture`、送る知らせは `described`・`captured`・`scroll`・`page`・`changed`。
  印は閉じた shadow root の層に描く（`marksHost`）。`describePage` は要素の元の並びも返す。セレクタに似た手がかりは
  `web/assets/live-diff.js` の `label` が作る（id を持つ祖先か 3 段まで）。`captureSnapshot` は、スクリプトを除き CSS と画像を
  `data:` に埋め込んだ 1 つの HTML を作る。canvas に描く処理（SVG の `foreignObject` など）はまだ無い。
- `web/assets/features/live.js` は `state.js` を import していない（コメントの一覧を受け取る経路が要る）。

### 順序

画像 → データと API とセッション → 場所を置く道具 → 会話パネルとツリー → 渡すと submit → 文書。

画像を最初にするのは、いちばん不確かなところだから。ページの中で写しを canvas に描く方法（SVG の `foreignObject` に写しを入れて
画像として描く）が、試験用のページで使える見た目になるかを先に確かめ、だめなら他を作る前に止める。次にデータと API とセッションを
作るのは、画面の操作がすべてそれを前提にするから。道具（場所を置く）は API ができてから作る。会話パネルとツリーは保存した
コメントが要るので道具の後。渡すことと submit は、全部がそろってから端から端まで確かめる。

### ページへのコメントの持ち方

`Comment` に「場所」を持たせる（コードへのコメントは今どおり、ページへのコメントは `page` を持つ）。どちらか一方だけを持つことを
型で表す（例: ファイルの位置か、ページの位置かの enum）。`comment_json` はページへのコメントで `path`・`side`・`start_line`・
`end_line` を `null`、`quote` を `[]`、`suggestion` を `null`、`outdated` を `false` にし、`page` に場所を入れる（[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)）。
グループは `page`、題は "Page"（[R-INPUT](../spec/kemi.md#r-input-入力)）。git の外の `--live` でもページへのコメントは付けられる
（[R-PAGE-MODE](../spec/live.md#r-page-mode-起動) の「ページへのコメント・案・submit はできる」）。

`page.url` はパスとクエリ（`#` から後ろを除く。`web/assets/live-model.js` の `pageKey` と同じ形）。`places` は番号 `n` の順に並べる。

`image` は `kemi wait` の応答でだけ絶対パス（[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)）。submit の JSON と
画面への応答（`api/review`・`api/comment`・SSE）では `null`。画像を書けなかったとき（セッションを作れないレビュー、書き込みの失敗、
大きすぎて送れない）は `null`（契約が許す値）にして、コメントの保存は止めない。

コメントと画像は 1 回の要求で保存する。別々にすると、画像が届く前に「エージェントに渡す」を押したとき、渡した写し（`Channel` が
渡す時点のコメントを複製する）の `image` が `null` のまま届く。画像のファイル名はサーバが決める（コメントの id など。要求の値で
パスを組まない）。画像を結び付けるのは保存のときの 1 回だけ。要求の本文の上限は、スナップショットの口と同じ
`SNAPSHOT_BODY_LIMIT`（8 MB）を使う（新しい値を作らないため）。`page.js` は画像をこれに収まる大きさに縮めて作り、それでも
収まらなければ画像を付けずに保存する（`image` は `null`）。

既知の制約: submit の確定で `<id>.files/` を消した後に、まだ受け取られていない `handed` を `kemi wait` が返すと、消えたファイルの
パスが届くことがある（仕様に決まりが無い。この計画では扱わない）。

### 画像の作り方

[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所) の画像は、`page.js` がコメントを保存するときに、今のページの写し
（`captureSnapshot` と同じ形）を描き、場所の周りを切り出し、描き込みと番号を重ねて PNG にする。画面がそれを受け取り、サーバに
送ってコメントに結び付け、サーバが `<id>.files/` に書く。CDP などでブラウザを操作しない（[却下](../spec/live.md#却下) の RL7 の但し書き）。
切り出す範囲（場所の外接矩形にどれだけ余白を足すか）と大きさは実装者が決める。

### 道具と場所

上の帯に道具「要素・矢印・ペン・操作」を置く（画面モックの案 A）。「操作」のときはページを普通に触れ、それ以外のときは
動いているページの上で場所を置く。要素を選ぶ・矢印の先の要素・ペンで囲んだ範囲と重なる要素を決めるのは、ページの中
（`page.js`）でしかできない（別のオリジン）。場所の座標はページの CSS ピクセル（[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)）で、
② の記述と同じく、スクロールに依存しない文書の座標にする。重ねて透かしている間も、場所は下の見る対象に置く
（[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)）。比べる相手の側には置けない。

### 保存したコメントの見え方

会話パネルのスレッドの項目に、ページへのコメントであること（URL と表示幅、場所の数）を出す。保存したコメントの場所は、そのコメントの
URL と表示幅で見ているとき、動いているページの上に番号つきの控えめな印として出す（画面モックの案 A の「保存すると、ページの上の
番号は控えめな印になり」）。スレッドを開いている間は、そのコメントの印を目立たせる。仕様はこの見せ方を決めておらず、採用した
画面モックに倣う見た目の判断（承認のときに確かめる）。別の表示幅で付けたコメントは付けた幅を出し、
押すとその幅とその URL に切り替える（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)）。保存した後は場所を変えず、
本文だけを編集できる。ページのツリーには、コメントのあるページ（コメントだけのページを含む）とコメントの数を出す
（[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)）。

## Scope of change

- Rust: `crates/kemi-core/`（`domain/review.rs` の `Comment`、`domain/comment.rs` の検証、`session/` の保存と `<id>.files/`、
  `domain/agent.rs` は形が変わるなら）、`crates/kemi-server/`（`session.rs` の JSON、`api/comments.rs`、`api/submit.rs`、
  画像を受け取る API、`lib.rs` の `SessionSink`・`Notice`）、`src/`（`session.rs`、`notice.rs`、`main.rs` の配線）
- テスト: `tests/e2e.rs`、`crates/kemi-core` と `crates/kemi-server` のテスト、`web/assets/*.test.js`、`scripts/test-live.mjs`、
  `scripts/live-dev-server.mjs`
- ページ: `web/live/page.js`、`web/assets/` の下（`features/live.js`・`views/live.js`・`live-model.js`・`live.css`、会話パネルと
  書く欄と札のモジュール、`model.js`・`state.js`・`api.js`）。新しいモジュールを足したら `PROJECT.md` の層の節に書く
  （`--live` 用なら `modulepreload` に載せない例外の列挙にも）
- 文書: `README.md`、`skills/kemi/`（`page` を持つコメントの読み方。[R-DIST](../spec/kemi.md#r-dist-名称と配布)）、`CHANGELOG.md`（Unreleased）、`PROJECT.md`
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/` は変えない。

## Step order and prerequisites

Step 1 → 2 → … → 6 の順。各ステップは前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。WSL の負荷のため。
cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`
3. ブラウザの検査があるステップ: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   `node scripts/test-live.mjs target/release/kemi` と、既存の 4 本（`test-agent-channel.mjs`・`test-narrow-screen.mjs`・
   `test-rendered-view.mjs`・`test-horizontal-scroll.mjs`）を 1 本ずつ回す。

---

## Step 1 — ページの中で場所の周りの画像を作る

Purpose: いちばん不確かな「写しを canvas に描いて画像にする」が、使える見た目になるかを先に確かめ、その処理を作る。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（画像の段落）、[却下](../spec/live.md#却下) の RL7。
Prerequisites: なし。
May change: `web/live/page.js`、`scripts/live-dev-server.mjs`、`scripts/test-live.mjs`。
Done when: `page.js` が、文書の座標の矩形と、重ねる描き込み（要素の枠・矢印・ペンの線と番号）を受け取り、その範囲の PNG を返す
処理を持つ。試験用の開発サーバの `/rich.html`（shadow DOM・canvas・SVG・入力欄）と `/resources.html`（CSS・画像・`@import`・
style 属性の `url()` など）で作った画像が、場所の周りの見た目として読める。ページの CSP が `style-src 'self'` や
`require-trusted-types-for 'script'` のときも画像が作れるか、作れないなら画像だけを諦めて理由が分かるか（②で足した
`/changing.html?csp=1`・`?tt=1` で確かめる）。
Shown by: external — 人が見る。`test-live.mjs` に、`/rich.html` と `/resources.html` で描き込み無しの画像を作り、動いているページの
同じ範囲の画面の画素と比べて差の割合を出し、差の画像を残す確かめを足す（②の画素の比較の道具を使う。描き込みを重ねると必ず差が
出るので、比べるのは描き込み無しの画像）。完全一致は求めない
（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所) が細部の違いを認めている）ので、検査は差の割合と差の画像を報告して
`CHECK` として人の確認に回す。作った PNG を `scripts/` の外の一時ディレクトリに書き、そのパスも出す。
Left to the implementer: 描き方（SVG の `foreignObject` に写しを入れて画像として描く、など）、切り出す余白、`SNAPSHOT_BODY_LIMIT` に
収める縮め方（大きなページを丸ごと描かず範囲だけ描く、場所が離れていれば縮める、など）、描き込みの見た目（画面モックの紫の線と番号に倣う）。
Stop and hand back if: 試験用のページで、画像が場所の周りとして読めないほど崩れる（例: 文字や画像が出ない、canvas が汚染されて
読み出せない）。そのときは差の画像と割合を添えて止める（作り方を仕様で決め直す判断になる）。CSP の下で画像が作れないことは
止める理由にしない（画像を `null` にする経路で受け止める。Step 5 で確かめる）。

## Step 2 — ページへのコメントのデータ、API、セッション

Purpose: ページへのコメントを作り・編集し・消し・保存して復元でき、JSON に正しい形で出るようにする。
Specification: [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（`page` の形とページへのコメントの各フィールド、`outdated`、`image`）、
[R-INPUT](../spec/kemi.md#r-input-入力)（`page` グループ）、[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（場所は 1 つ以上、
番号は振り直さない、保存した後は場所を変えない）、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（「保留と復元の分け方」の
③ の範囲）、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)。
Prerequisites: Step 1。
May change: `crates/kemi-core/`、`crates/kemi-server/`、`src/`、`tests/e2e.rs`、各クレートのテスト、`README.md`・`skills/kemi/`・
`CHANGELOG.md`（出力の `page` と `group_id: "page"` は契約なので、このコミットで文書も直す。[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
今のスキルと CHANGELOG は `page` が常に `null` と書いている）。
Done when:
- コメントの API でページへのコメントを、画像（任意）と一緒に 1 回の要求で足せる（URL、表示幅、場所の並び。各場所は番号・種類・点・要素）。
  場所が 0 個、番号が重なる、種類が 3 つのどれでもない、`--live` でないレビュー、のどれかは断る（エラーの文言は英語）。場所は番号の順に
  並べ直して持つ。要素の場所の `points` が空であることと、ペンの要素が 5 個までであることは作る側（`page.js`）が守り、サーバは検証しない。
  編集は本文だけを変え、場所と画像は変えない。
- 画像は `<id>.files/` に、サーバが決めた名前で書く。書けないとき、セッションが無いとき、画像が付いていないときは `image` が `null` で、
  コメントは残る。
- `comment_json` がページへのコメントを [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約) の形で出す。`image` は `kemi wait` の応答でだけ
  絶対パスで、submit の JSON と画面への応答では `null`。submit でページへのコメントの `outdated` が `false` のまま。
- ページへのコメントのグループは `page`、題は "Page"。git の外の `--live` でも付けられる。
- ページへのコメントが `<id>.session` に保存され、復元で場所と画像のパスごと戻る。渡したコメントの events も同じ。
- submit を確定したとき、セッションを消すとき（掃除で消すとき、`--live` で会話が無くなって `<id>.session` を消すときを含む）に、
  `<id>.files/` も消える。
- コードへのコメントの扱い（API、JSON、セッション）は変わらない。
Shown by: test — RED → GREEN → REFACTOR。
- e2e: ページへのコメントを足して渡すと、`kemi wait` の JSON の `page` に URL・表示幅・場所（番号・種類・点・要素）・画像の絶対パスが
  入り、そのファイルが読める（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所) の成功条件 1 のうち、画面の操作を
  API で代えた部分）。
- e2e: submit の JSON で、同じコメントの `image` が `null`、`path`・`side`・`start_line`・`end_line`・`suggestion` が `null`、`quote` が
  `[]`、`outdated` が `false`、`group_id` が `page`、`group_title` が "Page"。`kemi wait` と submit のコメントのキーが同じ（今の
  `a_handed_comment_has_the_same_keys_as_in_the_submit` に倣う）。
- e2e: ページへのコメントを渡してから保留して復元すると、コメントが場所ごと戻り、復元した後の `kemi wait` が返す渡したコメントにも
  画像の絶対パスが入っている。
- e2e: git の外の起動ディレクトリで `--live <ファイル>` を起動しても、ページへのコメントを足して渡せる。
- e2e: submit を確定すると、`<id>.files/` が消える。
- ユニットテスト（`crates/kemi-core` の session）: セッションの削除・掃除・会話が無くなったときの削除で、`<id>.files/` も消える。
- 統合テストかユニットテスト: 場所が 0 個・番号の重なり・知らない種類・`--live` でないレビューを断る（入力が境界に届く場面）。
  編集の要求が場所を変えない（本文だけが変わる）。
- 画像を書けないときの `null` は、決まった起こし方が無いので専用の検査は足さない（画像を付けない要求の `null` の確かめで足りる）。
Left to the implementer: `Comment` の場所の型の作り、1 回の要求の中での画像の送り方（base64 など）、stderr の
コメントの知らせの行（ページへのコメントで何を出すか。契約ではないが、コードへのコメントの行は変えない）、`SessionSink` に
置き場所を渡す口の作り。
Stop and hand back if: コードへのコメントの既存のテストで、確かめている内容（assert）を変える必要が出た場合。`Comment` の形が変わる
ことによる、テスト用の値の組み立ての機械的な書き換えは止める理由にしない。セッション形式の版を上げる必要が出た場合（版 3 は
未リリースなので、上げずに済むはず）。

## Step 3 — 場所を置く道具と、書く欄

Purpose: 動いているページの上で要素・矢印・ペンの場所を置き、番号を見ながら本文を書いて保存できるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（3 種類の場所、番号、書いている途中の足し・取り消し、
エージェントに渡す要素の情報、矢印の先の要素、ペンの重なる要素）、[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（見る対象の
側だけ、重ねて透かしている間も下の見る対象）、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（狭い画面で指で描く・タップで選ぶ）。
Prerequisites: Step 2。
May change: `web/live/page.js`、`web/assets/` の下、`scripts/live-dev-server.mjs`、`scripts/test-live.mjs`、`PROJECT.md`（層の節）。
Done when:
- 上の帯に道具「要素・矢印・ペン・操作」がある。「操作」ではページを普通に触れる。
- 要素: 押した要素を場所にする。同じ要素をもう一度選ぶと外れる。矢印: 引いた線の点と、先端の位置にある要素。ペン: 線の点と、
  囲んだ範囲と重なる要素を重なる面積の大きい順に 5 個まで。要素の情報はセレクタ・文字・位置と大きさ。
- 場所を足すたびに番号（1 から）がページの上と書く欄の上に出る。一覧からの削除、最後の 1 つの取り消し、描いている途中の取りやめが
  できる。消しても番号は振り直さない。
- 保存すると、Step 1 の画像を作り、Step 2 の API でコメントと画像を保存する。保存した後は場所を変えない。
- 比べる相手の側では場所を置けない。重ねて透かしている間に要素を選ぶと、下の見る対象の要素が場所になる。
- 狭い画面で、指で矢印とペンを描け、タップで要素を選べる。
- kemi が足した描き込みと番号は、② の要素の記述にも、スナップショットにも、変化の一覧にも入らない（仕様の明文ではなく、② の印の層
  （`marksHost`）が記述・スナップショット・見張りから外されている決まりに倣う）。
- この時点では会話パネルの見せ方は Step 4 で作るが、保存したページへのコメント（`path` が `null`）で、今の会話パネル・差分の中の札・
  ファイルのツリー・状態の読み取りが例外を出さない（最低限の表示でよい）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- 2 番目の場所を消してから保存すると、場所の番号が 1 と 3 のまま残る（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所) の成功条件 2）。
- 矢印の先の要素が、矢印の先端の位置にある要素と一致する（同じく成功条件 3）。
- 並べた比べる相手の側で要素を選ぶ・描く操作をしても場所が増えない。重ねて透かしている間に要素を選ぶと、見る対象の要素が
  場所になる（[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の成功条件 3）。
- 場所を描いている間と保存した後で、変化の一覧の数が変わらない（kemi の描き込みが差分に入らない。② の印の層の決まりに倣う
  退行の防止）。
指での操作感は人の確認（[R-VERIFY](../spec/kemi.md#r-verify-検証) の「動いているページ」）。
Left to the implementer: 押す・描く操作をどこで受けるか（`page.js` がページの中で受けるか、レビュー画面が枠の上に透明な層を重ねて
座標を `page.js` に渡すか）、セレクタの作り（ページの中でその要素 1 つに当たるものが望ましい）、道具の帯と書く欄の見た目
（画面モックの案 A に倣う）、ペンで「囲んだ範囲」の決め方（閉じた線の内側か、外接矩形か）。
Stop and hand back if: 押す・描く操作をページの中で受けると、ページのスクリプトがそれを止めたり横取りしたりして場所が置けない
試験用ページがある場合（透明な層を重ねる方式に変えても解けないとき）。

## Step 4 — 会話パネル、保存したコメントの印、ページのツリー

Purpose: 保存したページへのコメントを会話パネルで読み、場所をページの上で確かめ、ページのツリーで数を見られるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（別の表示幅で付けたコメントは幅を出し、押すとその幅に
切り替わる、保存した後は場所を変えない）、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（ツリーのコメントの数、コメントだけが
あるページ）、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)。
Prerequisites: Step 3。
May change: `web/assets/` の下、`web/live/page.js`、`scripts/test-live.mjs`、`PROJECT.md`（層の節）。
Done when:
- 会話パネルのスレッドの項目に、ページへのコメントであること（URL、表示幅、場所の数）が出る。今の絞り込み（すべて・未解決・表示中の
  ファイル）は変えない（ページへのコメントは「表示中のファイル」には入らない。ページでの絞り込みは仕様に無いので作らない）。
- 保存したコメントの場所が、そのコメントの URL と表示幅で見ているとき、動いているページの上に番号つきの控えめな印で出る。スレッドを
  開いている間は、そのコメントの印が目立つ。コメントの URL と表示幅が今と違えば、
  付けた幅を出し、押すとその URL とその幅に切り替わる（ページの見方でないときは、ページの見方に移る）。
- 返信・解決・畳む・本文の編集・削除が、コードへのコメントと同じように使える。場所は変えられない。
- ページのツリーに、コメントのあるページ（コメントだけのページを含む）が並び、各ページにコメントの数が出る。
- 狭い画面では、会話パネルのシートからスレッドを開いても同じことができる。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- コメントだけがあるページがページのツリーに出て、コメントの数が出る。表示幅の札を押すと、そのページのその幅に切り替わる
  （[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方) の成功条件 2 のうち、計画①が後回しにしたコメントの部分）。
- 390 で付けたコメントのスレッドを 1280 で開くと付けた幅が出て、押すと 390 に切り替わる。
- ページへのコメントを持つレビューを保留して復元すると、会話パネルとページのツリーにそのコメントが出る（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)
  の成功条件のうちコメントの部分。スナップショットの部分は ④）。
Left to the implementer: 会話パネルの項目と印の見た目（画面モックの案 A に倣う）、`features/live.js` がコメントの一覧を受け取る経路
（`actions.js` か引数。層の規則を守る）、ページへのコメントの絞り込みの名前。
Stop and hand back if: 会話パネルの行コメントの前提（`path` と行で探す）を崩さずにページへのコメントを並べられず、コードへの
コメントの振る舞いを変える必要が出た場合。

## Step 5 — 渡すことと submit を端から端まで

Purpose: 画面で付けたページへのコメントが、エージェントと submit に仕様の形で届くことを確かめる。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（成功条件 1）、
[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)（画像は `kemi wait` でだけ絶対パス）、
[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（確認ダイアログの件数、`page`）。
Prerequisites: Step 4。
May change: `web/assets/` の下、`crates/kemi-server/`、`tests/e2e.rs`、`scripts/test-live.mjs`。
Done when: 画面で要素・矢印・ペンの 3 つの場所を持つコメントを付けて渡すと、`kemi wait` に 3 つの場所と画像の絶対パスが届き、
そのファイルが PNG として読める。submit の JSON に `page` が入り、`image` は `null`。ページの CSP が `style-src 'self'` や
`require-trusted-types-for 'script'` のページでも、コメントと場所は届く（画像は、Step 1 で作れると分かったなら絶対パス、作れないなら `null`）。
Shown by: test — RED → GREEN → REFACTOR。
- ブラウザ自動化と e2e の組み合わせ（`test-live.mjs` から kemi を起動し、画面で 3 つの場所を置いて保存・渡し、別プロセスの
  `kemi wait` の JSON を読む）: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所) の成功条件 1。
- 同じ流れで submit し、`page` が入り `image` が `null`（[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json) の成功条件 2）。
- `/changing.html?csp=1` と `?tt=1` でコメントを付けて渡し、`kemi wait` に場所が届く（保存が止まらない）。
Left to the implementer: `test-live.mjs` から `kemi wait` を呼ぶ方法（`scripts/test-agent-channel.mjs` に倣う）。
Stop and hand back if: なし。

## Step 6 — 文書

Purpose: 利用者とエージェント向けの文書を、ページへのコメントに合わせる。
Specification: [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)。
Prerequisites: Step 5。
May change: `README.md`、`skills/kemi/`、`CHANGELOG.md`、`PROJECT.md`。
Done when: スキルが、`page` を持つコメント（場所の種類・番号・要素の情報・画像のパスは `kemi wait` でだけ、submit では `null`）の
読み方を書いている。README と CHANGELOG の Unreleased が、ページへのコメントと道具を書いている。PROJECT.md の層の節が、足した
モジュールを書いている。
Shown by: check — 次の順に実行する。
1. 各ステップの終わりの検査のすべて
2. `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`
Left to the implementer: 文の書き方（README と `skills/` は英語、ほかは日本語）。
Stop and hand back if: なし。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-COMMENT（画像の作り方） | Step 1（人の確認）、Step 5 |
| R-PAGE-COMMENT（場所・番号・取り消し・矢印・ペン） | Step 2（API の境界）、Step 3 |
| R-PAGE-COMMENT（別の幅で付けたコメント） | Step 4 |
| R-PAGE-COMMENT（成功条件 1: wait に場所と画像） | Step 2（API で代えた e2e）、Step 5（画面から） |
| R-SUBMIT（`page`、各フィールド、`outdated`、`image`） | Step 2、Step 5 |
| R-INPUT（`page` グループ） | Step 2 |
| R-AGENT-EVENTS（画像は wait でだけ） | Step 2、Step 5 |
| R-PAGE-REF（見る対象の側だけ、透かしている間も下に） | Step 3 |
| R-PAGE-VIEW（ツリーのコメントの数、狭い画面の指とタップ） | Step 3（指とタップは人の確認）、Step 4 |
| R-PAGE-SESSION（③ の範囲） | Step 2（e2e）、Step 4（ブラウザ自動化） |
| R-DIST | Step 2（契約を変えるコミットで文書も）、Step 6（仕上げ） |
| R-VERIFY（ページ用のファイルを読み込まない） | 各ステップの終わりの `test-live.mjs` |

## Left to the implementer

- 画像の描き方と切り出し（Step 1）、`Comment` の場所の型と画像を受け取る API の形（Step 2）、操作を受ける場所とセレクタの作りと
  ペンの囲みの決め方（Step 3）、会話パネルの見た目とコメントを受け取る経路（Step 4）。
- モジュールの分け方と名前（`PROJECT.md` の層と features の順の規則を守る）。画面の文言は今の英語の流儀に合わせる。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、保存先、エラーの扱い、上限）を決めないと進めない場合。
- 写しを描いた画像が使えないほど崩れる場合（Step 1）。
- コードへのコメントや今のモードの既存のテストを、仕様を変えずに書き換える必要が出た場合。
- セッション形式の版を上げる必要が出た場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 変化の一覧の項目から「場所に足す」（画面モックにあるが仕様に無い）。会話パネルのページでの絞り込み（仕様に無い）。
- 強制終了などで残った `<id>.files/` の回収（④）。
- スナップショットとモックの割り当ての保存、`<id>.files/` の 20 MB の規則と掃除の大きさの数え方（④）。
- 案（R-PAGE-VARIANT。ライブ改訂の (c)）。画像を見る対象にすること（(d)）。
