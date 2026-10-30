//! Contract-writer-phase tests for the static `loom status` tree, split out
//! of `graph_tests.rs` to keep that file under the line-count limit.

use super::{make_stage_summary, make_status_data};
use crate::commands::status::render::graph::render_graph;
use crate::models::session::SessionType;
use crate::models::stage::{StageStatus, StageType};

#[test]
fn test_executing_with_contract_session_shows_contracts_tag_not_foreign_kind_tag() {
    let mut stage = make_stage_summary("my-stage", vec![], StageStatus::Executing);
    stage.session_type = Some(SessionType::Contract);
    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![stage])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(!output_str.contains("contract session"), "{output_str}");
    assert!(
        output_str.contains("contracts"),
        "expected the contract-phase tag: {output_str}"
    );
}

#[test]
fn test_executing_with_contract_session_on_knowledge_stage_keeps_foreign_kind_tag() {
    // The contract-writer phase only exists for Standard stages; a Contract
    // session found on any other stage type is a coherence anomaly and must
    // keep the ordinary "{session_type} session" warning instead of being
    // read as the contract phase.
    let mut stage = make_stage_summary("my-stage", vec![], StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.session_type = Some(SessionType::Contract);
    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![stage])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("contract session"), "{output_str}");
}

#[test]
fn test_legend_adds_contracts_entry_only_when_contract_phase_present() {
    let mut writer = make_stage_summary("writer", vec![], StageStatus::Executing);
    writer.session_type = Some(SessionType::Contract);

    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![writer])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("contracts"),
        "legend should mention contracts: {output_str}"
    );

    let plain = make_stage_summary("plain", vec![], StageStatus::Executing);
    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![plain])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(
        !output_str.contains("contracts"),
        "legend should not mention contracts without a contract-phase stage: {output_str}"
    );
}

#[test]
fn test_contract_testing_replaces_the_tag_and_drops_the_silence_icon() {
    use crate::commands::status::data::{ActivityStatus, ContractTesting};
    let mut stage = make_stage_summary("my-stage", vec![], StageStatus::Executing);
    stage.session_type = Some(SessionType::Contract);
    stage.activity_status = ActivityStatus::Quiet;
    stage.contract_testing = Some(ContractTesting {
        done: 9,
        total: 11,
        queued: false,
        running_secs: Some(372),
    });
    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![stage.clone()])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("contracts written, testing 9/11 · 6m12s"),
        "{output_str}"
    );
    assert!(
        !output_str.contains(ActivityStatus::Quiet.icon()),
        "{output_str}"
    );

    stage.activity_status = ActivityStatus::Orphaned;
    let mut output = Vec::new();
    render_graph(&mut output, &make_status_data(vec![stage])).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(!output_str.contains("testing 9/11"), "{output_str}");
}

#[test]
fn test_contract_testing_is_hidden_when_the_writer_pid_is_dead() {
    use crate::commands::status::data::ContractTesting;
    let mut stage = make_stage_summary("my-stage", vec![], StageStatus::Executing);
    stage.session_type = Some(SessionType::Contract);
    stage.contract_testing = Some(ContractTesting {
        done: 3,
        total: 11,
        queued: false,
        running_secs: Some(60),
    });
    stage.pid = Some(999_999_999);
    stage.session_alive = false;
    assert_eq!(stage.contract_testing_text(), None);
    stage.session_alive = true;
    assert!(stage.contract_testing_text().is_some());
    stage.pid = None;
    stage.session_alive = false;
    assert!(
        stage.contract_testing_text().is_some(),
        "no pid is not dead"
    );
}
