# 残課題

最終更新: 2026-09-20（狭い画面のマージ後）

## レビューで記録のみになった指摘（修正要求ではない）

LAN 公開 `--bind`（v0.1.7）:

- id 1: README の `### Exposing kemi to other devices` がキー操作表の直前にあり、
  `Keys in the page` がその子見出しとして描画される
- id 3: Host 許可のループバック枝と「バインドした具体アドレス」の枝を直接確かめるテストが無い。
  製品の挙動は手動確認で正しい（`--bind 192.168.1.2` で 200 / 403）
- id 4: `ServeParams.share_address` に `Some(0.0.0.0)` を渡すと wildcard の Host を受理し、
  URL にも `0.0.0.0` を使う。CLI 経由では到達しないが、pub な値の前提がコードに現れていない
- evidence と oracle はローカルの `.agents/artifacts/reviews/lan-bind.json` にある（git 管理外・この環境のみ）

kemi v1（main にマージ済み）:

- id 12: ハイライトのファイル単位の手動指定が再読込で消える
- id 16: A/D が約 5 万件を超える worktree で `git` の引数長（E2BIG）で起動できない
- id 26: digest のグループ・ディレクトリ上限が、余裕があっても常時適用される
- id 36: split 表示の replace 旧側に `+` が無く、旧側に行コメントを付けられない
- id 39: コメント応答待ちに新しく開いたエディタが、先行応答の到着で閉じられる（狭い経路）

UI 改訂（main にマージ済み）:

- id 5: 畳みトグルをキーボードで操作すると、再描画でフォーカスが body に落ちる。
  UX 改訂 v2 でグループ帯の説明の開閉は描き直さずに切り替えるようにしたが、キーボードでの
  確認はしていない

evidence と oracle の詳細はローカルの `.agents/artifacts/reviews/kemi-v1.json` と
`.agents/artifacts/reviews/kemi-ui-fix.json` にある（git 管理外・この環境のみ）。

セッションの保存と復元（main にマージ済み）:

- id 7: 一覧と掃除が payload を含むセッションファイル全体を読む。コメント・見た・
  折りたたみの変更のたびに全ファイルを書き直し、全セッションを読み直すので、
  大きいレビューやセッションが増えた環境で操作が重くなる
- id 15: 復元のたびに写しを再凍結する。同じ内容を encode + gzip し直して書き直すため、
  何も変えていなくても最終更新が進み、一覧の先頭に移る

evidence と oracle はローカルの `.agents/artifacts/reviews/session-resume.json` にある
（git 管理外・この環境のみ）。ほかの info（record_only）の指摘も同ファイルにある。

描画表示（main にマージ済み）:

- id 10: 描画のブロックを行に写す `overlay::plan` が、ブロックの行番号で整列の配列を直接添字する。
  行数を超える行番号のブロック（末尾の改行の後ろから始まる Span など）が出ると panic しうる。
  再現する入力は未確認
- id 11: Markdown の画像の `src` が `#` 始まりのとき、仕様は href / src とも `#` 始まりを通すと
  書くが、実装は枠にしている（画像に断片は意味を持たないため）。仕様の文言の確認候補
- id 12: SVG はバイナリでないため `n` / `p` の移り先になり、移った先は既定が描画表示（画像 2 枚）で
  止まる場所が 0 になる。`R-NAV` の「バイナリ（画像を含む）は飛ばす」に SVG を含めるかの確認候補
- 仕様に無いと実装時に挙がった点（決めていない）: 行の整列が空行を優先して対応づけ、内容の違う
  段落が 1 ブロックに重なることがある（直すなら `domain/diff.rs`）。リンクの URL だけの変更には
  語の印が出ない。`api/render` が各側の全行を載せる（10,000 行で応答 2.7 MB）。
  `content_skipped` が立っていても `api/render` / `api/image/review` を直接叩けばディスクを読む

evidence と oracle はローカルの `.agents/artifacts/reviews/rendered-view.json` にある
（git 管理外・この環境のみ）。

狭い画面（main にマージ済み）:

- id 6: 上部バーの 2 段目で進捗を数字だけにする判定（`fitProgress`）が幅を見ないので広い画面でも
  走り、描画のたびに強制レイアウトが 1 回増える（見た目は変わらない）
- id 7: ファイル選択の先頭で常に引き出しを閉じるので、更新バッジの再取得やテーマの読み直しでも
  引き出しが閉じる。仕様は「ファイルを選ぶか外を押すと閉じる」しか言わない
- id 8: 描画表示のブロックのタップ（click）が広い画面でも登録される。マウスでは mouseover が
  先に届くので観察される差は無い
- id 9: 幅をまたいだときに、閉じている popover にも `hidePopover()` を呼ぶ。WebKit で例外に
  ならないかは未確認（Chromium と利用者の実機では通っている）
- id 10: `#menu-theme` の中身は `applyTheme` が作り直すので index.html 側は使われない。テーマの
  SVG が 3 箇所に重複
- id 11: `scripts/test-narrow-screen.mjs` の 2 回目の起動に try/finally が無く、途中で落ちると
  kemi と agent-browser のセッションが残る
- 仕様に無いと実装時に挙がった点（決めていない）: 範囲の端の行の再タップは新しい 1 行の選択、
  「…」のメニューは項目を押しても閉じない、引き出しを開いたまま `n` / `p` でファイルが
  切り替わると閉じる、一覧から選んだコメントの Edit / Delete 後の印の扱い

evidence と oracle はローカルの `.agents/artifacts/reviews/narrow-screen.json` にある
（git 管理外・この環境のみ）。

## 既知の制限
- `PROJECT.md` の `scripts/` の説明と Commands の表に、3 本のブラウザ自動化
  （`test-rendered-view.mjs` / `test-horizontal-scroll.mjs` / `test-narrow-screen.mjs`）が無い

- 写しの作成が終わる前に SIGKILL されると、`sessions/` に 0 バイトの `.lock` だけが残る
  （本体の `.session` は無い）。計測スクリプトのようにプロセスを即座に落とす使い方で起きる。
  復元の一覧には出ず実害は無いが、掃除もされない


## やりたいこと（未着手）


## UX 改訂 v2 で残したこと

- `R-ORIGIN` の辿り直しの深さの上限（8 段）には、まだテストが無い。マージを 9 段入れ子に
  した中で消した行の由来が「特定できない」になるフィクスチャで確かめられるが、そのフィクスチャを
  作る手間が大きいので見送っている
- 構文ハイライトの配色（syntect のテーマ）はテーマのプリセットと別で、light / dark の 2 つ
  しかない。solarized のプリセットでもハイライトの色は light / dark のまま

## リリース

- 最新のリリースは v0.1.7。変更の記録は `CHANGELOG.md`
