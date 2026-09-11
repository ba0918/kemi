//! テスト用の一時 git リポジトリ。決定的な日付でコミットし、Drop で消す。

use std::io::Write;
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
        repo.git(&["config", "core.hooksPath", "/dev/null"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        self.git_at("2026-01-01T00:00:00+00:00", args)
    }

    /// 日付を指定して git を実行する。複数のマージ基点から git が選ぶものを、
    /// コミットの日付で決めたいときに使う。
    ///
    /// フィクスチャを作る git は、開発者の全体・システムの設定（署名の program、
    /// commit.gpgsign など）を読まない。読むと、同じテストが環境によって失敗する。
    /// 全体の設定の置き場は一時リポジトリの中の無いファイルにし、書かれても外へ漏れない。
    pub fn git_at(&self, date: &str, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .env(
                "GIT_CONFIG_GLOBAL",
                self.path.join(".git").join("test-global-config"),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
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

    /// ssh 鍵で署名したコミット。鍵は `.git` の下に作り、作業ツリーの変更に混ぜない。
    /// ssh-keygen が無い環境では署名できないので None を返し、呼び出し側はテストを飛ばす。
    pub fn add_and_commit_signed(&self, message: &str) -> Option<String> {
        let key = self.path.join(".git").join("kemi-test-signing-key");
        if !key.exists() {
            let output = match Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&key)
                .output()
            {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // eprintln! ではなく stderr へ直接書く。eprintln! はテストの出力の
                    // 捕捉に呑まれ、飛ばしたテストが何も言わずに ok と出る。
                    let _ = writeln!(
                        std::io::stderr(),
                        "ssh-keygen が見つからないため、署名したコミットのテストを飛ばします"
                    );
                    return None;
                }
                result => result.expect("run ssh-keygen"),
            };
            assert!(
                output.status.success(),
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let signing_key = format!("user.signingkey={}", key.display());
        self.git(&["add", "-A"]);
        self.git(&[
            "-c",
            "gpg.format=ssh",
            "-c",
            &signing_key,
            "commit",
            "-q",
            "-S",
            "-m",
            message,
        ]);
        Some(self.head())
    }

    pub fn add_and_commit_at(&self, date: &str, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git_at(date, &["commit", "-q", "-m", message]);
        self.head()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
