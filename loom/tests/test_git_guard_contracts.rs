//! Guard contracts for the shared git fixture sweep.
//!
//! Every test walks `src/` and `tests/` and inspects only TEST CODE lines:
//! 1. every line of every file under `tests/`;
//! 2. every line of a `src/` file whose relative path has a component (a
//!    directory name or the file stem) named like a test module;
//! 3. in any other `src/` file, the lines from the first `#[cfg(test)]` that
//!    opens an inline `mod name {` block to the end of the file.
//!
//! Exempt: the shared helper, its contracts, this file, the two production git
//! modules, and any file carrying a reasoned bespoke fixture marker.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const EXEMPT_FILES: &[&str] = &[
    "tests/test_support/git_repo.rs",
    "tests/test_git_contracts.rs",
    "tests/test_git_guard_contracts.rs",
    "src/git/repository.rs",
    "src/git/init_blockers.rs",
];

const RUNNER_NAMES: &str =
    "git|run_git|git_ok|git_checked|git_unchecked|git_in|git_out|git_output|\
git_stdout|isolated_git|isolated_git_ok";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn is_test_component(name: &str) -> bool {
    name == "tests"
        || name == "test_support"
        || name.starts_with("tests_")
        || name.starts_with("test_")
        || name.ends_with("_tests")
        || name.ends_with("_test")
        || name.contains("_tests_")
        || name.contains("_test_")
}

/// Rule 2: a component of the path relative to `src/` names a test module.
fn src_path_is_test(rel_to_src: &Path) -> bool {
    let mut names: Vec<String> = Vec::new();
    if let Some(parent) = rel_to_src.parent() {
        for comp in parent.components() {
            names.push(comp.as_os_str().to_string_lossy().into_owned());
        }
    }
    if let Some(stem) = rel_to_src.file_stem() {
        names.push(stem.to_string_lossy().into_owned());
    }
    names.iter().any(|name| is_test_component(name))
}

/// Rule 3: index of the first `#[cfg(test)]` line whose next non-blank line
/// opens an inline module block, if any.
fn inline_test_module_start(lines: &[&str]) -> Option<usize> {
    let mod_open = Regex::new(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{\s*$").expect("regex");
    for (idx, line) in lines.iter().enumerate() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let next = lines[idx + 1..].iter().find(|l| !l.trim().is_empty());
        if next.is_some_and(|l| mod_open.is_match(l)) {
            return Some(idx);
        }
    }
    None
}

fn is_exempt(rel: &str, text: &str) -> bool {
    let marker = Regex::new(r"bespoke-git-fixture:.*\S").expect("regex");
    EXEMPT_FILES.contains(&rel) || text.lines().any(|line| marker.is_match(line))
}

/// First test-code line index of a file, or `None` when it holds no test code.
fn test_code_start(rel: &Path, lines: &[&str]) -> Option<usize> {
    if rel.starts_with("tests") {
        return Some(0);
    }
    let rel_to_src = rel.strip_prefix("src").expect("file under src/ or tests/");
    if src_path_is_test(rel_to_src) {
        return Some(0);
    }
    inline_test_module_start(lines)
}

/// Every `path:line` of a non-exempt test-code line that `offends` matches,
/// and how many files held test code the scan read.
fn scan(offends: impl Fn(&str) -> bool) -> (Vec<String>, usize) {
    let root = manifest_dir();
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    collect_rs_files(&root.join("tests"), &mut files);
    files.sort();
    let mut found = Vec::new();
    let mut scanned = 0;
    for path in files {
        let rel = path.strip_prefix(&root).expect("under manifest dir");
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {rel_str}: {e}"));
        if is_exempt(&rel_str, &text) {
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        let Some(start) = test_code_start(rel, &lines) else {
            continue;
        };
        scanned += 1;
        for (idx, line) in lines.iter().enumerate().skip(start) {
            if offends(line) {
                found.push(format!("{rel_str}:{}", idx + 1));
            }
        }
    }
    (found, scanned)
}

/// Every `path:line` of a non-exempt test-code line that `offends` matches.
fn offenders(offends: impl Fn(&str) -> bool) -> Vec<String> {
    scan(offends).0
}

fn sets_up_identity(line: &str) -> bool {
    line.contains("user.email")
}

fn defines_local_runner(line: &str) -> bool {
    static RUNNER: OnceLock<Regex> = OnceLock::new();
    RUNNER
        .get_or_init(|| {
            let pattern = format!(r"^\s*(pub(\([^)]*\))?\s+)?fn\s+({RUNNER_NAMES})\s*[<(]");
            Regex::new(&pattern).expect("regex")
        })
        .is_match(line)
}

fn hand_writes_config_isolation(line: &str) -> bool {
    line.contains("GIT_CONFIG_NOSYSTEM")
}

fn assert_no_offenders(what: &str, found: &[String]) {
    assert!(
        found.is_empty(),
        "{} test-code line(s) {what}; move them to tests/test_support/git_repo.rs \
         or mark the file with a reasoned bespoke-git-fixture marker:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn identity_setup_lives_only_in_shared_helper() {
    let found = offenders(sets_up_identity);
    assert_no_offenders("set up a git identity (user.email)", &found);
}

#[test]
fn test_code_defines_no_local_git_runner() {
    let found = offenders(defines_local_runner);
    assert_no_offenders("define a local git runner function", &found);
}

#[test]
fn git_config_isolation_lives_only_in_shared_helper() {
    let found = offenders(hand_writes_config_isolation);
    assert_no_offenders(
        "hand-write git config isolation (GIT_CONFIG_NOSYSTEM)",
        &found,
    );
}

#[test]
fn classification_matches_stage_rules() {
    assert!(src_path_is_test(Path::new("git/merge/test_support.rs")));
    assert!(src_path_is_test(Path::new("host_verify/tests_runner.rs")));
    assert!(src_path_is_test(Path::new("git/user_git_tests.rs")));
    assert!(src_path_is_test(Path::new(
        "orchestrator/core/merge_lifecycle_e2e_tests_support.rs"
    )));
    assert!(src_path_is_test(Path::new(
        "context/refresh/tests_source_graph/mod.rs"
    )));
    assert!(!src_path_is_test(Path::new("git/runner.rs")));
    assert!(!src_path_is_test(Path::new("testrun/languages.rs")));
    let lines = [
        "fn a() {}",
        "#[cfg(test)]",
        "use x;",
        "#[cfg(test)]",
        "",
        "mod tests {",
    ];
    assert_eq!(inline_test_module_start(&lines), Some(3));
    assert!(is_exempt(
        "src/a.rs",
        "// bespoke-git-fixture: exercises signing config"
    ));
    assert!(!is_exempt("src/a.rs", "// bespoke-git-fixture:   "));
}

#[test]
fn guard_predicates_match_offenders_and_reject_near_misses() {
    for line in [
        "fn git_ok(dir: &Path) -> String {",
        "    pub(crate) fn git<I: AsRef<OsStr>>(dir: &Path, args: I) {",
        "fn isolated_git(dir: &Path) {",
        "pub fn run_git(args: &[&str]) {",
    ] {
        assert!(defines_local_runner(line), "{line}");
    }
    for line in [
        "fn git_command_x(dir: &Path) {",
        "fn gitignore(dir: &Path) {",
        "    let out = git(dir, &[\"status\"]);",
        "// fn git(dir: &Path)",
    ] {
        assert!(!defines_local_runner(line), "{line}");
    }
    assert!(hand_writes_config_isolation(
        r#"    .env("GIT_CONFIG_NOSYSTEM", "1")"#
    ));
    assert!(!hand_writes_config_isolation(
        r#"    .env("GIT_CONFIG_GLOBAL", "/dev/null")"#
    ));
    assert!(sets_up_identity(
        r#"    git(dir, &["config", "user.email", "a@b"]);"#
    ));
    assert!(!sets_up_identity(
        r#"    git(dir, &["config", "user.name", "a"]);"#
    ));
}

#[test]
fn scan_visits_test_code_skips_exempt_files_and_reports_matches() {
    let (found, scanned) = scan(|_| true);
    assert!(scanned > 50, "scan visited only {scanned} test-code files");
    assert!(
        found.iter().any(|hit| hit == "tests/test_support/mod.rs:1"),
        "a tests/ file must be scanned from line 1"
    );
    for exempt in EXEMPT_FILES {
        let prefix = format!("{exempt}:");
        assert!(
            !found.iter().any(|hit| hit.starts_with(&prefix)),
            "{exempt} is exempt"
        );
    }
    assert!(
        !found.iter().any(|hit| hit == "src/git/runner.rs:1"),
        "production lines are not test code"
    );
    assert_eq!(
        test_code_start(Path::new("tests/x.rs"), &["a", "b"]),
        Some(0)
    );
    assert_eq!(
        test_code_start(Path::new("src/git/runner.rs"), &["fn a() {}"]),
        None
    );
}
