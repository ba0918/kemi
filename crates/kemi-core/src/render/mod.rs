//! 描画表示（R-RENDER）。Markdown・表・画像を、行の差分ではなく描画した見た目で読む
//! ためのブロック列と HTML を、純粋に計算する。
//!
//! ブロックの行レンジ・変更の印・重ねる判定は kemi の契約なので、描画器の出す属性には
//! 頼らず、構文木の `Span` から kemi が求める。

mod inline;
mod lines;
mod markdown;
mod overlay;
mod url;

#[cfg(test)]
mod tests;

pub use crate::domain::review::Side;

/// ブロックに付ける変更の印。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Unchanged,
    Added,
    Deleted,
    /// 旧と新を 1 つに重ねたブロック。消した語と足した語の印を中に持つ。
    Modified,
}

impl Mark {
    pub fn as_str(self) -> &'static str {
        match self {
            Mark::Unchanged => "unchanged",
            Mark::Added => "added",
            Mark::Deleted => "deleted",
            Mark::Modified => "modified",
        }
    }
}

/// 相対パス画像の参照。URL にするのは配信側。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageRef {
    /// その側のレビュー対象ファイルに一致した画像。
    Review { file_id: String, side: Side },
    /// レビュー対象に無く、リポジトリから読む画像（正規化したパス）。
    Repo { path: String, side: Side },
}

/// 描画した 1 ブロック。`data-kemi-block` の値と同じ側と行レンジを持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub side: Side,
    pub start: u32,
    pub end: u32,
    pub mark: Mark,
    pub images: Vec<ImageRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    pub blocks: Vec<Block>,
}

/// 各側のレビュー対象ファイル（id とパス）。相対パス画像がレビュー対象に一致するかを
/// 描画の時点で決めるために受け取る。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewPaths {
    pub old: Vec<(String, String)>,
    pub new: Vec<(String, String)>,
}

pub struct MarkdownInput<'a> {
    pub old: Option<&'a str>,
    pub new: Option<&'a str>,
    /// 相対パス画像の基準になる、その側の Markdown のパス。
    pub old_path: Option<&'a str>,
    pub new_path: Option<&'a str>,
    pub review_paths: ReviewPaths,
    /// リポジトリを読める入力か（git のモードなら真、manifest と復元なら偽）。
    pub repo_readable: bool,
}

/// コード塊のハイライト。言語名と本文を受けて、行ごとの HTML を返す。
pub type Highlight<'h> = &'h dyn Fn(&str, &str) -> Option<Vec<String>>;

/// 相対パス画像の参照を `src` の URL にする。
pub type ImageUrl<'u> = &'u dyn Fn(&ImageRef) -> String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unrenderable {
    /// 片側が行数かバイト数の上限を超える。
    TooLarge,
    Failed(String),
}

impl std::fmt::Display for Unrenderable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unrenderable::TooLarge => formatter.write_str("the file exceeds the rendering limit"),
            Unrenderable::Failed(message) => write!(formatter, "cannot render: {message}"),
        }
    }
}

pub fn render_markdown(
    input: &MarkdownInput<'_>,
    highlight: Option<Highlight<'_>>,
    image_url: ImageUrl<'_>,
) -> Result<Rendered, Unrenderable> {
    markdown::render(input, highlight, image_url)
}
