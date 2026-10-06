# worktree の監視を作業ツリー全体に広げる

## Goal

`--worktree` と `--live` のコードの見方で、起動時の差分に無かったファイルを変えたり足したりしても更新バッジが出て、押すと差分に現れる。
`.git` の中と git が無視するものは見張らず、巨大な作業ツリーでも起動と一覧の性能条件を守る。

## Specification

この計画は節を参照するだけで、本文を写さない。各ステップの前に、挙げた節を通しで読むこと。

- [kemi 仕様](../spec/kemi.md)
  - [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（監視対象の worktree の項、成功条件、反例。244a2c2 で改めた）
  - [R-INPUT-3](../spec/kemi.md#r-input-3-未コミットworktree)（監視の対象の一文）
  - [R-VERIFY](../spec/kemi.md#r-verify-検証)（起動と一覧の 1 秒、計測の方法）
  - [R-DEPS](../spec/kemi.md#r-deps-依存)（ファイル監視は `notify`、ライセンスの範囲）
- [動いているページのレビュー 仕様](../spec/live.md): [R-PAGE-MODE](../spec/live.md#r-page-mode-起動)（コードの見方は `--worktree` と同じ）

## Approach and why

### 今の作り（調べた事実）

- `crates/kemi-core/src/source/git.rs` の `watch_paths` は、worktree で `PlanStore::disk_paths()`（起動時の差分のファイルの新側のパス）を返す。
  `crates/kemi-server/src/watch.rs` の `start` は、そのファイルの親ディレクトリを `NonRecursive` で見張り、イベントのパスがそのファイルに
  一致したときだけ debounce して `Event::Update` を出す。起動は別スレッドで、起動の応答を待たせない。
- 2026-10-06 に確かめた: 起動時に差分の無かった追跡済みファイルを書き換えても `api/events` に何も届かず、差分のあるファイルでは
  `event: update` が届く。
- `--live <ファイル>` には別に、配れる範囲を `Recursive` で見張る `start_served`（ページの読み込み直し用）がある。これはこの計画では変えない。
- kemi は git の CLI を呼んで読み取っている（`git.rs` の `Command::new("git")`）。
- `--live` のコードの見方は `LiveSource` が `GitSource` に任せるので、worktree の監視を変えれば両方に効く。

### やり方（計画で決める）

- **無視するものの判定は git に聞く。新しいクレートは足さない。** 見張るディレクトリは、作業ツリーの根から辿り、`.git` と、git が無視する
  ディレクトリ（例: `git ls-files --others --ignored --exclude-standard --directory` の出力）の下には潜らずに集める。git と同じ規則で無視でき、
  依存も増えない（[R-DEPS](../spec/kemi.md#r-deps-依存)）。途中で作られたディレクトリは、git が無視するもの（例: `git check-ignore`）でなければ
  見張りに足す。
- **見張りは Linux でもディレクトリごとの `NonRecursive`。** `Recursive` は無視したディレクトリまで潜るので使わない。集めたディレクトリが
  10,000 個を超えるか、OS が見張りを断ったら、足した見張りを外して今の形（起動時の差分のファイルだけ）に落とし、stderr に 1 回だけ出す。
- **イベントの採否:** `.git` の中と、git が無視するパスへの変化は捨てる。それ以外の作業ツリーの中の変化（作成・変更・削除・改名）は
  debounce して `Event::Update`。差分の取り直しは今どおり人がバッジを押したときだけ。
- **登録は別スレッドで、起動の応答を待たせない**（今の `start` と同じ）。
- コミット範囲・staged・manifest の監視は変えない。

## Scope of change

- Rust: `crates/kemi-core/src/source/`（見張るディレクトリと無視の判定を git から得る口）、`crates/kemi-server/src/watch.rs`・`lib.rs`（監視の起動と
  落とす経路、stderr の知らせ）、`src/notice.rs`（知らせの行）
- テスト: `tests/e2e.rs`、各クレートのテスト
- スクリプト: `scripts/gen-fixture.sh`（無視したディレクトリを作る引数）
- 文書: `README.md`（ライブリロードの説明があれば）、`CHANGELOG.md`（Unreleased）
- 仕様、`CONTEXT.md`、`web/` は変えない。

## Step order and prerequisites

Step 1 → 2 → 3 → 4 の順。各ステップの終わりに次が通った状態でコミットする（`CARGO_BUILD_JOBS=4`、ビルドは 1 本ずつ、cargo mutants は使わない）。

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`
2. `node --test web`、`CARGO_BUILD_JOBS=4 cargo test --workspace`

---

## Step 1 — 見張るディレクトリと無視の判定

Purpose: 作業ツリーのうち見張るディレクトリの一覧と、あるパスを git が無視するかを、git から得る口を作る。
Specification: [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（worktree の項: `.git` と無視するものを見張らない、無視したディレクトリの下に潜らない）。
Prerequisites: なし。
May change: `crates/kemi-core/src/source/`（とそのテスト）。
Done when: worktree の `GitSource` から、見張るディレクトリ（作業ツリーの根を含み、`.git` と git が無視するディレクトリとその下を含まない）の
一覧と、パスが無視されるかの判定が得られる。一覧には上限（10,000）を超えたかどうかが分かる形がある（全部を数え上げずに打ち切ってよい）。
Shown by: test — RED → GREEN → REFACTOR（`crates/kemi-core` の単体テスト。`testutil.rs` の一時リポジトリで）。
- `.gitignore` に書いたディレクトリとその下、`.git` が一覧に入らず、追跡しているファイルのあるディレクトリと、無視されない未追跡の
  ディレクトリは入る。
- 上限を小さく渡すと、超えたことが分かる（上限は外から渡せる形にしてよい。既定は 10,000）。
Left to the implementer: git に聞く方法（`ls-files` の引数、`check-ignore` の使い方、まとめて聞くか）、口の名前と型、`ReviewSource` に足すか `GitSource` だけに持つか。
Stop and hand back if: git の出力だけでは無視したディレクトリを潜らずに判定できない場合（無視したディレクトリの中身を全部辿らないと分からない場合）。

## Step 2 — 作業ツリー全体の監視と、落とす経路

Purpose: worktree の監視を Step 1 の一覧で行い、無視したもの・`.git` を捨て、上限や OS の拒否では今の形に落とす。
Specification: [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（worktree の項、成功条件 1〜3、反例）、[R-INPUT-3](../spec/kemi.md#r-input-3-未コミットworktree)。
Prerequisites: Step 1。
May change: `crates/kemi-server/`、`crates/kemi-core/src/source/`、`src/`、`tests/e2e.rs`、各クレートのテスト。
Done when:
- worktree で、起動時の差分に無かった追跡済みファイルの変更と、新しいファイルの作成でバッジが出る。押すと差分に現れる。
- 無視したディレクトリの下への書き込みと `.git` の中の書き込みでは出ない。
- 監視中に作られたディレクトリ（無視されないもの）の中の変化でも出る。
- 見張るディレクトリが上限を超えると、起動時の差分のファイルだけを見張る形に落ち、stderr に 1 回出る。OS が見張りを断ったときも同じ。
- コミット範囲・staged・manifest の監視は変わらない。
Shown by: test — RED → GREEN → REFACTOR（e2e。今の更新バッジの e2e に倣い、`api/events` の `update` を見る）。
- 起動時の差分に無かった追跡済みファイルを変えると `update` が届く。新しいファイルを作っても届く。
- `.gitignore` で無視したディレクトリの下に書いても届かない。
- 見張るディレクトリが 10,000 個を超える作業ツリー（テストの中で空のディレクトリを作る）で、起動時の差分のファイルの変更で `update` が届き、
  stderr に落とした旨が 1 回出る。
OS の見張りの上限は決まった起こし方が無いので専用の検査は足さない（上限を超えたときと同じ経路を通ること）。
Left to the implementer: stderr の文言（英語、今の知らせの流儀。契約ではない）、新しいディレクトリの判定を作成のたびに git に聞くか、まとめるか。
Stop and hand back if: 落とす経路で、すでに足した見張りを外せず OS の上限を使い切ったままになる場合。

## Step 3 — 巨大な作業ツリーでの計測

Purpose: 巨大な作業ツリーで起動と一覧の性能条件が守られるかを測る。
Specification: [R-LIVE](../spec/kemi.md#r-live-ライブリロード)（成功条件 4）、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
Prerequisites: Step 2。
May change: `scripts/gen-fixture.sh`。
Done when: `gen-fixture.sh` に、git が無視するディレクトリを N 個作る引数がある（同じ引数なら同じ内容）。追跡ファイル 10 万個・無視したディレクトリ
5 万個のフィクスチャで、変更前（main の 244a2c2 の release ビルド）と変更後の release ビルドの `scripts/measure-startup.sh` の中央値が出ていて、
変更後が [R-VERIFY](../spec/kemi.md#r-verify-検証) の 1 秒を守る。
Shown by: external — 実装者が測り、両方の中央値と、フィクスチャを作った引数を報告する。合格かどうかは人が確かめる。フィクスチャは
リポジトリの外の一時ディレクトリに作り、コミットしない。
Left to the implementer: 引数の名前、無視したディレクトリの置き方（深さ、中のファイルの有無）。
Stop and hand back if: 変更後が 1 秒を超える、または変更前より目に見えて遅い（中央値で 2 割以上）場合。数字を添えて止める。

## Step 4 — 文書

Purpose: 利用者向けの文書を監視の範囲に合わせる。
Specification: [R-DIST](../spec/kemi.md#r-dist-名称と配布)、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)。
Prerequisites: Step 3。
May change: `README.md`、`CHANGELOG.md`、`PROJECT.md`（`gen-fixture.sh` の引数を Commands の表に書くなら）。
Done when: README の更新バッジの説明（あれば）と CHANGELOG の Unreleased が、作業ツリー全体を見張ること・無視したものを見張らないこと・上限で落ちることを書いている。
Shown by: check — 各ステップの終わりの検査。
Left to the implementer: 文の書き方（README は英語、ほかは日本語）。
Stop and hand back if: なし。

---

## Verification map

| 仕様の節 | 確かめるステップ |
|---|---|
| R-LIVE（`.git` と無視するものを見張らない、潜らない） | Step 1、Step 2 |
| R-LIVE 成功条件 1（差分に無かったファイル・新しいファイルでバッジ） | Step 2 |
| R-LIVE 成功条件 2（無視したディレクトリでは出ない） | Step 2 |
| R-LIVE 成功条件 3（上限で落ちて 1 回出す） | Step 2 |
| R-LIVE 成功条件 4（巨大な作業ツリーの性能） | Step 3（人が確認） |
| R-VERIFY | Step 3 |
| R-DIST | Step 4 |

## Left to the implementer

git に聞く方法と口の形（Step 1）、stderr の文言（Step 2）、フィクスチャの引数（Step 3）。

## Stop conditions

- 仕様に無い振る舞い（新しい入力、エラーの扱い、上限）を決めないと進めない場合。
- 今の更新バッジの既存のテストの assert を変える必要が出た場合。
- 計測で性能条件を守れない場合（Step 3）。
- WSL の負荷: ビルドは 1 本ずつ、`CARGO_BUILD_JOBS=4`。cargo mutants は使わない。

## Out of scope

- `--live <ファイル>` のページの読み込み直し用の見張り（`start_served`）。今は配れる範囲を `Recursive` で見張り、無視したディレクトリにも潜る。
  ページの読み込み直しの範囲は [R-PAGE-MODE](../spec/live.md#r-page-mode-起動) が「無視するファイルも配る」と決めているので、この計画では変えない。
- `--live` の画面の使い勝手（別の見直しで扱う）。
