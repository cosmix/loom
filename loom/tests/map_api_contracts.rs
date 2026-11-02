//! Contracts for stage map-api-freshness: the `loom map` API, freshness states,
//! census and source windows.
//!
//! Each test pins one rule of `doc/plans/briefs/source-graph-mechanism/design.md`
//! (sections 9, 10, 10.1 and 11).

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;
use crate::helpers::test_support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use loom::commands::hook::reconcile_graph::wants_rebuild;
use loom::context::freshness::Freshness;
use loom::context::source_graph::GRAPH_SCHEMA_VERSION;
use serde_json::Value;
use tempfile::{Builder, TempDir};

const LIB_RS: &str = "pub fn target() {}\npub fn caller() {\n    let _x = 1;\n    target();\n}\n";

fn scratch() -> TempDir {
    Builder::new()
        .prefix("loom-map-api-contracts-")
        .tempdir_in(std::env::temp_dir())
        .expect("create unique scratch directory")
}

fn git_output(root: &Path, args: &[&str]) -> Output {
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

fn git(root: &Path, args: &[&str]) {
    git_output(root, args);
}

/// Initialises `root` as a git repo, writes and commits `files`, then creates
/// the untracked `.loom/work` state dir so loom resolves `root` as the project.
fn commit_repo(root: &Path, files: &[(&str, &str)]) {
    fs::create_dir_all(root).expect("create repo dir");
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
}

fn lib_repo() -> TempDir {
    let temp = scratch();
    commit_repo(temp.path(), &[("src/lib.rs", LIB_RS)]);
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

/// Runs `loom map <args>`, requires exit 0 and parses stdout as ONE JSON object.
fn map_json(root: &Path, args: &[&str]) -> Value {
    parse_json(args, &run_map(root, args))
}

fn parse_json(args: &[&str], output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "loom map {args:?} exited {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );
    let value: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("stdout of loom map {args:?} is not one JSON object ({e}): {stdout}")
    });
    assert!(value.is_object(), "not a JSON object: {value}");
    value
}

#[test]
fn callers_json_reports_call_site_lines() {
    let repo = lib_repo();
    let json = map_json(repo.path(), &["--callers", "target", "--json"]);

    let neighbor = &json["views"]["callers"]["neighbors"][0];
    assert_eq!(
        neighbor["id"], "src/lib.rs#function:caller",
        "callers row: {json:#}"
    );
    assert_eq!(
        neighbor["line_start"], 2,
        "line_start is the caller's declaration line: {neighbor:#}"
    );
    let sites = neighbor["sites"]
        .as_array()
        .unwrap_or_else(|| panic!("callers row has no sites array: {neighbor:#}"));
    assert!(
        !sites.is_empty(),
        "callers row has an empty sites array: {neighbor:#}"
    );
    assert_eq!(
        sites[0]["line_start"], 4,
        "sites[0].line_start is the call line: {neighbor:#}"
    );
}

#[test]
fn map_json_carries_schema_and_snapshot() {
    let repo = lib_repo();
    let head_output = git_output(repo.path(), &["rev-parse", "HEAD"]);
    let head = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_string();
    let json = map_json(repo.path(), &["--find-all", "target", "--json"]);

    assert_eq!(json["schema"], "loom-map/2", "top level: {json:#}");
    let snapshot = &json["snapshot"];
    assert!(snapshot.is_object(), "no snapshot object: {json:#}");
    assert_eq!(snapshot["state"], "current", "snapshot: {snapshot:#}");
    assert_eq!(
        snapshot["base_revision"].as_str(),
        Some(head.as_str()),
        "base_revision must be HEAD: {snapshot:#}"
    );
    assert_eq!(
        snapshot["schema_version"].as_u64(),
        Some(u64::from(GRAPH_SCHEMA_VERSION)),
        "schema_version must be GRAPH_SCHEMA_VERSION: {snapshot:#}"
    );
}

#[test]
fn impact_labels_substring_fallback() {
    let repo = lib_repo();

    let fallback = map_json(repo.path(), &["--impact", "targ", "--json"]);
    assert_eq!(
        fallback["views"]["impact"]["match"], "substring",
        "targ names no symbol exactly: {fallback:#}"
    );

    let exact = map_json(repo.path(), &["--impact", "target", "--json"]);
    assert_eq!(
        exact["views"]["impact"]["match"], "exact",
        "target names a symbol exactly: {exact:#}"
    );
}

#[test]
fn map_timings_json_reports_phases_and_peak_rss() {
    let repo = lib_repo();
    let args = ["--find-all", "target", "--json", "--timings"];
    let output = run_map(repo.path(), &args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json = parse_json(&args, &output);

    let timings = json["timings"]
        .as_object()
        .unwrap_or_else(|| panic!("no timings object: {json:#}"));
    for phase in [
        "snapshot",
        "load",
        "resolve",
        "query",
        "render",
        "total",
        "peak_rss_kb",
    ] {
        assert!(
            timings.get(phase).is_some_and(Value::is_number),
            "timings.{phase} is missing or not numeric: {timings:#?}"
        );
    }
    assert!(
        stderr.contains("total"),
        "--timings prints phases on stderr: {stderr}"
    );
}

#[test]
fn never_built_graph_does_not_request_rebuild() {
    for degraded in [None, Some("degraded")] {
        assert!(
            !wants_rebuild(&Freshness::never_built("x"), degraded),
            "a never-built graph must not request a rebuild (degraded {degraded:?})"
        );
        assert!(
            !wants_rebuild(&Freshness::unavailable("x"), degraded),
            "an unavailable graph must not request a rebuild (degraded {degraded:?})"
        );
    }

    let stale = Freshness {
        revision: "abc".into(),
        stale: true,
        ..Default::default()
    };
    assert!(
        wants_rebuild(&stale, None),
        "a stale graph requests a rebuild"
    );

    let current = Freshness {
        revision: "abc".into(),
        stale: false,
        ..Default::default()
    };
    assert!(
        wants_rebuild(&current, Some("degraded")),
        "a current but degraded graph requests a rebuild"
    );
    assert!(
        !wants_rebuild(&current, None),
        "a current, healthy graph requests nothing"
    );
}

fn class_totals(json: &Value, class: &str) -> (u64, u64) {
    let entry = &json["totals"][class];
    let files = entry["files"]
        .as_u64()
        .unwrap_or_else(|| panic!("totals.{class}.files missing: {json:#}"));
    let bytes = entry["bytes"]
        .as_u64()
        .unwrap_or_else(|| panic!("totals.{class}.bytes missing: {json:#}"));
    (files, bytes)
}

#[test]
fn census_reports_exclusions_beside_eligible() {
    let temp = scratch();
    let root = temp.path();
    commit_repo(
        root,
        &[
            ("src/a.rs", "pub fn a() {}\n"),
            ("vendor/v.rs", "pub fn v() {}\n"),
            ("src/gen.rs", "// @generated\npub fn g() {}\n"),
            ("notes.xyz", "hello\n"),
        ],
    );
    let json = map_json(root, &["--census", "--json"]);

    assert_eq!(
        class_totals(&json, "eligible"),
        (1, 14),
        "eligible: {json:#}"
    );
    assert_eq!(
        class_totals(&json, "vendored"),
        (1, 14),
        "vendored: {json:#}"
    );
    assert_eq!(
        class_totals(&json, "generated"),
        (1, 28),
        "generated: {json:#}"
    );
    assert_eq!(
        class_totals(&json, "unsupported"),
        (1, 6),
        "unsupported: {json:#}"
    );
    assert_eq!(
        class_totals(&json, "excluded"),
        (0, 0),
        "excluded: {json:#}"
    );
}

#[test]
fn census_handles_thousands_of_paths() {
    const FILES: usize = 3000;
    let temp = scratch();
    let root = temp.path();
    let deep: Vec<String> = (0..13)
        .map(|level| format!("d{level:02}{}", "x".repeat(57)))
        .collect();
    let dir = deep.join("/");
    let paths: Vec<String> = (0..FILES).map(|n| format!("{dir}/f{n}.rs")).collect();
    let total_path_bytes: usize = paths.iter().map(String::len).sum();
    assert!(
        total_path_bytes > 2 * 1024 * 1024,
        "fixture paths total {total_path_bytes} bytes, not above ARG_MAX"
    );
    let files: Vec<(&str, &str)> = paths
        .iter()
        .map(|path| (path.as_str(), "pub fn f() {}\n"))
        .collect();
    commit_repo(root, &files);

    let json = map_json(root, &["--census", "--json"]);

    let (eligible, _) = class_totals(&json, "eligible");
    assert_eq!(eligible, FILES as u64, "totals: {:#}", json["totals"]);
}

#[test]
fn window_rejects_ids_outside_the_graph() {
    let outer = scratch();
    let repo = outer.path().join("repo");
    commit_repo(&repo, &[("src/lib.rs", LIB_RS)]);
    fs::write(outer.path().join("outside.txt"), "SECRET-MARKER\n").expect("write outside file");

    let output = run_map(&repo, &["--window", "../outside.txt@0-5"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(2),
        "an unknown id exits 2\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stderr.contains("unknown id"), "stderr: {stderr}");
    for (stream, text) in [("stdout", &stdout), ("stderr", &stderr)] {
        assert!(
            !text.contains("SECRE") && !text.contains("MARKER"),
            "{stream} leaked bytes of a file outside the graph: {text}"
        );
    }
}
