use super::*;
use crate::context::delivery::{delivered_in_epoch, delivered_to_session, load_deliveries};
use crate::context::retrieve::context_epoch;
use crate::context::schema::UnmetRequirement;
use std::path::PathBuf;

#[path = "worker_brief/test_support.rs"]
mod support;
use support::*;

const NONCE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NONCE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NONCE_C: &str = "cccccccccccccccccccccccccccccccc";

fn transcript_with(fixture: &Fixture, name: &str, nonces: &[&str]) -> PathBuf {
    let content = nonces
        .iter()
        .map(|nonce| marker(nonce))
        .collect::<Vec<_>>()
        .join("\n");
    let line = serde_json::json!({"message":{"content":content}}).to_string();
    fixture.transcript(name, &format!("{line}\n"))
}

#[test]
fn task_and_owned_path_emit_one_nonce_and_only_a_pending_record() {
    let fixture = Fixture::new(true);
    let prompt =
        "Implement the scoped worker.\n\nFiles owned (write only these):\n- `src/scoped.rs`";

    let envelope = issue(&payload(prompt), &fixture.config).expect("scoped source should match");

    assert_eq!(envelope.nonce.len(), 32);
    assert!(envelope.brief.contains("`src/scoped.rs`"));
    assert!(!envelope.brief.contains("src/unrelated.rs"));
    assert_eq!(
        envelope
            .brief
            .lines()
            .filter(|line| marker_nonce(line).is_some())
            .count(),
        1
    );
    assert_eq!(pending_count(&fixture), 1);
    assert!(load_deliveries(&fixture.config.work_dir, PLAN, STAGE)
        .unwrap()
        .is_empty());
}

#[test]
fn path_admission_reads_only_explicit_owned_and_may_read_sections() {
    let prompt = "Discuss src/ignored.rs.\nFiles owned:\n- ./src/owned.rs\nMay read:\n- `src/read.rs`\nNext task: src/also-ignored.rs";

    assert_eq!(
        declared_paths(prompt),
        vec!["src/owned.rs".to_string(), "src/read.rs".to_string()]
    );
}

#[test]
fn one_nonce_binds_once_to_the_first_actual_child() {
    let fixture = Fixture::new(false);
    let pack = pack("rev-a", "unit-a", "sha256:a", "src/a.rs");
    store_pending(&fixture.config, NONCE_A, &pack).unwrap();
    let transcript = transcript_with(&fixture, "child.jsonl", &[NONCE_A]);

    assert!(bind_pending(
        &fixture.config,
        "child-a",
        "worker",
        &transcript
    ));
    assert!(!bind_pending(
        &fixture.config,
        "child-b",
        "worker",
        &transcript
    ));

    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].recipient_id, "child-a");
}

#[test]
fn marker_before_a_truncated_multibyte_character_binds() {
    let fixture = Fixture::new(false);
    let pack = pack("rev-a", "unit-a", "sha256:a", "src/a.rs");
    store_pending(&fixture.config, NONCE_A, &pack).unwrap();
    let prefix = format!("{}\n", marker(NONCE_A));
    let filler = "x".repeat(MAX_TRANSCRIPT_BYTES as usize - prefix.len() - 1);
    let transcript = fixture.transcript("cut.jsonl", &format!("{prefix}{filler}é"));

    assert!(bind_pending(
        &fixture.config,
        "child-a",
        "worker",
        &transcript
    ));
}

#[test]
fn two_pending_nonces_are_isolated_when_one_child_starts() {
    let fixture = Fixture::new(false);
    let pack = pack("rev-a", "unit-a", "sha256:a", "src/a.rs");
    store_pending(&fixture.config, NONCE_A, &pack).unwrap();
    store_pending(&fixture.config, NONCE_B, &pack).unwrap();
    let transcript = transcript_with(&fixture, "child.jsonl", &[NONCE_A]);

    assert!(bind_pending(
        &fixture.config,
        "child-a",
        "worker",
        &transcript
    ));

    assert_eq!(pending_count(&fixture), 1);
    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].recipient_id, "child-a");
}

#[test]
fn missing_or_ambiguous_transcript_marker_binds_nothing() {
    let fixture = Fixture::new(false);
    let pack = pack("rev-a", "unit-a", "sha256:a", "src/a.rs");
    store_pending(&fixture.config, NONCE_A, &pack).unwrap();
    let missing = fixture.transcript("missing.jsonl", "{\"message\":\"none\"}\n");
    let ambiguous = transcript_with(&fixture, "ambiguous.jsonl", &[NONCE_A, NONCE_B]);

    assert!(!bind_pending(&fixture.config, "child", "", &missing));
    assert!(!bind_pending(&fixture.config, "child", "", &ambiguous));

    assert_eq!(pending_count(&fixture), 1);
    assert!(load_deliveries(&fixture.config.work_dir, PLAN, STAGE)
        .unwrap()
        .is_empty());
}

#[test]
fn changed_epoch_or_content_reopens_a_bound_child_pack() {
    let fixture = Fixture::new(false);
    let first = pack("rev-a", "unit", "sha256:old", "src/a.rs");
    let changed_content = pack("rev-a", "unit", "sha256:new", "src/a.rs");
    let changed_epoch = pack("rev-b", "unit", "sha256:new", "src/a.rs");
    store_pending(&fixture.config, NONCE_A, &first).unwrap();
    store_pending(&fixture.config, NONCE_B, &changed_content).unwrap();
    store_pending(&fixture.config, NONCE_C, &changed_epoch).unwrap();
    let transcript_a = transcript_with(&fixture, "a.jsonl", &[NONCE_A]);
    let transcript_b = transcript_with(&fixture, "b.jsonl", &[NONCE_B]);
    let transcript_c = transcript_with(&fixture, "c.jsonl", &[NONCE_C]);

    assert!(bind_pending(
        &fixture.config,
        "child",
        "worker",
        &transcript_a
    ));
    assert!(bind_pending(
        &fixture.config,
        "child",
        "worker",
        &transcript_b
    ));
    assert!(bind_pending(
        &fixture.config,
        "child",
        "worker",
        &transcript_c
    ));

    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    let old = delivered_in_epoch(&records, &context_epoch(&first));
    let new = delivered_in_epoch(&records, &context_epoch(&changed_epoch));
    assert!(old.contains(&("unit".to_string(), "sha256:old".to_string())));
    assert!(old.contains(&("unit".to_string(), "sha256:new".to_string())));
    assert!(new.contains(&("unit".to_string(), "sha256:new".to_string())));
}

#[test]
fn forwarder_binding_credits_only_the_claude_forwarder() {
    let fixture = Fixture::new(false);
    let pack = pack("rev-a", "unit-a", "sha256:a", "src/a.rs");
    let epoch = context_epoch(&pack);
    store_pending(&fixture.config, NONCE_A, &pack).unwrap();
    let transcript = transcript_with(&fixture, "forwarder.jsonl", &[NONCE_A]);

    assert!(bind_pending(
        &fixture.config,
        "claude-forwarder",
        "loom-codex-forwarder",
        &transcript
    ));

    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    assert_eq!(records[0].recipient_id, "claude-forwarder");
    assert!(delivered_to_session(&records, &epoch, "codex-rollout", None).is_empty());
}

#[test]
fn invalid_payload_and_empty_selection_write_nothing() {
    let fixture = Fixture::new(false);

    assert_eq!(issue_line("{}", &fixture.config), "{}");
    assert_eq!(
        issue_line(
            &payload("Implement a topic absent from all indexes"),
            &fixture.config
        ),
        "{}"
    );

    assert_eq!(pending_count(&fixture), 0);
    assert!(!fixture.worker_dir().exists());
}

#[test]
fn oversized_envelope_fails_closed_without_a_pending_record() {
    let mut fixture = Fixture::new(true);
    fixture.config.retrieval.max_payload_bytes = 32;
    let prompt = "Files owned:\n- src/scoped.rs\nImplement the scoped worker";

    assert_eq!(issue_line(&payload(prompt), &fixture.config), "{}");
    assert_eq!(pending_count(&fixture), 0);
}

#[test]
fn task_scoped_brief_material_survives_worker_scope() {
    let mut pack = pack(
        "rev-a",
        "task-brief",
        "sha256:brief",
        "doc/plans/briefs/example/lane/task.md",
    );

    scope_pack(&mut pack, &[]);

    assert_eq!(pack.items[0].id.as_str(), "task-brief");
}

#[test]
fn direct_plan_document_is_excluded_from_worker_scope() {
    let mut pack = pack(
        "rev-a",
        "plan-overview",
        "sha256:plan",
        "doc/plans/DONE-PLAN-example.md",
    );

    scope_pack(&mut pack, &[]);

    assert!(pack.items.is_empty());
}

#[test]
fn scoped_required_plan_document_is_reported_unmet() {
    let mut pack = pack(
        "rev-a",
        "required-plan",
        "sha256:plan",
        "doc/plans/IN_PROGRESS-PLAN-example.md",
    );

    scope_pack(&mut pack, &["required-plan".to_string()]);

    assert_eq!(
        pack.unmet_required,
        vec![UnmetRequirement {
            id: "required-plan".to_string(),
            needed_tokens: 10,
            available_tokens: 0,
            reason: "required item excluded from worker brief scope".to_string(),
        }]
    );
}

#[test]
fn unmet_required_material_is_rendered_without_plan_or_navigation_doctrine() {
    let fixture = Fixture::new(false);
    let mut pack = pack("rev-a", "allowed", "sha256:a", "src/allowed.rs");
    pack.items.push(item(
        "codex-kit",
        "sha256:kit",
        CODEX_FORWARD_PATH,
        "NAVIGATE WITH THE SOURCE GRAPH INSTEAD OF PAGING FILES.",
    ));
    pack.items.push(item(
        "plan-overview",
        "sha256:plan",
        "doc/plans/PLAN-example.md",
        "general stage doctrine",
    ));
    pack.unmet_required.push(UnmetRequirement {
        id: "required-large".to_string(),
        needed_tokens: 900,
        available_tokens: 20,
        reason: "required item exceeds the remaining budget".to_string(),
    });

    scope_pack(&mut pack, &[]);
    let brief = render_brief(&pack, STAGE, NONCE_A).unwrap();

    assert!(brief.contains("required-large"));
    assert!(brief.contains("worker material"));
    assert!(!brief.contains("NAVIGATE WITH THE SOURCE GRAPH"));
    assert!(!brief.contains("general stage doctrine"));
    assert_eq!(brief.lines().filter_map(marker_nonce).count(), 1);
    assert_eq!(pending_count(&fixture), 0, "rendering alone never records");
}
