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

struct Kemi {
    child: Child,
    stdout: ChildStdout,
    stderr: BufReader<ChildStderr>,
    url: String,
}

impl Kemi {
    fn spawn(dir: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kemi"))
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let url = loop {
            line.clear();
            let read = reader.read_line(&mut line).unwrap();
            assert!(read > 0, "kemi が URL を出さずに終了しました");
            if let Some(found) = line.strip_prefix("kemi: http") {
                break format!("http{found}").trim().to_string();
            }
        };
        Kemi {
            child,
            stdout,
            stderr: reader,
            url,
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
    Command::new(env!("CARGO_BIN_EXE_kemi"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn cli_no_mode_exits_2_with_usage() {
    let dir = TempDir::new();
    let output = run(&dir.path, &[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("使い方"));
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
    assert!(stderr.contains("静的書き出し"), "{stderr}");
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

    // live 表示は stderr に出る。stdout には submit の JSON だけ。
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
        stderr_lines.iter().any(|line| line.contains("コメント")),
        "live stderr missing: {stderr_lines:?}"
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_kemi"))
        .args(["-", "--no-open", "--port", "0"])
        .current_dir(&dir.path)
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
    assert!(stdout.len() < 100_000, "digest was {} bytes", stdout.len());
}
