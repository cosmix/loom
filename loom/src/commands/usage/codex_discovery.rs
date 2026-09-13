use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub(crate) struct CodexDiscovery {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) missing_roots: usize,
    pub(crate) unreadable_directories: usize,
}

pub(crate) fn discover(explicit_root: Option<&Path>) -> CodexDiscovery {
    let Some(root) = codex_root(explicit_root) else {
        return CodexDiscovery {
            missing_roots: 1,
            ..CodexDiscovery::default()
        };
    };
    if !root.is_dir() {
        return CodexDiscovery {
            missing_roots: 1,
            ..CodexDiscovery::default()
        };
    }
    let mut result = CodexDiscovery::default();
    let sessions = root.join("sessions");
    if sessions.is_dir() {
        collect_jsonl(&sessions, &mut result);
    } else {
        result.missing_roots += 1;
    }
    let archived = root.join("archived_sessions");
    if archived.is_dir() {
        collect_jsonl(&archived, &mut result);
    }
    result.files.sort();
    result
}

fn codex_root(explicit_root: Option<&Path>) -> Option<PathBuf> {
    explicit_root.map(Path::to_path_buf).or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".codex"))
    })
}

fn collect_jsonl(directory: &Path, result: &mut CodexDiscovery) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            result.unreadable_directories += 1;
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, result);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            result.files.push(path);
        }
    }
}

#[cfg(test)]
#[path = "codex_discovery_tests.rs"]
mod tests;
