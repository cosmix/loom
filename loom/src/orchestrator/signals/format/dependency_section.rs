use super::super::types::DependencyStatus;
use super::helpers::{format_dependency_outputs, format_dependency_table};

/// The `## Dependencies Status` table and, when any dependency recorded outputs,
/// the `## Dependency Outputs` block. Empty for a stage without dependencies.
pub(in crate::orchestrator::signals) fn format_dependency_sections(
    deps: &[DependencyStatus],
) -> String {
    if deps.is_empty() {
        return String::new();
    }
    let mut content = String::from("## Dependencies Status\n\n");
    content.push_str(&format_dependency_table(deps));
    content.push('\n');

    let with_outputs: Vec<_> = deps.iter().filter(|d| !d.outputs.is_empty()).collect();
    if !with_outputs.is_empty() {
        content.push_str("## Dependency Outputs\n\n");
        content.push_str(&format_dependency_outputs(&with_outputs));
        content.push('\n');
    }
    content
}
