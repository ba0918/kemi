//! stderr の行と終了コード。機械が読む契約の行（R-INPUT-6、R-AGENT-CLI、R-PAGE-MODE、
//! R-SESSION）の形と、終了コードの意味（R-SUBMIT、R-AGENT-CLI）はここで決める。

/// 成功。承認と変更要求の終了コードは verdict が決める（`Verdict::exit_code`）。
pub(crate) const SUCCESS: i32 = 0;
/// 起動・使い方・実行時のエラー。
pub(crate) const ERROR: i32 = 2;
/// `kemi wait --timeout` の時間切れ。
pub(crate) const TIMEOUT: i32 = 3;
/// 利用者による中止（保留と、選択の取り消し）。
pub(crate) const INTERRUPTED: i32 = 130;

/// レビューの URL の行。計測と起動の確かめが読む。
pub(crate) fn url_line(url: &str) -> String {
    format!("kemi: {url}")
}

/// `kemi wait <id>` などに使うレビューの id の行。
pub(crate) fn review_line(id: &str) -> String {
    format!("kemi: review {id}")
}

/// `--live` の中継の URL の行。
pub(crate) fn live_line(url: &str) -> String {
    format!("kemi: live {url}")
}

/// 復元できるセッションが残るときの、復元のコマンドの行。
pub(crate) fn resume_line(id: &str) -> String {
    format!("kemi: resume with: kemi --resume {id}")
}

/// 契約の行でない 1 行（警告や知らせ）を、`kemi: ` を付けて stderr に出す。
pub(crate) fn warn(message: &str) {
    eprintln!("kemi: {message}");
}

/// 理由を stderr に出して、終了コード 2 で終わる。
pub(crate) fn fail(message: &str) -> ! {
    warn(message);
    std::process::exit(ERROR);
}

/// stdout に書き切る。読み手が閉じたパイプなどで書けなければ、理由を出して終了コード 2 で
/// 終わる（R-SUBMIT）。`println!` は書けないと panic し、契約に無い 101 で終わる。
pub(crate) fn write_stdout(text: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if let Err(error) = stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        fail(&format!("cannot write to stdout: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_lines_keep_their_documented_shape() {
        assert_eq!(
            url_line("http://127.0.0.1:1/s/t/"),
            "kemi: http://127.0.0.1:1/s/t/"
        );
        assert_eq!(review_line("01K5"), "kemi: review 01K5");
        assert_eq!(
            live_line("http://127.0.0.1:2/app"),
            "kemi: live http://127.0.0.1:2/app"
        );
        assert_eq!(resume_line("01K5"), "kemi: resume with: kemi --resume 01K5");
    }
}
