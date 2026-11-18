use super::*;

#[test]
fn qualified_agent_type_stop_is_terminal_and_must_match_start() -> Result<()> {
    let fixture = Fixture::new("qualified-stop")?;
    fs::write(&fixture.worker, "{\"message\":\"done\"}\n")?;
    fixture.write_start("worker-a", "codex:codex-rescue")?;
    let starts =
        crate::commands::subagents::ledger::StartedAgentTypeIndex::load(Some(&fixture.work));
    let mut payload = serde_json::json!({
        "session_id": PARENT, "agent_id": "worker-a", "agent_type": "codex:codex-rescue",
        "transcript_path": fixture.parent.display().to_string(),
        "agent_transcript_path": fixture.worker.display().to_string(),
    });
    let env = super::super::ClaudeEnvironment {
        work_dir: &fixture.work,
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
        observed_at: "2026-09-13T10:00:01Z".parse()?,
    };
    let active = super::super::ActiveStageSession {
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
    };
    let record = super::super::validate_subagent_stop(&payload, &env, &starts, &active)?;
    append_locked(&fixture.work, &record)?;
    assert_eq!(
        replay(&fixture.work)?.outcome(&record.identity),
        WorkerOutcome::Succeeded
    );
    for bad_type in ["other:worker", "bad/type", "bad\nvalue", &"a".repeat(129)] {
        payload["agent_type"] = bad_type.into();
        assert!(super::super::validate_subagent_stop(&payload, &env, &starts, &active).is_err());
    }
    Ok(())
}
