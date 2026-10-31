//! Contracts for `loom plan check`: dispatch, the pinned JSON schema and the exit codes.

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const SCHEMA: &str = include_str!("fixtures/planning/check_schema.json");
const PLAN: &str = "doc/plans/alpha/PLAN-alpha.md";
const SECTION: &str = "doc/product/alpha/section_01.md";
const VISION: &str = "doc/product/alpha/index.md";
const METAPLAN: &str = "doc/plans/alpha/index.md";

fn tmp() -> (TempDir, PathBuf) {
    let builder = tempfile::Builder::new().prefix("loom-plan-check-").clone();
    let temp = builder.tempdir_in(std::env::temp_dir()).unwrap();
    let path = temp.path().to_path_buf();
    (temp, path)
}

fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

/// A git repository (no commit) holding `files`.
fn repo(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let (temp, root) = tmp();
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status();
    assert!(init.unwrap().success(), "git init");
    for (rel, text) in files {
        put(&root, rel, text);
    }
    (temp, root)
}

/// Two features and a standalone plan, written out of feature-name order.
fn clean() -> (TempDir, PathBuf) {
    repo(&[
        (
            "doc/plans/beta/PLAN-beta.md",
            "# Plan: Beta\n\nImplements: none\n",
        ),
        ("doc/plans/IN_PROGRESS-PLAN-solo.md", "# Plan: Solo\n"),
        (
            VISION,
            "# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Workflow |\n",
        ),
        (SECTION, "# Section 01\n\nThe workflow.\n"),
        (METAPLAN, "# Metaplan\n\n## 1. Alpha (`PLAN-alpha.md`)\n"),
        (PLAN, "# Plan: Alpha\n\nImplements: section_01\n"),
    ])
}

/// Runs `loom plan check --json` in `dir` with a scratch `LOOM_HOME` and no
/// other inherited `LOOM_*` variable.
fn check(dir: &Path, envs: &[(&str, &Path)]) -> Output {
    run_check(dir, true, envs)
}

fn run_check(dir: &Path, json: bool, envs: &[(&str, &Path)]) -> Output {
    let home = TempDir::new().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command.args(["plan", "check"]).current_dir(dir);
    if json {
        command.arg("--json");
    }
    command
        .env("LOOM_HOME", home.path())
        .envs(envs.iter().copied());
    command.output().expect("run loom plan check")
}

fn stdout_json(output: &Output) -> Value {
    let text = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: stdout {text} stderr {stderr}"))
}

/// Pin stable checklist details and every new schema-2 field; normalize the temp root.
fn projection(out: &Value) -> Value {
    let features: Vec<Value> = out["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|feature| {
            let checklist = &feature["checklist"];
            let secondary: Vec<_> = checklist["secondary"]
                .as_array()
                .unwrap()
                .iter()
                .map(|offer| &offer["id"])
                .collect();
            json!({
                "name": feature["name"],
                "checklist": {
                    "feature": checklist["feature"],
                    "current": checklist["current"],
                    "steps": checklist["steps"],
                    "primary": checklist["primary"]["id"],
                    "secondary": secondary,
                },
            })
        })
        .collect();
    let problems: Vec<Value> = out["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| json!({ "code": p["code"], "path": p["path"], "line": p["line"] }))
        .collect();
    json!({
        "schema": out["schema"],
        "root": "<repo>",
        "features": features,
        "standalone": out["standalone"],
        "moves": out["moves"],
        "problems": problems,
    })
}

fn assert_shape(out: &Value) {
    let keys =
        |value: &Value| -> Vec<String> { value.as_object().unwrap().keys().cloned().collect() };
    assert_eq!(
        keys(out),
        [
            "features",
            "moves",
            "problems",
            "root",
            "schema",
            "standalone"
        ]
    );
    assert_eq!(out["schema"], 2);
    assert!(out["root"].is_string());
    for field in ["features", "standalone", "moves", "problems"] {
        assert!(out[field].is_array(), "{field}: {out:#}");
    }
    for feature in out["features"].as_array().unwrap() {
        assert_eq!(keys(feature), ["checklist", "name"]);
        assert!(feature["name"].is_string());
        let checklist = &feature["checklist"];
        assert_eq!(
            keys(checklist),
            [
                "current",
                "feature",
                "items",
                "primary",
                "secondary",
                "state",
                "steps"
            ]
        );
        assert_eq!(checklist["feature"], feature["name"]);
        assert_eq!(checklist["steps"].as_array().unwrap().len(), 6);
        assert!(checklist["items"].is_array());
        assert!(checklist["state"].is_object());
    }
    for plan in out["standalone"].as_array().unwrap() {
        assert_eq!(keys(plan), ["path", "slug", "state"]);
        assert!(plan["slug"].is_string());
        assert!(plan["path"].is_string());
        assert!(matches!(
            plan["state"].as_str(),
            Some("written" | "running" | "done" | "retired")
        ));
    }
}

fn codes(out: &Value) -> Vec<String> {
    let problems = out["problems"].as_array().cloned().unwrap_or_default();
    let mut codes: Vec<String> = problems
        .iter()
        .filter_map(|p| p["code"].as_str().map(str::to_string))
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

#[test]
fn plan_check_json_cli() {
    let (_temp, root) = repo(&[
        ("doc/plans/beta/PLAN-beta.md", "# Plan: Beta\n"),
        (PLAN, "# Plan: Alpha\n\nImplements: none\n"),
        ("doc/plans/IN_PROGRESS-PLAN-solo.md", "# Plan: Solo\n"),
        ("doc/product/index.md", "# Vision\n"),
    ]);
    let output = check(&root, &[]);
    let out = stdout_json(&output);
    assert_shape(&out);
    assert_eq!(out["root"], json!(root.canonicalize().unwrap()));
    assert_eq!(output.status.code(), Some(1), "{out:#}");
    let want: Value = serde_json::from_str(SCHEMA).expect("check_schema.json parses");
    assert_eq!(projection(&out), want, "{out:#}");
}

#[test]
fn plan_check_clean_exits_zero() {
    let (_temp, root) = clean();
    let output = check(&root, &[]);
    let out = stdout_json(&output);
    assert_shape(&out);
    assert_eq!(out["problems"], json!([]), "{out:#}");
    assert_eq!(output.status.code(), Some(0), "{out:#}");
}

#[test]
fn plan_check_human_groups_features_then_standalone_then_problems() {
    let (_temp, root) = clean();
    put(&root, "doc/product/index.md", "# Vision\n");

    let output = run_check(&root, false, &[]);
    let text = String::from_utf8(output.stdout).unwrap();

    assert_eq!(output.status.code(), Some(1), "{text}");
    let (alpha, rest) = text.split_once("beta\n").unwrap();
    let (beta, rest) = rest.split_once("standalone plans\n").unwrap();
    assert!(alpha.starts_with("alpha\n1 Vision: Done\n"), "{text}");
    assert!(alpha.contains("Primary: pressure-test:alpha\n"), "{text}");
    assert!(beta.starts_with("1 Vision: Gap\n"), "{text}");
    assert!(beta.contains("Primary: start-vision\n"), "{text}");
    assert!(!alpha.contains("pressure-test:beta"), "{text}");
    assert!(!beta.contains("pressure-test:alpha"), "{text}");
    assert!(rest.starts_with("  solo: Running\n"), "{text}");
    assert!(
        rest.contains("doc/product/index.md: needs-feature:"),
        "{text}"
    );
    assert!(!rest.contains("Current:"), "{text}");
}

#[test]
fn plan_check_repository_docs_follow_shape_and_exit_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = check(root, &[]);
    let out = stdout_json(&output);

    assert_shape(&out);
    let expected = if out["problems"].as_array().unwrap().is_empty() {
        0
    } else {
        1
    };
    assert_eq!(output.status.code(), Some(expected), "{out:#}");
}

#[test]
fn plan_check_outside_repo() {
    let (temp, _) = tmp();
    let dir = temp.path().join("outside");
    fs::create_dir_all(&dir).unwrap();
    let output = check(&dir, &[("GIT_CEILING_DIRECTORIES", temp.path())]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    let canonical = dir.canonicalize().unwrap();
    let named = [&dir, &canonical]
        .iter()
        .any(|path| stderr.contains(&path.display().to_string()));
    assert!(named, "stderr names the directory: {stderr}");
}

/// Defects made by writing one file: the code it must raise, the file and its new text.
const WRITES: [(&str, &str, &str); 12] = [
    (
        "unlisted-section",
        "doc/product/alpha/section_09.md",
        "# Section 09\n\nBody.\n",
    ),
    (
        "missing-title",
        PLAN,
        "Plan alpha.\n\nImplements: section_01\n",
    ),
    (
        "metaplan-entry-without-plan",
        METAPLAN,
        "# Metaplan\n\n## 1. Alpha (`PLAN-alpha.md`)\n\n## 2. Missing name\n",
    ),
    ("implements-missing", PLAN, "# Plan: Alpha\n\nNo line.\n"),
    (
        "implements-duplicate",
        PLAN,
        "# Plan: Alpha\n\nImplements: section_01\nImplements: section_01\n",
    ),
    (
        "implements-broken",
        PLAN,
        "# Plan: Alpha\n\nImplements: section_42\n",
    ),
    (
        "broken-reference",
        SECTION,
        "# Section 01\n\nSee [missing](section_07.md).\n",
    ),
    (
        "broken-brief-reference",
        PLAN,
        concat!(
            "# Plan: Alpha\n\nImplements: section_01\n\nBrief: `doc/plans/alpha/",
            "briefs/alpha/w1.md`.\n"
        ),
    ),
    (
        "stray-plan-copy",
        "doc/plans/alpha/DONE-PLAN-alpha.md",
        "# Plan: Alpha\n\nImplements: section_01\n",
    ),
    ("needs-feature", "doc/product/index.md", "# Vision\n"),
    (
        "bad-feature-name",
        "doc/product/Bad_Name/index.md",
        "# Vision\n",
    ),
    (
        "duplicate-plan-slug",
        "doc/plans/beta/PLAN-alpha.md",
        "# Plan: Alpha copy\n\nImplements: none\n",
    ),
];

/// Turns the clean fixture into one with the defect `code` names.
fn make_defect(code: &str, root: &Path) {
    match code {
        "vision-table-unreadable" => {
            let text = "# Vision\n\n## Specification sections\n\nSee the list.\n";
            put(root, VISION, text);
            fs::remove_file(root.join(SECTION)).unwrap();
            put(root, PLAN, "# Plan: Alpha\n\nImplements: none\n");
        }
        "doc-missing" => {
            fs::rename(root.join("doc"), root.join("real-doc")).unwrap();
            symlink(root.join("real-doc"), root.join("doc")).unwrap();
        }
        _ => {
            let (_, rel, text) = WRITES.iter().find(|(c, _, _)| *c == code).unwrap();
            put(root, rel, text);
        }
    }
}

#[test]
fn plan_check_exits_nonzero_per_problem() {
    let special = ["vision-table-unreadable", "doc-missing"];
    for code in special.into_iter().chain(WRITES.map(|(code, _, _)| code)) {
        // The missing brief is also a path token that resolves to no document.
        let also: &[&str] = match code {
            "broken-brief-reference" => &["broken-reference"],
            _ => &[],
        };
        let (_temp, root) = clean();
        make_defect(code, &root);
        let output = check(&root, &[]);
        let out = stdout_json(&output);
        assert_eq!(output.status.code(), Some(1), "{code}: {out:#}");
        let found = codes(&out);
        assert!(found.iter().any(|c| c == code), "{code}: {found:?}");
        let extra = found
            .iter()
            .filter(|c| *c != code && !also.contains(&c.as_str()));
        let extra: Vec<&String> = extra.collect();
        assert!(extra.is_empty(), "{code}: unexpected codes {extra:?}");
    }
}
