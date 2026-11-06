# kemi ps: 起動中のレビューの一覧

## Goal

`kemi ps` で起動中のレビューの id・PID・URL を引けるようにし、人は URL を見失っても開き直せ、残ったサーバーを OS の kill で止められ、
エージェントは `kemi ps --format json` で id を引き直せるようにする。あわせて `kemi --resume` がロックを 1 秒まで取り直すようにする。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。用語は `CONTEXT.md`（特に「起動中」「起動中の記録」
「セッション」「保留」「復元」）。

- [kemi 仕様: 起動中のレビューの一覧](../spec/ps.md)
  - [R-PS-RUNNING 起動中の判定と対象](../spec/ps.md#r-ps-running-起動中の判定と対象)
  - [R-PS-RECORD 起動中の記録](../spec/ps.md#r-ps-record-起動中の記録)
  - [R-PS-OUTPUT 出力](../spec/ps.md#r-ps-output-出力)（text と json の小見出しを含む）
  - [R-PS-STOP 止め方](../spec/ps.md#r-ps-stop-止め方)
  - 末尾の作らないもの（P27〜P30）・委譲（D17 D18）・却下・未決
- [kemi 仕様: セッションの保存と復元](../spec/session.md)
  - [R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（ロックの 1 秒の取り直し、ロックは OS が外す、`--live` の項の「起動中の記録も
    一緒に消す」、掃除、`--resume` の一覧の 5 列）と委譲 D13
- ps.md が引く節（一段先。読むだけで変えない）:
  - [R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)（`kemi: review <id>` の行、セッションを作れなかったレビュー、
    サブコマンドの名前の manifest は `./wait` と書く決まり）
  - [R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)（`<id>.endpoint` の置き場所・権限・寿命。記録はこれにそろえる）
  - [R-RESULT](../spec/output.md#r-result-結果ファイル)（リポジトリの識別の基準）
  - [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（`kemi: live <url>` の行）

## Approach and why

### 今の作り（調べた事実）

ロックと置き場所:
- ロックは `crates/kemi-core/src/session/lock.rs` の `SessionLock::acquire`。`<id>.lock` を開いて `File::try_lock` し、取れなければ
  `SessionError::Locked`。ファイルに中身は書かない。解放は Drop（unix は名前を外してから閉じる）。保留の経路は `process::exit` の前に
  `drop(stored_session)` でロックを外す（`src/review.rs` の `finish_review`）。submit と `fail(...)` の経路は `process::exit` で、プロセスの終了で
  OS が外す。掃除の `prune_orphan_locks`（`cleanup.rs`）も `SessionLock::acquire` を使い、掃除は状態を書くたびに走る。
- 起動中かの判定は同じファイルの `is_locked`。開いて `try_lock` し、取れたらすぐ放す（＝一瞬ロックを取る）。`SessionStore::is_running`
  （`store.rs`）と掃除（`cleanup.rs` の endpoint の片付けと孤児のロックの片付け）が使う。
- `SessionStore::create`（新しいレビュー）と `SessionStore::open`（`--resume`）がロックを取る。`open` は `kemi-server` の
  `StoredSession::resume` から、`src/resume.rs` の `run_resume` が呼ぶ。
- 置き場所は `crates/kemi-core/src/result.rs` の `sessions_dir` と `endpoint_dir`、`src/results.rs` の同名の包み（`sessions_dir()`・
  `endpoints_dir()`）。unix では同じディレクトリ。Windows では `XDG_STATE_HOME` が絶対パスならセッションとロックはその下、endpoint は常に
  `%LOCALAPPDATA%\kemi\sessions\`。e2e の `kemi_command`（`tests/e2e.rs`）は常に `XDG_STATE_HOME` を設定するので、Windows の CI は
  2 つが分かれる形で走る（`tests/e2e.rs` の `endpoint_dir` がそれを写している）。

endpoint（記録が手本にするもの）:
- `crates/kemi-core/src/session/endpoint.rs` に `write_endpoint`（`storage_io::write_atomic` で原子的に、unix は 0600）・`read_endpoint`・
  `remove_endpoint`。`OpenSession::delete`（`store.rs`）はセッションの置き場所から `<id>.endpoint` も消す。掃除（`cleanup.rs` の
  `cleanup_at`）は `.endpoint` で終わる名前を集め、`is_locked` が偽なら消す。どちらもセッションの置き場所しか見ない（ps.md の「未決」の問題）。
- 起動の配線は `src/review.rs`。`prepare_review` が待ち受け・URL・`start_agent_channel`（endpoint を書く。`--explanation` のときは書くのを
  後回しにする `defer`）・`start_live`（中継の URL）をそろえ、`Announcement` を返す。`run_review` は、`--explanation` が無ければすぐ
  `announce()`（`kemi: <url>` → 警告 → 結果の保存先 → `kemi: review <id>` → `kemi: live <url>` → ブラウザ）、あれば `control.ready()` を
  待って endpoint を書いてから `announce()`。終わるときは `remove_endpoint()` を呼んでから `finish_review`（submit・実行時エラー・保留）。
  `fail(...)` で終わる経路（`report::fail`）は endpoint を消さない。URL を出した後に通る `fail` は、`run_review` の
  `the server stopped unexpectedly`（serve のタスクが異常に止まったとき）。`--explanation` の経路の `initial explanation could not start the review` は
  `control.ready()` の前で、記録を書く前に当たる。
- レビューの id は `StoredSession` の `review_id()`。セッションの無いレビュー（`src/input.rs` の `open_session` が `None`）は endpoint も
  `kemi: review` の行も無い。セッションがあっても、エージェントの待ち受けが立たない・`endpoints_dir()` が決まらないときは `start_agent_channel` が
  `None` を返し、endpoint と `kemi: review` の行が無い（`src/review.rs`）。

値の出どころ:
- `SessionInfo`（`crates/kemi-core/src/session/mod.rs`）に `workspace`（R-RESULT の基準のパス）・`workspace_key`・`mode: SessionMode`・`title`。
  `SessionMode::label(title)` が `--resume` の一覧のモードの列の文字（`src/session.rs` の `list_line`）。manifest のときは `title` を使う。
- 新しいレビューの `title` は、`kemi-server` の `serve.rs` が起動後の最初の読み込みで `describe_review` するまで空。つまり
  `kemi: <url>` を出す時点では、manifest のモードの文字がまだ無い。manifest のソース（`crates/kemi-core/src/source/manifest.rs`）の `review()` は
  `.gitattributes` を読み、内部の状態も更新するので、起動の前に余分に呼ばない。題そのものは、`ManifestSource::from_path` が読んだ manifest の
  `title`（無ければ既定の `"Review of changes"`）で、組み立てなくても決まる。
- ページの URL は `prepare_review` の `url`（`Announcement.url`、stderr の `kemi: <url>` と同じ）。中継の URL は `start_live` の戻り値。
- 時刻の整形は `src/local_time.rs`（新しい依存を足さない方針）。`utc_parts` は分までしか返さず、`civil_from_days` がある。RFC 3339 を
  書く・読む関数は無い。製品の版は `env!("CARGO_PKG_VERSION")`。

CLI:
- `src/main.rs` の `main` が、最初の引数が `wait`・`reply`・`explain` なら `agent::parse` → `agent::run` に渡す。使い方の誤りは
  `warn(...)` と `agent::USAGE` を stderr に出して `report::ERROR`（2）。`main.rs` の `USAGE` にはサブコマンドの一覧の行があり、
  「その名前の manifest は `kemi ./explain` と書く」の行は `agent.rs` の `USAGE` にだけある（`main.rs` ではコードのコメント）。
- stdout への書き込みは `report::write_stdout`、警告は `report::warn`（`kemi: ` を付けて stderr）。

テスト:
- e2e は `tests/e2e.rs`（補助）と `tests/e2e/*.rs`（場面ごと。`tests/e2e.rs` の末尾で `#[path]` の `mod` として束ねる）。`Kemi::spawn_with_state`・
  `review_id()`・`signal(child, "-TERM")`（引数をそのまま `kill` に渡す。`#[cfg(unix)]`）・`craft_session`・`sessions_dir(state)`・
  `endpoint_dir(state)` などがある。保留を作る既存の e2e はシグナルを使うので、すべて `#[cfg(unix)]`。Windows の CI は `cargo test --workspace --locked`（`.github/workflows/ci.yml` の `rust-windows`）。

### やり方（計画で決める）

- **記録は `kemi-core` の session モジュールに、endpoint と同じ形の関数で置く**（書く・読む・消す）。置き場所は endpoint と同じディレクトリ
  （呼ぶ側が `endpoints_dir()` を渡す）。中身と名前は D17 で実装に任せる（JSON で `<id>.running` などでよい）。記録の型は、PID・ページの URL・
  中継の URL（任意）・起動した時刻・製品の版・モードの種類・モードの文字・ワークスペースのパスと識別を持つ。エージェントのトークンを持つ
  欄を作らない。
- **ps の組み立ては純粋な関数にする。** 入力は「ロックの取られている id の一覧」と「id ごとの記録の読み取り結果（読めた / 無い / 壊れている）」と
  「今の場所の識別」と `--any`。出力は並べた行と範囲の外の数と警告の一覧。範囲・並び・`null` の扱い・記録の無いものを範囲を問わず出して数えない、
  はここで決まり、ユニットテストで確かめる。ファイルシステムに触るのは、ロックの一覧と記録を読む薄い層だけ。並びは記録の起動時刻（ミリ秒）の
  新しい順、時刻が同じか無いもの同士は id の文字列の昇順。
- **起動中の一覧は、セッションの置き場所の `<id>.lock` を列挙して `is_locked` で確かめる。** `<id>.session` の有無は見ない（起動したばかりの
  レビューには無い）。id として正しくない名前は飛ばす（`checked_id` / `is_valid_id`）。
- **記録を書く条件は、セッションがあること（`stored_session` の `review_id()` がある）。** endpoint の有無には結び付けない（エージェントの待ち受けが
  立たないレビューも ps に出す、と仕様が決めている）。
- **記録を書くのは `announce()` の直前**（`--explanation` のときは `control.ready()` の後、endpoint を書くのと同じ所）。消すのは `remove_endpoint()` と
  同じ所と、URL を出した後の `fail("the server stopped unexpectedly …")` の直前。どれも、その後の `drop(stored_session)` か `process::exit` より前なので、
  「ロックを外す前に消す」を満たす。
- **復元では、`StoredSession::resume` が成功した直後（ロックを取った後）に古い記録を消し**、そのあと新しいレビューと同じ所で書く。
- **ロックの取り直しは `lock.rs` に、`SessionLock::acquire` とは別の入口として置く**: 取れなければ短い間隔で取り直し、1 秒を過ぎたら `Locked`。
  使うのは `SessionStore::create` と `SessionStore::open` だけ。掃除の `prune_orphan_locks` が使う `acquire` は待たないまま（取り直すと、
  ページを開いていない起動中のレビューがある間、状態の保存のたびに最大 1 秒止まる）。

### 仮の答え（仕様が決めていないところ）

それぞれ、覆す条件つき。承認のときに判断点として見せる。

1. **manifest のモードの文字は、起動の時点で manifest の `title`（無ければ `"Review of changes"`）から作る。** 仕様は「`--resume` の一覧のモードの
   列と同じ文字」を求めるが、その題がセッションに入るのは記録を書いた後。`ManifestSource` に組み立てずに題を返す口を足して使い、`review()` は
   呼ばない（ディスクを読み、内部の状態を変えるため）。復元では `SessionInfo.title` を使う。覆す条件: 題の決め方が `review()` と食い違う場面が
   見つかる、または `label` を後から書き直す方がよいと人が判断する。
2. **新しいレビューのロック（`SessionStore::create`）にも同じ 1 秒の取り直しを入れる。** `<id>.lock` を作ってから `try_lock` するまでの間に
   ps や掃除がそのファイルを開いて一瞬ロックを取ると、新しいレビューはセッション無しで続いてしまう。仕様は `--resume` についてだけ書くが、
   `create` の取り直しは仕様が約束するふるまいと食い違わない。変わるのは、その競合で黙ってセッションを失うことが起きにくくなることと、
   競合したときに起動が最大 1 秒待つことだけ。覆す条件: 仕様を厳密に `--resume` だけに
   限りたい、と人が言う。
3. **Windows で `XDG_STATE_HOME` があるときの記録の掃除とセッションの削除は、endpoint と同じ作りにとどめる**（セッションの置き場所しか見ないので、
   その場合は消えない）。ps.md の「未決」のとおり、直すかは別の作業。このため「SIGKILL で残った記録が、別のレビューがセッションを書くと消える」
   e2e は `#[cfg(unix)]` にする（SIGKILL を送る場面でもあるので、どのみち unix だけ）。覆す条件: 人がこの計画の中で直すと決める。
4. **起動した時刻は、記録を書く時点の UNIX エポックからのミリ秒で記録に持ち、並べ替えもミリ秒で行う。** JSON の `started_at` はミリ秒までの
   RFC 3339 の UTC（`2026-11-07T01:12:30.123Z`）、text は `--resume` の一覧と同じ `local_time::format_local`。整形は新しい依存を足さず、
   `local_time.rs` に UTC の秒とミリ秒まで出す関数を足す。秒までにすると、同じ秒に起動した 2 つのレビューの並びと、同じ秒の復元での
   `started_at` の変化が見分けられない。覆す条件: JSON の時刻を秒までにそろえたい、と人が言う（並べ替えはミリ秒のまま）。
5. **範囲の判定は、ps を打った場所の `LaunchPlace::at(<カレントディレクトリ>).workspace()` から `kemi_core::result::workspace_key` で求めた
   識別と、記録の識別が等しいか。** パスどうしでは比べない（`--result` と同じ基準で、正規化の違いに左右されない）。
6. **壊れた記録の警告は英語の 1 行**（`kemi: ` 付き、文面は約束しない）。記録の置き場所（endpoint の置き場所）が決められないときは、どの記録も
   「無い」として扱い、警告しない（ps.md の「`%LOCALAPPDATA%` が決められないときは記録を書けない」と同じ見え方）。終了コード 2 になるのは
   セッションの置き場所が決められないときだけ。
7. **ヘルプ**: `main.rs` の `USAGE` に `kemi ps [--any] [--format text|json]` の行を足し、その行か直下に「text が既定で、`kemi wait` などと違う」と書く。
   `agent.rs` の `USAGE` には ps の行と、「その名前の manifest は…」の行に `ps` を足し、ps の使い方の誤りもこの `USAGE` を出す。`main.rs` の
   `USAGE` には `./ps` の注記を足さない（今も `./explain` の注記が無い）。`kemi ps` を `agent::parse` に入れるか別の parse にするかは実装に任せる。
8. **スキル**: `skills/kemi/SKILL.md` の本文には「`kemi: review <id>` を見失ったら `kemi ps --format json` で引き直す」を 1〜2 文だけ足し、
   キーの説明が要るなら `references/` のどれか（`conversation.md` か `resume.md`）に置く。`scripts/check-skill-format.sh` と
   `scripts/check-skill-frontmatter.sh` を通す。
9. **URL を出した後の `fail(...)` でも記録を消す**（`the server stopped unexpectedly` の直前）。仕様は実行時エラーで消すと決めていて、掃除に任せると
   そのセッションのロックは外れるので読まれはしないが、トークン入りの URL が残る。endpoint の同じ欠け（この経路で消さない）は今回は直さない。
   覆す条件: 人が endpoint もこの計画で直すと決める。
10. **ロックが取られているのに記録が無い id は、100 ミリ秒ほど置いてもう一度確かめ、それでも取られているときだけ出す。** ほかのプロセス
   （掃除、`--resume` の一覧、別の ps）がロックを確かめる一瞬に当たると、保留中のセッションが起動中に見える（仕様の反例「保留中のセッションが
   ps に出る」）。保留中のセッションは記録を消してからロックを外すので記録を持たず、記録のある id は確かめ直さなくてよい。覆す条件: 確かめ直しで
   ps が目に見えて遅くなる場面がある。

## Scope of change

- `crates/kemi-core/src/session/`（`lock.rs`、`store.rs`、`cleanup.rs`、`mod.rs`、記録の新しいファイル）
- `crates/kemi-server/src/stored_session.rs`（必要なら、復元の直後に古い記録を消す口）
- `src/main.rs`、`src/agent.rs`（`USAGE`）、`src/review.rs`、`src/resume.rs`、`src/input.rs`、`src/results.rs`、`src/local_time.rs`、ps の新しいファイル（例 `src/ps.rs`）
- `crates/kemi-core/src/source/manifest.rs`（題を返す口）、`crates/kemi-core/src/domain/`（ps の純粋な組み立てをここに置くとき）
- `tests/e2e.rs`、`tests/e2e/ps.rs`（新しい場面のファイル）、必要なら `tests/e2e/sessions.rs`
- `skills/kemi/SKILL.md` と `skills/kemi/references/` のうち 1 つ、`README.md`、`README-ja.md`、`CHANGELOG.md`（`[Unreleased]`）、
  `PROJECT.md`（「Stack and layout」の `src/` の行に ps のファイルを足す）
- これ以外（`web/`、`docs/spec/`、ほかの crate）は変えない

## Step order and prerequisites

1 → 2 → 3 → 4 → 5 の順。1（記録の読み書き）がなければ 3 も 4 も書けない。2（取り直し）は独立だが、4 の e2e が ps の直後に `--resume` を打つので
先に入れる。3（ps）を 4（起動時に書く）より先にするのは、3 はテストの中で記録とロックを作って確かめられ、4 の e2e は ps を使って確かめるから。
**3〜5 は 1 つのコミットにまとめる。** CLI と JSON の契約を変えるコミットでスキルと README も直す決まり（PROJECT.md の Conventions）があり、
ps の契約は 3 で入るため。1 と 2 はそれぞれ単独でコミットしてよい。

## Step 1 — 起動中の記録の読み書きと片付け（kemi-core）

Purpose: 記録を原子的に書き・読み・消す関数と、セッションの削除と掃除での片付けを作る。
Specification: [R-PS-RECORD](../spec/ps.md#r-ps-record-起動中の記録)、[R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（掃除、
「起動中の記録もセッションを消すときに一緒に消す」）。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/`（記録の新しいファイル、`mod.rs` の公開、`store.rs` の `delete`、`cleanup.rs`）。
Done when: 記録を書いて読むと同じ値が返り、unix で 0600 になる。無い記録は「無い」、壊れた記録は「壊れている（理由つき）」として読める。
`OpenSession::delete` の後に記録が無い。掃除がロックの外れた記録を、`<id>.session` の有無に関わらず消し、ロック中のものは消さない。
Shown by: test — `crates/kemi-core` のユニットテスト:
`running_record_roundtrips_and_is_owner_only`、`missing_running_record_reads_as_absent_and_broken_one_as_unreadable`、
`deleting_a_session_removes_its_running_record`、`cleanup_removes_running_records_whose_lock_is_released`（ロック中の記録が残ることも同じテストで見る）。
コマンドは `cargo test -p kemi-core running_record` など。
Left to the implementer: ファイル名と中身の形（D17）、型と関数の名前、endpoint と共通にする補助の切り出し。
Stop and hand back if: なし。

## Step 2 — ロックの 1 秒の取り直し

Purpose: `--resume`（と新しいレビュー）のロックが、確かめる側の一瞬の保持で断られないようにする。
Specification: [R-SESSION](../spec/session.md#r-session-セッションの保存と復元)（「ロックが取れないときは 1 秒まで取り直してから断る」とその成功条件）、
D13。仮の答え 2。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/lock.rs`、`store.rs`（`open` と `create` が取り直す方を使う）。`cleanup.rs` の `acquire` の呼び出しは変えない。
Done when: 別のスレッドがロックを 200 ミリ秒だけ持っている間に `open` すると取れ、2 秒持っている間なら `Locked` で断られる。
Shown by: test — `crates/kemi-core` のユニットテスト `resume_lock_waits_out_a_brief_hold`、`resume_lock_gives_up_after_a_second`（仕様の成功条件
そのもの）。既存の e2e `resume_of_a_running_session_exits_2`（`tests/e2e/sessions.rs`）が通り続けることも確かめる。
Left to the implementer: 取り直しの間隔（1 秒の上限の中で）、取り直す関数の形。
Stop and hand back if: Windows の CI で、ロックの取り直しが期待どおりに働かない（`try_lock` の振る舞いが違う）と分かったとき。

## Step 3 — `kemi ps` のコマンド

Purpose: ロックと記録から起動中のレビューを並べ、text と JSON で出す。
Specification: [R-PS-RUNNING](../spec/ps.md#r-ps-running-起動中の判定と対象)、[R-PS-OUTPUT](../spec/ps.md#r-ps-output-出力)。仮の答え 4〜7・10。
Prerequisites: Step 1。
May change: `src/main.rs`、`src/agent.rs`（`USAGE`）、ps の新しいファイル、`src/results.rs`、`src/local_time.rs`、`crates/kemi-core/src/domain/`
（純粋な組み立てを置くとき）、`tests/e2e.rs`、`tests/e2e/ps.rs`。
Done when:
- 純粋な組み立てが、範囲・並び（起動のミリ秒の新しい順、時刻が同じものと無いもの同士は id の昇順、取れないものは最後）・記録の無いものと
  壊れたもの（id 以外 `null`、範囲を問わず出す、範囲の外の数に数えない）・範囲の外の数を仕様どおりに返す。
- 記録が無くロックが取られている id は、確かめ直してから出す（仮の答え 10）。
- `kemi ps` が text と JSON（9 キー）を出し、0 件・置き場所がまだ無い・置き場所が決められない・使い方の誤り・`kemi ./ps` の終了コードと出力が
  仕様どおり。ps の前後でセッションの置き場所のファイルの一覧と中身が変わらない。
Shown by: test —
- ユニットテスト（純粋な組み立て）: `ps_rows_put_the_newest_start_first_and_unknown_last`、`ps_rows_outside_the_workspace_are_counted_not_listed`、
  `ps_rows_without_a_record_are_listed_everywhere_and_not_counted`。
- e2e（`tests/e2e/ps.rs`）。起動中のレビューは、テストのプロセスが `SessionStore::create` でロックを持ち（ロックは `sessions_dir(state)`）、Step 1 の
  関数で記録を書いて作る（記録は `endpoint_dir(state)`。Windows では 2 つが分かれる）。範囲の中に入れる記録の識別は、git の外の一時ディレクトリで
  ps を打ち、`kemi_core::result::workspace_key(&std::fs::canonicalize(<そのディレクトリ>))` で作る（ps 側は git の外ではその場所そのものを使う）。
  `ps_json_lists_a_running_review_with_all_keys`、`ps_text_shows_the_pid_and_the_whole_url`、`ps_without_running_reviews_prints_one_line_and_an_empty_array`
  （置き場所がまだ無い場合と、`craft_session` で作った保留中のセッションだけがある場合を含む）、`ps_lists_a_review_without_a_record_and_warns_only_for_a_broken_one`、
  `ps_reports_reviews_outside_the_repository_and_any_shows_them`、`ps_does_not_change_the_sessions_dir`（ファイルの名前と大きさを比べ、`.lock` 以外は
  中身も比べる。Windows ではロック中のファイルを読めないため）、`ps_rejects_unknown_flags_and_arguments`（`--all`、`extra`、`--format xml`）、
  `ps_named_manifest_opens_with_a_path`。unix だけ: `ps_exits_2_when_the_state_place_cannot_be_determined`。
Left to the implementer: text の列の並び・幅・0 件の行と範囲の外の行の文面（D18）、ps の parse を `agent::parse` に入れるか別にするか、純粋な関数を
置く場所（`kemi-core` の domain に置くなら `scripts/check-domain-purity.sh` を通す）。
Stop and hand back if: 範囲の判定に要る識別が、`workspace_key` で比べると `--result` の選び方とずれると分かったとき。

## Step 4 — 起動・終わり・復元で記録を書いて消す

Purpose: 起動中のレビューが記録を書き、終わるときと復元で正しく消して書き直す。
Specification: [R-PS-RECORD](../spec/ps.md#r-ps-record-起動中の記録)、[R-PS-RUNNING](../spec/ps.md#r-ps-running-起動中の判定と対象)、
[R-PS-OUTPUT](../spec/ps.md#r-ps-output-出力)、[R-PS-STOP](../spec/ps.md#r-ps-stop-止め方)。仮の答え 1・3・4・9。
Prerequisites: Step 1〜3。
May change: `src/review.rs`、`src/resume.rs`、`src/input.rs`、`crates/kemi-core/src/source/manifest.rs`（題を返す口）、`crates/kemi-server/src/stored_session.rs`、
`tests/e2e.rs`、`tests/e2e/ps.rs`、`tests/e2e/sessions.rs`。
Done when:
- セッションのあるレビューは、`kemi: <url>` を出す前に記録を書く（`--explanation` の経路でも）。セッションの無いレビューは書かない。書けなければ
  警告を出して続ける。
- submit・保留・実行時エラーの経路で記録を消す（endpoint と同じ所と、仮の答え 9 の `fail` の直前）。記録を書く条件はセッションがあることで、
  endpoint が無くても書く。
- `--resume` は、ロックを取った後に古い記録を消してから、その復元の値で書く。
- ps.md の R-PS-RECORD・R-PS-RUNNING・R-PS-OUTPUT・R-PS-STOP の e2e の成功条件が通る。
Shown by: test — `tests/e2e/ps.rs`。どの OS でも走る:
`ps_url_opens_the_review_and_never_shows_the_agent_token`、`ps_text_shows_the_same_url_as_stderr`（実プロセスの `kemi: <url>` と比べる）、
`ps_lists_the_review_started_later_first`、`ps_live_review_shows_the_relay_url`、`ps_lists_the_pid_and_url_on_windows_too`（`kemi_command` がいつも
`XDG_STATE_HOME` を設定するので、Windows では置き場所が分かれる形になる）、`ps_lists_the_pid_and_url_without_xdg_state_home`
（`env_remove("XDG_STATE_HOME")` で、置き場所が 1 つの形）。unix だけ（保留と強制終了にシグナルが要る）:
`running_record_is_owner_only_and_gone_after_submit_and_suspend`、`ps_shows_a_review_until_it_is_suspended`、
`resumed_review_is_listed_with_its_new_pid_url_and_start`、`ps_returns_within_five_seconds_while_the_server_is_stopped`（SIGSTOP）、
`killed_review_is_not_listed_and_its_record_is_cleaned_later`（SIGKILL）、`sigterm_from_ps_pid_suspends_and_the_review_resumes`、
`sigkill_from_ps_pid_still_resumes`（最後の 2 つは `api/review` を取って写しができてから止める）。シグナルは `signal(child, "-TERM")` の形で送る。
Left to the implementer: 記録を組み立てる場所（`prepare_review` か `run_review`）、manifest の題の取り方の細部。
Stop and hand back if: 記録を `kemi: <url>` の前に書くと既存の起動時間の計測（`scripts/measure-startup.sh`）で目に見えて遅くなると分かったとき。

## Step 5 — ヘルプ以外の文書

Purpose: 利用者とエージェントに ps を届ける。
Specification: [R-PS-STOP](../spec/ps.md#r-ps-stop-止め方)（README の Windows の止め方、スキルの手順）、PROJECT.md の Conventions（CLI と JSON の契約を
変えるコミットでスキルと README も直す、両 README をそろえる）。仮の答え 8。
Prerequisites: Step 3・4。
May change: `skills/kemi/SKILL.md`、`skills/kemi/references/` のうち 1 つ、`README.md`、`README-ja.md`、`CHANGELOG.md`、`PROJECT.md`。
Done when: 両 README の「Resume a review or read its result」付近（README-ja は対応する節）に `kemi ps` の使い方と、Windows では `taskkill /F` の強制終了で
止まり `--resume` の条件は SIGKILL と同じことが書かれ、構成と内容がそろっている。スキルにエージェントが id を引き直す手順がある。CHANGELOG の
`[Unreleased]` に ps と `--resume` の取り直しが利用者向けの言葉で入っている。PROJECT.md の `src/` の行に ps のファイルがある。
Shown by: check — `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh` が通る。README とスキルの文面は差分のレビューで確かめる
（R-PS-STOP の成功条件のとおり）。
Left to the implementer: 文面、スキルの手順を本文と references のどちらに何行置くか（本文は 1〜2 文まで）。
Stop and hand back if: なし。

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PS-RUNNING | 3（判定・置き場所・ps が何も変えない）、4（実プロセスの保留で消える、応答しないサーバー） |
| R-PS-RECORD | 1（読み書き・権限・削除・掃除）、4（書く時と消す時、トークンを出さない、復元、Windows） |
| R-PS-OUTPUT | 3（範囲・並び・text・json・誤り・`./ps`）、4（実プロセスで URL と中継の URL、並び） |
| R-PS-STOP | 4（SIGTERM・SIGKILL のあとの復元）、5（README とスキル） |
| R-SESSION の取り直し・D13 | 2 |
| R-SESSION の「記録もセッションと一緒に消す」 | 1 |

## Left to the implementer

- 記録のファイル名・中身の形（D17）と、text の見た目（D18）。
- 型・関数・テスト用の補助の名前、モジュールの切り方。
- 上のステップごとの「Left to the implementer」。

## Stop conditions

- 実装して、仕様のどれかの成功条件がこの環境では観測できない（例: Windows でロック中の別ファイルの読み取りが断られる）と分かったとき。
- 仕様の節どうしが、実装してみて食い違うと分かったとき（例: 記録を消す順と保留の片付けの順が両立しない）。

## Test command

`PROJECT.md` の Commands のとおり。`cargo test`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo fmt --all --check`、
`scripts/check-domain-purity.sh`（Step 3 で domain に置いたとき）、Step 5 では `scripts/check-skill-format.sh` と `scripts/check-skill-frontmatter.sh`。

## Out of scope

- ps.md の「未決」（Windows で `XDG_STATE_HOME` があるときの endpoint と記録の片付け）を直すこと。
- `kemi stop`、ps の `-a`、サーバーへの問い合わせ（ps.md の作らないもの）。
- リリース（版の更新とタグ）。スキルの直しを利用者に届けるにはリリースが要るが、別の作業。
- 既存の `--resume` の一覧の形と、セッションの形式の版。
