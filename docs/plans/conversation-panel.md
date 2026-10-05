# スレッドと発言を右の会話パネルにまとめる

## Goal

画面の右に高さいっぱいの会話パネルを置き、レビュー全体への発言とすべてのスレッドを
「最後に書き込みがあった順」に 1 本で並べる。スレッドを開くとパネル全体がそのスレッドになる。
差分の中のコメントは 1 行の札だけにする。今あるチャット欄・コメント一覧・左下のエージェントの
欄は、会話パネルに一本化する。狭い画面では、会話パネルを画面いっぱいのシートで出す。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [kemi 仕様](../spec/kemi.md)
  - [R-VIEW](../spec/kemi.md#r-view-差分の表示) のうち、コメントの札、会話パネル（並び・絞り込み・
    項目が示すもの・新着の数・開閉と幅）、上部バーの文字ラベルの決まりと、その成功条件・反例
  - [R-NARROW](../spec/kemi.md#r-narrow-狭い画面) のうち、上部バー、会話パネルのシート
    （始まり方を含む）、「Comments」の切り替え、幅をまたいだときの扱いと、その成功条件
  - [R-SERVE](../spec/kemi.md#r-serve-配信モデル) の localStorage の用途
  - [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元) のうち、書き込みの通し番号と、
    版 2 のセッションの読み方
  - [R-LIVE](../spec/kemi.md#r-live-ライブリロード) の例外（会話パネルの並びの追従）と、消えたコミット
  - [R-RENDER](../spec/kemi.md#r-render-描画表示) のうち、描画表示のコメントの置き場
  - [R-NAV](../spec/kemi.md#r-nav-変更間の移動) のうち、コメントを止まる場所にする決まり
  - [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約) の結果 JSON（変えないことを確かめるため）
  - [R-VERIFY](../spec/kemi.md#r-verify-検証)（速さを落とさないことを確かめるため）
- [エージェントとのやりとり 仕様](../spec/agent-channel.md)
  - 冒頭の「見た目の参照」
  - [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（解決と畳み、追従と、その成功条件・反例）
  - [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（状態と「Hand to agent」を
    どこに置き直しても守ること）
  - [R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)（`kemi wait` の JSON を
    変えないことを確かめるため）
- 見た目の参照: [docs/design/ui-mock-conversation.html](../design/ui-mock-conversation.html) の
  **案 A2**。モックと仕様が食い違うときは仕様が正しい。たとえばモックの札は最後の発言を出して
  いるが、仕様では本文の 1 行目を出す。
- 用語は `CONTEXT.md` の「会話パネル」「スレッド」「返信」「発言」「渡す」「シート」。札は
  [R-VIEW](../spec/kemi.md#r-view-差分の表示) の「1 行の札」のこと。「チャット欄」「コメント一覧」は
  会話パネルのことで、新しい文言や識別子には使わない。

## Approach and why

### 順序: 通し番号 → 純粋な判定 → 会話パネル（広い画面とシート）→ 差分の札 → 新着と追従 → 狭い画面の仕上げ → 文書と計測

会話パネルの並びは、書き込みの通し番号が無いと決められない。読み込み直すと並びが変わって
しまう。だから最初にサーバとセッションに通し番号を入れる。
次に、並び・絞り込み・札の中身・新着・追従するかどうかを `web/assets/model.js` の純粋な関数に
する。これは `node --test web` で確かめられ、画面を作る前に形が決まる。

画面は、まず会話パネルを作り、今のチャット欄・コメント一覧・左下の欄を置き換える。
今のチャット欄とコメント一覧は、狭い画面ではそのままシートになっている（`web/index.html` の
`#chat`・`#comment-list`）。だから、それを消すステップで、会話パネルの狭い画面のシートも一緒に
作る。そうしないと、間のコミットで狭い画面からスレッドと発言を読む場所が無くなる。

その後で、差分の中の吹き出しを札に変える。順番を逆にすると、吹き出しを消した時点でスレッドを
読む場所が無くなる。新着と追従はパネルの上に足す。狭い画面の残り（シートからその行へ移る、
「Comments」の切り替え、幅をまたいだとき）は最後にまとめる。

### 通し番号の置き場と出し方

- 今のコメント・返信・発言の番号は、セッションの `last_comment`・`last_reply`・`last_message` で
  別々に数えている（`crates/kemi-server/src/api/comments.rs`、`api/channel.rs`、`api/agent.rs`）。
  これとは別に、レビュー全体で 1 本の通し番号を足す。どの書き手（画面・エージェント）が
  書いても同じ番号の列から振る。
- セッション形式の版は 3 のまま。版 3 はまだリリースしていない（v0.1.9 の `encoding.rs` は版 2）
  ので、版 3 のバイト列に項目を足してよい。版 3 の固定のバイト列を使うテストは無い。
- 開発中に作られた番号の無い版 3 のセッションは、版 2 と同じ規則で番号を振り直して読む
  （[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)）。番号の無い発言は、コメントと返信の
  後ろに作成順で置く。リリースした形ではないので、この扱いはテストで固めない。
- 番号は、submit の結果 JSON（`crates/kemi-server/src/api/submit.rs`、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)）と
  `kemi wait` の JSON（`crates/kemi-server/src/session.rs` の `agent_event_json`、
  [R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)）には出さない。それ以外で
  ページに返す JSON にはすべて載せる。今 `comment_json`・`reply_json`・`message_json` を使っている
  のは次のとおり。
  - `api/review`（`crates/kemi-server/src/api.rs`）
  - `api/file`（`crates/kemi-server/src/api/file.rs`。ページはファイルを開くたびにそのファイルの
    コメントをここから読み直す）
  - `api/comment` と `api/message` の POST の応答（`api/comments.rs`、`api/channel.rs`。新しい
    コメントは SSE では届かず、この応答でしかページに入らない）
  - SSE の通知（`api.rs` の `events`）
  - submit と `kemi wait`（番号を出さない側）

### 会話パネルの組み立て

- 今のチャット欄（`web/assets/views/chat.js`、`#chat`）、コメント一覧
  （`views/comment-list.js`、`features/comment-list.js`、`#comment-list`）、左下のエージェントの欄
  （`#agent-dock`）は、どれも `.layout` の外に `position: fixed` で重ねている。会話パネルは
  `.layout` の grid の 3 列目に置き、上の 3 つは消す。狭い画面では、同じ会話パネルを
  画面いっぱいのシートとして出す。
- 中身は 2 つの状態を持つ。
  - 一覧: 発言とスレッドの項目、絞り込み、全体への発言を書く欄、「Hand to agent」。
  - 開いたスレッド: 対象の行の前後、本文・suggestion・返信、返信の欄、解決・畳む・編集・削除、
    その行へ移る操作。
- エージェントの状態の表示と「Hand to agent」は、パネルの見出しと書く欄に移す。
  出すかどうか・押せるかどうかの条件は [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態) のまま変えない。
- 今のチャット欄が守っていることは、パネルでもそのまま守る。
  - 届いた通知では書く欄を作り直さない（書きかけ・カーソル・変換中の文字を残す）。
  - 発言は足りない分だけ後ろに足す。
  - IME で変換している間は、スレッドの取り込みを後回しにする（`features/agent.js` の
    `whenNotComposing`）。
  - 返信の書きかけは `state.replyDrafts` に残す。
- 開いているか畳んでいるかと、パネルの幅は、localStorage に表示の好みとして覚える。
  localStorage を読み書きするのは `web/assets/storage.js` だけにする（今どおり）。
  覚えた開閉が効くのは広い画面だけ（[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)）。
- 上部バーの入口は、今の `#btn-comments` を会話パネルの開閉に作り替えて使う。id を変えると、
  `scripts/test-narrow-screen.mjs` の (9) と (13) も直すことになる。id を変えるかは実装者に任せる。

### 差分の中の札

- 今の札（`.cchip`）と吹き出し（`.bal`）は `views/comment.js` の `renderCommentOrEditor` →
  `renderComment` が出し分けている。吹き出しをやめて札だけにし、押したら会話パネルでその
  スレッドを開く。
- `state.threads.byLine` は札の置き場として残す。これで、帯の印（`views/nav.js`）、
  止まる場所（[R-NAV](../spec/kemi.md#r-nav-変更間の移動)）、高さの測り直しはそのまま効く。
- ファイル全体へのコメントは `#floating-threads` に札で出す。描画表示（`views/rendered.js`）も
  札で出す。
- 新しく書く入力欄と、編集の入力欄は、今どおり差分の中に出す。開いたスレッドの「編集」は、
  差分の中のそのコメントの位置に編集の入力欄を開く。
- 吹き出しが無くなると、狭い画面で「一覧から選んだ 1 件だけ吹き出しを開く」扱い
  （`state.narrowOnlyComment`、`model.js` の `balloonShown`）も意味を失う。札を出すかどうかは
  「Comments」の切り替えだけで決まる。この整理は札に変えるステップで行う。

### 新着と追従

- 新着の印・帯の新着の数・読み込んだ時点の扱いは、仕様（[R-VIEW](../spec/kemi.md#r-view-差分の表示)）の
  とおり。どこまで見たかはページを開いている間だけ持つ。
- 追従は、会話パネルの並び（一覧と、開いたスレッド）だけに効かせる。差分のスクロール位置は
  変えない（[R-LIVE](../spec/kemi.md#r-live-ライブリロード)）。一番下を見ているかどうかの判定は
  純粋な関数にする。

### 判断した点（仕様が直接書いていないが、仕様から読める範囲で決めたこと）

- 通し番号を振るのは、書き込みを作るときだけ。コメントの本文の編集・解決・削除では振り直さない。
  仕様の「書いた順」を「作った順」と読んだ。編集でスレッドが一番下へ移ると、読み返している
  途中のスレッドが勝手に動くため。
- 開発中の番号の無い版 3 のセッションの読み方（上の「通し番号の置き場と出し方」）。
- 上部バーの入口に `#btn-comments` を作り替えて使う。

## Scope of change

- サーバとセッション: `crates/kemi-core/src/domain/review.rs`、`crates/kemi-core/src/session/`
  （`mod.rs`・`store.rs`・`encoding.rs`）、`crates/kemi-server/src/session.rs`、`crates/kemi-server/src/api.rs`、
  `crates/kemi-server/src/api/`（`comments.rs`・`channel.rs`・`agent.rs`・`file.rs`・`submit.rs`）、
  `crates/kemi-server/src/lib.rs`、`crates/kemi-server/tests/server.rs`、`tests/e2e.rs`
- ページ: `web/index.html`、`web/assets/` の下（`style.css`、`model.js`、`model.test.js`、`state.js`、
  `storage.js`、`dom.js`、`actions.js`、`app.js`、`views/`、`features/`）。モジュールを足す・消す・
  名前を変えるときは、`index.html` の modulepreload の一覧と `PROJECT.md` の層の節も直す。
- ブラウザの検査: `scripts/test-agent-channel.mjs`、`scripts/test-narrow-screen.mjs`、
  `scripts/test-horizontal-scroll.mjs`、`scripts/test-rendered-view.mjs`
- 文書: `CHANGELOG.md`（Unreleased）、`README.md`、`PROJECT.md`、`skills/kemi/`（画面の
  説明があれば）
- 仕様（`docs/spec/`）と `CONTEXT.md` は変えない。モック（`docs/design/`）も変えない。

## Step order and prerequisites

Step 1 → 2 → 3 → 4 → 5 → 6 → 7 の順。どのステップも前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする。

1. `cargo test --workspace`（Rust を変えたとき。`CARGO_BUILD_JOBS=4`）、`node --test web`、
   `npx tsc -p web --noEmit`
2. そのステップの May change にブラウザの検査があるとき: `touch crates/kemi-webview/src/lib.rs` の
   後で `CARGO_BUILD_JOBS=4 cargo build --release` を実行する。続けて、May change に挙げた検査
   （`test-horizontal-scroll.mjs` は偽サーバなので引数なし）を 1 本ずつ回す。

途中のコミットでも、画面から機能が欠けないようにする（Approach の順序の理由）。

---

## Step 1 — 書き込みに通し番号を振り、ページに返す JSON にだけ載せる

Purpose: コメント・返信・発言を、レビュー全体で書いた順に比べられるようにする。
Specification: [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、
[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)、[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)。
Prerequisites: なし。
May change: サーバとセッションのファイル（Scope of change の 1 項目め）。
Done when:
- 画面とエージェントがどの順に書いても、`api/review` が返すコメント・返信・発言の番号の大小が、
  書いた順と一致する。
- 画面で書いたコメント・返信・発言の POST の応答と、`api/file` のコメントにも、同じ番号がある。
- 保留して復元しても番号はそのまま残り、復元した後に書いたものは、前のどれよりも大きい番号になる。
- 版 2 のセッションを読むと、仕様の規則どおりに番号が振られる。
- submit の結果 JSON と `kemi wait` の JSON には、番号が出ない。
Shown by: test — RED → GREEN → REFACTOR。次の振る舞いを 1 つずつテストにする。
- 画面のコメント、エージェントの返信、画面の発言、画面の返信の順に書くと、`api/review` で
  この順に番号が増えている。同じ書き込みの POST の応答と `api/file` の番号が、`api/review` と
  一致する（`crates/kemi-server/tests/server.rs`）。
- 保留して復元した後に書いた返信の番号が、復元前のどれよりも大きい（同上。復元後の返信が
  `r2` になることを確かめている既存のテストに倣う）。
- 版 2 のセッションの番号の振り直し（`crates/kemi-core/src/session/store.rs` のテスト。既存の
  `VERSION_2_SESSION` の固定のバイト列を使い、新しい固定のバイト列は作らない）。
- submit の結果のコメントのキーの集まりが、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約) に
  挙がっている項目と一致する（キーの名前は仕様が契約にしている）。`kemi wait` の `handed` の
  コメントが submit と同じキーを持つことは、既存のテスト
  `a_handed_comment_has_the_same_keys_as_in_the_submit`（`tests/e2e.rs`）が確かめているので、
  そちらは足さない。返信と発言は、既存のテストが丸ごと比べている（返信は `server.rs` の 835 行
  付近、発言は 2718 行付近）。
Left to the implementer: 番号の型と項目名、セッションの中での持ち方、ページ用の JSON と
submit・`kemi wait` 用の JSON を分ける方法。
Stop and hand back if: 版 2 の固定のバイト列を読むテストが、番号を足したことで書き換えを
求める場合（リリースした形の読み方が変わることになる）。

## Step 2 — 会話パネルの並び・絞り込み・札・新着・追従を純粋な関数にする

Purpose: 画面を作る前に、仕様の決まりを試せる形で固める。
Specification: [R-VIEW](../spec/kemi.md#r-view-差分の表示)、[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)。
Prerequisites: Step 1。
May change: `web/assets/model.js`、`web/assets/model.test.js`、`web/assets/state.js`。
Done when: 次の関数があり、テストが通る。
- 並び: 発言とスレッドを、最後の書き込みの番号の順に 1 本にする。返信が足されたスレッドが
  一番後ろへ移る。
- 絞り込み: すべて・未解決・表示中のファイル。
- 札の中身: 本文の 1 行目、返信の数、新着かどうか、畳んでいるか（解決済みの印）。
- 新着: スレッドを最後に開いた時点より後に届いたエージェントの返信があるか。読み込んだ時点で
  あったものは新着にしない。
- 帯の新着の数: 新着の印が付いたスレッドの数と、パネルを畳んでいる間に届いたエージェントの
  発言の数の合計。
- 追従: 一番下を見ているかどうかから、ついていくかを決める。
Shown by: test — `node --test web`。上の振る舞いごとに 1 つ、`model.test.js` にテストを足す。
今の `chip_shows_the_first_line_of_the_body` は残す。
Left to the implementer: 関数の名前と引数の形、一番下とみなす余白の px。
Stop and hand back if: なし。

## Step 3 — 会話パネルを置き、チャット欄・コメント一覧・左下の欄を置き換える

Purpose: 発言とスレッドを読む・書く場所を、会話パネル 1 つにする（広い画面は右の列、
狭い画面は画面いっぱいのシート）。
Specification: [R-VIEW](../spec/kemi.md#r-view-差分の表示)（会話パネル、上部バーの文字ラベル）、
[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（上部バー、会話パネルのシートと始まり方）、
[R-SERVE](../spec/kemi.md#r-serve-配信モデル)（localStorage）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)（消えたコミット）、
[R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)、
[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)。
Prerequisites: Step 2。
May change: `web/index.html`、`web/assets/` の下、`scripts/test-agent-channel.mjs`、
`scripts/test-narrow-screen.mjs`、`PROJECT.md`（層の節）。
Done when:
- 広い画面で、`.layout` の右に会話パネルがあり、既定は畳んだ帯で始まる。上部バーの入口
  （コメント件数つき）と帯の操作で開閉できる。開閉と幅（縁を掴んで変える）は、読み込み直しても残る。
- 一覧には発言とスレッドの項目が並ぶ。スレッドの項目は、グループ単位・パス・行範囲・最後の
  書き込み（コミットごとの項目はコミットの件名も）を示す。絞り込みが効き、全体への発言を書けて、
  「Hand to agent」がある。
- 項目を押すとパネル全体がそのスレッドになる。返信・解決・畳む・編集・削除ができ、その行へ
  グループ単位をまたいで移れる。消えたコミットのコメントは「消えたコミット」と示し、移る操作を
  出さない。
- 狭い画面では、上部バー 1 段目の入口から、画面いっぱいのシートで同じ会話パネルが開き、
  閉じる操作で閉じる。狭い画面で読み込んだときは、覚えた開閉にかかわらず閉じた状態で始まる。
- `#chat`、`#comment-list`、`#agent-dock`、`#btn-chat` と、それを動かすだけのコードと CSS が無い。
- エージェントの状態と「Hand to agent」が出る条件は今と同じ。
- 届いた通知で書く欄（全体への発言・返信）の書きかけと変換中の文字が消えない。
Shown by: test — ブラウザ自動化。
- `test-agent-channel.mjs`: `#chat`・`#btn-dock-chat`・`#agent-dock`・`#btn-chat` を使う検査
  （状態の表示、発言の表示、幅 390px のチャット）を、会話パネルの操作に置き換える。吹き出しの
  中の返信欄を使う検査は、ここでは変えない（吹き出しは Step 4 まで残る）。足すのは次の 2 つだけ。
  - 開閉と幅が読み込み直しても残る（[R-SERVE](../spec/kemi.md#r-serve-配信モデル) の用途に足した決まり）。
  - 一覧の項目からスレッドを開き、返信を書くと、その返信がスレッドに出る。
- `test-narrow-screen.mjs`: (12)（コメント一覧のシート）を会話パネルのシートに書き直し、
  狭い画面で読み込んだときに閉じた状態で始まることを足す。(20)（一覧から選ぶ）は、シートの
  一覧から選ぶ形に置き換える。入口の id を変えたときは (9) と (13) も直す。(13) の並びは
  [R-VIEW](../spec/kemi.md#r-view-差分の表示) の成功条件に合わせる。
Left to the implementer: パネルの部品の分け方とモジュール名（features の順番の規則は守る）、
幅の上限と下限、Escape で閉じる順番の中での会話パネルの位置、入口の id を変えるかどうか。
Stop and hand back if:
- 会話パネルの列を足すと、差分の初回表示が目に見えて遅くなる場合（Step 7 の計測より前に
  気づいたら報告する）。
- 今のチャット欄の守り（書く欄を作り直さない）が、パネルの構造では成り立たない場合。

## Step 4 — 差分の中のコメントを札にし、押すと会話パネルでスレッドを開く

Purpose: コメントを差分の中で必ず見せつつ、本文と会話はパネルで読むようにする。
Specification: [R-VIEW](../spec/kemi.md#r-view-差分の表示)（札、ファイル全体へのコメント、入力欄）、
[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（札と入力欄の幅、「Comments」）、
[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（畳んだスレッド）、
[R-RENDER](../spec/kemi.md#r-render-描画表示)（描画表示の札の置き場）、[R-NAV](../spec/kemi.md#r-nav-変更間の移動)。
Prerequisites: Step 3。
May change: `web/assets/` の下、`scripts/test-agent-channel.mjs`、`scripts/test-narrow-screen.mjs`、
`scripts/test-horizontal-scroll.mjs`、`scripts/test-rendered-view.mjs`。
Done when:
- 行コメント・ファイル全体へのコメント・描画表示のコメントが、すべて 1 行の札で出る。差分の中に
  吹き出しが無い。札に作成者（「あなた」）は出ない。
- 畳んだスレッド（解決したものを含む）の札は 1 行に縮み、解決したものは解決済みの印が付く。
- 札を押すと、会話パネル（畳んでいれば開いて）でそのスレッドが開く。
- 新しく書く入力欄と編集の入力欄は差分の中に出て、書いたコメントは札になる。
- 帯の印と止まる場所は今と同じ場所に出る。
- `state.narrowOnlyComment` と `balloonShown`（`model.js`）が無い。狭い画面で札を出すかどうかは
  「Comments」の切り替えだけで決まる。
Shown by: test — ブラウザ自動化と `node --test web`。
- `test-agent-channel.mjs`: 札を押すとパネルでスレッドが開き、返信の欄と解決が使える
  （今の「吹き出しの中の返信欄と解決」の検査を置き換える）。
- `test-narrow-screen.mjs`: (5)（吹き出しの左端と幅）は札と入力欄の左端と幅に、(19)（札と
  「Comments」）は吹き出しが無い形に書き直す（[R-NARROW](../spec/kemi.md#r-narrow-狭い画面) の成功条件）。
- `test-horizontal-scroll.mjs`: 札を押しても横位置が変わらない検査は残す。吹き出しの
  `.acts` から編集を開いていた検査は、パネルのスレッドの「編集」から開く形に置き換える。
- `model.test.js`: `balloonShown` のテストを消す。帯の印と止まる場所は、今ある
  `placeThreads_*`・`nav_stops` 系・`ruler_marks_*` のテストが通ることで確かめる。新しいテストは足さない。
Left to the implementer: 札の部品の作り方。`state.commentOpen` と `setCommentOpen` を消すか、
別の用途に使い直すか。
Stop and hand back if: 描画表示で札の置き場を変えないと札が出せない場合
（[R-RENDER](../spec/kemi.md#r-render-描画表示) の置き場の決まりに当たる）。

## Step 5 — 新着の印と、会話パネルの並びの追従

Purpose: エージェントの返信がどこに来たか分かり、一番下を見ていればついていくようにする。
Specification: [R-VIEW](../spec/kemi.md#r-view-差分の表示)（新着の印、帯の新着の数、並び）、
[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（追従）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 4。
May change: `web/assets/` の下、`scripts/test-agent-channel.mjs`。
Done when:
- 返信が届いたスレッドが一覧の一番下へ移り、札と一覧の項目に新着の印が付く。
  スレッドを開くと印が消える。
- 畳んだ帯に新着の数が出る。返信や発言が届いてもパネルは勝手に開かない。
- 一番下を見ているときに発言・返信が届くと、並びがそこまで進む。上のほうを見ているときは
  位置を変えず、届いたことを示す印を出す。どちらの場合も、差分のスクロール位置は変わらない。
Shown by: test — ブラウザ自動化（`test-agent-channel.mjs`）の 1 つの検査で、次を順に確かめる。
- パネルを畳んだまま `kemi reply` で返信を書くと、パネルは開かず、札に新着の印が付く。
- パネルを開いてスレッドを開くと、印が消える。
- [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す) の成功条件（一番下を見ているとき・
  上のほうを見ているとき・差分は動かない）。
今の「返信が来ても差分の scrollTop が変わらない」検査はこれに含める。並びの順と帯の数は
Step 2 の純粋な関数のテストで確かめているので、ブラウザでは重ねて確かめない。
Left to the implementer: 「届いたことを示す印」の見た目と文言（モックの「新しい返信 N 件 ↓」に
倣う）。
Stop and hand back if: なし。

## Step 6 — 狭い画面の仕上げ

Purpose: 狭い画面で、シートとその行の行き来、幅をまたいだときの扱いを仕様どおりにする。
Specification: [R-NARROW](../spec/kemi.md#r-narrow-狭い画面)。
Prerequisites: Step 5。
May change: `web/assets/` の下（主に `style.css` の狭い画面の規則、`features/narrow.js`）、
`web/index.html`、`scripts/test-narrow-screen.mjs`。
Done when:
- 狭い画面で札を押すと、シートでそのスレッドが開く。シートのスレッドからその行へ移ると、
  シートが閉じてその行が見える。
- 「…」の「Comments」で札を隠せて、行番号の欄の色の線は残る。
- 幅をまたぐと、再読込なしで配置が切り替わり、シートは閉じる。
Shown by: test — ブラウザ自動化（`node scripts/test-narrow-screen.mjs target/release/kemi`）。
(20) を「シートのスレッドからその行へ移るとシートが閉じる」に書き直す。幅をまたぐ検査に、
シートを開いた状態からまたぐと閉じることを足す（[R-NARROW](../spec/kemi.md#r-narrow-狭い画面) の
幅をまたいだときの決まり）。
Left to the implementer: シートの開閉の仕組み（popover か今のような属性の切り替えか）。
Stop and hand back if: なし。

## Step 7 — 文書を直し、速さと全体を確かめる

Purpose: 利用者向けの文書を今の画面に合わせ、速さを落としていないことを確かめる。
Specification: [R-VERIFY](../spec/kemi.md#r-verify-検証)。
Prerequisites: Step 6。
May change: `CHANGELOG.md`、`README.md`、`PROJECT.md`、`skills/kemi/`（画面の説明があるところだけ）。
Done when:
- CHANGELOG の Unreleased にある「スレッドとチャット欄」の説明が、会話パネルの説明になっている。
- README の「Threads (reply, resolve), the chat, ...」の段落が、会話パネルの説明になっている。
- スキルに画面の置き場の説明があれば、直してある。
- 起動の計測が、main（この計画の前）と比べて目に見えて遅くなっていない。
Shown by: check — 次の順に実行する。
1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
   `npx tsc -p web --noEmit`
2. `CARGO_BUILD_JOBS=4 cargo test --workspace`、`node --test web`
3. `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`（スキルを変えたとき）
4. `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release`
5. ブラウザの検査を 1 本ずつ: `test-agent-channel.mjs`、`test-narrow-screen.mjs`、
   `test-rendered-view.mjs`、`test-horizontal-scroll.mjs`
6. `scripts/gen-fixture.sh` で作ったフィクスチャに対して、`scripts/measure-startup.sh` を
   この計画の前の main のバイナリとこのブランチのバイナリで回し、結果を並べて報告する
   （[R-VERIFY](../spec/kemi.md#r-verify-検証) の 1 秒の基準を守っていること）。
Left to the implementer: フィクスチャの規模（R-VERIFY の目標の規模に合わせる）。
Stop and hand back if: 計測で、このブランチが main より目に見えて遅い場合（原因を調べる前に報告する）。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-SESSION（通し番号、復元後の続き、版 2 の読み方） | Step 1 |
| R-SUBMIT・R-AGENT-EVENTS（番号を出さない） | Step 1（キーの集まりのテストを 1 つ足し、ほかは既存のテスト） |
| R-VIEW（会話パネル、並び、絞り込み、項目が示すもの、上部バー） | Step 2、Step 3 |
| R-VIEW（札、ファイル全体へのコメント、入力欄、作成者を出さない） | Step 2、Step 4 |
| R-VIEW（新着、帯の数、勝手に開かない） | Step 2、Step 5 |
| R-SERVE（localStorage の用途） | Step 3 |
| R-LIVE（消えたコミット） | Step 3 |
| R-LIVE（差分は動かない） | Step 5 |
| R-AGENT-STATE（状態と Hand の条件） | Step 3（既存の検査を置き換えて維持） |
| R-AGENT-HAND（解決と畳み） | Step 2、Step 4 |
| R-AGENT-HAND（追従） | Step 2、Step 5 |
| R-RENDER・R-NAV | Step 4 |
| R-NARROW（シート、始まり方） | Step 3 |
| R-NARROW（札、「Comments」） | Step 4、Step 6 |
| R-NARROW（その行へ移る、幅をまたぐ） | Step 6 |
| R-VERIFY | Step 7 |

## Left to the implementer

- モジュールの分け方と名前（`PROJECT.md` の層と features の順番の規則は守る。新しい feature を
  足すなら、その順番の中の位置も `PROJECT.md` に書く）。
- CSS のクラス名と、見た目の細部（モックの案 A2 に倣う）。
- 画面の文言は今の英語の流儀に合わせる（例: "Chat" や "Comment list" に代わる名前）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、保存先、エラーの扱い）を決めないと進めない場合。
- 既存のテストのうち、仕様の決まりを確かめているものを、仕様を変えずに書き換える必要が出た場合。
- 速さの計測で目に見えて遅くなった場合（Step 7）。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- `--live` のページの見方（[live.md](../spec/live.md)）と、そこでの会話パネルの置き方。
- 案（variants）の表示。
- エージェントの状態の判定・`kemi wait` / `kemi reply` の振る舞い（変えない）。
- 吹き出しの見た目の参照だった `docs/design/ui-mock-v2.html` の手直し。
