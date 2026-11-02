use crate::models::stage::AcceptanceCriterion;

/// Render the command and its observable success conditions. Every signal kind
/// lists criteria through this, so an expected nonzero exit is never lost.
pub(in crate::orchestrator::signals) fn format_criterion(
    criterion: &AcceptanceCriterion,
) -> String {
    let mut line = format!("- [ ] {}", criterion.command());
    match criterion {
        AcceptanceCriterion::Simple(_) => line.push_str(" (expected exit code: 0)"),
        AcceptanceCriterion::Extended(check) => {
            line.push_str(&format!(
                " (expected exit code: {})",
                check.exit_code.unwrap_or(0)
            ));
            for value in &check.stdout_contains {
                line.push_str(&format!("; stdout contains {value:?}"));
            }
            for value in &check.stdout_not_contains {
                line.push_str(&format!("; stdout excludes {value:?}"));
            }
            if check.stderr_empty == Some(true) {
                line.push_str("; stderr empty");
            }
            if let Some(timeout) = check.timeout_secs {
                line.push_str(&format!("; timeout: {timeout}s"));
            }
            if let Some(description) = &check.description {
                line.push_str(&format!("; {description}"));
            }
        }
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_negative_exit_and_output_conditions() {
        let criterion: AcceptanceCriterion = crate::yaml::from_str(
            "command: check-absent\nexit_code: 1\nstdout_contains: [missing]\nstdout_not_contains: [present]\nstderr_empty: true\n",
        ).unwrap();
        let rendered = format_criterion(&criterion);
        assert!(
            rendered.contains("check-absent (expected exit code: 1)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("stdout contains \"missing\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("stdout excludes \"present\""),
            "{rendered}"
        );
        assert!(rendered.contains("stderr empty"), "{rendered}");
        assert!(
            format_criterion(&AcceptanceCriterion::Simple("check".into()))
                .contains("expected exit code: 0")
        );
    }
}
