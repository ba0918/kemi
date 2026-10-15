# EXPLANATIONに伴う既存契約の改訂

- Status: 承認済み。承認対象は `55ba772` の三文書の契約本文と、その後に加えた説明コメントの `selection`（[説明](explanation.md#r-explanation-comment-対象と更新後の会話)）。
- 承認対象: [説明](explanation.md)、[共通パーツ](content-parts.md)、この文書。
- この文書は変更する契約だけを持つ。対応する正典節からこの文書へリンクし、改訂部分の正典を一箇所に保つ。

## 現行との対応

| 正典の節 | 現行 | この承認セットの変更候補 |
|---|---|---|
| kemi R-INPUT-6 | 初期説明の指定なし | `--explanation <file>` を追加 |
| kemi R-SERVE / R-LIVE | 専用の差分表示版tokenなし | サーバ現在版と画面のコード表示版を分離。contextはサーバ現在版 |
| kemi R-VIEW / live R-PAGE-VIEW | Code、liveのPage、共通Conversation | EXPLANATIONと見出しリストを追加。Codeのグループ単位とは分離 |
| kemi R-COMMENT / R-SUBMIT | File/Page、result版2 | explanation対象、parts、説明の保存情報を追加し版3にする |
| kemi R-RESULT | 過去結果をそのまま返す | この意味を維持。新しく作る結果だけ版3 |
| kemi R-SESSION / live R-PAGE-SESSION | 版3を書く、版2・3を読む。liveは会話なしなら保留で削除 | 版4を書く、版2・3・4を読む。説明だけでもliveを保持。凍結復元とlive再取り込みのtokenを区別 |
| agent R-AGENT-CLI / R-AGENT-LINK | wait/reply、同一利用者のloopback接続 | explain/contextを同じ接続保護で追加 |
| agent R-AGENT-EVENTS | wait版2 | wait版3へ統一。コメント・返信・発言はresult版3と同じ形 |
| agent R-AGENT-WRITE | body、variants、applied | bodyかpartsの互換入力を追加。既存の案の対象制限は維持 |
| agent R-AGENT-HAND | 渡す時点の値をイベントに保存 | 新contentの保存上限を全体で確認し、拒否時は未渡し状態を維持 |
| CONTEXT コメント/quote/outdated/見方/版 | 説明対象の定義なし | explanation.mdの用語候補と対象別の読みを追加 |

見た、File/Pageの対象保存、resolve、handのまとめ方と再配達、submitの一回性、approvalのbytes/identity、suggestionの適用主体は現行正典に従う。
説明が追加されても、レビューの承認対象を説明の文字列へ置き換えない。
現行R-SESSIONの一般状態保存失敗を警告にする契約は維持する。
新しい説明更新の受理と差分表示版tokenの外部への発行には、下記の原子的保存の成功条件を追加する。

## R-EXPLANATION-CLI 初期投入と全体更新

### 要件

- 起動時の `--explanation <file>` はUTF-8 JSONを一つ読む。通常の入力モードすべてで使える。
  `--digest` / `--result` / `--resume` / wait / reply / explainとは組まない。stdinを奪わず、manifest stdinとも組める。
  ファイルは `{sections:[...]}` のみを持ち、基点と説明版は初期レビューからサーバが設定する。
  起動前に知らないreview tokenを要求しない。読み込み・検証に失敗したら終了コード2でレビューを開始しない。
- `kemi explain <review-id> [--file <file>]` は説明全体の更新用JSONをstdin、または指定ファイルから一つ読む。
  引数で本文を渡さない。入力は `{expected_revision,base_display_version,sections}`。
  expected_revisionは現在の説明版と等しい非負整数。未作成の初版を途中で追加するときは0。
  説明版は0〜9,007,199,254,740,991の整数とし、次の番号を表せない更新は拒否する。
  base_display_versionは同一reviewの既知token。
  新版番号はサーバが現在値+1とする。同じ内容の再送でも受理すれば進む。
- `kemi explain <review-id> --context` はstdinを読まず、stdoutにcontextのJSON一つを出す。--fileとは排他。
  contextは `{kemi:3,review,diff_display_version,explanation_revision,explanation,references}`。
  explanationは現在の説明版の保存値またはnull。referencesは `{code:[...],pages:[...]}`。
  code各項目は `{unit,group_id,path,old_present,new_present,old_lines,new_lines}`。
  presentは片側の有無を表すboolean。片側なし・行数未取得は行数nullとし、行参照には非nullの行数が必要。
  pages各項目は登録済みの `{url}`。非liveでは空配列。
  サーバの現在メタデータから作る対象の地図であり、差分本文や全ファイル固定の提供ではない。
  画面が取り込んだCode版とは別で、context取得によってその画面のCodeを更新しない。
- 初期保存値・更新成功時のstdoutは下の形。未知フィールド、重複JSONキー、誤った型、上限、不正参照、版の競合は全体を拒否する。
  拒否ではstdout空、理由はstderr、終了コード2。成功/contextは終了コード0。
  更新失敗で説明、版番号、コメント、未配達イベントの既存状態を変更しない。
- 同時更新は直列に比較・保存し、同じexpected_revisionの二要求は片方だけ受理する。
  submitと説明更新も直列化し、submitが先なら説明を拒否、説明が先なら確定結果に新版を含める。
- 保存先とagent endpointを持てないレビューには途中explainを提供しない。
  初期--explanationを明示したときも説明を永続保存できなければ開始を拒否する。
  新しい説明更新は必要な保存の原子的差し替えに成功してから受理・通知する。失敗時は旧状態を残す。
- explainも同じマシン・同じ利用者、Originなし、loopback、別agent tokenによる既存の接続保護を使う。
  内部APIのURLは既存DA1と同じく実装委任。認証tokenをcontextに出さない。
  最初の引数 `explain` はサブコマンドとして扱う。manifest名は `./explain` と書く。
- 説明の全体更新は会話への返信とは別。必要な返信は `kemi reply` を使う。
  explain/contextだけではagentの返事済み判定、handの行、10分の時計を変えない。
  SSEで画面へ説明追加・更新と基点を知らせ、各画面が自分のコード表示版との鮮度を判定する。
  説明SSEでCodeを自動更新しない。欠落後の説明再取得もCodeの取り込みと分離する。
  explain自体をwaitへの新しい会話イベントとして流さない。

### JSON値の例・提案

更新入力。`01K00000000000000000000000:1` は型に沿った架空の既知tokenで、実際にはcontextの値を使う。

```json
{"expected_revision":0,"base_display_version":"01K00000000000000000000000:1","sections":[{"id":"overview","title":"全体","blocks":[{"id":"intent","part":{"type":"text","body":"まず変更の目的を読む。"},"references":[]}]}]}
```

更新成功のstdout例。

```json
{"revision":1,"base_display_version":"01K00000000000000000000000:1"}
```

保存される説明版のJSON値の例。

```json
{"revision":1,"base_display_version":"01K00000000000000000000000:1","sections":[{"id":"overview","title":"全体","blocks":[{"id":"intent","part":{"type":"text","body":"まず変更の目的を読む。"},"references":[]}]}]}
```

### 成功条件

- manifest stdinと別の説明ファイルで開始でき、途中explainで版が1から2になる。
- 二つのexpected_revision=1の同時更新は一方だけ成功し、もう一方が再取得できる競合理由を示す。
- 不正な参照、保存先障害、submit後の更新で元状態が変わらず、stdoutに成功JSONが出ない。

### 反例

- 部分更新後に不正な参照を見つける。保存失敗でも新版を成功応答する。context取得で返事済みになる。

## R-EXPLANATION-RESULT resultとwaitの版3

### 要件

- 新しく生成するsubmitの結果とwaitの外側は常に `kemi:3`。説明がないレビューも、版2・3のセッションを復元したレビューも同じ。
  digest/manifestの契約の版はこの変更で進めない。
  過去resultを `--result` で読む場合は保存時の契約版と元文字列のまま出す。再シリアライズして版3にしない。
- resultの従来top-levelキーを残し、`diff_display_version` と `explanation` を必ず追加する。
  explanationなしはnull。ありは `{current_revision,versions}`、versionsは説明版の保存値を版番号の昇順で並べる。
  現在版と、コメント・引用・未配達イベントが参照する旧版を含める。説明を参照するresultが単独で読めること。
  resultのdiff_display_versionはsubmit時のサーバ現在メタデータ版で、全画面がその版を取り込んだという意味ではない。
- コメントは既存キーを維持し、`target_type` と `explanation` を必ず追加する。
  target_typeは `file` / `page` / `explanation`。
  File/Pageは従来の意味のままでexplanation=null。Fileはpage=null、Pageは従来どおりファイルの位置がnull。
  explanation対象はgroup_id/group_title/path/side/start_line/end_line/page/suggestionがnull、quoteは[]。
  explanationは `{revision,block_id,quote,selection}`。quoteは `{section_title,block}` の保存時の完全な値。
  selectionは保存時の `{text,occurrence}` かnull（[説明](explanation.md#r-explanation-comment-対象と更新後の会話)）。
  outdatedは現在版との比較値。元のrevisionと引用は変えない。
  コメントbodyはstring、replies/resolvedなどの必須性と並びは現行どおり。
- 全返信・発言の出力にはbodyとpartsを必ず両方出す。
  旧plainはbody=string、parts=null。新partsはbody=null、parts=非空配列。
  これは出力規則であり、入力の両キー混在禁止とは区別する。空のplain bodyは空文字のまま。
  返信のvariants/chosen/appliedの省略でなく[]/null/nullにする既存規則は維持する。
  Page案は従来の許可対象と適用契約のまま。説明対象へ案を追加できない。
- waitのeventsの型、順、handed/chosen/submitted、削除コメントはidのみという契約は維持する。
  handed内のコメント・返信・発言は版3のresultと同じ形。
  Page imageがwaitのみ保存画像のパスで、submitはnullという既存の違いは維持する。
  submitted.resultも版3。過去の未配達イベントは旧bodyをplainとして版3の形で出す。
- handで新contentを含むイベントを保存する場合は保存予定の追加部分の上限を先に確認する。
  超過時は全体を拒否し、未渡しのコメント・返信・発言、配達済みの位置を変更しない。
  人間には理由を表示する。受理したイベントは従来どおり受け取り確認まで保持する。

### JSON値の例・提案

説明コメント一つを含む架空のresult。これは完成したJSON値であり、schemaの型記法ではない。

```json
{
  "kemi": 3,
  "title": "更新のレビュー例",
  "verdict": "changes_requested",
  "approval": [],
  "diff_display_version": "01K00000000000000000000000:1",
  "explanation": {
    "current_revision": 1,
    "versions": [{"revision":1,"base_display_version":"01K00000000000000000000000:1","sections":[{"id":"overview","title":"全体","blocks":[{"id":"intent","part":{"type":"text","body":"まず変更の目的を読む。"},"references":[]}]}]}]
  },
  "comments": [{
    "id": "c1", "target_type": "explanation",
    "group_id": null, "group_title": null, "path": null, "side": null,
    "start_line": null, "end_line": null, "quote": [], "body": "前提を補足してほしい。",
    "page": null,
    "explanation": {"revision":1,"block_id":"intent","quote":{"section_title":"全体","block":{"id":"intent","part":{"type":"text","body":"まず変更の目的を読む。"},"references":[]}},"selection":{"text":"変更の目的","occurrence":0}},
    "replies": [{"id":"r1","author":"agent","body":null,"parts":[{"type":"code","language":"pseudocode","role":"example","body":"read_premise()"}],"variants":[],"chosen":null,"applied":null}],
    "resolved": false, "outdated": false, "suggestion": null
  }],
  "messages": []
}
```

### 成功条件

- File/Pageのみ、説明コメントあり、説明なし、旧セッション復元の各新result/waitがすべて版3を返す。
- 保存された古いresult版1・2の `--result` は元文字列とverdict終了コードを保つ。
- 図付き返信と旧plain返信が上記null規則で区別でき、result単独から旧図の全データを読める。

### 反例

- explanationを架空のファイルpathとして出す。partsの文章をbodyへ平坦化して図を失う。
- 説明なしの新waitだけ版2を返す。--resultで古い結果を書き換える。

## R-EXPLANATION-SESSION 保存と復元

### 要件

- セッション形式は版4を書き、版2・3・4を読む。既存の版2返信・通し番号補完を維持する。
  旧bodyはplain contentとして読む。説明はなし、説明版は0とする。
  旧形式の読み込みでは差分表示版の最初の世代を補完する。非liveはその世代を凍結版にも使う。
  読み込みだけで旧ファイルや固定済みpayloadを書き換えない。初回の状態保存で版4とともに保存する。
  補完したtokenを外部へ提供する段階は発行の状態保存に当たり、下の原子的保存規則に従う。
  形式と世代の補完だけでは元の作成・最終更新時刻を進めない。
- 版4は現在説明、必要な旧説明版、元引用と `selection` を持つ説明コメント、parts返信・発言、未配達イベントを保存する。
  サーバ現在差分表示版と、既存payloadの起動時メタデータに対応する `frozen_display_version` を小さい識別子として保存する。
  frozen_display_versionは写しに入れるメタデータの版を記録し、非liveの写し完成時に確定する。liveではnull。
  全token一覧の代わりに `issued_display_generation` を10進文字列一つで保存する。現在版・凍結版の世代はこの値以下でなければならない。
  保存可能なレビューでは新世代を外部へ返す前に、発行済み最大世代と新しい現在版を原子的にsessionへ保存する。
  世代情報だけの保存では説明内容を増やさず、説明側の容量検証をCode更新の可否判定として再適用しない。
  この保存に失敗した取り込みは理由を示して旧サーバ現在版を残すが、レビュー自体は終えない。
  保存先を持たない従来レビューはメモリ内の同じ採番規則を使い、もともと使えない復元やexplainを提供したとは扱わない。
  入力元の説明ファイルがなくても説明と会話を復元する。復元だけでは説明版を進めない。
- 非liveの版4復元は既存の凍結payloadを使い、コード表示版を保存済みfrozen_display_versionとする。
  元プロセスの最後のサーバ現在版と凍結payloadの版が違っていれば、後者を選ぶ。新tokenを発行せず、説明の基点を付け替えない。
  復元したサーバの現在版もこの凍結版となるが、発行済み最大世代は減らさない。
- live復元は現行R-PAGE-SESSIONどおり作業ツリーの新しい一覧を取り込む。
  取り込み成功時に以前の発行済み最大世代の次のtokenを発行する。以前の説明・基点・コメントは保持し、新しいCode版との不一致を示す。
  再取り込みに失敗した間はCodeを利用不可とし、以前のtokenを新しい一覧の版として使わない。
- liveの保留保持条件を「コメント・返信・発言・説明が一つもない場合だけ残さない」へ改訂する。
  説明だけがあるliveも一覧・復元の対象にする。元の説明ファイルは復元の入力にしない。
- 説明・引用・会話は状態として `<id>.session` に保存する。
  差分の写しは従来の `<id>.payload`、liveの再接続と画像等は従来 `<id>.files/` の契約を使う。
  説明更新のために差分payloadを読み直したり書き直したりしない。
  保存と復元の権限、ロック、結果保存後の削除順、liveの例外は既存正典に従う。
- 全ファイル・全世代の参照一覧をsessionに追加しない。contextは現在のレビューのメタデータから必要時に構成する。
  過去の索引は永続保存せず、旧基点のCodeリンクは説明仕様の利用不可表示と画面一覧による解決を使う。
  Seen/resolve等の一般状態保存で読み書きするものに、ファイル数や表示世代数に比例する配列を追加しない。
  追加するのは現在版・凍結版の小さい識別子であり、保存費用をレビューのファイル数や過去世代数に依存させない現行R-SESSIONを維持する。
  新しい索引ファイル・世代ログ・保存ディレクトリは導入しない。
  版4の小さい世代情報は既存sessionの容量集計・原子的差し替え・ロック・掃除・submit時削除に含める。
  一般状態保存は既存payloadを読み書きせず、contextも既存メタデータだけから作り、全ファイル内容を読み取り・固定しない。
- 古い説明版は、現在版、残存コメントの元対象、未配達イベント、開いている説明/参照からの戻り先のどれからも参照されていない場合だけ、古い順に容量確保のため削除できる。
  ブラウザで旧版を開く際はその版の保持を要求し、戻り先を解放するまで削除対象にしない。
  保留後はブラウザの一時的な保持は解除してよいが、コメント・未配達イベントの参照は保つ。
  版番号は再利用しない。削除済み版への新コメントは理由つきで拒否する。
- [共通パーツの上限](content-parts.md) は追加保存部分に適用する。
  全セッション500MB・100件とロック中除外等の既存掃除は維持し、追加状態も合計サイズに数える。
  差分payloadの20MB、live filesの20MBという既存判定を追加状態の16MiBと混同しない。
  全セッションの掃除は従来どおりセッション単位で消し、一つの残存セッション内の引用だけを間引かない。
  追加保存上限に達した要求は黙って切り捨てず、対象と理由を表示して既存状態を残す。
- 既存の一般状態保存でI/O失敗を警告する規則は維持し、警告後の未保存状態を復元保証済みとは表示しない。
  新しい説明更新の保存失敗はR-EXPLANATION-CLIの原子的拒否に従う。
  世代発行の保存失敗はこの節の旧サーバ現在版を残す規則に従う。未保存の世代を公開して復元後に番号を再利用しない。
  対応外の形式の版・壊れたpartsを推測で読み替えず、復元を理由つきで拒否する。

### 成功条件

- 版2・3の固定保存値を読み、旧bodyの文字列と既存対象を保ち、次の状態保存で版4を書く。会話パネルの表示は [R-CONTENT-MARKDOWN](content-parts.md#r-content-markdown-textと画像) に従う。
- 版1の図へのコメント、版2の説明、図付き返信、未配達handを保留し、元説明ファイルを除去して復元してもすべて読める。
- 会話のないliveに説明だけを保存して保留し、元の説明入力ファイルを削除して復元すると、同じ説明版・図・基点を読める。
  同じreviewの新しいCode取り込み版とは不一致になる。説明も会話もないliveは従来どおり残さない。
- 非liveの凍結payloadがD1、最後に取り込んだサーバ現在版がD2、説明基点がD2の状態で保留する。
  復元のCodeとcontextはD1、説明基点はD2のままで不一致となる。既知判定の最大世代はD2以上を維持する。
- liveでD1基点の説明を保留し、作業ツリーを変更して復元すると、Codeの一覧取り込み成功でD2を発行する。
  説明版は増えず、元引用も変わらない。新しい一覧を取り込んだ画面だけがD2との不一致を示す。
- ファイル数と表示世代数だけを増やし、会話・説明が同じ二セッションでSeen/resolveを保存する。
  新しい世代情報は固定個数の小さい値だけで、sessionにファイル一覧・世代配列がなく、payloadへのI/Oもないことを観測する。
- 参照中の旧版を削除せず、無参照版だけを削除できる。容量拒否後に保存済みコメントとイベントが一致する。

### 反例

- 復元のため元の説明ファイルを要求する。古い版の掃除で図の引用や未配達会話を失う。
- liveの新しい作業ツリーの一覧を旧tokenで表示する。凍結payloadの版を最後のライブリロード版へ付け替える。
- 説明だけのliveを会話なしとして消す。Seenを保存するたびに全世代の参照索引を書き直す。

## 文書照合の根拠と承認後の反映先

現行の事実は次の読みで確認した。新契約の動作確認ではない。

- `domain/review.rs` は `CommentTarget::File/Page` と `Reply/Message.body: String`。
- `domain/agent.rs` は本文65,536バイト、エージェント返信50件、発言200件、全件検証後の書き込み。
- `session/encoding.rs` は書く版3、読む版2・3。
- `api/submit.rs` と `src/agent.rs::print_events` はそれぞれresult/waitの `kemi:2`。
- `src/main.rs::print_result` はverdictを読んでから保存された文字列をそのまま出す。
- `crates/kemi-core/src/domain/content.rs` はCRLF/CRをLFへ正規化し、行分割では最後の改行一つを除く。
  `domain/comment.rs` のFile hashはその行全体に基づく。説明の内容一致はこの規則を使わない。
- `source/mod.rs::read_side` はDiskを読み直し、`web/assets/api.js::getFile` はfile_idと描画オプション等だけを送る。
  `web/assets/state.js::fileCacheKey` はfile_id、テーマ、ハイライト指定だけを鍵にする。
  現行の識別子にはレビュー全体の差分表示版や読み取りバイトの保証はない。
- `web/assets/views/conversation.js` の会話は共通で、bodyもpartsも [共通パーツの表示規則](content-parts.md) に従う。
- shoryoの説明型はpartのID・見出しを持たず、`web/components/explanation.js` はtextをplainに表示する。
  図の型・構造上限・描画失敗の元JSON・パン操作を実コードと説明referenceで照合した。
  公開出典は [ba0918/shoryo](https://github.com/ba0918/shoryo) 内の `crates/shoryo-core/src/explanation.rs`、`web/components/explanation.js`、`skills/shoryo/references/explanations.md`。
  Cargo.tomlのrepository欄とMITのLICENSEを確認した。調査したcloneの配置は実装要件ではなく、公開版と調査版のcommit一致は未確認。
- 現行 [R-PAGE-SESSION](live.md#r-page-session-保留と復元) は会話なしのliveを保留で残さず、復元で新しい作業ツリーを読む。
  説明だけの保持と復元後の新tokenはこの節への改訂候補として明記した。
- 現行 [R-SESSION](session.md#r-session-セッションの保存と復元) は凍結メタデータをpayloadに分離し、一般状態保存をファイル数から切り離している。
  今回は参照索引を永続保存しない方式を選び、その性能契約を緩めず、小さい世代情報だけをsessionへ追加する。

上の対応表の正典節からこの改訂を参照し、用語は `CONTEXT.md` に置く。CLI/JSONを実装する変更では `README.md` と `skills/kemi/` の派生説明も同時に更新する。
実装状況はコードと実装計画で確認する。

## 未決

利用者判断が必須の未決は残していない。値・名前・保持方式はこの承認セットの契約に従う。
図の見た目と容量の性能は未検証。今回の根拠は仕様とソースの照合であり、ブラウザやサーバの動作確認ではない。
