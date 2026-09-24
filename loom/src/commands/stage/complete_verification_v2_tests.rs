//! [`run_v2`] for an integration-verify stage re-verifies every completed
//! stage's `reachable` checks on the merged tree: a real git repository with
//! no published base layer, so its graph is extracted from scratch.

use super::super::VerificationChecks;
use super::run_v2;
use crate::git::runner::run_git_checked;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::save_stage;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// The merged tree: `main` calls `run`; nothing calls `orphan` any more.
const MAIN_RS: &str = "fn main() {\n    run();\n}\n\nfn run() {}\n\nfn orphan() {}\n";

/// Each completed stage and the symbol its `reachable` check names.
const COMPLETED: [(&str, &str); 2] = [("wire-run", "run"), ("wire-orphan", "orphan")];

const IV_STAGE: &str = "integration-verify";

fn git(dir: &Path, args: &[&str]) {
    let identity = [
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
    ];
    run_git_checked(&[&identity[..], args].concat(), dir).unwrap();
}

/// The merged worktree on `main` with everything committed, plus a `docs`
/// directory standing in for the integration stage's resolved
/// `acceptance_dir`: a graph built from there would find no symbol at all.
fn merged_worktree() -> TempDir {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("src/main.rs"), MAIN_RS).unwrap();
    fs::write(root.join("docs/README.md"), "# Docs\n").unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "src/main.rs", "docs/README.md"]);
    git(root, &["commit", "-q", "-m", "merged"]);
    repo
}

/// A v2 `standard` stage whose one `reachable` check names `symbol`.
fn stage_yaml(id: &str, symbol: &str) -> String {
    format!(
        "    - id: {id}
      name: \"{id}\"
      stage_type: standard
      working_dir: \".\"
      dependencies: []
      contracts:
        - id: {id}-runs
          file: \"tests/{id}.rs\"
          test: \"runs\"
          runner: cargo-test
          scenario: \"the program starts\"
          rejects: \"a program that never calls {symbol}\"
      reachable:
        - symbol: {symbol}
          from: main
          description: \"{symbol} is wired into main\"
      acceptance:
        - \"cargo test\"
"
    )
}

/// A work directory whose plan holds the completed stages and the
/// integration stage, with each completed stage recorded as `Completed`.
fn work_dir_with_plan() -> TempDir {
    let work = TempDir::new().unwrap();
    let stages: String = COMPLETED
        .iter()
        .map(|&(id, symbol)| stage_yaml(id, symbol))
        .collect();
    let plan = format!(
        "# PLAN: Reachable re-verification

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
{stages}    - id: {IV_STAGE}
      name: \"Integration\"
      stage_type: integration-verify
      working_dir: \".\"
      dependencies: [\"wire-run\", \"wire-orphan\"]
      acceptance:
        - \"cargo test\"
```

<!-- END loom METADATA -->
"
    );
    let plan_path = work.path().join("PLAN-reachable.md");
    fs::write(&plan_path, plan).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"reachable\"\nplan_name = \"Reachable\"\n\
         base_branch = \"main\"\n",
        plan_path.display()
    );
    fs::write(work.path().join("config.toml"), config).unwrap();
    for (id, _) in COMPLETED {
        let stage = Stage {
            id: id.to_string(),
            name: id.to_string(),
            status: StageStatus::Completed,
            plan_version: 2,
            ..Stage::default()
        };
        save_stage(&stage, work.path()).unwrap();
    }
    work
}

#[test]
fn aggregated_reachable_reverified_in_iv() {
    let (repo, work) = (merged_worktree(), work_dir_with_plan());
    let iv = Stage {
        id: IV_STAGE.to_string(),
        name: "Integration".to_string(),
        status: StageStatus::Executing,
        stage_type: StageType::IntegrationVerify,
        plan_version: 2,
        ..Stage::default()
    };
    let acceptance_dir = repo.path().join("docs");
    let checks = VerificationChecks {
        stage: &iv,
        stage_id: IV_STAGE,
        acceptance_dir: Some(acceptance_dir.as_path()),
        worktree_root: Some(repo.path()),
        control_session: None,
        work_dir: work.path(),
    };

    let error = run_v2(&checks, "main").expect_err("an unreachable symbol must fail completion");
    let message = format!("{error:#}");
    assert!(
        message.contains("stage 'wire-orphan': orphan is not reachable from main"),
        "{message}"
    );
    assert!(!message.contains("wire-run"), "{message}");
}
