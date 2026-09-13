# Windows を配布ターゲットに追加する

## Goal

kemi が Windows（x86_64 / aarch64）の配布ターゲットでビルド・テスト・配布できるようになる。利用者は GitHub Releases の zip を解いて `kemi.exe` を使う。Windows 固有の実装は、結果ファイルの置き場所（`%LOCALAPPDATA%`）、URL トークンの生成（getrandom）、ブラウザの自動起動の 3 箇所に収まる。

## Specification

`docs/spec/kemi.md` — 実装で参照する節は R-DIST、R-RESULT、R-SERVE、R-DEPS、R-INPUT、R-VERIFY。該当ステップに引用する。

## Approach and why

- 仕様（R-DIST / R-RESULT / R-DEPS / R-VERIFY）は承認済みで、Windows 対応の契約は定まった。`LOCALAPPDATA` の相対パスの扱いも `88d78d8` で仕様に定めた。
- コードの変更は 3 箇所に分かれる:
  1. 結果ファイルの置き場所 — `result::results_dir` は純粋関数（環境変数は呼び出し側が読む）。Windows では `LOCALAPPDATA`（絶対パス）を使い、相対・未設定なら「決められない」。unix と同じ `~/.local/state` フォールバックは持たない。`XDG_STATE_HOME` が絶対パスなら全 OS で最優先。
  2. URL トークン — 現在 `/dev/urandom` を読み、失敗時は時刻+pid に落ちる（Windows では常にこの弱いフォールバック）。`getrandom` に置き換え、フォールバックを消す。出力の形（現在の 32 桁 16 進）は後方互換のため保つ。
  3. ブラウザ起動 — 現在 macOS は `open`、それ以外は `xdg-open`。Windows ではどちらも無いので、既定で自動で開く挙動を Windows でも実現する（機構は実装委譲）。
- テストは Windows の CI ジョブ（`.github/workflows/ci.yml` に windows を追加）で走らせる。Windows 専用の分岐は `#[cfg(windows)]` のテストと、Linux 上からのクロスターゲット `cargo check`（リンクしないので MSVC 無しで通る）で検証する。
- 配布は `dist-workspace.toml` の `targets` に Windows 2 つを足し、`pr-run-mode = "upload"` にして PR でもビルドジョブを走らせる。これで `aarch64-pc-windows-msvc` のクロスコンパイルをリリース前の PR で検証でき、ビルドできない場合は R-DIST の退路（x86_64 のみに落とす）を適用できる。コミット済みの `.github/workflows/release.yml` を cargo-dist で再生成する。

## Scope of change

変更してよいファイル:

- `src/result.rs`（`results_dir` の引数と Windows 分岐、テスト）
- `src/main.rs`（`results_dir` の呼び出し、`random_token`、`open_browser`、結果ディレクトリのエラーメッセージ 3 箇所）
- `Cargo.toml` / `Cargo.lock`（`getrandom` の追加）
- `.github/workflows/ci.yml`（Windows のテストジョブ）
- `dist-workspace.toml`（Windows ターゲット 2 つと `pr-run-mode = "upload"`）
- `.github/workflows/release.yml`（cargo-dist による再生成）

変更しないもの:

- `docs/spec/kemi.md` と CONTEXT.md（承認済み。実装で変えてはならない）
- URL の形・CLI の契約・JSON の I/F
- `web/`（フロントには Windows 固有の変更が無い）
- README / `skills/kemi/SKILL.md`（仕様改訂のコミットで反映済み。CLI と JSON の契約は変わらない）

## Step order and prerequisites

Step 1 → 2 → 3 → 4 → 5 → 6。Step 1-3 は順不同でよいが、Windows の `#[cfg(windows)]` テストが CI で走るのは Step 4 の後。各ステップの前提はそのステップに書く。

## Verification map

- Step 1: R-RESULT（置き場所、成功条件、反例）
- Step 2: R-SERVE（URL トークン）、R-DEPS（getrandom、C 依存なし）
- Step 3: R-INPUT（`--no-open` の既定はブラウザを自動で開く）
- Step 4: R-VERIFY（Windows で `cargo test`）、R-RESULT（Windows の `#[cfg(windows)]` テストが CI で実行される）
- Step 5: R-DIST（6 ターゲット、Windows は zip + `.sha256`、PR でビルドが走る）
- Step 6: R-DIST（`aarch64-pc-windows-msvc` のビルド検証と退路）

## Left to the implementer

- `results_dir` の内部構造（分岐の書き方。契約は「XDG 絶対パス最優先 → Windows では LOCALAPPDATA 絶対パス、相対・未設定なら決められない → unix では HOME/.local/state、無ければ決められない」）
- 結果ディレクトリのエラーメッセージ 3 箇所の文言 — 英語のまま、その OS で使う環境変数（unix は HOME、Windows は LOCALAPPDATA）を指す文言にする（D1）
- `random_token` の getrandom API の選び方（`getrandom` / `fill`。どちらも承認済みの挙動を保つ）と、失敗時の `fail()`（既存の汎用エラーパス）への落とし方
- `open_browser` の Windows での開き方の機構（例: `cmd /C start "" <url>` と、コンソールの窓を出さない `CREATE_NO_WINDOW`。機構は仕様の契約ではない）
- cargo-dist で release.yml を再生成するコマンド
- コミットの分割（1 関心ごと）

## Stop conditions

- 仕様 R-RESULT の契約（XDG 優先、LOCALAPPDATA 相対・未設定は決められない）と異なる結果置き場所の実装が必要になる場合
- URL の形、CLI の出力、JSON の I/F を変えなければならない場合
- `getrandom` のバックエンドが C 実装に依存する（`cargo tree` で確認）、またはライセンス検査 `cargo deny check licenses` が失敗する場合
- Windows のテストを直すために、仕様が契約する挙動（出力形式など）を変える必要がある場合
- `aarch64-pc-windows-msvc` のビルドが CI で失敗し、R-DIST の退路（x86_64 のみに落とす）を適用する判断が必要になる場合

## Test command

プロジェクトの規約（PROJECT.md）が定める次のコマンドを使う:

- テスト: `cargo test`
- lint: `cargo clippy -- -D warnings`
- 整形: `cargo fmt --check`
- 依存ライセンス: `cargo deny check licenses`

Windows の分岐を Linux 上から検証するため、次のクロスターゲットのコンパイル確認を足す（プロジェクトの規約は定めていないので、この計画が決める）。前提として `rustup target add x86_64-pc-windows-msvc` を先に実行する:

- `cargo check --workspace --tests --target x86_64-pc-windows-msvc`

リンクしないので MSVC ツールチェーンは要らない。Windows の分岐と `#[cfg(windows)]` のテストがコンパイルできることを確かめる。このクロスターゲット確認を使うステップ（Step 1・2・3）は、それぞれの前提に `rustup target add` を挙げる。

## Out of scope

- Windows の実機での動作確認（手元に環境が無い。R-VERIFY の「Windows マシンを持つ人が zip の `kemi.exe` が `--version` を出すことを確認する」はこの計画の完了条件に含まれず、リリース後に利用者が行う外部検証として記録する）
- MSI / PowerShell インストーラ（`installers = []` のまま）
- mise の Windows での動作確認（仕様は保証しない）
- macOS の CI ジョブ追加（R-VERIFY の「ubuntu で代表する」）

---

## Step 1 — 結果ファイルの置き場所を Windows 対応にする

Purpose: `result::results_dir` が Windows では `%LOCALAPPDATA%\kemi\results\` を使い、`LOCALAPPDATA` が相対・未設定なら「決められない」とする。
Specification: `docs/spec/kemi.md`#R-RESULT。
Prerequisites: なし。
May change: `src/result.rs`、`src/main.rs`（`results_dir` の呼び出しとエラーメッセージ 3 箇所）。
Done when: `results_dir` のシグネチャが 3 引数（XDG / HOME / LOCALAPPDATA）になり、`XDG_STATE_HOME` が絶対パスなら全 OS で最優先、Windows では `LOCALAPPDATA` が絶対パスなら `LOCALAPPDATA/kemi/results`、相対・未設定なら `None`、unix では `HOME/.local/state/kemi/results`、無ければ `None` を返す。`src/main.rs` の呼び出しが `LOCALAPPDATA` を渡し、エラーメッセージ 3 箇所が英語で、Windows では LOCALAPPDATA を指す（HOME に言及しない）。Windows での実挙動（LOCALAPPDATA での置き場所、相対・未設定で起動エラー）は Step 4 の CI で観測される。
Shown by: test — `cargo test` で RED→GREEN→REFACTOR。既存の `result_dir_uses_xdg_state_home_only_when_absolute` を 3 引数に直し、`#[cfg(windows)]` で「LOCALAPPDATA 絶対パス → その下の kemi/results」「XDG 絶対パスは LOCALAPPDATA より優先」「LOCALAPPDATA 無し → None」「LOCALAPPDATA 相対 → None」のテストを足し、`#[cfg(not(windows))]` で「unix では LOCALAPPDATA 引数を無視する」テストを足す。Windows のテストは `rustup target add x86_64-pc-windows-msvc` の後の `cargo check --workspace --tests --target x86_64-pc-windows-msvc` でコンパイルを確認し、実実行は CI（Step 4）で行う。
Left to the implementer: 分岐の書き方、エラーメッセージの文言。
Stop and hand back if: なし（一般の 4 条件に加えて）。

## Step 2 — URL トークンを getrandom で生成する

Purpose: `random_token` を `getrandom` から生成し、`/dev/urandom` の読み取りと時刻+pid のフォールバックを消す。
Specification: `docs/spec/kemi.md`#R-SERVE（トークン）、#R-DEPS（getrandom、C 依存なし）。
Prerequisites: `rustup target add x86_64-pc-windows-msvc`（クロスターゲット確認のため）。
May change: `Cargo.toml`、`Cargo.lock`、`src/main.rs`（`random_token`）。
Done when: `random_token` が `getrandom` からの乱数で 16 バイトを埋め、現在の 32 桁 16 進の出力を保つ（後方互換のため。仕様は形式を契約していない）。`/dev/urandom` の読み取りと時刻+pid フォールバックのコードが無い。
Shown by: check — `cargo check --target x86_64-pc-windows-msvc`（getrandom の Windows バックエンドがコンパイルできる）、`cargo test`（既存が GREEN）、`cargo clippy -- -D warnings`、`cargo deny check licenses`、`cargo tree -e features` に oniguruma 系の C 依存が現れない（R-DEPS）。テストは新たに足さない: トークンの形式と一意性は仕様が固定しておらず、観測可能な振る舞いを変えない依存の差し替えで、RED になるテストが無い。
Left to the implementer: getrandom の 0.3 系の版、`getrandom` / `fill` のどちらを使うか、失敗時の `fail()` への落とし方。
Stop and hand back if: `getrandom` のバックエンドが C 実装に依存する（`cargo tree` で確認。R-DEPS の musl 静的リンクに影響する）、または `cargo deny check licenses` が失敗する。

## Step 3 — ブラウザの自動起動を Windows 対応にする

Purpose: `open_browser` が Windows でも既定でブラウザを自動で開く。
Specification: `docs/spec/kemi.md`#R-INPUT（`--no-open` を付けなければ自動で開く）。
Prerequisites: `rustup target add x86_64-pc-windows-msvc`（クロスターゲット確認のため）。
May change: `src/main.rs`（`open_browser`）。
Done when: Windows では `--no-open` を付けなければブラウザが自動で開く。macOS は `open`、それ以外は `xdg-open` のまま。
Shown by: check — `cargo check --target x86_64-pc-windows-msvc`（Windows 分岐と `std::os::windows::process::CommandExt` がコンパイルできる）、`cargo clippy -- -D warnings`、`cargo test`（既存が GREEN）。テストは新たに足さない: ブラウザの起動は副作用で、テストで spawn しても CI で不安定になる。
Left to the implementer: Windows で開く機構（例: `cmd /C start "" <url>` と、コンソールの窓を出さない `CREATE_NO_WINDOW`。機構は仕様の契約ではない）。
Stop and hand back if: 仕様が契約する挙動（`--no-open` の既定で自動で開く）を変える必要が出た場合。

## Step 4 — CI に Windows のテストジョブを足す

Purpose: `.github/workflows/ci.yml` に windows-latest の `cargo test` ジョブを足し、Windows の `#[cfg(windows)]` テストを含む全テストが CI で走るようにする。
Specification: `docs/spec/kemi.md`#R-VERIFY（Windows で `cargo test`）。
Prerequisites: Step 1-3 完了（Windows の分岐とテストが実装済み）。
May change: `.github/workflows/ci.yml`。
Done when: `ci.yml` に `windows-latest` ランナーで `cargo test` を実行するジョブがある。ブランチの CI 実行で Windows ジョブが green になる。
Shown by: external — ブランチの CI 実行（push / PR で走る）の Windows ジョブが green。`ci.yml` の内容（windows-latest のジョブがある）は Done when の前提として同じ変更で確認する。
Left to the implementer: ジョブ名と構成（既存の `rust` ジョブと並べる）。
Stop and hand back if: Windows で通らないテストがあり、直すために仕様が契約する挙動（出力形式、URL の形、CLI の出力）を変える必要がある場合。Windows 固有の前提（パス区切り、git の出力、環境変数）の吸収は実装の範囲。

## Step 5 — 配布ターゲットに Windows を足し、release.yml を再生成する

Purpose: `dist-workspace.toml` の `targets` に `x86_64-pc-windows-msvc` と `aarch64-pc-windows-msvc` を足し、`pr-run-mode = "upload"` で PR でもビルドジョブが走るようにして、コミット済みの `.github/workflows/release.yml` を cargo-dist で再生成する。
Specification: `docs/spec/kemi.md`#R-DIST（6 ターゲット、Windows は zip + `.sha256`、PR でビルドを検証）。
Prerequisites: なし（コードの変更とは独立）。
May change: `dist-workspace.toml`、`.github/workflows/release.yml`。
Done when: `dist-workspace.toml` の `targets` に Windows の 2 ターゲットがあり、`pr-run-mode = "upload"` がある。再生成した `release.yml` のビルドマトリクスが Windows ターゲットを含み、PR でもビルドジョブが走る。cargo-dist の出力で Windows の成果物が `kemi-<target>.zip` と対応する `.sha256` になることを確認できる。
Shown by: check — cargo-dist の再生成コマンドを実行し、その出力と再生成後の `release.yml` で、Windows ターゲットがマトリクスに現れ、成果物が `.zip` + `.sha256` になることを確認。
Left to the implementer: cargo-dist で `release.yml` を再生成するコマンド。
Stop and hand back if: cargo-dist の再生成が 6 ターゲットと `pr-run-mode` を正しく反映しない場合。

## Step 6 — aarch64-pc-windows-msvc のビルドを CI で検証する

Purpose: 仕様 R-DIST の退路（ビルドできなければ x86_64 のみに落とす）を判断できるよう、`aarch64-pc-windows-msvc` のクロスコンパイルを CI で検証する。
Specification: `docs/spec/kemi.md`#R-DIST（`aarch64-pc-windows-msvc` は CI の Windows ランナー上でのクロスコンパイルでビルドを検証し、ビルドできない場合は `x86_64-pc-windows-msvc` のみに落とす）。
Prerequisites: Step 5（`pr-run-mode = "upload"` と再生成済みの `release.yml`）。
May change: `.github/workflows/release.yml`（ビルド失敗時に R-DIST の退路を適用する場合のみ）。
Done when: ブランチの PR の release.yml 実行（build-local-artifacts）で、`aarch64-pc-windows-msvc` を含む Windows ターゲットのビルドが成功する。失敗した場合は、R-DIST の退路に従って `x86_64-pc-windows-msvc` のみに落として再検証する。
Shown by: external — ブランチの PR の release.yml 実行（build-local-artifacts）のビルドジョブが、Windows ターゲットを含めて green。
Left to the implementer: なし。
Stop and hand back if: `aarch64-pc-windows-msvc` のビルドが ARM64 MSVC ツールチェーンの不足で失敗し、その設定の追加（cargo-dist の `dependencies` 等）が必要になる判断が出た場合。退路の適用（x86_64 のみに落とす）は R-DIST が定める。