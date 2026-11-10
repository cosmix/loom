//! Applies the current redaction and scrubbing to the probe fixtures already on disk, so a
//! change to either needs no rerun of the probes.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::home::{identity, real_home, user_name};
use super::output::{fixtures, note_text, records};
use super::redact::Redactor;

/// The directories under `tests/fixtures/plan_session` the probes write.
const DIRS: [&str; 3] = ["transcripts", "spool", "probe-notes"];

/// Text a fixture must never hold again, whatever the operator's setup.
const FORBIDDEN: [&str; 9] = [
    "srt.",
    "noreply@anthropic.com",
    "Co-Authored-By",
    "Europe/",
    "Claude Ultra",
    "/tmp/claude-1000",
    "cc-socks",
    "skill_listing",
    "--setenv",
];

/// The `p1-*` and `p2-*` files of the three fixture directories, sorted. The fixtures the
/// contract tests committed (no such prefix) are not the probes' and stay as they are.
fn probe_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in DIRS {
        let entries = fs::read_dir(fixtures().join(dir)).into_iter().flatten();
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("p1-") || name.starts_with("p2-") {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    found
}

fn rescrub(redactor: &mut Redactor, path: &Path) {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cleaned = if path.extension().is_some_and(|ext| ext == "md") {
        note_text(redactor, &text)
    } else {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        records(redactor, &lines)
    };
    fs::write(path, cleaned).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// Every non-empty line of a JSON file parses; none holds text a fixture must not keep.
fn check(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let name = path.display();
    let mut problems = Vec::new();
    let json = path
        .extension()
        .is_some_and(|ext| ext == "jsonl" || ext == "json");
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        if json && serde_json::from_str::<Value>(line).is_err() {
            problems.push(format!("{name}:{}: not JSON", n + 1));
        }
        for word in FORBIDDEN.iter().filter(|word| line.contains(**word)) {
            problems.push(format!("{name}:{}: still holds {word}", n + 1));
        }
    }
    problems
}

#[test]
#[ignore = "rewrites the committed probe fixtures in place"]
fn rescrub_existing_fixtures() {
    let real = real_home();
    let base = Path::new("/nonexistent/probe-base");
    let files = probe_files();
    assert!(!files.is_empty(), "no probe fixtures to scrub");
    for path in &files {
        let mut redactor = Redactor::new(&real, base, &user_name(&real), &identity(&real));
        rescrub(&mut redactor, path);
    }
    let problems: Vec<String> = files.iter().flat_map(|path| check(path)).collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    println!("rescrubbed {} fixture files", files.len());
}
