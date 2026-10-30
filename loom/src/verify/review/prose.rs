//! Behaviour hashes exclude prose and ordinary whole-line comments.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn behaviour_digest(path: &str, content: Option<&[u8]>) -> Option<String> {
    if is_prose(path) {
        return None;
    }
    let Some(content) = content else {
        return Some("deleted".to_string());
    };
    let prefix = match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "go") => "//",
        Some("py") => "#",
        _ => return Some(hash(content)),
    };
    let Ok(text) = std::str::from_utf8(content) else {
        return Some(hash(content));
    };
    // A comment-prefixed line inside a multiline string counts as a comment.
    // Revisit if such an edit passes a stage's review gate.
    let mut digest = Sha256::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || (trimmed.starts_with(prefix) && !is_pragma(trimmed)) {
            continue;
        }
        digest.update(line.as_bytes());
    }
    Some(hex::encode(digest.finalize()))
}

fn hash(content: &[u8]) -> String {
    hex::encode(Sha256::digest(content))
}

fn is_prose(path: &str) -> bool {
    let path = Path::new(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    path.extension().is_some_and(|ext| ext == "md")
        && !name.starts_with("CLAUDE")
        && !name.starts_with("AGENTS")
        && !path.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some(
                    "skills"
                        | "agents"
                        | "commands"
                        | "loom-hooks"
                        | ".claude"
                        | ".codex"
                        | "tests"
                        | "fixtures"
                        | "guidance"
                )
            )
        })
}

fn is_pragma(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    [
        "@ts-",
        "eslint",
        "oxlint",
        "biome-",
        "prettier-",
        "noqa",
        "type:",
        "pylint",
        "mypy",
        "rustfmt",
        "clippy",
        "go:",
        "nolint",
        "istanbul",
        "c8 ",
        "v8 ignore",
        "@vitest",
        "@jest",
        "@generated",
        "<reference",
        "+build",
        "coding",
        "#!",
        "@jsx",
    ]
    .iter()
    .any(|pragma| line.contains(pragma))
}

#[cfg(test)]
#[path = "prose_tests.rs"]
mod tests;
