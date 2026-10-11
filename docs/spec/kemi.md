# kemi 仕様

- Status: 承認済み（2026-09-10）。UX 改訂 v2 承認済み（2026-09-11）。
  セッション改訂 承認済み（2026-09-18）。ライブ改訂 承認済み（2026-10-08）
- 用語の定義: `CONTEXT.md`
- 関連する仕様: [エージェントとのやりとり](agent-channel.md)、[動いているページのレビュー](live.md)
- 改訂 v2 を一文で: 差分を「読む・変更間を移る・見終えたと分かる・結果を確実に受け取る」
  を速く確実にし、コミット範囲を最終形とコミットごとの両方で読めるようにする。
- セッション改訂を一文で: submit せずに終えたレビューを当時の差分ごとディスクに残し、
  あとから続きを読めるようにする。
- ライブ改訂を一文で: submit までの間にエージェントと返信と発言で往復できるようにし、
  手元で動いているページにもコメントを付け、案を選べるようにする。作る順は、往復
  （agent-channel.md）→ 動いているページ → 案。

結果を一文で: **kemi は、変更をブラウザで快適に読ませ、人間が付けた行コメントと
suggestion を実行ターミナルへ 1 つの JSON として返す、単一バイナリのローカルレビュー
道具である。** submit までの間のエージェントとの往復は [agent-channel.md](agent-channel.md)、
動いているページのレビューは [live.md](live.md)。

各要件には「成功条件」（機械または人が観測できる確認）と「反例」（その要件が破られて
いると判定できる観測）を付ける。証拠の出所は [R-VERIFY](build.md#r-verify-検証) にまとめる。

## 各部

本体の仕様は責務ごとに次のファイルに分かれている。要件の節 ID（`R-…`）と、作らないもの（`P…`）・
委譲（`D…`）・却下（`R` と数字）の番号は本体の仕様全体で一意。作らないもの・委譲・却下は、
関係する責務のファイルの末尾に置く。

| ファイル | 中身 | 節 | 作らない・委譲・却下 |
|---|---|---|---|
| [distribution.md](distribution.md) | 名称、配布、スキルとリリース | R-DIST | P6 P12、D3 D10、R1 R17 |
| [input.md](input.md) | 入力モードと focus レイヤ、その他の入出力 | R-INPUT | D1 |
| [serve.md](serve.md) | サーバの配信モデルとライブリロード | R-SERVE、R-LIVE | P5 P10、D7、R2 |
| [view.md](view.md) | 差分の表示 | R-VIEW | P1 P2 P4 P11、D2 D6、R3 R4 |
| [reading.md](reading.md) | 読み進める操作（単位の切り替え、由来、移動、見た、コメント、重要） | R-UNIT、R-ORIGIN、R-NAV、R-SEEN、R-COMMENT、R-FOCUS | P3 P8 P9、D8、R6〜R13 |
| [render.md](render.md) | 描画表示 | R-RENDER | P15〜P21、D15 D16、R25〜R27 |
| [narrow.md](narrow.md) | 狭い画面 | R-NARROW | P22〜P26 |
| [output.md](output.md) | エージェントに返す出力（submit の契約、結果ファイル、digest） | R-SUBMIT、R-RESULT、R-DIGEST | P13、D4 D9、R14〜R16 R18 R19 |
| [session.md](session.md) | セッションの保存と復元 | R-SESSION | P14、D12〜D14、R20〜R24 |
| [build.md](build.md) | 構成、依存、検証の出所 | R-WS、R-DEPS、R-VERIFY | D5 D11 |
