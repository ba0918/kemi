use super::*;

#[test]
fn live_usage_errors_exit_2_in_english() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    dir.write("notes.txt", "notes\n");
    let outside = TempDir::new();
    outside.write("page.html", "<p>outside</p>\n");
    let outside_page = outside.path.join("page.html");
    let outside_page = outside_page.to_str().unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec!["--live", "http://example.com/"],
        vec!["--live", "https://localhost:5173/"],
        vec!["--live", "http://localhost:5173/", "--worktree"],
        vec!["--live", "http://localhost:5173/", "--digest"],
        vec!["--live", "http://localhost:5173/", "--staged"],
        vec!["--live", "http://localhost:5173/", "--from", "HEAD"],
        vec!["--live", "http://localhost:5173/", "--base", "."],
        vec!["--live", "http://localhost:5173/", "--digest-top", "3"],
        vec!["--live", "http://localhost:5173/", "--result"],
        vec!["--live", outside_page],
        vec!["--live", "notes.txt"],
        vec!["--live", "missing.html"],
        vec!["--live"],
        vec!["--worktree", "--live-port", "0"],
    ];
    for args in &cases {
        let output = run(&dir.path, args);
        assert_eq!(output.status.code(), Some(2), "kemi {args:?}");
        assert!(output.stdout.is_empty(), "kemi {args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty(), "kemi {args:?}");
        assert!(!contains_japanese(&stderr), "kemi {args:?}: {stderr}");
    }
}

#[cfg(unix)]
#[test]
fn live_refuses_a_symlink_whose_target_is_outside_the_served_range() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let outside = TempDir::new();
    outside.write("page.html", "<p>outside</p>\n");
    std::os::unix::fs::symlink(outside.path.join("page.html"), dir.path.join("link.html")).unwrap();

    let output = run(&dir.path, &["--live", "link.html"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[tokio::test]
async fn live_accepts_an_html_file_outside_git_and_titles_it_by_its_path() {
    let dir = TempDir::new();
    dir.write("docs/page.html", "<p>hello</p>\n");
    let mut kemi = Kemi::spawn(&dir.path, &["--live", "docs/page.html", "--no-open"]);
    kemi.review_id();

    let review = kemi.review_json().await;

    assert_eq!(review["title"], "Live review of /docs/page.html");
    assert_eq!(review["groups"].as_array().unwrap().len(), 0, "{review}");
    assert_eq!(review["approval"], serde_json::json!([]));
    kemi.kill();
}

#[tokio::test]
async fn live_prints_the_url_the_results_and_the_review_lines_in_order() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let state = TempDir::new();
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);

    // 結果の保存先の行は文言を契約としないので、review の行の前に 1 行あることだけを見る。
    let mut line = String::new();
    kemi.stderr.read_line(&mut line).unwrap();
    assert!(
        !line.trim().is_empty() && !line.starts_with("kemi: review "),
        "{line}"
    );
    line.clear();
    kemi.stderr.read_line(&mut line).unwrap();
    assert!(line.starts_with("kemi: review "), "{line}");
    kemi.kill();
}

#[tokio::test]
async fn live_code_view_inside_git_is_the_working_tree() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--live", LIVE_URL, "--no-open"]);

    let review = kemi.review_json().await;

    assert_eq!(review["title"], format!("Live review of {LIVE_URL}"));
    assert_eq!(review["groups"][0]["id"], "worktree");
    assert_eq!(review["groups"][0]["title"], "Working tree changes");
    assert_eq!(review["groups"][0]["files"][0]["path"], "a.txt");
    kemi.kill();
}

#[tokio::test]
async fn live_page_is_read_through_the_live_url_after_opening_the_review_url() {
    let (port, _dev) =
        start_static_dev_server("<html><head></head><body>dev page body</body></html>").await;
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let page = format!("http://127.0.0.1:{port}/start");
    let mut kemi = Kemi::spawn(&dir.path, &["--live", &page, "--no-open"]);
    kemi.review_id();
    let live = kemi.live_url();
    assert!(live.starts_with("http://127.0.0.1:"), "{live}");
    assert!(live.ends_with("/start"), "{live}");

    let client = reqwest::Client::new();
    let refused = client.get(&live).send().await.unwrap();
    let opened = client.get(&kemi.url).send().await.unwrap();
    let cookie = opened
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body = client
        .get(&live)
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(refused.status(), 403);
    assert!(body.contains("dev page body"), "{body}");
    kemi.kill();
}

#[tokio::test]
async fn live_bind_wildcard_warns_after_the_url_with_the_lan_prefix() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let mut kemi = Kemi::spawn(
        &dir.path,
        &["--live", LIVE_URL, "--bind", "0.0.0.0", "--no-open"],
    );

    let mut warning = String::new();
    kemi.stderr.read_line(&mut warning).unwrap();

    assert!(
        warning.starts_with("kemi: exposed on the LAN;"),
        "{warning}"
    );
    kemi.kill();
}

#[tokio::test]
async fn live_file_outside_git_is_read_through_the_live_url() {
    let dir = TempDir::new();
    dir.write(
        "site/page.html",
        "<html><head></head><body>local file page</body></html>",
    );
    let mut kemi = Kemi::spawn(&dir.path, &["--live", "site/page.html", "--no-open"]);
    kemi.review_id();
    let live = kemi.live_url();
    assert!(live.ends_with("/site/page.html"), "{live}");

    let (status, body) = read_live_page(&kemi, &live).await;

    assert_eq!(status, 200);
    assert!(body.contains("local file page"), "{body}");
    kemi.kill();
}
