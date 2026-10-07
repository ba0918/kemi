# docs

kemi の文書の入口。各ディレクトリの役割と、いま正しい文書の一覧。

| ディレクトリ | 役割 | いまの文書 |
|---|---|---|
| `spec/` | 仕様。kemi が何をしなければならないか。契約（CLI、JSON、性能条件）の正典 | [`spec/kemi.md`](spec/kemi.md)（本体）、[`spec/agent-channel.md`](spec/agent-channel.md)（エージェントとの往復）、[`spec/live.md`](spec/live.md)（動いているページのレビュー）、[`spec/live-compare.md`](spec/live-compare.md)（比べる相手） |
| `design/` | 見た目の参照。仕様が採用したモックと、その比較に使った改訂前の画面 | [`design/ui-mock-v2.html`](design/ui-mock-v2.html)（`R-VIEW`）、[`design/ui-mock-conversation.html`](design/ui-mock-conversation.html)（会話パネル）、[`design/ui-mock-live-v2.html`](design/ui-mock-live-v2.html)（`--live`。前の参照 `ui-mock-live.html` に代わる。前のファイルも残してある） |

用語の読みの正典はルートの [`CONTEXT.md`](../CONTEXT.md)、残課題はルートの
[`TODO.md`](../TODO.md)、版ごとの変更はルートの [`CHANGELOG.md`](../CHANGELOG.md) にある。
