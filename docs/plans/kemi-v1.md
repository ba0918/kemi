# 実装計画: kemi v1

- 仕様: `docs/spec/kemi.md`（承認済み `2258abf`、`R-INPUT-1` と `R-SERVE` の
  成功条件の矛盾を解消する修正を含む）
- ブランチ名: `kemi-v1`
- 計画の状態: 承認待ち

## Goal

仕様 `docs/spec/kemi.md` の全要件を満たす kemi v1 を、リリース可能な状態
（`cargo build --release` と `dist plan` が通り、タグを切れば仕様どおりの
アセットが出る）まで作る。実際のタグ切り・公開・mise インストールの確認は
`ba0918-release` の受入に属する。

## Specification

`docs/spec/kemi.md` が唯一の規範。この計画は同仕様を節 ID で参照し、内容を
コピーしない。

## Approach and why

- 依存の向き（domain → source → server / cli）に沿って、**純粋な部分から作り、
  HTTP と UI を後に載せる**。domain のテストが git もブラウザも無しで回ると、
  後続の失敗の切り分けが速い。
- 実装は TDD で行う（`ba0918-tdd`）。各ステップの `Shown by: test` は
  RED → GREEN → REFACTOR の順で、失敗するテストから始める。`check` のステップも
  可能な限りテストを先に書く。
- 大規模対応の核は「起動時は一覧と統計だけ、行データは表示時」の一点。
  ステップ 4（source）で遅延取得の境界を決め、ステップ 5（server）でそれが
  崩れていないことをテストする。
- フロントはビルドなしの ESM。型は JSDoc で、`tsc` は検査だけに使う。表示の
  正しさはブラウザ自動化で、規模の条件は `scripts/` のフィクスチャで確かめる。
- リリース準備（cargo-dist、CI）は最後に回す。バイナリが完成してから
  パッケージングを合わせる方が、手戻りが少ない。

## Scope of change

リポジトリ全体。仕様 `R-WS` が宣言したレイアウトに加えて、`tests/fixtures/`、
`scripts/`、`.github/workflows/`、`README.md`、ライセンス 2 ファイル、
`dist-workspace.toml` を作る。

各ステップは、そのステップで必要になった依存クレートを、該当する
`Cargo.toml`（クレートとルート）と `Cargo.lock`、`web/package.json` に
追加してよい。

## Step order and prerequisites

```text
1 骨格 ─┬─ 2 整列 ──────────────────┐
        └─ 3 モデル ────────────────┤
                                    ├─ 4 source ─ 5 server ─ 6 webview 骨格
                                    │                └─ 7 コメント UI
                                    │                └─ 8 ハイライト
                                    │                └─ 9 ライブリロード
                                    └─ 10 digest
5,6,8,9,10 ─ 11 規模検証 ─ 12 リリース準備
```

- 2 と 3 は互いに独立。並べ替えてもよい。
- 10 は 3 と 4 を前提とする（4 の統計を共有する）。
- 6 は 5 の API 契約に依存する。7・8・9 は 6 の後。
- 11 は対象機能が全部動いてから。12 は 11 の後。

## Verification map

| Step | 確かめる仕様節 |
|---|---|
| 1 | R-WS, R-DIST（版の正典） |
| 2 | R-VIEW（行の整列、折りたたみ、単語単位、正規化） |
| 3 | R-COMMENT, R-FOCUS, R-DIGEST, R-VIEW（ノイズ）, R-WS（domain の純度） |
| 4 | R-INPUT, R-INPUT-1〜5 |
| 5 | R-SERVE, R-SUBMIT, R-INPUT-6, R-COMMENT（API と状態） |
| 6 | R-VIEW, R-FOCUS（表示）, R-WS（web） |
| 7 | R-COMMENT（UI）, R-SUBMIT（UI） |
| 8 | R-VIEW（ハイライト）, R-DEPS |
| 9 | R-LIVE |
| 10 | R-DIGEST, R-INPUT-6（--digest / --digest-top） |
| 11 | R-VERIFY, R-SERVE（性能）, R-VIEW（50 万行・DOM）, R-INPUT-2, R-DIGEST |
| 12 | R-DIST, R-DEPS（ライセンス）, R-VERIFY, R-WS（埋め込み） |

## Left to the implementer (plan-wide)

仕様の `## 委譲`（D1〜D7）の範囲。加えて、宣言されたパスの中のモジュール分割、
関数名、テストヘルパーの構成、ログの文言。さらに次の 3 つの値は、どれを選んでも
仕様の要求（折りたたみが存在する / 単語単位の強調がある / 変更を検知して
debounce する）を満たすため、実装に委ねる: 折りたたみのしきい値、単語分割の
粒度、監視の debounce 値。

## Stop conditions

一般（`ba0918-plan` の 4 条件）に加えて:

- 仕様に無い入力種別・受入境界・エラー挙動を決める必要が出たら、実装せず
  brainstorm へ差し戻す。
- 性能条件（`R-SERVE` の 1 秒、`R-DIGEST` の 100 KB）を満たせないことが計測で
  分かったら、黙って基準を緩めず、計測値と共に差し戻す。
- 仕様の成功条件を、検証手段の都合で満たせない（テストが書けない）と分かった
  場合は、成功条件そのものを人に確認してから直す。
- `similar` / `syntect` / `axum` / `notify` の API が期待と違い、仕様の要件を
  別の方法で満たす必要が出たら、その差分を報告する。

## Out of scope

- 納品後の環境更新（`~/.local/bin/diff-review` の置換、`diff-review-viewer`
  スキルの更新）。受入後に人が行う。
- タグ切りとリリース実行、および実アセットでの `mise use -g github:ba0918/kemi`
  の確認（`ba0918-release` の受入）。
- 仕様 `## 作らないもの` の機能そのもの（P1〜P10）。ただし `P10` が要求する
  `--out` の拒否は、作らないことの一部として実装してテストする（Step 5）。

---

## Step 1 — ワークスペースとツールチェーンの骨格

Purpose: 仕様 `R-WS` のレイアウトで、Node 無しの `cargo build` が通る最小の
ワークスペースを作る。Specification: `docs/spec/kemi.md#R-WS`, `#R-DIST`.
Prerequisites: なし。
May change: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.mise.toml`,
`src/main.rs`, `crates/kemi-core/`, `crates/kemi-server/`, `crates/kemi-webview/`,
`web/package.json`, `web/tsconfig.json`.
Done when: `cargo build` が Node 無しで成功し、ワークスペースがルートの bin と
3 つのライブラリを持ち、`cargo run -- --version` がルート `Cargo.toml` の
`version` を出し、`kemi-webview` が他クレートに依存せず、Rust と Node の版が
リポジトリ内で固定されている。
Shown by: check — `cargo build`、`cargo run -- --version`、`cargo metadata
--no-deps --format-version 1`、`cargo clippy -- -D warnings`、
`cargo fmt --check`。
Left to the implementer: crate 内部のファイル名、空モジュールの分割。
Stop and hand back if: `rust-toolchain.toml` の版が手元で入手できない、または
依存クレートの取得に失敗する。

## Step 2 — ドメイン: 差分整列と表示行

Purpose: 左右の行列から表示行（equal / replace / delete / insert / skip）を作り、
単語単位の強調と文脈の折りたたみを純粋関数として提供する。
Specification: `docs/spec/kemi.md#R-VIEW`.
Prerequisites: Step 1。
May change: `crates/kemi-core/src/domain/`。
Done when: 追加・削除・置換の各入力で行が正しく整列し、文脈の折りたたみと展開、
置換行の単語単位の強調、CRLF/LF の正規化がテストで固定されている。バイナリの
判定は D6 の委譲に従い、方式をここで固定しない。
Shown by: test — `cargo test -p kemi-core` の `align_*` / `segments_*` /
`collapse_*` / `normalize_*` テスト（RED → GREEN → REFACTOR）。
Left to the implementer: 折りたたみのしきい値、行データの内部表現、単語分割の
粒度。
Stop and hand back if: `similar` が行内の単語対応を取れず、仕様の「単語単位の
強調」を別アルゴリズムで満たす必要が出た場合。

## Step 3 — ドメイン: コメント、focus、ノイズ、digest

Purpose: コメントの検証と状態、focus のマージ規則、唯一のノイズ分類器、
digest の集約と有界化を純粋関数として提供する。
Specification: `docs/spec/kemi.md#R-COMMENT`, `#R-FOCUS`, `#R-DIGEST`,
`#R-VIEW`, `#R-INPUT-5`, `#R-WS`.
Prerequisites: Step 1。
May change: `crates/kemi-core/src/domain/`。
Done when: 行レンジの検証（1 始まり・両端含む・`side` の値域）、ファイル全体
コメント、旧側とファイル全体での suggestion 禁止、行テキストとハッシュによる
outdated 判定（行番号を自動で付け替えない）、focus の `--focus` 優先マージと
未知キー・未知パスのエラー、focus を変更量や拡張子から自動付与しないこと、
ノイズ分類（lockfile / 生成物 / vendored / minified / `linguist-generated` /
バイナリ）、digest のディレクトリ集約・増減合計の降順・2000 文字と `…` の
切り詰めがテストで固定されている。
Shown by: check — `cargo test -p kemi-core` の `comment_*` / `focus_*` /
`noise_*` / `digest_*` テスト（3 万ファイルの合成統計を渡す `digest_bounded_*`
を含む）を先に通し、続いて
`rg 'use (axum|hyper|tokio|std::process)' crates/kemi-core/src/domain` が
import 行に一致しないことを確認する。
Left to the implementer: ノイズの具体パターン（D6）、ハッシュ関数の選択、
分類器の内部構造。
Stop and hand back if: 仕様が沈黙している入力（例: 行番号 0、範囲の逆転）の
扱いを決める必要が出た場合。

## Step 4 — 入力ソース

Purpose: manifest / コミット範囲 / worktree / staged の 4 モードと `--focus` を
読み、グループとファイルのメタデータ（統計つき）とオンデマンドの行内容を
提供する。Specification: `docs/spec/kemi.md#R-INPUT`, `#R-INPUT-1`〜`#R-INPUT-5`.
Prerequisites: Step 2, Step 3。
May change: `crates/kemi-core/src/source/`, `tests/fixtures/`。
Done when: 旧ツール互換のキーを使う `tests/fixtures/legacy-manifest.json` と、
その内容から手で導ける小さな期待値 `legacy-manifest.expected.json`（グループ、
パス、状態、増減数、`approval`）を新規に作り、比較テストが通る。標準入力 `-`、
`old`/`old_path` 同時と欠損パスのエラー、`approval` の受理、focus レイヤの
マージとエラー、`R-INPUT` の表どおりのグループ生成（manifest は `g1…` と
title 既定「変更のレビュー」、commit はマージ以外・完全 sha・subject/body、
file は 1 グループ `all` で title `from...to`、worktree は `worktree`、
staged は `staged`）が一時 git リポジトリのテストヘルパーで固定されている。
worktree は untracked を含み上限で内容を出さず、staged は同じファイルに
未ステージの変更があってもインデックス側だけを表示する。
Shown by: test — `cargo test -p kemi-core` の `manifest_*` / `focus_*` /
`range_*` / `worktree_*` / `staged_*` テスト。
Left to the implementer: 入力の内部表現（trait か enum か）、numstat の解析方法、
untracked の上限値（D6）、一時リポジトリの作り方。
Stop and hand back if: 仕様のグループ規則（`R-INPUT` の表）と git の実際の
出力が食い違い、規則を変えないと進めない場合。

## Step 5 — HTTP サーバ、セッション、submit 契約

Purpose: ループバックの HTTP / SSE サーバを立て、レビューの API、セッション
状態、submit の契約（stdout の JSON、終了コード）、CLI の入力エラーを
成立させる。Specification: `docs/spec/kemi.md#R-SERVE`, `#R-SUBMIT`,
`#R-INPUT-6`, `#R-COMMENT`（API と状態）.
Prerequisites: Step 4。
May change: `crates/kemi-server/`, `src/main.rs`, `tests/`。
Done when: `api/review` / `api/file` / `api/comment` / `api/submit` /
`api/events` が動く。token 無しとクロスオリジンの `POST` が拒否され、
`Origin`/`Host` が検証される。起動と `api/review` はファイル内容を読まない
（内容取得の呼び出し回数が 0）。コメントの追加は stdout を汚さずに stderr へ
ライブ表示を出す。セッション状態（コメント、見た、折りたたみ、解決）が
API 経由で保持される。submit は同時に 2 つ届いても 1 つだけ受理され、
もう片方に 409 を返し、stdout が 1 つの JSON 文書になり、`R-SUBMIT` の
フィールド規約（`side` の値域、ファイル全体コメントの null と空 `quote`、
旧側・ファイル全体での `suggestion: null`、`replies`/`resolved` の常時存在、
作成順、`approval` のそのまま返却、改行・引用符入り本文）を満たす。終了コード
0 / 1 / 2 / 130 が verdict とエラーに対応し、実行時の git・I/O 失敗は
stdout に JSON を出さず 2 で終わる。stderr に `kemi: <url>` の 1 行が出る。
`--out` は理由付きで 2、複数モードの同時指定とモード無しも 2、
`--base` 相対の解決が効く。
Shown by: test — `cargo test -p kemi-server` と `cargo test --test e2e` の
`review_api_*` / `token_required_*` / `cross_origin_rejected_*` /
`content_reads_zero_*` / `comment_live_stderr_*` / `seen_state_*` /
`submit_stdout_json_*` / `submit_schema_*` / `submit_concurrent_409_*` /
`approval_passthrough_*` / `exit_code_*` / `runtime_error_exit2_*` /
`cli_out_rejected_*` / `cli_mode_exclusive_*` / `cli_no_mode_*` /
`cli_base_*` テスト。バイナリ spawn の e2e と in-process を使い分ける。
Left to the implementer: 内部 API の JSON 形（D7）、token の生成方法、
SSE イベント名、`--serve` 互換の受理のしかた（D1）。
Stop and hand back if: axum の SSE がテスト環境で観測できず、`R-LIVE` の
検証手段が成立しない場合。

## Step 6 — webview の骨格と差分表示

Purpose: ブラウザのページを組み、ファイルツリー、unified / split、折りたたみ、
仮想スクロール、テーマ、focus・ノイズ・承認対象の表示を API に結線する。
Specification: `docs/spec/kemi.md#R-VIEW`, `#R-FOCUS`, `#R-WS`.
Prerequisites: Step 5。
May change: `web/`, `crates/kemi-webview/`。
Done when: ページがレビューを表示し、split/unified の切替、ハンクの展開、
ファイルツリーからのジャンプ、sticky ヘッダ、ツリーのパス・状態・増減・
ノイズ・重点の表示、focus バッジと note と「重点のみ」フィルタ、グループ
見出し下の `watch` 表示、ノイズの既定折りたたみ、変更量ソートのトグル
（初期の並びは入力順）、light/dark の自動追従と手動切替のプリセット、
バイナリのバイト数表示、折返しは既定 off で on のとき実測高さ、最終行改行の
非表示、`data-kemi-row` の行数制御、「見た」の操作と再読込後の保持、
`approval` のパスと `identity` のフッター表示が動く。
Shown by: check — `node --test web` で JS の純ロジック（行の窓計算、状態、
キー処理）を通し、`npx tsc -p web --noEmit` を通し、サーブしたページを
ブラウザ自動化で操作して次を表明する: ツリーと一覧が出る / split 切替で
列が変わる / ハンクが開く / ノイズが畳まれ開ける / focus バッジと note が
出てフィルタで絞れる / `watch` が出る / 「見た」が再読込後も残る /
既定の並びが入力順 / approval フッターが出る。
Left to the implementer: テーマの具体色（D2）、CSS の構成、DOM のクラス名
（`data-kemi-row` 以外）。
Stop and hand back if: ブラウザで仮想スクロールの行数制御が成立せず、
`R-VIEW` の 50 万行条件を満たす設計に変更が必要な場合。

## Step 7 — コメント UI と submit UI

Purpose: 行レンジとファイル全体のコメント、返信、解決、suggestion の入力、
outdated 表示、承認 / 変更要求の送信をページに載せる。
Specification: `docs/spec/kemi.md#R-COMMENT`, `#R-SUBMIT`, `#R-FOCUS`.
Prerequisites: Step 6。
May change: `web/`, `tests/`, `crates/kemi-server/`（表示に必要な API の調整のみ）。
Done when: 行番号の範囲選択からスレッドを追加でき、返信と解決ができ、新側の
行レンジに suggestion（置換文字列）を付けられ、旧側とファイル全体では
suggestion が付かず、ファイル変更後の再取得で対象コメントが「古い」表示に
なり（行番号は付け替えられない）、submit 後にページが終了状態になる。
submit の中身は Step 5 の契約テストで固定済みの形で出る。
Shown by: check — `node --test web` のコメント状態テストを通し、ブラウザ
自動化で「コメント追加 → ファイル変更 → 再取得 → 古い表示 → submit」の一連を
操作し、終了後に CLI 側の JSON を読み取って本文・返信・解決・`quote`・
`outdated`・suggestion を表明する。
Left to the implementer: UI の文言、スレッドの見た目、下書きの保存キー。
Stop and hand back if: 行選択と suggestion の対応（挿入のみの表現）が
仕様 `R-COMMENT` の書き方で表現できない入力が出た場合。

## Step 8 — 構文ハイライト

Purpose: 左右のファイル内容をサーバ側で着色し、行範囲に切り出して配る。
Specification: `docs/spec/kemi.md#R-VIEW`, `#R-DEPS`.
Prerequisites: Step 5, Step 6。
May change: `crates/kemi-server/`, `web/`, `Cargo.toml`（該当クレートとルート）。
Done when: Rust の複数行文字列とブロックコメントをまたいでも色が壊れず、
片側が 10,000 行を超えるか 1 MB を超えるファイルでは自動的に切れ、その
ファイルでだけ有効化でき、`cargo tree -e features` に oniguruma 系の C 依存が
出ない。
Shown by: check — `cargo test -p kemi-server` の `highlight_multiline_*` /
`highlight_cap_*` テストを通し、続いてブラウザ自動化でハイライト有効の
ファイルを開き、複数行にまたがる文字列とコメントに対応する強調要素が存在する
ことを表明する。
Left to the implementer: スタイルの当て方（クラスかインラインか）、キャッシュ
方式（D6）、拡張子から言語への対応表。
Stop and hand back if: 純 Rust 正規表現バックエンドの速度が実用に足りず、
`R-VIEW` の条件を満たせない計測結果が出た場合。

## Step 9 — ライブリロード

Purpose: 新側の供給元だけを監視し、変更を SSE のバッジで知らせ、再取得で
outdated を更新する。Specification: `docs/spec/kemi.md#R-LIVE`, `#R-COMMENT`,
`#R-INPUT-3`.
Prerequisites: Step 5, Step 6。
May change: `crates/kemi-server/`, `web/`, `crates/kemi-core/src/source/`。
Done when: worktree モードと manifest の `new_path` で対象ファイルを外部から
変えるとバッジが出て、押すと差分が更新され、スクロール位置が保たれ、その
ファイルのコメントが outdated になり、`--group-by commit --to HEAD` では
新しいコミットでバッジが出て、`.git` の他の書き込みでは出ない。監視は
debounce される。
Shown by: check — `cargo test -p kemi-server` の `live_worktree_*` /
`live_manifest_path_*` / `live_ref_*` / `live_debounce_*` テスト（一時
リポジトリでファイルを変更し、SSE のイベントを読む）を通し、続いてブラウザ
自動化でバッジの出現、クリック後の内容更新、スクロール位置の保持を表明する。
Left to the implementer: debounce の値、イベントの JSON 形（D7）、監視の
再登録方法。
Stop and hand back if: テスト環境で inotify が使えず、`R-LIVE` の検証が
成立しない場合。

## Step 10 — digest モード

Purpose: `--digest` / `--digest-top` を実装し、行内容を含まない有界な地図を
stdout に 1 つ出す。Specification: `docs/spec/kemi.md#R-DIGEST`, `#R-INPUT-6`.
Prerequisites: Step 3, Step 4。
May change: `src/main.rs`, `crates/kemi-core/src/domain/`。
Done when: JSON が `R-DIGEST` の契約どおりで、`top_files` が増減合計の降順・
同数パス昇順、`directories` がルート直下で集約し直下ファイルを `"."` の
1 項目にまとめ、文字列が 2000 文字 + `…`、3 万ファイルで 100 KB 未満、
行内容と `quote` を含まない。`--digest-top` が効く。
Shown by: test — `cargo test -p kemi-core` と `cargo test --test e2e` の
`digest_mode_schema_*` / `digest_mode_bounded_*` / `digest_mode_top_n_*`
テスト（合成データのみ。フィクスチャ計測は Step 11）。
Left to the implementer: なし（D6 の範囲の値のみ）。
Stop and hand back if: 100 KB 未満が契約を守ったまま達成できない計測結果が
出た場合。

## Step 11 — フィクスチャ、性能、スケール検証

Purpose: 仕様 `R-VERIFY` の生産者（フィクスチャ生成器と計測スクリプト）を作り、
性能と規模の条件を実際に測る。Specification: `docs/spec/kemi.md#R-VERIFY`,
`#R-SERVE`, `#R-VIEW`, `#R-INPUT-2`, `#R-DIGEST`.
Prerequisites: Step 4, 5, 6, 8, 9, 10。
May change: `scripts/`, `tests/`, `PROJECT.md`（未着手の注記を外す）。
Done when: `scripts/gen-fixture.sh` が同じ引数で同じ内容の git リポジトリを
作り（tree ハッシュ一致）、`scripts/measure-startup.sh` が stderr の
`kemi: <url>` を読んで 3 回の中央値を出し、10,000 ファイルで 1 秒未満、
10,000 ファイルの `--group-by commit` でグループ数がマージ以外のコミット数と
一致し、`--group-by file` で同一パスが重複せず、50 万行のファイルで
スクロールが固まらず `data-kemi-row` の要素数が表示中の行数程度、
30,000 ファイルの `--digest` が 100 KB 未満になる。
Shown by: check — `cargo build --release` → `scripts/gen-fixture.sh` を同じ
引数で 2 回実行して tree ハッシュを比較 → `scripts/measure-startup.sh` →
ブラウザ自動化で `data-kemi-row` を数える → 人が 50 万行ファイルを操作する
（合否: 3 秒を超える操作不能な停止が発生しない）。計測値を計画の完了報告に
記録する。
Left to the implementer: CI 用の余裕ある閾値、フィクスチャの内容とファイル
構成、ブラウザ自動化の道具（agent-browser 等）。
Stop and hand back if: 開発機で目標を大きく外れた計測値が出た場合（黙って
基準を緩めない）。

## Step 12 — リリース準備

Purpose: README、ライセンス、CI、cargo-dist を揃え、タグを切れば仕様どおりの
アセットが出る状態にする。Specification: `docs/spec/kemi.md#R-DIST`,
`#R-DEPS`, `#R-VERIFY`, `#R-WS`.
Prerequisites: Step 11。
May change: `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `Cargo.toml` の
メタデータ, `dist-workspace.toml`, `.github/workflows/`, `deny.toml`,
`PROJECT.md`。
Done when: CI が `cargo test` / `clippy` / `fmt` / `tsc` / `node --test` /
`cargo deny check licenses` / domain 純度の `rg` を回し、`v*` タグで
4 ターゲット（`x86_64`/`aarch64-unknown-linux-musl`、
`x86_64`/`aarch64-apple-darwin`）の `kemi-<target>.tar.xz` と `.sha256` を
出す設定があり、ローカルの `dist plan` で同じアセット名が確認でき、
release バイナリを `web/` の無い場所から起動してもページが配られ（資産の
埋め込み）、README が英語で導入・使い方・JSON 契約を説明し、UI 文言が
日本語である。タグの作成とリリース実行はこのステップに含めない。
Shown by: check — `cargo deny check licenses`、`dist plan`、
`target/release/kemi` をリポジトリ外の cwd から起動してページを取得、
`.github/workflows/` のファイル確認、ローカルでのテスト一式と UI 文言の目視。
Left to the implementer: CI のトリガー構成、cargo-dist のバージョン、README の
文面。
Stop and hand back if: cargo-dist の設定にリポジトリ側の権限や設定が必要で、
ローカルで `dist plan` まで確認できない場合。
