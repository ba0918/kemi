//! バイナリを起動する e2e（CLI、exit code、stdout の JSON、live stderr）。

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
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
/// よう、`XDG_STATE_HOME` と `HOME` を必ず `state` の下に向ける。
fn kemi_command(dir: &Path, state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kemi"));
    command
        .current_dir(dir)
        .env("XDG_STATE_HOME", state)
        .env("HOME", state.join("home"));
    command
}

struct Kemi {
    child: Child,
    stdout: ChildStdout,
    stderr: BufReader<ChildStderr>,
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
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let mut preamble = Vec::new();
        let url = loop {
            line.clear();
            let read = reader.read_line(&mut line).unwrap();
            assert!(read > 0, "kemi が URL を出さずに終了しました");
            if let Some(found) = line.strip_prefix("kemi: http") {
                break format!("http{found}").trim().to_string();
            }
            preamble.push(line.clone());
        };
        Kemi {
            child,
            stdout,
            stderr: reader,
            url,
            preamble,
            _state: None,
        }
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
        let status = self.child.wait().unwrap();
        let mut stdout = String::new();
        self.stdout.read_to_string(&mut stdout).unwrap();
        (status, stdout)
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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

#[test]
fn cli_no_mode_exits_2_with_usage() {
    let dir = TempDir::new();
    let output = run(&dir.path, &[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !contains_japanese(&stderr),
        "usage must be in English: {stderr}"
    );
}

#[test]
fn cli_mode_exclusive_exits_2() {
    let dir = TempDir::new();
    let output = run(&dir.path, &["--worktree", "--staged"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn cli_out_rejected_with_reason() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let output = run(&dir.path, &["manifest.json", "--out", "x.html"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !contains_japanese(&stderr),
        "the --out rejection must be in English: {stderr}"
    );
}

#[test]
fn cli_missing_manifest_error_has_no_japanese() {
    let dir = TempDir::new();
    let output = run(&dir.path, &["no-such-manifest.json"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !contains_japanese(&stderr),
        "the manifest error must be in English: {stderr}"
    );
}

#[test]
fn cli_runtime_error_exits_2_without_json() {
    let dir = TempDir::new();
    let output = run(&dir.path, &["--worktree"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[tokio::test]
async fn cli_base_resolves_manifest_paths() {
    let dir = TempDir::new();
    dir.write("data/old.txt", "old\n");
    dir.write("data/new.txt", "new\n");
    dir.write(
        "data/manifest.json",
        r#"{"groups":[{"diffs":[{"path":"a.txt","old_path":"old.txt","new_path":"new.txt"}]}]}"#,
    );
    let kemi = Kemi::spawn(
        &dir.path,
        &[
            "data/manifest.json",
            "--base",
            "data",
            "--no-open",
            "--port",
            "0",
        ],
    );

    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(review["groups"][0]["files"][0]["path"], "a.txt");
    assert_eq!(review["groups"][0]["files"][0]["add"], 1);
    assert_eq!(review["groups"][0]["files"][0]["del"], 1);

    kemi.kill();
}

#[tokio::test]
async fn exit_code_approved_is_0_and_stdout_json() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);

    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(review["title"], "e2e のレビュー");

    // 起動時の stderr は、URL の行（start が読む）の後に結果ファイルの保存先の行
    // （R-RESULT）が続く。ここで読み、後から出るコメントのライブ表示と区別する。
    let mut save_line = String::new();
    kemi.stderr.read_line(&mut save_line).unwrap();
    assert!(
        save_line.starts_with("kemi: ") && !save_line.contains("http://"),
        "unexpected stderr after the URL: {save_line:?}"
    );

    let _ = kemi
        .post(
            "api/comment",
            serde_json::json!({
                "op": "add", "file_id": "f1", "side": "new",
                "start_line": 2, "end_line": 2, "body": "ここ直して"
            }),
        )
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);

    // コメントの追加で、人間向けのライブ表示の行が stderr に出る（R-COMMENT）。
    // 文言は契約でないので、保存先の行の後に `kemi:` の行が追加で出ることを確かめる。
    // stdout には submit の JSON だけ。
    let mut stderr_lines = Vec::new();
    for line in kemi.stderr.by_ref().lines() {
        stderr_lines.push(line.unwrap());
    }
    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(document["verdict"], "approved");
    assert_eq!(document["comments"][0]["body"], "ここ直して");
    assert!(
        stderr_lines.iter().any(|line| line.starts_with("kemi: ")),
        "the comment live display is missing from stderr: {stderr_lines:?}"
    );
    let stderr = format!("{save_line}{}", stderr_lines.join("\n"));
    assert!(
        !contains_japanese(&stderr),
        "live stderr must be in English: {stderr_lines:?}"
    );
}

#[tokio::test]
async fn submit_with_events_open_exits_with_json() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);

    // ブラウザと同じく SSE を開いたまま submit しても、サーバは接続を閉じて終了する。
    let events = reqwest::Client::new()
        .get(format!("{}api/events", kemi.url))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);

    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);

    let status = wait_for_exit(&mut kemi, std::time::Duration::from_secs(10)).await;
    drop(events);

    let mut stdout = String::new();
    kemi.stdout.read_to_string(&mut stdout).unwrap();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(document["verdict"], "approved");
}

#[tokio::test]
async fn runtime_io_error_stops_with_exit_2_without_json() {
    let dir = TempDir::new();
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "kemi@example.com"]);
    git(&dir.path, &["config", "user.name", "kemi"]);
    dir.write("a.txt", "one\ntwo\n");
    git(&dir.path, &["add", "a.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    dir.write("a.txt", "one\nTWO\n");

    let mut kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let file_id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    std::fs::remove_file(dir.path.join("a.txt")).unwrap();

    let response = reqwest::get(format!("{}api/file/{file_id}", kemi.url))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);

    let status = wait_for_exit(&mut kemi, std::time::Duration::from_secs(10)).await;
    let mut stdout = String::new();
    kemi.stdout.read_to_string(&mut stdout).unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(stdout.is_empty(), "stdout: {stdout}");
}

#[tokio::test]
async fn refresh_keeps_comment_on_the_same_file() {
    let dir = TempDir::new();
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "kemi@example.com"]);
    git(&dir.path, &["config", "user.name", "kemi"]);
    dir.write("a.txt", "a1\n");
    dir.write("b.txt", "b1\n");
    git(&dir.path, &["add", "a.txt", "b.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    dir.write("a.txt", "a2\n");
    dir.write("b.txt", "b2\n");

    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let file_id = |review: &serde_json::Value, path: &str| {
        review["groups"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let b_id = file_id(&review, "b.txt");
    let a_id = file_id(&review, "a.txt");

    for (id, body) in [(&b_id, "b へのコメント"), (&a_id, "a へのコメント")] {
        let response = kemi
            .post(
                "api/comment",
                serde_json::json!({
                    "op": "add", "file_id": id, "side": "new",
                    "start_line": 1, "end_line": 1, "body": body
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
    }

    // a を戻して再取得しても、残る b の id とコメントの対応は変わらない。
    dir.write("a.txt", "a1\n");
    let refreshed = reqwest::get(format!("{}api/review?refresh=1", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(file_id(&refreshed, "b.txt"), b_id);

    let file = reqwest::get(format!("{}api/file/{b_id}", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(file["comments"][0]["body"], "b へのコメント");

    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let comments = document["comments"].as_array().unwrap();
    let b_comment = comments
        .iter()
        .find(|comment| comment["path"] == "b.txt")
        .unwrap();
    assert_eq!(b_comment["outdated"], false);
    // 一覧から消えたファイルへのコメントは古い扱いで残る。
    let a_comment = comments
        .iter()
        .find(|comment| comment["path"] == "a.txt")
        .unwrap();
    assert_eq!(a_comment["outdated"], true);
}

#[tokio::test]
async fn exit_code_changes_requested_is_1() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);

    let response = kemi
        .post(
            "api/submit",
            serde_json::json!({"verdict": "changes_requested"}),
        )
        .await;
    assert_eq!(response.status(), 200);

    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(1));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(document["verdict"], "changes_requested");
    assert_eq!(document["approval"][0]["identity"], "sha256:e2e");
}

#[tokio::test]
async fn stdin_dash_reads_manifest() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let mut child = kemi_command(&dir.path, &state.path)
        .args(["-", "--no-open", "--port", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(MANIFEST.as_bytes())
        .unwrap();

    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    let url = loop {
        line.clear();
        let read = stderr.read_line(&mut line).unwrap();
        assert!(read > 0, "kemi が URL を出さずに終了しました");
        if let Some(found) = line.strip_prefix("kemi: http") {
            break format!("http{found}").trim().to_string();
        }
    };
    let review = reqwest::get(format!("{url}api/review"))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(review["title"], "e2e のレビュー");

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_exits_130_without_json() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);
    kemi.wait_serving().await;
    let pid = kemi.child.id().to_string();

    let status = Command::new("kill").args(["-INT", &pid]).status().unwrap();
    assert!(status.success());

    let (exit, stdout) = kemi.wait();
    assert_eq!(exit.code(), Some(130));
    assert!(stdout.is_empty());
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

#[test]
fn digest_mode_schema_and_bounded_output() {
    let dir = TempDir::new();
    dir.write("manifest.json", &digest_manifest());
    let output = run(&dir.path, &["manifest.json", "--digest"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("kemi: http"));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let digest: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    assert_eq!(digest["kemi"], 1);
    assert_eq!(digest["title"], "digest のテスト");
    assert_eq!(digest["totals"]["files"], 4);
    assert_eq!(digest["totals"]["noise_files"], 1);
    assert_eq!(digest["directories"][0]["path"], ".");
    assert!(digest["directories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|directory| directory["path"] == "src"));
    assert_eq!(digest["top_n"], 100);
    assert_eq!(digest["top_files_omitted"]["files"], 0);
    assert_eq!(digest["top_files_omitted"]["add"], 0);
    assert_eq!(digest["top_files_omitted"]["del"], 0);
    assert!(!stdout.contains("\"quote\""));
    assert!(stdout.len() < 100_000, "digest was {} bytes", stdout.len());
}

#[test]
fn digest_mode_top_n_limits_and_sorts() {
    let dir = TempDir::new();
    dir.write("manifest.json", &digest_manifest());
    let output = run(
        &dir.path,
        &["manifest.json", "--digest", "--digest-top", "2"],
    );

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let digest: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let top = digest["top_files"].as_array().unwrap();

    assert_eq!(digest["top_n"], 2);
    assert_eq!(top.len(), 2);
    let omitted = digest["top_files_omitted"]["files"].as_u64().unwrap();
    assert_eq!(omitted, 2);
    assert_eq!(
        top.len() as u64 + omitted,
        digest["totals"]["files"].as_u64().unwrap()
    );
    for pair in top.windows(2) {
        let left = pair[0]["add"].as_u64().unwrap() + pair[0]["del"].as_u64().unwrap();
        let right = pair[1]["add"].as_u64().unwrap() + pair[1]["del"].as_u64().unwrap();
        assert!(
            left > right || (left == right && pair[0]["path"].as_str() <= pair[1]["path"].as_str()),
            "top_files not sorted: {top:?}"
        );
    }
}

#[test]
fn digest_mode_bounded_thirty_thousand_files() {
    let mut groups = Vec::new();
    for group_index in 0..100 {
        let mut diffs = Vec::new();
        for file_index in 0..300 {
            let new_text = "line\n".repeat((file_index % 5) + 1);
            diffs.push(serde_json::json!({
                "path": format!("src/group{group_index}/file{file_index}.rs"),
                "old": "line\n",
                "new": new_text,
            }));
        }
        groups.push(serde_json::json!({
            "id": format!("g{group_index}"),
            "title": format!("コミット {group_index}"),
            "why": "理由".repeat(30),
            "watch": "確認".repeat(30),
            "diffs": diffs,
        }));
    }
    let manifest = serde_json::json!({
        "title": "3 万ファイル",
        "groups": groups,
        "approval": []
    })
    .to_string();

    let dir = TempDir::new();
    dir.write("manifest.json", &manifest);
    let output = run(&dir.path, &["manifest.json", "--digest"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let digest: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    assert_eq!(digest["totals"]["files"], 30_000);
    assert_eq!(digest["top_files"].as_array().unwrap().len(), 100);
    assert_eq!(digest["top_files_omitted"]["files"], 29_900);
    assert_eq!(
        digest["top_files"].as_array().unwrap().len() as u64
            + digest["top_files_omitted"]["files"].as_u64().unwrap(),
        digest["totals"]["files"].as_u64().unwrap()
    );
    assert!(stdout.len() < 100_000, "digest was {} bytes", stdout.len());
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

#[tokio::test]
async fn cli_default_group_by_file_starts_in_final_form() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--from", &base, "--no-open", "--port", "0"]);

    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(review["unit"], "file");
    assert_eq!(review["groups"].as_array().unwrap().len(), 1);
    assert_eq!(review["groups"][0]["id"], "all");
    kemi.kill();
}

#[test]
fn digest_default_final_form_has_one_group() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);

    let output = run(&dir.path, &["--from", &base, "--digest"]);

    assert_eq!(output.status.code(), Some(0));
    let digest: serde_json::Value =
        serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    let groups = digest["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["id"], "all");
    assert_eq!(digest["totals"]["files"], 2);
}

#[test]
fn cli_result_usage_errors_exit_2() {
    let dir = TempDir::new();
    two_commit_repo(&dir);
    dir.write("manifest.json", MANIFEST);

    for args in [
        &["--result", "--from", "HEAD~1"][..],
        &["--result", "--port", "0"],
        &["--result", "--worktree"],
        &["--result", "manifest.json"],
        &["--result", "--digest"],
        &["--any"],
        &["--workspace", "."],
        &["--result", "--any", "--workspace", "."],
        &["--worktree", "--group-by", "commit"],
    ] {
        let output = run(&dir.path, args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
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

#[cfg(unix)]
#[tokio::test]
async fn result_file_matches_stdout_with_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let state = TempDir::new();

    let (answer, stdout, code) = submit_manifest(&dir.path, &state.path, "approved", "保存").await;

    assert_eq!(code, Some(0));
    let files = result_files(&state.path);
    assert_eq!(files.len(), 1);
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), stdout);
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&files[0]), 0o600);
    assert_eq!(mode(&results_dir(&state.path)), 0o700);
    assert_eq!(answer["saved"]["path"], files[0].to_string_lossy().as_ref());
    assert_eq!(answer["saved"]["error"], serde_json::Value::Null);
    assert_eq!(answer["result"]["verdict"], "approved");
}

#[tokio::test]
async fn result_flag_returns_the_same_json_and_the_original_exit_code() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (_, stdout, code) =
        submit_manifest(&dir.path, &state.path, "changes_requested", "変更要求").await;
    assert_eq!(code, Some(1));

    let output = run_with_state(&dir.path, &["--result"], &state.path);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), stdout);
}

#[test]
fn result_flag_without_any_result_exits_2_and_prints_nothing() {
    let dir = TempDir::new();
    let state = TempDir::new();

    let output = run_with_state(&dir.path, &["--result"], &state.path);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn result_flag_with_a_broken_latest_file_exits_2_and_prints_nothing() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (_, stdout, _) = submit_manifest(&dir.path, &state.path, "approved", "途中で切れる").await;
    let files = result_files(&state.path);
    assert_eq!(files.len(), 1);
    // 書き込みの途中で止まった結果ファイルに相当する、途中で切れた JSON。
    std::fs::write(&files[0], &stdout.as_bytes()[..stdout.len() / 2]).unwrap();

    let output = run_with_state(&dir.path, &["--result"], &state.path);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn result_keeps_twenty_across_repositories() {
    // Why not 最初の 1 件が消えたことまで見ない: どれを消すかは送信時刻で決まり、動かして
    // いる機械の時計が戻ると submit の順と食い違う。送信時刻の古い方から消すことは、
    // 送信時刻を与える src/result.rs のテストで確かめる。
    let first_repo = TempDir::new();
    let second_repo = TempDir::new();
    let state = TempDir::new();
    for index in 1..=21 {
        let dir = if index % 2 == 0 {
            &second_repo.path
        } else {
            &first_repo.path
        };
        submit_manifest(dir, &state.path, "approved", &format!("{index} 件目")).await;
    }

    assert_eq!(result_files(&state.path).len(), 20);
}

#[tokio::test]
async fn result_of_another_repository_is_returned_only_with_any_or_workspace() {
    let first = TempDir::new();
    let second = TempDir::new();
    git_repo(&first);
    git_repo(&second);
    let state = TempDir::new();
    let (_, stdout, _) = submit_manifest(&first.path, &state.path, "approved", "A").await;

    let other = run_with_state(&second.path, &["--result"], &state.path);
    assert_eq!(other.status.code(), Some(2));
    assert!(other.stdout.is_empty());

    let any = run_with_state(&second.path, &["--result", "--any"], &state.path);
    assert_eq!(any.status.code(), Some(0));
    assert_eq!(String::from_utf8(any.stdout).unwrap(), stdout);

    let first_path = first.path.to_string_lossy().into_owned();
    let pointed = run_with_state(
        &second.path,
        &["--result", "--workspace", &first_path],
        &state.path,
    );
    assert_eq!(pointed.status.code(), Some(0));
    assert_eq!(String::from_utf8(pointed.stdout).unwrap(), stdout);
}

#[tokio::test]
async fn result_same_repository_from_a_subdirectory() {
    let repo = TempDir::new();
    git_repo(&repo);
    std::fs::create_dir_all(repo.path.join("sub/deeper")).unwrap();
    let state = TempDir::new();
    let (_, stdout, _) = submit_manifest(&repo.path, &state.path, "approved", "root").await;

    let output = run_with_state(&repo.path.join("sub/deeper"), &["--result"], &state.path);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), stdout);
}

#[tokio::test]
async fn result_outside_git_is_identified_by_the_directory() {
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path.join("sub")).unwrap();
    let state = TempDir::new();
    submit_manifest(&dir.path, &state.path, "approved", "git の外").await;

    let same = run_with_state(&dir.path, &["--result"], &state.path);
    let sub = run_with_state(&dir.path.join("sub"), &["--result"], &state.path);

    assert_eq!(same.status.code(), Some(0));
    assert_eq!(sub.status.code(), Some(2));
}

#[cfg(not(windows))]
#[tokio::test]
async fn result_relative_xdg_state_home_falls_back_to_home() {
    let dir = TempDir::new();
    let home = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut child = Command::new(env!("CARGO_BIN_EXE_kemi"))
        .args(["manifest.json", "--no-open", "--port", "0"])
        .current_dir(&dir.path)
        .env("XDG_STATE_HOME", "relative/state")
        .env("HOME", &home.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    let url = loop {
        line.clear();
        assert!(stderr.read_line(&mut line).unwrap() > 0);
        if let Some(found) = line.strip_prefix("kemi: http") {
            break format!("http{found}").trim().to_string();
        }
    };
    let origin = url.split("/s/").next().unwrap().to_string();
    let response = reqwest::Client::new()
        .post(format!("{url}api/submit"))
        .header("Origin", origin)
        .json(&serde_json::json!({"verdict": "approved"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    child.wait().unwrap();

    assert_eq!(
        result_files(&home.path.join(".local/state")).len(),
        1,
        "the default location under HOME must be used"
    );
    assert!(!dir.path.join("relative").exists());
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

#[tokio::test]
async fn result_unwritable_location_keeps_stdout_and_exit_code() {
    let writable = TempDir::new();
    let (_, _, writable_stdout, writable_stderr) = approve_manifest_review(&writable.path).await;
    let unwritable = TempDir::new();
    // 状態ディレクトリの kemi がファイルなので、その下に結果を作れない。
    std::fs::write(unwritable.path.join("kemi"), "not a directory").unwrap();

    let (answer, status, stdout, stderr) = approve_manifest_review(&unwritable.path).await;

    assert_eq!(status.code(), Some(0));
    assert_eq!(stdout, writable_stdout);
    assert!(answer["saved"]["error"].is_string());
    // 文言は契約でないので、書ける場合より stderr の行が増えたことで警告を確かめる。
    assert!(
        stderr.len() > writable_stderr.len(),
        "a warning must go to stderr: {stderr:?} vs {writable_stderr:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn result_not_written_on_interrupt_or_runtime_error() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let kemi = Kemi::spawn_with_state(
        &dir.path,
        &["manifest.json", "--no-open", "--port", "0"],
        &state.path,
    );
    kemi.wait_serving().await;
    let pid = kemi.child.id().to_string();
    assert!(Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .unwrap()
        .success());
    let (exit, _) = kemi.wait();
    assert_eq!(exit.code(), Some(130));

    let repo = TempDir::new();
    git_repo(&repo);
    repo.write("a.txt", "one\n");
    git(&repo.path, &["add", "a.txt"]);
    git(&repo.path, &["commit", "-q", "-m", "base"]);
    repo.write("a.txt", "two\n");
    let mut failing = Kemi::spawn_with_state(
        &repo.path,
        &["--worktree", "--no-open", "--port", "0"],
        &state.path,
    );
    let review: serde_json::Value = reqwest::get(format!("{}api/review", failing.url))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    std::fs::remove_file(repo.path.join("a.txt")).unwrap();
    let _ = reqwest::get(format!("{}api/file/{id}", failing.url)).await;
    let status = wait_for_exit(&mut failing, std::time::Duration::from_secs(10)).await;
    assert_eq!(status.code(), Some(2));

    assert!(result_files(&state.path).is_empty());
}

#[tokio::test]
async fn result_location_is_printed_on_its_own_stderr_line() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn_with_state(
        &dir.path,
        &["manifest.json", "--no-open", "--port", "0"],
        &state.path,
    );
    kemi.wait_serving().await;
    let _ = kemi.child.kill();
    let _ = kemi.child.wait();
    let mut rest = String::new();
    kemi.stderr.read_to_string(&mut rest).unwrap();

    let location = results_dir(&state.path).to_string_lossy().into_owned();
    let lines: Vec<&str> = kemi
        .preamble
        .iter()
        .map(String::as_str)
        .chain(rest.lines())
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(&location) && !line.contains("http://")),
        "{lines:?}"
    );
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

#[tokio::test]
async fn origin_is_found_even_if_the_global_blame_ignore_revs_file_is_missing() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);
    let changed_a = String::from_utf8(run_git(&dir.path, &["rev-parse", "HEAD~1"])).unwrap();
    let state = TempDir::new();
    // 利用者の本物の設定の代わりに、一時ファイルを git の全体設定として読ませる。
    let global = state.path.join("gitconfig");
    std::fs::write(
        &global,
        format!(
            "[blame]\n\tignoreRevsFile = {}\n",
            git_path(&state.path.join("missing-ignore-revs"))
        ),
    )
    .unwrap();
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "a.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed_a.trim());
    kemi.kill();
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

#[tokio::test]
async fn origin_is_found_in_a_partial_clone_whose_remote_needs_the_global_config() {
    let upstream = TempDir::new();
    let clone = TempDir::new();
    let state = TempDir::new();
    // 取り寄せ先の URL は、全体の設定の url.<base>.insteadOf を通したときだけ upstream に届く。
    let unreachable = file_url(&state.path.join("nowhere"));
    let global = state.path.join("gitconfig");
    std::fs::write(
        &global,
        format!(
            "[user]\n\tname = kemi\n\temail = kemi@example.com\n\
             [core]\n\thooksPath = /dev/null\n\
             [url \"{}\"]\n\tinsteadOf = {unreachable}\n",
            file_url(&upstream.path)
        ),
    )
    .unwrap();
    let git_up = |args: &[&str]| git_with_global(&upstream.path, &global, args);
    git_up(&["init", "-q"]);
    git_up(&["config", "uploadpack.allowFilter", "true"]);
    upstream.write("a.txt", "one\n");
    git_up(&["add", "-A"]);
    git_up(&["commit", "-q", "-m", "base"]);
    let base = git_up(&["rev-parse", "HEAD"]);
    // 途中の版の blob は、差分には要らず blame だけが取り寄せる。
    upstream.write("a.txt", "two\n");
    git_up(&["commit", "-q", "-am", "feat: two"]);
    upstream.write("a.txt", "three\n");
    git_up(&["commit", "-q", "-am", "feat: three"]);
    let changed = git_up(&["rev-parse", "HEAD"]);
    git_with_global(
        &clone.path,
        &global,
        &["clone", "-q", "--filter=blob:none", &unreachable, "."],
    );
    let missing = git_with_global(
        &clone.path,
        &global,
        &["rev-list", "--objects", "--missing=print", "--all"],
    );
    assert!(
        missing.lines().any(|line| line.starts_with('?')),
        "the clone must lack some blobs"
    );
    let mut kemi = start_with_global(
        &clone.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );
    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = reqwest::get(format!("{}api/origin/{id}", kemi.url))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed);
    assert!(kemi.child.try_wait().unwrap().is_none());
    kemi.kill();
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

#[tokio::test]
async fn origin_names_each_block_even_if_a_textconv_driver_rewrites_the_file() {
    let dir = TempDir::new();
    let state = TempDir::new();
    // 先頭の 2 行を落とす変換。変換後の行で数えると、行番号が 2 行ずれる。
    let global = global_config(&state, "[diff \"cut\"]\n\ttextconv = tail -n +3\n");
    let (base, changing, deleting) = textconv_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "f.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changing);
    assert_eq!(origin["blocks"][1]["entries"][0]["sha"], deleting);
    kemi.kill();
}

#[tokio::test]
async fn origin_is_found_even_if_the_textconv_driver_fails() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[diff \"cut\"]\n\ttextconv = false\n");
    let (base, changing, deleting) = textconv_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "f.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changing);
    assert_eq!(origin["blocks"][1]["entries"][0]["sha"], deleting);
    kemi.kill();
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

#[tokio::test]
async fn commit_unit_reads_utf8_messages_even_if_log_output_encoding_is_legacy() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[i18n]\n\tlogOutputEncoding = Shift_JIS\n");
    let (base, changed) = japanese_message_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &[
            "--from",
            &base,
            "--group-by",
            "commit",
            "--no-open",
            "--port",
            "0",
        ],
    );

    let review = reqwest::get(format!("{}api/review", kemi.url))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(review["groups"][0]["id"], changed);
    assert_eq!(review["groups"][0]["title"], "feat: 二行目に変える");
    assert_eq!(review["groups"][0]["why"], "理由の本文");
    kemi.kill();
}

#[tokio::test]
async fn origin_reads_utf8_messages_even_if_log_output_encoding_is_legacy() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[i18n]\n\tlogOutputEncoding = Shift_JIS\n");
    let (base, changed) = japanese_message_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "a.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed);
    assert_eq!(
        origin["commits"][changed.as_str()]["subject"],
        "feat: 二行目に変える"
    );
    kemi.kill();
}
