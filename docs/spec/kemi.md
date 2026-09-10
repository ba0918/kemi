# kemi 仕様

- Status: 承認済み（2026-09-10）
- 用語の定義: `CONTEXT.md`
- 対話の記録: `.agents/tmp/brainstorm-diff-viewer.md`（承認後に削除）

結果を一文で: **kemi は、変更をブラウザで快適に読ませ、人間が付けた行コメントと
suggestion を実行ターミナルへ 1 つの JSON として返す、単一バイナリのローカルレビュー
道具である。**

各要件には「成功条件」（機械または人が観測できる確認）と「反例」（その要件が破られて
いると判定できる観測）を付ける。証拠の出所は `R-VERIFY` にまとめる。

---

## R-DIST 名称と配布

**要件**

- リポジトリ名とバイナリ名は `kemi`（「閲する」から）。
- ライセンスは MIT OR Apache-2.0。
- README は英語、UI と docs（この仕様を含む）は日本語。
- 対応ターゲットは次の 4 つ:
  - `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`（静的リンク）
  - `x86_64-apple-darwin` / `aarch64-apple-darwin`
- GitHub Releases に cargo-dist 形式のアセット
  `<name>-<target>.tar.xz` と checksum（`.sha256`）を添付し、
  `mise use -g github:ba0918/kemi` でインストールできる。
- バージョンの正典はリポジトリ内の 1 箇所（ルート `Cargo.toml` の `version`）。
  タグは `v<semver>`。

**成功条件**

- リリースタグのアセット一覧に、上の 4 ターゲットの
  `kemi-<target>.tar.xz` と対応する `.sha256` が存在する。
- きれいな環境で `mise use -g github:ba0918/kemi` の後、`kemi --version` が
  タグと一致するバージョンを出す。
- README が英語で、UI 文言と docs が日本語である。

**反例**

- Windows 用アセットが無いこと自体は反例ではない（`P7`）。
- インストールされた `kemi` が動的ライブラリ不足で起動しない（musl 静的の失敗）。
- UI に英語の未翻訳文言が混ざる。

---

## R-INPUT 入力

入力は 4 つのモードのいずれか。同時に複数与えられた場合はエラー（終了コード 2）と
する。**グループの生成規則はモードごとに次のとおりで、`R-SUBMIT` / `R-DIGEST` /
`R-INPUT-5` はこの規則に従う。**

| モード | グループ | `id` | `title` |
|---|---|---|---|
| manifest | 作成者が決める | 作成者の値、省略時は `g1`, `g2`, … | 作成者の値（既定 空） |
| コミット範囲・`commit` | マージ以外の各コミット | 完全な commit sha | commit の subject |
| コミット範囲・`file` | 1 つだけ | `all` | `from...to` |
| worktree | 1 つだけ | `worktree` | 「作業ツリーの変更」 |
| staged | 1 つだけ | `staged` | 「ステージ済みの変更」 |

レビューの `title` は、manifest は `title` キー（省略時「変更のレビュー」）、
コミット範囲は `from..to`、worktree は「作業ツリーの変更」、staged は
「ステージ済みの変更」とする。

### R-INPUT-1 manifest

既存 `diff-review` の manifest JSON と互換のキーをすべて受理する。入力は位置引数の
パス、または `-` で標準入力から読む。

| キー | 意味 |
|---|---|
| `title` / `subtitle` / `meta` | ページの見出し情報 |
| `groups[].id` / `.title` / `.why` / `.watch` | グループの識別子と理由、見てほしい点 |
| `groups[].diffs[].path` | 表示パス |
| `groups[].diffs[].old` / `.new` | 差分の左右を直接与える文字列 |
| `groups[].diffs[].old_path` / `.new_path` | 差分の左右をファイルから読む（`--base` 相対） |
| `groups[].diffs[].status` | `add` / `delete` / `rename` / `modify`（省略時は推論） |
| `groups[].diffs[].renamed_from` | 改名元。`status` 省略時は rename と推論 |
| `groups[].diffs[].focus` / `.note` | ファイル単位の重点と理由（`R-FOCUS`） |
| `approval[]` | 承認対象の `path` と `identity`（例 `sha256:...`） |

- 片側が無いことは追加・削除を意味する（空ファイルではなく、行が無いこと）。
- 同じ側に `old` と `old_path`（`new` と `new_path`）を同時に指定した場合は
  エラー（終了コード 2）。存在しないパスを指した場合もエラー（終了コード 2）。
  旧ツールはどちらも黙って空として扱ったが、kemi は黙って進まない。
- `id` 省略時は `g1`, `g2`, … を生成する（旧ツールと同じ形）。
- 各ファイルの統計（増減数）と行データは起動時に整列して求める。manifest は
  人が手で書く入力を想定し、起動時間の目標は設けない。巨大な入力を機械的に
  作りたい場合はコミット範囲モードを使う。

**成功条件**

- リポジトリ内の `tests/fixtures/legacy-manifest.json` を渡すと、レビュー API の
  メタデータが `tests/fixtures/legacy-manifest.expected.json`（グループ、パス、
  状態、増減数）と一致する。
- `kemi -` が標準入力の manifest を読む。

**反例**

- `old` と `old_path` が同時にあり、どちらが優先されるか決まっていない。
- 存在しない `old_path` が黙って空ファイルとして扱われる。
- 同じ manifest で起動するたびにグループ id が変わる。

### R-INPUT-2 コミット範囲

`--from <ref> [--to <ref>]`（`--to` の既定は `HEAD`）でコミット範囲を渡す。

- 既定 `--group-by commit`: `--from..--to` の各コミットが 1 グループ。グループの
  `title` は subject、`why` は body。**マージコミットはグループにしない**
  （変更一覧が空になり、レビュー単位として意味を持たないため）。マージ結果を
  見たい場合は `--group-by file` を使う。
- `--group-by file`: `--from...--to`（merge-base から `--to`）の全体を 1 つの
  差分としてファイルごとに 1 回だけ表示する。2 点ドットと 3 点ドットの非対称は
  意図: commit は「各コミットが何を変えたか」、file は「ブランチ全体の最終形」を
  見るため。
- 統計は `git diff --numstat` 相当で起動時に求める。行データは表示時に求める。
- ファイルの左右の内容は git オブジェクトから読む。改名は git の改名検出に従う。

**成功条件**

- 10,000 ファイルのフィクスチャ（`R-VERIFY`）で、`--group-by commit` のグループ数が
  マージ以外のコミット数と一致する。
- `--group-by file` では同じパスが複数グループに現れない。
- マージコミットのグループが存在しない。

**反例**

- `--group-by file` でコミットをまたいだ同じパスが複数グループに現れる。
- マージコミットが空のグループとして並ぶ。

### R-INPUT-3 未コミット（worktree）

`--worktree` は `HEAD` と作業ツリーの差を表示する（ステージ済みと未ステージの
両方を含む）。untracked ファイルは「新規」として含める。

- 統計は `git diff --numstat HEAD` と untracked の行数から求める。行データは
  表示時に求める。
- サイズまたは種別の上限を超える untracked は、内容を出さず「バイナリ/巨大
  ファイル」として統計だけ表示する。上限値は D6。
- 監視の対象は作業ツリーのファイルそのもの（`R-LIVE`）。

**成功条件**

- untracked の新規ファイルが `add` として現れる。
- 上限を超える untracked が内容なしで一覧に出る。

**反例**

- untracked が無視され、レビュー対象から漏れる。

### R-INPUT-4 ステージ済み（staged）

`--staged` は `HEAD` とインデックスの差を表示する。作業ツリーの変更は含めない。
統計は `git diff --numstat --cached` 相当で求める。

**成功条件**

- 同じファイルにステージ済みと未ステージの変更があるとき、`--staged` は
  インデックス側の内容だけを表示する。

**反例**

- 未ステージの内容が混ざる。

### R-INPUT-5 focus レイヤ

`--focus <path>` は JSON ファイルを読み、入力モードに関係なくグループとファイルへ
focus と note を後付けする。パスは `--base` 相対。

```json
{
  "groups": { "<group-id>": { "watch": "…" } },
  "files": [
    { "path": "src/a.rs", "focus": true, "note": "ここが決定点" }
  ]
}
```

- `groups` のキーは `R-INPUT` の表で定義した `id`（コミットでは完全な sha）。
  許可するキーは `watch` のみ。それ以外のキーはエラー（終了コード 2）。
- `files` はパスで一致する。同じパスが複数グループに現れる場合はすべてに適用する。
- 存在しない `group-id` や `path` を指した場合はエラー（終了コード 2）。
  黙って無視しない。
- manifest 側にも同じ項目がある場合、`--focus` の値が勝つ。
- focus の表示は `R-FOCUS`、`--digest` への反映は `R-DIGEST`。

**成功条件**

- 存在しないパスを書いた `--focus` が、そのパスを名指しして終了コード 2 で失敗する。
- 同じグループに `watch` が両方にある場合、`--focus` の文言が表示される。

**反例**

- タイポしたパスが黙って無視され、重点が表示されないままレビューが始まる。

### R-INPUT-6 その他の入出力

| フラグ | 意味 |
|---|---|
| `--base <dir>` | manifest と `--focus` の相対パス解決の基準（既定 `.`） |
| `--port <n>` | 待ち受けポート（既定 0 = 空きを選ぶ） |
| `--no-open` | ブラウザを自動で開かない |
| `--digest` | ページを出さず `R-DIGEST` の JSON を stdout に出して終了 |
| `--digest-top <n>` | digest のトップファイル数（既定 100） |
| `--serve` | 互換のための受理のみ（既定で常にサーブする） |
| `--out` | 廃止。静的書き出しはしない（`P10`）。指定されたら理由を出して終了コード 2 |

- manifest を省略した場合、`--from` / `--worktree` / `--staged` のいずれかが必要。
- どれも無い場合は使い方を stderr に出して終了コード 2。
- サーブ開始時に stderr へ `kemi: <url>` の 1 行を出す。この行は計測の
  インターフェースとして契約とする（`R-VERIFY`）。

**成功条件**

- `kemi --out x.html` が、静的書き出しが無いことを説明して終了コード 2 で終わる。
- サーブ開始後、stderr の 1 行から URL を取り出せる。

**反例**

- `--out` が黙って無視され、ファイルが出ないまま成功扱いになる。
- URL の行が stdout に出て、submit の JSON と混ざる。

---

## R-SERVE 配信モデル

**要件**

- 常にローカル HTTP サーバを立てる。静的 HTML は書き出さない。
- 起動時に計算するのは、ファイル一覧・状態・統計・グループ情報まで。行データは
  ファイルが表示された時に初めて計算して配信する。
- バインドは `127.0.0.1` のみ。
- URL は `http://127.0.0.1:<port>/s/<token>/` の形。`token` はセッションごとの
  ランダム値で、URL を知らないページからの API 利用を防ぐ。
- API は同じオリジンのページからの呼び出しだけを受理する（`Origin` と `Host` を
  検証する）。クロスオリジンの `POST` は拒否する。
- API の契約:
  - `GET /s/<token>/api/review`: グループとファイルのメタデータ。各ファイルは
    不透明な `id` を含む。
  - `GET /s/<token>/api/file/<id>`: そのファイルの整列済み行データ。
  - `POST /s/<token>/api/comment`: コメントの追加・返信・解決（`R-COMMENT`）。
  - `POST /s/<token>/api/submit`: submit（`R-SUBMIT`）。
  - `GET /s/<token>/api/events`: `R-LIVE` の通知（SSE）。
- 内部 API の JSON 形（行データのフィールド名など）は実装が決める（D7）。
- セッション状態（コメント、見た、折りたたみ、解決）はサーバのメモリに置く。
  プロセス終了で消える（`R-SUBMIT`）。ブラウザの localStorage は表示状態と
  下書きにのみ使う。

**成功条件**

- 10,000 ファイルのフィクスチャで、プロセス開始から `api/review` の初回応答までが
  1 秒未満（release ビルド、`scripts/measure-startup.sh`、3 回の中央値）。
- ファイルを開いていない間、サーバがそのファイルの行データを保持しない
  （`api/file` を呼ぶ前後でメモリ使用量が増えないことを計測で確認）。
- URL の token 無しの API 呼び出しが 404 または 403 で拒否される。
- `Origin` の異なる `POST` が拒否される。

**反例**

- 起動時に全ファイルの内容を読んで行データを作る（10,000 ファイルで 1 秒を超える）。
- `0.0.0.0` にバインドされ、同じネットワークの他端末から到達できる。
- URL の token 無しで API が応答する。
- 別オリジンのページからの `POST` が受理される。

---

## R-VIEW 差分の表示

見た目は diffs.com（OSS ライブラリ `@pierre/diffs`、Apache-2.0）を参照するが、
コードはコピーしない。

**要件**

- 1 列（unified）と 2 列（split）を切り替えられる。行番号、変更記号、単語単位の
  強調、ハンクの折りたたみと展開（クリックでその場に開く）を持つ。
- ファイルヘッダは追従（sticky）し、左のファイルツリーから各ファイルへ飛べる。
- 構文ハイライトをサーバ側で行う。左右それぞれのファイル内容を行単位で着色し、
  複数行にまたがる文字列・コメントを壊さない。
- 1 ファイルの片側が 10,000 行超または 1 MB 超のときはハイライトを切って表示し、
  そのファイルでだけ有効化できる。
- 折返しは既定で無効（横スクロール）。有効にした場合も行の高さを実測して
  描画を破綻させない。
- 表示中の行だけを DOM に載せる。1 ファイル 50 万行でもスクロールが固まらない。
  行要素は `data-kemi-row` 属性を持ち、自動検査が数えられる。
- ノイズ（`CONTEXT.md` 参照）に分類したファイルは既定で畳んで表示し、開ける。
  分類は 1 つの分類器で行い、規則（パスパターン、`.gitattributes` の
  `linguist-generated`、バイナリ判定）は実装に委譲する（D6）。
- 増減数で並べ替えるトグルを持つ。既定の並びは入力の順（manifest の配列順 /
  コミット順）で、トグルを操作した時だけ並びが変わる。
- テーマは light / dark の自動と手動切替、および数種のプリセットを持つ。
  配色は CSS 変数に閉じる。
- バイナリは内容を出さず、バイト数の増減だけを示す。
- CRLF と LF の違いは表示時に正規化し、改行コードの変更のみを差分として
  強調しない。最終行の改行の有無は表示しない。
- `approval` があるときは、承認対象のパスと `identity`（承認対象を識別する値。
  manifest が与える）をフッターに表示する。
- ファイルツリーにはパス、状態（追加 / 削除 / 改名 / 変更）、増減数、ノイズか
  どうか、重点かどうかを示す。

**成功条件**

- 50 万行のフィクスチャファイルを開いてスクロールしても操作が固まらない
  （人が確認）。併せて、DOM 上の `data-kemi-row` 要素数が表示中の行数程度に
  収まる（ブラウザ自動化で確認）。
- 10,000 行超のファイルでハイライトが切れ、当該ファイルで有効化できる。
- `Cargo.lock` などの lockfile が既定で畳まれ、開ける。
- 「見た」と折りたたみの状態が再読込後も残る。コメントはサーバが生きている限り
  残る。

**反例**

- 50 万行すべてを行要素として生成する。
- ハイライトが 1 ファイルの全行を起動時に処理する。
- 改行コードだけの変更が、全行の変更として表示される。
- lockfile が通常ファイルとして展開される。
- ノイズ判定が 2 箇所にあり、ツリーと本文で結果が食い違う。

---

## R-COMMENT コメントと suggestion

**要件**

- コメントは行レンジ（左右どちらかの連続した行）またはファイル全体に付けられる。
  ファイル全体のコメントは行を持たない。
- 行番号は 1 始まり、両端を含む。旧側の行番号は旧ファイルの行番号を指す。
- コメントには返信を追加でき、解決（resolve）できる。
- コメントにはファイル内容の変化に対する `outdated` 状態がある:
  - コメント作成時の行テキスト（`quote`）と、対象ファイルの内容ハッシュを保持する。
  - 内容ハッシュが変わっていたら「古いコメント」として表示する。
    **自動で行番号を付け替えはしない。**
- suggestion は「新側の行レンジを置換する文字列」として構造化して保持する。
  空文字は行の削除を意味する。挿入のみの提案は、隣接行を含む置換として表現する。
  **suggestion は新側の行コメントにだけ付く。旧側とファイル全体のコメントでは
  常に `null`。**
- suggestion をブラウザから適用する機能は持たない（`P8`）。受け取った JSON を
  使って適用するのはエージェントの仕事。
- コメントが追加されたとき、人間向けのライブ表示を stderr に出す。
  この表示の文言は契約としない。

**成功条件**

- コメントを付けて submit すると、`R-SUBMIT` の JSON に、本文・返信・解決状態・
  `quote`・`outdated`・suggestion が入っている。
- コメント後にファイルを変更して submit すると、そのコメントの `outdated` が
  `true` になる。
- 旧側（削除された行）にコメントでき、その suggestion は `null` になる。

**反例**

- suggestion が自由文に埋め込まれ、機械的に適用できない。
- 内容が変わったのに古い行番号のまま最新の行として提示される。

---

## R-FOCUS 重点の表示

kemi は重要度を推定しない。重点は実装したエージェント（manifest または
`--focus`）が印を付けたものだけを、優先的に見せる。

**要件**

- `focus: true` のファイルにはバッジと `note` を表示する。
- グループの `watch` は既存どおり、グループ見出しの下に表示する。
- 「重点のみ表示」のフィルタを持つ。
- kemi が自動で並び順や印を変えることはない。人が変更量ソートのトグルを
  操作した時だけ並びが変わる。
- ファイルツリーと `R-DIGEST` にも `focus` を反映する。

**成功条件**

- `focus` を付けたファイルにバッジとノートが出て、フィルタで絞り込める。
- 変更量だけを理由に順序や印が自動で変わることはない。

**反例**

- kemi が変更量や拡張子から「重要そう」なファイルを自動で重点扱いする。

---

## R-SUBMIT 送信と契約

**要件**

- ページには「承認」「変更要求」の 2 つの送信ボタンと、任意のコメントがある。
  どちらかを押すとレビューは終了し、サーバは応答を返した後に停止して CLI が
  終わる。
- 同時に 2 つの submit が届いた場合、受理するのは 1 つだけとし、もう片方には
  409 を返す。受理して停止した後の再送は接続が拒否されてよい。
- CLI は終了時に stdout へ JSON 文書を 1 つだけ出力する。ログと URL は stderr に
  出す。stdout にそれ以外を混ぜない。
- 終了コード: `0` 承認 / `1` 変更要求 / `2` 起動・使い方・実行時のエラー /
  `130` 中断（submit なしの Ctrl+C）。レビュー中の git や I/O の失敗も `2` とし、
  stdout に JSON を出さない。
- 出力する JSON の契約:

```json
{
  "kemi": 1,
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
      "replies": ["返信"],
      "resolved": false,
      "outdated": false,
      "suggestion": { "replacement": "置換後の全文" }
    }
  ]
}
```

- フィールドの規約:
  - `side` は `"new"` か `"old"`。ファイル全体のコメントは `"new"`。
  - `start_line` / `end_line` は 1 始まり、両端を含む。ファイル全体のコメントは
    `null`。
  - `quote` は `side` 側の対象行のテキスト（ファイル全体のコメントは `[]`）。
  - `suggestion` は無いとき `null`。旧側とファイル全体では常に `null`。
  - `replies` と `resolved` は全コメントに必ずある（無い場合は `[]` / `false`）。
  - `approval` は入力をそのまま返す。無い場合は `[]`。
  - `group_id` は `R-INPUT` の表の値。`group_title` はそのグループの `title`。
  - `comments` は作成順。`kemi` はこの契約の版（数値）。

**成功条件**

- 承認で終了コード 0、変更要求で 1 になり、stdout が 1 つの JSON として
  パースできる。
- コメント本文に改行や引用符があっても JSON が壊れない。
- 中断（Ctrl+C）では JSON が出ず、終了コード 130。
- ファイル全体へのコメントが `start_line: null` / `end_line: null` /
  `quote: []` で出る。

**反例**

- 進捗ログが stdout に混ざり、JSON のパースに失敗する。
- 同時 submit の両方が受理され、どちらの verdict か分からなくなる。

---

## R-DIGEST digest

**要件**

- `--digest` はページを出さず、レビューの地図だけを JSON として stdout に
  1 つ出して終了する（行内容は含めない）。
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
  "top_n": 100
}
```

- `directories` はルート直下のディレクトリ単位で集約する。直下のファイルは
  `path` を `"."` とする 1 項目にまとめる。
- `top_files` は増減の合計（`add + del`）の降順、同数はパス昇順。件数は
  `--digest-top`（既定 100）。
- 文字列は 2,000 文字を上限とし、切った場合は末尾に `…` を付ける。
- 行内容や `quote` は含めない。30,000 ファイルの入力でも出力が 100 KB 未満に
  収まる。

**成功条件**

- 30,000 ファイルのフィクスチャで `--digest` の出力が 100 KB 未満で、
  `top_files` が増減合計の降順に `--digest-top` 件入っている。
- エージェントがこの JSON だけで「どこを読むべきか」を選べる（人が確認）。

**反例**

- `--digest` が全ファイルの一覧をそのまま出し、30,000 ファイルで数 MB になる。
- 行内容や `quote` が混ざる。
- 切り詰めた `why` に印が無く、完全な文と区別できない。

---

## R-LIVE ライブリロード

**要件**

- 監視対象は「レビューの新側の供給元」に限る:
  - worktree / manifest の `new_path`: 対象ファイル
  - コミット範囲: `--to` の ref（既定 `HEAD`）。ref の更新は検知の対象。
    それ以外の `.git` 内部の書き込みは無視する。
- 変更を検知したら SSE でブラウザに「更新あり」を知らせ、バッジを表示する。
- 再取得は人の操作で行う。勝手にスクロール位置や表示内容を変えない。
- 再取得後、内容が変わったファイルのコメントは `outdated` 表示にする（`R-COMMENT`）。
- 監視の追加・更新は debounce し、短期間の複数書き込みでバッジを点滅させない。

**成功条件**

- worktree モードでファイルを外部から変更すると、バッジが出て、押すと差分が更新
  される。
- 変更後にページのスクロール位置が勝手に変わらない。
- `--group-by commit` で `--to HEAD` の状態で新しいコミットを作るとバッジが出る。

**反例**

- リポジトリ全体を再帰監視して、数万ファイルのリポジトリで監視が破綻する。
- 変更のたびに自動で再読込し、読んでいた位置を失う。
- 対象外の `.git` 書き込みでバッジが出続ける。

---

## R-WS 構成

```
Cargo.toml            # workspace + バイナリパッケージ kemi
rust-toolchain.toml   # Rust の版を固定
src/main.rs           # CLI 起動と配線、stdout/stderr、終了コード
crates/kemi-core/
  src/domain/         # 純粋なドメイン（差分整列、コメント、digest、ノイズ判定）
  src/source/         # git・manifest の読み取り（外部入力アダプタ）
crates/kemi-server/   # HTTP / SSE（axum）
crates/kemi-webview/  # HTML/CSS/JS 資産の埋め込み
web/
  package.json        # TypeScript（型チェックのみ）を固定
  tsconfig.json
  *.js                # ビルド不要の ESM + CSS
scripts/
  gen-fixture.sh      # 検証用の合成リポジトリ生成
  measure-startup.sh  # 起動時間の計測
```

- `kemi-core` の `domain` は純粋関数で、axum / hyper / tokio / `std::process` に
  依存しない（`source` モジュールはこの限りでない）。層は
  domain → source → server/cli の一方向。
- `kemi-server` は `kemi-core` に依存する。`kemi-webview` はどこにも依存しない。
  ルートのバイナリが 3 つを配線する。
- フロントはビルドなしのネイティブ ESM と CSS。型は JSDoc で書き、CI で
  `tsc --checkJs --noEmit` を通す（型チェックのみで生成物を出さない）。
  純粋なロジックは `node --test` でテストする。
- Node と Rust の版はリポジトリ内で固定する（`.mise.toml` / `rust-toolchain.toml`）。
- 資産は release ビルドでバイナリに埋め込み、開発時はディスクから配る。

**成功条件**

- `cargo build` だけで（Node 無しでも）バイナリができる。
- `rg 'use (axum|hyper|tokio|std::process)' crates/kemi-core/src/domain` が
  import 行に一致しない。
- `npx tsc -p web --noEmit` と `node --test web` が成功する。

**反例**

- ドメインが git コマンドや HTTP の型を直接参照する。
- フロントのビルドに Node が必須で、クリーンな `cargo build` が失敗する。

---

## R-DEPS 依存

調査済みの推奨スタックを使う（`D5`）。互換のため、同じ役割の別クレートに
差し替えても上の要件を満たすなら変更してよい。

- 差分整列: `similar`（単語単位の強調を含む）
- 構文ハイライト: `syntect` + `two-face`。`syntect` は `default-features = false` と
  `default-fancy` を使い、oniguruma（C 実装）に依存しない。計画時に
  `cargo tree -e features` で C 依存が無いことを確認する
- HTTP / SSE: `axum`
- 資産埋め込み: `rust-embed`
- ファイル監視: `notify` + `notify-debouncer-full`
- JSON: `serde` / `serde_json`

許可する依存ライセンスは MIT / Apache-2.0 / BSD-3-Clause / ISC / Unicode-3.0 と
その互換（0BSD 等）に限る。

**成功条件**

- `cargo deny check licenses` が成功する。
- `cargo tree -e features` に oniguruma 系の C 依存が現れない。

**反例**

- GPL 系ライセンスのクレートが混入する。
- musl 静的リンクが C 依存で失敗する。

---

## R-VERIFY 検証

| 条件 | 出所 |
|---|---|
| Rust のユニット・統合テスト | `cargo test` |
| 警告ゼロ | `cargo clippy -- -D warnings` / `cargo fmt --check` |
| 依存ライセンス | `cargo deny check licenses` |
| フロントの型 | `npx tsc -p web --noEmit` |
| フロントの純ロジック | `node --test web` |
| 規模の目標 | `scripts/gen-fixture.sh` と `scripts/measure-startup.sh` |
| 表示と操作 | 人による確認。DOM 行数はブラウザ自動化（agent-browser 等）で `data-kemi-row` を数える |
| 配布 | リリースワークフローの成果物と、別環境での mise インストール |

- `scripts/gen-fixture.sh <dir> --files N --lines M [--commits K]` は、決定的な
  内容で git リポジトリを生成する（同じ引数なら同じ結果）。
- `scripts/measure-startup.sh <fixture> <kemi-bin>` は、release ビルドの kemi を
  `--port 0 --no-open` で起動し、stderr の `kemi: <url>` 行を待って
  `api/review` を取得するまでの時間を計測し、3 回の中央値を表示する。
- 時間の上限（1 秒）は開発機でのスモーク基準とする。CI では余裕を持った閾値に
  し、マシン差を理由に失敗し続けないようにする。

**成功条件**: 上のコマンドがすべて成功する。

**反例**: 目標の規模で 1 秒を大きく超える、またはテストが環境依存で
恒常的に不安定。

---

## 作らないもの

- P1 マージコンフリクトの解決 UI。コンフリクトマーカーは通常の行として表示される。
  解決はエージェントが行う。
- P2 ブラウザ内の編集モード。
- P3 ミニマップ。
- P4 テーマのカスタム編集 UI（プリセットのみ）。
- P5 複数ユーザーと認証。
- P6 crates.io への配布（まず自分で使い、必要になったら別途）。
- P7 Windows 対応（まず Linux と macOS）。
- P8 suggestion のブラウザからの適用。適用はエージェントが行う。
- P9 LLM がコメントや返信を kemi に書き込む機能。作る場合は、ファイルあたり・
  合計の件数上限を契約に含めること。
- P10 静的 HTML の書き出し（`--out`）。

## 委譲

- D1 CLI 表面の細部（`R-INPUT-6` に書いた契約以外のフラグ名、エラーメッセージ、
  モード排他時の文言）。
- D2 テーマプリセットの具体的な配色。
- D3 リリースパイプラインの道具立て（cargo-dist、GitHub Actions）。
- D4 submit JSON の残余の細部（`id` の形式、ライブ表示の文言）。骨格は
  `R-SUBMIT` が契約。
- D5 クレート選定（`R-DEPS` の範囲内）。
- D6 ノイズ判定の具体パターン、untracked のサイズ上限の値、行ハイライトの
  キャッシュ方式。存在と意味は契約、値は動作を変えない範囲で実装が決める。
- D7 内部 HTTP API の JSON 形（`R-SERVE` のエンドポイント以外の形）。

## 却下

- R1 名前 `diff-review` の継承（理由: kemi を選択）。
- R2 静的 HTML 書き出し（理由: 大規模初期表示とコメント機能の両方で不利）。
- R3 テーマカスタム編集 UI（理由: プリセットで足りる）。
- R4 ブラウザ内編集（理由: レビューが目的。編集はエージェントが行う）。
- R5 Windows 対応（理由: 今は不要。後から検討）。
- R6 suggestion のブラウザ適用（理由: 自分では直さない方針）。

## 未決

- なし。

## 納品後の環境更新（リポジトリ外）

- `~/.local/bin/diff-review`（Python）を kemi に置き換える。
- `diff-review-viewer` スキルを更新する: `kemi` の呼び方、stdout JSON 契約、
  `--out` 廃止、suggestion はエージェントが適用すること。
