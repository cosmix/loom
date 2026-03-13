//! Duplicate symbol detection - identify top-level symbols in new or modified files
//! that share a name with symbols already present elsewhere in the codebase.
//!
//! This is a heuristic check; it intentionally trades precision for speed by using
//! simple regex patterns instead of a full language server. Common/generic names are
//! filtered out to reduce noise.

use anyhow::{bail, Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

/// A potential symbol duplication between a new/modified file and an existing file.
#[derive(Debug)]
pub struct DuplicateSymbol {
    /// The name of the symbol that appears to be duplicated
    pub symbol_name: String,
    /// Path (relative to worktree) of the new or modified file
    pub new_file: String,
    /// Line number within `new_file` where the symbol is defined
    pub new_line: usize,
    /// Path (relative to worktree) of the existing file where the same name is found
    pub existing_file: String,
    /// Line number within `existing_file` where the name is found
    pub existing_line: usize,
    /// Human-readable symbol kind (e.g. "function", "struct", "class")
    pub symbol_type: String,
}

/// Common / generic symbol names that would produce too many false positives.
const NOISE_NAMES: &[&str] = &[
    "new",
    "default",
    "from",
    "into",
    "build",
    "test",
    "main",
    "run",
    "init",
    "error",
    "config",
    "types",
    "utils",
    "helpers",
    "common",
    "get",
    "set",
    "create",
    "update",
    "delete",
    "list",
    "parse",
    "format",
    "write",
    "read",
    "open",
    "close",
    "handle",
    "process",
    "execute",
    "start",
    "stop",
    "reset",
    "clone",
    "copy",
    "add",
    "remove",
    "insert",
    "find",
    "check",
    "validate",
    "load",
    "save",
    "send",
    "receive",
    "connect",
    "disconnect",
];

/// Source file extensions that are eligible for duplicate detection.
const SOURCE_EXTENSIONS: &[&str] = &["rs", "ts", "tsx", "jsx", "js", "py", "go"];

/// A single extracted symbol with its definition location.
#[derive(Debug)]
struct SymbolDef {
    name: String,
    line: usize,
    kind: String,
}

// ---------------------------------------------------------------------------
// Pre-compiled regex statics (compiled once per process lifetime)
// ---------------------------------------------------------------------------

static RUST_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?m)^\s*pub\s+(?:async\s+)?fn\s+(\w+)").unwrap(),
            "function",
        ),
        (
            Regex::new(r"(?m)^\s*pub\s+struct\s+(\w+)").unwrap(),
            "struct",
        ),
        (Regex::new(r"(?m)^\s*pub\s+enum\s+(\w+)").unwrap(), "enum"),
        (Regex::new(r"(?m)^\s*pub\s+trait\s+(\w+)").unwrap(), "trait"),
    ]
});

static TS_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?m)^\s*export\s+(?:async\s+)?function\s+(\w+)").unwrap(),
            "function",
        ),
        (
            Regex::new(r"(?m)^\s*export\s+(?:default\s+)?class\s+(\w+)").unwrap(),
            "class",
        ),
        (
            Regex::new(r"(?m)^\s*export\s+const\s+(\w+)").unwrap(),
            "const",
        ),
        (
            Regex::new(r"(?m)^\s*export\s+(?:type|interface)\s+(\w+)").unwrap(),
            "type",
        ),
    ]
});

static PYTHON_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"(?m)^def\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"(?m)^class\s+(\w+)").unwrap(), "class"),
    ]
});

static GO_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"(?m)^func\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"(?m)^type\s+(\w+)\s+struct").unwrap(), "struct"),
        (
            Regex::new(r"(?m)^type\s+(\w+)\s+interface").unwrap(),
            "interface",
        ),
    ]
});

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect symbols in new/modified files that may duplicate existing ones.
///
/// Steps:
/// 1. Run `git diff --name-only <base_branch>..HEAD` to find changed files.
/// 2. Filter to source files.
/// 3. For each file, extract top-level public symbols using language-specific regexes.
/// 4. For each symbol, search the rest of the codebase (excluding the changed files)
///    for the same name.
/// 5. Return all matches as potential duplicates.
pub fn detect_duplicate_symbols(
    worktree_path: &Path,
    base_branch: &str,
) -> Result<Vec<DuplicateSymbol>> {
    let changed_files = collect_changed_source_files(worktree_path, base_branch)?;

    if changed_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut duplicates = Vec::new();

    for file_path in &changed_files {
        let abs_path = worktree_path.join(file_path);

        let content = match fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => continue, // file may have been deleted in this diff
        };

        let ext = extension_of(file_path);
        let symbols = extract_symbols(&content, &ext);

        for sym in symbols {
            // Skip noise names
            if NOISE_NAMES.contains(&sym.name.as_str()) {
                continue;
            }

            // Search for the same name in the rest of the codebase
            let matches =
                find_symbol_in_codebase(worktree_path, &sym.name, &sym.kind, &changed_files)?;

            for (existing_file, existing_line) in matches {
                duplicates.push(DuplicateSymbol {
                    symbol_name: sym.name.clone(),
                    new_file: file_path.clone(),
                    new_line: sym.line,
                    existing_file,
                    existing_line,
                    symbol_type: sym.kind.clone(),
                });
            }
        }
    }

    Ok(duplicates)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run `git diff --name-only <base>..HEAD` and return the paths of changed files
/// that have a known source extension.
fn collect_changed_source_files(worktree_path: &Path, base_branch: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{}..HEAD", base_branch)])
        .current_dir(worktree_path)
        .output()
        .with_context(|| format!("Failed to run git diff in {}", worktree_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git diff failed in {}: {}",
            worktree_path.display(),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = stdout
        .lines()
        .map(str::trim)
        .filter(|p| !p.is_empty() && has_source_extension(p))
        .map(str::to_string)
        .collect();

    Ok(result)
}

/// Return the lowercase extension of a file path, or an empty string.
fn extension_of(path: &str) -> String {
    path.rsplit('.').next().unwrap_or("").to_lowercase()
}

/// Return `true` if the path has a recognised source extension.
fn has_source_extension(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    SOURCE_EXTENSIONS.contains(&ext.as_str())
}

/// Extract top-level public symbols from `content` using language-specific patterns.
fn extract_symbols(content: &str, ext: &str) -> Vec<SymbolDef> {
    match ext {
        "rs" => extract_with_patterns(content, &RUST_PATTERNS),
        "ts" | "tsx" | "js" | "jsx" => extract_with_patterns(content, &TS_PATTERNS),
        "py" => extract_with_patterns(content, &PYTHON_PATTERNS),
        "go" => extract_with_patterns(content, &GO_PATTERNS),
        _ => Vec::new(),
    }
}

/// Shared implementation: iterate over pre-compiled (Regex, kind) pairs, run against
/// `content`, and collect symbol definitions with line numbers.
fn extract_with_patterns(content: &str, patterns: &[(Regex, &str)]) -> Vec<SymbolDef> {
    let mut symbols = Vec::new();

    for (re, kind) in patterns {
        for cap in re.captures_iter(content) {
            let name = cap[1].to_string();
            // Compute line number for the match start
            let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..match_start]
                .chars()
                .filter(|&c| c == '\n')
                .count()
                + 1;

            symbols.push(SymbolDef {
                name,
                line,
                kind: kind.to_string(),
            });
        }
    }

    symbols
}

/// Search the worktree for definitions of a symbol with a given name (and kind),
/// excluding `changed_files`.
///
/// Returns a list of (file_path, line_number) pairs for each match found.
fn find_symbol_in_codebase(
    worktree_path: &Path,
    name: &str,
    kind: &str,
    changed_files: &[String],
) -> Result<Vec<(String, usize)>> {
    let grep_pattern = build_grep_pattern(name, kind);

    let output = Command::new("grep")
        .args([
            "-r",
            "-n",
            "-E",
            "--binary-files=without-match",
            "--exclude-dir=.git",
            "--exclude-dir=target",
            "--exclude-dir=node_modules",
            "--exclude-dir=.worktrees",
            &grep_pattern,
            ".",
        ])
        .current_dir(worktree_path)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };

    // grep exit code 1 means no matches (normal), exit code 2 means error
    if let Some(code) = output.status.code() {
        if code == 2 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "warn: grep reported an error while searching for '{}': {}",
                name,
                stderr.trim()
            );
            return Ok(Vec::new());
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        // Format: "./path/to/file.rs:42:pub fn foo(...)"
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() < 3 {
            continue;
        }

        let file_raw = parts[0].trim_start_matches("./");
        // Skip if this file is one of the changed files (we're looking for pre-existing defs)
        if changed_files.iter().any(|f| f == file_raw) {
            continue;
        }

        // Only consider files with a source extension
        if !has_source_extension(file_raw) {
            continue;
        }

        let line_num: usize = parts[1].parse().unwrap_or(0);

        // Verify the matched line actually contains the symbol name as a word boundary
        // to avoid matching "get_something" when searching for "get".
        let matched_content = parts[2];
        if !word_boundary_match(matched_content, name) {
            continue;
        }

        results.push((file_raw.to_string(), line_num));
    }

    Ok(results)
}

/// Build a grep extended-regex pattern that matches symbol definitions in supported languages,
/// tailored by `kind` to reduce false positives.
fn build_grep_pattern(name: &str, kind: &str) -> String {
    match kind {
        "function" => format!(r"(fn|def|function)\s+{name}", name = name),
        "struct" => format!(r"struct\s+{name}", name = name),
        "enum" => format!(r"enum\s+{name}", name = name),
        "trait" => format!(r"trait\s+{name}", name = name),
        "class" => format!(r"class\s+{name}", name = name),
        "interface" => format!(r"interface\s+{name}", name = name),
        "type" | "const" => format!(r"(type|const|interface)\s+{name}", name = name),
        _ => format!(
            r"(fn|struct|enum|trait|class|interface|def|type|const|function)\s+{name}",
            name = name
        ),
    }
}

/// Return `true` if `text` contains `word` as a whole word (not as part of a longer identifier).
fn word_boundary_match(text: &str, word: &str) -> bool {
    // Simple implementation: check that the occurrence is not immediately preceded or
    // followed by an identifier character.
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        let before_ok = abs == 0
            || !text
                .as_bytes()
                .get(abs - 1)
                .map(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .unwrap_or(false);
        let after_ok = abs + word.len() >= text.len()
            || !text
                .as_bytes()
                .get(abs + word.len())
                .map(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols_pub_fn() {
        let content = "pub fn my_function() {}\npub struct MyStruct {}\n";
        let symbols = extract_with_patterns(content, &RUST_PATTERNS);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"my_function"));
        assert!(names.contains(&"MyStruct"));
    }

    #[test]
    fn test_extract_rust_symbols_private_excluded() {
        let content = "fn private_fn() {}\nstruct PrivateStruct {}\n";
        let symbols = extract_with_patterns(content, &RUST_PATTERNS);
        // Private symbols without `pub` should not be captured by pub patterns
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_rust_symbols_enum_and_trait() {
        let content = "pub enum Color { Red, Green }\npub trait Display {}\n";
        let symbols = extract_with_patterns(content, &RUST_PATTERNS);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Color"));
        assert!(names.contains(&"Display"));
    }

    #[test]
    fn test_extract_ts_symbols() {
        let content =
            "export function greet() {}\nexport class Greeter {}\nexport const VERSION = '1';\n";
        let symbols = extract_with_patterns(content, &TS_PATTERNS);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"Greeter"));
        assert!(names.contains(&"VERSION"));
    }

    #[test]
    fn test_extract_python_symbols() {
        let content = "def my_func():\n    pass\nclass MyClass:\n    pass\n";
        let symbols = extract_with_patterns(content, &PYTHON_PATTERNS);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"my_func"));
        assert!(names.contains(&"MyClass"));
    }

    #[test]
    fn test_extract_go_symbols() {
        let content = "func HandleRequest() {}\ntype Server struct {}\n";
        let symbols = extract_with_patterns(content, &GO_PATTERNS);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"HandleRequest"));
        assert!(names.contains(&"Server"));
    }

    #[test]
    fn test_word_boundary_match_exact() {
        assert!(word_boundary_match("pub fn store()", "store"));
    }

    #[test]
    fn test_word_boundary_match_no_match() {
        assert!(!word_boundary_match("pub fn storage()", "store"));
    }

    #[test]
    fn test_word_boundary_match_prefix() {
        assert!(!word_boundary_match("fn restore()", "store"));
    }

    #[test]
    fn test_has_source_extension() {
        assert!(has_source_extension("src/foo.rs"));
        assert!(has_source_extension("src/bar.ts"));
        assert!(has_source_extension("src/baz.py"));
        assert!(!has_source_extension("src/baz.md"));
        assert!(!has_source_extension("src/baz.toml"));
    }

    #[test]
    fn test_noise_names_filtered() {
        // Verify that common noise names are in the list
        assert!(NOISE_NAMES.contains(&"new"));
        assert!(NOISE_NAMES.contains(&"default"));
        assert!(NOISE_NAMES.contains(&"main"));
    }

    #[test]
    fn test_symbol_line_number() {
        let content = "// comment\npub fn first() {}\npub fn second() {}\n";
        let symbols = extract_with_patterns(content, &RUST_PATTERNS);
        let first = symbols.iter().find(|s| s.name == "first").unwrap();
        let second = symbols.iter().find(|s| s.name == "second").unwrap();
        assert_eq!(first.line, 2);
        assert_eq!(second.line, 3);
    }

    #[test]
    fn test_build_grep_pattern_function() {
        let pat = build_grep_pattern("my_func", "function");
        assert!(pat.contains("fn"));
        assert!(pat.contains("def"));
        assert!(pat.contains("function"));
        assert!(pat.contains("my_func"));
    }

    #[test]
    fn test_build_grep_pattern_struct() {
        let pat = build_grep_pattern("MyStruct", "struct");
        assert!(pat.contains("struct"));
        assert!(pat.contains("MyStruct"));
        // struct pattern should not include fn/def
        assert!(!pat.contains("fn"));
    }

    #[test]
    fn test_build_grep_pattern_kind_precision() {
        // enum pattern should only contain enum keyword
        let pat = build_grep_pattern("Color", "enum");
        assert!(pat.contains("enum"));
        assert!(pat.contains("Color"));
        assert!(!pat.contains("fn"));
        assert!(!pat.contains("struct"));
    }
}
