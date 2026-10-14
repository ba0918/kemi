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
    GeneratedAttributes::parse(attributes).matches(path)
}

pub(crate) struct GeneratedAttributes<'a> {
    rules: Vec<(&'a str, bool)>,
}

impl<'a> GeneratedAttributes<'a> {
    pub(crate) fn parse(attributes: &'a str) -> Self {
        let rules = attributes
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let mut parts = line.split_whitespace();
                let pattern = parts.next()?;
                Some((pattern, parts.filter_map(parse_attribute).next_back()?))
            })
            .collect();
        Self { rules }
    }

    pub(crate) fn matches(&self, path: &str) -> bool {
        self.rules
            .iter()
            .rev()
            .find(|(pattern, _)| match_pattern(pattern, path))
            .is_some_and(|(_, value)| *value)
    }
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
    let mut matched = vec![false; segments.len() + 1];
    matched[0] = true;
    for &pattern in patterns {
        if pattern == "**" {
            for end in 1..matched.len() {
                matched[end] |= matched[end - 1];
            }
        } else {
            for end in (1..matched.len()).rev() {
                matched[end] = matched[end - 1] && match_segment(pattern, segments[end - 1]);
            }
            matched[0] = false;
        }
    }
    matched[segments.len()]
}

fn match_segment(pattern: &str, text: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    // 再帰で * の長さを総当たりすると、失敗するパターンで同じ接尾辞を何度も調べる。
    let mut matched = vec![false; text.len() + 1];
    matched[0] = true;
    for token in pattern.chars() {
        if token == '*' {
            for end in 1..matched.len() {
                matched[end] |= matched[end - 1];
            }
        } else {
            for end in (1..matched.len()).rev() {
                matched[end] = matched[end - 1] && (token == '?' || token == text[end - 1]);
            }
            matched[0] = false;
        }
    }
    matched[text.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_attribute_rules_keep_the_last_matching_value_and_ignore_unrelated_attributes() {
        let attributes = "# *.rs linguist-generated\n*.rs diff linguist-generated -linguist-generated linguist-generated=true\nsrc/* eol=lf\nsrc/*.rs -linguist-generated\nsrc/keep.rs linguist-generated=false linguist-generated\n*.txt linguist-generated=unknown\n";
        let rules = GeneratedAttributes::parse(attributes);
        for (path, expected) in [
            ("lib/a.rs", true),
            ("src/a.rs", false),
            ("src/keep.rs", true),
            ("a.txt", false),
        ] {
            assert_eq!(rules.matches(path), expected, "{path}");
        }
        assert!(!GeneratedAttributes::parse("\n# comment\n* text\n").matches("a.rs"));
    }

    #[test]
    fn wildcard_matching_keeps_unicode_empty_segments_and_directory_semantics() {
        for (pattern, path, expected) in [
            ("", "", true),
            ("*", "", true),
            ("?", "", false),
            ("?", "日", true),
            ("*本?", "日本語", true),
            ("a**b", "ab", true),
            ("a**b", "axxb", true),
            ("/a.rs", "src/a.rs", false),
            ("a.rs", "src/a.rs", true),
            ("generated/", "generated/a/b.rs", true),
            ("**/a/**/b", "a/b", true),
            ("**/a/**/b", "x/a/y/z/b", true),
            ("**/**/missing", "a/b/c", false),
            ("a/*/b", "a//b", true),
            ("a/*/b", "a/x/y/b", false),
            ("*a*a*a*a*a*a*a*ab", "aaaaaaaaaaaaaaaa", false),
        ] {
            assert_eq!(
                match_pattern(pattern, path),
                expected,
                "{pattern} -> {path}"
            );
        }
    }

    #[test]
    fn wildcard_matching_agrees_with_short_exhaustive_reference_cases() {
        fn reference(pattern: &[char], text: &[char]) -> bool {
            match pattern.first() {
                None => text.is_empty(),
                Some('*') => (0..=text.len()).any(|skip| reference(&pattern[1..], &text[skip..])),
                Some('?') => !text.is_empty() && reference(&pattern[1..], &text[1..]),
                Some(&literal) => {
                    text.first() == Some(&literal) && reference(&pattern[1..], &text[1..])
                }
            }
        }
        fn strings(alphabet: &[char], max: usize) -> Vec<String> {
            let mut all = vec![String::new()];
            let mut level = vec![String::new()];
            for _ in 0..max {
                level = level
                    .iter()
                    .flat_map(|prefix| alphabet.iter().map(move |c| format!("{prefix}{c}")))
                    .collect();
                all.extend(level.clone());
            }
            all
        }
        for pattern in strings(&['a', '日', '*', '?'], 4) {
            for text in strings(&['a', '日'], 3) {
                assert_eq!(
                    match_segment(&pattern, &text),
                    reference(
                        &pattern.chars().collect::<Vec<_>>(),
                        &text.chars().collect::<Vec<_>>()
                    ),
                    "{pattern} -> {text}"
                );
            }
        }
    }

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
