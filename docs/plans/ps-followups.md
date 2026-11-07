# ps の後の補い: 掃除の排他・ps の確かめ直し・つなぎ先の置き場所の掃除

## Goal

Windows でつなぎ先と起動中の記録がセッションと別の置き場所にあっても、強制終了の後の残り物を掃除とセッションの削除が片付け、
しかも別のセッションの置き場所で動いているレビューのものは消さないようにする。あわせて、実装済みで仕様に書き足した掃除の排他・
id でない名前に触らない掃除・ps の確かめ直しに、仕様の成功条件のテストを付ける。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。用語は `CONTEXT.md`（特に「起動中」
「起動中の記録」「セッション」「保留」「復元」）。

- [R-SESSION セッションの保存と復元](../spec/session.md#r-session-セッションの保存と復元)
  - 要件のうち、ロックの取り直しの項と、「掃除が見るのは…」から「つなぎ先と起動中の記録の置き場所は…」までの 4 項
  - 成功条件のうち、取り直しの 3 項（200 ミリ秒・2 秒・新しいセッション）、`foo.payload` の項、片付けている間のロックの項、
    4 つの id の項
  - 反例の、掃除と Windows の項
  - 末尾の P31、D13、R48〜R50
- [R-PS-RUNNING 起動中の判定と対象](../spec/ps.md#r-ps-running-起動中の判定と対象)（確かめ直しの項、その成功条件と反例）と D19
- [R-PS-RECORD 起動中の記録](../spec/ps.md#r-ps-record-起動中の記録)（寿命・掃除・Windows の置き場所の項と、SIGKILL の成功条件）
- [R-AGENT-LINK つなぎ先と安全](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)（`<id>.endpoint` の置き場所と寿命。読むだけ）

## Approach and why

### 今の作り（調べた事実）

- ロックは `crates/kemi-core/src/session/lock.rs`。`SessionLock::acquire_within` が新しいレビュー（`SessionStore::create`）と
  復元（`SessionStore::open`）で 1 秒取り直す。`is_locked` はロックのファイルを作らずに開いて確かめる。掃除が使う
  `with_free_lock` はロックのファイルを開いて（無ければ作って）取り、持ったまま渡された処理をしてから、名前を外さずに閉じる。
- 掃除は `crates/kemi-core/src/session/cleanup.rs` の `cleanup_at`。`classify` がセッションの置き場所の一覧を名前で振り分け
  （id でない名前は飛ばす）、`remove_records_of_stopped_reviews` が `.endpoint` か `.running` のある id を `with_free_lock` の中で
  消す。見るのはセッションの置き場所だけで、つなぎ先の置き場所は知らない。呼ぶのは `OpenSession` の状態を書く 3 か所
  （`store.rs`）。
- `OpenSession::delete`（`store.rs`）は、セッションの置き場所から `<id>.endpoint` と起動中の記録を消す。
- 置き場所は `crates/kemi-core/src/result.rs` の `sessions_dir` と `endpoint_dir`、本体では `src/results.rs` の `sessions_dir()` と
  `endpoints_dir()`。unix では同じ。Windows では、つなぎ先と記録は `XDG_STATE_HOME` に関わらず `%LOCALAPPDATA%\kemi\sessions\` で、
  セッションとロックは `XDG_STATE_HOME` が絶対パスならその下、そうでなければ同じ `%LOCALAPPDATA%\kemi\sessions\`。
- 本体で `SessionStore` を作るのは `src/input.rs`（新しいレビュー。`StoredSession::start`）、`src/resume.rs`（復元。
  `StoredSession::resume`）、`src/ps.rs`、`src/agent.rs`。掃除が走るのは前の 2 つの経路だけ。
- ps の確かめ直しは `src/ps.rs` の `read_running`。`running_ids` の後に `RECHECK_DELAY`（100 ms）眠ってから `is_running` で
  確かめ直す。待ちを外から差し替える口は無い。
- 既にあるテスト（`crates/kemi-core/src/session/store/tests/`）:
  - `ids_and_locks.rs` の `new_session_lock_waits_out_a_brief_hold`、`resume_lock_waits_out_a_brief_hold`、
    `resume_lock_gives_up_after_a_second` は、取り直しの 3 つの成功条件を満たしている（「2 秒持っている間」の項は、5 秒まで持たせて
    2 秒より前に断られることを見る形で、意図を満たす）。この計画では触らない。
  - 同じプロセスの別のスレッドが持つロックを `is_running` が見分けることは、`ids_and_locks.rs` の
    `a_running_session_is_told_apart_from_a_dead_one` と `running_ids_are_the_sessions_whose_lock_is_held` が確かめている。
  - `cleanup.rs` の `cleanup_removes_the_endpoints_and_running_records_whose_lock_is_released` は、つなぎ先だけ・記録だけの id を
    ロックのファイルもセッションも無い形で置き、消えることを確かめている。新しい決まり（自分のセッションの置き場所に
    `<id>.session` かロックのファイルがある id だけを消す）では、この形は消えなくなる。
  - id でない名前に触らない掃除（`foo.payload` の成功条件）と、片付けている間のロックの成功条件には、テストが無い。
- e2e の `kemi_command`（`tests/e2e.rs`）は `XDG_STATE_HOME` を `state` に、`LOCALAPPDATA` を `state/localappdata` に向ける。
  だから Windows の CI では、つなぎ先の置き場所とセッションの置き場所が分かれる。`tests/e2e/ps.rs` の
  `killed_review_is_not_listed_and_its_record_is_cleaned_later` は、SIGKILL を使うので `#[cfg(unix)]` になっている。

### 進め方

1. 実装済みの振る舞いにテストを付けるステップ（1、2）を先に置く。この 2 つは振る舞いを変えないので、RED は「確かめ直しや排他を
   一時的に外すとテストが落ちる」ことで示す。外した変更は残さない。
2. 振る舞いを変えるのはステップ 3（kemi-core）とステップ 4（本体の配線）。kemi-core に「つなぎ先の置き場所」を受け取る口を作り、
   掃除と削除がそちらを見るようにする。本体は、新しいレビューと復元の `SessionStore` に `endpoints_dir()` を渡す。
3. ステップ 5 で変更履歴に書く。

### 仕様が決めていないので計画で決めたこと（仮の答え）

- **A1 つなぎ先の置き場所が分かれているとき、掃除がつなぎ先と記録を探して消すのは、つなぎ先の置き場所だけにする。**
  セッションの置き場所にある `.endpoint` と `.running` は探さない。今の kemi は、Windows ではつなぎ先と記録を必ず
  `%LOCALAPPDATA%` 側に書き、セッションの置き場所には書かないので。置き場所が同じなら（unix、または `XDG_STATE_HOME` の無い
  Windows）、今と同じく 1 つの一覧から探す。
  覆す条件: 「前の版の kemi が Windows で `XDG_STATE_HOME` 側につなぎ先を書いていたので、そちらの残りも消してほしい」。
- **A2 Windows で置き場所が分かれる経路の e2e は、Windows の CI でしか RED にならないことを受け入れる。**
  開発機（Linux）では 2 つの置き場所が同じなので、ステップ 4 の e2e は配線の前から通る。配線の効き目は、kemi-core の単体テスト
  （ステップ 3。どの OS でも置き場所を分けて走る）と、push の後の Windows の CI で確かめる。
  覆す条件: 「Windows で RED を見てからでないと進めたくない」。そのときは、配線の前の版を Windows の CI で一度走らせる。
- **A3 `cleanup_removes_the_endpoints_and_running_records_whose_lock_is_released` は、つなぎ先だけ・記録だけの id にもロックの
  ファイルを残した形（強制終了したレビューの形）に直す。** 仕様の新しい決まりに合わせるためで、テストが守ってきた「どちらか片方
  だけのファイルでも片付けの対象にする」ことは保つ。
  覆す条件: 「ロックのファイルも無いつなぎ先だけの id も消してほしい」。これは仕様の変更なので brainstorm に戻る。
- **A4 「片付けている間は、復元も新しいセッションもロックを取れない」の成功条件は、ロックを 1 回だけ取りに行って取れないことで
  確かめる。** `open` と `create` は 1 秒取り直すので、そのまま使うとテストが 1 秒待つ。どちらも同じ「開いて取りに行く」処理
  （`lock.rs` の `try_acquire`）を使うので、1 回の取りに行きで取れないことは、両方が取れないことと同じ。
  覆す条件: 「`open` / `create` そのもので確かめたい」。そのときは、1 秒待つテストを受け入れる。
- **A5 `src/resume.rs` の配線は、差分のレビューで確かめる。** 復元の経路で掃除が起きるのは状態を書いたときで、強制終了の残り物の
  掃除を復元から起こす e2e を作るには、保留中のセッションの用意と状態の書き込みが要る。kemi-core の掃除の決まりはステップ 3 の
  単体テストが、新しいレビューの配線はステップ 4 の e2e（Windows の CI）が見るので、残るのは 1 行の配線だけ。
  覆す条件: 「復元の配線にも e2e がほしい」。

## Scope of change

- `crates/kemi-core/src/session/`（`cleanup.rs`、`store.rs`、`lock.rs`、`mod.rs`）と、その `store/tests/`
- `src/ps.rs`、`src/input.rs`、`src/resume.rs`、必要なら `src/results.rs`
- `tests/e2e/ps.rs`、必要なら `tests/e2e.rs` の補助
- `CHANGELOG.md`

`crates/kemi-server` は触らない（`SessionStore` を受け取るだけなので）。仕様（`docs/spec/`）と `CONTEXT.md` も変えない。

## Step order and prerequisites

ステップ 1 と 2 は互いに独立。ステップ 3 は 2 の後にする（同じ `cleanup.rs` と `tests/cleanup.rs` を触るので、2 で付けたテストが
3 の書き換えを見張る）。ステップ 4 は 3 の後、ステップ 5 は 3 と 4 の後。

## Step 1 — ps の確かめ直しのテスト

Purpose: ps が、確かめ直すまでにロックが放された id を出さないことを、時間に頼らないテストで確かめる。
Specification: [R-PS-RUNNING 起動中の判定と対象](../spec/ps.md#r-ps-running-起動中の判定と対象)（確かめ直しの項と、その成功条件）、D19。
Prerequisites: なし。
May change: `src/ps.rs`（その中のテストを含む）。
Done when: `read_running` の、最初の確認と確かめ直しの間の待ちをテストから差し替えられる。別のスレッドがセッションのロックを持ち、
最初の確認で起動中に見えた後、差し替えた待ちの中でそのロックを放すと、返る一覧にその id が無い（`read_running` は `<id>.session` を
見ないので、`SessionStore::create` で開いて持つだけで「保留中のセッション」の役になる）。本番の待ちは今の `RECHECK_DELAY` の
まま、全部の id についてまとめて 1 回だけ待つ。
Shown by: test — `src/ps.rs` のテストに、上の場面のテストを 1 本足す（名前は実装に任せる。例:
`read_running_drops_an_id_released_before_the_recheck`）。RED は、確かめ直し（`filter(|id| store.is_running(id))`）を一時的に外すと
落ちることで示し、その変更は残さない。本番の待ちが 1 回で `RECHECK_DELAY` のままであることは、差分のレビューで確かめる。GREEN は `cargo test --locked --offline -p kemi --bin kemi ps::`。
Left to the implementer: 待ちを差し替える形（関数を受け取る、内側の関数を分ける、など）。ロックを持たせる方法（`SessionStore::create`
で開いたまま持つ、など）。どちらでも本番の振る舞いは同じ。本体の crate には一時ディレクトリの補助も tempfile も無く、`--offline`
なので足せない。`std::env::temp_dir()` の下に一意の名前のディレクトリを作り、テストの終わりに消す。
Stop and hand back if: なし（同じプロセスの別のスレッドのロックが見分けられることは、既存のテストが確かめている）。

## Step 2 — 掃除の排他と、id でない名前のテスト

Purpose: 実装済みの 2 つの掃除の決まりに、仕様の成功条件のテストを付ける。
Specification: [R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（「掃除が見るのは…」と「掃除は、起動中でないのを確かめてから…」の項、
成功条件の `foo.payload` の項と片付けている間のロックの項）、D13。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/cleanup.rs`、`crates/kemi-core/src/session/lock.rs`、`crates/kemi-core/src/session/store/tests/cleanup.rs`。
Done when: 次の 2 つのテストが通る。
- `foo.payload` と `foo.endpoint` だけを置いたセッションの置き場所で `cleanup` を直接呼ぶと、前後でファイルの一覧（名前の集まり）が
  同じ（`OpenSession` の書き込みで起こすと `<id>.session` などが増えるので、既存の孤児のロックのテストと同じく直接呼ぶ）。
- 掃除がある id のつなぎ先と記録を片付けている間は、その id のセッションのロックをほかから 1 回で取りに行っても取れず、片付け終わると
  取れる。このテストは掃除の本番の経路（`remove_records_of_stopped_reviews` が `with_free_lock` の中で片付けるところ）を通る。
  片付けの中身だけをテストから差し替えられるようにしてよい。
Shown by: test — `store/tests/cleanup.rs` に 2 本足す（名前は実装に任せる）。RED は、それぞれ次の一時的な変更で落ちることで示し、
その変更は残さない。1 本目は `classify` の `is_valid_id` の判定を外す。2 本目は `with_free_lock` を使わずに
`is_locked` で確かめてから消す。GREEN は `cargo test --workspace --locked --offline`。
Left to the implementer: 片付けの中身を差し替える形。ロックを 1 回で取りに行く手段（`SessionLock::acquire` など。A4）。
Stop and hand back if: なし。

## Step 3 — 掃除と削除が、つなぎ先の置き場所を見る（kemi-core）

Purpose: つなぎ先と記録の置き場所がセッションの置き場所と違っても、掃除と削除がそちらを片付ける。消すのは、自分のセッションの置き場所に
`<id>.session` かロックのファイルがある id のものだけにする。
Specification: [R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（「掃除は、ロックの外れた id の…」と「つなぎ先と起動中の記録の置き場所は…」の項、
4 つの id の成功条件、反例の Windows の 2 項）、P31、R50、[R-PS-RECORD 起動中の記録](../spec/ps.md#r-ps-record-起動中の記録)。
Prerequisites: ステップ 2。
May change: `crates/kemi-core/src/session/`（`cleanup.rs`、`store.rs`、`lock.rs`、`mod.rs`）と `store/tests/`。
Done when:
- `SessionStore` に、つなぎ先と記録の置き場所を渡せる。渡さなければセッションの置き場所と同じ（今の振る舞い）。
- 掃除は、つなぎ先の置き場所の `.endpoint` と `.running` を探す（A1）。その一覧にも、id でない名前を飛ばす決まりを当てる。
  そのうち、ロックが外れていて、自分のセッションの置き場所に `<id>.session` かロックのファイルがある id のものだけを、その id の
  ロックを持ったまま消す。この条件は、置き場所が同じときにも当てる。
- この条件は、掃除の始めにセッションの置き場所を振り分けた一覧（`classify` の `sessions` と `locks`）で決め、ファイルを見直して
  決めない。記録の片付けは、今のとおり孤児のロックの片付けより前に行う（先に孤児のロックを消すと、ロックのファイルだけを残した id
  の記録が二度と消えなくなる）。
- つなぎ先の置き場所が無い・読めないときは、記録の片付けを何もしないで続け、上限と孤児の片付けは今のとおり行う。
- `OpenSession::delete` は、つなぎ先の置き場所から `<id>.endpoint` と記録を消す。
- 仕様の 4 つの id の成功条件のテストが通る。つなぎ先の置き場所をセッションの置き場所と別の一時ディレクトリにし、`<id>.session` を
  残した id と、ロックのファイルだけを残した id のつなぎ先と記録は消える。ロックが取られている id と、セッションの置き場所に何も
  無い id のものは残る。掃除の前後で、何も無い id のロックのファイルがセッションの置き場所にできない。掃除は、つなぎ先の置き場所を
  渡した `SessionStore` から `create` で開いた `OpenSession` の状態の書き込み（`save_state_at` など）で起こし、`cleanup_at` を
  直接呼ばない（store から掃除まで置き場所が届くことを確かめるため）。
- 同じテストか別のテストで、つなぎ先の置き場所を渡した store の `OpenSession` を開いたまま、その id の `.endpoint` と記録を
  つなぎ先の置き場所に置いてから `delete` すると、両方が消える。
- `cleanup_removes_the_endpoints_and_running_records_whose_lock_is_released` を A3 のとおりに直し、さらにセッションの置き場所に何も
  無い id のつなぎ先を 1 つ足して、それが残ることを確かめる（置き場所が同じときの条件を見るため）。
  `deleting_a_session_removes_its_endpoint_and_running_record` も通る。
Shown by: test — `store/tests/cleanup.rs` に 4 つの id のテストを 1 本足す（名前は実装に任せる）。RED は、口を作った直後、掃除が
つなぎ先の置き場所を見ない状態で、このテストが落ちること。続けて、置き場所が同じときの条件のために A3 の直しをしてから、条件を
入れる。GREEN は `cargo test --workspace --locked --offline`。
Left to the implementer: 置き場所を渡す口の形（`SessionStore` の builder、`cleanup` の引数など）と、`OpenSession` への持たせ方。
ロックのファイルを強制終了の形で残す、テストの中の手段。
Stop and hand back if: 「自分のセッションの置き場所に `<id>.session` かロックのファイルがある」を確かめるために、ロックのファイルの
名前を kemi-core の外へ出す必要が出たとき（D13 で実装に任せた名前を契約にすることになるので）。

## Step 4 — 新しいレビューと復元に、つなぎ先の置き場所を渡す（本体）

Purpose: 本体の新しいレビューと復元で、掃除と削除がつなぎ先の置き場所を見るようにする。
Specification: [R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（「つなぎ先と起動中の記録の置き場所は…」の項）、
[R-PS-RECORD 起動中の記録](../spec/ps.md#r-ps-record-起動中の記録)（SIGKILL の成功条件）、
[R-AGENT-LINK つなぎ先と安全](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)。
Prerequisites: ステップ 3。
May change: `src/input.rs`、`src/resume.rs`、`src/results.rs`、`tests/e2e/ps.rs`、`tests/e2e.rs`。
Done when:
- `src/input.rs`（新しいレビュー）と `src/resume.rs`（復元）の `SessionStore` に、`endpoints_dir()` が決まればそれを、つなぎ先の
  置き場所として渡す。決まらなければ渡さない（今と同じ）。`src/ps.rs` と `src/agent.rs` は掃除をしないので変えない。
  `src/resume.rs` の配線は差分のレビューで確かめる（A5）。
- `killed_review_is_not_listed_and_its_record_is_cleaned_later` を Windows でも走らせる。SIGKILL の代わりに、どの OS でも強制終了に
  なる止め方（`std::process::Child::kill`。unix は SIGKILL、Windows は TerminateProcess）にし、`#[cfg(unix)]` を外す。残った記録の
  パスは、今のとおり `running_record_path`（つなぎ先の置き場所）で見る。
Shown by: test — `tests/e2e/ps.rs` の上のテスト。開発機（Linux）では配線の前から通る（A2）。GREEN は
`cargo test --workspace --locked --offline`。Windows での RED と GREEN は push の後の Windows の CI で確かめる（A2）。push は人が決める。
Left to the implementer: `endpoints_dir()` を渡す処理を、`src/results.rs` に 1 つの関数としてまとめるかどうか。
Stop and hand back if: Windows で `Child::kill` がロックのファイルや記録を残さない（Windows の CI で、kill の直後の `record.exists()` が
偽になる）と分かったとき。成功条件の前提が崩れるので、計画に戻す。

## Step 5 — 変更履歴

Purpose: 利用者に見える直しを、次のリリースの読者に向けて書く。
Specification: なし（`CHANGELOG.md` の冒頭の決まりと、ルール `ba0918-release` の「未リリースの節は前のリリースからの差分」）。
Prerequisites: ステップ 3、4。
May change: `CHANGELOG.md` の `[Unreleased]`。
Done when: `[Unreleased]` の「### 修正」に、次の 2 つが前の版（0.4.2）から上げる人に分かる言葉で書いてある。
- Windows で `XDG_STATE_HOME` を設定していると、強制終了の後のつなぎ先と起動中の記録が `%LOCALAPPDATA%` 側に残り続けていた。
- Windows で `XDG_STATE_HOME` を設定せずに動かした kemi の掃除が、`XDG_STATE_HOME` を設定して動いているレビューのつなぎ先と
  記録を消し、`kemi wait` が届かず ps にも id だけで出ることがあった。
ステップ 1 と 2 はテストだけなので書かない。
Shown by: artifact — `CHANGELOG.md`。`git diff CHANGELOG.md` で、`[Unreleased]` に「### 修正」の見出しと上の 2 項だけが
足されていること。
Left to the implementer: 文面。
Stop and hand back if: なし。

## Verification map

| 仕様の節 | ステップ |
|---|---|
| R-SESSION: 取り直し（新しいセッションを含む） | 既存のテスト（触らない） |
| R-SESSION: 掃除が見る名前 | 2 |
| R-SESSION: 掃除の排他 | 2 |
| R-SESSION: つなぎ先の置き場所と、自分の置き場所の id だけ | 3、4 |
| R-PS-RUNNING: 確かめ直し | 1 |
| R-PS-RECORD: 掃除・SIGKILL の成功条件 | 3、4 |

## Left to the implementer

各ステップの「Left to the implementer」のほか、テストの名前と、補助の関数の切り出し方。

## Stop conditions

各ステップの「Stop and hand back if」のほか、仕様の成功条件が今の作りでは成り立たないと分かったとき（ロックの効き方、Windows の
ファイルの扱いなど）。ネットワークを使う操作と、push は行わない。

## Test command

`cargo test --workspace --locked --offline`。仕上げに `cargo fmt --all --check` と
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`（`PROJECT.md` の Lint と同じ）。

## Out of scope

- 仕様に書き足したもののうち、もう実装と既存のテストがあるもの: 新しいレビューと復元のロックの取り直し。
- 仕様に書き足したもののうち、実装済みで、仕様に成功条件の無いもの（テストを足さない）: 取り直すのはほかのプロセスが持っているとき
  だけ、セッションを作れずに続いたレビューは起動中の記録を書かない、ps と `kemi wait` / `kemi reply` はロックのファイルを作らない
  （ps は e2e の「前後で置き場所が変わらない」が見ている）。
- P31（使わなくなった `XDG_STATE_HOME` の置き場所のレビューが残したものを消すこと）。
- 書き込み中の一時ファイル（`.<名前>.tmp`）が強制終了で残ったときの片付け。今の掃除も見ておらず、この仕様の改訂でも扱っていない。
- `kemi wait` / `kemi reply` が、強制終了で残ったつなぎ先の id をほかの kemi の確かめと重なって起動中と見たときの振る舞い。
