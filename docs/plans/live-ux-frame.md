# --live の使い勝手 ①: 見方と画面の骨組み

## Goal

`--live` のレビュー画面を、採用した画面モックの骨組みに作り替える。上部バーのモードのタブ、既定の「見る対象だけ」と 3 つの見比べ方、
Fit / 100%、比べる相手の名前、道具の既定と始め方の案内、狭い画面の帯を、仕様どおりにする。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（モードのタブ、ページの見方の上部バー、ページの見方の更新バッジ）
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（見比べ方 3 つと既定、Fit / 100%、手で読み込み直す、幅の欄、覚える範囲、狭い画面）
  - [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)のうち、道具の既定・道具を選んだだけでは書く欄を開かない・始め方の案内
  - [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)のうち、「ページへのコメントを一度でも保存したか」の保存と、保存しないもの
- [比べる相手 仕様](../spec/live-compare.md): [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（選択を出す見比べ方、
  自動の実物の名前と並び、規則の説明、重ねている間の見出し、上端揃え、覚える範囲）
- [kemi 仕様](../spec/kemi.md): [R-VIEW](../spec/kemi.md#r-view-差分の表示)（`--live` のタブ）、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（`--live` の
  1 段目のタブ）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)、[R-VERIFY](../spec/kemi.md#r-verify-検証)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)
- [エージェントとのやりとり 仕様](../spec/agent-channel.md): [R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)（上部バーに出す
  状態の読み方だけ。状態の表と `data-kemi-agent-state` は計画②）
- 見た目の参照: [docs/design/ui-mock-live-v2.html](../design/ui-mock-live-v2.html)（状態 1・4・5・10・11）。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md` の「ページの見方」「コードの見方」「見比べ方」「比べる相手」「見る対象」。

## 3 つの計画の中の位置

使い勝手の見直しは 3 つに分ける。**① この計画（見方と画面の骨組み）**、② コメントと渡す流れ（番号の詰め直しと `#n`、範囲だけの場所、
Hand を常に出す、受け取り待ち・作業中の行、状態の表と `data-kemi-*`、別スレッドの新着、狭い画面の浮かぶ Hand、スキルと README）、
③ 見比べの中身とモック（変化の一覧、比べる相手の名前を一覧の見出しに出す、消えた要素の行、Mock file… の一覧と API、Record now の切り替え、
残りの小さな直し）。リリースは 3 つとも終えてから。

この計画で作らないもの: 上の②③の項目。とくに、モックの割り当ての操作（今の帯の入力欄と、割り当て・外す・読み直すのボタン）は ③ で
Mock file… に置き換えるまで今の形で残し、見比べ方によらず帯に見えるようにしておく（見る対象だけのときに外せなくならないように）。
Record now の押した後の動き（比べる相手の切り替えと、成功の知らせ）は ③。狭い画面の浮かぶ Hand to agent は ②。

③ までの途中の状態として受け入れること: 見る対象だけで Record now を押して取れたとき、画面に成功の知らせは出ない（取ったものは選択肢には
増える）。取れなかったこと・保存しなかったことの知らせは既存の要件なので、この計画で見比べ方によらず見える場所に出す（Step 4）。

## Approach and why

### 今の作り（調べた事実）

- 上部バー `header.topbar` は全モード共通で `web/index.html` に固定で並ぶ（表示モード、Wrap、focus、Sort、テーマ、会話、更新バッジ、承認）。
  `live.css` にも `style.css` にも、`--live` で上部バーを出し分ける規則は無い。`features/live.js` は `body.dataset.liveView` を設定している。
- Page | Code の切り替えは `views/live.js` の `buildShell` が作る帯 `.lv-band` の中の小さな `viewSeg`。帯には幅、比べる相手の選択、Record now、
  Side by side / Overlay、透かし具合、モックの入力、Now / Before（狭い画面だけ）、道具が並ぶ。
- `features/live.js` の状態: `compare: "side" | "overlay"`（既定 side）、`tool: "interact"`（既定）、`chosen: Map`（メモリだけ）。
  倍率は `layoutFrames` と `live-model.js` の `fitScale`（縮めるだけ）。`.lv-viewport` は `overflow: hidden`。
- 比べる相手の選択肢は `renderBand` が組む: モックがあれば先頭、次に固定文言 `"Latest snapshot (handed, start, recorded)"`、続けて
  `snapshotOptions`（新しい順、種類が混ざる）。見出しは固定の "Before" / "Now"。
- 書く欄は、道具が interact 以外なら場所 0 でも開く（`features/live.js` の `compose.box.hidden` の条件）。始め方の案内は無い。
  道具が interact 以外のとき、ページの上の `capture` 層が wheel を受けてページへの `scroll-by` に変えて送る。
- Record now、見比べ方の切り替え、透かし具合、モックの入力と割り当て・外す・読み直すは、どれも比べる相手の選択の入れ物 `compareSlot` の子。
  取れなかった・保存しなかったなどの知らせ（`refNotice`）は比べる相手の枠の見出しの中にある。`capture("manual")` は比べる相手を切り替えない。
- 狭い画面で 1 枚ずつ出す CSS は `.lv-stage[data-compare="side"]` にだけ掛かる。
- 見たの進捗は `views/header.js` の `renderProgress` が呼ばれるたびに `dom.progress.hidden` を書き直す（コメントの保存の後の `renderHeader` からも）。
- 幅の欄は Enter と `change`（欄から離れたとき）の両方で効くが、効いた後に `setWidth` が欄を空にする。
- 更新バッジは `app.js` → `features/files.js` の `refresh()`。見方は変えない。`setView` は `features/live.js` の中で export されていない。
- エージェントの状態は `views/conversation.js` の `renderAgentState` が会話パネルに描く（`renderConversation` から毎回呼ばれる）。
- 狭い画面は `app.js` の `matchMedia` → `features/narrow.js`。`--live` の帯は折り返すだけで、重ねて透かす・比べる相手の選択・Record now・
  モックの入力も狭い画面に出ている。
- セッション状態 `SessionState`（`crates/kemi-core/src/session/mod.rs`）は版 3（未リリース）で、`#[serde(default, skip_serializing_if…)]` の前例がある。
  ページへのコメントの保存は `crates/kemi-server/src/api/comments.rs` の `add_page_comment`。

### やり方（計画で決める）

- **上部バーの出し分けは CSS と、`--live` のときだけ作る要素で行う。** 全モード共通の `index.html` の要素は消さずに、ページの見方の間だけ
  `body[data-live-view="page"]` で隠す。モードのタブ・ページの URL と幅・エージェントの状態は、`features/live.js`（`--live` のときだけ動的に
  読み込まれる）が作って上部バーに差し込む。`--live` でないレビューがページ用のファイルを読まない約束（R-VERIFY）を保つため。
  見たの進捗を CSS で隠すと `views/header.js` の `fitProgress` の測りが崩れるので、進捗の出し分けは `hidden` で行い、`renderProgress` 自身が
  ページの見方（`state.live`）を見て隠す（ほかの所から描き直されても戻らないように）。
- **見比べ方は 3 値にする**（`"now" | "side" | "overlay"`、既定 `"now"`。`stage.dataset.compare` の値もこの名前にする。既存の検査が
  `'side'`・`'overlay'` を見ているため）。`stage.dataset.compare` を読む CSS と `layoutFrames`・`showMock` を 3 値に合わせる。見る対象だけのときも、
  比べる相手（スナップショットの本文と記述）は読み続け、変化の一覧と見る対象の側の印は今どおり出す。狭い画面の 1 枚ずつの切り替え（Now / Before）は、
  見比べ方の値によらず効くようにする（今は `side` にだけ掛かっている）。
- **帯の作り直し:** Record now は（モックどおり）幅の帯に置き、見比べ方によらず押せる。モックの入力と割り当て・外す・読み直すも、③ まで見比べ方に
  よらず帯に見える。比べる相手の選択と透かし具合だけが、広い画面で並べる・重ねるの間だけ出る。取れなかった・保存しなかった・保存できなかった
  スナップショットの知らせは、見る対象だけのときも見える場所に出す。
- **100% は枠の中のスクロールで見せる。** `.lv-viewport` の `overflow` を等倍のときだけスクロールにする。重ねて透かすの外側の translate
  （`overlayPlacement`）はページ内のスクロールを前提にしているので、等倍と重ねるの組み合わせで位置がずれないことを確かめる。
- **比べる相手の名前の作りは `live-model.js` の純粋な関数にする**（自動の実物の名前、並び、規則の説明）。単体テストで決める。
- **更新バッジ:** ページの見方の間に押されたら、コードの見方に切り替えてから `refresh()` する。層の規則（`features` の順で live は files より後ろ）に
  従い、live.js が files の `refresh` を直接使うか、`actions.js` に口を足すかは実装者が決める。
- **「ページへのコメントを一度でも保存したか」**は `SessionState` に真偽で足し、保存形式の `StateDto` に `#[serde(default, skip_serializing_if = …)]` で写し
  （`mocks` の前例と同じ）、
  `add_page_comment` のセッションのロックの中で立てる。画面へは `api/review` の応答で渡す。`is_empty`・`has_conversation` には入れない
  （コメントが無い `--live` のセッションは今どおり残さない）。

## Scope of change

- Rust: `crates/kemi-core/src/session/`、`crates/kemi-server/`（`session.rs`、`api/comments.rs`、`api.rs` の応答）
- ページ: `web/index.html`（必要なら）、`web/assets/` の下（`features/live.js`・`views/live.js`・`live-model.js`・`live.css`・`style.css`・
  `views/header.js`・`views/conversation.js`・`app.js`・`actions.js`）
- テスト: `web/assets/*.test.js`、`scripts/test-live.mjs`、`scripts/test-narrow-screen.mjs`、`scripts/live-dev-server.mjs`、各クレートのテスト、`tests/e2e.rs`
- 文書: `README.md`（Live review の節）、`CHANGELOG.md`（Unreleased）、`PROJECT.md`（モジュールや層の説明が変わるとき）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/`、`skills/` は変えない。

## 既存のブラウザ検査の扱い

仕様が変わったので、`scripts/test-live.mjs` の次の検査は前提が変わる。手順を足して（並べる見比べ方に切り替える、道具を Interact に切り替える、
… メニューを開く、など）今の確かめを保つのはよい。仕様が変えた振る舞いそのもの（既定の見比べ方、道具の既定、比べる相手の選択の出し方、
狭い画面の帯、Page | Code の切り替えの場所）を確かめている assert は、新しい仕様に合わせて書き換えてよい。それ以外の assert を変える必要が
出たら止まる。

- 道具の既定 Interact を前提にページを押す: `outsideGitFilePages`、`snapshotsAreTakenShownAndChosen`
- Page / Code のボタン `.lv-view`: `outsideGitFilePages`、`changeListFollowsThePage`、`switchingWhileSavingKeepsTheImageAroundThePlace`、
  `savingAndHandingKeepThePageLaidOut`
- 既定が並べる見方、または比べる相手の選択がいつも出ている前提: `snapshotsAreTakenShownAndChosen`、`overlayFollowsTheScrollAndTheOpacity`、
  `marksFollowTheChanges`、`scrollingMakesNoChangeAndMarksStay`、`pageCommentPlacesArePutAndSaved`、`changeListFollowsThePage`、
  `snapshotsAndMocksComeBackAfterResuming`、`chooseReference` を使う検査
- 狭い画面の帯（`.lv-side`、比べる相手の選択・重ねて透かす・Record now が帯に見える）: `pageViewShowsFramedPagesWidthsAndNarrowScreens`、
  `changeListFollowsThePage`、`narrowReferenceSideSavesTheImage`、`switchingWhileSavingKeepsTheImageAroundThePlace`、`overlayFollowsTheScrollAndTheOpacity`
- 比べる相手の名前（'Handed 1' などの文言で見ている所）: [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の成功条件が文言を契約にしていない
  ので、新しく足す検査は文言に頼らない形にする。既存の文言の確かめは、名前の作りを変えても通るなら残してよい。並べ方や名前の作りが変わって
  先頭一致などが崩れたら、「選択に出ている名前と選択肢の項目の名前が同じ」を見る形へ書き換えてよい。
- 上の一覧に無い検査も含め、`scripts/test-live.mjs` のどの検査でも、比べる相手の枠を見る・押す・撮るために「並べる見比べ方に切り替える」手順を足すのは
  よい（例: `snapshotsCarryTheirResources`、`mocksAreAssignedShownAndKeptApart`、`marksAreDrawnUnderAStrictStylePolicy`、`snapshotsAreTakenUnderTrustedTypes`、
  `removedMarksSurviveReparsing`、`widthSwitchesWithoutResizingTheDocument`、`placesMakeNoChange`）。`showsSnapshot` は枠の `dataset` と見出しの文字だけを見るので
  隠れていても通りうるが、`notRecorded` は見えることも見るので、隠れたままでは落ちる。手順を足すのは確かめの中身を保つためで、assert を緩めるためではない。
- 狭い画面で重ねて透かすに切り替えられることを PASS にしている `overlayFollowsTheScrollAndTheOpacity` の 390px の部分は、仕様（狭い画面では重ねて透かすを
  出さない）に合わせて書き換える。

## Step order and prerequisites

Step 1 → 2 → … → 7 の順。各ステップは前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`
3. ブラウザの検査があるステップ: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   `node scripts/test-live.mjs target/release/kemi` と、既存の 4 本（`test-agent-channel.mjs`・`test-narrow-screen.mjs`・
   `test-rendered-view.mjs`・`test-horizontal-scroll.mjs`）を 1 本ずつ回す。

---

## Step 1 — 「ページへのコメントを一度でも保存したか」の保存

Purpose: 始め方の案内を、保留と復元をまたいで「初めて保存するまで」出せるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（始め方の案内）、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（保存するもの）。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/`、`crates/kemi-server/`、`tests/e2e.rs`、各クレートのテスト。
Done when: ページへのコメントを初めて保存するとセッション状態に印が立ち、`api/review` の応答に載る。コメントを全部消しても印は残る。保留して
復元しても残る（コメントが残っているセッションのとき）。印の無い版 2・版 3 の `<id>.session` は、印が無いものとして読める。
`is_empty` と `has_conversation` は変わらない。
Shown by: test — RED → GREEN → REFACTOR。
- 単体テスト（`crates/kemi-core` の session）: 印を持つ状態を書いて読み戻すと同じ。
- e2e: `--live` でページへのコメントを足すと `api/review` の応答に印が立ち、そのコメントを消しても立ったまま。別のコメントを残して保留・復元しても立ったまま。
- 単体テスト: 印の項目が無い `<id>.session`（版 2 の固定のバイト列の今のテストと、印の無い版 3）が、印が無いものとして読める。
Left to the implementer: 項目の名前、応答のどこに載せるか（契約ではない。submit と `kemi wait` の JSON には入れない）。
Stop and hand back if: セッション形式の版を上げる必要が出た場合。

## Step 2 — 比べる相手の名前と並び（純粋な関数）

Purpose: 自動の実物の名前、選択肢の並び、規則の説明を、画面に組み込む前に純粋な関数で決める。
Specification: [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（選択肢と並び、自動の名前、規則の説明、重ねている間の見出し、端の見出し）。
Prerequisites: なし。
May change: `web/assets/live-model.js`、`web/assets/live-model.test.js`。
Done when: `live-model.js` に、(1) 選択肢の並び（自動 → 渡した時点 → 開始時 → 手で取った時点 → モック）を作る関数、(2) 自動のときに実際に
選ばれているものの名前を、選択肢の項目の名前と同じ作りで返す関数、(3) 重ねている間の見出し（両方の名前と透かし具合、端では見えている方だけ。
端でも「見る対象だけ」の見出しと同じにならない）を作る関数がある。
Shown by: test — RED → GREEN → REFACTOR（`node --test web`）。
- 渡した時点・開始時・手で取った時点が混ざったスナップショットの並びが、仕様の順になる。
- 自動の選択の名前が、選ばれた選択肢の項目の名前と同じになる（渡す前は開始時、渡した後は最後に渡した時点）。
- 重ねている間の見出しが、途中・両端・見る対象だけで互いに違う。
Left to the implementer: 関数の名前と形、規則の説明の文言（英語。モックに倣う）、同じ種類の中の並び（モックに倣って新しい順でよい）。
Stop and hand back if: なし。

## Step 3 — 上部バーとモードのタブ

Purpose: ページとコードを、上部バー左端の 2 つのモードのタブで切り替え、ページの見方の上部バーをページ向けにする。
Specification: [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)、[R-VIEW](../spec/kemi.md#r-view-差分の表示)（`--live` のタブ）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)、
[R-VERIFY](../spec/kemi.md#r-verify-検証)（`--live` でないレビューはページ用のファイルを読まない）。
Prerequisites: Step 1、Step 2。
May change: `web/assets/` の下、`web/index.html`、`scripts/test-live.mjs`、`scripts/test-narrow-screen.mjs`。
Done when:
- 上部バーの左端に、ページの見方とコードの見方のタブがある（名前とアイコン）。コードのタブには変更ファイルの数が出る。帯の中の Page | Code の切り替えは無い。
- ページの見方の間は、上部バーにコード用の操作（表示モード、Wrap、focus、Sort）とファイル数・増減数・見たの進捗が見えず、ページの URL・
  表示幅・エージェントの状態と、ページのレビューであることが見える。コードの見方では今どおり。ページへのコメントを保存した後も、見たの進捗は戻らない。
- ページの見方の間も更新バッジが出て、押すとコードの見方に切り替えて読み直す。
- `--live` でないレビューの上部バーは変わらない（ページ用のファイルも読まない）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- ページの見方でコード用の操作と見たの進捗が見えず、コードのタブを押すと見え、ページのタブで戻る（[R-PAGE-MODE](../spec/live.md#r-page-mode-起動) の成功条件）。
  ページへのコメントを保存した後も、ページの見方では見たの進捗が見えない。
- ページの見方の上部バーに、見ているページの URL と表示幅、エージェントの状態（今の状態の値で確かめる。`data-kemi-agent-state` は計画②）が見え、
  コードのタブに変更ファイルの数（`api/review` のファイル数と同じ）が出る。
- ページの見方の間に作業ツリーのファイルを変えると更新バッジが出て、押すとコードの見方になり差分が新しくなる。
- 既存の `otherReviewsLoadNoPageFiles` が通り続ける。
Left to the implementer: 上部バーに差し込む要素の作り方と置き場所の DOM、エージェントの状態を上部バーにも描く方法（`renderAgentState` から
更新する、など）、更新バッジから見方を切り替える経路（層の規則を守る）。
Stop and hand back if: 全モード共通の上部バーの振る舞い（`--live` でないレビュー）を変えないと実現できない場合。

## Step 4 — 見比べ方 3 つ、Fit / 100%、手で読み込み直す、比べる相手の選択の出し方

Purpose: 既定を「見る対象だけ」にし、必要なときに並べる・重ねるに切り替え、比べる相手の名前を仕様どおりに出す。
Specification: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（見比べ方、Fit / 100%、手で読み込み直す、覚える範囲）、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（選択を出す見比べ方、名前、規則の説明、重ねている間の見出し、上端揃え、覚える範囲）。
Prerequisites: Step 3。
May change: `web/assets/` の下、`scripts/test-live.mjs`、`scripts/live-dev-server.mjs`。
Done when:
- 既定の見比べ方は見る対象だけ。見比べ方の 3 つの切り替え、Fit / 100%、手で読み込み直す操作がある（置き場所はモックに倣う）。今の倍率は見る対象の
  枠の見出しに出る。
- Record now とモックの入力・割り当て・外す・読み直すは、見比べ方によらず帯にあって使える。取れなかった・保存しなかった・保存できなかった
  スナップショットの知らせは、見る対象だけのときも見える。
- 狭い画面の 1 枚ずつの切り替えは、見比べ方の値によらず効く。
- 比べる相手の選択は、広い画面では並べる・重ねるの間だけ出る。選択と選択肢に Step 2 の名前と並びと規則の説明が出る。
- 重ねている間の見出しは Step 2 のとおり。並べたとき、両方のページの上端が揃う（長い名前で見出しが折り返しても）。
- 100% では縮めずに枠の中でスクロールして見られる。重ねて透かすと組み合わせても、比べる相手が見る対象とずれない。
- 見比べ方・倍率・比べる相手の選択は画面を開いている間だけ覚え、読み込み直すと既定に戻る。
- 見る対象だけのときも、変化の一覧と見る対象の側の印は出る。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- 開いた直後は見る対象だけで、比べる相手の選択が見えない。並べる・重ねるに切り替えると見え、見る対象だけに戻すと見えない。
- 100% にすると倍率が 1 と出て、枠の中でスクロールしてページの下端まで見られる。Fit に戻すと縮む。
- 手で読み込み直す操作を押すと、見る対象のページが読み込み直される（試験用の開発サーバでページが読み込まれた回数を数えて確かめる。数える口は
  `scripts/live-dev-server.mjs` に足してよい）。
- 2 回渡してから並べると、選択に出ている名前が、選択肢の 2 回目に渡した時点の項目の名前と同じ（文言は固定しない）。
- 重ねて透かすで透かし具合を 0・中間・100 にしたときと見る対象だけのときで、見出しが互いに違う（文言は固定しない）。
- 見る対象だけのまま、2 MB を超えるページで Record now を押すと、取れなかったことが見える。
- 並べる・100% にしてから画面を読み込み直すと、見る対象だけ・Fit に戻る。
- パスが 200 文字のモックを割り当てて並べ、見出しが折り返す幅にしても、左右のページの上端の差が 1px 以内（[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の成功条件）。
- 390px で、見る対象だけのまま Now / Before を切り替えると、見えている枠が入れ替わる（今の `pageViewShowsFramedPagesWidthsAndNarrowScreens` の確かめを保つ）。
- 見る対象だけのまま CSS を変えると、変化の一覧と印が出る。
Left to the implementer: アイコンの SVG と置き場所（モックに倣う）、等倍のスクロールの作り、知らせを出す場所。
Stop and hand back if: 100% と重ねて透かすを組み合わせると位置がずれ、`web/live/page.js` のスクロールの知らせを変えないと直せない場合
（変えてよいかは仕様に無い。報告して止まる）。

## Step 5 — 道具の既定と始め方の案内

Purpose: 最初に何をすればよいか分かる画面にする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（道具の既定は要素、道具を選んだだけでは欄を開かない、始め方の案内）。
Prerequisites: Step 4。
May change: `web/assets/` の下、`scripts/test-live.mjs`。
Done when:
- 道具の既定は要素。道具は見る対象の枠のすぐ上のツールバーにある（見た目はモック）。
- 道具を選んだだけでは書く欄を開かず、最初の場所を置いたときに開く。
- ページへのコメントをまだ保存していないレビュー（Step 1 の印が無い）では、見る対象の上に 3 つの手順の案内が出る。初めて保存したら消え、
  コメントを全部消しても、保留して復元しても出ない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- 開いた直後の道具が要素で、ページを押すと場所が置かれる。
- 道具を切り替えただけでは書く欄が開かず、場所を置くと開く。
- 案内が出ていて、コメントを保存すると消え、そのコメントを消しても出ない。
- 要素の道具のまま 100% にして、横にも下端までもスクロールできる（道具の層がスクロールを横取りしない）。
Left to the implementer: 案内とツールバーの見た目（モックに倣う）。
Stop and hand back if: なし。

## Step 6 — 狭い画面

Purpose: 狭い画面で、ページを大きく見られ、操作が 1 行に収まるようにする。
Specification: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（狭い画面）、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)（`--live` の 1 段目のタブ）、
[R-PAGE-MODE](../spec/live.md#r-page-mode-起動)。
Prerequisites: Step 5。
May change: `web/assets/` の下、`scripts/test-live.mjs`、`scripts/test-narrow-screen.mjs`。
Done when:
- 狭い画面のページの見方では、上部バーの 1 段目にページとコードの短いタブがあり、2 段目の見たの進捗は出ない。上部バーの … メニューは表示の
  操作のうちテーマだけ。
- その下の操作の帯は 1 行で、ページの名前、表示幅、Now / Before の切り替え、エージェントの状態、… メニュー。比べる相手の選択・Record now・
  Fit / 100%・（この計画の間は今の）モックの入力は … メニューの中。重ねて透かすは出ない。
- 道具は ② で下に浮かぶツールバーに移すまで、Step 5 のツールバーのままでよい。
- 広い画面で重ねて透かすを選んだまま狭い画面に移ったら、狭い画面では 1 枚ずつ（Now / Before）の見方になる。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs` の 390px の検査）。
- 390px でページの見方を開くと、帯の子の上端が揃って 1 行に並び、エージェントの状態が帯にあり、見たの進捗が見えず、重ねて透かすの操作が無い。
  … メニューを開くと比べる相手の選択、Record now、Fit / 100%、モックの入力がある。上部バーの … メニューには、表示の操作のうちテーマだけがある。
- 1 段目のタブでコードの見方に切り替わる。
Left to the implementer: … メニューの作り（既存の popover の部品を使うなど）。
Stop and hand back if: 狭い画面の全モード共通の上部バー（`--live` でないレビュー）を変える必要が出た場合。

## Step 7 — 小さな直しと文書

Purpose: この計画の範囲の小さな直しを入れ、文書を合わせる。
Specification: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（幅の欄は今の幅を残す）、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。見た目はモックの状態 1・4・5。
Prerequisites: Step 6。
May change: `web/assets/` の下、`scripts/test-live.mjs`、`README.md`、`CHANGELOG.md`、`PROJECT.md`。
Done when:
- 幅の欄は（今どおり Enter でも欄から離れても効き）効いた後も今の幅が欄に残り、プリセットの幅ならそのボタンが選ばれる（今は効いた後に欄を空にしている）。
- ページの見方の操作の見た目（アイコンにするものと文字を残すもの）は、モックの状態 1・4・5 に倣う。仕様に規則は無いので、機械的な検査は足さない。
- README の Live review の節と CHANGELOG の Unreleased が、この計画の変更（タブ、既定の見る対象だけ、見比べ方、Fit / 100%、道具の既定と案内、
  狭い画面）を書いている。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（幅の欄）。文書は check（各ステップの終わりの検査）。
- 幅の欄に 1000 を入れて欄から離れると幅が 1000 になり、欄に 1000 が残る。390 を入れると 390 のボタンが選ばれる。
Left to the implementer: アイコンの SVG、文の書き方（README は英語、ほかは日本語）。
Stop and hand back if: なし。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-MODE（タブ、上部バー、更新バッジ） | Step 3、Step 6（狭い画面） |
| R-PAGE-VIEW（見比べ方、Fit/100%、読み込み直す、覚える範囲） | Step 4 |
| R-PAGE-VIEW（狭い画面） | Step 6 |
| R-PAGE-VIEW（幅の欄） | Step 7 |
| R-PAGE-COMMENT（道具の既定、欄を開かない、案内） | Step 1（保存）、Step 5 |
| R-PAGE-SESSION（一度でも保存したか） | Step 1 |
| R-PAGE-REF（名前と並び、見出し） | Step 2（純粋な関数）、Step 4（画面） |
| R-PAGE-REF（選択を出す見比べ方、上端揃え、覚える範囲） | Step 4 |
| R-VIEW / R-NARROW（`--live` のタブ） | Step 3、Step 6 |
| R-VERIFY（ページ用のファイルを読まない） | Step 3 と各ステップの `otherReviewsLoadNoPageFiles` |
| R-DIST | Step 7 |

## Left to the implementer

- 上部バーに差し込む要素の DOM と経路、更新バッジから見方を切り替える経路（Step 3）、アイコンの SVG と見た目（モックに倣う）、関数と値の名前。
- モジュールの分け方（`PROJECT.md` の層と features の順の規則を守る。`--live` 用のモジュールは `modulepreload` に載せない例外に入れる）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、エラーの扱い、上限）を決めないと進めない場合。
- 上の「既存のブラウザ検査の扱い」に挙げた以外の既存の検査の assert を変える必要が出た場合。
- `--live` でないレビューの画面の振る舞いを変える必要が出た場合。
- セッション形式の版を上げる必要が出た場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 計画②: 場所の番号の詰め直しと `#n`、範囲だけの場所と html/body を選ばない、保存済みの印、Hand を常に出す、受け取り待ち・作業中の行、
  状態の表と `data-kemi-*`、返事済み、別スレッドの新着、狭い画面の浮かぶ Hand、スキルと README の契約の直し。
- 計画③: 変化の一覧の作り直し（要素ごと、押すと光る、比べる相手を見出しに、消えた要素の行）、Mock file… の一覧と API、Record now の切り替えと
  知らせ、場所の行で光る・Show on page・白い帯・Remove mock の Undo などの小さな直し。
