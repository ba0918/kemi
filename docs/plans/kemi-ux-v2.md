# 実装計画: kemi UX 改訂 v2

- 仕様: `docs/spec/kemi.md`（UX 改訂 v2 を承認済み。コミット `63bcabd` と `39a2def`）
- 見た目の参照: `docs/design/ui-mock-v2.html`（仕様と食い違う箇所は仕様が正しい）
- ブランチ名: `kemi-ux-v2`

## Goal

仕様 `docs/spec/kemi.md` の UX 改訂 v2 で追加・変更された要件を、main の kemi v1 の上に
実装する。

- 追加された節: `R-UNIT` / `R-ORIGIN` / `R-NAV` / `R-SEEN` / `R-RESULT`
- 改訂された節: `R-DIST` / `R-INPUT` / `R-INPUT-2` / `R-INPUT-5` / `R-INPUT-6` / `R-SERVE` /
  `R-VIEW` / `R-COMMENT` / `R-FOCUS` / `R-SUBMIT` / `R-DIGEST` / `R-LIVE` / `R-WS` /
  `R-VERIFY`

テストと仕様の計測条件がすべて通り、人が画面で確かめる項目の一覧が揃った状態で、サイクルの
最後の人の確認に渡す。リリース（版の更新・タグ）は含めない。

## Specification

`docs/spec/kemi.md` が唯一の規範。この計画は節 ID（`#R-UNIT` など）で仕様を参照し、仕様の
文をコピーしない。各ステップの前に、挙げた節を必ず読む。用語は `CONTEXT.md` に従う
（グループ単位 / 表示モード / 変更ブロック / 由来 / 結果ファイル / 見た）。

## Approach and why

- **危ない仮説と速さを先に確かめる。**
  - 仕様には、未検証の仮説が 1 つある。`#R-ORIGIN` の「削除した行のコミットを辿れる」。
  - 数値の目標も 2 つある。`#R-UNIT` の 1 秒と 100 ms。
  - どれも崩れたら仕様に戻る必要があるので、画面を作る前の Step 1〜5 で確かめる。
  - 今のコミットごとの単位は、範囲の標準フィクスチャで 3.6 秒かかる（2026-09-11、開発機
    10 コア、release ビルドの計測）。1 コミットあたり git を 3 回、順番に呼んでいるのが原因と
    見られる。同じ git の呼び出しを 8 本並列にすると 0.5 秒で終わった。
- **依存の向き（domain → source → server / cli → web）に沿って、下の層から作る。** 由来の
  割り当て、変更ブロック、止まる場所の計算は純粋関数にする。そうすれば、git もブラウザも
  無しにテストで固定できる。
- **2 つのグループ単位は、1 つの source が両方の計画を同時に持つ形にする。**
  - 今は計画を 1 つしか持てない（`crates/kemi-core/src/source/mod.rs` の `PlanStore`）。
    差し替えると、前の単位のファイル id が引けなくなる。サーバも、レビューの情報を 1 組
    しか持っていない（`crates/kemi-server/src/lib.rs` の `AppState.meta`）。
  - ファイル id は `(group_id, path)` ごとに振られる（`crates/kemi-core/src/source/git.rs` の
    `FileIds`）。同じ採番器を共有すれば、2 つの単位の id は衝突しない。
  - 「見た」はファイル id をキーにしている（`crates/kemi-server/src/session.rs`）。だから、
    単位ごとに別に持つという仕様の要求は、そのまま満たせる。
- **フロントは、純粋ロジックを `web/assets/model.js` に置いてテストする。**
  `web/assets/app.js` は結線と描画に徹する。見た目は、ブラウザ自動化のスクリーンショットと、
  サイクルの最後の人の確認で確かめる（`#R-VERIFY` の「表示と操作」）。
- **テストは失敗させてから通す。** 新しい振る舞いは、失敗するテストを先に書いてから実装する
  （RED → GREEN → REFACTOR）。ただし main で既に成り立っている条件（例: `--from` なしの
  `--group-by` が終了コード 2）は、テストを足す前に main で通るかを確かめる。通るなら、
  回帰テストとして足すだけにして RED を求めない。

## Scope of change

`crates/kemi-core/src/domain/`、`crates/kemi-core/src/source/`、`crates/kemi-server/`、
`src/`、`web/`、`tests/`、`scripts/`、`README.md`、`TODO.md`。

依存クレートを足すときは、`#R-DEPS` のライセンス範囲に限る。C 依存は持ち込まない。

仕様（`docs/spec/`）、用語集、モックは変えない。仕様を変える必要が出たら、止めて差し戻す。

## Step order and prerequisites

```text
1 計測 ─┬─ 2 由来（source + domain）──┐
        └─ 3 2 単位の source ──────────┴─ 4 サーバ ─ 5 速さの確認
4 ─┬─ 6 コメント編集・削除（API）───────────────────────┐
   ├─ 7 CLI と既定の変更 ─ 8 結果ファイル ──────────────┤
   └─ 9 表示の純粋ロジック ─ 10 差分の描画 ─ 11 切り替え・見た・移動 ─ 12 コメントの UI ─ 13 送信の UI と配色
全部 ─ 14 全体の検証と文書
```

- Step 2 と Step 3 は、互いに独立。Step 4 は両方の後。
- Step 5 は Step 4 の後。ここで速さが足りなければ、画面の作業に進まずに止める。
- Step 6・7・9 は、Step 4 の後なら順不同。Step 8 は Step 7 の後。
- Step 10〜13 は、この順に進める。どれも同じ `app.js` を触るので、順に積む方が衝突しない。
  - Step 12 は Step 6 の API を使う。
  - Step 13 は Step 8 の結果ファイルを使う。

## Verification map

| Step | 確かめる仕様節 |
|---|---|
| 1 | R-VERIFY, R-WS（`measure-range.sh`） |
| 2 | R-ORIGIN（計算） |
| 3 | R-UNIT（作成）, R-INPUT-2, R-INPUT-5 |
| 4 | R-UNIT（配信・失敗・再試行）, R-ORIGIN（取得）, R-SERVE, R-LIVE, R-SUBMIT（終了コードの例外）, R-INPUT-5（両単位への適用） |
| 5 | R-UNIT（1 秒・100 ms）, R-SERVE（10,000 ファイルの 1 秒） |
| 6 | R-COMMENT（編集・削除） |
| 7 | R-INPUT-2（既定）, R-INPUT-6, R-DIGEST |
| 8 | R-RESULT, R-SUBMIT（結果ファイルへの書き込み） |
| 9 | R-VIEW（1 列の並び・コメントの位置・種類の枠）, R-NAV（止まる場所・次のファイル） |
| 10 | R-VIEW（2 列の色・行番号の欄・折りたたみ行・由来の行・読み込み中のヘッダ）, R-ORIGIN（表示） |
| 11 | R-UNIT（切り替えの UI）, R-SEEN, R-NAV（UI）, R-FOCUS, R-VIEW（上部バー・グループ帯・ツリー） |
| 12 | R-VIEW（吹き出し・札・コメント一覧）, R-COMMENT（編集・削除の UI） |
| 13 | R-SUBMIT（確認ダイアログ・完了画面・ボタン）, R-RESULT（完了画面）, R-VIEW（配色）, R-DIST（UI の文言） |
| 14 | R-VERIFY（配布の行を除くコマンド）, R-WS, R-DEPS, R-DIST（README）, R-VIEW（50 万行の DOM 行数） |

`#R-VERIFY` の「配布」の行（リリースの成果物と mise での導入）は、この計画の対象外
（Out of scope）。

## Left to the implementer (plan-wide)

- 仕様の `## 委譲` の範囲。とくに次の 5 つ。
  - D2: 役割ごとの色の値
  - D6: 既存の値
  - D7: 内部 API の形と、`R-SERVE` の一覧に無いエンドポイントの追加
  - D8: コミットごとの単位を作るときの並列度
  - D9: 結果ファイルの名前の形式と、リポジトリの識別の持ち方
- モジュールの分け方、関数名、テストヘルパーの形、ログの文言。
- `ReviewSource` トレイトの形を変えるか、別のトレイトを足すか。どちらでも、仕様の振る舞い
  （両方の単位を同時に引ける、id が安定する）は同じになる。
- 画面の文言のうち、仕様が決めていないもの。仕様が決めている文言は変えない。例:
  「最終形」「コミットごと」「作れなかった」「最後の変更です」「最初の変更です」「見た」
  「閲」「一部特定できない」「特定できない」「消えたコミット」「保存できませんでした」
  「見てほしい点」。

## Stop conditions

次のどれかに当たったら、作業を止めて、何が起きたかと根拠を添えて差し戻す。

- 仕様の意味が足りない、または承認された内容から外れないと進めない。仕様に無い入力の
  種類、受け入れの境界、エラーの挙動を決める必要が出た場合も、これに当たる。
- 取り消せない操作、特権が要る操作、危ない対象への操作が必要になった。例: 利用者の本物の
  `~/.local/state/kemi/` を消す、`git push`、リポジトリの外のファイルを変える。
- 失敗が広がっている。1 か所を直すと別の場所が壊れ続け、変更の範囲が膨らんでいく。
- やり方を変えても、同じ所で進まない。

加えて、この計画に固有の条件がある。

- 削除した行のコミットを辿れない、または辿った結果が仕様の成功条件と食い違う
  （`#R-ORIGIN`）。この場合、代わりの規則を実装で決めない。例と計測を添えて、仕様の改訂に
  戻す。
- 仕様の数値条件を満たせないと、計測で分かった。基準を黙って緩めず、計測値を添えて報告する。
  - 範囲の標準フィクスチャで、コミットごとの単位の作成が 1 秒未満
  - 裏で作っている間の `api/file` が 100 ms 未満
  - 10,000 ファイルの起動が 1 秒未満
- 既存のテストが、仕様の古い振る舞いを固定している。例: 1 列表示の交互の並び、
  `--group-by` の既定 `commit`、長いコメントの数行への畳み。仕様どおりに直すと、仕様の別の
  節と矛盾しそうになったら、直す前に報告する。

## Out of scope

- 版の更新とリリース、`#R-VERIFY` の「配布」の行。`--group-by` の既定が変わるのは互換性が
  崩れる変更なので、次のリリースで扱う。
- `diff-review-viewer` スキルへの `kemi --result` の案内の追記と、旧 `~/.local/bin/diff-review`
  の削除（`TODO.md` の「納品後の環境更新」）。
- `TODO.md` に記録だけで残っている、v1 の既知の指摘（id 12 / 16 / 26 / 28 / 29 / 36 / 39 / 42、
  UI 改訂の id 4 / 5）。触る箇所で自然に直るものは直してよい。ただし、そのために範囲を
  広げない。
- 仕様の `## 作らないもの`（P1〜P11）。

## 画面の確認に使う入力

Step 10〜13 の確認（external）では、次の入力でサーブしたページを使う。どれもリポジトリの
中の手段で作れる。

- **このリポジトリ自身の範囲**: `kemi --from c01e476~1 --to f65473c --no-open`。27 コミットで、
  `web/assets/app.js` が 21 のグループに出てくる。コメント、書き換え、削除を含む。
- **範囲の標準フィクスチャ**: `scripts/gen-fixture.sh <dir> --files 100 --lines 20
  --commits 201` で作り、`kemi --from <最初のコミット> --no-open`。
- **50 万行のファイル**: `scripts/gen-fixture.sh <dir> --files 1 --lines 500000` で作り、
  `kemi --worktree --no-open`。
- **1 コミットだけの範囲**: このリポジトリで `kemi --from HEAD~1 --no-open`。
- **worktree / staged / manifest**: このリポジトリの作業ツリー、`--staged`、
  `tests/fixtures/legacy-manifest.json`。

ブラウザ自動化（agent-browser など）が使えない環境では、その確認を飛ばさずに止めて報告する。
人が代わりに確かめるかどうかは、人が決める。

---

## Step 1 — コミット範囲の計測スクリプト

Purpose: グループ単位を作る時間を測る手段を用意し、今の遅さを数値で残す。
Specification: `docs/spec/kemi.md#R-VERIFY`, `#R-WS`, `#R-UNIT`.
Prerequisites: なし。
May change: `scripts/measure-range.sh`（新規）。
Done when:

- `scripts/measure-range.sh <fixture> <kemi-bin> <commit|file|busy>` が `#R-VERIFY` の説明
  どおりに動く。
- `busy` は、`--group-by file` を明示して最終形で起動する。Step 7 で既定が最終形に変わる
  前でも、同じ単位を測れるようにするため。
- 裏での作成は Step 4 までは無い。そのため Step 1 の時点の `busy` は、裏での作成が無い
  状態の値を出す。
- 範囲の標準フィクスチャで `commit` と `file` の値を測り、ステップの報告に残す。

Shown by: check — 次を順に実行する。

1. `scripts/gen-fixture.sh <tmp> --files 100 --lines 20 --commits 201`
2. `cargo build --release`
3. `scripts/measure-range.sh <tmp> target/release/kemi commit`
4. 同じく `file` と `busy`

どれも数値 1 行を出して終了コード 0 で終わる。`commit` は 2 秒以上になる（今の遅さの再現）。

Left to the implementer: シェルの書き方、一時ファイルの扱い。
Stop and hand back if: `commit` が 2 秒未満で、今の遅さが再現しない（計測の前提が違う）。

## Step 2 — 由来の計算（source と domain）

Purpose: 最終形の各変更ブロックに、それを入れたコミットを割り当てる計算を作る。削除の
由来を辿れるかという仮説も、ここで確かめる。
Specification: `docs/spec/kemi.md#R-ORIGIN`, `#R-INPUT-2`.
Prerequisites: Step 1。
May change: `crates/kemi-core/src/domain/`、`crates/kemi-core/src/source/`。
Done when:

- git の呼び出しは source 側に置く。
  - 新側の行ごとに、その行を最後に変えた範囲内のコミットを求める。
  - 削除された旧側の行ごとに、その行を消したコミットを求める。
- 変更ブロックへの割り当ては、domain の純粋関数にする。入力は、整列済みの行と、行ごとの
  コミットの対応。
- 一時リポジトリ（`crates/kemi-core/src/source/testutil.rs` の `TempRepo`）を使うテストで、
  次の場面が固定されている。
  - 2 つのコミットが同じファイルの別の箇所を変えた。各ブロックの由来が、そのコミットになる。
  - 1 つのブロックの新側の行を、複数のコミットが変えた。範囲内のコミットがすべて、新しい
    順に出る。
  - 1 つのコミットが行を消した。削除だけのブロックの由来が、そのコミットになる。
  - 削除のうち一部の行だけが辿れる。見つかったコミットを出したうえで「一部特定できない」を
    添える。まったく辿れなければ「特定できない」だけになる。
  - 範囲より前・範囲外のコミットは、由来に出ない。範囲内のどのコミットにも当たらない行は
    「特定できない」になる。
  - マージ自体が変えた行・消した行は、マージとして出る。
  - 途中で改名されたファイルでは、移り先（そのコミット時点のパスと行番号）が正しい。
  - 片側が 10,000 行または 1 MB を超えるファイルは、既定で計算しない。

Shown by: test — `cargo test -p kemi-core` の、上の場面ごとの `origin_*` テスト。
Left to the implementer: `git blame` / `git blame --reverse` / `git log` のどれをどう組むか、
行の対応の内部表現、キャッシュの有無。
Stop and hand back if: 消したコミットを `git blame --reverse` 相当で辿れない。または、辿り方が
履歴の形（マージ、改名）によって仕様の成功条件と食い違う。仮説が崩れたので、例と計測を
添えて仕様の改訂に戻す（`#R-ORIGIN` が明記している）。

## Step 3 — 2 つのグループ単位を持つ source

Purpose: コミット範囲の source が、最終形とコミットごとの両方の計画を同時に持てるように
する。コミットごとの単位は、git の呼び出しを並列にして作る。
Specification: `docs/spec/kemi.md#R-UNIT`, `#R-INPUT-2`, `#R-INPUT-5`.
Prerequisites: Step 1。
May change:

- `crates/kemi-core/src/source/`
- `crates/kemi-core/src/domain/`（型と `focus.rs` の照合）
- `src/main.rs` と `crates/kemi-server/`（source の形が変わったときに、コンパイルを保つ
  範囲だけ）

Done when:

- 取得と id
  - 1 つの source から両方の単位を別々に取得でき、両方のファイル id で内容を引ける。
  - 片方を取得し直しても、もう片方の id が引けなくならない。
  - 同じ `(group_id, path)` の id は、取得し直しても変わらない。
- コミットごとの単位は、git の呼び出しを並列にして作る。
- `--focus` の照合
  - 起動時に、`all` と範囲内のコミット sha の両方と照合される。
  - パスは、最終形か範囲内のどれかのコミットに現れれば受け付ける。コミットごとのパスの
    一覧は、`--focus` があるときだけ求める。
  - どちらにも無いパスとグループ id は、誤りになる。
- 起動の経路（`--focus` 付きを含む）で、ファイルの内容を読まない。

Shown by: test — `cargo test -p kemi-core` の、次の `unit_*` / `focus_*` テスト。

- `unit_both_units_serve_content_*`
- `unit_refetch_keeps_other_unit_*`
- `unit_ids_stable_*`
- `focus_accepts_commit_group_id_*`
- `focus_accepts_path_removed_within_range_*`
- `focus_rejects_unknown_path_*`
- `unit_startup_reads_no_content_*`

Left to the implementer: 並列度（D8）、並列の仕組み（`std::thread` か、既存の
`worktree_numstat` と同じ分割か）、計画の持ち方。
Stop and hand back if: なし（速さは Step 5 で確かめる）。

## Step 4 — サーバ: 2 単位の配信、裏での作成、ライブリロード

Purpose: サーバが起動時の単位を返した後、もう片方を裏で作る。そのうえで、両方の単位の
ファイル・由来・コメント・見たを扱えるようにする。
Specification: `docs/spec/kemi.md#R-UNIT`, `#R-ORIGIN`, `#R-SERVE`, `#R-LIVE`, `#R-SUBMIT`,
`#R-COMMENT`, `#R-INPUT-5`.
Prerequisites: Step 2, Step 3。
May change: `crates/kemi-server/`、`crates/kemi-core/src/source/`（配信に必要な範囲）、
`src/main.rs`（配線だけ）。
Done when:

- 裏での作成
  - 起動時の `api/review` は、起動時の単位だけで返る。もう片方は、その後に裏で作られる。
  - 作成中・完成・失敗（理由付き）の状態を、ページが知る手段がある（D7）。
  - 失敗の後に作成をやり直す手段がある（D7）。
  - 作成に失敗しても、レビューは続く。
- 両方の単位
  - ファイル・コメント・見た・submit は、両方の単位のファイル id を受け付ける。
  - submit JSON には両方の単位のコメントが入る。前の単位のコメントが、単位の切り替えで
    `outdated` にならない。
  - `--focus` の focus と note は、両方の単位のファイルに付く。
- 由来の取得
  - 最終形のファイルの由来（Step 2）を、差分とは別に後から取得できる（D7）。
  - 上限を超えるファイルでも、そのファイルだけ由来を有効にして取得できる（D7）。
- ライブリロード
  - 再取得では、作ってある両方の単位を取り直す。見たとコメントは引き継ぐ。
  - 新しいコミットは、コミットごとの単位に未読のグループとして加わる。
  - 裏での作成中に再取得が起きても、作り終えた単位は再取得後の履歴と一致する。
  - 履歴の書き換えで消えたグループのコメントは、元の `group_id` と `group_title` のまま
    `outdated: true` で入る。
- 終了コードと内容の読み取り
  - 裏での作成と結果ファイルの保存（Step 8）以外の git / I/O の失敗は、今までどおり終了
    コード 2 になる。
  - 起動と `api/review` はファイルの内容を読まない。コミット範囲と、裏での作成の経路でも
    同じ。

Shown by: test — `cargo test -p kemi-server` の、`LiveServer` と一時リポジトリを使う次の
テスト。あわせて `cargo test --test e2e` の既存テストが通る。

- `unit_background_first_review_does_not_wait_*`
- `unit_failure_keeps_review_and_retries_*`
- `unit_comments_both_in_submit_*`
- `unit_seen_separate_*`
- `unit_focus_applies_to_both_*`
- `origin_api_*`
- `origin_api_large_file_opt_in_*`
- `live_refresh_both_units_adds_new_commit_unseen_*`
- `live_rewritten_history_comment_outdated_*`
- `range_startup_reads_no_content_*`

Left to the implementer: 状態の持ち方、エンドポイントの名前と JSON の形（D7）、裏の作業の
起動方法（`spawn_blocking` など）、作成中の再取得の扱い（作り直す、または待ってから取り直す）。
Stop and hand back if: 「作成に失敗する」場面を、テストで決まった結果として再現できない
（例: 作成中に `--to` の ref を消しても失敗にならない）。この場合は再現の方法を報告し、
代わりの失敗の起こし方を人と決める。

## Step 5 — 速さの確認

Purpose: 仕様の数値条件を、画面の作業に入る前に確かめる。
Specification: `docs/spec/kemi.md#R-UNIT`, `#R-SERVE`, `#R-VERIFY`.
Prerequisites: Step 4。
May change: `crates/kemi-core/src/source/`、`crates/kemi-server/`（速さのための修正だけ）。
Done when: 範囲の標準フィクスチャと 10,000 ファイルのフィクスチャで、次の値が報告に
残っている。

- `scripts/measure-range.sh … commit` が 1 秒未満
- `scripts/measure-range.sh … busy` が 100 ms 未満
- `scripts/measure-startup.sh` が 1 秒未満（10,000 ファイル）

Shown by: check — 次を順に実行する。

1. `cargo build --release`
2. `scripts/gen-fixture.sh <tmp-range> --files 100 --lines 20 --commits 201`
3. `scripts/measure-range.sh <tmp-range> target/release/kemi commit`
4. `scripts/measure-range.sh <tmp-range> target/release/kemi busy`
5. `scripts/gen-fixture.sh <tmp-10k> --files 10000 --lines 10`
6. `scripts/measure-startup.sh <tmp-10k> target/release/kemi`

Left to the implementer: 並列度（D8）の値。
Stop and hand back if: 並列度を変えても、1 秒と 100 ms を両立できない。基準を緩めず、
計測値と、どこに時間がかかっているかを添えて報告する。

## Step 6 — コメントの編集と削除（API）

Purpose: コメントの本文と suggestion の編集、削除をサーバで受け付ける。
Specification: `docs/spec/kemi.md#R-COMMENT`, `#R-SUBMIT`.
Prerequisites: Step 4。
May change: `crates/kemi-server/`、`crates/kemi-core/src/domain/`（コメントの検証）。
Done when:

- 編集で変わるのは本文と suggestion だけ。行範囲・`side`・`quote`・作成時の内容ハッシュは
  変わらない。
- 旧側とファイル全体のコメントに suggestion を付ける編集は拒否される。
- 削除したコメントは submit JSON に出ない。
- 削除の後に付けたコメントの id は、消した id と重ならない。
- 存在しない id への編集・削除は、既存と同じ誤りの形で拒否される。

Shown by: test — `cargo test -p kemi-server` の `comment_edit_*` / `comment_delete_*` テスト。
Left to the implementer: 操作の名前と JSON の形（D7）。
Stop and hand back if: 返信（`replies`）が付いたコメントの削除など、仕様が扱いを決めていない
場面が API に現れる。v1 の UI は返信を送らないので、通常は起きない。

## Step 7 — CLI: 既定の単位と新しいフラグ

Purpose: `--group-by` の既定を最終形にし、`--result` まわりのフラグと使い方の誤りを
仕様どおりにする。
Specification: `docs/spec/kemi.md#R-INPUT-2`, `#R-INPUT-6`, `#R-DIGEST`.
Prerequisites: Step 4。
May change: `src/main.rs`、`tests/e2e.rs`。
Done when:

- 既定の単位
  - `--group-by` を付けない `--from` が最終形で始まる。
  - digest も、既定では最終形（グループ 1 つ）で集計する。
  - 使い方の表示（USAGE）の既定が `file` になっている。
- 使い方の誤り: `#R-INPUT-6` の成功条件に挙がったものが、すべて終了コード 2 になる。
  - `--result` と、入力モードや `--any` / `--workspace` 以外のフラグの同時指定
  - `--result` なしの `--any` / `--workspace`
  - `--any` と `--workspace` の同時指定
  - `--from` なしの `--group-by`（main で既に成り立つので、回帰テストとして足す）
- 既定の変更で壊れる既存テストは、仕様どおりに直す。

Shown by: test — `cargo test --test e2e` の `cli_default_group_by_file_*` /
`cli_result_usage_errors_*` / `digest_default_final_*` テスト。
Left to the implementer: 引数解析の内部の書き方（手書きのまま）、誤りの文言（D1）。
Stop and hand back if: 既存の e2e が既定 `commit` を前提にしていて、直すと別の仕様節の
成功条件を落とす。

## Step 8 — 結果ファイルと `kemi --result`

Purpose: submit の結果を状態ディレクトリにも残し、後から読めるようにする。
Specification: `docs/spec/kemi.md#R-RESULT`, `#R-SUBMIT`, `#R-INPUT-6`.
Prerequisites: Step 7。
May change: `src/`（結果ファイルの読み書きのモジュールを足してよい）、`tests/e2e.rs`、
`crates/kemi-server/`（完了画面に保存先と失敗を渡す範囲）。
Done when:

- テストの隔離
  - submit するすべてのテストが、状態ディレクトリを一時ディレクトリに向けている
    （e2e では子プロセスに `XDG_STATE_HOME` を渡す）。今の e2e のヘルパーは環境変数を渡して
    いないので、渡せるようにする。
  - テストが、利用者の本物の `~/.local/state/kemi/` に書き込まない。
- `#R-RESULT` の要件と成功条件が、e2e と純粋関数のユニットテストで固定されている。
  - 結果ファイルの中身が stdout と同じで、権限が `0600` / `0700` になる。
  - `--result` が同じ JSON と元の終了コードを返し、該当が無ければ終了コード 2 で何も
    出さない。
  - 20 件の上限は、全リポジトリの合計で数える。21 件目で最も古い 1 件が消える。2 つの
    リポジトリに分けて書いても、合計で数える。
  - 別のリポジトリの結果は返さず、`--any` なら返す。`--workspace` で場所を指定できる。
  - 同じリポジトリのサブディレクトリで起動しても、同じリポジトリとして扱う。git の外では、
    起動したディレクトリで扱う。
  - `XDG_STATE_HOME` が相対パスなら、既定の場所を使う。
  - 置き場所に書けなくても、stdout と終了コードは変わらない。stderr に警告が出る。
  - 中断（130）とエラー（2）では書かない。
  - 保存先を、サーブ開始時に、stderr の `kemi: <url>` とは別の行で出す。
  - 「最新」は送信時刻（ミリ秒）で決まる。同じ時刻でも名前が衝突しない。

Shown by: test — `cargo test` の `result_*` テスト（e2e と、「最新」の選び方・古いものの削除の
ユニットテスト）。
Left to the implementer: 名前の形式とリポジトリの識別の持ち方（D9）、時刻の取り方
（テストでは注入できるようにする）、結果ファイルを書く場所のコード上の位置。
Stop and hand back if: 同じミリ秒で名前が衝突しないことを保証する方法が、D9 の範囲
（名前か置き場所）で作れない。

## Step 9 — 表示の純粋ロジック（web）

Purpose: 画面の見た目の前に、並び・位置・止まる場所・種類の枠などの計算を、テストできる
純粋関数として `model.js` に作る。
Specification: `docs/spec/kemi.md#R-VIEW`, `#R-NAV`.
Prerequisites: Step 4（由来と 2 単位の API の形が決まっていること）。
May change: `web/assets/model.js`、`web/assets/model.test.js`、`web/assets/app.js`（変えた
関数の呼び出し側を合わせる最小限だけ）。
Done when: `node --test web` と `npx tsc -p web --noEmit` が通り、次が固定されている。

- 1 列表示の並び
  - 連続する書き換えが、消した行の塊 → 足した行の塊の順に並ぶ。
  - 単語単位の強調は、対応する行の組で保たれる。今の `toDisplayLines` の交互の並びを直す。
  - 折りたたみ行の展開（`logicalIndex` での対応）は保たれる。
- 行コメントが、範囲の最後の行に置かれる（今の `placeThreads` は最初の行に置いている）。
- 止まる場所が `#R-NAV` どおりになる。
  - 変更ブロックの先頭（由来の行があるときはその行）
  - ブロック外のコメントは、範囲の最初の行
  - ブロック内のコメントは、ブロックの先頭と同じ
  - ファイル全体のコメントは含めない
- 次のファイルの選び方が `#R-NAV` どおりになる。
  - 見えている順（変更量順の並べ替えと重要のみの絞り込みを反映）で、グループをまたぐ。
  - 畳まれたツリーの中のファイルも含める。
  - ノイズ・バイナリ・止まる場所の無いファイルは飛ばし、見たのファイルは飛ばさない。
  - 両端では止まる。
- 件名から、コミットの種類の枠が `#R-VIEW` の形式どおりに切り出される。形式に合わない
  件名からは切り出さない。
- 状態が 1 文字（A / D / R / M）になる。

Shown by: test — `node --test web` の、振る舞いを名前にしたテスト。例:
`unified_shows_removed_block_before_added_block`、`range_comment_sits_under_last_line`、
`nav_stops_*`、`next_file_*`、`commit_type_box_*`、`status_letter_*`。
Left to the implementer: 関数の名前と分け方、データの形。
Stop and hand back if: 1 列の並べ替えで、既存の展開（折りたたみ行の `logicalIndex`）との
対応が崩れ、展開の仕組みそのものを変える必要が出る。

## Step 10 — 差分の描画

Purpose: 2 列の色、行番号の欄、折りたたみ行、由来の行、読み込み中のヘッダを、仕様どおりに
描く。
Specification: `docs/spec/kemi.md#R-VIEW`, `#R-ORIGIN`.
Prerequisites: Step 9。
May change: `web/`。
Done when:

- 2 列表示
  - 書き換え行の旧側が削除の色、新側が追加の色になる。
  - 変更箇所の強調も、旧側は削除の強調色、新側は追加の強調色になる。
  - 相手の無い側は斜線の無地になる。
- 行番号の欄に追加・削除の色が付き、変更記号（`+` / `−`）が太くなる。
- 折りたたみ行は、隠れている行数と、その下に続く範囲の旧・新の行番号を出す。
- 由来の行
  - 最終形では、変更ブロックの上に由来の行が出る。1 列でも 2 列でも出て、最終形以外では
    出ない。
  - 計算中は印を出す。
  - 上限を超えるファイルでは出さず、有効にすると出る。
  - 由来を押すと理由が開く。
- 読み込み中のヘッダ: ファイルを切り替えた直後、ヘッダは新しいファイルになり、読み込み中は
  操作できない。
- 仮想スクロール: `data-kemi-row` の行数の制御が保たれる。

Shown by: external — 「画面の確認に使う入力」のこのリポジトリ自身の範囲をブラウザ自動化で
開き、次を確かめる。スクリーンショットを報告に添える。

- 2 列表示の書き換え行が、上の色の組み合わせになっている。
- 1 列表示で、消した塊の後に足した塊が並ぶ。
- 行番号の欄の色、太い変更記号、折りたたみ行の行数と範囲が見える。
- 最終形で由来の行が 1 列・2 列の両方に出て、コミットごとでは出ない。押すと理由が開く。
- ファイルを切り替えた直後に見たを押しても、前のファイルに付かない。API の見た状態で
  確かめる。

Left to the implementer: CSS の構成、DOM のクラス名（`data-kemi-row` 以外）。
Stop and hand back if: 由来の行を仮想スクロールの行として入れると、止まる場所やコメントの
位置の計算と矛盾する。

## Step 11 — 単位の切り替え・見た・移動・上部バー

Purpose: グループ単位の切り替え、見たの進捗と「閲」の印、`n` / `p` と位置の帯、上部バーと
グループ帯・ツリーの見た目を載せる。
Specification: `docs/spec/kemi.md#R-UNIT`, `#R-SEEN`, `#R-NAV`, `#R-FOCUS`, `#R-VIEW`,
`#R-ORIGIN`.
Prerequisites: Step 10。
May change: `web/`。
Done when:

- 切り替え
  - コミット範囲では「最終形 | コミットごと」の切り替えがタイトルの右にある。1 コミットだけ
    の範囲でも出る。manifest / worktree / staged には出ない。
  - 作成中は読み込み中、失敗は「作れなかった」表示（理由と再試行）が出る。
  - 切り替えると同じパスのファイルを出す。コミットごとへは、そのパスを含む最初のコミット。
    同じパスが無ければ先頭のファイル。
  - 由来の「このコミットで見る」で、コミットごとの単位のそのコミットの該当行へ移る。改名
    されたファイルでは、そのコミット時点のパスへ移る。マージの由来には、移る操作が無い。
  - 切り替えても、見たとコメントは単位ごとに残る。コメントは付けた単位のファイルにだけ出る。
- 見た
  - 「見た」は文字付きのチェックで、`v` でも付け外しできる。ツリーと上部の進捗が同時に
    変わる。
  - ツリーは左端のチェックで見たを示し、取り消し線を使わない。
  - ツリーのグループ見出しに「見た数 / 全数」と進捗バーが出る。全部見たら「閲」の印になる。
  - 上部に、表示中の単位の進捗が出る。
  - 「閲」の印は、ここ（ツリーのグループ見出し）と承認の完了画面（Step 13）以外で使わない。
- 移動
  - 右下に「前の変更 / 次の変更」と「現在 / 全体」が出る。
  - `n` / `p` はファイルをまたいで動き、端で止まって「最後の変更です」「最初の変更です」と
    出す。
  - 文字の入力中は、`n` / `p` / `v` が働かない。今の `handleKey` の仕組みを保つ。
  - 位置の帯に変更とコメントの印と、見えている範囲の枠が出て、押すと移る。
- 上部バー・グループ帯・ツリー
  - グループ帯は `why` を畳み（開閉はページを開いている間だけ覚える）、`watch` を常に出す。
  - ツリーのグループ見出しは `why` と `watch` を出さない。
  - コミットの種類の枠は、ツリーのグループ見出しとグループ帯にだけ出す。
  - 上部の文字ラベルは、切り替え・送信のボタン・更新バッジだけ。左上は `kemi` の文字。
  - ツリーの状態は 1 文字で、`M` は目立たない。
- 状態の置き場所: 開閉の状態は、ページを開いている間だけ持つ。localStorage は、表示モード・
  テーマ・下書き以外に使わない。

Shown by: external — 「画面の確認に使う入力」をブラウザ自動化で開き、次を確かめる。
スクリーンショットを報告に添える。

- このリポジトリ自身の範囲で、次を確かめる。
  - 単位を切り替えて戻っても、見たとコメントが残る。
  - 由来から、コミットごとの該当行へ移る。
  - 見たを全部付けたグループに「閲」が出る。
  - `v` でツリーと上部の進捗が変わる。
  - 最後の変更で `n` を押すと止まる。
- 1 コミットの範囲に切り替えがあり、worktree・staged・manifest には無い。
- 50 万行のファイルで、帯と `n` を使ってもスクロールが固まらない。DOM の `data-kemi-row` の
  数が、表示中の行数程度に収まる。

Left to the implementer: 帯の描き方（DOM に全行を描かない限り自由）、見た目の細部。
Stop and hand back if: 50 万行で帯の印を求める計算が重く、仕様の「固まらない」を満たせない。

## Step 12 — コメントの UI

Purpose: 吹き出し・札・コメント一覧・編集と削除を、ページに載せる。
Specification: `docs/spec/kemi.md#R-VIEW`, `#R-COMMENT`, `#R-UNIT`, `#R-LIVE`.
Prerequisites: Step 6, Step 11。
May change: `web/`。
Done when:

- 吹き出し
  - 行コメントは、範囲の最後の行の下に、コードの列の位置から始まる吹き出しで出る。対象行を
    指し、範囲の行番号の欄の左端にコメントの色の線が付く。
  - 本文の表示幅は約 72 文字で、超える行は折り返す。
  - 「あなた」の札は出ない。
  - ファイル全体のコメントは、ヘッダの下に同じ吹き出しで出る。
- 札
  - 既存のコメントは 1 行の札に畳まれる。札は本文の 1 行目を出し、収まらなければ省略記号で
    切る。押すと本文がすべて出る。
  - 付けた直後のコメントは開いて出る。開閉はページを開いている間だけ覚える。
  - v1 の「長い本文を数行に畳む」は取り除く。
- 件数
  - ファイルヘッダに、そのファイルのコメント件数（ファイル全体のコメントを含む）が出る。
  - 上部のコメント一覧の入口に件数が出る。
- コメント一覧
  - すべての単位のコメントを並べる。各項目は、グループ単位・パス・行範囲・本文の 1 行目を
    出す。コミットごとの項目は、そのコミットの件名も出す。
  - 押すと、単位を切り替えてその行へ移る。
  - 履歴の書き換えで消えたグループのコメントは「消えたコミット」として出る。押しても
    移らず、本文を見せる。「古いコメント」の表示が付く。
  - 閉じると消える。
- 編集（本文と suggestion）と削除（確認付き）ができる。

Shown by: external — このリポジトリ自身の範囲をブラウザ自動化で開いて次の流れを確かめ、
スクリーンショットを報告に添える。

1. 行の範囲にコメントを付ける。範囲の最後の行の下に、開いた吹き出しが出る。
2. ファイルを移って戻ると、札になっている。札を開く。
3. 本文を編集する。
4. 別のコメントを削除する。
5. 別の単位で付けたコメントを、一覧から開く。
6. submit する。CLI の JSON（stdout）に編集後の本文が入り、削除したコメントが無い。

Left to the implementer: 仕様が決めていない文言、見た目の細部。
Stop and hand back if: 吹き出しを仮想スクロールの行の高さの実測と両立させられず、
スクロールが崩れる。

## Step 13 — 送信の UI、配色、画面の文言

Purpose: 確認ダイアログ・完了画面・送信後のボタンと、役割ごとの配色を仕上げる。画面の
英語の文言が、仕様の許す範囲に収まっているかも点検する。
Specification: `docs/spec/kemi.md#R-SUBMIT`, `#R-RESULT`, `#R-VIEW`, `#R-DIST`.
Prerequisites: Step 8, Step 12。
May change: `web/`、`crates/kemi-server/`（完了画面に渡す値の範囲）。
Done when:

- 確認ダイアログ
  - コメント件数（うち suggestion 付き、両方の単位の合計）を出す。
  - 表示中の単位の見た数 / 全数を出し、未読があれば注意を出す。
- 完了画面
  - 結果の JSON、コピーの操作、保存先を出す。保存に失敗したときは「保存できませんでした」を
    出す。
  - エージェントが反応しないときは、この JSON を会話へ貼れば済むと示す。
  - 承認のときは「閲」の印を出す。
- 送信後は、承認・変更要求のボタンが押せない。
- 配色は役割ごとに分かれ（藍・付箋の黄・藤・緑・赤）、追加と削除以外に赤系が無い。
  4 つのテーマのプリセットで読める（light / dark / solarized light / solarized dark）。
- 画面の文言を点検する。英語の文言は、`#R-DIST` の一覧にあるものだけ。一覧に無い英語は
  日本語にする。どちらにするか迷うものは、止めて報告する。

Shown by: external — このリポジトリ自身の範囲をブラウザ自動化で開いて次を確かめ、4 つの
テーマのスクリーンショットを報告に添える。

- 確認ダイアログの件数と見た数が合っている。
- 完了画面の JSON が、CLI の stdout と同じ。
- 結果の置き場所に書けない状態では、「保存できませんでした」が出る。
- 送信後に、ボタンが押せない。

`app.js` / `index.html` の文言に一覧外の英語が無いことは、目で点検して報告に書く。

Left to the implementer: 色の値（D2）、仕様が決めていない文言の細部。
Stop and hand back if: テーマのプリセット名（light / dark / solarized など）の英語が
`#R-DIST` の一覧に入るかどうか、判断が要る場合。仕様の一覧には、テーマ名が無い。

## Step 14 — 全体の検証と文書

Purpose: 仕様の検証コマンドと計測をすべて通し、利用者向けの文書を新しい振る舞いに合わせる。
人が最後に確かめる項目の一覧も揃える。
Specification: `docs/spec/kemi.md#R-VERIFY`, `#R-WS`, `#R-DEPS`, `#R-DIST`, `#R-VIEW`.
Prerequisites: Step 1〜13。
May change: `README.md`、`TODO.md`、`scripts/`（計測の不具合の修正だけ）。
Done when:

- `#R-VERIFY` の表のうち、配布を除くコマンドがすべて成功する。
- 計測の値が報告に残っている（Step 5 と同じ 3 つ）。
- README（英語）に、次が書かれている。
  - `--group-by` の既定が最終形に変わったこと
  - `--result` / `--any` / `--workspace` と、結果ファイルの場所
  - `n` / `p` / `v` のキー
- `TODO.md` の残課題が更新されている。
- 人が最後に画面で確かめる項目の一覧が、ステップの報告にまとまっている。仕様で「人が確認」と
  書かれた成功条件ごとに、どの入力で何を見ればよいかを書く。

Shown by: check — 次を順に実行する。

1. `cargo build --release`
2. `cargo test`
3. `node --test web`
4. `cargo clippy -- -D warnings`
5. `cargo fmt --check`
6. `npm ci && npx tsc -p web --noEmit`（`node_modules` はリポジトリに無い。ネットワークが要る）
7. `cargo deny check licenses`
8. `rg 'use (axum|hyper|tokio|std::process)' crates/kemi-core/src/domain` が import 行に
   一致しない
9. Step 5 の計測コマンド

Left to the implementer: README の構成。
Stop and hand back if:

- 計測のどれかが仕様の値を満たさない。基準を緩めずに報告する。
- テストやリントが失敗し、このステップの変更範囲で直せない。
- `npm ci` や `cargo deny` の導入に必要なネットワークが使えない。
