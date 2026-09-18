# セッションの保存と復元を実装する

## Goal

submit せずに終えたレビューを、当時の差分ごとディスクに残す。次に `kemi --resume`
でそれを開き、コメントと見たを保ったまま続きを読める。終了時には復元コマンドを
案内し、`submit` の結果は元いたワークスペースの結果ファイルとして残る。

## Specification

`docs/spec/kemi.md`（セッション改訂 `da80764`）。見出しは
`R-SESSION セッションの保存と復元`、`R-INPUT-6 その他の入出力`、
`R-SERVE 配信モデル`、`R-SUBMIT 送信と契約`、`R-RESULT 結果ファイル`、
`R-LIVE ライブリロード`、`R-VIEW 差分の表示`、`R-UNIT グループ単位の切り替え`、
`R-ORIGIN 由来`、`R-DIST 名称と配布`、`R-DEPS 依存`、`R-VERIFY 検証`。

## Approach and why

- **セッションの形式と保存は kemi-core の新しいモジュールに置く。** 復元で
  `ReviewSource` を実装するには、`Plan` / `SideRef` / `file_origin` など
  crate 内限定の型に触れる必要があり、形式の型と復元の実装を同じクレートに
  置くのが素直。ルートのバイナリと `kemi-server` の両方から使える。
- **ドメイン型に serde を付けず、専用の DTO を作る。** `ReviewMeta` などに
  直接 `Serialize` / `Deserialize` を足すと、保存形式がドメインの変更に
  引きずられる。`digest` が専用の型を持っている前例に合わせる。
- **凍結は「両方のグループ単位のメタデータ」と「各ファイル id の `content()` の
  実バイト」を取る。** `Plan` は git とディスクへの参照しか持たないので、
  参照が生きているうちに読んで写す。内容は非 UTF-8 のバイト列をそのまま
  往復できる形で束ね、gzip する。
- **復元は `ReviewSource` を実装する凍結ソースで配る。** 内容は凍結バイトから
  返し、`watch_paths` は空にする。`watch::start` は空なら何もしない既存の作り
  なので、これだけで監視も更新バッジも止まる（R-LIVE の復元条件）。
- **由来だけは元のリポジトリで計算する。** 起動時に解決した `from` / `to` の
  完全な sha と、元のワークスペースのパスをセッションに記録し、ファイルごとの
  old/new のパスは凍結したメタデータから作る。merge-base は元のリポジトリから
  求める。リポジトリが無い、またはパスが導けないファイルは、既存の
  「特定できない」表示に落とす（R-ORIGIN の失敗経路と同じ）。
- **保存はルートの `src/result.rs` の作法を踏襲する。** 一時ファイルに書いて
  rename する原子的な差し替え、unix の `0600` / `0700`、決まった上限での掃除。
  core の保存モジュールは渡されたディレクトリだけを扱い、置き場所の解決
  （`XDG_STATE_HOME` → `HOME` / `LOCALAPPDATA` と `sessions/` の組み立て）は
  ルートの `src/result.rs` の隣に足す。
- **ロックは `File::try_lock`（Rust 1.89 で安定）を使う。** OS がプロセスの終了で
  解放するので、クラッシュの後に残ったロックを奪う処理が要らない。仕様の
  「pid の生死で見分けて奪う」は D13 が判定の細部を委譲している範囲で、OS の
  解放に置き換える（観測できる契約は「取れない復元は 2」「クラッシュ後は復元
  できる」のまま）。追加のクレートは要らない。
- **サーバへは sink を注入する。** `ServeParams` に任意の保存先を足し、コメント・
  見た・折りたたみ・解決の変更で状態を書く。既存のテストは保存先なしで今まで
  どおり動く。凍結は最初の `api/review` の後に裏で走らせ、起動時間の条件
  （R-SERVE / R-UNIT）を変えない。
- **復元の結果ファイルは、記録した元ワークスペースの識別で置く。** 復元した
  ディレクトリの識別は使わない。
- **スキルと README は同じ流れで直す。** R-DIST が契約を変えるコミットでの
  同時更新を要求する。

## Scope of change

- `crates/kemi-core/src/` の新しいセッションモジュール（形式、保存、掃除、ロック、
  ULID、gzip）とそのテスト
- `crates/kemi-core/src/source/` の凍結ソース（`mod.rs` への登録を含む）
- `crates/kemi-core/Cargo.toml`（`miniz_oxide`）
- `crates/kemi-server/src/lib.rs`、`api.rs`、`units.rs`、`session.rs`
- `src/main.rs`、`src/result.rs`、`src/` の新しい再開モジュール
- `Cargo.toml`（`inquire`）
- `crates/kemi-server/tests/server.rs`、`tests/e2e.rs`、core のテスト
- `README.md`、`skills/kemi/SKILL.md`、`skills/kemi/references/*.md`

これ以外は触らない。`web/` は変更しない（監視を止めるのはサーバ側で完結する）。

## Step order and prerequisites

Step 1 →（Step 2 と Step 3。どちらも Step 1 だけが前提で、互いに独立なので順序は
入れ替えてよい）→ Step 4（Step 1 と Step 3 が前提）→ Step 5（Step 2・3・4 が
前提）。Step 4 → Step 6、Step 5 → Step 7。Step 1 の形式がすべての前提。

## Verification map

| 仕様の見出し | 確かめる Step |
|---|---|
| `R-SESSION セッションの保存と復元` | Step 1〜6（形式、凍結、保留、復元、選択、掃除、ロック） |
| `R-INPUT-6 その他の入出力` | Step 4（排他と使い方の誤り）、Step 6（一覧） |
| `R-SERVE 配信モデル` | Step 3（起動を待たせない）、Step 5（復元の配信） |
| `R-SUBMIT 送信と契約` | Step 3（submit の順序と削除）、Step 4（130 と案内） |
| `R-RESULT 結果ファイル` | Step 1（置き場所と権限）、Step 5（元ワークスペースへ置く） |
| `R-LIVE ライブリロード` | Step 5（監視をしない） |
| `R-VIEW 差分の表示` | Step 5（折りたたみの復元） |
| `R-SEEN 見たと進捗` | Step 5（復元で見たが戻る） |
| `R-COMMENT コメントと suggestion` | Step 3（変更ごとの保存）、Step 5（復元で戻る） |
| `R-UNIT グループ単位の切り替え` | Step 2（両単位）、Step 3（凍結は両単位が揃って完成） |
| `R-ORIGIN 由来` | Step 2（再計算）、Step 5（リポジトリが無いときの表示） |
| `R-DIST 名称と配布` | Step 7（スキルと README）、Step 4〜6（CLI 出力は英語） |
| `R-DEPS 依存` | Step 1（`miniz_oxide`）、Step 6（`inquire`） |
| `R-VERIFY 検証` | 各 Step の Shown by。規模は Step 3 |

## Left to the implementer

- 新しいモジュール・型・フィールドの名前、DTO の形（D12）。
- 内容の束ね方（長さ前置の連結など）と gzip の入れ方、圧縮の単位（D12）。
- ロックファイルの名前と持ち方、形式の版の持ち方（D12、D13）。
- 凍結タスクの起動の仕方と、失敗したときの再試行の回数。仕様が決めているのは
  「未完成の写しは復元に使わない」という結果だけなので、試行の回数は実装が決める。
- 選択画面の見た目（色、幅、スクロール）と ULID の生成実装（D14）。
- スキルと README の見出し構成と言い回し（D10）。
- セッションの保存先を組み立てるヘルパの置き場（`src/result.rs` に寄せるか、
  新しいモジュールに出すか。どちらでも置き場所と権限の結果は同じ）。

## Stop conditions

- 由来の再計算に必要なファイルごとの old/new のパスが、凍結したメタデータから
  導けないと分かったとき。由来のための文脈を凍結する迂回は、`R-SESSION` の
  「由来は記録した範囲の sha から計算する」に反するので、人に相談する。
- `File::try_lock` が対象の 6 ターゲットで使えない、または Windows で挙動が
  揃わないと分かったとき。
- SIGTERM の扱いが対象 OS で揃わず、保留の契約（130 と案内）を同じ形にできない
  と分かったとき。
- 凍結が起動時間（R-SERVE）や `busy` の条件（R-UNIT）を超えると計測が示したとき。
- 既存の e2e / server テストのヘルパの変更が、この計画のファイル範囲の外へ
  広がるとき。
- スキルと README に、契約と食い違わない書き方が見つからないとき。

## Out of scope

- バージョンと `CHANGELOG.md`（リリース時にまとめる）。
- 内容を欠いたセッションの復元（`P14`）。
- 由来そのものの凍結（`R-SESSION` は復元時に元のリポジトリから計算すると定める）。
- `web/` の変更。
- セッションの手動編集、エクスポート、別マシンへの持ち出し。

---

## Step 1 — セッションの形式と保存の土台

Purpose: セッションをディスクに保存し、読み、掃除し、ロックする土台を作る。
Specification: `docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-RESULT 結果ファイル`（置き場所と権限の決め方）、
`docs/spec/kemi.md#R-DEPS 依存`。
Prerequisites: なし。
May change: `crates/kemi-core/src/` の新しいセッションモジュール、
`crates/kemi-core/Cargo.toml`、core のテスト。
Done when: セッションの情報・状態・凍結したレビューと内容を 1 つのセッションとして
保存し、そのまま読み戻せる。内容は非 UTF-8 のバイト列を含めて往復し、保存された
内容は gzip として展開できる。書き込みは一時ファイルから rename する原子的な
差し替えで、途中のファイルがセッションとして読まれない。unix でファイル `0600`・
ディレクトリ `0700`。id は 26 文字の ULID で、同じ時刻でも衝突しない。全ワーク
スペース合計で 100 件または 500 MB を超えた分を最終更新の古い順に消し、ロック中
のセッションは消さず合計にも数えない。復元できないセッションも数える。写しは
圧縮後のバイト数で 1 セッション 20 MB を超えたら破棄して「復元できない」印を
付ける。形式の版を持ち、読めない版は専用のエラーになって、復元では理由付きの
終了コード 2 に使える。保存すべき状態が空で写しも使えないセッションは残さない。
ロックは `File::try_lock` で取り、取れないときは「使用中」と分かるエラーになる。
Shown by: test — core のユニットテストを RED から始める。往復（非 UTF-8 を含む）と
gzip としての展開、20 MB 超過で写しが破棄されること、読めない版の拒否、空の
状態のセッションを残さないこと、原子的な差し替え後に壊れたファイルが読まれない
こと、掃除の 2 つの上限とロック中除外、権限、ULID の 26 文字と同一ミリ秒での
一意性。
Left to the implementer: モジュール名、DTO の形、内容の束ね方、圧縮の単位、
ロックファイルの名前、ULID の生成（D12、D13、D14）。保存先のディレクトリは
呼び出し側から渡され、場所の解決は Step 4 で行う。
Stop and hand back if: 非 UTF-8 を含む内容の往復表現が決まらない。

## Step 2 — 凍結したレビューを配るソース

Purpose: 凍結したデータだけで、両方のグループ単位のメタデータと内容を配る。
由来だけは元のリポジトリで計算する。
Specification: `docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-UNIT グループ単位の切り替え`、
`docs/spec/kemi.md#R-ORIGIN 由来`。
Prerequisites: Step 1。
May change: `crates/kemi-core/src/source/` の凍結ソースと `mod.rs`、core のテスト。
Done when: 1 つの git フィクスチャについて、元の git ソースと凍結ソースが、両方の
グループ単位で同じメタデータ（グループ、ファイル、focus、approval、題）と同じ
内容のバイト列を返す。`watch_paths` は空。由来は、リポジトリがあるときに元の
ソースと同じ結果になり、リポジトリを消すと `None` になる（画面では
「特定できない」）。
Shown by: test — 一時リポジトリのフィクスチャで、`review` / `review_unit` の
メタデータ比較、`content` のバイト比較、`origin` の一致と、リポジトリを消した
後の `None`。
Left to the implementer: 凍結ソースの内部表現、merge-base を記録するか元の
リポジトリから求めるか（D12）。
Stop and hand back if: なし（全体の Stop conditions に従う）。

## Step 3 — サーバの保存と凍結、submit での削除

Purpose: レビュー中の状態をセッションに書き続け、最初の応答の後に凍結し、
submit でセッションを消す。
Specification: `docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-SERVE 配信モデル`、
`docs/spec/kemi.md#R-SUBMIT 送信と契約`、
`docs/spec/kemi.md#R-UNIT グループ単位の切り替え`。
Prerequisites: Step 1。
May change: `crates/kemi-server/src/lib.rs`、`api.rs`、`units.rs`、`session.rs`、
`src/main.rs` の配線、`crates/kemi-server/tests/server.rs`。
Done when: コメントの追加・編集・削除・返信・解決と、見た・折りたたみの変更が
保存先に届く。保存先が無いときは今までどおり動く。保存が失敗しても警告だけで、
レビューは終わらず終了コードも変わらない。凍結は最初の `api/review` の後に裏で
始まり、両方のグループ単位のメタデータと内容がそろったときだけ「復元できる」印が
付く。写しが 1 セッション 20 MB を超えたら写しを捨てて復元不可にする。凍結が
起動を待たせない。submit は結果ファイルを書いてからセッションを消し、その後に
停止する（結果の保存に失敗してもセッションは消す）。
Shown by: test — `FakeSink` を注入した server テストで、変更ごとの保存、失敗する
保存先でも続行して警告が出ること、凍結の完了と印、20 MB 超過での復元不可、submit
での削除、保存先なしでの従来どおりの応答。あわせて `scripts/measure-startup.sh` が
1 秒未満、`scripts/measure-range.sh ... busy` が 100 ms 未満のままであることを
回して確かめる。
Left to the implementer: 保存先のトレイトの形、凍結タスクの起動の仕方、失敗時の
再試行、両方の単位がそろうのを待つ方法。
Stop and hand back if: 両方の単位がそろわないまま凍結を完了できないとき
（R-UNIT の再試行と噛み合わない場合）の扱いが決まらない。

## Step 4 — `--resume` の受理、保留、非端末の一覧

Purpose: CLI の表面を仕様に合わせ、終了時にセッションを残して案内する。
Specification: `docs/spec/kemi.md#R-INPUT-6 その他の入出力`、
`docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-SUBMIT 送信と契約`、
`docs/spec/kemi.md#R-RESULT 結果ファイル`、
`docs/spec/kemi.md#R-DIST 名称と配布`。
Prerequisites: Step 1、Step 3。
May change: `src/main.rs`、`src/result.rs`、`src/` の新しい再開モジュール、
`tests/e2e.rs`。
Done when: `--resume` が入力モード・`--digest`・`--group-by`・`--base`・
`--focus`・`--result` 系と排他で、違反は終了コード 2。通常の起動でセッションが
でき、ロック中のセッションの `--resume <id>` は理由を出して終了コード 2。
Ctrl+C と SIGTERM は、stdout に何も出さず、セッションを残し、復元できるときは
`kemi: resume with: kemi --resume <id>` を stderr に出して終了コード 130。
実行時のエラーでも、復元できるセッションが残るなら同じ行を出す。`id` なしで
端末でないときは、復元できるセッションがあるときに 5 列のタブ区切りの一覧を
stdout に出して終了コード 0、無ければ stdout 空で終了コード 2。一覧の値に
タブや改行があれば空白に置き換わり、並びは最終更新の新しい順。復元できない
セッション（未完成・20 MB 超過・読めない版）の id 指定は理由付きの 2 で、
一覧にも出ない。`--digest` と `--result` はセッションを作らない。
Shown by: test — `tests/e2e.rs` に、排他の 2、SIGINT と SIGTERM の 130 と stderr の
行とセッションの残り、不明な id・復元不可の id の 2 と一覧に出ないこと、起動中の
`--resume` の 2、非端末の一覧（5 列、タブと改行の置換、並び、0 件の 2、id なしで
`--port` / `--bind` / `--no-open` を付けても 2 にならないこと）、`--digest` と
`--result` の前後で `sessions/` が変わらないこと、空の状態で写しも無いまま終了
したセッションが残らないこと。実行時エラーの案内は、復元できるセッションがある
ときに stderr の行を組み立てる関数のユニットテストで確かめる（プロセス全体で
強制するのは難しいため）。
Left to the implementer: フラグの持ち方、エラーメッセージの文言（D1）、一覧の
組み立ての置き場。
Stop and hand back if: なし（全体の Stop conditions に従う）。

## Step 5 — 復元の実行

Purpose: `--resume <id>` で、凍結した差分と復元した状態のレビューを始める。
Specification: `docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-INPUT-6 その他の入出力`、
`docs/spec/kemi.md#R-RESULT 結果ファイル`、
`docs/spec/kemi.md#R-LIVE ライブリロード`、
`docs/spec/kemi.md#R-VIEW 差分の表示`。
Prerequisites: Step 2、Step 3、Step 4。
May change: `src/main.rs`、`src/` の再開モジュール、core と server の小改修、
`tests/e2e.rs`。
Done when: 保留の間にワークツリーを変えてから復元しても、ファイル一覧と内容は
開始時のまま。見た・折りたたみ・コメント・解決が戻る。復元は同じ id の続きで、
セッションは増えず、状態は同じ id に書き戻る。監視と更新バッジが無い。元の
ワークスペースのリポジトリを消しても開けて、由来だけ「特定できない」になる。
URL と token は新しく作る。submit の結果ファイルは記録した元ワークスペースの
識別で置かれ、manifest の `approval` は元の入力のまま返る。
Shown by: test — `tests/e2e.rs` で、worktree を変えてからの復元、コメントと見たの
維持、復元の前後でセッションが増えないこと、元リポジトリを消しての由来、復元
からの submit の結果ファイルの場所、approval 付き manifest の approval の往復。
ブラウザ自動化で、復元したページに更新バッジが出ず、元の入力が変わっても内容と
コメントが変わらないことも見る。
Left to the implementer: 復元時の stderr の案内の順序、状態の読み込みの詳細、
`--serve` を受け流す扱い。
Stop and hand back if: 記録した元ワークスペースのパスから `R-RESULT` の置き場所が
導けないと分かったとき。

## Step 6 — 端末の選択画面

Purpose: `id` なしで端末から起動したときに、セッションを選べるようにする。
Specification: `docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-DEPS 依存`、`docs/spec/kemi.md#R-DIST 名称と配布`。
Prerequisites: Step 4。
May change: `src/main.rs`、`src/` の再開モジュール、`Cargo.toml`、
`tests/e2e.rs`。
Done when: 端末では選択画面が出て、復元できるセッションが最終更新の新しい順に
並ぶ。列は id、最終更新（ローカル `YYYY-MM-DD HH:MM`）、ワークスペース、モード、
見た数 / 全数で、モードの出し方は仕様どおり。Esc と Ctrl+C では何も変えず
終了コード 130。端末でないときの一覧は Step 4 のまま。文言は英語。
Shown by: external — 人が端末で選択画面を開き、列、並び、取り消しの 130 を
確かめる。端末でない経路は Step 4 のテストがそのまま回帰を守る。
Left to the implementer: 見た目（色、幅、スクロール）と選択の操作（D14）。
Stop and hand back if: `inquire` が対象の 6 ターゲットでビルドできない。

## Step 7 — スキルと README の追随

Purpose: エージェント向けの文書と README を新しい契約に合わせる。
Specification: `docs/spec/kemi.md#R-DIST 名称と配布`、
`docs/spec/kemi.md#R-SESSION セッションの保存と復元`、
`docs/spec/kemi.md#R-SUBMIT 送信と契約`。
Prerequisites: Step 4、Step 5。
May change: `README.md`、`skills/kemi/SKILL.md`、
`skills/kemi/references/*.md`。
Done when: 終了コードの表で `130` が保留（結果は無いがセッションが残り、
`--resume` の案内に従える）と説明され、`--resume` の使い方と、id なし・端末
でないときの一覧が載る。フラグの説明に `--resume` がある。frontmatter は 3 項目
のまま ASCII だけで、本文と README は英語。
Shown by: check — `agentskills validate ./skills/kemi`、
`PROJECT.md` にある frontmatter の非 ASCII と項目数の検査。本文と README が英語で
あることは `R-DIST` の成功条件として人が確認する。
Left to the implementer: 見出しの構成と言い回し（D10）。
Stop and hand back if: なし（全体の Stop conditions に従う）。
