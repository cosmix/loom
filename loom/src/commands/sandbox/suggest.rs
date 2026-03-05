//! Suggest sandbox network domains based on project dependencies.

use anyhow::Result;
use std::env;
use std::path::Path;

use crate::language::{detect_project_languages, DetectedLanguage};

/// Detect project type and suggest sandbox network domains
pub fn execute() -> Result<()> {
    let current_dir = env::current_dir()?;
    let (suggestions, detected_languages) = detect_project_and_suggest(&current_dir)?;

    let excluded_commands = suggest_excluded_commands(&detected_languages);

    // Print YAML snippet that users can copy to their plan
    println!("# Suggested sandbox configuration for your plan");
    println!("sandbox:");

    if !excluded_commands.is_empty() {
        println!("  excluded_commands:");
        for cmd in &excluded_commands {
            println!("    - \"{}\"", cmd);
        }
    }

    println!("  network:");
    println!("    allowed_domains:");

    if suggestions.is_empty() {
        println!("      # No project-specific dependencies detected");
        println!("      # Add domains manually as needed");
    } else {
        for domain in &suggestions {
            println!("      - \"{}\"", domain);
        }
    }

    // Print explanatory text
    if !detected_languages.is_empty() {
        println!();
        println!("# Detected project types:");
        for lang in &detected_languages {
            match lang {
                DetectedLanguage::Rust => {
                    println!("#   - Rust (Cargo.toml found)");
                }
                DetectedLanguage::TypeScript => {
                    println!("#   - TypeScript/Node.js (package.json or tsconfig.json found)");
                }
                DetectedLanguage::Python => {
                    println!("#   - Python (pyproject.toml or requirements.txt found)");
                }
                DetectedLanguage::Go => {
                    println!("#   - Go (go.mod found)");
                }
            }
        }
        if !suggestions.is_empty() {
            println!("#   - Git repository");
        }
    }

    Ok(())
}

/// Suggest build tool commands to exclude from OS sandbox based on detected languages
fn suggest_excluded_commands(detected_languages: &[DetectedLanguage]) -> Vec<String> {
    let mut commands = vec!["loom".to_string(), "git".to_string()];

    for lang in detected_languages {
        match lang {
            DetectedLanguage::Rust => {
                commands.push("cargo".to_string());
            }
            DetectedLanguage::TypeScript => {
                commands.push("bun".to_string());
                commands.push("npm".to_string());
                commands.push("npx".to_string());
            }
            DetectedLanguage::Python => {
                commands.push("uv".to_string());
                commands.push("pip".to_string());
                commands.push("python".to_string());
            }
            DetectedLanguage::Go => {
                commands.push("go".to_string());
            }
        }
    }

    commands
}

/// Detect project types and return suggested domains and detected languages
fn detect_project_and_suggest(project_root: &Path) -> Result<(Vec<String>, Vec<DetectedLanguage>)> {
    let mut domains = Vec::new();

    // Always add git domains (very common)
    domains.push("github.com".to_string());
    domains.push("api.github.com".to_string());
    domains.push("raw.githubusercontent.com".to_string());

    // Detect project languages using shared module
    let detected_languages = detect_project_languages(project_root);

    // Map each detected language to its domain list
    for lang in &detected_languages {
        match lang {
            DetectedLanguage::Rust => {
                domains.extend_from_slice(&[
                    "crates.io".to_string(),
                    "static.crates.io".to_string(),
                    "static.rust-lang.org".to_string(),
                    "doc.rust-lang.org".to_string(),
                ]);
            }
            DetectedLanguage::TypeScript => {
                domains.extend_from_slice(&[
                    "registry.npmjs.org".to_string(),
                    "npmjs.com".to_string(),
                ]);
            }
            DetectedLanguage::Python => {
                domains.extend_from_slice(&[
                    "pypi.org".to_string(),
                    "files.pythonhosted.org".to_string(),
                ]);
            }
            DetectedLanguage::Go => {
                domains.extend_from_slice(&[
                    "proxy.golang.org".to_string(),
                    "sum.golang.org".to_string(),
                ]);
            }
        }
    }

    Ok((domains, detected_languages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create Cargo.toml
        fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname = \"test\"",
        )
        .unwrap();

        let (domains, _languages) = detect_project_and_suggest(project_root).unwrap();

        assert!(domains.contains(&"crates.io".to_string()));
        assert!(domains.contains(&"static.crates.io".to_string()));
        assert!(domains.contains(&"github.com".to_string()));
    }

    #[test]
    fn test_detect_node_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create package.json
        fs::write(project_root.join("package.json"), "{}").unwrap();

        let (domains, _languages) = detect_project_and_suggest(project_root).unwrap();

        assert!(domains.contains(&"registry.npmjs.org".to_string()));
        assert!(domains.contains(&"npmjs.com".to_string()));
        assert!(domains.contains(&"github.com".to_string()));
    }

    #[test]
    fn test_detect_python_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create requirements.txt
        fs::write(project_root.join("requirements.txt"), "requests==2.28.0").unwrap();

        let (domains, _languages) = detect_project_and_suggest(project_root).unwrap();

        assert!(domains.contains(&"pypi.org".to_string()));
        assert!(domains.contains(&"files.pythonhosted.org".to_string()));
        assert!(domains.contains(&"github.com".to_string()));
    }

    #[test]
    fn test_detect_multiple_project_types() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create multiple project files
        fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname = \"test\"",
        )
        .unwrap();
        fs::write(project_root.join("package.json"), "{}").unwrap();

        let (domains, _languages) = detect_project_and_suggest(project_root).unwrap();

        // Should have Rust domains
        assert!(domains.contains(&"crates.io".to_string()));
        // Should have Node domains
        assert!(domains.contains(&"registry.npmjs.org".to_string()));
        // Should have Git domains
        assert!(domains.contains(&"github.com".to_string()));
    }

    #[test]
    fn test_no_project_files() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let (domains, _languages) = detect_project_and_suggest(project_root).unwrap();

        // Should only have Git domains (always included)
        assert!(domains.contains(&"github.com".to_string()));
        assert!(domains.contains(&"api.github.com".to_string()));
        assert!(domains.contains(&"raw.githubusercontent.com".to_string()));

        // Should not have project-specific domains
        assert!(!domains.contains(&"crates.io".to_string()));
        assert!(!domains.contains(&"npmjs.com".to_string()));
        assert!(!domains.contains(&"pypi.org".to_string()));
    }

    #[test]
    fn test_suggest_excluded_commands_rust() {
        let languages = vec![DetectedLanguage::Rust];
        let commands = suggest_excluded_commands(&languages);
        assert!(commands.contains(&"loom".to_string()));
        assert!(commands.contains(&"cargo".to_string()));
        assert!(!commands.contains(&"bun".to_string()));
    }

    #[test]
    fn test_suggest_excluded_commands_typescript() {
        let languages = vec![DetectedLanguage::TypeScript];
        let commands = suggest_excluded_commands(&languages);
        assert!(commands.contains(&"loom".to_string()));
        assert!(commands.contains(&"bun".to_string()));
        assert!(commands.contains(&"npm".to_string()));
    }

    #[test]
    fn test_suggest_excluded_commands_empty() {
        let commands = suggest_excluded_commands(&[]);
        assert_eq!(commands, vec!["loom", "git"]);
    }
}
