//! Locates the `subagents/` transcript directory `loom subagents` reads.
//!
//! Layout (verified empirically against live transcripts, mirrored by
//! `hooks/_common.sh:1010-1043`'s own classification of the same paths):
//!
//! ```text
//! ~/.claude/projects/<project-slug>/<session-uuid>/subagents/agent-<agentId>.jsonl
//! ~/.claude/projects/<project-slug>/<session-uuid>.jsonl   (the main session's own transcript)
//! ```
//!
//! `<project-slug>` is the absolute cwd with every `/` and `.` replaced by
//! `-` (a `.worktrees` path segment's `/.` therefore collapses to `--`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Where `loom subagents` decided to look for transcripts.
pub enum Resolution {
    /// A concrete `subagents/` directory to scan. It may not exist yet, or
    /// may currently be empty -- that is normal, not an error.
    Found(PathBuf),
    /// Resolution failed outright before reaching a directory to scan (no
    /// home directory, or no session found for this cwd). The string names
    /// what was looked for, for the diagnostic printed to the user.
    NotFound(String),
}

/// Resolve the transcript directory: `dir` wins outright; else `session`
/// selects `<project-slug>/<session>/subagents` directly; else auto-detect
/// the most recently active session under this cwd's project slug.
pub fn resolve(dir: Option<PathBuf>, session: Option<String>) -> Resolution {
    if let Some(dir) = dir {
        return Resolution::Found(dir);
    }

    let Some(projects_dir) = projects_root() else {
        return Resolution::NotFound(
            "could not determine home directory (~/.claude/projects)".to_string(),
        );
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_dir = projects_dir.join(project_slug(&cwd));

    if let Some(session) = session {
        return Resolution::Found(project_dir.join(session).join("subagents"));
    }

    match most_recent_session_dir(&project_dir) {
        Some(session_dir) => Resolution::Found(session_dir.join("subagents")),
        None => Resolution::NotFound(format!(
            "no session with a transcript found under {}",
            project_dir.display()
        )),
    }
}

/// List `agent-*.jsonl` transcript files in `dir`, sorted by file name for a
/// stable listing order. Returns an empty vec when `dir` does not exist --
/// absence of subagents is normal, not an error.
pub fn list_agent_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "jsonl")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem.starts_with("agent-"))
        })
        .collect();
    files.sort();
    files
}

/// Extract the agent ID from an `agent-<id>.jsonl` path (the ID is whatever
/// follows the `agent-` prefix, verbatim).
pub fn agent_id_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown");
    stem.strip_prefix("agent-").unwrap_or(stem).to_string()
}

fn projects_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("projects"))
}

/// Replace every `/` and `.` in the absolute cwd with `-`, matching the slug
/// Claude Code derives for `~/.claude/projects/<slug>/`.
fn project_slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// Among `project_dir`'s session subdirectories, pick the one whose sibling
/// `<name>.jsonl` (the main session's own transcript) has the most recent
/// mtime. Returns `None` when `project_dir` doesn't exist or has no session
/// with a sibling transcript.
fn most_recent_session_dir(project_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(project_dir).ok()?;
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let sibling = project_dir.join(format!("{name}.jsonl"));
        let Ok(modified) = fs::metadata(&sibling).and_then(|meta| meta.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(newest, _)| modified > *newest) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_replaces_slashes_and_dots() {
        assert_eq!(
            project_slug(Path::new("/home/dkaponis/src/loom")),
            "-home-dkaponis-src-loom"
        );
    }

    #[test]
    fn slug_collapses_dot_segment_to_double_dash() {
        assert_eq!(
            project_slug(Path::new(
                "/home/dkaponis/src/cartolyth/.worktrees/admin1-atlas"
            )),
            "-home-dkaponis-src-cartolyth--worktrees-admin1-atlas"
        );
    }

    #[test]
    fn agent_id_strips_prefix_and_extension() {
        let path = Path::new("/tmp/subagents/agent-review-1f5454ffb2be845b.jsonl");
        assert_eq!(agent_id_from_path(path), "review-1f5454ffb2be845b");
    }

    #[test]
    fn agent_id_falls_back_when_prefix_missing() {
        let path = Path::new("/tmp/subagents/not-an-agent-file.jsonl");
        assert_eq!(agent_id_from_path(path), "not-an-agent-file");
    }

    #[test]
    fn list_agent_files_filters_by_name_and_extension() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("agent-a.jsonl"), "").unwrap();
        std::fs::write(temp.path().join("agent-a.meta.json"), "{}").unwrap();
        std::fs::write(temp.path().join("agent-b.jsonl"), "").unwrap();
        std::fs::write(temp.path().join("stray.txt"), "").unwrap();

        let files = list_agent_files(temp.path());
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["agent-a.jsonl", "agent-b.jsonl"]);
    }

    #[test]
    fn list_agent_files_missing_directory_is_empty_not_error() {
        let files = list_agent_files(Path::new("/nonexistent/path/does/not/exist"));
        assert!(files.is_empty());
    }

    #[test]
    fn resolve_dir_flag_wins_outright() {
        let explicit = PathBuf::from("/tmp/explicit-subagents-dir");
        match resolve(Some(explicit.clone()), Some("some-session".to_string())) {
            Resolution::Found(path) => assert_eq!(path, explicit),
            Resolution::NotFound(_) => panic!("--dir must always resolve"),
        }
    }

    #[test]
    fn most_recent_session_dir_picks_newest_sibling_transcript() {
        let temp = tempfile::tempdir().unwrap();
        let older = temp.path().join("older-uuid");
        let newer = temp.path().join("newer-uuid");
        std::fs::create_dir(&older).unwrap();
        std::fs::create_dir(&newer).unwrap();
        std::fs::write(temp.path().join("older-uuid.jsonl"), "").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(temp.path().join("newer-uuid.jsonl"), "").unwrap();

        let chosen = most_recent_session_dir(temp.path()).unwrap();
        assert_eq!(chosen, newer);
    }

    #[test]
    fn most_recent_session_dir_none_when_project_dir_missing() {
        assert!(most_recent_session_dir(Path::new("/nonexistent/project/dir")).is_none());
    }
}
