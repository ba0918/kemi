use super::*;

const RUNNING: &str = "01HF7YAT00AAAAAAAAAAAAAAAA";
const OTHER: &str = "01HF7YAT00BBBBBBBBBBBBBBBB";

#[test]
fn ps_json_lists_a_running_review_with_all_keys() {
    let here = TempDir::new();
    let state = TempDir::new();
    let record = RunningRecord {
        live_url: Some("http://127.0.0.1:2/app".to_string()),
        mode: RunningMode::Live,
        label: "live http://127.0.0.1:9/app".to_string(),
        ..crafted_record(&here.path, 1_700_000_000_123)
    };
    let _running = hold_running(&state.path, RUNNING, Some(&record));

    let listed = ps_json(&here.path, &state.path, &[]);

    assert_eq!(listed.len(), 1, "{listed:?}");
    let row = &listed[0];
    assert_eq!(
        sorted_keys(row),
        [
            "id",
            "label",
            "live_url",
            "mode",
            "pid",
            "started_at",
            "url",
            "version",
            "workspace"
        ]
    );
    assert_eq!(row["id"], RUNNING);
    assert_eq!(row["workspace"], *record.workspace.to_string_lossy());
    assert_eq!(row["mode"], "live");
    assert_eq!(row["label"], record.label);
    assert_eq!(row["started_at"], "2023-11-14T22:13:20.123Z");
    assert_eq!(row["pid"], 4242);
    assert_eq!(row["url"], record.url);
    assert_eq!(row["live_url"], "http://127.0.0.1:2/app");
    assert_eq!(row["version"], "9.9.9");
}

#[test]
fn ps_text_shows_the_pid_and_the_whole_url() {
    let here = TempDir::new();
    let state = TempDir::new();
    let record = crafted_record(&here.path, 1_700_000_000_123);
    let _running = hold_running(&state.path, RUNNING, Some(&record));

    let output = run_ps(&here.path, &state.path, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    for value in [RUNNING, "4242", record.url.as_str(), "worktree"] {
        assert!(text.contains(value), "{value} is missing from:\n{text}");
    }
    assert!(!contains_japanese(&text), "{text}");
}

#[test]
fn ps_without_running_reviews_prints_one_line_and_an_empty_array() {
    let here = TempDir::new();
    let state = TempDir::new();
    let assert_nothing_running = || {
        let output = run_ps(&here.path, &state.path, &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
        assert!(ps_json(&here.path, &state.path, &[]).is_empty());
    };

    // kemi を初めて使う（置き場所がまだ無い）。
    assert!(!sessions_dir(&state.path).exists());
    assert_nothing_running();
    // 保留中のセッションだけがある。
    craft_session(
        &state.path,
        OTHER,
        SessionMode::Worktree,
        "Working tree changes",
    );
    assert_nothing_running();
}

#[test]
fn ps_lists_a_review_without_a_record_and_warns_only_for_a_broken_one() {
    let here = TempDir::new();
    let state = TempDir::new();
    let _missing = hold_running(&state.path, RUNNING, None);
    let _broken = hold_running(&state.path, OTHER, None);
    std::fs::create_dir_all(endpoint_dir(&state.path)).unwrap();
    std::fs::write(running_record_path(&state.path, OTHER), b"{broken").unwrap();

    let output = run_ps(&here.path, &state.path, &["--format", "json"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let listed: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let ids: Vec<&str> = listed
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [RUNNING, OTHER]);
    for row in &listed {
        for (key, value) in row.as_object().unwrap() {
            assert!(key == "id" || value.is_null(), "{key} of {row}");
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(OTHER), "{stderr}");
    assert!(!stderr.contains(RUNNING), "{stderr}");
}

#[test]
fn ps_reports_reviews_outside_the_repository_and_any_shows_them() {
    let here = TempDir::new();
    let elsewhere = TempDir::new();
    let state = TempDir::new();
    let _here = hold_running(&state.path, RUNNING, Some(&crafted_record(&here.path, 200)));
    let _elsewhere = hold_running(
        &state.path,
        OTHER,
        Some(&crafted_record(&elsewhere.path, 100)),
    );

    let output = run_ps(&here.path, &state.path, &[]);
    let any = run_ps(&here.path, &state.path, &["--any"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(RUNNING) && !text.contains(OTHER), "{text}");
    let last = text.lines().last().unwrap();
    assert!(last.contains('1') && last.contains("--any"), "{text}");
    let any = String::from_utf8(any.stdout).unwrap();
    assert!(any.contains(RUNNING) && any.contains(OTHER), "{any}");
    let ids: Vec<String> = ps_json(&here.path, &state.path, &["--any"])
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, [RUNNING, OTHER]);
}

/// ファイルの名前と大きさ、`.lock` 以外は中身。Windows ではロック中のファイルを読めない。
fn snapshot(dir: &Path) -> BTreeMap<String, (u64, Option<Vec<u8>>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let size = entry.metadata().unwrap().len();
            let content =
                (!name.ends_with(".lock")).then(|| std::fs::read(entry.path()).unwrap_or_default());
            (name, (size, content))
        })
        .collect()
}

#[test]
fn ps_does_not_change_the_sessions_dir() {
    let here = TempDir::new();
    let state = TempDir::new();
    craft_session(
        &state.path,
        "01HF7YAT00CCCCCCCCCCCCCCCC",
        SessionMode::Worktree,
        "Working tree changes",
    );
    let _running = hold_running(&state.path, RUNNING, Some(&crafted_record(&here.path, 200)));
    let _missing = hold_running(&state.path, OTHER, None);
    let before = (
        snapshot(&sessions_dir(&state.path)),
        snapshot(&endpoint_dir(&state.path)),
    );

    run_ps(&here.path, &state.path, &[]);
    ps_json(&here.path, &state.path, &["--any"]);

    let after = (
        snapshot(&sessions_dir(&state.path)),
        snapshot(&endpoint_dir(&state.path)),
    );
    assert_eq!(before, after);
}

#[test]
fn ps_rejects_unknown_flags_and_arguments() {
    let here = TempDir::new();
    let state = TempDir::new();
    for args in [&["--all"][..], &["extra"], &["--format", "xml"]] {
        let output = run_ps(&here.path, &state.path, args);

        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("kemi ps"), "{args:?}: {stderr}");
    }
}

#[tokio::test]
async fn ps_named_manifest_opens_with_a_path() {
    let dir = TempDir::new();
    dir.write("ps", MANIFEST);

    let kemi = Kemi::spawn(&dir.path, &["./ps", "--no-open"]);

    assert_eq!(kemi.review_json().await["title"], "e2e のレビュー");
    kemi.kill();
}

#[cfg(unix)]
#[test]
fn ps_exits_2_when_the_state_place_cannot_be_determined() {
    let here = TempDir::new();
    let state = TempDir::new();

    let output = run_command(
        kemi_command(&here.path, &state.path)
            .env_remove("XDG_STATE_HOME")
            .env_remove("HOME")
            .arg("ps"),
        None,
    );

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

// ---- 起動したレビューが書く記録（R-PS-RECORD、R-PS-STOP） ----

#[tokio::test]
async fn ps_url_opens_the_review_and_never_shows_the_agent_token() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);

    let output = run_ps(&dir.path, &state.path, &["--format", "json"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let listed: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    let url = ps_row(&listed, &id)["url"].as_str().unwrap().to_string();
    let response = kemi
        .client
        .get(format!("{url}api/review"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let token = kemi_core::session::read_endpoint(&endpoint_dir(&state.path), &id)
        .unwrap()
        .unwrap()
        .token;
    assert!(!text.contains(&token), "{text}");
    let record = std::fs::read_to_string(running_record_path(&state.path, &id)).unwrap();
    assert!(!record.contains(&token), "{record}");
    kemi.kill();
}

#[test]
fn ps_text_shows_the_same_url_as_stderr() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, _id) = start_worktree_review(&dir, &state, &[]);

    let output = run_ps(&dir.path, &state.path, &[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains(&kemi.url),
        "{} is missing from:\n{text}",
        kemi.url
    );
    assert!(text.contains(&kemi.child.id().to_string()), "{text}");
    kemi.kill();
}

#[test]
fn ps_lists_the_review_started_later_first() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (first, first_id) = start_worktree_review(&dir, &state, &[]);
    let mut second = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    let second_id = second.review_id();

    let ids: Vec<String> = ps_json(&dir.path, &state.path, &[])
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(ids, [second_id, first_id]);
    first.kill();
    second.kill();
}

#[test]
fn ps_live_review_shows_the_relay_url() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (mut live, live_id) = start_live_review(&dir, &state);
    let relay = live.live_url();
    let mut other = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    let other_id = other.review_id();

    let listed = ps_json(&dir.path, &state.path, &[]);

    let live_row = ps_row(&listed, &live_id);
    assert_eq!(live_row["live_url"], relay);
    assert_eq!(live_row["mode"], "live");
    assert_eq!(live_row["label"], format!("live {LIVE_URL}"));
    let other_row = ps_row(&listed, &other_id);
    assert!(other_row["live_url"].is_null(), "{other_row}");
    assert_eq!(other_row["mode"], "worktree");
    live.kill();
    other.kill();
}

/// `kemi_command` はいつも `XDG_STATE_HOME` を設定するので、Windows ではセッションの置き場所と
/// 記録の置き場所（`LOCALAPPDATA` の下）が分かれる形で走る。
#[test]
fn ps_lists_the_pid_and_url_on_windows_too() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);

    let listed = ps_json(&dir.path, &state.path, &[]);

    let row = ps_row(&listed, &id);
    assert_eq!(row["pid"], kemi.child.id());
    assert_eq!(row["url"], kemi.url);
    kemi.kill();
}

/// `XDG_STATE_HOME` が無いと、セッションと記録は同じ置き場所になる（unix は `HOME` の下、
/// Windows は `LOCALAPPDATA` の下）。
#[test]
fn ps_lists_the_pid_and_url_without_xdg_state_home() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    let without_xdg = || {
        let mut command = kemi_command(&dir.path, &state.path);
        command.env_remove("XDG_STATE_HOME");
        command
    };
    let mut kemi = Kemi::start(without_xdg().args(["--worktree", "--no-open"]));
    let id = kemi.review_id();

    let output = run_command(without_xdg().args(["ps", "--format", "json"]), None);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let listed: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let row = ps_row(&listed, &id);
    assert_eq!(row["pid"], kemi.child.id());
    assert_eq!(row["url"], kemi.url);
    assert!(!sessions_dir(&state.path).exists());
    kemi.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn running_record_is_owner_only_and_gone_after_submit() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let record = running_record_path(&state.path, &id);
    let mode = std::fs::metadata(&record).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    kemi.submit("approved").await;
    let (status, _) = kemi.wait();

    assert_eq!(status.code(), Some(0));
    assert!(!record.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn running_record_is_gone_after_suspend() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.wait_serving().await;
    let record = running_record_path(&state.path, &id);
    assert!(record.exists());

    signal(&kemi.child, "-TERM");
    let (status, _) = kemi.wait();

    assert_eq!(status.code(), Some(130));
    assert!(!record.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn ps_shows_a_review_until_it_is_suspended() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.wait_serving().await;
    ps_row(&ps_json(&dir.path, &state.path, &[]), &id);

    signal(&kemi.child, "-TERM");
    let (status, _) = kemi.wait();

    assert_eq!(status.code(), Some(130));
    assert!(ps_json(&dir.path, &state.path, &[]).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn resumed_review_is_listed_with_its_new_pid_url_and_start() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id, _) = frozen_worktree_review(&dir, &state).await;
    let before = ps_row(&ps_json(&dir.path, &state.path, &[]), &id).clone();
    signal(&kemi.child, "-TERM");
    let (status, _) = kemi.wait();
    assert_eq!(status.code(), Some(130));

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);

    let after = ps_row(&ps_json(&dir.path, &state.path, &[]), &id).clone();
    assert_eq!(after["pid"], resumed.child.id());
    assert_ne!(after["pid"], before["pid"]);
    assert_eq!(after["url"], resumed.url);
    assert_ne!(after["url"], before["url"]);
    assert!(
        after["started_at"].as_str() > before["started_at"].as_str(),
        "{before} {after}"
    );
    assert_eq!(after["workspace"], before["workspace"]);
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn ps_returns_within_five_seconds_while_the_server_is_stopped() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.wait_serving().await;
    signal(&kemi.child, "-STOP");

    let started = std::time::Instant::now();
    let listed = ps_json(&dir.path, &state.path, &[]);

    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(ps_row(&listed, &id)["pid"], kemi.child.id());
    kemi.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn killed_review_is_not_listed_and_its_record_is_cleaned_later() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (killed, killed_id) = start_worktree_review(&dir, &state, &[]);
    killed.wait_serving().await;
    signal(&killed.child, "-KILL");
    let _ = killed.wait();
    let record = running_record_path(&state.path, &killed_id);
    assert!(record.exists(), "the record is left by the kill");

    let listed = ps_json(&dir.path, &state.path, &[]);
    assert!(listed.iter().all(|row| row["id"] != killed_id.as_str()));

    // 別のレビューがセッションを書くと、掃除が残った記録を消す。
    let other = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    other.wait_serving().await;
    for _ in 0..500 {
        if !record.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!record.exists());
    other.kill();
}

/// `kemi ps` の出した PID に送り、写しができたセッションを `--resume` で続ける（R-PS-STOP）。
#[cfg(unix)]
async fn stop_by_the_listed_pid_and_resume(signal: &str) -> Option<i32> {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id, _) = frozen_worktree_review(&dir, &state).await;
    let pid = ps_row(&ps_json(&dir.path, &state.path, &[]), &id)["pid"]
        .as_u64()
        .unwrap();

    signal_pid(pid, signal);
    let (status, _) = kemi.wait();

    let text = String::from_utf8(run_ps(&dir.path, &state.path, &[]).stdout).unwrap();
    assert!(!text.contains(&id), "{text}");
    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    resumed.kill();
    status.code()
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_from_ps_pid_suspends_and_the_review_resumes() {
    assert_eq!(stop_by_the_listed_pid_and_resume("-TERM").await, Some(130));
}

#[cfg(unix)]
#[tokio::test]
async fn sigkill_from_ps_pid_still_resumes() {
    assert_eq!(stop_by_the_listed_pid_and_resume("-KILL").await, None);
}

/// manifest のモードの文字は、`kemi --resume` の一覧と同じく manifest の題（R-PS-OUTPUT）。
#[test]
fn ps_labels_a_manifest_review_with_its_title() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("review.json", MANIFEST);
    let mut kemi = Kemi::spawn_with_state(&dir.path, &["review.json", "--no-open"], &state.path);
    let id = kemi.review_id();

    let listed = ps_json(&dir.path, &state.path, &[]);

    let row = ps_row(&listed, &id);
    assert_eq!(row["mode"], "manifest");
    assert_eq!(row["label"], "e2e のレビュー");
    kemi.kill();
}
