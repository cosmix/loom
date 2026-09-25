use crate::models::stage::Stage;

/// The `## Goal-Backward Verification` block: outcome checks beyond acceptance
/// criteria (artifacts, wiring, wiring tests, dead code, regression test,
/// reachable units).
///
/// Split out of `sections.rs` to keep it under its maintainability
/// line-count baseline. Call only when `stage.has_any_goal_checks()` is true.
pub(super) fn format_goal_backward_verification_section(stage: &Stage) -> String {
    let mut content = String::new();
    content.push_str("\n## Goal-Backward Verification\n\n");
    content.push_str("Beyond acceptance criteria, verify these OUTCOMES work:\n\n");
    content.push_str(&format_artifacts_subsection(stage));
    content.push_str(&format_wiring_subsection(stage));
    content.push_str(&format_wiring_tests_subsection(stage));
    content.push_str(&format_dead_code_subsection(stage));
    content.push_str(&format_regression_test_subsection(stage));
    content.push_str(&format_reachable_subsection(stage));
    content.push_str("Run `loom check <stage-id> --suggest` to check these automatically.\n\n");
    content
}

fn format_artifacts_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if !stage.artifacts.is_empty() {
        content.push_str("### Artifacts (files must exist with real implementation)\n\n");
        for artifact in &stage.artifacts {
            content.push_str(&format!("- `{artifact}`\n"));
        }
        content.push('\n');
    }
    content
}

fn format_wiring_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if !stage.wiring.is_empty() {
        content.push_str("### Wiring (critical connections to verify)\n\n");
        for check in &stage.wiring {
            content.push_str(&format!(
                "- **{}**: pattern `{}` in `{}`\n",
                check.description, check.pattern, check.source
            ));
        }
        content.push('\n');
    }
    content
}

fn format_wiring_tests_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if stage.wiring_tests.is_empty() {
        return content;
    }
    content.push_str("### Wiring Tests (integration commands)\n\n");
    for test in &stage.wiring_tests {
        content.push_str(&format!("**{}:** `{}`\n", test.name, test.command));
        if let Some(desc) = &test.description {
            content.push_str(&format!("  *{}*\n", desc));
        }
        let mut criteria = Vec::new();
        if let Some(code) = test.success_criteria.exit_code {
            criteria.push(format!("exit code: {}", code));
        }
        if !test.success_criteria.stdout_contains.is_empty() {
            criteria.push(format!(
                "stdout contains: {}",
                test.success_criteria.stdout_contains.join(", ")
            ));
        }
        if !test.success_criteria.stdout_not_contains.is_empty() {
            criteria.push(format!(
                "stdout must NOT contain: {}",
                test.success_criteria.stdout_not_contains.join(", ")
            ));
        }
        if let Some(true) = test.success_criteria.stderr_empty {
            criteria.push("stderr must be empty".to_string());
        }
        if !criteria.is_empty() {
            content.push_str(&format!("  Success: {}\n", criteria.join("; ")));
        }
        content.push('\n');
    }
    content
}

fn format_dead_code_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if let Some(dead_code) = &stage.dead_code_check {
        content.push_str("### Dead Code Check\n\n");
        content.push_str(&format!("**Build command:** `{}`\n", dead_code.command));
        if !dead_code.fail_patterns.is_empty() {
            content.push_str(&format!(
                "  Fail patterns: {}\n",
                dead_code.fail_patterns.join(", ")
            ));
        }
        if !dead_code.ignore_patterns.is_empty() {
            content.push_str(&format!(
                "  Ignore patterns: {}\n",
                dead_code.ignore_patterns.join(", ")
            ));
        }
        content.push('\n');
    }
    content
}

fn format_regression_test_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if let Some(regression_test) = &stage.regression_test {
        content.push_str("### Regression Test\n\n");
        content.push_str("The test file must exist and contain the listed patterns.\n\n");
        content.push_str(&format!("**File:** `{}`\n", regression_test.file));
        if !regression_test.must_contain.is_empty() {
            content.push_str(&format!(
                "  Must contain: {}\n",
                regression_test.must_contain.join(", ")
            ));
        }
        content.push('\n');
    }
    content
}

fn format_reachable_subsection(stage: &Stage) -> String {
    let mut content = String::new();
    if stage.reachable.is_empty() {
        return content;
    }
    content.push_str("### Reachable (units the code graph must reach from an entry point)\n\n");
    for check in &stage.reachable {
        content.push_str(&format!(
            "- **{}**: `{}` reachable from `{}`",
            check.description, check.symbol, check.from
        ));
        if let Some(min_confidence) = check.min_confidence {
            content.push_str(&format!(" (min confidence {min_confidence})"));
        }
        content.push('\n');
    }
    content.push('\n');
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::RegressionTest;
    use crate::plan::schema::ReachableCheck;

    #[test]
    fn renders_regression_test_subsection_with_file_and_pattern() {
        let stage = Stage {
            regression_test: Some(RegressionTest {
                file: "tests/policy.rs".to_string(),
                must_contain: vec!["refuses_expired".to_string()],
            }),
            ..Default::default()
        };

        let rendered = format_goal_backward_verification_section(&stage);

        assert!(rendered.contains("### Regression Test"));
        assert!(rendered.contains("tests/policy.rs"));
        assert!(rendered.contains("refuses_expired"));
    }

    #[test]
    fn renders_reachable_subsection_with_the_check_symbol() {
        let stage = Stage {
            reachable: vec![ReachableCheck {
                symbol: "crate::verify::goal_backward::reachable_gaps".to_string(),
                from: "crate::verify::goal_backward::run_goal_backward_verification".to_string(),
                min_confidence: None,
                description: "reachable_gaps runs from run_goal_backward_verification".to_string(),
            }],
            ..Default::default()
        };

        let rendered = format_goal_backward_verification_section(&stage);

        assert!(rendered.contains("### Reachable"));
        assert!(rendered.contains("crate::verify::goal_backward::reachable_gaps"));
    }
}
