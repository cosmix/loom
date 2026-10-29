//! Source contracts for the stage's chat wiring.

#[test]
fn chat_wiring_names_both_draft_team_and_kickoff() {
    let source = include_str!("../mod.rs");
    assert!(source.contains("team::publish_draft_team("));
    assert!(source.contains("kickoff"));
}

#[test]
fn confirmation_and_delta_publishing_are_sibling_modules() {
    let source = include_str!("../mod.rs");
    assert!(source.contains("mod kickoff_cmd;"));
    assert!(source.contains("mod publish;"));
    assert!(!include_str!("../dispatch.rs").contains("#[path"));
    let team = include_str!("../team.rs");
    assert!(!team.contains("fn delta_message("));
    assert!(!team.contains("ChatDelta::Queue"));
    assert!(include_str!("../publish.rs").contains("fn delta_message("));
}
