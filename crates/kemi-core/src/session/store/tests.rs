//! セッションファイルの保存のテスト。関心ごとにファイルを分ける。

mod cleanup;
mod copy;
mod files_dir;
pub(in crate::session) mod fixtures;
mod ids_and_locks;
mod live;
mod state;
mod versions;
