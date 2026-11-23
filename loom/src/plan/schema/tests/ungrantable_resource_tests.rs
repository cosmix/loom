use super::{make_stage, AcceptanceCriterion};
use crate::plan::schema::validate_structural_preflight;

fn resource_warnings(command: &str) -> Vec<String> {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec![AcceptanceCriterion::Simple(command.into())];
    validate_structural_preflight(&[stage], None)
        .into_iter()
        .filter(|warning| {
            warning.contains("Acceptance criterion #1")
                && warning.contains("host command confinement cannot grant")
        })
        .collect()
}

#[test]
fn preflight_warns_when_commands_invoke_ungrantable_resources() {
    for (command, resource) in [
        ("loom map --outline src/lib.rs", "loom map"),
        ("./target/debug/loom map --outline src/lib.rs", "loom map"),
        (
            "sh -c 'loom knowledge context --stage x'",
            "loom knowledge context",
        ),
        ("tmux kill-server", "tmux"),
        ("docker ps", "docker"),
        ("echo \"$(docker ps)\"", "docker"),
        ("docker ps; loom map --outline src/lib.rs", "docker"),
    ] {
        let warnings = resource_warnings(command);
        assert_eq!(warnings.len(), 1, "{command}: {warnings:?}");
        assert!(
            warnings[0].contains(&format!("invokes '{resource}'")),
            "{command}: {warnings:?}"
        );
    }
}

#[test]
fn preflight_ignores_ungrantable_resource_names_in_arguments() {
    for command in [
        "rg -n 'loom map' doc/x.md",
        "echo \"loom knowledge context\"",
        "rg -n tmux docs/x.md",
        "cargo test --test tmux_backend",
    ] {
        let warnings = resource_warnings(command);
        assert!(warnings.is_empty(), "{command}: {warnings:?}");
    }
}
