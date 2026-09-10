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
