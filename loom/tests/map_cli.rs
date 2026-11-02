//! Binary-level checks of the `loom map` surface beyond the frozen API
//! contracts: source windows, timings, references, evidence and language
//! filters, each run against a temp git repository.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;
use crate::helpers::test_support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::{Builder, TempDir};

const LIB_RS: &str = "pub fn target() {}\npub fn caller() {\n    let _x = 1;\n    target();\n}\n";
const TOOL_PY: &str = "def target():\n    return 1\n";

fn git(root: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A committed git repo holding `files`, with the untracked `.loom/work` dir
/// that makes loom resolve it as the project.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let temp = Builder::new()
        .prefix("loom-map-cli-")
        .tempdir_in(std::env::temp_dir())
        .expect("create unique scratch directory");
    let root = temp.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "user.email", "t@t"]);
    for (path, body) in files {
        let full = root.join(path);
        fs::create_dir_all(full.parent().expect("file has a parent")).expect("create dirs");
        fs::write(&full, body).expect("write fixture file");
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    test_support::write_main_checkout_identity(root, &root.join(".loom/work"));
    temp
}

fn run_map(root: &Path, args: &[&str]) -> Output {
    helpers::loom_cmd()
        .arg("map")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn loom map")
}

fn map_json(root: &Path, args: &[&str]) -> Value {
    parse_json(args, &run_map(root, args))
}

fn parse_json(args: &[&str], output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "loom map {args:?} exited {:?}\nstdout: {stdout}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout of loom map {args:?} is not JSON ({e}): {stdout}"))
}

#[test]
fn window_of_a_node_prints_exactly_its_lines() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);
    let head = String::from_utf8_lossy(&git(repo.path(), &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();

    let output = run_map(repo.path(), &["--window", "src/lib.rs#function:caller"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            format!("src/lib.rs:L2-L5  current {}  hash match", &head[..8]).as_str(),
            "pub fn caller() {",
            "    let _x = 1;",
            "    target();",
            "}",
        ],
        "stdout: {stdout}"
    );
}

#[test]
fn timings_json_holds_the_seven_numeric_phases() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);
    let args = ["--find-all", "target", "--json", "--timings"];

    let output = run_map(repo.path(), &args);
    let json = parse_json(&args, &output);

    let timings = json["timings"].as_object().expect("timings object");
    for phase in [
        "snapshot",
        "load",
        "resolve",
        "view",
        "query",
        "render",
        "total",
        "peak_rss_kb",
    ] {
        assert!(
            timings.get(phase).is_some_and(Value::is_number),
            "timings.{phase}: {timings:?}"
        );
    }
    assert!(String::from_utf8_lossy(&output.stderr).contains("total"));
}

#[test]
fn warm_second_run_reads_the_materialized_view() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);
    let args = ["--find-all", "target", "--json"];

    let first = map_json(repo.path(), &args);
    let second = map_json(repo.path(), &args);

    assert!(
        first["snapshot"]["resolver_version"].is_number(),
        "{first:#}"
    );
    assert!(
        matches!(
            first["snapshot"]["view"].as_str(),
            Some("built" | "materialized")
        ),
        "{first:#}"
    );
    assert_eq!(second["snapshot"]["view"], "materialized", "{second:#}");
    assert_eq!(second["views"], first["views"]);
}

#[test]
fn references_without_any_print_an_empty_state_and_exit_zero() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);

    let output = run_map(repo.path(), &["--references", "caller"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("no direct references"), "stdout: {stdout}");
}

#[test]
fn evidence_restricts_which_edges_impact_walks() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);

    let all = map_json(repo.path(), &["--impact", "target", "--json"]);
    let import_only = map_json(
        repo.path(),
        &["--impact", "target", "--evidence", "import", "--json"],
    );

    let ids = |json: &Value| -> Vec<String> {
        json["views"]["impact"]["hits"]
            .as_array()
            .expect("hits array")
            .iter()
            .map(|hit| hit["id"].as_str().expect("hit id").to_string())
            .collect()
    };
    assert_eq!(ids(&all), ["src/lib.rs#function:caller"], "{all:#}");
    assert!(ids(&import_only).is_empty(), "{import_only:#}");
}

#[test]
fn lang_filters_find_all_and_reports_the_scan_filter() {
    let repo = repo(&[("src/lib.rs", LIB_RS), ("tool.py", TOOL_PY)]);

    let json = map_json(
        repo.path(),
        &["--find-all", "target", "--lang", "python", "--json"],
    );

    let matches = json["views"]["find_all"]["matches"]
        .as_array()
        .expect("matches array");
    assert_eq!(matches.len(), 1, "{json:#}");
    assert_eq!(matches[0]["path"], "tool.py");
    let lang = &json["views"]["find_all"]["filters"]["lang"];
    assert_eq!(lang["value"], "python");
    assert_eq!(lang["applied"], "scan");
    assert_eq!(lang["filtered_out"], 1);
}

#[test]
fn unknown_language_is_rejected_before_any_work() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);

    let output = run_map(repo.path(), &["--find-all", "target", "--lang", "cobol"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("valid languages"));
}

#[test]
fn window_refuses_ids_the_graph_does_not_name() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);
    // The local overlay indexes untracked files, so the file inside the root is
    // git-ignored to keep it out of the graph.
    fs::write(repo.path().join(".gitignore"), "notes.txt\n").expect("write .gitignore");
    fs::write(repo.path().join("notes.txt"), "private\n").expect("write ignored file");

    // An absolute path outside the root, and a file inside the root that the
    // graph never indexed.
    for id in ["/etc/hostname@0-5", "notes.txt@0-5"] {
        let output = run_map(repo.path(), &["--window", id]);

        assert_eq!(output.status.code(), Some(2), "{id}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unknown id"),
            "{id}: {output:?}"
        );
        assert!(output.stdout.is_empty(), "{id}: {output:?}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn timings_report_a_positive_peak_rss() {
    let repo = repo(&[("src/lib.rs", LIB_RS)]);
    let args = ["--find-all", "target", "--json", "--timings"];

    let json = map_json(repo.path(), &args);

    let peak = json["timings"]["peak_rss_kb"].as_u64();
    assert!(peak.is_some_and(|kb| kb > 0), "{json:#}");
}

#[test]
fn census_of_another_root_takes_no_snapshot_of_this_project() {
    let project = repo(&[("src/lib.rs", LIB_RS)]);
    let other = repo(&[("b.rs", "pub fn b() {}\n")]);
    let other_root = other.path().to_str().expect("utf-8 temp path");
    let cache = project.path().join(".loom/cache/context-v1");

    let alone = map_json(
        project.path(),
        &["--census", "--root", other_root, "--json"],
    );

    let roots = alone["roots"].as_array().expect("roots array");
    assert_eq!(roots.len(), 1, "{alone:#}");
    assert_eq!(roots[0]["coverage_source"], "in-memory", "{alone:#}");
    assert!(
        !cache.exists(),
        "a snapshot of the current project was taken"
    );

    // Control: a census that includes this project reads its graph.
    let own = map_json(project.path(), &["--census", "--json"]);
    assert_eq!(own["roots"][0]["coverage_source"], "graph", "{own:#}");
    assert!(cache.exists());
}
