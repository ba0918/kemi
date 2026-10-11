# 本文・返信・発言の共通パーツ 仕様

- Status: 承認済み。`55ba772` の [EXPLANATION](explanation.md) と [既存契約の改訂](explanation-contracts.md) と同じ承認対象。インラインの図のパンをドラッグと矢印キーで行う改訂も承認済み。
- 変更対象: `Reply` / `Message` の文章のみの契約、説明本文の新規契約。コメント本文は従来plainのまま。

## R-CONTENT-INPUT 入力と互換性

### 要件

- 返信・発言は旧 `body: String` または新 `parts: Part[]` の片方を受け取る。
  旧入力はプレーン表示を維持する。旧文字列をtextパーツへ変換してMarkdownとして解釈しない。
  新partsは1〜32個で、EXPLANATION/Code/Pageの返信・発言すべてで同じ読み順と表示を使う。
  人間の従来の文章入力もbodyで保存する。図付き発言を受け取る経路も同じparts規則を使う。
- 入力にbodyとpartsの両方がある場合、片方がnullでも拒否する。両方の省略、入力値nullも拒否する。
  旧bodyの空文字は従来同様受理する。partsの空配列は拒否する。
- パーツは次の型。任意フィールドは省略し、nullは拒否する。パーツ以下の未知フィールドを拒否する。

| type | 必須 | 任意 |
|---|---|---|
| text | body | なし |
| code | body, language, role | title |
| diagram | source, title, role | なし |
| sequence | title, role, participants, events | layout, canvas |
| flow | title, role, nodes, edges | canvas |

- roleは `proposal` / `example` / `confirmed`。作成者が意味を付ける表示ラベルで、kemiが事実確認した印ではない。
  languageはliteral、予約値 `pseudocode` は `Pseudocode` と表示する。
  codeのbody、diagramのsourceは実行もHTML解釈もしない。コードは読み取り専用でコピーでき、初版では構文ハイライトを必須にしない。
- 説明のIDや見出しはpartsに入れず、[説明ブロック](explanation.md)の外側に置く。

### JSON値の例・提案

架空のreply入力。roleは型付きの `example` であり、確認済み実装を表さない。

```json
{"writes":[{"type":"reply","comment_id":"c1","parts":[{"type":"text","body":"次は**構文の例**。"},{"type":"code","language":"pseudocode","role":"example","title":"更新の例","body":"if accepted:\n  replace_explanation()"}]}]}
```

旧入力のJSON値の例。`**` もそのままプレーンに表示する。

```json
{"writes":[{"type":"message","body":"**これは旧文章**"}]}
```

### 成功条件

- 同じparts返信を三つの対象のスレッドで同じ順・書式で表示し、result/wait/復元でも型を保つ。
- 旧bodyの `**文字**` は装飾されず、新textの同じ値は太字で表示される。
- bodyとpartsの混在、一つの不正role、未知フィールドが書き込み全体を拒否する。

### 反例

- 旧返信のHTMLやMarkdownを解釈する。Pageだけがpartsを失う。codeをsuggestionとして適用する。

## R-CONTENT-MARKDOWN textと画像

### 要件

- 新text.bodyはkemi既存のMarkdown構文・安全なリンクの規則を使う。
  [R-RENDER](kemi.md#r-render-描画表示) の生HTMLを文字として扱う規則も引き継ぐ。
  ファイル差分の整列、行対応、変更の印は説明のtextには付けない。
- 初版は外部画像の取り込みを行わない。新parts内のMarkdown画像は外部・相対パス・data URL等を問わずロードしない。
  インライン画像と参照形式画像の双方を、`Image not loaded` とalt文字列・元の参照先を文字として読める非画像の枠にする。
  altのない画像も参照先を文字として示す。画像src/srcset、CSS背景、画像取得APIへの要求を生成しない。
  生HTMLのimgも文字表示でありロードしない。一般リンクは既存の安全なリンク規則に従い、自動では取得しない。
- 既存rendererは画像を描画するため、その出力を無条件に流用しない。
  説明とparts返信・発言では画像無効の境界を設ける。ファイルのRenderedにある従来の画像表示契約は変更しない。
- Markdown描画が失敗したパーツだけ理由と元bodyをプレーンに表示する。別のパーツや会話を失わない。

### 成功条件

- EXPLANATIONと返信に外部/相対/参照形式/data画像、HTMLのimgを書いても画像要求は0件で、altと参照先を読める。
- 通常のRenderedファイルでは従来画像を表示でき、textのリンク・表・コードフェンスは安全に読める。

### 反例

- 外部画像だけを防ぎ相対画像を取得する。表示後にimgを消して最初の要求を発生させる。

## R-CONTENT-DIAGRAM 図の型と描画

### 要件

- この節の波括弧は型記法。`?` 付きだけが省略可能で、型内のJSON文字列は列挙値を示す。
- sequenceのparticipantは `{id,label,x?}` で配列順が左から右。
  eventはmessage `{type:"message",from,to,label,kind,gap_after?}`、alt `{type:"alt",branches,gap_after?}`、loop `{type:"loop",condition,messages,gap_after?}`。
  kindは `call` / `return`。branchは `{condition,messages}`。枠内messagesは同じmessage型で、枠を入れ子にしない。
  参照は同じ図のparticipant IDへ解決する。self messageを許可し、callは実線、returnは破線。
  layoutは `participant_gap` / `event_gap` / `self_loop_width` の任意整数で、既定は220/72/48。
  participantの既定xは `124 + index * participant_gap`。指定と既定を合わせた中心が厳密に増加すること。
- flowのnodeは `{id,kind,label,position?,width?,height?}`。
  kindは `start` / `end` / `process` / `decision`、positionは `{x,y}`。
  edgeは `{from,to,from_port?,to_port?,via?,label?}`、portは `north` / `east` / `south` / `west`。
  labelは `{text,position?}`。decisionを始点にするすべてのedgeは空白でない条件ラベルを持つ。
  node参照は図内で解決する。既定中心は `(160,100 + index * 160)`、幅180、decisionの幅220。
  高さは文字から決め、最小64、decisionは96。portはsouthからnorthが既定。
  via省略は両portのyの中点を使い、空配列は直線。明示座標を勝手に移動して修復しない。
- sequence/flowのcanvasは `{width,height}`。省略時は内容に余白24を足す。明示時は原点0,0の固定領域。
  実フォントで文字を測り、省略せず折り返す。図・文字・線・矢印頭が領域内に収まること。
  形と文字の重なり、文字の自分の形からのはみ出し、経路のノード内部への侵入を描画失敗とする。
  接触と線同士の交差は許可する。フォント・拡大率変更後も再検証する。
  ラベル位置・余白・描画検証の詳細は [shoryo公開upstream](https://github.com/ba0918/shoryo) 内の `skills/shoryo/references/explanations.md` と実コードを再利用候補とし、取り込み時もこの条件を守る。
- diagramはgridの文字列文法。`id = label`、`id = ? label`、`| a | b | . |`、`a -> b : label`。
  空行を無視し、重複定義、未定義参照、不明な非空行、不均等な行幅を拒否する。
  重複セルは最初の配置を使う。図のIDは空白を含まないtokenで、新IDのASCII規則とは別。
  gridラベル内のCR、Unicodeの行区切りU+2028/U+2029は拒否し、描画文法で失われる行を受理しない。
  gridの長いラベルは既存の3行・省略記号表示を候補とし、hover/tapで全文を読めることを維持する。
- sequence/flowはインラインのパン、拡大、全体表示、別表示での拡大縮小・移動・リセットを使える。
  インラインの拡大・縮小・全体表示・別表示はボタンで、パンはドラッグと矢印キーで行う。別表示では移動もボタンで行える。
  マウスだけでなくキーボードでもすべて操作でき、別表示を閉じると元のブロックへ戻る。
  説明の中での操作の置き方は [R-EXPLANATION-READING](explanation.md#r-explanation-reading-読む画面とコメントの入口) に従う。
  grid図は既存同様非対話表示でよい。コードと全図で入力内容を実行しない。
- 構造不正は保存前に要求全体を拒否する。ブラウザの幾何・計測の失敗はそのpartだけ理由と完全な元JSONを表示する。
  失敗した図には未完成の図と拡大操作を出さない。入力受理やroleは視覚・意味の正しさを保証しない。

### 成功条件

- call/return/self、alt、loop、条件分岐と戻り経路を型を失わず描画し、キーボードで移動・拡大できる。
- 未定義IDや条件のないdecision edgeを含む書き込みは全体を拒否する。
- フォント変更で収まらなくなった図は理由と元JSONを表示し、隣のtextとコメントは残る。

### 反例

- 描けるよう条件や枝を削る。矢印をノード越しに通して成功扱いする。失敗時にタイトルしか残さない。

## R-CONTENT-LIMIT 上限

### 要件

境界はすべて包含。文字数はUnicode scalar値の数で、バイト数はUTF-8。
新IDは `[A-Za-z][A-Za-z0-9_-]{0,63}`。上限はshoryoと現行kemiからの提案で、性能測定済みではない。

| 項目 | 上限 |
|---|---|
| text.body / code.body / diagram.source | 0〜16,384 / 0〜32,768 / 1〜32,768文字 |
| title / language / 新ID・参照ID | 1〜120 / 1〜64 / 1〜64文字。空白だけは不可 |
| 新図のlabel・condition | 1〜512文字、空白だけは不可 |
| sequenceのparticipants / 全message / frame / 最上位events | 1〜12 / 1〜96 / 0〜12 / 1〜96 |
| alt branches / 各branch・loopのmessages | 2〜8 / 1以上、全message上限内 |
| flow nodes / edges | 1〜32 / 0〜64 |
| viaの点数、一edge / 一flow | 0〜12 / 0〜768 |
| canvas幅・高さ / 座標x,y | 64〜8192 / 0〜8192、整数 |
| node幅・高さ / sequenceのgap・self幅 | 80〜640・32〜640 / 16〜512、整数 |
| grid nodes / edges / rows / 一rowのcolumns | 1〜64 / 0〜128 / 0〜64 / 0〜32 |
| grid ID・参照 / node・edge label | 1〜64 / 0〜512文字 |
| 一返信・発言のparts | 1〜32、全文字列値の合計524,288バイト以下 |
| 旧body、一コメント・返信・発言 | 65,536バイト以下 |
| 説明sections / 全blocks / 一blockのreferences | 1〜128 / 1〜512 / 0〜32。各sectionは1以上のblock |
| Code参照path / Page参照url / 差分表示版token | 1〜4096 / 1〜4096 / 1〜128文字 |
| 一説明の文字列値合計 / 正準JSONサイズ | 524,288バイト / 1,048,576バイト |
| 保持中の説明版数 / 説明本文履歴の正準JSON合計 | 128 / 8,388,608バイト |
| 説明・引用・parts会話・未配達イベントの追加保存部分 | 正準JSON合計16,777,216バイト |

- 文字列値合計は値の葉だけを数え、繰り返したtype、role、ID、portも含める。キー名は数えない。
  正準JSONサイズは型付き値からキーを辞書順、空白なし、非ASCIIをUTF-8、必要なエスケープだけで書いたサイズ。
  容量算定で文字列や内容の正規化はしない。引用やイベント内の重複も保存する分を数える。
- 既知tokenはreview所属と発行済み最大世代で判定する。4096個等の保持件数制限を設けず、説明の容量不足を理由にCode更新を拒否しない。
  全世代の参照索引は保存しない。説明側の上限は初期説明、説明更新、parts会話、対象コメント、イベント追加の保存予定部分にだけ適用する。
  一件でも超える要求は部分保存せず、理由と対象フィールドを示して既存状態を残す。
- 説明の旧版は [保存規則](explanation-contracts.md) に従ってのみ削除できる。コメントや未配達イベントを切り捨てて上限内にしない。
- エージェントの返信50件/コメント、発言200件、案8個、置き換え8か所、案内容256KiBは現行のまま。
  複数writesの合算により同じ制限をすり抜けない。空writesの既存の無操作契約も維持する。
  旧セッションの読み込みは新上限で黙って欠損させず、旧形式の契約で読む。
  追加保存部分の容量には新しい説明対象・parts内容・説明履歴と、それを含むイベントの保存表現を数える。
  既存bodyだけの会話やFile/Page対象を、新機能の16MiB上限にさかのぼって数えない。

### 成功条件

- 境界値を受理し、1文字・1バイト・1件超過を理由つきで拒否する。日本語を文字数とバイト数で区別する。
- 後半の不正partsでwrites前半も増えない。履歴上限に達しても元コメントの図と未配達イベントを失わない。
- 説明の保存上限に達して説明更新が拒否されても、Codeの新しいメタデータ取り込みは続けられる。

### 反例

- 長い本文や条件を省略して保存する。新上限を理由に復元した旧bodyを書き換える。
