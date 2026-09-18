# LAN 公開の `--bind` を実装する

## Goal

`kemi --bind <IPv4>` で待ち受けアドレスを変えられるようにする。`0.0.0.0` のときは
LAN の他端末から開ける URL を 1 行出し、公開中であることを警告する。既定は今まで
どおり `127.0.0.1` で、既定の振る舞いは変えない。

## Specification

`docs/spec/kemi.md`（v0.1.6 時点の改訂 `cea6c38`）。見出しは
`R-INPUT-6 その他の入出力`、`R-SERVE 配信モデル`、`R-DIST 名称と配布`、`R-VERIFY 検証`。

## Approach and why

- **値の検証は引数のパース時に行う。** `Ipv4Addr` へのパースでホスト名と IPv6 の拒否
  （R-INPUT-6）が自然に得られ、`--digest` のようにサーブしない経路でも検査が抜けない。
- **共有アドレスの特定は起動側（`src/main.rs`）で行い、結果を `ServeParams` で
  サーバへ渡す。** 仕様の「テストから差し替えられる形」はこれで満たす。`serve()` の
  中で環境を読むと、URL と Host の許可集合が環境依存になり、テストが成立しない。
  特定の失敗（既定経路が無い）は `None` を渡すことで再現できる。
- **警告の文面は bind と `Option<共有アドレス>` を受ける純粋関数が組み立て、
  `src/main.rs` が URL 行の後に出す。** 固定文と `--bind` 案内の 2 分岐はこの関数の
  テストで固定し、e2e は配線（`0.0.0.0` で接頭辞が出る、既定で出ない）を見る。
  警告の条件は「バインド先がループバック（`127.0.0.0/8`）でないとき」で、`0.0.0.0`
  だけでなく具体アドレス（例 `192.168.1.5`）も含む（R-SERVE）。
- **Host の許可集合は `serve()` が listener のアドレスと渡された共有アドレスから
  組み立てる。** 許可するのは `127.0.0.0/8`、バインドした具体アドレス、共有アドレス
  （R-SERVE）。`0.0.0.0` の listener アドレスは wildcard なので許可集合に入れない。
  列挙の依存は足さない。
- **URL の行を先に、警告行には URL を入れない。** 計測スクリプトが stderr の最初の
  `http://` を拾うため。
- **ブラウザで開く URL** は、`0.0.0.0` のときだけ `127.0.0.1` に読み替える
  （R-SERVE）。具体アドレスのときは表示した URL をそのまま開く。
- **テストは既存の型を踏襲する。** CLI は `tests/e2e.rs` のバイナリ起動、サーバは
  `crates/kemi-server/tests/server.rs` の `serve()` 直呼びと raw TCP の Host 検査。
- **Step 1〜4 の成果は 1 コミットにまとめる。** R-DIST が、CLI と JSON の契約を変える
  コミットで README とスキルも同時に直すことを要求するため、公開契約を変えた中間
  コミットを作らない。

## Scope of change

- `src/main.rs`
- `crates/kemi-server/src/lib.rs`
- `crates/kemi-server/src/api.rs`
- `crates/kemi-server` 内の新しい小さなモジュール（アドレス特定と警告の文面。置き場は
  実装が決める）
- `tests/e2e.rs`
- `crates/kemi-server/tests/server.rs`
- `README.md`
- `skills/kemi/SKILL.md`、`skills/kemi/references/inputs.md`

これ以外のファイルは触らない。

## Step order and prerequisites

Step 1 → Step 2 → Step 3 → Step 4 → Step 5 の順。Step 2 は Step 1 のフラグに依存し、
Step 3 は Step 2 の `ServeParams` と URL 生成に依存する。Step 4 は Step 3 までで
固まった CLI の表面に依存する。Step 5 は全部に依存する。Step 1〜4 は 1 コミットに
まとめる。

## Verification map

| 仕様の見出し | 確かめる Step |
|---|---|
| `R-INPUT-6 その他の入出力` | Step 1 |
| `R-SERVE 配信モデル`（バインド・URL・警告・Host/Origin・複数端末） | Step 2、Step 3 |
| `R-DIST 名称と配布` | Step 4、Step 1（日本語の混入なし） |
| `R-VERIFY 検証` | Step 4、Step 5 |

## Left to the implementer

- 内部の名前、ヘルパの置き場、`ServeParams` のフィールド名。
- 共有アドレス特定の UDP connect の宛先（既定経路を選べる任意のアドレス。送信は
  しない）。
- 特定失敗時の警告の続きの文言（接頭辞と含める内容は仕様が決めている）。
- README とスキルの見出し構成と言い回し（`R-DIST` の範囲で）。

## Stop conditions

- 既存の `tests/e2e.rs` / `server.rs` のヘルパの変更が、この計画のファイル範囲の外へ
  波及する。
- 開発機で共有アドレスの特定が `None` になり、`0.0.0.0` の e2e の成功条件を
  確かめられない（その場合はフォールバックの挙動として人に確認する）。
- `README.md` かスキルに、契約と食い違わない書き方が見つからない。
- Step 1 の `--result` との排他が、既存のフラグ収集の仕組みで成立しない。

## Test command

`R-VERIFY 検証` の表のコマンドをそのまま使う。起動時間の計測スクリプト
（`scripts/measure-startup.sh` / `scripts/measure-range.sh`）は既定バインドのままの
起動を測るもので、この変更は時間条件に触れないため対象外とする。fixture の生成
（`scripts/gen-fixture.sh`）も同様に対象外。

## Out of scope

- バージョンと `CHANGELOG.md`（リリース時にまとめる）。
- read-only モードとアクセス元での判別（`P13`、`R18`、`R19`）。
- インターフェースの全列挙（新しい依存の追加）。
- ファイアウォールや portproxy の設定（kemi は関与しない。README の説明まで）。

---

## Step 1 — `--bind` の受理と拒否

Purpose: CLI の表面を `R-INPUT-6` の契約に合わせる。
Specification: `docs/spec/kemi.md#R-INPUT-6 その他の入出力`。
Prerequisites: なし。
May change: `src/main.rs`、`tests/e2e.rs`。
Done when: `--bind` を引数のパース時に `Ipv4Addr` として検証し、既定は `127.0.0.1`。
ホスト名・IPv6・`--result` との同時指定・`--digest` との併用時の不正値はすべて
終了コード 2。新しい使い方のエラーメッセージに日本語が含まれない。既定の起動と
既存のフラグは今までどおり。
Shown by: test — `tests/e2e.rs` に、`--bind localhost`、`--bind ::1`、
`--result --bind 0.0.0.0`、`--digest --bind localhost` が終了コード 2 になるテストを
足す。既存の `contains_japanese` で新しいエラーの stderr を検査する。既存の
スイートが緑のまま。
Left to the implementer: フィールドの型と名前、使い方の文言（`D1`）。
Stop and hand back if: `--result` との排他が既存のフラグ収集では成立しない。

## Step 2 — バインド先、URL、Host/Origin の許可

Purpose: 指定アドレスで待ち受け、URL と POST の許可集合を仕様どおりにする。
Specification: `docs/spec/kemi.md#R-SERVE 配信モデル`。
Prerequisites: Step 1。
May change: `src/main.rs`、`crates/kemi-server/src/lib.rs`、
`crates/kemi-server/src/api.rs`、`crates/kemi-server/tests/server.rs`。
Done when: URL の host が、具体アドレスへの bind ではそのアドレス、`0.0.0.0` では
渡された共有アドレス A、特定できないときは `127.0.0.1` になる。`Host: A:<port>` と
対応する `Origin` の POST が受理され、それ以外のアドレスの Host は 403。wildcard の
`Host: 0.0.0.0:<port>` は 403。共有アドレスが `None` のとき、LAN 側のアドレスを
Host に持つ POST は 403。同じ URL を 2 つのクライアントで使うと、コメントと見たが
共有され、submit の JSON に著者のフィールドが無い。
Shown by: test — `crates/kemi-server/tests/server.rs` に、共有アドレスを注入した
受理・拒否（A、他のアドレス、`0.0.0.0`）、`None` のフォールバック、複数クライアント
共有のテストを足す。URL の組み立ては、ホストを引数に取る関数を具体アドレスと
`None` の両方で直接テストする。既存の `host_mismatch_rejected`・
`cross_origin_rejected_on_comment_post` は許可集合の変更に合わせて更新する。
Left to the implementer: `ServeParams` と許可集合の内部表現、`session_url` の
シグネチャ、共有アドレスの取得関数の置き場（Step 3 で使う）。
Stop and hand back if: 既存のテストヘルパの変更が `crates/kemi-server/tests/server.rs`
の外へ波及する。

## Step 3 — 共有アドレスの特定、警告、ブラウザ起動

Purpose: 非ループバックの起動で、共有可能な URL と公開の警告を出し、ブラウザを
正しく開く。
Specification: `docs/spec/kemi.md#R-SERVE 配信モデル`。
Prerequisites: Step 2。
May change: `src/main.rs`、アドレス特定と警告のモジュール、
`crates/kemi-server/src/lib.rs`（モジュールの登録）、`tests/e2e.rs`。
Done when: 警告の文面の純粋関数が、ループバックでは `None`、非ループバックで共有
アドレスがあるときは固定の 1 文、共有アドレスが `None` のときは接頭辞に続けて
取得できなかったことと `--bind` の案内を返す（どちらも公開のリスクを述べる）。
`--bind 0.0.0.0 --port 0 --no-open` の起動で、stderr の URL に `api/review` が
応答し、URL 行の後に `kemi: exposed on the LAN;` で始まる警告が出る。既定の起動には
警告が出ず、URL は `http://127.0.0.1:<port>/...` の形。`0.0.0.0` でブラウザを自動で
開くとき、開かれる URL は `127.0.0.1`。
Shown by: test — 警告の純粋関数のテスト（ループバック・具体アドレス・`0.0.0.0` で
特定成功・特定失敗の 4 ケース）と、`tests/e2e.rs` の配線テスト（`0.0.0.0` の URL
応答と接頭辞、既定の警告なし）。加えて external — 人による確認:
`--bind 0.0.0.0`、`--no-open` なし、固定ポートで起動し、開かれたブラウザの URL が
LAN アドレスではなく `127.0.0.1` であること。
Left to the implementer: UDP connect の宛先、特定失敗時の文面、ヘルパの置き場。
Stop and hand back if: 開発機で特定が `None` になり、e2e の成功条件を確かめられない。

## Step 4 — README とスキルの更新

Purpose: 配布物を新しい契約に追随させる。
Specification: `docs/spec/kemi.md#R-DIST 名称と配布`。
Prerequisites: Step 3（CLI の表面が固まっている）。
May change: `README.md`、`skills/kemi/SKILL.md`、
`skills/kemi/references/inputs.md`。
Done when: README の説明とフラグ表に `--bind` があり、公開が OS とネットワークに
依存すること（OS のファイアウォール、WSL2 では Windows 側の設定、繰り返し公開する
ときの `--port` の固定）に触れている。ループバック限定と読める既存の記述
（`README.md` の冒頭の説明、`kemi: http://127.0.0.1:...` の例、スキルの
「serves a local review page on `127.0.0.1`」）を、既定がループバックで `--bind` に
よっては公開される、という説明に直す。スキルにも `--bind` がある。どちらも英語で、
スキルの frontmatter は 3 項目のままで値は ASCII。
Shown by: check — `agentskills validate ./skills/kemi`（`R-VERIFY` の表）と、
frontmatter の項目数・ASCII の確認。external — 人が README とスキル本文の `--bind`
の説明を契約と突き合わせる。
Left to the implementer: 見出し構成と言い回し（`D10`）。
Stop and hand back if: 契約と食い違わない書き方が見つからない。

## Step 5 — 全体検証と受け入れ

Purpose: 契約のすべての成功条件を確かめる。
Specification: `docs/spec/kemi.md#R-VERIFY 検証`。
Prerequisites: Step 1〜Step 4。
May change: 変更なし（見つかった不備は該当 Step に戻って直す）。
Done when: `R-VERIFY` の表のコマンドがすべて成功し、`R-SERVE` の成功条件（警告の
接頭辞、既定で警告なし、Host/Origin の受理と拒否、複数クライアント共有）が緑。
加えて、LAN 公開の実地確認: 開発機ともう 1 台の端末で、固定ポートを指定した
`--bind 0.0.0.0` の起動を開き、2 台目のブラウザからコメントと submit が届く。
Shown by: check — `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`、
`cargo deny check licenses`、`npx tsc -p web --noEmit`、`node --test web`、
`agentskills validate ./skills/kemi` と frontmatter の ASCII 検査。external — 人が
2 台目の端末で操作し、警告どおりの公開であることと submit の JSON が返ることを
確認する。
Left to the implementer: なし。
Stop and hand back if: 2 台目の端末が無い、または OS のファイアウォールで届かず、
到達性の確認ができない（その場合は警告と README の説明までを確認し、到達性は環境
依存として人に報告する）。
