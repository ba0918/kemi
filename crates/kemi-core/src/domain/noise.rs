//! ノイズ分類器（R-VIEW, D6）。分類はこの 1 箇所だけで行う。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseReason {
    Lockfile,
    Minified,
    Vendored,
    Generated,
    LinguistGenerated,
    Binary,
}

pub struct NoiseInput<'a> {
    pub path: &'a str,
    /// `.gitattributes` の `linguist-generated` の評価結果。
    pub linguist_generated: bool,
    pub binary: bool,
}

const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "pnpm-lock.yml",
    "Gemfile.lock",
    "poetry.lock",
    "uv.lock",
    "composer.lock",
    "go.sum",
    "flake.lock",
    "packages.lock.json",
    "bun.lock",
    "bun.lockb",
    "gradle.lockfile",
    "mix.lock",
    "pubspec.lock",
    "Pipfile.lock",
    "Package.resolved",
];

const VENDORED_SEGMENTS: &[&str] = &["vendor", "node_modules", "third_party", "thirdparty"];

const GENERATED_SEGMENTS: &[&str] = &[
    "dist",
    "target",
    "generated",
    ".next",
    "coverage",
    "__generated__",
];

const GENERATED_SUFFIXES: &[&str] = &[
    ".generated.rs",
    ".generated.ts",
    ".generated.js",
    ".pb.go",
    ".pb.rs",
    "_pb2.py",
    ".g.dart",
    ".designer.cs",
];

pub fn classify(input: &NoiseInput) -> Option<NoiseReason> {
    if input.binary {
        return Some(NoiseReason::Binary);
    }
    if input.linguist_generated {
        return Some(NoiseReason::LinguistGenerated);
    }

    let name = input.path.rsplit('/').next().unwrap_or(input.path);
    if LOCKFILES.contains(&name) {
        return Some(NoiseReason::Lockfile);
    }
    if name.ends_with(".min.js") || name.ends_with(".min.css") {
        return Some(NoiseReason::Minified);
    }
    for segment in input.path.split('/') {
        if VENDORED_SEGMENTS.contains(&segment) {
            return Some(NoiseReason::Vendored);
        }
    }
    for segment in input.path.split('/') {
        if GENERATED_SEGMENTS.contains(&segment) {
            return Some(NoiseReason::Generated);
        }
    }
    if GENERATED_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        return Some(NoiseReason::Generated);
    }

    None
}

/// `.gitattributes` の内容を先頭から評価し、`linguist-generated` の最後の一致を返す。
pub fn linguist_generated(attributes: &str, path: &str) -> bool {
    let mut generated = false;
    for line in attributes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(pattern) = parts.next() else {
            continue;
        };
        for attribute in parts {
            let Some(value) = parse_attribute(attribute) else {
                continue;
            };
            if match_pattern(pattern, path) {
                generated = value;
            }
        }
    }
    generated
}

fn parse_attribute(attribute: &str) -> Option<bool> {
    match attribute {
        "linguist-generated" | "linguist-generated=true" => Some(true),
        "-linguist-generated" | "linguist-generated=false" => Some(false),
        _ => None,
    }
}

/// gitattributes 風のパターン照合。`*` は `/` を跨がず、`**` は跨ぐ。
/// `/` を含まないパターンはベース名に一致する。
fn match_pattern(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_prefix('/') {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };
    let directory_pattern;
    let pattern = if let Some(rest) = pattern.strip_suffix('/') {
        directory_pattern = format!("{rest}/**");
        directory_pattern.as_str()
    } else {
        pattern
    };

    if !anchored && !pattern.contains('/') {
        let name = path.rsplit('/').next().unwrap_or(path);
        return match_segment(pattern, name);
    }

    let patterns: Vec<&str> = pattern.split('/').collect();
    let segments: Vec<&str> = path.split('/').collect();
    match_segments(&patterns, &segments)
}

fn match_segments(patterns: &[&str], segments: &[&str]) -> bool {
    match patterns.first() {
        None => segments.is_empty(),
        Some(&"**") => {
            match_segments(&patterns[1..], segments)
                || (!segments.is_empty() && match_segments(patterns, &segments[1..]))
        }
        Some(&pattern) => match segments.split_first() {
            Some((segment, rest)) => {
                match_segment(pattern, segment) && match_segments(&patterns[1..], rest)
            }
            None => false,
        },
    }
}

fn match_segment(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    match_segment_inner(&pattern, &text)
}

fn match_segment_inner(pattern: &[char], text: &[char]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some('*') => (0..=text.len()).any(|skip| match_segment_inner(&pattern[1..], &text[skip..])),
        Some('?') => !text.is_empty() && match_segment_inner(&pattern[1..], &text[1..]),
        Some(&literal) => {
            text.first() == Some(&literal) && match_segment_inner(&pattern[1..], &text[1..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(path: &'a str) -> NoiseInput<'a> {
        NoiseInput {
            path,
            linguist_generated: false,
            binary: false,
        }
    }

    #[test]
    fn noise_lockfile_is_noise() {
        assert_eq!(classify(&input("Cargo.lock")), Some(NoiseReason::Lockfile));
        assert_eq!(
            classify(&input("front/package-lock.json")),
            Some(NoiseReason::Lockfile)
        );
    }

    #[test]
    fn noise_lockfile_match_is_basename_exact() {
        assert_eq!(classify(&input("docs/Cargo.lock.md")), None);
        assert_eq!(classify(&input("src/go.sum.rs")), None);
    }

    #[test]
    fn noise_minified_file_is_noise() {
        assert_eq!(
            classify(&input("web/app.min.js")),
            Some(NoiseReason::Minified)
        );
        assert_eq!(
            classify(&input("web/theme.min.css")),
            Some(NoiseReason::Minified)
        );
    }

    #[test]
    fn noise_vendored_path_is_noise() {
        assert_eq!(
            classify(&input("vendor/github.com/x/y.go")),
            Some(NoiseReason::Vendored)
        );
        assert_eq!(
            classify(&input("web/node_modules/x/index.js")),
            Some(NoiseReason::Vendored)
        );
    }

    #[test]
    fn noise_generated_path_is_noise() {
        assert_eq!(
            classify(&input("dist/bundle.js")),
            Some(NoiseReason::Generated)
        );
        assert_eq!(
            classify(&input("src/schema.generated.rs")),
            Some(NoiseReason::Generated)
        );
    }

    #[test]
    fn noise_binary_is_noise() {
        let binary = NoiseInput {
            path: "assets/logo.png",
            linguist_generated: false,
            binary: true,
        };
        assert_eq!(classify(&binary), Some(NoiseReason::Binary));
    }

    #[test]
    fn noise_linguist_generated_is_noise() {
        let generated = NoiseInput {
            path: "src/schema.ts",
            linguist_generated: true,
            binary: false,
        };
        assert_eq!(classify(&generated), Some(NoiseReason::LinguistGenerated));
    }

    #[test]
    fn noise_normal_source_is_not_noise() {
        for path in [
            "src/main.rs",
            "README.md",
            "tests/e2e.rs",
            "docs/spec/kemi.md",
        ] {
            assert_eq!(classify(&input(path)), None, "{path}");
        }
    }

    #[test]
    fn noise_gitattributes_marks_generated_paths() {
        assert!(linguist_generated("*.rs linguist-generated", "src/a.rs"));
        assert!(linguist_generated(
            "src/*.ts linguist-generated=true",
            "src/schema.ts"
        ));
    }

    #[test]
    fn noise_gitattributes_last_match_wins() {
        let attributes = "*.rs linguist-generated\nsrc/*.rs -linguist-generated\n";
        assert!(!linguist_generated(attributes, "src/a.rs"));
        assert!(linguist_generated(attributes, "lib/a.rs"));
    }

    #[test]
    fn noise_gitattributes_double_star_crosses_directories() {
        assert!(linguist_generated(
            "generated/** linguist-generated",
            "generated/a/b.rs"
        ));
        assert!(!linguist_generated(
            "generated/* linguist-generated",
            "generated/a/b.rs"
        ));
    }
}
