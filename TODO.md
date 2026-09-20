# 残課題

最終更新: 2026-09-20（v0.1.9 の公開の後に更新）

## 記録のみの指摘（修正要求ではない。詳細はローカルの `.agents/artifacts/reviews/*.json`）

LAN 公開 `--bind`（`lan-bind.json`）:

- Host 許可のループバック枝と「バインドした具体アドレス」の枝を直接確かめるテストが無い。
  製品の挙動は手動確認で正しい（`--bind 192.168.1.2` で 200 / 403）
- `ServeParams.share_address` に `Some(0.0.0.0)` を渡すと wildcard の Host を受理し、URL にも
  `0.0.0.0` を使う。CLI 経由では到達しないが、pub な値の前提がコードに現れていない

kemi v1 と UI 改訂（`kemi-v1.json`、`kemi-ui-fix.json`）:

- ハイライトのファイル単位の手動指定が再読込で消える
- A/D が約 5 万件を超える worktree で `git` の引数長（E2BIG）で起動できない
- digest のグループ・ディレクトリ上限が、余裕があっても常時適用される
- コメント応答待ちに新しく開いたエディタが、先行応答の到着で閉じられる（狭い経路）
- 畳みトグルをキーボードで操作すると、再描画でフォーカスが body に落ちる（キーボードでの
  確認はしていない）

描画表示（`rendered-view.json`）:

- `content_skipped` が立っていても `api/render` / `api/image/review` を直接叩けばディスクを読む
  （ページは呼ばない）

狭い画面（`narrow-screen.json`）:

- 上部バーの 2 段目で進捗を数字だけにする判定（`fitProgress`）が幅を見ないので広い画面でも走り、
  描画のたびに強制レイアウトが 1 回増える（見た目は変わらない）
- 描画表示のブロックのタップ（click）が広い画面でも登録される（mouseover が先に届くので
  観察される差は無い）
- 幅をまたいだときに、閉じている popover にも `hidePopover()` を呼ぶ。WebKit で例外にならないかは
  未確認（Chromium と利用者の実機では通っている）
- `scripts/test-narrow-screen.mjs` の 2 回目の起動に try/finally が無く、途中で落ちると kemi と
  agent-browser のセッションが残る

リリース前の小さな直し（`prerelease-fixes.json`）:

- 2 列表示のどちらの側に `+` を出すかは仕様のどの節にも無い。`R-COMMENT` の「旧側（削除された行）
  にコメントでき」から含意されるだけで、規則は `model.js` の `plusSides` の JSDoc と 2 本の
  テストにしかない
- README の `### Exposing kemi to other devices` に残る本文が狭い画面の説明で、スマホの挙動が
  LAN 公開の節を読まないと見つからない

api の子モジュールへの分割（`api-split-2.json`）:

- 由来のエンドポイント一式（`OriginQuery` / `origin` / `unknown_origin` / `origin_json`、約 78 行）は、
  他から参照されず子へ出せる条件を満たすが、小さいので親に残した
- `content::normalize(&String::from_utf8_lossy(..))` が `api/file.rs` と `api/rendered.rs` の
  `side_text` に同形で 2 つある
- ハイライトの `capable` / `forced` / `enabled` の決め方と JSON が、`api/file.rs` と
  `api/rendered.rs` に同形で 2 つある
- `source_content(..).await?.ok_or_else(file_not_found)?` が 3 ファイルに合わせて 6 か所ある

セッションの写しの分割（`session-payload-file.json` / `resume-unreadable-version.json`）:

- `read_meta` が `<id>.session` を丸ごと読んでから版を検査する。旧実装にあった「meta の確保を
  ファイルの実際の大きさで頭打ちにする」保護が無くなり、読めない版の残骸（最大 20 MB）も
  拒否のためだけに全部メモリへ載る
- `persist_with_copy` が `<id>.session` を書く前にメモリ上の写しの状態を「使える」にする。
  セッションの書き込みが失敗すると、ディスクの不変条件は保たれたままプロセスだけが復元可能だと
  信じ、中断時に `resume with` の行を出す
- `has_unreadable_version` が `.session` で終わる名前を無条件に開くので、そこに名前付きパイプ
  （unix の FIFO）が置かれていると `File::open` が返らない。利用者自身の保存領域への細工が要る
- `list_sessions` の `sessions.is_empty()` の枝が、唯一の呼び出し元からは到達しない
- `StateDto.last_comment` に `#[serde(default)]` が残っている。版 2 だけを読む前提の表れ方が
  `FileDto` と揃っていない
- `encode_copy` の `body` が内容の分の容量を確保せずに伸びるので、追記のたびに償却的な再確保が
  起きる。分割前の `encode_contents` でも同じで、この変更が持ち込んだものではない
- `encoding.rs` の説明が実際とずれている 2 か所。`decode_copy_body` / `decode_contents` の doc が
  `#[cfg(test)]` になった関数を「の逆」として参照している。`push_copy_meta` の「写しを二重に
  持たない」は実際より強い保証に読める（内容のバイト列は `copy.contents` と `body` と gzip 結果に
  同時に存在する）
- README の復元できない理由の列挙が「copy is unfinished or too large」の 2 つのままで、写しの
  ファイルが無い場合が入っていない
- CHANGELOG の 0.1.9 の破壊的変更の節が、両方の意味が隣り合う一文で裸の「版」を使っている
  （`CONTEXT.md` は「製品の版」「セッション形式の版」の書き分けを求めている）

仕様の側に残っている食い違い（同上）:

- `<id>.payload` が「在るが壊れている」セッションを一覧に出すかを `R-SESSION` が決めていない。
  いまは一覧に出て、選ぶと終了コード 2。一覧から外すと `list` が写しを読むことになり、状態の
  保存ごとの費用が写しの大きさに依存する（性能上の意図とぶつかる）
- `R-SESSION` が入れ分けの理由として「状態を書く費用が、写しの大きさにもレビューのファイル数にも
  左右されない」と書くが、状態は見たの印をファイルごとに持つので、実測では 100 / 1,000 /
  10,000 ファイルで約 1.7 / 14 / 147 KiB とファイル数につれて増える。過大申告しているのは仕様の側

## アイデア（まだ仕様にしていない）

- ファイルヘッダのコメント件数を押すと、そのファイルのコメント一覧が出て、選ぶとその行へ移れる。
  上部バーの全体のコメント一覧（`R-VIEW`）はすでに行へ移れるので、それをファイル単位に絞った版に
  あたる。件数は今は表示だけで入口ではない。着手するなら仕様の文言から決める

## 既知の制限

- 写しの作成が終わる前に SIGKILL されると、`sessions/` に 0 バイトの `.lock` だけが残る（本体の
  `.session` は無い）。計測スクリプトのようにプロセスを即座に落とす使い方で起きる。復元の一覧には
  出ず実害は無いが、掃除もされない。写しを書いてから `<id>.session` を書くまでの間に落ちた場合は
  `<id>.payload` も残るが、こちらは掃除が回収する
- `R-ORIGIN` の辿り直しの深さの上限（8 段）には、まだテストが無い。マージを 9 段入れ子にした中で
  消した行の由来が「特定できない」になるフィクスチャで確かめられるが、作る手間が大きいので
  見送っている
- 構文ハイライトの配色（syntect のテーマ）はテーマのプリセットと別で、light / dark の 2 つしか
  ない。solarized のプリセットでもハイライトの色は light / dark のまま
- 描画表示の行の整列は空行を優先して対応づけるので、内容の違う段落が 1 つのブロックに
  重なって見えることがある。直すなら `crates/kemi-core/src/domain/diff.rs` の整列から
- `api/render` の応答は各側の全行を載せる（コメントの入力欄にそのブロックの元の行を
  見せるため）。10,000 行の Markdown で応答が約 2.7 MB になる。描画表示の上限
  （`R-RENDER`）がこの大きさの上限も兼ねている

## リリース

- 最新のリリースは v0.1.9（2026-09-20）。セッションの保存を `<id>.session` と `<id>.payload` に
  分けた破壊的変更で、セッション形式の版が 2 に上がり、版 1（v0.1.8 まで）のセッションは復元
  できない。以降の main に入っているのは済んだ計画の削除だけ。変更の記録は `CHANGELOG.md`
