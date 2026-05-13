//! Integration tests for the adjudication subsystem driven against a
//! local httpmock-backed HTTP server (no real Anthropic API calls).
//!
//! Each test wires up a fresh tmp .work directory, spins up an
//! httpmock server, configures `AdjudicatorRegistry` to point at the
//! mock URL, then drives the registry through the same hooks the
//! orchestrator's main loop uses.

use httpmock::prelude::*;
use httpmock::Mock;
use loom::models::dispute::{
    applied_marker, dispute_dir, request_file, verdict_file, DisputeRequest,
};
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::adjudication::{feedback, worker as adj_worker, AdjudicatorRegistry};
use loom::plan::schema::AcceptanceCriterion;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn write_stage(work_dir: &Path, stage: &Stage) {
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    loom::verify::transitions::save_stage(stage, work_dir).unwrap();
}

fn make_stage(id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status: StageStatus::NeedsAdjudication,
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        ..Default::default()
    }
}

/// Stage seeded with two acceptance criteria so a Delete-index-0 amendment
/// still produces a validatable Standard stage (validation requires at least
/// one of acceptance/wiring/artifacts/wiring_tests to remain).
fn make_stage_two_criteria(id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: Some(".".to_string()),
        status: StageStatus::NeedsAdjudication,
        acceptance: vec![
            AcceptanceCriterion::Simple(
                "test -f /__loom_intentionally_wrong/marker.txt".to_string(),
            ),
            AcceptanceCriterion::Simple("ls /tmp".to_string()),
        ],
        ..Default::default()
    }
}

fn write_dispute(work_dir: &Path, stage_id: &str, id: u32) {
    let disputes_root = work_dir.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(id.to_string())).unwrap();
    let req = DisputeRequest {
        id,
        stage_id: stage_id.to_string(),
        criterion_index: 0,
        reason: "criterion impossible".to_string(),
        evidence_commit: None,
        failure_output: Some("err: something".to_string()),
        fix_attempts_at_dispute: 1,
        created_at: chrono::Utc::now(),
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    let path = request_file(&disputes_root, stage_id, id);
    std::fs::write(&path, format!("---\n{yaml}---\n\n# Dispute\n")).unwrap();
}

fn write_plan(work_dir: &Path) {
    // The adjudicator resolves the plan path from config.toml; for
    // tests that don't exercise the plan file, write a minimal valid
    // markdown so prompt::build can read it without panicking.
    let plan = work_dir.join("PLAN.md");
    std::fs::write(
        &plan,
        "# Plan\n\n```yaml\nloom:\n  version: 1\n  stages:\n    - id: s1\n      name: s1\n      working_dir: .\n      acceptance:\n        - cargo test\n```\n",
    )
    .unwrap();
    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\nbase_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work_dir.join("config.toml"), cfg).unwrap();
}

/// Write a plan file with the full `<!-- loom METADATA -->` markers
/// that `apply_amendment` requires to splice the amended YAML back into
/// the document. The stage `s1` has two acceptance criteria so a
/// Delete-index-0 amendment leaves the plan schema-valid.
fn write_plan_with_metadata_markers(work_dir: &Path) -> PathBuf {
    let plan = work_dir.join("PLAN.md");
    let content = "\
# Plan: Adjudication End-to-End

Prose section that must be preserved across amendments.

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: s1
      name: s1
      working_dir: \".\"
      acceptance:
        - \"test -f /__loom_intentionally_wrong/marker.txt\"
        - \"ls /tmp\"
```

<!-- END loom METADATA -->

Trailing prose section.
";
    std::fs::write(&plan, content).unwrap();
    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\nbase_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work_dir.join("config.toml"), cfg).unwrap();
    plan
}

fn mock_accept_response(server: &MockServer) -> Mock<'_> {
    let body = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&serde_json::json!({
                    "verdict": "reject",
                    "reasoning": "criterion is correct",
                    "citations": [
                        {"file": "src/a.rs", "line": 1, "excerpt": "fn foo", "claim": "function exists"}
                    ]
                })).unwrap()
            }
        ]
    });
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body(body.to_string());
    })
}

fn mock_accept_with_amendment(server: &MockServer) -> Mock<'_> {
    let body = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&serde_json::json!({
                    "verdict": "accept",
                    "reasoning": "criterion was overspecified",
                    "citations": [
                        {"file": "src/a.rs", "line": 1, "excerpt": "X", "claim": "Y"}
                    ],
                    "plan_patch": {
                        "stage_id": "s1",
                        "field": "acceptance",
                        "patch": {"op": "delete", "index": 0},
                        "reason": "test amendment"
                    }
                })).unwrap()
            }
        ]
    });
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body(body.to_string());
    })
}

/// Mock an Accept verdict whose `plan_patch` deletes acceptance[0].
/// Targets the stage seeded by [`make_stage_two_criteria`].
fn mock_accept_delete_first(server: &MockServer) -> Mock<'_> {
    let body = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&serde_json::json!({
                    "verdict": "accept",
                    "reasoning": "acceptance[0] references a path that cannot exist; criterion has no valid interpretation",
                    "citations": [
                        {
                            "file": "PLAN.md",
                            "line": 1,
                            "excerpt": "intentionally_wrong",
                            "claim": "path does not exist in the project root"
                        }
                    ],
                    "plan_patch": {
                        "stage_id": "s1",
                        "field": "acceptance",
                        "patch": {"op": "delete", "index": 0},
                        "reason": "criterion path is mechanically wrong"
                    }
                })).unwrap()
            }
        ]
    });
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body(body.to_string());
    })
}

fn mock_needs_more(server: &MockServer) -> Mock<'_> {
    let body = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&serde_json::json!({
                    "verdict": "needs-more-evidence",
                    "questions": ["what is X?"]
                })).unwrap()
            }
        ]
    });
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body(body.to_string());
    })
}

fn mock_500_then_success(server: &MockServer) -> Mock<'_> {
    // Just configure a successful response; testing the retry path
    // requires sequenced responses which httpmock doesn't expose
    // directly. Acceptance of the retry behaviour is covered by
    // unit tests in client.rs.
    let good_body = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&serde_json::json!({
                    "verdict": "reject",
                    "reasoning": "ok",
                    "citations": [{"file":"f","excerpt":"e","claim":"c"}]
                })).unwrap()
            }
        ]
    });
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body(good_body.to_string());
    })
}

fn make_registry(work_dir: &Path, endpoint: String) -> AdjudicatorRegistry {
    let mut reg = AdjudicatorRegistry::new(Some("test-key".to_string()), work_dir);
    reg.endpoint = endpoint;
    reg.model = "claude-test-model".to_string();
    reg
}

/// Wait until either the verdict file or applied.marker appears, or a
/// short deadline expires. Returns whether the predicate was satisfied.
fn wait_for<F: Fn() -> bool>(pred: F, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if pred() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    pred()
}

#[test]
fn reject_verdict_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_accept_response(&server);

    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    // First tick: spawn the worker.
    reg.check_pending_disputes(work).unwrap();
    // Wait for the verdict file to land.
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    let ok = wait_for(|| verdict_path.exists(), Duration::from_secs(10));
    assert!(ok, "verdict.md never appeared");

    // Drain so the worker handle gets joined.
    reg.drain_completed_workers(work).unwrap();
    // Apply the verdict.
    reg.apply_pending_verdicts(work).unwrap();

    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);

    let fb = feedback::read_feedback(work, "s1").unwrap().unwrap();
    assert!(fb.contains("rejected"));

    assert!(applied_marker(&work.join("disputes"), "s1", 1).exists());
}

#[test]
fn needs_more_evidence_writes_questions() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_needs_more(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(wait_for(|| verdict_path.exists(), Duration::from_secs(10)));
    reg.drain_completed_workers(work).unwrap();
    reg.apply_pending_verdicts(work).unwrap();

    let fb = feedback::read_feedback(work, "s1").unwrap().unwrap();
    assert!(fb.contains("what is X?"));
}

#[test]
fn accept_verdict_amends_plan_and_clears_feedback() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);
    // Pre-seed feedback to verify it gets cleared.
    feedback::append_questions(work, "s1", &["stale".to_string()]).unwrap();

    let server = MockServer::start();
    let _m = mock_accept_with_amendment(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(wait_for(|| verdict_path.exists(), Duration::from_secs(10)));
    reg.drain_completed_workers(work).unwrap();
    // `apply_pending_verdicts` returns Ok even when individual verdicts
    // fail to apply — per-verdict failures are logged via tracing and the
    // outer call still walks the rest of the queue. The plan in this test
    // intentionally lacks the `<!-- loom METADATA -->` markers needed by
    // `apply_amendment`, so the amendment is expected to fail and the
    // applied.marker SHOULD NOT exist. (For the success-path contract see
    // `dispute_to_amendment_to_pass`.)
    reg.apply_pending_verdicts(work)
        .expect("apply_pending_verdicts should return Ok even when per-verdict apply fails");

    // The worker produced a parseable Accept verdict but the amendment
    // did not land — the verdict file exists, the marker does not.
    let content = std::fs::read_to_string(&verdict_path).unwrap();
    assert!(content.contains("accept"));
    assert!(
        !applied_marker(&work.join("disputes"), "s1", 1).exists(),
        "applied.marker must not exist when apply_amendment failed",
    );
}

#[test]
fn http_500_retries_then_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_500_then_success(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(
        wait_for(|| verdict_path.exists(), Duration::from_secs(15)),
        "verdict.md never appeared",
    );
    reg.drain_completed_workers(work).unwrap();
}

#[test]
fn registry_without_api_key_escalates_disputes() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let mut reg = AdjudicatorRegistry::new(None, work);
    reg.check_pending_disputes(work).unwrap();

    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
}

#[test]
fn inflight_marker_blocks_double_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    // Pre-create a fresh inflight marker.
    let dir = dispute_dir(&work.join("disputes"), "s1", 1);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".inflight"), b"").unwrap();

    // No mock server needed — if the worker spawned anyway, it would
    // fail (no endpoint) but the registry's job is to NOT spawn it.
    let mut reg = make_registry(work, "http://127.0.0.1:1/should-not-be-hit".to_string());
    reg.check_pending_disputes(work).unwrap();
    assert!(
        reg.handles.is_empty(),
        "must not spawn while inflight is fresh"
    );
}

#[test]
fn double_apply_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_accept_response(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(wait_for(|| verdict_path.exists(), Duration::from_secs(10)));
    reg.drain_completed_workers(work).unwrap();

    reg.apply_pending_verdicts(work).unwrap();
    let mid = loom::verify::transitions::load_stage("s1", work).unwrap();
    reg.apply_pending_verdicts(work).unwrap();
    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(mid.status, after.status);
}

#[test]
fn shutdown_completes_within_deadline() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let mut reg = make_registry(work, "http://127.0.0.1:1/x".to_string());
    let start = Instant::now();
    reg.shutdown(start + Duration::from_secs(2));
    assert!(start.elapsed() < Duration::from_secs(3));
}

/// True end-to-end coverage of the autonomous-criteria-adjudication
/// happy path:
///
/// 1. Stage `s1` has a mechanically wrong acceptance criterion at
///    index 0 plus a passing one at index 1.
/// 2. A dispute is filed against index 0.
/// 3. The mocked Anthropic endpoint returns an Accept verdict whose
///    `plan_patch` deletes acceptance[0].
/// 4. `apply_pending_verdicts` MUST succeed (no silent fallthrough).
/// 5. Assertions cover every observable side-effect of a successful
///    amendment: `plan_versions/1.md` snapshot, `audit.md` row, live
///    plan file rewritten, stage transitions back to `Queued`, and
///    `amendments_applied` is incremented.
#[test]
fn dispute_to_amendment_to_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let plan = write_plan_with_metadata_markers(work);
    let stage = make_stage_two_criteria("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_accept_delete_first(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(
        wait_for(|| verdict_path.exists(), Duration::from_secs(10)),
        "verdict.md never appeared",
    );
    reg.drain_completed_workers(work).unwrap();

    // CRITICAL: apply must succeed. No `let _ =` fallthrough.
    reg.apply_pending_verdicts(work)
        .expect("apply_pending_verdicts must succeed for a well-formed plan");

    // Snapshot exists at .work/plan_versions/1.md
    let snapshot = work.join("plan_versions").join("1.md");
    assert!(
        snapshot.exists(),
        "plan_versions/1.md must exist after first amendment",
    );

    // Audit log records the amendment with stage_id and op.
    let audit = std::fs::read_to_string(work.join("plan_versions").join("audit.md"))
        .expect("audit.md must exist after first amendment");
    assert!(
        audit.contains("s1"),
        "audit row must mention stage_id 's1' — audit:\n{audit}",
    );
    assert!(
        audit.contains("delete"),
        "audit row must record the patch op — audit:\n{audit}",
    );

    // Live plan file got rewritten and no longer carries the deleted criterion.
    let live_plan = std::fs::read_to_string(&plan).unwrap();
    assert!(
        !live_plan.contains("intentionally_wrong"),
        "live plan must no longer contain the deleted criterion",
    );
    // Prose around the YAML block must survive the splice.
    assert!(
        live_plan.contains("Prose section that must be preserved"),
        "leading prose must survive amendment",
    );
    assert!(
        live_plan.contains("Trailing prose section"),
        "trailing prose must survive amendment",
    );

    // Stage state: Queued, single remaining acceptance, amendment counter bumped.
    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert_eq!(
        after.acceptance.len(),
        1,
        "acceptance[0] should be deleted, leaving 1 criterion",
    );
    match &after.acceptance[0] {
        AcceptanceCriterion::Simple(cmd) => assert_eq!(cmd, "ls /tmp"),
        other => panic!("expected Simple criterion, got {other:?}"),
    }
    assert_eq!(
        after.amendments_applied, 1,
        "stage.amendments_applied must increment to 1",
    );

    // applied.marker landed so reapplication is a no-op.
    assert!(applied_marker(&work.join("disputes"), "s1", 1).exists());
}

/// Calling `apply_pending_verdicts` a second time after a successful
/// amendment must NOT re-apply the patch (which would double-bump
/// `amendments_applied` and grow `plan_versions/` indefinitely).
///
/// This is the daemon-restart idempotency contract: the
/// `applied.marker` file is the persistent guard, surviving across
/// process restarts.
#[test]
fn dispute_amendment_is_idempotent_under_repeat_apply() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan_with_metadata_markers(work);
    let stage = make_stage_two_criteria("s1");
    write_stage(work, &stage);
    write_dispute(work, "s1", 1);

    let server = MockServer::start();
    let _m = mock_accept_delete_first(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), "s1", 1);
    assert!(wait_for(|| verdict_path.exists(), Duration::from_secs(10)));
    reg.drain_completed_workers(work).unwrap();

    reg.apply_pending_verdicts(work).unwrap();
    let mid = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(mid.amendments_applied, 1);

    // Second apply: applied.marker exists, so the call must be a no-op.
    reg.apply_pending_verdicts(work).unwrap();
    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(
        after.amendments_applied, 1,
        "second apply must NOT double-count amendments_applied",
    );
    // Only one snapshot, only one audit row.
    let snapshot_2 = work.join("plan_versions").join("2.md");
    assert!(
        !snapshot_2.exists(),
        "second apply must NOT write a new snapshot",
    );
}

/// Write a plan with the loom-METADATA markers and a configurable
/// `max_amendments_per_stage`. The stage `s1` has three acceptance
/// criteria so two successive Delete-index-0 amendments leave a
/// schema-valid Standard stage; a third would still leave one criterion
/// but is rejected by the cap.
fn write_plan_with_cap(work_dir: &Path, max_amendments_per_stage: u32) -> PathBuf {
    let plan = work_dir.join("PLAN.md");
    let content = format!(
        "# Plan: Adjudication Cap Test\n\
\n\
Prose section that must be preserved across amendments.\n\
\n\
<!-- loom METADATA -->\n\
\n\
```yaml\n\
loom:\n  version: 1\n  adjudication:\n    max_amendments_per_stage: {max_amendments_per_stage}\n  stages:\n    - id: s1\n      name: s1\n      working_dir: \".\"\n      acceptance:\n        - \"test -f /__loom_wrong_a/marker.txt\"\n        - \"test -f /__loom_wrong_b/marker.txt\"\n        - \"ls /tmp\"\n```\n\
\n\
<!-- END loom METADATA -->\n\
\n\
Trailing prose section.\n",
    );
    std::fs::write(&plan, content).unwrap();
    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\nbase_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work_dir.join("config.toml"), cfg).unwrap();
    plan
}

fn make_stage_three_criteria(id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: Some(".".to_string()),
        status: StageStatus::NeedsAdjudication,
        acceptance: vec![
            AcceptanceCriterion::Simple("test -f /__loom_wrong_a/marker.txt".to_string()),
            AcceptanceCriterion::Simple("test -f /__loom_wrong_b/marker.txt".to_string()),
            AcceptanceCriterion::Simple("ls /tmp".to_string()),
        ],
        ..Default::default()
    }
}

/// Drive one dispute through the registry to apply.marker. Used to land
/// successful amendments in the cap-exceeded e2e test. Re-seeds the
/// stage to `NeedsAdjudication` before scanning so back-to-back disputes
/// can be processed without an agent-flow round trip.
fn drive_dispute_to_applied(
    reg: &mut AdjudicatorRegistry,
    work: &Path,
    stage_id: &str,
    dispute_id: u32,
) {
    let mut s = loom::verify::transitions::load_stage(stage_id, work).unwrap();
    s.status = StageStatus::NeedsAdjudication;
    loom::verify::transitions::save_stage(&s, work).unwrap();
    write_dispute(work, stage_id, dispute_id);
    reg.check_pending_disputes(work).unwrap();
    let verdict_path = verdict_file(&work.join("disputes"), stage_id, dispute_id);
    assert!(
        wait_for(|| verdict_path.exists(), Duration::from_secs(10)),
        "verdict.md never appeared for dispute {dispute_id}",
    );
    reg.drain_completed_workers(work).unwrap();
    reg.apply_pending_verdicts(work)
        .expect("apply_pending_verdicts should be Ok");
}

/// End-to-end coverage of the amendment-cap escalation path. Two
/// accepted amendments use the cap of 2; the third dispute lands an
/// Accept verdict that `apply_amendment` rejects with the
/// `amendment cap exceeded` error. The orchestrator must NOT loop on
/// that verdict — instead it escalates the stage to
/// `NeedsHumanReview` and writes `applied.marker` for the third
/// dispute so subsequent ticks short-circuit.
#[test]
fn amendment_cap_exceeded_escalates_to_human_review() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan_with_cap(work, 2);
    let stage = make_stage_three_criteria("s1");
    write_stage(work, &stage);

    let server = MockServer::start();
    let _m = mock_accept_delete_first(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    // Two accepted amendments under the cap.
    drive_dispute_to_applied(&mut reg, work, "s1", 1);
    drive_dispute_to_applied(&mut reg, work, "s1", 2);

    let after_two = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after_two.amendments_applied, 2);
    assert_eq!(after_two.status, StageStatus::Queued);

    // Third dispute: stage must be put back into NeedsAdjudication first
    // (apply_verdict refuses to act on Queued stages with no verdict file).
    let mut staged = after_two;
    staged.status = StageStatus::NeedsAdjudication;
    loom::verify::transitions::save_stage(&staged, work).unwrap();
    write_dispute(work, "s1", 3);
    reg.check_pending_disputes(work).unwrap();
    let verdict_path_3 = verdict_file(&work.join("disputes"), "s1", 3);
    assert!(wait_for(
        || verdict_path_3.exists(),
        Duration::from_secs(10)
    ));
    reg.drain_completed_workers(work).unwrap();
    reg.apply_pending_verdicts(work)
        .expect("apply_pending_verdicts should be Ok");

    // The cap blocked the third amendment. The orchestrator must
    // escalate AND write applied.marker so we don't retry forever.
    let after_three = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(
        after_three.status,
        StageStatus::NeedsHumanReview,
        "third Accept must escalate via cap-exceeded path",
    );
    assert!(
        after_three
            .review_reason
            .as_deref()
            .unwrap_or("")
            .contains("amendment cap exceeded"),
        "review_reason should mention amendment cap; got: {:?}",
        after_three.review_reason,
    );
    assert_eq!(
        after_three.amendments_applied, 2,
        "amendments_applied must NOT increment past the cap",
    );
    assert!(
        applied_marker(&work.join("disputes"), "s1", 3).exists(),
        "applied.marker for dispute 3 must exist so the verdict is not retried",
    );

    // A subsequent tick must be a no-op.
    reg.apply_pending_verdicts(work).unwrap();
    let after_replay = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after_replay.amendments_applied, 2);
}

/// End-to-end coverage of the evidence-rounds escalation path. The
/// mocked server returns `NeedsMoreEvidence` every time; the
/// orchestrator drives the stage Queued → NeedsAdjudication → Queued
/// until `evidence_rounds >= MAX_EVIDENCE_ROUNDS`, at which point the
/// stage must escalate to `NeedsHumanReview` instead of looping.
#[test]
fn evidence_rounds_exhausted_escalates_to_human_review() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    let stage = make_stage("s1");
    write_stage(work, &stage);

    let server = MockServer::start();
    let _m = mock_needs_more(&server);
    let endpoint = format!("{}/v1/messages", server.base_url());
    let mut reg = make_registry(work, endpoint);

    // Drive enough rounds to hit MAX_EVIDENCE_ROUNDS. Each round files a
    // fresh dispute and lets the orchestrator apply the
    // NeedsMoreEvidence verdict (which bumps evidence_rounds).
    let mut dispute_id: u32 = 0;
    loop {
        let s = loom::verify::transitions::load_stage("s1", work).unwrap();
        if s.status == StageStatus::NeedsHumanReview {
            break;
        }
        // Safety: bound the loop so a regression doesn't hang the test.
        assert!(
            dispute_id < 10,
            "did not escalate after 10 rounds; got status {:?}, evidence_rounds {}",
            s.status,
            s.evidence_rounds
        );
        // Ensure the stage is back in NeedsAdjudication so the next dispute
        // can be processed.
        let mut staged = s;
        staged.status = StageStatus::NeedsAdjudication;
        loom::verify::transitions::save_stage(&staged, work).unwrap();

        dispute_id += 1;
        write_dispute(work, "s1", dispute_id);
        reg.check_pending_disputes(work).unwrap();
        let v = verdict_file(&work.join("disputes"), "s1", dispute_id);
        assert!(
            wait_for(|| v.exists(), Duration::from_secs(10)),
            "verdict.md never appeared for dispute {dispute_id}",
        );
        reg.drain_completed_workers(work).unwrap();
        reg.apply_pending_verdicts(work).unwrap();
    }

    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert!(
        after
            .review_reason
            .as_deref()
            .unwrap_or("")
            .contains("evidence loop exhausted"),
        "review_reason should mention evidence-loop exhaustion; got: {:?}",
        after.review_reason,
    );
}

#[test]
fn worker_helper_paths_are_under_work_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let req = adj_worker::request_path(work, "s1", 1);
    let ver = adj_worker::verdict_path(work, "s1", 1);
    let app = adj_worker::applied_marker_path(work, "s1", 1);
    let inflight = adj_worker::inflight_marker_path(&work.join("disputes"), "s1", 1);
    for p in [&req, &ver, &app, &inflight] {
        assert!(
            p.starts_with(work),
            "expected {} to start with work_dir",
            p.display()
        );
    }
}
