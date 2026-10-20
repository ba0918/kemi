//! 秘密の値（エージェント用の API のトークン、中継の cookie）の比べ方。

/// 長さ以外の比べ方で時間が変わらないように、すべてのバイトを比べる。
pub(crate) fn same_secret(given: &[u8], expected: &[u8]) -> bool {
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}
