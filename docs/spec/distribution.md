# kemi 仕様: 名称と配布

[kemi 仕様](kemi.md) の一部。名称、配布、スキルとリリースの約束。
用語の定義は `CONTEXT.md`、成功条件と反例の書き方は [kemi 仕様](kemi.md) の冒頭にある。

---

## R-DIST 名称と配布

**要件**

- リポジトリ名とバイナリ名は `kemi`（「閲する」から）。
- ライセンスは MIT OR Apache-2.0。
- README と `skills/` の配布物は英語（翻訳版は作らない）、docs（この仕様を含む）は
  日本語。UI の文言（文章、ボタン、説明文、エラー表示、端末の選択画面）と CLI の出力
  （ヘルプ、エラーメッセージ、stderr の運用メッセージとライブ表示）は英語にする。エラーメッセージには
  kemi が作るすべてのエラー（入力の読み取り、manifest の検証、git の失敗、focus の
  検証、API のエラー）を含む。レビューの既定の `title`（`R-INPUT`）も英語にする。
- 対応ターゲットは次の 6 つ:
  - `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`（静的リンク）
  - `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`
- `aarch64-pc-windows-msvc` は CI の Windows ランナー上でのクロスコンパイルでビルドを
  検証し、ビルドできない場合は `x86_64-pc-windows-msvc` のみに落とす。
- GitHub Releases に cargo-dist 形式のアセットを添付する。unix は
  `<name>-<target>.tar.xz`、Windows は `<name>-<target>.zip` で、それぞれ
  checksum（`.sha256`）を付ける。`mise use -g github:ba0918/kemi` でインストール
  できる（動作確認は unix のみ。Windows での mise 経由は保証しない）。Windows の
  導入手順は zip を解いて `kemi.exe` を PATH へ置く。
- バージョンの正典はリポジトリ内の 1 箇所（ルート `Cargo.toml` の `version`）。
  タグは `v<semver>`。
- エージェント向けの使い方の文書を `skills/kemi/SKILL.md` に置き、リポジトリごと配る。
  形式は Agent Skills（agentskills.io の仕様。`SKILL.md` の先頭に `---` で囲んだ設定欄
  = frontmatter を持つ）。以下、この文書を「スキル」と呼ぶ。スキルは README と同じく
  この仕様から派生した配布物で、契約の正典ではない。正典はこの仕様で、スキルの文が
  契約を上書きしない。
  - 含むもの（括弧は書き写す元の節）: 起動と終了の待ち方 — レビュー中にタイムアウトで
    終了させない実行方法を選び、URL を伝えた後も終了コードと stdout の JSON を受け取る
    まで待機を継続する。ツールが処理を継続したまま一旦戻ることは、プロセスの終了ではない。
    ターンを終了してよいのは、終了通知でエージェントが自動再開されると確認できる場合だけ。
    それ以外は、実行識別子を保持し、待機・出力取得のツールを繰り返し使う。短い間隔での
    確認は避けるが、必要な待機は禁止しない。途中で止めない、会話の
    中の「承認」を verdict と見なさない（`R-SUBMIT`）。終了コードの表と、`130`
    （利用者による中止）では結果が無いが、保留ならセッションが残り、案内される
    `--resume` で続けられること（`R-SUBMIT`、`R-SESSION`）。入力モードの選び方と、
    `--group-by` の既定
    `file` とコミットごとを見せたいときの指定、`--no-open` / `--port` / `--bind` /
    `--focus`（`R-INPUT`）。エージェントとの往復 — `kemi: review <id>` の行、`kemi wait` を
    裏で待たせて起きたことを読む、`kemi reply` で返信・発言・案を書く、`--timeout` と終了
    コード、渡されたコメントへの対応のしかた（[agent-channel.md](agent-channel.md)）。
    `--live` の使い方 — 対象はループバックの開発サーバか手元の HTML ファイル、ページとコードの 2 つの見方、
    比べる相手（スナップショットとモック。[live-compare.md](live-compare.md)）、
    ページのコメントの場所の読み方と描き込みの画像、案の書き方と `applied`
    （[live.md](live.md)）。manifest の書き方 — グループの `why` / `watch`、差分の `old` / `new` と
    `old_path` / `new_path`、`approval` の identity をエージェント自身が計算すること
    （`R-INPUT-1`）。結果 JSON の読み方 — 契約版 `kemi`、verdict、コメント、suggestion は
    エージェントが適用する、outdated の扱い、承認のときは identity を確かめてから進む
    （`R-SUBMIT`、`R-COMMENT`、`R-LIVE`）。`--result` での取り直し（`R-RESULT`）と、
    保留したレビューの再開（`R-SESSION`）。`--digest` の使い所（`R-DIGEST`）。
    起動後に人へ伝えること — URL、グループの数、
    最も見てほしい判断、自分の仮定（`R-VIEW`）。
  - 含まないもの: kemi の内部構造、ビルド、開発の手順。
  - スキルのディレクトリ内で自己完結する。`SKILL.md` は使用条件、起動から結果受け取り
    までの完了条件、必要な詳細への案内を持ち、入力形式や結果 JSON の詳細は同梱の
    `references/` に分けて必要なときだけ読む。他のスキルや外部の共有ファイルに依存せず、変更の提示の規範（例:
    差分を意図ごとにまとめる規則）は「あればそれに従う」と言及するだけ。本文（末尾の
    注を除く部分）は道具に依存しない言葉で書き、特定のエージェント向けの具体は末尾の注
    に分ける。
  - frontmatter は `name: kemi`、`description`（何をするか、いつ使うか）、`license` の
    3 つだけを持ち、ほかの項目は付けない。値は ASCII だけで書く（配布の道具の中に
    frontmatter を ASCII に限るものがある）。
  - 配布は、配布の定義ファイルを要らない経路（以下、コピー経路。symlink で置く道具を
    含む）だけ。保証するのは `gh skill install ba0918/kemi kemi`（GitHub CLI の組み込み。
    2 つ目の引数がスキル名）。`skills/kemi/` を手でコピーしても使える。README に載せる
    経路はこの `gh skill install` の 1 つだけ。`npx skills` と APM はこの形のリポジトリを
    読める仕組みだが未実測で、README には載せない。Claude Code / Codex の plugin の
    定義ファイル（`.claude-plugin/`）は置かない（`P12`、`R17`）。
  - スキルは frontmatter に版を持たない。届く内容は経路で決まる: `gh skill` はタグ付き
    リリースがあればその最新、無ければ既定ブランチの HEAD を配る。利用者に届けたい
    スキルの直しは、文面だけの変更でもリリース（`v<semver>` のタグ）で出す。ほかの経路
    の更新は、利用者が入れ直したときに届く。
  - CLI と JSON の契約を変えるコミットで、スキルと README も同時に直す。両者の一致を
    機械では検査しない（レビューで見る）。
  - README の `--bind` の説明に、公開が OS とネットワークに依存することを書く
    （OS のファイアウォール、WSL2 では Windows 側の設定、繰り返し公開するときの
    `--port` での固定）。kemi 自身はファイアウォールや転送の設定を行わない。

**成功条件**

- リリースタグのアセット一覧に、上の 6 ターゲットの成果物がある。unix は
  `kemi-<target>.tar.xz`、Windows は `kemi-<target>.zip` で、それぞれ対応する
  `.sha256` を付ける。
- きれいな環境で `mise use -g github:ba0918/kemi` の後、`kemi --version` が
  タグと一致するバージョンを出す（unix）。
- Windows の zip を解いた `kemi.exe` を PATH に置いた環境で、`kemi --version`
  がタグと一致するバージョンを出す。
- README と `skills/` 配下のすべてのファイルが英語で、docs が日本語、UI の文言と CLI の出力が
  英語である。
- e2e テストが、CLI のヘルプと、使い方の誤りで出力されるエラーメッセージに、日本語の
  文字が含まれないことを検証する。
- `skills/kemi/SKILL.md` の frontmatter が、`name` は親ディレクトリ名 `kemi` と一致、
  `description` は空でなく 1024 文字以内、項目は上の 3 つだけ、値は ASCII だけ、を
  満たす。`name` と `description` は Agent Skills の公式バリデータ
  `agentskills validate ./skills/kemi`（skills-ref。版の固定は `D3`）で、項目の数と
  ASCII は frontmatter を読む短い検査で確かめる。
- 最初のリリース以降、きれいな環境で `gh skill install ba0918/kemi kemi` の後、
  エージェントがスキルの手順だけで kemi を起動し、submit（承認または変更要求）まで
  到達して stdout の JSON を受け取れる（人による確認。エージェントはどれでもよい）。
- README に `--bind` の説明があり、OS のファイアウォール、WSL2 の Windows 側の
  設定、繰り返し公開するときの `--port` の固定に触れている。スキルにも `--bind` が
  ある（人による確認）。

**反例**

- スキルが、この仕様に無いフラグや JSON のキーを案内している。
- スキルの本文（末尾の注を除く部分）に、特定のエージェントの道具の名前が出ている。
- インストールされた `kemi` が動的ライブラリ不足で起動しない（musl 静的の失敗）。
- UI の文言（文章、ボタン、説明文、エラー表示）に、日本語の文言が残る。
- CLI の出力（ヘルプ、エラーメッセージ、stderr の運用メッセージとライブ表示）に日本語の
  文言が残る。

---

## 作らないもの

- P6 crates.io への配布（まず自分で使い、必要になったら別途）。
- P12 スキルの plugin の定義ファイル（`.claude-plugin/plugin.json`、`marketplace.json`）。
  コピー経路だけで配る（`R17`）。

## 委譲

- D3 リリースパイプラインの道具立て（cargo-dist、GitHub Actions）。
- D10 スキル（`skills/kemi/SKILL.md`）と README の Agent skill の節の、言い回しと見出しの
  構成。`R-DIST` の「含むもの」を欠かさず、契約と食い違わない限り、どの書き方でも
  振る舞いは同じ。手順だけで submit まで到達できることは `R-DIST` の成功条件が縛る。

## 却下

- R1 名前 `diff-review` の継承（理由: kemi を選択）。
- R17 スキルを plugin 経路（Claude Code / Codex の marketplace の定義ファイル）でも配る
  （理由: 単体のスキルに対して、エージェントごとの定義ファイルと版の写しが要り、手間に
  見合わない。コピー経路は定義ファイル無しで動く）。
