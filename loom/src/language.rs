//! Language detection for projects.

use std::fmt;
use std::path::Path;

/// Detected programming language in a project
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DetectedLanguage {
    Rust,
    TypeScript,
    Python,
    Go,
}

impl fmt::Display for DetectedLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetectedLanguage::Rust => write!(f, "Rust"),
            DetectedLanguage::TypeScript => write!(f, "TypeScript"),
            DetectedLanguage::Python => write!(f, "Python"),
            DetectedLanguage::Go => write!(f, "Go"),
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
            DetectedLanguage::Python => "python",
            DetectedLanguage::Go => "golang",
        }
    }

    /// Return the canonical identifier for this language used in image tags
    /// and fingerprint prefixes.
    ///
    /// Distinct from `skill_name()`: Go returns `"go"` here (not `"golang"`)
    /// so image tags stay compact and match Docker convention.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            DetectedLanguage::Rust => "rust",
            DetectedLanguage::TypeScript => "typescript",
            DetectedLanguage::Python => "python",
            DetectedLanguage::Go => "go",
        }
    }
}

/// Detect programming languages used in a project
///
/// Returns a Vec of detected languages based on manifest files:
/// - Rust: Cargo.toml
/// - TypeScript: tsconfig.json or package.json
/// - Python: pyproject.toml or requirements.txt
/// - Go: go.mod
///
/// Returns empty Vec if no languages detected.
pub fn detect_project_languages(root: &Path) -> Vec<DetectedLanguage> {
    let mut languages = Vec::new();

    // Check for Rust
    if root.join("Cargo.toml").exists() {
        languages.push(DetectedLanguage::Rust);
    }

    // Check for TypeScript (via tsconfig.json or package.json)
    if root.join("tsconfig.json").exists() || root.join("package.json").exists() {
        languages.push(DetectedLanguage::TypeScript);
    }

    // Check for Python
    if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        languages.push(DetectedLanguage::Python);
    }

    // Check for Go
    if root.join("go.mod").exists() {
        languages.push(DetectedLanguage::Go);
    }

    languages
}

/// Detect programming languages from a list of file paths or glob patterns.
///
/// Inspects each entry's file extension — handling globs like `src/**/*.rs`,
/// `frontend/**/*.tsx`, or bare `*.py` by matching on the trailing extension.
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
/// Returns `None` for paths with no recognized extension (directories,
/// `Makefile`, dotfiles like `.gitignore`, or unknown extensions).
fn language_for_path(path: &str) -> Option<DetectedLanguage> {
    // Isolate the filename component so a dot in a directory name
    // (e.g. `my.dir/Makefile`) is never mistaken for an extension.
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "rs" => Some(DetectedLanguage::Rust),
        "ts" | "tsx" | "mts" | "cts" => Some(DetectedLanguage::TypeScript),
        "py" | "pyi" => Some(DetectedLanguage::Python),
        "go" => Some(DetectedLanguage::Go),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::Rust));
    }

    #[test]
    fn test_detect_typescript() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("tsconfig.json"), "{}").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::TypeScript));
    }

    #[test]
    fn test_detect_typescript_via_package_json() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::TypeScript));
    }

    #[test]
    fn test_detect_python_via_pyproject() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pyproject.toml"), "[tool.poetry]").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::Python));
    }

    #[test]
    fn test_detect_python_via_requirements() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("requirements.txt"), "requests==2.28.0").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::Python));
    }

    #[test]
    fn test_detect_go() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("go.mod"), "module example.com/myapp").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 1);
        assert!(languages.contains(&DetectedLanguage::Go));
    }

    #[test]
    fn test_detect_multiple() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let languages = detect_project_languages(temp.path());

        assert_eq!(languages.len(), 2);
        assert!(languages.contains(&DetectedLanguage::Rust));
        assert!(languages.contains(&DetectedLanguage::TypeScript));
    }

    #[test]
    fn test_detect_none() {
        let temp = TempDir::new().unwrap();
        // Empty directory

        let languages = detect_project_languages(temp.path());

        assert!(languages.is_empty());
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", DetectedLanguage::Rust), "Rust");
        assert_eq!(format!("{}", DetectedLanguage::TypeScript), "TypeScript");
        assert_eq!(format!("{}", DetectedLanguage::Python), "Python");
        assert_eq!(format!("{}", DetectedLanguage::Go), "Go");
    }

    #[test]
    fn test_skill_name() {
        assert_eq!(DetectedLanguage::Rust.skill_name(), "rust");
        assert_eq!(DetectedLanguage::TypeScript.skill_name(), "typescript");
        assert_eq!(DetectedLanguage::Python.skill_name(), "python");
        assert_eq!(DetectedLanguage::Go.skill_name(), "golang");
    }

    #[test]
    fn test_canonical_name() {
        assert_eq!(DetectedLanguage::Rust.canonical_name(), "rust");
        assert_eq!(DetectedLanguage::TypeScript.canonical_name(), "typescript");
        assert_eq!(DetectedLanguage::Python.canonical_name(), "python");
        // Critical: Go must return "go", NOT "golang" (finding #18).
        assert_eq!(DetectedLanguage::Go.canonical_name(), "go");
    }

    #[test]
    fn test_detect_from_files_globs() {
        let files = vec![
            "loom/src/**/*.rs".to_string(),
            "frontend/**/*.tsx".to_string(),
        ];
        let langs = detect_languages_from_files(&files);
        assert_eq!(
            langs,
            vec![DetectedLanguage::Rust, DetectedLanguage::TypeScript]
        );
    }

    #[test]
    fn test_detect_from_files_extensions() {
        assert_eq!(
            detect_languages_from_files(&["a.rs".to_string()]),
            vec![DetectedLanguage::Rust]
        );
        // All TypeScript extension variants resolve.
        for ext in ["ts", "tsx", "mts", "cts"] {
            assert_eq!(
                detect_languages_from_files(&[format!("a.{ext}")]),
                vec![DetectedLanguage::TypeScript],
                "extension .{ext} should map to TypeScript"
            );
        }
        assert_eq!(
            detect_languages_from_files(&["pkg/main.go".to_string()]),
            vec![DetectedLanguage::Go]
        );
        assert_eq!(
            detect_languages_from_files(&["app/models.py".to_string()]),
            vec![DetectedLanguage::Python]
        );
    }

    #[test]
    fn test_detect_from_files_dedup_preserves_order() {
        let files = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "scripts/x.py".to_string(),
        ];
        let langs = detect_languages_from_files(&files);
        assert_eq!(
            langs,
            vec![DetectedLanguage::Rust, DetectedLanguage::Python]
        );
    }

    #[test]
    fn test_detect_from_files_ignores_unknown_and_extensionless() {
        let files = vec![
            "Makefile".to_string(),
            ".gitignore".to_string(),
            "docs/readme.md".to_string(),
            "my.dir/Makefile".to_string(),
            "src/".to_string(),
        ];
        assert!(detect_languages_from_files(&files).is_empty());
    }
}
