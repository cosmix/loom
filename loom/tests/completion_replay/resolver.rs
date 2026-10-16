//! A resolved commit must pass current completion gates before it can land.
use super::support::{run_git, ReplayFixture};
use loom::handoff::worktree_head_commit;
use loom::models::stage::{AcceptanceCriterion, StageStatus};

fn resolved_fixture(status: StageStatus, acceptance: &[&str]) -> ReplayFixture {
    let fixture = ReplayFixture::new(acceptance).unwrap();
    let original = worktree_head_commit(&fixture.worktree).unwrap();
    std::fs::write(fixture.worktree.join("README.md"), "resolved content\n").unwrap();
    run_git(&fixture.worktree, &["add", "README.md"]).unwrap();
    run_git(&fixture.worktree, &["commit", "-m", "resolve"]).unwrap();
    let mut stage = fixture.reload_stage().unwrap();
    stage.status = status;
    stage.completed_commit = Some(original);
    stage.session = None;
    fixture.write_stage(&stage).unwrap();
    std::fs::write(
        fixture.root().join(".loom/work/config.toml"),
        "base_branch = \"main\"\n",
    )
    .unwrap();
    fixture
}

#[test]
fn resolver_completion_refuses_failing_current_policy_without_landing() {
    let fixture = resolved_fixture(StageStatus::NeedsHumanReview, &["false"]);
    let before = worktree_head_commit(fixture.root()).unwrap();
    let proof = fixture.reload_stage().unwrap().completed_commit;
    let output = fixture.run_operator_completion();
    assert!(!output.status.success());
    let stage = fixture.reload_stage().unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.completed_commit, proof);
    assert_eq!(worktree_head_commit(fixture.root()).unwrap(), before);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Acceptance criteria FAILED"));
}

#[test]
fn resolver_completion_from_review_verifies_and_lands_the_new_commit() {
    let fixture = resolved_fixture(StageStatus::NeedsHumanReview, &["true"]);
    assert_verified_landing(&fixture);
}

#[test]
fn resolver_completion_in_conflict_runs_pipeline_even_with_live_daemon() {
    let fixture = resolved_fixture(StageStatus::MergeConflict, &["true"]);
    assert_verified_landing(&fixture);
}

fn assert_verified_landing(fixture: &ReplayFixture) {
    let resolved = worktree_head_commit(&fixture.worktree).unwrap();
    let output = fixture.run_operator_completion();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stage = fixture.reload_stage().unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(resolved.as_str()));
    assert!(loom::git::branch::is_ancestor_of(&resolved, "main", fixture.root()).unwrap());
}

#[test]
fn resolver_completion_refuses_policy_mutation_during_acceptance() {
    let fixture = resolved_fixture(StageStatus::NeedsHumanReview, &["true"]);
    let mut stage = fixture.reload_stage().unwrap();
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "mkdir -p target; touch target/started; while [ ! -f target/release ]; do sleep 0.1; done"
            .into(),
    )];
    fixture.write_stage(&stage).unwrap();
    let before = worktree_head_commit(fixture.root()).unwrap();
    let output = std::thread::scope(|scope| {
        let completion = scope.spawn(|| fixture.run_operator_completion());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let started = fixture.worktree.join("target/started");
        while !started.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::write(
            fixture.root().join(".loom/work/config.toml"),
            "base_branch = \"main\"\n# concurrent policy edit\n",
        )
        .unwrap();
        std::fs::create_dir_all(fixture.worktree.join("target")).unwrap();
        std::fs::write(fixture.worktree.join("target/release"), "").unwrap();
        let output = completion.join().unwrap();
        assert!(
            started.exists(),
            "acceptance must run before policy mutation"
        );
        output
    });
    assert!(!output.status.success());
    assert_eq!(worktree_head_commit(fixture.root()).unwrap(), before);
    assert_eq!(
        fixture.reload_stage().unwrap().completed_commit,
        stage.completed_commit
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy changed"));
}

#[test]
fn explicit_operator_skip_pins_content_without_claiming_tests_passed() {
    let fixture = resolved_fixture(StageStatus::NeedsHumanReview, &["false"]);
    let resolved = worktree_head_commit(&fixture.worktree).unwrap();
    let before = worktree_head_commit(fixture.root()).unwrap();
    let output = fixture.run_operator_completion_with_args(&["--no-verify"]);
    assert!(output.status.success(), "{output:?}");
    let stage = fixture.reload_stage().unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(stage.completed_commit.as_deref(), Some(resolved.as_str()));
    assert!(!stage.merged);
    assert_eq!(worktree_head_commit(fixture.root()).unwrap(), before);
    assert!(String::from_utf8_lossy(&output.stdout).contains("skipped verification"));
}
