# kemi ランディングページ

`index.html` をブラウザで開くと確認できます。ビルド・インストール不要。
HTML/CSS・画像と、言語切替用のローカルJavaScriptで構成します。
外部フォント、外部スクリプト、フォーム、解析は使いません。
リンク先はGitHubの公開ドキュメントです。

公開先: https://ba0918.github.io/kemi/
`main` の `site/` または `.github/workflows/pages.yml` の変更でGitHub Pagesへ公開します。
Actions画面の `Pages` から手動実行もできます。公開対象は `site/` だけです。
既存の `github-pages` 環境のmainブランチ制限を利用します。

有効な共通キー ba0918-language（en / ja）、このLPの旧キー kemi-site-language、英語の順に読みます。
旧キーを共通キーへ自動移行せず、読み取り時には保存しません。
ヘッダーの EN / 日本語 で明示的に切り替えた場合だけ、共通キーへ保存します。
保存が許可されない場合は現在の画面だけ切り替えます。
同一originのTOPやほかのLPには次の遷移・再読込で選択が引き継がれます。
JavaScript無効時は英語を表示します。file URLでの保存可否はブラウザに依存します。
本文・操作・代替テキスト・文書言語・タイトルを揃えて切り替えます。
製品UIの英語は元のまま、サンプルデータは日英それぞれを撮影しています。
実画面は公開 v0.3.2 のWindowsバイナリを起動し、架空のmanifestと初期Explanationを
渡してChromiumで撮影しました。日英のデスクトップは1440×650、モバイルは390×780です。
実際の `kemi wait`、Hand to agent、`kemi reply` で質問と返答を表示しています。
デスクトップは差分とConversation、モバイルはConversationのシートを示します。
製品の動作確認全般を代替するものではありません。
JSONは形式3の説明用の抜粋です。省略された必須フィールドもあるため、完全な結果を
処理する際は[結果の参照資料](../skills/kemi/references/results.md)を参照してください。

確認元: リポジトリの README.md / README-ja.md / PROJECT.md / web/、
skills/kemi のExplanation・会話・結果の参照資料、docs/spec/ の現行契約。
commit `436d362d7fb5145de085ff5bbd50735f3331314d` の実行コードとweb/はv0.3.2と一致します。
初期Explanationは `--explanation <file>` で起動時に保存します。
起動前に `explain --context` を呼ぶ手順はありません。
`--resume` による新しい差分へのコメント引き継ぎは紹介していません。

ページ中の開閉はブラウザ標準の details/summary、移動はアンカーです。
320px・390px・デスクトップで確認。横幅600px以下で実UI画像をモバイル版へ切り替えます。
スクリーンショット内の数値は架空の差分の行数で、利用実績ではありません。

ローカルとPages共通の確認入口は python3 site/check.py（Python 3.11+ / Node.js 18+）。
ビルドは引き続き不要です。日英の翻訳キー、JS構文、言語の保存契約、ローカル資産と
アンカーを確認します。外部リンク先への通信確認は行いません。
見た目を変更した場合は日英1440 / 390 / 320pxと、代表CTA、同一originのTOPからの遷移を確認します。
