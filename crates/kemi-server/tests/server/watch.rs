use super::*;

struct SseStream {
    stream: tokio::net::TcpStream,
    buffer: String,
}

impl SseStream {
    async fn connect(port: u16) -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "GET /s/test-token/api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut sse = SseStream {
            stream,
            buffer: String::new(),
        };
        let mut chunk = [0u8; 2048];
        let read = tokio::time::timeout(WAIT_LIMIT, sse.stream.read(&mut chunk))
            .await
            .unwrap()
            .unwrap();
        sse.buffer
            .push_str(&String::from_utf8_lossy(&chunk[..read]));
        sse
    }

    async fn next_update(&mut self, timeout: Duration) -> bool {
        use tokio::io::AsyncReadExt;

        let deadline = Instant::now() + timeout;
        loop {
            if self.drain_updates() > 0 {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let mut chunk = [0u8; 2048];
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => return false,
                Ok(Ok(read)) => self
                    .buffer
                    .push_str(&String::from_utf8_lossy(&chunk[..read])),
            }
        }
    }

    async fn count_updates(&mut self, duration: Duration) -> usize {
        use tokio::io::AsyncReadExt;

        let deadline = Instant::now() + duration;
        let mut count = self.drain_updates();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return count;
            }
            let mut chunk = [0u8; 2048];
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => return count,
                Ok(Ok(read)) => {
                    self.buffer
                        .push_str(&String::from_utf8_lossy(&chunk[..read]));
                    count += self.drain_updates();
                }
            }
        }
    }

    /// 見張りが効くまで、`touch` で見張られているものを書き換えては update を待つ。見張りの
    /// 登録は別スレッドで行われ、終わった合図が無いので、update が届いたことで登録を観測する。
    /// 書き換えては debounce（500 ms）より十分長く待ち、届かなかったときだけ書き換え直す
    /// （間を詰めると debounce が延び続けて update が出ない）。前の書き換えの update が、
    /// 受けた後になって届くのは、見張りか SSE がこの待ちに近いほど遅れたときに限る。
    async fn until_watched(&mut self, mut touch: impl FnMut(u32)) {
        let deadline = Instant::now() + WAIT_LIMIT;
        for round in 1.. {
            touch(round);
            if self.next_update(Duration::from_secs(2)).await {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "no update event within {WAIT_LIMIT:?}"
            );
        }
    }

    fn drain_updates(&mut self) -> usize {
        let needle = "event: update";
        let mut count = 0;
        while let Some(index) = self.buffer.find(needle) {
            count += 1;
            self.buffer.drain(..index + needle.len());
        }
        count
    }
}

#[tokio::test]
async fn live_worktree_change_sends_update() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "initial change\n");
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = TestServer::start_source(source).await;

    let mut sse = SseStream::connect(server.port).await;
    sse.until_watched(|round| repo.write("a.txt", &format!("two {round}\n")))
        .await;

    server.stop();
}

#[tokio::test]
async fn live_manifest_path_change_sends_update() {
    let dir = TempRepo::new();
    dir.write("old.txt", "one\n");
    dir.write("new.txt", "one\n");
    let json =
        r#"{"groups":[{"diffs":[{"path":"a.txt","old_path":"old.txt","new_path":"new.txt"}]}]}"#;
    let source = Arc::new(ManifestSource::from_json(json, &dir.path).unwrap());
    let server = TestServer::start_source(source).await;

    let mut sse = SseStream::connect(server.port).await;
    sse.until_watched(|round| dir.write("new.txt", &format!("two {round}\n")))
        .await;

    server.stop();
}

#[tokio::test]
async fn live_ref_change_sends_update_and_other_git_writes_are_ignored() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.commit("base");
    let source = Arc::new(GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: "HEAD".to_string(),
            group_by: GroupBy::Commit,
        },
    ));
    let server = TestServer::start_source(source).await;

    let mut sse = SseStream::connect(server.port).await;
    // 対象の ref を進めて、見張りが効いてから確かめる（効く前なら何も確かめずに通る）。
    sse.until_watched(|round| {
        repo.write("probe.txt", &format!("{round}\n"));
        repo.commit(&format!("probe {round}"));
    })
    .await;

    // `--to`（HEAD）以外の ref、たとえば別ブランチの更新は監視対象ではない。
    repo.git(&["branch", "other"]);
    assert_eq!(
        sse.count_updates(Duration::from_millis(1000)).await,
        0,
        "another branch ref sent an event"
    );

    // `.git` の他の書き込みも監視対象ではない。
    std::fs::write(repo.path.join(".git/not-a-ref.txt"), "x").unwrap();
    assert_eq!(
        sse.count_updates(Duration::from_millis(1000)).await,
        0,
        "unrelated .git write sent an event"
    );

    // 対象の ref が進めば通知する。
    repo.write("b.txt", "two\n");
    repo.commit("second");
    assert!(
        sse.next_update(WAIT_LIMIT).await,
        "no update event for the new commit"
    );

    server.stop();
}

#[tokio::test]
async fn live_worktree_read_does_not_send_update() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "initial change\n");
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = TestServer::start_source(source).await;

    let mut sse = SseStream::connect(server.port).await;
    // 見張りが効いてから確かめる（効く前なら何も確かめずに通る）。
    sse.until_watched(|round| repo.write("a.txt", &format!("change {round}\n")))
        .await;
    // api/review はファイルを読む。読み取り（Access）は更新ではない。
    let response = http()
        .get(format!(
            "http://127.0.0.1:{}/s/test-token/api/review",
            server.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    assert!(
        !sse.next_update(Duration::from_millis(700)).await,
        "read-only access must not send an update"
    );
    server.stop();
}

#[tokio::test]
async fn live_rewritten_history_comment_outdated_with_original_group() {
    let (repo, base) = range_repo();
    let server = TestServer::start_source(Arc::new(range_source(
        &repo,
        &base,
        "HEAD",
        GroupBy::Commit,
    )))
    .await;
    let per_commit = server.get_json("api/review").await;
    let old_sha = per_commit["groups"][1]["id"].as_str().unwrap().to_string();
    let file = file_in(&per_commit, 1, "a.txt")["id"].clone();
    server
        .post(
            "api/comment",
            json!({"op": "add", "file_id": file, "side": "new",
                   "start_line": 1, "end_line": 1, "body": "書き換え前"}),
        )
        .await;

    repo.git(&["commit", "--amend", "-q", "-m", "fix: three (amended)"]);
    let refreshed = server.get_json("api/review?refresh=1").await;
    assert_ne!(refreshed["groups"][1]["id"], old_sha.as_str());
    let response = server
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;

    let comment = &document["comments"][0];
    assert_eq!(comment["group_id"], old_sha.as_str());
    assert_eq!(comment["group_title"], "fix: three");
    assert_eq!(comment["outdated"], true);
}
