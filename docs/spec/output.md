# kemi 仕様: エージェントに返す出力

[kemi 仕様](kemi.md) の一部。エージェントに返すもの。submit の JSON の契約、結果ファイル、digest。
用語の定義は `CONTEXT.md`、成功条件と反例の書き方は [kemi 仕様](kemi.md) の冒頭にある。

---

## R-SUBMIT 送信と契約

新しく作るresultとwaitの `kemi` は4（結果JSONの契約の版4）。版4の形と追加キー、説明の `versions` の規則は [R-EXPLANATION-RESULT](explanation-contracts.md#r-explanation-result-resultとwaitの版4) を適用する。以下の例と規約は版2の時点の形で、R-EXPLANATION-RESULTがこれに優先する。

**要件**

- ページには "Approve" / "Request changes" の 2 つの送信ボタンと、任意のコメントがある。
  送信の前に、取り消せないことと verdict を確認するダイアログを挟む。確定した
  ときだけレビューを終了し、サーバは応答を返した後に停止して CLI が終わる。
  キャンセルでは何もしない。
- 確認ダイアログには、送るコメントの件数（うち suggestion 付きの件数、両方の
  グループ単位の合計）と、表示中のグループ単位の見たファイル数 / 全数を出す。
  見ていないファイルがあるときは、その数を注意として出す。一度でも `kemi wait` が呼ばれた
  レビューでは、エージェントにまだ渡していないコメント・返信・発言の件数も出す
  （[agent-channel.md](agent-channel.md#r-agent-hand-渡す)）。
- 送信後の完了画面には、結果の JSON と、それをクリップボードへ写す操作、結果ファイル
  の保存先（`R-RESULT`）を出す。エージェントが反応しないときは、この JSON を会話へ
  貼れば済むと示す。承認のときは「閲」の印を出す。
- 送信後は承認と変更要求のボタンを無効にする。
- 同時に 2 つの submit が届いた場合、受理するのは 1 つだけとし、もう片方には
  409 を返す。受理して停止した後の再送は接続が拒否されてよい。
- CLI は終了時に stdout へ JSON 文書を 1 つだけ出力する。ログと URL は stderr に
  出す。stdout にそれ以外を混ぜない。同じ JSON を結果ファイルにも書く（`R-RESULT`）。
  セッションを消すのは submit を確定したとき（`R-SESSION`）。
- 終了コード: `0` 承認 / `1` 変更要求 / `2` 起動・使い方・実行時のエラー /
  `130` 利用者による中止（submit なしの Ctrl+C / SIGTERM の保留と、選択の取り消し。
  `R-SESSION`）。保留では stdout に何も出さず、セッションを残す。復元できる
  セッションが残るときは、保留でも実行時エラーでも、復元コマンドの 1 行を stderr に
  出す（`R-SESSION`）。レビュー中の git や I/O の失敗も `2` とし、stdout に JSON を
  出さない（このときもセッションは残る）。ただし、裏でのグループ単位の作成の失敗
  （`R-UNIT`）、由来の計算の失敗（`R-ORIGIN`）、結果ファイルの保存の失敗（`R-RESULT`）
  はレビューを終えず、終了コードも変えない。
- `kemi` が stdout に書けないとき（読み手が先に閉じたパイプなど）は、submit の JSON・
  `--result`・`--digest`・`--version`・復元の一覧のどれでも、`kemi: cannot write to stdout: <理由>`
  を stderr に出して終了コード `2` で終わる。submit の JSON は stdout より先に結果ファイルへ
  書いている（`R-SESSION`）ので、書けていれば `kemi --result` で読める。
- 出力する JSON の契約:

```json
{
  "kemi": 2,
  "title": "…",
  "verdict": "approved" | "changes_requested",
  "approval": [{ "path": "…", "identity": "sha256:…" }],
  "comments": [
    {
      "id": "c1",
      "group_id": "g1",
      "group_title": "…",
      "path": "src/a.rs",
      "side": "new" | "old",
      "start_line": 12,
      "end_line": 14,
      "quote": ["コメント時の行テキスト", "…"],
      "body": "本文",
      "page": null,
      "replies": [
        {
          "id": "r1",
          "author": "agent" | "reviewer",
          "body": "本文",
          "variants": [{ "id": "A", "label": "案 A" }],
          "chosen": "A",
          "applied": { "reply_id": "r1", "variant_id": "A" }
        }
      ],
      "resolved": false,
      "outdated": false,
      "suggestion": { "replacement": "置換後の全文" }
    }
  ],
  "messages": [{ "id": "m1", "author": "agent" | "reviewer", "body": "本文" }]
}
```

- フィールドの規約:
  - `side` は `"new"` か `"old"`。ファイル全体のコメントは `"new"`。
  - `start_line` / `end_line` は 1 始まり、両端を含む。ファイル全体のコメントは
    `null`。
  - `quote` は `side` 側の対象行のテキスト（ファイル全体のコメントは `[]`）。
  - `suggestion` は無いとき `null`。旧側とファイル全体では常に `null`。
  - `replies` と `resolved` は全コメントに必ずある（無い場合は `[]` / `false`）。`replies` は
    作成順。`author` は書いた人で、人間は `"reviewer"`、エージェントは `"agent"`。
    `variants`（案の `id` と `label` だけ。中身は入れない）、`chosen`（「これにする」で
    選んだ案の `id`）、`applied`（エージェントが反映を知らせた案）は、無いとき `[]` /
    `null` / `null`（[live.md](live.md#r-page-variant-案)）。
  - `page` は `--live` のページへのコメントだけが持ち、それ以外は `null`。ページへの
    コメントは `path` / `side` / `start_line` / `end_line` が `null`、`quote` が `[]`、
    `suggestion` が `null` で、`page` は次の形:
    `{ "url": "/products?x=1", "width": 390, "places": [{ "n": 1, "kind": "element" | "arrow" | "pen",
    "points": [{ "x": 0, "y": 0 }], "elements": [{ "selector": "…", "text": "…",
    "rect": { "x": 0, "y": 0, "w": 0, "h": 0 } }] }], "image": "/絶対パス.png" | null }`。
    `places` は 1 つ以上で番号 `n` の順。番号は 1 からの連番（書いている途中で消した場所は
    詰め直す。[live.md](live.md#r-page-comment-コメントの場所)）。`points` は矢印とペンの点
    （要素では `[]`）、座標はページの CSS ピクセル。結果 JSON の契約の版 4 からは、矢印もペンも、整数に
    丸めて許容 1 CSS ピクセルで間引いた点で（矢印も始点と先端の 2 点に限らない）、座標は整数
    （[live.md](live.md#r-page-comment-コメントの場所)）。
    それより前の kemi で置いて保存した場所の点は、保存したときのまま出す。`elements` は要素では 1 つ、矢印では 0 か 1 つ、
    ペンでは 0〜5 個で、囲んだ中に意味のある要素が無いペンと先が余白の矢印では `[]`
    （[live.md](live.md#r-page-comment-コメントの場所)）。場所ごとの画像の項目は無い。本文
    （`body`）の中の `#n` は、同じコメントの場所 `n` を指す（`#` の付かない数字は参照ではない）。
    場所を詰め直したときは本文の `#n` も書き換えてあるので、`#n` と `n` は食い違わない
    （[live.md](live.md#r-page-comment-コメントの場所)）。この 3 つ（連番、空の `elements`、本文の
    `#n`）は、ライブ改訂の使い勝手の見直しで決めた契約の変更。そのとき契約の版 `2` はまだリリース
    していなかった（CHANGELOG の Unreleased）ので、版を上げず、版 `2` の定義そのものを直した。実装した
    コミットで `skills/kemi/SKILL.md`（`references/results.md` の「numbers may skip」を含む）、README、
    CHANGELOG も同時に直した（[R-DIST](distribution.md#r-dist-名称と配布)）。`image` は
    描き込みを重ねた画像（[live.md](live.md#r-page-comment-コメントの場所)）。submit の
    確定でセッションと一緒に消えるので、submit の後は `null` にする。`kemi wait` の応答でも、画像を
    作れなかったときと、セッションの上限で保存しなかったとき
    （[live.md](live.md#r-page-session-保留と復元)）は `null`。
  - `messages` はレビュー全体への発言で、作成順。無い場合は `[]`。
  - ページへのコメントの `outdated` は常に `false`（内容ハッシュを持たない）。
  - `approval` は入力をそのまま返す。無い場合は `[]`。
  - `group_id` は `R-INPUT` の表の値。`group_title` はそのグループの `title`。
  - `comments` は作成順。`kemi` は結果 JSON の契約の版（数値）。ライブ改訂で `2` に上げた
    （`replies` に中身が入り、`page` と `messages` が増えた）。説明の改訂で `3`、説明へのコメントが
    無い結果の `explanation.versions` を空にする改訂で `4` に上げた
    （[R-EXPLANATION-RESULT](explanation-contracts.md#r-explanation-result-resultとwaitの版4)）。新しく作る
    結果は `4`。読む側は契約の版を見て、知らない契約の版を推測で読まない。

**成功条件**

- 承認で終了コード 0、変更要求で 1 になり、stdout が 1 つの JSON として
  パースできる。
- コメント本文に改行や引用符があっても JSON が壊れない。
- 保留（Ctrl+C）では JSON が出ず、終了コード 130。セッションが残り、stderr の
  「resume with」の行のコマンドで同じレビューを続けられる（`R-SESSION`）。
- ファイル全体へのコメントが `start_line: null` / `end_line: null` /
  `quote: []` で出る。
- エージェントの返信と人間の返信が、`author` の違う 2 件として作成順に `replies` に入る。
  エージェントの発言が `messages` に入る（e2e）。
- 送信後、承認と変更要求のボタンが押せない（ブラウザ自動化で確認）。
- 完了画面の JSON が stdout の JSON と同じ内容になる（人が確認）。
- 読み手を閉じたパイプを stdout にして submit・`kemi --result`・`kemi --digest`・`kemi --version`・
  端末でない `kemi --resume`（一覧）を実行すると、どれも終了コード 2 で、stderr に
  `kemi: cannot write to stdout:` の行が出る（e2e）。

**反例**

- 進捗ログが stdout に混ざり、JSON のパースに失敗する。
- stdout に書けないと panic し、契約に無い終了コード（101）で終わる。
- 同時 submit の両方が受理され、どちらの verdict か分からなくなる。
- 送信後もボタンが押せ、確認ダイアログの後に何も起きない。
- Ctrl+C でレビュー中のコメントが失われる。

---

## R-RESULT 結果ファイル

新結果と過去結果の区別は [R-EXPLANATION-RESULT](explanation-contracts.md#r-explanation-result-resultとwaitの版4) を適用する。

submit の結果を、エージェントが受け取り損ねても後から読めるように残す。

**要件**

- submit を確定したとき、stdout に出す JSON と同じ内容を結果ファイルに書く。verdict は
  問わない。保留（終了コード 130）とエラー（終了コード 2）では書かない。
- 置き場所は `$XDG_STATE_HOME/kemi/results/`。`XDG_STATE_HOME` が未設定か相対パスなら
  unix では `~/.local/state/kemi/results/`、Windows では `%LOCALAPPDATA%\kemi\results\`。
  `LOCALAPPDATA` も相対パスなら未設定と同じ扱いにする。Windows で `LOCALAPPDATA`
  も未設定なら決められず、unix で HOME が無い場合と同じ扱い
  （起動時に決められないエラー）にする。1 回の submit で 1 ファイルを作り、名前に送信時刻を
  含める。同じ時刻に送信したセッションがあっても名前は衝突しない。「最新」と「古い」は
  送信時刻（ミリ秒）で決める。ファイルとディレクトリの権限は unix のみ定める:
  ファイルは所有者だけが読み書きでき（`0600`）、ディレクトリは所有者だけが使える
  （`0700`）。Windows では `%LOCALAPPDATA%` の既定の ACL に任せ、特別な ACL 設定は
  行わない。
- 結果ファイルは、どのリポジトリのレビューだったかを識別できるように置く。識別の基準は、
  kemi を起動したディレクトリを含む git リポジトリのトップのディレクトリ。git の外なら
  起動したディレクトリそのもの。識別の情報はファイルの中身（JSON）に足さず、置き場所
  か名前で持つ（方法は D9）。
- 結果ファイルは全リポジトリ合計で新しい方から 20 件を残し、書くときにそれより古い
  ものを消す。
- 保存に失敗しても、stderr に警告を出し、完了画面に英語の保存失敗メッセージを出す
  だけで、stdout の JSON と終了コードは変えない。
- 保存先のディレクトリを、サーブ開始時に stderr へ出し、完了画面にも出す（文言は
  契約としない。`kemi: <url>` の行とは別の行にする）。
- `kemi --result` は、今いるリポジトリ（識別の基準は上と同じ。`--workspace <path>`
  で場所を指定できる）の最新の結果ファイルの中身を stdout に出して終わる。終了コードは
  元の verdict と同じ（承認 0 / 変更要求 1）。該当する結果が無ければ、stdout に何も
  出さず終了コード 2。`--any` を付けると場所に関係なく最新の結果を出す。結果ファイルの中身は
  書き換えずに出す（ライブ改訂の前に書いた結果 JSON の契約の版 1 の結果も、版 4 より前の版 2・3 の結果もそのまま）。

**成功条件**

- `XDG_STATE_HOME` を一時ディレクトリにして submit すると、stdout と同じ JSON の
  ファイルが unix では `0600` でできる。続けて同じディレクトリで `kemi --result` を実行すると、
  同じ JSON が出て、終了コードが元の verdict と一致する。
- Windows で `XDG_STATE_HOME` を未設定のまま `LOCALAPPDATA` を一時ディレクトリにして
  submit すると、その下の `kemi\results\` に同じ JSON のファイルができる。
- 21 回 submit すると、結果ファイルは 20 件で、最初の 1 件が消えている。
- リポジトリ A で submit した後、リポジトリ B で `kemi --result` を実行すると終了コード
  2 で何も出ず、`--any` を付けると A の結果が出る。
- 結果の置き場所に書けない状態で submit しても、stdout の JSON と終了コードが
  書ける場合と同じになる。

**反例**

- 別のリポジトリの結果を、今のリポジトリの結果として返す。
- 保存の失敗でレビューが終了コード 2 になり、stdout に JSON が出ない。
- 結果ファイルが消されずに増え続ける。
- 他の利用者が結果ファイルを読める権限で作られる（unix の権限条件。Windows では
  `%LOCALAPPDATA%` の既定 ACL に任せるため、この反例は適用しない）。

---

## R-DIGEST digest

**要件**

- `--digest` はページを出さず、レビューの地図だけを JSON として stdout に
  1 つ出して終了する（行内容は含めない）。コミット範囲では `--group-by` の単位
  （既定 最終形。グループは 1 つ）で集計する。
- 出力の契約:

```json
{
  "kemi": 1,
  "title": "…",
  "totals": { "files": 30000, "add": 123456, "del": 7890, "noise_files": 12000 },
  "groups": [
    { "id": "…", "title": "…", "why": "…", "watch": "…",
      "files": 10, "add": 100, "del": 20 }
  ],
  "directories": [
    { "path": "src", "files": 100, "add": 1000, "del": 200, "noise_files": 3 }
  ],
  "top_files": [
    { "path": "src/a.rs", "status": "modify", "add": 100, "del": 20,
      "noise": false, "focus": true, "note": "…" }
  ],
  "top_files_omitted": { "files": 0, "add": 0, "del": 0 },
  "top_n": 100
}
```

- `directories` はルート直下のディレクトリ単位で集約する。直下のファイルは
  `path` を `"."` とする 1 項目にまとめる。
- `top_files` は増減の合計（`add + del`）の降順、同数はパス昇順。件数は
  `--digest-top`（既定 100）を要求値とする。ただし出力全体の予算 100 KB に
  収まらない場合は、載せられなかった分を `top_files_omitted`（件数と増減の
  合計）にまとめる。切り捨てが無いときも 0 件で存在する。
- 文字列は 2,000 文字を上限とし、切った場合は末尾に `…` を付ける。
- 行内容や `quote` は含めない。30,000 ファイルの入力でも出力が 100 KB 未満に
  収まる。

**成功条件**

- 30,000 ファイルのフィクスチャで `--digest` の出力が 100 KB 未満で、
  `top_files` が増減合計の降順で入り、`top_files` と `top_files_omitted` の
  件数の合計が対象ファイル数と一致する（切り捨てが無ければ omitted は 0 件）。
- エージェントがこの JSON だけで「どこを読むべきか」を選べる（人が確認）。

**反例**

- `--digest` が全ファイルの一覧をそのまま出し、30,000 ファイルで数 MB になる。
- 行内容や `quote` が混ざる。
- 切り詰めた `why` に印が無く、完全な文と区別できない。

---

## 作らないもの

- P13 レビューの read-only モードと、アクセス元（ループバックかどうか）で submit や
  表示を変える機能。公開中は、URL を知る人は誰でも同じ操作ができる。これはページの操作の
  話で、エージェント用の API をループバックに限ること（[agent-channel.md](agent-channel.md#r-agent-link-つなぎ先と安全)）とは別。

## 委譲

- D4 submit JSON の残余の細部（`id` の形式、ライブ表示の文言）。骨格は
  `R-SUBMIT` が契約。
- D9 結果ファイルの名前の形式と、リポジトリの識別を置き場所と名前のどちらでどう持つか。
  どの方法でも `R-RESULT` の選び方（送信時刻で決める最新、リポジトリの識別の基準）と
  中身は同じ。

## 却下

- R14 digest だけコミットごとを既定に残す（理由: 同じ引数で画面と digest の単位が
  食い違う）。
- R15 結果ファイルを消さずに増やす、または 1 件だけ残す（理由: 前者は際限なく増え、
  後者は同時に動かしたセッションの結果を上書きする）。
- R16 結果の読み出しをサブコマンド `kemi result` にする（理由: 位置引数の manifest
  パスと区別できない）。
- R18 submit を止める read-only モード（理由: 公開は同僚に kemi を体験してもらうのが
  目的で、押せない UI は要らない。最初の submit でレビューが終わるのは既存の契約の
  まま）。
- R19 アクセス元で submit の可否を変える（理由: ループバック限定は公開の目的に反し、
  判定の基準も増える）。
