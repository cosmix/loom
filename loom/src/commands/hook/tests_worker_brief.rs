use super::*;
use crate::context::delivery::{delivered_in_epoch, load_deliveries, worker_recipient_id};
use crate::context::retrieve::context_epoch;
use crate::context::schema::UnmetRequirement;

#[path = "worker_brief/test_support.rs"]
mod support;
use support::*;
#[path = "tests_worker_brief_dependency.rs"]
mod dependency;

const NONCE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NONCE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NONCE_C: &str = "cccccccccccccccccccccccccccccccc";

#[test]
fn issued_brief_files_one_record_load_deliveries_returns() {
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
    assert_eq!(record_count(&fixture), 1);
    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].recipient_id,
        worker_recipient_id(STAGE, PARENT, &envelope.nonce)
    );
    assert!(!records[0].delivered.is_empty());
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
fn changed_epoch_or_content_reopens_an_issued_worker_pack() {
    let fixture = Fixture::new(false);
    let first = pack("rev-a", "unit", "sha256:old", "src/a.rs");
    let changed_content = pack("rev-a", "unit", "sha256:new", "src/a.rs");
    let changed_epoch = pack("rev-b", "unit", "sha256:new", "src/a.rs");
    store_record(&fixture.config, NONCE_A, &first).unwrap();
    store_record(&fixture.config, NONCE_B, &changed_content).unwrap();
    store_record(&fixture.config, NONCE_C, &changed_epoch).unwrap();
    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    let old = delivered_in_epoch(&records, &context_epoch(&first));
    let new = delivered_in_epoch(&records, &context_epoch(&changed_epoch));
    assert!(old.contains(&("unit".to_string(), "sha256:old".to_string())));
    assert!(old.contains(&("unit".to_string(), "sha256:new".to_string())));
    assert!(new.contains(&("unit".to_string(), "sha256:new".to_string())));
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

    assert_eq!(record_count(&fixture), 0);
    assert!(!fixture.worker_dir().exists());
}

#[test]
fn oversized_envelope_fails_closed_without_a_record() {
    let mut fixture = Fixture::new(true);
    fixture.config.retrieval.max_payload_bytes = 32;
    let prompt = "Files owned:\n- src/scoped.rs\nImplement the scoped worker";

    assert_eq!(issue_line(&payload(prompt), &fixture.config), "{}");
    assert_eq!(record_count(&fixture), 0);
}

#[test]
fn declared_skills_line_names_each_skill_with_its_invocation() {
    let mut fixture = Fixture::new(true);
    fixture.config.stage.skills = vec!["loom-rust".to_string()];
    let prompt =
        "Implement the scoped worker.\n\nFiles owned (write only these):\n- `src/scoped.rs`";

    let envelope = issue(&payload(prompt), &fixture.config).expect("scoped source should match");

    assert!(
        envelope.brief.contains("Required skills for this stage"),
        "{}",
        envelope.brief
    );
    assert!(
        envelope
            .brief
            .contains("Skill(skill=\"loom-skills\", args=\"loom-rust\")"),
        "{}",
        envelope.brief
    );
}

#[test]
fn declared_skills_reach_the_worker_even_with_an_empty_context_pack() {
    let mut fixture = Fixture::new(false);
    fixture.config.stage.skills = vec!["loom-rust".to_string()];

    let envelope = issue(
        &payload("Implement a topic absent from all indexes"),
        &fixture.config,
    )
    .expect("declared skills alone should still emit a brief");

    assert!(envelope.brief.contains("Required skills for this stage"));
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
    let brief = render_brief(&pack, &fixture.config.stage, NONCE_A).unwrap();

    assert!(brief.contains("required-large"));
    assert!(brief.contains("worker material"));
    assert!(!brief.contains("NAVIGATE WITH THE SOURCE GRAPH"));
    assert!(!brief.contains("general stage doctrine"));
    assert_eq!(brief.lines().filter_map(marker_nonce).count(), 1);
    assert_eq!(record_count(&fixture), 0, "rendering alone never records");
}

#[test]
fn worker_query_leads_with_the_worker_task() {
    let stage = Stage {
        id: "q-stage".to_string(),
        name: "Distinct stage name".to_string(),
        ..Stage::default()
    };

    let query = worker_query_text(&stage, "worker task", "worker prompt");

    let task = query.find("worker task").expect("task present");
    let prompt = query.find("worker prompt").expect("prompt present");
    let name = query.find("Distinct stage name").expect("stage present");
    assert!(task < prompt && prompt < name, "{query}");
}

#[test]
fn capped_query_still_issues_a_scoped_brief() {
    let mut fixture = Fixture::new(true);
    fixture.config.stage.description = Some("stage words ".repeat(4_000));
    let prompt = format!(
        "Implement the scoped worker.\n\nFiles owned (write only these):\n- `src/scoped.rs`\n{}",
        "filler words ".repeat(300)
    );

    let envelope = issue(&payload(&prompt), &fixture.config).expect("capped query should issue");

    assert!(envelope.brief.contains("`src/scoped.rs`"));
    assert_eq!(
        envelope
            .brief
            .lines()
            .filter(|line| marker_nonce(line).is_some())
            .count(),
        1
    );
    assert_eq!(record_count(&fixture), 1);
    let records = load_deliveries(&fixture.config.work_dir, PLAN, STAGE).unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].delivered.is_empty());
    assert_pack_query_capped(&fixture, &payload(&prompt));
}

/// `retrieve_pack` must route the query through `worker_query_text`: the pack
/// records the query text it was built from, so an uncapped query shows here.
fn assert_pack_query_capped(fixture: &Fixture, raw: &str) {
    let parsed: HookPayload = serde_json::from_str(raw).expect("payload parses");
    let pack = retrieve_pack(&fixture.config, &parsed, &[]).expect("pack retrieves");
    assert!(
        pack.query.len() <= WORKER_QUERY_MAX_BYTES,
        "query is {} bytes",
        pack.query.len()
    );
}
