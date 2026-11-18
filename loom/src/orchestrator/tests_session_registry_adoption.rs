//! Orphan discovery and adoption tests for the session registry.

use super::*;

#[test]
fn orphan_evidence_finds_the_recordless_agent_and_ignores_every_recorded_one() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    stage_at(work, "beta", StageStatus::Executing);
    let healthy = session_for("beta", SessionStatus::Running);
    spawn_a_live_agent(work, &healthy);
    save_session(&healthy, work).unwrap();

    stage_at(work, "gamma", StageStatus::Executing);
    let stale_record = session_for("gamma", SessionStatus::Completed);
    spawn_a_live_agent(work, &stale_record);
    save_session(&stale_record, work).unwrap();

    stage_at(work, "delta", StageStatus::Queued);
    let not_executing = session_for("delta", SessionStatus::Running);
    spawn_a_live_agent(work, &not_executing);

    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1, "unexpected evidence: {evidence:?}");
    assert_eq!(
        evidence[0],
        OrphanEvidence {
            session_id: orphan.id.clone(),
            stage_id: "alpha".to_string(),
            tracking_key: orphan.tracking_key.clone(),
            session_type: SessionType::Stage,
            pid: std::process::id(),
            backend: SessionBackendKind::Native,
        }
    );
}

#[test]
fn a_dead_agents_pid_file_is_not_evidence_of_an_orphan() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let corpse = session_for("alpha", SessionStatus::Running);
    spawn_a_dead_agent(work, &corpse);

    assert!(orphan_evidence(work).is_empty());
}

#[test]
fn adoption_is_idempotent_because_its_record_hides_the_pid_file() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    let first = orphan_evidence(work);
    assert_eq!(first.len(), 1);
    adopt_orphan(work, &first[0]).unwrap();

    assert!(
        orphan_evidence(work).is_empty(),
        "a second pass re-adopted an agent it had already recorded"
    );
}

#[test]
fn an_adopted_record_is_attachable_again() {
    let temp = work_dir();
    let work = temp.path();

    let session_id = "session-abcd1234-1700000000";
    let evidence = OrphanEvidence {
        session_id: session_id.to_string(),
        stage_id: "alpha".to_string(),
        tracking_key: "loom-v2-stage-alpha".to_string(),
        session_type: SessionType::Stage,
        pid: std::process::id(),
        backend: SessionBackendKind::Tmux,
    };

    let mut spawned = Session::new();
    spawned.id = session_id.to_string();
    spawned.assign_to_stage("alpha".to_string());
    spawn_a_live_agent(work, &spawned);

    let adopted = adopt_orphan(work, &evidence).unwrap();
    assert_eq!(adopted.id, session_id);
    assert_eq!(adopted.stage_id.as_deref(), Some("alpha"));
    assert_eq!(adopted.tracking_key, "loom-v2-stage-alpha");
    assert_eq!(adopted.session_type, SessionType::Stage);
    assert_eq!(adopted.status, SessionStatus::Running);
    assert_eq!(adopted.backend, SessionBackendKind::Tmux);
    assert_eq!(adopted.pid, Some(std::process::id()));

    let attachable = live_tmux_sessions(work).unwrap();
    assert_eq!(ids(&attachable), vec![session_id]);
    let live = live_sessions_for_stage(work, "alpha").unwrap();
    assert_eq!(ids(&live), vec![session_id]);
}

#[test]
fn the_adoption_pass_links_the_stage_without_touching_its_status() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1);
    let adopted = adopt_orphan(work, &evidence[0]).unwrap();

    crate::verify::transitions::update_stage("alpha", work, |stage| {
        if stage.status == StageStatus::Executing && stage.session.is_none() {
            stage.session = Some(adopted.id.clone());
        }
        Ok(())
    })
    .unwrap();

    let stage = crate::verify::transitions::load_stage("alpha", work).unwrap();
    assert_eq!(stage.session.as_deref(), Some(adopted.id.as_str()));
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn a_merge_agents_tracking_key_survives_adoption() {
    let temp = work_dir();
    let work = temp.path();

    let evidence = OrphanEvidence {
        session_id: "session-beef0001-1700000001".to_string(),
        stage_id: "alpha".to_string(),
        tracking_key: "loom-merge-alpha".to_string(),
        session_type: SessionType::Merge,
        pid: std::process::id(),
        backend: SessionBackendKind::Native,
    };

    let adopted = adopt_orphan(work, &evidence).unwrap();
    assert_eq!(adopted.tracking_key, "loom-merge-alpha");
    assert_eq!(adopted.session_type, SessionType::Merge);
}

/// A contract session is the stage's agent while it runs, so a daemon that
/// died before writing its record must still find it by its tracking key.
#[test]
fn a_recordless_contract_agent_is_found_by_its_tracking_key() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let mut orphan = Session::new_contract("alpha");
    orphan.status = SessionStatus::Running;
    spawn_a_live_agent(work, &orphan);

    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1, "unexpected evidence: {evidence:?}");
    assert_eq!(evidence[0].session_id, orphan.id);
    assert_eq!(evidence[0].tracking_key, orphan.tracking_key);
    assert_eq!(evidence[0].session_type, SessionType::Contract);
}

#[test]
fn ambiguous_legacy_tracking_key_is_never_adopted() {
    for contract_stage in ["auth", "z-auth"] {
        let temp = work_dir();
        let work = temp.path();
        let stage_alias = format!("contract-{contract_stage}");
        stage_at(work, contract_stage, StageStatus::Executing);
        stage_at(work, &stage_alias, StageStatus::Executing);
        let mut orphan = Session::new_contract(contract_stage);
        orphan.status = SessionStatus::Running;
        orphan.tracking_key = Session::legacy_tracking_key(contract_stage, SessionType::Contract);
        std::fs::create_dir_all(work.join("pids")).unwrap();
        std::fs::write(
            work.join("pids")
                .join(format!("{}-{}.pid", orphan.tracking_key, orphan.id)),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        assert!(orphan_evidence(work).is_empty());

        let current = Session::new_contract(contract_stage);
        spawn_a_live_agent(work, &current);
        let evidence = orphan_evidence(work);
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].stage_id, contract_stage);
        assert_eq!(evidence[0].session_type, SessionType::Contract);
    }
}

/// Two executing stages whose pairs (`stage`, Contract) and
/// (`contract-<stage>`, Stage) were one legacy key. The pid file belongs to the
/// `contract-<stage>` stage's own session; the other stage sorts first for
/// `auth` and last for `z-auth`, so sorted iteration reaches the wrong stage
/// first in one order and the right one first in the other.
#[test]
fn a_stage_session_aliasing_another_stages_contract_keeps_its_owner() {
    for contract_stage in ["auth", "z-auth"] {
        let temp = work_dir();
        let work = temp.path();
        let owner = format!("contract-{contract_stage}");
        stage_at(work, contract_stage, StageStatus::Executing);
        stage_at(work, &owner, StageStatus::Executing);
        let orphan = session_for(&owner, SessionStatus::Running);
        spawn_a_live_agent(work, &orphan);

        let evidence = orphan_evidence(work);

        assert_eq!(evidence.len(), 1, "unexpected evidence: {evidence:?}");
        assert_eq!(evidence[0].session_id, orphan.id);
        assert_eq!(evidence[0].stage_id, owner);
        assert_eq!(evidence[0].session_type, SessionType::Stage);
        assert_eq!(evidence[0].tracking_key, orphan.tracking_key);
    }
}

#[test]
fn an_unambiguous_legacy_stage_pid_is_still_adopted() {
    let temp = work_dir();
    let work = temp.path();
    stage_at(work, "alpha", StageStatus::Executing);
    let mut orphan = session_for("alpha", SessionStatus::Running);
    orphan.tracking_key = "loom-alpha".to_string();
    std::fs::create_dir_all(work.join("pids")).unwrap();
    std::fs::write(
        work.join("pids")
            .join(format!("{}-{}.pid", orphan.tracking_key, orphan.id)),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].tracking_key, orphan.tracking_key);
    assert_eq!(evidence[0].session_type, SessionType::Stage);
}

#[test]
fn missing_new_owner_metadata_cannot_be_adopted_as_a_legacy_stage() {
    let temp = work_dir();
    let work = temp.path();
    stage_at(work, "auth", StageStatus::Executing);
    stage_at(work, "v2-stage-auth", StageStatus::Executing);
    let orphan = session_for("auth", SessionStatus::Running);
    std::fs::create_dir_all(work.join("pids")).unwrap();
    std::fs::write(
        work.join("pids")
            .join(format!("{}-{}.pid", orphan.tracking_key, orphan.id)),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    assert!(orphan_evidence(work).is_empty());
}
