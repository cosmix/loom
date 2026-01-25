//! Detection functions for codebase analysis.

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Detect the project type based on manifest files
pub fn detect_project_type(root: &Path) -> Result<String> {
    let mut types = Vec::new();

    // Check for Rust
    if root.join("Cargo.toml").exists() {
        types.push("- **Rust** (Cargo.toml found)");
    }

    // Check for Node.js
    if root.join("package.json").exists() {
        types.push("- **Node.js** (package.json found)");
    }

    // Check for Go
    if root.join("go.mod").exists() {
        types.push("- **Go** (go.mod found)");
    }

    // Check for Python
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        types.push("- **Python** (pyproject.toml or setup.py found)");
    }

    // Check for Ruby
    if root.join("Gemfile").exists() {
        types.push("- **Ruby** (Gemfile found)");
    }

    if types.is_empty() {
        return Ok(String::new());
    }

    Ok(types.join("\n"))
}

/// Analyze dependencies from manifest files
pub fn analyze_dependencies(root: &Path) -> Result<String> {
    let mut deps = Vec::new();

    // Parse Cargo.toml dependencies
    let cargo_path = root.join("Cargo.toml");
    if cargo_path.exists() {
        if let Ok(content) = fs::read_to_string(&cargo_path) {
            deps.push("### Rust Dependencies (from Cargo.toml)\n".to_string());
            let mut in_deps = false;
            for line in content.lines() {
                if line.starts_with("[dependencies]") || line.starts_with("[dev-dependencies]") {
                    in_deps = true;
                    continue;
                }
                if line.starts_with('[') {
                    in_deps = false;
                    continue;
                }
                if in_deps && !line.trim().is_empty() && line.contains('=') {
                    // Extract just the package name
                    if let Some(name) = line.split('=').next() {
                        let name = name.trim();
                        if !name.is_empty() {
                            deps.push(format!("- {name}"));
                        }
                    }
                }
            }
        }
    }

    // Parse package.json dependencies
    let pkg_path = root.join("package.json");
    if pkg_path.exists() {
        if let Ok(content) = fs::read_to_string(&pkg_path) {
            deps.push("### Node.js Dependencies (from package.json)\n".to_string());
            // Simple JSON parsing for dependencies
            if content.contains("\"dependencies\"") {
                deps.push("(dependencies section found)".to_string());
            }
        }
    }

    if deps.is_empty() {
        return Ok(String::new());
    }

    Ok(deps.join("\n"))
}

/// Find entry points in the codebase
pub fn find_entry_points(root: &Path, focus: Option<&str>) -> Result<String> {
    let mut entries = Vec::new();

    // Common entry point patterns
    let patterns = [
        ("src/main.rs", "Rust CLI entry point"),
        ("src/lib.rs", "Rust library entry point"),
        ("src/index.ts", "TypeScript entry point"),
        ("src/index.js", "JavaScript entry point"),
        ("index.ts", "TypeScript entry point"),
        ("index.js", "JavaScript entry point"),
        ("main.go", "Go entry point"),
        ("cmd/", "Go command directory"),
        ("app.py", "Python application"),
        ("main.py", "Python entry point"),
        ("src/App.tsx", "React application"),
        ("src/App.jsx", "React application"),
    ];

    for (pattern, desc) in patterns {
        let path = root.join(pattern);
        if path.exists() {
            // If focus is provided, only include matching entries
            if let Some(f) = focus {
                if !pattern.contains(f) && !desc.to_lowercase().contains(&f.to_lowercase()) {
                    continue;
                }
            }
            entries.push(format!("- `{pattern}` - {desc}"));
        }
    }

    if entries.is_empty() {
        return Ok(String::new());
    }

    Ok(entries.join("\n"))
}

/// Analyze directory structure
pub fn analyze_structure(root: &Path, deep: bool) -> Result<String> {
    let mut structure = Vec::new();
    let max_depth = if deep { 3 } else { 2 };

    structure.push("```".to_string());
    analyze_dir_recursive(root, root, 0, max_depth, &mut structure)?;
    structure.push("```".to_string());

    Ok(structure.join("\n"))
}

fn analyze_dir_recursive(
    _base: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    output: &mut Vec<String>,
) -> Result<()> {
    if depth > max_depth {
        return Ok(());
    }

    let indent = "  ".repeat(depth);

    // Skip hidden dirs and common non-essential directories
    let skip_dirs = [
        ".git",
        ".work",
        ".worktrees",
        "node_modules",
        "target",
        ".venv",
        "__pycache__",
    ];

    let mut entries: Vec<_> = fs::read_dir(current)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            !skip_dirs.contains(&name.as_str()) && !name.starts_with('.')
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries.iter().take(15) {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();

        if path.is_dir() {
            output.push(format!("{indent}{name}/"));
            analyze_dir_recursive(_base, &path, depth + 1, max_depth, output)?;
        } else if depth == 0 {
            // Only show root-level files
            output.push(format!("{indent}{name}"));
        }
    }

    if entries.len() > 15 {
        let remaining = entries.len() - 15;
        output.push(format!("{indent}... ({remaining} more)"));
    }

    Ok(())
}

/// Detect coding conventions
pub fn detect_conventions(root: &Path) -> Result<String> {
    let mut conventions = Vec::new();

    // Check for configuration files that indicate conventions
    let configs = [
        (".rustfmt.toml", "Rust formatting configured"),
        ("rustfmt.toml", "Rust formatting configured"),
        (".prettierrc", "Prettier formatting configured"),
        (".eslintrc", "ESLint linting configured"),
        (".eslintrc.js", "ESLint linting configured"),
        (".eslintrc.json", "ESLint linting configured"),
        ("tsconfig.json", "TypeScript configured"),
        (".editorconfig", "EditorConfig configured"),
    ];

    for (file, desc) in configs {
        if root.join(file).exists() {
            conventions.push(format!("- {desc}"));
        }
    }

    // Check for test directories
    let test_patterns = [
        ("tests/", "Tests in tests/ directory"),
        ("test/", "Tests in test/ directory"),
        ("__tests__/", "Jest-style tests in __tests__/"),
        ("spec/", "Spec-style tests in spec/"),
    ];

    for (pattern, desc) in test_patterns {
        if root.join(pattern).exists() {
            conventions.push(format!("- {desc}"));
            break; // Only report one test location
        }
    }

    if conventions.is_empty() {
        return Ok(String::new());
    }

    Ok(conventions.join("\n"))
}

/// Find potential concerns (tech debt, TODOs, etc.)
pub fn find_concerns(root: &Path) -> Result<String> {
    let mut concerns = Vec::new();

    // Count TODOs and FIXMEs in source files
    let todo_count = count_pattern_in_source(root, "TODO")?;
    let fixme_count = count_pattern_in_source(root, "FIXME")?;

    if todo_count > 0 {
        concerns.push(format!("- **{todo_count} TODO comments** found in source files"));
    }
    if fixme_count > 0 {
        concerns.push(format!("- **{fixme_count} FIXME comments** found in source files"));
    }

    // Check for common security concerns
    let security_files = [".env", ".env.local", "secrets.json", "credentials.json"];
    for file in security_files {
        if root.join(file).exists() {
            concerns.push(format!("- **Warning**: `{file}` file present (ensure not committed)"));
        }
    }

    if concerns.is_empty() {
        return Ok(String::new());
    }

    Ok(concerns.join("\n"))
}

fn count_pattern_in_source(root: &Path, pattern: &str) -> Result<usize> {
    let mut count = 0;
    let extensions = ["rs", "ts", "js", "py", "go", "java", "rb"];

    fn walk_dir(dir: &Path, pattern: &str, extensions: &[&str], count: &mut usize) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let skip_dirs = [
            ".git",
            "node_modules",
            "target",
            ".venv",
            "__pycache__",
            ".work",
            ".worktrees",
        ];

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
                    walk_dir(&path, pattern, extensions, count)?;
                }
            } else if let Some(ext) = path.extension() {
                if extensions.contains(&ext.to_string_lossy().as_ref()) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        *count += content.matches(pattern).count();
                    }
                }
            }
        }
        Ok(())
    }

    walk_dir(root, pattern, &extensions, &mut count)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();

        let result = detect_project_type(temp.path()).unwrap();
        assert!(result.contains("Rust"));
    }

    #[test]
    fn test_detect_node_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let result = detect_project_type(temp.path()).unwrap();
        assert!(result.contains("Node.js"));
    }

    #[test]
    fn test_find_entry_points() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();

        let result = find_entry_points(temp.path(), None).unwrap();
        assert!(result.contains("main.rs"));
    }

    #[test]
    fn test_count_todos() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "// TODO: fix this\n// TODO: and this").unwrap();

        let count = count_pattern_in_source(temp.path(), "TODO").unwrap();
        assert_eq!(count, 2);
    }
}
