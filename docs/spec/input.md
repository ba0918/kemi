# kemi 仕様: 入力

[kemi 仕様](kemi.md) の一部。入力モード（manifest、コミット範囲、worktree、staged）と focus レイヤ、その他の入出力。
用語の定義は `CONTEXT.md`、成功条件と反例の書き方は [kemi 仕様](kemi.md) の冒頭にある。

---

## R-INPUT 入力

入力は 5 つのモードのいずれか。同時に複数与えられた場合はエラー（終了コード 2）と
する。**グループの生成規則はモードごとに次のとおりで、`R-SUBMIT` / `R-DIGEST` /
`R-INPUT-5` はこの規則に従う。**

| モード | グループ | `id` | `title` |
|---|---|---|---|
| manifest | 作成者が決める | 作成者の値、省略時は `g1`, `g2`, … | 作成者の値（既定 空） |
| コミット範囲・コミットごと | マージ以外の各コミット | 完全な commit sha | commit の subject |
| コミット範囲・最終形 | 1 つだけ | `all` | `from...to` |
| worktree | 1 つだけ | `worktree` | "Working tree changes" |
| staged | 1 つだけ | `staged` | "Staged changes" |
| `--live`（[live.md](live.md)） | ページ 1 つと、コード（worktree と同じ） | ページは `page`、コードは `worktree` | ページは "Page"、コードは "Working tree changes" |

コミット範囲は「最終形」と「コミットごと」の 2 つのグループ単位を両方持ち、画面で
切り替える（`R-UNIT`）。ほかのモードのグループ単位は 1 つだけ。

レビューの `title` は、manifest は `title` キー（省略時 "Review of changes"）、
コミット範囲は `from..to`、worktree は "Working tree changes"、staged は
"Staged changes"、`--live` は "Live review of <URL>" とする（`<ファイル>` のときの URL は
[live.md](live.md#r-page-mode-起動) の配れる範囲の根からの相対パス）。

**成功条件**

- `kemi --worktree --staged` が終了コード 2 で終わる。
- `title` の無い manifest で、レビューの `title` が "Review of changes" になる。

**反例**

- 2 つの入力モードを同時に与えると、片方が黙って無視される。

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
| `groups[].diffs[].focus` / `.note` | ファイル単位の重要と理由（`R-FOCUS`） |
| `approval[]` | 承認対象の `path` と `identity`（例 `sha256:...`） |

- 片側が無いことは追加・削除を意味する（空ファイルではなく、行が無いこと）。
- 同じ側に `old` と `old_path`（`new` と `new_path`）を同時に指定した場合は
  エラー（終了コード 2）。存在しないパスを指した場合もエラー（終了コード 2）。
  旧ツールはどちらも黙って空として扱ったが、kemi は黙って進まない。
- `id` 省略時は `g1`, `g2`, … を生成する（旧ツールと同じ形）。
- 各ファイルの統計（増減数）は起動時に求める（manifest では整列して求める）。
  行データの計算は `R-SERVE` に従い、表示された時に初めて行う。manifest は
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
コミット範囲は次の 2 つのグループ単位を持ち、1 回の起動の中で切り替えられる
（`R-UNIT`）。`--group-by` は起動時に表示する単位を選ぶ（既定 `file`）。

- 最終形（`--group-by file`、既定）: `--from...--to`（merge-base から `--to`）の
  全体を 1 つの差分としてファイルごとに 1 回だけ表示する。変更ブロックごとに
  由来のコミットを示す（`R-ORIGIN`）。
- コミットごと（`--group-by commit`）: `--from..--to` の各コミットが 1 グループ。
  グループの `title` は subject、`why` は body。**マージコミットはグループにしない**
  （変更一覧が空になり、レビュー単位として意味を持たないため）。
- 2 点ドットと 3 点ドットの非対称は意図: コミットごとは「各コミットが何を変えたか」、
  最終形は「ブランチ全体の最終形」を見るため。範囲にマージがある場合、マージ自体
  （衝突の解決）が変えた行は最終形にだけ現れ、コミットごとのどのグループにも
  属さない。
- 統計は `git diff --numstat` 相当で起動時に求める。行データは表示時に求める。
- ファイルの左右の内容は git オブジェクトから読む。改名は git の改名検出に従う。

**成功条件**

- 10,000 ファイルのフィクスチャ（`R-VERIFY`）で、コミットごとのグループ数が
  マージ以外のコミット数と一致する。
- 最終形では同じパスが複数グループに現れない。
- マージコミットのグループが存在しない。
- `--group-by` を付けない `--from` が最終形で始まる。

**反例**

- 最終形でコミットをまたいだ同じパスが複数グループに現れる。
- マージコミットが空のグループとして並ぶ。
- `--group-by` を付けない `--from` がコミットごとで始まる。

### R-INPUT-3 未コミット（worktree）

`--worktree` は `HEAD` と作業ツリーの差を表示する（ステージ済みと未ステージの
両方を含む）。untracked ファイルは「新規」として含める。

- 統計は `git diff --numstat HEAD` と untracked の行数から求める。行データは
  表示時に求める。
- サイズまたは種別の上限を超える untracked は、内容を出さず「バイナリ/巨大
  ファイル」として統計だけ表示する。上限値は D6。
- 監視の対象は作業ツリー全体（`.git` の中と git が無視するものを除く。`R-LIVE`）。

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
  コミット範囲では、起動時に最終形の `all` と範囲内のコミットの sha の両方と照合する
  （コミットごとの単位を作り終えるのを待たない）。
- `files` はパスで一致する。同じパスが複数グループに現れる場合はすべてに適用する。
  コミット範囲では両方のグループ単位に適用する。パスの照合も起動時に行い、最終形か
  範囲内のどれかのコミットに現れるパスを「存在する」とみなす（コミットごとのパスの
  一覧は、`--focus` を付けたときだけ起動時に求める）。
- 存在しない `group-id` や `path` を指した場合はエラー（終了コード 2）。
  黙って無視しない。
- manifest 側にも同じ項目がある場合、`--focus` の値が勝つ。
- focus の表示は `R-FOCUS`、`--digest` への反映は `R-DIGEST`。

**成功条件**

- 存在しないパスを書いた `--focus` が、そのパスを名指しして終了コード 2 で失敗する。
- 同じグループに `watch` が両方にある場合、`--focus` の文言が表示される。

**反例**

- タイポしたパスが黙って無視され、重要が表示されないままレビューが始まる。

### R-INPUT-6 その他の入出力

説明の初期入力と排他は [R-EXPLANATION-CLI](explanation-contracts.md#r-explanation-cli-初期投入と全体更新) を適用する。

| フラグ | 意味 |
|---|---|
| `--base <dir>` | manifest と `--focus` の相対パス解決の基準（既定 `.`） |
| `--port <n>` | 待ち受けポート（既定 0 = 空きを選ぶ） |
| `--bind <addr>` | 待ち受ける IPv4 アドレス（既定 `127.0.0.1`。`0.0.0.0` は全 IPv4 インターフェース） |
| `--no-open` | ブラウザを自動で開かない |
| `--digest` | ページを出さず `R-DIGEST` の JSON を stdout に出して終了 |
| `--digest-top <n>` | digest のトップファイル数（既定 100） |
| `--serve` | 互換のための受理のみ（既定で常にサーブする） |
| `--out` | 廃止。静的書き出しはしない（`P10`）。指定されたら理由を出して終了コード 2 |
| `--group-by <commit\|file>` | コミット範囲で起動時に表示するグループ単位（既定 `file`、`R-UNIT`） |
| `--result` | レビューを始めず、結果ファイルの最新の結果を stdout に出して終了（`R-RESULT`） |
| `--any` | `--result` と一緒に使う。場所に関係なく最新の結果を選ぶ |
| `--workspace <path>` | `--result` と一緒に使う。結果を探す場所を起動ディレクトリの代わりに指定する |
| `--resume [<id>]` | 保留したセッションを復元する。`id` を省略すると対話で選ぶ（`R-SESSION`） |
| `--live <url\|ファイル>` | 動いているページか手元の HTML ファイルのレビュー。組めるフラグは [live.md](live.md#r-page-mode-起動) |
| `--live-port <n>` | `--live` の中継の待ち受けポート（既定 0 = 空きを選ぶ） |

- manifest を省略した場合、`--from` / `--worktree` / `--staged` / `--live` / `--result` /
  `--resume` のいずれかが必要。`kemi wait` と `kemi reply` はレビューを始めないサブコマンドで、
  この規則の外（[agent-channel.md](agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)）。
- どれも無い場合は使い方を stderr に出して終了コード 2。
- `--result` と一緒に使えるのは `--any` と `--workspace` だけで、それ以外のフラグや
  入力モード（manifest / `--from` / `--worktree` / `--staged` / `--live` / `--resume`）との同時指定、
  `--result` なしの `--any` / `--workspace`、`--any` と `--workspace` の同時指定、
  `--from` なしの `--group-by` は、使い方の誤りとして終了コード 2。
- `--resume` と一緒に使えるのは `--port` / `--bind` / `--no-open` / `--serve` /
  `--live-port` だけで、入力モード（manifest / `--from` / `--worktree` / `--staged` / `--live`）と `--digest` /
  `--group-by` / `--base` / `--focus` / `--result` / `--any` / `--workspace` との
  同時指定は、使い方の誤りとして終了コード 2。
- `--bind` の値は IPv4 アドレスのリテラルだけを受理する。ホスト名と IPv6 は
  使い方の誤りとして終了コード 2。`--digest` はサーブしないため値を使わないが、
  検査はサーブ時と同じに行う。
- サーブ開始時に stderr へ `kemi: <url>` の 1 行を出す。この行は計測の
  インターフェースとして契約とする（`R-VERIFY`）。続けて `kemi: review <id>` の 1 行を出す
  （[agent-channel.md](agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)）。

**成功条件**

- `kemi --out x.html` が、静的書き出しが無いことを説明して終了コード 2 で終わる。
- サーブ開始後、stderr の 1 行から URL を取り出せる。
- `kemi --result --from HEAD~1`、`kemi --result --port 0`、`kemi --any`、
  `kemi --result --any --workspace .`、`kemi --worktree --group-by commit` が
  それぞれ終了コード 2 で終わる。
- `kemi --worktree --resume`、`kemi --resume --digest`、`kemi --resume --workspace .`
  がそれぞれ終了コード 2 で終わる。
- `kemi --worktree --bind localhost`（ホスト名）、`kemi --worktree --bind ::1`
  （IPv6）、`kemi --result --bind 0.0.0.0`（`--result` との排他）が、それぞれ
  終了コード 2 で終わる。

**反例**

- `--out` が黙って無視され、ファイルが出ないまま成功扱いになる。
- `--result` なしの `--any` が黙って無視され、レビューが始まる。
- `--resume` に元のモードのフラグを足すと、黙って無視されてセッションと違うレビューが始まる。
- URL の行が stdout に出て、submit の JSON と混ざる。
- `--bind` の値が検査されず、意図しないアドレスで待ち受ける。

---

## 委譲

- D1 CLI 表面の細部（`R-INPUT-6` に書いた契約以外のフラグ名、エラーメッセージ、
  モード排他時の文言）。
