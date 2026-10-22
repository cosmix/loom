//! Fixtures for `tests/gate_dedup_contracts.rs`: an accepted contract dispute
//! over a stage whose harness glob matches a frozen support file, a version 1
//! plan for `loom plan verify`, and a simulated main-agent process tree for
//! `subagent-verify-guard.sh`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::Utc;
use loom::fs::work_dir::WorkDir;
use loom::models::dispute::{
    request_file, verdict_file, Citation, DisputeKind, DisputeRequest, DisputeVerdict,
    DisputeVerdictRecord, PlanPatch,
};
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::adjudication::AdjudicatorRegistry;
use loom::plan::schema::{AcceptanceCriterion, ContractSpec};
use loom::relay::sha256_hex;
use loom::verify::contracts::store::{
    load_freeze, write_freeze, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use tempfile::TempDir;

use crate::hv_support;

pub const STAGE: &str = "stage-a";
pub const CONTRACT: &str = "c1";
pub const CONTRACT_FILE: &str = "tests/x.rs";
pub const HELPER_FILE: &str = "tests/support/helper.rs";
pub const CONTRACT_BYTES: &[u8] = b"#[test]\nfn c1() { assert!(helper::ready()); }\n";
pub const HELPER_BYTES: &[u8] = b"pub fn ready() -> bool { false }\n";

const PLAN: &str = r#"# Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
    - id: stage-a
      name: stage-a
      stage_type: standard
      working_dir: "."
      harness: ["tests/support/**"]
      contracts:
        - id: c1
          file: "tests/x.rs"
          test: "c1"
          runner: cargo-test
          scenario: "x arrives"
          rejects: "accepting x"
      acceptance:
        - "cargo test"
```

<!-- END loom METADATA -->
"#;

/// The repository, its stage worktree and its state directory.
pub struct Refreeze {
    _tmp: TempDir,
    pub work: PathBuf,
    pub worktree: PathBuf,
}

/// A repository whose one commit holds the contract and the harness file,
/// with a `stage-a` worktree, stage-a frozen over both files at their
/// committed bytes, and an accepted contract dispute on c1 (number 1) whose
/// plan patch changes nothing, recorded but not yet applied.
pub fn refreeze_fixture() -> Refreeze {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let repo = tmp.path().join("repo");
    let worktree = repo_with_worktree(&home, &repo);
    let work = repo.join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("orchestrator.lock"), b"").unwrap();
    WorkDir::new(&repo).unwrap().adopt_existing().unwrap();
    write_plan(&repo, &work);
    loom::verify::transitions::save_stage(&stage(), &work).unwrap();
    freeze(&work);
    write_dispute(&work);
    Refreeze {
        _tmp: tmp,
        work,
        worktree,
    }
}

impl Refreeze {
    /// Apply the recorded verdict as the daemon does.
    pub fn apply(&self) -> anyhow::Result<()> {
        AdjudicatorRegistry::new().apply_verdict(&self.work, STAGE, 1)
    }

    /// The hash the freeze record holds for `path`, if it lists it.
    pub fn frozen_hash(&self, path: &str) -> Option<String> {
        let record = load_freeze(&self.work, STAGE).unwrap().unwrap();
        let found = record.files.iter().find(|file| file.path == path);
        found.map(|file| file.sha256.clone())
    }

    /// The frozen copy of `path` under the contracts store.
    pub fn frozen_copy(&self, path: &str) -> Vec<u8> {
        let copy = loom::verify::contracts::store::frozen_file_path(&self.work, STAGE, path);
        fs::read(&copy).unwrap_or_else(|e| panic!("read {}: {e}", copy.display()))
    }
}

fn repo_with_worktree(home: &Path, repo: &Path) -> PathBuf {
    fs::create_dir_all(repo.join("tests/support")).unwrap();
    hv_support::git(home, repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.email", "test@test.com"),
        ("user.name", "Test User"),
        ("commit.gpgsign", "false"),
    ] {
        hv_support::git(home, repo, &["config", key, value]);
    }
    fs::write(repo.join(".gitignore"), ".loom/\n.worktrees/\n").unwrap();
    fs::write(repo.join(CONTRACT_FILE), CONTRACT_BYTES).unwrap();
    fs::write(repo.join(HELPER_FILE), HELPER_BYTES).unwrap();
    hv_support::git(home, repo, &["add", "."]);
    hv_support::git(home, repo, &["commit", "-q", "-m", "base"]);
    let relative = format!(".worktrees/{STAGE}");
    let branch = format!("loom/{STAGE}");
    hv_support::git(
        home,
        repo,
        &["worktree", "add", "-q", "-b", &branch, &relative],
    );
    repo.join(relative)
}

fn write_plan(repo: &Path, work: &Path) {
    let plans = repo.join("doc").join("plans");
    fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("PLAN-refreeze.md");
    fs::write(&plan, PLAN).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    fs::write(work.join("config.toml"), config).unwrap();
}

fn stage() -> Stage {
    Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::NeedsAdjudication,
        plan_version: 2,
        worktree: Some(STAGE.to_string()),
        working_dir: Some(".".to_string()),
        harness: vec!["tests/support/**".to_string()],
        contracts: vec![ContractSpec {
            id: CONTRACT.to_string(),
            file: CONTRACT_FILE.to_string(),
            test: "c1".to_string(),
            runner: Some("cargo-test".to_string()),
            cwd: None,
            scenario: "x arrives".to_string(),
            rejects: "accepting x".to_string(),
        }],
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        ..Default::default()
    }
}

fn freeze(work: &Path) {
    let files = [(CONTRACT_FILE, CONTRACT_BYTES), (HELPER_FILE, HELPER_BYTES)];
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: files
            .iter()
            .map(|(path, bytes)| FrozenFile {
                path: path.to_string(),
                sha256: sha256_hex(bytes),
            })
            .collect(),
        contracts: vec![FrozenContract {
            id: CONTRACT.to_string(),
            adapter: Some("cargo-test".to_string()),
            outcome: "failed".to_string(),
            exit_code: Some(101),
            file: Some(CONTRACT_FILE.to_string()),
            cwd: Some(".".to_string()),
        }],
    };
    let copies: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
        .collect();
    write_freeze(work, &record, &copies).unwrap();
}

fn write_dispute(work: &Path) {
    let root = work.join("disputes");
    fs::create_dir_all(root.join(STAGE).join("1")).unwrap();
    let request = DisputeRequest {
        id: 1,
        stage_id: STAGE.to_string(),
        kind: DisputeKind::Contract {
            contract_id: CONTRACT.to_string(),
        },
        reason: "the contract is wrong".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: Utc::now(),
    };
    let yaml = loom::yaml::to_string(&request).unwrap();
    let body = format!("---\n{yaml}---\n\n# Dispute\n");
    fs::write(request_file(&root, STAGE, 1), body).unwrap();
    let verdict = DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: serde_json::json!({}),
        },
        citations: vec![Citation {
            file: CONTRACT_FILE.to_string(),
            line: Some(1),
            excerpt: "fn c1()".to_string(),
            claim: "the contract's spec is wrong".to_string(),
        }],
        reasoning: "the disputed contract is wrong".to_string(),
    };
    let record = DisputeVerdictRecord {
        id: 1,
        stage_id: STAGE.to_string(),
        verdict,
        adjudicator_attempt_count: 1,
        created_at: Utc::now(),
        model: "test".to_string(),
        session_id: None,
    };
    let yaml = loom::yaml::to_string(&record).unwrap();
    let body = format!("---\n{yaml}---\n\n# Verdict\n");
    fs::write(verdict_file(&root, STAGE, 1), body).unwrap();
}

/// Write a version 1 plan whose one standard stage has `acceptance`.
pub fn write_v1_plan(path: &Path, acceptance: &str) {
    let quoted = serde_json::to_string(acceptance).unwrap();
    let body = format!(
        "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: stage-a\n      name: stage-a\n      stage_type: standard\n      \
         working_dir: .\n      acceptance:\n        - {quoted}\n```\n\n\
         <!-- END loom METADATA -->\n"
    );
    fs::write(path, body).unwrap();
}

/// A temporary directory whose path holds no "claude": the guard's process
/// detector matches that name anywhere in a command line (see GOTCHA 1 in
/// `tests/integration/hooks_subagent_verify_guard.rs`). The runner's
/// `TMPDIR` may contain it, so this prefers cargo's per-target directory.
pub fn temp_dir_no_claude() -> TempDir {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let base = if base.to_string_lossy().to_lowercase().contains("claude") {
        std::env::temp_dir()
    } else {
        base
    };
    assert!(
        !base.to_string_lossy().to_lowercase().contains("claude"),
        "no temporary root without \"claude\" in its path ({})",
        base.display()
    );
    fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("gate-dedup-guard-")
        .tempdir_in(&base)
        .unwrap()
}

fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// Install the guard and its sourced helpers into `dir`; returns the hook.
pub fn install_guard(dir: &Path) -> PathBuf {
    use loom::fs::permissions::constants::{HOOK_BOUNDED, HOOK_COMMON};
    write_exec(&dir.join("_bounded.sh"), HOOK_BOUNDED);
    write_exec(&dir.join("_common.sh"), HOOK_COMMON);
    let hook = dir.join("subagent-verify-guard.sh");
    let script = loom::fs::permissions::constants::HOOK_SUBAGENT_VERIFY_GUARD;
    write_exec(&hook, script);
    hook
}

/// Write `stages/01-<stage_id>.md` declaring a running standard stage.
pub fn write_standard_stage(work_dir: &Path, stage_id: &str) {
    let stages = work_dir.join("stages");
    fs::create_dir_all(&stages).unwrap();
    let content = format!(
        "---\nid: {stage_id}\nname: Standard\nstatus: executing\nstage_type: standard\n\
         plan_id: PLAN-test\nworking_dir: .\n---\n\n# Stage: Standard\n"
    );
    fs::write(stages.join(format!("01-{stage_id}.md")), content).unwrap();
}

/// Run the guard as the MAIN agent: a two-level "claude"-named chain whose
/// outer process exports itself as `LOOM_MAIN_AGENT_PID`, as
/// `run_hook_in_tree(.., subagent: false, ..)` builds it. The Bash payload
/// carries no `agent_type`. Returns (exit code, stderr).
pub fn run_guard_as_main(
    dir: &Path,
    hook: &Path,
    command: &str,
    env: &[(&str, &str)],
) -> (i32, String) {
    let mid = dir.join("claude-mid.sh");
    let outer = dir.join("claude-outer.sh");
    write_exec(
        &mid,
        &format!("#!/usr/bin/env bash\nbash \"{}\"\n", hook.display()),
    );
    write_exec(
        &outer,
        &format!(
            "#!/usr/bin/env bash\nexport LOOM_MAIN_AGENT_PID=$$\nbash \"{}\"\n",
            mid.display()
        ),
    );
    let mut cmd = Command::new("bash");
    cmd.arg(&outer)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_WORK_DIR")
        .env_remove("LOOM_MAIN_AGENT_PID");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("spawn process tree");
    let payload = format!(
        r#"{{"tool_name": "Bash", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).unwrap()
    );
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload.as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for process tree");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code().unwrap_or(-1), stderr)
}
