# セッションの写しを `<id>.payload` へ分ける

## Goal

セッション状態を保存しても、復元に使うレビューの写しをディスクに書き直さなくなる。

## Specification

`docs/spec/kemi.md` の `R-SESSION セッションの保存と復元`。この計画は節を参照するだけで、
本文を写さない。実装する前に節を通しで読むこと。

## Approach and why

いまは `<id>.session` 1 つに「セッションの情報」「セッション状態」「凍結したレビューの
メタデータ」「変更ファイルの内容（gzip）」がすべて入っている。セッション状態は変更のたびに
書くので、見たの印 1 つでもこの全部を書き直す。

仕様が決めた入れ分けは 2 つ。

- `<id>.session` — セッションの情報とセッション状態だけ
- `<id>.payload` — 写し（凍結したレビューのメタデータ＋変更ファイルの内容）

この計画を書くときに実測した値。`MetaDto` を組み立てて `serde_json::to_vec` した長さで、
レビューは 1 グループに N ファイル、グループ単位 2 つ分、パスは
`crates/kemi-server/src/api/module_<n>/handler.rs` の形。「情報 + 状態だけ」は全ファイルを
見た状態に、ファイル数 / 50 件のコメントを足した値。計測用のテストは残していない。

| レビューのファイル数 | いまの meta（レビューの一覧込み） | 情報 + 状態だけ |
|---|---|---|
| 100 | 49 KB | 1 KB |
| 1,000 | 486 KB | 12 KB |
| 10,000 | 4,898 KB | 129 KB |

レビューのメタデータを写しへ移さないと、写しを分けても 10,000 ファイルで 4.9 MB を毎回
書き続けることになる。ここが分割の本体で、gzip の内容を移すことだけが目的ではない。

2 つのファイルを同時に原子的に差し替える移植可能な方法は無いので、仕様は不変条件を 1 つ
決めて、そこから書く順序を導く形になっている。実装もその順序をそのまま持つ。

### セッション形式の版

版を表す定数がいま 2 つある。

- `crates/kemi-core/src/session/encoding.rs` の `VERSION: u8` — ファイルの先頭に書き、
  読むときに検査する。
- `crates/kemi-core/src/session/mod.rs` の `FORMAT: u32` — meta の JSON に `format` として
  書くが、**読むときに検査していない**（`MetaDto::into_parts` は見ない）。

`VERSION` を 1 から 2 へ上げ、**`FORMAT` は消す**。検査しない版表示が 2 つあると食い違うため。
版を持つ場所は `VERSION` ひとつにする。

**版 1 を読むコードは書かない。** 版 1 は製品の版 0.1.8（2026-09-20 公開）でだけ存在し、
読めない版のセッションの扱いは `R-SESSION` が既に決めている。手元に残った版 1 のセッションは
その規則に落ちる。

### `<id>.payload` の中身

写しをひとまとまりの gzip として置く。ファイルの目印も版も付けない。読み手は `<id>.session`
の版を確かめてからでないと `<id>.payload` を見に行かないので、版の食い違いは構造的に起きない。
gzip 自身が先頭に目印を持つ。中の並べ方は `docs/spec/kemi.md` の委譲 D12 の範囲で、実装が
決める。

**1 セッション 20 MB の上限は、この `<id>.payload` 全体の大きさで判定する**（仕様がそう
決めている）。いまは変更ファイルの内容だけを gzip した長さで判定しているので、判定の対象が
変わる。凍結したレビューのメタデータが加わる分、境目にある写しは今より使えなくなりやすい。

### 一時ファイルの名前

書き込み中の一時ファイルの名前は、`.session` でも `.payload` でも終わらせないこと。掃除は
名前の接尾辞で `<id>.payload` を見分けるので、書き込み中のものが孤児の候補に見える。

## Scope of change

- `crates/kemi-core/src/session/encoding.rs`
- `crates/kemi-core/src/session/mod.rs`
- `crates/kemi-core/src/session/store.rs`
- `tests/e2e.rs`（Step 3 と Step 4）
- `crates/kemi-server/tests/server.rs`（Step 1。上限のテストが 1 本ある）
- `Cargo.toml`、`CHANGELOG.md`（Step 6 のみ）

`crates/kemi-server/src/` と `src/main.rs` は、`kemi-core` の公開型（`SessionCopy`、
`CopyState`、`StoredSession`、`OpenSession` のメソッド）の形が変わらない限り触らない。変える
必要が出たら Step の「Stop and hand back if」に当たる。

## Step order and prerequisites

Step 1 が形式を変えるので最初。Step 2〜4 は Step 1 の上に積む。Step 5 は Step 2 と Step 4 が
終わってから（3 つの経路がそろわないと順序を見られない）。Step 6 は全部が緑になってから。

## Verification map

| 仕様の成功条件 | Step |
|---|---|
| 写しにだけあるファイルのパスが `<id>.session` に現れない | 1 |
| セッションのファイルとディレクトリが unix で `0600` / `0700`（既存。`<id>.payload` も） | 1 |
| 読めないセッション形式の版は終了コード 2（既存。版を上げても成り立つ） | 1 |
| `<id>.payload` を壊れたバイト列にしても状態の保存が成功し、そのままである | 2 |
| `<id>.payload` を消すと一覧に出ず、id 指定の復元が終了コード 2 | 3 |
| `<id>.session` の無い `<id>.payload` が消える／起動中のものは消えない | 4 |
| 「使える」でない `.session` と `.payload` が揃っていても上限で 2 つとも消える | 4 |
| 合計は 2 ファイルの合算で数える | 4 |
| 書く順序（`<id>.payload` が先、捨てると消すは逆） | 5 |

## Left to the implementer

- `<id>.payload` の中の並べ方（凍結したレビューのメタデータと内容の置き方、境界の持ち方）。
  委譲 D12 の範囲。
- 新しい関数名・内部の構造・補助関数の切り出し。
- 一時ファイルの名前（上の制約の範囲で）。

入れ分け（何が `<id>.session` に入り、何が `<id>.payload` に入るか）、上限の判定対象、
書く順序、失敗したときの振る舞いは仕様が決めているので、ここには入らない。

## Stop conditions

各 Step の「Stop and hand back if」に加えて、次のどれかに当たったら手を止めて戻す。

1. 仕様に書かれていない意味を決める必要が出た、または承認済みの内容から外れる必要が出た。
2. 不可逆な操作、特権の要る操作、危険な対象への操作が必要になった。
3. 変更が `Scope of change` の外へ広がった。
4. approach を変えても進まない。

## Test command

`cargo test --workspace`。ルートに package があるので、`--workspace` を付けないとルートの
テストしか走らない。`cargo clippy --workspace --all-targets -- -D warnings` と
`cargo fmt --check` は各 Step の終わりに通す。フロントは触らないので `node --test web` は
Step 6 の最終確認だけでよい。

## Out of scope

- 版 1 のセッションを読む移行コード（書かない）。
- セッション状態の書き込みをまとめる（debounce）。仕様の「変更のたびに書く」を弱めるので
  却下済み。
- `sessions/` 以外の保存（結果ファイル `R-RESULT`）。
- タグを打つこと、リリースを公開すること（利用者が判断する）。

---

## Step 1 — `<id>.session` と `<id>.payload` に分けて往復する

Purpose: 写し（凍結したレビューのメタデータと変更ファイルの内容）を `<id>.payload` へ移し、
`<id>.session` をセッションの情報とセッション状態だけにする。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`。
Prerequisites: なし。
May change: `crates/kemi-core/src/session/encoding.rs`、`crates/kemi-core/src/session/mod.rs`、
`crates/kemi-core/src/session/store.rs`、`crates/kemi-server/tests/server.rs`。
Done when:
  - 写しを持つセッションを保存して読み戻すと、情報・状態・写しがすべて元どおりになる。
  - `sessions/` に `<id>.session` と `<id>.payload` の両方がある。
  - 写しにだけあるファイルのパス（セッション状態のどのコメントも指していないもの）が
    `<id>.session` のバイト列に現れない。
  - 20 MB の上限の判定が `<id>.payload` 全体の大きさになっている。
  - `<id>.payload` が unix で `0600` になっている。
  - `encoding::VERSION` が 2 で、`FORMAT` が無い。
  - `encoding.rs` 冒頭の図、`mod.rs` 冒頭、`store.rs` 冒頭と `read_meta` の上の説明が、
    分割後の置き方に合っている。
Shown by: test — RED → GREEN → REFACTOR。
  - 既存 `session_roundtrips_info_state_and_copy`（`crates/kemi-core/src/session/store.rs`）を
    2 ファイル構成で通るように直し、両方のファイルがあることも確かめる。
  - 既存 `session_copy_is_stored_as_gzip` は `<id>.session` から payload を取り出しているので
    **必ず壊れる**。`<id>.payload` を見る形に直す。
  - 既存 `session_files_are_owner_only` は `<id>.session` の権限しか見ていない。`<id>.payload`
    も見るように足す。
  - 既存 `session_copy_over_the_limit_is_unusable`（store.rs）と
    `session_copy_over_the_limit_is_unresumable`（`crates/kemi-server/tests/server.rs`）を、
    判定の対象が `<id>.payload` 全体であることが分かる形に直す。
  - 既存 `session_rejects_an_unknown_format_version` がそのまま通ること。
  - 新規 `session_file_holds_no_frozen_review_paths`: 状態のコメントが指していない特徴の
    あるパス（例 `src/needle_marker.rs`）だけを写しに入れて保存し、`<id>.session` のバイト列に
    そのパスが含まれないことを確かめる。
Left to the implementer: `<id>.payload` の中の並べ方（D12）。
Stop and hand back if:
  - `kemi-core` の公開型の形を変えないと分けられないと分かったとき（`crates/kemi-server/src/`
    と `src/main.rs` に波及する）。
  - `<id>.session` から写しを外すと `SessionStore::list` が必要な値（題、全ファイル数、
    見た数、写しの状態）を取れないと分かったとき。
  - 上限の判定対象が変わることで、既存のテストが表す振る舞いと仕様が食い違うと分かったとき。

## Step 2 — 状態の書き込みが `<id>.payload` に触らない

Purpose: セッション状態の保存が `<id>.session` だけを書き、写しのファイルを読みも書きも
しないようにする。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/session/store.rs`。
Done when: 写しを保存したセッションで状態を何度保存しても、保存が成功し、`<id>.payload` の
バイト列が変わらない。`OpenSession` が gzip 済みの写しをメモリに持つ理由（「書き直しの
たびに圧縮し直さないため」）はもう当てはまらないので、フィールドとその説明が現状に合って
いる。
Shown by: test — RED → GREEN。新規
`session_state_write_leaves_the_copy_file_untouched`: 写しを保存したあと `<id>.payload` を
gzip として読めないバイト列（例 `b"not a gzip"`）に置き換え、状態の保存を数回行い、保存が
成功し、そのバイト列がそのまま残っていることを確かめる。時間は測らない。
Left to the implementer: `OpenSession` が写しをメモリに持ち続けるか、持つなら何のために持つか
（どちらでもディスクの振る舞いは同じ）。
Stop and hand back if: 状態の保存で `<id>.payload` を読まないと写しの状態を決められない経路が
見つかったとき。

## Step 3 — 写しのファイルが無いセッションを一覧と復元から外す

Purpose: `<id>.session` が写しの状態を「使える」と書いているのに `<id>.payload` が無い
セッションを、一覧に出さず、復元もさせない。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/session/store.rs`、`tests/e2e.rs`。
Done when: 写しを持つセッションの `<id>.payload` を消すと、そのセッションが
`SessionStore::list` の結果に出ず、`SessionStore::open` が理由つきで失敗する。CLI では
`kemi --resume` の一覧に出ず、id を指定した `kemi --resume <id>` が終了コード 2 で終わり、
stdout が空になる。
Shown by: test — RED → GREEN。
  - 新規 `session_without_its_copy_file_is_not_listed_and_cannot_be_opened`
    （`crates/kemi-core/src/session/store.rs`）。`list` から外れることがこの Step で新しく
    なる部分で、`open` が失敗すること自体は Step 1 の時点で成り立っているかもしれない。
    両方をこの 1 本で固定する。
  - `tests/e2e.rs` に、`<id>.payload` を消したセッションに対する `kemi --resume <id>` が
    終了コード 2・stdout が空であること、端末でない `kemi --resume` の一覧にその id が
    出ないことを足す。
Left to the implementer: 失敗の理由の文言（`R-SESSION` は契約にしていない）。`SessionError` に
列挙子を足すかどうか（`src/` と `crates/kemi-server/src/` に `SessionError::` のパターン
マッチは無いので、どちらでも外へは波及しない）。
Stop and hand back if: `<id>.payload` の有無を、`list` がすでに行っている `read_dir` の
結果から決められないと分かったとき。

## Step 4 — 掃除と削除を 2 ファイルに合わせる

Purpose: 掃除が 2 ファイルの合算で数え、消すときは 2 ファイルとも消し、`<id>.session` の
無い `<id>.payload` を回収する。写しが使えなくなったときは、そのセッション自身が
`<id>.payload` を消す。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`。
Prerequisites: Step 1、Step 2。
May change: `crates/kemi-core/src/session/store.rs`、`tests/e2e.rs`。
Done when:
  - 件数とバイト数の上限の判定が 2 ファイルの合算になっている。上限で消すときは 2 ファイル
    とも消える。
  - `<id>.session` の無い `<id>.payload` が消える。起動中のレビューが持っているものは残る。
  - 写しの状態が「使える」でなくなったとき、そのセッションが `<id>.session` を書いた後に
    `<id>.payload` を消す。**状態が空で写しも無くなって `<id>.session` ごと消す枝でも、
    `<id>.payload` が残らない**（いまの `persist` はこの枝で `<id>.session` だけを消して
    戻る）。
  - `SessionStore::delete` と `OpenSession` の削除が 2 ファイルとも消す。
  - `tests/e2e.rs` の、submit の後に `sessions/` が空になることを見るヘルパが、
    `<id>.payload` も数える。
Shown by: test — RED → GREEN。
  - 既存 `session_cleanup_keeps_the_total_size` を、合算で数えることが分かる形に直す。
  - 既存 `session_cleanup_keeps_the_newest_sessions`、
    `session_cleanup_keeps_a_locked_session_out_of_the_count`、
    `session_cleanup_removes_orphan_lock_files`、`session_delete_removes_the_file`、
    `session_without_state_and_an_unusable_copy_is_not_kept` が 2 ファイル構成で通ること。
  - 新規 `session_cleanup_removes_a_payload_without_its_session`: `<id>.session` の無い
    `<id>.payload` を置いてからセッションを書くと消えること。ロックを取った id の
    `<id>.payload` は残ること。
  - 新規 `session_cleanup_evicts_a_stale_pair_by_the_limits`: 写しの状態が「使える」でない
    `<id>.session` と `<id>.payload` が揃って残っている状態を作り、件数かバイト数の上限を
    超えさせると 2 ファイルとも消えること。この状態は公開 API だけでは作れないかもしれない
    （強制終了の途中でしか起きない形なので）。その場合はテストがファイルを直接置いてよい。
Left to the implementer: `read_dir` の結果を id で突き合わせる持ち方。
Stop and hand back if: 掃除が `<id>.session` の中身を読まずには仕様の条件を満たせないと
分かったとき（そのときは仕様の側の判断が要る）。

## Step 5 — 書く順序と、掃除が中身を読まないことを差分で確かめる

Purpose: 仕様の不変条件を保つ書き方になっていることを、人が差分を読んで確かめる。機械で
検査するには製品コードに失敗を差し込む口が要り、テストのためのコードが製品に漏れる。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`。
Prerequisites: Step 2、Step 4。
May change: なし（確認だけ。直すことがあれば該当の Step に戻る）。
Done when: 次の 4 つが差分から読み取れる。
  1. 写しを保存する経路が、`<id>.payload` を書いてから `<id>.session` を書く。
  2. 写しを捨てる経路が、`<id>.session` を書いてから `<id>.payload` を消す。
  3. セッションを消す経路が、`<id>.session` を消してから `<id>.payload` を消す。
  4. 掃除が孤児を見分けるのに、`<id>.session` の中身を読んでいない
     （`read_dir` の結果だけで決めている）。
Shown by: external — Step 1〜4 の差分（`git diff <分岐点>..HEAD`）を人が読み、上の 4 つを
それぞれ該当のコードで指させたら合格。1 つでも指せなければ該当の Step に戻る。
Left to the implementer: なし。
Stop and hand back if: 順序を保てない経路が見つかったとき（仕様の不変条件が破れるので、
仕様の側の判断が要る）。

## Step 6 — 製品の版 0.1.9 と変更履歴

Purpose: 破壊的変更として利用者に伝わる形で出す。
Specification: `docs/spec/kemi.md`#`R-SESSION セッションの保存と復元`（振る舞いの根拠）。
Prerequisites: Step 1〜5。
May change: `Cargo.toml`、`CHANGELOG.md`。
Done when: `Cargo.toml` の `[workspace.package] version` が `0.1.9` で、`CHANGELOG.md` に
`[0.1.9]` の節があり、`[Unreleased]` にあった項目がそこへ移っていて、破壊的変更として
「この版より前に保留したレビューは復元できない。復元したいものが残っていれば 0.1.8 のうちに
submit してほしい」旨が書いてあり、比較リンクが更新されている。
Shown by: check — 次の順に通す。
  1. `cargo test --workspace`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo fmt --check`
  4. `node --test web`
  5. `grep -n '0\.1\.9' Cargo.toml CHANGELOG.md`
Left to the implementer: 変更履歴の言い回し。
Stop and hand back if: なし。
