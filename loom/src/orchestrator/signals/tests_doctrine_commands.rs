//! Every `loom` command a doctrine surface teaches resolves against the real
//! CLI tree, so a renamed or removed subcommand fails here instead of costing
//! an agent a correction turn.
//!
//! Surfaces are read at run time from the repository root. Candidates are
//! taken by a markdown rule (fenced shell blocks and inline code spans that
//! start with `loom`) or a hook rule (the whole script plus the guidance lines
//! a hook prints from heredoc bodies the lexer skips).

use std::fs;
use std::path::{Path, PathBuf};

use crate::plan::schema::validation::v2_lints::loom_subcommands::loom_command_paths;

const SHELL_FENCES: [&str; 5] = ["bash", "sh", "shell", "zsh", "console"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Files under `dir` (recursively) whose name ends in `ext`, sorted.
fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_with_ext(&path, ext));
        } else if path.extension().is_some_and(|e| e == ext) {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Whether `text` starts with the word `loom`.
fn starts_with_loom(text: &str) -> bool {
    text.strip_prefix("loom")
        .is_some_and(|rest| rest.chars().next().is_none_or(char::is_whitespace))
}

/// Candidates of the markdown rule, as `(line number, text)`.
fn markdown_candidates(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    // Backtick count and shell-ness of the fence being read.
    let mut fence: Option<(usize, bool)> = None;
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        if let Some((open, shell)) = fence {
            if ticks >= open && trimmed.chars().all(|c| c == '`') {
                fence = None;
            } else if shell {
                let code = trimmed.strip_prefix('$').unwrap_or(trimmed).trim();
                out.push((idx + 1, code.to_string()));
            }
        } else if ticks >= 3 {
            let info = trimmed[ticks..].split_whitespace().next().unwrap_or("");
            fence = Some((ticks, SHELL_FENCES.contains(&info)));
        } else {
            for span in line.split('`').skip(1).step_by(2) {
                if starts_with_loom(span.trim()) {
                    out.push((idx + 1, span.trim().to_string()));
                }
            }
        }
    }
    out
}

/// Lines a hook prints that start with `loom`, past box-drawing characters.
fn hook_guidance_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_start_matches(|c: char| c.is_whitespace() || c == '│'))
        .filter(|line| starts_with_loom(line))
        .map(|line| line.trim_end_matches(|c: char| c.is_whitespace() || c == '│'))
        .map(str::to_string)
        .collect()
}

/// Run every candidate through the resolver; returns the invocations checked
/// and `<file>: <path>` for each unresolved one.
fn scan(label: &str, files: &[PathBuf], hooks: bool) -> (usize, Vec<String>) {
    let (mut checked, mut bad) = (0, Vec::new());
    for file in files {
        let text = fs::read_to_string(file).unwrap_or_else(|e| panic!("{file:?}: {e}"));
        let name = file.strip_prefix(repo_root()).unwrap_or(file).display();
        let candidates: Vec<(usize, String)> = if hooks {
            let lines = hook_guidance_lines(&text).into_iter().map(|l| (0, l));
            std::iter::once((0, text.clone())).chain(lines).collect()
        } else {
            markdown_candidates(&text)
        };
        for (line, candidate) in candidates {
            let found = loom_command_paths(&candidate);
            checked += found.checked;
            for path in found.unresolved {
                let at = if line > 0 {
                    format!(":{line}")
                } else {
                    String::new()
                };
                bad.push(format!("{label} {name}{at}: {path}"));
            }
        }
    }
    (checked, bad)
}

fn assert_group(label: &str, files: Vec<PathBuf>, hooks: bool, minimum: usize) {
    assert!(!files.is_empty(), "{label}: no surface files found");
    let (checked, bad) = scan(label, &files, hooks);
    eprintln!("{label}: checked {checked}");
    assert!(
        checked >= minimum,
        "{label}: checked {checked} invocations, expected at least {minimum}; \
         the extractor misses a form"
    );
    assert!(
        bad.is_empty(),
        "unresolved loom commands:\n{}",
        bad.join("\n")
    );
}

#[test]
fn template_commands_resolve() {
    let root = repo_root();
    let files = vec![
        root.join("CLAUDE.md.template"),
        root.join("AGENTS.md.template"),
    ];
    assert_group("templates", files, false, 20);
}

#[test]
fn slash_command_commands_resolve() {
    let files = files_with_ext(&repo_root().join("commands"), "md");
    assert_group("commands", files, false, 5);
}

#[test]
fn skill_commands_resolve() {
    let skills = repo_root().join("skills");
    let mut files = Vec::new();
    for entry in fs::read_dir(&skills).expect("skills dir").flatten() {
        if entry.file_name().to_string_lossy().starts_with("loom-") && entry.path().is_dir() {
            files.extend(files_with_ext(&entry.path(), "md"));
        }
    }
    files.sort();
    assert_group("skills", files, false, 150);
}

#[test]
fn hook_commands_resolve() {
    let files = files_with_ext(&repo_root().join("loom-hooks"), "sh")
        .into_iter()
        .filter(|p| p.parent().is_some_and(|d| d.ends_with("loom-hooks")))
        .collect();
    assert_group("hooks", files, true, 10);
}

#[test]
fn resolver_self_check() {
    assert!(!loom_command_paths("loom knowledg update mistakes x")
        .unresolved
        .is_empty());
    let bin = loom_command_paths("\"${LOOM_BIN:-loom}\" hook nope");
    assert!(!bin.unresolved.is_empty());
    let ok = loom_command_paths("loom knowledge update mistakes x");
    assert_eq!(ok.checked, 1);
    assert!(ok.unresolved.is_empty());
    assert!(loom_command_paths("loom knowledge").unresolved.is_empty());
}

#[test]
fn hook_guidance_self_check() {
    let fixture = "cat <<'EOF'\n│  loom memroy note \"x\"   │\nEOF\n";
    assert_eq!(loom_command_paths(fixture).checked, 0);
    let lines = hook_guidance_lines(fixture);
    assert_eq!(lines.len(), 1);
    assert!(!loom_command_paths(&lines[0]).unresolved.is_empty());
}
