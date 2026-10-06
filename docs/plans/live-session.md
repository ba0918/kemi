# --live ④: 保留と復元の残り

## Goal

`--live` のレビューを保留して復元すると、コメントや発言に加えて、スナップショット（開始時のものを含む）とモックの割り当てが戻る。
`<id>.files/` は 1 セッション 20 MB の規則で古いものから消え、入らなかったものは画面に出る。掃除は `<id>.files/` も数え、
強制終了で取り残された `<id>.files/` も回収する。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（この計画の中心。20 MB の規則で消したもの・保存しなかったものの扱いの段落を含む）
  - [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)（取る時点の種類、1 つ 2 MB）
  - [委譲](../spec/live.md#委譲) の DL3（スナップショットの中の形は実装が決める）
- [比べる相手 仕様](../spec/live-compare.md): [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（既定の比べる相手の決め方。
  開始時のスナップショットが戻らないとこれが崩れる）、[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（保存するのは
  割り当てだけ、出すときにファイルが無ければ読めないことを出す）
- [kemi 仕様](../spec/kemi.md): [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)（`--live` の段落: 1 セッション 20 MB は
  `<id>.files/` で判定、全体の 500 MB に `<id>.files/` も数える、セッションと同じ権限。掃除はセッションの中身を読まずディレクトリの
  一覧で決める、ロック中のものは消さない）、[R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
- 用語は `CONTEXT.md` の「セッション」「保留」「復元」「スナップショット」「モック」「比べる相手」。

## 4 つの計画の中の位置

`--live` は 4 つの計画に分けた。① 中継と見方とスナップショット、② 差分と変化の一覧、③ ページへのコメント（どれも main にマージ済み）、
**④ この計画**。

[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元) のうち、次はもう出来ていて、この計画では作らない（退行させない）:
コメント・返信・発言・解決と折りたたみの保存と復元、コメントの画像を `<id>.files/` に書くこと、submit の確定で `<id>.files/` を消すこと、
会話の無い `--live` のセッションを残さないこと、復元で同じ URL・ファイルにつなぎ直して待つこと、コードの見方の監視、復元の一覧の
`live <URL>`。保存するもののうち「案」は、案（[R-PAGE-VARIANT](../spec/live.md#r-page-variant-案)）そのものが未実装なので、この計画では
扱わない（ライブ改訂の (c)）。

## Approach and why

### 今の作り（調べた事実）

- スナップショットは `crates/kemi-server/src/live/mod.rs` の `LiveInfo.snapshots`（`Mutex<Vec<Snapshot>>`）にメモリでだけ持つ。
  `Snapshot` は id・ページ・表示幅・種類（`start`・`handed`・`manual`）・HTML・要素の記述（`description`。フロントが gzip して base64 に
  した文字列で、サーバは中身を読まない）。id は `api/snapshot.rs` の `take_snapshot` で `format!("s{}", len + 1)`。
- モックの割り当ては `LiveInfo.mocks`（ページ → 範囲の根からの相対パス）にメモリでだけ持ち、`api/mock.rs` の `assign_mock` は保存を呼ばない。
- 復元（`src/main.rs` の `run_resume`）は `LiveRun { page, root }` だけを作り直す。`LiveParams` にもスナップショットやモックを渡す口が無い。
  そのため今は、復元するとサーバのスナップショットが空になり、フロント（`web/assets/features/live.js` の起動時）が開始時のスナップショットを
  取り直す。[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元) の反例そのもの。
- `crates/kemi-core/src/session/store.rs`:
  - `OpenSession::save_file` は `<id>.files/<name>` に書くだけで、大きさを見ない。読み出し・一覧・削除の口は無い。
    サーバ側の `SessionSink`（`crates/kemi-server/src/lib.rs`）も `save_file` だけ。
  - `persist` は、`--live` で会話が無いと `<id>.session` を消し、`<id>.files/` も消す。
  - `cleanup` は `<id>.session`・`<id>.payload`・`<id>.endpoint` だけを名前で振り分ける。大きさに `<id>.files/` を数えず、
    `<id>.session` の無い `<id>.files/` を回収しない。上限は `COPY_LIMIT`（20 MB、`<id>.payload` の判定）だけ。
- コメントの画像は `crates/kemi-server/src/api/comments.rs` の `save_image` が `{コメントの id}.png` で書く。失敗は stderr の
  `Notice::CommentImageNotSaved` だけで、画面には出ない。コメントを消しても画像は残る。
- 保留（`src/main.rs` の `None =>` の枝）は、最後に何も片付けずにロックを手放す。
- セッション形式の版は 3 で、版 3 はまだリリースしていない（v0.1.9 は版 2）。`StateDto` に `#[serde(default)]` の項目を足せば版を上げずに済む。

### 順序

`<id>.files/` の置き方と 20 MB の規則（kemi-core）→ 掃除と取り残しの回収（kemi-core）→ スナップショットの保存と復元（サーバと配線）
→ モックの割り当て → 画面の表示と端から端まで → 文書。

置き方を最初に決めるのは、20 MB の規則・掃除・復元がすべて「`<id>.files/` の中のどのファイルが何か」を前提にするから。ディスクの
決まりを純粋なところで固めてから、サーバとフロントをつなぐ。

### `<id>.files/` の置き方（計画で決める）

- スナップショットは 1 つを `<id>.files/` の中のファイルとして置く（ディレクトリは 1 段）。**種類と取った順はファイルの名前から分かる形にする**
  （20 MB の規則と掃除が、中身を読まずに一覧だけで決められるように）。ページ・表示幅・HTML・記述は名前でも中身でもよい。
  `<id>.session` には入れない。状態を書く費用がスナップショットの数と大きさに左右されないようにするため（[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)
  の `<id>.session` と `<id>.payload` を分けた理由と同じ）。
- コメントの画像は今の名前（`{コメントの id}.png`）のまま。「削除したコメントの画像」は、`<id>.files/` の画像の名前を**サーバのメモリにある
  今のコメントの id**（と、いま書こうとしている画像のコメントの id）と突き合わせて決める。最後に保存した状態（`OpenSession` が持つもの）は
  使わない。`add_page_comment` は画像を書いてからコメントを状態に入れるので、保存済みの状態と突き合わせると、書いたばかりの画像を
  消したものと見誤る。スナップショットの書き込みも、コメントの追加と同じ `state.persist` のロックを取り、この間に割り込まないようにする。
  コメントを消したときに画像を消さない（未受信の渡した知らせがまだそのパスを持っていることがある。③ で受け入れた既知の制約。仕様の消す順に
  従い、上限に当たるまで残す）。
- スナップショットの id は、保存したものと、そのレビューの間に取ったものの中で重ならないようにする（今の `len + 1` は、途中を消すと重なる）。
  保存しなかったものは復元すると消えるので、復元の後にその id を使い直してもよい（計画で決めたこと。仕様は id を決めていない）。
- 1 セッションの大きさは `<id>.files/` の中のファイルの合計で判定する（`<id>.session` は数えない。[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)）。
  新しいものを書く前に「今の合計 + 新しいもの」が 20 MB を超えるかを見て、超えるなら仕様の順に消してから書く。開始時のスナップショットと、
  今あるコメントの画像は消さない。それでも入らなければ書かない。
- 消すものと書けるかの判断は、ディスクに触らない純粋な関数にする（入力: `<id>.files/` のファイルの名前と大きさ、今のコメントの id の集合、
  新しいものの大きさ → 出力: 消すものと、書けるか）。順番の決まりを境界の値でテストしやすくするため。
- 20 MB の上限は `crates/kemi-core` の外から差し替えられるようにする（`SessionStore` の作り方で上限を渡す、など。既定は 20 MB）。
  サーバの統合テストが小さな上限で規則を確かめるため。依存を外から入れる形で、試験専用の分岐ではない。
- スナップショットを書いても、セッションの最終更新（`info.updated`）は進めない（今と同じく、状態の変更でだけ進む。計画で決めたこと）。
- 復元で読めないスナップショットのファイル（壊れている、形が違う）は、飛ばして stderr に警告を出し、復元は続ける（計画で決めたこと。
  仕様は黙っている。状態の書き込みの失敗を警告だけにする [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元) の扱いに倣う）。

### いつ書いて、いつ消すか（計画で決める）

スナップショットは取ったときにすぐ `<id>.files/` に書く。会話がまだ無い間も書く。そのかわり、レビューの途中では会話が無くなっても
`<id>.files/` を消さず（`<id>.session` だけを消す）、保留で終わるときに、会話が無ければ `<id>.files/` を消す。

理由: 今の `persist` のように会話が無いたびに `<id>.files/` を消すと、「開始時のスナップショットを取る → コメント無しで見たの印を付ける」
の時点で開始時のスナップショットがディスクから消え、後で最初のコメントを付けて保留しても戻らない。書くのを会話ができるまで待つ方法もあるが、
メモリとディスクのずれを持ち越すことになる。起動中のレビューはロックを持つので、掃除がこの `<id>.files/` を取り残しと見誤ることは無い。
強制終了で `<id>.session` の無い `<id>.files/` が残っても、ロックが外れているので掃除が回収する。

仕様の読み方: [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元) は `<id>.files/` を「セッションを消すときに一緒に消す」と書く。
レビューの途中で会話が無くなって `<id>.session` を消すのは、ロックを持ったまま続くレビューの、保存するものが一時的に無い状態であって、
セッションを消すことではないと読む（セッションが終わるのは submit の確定・保留・掃除）。保留で会話が無ければそこで `<id>.files/` も消すので、
終わった後に `<id>.files/` だけが残ることは無い（強制終了を除き、それは掃除が回収する）。

これは ③ の単体テスト `a_live_session_whose_conversation_is_gone_removes_its_files_directory` が確かめている振る舞い（会話が無くなったら
その場で `<id>.files/` を消す）を変える。このテストは「保留で終わるときに消える」を確かめる形に書き換えてよい（Step 1 の止まる条件の例外として明記）。
同じ理由で、`crates/kemi-server/src/api/comments.rs` の `add_page_comment` が `state.persist` のロックを取る理由のコメント（空の保存で
`<id>.files/` ごと消えるから）と、`crates/kemi-server/tests/live.rs` の
`a_page_comment_keeps_its_image_when_the_session_is_emptied_while_the_image_is_written` は意味が変わる。Step 3 で見直す（ロック自体は、
上の「削除したコメントの画像」の取り違えを防ぐために残す）。

保留の口は Step 3 で `src/main.rs` につなぐ。Step 1・2 のコミットの間は、画像を持つコメントを消してから保留すると `<id>.session` の無い
`<id>.files/` が残るが、Step 2 の掃除が回収するので受け入れる（途中の状態）。

### 画面とメモリ

20 MB の規則で消したスナップショットは、動いているレビューの比べる相手の選択肢からも消す。保存できなかった新しいスナップショットは、
そのレビューの間は比べる相手として使え、保存していないので復元すると消えることを画面に出す（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)
の追記、986cbe8）。コメントの画像が 20 MB の規則で保存できなかったときも、そのことを画面に出す（`image` は今どおり `null`）。

20 MB の規則以外の理由（書き込みの失敗）でスナップショットを保存できなかったときは、stderr に警告を出し（[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)
の書き込みの失敗の扱い）、画面でも 20 MB のときと同じ「保存していない」の印を付ける（計画で決めたこと。実際に保存していないので、印は事実と合う）。
コメントの画像の書き込みの失敗と base64 の誤りは、今どおり stderr だけ。

画面がこれを知る経路（計画で決める）: スナップショットの一覧と取ったときの応答に、保存したかどうかを載せる。別の要求（例: コメントの画像の
保存）が原因で古いスナップショットが消えたときは、サーバが SSE で「スナップショットが変わった」ことを知らせ、画面が一覧を読み直す
（今の SSE の `Event` に種類を 1 つ足す）。ページを読み込み直さずに選択肢から消える。

選んでいたスナップショットが消えたときは、今どおり「記録されていない」と出る（[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)
は既定に戻すとは書いていない。今のコードも選んだ id が無ければ記録されていないになる）。

セッションを作れないレビュー（セッションの置き場所が決まらない、など）では、今どおりスナップショットはメモリにだけ持ち、20 MB の規則は掛けない。

### モックの割り当て

割り当て（ページ → 範囲の根からの相対パス）をセッション状態に `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]` で足し
（`mod.rs` に同じ形の前例がある）、割り当て・外すたびに状態を書く。割り当てだけでは会話にならない（会話が無ければセッションは残らない。
仕様どおり）。復元で `LiveInfo.mocks` に戻す。比べる相手の選択（フロントの `live.chosen`）は保存しない（仕様が保存するものに挙げていない）。

出すときにファイルが無ければ、読めないことを画面に出す（[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)）。調べた事実: 今の画面に
この表示は無い。`features/live.js` の `showMock` は iframe の `src` を差し替えるだけで、`api/mock.rs` の `mock_file` は無いファイルに 404 と本文
`kemi: not found` を返す（それが sandbox の枠に出るだけ）。枠は別オリジンなので中から状態は読めない。見分け方（モックを出し始めるときに
画面から同じ URL の状態を確かめる、割り当ての一覧に読めるかを載せる、など）は実装者が決める。どれでも「出し始めるときに読む」は変わらない。

## Scope of change

- Rust: `crates/kemi-core/src/session/`（`store.rs`・`mod.rs`）、`crates/kemi-server/`（`lib.rs` の `SessionSink`、`live/mod.rs`、
  `api/snapshot.rs`、`api/mock.rs`、`api/comments.rs`、`session.rs`）、`src/`（`main.rs` の復元と保留の配線、`session.rs`、`notice.rs`）
- テスト: `tests/e2e.rs`、`crates/kemi-core` と `crates/kemi-server` のテスト（`crates/kemi-server/tests/live.rs` を含む）、`web/assets/*.test.js`、
  `scripts/test-live.mjs`、`scripts/live-dev-server.mjs`
- ページ: `web/assets/` の下（`features/live.js`・`views/live.js`・`live-model.js`・`live.css`・`api.js`）。`web/live/page.js` は変えない見込み
- 文書: `README.md`（Sessions と Live review の節）、`CHANGELOG.md`（Unreleased）、`PROJECT.md`（モジュールを足したときだけ）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/`、`skills/kemi/` は変えない（submit と `kemi wait` の JSON は変わらない）。

## Step order and prerequisites

Step 1 → 2 → … → 6 の順。各ステップは前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。WSL の負荷のため。
cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`
3. ブラウザの検査があるステップ（Step 5）: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   `node scripts/test-live.mjs target/release/kemi` と、既存の 4 本（`test-agent-channel.mjs`・`test-narrow-screen.mjs`・
   `test-rendered-view.mjs`・`test-horizontal-scroll.mjs`）を 1 本ずつ回す。

---

## Step 1 — `<id>.files/` の置き方と 20 MB の規則

Purpose: スナップショットを `<id>.files/` に置いて読み戻せるようにし、20 MB の規則（消す順と書けないとき）を決める。
Specification: [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（保存するもの、20 MB の規則と消す順、開始時は消さない、
保留しても残さない）、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)（`--live` の段落、権限）、[委譲](../spec/live.md#委譲) の DL3。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/`（とそのテスト）。
Done when:
- `OpenSession` に、スナップショット（種類・ページ・表示幅・HTML・要素の記述）を `<id>.files/` に書く口、置いてあるスナップショットを
  読み戻す口がある。書いたファイルは今の `save_file` と同じ権限（unix で `0600`、ディレクトリ `0700`）。
- 書く前に 20 MB の規則を掛ける: 今の `<id>.files/` の合計と新しいものの大きさから、古い渡した時点のスナップショット → 古い手で取った
  スナップショット → 削除したコメントの画像（状態のコメントの id に無い画像）の順に消し、入るなら書く。開始時のスナップショットと、
  今あるコメントの画像は消さない。入らなければ書かず、書かなかったこと（と消したスナップショット）を呼ぶ側に返す。コメントの画像の
  書き込み（`save_file`）にも同じ規則を掛ける。
- 消すものと書けるかを決める処理は、ディスクに触らない純粋な関数で、ファイルの名前と大きさ・今のコメントの id の集合を入力にとる。
- 20 MB の上限を外から渡せる（既定は 20 MB）。
- `--live` で会話が無いとき、`persist` は `<id>.session` だけを消し、`<id>.files/` を残す。保留で終わるときに呼ぶ口（名前は任意）があり、
  会話が無ければ `<id>.files/` を消す。
Shown by: test — RED → GREEN → REFACTOR。
- 純粋な関数: 渡した時点が手で取ったものより先に、古いものから消える。削除したコメントの画像はスナップショットの後に消える。開始時と
  今あるコメントの画像は残る。それでも入らなければ「書けない」。ちょうど 20 MB に収まるときは書ける（境界）。
- `OpenSession`: 書いたスナップショットを、別の `OpenSession` で開き直して読み戻すと、種類・ページ・表示幅・HTML・記述が同じ。
  unix でスナップショットのファイルが `0600`、`<id>.files/` が `0700`（今の `a_session_file_is_written_into_its_files_directory_for_the_owner_only` に倣う）。
- `OpenSession`: 読めないスナップショットのファイルが混ざっていても、ほかのものは読み戻せる。
- `OpenSession`: 合計が上限に近い `<id>.files/` に書くと、規則どおりのファイルが消え、新しいものが書かれる（小さな上限を渡して試す）。
- 会話の無いまま見たの印だけで `persist` しても、書いたスナップショットが残る。保留の口を呼ぶと消える。会話があれば保留の口を呼んでも残る。
- ③ の `a_live_session_whose_conversation_is_gone_removes_its_files_directory` を、「会話が無くなった後に保留の口を呼ぶと消える」を
  確かめる形に書き換える（上の「いつ書いて、いつ消すか」）。
Left to the implementer: ファイルの名前の具体的な形（種類と取った順が名前から分かること）と中の形（1 ファイルにまとめるか、HTML と記述を
分けるか。記述の base64 を解いて書くか）、スナップショットの id の決め方（上の「置き方」の範囲で重ならなければよい）、上限の渡し方、
口の名前と戻り値の型。
Stop and hand back if: 上に書いた ③ のテスト以外で、既存のセッションのテストの assert を変える必要が出た場合。セッション形式の版を
上げる必要が出た場合。

## Step 2 — 掃除の数え方と取り残しの回収

Purpose: 掃除が `<id>.files/` も数え、`<id>.session` の無い `<id>.files/` を回収するようにする。
Specification: [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)（`--live` の段落の 500 MB、掃除はセッションの中身を読まない、
ロック中は消さない・数えない）、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（保留しても残さない）。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/session/store.rs`（とそのテスト）。
Done when:
- `cleanup` が 1 セッションの大きさを `<id>.session` + `<id>.payload` + `<id>.files/` の中のファイルの合計で数え、件数とバイト数の上限で
  消すときに 3 つとも消す（`<id>.files/` を消すのは今もしている）。
- `<id>.session` の無い `<id>.files/` を、ロックを持つセッションのものでなければ消す（`<id>.payload` の今の回収と同じ形）。
- 判定に `<id>.session` の中身を読まない。`<id>.files/` は一覧と大きさ（メタデータ）だけを読む。
Shown by: test — RED → GREEN → REFACTOR（`crates/kemi-core` の単体テスト）。
- `<id>.files/` が大きいために合計が上限を超えるとき、最終更新の古いセッションが `<id>.files/` ごと消える（`<id>.session` と
  `<id>.payload` だけなら上限に収まる組み立てで）。
- `<id>.session` の無い `<id>.files/` を置いてからセッションを書くと消える。ロックを持つセッションのものは消えない。
Left to the implementer: `<id>.files/` の大きさの数え方（中を 1 段だけ読む、など。Step 1 の置き方が 1 段なら 1 段でよい）。
Stop and hand back if: 掃除を、状態を書くたびに走らせると目に見えて遅くなる測定が出た場合（今の `cleanup` は書くたびに走る）。

## Step 3 — スナップショットの保存と復元（サーバ）

Purpose: 取ったスナップショットがセッションに残り、復元で戻るようにする。20 MB の規則の結果をサーバのメモリにも映す。
Specification: [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（成功条件 1 のスナップショットの部分、反例、20 MB で消したものは
選択肢からも消える、保存しなかったものはそのレビューの間は使える）、[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（復元した後も、開始時のスナップショットが既定の比べる相手の候補に残る）。
Prerequisites: Step 1、Step 2。
May change: `crates/kemi-server/`、`crates/kemi-core/src/session/`、`src/`、`tests/e2e.rs`、各クレートのテスト。
Done when:
- `take_snapshot` が、セッションがあればスナップショットを Step 1 の口で書く（`state.persist` のロックの中で）。20 MB の規則で消した
  スナップショットは `LiveInfo.snapshots` からも消える。書けなかったもの（20 MB に入らない、書き込みの失敗）は `LiveInfo.snapshots` に残り、
  保存していないことが一覧と取ったときの応答で分かる。書き込みの失敗は stderr にも出る。セッションが無いレビューでは今どおり。
- コメントの画像の保存（`save_image`）も規則を通り、消えたスナップショットが `LiveInfo.snapshots` からも消える。20 MB の規則で画像を
  書けなかったことが、コメントの追加の応答で分かる（`image` は今どおり `null`）。
- 別の要求が原因でスナップショットが消えたとき、SSE で「スナップショットが変わった」ことを知らせる。
- 復元（`run_resume`）で、保存したスナップショットが `LiveInfo.snapshots` に戻る。読めないものは飛ばして stderr に出す。戻した後に取った
  スナップショットの id は、戻したものと重ならない。一覧に開始時のものが戻るので、今のフロント（`features/live.js` の起動時）は開始時の
  スナップショットを取り直さない。
- 保留（`src/main.rs` の保留の枝）と実行時エラーで終わるときに、サーブのタスクが終わった後（`serving` を abort して待った後）、
  `drop(stored_session)` の前に Step 1 の保留の口を呼ぶ（処理中の `take_snapshot` が後から書き足さないように）。
- `add_page_comment` のロックの理由のコメントを、上の「いつ書いて、いつ消すか」に合わせて直す。`a_page_comment_keeps_its_image_when_the_session_is_emptied_while_the_image_is_written`
  が何も確かめなくなっていれば、取り違えの防止（書いている画像が消したコメントの画像と見誤られない）を確かめる形に直すか外す。
Shown by: test — RED → GREEN → REFACTOR。
- e2e: `--live` で開始時のスナップショットを API で取り、コメント無しで見たの印を付け、その後コメントを足してから SIGINT で保留し、
  `--resume` すると、`api/snapshots` に開始時のスナップショットがあり、`api/snapshot/{id}` が同じ HTML と記述を返す
  （[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元) の成功条件 1 と反例。上の「いつ書いて、いつ消すか」の順序の退行防止）。
- e2e: コメント無しで保留すると、`<id>.session` も `<id>.files/` も残らない。
- 統合テスト（`crates/kemi-server/tests/live.rs`、`kemi-core` の `SessionStore` に小さな上限を渡したセッションで）: 上限に近いときに
  スナップショットを取ると、古い渡した時点のものが一覧から消える。開始時のものと今あるコメントの画像だけで埋まっているときに取ると、
  一覧に残り、保存していない印が付く。コメントの画像の保存が原因で消えたとき、SSE の知らせが届く。
- 統合テスト: 復元した後に取ったスナップショットの id が、戻したものと重ならない。
Left to the implementer: `SessionSink` の口の形（スナップショットを書く・読み戻すメソッドを足す、など。テストの sink（`crates/kemi-server/tests/`
の 4 つ）を追従させるか、既定の実装を付けるかも任せる。assert を変えない追従は止まる条件に当たらない）、復元したスナップショットを
`LiveParams` か `prepare` に渡す経路、保存していない印・画像を書けなかったこと・SSE の種類の名前（画面への応答だけで使い、submit・
`kemi wait` の契約には入れない）。
Stop and hand back if: submit や `kemi wait` の JSON の形を変える必要が出た場合（契約の変更になる）。

## Step 4 — モックの割り当ての保存と復元

Purpose: モックの割り当てがセッションに残り、復元で戻り、ファイルが無ければそのことが出るようにする。
Specification: [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（保存するのは割り当てだけ、ファイルが無ければ読めないことを出す、
成功条件 4）、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（保存するもの、会話が無ければ残さない）、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（割り当てたページの既定はモック）。
Prerequisites: Step 3。
May change: `crates/kemi-core/src/session/`、`crates/kemi-server/`、`src/`、`tests/e2e.rs`、各クレートのテスト。
Done when:
- セッション状態に割り当て（ページ → 範囲の根からの相対パス）が上の「モックの割り当て」の形であり、割り当て・外すたびに状態を書く。
  版 2 と、この項目の無い版 3 の `<id>.session` は、割り当て無しとして読める。割り当てが無いときは書き出す形が今と変わらない。
- 割り当てだけで会話が無ければ、保留してもセッションは残らない。
- 復元で割り当てが `LiveInfo.mocks` に戻り、`api/mocks`（今の一覧の口）に出る。
Shown by: test — RED → GREEN → REFACTOR。
- e2e: モックを割り当て、コメントを足して保留し、`--resume` すると、モックの一覧に同じページとパスが出る。
- 単体テスト（`crates/kemi-core` の session）: 割り当てを持つ状態を書いて読み戻すと同じ。割り当ての項目の無い `<id>.session` が読める
  （版 2 の固定のバイト列の今のテストが通り続けることで足りるなら、新しいテストは足さない）。
Left to the implementer: 割り当てを `Session`（サーバの状態）と `LiveInfo` のどちらに持たせるか（保存される状態に入ること、復元で
`LiveInfo.mocks` と同じ中身になることを守る）。
Stop and hand back if: なし。

## Step 5 — 画面の表示と、端から端まで

Purpose: 20 MB の規則の結果とモックが読めないことを画面に出し、保留と復元をブラウザで確かめる。
Specification: [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（保存しなかったことを画面に出す、成功条件 1）、
[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)（成功条件 4）、[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)、
[R-VERIFY](../spec/kemi.md#r-verify-検証)。
Prerequisites: Step 4。
May change: `web/assets/` の下、`scripts/test-live.mjs`、`scripts/live-dev-server.mjs`、`PROJECT.md`（モジュールを足したときだけ）。
モックが読めないことの見分けにサーバの手が要るなら `crates/kemi-server/` も。
Done when:
- 保存していないスナップショットを選ぶと（または一覧で）、保存していないので復元すると消えることが出る。SSE の知らせを受けると一覧を
  読み直し、消えたスナップショットがページを読み込み直さずに比べる相手の選択肢から消える。選んでいたものが消えたときは今どおり
  「記録されていない」が出る。
- 20 MB の規則でコメントの画像を保存できなかったとき、そのことが出る。
- 割り当てたモックのファイルが無いとき、比べる相手の場所に読めないことが出る（今の 404 の本文が枠に出るだけの状態を置き換える）。
Shown by: test — RED → GREEN → REFACTOR、ブラウザ自動化（`scripts/test-live.mjs`）。
- ブラウザ自動化: コメントと手で取ったスナップショットのあるレビューを保留して復元すると、比べる相手の選択に開始時と手で取った
  スナップショットが出て、開始時のものが既定になる（渡す前のレビュー）。開始時のスナップショットの id と HTML が保留の前と同じで、
  開始時のものが 1 つだけ（取り直していない）。コメントも出る（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元) の成功条件 1 と反例）。
- ブラウザ自動化: モックを割り当てて保留し、モックのファイルを消してから復元すると、読めない旨が出る（[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック) の成功条件 4）。
- 単体テスト（`web/assets/*.test.js`、純粋な読み）: 保存していない印を持つスナップショットの見せ方、消えたスナップショットを選んでいたときに
  記録されていないになること、画像を保存できなかったことの見せ方。20 MB の規則そのものは Step 1・3 で確かめたので、ブラウザでは本物の
  20 MB を作らない。SSE を受けて一覧を読み直す配線は、Step 3 の統合テスト（知らせが届く）とこの読みのテストの組み合わせで足りるとする。
Left to the implementer: 表示の文言（英語、今の流儀）と置き場所（画面モックの案 A に倣う）、保存していない印の見た目。
Stop and hand back if: ② の差分の既定の比べる相手の検査（`test-live.mjs` の既存のもの）の assert を変える必要が出た場合。

## Step 6 — 文書

Purpose: 利用者向けの文書を、`--live` の保留と復元に合わせる。
Specification: [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)。
Prerequisites: Step 5。
May change: `README.md`、`CHANGELOG.md`、`PROJECT.md`。
Done when: README の Sessions と Live review の節が、`--live` の保留で残るもの（スナップショットとモックの割り当て）、`<id>.files/` の 20 MB の
規則、会話の無い `--live` は残らないことを書いている。CHANGELOG の Unreleased にこの変更がある。
Shown by: check — 各ステップの終わりの検査のすべてを、上の順に実行する。
Left to the implementer: 文の書き方（README は英語、ほかは日本語）。
Stop and hand back if: なし。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-SESSION（保存するもの: スナップショット） | Step 1（置き方）、Step 3（e2e）、Step 5（ブラウザ） |
| R-PAGE-SESSION（保存するもの: モックの割り当て） | Step 4（e2e）、Step 5（ブラウザ） |
| R-PAGE-SESSION（20 MB の規則と消す順、開始時は消さない） | Step 1（純粋な関数）、Step 3（統合） |
| R-PAGE-SESSION（消したものは選択肢からも、保存しなかったものは使えて印） | Step 3（統合）、Step 5（読みの単体テスト） |
| R-PAGE-SESSION（会話が無ければ残さない） | Step 1、Step 3（e2e）、Step 4 |
| R-PAGE-SESSION（反例: 開始時のスナップショットが消える） | Step 3（e2e）、Step 5（id と HTML が同じで 1 つだけ） |
| R-SESSION（`--live`: 500 MB に `<id>.files/`、掃除、権限） | Step 1（権限）、Step 2 |
| R-PAGE-MOCK（成功条件 4: 消してから復元すると読めない旨） | Step 5 |
| R-PAGE-REF（復元後の既定の比べる相手） | Step 5 |
| R-DIST | Step 6 |
| R-VERIFY（ページ用のファイルを読み込まない） | Step 5 の `test-live.mjs` と既存の 4 本 |

## Left to the implementer

- `<id>.files/` のファイルの名前と中の形、スナップショットの id の決め方（Step 1）、`SessionSink` の口と復元の経路と画面への伝え方（Step 3）、
  割り当ての持ち場所（Step 4）、表示の文言と見た目（Step 5）。
- モジュールの分け方と名前（`PROJECT.md` の層と features の順の規則を守る）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、保存先、エラーの扱い、上限）を決めないと進めない場合。
- submit・`kemi wait` の JSON の形を変える必要が出た場合。
- 今のモードや ③ までの既存のテストの assert を、仕様を変えずに書き換える必要が出た場合（Step 1 と Step 3 に書いた ③ のテストを除く。
  assert を変えないテストの道具の追従は当たらない）。
- セッション形式の版を上げる必要が出た場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 案（R-PAGE-VARIANT）とその保存（ライブ改訂の (c)）。
- 比べる相手の選択（どの時点を選んでいたか）の保存。仕様が保存するものに挙げていない。
- submit の確定で `<id>.files/` を消した後に、未受信の渡した知らせが消えたパスを返しうること（③ で受け入れた既知の制約）。
- 画像を見る対象にすること（(d)）。
- 20 MB の規則で古い渡した時点のスナップショットが消えたときに、画面の「Handed n」の番号が詰まって付け直されること（`live-model.js` の
  `snapshotLabel` は並びの位置で数える。仕様は番号を決めていない）。
- 要素の記述（`description`）に 2 MB の上限が無く、1 つのスナップショットが 20 MB の枠を大きく占めうること（仕様の 2 MB は HTML だけ）。
