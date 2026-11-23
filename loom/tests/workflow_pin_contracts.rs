//! Contract for stage hooks-and-delivery: the GitHub workflows pin every action
//! to a commit, keep workflow expressions out of release shell bodies, and the
//! CI `rust:` path filter covers every directory and template that
//! `loom/build/assets.rs` embeds into the binary.
//!
//! The files are read as text and scanned line by line (standard library only).

use std::fs;
use std::path::{Path, PathBuf};

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn read(relative: &str) -> String {
    fs::read_to_string(repo_file(relative)).unwrap_or_else(|err| panic!("read {relative}: {err}"))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// `(file name, text)` of every `.github/workflows/*.yml`, sorted by name.
fn workflows() -> Vec<(String, String)> {
    let dir = repo_file(".github/workflows");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("workflow dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yml"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path).expect("read workflow");
            (name, text)
        })
        .collect();
    files.sort();
    files
}

/// The value of a `key:` line, also in its `- key:` form, with the column the
/// key starts at.
fn key_value<'a>(line: &'a str, key: &str) -> Option<(usize, &'a str)> {
    let trimmed = line.trim_start();
    let (column, rest) = match trimmed.strip_prefix("- ") {
        Some(rest) => (indent(line) + 2, rest.trim_start()),
        None => (indent(line), trimmed),
    };
    rest.strip_prefix(key)?
        .strip_prefix(':')
        .map(|value| (column, value.trim()))
}

fn is_commit_sha(reference: &str) -> bool {
    reference.len() == 40
        && reference
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[test]
fn no_action_is_pinned_to_a_mutable_tag() {
    let files = workflows();
    assert!(
        files.len() >= 4,
        "expected ci, codeql, nightly and release workflows"
    );
    let mut mutable = Vec::new();
    for (name, text) in &files {
        for (number, line) in text.lines().enumerate() {
            let Some((_, value)) = key_value(line, "uses") else {
                continue;
            };
            let action = value
                .split(|c: char| c.is_whitespace() || c == '#')
                .next()
                .unwrap_or_default()
                .trim_matches(|c| c == '"' || c == '\'');
            if action.starts_with("./") {
                continue;
            }
            let reference = action.rsplit_once('@').map(|(_, r)| r).unwrap_or_default();
            if !is_commit_sha(reference) {
                mutable.push(format!("{name}:{}: {action}", number + 1));
            }
        }
    }
    assert!(
        mutable.is_empty(),
        "actions not pinned to a 40-hex commit:\n{}",
        mutable.join("\n")
    );
}

/// Every `run:` body of a workflow: the inline value, or the deeper-indented
/// lines below a `|` or `>` block scalar. Each body carries its first line number.
fn run_bodies(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut bodies = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((column, value)) = key_value(line, "run") else {
            continue;
        };
        if !(value.starts_with('|') || value.starts_with('>')) {
            bodies.push((index + 1, value.to_owned()));
            continue;
        }
        let body: Vec<&str> = lines[index + 1..]
            .iter()
            .take_while(|next| next.trim().is_empty() || indent(next) > column)
            .copied()
            .collect();
        bodies.push((index + 1, body.join("\n")));
    }
    bodies
}

#[test]
fn release_run_bodies_hold_no_workflow_expression() {
    let bodies = run_bodies(&read(".github/workflows/release.yml"));
    assert!(!bodies.is_empty(), "release.yml has no run: step");
    let tainted: Vec<String> = bodies
        .iter()
        .filter(|(_, body)| body.contains("${{"))
        .map(|(line, body)| format!("release.yml:{line}:\n{body}"))
        .collect();
    assert!(
        tainted.is_empty(),
        "run bodies holding a workflow expression:\n{}",
        tainted.join("\n---\n")
    );
}

/// The quoted patterns of the `rust:` filter inside the `changes` job of ci.yml.
fn rust_filter_patterns(ci: &str) -> Vec<String> {
    let lines: Vec<&str> = ci.lines().collect();
    let job = lines
        .iter()
        .position(|line| line.trim() == "changes:")
        .expect("ci.yml has a changes job");
    let job_indent = indent(lines[job]);
    let job_lines: Vec<&str> = lines[job + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indent(line) > job_indent)
        .copied()
        .collect();
    let start = job_lines
        .iter()
        .position(|line| line.trim() == "rust:")
        .expect("the changes job has a rust: filter");
    let key_indent = indent(job_lines[start]);
    job_lines[start + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indent(line) > key_indent)
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|pattern| pattern.trim().trim_matches('"').to_owned())
        .collect()
}

/// The paths of every `const NAME_ROOT` / `const NAME_TEMPLATE` in assets.rs.
fn embed_roots(assets: &str) -> Vec<String> {
    assets
        .lines()
        .filter_map(|line| line.trim().strip_prefix("const "))
        .filter(|decl| decl.contains("_ROOT: &str = \"") || decl.contains("_TEMPLATE: &str = \""))
        .filter_map(|decl| decl.split('"').nth(1).map(str::to_owned))
        .collect()
}

fn covers(pattern: &str, path: &str) -> bool {
    pattern == path
        || pattern.strip_suffix("/**") == Some(path)
        || pattern
            .strip_prefix('*')
            .is_some_and(|suffix| path.ends_with(suffix))
}

#[test]
fn rust_filter_covers_every_embed_root() {
    let patterns = rust_filter_patterns(&read(".github/workflows/ci.yml"));
    let roots = embed_roots(&read("loom/build/assets.rs"));
    for expected in ["plan-plugin", "web/dist", "agents", "CLAUDE.md.template"] {
        assert!(
            roots.iter().any(|root| root == expected),
            "assets.rs no longer declares the embed root {expected}: {roots:?}"
        );
    }
    let uncovered: Vec<&String> = roots
        .iter()
        .filter(|root| !patterns.iter().any(|pattern| covers(pattern, root)))
        .collect();
    assert!(
        uncovered.is_empty(),
        "embed roots outside the ci.yml rust: filter {patterns:?}: {uncovered:?}"
    );
}
