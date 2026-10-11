# EXPLANATION の実装計画

## Goal

コードと live Page を横断する説明を読み、説明ブロックへの質問と図付きの返信を、更新・保留・復元をまたいで続けられる一つの成果物を実装する。

実装 branch 名は `explanation-view`。この計画は実装前の引き継ぎであり、実装完了や計画承認を表さない。受け入れとマージの後、主担当が計画を削除する。履歴には残す。

## Specification

承認の入力は commit `55ba772` の次の三文書。同じバイト列を kemi でレビューして承認したという依頼者の確定事項に基づく。文書の Status に残る「草稿」「承認前」は、この計画の入力を未承認に戻すものではない。実装済みとも解釈しない。着手時に当該 commit との差を確認し、差があれば承認対象を取り違えずに返す。

- [EXPLANATION](../spec/explanation.md)。各 step で見出しを指定する。
- [本文・返信・発言の共通パーツ](../spec/content-parts.md)。各 step で見出しを指定する。
- [既存契約の改訂候補](../spec/explanation-contracts.md)。改訂する範囲は [現行との対応](../spec/explanation-contracts.md#現行との対応) に従う。

一リンク深で参照する既存正典は以下。改訂部分には上の承認セットを適用し、それ以外は既存契約を維持する。本文をこの計画へ転記しない。

- `kemi.md`: [R-INPUT-6](../spec/kemi.md#r-input-6-その他の入出力)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[R-VIEW](../spec/kemi.md#r-view-差分の表示)、[R-RENDER](../spec/kemi.md#r-render-描画表示)、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)、[R-UNIT](../spec/kemi.md#r-unit-グループ単位の切り替え)、[R-COMMENT](../spec/kemi.md#r-comment-コメントと-suggestion)、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)、[R-RESULT](../spec/kemi.md#r-result-結果ファイル)、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)、[R-DEPS](../spec/kemi.md#r-deps-依存)、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
- `agent-channel.md`: [R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)、[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)、[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)、[R-AGENT-WRITE](../spec/agent-channel.md#r-agent-write-reply-の入力と上限)、[R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)。
- `live.md`: [R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)。Page 固有の操作はここを入口に既存実装を使い、新しい比較機能を設計しない。
- [CONTEXT.md](../../CONTEXT.md)。説明で追加する語と、コメント・quote・outdated・見方・版の読みを step 1 で反映する。

実装前には `AGENTS.md`、`PROJECT.md` と該当する rule skill を読む。構成と標準コマンドの根拠は `PROJECT.md`。実装は TDD、Rust 変更は Rust 規約に従う。この計画作成時には製品コード v0.2.1、main `55ba772` を確認した。`38e6204` の Page 操作改善を含む現行実装を基準にする。

## Approach and why

### 確認した境界と再利用の選択

| 責務 | 採用または自作の方針と理由 |
|---|---|
| 公開出典と許諾 | shoryo commit `46c6f764c1d373365a8902ffcc682245079363e6` を固定対象にする。当該 commit の `LICENSE` は MIT、著作権表示は `Copyright (c) 2026 ba0918`。取り込み先に著作権表示と許諾文を保持する。 |
| Part と構造検証 | 固定公開版の `explanation.rs` / `explanation_validation.rs` を限定取り込みする。任意項目の省略と上限検証を再利用できる。重複キー拒否とcontent互換境界は既存serdeで追加し、検証を一つの境界へ集める。 |
| 文章の互換性 | kemi の返信・発言を拡張する。`Plain` と `Parts` を区別する小さいドメイン型を自作し、旧 body を Markdown 化する経路をなくす。 |
| 説明の版・ID・参照・内容一致 | kemi の純粋 domain に追加する。File の hash と改行正規化は用途が異なるため再利用しない。保存・HTTP 型は外側で変換する。 |
| Markdown | `render/markdown.rs`、`inline.rs`、`url.rs` と既存 ox-content を採用する。parts 専用の画像無効出力を追加し、ファイルの差分整列・画像解決は持ち込まない。 |
| 図計算・SVG・実寸検証 | 固定公開版の `explanation-{sequence,flow,svg,bounds,layout-validation,collision}.js` を採る。DOMとブラウザの文字計測だけに依存し、新runtime依存は不要。 |
| 図の操作 | 固定公開版の `pan-zoom.js` とviewport/viewerの操作規則を採る。Component/translatorへの接続はkemiのviews/featuresへ適合させ、framework・i18n・round・保存モデルは取り込まない。 |
| grid図・コード | 固定公開版の `diagram.js` / `measure.js` をgrid候補とし、省略と全文表示を再利用する。codeはkemiのDOMで読み取り専用表示を作る。 |
| 保存と復元 | kemi の session DTO、原子的ファイル差し替え、ロック、payload 分離を採用する。世代発行と説明更新の受理に必要な候補保存だけ追加する。 |
| CLI・HTTP・イベント | kemi の agent 接続保護、writes 全件検証、wait の配達確認、SSE を採用する。explain の会話外通知と context を追加する。 |
| 画面・会話 | kemi の ESM/JSDoc、共有 state、features、views、actions を採用する。全 View で同じ Part 描画を呼ぶ。新しい framework やビルド工程は不要。 |

現行 `Reply/Message` は `body: String`、`CommentTarget` は File/Page だけ。`session/mod.rs` が DTO 変換を持ち、`encoding.rs` は形式 3 を書き 2/3 を読む。`server/session.rs` が会話とイベントを JSON 化し、`api/submit.rs` と `src/agent.rs` が新 result/wait の契約版を決める。このため型変更と保存・出力対応を別の未完成段階にしない。

現行 `api.rs::review` は startup を差し替えてからもう一方を更新する。source の `PlanStore` 自体も内容参照を差し替える。新しい保存条件には、サーバのメタデータだけでなく source の参照計画も含む候補作成・公開の境界が必要。ファイルの読み取りバイトを固定する変更ではない。

`FileEntry` に片側の有無と総行数の明示値はない。参照地図には既存の統計・入力時に既に分かる情報を使い、不明を不明として扱う。追加で全ファイルを読んで埋めない。`features/agent.js::resyncAgent` は会話回復のため `api/review` を読むが、Code 一覧を取り込んではいない。この区別を新 token の処理にも保つ。

初期説明の反対単位参照は、未準備という理由で拒否しない。完成済み一覧の照合と、未準備単位の選択的な source 照合を同じ参照検証へ接続する。後者は初期メタデータを作った範囲の端の sha・merge-base を使う。`unit=commit` は要求された commit が範囲内の非merge group であることを確認し、その group の差分から要求された path と片側の有無を解く。`unit=file` は group `all` と同じ最終形の差分定義で解く。複数参照は要求された group/path ごとにまとめる。別 commit にだけある path、変更一覧にないが tree にはある path、片側不在、表示 path が曖昧な場合を正当な参照としない。

既存 `crates/kemi-core/src/source/git.rs:229-260` と同ファイルの `284-315` に group と旧新参照の組み立てがある。`441-487` の focus 向け集合は group/path の対応と片側を失うため、その集合だけで説明を検証しない。`857-865` の name-status と `932-973` の commit 差分処理を分離・再利用し、要求された group の名前・状態・改名候補を照合する source 境界を追加する。改名検出や lossy path の曖昧性を保つために必要な同 group の候補は照合するが、無関係な全 commit の統計・本文・ReviewMeta・file_id・PlanStore を起動前に構築しない。path を git の式や pathspec として解釈させず、返ったエントリとの厳密な比較に使う。これで現在の表示一覧を先行完成させずに初期参照を検証する。総行数は増減数から推測せず、本当に不明なら承認済みの行参照拒否と全ファイル参照許可を適用する。context の一覧未準備時の拒否とは別の経路である。

初回凍結には、起動 D1 のメタデータ、世代、未完成の反対単位を作る入力記述、内容参照計画をメモリ内の一つの作成対象として保持する。現行 `server/session.rs:304-333` は反対単位を待った後の現在 review を読むため、D2 を取り込むと初回写しへ混ぜうる。D1 の反対単位の計算は初回応答後に始め、D1 の作成対象へ完成結果を渡す。D2 用の公開処理は別に世代を検査する。現行 `units.rs:153-189` の「古い完成を捨てて最新で作り直す」処理だけに D1 凍結を従属させない。D1/D2 の参照先の分離はメタデータと既存 SideRef の分離であり、worktree のバイトを新たに固定しない。起動前に本文を読む変更も加えない。凍結用の保持は完成・失敗・終了時に解放し、全世代の索引へ拡張しない。

### 暫定判断と覆す条件

承認済みの入力形式・数値上限・保存先・エラー契約は選択事項にしない。次の判断は仕様が内部方式または未準備時の扱いまで固定していない部分である。

1. **図の取り込み方法**。新しい runtime 依存を増やさず、公開 shoryo の必要モジュールを固定 commit から限定取り込みする案を採る。著作権表示と MIT 許諾文を同梱し、採用元と差を履歴に記録する。仕様互換の配布パッケージがあり ESM・musl・license 条件を満たす場合、または人が依存としての採用を指定した場合に覆す。固定対象を確認できなければ実装しない。
2. **JSON の境界**。入力用、保存用、出力用の変換を区別し、保存と出力の body/parts 両キーを入力禁止規則に通さない。既存 serde/serde_json を使い、キー存在と重複キーを値へ潰す前に検査する。型付き内容の任意項目は省略の有無を保持する。既存の共通 decoder がこの条件を満たすと実証できた場合はそれを採る。
3. **差分取り込みと凍結の分離**。準備済みの両単位を候補として扱い、途中失敗や保存失敗で公開済みの一覧・token・source 参照計画を変えない。もう一方が未準備の間は世代を混ぜず、context を理由つきで拒否する。非同期完成は要求時の世代へ対応づけ、D1 初回凍結と D2 現在版の完成先を分ける。D1 の写し保存時に最新の現在版・最大発行世代を D1 へ戻さない。別方式でも同じ観測結果と起動時の遅延条件を満たすなら覆せる。
4. **初期参照検証の内部方式**。未準備の反対単位は上記の選択的 source 照合で検証する案を採る。group/path/片側の対応が既存集合からは得られず、全単位の先行構築を避けるためである。同じ拒否範囲、改名・曖昧性の読み、初回応答後の全単位作成、起動性能を維持する別のメタデータ照合方式が示された場合に覆せる。未準備だけを理由に正当な参照を拒否する案は採らない。
5. **開いている旧説明の保持**。画面ごとの保持要求をサーバのメモリで管理し、表示中と参照戻り先を明示取得・明示解放する。通信の一時切断だけで保持を消さず、放置期限を導入しない。放置された保持が残れば更新が容量拒否になりうるが、読んでいる版を失うより仕様に沿う。人が切断画面の保持寿命を指定し、開いた版の保証をどう守るか確定した場合に覆す。保留で一時保持を解く規則は仕様どおり。
6. **世代の整数上限**。u128 の次世代を表せない取り込みは理由つきで拒否し、旧状態とレビューを残す。巻き戻しや token 再利用をしない。人が上限到達時の終了を明示した場合に覆す。説明版の上限到達時の扱いは既に仕様で定まっている。

## Scope of change

一つの branch 内で domain、source のメタデータ境界、session、HTTP/agent、CLI、web、検証、派生文書を実装する。各 step の May change が許可範囲。新ファイル名はその範囲内で責務に合わせてよい。

利用者の `.mise.toml`、未コミットの `docs/design/ui-mock-explanation.html`、`ui-mock-explanation-branch.html`、`ui-mock-explanation-integration.css`、`ui-mock-explanation-integration.js` は対象外。未コミットモックは承認済み仕様やテストの正解として扱わない。`docs/README.md` の既存追記を残す。設定・CI・製品 version・配布の仕組みは変更対象に含めない。依存が必要になれば May change 外の Cargo 変更を始める前に判断を返す。

ソースの説明・コメントは日本語、UI/CLI は英語、README/配布 skill は英語、docs は日本語。`PROJECT.md` の web 層規則を適用し、gui-structure は適用しない。

## Step order and prerequisites

順序は 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9。すべて同じ成果物に含める。

- 2/3 は純粋な値と規則を追加する段階。既存 Reply/Message/Comment の runtime 型はまだ切り替えない。
- 4 で既存型を切り替えるなら、その全構築箇所、全 match、保存 DTO、旧形式 reader、browser の既存 plain 消費、全新 result/wait serializer を同じ step 内で完了する。CLI に parts 入力を公開するのは 6。
- 5 の描画は 4 で復元できる形式 4 の入力からも検証できる。まだ存在しない explain CLI を前提にテストを組まない。
- 6 は描画・保存・出力がそろってから新入力を公開する。7 はその API を使い、pin・取得・戻り・通知を一体で結線する。
- CLI/JSON を変える step 4/6 では README と配布 skill の該当説明も同時に更新する。9 は最後の照合であり、契約更新をそこまで先送りする理由にはしない。

各実装 step は対象動作の失敗を実際に走らせて確認してから本番コードを変える。GREEN 後に整理し、同じ対象を再実行する。既に成立する動作のためだけに新テストを増やさない。各 step のコマンドは必要な範囲を示す。同じコード状態で既に成功した Browser ケースを後の step で繰り返さず、変更・失敗・未解決の懸念が届く範囲だけ再実行する。8 の最終結線確認では未実行または変更の影響があるケースを補完する。

## Verification map

| 仕様の見出し | 決定的な観測と step |
|---|---|
| 用語・現行との対応・再利用境界 | 1 の文書照合と固定 upstream/license の記録。9 で配布文書を照合する。 |
| R-CONTENT-INPUT / LIMIT | 2 の入力検証、4 の旧形式と容量、6 の writes 全件拒否、5/7 の plain/Markdown 表示。 |
| R-EXPLANATION-DOCUMENT / COMMENT | 3 の純粋規則、4 の完全引用保存、6 の反対単位の正当な全ファイル参照受理と不正な組の拒否・第三対象 API、7 の旧・現在ブロック表示。 |
| R-EXPLANATION-BASE | 3 の token 規則、4 の永続発行と D1 初回凍結/D2 取り込みの交錯、6 の context/SSE、7 の一致時の情報入口と不一致通知の区別、7/8 の二画面独立表示。 |
| R-EXPLANATION-SESSION | 4 の形式 2/3 固定値・形式 4・障害と凍結前刷新の制御 fixture、7 の保持解放、8 の実プロセス復元・保存費用。 |
| R-EXPLANATION-CLI | 6 の引数・stdin・反対単位の選択的初期参照検証・競合・接続保護・stdout/終了コード。 |
| R-EXPLANATION-RESULT / AGENT-HAND | 4/6 の新 result/wait と過去 result、原子的 hand・受取確認、8 の往復。 |
| R-CONTENT-MARKDOWN | 5 の生成 HTML・実ブラウザの画像要求と、到達可能な通信失敗による Part 単位の完全 body/plain fallback・隣接内容/コメント保持。8 の Rendered 回帰。 |
| R-CONTENT-DIAGRAM | 2 の構造検証、5 の実フォント・領域・衝突・操作・元 JSON。意味の正しさを判定する機能は作らない。 |
| R-EXPLANATION-VIEW / VIEW / NARROW / PAGE-VIEW | 7 の入口・参照戻り・会話維持・390px/1280px、8 の初期モード組合せと既存 Page 回帰。 |
| R-SERVE / UNIT / SESSION / DEPS / VERIFY | 1 の license、4 の I/O/遅延境界、8 の標準規模計測・全体検証。 |

テストの正解は仕様が述べる製品動作から作る。入力は CLI/HTTP、旧保存値、利用者の更新・保留・復元、ブラウザのフォントや zoom など実際の生成元を持つものにする。API 内部 URL、helper 名、未契約 DOM 属性、任意の文言の字面は固定しない。意味を保証しない図に「意味 valid」の自動合格を作らない。

## Left to the implementer

May change 内のモジュール名、内部 API URL、純粋関数の分割、既存 fixture の拡張方法、仕様で固定していない理由文、図の構文ハイライトの有無を任せる。どれを選んでも、公開入力・出力、拒否範囲、保存、容量、版・引用・参照、操作性が同じであること。web feature の追加位置は既存の一方向依存を満たす位置を選び、必要なら `PROJECT.md` の列挙も同時に直す。views から API を呼ばず、上位呼出は actions を通す。

## Stop conditions

承認仕様に両立しない解釈が生じたとき、許可範囲外や不可逆・外部公開操作が必要なとき、測定で既存性能条件を維持できないと分かったときは、根拠の節・反例・最小の選択肢を添えて返す。通常の局所的な失敗は直して再検証し、手順ごとの人間承認を追加しない。

初期参照検証では、選択的なメタデータ照合でも承認された対象の一意性・片側を判定できず、全反対単位の先行構築以外に両立手段がないことを具体的な入力・source 経路・性能条件で示せた場合だけ、仕様の判断へ返す。後工程の一覧が未準備という事情だけでは返さず、入力も拒否しない。今回のコード調査でこの両立不能は確認されていない。

## Out of scope

画像取り込み、読者適応、説明の意味検証、LLM が読んだ内容との byte 一致保証、api/file の byte 固定、起動時の全 file hash、全世代参照索引、新しい索引ファイル・世代ログ・保存ディレクトリ、Page の比較機能追加、モックの採用、重いモデル評価、mutation、CI 更新、version bump、release、tag、push、インストール・サーバの恒常運用。

## Step 1: 正典・用語と再利用境界を確定する

- Purpose: 実装者が承認入力と現状を取り違えず、取り込むコードの出典と境界を追えるようにする。
- Specification: [用語の追加候補](../spec/explanation.md#用語の追加候補)、[再利用の境界と調査根拠](../spec/explanation.md#再利用の境界と調査根拠)、[現行との対応](../spec/explanation-contracts.md#現行との対応)、[文書照合の根拠と承認後の反映先](../spec/explanation-contracts.md#文書照合の根拠と承認後の反映先)、[R-DEPS](../spec/kemi.md#r-deps-依存)。
- Prerequisites: `55ba772` の三仕様を直接読めること。同目的の別計画が増えていれば統合先を確認する。公開 upstream は `https://github.com/ba0918/shoryo`。
- May change: `CONTEXT.md`、`docs/spec/kemi.md`、`docs/spec/agent-channel.md`、`docs/spec/live.md`、`docs/spec/explanation.md`、`docs/spec/content-parts.md`、`docs/spec/explanation-contracts.md`、`docs/README.md`、この計画。三仕様の変更は承認状態と正典参照の整理だけで、契約本文を変更しない。
- Done when: 対応表の正典節・用語が承認内容を参照し、現状を実装済みとして記していない。公開 shoryo の完全 commit ID、当該 commit の LICENSE、必要な型・検証・図計算・操作の採否と理由をこの計画の再利用表へ記録してある。既存モック追記が保たれている。
- Shown by: artifact。更新した上記文書とこの計画。`git diff --check` と節リンク確認で形式を確認し、固定公開 commit の LICENSE と採用候補の diff を照合する。依存を採る判断なら `cargo deny check licenses`、`cargo tree -e features` の根拠も記録する。
- Left to implementer: 既存正典へ要件を一箇所に集めるか新仕様への節リンクで反映するか。重複した契約本文は増やさない。出典の永続表示は取り込み先の著作権表示に置く。
- Stop and hand back if: 公開 commit や許諾文を確認できない、または候補の適合に framework/i18n/round モデルや許可外依存が必要。探索はこの step に必要な境界で打ち切り、未確認を採用済みと記さない。

## Step 2: Part と互換 content の純粋型・境界検証を追加する

- Purpose: 旧 plain と新 parts を区別し、すべての入力・保存境界が同じ構造規則を使えるようにする。
- Specification: [R-CONTENT-INPUT](../spec/content-parts.md#r-content-input-入力と互換性)、[R-CONTENT-DIAGRAM](../spec/content-parts.md#r-content-diagram-図の型と描画)、[R-CONTENT-LIMIT](../spec/content-parts.md#r-content-limit-上限)。
- Prerequisites: 1。固定 upstream の採否が決まっていること。
- May change: `crates/kemi-core/src/domain/mod.rs`、新規 `crates/kemi-core/src/domain/parts*.rs`、新規 `crates/kemi-core/src/domain/conversation_content*.rs`、新規 `crates/kemi-core/src/parts/` の入出力変換、`crates/kemi-core/src/lib.rs` の宣言。取り込み先に必要な license 表示。
- Done when: 正しい全 Part 型を型付き値へ検証でき、未知項目・null・重複キー・body/parts の混在を拒否する。日本語の scalar 数と UTF-8 容量を区別し、任意項目の省略が失われない。grid 文法と sequence/flow 構造の拒否が browser 計測なしで決まる。旧 body の空文字を保持する。既存 runtime 型をまだ変えていない。
- Shown by: test。`cargo test -p kemi-core` で、入力混在と重複キー、Unicode/容量の境界、decision 条件・未定義参照・grid 不正、任意項目の往復を RED/GREEN/REFACTOR で示す。純粋型にファイル・HTTP・描画依存がないことは `scripts/check-domain-purity.sh` で併せて確認する。
- Left to implementer: serde DTO とドメイン型の内部名、正準サイズ算定の helper 分割。既存 serde/serde_json で足りる限り依存追加はしない。
- Stop and hand back if: 固定候補が承認済みの省略/明示値の違いを失い、適合できない、または仕様の構造検証を実寸・意味検証に置き換える必要が生じる。

## Step 3: 説明・第三対象・差分表示版の純粋規則を追加する

- Purpose: 説明更新、元引用、outdated と参照解決の正解を I/O から独立させる。
- Specification: [R-EXPLANATION-DOCUMENT](../spec/explanation.md#r-explanation-document-説明の構造)、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-CONTENT-LIMIT](../spec/content-parts.md#r-content-limit-上限)。
- Prerequisites: 2。
- May change: `crates/kemi-core/src/domain/mod.rs`、新規 `crates/kemi-core/src/domain/explanation*.rs`、新規 `crates/kemi-core/src/domain/display_version*.rs`、2 の変換境界。既存 `crates/kemi-core/src/domain/review.rs` の variant 切替は 4 まで行わない。
- Done when: ID・参照・expected_revision・review 所属と発行済み最大世代を検証し、現在基点と既知旧基点の受理を分けられる。ブロックの移動・節名変更と内容変更を区別し、同 ID/同内容の再登場で outdated が戻っても引用は変わらない。現在版・コメント・未配達イベント・画面保持を入力として無参照旧版だけを削除候補にできる。容量超過時の候補を適用しない。
- Shown by: test。`cargo test -p kemi-core` で、型付き値のキー順/escape と文字列差、座標/role/参照変更、削除・再登場、既知旧 token・別 review・未来世代、現在/旧基点の参照、保持版を除外する容量計算を RED/GREEN/REFACTOR で示す。実行はネットワーク不要。
- Left to implementer: 履歴集合の内部表現と比較 helper。hash を補助にしても最終一致判定は型付き値で行う。
- Stop and hand back if: File の hash/正規化を説明へ流用しないと成立しない、または全世代索引・内容固定を規則の前提にする必要が生じる。

## Step 4: runtime 型・形式 4・永続世代発行・版 3 出力を一体で切り替える

- Purpose: 新しい値を失わず保存・復元・出力し、永続保存できていない世代を外部へ発行しない。
- Specification: [R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-CONTENT-LIMIT](../spec/content-parts.md#r-content-limit-上限)、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、[R-RESULT](../spec/kemi.md#r-result-結果ファイル)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[R-UNIT](../spec/kemi.md#r-unit-グループ単位の切り替え)、[R-LIVE](../spec/kemi.md#r-live-ライブリロード)、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)、[R-AGENT-EVENTS](../spec/agent-channel.md#r-agent-events-wait-が返す-json)。
- Prerequisites: 3。既存 session 固定値と source/server テストの fixture を利用できること。
- May change: `crates/kemi-core/src/domain/{review,comment,agent}.rs` と 2/3 の新規 domain、`crates/kemi-core/src/session/`、`crates/kemi-core/src/source/`、`crates/kemi-server/src/{lib,session,units,api}.rs`、`crates/kemi-server/src/api/{comments,channel,agent,submit}.rs`、`src/{main,agent,session,result,notice}.rs`、`crates/kemi-server/tests/{server,live}.rs`、`tests/e2e.rs`・`tests/fixtures/` の固定値、`web/assets/{model,state}.js`・`web/assets/views/{conversation,comment}.js`・`web/assets/features/{agent,comments,live}.js` の既存 plain 消費、`README.md` と `skills/kemi/` の結果・復元説明。
- Done when: runtime の content/第三対象を全構築・match・serializer で扱える。旧 2/3 は旧契約で読み、形式 4 は壊れた parts を拒否し、元 body/引用/未配達イベントを欠損させない。新 result/wait は説明なし・旧復元も版 3 になり、過去 `--result` は再シリアライズしない。初回応答の D1 後、反対単位と凍結の完成前に D2 を受理しても、初回非live payload の両単位は起動 D1、frozen_display_version は D1、稼働中の current は D2、最大発行世代は D2 以上となる。復元は D1 の一覧と token を使い、最大発行世代を下げない。保存先のない既存レビューも認証用 token と別の review 識別子を持ち、メモリ採番で新 result を作れるが explain/復元は提供しない。source 候補の失敗が公開済み参照計画を破壊しない。
- Shown by: test。`cargo test -p kemi-core`、`cargo test -p kemi-server`、`cargo test --test e2e`、`node --test web` を実行する。固定形式 2/3 を読む、4 へ初回保存する、補完だけでは時刻/payload を変えない、rename/I/O 失敗でメモリと保存済み状態・token が旧値を保つ、両単位の候補失敗、非live の D1 凍結/D2 現在から D1 に復元、live 再scan成功/失敗、説明だけの live 保留、新 result/wait の必須/null、古い result の文字列と終了コードを製品境界で RED/GREEN/REFACTOR にする。一般状態保存が payload を触らない既存テストも再実行する。
- Left to implementer: 候補作成/commit の source 境界、既存 persist と受理前保存の helper 分割、lock の実装方式。既存の一般状態保存の警告扱いは保つ。下記の凍結入力と直列化条件を満たす別方式は選べる。
- Stop and hand back if: 新世代発行に保存成功を保証できない、凍結メタデータへ正しい token を対応づけられない、一般状態へファイル一覧/世代配列を足す必要がある、または旧形式を新上限で削る必要がある。

この step の凍結 gate は `crates/kemi-server/tests/server.rs:1442-1466` の GatedSource、`2015-2080` の RecordingSink、`2111-2165` の SlowSink と実保存先を再利用・拡張する。sleep で順序を期待せず、D1 起動 → 初回応答 → D1 反対単位を停止したまま D2 を取り込む → D1 反対単位完成 → D2 反対単位完成 → 初回写し保存 → 保留・復元を制御する。D1/D2 で group/path が異なるメタデータを使い、保存した payload の両一覧、frozen/current/最大世代、復元応答を観測する。D1 完成前にも D2 を受理・公開でき、D1 保存で D2 が巻き戻らないことを確かめる。完了通知の逆順と遅い一般状態保存の交錯も同じ fixture で確認する。これは上記 Shown by の server/session テストに含める。

lock 順の暫定案は、refresh 要求の直列化 → 保存・公開の共通 gate → source 計画の短い公開操作 → review/凍結作成対象 → session → 保存 sink の内部 lock とする。一般状態保存・初回凍結の公開・裏単位完成も共通 gate を通し、その内側から refresh lock を取りに戻らない。source 計画の guard は review/session の取得前に解放する。git/source 計算、内容読取、圧縮、非同期完成待ちは共通 gate の外で行い、commit 時に候補の世代と停止状態を再確認する。初回凍結の保存には D1 の payload と token に加えて最新 session の current/最大世代を使い、古い session snapshot を後から書かない。payload → session の既存保存順も守る。取得順の違う既存経路はこの step 内でそろえる。メタデータと source 参照計画の D1 保持はメモリだけで、一般状態へ比例配列を増やさない。

この lock 列挙は取得順の案であり、保存前の公開を許す順序ではない。共通 gate の中で候補 session を保存し、sink 内部 lock を解放した後、成功した候補だけを source/review/session に公開する。失敗した候補は公開しない。一般保存や凍結保存も同じ gate で順序づける。同等の原子性・待機費用・D1/D2 の観測条件を満たす別の lock 方式が確認された場合は、この内部方式を覆せる。

## Step 5: 画像無効 Markdown・図描画・共通会話表示を実装する

- Purpose: 説明と全対象の返信・発言で同じ Part を安全に読み、描画失敗でも元内容と会話を残す。
- Specification: [R-CONTENT-INPUT](../spec/content-parts.md#r-content-input-入力と互換性)、[R-CONTENT-MARKDOWN](../spec/content-parts.md#r-content-markdown-textと画像)、[R-CONTENT-DIAGRAM](../spec/content-parts.md#r-content-diagram-図の型と描画)、[再利用の境界と調査根拠](../spec/explanation.md#再利用の境界と調査根拠)、[R-RENDER](../spec/kemi.md#r-render-描画表示)、[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)。
- Prerequisites: 4。復元できる形式 4 の parts 会話 fixture。agent-browser と実バイナリを使う browser 検証環境。
- May change: `crates/kemi-core/src/render/`、`crates/kemi-server/src/api.rs`、新規 `crates/kemi-server/src/api/parts.rs`、`crates/kemi-server/tests/server.rs`、`web/assets/views/{conversation,comment}.js` と新規 `web/assets/views/parts*.js`・`web/assets/views/diagrams/`、新規 `web/assets/parts*.js`・`web/assets/diagrams/` とテスト、`web/assets/features/agent.js`・新規 `web/assets/features/parts.js`、`web/assets/{app,actions,api,model,state,dom}.js`、`web/assets/style.css`・新規 `web/assets/parts.css`、`web/index.html`、`PROJECT.md` の feature 依存列挙、新規 `scripts/test-explanation-view.mjs`、既存 `scripts/test-rendered-view.mjs`。取り込み先の license 表示。
- Done when: 旧 plain は装飾されず、新 text は既存安全 Markdown で表示する。全種類の画像参照を非画像枠へ変え、生成時から画像要求経路がない。Rendered ファイルの画像は維持する。text A/code B/text C の A の Markdown 描画に失敗した場合、A だけが理由と完全な元 body の plain 表示になり、B/C の内容・順序とコメントを保持する。返信・発言・説明でも同じ Part 単位の fallback を使い、元 body を再取得や成功した HTML で置き換えない。全 Part を会話の同じ描画入口から順に表示し、code は読み取り専用でコピーできる。図の構造受理と実寸 layout 成功を区別し、font/zoom 後の失敗はその Part の理由と完全 JSON に変わる。図のインライン/別表示操作と閉じた後の元ブロックへの復帰を利用できる。
- Shown by: test。`cargo test -p kemi-core`、`cargo test -p kemi-server`、`node --test web`、`npx tsc -p web --noEmit`、`cargo build --release`、`node scripts/test-explanation-view.mjs target/release/kemi`、`node scripts/test-rendered-view.mjs target/release/kemi`。新 browser script は既存 scripts の fixture/起動/終了処理を再利用する。4 の保存値を復元した会話で外部・相対・参照形式・data・HTML img の要求が発生しないことを計測し、alt/元参照と隣の内容を確認する。実フォントで領域外・ラベル衝突・経路侵入を観測し、許可された接触/交差まで拒否しない。font/zoom の再検証と keyboard/button の操作を RED/GREEN/REFACTOR で示す。
- Left to implementer: 画像無効 Markdown の内部 API、font ready/サイズ変更を検知する仕組み、未契約のボタン配置。通信は feature/API に置き、leaf 同士の import を増やさず計算群を内部ディレクトリに閉じる。新モジュールは modulepreload を更新する。
- Stop and hand back if: parts の画像を生成後に消す方式しか取れない、実フォントを固定値で代用しないと成立しない、または図を成立させるため入力の条件・座標を勝手に修復する必要がある。

Markdown fallback の Browser 観測は上記 Shown by に含める。現行 `render/markdown.rs:116-119` に parser のエラー変換、`web/assets/api.js:8-23,33-52` に fetch/HTTP/JSON の一般失敗経路がある。特定の有効 Markdown が parser を壊すとは未確認なので、その入力を捏造しない。サーバ描画を取得する通常の Part 経路で、ブラウザの通信切断に相当する fetch rejection を A の要求一件へ与える。`scripts/test-live.mjs:3335-3343` の browser eval 内の fetch 差し替え機構を再利用し、A の通常要求から対象を識別する。B/C の要求は実サーバへ通して描画完了を確認する。元 body は描画要求より前からクライアントが保持し、A の失敗処理がコンテナ全体を落とさないことを確認する。描画失敗も通信失敗も同じ Part 単位の失敗表示へ合流させる。5 の時点では形式 4 の復元会話を使い、7 で説明画面へ結線した際にその表示経路を確認する。テスト専用 endpoint・失敗フラグ・production API は足さず、renderer の新しい文字数拒否も作らない。

## Step 6: explain/context・初期説明・parts reply・第三対象 API を公開する

- Purpose: 入力から原子的保存・出力・通知までをつなぎ、説明を会話への返信と混同しない。
- Specification: [R-EXPLANATION-CLI](../spec/explanation-contracts.md#r-explanation-cli-初期投入と全体更新)、[R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-EXPLANATION-DOCUMENT](../spec/explanation.md#r-explanation-document-説明の構造)、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-CONTENT-INPUT](../spec/content-parts.md#r-content-input-入力と互換性)、[R-CONTENT-LIMIT](../spec/content-parts.md#r-content-limit-上限)、[R-INPUT-6](../spec/kemi.md#r-input-6-その他の入出力)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[R-UNIT](../spec/kemi.md#r-unit-グループ単位の切り替え)、[R-SUBMIT](../spec/kemi.md#r-submit-送信と契約)、[R-AGENT-CLI](../spec/agent-channel.md#r-agent-cli-kemi-wait-と-kemi-reply)、[R-AGENT-WRITE](../spec/agent-channel.md#r-agent-write-reply-の入力と上限)、[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)、[R-AGENT-LINK](../spec/agent-channel.md#r-agent-link-つなぎ先と安全)。
- Prerequisites: 5。4 の保存候補・token・版 3 serializer と、起動時メタデータの source 入力記述を利用する。
- May change: `src/{main,agent,session}.rs`、`crates/kemi-core/src/parts/` と 2/3 の domain、`crates/kemi-server/src/{api,lib,session,units}.rs`、`crates/kemi-server/src/api/{agent,channel,comments,submit}.rs`・新規 `crates/kemi-server/src/api/explanation.rs`、`crates/kemi-core/src/source/` の参照メタデータ取得、`crates/kemi-core/src/session/`、`tests/e2e.rs`、`crates/kemi-server/tests/{server,live}.rs`、`web/assets/api.js`、`README.md` と `skills/kemi/{SKILL.md,references/inputs.md,references/results.md,references/conversation.md}`。
- Done when: 各 CLI の file/stdin/context・排他・エラーが承認契約どおりになり、接続保護が既存 agent と同じである。両起動単位について、まだ全一覧を作っていない反対単位の正当な全ファイル参照を含む初期説明を受理し、不正な group/path の組と片側を拒否する。選択的検証では反対単位全体の ReviewMeta/統計/内容を先行生成しない。既知の行数は検証し、本当に不明な場合は行参照だけを拒否して全ファイル参照は受理する。context は一回の整合した読みによる地図を返し、行数不明を追加全内容読取で埋めない。説明更新同士と submit を直列化し、保存失敗や容量拒否で説明・コメント・イベント・版を変えない。parts writes を全件検証し、案は許可対象だけに残す。説明 comment を第三対象として作り、旧版取得と保持/解放の browser API が使える。explain/context は返事済み・hand 行・会話イベント・応答時計を変えない。説明 SSE と Code 更新を別に届ける。
- Shown by: test。`cargo test -p kemi-core`、`cargo test -p kemi-server`、`cargo test --test e2e`。manifest stdin と説明 file、全入力モードの初期説明、CLI 排他・重複キー・競合・保存先障害、同 expected_revision の二更新と submit の両先行順、現在/旧基点の Code 参照、登録/未登録 Page、body/parts と後半不正 writes、hand 保存容量拒否と再配達を RED/GREEN/REFACTOR で示す。Origin/loopback/token の既存保護を新 API に適用する統合テストも含める。復元した旧未配達イベントも版 3 で読めることを確認する。
- Left to implementer: 内部 endpoint 名、直列化 helper の配置、参照地図 DTO と選択的 source 照合の内部生成。source が既に知る片側情報を足す方法は任せるが、行数不明を推測したり追加全ファイル読取を行ったりしない。
- Stop and hand back if: 選択的照合でも承認された対象の対応を保てず、全反対単位の先行構築以外に手段がないと具体例で証明できる、一般 body の旧受理を変更する必要がある、または既知旧基点を歴史的な実在証明として扱う必要がある。未準備という理由だけの初期参照拒否は行わない。

初期参照のテストは上記 Shown by の source/server/e2e に含める。`--group-by file` で範囲内の特定 commit の全ファイル参照、`--group-by commit` で `all` の全ファイル参照をそれぞれ初期投入し、反対単位の全 build を gate で止めたまま開始と初回応答を観測する。不正 group、範囲外/merge commit、別 group にだけある path、変更一覧外の path、改名・追加・削除の片側、曖昧な表示 path、行数既知/不明を区別する。保存先と token は成功時だけ公開する。選択的検証が `review_unit` の全 build や内容取得を呼んでいないことを既存 CountingSource/GatedSource の観測で確かめ、標準の起動性能は 8 で確認する。

## Step 7: EXPLANATION・参照戻り・版保持・通知を全 View へ結線する

- Purpose: 新版通知で読む場所を奪わず、画面自身の Code 一覧に基づいて安全に移動できるようにする。
- Specification: [R-EXPLANATION-VIEW](../spec/explanation.md#r-explanation-view-入口と移動)、[R-EXPLANATION-DOCUMENT](../spec/explanation.md#r-explanation-document-説明の構造)、[R-EXPLANATION-COMMENT](../spec/explanation.md#r-explanation-comment-対象と更新後の会話)、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-VIEW](../spec/kemi.md#r-view-差分の表示)、[R-UNIT](../spec/kemi.md#r-unit-グループ単位の切り替え)、[R-NARROW](../spec/kemi.md#r-narrow-狭い画面)、[R-PAGE-VIEW](../spec/live.md#r-page-view-ページの見方)、[R-AGENT-HAND](../spec/agent-channel.md#r-agent-hand-渡す)。
- Prerequisites: 6。説明取得・第三対象 comment・保持 API と 5 の共通 Part 描画が動くこと。
- May change: `web/assets/{app,state,model,api,actions,dom,storage}.js`、新規 `web/assets/explanation-model.js` とそのテスト、`web/assets/features/explanation.js`・`web/assets/views/explanation.js`、既存 `web/assets/features/{files,units,navigation,comments,agent,conversation,narrow,theme,live}.js`、`web/assets/views/{header,tree,conversation,comment,live}.js`、`web/assets/{style,live}.css`・新規 `web/assets/explanation.css`、`web/index.html`、`PROJECT.md` の feature 依存列挙、変更した web モジュールのテスト、`scripts/test-explanation-view.mjs`、`scripts/test-narrow-screen.mjs`。
- Done when: 説明あり/なしの初期入口、目次、第三対象コメントと完全引用を表示できる。全 View の Conversation の下書き・開閉・スレッドを保つ。最新説明、読む版、通知の版、Code 一覧の token を別に持ち、会話/説明再取得が Code cache や表示版を更新しない。基点一致時には保証範囲の注意を常時出さず、詳細または tooltip を開くと内容固定でも説明の正しさの証明でもないと読める。この入口と不一致時の通知を区別し、font/zoom の再描画で不一致扱いにしない。Code 用の別単位 cache も世代を混ぜず、参照移動のためサーバ最新を自動取得しない。現在の一覧で対象・片側・行を厳密に解ける場合だけ移動し、取得後の行不一致も縮めず理由を表示する。旧版の pin を確保してから移動し、版・項目・開閉・位置へ戻る。新版を開く前に読む版を変えず、Seen を付けない。390px でも同じ操作ができる。
- Shown by: test。`node --test web`、`npx tsc -p web --noEmit`、`cargo build --release`、`node scripts/test-explanation-view.mjs target/release/kemi`、`node scripts/test-narrow-screen.mjs target/release/kemi`。旧版中ほどから Code/Page へ移り新版通知後に戻る、同 ID/同内容コメントの現在表示、削除/再登場、参照不可・行数不明・消失時の理由、説明だけの通知と Code バッジの区別、旧版と新版通知の鮮度の別判定、会話/下書き/Seen の維持を RED/GREEN/REFACTOR で示す。SSE 欠落からの説明再取得も Code を取り込まないことを確認する。
- Left to implementer: View 切替 feature の内部位置、読み位置の保持方法、保持要求の内部識別子。初期 Code 取得前の鮮度保留を明示する。live モジュールは非live へ import しない。別単位未取得のリンクは自動で新しい一覧へ飛ばさず、利用不可と理由を示す。
- Stop and hand back if: 見方とグループ単位を同じ state で扱う必要がある、参照を任意 path 読取に変える必要がある、切断で保持を解放しないと容量設計が成立しない、または未コミットモックの採用が必要。

基点の情報入口の Browser 観測は上記 Shown by に含め、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度) の保証範囲の情報入口と成功条件を正解にする。D1 説明/D1 Code の表示では常時注意がなく、利用者が詳細/tooltip を開いたときに二つの非保証を読めることを確認する。次にその画面だけ D2 の一覧を取り込み、不一致通知と対応する二つの版が出て、保証範囲の入口とは別であることを確認する。font/zoom 変更では token と判定が変わらないことも観測する。文言の完全一致や内部 DOM 名は固定しない。5 の Part 単位 fallback が説明の A/B/C とコメントも残すことを、通常の描画要求の通信失敗で確認する。

## Step 8: 複数画面・復元・容量・性能と既存操作を実環境で確認する

- Purpose: 各段階の境界が結線後も成り立ち、既存 Code/Page の性能と操作を損なわないことを確認する。
- Specification: [R-EXPLANATION-VIEW](../spec/explanation.md#r-explanation-view-入口と移動)、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-CONTENT-LIMIT](../spec/content-parts.md#r-content-limit-上限)、[R-SERVE](../spec/kemi.md#r-serve-配信モデル)、[R-UNIT](../spec/kemi.md#r-unit-グループ単位の切り替え)、[R-SESSION](../spec/kemi.md#r-session-セッションの保存と復元)、[R-VERIFY](../spec/kemi.md#r-verify-検証)、[R-PAGE-SESSION](../spec/live.md#r-page-session-保留と復元)。
- Prerequisites: 7。agent-browser、release バイナリ、git、既存 browser scripts の必要ツールが使えること。全検証の状態保存先は一時領域で、利用者の本物の状態を使わない。
- May change: `tests/e2e.rs`・fixture、`crates/kemi-server/tests/{server,live}.rs`、既存 domain/session/web テスト、`scripts/test-explanation-view.mjs`、`scripts/test-{agent-channel,rendered-view,narrow-screen,live}.mjs`。観測で見つかった実装原因は 2〜7 の May change 内で直し、該当 step の対象検証を再実行する。
- Done when: A だけの Code 更新後、B は旧 Code token のまま新版説明通知を読み、context はサーバ現在版を返す。元説明 file を削除しても非live 凍結版と live 新scan の違い、図の旧引用・parts・未配達 hand を復元できる。説明なしで 4097 回の Code 更新を受理し、説明上限に達した状態でも Code 更新が続く。ファイル数/世代数だけを増やしても一般 state 保存に新しい比例配列や payload I/O がない。初期モードの組合せと既存 Page 操作、非live の live 資産非読込を確認し、標準性能条件を満たす。
- Shown by: test。順に `cargo test`、`node --test web`、`npx tsc -p web --noEmit`、`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`scripts/check-domain-purity.sh`、`cargo build --release` を実行する。Browser の対象コマンドは `node scripts/test-explanation-view.mjs target/release/kemi`、`node scripts/test-agent-channel.mjs target/release/kemi`、`node scripts/test-rendered-view.mjs target/release/kemi`、`node scripts/test-narrow-screen.mjs target/release/kemi`、`node scripts/test-live.mjs target/release/kemi`。5/7 から同じコード状態で成功済みのケースは再実行せず、最終結線の変更が届くケースと未確認ケースを実行する。live の全 script は一回で直前統合の5群も含むので、成功後に同じスモーク flag を全部追加実行しない。新規の失敗を追加する場合は製品動作の RED を先に示す。
- Left to implementer: 二画面を別 browser session/context で作る方法、既存 fixture の最小拡張。容量拒否テストは実際の正準サイズから作り、内部実装のメモリ量を契約扱いしない。
- Stop and hand back if: 実ブラウザや標準計測が使えず決定的観測が欠ける、または性能条件を守るため承認上限や遅延読みを変える必要がある。型・ユニットが成功しても browser 未確認を合格扱いしない。

性能はこの step 内で既存 script により測る。作業用一時領域に `scripts/gen-fixture.sh <startup-fixture> --files 10000 --lines 20` と `scripts/gen-fixture.sh <range-fixture> --files 100 --lines 20 --commits 201` を作り、`scripts/measure-startup.sh <startup-fixture> target/release/kemi`、`scripts/measure-range.sh <range-fixture> target/release/kemi commit`、同 `file`、同 `busy` を実行する。相対指定のバイナリは script 起動前に絶対解決して渡す。測定は release、既存の中央値/最遅応答と正典の閾値を使う。新しい説明の規模に未承認の時間上限を設けない。起動/api/review/context による新しい全内容読取がないことは source の既存観測用 fixture で確認し、既存の応答後 payload 凍結とは区別する。

live のケース重複は `scripts/test-live.mjs:3976-4036` の全ケース呼出と `4041-4056` の flag 分岐を照合した。失敗を直した後の局所再実行だけに、`node scripts/test-live.mjs target/release/kemi --places-glow`、同 `--pen-places`、同 `--compose-layout`、同 `--page-surrounding`、同 `--baseline-guide` のうち修正が届くものを使う。追加変更や未解決の影響がなければ全 script をもう一度回さない。

## Step 9: README と配布 skill の契約説明を照合して仕上げる

- Purpose: 利用者とエージェントが新旧契約、説明更新と返信、保証範囲を取り違えず使えるようにする。
- Specification: [文書照合の根拠と承認後の反映先](../spec/explanation-contracts.md#文書照合の根拠と承認後の反映先)、[R-EXPLANATION-CLI](../spec/explanation-contracts.md#r-explanation-cli-初期投入と全体更新)、[R-EXPLANATION-RESULT](../spec/explanation-contracts.md#r-explanation-result-resultとwaitの版3)、[R-EXPLANATION-SESSION](../spec/explanation-contracts.md#r-explanation-session-保存と復元)、[R-EXPLANATION-BASE](../spec/explanation.md#r-explanation-base-差分表示版と鮮度)、[R-VERIFY](../spec/kemi.md#r-verify-検証)。
- Prerequisites: 8。4/6 で同時更新した派生文書と、実 CLI/result/wait/context の検証結果。
- May change: `README.md`、`skills/kemi/SKILL.md`、`skills/kemi/references/{inputs,results,conversation}.md`、`docs/README.md`。新しい説明 reference が必要なら `skills/kemi/references/` 内で一 topic にまとめ、入口からリンクする。
- Done when: 英語の README/skill が正典の入力・排他・版・null・保存と一致し、普通の補足は reply、本文訂正は explain という使い方を示す。context token を byte/意味保証と呼ばず、初期説明・旧 plain・図付き parts・旧 result 再出力・復元の違いが読める。仕様の複製を docs に増やさず、既存 docs 追記を残す。
- Shown by: check。`scripts/check-skill-format.sh`、`scripts/check-skill-frontmatter.sh`、`git diff --check`。CLI/JSON 変更の成果物と README/skill の対応は R-VERIFY に従って人が最終レビューで照合する。実装途中の手順ごとの承認は求めない。
- Left to implementer: 英語の説明文と架空例の題材。例は承認済み形式を使い、実在の review token や機密・利用者固有 path を載せない。
- Stop and hand back if: 派生文書を一致させるには公開契約を変える必要がある。配布・タグ・製品 version の更新はこの step で行わない。
