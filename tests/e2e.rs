//! バイナリを起動する e2e（CLI、exit code、stdout の JSON、live stderr）。

#![expect(
    clippy::unwrap_used,
    reason = "#[test] の外の補助関数も、失敗をそのままテストの失敗として見せる"
)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kemi-e2e-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn write(&self, relative: &str, content: &str) {
        let full = self.path.join(relative);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const MANIFEST: &str = r#"{
  "title": "e2e のレビュー",
  "groups": [
    {
      "id": "g1",
      "title": "変更",
      "watch": "見てほしい点",
      "diffs": [
        { "path": "a.txt", "old": "one\ntwo\n", "new": "one\nTWO\n", "focus": true, "note": "重点" }
      ]
    }
  ],
  "approval": [{ "path": "a.txt", "identity": "sha256:e2e" }]
}"#;

/// kemi を起動するコマンド。結果ファイルが利用者の本物の状態ディレクトリへ書かれない
/// よう、`XDG_STATE_HOME` と `HOME` を必ず `state` の下に向ける。Windows の
/// `<id>.endpoint` は `XDG_STATE_HOME` に関わらず `LOCALAPPDATA` の下に置かれるので、
/// それも向ける。
fn kemi_command(dir: &Path, state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kemi"));
    command
        .current_dir(dir)
        .env("XDG_STATE_HOME", state)
        .env("HOME", state.join("home"))
        .env("LOCALAPPDATA", state.join("localappdata"));
    command
}

/// 子プロセスがまだ動いていれば止めて回収する。テストが途中で失敗しても、起動した kemi を
/// cargo test の後に残さないよう、`Drop` から呼ぶ。止め終えたものには何もしない。
fn stop(child: &mut Child) {
    if let Ok(None) = child.try_wait() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// `Kemi` を通さずに起動した子プロセスを、捨てるときに止める。
struct StopOnDrop(Child);

#[path = "support/process.rs"]
mod process;

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        stop(&mut self.0);
    }
}

struct Kemi {
    child: Child,
    stdout: process::CapturedPipe,
    stderr: process::CapturedPipe,
    url: String,
    /// URL の行より前に stderr へ出た行。
    preamble: Vec<String>,
    _state: Option<TempDir>,
}

impl Kemi {
    fn spawn(dir: &Path, args: &[&str]) -> Self {
        let state = TempDir::new();
        let mut kemi = Kemi::spawn_with_state(dir, args, &state.path);
        kemi._state = Some(state);
        kemi
    }

    fn spawn_with_state(dir: &Path, args: &[&str], state: &Path) -> Self {
        Kemi::start(kemi_command(dir, state).args(args))
    }

    /// 組み立てたコマンドで起動し、stderr の URL の行を待つ。
    fn start(command: &mut Command) -> Self {
        Self::start_with_input(command, None)
    }

    fn start_with_input(command: &mut Command, input: Option<&str>) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = process::CapturedPipe::new(child.stdout.take().unwrap());
        let stderr = process::CapturedPipe::new(child.stderr.take().unwrap());
        let mut kemi = Kemi {
            child,
            stdout,
            stderr,
            url: String::new(),
            preamble: Vec::new(),
            _state: None,
        };
        if let Some(input) = input {
            kemi.child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        let mut line = String::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        kemi.url = loop {
            assert!(
                std::time::Instant::now() < deadline,
                "kemi did not announce a URL in time"
            );
            line.clear();
            let read = kemi.stderr.read_line(&mut line).unwrap();
            if read == 0 {
                let status = process::wait(&mut kemi.child, std::time::Duration::from_secs(20));
                panic!(
                    "kemi が URL を出さずに終了しました\n終了状態: {status:?}\nURL より前の stderr:\n{}",
                    kemi.preamble.concat()
                );
            }
            if let Some(found) = line.strip_prefix("kemi: http") {
                break format!("http{found}").trim().to_string();
            }
            kemi.preamble.push(line.clone());
        };
        kemi
    }

    fn origin(&self) -> String {
        self.url.split("/s/").next().unwrap().to_string()
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}{path}", self.url))
            .header("Origin", self.origin())
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    fn wait(mut self) -> (std::process::ExitStatus, String) {
        let status = process::wait(&mut self.child, std::time::Duration::from_secs(20)).unwrap();
        let mut stdout = String::new();
        self.stdout.read_to_string(&mut stdout).unwrap();
        (status, stdout)
    }

    fn kill(mut self) {
        stop(&mut self.child);
    }

    /// サーバが応答するまで待つ。応答した時点で、サーブ前の stderr の行は出終わり、
    /// Ctrl+C の受け取りも始まっている。
    async fn wait_serving(&self) {
        let response = reqwest::get(format!("{}api/review", self.url))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let _ = response.bytes().await;
    }
}

impl Drop for Kemi {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}

/// プロセスの終了を待つ。時間内に終わらなければパニックする。
async fn wait_for_exit(kemi: &mut Kemi, timeout: std::time::Duration) -> std::process::ExitStatus {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(status) = kemi.child.try_wait().unwrap() {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "kemi が時間内に終了しませんでした"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    let state = TempDir::new();
    run_with_state(dir, args, &state.path)
}

/// CLI の出力が英語であることの検証（R-DIST の成功条件）。ひらがな・カタカナ・
/// 漢字（CJK 統合漢字と拡張 A）のいずれかを検出したら日本語とする。
fn contains_japanese(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character as u32,
            0x3040..=0x309f
                | 0x30a0..=0x30ff
                | 0x3400..=0x4dbf
                | 0x4e00..=0x9fff
        )
    })
}

fn run_with_state(dir: &Path, args: &[&str], state: &Path) -> std::process::Output {
    kemi_command(dir, state).args(args).output().unwrap()
}

/// フィクスチャを作る git。開発者の全体・システムの設定（署名の program、
/// commit.gpgsign、diff.orderFile など）を読まない。読むと、同じテストが環境によって
/// 失敗する。全体の設定の置き場は一時リポジトリの中の無いファイルにし、書かれても
/// 外へ漏れない。利用者の設定を kemi に読ませるテストは、これを使わず
/// `git_with_global` で設定を明示する。
fn fixture_git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            dir.join(".git").join("test-global-config"),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

fn git(dir: &Path, args: &[&str]) {
    let output = fixture_git(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn digest_manifest() -> String {
    serde_json::json!({
        "title": "digest のテスト",
        "groups": [
            {
                "id": "g1",
                "title": "最初",
                "why": "理由",
                "watch": "watch",
                "diffs": [
                    { "path": "src/a.rs", "old": "a\nb\n", "new": "a\nB\nc\n" },
                    { "path": "src/sub/b.rs", "new": "x\n" },
                    { "path": "Cargo.lock", "old": "x\n", "new": "y\n" }
                ]
            },
            {
                "id": "g2",
                "title": "次",
                "diffs": [
                    { "path": "README.md", "old": "one\ntwo\n", "new": "one\n" }
                ]
            }
        ],
        "approval": []
    })
    .to_string()
}

/// base → 2 コミットの小さなリポジトリ。base の sha を返す。
fn two_commit_repo(dir: &TempDir) -> String {
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "kemi@example.com"]);
    git(&dir.path, &["config", "user.name", "kemi"]);
    git(&dir.path, &["config", "core.hooksPath", "/dev/null"]);
    dir.write("a.txt", "one\n");
    git(&dir.path, &["add", "-A"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    let base = String::from_utf8(run_git(&dir.path, &["rev-parse", "HEAD"])).unwrap();
    dir.write("a.txt", "two\n");
    git(&dir.path, &["commit", "-q", "-am", "feat: two"]);
    dir.write("b.txt", "b\n");
    git(&dir.path, &["add", "-A"]);
    git(&dir.path, &["commit", "-q", "-m", "feat: b"]);
    base.trim().to_string()
}

fn run_git(dir: &Path, args: &[&str]) -> Vec<u8> {
    let output = fixture_git(dir).args(args).output().unwrap();
    assert!(output.status.success());
    output.stdout
}

// ---- R-RESULT（結果ファイルと kemi --result）----

fn results_dir(state: &Path) -> PathBuf {
    state.join("kemi").join("results")
}

fn result_files(state: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(results_dir(state))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// manifest を送って submit し、(submit の応答, stdout, 終了コード) を返す。
async fn submit_manifest(
    dir: &Path,
    state: &Path,
    verdict: &str,
    body: &str,
) -> (serde_json::Value, String, Option<i32>) {
    let manifest = MANIFEST.replace("e2e のレビュー", body);
    std::fs::write(dir.join("manifest.json"), manifest).unwrap();
    let kemi = Kemi::spawn_with_state(dir, &["manifest.json", "--no-open", "--port", "0"], state);
    let response = kemi
        .post("api/submit", serde_json::json!({ "verdict": verdict }))
        .await;
    assert_eq!(response.status(), 200);
    let answer: serde_json::Value = response.json().await.unwrap();
    let (status, stdout) = kemi.wait();
    (answer, stdout, status.code())
}

fn git_repo(dir: &TempDir) {
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "kemi@example.com"]);
    git(&dir.path, &["config", "user.name", "kemi"]);
}

/// manifest のレビューを承認で終える。submit の応答、終了コード、stdout と、
/// URL の行より後に stderr へ出た行を返す。
async fn approve_manifest_review(
    state: &Path,
) -> (
    serde_json::Value,
    std::process::ExitStatus,
    String,
    Vec<String>,
) {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn_with_state(
        &dir.path,
        &["manifest.json", "--no-open", "--port", "0"],
        state,
    );
    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let answer: serde_json::Value = response.json().await.unwrap();
    let mut stderr = String::new();
    kemi.stderr.read_to_string(&mut stderr).unwrap();
    let (status, stdout) = kemi.wait();
    let lines = stderr.lines().map(str::to_string).collect();
    (answer, status, stdout, lines)
}

// ---- 利用者の git の設定 ----

/// git の設定ファイルに書くパス。Windows の一時ディレクトリは `\` を含むので、git config が
/// エスケープとして解釈しないよう `/` に置き換える。
fn git_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// git の設定ファイルに書く file URL。Windows はドライブ文字の前に `/` が要る
/// （`file:///C:/...`）。unix は従来通り `file:///tmp/...` になる。
fn file_url(path: &Path) -> String {
    let path = git_path(path);
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// `GIT_CONFIG_GLOBAL` を一時ファイルに向けて、システムの設定は読ませずに git を呼ぶ。
fn git_with_global(dir: &Path, global: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// 利用者の本物の設定の代わりに読ませる、一時ファイルの git の全体設定。
/// コミットに要る設定に `extra` を足す。
fn global_config(state: &TempDir, extra: &str) -> PathBuf {
    let global = state.path.join("gitconfig");
    std::fs::write(
        &global,
        format!(
            "[user]\n\tname = kemi\n\temail = kemi@example.com\n\
             [core]\n\thooksPath = /dev/null\n{extra}"
        ),
    )
    .unwrap();
    global
}

/// `global` だけを git の設定として読ませて kemi を起動する。
fn start_with_global(dir: &Path, state: &TempDir, global: &Path, args: &[&str]) -> Kemi {
    Kemi::start(
        kemi_command(dir, &state.path)
            .env("GIT_CONFIG_GLOBAL", global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(args),
    )
}

/// 最終形の `path` のファイルの由来を取りに行く。
async fn fetch_origin(kemi: &Kemi, path: &str) -> reqwest::Response {
    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = review["groups"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    reqwest::get(format!("{}api/origin/{id}", kemi.url))
        .await
        .unwrap()
}

/// `.gitattributes` で `*.txt` に表示用の変換 `cut` を割り当てたリポジトリに、
/// 5 行目を変えるコミットと 8 行目を消すコミットを積む。(基点, 変えた, 消した) を返す。
fn textconv_repo(dir: &TempDir, global: &Path) -> (String, String, String) {
    let git = |args: &[&str]| git_with_global(&dir.path, global, args);
    git(&["init", "-q"]);
    let base_text: String = (1..=10).map(|n| format!("line {n}\n")).collect();
    dir.write(".gitattributes", "*.txt diff=cut\n");
    dir.write("f.txt", &base_text);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    let base = git(&["rev-parse", "HEAD"]);
    let changed_text = base_text.replace("line 5\n", "five\n");
    dir.write("f.txt", &changed_text);
    git(&["commit", "-q", "-am", "feat: five"]);
    let changing = git(&["rev-parse", "HEAD"]);
    dir.write("f.txt", &changed_text.replace("line 8\n", ""));
    git(&["commit", "-q", "-am", "feat: drop eight"]);
    let deleting = git(&["rev-parse", "HEAD"]);
    (base, changing, deleting)
}

/// 件名と本文が日本語のコミットを 1 つ積んだリポジトリ。(基点, そのコミット) を返す。
fn japanese_message_repo(dir: &TempDir, global: &Path) -> (String, String) {
    let git = |args: &[&str]| git_with_global(&dir.path, global, args);
    git(&["init", "-q"]);
    dir.write("a.txt", "one\n");
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    let base = git(&["rev-parse", "HEAD"]);
    dir.write("a.txt", "two\n");
    git(&[
        "commit",
        "-q",
        "-am",
        "feat: 二行目に変える",
        "-m",
        "理由の本文",
    ]);
    let changed = git(&["rev-parse", "HEAD"]);
    (base, changed)
}

// ---- R-SESSION（--resume の受理、保留、一覧） ----

use std::collections::BTreeMap;

use kemi_core::domain::review::{Approval, FileEntry, Group, ReviewMeta, Side, Status, Suggestion};
use kemi_core::session::{
    FrozenUnit, SessionCopy, SessionInfo, SessionMode, SessionState, SessionStore,
};
use kemi_core::source::FileContent;

impl Kemi {
    /// 終了を待ち、stdout と stderr の全部を返す。
    #[cfg(unix)]
    fn wait_with_stderr(mut self) -> (std::process::ExitStatus, String, String) {
        let status = process::wait(&mut self.child, std::time::Duration::from_secs(20)).unwrap();
        let mut stdout = String::new();
        self.stdout.read_to_string(&mut stdout).unwrap();
        let mut stderr = String::new();
        self.stderr.read_to_string(&mut stderr).unwrap();
        (status, stdout, stderr)
    }
}

fn sessions_dir(state: &Path) -> PathBuf {
    state.join("kemi").join("sessions")
}

fn session_files(state: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(sessions_dir(state))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "session")
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// `sessions/` に残っているセッションのファイル。セッション 1 つは `<id>.session` と
/// `<id>.payload` の 2 つなので、片方だけ残っていないことも見られるように両方を数える。
#[cfg(unix)]
fn session_dir_files(state: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(sessions_dir(state))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "session" || extension == "payload")
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

async fn wait_for_session(state: &Path) -> PathBuf {
    for _ in 0..500 {
        if let Some(path) = session_files(state).first() {
            return path.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("no session file appeared");
}

async fn wait_for_frozen_session(state: &Path) -> PathBuf {
    let store = SessionStore::new(sessions_dir(state));
    for _ in 0..500 {
        for path in session_files(state) {
            if store
                .read(&session_id(&path))
                .is_ok_and(|saved| matches!(saved.copy, kemi_core::session::CopyState::Ready(_)))
            {
                return path;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("no frozen session appeared");
}

fn session_id(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

#[cfg(unix)]
fn signal(child: &Child, signal: &str) {
    let status = Command::new("kill")
        .arg(signal)
        .arg(child.id().to_string())
        .status()
        .unwrap();
    assert!(status.success(), "kill {signal} failed");
}

/// 変更のある worktree のフィクスチャ。
fn worktree_fixture(dir: &TempDir) {
    std::fs::write(dir.path.join("a.txt"), "one\ntwo\n").unwrap();
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "e2e@example.com"]);
    git(&dir.path, &["config", "user.name", "e2e"]);
    git(&dir.path, &["config", "core.hooksPath", "/dev/null"]);
    git(&dir.path, &["add", "-A"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    std::fs::write(dir.path.join("a.txt"), "one\nTWO\n").unwrap();
}

fn session_info(id: &str, mode: SessionMode, title: &str, workspace: &Path) -> SessionInfo {
    SessionInfo {
        id: id.to_string(),
        created: 1,
        updated: 1,
        workspace: workspace.to_path_buf(),
        workspace_key: "00000000000000aa".to_string(),
        mode,
        title: title.to_string(),
        total_files: 1,
    }
}

fn session_comment() -> kemi_core::domain::review::Comment {
    kemi_core::domain::review::Comment {
        id: "c1".to_string(),
        seq: 1,
        group_id: "worktree".to_string(),
        group_title: "Working tree changes".to_string(),
        body: "note".to_string(),
        replies: Vec::new(),
        resolved: false,
        outdated: false,
        target: kemi_core::domain::review::CommentTarget::File(
            kemi_core::domain::review::FileTarget {
                file_id: "f1".to_string(),
                path: "a.txt".to_string(),
                side: Side::New,
                start_line: Some(1),
                end_line: Some(1),
                quote: vec!["one".to_string()],
                content_hash: "hash".to_string(),
                suggestion: Some(Suggestion {
                    replacement: "ONE".to_string(),
                }),
            },
        ),
    }
}

fn session_copy() -> SessionCopy {
    SessionCopy {
        startup_unit: None,
        units: vec![FrozenUnit {
            unit: None,
            review: ReviewMeta {
                title: "Working tree changes".to_string(),
                subtitle: String::new(),
                meta: serde_json::Value::Null,
                groups: vec![Group {
                    id: "worktree".to_string(),
                    title: "Working tree changes".to_string(),
                    why: String::new(),
                    watch: String::new(),
                    files: vec![FileEntry {
                        id: "f1".to_string(),
                        group_id: "worktree".to_string(),
                        path: "a.txt".to_string(),
                        old_path: None,
                        status: Status::Modify,
                        add: 1,
                        del: 1,
                        binary: false,
                        old_size: 0,
                        new_size: 0,
                        focus: false,
                        note: String::new(),
                        noise: false,
                        content_skipped: false,
                    }],
                }],
                approval: vec![Approval {
                    path: "a.txt".to_string(),
                    identity: "sha256:e2e".to_string(),
                }],
            },
        }],
        contents: BTreeMap::from([(
            "f1".to_string(),
            FileContent {
                old: Some(b"one\n".to_vec()),
                new: Some(b"ONE\n".to_vec()),
            },
        )]),
    }
}

/// 復元できるセッションを 1 つ作る。タイトルは一覧の検証に使う。
fn craft_session(state: &Path, id: &str, mode: SessionMode, title: &str) -> SessionStore {
    let store = SessionStore::new(sessions_dir(state));
    let workspace = state.join("workspace");
    let mut open = store
        .create(session_info(id, mode, title, &workspace))
        .unwrap();
    open.save_state(SessionState {
        comments: vec![session_comment()],
        seen: ["f1".to_string()].into_iter().collect(),
        collapsed: BTreeMap::new(),
        last_comment: 1,
        ..SessionState::default()
    })
    .unwrap();
    open.save_copy(session_copy()).unwrap();
    store
}

// ---- R-SESSION（復元の実行） ----

impl Kemi {
    #[cfg(unix)]
    async fn get_json(&self, path: &str) -> serde_json::Value {
        let response = reqwest::get(format!("{}{path}", self.url)).await.unwrap();
        assert_eq!(response.status(), 200, "GET {path}");
        response.json().await.unwrap()
    }
}

// ---- R-AGENT-CLI / R-AGENT-LINK（kemi wait と kemi reply） ----

impl Kemi {
    /// URL の後の stderr を `kemi: review <id>` の行まで読み、id を返す（R-AGENT-CLI）。
    fn review_id(&mut self) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stderr.read_line(&mut line).unwrap();
            assert!(read > 0, "kemi printed no review line");
            if let Some(id) = line.trim().strip_prefix("kemi: review ") {
                return id.to_string();
            }
        }
    }

    async fn review_json(&self) -> serde_json::Value {
        reqwest::get(format!("{}api/review", self.url))
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn wait_agent_status(&self, expected: &str) {
        for _ in 0..500 {
            if self.review_json().await["agent"]["status"] == expected {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("the agent status never became {expected}");
    }

    async fn add_comment(&self, line: u32, body: &str) {
        let response = self
            .post(
                "api/comment",
                serde_json::json!({
                    "op": "add", "file_id": "f1", "side": "new",
                    "start_line": line, "end_line": line, "body": body
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
    }

    async fn hand(&self) {
        let response = self.post("api/hand", serde_json::json!({})).await;
        assert_eq!(response.status(), 200);
    }

    async fn submit(&self, verdict: &str) -> serde_json::Value {
        let response = self
            .post("api/submit", serde_json::json!({ "verdict": verdict }))
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }
}

fn endpoint_dir(state: &Path) -> PathBuf {
    if cfg!(windows) {
        state.join("localappdata").join("kemi").join("sessions")
    } else {
        sessions_dir(state)
    }
}

/// `kemi wait` / `kemi reply` を終わるまで走らせる。
fn run_agent(dir: &Path, state: &Path, args: &[&str], stdin: &str) -> std::process::Output {
    let mut child = kemi_command(dir, state)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// `kemi wait` を裏で待たせる。
fn spawn_wait(dir: &Path, state: &Path, args: &[&str]) -> StopOnDrop {
    StopOnDrop(
        kemi_command(dir, state)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

/// 終わるのを待ち、(終了コード, stdout, stderr)。
async fn finish_agent(mut child: StopOnDrop) -> (Option<i32>, String, String) {
    tokio::task::spawn_blocking(move || {
        let mut stdout = process::CapturedPipe::new(child.0.stdout.take().unwrap());
        let mut stderr = process::CapturedPipe::new(child.0.stderr.take().unwrap());
        let status = process::wait(&mut child.0, std::time::Duration::from_secs(20)).unwrap();
        let mut out = String::new();
        let mut err = String::new();
        stdout.read_to_string(&mut out).unwrap();
        stderr.read_to_string(&mut err).unwrap();
        (status.code(), out, err)
    })
    .await
    .unwrap()
}

/// worktree のレビューを起動し、review の id を読む。
fn start_worktree_review(dir: &TempDir, state: &TempDir, extra: &[&str]) -> (Kemi, String) {
    worktree_fixture(dir);
    let mut args = vec!["--worktree", "--no-open"];
    args.extend_from_slice(extra);
    let mut kemi = Kemi::spawn_with_state(&dir.path, &args, &state.path);
    let id = kemi.review_id();
    (kemi, id)
}

// ---- --live（live.md の R-PAGE-MODE と R-PAGE-SESSION） ----

/// 開発サーバの居ないループバックの URL。つながらなくても起動して待つ。
const LIVE_URL: &str = "http://127.0.0.1:9/app";

impl Kemi {
    /// コードの見方の最初のファイルの id。`--live` の id はパスから決まる。
    async fn first_file_id(&self) -> String {
        self.review_json().await["groups"][0]["files"][0]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// コードの見方の最初のファイルの 1 行目に、コメントを付ける。
    async fn add_live_comment(&self, body: &str) {
        let file_id = self.first_file_id().await;
        let response = self
            .post(
                "api/comment",
                serde_json::json!({
                    "op": "add", "file_id": file_id, "side": "new",
                    "start_line": 1, "end_line": 1, "body": body
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
    }
}

/// 開発サーバの代わり。どの要求にも同じ HTML を返し、1 回ごとに接続を閉じる。
async fn start_static_dev_server(body: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (port, task)
}

impl Kemi {
    /// `kemi: live <url>` の行を待つ。
    fn live_url(&mut self) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stderr.read_line(&mut line).unwrap();
            assert!(read > 0, "kemi printed no live line");
            if let Some(url) = line.trim().strip_prefix("kemi: live ") {
                return url.to_string();
            }
        }
    }
}

/// レビューの URL を開いて中継用の cookie を受け取り、中継の URL を読む。
async fn read_live_page(kemi: &Kemi, live: &str) -> (u16, String) {
    let client = reqwest::Client::new();
    let opened = client.get(&kemi.url).send().await.unwrap();
    let cookie = opened.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let response = client
        .get(live)
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.text().await.unwrap())
}

// ---- ページへのコメント（live.md の R-PAGE-COMMENT、R-PAGE-SESSION、output.md の R-SUBMIT） ----

/// 画像の中身。base64 にしやすいよう 0 のバイトを並べ、本文の既定の上限（2 MB）を超える大きさにする。
const PAGE_IMAGE_BYTES: usize = 3 * 1024 * 1024;

fn page_image() -> (Vec<u8>, String) {
    // 0 のバイト 3 つは base64 で "AAAA"。
    (
        vec![0u8; PAGE_IMAGE_BYTES],
        "AAAA".repeat(PAGE_IMAGE_BYTES / 3),
    )
}

fn page_places() -> serde_json::Value {
    let element = serde_json::json!({ "selector": "#buy", "text": "Buy", "rect": { "x": 0, "y": 120, "w": 200, "h": 60 } });
    serde_json::json!([
        { "n": 1, "kind": "element", "points": [], "elements": [element] },
        { "n": 2, "kind": "arrow", "points": [{ "x": 300, "y": 40 }, { "x": 100, "y": 150 }], "elements": [element] },
        { "n": 3, "kind": "pen", "points": [{ "x": 0, "y": 100 }, { "x": 220, "y": 100 }, { "x": 220, "y": 200 }], "elements": [element] },
    ])
}

impl Kemi {
    /// 3 つの場所と画像を持つページへのコメントを足し、その JSON を返す。
    async fn add_page_comment(&self, body: &str) -> serde_json::Value {
        let response = self
            .post(
                "api/comment",
                serde_json::json!({
                    "op": "add_page",
                    "page": { "url": "/products?x=1", "width": 390, "places": page_places() },
                    "body": body,
                    "image": page_image().1,
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }
}

/// `kemi wait` が返した最初の渡したコメント。
fn handed_comment(output: &std::process::Output) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    answer["events"][0]["comments"][0]["comment"].clone()
}

fn start_live_review(dir: &TempDir, state: &TempDir) -> (Kemi, String) {
    worktree_fixture(dir);
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    let id = kemi.review_id();
    (kemi, id)
}

impl Kemi {
    /// 開始時のスナップショットを API で取る（ページ用のスクリプトの代わり）。
    async fn take_start_snapshot(&self) -> serde_json::Value {
        let response = self
            .post(
                "api/snapshot",
                serde_json::json!({
                    "page": "/", "width": 390, "kind": "start",
                    "html": "<p>at the start</p>", "description": "H4sIdescribed",
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }
}

/// `api/events` の SSE を読み、`update` を数える。
struct Updates {
    response: reqwest::Response,
    buffer: String,
}

impl Updates {
    async fn open(kemi: &Kemi) -> Self {
        let response = reqwest::get(format!("{}api/events", kemi.url))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        Updates {
            response,
            buffer: String::new(),
        }
    }

    /// `within` のあいだに届いた `update` の数。
    async fn count(&mut self, within: std::time::Duration) -> usize {
        let deadline = tokio::time::Instant::now() + within;
        let mut count = 0;
        loop {
            count += self.drain();
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return count;
            }
            match tokio::time::timeout(remaining, self.response.chunk()).await {
                Ok(Ok(Some(chunk))) => self.buffer.push_str(&String::from_utf8_lossy(&chunk)),
                Ok(Ok(None) | Err(_)) | Err(_) => return count,
            }
        }
    }

    fn drain(&mut self) -> usize {
        let needle = "event: update";
        let mut count = 0;
        while let Some(index) = self.buffer.find(needle) {
            count += 1;
            self.buffer.drain(..index + needle.len());
        }
        count
    }

    /// `update` が届くまで `change` を繰り返す。見張りの登録は起動の応答を待たせず裏で
    /// 進むので、登録の前の書き込みは取りこぼしうる。届いたら、続く通知が静まるまで待つ。
    async fn after_repeating(&mut self, mut change: impl FnMut(usize)) -> bool {
        for attempt in 0..30 {
            change(attempt);
            if self.count(std::time::Duration::from_millis(1000)).await > 0 {
                while self.count(std::time::Duration::from_millis(800)).await > 0 {}
                return true;
            }
        }
        false
    }
}

/// `a.txt` だけに差分があり、`b.txt` は追跡しているが変わっていない作業ツリー。
fn worktree_with_an_unchanged_file(dir: &TempDir) {
    git_repo(dir);
    dir.write(".gitignore", "build/\n*.log\n");
    dir.write("a.txt", "a1\n");
    dir.write("b.txt", "b1\n");
    git(&dir.path, &["add", ".gitignore", "a.txt", "b.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    dir.write("a.txt", "a2\n");
    dir.write("build/out.o", "o\n");
}

async fn review_paths(kemi: &Kemi) -> Vec<String> {
    let review = reqwest::get(format!("{}api/review?refresh=1", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    review["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["files"].as_array().unwrap().clone())
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect()
}

/// 差分のファイルを変えて `update` が届くのを確かめてから submit し、レビューの間に stderr に
/// 出た行をすべて返す。
#[cfg(target_os = "linux")]
async fn stderr_of_a_review_that_sees_an_update(dir: &TempDir) -> Vec<String> {
    let mut kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;

    let updated = updates
        .after_repeating(|attempt| dir.write("a.txt", &format!("a3 {attempt}\n")))
        .await;
    assert!(updated, "changing a file in the diff sent no update");

    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    drop(updates);
    let mut lines = kemi.preamble.clone();
    for line in kemi.stderr.by_ref().lines() {
        lines.push(line.unwrap());
    }
    let status = wait_for_exit(&mut kemi, std::time::Duration::from_secs(10)).await;
    assert_eq!(status.code(), Some(0));
    lines
}

#[path = "e2e/conversation.rs"]
mod conversation_cases;

#[path = "e2e/digest.rs"]
mod digest_cases;

#[path = "e2e/explanation.rs"]
mod explanation_cases;

#[path = "e2e/live.rs"]
mod live_cases;

#[path = "e2e/process.rs"]
mod process_cases;

#[path = "e2e/repository.rs"]
mod repository_cases;

#[path = "e2e/results.rs"]
mod results_cases;

#[path = "e2e/sessions.rs"]
mod sessions_cases;

#[path = "e2e/startup.rs"]
mod startup_cases;

#[path = "e2e/watch.rs"]
mod watch_cases;
