use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use clap::Parser;
use serde_json::json;

use super::*;
use crate::commands::usage::provider_types::{
    ProviderLedger, TimeRangeView, PROVIDER_LEDGER_SCHEMA_VERSION,
};
use crate::commands::usage::time_range::TimeRange;
use crate::commands::usage::transcript::{
    Request, RequestNormalization, TokenUsage, ToolUse, TranscriptDiagnostics, UserEntry,
};
use crate::models::forward_receipt::{ForwardObservation, FORWARD_RECEIPT_SCHEMA};

fn identity(agent: &str, tool: &str, stage: &str) -> ForwardIdentity {
    ForwardIdentity::new("parent-1", agent, tool, stage, "loom-1").expect("safe fixture identity")
}

fn write_receipt(
    root: &Path,
    identity: &ForwardIdentity,
    backend: ForwardBackend,
    backend_id: &str,
    thread_id: Option<&str>,
) -> Result<()> {
    let path = receipts_path(root, &identity.stage_id)?;
    fs::create_dir_all(path.parent().expect("receipt has parent"))?;
    let observation = ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id.clone(),
        agent_id: identity.agent_id.clone(),
        tool_use_id: identity.tool_use_id.clone(),
        stage_id: identity.stage_id.clone(),
        loom_session_id: identity.loom_session_id.clone(),
        backend,
        backend_id: backend_id.to_owned(),
        state: ForwardState::Running,
        observed_at: Utc
            .with_ymd_and_hms(2026, 9, 13, 12, 0, 0)
            .single()
            .unwrap(),
        exit_code: None,
        codex_thread_id: thread_id.map(str::to_owned),
        locator: None,
        model: Some("same-model".to_owned()),
        effort: Some("xhigh".to_owned()),
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", observation.encode_line()?)?;
    Ok(())
}

fn transcript(identity: &ForwardIdentity) -> Transcript {
    let timestamp = Utc
        .with_ymd_and_hms(2026, 9, 13, 12, 1, 0)
        .single()
        .unwrap();
    let request = Request {
        message_id: Some("message-1".to_owned()),
        timestamp,
        model: "same-model".to_owned(),
        usage: TokenUsage {
            input: 7,
            output: 3,
            ..TokenUsage::default()
        },
        tool_uses: vec![ToolUse {
            id: identity.tool_use_id.clone(),
            name: "Bash".to_owned(),
            input: json!({"private": "never serialized"}),
        }],
        thinking_chars: 0,
        text_chars: 0,
        normalization: RequestNormalization {
            usage_observed: true,
            ..Default::default()
        },
    };
    Transcript {
        path: "private/transcript.jsonl".into(),
        scope: Scope::Subagent,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: identity.parent_session_id.clone(),
        agent_id: Some(identity.agent_id.clone()),
        agent_type: Some(FORWARDER_TYPE.to_owned()),
        stage_id: Some(identity.stage_id.clone()),
        loom_session_id: Some(identity.loom_session_id.clone()),
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries: vec![
            Entry::Assistant(Box::new(request)),
            Entry::User(UserEntry {
                timestamp,
                tool_use_id: Some(identity.tool_use_id.clone()),
                text: "private result".to_owned(),
            }),
        ],
        diagnostics: TranscriptDiagnostics::default(),
    }
}

fn range() -> TimeRange {
    TimeRange {
        since: Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).single().unwrap(),
        until: None,
    }
}

fn codex_ledger(thread_ids: &[&str]) -> Result<ProviderLedger> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("rollout.jsonl");
    let mut lines = thread_ids
        .iter()
        .map(|id| json!({"type":"session_meta","payload":{"id":id}}).to_string())
        .collect::<Vec<_>>();
    lines.push(
        json!({
            "timestamp":"2026-09-13T12:00:00Z", "type":"token_usage_record",
            "payload":{"response_id":"response-1","usage":{
                "input_tokens":10,"cached_input_tokens":4,"output_tokens":2,"total_tokens":12
            }}
        })
        .to_string(),
    );
    fs::write(path.clone(), format!("{}\n", lines.join("\n")))?;
    let normalized = super::super::codex_provider::normalize(&[path], &range());
    let report = super::super::provider_report::build(
        Provider::Codex,
        &normalized.rows,
        normalized.diagnostics,
    );
    Ok(ProviderLedger {
        schema_version: PROVIDER_LEDGER_SCHEMA_VERSION,
        range: TimeRangeView {
            since: range().since,
            until: None,
        },
        providers: vec![report],
        rows: normalized.rows,
        quota_history: None,
    })
}

#[test]
fn explicit_project_reads_only_its_receipt_root() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    let root_a = project_a.join(".loom/work");
    let root_b = project_b.join(".loom/work");
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(
        &root_a,
        &id,
        ForwardBackend::Companion,
        "job-a",
        Some("thread-a"),
    )?;
    write_receipt(
        &root_b,
        &id,
        ForwardBackend::Companion,
        "job-b",
        Some("thread-b"),
    )?;

    let selected = super::super::usage_work_dir(Some(&project_a), false).unwrap();
    let mut parsed = transcript(&id);
    ForwardJoin::load(Some(&selected)).join_transcript(&mut parsed);
    let foreign = super::super::forward_receipts_root(
        false,
        Some(&root_b),
        Some(&project_a),
        Some(&selected),
    );

    assert_eq!(parsed.forward_receipt.unwrap().backend_id, "job-a");
    assert!(foreign.is_err());
    Ok(())
}

#[test]
fn current_project_uses_the_resolved_usage_work_dir() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join(".loom/work");
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(&root, &id, ForwardBackend::Direct, "thread-a", None)?;
    let selected = super::super::forward_receipts_root(false, None, None, Some(&root))?;
    let mut parsed = transcript(&id);

    ForwardJoin::load(selected).join_transcript(&mut parsed);

    assert_eq!(parsed.forward_receipt.unwrap().backend_id, "thread-a");
    Ok(())
}

#[test]
fn missing_explicit_root_never_falls_back() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("work");
    let missing = temp.path().join("missing");
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(&root, &id, ForwardBackend::Direct, "thread-a", None)?;
    let foreign = super::super::forward_receipts_root(false, Some(&missing), None, Some(&root));
    assert!(foreign.is_err());

    let selected =
        super::super::forward_receipts_root(false, Some(&missing), None, Some(&missing))?;
    let join = ForwardJoin::load(selected);
    let mut parsed = transcript(&id);

    join.join_transcript(&mut parsed);

    assert!(parsed.forward_receipt.is_none());
    assert_eq!(join.load_diagnostics.missing_forward_receipt_roots, 1);
    Ok(())
}

#[test]
fn all_scope_stays_unattributed_with_diagnostic() {
    let id = identity("agent-1", "tool-1", "stage-a");
    let root = super::super::forward_receipts_root(true, None, None, None)
        .expect("all scope has no receipt root");
    let join = ForwardJoin::load(root);
    let mut parsed = transcript(&id);

    join.join_transcript(&mut parsed);

    assert!(parsed.forward_candidate && parsed.forward_receipt.is_none());
    assert_eq!(join.load_diagnostics.forward_receipt_scope_unavailable, 1);
}

#[derive(Parser)]
#[allow(dead_code)]
struct UsageHarness {
    #[command(flatten)]
    args: super::super::UsageArgs,
}

#[test]
fn forward_receipts_root_is_rejected_with_all() {
    let parsed = UsageHarness::try_parse_from([
        "usage",
        "--all",
        "--forward-receipts-root",
        "/tmp/receipts",
    ]);
    assert!(parsed.is_err());
}

#[test]
fn same_model_siblings_join_only_their_exact_tuple() -> Result<()> {
    let root = tempfile::tempdir()?;
    let left_id = identity("agent-left", "tool-left", "stage-a");
    let right_id = identity("agent-right", "tool-right", "stage-a");
    write_receipt(
        root.path(),
        &left_id,
        ForwardBackend::Companion,
        "job-left",
        None,
    )?;
    write_receipt(
        root.path(),
        &right_id,
        ForwardBackend::Companion,
        "job-right",
        None,
    )?;
    let join = ForwardJoin::load(Some(root.path()));
    let (mut left, mut right) = (transcript(&left_id), transcript(&right_id));

    join.join_transcript(&mut left);
    join.join_transcript(&mut right);

    assert_eq!(left.forward_receipt.unwrap().backend_id, "job-left");
    assert_eq!(right.forward_receipt.unwrap().backend_id, "job-right");
    Ok(())
}

#[test]
fn codex_thread_joins_one_unique_receipt_and_redacts_private_fields() -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(
        root.path(),
        &id,
        ForwardBackend::Companion,
        "private-job",
        Some("thread-a"),
    )?;
    let mut ledger = codex_ledger(&["thread-a"])?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    let value = serde_json::to_value(&ledger.rows[0])?;
    assert_eq!(value["forward_receipt"]["receipt_id"], id.receipt_id());
    assert_eq!(value["forward_receipt"]["backend"], "companion");
    assert_eq!(value["forward_receipt"]["state"], "running");
    assert!(!value.to_string().contains("private-job"));
    Ok(())
}

#[test]
fn codex_thread_shared_by_receipts_is_ambiguous() -> Result<()> {
    let root = tempfile::tempdir()?;
    write_receipt(
        root.path(),
        &identity("agent-a", "tool-a", "stage-a"),
        ForwardBackend::Companion,
        "job-a",
        Some("same-thread"),
    )?;
    write_receipt(
        root.path(),
        &identity("agent-b", "tool-b", "stage-b"),
        ForwardBackend::Companion,
        "job-b",
        Some("same-thread"),
    )?;
    let mut ledger = codex_ledger(&["same-thread"])?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    assert!(ledger.rows[0].forward_receipt.is_none());
    assert_eq!(
        ledger.providers[0].diagnostics.forward_identity_ambiguous,
        1
    );
    Ok(())
}

#[test]
fn codex_absent_or_conflicting_thread_is_diagnostic() -> Result<()> {
    let root = tempfile::tempdir()?;
    let join = ForwardJoin::load(Some(root.path()));
    let mut absent = codex_ledger(&[])?;
    let mut conflicting = codex_ledger(&["thread-a", "thread-b"])?;

    join.join_ledger(&mut absent);
    join.join_ledger(&mut conflicting);

    assert_eq!(absent.providers[0].diagnostics.forward_identity_absent, 1);
    assert_eq!(
        conflicting.providers[0]
            .diagnostics
            .forward_identity_conflicts,
        1
    );
    Ok(())
}

#[test]
fn joining_changes_no_tokens_counts_or_usage_receipts() -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(root.path(), &id, ForwardBackend::Direct, "thread-a", None)?;
    let mut ledger = codex_ledger(&["thread-a"])?;
    let mut usage_receipt = ledger.rows[0].clone();
    usage_receipt.source_kind = SourceKind::ExecutionReceipt;
    usage_receipt.codex_thread_id = None;
    ledger.rows.push(usage_receipt);
    let receipt_before = serde_json::to_vec(&ledger.rows[1])?;
    let row_count = ledger.rows.len();
    let before = serde_json::to_value((
        &ledger.rows[0].tokens,
        &ledger.providers[0].coverage,
        &ledger.providers[0].totals,
    ))?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    let after = serde_json::to_value((
        &ledger.rows[0].tokens,
        &ledger.providers[0].coverage,
        &ledger.providers[0].totals,
    ))?;
    assert_eq!(before, after);
    assert_eq!(receipt_before, serde_json::to_vec(&ledger.rows[1])?);
    assert_eq!(row_count, ledger.rows.len());
    Ok(())
}
