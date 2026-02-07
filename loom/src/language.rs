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
}
