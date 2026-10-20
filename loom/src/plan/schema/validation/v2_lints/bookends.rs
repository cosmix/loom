//! A version-2 plan needs one knowledge-distill stage after every other stage,
//! and each standard stage needs a later integration-verify stage. These are
//! errors in version 2 and warnings in version 1.

use std::collections::HashMap;

use crate::plan::schema::detect_stage_type;
use crate::plan::schema::structural_checks::transitive_dependencies;
use crate::plan::schema::StageType;

use super::{LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let stages = &ctx.metadata.loom.stages;
    if stages.is_empty() {
        return;
    }

    let stage_types: Vec<_> = stages.iter().map(detect_stage_type).collect();
    let index_by_id: HashMap<&str, usize> = stages
        .iter()
        .enumerate()
        .map(|(idx, stage)| (stage.id.as_str(), idx))
        .collect();

    check_distill(stages, &stage_types, &index_by_id, out);
    check_verify(stages, &stage_types, &index_by_id, out);
}

fn check_distill(
    stages: &[crate::plan::schema::StageDefinition],
    stage_types: &[StageType],
    index_by_id: &HashMap<&str, usize>,
    out: &mut Vec<LintFinding>,
) {
    let distills: Vec<_> = stage_types
        .iter()
        .enumerate()
        .filter_map(|(idx, kind)| (*kind == StageType::KnowledgeDistill).then_some(idx))
        .collect();
    if distills.len() != 1 {
        let count = distills.len();
        out.push(LintFinding {
            stage_id: None,
            message: format!(
                "the plan has {count} knowledge-distill stages; a plan needs exactly one, after every other stage, so the memory of every stage is curated once"
            ),
            error_in_v2: true,
        });
        return;
    }
    let distill = distills[0];
    let dependents: Vec<_> = stages
        .iter()
        .filter(|stage| stage.dependencies.contains(&stages[distill].id))
        .map(|stage| stage.id.as_str())
        .collect();
    if !dependents.is_empty() {
        let message = format!(
            "knowledge-distill stage has dependents ({}); it must be the last stage of the plan",
            dependents.join(", ")
        );
        out.push(LintFinding::in_stage(&stages[distill], message, true));
    }
    let ancestors = transitive_dependencies(distill, stages, index_by_id);
    for (idx, stage) in stages.iter().enumerate() {
        if idx != distill && !ancestors.contains(&idx) {
            out.push(LintFinding::in_stage(
                stage,
                format!(
                    "is not an ancestor of knowledge-distill stage `{}`, so its memory is never distilled",
                    stages[distill].id
                ),
                true,
            ));
        }
    }
}

fn check_verify(
    stages: &[crate::plan::schema::StageDefinition],
    stage_types: &[StageType],
    index_by_id: &HashMap<&str, usize>,
    out: &mut Vec<LintFinding>,
) {
    let verify_ancestors: Vec<_> = stage_types
        .iter()
        .enumerate()
        .filter(|(_, kind)| **kind == StageType::IntegrationVerify)
        .map(|(idx, _)| transitive_dependencies(idx, stages, index_by_id))
        .collect();

    for (idx, stage) in stages.iter().enumerate() {
        if stage_types[idx] == StageType::Standard
            && verify_ancestors
                .iter()
                .all(|ancestors| !ancestors.contains(&idx))
        {
            out.push(LintFinding::in_stage(
                stage,
                "is not an ancestor of any integration-verify stage, so its merged code is never verified"
                    .to_string(),
                true,
            ));
        }
    }
}
