//! Aggregated wiring re-verification: an integration-verify stage re-runs
//! every completed stage's wiring checks on its merged tree.

use anyhow::{bail, Result};
use std::path::Path;

use super::{load_parsed_plan, OutputStream, VerificationChecks};
use crate::models::dispute::CriterionField;
use crate::models::stage::StageStatus;
use crate::plan::schema::StageDefinition;
use crate::verify::goal_backward::{indexed_gaps, VerificationGap};

/// Re-verify the wiring of every completed stage. Every stage of one plan
/// shares `plan_version`, so the finishing stage's version applies to all.
pub(super) fn aggregated_wiring(
    checks: &VerificationChecks<'_>,
    worktree_root: &Path,
) -> Result<()> {
    let mut all_gaps = Vec::new();
    for definition in completed_definitions(checks.work_dir)? {
        all_gaps.extend(stage_wiring_gaps(checks, &definition, worktree_root)?);
    }
    if !all_gaps.is_empty() {
        bail!(
            "Aggregated wiring re-verification failed with {} issue(s)",
            all_gaps.len()
        );
    }
    (checks.emit)(
        OutputStream::Stdout,
        "Aggregated wiring re-verification passed!",
    );
    Ok(())
}

/// The plan definition of every `Completed` stage, in stage-list order.
pub(crate) fn completed_definitions(work_dir: &Path) -> Result<Vec<StageDefinition>> {
    let stages = crate::verify::transitions::list_all_stages(work_dir)?;
    let Some(plan) = load_parsed_plan(work_dir)? else {
        bail!("Could not load plan for aggregated re-verification");
    };
    let definitions = &plan.metadata.loom.stages;
    Ok(stages
        .iter()
        .filter(|stage| stage.status == StageStatus::Completed)
        .filter_map(|stage| definitions.iter().find(|item| item.id == stage.id).cloned())
        .collect())
}

fn stage_wiring_gaps(
    checks: &VerificationChecks<'_>,
    definition: &StageDefinition,
    worktree_root: &Path,
) -> Result<Vec<VerificationGap>> {
    if definition.wiring.is_empty() {
        return Ok(Vec::new());
    }
    let stage_id = &definition.id;
    (checks.emit)(
        OutputStream::Stdout,
        &format!("  Re-verifying wiring from stage '{stage_id}'..."),
    );
    let working_dir = if definition.working_dir == "." {
        worktree_root.to_path_buf()
    } else {
        worktree_root.join(&definition.working_dir)
    };
    let mut gaps = Vec::new();
    for (index, check) in definition.wiring.iter().enumerate() {
        let found = crate::verify::goal_backward::verify_wiring(
            std::slice::from_ref(check),
            &working_dir,
            checks.stage.plan_version,
        )?;
        gaps.extend(indexed_gaps(CriterionField::Wiring, index, found));
    }
    for gap in &gaps {
        (checks.emit)(
            OutputStream::Stderr,
            &format!("    ✗ {stage_id}: {}", gap.description),
        );
    }
    if !gaps.is_empty() {
        // The index names THIS stage's wiring list, which the integration-verify
        // stage's own `--field wiring` index does not reach.
        (checks.emit)(
            OutputStream::Stderr,
            &format!(
                "    A check no correct work could pass: loom stage dispute-criteria {} \
                 --field wiring --owner-stage {stage_id} --criterion-index <n> --reason \"<why>\"",
                checks.stage.id
            ),
        );
    }
    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageType, WiringCheck};
    use std::cell::RefCell;

    thread_local! {
        static STDERR: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn capture(stream: OutputStream, line: &str) {
        if matches!(stream, OutputStream::Stderr) {
            STDERR.with(|lines| lines.borrow_mut().push(line.to_string()));
        }
    }

    fn check(pattern: &str) -> WiringCheck {
        WiringCheck {
            source: "src/lib.rs".to_string(),
            pattern: pattern.to_string(),
            description: format!("{pattern} is wired"),
            literal: false,
        }
    }

    /// Each gap names the index into the OWNER's list, and the failure tells
    /// the integration-verify stage how to dispute exactly that entry.
    #[test]
    fn a_gap_names_the_owners_index_and_the_owner_stage_dispute() {
        let tree = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tree.path().join("src")).unwrap();
        std::fs::write(tree.path().join("src/lib.rs"), "fn present() {}\n").unwrap();
        let iv = Stage {
            id: "iv".to_string(),
            stage_type: StageType::IntegrationVerify,
            ..Stage::default()
        };
        let checks = VerificationChecks {
            stage: &iv,
            stage_id: "iv",
            acceptance_dir: None,
            worktree_root: Some(tree.path()),
            daemon_gates: false,
            fail_closed: false,
            expected_policy: None,
            emit: capture,
            work_dir: tree.path(),
        };
        let definition = StageDefinition {
            id: "up".to_string(),
            working_dir: ".".to_string(),
            wiring: vec![check("present"), check("absent_symbol")],
            ..StageDefinition::default()
        };

        let gaps = stage_wiring_gaps(&checks, &definition, tree.path()).unwrap();

        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(
            gaps[0].description.starts_with("[wiring 1] "),
            "{}",
            gaps[0].description
        );
        let stderr = STDERR.with(|lines| lines.borrow().join("\n"));
        assert!(stderr.contains("✗ up: [wiring 1] "), "{stderr}");
        assert!(
            stderr.contains(
                "loom stage dispute-criteria iv --field wiring --owner-stage up \
                 --criterion-index <n>"
            ),
            "{stderr}"
        );
    }
}
