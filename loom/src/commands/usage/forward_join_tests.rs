use std::collections::HashSet;
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
    let timestamp = Utc.with_ymd_and_hms(2026, 9, 13, 12, 1, 0).unwrap();
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
        requested_model: None,
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

fn write_rollout(path: &Path, thread: &str, input: u64) -> Result<()> {
    fs::write(
        path,
        format!(
            "{}\n{}\n",
            json!({"type":"session_meta","payload":{"id":thread}}),
            json!({"timestamp":"2026-09-13T12:01:00Z","type":"token_usage_record","payload":{"response_id":thread,"usage":{"input_tokens":input,"cached_input_tokens":4,"output_tokens":2,"total_tokens":input+2}}})
        ),
    )?;
    Ok(())
}

#[test]
fn stage_receipt_threads_produce_disjoint_codex_totals() -> Result<()> {
    let work = tempfile::tempdir()?;
    let rollouts = tempfile::tempdir()?;
    for (stage, agent, tool, thread) in [
        ("alpha", "agent-a", "tool-a", "thread-a"),
        ("beta", "agent-b", "tool-b", "thread-b"),
    ] {
        write_receipt(
            work.path(),
            &identity(agent, tool, stage),
            ForwardBackend::Direct,
            thread,
            Some(thread),
        )?;
    }
    let mut paths = Vec::new();
    for (thread, input) in [("thread-a", 10), ("thread-b", 20)] {
        let path = rollouts.path().join(format!("{thread}.jsonl"));
        write_rollout(&path, thread, input)?;
        paths.push(path);
    }
    let join = ForwardJoin::load(Some(work.path()));
    for (stage, expected) in [("alpha", 6), ("beta", 16)] {
        let stages = HashSet::from([stage.to_owned()]);
        let scope = super::super::codex_scope::CodexScope::for_selection(
            false,
            &join,
            &range(),
            Some(&stages),
        );
        let mut normalized =
            super::super::codex_provider::normalize_unclassified(&paths, &range(), &scope);
        let outcome = super::super::codex_scope::apply(&scope, &mut normalized.rows);
        super::super::codex_provider::classify_response_ids(&mut normalized.rows);
        let report = super::super::provider_report::build(
            Provider::Codex,
            &normalized.rows,
            normalized.diagnostics,
        );
        assert_eq!(outcome.missing_threads, 0);
        assert_eq!(report.totals.fresh_input.value, expected);
    }
    Ok(())
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

#[path = "forward_join_tests_joins.rs"]
mod joins;
