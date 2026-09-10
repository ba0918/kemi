//! テスト用の一時 git リポジトリ。決定的な日付でコミットし、Drop で消す。

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempRepo {
    pub path: PathBuf,
}

impl TempRepo {
    pub fn new() -> Self {
        let unique = format!(
            "kemi-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        let repo = TempRepo { path };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.email", "kemi@example.com"]);
        repo.git(&["config", "user.name", "kemi"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    pub fn write(&self, relative: &str, content: &str) {
        let full = self.path.join(relative);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(full, content).expect("write file");
    }

    pub fn remove(&self, relative: &str) {
        std::fs::remove_file(self.path.join(relative)).expect("remove file");
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn add_and_commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.head()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
