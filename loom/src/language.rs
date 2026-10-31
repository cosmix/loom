//! Language detection for projects.

use std::fmt;
use std::path::Path;

use crate::context::extract::dialect::dialect_for_path;
use crate::context::source_graph::NodeLanguage;

pub(crate) mod evidence;
#[cfg(test)]
mod evidence_tests;

/// Detected programming language in a project
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DetectedLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    CSharp,
    Ruby,
    Php,
    C,
    Cpp,
}

impl fmt::Display for DetectedLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetectedLanguage::Rust => write!(f, "Rust"),
            DetectedLanguage::TypeScript => write!(f, "TypeScript"),
            DetectedLanguage::JavaScript => write!(f, "JavaScript"),
            DetectedLanguage::Python => write!(f, "Python"),
            DetectedLanguage::Go => write!(f, "Go"),
            DetectedLanguage::Java => write!(f, "Java"),
            DetectedLanguage::CSharp => write!(f, "C#"),
            DetectedLanguage::Ruby => write!(f, "Ruby"),
            DetectedLanguage::Php => write!(f, "PHP"),
            DetectedLanguage::C => write!(f, "C"),
            DetectedLanguage::Cpp => write!(f, "C++"),
        }
    }
}

impl DetectedLanguage {
    /// Return the skill name for this language.
    ///
    /// This is the name used to look up skills in the skill index
    /// (e.g., the directory name under ~/.claude/skills/).
    /// Decoupled from Display to avoid breakage if display names diverge.
    pub fn skill_name(&self) -> &'static str {
        match self {
            DetectedLanguage::Rust => "rust",
            DetectedLanguage::TypeScript => "typescript",
            DetectedLanguage::JavaScript => "javascript",
            DetectedLanguage::Python => "python",
            DetectedLanguage::Go => "golang",
            DetectedLanguage::Java => "java",
            DetectedLanguage::CSharp => "csharp",
            DetectedLanguage::Ruby => "ruby",
            DetectedLanguage::Php => "php",
            DetectedLanguage::C => "c",
            DetectedLanguage::Cpp => "cpp",
        }
    }

    /// Package-registry hosts suggested for the sandbox network allowlist.
    ///
    /// Empty for C and C++, which have no single canonical registry.
    pub fn registry_domains(&self) -> &'static [&'static str] {
        match self {
            DetectedLanguage::Rust => &["crates.io", "static.crates.io"],
            DetectedLanguage::TypeScript | DetectedLanguage::JavaScript => &["registry.npmjs.org"],
            DetectedLanguage::Python => &["pypi.org"],
            DetectedLanguage::Go => &["proxy.golang.org"],
            DetectedLanguage::Java => &[
                "repo.maven.apache.org",
                "plugins.gradle.org",
                "services.gradle.org",
            ],
            DetectedLanguage::CSharp => &["api.nuget.org"],
            DetectedLanguage::Ruby => &["rubygems.org"],
            DetectedLanguage::Php => &["repo.packagist.org"],
            DetectedLanguage::C | DetectedLanguage::Cpp => &[],
        }
    }
}

/// Detect programming languages used in a project
///
/// Projects detected in this directory use the shared manifest evidence rules.
/// Kotlin, Scala, Swift, Elixir and Dart have no `DetectedLanguage` projection.
///
/// C has no manifest: nothing distinguishes a C build from a C++ build, so C
/// is detected only from file extensions (see [`detect_languages_from_files`]).
///
/// Returns empty Vec if no languages detected.
pub fn detect_project_languages(root: &Path) -> Vec<DetectedLanguage> {
    use evidence::LanguageEvidence as E;
    let found = evidence::detect(root);
    [
        (E::Rust, DetectedLanguage::Rust),
        (E::TypeScript, DetectedLanguage::TypeScript),
        (E::JavaScript, DetectedLanguage::JavaScript),
        (E::Python, DetectedLanguage::Python),
        (E::Go, DetectedLanguage::Go),
        (E::Java, DetectedLanguage::Java),
        (E::CSharp, DetectedLanguage::CSharp),
        (E::Ruby, DetectedLanguage::Ruby),
        (E::Php, DetectedLanguage::Php),
        (E::Cpp, DetectedLanguage::Cpp),
    ]
    .into_iter()
    .filter_map(|(evidence, language)| found.contains(&evidence).then_some(language))
    .collect()
}

/// Detect programming languages from a list of file paths or glob patterns.
///
/// Inspects each entry's file extension — handling globs like `src/**/*.rs`,
/// `frontend/**/*.tsx`, or bare `*.py` by matching on the trailing extension.
/// The extension-to-language mapping comes from the source graph's `DIALECTS`
/// registry, so there is one extension table.
/// Returns the distinct languages in first-seen order.
///
/// Unlike [`detect_project_languages`] (which inspects manifest files at a single
/// root), this looks at the specific files a stage will edit. That makes it work
/// for monorepos and subdirectory layouts: a stage editing `frontend/**/*.tsx`
/// resolves to TypeScript even when the repo root has a `Cargo.toml`.
pub fn detect_languages_from_files(files: &[String]) -> Vec<DetectedLanguage> {
    let mut languages = Vec::new();
    for file in files {
        if let Some(lang) = language_for_path(file) {
            if !languages.contains(&lang) {
                languages.push(lang);
            }
        }
    }
    languages
}

/// Map a single file path or glob to a language by its extension.
///
/// The mapping comes from `DIALECTS` via `dialect_for_path`. Returns `None`
/// for paths with no recognized extension (directories, `Makefile`, dotfiles
/// like `.gitignore`, or unknown extensions).
fn language_for_path(path: &str) -> Option<DetectedLanguage> {
    // Isolate the filename component so a dot in a directory name
    // (e.g. `my.dir/Makefile`) is never mistaken for an extension.
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let dialect = dialect_for_path(Path::new(file))?;
    match &dialect.language {
        NodeLanguage::Rust => Some(DetectedLanguage::Rust),
        NodeLanguage::TypeScript | NodeLanguage::Tsx => Some(DetectedLanguage::TypeScript),
        NodeLanguage::JavaScript => Some(DetectedLanguage::JavaScript),
        NodeLanguage::Python => Some(DetectedLanguage::Python),
        NodeLanguage::Go => Some(DetectedLanguage::Go),
        NodeLanguage::Java => Some(DetectedLanguage::Java),
        NodeLanguage::CSharp => Some(DetectedLanguage::CSharp),
        NodeLanguage::Ruby => Some(DetectedLanguage::Ruby),
        NodeLanguage::Php => Some(DetectedLanguage::Php),
        NodeLanguage::C => Some(DetectedLanguage::C),
        NodeLanguage::Cpp => Some(DetectedLanguage::Cpp),
        NodeLanguage::Other(_) => None,
    }
}

#[cfg(test)]
mod tests;
