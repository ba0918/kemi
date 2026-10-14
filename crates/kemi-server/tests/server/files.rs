use super::*;

#[tokio::test]
async fn file_rows_collapse_and_expand() {
    let server = TestServer::start().await;
    let body: Value = server.get("api/file/f1").await.json().await.unwrap();

    assert_eq!(body["binary"], false);
    assert_eq!(body["rows"][0]["kind"], "skip");
    assert_eq!(body["rows"][0]["count"], 7);
    assert_eq!(body["rows"][0]["from"], 0);
    assert_eq!(body["rows"][0]["to"], 7);
    assert_eq!(body["old_total"], 21);
    assert_eq!(body["new_total"], 21);

    let expanded: Value = server
        .get("api/file/f1?from=0&to=7")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(expanded["rows"].as_array().unwrap().len(), 7);
    assert_eq!(expanded["rows"][0]["kind"], "equal");
    assert_eq!(expanded["rows"][0]["old"]["text"], "top0");
    assert_eq!(expanded["next"], Value::Null);
}

#[tokio::test]
async fn binary_file_has_no_rows() {
    let server = TestServer::start().await;
    let body: Value = server.get("api/file/f2").await.json().await.unwrap();

    assert_eq!(body["binary"], true);
    assert_eq!(body["old_size"], 10);
    assert_eq!(body["new_size"], 20);
    assert_eq!(body["rows"].as_array().unwrap().len(), 0);
    assert_eq!(server.source.reads(), 0);
}

#[tokio::test]
async fn highlight_enabled_by_default_and_rows_carry_html() {
    let server = TestServer::start().await;
    let body: Value = server
        .get("api/file/f1?from=0&to=3")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["highlight"]["capable"], true);
    assert_eq!(body["highlight"]["enabled"], true);
    let html = body["rows"][0]["old"]["html"].as_str().unwrap();
    assert!(html.contains("<span"), "{html}");
}

#[tokio::test]
async fn highlight_can_be_turned_off() {
    let server = TestServer::start().await;
    let body: Value = server
        .get("api/file/f1?from=0&to=3&highlight=off")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["highlight"]["enabled"], false);
    assert!(body["rows"][0]["old"].get("html").is_none());
}

#[tokio::test]
async fn highlight_cap_can_be_overridden_for_one_file() {
    let server = TestServer::start().await;

    let capped: Value = server.get("api/file/f3").await.json().await.unwrap();
    assert_eq!(capped["highlight"]["capable"], false);
    assert_eq!(capped["highlight"]["enabled"], false);

    let forced: Value = server
        .get("api/file/f3?highlight=on&from=0&to=2")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(forced["highlight"]["capable"], false);
    assert_eq!(forced["highlight"]["enabled"], true);
    assert!(forced["rows"][0]["old"]["html"].is_string());
}

// ---- R-RENDER（描画のエンドポイントと api/file の描画表示の情報） ----

#[tokio::test]
async fn render_requires_the_token() {
    let server = TestServer::start().await;
    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/api/render/f5",
            server.port
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn render_reads_content_only_when_the_rendered_view_is_requested() {
    let server = TestServer::start().await;
    let _ = server.get("api/review").await.text().await.unwrap();
    assert_eq!(server.source.reads(), 0);

    let response = server.get("api/render/f5").await;

    assert_eq!(response.status(), 200);
    assert_eq!(server.source.reads(), 1);
}

#[tokio::test]
async fn file_of_a_markdown_offers_the_rendered_toggle_and_other_files_do_not() {
    let server = TestServer::start().await;

    let markdown: Value = server.get("api/file/f5").await.json().await.unwrap();
    assert_eq!(markdown["render"]["target"], "markdown");
    assert_eq!(markdown["render"]["toggle"], true);
    assert_eq!(markdown["render"]["initial"], "source");
    assert_eq!(markdown["render"]["reason"], Value::Null);

    let source: Value = server.get("api/file/f1").await.json().await.unwrap();
    assert_eq!(source["render"]["target"], Value::Null);
    assert_eq!(source["render"]["toggle"], false);
}

#[tokio::test]
async fn file_of_a_markdown_over_the_line_limit_reports_why_it_cannot_be_rendered() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f4").await.json().await.unwrap();

    assert_eq!(body["render"]["target"], "markdown");
    assert_eq!(body["render"]["toggle"], true);
    assert!(body["render"]["reason"].is_string(), "{body}");
}

#[tokio::test]
async fn file_of_an_image_carries_no_line_or_byte_reason() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f2").await.json().await.unwrap();

    assert_eq!(body["render"]["reason"], Value::Null, "{body}");
}

#[tokio::test]
async fn render_follows_the_highlight_setting_and_the_theme() {
    let server = TestServer::start().await;

    let light: Value = server.get("api/render/f5").await.json().await.unwrap();
    let off: Value = server
        .get("api/render/f5?highlight=off")
        .await
        .json()
        .await
        .unwrap();
    let dark: Value = server
        .get("api/render/f5?dark=1")
        .await
        .json()
        .await
        .unwrap();

    let light_html = light["html"].as_str().unwrap();
    let off_html = off["html"].as_str().unwrap();
    let dark_html = dark["html"].as_str().unwrap();
    assert!(light_html.contains("style=\"color:#"), "{light_html}");
    assert!(!off_html.contains("style=\"color:#"), "{off_html}");
    assert!(dark_html.contains("style=\"color:#"), "{dark_html}");
    assert_ne!(light_html, dark_html);
    assert!(off_html.contains("let a = 1;"), "{off_html}");
}

#[tokio::test]
async fn render_returns_blocks_and_turns_review_image_references_into_urls() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();

    assert_eq!(body["kind"], "markdown");
    let blocks = body["blocks"].as_array().unwrap();
    assert_eq!(blocks[0]["side"], "new");
    assert_eq!(blocks[0]["start"], 1);
    assert_eq!(blocks[0]["end"], 1);
    assert_eq!(blocks[0]["mark"], "unchanged");
    assert_eq!(blocks[1]["mark"], "modified");
    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<img src=\"api/image/"), "{html}");
    assert!(html.contains("data-kemi-path=\"img/shot.png\""), "{html}");
    assert_eq!(body["new_lines"].as_array().unwrap().len(), 9);
    assert_eq!(body["old_lines"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn render_of_a_markdown_the_parser_rejects_is_a_failure_response() {
    let server = TestServer::start().await;

    let response = server.get("api/render/f7").await;

    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.unwrap();
    assert!(body["error"].is_string(), "{body}");
}

#[tokio::test]
async fn crlf_markdown_is_measured_the_same_way_before_and_at_rendering() {
    // 生バイトは 1 MB を超え、改行を正規化すると下回る。行数（10,000）は上限の中。
    let text = format!("{}\r\n\r\n", "x".repeat(206)).repeat(5_000);
    assert!(text.len() > AUTO_MAX_BYTES);
    assert!(text.len() - 10_000 <= AUTO_MAX_BYTES);
    let source = FakeSource::new().with_file(
        FileEntry {
            status: Status::Add,
            ..file_entry("f13", "docs/crlf.md")
        },
        FileContent {
            old: None,
            new: Some(text.into_bytes()),
        },
    );
    let server = TestServer::start_source(Arc::new(source), Ipv4Addr::LOCALHOST, None).await;

    let file: Value = server
        .get("api/file/f13?highlight=off")
        .await
        .json()
        .await
        .unwrap();
    let render = server.get("api/render/f13").await;

    assert_eq!(file["render"]["reason"], Value::Null, "{}", file["render"]);
    assert_eq!(render.status(), 200);
}

#[tokio::test]
async fn file_of_a_csv_with_an_unbalanced_quote_reports_why_it_cannot_be_rendered() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f8").await.json().await.unwrap();

    assert_eq!(body["render"]["target"], "table");
    assert_eq!(body["render"]["toggle"], true);
    assert!(body["render"]["reason"].is_string(), "{body}");
    let response = server.get("api/render/f8").await;
    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn render_of_a_tsv_returns_table_rows_as_blocks() {
    let server = TestServer::start().await;

    let file: Value = server.get("api/file/f9").await.json().await.unwrap();
    assert_eq!(file["render"]["target"], "table");
    assert_eq!(file["render"]["reason"], Value::Null);

    let body: Value = server.get("api/render/f9").await.json().await.unwrap();
    assert_eq!(body["kind"], "table");
    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<th>note</th>"), "{html}");
    assert!(html.contains("data-kemi-block=\"old:2-2\""), "{html}");
    assert!(html.contains("data-kemi-block=\"new:2-2\""), "{html}");
    assert_eq!(body["blocks"].as_array().unwrap().len(), 3);
}

// ---- R-RENDER（画像） ----

#[tokio::test]
async fn review_image_bytes_carry_the_content_type_nosniff_and_sandbox_headers() {
    let server = TestServer::start().await;

    let response = server.get("api/image/review/f2/new").await;

    assert_eq!(response.status(), 200);
    let headers = response.headers().clone();
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("content-security-policy").unwrap(), "sandbox");
    assert_eq!(
        response.bytes().await.unwrap().to_vec(),
        vec![0xff, 0x00, 0x02]
    );

    let old = server.get("api/image/review/f2/old").await;
    assert_eq!(old.bytes().await.unwrap().to_vec(), vec![0xff, 0x00, 0x01]);
    let svg = server.get("api/image/review/f11/new").await;
    assert_eq!(svg.headers().get("content-type").unwrap(), "image/svg+xml");
}

#[tokio::test]
async fn review_image_is_refused_without_the_token_and_for_unknown_or_non_image_ids() {
    let server = TestServer::start().await;
    let without_token = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/api/image/review/f2/new",
            server.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(without_token.status(), 404);

    assert_eq!(server.get("api/image/review/nope/new").await.status(), 404);
    assert_eq!(server.get("api/image/review/f1/new").await.status(), 404);
    assert_eq!(server.get("api/image/review/f6/old").await.status(), 404);
}

#[tokio::test]
async fn file_of_an_image_over_five_megabytes_only_reports_the_byte_counts() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f10").await.json().await.unwrap();

    assert_eq!(body["binary"], true);
    assert_eq!(body["new_size"], 6_000_000);
    assert_eq!(body["render"]["target"], Value::Null);
    assert_eq!(server.get("api/render/f10").await.status(), 422);
}

#[tokio::test]
async fn untracked_files_over_the_limit_are_not_rendered_because_their_content_was_not_read() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    // 1 MB 超 5 MB 未満の untracked。画像は 5 MB の規則の中だが、内容を読んでいない。
    let big = vec![b'x'; UNTRACKED_LIMIT as usize + 1];
    std::fs::write(repo.path.join("shot.png"), &big).unwrap();
    std::fs::write(repo.path.join("notes.md"), &big).unwrap();
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = LiveServer::start(source).await;

    let review = server.get_json("api/review").await;
    let id_of = |path: &str| {
        file_in(&review, 0, path)["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let image = server
        .get_json(&format!("api/file/{}", id_of("shot.png")))
        .await;
    let markdown = server
        .get_json(&format!("api/file/{}", id_of("notes.md")))
        .await;

    assert_eq!(
        image["render"]["target"],
        Value::Null,
        "{}",
        image["render"]
    );
    assert!(
        markdown["render"]["reason"].is_string(),
        "{}",
        markdown["render"]
    );
    server.stop();
}

#[tokio::test]
async fn file_of_an_svg_has_the_toggle_with_rendered_as_default_and_other_images_have_no_toggle() {
    let server = TestServer::start().await;

    let svg: Value = server.get("api/file/f11").await.json().await.unwrap();
    assert_eq!(svg["render"]["target"], "image");
    assert_eq!(svg["render"]["toggle"], true);
    assert_eq!(svg["render"]["initial"], "rendered");
    assert_eq!(svg["binary"], false);

    let png: Value = server.get("api/file/f2").await.json().await.unwrap();
    assert_eq!(png["render"]["target"], "image");
    assert_eq!(png["render"]["toggle"], false);
    assert_eq!(png["render"]["initial"], "rendered");
}

#[tokio::test]
async fn render_of_an_image_gives_both_sides_and_marks_a_rename_with_identical_bytes_as_unchanged()
{
    let server = TestServer::start().await;

    let changed: Value = server.get("api/render/f2").await.json().await.unwrap();
    assert_eq!(changed["kind"], "image");
    assert_eq!(changed["same"], false);
    assert_eq!(changed["old"]["size"], 3);
    assert_eq!(changed["new"]["size"], 3);
    assert!(
        changed["old"]["url"]
            .as_str()
            .unwrap()
            .contains("api/image/review/f2/old")
    );

    let renamed: Value = server.get("api/render/f12").await.json().await.unwrap();
    assert_eq!(renamed["same"], true);

    let added: Value = server.get("api/render/f6").await.json().await.unwrap();
    assert_eq!(added["old"], Value::Null);
    assert_eq!(added["new"]["size"], 5);
}

#[tokio::test]
async fn relative_images_in_the_review_use_the_review_url_and_others_use_the_repository_url() {
    let source =
        Arc::new(guide_with_two_images().with_repository(&[("docs/img/other.png", &[0x89, 1])]));
    let server = TestServer::start_source(source, Ipv4Addr::LOCALHOST, None).await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let sources = image_sources(body["html"].as_str().unwrap());

    assert_eq!(sources.len(), 2, "{sources:?}");
    assert!(
        sources[0].contains("api/image/review/f6/new"),
        "{sources:?}"
    );
    assert!(!sources[1].contains("review"), "{sources:?}");
    let response = server.get(&sources[1]).await;
    let status = response.status();
    assert_eq!(
        status,
        200,
        "{} -> {}",
        sources[1],
        response.text().await.unwrap()
    );
    let response = server.get(&sources[1]).await;
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        response.headers().get("content-security-policy").unwrap(),
        "sandbox"
    );
    assert_eq!(response.bytes().await.unwrap().to_vec(), vec![0x89, 1]);
}

#[tokio::test]
async fn relative_images_the_source_cannot_serve_are_404_from_the_repository_url() {
    let source = Arc::new(guide_with_two_images().with_repository(&[]));
    let server = TestServer::start_source(source, Ipv4Addr::LOCALHOST, None).await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let sources = image_sources(body["html"].as_str().unwrap());

    assert_eq!(sources.len(), 2, "{sources:?}");
    assert_eq!(server.get(&sources[1]).await.status(), 404);
    let without_token = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/{}",
            server.port, sources[1]
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(without_token.status(), 404);
}

#[tokio::test]
async fn relative_images_are_frames_when_the_source_cannot_read_the_repository() {
    let server =
        TestServer::start_source(Arc::new(guide_with_two_images()), Ipv4Addr::LOCALHOST, None)
            .await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let html = body["html"].as_str().unwrap();
    let sources = image_sources(html);

    assert_eq!(sources.len(), 1, "{sources:?}");
    assert!(
        sources[0].contains("api/image/review/f6/new"),
        "{sources:?}"
    );
    assert!(html.contains("kb-img-frame"), "{html}");
    assert!(html.contains("img/other.png"), "{html}");
}

#[tokio::test]
async fn expanding_one_row_of_a_large_file_keeps_line_numbers_and_disabled_highlighting() {
    let text = "abcdefghij abcdefghij abcdefghij abcdefghij\n".repeat(100_000);
    let source = Arc::new(FakeSource::new().with_file(
        file_entry("f20", "large.txt"),
        FileContent {
            old: Some(text.clone().into_bytes()),
            new: Some(text.into_bytes()),
        },
    ));
    let server = TestServer::start_source(source, Ipv4Addr::LOCALHOST, None).await;
    let file: Value = server
        .get("api/file/f20?highlight=off&from=0&to=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(file["rows"].as_array().unwrap().len(), 1);
    assert_eq!(file["rows"][0]["old"]["number"], 1);
    assert_eq!(file["rows"][0]["new"]["number"], 1);
    assert_eq!(file["highlight"]["enabled"], false);
    assert!(file["next"].is_null());
    server
        .post("api/submit", json!({"verdict":"approved"}))
        .await;
    server.finish().await;
}

#[tokio::test]
async fn conversation_markdown_renders_without_image_requests_and_does_not_read_files() {
    let server = TestServer::start().await;
    let response = server
        .post(
            "api/parts",
            json!({"type":"text","body":"**hello** ![shot](https://example.com/a.png)"}),
        )
        .await;
    assert_eq!(response.status(), 200);
    let value: Value = response.json().await.unwrap();
    assert!(
        value["html"]
            .as_str()
            .unwrap()
            .contains("<strong>hello</strong>")
    );
    assert!(!value["html"].as_str().unwrap().contains("<img"));
}
