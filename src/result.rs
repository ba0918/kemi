//! 結果ファイル（R-RESULT）。submit で stdout に出した JSON を、状態ディレクトリにも残す。
//!
//! 名前は `<送信時刻のミリ秒 13 桁>-<リポジトリの識別 16 桁>-<pid>-<試行>.json`（D9）。
//! どのリポジトリの結果かは中身（JSON）に足さず、名前で持つ。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// 全リポジトリ合計で残す件数。
pub const KEEP: usize = 20;

/// 結果ファイルの置き場所。`XDG_STATE_HOME` が絶対パスなら全 OS でその下。それ以外は
/// Windows では `LOCALAPPDATA`（絶対パス）の下、unix では `~/.local/state` の下。
/// 使う環境変数が相対パスか未設定なら決められない。
pub fn results_dir(
    xdg_state_home: Option<&OsStr>,
    home: Option<&OsStr>,
    local_app_data: Option<&OsStr>,
) -> Option<PathBuf> {
    match xdg_state_home.map(Path::new) {
        Some(path) if path.is_absolute() => Some(path.join("kemi").join("results")),
        #[cfg(windows)]
        _ => {
            let _ = home;
            let app_data = Path::new(local_app_data?);
            app_data
                .is_absolute()
                .then(|| app_data.join("kemi").join("results"))
        }
        #[cfg(not(windows))]
        _ => {
            let _ = local_app_data;
            let state = Path::new(home?).join(".local").join("state");
            Some(state.join("kemi").join("results"))
        }
    }
}

/// リポジトリ（git の外なら起動したディレクトリ）の識別。パスのバイト列の FNV-1a。
pub fn workspace_key(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn file_name(millis: u128, key: &str, pid: u32, attempt: u32) -> String {
    format!("{millis:013}-{key}-{pid}-{attempt}.json")
}

/// 名前から（送信時刻, 識別）を読む。結果ファイルでない名前は None。
fn parse_name(name: &str) -> Option<(u128, &str)> {
    let stem = name.strip_suffix(".json")?;
    let mut parts = stem.split('-');
    let millis = parts.next()?.parse().ok()?;
    let key = parts.next()?;
    let pid_ok = parts.next()?.parse::<u32>().is_ok();
    let attempt_ok = parts.next()?.parse::<u32>().is_ok();
    let valid_key = key.len() == 16 && key.bytes().all(|byte| byte.is_ascii_hexdigit());
    (pid_ok && attempt_ok && parts.next().is_none() && valid_key).then_some((millis, key))
}

/// 結果ファイルの名前を古い順（送信時刻、同じ時刻なら名前の順）に並べる。
fn sorted_results(names: &[String]) -> Vec<(u128, &str, &String)> {
    let mut results: Vec<(u128, &str, &String)> = names
        .iter()
        .filter_map(|name| parse_name(name).map(|(millis, key)| (millis, key, name)))
        .collect();
    results.sort_by(|left, right| (left.0, left.2).cmp(&(right.0, right.2)));
    results
}

/// 最新の結果。`key` があればそのリポジトリのものだけから選ぶ（None は `--any`）。
pub fn latest<'a>(names: &'a [String], key: Option<&str>) -> Option<&'a String> {
    sorted_results(names)
        .into_iter()
        .rev()
        .find(|(_, name_key, _)| key.is_none_or(|key| key == *name_key))
        .map(|(_, _, name)| name)
}

/// 新しい方から `keep` 件を残すときに消す名前（古い順）。
pub fn to_prune(names: &[String], keep: usize) -> Vec<&String> {
    let results = sorted_results(names);
    let excess = results.len().saturating_sub(keep);
    results
        .into_iter()
        .take(excess)
        .map(|(_, _, name)| name)
        .collect()
}

fn list_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// 結果を 1 ファイルに書き、古いものを消す。同じ時刻・同じ pid でも、既にある名前には
/// 書かず試行の番号を進めるので、名前は衝突しない。
pub fn save(dir: &Path, key: &str, text: &str, millis: u128, pid: u32) -> std::io::Result<PathBuf> {
    create_private_dir(dir)?;
    let mut attempt = 0;
    let (name, path) = loop {
        let name = file_name(millis, key, pid, attempt);
        let path = dir.join(&name);
        if path.symlink_metadata().is_err() {
            break (name, path);
        }
        attempt += 1;
    };
    // 書き終えてから結果の名前に移す。書き込みの途中で失敗したファイルが、最新の
    // 結果として読まれないように。結果の名前にならない一時の名前で書く。
    // Why not hard_link で名前の衝突まで原子的に防がない: ハードリンクを作れない
    // ファイルシステムでは保存そのものが失敗する。名前には pid が入るので、別の
    // プロセスと同じ名前を取り合うことは無い。
    let temporary = dir.join(format!(".{name}.tmp"));
    let written = create_private_file(&temporary)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(text.as_bytes())
        })
        .and_then(|()| std::fs::rename(&temporary, &path));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    let names = list_names(dir);
    for name in to_prune(&names, KEEP) {
        let _ = std::fs::remove_file(dir.join(name));
    }
    Ok(path)
}

/// 最新の結果ファイルの中身。`key` は `latest` と同じ。
pub fn load_latest(dir: &Path, key: Option<&str>) -> Option<String> {
    let names = list_names(dir);
    let name = latest(&names, key)?;
    std::fs::read_to_string(dir.join(name)).ok()
}

/// 所有者だけが使えるディレクトリ（0700）を作る。新しく作る途中のディレクトリも 0700 に
/// なる。既にある結果のディレクトリは 0700 に直すが、その親は利用者の持ち物なので触らない。
#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// 所有者だけが読み書きできるファイル（0600）を新しく作る。既にあれば失敗する。
#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kemi-result-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&path);
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    const A: &str = "00000000000000aa";
    const B: &str = "00000000000000bb";

    #[test]
    fn result_latest_is_chosen_by_send_time() {
        let names = names(&[
            &format!("0000000000200-{A}-1-0.json"),
            &format!("0000000000900-{A}-1-0.json"),
            &format!("0000000000500-{A}-1-0.json"),
        ]);

        assert_eq!(
            latest(&names, Some(A)),
            Some(&format!("0000000000900-{A}-1-0.json"))
        );
    }

    #[test]
    fn result_latest_ignores_other_workspaces_unless_any() {
        let names = names(&[
            &format!("0000000000100-{A}-1-0.json"),
            &format!("0000000000200-{B}-1-0.json"),
        ]);

        assert_eq!(
            latest(&names, Some(A)),
            Some(&format!("0000000000100-{A}-1-0.json"))
        );
        assert_eq!(
            latest(&names, None),
            Some(&format!("0000000000200-{B}-1-0.json"))
        );
        assert_eq!(latest(&names, Some("00000000000000cc")), None);
    }

    #[test]
    fn result_prune_keeps_the_newest_across_workspaces() {
        let mut all = Vec::new();
        for index in 0..(KEEP + 2) {
            let key = if index % 2 == 0 { A } else { B };
            all.push(format!("{:013}-{key}-1-0.json", 1000 + index));
        }

        let pruned = to_prune(&all, KEEP);

        assert_eq!(pruned, vec![&all[0], &all[1]]);
    }

    #[test]
    fn result_ignores_files_that_are_not_results() {
        let names = names(&["notes.txt", "0000000000100-zz-1-0.json", "x.json"]);

        assert_eq!(latest(&names, None), None);
        assert!(to_prune(&names, 0).is_empty());
    }

    #[test]
    fn result_save_in_the_same_millisecond_does_not_collide() {
        let scratch = Scratch::new();

        let first = save(&scratch.0, A, "{\"n\":1}\n", 1234, 7).unwrap();
        let second = save(&scratch.0, A, "{\"n\":2}\n", 1234, 7).unwrap();

        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "{\"n\":1}\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "{\"n\":2}\n");
    }

    #[test]
    fn result_save_removes_the_oldest_beyond_the_limit() {
        let scratch = Scratch::new();
        for index in 0..=KEEP {
            let key = if index % 2 == 0 { A } else { B };
            save(
                &scratch.0,
                key,
                &format!("{index}\n"),
                1000 + index as u128,
                1,
            )
            .unwrap();
        }

        let mut contents: Vec<String> = std::fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect();
        contents.sort();

        assert_eq!(contents.len(), KEEP);
        assert!(!contents.contains(&"0\n".to_string()));
        assert_eq!(load_latest(&scratch.0, None), Some(format!("{KEEP}\n")));
    }

    #[cfg(not(windows))]
    #[test]
    fn result_dir_uses_xdg_state_home_only_when_absolute() {
        let home = Some(OsStr::new("/home/user"));
        let app_data = Some(OsStr::new("C:\\Users\\user\\AppData\\Local"));

        assert_eq!(
            results_dir(Some(OsStr::new("/state")), home, app_data),
            Some(PathBuf::from("/state/kemi/results"))
        );
        assert_eq!(
            results_dir(Some(OsStr::new("relative")), home, app_data),
            Some(PathBuf::from("/home/user/.local/state/kemi/results"))
        );
        assert_eq!(
            results_dir(None, home, app_data),
            Some(PathBuf::from("/home/user/.local/state/kemi/results"))
        );
        assert_eq!(results_dir(None, None, None), None);
    }

    #[cfg(windows)]
    #[test]
    fn result_dir_uses_local_app_data_on_windows() {
        let app_data = Some(OsStr::new(r"C:\Users\user\AppData\Local"));

        assert_eq!(
            results_dir(None, None, app_data),
            Some(PathBuf::from(r"C:\Users\user\AppData\Local\kemi\results"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn result_dir_prefers_absolute_xdg_over_local_app_data_on_windows() {
        assert_eq!(
            results_dir(
                Some(OsStr::new(r"D:\state")),
                None,
                Some(OsStr::new(r"C:\Users\user\AppData\Local"))
            ),
            Some(PathBuf::from(r"D:\state\kemi\results"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn result_dir_without_local_app_data_is_none_on_windows() {
        assert_eq!(results_dir(None, None, None), None);
    }

    #[cfg(windows)]
    #[test]
    fn result_dir_with_relative_local_app_data_is_none_on_windows() {
        assert_eq!(
            results_dir(None, None, Some(OsStr::new(r"AppData\Local"))),
            None
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn result_dir_ignores_local_app_data_on_unix() {
        let home = Some(OsStr::new("/home/user"));
        let app_data = Some(OsStr::new(r"C:\Users\user\AppData\Local"));

        assert_eq!(
            results_dir(None, home, app_data),
            Some(PathBuf::from("/home/user/.local/state/kemi/results"))
        );
        assert_eq!(
            results_dir(None, home, None),
            Some(PathBuf::from("/home/user/.local/state/kemi/results"))
        );
    }

    #[test]
    fn result_workspace_key_differs_by_path() {
        assert_eq!(
            workspace_key(Path::new("/a")),
            workspace_key(Path::new("/a"))
        );
        assert_ne!(
            workspace_key(Path::new("/a")),
            workspace_key(Path::new("/b"))
        );
        assert_eq!(workspace_key(Path::new("/a")).len(), 16);
    }
}
