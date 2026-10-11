# Explanation の読む画面と選択引用の実装計画

## Goal

Explanation を畳まない一続きの文書として読み、文字を選んだその場でコメントでき、選んだ文字が引用として result と会話まで届く画面を実装する。

実装 branch 名は `explanation-reading`。`explanation-view` の、この計画をコミットした後の head から切り、受け入れ後に `explanation-view` へ戻す。受け入れとマージの後、主担当がこの計画を削除する。

## Specification

承認の入力は commit `dbf4d75` の三文書。

- [EXPLANATION](../spec/explanation.md)：[R-EXPLANATION-READING 読む画面とコメントの入口](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口)、[R-EXPLANATION-COMMENT 対象と更新後の会話](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-VIEW 入口と移動](../spec/explanation.md#r-explanation-view-入口と移動)
- [既存契約の改訂](../spec/explanation-contracts.md)：[R-EXPLANATION-RESULT resultとwaitの版3](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-SESSION 保存と復元](../spec/explanation-contracts.md#r-explanation-session-保存と復元)
- [本文・返信・発言の共通パーツ](../spec/content-parts.md)：[R-CONTENT-DIAGRAM 図の型と描画](../spec/content-parts.md#r-content-diagram-図の型と描画)（「sequence/flowはインラインのパン…」の項）

見た目の正解は [design/ui-mock-explanation-reading.html](../design/ui-mock-explanation-reading.html)（仕様が参照するモック）。モックの上部の帯と「決めたいこと」の枠は画面に出さない。入力欄は「B 選んだ所に浮かぶ」、引用は「選んだ文字」が採用された形。

前提の既存実装は [docs/plans/explanation-view.md](explanation-view.md) の 1〜9 が実装済みの `explanation-view` branch。実装前に `AGENTS.md`、`PROJECT.md` と該当する rule skill（ba0918-tdd、ba0918-rust、ba0918-design、ba0918-placement、ba0918-readability、ba0918-secrets）を読む。

## Approach and why

### 今の実装と変える場所

- 選択引用の値は `ExplanationTarget`（`crates/kemi-core/src/domain/review.rs:217-223`）に一つの任意フィールドとして足す。この構造体は session の `CommentDto.explanation`（`crates/kemi-core/src/session/mod.rs:586-617`）と result・wait・画面向けの `json!(target)`（`crates/kemi-server/src/session.rs:165-169`）がそのまま serde で使っている。足すだけで保存・出力の全経路に載る。
- 作成 API は `CommentRequest::AddExplanation`（`crates/kemi-server/src/api/comments.rs:22-32`）と `add_explanation_comment`（同 `430-489`）。復元側の形の検証は `CommentDto::into_comment`（`session/mod.rs:1123-1143`）。作成と復元で同じ検証関数を使い、検証を一か所に置く。
- 画面は `web/assets/views/explanation.js` の `renderExplanationDocument`（節を `<details>` で作り、ブロックごとに常設のトリガーと隠した入力欄を持つ）、`web/assets/features/explanation.js`、`web/assets/features/explanation/selection.js`、`web/assets/explanation.css` を作り替える。図は `web/assets/views/diagrams/viewport.js`（インラインにもパンのボタンがある）と `grid.js`（操作の行が無い）。
- text パーツの Markdown はサーバで描画され、`api/parts` の応答の後に DOM ができる（`web/assets/views/parts.js:22-25,51-55`）。目次の小見出し、選択の印、札の位置はこの描画が終わった後に作る。

### 暫定判断と覆す条件

1. **selection の出力**。`ExplanationTarget.selection` は `Option` で、serde の既定値（`#[serde(default)]`）付き、`skip_serializing_if` は付けない。result・wait・画面向け・session のどれにも `"selection": null` を含めて常に出す。`selection` の無い既存の版 4 セッションは null として読める。版 4 は未公開なので形式の版は上げない。`version_four_restores_parts_full_explanation_quotes_and_generation_without_rewriting`（`session/store.rs:1548-1598`）の完全一致の期待は null を含む形へ更新する。覆す条件：人が session には selection を省略して書くことを求めたとき。
2. **selection の入力の形と拒否の理由**。作成要求の `selection` は、いったん JSON の値として受けてから自前で検証する。object でない、`text` か `occurrence` が欠けている・型が違う、未知のキーがある、`occurrence` が整数の表記でない（`1.0` も拒否）・負・9,007,199,254,740,991 を超える、のどれも 400 と理由の文で拒否する。axum の JSON 拒否（422）に任せると、仕様の「理由が返る」に合う文で出るとは限らないため。`selection` が無い・null は選択なしとして受ける。保存値の occurrence は u64。覆す条件：axum の JSON 拒否の文で理由が十分と人が判断したとき。
3. **text の上限の数え方**。仕様どおり UTF-8 バイト数で 1 以上 65,536 以下（`parts_validation::bound` に `len()` を渡す）。文字数を数える既存の `text()` は使わない。body の上限とは別に数える。追加保存の容量（`additional_saved_size`）はコメントの canonical JSON 全体を数えているので、selection も自動で入る。ブラウザでは、選択が空白だけのときはボタンを出さず（今の動きと同じ）、65,536 バイトを超えるときはボタンの代わりに長すぎて引用できない理由を出す。
4. **作成要求の未知フィールド**。`CommentRequest` には今 `deny_unknown_fields` が無く、サーバが selection を知る前にブラウザが送ると黙って捨てられる。この計画では Step 1（サーバ）を Step 3（ブラウザの送信）より先に終える順序で防ぎ、`CommentRequest` 全体の受理範囲は変えない。覆す条件：人が既存の op にも未知フィールドの拒否を求めたとき（別の変更として扱う）。
5. **「表示上の文字列」の範囲と数え方**。text パーツは描画した `.part-markdown` 要素、code パーツはコード本文の要素を根とし、見出し・Copy などの操作・参照ボタンは含めない。仕様の「描画したパーツの `textContent`」を、利用者が読む本文だけに絞った読み。根の中に画面が後から入れる印・札・入力欄の要素の文字は数えない（描画直後の DOM の `textContent` と同じ文字列にする）。札がリスト項目の中に入っても、次の選択と復元の一致探しがずれないようにするため。`text` は `Selection.toString()` ではなく、Range の両端の位置から表示上の文字列の部分文字列として取る（改行や非表示要素で食い違わないため）。覆す条件：人がパーツ全体の要素を基準にすることを求めたとき。
   選択の判定は次の順に行う。両端を含む説明のブロック（`[data-explanation-block]`）が違う、または片端がどのブロックにも無いときは、ボタンを出さずにまたげない理由を出す。同じブロックで、端が根の外（見出しや参照ボタン、トリプルクリックで次の要素の先頭に置かれた端など）にあるときは、その端を根の境界に詰める。詰めた後に空なら何も出さない。
6. **印を付けるコメント**。札と印は、今の `commentsForBlock`（`web/assets/explanation-model.js:35-38`、読んでいる版と同じ版か、内容が一致するコメントを出す）が読んでいる版のブロックのそばに出すコメントに付ける。現在の版を読んでいるときは、仕様の「outdated でないコメントを現在の版の同じブロックで」と一致する。旧版を読んでいるときは、その版のコメントにもその版のブロックで印を付ける（読んでいる版で札を出す今の動きに合わせる）。解決済みのコメントも、今の札（`Resolved · ` の前置き）と同じく印と札を出す。保存時の selection を、表示上の文字列の `occurrence` 番目の一致として DOM の範囲に戻す。範囲が要素をまたぐときも印を付ける（方法は実装者に任せる）。一致が無ければブロック全体の表示に落とす。覆す条件：人が旧版や解決済みのコメントには印を付けないことを求めたとき。
7. **札の位置**。選択の終わりを含む段落・リスト項目・引用・表・コードの要素のうち、パーツ内で最も内側のものの直下に置く。リスト項目では項目の中の末尾に入れる（`ul` の直下に `div` を置かない）。
8. **目次の小見出し**。text パーツ内の Markdown 見出しをすべての階層で同じ一段下げの字下げで並べる。押したときに移るための DOM の id は画面が付ける。これはコメントの対象 ID ではない。覆す条件：人が深い階層を省くか階層ごとに字下げを分けることを求めたとき。
9. **節のコメント数**。読んでいる版で、その節のブロックに `commentsForBlock` が出す札の数（暫定判断 6 と同じ集合。解決済みも数える）。0 なら出さない。
10. **タッチでのインラインのパン**。マウスのドラッグはすでにパンになっている（`web/assets/diagrams/pan-zoom.js:21-40`）。指 1 本のタッチは今どおりページの縦スクロールのままにし、タッチでのパンは別表示とピンチで行う。長い文書の途中の図が縦スクロールを奪わないため。覆す条件：人が指 1 本のドラッグでもインラインの図を動かすことを求めたとき。
11. **会話の返信の図**。`diagramViewport` は会話の返信の図でも使われる。インラインのパンのボタンを外す変更と、ホバーで操作を出す変更は返信の図にも同じく効かせる（共通パーツの規則は説明に限らないため）。コメントのボタンは説明のブロックの図にだけ出す。
12. **grid 図（`type:"diagram"`）**。今は操作の行が無い。説明のブロックでは見出しの行の右にコメントのボタンだけを置き、拡大などは足さない（共通パーツで grid は非対話でよいため）。ボタンは sequence・flow の操作と同じく、ポインタを図に乗せたときと図の中にフォーカスがあるとき（ボタン自身にフォーカスが来たときを含む）に出し、ホバーできない端末では常に出す。
13. **`c` キー**。`selection.js` の中で、コメントできる選択がある間だけ受ける。入力欄・textarea の中、Ctrl・Meta・Alt 付き（コピーの Ctrl+C / Cmd+C を含む）、IME の変換中（`isComposing`）は受けない。全体のキー処理 `handleKey`（`web/assets/app.js:104`。133 行で Code 以外の見方を return する）には置かない。
14. **入力欄の位置**。本文のスクロールする要素（`#explanation-document`）に `position: relative` を付けてそれを基準にし、その中に絶対位置で置く（本文と一緒にスクロールさせるため）。選択の最後の行の下端から少し下に出す。選んだ文字と重ならない位置を取れないときも下に出す。720px 未満では本文の幅いっぱいにする。狭い画面の手渡しバーとの重なりは今の `openExplanationEditor` の補正を引き継ぐ。下書きは今どおりブロック単位（`${revision}:${blockId}`）で持ち、selection は下書きに含めない。

## Scope of change

この branch で変えるのは、選択引用のサーバ側（domain、session、作成 API、出力）と、Explanation の画面（文書の組み立て、目次、選択からのコメント、図の操作）、それらのテストと派生文書だけ。各 step の May change が許可範囲。

仕様・モック・`docs/plans/explanation-view.md` は変えない。設定・CI・製品 version・依存は変えない。依存が必要になれば始める前に返す。利用者の `.mise.toml` と、main の作業ツリーにある未コミットのモック（`ui-mock-explanation*.html` など）は対象外。

ソースのコメントは日本語、UI と CLI の文言・README・配布 skill は英語、docs は日本語。web の層の規則は `PROJECT.md` に従う。新しいモジュールを足したら `web/index.html` の `modulepreload` も直す。

## Step order and prerequisites

順序は 1 → 2 → 3 → 4 → 5 → 6。

- 1 を最初に置く。ブラウザが selection を送る前に、サーバが受け取って保存・出力できる必要がある（暫定判断 4）。
- 2 で文書の組み立てと目次を作り替えてから 3 の選択コメントを載せる。3 の入力欄と札の位置は 2 の DOM の形に依存する。
- 4 の図の操作は 2・3 と独立しているが、図のコメントのボタンが 3 の入力欄を開くので 3 の後に置く。
- 5 で検査スクリプトをまとめて新しい画面に合わせ、実バイナリで通す。2〜4 の各 step では対象の node テストと型検査までを回す。
- 6 の派生文書は実装の形が決まった後に合わせる。

WSL の負荷のため、cargo のビルドは `CARGO_BUILD_JOBS=4`、ブラウザを使う検査は一つずつ動かす。web/ にファイルを足したら release ビルドの前に `touch crates/kemi-webview/src/lib.rs` する（埋め込みが古くなるため）。途中の確認は debug ビルドでよい。

## Verification map

| 仕様の節 | 示す step |
|---|---|
| R-EXPLANATION-COMMENT（selection の形・上限・図での拒否・保存後不変・occurrence の印） | 1（サーバ）、3（画面） |
| R-EXPLANATION-RESULT（explanation の `selection`） | 1 |
| R-EXPLANATION-SESSION（版 4 に selection を保存・復元） | 1 |
| R-EXPLANATION-READING（畳まない文書・目次） | 2、5 |
| R-EXPLANATION-READING（入口・入力欄・札） | 3、5 |
| R-EXPLANATION-READING（図の操作）と共通パーツの図の操作 | 4、5 |
| R-EXPLANATION-VIEW（開閉を戻す対象から外れた読み位置の復帰） | 2、5 |

## Left to the implementer

関数・モジュールの名前と分け方、印を DOM に付ける方法（要素で包む、CSS Custom Highlight API など）、CSS の細部の値（モックの見た目に合わせる範囲で）。選択から `{text, occurrence}` を出す計算と、`{text, occurrence}` から範囲を戻す計算は、DOM に依存しない純粋な関数として `web/assets/explanation-model.js` に置くことだけは守る（node のテストで示すため）。

## Stop conditions

- 仕様やモックのとおりに作ると、既存の承認済みの動作（参照から戻る、版の保持、会話、Seen など）が壊れると分かったとき。
- 依存の追加が必要になったとき。
- 図の矢印キーのパンや別表示の操作が、共通パーツの「キーボードでもすべて操作でき」を満たせないと分かったとき。

## Out of scope

- 利用者の理解度に合わせた説明の出し分け。
- `CommentRequest` の既存の op の受理範囲の変更（暫定判断 4）。
- Code・Page の見方の変更。

## Step 1 — selection をサーバで受けて保存・出力する

Purpose: 説明コメントの `selection` を作成 API で検証して保存し、session・result・wait・画面向けの JSON に常に出す。Specification: [R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)。
Prerequisites: なし。
May change: `crates/kemi-core/src/domain/{review,explanation,parts_validation}.rs`、`crates/kemi-core/src/session/{mod,store}.rs`、`crates/kemi-server/src/api/comments.rs`、`crates/kemi-server/src/session.rs`、`crates/kemi-server/tests/server.rs`、`tests/e2e.rs`、それぞれのテストの fixture。
Done when: text・code のブロックへ selection 付きで作成すると、`api/review`・wait の handed・submit の result の `explanation.selection` に同じ `{text,occurrence}` が出る。selection なしのコメントでは `null` が出る。図のブロックへの selection、空の text、65,537 バイトの text、負・小数・範囲外の occurrence は保存されず、400 と理由が返り、既存のコメントと保存状態が変わらない。selection 付きのコメントを保留・復元しても同じ値を読める。selection の無い既存の版 4 セッションも読める。コメントの編集で selection は変わらない。
Shown by: test — RED → GREEN → REFACTOR。`cargo test -p kemi-core` に selection の検証関数の境界（1 バイト・65,536 バイト受理、0 と 65,537 バイト拒否、日本語のバイト数）と、selection 付きの版 4 の往復・selection の無い版 4 の読み込みのテスト。`cargo test -p kemi-server` に作成 API の受理・拒否と無変更（図・空・上限超過・負・小数表記・9,007,199,254,740,991 超・形の不正・未知のキー）、編集で selection が変わらないこと、result と wait と `api/review` の `selection` のテスト。`tests/e2e.rs` の `cli_context_does_not_wait_for_stdin_and_parts_roundtrip_through_all_targets_and_wait` を selection 付きに広げ、`cargo test --test e2e cli_context_does_not_wait_for_stdin_and_parts_roundtrip_through_all_targets_and_wait` で wait と result まで届くことを示す。最後に `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`scripts/check-domain-purity.sh`。
Left to the implementer: 検証関数の置き場所（May change の三つのファイルのどれか）と型の名前。
Stop and hand back if: selection の追加で、既存の版 2・3 の読み込みや古い result の `--result` の出力が変わるとき。

## Step 2 — 説明を畳まない文書と目次に作り替える

Purpose: 節の開閉と枠をなくし、モックの見た目の一続きの文書と、小見出しとコメント数を持つ目次にする。Specification: [R-EXPLANATION-READING](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口)、[R-EXPLANATION-VIEW](../spec/explanation.md#r-explanation-view-入口と移動)。
Prerequisites: Step 1。
May change: `web/assets/views/explanation.js`、`web/assets/features/explanation.js`、`web/assets/features/explanation/selection.js`（`<details>` の監視を外す範囲）、`web/assets/views/parts.js`・`web/assets/features/parts.js`（Markdown の描画の完了を受け取れるようにする範囲）、`web/assets/explanation-model.js` とそのテスト、`web/assets/explanation.css`、`web/assets/{state,dom,actions,app}.js`（必要な結線だけ）、`web/index.html`。
Done when: 節に `<details>`・開閉の矢印・枠が無く、番号付きの見出し・下線・余白で区切られる。ブロックごとの常設のトリガーと隠した入力欄はこの step では残す（消すのは Step 3。途中でコメントの手段が無くならないように）。目次に節と text パーツ内の Markdown 見出しが一段下げて並び、読み位置の項目が示され、押すとその位置へ移る。節ごとのコメント数が出る。目次の幅の変更（ドラッグとキーボード）と 720px 未満の引き出しは今どおり動く。参照から Code へ移って戻ると、元の版と読み位置に戻る。
Shown by: test — RED → GREEN → REFACTOR。`node --test web` に、説明の版とコメントから節ごとの表示コメント数を数える純粋関数のテスト（outdated を数えない、0 の節）を `web/assets/explanation-model.test.js` に足す。DOM の形と見た目は Step 5 のブラウザ検査で示す。`npx tsc -p web --noEmit`。
Left to the implementer: 目次の項目の DOM の作り方、読み位置の判定の方法、Markdown の描画の完了を受け取る方法（`partView` が今は `renderPartMarkdown` の Promise を捨てている。`web/assets/views/parts.js:25`）。
Stop and hand back if: 節の開閉をなくすと、既存の参照からの戻り（版の保持と読み位置）が保てないとき。

## Step 3 — 選んだ文字からその場でコメントする

Purpose: 文字選択で出るボタンと `c` キーで入力欄を選択の下に浮かべ、selection を送り、保存後は選んだ文字に印と札を付ける。Specification: [R-EXPLANATION-READING](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口)、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)。
Prerequisites: Step 1、Step 2。
May change: `web/assets/features/explanation/selection.js`、`web/assets/features/explanation.js`、`web/assets/views/explanation.js`、`web/assets/views/conversation.js`（説明コメントの引用の表示）、`web/assets/views/parts.js`・`web/assets/features/parts.js`（描画の完了を受け取る範囲）、`web/assets/explanation-model.js` とそのテスト、`web/assets/model.js` とそのテスト（コメントのラベル）、`web/assets/explanation.css`、`web/assets/{state,dom,actions,api,app}.js`（必要な結線だけ）、`web/index.html`。
Done when: ブロックごとの常設のトリガーと隠した入力欄が無い。text・code のパーツ内で文字を選ぶとボタンが選択の近くに出る。二つのブロックにまたがる選択ではボタンが出ず、同じ場所に理由が出る。ボタンか `c` で、選択の最後の行の下に入力欄が浮かび、節見出しと選んだ文字が引用として出る。Esc で閉じる。保存の要求に `selection` が入り、保存後は選んだ文字に印、選択の終わりを含む要素の直下に一行の札が出る。再読み込み後も同じ箇所に印と札が出る。一致が無いコメントはブロックの左の線とブロックの直下の札で出る。会話パネルの説明コメントは、selection があれば選んだ文字を引用として出し、元のブロック全体も読める。
Shown by: test — RED → GREEN → REFACTOR。`node --test web` に、表示上の文字列と選択の始点・終点の位置から `{text, occurrence}` を出す関数（同じ語が二回ある文の二回目で occurrence が 1、重なる一致を数える）と、`{text, occurrence}` から位置を戻す関数（一致が無ければ無いと返す）のテスト。表示上の文字列を数える関数には、札・印・入力欄の要素を含む DOM に相当する入力でも、それらの文字を数えないことのテストを足す（札のあるリスト項目に二つ目のコメントを付けても occurrence がずれない）。`npx tsc -p web --noEmit`。画面の動きは Step 5 のブラウザ検査で示す。
Left to the implementer: ボタンと入力欄の DOM の作り方、浮かべる位置の計算の細部。
Stop and hand back if: 描画した Markdown の `textContent` が描画のたびに変わり、同じ説明版で occurrence が安定しないと分かったとき。

## Step 4 — 図の操作を絞り、図にコメントのボタンを置く

Purpose: インラインの図の操作を縮小・拡大・全体表示・別表示・コメントに絞ってホバーとフォーカスで出し、パンはドラッグと矢印キーで行う。Specification: [R-EXPLANATION-READING](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口)、[R-CONTENT-DIAGRAM 図の型と描画](../spec/content-parts.md#r-content-diagram-図の型と描画)。
Prerequisites: Step 3。
May change: `web/assets/views/diagrams/{viewport,grid}.js`、`web/assets/views/parts.js`、`web/assets/views/explanation.js`、`web/assets/features/explanation.js`、`web/assets/features/explanation/selection.js`、`web/assets/parts.css`、`web/assets/explanation.css`、`web/assets/{actions,state,dom}.js`（結線だけ）、関係する web のテスト。
Done when: インラインの sequence・flow の図にパンのボタンとリセット（↺）が無く、縮小・拡大・全体表示・別表示が見出しの行の右に小さく出る。操作はポインタを乗せたときと図の中にフォーカスがあるときに出て、ホバーできない端末では常に出る。図にフォーカスがある間は矢印キーでパンでき、マウスのドラッグでもパンできる。別表示ではパンのボタンとリセットも出て、閉じると元の図へ戻る。説明のブロックの図（grid を含む）にだけコメントのボタンがあり、キーボードとタッチで押せて、図の見出しを引用にした入力欄がボタンの下に開く。会話の返信の図にはコメントのボタンが無い。
Shown by: test — RED → GREEN → REFACTOR は Step 5 のブラウザ検査（`scripts/test-explanation-view.mjs` と `scripts/test-explanation-selection.mjs`）で示す。この step では `node --test web` と `npx tsc -p web --noEmit` を通す。
Left to the implementer: 操作を出し入れする CSS の方法。
Stop and hand back if: パンのボタンを外すと、キーボードだけでインラインの図を動かせない場面が残るとき。

## Step 5 — ブラウザ検査を新しい画面に合わせて実バイナリで通す

Purpose: 説明の画面の検査スクリプトを新しい入口と見た目に合わせ、1280px と 390px、light と dark で実バイナリの動きを示す。Specification: [R-EXPLANATION-READING](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口) の成功条件、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話) の成功条件、[R-EXPLANATION-VIEW](../spec/explanation.md#r-explanation-view-入口と移動) の成功条件。
Prerequisites: Step 1〜4。
May change: `scripts/test-explanation-view.mjs`、`scripts/test-explanation-selection.mjs`、検査の fixture。390px の説明の検査は、今もある二つのスクリプトの 390px の部分（`test-explanation-view.mjs` と `test-explanation-selection.mjs`）に足す。見つかった実装の原因は 2〜4 の May change の中で直し、その step の検証をやり直す。
Done when: 次をすべて観測する。
- 1280px・390px と light・dark の 4 通りの組で、節に `<details>`・開閉の操作が無く、節の要素に枠線が無く（計算後のスタイルで確かめる）、目次の項目で移れる。テーマは画面のテーマの切り替えのボタンで切り替える。4 通りのスクリーンショットを `test-explanation-view.mjs` が `.agents/artifacts/explanation-reading-<幅>-<テーマ>.png` に撮る。
- 目次に Markdown の小見出しが一段下げて出て、押すとその位置へ移る。コメントのある節に数が出る。
- 選択中に `c` で入力欄が開き、Ctrl+C では開かない。Esc で閉じる。入力欄の引用に節見出しと選んだ文字が出る。入力欄は本文と一緒にスクロールする。
- 札を押すと会話パネルでそのスレッドが開き、会話パネルの説明コメントは選んだ文字を引用として出す。
- 一致が無いコメント（API で本文に無い text の selection を送って作る）は、ブロックの左の線とブロックの直下の札で出る。図へのコメントも左の線と札で出る。
- 図の操作がポインタを乗せたときとフォーカスで出入りし、`hover: none` では常に出る（CDP の `Emulation.setEmulatedMedia` で `hover: none` を与える。agent-browser の CDP の URL を使う方法は `test-explanation-selection.mjs` の実タッチと同じ）。会話の返信の図にはコメントのボタンが無い。
- 次の項目も観測する。段落の途中の数文字を選んで保存すると、入力欄が選んだ行のすぐ下に出て、保存後はその文字に印、段落の直下に札が付く。同じ語が二回ある段落で二回目を選ぶと、result の `occurrence` が 1 で、再読み込み後も二回目に印が付く。code パーツでもボタンが出る。ブロックをまたぐ選択ではボタンが出ず理由が出る。図のコメントのボタンをキーボードとタッチで押せ、引用が図の見出しになる。インラインの図を矢印キーでパンでき、別表示を閉じると元の図へ戻る。ブロックごとの常設のトリガーがどこにも無い。参照から Code へ移って戻ると同じ版と読み位置に戻る。390px でも同じ操作ができ、入力欄が表示領域と手渡しバーの外にはみ出さない。
Shown by: test — 旧スクリプトの「ブロックごとのトリガーを押す」「code ではボタンが出ない」などの期待を新しい仕様の期待へ書き換える。書き換えたスクリプトを、release ビルドし直す前の `target/release/kemi`（`explanation-view` の `b3380d9` で作ったもの。無ければ `explanation-view` の head で一度だけ debug ビルドする）に当てて失敗することを示し、その後に新しいバイナリで通す。今のバイナリでもすでに通る項目（インラインの図の矢印キーのパン、別表示を閉じると元に戻るなど）は RED を示せないので、その旨を記録して除く。順に `node --test web`、`npx tsc -p web --noEmit`、`touch crates/kemi-webview/src/lib.rs`、`cargo build --release`、`node scripts/test-explanation-view.mjs target/release/kemi`、`node scripts/test-explanation-selection.mjs target/release/kemi`、既存の狭い画面の回帰として `node scripts/test-narrow-screen.mjs target/release/kemi`。4 通りのスクリーンショットは人がモックと並べて見る。
Left to the implementer: スクリプトの中の待ち方と DOM の見つけ方（aria-label や role を使い、内部のクラス名に依存しすぎない）。
Stop and hand back if: 既存のブラウザ検査（`test-agent-channel.mjs`、`test-rendered-view.mjs`）が、この branch の変更で失敗し、原因が 2〜4 の範囲の外にあるとき。

## Step 6 — 派生文書を新しい形に合わせる

Purpose: README・配布 skill・用語集の説明コメントと Explanation の画面の記述を、selection と新しい読む画面に合わせる。Specification: [R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-READING](../spec/explanation.md#r-explanation-reading-読む画面とコメントの入口)。
Prerequisites: Step 1〜5。
May change: `README.md`、`skills/kemi/SKILL.md`、`skills/kemi/references/{results,inputs,conversation}.md`、`CONTEXT.md`。
Done when: README の result の `explanation` の説明と例が `{revision, block_id, quote, selection}` になり、selection の読み方（ブロック全体の引用が正で、selection は手がかり）が書かれている。Explanation の画面の説明が「ブロックごとにコメント」から「文字を選んでコメント、図はボタン」に変わっている。`skills/kemi/references/results.md`・`conversation.md`（説明のスレッドの記述）と `SKILL.md` も同じ読みになっている。`CONTEXT.md` の quote の項に selection の読みがある。
Shown by: check — `scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`、`git diff --check`。README・skill と実装の対応は最終レビューで人が照合する。
Left to the implementer: 文の書き方。
Stop and hand back if: none。
