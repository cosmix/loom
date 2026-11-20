//! The integration-verify signal's "Handed-Off Tests" section: every test an
//! adjudicator left out of a stage's check, where it runs instead, and every
//! amendment of the plan.

use std::path::Path;

use anyhow::Result;

use crate::models::stage::{Stage, StageType};
use crate::plan::amendment::read_audit_rows;
use crate::utils::text::single_line;
use crate::verify::review::handoffs::{load_handoffs, Handoff};
use crate::verify::transitions::list_all_stages;

const HEADING: &str = "## Handed-Off Tests";

const INSTRUCTION: &str = "Each test below was left out of a stage's check by an adjudicator. \
Your full-suite criterion runs the ones that can run here: a failure among them is yours to fix \
or dispute. For each one that cannot run here, record where it runs with loom memory.";

/// One amendment of the plan, as the audit journal recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendmentLine {
    pub stage: String,
    pub field: String,
    pub op: String,
    pub index: usize,
    pub reason: String,
}

/// `<stage> <field>[<index>]: <tests> -> <runs_at>: <reason>`, with
/// ` (unverified)` when the base run did not count the failing tests.
pub(crate) fn handoff_line(handoff: &Handoff) -> String {
    let unverified = if handoff.verified {
        ""
    } else {
        " (unverified)"
    };
    format!(
        "{} {}[{}]: {} -> {}: {}{unverified}",
        single_line(&handoff.stage),
        single_line(&handoff.field),
        handoff.index,
        handoff
            .tests
            .iter()
            .map(|test| single_line(test))
            .collect::<Vec<_>>()
            .join(", "),
        single_line(&handoff.runs_at),
        single_line(&handoff.reason),
    )
}

fn amendment_line(amendment: &AmendmentLine) -> String {
    format!(
        "{} {} {} [{}]: {}",
        single_line(&amendment.stage),
        single_line(&amendment.field),
        single_line(&amendment.op),
        amendment.index,
        single_line(&amendment.reason),
    )
}

/// Append the section; nothing when there is nothing to list.
pub fn append_handoffs(content: &mut String, handoffs: &[Handoff], amendments: &[AmendmentLine]) {
    if handoffs.is_empty() && amendments.is_empty() {
        return;
    }
    content.push_str(&format!("\n{HEADING}\n\n{INSTRUCTION}\n\n"));
    for handoff in handoffs {
        content.push_str(&format!("- {}\n", handoff_line(handoff)));
    }
    if !amendments.is_empty() {
        content.push_str("\n### Amendments\n\n");
        for amendment in amendments {
            content.push_str(&format!("- {}\n", amendment_line(amendment)));
        }
    }
}

/// Every hand-off recorded for the stages `stage_ids`, in the order given.
pub(crate) fn stage_handoffs<'a>(
    work_dir: &Path,
    stage_ids: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<Handoff>> {
    let mut handoffs = Vec::new();
    for stage_id in stage_ids {
        handoffs.extend(load_handoffs(work_dir, stage_id)?);
    }
    Ok(handoffs)
}

/// Every hand-off recorded for any stage of the plan.
fn plan_handoffs(work_dir: &Path) -> Result<Vec<Handoff>> {
    let mut ids: Vec<String> = list_all_stages(work_dir)?
        .into_iter()
        .map(|stage| stage.id)
        .collect();
    ids.sort();
    stage_handoffs(work_dir, ids.iter().map(String::as_str))
}

/// Every amendment of the plan, oldest first.
pub(crate) fn plan_amendments(work_dir: &Path) -> Result<Vec<AmendmentLine>> {
    Ok(read_audit_rows(work_dir)?
        .into_iter()
        .map(|row| AmendmentLine {
            stage: row.stage_id.clone(),
            field: row.field_name().to_string(),
            op: row.patch_op.to_string(),
            index: row.index,
            reason: row.reason.unwrap_or_default(),
        })
        .collect())
}

/// Integration-verify only: the plan's hand-offs and amendments, or a note
/// that loom could not read them.
pub(super) fn append_plan_handoffs(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.stage_type != StageType::IntegrationVerify {
        return;
    }
    match plan_handoffs(work_dir).and_then(|h| Ok((h, plan_amendments(work_dir)?))) {
        Ok((handoffs, amendments)) => append_handoffs(content, &handoffs, &amendments),
        Err(error) => content.push_str(&format!(
            "\n{HEADING}\n\nLoom could not read the plan's recorded hand-offs and amendments \
             ({}).\n",
            single_line(&format!("{error:#}"))
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::signals::v2_section::append_v2_section;
    use crate::verify::review::handoffs::record_handoffs;
    use crate::verify::transitions::save_stage;

    fn stage_of(id: &str, stage_type: StageType) -> Stage {
        Stage {
            id: id.to_string(),
            plan_version: 2,
            stage_type,
            ..Stage::default()
        }
    }

    fn render(stage: &Stage, work_dir: &Path) -> String {
        let mut content = String::new();
        append_v2_section(&mut content, stage, work_dir);
        content
    }

    #[test]
    fn only_an_integration_verify_signal_lists_a_recorded_hand_off() {
        let work = tempfile::tempdir().unwrap();
        let standard = stage_of("alpha", StageType::Standard);
        let verify = stage_of("verify", StageType::IntegrationVerify);
        save_stage(&standard, work.path()).unwrap();
        save_stage(&verify, work.path()).unwrap();
        let handoff = Handoff {
            stage: "alpha".to_string(),
            field: "acceptance".to_string(),
            index: 1,
            tests: vec!["needs_network_fetch".to_string()],
            runs_at: "ci".to_string(),
            reason: "no network in the worktree".to_string(),
            dispute: "dispute-1".to_string(),
            verified: true,
        };
        record_handoffs(work.path(), "alpha", &[handoff]).unwrap();

        let listed = render(&verify, work.path());
        assert!(listed.contains(HEADING), "{listed}");
        assert!(
            listed.contains("alpha acceptance[1]: needs_network_fetch -> ci: no network"),
            "{listed}"
        );
        let standard_signal = render(&standard, work.path());
        assert!(!standard_signal.contains(HEADING), "{standard_signal}");
        assert!(
            !standard_signal.contains("needs_network_fetch"),
            "{standard_signal}"
        );
    }

    #[test]
    fn a_plan_without_hand_offs_adds_no_section() {
        let work = tempfile::tempdir().unwrap();
        let verify = stage_of("verify", StageType::IntegrationVerify);
        save_stage(&verify, work.path()).unwrap();
        assert!(!render(&verify, work.path()).contains(HEADING));
    }
}
