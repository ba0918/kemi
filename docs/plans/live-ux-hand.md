# --live の使い勝手 ②: コメントと渡す流れ

## Goal

ページへのコメントの場所の番号と `#n` の参照、意味のある要素だけを渡す場所、保存済みの印を仕様どおりにし、「エージェントに渡す」を
いつも見えるようにして、渡した後の受け取り待ち・作業中・返事済みと、別のスレッドの新着が画面で分かるようにする（全入力モード）。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（`#n`、番号を押して入れる、要素の道具は `html` / `body` を選ばない、
    矢印の先とペンの範囲、詰め直し、消した場所を指す参照、ページの上の印）
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)のうち、狭い画面の「道具と『エージェントに渡す』は画面の下に浮かぶツールバー」
- [エージェントとのやりとり 仕様](../spec/agent-channel.md)
  - [R-AGENT-FLOW](../spec/agent-channel.md#r-agent-flow-往復の流れ)（`kemi wait` の前の「エージェントに渡す」）
  - [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（渡した 1 回分の行と `data-kemi-hand-line`、別のスレッドの新着、パネルの見出しの新着の数）
  - [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（5 つの状態と移り方の表、`data-kemi-agent-state`、「エージェントに渡す」を常に出す、
    `kemi wait <id>` を写す操作、渡すものが無いときの見た目、狭い画面の浮かぶボタン、スキル・README・CHANGELOG を同じコミットで直す）
  - [R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)（`kemi wait` の JSON。場所の形は R-SUBMIT と同じ）
- [kemi 仕様](../spec/kemi.md): [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（`places` の連番、空の `elements`、本文の `#n`、確認に出す未渡しの件数）、
  [R-VIEW](../spec/kemi.md#r-view-差分の表示)（帯の新着の数の数え方）、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（画面の下に浮かぶ「エージェントに渡す」）、
  [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-VERIFY](../spec/kemi.md#r-verify-検証)
- 見た目の参照: [docs/design/ui-mock-live-v2.html](../design/ui-mock-live-v2.html)（状態 2・3・9・11）。会話パネル全体は
  [docs/design/ui-mock-conversation.html](../design/ui-mock-conversation.html)。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md`。

## 3 つの計画の中の位置

使い勝手の見直しは 3 つに分けた。① 見方と画面の骨組み（済み、main にマージ済み）、**② この計画（コメントと渡す流れ）**、③ 見比べの中身と
モック（変化の一覧の作り直し、比べる相手の名前を一覧の見出しに出す、消えた要素の行、Mock file… の一覧と API、Record now の切り替えと知らせ、
場所の一覧の行に乗せると光る、「ページで見る」、書く欄を閉じた後の枠の高さ、Remove mock の Undo などの小さな直し）。リリースは 3 つとも終えてから。

## Approach and why

### 今の作り（調べた事実）

場所の番号:
- 書きかけの場所は `web/assets/live-model.js` の `PlaceDraft = { url, width, places, next }`。`addPlace` は `n: next` を振って `next` を進め、
  `removePlace` は除くだけ、`undoPlace` は `n` が最大のものを除く。番号は振り直さない（コメントにもそう書いてある）。
- 書く欄（`views/live.js` の `buildCompose`）の `textarea.lv-compose-body` は一度だけ作られ、描き直しで作り直されない。本文の `#n` を読む処理は
  どこにも無い。場所の一覧（`views/live.js` の `renderPlaces`）の番号は `span.lv-place-n` で、押しても何も起きない。
- 保存は `features/live.js` の `savePageComment` → `{op:"add_page", page:{url,width,places}, body}`。保存の可否は `renderCompose` の disabled の条件。
- サーバは `crates/kemi-core/src/domain/comment.rs` の `validate_places`（空でない・`n` の順に並べる・0 と重複を断る。欠番は許す）。
  submit と `kemi wait` の JSON は `crates/kemi-server/src/session.rs` の `page_json` で同じ形。

`html` / `body`:
- 場所の解決は `web/live/page.js` の `resolvePlace`。要素の道具と矢印の先は `elementAt` で、`html` / `body` を除かない。ペンは `enclosedElements` で
  `PAGE_ROOTS` を除くが、残りが 0 件なら `innermost.element ?? document.documentElement` を返す。地を選んだ要素の場所のために、`at`・`onlyRootAt`・
  `placeAnchor` の `at` の分岐・`imageArea` の `at` がある。一覧の表示は、ペンは常に `"<数> element(s) inside"`、要素と矢印は先頭の要素が無ければ `"no element"`。
- 画像はコメントに 1 枚（`placesImage`）。場所ごとの画像は無い。

ページの上の印:
- `features/live.js` の `sendPlaces` が `{places, look}` の組を page.js に送る（`look` は `draft`・`focus`（開いているスレッド）・`saved`）。
  page.js の `PLACE_LOOKS` は 3 つとも `numbers: true`。番号の札は `placeShapes` が作り、どのコメントかを示すものは何も送っていない。

エージェント:
- 状態は `crates/kemi-core/src/domain/agent.rs` の純粋な関数 `agent_status(called, waiting, last_activity, now)` と `AgentStatus`
  （`unconnected`・`waiting`・`working`・`unresponsive`）。実行時の値は `crates/kemi-server/src/lib.rs` の `AgentRuntime { waiting, last_activity }`
  （メモリだけ）。画面へは `api/channel.rs` の `agent_json`（`{called, status, unhanded}`）を SSE の `agent` で送り、15 秒ごとの `start_status_ticker` が
  時刻だけで変わる分を拾う。
- 渡した 1 回分は `Channel.events: Vec<AgentEvent>`（`Handed { comments: Added|Edited|Deleted(id), replies: {comment_id, reply}, messages }`）。
  `kemi wait` は今ある events を全部 1 つの応答で返し、受け取りの知らせ（`received_api`）で `events.drain` する。返した 1 回分をサーバは覚えていない。
  events はセッションに保存され（`ChannelDto`）、`received` はメモリだけで復元時は 0。
- 画面の状態は `views/conversation.js` の `renderAgentState` が `#agent-status` と `#rail-status` の `data-status` に書き、`--live` の上部バーと帯は
  `features/live.js` の `mirrorAgentState`（`data-status` を見張る MutationObserver）で写す。`data-kemi-agent-state` と `data-kemi-hand-line` はまだ無い。
- 「Hand to agent」は `#rail-hand`（畳んだ帯）と `#btn-hand`（パネルの中）。`model.js` の `handShown(agent)` が `agent.called` のときだけ出し、
  `unhandedNotice`（submit の確認の件数）も `handShown` を使う。押すと `features/agent.js` の `handToAgent`。
- レビューの `id` は `src/main.rs` の `start_agent_channel` の中の変数と stderr の `kemi: review <id>` だけで、ページには届かない。エージェントの API は
  セッションのあるレビューでだけ立つ（立てられないときは警告して submit だけ）。クリップボードへ写す既存の処理は `features/files.js` の `copyPath`。
- 新着: `model.js` の `unreadCount` を帯の `#cv-unread` にだけ出す。別のスレッドに返信が届いても `threadUnread` が立つだけで知らせは無い。
- 狭い画面で会話パネルを閉じている間は `.conversation { display: none }` で、`#rail-hand` も見えない。`--live` の道具（`.lv-tools`）は見る対象の枠の上の
  ツールバー（`.lv-toolbar`）にある。

### やり方（計画で決める）

- **詰め直しと `#n` は `live-model.js` の純粋な関数で決める。** 場所を消す・最後を取り消す・同じ要素を選び直して外す、の 3 つとも、残った場所を置いた順に
  1 から振り直し、本文の `#n` を同じ対応で書き換えた本文も返す。`next` は要らなくなる。
- **宙に浮いた参照（消した場所を指していた `#n`）は、本文の中の範囲として覚え、編集に合わせて動かす。** 詰め直すと、宙に浮いた `#1` が詰め直した後の
  場所 1 と同じ字面になりうる（例: 場所 1・2、本文 `#1 #2` で 1 を消すと本文は `#1 #1` で、前の `#1` は宙に浮いている）。字面だけでは見分けられないので、
  書きかけの状態に宙に浮いた範囲の一覧を持ち、本文の編集（前後の一致する部分を比べて求めた変わった区間）に合わせて範囲をずらし、範囲の中を編集・削除したら
  その範囲を外す。範囲が 1 つでも残っている間は、その箇所を示して保存を押せない。宙に浮いた範囲は、その後の詰め直しで書き換えない。範囲を動かす計算も
  純粋な関数にする。本文の字面は人が書いたまま残す（仕様の成功条件は「本文から `#1` を消すと押せる」で、字面を書き換えない）。
- **サーバも連番を守る。** `validate_places` は、`n` が 1 からの連番でない `places` を断る（足す API は 400）。連番は submit と `kemi wait` の JSON の約束
  （[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)）で、その入口は足す API だけ（セッションの読み戻しは `validate_places` を通らない）。足す API は
  ポートに届く誰でも呼べる境目なので、画面だけに任せない。
- **`#n` の読み取り:** `#` に続く 1 個以上の数字で、直後が数字でないもの。`#` の付かない数字は参照ではない（仕様）。`##3` や `#03` のような形の扱いは
  仕様に無いので、`#` の直後から数字の続く限りを番号として読む（`#03` は 3）。書き換えでは元の書き方を保たず `#<新しい番号>` にする。
- **`html` / `body` は page.js の `resolvePlace` で落とす。** 要素の道具で `html` / `body` に当たったら場所を作らない（今の「要素が無い」と同じ扱い）。
  矢印の先が `html` / `body` なら `elements: []`。ペンで意味のある要素が無ければ `elements: []`（範囲だけ）。これで使われなくなる地のための分岐
  （`at`、`onlyRootAt`、`placeAnchor` の `at`、`imageArea` の `at`）は消す。一覧は要素の無い場所を「範囲だけ」の行として出す。
- **保存済みの印:** page.js の `saved` の見た目を番号なしの小さな印にする。開いているスレッド（`focus`）と書きかけ（`draft`）は今どおり番号付き。
  「触れるとどのコメントか分かる」は、ページの中の `title` では出せない（page.js の印の層は `pointer-events: none` で、場所を置く道具の間はレビュー画面の
  `capture` 層が枠を覆っていて、ポインタがページに届かない）。そこで、レビュー画面がポインタの位置（ページの座標）を page.js に尋ね、page.js がその位置に
  ある保存済みの印のコメントの id を返し、レビュー画面が自分の UI にそのコメントの短い名前（仮決め 7）を出す。操作の道具の間（ポインタがページに届く）は、
  page.js がポインタの動きを見て同じ答えを知らせる。page.js へ送る組には、コメントの id を足す（本文はページに送らない）。
- **渡した 1 回分の行はサーバで持つ。** 状態の表の「残りの行」が状態を決めるので、状態を計算するサーバ（`agent_status`）が行を知っている必要がある。
  `AgentRuntime` に行の表（スレッドの id → `pending` / `working`、発言だけの 1 回分は「並びの末尾」の 1 つ）を足す。
  - 渡したとき: その 1 回分に入ったスレッド（足した・編集したコメント、返信の `comment_id`）の行を `pending` にする。スレッドを 1 つも含まず発言だけを
    含む 1 回分は、末尾の行を `pending` にする。消したコメント（`Deleted`）は行を付けない。
  - `kemi wait` が 1 回分を返したとき: その応答で返した 1 回分の行のうち、今 `pending` で残っているものだけを `working` にする（仮決め 3）。行を新しく
    作ったり、消えた行を戻したりはしない（同じ 1 回分が二度返ることがある。[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)）。
  - エージェントの返信が届いたとき: そのスレッドの行を消す。エージェントの発言が届いたとき: 末尾の行を消す。
  - 状態の「残りの行」のために、行の表とは別に「最後に返った応答の行」の集合を持つ。1 回分を返した応答のたびにその応答の行で置き換え、時間切れで返ったときと
    待っていた接続が切れたとき（1 回分を返さずに終わったとき）は空にする。「残りの行」は、この集合の行のうち、まだ `working` で残っているもの
    （消えた行と、渡し直して `pending` に戻った行は数えない。仮決め 5）。前の応答の `working` の行は、表示は残るが状態には数えない。
  - 行は `agent_json` に載せて SSE で送り、画面は会話パネルのスレッドの末尾・並びの末尾に `data-kemi-hand-line` を付けて描く。
- **状態の値は契約の名前にそろえる。** `AgentStatus` の値・`agent_json` の `status`・CSS・ラベル表・`mirrorAgentState` を `not-connected`・`waiting`・`working`・
  `replied`・`no-response` にし、画面は `data-status` の代わりに `data-kemi-agent-state` を出す（同じものに 2 つの名前を持たせない）。状態の移り方は
  `agent_status` に「残りの行があるか」と「返事済みになったか」を入れる純粋な関数にし、表の各行を単体テストで決める（時刻は注入）。
- **レビューの `id` は `AgentParams` からサーバの状態を通して `agent_json` に載せる。** 「エージェントに渡す」は状態によらず出し、未接続の間は押せないまま、
  `kemi wait` を始めると渡せることとコマンドを写す操作を出す。写すのは `kemi wait <id>`。エージェントの API が立っていないレビューは、
  [R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply) のとおり一度もつながらないレビューと同じに扱い、ボタンは押せないまま出す
  （仮決め 6）。
  `unhandedNotice` は `handShown` ではなく「一度でも `kemi wait` が呼ばれた」（`agent.called`）で決める（[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約) の確認の条件）。
- **新着:** 別のスレッドに返信が届いたら、開いているスレッドの上に知らせを出し、押すとそのスレッドを開く。パネルの見出しには `unreadCount` と同じ数え方の
  数を出す。
- **狭い画面の浮かぶ「エージェントに渡す」**は会話パネル（`aside`）の外に置く（閉じたパネルは `display: none` で中身ごと消えるため）。全モード共通の
  部品は `index.html` と `views/conversation.js`・`features/agent.js` の側、`--live` のページの見方では道具と一緒の浮かぶツールバーを `views/live.js`・
  `features/live.js` が作る（`--live` でないレビューがページ用のファイルを読まない約束、[R-VERIFY](../spec/kemi.md#r-verify-検証)）。
- **契約の文書は、契約を変えるコミットで一緒に直す**（[R-DIST](../spec/kemi.md#r-dist-名称と配布)、仕様の注記）。どのステップでどの文書を直すかは各ステップに書く。

### 仮決め（仕様が決めていないところ。覆す条件つき）

1. 本文の `#n` のうち、今の場所の数を超える番号（場所が 2 つで `#9` など）も宙に浮いた参照と同じく示し、保存を押せなくする。理由: どの場所も指さない参照が
   渡ると、エージェントが別の場所を探して直しかねない（仕様の反例と同じ害）。覆す条件: 「場所を置く前に先に番号を書くこともあるので、超える番号は止めないで」。
2. 保存済みのコメントの本文を後から編集するとき（会話パネルの `pageCommentEditor`）は、`#n` を調べない。理由: 仕様の「直すまで保存できない」は場所を消した
   ときの規則で、保存済みのコメントの場所は消せない。覆す条件: 「編集でも存在しない `#n` を書いたら保存させないで」。
3. 1 回の `kemi wait` の応答が複数の 1 回分をまとめて返したときは、その応答で返したすべての 1 回分の行をまとめて「最後に返った 1 回分」とみなす。理由:
   エージェントはその応答の全部を受け取って作業するので、どれかだけを作業中にすると表示が実際とずれる。覆す条件: 「最後の 1 回分だけを作業中にして」。
4. 渡した 1 回分の行と「最後に返った応答の行」はメモリだけで持ち、保留と復元をまたがない。復元したときは、まだ受け取られていない events（セッションに
   保存されている）から `pending` の行を作り直し、`working` の行は作らない。理由: `kemi wait` が返したかどうか（`received`）は今もメモリだけで、復元で
   作業中の行だけを戻すと根拠の無い表示になる。覆す条件: 「復元しても作業中の行を戻して」。
5. 状態の「残りの行」には、作業中に同じスレッドをまた渡して `pending` に戻った行を数えない。理由: その行はまだエージェントが受け取っておらず、返事を待つ
   相手ではない。覆す条件: 「渡し直した行が残っている間も作業中のままにして」。
6. エージェントの API が立っていないレビュー（セッションが無い、API を立てられなかった）でも「エージェントに渡す」を押せないまま出す
   （[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply) の「一度もつながらないレビューと同じ」に従う）。ただし写す `id` が無いので、
   コマンドを写す操作は出さず、案内は「このレビューにはエージェントがつながれない」ことを言う。理由: 写したコマンドが動かない案内は出せない。
   覆す条件: 「その場合はボタンごと出さないで」（仕様の直しが要る）。
7. 保存済みの印に触れたときの「どのコメントか」は、本文の 1 行目を 40 文字ほどに切ったものを、レビュー画面の側に出す。理由: モックはこの部分を描いていない。
   覆す条件: 「番号やスレッドの見出しなど別のものを出して」。
8. 復元したレビューで、一度でも `kemi wait` が呼ばれていたものは、今どおり作業中で始まる（返事済みだったかは戻さない）。状態の表に復元の行は無い。理由:
   仮決め 4 のとおり行も戻さないので、返事済みと決める根拠が無い。覆す条件: 「復元したら返事済みで始めて」。

## Scope of change

- Rust: `crates/kemi-core/src/domain/agent.rs`（状態と行）、`crates/kemi-core/src/domain/comment.rs`（テストの名前だけ変わることはある）、
  `crates/kemi-server/`（`lib.rs` の `AgentRuntime` と `AgentParams`、`api/agent.rs`、`api/channel.rs`、`api.rs`、`session.rs`）、`src/main.rs`（`id` を渡す）
- ページ: `web/index.html`、`web/assets/` の下（`live-model.js`・`model.js`・`features/live.js`・`features/agent.js`・`features/conversation.js`・
  `views/live.js`・`views/conversation.js`・`live.css`・`style.css`・`state.js`）、`web/live/page.js`
- テスト: `web/assets/*.test.js`、`scripts/test-live.mjs`、`scripts/test-agent-channel.mjs`、`scripts/test-narrow-screen.mjs`、各クレートのテスト、`tests/e2e.rs`
- 文書: `skills/kemi/`（`SKILL.md`、`references/conversation.md`・`results.md`・`inputs.md`）、`README.md`、`CHANGELOG.md`（Unreleased）、`PROJECT.md`
  （モジュールや層の説明が変わるとき）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/` は変えない。

## 既存の検査の扱い

仕様が変えた振る舞いを確かめている次の assert は、新しい仕様に合わせて書き換えてよい。それ以外の assert を変える必要が出たら止まる。

- 番号を振り直さない: `web/assets/live-model.test.js` の「removing a place keeps the other numbers and its number is not given again」と、番号・取り消しの
  検査（新しい振る舞いで通るなら残す）、`scripts/test-live.mjs` の `pageCommentPlacesArePutAndSaved`（2 を消すと `[1,3]`）。
  `crates/kemi-core/src/domain/comment.rs` の `page_comment_places_are_kept_in_number_order_with_gaps` と `crates/kemi-server/tests/live.rs` の
  `a_page_comment_is_added_in_the_page_group_with_its_places_in_number_order`（欠番を受け付ける）は、連番でない `places` を断る形に書き換える
  （「やり方」のサーバも連番を守る）。
- 地を選ぶと根が要素になる: `scripts/test-live.mjs` の `placesOnTheBackgroundNameThePage`（と冒頭の説明のコメント）は、要素の道具で場所が増えない・
  矢印とペンは `elements: []` を確かめる形に書き換える。
- 状態の名前と 4 つの状態: `agent.rs` の `status_is_*` の単体テスト、`crates/kemi-server/tests/server.rs` の状態の文字列の比較、`tests/e2e.rs` の状態を待つ
  ヘルパー、`web/assets/model.test.js` の 4 つの状態の検査、`scripts/test-agent-channel.mjs` の `statusIs` と `'unconnected'`、`scripts/test-live.mjs` の
  上部バーと帯の状態の一致の検査。値の名前と、`data-status` から `data-kemi-agent-state` への置き換えだけを変えてよい。
  `crates/kemi-server/tests/server.rs` の復元後に `working` を確かめる検査は、仮決め 8 のとおり値の名前のほかは変えない。
- `kemi wait` の前に「Hand to agent」が無い: `web/assets/model.test.js` の「kemi wait が一度でも呼ばれたレビューでだけ「Hand to agent」を出す」、
  `scripts/test-agent-channel.mjs` の wait 前に `#rail-hand` / `#btn-hand` が見えないことの検査、`scripts/test-live.mjs` の `handInThePage` / `pressHand`
  （見える方のボタンを探す前提）。「出ていて押せない」を確かめる形に書き換える。`model.test.js` の submit の確認の件数の検査は、条件が
  `agent.called` のまま通るはずなので変えない。

## Step order and prerequisites

Step 1 → 2 → … → 8 の順。各ステップは前のステップが終わっていることを前提にする（Step 1〜3 の場所の話と Step 4〜8 の渡す流れは互いに独立だが、
順に進める）。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`
3. ブラウザの検査があるステップ: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   `node scripts/test-live.mjs target/release/kemi`、`node scripts/test-agent-channel.mjs target/release/kemi`、
   `node scripts/test-narrow-screen.mjs target/release/kemi`、`node scripts/test-rendered-view.mjs target/release/kemi`、
   `node scripts/test-horizontal-scroll.mjs` を 1 本ずつ回す。

---

## Step 1 — 場所の番号の詰め直しと `#n`

Purpose: 場所を消しても番号が 1 からの連番のままで、本文の `#n` が同じ場所を指し続けるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（`#n`、番号を押して入れる、詰め直し、消した場所を指す参照）、
[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（連番、本文の `#n`）、[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: なし。
May change: `web/assets/live-model.js`、`web/assets/live-model.test.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live.css`、
`crates/kemi-core/src/domain/comment.rs`、`crates/kemi-server/`（足す API とそのテスト）、`scripts/test-live.mjs`、`skills/kemi/`、`README.md`、`CHANGELOG.md`。
Done when:
- 場所を消す・最後を取り消す・同じ要素を選び直して外すと、残った場所が置いた順に 1 から振り直され、本文の `#n` が同じ対応で書き換わる。
- 消した場所を指す `#n`（詰め直した後の番号と同じ字面になったものも）と、場所の数を超える `#n` が本文にあると、その箇所が示され、保存を押せない。
  本文から直すと押せる。
- サーバの足す API は、`n` が 1 からの連番でない `places` を断る。
- 場所の一覧の番号を押すと、本文の入力位置に `#n` が入る。本文の欄の案内（placeholder）が `#n` で指すことを言う。
- 渡した JSON の `places[].n` が 1 からの連番で、`body` は書き換えた後のもの。
- `skills/kemi/references/results.md` の「numbers may skip」、`inputs.md` の番号での参照、README の Live review の節と JSON の説明（「never renumbers」
  「possibly with gaps」）、CHANGELOG の Unreleased の該当行が、連番と `#n` を書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR。
- 単体テスト（`node --test web`）: 3 つの場所と本文 `#1 と #3 と 3` から 2 番目を消すと、番号が 1 と 2、本文が `#1 と #2 と 3` になる。最後の取り消しと
  選び直しでも同じく詰まる。場所 1・2 と本文 `#1 #2` から 1 番目を消すと、本文が `#1 #1` になり、前の `#1` だけが宙に浮いた範囲になる。宙に浮いた範囲は、
  その前に文字を足すとずれて残り、範囲の中を消すと外れる。場所の数を超える `#9` は宙に浮いた参照として返る（仮決め 1）。
- サーバの単体テスト（`crates/kemi-core` の `validate_places`）: `n` が 1 と 3 の `places` を断り、1 と 2 は受け付ける（足す API は、ポートに届く誰でも呼べる境目）。
- ブラウザ自動化（`scripts/test-live.mjs` の `pageCommentPlacesArePutAndSaved` を書き換え、足す）: 3 つ置いて本文に `#1 と #3 と 3` と書き、2 番目を消すと
  一覧の番号が 1 と 2、本文が `#1 と #2 と 3`。保存して渡し、`kemi wait` の JSON の `n` が 1 と 2、`body` が書き換えた後のもの。別の書きかけで `#1` を
  書いて 1 番目を消すと保存が押せず、本文から `#1` を消すと押せる（この書きかけには場所を 2 つ置いておく。場所が 0 になると、それだけで保存は押せない）。
  一覧の番号を押すと本文に `#n` が入る。
Left to the implementer: 関数の名前と形、宙に浮いた参照の示し方（見た目はモックの状態 2 に倣う）、文書の文の書き方（skills と README は英語、CHANGELOG は日本語）。
Stop and hand back if: なし。

## Step 2 — 要素の道具は `html` / `body` を選ばず、範囲だけの場所を作る

Purpose: エージェントに意味の無い `html` とページの全文を渡さないようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（要素の道具、矢印の先、ペンの範囲だけ）、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（空の `elements`）、
[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 1。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live-model.js`、`web/assets/*.test.js`、`scripts/test-live.mjs`、
`skills/kemi/`、`README.md`、`CHANGELOG.md`。
Done when:
- 要素の道具で `html` か `body` にしか当たらない所を押しても、場所が増えない。
- 先が `html` / `body` の矢印と、意味のある要素を囲まないペンは、`elements: []` で `points` を持つ。どちらもコメントの画像は 1 つで、場所ごとの画像の項目は無い。
- 場所の一覧で、要素の無い場所は範囲だけの行として出る（要素が出ない）。
- 地のために使っていた分岐（`at` など）が無い。
- `results.md`（矢印とペンの `elements`）、README、CHANGELOG が空の `elements` と範囲だけを書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`。`placesOnTheBackgroundNameThePage` を書き換える）。
- 要素の道具でページの余白を押しても場所が増えない。
- 余白だけをペンで囲み、余白を指す矢印を置いて保存して渡すと、`kemi wait` の JSON のその場所の `elements` が `[]` で `points` を持ち、コメントの `image` が 1 つある。
- 範囲だけの場所の行に要素が出ない。
Left to the implementer: 範囲だけの行の文言（英語。モックに倣う）。
Stop and hand back if: 要素の道具の当たり判定を変えると、要素を選べるはずの所（`body` の直下の小さな要素など）で選べなくなる場合（仕様が決めていない境目）。

## Step 3 — 保存済みのコメントの印

Purpose: 書きかけの場所と保存済みのコメントの場所を見分けられるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（ページの上の印）。
Prerequisites: Step 2。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live.css`、`scripts/test-live.mjs`、`README.md`、`CHANGELOG.md`。
Done when:
- 保存済みのコメントの場所は、そのスレッドを開いていない間は番号の無い小さな印。印にポインタを乗せると、どの道具を選んでいても、レビュー画面にそのコメントの
  短い名前が出る（「やり方」の保存済みの印、仮決め 7）。
- そのスレッドを開いている間は番号付きで出る。書きかけの場所はいつも番号付き。
- README の Live review の節（「quiet numbered marks」）と CHANGELOG が、番号の無い保存済みの印を書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- c1 を保存してから新しいコメントを書き始めると、c1 の場所に番号が出ず、書きかけの場所にだけ番号が出る。c1 のスレッドを開くと c1 の場所に番号が出る
  （番号の札があるかは page.js の中の印の DOM で確かめる。ページの中の印の作りは試験用の観測で、契約ではない）。
- 要素の道具のまま c1 の印の位置へポインタを動かす（レビュー画面の `capture` 層にポインタの動きのイベントを送る）と、レビュー画面に c1 の短い名前が出る。
Left to the implementer: 印の大きさと形（モックに倣う）。
Stop and hand back if: なし。

## Step 4 — エージェントの状態の 5 つと渡した 1 回分の行（サーバ）

Purpose: 「返事済み」と、渡した 1 回分ごとの受け取り待ち・作業中を、サーバが表のとおりに決めて画面へ送る。
Specification: [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（状態と移り方の表、属性の値）、[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（渡した 1 回分の行）、
[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 3。
May change: `crates/kemi-core/`、`crates/kemi-server/`、`tests/e2e.rs`、`web/assets/`（値の名前と `data-kemi-agent-state` の置き換え、返事済みのラベルと見た目、
待機中のラベルの言い方）、`web/assets/*.test.js`、`scripts/test-agent-channel.mjs`、`scripts/test-live.mjs`（値の名前と属性だけ）、
`skills/kemi/references/conversation.md`、`README.md`、`CHANGELOG.md`。
Done when:
- 状態は `not-connected`・`waiting`・`working`・`replied`・`no-response` の 5 つで、表の各行のとおりに移る（仮決め 3・5）。返事済みは何分たっても応答なしにならない。
  復元したレビューは仮決め 8 のとおり。
- 行の表（スレッド → `pending` / `working`、発言だけの 1 回分の末尾の行）を「やり方」のとおりに保ち、`agent_json` に載せて SSE で送る。復元したときは
  仮決め 4 のとおり。
- 画面は状態を `data-kemi-agent-state` に出し（`#agent-status`、帯、`--live` の上部バーと帯の写し）、ラベルと見た目に返事済みがある（待機中は渡せること、
  返事済みは返事が届いたことが読み取れる言い方。文言はモック）。
- `conversation.md` の状態の説明、README、CHANGELOG が 5 つの状態を書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR。属性の名前を変えるので、このステップの終わりにはブラウザの検査（各ステップの終わりの検査の 3）も回す。
- Rust の単体テスト（`agent.rs`。時刻は注入）: 状態の表の各行を 1 つずつ。作業中のまま 10 分で応答なし、返事済みのまま 10 分でも返事済み。
  前の応答の `working` の行が残っていても、最後に返った応答の行に全部返事が来たら返事済み。一部に返事が来た後で次の `kemi wait` が時間切れで返り、
  その後に返信が届いたら返事済み。
- Rust の単体テスト: 行の表の移り方（渡すと `pending`、返すと `working`、返信でそのスレッドだけ消える、発言で末尾の行が消える、渡し直すと `pending`、
  1 応答で複数の 1 回分を返すと全部 `working`、同じ 1 回分が二度返っても返信で消えた行は戻らない、消したコメントは行を持たない）。
- サーバのテスト（`crates/kemi-server/tests/server.rs`）: 渡して `kemi wait` が返った後に c1 にだけ返信すると `agent_json` の状態が `working`、c2 にも返信すると
  `replied`。返事済みの間に渡すと `replied` のままで、行が `pending` になる。
Left to the implementer: 行の表の型と置き場所（`AgentRuntime` の中、ドメインの純粋な関数にする部分）、`agent_json` の中の行の形（画面とサーバの間だけの形で、
契約ではない）。
Stop and hand back if: 表の状態をサーバだけで決められない（画面の情報が要る）と分かった場合。

## Step 5 — 渡した 1 回分の行（画面）

Purpose: 渡した後に、どのスレッドがエージェントの受け取り待ちか作業中かが画面で分かるようにする。
Specification: [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（渡した 1 回分の行）、[R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（成功条件のブラウザ自動化）。
Prerequisites: Step 4。
May change: `web/assets/`（`views/conversation.js`・`features/agent.js`・`model.js`・`style.css` など）、`web/assets/*.test.js`、`scripts/test-agent-channel.mjs`、
`skills/kemi/references/conversation.md`、`README.md`、`CHANGELOG.md`。
Done when:
- 会話パネルの、行のあるスレッドの末尾に `data-kemi-hand-line="pending"` / `"working"` の行が出る。発言だけの 1 回分の行は並びの末尾に出る。サーバの行の表が
  変わるとその場で変わる。
- README（と、エージェントに見え方を説明している所があれば `conversation.md`）と CHANGELOG が、渡した後の行を書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-agent-channel.mjs`）。
- [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す) の成功条件のうち渡した 1 回分の行の 3 つ（`pending` → `working` → 返信で消える、c1 と c2 で片方だけ消える、
  発言だけの行）。
- [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態) の成功条件のうち `data-kemi-agent-state` の 3 つ（未接続 → 待機中 → 作業中 → 返事済み → 待機中、
  c1 と c2 で片方だけ返信すると `working`、返事済みの間に渡すと `replied` のままで受け取り待ちの行が出る）。
Left to the implementer: 行の見た目と文言（英語。モックの状態 3 に倣う）。
Stop and hand back if: なし。

## Step 6 — 「エージェントに渡す」を常に出す

Purpose: `kemi wait` の前から渡す操作が見え、何をすれば渡せるようになるかが分かるようにする。
Specification: [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（常に出す、未接続の案内とコマンドを写す操作、渡すものが無いときの見た目）、
[R-AGENT-FLOW](../spec/agent-channel.md#r-agent-flow-往復の流れ)、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（確認の未渡しの件数の条件）、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 5。
May change: `src/main.rs`、`crates/kemi-server/`、`tests/e2e.rs`、`web/index.html`、`web/assets/`、`web/assets/*.test.js`、`scripts/test-agent-channel.mjs`、
`scripts/test-live.mjs`、`skills/kemi/`（`references/conversation.md` の「appears only once `kemi wait` has been called」）、`README.md`、`CHANGELOG.md`。
Done when:
- 「エージェントに渡す」が状態によらず出ている（会話パネルを畳んでいても）。未接続の間は押せず、そばに `kemi wait` を始めると渡せることと、
  `kemi wait <id>`（このレビューの id）を写す操作がある。`kemi wait` が呼ばれたら押せる。
- 渡すもの（前に渡した後の変化）が無い間は、押せない見た目で、渡すものが無いことが分かる。
- エージェントの API が立っていないレビューでも押せないまま出て、写す操作は無く、案内はつながれないことを言う（仮決め 6）。
- submit の確認の未渡しの件数は、今どおり一度でも `kemi wait` が呼ばれたレビューでだけ出る。
- `conversation.md`、README、CHANGELOG が「常に出ていて、`kemi wait` の前は押せない」を書いている（同じコミットで）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-agent-channel.mjs`）と e2e。
- ブラウザ自動化: `kemi wait` の前は「エージェントに渡す」が出ていて押せず、案内とコマンドを写す操作が出て、写したものが `kemi wait <id>`（stderr の
  `kemi: review <id>` の id）。会話パネルを畳んでも見える。`kemi wait` を呼ぶと押せる。渡した直後は押せず、返信を 1 つ書くと押せる。
- ブラウザ自動化と e2e: `kemi wait` を一度も呼ばずに起動したレビューで「エージェントに渡す」が出ていて押せず、submit すると今どおり stdout に JSON が出て終わる。
Left to the implementer: 案内と写す操作の見た目と文言（英語。モックの状態 1 に倣う）、写す処理（`copyPath` を使う・共通にするなど）。
Stop and hand back if: なし。

## Step 7 — 別のスレッドの新着とパネルの見出しの数

Purpose: 別のスレッドを読んでいる間に届いた返事を見落とさないようにする。
Specification: [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)（別のスレッドの新着、パネルの見出しの新着の数）、[R-VIEW](../spec/kemi.md#r-view-差分の表示)（新着の数の数え方）。
Prerequisites: Step 6。
May change: `web/index.html`、`web/assets/`、`web/assets/*.test.js`、`scripts/test-agent-channel.mjs`、`README.md`、`CHANGELOG.md`。
Done when:
- README と CHANGELOG が、別のスレッドの新着の知らせとパネルの見出しの数を書いている（同じコミットで）。
- スレッドを開いている間に別のスレッドへ返信が届くと、開いているスレッドの上にどのスレッドに届いたかの知らせが出て、押すとそのスレッドが開く。
- 会話パネルを開いている間も、パネルの見出しに新着の数（帯の新着の数と同じ数え方）が出る。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-agent-channel.mjs`）。
- c1 を開いている間に `kemi reply` で c2 に返信を書くと、知らせとパネルの見出しの新着の数が出る。知らせを押すと c2 が開く。
Left to the implementer: 知らせの見た目と文言（英語。モックの状態 9 に倣う）、複数のスレッドに届いたときの知らせのまとめ方（最後に届いたものを出す、など）。
Stop and hand back if: なし。

## Step 8 — 狭い画面の浮かぶ「エージェントに渡す」と道具

Purpose: 狭い画面で、会話パネルのシートを開かずに渡せるようにし、`--live` では道具も画面の下にまとめる。
Specification: [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（狭い画面の浮かぶボタン）、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（狭い画面でだけ使う要素）、
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（狭い画面の道具と「エージェントに渡す」）、[R-VERIFY](../spec/kemi.md#r-verify-検証)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 7。
May change: `web/index.html`、`web/assets/`、`scripts/test-agent-channel.mjs`、`scripts/test-narrow-screen.mjs`、`scripts/test-live.mjs`、`README.md`、`CHANGELOG.md`。
Done when:
- 狭い画面では、どの入力モード・見方でも「エージェントに渡す」が画面の下に浮かび、会話パネルのシートを閉じたまま渡せる。状態ごとの押せる・押せないは Step 6 と同じ。
- `--live` のページの見方の狭い画面では、道具（要素・矢印・ペン・操作）と「エージェントに渡す」が画面の下の 1 つの浮かぶツールバーにあり、見る対象の枠の上の
  ツールバーは出ない。コードの見方では浮かぶ「エージェントに渡す」だけ。
- 広い画面は変わらない。`--live` でないレビューはページ用のファイルを読まない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化。
- 幅 390px の worktree のレビュー（`scripts/test-agent-channel.mjs`）で、シートを閉じたまま画面の下の「エージェントに渡す」で渡せる（`kemi wait` がその 1 回分を返す）。
- 幅 390px の `--live` のページの見方（`scripts/test-live.mjs`）で、道具と「エージェントに渡す」が画面の下にあり、道具を切り替えて場所を置ける。
- 幅 1280px で、浮かぶ「エージェントに渡す」と `--live` の浮かぶツールバーが見えない（`scripts/test-narrow-screen.mjs` の狭い画面でだけ使う要素の一覧に足す。
  `--live` のものは `scripts/test-live.mjs`）。
- 既存の `otherReviewsLoadNoPageFiles` が通り続ける。
- 起動の上限（[R-VERIFY](../spec/kemi.md#r-verify-検証)。往復の機能は全モードで読み込まれる）: リポジトリの外に `scripts/gen-fixture.sh` で作った試験用リポジトリで
  `scripts/measure-startup.sh <fixture> target/release/kemi` を回し、中央値が 1 秒未満であることを報告する（人が確認する）。
Left to the implementer: 浮かぶボタンとツールバーの見た目（モックの状態 11 に倣う）、全モードの浮かぶボタンを `--live` のツールバーの中へ移すか、`--live` の側で
別に出して全モードのものを隠すか。
Stop and hand back if: 狭い画面の全モード共通の部品を変えると、広い画面の振る舞いが変わってしまう場合。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-COMMENT（`#n`、番号を押して入れる、詰め直し、宙に浮いた参照） | Step 1 |
| R-PAGE-COMMENT（要素の道具と `html` / `body`、矢印の先、範囲だけ） | Step 2 |
| R-PAGE-COMMENT（ページの上の印） | Step 3 |
| R-SUBMIT / R-AGENT-EVENTS（連番、空の `elements`、本文の `#n`） | Step 1、Step 2 |
| R-AGENT-STATE（5 つの状態、表、`data-kemi-agent-state`） | Step 4（単体とサーバ）、Step 5（画面） |
| R-AGENT-HAND（渡した 1 回分の行、`data-kemi-hand-line`） | Step 4（サーバ）、Step 5（画面） |
| R-AGENT-STATE / R-AGENT-FLOW（常に出す、未接続の案内と写す操作、渡すものが無い） | Step 6 |
| R-SUBMIT（確認の未渡しの件数の条件） | Step 6 |
| R-AGENT-HAND（別のスレッドの新着、見出しの数） | Step 7 |
| R-AGENT-STATE / R-NARROW / R-PAGE-VIEW（狭い画面の浮かぶ Hand と道具） | Step 8 |
| R-DIST（スキル・README・CHANGELOG を同じコミットで） | Step 1〜8 |
| R-VERIFY（ページ用のファイルを読まない） | Step 8 と各ステップの `otherReviewsLoadNoPageFiles` |

## Left to the implementer

- 関数・型・値の名前、モジュールの分け方（`PROJECT.md` の層と features の順の規則を守る。`--live` 用のモジュールは `modulepreload` に載せない例外に入れる）。
- 見た目と文言（UI の文言は英語。モックの状態 1・2・3・9・11 に倣う）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、エラーの扱い、上限）を決めないと進めない場合で、上の仮決めのどれにも当たらないとき。
- 上の「既存の検査の扱い」に挙げた以外の既存の検査の assert を変える必要が出た場合。
- submit の JSON や `kemi wait` の JSON の形を、仕様が書いた変更（連番、空の `elements`、本文の `#n`）より広く変える必要が出た場合。
- セッション形式の版を上げる必要が出た場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 計画③: 変化の一覧の作り直し（要素ごと、押すと光る、比べる相手を見出しに、消えた要素の行）、Mock file… の一覧と API、Record now の切り替えと知らせ、
  場所の一覧の行に乗せると光る、「ページで見る」、書く欄を閉じた後の枠の高さ、Remove mock の Undo などの小さな直し。
- エージェントのスクリーンショット（UL2）、`--live` の性能の数字（UL1）、モックの一覧の件数の上限（UC3）。
