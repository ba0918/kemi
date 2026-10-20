use super::*;

#[test]
fn digest_accepts_a_valid_bind_without_serving() {
    let dir = TempDir::new();
    dir.write("manifest.json", &digest_manifest());
    let output = run(
        &dir.path,
        &[
            "manifest.json",
            "--digest",
            "--bind",
            "127.0.0.1",
            "--no-open",
            "--port",
            "0",
        ],
    );

    assert_eq!(output.status.code(), Some(0));
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
    assert!(
        digest["directories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|directory| directory["path"] == "src")
    );
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
