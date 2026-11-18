use std::{fs, path::Path};

use super::generate::{build_embedded_context_with_stage, generate_signal_with_metrics};
use super::recovery_format::format_recovery_signal;
use super::recovery_types::RecoverySignalContent;
use super::tests::{create_test_session, create_test_stage, create_test_worktree};
use crate::quota::{ProviderQuota, QuotaWindow, WindowKind};

fn write_fresh_quota(work_dir: &Path) {
    let reading = ProviderQuota {
        observed_at: chrono::Utc::now().timestamp(),
        windows: vec![QuotaWindow {
            kind: WindowKind::FiveHour,
            used_percent: 80.0,
            resets_at: None,
        }],
        plan: None,
        error: None,
    };
    crate::quota::cache::write_provider(work_dir, "claude", &reading).unwrap();
}

fn generated_signal(work_dir: &Path) -> String {
    let (path, _) = generate_signal_with_metrics(
        &create_test_session(),
        &create_test_stage(),
        &create_test_worktree(),
        &[],
        None,
        None,
        work_dir,
    )
    .unwrap();
    fs::read_to_string(path).unwrap()
}

fn assert_before_assignment(signal: &str) {
    assert!(signal.find("## Model Budget").unwrap() < signal.find("## Assignment").unwrap());
}

#[test]
fn stage_signal_gates_budget_on_fresh_cache() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!generated_signal(dir.path()).contains("## Model Budget"));
    write_fresh_quota(dir.path());
    assert_before_assignment(&generated_signal(dir.path()));
}

#[test]
fn recovery_signal_gates_budget_on_fresh_cache() {
    let dir = tempfile::tempdir().unwrap();
    let content = RecoverySignalContent::for_crash(
        "session-recovered".into(),
        "stage".into(),
        "session-crashed".into(),
        None,
        1,
    );
    let stage = create_test_stage();
    let embedded = build_embedded_context_with_stage(dir.path(), None, None);
    assert!(!format_recovery_signal(&content, &stage, &embedded).contains("## Model Budget"));
    write_fresh_quota(dir.path());
    let embedded = build_embedded_context_with_stage(dir.path(), None, None);
    assert_before_assignment(&format_recovery_signal(&content, &stage, &embedded));
}

#[test]
fn knowledge_and_contract_signals_never_carry_the_budget() {
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("work");
    fs::create_dir_all(&work_dir).unwrap();
    write_fresh_quota(&work_dir);

    let mut knowledge_stage = create_test_stage();
    knowledge_stage.stage_type = crate::models::stage::StageType::Knowledge;
    let knowledge = super::generate_knowledge_signal(
        &crate::models::session::Session::new_knowledge(&knowledge_stage.id),
        &knowledge_stage,
        dir.path(),
        &[],
        &work_dir,
        None,
    )
    .unwrap();

    let mut contract_stage = create_test_stage();
    contract_stage.plan_version = 2;
    let contract = super::generate_contract_signal(
        &crate::models::session::Session::new_contract(&contract_stage.id),
        &contract_stage,
        &create_test_worktree(),
        &work_dir,
        None,
        &[],
    )
    .unwrap();

    for path in [knowledge, contract] {
        let signal = fs::read_to_string(path).unwrap();
        assert!(!signal.is_empty());
        assert!(!signal.contains("## Model Budget"), "{signal}");
    }
}
