use super::*;

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

#[tokio::test]
async fn bind_wildcard_warns_after_the_url_and_serves() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn(
        &dir.path,
        &[
            "manifest.json",
            "--bind",
            "0.0.0.0",
            "--port",
            "0",
            "--no-open",
        ],
    );

    let response = kemi.get("api/review").await;
    assert_eq!(response.status(), 200);

    let mut warning = String::new();
    kemi.stderr.read_line(&mut warning).unwrap();
    assert!(
        warning.starts_with("kemi: exposed on the LAN;"),
        "the URL line must be followed by the LAN warning: {warning:?}"
    );
    assert!(
        !contains_japanese(&warning),
        "the warning must be in English: {warning:?}"
    );
    kemi.kill();
}

#[tokio::test]
async fn default_bind_keeps_the_loopback_url_without_a_warning() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);

    assert!(
        kemi.url.starts_with("http://127.0.0.1:"),
        "the default URL must stay on loopback: {}",
        kemi.url
    );
    let mut after_url = String::new();
    kemi.stderr.read_line(&mut after_url).unwrap();
    assert!(
        !after_url.starts_with("kemi: exposed on the LAN;"),
        "the default bind must not warn: {after_url:?}"
    );
    kemi.kill();
}

#[test]
fn bind_rejects_hostname_ipv6_and_result_or_digest_combinations() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    // 入力モードは有効にして、終了コード 2 が --bind の扱いだけに由来するようにする。
    let cases: &[&[&str]] = &[
        &[
            "manifest.json",
            "--bind",
            "localhost",
            "--no-open",
            "--port",
            "0",
        ],
        &["manifest.json", "--bind", "::1", "--no-open", "--port", "0"],
        &["--result", "--bind", "0.0.0.0"],
        &["manifest.json", "--digest", "--bind", "localhost"],
    ];

    for args in cases {
        let output = run(&dir.path, args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !contains_japanese(&stderr),
            "{args:?} must be rejected in English: {stderr}"
        );
    }
}

/// `--serve` は互換のために受け付けるだけで、付けても今どおりサーブする（R-INPUT-6）。
#[test]
fn serve_flag_is_accepted_for_compatibility_and_the_review_starts() {
    let dir = TempDir::new();
    worktree_fixture(&dir);

    let kemi = Kemi::spawn(
        &dir.path,
        &["--worktree", "--serve", "--no-open", "--port", "0"],
    );

    assert!(kemi.url.starts_with("http://127.0.0.1:"), "{}", kemi.url);
    kemi.kill();
}

#[test]
fn cli_out_rejected_with_reason() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let output = run(
        &dir.path,
        &[
            "manifest.json",
            "--out",
            "x.html",
            "--no-open",
            "--port",
            "0",
        ],
    );

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

    let review = kemi.review_json().await;
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

    let review = kemi.review_json().await;
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
    assert_eq!(document["kemi"], 3);
    assert_eq!(document["verdict"], "approved");
    assert_eq!(document["comments"][0]["body"], "ここ直して");
    assert_eq!(document["comments"][0]["page"], serde_json::Value::Null);
    assert_eq!(document["messages"], serde_json::json!([]));
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
async fn runtime_io_error_stops_with_exit_2_without_json_or_a_result_file() {
    let dir = TempDir::new();
    let state = TempDir::new();
    git_repo(&dir);
    dir.write("a.txt", "one\ntwo\n");
    git(&dir.path, &["add", "a.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    dir.write("a.txt", "one\nTWO\n");

    let kemi = Kemi::spawn_with_state(
        &dir.path,
        &["--worktree", "--no-open", "--port", "0"],
        &state.path,
    );
    let review = kemi.review_json().await;
    let file_id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    std::fs::remove_file(dir.path.join("a.txt")).unwrap();

    let response = kemi.get(&format!("api/file/{file_id}")).await;
    assert_eq!(response.status(), 500);

    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(2));
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(result_files(&state.path).is_empty());
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
    let kemi = Kemi::start_with_input(
        kemi_command(&dir.path, &state.path).args(["-", "--no-open", "--port", "0"]),
        Some(MANIFEST),
    );

    let review = kemi.review_json().await;
    assert_eq!(review["title"], "e2e のレビュー");
}

// ---- stdout に書けないとき（R-SUBMIT） ----

/// 読み手を先に閉じたパイプの書き手。ここへの書き込みは誤りになる。
fn closed_pipe() -> std::io::PipeWriter {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    writer
}

fn assert_cannot_write_to_stdout(code: Option<i32>, stderr: &str, what: &str) {
    assert_eq!(code, Some(2), "{what}: {stderr}");
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("kemi: cannot write to stdout: ")),
        "{what}: {stderr}"
    );
}

#[test]
fn version_result_digest_and_session_list_exit_2_when_stdout_is_closed() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    kemi_core::result::save(
        &results_dir(&state.path),
        "00000000000000aa",
        r#"{"kemi":3,"verdict":"approved"}"#,
        1,
        1,
    )
    .unwrap();
    craft_session(
        &state.path,
        "01HF7YAT00AAAAAAAAAAAAAAAA",
        SessionMode::Worktree,
        "Working tree changes",
    );

    for args in [
        vec!["--version"],
        vec!["--result", "--any"],
        vec!["manifest.json", "--digest"],
        vec!["--resume"],
    ] {
        // stdout を閉じたパイプにするので、stdout を受け取る run_command は使えない。
        let mut child = StopOnDrop(
            kemi_command(&dir.path, &state.path)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(closed_pipe())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut stderr = process::CapturedPipe::new(child.0.stderr.take().unwrap());
        let status = process::wait(&mut child.0, std::time::Duration::from_secs(20))
            .unwrap_or_else(|error| panic!("kemi {args:?}: {error}"));
        let mut text = String::new();
        stderr.read_to_string(&mut text).unwrap();

        assert_cannot_write_to_stdout(status.code(), &text, &format!("kemi {args:?}"));
    }
}

/// submit の JSON を stdout に書けなくても、結果ファイルには残る（R-RESULT）。
#[tokio::test]
async fn submit_with_a_closed_stdout_exits_2_and_keeps_the_result_file() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let mut child = StopOnDrop(
        kemi_command(&dir.path, &state.path)
            .args(["manifest.json", "--no-open", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(closed_pipe())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stderr = process::CapturedPipe::new(child.0.stderr.take().unwrap());
    let mut line = String::new();
    let url = loop {
        line.clear();
        assert!(
            stderr.read_line(&mut line).unwrap() > 0,
            "kemi exited without a URL"
        );
        if let Some(found) = line.strip_prefix("kemi: http") {
            break format!("http{}", found.trim());
        }
    };

    let response = http_client()
        .post(format!("{url}api/submit"))
        .header("Origin", url.split("/s/").next().unwrap())
        .json(&serde_json::json!({ "verdict": "approved" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let status = process::wait(&mut child.0, std::time::Duration::from_secs(20)).unwrap();
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();

    assert_cannot_write_to_stdout(status.code(), &rest, "submit");
    assert_eq!(result_files(&state.path).len(), 1);
}
