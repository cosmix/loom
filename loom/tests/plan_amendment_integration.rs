//! File-locking integration tests for `plan::amendment::apply_amendment`.
//!
//! These run in a separate integration-test binary (see Cargo's default
//! `tests/` discovery) so they can spawn multiple threads driving the
//! amendment path concurrently. The plan-versions flock must serialise
//! these calls and assign strictly-monotonic version numbers.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use tempfile::TempDir;

use loom::models::stage::Stage;
use loom::plan::amendment::{
    apply_amendment, count_amendments_for_stage, plan_versions_dir, AmendmentField,
    AmendmentPatch, AmendmentRequest,
};
use loom::plan::schema::AcceptanceCriterion;
use loom::verify::transitions::{load_stage, save_stage};

const PLAN_CONTENT: &str = "# PLAN: Concurrent Amendment Test\n\n\
Some prose.\n\n\
<!-- loom METADATA -->\n\n\
```yaml\n\
loom:\n  version: 1\n  adjudication:\n    max_amendments_per_stage: 100\n  stages:\n    - id: stage-c\n      name: \"Concurrent\"\n      working_dir: \".\"\n      dependencies: []\n      acceptance:\n        - \"cargo test\"\n```\n\n\
<!-- END loom METADATA -->\n";

struct Env {
    _tmp: TempDir,
    work_dir: PathBuf,
    plan_path: PathBuf,
}

fn setup() -> Env {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_path_buf();
    let work_dir = project_root.join(".work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(project_root.join("doc").join("plans")).unwrap();
    let plan_path = project_root
        .join("doc")
        .join("plans")
        .join("PLAN-concurrent.md");
    fs::write(&plan_path, PLAN_CONTENT).unwrap();

    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"X\"\nbase_branch = \"main\"\n",
        plan_path.display(),
    );
    fs::write(work_dir.join("config.toml"), cfg).unwrap();

    let s = Stage {
        id: "stage-c".to_string(),
        name: "Concurrent".to_string(),
        working_dir: Some(".".to_string()),
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        ..Stage::default()
    };
    save_stage(&s, &work_dir).unwrap();

    Env {
        _tmp: tmp,
        work_dir,
        plan_path,
    }
}

fn make_amendment(suffix: &str) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: "stage-c".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: format!("\"cargo test --variant-{suffix}\""),
        },
        reason: Some(format!("variant {suffix}")),
        dispute_id: Some(format!("d-{suffix}")),
    }
}

fn read_audit_versions(work_dir: &Path) -> Vec<u64> {
    let p = plan_versions_dir(work_dir).join("audit.md");
    let mut out = Vec::new();
    if !p.exists() {
        return out;
    }
    let content = fs::read_to_string(p).unwrap();
    for line in content.lines() {
        if !line.starts_with("| ") {
            continue;
        }
        if line.starts_with("| version ") || line.starts_with("|---") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if cells.len() < 3 {
            continue;
        }
        if let Ok(v) = cells[1].parse::<u64>() {
            out.push(v);
        }
    }
    out
}

// --------------------------------------------------------------------------
// 1. Concurrent threads under flock allocate strictly-increasing versions.
// --------------------------------------------------------------------------
#[test]
fn concurrent_amendments_produce_strictly_increasing_versions() {
    let env = setup();
    let work_dir = Arc::new(env.work_dir.clone());
    let plan_path = Arc::new(env.plan_path.clone());

    let mut handles = Vec::new();
    for i in 0..8 {
        let work_dir = Arc::clone(&work_dir);
        let plan_path = Arc::clone(&plan_path);
        let handle = thread::spawn(move || {
            apply_amendment(&plan_path, &work_dir, make_amendment(&i.to_string()))
        });
        handles.push(handle);
    }

    let mut versions = Vec::new();
    for h in handles {
        let r = h.join().unwrap().unwrap();
        versions.push(r.version);
    }
    versions.sort_unstable();
    let unique: HashSet<u64> = versions.iter().copied().collect();
    assert_eq!(unique.len(), versions.len(), "all versions must be unique");
    for w in versions.windows(2) {
        assert!(
            w[1] == w[0] + 1,
            "versions must be strictly increasing without gaps; got window {w:?}",
        );
    }
    assert_eq!(*versions.first().unwrap(), 1);
}

// --------------------------------------------------------------------------
// 2. After every commit, load_stage reflects the latest amendment so the
//    orchestrator's sync_graph_with_stage_files observes the amended
//    criteria.
// --------------------------------------------------------------------------
#[test]
fn stage_file_reflects_latest_amendment_after_each_commit() {
    let env = setup();
    for i in 0..5 {
        let r = apply_amendment(&env.plan_path, &env.work_dir, make_amendment(&i.to_string()))
            .unwrap();
        assert_eq!(r.amendments_applied as usize, i + 1);
        let stage = load_stage("stage-c", &env.work_dir).unwrap();
        let expected = format!("cargo test --variant-{i}");
        assert_eq!(
            stage.acceptance[0].command(),
            expected,
            "stage file must reflect amendment {i}"
        );
    }
}

// --------------------------------------------------------------------------
// 3. Audit rows appear in completion order (matches the order in which
//    the flock was released).
// --------------------------------------------------------------------------
#[test]
fn audit_rows_appear_in_completion_order() {
    let env = setup();
    let work_dir = Arc::new(env.work_dir.clone());
    let plan_path = Arc::new(env.plan_path.clone());

    let mut handles = Vec::new();
    for i in 0..6 {
        let work_dir = Arc::clone(&work_dir);
        let plan_path = Arc::clone(&plan_path);
        let handle = thread::spawn(move || {
            let r = apply_amendment(
                &plan_path,
                &work_dir,
                make_amendment(&format!("t{i}")),
            )
            .unwrap();
            r.version
        });
        handles.push(handle);
    }

    let mut completion_versions = Vec::new();
    for h in handles {
        completion_versions.push(h.join().unwrap());
    }
    // Now read audit log in file order — the versions there should equal
    // the set of versions completed (not necessarily by thread spawn order).
    let mut audit_versions = read_audit_versions(&env.work_dir);
    audit_versions.sort_unstable();
    completion_versions.sort_unstable();
    assert_eq!(audit_versions, completion_versions);

    // In the file itself, versions must be strictly increasing.
    let on_disk = read_audit_versions(&env.work_dir);
    for w in on_disk.windows(2) {
        assert!(w[1] > w[0], "audit row order is not strictly increasing: {on_disk:?}");
    }
}

// --------------------------------------------------------------------------
// 4. Per-stage amendment count from the audit log equals the number of
//    successful concurrent amendments.
// --------------------------------------------------------------------------
#[test]
fn count_amendments_matches_concurrent_successes() {
    let env = setup();
    let work_dir = Arc::new(env.work_dir.clone());
    let plan_path = Arc::new(env.plan_path.clone());

    let mut handles = Vec::new();
    for i in 0..7 {
        let work_dir = Arc::clone(&work_dir);
        let plan_path = Arc::clone(&plan_path);
        let handle = thread::spawn(move || {
            apply_amendment(&plan_path, &work_dir, make_amendment(&format!("c{i}"))).is_ok()
        });
        handles.push(handle);
    }
    let mut successes = 0u32;
    for h in handles {
        if h.join().unwrap() {
            successes += 1;
        }
    }
    assert_eq!(successes, 7);

    let counted = count_amendments_for_stage(&env.work_dir, "stage-c").unwrap();
    assert_eq!(counted, 7);
}
