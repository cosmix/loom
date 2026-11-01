//! Behaviour hashes exclude unprotected prose, but retain source bytes exactly.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn behaviour_digest(path: &str, content: Option<&[u8]>) -> Option<String> {
    if is_prose(path) {
        return None;
    }
    let Some(content) = content else {
        return Some("v2:deleted".to_string());
    };
    // Prefix the evidence so older, normalized hashes cannot authorize an exemption.
    Some(format!("v2:{}", hash(content)))
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

#[cfg(test)]
#[path = "prose_tests.rs"]
mod tests;
