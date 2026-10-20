use super::*;

#[cfg(unix)]
#[tokio::test]
async fn version_four_process_resume_uses_frozen_d1_and_keeps_d2_explanation_parts_and_undelivered_hand()
 {
    use kemi_core::domain::conversation_content::ConversationContent;
    use kemi_core::domain::display_version::DisplayVersions;
    use kemi_core::domain::explanation::{
        ExplanationDocument, ExplanationHistory, ExplanationVersion, quote_block,
    };
    use kemi_core::domain::review::{Author, CommentTarget, ExplanationTarget, Reply};
    let workspace = TempDir::new();
    let state = TempDir::new();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    workspace.write("explanation.json", "temporary input");
    let original: ExplanationDocument = serde_json::from_value(serde_json::json!({
        "sections": [{
            "id": "s",
            "title": "Original",
            "blocks": [{
                "id": "b",
                "part": {
                    "type": "code",
                    "body": "literal\r\n",
                    "language": "pseudocode",
                    "role": "example",
                },
                "references": [],
            }],
        }],
    }))
    .unwrap();
    let quote = quote_block(&original, "b").unwrap();
    let mut current = original.clone();
    current.sections[0].blocks[0].part =
        serde_json::from_value(serde_json::json!({"type":"text","body":"current"})).unwrap();
    let mut saved = SessionState {
        display: Some(DisplayVersions {
            current: format!("{id}:2"),
            frozen: Some(format!("{id}:1")),
            issued: 2,
        }),
        explanation: Some(ExplanationHistory {
            current_revision: 2,
            versions: vec![
                ExplanationVersion {
                    revision: 1,
                    base_display_version: format!("{id}:1"),
                    sections: original.sections,
                },
                ExplanationVersion {
                    revision: 2,
                    base_display_version: format!("{id}:2"),
                    sections: current.sections,
                },
            ],
        }),
        ..SessionState::default()
    };
    let mut comment = session_comment();
    comment.group_id.clear();
    comment.group_title.clear();
    comment.target = CommentTarget::Explanation(ExplanationTarget {
        revision: 1,
        block_id: "b".into(),
        quote: quote.clone(),
        selection: None,
    });
    comment.replies.push(Reply {
        id: "r1".into(),
        seq: 2,
        author: Author::Agent,
        body: ConversationContent::Parts(
            serde_json::from_value(serde_json::json!([{"type":"text","body":"**parts**"}]))
                .unwrap(),
        ),
    });
    saved.comments.push(comment);
    saved.channel.note_comment("c1");
    saved.channel.hand(&saved.comments, &saved.messages);
    let store = SessionStore::new(sessions_dir(&state.path));
    let mut open = store
        .create(session_info(
            id,
            SessionMode::Worktree,
            "Working tree changes",
            &workspace.path,
        ))
        .unwrap();
    open.save_frozen_copy(
        kemi_core::session::prepare_copy(session_copy()).unwrap(),
        saved,
    )
    .unwrap();
    drop(open);
    let payload = sessions_dir(&state.path).join(format!("{id}.payload"));
    let before = std::fs::read(&payload).unwrap();
    std::fs::remove_file(workspace.path.join("explanation.json")).unwrap();
    workspace.write("a.txt", "AFTER\n");
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &["--resume", id, "--no-open", "--port", "0"],
        &state.path,
    );
    let review = kemi.review_json().await;
    assert_eq!(review["diff_display_version"], format!("{id}:1"));
    assert_eq!(review["explanation"]["current_revision"], 2);
    assert_eq!(
        kemi.get_json("api/explanation?revision=2").await["base_display_version"],
        format!("{id}:2")
    );
    assert_eq!(std::fs::read(&payload).unwrap(), before);
    assert_eq!(store.read(id).unwrap().state.display.unwrap().issued, 2);
    let file: serde_json::Value = kemi.get("api/file/f1").await.json().await.unwrap();
    assert_eq!(file["rows"][0]["new"]["text"], "ONE");
    let wait = run_with_state(
        &workspace.path,
        &["wait", id, "--timeout", "3"],
        &state.path,
    );
    assert!(
        wait.status.success(),
        "{}",
        String::from_utf8_lossy(&wait.stderr)
    );
    let handed: serde_json::Value = serde_json::from_slice(&wait.stdout).unwrap();
    assert_eq!(handed["kemi"], 3);
    assert_eq!(
        handed["events"][0]["comments"][0]["comment"]["explanation"]["quote"],
        serde_json::json!(quote)
    );
    assert_eq!(
        handed["events"][0]["comments"][0]["comment"]["replies"][0]["parts"][0]["body"],
        "**parts**"
    );
    assert_eq!(
        kemi.post("api/submit", serde_json::json!({"verdict":"approved"}))
            .await
            .status(),
        200
    );
    let (status, text) = kemi.wait();
    assert!(status.success());
    let result: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(result["kemi"], 3);
    assert_eq!(result["diff_display_version"], format!("{id}:1"));
    assert_eq!(
        result["comments"][0]["explanation"]["quote"],
        serde_json::json!(quote)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn live_explanation_only_session_survives_process_resume_and_suspend_without_its_input() {
    let workspace = TempDir::new();
    worktree_fixture(&workspace);
    workspace.write("index.html", "<!doctype html><title>Page</title>");
    workspace.write("explanation.json", "input");
    let state = TempDir::new();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let store = SessionStore::new(sessions_dir(&state.path));
    let mode = SessionMode::Live {
        page: kemi_core::domain::live::LivePage::File("index.html".into()),
        root: workspace.path.clone(),
    };
    let mut open = store
        .create(session_info(
            id,
            mode,
            "Live review of /index.html",
            &workspace.path,
        ))
        .unwrap();
    let explanation = serde_json::json!({
        "current_revision": 1,
        "versions": [{
            "revision": 1,
            "base_display_version": format!("{id}:1"),
            "sections": one_section("Section", "explanation only", serde_json::json!([])),
        }],
    });
    let saved = SessionState {
        display: Some(kemi_core::domain::display_version::DisplayVersions::initial(id).unwrap()),
        explanation: Some(serde_json::from_value(explanation).unwrap()),
        ..SessionState::default()
    };
    open.save_issued_state(saved).unwrap();
    open.close_suspended().unwrap();
    drop(open);
    std::fs::remove_file(workspace.path.join("explanation.json")).unwrap();
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &["--resume", id, "--no-open", "--port", "0"],
        &state.path,
    );
    let review = kemi.get_json("api/review").await;
    assert_eq!(review["diff_display_version"], format!("{id}:2"));
    assert_eq!(review["explanation"]["current_revision"], 1);
    assert_eq!(
        review["explanation"]["versions"][0]["base_display_version"],
        format!("{id}:1")
    );
    assert_eq!(review["comments"], serde_json::json!([]));
    assert_eq!(review["messages"], serde_json::json!([]));
    signal(&kemi.child, "-INT");
    let (status, stdout, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));
    assert!(stdout.is_empty());
    assert_eq!(store.list().unwrap().len(), 1);
    assert_eq!(
        store
            .read(id)
            .unwrap()
            .state
            .explanation
            .unwrap()
            .current_revision,
        1
    );
    assert!(
        !sessions_dir(&state.path)
            .join(format!("{id}.payload"))
            .exists()
    );
}

#[tokio::test]
async fn initial_explanation_and_cli_context_update_roundtrip() {
    let workspace = TempDir::new();
    let state = TempDir::new();
    workspace.write("manifest.json", MANIFEST);
    let sections = serde_json::json!([{
        "id": "overview",
        "title": "Overview",
        "blocks": [{
            "id": "intent",
            "part": { "type": "text", "body": "**Intent**" },
            "references": [],
        }],
    }]);
    workspace.write("explanation.json", &document(&sections));
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &[
            "manifest.json",
            "--explanation",
            "explanation.json",
            "--no-open",
            "--port",
            "0",
        ],
        &state.path,
    );
    let review = kemi.review_json().await;
    assert_eq!(review["explanation"]["current_revision"], 1);
    let id = review["agent"]["review"].as_str().unwrap();
    let context = run_with_state(&workspace.path, &["explain", id, "--context"], &state.path);
    assert_eq!(
        context.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(context["kemi"], 3);
    workspace.write(
        "update.json",
        &update(1, &context["diff_display_version"], &sections),
    );
    let update = run_with_state(
        &workspace.path,
        &["explain", id, "--file", "update.json"],
        &state.path,
    );
    assert_eq!(
        update.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&update.stdout).unwrap()["revision"],
        2
    );
    let refused = run_with_state(
        &workspace.path,
        &["explain", id, "--file", "update.json"],
        &state.path,
    );
    assert_eq!(refused.status.code(), Some(2));
    assert!(refused.stdout.is_empty());
    kemi.post("api/submit", serde_json::json!({"verdict":"approved"}))
        .await;
    let (_, result) = kemi.wait();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result).unwrap()["explanation"]["current_revision"],
        2
    );
}

#[tokio::test]
async fn live_explanation_references_accept_only_registered_pages_and_registration_keeps_the_code_token()
 {
    let workspace = TempDir::new();
    let state = TempDir::new();
    workspace.write("page.html", "<h1>Page</h1>");
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &["--live", "page.html", "--no-open", "--port", "0"],
        &state.path,
    );
    let review = kemi.review_json().await;
    let id = review["agent"]["review"].as_str().unwrap();
    let before = run_with_state(&workspace.path, &["explain", id, "--context"], &state.path);
    let context: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    let sections = one_section(
        "Page",
        "Page explanation",
        serde_json::json!([{ "type": "page", "url": "/next?state=1" }]),
    );
    workspace.write(
        "update.json",
        &update(0, &context["diff_display_version"], &sections),
    );
    assert_eq!(
        run_with_state(
            &workspace.path,
            &["explain", id, "--file", "update.json"],
            &state.path
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        kemi.post(
            "api/explanation",
            serde_json::json!({"op":"register_page","url":"/next?state=1"})
        )
        .await
        .status(),
        200
    );
    let after = run_with_state(&workspace.path, &["explain", id, "--context"], &state.path);
    let after: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    assert_eq!(
        after["diff_display_version"],
        context["diff_display_version"]
    );
    assert!(
        after["references"]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|page| page["url"] == "/next?state=1")
    );
    assert_eq!(
        run_with_state(
            &workspace.path,
            &["explain", id, "--file", "update.json"],
            &state.path
        )
        .status
        .code(),
        Some(0)
    );
    kemi.kill();
}

#[tokio::test]
async fn initial_explanations_work_with_all_input_modes_including_manifest_stdin() {
    let workspace = TempDir::new();
    workspace.write("manifest.json", MANIFEST);
    workspace.write("explanation.json", &one_block_explanation());
    git_repo(&workspace);
    workspace.write("a.txt", "old\n");
    git(&workspace.path, &["add", "a.txt"]);
    git(&workspace.path, &["commit", "-qm", "base"]);
    let base = String::from_utf8(run_git(&workspace.path, &["rev-parse", "HEAD"])).unwrap();
    workspace.write("a.txt", "new\n");
    git(&workspace.path, &["add", "a.txt"]);
    git(&workspace.path, &["commit", "-qm", "new"]);
    workspace.write("a.txt", "working\n");
    git(&workspace.path, &["add", "a.txt"]);
    workspace.write("page.html", "<h1>Page</h1>");
    let modes = vec![
        vec!["manifest.json"],
        vec!["-"],
        vec!["--worktree"],
        vec!["--staged"],
        vec!["--from", base.trim(), "--group-by", "file"],
        vec!["--from", base.trim(), "--group-by", "commit"],
        vec!["--live", "page.html"],
    ];
    for mut args in modes {
        let stdin = args[0] == "-";
        args.extend([
            "--explanation",
            "explanation.json",
            "--no-open",
            "--port",
            "0",
        ]);
        let state = TempDir::new();
        let kemi = Kemi::start_with_input(
            kemi_command(&workspace.path, &state.path).args(&args),
            stdin.then_some(MANIFEST),
        );
        let review = kemi.review_json().await;
        assert_eq!(review["explanation"]["current_revision"], 1, "{args:?}");
        assert_eq!(
            kemi.post("api/submit", serde_json::json!({"verdict":"approved"}))
                .await
                .status(),
            200
        );
        assert_eq!(kemi.wait().0.code(), Some(0));
    }
}

#[test]
fn initial_explanation_exclusions_and_bad_inputs_never_start_a_review_or_print_success() {
    let workspace = TempDir::new();
    workspace.write("manifest.json", MANIFEST);
    workspace.write("bad.json", "{\"sections\":[],\"sections\":[]}");
    for args in [
        vec![
            "manifest.json",
            "--explanation",
            "bad.json",
            "--no-open",
            "--port",
            "0",
        ],
        vec![
            "manifest.json",
            "--explanation",
            "missing.json",
            "--no-open",
            "--port",
            "0",
        ],
        // --explanation と組めないモードの判定は cli.rs と agent.rs の単体テストが確かめる。
        // ここは --digest との拒否の配線だけを見る（拒否が無ければ digest を出して 0 で終わる）。
        vec!["manifest.json", "--digest", "--explanation", "bad.json"],
    ] {
        let result = run(&workspace.path, &args);
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        assert!(result.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("kemi: http"));
    }
}

/// 節 1 つに本文のブロック 1 つの、説明の節の並び。節の id は s、ブロックの id は b。
fn one_section(title: &str, body: &str, references: serde_json::Value) -> serde_json::Value {
    serde_json::json!([{
        "id": "s",
        "title": title,
        "blocks": [{
            "id": "b",
            "part": { "type": "text", "body": body },
            "references": references,
        }],
    }])
}

/// `--explanation` で渡す説明の文書。
fn document(sections: &serde_json::Value) -> String {
    serde_json::json!({ "sections": sections }).to_string()
}

/// `kemi explain` に渡す、説明の置き換えの要求。
fn update(
    expected_revision: u64,
    base_display_version: &serde_json::Value,
    sections: &serde_json::Value,
) -> String {
    serde_json::json!({
        "expected_revision": expected_revision,
        "base_display_version": base_display_version,
        "sections": sections,
    })
    .to_string()
}

/// 本文 1 つだけの起動時の説明。
fn one_block_explanation() -> String {
    document(&one_section(
        "Explanation",
        "Explanation",
        serde_json::json!([]),
    ))
}

/// URL の行の後の stderr を `count` 行読む。
fn lines_after_the_url(kemi: &mut Kemi, count: usize) -> Vec<String> {
    (0..count)
        .map(|_| {
            let mut line = String::new();
            kemi.stderr.read_line(&mut line).unwrap();
            line
        })
        .collect()
}

/// 起動時の説明があっても、契約の行は kemi: <url> → 結果の保存先 → kemi: review →
/// kemi: live の順に出る（live.md の R-PAGE-MODE）。
#[tokio::test]
async fn initial_explanation_keeps_the_url_results_review_and_live_lines_in_order() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    dir.write("page.html", "<h1>Page</h1>");
    dir.write("explanation.json", &one_block_explanation());

    let mut kemi = Kemi::spawn(
        &dir.path,
        &[
            "--live",
            "page.html",
            "--explanation",
            "explanation.json",
            "--no-open",
            "--port",
            "0",
        ],
    );

    assert!(
        kemi.preamble.is_empty(),
        "nothing may precede the URL line: {:?}",
        kemi.preamble
    );
    let lines = lines_after_the_url(&mut kemi, 3);
    // 結果の保存先の行は文言を契約としないので、review と live の前に 1 行あることだけを見る。
    assert!(
        !lines[0].trim().is_empty()
            && !lines[0].starts_with("kemi: review ")
            && !lines[0].starts_with("kemi: live "),
        "{lines:?}"
    );
    assert!(lines[1].starts_with("kemi: review "), "{lines:?}");
    assert!(lines[2].starts_with("kemi: live "), "{lines:?}");
    kemi.kill();
}

/// 起動時の説明があっても、公開の警告は URL の行の後に出る（serve.md の R-SERVE）。
#[tokio::test]
async fn initial_explanation_warns_about_the_lan_after_the_url() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    dir.write("explanation.json", &one_block_explanation());

    let mut kemi = Kemi::spawn(
        &dir.path,
        &[
            "manifest.json",
            "--explanation",
            "explanation.json",
            "--bind",
            "0.0.0.0",
            "--no-open",
            "--port",
            "0",
        ],
    );

    assert!(
        kemi.preamble.is_empty(),
        "nothing may precede the URL line: {:?}",
        kemi.preamble
    );
    let lines = lines_after_the_url(&mut kemi, 1);
    assert!(
        lines[0].starts_with("kemi: exposed on the LAN;"),
        "the URL line must be followed by the LAN warning: {lines:?}"
    );
    kemi.kill();
}

/// レビューの準備を止める門。作業ツリーの `*.txt` を読むときに git が通す clean フィルタが
/// `started` を作り、`released` ができるまで戻らない。準備は起動側がシグナルの受け口を
/// 作った後にサーバの中で走るので、`started` ができた時点でその間にいる。
#[cfg(unix)]
struct PreparationGate {
    dir: TempDir,
}

#[cfg(unix)]
impl PreparationGate {
    fn new(workspace: &TempDir) -> Self {
        workspace.write(".gitattributes", "*.txt filter=gate\n");
        PreparationGate {
            dir: TempDir::new(),
        }
    }

    /// フィルタを、kemi が起動する git にだけ設定する（フィクスチャの git には効かない）。
    /// 門のディレクトリが無くなっても抜ける。テストが release の前に落ちると kemi は止まるが、
    /// その子の git と sh は残り、released を待つだけでは永遠に回り続ける。
    fn apply<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        let filter = format!(
            "sh -c 'touch \"{}\"; while [ ! -e \"{}\" ] && [ -d \"{}\" ]; do sleep 0.01; done; cat'",
            self.dir.path.join("started").display(),
            self.dir.path.join("released").display(),
            self.dir.path.display()
        );
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "filter.gate.clean")
            .env("GIT_CONFIG_VALUE_0", filter)
    }

    fn wait_started(&self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !self.dir.path.join("started").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the review preparation never reached the gate"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn release(&self) {
        std::fs::write(self.dir.path.join("released"), "").unwrap();
    }
}

/// 門で止めた起動時の説明つきのレビューを起動する。
#[cfg(unix)]
fn spawn_gated(
    workspace: &TempDir,
    state: &TempDir,
    gate: &PreparationGate,
    args: &[&str],
) -> StopOnDrop {
    StopOnDrop(
        gate.apply(&mut kemi_command(&workspace.path, &state.path))
            .args(args)
            .args([
                "--explanation",
                "explanation.json",
                "--no-open",
                "--port",
                "0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

/// 起動時の説明を用意している間の Ctrl+C は、URL を出さずに保留として終わる
/// （output.md の R-SUBMIT、R-EXPLANATION-CLI）。
#[cfg(unix)]
#[test]
fn interrupt_while_preparing_the_initial_explanation_suspends_without_a_url() {
    let workspace = TempDir::new();
    worktree_fixture(&workspace);
    workspace.write("explanation.json", &one_block_explanation());
    let gate = PreparationGate::new(&workspace);
    let state = TempDir::new();
    let child = spawn_gated(&workspace, &state, &gate, &["--worktree"]);

    gate.wait_started();
    signal(&child.0, "-INT");
    gate.release();
    let (code, stdout, stderr) = finish_child(child);

    assert_eq!(code, Some(130), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(!stderr.contains("kemi: http"), "{stderr}");
    assert!(!stderr.contains("kemi: review "), "{stderr}");
}

/// 説明を保存した後に起動が失敗しても、復元できるセッションが残るなら復元の行を出して
/// 終了コード 2 で終わる（output.md の R-SUBMIT、session.md の R-SESSION）。`--live` の
/// セッションは説明だけでも復元できる。`<id>.endpoint` の場所をディレクトリで塞いで、
/// つなぎ先を書けなくする。
#[cfg(unix)]
#[test]
fn failing_to_publish_after_saving_the_initial_explanation_prints_the_resume_line() {
    let workspace = TempDir::new();
    worktree_fixture(&workspace);
    workspace.write("page.html", "<h1>Page</h1>");
    workspace.write("explanation.json", &one_block_explanation());
    let gate = PreparationGate::new(&workspace);
    let state = TempDir::new();
    let child = spawn_gated(&workspace, &state, &gate, &["--live", "page.html"]);

    gate.wait_started();
    let id = session_id(&files_with_extensions(&sessions_dir(&state.path), &["lock"])[0]);
    std::fs::create_dir(endpoint_dir(&state.path).join(format!("{id}.endpoint"))).unwrap();
    gate.release();
    let (code, stdout, stderr) = finish_child(child);

    assert_eq!(code, Some(2), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(!stderr.contains("kemi: http"), "{stderr}");
    assert!(
        stderr.contains(&format!("kemi: resume with: kemi --resume {id}")),
        "{stderr}"
    );
}

#[tokio::test]
async fn explanation_cli_storage_failure_keeps_the_previous_state_and_prints_no_success() {
    let workspace = TempDir::new();
    let state = TempDir::new();
    workspace.write("manifest.json", MANIFEST);
    let sections = one_section("Explanation", "old", serde_json::json!([]));
    workspace.write("explanation.json", &document(&sections));
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &[
            "manifest.json",
            "--explanation",
            "explanation.json",
            "--no-open",
            "--port",
            "0",
        ],
        &state.path,
    );
    let review = kemi.review_json().await;
    let id = review["agent"]["review"].as_str().unwrap();
    // 凍結前に差し替えると、保存失敗ではなく凍結との競合を確かめてしまうため。
    let session = wait_for_frozen_session(&state.path).await;
    assert_eq!(session_id(&session), id);
    let original = std::fs::read(&session).unwrap();
    let backup = session.with_extension("backup");
    std::fs::rename(&session, &backup).unwrap();
    std::fs::create_dir(&session).unwrap();
    workspace.write(
        "update.json",
        &update(1, &review["diff_display_version"], &sections),
    );
    let failed = run_with_state(
        &workspace.path,
        &["explain", id, "--file", "update.json"],
        &state.path,
    );
    assert_eq!(failed.status.code(), Some(2));
    assert!(failed.stdout.is_empty());
    let context = run_with_state(&workspace.path, &["explain", id, "--context"], &state.path);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&context.stdout).unwrap()["explanation_revision"],
        1
    );
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    std::fs::remove_dir(&session).unwrap();
    std::fs::rename(&backup, &session).unwrap();
    assert_eq!(
        run_with_state(
            &workspace.path,
            &["explain", id, "--file", "update.json"],
            &state.path
        )
        .status
        .code(),
        Some(0)
    );
    kemi.kill();
}

#[tokio::test]
async fn cli_context_does_not_wait_for_stdin_and_parts_roundtrip_through_all_targets_and_wait() {
    let workspace = TempDir::new();
    let state = TempDir::new();
    git_repo(&workspace);
    workspace.write("a.txt", "old\n");
    git(&workspace.path, &["add", "a.txt"]);
    git(&workspace.path, &["commit", "-qm", "base"]);
    workspace.write("a.txt", "new\n");
    workspace.write("page.html", "<h1>Page</h1>");
    workspace.write("explanation.json", &one_block_explanation());
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &[
            "--live",
            "page.html",
            "--explanation",
            "explanation.json",
            "--no-open",
            "--port",
            "0",
        ],
        &state.path,
    );
    let review = kemi.review_json().await;
    let id = review["agent"]["review"].as_str().unwrap();
    let mut context = StopOnDrop(
        kemi_command(&workspace.path, &state.path)
            .args(["explain", id, "--context"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let _unread = context.0.stdin.take().unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while context.0.try_wait().unwrap().is_none() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "context waited for stdin"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(context.0.wait().unwrap().code(), Some(0));
    let file = file_entry(&review, "a.txt");
    let on_file = serde_json::json!({
        "op": "add",
        "file_id": file["id"],
        "side": "new",
        "body": "File question",
    });
    let on_page = serde_json::json!({
        "op": "add_page",
        "body": "Page question",
        "page": {
            "url": "/page.html",
            "width": 640,
            "places": [{
                "n": 1,
                "kind": "pen",
                "points": [{ "x": 1, "y": 1 }, { "x": 3, "y": 3 }],
            }],
        },
    });
    let on_explanation = serde_json::json!({
        "op": "add_explanation",
        "explanation_revision": 1,
        "block_id": "b",
        "body": "Explanation question",
        "selection": { "text": "plan", "occurrence": 0 },
    });
    let f: serde_json::Value = kemi
        .post("api/comment", on_file)
        .await
        .json()
        .await
        .unwrap();
    let p: serde_json::Value = kemi
        .post("api/comment", on_page)
        .await
        .json()
        .await
        .unwrap();
    let e: serde_json::Value = kemi
        .post("api/comment", on_explanation)
        .await
        .json()
        .await
        .unwrap();
    let parts = serde_json::json!([
        { "type": "text", "body": "**Answer**" },
        { "type": "code", "body": "literal()", "language": "pseudocode", "role": "example" },
    ]);
    let writes = serde_json::json!({ "writes": [
        { "type": "reply", "comment_id": f["id"], "parts": parts },
        { "type": "reply", "comment_id": p["id"], "parts": parts },
        { "type": "reply", "comment_id": e["id"], "parts": parts },
        { "type": "message", "parts": parts },
    ]});
    let reply = run_agent(
        &workspace.path,
        &state.path,
        &["reply", id],
        &writes.to_string(),
    );
    assert_eq!(
        reply.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&reply.stderr)
    );
    let after = kemi.review_json().await;
    for comment in after["comments"].as_array().unwrap() {
        assert_eq!(comment["replies"][0]["parts"], parts);
        assert!(comment["replies"][0]["body"].is_null());
    }
    assert_eq!(after["messages"][0]["parts"], parts);
    let initial_wait = run_with_state(
        &workspace.path,
        &["wait", id, "--timeout", "0"],
        &state.path,
    );
    assert_eq!(initial_wait.status.code(), Some(3));
    assert_eq!(
        kemi.post("api/hand", serde_json::json!({})).await.status(),
        200
    );
    let waited = run_with_state(
        &workspace.path,
        &["wait", id, "--timeout", "0"],
        &state.path,
    );
    assert_eq!(waited.status.code(), Some(0));
    let waited: serde_json::Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(waited["kemi"], 3);
    let selection = serde_json::json!({"text":"plan","occurrence":0});
    for handed in waited["events"][0]["comments"].as_array().unwrap() {
        assert_eq!(handed["comment"]["replies"][0]["parts"], parts);
        if handed["comment"]["target_type"] == "explanation" {
            assert_eq!(handed["comment"]["explanation"]["selection"], selection);
        }
    }
    assert_eq!(
        waited["events"][0]["comments"][2]["comment"]["target_type"],
        "explanation"
    );
    kemi.post("api/submit", serde_json::json!({"verdict":"approved"}))
        .await;
    let (_, result) = kemi.wait();
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["explanation"]["current_revision"], 1);
    assert_eq!(result["comments"][2]["target_type"], "explanation");
    assert_eq!(result["comments"][2]["explanation"]["selection"], selection);
}

#[tokio::test]
async fn resumed_context_keeps_skipped_files_present_but_does_not_offer_line_references() {
    let workspace = TempDir::new();
    let state = TempDir::new();
    git_repo(&workspace);
    workspace.write("base.txt", "base\n");
    git(&workspace.path, &["add", "base.txt"]);
    git(&workspace.path, &["commit", "-qm", "base"]);
    workspace.write("big.txt", &"line\n".repeat(210000));
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &["--worktree", "--no-open", "--port", "0"],
        &state.path,
    );
    let review = kemi.review_json().await;
    let id = review["agent"]["review"].as_str().unwrap().to_string();
    assert_eq!(session_id(&wait_for_frozen_session(&state.path).await), id);
    kemi.kill();
    let resumed = Kemi::spawn_with_state(
        &workspace.path,
        &["--resume", &id, "--no-open", "--port", "0"],
        &state.path,
    );
    resumed.wait_serving().await;
    let context = run_with_state(&workspace.path, &["explain", &id, "--context"], &state.path);
    assert_eq!(
        context.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&context.stdout).unwrap();
    let file = context["references"]["code"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "big.txt")
        .unwrap();
    assert_eq!(file["new_present"], true);
    assert!(file["new_lines"].is_null());
    resumed.kill();
}

#[tokio::test]
async fn explanation_updates_read_one_document_from_stdin_and_named_manifests_remain_paths() {
    let workspace = TempDir::new();
    let state = TempDir::new();
    workspace.write("explain", MANIFEST);
    assert_eq!(
        run(&workspace.path, &["./explain", "--digest"])
            .status
            .code(),
        Some(0)
    );
    let kemi = Kemi::spawn_with_state(
        &workspace.path,
        &["./explain", "--no-open", "--port", "0"],
        &state.path,
    );
    kemi.wait_serving().await;
    let review = kemi.review_json().await;
    let id = review["agent"]["review"].as_str().unwrap();
    let request = update(
        0,
        &review["diff_display_version"],
        &one_section("Title", "From stdin", serde_json::json!([])),
    );
    let answer = run_agent(&workspace.path, &state.path, &["explain", id], &request);
    assert_eq!(
        answer.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&answer.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&answer.stdout).unwrap()["revision"],
        1
    );
    kemi.kill();
}

#[test]
fn initial_opposite_unit_references_refuse_wrong_group_path_side_and_unknown_lines_before_publication()
 {
    let workspace = TempDir::new();
    git_repo(&workspace);
    workspace.write("unchanged.txt", "unchanged\n");
    workspace.write("old-name.txt", "rename\n");
    git(&workspace.path, &["add", "unchanged.txt", "old-name.txt"]);
    git(&workspace.path, &["commit", "-qm", "base"]);
    let sha = || {
        String::from_utf8(run_git(&workspace.path, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string()
    };
    let base = sha();
    workspace.write("first.txt", "first\n");
    git(&workspace.path, &["mv", "old-name.txt", "new-name.txt"]);
    git(&workspace.path, &["add", "first.txt"]);
    git(&workspace.path, &["commit", "-qm", "first"]);
    let first = sha();
    workspace.write("second.txt", "second\n");
    git(&workspace.path, &["add", "second.txt"]);
    git(&workspace.path, &["commit", "-qm", "second"]);
    for startup in ["file", "commit"] {
        let (unit, group) = if startup == "file" {
            ("commit", first.as_str())
        } else {
            ("file", "all")
        };
        let baseline = serde_json::json!({
            "type": "code",
            "unit": unit,
            "group_id": group,
            "path": "first.txt",
            "side": "new",
            "start_line": null,
            "end_line": null,
        });
        let mut references = Vec::new();
        for (key, value) in [
            ("group_id", serde_json::json!(&base)),
            ("path", serde_json::json!("unchanged.txt")),
            ("path", serde_json::json!("old-name.txt")),
            ("side", serde_json::json!("old")),
        ] {
            let mut reference = baseline.clone();
            reference[key] = value;
            references.push(reference);
        }
        if startup == "file" {
            let mut wrong = baseline.clone();
            wrong["path"] = serde_json::json!("second.txt");
            references.push(wrong);
        }
        let mut line = baseline.clone();
        line["start_line"] = serde_json::json!(1);
        line["end_line"] = serde_json::json!(1);
        references.push(line);
        for reference in references {
            let sections = one_section("Title", "Reference", serde_json::json!([reference]));
            workspace.write("explanation.json", &document(&sections));
            let result = run(
                &workspace.path,
                &[
                    "--from",
                    &base,
                    "--group-by",
                    startup,
                    "--explanation",
                    "explanation.json",
                    "--no-open",
                    "--port",
                    "0",
                ],
            );
            assert_eq!(
                result.status.code(),
                Some(2),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stdout.is_empty());
            assert!(!String::from_utf8_lossy(&result.stderr).contains("kemi: http"));
        }
    }
}
