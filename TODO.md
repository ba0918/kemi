# 残課題

最終更新: 2026-09-11（kemi v1 と UI 改訂のサイクル完了時点）

## レビューで記録のみになった指摘（修正要求ではない）

kemi v1（main にマージ済み）:

- id 12: ハイライトのファイル単位の手動指定が再読込で消える
- id 16: A/D が約 5 万件を超える worktree で `git` の引数長（E2BIG）で起動できない
- id 26: digest のグループ・ディレクトリ上限が、余裕があっても常時適用される
- id 28: e2e `interrupt_exits_130_without_json` が SIGINT のタイミングで稀に落ちる
- id 29: 離れた 2 つの変更があるファイルで、ハンク折りたたみの中間行が反転する（既存バグ）
- id 36: split 表示の replace 旧側に `+` が無く、旧側に行コメントを付けられない
- id 39: コメント応答待ちに新しく開いたエディタが、先行応答の到着で閉じられる（狭い経路）
- id 42: README の digest サンプルに `top_files_omitted` が無い

UI 改訂（main にマージ済み）:

- id 4: `why` / `watch` を持たないグループでも畳みトグルが出て、押しても何も隠れない
- id 5: 畳みトグルをキーボードで操作すると、再描画でフォーカスが body に落ちる

evidence と oracle の詳細はローカルの `.agents/artifacts/reviews/kemi-v1.json` と
`.agents/artifacts/reviews/kemi-ui-fix.json` にある（git 管理外・この環境のみ）。

## 納品後の環境更新

- `~/.local/bin/diff-review`（旧 Python ツール）を kemi に置き換える
- `diff-review-viewer` スキルを更新する（kemi の呼び方、stdout の JSON 契約、`--out`
  廃止、suggestion はエージェントが適用する）

## リリース

- `v*` タグを切って GitHub Releases に配布する（`ba0918-release`）
- `mise use -g github:ba0918/kemi` の実アセット確認もその時に行う
- Windows 対応は将来の検討
