use super::*;

#[tokio::test]
async fn submit_with_events_open_exits_with_json() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let kemi = Kemi::spawn(&dir.path, &["manifest.json", "--no-open", "--port", "0"]);

    // ブラウザと同じく SSE を開いたまま submit しても、サーバは接続を閉じて終了する。
    let events = open_events(&kemi.url).await;
    assert_eq!(events.status(), 200);

    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);

    let (status, stdout) = kemi.wait();
    drop(events);

    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(document["verdict"], "approved");
}

/// 組めないフラグの判定は cli.rs の単体テストが確かめる。ここは拒否が終了コードと stderr に
/// 届く配線だけを見る。
#[test]
fn result_refuses_other_flags_in_english() {
    let dir = TempDir::new();
    let state = TempDir::new();
    // 結果を 1 件置く。拒否が無ければ、それを出して 0 で終わる。
    kemi_core::result::save(
        &results_dir(&state.path),
        "00000000000000aa",
        r#"{"kemi":3,"verdict":"approved"}"#,
        1,
        1,
    )
    .unwrap();

    let output = run_with_state(&dir.path, &["--result", "--any", "--worktree"], &state.path);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
    assert!(!contains_japanese(&stderr), "{stderr}");
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

#[tokio::test]
async fn result_flag_prints_past_contract_versions_without_reserializing() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (_, _, code) = submit_manifest(&dir.path, &state.path, "approved", "版 1").await;
    assert_eq!(code, Some(0));
    // ライブ改訂の前の kemi が書いた結果（返信は本文の並び、page も messages も無い）。
    let version_1 = "{\"kemi\":1,\"title\":\"old\",\"verdict\":\"changes_requested\",\
        \"approval\":[],\"comments\":[{\"id\":\"c1\",\"replies\":[\"ok\"]}]}\n";
    std::fs::write(&result_files(&state.path)[0], version_1).unwrap();

    let output = run_with_state(&dir.path, &["--result"], &state.path);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), version_1);
    let version_2 = "{ \"verdict\" : \"approved\", \"kemi\" : 2, \"title\" : \"old\", \"approval\" : [], \"comments\" : [], \"messages\" : [] }\n";
    std::fs::write(&result_files(&state.path)[0], version_2).unwrap();
    let output = run_with_state(&dir.path, &["--result"], &state.path);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), version_2);
    // 説明へのコメントが無くても説明の全文を versions に持ち、点を 10.0 と書いた版 3 の結果。
    let version_3 = "{\"kemi\":3,\"diff_display_version\":\"01K00000000000000000000000:2\",\
        \"explanation\":{\"current_revision\":1,\"versions\":[{\"revision\":1,\
        \"base_display_version\":\"01K00000000000000000000000:1\",\"sections\":[]}]},\
        \"title\":\"old\",\"verdict\":\"changes_requested\",\"approval\":[],\"comments\":[{\"id\":\"c1\",\
        \"target_type\":\"page\",\"page\":{\"places\":[{\"n\":1,\"kind\":\"pen\",\
        \"points\":[{\"x\":10.0,\"y\":20.5}]}]}}],\"messages\":[]}\n";
    std::fs::write(&result_files(&state.path)[0], version_3).unwrap();
    let output = run_with_state(&dir.path, &["--result"], &state.path);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), version_3);
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
    // 2 つのワークスペースの結果を、送信時刻を 1970 年の直後にして 20 件置く。kemi の submit
    // の送信時刻はどれよりも新しいので、機械の時計が多少戻っても、消えるのは種の最古の 1 件。
    // 残す件数と消す順そのものは、送信時刻を与える crates/kemi-core/src/result.rs のテストが
    // 確かめる。ここはバイナリの保存で刈り込みが起きることを見る。
    let dir = TempDir::new();
    let state = TempDir::new();
    let seeds: Vec<PathBuf> = (0..20u32)
        .map(|index| {
            let key = if index % 2 == 0 {
                "00000000000000aa"
            } else {
                "00000000000000bb"
            };
            kemi_core::result::save(
                &results_dir(&state.path),
                key,
                r#"{"kemi":3,"verdict":"approved"}"#,
                1000 + u128::from(index),
                1,
            )
            .unwrap()
        })
        .collect();

    let (_, stdout, code) = submit_manifest(&dir.path, &state.path, "approved", "21 件目").await;

    assert_eq!(code, Some(0));
    let files = result_files(&state.path);
    assert_eq!(files.len(), 20);
    assert!(!seeds[0].exists(), "the oldest result must be pruned");
    assert!(
        files
            .iter()
            .any(|file| std::fs::read_to_string(file).unwrap() == stdout),
        "the submitted result must be kept"
    );
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
    // 状態の置き場所を相対パスにするので kemi_command は使わない。HOME を一時ディレクトリに
    // 向けるので、結果は利用者の本物の置き場所には書かれない。
    let kemi = Kemi::start(
        Command::new(env!("CARGO_BIN_EXE_kemi"))
            .args(["manifest.json", "--no-open", "--port", "0"])
            .current_dir(&dir.path)
            .env("XDG_STATE_HOME", "relative/state")
            .env("HOME", &home.path),
    );
    kemi.submit("approved").await;
    kemi.wait();

    assert_eq!(
        result_files(&home.path.join(".local/state")).len(),
        1,
        "the default location under HOME must be used"
    );
    assert!(!dir.path.join("relative").exists());
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
    let mut actual: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(&writable_stdout).unwrap();
    assert!(actual["diff_display_version"].is_string());
    assert!(expected["diff_display_version"].is_string());
    assert_ne!(
        actual["diff_display_version"],
        expected["diff_display_version"]
    );
    actual
        .as_object_mut()
        .unwrap()
        .remove("diff_display_version");
    expected
        .as_object_mut()
        .unwrap()
        .remove("diff_display_version");
    assert_eq!(actual, expected);
    assert!(answer["saved"]["error"].is_string());
    // 文言は契約でないので、書ける場合より stderr の行が増えたことで警告を確かめる。
    // 状態の置き場所に書けないとセッションを作れず、review の行も出ないので、比べる前に
    // 書ける側からも除く。
    let writable_stderr: Vec<&String> = writable_stderr
        .iter()
        .filter(|line| !line.starts_with("kemi: review "))
        .collect();
    assert!(
        stderr.len() > writable_stderr.len(),
        "a warning must go to stderr: {stderr:?} vs {writable_stderr:?}"
    );
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

#[tokio::test]
async fn without_a_state_location_there_is_no_review_line_and_submit_still_works() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let mut kemi = Kemi::start(
        Command::new(env!("CARGO_BIN_EXE_kemi"))
            .args(["--worktree", "--no-open"])
            .current_dir(&dir.path)
            .env_remove("XDG_STATE_HOME")
            .env_remove("HOME")
            .env_remove("LOCALAPPDATA"),
    );
    kemi.wait_serving().await;

    kemi.submit("approved").await;

    let mut rest = String::new();
    kemi.stderr.read_to_string(&mut rest).unwrap();
    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    assert!(stdout.contains("\"verdict\":\"approved\""));
    assert!(!rest.contains("kemi: review"), "{rest}");
}
