# --live の使い勝手 ③: 見比べの中身とモック

## Goal

`--live` の見比べの中身を仕様どおりにする: 変化の一覧を要素ごとの行にして押すとその要素へ連れて行き、一覧の見出しに何と比べているかを出し、
Record now で取ったものに比べる相手を切り替えて知らせ、モックを一覧から選べるパネルと取り消せる外し方にし、ページへのコメントの場所を
一覧や「ページで見る」から光らせ、書く欄を閉じた後に枠の高さを戻す。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（変化の一覧の見出しに比べる相手の名前、モックのときの見出しとメニュー、消えた要素の行を押したときの
    切り替え、狭い画面のメニューの「モックのファイル」）
  - [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)（要素ごとの行、色の前後、押すとスクロールして光る、ずれただけを畳んで説明、凡例、`html` / `body`）
  - [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)（手で取る操作。スナップショットのスクリプトは止めたまま）
  - [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)のうち、場所の一覧の行に乗せると光る、「ページで見る」、書く欄を閉じた後の枠の高さ
- [比べる相手 仕様](../spec/live-compare.md)
  - [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（手で取ったら比べる相手を切り替えて知らせる、選択肢の並びの最後の「モックのファイル」）
  - [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（パネル、一覧と上限 200、パスの入力欄、選択の横のメニュー、外したときの取り消し）
- [kemi 仕様](../spec/kemi.md): [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（変化の一覧と印はその場で変わる）、[R-VERIFY](../spec/kemi.md#r-verify-検証)、
  [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-DEPS](../spec/kemi.md#r-deps-依存)
- 見た目の参照: [docs/design/ui-mock-live-v2.html](../design/ui-mock-live-v2.html)（状態 2・6・7・8）。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md`。

## 3 つの計画の中の位置

使い勝手の見直しの最後。① 見方と画面の骨組み、② コメントと渡す流れは main にマージ済み。リリースはこの計画を終えてから。

## Approach and why

### 今の作り（調べた事実）

変化の一覧:
- 差分は `web/assets/live-diff.js` の `diffDescriptions(before, now)`。出力 `Change` は性質ごとに 1 件（`visual` は CSS の性質ごと）で、並びは主な変化 → removed →
  shifted。`shifted` は見た目と文字に変化が無く箱が動いた要素で、タグで除いていないので `html` と `body` も入りうる。`excerpt`（要素の文字）は added と
  removed にだけ付く。印は `marksOf(changes)`。
- 状態は `features/live.js` の `live.changes`・`live.changesFrom`・`live.shiftedOpen`・`live.listed`。`refreshChanges()` → `setChanges()` → `renderChanges(...)`。
  比べる相手がモックか無いときは `setChanges(null, null)` で、一覧（`.lv-changes`）は見出しごと消える。
- 描画は `views/live.js` の `changeList` / `changeItems` / `changeWhat`。見出しは件数だけで比べる相手の名前は無い。行は `li.lv-change[data-kind]`（点・何が・どこ）で
  押しても何も起きない。色は値の文字。ずれただけは `<details class="lv-shifted">` に説明も凡例も無い。`box.dataset.main` / `dataset.shifted` は Change の件数。
- page.js（`web/live/page.js`）が受けるメッセージは `describe`・`marks`・`place`・`places`・`saved-at`・`scroll-by`・`image`・`capture`。要素や場所までスクロールして
  光らせるものは無い。`marks` の `index` は最後に渡した記述の要素の並びを指す。
- 比べる相手の枠（`refFrame`）は `sandbox=""` の srcdoc で、スクリプトが動かず親から中をスクロールできない。消えた要素の印は `markRemovedInSnapshot` で
  srcdoc に足す。重ねて透かすときだけ、比べる相手を中身の高さで描いて外側の transform でずらす（`overlayPlacement`）。それ以外は枠の高さが viewport の高さで、
  中の文書が自分でスクロールする。

Record now:
- `capture("manual")` は成功しても `live.chosen` に触らず、知らせは失敗の文言（`refNotice`、`dataset.kind = "waiting"` 固定）だけ。

モック:
- 画面は帯の `mockGroup`（入力欄・Assign・Remove・Reload・エラー）。外すとパスはどこにも残らない。選択肢（`live-model.js` の `referenceOptions`）は
  モックを割り当てているときだけ `Mock: <path>` を足し、「モックのファイル」の項目は無い。
- サーバは `crates/kemi-server/src/api/mock.rs`: `POST api/mock`（割り当て・外す。`mock_path` が canonicalize と `served_path` で範囲の中の HTML か確かめる）、
  `GET api/mocks`（割り当ての一覧）、`GET /m/{secret}/{*path}`（配る）。ファイルの一覧を返す API は無い。範囲の根は `LiveInfo.root`、git の中かは
  `repo_root(&live.root).is_ok()`。
- git の CLI は `kemi-core` の `source/` で呼ぶ（`git_raw` は `pub(crate)`）。`WorkTree`（pub）が作業ツリーの監視に使われている。ディレクトリを辿るクレートは依存に無い。
- トーストは `views/overlay.js` の `showToast(message)` で、文字だけで操作を置けない。

ページへのコメントの場所:
- 場所の一覧（`views/live.js` の `renderPlaces`）の行に乗せる・押す処理は無い。「Show on page」（`views/conversation.js`）は `actions.showPageComment` →
  `features/live.js` の `showPageComment` → `showPage(url, width)` で、スクロールも光らせるのもしない。
- 書く欄（`.lv-compose`）の開閉は `.lv-stage` の大きさを変えないので、`ResizeObserver`（stage だけを見ている）が `layoutFrames()` を呼ばない。開いている間に
  `layoutFrames()` が走ると縮んだ高さが枠に書き込まれ、閉じても測り直されない（白い帯の原因の見込み。最初の保存で案内が消えるときも同じ）。

### やり方（計画で決める）

- **差分の出力の形は変えず、要素ごとにまとめる純粋な関数を足す。** `diffDescriptions` の性質ごとの `Change` と `marksOf` はそのまま使い、`live-diff.js` に
  要素ごとに Change をまとめる関数を足して、一覧はそれを描く。まとめる鍵は側ごとに分ける: 消えた要素は比べる相手の側の番号（`before`）、ほかは今の側の番号
  （`now`）。`before` と `now` は別の記述の番号で、番号だけを鍵にすると、消えた要素と今のページで同じ番号の別の要素が 1 つにまとまってしまうため。既存の単体テストを壊さずに、まとめ方を単体テストで決められるため。
  行の「どの要素か」は種類（タグ）と文字（`excerpt` をすべての種類に付ける）、「どこにあるか」は今の `label`（セレクタ）。
- **`html` と `body` はずれただけに入れない**（`diffDescriptions` で除く）。見た目と文字の変化は今どおり主な変化に入る。
- **見る対象の要素へ連れて行くのは page.js の新しいメッセージ**（記述の要素の番号を受け、その要素をページの中で見える位置までスクロールし、印を光らせ、
  要素の箱を返す）。等倍で横にはみ出すときは、返った箱で親が外側の横スクロールを合わせる。
- **比べる相手の側（消えた要素）へ連れて行くのは親の側で行う。** 比べる相手の枠はスクリプトが動かず中をスクロールできない。今、並べる見比べ方の比べる相手の枠は
  viewport の高さで中の文書が自分でスクロールし、見る対象との連動は横の `scrollLeft` だけ（縦の連動は無い）。重ねて透かすときだけ「中身の高さで描いて外側で
  ずらす」形（`overlayPlacement`）。そこで消えた要素の行を押したときは（仮決め 1）:
  - 並べる・狭い画面の比べる相手の 1 枚: 比べる相手を「中身の高さで描いて外側でずらす」形にし、スナップショットの記述にあるその要素の箱の位置まで外側をずらして、
    その要素の印を光らせる。この形の間は、比べる相手の上でのホイールが外側のずれを動かす。比べる相手・見比べ方・ページ・表示幅のどれかが変わったら、元の形
    （枠の中で自分でスクロールする形）に戻す。
  - 見る対象だけ: 並べる見比べ方に切り替えてから上と同じ。狭い画面では比べる相手の 1 枚に切り替えてから
    （[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)）。
  - 重ねて透かす: 見比べ方は変えず、両方のスクロールをそろえたまま（[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)）、その要素の箱の位置まで
    スクロールして光らせる（仮決め 10）。
- **一覧の見出しに比べる相手の名前**: `live-model.js` の `referenceName` を `changesToList` 経由で渡し、実物の名前を出す（自動のときの選択は
  `Auto — <実物>` なので、見出しは選択の文字に含まれる名前になる。仮決め 12）。比べる相手がモックのときは、
  一覧の代わりに見出し（割り当てたファイルの名前）と、選択の横と同じメニュー（読み直す・外す）を出す。
- **Record now で取れたら** `live.chosen.set(page, 取った id)` で比べる相手を切り替え、成功の知らせを出す。見る対象だけのときは知らせに「並べる」操作を付ける。
  知らせは帯の今の知らせの場所（失敗の知らせと同じ場所）に、成功と分かる種類で出す。狭い画面では「並べる」が無いので、知らせの操作は比べる相手の 1 枚への
  切り替えにする（仮決め 11）。
- **モックの一覧の API** を足す: `GET api/mock-files?q=<検索>` が `{ files: [相対パス], total: <検索に合う数> }` を返し、`files` は 200 件まで
  （[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)）。集めるのは `kemi-core` の `source/` に pub の関数として置く: git の中なら
  `git ls-files --cached --others --exclude-standard -z` 相当で集めてから、割り当てと同じ `is_html`（大文字小文字を区別しない）で絞る（pathspec の `*.html` は
  大文字小文字を区別し、`.HTML` が漏れるため）。git の外なら標準ライブラリで範囲の下を辿る（`.git/` は入らない、
  シンボリックリンクのディレクトリは辿らない）。どちらも各パスに `mock_path` と同じ canonicalize と `served_path` を当て、範囲の外を指すものと消えたものを落とす。
  絞り込みはサーバで行う（パスに検索の文字を大文字小文字を区別せずに含むもの）。新しい依存は足さない（[R-DEPS](../spec/kemi.md#r-deps-依存)）。
- **モックのパネル**: 比べる相手の選択肢の最後に「モックのファイル」を常に置き、選んだら `live.chosen` に入れずにパネルを開く。パネルは一行の説明・検索の欄・
  一覧（「出している数 / 全体の数」と絞り込めること）・一覧に無いファイルのパスを入れる欄。帯の今の `mockGroup` は無くす（入口はパネルと、選択の横のメニュー）。
  割り当てている間は選択にファイルの名前を出し、選択の横のメニューに読み直す・外すを置く。外したら、外したパスを覚えて取り消す操作つきの知らせを出し、
  押すと同じパスで割り当て直す。
- **取り消しつきの知らせ**は、全モード共通のトースト（`views/overlay.js` の `showToast`）ではなく、帯の知らせの場所（Record now の知らせと同じ所）に操作つきで出す。
  共通のトーストは 1 つの欄とタイマーを使い回すので、ほかの知らせ（エージェントの発言など）が来ると取り消す操作が消えてしまうため。
- **場所を光らせる**: 書きかけの場所の一覧の行に乗せる・押すと、page.js に場所を光らせるメッセージを送る（番号で指す）。「ページで見る」は、`showPage` で
  ページと幅をそろえた後、そのページの読み込みと場所の描き込みが済んでから、場所までスクロールして光らせるメッセージを送る。
- **白い帯**: 見る対象の枠の外側（`.lv-viewport`）の大きさの変化も `layoutFrames()` の引き金にする（stage だけでなく）。書く欄や案内の出し入れのたびに
  測り直されるので、閉じた後に欄を開く前の高さへ戻る。

### 仮決め（仕様が決めていないところ。覆す条件つき）

1. 消えた要素の行を押したときは（並べる・狭い画面の 1 枚）、比べる相手の枠を「中身の高さで描いて外側でずらす」形にしてから、その要素の位置まで外側をずらす。
   この形の間は比べる相手の上のホイールで外側のずれを動かし、比べる相手・見比べ方・ページ・表示幅のどれかが変わったら元の形に戻す。理由: 比べる相手の枠は
   スクリプトを止めた sandbox で、親から中をスクロールできず、スクリプトを足すのは [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット) に反する。
   重ねて透かすで同じ形が既に動いている。覆す条件: 「押した後の最初のスクロールで元の形に戻して」「消えた要素は位置を合わせず光らせるだけでいい」。
2. 行の「ページのどこにあるかの手がかり」は今のセレクタ（`label`）。理由: 既にあり、要素を一意に近く指せる。覆す条件: 「近くの見出しの文字など、別の手がかりにして」。
3. 一覧の見出しの件数（`dataset.main` / `dataset.shifted`）は、性質の数ではなく要素の数にする。理由: 行が要素ごとになるので、見出しの数と行の数がそろう。
   覆す条件: 「変わった性質の数を出して」。
4. Record now の成功の知らせは、次に撮るか、ページを移るか、比べる相手を選び直すまで出しておく（失敗の知らせと同じ）。理由: 失敗の知らせと同じ扱いで、
   見る対象だけのときの「並べる」操作を押す前に消えない。覆す条件: 「数秒で消して」。
5. モックの一覧の検索は、パスに検索の文字を大文字小文字を区別せずに含むもの。並びはパスの辞書順。理由: 仕様は「パスを絞り込める」だけで、いちばん素直な形。
   覆す条件: 「あいまい検索や、最近変えた順にして」。
6. git のサブモジュールの中の HTML は一覧に出さない（パスを入れる欄からは割り当てられる）。理由: `ls-files --others` はサブモジュールの中を見ず、サブモジュールを
   モックに使う場面は少ない。覆す条件: 「サブモジュールの中も一覧に出して」。
7. git の外で辿るときの上限は設けない（返すのは 200 件だが、数えるために全部辿る）。シンボリックリンクのディレクトリは辿らない。理由: 仕様が全体の数を求めていて、
   起動したディレクトリの下だけを辿る。覆す条件: 「大きいディレクトリで遅いので、辿る数に上限を付けて」。
8. 外したときの取り消しつきの知らせは、帯の知らせの場所に 8 秒出して消える（取り消さなければ外したまま）。理由: 普通のトーストの 2 秒では押す前に消え、
   共通のトーストはほかの知らせで上書きされる。覆す条件: 「消えずに残して」。
9. 書きかけの場所の一覧の行は、乗せている間と、押したときに光らせる（押すとその場所まで見る対象をスクロールする）。理由: 仕様は「乗せるか押すと光らせる」で、
   押したときは見えない場所でも分かるようにする。覆す条件: 「押してもスクロールしないで」。
10. 重ねて透かすときに消えた要素の行を押したら、見比べ方は変えず、両方のスクロールをそろえたまま、その要素の位置までスクロールして光らせる。理由: 重ねて透かすは
    既に中身の高さで描いて外側でずらす形で、仕様はスクロールを両方でそろえると決めている。覆す条件: 「重ねて透かすときは並べる見比べ方に切り替えてから光らせて」。
11. 狭い画面で Record now が取れたときの知らせの操作は、並べる見比べ方の代わりに比べる相手の 1 枚へ切り替える。理由: 狭い画面には並べる見比べ方が無く、
    消えた要素の行（[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)）と同じ扱い。覆す条件: 「狭い画面では知らせに操作を付けないで」。
12. 変化の一覧の見出しには比べる相手の実物の名前を出す（自動のときに `Auto — ` は付けない）。理由: 見出しは何と比べているかを言う所で、自動の規則は選択の
    そばに出ている。覆す条件: 「見出しにも選択と同じ `Auto — …` を全文で出して」。

## Scope of change

- Rust: `crates/kemi-core/src/source/`（HTML のファイルを集める pub の関数）、`crates/kemi-server/src/api/mock.rs`・`api.rs`（route）、各クレートのテスト、`tests/e2e.rs`
- ページ: `web/assets/` の下（`live-diff.js`・`live-model.js`・`features/live.js`・`views/live.js`・`live.css`・`api.js`・`views/conversation.js`）、`web/live/page.js`
- テスト: `web/assets/*.test.js`、`scripts/test-live.mjs`、`scripts/live-dev-server.mjs`
- 文書: `README.md`（Live review の節）、`CHANGELOG.md`（Unreleased）、`PROJECT.md`（新しいモジュールを作ったとき）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/`、`skills/` は変えない（`skills/kemi` の契約に触れる変更は無い）。

## 既存の検査の扱い

仕様が変えた振る舞いを確かめている次の assert は、新しい仕様に合わせて書き換えてよい。それ以外の assert を変える必要が出たら止まる。

- 変化の一覧の形: `scripts/test-live.mjs` の `changeListFollowsThePage`（値の文字で色を見る所、`dataset.main` / `dataset.shifted` の数、モックを割り当てている間
  `.lv-changes` が無いこと）、`cssomChangesAreFollowed`（行の `data-kind` が `visual` であることと値の文字）、`marksFollowTheChanges` と `changesShowWithThePageAlone` の `dataset.main` の数（要素の数に。仮決め 3）。
  `web/assets/live-diff.test.js` の shifted の件数の検査（`html` / `body` を除いたことで数が変わる所だけ）。
- 手で取った後の比べる相手: `snapshotsAreTakenShownAndChosen` の「390 で手で取ってから 1280 に戻すと Start」「渡すと Handed 1」「`/rich.html` の 1280 に
  戻ると Start」の所、`widthSwitchesWithoutResizingTheDocument` の「768 で取り、390 で取ってから 768 に戻すと Recorded 1」の所（取ったものに切り替わり、選んだ
  時点は今の幅で探すので別の幅では記録されていないになり、渡しても戻らない仕様に合わせる）。
- モックの入口: `mocksAreAssignedShownAndKeptApart`、`sideBySidePagesShareTheirTop`、`narrowPageViewFitsOneRow`、`marksFollowTheChanges`、`changeListFollowsThePage` が使う
  `.lv-mock-input`・`.lv-mock-assign`・`.lv-mock-remove`・`.lv-mock-error` は、パネルとメニューの操作に置き換えてよい（確かめている中身は保つ）。
  `web/assets/live-model.test.js` の選択肢の並び（`["latest","s2","s1","mock"]`）とモックの名前（`Mock: path`）の検査は、「モックのファイル」を最後に常に置く形に合わせる。
- `savedPageCommentsAreListedShownAndSwitched` の「Show on page」を押した後の確かめは、スクロールして光らせる振る舞いを足しても通るなら残す。

## Step order and prerequisites

Step 1 → 2 → … → 8 の順。各ステップは前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`
3. `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、`node scripts/test-live.mjs target/release/kemi`、
   `node scripts/test-agent-channel.mjs target/release/kemi`、`node scripts/test-narrow-screen.mjs target/release/kemi`、
   `node scripts/test-rendered-view.mjs target/release/kemi`、`node scripts/test-horizontal-scroll.mjs` を 1 本ずつ回す。

README の Live review の節と CHANGELOG の Unreleased は、振る舞いを変えたステップのコミットで一緒に直す（[R-DIST](../spec/kemi.md#r-dist-名称と配布)）。

---

## Step 1 — 書く欄を閉じた後の枠の高さ

Purpose: 書く欄を閉じた後や案内が消えた後に、見る対象の枠の下に白い帯が残らないようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（書く欄を閉じたら、ページの枠は欄を開く前の高さに戻る）。
Prerequisites: なし。
May change: `web/assets/features/live.js`、`scripts/test-live.mjs`、`CHANGELOG.md`。
Done when: 場所を置いて書く欄を開き、保存しても取り消しても、見る対象の枠の高さが欄を開く前と同じになる。最初の保存で案内が消えたときも、枠が空いた高さまで伸びる。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- 欄を開く前の見る対象の枠の高さを測り、場所を置いて欄を開き、保存して欄が閉じた後の高さが同じ（1px 以内）。取り消しでも同じ。
- コメントの無いレビューで最初の保存をして案内が消えた後、見る対象の枠の高さが、その外側（`.lv-viewport`）の高さを縮めた倍率で割ったものと同じ（1px 以内）。
Left to the implementer: 何の大きさの変化を引き金にするか（`.lv-viewport` を見張るなど）。
Stop and hand back if: なし。

## Step 2 — 場所を光らせる、「ページで見る」

Purpose: 書いている途中の場所と保存したコメントの場所を、ページの上ですぐに見つけられるようにする。
Specification: [R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)（場所の一覧の行に乗せるか押すと光る、「ページで見る」はその場所までスクロールして光る）。
Prerequisites: Step 1。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live.css`、`web/assets/views/conversation.js`、`scripts/test-live.mjs`、
`README.md`、`CHANGELOG.md`。
Done when:
- 書きかけの場所の一覧の行に乗せている間、その場所がページの上で光る。押すと、その場所まで見る対象がスクロールして光る（仮決め 9）。
- 開いたスレッドの「ページで見る」を押すと、ページと表示幅をそろえた後、見る対象がそのコメントの場所までスクロールして光る（別のページ・幅のときも、読み込みと
  描き込みが済んでから）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- 下の方に置いた場所の一覧の行を押すと、見る対象がその場所までスクロールし（場所が枠の中に見える）、光っている（page.js の中の光の印は画素で確かめる。
  ページの中の印の作りは閉じた shadow root の中で、契約ではない）。
- 行に乗せると光り、離れると光が消える（画素で確かめる）。
- c1 を下の方に保存してページの上端に戻り、c1 のスレッドの「ページで見る」を押すと、c1 の場所が枠の中に見えるところまでスクロールして光る。別の表示幅で保存した
  コメントでも、幅が切り替わってから同じになる。
Left to the implementer: 光らせ方と長さ（モックの状態 2 に倣う）、メッセージの名前と形、読み込みを待つ方法。
Stop and hand back if: なし。

## Step 3 — Record now で比べる相手を切り替えて知らせる

Purpose: 手で取ったときに、取れたことと、それと比べていることが分かるようにする。
Specification: [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（手で取ったら切り替えて知らせる、見る対象だけのときは並べる操作、取れなかったときは変えない、
渡しても戻さない、読み込み直すと既定）、[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)。
Prerequisites: Step 2。
May change: `web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live.css`、`scripts/test-live.mjs`、`README.md`、`CHANGELOG.md`。
Done when:
- Record now で取れたら、そのページの比べる相手が取ったスナップショットになり、見比べ方は変わらず、成功の知らせが出る（仮決め 4）。見る対象だけのときは、
  知らせの操作で並べる見比べ方になり、取ったスナップショットが並ぶ。狭い画面では、知らせの操作で比べる相手の 1 枚に切り替わる（仮決め 11）。
- 取れなかったときは比べる相手を変えず、今どおり理由の知らせが出る。
- その後に渡しても比べる相手は取ったスナップショットのまま。レビュー画面を読み込み直すと既定（自動）に戻る。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の成功条件の 2 つ（見る対象だけで手で取る → 知らせ → 操作で並べる、手で取った後に渡しても変わらず読み込み直すと自動）。
- 390px で手で取ると知らせが出て、その操作で比べる相手の 1 枚に切り替わり、取ったスナップショットが出る。
- `snapshotsAreTakenShownAndChosen`・`widthSwitchesWithoutResizingTheDocument` の書き換え（上の「既存の検査の扱い」）。
Left to the implementer: 知らせの見た目と文言（英語。モックの状態 8 に倣う）。
Stop and hand back if: なし。

## Step 4 — 変化を要素ごとにまとめる（純粋な関数）

Purpose: 一覧の行を要素ごとにするための材料を、画面に組み込む前に決める。
Specification: [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)（要素ごとに 1 行、その要素で変わったものを並べる、`html` と `body` をずれただけに出さない）。
Prerequisites: Step 3。
May change: `web/assets/live-diff.js`、`web/assets/live-diff.test.js`。
Done when:
- `live-diff.js` に、性質ごとの Change を要素ごとにまとめる関数がある（鍵は側ごと: 消えた要素は比べる相手の側の番号、ほかは今の側の番号。1 つの要素の色・背景・枠・文字の変化が 1 つにまとまり、どの要素か（種類と文字）と
  どこにあるか（`label`）を持つ）。消えた要素と足した要素もそれぞれ 1 つ。
- `html` と `body` は箱が動いただけではずれただけに入らない。見た目や文字が変わったときは主な変化に入る。
- 足した・消えた以外の要素にも、要素の文字（`excerpt`）が付く。
Shown by: test — RED → GREEN → REFACTOR（`node --test web`）。
- ボタンの文字色・背景色・枠を同時に変えた記述の組から、そのボタンの 1 つのまとまりに 3 つの変化が入る。
- 兄弟の途中の要素を消した記述の組で、消えた要素と、今のページで同じ番号にある別の要素が別々のまとまりになる。
- 兄弟の途中に要素を足してページ全体がずれる記述の組で、`html` と `body` がずれただけに入らない。`body` の背景色を変えた組では `body` が主な変化に入る。
Left to the implementer: 関数の名前と形。
Stop and hand back if: なし。

## Step 5 — 変化の一覧の画面

Purpose: 一覧を要素ごとの行にし、何と比べているか、押すとどこかが分かるようにする。
Specification: [R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)（行、色の前後、押すとスクロールして光る、ずれただけを畳んで説明、凡例）、
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（見出しに比べる相手の名前）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 4。
May change: `web/live/page.js`、`web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live-model.js`、`web/assets/live.css`、`web/assets/*.test.js`、
`scripts/test-live.mjs`、`README.md`、`CHANGELOG.md`。
Done when:
- 一覧は Step 4 のまとまりごとに 1 行で、どの要素か（種類と文字）、どこにあるか、その要素で変わったものが並ぶ。色の前後は色の見本で出る（値の文字だけにしない）。
- 主な変化とずれただけの行を押すと、見る対象がその要素までスクロールして、その要素の印が光る（等倍で横にはみ出すときも見える位置まで）。消えた要素の行は Step 6。
- ずれただけは既定で畳み、それが何かの短い説明がある。印の色の意味の凡例が一覧のそばにある。
- 見出しに、見比べ方によらず、比べる相手の実物の名前（自動なら自動で選ばれた実物。仮決め 12）が出る。見出しの数は要素の数（仮決め 3）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- ボタンの文字色・背景色・枠を同時に変えると、一覧ではそのボタンが 1 行になる。その行を押すと、見る対象がそのボタンまでスクロールし、ボタンの印が光る。
- 見る対象だけの見比べ方で、一覧の見出しの名前が、並べる見比べ方に切り替えたときの比べる相手の選択の文字に含まれる（文言は決めず、含まれることだけを見る。
  [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の同種の成功条件と同じ見方）。
- 兄弟の途中に要素を足しても `html` と `body` がずれただけに出ない。`body` の背景色を変えると `body` が主な変化に出る。
- `changeListFollowsThePage` などの書き換え（上の「既存の検査の扱い」）。
Left to the implementer: 行・色の見本・凡例・説明の見た目と文言（英語。モックの状態 7 に倣う）、page.js のメッセージの名前と形、300 件ずつ出す今の区切りを
要素の数で保つか。
Stop and hand back if: なし。

## Step 6 — 消えた要素の行

Purpose: 消えた要素を、比べる相手の側で見つけられるようにする。
Specification: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（消えた要素は一覧に必ず出す、見る対象だけのときに押すと並べる見比べ方にして光らせる、狭い画面では
比べる相手の 1 枚）、[R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)（消えた要素の行はスナップショットの側のその要素までスクロールして光らせる）、
[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)（スナップショットのスクリプトは止めたまま）。
Prerequisites: Step 5。
May change: `web/assets/features/live.js`、`web/assets/views/live.js`、`web/assets/live-model.js`、`web/assets/live.css`、`web/assets/*.test.js`、`scripts/test-live.mjs`、
`README.md`、`CHANGELOG.md`。
Done when:
- 消えた要素の行を押すと、比べる相手の側がその要素の位置まで動き、その要素の印が光る（仮決め 1）。見る対象だけのときは並べる見比べ方に、狭い画面では比べる
  相手の 1 枚に切り替わってから。重ねて透かすときは見比べ方を変えず、両方をそろえたまま（仮決め 10）。
- ずらした形の間も比べる相手を動かせ、比べる相手・見比べ方・ページ・表示幅のどれかを変えると元の形（枠の中でスクロールする形）に戻る。
- 比べる相手の枠のスクリプトは今どおり動かない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- スナップショットにあって今のページから消した、下の方の要素が一覧に出て、見る対象だけのときにその行を押すと、並べる見比べ方になり、比べる相手の枠の中でその要素が
  見える位置にあり、光っている（画素で確かめる）。
- 390px で同じ行を押すと、比べる相手の 1 枚に切り替わり、同じく見える位置にある。
- 重ねて透かすで同じ行を押すと、見比べ方は重ねて透かすのままで、その要素が見える位置で光り、見る対象と比べる相手の位置の差が今の重ね方の確かめの範囲に収まる。
- 既存の `marksFollowTheChanges`・`scrollingMakesNoChangeAndMarksStay` の比べる相手の枠のスクロールの確かめと、`sideBySidePagesShareTheirTop` の上端揃えが通り続ける。
Left to the implementer: 光らせ方、ずらした形と元の形を切り替える細部（仮決め 1 の範囲で）。
Stop and hand back if: 比べる相手を中身の高さで描くと、上に挙げた既存の検査（比べる相手の枠のスクロール、上端揃え）が通らず、仮決め 1 のやり方では直せない場合。

## Step 7 — モックのファイルの一覧（サーバ）

Purpose: モックのパネルに出すファイルの一覧を、範囲と無視の約束どおりに返す。
Specification: [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（一覧に出すもの、範囲の判定、`.git/`、git の外、上限 200 と全体の数、検索にも上限）、
[R-DEPS](../spec/kemi.md#r-deps-依存)、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
Prerequisites: Step 6。
May change: `crates/kemi-core/src/source/`、`crates/kemi-server/src/api/mock.rs`、`crates/kemi-server/src/api.rs`、`crates/kemi-server/tests/`、`tests/e2e.rs`。
Done when:
- `GET api/mock-files?q=` が、範囲の中の git が無視しない `.html` / `.htm`（追跡中と、無視されない未追跡）を、パスの辞書順で 200 件まで返し、検索に合う全体の数を返す
  （仮決め 5）。`.git/` の中、範囲の外を指すシンボリックリンク、git が無視するもの、消えたものは返さない。サブモジュールの中は返さない（仮決め 6）。
- git の外で起動したときは、範囲の下の `.html` / `.htm` をすべて対象にする（仮決め 7）。
- 新しい依存は無い。ページのトークンが無い要求は今の API と同じく断る。
Shown by: test — RED → GREEN → REFACTOR（`crates/kemi-server/tests/live.rs` と `kemi-core` の単体テスト）。依存は check — `git diff main -- Cargo.toml '*/Cargo.toml' Cargo.lock`
が空で、`cargo deny check licenses` が通る。
- 追跡中の HTML、無視されない未追跡の HTML、git が無視する HTML、`.git/` の中の HTML、範囲の外を指すシンボリックリンクの HTML、`.txt` を置いた作業ツリーで、
  前の 2 つだけが返る。
- git の外の、起動したディレクトリの下の HTML が返る。
- HTML を 201 個置くと 200 件と全体の数 201 が返り、検索で絞ると出ていなかったものが返る。
- 拡張子が大文字の `.HTML` も、git の中と外のどちらでも返る。
Left to the implementer: 関数の置き場所と名前（`source/` の中）、応答の形の細部（画面とサーバの間だけの形で、契約ではない）。
Stop and hand back if: 作業ツリーの監視（`WorkTree`）の振る舞いを変えないと集められない場合。

## Step 8 — モックのパネルと選択の横のメニュー

Purpose: モックを一覧から選んで割り当て、取り消せる形で外せるようにする。
Specification: [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（パネル、選択に名前、選択の横のメニュー、外したときの取り消し、範囲の外と HTML でないものを断る）、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（選択肢の並びの最後）、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（一覧の見出しからのメニュー、
狭い画面のメニューの「モックのファイル」）。
Prerequisites: Step 7。
May change: `web/assets/`（`features/live.js`・`views/live.js`・`live-model.js`・`live.css`・`api.js`）、`web/assets/*.test.js`、`scripts/test-live.mjs`、
`scripts/live-dev-server.mjs`、`README.md`、`CHANGELOG.md`、`PROJECT.md`（新しいモジュールを作ったとき）。
Done when:
- 比べる相手の選択肢の最後に「モックのファイル」が常にあり、選ぶとパネルが開く（選択は変わらない）。パネルに一行の説明、検索の欄、Step 7 の一覧（出している数と
  全体の数、絞り込めること）、パスを入れる欄がある。一覧から選ぶかパスを入れると割り当てられ、範囲の外と HTML でないものは理由が出て断られる。
- 割り当てている間は、選択にファイルの名前が出て、選択の横のメニューに読み直す・外すがある。見る対象だけの間は、変化の一覧の見出し（モックのときは見出しだけ）から
  同じメニューを開ける。
- 外すと、取り消す操作つきの知らせが出て（仮決め 8）、押すと同じモックが比べる相手に戻る。
- 帯の今のモックの入力欄とボタンは無い。狭い画面では帯のメニューに「モックのファイル」の入口がある。
- `--live` でないレビューはページ用のファイルを読まない。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック) の成功条件のうち一覧の 3 つ（追跡中と無視されない未追跡だけが出て絞り込め、無視するものはパスで割り当てられる、
  git の外、201 個で 200 件と全体の数）と、外して取り消すと同じモックに戻ること。
- 見る対象だけのときに、モックを割り当てたページで、変化の一覧の見出しに割り当てたファイルの名前が出て、そこから開いたメニューでモックを外せる
  （[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方) の成功条件）。
- モックの入口を使う既存の検査の書き換え（上の「既存の検査の扱い」）。既存の `otherReviewsLoadNoPageFiles` が通り続ける。
Left to the implementer: パネル・メニュー・知らせの見た目と文言（英語。モックの状態 6 に倣う）、検索を送る間隔、新しいモジュールに分けるか。
Stop and hand back if: なし。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-COMMENT（書く欄を閉じた後の枠の高さ） | Step 1 |
| R-PAGE-COMMENT（場所の行で光る、ページで見る） | Step 2 |
| R-PAGE-REF（手で取ったら切り替えて知らせる） | Step 3 |
| R-PAGE-DIFF（要素ごと、`html` / `body`） | Step 4（純粋な関数）、Step 5（画面） |
| R-PAGE-DIFF（色の見本、押すと光る、ずれただけの説明、凡例） | Step 5 |
| R-PAGE-VIEW（一覧の見出しの名前） | Step 5 |
| R-PAGE-VIEW / R-PAGE-DIFF（消えた要素の行） | Step 6 |
| R-PAGE-MOCK（一覧、範囲、上限 200） | Step 7（サーバ）、Step 8（画面） |
| R-PAGE-MOCK / R-PAGE-REF / R-PAGE-VIEW（パネル、メニュー、取り消し、見出しからのメニュー） | Step 8 |
| R-LIVE（一覧と印はその場で変わる） | Step 5 と今の検査 |
| R-DEPS（新しい依存なし） | Step 7 |
| R-DIST（README・CHANGELOG を同じコミットで） | Step 1（CHANGELOG）・2・3・5・6・8 |
| R-VERIFY（ページ用のファイルを読まない） | Step 8 と各ステップの `otherReviewsLoadNoPageFiles` |

## Left to the implementer

- 関数・型・値・メッセージの名前、モジュールの分け方（`PROJECT.md` の層と features の順の規則を守る。`--live` 用のモジュールは `modulepreload` に載せない例外に入れる）。
- 見た目と文言（UI の文言は英語。モックの状態 2・6・7・8 に倣う）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、エラーの扱い、上限）を決めないと進めない場合で、上の仮決めのどれにも当たらないとき。
- 上の「既存の検査の扱い」に挙げた以外の既存の検査の assert を変える必要が出た場合。
- スナップショットの枠でスクリプトを動かさないと実現できない場合（[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット) に反する）。
- 新しい依存（クレート・npm パッケージ）を足す必要が出た場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 案（[R-PAGE-VARIANT](../spec/live.md#r-page-variant-案)。保留中）、エージェントのスクリーンショット（UL2）、`--live` の性能の数字（UL1）。
- リリースの作業（この計画の後で別に行う）。
