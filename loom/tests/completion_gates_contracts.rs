//! Contracts for stage completion-gates: impact selection from the stage's
//! merge base, criterion disputes on every criterion list, and the contract
//! freeze's formatter gate.
//!
//! The surface is `loom::verify::impact_tests::{stage_changes, run}`,
//! `loom::skills::project::ProjectProfile::package_details`,
//! `loom::models::dispute::{CriterionField, DisputeKind}`,
//! `loom::daemon::handle_dispute_criteria`,
//! `loom::orchestrator::adjudication::verdict::parse_and_validate_for` and
//! `loom::verify::contracts::format_gate::format_problems`.

use std::fs;
use std::path::{Path, PathBuf};

use loom::daemon::{handle_dispute_criteria, Response};
use loom::fs::work_dir::WorkDir;
use loom::models::dispute::{
    request_file, CriterionField, DisputeKind, DisputeRequest, DisputeVerdict,
};
use loom::models::stage::{
    AcceptanceCriterion, CommandConfinement, Stage, StageStatus, WiringCheck,
};
use loom::orchestrator::adjudication::verdict::{parse_and_validate_for, ValidationOutcome};
use loom::skills::project::ProjectProfile;
use loom::verify::contracts::format_gate::format_problems;
use loom::verify::criteria::CriteriaConfig;
use loom::verify::impact_tests::{run, stage_changes};
use loom::verify::transitions::{load_stage, save_stage};
use tempfile::TempDir;

/// Git isolated from the host's global and system configuration.
fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// An empty repository on branch `main`, returned with its canonical root.
fn repo() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    git(&root, &["init", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.com"]);
    git(&root, &["config", "user.name", "t"]);
    (temp, root)
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(root, &["commit", "-m", message]);
}

#[test]
fn impact_selection_uses_the_stage_merge_base() {
    let (_temp, root) = repo();
    write(&root, "src/lib.rs", "pub fn a() {}\n");
    write(&root, "src/other.rs", "pub fn b() {}\n");
    write(&root, "web/a.test.ts", "export const a = 1;\n");
    commit(&root, "A");
    write(&root, "web/a.test.ts", "export const a = 2;\n");
    commit(&root, "B: merged before the stage branched");

    git(&root, &["checkout", "-b", "loom/s"]);
    write(&root, "src/lib.rs", "pub fn a() { let _ = 1; }\n");
    commit(&root, "C: the stage's own commit");
    write(&root, "src/other.rs", "pub fn b() { let _ = 2; }\n");
    write(&root, "src/new.rs", "pub fn c() {}\n");

    let changed = stage_changes(&root, "main").unwrap();
    assert_eq!(
        changed,
        vec![
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/new.rs"),
            PathBuf::from("src/other.rs"),
        ],
        "the stage changed exactly what it committed, edited and created since \
         `git merge-base HEAD main`"
    );
}

#[test]
fn js_runner_without_node_modules_is_a_note() {
    let (_temp, root) = repo();
    write(&root, "README.md", "probe\n");
    commit(&root, "seed");
    write(
        &root,
        "web/package.json",
        r#"{"devDependencies":{"vitest":"^3.2.0"}}"#,
    );
    write(
        &root,
        "web/src/a.test.ts",
        "import { test } from \"vitest\";\n\
         function boom() { throw new Error(\"boom\") }\n\
         test(\"x\", boom);\n",
    );

    let outcome = run(&Stage::default(), &root, &CriteriaConfig::default(), "main")
        .unwrap_or_else(|e| panic!("a package with no node_modules must not fail: {e:#}"));
    assert!(
        outcome.ran.is_empty(),
        "nothing may run without node_modules: {:?}",
        outcome.ran
    );
    assert!(
        outcome
            .notes
            .iter()
            .any(|note| note.contains("has no node_modules") && note.contains("web")),
        "expected a note that `web` has no node_modules: {:?}",
        outcome.notes
    );
}

#[test]
fn fixture_directories_hold_no_packages() {
    let (_temp, root) = repo();
    let cargo = |name: &str| format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    write(&root, "Cargo.toml", &cargo("root"));
    write(
        &root,
        "tests/fixtures/labeled/rust/Cargo.toml",
        &cargo("labeled"),
    );
    write(
        &root,
        "tests/fixtures/labeled/go/go.mod",
        "module example.com/labeled\n\ngo 1.22\n",
    );
    write(&root, "fixtures/app/package.json", r#"{"name":"app"}"#);
    write(&root, "tests/fixtures-extra/pkg/Cargo.toml", &cargo("pkg"));

    let mut paths: Vec<PathBuf> = ProjectProfile::discover(&root)
        .package_details()
        .into_iter()
        .map(|detail| detail.path)
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![PathBuf::from(""), PathBuf::from("tests/fixtures-extra/pkg")],
        "a directory named `fixtures` holds no package; a name merely containing it does"
    );
}

/// An initialised work dir holding `stages`, returned with its root.
fn work_dir(stages: &[Stage]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let wd = WorkDir::new(tmp.path()).unwrap();
    wd.initialize().unwrap();
    let root = wd.root().to_path_buf();
    for stage in stages {
        save_stage(stage, &root).unwrap();
    }
    (tmp, root)
}

fn stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_uppercase(),
        status,
        acceptance: vec![AcceptanceCriterion::Simple("echo 0".to_string())],
        ..Stage::default()
    }
}

fn wiring(description: &str) -> WiringCheck {
    WiringCheck {
        source: "src/lib.rs".to_string(),
        pattern: "pub fn".to_string(),
        description: description.to_string(),
        literal: true,
    }
}

/// The YAML frontmatter of a `request.md`: the text between its first two `---` lines.
fn read_request(path: &Path) -> DisputeRequest {
    let content =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut lines = content
        .lines()
        .skip_while(|line| line.trim() != "---")
        .skip(1);
    let frontmatter: Vec<&str> = lines
        .by_ref()
        .take_while(|line| line.trim() != "---")
        .collect();
    loom::yaml::from_str(&frontmatter.join("\n"))
        .unwrap_or_else(|e| panic!("parse {}: {e}\n{content}", path.display()))
}

fn no_request_written(work: &Path, stage_id: &str) -> bool {
    let dir = work.join("disputes").join(stage_id);
    let Ok(entries) = fs::read_dir(&dir) else {
        return true;
    };
    entries
        .flatten()
        .all(|entry| !entry.path().join("request.md").exists())
}

#[test]
fn wiring_dispute_indexes_the_wiring_list() {
    let s1 = Stage {
        wiring: vec![wiring("first"), wiring("second")],
        ..stage("s1", StageStatus::Executing)
    };
    let s2 = stage("s2", StageStatus::Executing);
    let (_tmp, work) = work_dir(&[s1, s2]);
    let disputes = work.join("disputes");

    let filed = handle_dispute_criteria(
        &work,
        "s1",
        CriterionField::Wiring,
        1,
        "the wiring pattern is wrong".to_string(),
        None,
        None,
    )
    .unwrap();
    let id = match filed {
        Response::DisputeCreated { id } => id,
        other => panic!("wiring index 1 of two wiring checks must file: {other:?}"),
    };
    let request = read_request(&request_file(&disputes, "s1", id));
    assert_eq!(
        request.kind,
        DisputeKind::criterion(CriterionField::Wiring, 1)
    );

    let refused = handle_dispute_criteria(
        &work,
        "s2",
        CriterionField::WiringTests,
        0,
        "the wiring test is wrong".to_string(),
        None,
        None,
    )
    .unwrap();
    match refused {
        Response::Error { message } => assert!(
            message.contains("out of range"),
            "a stage with no wiring tests must refuse wiring-tests index 0: {message}"
        ),
        other => panic!("wiring-tests index 0 on a stage with none must refuse: {other:?}"),
    }
    assert!(
        no_request_written(&work, "s2"),
        "a refused dispute leaves no request.md"
    );
}

/// An accept verdict whose plan patch replaces entry 0 of `field`.
fn accept_amending(field: &str) -> String {
    serde_json::json!({
        "verdict": "accept",
        "plan_patch": {
            "field": field,
            "patch": {
                "op": "replace",
                "index": 0,
                "value": "name: t\ncommand: \"true\"\nsuccess_criteria:\n  exit_code: 0\n"
            },
            "reason": "r"
        },
        "citations": [{
            "file": "src/lib.rs",
            "line": 1,
            "excerpt": "pub fn a() {}",
            "claim": "the wiring test checks the wrong thing"
        }],
        "reasoning": "the disputed wiring test cannot pass against a correct implementation"
    })
    .to_string()
}

#[test]
fn wiring_tests_verdict_must_amend_wiring_tests() {
    let kind = DisputeKind::criterion(CriterionField::WiringTests, 0);

    match parse_and_validate_for(&accept_amending("wiring-tests"), &kind) {
        ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) => assert_eq!(
            plan_patch.inner.get("field").and_then(|f| f.as_str()),
            Some("wiring-tests"),
            "the accepted patch amends the wiring-tests list: {:?}",
            plan_patch.inner
        ),
        other => panic!("a wiring-tests accept amending wiring-tests must stand: {other:?}"),
    }

    match parse_and_validate_for(&accept_amending("acceptance"), &kind) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { .. }) => {}
        other => {
            panic!("a wiring-tests dispute's verdict may not amend the acceptance list: {other:?}")
        }
    }
}

#[test]
fn refused_dispute_writes_no_request() {
    let (_tmp, work) = work_dir(&[stage("s3", StageStatus::Completed)]);

    let answer = handle_dispute_criteria(
        &work,
        "s3",
        CriterionField::Acceptance,
        0,
        "the criterion is wrong".to_string(),
        None,
        None,
    );
    if let Ok(Response::DisputeCreated { id }) = answer {
        panic!("a Completed stage cannot move to NeedsAdjudication, yet dispute {id} was filed");
    }
    assert!(
        !request_file(&work.join("disputes"), "s3", 1).exists() && no_request_written(&work, "s3"),
        "a refused dispute leaves no open request.md"
    );
    assert_eq!(
        load_stage("s3", &work).unwrap().dispute_count,
        0,
        "a refused filing spends no dispute budget"
    );
}

#[test]
fn freeze_refuses_unformatted_contract_files() {
    let temp = TempDir::new().unwrap();
    let crate_dir = temp.path().canonicalize().unwrap();
    write(
        &crate_dir,
        "Cargo.toml",
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    );
    write(
        &crate_dir,
        "src/lib.rs",
        "pub fn  add(a:u32,b:u32)->u32{a+b}\n",
    );

    let acceptance = [
        AcceptanceCriterion::Simple("cargo fmt --check".to_string()),
        AcceptanceCriterion::Simple("false".to_string()),
    ];
    let problems =
        format_problems(&acceptance, &[], &crate_dir, CommandConfinement::Confined).unwrap();
    assert_eq!(
        problems.len(),
        1,
        "only the formatter check runs, and it fails: {problems:?}"
    );
    assert!(
        problems[0].contains("cargo fmt --check"),
        "the problem names its command: {problems:?}"
    );
}
