# --live ①: 中継と見方とスナップショット（モックと重ねて透かす表示を含む）

## Goal

`kemi --live <URL|ファイル>` で、手元の開発サーバで動いているページか手元の HTML ファイルを kemi の中に
出し、同じ表示幅で比べる相手（スナップショットかモック）と並べたり重ねて透かしたりして見比べられる
ようにする。保留と復元の最小限（コードへのコメントと会話が残り、同じ URL かファイルにつなぎ直す）と、
`kemi wait` / `kemi reply` による往復もこの段階から使える。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [動いているページのレビュー 仕様](../spec/live.md)
  - 冒頭（見る対象、画面モックの参照）
  - [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)
  - [R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)
  - [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（変化の一覧と変化の数を除く）
  - [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)
  - [R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（下の「保留と復元の分け方」の範囲だけ）
  - [作らないもの](../spec/live.md#作らないもの)、[委譲](../spec/live.md#委譲)（DL1・DL3）、[却下](../spec/live.md#却下)
- [比べる相手 仕様](../spec/live-compare.md): [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)、
  [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)、[委譲](../spec/live-compare.md#委譲)（DC1・DC2）
- [kemi 仕様](../spec/kemi.md) のうち `--live` で変わる、または守る節:
  [R-INPUT](../spec/kemi.md#r-input-入力)（入力の表と題）、
  [R-SERVE](../spec/kemi.md#r-serve-配信モデル)（2 つ目のポート、localStorage、`--bind`）、
  [R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)（復元の一覧のモードの列、写しを持たないセッション）、
  [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（`--live <ファイル>` の見張りと例外）、
  [R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)（`page` は ③ まで常に `null`）、
  [R-DIST](../spec/kemi.md#r-dist-名称と配布)（スキルの「含むもの」の `--live` の行）、
  [R-DEPS](../spec/kemi.md#r-deps-依存)、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
- [エージェントとのやりとり 仕様](../spec/agent-channel.md): [R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)
  （`--live` でも同じ形。ページへのコメントは ③ まで無いので、`handed` に入るのはコードへのコメント・返信・発言だけ）。
- 見た目の参照: [docs/design/ui-mock-live.html](../design/ui-mock-live.html)（案 A）。食い違うときは仕様が正しい。
- 用語は `CONTEXT.md` の「見る対象」「比べる相手」「モック」「スナップショット」「ページのツリー」「会話パネル」。

## 4 つの計画の中の位置

`--live`（ライブ改訂の (b)）は 4 つの計画に分ける。これは ① で、残りは後の計画で作る。

1. **この計画**: 起動、中継とファイルの配信、ページの見方（並べる・重ねて透かす・ページのツリー・表示幅）、
   スナップショット（取る・出す）、比べる相手の選び方、モック、保留と復元の最小限。
2. 差分と変化の一覧（[R-PAGE-DIFF](../spec/live.md#r-page-diff-差分)、ページのツリーの変化の数）。
3. ページへのコメント（[R-PAGE-COMMENT](../spec/live.md#r-page-comment-コメントの場所)、submit の `page`）。
4. 保留と復元の残り（スナップショットと描き込みの画像の `<id>.files/`、モックの割り当ての保存、20 MB の規則）。

案（[R-PAGE-VARIANT](../spec/live.md#r-page-variant-案)）はライブ改訂の (c) で作る。

### 保留と復元の分け方

この計画で作るもの: `--live` のセッションを作る、写しを持たないまま一覧と復元に出す、コードへのコメント・
返信・発言・解決・見た・折りたたみを保存する、復元したら同じ `<URL>` につなぎ直す（`<ファイル>` は配り直す）、
つながらない・ファイルが無いときは出して待つ、復元したコードの見方は今の作業ツリーを読み監視を続ける、
コメント・返信・発言が無ければ保留しても残さない。
④ で作るもの: スナップショットの保存（`<id>.files/`）、モックの割り当ての保存、20 MB の規則。
したがって ① の段階では、保留して復元するとスナップショットとモックの割り当ては戻らない
（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元) の反例に当たるのは ④ が終わるまでの途中の状態）。

## Approach and why

### 順序

依存の選定 → 起動とセッション → 試験用の開発サーバ → 中継 → ファイルの配信 → ページの見方 →
スナップショット → 比べる相手とモック → 重ねて透かす → 文書と計測。

起動とセッションを先に作るのは、ほかのすべてが `--live` のレビューを前提にするから。中継の前に試験用の
開発サーバを作るのは、中継のすべての検査（HMR、`X-Frame-Options`、CSP の書き換え）がそれを使うから。
画面は、中継が動いてから作る。スナップショットは画面に出す先（`sandbox` の iframe）が要るので画面の後。
モックはスナップショットと同じ出し方を使うので、その後。

### 今の作りとの合わせ方（調べた事実）

- 入力モードの排他は `src/main.rs` の bool の数で判定し（`specify exactly one input mode`）、
  セッションの種類は `session_info()`、元データは `build_source()` で決まる。`--live` はここに 5 つ目として足す。
- `serve()`（`crates/kemi-server/src/lib.rs`）は `ReviewSource` を前提にし、コメントの API は
  ファイル id を前提にする。`--live` のコードの見方は worktree と同じ（[R-PAGE-MODE](../spec/live.md#r-page-mode-起動)）
  なので、git の作業ツリーの中では worktree の元データをそのまま使う。git の外ではコードの見方を出さない
  ので、ファイルを持たない元データを使う。
- 2 つ目のリスナーの前例は、エージェント用のリスナー（`bind_agent_listener()` と `AgentParams`、
  `serve()` の `try_join!`、別の `Router`）。中継のリスナーも同じ形で足す。ただし中継は `--bind` に従い、
  `--live-port` で固定できる（エージェント用は常に `127.0.0.1`）。
- セッションは今、写し（`<id>.payload`）が Ready のものだけを一覧と復元に出す。そのための関門が
  `store.list()`・`store.open()`・`persist()` の空の判定・`resume_line`・`frozen_source()`・
  `needs_copy()`/`start_freeze` の 6 か所にある。`--live` のセッションは写しを持たない
  （[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)）ので、`SessionMode` に `Live` を足し、
  6 か所それぞれで `Live` を写し無しで通す。ほかのモードの判定は変えない。掃除（`cleanup`）は中身を読まずに
  大きさと件数で決める作りなので、① では変えない。
- 空のセッションの判定（`SessionState::is_empty`）は今、見た・折りたたみ・往復の続きも数える。`--live` は
  「コメント・返信・発言が 1 つも無ければ残さない」（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)）
  なので、`Live` には別の判定が要る。
- `Live` のセッションには、つなぎ直しと配り直しのために、`<URL>` か `<ファイル>` のパスと、配れる範囲の根を必ず持たせる
  （git の外では起動ディレクトリが根になるので、復元時の作業ディレクトリからは決められない）。
- セッション形式の版は 3 のまま。版 3 はまだリリースしていない（v0.1.9 は版 2）ので、`ModeDto` に
  種類を足してよい。
- 監視は `crates/kemi-server/src/watch.rs`（`notify`、親ディレクトリを NonRecursive で見張り、500 ms の
  自前の debounce）。`--live <ファイル>` は配れる範囲のファイルを見張る必要があるので、範囲を見張る
  入口を足す（今の「ファイルのパスの一覧」の入口は変えない）。
- 本番の依存に HTTP クライアントも WebSocket も無い。中継には `hyper` / `hyper-util`（今は間接依存）を
  直接依存にするか、別のクレートを足す。WebSocket は中身を解釈しない（[R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)）
  ので、HTTP の upgrade の後はバイト列を両方向に流すだけでよく、WebSocket の実装クレートは要らない見込み
  （見込みであって確かめていない。Step 1 で確かめる）。
- 「エージェントに渡す」は `crates/kemi-server/src/api/channel.rs` の `hand_api` が受け、`Channel::hand` が
  true を返したら起きたことができる。渡すたびのスナップショットは、ここを合図にページへ取らせる。

### スナップショットとモックを出す場所

スナップショットは `sandbox`（スクリプトを止める）の iframe、モックは `sandbox="allow-scripts"` の iframe に出す。
どちらも `allow-same-origin` を持たないのでオリジンが不透明になり、そこからの要求には中継用の cookie
（`SameSite=Strict`）が付かない。中継のポートは cookie の無い要求を通さない
（[R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)）ので、モックとその参照ファイル、スナップショットが
参照するファイルは中継のポートからは配らない。レビュー画面のポートで、ページのトークン（`/s/<token>/`）の外の、
レビューごとに推測できない値を含むパスで配る（[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック) の
「トークンを含まない URL」。どの値をどう持つかは DC1）。このパスは配るだけで、kemi の API は今どおり
トークンと `Origin` の検証で受け付けるので、不透明なオリジン（`Origin: null`）からの API 呼び出しは断られる。
スナップショットとモックの割り当ては、この計画ではメモリにだけ持つ（④ で保存）。

### 仕様から読んで決めたこと

- `--live` と組めるのは [R-PAGE-MODE](../spec/live.md#r-page-mode-起動) に挙げたフラグだけ。`--base`・`--group-by`・
  `--to`・`--digest-top` などとの組み合わせは使い方の誤りとして終了コード 2。`--live-port` は `--live` か `--resume` と
  だけ組める（[R-INPUT](../spec/kemi.md#r-input-入力) の `--resume` の許可に `--live-port` がある）。
- 表示幅・透かし具合・比べる相手の選択は localStorage に入れない（[R-SERVE](../spec/kemi.md#r-serve-配信モデル) の
  用途に無い）。ページを開いている間だけ覚える。
- `--bind` の警告は `kemi: exposed on the LAN;` で始まり、開発中のページも見られることを述べる。続きの文面は仕様が
  契約にしていないので、検査は前置きだけを確かめ、文面は人が確かめる。
- グループ単位の表の `page` グループ（[R-INPUT](../spec/kemi.md#r-input-入力)）は、ページへのコメントが属する先なので ③ で作る。

## Scope of change

- Rust: `src/main.rs`、`src/` の下の補助、`crates/kemi-core/`（`session/`、`source/`、新しい純粋な判定）、
  `crates/kemi-server/`（`lib.rs`、`api*.rs`、`watch.rs`、新しい中継のモジュール）、`Cargo.toml` と `Cargo.lock`、
  `deny.toml`（変えるのは許可の追加ではなく、確認だけ。許可していないライセンスが要るなら止まる）
- テスト: `tests/e2e.rs`、`crates/kemi-server/tests/`、`crates/kemi-core` のテスト、`web/assets/model.test.js`、
  新しい `scripts/test-live.mjs` と試験用の開発サーバ（`scripts/` の下）
- ページ: `web/index.html`、`web/assets/` の下（新しいモジュールは `index.html` の modulepreload と
  `PROJECT.md` の層の節に足す）
- 文書: `CHANGELOG.md`（Unreleased）、`README.md`、`PROJECT.md`（入力モードの数を 5 つに、層の節）、
  `skills/kemi/`（[R-DIST](../spec/kemi.md#r-dist-名称と配布) の `--live` の行の範囲で）
- 仕様（`docs/spec/`）、`CONTEXT.md`、`docs/design/` は変えない。

## Step order and prerequisites

Step 1 → 2 → … → 10 の順。各ステップは前のステップが終わっていることを前提にする。

各ステップの終わりに、次がすべて通った状態でコミットする（ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`npx tsc -p web --noEmit`
2. `CARGO_BUILD_JOBS=4 cargo test --workspace`、`node --test web`
3. ブラウザの検査があるステップ: `touch crates/kemi-webview/src/lib.rs && CARGO_BUILD_JOBS=4 cargo build --release` の後、
   既存の 4 本（`test-agent-channel.mjs`・`test-narrow-screen.mjs`・`test-rendered-view.mjs`・`test-horizontal-scroll.mjs`）と、
   そのステップが足した `test-live.mjs` の検査を 1 本ずつ回す。`--live` を使わないモードを壊していないことを、既存の 4 本で確かめる。

---

## Step 1 — 中継に使うクレートを選び、記録する

Purpose: 中継（HTTP の転送、WebSocket の upgrade の素通し、応答の書き換え）に使う依存を、
ba0918-reuse の手順で決める。
Specification: [R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)、[委譲](../spec/live.md#委譲) の DL1、
[R-DEPS](../spec/kemi.md#r-deps-依存)。
Prerequisites: なし。
May change: `Cargo.toml`、`crates/kemi-server/Cargo.toml`、`Cargo.lock`。
Done when: 中継の層（HTTP の転送、upgrade の素通し、HTML への差し込み、ヘッダの書き換え）ごとに、
採るか自作かを 1 行の理由つきで決め、選んだクレートを依存に足したコミットがある。WebSocket の素通しに
WebSocket の実装クレートが要らないこと（HTTP の upgrade の後にバイト列を流すだけで足りること）を、使い捨ての
試しで確かめてある（試しはコミットに残さない）。理由はコミットメッセージの本文に書く。
足したクレートが C 依存を持たず、ライセンスが `deny.toml` の許可の中にある。
Shown by: check — `cargo deny check licenses`（cargo-deny が無ければ、CI の `cargo-deny-action` と同じ確認を
`cargo metadata` のライセンス欄で行い、その出力を報告する）、`cargo tree -e normal,build -p kemi-server | rg '\bcc\b|-sys'` が
何も出さないことを確かめる。
Left to the implementer: クレートの選択（R-DEPS と許可のライセンスを守るなら、どれでも振る舞いは同じ）。
Stop and hand back if: 許可していないライセンスか C 依存が避けられない場合。WebSocket の素通しに
WebSocket の実装クレートが要ると分かった場合（依存が増えるので報告する）。

## Step 2 — `--live` を起動でき、セッションを写し無しで保留・復元できるようにする

Purpose: `--live` のレビューを始め、コードの見方と会話を今と同じように使い、保留と復元の最小限を通す。
Specification: [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)、
[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)（「保留と復元の分け方」の ① の範囲）、
[R-INPUT](../spec/kemi.md#r-input-入力)、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、
[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)。
Prerequisites: Step 1。
May change: `src/`、`crates/kemi-core/`、`crates/kemi-server/`（中継はまだ足さない）、`tests/e2e.rs`、
`crates/kemi-core` と `crates/kemi-server` のテスト、`README.md`・`skills/kemi/`・`CHANGELOG.md`（CLI の契約を変えるコミットで
文書も直す、という `PROJECT.md` の規約のため。このステップで使えるようになった範囲だけを書く）。
Done when:
- `--live` の引数の受け付けと拒否が [R-PAGE-MODE](../spec/live.md#r-page-mode-起動) のとおり（URL の形、配れる範囲、
  `.html`/`.htm`、シンボリックリンク、組めるフラグ、ほかの入力モードとの組み合わせ）。配れる範囲の判定は純粋な関数。
- stderr の `kemi: <url>`・結果の保存先・`kemi: review <id>` が今のモードと同じ順に出る。`kemi: live <url>` は中継が
  できる Step 4 で足す（このステップでは出さない。動かない URL を出さないため）。
- 題・復元の一覧のモードの列が仕様のとおり（`<ファイル>` は配れる範囲の根からの相対パス）。
- git の作業ツリーの中ではコードの見方が worktree と同じで、外ではコードの見方の代わりに理由が出る。
- `--live` のセッションが写し無しで一覧と復元に出る。コードへのコメントと発言が保留と復元をまたいで残る。
  コメント・返信・発言が無ければ保留しても残らない。復元したコードの見方は今の作業ツリーを読み、監視を続ける。
  ほかのモードのセッションの扱いは変わらない。
- `kemi wait` / `kemi reply` が `--live` のレビューで使え、`handed` の形が変わらない。
Shown by: test — RED → GREEN → REFACTOR。
- 配れる範囲の判定（git の中・外、シンボリックリンク、`.git/`、`.htm`）の純粋な関数のテスト。
- [R-PAGE-MODE](../spec/live.md#r-page-mode-起動) の成功条件の終了コード 2 の組と、git の外の HTML を受け付けることの e2e。
- `--live` で保留して復元すると、コードへのコメントと発言が戻る e2e（`tests/e2e.rs` の今の復元の補助に倣う）。
- 復元の一覧に `live` とページの URL が出る e2e。
- `kemi wait` が `--live` のレビューで渡したコメントを返す e2e（今の agent の補助に倣う）。
- 見た・折りたたみだけを変えた `--live` のレビューを保留すると、セッションが残らない e2e。
Left to the implementer: `SessionMode::Live` の持ち方（ただし `<URL>` かファイルのパスと配れる範囲の根を必ず持つ）、
git の外の元データの作り。
Stop and hand back if: 写しの関門を `Live` で通すと、ほかのモードの既存のテストの書き換えが要る場合。

## Step 3 — 試験用の開発サーバを作る

Purpose: 中継とページの見方の検査に使う、手元で動く開発サーバを用意する。
Specification: [R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全) の成功条件（HMR、`X-Frame-Options: DENY`、
`frame-ancestors 'none'`）、[R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット) の成功条件
（shadow DOM・canvas・SVG・入力欄・`onclick`）。
Prerequisites: Step 2。
May change: `scripts/` の下（新しいファイル）、`crates/kemi-server/tests/` の下（Rust の統合テスト用の同じ役の補助）。
Done when: Node で動く試験用の開発サーバが、次を返せる。ページの HTML とそれが参照する CSS、ファイルを書き換えたら
WebSocket でページに知らせて CSS を差し替える仕組み（HMR に当たる）、`X-Frame-Options: DENY` と `frame-ancestors 'none'`
を返すページ、shadow DOM・canvas・SVG・入力欄・`onclick` を持つページ、兄弟の途中に要素を足せるページ（② で使う）。
npm の依存を足さない（Node の標準だけ）。Rust の統合テストには、同じ役の最小のサーバを `crates/kemi-server/tests/` の補助として置く。
Shown by: artifact — `scripts/` の試験用の開発サーバ（`node --check` が通る）と、Rust の補助。
これは後のステップの検査が使う道具で、それ自体の検査は足さない。
Left to the implementer: ファイル名、ページの中身、WebSocket の最小の実装の仕方。
Stop and hand back if: Node の標準だけでは WebSocket のサーバが書けない場合。

## Step 4 — 中継を作る

Purpose: 開発サーバのオリジンを別のポートで中継し、スクリプトを差し込み、安全の約束を守る。
Specification: [R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)（`--bind` と警告）。
Prerequisites: Step 3。
May change: `crates/kemi-server/`（中継のモジュール、`lib.rs`、`src/lan.rs` の警告の文）、`src/`、テスト、`scripts/test-live.mjs`（新規）、
`README.md`・`skills/kemi/`・`CHANGELOG.md`（`kemi: live` の行は契約なので、このコミットで文書も直す）。
Done when:
- 中継のリスナーが `--live-port`（既定は空き）と `--bind` に従って立ち、`kemi: live <url>` がその URL を指す。
- 中継用の cookie（`HttpOnly`・`SameSite=Strict`、トークンの URL を開いたときに入る）を持たない要求を拒み、
  開発サーバへ転送する前に cookie を取り除く。kemi の API は cookie では受け付けない。
- HTML の応答にページ用のスクリプトを差し込む。`X-Frame-Options` を外し、`frame-ancestors` をレビュー画面のオリジンだけに、
  `script-src` に差し込むスクリプトだけを足す。書き換えたら画面に出すための知らせをページに渡す。
- WebSocket は upgrade の後にバイト列をそのまま中継し、試験用の開発サーバの HMR が中継越しに効く。
- 中継したページとレビュー画面は `postMessage` だけで話す（このステップでは、つながったことと書き換えの知らせ）。
- `--bind 0.0.0.0` の警告に、開発中のページも見られることが入る。
- 復元したら、記録した `<URL>` につなぎ直し、`kemi: live <url>` を出す。開発サーバにつながらないときは、そのことをページに
  出して待ち、開発サーバが起動するとつながる。新しく起動したときも同じ。`--resume` に `--live-port` を付けられる。
Shown by: test —
- 統合テスト: cookie の無い要求が拒まれる、cookie が転送先に届かない、ヘッダの書き換え（Rust の補助のサーバで）。
- e2e: `--bind 0.0.0.0` の警告が `kemi: exposed on the LAN;` で始まる（続きの文面は人が確かめる）。試験用の開発サーバで、
  トークンの URL を開いて cookie を受け取った後に `kemi: live <url>` からページが読める。`--resume <id> --live-port <n>` が受け付けられる。
- ブラウザ自動化（`test-live.mjs`）: 中継の URL を直接開いて、HMR が中継越しに効く、`document.cookie` に中継用の cookie が出ない。
  開発サーバを止めたまま復元すると、つながらない旨が出て、開発サーバを起動するとつながる（[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)）。
  開発サーバを止めたまま新しく起動しても同じ（[R-PAGE-MODE](../spec/live.md#r-page-mode-起動)）。
  レビュー画面の中に出ることと書き換えの表示は、画面ができる Step 6 で確かめる。
Left to the implementer: 差し込むスクリプトの作り（DL3）、中継のモジュールの分け方。
Stop and hand back if: HMR の WebSocket を中身を解釈せずに中継できない場合。

## Step 5 — 手元の HTML ファイルを配り、保存で読み込み直す

Purpose: `--live <ファイル>` で、開発サーバ無しにページとして見られるようにする。
Specification: [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（`<ファイル>`、配れる範囲）、
[R-PAGE-PROXY](../spec/live.md#r-page-proxy-中継と安全)（同じポートで配る）、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 4。
May change: `crates/kemi-server/`（配信と見張り、`watch.rs` に範囲を見張る入口を足す）、`src/`、テスト、`scripts/test-live.mjs`。
Done when: 配れる範囲のファイルを中継のポートで配り、HTML には Step 4 と同じスクリプトを差し込む。範囲の外（`..`、
シンボリックリンクの実体が外、`.git/` の中）は返さない。範囲の中のファイルが保存されたら、ページに読み込み直させる
（コードの見方の更新バッジとは別）。範囲の中の別の HTML へのリンクで移れる。復元したら同じファイルを配り直し、
ファイルが無ければそのことを出して待つ。
Shown by: test —
- 統合テスト: 範囲の外のパスと `.git/` の中を返さない。
- e2e とブラウザ自動化: [R-PAGE-MODE](../spec/live.md#r-page-mode-起動) の成功条件のうちファイルのもの
  （参照する CSS の保存で読み込み直される、git の外の起動ディレクトリで読める）。範囲の中の別の HTML に移るとページのツリーに
  出ることは、ツリーができる Step 6 で確かめる。
- e2e: ファイルで始めたレビューを保留し、ファイルを消してから復元すると、読めない旨が出て待つ。
Left to the implementer: 範囲を見張る方法（`notify` の再帰か、親ディレクトリの集合か）と、読み込み直しの合図の送り方。
Stop and hand back if: 範囲を再帰で見張ると、大きな作業ツリーで起動や監視が目に見えて重くなる場合（計測して報告する）。

## Step 6 — ページの見方を作る

Purpose: ページとコードの 2 つの見方、表示幅、並べる表示、ページのツリーを画面に出す。
Specification: [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（2 つの見方、git の外）、
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（変化の一覧と変化の数を除く）、
[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、見た目は [docs/design/ui-mock-live.html](../design/ui-mock-live.html) の案 A。
Prerequisites: Step 5。
May change: `web/` の下、`crates/kemi-server/`（ページの見方が要る API）、`scripts/test-live.mjs`、`PROJECT.md`（層の節）。
Done when:
- 上部の帯に、見方の切り替え・表示幅（390・768・1280 と 320〜3840 の任意の数字。範囲外は受け付けず範囲を出す）・
  比べる相手の選択の置き場・今を記録の置き場がある（比べる相手と今を記録の中身は Step 7・8）。
- 動いているページを、選んだ表示幅で描き、画面に収まらないときは同じ倍率で縮める。比べる相手の置き場を並べて出す。
- 左の列はページの見方ではページのツリー、コードの見方ではファイルのツリー。ページのツリーには、表示中のページと、
  このステップで分かるページ（コメントは ③、スナップショットは Step 7、モックは Step 8 で増える）を出し、押すとその URL に、
  表示幅の札を押すとその幅に移る。
- 狭い画面では 1 枚ずつ切り替えて見る。ツリーは引き出し、会話パネルはシート。
- `--live` を使わないレビューでは、ページ用の機能を読み込まない（今の 4 モードの画面は変わらない）。
Shown by: test — ブラウザ自動化（`test-live.mjs`）:
- 表示幅の 319 と 3841 が受け付けられず範囲が出る、幅 390px で 1 枚ずつの表示と切り替えが出る、git の外で起動するとコードの見方の
  代わりに理由が出る。
- `X-Frame-Options: DENY` と `frame-ancestors 'none'` を返すページがレビュー画面の中に出て、書き換えの表示が出る（Step 4 から移した）。
- ファイルのページの中で範囲の別の HTML へのリンクを押すと、見る対象が移り、ページのツリーにそのページが出る（Step 5 から移した）。
- `--live` でないレビューで、ページ用のファイルが読み込まれない（読み込んだ URL を数える。[R-VERIFY](../spec/kemi.md#r-verify-検証) の成功条件）。
既存の 4 本が通ることで、今のモードの画面が変わらないことを確かめる。
Left to the implementer: モジュールの分け方（features の順の規則は守る）、帯と札の見た目の細部。
Stop and hand back if: ページ用の機能を読み込まない作りにできず、今のモードの起動が重くなる場合。

## Step 7 — スナップショットを取り、出す

Purpose: 開始時・渡すたび・手でスナップショットを取り、比べる相手として出せるようにする。
Specification: [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット)、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（スナップショットの既定の選び方と、時点を選ぶこと）、
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（記録されていない旨と取る操作）。
Prerequisites: Step 6。
May change: `web/` の下、差し込むスクリプト、`crates/kemi-server/`（スナップショットを受け取り・メモリに持ち・配る API、`hand_api` を合図にする）、
テスト、`scripts/test-live.mjs`。
Done when:
- 開始時（つながらなければ最初につながったとき）と「エージェントに渡す」のたび（表示中のページだけ）と手で、スナップショットを取る。URL と表示幅を覚える。
  スクリプトを含まず、shadow DOM・canvas の中身・入力欄の値を含む。2 MB を超えるものは取らず、取れなかったことを出し、
  比べる相手は 1 つ前のまま。
- `sandbox`（スクリプトを止める）の iframe に、動いているページと同じ表示幅で出す。
- 既定の比べる相手は、そのページと幅で 最後に渡した時点 → 開始時 → 最後に手で取ったもの。時点を選べ、幅を変えても時点は保つ。
  無ければ記録されていない旨と取る操作を出す。
- ページのツリーに、スナップショットのあるページとその表示幅が出る。
Shown by: test — ブラウザ自動化（`test-live.mjs`）: [R-PAGE-SNAPSHOT](../spec/live.md#r-page-snapshot-スナップショット) の 2 つの成功条件
（画素の比較、`onclick` が動かない）、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方) の成功条件のうちスナップショットのもの
（390 と 1280 で取って切り替える、別の URL に移ると無い旨が出る、2 つのページがツリーに並ぶ）、
[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の「渡す前は開始時が既定」。
画素の比較は、完全に一致すれば通し、一致しなければ差の画像と割合を報告して人の確認に回す（許容の値は仕様に無いので、検査が
自分で決めない）。
Left to the implementer: スナップショットの中の形（DL3）。
Stop and hand back if: 取る時間が大きなページで目に見えて長い場合（UL1 の計測の材料として報告する）。

## Step 8 — 比べる相手の選択とモック

Purpose: ページにモックを割り当て、比べる相手として選べるようにする。
Specification: [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)、[R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック)、DC1。
Prerequisites: Step 7。
May change: `web/` の下、`crates/kemi-server/`（割り当ての API、モックの配信）、テスト、`scripts/test-live.mjs`。
Done when:
- 比べる相手の選択からモックのパスを入れて割り当て・外せる。配れる範囲の外と HTML でないファイルは理由を出して断る。
- モックを割り当てたページは既定でモックと比べ、外すとスナップショットに戻る。ページのツリーに割り当てたことが出る。
- モックとその参照ファイルを、ページのトークンを含まず、レビューごとに推測できない値を含む URL で配る。範囲の外は返さない。
- モックは `sandbox="allow-scripts"` の iframe に出す。読むきっかけは仕様のとおりで、出している間は勝手に描き直さない。
- 割り当てはこの計画ではメモリにだけ持つ（④ で保存）。
Shown by: test — ブラウザ自動化（`test-live.mjs`）: [R-PAGE-MOCK](../spec/live-compare.md#r-page-mock-モック) の成功条件のうち保存に関わらない 3 つ
（CSS と画像ごと出る、範囲外と `.txt` を断る、トークンが得られず API が断る）と、[R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手) の
「モックを割り当てると変化の一覧が出ない」のうち比べる相手が切り替わる部分（変化の一覧は ② なので、② の計画で足す）。
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方) の「モックの割り当てだけがあるページがツリーに出る。表示幅の札を押すとその幅に切り替わる」
（コメントだけがあるページの部分は ③）。
統合テスト: モックの URL が範囲の外を返さない。
Left to the implementer: DC1（モックを配る URL の作り）。
Stop and hand back if: モックの参照ファイルをトークン無しで配る作りが、中継の cookie の約束とぶつかる場合。

## Step 9 — 重ねて透かす表示

Purpose: 比べる相手を見る対象に重ね、透かして見比べられるようにする。
Specification: [R-PAGE-REF](../spec/live-compare.md#r-page-ref-比べる相手)（重ねて透かす、操作は下の見る対象に届く）、DC2、
[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)（狭い画面でも使える）。
Prerequisites: Step 8。
May change: `web/` の下、差し込むスクリプト（スクロールをそろえるのに要るなら）、`scripts/test-live.mjs`。
Done when: 並べると重ねて透かすを切り替えられ、透かし具合を変えられ、スクロールがそろう。スナップショットでもモックでも使える。
重ねている間、比べる相手は操作を受けない。狭い画面でも使える。
Shown by: test — ブラウザ自動化（`test-live.mjs`）: 透かし具合を変えると見え方が変わり、見る対象をスクロールすると比べる相手も同じだけ動く。
幅 390px でも、比べる相手の選択と重ねて透かす表示に切り替えられる。
「重ねている間に要素を選ぶと見る対象の要素がコメントの場所になる」は、コメントの場所ができる ③ の計画で確かめる。
Left to the implementer: DC2（スクロールのそろえ方）、透かし具合の操作の見た目。
Stop and hand back if: なし。

## Step 10 — 文書を直し、速さを確かめる

Purpose: 利用者向けの文書を `--live` に合わせ、今のモードの速さを落としていないことを確かめる。
Specification: [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-VERIFY](../spec/kemi.md#r-verify-検証)、
[R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（`--live` を使わないモードの性能の約束）。
Prerequisites: Step 9。
May change: `CHANGELOG.md`、`README.md`、`PROJECT.md`、`skills/kemi/`。
Done when: CHANGELOG・README・スキルが、この計画で使えるようになった `--live` の範囲（対象、2 つの見方、比べる相手）を
過不足なく書いている（Step 2・4 で足した分の仕上げ）。PROJECT.md の入力モードが 5 つになっている。ページへのコメントと案は
③ と (c) なので、まだ書かない。今のモードと `--live` のコードの見方の起動が、[R-VERIFY](../spec/kemi.md#r-verify-検証) の 1 秒の基準を守り、
この計画の前の main と比べて遅くなっていない。
Shown by: check — 次の順に実行する。
1. 各ステップの終わりの検査のすべて
2. `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`
3. `scripts/gen-fixture.sh` で作ったフィクスチャに対し、`scripts/measure-startup.sh` をこの計画の前の main のバイナリとこのブランチの
   バイナリで回し、結果を並べて報告する
4. 同じフィクスチャで、試験用の開発サーバを相手に `--live` で起動し、コードの見方の `api/review` が返るまでの時間を測って報告する
   （`measure-startup.sh` は `--worktree` 固定なので、同じ測り方を `--live` で行う。スクリプトを変えるなら、`--worktree` の測り方は変えない）
Left to the implementer: フィクスチャの規模（R-VERIFY の目標の規模）。
Stop and hand back if: 今のモードか `--live` のコードの見方の起動が 1 秒を超えるか、main より遅くなった場合。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-PAGE-MODE | Step 2（引数・終了コード・セッション）、Step 4（`kemi: live` の URL）、Step 5（ファイル）、Step 6（2 つの見方・git の外） |
| R-PAGE-PROXY | Step 4、Step 5（ファイルを同じポートで） |
| R-PAGE-VIEW（変化の一覧と数を除く） | Step 6、Step 7（スナップショットとツリー）、Step 9（狭い画面の透かし） |
| R-PAGE-SNAPSHOT | Step 7 |
| R-PAGE-REF | Step 7（スナップショットの既定と時点）、Step 8（モックの既定）、Step 9（重ねて透かす） |
| R-PAGE-MOCK（保存を除く） | Step 8 |
| R-PAGE-SESSION（① の範囲） | Step 2（セッション）、Step 4（URL のつなぎ直し）、Step 5（ファイルの配り直し） |
| R-INPUT・R-SESSION（`--live` の題と一覧） | Step 2 |
| R-LIVE（ファイルの見張り） | Step 5 |
| R-SERVE（2 つ目のポート、`--bind`） | Step 4 |
| R-AGENT-EVENTS（`--live` でも同じ形） | Step 2 |
| R-DEPS | Step 1 |
| R-DIST | Step 2・4（契約を足すコミット）、Step 10（仕上げ） |
| R-VERIFY | Step 6（ページ用のファイルを読み込まない）、Step 10（起動の計測） |

## Left to the implementer

- 中継のクレートの選択（DL1。Step 1 で記録する）、差し込むスクリプトとスナップショットの中の形（DL3）、
  モックを配る URL の作り（DC1）、重ねて透かすときのスクロールのそろえ方（DC2）。
- モジュールの分け方と名前（`PROJECT.md` の層と features の順の規則を守る。新しい feature の位置は `PROJECT.md` に書く）。
- 画面の文言は今の英語の流儀に合わせる。見た目の細部は採用した画面モックの案 A に倣う。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、保存先、エラーの扱い、上限）を決めないと進めない場合。
- 許可していないライセンスか C 依存が要る場合。
- 今のモード（manifest・コミット範囲・worktree・staged）の既存のテストを、仕様を変えずに書き換える必要が出た場合。
- 今のモードの起動が目に見えて遅くなった場合。
- WSL の負荷: ビルドとブラウザの検査は 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- 差分と変化の一覧、ページのツリーの変化の数（②）。
- ページへのコメント、submit の `page`、重ねている間に選んだ要素がコメントの場所になること（③）。
- スナップショット・描き込みの画像・モックの割り当ての保存、20 MB の規則（④）。
- 案（(c)）。画像を見る対象・比べる相手にすること、エージェントのスクリーンショット（(d)）。
- `--live` 独自の性能の数字（UL1。中継を作った後に測って決める。この計画は材料を報告するだけ）。
