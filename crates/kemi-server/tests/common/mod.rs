//! 結合テストのバイナリ（server と live）が共有する補助。

use std::time::Duration;

/// テストの HTTP クライアント。サーバが応答しない退行（ロック順のデッドロックなど）で
/// テストが止まり続けないよう、接続と読み取りに期限を付ける。読み取りの期限は全体の期限では
/// なく、応答の頭が届くまでと、本文の断片の間ごとにかかるので、断片が届き続ける SSE は
/// 切らない。テストの `kemi wait` はこれより早く返る。
///
/// 接続の溜め置きはランタイムに結び付くので、テストごとのランタイムをまたいで 1 つの
/// クライアントを使い回さず、呼ぶたびに作る。
pub(crate) fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .build()
        .expect("the test HTTP client builds")
}
