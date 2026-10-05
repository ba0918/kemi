# エージェントとの往復を作る

## Goal

レビューを submit で終えるまでの間に、人間が画面で書いたコメント・返信・発言を
「エージェントに渡す」でまとめて届け、エージェントが `kemi wait` で受け取り `kemi reply` で
返信と発言を書き返せるようになる。エージェントを使わない今までのレビューは何も変わらない。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [エージェントとのやりとり 仕様](../spec/agent-channel.md) — 全節。ただし `variants` と
  `applied`（案）、`chosen` の起きたことは `--live` の後の計画で作る（下の Out of scope）。
- [kemi 仕様](../spec/kemi.md) のうち、往復で変わる節:
  [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（確認ダイアログの件数、結果 JSON の契約の版 2）、
  [R-COMMENT](../spec/kemi.md#r-comment-コメントと-suggestion)（返信と解決）、
  [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)（セッション状態の中身、セッション形式の版 3）、
  [R-SERVE](../spec/kemi.md#r-serve-配信モデル)（エージェント用の API、`api/events`）、
  [R-INPUT-6](../spec/kemi.md#r-input-6-その他の入出力)（`kemi: review <id>` の行、サブコマンド）、
  [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（エージェントの書き込みはその場で出る例外）、
  [R-RESULT](../spec/kemi.md#r-result-結果ファイル)（古い結果をそのまま出す）、
  [R-DIST](../spec/kemi.md#r-dist-名称と配布)（スキルの「含むもの」）、
  [R-VERIFY](../spec/kemi.md#r-verify-検証)（性能の約束を変えない）、
  [作らないもの](../spec/kemi.md#作らないもの)（P5、P9、P13）。
- 用語は `CONTEXT.md` の「コメント」「スレッド」「返信」「発言」「渡す」「解決」「セッション」。
  「送信」「送る」を渡す操作に使わない。

## Approach and why

### 順序: データ → 契約 → サーバ → CLI → 画面

往復の中身（返信・発言・どこまで渡したか・まだ受け取られていない起きたこと）は、
セッションに保存しないと保留と復元をまたげない（[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)）。
だから最初にセッション状態の形を決め、次に外へ出る契約（結果 JSON）を固める。サーバの API は
その形の上に作り、CLI はサーバの API を呼ぶだけにする。画面は最後に作る。サーバと CLI の
振る舞いは e2e で画面の操作をサーバの API の呼び出しに代えて確かめられるので、画面より先に
完成させて固定できる。

### 今の形と、変えるところ

- `crates/kemi-core/src/domain/review.rs` の `Comment.replies` は `Vec<String>` で、
  `crates/kemi-server/src/api/comments.rs` の `CommentRequest::Reply` と `Resolve` は既に
  受理する（画面から送られないだけ）。返信は書いた人などを持つ構造体に変える。
- セッション形式の版の定数は `crates/kemi-core/src/session/encoding.rs` の `VERSION: u8 = 2`
  だが、`crates/kemi-core/src/session/store.rs` が `version != encoding::VERSION` で比べている
  （`read_meta` と `has_unreadable_version`）。定数を 3 にするだけでは版 2 のセッションが
  「読めない版」になり一覧から消える。書くのは 3、読めるのは {2, 3} として、比べる箇所を
  「読める版か」の判定に揃える。
- 版 2 の `replies` は文字列の並び。これを、書いた人を「人間」とした返信として作成順のまま
  読む（版 2 の画面からは返信を書けず、API で書けたのは画面の側、つまり人間だけ）。返信の
  `id` は読むときに作成順で振る（DA2 の範囲）。それ以外の 2 に無い項目は空として読む。
- 結果 JSON は `crates/kemi-server/src/session.rs` の `comment_json` と
  `crates/kemi-server/src/api/submit.rs` が組み立てる。契約の版を 2 にし、`replies` の形、
  `page`（この計画では常に `null`）、`messages` を足す。
- SSE は `crates/kemi-server/src/lib.rs` の `Event`（`Update` と `Unit`）を
  `crates/kemi-server/src/api.rs` の `events` が流す。返信・発言・エージェントの状態の通知を
  足す。

### エージェント用の API は別のリスナー

[R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全) は、`--bind` に関わらず
`127.0.0.1` で待つことと、ページのトークンと別のトークンを求める。ページ用のリスナー
（`--bind` に従う）とは別に、`127.0.0.1` のポート 0 でエージェント用のリスナーを立て、
同じ `AppState` を共有する別の `Router` を載せる。ページ用の `Router` にエージェント用の
経路を足さない。`Origin` と `Host` の検証の規則が違うので、混ぜると片方の検証を緩めることに
なる。

### `kemi wait` の待ち方

`kemi wait` はエージェント用の API へ長い HTTP の要求（long-poll）を出し、サーバは起きたことが
たまるか、レビューが終わるまで応答を保留する。WebSocket は使わない（CLI 側に新しい依存が要る）。

**時間切れはサーバが決める。** `--timeout` の秒数を要求に載せ、サーバが「空で時間切れ」と
応答し、CLI はそれを見て stdout 空・終了コード 3 で終わる。CLI 側で数えて接続を切る形に
すると、サーバが起きたことを応答に書いて「受け取られた」とした瞬間に CLI が切り、その
起きたこと（渡した 1 回分や submit）が失われる（[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply) の
「すべて返す」と反例「submit を見逃す」に当たる）。起きたことを外すのは、応答を書き終えた
後にする。サーバは待ちの接続が切れたことを「作業中」に戻す合図として使う
（[R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)）。

**止まり方は 3 通りで、今の経路が違う。**

- submit: `lib.rs` の `serve` は `shutdown` を受けて `axum::serve(...).with_graceful_shutdown` を
  抜ける。graceful shutdown は処理中の接続が閉じるまで待つので、待ちの処理は submit の結果を
  応答してから終わる。エージェント用のリスナーも同じ `shutdown` で止める。
- 実行時エラー（`stop_with_error` → `shutdown`）: 待ちの処理が `shutdown` を見て、理由つきの
  エラー応答を返して自分で終わること。見ないと graceful shutdown が待ち続け、プロセスが
  止まらない（今の SSE の `events` が `shutdown` を見て終わるのと同じ理由）。
- 保留（SIGINT / SIGTERM / Windows の Ctrl+C）: `shutdown` を通らない。`src/main.rs` の
  `run_review` が `tokio::select!` で `serve` の future を捨て、そのまま終了コード 130 で終わる。
  待っている `kemi wait` に理由を返すには、`run_review` がシグナルを受けたときに、終了する前に
  サーバへ「保留で終わる」を知らせて、待ちに理由つきのエラー応答を返させてから終える配線を
  足す。待ちが無ければ今と同じ速さで終わる。

### CLI の HTTP クライアント

本体のバイナリは今 HTTP クライアントの依存を持たない（`reqwest` は dev-dependency だけ）。
`kemi wait` / `kemi reply` が送るのはループバックへの平文 HTTP/1.1 の要求 1 つなので、
クレートを足さずに書いてよい。`std::net::TcpStream`（ブロッキング）でも、ルートの `tokio` に
`io-util` の feature を足して `tokio::net::TcpStream` でもよい（feature の追加は許す）。足すなら
[R-DEPS](../spec/kemi.md#r-deps-依存) の範囲で、TLS と C 依存を持ち込まないもの（下の Stop conditions）。

### 契約を変えるコミットとスキル

[R-DIST](../spec/kemi.md#r-dist-名称と配布) は「CLI と JSON の契約を変えるコミットで、スキルと
README も同時に直す」と決めている。結果 JSON の契約の版 2（Step 2）と、`kemi wait` /
`kemi reply`（Step 5）のコミットに、`skills/kemi/SKILL.md`・`skills/kemi/references/`・`README.md`
の対応する直しを含める。`CHANGELOG.md` の `[Unreleased]` も、利用者から見える変更を入れた
ステップで書く。結果 JSON の契約の版 2 は破壊的変更として書く。

### stderr の行の並び

[R-INPUT-6](../spec/kemi.md#r-input-6-その他の入出力) は `kemi: <url>` に「続けて」
`kemi: review <id>` を出すとし、[R-SERVE](../spec/kemi.md#r-serve-配信モデル) は公開の警告を
「URL の行の後に」置き、[R-RESULT](../spec/kemi.md#r-result-結果ファイル) は保存先の行を
サーブ開始時に出す。3 つの間の並びは [live.md の R-PAGE-MODE](../spec/live.md#r-page-mode-起動) が
`kemi: <url>` → 警告 → 保存先 → `kemi: review <id>` と決めているので、全モードでこの並びに
する（今の並びは `kemi: <url>` → 警告 → 保存先で、その後ろに足すだけ）。

### 画面の見た目

[agent-channel.md](../spec/agent-channel.md) は画面の配置を決めていない（ライブ改訂では、配置と
見た目は試作を重ねて決める）。この計画では今の kemi の部品と見た目に合わせて作り、受け入れは
サイクルの最後に人が確かめる。配置を変える判断は受け入れの後の試作で行う。

## Scope of change

- `crates/kemi-core/src/domain/review.rs`、`crates/kemi-core/src/session/`（Step 1、Step 5）
- `crates/kemi-server/src/`（Step 2〜4）。新しいファイルを足してよい（例: `api/agent.rs`）
- `crates/kemi-server/tests/server.rs`（Step 2〜4）
- `src/`（Step 5）。新しいファイルを足してよい（例: `src/agent.rs`）
- `tests/e2e.rs`（Step 2、Step 5）
- `web/`（Step 6）。`web/index.html` の `modulepreload` の一覧も（`PROJECT.md` の規則）
- `scripts/test-agent-channel.mjs`（Step 6、新規）
- `skills/kemi/`、`README.md`（Step 2、Step 5）、`CHANGELOG.md`（Step 2、Step 5、Step 6）
- 依存か `tokio` の feature を足すときだけ `Cargo.toml`・`crates/*/Cargo.toml`・`Cargo.lock`
- `.github/workflows/ci.yml` は変えない（新しいテストは今の `cargo test` の job で ubuntu と
  windows の両方で走る）

これ以外（`docs/spec/` を含む）は変えない。仕様と食い違いを見つけたら止める。

## Step order and prerequisites

Step 1 → 2 → 3 → 4 → 5 → 6 → 7 の順。各ステップは前のステップに依存する。Step 6 は Step 3 と
Step 4 の API を使う。

---

## Step 1 — セッション状態に返信・発言・往復の続きを持たせる

Purpose: 返信（書いた人つき）・発言・どこまで渡したか・まだ受け取られていない起きたこと・
`kemi wait` が呼ばれたかを、セッション状態として保存し復元できるようにする。
Specification: [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、
[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)、
[R-AGENT-WRITE](../spec/agent-channel.md#r-agent-write-reply-の入力と上限)。
Prerequisites: なし。
May change: `crates/kemi-core/src/domain/review.rs`、`crates/kemi-core/src/session/`、
`crates/kemi-server/src/`（`replies` の型が変わったことでコンパイルが通らない箇所を直すだけ）。
Done when: セッション形式の版 3 で状態を書いて読み戻すと新しい項目がそのまま戻り、版 2 の
`<id>.session` を読むと返信が人間の返信として、ほかの新しい項目が空として読める。版 2 の
セッションが一覧に出て、読めない版として扱われず、復元できる。
Shown by: test — `crates/kemi-core/src/session/` のユニットテストで、
「版 3 で書いた返信・発言・渡した境目・未受け取りの起きたこと・wait の有無が読み戻せる」
「版 2 の `<id>.session` の固定のバイト列が読め、文字列の返信が人間の返信になる」
「版 2 の `<id>.session` と `<id>.payload` を置くと一覧に出て、`has_unreadable_version` が
false で、復元できる」（`store.rs` のテスト）
「読み手の無い版（例: 99）の `<id>.session` は `UnsupportedVersion` になり、
`has_unreadable_version` が true」。
版 2 の固定のバイト列は、このステップを始める前の `encode_session(2, …)` で作って
テストに埋め込む。
Left to the implementer: 返信・発言・起きたことの Rust の型の名前と置き場所（`domain` か
`session` か）、状態の JSON の並べ方（D12）。
Stop and hand back if: 版 2 の `<id>.session` に、返信以外にも版 3 の読み手で意味が変わる
項目が見つかったとき。

## Step 2 — 結果 JSON を契約の版 2 にする

Purpose: submit の結果と結果ファイルを、返信の構造・`page`・`messages` を持つ契約の版 2 で
出す。
Specification: [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)、
[R-RESULT](../spec/kemi.md#r-result-結果ファイル)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 1。
May change: `crates/kemi-server/src/session.rs`、`crates/kemi-server/src/api/submit.rs`、
`crates/kemi-server/tests/server.rs`、`tests/e2e.rs`、`skills/kemi/`、`README.md`
（Submit contract の節）、`CHANGELOG.md`。
Done when: submit の JSON が `kemi: 2` で、全コメントが `page: null` と構造化された `replies`
を持ち、トップに `messages` がある。各返信は `id`・`author`・`body`・`variants: []`・
`chosen: null`・`applied: null` を必ず持つ（案の中身を作るのは後の計画だが、キーは今から
契約どおりに出す）。`kemi --result` が契約の版 1 の結果ファイルを書き換えずに出す。
スキルと README が版 2 の形を案内する。
Shown by: test — `crates/kemi-server/tests/server.rs` に「API で返信を 2 件（人間・エージェント
相当）足して submit すると `replies` に `author` の違う 2 件が作成順に入り、どちらも
`variants: []`・`chosen: null`・`applied: null` を持つ」、
`tests/e2e.rs` に「submit の stdout が `kemi: 2` で `messages` と `page` を持つ」
「契約の版 1 の結果ファイルを置いた `kemi --result` がそのバイト列を出す」。
エージェントの返信はまだ書く経路が無いので、このステップの server テストではテスト用の
セッション状態に直接置いてよい。
Left to the implementer: 返信と発言の `id` の形式（DA2）。
Stop and hand back if: 既存の e2e やサーバのテストが `kemi: 1` を前提にしていて、版 2 に直す
以外の変更（キーの意味の変更）が要るとき。

## Step 3 — ページ側の API: 渡す・返信・解決・発言と、その通知

Purpose: 画面から返信・解決・発言を書け、「エージェントに渡す」で前回からの変化を 1 回分に
まとめてためられるようにし、変化を SSE で画面に知らせる。
Specification: [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)、
[R-AGENT-FLOW](../spec/agent-channel.md#r-agent-flow-往復の流れ)、
[R-COMMENT](../spec/kemi.md#r-comment-コメントと-suggestion)、
[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 2。
May change: `crates/kemi-server/src/`、`crates/kemi-server/tests/server.rs`。
Done when: 渡すと、前回渡した後に起きたコメントの追加・編集・削除と返信と発言が 1 回分の
`handed` としてたまり、渡していない間はたまらない。編集済みが分かる。解決は人間の経路でだけ
変わる。返信と発言を書くと SSE で知らせる。ページの最初の読み込み（再読み込みと復元を含む）で、
返信・発言・`kemi wait` が呼ばれたか・エージェントの状態・未渡しの件数が得られる。
たまった起きたことと往復の続きは、変わるたびにセッション状態として保存される。
Shown by: test — `crates/kemi-server/tests/server.rs` に、観測点をテスト用の `SessionSink` に
保存された状態（まだ受け取られていない起きたこと）として:
「コメントを 2 つ書いて渡すと 1 回分に 2 つ入る」「書いただけでは何もたまらない」
「渡した後に編集して渡し直すと編集済みで入る」「削除したコメントは `id` だけで入る」
「発言を書くと SSE に通知が来る」「最初の読み込みの応答に返信・発言・wait の有無・状態・未渡しの
件数が入る」。
Left to the implementer: ページ側の API の URL と JSON の形（D7）、たまった起きたことの持ち方。
Stop and hand back if: 渡す前に書いたコメントを、渡した後に削除したときの扱いが
[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json) から一意に決まらないとき。

## Step 4 — エージェント用の API: wait と reply と状態

Purpose: `127.0.0.1` の別リスナーで、起きたことを長く待って返す経路と、返信と発言を書く経路を
用意し、エージェントの状態を管理する。
Specification: [R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)、
[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)、
[R-AGENT-WRITE](../spec/agent-channel.md#r-agent-write-reply-の入力と上限)、
[R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)、
[R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)、
[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[作らないもの](../spec/kemi.md#作らないもの)（P13）。
Prerequisites: Step 3。
May change: `crates/kemi-server/src/`、`crates/kemi-server/tests/server.rs`、`src/main.rs`
（エージェント用のリスナーを立てて `ServeParams` に渡す配線だけ）。
Done when: 待っている要求が、渡す・submit で返る。submit のときは結果を返し終えてから
サーバが止まる。2 つ目の同時の待ちは拒まれる。実行時エラーのときは待ちが理由つきのエラーで
終わり、`serve` が返る。時間切れはサーバが「空で時間切れ」と応答し、時間切れと渡すが同時に
起きても起きたことが失われない。エージェント用のリスナーは `--bind` に関わらず `127.0.0.1` に立つ。
書き込みは上限・形の誤り・存在しないコメントで 1 件も書かれない。`Origin` 付きの要求と
トークン違いは拒まれる。状態が未接続・待機中・作業中・応答なしに変わり、SSE で知らせる。
Shown by: test — `crates/kemi-server/tests/server.rs` に、次の振る舞いを 1 つずつ:
「待っている要求が渡すで返る」「起きたことがある状態で来た要求はすぐ返る」
「submit すると待っている要求が結果を受け取り、その後サーバが止まる」
「同時の 2 つ目の待ちが拒まれる」「`stop_with_error` の後に `serve` が返り、待ちが理由つきの
エラー応答で終わる」「時間切れの直前に渡しても、その 1 回分が次の待ちで必ず返る」
「上限を超える 1 件を含む書き込みが 1 件も書かない」（返信 51 件目、発言 201 件目、本文 64 KB 超、
存在しない `comment_id`）「`variants` を含む書き込みが形の誤りになる」（この計画では案を
受け付けない。`--live` のレビューが無いため）
「`Origin` を付けた要求が 403」「トークン違いが拒まれる」
「ページ用のリスナーを `0.0.0.0` にしても、エージェント用のリスナーの待ち受けアドレスが
`127.0.0.1`」「接続元を非ループバックに差し替えた要求が拒まれる」
「状態が未接続 → 待機中 → 作業中に変わる」「待ちの接続が切れると作業中になる」
「時刻を差し替えて作業中のまま 10 分たつと応答なしになる」（現在時刻に依存させない）。
Left to the implementer: エージェント用の API の URL と内部の形（DA1）、エージェント用の
トークンの作り方（今のページのトークンと同じ `getrandom` を使ってよい）、時刻の差し替えの形、
接続元の差し替えの形。
Stop and hand back if: 待ちが残ったまま `serve` が返らない（プロセスが止まらない）とき。
待ちの接続が切れたことをサーバ側で検知できない（ubuntu か windows の CI で）とき。
`127.0.0.1` の 2 つ目のリスナーが立てられない環境があるとき。

## Step 5 — CLI: review の行、`<id>.endpoint`、`kemi wait`、`kemi reply`

Purpose: エージェントが CLI だけで往復できるようにする。
Specification: [R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)、
[R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)、
[R-INPUT-6](../spec/kemi.md#r-input-6-その他の入出力)、
[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、
[R-DIST](../spec/kemi.md#r-dist-名称と配布)。
Prerequisites: Step 4。
May change: `src/`、`crates/kemi-core/src/session/`（`<id>.endpoint` をセッションの削除と掃除で
一緒に消すこと、ロックの生死を外から調べる関数）、`crates/kemi-server/src/`（保留を待ちへ知らせる
入口だけ）、`tests/e2e.rs`、`skills/kemi/`、`README.md`、`CHANGELOG.md`。依存か `tokio` の feature を
足すときだけ `Cargo.toml` と `Cargo.lock`。
Done when: セッションがあるレビューは、通常の起動でも復元でも `kemi: review <id>` を出し
（並びは Approach の「stderr の行の並び」）、`<id>.endpoint` を書き、submit・保留・実行時エラーで
消す。保留のとき、待っている `kemi wait` は理由を受け取って 2 で終わる。強制終了で残った
`<id>.endpoint` は、ロックが死んでいれば無いものとして扱う。セッションが無い
レビューはどちらもしない。`kemi wait` と `kemi reply` が仕様の終了コード（0・1・2・3）で終わる。
最初の引数が `wait` / `reply` ならサブコマンドとして扱う。スキルと README が往復の手順を案内する。
Shown by: test — `tests/e2e.rs` に:
「`--worktree` の stderr から review の id を取り出し、`kemi wait` を裏で待たせて、サーバの
API でコメントを書いて渡すと `kemi wait` がそのコメントを出して 0 で終わる」
「`kemi reply` で返信と発言を書くと stdout に `ids` が 2 つ出て、submit の結果に入る」
「承認すると待っていた `kemi wait` が結果を出して 0、変更要求なら 1」
「`kemi wait <id> --timeout 1` が stdout 空で 3」
「存在しない id の `kemi wait` と `kemi reply` が 2」
「submit の後の `kemi wait` が 2 で `--result` を案内する」（submit で終わった id と存在しない id は
CLI から見分けられないので、どちらにも「動いているレビューが無い、submit 済みなら
`kemi --result` で読める」と出す）
「`kemi wait` にほかのフラグを付けると 2」
「状態の置き場所を決められない環境で review の行が出ず、submit は今どおりできる」
「`<id>.endpoint` が unix で `0600` で、submit の後に無い」
「保留（unix で SIGINT）の後に `<id>.endpoint` が無く、待っていた `kemi wait` が 2」
「ロックの無い古い `<id>.endpoint` を置いた id の `kemi wait` が 2」
「`--bind 127.0.0.2`（ループバックの範囲の具体アドレス。linux だけで走らせる）で起動しても
`kemi wait` と `kemi reply` が働く」
「上限を超える 1 件を含む `kemi reply` と `variants` を含む `kemi reply` が 2 で、その後の submit の
結果に何も増えていない」
「`kemi wait` の出力が `kemi: 2` と `review: <id>` を持ち、`handed.comments[].comment` のキーの集合が
submit の `comments[]` の同じ `id` のキーの集合と一致する」
「渡した後、`kemi wait` を呼ぶ前に保留し、`--resume` してから `kemi wait` を呼ぶと、保留の前に渡した
1 回分が返り、復元した画面に渡すが出る状態（wait が呼ばれた）が残る」
「`kemi wait` と `kemi reply` の使い方の誤りのメッセージに日本語が無い」（今の日本語を含まない
ことの e2e の対象に足す）
「`./wait` という名前の manifest を `kemi ./wait` で開ける」。
e2e は `XDG_STATE_HOME` に加えて、Windows では `LOCALAPPDATA` も一時ディレクトリに差し替える
（`<id>.endpoint` が `XDG_STATE_HOME` に関わらず `LOCALAPPDATA` に置かれるため）。
Windows の `<id>.endpoint` の置き場所（`XDG_STATE_HOME` に関わらず
`%LOCALAPPDATA%\kemi\sessions\`）は、置き場所を決める関数のユニットテストで確かめる
（`src/result.rs` の今の置き場所のテストと同じやり方）。
Left to the implementer: `<id>.endpoint` の中の形（DA1）、HTTP クライアントの書き方（クレートを
足さない方法を優先）、`src/` の中のモジュールの分け方、スキルの言い回し（D10）。
Stop and hand back if: クレートを足さずに書けず、足そうとしたクレートが TLS か C 依存を
持ち込むとき（`cargo deny check licenses` と musl のビルドで確かめる）。保留のときに待ちへ理由を
返してから終える配線で、待ちが無いときの終了が遅くなるとき。

## Step 6 — 画面: スレッド・発言・渡す・状態

Purpose: 人間が画面で返信・解決・発言を書き、エージェントに渡し、エージェントの返信と状態を
その場で見られるようにする。
Specification: [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)、
[R-AGENT-STATE](../spec/agent-channel.md#r-agent-state-エージェントの状態)、
[R-AGENT-FLOW](../spec/agent-channel.md#r-agent-flow-往復の流れ)、
[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（確認ダイアログの件数）、
[R-LIVE](../spec/kemi.md#r-live-ライブリロード)（届いたらその場で出す、スクロール位置は変えない）、
[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)、[R-DIST](../spec/kemi.md#r-dist-名称と配布)
（UI の文言は英語）。
Prerequisites: Step 3、Step 4、Step 5。
May change: `web/`、`scripts/test-agent-channel.mjs`、`CHANGELOG.md`（サーバの API は変えない。
足りなければ止める）。
Done when: コメントの下にスレッド（返信、書いた人、解決、畳み）が出る。解決したスレッドは
自動で畳まれる。チャット欄で発言を書ける（狭い画面ではシート）。一度でも `kemi wait` が
呼ばれたレビューでだけ「エージェントに渡す」と状態の表示が出る。エージェントの返信と発言が
届くとスクロール位置を変えずに出る。そのレビューの submit の確認に未渡しの件数が出る。
エージェントを使わないレビューの画面は今と同じ。
Shown by: test — 状態の判定や未渡しの数え方など純粋な部分は `web/assets/model.js` に置き、
`node --test web` で振る舞いごとにテストする。画面は `scripts/test-agent-channel.mjs`
（今の `scripts/test-narrow-screen.mjs` と同じく、実バイナリと agent-browser を使う）で、
「`kemi wait` を呼ぶ前は渡すが無く状態が未接続」「`kemi wait` を待たせると待機中、返った後は
作業中に変わり、渡すが出る」「`kemi reply` の返信がスレッドに出て、スクロール位置が変わらない」
「`kemi reply` の発言がチャット欄に出る」「未渡しを残して submit を押すと確認に件数が出て、submit
の JSON にそのコメントが入る」「幅 390px でチャット欄がシートで開く」を確かめる。
Left to the implementer: 部品の配置と見た目（今の kemi に合わせる。受け入れは最後に人が見る）、
`web/assets/` の層への置き方（`PROJECT.md` の規則に従う）、英語の文言。
Stop and hand back if: 今の画面の構成のままでは、エージェントを使わないレビューの画面を
変えずに置き場所を作れないとき。画面に要る情報がサーバの API から得られないとき。

## Step 7 — 仕上げ: 性能と全体の確認

Purpose: 往復の機能を足しても今のモードの性能の約束が変わらないことと、全体の検査が通ることを
確かめる。
Specification: [R-VERIFY](../spec/kemi.md#r-verify-検証)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)。
Prerequisites: Step 6。
May change: 測定や検査で見つかった問題を直すための、Step 1〜6 の範囲のファイル。
Done when: 下の検査がすべて通り、計測が今の閾値の中に収まる。
Shown by: check — 次を順に実行する。
1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo test --workspace --locked`
4. `npx tsc -p web --noEmit` と `node --test web`
5. `scripts/check-domain-purity.sh`、`scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`、
   `cargo deny check licenses`
6. `cargo build --release` の後、`scripts/gen-fixture.sh` の 10,000 ファイルのフィクスチャで
   `scripts/measure-startup.sh` が 1 秒未満。範囲の標準フィクスチャで
   `scripts/measure-range.sh` の `commit` / `file` / `busy` が今の閾値の中
   （[R-VERIFY](../spec/kemi.md#r-verify-検証) の手順。`web/` にファイルを足したので、release ビルドの
   前に `touch crates/kemi-webview/src/lib.rs` で埋め込みを作り直す）。
7. `node scripts/test-agent-channel.mjs target/release/kemi` と、今のブラウザの検査
   （`test-rendered-view.mjs`・`test-narrow-screen.mjs`・`test-horizontal-scroll.mjs`）。
Left to the implementer: なし。
Stop and hand back if: 計測が閾値を超え、往復の機能を読み込まないようにしても戻らないとき。

---

## Verification map

| 仕様の節 | 示すステップ |
|---|---|
| R-AGENT-FLOW | Step 3、Step 5（e2e の往復）、Step 6（渡すが出ない画面） |
| R-AGENT-HAND | Step 1（保存）、Step 3（ためる・編集済み・削除）、Step 6（画面と確認の件数） |
| R-AGENT-CLI | Step 4（サーバ側の待ちと止まり方）、Step 5（CLI の終了コードと review の行） |
| R-AGENT-EVENTS | Step 3、Step 4、Step 5 |
| R-AGENT-WRITE | Step 4（上限と形の誤り）、Step 5（`ids`） |
| R-AGENT-STATE | Step 4（判定と通知）、Step 6（表示） |
| R-AGENT-LINK | Step 4（`Origin`・トークン・`127.0.0.1` の待ち受け・接続元）、Step 5（`<id>.endpoint` の権限と寿命・具体アドレスの `--bind`） |
| R-SUBMIT（契約の版 2、確認の件数） | Step 2、Step 6 |
| R-RESULT（古い結果をそのまま） | Step 2 |
| R-SESSION（状態の中身、セッション形式の版 3） | Step 1（符号化と版 2 の読み）、Step 3（変わるたびの保存）、Step 5（保留と復元をまたぐ往復） |
| R-LIVE（届いたらその場で出す例外） | Step 6 |
| R-VERIFY（性能を変えない） | Step 7 |
| R-DIST（スキルと README） | Step 2、Step 5 |

R-AGENT-LINK の「中継した開発中のページからエージェント用の API が呼べない」は、中継が
`--live` の計画で入るので、その計画で確かめる。

## Left to the implementer

- 委譲 DA1（エージェント用の API の URL と内部の形、`<id>.endpoint` の中の形）、DA2（返信と
  発言の `id` の形式）、D7（ページ側の内部 API の形）、D10（スキルの言い回し）、D12（セッションの
  ファイルの中の形）。
- 型・関数・モジュールの名前と分け方。
- 画面の配置と見た目（今の kemi に合わせ、受け入れはサイクルの最後）。

## Stop conditions

- 仕様の意味が足りない、または承認された内容から外れる必要が出たとき（仕様は直さず止める）。
- 依存を足す必要があり、そのクレートが TLS・C 依存・許可していないライセンスを持ち込むとき。
- 変更が今の入力モードの画面や submit の振る舞いに広がり、エージェントを使わないレビューが
  変わってしまうとき。
- やり方を変えても進まないとき。

## Test command

プロジェクトの決まり（`PROJECT.md` の Commands）のとおり。画面の検査は
`node scripts/test-agent-channel.mjs <target/release/kemi>`（agent-browser をローカルに入れて実行）。

## Out of scope

- `--live`、中継、スナップショット、ページへのコメントの場所（`page` の中身）、案
  （`variants`・`applied`・`chosen`）。[live.md](../spec/live.md) の計画で作る。この計画では
  `page` は常に `null`、`variants` を含む `kemi reply` は形の誤りになる。
- 画面の配置を試作で詰め直すこと（受け入れの後）。
- リリースとタグ。
